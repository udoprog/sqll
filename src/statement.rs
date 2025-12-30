use core::ffi::{CStr, c_int};
use core::fmt;
use core::mem::MaybeUninit;
use core::ptr;
use core::slice;

use alloc::string::String;
use alloc::vec::Vec;

use sqlite3_sys as ffi;

use crate::bytes;
use crate::error::{Error, Result};
use crate::utils::sqlite3_try;
use crate::value::{Kind, Type, Value};

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

mod sealed_bindable {
    use crate::{Null, Value};

    pub trait Sealed {}
    impl Sealed for str {}
    impl Sealed for [u8] {}
    impl Sealed for f64 {}
    impl Sealed for i64 {}
    impl Sealed for Value {}
    impl Sealed for Null {}
    impl<T> Sealed for Option<T> where T: Sealed {}
    impl<T> Sealed for &T where T: ?Sized + Sealed {}
}

/// A type suitable for binding to a prepared statement.
///
/// Use with [`Statement::bind`] or [`Statement::bind_by_name`].
pub trait Bindable
where
    Self: self::sealed_bindable::Sealed,
{
    #[doc(hidden)]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()>;
}

impl<T> Bindable for &T
where
    T: ?Sized + Bindable,
{
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        (**self).bind(stmt, index)
    }
}

mod sealed_writable {
    use alloc::string::String;
    use alloc::vec::Vec;

    pub trait Sealed {}
    impl Sealed for String {}
    impl Sealed for Vec<u8> {}
}

/// Trait governing types which can be written to in-place.
///
/// Use with [`Statement::read_into`].
pub trait Writable
where
    Self: self::sealed_writable::Sealed,
{
    #[doc(hidden)]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()>;
}

mod sealed_readable {
    use alloc::string::String;
    use alloc::vec::Vec;

    use crate::{FixedBytes, Null, Value};

    pub trait Sealed {}
    impl Sealed for i64 {}
    impl Sealed for f64 {}
    impl Sealed for Null {}
    impl Sealed for String {}
    impl Sealed for Vec<u8> {}
    impl<T> Sealed for Option<T> where T: Sealed {}
    impl<const N: usize> Sealed for FixedBytes<N> {}
    impl Sealed for Value {}
}

/// A type suitable for reading from a prepared statement.
///
/// Use with [`Statement::read`].
pub trait Readable
where
    Self: self::sealed_readable::Sealed + Sized,
{
    #[doc(hidden)]
    fn read(stmt: &Statement, index: c_int) -> Result<Self>;
}

impl Statement {
    /// Construct a statement from a raw pointer.
    #[inline]
    pub(crate) fn from_raw(raw: ptr::NonNull<ffi::sqlite3_stmt>) -> Statement {
        Statement { raw }
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

impl Bindable for Value {
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        match &self.kind {
            Kind::Blob(value) => value.as_slice().bind(stmt, index),
            Kind::Float(value) => value.bind(stmt, index),
            Kind::Integer(value) => value.bind(stmt, index),
            Kind::Text(value) => value.as_str().bind(stmt, index),
            Kind::Null => Null.bind(stmt, index),
        }
    }
}

impl Bindable for [u8] {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        let (ptr, len, dealloc) = bytes::alloc(self)?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_blob(
                    stmt.raw.as_ptr(),
                    index,
                    ptr,
                    len,
                    dealloc,
                )
            };
        }

        Ok(())
    }
}

impl Bindable for f64 {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_double(
                    stmt.raw.as_ptr(),
                    index,
                    *self
                )
            };
        }

        Ok(())
    }
}

impl Bindable for i64 {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_int64(
                    stmt.raw.as_ptr(),
                    index,
                    *self as ffi::sqlite3_int64
                )
            };
        }

        Ok(())
    }
}

impl Bindable for str {
    #[inline]
    fn bind(&self, stmt: &mut Statement, i: c_int) -> Result<()> {
        let (data, len, dealloc) = bytes::alloc(self.as_bytes())?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_text(
                    stmt.raw.as_ptr(),
                    i,
                    data.cast(),
                    len,
                    dealloc,
                )
            };
        }

        Ok(())
    }
}

impl Bindable for Null {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_null(stmt.raw.as_ptr(), index)
            };
        }

        Ok(())
    }
}

impl<T> Bindable for Option<T>
where
    T: Bindable,
{
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        match self {
            Some(inner) => inner.bind(stmt, index),
            None => Null.bind(stmt, index),
        }
    }
}

impl Readable for Value {
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let value = match stmt.column_type(index) {
            Type::BLOB => Value::blob(Readable::read(stmt, index)?),
            Type::TEXT => Value::text(Readable::read(stmt, index)?),
            Type::FLOAT => Value::float(Readable::read(stmt, index)?),
            Type::INTEGER => Value::integer(Readable::read(stmt, index)?),
            Type::NULL => Value::null(),
            _ => return Err(Error::new(ffi::SQLITE_MISMATCH)),
        };

        Ok(value)
    }
}

impl Readable for f64 {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_double(stmt.raw.as_ptr(), index) })
    }
}

impl Readable for i64 {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.raw.as_ptr(), index) })
    }
}

/// [`Readable`] implementation which returns a newly allocated [`String`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Writable`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<String>(0)?;
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<String>(0)?;
///     assert!(matches!(name.as_str(), "1" | "2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Readable for String {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut s = String::new();
        s.write(stmt, index)?;
        Ok(s)
    }
}

/// [`Writable`] implementation for [`String`] which appends the content of the
/// column to the current container.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
/// let mut name = String::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
/// let mut name = String::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_str(), "1" | "2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Writable for String {
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        unsafe {
            let len = ffi::sqlite3_column_bytes(stmt.raw.as_ptr(), index);

            let Ok(len) = usize::try_from(len) else {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            };

            if len == 0 {
                return Ok(());
            }

            // SAFETY: This is guaranteed to return valid UTF-8 by sqlite.
            let ptr = ffi::sqlite3_column_text(stmt.raw.as_ptr(), index);

            if ptr.is_null() {
                return Ok(());
            }

            let bytes = slice::from_raw_parts(ptr, len);
            let s = str::from_utf8_unchecked(bytes);
            self.push_str(s);
            Ok(())
        }
    }
}

/// [`Readable`] implementation which returns a newly allocated [`Vec`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Writable`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<Vec<u8>>(0)?;
///     assert!(matches!(name.as_slice(), b"Alice" | b"Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<Vec::<u8>>(0)?;
///     assert!(matches!(name.as_slice(), b"1" | b"2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Readable for Vec<u8> {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut buf = Vec::new();
        buf.write(stmt, index)?;
        Ok(buf)
    }
}

/// [`Writable`] implementation for [`String`] which appends the content of the
/// column to the current container.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
/// let mut name = Vec::<u8>::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_slice(), b"Alice" | b"Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
/// let mut name = Vec::<u8>::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_slice(), b"1" | b"2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Writable for Vec<u8> {
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        unsafe {
            let i = c_int::try_from(index).unwrap_or(c_int::MAX);

            let Ok(len) = usize::try_from(ffi::sqlite3_column_bytes(stmt.raw.as_ptr(), i)) else {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            };

            if len == 0 {
                return Ok(());
            }

            let ptr = ffi::sqlite3_column_blob(stmt.raw.as_ptr(), i);

            if ptr.is_null() {
                return Ok(());
            }

            let bytes = slice::from_raw_parts(ptr.cast(), len);
            self.extend_from_slice(bytes);
            Ok(())
        }
    }
}

/// A helper to read at most a fixed number of `N` bytes from a column. This
/// allocates the storage for the bytes read on the stack.
pub struct FixedBytes<const N: usize> {
    /// Storage to read to.
    data: [MaybeUninit<u8>; N],
    /// Number of bytes initialized.
    init: usize,
}

impl<const N: usize> FixedBytes<N> {
    /// Coerce into the underlying bytes if all of them have been initialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State, FixedBytes};
    ///
    /// let c = Connection::memory()?;
    /// c.execute(r##"
    /// CREATE TABLE users (id BLOB);
    /// INSERT INTO users (id) VALUES (X'01020304'), (X'05060708');
    /// "##)?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM users")?;
    ///
    /// while let State::Row = stmt.step()? {
    ///     let bytes = stmt.read::<FixedBytes<4>>(0)?;
    ///     assert!(matches!(bytes.into_bytes(), Some([1, 2, 3, 4] | [5, 6, 7, 8])));
    /// }
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    pub fn into_bytes(self) -> Option<[u8; N]> {
        if self.init == N {
            // SAFETY: All of the bytes in the sequence have been initialized
            // and can be safety transmuted.
            //
            // Method of transmuting comes from the implementation of
            // `MaybeUninit::array_assume_init` which is not yet stable.
            unsafe { Some((&self.data as *const _ as *const [u8; N]).read()) }
        } else {
            None
        }
    }

    /// Coerce into the slice of initialized memory which is present.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State, FixedBytes};
    ///
    /// let c = Connection::memory()?;
    /// c.execute(r##"
    /// CREATE TABLE users (id BLOB);
    /// INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
    /// "##)?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM users")?;
    ///
    /// while let State::Row = stmt.step()? {
    ///     let bytes = stmt.read::<FixedBytes<10>>(0)?;
    ///     assert!(matches!(bytes.as_bytes(), &[1, 2, 3, 4] | &[5, 6, 7, 8, 9]));
    /// }
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        if self.init == 0 {
            return &[];
        }

        // SAFETY: We've asserted that `initialized` accounts for the number of
        // bytes that have been initialized.
        unsafe { slice::from_raw_parts(self.data.as_ptr() as *const u8, self.init) }
    }
}

impl<const N: usize> fmt::Debug for FixedBytes<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_bytes().fmt(f)
    }
}

/// [`Readable`] implementation for [`FixedBytes`] which reads at most `N`
/// bytes.
///
/// If the column contains more than `N` bytes, a [`Code::MISMATCH`] error is
/// returned.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State, FixedBytes, Code};
///
/// let c = Connection::memory()?;
/// c.execute(r##"
/// CREATE TABLE users (id BLOB);
/// INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// assert_eq!(stmt.step()?, State::Row);
/// let bytes = stmt.read::<FixedBytes<4>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[1, 2, 3, 4]);
///
/// assert_eq!(stmt.step()?, State::Row);
/// let e = stmt.read::<FixedBytes<4>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.read::<FixedBytes<5>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[5, 6, 7, 8, 9]);
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl<const N: usize> Readable for FixedBytes<N> {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut bytes = FixedBytes {
            // SAFETY: this is safe as per `MaybeUninit::uninit_array`, which isn't stable (yet).
            data: unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() },
            init: 0,
        };

        unsafe {
            let pointer = ffi::sqlite3_column_blob(stmt.raw.as_ptr(), index);

            if pointer.is_null() {
                return Ok(bytes);
            }

            let Ok(len) = usize::try_from(ffi::sqlite3_column_bytes(stmt.raw.as_ptr(), index))
            else {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            };

            if len > N {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            }

            ptr::copy_nonoverlapping(
                pointer as *const u8,
                bytes.data.as_mut_ptr() as *mut u8,
                len,
            );

            bytes.init = len;
            Ok(bytes)
        }
    }
}

impl<T> Readable for Option<T>
where
    T: Readable,
{
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == Type::NULL {
            Ok(None)
        } else {
            T::read(stmt, index).map(Some)
        }
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
