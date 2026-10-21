//! Demonstrates the high-level [`Pool`], which keeps a set of read-only
//! connections for concurrent reads and a single write connection for
//! serialized writes.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example pool
//! ```

use std::sync::Arc;

use anyhow::Result;
use sqll::{OpenOptions, PoolBuilder, Statements, TypedStatement};
use tokio::task;

/// Statements used for read-only access.
///
/// Marked `#[sql(read_only)]` so the pool is allowed to use several of these
/// connections concurrently. Building the pool fails if any of these statements
/// turn out to mutate the database.
#[derive(Statements)]
#[sql(read_only)]
struct Read {
    #[sql = "SELECT name, age FROM users ORDER BY age"]
    all_users: TypedStatement<(), (String, i64)>,
}

/// Statements used for exclusive write access.
#[derive(Statements)]
struct Write {
    #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    insert_user: TypedStatement<(String, i64), ()>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // The pool needs a filesystem database so SQLite can coordinate access
    // between the separate read and write connections.
    let dir = tempfile::TempDir::new()?;
    let path = dir.path().join("pool.db");

    // The statements in `Read` reference the `users` table, and the pool
    // prepares every statement up front, so the schema has to exist before the
    // pool is constructed.
    {
        let c = OpenOptions::new()
            .create()
            .read_write()
            .no_mutex()
            .open(&path)?;

        c.execute("CREATE TABLE users (name TEXT PRIMARY KEY NOT NULL, age INTEGER)")?;
    }

    let mut options = OpenOptions::new();
    options.no_mutex().create();

    let pool = Arc::new(PoolBuilder::new(options, 4).open::<Read, Write>(&path)?);

    // Insert a few rows under an exclusive lock. While this guard is held no
    // other task can acquire a shared or exclusive lock. The actual statement
    // calls are performed on a blocking thread, since SQLite is blocking.
    {
        let guard = pool.clone().exclusive().await?;

        task::spawn_blocking(move || -> Result<()> {
            let mut guard = guard;

            for (name, age) in [("Alice", 42), ("Bob", 52), ("Charlie", 20)] {
                guard.insert_user.execute((name, age))?;
            }

            Ok(())
        })
        .await??;
    }

    // Read concurrently from several tasks. Each task acquires its own shared
    // guard backed by one of the pool's read connections.
    let mut tasks = Vec::new();

    for _ in 0..4 {
        let pool = pool.clone();

        tasks.push(task::spawn(async move {
            let mut guard = pool.shared().await?;

            task::spawn_blocking(move || -> Result<Vec<(String, i64)>> {
                let rows = guard.all_users.query()?.collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await?
        }));
    }

    for t in tasks {
        let rows = t.await??;
        println!("{rows:?}");
    }

    Ok(())
}
