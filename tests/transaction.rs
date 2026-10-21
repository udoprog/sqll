use std::panic::{AssertUnwindSafe, catch_unwind};

use sqll::{Code, Connection, Error, Result, TransactionBehavior};

fn count(c: &Connection) -> Result<i64> {
    let mut stmt = c.prepare("SELECT COUNT(*) FROM users")?;
    stmt.first::<i64>(()).map(|row| row.unwrap_or_default())
}

fn setup() -> Result<Connection> {
    let c = Connection::open_in_memory()?;
    c.execute("CREATE TABLE users (name TEXT)")?;
    Ok(c)
}

#[test]
fn commit_on_ok() -> Result<()> {
    let c = setup()?;
    c.transaction(|c| c.execute("INSERT INTO users VALUES ('Alice')"))?;
    assert_eq!(count(&c)?, 1);
    Ok(())
}

#[test]
fn rollback_on_err() -> Result<()> {
    let c = setup()?;

    let result = c.transaction(|c| {
        c.execute("INSERT INTO users VALUES ('Alice')")?;
        Err::<(), _>(Error::custom("nope"))
    });

    assert_eq!(result.unwrap_err().code(), Code::ERROR);
    assert_eq!(count(&c)?, 0);

    // The connection is usable for another transaction afterwards.
    c.transaction(|c| c.execute("INSERT INTO users VALUES ('Bob')"))?;
    assert_eq!(count(&c)?, 1);
    Ok(())
}

#[test]
fn rollback_on_panic() -> Result<()> {
    let c = setup()?;

    let result = catch_unwind(AssertUnwindSafe(|| {
        c.transaction(|c| -> Result<()> {
            c.execute("INSERT INTO users VALUES ('Alice')")?;
            panic!("boom");
        })
    }));

    assert!(result.is_err());
    assert_eq!(count(&c)?, 0);

    c.transaction(|c| c.execute("INSERT INTO users VALUES ('Bob')"))?;
    assert_eq!(count(&c)?, 1);
    Ok(())
}

#[test]
fn commit_failure_rolls_back() -> Result<()> {
    let c = Connection::open_in_memory()?;

    c.execute(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE parent (id INTEGER PRIMARY KEY);
        CREATE TABLE child (
            parent INTEGER REFERENCES parent (id) DEFERRABLE INITIALLY DEFERRED
        );
        "#,
    )?;

    // The deferred foreign key is only checked by COMMIT, which fails and
    // leaves the transaction open.
    let result = c.transaction(|c| c.execute("INSERT INTO child VALUES (1)"));
    assert_eq!(result.unwrap_err().code().base(), Code::CONSTRAINT);

    // The transaction was rolled back rather than left open.
    let mut stmt = c.prepare("SELECT COUNT(*) FROM child")?;
    assert_eq!(stmt.first::<i64>(())?, Some(0));
    drop(stmt);

    c.transaction(|c| c.execute("INSERT INTO parent VALUES (1); INSERT INTO child VALUES (1)"))?;
    Ok(())
}

#[test]
fn nested_fails() -> Result<()> {
    let c = setup()?;

    let result = c.transaction(|c| {
        c.execute("INSERT INTO users VALUES ('Alice')")?;
        c.transaction(|c| c.execute("INSERT INTO users VALUES ('Bob')"))
    });

    assert!(result.is_err());
    assert_eq!(count(&c)?, 0);
    Ok(())
}

#[test]
fn guard_drop_rolls_back() -> Result<()> {
    let c = setup()?;

    for behavior in [
        TransactionBehavior::Deferred,
        TransactionBehavior::Immediate,
        TransactionBehavior::Exclusive,
    ] {
        let tx = c.begin(behavior)?;
        tx.execute("INSERT INTO users VALUES ('Alice')")?;
        drop(tx);
        assert_eq!(count(&c)?, 0);
    }

    let tx = c.begin(TransactionBehavior::default())?;
    tx.execute("INSERT INTO users VALUES ('Alice')")?;
    tx.commit()?;
    assert_eq!(count(&c)?, 1);
    Ok(())
}

#[test]
fn immediate_takes_write_lock() -> Result<()> {
    let dir = tempfile::tempdir().map_err(Error::custom)?;
    let path = dir.path().join("test.db");

    let a = Connection::open(&path)?;
    a.execute("CREATE TABLE users (name TEXT)")?;
    let b = Connection::open(&path)?;

    let tx = a.begin(TransactionBehavior::Immediate)?;
    // A second writer cannot start while the first holds the write lock.
    let e = b.begin(TransactionBehavior::Immediate).unwrap_err();
    assert_eq!(e.code().base(), Code::BUSY);
    // A deferred transaction can start, since it has not taken a lock yet.
    b.begin(TransactionBehavior::Deferred)?.rollback()?;
    tx.commit()?;

    b.begin(TransactionBehavior::Immediate)?.commit()?;
    Ok(())
}
