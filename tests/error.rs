use core::error::Error as _;
use core::fmt;

use sqll::{Code, Connection, Error, Result, TransactionBehavior};

#[test]
fn busy_with_and_without_extended_codes() -> Result<()> {
    let dir = tempfile::tempdir().map_err(Error::custom)?;
    let path = dir.path().join("test.db");

    let a = Connection::open(&path)?;
    a.execute("CREATE TABLE users (name TEXT)")?;
    let mut b = Connection::open(&path)?;

    let tx = a.begin(TransactionBehavior::Immediate)?;

    let e = b.begin(TransactionBehavior::Immediate).unwrap_err();
    assert!(e.is_busy());
    assert!(!e.is_constraint());
    assert!(!e.is_full());

    b.extended_result_codes(true)?;
    let e = b.begin(TransactionBehavior::Immediate).unwrap_err();
    assert!(e.is_busy());

    tx.commit()?;
    Ok(())
}

#[test]
fn constraint_with_extended_codes() -> Result<()> {
    let mut c = Connection::open_in_memory()?;
    c.extended_result_codes(true)?;
    c.execute("CREATE TABLE users (name TEXT NOT NULL UNIQUE)")?;
    c.execute("INSERT INTO users VALUES ('Alice')")?;

    let e = c.execute("INSERT INTO users VALUES ('Alice')").unwrap_err();
    assert_eq!(e.code(), Code::CONSTRAINT_UNIQUE);
    assert!(e.is_constraint());
    assert!(!e.is_busy());

    let e = c.execute("INSERT INTO users VALUES (NULL)").unwrap_err();
    assert_eq!(e.code(), Code::CONSTRAINT_NOTNULL);
    assert!(e.is_constraint());
    Ok(())
}

#[derive(Debug)]
struct Outer(Inner);

impl fmt::Display for Outer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "outer")
    }
}

impl core::error::Error for Outer {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[derive(Debug)]
struct Inner;

impl fmt::Display for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "inner")
    }
}

impl core::error::Error for Inner {}

#[test]
fn conversion_keeps_source() {
    let e = Error::conversion(Outer(Inner));
    assert_eq!(e.code(), Code::MISMATCH);
    assert_eq!(e.to_string(), "outer");
    assert!(e.get_ref().is_some_and(|e| e.is::<Outer>()));

    let source = e.source().expect("source of outer");
    assert!(source.is::<Inner>());
    assert!(source.source().is_none());

    let e = Error::mismatch("plain");
    assert_eq!(e.code(), Code::MISMATCH);
    assert!(e.get_ref().is_none());
    assert!(e.source().is_none());
}

#[test]
fn error_is_send_sync() {
    fn assert<T: Send + Sync + 'static>() {}
    assert::<Error>();
}

/// A column type that fails to convert with a source error.
#[derive(Debug)]
struct Failing;

impl<'stmt> sqll::FromColumn<'stmt> for Failing {
    type Type = sqll::ty::Integer;

    fn from_column(_: &'stmt sqll::Statement, _: Self::Type) -> Result<Self> {
        Err(Error::conversion(Outer(Inner)))
    }
}

fn assert_outer_source(e: &Error) {
    assert_eq!(e.code(), Code::MISMATCH);
    assert!(e.get_ref().is_some_and(|e| e.is::<Outer>()));
    let source = e.source().expect("source of outer");
    assert!(source.is::<Inner>());
    assert!(source.source().is_none());
}

#[test]
fn type_check_names_column() -> Result<()> {
    let c = Connection::open_in_memory()?;
    let mut stmt = c.prepare("SELECT 1 AS id, NULL AS title, 'x'")?;
    assert!(stmt.step()?.is_row());

    let e = stmt.column::<String>(1).unwrap_err();
    assert_eq!(e.code(), Code::MISMATCH);
    assert_eq!(
        e.to_string(),
        "column 1 (`title`): expected TEXT but found NULL"
    );

    let e = stmt.unsized_column::<[u8]>(2).unwrap_err();
    assert_eq!(
        e.to_string(),
        "column 2 (`'x'`): expected BLOB but found TEXT"
    );

    let e = stmt.column::<sqll::Value<'_>>(1).unwrap_err();
    assert_eq!(
        e.to_string(),
        "column 1 (`title`): dynamic value has unsupported column type NULL"
    );
    Ok(())
}

#[test]
fn from_column_names_column_and_keeps_source() -> Result<()> {
    let c = Connection::open_in_memory()?;
    let mut stmt = c.prepare("SELECT 'a' AS name, 7 AS value")?;
    assert!(stmt.step()?.is_row());

    let e = stmt.column::<Failing>(1).unwrap_err();
    assert_eq!(e.to_string(), "column 1 (`value`): outer");
    assert_outer_source(&e);

    stmt.reset()?;
    let e = stmt.next::<(String, Failing)>().unwrap_err();
    assert_eq!(e.to_string(), "column 1 (`value`): outer");
    assert_outer_source(&e);

    stmt.reset()?;
    let e = stmt.next::<(i64, Failing)>().unwrap_err();
    assert_eq!(
        e.to_string(),
        "column 0 (`name`): expected INTEGER but found TEXT"
    );
    Ok(())
}

#[test]
fn row_derive_names_field() -> Result<()> {
    #[derive(Debug, sqll::Row)]
    #[allow(dead_code)]
    struct Named {
        name: String,
        r#value: Failing,
    }

    #[derive(Debug, sqll::Row)]
    #[allow(dead_code)]
    struct Checked {
        name: String,
        title: String,
    }

    #[derive(Debug, sqll::Row)]
    #[allow(dead_code)]
    struct Tuple(String, Failing);

    let c = Connection::open_in_memory()?;
    let mut stmt = c.prepare("SELECT 'a' AS name, 7 AS value")?;

    let e = stmt.next::<Named>().unwrap_err();
    assert_eq!(
        e.to_string(),
        "field `value` from column 1 (`value`): outer"
    );
    assert_outer_source(&e);

    stmt.reset()?;
    let e = stmt.next::<Tuple>().unwrap_err();
    assert_eq!(e.to_string(), "field `1` from column 1 (`value`): outer");
    assert_outer_source(&e);

    stmt.reset()?;
    let e = stmt.next::<Checked>().unwrap_err();
    assert_eq!(
        e.to_string(),
        "field `title` from column 1 (`value`): expected TEXT but found INTEGER"
    );
    Ok(())
}

#[test]
fn context_is_kept_by_first_writer() {
    let e = Error::conversion(Outer(Inner))
        .with_field("inner")
        .with_field("outer");
    assert_eq!(e.to_string(), "field `inner`: outer");
    assert_outer_source(&e);
}
