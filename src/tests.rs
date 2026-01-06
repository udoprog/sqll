use std::path::Path;
use std::thread;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use anyhow::{Context, Result};

use crate::{Code, Connection, Null, OpenOptions, Text, Value};

// Test cases copied from https://github.com/stainless-steel/sqlite under the
// MIT license.

#[test]
fn connection_change_count() -> Result<()> {
    let c = setup_users(":memory:")?;
    assert_eq!(c.changes(), 1);
    assert_eq!(c.total_changes(), 1);

    c.execute("INSERT INTO users VALUES (2, 'Bob', NULL, NULL, NULL)")?;
    assert_eq!(c.changes(), 1);
    assert_eq!(c.total_changes(), 2);

    c.execute("UPDATE users SET name = 'Bob' WHERE id = 1")?;
    assert_eq!(c.changes(), 1);
    assert_eq!(c.total_changes(), 3);

    c.execute("DELETE FROM users")?;
    assert_eq!(c.changes(), 2);
    assert_eq!(c.total_changes(), 5);
    Ok(())
}

#[test]
fn connection_error() -> Result<()> {
    let c = setup_users(":memory:")?;
    let e = c.execute(":)").unwrap_err();
    assert_eq!(e.code(), Code::ERROR);
    Ok(())
}

#[test]
fn connection_open_with_flags() -> Result<()> {
    let dir = tempfile::tempdir().context("tempdir")?;
    let path = dir.path().join("database.sqlite3");

    setup_users(&path)?;

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

    setup_users(&path)?;

    let mut guards = Vec::with_capacity(100);

    for _ in 0..100 {
        let path = path.to_path_buf();

        guards.push(thread::spawn(move || -> Result<bool> {
            let mut c = Connection::open(path)?;
            c.busy_handler(|_| true)?;
            let mut stmt = c.prepare("INSERT INTO users VALUES (?, ?, ?, ?, ?)")?;
            stmt.bind_value(1, 2i64)?;
            stmt.bind_value(2, "Bob")?;
            stmt.bind_value(3, 69.42)?;
            stmt.bind_value(4, &[0x69u8, 0x42u8][..])?;
            stmt.bind_value(5, Null)?;
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
fn statement_bind() -> Result<()> {
    let c = setup_users(":memory:")?;
    let mut stmt = c.prepare("INSERT INTO users VALUES (?, ?, ?, ?, ?)")?;

    stmt.bind_value(1, 2i64)?;
    stmt.bind_value(2, "Bob")?;
    stmt.bind_value(3, 69.42)?;
    stmt.bind_value(4, &[0x69u8, 0x42u8][..])?;
    stmt.bind_value(5, Null)?;

    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn statement_bind_with_nullable() -> Result<()> {
    let c = setup_users(":memory:")?;
    let mut stmt = c.prepare("INSERT INTO users VALUES (?, ?, ?, ?, ?)")?;

    stmt.bind_value(1, None::<i64>)?;
    stmt.bind_value(2, None::<&str>)?;
    stmt.bind_value(3, None::<f64>)?;
    stmt.bind_value(4, None::<&[u8]>)?;
    stmt.bind_value(5, None::<&str>)?;

    assert!(stmt.step()?.is_done());

    let mut stmt = c.prepare("INSERT INTO users VALUES (?, ?, ?, ?, ?)")?;

    stmt.bind_value(1, Some(2i64))?;
    stmt.bind_value(2, Some("Bob"))?;
    stmt.bind_value(3, Some(69.42))?;
    stmt.bind_value(4, Some(&[0x69u8, 0x42u8][..]))?;
    stmt.bind_value(5, None::<&str>)?;
    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn statement_bind_by_name() -> Result<()> {
    let c = setup_users(":memory:")?;
    let mut stmt = c.prepare("INSERT INTO users VALUES (:id, :name, :age, :photo, :email)")?;

    stmt.bind_value_by_name(c":id", 2i64)?;
    stmt.bind_value_by_name(c":name", "Bob")?;
    stmt.bind_value_by_name(c":age", 69.42)?;
    stmt.bind_value_by_name(c":photo", &[0x69u8, 0x42u8][..])?;
    stmt.bind_value_by_name(c":email", Null)?;
    assert!(stmt.bind_value_by_name(c":missing", 404).is_err());
    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn statement_column_count() -> Result<()> {
    let c = setup_users(":memory:")?;
    let mut stmt = c.prepare("SELECT * FROM users")?;
    assert!(stmt.step()?.is_row());
    assert_eq!(stmt.column_count(), 5);
    Ok(())
}

#[test]
fn statement_column_name() -> Result<()> {
    let c = setup_users(":memory:")?;
    let stmt = c.prepare("SELECT id, name, age, photo AS user_photo FROM users")?;

    let names = stmt.column_names().collect::<Vec<_>>();
    assert_eq!(names, ["id", "name", "age", "user_photo"]);
    assert_eq!(stmt.column_name(3), Some(Text::new("user_photo")));
    Ok(())
}

#[test]
fn statement_parameter_index() -> Result<()> {
    let c = setup_users(":memory:")?;
    let statement = "INSERT INTO users VALUES (:id, :name, :age, :photo, :email)";
    let mut stmt = c.prepare(statement)?;

    stmt.bind_value(stmt.bind_parameter_index(c":id").unwrap(), 2)?;
    stmt.bind_value(stmt.bind_parameter_index(c":name").unwrap(), "Bob")?;
    stmt.bind_value(stmt.bind_parameter_index(c":age").unwrap(), 69.42)?;
    stmt.bind_value(
        stmt.bind_parameter_index(c":photo").unwrap(),
        &[0x69u8, 0x42u8][..],
    )?;
    stmt.bind_value(stmt.bind_parameter_index(c":email").unwrap(), Null)?;
    assert_eq!(stmt.bind_parameter_index(c":missing"), None);
    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn statement_read() -> Result<()> {
    let c = setup_users(":memory:")?;
    let mut stmt = c.prepare("SELECT * FROM users")?;

    assert!(stmt.step()?.is_row());
    assert_eq!(stmt.column::<i64>(0)?, 1);
    assert_eq!(stmt.column::<String>(1)?, String::from("Alice"));
    assert_eq!(stmt.column::<f64>(2)?, 42.69);
    assert_eq!(stmt.column::<Vec<u8>>(3)?, [0x42, 0x69]);
    assert_eq!(stmt.column::<Value>(4)?, Value::null());
    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn statement_read_with_nullable() -> Result<()> {
    let c = setup_users(":memory:")?;
    let mut stmt = c.prepare("SELECT * FROM users")?;

    assert!(stmt.step()?.is_row());
    assert_eq!(stmt.column::<Option<i64>>(0)?, Some(1));
    assert_eq!(
        stmt.column::<Option<String>>(1)?,
        Some(String::from("Alice"))
    );
    assert_eq!(stmt.column::<Option<f64>>(2)?, Some(42.69));
    assert_eq!(stmt.column::<Option<Vec<u8>>>(3)?, Some(vec![0x42, 0x69]));
    assert_eq!(stmt.column::<Option<String>>(4)?, None);
    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn statement_wildcard() -> Result<()> {
    let c = setup_english(":memory:")?;
    let mut stmt = c.prepare("SELECT value FROM english WHERE value LIKE '%type'")?;

    let mut count = 0;

    while stmt.step()?.is_row() {
        count += 1;
    }

    assert_eq!(count, 6);
    Ok(())
}

#[test]
fn statement_wildcard_with_binding() -> Result<()> {
    let c = setup_english(":memory:")?;
    let mut stmt = c.prepare("SELECT value FROM english WHERE value LIKE ?")?;

    stmt.bind_value(1, "%type")?;

    let mut count = 0;

    while stmt.step()?.is_row() {
        count += 1;
    }

    assert_eq!(count, 6);
    Ok(())
}

#[test]
fn test_dropped_connection() -> Result<()> {
    let c = setup_users(":memory:")?;
    let stmt = c.prepare("SELECT id, name, age, photo AS user_photo FROM users")?;
    drop(c);

    let names = stmt.column_names().collect::<Vec<_>>();
    assert_eq!(names, ["id", "name", "age", "user_photo"]);
    assert_eq!(stmt.column_name(3), Some(Text::new("user_photo")));
    Ok(())
}

fn setup_english(path: impl AsRef<Path>) -> Result<Connection> {
    let c = Connection::open(path)?;

    c.execute(
        "
        CREATE TABLE english (value TEXT);
        INSERT INTO english VALUES ('cerotype');
        INSERT INTO english VALUES ('metatype');
        INSERT INTO english VALUES ('ozotype');
        INSERT INTO english VALUES ('phenotype');
        INSERT INTO english VALUES ('plastotype');
        INSERT INTO english VALUES ('undertype');
        INSERT INTO english VALUES ('nonsence');
        ",
    )?;

    Ok(c)
}

fn setup_users(path: impl AsRef<Path>) -> Result<Connection> {
    let c = Connection::open(path)?;

    c.execute(
        "
        CREATE TABLE users (id INTEGER, name TEXT, age REAL, photo BLOB, email TEXT);
        INSERT INTO users VALUES (1, 'Alice', 42.69, X'4269', NULL);
        ",
    )?;

    Ok(c)
}
