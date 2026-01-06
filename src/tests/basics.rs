use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use anyhow::Result;

use crate::{Connection, Null, Text, Value};

use super::data;

// Test cases copied from https://github.com/stainless-steel/sqlite under the
// MIT license.

#[test]
fn connection_change_count() -> Result<()> {
    let mut c = Connection::open_in_memory()?;
    data::users(&mut c)?;

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
fn statement_bind() -> Result<()> {
    let mut c = Connection::open_in_memory()?;
    data::users(&mut c)?;

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
fn statement_column_name() -> Result<()> {
    let mut c = Connection::open_in_memory()?;
    data::users(&mut c)?;

    let stmt = c.prepare("SELECT id, name, age, photo AS user_photo FROM users")?;

    let names = stmt.column_names().collect::<Vec<_>>();
    assert_eq!(names, ["id", "name", "age", "user_photo"]);
    assert_eq!(stmt.column_name(3), Some(Text::new("user_photo")));
    Ok(())
}

#[test]
fn statement_parameter_index() -> Result<()> {
    let mut c = Connection::open_in_memory()?;
    data::users(&mut c)?;

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
    let mut c = Connection::open_in_memory()?;
    data::users(&mut c)?;

    let mut stmt = c.prepare("SELECT * FROM users")?;

    assert!(stmt.step()?.is_row());
    assert_eq!(stmt.column::<i64>(0)?, 1);
    assert_eq!(stmt.column::<String>(1)?, String::from("Alice"));
    assert_eq!(stmt.column::<f64>(2)?, 42.69);
    assert_eq!(stmt.column::<Vec<u8>>(3)?, [0x42, 0x69]);
    assert_eq!(stmt.column::<Option<Value<'_>>>(4)?, None);
    assert!(stmt.step()?.is_done());
    Ok(())
}

#[test]
fn statement_read_with_nullable() -> Result<()> {
    let mut c = Connection::open_in_memory()?;
    data::users(&mut c)?;

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
    let mut c = Connection::open_in_memory()?;
    data::english(&mut c)?;

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
    let mut c = Connection::open_in_memory()?;
    data::english(&mut c)?;

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
    let mut c = Connection::open_in_memory()?;
    data::users(&mut c)?;

    let stmt = c.prepare("SELECT id, name, age, photo AS user_photo FROM users")?;
    drop(c);

    let names = stmt.column_names().collect::<Vec<_>>();
    assert_eq!(names, ["id", "name", "age", "user_photo"]);
    assert_eq!(stmt.column_name(3), Some(Text::new("user_photo")));
    Ok(())
}
