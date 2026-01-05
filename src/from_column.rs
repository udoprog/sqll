use core::ffi::c_int;

use alloc::string::String;
use alloc::vec::Vec;

use crate::ffi;
use crate::{
    CheckBytes, Code, Error, FixedBlob, FixedText, FromUnsizedColumn, Null, Result, Statement,
    Type, Value,
};

/// Type returned when calling [`FromColumn::check`] for a primitive value.
pub struct CheckPrimitive {
    index: c_int,
}

/// A type suitable for reading a single value from a prepared statement.
///
/// Use with [`Statement::get`].
///
/// To read multiple columns simultaneously see [`Row`].
///
/// [`Row`]: crate::Row
///
/// # Safe implementation
///
/// Note that column loading is separated into two stages: checking and loading.
///
/// During checking, we do all the necessary work to ensure that the underlying
/// statement doesn't change when loading which could invalidate any references
/// we take into it. This typically happens during something called
/// auto-conversion which happens for example when the stored data is UTF-16 but
/// we ask for a string which we expect to be UTF-8.
///
/// We provide a safe API for this by ensuring that the underlying type is
/// idempotently converted and strictly type-checked. As in we only permit
/// string to string conversions, but deny integer to string conversions.
pub trait FromColumn<'stmt>
where
    Self: Sized,
{
    /// The prepared check for reading the column.
    type Check;

    /// Perform checks and warm up for the given column.
    ///
    /// Calling this ensures that any conversion performed over the column is
    /// done before we attempt to read it.
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check>;

    /// Read a value from the specified column.
    ///
    /// For custom implementations this typically means accessing the value from
    /// the column using [`Statement::get`].
    ///
    /// # Examples
    ///
    /// ```
    /// use core::ffi::c_int;
    /// use core::fmt;
    ///
    /// use sqll::{Connection, FromColumn, Result, Statement, CheckBytes};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// #[derive(Debug, PartialEq, Eq)]
    /// struct Id(Vec<u8>);
    ///
    /// impl<'stmt> FromColumn<'stmt> for Id {
    ///     type Check = CheckBytes;
    ///
    ///     #[inline]
    ///     fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
    ///         Vec::<u8>::check(stmt, index)
    ///     }
    ///
    ///     #[inline]
    ///     fn load(stmt: &Statement, checked: CheckBytes) -> Result<Self> {
    ///         Ok(Id(Vec::<u8>::load(stmt, checked)?))
    ///     }
    /// }
    ///
    /// c.execute(r#"
    ///     CREATE TABLE ids (id BLOB NOT NULL);
    ///
    ///     INSERT INTO ids (id) VALUES (X'abcdabcd');
    /// "#)?;
    ///
    /// let mut select = c.prepare("SELECT id FROM ids")?;
    /// assert!(select.step()?.is_row());
    ///
    /// assert_eq!(select.get::<Id>(0)?, Id(vec![0xab, 0xcd, 0xab, 0xcd]));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    fn load(stmt: &'stmt Statement, check: Self::Check) -> Result<Self>;
}

/// [`FromColumn`] implementation for [`Null`].
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
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind_value(1, "Alice")?;
///
/// let mut names = Vec::new();
///
/// while let Some(value) = stmt.next::<Null>()? {
///     names.push(value);
/// }
///
/// assert_eq!(names, vec![Null]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for Null {
    type Check = ();

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        type_check(stmt, index, Type::NULL)
    }

    #[inline]
    fn load(_: &Statement, _: Self::Check) -> Result<Self> {
        Ok(Null)
    }
}

/// Type returned when calling [`FromColumn::check`] for a dynamic [`Value`].
pub struct CheckValue {
    kind: CheckValueKind,
}

enum CheckValueKind {
    Blob(CheckBytes),
    Text(CheckBytes),
    Float(CheckPrimitive),
    Integer(CheckPrimitive),
    Null(CheckPrimitive),
}

/// [`FromColumn`] implementation for [`Value`].
impl FromColumn<'_> for Value {
    type Check = CheckValue;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        let kind = match stmt.column_type(index) {
            Type::BLOB => CheckValueKind::Blob(Vec::<u8>::check(stmt, index)?),
            Type::TEXT => CheckValueKind::Text(String::check(stmt, index)?),
            Type::FLOAT => CheckValueKind::Float(f64::check(stmt, index)?),
            Type::INTEGER => CheckValueKind::Integer(i64::check(stmt, index)?),
            Type::NULL => CheckValueKind::Null(CheckPrimitive { index }),
            ty => {
                return Err(Error::new(
                    Code::MISMATCH,
                    format_args!("dynamic value has unsupported column type {ty}"),
                ));
            }
        };

        Ok(CheckValue { kind })
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckValue) -> Result<Self> {
        match check.kind {
            CheckValueKind::Blob(check) => Ok(Value::blob(Vec::<u8>::load(stmt, check)?)),
            CheckValueKind::Text(check) => Ok(Value::text(String::load(stmt, check)?)),
            CheckValueKind::Float(check) => Ok(Value::float(f64::load(stmt, check)?)),
            CheckValueKind::Integer(check) => Ok(Value::integer(i64::load(stmt, check)?)),
            CheckValueKind::Null(CheckPrimitive { .. }) => Ok(Value::null()),
        }
    }
}

/// [`FromColumn`] implementation for [`f64`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(value) = stmt.next::<f64>()? {
///     assert!(matches!(value, 3.14 | 2.71));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<i64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for f64 {
    type Check = CheckPrimitive;

    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self::Check> {
        type_check(stmt, index, Type::FLOAT)?;
        Ok(CheckPrimitive { index })
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckPrimitive) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_double(stmt.as_ptr(), check.index) })
    }
}

/// [`FromColumn`] implementation for [`f32`].
///
/// Getting this type requires conversion and might be subject to precision
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
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(value) = stmt.next::<f32>()? {
///     assert!(matches!(value, 3.14 | 2.71));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<i32>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for f32 {
    type Check = CheckPrimitive;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        f64::check(stmt, index)
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckPrimitive) -> Result<Self> {
        Ok(f64::load(stmt, check)? as f32)
    }
}

/// [`FromColumn`] implementation for `i64`.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value INTEGER);
///
///     INSERT INTO numbers (value) VALUES (3), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(value) = stmt.next::<i64>()? {
///     assert!(matches!(value, 3 | 2));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value INTEGER);
///
///     INSERT INTO numbers (value) VALUES (3), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<f64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for i64 {
    type Check = CheckPrimitive;

    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self::Check> {
        type_check(stmt, index, Type::INTEGER)?;
        Ok(CheckPrimitive { index })
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckPrimitive) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.as_ptr(), check.index) })
    }
}

macro_rules! lossless {
    ($ty:ty) => {
        #[doc = concat!("[`FromColumn`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        #[doc = concat!("while let Some(value) = stmt.next::<", stringify!($ty), ">()? {")]
        ///     assert!(matches!(value, 3 | 2));
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        ///
        /// Automatic conversion being denied:
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while stmt.step()?.is_row() {
        ///     let e = stmt.get::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl FromColumn<'_> for $ty {
            type Check = CheckPrimitive;

            #[inline]
            fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
                i64::check(stmt, index)
            }

            #[inline]
            fn load(stmt: &Statement, check: CheckPrimitive) -> Result<Self> {
                let value = i64::load(stmt, check)?;
                Ok(value as $ty)
            }
        }
    };
}

macro_rules! lossy {
    ($ty:ty, $conversion:literal) => {
        #[doc = concat!("[`FromColumn`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Errors
        ///
        /// Getting this type requires conversion and might fail if the value
        /// cannot be represented by a [`i64`].
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (-9223372036854775808);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// assert!(stmt.step()?.is_row());
        #[doc = concat!("let e = stmt.get::<", stringify!($ty), ">(0).unwrap_err();")]
        /// assert_eq!(e.code(), Code::MISMATCH);
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
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        #[doc = concat!("while let Some(value) = stmt.next::<", stringify!($ty), ">()? {")]
        ///     assert!(matches!(value, 3 | 2));
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        ///
        /// Automatic conversion being denied:
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while stmt.step()?.is_row() {
        ///     let e = stmt.get::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl FromColumn<'_> for $ty {
            type Check = CheckPrimitive;

            #[inline]
            fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
                i64::check(stmt, index)
            }

            #[inline]
            fn load(stmt: &Statement, check: CheckPrimitive) -> Result<Self> {
                let value = i64::load(stmt, check)?;

                let Ok(value) = <$ty>::try_from(value) else {
                    return Err(Error::new(Code::MISMATCH, format_args!($conversion, value)));
                };

                Ok(value)
            }
        }
    };
}

lossy!(i8, "integer {} cannot be converted to i8");
lossy!(i16, "integer {} cannot be converted to i16");
lossy!(i32, "integer {} cannot be converted to i32");
lossy!(u8, "integer {} cannot be converted to u8");
lossy!(u16, "integer {} cannot be converted to u16");
lossy!(u32, "integer {} cannot be converted to u32");
lossy!(u64, "integer {} cannot be converted to u64");
lossy!(u128, "integer {} cannot be converted to u128");
lossless!(i128);

/// [`FromColumn`] implementation which returns a borrowed [`str`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT);
///
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let Some(name) = stmt.next::<String>()? {
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<&str>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for &'stmt str {
    type Check = CheckBytes;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        str::check_unsized(stmt, index)
    }

    #[inline]
    fn load(stmt: &'stmt Statement, check: CheckBytes) -> Result<Self> {
        str::load_unsized(stmt, check)
    }
}

/// [`FromColumn`] implementation which returns a newly allocated [`String`].
///
/// For a more memory-efficient way of reading bytes, consider using the
/// [`FromUnsizedColumn`] implementation for [`str`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT);
///
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let Some(name) = stmt.next::<String>()? {
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<String>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for String {
    type Check = CheckBytes;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        str::check_unsized(stmt, index)
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckBytes) -> Result<Self> {
        let mut s = String::with_capacity(check.len());
        s.push_str(str::load_unsized(stmt, check)?);
        Ok(s)
    }
}

/// [`FromColumn`] implementation which returns a newly allocated [`Vec`].
///
/// For a more memory-efficient way of reading bytes, consider using the
/// [`FromUnsizedColumn`] implementation for a byte slice.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (blob BLOB);
///
///     INSERT INTO users (blob) VALUES (X'aabb'), (X'bbcc');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT blob FROM users")?;
///
/// while let Some(value) = stmt.next::<Vec<u8>>()? {
///     assert!(matches!(value.as_slice(), b"\xaa\xbb" | b"\xbb\xcc"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<Vec::<u8>>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for Vec<u8> {
    type Check = CheckBytes;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        <[u8]>::check_unsized(stmt, index)
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckBytes) -> Result<Self> {
        let mut buf = Vec::with_capacity(check.len());
        buf.extend_from_slice(<[u8]>::load_unsized(stmt, check)?);
        Ok(buf)
    }
}

/// [`FromColumn`] implementation which returns a borrowed `[u8]`.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (blob BLOB);
///
///     INSERT INTO users (blob) VALUES (X'aabb'), (X'bbcc');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT blob FROM users")?;
///
/// while let Some(blob) = stmt.next::<&[u8]>()? {
///     assert!(matches!(blob, b"\xaa\xbb" | b"\xbb\xcc"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<&[u8]>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for &'stmt [u8] {
    type Check = CheckBytes;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        <[u8]>::check_unsized(stmt, index)
    }

    #[inline]
    fn load(stmt: &'stmt Statement, check: CheckBytes) -> Result<Self> {
        FromUnsizedColumn::load_unsized(stmt, check)
    }
}

/// [`FromColumn`] implementation for [`FixedBlob`] which reads at most `N`
/// bytes.
///
/// If the column contains more than `N` bytes, a [`Code::MISMATCH`] error is
/// returned.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FixedBlob, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id BLOB);
///
///     INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// assert!(stmt.step()?.is_row());
/// let bytes = stmt.get::<FixedBlob<4>>(0)?;
/// assert_eq!(bytes.as_slice(), &[1, 2, 3, 4]);
///
/// assert!(stmt.step()?.is_row());
/// let e = stmt.get::<FixedBlob<4>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.get::<FixedBlob<5>>(0)?;
/// assert_eq!(bytes.as_slice(), &[5, 6, 7, 8, 9]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> FromColumn<'_> for FixedBlob<N> {
    type Check = CheckBytes;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        <[u8]>::check_unsized(stmt, index)
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckBytes) -> Result<Self> {
        match FixedBlob::try_from(<[u8]>::load_unsized(stmt, check)?) {
            Ok(bytes) => Ok(bytes),
            Err(err) => Err(Error::new(Code::MISMATCH, err)),
        }
    }
}

/// [`FromColumn`] implementation for [`FixedBlob`] which reads at most `N`
/// bytes.
///
/// If the column contains more than `N` bytes, a [`Code::MISMATCH`] error is
/// returned.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FixedText, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT);
///
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// assert!(stmt.step()?.is_row());
/// let bytes = stmt.get::<FixedText<5>>(0)?;
/// assert_eq!(bytes.as_str(), "Alice");
///
/// assert!(stmt.step()?.is_row());
/// let e = stmt.get::<FixedText<2>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.get::<FixedText<5>>(0)?;
/// assert_eq!(bytes.as_str(), "Bob");
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> FromColumn<'_> for FixedText<N> {
    type Check = CheckBytes;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        str::check_unsized(stmt, index)
    }

    #[inline]
    fn load(stmt: &Statement, check: CheckBytes) -> Result<Self> {
        match FixedText::try_from(str::load_unsized(stmt, check)?) {
            Ok(s) => Ok(s),
            Err(err) => Err(Error::new(Code::MISMATCH, err)),
        }
    }
}

/// [`FromColumn`] implementation for [`Option`].
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
/// let mut stmt = c.prepare("SELECT name, age FROM users")?;
///
/// let mut names_and_ages = Vec::new();
///
/// while let Some((name, age)) = stmt.next::<(String, Option<i64>)>()? {
///     names_and_ages.push((name, age));
/// }
///
/// names_and_ages.sort();
/// assert_eq!(names_and_ages, vec![(String::from("Alice"), None), (String::from("Bob"), Some(30))]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt, T> FromColumn<'stmt> for Option<T>
where
    T: FromColumn<'stmt>,
{
    type Check = Option<T::Check>;

    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self::Check> {
        if stmt.column_type(index) == Type::NULL {
            return Ok(None);
        }

        Ok(Some(T::check(stmt, index)?))
    }

    #[inline]
    fn load(stmt: &'stmt Statement, check: Option<T::Check>) -> Result<Self> {
        match check {
            Some(check) => Ok(Some(T::load(stmt, check)?)),
            None => Ok(None),
        }
    }
}

// NB: We have to perform strict type checking to avoid auto-conversion, if we
// permit it, the pointers that have previously been fetched for a given column
// may become invalidated.
//
// See: https://sqlite.org/c3ref/column_blob.html
#[inline(always)]
pub(crate) fn type_check(stmt: &Statement, index: c_int, expected: Type) -> Result<()> {
    if stmt.column_type(index) != expected {
        return Err(Error::new(
            Code::MISMATCH,
            format_args!(
                "expected column type {expected} but found found {}",
                stmt.column_type(index)
            ),
        ));
    }

    Ok(())
}
