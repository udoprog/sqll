use core::ffi::{CStr, c_int};
use core::fmt;
use core::marker::PhantomData;
use core::ops::Range;
use core::ptr;
use core::slice;

use crate::ffi;
use crate::utils::{c_to_errstr, c_to_str};
use crate::{
    Bind, BindValue, Code, Error, FromColumn, FromRow, FromUnsizedColumn, Result, Sink, Type,
};

/// A marker type representing a NULL value.
///
/// This can be used both as [`BindValue`] and [`FromColumn`].
///
/// See [`Statement::bind_value`] and [`Statement::get`].
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
/// Prepared statements are compiled using [`Connection::prepare`] or
/// [`Connection::prepare_with`]. The [`Connection`] which constructed the
/// prepared statement will remain alive for as long as the statement is alive,
/// even if the connection is dropped.
///
/// They can be re-used, but between each re-use they must be reset using
/// [`reset`]. A defensive coding style suggests its appropriate to always call
/// this before using a statement unless it was just created. A call to
/// [`reset`] must also be done to refresh the prepared statement with respects
/// to changes in the database.
///
/// For durable prepared statements it is recommended that
/// [`Connection::prepare_with`] is used with [`Prepare::PERSISTENT`] set.
///
/// [`Connection::prepare_with`]: crate::Connection::prepare_with
/// [`Connection::prepare`]: crate::Connection::prepare
/// [`Connection`]: crate::Connection
/// [`Prepare::PERSISTENT`]: crate::Prepare::PERSISTENT
/// [`reset`]: Self::reset
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
/// insert_stmt.reset()?;
/// insert_stmt.bind_value(1, 42)?;
/// assert!(insert_stmt.step()?.is_done());
///
/// query.reset()?;
///
/// while let Some(value) = query.next::<i64>()? {
///     assert_eq!(value, 42);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
#[repr(transparent)]
pub struct Statement {
    raw: ptr::NonNull<ffi::sqlite3_stmt>,
}

impl fmt::Debug for Statement {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Statement").finish_non_exhaustive()
    }
}

/// A prepared statement is `Send`.
#[cfg(feature = "threadsafe")]
unsafe impl Send for Statement {}

impl Statement {
    /// Construct a statement from a raw pointer.
    #[inline]
    pub(crate) fn from_raw(raw: ptr::NonNull<ffi::sqlite3_stmt>) -> Statement {
        Statement { raw }
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

    pub(crate) fn error_message(&self) -> &str {
        unsafe {
            let db = ffi::sqlite3_db_handle(self.as_ptr());
            let msg_ptr = ffi::sqlite3_errmsg(db);
            c_to_errstr(msg_ptr)
        }
    }

    /// Get and read the next row from the statement using the [`FromRow`]
    /// trait.
    ///
    /// The [`FromRow`] trait is a convenience trait which is usually
    /// implemented using the [`FromRow` derive].
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
    /// use sqll::{Connection, FromRow};
    ///
    /// #[derive(FromRow)]
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
    /// [`FromRow` derive]: derive@crate::FromRow
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FromRow};
    ///
    /// #[derive(FromRow)]
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
    ///     stmt.reset()?;
    ///     stmt.bind_value(1, age)?;
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
    pub fn next<'stmt, T>(&'stmt mut self) -> Result<Option<T>>
    where
        T: FromRow<'stmt>,
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
    /// assert_eq!(stmt.get::<i64>(0).unwrap_err().code(), Code::MISMATCH);
    /// assert_eq!(stmt.get::<String>(1).unwrap_err().code(), Code::MISMATCH);
    ///
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.get::<i64>(0)?, 0);
    /// assert_eq!(stmt.get_unsized::<str>(1)?, "Alice");
    ///
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.get::<i64>(0)?, 1);
    /// assert_eq!(stmt.get_unsized::<str>(1)?, "Bob");
    ///
    /// assert!(stmt.step()?.is_done());
    /// assert_eq!(stmt.get::<i64>(0).unwrap_err().code(), Code::MISMATCH);
    /// assert_eq!(stmt.get::<String>(1).unwrap_err().code(), Code::MISMATCH);
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
    ///     stmt.reset()?;
    ///     stmt.bind_value(1, age)?;
    ///
    ///     while stmt.step()?.is_row() {
    ///         results.push((stmt.get::<String>(0)?, stmt.get::<i64>(1)?));
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
    pub fn step(&mut self) -> Result<State> {
        // SAFETY: We own the raw handle to this statement.
        unsafe {
            match ffi::sqlite3_step(self.raw.as_ptr()) {
                ffi::SQLITE_ROW => Ok(State::Row),
                ffi::SQLITE_DONE => Ok(State::Done),
                code => Err(Error::from_raw(code, self.error_message())),
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
    /// statement through the [`FromRow`] trait.
    ///
    /// This does not support borrowing from the statement, because a statement
    /// stores the state for each row.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FromRow, Result};
    ///
    /// #[derive(FromRow, Debug, PartialEq)]
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
        for<'stmt> T: FromRow<'stmt>,
    {
        Iter {
            stmt: self,
            _marker: PhantomData,
        }
    }

    /// Coerce a statement into an owned typed iterator over the rows produced
    /// by this statement through the [`FromRow`] trait.
    ///
    /// This does not support borrowing from the statement, because a statement
    /// stores the state for each row.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FromRow, Result};
    ///
    /// #[derive(FromRow, Debug, PartialEq)]
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
        for<'stmt> T: FromRow<'stmt>,
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
    ///     stmt.reset()?;
    ///     stmt.bind_value(1, age)?;
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
    ///     stmt.reset()?;
    ///     stmt.bind_value(1, age)?;
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

    /// Bind a value to a parameter by name.
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
    /// let mut statement = c.prepare("SELECT * FROM users WHERE name = :name")?;
    /// statement.bind_value_by_name(c":name", "Bob")?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn bind_value_by_name(
        &mut self,
        name: impl AsRef<CStr>,
        value: impl BindValue,
    ) -> Result<()> {
        let Some(index) = self.bind_parameter_index(name) else {
            return Err(Error::new(Code::ERROR, "no such bind parameter"));
        };

        self.bind_value(index, value)?;
        Ok(())
    }

    /// Return the number of columns in the result set returned by the
    /// [`Statement`]. If this routine returns 0, that means the [`Statement`]
    /// returns no data (for example an `UPDATE`).
    ///
    /// However, just because this routine returns a positive number does not
    /// mean that one or more rows of data will be returned.
    ///
    /// A SELECT statement will always have a positive [`Self::column_count()`]
    /// but depending on the WHERE clause constraints and the table content, it
    /// might return no rows.
    #[inline]
    pub fn column_count(&self) -> c_int {
        unsafe { ffi::sqlite3_column_count(self.raw.as_ptr()) }
    }

    /// Return the name of a column.
    ///
    /// If an invalid index is specified, `None` is returned.
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
    /// assert_eq!(stmt.column_name(0), Some("name"));
    /// assert_eq!(stmt.column_name(1), Some("age"));
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
    /// assert_eq!(cols, vec![0, 1]);
    /// assert!(cols.iter().flat_map(|i| stmt.column_name(*i)).eq(["name", "age"]));
    ///
    /// let cols = stmt.columns().rev().collect::<Vec<_>>();
    /// assert_eq!(cols, vec![1, 0]);
    /// assert!(cols.iter().flat_map(|i| stmt.column_name(*i)).eq(["age", "name"]));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn column_name(&self, index: c_int) -> Option<&str> {
        unsafe {
            let ptr = ffi::sqlite3_column_name(self.raw.as_ptr(), index);

            if ptr.is_null() {
                return None;
            }

            // NB: Look for the null terminator. sqlite guarantees that it's in
            // here somewhere. Unfortunately we have to go byte-by-byte since we
            // don't know the extend of the string being returned.
            for len in 0.. {
                if ptr.add(len).read() == 0 {
                    let bytes = slice::from_raw_parts(ptr.cast(), len);
                    let s = str::from_utf8_unchecked(bytes);
                    return Some(s);
                }
            }

            None
        }
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
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
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
    /// assert_eq!(column_names, vec!["name", "age", "occupation"]);
    ///
    /// let column_names = stmt.column_names().rev().collect::<Vec<_>>();
    /// assert_eq!(column_names, vec!["occupation", "age", "name"]);
    ///
    /// let name = stmt.column_names().nth(1);
    /// assert_eq!(name, Some("age"));
    ///
    /// let name = stmt.column_names().nth(2);
    /// assert_eq!(name, Some("occupation"));
    ///
    /// let name = stmt.column_names().rev().nth(2);
    /// assert_eq!(name, Some("name"));
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
    /// use sqll::{Connection, Type};
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
    /// assert_eq!(stmt.column_type(0), Type::NULL);
    /// assert_eq!(stmt.column_type(1), Type::NULL);
    /// assert_eq!(stmt.column_type(2), Type::NULL);
    /// assert_eq!(stmt.column_type(3), Type::NULL);
    ///
    /// assert!(stmt.step()?.is_row());
    ///
    /// assert_eq!(stmt.column_type(0), Type::INTEGER);
    /// assert_eq!(stmt.column_type(1), Type::TEXT);
    /// assert_eq!(stmt.column_type(2), Type::FLOAT);
    /// assert_eq!(stmt.column_type(3), Type::BLOB);
    /// // Since the fifth column does not exist it is always `Null`.
    /// assert_eq!(stmt.column_type(4), Type::NULL);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn column_type(&self, index: c_int) -> Type {
        unsafe { Type::new(ffi::sqlite3_column_type(self.raw.as_ptr(), index)) }
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

    /// Return the name for a bind parameter if it exists.
    ///
    /// If it does not exit, `None` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// # let c = sqll::Connection::open(":memory:")?;
    /// c.execute(r#"
    ///     CREATE TABLE users (name STRING)
    /// "#);
    /// let stmt = c.prepare("SELECT * FROM users WHERE name = :name")?;
    /// assert_eq!(stmt.bind_parameter_name(1), Some(":name"));
    /// assert_eq!(stmt.bind_parameter_name(2), None);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn bind_parameter_name(&self, index: c_int) -> Option<&str> {
        unsafe { c_to_str(ffi::sqlite3_bind_parameter_name(self.raw.as_ptr(), index)) }
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
    ///     stmt.reset()?;
    ///     stmt.bind_value(1, age)?;
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
    pub fn get<'stmt, T>(&'stmt self, index: c_int) -> Result<T>
    where
        T: FromColumn<'stmt>,
    {
        FromColumn::from_column(self, index)
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
    ///     stmt.reset()?;
    ///     stmt.bind_value(1, age)?;
    ///
    ///     while stmt.step()?.is_row() {
    ///         assert!(matches!(stmt.get_unsized::<str>(0)?, "Alice" | "Bob"));
    ///     }
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn get_unsized<T>(&self, index: c_int) -> Result<&T>
    where
        T: ?Sized + FromUnsizedColumn,
    {
        FromUnsizedColumn::from_unsized_column(self, index)
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
    /// let mut stmt = c.prepare("SELECT name, age FROM users")?;
    ///
    /// while stmt.step()?.is_row() {
    ///     assert!(matches!(stmt.get_row::<(&str, i64)>()?, ("Alice", 72) | ("Bob", 40)));
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn get_row<'stmt, T>(&'stmt self) -> Result<T>
    where
        T: FromRow<'stmt>,
    {
        FromRow::from_row(self)
    }

    /// Read a value from a column into the provided [`Sink`].
    ///
    /// The first column has index 0. The same column can be read multiple
    /// times.
    ///
    /// This can be much more efficient than calling `read` since you can
    /// provide your own buffers.
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
    /// let mut name = String::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind_value(1, age)?;
    ///
    ///     while stmt.step()?.is_row() {
    ///         name.clear();
    ///         stmt.read(0, &mut name)?;
    ///
    ///         if name == "Bob" {
    ///             results.push(stmt.get::<i64>(1)?);
    ///         }
    ///     }
    /// }
    ///
    /// assert_eq!(results, [40]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn read(&self, index: c_int, mut out: impl Sink) -> Result<()> {
        out.write(self, index)?;
        Ok(())
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
    for<'stmt> T: FromRow<'stmt>,
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
    for<'stmt> T: FromRow<'stmt>,
{
    type Item = Result<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.stmt.step() {
            Ok(State::Row) => Some(T::from_row(&self.stmt)),
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
    type Item = &'a str;

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
