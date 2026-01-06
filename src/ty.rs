use core::ffi::c_int;
use core::marker::PhantomData;

use crate::ffi;
use crate::{Code, Error, Null, Result, Statement, Text, Type};

mod sealed {
    use crate::{Null, Text};

    use super::{Dynamic, Primitive, Unsized};

    pub trait Sealed
    where
        Self: Sized,
    {
    }

    impl Sealed for Dynamic {}
    impl Sealed for Primitive<f64> {}
    impl Sealed for Primitive<i64> {}
    impl Sealed for Primitive<Null> {}
    impl Sealed for Unsized<[u8]> {}
    impl Sealed for Unsized<Text> {}
    impl<T> Sealed for Option<T> where T: Sealed {}
}

/// A trait which defines the underlying static value type that is supported by
/// a value that implements [`FromColumn`] or [`FromUnsizedColumn`].
///
/// One thing worth noting about SQLite is that tables are dynamically typed.
/// Any column of any type can contain any value. If there is a discrepancy
/// during loading, a process known as auto-conversion will be attempted. This
/// however can cause problems, since pointers, which are subseqeuently used to
/// construct references in Rust may be invalidated.
///
/// We carefully provide an API to ensure that references loaded from sqlite
/// remain valid. This type is a key component to that. We break loading up into
/// two steps on of them being checking which is done through
/// [`ValueType::check`].
///
/// We ensure that the type conversion is idempotent by sealing this unsafe
/// trait and requiring it to be used when loading a column of a particular
/// type. This way, we can hopefully ensures that pointers remain valid for the
/// lifetime of the column being loaded.
///
/// [`FromColumn`]: crate::FromColumn
/// [`FromUnsizedColumn`]: crate::FromUnsizedColumn
///
/// # Safety
///
/// Implementors must ensure that the type check exercises the underlying
/// statement in a manner which ensures that the values which will later be
/// loaded are cached and won't change.
pub unsafe trait ValueType
where
    Self: self::sealed::Sealed + Sized,
{
    /// Perform checks and warm up for the given column ensuring that any
    /// auto-conversion that needs to occur to load the field is done.
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self>;
}

/// The [`ValueType`] implementation which defines a primitive type.
///
/// If your type loads from a primitive type like [`i64`], or [`f64`], or
/// [`Null`], you should use this as the associated type for
/// [`FromColumn::Type`].
///
/// [`FromColumn::Type`]: crate::FromColumn::Type
pub struct Primitive<T> {
    pub(crate) index: c_int,
    _marker: PhantomData<T>,
}

/// [`ValueType`] implementation for [`Null`].
///
/// This must be used when implementing custom types that can be read from
/// column and a [`Null`] value is expected.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Primitive, Null};
///
/// struct MyNull(Null);
///
/// impl FromColumn<'_> for MyNull {
///     type Type = Primitive<Null>;
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
unsafe impl ValueType for Primitive<Null> {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::NULL)?;

        Ok(Self {
            index,
            _marker: PhantomData,
        })
    }
}

/// [`ValueType`] implementation for [`f64`].
///
/// This must be used when implementing custom types that can be read from
/// column and a [`f64`] value is expected.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Primitive};
///
/// struct MyFloat(f64);
///
/// impl FromColumn<'_> for MyFloat {
///     type Type = Primitive<f64>;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: Self::Type) -> Result<Self> {
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
unsafe impl ValueType for Primitive<f64> {
    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::FLOAT)?;

        Ok(Self {
            index,
            _marker: PhantomData,
        })
    }
}

/// [`ValueType`] implementation for [`i64`].
///
/// This must be used when implementing custom types that can be read from
/// column and a [`i64`] value is expected.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Primitive};
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyInteger(i64);
///
/// impl FromColumn<'_> for MyInteger {
///     type Type = Primitive<i64>;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: Self::Type) -> Result<Self> {
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
unsafe impl ValueType for Primitive<i64> {
    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::INTEGER)?;

        Ok(Self {
            index,
            _marker: PhantomData,
        })
    }
}

/// Type returned when calling [`ValueType::check`] for a dynamic [`Value`].
///
/// [`Value`]: crate::Value
pub struct Dynamic {
    pub(crate) kind: DynamicKind,
}

pub(crate) enum DynamicKind {
    Blob(Unsized<[u8]>),
    Text(Unsized<Text>),
    Float(Primitive<f64>),
    Integer(Primitive<i64>),
    Null(Primitive<Null>),
}

unsafe impl ValueType for Dynamic {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        let kind = match stmt.column_type(index) {
            Type::BLOB => DynamicKind::Blob(Unsized::check(stmt, index)?),
            Type::TEXT => DynamicKind::Text(Unsized::check(stmt, index)?),
            Type::FLOAT => DynamicKind::Float(Primitive::check(stmt, index)?),
            Type::INTEGER => DynamicKind::Integer(Primitive::check(stmt, index)?),
            Type::NULL => DynamicKind::Null(Primitive::check(stmt, index)?),
            ty => {
                return Err(Error::new(
                    Code::MISMATCH,
                    format_args!("dynamic value has unsupported column type {ty}"),
                ));
            }
        };

        Ok(Dynamic { kind })
    }
}

/// The outcome of calling [`ValueType::check`] for [`str`] or a byte slice.
pub struct Unsized<T>
where
    T: ?Sized,
{
    pub(crate) index: c_int,
    pub(crate) len: usize,
    _marker: PhantomData<T>,
}

impl<T> Unsized<T>
where
    T: ?Sized,
{
    /// Returns the length of the prepared bytes.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the prepared bytes is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// [`ValueType`] implementation for a string.
///
/// This must be used when implementing custom types that can be read from
/// column and a string value is expected.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Text, Unsized};
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyString<'stmt>(&'stmt str);
///
/// impl<'stmt> FromColumn<'stmt> for MyString<'stmt> {
///     type Type = Unsized<Text>;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: Self::Type) -> Result<Self> {
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
unsafe impl ValueType for Unsized<Text> {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        unsafe {
            // Note that this type check is important, because it locks the type
            // of conversion we permit for a string column.
            type_check(stmt, index, Type::TEXT)?;

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

            Ok(Self {
                index,
                len,
                _marker: PhantomData,
            })
        }
    }
}

/// [`ValueType`] implementation for a blob.
///
/// This must be used when implementing custom types that can be read from
/// column and a blob value is expected.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromColumn, Result, Statement, Unsized};
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct MyBytes<'stmt>(&'stmt [u8]);
///
/// impl<'stmt> FromColumn<'stmt> for MyBytes<'stmt> {
///     type Type = Unsized<[u8]>;
///
///     #[inline]
///     fn from_column(stmt: &'stmt Statement, index: Self::Type) -> Result<Self> {
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
unsafe impl ValueType for Unsized<[u8]> {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        unsafe {
            // Note that this type check is important, because it locks the type
            // of conversion we permit for a blob column.
            type_check(stmt, index, Type::BLOB)?;

            let len = ffi::sqlite3_column_bytes(stmt.as_ptr(), index);

            // This is unlikely to not be optimized out, but for the off chance
            // we still keep it.
            let Ok(len) = usize::try_from(len) else {
                return Err(Error::new(
                    Code::ERROR,
                    format_args!("column size {len} exceeds addressable memory"),
                ));
            };

            Ok(Unsized {
                index,
                len,
                _marker: PhantomData,
            })
        }
    }
}

unsafe impl<T> ValueType for Option<T>
where
    T: ValueType,
{
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == Type::NULL {
            return Ok(None);
        }

        Ok(Some(T::check(stmt, index)?))
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
