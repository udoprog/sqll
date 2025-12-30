use core::ffi::c_int;
use core::slice;
use core::str;

use alloc::string::String;
use alloc::vec::Vec;

use sqlite3_sys as ffi;

use crate::{Error, Result, Statement};

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
/// Use with [`Statement::read_into`].
pub trait Writable
where
    Self: self::sealed::Sealed,
{
    #[doc(hidden)]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()>;
}

impl<T> Writable for &mut T
where
    T: ?Sized + Writable,
{
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        (**self).write(stmt, index)
    }
}

/// [`Writable`] implementation for [`String`] which appends the content of the
/// column to the current container.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
/// let mut name = String::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
/// let mut name = String::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_str(), "1" | "2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Writable for String {
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        unsafe {
            let len = ffi::sqlite3_column_bytes(stmt.as_ptr(), index);

            let Ok(len) = usize::try_from(len) else {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            };

            if len == 0 {
                return Ok(());
            }

            // SAFETY: This is guaranteed to return valid UTF-8 by sqlite.
            let ptr = ffi::sqlite3_column_text(stmt.as_ptr(), index);

            if ptr.is_null() {
                return Ok(());
            }

            let bytes = slice::from_raw_parts(ptr, len);
            let s = str::from_utf8_unchecked(bytes);
            self.push_str(s);
            Ok(())
        }
    }
}

/// [`Writable`] implementation for [`String`] which appends the content of the
/// column to the current container.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
/// let mut name = Vec::<u8>::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_slice(), b"Alice" | b"Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
/// let mut name = Vec::<u8>::new();
///
/// while let State::Row = stmt.step()? {
///     name.clear();
///     stmt.read_into(0, &mut name)?;
///     assert!(matches!(name.as_slice(), b"1" | b"2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Writable for Vec<u8> {
    #[inline]
    fn write(&mut self, stmt: &Statement, index: c_int) -> Result<()> {
        unsafe {
            let i = c_int::try_from(index).unwrap_or(c_int::MAX);

            let Ok(len) = usize::try_from(ffi::sqlite3_column_bytes(stmt.as_ptr(), i)) else {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            };

            if len == 0 {
                return Ok(());
            }

            let ptr = ffi::sqlite3_column_blob(stmt.as_ptr(), i);

            if ptr.is_null() {
                return Ok(());
            }

            let bytes = slice::from_raw_parts(ptr.cast(), len);
            self.extend_from_slice(bytes);
            Ok(())
        }
    }
}
