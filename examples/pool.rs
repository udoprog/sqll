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
use sqll::{JournalMode, OpenOptions, PoolBuilder, Statements, TypedStatement};
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

    let mut options = OpenOptions::new();
    // WAL lets readers proceed while the write connection is in use.
    options.create().journal_mode(JournalMode::Wal);

    // The pool prepares every statement up front, so the `users` table the
    // statements reference is created by the write setup, which runs before
    // any statement is prepared.
    let pool = PoolBuilder::new(options, 4)
        .with_write_setup(|c| {
            c.execute("CREATE TABLE IF NOT EXISTS users (name TEXT PRIMARY KEY NOT NULL, age INTEGER)")
        })
        .open::<Read, Write>(&path)?;

    let pool = Arc::new(pool);

    // Insert a few rows through the write connection, in one transaction.
    // `write` runs the closure on a blocking thread, since SQLite is blocking.
    // While it runs no other writer can get in, but readers still can.
    pool.write(|w| {
        w.transaction(|w, _| {
            for (name, age) in [("Alice", 42), ("Bob", 52), ("Charlie", 20)] {
                w.insert_user.execute((name, age))?;
            }

            Ok::<_, sqll::Error>(())
        })
    })
    .await??;

    // Read concurrently from several tasks. Each read runs on its own read
    // connection.
    let mut tasks = Vec::new();

    for _ in 0..4 {
        let pool = pool.clone();

        tasks.push(task::spawn(async move {
            let rows = pool
                .read(|r| r.all_users.query()?.collect::<Result<Vec<_>, _>>())
                .await??;
            Ok::<_, anyhow::Error>(rows)
        }));
    }

    for t in tasks {
        let rows = t.await??;
        println!("{rows:?}");
    }

    // SQL composed at runtime is prepared on the guard's connection.
    let min_age = 30;

    let names = pool
        .read(move |r| -> Result<Vec<String>> {
            let mut stmt = r
                .connection()
                .prepare(format!("SELECT name FROM users WHERE age > {min_age}"))?;
            stmt.bind(())?;
            Ok(stmt.iter::<String>().collect::<Result<_, _>>()?)
        })
        .await??;

    println!("{names:?}");
    Ok(())
}
