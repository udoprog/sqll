use core::ffi::c_int;
use core::slice;

use crate::gettable::type_check;
use crate::{Code, Error, Result, Statement};
use crate::{Type, ffi};

mod sealed {
    pub trait Sealed {}
    impl Sealed for str {}
    impl Sealed for [u8] {}
}

/// A type suitable for borrow directly out of a prepared statement.
///
/// Use with [`Statement::borrow`].
pub trait Borrowable
where
    Self: self::sealed::Sealed,
{
    #[doc(hidden)]
    fn borrow(stmt: &Statement, index: c_int) -> Result<&Self>;
}

/// [`Borrowable`] implementation for [`str`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
///     CREATE TABLE users (name TEXT);
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// ")?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let name = row.borrow::<str>(0)?;
///     assert!(matches!(name, "Alice" | "Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
///     CREATE TABLE users (id INTEGER);
///     INSERT INTO users (id) VALUES (1), (2);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
/// let mut name = String::new();
///
/// while let Some(row) = stmt.next()? {
///     let e = row.borrow::<str>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Borrowable for str {
    #[inline]
    fn borrow(stmt: &Statement, index: c_int) -> Result<&Self> {
        unsafe {
            type_check(stmt, index, Type::TEXT)?;

            let len = ffi::sqlite3_column_bytes(stmt.as_ptr(), index);

            let Ok(len) = usize::try_from(len) else {
                return Err(Error::new(Code::MISMATCH));
            };

            if len == 0 {
                return Ok("");
            }

            // SAFETY: This is guaranteed to return valid UTF-8 by sqlite.
            let ptr = ffi::sqlite3_column_text(stmt.as_ptr(), index);

            if ptr.is_null() {
                return Ok("");
            }

            let bytes = slice::from_raw_parts(ptr, len);
            let string = str::from_utf8_unchecked(bytes);
            Ok(string)
        }
    }
}

/// [`Borrowable`] implementation for `[u8]`.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (name BLOB);
/// INSERT INTO users (name) VALUES (X'aabb'), (X'bbcc');
/// ")?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
/// let mut name = Vec::<u8>::new();
///
/// while let Some(row) = stmt.next()? {
///     name.clear();
///     let name = row.borrow::<[u8]>(0)?;
///     assert!(matches!(name, b"\xaa\xbb" | b"\xbb\xcc"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
/// let mut name = Vec::<u8>::new();
///
/// while let Some(row) = stmt.next()? {
///     name.clear();
///     let e = row.borrow::<[u8]>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Borrowable for [u8] {
    #[inline]
    fn borrow(stmt: &Statement, index: c_int) -> Result<&Self> {
        unsafe {
            type_check(stmt, index, Type::BLOB)?;

            let i = c_int::try_from(index).unwrap_or(c_int::MAX);

            let Ok(len) = usize::try_from(ffi::sqlite3_column_bytes(stmt.as_ptr(), i)) else {
                return Err(Error::new(Code::MISMATCH));
            };

            if len == 0 {
                return Ok(b"");
            }

            let ptr = ffi::sqlite3_column_blob(stmt.as_ptr(), i);

            if ptr.is_null() {
                return Ok(b"");
            }

            let bytes = slice::from_raw_parts(ptr.cast(), len);
            Ok(bytes)
        }
    }
}
