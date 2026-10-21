//! Tests for the `FromColumn` and `BindValue` derives.

#![cfg(feature = "derive")]

use sqll::{Bind, BindValue, Code, Connection, FromColumn, Result};

#[derive(Debug, PartialEq, FromColumn, BindValue)]
struct Id(u64);

#[derive(Debug, PartialEq, FromColumn, BindValue)]
#[sql(transparent)]
struct Named {
    value: String,
}

#[derive(Debug, PartialEq, FromColumn, BindValue)]
struct Borrowed<'a>(&'a str);

#[derive(Debug, PartialEq, FromColumn, BindValue)]
struct Generic<T>(T);

#[derive(Debug, PartialEq, FromColumn, BindValue)]
#[sql(integer)]
enum Implicit {
    A,
    B,
    C = 10,
    D,
    #[sql(value = -5)]
    E,
}

#[derive(Debug, PartialEq, FromColumn, BindValue)]
#[sql(integer)]
enum WithOther {
    A = 1,
    #[sql(other)]
    Unknown = 0,
}

#[derive(Debug, PartialEq, FromColumn, BindValue)]
#[sql(text, rename_all = "kebab-case")]
enum Text {
    InProgress,
    #[sql(rename = "in")]
    Input,
    #[sql(other)]
    Other,
}

#[derive(Debug, PartialEq, FromColumn, BindValue)]
#[sql(text)]
enum Strict {
    Yes,
    No,
}

/// Bind `value` and read it back as `T`.
fn select<T>(c: &Connection, value: impl Bind) -> Result<T>
where
    T: for<'stmt> FromColumn<'stmt>,
{
    let mut stmt = c.prepare("SELECT ?")?;
    stmt.bind(value)?;
    Ok(stmt.next::<T>()?.expect("a row"))
}

fn roundtrip<T>(c: &Connection, value: T) -> Result<T>
where
    T: Bind + for<'stmt> FromColumn<'stmt>,
{
    select(c, value)
}

#[test]
fn newtypes() -> Result<()> {
    let c = Connection::open_in_memory()?;

    assert_eq!(roundtrip(&c, Id(42))?, Id(42));
    assert_eq!(
        roundtrip(&c, Named { value: "hi".into() })?,
        Named { value: "hi".into() }
    );
    assert_eq!(roundtrip(&c, Generic(1.5f64))?, Generic(1.5));
    assert_eq!(roundtrip(&c, Some(Id(7)))?, Some(Id(7)));

    let mut stmt = c.prepare("SELECT ?")?;
    stmt.bind(Borrowed("borrowed"))?;
    assert_eq!(stmt.next::<Borrowed<'_>>()?, Some(Borrowed("borrowed")));

    let mut stmt = c.prepare("SELECT NULL")?;
    assert_eq!(stmt.next::<Option<Id>>()?, Some(None));
    Ok(())
}

#[test]
fn integer_enum() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for (variant, value) in [
        (Implicit::A, 0),
        (Implicit::B, 1),
        (Implicit::C, 10),
        (Implicit::D, 11),
        (Implicit::E, -5),
    ] {
        let mut stmt = c.prepare("SELECT ?")?;
        stmt.bind(&variant)?;
        assert_eq!(stmt.next::<i64>()?, Some(value));
        assert_eq!(select::<Implicit>(&c, value)?, variant);
    }

    let mut stmt = c.prepare("SELECT 2")?;
    let err = stmt.next::<Implicit>().unwrap_err();
    assert_eq!(err.code(), Code::MISMATCH);
    assert_eq!(err.to_string(), "unknown Implicit value 2");

    let mut stmt = c.prepare("SELECT 'text'")?;
    assert_eq!(stmt.next::<Implicit>().unwrap_err().code(), Code::MISMATCH);

    let mut stmt = c.prepare("SELECT 1 UNION ALL SELECT 99")?;
    assert_eq!(stmt.next::<WithOther>()?, Some(WithOther::A));
    assert_eq!(stmt.next::<WithOther>()?, Some(WithOther::Unknown));
    assert_eq!(roundtrip(&c, WithOther::Unknown)?, WithOther::Unknown);
    Ok(())
}

#[test]
fn text_enum() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for (variant, value) in [
        (Text::InProgress, "in-progress"),
        (Text::Input, "in"),
        (Text::Other, "other"),
    ] {
        let mut stmt = c.prepare("SELECT ?")?;
        stmt.bind(&variant)?;
        assert_eq!(stmt.next::<String>()?.as_deref(), Some(value));
        assert_eq!(select::<Text>(&c, value)?, variant);
    }

    let mut stmt = c.prepare("SELECT 'whatever'")?;
    assert_eq!(stmt.next::<Text>()?, Some(Text::Other));

    assert_eq!(roundtrip(&c, Strict::Yes)?, Strict::Yes);
    assert_eq!(roundtrip(&c, Strict::No)?, Strict::No);

    let mut stmt = c.prepare("SELECT 'maybe'")?;
    let err = stmt.next::<Strict>().unwrap_err();
    assert_eq!(err.code(), Code::MISMATCH);
    assert_eq!(err.to_string(), "unknown Strict value \"maybe\"");
    Ok(())
}

#[test]
fn execute_single_value() -> Result<()> {
    let c = Connection::open_in_memory()?;

    c.execute(
        r#"
        CREATE TABLE things (id INTEGER, kind TEXT);
        INSERT INTO things VALUES (1, 'Yes'), (2, 'No'), (3, 'No');
        "#,
    )?;

    let mut delete = c.prepare("DELETE FROM things WHERE kind = ?")?;
    assert_eq!(delete.execute(Strict::No)?, 2);

    let mut delete = c.prepare("DELETE FROM things WHERE id = ?")?;
    assert_eq!(delete.execute(Id(1))?, 1);
    Ok(())
}

mod renamed {
    extern crate sqll as my_sqll;

    #[derive(Debug, PartialEq, my_sqll::FromColumn, my_sqll::BindValue)]
    #[sql(crate = my_sqll)]
    struct Id(i64);

    #[derive(Debug, PartialEq, my_sqll::FromColumn, my_sqll::BindValue)]
    #[sql(crate = my_sqll, integer)]
    enum E {
        A,
    }

    #[test]
    fn renamed_crate() -> my_sqll::Result<()> {
        let c = my_sqll::Connection::open_in_memory()?;
        assert_eq!(super::roundtrip(&c, Id(3))?, Id(3));
        assert_eq!(super::roundtrip(&c, E::A)?, E::A);
        Ok(())
    }
}
