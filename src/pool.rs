use core::cell::UnsafeCell;
use core::error::Error;
use core::ffi::CStr;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, Ordering};

use alloc::boxed::Box;
#[cfg(feature = "std")]
use alloc::ffi::CString;
use alloc::sync::Arc;
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::path::Path;

use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};

use crate::{Connection, Error as SqllError, NotThreadSafe, OpenOptions};

/// The error a [`ConnectionSetup`] hook fails with, boxed so that the hook can
/// return any error type, such as the application's own error or
/// [`anyhow::Error`].
///
/// [`anyhow::Error`]: https://docs.rs/anyhow/latest/anyhow/struct.Error.html
type SetupError = Box<dyn Error + Send + Sync + 'static>;

mod sealed_connection_setup {
    use crate::Connection;

    use super::{EmptySetup, SetupError};

    pub trait Sealed {}

    impl Sealed for EmptySetup {}
    impl<F, E> Sealed for F
    where
        F: Fn(&mut Connection) -> Result<(), E>,
        E: Into<SetupError>,
    {
    }
}

/// The trait governing how a [`Connection`] is set up before its statements are
/// prepared.
///
/// This is used by [`PoolBuilder::with_write_setup`] and
/// [`PoolBuilder::with_read_setup`] to run setup logic (such as `PRAGMA`s,
/// creating tables or running migrations) on a connection before the
/// corresponding [`Statements`] are prepared on it.
///
/// It is implemented for any `Fn(&mut Connection) -> Result<(), E>` where `E`
/// converts into `Box<dyn Error + Send + Sync>`. That covers every error type
/// implementing [`core::error::Error`] + `Send` + `Sync` + `'static`, such as
/// [`sqll::Error`], as well as [`anyhow::Error`]. If the hook fails, the
/// returned error is the [`source`] of the [`PoolError`] that opening the pool
/// fails with.
///
/// The trait is sealed so it cannot be implemented for other types.
///
/// [`sqll::Error`]: crate::Error
/// [`anyhow::Error`]: https://docs.rs/anyhow/latest/anyhow/struct.Error.html
/// [`source`]: core::error::Error::source
pub trait ConnectionSetup
where
    Self: self::sealed_connection_setup::Sealed,
{
    /// Run the setup against the given connection.
    fn setup(&self, c: &mut Connection) -> Result<(), Box<dyn Error + Send + Sync + 'static>>;
}

/// An empty connection setup that does nothing.
#[non_exhaustive]
pub struct EmptySetup;

impl ConnectionSetup for EmptySetup {
    #[inline]
    fn setup(&self, _c: &mut Connection) -> Result<(), SetupError> {
        Ok(())
    }
}

impl<F, E> ConnectionSetup for F
where
    F: Fn(&mut Connection) -> Result<(), E>,
    E: Into<SetupError>,
{
    #[inline]
    fn setup(&self, c: &mut Connection) -> Result<(), SetupError> {
        self(c).map_err(Into::into)
    }
}

/// Builder for a [`Pool`]. The pool needs to know how to prepare the statements
/// for the read and write connections, and this builder provides the necessary
/// information. The pool will prepare the statements on each connection up
/// front, so the schema of the database must be compatible with the statements
/// in the builder at the time the pool is constructed.
pub struct PoolBuilder<RB, WB> {
    open_options: OpenOptions,
    read_concurrency: usize,
    write_builder: WB,
    read_builder: RB,
}

impl PoolBuilder<EmptySetup, EmptySetup> {
    /// Construct a builder for the pool.
    ///
    /// See [`PoolBuilder::open`] for more details and examples.
    pub fn new(open_options: OpenOptions, read_concurrency: usize) -> Self {
        Self {
            open_options,
            read_concurrency,
            write_builder: EmptySetup,
            read_builder: EmptySetup,
        }
    }
}

impl<RB, WB> PoolBuilder<RB, WB>
where
    RB: ConnectionSetup,
    WB: ConnectionSetup,
{
    /// Set the connection setup for the write connection.
    ///
    /// This is called exactly once on the single `read_write` connection,
    /// before its statements are prepared and before any of the read
    /// connections are opened. It is the right place to create or migrate the
    /// schema that the read and write statements depend on, so there is no
    /// need to open a separate connection to migrate the database before
    /// building the pool.
    ///
    /// The closure may fail with any error that converts into `Box<dyn Error +
    /// Send + Sync>`, such as [`sqll::Error`], the application's own error type
    /// or [`anyhow::Error`]. If it does, [`open`] fails with a [`PoolError`]
    /// whose [`source`] is the returned error.
    ///
    /// Since the error type is generic, a closure that uses `?` and ends in
    /// `Ok(())` has to name it, as in `Ok::<_, sqll::Error>(())`. A closure
    /// that ends in a call returning a [`Result`], or a function with a
    /// declared return type as below, needs no annotation.
    ///
    /// [`open`]: PoolBuilder::open
    /// [`sqll::Error`]: crate::Error
    /// [`anyhow::Error`]: https://docs.rs/anyhow/latest/anyhow/struct.Error.html
    /// [`source`]: core::error::Error::source
    ///
    /// # Examples
    ///
    /// Migrating the schema with the application's own error handling before
    /// the statements, which depend on the migrated schema, are prepared:
    ///
    /// ```
    /// use anyhow::Context;
    /// use sqll::{Connection, OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name, email FROM users ORDER BY name"]
    ///     users: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name, email) VALUES (?, ?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// const MIGRATIONS: &[&str] = &[
    ///     "CREATE TABLE users (name TEXT)",
    ///     "ALTER TABLE users ADD COLUMN email TEXT",
    /// ];
    ///
    /// fn migrate(c: &mut Connection) -> anyhow::Result<()> {
    ///     let mut stmt = c.prepare("PRAGMA user_version")?;
    ///     let version = stmt.next::<i64>()?.context("missing user_version")?;
    ///     drop(stmt);
    ///
    ///     for (n, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
    ///         c.execute(sql).with_context(|| format!("running migration #{n}"))?;
    ///         c.execute(format!("PRAGMA user_version = {}", n + 1))?;
    ///     }
    ///
    ///     Ok(())
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(migrate)
    ///     .open::<Read, Write>(&path)?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    ///
    /// A failing setup is the source of the error opening the pool:
    ///
    /// ```
    /// use core::error::Error as _;
    ///
    /// use sqll::{OpenOptions, PoolBuilder, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {}
    ///
    /// #[derive(Statements)]
    /// struct Write {}
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let Err(error) = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|_| Err(anyhow::anyhow!("schema is from the future")))
    ///     .open::<Read, Write>(&path)
    /// else {
    ///     panic!("expected an error");
    /// };
    ///
    /// assert_eq!(error.to_string(), "setting up write connection");
    /// assert_eq!(error.source().unwrap().to_string(), "schema is from the future");
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    pub fn with_write_setup<T, E>(self, write_builder: T) -> PoolBuilder<RB, T>
    where
        T: Fn(&mut Connection) -> Result<(), E>,
        E: Into<Box<dyn Error + Send + Sync + 'static>>,
    {
        PoolBuilder {
            open_options: self.open_options,
            read_concurrency: self.read_concurrency,
            read_builder: self.read_builder,
            write_builder,
        }
    }

    /// Set the connection setup for the read connections.
    ///
    /// This is called once on each of the `read_concurrency` read-only
    /// connections, after the write connection has been set up, and before the
    /// read statements are prepared on that connection. Since read connections
    /// are opened `read_only`, this should not attempt to modify the database.
    ///
    /// The closure may fail with any error that converts into `Box<dyn Error +
    /// Send + Sync>`, as with [`with_write_setup`]. If it does, [`open`] fails
    /// with a [`PoolError`] whose [`source`] is the returned error.
    ///
    /// [`open`]: PoolBuilder::open
    /// [`with_write_setup`]: PoolBuilder::with_write_setup
    /// [`source`]: core::error::Error::source
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name FROM users"]
    ///     users: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {}
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| c.execute("CREATE TABLE users (name TEXT)"))
    ///     .with_read_setup(|c| c.execute("PRAGMA cache_size = -16000"))
    ///     .open::<Read, Write>(&path)?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    pub fn with_read_setup<T, E>(self, read_builder: T) -> PoolBuilder<T, WB>
    where
        T: Fn(&mut Connection) -> Result<(), E>,
        E: Into<Box<dyn Error + Send + Sync + 'static>>,
    {
        PoolBuilder {
            open_options: self.open_options,
            read_concurrency: self.read_concurrency,
            read_builder,
            write_builder: self.write_builder,
        }
    }

    /// Build the pool with the specified path.
    ///
    /// See [`Pool::new`] for more details and examples.
    ///
    /// [`Pool::new`]: Pool::new
    #[inline]
    #[cfg(feature = "std")]
    pub fn open<R, W>(self, path: impl AsRef<Path>) -> Result<Pool<R, W>, PoolError>
    where
        R: IsReadOnly,
        W: Statements,
    {
        let path = path.as_ref();

        let Some(bytes) = path.to_str() else {
            return Err(PoolError::from(ErrorKind::NotUtf8Path));
        };

        let Ok(string) = CString::new(bytes) else {
            return Err(PoolError::from(ErrorKind::NulByteInPath));
        };

        Pool::new_c_str(
            self.open_options,
            &string,
            self.read_concurrency,
            self.write_builder,
            self.read_builder,
        )
    }

    /// Build the pool with the specified path as a C string.
    ///
    /// See [`Pool::new_c_str`] for more details and examples.
    ///
    /// [`Pool::new_c_str`]: Pool::new_c_str
    pub fn open_c_str<R, W>(self, path: &CStr) -> Result<Pool<R, W>, PoolError>
    where
        R: IsReadOnly,
        W: Statements,
    {
        Pool::new_c_str(
            self.open_options,
            path,
            self.read_concurrency,
            self.write_builder,
            self.read_builder,
        )
    }
}

/// A collection of prepared statements that can be built from a [`Connection`].
///
/// This is what the read (`R`) and write (`W`) sides of a [`Pool`] are made of.
/// Each field of the implementing type is a [`SendStatement`] prepared from a
/// `#[sql = "..."]` attribute. See the [`Statements` derive] for the available
/// attributes and a complete example.
///
/// Implementing this trait by hand is an error, it should only be implemented
/// by the [`Statements` derive] macro.
///
/// [`Statements` derive]: derive@crate::Statements
/// [`SendStatement`]: crate::SendStatement
pub trait Statements
where
    Self: Sized,
{
    #[doc(hidden)]
    fn build(c: &mut Connection) -> Result<Self, PoolError>;
}

/// A marker trait for [`Statements`] that only ever read from the database.
///
/// This is required for the read side `R` of a [`Pool`], since it is what allows
/// multiple [`shared`] guards to access distinct connections concurrently. It is
/// implemented automatically by the [`Statements` derive] for types annotated
/// with `#[sql(read_only)]`.
///
/// [`shared`]: Pool::shared
/// [`Statements` derive]: derive@crate::Statements
///
/// # Safety
///
/// The implementer must guarantee that none of the statements in the collection
/// mutate the database. If a mutating statement is run through a [`shared`]
/// guard it may execute concurrently with other readers, which SQLite does not
/// permit on a single connection and which can lead to data races or
/// corruption. The [`Statements` derive] upholds this by rejecting any
/// statement for which [`Statement::is_read_only`] returns `false`.
///
/// [`Statement::is_read_only`]: crate::Statement::is_read_only
pub unsafe trait IsReadOnly: Statements {}

/// Errors raised by the high-level [`Pool`] API.
///
/// This covers failures while opening the underlying connections, preparing the
/// statements of a [`Statements`] collection, converting them into a
/// thread-safe form, or asserting that a `#[sql(read_only)]` collection really
/// is read-only.
pub struct PoolError {
    kind: ErrorKind,
}

impl PoolError {
    #[inline]
    #[doc(hidden)]
    pub fn index_not_read_only(index: usize) -> Self {
        Self::from(ErrorKind::IndexNotReadOnly(index))
    }

    #[inline]
    #[doc(hidden)]
    pub fn field_not_read_only(name: &'static str) -> Self {
        Self::from(ErrorKind::FieldNotReadOnly(name))
    }
    #[inline]
    #[doc(hidden)]
    pub fn index_prepare_failed(index: usize, source: SqllError) -> Self {
        Self::from(ErrorKind::IndexPrepareFailed(index, source))
    }

    #[inline]
    #[doc(hidden)]
    pub fn field_prepare_failed(name: &'static str, source: SqllError) -> Self {
        Self::from(ErrorKind::FieldPrepareFailed(name, source))
    }

    #[inline]
    #[doc(hidden)]
    pub fn index_not_thread_safe(index: usize, source: NotThreadSafe) -> Self {
        Self::from(ErrorKind::IndexNotThreadSafe(index, source))
    }

    #[inline]
    #[doc(hidden)]
    pub fn field_not_thread_safe(name: &'static str, source: NotThreadSafe) -> Self {
        Self::from(ErrorKind::FieldNotThreadSafe(name, source))
    }

    #[inline]
    #[doc(hidden)]
    pub fn index_build_failed(index: usize, source: impl Error + Send + Sync + 'static) -> Self {
        Self::from(ErrorKind::IndexBuildFailed(index, Box::new(source)))
    }

    #[inline]
    #[doc(hidden)]
    pub fn field_build_failed(
        name: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::from(ErrorKind::FieldBuildFailed(name, Box::new(source)))
    }
}

impl fmt::Display for PoolError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Debug for PoolError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl Error for PoolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind {
            ErrorKind::BuildWriteConnection(ref source) => Some(source),
            ErrorKind::BuildReadConnection(_, ref source) => Some(source),
            ErrorKind::SetupWriteConnection(ref source) => Some(&**source),
            ErrorKind::SetupReadConnection(_, ref source) => Some(&**source),
            ErrorKind::IndexPrepareFailed(_, ref source) => Some(source),
            ErrorKind::FieldPrepareFailed(_, ref source) => Some(source),
            ErrorKind::IndexNotThreadSafe(_, ref source) => Some(source),
            ErrorKind::FieldNotThreadSafe(_, ref source) => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum ErrorKind {
    #[cfg(feature = "std")]
    NotUtf8Path,
    #[cfg(feature = "std")]
    NulByteInPath,
    BuildWriteConnection(SqllError),
    BuildReadConnection(usize, SqllError),
    SetupWriteConnection(SetupError),
    SetupReadConnection(usize, SetupError),
    NoConnections,
    ZeroConcurrency,
    TooManyConnections(usize),
    IndexNotReadOnly(usize),
    FieldNotReadOnly(&'static str),
    IndexPrepareFailed(usize, SqllError),
    FieldPrepareFailed(&'static str, SqllError),
    IndexNotThreadSafe(usize, NotThreadSafe),
    FieldNotThreadSafe(&'static str, NotThreadSafe),
    IndexBuildFailed(usize, Box<dyn Error + Send + Sync + 'static>),
    FieldBuildFailed(&'static str, Box<dyn Error + Send + Sync + 'static>),
}

impl From<ErrorKind> for PoolError {
    #[inline]
    fn from(kind: ErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "std")]
            Self::NotUtf8Path => write!(f, "path is not valid UTF-8"),
            #[cfg(feature = "std")]
            Self::NulByteInPath => write!(f, "path contains a nul byte"),
            Self::BuildWriteConnection(_) => write!(f, "building write connection"),
            Self::BuildReadConnection(index, _) => write!(f, "building read connection #{index}"),
            Self::SetupWriteConnection(_) => write!(f, "setting up write connection"),
            Self::SetupReadConnection(index, _) => write!(f, "setting up read connection #{index}"),
            Self::NoConnections => write!(f, "no connections available"),
            Self::ZeroConcurrency => write!(f, "read concurrency must be at least 1"),
            Self::TooManyConnections(concurrency) => {
                write!(
                    f,
                    "read concurrency {concurrency} exceeds the maximum of 64"
                )
            }
            Self::IndexNotReadOnly(index) => write!(f, "index {index} is not read-only"),
            Self::FieldNotReadOnly(name) => write!(f, "field {name} is not read-only"),
            Self::IndexPrepareFailed(index, _) => write!(f, "index {index} prepare failed"),
            Self::FieldPrepareFailed(name, _) => write!(f, "field {name} prepare failed"),
            Self::IndexNotThreadSafe(index, _) => write!(f, "index {index} is not thread-safe"),
            Self::FieldNotThreadSafe(name, _) => write!(f, "field {name} is not thread-safe"),
            Self::IndexBuildFailed(index, source) => {
                write!(f, "index {index} build failed: {source}")
            }
            Self::FieldBuildFailed(name, source) => {
                write!(f, "field {name} build failed: {source}")
            }
        }
    }
}

/// A pool of connections to the database.
///
/// Connections are represented by the custom types `R` and `W`, where `R` is a
/// read-only connection and `W` is a write connection.
///
/// The pool hands out two kinds of guards, each through its own lane:
///
/// * [`shared`] lends out one of the `read_concurrency` read-only connections.
///   Up to `read_concurrency` shared guards can be held at once.
/// * [`exclusive`] lends out the single write connection. Only one exclusive
///   guard can be held at a time, but it does not wait for or block shared
///   guards: a writer runs alongside the readers.
///
/// Readers proceeding during a write is what SQLite's [WAL] journal mode is
/// for, and it is the mode a pool should be used with (see
/// [`OpenOptions::journal_mode`]). Each reader then sees the database as of
/// the start of its read transaction until the writer commits. Under a rollback
/// journal, SQLite instead makes a writer that commits while readers hold
/// their locks (or readers that start while the writer commits) fail with
/// `SQLITE_BUSY`, or wait if a [`busy_timeout`] is set. Use [`exclusive_all`]
/// to keep every reader out for the duration of a write instead.
///
/// Note that each lane is fair, so if there are multiple tasks waiting for a
/// guard, they will be granted in the order they were requested meaning calls
/// to [`exclusive`] will complete in the order they were made.
///
/// [`shared`]: Self::shared
/// [`exclusive`]: Self::exclusive
/// [`exclusive_all`]: Self::exclusive_all
/// [WAL]: https://sqlite.org/wal.html
/// [`busy_timeout`]: OpenOptions::busy_timeout
///
/// # Examples
///
/// The read side `R` must be a `#[sql(read_only)]` [`Statements`] collection so
/// that its connections can be used concurrently, while the write side `W` holds
/// the statements that mutate the database:
///
/// ```no_run
/// use sqll::{SendStatement, Statements, PoolBuilder, OpenOptions};
///
/// #[derive(Statements)]
/// #[sql(read_only)]
/// struct Read {
///     #[sql = "SELECT name, age FROM users ORDER BY age"]
///     all_users: SendStatement,
/// }
///
/// #[derive(Statements)]
/// struct Write {
///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
///     insert_user: SendStatement,
/// }
///
/// let mut options = OpenOptions::new();
/// options.no_mutex().create();
/// let pool = PoolBuilder::new(options, 4).open::<Read, Write>("example.db")?;
/// # Ok::<_, sqll::PoolError>(())
/// ```
///
/// Once built, wrap the pool in an [`Arc`] and use [`shared`] to read or
/// [`exclusive`] to write. Since SQLite is a blocking library, the actual
/// statement calls should be performed on a blocking thread (such as
/// [`tokio::task::spawn_blocking`]). See [`examples/pool.rs`] for a complete
/// asynchronous example.
///
/// SQL which cannot be declared up front, such as a query composed at runtime,
/// can be prepared on the connection a guard lends out through
/// [`SharedGuard::connection`] or [`ExclusiveGuard::connection`]. These also
/// give access to [`Connection::changes`] and [`Connection::last_insert_rowid`]
/// after a write.
///
/// [`Arc`]: alloc::sync::Arc
/// [`examples/pool.rs`]: https://github.com/udoprog/sqll/blob/main/examples/pool.rs
/// [`tokio::task::spawn_blocking`]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
//
// # Safety
//
// The pool hands out `NonNull` pointers into `read_pool` and `write` through
// its guards, and turns them into `&mut` references in `Deref(Mut)`. That is
// sound as long as no two live guards ever point to the same slot. The two
// lanes guarantee this independently of each other:
//
// * Read slots. `read_semaphore` has `concurrency` permits, one per slot in
//   `read_pool`, and a `SharedGuard` holds exactly one. So at most
//   `concurrency` shared guards are live at once, and when one is acquiring its
//   slot at most `concurrency - 1` bits of `used_read` are set: there is always
//   a clear bit below `concurrency`. The guard claims it with a compare-exchange
//   (so two guards can never claim the same bit), and clears it in `Drop`
//   before its permit is released (fields drop after `Drop::drop`), so the bit
//   is free again by the time anyone else can obtain the permit to claim it.
//   Bit `i` set means slot `i` is owned by exactly one guard.
// * The write slot. `write_semaphore` has a single permit, and an
//   `ExclusiveGuard` holds it for as long as it lives, so at most one
//   exclusive guard, and therefore one pointer to `write`, is live at a time.
//
// The lanes never touch each other's slots: `read_pool` and `write` are
// disjoint allocations, a shared guard only ever points into `read_pool` and
// an exclusive guard only ever points to `write`. So a writer holding the write
// slot while readers hold read slots creates no aliasing, and the read lane's
// argument above makes no assumption about the write lane (or vice versa).
// `exclusive_all` additionally takes every read permit, but that is only a
// behavioural opt-in; it is not needed for soundness.
//
// This is also why `Pool: Sync` only needs `R: Send` and `W: Send`: through a
// `&Pool`, each `R` and the `W` is only ever reached by the one guard owning
// it, which may move it between threads but never shares it, while the readers
// and the writer running in parallel share nothing but the atomics and
// semaphores.
//
// On the SQLite side each slot owns its own `Connection` handle. A handle is
// only ever used by the one guard that owns its slot, so it is never used from
// two threads at once (which also keeps `no_mutex_unchecked` sound, as its docs
// require; the guards' `connection` docs cover a statement a caller prepares
// on it and keeps past the guard). Different handles used from different threads at the same time
// is what SQLite's multi-thread and serialized modes allow, and coordinating
// several connections to the same database file is SQLite's own job, through
// its file locks (or the WAL index). The worst a concurrent reader and writer
// can observe is `SQLITE_BUSY` under a rollback journal, an ordinary error. The
// read connections are opened read-only, and `R: IsReadOnly` promises their
// statements do not write, so the writer is the only connection that modifies
// the database.
//
// `as_read_mut` and `as_write_mut` bypass the lanes, which is fine since they
// take `&mut self`: no guard can be live, since each guard holds an `Arc` of
// the pool, and `Arc` only hands out `&Pool`.
pub struct Pool<R, W> {
    /// The pool of read connections of size `concurrency`.
    read_pool: Box<[UnsafeCell<Pooled<R>>]>,
    /// Bit set of which read connections are in use.
    used_read: AtomicU64,
    /// The single write connection.
    write: UnsafeCell<Pooled<W>>,
    /// The number of read connections, which is also the number of permits in
    /// `read_semaphore`.
    concurrency: usize,
    /// One permit per read connection, held by each [`SharedGuard`].
    read_semaphore: Arc<Semaphore>,
    /// A single permit for the write connection, held by each
    /// [`ExclusiveGuard`]. Independent of `read_semaphore`, so that a writer
    /// does not exclude readers.
    write_semaphore: Arc<Semaphore>,
}

/// A collection of statements together with the connection they were prepared
/// on.
///
/// The connection must outlive its statements. If it is closed first, SQLite
/// keeps it around as a zombie handle until the statements are finalized, and
/// any statement that needs to be re-prepared on it (such as one with a bare
/// `LIMIT ?` or `LIKE ?` whose plan depends on the bound value) fails with
/// `SQLITE_MISUSE`.
struct Pooled<T> {
    /// The prepared statements. Declared before `connection` so that they are
    /// dropped (finalized) first.
    statements: T,
    /// The connection the statements were prepared on. Kept alive for the
    /// statements, and lent out by [`SharedGuard::connection`] and
    /// [`ExclusiveGuard::connection`].
    #[cfg_attr(not(feature = "threadsafe"), allow(dead_code))]
    connection: Connection,
}

unsafe impl<R, W> Send for Pool<R, W>
where
    R: Send,
    W: Send,
{
}
unsafe impl<R, W> Sync for Pool<R, W>
where
    R: Send,
    W: Send,
{
}

impl<R, W> Pool<R, W>
where
    R: IsReadOnly,
    W: Statements,
{
    /// Create a new pool of connections to the database.
    ///
    /// The `open_options` parameter is used to configure the connections to the
    /// database. With the `threadsafe` feature the connections are always
    /// opened in the "serialized" threading mode, so [`no_mutex`] has no
    /// effect. Since the pool only ever hands each connection to one guard at a
    /// time, the per-connection mutex can be avoided with the unsafe
    /// [`no_mutex_unchecked`], as long as no statement prepared from a pooled
    /// connection, whether by the setup hooks or through a guard, is used or
    /// dropped on another thread than the one currently holding its guard. See
    /// the [thread safety] section of [`OpenOptions`].
    ///
    /// [`no_mutex`]: OpenOptions::no_mutex
    /// [`no_mutex_unchecked`]: OpenOptions::no_mutex_unchecked
    /// [thread safety]: OpenOptions#thread-safety
    ///
    /// This type implements `Send` and `Sync`, so it can be shared across
    /// threads.
    ///
    /// It constructs `read_concurrency` read connections to the database at the
    /// specified path. This database must be a filesystem database, since
    /// SQLite otherwise has no way of coordinating access, so specifying
    /// `:memory:` or a URI with `mode=memory` while it won't cause any error
    /// will result in every instance having their own private database.
    ///
    /// Note that the statements of `R` and `W` are prepared eagerly here, so any
    /// table they reference must exist by the time they are prepared. The
    /// `write_builder` runs first, so it can create or migrate the schema.
    ///
    /// The `write_builder` and `read_builder` are [`ConnectionSetup`] hooks run
    /// before the statements are prepared on the write and read connections
    /// respectively. They can be used to create the schema the statements depend
    /// on, or to apply per-connection `PRAGMA`s. Pass [`EmptySetup`] for either
    /// to do nothing. Prefer [`PoolBuilder`], which constructs these for you and
    /// defaults both to [`EmptySetup`].
    ///
    /// # Errors
    ///
    /// `read_concurrency` must be in the range `1..=64` (the upper bound is a
    /// consequence of tracking in-use read connections in a single 64-bit word),
    /// otherwise a [`PoolError`] is returned.
    ///
    /// # Examples
    ///
    /// See the [type-level documentation][Pool] for how to declare the `R` and
    /// `W` statement collections, and [`examples/pool.rs`] for a complete
    /// asynchronous example.
    ///
    /// [`examples/pool.rs`]: https://github.com/udoprog/sqll/blob/main/examples/pool.rs
    #[inline]
    #[cfg(feature = "std")]
    pub fn new(
        open_options: OpenOptions,
        path: impl AsRef<Path>,
        read_concurrency: usize,
        write_builder: impl ConnectionSetup,
        read_builder: impl ConnectionSetup,
    ) -> Result<Self, PoolError> {
        let path = path.as_ref();

        let Some(bytes) = path.to_str() else {
            return Err(PoolError::from(ErrorKind::NotUtf8Path));
        };

        let Ok(string) = CString::new(bytes) else {
            return Err(PoolError::from(ErrorKind::NulByteInPath));
        };

        Self::_new_c_str(
            open_options,
            &string,
            read_concurrency,
            write_builder,
            read_builder,
        )
    }

    /// Same as [`new`] but takes a C string for the path.
    ///
    /// [`new`]: Self::new
    pub fn new_c_str(
        open_options: OpenOptions,
        path: &CStr,
        read_concurrency: usize,
        write_builder: impl ConnectionSetup,
        read_builder: impl ConnectionSetup,
    ) -> Result<Self, PoolError> {
        Self::_new_c_str(
            open_options,
            path,
            read_concurrency,
            write_builder,
            read_builder,
        )
    }

    fn _new_c_str(
        open_options: OpenOptions,
        path: &CStr,
        concurrency: usize,
        write_builder: impl ConnectionSetup,
        read_builder: impl ConnectionSetup,
    ) -> Result<Self, PoolError> {
        if concurrency == 0 {
            return Err(PoolError::from(ErrorKind::ZeroConcurrency));
        }

        // The set of in-use read connections is tracked as a bit set in a single
        // `AtomicU64`, so we cannot represent more than 64 concurrent readers.
        if concurrency > 64 {
            return Err(PoolError::from(ErrorKind::TooManyConnections(concurrency)));
        }

        let mut write = open_options
            .clone()
            .read_write()
            .open_c_str(path)
            .map_err(ErrorKind::BuildWriteConnection)?;

        write_builder
            .setup(&mut write)
            .map_err(ErrorKind::SetupWriteConnection)?;

        let write = Pooled {
            statements: W::build(&mut write)?,
            connection: write,
        };

        let mut read_pool = Vec::with_capacity(concurrency);

        for index in 0..concurrency {
            let mut read = open_options
                .clone()
                .no_create()
                .read_only()
                .open_c_str(path)
                .map_err(move |error| ErrorKind::BuildReadConnection(index, error))?;

            read_builder
                .setup(&mut read)
                .map_err(move |error| ErrorKind::SetupReadConnection(index, error))?;

            let read = Pooled {
                statements: R::build(&mut read)?,
                connection: read,
            };

            read_pool.push(UnsafeCell::new(read));
        }

        Ok(Self {
            read_pool: Box::from(read_pool),
            used_read: AtomicU64::new(0),
            write: UnsafeCell::new(write),
            concurrency,
            read_semaphore: Arc::new(Semaphore::new(concurrency)),
            write_semaphore: Arc::new(Semaphore::new(1)),
        })
    }

    /// Access the first read connection without acquiring a lock.
    ///
    /// This is fine since it requires mutable access to the pool, so no other
    /// thread can have access to it at the same time. This is primarily useful
    /// for setup performed before the pool is shared across tasks.
    pub fn as_read_mut(&mut self) -> &mut R {
        unsafe { &mut (*self.read_pool[0].get()).statements }
    }

    /// Access the write connection without acquiring a lock.
    ///
    /// This is fine since it requires mutable access to the pool, so no other
    /// thread can have access to it at the same time.
    pub fn as_write_mut(&mut self) -> &mut W {
        unsafe { &mut (*self.write.get()).statements }
    }

    /// Acquire a shared lock on the pool.
    ///
    /// This provides access to one of the read side `R` connections in the
    /// pool. The number of concurrent shared locks is limited by the
    /// `read_concurrency` the pool was constructed with, and if all connections
    /// are in use, this method will wait until one is available.
    ///
    /// Shared guards are independent of [`exclusive`] guards, so this does not
    /// wait for a writer, unless one was taken with [`exclusive_all`].
    ///
    /// [`exclusive`]: Self::exclusive
    /// [`exclusive_all`]: Self::exclusive_all
    pub async fn shared(self: Arc<Self>) -> Result<SharedGuard<R, W>, PoolError> {
        let result = self.read_semaphore.clone().acquire_owned().await;

        let permit = match result {
            Ok(permit) => permit,
            Err(AcquireError { .. }) => return Err(PoolError::from(ErrorKind::NoConnections)),
        };

        // Find the first unused connection in the used set, mark it as used,
        // and return it. We can do this without locking because each permit
        // corresponds to one bit in the used set: holding a permit means fewer
        // than `concurrency` bits are set, so `trailing_ones` is always the
        // index of a clear bit within `read_pool`. See the safety comment on
        // `Pool`.
        let index = loop {
            let value = self.used_read.load(Ordering::Acquire);
            let index = value.trailing_ones();

            if self
                .used_read
                .compare_exchange(
                    value,
                    value | (1 << index),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                break index as usize;
            }
        };

        // SAFETY: We have exclusive access to the read connection at `index`
        // because we set its bit in `used_read` while holding a read permit, so
        // no other guard can claim it until our `Drop` clears the bit. No
        // exclusive guard ever touches `read_pool`. We are also keeping the
        // pool alive by storing it in the guard.
        let read = unsafe { NonNull::new_unchecked(self.read_pool[index].get()) };

        Ok(SharedGuard {
            read,
            pool: self,
            index,
            _permit: permit,
        })
    }

    /// Acquire the write connection of the pool.
    ///
    /// This provides access to the write side `W` of the pool. For the lifetime
    /// of the [`ExclusiveGuard`] no other exclusive guard can be acquired, so
    /// writes are serialized, but [`shared`] guards can still be acquired and
    /// used alongside it. The write connection is the only one which can modify
    /// the database.
    ///
    /// This relies on the database being in [WAL] mode for readers to proceed
    /// during a write. See the [type-level documentation][Pool] for what
    /// happens otherwise, and use [`exclusive_all`] to also keep every reader
    /// out.
    ///
    /// [`shared`]: Self::shared
    /// [`exclusive_all`]: Self::exclusive_all
    /// [WAL]: https://sqlite.org/wal.html
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use sqll::{JournalMode, OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT COUNT(*) FROM users"]
    ///     count: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name) VALUES (?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create().journal_mode(JournalMode::Wal);
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| c.execute("CREATE TABLE users (name TEXT)"))
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let mut write = pool.clone().exclusive().await?;
    /// let (w, c) = write.split();
    /// c.execute("BEGIN")?;
    /// w.insert.execute("Alice")?;
    ///
    /// // A reader is not held up by the writer, and does not see its
    /// // uncommitted changes.
    /// let mut read = pool.clone().shared().await?;
    /// read.count.reset()?;
    /// assert_eq!(read.count.next::<i64>()?, Some(0));
    /// read.count.reset()?;
    ///
    /// c.execute("COMMIT")?;
    /// assert_eq!(read.count.next::<i64>()?, Some(1));
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    pub async fn exclusive(self: Arc<Self>) -> Result<ExclusiveGuard<R, W>, PoolError> {
        let write_permit = match self.write_semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(AcquireError { .. }) => return Err(PoolError::from(ErrorKind::NoConnections)),
        };

        // SAFETY: We have exclusive access to the write connection because we
        // hold the single permit of `write_semaphore`, so no other exclusive
        // guard can be live. No shared guard ever touches `write`. We are also
        // keeping the pool alive by storing it in the guard.
        let write = unsafe { NonNull::new_unchecked(self.write.get()) };

        Ok(ExclusiveGuard {
            write,
            _pool: self,
            _read_permits: None,
            _write_permit: write_permit,
        })
    }

    /// Acquire the write connection of the pool while keeping every reader
    /// out.
    ///
    /// This is like [`exclusive`], but also waits for every [`shared`] guard to
    /// be dropped, and blocks new ones until the returned guard is dropped.
    /// It is for databases not in [WAL] mode, where a write conflicts with
    /// concurrent readers, or for anything else that needs the database to
    /// itself, such as `VACUUM`.
    ///
    /// Holding a [`SharedGuard`] of the same pool while waiting for this
    /// deadlocks, since it waits for that guard to be dropped.
    ///
    /// [`exclusive`]: Self::exclusive
    /// [`shared`]: Self::shared
    /// [WAL]: https://sqlite.org/wal.html
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// use sqll::{OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT COUNT(*) FROM users"]
    ///     count: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name) VALUES (?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| c.execute("CREATE TABLE users (name TEXT)"))
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let mut write = pool.clone().exclusive_all().await?;
    /// write.insert.execute("Alice")?;
    ///
    /// // No reader can get in while the guard is held.
    /// let wait = Duration::from_millis(50);
    /// assert!(tokio::time::timeout(wait, pool.clone().shared()).await.is_err());
    /// drop(write);
    ///
    /// let mut read = pool.clone().shared().await?;
    /// assert_eq!(read.count.next::<i64>()?, Some(1));
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    pub async fn exclusive_all(self: Arc<Self>) -> Result<ExclusiveGuard<R, W>, PoolError> {
        // The write permit is taken first, so that waiting for the readers to
        // drain does not hold up readers on behalf of a writer that is itself
        // still queued behind another writer. Taking the two in a fixed order
        // also means two `exclusive_all` calls cannot each hold half of what
        // the other needs.
        let write_permit = match self.write_semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(AcquireError { .. }) => return Err(PoolError::from(ErrorKind::NoConnections)),
        };

        let read_permits = match self
            .read_semaphore
            .clone()
            .acquire_many_owned(self.concurrency as u32)
            .await
        {
            Ok(permit) => permit,
            Err(AcquireError { .. }) => return Err(PoolError::from(ErrorKind::NoConnections)),
        };

        // SAFETY: As in `exclusive`, we hold the single write permit. Holding
        // every read permit as well is not needed for this access.
        let write = unsafe { NonNull::new_unchecked(self.write.get()) };

        Ok(ExclusiveGuard {
            write,
            _pool: self,
            _read_permits: Some(read_permits),
            _write_permit: write_permit,
        })
    }
}

/// An shared guard to the pool.
///
/// See [`Pool::shared`] for more details.
pub struct SharedGuard<R, W> {
    read: NonNull<Pooled<R>>,
    pool: Arc<Pool<R, W>>,
    index: usize,
    _permit: OwnedSemaphorePermit,
}

impl<R, W> SharedGuard<R, W> {
    /// Access the read-only connection that this guard's statements were
    /// prepared on.
    ///
    /// This is useful for SQL which cannot be declared up front as part of the
    /// `R` statement collection, such as a query whose filter or ordering is
    /// composed at runtime. The connection was opened read-only, so statements
    /// which attempt to modify the database fail.
    ///
    /// A [`Statement`] prepared on this connection should not outlive the
    /// guard. Once the guard is dropped the connection is handed to the next
    /// guard, which may run on another thread. Under the `threadsafe` feature
    /// this is still sound since the connection is serialized by SQLite, unless
    /// it was opened with [`no_mutex_unchecked`] whose safety requirements
    /// forbid it. It also holds the connection open after the pool is dropped.
    ///
    /// [`Statement`]: crate::Statement
    /// [`no_mutex_unchecked`]: OpenOptions::no_mutex_unchecked
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use sqll::{OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT COUNT(*) FROM users"]
    ///     count: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| {
    ///         c.execute(r#"
    ///             CREATE TABLE users (name TEXT, age INTEGER);
    ///             INSERT INTO users VALUES ('Alice', 42), ('Bob', 72), ('Carol', 21);
    ///         "#)
    ///     })
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let guard = pool.shared().await?;
    ///
    /// // An ordering only known at runtime.
    /// let order = "age DESC";
    ///
    /// let mut stmt = guard
    ///     .connection()
    ///     .prepare(format!("SELECT name FROM users ORDER BY {order}"))?;
    ///
    /// stmt.bind(())?;
    /// let names = stmt.iter::<String>().collect::<Result<Vec<_>, _>>()?;
    /// assert_eq!(names, ["Bob", "Alice", "Carol"]);
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    #[inline]
    #[cfg(feature = "threadsafe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "threadsafe")))]
    pub fn connection(&self) -> &Connection {
        // SAFETY: The guard has exclusive access to this read slot.
        unsafe { &self.read.as_ref().connection }
    }

    /// Access both the statements and the connection they were prepared on at
    /// the same time.
    ///
    /// This is the same as [`connection`] combined with the mutable access the
    /// guard otherwise gives to its statements through `DerefMut`, for when both
    /// need to be borrowed at once.
    ///
    /// [`connection`]: Self::connection
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use sqll::{OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name FROM users WHERE age > ? ORDER BY name"]
    ///     older_than: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| {
    ///         c.execute(r#"
    ///             CREATE TABLE users (name TEXT, age INTEGER);
    ///             INSERT INTO users VALUES ('Alice', 42), ('Bob', 72), ('Carol', 21);
    ///         "#)
    ///     })
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let mut guard = pool.shared().await?;
    /// let (read, c) = guard.split();
    ///
    /// // A statement prepared at runtime next to one prepared up front.
    /// let mut total = c.prepare("SELECT COUNT(*) FROM users")?;
    /// total.bind(())?;
    /// assert_eq!(total.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [3]);
    ///
    /// read.older_than.bind(30)?;
    /// let names = read.older_than.iter::<String>().collect::<Result<Vec<_>, _>>()?;
    /// assert_eq!(names, ["Alice", "Bob"]);
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    #[inline]
    #[cfg(feature = "threadsafe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "threadsafe")))]
    pub fn split(&mut self) -> (&mut R, &Connection) {
        // SAFETY: The guard has exclusive access to this read slot, and the
        // statements and the connection are disjoint fields.
        let pooled = unsafe { self.read.as_mut() };
        (&mut pooled.statements, &pooled.connection)
    }
}

impl<R, W> Deref for SharedGuard<R, W> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &self.read.as_ref().statements }
    }
}

impl<R, W> DerefMut for SharedGuard<R, W> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut self.read.as_mut().statements }
    }
}

unsafe impl<R, W> Send for SharedGuard<R, W>
where
    R: Send,
    W: Send,
{
}

impl<R, W> Drop for SharedGuard<R, W> {
    fn drop(&mut self) {
        // Mark the connection as unused again. We can do this without locking
        // because each permit corresponds to one bit in the used set.
        self.pool
            .used_read
            .fetch_and(!(1 << self.index), Ordering::AcqRel);
    }
}

/// An exclusive guard to the write connection of the pool.
///
/// See [`Pool::exclusive`] and [`Pool::exclusive_all`] for more details.
pub struct ExclusiveGuard<R, W> {
    write: NonNull<Pooled<W>>,
    _pool: Arc<Pool<R, W>>,
    /// Every read permit, when taken with [`Pool::exclusive_all`].
    _read_permits: Option<OwnedSemaphorePermit>,
    /// The single write permit, which is what makes access to `write` sound.
    _write_permit: OwnedSemaphorePermit,
}

impl<R, W> ExclusiveGuard<R, W> {
    /// Access the write connection that this guard's statements were prepared
    /// on.
    ///
    /// This is useful for SQL which cannot be declared up front as part of the
    /// `W` statement collection, and for [`Connection::changes`] and
    /// [`Connection::last_insert_rowid`] after running one of the write
    /// statements.
    ///
    /// A [`Statement`] prepared on this connection should not outlive the
    /// guard. Once the guard is dropped the connection is handed to the next
    /// guard, which may run on another thread. Under the `threadsafe` feature
    /// this is still sound since the connection is serialized by SQLite, unless
    /// it was opened with [`no_mutex_unchecked`] whose safety requirements
    /// forbid it. It also holds the connection open after the pool is dropped.
    ///
    /// [`Statement`]: crate::Statement
    /// [`no_mutex_unchecked`]: OpenOptions::no_mutex_unchecked
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use sqll::{OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name, age FROM users"]
    ///     all: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    ///     insert: SendStatement,
    ///     #[sql = "UPDATE users SET age = age + 1 WHERE name = ?"]
    ///     birthday: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| c.execute("CREATE TABLE users (name TEXT, age INTEGER)"))
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let mut guard = pool.exclusive().await?;
    ///
    /// guard.insert.execute(("Alice", 42))?;
    /// assert_eq!(guard.connection().last_insert_rowid(), 1);
    ///
    /// guard.birthday.execute("Bob")?;
    /// guard.birthday.reset()?;
    /// assert_eq!(guard.connection().changes(), 0);
    ///
    /// guard.birthday.execute("Alice")?;
    /// guard.birthday.reset()?;
    /// assert_eq!(guard.connection().changes(), 1);
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    #[inline]
    #[cfg(feature = "threadsafe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "threadsafe")))]
    pub fn connection(&self) -> &Connection {
        // SAFETY: The guard has exclusive access to the write slot.
        unsafe { &self.write.as_ref().connection }
    }

    /// Access both the statements and the connection they were prepared on at
    /// the same time.
    ///
    /// This is the same as [`connection`] combined with the mutable access the
    /// guard otherwise gives to its statements through `DerefMut`, for when both
    /// need to be borrowed at once, such as when running the write statements
    /// inside of a transaction.
    ///
    /// [`connection`]: Self::connection
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use sqll::{OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT COUNT(*) FROM users"]
    ///     count: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| c.execute("CREATE TABLE users (name TEXT, age INTEGER)"))
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let mut guard = pool.clone().exclusive().await?;
    /// let (write, c) = guard.split();
    ///
    /// c.execute("BEGIN")?;
    /// write.insert.execute(("Alice", 42))?;
    /// write.insert.reset()?;
    /// write.insert.execute(("Bob", 72))?;
    /// write.insert.reset()?;
    /// c.execute("ROLLBACK")?;
    /// drop(guard);
    ///
    /// let mut guard = pool.shared().await?;
    /// guard.count.bind(())?;
    /// assert_eq!(guard.count.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [0]);
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    #[inline]
    #[cfg(feature = "threadsafe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "threadsafe")))]
    pub fn split(&mut self) -> (&mut W, &Connection) {
        // SAFETY: The guard has exclusive access to the write slot, and the
        // statements and the connection are disjoint fields.
        let pooled = unsafe { self.write.as_mut() };
        (&mut pooled.statements, &pooled.connection)
    }

    /// Run `f` with the write statements and their connection inside of an
    /// [`Immediate`] transaction.
    ///
    /// This behaves like [`Connection::transaction`]: the transaction is
    /// committed if `f` returns `Ok`, and rolled back if it returns `Err` or
    /// panics. If the commit fails, it is rolled back and the commit error is
    /// returned. Use [`transaction_with`] to pick another
    /// [`TransactionBehavior`].
    ///
    /// [`Immediate`]: TransactionBehavior::Immediate
    /// [`transaction_with`]: Self::transaction_with
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use sqll::{Error, OpenOptions, PoolBuilder, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT COUNT(*) FROM users"]
    ///     count: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| c.execute("CREATE TABLE users (name TEXT, age INTEGER)"))
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let mut guard = pool.clone().exclusive().await?;
    ///
    /// guard.transaction(|write, _| {
    ///     write.insert.execute(("Alice", 42))?;
    ///     write.insert.execute(("Bob", 72))?;
    ///     Ok::<_, Error>(())
    /// })?;
    ///
    /// let result = guard.transaction(|write, c| {
    ///     write.insert.execute(("Charlie", 15))?;
    ///     c.execute("INSERT INTO missing VALUES (1)")
    /// });
    ///
    /// assert!(result.is_err());
    /// drop(guard);
    ///
    /// let mut guard = pool.shared().await?;
    /// guard.count.bind(())?;
    /// assert_eq!(guard.count.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [2]);
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    #[inline]
    #[cfg(feature = "threadsafe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "threadsafe")))]
    pub fn transaction<T, E>(
        &mut self,
        f: impl FnOnce(&mut W, &Connection) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SqllError>,
    {
        self.transaction_with(crate::TransactionBehavior::Immediate, f)
    }

    /// Run `f` with the write statements and their connection inside of a
    /// transaction with the given [`TransactionBehavior`].
    ///
    /// This is the same as [`transaction`] apart from the behavior.
    ///
    /// [`transaction`]: Self::transaction
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use sqll::{Error, OpenOptions, PoolBuilder, SendStatement, Statements, TransactionBehavior};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT COUNT(*) FROM users"]
    ///     count: SendStatement,
    /// }
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name) VALUES (?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// # let dir = tempfile::tempdir()?;
    /// # let path = dir.path().join("example.db");
    /// let mut options = OpenOptions::new();
    /// options.create();
    ///
    /// let pool = PoolBuilder::new(options, 4)
    ///     .with_write_setup(|c| c.execute("CREATE TABLE users (name TEXT)"))
    ///     .open::<Read, Write>(&path)?;
    ///
    /// let pool = Arc::new(pool);
    ///
    /// # tokio::runtime::Runtime::new()?.block_on(async move {
    /// let mut guard = pool.clone().exclusive().await?;
    ///
    /// guard.transaction_with(TransactionBehavior::Exclusive, |write, _| {
    ///     write.insert.execute("Alice")?;
    ///     Ok::<_, Error>(())
    /// })?;
    ///
    /// drop(guard);
    ///
    /// let mut guard = pool.shared().await?;
    /// guard.count.bind(())?;
    /// assert_eq!(guard.count.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [1]);
    /// # Ok::<_, anyhow::Error>(())
    /// # })?;
    /// # Ok::<_, anyhow::Error>(())
    /// ```
    #[cfg(feature = "threadsafe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "threadsafe")))]
    pub fn transaction_with<T, E>(
        &mut self,
        behavior: crate::TransactionBehavior,
        f: impl FnOnce(&mut W, &Connection) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SqllError>,
    {
        let (write, c) = self.split();
        c.transaction_with(behavior, |c| f(write, c))
    }
}

impl<R, W> Deref for ExclusiveGuard<R, W> {
    type Target = W;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &self.write.as_ref().statements }
    }
}

impl<R, W> DerefMut for ExclusiveGuard<R, W> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut self.write.as_mut().statements }
    }
}

unsafe impl<R, W> Send for ExclusiveGuard<R, W>
where
    R: Send,
    W: Send,
{
}
