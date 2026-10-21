use sqll::{Code, Connection, OwnedValue, Result, Value};

fn setup() -> Result<Connection> {
    let c = Connection::open_in_memory()?;

    c.execute(
        r#"
        CREATE TABLE users (name TEXT, age INTEGER);

        INSERT INTO users VALUES ('Alice', 42), ('Bob', NULL), ('Carol', 35);
        "#,
    )?;

    Ok(c)
}

#[test]
fn bind_iter_empty() -> Result<()> {
    let c = setup()?;
    let mut stmt = c.prepare("SELECT COUNT(*) FROM users")?;
    stmt.bind_iter(Vec::<OwnedValue>::new())?;
    assert_eq!(stmt.first::<i64>(())?, Some(3));
    Ok(())
}

#[test]
fn bind_iter_rebinds_after_reset() -> Result<()> {
    let c = setup()?;
    let mut stmt = c.prepare("SELECT name FROM users WHERE name IN (?, ?) ORDER BY name")?;

    stmt.bind_iter(["Alice", "Bob"])?;
    let names = stmt.iter::<String>().collect::<Result<Vec<_>>>()?;
    assert_eq!(names, ["Alice", "Bob"]);

    stmt.reset()?;
    stmt.bind_iter(["Carol", "Dave"])?;
    let names = stmt.iter::<String>().collect::<Result<Vec<_>>>()?;
    assert_eq!(names, ["Carol"]);
    Ok(())
}

#[test]
fn bind_iter_mixed_values() -> Result<()> {
    let c = setup()?;
    let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ? OR name = ? ORDER BY name")?;

    stmt.bind_iter([OwnedValue::Null, OwnedValue::from("Carol")])?;
    let names = stmt.iter::<String>().collect::<Result<Vec<_>>>()?;
    assert_eq!(names, ["Bob", "Carol"]);

    stmt.reset()?;
    stmt.bind_iter([Value::integer(42), Value::text("Bob")])?;
    let names = stmt.iter::<String>().collect::<Result<Vec<_>>>()?;
    assert_eq!(names, ["Alice", "Bob"]);
    Ok(())
}

#[test]
fn bind_iter_count_mismatch() -> Result<()> {
    let c = setup()?;
    let mut stmt = c.prepare("SELECT name FROM users WHERE name IN (?, ?)")?;

    let e = stmt.bind_iter(["Alice"]).unwrap_err();
    assert_eq!(e.code(), Code::RANGE);

    let e = stmt.bind_iter(["Alice", "Bob", "Carol"]).unwrap_err();
    assert_eq!(e.code(), Code::RANGE);

    let mut stmt = c.prepare("SELECT name FROM users")?;
    let e = stmt.bind_iter(["Alice"]).unwrap_err();
    assert_eq!(e.code(), Code::RANGE);
    Ok(())
}

#[test]
fn bind_iter_named_parameters() -> Result<()> {
    let c = setup()?;
    let mut stmt =
        c.prepare("SELECT name FROM users WHERE name = :a OR name = :b ORDER BY name")?;
    stmt.bind_iter(["Carol", "Alice"])?;
    let names = stmt.iter::<String>().collect::<Result<Vec<_>>>()?;
    assert_eq!(names, ["Alice", "Carol"]);
    Ok(())
}

#[test]
fn owned_value_round_trip() -> Result<()> {
    let c = Connection::open_in_memory()?;
    c.execute("CREATE TABLE test (value)")?;

    let values = [
        OwnedValue::Null,
        OwnedValue::Integer(i64::MIN),
        OwnedValue::Float(-0.5),
        OwnedValue::Text(String::new()),
        OwnedValue::Text(String::from("hello")),
        OwnedValue::Blob(Vec::new()),
        OwnedValue::Blob(vec![0, 1, 2]),
    ];

    let mut insert = c.prepare("INSERT INTO test (value) VALUES (?)")?;

    for value in &values {
        insert.execute(value)?;
    }

    let mut select = c.prepare("SELECT value FROM test ORDER BY rowid")?;
    let read = select.iter::<OwnedValue>().collect::<Result<Vec<_>>>()?;
    assert_eq!(read, values);
    Ok(())
}

#[test]
fn owned_value_invalid_utf8() -> Result<()> {
    let c = Connection::open_in_memory()?;
    let mut stmt = c.prepare("SELECT CAST(X'FF' AS TEXT)")?;
    let e = stmt.next::<OwnedValue>().unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);
    Ok(())
}
