use core::ffi::CStr;
use core::fmt;
use core::ops::{Deref, DerefMut};

#[cfg(feature = "std")]
use std::path::Path;

use crate::pool::ErrorKind;
use crate::{Connection, OpenOptions, PoolError, Statements};

/// A [`Statements`] collection together with the connection it was prepared
/// on, for synchronous use without a [`Pool`].
///
/// This dereferences to the statements `T`, and gives access to the connection
/// through [`connection`] for SQL which cannot be declared up front, or for
/// [`Connection::changes`] and [`Connection::last_insert_rowid`].
///
/// Statements keep the connection they were prepared on open, so unlike a
/// connection and statements stored side by side, the order in which the two
/// are dropped does not matter.
///
/// [`Pool`]: crate::Pool
/// [`connection`]: Self::connection
///
/// # Examples
///
/// ```
/// use sqll::{OpenOptions, Prepared, SendStatement, Statements};
///
/// #[derive(Statements)]
/// struct Users {
///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
///     insert: SendStatement,
///     #[sql = "SELECT name FROM users WHERE age > ? ORDER BY name"]
///     older_than: SendStatement,
/// }
///
/// let c = OpenOptions::new().create().read_write().open_in_memory()?;
/// c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
///
/// let mut users = Prepared::<Users>::new(c)?;
///
/// users.insert.execute(("Alice", 42))?;
/// users.insert.execute(("Bob", 21))?;
/// assert_eq!(users.connection().last_insert_rowid(), 2);
///
/// users.older_than.bind(30)?;
/// let names = users.older_than.iter::<String>().collect::<Result<Vec<_>, _>>()?;
/// assert_eq!(names, ["Alice"]);
/// # Ok::<_, Box<dyn std::error::Error>>(())
/// ```
pub struct Prepared<T> {
    // NB: Field order does not matter for soundness or correctness, since every
    // statement holds its own reference to the connection handle and keeps it
    // open until it is finalized.
    pub(crate) statements: T,
    pub(crate) connection: Connection,
}

impl<T> Prepared<T>
where
    T: Statements,
{
    /// Prepare the statements `T` on the given connection.
    ///
    /// Any setup the statements depend on, such as creating tables or running
    /// [`Migrations`], has to be done on the connection before calling this.
    ///
    /// [`Migrations`]: crate::Migrations
    ///
    /// # Errors
    ///
    /// Fails if any statement fails to prepare, or cannot be converted into a
    /// [`SendStatement`] because the connection is not thread safe. With the
    /// `threadsafe` feature every connection is thread safe; without it, open
    /// it with [`OpenOptions::full_mutex`].
    ///
    /// [`SendStatement`]: crate::SendStatement
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Prepared, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// struct Counter {
    ///     #[sql = "UPDATE counter SET n = n + 1"]
    ///     increment: SendStatement,
    ///     #[sql = "SELECT n FROM counter"]
    ///     get: SendStatement,
    /// }
    ///
    /// let c = OpenOptions::new().create().read_write().full_mutex().open_in_memory()?;
    /// c.execute("CREATE TABLE counter (n INTEGER); INSERT INTO counter VALUES (0)")?;
    ///
    /// let mut counter = Prepared::<Counter>::new(c)?;
    /// counter.increment.execute(())?;
    /// assert_eq!(counter.get.first::<i64>(())?, Some(1));
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(mut connection: Connection) -> Result<Self, PoolError> {
        Ok(Self {
            statements: T::build(&mut connection)?,
            connection,
        })
    }

    /// Open a connection to the database at `path` with the given options and
    /// prepare the statements `T` on it.
    ///
    /// This is [`OpenOptions::open`] followed by [`Prepared::new`]. To set up
    /// the database before the statements are prepared, open the connection
    /// and call [`Prepared::new`] instead.
    ///
    /// # Errors
    ///
    /// Fails if the connection cannot be opened, or for the same reasons as
    /// [`Prepared::new`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Prepared, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// struct Tables {
    ///     #[sql = "SELECT name FROM sqlite_schema WHERE type = 'table'"]
    ///     names: SendStatement,
    /// }
    ///
    /// let dir = tempfile::tempdir()?;
    /// let path = dir.path().join("example.db");
    ///
    /// let mut options = OpenOptions::new();
    /// options.create().read_write();
    ///
    /// let mut tables = Prepared::<Tables>::open(&options, &path)?;
    /// assert_eq!(tables.names.iter::<String>().count(), 0);
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub fn open(options: &OpenOptions, path: impl AsRef<Path>) -> Result<Self, PoolError> {
        let connection = options.open(path).map_err(ErrorKind::Open)?;
        Self::new(connection)
    }

    /// Same as [`open`] but takes a C string for the path.
    ///
    /// [`open`]: Self::open
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Prepared, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// struct One {
    ///     #[sql = "SELECT 1"]
    ///     one: SendStatement,
    /// }
    ///
    /// let mut options = OpenOptions::new();
    /// options.create().read_write();
    ///
    /// let mut one = Prepared::<One>::open_c_str(&options, c":memory:")?;
    /// assert_eq!(one.one.first::<i64>(())?, Some(1));
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_c_str(options: &OpenOptions, path: &CStr) -> Result<Self, PoolError> {
        let connection = options.open_c_str(path).map_err(ErrorKind::Open)?;
        Self::new(connection)
    }
}

impl<T> Prepared<T> {
    /// Access the connection the statements were prepared on.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Prepared, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// struct Insert {
    ///     #[sql = "INSERT INTO t (a) VALUES (?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// let c = OpenOptions::new().create().read_write().open_in_memory()?;
    /// c.execute("CREATE TABLE t (a INTEGER)")?;
    ///
    /// let mut p = Prepared::<Insert>::new(c)?;
    /// p.insert.execute(1)?;
    /// assert_eq!(p.connection().changes(), 1);
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Borrow the statements mutably and the connection at the same time.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Prepared, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// struct Insert {
    ///     #[sql = "INSERT INTO t (a) VALUES (?)"]
    ///     insert: SendStatement,
    /// }
    ///
    /// let c = OpenOptions::new().create().read_write().open_in_memory()?;
    /// c.execute("CREATE TABLE t (a INTEGER)")?;
    ///
    /// let mut p = Prepared::<Insert>::new(c)?;
    /// let (stmts, c) = p.split();
    ///
    /// stmts.insert.execute(7)?;
    /// let mut select = c.prepare("SELECT a FROM t")?;
    /// assert_eq!(select.first::<i64>(())?, Some(7));
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn split(&mut self) -> (&mut T, &Connection) {
        (&mut self.statements, &self.connection)
    }

    /// Take the statements and the connection apart.
    ///
    /// Either can be dropped first, since the statements keep the connection
    /// open.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Prepared, SendStatement, Statements};
    ///
    /// #[derive(Statements)]
    /// struct One {
    ///     #[sql = "SELECT 1"]
    ///     one: SendStatement,
    /// }
    ///
    /// let c = OpenOptions::new().create().read_write().open_in_memory()?;
    /// let (mut stmts, c) = Prepared::<One>::new(c)?.into_parts();
    /// drop(c);
    ///
    /// assert_eq!(stmts.one.first::<i64>(())?, Some(1));
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn into_parts(self) -> (T, Connection) {
        (self.statements, self.connection)
    }
}

impl<T> Deref for Prepared<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.statements
    }
}

impl<T> DerefMut for Prepared<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.statements
    }
}

impl<T> fmt::Debug for Prepared<T>
where
    T: fmt::Debug,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prepared")
            .field("statements", &self.statements)
            .field("connection", &self.connection)
            .finish()
    }
}
