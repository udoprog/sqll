use core::ffi::c_int;

use sqlite3_sys as ffi;

use crate::bytes;
use crate::error::Result;
use crate::utils::sqlite3_try;
use crate::value::Kind;
use crate::{Null, Statement, Value};

mod sealed {
    use crate::{Null, Value};

    pub trait Sealed {}
    impl Sealed for str {}
    impl Sealed for [u8] {}
    impl<const N: usize> Sealed for [u8; N] {}
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
    Self: self::sealed::Sealed,
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

/// [`Bindable`] implementation for [`Null`].
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, Null};
///
/// let c = Connection::memory()?;
/// c.execute(r##"
/// CREATE TABLE users (name TEXT, age INTEGER);
/// INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
/// stmt.bind(1, Null)?;
///
/// let mut names = Vec::new();
///
/// while let Some(row) = stmt.next()? {
///     names.push(row.read::<String>(0)?);
/// }
///
/// assert_eq!(names, vec![String::from("Alice")]);
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Bindable for Null {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_null(stmt.as_ptr_mut(), index)
            };
        }

        Ok(())
    }
}

/// [`Bindable`] implementation for a dynamic [`Value`].
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, Value};
///
/// let c = Connection::memory()?;
/// c.execute(r##"
/// CREATE TABLE users (name TEXT, age INTEGER);
/// INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
/// stmt.bind(1, Value::null())?;
///
/// let mut names = Vec::new();
///
/// while let Some(row) = stmt.next()? {
///     names.push(row.read::<String>(0)?);
/// }
///
/// assert_eq!(names, vec![String::from("Alice")]);
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Bindable for Value {
    #[inline]
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

/// [`Bindable`] implementation for byte slices.
///
/// # Examples
///
/// ```
/// use sqlite_ll::Connection;
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE files (id INTEGER, data BLOB);
/// INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
/// INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
/// stmt.bind(1, &b"Hello"[..])?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.read::<i64>(0)?, 1);
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Bindable for [u8] {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        let (ptr, len, dealloc) = bytes::alloc(self)?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_blob(
                    stmt.as_ptr_mut(),
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

/// [`Bindable`] implementation for byte arrays.
///
/// # Examples
///
/// ```
/// use sqlite_ll::Connection;
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE files (id INTEGER, data BLOB);
/// INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
/// INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
/// stmt.bind(1, b"Hello")?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.read::<i64>(0)?, 1);
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl<const N: usize> Bindable for [u8; N] {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        self.as_slice().bind(stmt, index)
    }
}

/// [`Bindable`] implementation for [`f64`].
///
/// # Examples
///
/// ```
/// use sqlite_ll::Connection;
///
/// let c = Connection::memory()?;
///
/// c.execute(r#"
/// CREATE TABLE measurements (value REAL);
/// INSERT INTO measurements (value) VALUES (3.14), (2.71), (1.61);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind(1, 2.0f64)?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.read::<i64>(0)?, 2);
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Bindable for f64 {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_double(
                    stmt.as_ptr_mut(),
                    index,
                    *self
                )
            };
        }

        Ok(())
    }
}

/// [`Bindable`] implementation for [`i64`].
///
/// # Examples
///
/// ```
/// use sqlite_ll::Connection;
///
/// let c = Connection::memory()?;
///
/// c.execute(r#"
/// CREATE TABLE measurements (value INTEGER);
/// INSERT INTO measurements (value) VALUES (3), (2), (1);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind(1, 2)?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.read::<i64>(0)?, 1);
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Bindable for i64 {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_int64(
                    stmt.as_ptr_mut(),
                    index,
                    *self as ffi::sqlite3_int64
                )
            };
        }

        Ok(())
    }
}

/// [`Bindable`] implementation for [`str`] slices.
///
/// # Examples
///
/// ```
/// use sqlite_ll::Connection;
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT, age INTEGER);
/// INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind(1, "Alice")?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.read::<i64>(0)?, 42);
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Bindable for str {
    #[inline]
    fn bind(&self, stmt: &mut Statement, i: c_int) -> Result<()> {
        let (data, len, dealloc) = bytes::alloc(self.as_bytes())?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_text(
                    stmt.as_ptr_mut(),
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

/// [`Bindable`] implementation for [`Option`].
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
/// c.execute(r##"
/// CREATE TABLE users (name TEXT, age INTEGER);
/// "##)?;
///
/// let mut stmt = c.prepare("INSERT INTO users (name, age) VALUES (?, ?)")?;
///
/// stmt.reset()?;
/// stmt.bind(1, "Alice")?;
/// stmt.bind(2, None::<i64>)?;
/// assert_eq!(stmt.step()?, State::Done);
///
/// stmt.reset()?;
/// stmt.bind(1, "Bob")?;
/// stmt.bind(2, Some(30i64))?;
/// assert_eq!(stmt.step()?, State::Done);
///
/// let mut stmt = c.prepare("SELECT name, age FROM users")?;
///
/// let mut names_and_ages = Vec::new();
///
/// while let State::Row = stmt.step()? {
///     let name: String = stmt.read(0)?;
///     let age: Option<i64> = stmt.read(1)?;
///     names_and_ages.push((name, age));
/// }
///
/// names_and_ages.sort();
/// assert_eq!(names_and_ages, vec![(String::from("Alice"), None), (String::from("Bob"), Some(30))]);
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
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
