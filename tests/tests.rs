#![cfg(not(miri))]

use std::path::Path;
use std::thread;

use anyhow::{Context, Result};
use sqlite_ll::{Code, Connection, OpenOptions, State, Type, Value};

// Test cases copied from https://github.com/stainless-steel/sqlite under the
// MIT license.

#[test]
fn connection_change_count() -> sqlite_ll::Result<()> {
    let c = setup_users(":memory:")?;
    assert_eq!(c.change_count(), 1);
    assert_eq!(c.total_change_count(), 1);

    c.execute("INSERT INTO users VALUES (2, 'Bob', NULL, NULL, NULL)")?;
    assert_eq!(c.change_count(), 1);
    assert_eq!(c.total_change_count(), 2);

    c.execute("UPDATE users SET name = 'Bob' WHERE id = 1")?;
    assert_eq!(c.change_count(), 1);
    assert_eq!(c.total_change_count(), 3);

    c.execute("DELETE FROM users")?;
    assert_eq!(c.change_count(), 2);
    assert_eq!(c.total_change_count(), 5);
    Ok(())
}

#[test]
fn connection_error() -> sqlite_ll::Result<()> {
    let connection = setup_users(":memory:")?;
    let e = connection.execute(":)").unwrap_err();
    assert_eq!(e.code(), Code::ERROR);
    Ok(())
}

#[test]
fn connection_open_with_flags() -> Result<()> {
    let dir = tempfile::tempdir().context("tempdir")?;
    let path = dir.path().join("database.sqlite3");

    setup_users(&path)?;

    let flags = OpenOptions::new().set_read_only();
    let connection = flags.open(path)?;
    let e = connection
        .execute("INSERT INTO users VALUES (2, 'Bob', NULL, NULL, NULL)")
        .unwrap_err();

    assert_eq!(e.code(), Code::READONLY);
    Ok(())
}

#[test]
fn connection_set_busy_handler() -> Result<()> {
    let dir = tempfile::tempdir().context("tempdir")?;
    let path = dir.path().join("database.sqlite3");

    setup_users(&path)?;

    let mut guards = Vec::with_capacity(100);

    for _ in 0..100 {
        let path = path.to_path_buf();

        guards.push(thread::spawn(move || {
            let mut connection = Connection::open(path)?;
            connection.set_busy_handler(|_| true)?;
            let statement = "INSERT INTO users VALUES (?, ?, ?, ?, ?)";
            let mut statement = connection.prepare(statement)?;
            statement.bind(1, 2i64)?;
            statement.bind(2, "Bob")?;
            statement.bind(3, 69.42)?;
            statement.bind(4, &[0x69u8, 0x42u8][..])?;
            statement.bind(5, ())?;
            assert_eq!(statement.step()?, State::Done);
            Ok::<_, sqlite_ll::Error>(true)
        }));
    }

    for guard in guards {
        assert!(guard.join().unwrap()?);
    }

    Ok(())
}

#[test]
fn statement_bind() -> sqlite_ll::Result<()> {
    let c = setup_users(":memory:")?;
    let statement = "INSERT INTO users VALUES (?, ?, ?, ?, ?)";
    let mut s = c.prepare(statement)?;

    s.bind(1, 2i64)?;
    s.bind(2, "Bob")?;
    s.bind(3, 69.42)?;
    s.bind(4, &[0x69u8, 0x42u8][..])?;
    s.bind(5, ())?;
    assert_eq!(s.step()?, State::Done);
    Ok(())
}

#[test]
fn statement_bind_with_nullable() -> sqlite_ll::Result<()> {
    let connection = setup_users(":memory:")?;
    let s = "INSERT INTO users VALUES (?, ?, ?, ?, ?)";
    let mut s = connection.prepare(s)?;

    s.bind(1, None::<i64>)?;
    s.bind(2, None::<&str>)?;
    s.bind(3, None::<f64>)?;
    s.bind(4, None::<&[u8]>)?;
    s.bind(5, None::<&str>)?;
    assert_eq!(s.step()?, State::Done);

    let s = "INSERT INTO users VALUES (?, ?, ?, ?, ?)";
    let mut s = connection.prepare(s)?;

    s.bind(1, Some(2i64))?;
    s.bind(2, Some("Bob"))?;
    s.bind(3, Some(69.42))?;
    s.bind(4, Some(&[0x69u8, 0x42u8][..]))?;
    s.bind(5, None::<&str>)?;
    assert_eq!(s.step()?, State::Done);
    Ok(())
}

#[test]
fn statement_bind_by_name() -> sqlite_ll::Result<()> {
    let connection = setup_users(":memory:")?;
    let s = "INSERT INTO users VALUES (:id, :name, :age, :photo, :email)";
    let mut s = connection.prepare(s)?;

    s.bind_by_name(c":id", 2i64)?;
    s.bind_by_name(c":name", "Bob")?;
    s.bind_by_name(c":age", 69.42)?;
    s.bind_by_name(c":photo", &[0x69u8, 0x42u8][..])?;
    s.bind_by_name(c":email", ())?;
    assert!(s.bind_by_name(c":missing", 404).is_err());
    assert_eq!(s.step()?, State::Done);
    Ok(())
}

#[test]
fn statement_column_count() -> sqlite_ll::Result<()> {
    let connection = setup_users(":memory:")?;
    let s = "SELECT * FROM users";
    let mut s = connection.prepare(s)?;

    assert_eq!(s.step()?, State::Row);

    assert_eq!(s.column_count(), 5);
    Ok(())
}

#[test]
fn statement_column_name() -> sqlite_ll::Result<()> {
    let connection = setup_users(":memory:")?;
    let s = "SELECT id, name, age, photo AS user_photo FROM users";
    let s = connection.prepare(s)?;

    let names = s.column_names().collect::<Vec<_>>();
    assert_eq!(names, ["id", "name", "age", "user_photo"]);
    assert_eq!("user_photo", s.column_name(3)?);
    Ok(())
}

#[test]
fn statement_column_type() -> sqlite_ll::Result<()> {
    let connection = setup_users(":memory:")?;
    let s = "SELECT * FROM users";
    let mut s = connection.prepare(s)?;

    assert_eq!(s.column_type(0), Type::Null);
    assert_eq!(s.column_type(1), Type::Null);
    assert_eq!(s.column_type(2), Type::Null);
    assert_eq!(s.column_type(3), Type::Null);

    assert_eq!(s.step()?, State::Row);

    assert_eq!(s.column_type(0), Type::Integer);
    assert_eq!(s.column_type(1), Type::Text);
    assert_eq!(s.column_type(2), Type::Float);
    assert_eq!(s.column_type(3), Type::Blob);
    Ok(())
}

#[test]
fn statement_parameter_index() -> sqlite_ll::Result<()> {
    let connection = setup_users(":memory:")?;
    let statement = "INSERT INTO users VALUES (:id, :name, :age, :photo, :email)";
    let mut statement = connection.prepare(statement)?;

    statement.bind(statement.parameter_index(c":id").unwrap(), 2)?;
    statement.bind(statement.parameter_index(c":name").unwrap(), "Bob")?;
    statement.bind(statement.parameter_index(c":age").unwrap(), 69.42)?;
    statement.bind(
        statement.parameter_index(c":photo").unwrap(),
        &[0x69u8, 0x42u8][..],
    )?;
    statement.bind(statement.parameter_index(c":email").unwrap(), ())?;
    assert_eq!(statement.parameter_index(c":missing"), None);
    assert_eq!(statement.step()?, State::Done);
    Ok(())
}

#[test]
fn statement_read() -> sqlite_ll::Result<()> {
    let c = setup_users(":memory:")?;
    let s = "SELECT * FROM users";
    let mut s = c.prepare(s)?;

    assert_eq!(s.step()?, State::Row);
    assert_eq!(s.read::<i64>(0)?, 1);
    assert_eq!(s.read::<String>(1)?, String::from("Alice"));
    assert_eq!(s.read::<f64>(2)?, 42.69);
    assert_eq!(s.read::<Vec<u8>>(3)?, [0x42, 0x69]);
    assert_eq!(s.read::<Value>(4)?, Value::null());
    assert_eq!(s.step()?, State::Done);
    Ok(())
}

#[test]
fn statement_read_with_nullable() -> sqlite_ll::Result<()> {
    let c = setup_users(":memory:")?;
    let s = "SELECT * FROM users";
    let mut s = c.prepare(s)?;

    assert_eq!(s.step()?, State::Row);
    assert_eq!(s.read::<Option<i64>>(0)?, Some(1));
    assert_eq!(s.read::<Option<String>>(1)?, Some(String::from("Alice")));
    assert_eq!(s.read::<Option<f64>>(2)?, Some(42.69));
    assert_eq!(s.read::<Option<Vec<u8>>>(3)?, Some(vec![0x42, 0x69]));
    assert_eq!(s.read::<Option<String>>(4)?, None);
    assert_eq!(s.step()?, State::Done);
    Ok(())
}

#[test]
fn statement_wildcard() -> sqlite_ll::Result<()> {
    let c = setup_english(":memory:")?;
    let s = "SELECT value FROM english WHERE value LIKE '%type'";
    let mut s = c.prepare(s)?;

    let mut count = 0;

    while let State::Row = s.step()? {
        count += 1;
    }

    assert_eq!(count, 6);
    Ok(())
}

#[test]
fn statement_wildcard_with_binding() -> sqlite_ll::Result<()> {
    let c = setup_english(":memory:")?;
    let s = "SELECT value FROM english WHERE value LIKE ?";
    let mut s = c.prepare(s)?;
    s.bind(1, "%type")?;

    let mut count = 0;
    while let State::Row = s.step()? {
        count += 1;
    }
    assert_eq!(count, 6);
    Ok(())
}

#[test]
fn test_dropped_connection() -> sqlite_ll::Result<()> {
    let c = setup_users(":memory:")?;
    let s = "SELECT id, name, age, photo AS user_photo FROM users";
    let s = c.prepare(s)?;
    drop(c);

    let names = s.column_names().collect::<Vec<_>>();
    assert_eq!(names, ["id", "name", "age", "user_photo"]);
    assert_eq!("user_photo", s.column_name(3)?);
    Ok(())
}

fn setup_english(path: impl AsRef<Path>) -> sqlite_ll::Result<Connection> {
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

fn setup_users(path: impl AsRef<Path>) -> sqlite_ll::Result<Connection> {
    let c = Connection::open(path)?;
    c.execute(
        "
        CREATE TABLE users (id INTEGER, name TEXT, age REAL, photo BLOB, email TEXT);
        INSERT INTO users VALUES (1, 'Alice', 42.69, X'4269', NULL);
        ",
    )?;
    Ok(c)
}
