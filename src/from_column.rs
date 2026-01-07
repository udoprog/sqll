use alloc::string::String;
use alloc::vec::Vec;

use crate::ffi;
use crate::ty::{self, AnyKind, NotNull, Type};
use crate::{
    Code, Error, FixedBlob, FixedText, FromUnsizedColumn, Null, Result, Statement, Text, Value,
};

/// A type suitable for reading a single value from a prepared statement.
///
/// This trait can be used directly through [`Statement::column`], to read multiple
/// columns simultaneously see [`Row`].
///
///
/// [`Row`]: crate::Row
///
/// # Safe implementation
///
/// Note that column loading is separated into two stages: checking and loading.
/// By separating reading a column into two stages in the underlying row API we
/// can hopefully load references directly from the database.
///
/// The [`Type`] trait is response for checking, see it for more information.
///
/// # Examples
///
/// It is expected that this trait is implemented for types which can be
/// conveniently read out of a row.
///
/// In order to do so, the first step is to pick the implementation of [`Type`]
/// to associated with the [`Type` associated type]. This determines the
/// underlying database type being loaded.
///
/// An instance of this type is then passed into [`FromColumn::from_column`]
/// allowing the underlying type to be loaded from the statement it is
/// associated with.
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct Timestamp {
///     seconds: i64,
/// }
///
/// impl FromColumn<'_> for Timestamp {
///     type Type = ty::Integer;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Integer) -> Result<Self> {
///         Ok(Timestamp {
///             seconds: i64::from_column(stmt, index)?,
///         })
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (ts INTEGER);
///
///     INSERT INTO test (ts) VALUES (1767675413);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT ts FROM test")?;
///
/// assert_eq!(stmt.next::<Timestamp>()?, Some(Timestamp { seconds: 1767675413 }));
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// [`Type` associated type]: FromColumn::Type
pub trait FromColumn<'stmt>
where
    Self: Sized,
{
    /// The type of a column.
    ///
    /// This must designate one of the database-primitive types as checks, like:
    /// * [`Integer`] or [`Float`].
    /// * [`Blob`] or [`Text`].
    /// * [`Any`] for dynamically typed column.
    /// * [`Nullable<T>`] for nullable types.
    ///
    /// When this value is received in [`from_column`] it can be used to
    /// actually load the a value of the underlying type.
    ///
    /// [`Integer`]: crate::ty::Integer
    /// [`Float`]: crate::ty::Float
    /// [`Blob`]: crate::ty::Blob
    /// [`Text`]: crate::ty::Text
    /// [`Any`]: crate::ty::Any
    /// [`Nullable<T>`]: crate::ty::Nullable
    /// [`from_column`]: FromColumn::from_column
    type Type: Type;

    /// Read a value from the specified column.
    ///
    /// For custom implementations this typically means accessing the value from
    /// the column using [`Statement::column`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FromColumn, Result, Statement};
    /// use sqll::ty;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// #[derive(Debug, PartialEq, Eq)]
    /// struct Id(Vec<u8>);
    ///
    /// impl FromColumn<'_> for Id {
    ///     type Type = ty::Blob;
    ///
    ///     #[inline]
    ///     fn from_column(stmt: &Statement, index: ty::Blob) -> Result<Self> {
    ///         Ok(Id(<_>::from_column(stmt, index)?))
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
    /// assert_eq!(select.column::<Id>(0)?, Id(vec![0xab, 0xcd, 0xab, 0xcd]));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    fn from_column(stmt: &'stmt Statement, index: Self::Type) -> Result<Self>;
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
/// stmt.bind("Alice")?;
///
/// assert_eq!(stmt.iter::<Null>().collect::<Vec<_>>(), [Ok(Null)]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for Null {
    type Type = Null;

    #[inline]
    fn from_column(_: &Statement, _: Self::Type) -> Result<Self> {
        Ok(Null)
    }
}

/// [`FromColumn`] implementation for [`Value`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Value, Result};
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
/// stmt.bind(None::<Value<'_>>)?;
///
/// assert_eq!(stmt.next::<Value<'_>>(), Ok(Some(Value::text("Alice"))));
/// assert_eq!(stmt.next::<Value<'_>>(), Ok(None));
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for Value<'stmt> {
    type Type = ty::Any;

    #[inline]
    fn from_column(stmt: &'stmt Statement, index: ty::Any) -> Result<Self> {
        match index.into_kind() {
            AnyKind::Blob(index) => Ok(Value::blob(<[u8]>::from_unsized_column(stmt, index)?)),
            AnyKind::Text(index) => Ok(Value::text(Text::from_unsized_column(stmt, index)?)),
            AnyKind::Float(index) => Ok(Value::float(f64::from_column(stmt, index)?)),
            AnyKind::Integer(index) => Ok(Value::integer(i64::from_column(stmt, index)?)),
        }
    }
}

/// [`FromColumn`] implementation for [`f64`].
///
/// This corresponds exactly with the internal SQLite [`FLOAT`][value-type] or
/// [`Float`][type] types.
///
/// [value-type]: crate::ValueType::FLOAT
/// [type]: crate::ty::Float
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
///     let e = stmt.column::<i64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for f64 {
    type Type = ty::Float;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Float) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_double(stmt.as_ptr(), index.index) })
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
///     let e = stmt.column::<i32>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for f32 {
    type Type = ty::Float;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Float) -> Result<Self> {
        let value = f64::from_column(stmt, index)?;
        Ok(value as f32)
    }
}

/// [`FromColumn`] implementation for [`i64`].
///
/// This corresponds exactly with the internal SQLite [`INTEGER`][value-type] or
/// [`Integer`][type] types.
///
/// [value-type]: crate::ValueType::INTEGER
/// [type]: crate::ty::Integer
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
///     let e = stmt.column::<f64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for i64 {
    type Type = ty::Integer;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Integer) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.as_ptr(), index.index) })
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
        ///     let e = stmt.column::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl FromColumn<'_> for $ty {
            type Type = ty::Integer;

            #[inline]
            fn from_column(stmt: &Statement, index: ty::Integer) -> Result<Self> {
                let value = i64::from_column(stmt, index)?;
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
        #[doc = concat!("let e = stmt.column::<", stringify!($ty), ">(0).unwrap_err();")]
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
        ///     let e = stmt.column::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl FromColumn<'_> for $ty {
            type Type = ty::Integer;

            #[inline]
            fn from_column(stmt: &Statement, index: ty::Integer) -> Result<Self> {
                let value = i64::from_column(stmt, index)?;

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

/// [`FromColumn`] implementation which returns a borrowed [`Text`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Text};
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
/// assert_eq!(stmt.next::<&Text>()?, Some(Text::new(b"Alice")));
/// assert_eq!(stmt.next::<&Text>()?, Some(Text::new(b"Bob")));
/// assert_eq!(stmt.next::<&Text>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code, Text};
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
/// let e = stmt.next::<&Text>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for &'stmt Text {
    type Type = ty::Text;

    #[inline]
    fn from_column(stmt: &'stmt Statement, index: ty::Text) -> Result<Self> {
        <_>::from_unsized_column(stmt, index)
    }
}

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
/// assert_eq!(stmt.next::<&str>()?, Some("Alice"));
/// assert_eq!(stmt.next::<&str>()?, Some("Bob"));
/// assert_eq!(stmt.next::<&str>()?, None);
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
/// let e = stmt.next::<&str>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for &'stmt str {
    type Type = ty::Text;

    #[inline]
    fn from_column(stmt: &'stmt Statement, index: ty::Text) -> Result<Self> {
        <_>::from_unsized_column(stmt, index)
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
/// assert_eq!(stmt.next::<String>()?, Some(String::from("Alice")));
/// assert_eq!(stmt.next::<String>()?, Some(String::from("Bob")));
/// assert_eq!(stmt.next::<String>()?, None);
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
/// let e = stmt.next::<String>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for String {
    type Type = ty::Text;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Text) -> Result<Self> {
        let mut s = String::with_capacity(index.len());
        s.push_str(<_>::from_unsized_column(stmt, index)?);
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
/// assert_eq!(stmt.next::<Vec<u8>>()?, Some(vec![0xaa, 0xbb]));
/// assert_eq!(stmt.next::<Vec<u8>>()?, Some(vec![0xbb, 0xcc]));
/// assert_eq!(stmt.next::<Vec<u8>>()?, None);
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
/// let e = stmt.next::<Vec::<u8>>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for Vec<u8> {
    type Type = ty::Blob;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Blob) -> Result<Self> {
        let mut buf = Vec::with_capacity(index.len());
        buf.extend_from_slice(<_>::from_unsized_column(stmt, index)?);
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
///     let e = stmt.column::<&[u8]>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for &'stmt [u8] {
    type Type = ty::Blob;

    #[inline]
    fn from_column(stmt: &'stmt Statement, index: ty::Blob) -> Result<Self> {
        <_>::from_unsized_column(stmt, index)
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
/// let bytes = stmt.column::<FixedBlob<4>>(0)?;
/// assert_eq!(bytes.as_slice(), &[1, 2, 3, 4]);
///
/// assert!(stmt.step()?.is_row());
/// let e = stmt.column::<FixedBlob<4>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.column::<FixedBlob<5>>(0)?;
/// assert_eq!(bytes.as_slice(), &[5, 6, 7, 8, 9]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> FromColumn<'_> for FixedBlob<N> {
    type Type = ty::Blob;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Blob) -> Result<Self> {
        match FixedBlob::try_from(<[u8]>::from_unsized_column(stmt, index)?) {
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
/// let bytes = stmt.column::<FixedText<5>>(0)?;
/// assert_eq!(bytes.as_text(), "Alice");
///
/// assert!(stmt.step()?.is_row());
/// let e = stmt.column::<FixedText<2>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.column::<FixedText<5>>(0)?;
/// assert_eq!(bytes.as_text(), "Bob");
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> FromColumn<'_> for FixedText<N> {
    type Type = ty::Text;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Text) -> Result<Self> {
        match FixedText::try_from(str::from_unsized_column(stmt, index)?) {
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
/// stmt.execute(("Alice", None::<i64>))?;
/// stmt.execute(("Bob", Some(30i64)))?;
///
/// let mut stmt = c.prepare("SELECT name, age FROM users")?;
///
/// let mut names_and_ages = Vec::new();
///
/// while let Some(row) = stmt.next::<(String, Option<i64>)>()? {
///     names_and_ages.push(row);
/// }
///
/// names_and_ages.sort();
/// assert_eq!(names_and_ages, vec![(String::from("Alice"), None), (String::from("Bob"), Some(30))]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt, T> FromColumn<'stmt> for Option<T>
where
    T: FromColumn<'stmt, Type: NotNull>,
{
    type Type = ty::Nullable<T::Type>;

    #[inline]
    fn from_column(stmt: &'stmt Statement, index: ty::Nullable<T::Type>) -> Result<Self> {
        match index.inner {
            Some(index) => Ok(Some(T::from_column(stmt, index)?)),
            None => Ok(None),
        }
    }
}
