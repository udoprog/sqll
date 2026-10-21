//! Integration tests for the typed statement API.
//!
//! Behavioural examples for the `BoundStatement` methods (`first`, `Iterator`,
//! `reset`, and `Drop`) live as doctests on the methods themselves. This file
//! covers the end-to-end derive flow plus the `TryFrom` conversion error paths,
//! which assert exact diagnostics and are awkward to express as doctests.

use anyhow::Result;
use sqll::{OpenOptions, Statements, TryFromSendStatementError, TypedStatement};

#[derive(Statements)]
#[sql(read_only)]
struct Read {
    #[sql = "SELECT name, age FROM users ORDER BY age"]
    all: TypedStatement<(), (String, i32)>,
}

#[derive(Statements)]
struct Write {
    #[sql(statements)]
    read: Read,
    #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    insert: TypedStatement<(String, i32), ()>,
}

fn setup() -> Result<sqll::Connection> {
    let c = OpenOptions::new()
        .no_mutex()
        .read_write()
        .create()
        .open_in_memory()?;

    c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
    Ok(c)
}

#[test]
fn end_to_end_execute_and_query() -> Result<()> {
    let mut c = setup()?;
    let mut w = Write::build(&mut c)?;

    w.insert.execute(("Alice".to_string(), 25))?;
    w.insert.execute(("Bob".to_string(), 35))?;
    w.insert.execute(("Charlie".to_string(), 30))?;

    let out = w.read.all.query()?.collect::<Result<Vec<_>, _>>()?;

    assert_eq!(
        out,
        vec![
            ("Alice".to_string(), 25),
            ("Charlie".to_string(), 30),
            ("Bob".to_string(), 35),
        ]
    );
    Ok(())
}

/// Prepare a parameterless single-column statement and coerce it into a
/// thread-safe [`sqll::SendStatement`] for the `TryFrom` conversion tests.
fn select_one_send(c: &sqll::Connection) -> Result<sqll::SendStatement> {
    let s = c.prepare("SELECT 1")?;
    // Safe: the connection was opened with `no_mutex`.
    Ok(unsafe { s.into_send()? })
}

#[test]
fn try_from_wrong_bind_count() -> Result<()> {
    let c = setup()?;
    let send = select_one_send(&c)?;

    // "SELECT 1" binds 0 parameters, but the target type expects 1.
    // `map(drop)` discards the non-`Debug` `Ok` value so `unwrap_err` compiles.
    let err = TypedStatement::<(i32,), (i32,)>::try_from(send)
        .map(drop)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "unexpected bind parameter count for statement: expected 1, got 0"
    );
    // Assert the error is publicly nameable and a real `std::error::Error`.
    fn assert_error<E: core::error::Error>(_: &E) {}
    assert_error::<TryFromSendStatementError>(&err);
    Ok(())
}

#[test]
fn try_from_wrong_column_count() -> Result<()> {
    let c = setup()?;
    let send = select_one_send(&c)?;

    // "SELECT 1" yields 1 column, but the target type expects 2.
    let err = TypedStatement::<(), (i32, i32)>::try_from(send)
        .map(drop)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "unexpected column count for statement: expected 2, got 1"
    );
    Ok(())
}

#[test]
fn try_from_matching_counts() -> Result<()> {
    let c = setup()?;
    let send = select_one_send(&c)?;

    let mut stmt = TypedStatement::<(), (i32,)>::try_from(send).unwrap();
    assert_eq!(stmt.query()?.first()?, Some((1,)));
    Ok(())
}

/// After a row fails to convert, the iterator is fused rather than stepping on
/// (or restarting) the statement.
#[test]
fn iterator_fused_after_error() -> Result<()> {
    let mut c = setup()?;
    c.execute("INSERT INTO users (name, age) VALUES ('Alice', 'not a number'), ('Bob', 35)")?;

    let mut read = Read::build(&mut c)?;
    let mut stmt = read.all.query()?;

    // Integers sort before text, so Bob comes first and Alice fails to convert.
    assert_eq!(stmt.next().transpose()?, Some(("Bob".to_string(), 35)));
    assert!(matches!(stmt.next(), Some(Err(..))));
    assert!(stmt.next().is_none());
    assert!(stmt.next().is_none());
    drop(stmt);

    // Dropping reset the statement, so it reads from the top again.
    let mut stmt = read.all.query()?;
    assert_eq!(stmt.next().transpose()?, Some(("Bob".to_string(), 35)));
    assert!(matches!(stmt.next(), Some(Err(..))));
    Ok(())
}

/// After the last row the iterator is fused rather than restarting the
/// statement, which SQLite would otherwise do on the next step.
#[test]
fn iterator_fused_after_done() -> Result<()> {
    let mut c = setup()?;
    c.execute("INSERT INTO users (name, age) VALUES ('Alice', 25)")?;

    let mut read = Read::build(&mut c)?;
    let mut stmt = read.all.query()?;

    assert_eq!(stmt.next().transpose()?, Some(("Alice".to_string(), 25)));
    assert!(stmt.next().is_none());
    assert!(stmt.next().is_none());
    Ok(())
}
