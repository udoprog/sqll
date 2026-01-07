use core::ffi::c_int;

use crate::ffi;
use crate::{Code, Error, Null, Result, Statement, ValueType};

use super::NotNull;

mod sealed {
    use super::{Any, Blob, Float, Integer, NotNull, Null, Nullable, Text};

    pub trait Sealed
    where
        Self: Sized,
    {
    }

    impl Sealed for Any {}
    impl Sealed for Null {}
    impl Sealed for Float {}
    impl Sealed for Integer {}
    impl Sealed for Blob {}
    impl Sealed for Text {}
    impl<T> Sealed for Nullable<T> where T: NotNull {}
}

/// A trait which defines the underlying static value type that is supported by
/// a value that implements [`FromColumn`] or [`FromUnsizedColumn`].
///
/// This type exists in contrast to [`ValueType`], which is a runtime value
/// defining a particular value.
///
/// One thing worth noting about SQLite is that tables are dynamically typed.
/// Any column of any type can contain any value. If there is a discrepancy
/// during loading, a process known as auto-conversion will be attempted. This
/// however can cause problems, since pointers, which are subseqeuently used to
/// construct references in Rust may be invalidated.
///
/// We carefully provide an API to ensure that references loaded from sqlite
/// remain valid. This type is a key component to that. We break loading up into
/// two steps on of them being checking which is done through [`check`].
///
/// We ensure that the type conversion is idempotent by sealing this unsafe
/// trait and requiring it to be used when loading a column of a particular
/// type. This way, we can hopefully ensures that pointers remain valid for the
/// lifetime of the column being loaded.
///
/// [`FromColumn`]: crate::FromColumn
/// [`FromUnsizedColumn`]: crate::FromUnsizedColumn
/// [`check`]: Self::check
///
/// # Safety
///
/// Implementors must ensure that the type check exercises the underlying
/// statement in a manner which ensures that the values which will later be
/// loaded are cached and won't change.
pub unsafe trait Type
where
    Self: self::sealed::Sealed + Sized,
{
    /// Perform checks and warm up for the given column ensuring that any
    /// auto-conversion that needs to occur to load the field is done.
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self>;
}

/// [`Type`] implementation for any non-null value.
///
/// To make a type nullable, wrap it in [`Nullable`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Value};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq)]
/// struct MyValue<'stmt>(Value<'stmt>);
///
/// impl<'stmt> FromColumn<'stmt> for MyValue<'stmt> {
///     type Type = ty::Any;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: ty::Any) -> Result<Self> {
///         Ok(MyValue(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value);
///
///     INSERT INTO test (value) VALUES ('Hello, world!'), (42), (3.14), (X'DEADBEEF');
/// "#)?;
///
/// let mut select = c.prepare("SELECT value FROM test")?;
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::text("Hello, world!"))));
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::integer(42))));
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::float(3.14))));
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::blob(&[0xDE, 0xAD, 0xBE, 0xEF]))));
/// assert_eq!(select.next::<MyValue<'_>>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Any {
    kind: AnyKind,
}

impl Any {
    /// Returns the underlying kind of the dynamic column.
    #[inline]
    pub(crate) const fn into_kind(self) -> AnyKind {
        self.kind
    }
}

pub(crate) enum AnyKind {
    Blob(Blob),
    Text(Text),
    Float(Float),
    Integer(Integer),
}

/// [`Type`] implementation for any non-null value.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Value};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq)]
/// struct MyValue<'stmt>(Value<'stmt>);
///
/// impl<'stmt> FromColumn<'stmt> for MyValue<'stmt> {
///     type Type = ty::Any;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: ty::Any) -> Result<Self> {
///         Ok(MyValue(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value);
///
///     INSERT INTO test (value) VALUES ('Hello, world!'), (42), (3.14), (X'DEADBEEF');
/// "#)?;
///
/// let mut select = c.prepare("SELECT value FROM test")?;
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::text("Hello, world!"))));
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::integer(42))));
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::float(3.14))));
/// assert_eq!(select.next::<MyValue<'_>>()?, Some(MyValue(Value::blob(&[0xDE, 0xAD, 0xBE, 0xEF]))));
/// assert_eq!(select.next::<MyValue<'_>>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
unsafe impl Type for Any {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        let kind = match stmt.column_type(index) {
            ValueType::BLOB => AnyKind::Blob(Blob::check(stmt, index)?),
            ValueType::TEXT => AnyKind::Text(Text::check(stmt, index)?),
            ValueType::FLOAT => AnyKind::Float(Float::check(stmt, index)?),
            ValueType::INTEGER => AnyKind::Integer(Integer::check(stmt, index)?),
            ty => {
                return Err(Error::new(
                    Code::MISMATCH,
                    format_args!("dynamic value has unsupported column type {ty}"),
                ));
            }
        };

        Ok(Any { kind })
    }
}

/// [`Type`] implementation for [`Null`].
///
/// This must be used when implementing custom types that can be read from
/// column and a [`Null`] value is expected.
///
/// # Examples
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
unsafe impl Type for Null {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, ValueType::NULL)?;
        Ok(Self)
    }
}

/// [`Type`] implementation for an integer column.
///
/// This is represented in rust by the [`i64`] value and corresponds to the
/// [`INTEGER`] runtime-time type.
///
/// This must be used when implementing custom types that can be read from
/// column and a [`i64`] value is expected.
///
/// This type is [`NotNull`], use [`Nullable<Float>`] to make it nullable.
///
/// [`INTEGER`]: crate::ValueType::INTEGER
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyInteger(i64);
///
/// impl FromColumn<'_> for MyInteger {
///     type Type = ty::Integer;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Integer) -> Result<Self> {
///         Ok(MyInteger(i64::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value INTEGER);
///
///     INSERT INTO test (value) VALUES (42);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyInteger>()?, Some(MyInteger(42)));
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Integer {
    pub(crate) index: c_int,
}

/// [`Type`] implementation for [`i64`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyInteger(i64);
///
/// impl FromColumn<'_> for MyInteger {
///     type Type = ty::Integer;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Integer) -> Result<Self> {
///         Ok(MyInteger(i64::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value INTEGER);
///
///     INSERT INTO test (value) VALUES (42);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyInteger>()?, Some(MyInteger(42)));
/// # Ok::<_, sqll::Error>(())
/// ```
unsafe impl Type for Integer {
    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, ValueType::INTEGER)?;
        Ok(Self { index })
    }
}

/// [`Type`] implementation for a float column.
///
/// This is represented in rust by the [`f64`] value and corresponds to the
/// [`FLOAT`] runtime-time type.
///
/// This must be used when implementing custom types that can be read from
/// column and a [`f64`] value is expected.
///
/// This type is [`NotNull`], use [`Nullable<Float>`] to make it nullable.
///
/// [`FLOAT`]: crate::ValueType::FLOAT
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// struct MyFloat(f64);
///
/// impl FromColumn<'_> for MyFloat {
///     type Type = ty::Float;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Float) -> Result<Self> {
///         Ok(MyFloat(f64::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value FLOAT);
///
///     INSERT INTO test (value) VALUES (4.42);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert!(matches!(stmt.next::<MyFloat>()?, Some(MyFloat(4.4..4.5))));
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Float {
    pub(crate) index: c_int,
}

/// [`Type`] implementation for float.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// struct MyFloat(f64);
///
/// impl FromColumn<'_> for MyFloat {
///     type Type = ty::Float;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Float) -> Result<Self> {
///         Ok(MyFloat(f64::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value FLOAT);
///
///     INSERT INTO test (value) VALUES (4.42);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert!(matches!(stmt.next::<MyFloat>()?, Some(MyFloat(4.4..4.5))));
/// # Ok::<_, sqll::Error>(())
/// ```
unsafe impl Type for Float {
    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, ValueType::FLOAT)?;
        Ok(Self { index })
    }
}

/// [`Type`] implementation for a text column.
///
/// This is represented in rust by the [`Text`] value and corresponds to the
/// [`TEXT`] runtime-time type.
///
/// This type is [`NotNull`], use [`Nullable<Text>`] to make it nullable.
///
/// [`TEXT`]: crate::ValueType::TEXT
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyString<'stmt>(&'stmt str);
///
/// impl<'stmt> FromColumn<'stmt> for MyString<'stmt> {
///     type Type = ty::Text;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: ty::Text) -> Result<Self> {
///         Ok(MyString(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value TEXT);
///
///     INSERT INTO test (value) VALUES ('Hello, world!');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyString>()?, Some(MyString("Hello, world!")));
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Text {
    index: c_int,
    len: usize,
}

impl Text {
    /// Returns the length in bytes text column.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns if the text column is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the column index.
    #[inline]
    pub(crate) fn column(&self) -> c_int {
        self.index
    }
}

/// [`Type`] implementation for a text column.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyString<'stmt>(&'stmt str);
///
/// impl<'stmt> FromColumn<'stmt> for MyString<'stmt> {
///     type Type = ty::Text;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: ty::Text) -> Result<Self> {
///         Ok(MyString(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value TEXT);
///
///     INSERT INTO test (value) VALUES ('Hello, world!');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyString>()?, Some(MyString("Hello, world!")));
/// # Ok::<_, sqll::Error>(())
/// ```
unsafe impl Type for Text {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        unsafe {
            // Note that this type check is important, because it locks the type
            // of conversion we permit for a string column.
            type_check(stmt, index, ValueType::TEXT)?;

            // NB: This will force an internal conversion to UTF-8 if the column
            // is stored in UTF-16.
            let len = ffi::sqlite3_column_bytes(stmt.as_ptr(), index);

            // This is unlikely to not be optimized out, but for the off chance
            // we still keep it.
            let Ok(len) = usize::try_from(len) else {
                return Err(Error::new(
                    Code::ERROR,
                    format_args!("column size {len} exceeds addressable memory"),
                ));
            };

            Ok(Self { index, len })
        }
    }
}

/// [`Type`] implementation for a blob.
///
/// This is represented in rust by the `[u8]` slice and corresponds to the
/// [`BLOB`] runtime-time type.
///
/// This type is [`NotNull`], use [`Nullable<Blob>`] to make it nullable.
///
/// [`BLOB`]: crate::ValueType::BLOB
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyBytes<'stmt>(&'stmt [u8]);
///
/// impl<'stmt> FromColumn<'stmt> for MyBytes<'stmt> {
///     type Type = ty::Blob;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: ty::Blob) -> Result<Self> {
///         Ok(MyBytes(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value BLOB);
///
///     INSERT INTO test (value) VALUES (X'2A2B2C');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyBytes>()?, Some(MyBytes(&[0x2A, 0x2B, 0x2C])));
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Blob {
    index: c_int,
    len: usize,
}

impl Blob {
    /// Returns the length in bytes of the blob column.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns if the blob column is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the column index.
    #[inline]
    pub(crate) fn column(&self) -> c_int {
        self.index
    }
}

/// [`Type`] implementation for a blob.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyBytes<'stmt>(&'stmt [u8]);
///
/// impl<'stmt> FromColumn<'stmt> for MyBytes<'stmt> {
///     type Type = ty::Blob;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: ty::Blob) -> Result<Self> {
///         Ok(MyBytes(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value BLOB);
///
///     INSERT INTO test (value) VALUES (X'2A2B2C');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyBytes>()?, Some(MyBytes(&[0x2A, 0x2B, 0x2C])));
/// # Ok::<_, sqll::Error>(())
/// ```
unsafe impl Type for Blob {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        unsafe {
            // Note that this type check is important, because it locks the type
            // of conversion we permit for a blob column.
            type_check(stmt, index, ValueType::BLOB)?;

            let len = ffi::sqlite3_column_bytes(stmt.as_ptr(), index);

            // This is unlikely to not be optimized out, but for the off chance
            // we still keep it.
            let Ok(len) = usize::try_from(len) else {
                return Err(Error::new(
                    Code::ERROR,
                    format_args!("column size {len} exceeds addressable memory"),
                ));
            };

            Ok(Self { index, len })
        }
    }
}

/// [`Type`] implementation for a nullable column.
///
/// The `T` parameter must be a [`NotNull`] type marker. This constraint
/// prevents assymetric types from being defined.
///
/// So something like this is supported:
///
/// ```
/// # use sqll::ty;
/// # fn ret() ->
/// ty::Nullable<ty::Integer>
/// # { todo!() }
/// ```
///
/// While something like this is denied at compile time:
///
/// ```compile_fail
/// # use sqll::ty;
/// # fn ret() ->
/// // This will not compile because Nullable does not implement `NotNull`.
/// ty::Nullable<ty::Nullable<ty::Integer>>;
/// # { todo!() }
/// ```
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyOptional(Option<u32>);
///
/// impl FromColumn<'_> for MyOptional {
///     type Type = ty::Nullable<ty::Integer>;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Nullable<ty::Integer>) -> Result<Self> {
///         Ok(MyOptional(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value INTEGER);
///
///     INSERT INTO test (value) VALUES (42), (NULL);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyOptional>()?, Some(MyOptional(Some(42))));
/// assert_eq!(stmt.next::<MyOptional>()?, Some(MyOptional(None)));
/// assert_eq!(stmt.next::<MyOptional>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Nullable<T>
where
    T: NotNull,
{
    pub(crate) inner: Option<T>,
}

/// [`Type`] implementation for a nullable column.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyOptional(Option<u32>);
///
/// impl FromColumn<'_> for MyOptional {
///     type Type = ty::Nullable<ty::Integer>;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Nullable<ty::Integer>) -> Result<Self> {
///         Ok(MyOptional(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value INTEGER);
///
///     INSERT INTO test (value) VALUES (42), (NULL);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM test")?;
///
/// assert_eq!(stmt.next::<MyOptional>()?, Some(MyOptional(Some(42))));
/// assert_eq!(stmt.next::<MyOptional>()?, Some(MyOptional(None)));
/// assert_eq!(stmt.next::<MyOptional>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
unsafe impl<T> Type for Nullable<T>
where
    T: NotNull,
{
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == ValueType::NULL {
            return Ok(Nullable { inner: None });
        }

        Ok(Nullable {
            inner: Some(T::check(stmt, index)?),
        })
    }
}

// NB: We have to perform strict type checking to avoid auto-conversion, if we
// permit it, the pointers that have previously been fetched for a given column
// may become invalidated.
//
// See: https://sqlite.org/c3ref/column_blob.html
#[inline(always)]
fn type_check(stmt: &Statement, index: c_int, expected: ValueType) -> Result<()> {
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
