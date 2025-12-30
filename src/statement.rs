use core::ffi::{CStr, c_int};
use core::fmt;
use core::ptr;
use core::slice;

use sqlite3_sys as ffi;

use crate::{Bindable, Error, Readable, Result, Type, Writable};

/// A marker type representing a NULL value.
pub struct Null;

/// A prepared statement.
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

/// A state of a prepared statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum State {
    /// There is a row available for reading.
    Row,
    /// The statement has been entirely evaluated.
    Done,
}

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
    pub fn bind<T>(&mut self, index: c_int, value: T) -> Result<()>
    where
        T: Bindable,
    {
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

    /// Return an iterator of columns.
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
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn columns(&self) -> Columns {
        Columns {
            start: 0,
            end: self.column_count(),
        }
    }

    /// Return an iterator of column names.
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
    /// let column_names = stmt.column_names().collect::<Vec<_>>();
    /// assert_eq!(column_names, vec!["name", "age"]);
    ///
    /// let column_names = stmt.column_names().rev().collect::<Vec<_>>();
    /// assert_eq!(column_names, vec!["age", "name"]);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn column_names(&self) -> ColumnNames<'_> {
        ColumnNames {
            stmt: self,
            start: 0,
            end: self.column_count(),
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

    /// Step to the next state.
    ///
    /// The function should be called multiple times until `State::Done` is
    /// reached in order to evaluate the statement entirely.
    pub fn step(&mut self) -> Result<State> {
        unsafe {
            match ffi::sqlite3_step(self.raw.as_ptr()) {
                ffi::SQLITE_ROW => Ok(State::Row),
                ffi::SQLITE_DONE => Ok(State::Done),
                _ => {
                    let handle = ffi::sqlite3_db_handle(self.raw.as_ptr());
                    let code = ffi::sqlite3_errcode(handle);
                    Err(Error::new(code))
                }
            }
        }
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

    /// Read a value from a column.
    ///
    /// The first column has index 0.
    #[inline]
    pub fn read<T>(&self, index: c_int) -> Result<T>
    where
        T: Readable,
    {
        Readable::read(self, index)
    }

    /// Read a value from a column into the provided [`Writable`].
    ///
    /// This can be much more efficient than calling `read` since you can
    /// provide your own buffers.
    #[inline]
    pub fn read_into<T>(&self, index: c_int, out: &mut T) -> Result<()>
    where
        T: ?Sized + Writable,
    {
        debug_assert!(
            index < self.column_count(),
            "the index is out of bounds 0..{}",
            self.column_count()
        );

        out.write(self, index)?;
        Ok(())
    }

    /// Reset the statement allowing it to be re-used.
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
    /// let expected = vec![
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
    start: c_int,
    end: c_int,
}

impl<'a> Iterator for ColumnNames<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }

        let name = self.stmt.column_name(self.start);
        self.start += 1;
        name
    }
}

impl<'a> DoubleEndedIterator for ColumnNames<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }

        self.end -= 1;
        self.stmt.column_name(self.end)
    }
}

/// An iterator over the column names of a statement.
///
/// See [`Statement::columns`].
pub struct Columns {
    start: c_int,
    end: c_int,
}

impl Iterator for Columns {
    type Item = c_int;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }

        let index = self.start;
        self.start += 1;
        Some(index)
    }
}

impl DoubleEndedIterator for Columns {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }

        self.end -= 1;
        Some(self.end)
    }
}
