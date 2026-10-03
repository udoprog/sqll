use std::thread;

use alloc::vec::Vec;

use anyhow::{Context, Result};

use crate::{Code, Connection, Index, Null, OpenOptions};

use super::data;

#[test]
fn connection_open_with_flags() -> Result<()> {
    let dir = tempfile::tempdir().context("tempdir")?;
    let path = dir.path().join("database.sqlite3");

    let mut c = Connection::open(&path)?;

    data::users(&mut c)?;

    let mut flags = OpenOptions::new();
    flags.read_only();
    let c = flags.open(path)?;
    let e = c
        .execute("INSERT INTO users VALUES (2, 'Bob', NULL, NULL, NULL)")
        .unwrap_err();

    assert_eq!(e.code(), Code::READONLY);
    Ok(())
}

#[test]
fn connection_busy_handler() -> Result<()> {
    let dir = tempfile::tempdir().context("tempdir")?;
    let path = dir.path().join("database.sqlite3");

    let mut c = Connection::open(&path)?;

    data::users(&mut c)?;

    let mut guards = Vec::with_capacity(100);

    for _ in 0..100 {
        let path = path.to_path_buf();

        guards.push(thread::spawn(move || -> Result<bool> {
            let mut c = Connection::open(path)?;
            c.busy_handler(|_| true)?;
            let mut stmt = c.prepare("INSERT INTO users VALUES (?, ?, ?, ?, ?)")?;
            stmt.bind_value(Index::BIND, 2i64)?;
            stmt.bind_value(Index::BIND + 1, "Bob")?;
            stmt.bind_value(Index::BIND + 2, 69.42)?;
            stmt.bind_value(Index::BIND + 3, &[0x69u8, 0x42u8][..])?;
            stmt.bind_value(Index::BIND + 4, Null)?;
            assert!(stmt.step()?.is_done());
            Ok(true)
        }));
    }

    for guard in guards {
        assert!(guard.join().unwrap()?);
    }

    Ok(())
}

#[test]
fn connection_busy_handler_drops_closure() -> Result<()> {
    use alloc::sync::Arc;

    let first = Arc::new(());
    let second = Arc::new(());

    let mut c = Connection::open_in_memory()?;

    let captured = first.clone();
    c.busy_handler(move |_| {
        let _ = &captured;
        false
    })?;
    assert_eq!(Arc::strong_count(&first), 2);

    // Replacing the handler drops the previous closure.
    let captured = second.clone();
    c.busy_handler(move |_| {
        let _ = &captured;
        false
    })?;
    assert_eq!(Arc::strong_count(&first), 1);
    assert_eq!(Arc::strong_count(&second), 2);

    // Clearing the handler drops the closure.
    c.clear_busy_handler()?;
    assert_eq!(Arc::strong_count(&second), 1);

    // Dropping the connection drops the closure.
    let captured = first.clone();
    c.busy_handler(move |_| {
        let _ = &captured;
        false
    })?;
    assert_eq!(Arc::strong_count(&first), 2);
    drop(c);
    assert_eq!(Arc::strong_count(&first), 1);
    Ok(())
}
