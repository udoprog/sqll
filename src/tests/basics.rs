use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use anyhow::Result;

use crate::{Code, Connection, Index, Null, Text, Value};

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

    stmt.bind_value(Index::BIND, 2i64)?;
    stmt.bind_value(Index::BIND + 1, "Bob")?;
    stmt.bind_value(Index::BIND + 2, 69.42)?;
    stmt.bind_value(Index::BIND + 3, &[0x69u8, 0x42u8][..])?;
    stmt.bind_value(Index::BIND + 4, Null)?;

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

    stmt.bind_value(Index::BIND, "%type")?;

    let mut count = 0;

    while stmt.step()?.is_row() {
        count += 1;
    }

    assert_eq!(count, 6);
    Ok(())
}

#[test]
fn statement_limit_binding() -> Result<()> {
    let mut c = Connection::open_in_memory()?;
    data::english(&mut c)?;

    let mut stmt = c.prepare("SELECT value AS value2 FROM english WHERE value LIKE ? LIMIT ?")?;
    stmt.bind(("%type", 3))?;

    let mut values = Vec::<String>::new();

    while let Some(value) = stmt.next()? {
        values.push(value);
    }

    assert_eq!(values, ["cerotype", "metatype", "ozotype"]);
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

#[test]
fn prepare_empty() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for sql in [
        "",
        "   ",
        "\n\t",
        ";",
        " ; ;",
        "-- comment",
        "/* comment */",
        "-- a\n/* b */ ;",
    ] {
        let Err(e) = c.prepare(sql) else {
            panic!("expected error for {sql:?}");
        };

        assert_eq!(e.code(), Code::MISUSE, "{sql:?}");
    }

    Ok(())
}

#[test]
fn prepare_trailing_empty() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for sql in [
        "SELECT 1",
        "SELECT 1;",
        "SELECT 1;\n",
        "SELECT 1\n",
        "SELECT 1; ",
        "SELECT 1;;",
        "SELECT 1; ; \n ;",
        "SELECT 1; -- comment",
        "SELECT 1 -- comment",
        "SELECT 1; /* comment */",
        "SELECT 1; /* comment */\n-- another\n",
        "SELECT 1; /* unterminated",
    ] {
        let mut stmt = c
            .prepare(sql)
            .map_err(|e| anyhow::anyhow!("{sql:?}: {e}"))?;
        assert_eq!(stmt.next::<i64>()?, Some(1), "{sql:?}");
    }

    Ok(())
}

#[test]
fn prepare_multiple_statements() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for sql in [
        "SELECT 1; SELECT 2",
        "SELECT 1; SELECT 2;",
        "SELECT 1;\n-- comment\nSELECT 2;\n",
        "SELECT 1; ; SELECT 2",
        "SELECT 1; SELECT * FROM missing",
        "SELECT 1; not valid sql",
    ] {
        let Err(e) = c.prepare(sql) else {
            panic!("expected error for {sql:?}");
        };

        assert_eq!(e.code(), Code::MISUSE, "{sql:?}");
    }

    Ok(())
}

#[test]
fn deserialize_empty() -> Result<()> {
    let c = Connection::open_in_memory()?;
    c.deserialize(c"main", crate::OwnedBytes::new())?;

    c.execute("CREATE TABLE users (name TEXT); INSERT INTO users VALUES ('Alice')")?;

    let names = c
        .prepare("SELECT name FROM users")?
        .iter::<String>()
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(names, ["Alice"]);

    // Dropping the connection frees the buffer handed to sqlite.
    drop(c);
    Ok(())
}
