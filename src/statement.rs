use core::ffi::{CStr, c_int};
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut, Range};
use core::ptr::NonNull;

use crate::ffi;
use crate::ty::Type;
use crate::utils::{c_to_error_text, c_to_text};
use crate::{
    Bind, BindValue, Code, Error, FromColumn, FromUnsizedColumn, Result, Row, Text, ValueType,
};

/// A marker type representing NULL.
///
/// This is both a value and type marker through [`Type`] representing NULL.
///
/// This can be used in [`BindValue`] and [`FromColumn`] when a NULL value is
/// expected.
///
/// To optionally support NULL values, consider using `Option<T>` instead.
///
/// See [`Statement::bind_value`] and [`Statement::column`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Null};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value);
///
///     INSERT INTO test (value) VALUES (NULL);
/// "#)?;
///
/// let mut select = c.prepare("SELECT value FROM test")?;
/// assert_eq!(select.iter::<Null>().collect::<Vec<_>>(), [Ok(Null)]);
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Using as a type in a [`FromColumn`] implementation:
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Null};
///
/// struct MyNull(Null);
///
/// impl FromColumn<'_> for MyNull {
///     type Type = Null;
///
///     #[inline]
///     fn from_column(stmt: &Statement, _: Self::Type) -> Result<Self> {
///         Ok(MyNull(Null))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value INTEGER);
///
///     INSERT INTO test (value) VALUES (NULL);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert!(matches!(stmt.next::<MyNull>()?, Some(MyNull(..))));
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Null;

/// The state after stepping a statement.
///
/// See [`Statement::step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum State {
    /// There is a row available for reading.
    Row,
    /// The statement has been entirely evaluated.
    Done,
}

impl State {
    /// Return `true` if the state is [`State::Done`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (id INTEGER);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("INSERT INTO test (id) VALUES (1)")?;
    /// assert!(stmt.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn is_done(&self) -> bool {
        matches!(self, State::Done)
    }

    /// Return `true` if the state is a [`State::Row`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (id INTEGER);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("INSERT INTO test (id) VALUES (1)")?;
    /// assert!(stmt.step()?.is_done());
    ///
    /// let mut stmt = c.prepare("SELECT id FROM test")?;
    /// assert!(stmt.step()?.is_row());
    /// assert!(stmt.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn is_row(&self) -> bool {
        matches!(self, State::Row)
    }
}

/// A prepared statement.
///
/// Prepared statements are compiled from a [`Connection`] using [`prepare`] or
/// [`prepare_with`]. The [`Connection`] which constructed the prepared
/// statement will remain alive for as long as the statement is alive, even if
/// the connection is closed.
///
/// Statements can be re-used, but between each re-use [`reset`] has to be
/// called. A defensive coding style suggests its appropriate to always call
/// this before using a statement. A call to [`reset`] must also be done to
/// refresh the prepared statement with respects to changes in the database.
///
/// A handful of higher-level convenience methods calls [`reset`] internally,
/// such as [`bind`] and [`execute`] since it wouldn't make sense to use them
/// without resetting first. Binding in the middle of stepping through the
/// results has no effect.
///
/// Low level APIs are the following:
/// * [`reset`] - Resets the statement to be re-executed.
/// * [`step`] - Steps the statement over the query.
/// * [`bind_value`] - Binds a single value to a specific index in a statement.
/// * [`column`] - Reads a single column from the current row.
/// * [`unsized_column`] - Reads a single unsized column from the current row.
///
/// Higher level APIs are the following and are generally preferred to use since
/// they are less prone to errors:
/// * [`bind`] - Binds values to the statement using the [`Bind`] trait allowing
///   for multiple values to be bound.
/// * [`execute`] - Executes the statement to completion. Can take a binding
///   using the [`Bind`] trait.
/// * [`next`] - Reads the entire next row from the statement using the [`Row`]
///   trait.
/// * [`iter`] - Coerces the statement into an iterator over rows using the
///   [`Row`] trait.
/// * [`into_iter`] - Coerces the statement into an owned iterator over rows
///   using the [`Row`] trait.
///
/// For durable prepared statements it is recommended that
/// [`prepare_with`] is used with [`Prepare::PERSISTENT`] set.
///
/// [`bind_value`]: Self::bind_value
/// [`bind`]: Self::bind
/// [`column`]: Self::column
/// [`prepare_with`]: crate::Connection::prepare_with
/// [`prepare`]: crate::Connection::prepare
/// [`Connection`]: crate::Connection
/// [`execute`]: Self::execute
/// [`into_iter`]: Self::into_iter
/// [`iter`]: Self::iter
/// [`next`]: Self::next
/// [`Prepare::PERSISTENT`]: crate::Prepare::PERSISTENT
/// [`reset`]: Self::reset
/// [`row`]: Self::row
/// [`step`]: Self::step
/// [`unsized_column`]: Self::unsized_column
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Prepare};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (id INTEGER);
/// "#)?;
///
/// let mut insert_stmt = c.prepare_with("INSERT INTO test (id) VALUES (?);", Prepare::PERSISTENT)?;
/// let mut query = c.prepare_with("SELECT id FROM test;", Prepare::PERSISTENT)?;
///
/// drop(c);
///
/// /* .. */
///
/// insert_stmt.execute(42)?;
///
/// query.bind(())?;
/// assert_eq!(query.iter::<i64>().collect::<Vec<_>>(), [Ok(42)]);
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Statement {
    raw: NonNull<ffi::sqlite3_stmt>,
    is_thread_safe: bool,
}

impl fmt::Debug for Statement {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Statement").finish_non_exhaustive()
    }
}

impl Statement {
    /// Construct a statement from a raw pointer.
    #[inline]
    pub(crate) fn from_raw(raw: NonNull<ffi::sqlite3_stmt>, is_thread_safe: bool) -> Statement {
        Statement {
            raw,
            is_thread_safe,
        }
    }

    /// Return the raw pointer.
    #[inline]
    pub(super) fn as_ptr(&self) -> *mut ffi::sqlite3_stmt {
        self.raw.as_ptr()
    }

    /// Return the raw mutable pointer.
    #[inline]
    pub(super) fn as_ptr_mut(&mut self) -> *mut ffi::sqlite3_stmt {
        self.raw.as_ptr()
    }

    #[inline]
    pub(crate) fn error_message(&self) -> &Text {
        unsafe {
            let db = ffi::sqlite3_db_handle(self.as_ptr());
            let msg_ptr = ffi::sqlite3_errmsg(db);
            c_to_error_text(msg_ptr)
        }
    }

    /// Coerce this statement into a [`SendStatement`] which can be sent across
    /// threads.
    ///
    /// # Panics
    ///
    /// This will panic if neither [`full_mutex`] or [`no_mutex`] are set, or if
    /// the `threadsafe` feature is not set.
    ///
    /// [`full_mutex`]: crate::OpenOptions::full_mutex
    /// [`no_mutex`]: crate::OpenOptions::no_mutex
    ///
    /// # Safety
    ///
    /// This is unsafe because it required that the caller ensures that any
    /// database objects are synchronized. The exact level of synchronization
    /// depends on how the connection was opened:
    /// * If [`full_mutex`] was set and [`no_mutex`] was not set, no external
    ///   synchronization is necessary, but calls to the statement might block
    ///   if it's busy.
    /// * If [`no_mutex`] was set, the caller must ensure that the [`Statement`]
    ///   is fully synchronized with respect to the connection that constructed
    ///   it. One way to achieve this is to wrap all the statements behind a
    ///   single mutex.
    ///
    /// [`full_mutex`]: crate::OpenOptions::full_mutex
    /// [`no_mutex`]: crate::OpenOptions::no_mutex
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use sqll::{OpenOptions, Prepare, SendStatement};
    /// use anyhow::Result;
    /// use tokio::task;
    /// use tokio::sync::Mutex;
    ///
    /// struct Statements {
    ///     select: SendStatement,
    ///     update: SendStatement,
    /// }
    ///
    /// #[derive(Clone)]
    /// struct Database {
    ///     stmts: Arc<Mutex<Statements>>,
    /// }
    ///
    /// fn setup_database() -> Result<Database> {
    ///     let c = OpenOptions::new()
    ///         .create()
    ///         .read_write()
    ///         .no_mutex()
    ///         .open_in_memory()?;
    ///
    ///     c.execute(
    ///         r#"
    ///         CREATE TABLE users (name TEXT PRIMARY KEY NOT NULL, age INTEGER);
    ///
    ///         INSERT INTO users VALUES ('Alice', 60), ('Bob', 70), ('Charlie', 20);
    ///         "#,
    ///     )?;
    ///
    ///     let select = c.prepare_with("SELECT age FROM users ORDER BY age", Prepare::PERSISTENT)?;
    ///     let update = c.prepare_with("UPDATE users SET age = age + ?", Prepare::PERSISTENT)?;
    ///
    ///     // SAFETY: We serialize all accesses to the statements behind a mutex.
    ///     let inner = unsafe {
    ///         Statements {
    ///             select: select.into_send(),
    ///             update: update.into_send(),
    ///         }
    ///     };
    ///
    ///     Ok(Database {
    ///         stmts: Arc::new(Mutex::new(inner)),
    ///     })
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let db = setup_database()?;
    ///
    ///     let mut tasks = Vec::new();
    ///
    ///     for _ in 0..10 {
    ///         _ = task::spawn({
    ///             let db = db.clone();
    ///
    ///             async move {
    ///                 let mut stmts = db.stmts.lock_owned().await;
    ///                 let task = task::spawn_blocking(move || stmts.update.execute(2));
    ///                 Ok::<_, anyhow::Error>(task.await??)
    ///             }
    ///         });
    ///
    ///         let t = task::spawn({
    ///             let db = db.clone();
    ///
    ///             async move {
    ///                 let mut stmts = db.stmts.lock_owned().await;
    ///
    ///                 let task = task::spawn_blocking(move || -> Result<Option<i64>> {
    ///                     stmts.select.reset()?;
    ///                     Ok(stmts.select.next::<i64>()?)
    ///                 });
    ///
    ///                 task.await?
    ///             }
    ///         });
    ///
    ///         tasks.push(t);
    ///     }
    ///
    ///     for t in tasks {
    ///         let first = t.await??;
    ///         assert!(matches!(first, Some(20..=40)));
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub unsafe fn into_send(self) -> SendStatement {
        if !self.is_thread_safe {
            panic!("Database objects are not thread safe");
        }

        SendStatement { inner: self }
    }

    /// Get and read the next row from the statement using the [`Row`]
    /// trait.
    ///
    /// The [`Row`] trait is a convenience trait which is usually
    /// implemented using the [`Row` derive].
    ///
    /// Returns `None` when there are no more rows.
    ///
    /// This is a higher level API than `step` and is less prone to misuse. Note
    /// however that misuse never leads to corrupted data or undefined behavior,
    /// only surprising behavior such as NULL values being auto-converted (see
    /// [`Statement::step`]).
    ///
    /// Note that since this borrows from a mutable reference, it is *not*
    /// possible to decode multiple rows that borrow from the statement
    /// simultaneously. This is intentional since the state of the row is stored
    /// in the [`Statement`] from which it is returned.
    ///
    /// ```compile_fail
    /// use sqll::{Connection, Row};
    ///
    /// #[derive(Row)]
    /// struct Person {
    ///     name: String,
    ///     age: i64,
    /// }
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users")?;
    ///
    /// let a = stmt.next::<Person<'_>>()?;
    /// let b = stmt.next::<Person<'_>>()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// [`Row` derive]: derive@crate::Row
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Row};
    ///
    /// #[derive(Row)]
    /// struct Person {
    ///     name: String,
    ///     age: i64,
    /// }
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.bind(age)?;
    ///
    ///     while let Some(person) = stmt.next::<Person>()? {
    ///         results.push((person.name, person.age));
    ///     }
    /// }
    ///
    /// let expected = [
    ///     (String::from("Alice"), 72),
    ///     (String::from("Bob"), 40),
    ///     (String::from("Alice"), 72),
    /// ];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn next<'stmt, T>(&'stmt mut self) -> Result<Option<T>>
    where
        T: Row<'stmt>,
    {
        match self.step()? {
            State::Row => Ok(Some(T::from_row(self)?)),
            State::Done => Ok(None),
        }
    }

    /// Step the statement.
    ///
    /// This is necessary in order to produce rows from a statement. It must be
    /// called once before the first row is returned in order for results to be
    /// meaningful.
    ///
    /// When step returns [`State::Row`] it indicates that a row is ready to
    /// read from the statement. When step returns [`State::Done`] no more rows
    /// are available.
    ///
    /// For a less error-prone alternative, consider using [`Statement::next`].
    ///
    /// Trying to read data from a statement which has not been stepped will
    /// always result in a NULL value being read which will always result in an
    /// error.
    ///
    /// ```
    /// use sqll::{Connection, Code};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (id INTEGER, name TEXT);
    ///
    ///     INSERT INTO users (id, name) VALUES (0, 'Alice'), (1, 'Bob');
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT id, name FROM users;")?;
    /// assert_eq!(stmt.column::<i64>(0).unwrap_err().code(), Code::MISMATCH);
    /// assert_eq!(stmt.column::<String>(1).unwrap_err().code(), Code::MISMATCH);
    ///
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.column::<i64>(0)?, 0);
    /// assert_eq!(stmt.unsized_column::<str>(1)?, "Alice");
    ///
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.column::<i64>(0)?, 1);
    /// assert_eq!(stmt.unsized_column::<str>(1)?, "Bob");
    ///
    /// assert!(stmt.step()?.is_done());
    /// assert_eq!(stmt.column::<i64>(0).unwrap_err().code(), Code::MISMATCH);
    /// assert_eq!(stmt.column::<String>(1).unwrap_err().code(), Code::MISMATCH);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.bind(age)?;
    ///
    ///     while let Some(row) = stmt.next::<(String, i64)>()? {
    ///         results.push(row);
    ///     }
    /// }
    ///
    /// let expected = [
    ///     (String::from("Alice"), 72),
    ///     (String::from("Bob"), 40),
    ///     (String::from("Alice"), 72),
    /// ];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn step(&mut self) -> Result<State> {
        // SAFETY: We own the raw handle to this statement.
        unsafe {
            match ffi::sqlite3_step(self.raw.as_ptr()) {
                ffi::SQLITE_ROW => Ok(State::Row),
                ffi::SQLITE_DONE => Ok(State::Done),
                code => Err(Error::new(Code::new(code), self.error_message())),
            }
        }
    }

    /// In one call,  [`reset`] the statement, [`bind`] the specified values,
    /// and [`step`] until the current statement reports [`State::is_done`].
    ///
    /// This is a convenience wrapper around the these three operations since
    /// they are commonly used together.
    ///
    /// To not bind anything, use `()` as the argument.
    ///
    /// [`reset`]: Self::reset
    /// [`bind`]: Self::bind
    /// [`step`]: Self::step
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Result};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 42);
    ///     INSERT INTO users VALUES ('Bob', 69);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("UPDATE users SET age = age + 1")?;
    /// stmt.execute(())?;
    /// stmt.execute(())?;
    ///
    /// let mut query = c.prepare("SELECT age FROM users ORDER BY name")?;
    /// let results = query.iter::<i64>().collect::<Result<Vec<_>>>()?;
    /// assert_eq!(results, [44, 71]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn execute(&mut self, bind: impl Bind) -> Result<()> {
        self.reset()?;
        self.bind(bind)?;
        while !self.step()?.is_done() {}
        Ok(())
    }

    /// Coerce a statement into a typed iterator over the rows produced by this
    /// statement through the [`Row`] trait.
    ///
    /// Unlike [`next`], this does not support borrowing from the columns of the
    /// row because in order to allow multiple items to be accessed from the
    /// iterator each row has to be owned. Columns therefore has to used owned
    /// variants such as [`String`] or [`FixedBlob`].
    ///
    /// [`next`]: Self::next
    /// [`String`]: alloc::string::String
    /// [`FixedBlob`]: crate::FixedBlob
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Row, Result};
    ///
    /// #[derive(Row, Debug, PartialEq)]
    /// struct Person {
    ///     name: String,
    ///     age: i64,
    /// }
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > 40")?;
    ///
    /// stmt.reset()?;
    /// let results = stmt.iter::<(String, i64)>().collect::<Result<Vec<_>>>()?;
    /// let expected = [(String::from("Alice"), 72)];
    /// assert_eq!(results, expected);
    ///
    /// stmt.reset()?;
    /// let results = stmt.iter::<Person>().collect::<Result<Vec<_>>>()?;
    /// let expected = [Person { name: String::from("Alice"), age: 72 }];
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn iter<T>(&mut self) -> Iter<'_, T>
    where
        for<'stmt> T: Row<'stmt>,
    {
        Iter {
            stmt: self,
            _marker: PhantomData,
        }
    }

    /// Coerce a statement into an owned typed iterator over the rows produced
    /// by this statement through the [`Row`] trait.
    ///
    /// Unlike [`next`], this does not support borrowing from the columns of the
    /// row because in order to allow multiple items to be accessed from the
    /// iterator each row has to be owned. Columns therefore has to used owned
    /// variants such as [`String`] or [`FixedBlob`].
    ///
    /// [`next`]: Self::next
    /// [`String`]: alloc::string::String
    /// [`FixedBlob`]: crate::FixedBlob
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Row, Result};
    ///
    /// #[derive(Row, Debug, PartialEq)]
    /// struct Person {
    ///     name: String,
    ///     age: i64,
    /// }
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > 40")?;
    ///
    /// let results = c.prepare("SELECT * FROM users WHERE age > 40")?
    ///     .into_iter::<(String, i64)>().collect::<Result<Vec<_>>>()?;
    /// let expected = [(String::from("Alice"), 72)];
    /// assert_eq!(results, expected);
    ///
    /// let results = c.prepare("SELECT * FROM users WHERE age > 40")?
    ///     .into_iter::<Person>().collect::<Result<Vec<_>>>()?;
    /// let expected = [Person { name: String::from("Alice"), age: 72 }];
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn into_iter<T>(self) -> IntoIter<T>
    where
        for<'stmt> T: Row<'stmt>,
    {
        IntoIter {
            stmt: self,
            _marker: PhantomData,
        }
    }

    /// Reset the statement allowing it to be re-executed.
    ///
    /// The next call to [`Statement::step`] will start over from the first
    /// resulting row again.
    ///
    /// Note that resetting a statement doesn't unset bindings set by
    /// [`Statement::bind_value`]. To do this, use
    /// [`Statement::clear_bindings`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.bind(age)?;
    ///
    ///     while let Some(row) = stmt.next::<(String, i64)>()? {
    ///         results.push(row);
    ///     }
    /// }
    ///
    /// let expected = [
    ///     (String::from("Alice"), 72),
    ///     (String::from("Bob"), 40),
    ///     (String::from("Alice"), 72),
    /// ];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn reset(&mut self) -> Result<()> {
        unsafe { ffi::sqlite3_reset(self.raw.as_ptr()) };
        Ok(())
    }

    /// Contrary to the intuition of many, [`Statement::reset`] does not reset
    /// the bindings on a [`Statement`].
    ///
    /// Use this routine to reset all host parameters to NULL.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.bind(age)?;
    ///
    ///     while let Some(row) = stmt.next::<(String, i64)>()? {
    ///         results.push(row);
    ///     }
    /// }
    ///
    /// let expected = [
    ///     (String::from("Alice"), 72),
    ///     (String::from("Bob"), 40),
    ///     (String::from("Alice"), 72),
    /// ];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn clear_bindings(&mut self) -> Result<()> {
        unsafe { ffi::sqlite3_clear_bindings(self.raw.as_ptr()) };
        Ok(())
    }

    /// Reset the statement and bind values to parameters.
    ///
    /// This always binds to the first index, to specify a custom index use
    /// [`Statement::bind_value`] or configure the [`Bind` derive] with
    /// `#[sqll(index = ..)]`.
    ///
    /// If a statement is stepped without a parameter being bound, the parameter
    /// is bound by sqlite to `NULL` by default.
    ///
    /// [`Bind` derive]: derive@crate::Bind
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Bind};
    ///
    /// #[derive(Bind)]
    /// struct Binding<'a> {
    ///     name: &'a str,
    ///     age: u32,
    ///     order_by: &'a str,
    /// }
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 42);
    ///     INSERT INTO users VALUES ('Bob', 72);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT name, age FROM users WHERE name = ? AND age = ? ORDER BY ?")?;
    /// stmt.bind(Binding { name: "Bob", age: 72, order_by: "age" })?;
    ///
    /// assert_eq!(stmt.next::<(String, u32)>()?, Some(("Bob".to_string(), 72)));
    /// assert_eq!(stmt.next::<(String, u32)>()?, None);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn bind(&mut self, value: impl Bind) -> Result<()> {
        self.reset()?;
        value.bind(self)
    }

    /// Bind a value to a parameter by index.
    ///
    /// If a statement is stepped without a parameter being bound, the parameter
    /// is bound by sqlite to `NULL` by default.
    ///
    /// Bindings are sticky and are not cleared when the statement is [`reset`].
    /// To explicitly clear bindings you have to call [`clear_bindings`].
    ///
    /// [`reset`]: Self::reset
    /// [`clear_bindings`]: Self::clear_bindings
    ///
    /// # Errors
    ///
    /// The first parameter has index 1, attempting to bind to 0 will result in
    /// an error.
    ///
    /// ```
    /// use sqll::{Connection, Code};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name STRING)
    /// "#);
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE name = ?")?;
    /// let e = stmt.bind_value(0, "Bob").unwrap_err();
    /// assert_eq!(e.code(), Code::RANGE);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Code};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name STRING)
    /// "#);
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE name = ?")?;
    /// stmt.bind_value(1, "Bob")?;
    ///
    /// assert!(stmt.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn bind_value(&mut self, index: c_int, value: impl BindValue) -> Result<()> {
        value.bind_value(self, index)
    }

    /// Return the index for a named parameter if exists.
    ///
    /// Note that this takes a c-string as the parameter name since that is what
    /// the underlying API expects. To accomodate this, you can make use of the
    /// `c"string"` syntax.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name STRING)
    /// "#);
    ///
    /// let stmt = c.prepare("SELECT * FROM users WHERE name = :name")?;
    /// assert_eq!(stmt.bind_parameter_index(c":name"), Some(1));
    /// assert_eq!(stmt.bind_parameter_index(c":asdf"), None);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn bind_parameter_index(&self, parameter: impl AsRef<CStr>) -> Option<c_int> {
        let index = unsafe {
            ffi::sqlite3_bind_parameter_index(self.raw.as_ptr(), parameter.as_ref().as_ptr())
        };

        match index {
            0 => None,
            _ => Some(index),
        }
    }

    /// Return the number of columns in the result set returned by the
    /// [`Statement`]. If this routine returns 0, that means the [`Statement`]
    /// returns no data (for example an `UPDATE`).
    ///
    /// However, just because this routine returns a positive number does not
    /// mean that one or more rows of data will be returned.
    ///
    /// A `SELECT` statement will always have a positive
    /// [`column_count`] but depending on the `WHERE` clause constraints
    /// and the table content, it might return no rows.
    ///
    /// [`column_count`]: Self::column_count
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    /// "#)?;
    ///
    /// let mut select_stmt = c.prepare("SELECT * FROM users")?;
    /// assert_eq!(select_stmt.column_count(), 2);
    ///
    /// c.execute(r#"
    ///     ALTER TABLE users ADD COLUMN occupation TEXT;
    /// "#)?;
    ///
    /// assert_eq!(select_stmt.column_count(), 2);
    /// select_stmt.reset()?;
    /// assert_eq!(select_stmt.column_count(), 2);
    ///
    /// // In order to see the new column, we have to prepare a new statement.
    /// let select_stmt = c.prepare("SELECT * FROM users")?;
    /// assert_eq!(select_stmt.column_count(), 3);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn column_count(&self) -> c_int {
        unsafe { ffi::sqlite3_column_count(self.raw.as_ptr()) }
    }

    /// Return the name of a column.
    ///
    /// Note that column names might internally undergo some normalization by
    /// SQLite, since we provide an API where we return UTF-8, if the opened
    /// database stores them in UTF-16 an internal conversion will take place.
    ///
    /// Since we are not using the UTF-16 APIs, these conversions are cached and
    /// are expected to be one way. The returned references are therefore
    /// assumed to be valid for the shared lifetime of the statement.
    ///
    /// If an invalid index is specified or some other error internal to sqlite
    /// occurs, `None` is returned.
    ///
    /// ```
    /// use sqll::{Connection, Text};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    /// "#)?;
    ///
    /// let stmt = c.prepare("SELECT * FROM users;")?;
    ///
    /// assert_eq!(stmt.column_name(0), Some(Text::new("name")));
    /// assert_eq!(stmt.column_name(1), Some(Text::new("age")));
    /// assert_eq!(stmt.column_name(2), None);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    /// "#)?;
    ///
    /// let stmt = c.prepare("SELECT * FROM users;")?;
    ///
    /// let cols = stmt.columns().collect::<Vec<_>>();
    /// assert_eq!(cols, [0, 1]);
    /// assert_eq!(cols.iter().flat_map(|i| stmt.column_name(*i)).collect::<Vec<_>>(), ["name", "age"]);
    ///
    /// let cols = stmt.columns().rev().collect::<Vec<_>>();
    /// assert_eq!(cols, [1, 0]);
    /// assert_eq!(cols.iter().flat_map(|i| stmt.column_name(*i)).collect::<Vec<_>>(), ["age", "name"]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn column_name(&self, index: c_int) -> Option<&Text> {
        unsafe { c_to_text(ffi::sqlite3_column_name(self.raw.as_ptr(), index)) }
    }

    /// Return an iterator of column indexes.
    ///
    /// Column names are visible even when a prepared statement has not been
    /// advanced using [`Statement::step`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    /// "#)?;
    ///
    /// let stmt = c.prepare("SELECT * FROM users;")?;
    ///
    /// let cols = stmt.columns().collect::<Vec<_>>();
    /// assert_eq!(cols, vec![0, 1]);
    ///
    /// let cols = stmt.columns().rev().collect::<Vec<_>>();
    /// assert_eq!(cols, vec![1, 0]);
    ///
    /// let col = stmt.columns().nth(1);
    /// assert_eq!(col, Some(1));
    ///
    /// let col = stmt.columns().rev().nth(1);
    /// assert_eq!(col, Some(0));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn columns(&self) -> Columns {
        Columns {
            range: 0..self.column_count().max(0),
        }
    }

    /// Return an iterator of column names.
    ///
    /// Column names are visible even when a prepared statement has not been
    /// advanced using [`Statement::step`].
    ///
    /// Note that column names might internally undergo some normalization by
    /// SQLite, since we provide an API where we return UTF-8, if the opened
    /// database stores them in UTF-16 an internal conversion will take place.
    ///
    /// Since we are not using the UTF-16 APIs, these conversions are cached and
    /// are expected to be one way. The returned references are therefore
    /// assumed to be valid for the shared lifetime of the statement.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Text};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER, occupation TEXT);
    /// "#)?;
    ///
    /// let stmt = c.prepare("SELECT * FROM users;")?;
    ///
    /// let column_names = stmt.column_names().collect::<Vec<_>>();
    /// assert_eq!(column_names, vec![Text::new("name"), Text::new("age"), Text::new("occupation")]);
    ///
    /// let column_names = stmt.column_names().rev().collect::<Vec<_>>();
    /// assert_eq!(column_names, vec![Text::new("occupation"), Text::new("age"), Text::new("name")]);
    ///
    /// let name = stmt.column_names().nth(1);
    /// assert_eq!(name, Some(Text::new("age")));
    ///
    /// let name = stmt.column_names().nth(2);
    /// assert_eq!(name, Some(Text::new("occupation")));
    ///
    /// let name = stmt.column_names().rev().nth(2);
    /// assert_eq!(name, Some(Text::new("name")));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn column_names(&self) -> ColumnNames<'_> {
        ColumnNames {
            stmt: self,
            range: 0..self.column_count().max(0),
        }
    }

    /// Return the type of a column.
    ///
    /// The first column has index 0. The type becomes available after taking a
    /// step.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, ValueType};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age REAL, photo BLOB);
    ///
    ///     INSERT INTO users (id, name, age, photo) VALUES (1, 'Bob', 30.5, X'01020304');
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users")?;
    ///
    /// assert_eq!(stmt.column_type(0), ValueType::NULL);
    /// assert_eq!(stmt.column_type(1), ValueType::NULL);
    /// assert_eq!(stmt.column_type(2), ValueType::NULL);
    /// assert_eq!(stmt.column_type(3), ValueType::NULL);
    ///
    /// assert!(stmt.step()?.is_row());
    ///
    /// assert_eq!(stmt.column_type(0), ValueType::INTEGER);
    /// assert_eq!(stmt.column_type(1), ValueType::TEXT);
    /// assert_eq!(stmt.column_type(2), ValueType::FLOAT);
    /// assert_eq!(stmt.column_type(3), ValueType::BLOB);
    /// // Since the fifth column does not exist it is always `Null`.
    /// assert_eq!(stmt.column_type(4), ValueType::NULL);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn column_type(&self, index: c_int) -> ValueType {
        unsafe { ValueType::new(ffi::sqlite3_column_type(self.raw.as_ptr(), index)) }
    }

    /// Return the name for a bind parameter if it exists.
    ///
    /// If it does not exit, `None` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Text};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name STRING)
    /// "#);
    ///
    /// let stmt = c.prepare("SELECT * FROM users WHERE name = :name")?;
    /// assert_eq!(stmt.bind_parameter_name(1), Some(Text::new(":name")));
    /// assert_eq!(stmt.bind_parameter_name(2), None);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn bind_parameter_name(&self, index: c_int) -> Option<&Text> {
        unsafe { c_to_text(ffi::sqlite3_bind_parameter_name(self.raw.as_ptr(), index)) }
    }

    /// Read a value from the entire row using the [`Row`] trait.
    ///
    /// This is usually implemented using the [`Row` derive].
    ///
    /// [`Row` derive]: derive@crate::Row
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT name, age FROM users")?;
    ///
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.row::<(&str, i64)>()?, ("Alice", 72));
    ///
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.row::<(&str, i64)>()?, ("Bob", 40));
    ///
    /// assert!(stmt.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn row<'stmt, T>(&'stmt mut self) -> Result<T>
    where
        T: Row<'stmt>,
    {
        Row::from_row(self)
    }

    /// Get a single value from a column through [`FromColumn`].
    ///
    /// The first column has index 0. The same column can be read multiple
    /// times.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.bind(age)?;
    ///
    ///     while stmt.step()?.is_row() {
    ///         let name = stmt.column::<String>(0)?;
    ///         let age = stmt.column::<i64>(1)?;
    ///         results.push((name, age));
    ///     }
    /// }
    ///
    /// let expected = [
    ///     (String::from("Alice"), 72),
    ///     (String::from("Bob"), 40),
    ///     (String::from("Alice"), 72),
    /// ];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn column<'stmt, T>(&'stmt mut self, index: c_int) -> Result<T>
    where
        T: FromColumn<'stmt>,
    {
        let prepare = T::Type::check(self, index)?;
        T::from_column(self, prepare)
    }

    /// Borrow a value from a column using the [`FromUnsizedColumn`] trait.
    ///
    /// The first column has index 0. The same column can be read multiple
    /// times.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT name FROM users WHERE age > ?")?;
    ///
    /// for age in [30, 50] {
    ///     stmt.bind(age)?;
    ///
    ///     while stmt.step()?.is_row() {
    ///         let name = stmt.unsized_column::<str>(0)?;
    ///         assert!(matches!(name, "Alice" | "Bob"));
    ///     }
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn unsized_column<T>(&mut self, index: c_int) -> Result<&T>
    where
        T: ?Sized + FromUnsizedColumn,
    {
        let index = T::Type::check(self, index)?;
        T::from_unsized_column(self, index)
    }
}

impl Drop for Statement {
    #[inline]
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_finalize(self.raw.as_ptr()) };
    }
}

/// A typed iterator over the rows produced by a statement.
///
/// See [`Statement::iter`].
pub struct Iter<'stmt, T> {
    stmt: &'stmt mut Statement,
    _marker: PhantomData<T>,
}

impl<T> Iterator for Iter<'_, T>
where
    for<'stmt> T: Row<'stmt>,
{
    type Item = Result<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.stmt.step() {
            Ok(State::Row) => Some(T::from_row(self.stmt)),
            Ok(State::Done) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

/// An owned typed iterator over the rows produced by a statement.
///
/// See [`Statement::into_iter`].
pub struct IntoIter<T> {
    stmt: Statement,
    _marker: PhantomData<T>,
}

impl<T> Iterator for IntoIter<T>
where
    for<'stmt> T: Row<'stmt>,
{
    type Item = Result<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.stmt.step() {
            Ok(State::Row) => Some(T::from_row(&mut self.stmt)),
            Ok(State::Done) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

/// An iterator over the column names of a statement.
///
/// See [`Statement::column_names`].
pub struct ColumnNames<'a> {
    stmt: &'a Statement,
    range: Range<c_int>,
}

impl<'a> Iterator for ColumnNames<'a> {
    type Item = &'a Text;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.stmt.column_name(self.range.next()?)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.stmt.column_name(self.range.nth(n)?)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl<'a> DoubleEndedIterator for ColumnNames<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.stmt.column_name(self.range.next_back()?)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.stmt.column_name(self.range.nth_back(n)?)
    }
}

impl ExactSizeIterator for ColumnNames<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.range.len()
    }
}

/// An iterator over the column names of a statement.
///
/// See [`Statement::columns`].
pub struct Columns {
    range: Range<c_int>,
}

impl Iterator for Columns {
    type Item = c_int;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.range.next()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.range.nth(n)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for Columns {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.range.next_back()
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.range.nth_back(n)
    }
}

/// A [`Statement`] that can be sent between threads.
///
/// Constructed using [`Statement::into_send`].
pub struct SendStatement {
    inner: Statement,
}

unsafe impl Send for SendStatement {}

impl Deref for SendStatement {
    type Target = Statement;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SendStatement {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
