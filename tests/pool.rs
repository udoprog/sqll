//! Concurrent use of [`Pool::shared`] and [`Pool::exclusive`] from many tasks.

#![cfg(feature = "pool")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::Result;
use sqll::{OpenOptions, Pool, PoolBuilder, Statements, TypedStatement};
use tempfile::TempDir;
use tokio::sync::Barrier;
use tokio::task;

#[derive(Statements)]
#[sql(read_only)]
struct Read {
    #[sql = "SELECT n FROM counter"]
    get: TypedStatement<(), i64>,
}

#[derive(Statements)]
struct Write {
    #[sql = "UPDATE counter SET n = n + 1"]
    increment: TypedStatement<(), ()>,
}

fn setup(concurrency: usize) -> Result<(TempDir, Arc<Pool<Read, Write>>)> {
    let dir = TempDir::new()?;
    let path = dir.path().join("pool.db");

    {
        let c = OpenOptions::new().create().read_write().open(&path)?;
        c.execute("CREATE TABLE counter (n INTEGER NOT NULL); INSERT INTO counter VALUES (0);")?;
    }

    let mut options = OpenOptions::new();
    options.create();

    let pool = PoolBuilder::new(options, concurrency).open::<Read, Write>(&path)?;
    Ok((dir, Arc::new(pool)))
}

fn read(guard: &mut Read) -> Result<i64> {
    let mut stmt = guard.get.query()?;
    let n = stmt.next()?.expect("a row");
    assert!(stmt.next()?.is_none());
    Ok(n)
}

/// Every read connection can be held by a shared guard at the same time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_guards_are_concurrent() -> Result<()> {
    const CONCURRENCY: usize = 4;

    let (_dir, pool) = setup(CONCURRENCY)?;
    let barrier = Arc::new(Barrier::new(CONCURRENCY));
    let mut tasks = Vec::new();

    for _ in 0..CONCURRENCY {
        let pool = pool.clone();
        let barrier = barrier.clone();

        tasks.push(task::spawn(async move {
            let mut guard = pool.shared().await?;
            // Only completes once all shared guards are held at once.
            barrier.wait().await;

            task::spawn_blocking(move || read(&mut guard)).await?
        }));
    }

    for t in tasks {
        assert_eq!(t.await??, 0);
    }

    Ok(())
}

/// Shared and exclusive guards taken from many tasks never overlap, and every
/// read connection is reused correctly once its guard is dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_and_exclusive_guards() -> Result<()> {
    const TASKS: usize = 16;
    const ROUNDS: usize = 25;

    // Fewer read connections than tasks so guards contend and slots are
    // reused.
    let (_dir, pool) = setup(2)?;

    let readers = Arc::new(AtomicUsize::new(0));
    let writer = Arc::new(AtomicBool::new(false));
    let mut tasks = Vec::new();

    for index in 0..TASKS {
        let pool = pool.clone();
        let readers = readers.clone();
        let writer = writer.clone();

        tasks.push(task::spawn(async move {
            let mut writes = 0usize;

            for round in 0..ROUNDS {
                if (index + round) % 4 == 0 {
                    let mut guard = pool.clone().exclusive().await?;
                    assert!(!writer.swap(true, Ordering::SeqCst));
                    assert_eq!(readers.load(Ordering::SeqCst), 0);

                    guard = task::spawn_blocking(move || -> Result<_> {
                        guard.increment.execute(())?;
                        Ok(guard)
                    })
                    .await??;

                    writer.store(false, Ordering::SeqCst);
                    drop(guard);
                    writes += 1;
                } else {
                    let mut guard = pool.clone().shared().await?;
                    readers.fetch_add(1, Ordering::SeqCst);
                    assert!(!writer.load(Ordering::SeqCst));

                    guard = task::spawn_blocking(move || -> Result<_> {
                        read(&mut guard)?;
                        Ok(guard)
                    })
                    .await??;

                    assert!(!writer.load(Ordering::SeqCst));
                    readers.fetch_sub(1, Ordering::SeqCst);
                    drop(guard);
                }

                task::yield_now().await;
            }

            Ok::<_, anyhow::Error>(writes)
        }));
    }

    let mut writes = 0;

    for t in tasks {
        writes += t.await??;
    }

    assert_eq!(writes, TASKS * ROUNDS / 4);

    let mut guard = pool.clone().shared().await?;
    assert_eq!(read(&mut guard)? as usize, writes);
    Ok(())
}
