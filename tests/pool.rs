//! Concurrent use of [`Pool::shared`], [`Pool::exclusive`],
//! [`Pool::exclusive_all`], [`Pool::read`] and [`Pool::write`] from many tasks.
//!
//! Every wait on async work is bounded, so that a test fails rather than hangs
//! if a guard is never released.

#![cfg(feature = "pool")]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier as SyncBarrier};
use std::time::Duration;

use anyhow::Result;
use sqll::{
    JournalMode, OpenOptions, Pool, PoolBuilder, Prepared, SendStatement, Statements,
    TypedStatement,
};
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
    setup_with(concurrency, OpenOptions::new())
}

/// A pool in WAL mode, where readers can proceed during a write.
fn setup_wal(concurrency: usize) -> Result<(TempDir, Arc<Pool<Read, Write>>)> {
    let mut options = OpenOptions::new();
    options
        .journal_mode(JournalMode::Wal)
        .busy_timeout(Duration::from_secs(10));
    setup_with(concurrency, options)
}

fn setup_with(
    concurrency: usize,
    mut options: OpenOptions,
) -> Result<(TempDir, Arc<Pool<Read, Write>>)> {
    let dir = TempDir::new()?;
    let path = dir.path().join("pool.db");

    {
        let c = OpenOptions::new().create().read_write().open(&path)?;
        c.execute("CREATE TABLE counter (n INTEGER NOT NULL); INSERT INTO counter VALUES (0);")?;
    }

    options.create();

    let pool = PoolBuilder::new(options, concurrency).open::<Read, Write>(&path)?;
    Ok((dir, Arc::new(pool)))
}

/// Fail rather than hang if acquiring a guard waits for one that is held.
async fn within<F>(f: F) -> Result<F::Output>
where
    F: Future,
{
    Ok(tokio::time::timeout(Duration::from_secs(10), f).await?)
}

fn read(guard: &mut Read) -> Result<i64> {
    let mut stmt = guard.get.query()?;
    let n = stmt.next().transpose()?.expect("a row");
    assert!(stmt.next().is_none());
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
            let mut guard = within(pool.shared()).await??;
            // Only completes once all shared guards are held at once.
            barrier.wait().await;

            task::spawn_blocking(move || read(&mut guard)).await?
        }));
    }

    for t in tasks {
        assert_eq!(within(t).await???, 0);
    }

    Ok(())
}

/// A writer and every reader hold their guards, and use them, at the same
/// time: the exclusive guard neither waits for the shared guards nor blocks
/// them. In WAL mode the readers see the database as it was before the
/// writer's open transaction until it commits.
#[test]
fn writer_and_readers_hold_guards_at_once() -> Result<()> {
    const CONCURRENCY: usize = 4;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    let (_dir, pool) = setup_wal(CONCURRENCY)?;

    // Take the exclusive guard and then every shared guard, so that they are
    // all held at once. Each acquisition would wait forever if it had to wait
    // for another guard to be dropped, so bound it.
    let (mut write, reads) = rt.block_on(async {
        let write = within(pool.exclusive()).await??;
        let mut reads = Vec::new();

        for _ in 0..CONCURRENCY {
            reads.push(within(pool.shared()).await??);
        }

        Ok::<_, anyhow::Error>((write, reads))
    })?;

    // Then use them from one thread each, in lockstep. The barrier counts the
    // writer and every reader, so each step only completes once all of them
    // reach it with their guard still held.
    let barrier = Arc::new(SyncBarrier::new(CONCURRENCY + 1));

    let writer = {
        let barrier = barrier.clone();

        std::thread::spawn(move || -> Result<()> {
            let (statements, c) = write.split();
            barrier.wait(); // 1: all guards are held.

            c.execute("BEGIN")?;
            statements.increment.execute(())?;
            barrier.wait(); // 2: the write is in flight.
            barrier.wait(); // 3: readers have read during the write.

            c.execute("COMMIT")?;
            barrier.wait(); // 4: the write is committed.
            barrier.wait(); // 5: readers have read after the write.
            drop(write);
            Ok(())
        })
    };

    let mut readers = Vec::new();

    for mut guard in reads {
        let barrier = barrier.clone();

        readers.push(std::thread::spawn(move || -> Result<()> {
            barrier.wait(); // 1
            barrier.wait(); // 2
            assert_eq!(read(&mut guard)?, 0);
            barrier.wait(); // 3
            barrier.wait(); // 4
            assert_eq!(read(&mut guard)?, 1);
            barrier.wait(); // 5
            Ok(())
        }));
    }

    writer.join().expect("writer panicked")?;

    for t in readers {
        t.join().expect("reader panicked")?;
    }

    Ok(())
}

/// A task holding a shared guard can take the exclusive guard, and a task
/// holding the exclusive guard can take a shared guard, without waiting for
/// itself.
#[tokio::test]
async fn guards_nest_in_either_order() -> Result<()> {
    let (_dir, pool) = setup_wal(1)?;

    let mut r = within(pool.shared()).await??;
    let mut w = within(pool.exclusive()).await??;
    w.increment.execute(())?;
    assert_eq!(read(&mut r)?, 1);
    drop(r);

    let mut r = within(pool.shared()).await??;
    w.increment.execute(())?;
    assert_eq!(read(&mut r)?, 2);
    Ok(())
}

/// Exclusive guards exclude each other, and `exclusive_all` also excludes
/// shared guards and waits for those already held.
#[tokio::test]
async fn exclusive_guards_exclude() -> Result<()> {
    let (_dir, pool) = setup_wal(2)?;
    let wait = Duration::from_millis(50);

    let write = within(pool.exclusive()).await??;
    assert!(tokio::time::timeout(wait, pool.exclusive()).await.is_err());
    assert!(
        tokio::time::timeout(wait, pool.exclusive_all())
            .await
            .is_err()
    );
    drop(write);

    let read = within(pool.shared()).await??;
    // Waits for `read` to be dropped.
    assert!(
        tokio::time::timeout(wait, pool.exclusive_all())
            .await
            .is_err()
    );
    drop(read);

    let all = within(pool.exclusive_all()).await??;
    assert!(tokio::time::timeout(wait, pool.shared()).await.is_err());
    assert!(tokio::time::timeout(wait, pool.exclusive()).await.is_err());
    drop(all);

    // Cancelled acquisitions above released whatever permits they had taken.
    let _read = within(pool.shared()).await??;
    let _read = within(pool.shared()).await??;
    let _write = within(pool.exclusive()).await??;
    Ok(())
}

/// Shared guards and `exclusive_all` guards taken from many tasks never
/// overlap, and every read connection is reused correctly once its guard is
/// dropped. This holds without WAL, since no reader runs during a write.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_and_exclusive_all_guards() -> Result<()> {
    // Fewer read connections than tasks so guards contend and slots are
    // reused.
    let (_dir, pool) = setup(2)?;
    contend(pool, true).await
}

/// Exclusive guards taken from many tasks never overlap each other, while
/// shared guards run alongside them, and every write lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_and_exclusive_guards() -> Result<()> {
    let (_dir, pool) = setup_wal(2)?;
    contend(pool, false).await
}

/// Take shared and exclusive guards from many tasks at once. With `all`,
/// writers use `exclusive_all` and no reader may overlap a writer.
async fn contend(pool: Arc<Pool<Read, Write>>, all: bool) -> Result<()> {
    const TASKS: usize = 16;
    const ROUNDS: usize = 25;

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
                    let mut guard = if all {
                        within(pool.exclusive_all()).await??
                    } else {
                        within(pool.exclusive()).await??
                    };

                    assert!(!writer.swap(true, Ordering::SeqCst));

                    if all {
                        assert_eq!(readers.load(Ordering::SeqCst), 0);
                    }

                    guard = task::spawn_blocking(move || -> Result<_> {
                        guard.increment.execute(())?;
                        Ok(guard)
                    })
                    .await??;

                    writer.store(false, Ordering::SeqCst);
                    drop(guard);
                    writes += 1;
                } else {
                    let mut guard = within(pool.shared()).await??;
                    readers.fetch_add(1, Ordering::SeqCst);
                    assert!(!all || !writer.load(Ordering::SeqCst));

                    guard = task::spawn_blocking(move || -> Result<_> {
                        read(&mut guard)?;
                        Ok(guard)
                    })
                    .await??;

                    assert!(!all || !writer.load(Ordering::SeqCst));
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
        writes += within(t).await???;
    }

    assert_eq!(writes, TASKS * ROUNDS / 4);

    let mut guard = within(pool.shared()).await??;
    assert_eq!(read(&mut guard)? as usize, writes);
    Ok(())
}

#[derive(Statements)]
#[sql(read_only)]
struct PeopleRead {
    #[sql = "SELECT id FROM people ORDER BY id LIMIT ?"]
    limit: SendStatement,
    #[sql = "SELECT id FROM people WHERE id LIKE ? ORDER BY id"]
    like: SendStatement,
}

#[derive(Statements)]
struct PeopleWrite {
    #[sql = "SELECT id FROM people ORDER BY id LIMIT ?"]
    limit: SendStatement,
    #[sql = "SELECT id FROM people WHERE id LIKE ? ORDER BY id"]
    like: SendStatement,
}

fn collect(stmt: &mut SendStatement, value: impl sqll::Bind) -> Result<Vec<String>> {
    stmt.reset()?;
    stmt.bind(value)?;

    let mut out = Vec::new();

    while let Some(id) = stmt.next::<String>()? {
        out.push(id);
    }

    Ok(out)
}

/// Pooled statements stay usable after the pool is built.
///
/// SQLite re-prepares a statement whose plan depends on a bound value, such as
/// a bare `LIMIT ?` or `LIKE ?`. That fails with `SQLITE_MISUSE` if the
/// connection the statement was prepared on has been closed, so it must stay
/// open for as long as its statements, which they ensure by holding a reference
/// to the connection handle.
#[tokio::test]
async fn pooled_statements_rebind_limit_and_like() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("people.db");

    {
        let c = OpenOptions::new().create().read_write().open(&path)?;
        c.execute(
            "CREATE TABLE people (id TEXT NOT NULL);
             INSERT INTO people VALUES ('alice'), ('alex'), ('bob'), ('carol');",
        )?;
    }

    let pool = PoolBuilder::new(OpenOptions::new(), 2).open::<PeopleRead, PeopleWrite>(&path)?;
    let pool = Arc::new(pool);

    for _ in 0..2 {
        let mut read = within(pool.shared()).await??;

        assert_eq!(collect(&mut read.limit, 2i64)?, ["alex", "alice"]);
        assert_eq!(collect(&mut read.limit, 3i64)?, ["alex", "alice", "bob"]);

        assert_eq!(collect(&mut read.like, "al%")?, ["alex", "alice"]);
        assert_eq!(collect(&mut read.like, "c%")?, ["carol"]);
    }

    let mut write = within(pool.exclusive()).await??;

    assert_eq!(collect(&mut write.limit, 1i64)?, ["alex"]);

    assert_eq!(collect(&mut write.like, "b%")?, ["bob"]);
    Ok(())
}

/// A [`Prepared`] collection taken apart, with its connection dropped before
/// statements that have to re-prepare on it.
#[test]
fn prepared_statements_outlive_their_connection() -> Result<()> {
    let c = OpenOptions::new().create().read_write().open_in_memory()?;
    c.execute(
        "CREATE TABLE people (id TEXT NOT NULL);
         INSERT INTO people VALUES ('alice'), ('alex'), ('bob'), ('carol');",
    )?;

    let (mut people, c) = Prepared::<PeopleWrite>::new(c)?.into_parts();
    drop(c);

    assert_eq!(collect(&mut people.limit, 2i64)?, ["alex", "alice"]);
    assert_eq!(collect(&mut people.limit, 3i64)?, ["alex", "alice", "bob"]);
    assert_eq!(collect(&mut people.like, "al%")?, ["alex", "alice"]);
    assert_eq!(collect(&mut people.like, "c%")?, ["carol"]);
    Ok(())
}

/// Guards lend out the connection their statements were prepared on: the read
/// side is read-only, and the write side reports changes from its statements.
#[tokio::test]
async fn guards_expose_their_connection() -> Result<()> {
    let (_dir, pool) = setup(2)?;

    {
        let mut guard = within(pool.exclusive()).await??;
        guard.increment.execute(())?;
        assert_eq!(guard.connection().changes(), 1);

        let (write, c) = guard.split();
        c.execute("INSERT INTO counter VALUES (10)")?;
        assert_eq!(c.last_insert_rowid(), 2);
        write.increment.execute(())?;
        assert_eq!(c.changes(), 2);
    }

    let mut guard = within(pool.shared()).await??;
    assert!(guard.connection().execute("DELETE FROM counter").is_err());

    let mut stmt = guard
        .connection()
        .prepare(format!("SELECT n FROM counter ORDER BY n {}", "DESC"))?;
    stmt.bind(())?;
    assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [11, 2]);
    drop(stmt);

    let (read, c) = guard.split();
    assert!(c.database_read_only(c"main")?);
    assert_eq!(read.get.query()?.first()?, Some(2));
    Ok(())
}

/// A write-side transaction commits on `Ok` and rolls back on `Err` and on
/// panic, leaving the guard's connection usable.
#[tokio::test]
async fn exclusive_transaction() -> Result<()> {
    let (_dir, pool) = setup(1)?;

    let mut guard = within(pool.exclusive()).await??;

    guard.transaction(|write, _| {
        write.increment.execute(())?;
        Ok::<_, anyhow::Error>(())
    })?;

    let result = guard.transaction(|write, c| {
        write.increment.execute(())?;
        c.execute("INSERT INTO missing VALUES (1)")
    });
    assert!(result.is_err());

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        guard.transaction(|write, _| -> Result<()> {
            write.increment.execute(())?;
            panic!("boom");
        })
    }));
    assert!(result.is_err());

    guard.transaction_with(sqll::TransactionBehavior::Deferred, |write, _| {
        write.increment.execute(())
    })?;
    drop(guard);

    let mut guard = within(pool.shared()).await??;
    assert_eq!(read(&mut guard)?, 2);
    Ok(())
}

/// `read` and `write` run their closure with the guard, which reaches both the
/// statements and the connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn helpers_reach_statements_and_connection() -> Result<()> {
    let (_dir, pool) = setup(2)?;

    let changes = within(pool.write(|w| -> Result<_> {
        w.increment.execute(())?;
        Ok(w.connection().changes())
    }))
    .await???;
    assert_eq!(changes, 1);

    let rowid = within(pool.write(|w| {
        w.transaction(|_, c| {
            c.execute("INSERT INTO counter VALUES (10)")?;
            Ok::<_, sqll::Error>(c.last_insert_rowid())
        })
    }))
    .await???;
    assert_eq!(rowid, 2);

    let rows = within(pool.read(|r| -> Result<Vec<i64>> {
        let (_, c) = r.split();
        let mut stmt = c.prepare("SELECT n FROM counter ORDER BY n DESC")?;
        stmt.bind(())?;
        Ok(stmt.iter::<i64>().collect::<Result<_, _>>()?)
    }))
    .await???;
    assert_eq!(rows, [10, 1]);

    let read_only = within(pool.read(|r| r.connection().execute("DELETE FROM counter"))).await??;
    assert!(read_only.is_err());
    Ok(())
}

/// The closure runs on a blocking thread, not on the runtime's worker.
#[tokio::test]
async fn helpers_run_on_a_blocking_thread() -> Result<()> {
    let (_dir, pool) = setup(1)?;
    let worker = std::thread::current().id();

    let reader = within(pool.read(|_| std::thread::current().id())).await??;
    let writer = within(pool.write(|_| std::thread::current().id())).await??;
    assert_ne!(reader, worker);
    assert_ne!(writer, worker);
    Ok(())
}

/// A panicking closure is returned as an error, and its guard is released so
/// the pool stays usable.
#[tokio::test]
async fn helpers_turn_panics_into_errors() -> Result<()> {
    let (_dir, pool) = setup(1)?;

    let error = within(pool.read(|_| -> i64 { panic!("read boom") }))
        .await?
        .unwrap_err();
    assert_eq!(error.to_string(), "blocking task panicked");
    assert!(core::error::Error::source(&error).is_some());

    let error = within(pool.write(|w| -> usize {
        w.increment.execute(()).unwrap();
        panic!("write boom");
    }))
    .await?
    .unwrap_err();
    assert_eq!(error.to_string(), "blocking task panicked");

    // Both lanes are free again: the single read connection and the writer.
    within(pool.write(|w| w.increment.execute(()))).await???;
    let n = within(pool.read(|r| read(r))).await???;
    assert_eq!(n, 2);
    Ok(())
}

/// A `read` proceeds while the exclusive guard is held, and a `write` waits
/// for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn helpers_follow_the_lanes() -> Result<()> {
    let (_dir, pool) = setup_wal(1)?;
    let wait = Duration::from_millis(50);

    let mut guard = within(pool.exclusive()).await??;
    guard.increment.execute(())?;

    assert_eq!(within(pool.read(|r| read(r))).await???, 1);

    let write = task::spawn({
        let pool = pool.clone();
        async move { pool.write(|w| w.increment.execute(())).await }
    });

    tokio::time::sleep(wait).await;
    assert!(!write.is_finished());
    drop(guard);

    within(write).await????;
    assert_eq!(within(pool.read(|r| read(r))).await???, 2);
    Ok(())
}

/// Many tasks reading and writing through the helpers at once: every write
/// lands, and no two writers overlap.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn helpers_contend() -> Result<()> {
    const TASKS: usize = 16;
    const ROUNDS: usize = 25;

    let (_dir, pool) = setup_wal(2)?;
    let writer = Arc::new(AtomicBool::new(false));
    let mut tasks = Vec::new();

    for index in 0..TASKS {
        let pool = pool.clone();
        let writer = writer.clone();

        tasks.push(task::spawn(async move {
            let mut writes = 0usize;

            for round in 0..ROUNDS {
                if (index + round) % 4 == 0 {
                    let writer = writer.clone();

                    pool.write(move |w| -> Result<()> {
                        assert!(!writer.swap(true, Ordering::SeqCst));
                        w.increment.execute(())?;
                        writer.store(false, Ordering::SeqCst);
                        Ok(())
                    })
                    .await??;

                    writes += 1;
                } else {
                    pool.read(|r| read(r)).await??;
                }
            }

            Ok::<_, anyhow::Error>(writes)
        }));
    }

    let mut writes = 0;

    for t in tasks {
        writes += within(t).await???;
    }

    assert_eq!(writes, TASKS * ROUNDS / 4);
    assert_eq!(within(pool.read(|r| read(r))).await??? as usize, writes);
    Ok(())
}
