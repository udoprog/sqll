use core::ffi::c_int;

use alloc::string::String;
use alloc::vec::Vec;

use crate::bytes;
use crate::ffi;
use crate::utils::sqlite3_try;
use crate::value::Kind;
use crate::{Code, Error, FixedBlob, FixedText, Null, Result, Statement, Value};

/// A type suitable for binding to a prepared statement.
///
/// Use with [`Statement::bind_value`] or [`Statement::bind_value_by_name`].
pub trait BindValue {
    /// Bind a value.
    ///
    /// For custom implementations this typically means forwarding a binding
    /// using an existing implementation and [`Statement::bind_value`].
    ///
    /// # Examples
    ///
    /// ```
    /// use core::ffi::c_int;
    ///
    /// use sqll::{BindValue, Connection, Result, Statement};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// struct Id([u8; 8]);
    ///
    /// impl BindValue for Id {
    ///     #[inline]
    ///     fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
    ///         self.0.as_slice().bind_value(stmt, index)
    ///     }
    /// }
    ///
    /// c.execute(r#"
    ///     CREATE TABLE ids (id BLOB NOT NULL);
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("INSERT INTO ids (id) VALUES (?)")?;
    ///
    /// stmt.execute(Id(*b"abcdabcd"))?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()>;
}

impl<T> BindValue for &T
where
    T: ?Sized + BindValue,
{
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        (**self).bind_value(stmt, index)
    }
}

/// [`BindValue`] implementation for [`Null`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Null};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
/// stmt.bind_value(1, Null)?;
///
/// let mut names = Vec::new();
///
/// while let Some(name) = stmt.next::<String>()? {
///     names.push(name);
/// }
///
/// assert_eq!(names, vec![String::from("Alice")]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for Null {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                stmt, ffi::sqlite3_bind_null(stmt.as_ptr_mut(), index)
            };
        }

        Ok(())
    }
}

/// [`BindValue`] implementation for a dynamic [`Value`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Value};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
/// stmt.bind_value(1, Value::null())?;
///
/// let mut names = Vec::new();
///
/// while let Some(name) = stmt.next::<String>()? {
///     names.push(name);
/// }
///
/// assert_eq!(names, vec![String::from("Alice")]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for Value {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        match &self.kind {
            Kind::Blob(value) => value.as_slice().bind_value(stmt, index),
            Kind::Float(value) => value.bind_value(stmt, index),
            Kind::Integer(value) => value.bind_value(stmt, index),
            Kind::Text(value) => value.as_str().bind_value(stmt, index),
            Kind::Null => Null.bind_value(stmt, index),
        }
    }
}

/// [`BindValue`] implementation for byte slices.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
/// stmt.bind_value(1, &b"Hello"[..])?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(1));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for [u8] {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        let (ptr, len, dealloc) = bytes::alloc(self)?;

        unsafe {
            sqlite3_try! {
                stmt,
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

/// [`BindValue`] implementation for a [`Vec<u8>`] byte array.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
/// stmt.bind_value(1, vec![b'H', b'e', b'l', b'l', b'o'])?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(1));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for Vec<u8> {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        self.as_slice().bind_value(stmt, index)
    }
}

/// [`BindValue`] implementation for [`FixedBlob`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FixedBlob};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
///
/// let data = FixedBlob::from(b"Hello");
/// stmt.bind_value(1, data)?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(1));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> BindValue for FixedBlob<N> {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        self.as_slice().bind_value(stmt, index)
    }
}

/// [`BindValue`] implementation for byte arrays.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
/// stmt.bind_value(1, b"Hello")?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(1));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> BindValue for [u8; N] {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        self.as_slice().bind_value(stmt, index)
    }
}

/// [`BindValue`] implementation for [`f64`].
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
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE measurements (value REAL);
///
///     INSERT INTO measurements (value) VALUES (3.14), (2.71), (1.61);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind_value(1, 2.0f64)?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(2));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for f64 {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                stmt,
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

/// [`BindValue`] implementation for [`f32`].
///
/// Binding this type requires conversion and might be subject to precision
/// loss. To avoid this, consider using [`f64`] instead.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE measurements (value REAL);
///
///     INSERT INTO measurements (value) VALUES (3.14), (2.71), (1.61);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind_value(1, 2.0f32)?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(2));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for f32 {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                stmt,
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

/// [`BindValue`] implementation for [`i64`].
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
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE measurements (value INTEGER);
///
///     INSERT INTO measurements (value) VALUES (3), (2), (1);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
/// stmt.bind_value(1, 2i64)?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(1));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for i64 {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                stmt,
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
        #[doc = concat!("[`BindValue`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE measurements (value INTEGER);
        ///
        ///     INSERT INTO measurements (value) VALUES (3), (2), (1);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
        #[doc = concat!("stmt.bind_value(1, 2", stringify!($ty), ")?;")]
        ///
        /// assert!(stmt.step()?.is_row());
        /// assert_eq!(stmt.get::<i64>(0)?, 1);
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl BindValue for $ty {
            #[inline]
            fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
                let value = *self as i64;
                value.bind_value(stmt, index)
            }
        }
    };
}

macro_rules! lossy {
    ($ty:ty, $conversion:literal) => {
        #[doc = concat!("[`BindValue`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Errors
        ///
        /// Binding this type requires conversion and might fail if the value
        /// cannot be represented by a [`i64`].
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE measurements (value INTEGER);
        ///
        ///     INSERT INTO measurements (value) VALUES (3), (2), (1);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
        #[doc = concat!("let e = stmt.bind_value(1, ", stringify!($ty), "::MAX).unwrap_err();")]
        /// assert_eq!(e.code(), sqll::Code::MISMATCH);
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
        ///     CREATE TABLE measurements (value INTEGER);
        ///
        ///     INSERT INTO measurements (value) VALUES (3), (2), (1);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT COUNT(*) FROM measurements WHERE value > ?")?;
        #[doc = concat!("stmt.bind_value(1, 2", stringify!($ty), ")?;")]
        ///
        /// assert!(stmt.step()?.is_row());
        /// assert_eq!(stmt.get::<i64>(0)?, 1);
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl BindValue for $ty {
            #[inline]
            fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
                let Ok(value) = i64::try_from(*self) else {
                    return Err(Error::new(Code::MISMATCH, format_args!($conversion, *self)));
                };

                value.bind_value(stmt, index)
            }
        }
    };
}

lossless!(i8);
lossless!(i16);
lossless!(i32);
lossy!(i128, "value {} cannot be converted to sqlite integer");
lossless!(u8);
lossless!(u16);
lossless!(u32);
lossy!(u64, "value {} cannot be converted to sqlite integer");
lossy!(u128, "value {} cannot be converted to sqlite integer");

/// [`BindValue`] implementation for [`str`] slices.
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
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind_value(1, "Alice")?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(42));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for str {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        let (data, len, dealloc) = bytes::alloc(self.as_bytes())?;

        unsafe {
            sqlite3_try! {
                stmt,
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

/// [`BindValue`] implementation for a [`String`].
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
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind_value(1, String::from("Alice"))?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(42));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for String {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        self.as_str().bind_value(stmt, index)
    }
}

/// [`BindValue`] implementation for [`FixedText`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FixedText};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
///
/// let name = FixedText::<5>::try_from("Alice")?;
/// stmt.bind_value(1, name)?;
///
/// assert_eq!(stmt.next::<i64>()?, Some(42));
/// assert_eq!(stmt.next::<i64>()?, None);
/// # Ok::<_, Box<dyn std::error::Error>>(())
/// ```
impl<const N: usize> BindValue for FixedText<N> {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        self.as_str().bind_value(stmt, index)
    }
}

/// [`BindValue`] implementation for [`Option`].
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
/// let mut stmt = c.prepare("INSERT INTO users (name, age) VALUES (?, ?)")?;
///
/// stmt.reset()?;
/// stmt.bind_value(1, "Alice")?;
/// stmt.bind_value(2, None::<i64>)?;
/// assert!(stmt.step()?.is_done());
///
/// stmt.reset()?;
/// stmt.bind_value(1, "Bob")?;
/// stmt.bind_value(2, Some(30i64))?;
/// assert!(stmt.step()?.is_done());
///
/// let mut it = c.prepare("SELECT name, age FROM users")?.into_iter::<(String, Option<i64>)>();
///
/// assert_eq!(it.next(), Some(Ok((String::from("Alice"), None))));
/// assert_eq!(it.next(), Some(Ok((String::from("Bob"), Some(30)))));
/// assert_eq!(it.next(), None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<T> BindValue for Option<T>
where
    T: BindValue,
{
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: c_int) -> Result<()> {
        match self {
            Some(inner) => inner.bind_value(stmt, index),
            None => Null.bind_value(stmt, index),
        }
    }
}
