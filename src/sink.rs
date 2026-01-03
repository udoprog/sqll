use core::ffi::c_int;
use core::slice;
use core::str;

use alloc::string::String;
use alloc::vec::Vec;

use crate::ffi;
use crate::from_column::type_check;
use crate::{Code, Error, Result, Statement, Type};

mod sealed {
    use alloc::string::String;
    use alloc::vec::Vec;

    pub trait Sealed {}
    impl Sealed for String {}
    impl Sealed for Vec<u8> {}
    impl<T> Sealed for &mut T where T: ?Sized + Sealed {}
}

/// Trait governing types which can be written to in-place.
///
/// Use with [`Statement::read`].
pub trait Sink
where
    Self: self::sealed::Sealed,
{
    #[doc(hidden)]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()>;
}

impl<T> Sink for &mut T
where
    T: ?Sized + Sink,
{
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        (**self).write(stmt, index)
    }
}

/// [`Sink`] implementation for [`String`] which appends the content of the
/// column to the current container.
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
/// let mut name = String::new();
///
/// while stmt.step()?.is_row() {
///     name.clear();
///     stmt.read(0, &mut name)?;
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
/// let mut name = String::new();
///
/// while stmt.step()?.is_row() {
///     name.clear();
///     let e = stmt.read(0, &mut name).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Sink for String {
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        unsafe {
            type_check(stmt, index, Type::TEXT)?;

            let len = ffi::sqlite3_column_bytes(stmt.as_ptr(), index);

            let Ok(len) = usize::try_from(len) else {
                return Err(Error::new(
                    Code::ERROR,
                    "column size exceeds addressable memory",
                ));
            };

            if len == 0 {
                return Ok(());
            }

            // SAFETY: This is guaranteed to return valid UTF-8 by sqlite.
            let ptr = ffi::sqlite3_column_text(stmt.as_ptr(), index);

            if ptr.is_null() {
                return Ok(());
            }

            self.reserve(len);

            let bytes = slice::from_raw_parts(ptr, len);
            let string = str::from_utf8_unchecked(bytes);
            self.push_str(string);
            Ok(())
        }
    }
}

/// [`Sink`] implementation for [`String`] which appends the content of the
/// column to the current container.
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
/// let mut blob = Vec::<u8>::new();
///
/// while stmt.step()?.is_row() {
///     blob.clear();
///     stmt.read(0, &mut blob)?;
///     assert!(matches!(blob.as_slice(), b"\xaa\xbb" | b"\xbb\xcc"));
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
/// let mut name = Vec::<u8>::new();
///
/// while stmt.step()?.is_row() {
///     name.clear();
///     let e = stmt.read(0, &mut name).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Sink for Vec<u8> {
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        unsafe {
            type_check(stmt, index, Type::BLOB)?;

            let Ok(len) = usize::try_from(ffi::sqlite3_column_bytes(stmt.as_ptr(), index)) else {
                return Err(Error::new(
                    Code::MISMATCH,
                    "column size exceeds addressable memory",
                ));
            };

            if len == 0 {
                return Ok(());
            }

            let ptr = ffi::sqlite3_column_blob(stmt.as_ptr(), index);

            if ptr.is_null() {
                return Ok(());
            }

            self.reserve(len);

            let bytes = slice::from_raw_parts(ptr.cast(), len);
            self.extend_from_slice(bytes);
            Ok(())
        }
    }
}
