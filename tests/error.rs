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
