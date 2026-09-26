//! Regression tests ensuring that a busy timeout configured on a connection
//! keeps applying to statements which outlive the connection.

use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use sqll::{Connection, OpenOptions, PoolBuilder, SendStatement, Statements};

#[derive(Statements)]
#[sql(read_only)]
#[allow(dead_code)]
struct Read {
    #[sql = "SELECT id FROM items"]
    select: SendStatement,
}

#[derive(Statements)]
struct Write {
    #[sql = "INSERT INTO items (id) VALUES (?)"]
    insert: SendStatement,
}

fn setup(path: &Path) -> Result<Connection> {
    let c = Connection::open(path)?;
    c.execute("PRAGMA journal_mode = WAL; CREATE TABLE IF NOT EXISTS items (id INTEGER)")?;
    Ok(c)
}

/// Hold a write lock on `path` for a short while on another thread.
fn hold_write_lock(path: &Path) -> Result<thread::JoinHandle<Result<()>>> {
    let c = Connection::open(path)?;
    c.execute("BEGIN IMMEDIATE")?;

    Ok(thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        c.execute("COMMIT")?;
        Ok(())
    }))
}

#[test]
fn test_busy_timeout_outlives_connection() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let path = temp.path().join("test.db");
    drop(setup(&path)?);

    let mut c = Connection::open(&path)?;
    c.busy_timeout(5000)?;
    let mut insert = c.prepare("INSERT INTO items (id) VALUES (?)")?;
    drop(c);

    let holder = hold_write_lock(&path)?;
    insert.execute(1)?;
    holder.join().unwrap()?;
    Ok(())
}

#[test]
fn test_busy_timeout_replaces_busy_handler() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let path = temp.path().join("test.db");
    drop(setup(&path)?);

    let mut c = Connection::open(&path)?;
    c.busy_handler(|_| false)?;
    c.busy_timeout(5000)?;
    let mut insert = c.prepare("INSERT INTO items (id) VALUES (?)")?;
    drop(c);

    let holder = hold_write_lock(&path)?;
    insert.execute(1)?;
    holder.join().unwrap()?;
    Ok(())
}

#[test]
fn test_pool_busy_timeout() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let path = temp.path().join("test.db");
    drop(setup(&path)?);

    let mut options = OpenOptions::new();
    options.no_mutex();

    let mut pool = PoolBuilder::new(options, 1)
        .with_write_setup(|c| {
            c.busy_timeout(5000)?;
            Ok(())
        })
        .open::<Read, Write>(&path)?;

    let holder = hold_write_lock(&path)?;
    pool.as_write_mut().insert.execute(1)?;
    holder.join().unwrap()?;
    Ok(())
}
