use core::ffi::c_int;
use core::slice;

use crate::ffi;
use crate::from_column::type_check;
use crate::{Code, Error, Result, Statement, Type};

/// The outcome of calling [`FromUnsizedColumn::check_unsized`] for [`str`] or a
/// byte slice.
pub struct CheckBytes {
    index: c_int,
    len: usize,
}

impl CheckBytes {
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

/// A type suitable for borrow directly out of a prepared statement.
///
/// Use with [`Statement::get_unsized`].
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
pub trait FromUnsizedColumn {
    /// The prepared check for reading the unsized column.
    type CheckUnsized;

    /// Perform checks and warm up for the given column.
    ///
    /// Calling this ensures that any conversion performed over the column is
    /// done before we attempt to read it.
    fn check_unsized(stmt: &mut Statement, index: c_int) -> Result<Self::CheckUnsized>;

    /// Read an unsized value from the specified column.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::ffi::c_int;
    /// use core::fmt;
    ///
    /// use sqll::{Connection, FromUnsizedColumn, Result, Statement, CheckBytes};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// #[derive(Debug, PartialEq, Eq)]
    /// #[repr(transparent)]
    /// struct Id([u8]);
    ///
    /// impl Id {
    ///     fn new(data: &[u8]) -> &Self {
    ///         // SAFETY: Id is #[repr(transparent)] over [u8].
    ///         unsafe { &*(data as *const [u8] as *const Id) }
    ///     }
    /// }
    ///
    /// impl FromUnsizedColumn for Id {
    ///     type CheckUnsized = CheckBytes;
    ///
    ///     #[inline]
    ///     fn check_unsized(stmt: &mut Statement, index: c_int) -> Result<Self::CheckUnsized> {
    ///         <[u8]>::check_unsized(stmt, index)
    ///     }
    ///
    ///     #[inline]
    ///     fn load_unsized(stmt: &Statement, check: Self::CheckUnsized) -> Result<&Self> {
    ///         Ok(Id::new(<[u8]>::load_unsized(stmt, check)?))
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
    /// assert_eq!(select.get_unsized::<Id>(0)?, Id::new(b"\xab\xcd\xab\xcd"));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    fn load_unsized(stmt: &Statement, check: Self::CheckUnsized) -> Result<&Self>;
}

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
/// while stmt.step()?.is_row() {
///     let name = stmt.get_unsized::<str>(0)?;
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
///     let e = stmt.get_unsized::<str>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromUnsizedColumn for str {
    type CheckUnsized = CheckBytes;

    #[inline]
    fn check_unsized(stmt: &mut Statement, index: c_int) -> Result<Self::CheckUnsized> {
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

            Ok(CheckBytes { index, len })
        }
    }

    #[inline]
    fn load_unsized(
        stmt: &Statement,
        CheckBytes { index, len }: Self::CheckUnsized,
    ) -> Result<&Self> {
        unsafe {
            if len == 0 {
                return Ok("");
            }

            // SAFETY: Documentation guaranteeds this always returns a valid UTF-8 by sqlite.
            let ptr = ffi::sqlite3_column_text(stmt.as_ptr(), index);
            debug_assert!(!ptr.is_null(), "sqlite3_column_bytes returned null pointer");
            let bytes = slice::from_raw_parts(ptr, len);
            let string = str::from_utf8_unchecked(bytes);
            Ok(string)
        }
    }
}

/// [`FromUnsizedColumn`] implementation for `[u8]`.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name BLOB);
///
///     INSERT INTO users (name) VALUES (X'aabb'), (X'bbcc'), (X'');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let name = stmt.get_unsized::<[u8]>(0)?;
///     assert!(matches!(name, b"\xaa\xbb" | b"\xbb\xcc" | b""));
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
///     let e = stmt.get_unsized::<[u8]>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromUnsizedColumn for [u8] {
    type CheckUnsized = CheckBytes;

    #[inline]
    fn check_unsized(stmt: &mut Statement, index: c_int) -> Result<Self::CheckUnsized> {
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

            Ok(CheckBytes { index, len })
        }
    }

    #[inline]
    fn load_unsized(
        stmt: &Statement,
        CheckBytes { index, len }: Self::CheckUnsized,
    ) -> Result<&Self> {
        unsafe {
            let ptr = ffi::sqlite3_column_blob(stmt.as_ptr(), index);

            // NB: Per documentation, an empty column is null.
            if ptr.is_null() {
                return Ok(b"");
            }

            let bytes = slice::from_raw_parts(ptr.cast(), len);
            Ok(bytes)
        }
    }
}
