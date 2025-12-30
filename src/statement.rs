use core::ffi::{CStr, c_int};
use core::fmt;
use core::ops::Range;
use core::ptr;
use core::slice;

use sqlite3_sys as ffi;

use crate::{Bindable, Error, Readable, Result, Type, Writable};

/// A marker type representing a NULL value.
///
/// This can be used with both [`Bindable`] and [`Writable`].
///
/// See [`Statement::bind`] and [`Statement::read`].
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

/// A prepared statement.
///
/// Prepared statements are compiled using [`Connection::prepare`] or
/// [`Connection::prepare_with`].
///
/// They can be re-used, but between each re-use they must be reset using
/// [`Statement::reset`]. Defensive coding would suggest its appropriate to
/// always call this before using a statement unless it was just created.
///
/// For durable prepared statements it is recommended that
/// [`Connection::prepare_with`] is used with [`Prepare::PERSISTENT`] set.
///
/// [`Connection::prepare`]: crate::Connection::prepare
/// [`Connection::prepare_with`]: crate::Connection::prepare_with
/// [`Prepare::PERSISTENT`]: crate::Prepare::PERSISTENT
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State, Prepare};
///
/// let c = Connection::memory()?;
/// c.execute("CREATE TABLE test (id INTEGER);")?;
///
/// let mut insert_stmt = c.prepare_with("INSERT INTO test (id) VALUES (?);", Prepare::PERSISTENT)?;
/// let mut query_stmt = c.prepare_with("SELECT id FROM test;", Prepare::PERSISTENT)?;
///
/// drop(c);
///
/// /* .. */
///
/// insert_stmt.reset()?;
/// insert_stmt.bind(1, 42)?;
/// assert_eq!(insert_stmt.step()?, State::Done);
///
/// query_stmt.reset()?;
///
/// while let Some(mut row) = query_stmt.next()? {
///     let id: i64 = row.read(0)?;
///     assert_eq!(id, 42);
/// }
/// # Ok::<_, sqlite_ll::Error>(())
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

    /// Get the next row from the statement.
    ///
    /// Returns `None` when there are no more rows.
    ///
    /// This is a higher level API than `step` and is less prone to misuse. Note
    /// however that misuse never leads to corrupted data or undefined behavior,
    /// only surprising behavior such as NULL values being auto-converted (see
    /// [`Statement::step`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Connection;
    ///
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let Some(row) = stmt.next()? {
    ///         results.push((row.read::<String>(0)?, row.read::<i64>(1)?));
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
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    pub fn next(&mut self) -> Result<Option<Row<'_>>> {
        match self.step()? {
            State::Row => Ok(Some(Row { stmt: self })),
            State::Done => Ok(None),
        }
    }

    /// Step the statement.
    ///
    /// This is necessary in order to produce rows from a statement. It must be
    /// called once before the first row is returned. Trying to read data from a
    /// statement which has not been stepped will always result in a NULL value
    /// being read which is subject to auto-conversion.
    ///
    /// ```
    /// use sqlite_ll::{Connection, State, Code};
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (id INTEGER, name TEXT);")?;
    /// c.execute("INSERT INTO users (id, name) VALUES (0, 'Alice'), (1, 'Bob');")?;
    ///
    /// let mut stmt = c.prepare("SELECT name FROM users;")?;
    /// assert_eq!(stmt.read::<i64>(0)?, 0);
    /// assert_eq!(stmt.read::<String>(0)?, "");
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    ///
    /// When the statement returns [`State::Done`] no more rows are available.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State};
    ///
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let State::Row = stmt.step()? {
    ///         results.push((stmt.read::<String>(0)?, stmt.read::<i64>(1)?));
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
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    pub fn step(&mut self) -> Result<State> {
        // SAFETY: We own the raw handle to this statement.
        unsafe {
            match ffi::sqlite3_step(self.raw.as_ptr()) {
                ffi::SQLITE_ROW => Ok(State::Row),
                ffi::SQLITE_DONE => Ok(State::Done),
                code => Err(Error::new(code)),
            }
        }
    }

    /// Reset the statement allowing it to be re-used.
    ///
    /// Resetting a statement unsets all bindings set by [`Statement::bind`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State};
    ///
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let State::Row = stmt.step()? {
    ///         results.push((stmt.read::<String>(0)?, stmt.read::<i64>(1)?));
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
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn reset(&mut self) -> Result<()> {
        unsafe { ffi::sqlite3_reset(self.raw.as_ptr()) };
        Ok(())
    }

    /// Bind a value to a parameter by index.
    ///
    /// # Errors
    ///
    /// The first parameter has index 1, attempting to bind to 0 will result in an error.
    ///
    /// ```
    /// use sqlite_ll::{Connection, Null, Code};
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (name STRING)");
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE name = ?")?;
    /// let e = stmt.bind(0, "Bob").unwrap_err();
    /// assert_eq!(e.code(), Code::RANGE);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, Null, Code, State};
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (name STRING)");
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE name = ?")?;
    /// stmt.bind(1, "Bob")?;
    ///
    /// assert_eq!(stmt.step()?, State::Done);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn bind(&mut self, index: c_int, value: impl Bindable) -> Result<()> {
        value.bind(self, index)
    }

    /// Bind a value to a parameter by name.
    ///
    /// # Examples
    ///
    /// ```
    /// # let c = sqlite_ll::Connection::open(":memory:")?;
    /// # c.execute("CREATE TABLE users (name STRING)");
    /// let mut statement = c.prepare("SELECT * FROM users WHERE name = :name")?;
    /// statement.bind_by_name(c":name", "Bob")?;
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    pub fn bind_by_name(&mut self, name: impl AsRef<CStr>, value: impl Bindable) -> Result<()> {
        if let Some(index) = self.parameter_index(name) {
            self.bind(index, value)?;
            Ok(())
        } else {
            Err(Error::new(ffi::SQLITE_MISMATCH))
        }
    }

    /// Return the number of columns.
    #[inline]
    pub fn column_count(&self) -> c_int {
        unsafe { ffi::sqlite3_column_count(self.raw.as_ptr()) }
    }

    /// Return the name of a column.
    ///
    /// If an invalid index is specified, `None` is returned.
    ///
    /// ```
    /// use sqlite_ll::Connection;
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER);")?;
    /// let stmt = c.prepare("SELECT * FROM users;")?;
    ///
    /// assert_eq!(stmt.column_name(0), Some("name"));
    /// assert_eq!(stmt.column_name(1), Some("age"));
    /// assert_eq!(stmt.column_name(2), None);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Connection;
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER);")?;
    /// let stmt = c.prepare("SELECT * FROM users;")?;
    ///
    /// let cols = stmt.columns().collect::<Vec<_>>();
    /// assert_eq!(cols, vec![0, 1]);
    /// assert!(cols.iter().flat_map(|i| stmt.column_name(*i)).eq(["name", "age"]));
    ///
    /// let cols = stmt.columns().rev().collect::<Vec<_>>();
    /// assert_eq!(cols, vec![1, 0]);
    /// assert!(cols.iter().flat_map(|i| stmt.column_name(*i)).eq(["age", "name"]));
    /// # Ok::<_, sqlite_ll::Error>(())
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
    /// use sqlite_ll::Connection;
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER);")?;
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
    /// # Ok::<_, sqlite_ll::Error>(())
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
    /// use sqlite_ll::Connection;
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER, occupation TEXT);")?;
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
    /// # Ok::<_, sqlite_ll::Error>(())
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
    /// use sqlite_ll::{Connection, Type, State};
    ///
    /// let mut c = Connection::memory()?;
    ///
    /// c.execute(r##"
    /// CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age REAL, photo BLOB);
    /// "##)?;
    ///
    /// c.execute(r##"
    /// INSERT INTO users (id, name, age, photo) VALUES (1, 'Bob', 30.5, X'01020304');
    /// "##)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users")?;
    ///
    /// assert_eq!(stmt.column_type(0), Type::NULL);
    /// assert_eq!(stmt.column_type(1), Type::NULL);
    /// assert_eq!(stmt.column_type(2), Type::NULL);
    /// assert_eq!(stmt.column_type(3), Type::NULL);
    ///
    /// assert_eq!(stmt.step()?, State::Row);
    ///
    /// assert_eq!(stmt.column_type(0), Type::INTEGER);
    /// assert_eq!(stmt.column_type(1), Type::TEXT);
    /// assert_eq!(stmt.column_type(2), Type::FLOAT);
    /// assert_eq!(stmt.column_type(3), Type::BLOB);
    /// // Since the fifth column does not exist it is always `Null`.
    /// assert_eq!(stmt.column_type(4), Type::NULL);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn column_type(&self, index: c_int) -> Type {
        unsafe { Type::from_raw(ffi::sqlite3_column_type(self.raw.as_ptr(), index)) }
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
    /// # let c = sqlite_ll::Connection::open(":memory:")?;
    /// c.execute("CREATE TABLE users (name STRING)");
    /// let stmt = c.prepare("SELECT * FROM users WHERE name = :name")?;
    /// assert_eq!(stmt.parameter_index(c":name"), Some(1));
    /// assert_eq!(stmt.parameter_index(c":asdf"), None);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn parameter_index(&self, parameter: impl AsRef<CStr>) -> Option<c_int> {
        let index = unsafe {
            ffi::sqlite3_bind_parameter_index(self.raw.as_ptr(), parameter.as_ref().as_ptr())
        };

        match index {
            0 => None,
            _ => Some(index),
        }
    }

    /// Read a value from a column into a [`Readable`].
    ///
    /// The first column has index 0. The same column can be read multiple
    /// times.
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State};
    ///
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let State::Row = stmt.step()? {
    ///         results.push((stmt.read::<String>(0)?, stmt.read::<i64>(1)?));
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
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn read<T>(&self, index: c_int) -> Result<T>
    where
        T: Readable,
    {
        Readable::read(self, index)
    }

    /// Read a value from a column into the provided [`Writable`].
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
    /// use sqlite_ll::{Connection, State};
    ///
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    /// let mut name_buffer = String::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let State::Row = stmt.step()? {
    ///         name_buffer.clear();
    ///         stmt.read_into(0, &mut name_buffer)?;
    ///
    ///         if name_buffer == "Bob" {
    ///             results.push(stmt.read::<i64>(1)?);
    ///         }
    ///     }
    /// }
    ///
    /// let expected = [40];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn read_into(&self, index: c_int, out: &mut (impl ?Sized + Writable)) -> Result<()> {
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

/// A row produced by a statement.
///
/// See [`Statement::next`].
pub struct Row<'a> {
    stmt: &'a mut Statement,
}

impl<'a> Row<'a> {
    /// Read a value from a column into a [`Readable`].
    ///
    /// The first column has index 0. The same column can be read multiple
    /// times.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Connection;
    ///
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let Some(row) = stmt.next()? {
    ///         results.push((row.read::<String>(0)?, row.read::<i64>(1)?));
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
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn read<T>(&self, index: c_int) -> Result<T>
    where
        T: Readable,
    {
        Readable::read(self.stmt, index)
    }

    /// Read a value from a column into the provided [`Writable`].
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
    /// use sqlite_ll::{Connection, State};
    ///
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    /// let mut name_buffer = String::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let Some(row) = stmt.next()? {
    ///         name_buffer.clear();
    ///         row.read_into(0, &mut name_buffer)?;
    ///
    ///         if name_buffer == "Bob" {
    ///             results.push(row.read::<i64>(1)?);
    ///         }
    ///     }
    /// }
    ///
    /// let expected = [40];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn read_into(&self, index: c_int, out: &mut (impl ?Sized + Writable)) -> Result<()> {
        out.write(self.stmt, index)?;
        Ok(())
    }
}
