#![allow(unused)]

use core::error::Error as _;
use core::fmt;

use anyhow::Result;
use sqll::{OpenOptions, PoolBuilder, PoolError, Prepared, SendStatement, Statements};

#[derive(Statements)]
struct TupleStatements(
    #[sql = "SELECT 1"] SendStatement,
    #[sql = "CREATE TABLE foo (id INTEGER)"] SendStatement,
);

#[derive(Statements)]
#[sql(read_only)]
struct TupleReadOnlyStatements(#[sql = "SELECT 1"] SendStatement);

#[derive(Statements)]
struct Write {
    #[sql = "SELECT 1"]
    select_one: SendStatement,
    #[sql = "CREATE TABLE foo (id INTEGER)"]
    create_table: SendStatement,
}

#[derive(Statements)]
#[sql(read_only)]
struct Read {
    #[sql = "SELECT 1"]
    select_one: SendStatement,
}

#[derive(Statements)]
struct WriteWithRead {
    #[sql(statements)]
    read: Read,
    #[sql = "CREATE TABLE foo (id INTEGER)"]
    create_table: SendStatement,
}

// A read-only collection may embed another read-only collection.
#[derive(Statements)]
#[sql(read_only)]
struct ReadWithRead {
    #[sql(statements)]
    inner: Read,
    #[sql = "SELECT 2"]
    select_two: SendStatement,
}

#[test]
fn test_pool() -> Result<()> {
    let mut c = OpenOptions::new();
    c.create();

    let temp = tempfile::TempDir::new()?;

    let _pool = PoolBuilder::new(c, 4).open::<Read, Write>(temp.path().join("test.db"))?;
    Ok(())
}

#[test]
fn test_nested_statements() -> Result<()> {
    let mut c = OpenOptions::new();
    c.create();

    let temp = tempfile::TempDir::new()?;

    // Build a pool whose read and write sides both contain nested statement
    // collections, exercising the recursive `Statements::build`.
    let _pool =
        PoolBuilder::new(c, 4).open::<ReadWithRead, WriteWithRead>(temp.path().join("test.db"))?;
    Ok(())
}

#[derive(Statements)]
#[sql(read_only)]
struct NotReadOnly {
    #[sql = "SELECT 1"]
    select_one: SendStatement,
    #[sql = "CREATE TABLE foo (id INTEGER)"]
    create_table: SendStatement,
}

#[test]
fn test_not_read_only() -> Result<()> {
    let mut c = OpenOptions::new();
    c.create();

    let temp = tempfile::TempDir::new()?;

    assert!(
        PoolBuilder::new(c, 4)
            .open::<NotReadOnly, Write>(temp.path().join("test.db"))
            .is_err()
    );
    Ok(())
}

// Statements that reference a table which only exists if the connection setup
// created it.
#[derive(Statements)]
#[sql(read_only)]
struct ReadItems {
    #[sql = "SELECT id FROM items"]
    select: SendStatement,
}

#[derive(Statements)]
struct WriteItems {
    #[sql = "INSERT INTO items (id) VALUES (?)"]
    insert: SendStatement,
}

#[test]
fn test_setup_creates_schema() -> Result<()> {
    let mut options = OpenOptions::new();
    options.create();

    let temp = tempfile::TempDir::new()?;

    // Without any setup, preparing `SELECT id FROM items` fails because the
    // table does not exist yet.
    assert!(
        PoolBuilder::new(options, 2)
            .open::<ReadItems, WriteItems>(temp.path().join("missing.db"))
            .is_err()
    );

    // The write setup runs on the write connection before any statement is
    // prepared and before the read connections are opened, so creating the
    // table there makes both the write and read statements preparable. This
    // also guards against the read/write setup being applied to the wrong
    // connection.
    let _pool = PoolBuilder::new(options, 2)
        .with_write_setup(|c| c.execute("CREATE TABLE items (id INTEGER)"))
        .open::<ReadItems, WriteItems>(temp.path().join("created.db"))?;
    Ok(())
}

#[test]
fn test_setup_error_surfaces() -> Result<()> {
    let mut options = OpenOptions::new();
    options.create();

    let temp = tempfile::TempDir::new()?;

    // An error returned from the setup closure aborts pool construction. A
    // closure ending in `Ok(())` names its error type, since the setup accepts
    // any error.
    let result = PoolBuilder::new(options, 1)
        .with_write_setup(|c| {
            c.execute("THIS IS NOT VALID SQL")?;
            Ok::<_, sqll::Error>(())
        })
        .open::<Read, Write>(temp.path().join("test.db"));

    let Err(error) = result else {
        panic!("expected an error");
    };

    assert_eq!(error.to_string(), "setting up write connection");
    let source = error.source().expect("setup error is the source");
    assert!(source.downcast_ref::<sqll::Error>().is_some());
    Ok(())
}

#[derive(Debug)]
struct MigrationError(u32);

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "migration #{} failed", self.0)
    }
}

impl core::error::Error for MigrationError {}

#[test]
fn test_setup_custom_error() -> Result<()> {
    let mut options = OpenOptions::new();
    options.create();

    let temp = tempfile::TempDir::new()?;
    let path = temp.path().join("test.db");

    // The write setup can fail with the application's own error type, which
    // comes back as the source of the pool error.
    let result = PoolBuilder::new(options, 1)
        .with_write_setup(|_| Err(MigrationError(3)))
        .open::<Read, Write>(&path);

    let Err(error) = result else {
        panic!("expected an error");
    };

    let source = error.source().expect("setup error is the source");
    assert_eq!(source.to_string(), "migration #3 failed");
    assert!(matches!(
        source.downcast_ref::<MigrationError>(),
        Some(MigrationError(3))
    ));

    // The same for the read setup, which runs on every read connection.
    let result = PoolBuilder::new(options, 2)
        .with_read_setup(|_| Err(MigrationError(4)))
        .open::<Read, Write>(&path);

    let Err(error) = result else {
        panic!("expected an error");
    };

    assert_eq!(error.to_string(), "setting up read connection #0");
    let source = error.source().expect("setup error is the source");
    assert!(matches!(
        source.downcast_ref::<MigrationError>(),
        Some(MigrationError(4))
    ));

    // And with anyhow, whose error converts into the boxed source.
    let result = PoolBuilder::new(options, 1)
        .with_write_setup(|_| Err(anyhow::anyhow!("schema is from the future")))
        .open::<Read, Write>(&path);

    let Err(error) = result else {
        panic!("expected an error");
    };

    let source = error.source().expect("setup error is the source");
    assert_eq!(source.to_string(), "schema is from the future");
    Ok(())
}

// Queries customised through a trait of associated constants with defaults, as
// a library would expose them so that downstreams can override its SQL.
trait Queries: 'static {
    const SELECT: &'static str = "SELECT id FROM items";
    const INSERT: &'static str = "INSERT INTO items (id) VALUES (?)";
}

struct Defaults;

impl Queries for Defaults {}

struct Custom;

impl Queries for Custom {
    const SELECT: &'static str = "SELECT id FROM items WHERE id > 0";
}

const SELECT_ONE: &str = "SELECT 1";

#[derive(Statements)]
#[sql(read_only)]
struct GenericRead<Q: Queries = Defaults> {
    #[sql(query = Q::SELECT)]
    select: SendStatement,
    #[sql(query = SELECT_ONE)]
    select_one: SendStatement,
    #[sql = "SELECT"]
    #[sql = "2"]
    select_two: SendStatement,
    #[sql(skip)]
    _marker: core::marker::PhantomData<Q>,
}

#[derive(Statements)]
struct GenericWrite<Q>
where
    Q: Queries,
{
    #[sql(statements)]
    read: GenericRead<Q>,
    #[sql(query = Q::INSERT)]
    insert: SendStatement,
}

// An application embedding the generic collections into its own.
#[derive(Statements)]
#[sql(read_only)]
struct AppRead {
    #[sql(statements)]
    items: GenericRead<Custom>,
    #[sql(statements)]
    defaults: GenericRead,
}

#[derive(Statements)]
struct AppWrite {
    #[sql(statements)]
    items: GenericWrite<Custom>,
}

#[test]
fn test_generic_const_queries() -> Result<()> {
    let mut options = OpenOptions::new();
    options.create();

    let temp = tempfile::TempDir::new()?;

    // The pool builds every generic collection, read and write.
    let _pool = PoolBuilder::new(options, 2)
        .with_write_setup(|c| c.execute("CREATE TABLE items (id INTEGER)"))
        .open::<AppRead, AppWrite>(temp.path().join("pool.db"))?;

    let c = OpenOptions::new().create().read_write().open_in_memory()?;
    c.execute("CREATE TABLE items (id INTEGER)")?;

    let mut write = Prepared::<AppWrite>::new(c)?;
    write.items.insert.execute(0)?;
    write.items.insert.execute(7)?;

    let read = &mut write.items.read;
    read.select.bind(())?;
    assert_eq!(
        read.select.iter::<i64>().collect::<Result<Vec<_>, _>>()?,
        [7]
    );
    read.select_one.bind(())?;
    assert_eq!(
        read.select_one
            .iter::<i64>()
            .collect::<Result<Vec<_>, _>>()?,
        [1]
    );
    read.select_two.bind(())?;
    assert_eq!(
        read.select_two
            .iter::<i64>()
            .collect::<Result<Vec<_>, _>>()?,
        [2]
    );

    let (write, c) = write.into_parts();
    drop(write);

    let mut defaults = Prepared::<GenericRead>::new(c)?;
    defaults.select.bind(())?;
    assert_eq!(
        defaults
            .select
            .iter::<i64>()
            .collect::<Result<Vec<_>, _>>()?,
        [0, 7]
    );
    Ok(())
}

struct Mutating;

impl Queries for Mutating {
    const SELECT: &'static str = "DELETE FROM items";
}

#[test]
fn test_generic_const_query_not_read_only() -> Result<()> {
    let c = OpenOptions::new().create().read_write().open_in_memory()?;
    c.execute("CREATE TABLE items (id INTEGER)")?;

    // The read-only check of a query expression happens when the collection is
    // built, since its value is only known once the type parameter is.
    let Err(error) = Prepared::<GenericRead<Mutating>>::new(c) else {
        panic!("expected an error");
    };

    assert_eq!(error.to_string(), "field select is not read-only");
    Ok(())
}
