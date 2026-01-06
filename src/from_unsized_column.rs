use core::slice;

use crate::ffi;
use crate::{Result, Statement, Text, Unsized, ValueType};

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
    /// The prepared index for reading the column.
    //
    /// This must designate one of the database-primitive types as checks, like:
    /// * [`Unsized<T>`] where `T` is a slice type like `[u8]` or `str`.
    ///
    /// When this value is received in [`FromUnsizedColumn::from_unsized_column`] it
    /// can be used to actually load the a value of the underlying type.
    type UnsizedType: ValueType;

    /// Read an unsized value from the specified column.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::ffi::c_int;
    ///
    /// use sqll::{Connection, FromUnsizedColumn, Result, Statement, Unsized};
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
    ///     type UnsizedType = Unsized<[u8]>;
    ///
    ///     #[inline]
    ///     fn from_unsized_column(stmt: &Statement, index: Unsized<[u8]>) -> Result<&Self> {
    ///         Ok(Id::new(<_>::from_unsized_column(stmt, index)?))
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
    fn from_unsized_column(stmt: &Statement, index: Self::UnsizedType) -> Result<&Self>;
}

/// [`FromUnsizedColumn`] implementation for [`Text`].
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
/// assert!(stmt.step()?.is_row());
/// let name = stmt.get_unsized::<Text>(0)?;
/// assert_eq!(name, "Alice");
///
/// assert!(stmt.step()?.is_row());
/// let name = stmt.get_unsized::<Text>(0)?;
/// assert_eq!(name, "Bob");
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
impl FromUnsizedColumn for Text {
    type UnsizedType = Unsized<Text>;

    #[inline]
    fn from_unsized_column(
        stmt: &Statement,
        Unsized { index, len, .. }: Self::UnsizedType,
    ) -> Result<&Self> {
        unsafe {
            if len == 0 {
                return Ok(Text::from_bytes(b""));
            }

            // SAFETY: Documentation guaranteeds this always returns a valid UTF-8 by sqlite.
            let ptr = ffi::sqlite3_column_text(stmt.as_ptr(), index);
            debug_assert!(!ptr.is_null(), "sqlite3_column_bytes returned null pointer");
            let text = slice::from_raw_parts(ptr, len);
            Ok(Text::from_bytes(text))
        }
    }
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
    type UnsizedType = Unsized<Text>;

    #[inline]
    fn from_unsized_column(stmt: &Statement, index: Self::UnsizedType) -> Result<&Self> {
        let Ok(string) = Text::from_unsized_column(stmt, index)?.to_str() else {
            return Err(crate::Error::new(
                crate::Code::MISMATCH,
                "column is not valid UTF-8",
            ));
        };

        Ok(string)
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
    type UnsizedType = Unsized<[u8]>;

    #[inline]
    fn from_unsized_column(
        stmt: &Statement,
        Unsized { index, len, .. }: Self::UnsizedType,
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
