use core::ffi::c_int;
use core::marker::PhantomData;

use crate::ffi;
use crate::{Code, Error, Null, Result, Statement, Type};

mod sealed {
    use crate::Null;

    use super::{CheckBytes, CheckPrimitive, CheckValue};

    pub trait Sealed
    where
        Self: Sized,
    {
    }

    impl Sealed for CheckPrimitive<Null> {}
    impl Sealed for CheckPrimitive<f64> {}
    impl Sealed for CheckPrimitive<i64> {}
    impl Sealed for CheckValue {}
    impl Sealed for CheckBytes<str> {}
    impl Sealed for CheckBytes<[u8]> {}
    impl<T> Sealed for Option<T> where T: Sealed {}
}

/// A trait for performing checks on columns.
pub trait Check
where
    Self: self::sealed::Sealed + Sized,
{
    /// Perform checks and warm up for the given column.
    ///
    /// Calling this ensures that any conversion performed over the column is
    /// done before we attempt to read it.
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self>;
}

/// Type returned when calling [`FromColumn::check`] for a primitive value.
///
/// [`FromColumn::check`]: crate::FromColumn::check
pub struct CheckPrimitive<T> {
    pub(crate) index: c_int,
    _marker: PhantomData<T>,
}

impl Check for CheckPrimitive<Null> {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::NULL)?;

        Ok(Self {
            index,
            _marker: PhantomData,
        })
    }
}

impl Check for CheckPrimitive<f64> {
    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::FLOAT)?;

        Ok(Self {
            index,
            _marker: PhantomData,
        })
    }
}

impl Check for CheckPrimitive<i64> {
    #[inline]
    fn check(stmt: &'_ mut Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::INTEGER)?;

        Ok(Self {
            index,
            _marker: PhantomData,
        })
    }
}

/// Type returned when calling [`FromColumn::check`] for a dynamic [`Value`].
pub struct CheckValue {
    pub(crate) kind: CheckValueKind,
}

pub(crate) enum CheckValueKind {
    Blob(CheckBytes<[u8]>),
    Text(CheckBytes<str>),
    Float(CheckPrimitive<f64>),
    Integer(CheckPrimitive<i64>),
    Null(CheckPrimitive<Null>),
}

impl Check for CheckValue {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        let kind = match stmt.column_type(index) {
            Type::BLOB => CheckValueKind::Blob(CheckBytes::check(stmt, index)?),
            Type::TEXT => CheckValueKind::Text(CheckBytes::check(stmt, index)?),
            Type::FLOAT => CheckValueKind::Float(CheckPrimitive::check(stmt, index)?),
            Type::INTEGER => CheckValueKind::Integer(CheckPrimitive::check(stmt, index)?),
            Type::NULL => CheckValueKind::Null(CheckPrimitive::check(stmt, index)?),
            ty => {
                return Err(Error::new(
                    Code::MISMATCH,
                    format_args!("dynamic value has unsupported column type {ty}"),
                ));
            }
        };

        Ok(CheckValue { kind })
    }
}

/// The outcome of calling [`FromUnsizedColumn::check_unsized`] for [`str`] or a
/// byte slice.
///
/// [`FromUnsizedColumn::check_unsized`]: crate::FromUnsizedColumn::check_unsized
pub struct CheckBytes<T>
where
    T: ?Sized,
{
    pub(crate) index: c_int,
    pub(crate) len: usize,
    _marker: PhantomData<T>,
}

impl Check for CheckBytes<str> {
    #[inline]
    fn check(stmt: &mut Statement, index: c_int) -> Result<Self> {
        unsafe {
            // Note that this type check is important, because it locks the type
            // of conversion we permit for a string column.
            type_check(stmt, index, Type::TEXT)?;

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

impl Check for CheckBytes<[u8]> {
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

            Ok(CheckBytes {
                index,
                len,
                _marker: PhantomData,
            })
        }
    }
}

impl<T> CheckBytes<T>
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

impl<T> Check for Option<T>
where
    T: Check,
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
