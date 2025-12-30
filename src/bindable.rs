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
