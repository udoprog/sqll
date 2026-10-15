//! The `Statements` derive used through a renamed path to the `sqll` crate
//! with `#[sql(crate = ..)]`.

#![allow(unused)]

extern crate sqll as renamed;

use anyhow::Result;
use renamed::{OpenOptions, PoolBuilder, SendStatement, Statements};

#[derive(Statements)]
#[sql(read_only, crate = renamed)]
struct Read {
    #[sql = "SELECT 1"]
    select_one: SendStatement,
}

// Separate attributes, with the read-only side nesting another collection.
#[derive(Statements)]
#[sql(read_only)]
#[sql(crate = ::renamed)]
struct ReadWithRead {
    #[sql(statements)]
    inner: Read,
    #[sql = "SELECT 2"]
    select_two: SendStatement,
}

#[derive(Statements)]
#[sql(crate = renamed)]
struct WriteWithRead {
    #[sql(statements)]
    read: Read,
    #[sql = "CREATE TABLE foo (id INTEGER)"]
    create_table: SendStatement,
}

#[derive(Statements)]
#[sql(crate = renamed)]
struct Tuple(#[sql = "SELECT 1"] SendStatement);

#[test]
fn test_renamed_crate() -> Result<()> {
    let mut c = OpenOptions::new();
    c.no_mutex().create();

    let temp = tempfile::TempDir::new()?;

    let _pool =
        PoolBuilder::new(c, 4).open::<ReadWithRead, WriteWithRead>(temp.path().join("test.db"))?;
    Ok(())
}
