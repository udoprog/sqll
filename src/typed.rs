//! Typed wrapper around [`SendStatement`] that encodes the expected bind
//! parameter and column counts in the type system. This allows for ahead of
//! time checking of bind and column counts, and provides a more ergonomic API
//! for executing statements and retrieving rows.

use core::fmt;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;

use crate::{Bind, Error, Row, SendStatement};

/// A typed wrapper around [`SendStatement`] that encodes the expected bind
/// parameter and column counts in the type system. This allows for ahead of
/// time checking of bind and column counts, and provides a more ergonomic API
/// for executing statements and retrieving rows.
///
/// The typical use case for this is in combination with the [`Statements`]
/// derive macro, which generates a struct of typed statements.
///
/// This encodes invariants of the low level API which makes it more
/// abuse-resistant. For example, it ensures that a statement is appropriately
/// reset between uses which ensures that write transactions are actually
/// persisted.
///
/// [`Statements`]: derive@crate::Statements
///
/// # Examples
///
/// ```
/// use sqll::{Statements, TypedStatement};
///
/// #[derive(Statements)]
/// #[sql(read_only)]
/// struct Read {
///     #[sql = "SELECT name, age FROM users ORDER BY age"]
///     all_users: TypedStatement<(), (String, i32)>,
/// }
///
/// #[derive(Statements)]
/// struct Write {
///     // Reuse every statement in `Read`, accessible as `write.read.all_users`.
///     #[sql(statements)]
///     read: Read,
///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
///     insert_user: TypedStatement<(String, i32), ()>,
/// }
/// ```
pub struct TypedStatement<I, O> {
    inner: SendStatement,
    _marker: PhantomData<(I, O)>,
}

impl<O> TypedStatement<(), O> {
    /// Return a bound statement that is used only to retrieve rows without any
    /// bind parameters.
    ///
    /// This is the same as calling [`TypedStatement::bind`] with an empty
    /// tuple.
    ///
    /// See [`TypedStatement::bind`] for more details and examples.
    pub fn query(&mut self) -> Result<BoundStatement<'_, (), O>, Error> {
        self.bind(())
    }
}

impl<I> TypedStatement<I, ()>
where
    I: Bind,
{
    /// Execute the statement with the specified bind parameters, this is not
    /// expected to return any rows.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Statements, TypedStatement};
    ///
    /// #[derive(Statements)]
    /// struct Write {
    ///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
    ///     insert_user: TypedStatement<(String, i32), ()>,
    /// }
    ///
    /// let mut c = OpenOptions::new()
    ///     .no_mutex()
    ///     .read_write()
    ///     .create()
    ///     .open_in_memory()?;
    ///
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
    ///
    /// let mut write = Write::build(&mut c)?;
    /// write.insert_user.execute(("Alice".to_string(), 25))?;
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    #[track_caller]
    pub fn execute<B>(&mut self, bind: B) -> Result<(), Error>
    where
        B: Bind,
    {
        const {
            assert!(
                I::COUNT == B::COUNT,
                "unexpected bind parameter count for statement",
            );
        }

        self.inner.reset()?;
        self.inner.bind(bind)?;
        while !self.inner.step()?.is_done() {}
        Ok(())
    }
}

impl<I, O> TypedStatement<I, O>
where
    I: Bind,
{
    /// Bind the given parameters to the statement, returning a [`BoundStatement`]
    /// that can be used to execute the statement or retrieve rows. The bind
    /// parameter count is checked at compile time, so if the provided bind
    /// parameters do not match the expected count, this will result in a
    /// compile time error.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Statements, TypedStatement};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name, age FROM users WHERE age > ? ORDER BY age"]
    ///     users_older_than: TypedStatement<(i32,), (String, i32)>,
    /// }
    ///
    /// let mut c = OpenOptions::new()
    ///     .no_mutex()
    ///     .read_write()
    ///     .create()
    ///     .open_in_memory()?;
    ///
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
    /// c.execute("INSERT INTO users (name, age) VALUES ('Alice', 25), ('Bob', 35), ('Charlie', 30)")?;
    ///
    /// let mut read = Read::build(&mut c)?;
    /// let mut stmt = read.users_older_than.bind((30,))?;
    ///
    /// let mut out = Vec::new();
    ///
    /// while let Some((name, age)) = stmt.next()? {
    ///     out.push((name, age));
    /// }
    ///
    /// assert_eq!(out, vec![("Bob".to_string(), 35)]);
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    #[track_caller]
    pub fn bind<B>(&mut self, bind: B) -> Result<BoundStatement<'_, I, O>, Error>
    where
        B: Bind,
    {
        const {
            assert!(
                I::COUNT == B::COUNT,
                "unexpected bind parameter count for statement",
            );
        }

        self.inner.reset()?;
        self.inner.bind(bind)?;
        Ok(BoundStatement {
            stmt: &mut self.inner,
            _marker: PhantomData,
        })
    }
}

#[derive(Debug)]
enum TryFromSendStatementErrorKind {
    BindParameterCount { expected: i32, actual: i32 },
    ColumnCount { expected: i32, actual: i32 },
}

/// An error that can occur when trying to convert a [`SendStatement`] into a
/// [`TypedStatement`], if the bind parameter count or column count does not
/// match the expected counts encoded in the type parameters of the
/// [`TypedStatement`].
pub struct TryFromSendStatementError {
    kind: TryFromSendStatementErrorKind,
}

impl fmt::Debug for TryFromSendStatementError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Display for TryFromSendStatementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TryFromSendStatementErrorKind::BindParameterCount { expected, actual } => write!(
                f,
                "unexpected bind parameter count for statement: expected {expected}, got {actual}",
            ),
            TryFromSendStatementErrorKind::ColumnCount { expected, actual } => write!(
                f,
                "unexpected column count for statement: expected {expected}, got {actual}",
            ),
        }
    }
}

impl core::error::Error for TryFromSendStatementError {}

impl<I, O> TryFrom<SendStatement> for TypedStatement<I, O>
where
    I: Bind,
    O: for<'stmt> Row<'stmt>,
{
    type Error = TryFromSendStatementError;

    fn try_from(inner: SendStatement) -> Result<Self, TryFromSendStatementError> {
        if inner.bind_parameter_count() != I::COUNT as i32 {
            return Err(TryFromSendStatementError {
                kind: TryFromSendStatementErrorKind::BindParameterCount {
                    expected: I::COUNT as i32,
                    actual: inner.bind_parameter_count(),
                },
            });
        }

        if inner.column_count() != O::COUNT as i32 {
            return Err(TryFromSendStatementError {
                kind: TryFromSendStatementErrorKind::ColumnCount {
                    expected: O::COUNT as i32,
                    actual: inner.column_count(),
                },
            });
        }

        Ok(Self {
            inner,
            _marker: PhantomData,
        })
    }
}

/// A [`TypedStatement`] that has been bound with parameters, ready to execute
/// or retrieve rows.
///
/// When a `BoundStatement` is dropped it resets the underlying statement (any
/// reset error is ignored), so the statement can be bound again even if it was
/// only partially read. Use [`reset`] instead when you need to observe a reset
/// error.
///
/// See [`TypedStatement::bind`] for more details and examples.
///
/// # Examples
///
/// Dropping a bound statement mid-iteration resets it, so the next bind replays
/// from the top:
///
/// ```
/// use sqll::{OpenOptions, Statements, TypedStatement};
///
/// #[derive(Statements)]
/// #[sql(read_only)]
/// struct Read {
///     #[sql = "SELECT name, age FROM users ORDER BY age"]
///     all: TypedStatement<(), (String, i32)>,
/// }
///
/// let mut c = OpenOptions::new()
///     .no_mutex()
///     .read_write()
///     .create()
///     .open_in_memory()?;
///
/// c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
/// c.execute("INSERT INTO users (name, age) VALUES ('Alice', 25), ('Bob', 35)")?;
///
/// let mut read = Read::build(&mut c)?;
///
/// // Read a single row, then drop the bound statement mid-iteration.
/// let mut stmt = read.all.query()?;
/// assert_eq!(stmt.next()?, Some(("Alice".to_string(), 25)));
/// drop(stmt);
///
/// assert_eq!(read.all.query()?.first()?, Some(("Alice".to_string(), 25)));
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
///
/// [`reset`]: BoundStatement::reset
pub struct BoundStatement<'stmt, I, O> {
    stmt: &'stmt mut SendStatement,
    _marker: PhantomData<(I, O)>,
}

impl<I, O> BoundStatement<'_, I, O> {
    /// Consume the bound statement and return the first row, if any, resetting
    /// the underlying statement. This is a convenient way to execute a query
    /// that is expected to return at most one row, without needing to manually
    /// reset the statement or handle multiple rows.
    ///
    /// # Examples
    ///
    /// `first` returns only the first matching row, discards the rest, and
    /// resets the statement so it can be bound again immediately:
    ///
    /// ```
    /// use sqll::{OpenOptions, Statements, TypedStatement};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name, age FROM users WHERE age > ? ORDER BY age"]
    ///     older_than: TypedStatement<(i32,), (String, i32)>,
    /// }
    ///
    /// let mut c = OpenOptions::new().no_mutex().read_write().create().open_in_memory()?;
    ///
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
    /// c.execute("INSERT INTO users (name, age) VALUES ('Alice', 25), ('Bob', 35)")?;
    ///
    /// let mut read = Read::build(&mut c)?;
    ///
    /// // Two rows match, but only the first is returned.
    /// let first = read.older_than.bind((10,))?.first()?;
    /// assert_eq!(first, Some(("Alice".to_string(), 25)));
    ///
    /// // The statement was reset, so a fresh bind replays from the top.
    /// let again = read.older_than.bind((10,))?.first()?;
    /// assert_eq!(again, Some(("Alice".to_string(), 25)));
    ///
    /// // When nothing matches, `first` yields `None` rather than erroring.
    /// let none = read.older_than.bind((100,))?.first()?;
    /// assert_eq!(none, None);
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    pub fn first(self) -> Result<Option<O>, Error>
    where
        O: for<'stmt> Row<'stmt>,
    {
        let value = self.stmt.next()?;
        let mut this = ManuallyDrop::new(self);
        this.stmt.reset()?;
        Ok(value)
    }

    /// Return the next row, if any, without resetting the underlying statement.
    ///
    /// This is a convenient way to execute a query that is expected to return
    /// multiple rows, allowing the caller to iterate over them without needing
    /// to manually reset the statement between rows. The caller is responsible
    /// for resetting the statement when finished, either by calling `reset` or
    /// by letting the bound statement go out of scope.
    ///
    /// # Examples
    ///
    /// `next` yields each row in turn, returning `None` once the result set is
    /// exhausted (which terminates the loop below):
    ///
    /// ```
    /// use sqll::{OpenOptions, Statements, TypedStatement};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name, age FROM users ORDER BY age"]
    ///     all: TypedStatement<(), (String, i32)>,
    /// }
    ///
    /// let mut c = OpenOptions::new().no_mutex().read_write().create().open_in_memory()?;
    ///
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
    /// c.execute("INSERT INTO users (name, age) VALUES ('Alice', 25), ('Bob', 35)")?;
    ///
    /// let mut read = Read::build(&mut c)?;
    /// let mut stmt = read.all.query()?;
    ///
    /// let mut out = Vec::new();
    /// while let Some(row) = stmt.next()? {
    ///     out.push(row);
    /// }
    /// assert_eq!(out, [("Alice".to_string(), 25), ("Bob".to_string(), 35)]);
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    #[inline]
    pub fn next(&mut self) -> Result<Option<O>, Error>
    where
        O: for<'stmt> Row<'stmt>,
    {
        self.stmt.next()
    }

    /// Consume the bound statement and reset the underlying statement. This
    /// lets a caller release the statement between sequential uses without a
    /// scope.
    ///
    /// # Errors
    ///
    /// This forwards the result of [`Statement::reset`], which currently always
    /// succeeds. See its documentation for why.
    ///
    /// [`Statement::reset`]: crate::Statement::reset
    ///
    /// # Examples
    ///
    /// `reset` releases the statement after a partial read, leaving it ready to
    /// be bound again from the top:
    ///
    /// ```
    /// use sqll::{OpenOptions, Statements, TypedStatement};
    ///
    /// #[derive(Statements)]
    /// #[sql(read_only)]
    /// struct Read {
    ///     #[sql = "SELECT name, age FROM users ORDER BY age"]
    ///     all: TypedStatement<(), (String, i32)>,
    /// }
    ///
    /// let mut c = OpenOptions::new().no_mutex().read_write().create().open_in_memory()?;
    ///
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;
    /// c.execute("INSERT INTO users (name, age) VALUES ('Alice', 25), ('Bob', 35)")?;
    ///
    /// let mut read = Read::build(&mut c)?;
    ///
    /// let mut stmt = read.all.query()?;
    /// assert_eq!(stmt.next()?, Some(("Alice".to_string(), 25)));
    /// stmt.reset()?; // stop early so the statement can be reused
    ///
    /// // The statement replays from the top on the next bind.
    /// assert_eq!(read.all.query()?.first()?, Some(("Alice".to_string(), 25)));
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    pub fn reset(self) -> Result<(), Error> {
        // Skip the `Drop` reset since we reset here. `Statement::reset`
        // currently never fails, but its `Result` is forwarded in case it does.
        let mut this = ManuallyDrop::new(self);
        this.stmt.reset()?;
        Ok(())
    }
}

impl<I, O> Drop for BoundStatement<'_, I, O> {
    fn drop(&mut self) {
        // Ensure the statement is reset when the bound statement goes out of
        // scope, so that the underlying statement can be reused.
        let _ = self.stmt.reset();
    }
}
