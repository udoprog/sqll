//! With the `threadsafe` feature `Connection` is `Send` while statements do not
//! borrow it, so every safely opened connection must be serialized.

use std::format;
use std::thread;

use anyhow::Result;

use crate::{Connection, OpenOptions, State, ffi};

unsafe extern "C" {
    // Returns NULL unless the connection is in the "serialized" threading mode.
    fn sqlite3_db_mutex(db: *mut ffi::sqlite3) -> *mut core::ffi::c_void;
}

fn rw() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read_write().create();
    options
}

fn is_serialized(c: &Connection) -> Result<bool> {
    let stmt = c.prepare("SELECT 1")?;

    // SAFETY: The statement and its connection are alive.
    let mutex = unsafe { sqlite3_db_mutex(ffi::sqlite3_db_handle(stmt.as_ptr())) };
    Ok(!mutex.is_null())
}

#[test]
fn safe_open_is_serialized() -> Result<()> {
    assert!(is_serialized(&rw().open_in_memory()?)?);
    assert!(is_serialized(
        &OpenOptions::empty().read_write().open_in_memory()?
    )?);
    assert!(is_serialized(&rw().no_mutex().open_in_memory()?)?);
    assert!(is_serialized(&rw().full_mutex().open_in_memory()?)?);
    assert!(is_serialized(
        &rw().no_mutex().full_mutex().open_in_memory()?
    )?);
    assert!(is_serialized(&Connection::open_in_memory()?)?);
    Ok(())
}

#[test]
fn no_mutex_unchecked_is_not_serialized() -> Result<()> {
    // SAFETY: The connection and its statements stay on this thread.
    let c = unsafe { rw().no_mutex_unchecked().open_in_memory()? };
    assert!(!is_serialized(&c)?);
    Ok(())
}

#[test]
fn into_send_accepts_every_safe_open() -> Result<()> {
    for options in [rw(), *rw().no_mutex(), *rw().full_mutex()] {
        let c = options.open_in_memory()?;
        let stmt = c.prepare("SELECT 1")?;

        // SAFETY: Nothing is sent anywhere.
        unsafe {
            stmt.into_send()?;
            c.into_send()?;
        }
    }

    Ok(())
}

/// Moving a connection to another thread while one of its statements keeps
/// stepping on this one. This used to drive a `SQLITE_OPEN_NOMUTEX` handle from
/// two threads at once.
#[test]
#[cfg_attr(miri, ignore)]
fn connection_and_statement_on_different_threads() -> Result<()> {
    let c = rw().no_mutex().open_in_memory()?;

    c.execute(
        "
        CREATE TABLE src (n INTEGER);
        CREATE TABLE dst (n INTEGER);
        WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 1000)
        INSERT INTO src SELECT n FROM r;
        ",
    )?;

    let mut select = c.prepare("SELECT n FROM src")?;

    let writer = thread::spawn(move || -> Result<()> {
        for n in 0..2000 {
            c.execute(format!("INSERT INTO dst VALUES ({n})"))?;
        }

        Ok(())
    });

    let mut total = 0i64;

    for _ in 0..20 {
        select.reset()?;

        while let State::Row = select.step()? {
            total += select.column::<i64>(0)?;
        }
    }

    writer.join().unwrap()?;
    assert_eq!(total, 20 * 500_500);
    Ok(())
}
