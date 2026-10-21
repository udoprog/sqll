//! Connection settings applied by [`OpenOptions`] when opening.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;
use sqll::{Code, Connection, JournalMode, OpenOptions, Synchronous};
use tempfile::TempDir;

fn journal_mode(c: &Connection) -> Result<String> {
    let mut stmt = c.prepare("PRAGMA journal_mode")?;
    assert!(stmt.step()?.is_row());
    Ok(stmt.column::<String>(0)?)
}

fn pragma_i64(c: &Connection, pragma: &str) -> Result<i64> {
    let mut stmt = c.prepare(pragma)?;
    assert!(stmt.step()?.is_row());
    Ok(stmt.column::<i64>(0)?)
}

fn create(path: &Path) -> Result<()> {
    let c = OpenOptions::new().create().read_write().open(path)?;
    c.execute("CREATE TABLE t (n INTEGER)")?;
    Ok(())
}

#[test]
fn defaults_leave_settings_alone() -> Result<()> {
    let c = OpenOptions::new().create().read_write().open_in_memory()?;
    assert_eq!(pragma_i64(&c, "PRAGMA foreign_keys")?, 0);
    assert_eq!(pragma_i64(&c, "PRAGMA synchronous")?, 2);
    Ok(())
}

#[test]
fn foreign_keys() -> Result<()> {
    let c = OpenOptions::new()
        .create()
        .read_write()
        .foreign_keys(true)
        .open_in_memory()?;
    assert_eq!(pragma_i64(&c, "PRAGMA foreign_keys")?, 1);

    let c = OpenOptions::new()
        .create()
        .read_write()
        .foreign_keys(false)
        .open_in_memory()?;
    assert_eq!(pragma_i64(&c, "PRAGMA foreign_keys")?, 0);
    Ok(())
}

#[test]
fn synchronous() -> Result<()> {
    for (setting, expected) in [
        (Synchronous::Off, 0),
        (Synchronous::Normal, 1),
        (Synchronous::Full, 2),
        (Synchronous::Extra, 3),
    ] {
        let c = OpenOptions::new()
            .create()
            .read_write()
            .synchronous(setting)
            .open_in_memory()?;
        assert_eq!(pragma_i64(&c, "PRAGMA synchronous")?, expected);
    }

    Ok(())
}

#[test]
fn journal_mode_on_file() -> Result<()> {
    let dir = TempDir::new()?;

    for (mode, expected) in [
        (JournalMode::Delete, "delete"),
        (JournalMode::Truncate, "truncate"),
        (JournalMode::Persist, "persist"),
        (JournalMode::Memory, "memory"),
        (JournalMode::Wal, "wal"),
        (JournalMode::Off, "off"),
    ] {
        let path = dir.path().join(format!("{expected}.db"));

        let c = OpenOptions::new()
            .create()
            .read_write()
            .journal_mode(mode)
            .open(&path)?;
        assert_eq!(journal_mode(&c)?, expected);
    }

    Ok(())
}

/// Setting the journal mode is a write, so it is skipped on a read-only
/// connection instead of failing.
#[test]
fn journal_mode_skipped_on_read_only() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("db.db");
    create(&path)?;

    let r = OpenOptions::new()
        .read_only()
        .journal_mode(JournalMode::Wal)
        .open(&path)?;
    assert_eq!(journal_mode(&r)?, "delete");
    drop(r);

    // A writer sets WAL and a reader opened with the same options sees it.
    let mut options = OpenOptions::new();
    options.journal_mode(JournalMode::Wal);

    let w = options.clone().read_write().open(&path)?;
    assert_eq!(journal_mode(&w)?, "wal");

    let r = options.clone().read_only().open(&path)?;
    assert_eq!(journal_mode(&r)?, "wal");
    Ok(())
}

/// An in-memory database keeps its mode when asked for one it cannot use.
#[test]
fn journal_mode_in_memory() -> Result<()> {
    let c = OpenOptions::new()
        .create()
        .read_write()
        .journal_mode(JournalMode::Wal)
        .open_in_memory()?;
    assert_eq!(journal_mode(&c)?, "memory");
    Ok(())
}

/// A setting that fails makes opening fail, and the busy timeout is applied
/// before the journal mode so that it waits on a locked database.
#[test]
fn busy_timeout_applies_first() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("db.db");
    create(&path)?;

    let lock = OpenOptions::new().read_write().open(&path)?;
    lock.execute("BEGIN EXCLUSIVE")?;

    let e = OpenOptions::new()
        .read_write()
        .journal_mode(JournalMode::Wal)
        .open(&path)
        .unwrap_err();
    assert_eq!(e.code(), Code::BUSY);

    let start = Instant::now();

    let e = OpenOptions::new()
        .read_write()
        .busy_timeout(Duration::from_millis(200))
        .journal_mode(JournalMode::Wal)
        .open(&path)
        .unwrap_err();
    assert_eq!(e.code(), Code::BUSY);
    assert!(start.elapsed() >= Duration::from_millis(150));

    lock.execute("COMMIT")?;

    let c = OpenOptions::new()
        .read_write()
        .busy_timeout(Duration::from_millis(200))
        .journal_mode(JournalMode::Wal)
        .open(&path)?;
    assert_eq!(journal_mode(&c)?, "wal");
    Ok(())
}

#[test]
fn busy_timeout_saturates() -> Result<()> {
    OpenOptions::new()
        .create()
        .read_write()
        .busy_timeout(Duration::MAX)
        .open_in_memory()?;
    Ok(())
}

/// Pool connections get the settings without a setup hook, and the read-only
/// readers skip the journal mode.
#[cfg(feature = "pool")]
#[test]
fn pool_connections() -> Result<()> {
    use sqll::{PoolBuilder, Statements, TypedStatement};

    #[derive(Statements)]
    #[sql(read_only)]
    struct Read {
        #[sql = "PRAGMA foreign_keys"]
        foreign_keys: TypedStatement<(), i64>,
        #[sql = "SELECT journal_mode FROM pragma_journal_mode"]
        journal_mode: TypedStatement<(), String>,
    }

    #[derive(Statements)]
    struct Write {
        #[sql = "PRAGMA journal_mode"]
        journal_mode: TypedStatement<(), String>,
    }

    let dir = TempDir::new()?;
    let path = dir.path().join("pool.db");
    create(&path)?;

    let mut options = OpenOptions::new();
    options
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true)
        .journal_mode(JournalMode::Wal)
        .synchronous(Synchronous::Normal);

    let mut pool = PoolBuilder::new(options, 2).open::<Read, Write>(&path)?;

    let write = pool.as_write_mut();
    assert_eq!(write.journal_mode.query()?.first()?.as_deref(), Some("wal"));

    let read = pool.as_read_mut();
    assert_eq!(read.foreign_keys.query()?.first()?, Some(1));
    assert_eq!(read.journal_mode.query()?.first()?.as_deref(), Some("wal"));
    Ok(())
}
