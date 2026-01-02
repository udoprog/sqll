use core::ffi::c_int;

use crate::bytes;
use crate::ffi;
use crate::utils::sqlite3_try;
use crate::value::Kind;
use crate::{Code, Error, Null, Result, Statement, Value};

mod sealed {
    use crate::{Null, Value};

    pub trait Sealed {}
    impl Sealed for str {}
    impl Sealed for [u8] {}
    impl<const N: usize> Sealed for [u8; N] {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
    impl Sealed for i8 {}
    impl Sealed for i16 {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
    impl Sealed for i128 {}
    impl Sealed for u8 {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
    impl Sealed for u64 {}
    impl Sealed for u128 {}
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
/// use sqll::{Connection, Null};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
/// stmt.bind(1, Null)?;
///
/// let mut names = Vec::new();
///
/// while let Some(row) = stmt.next()? {
///     names.push(row.get::<String>(0)?);
/// }
///
/// assert_eq!(names, vec![String::from("Alice")]);
/// # Ok::<_, sqll::Error>(())
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
/// use sqll::{Connection, Value};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
/// stmt.bind(1, Value::null())?;
///
/// let mut names = Vec::new();
///
/// while let Some(row) = stmt.next()? {
///     names.push(row.get::<String>(0)?);
/// }
///
/// assert_eq!(names, vec![String::from("Alice")]);
/// # Ok::<_, sqll::Error>(())
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
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
/// stmt.bind(1, &b"Hello"[..])?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.get::<i64>(0)?, 1);
/// }
/// # Ok::<_, sqll::Error>(())
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
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
/// stmt.bind(1, b"Hello")?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.get::<i64>(0)?, 1);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> Bindable for [u8; N] {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        self.as_slice().bind(stmt, index)
    }
}

/// [`Bindable`] implementation for [`f64`].
///
/// This corresponds to the internal SQLite [`FLOAT`] type.
///
/// [`FLOAT`]: crate::Type::FLOAT
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE measurements (value REAL);
///
///     INSERT INTO measurements (value) VALUES (3.14), (2.71), (1.61);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind(1, 2.0f64)?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.get::<i64>(0)?, 2);
/// }
/// # Ok::<_, sqll::Error>(())
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

/// [`Bindable`] implementation for [`f32`].
///
/// Binding this type requires conversion and might be subject to precision
/// loss. To avoid this, consider using [`f64`] instead.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE measurements (value REAL);
///
///     INSERT INTO measurements (value) VALUES (3.14), (2.71), (1.61);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind(1, 2.0f32)?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.get::<i64>(0)?, 2);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Bindable for f32 {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_double(
                    stmt.as_ptr_mut(),
                    index,
                    *self as f64
                )
            };
        }

        Ok(())
    }
}

/// [`Bindable`] implementation for [`i64`].
///
/// This corresponds to the internal SQLite [`INTEGER`] type and can therefore
/// represent any value.
///
/// [`INTEGER`]: crate::Type::INTEGER
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE measurements (value INTEGER);
///
///     INSERT INTO measurements (value) VALUES (3), (2), (1);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind(1, 2i64)?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.get::<i64>(0)?, 1);
/// }
/// # Ok::<_, sqll::Error>(())
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

macro_rules! lossless {
    ($ty:ty) => {
        #[doc = concat!(" [`Bindable`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE measurements (value INTEGER);
        ///
        ///     INSERT INTO measurements (value) VALUES (3), (2), (1);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
        #[doc = concat!(" stmt.bind(1, 2", stringify!($ty), ")?;")]
        ///
        /// assert!(stmt.step()?.is_row());
        /// assert_eq!(stmt.get::<i64>(0)?, 1);
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl Bindable for $ty {
            #[inline]
            fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
                let value = *self as i64;
                value.bind(stmt, index)
            }
        }
    };
}

macro_rules! lossy {
    ($ty:ty) => {
        #[doc = concat!(" [`Bindable`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Errors
        ///
        /// Binding this type requires conversion and might fail if the value
        /// cannot be represented by a [`i64`].
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE measurements (value INTEGER);
        ///
        ///     INSERT INTO measurements (value) VALUES (3), (2), (1);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
        #[doc = concat!(" let e = stmt.bind(1, ", stringify!($ty), "::MAX).unwrap_err();")]
        /// assert_eq!(e.code(), sqll::Code::MISMATCH);
        /// # Ok::<_, sqll::Error>(())
        /// ```
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE measurements (value INTEGER);
        ///
        ///     INSERT INTO measurements (value) VALUES (3), (2), (1);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
        #[doc = concat!(" stmt.bind(1, 2", stringify!($ty), ")?;")]
        ///
        /// assert!(stmt.step()?.is_row());
        /// assert_eq!(stmt.get::<i64>(0)?, 1);
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl Bindable for $ty {
            #[inline]
            fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
                let Ok(value) = i64::try_from(*self) else {
                    return Err(Error::new(Code::MISMATCH));
                };

                value.bind(stmt, index)
            }
        }
    };
}

lossless!(i8);
lossless!(i16);
lossless!(i32);
lossy!(i128);
lossless!(u8);
lossless!(u16);
lossless!(u32);
lossy!(u64);
lossy!(u128);

/// [`Bindable`] implementation for [`str`] slices.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind(1, "Alice")?;
///
/// while let Some(row) = stmt.next()? {
///     assert_eq!(row.get::<i64>(0)?, 42);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Bindable for str {
    #[inline]
    fn bind(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        let (data, len, dealloc) = bytes::alloc(self.as_bytes())?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_bind_text(
                    stmt.as_ptr_mut(),
                    index,
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
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
/// "#)?;
///
/// let mut stmt = c.prepare("INSERT INTO users (name, age) VALUES (?, ?)")?;
///
/// stmt.reset()?;
/// stmt.bind(1, "Alice")?;
/// stmt.bind(2, None::<i64>)?;
/// assert!(stmt.step()?.is_done());
///
/// stmt.reset()?;
/// stmt.bind(1, "Bob")?;
/// stmt.bind(2, Some(30i64))?;
/// assert!(stmt.step()?.is_done());
///
/// let mut stmt = c.prepare("SELECT name, age FROM users")?;
///
/// let mut names_and_ages = Vec::new();
///
/// while let Some(row) = stmt.next()? {
///     let name = row.get::<String>(0)?;
///     let age = row.get::<Option<i64>>(1)?;
///     names_and_ages.push((name, age));
/// }
///
/// names_and_ages.sort();
/// assert_eq!(names_and_ages, vec![(String::from("Alice"), None), (String::from("Bob"), Some(30))]);
/// # Ok::<_, sqll::Error>(())
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
