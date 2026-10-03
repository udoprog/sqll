use core::slice;

use crate::ffi;
use crate::ty::{self, Type};
use crate::{Code, Column, Error, Result, Statement, Text, ValueType};

/// A type suitable for borrow directly out of a prepared statement.
///
/// Use with [`Statement::unsized_column`].
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
    /// The type of a column.
    ///
    /// This must designate one of the database-primitive types as checks, like:
    /// * [`Integer`] or [`Float`].
    /// * [`Blob`] or [`Text`].
    /// * [`Any`] for dynamically typed column.
    /// * [`Nullable<T>`] for nullable types.
    ///
    /// When this value is received in [`from_unsized_column`] it can be used to
    /// actually load the a value of the underlying type.
    ///
    /// [`Integer`]: crate::ty::Integer
    /// [`Float`]: crate::ty::Float
    /// [`Blob`]: crate::ty::Blob
    /// [`Text`]: crate::ty::Text
    /// [`Any`]: crate::ty::Any
    /// [`Nullable<T>`]: crate::ty::Nullable
    /// [`from_unsized_column`]: FromUnsizedColumn::from_unsized_column
    type Type: Type;

    /// Read an unsized value from the specified column.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::ffi::c_int;
    ///
    /// use sqll::{Connection, FromUnsizedColumn, Result, Statement};
    /// use sqll::ty;
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
    ///     type Type = ty::Blob;
    ///
    ///     #[inline]
    ///     fn from_unsized_column(stmt: &Statement, index: ty::Blob) -> Result<&Self> {
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
    /// assert_eq!(select.unsized_column::<Id>(0)?, Id::new(b"\xab\xcd\xab\xcd"));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    fn from_unsized_column(stmt: &Statement, index: Self::Type) -> Result<&Self>;
}

/// [`FromUnsizedColumn`] implementation for [`Text`].
///
/// This corresponds exactly with the internal SQLite [`TEXT`][value-type] or
/// [`Text`][type] types.
///
/// [value-type]: crate::ValueType::TEXT
/// [type]: crate::ty::Text
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
/// let name = stmt.unsized_column::<Text>(0)?;
/// assert_eq!(name, "Alice");
///
/// assert!(stmt.step()?.is_row());
/// let name = stmt.unsized_column::<Text>(0)?;
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
///     let e = stmt.unsized_column::<str>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromUnsizedColumn for Text {
    type Type = ty::Text;

    #[inline]
    fn from_unsized_column(stmt: &Statement, index: ty::Text) -> Result<&Self> {
        // SAFETY: `column_bytes` re-validates the column type before reading
        // so that a stale or foreign `ty::Text` cannot trigger a conversion
        // which would invalidate previously borrowed values.
        let text = unsafe { column_bytes(stmt, index.column(), ValueType::TEXT)? };
        Ok(Text::from_bytes(text))
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
///     let name = stmt.unsized_column::<str>(0)?;
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
///     let e = stmt.unsized_column::<str>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromUnsizedColumn for str {
    type Type = ty::Text;

    #[inline]
    fn from_unsized_column(stmt: &Statement, index: Self::Type) -> Result<&Self> {
        let text = Text::from_unsized_column(stmt, index)?;

        let Ok(string) = text.to_str() else {
            return Err(Error::new(Code::MISMATCH, "column is not valid UTF-8"));
        };

        Ok(string)
    }
}

/// [`FromUnsizedColumn`] implementation for `[u8]`.
///
/// This corresponds exactly with the internal SQLite [`BLOB`][value-type] or
/// [`Blob`][type] types.
///
/// [value-type]: crate::ValueType::BLOB
/// [type]: crate::ty::Blob
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
///     let name = stmt.unsized_column::<[u8]>(0)?;
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
///     let e = stmt.unsized_column::<[u8]>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromUnsizedColumn for [u8] {
    type Type = ty::Blob;

    #[inline]
    fn from_unsized_column(stmt: &Statement, index: ty::Blob) -> Result<&Self> {
        // SAFETY: See the implementation for `Text`.
        unsafe { column_bytes(stmt, index.column(), ValueType::BLOB) }
    }
}

/// Load the bytes of a `TEXT` or `BLOB` column.
///
/// This does not trust the length or type captured by [`Type::check`], since
/// type tokens can be held onto past a call to [`Statement::step`] or used with
/// a different statement than the one they were checked against. Instead the
/// column type is checked again and the length is read from the current row.
///
/// The type check is what prevents auto-conversion, which could otherwise
/// invalidate references previously borrowed from the same column. For a
/// column which has already been checked with [`Type::check`] on the current
/// row, no conversion takes place.
///
/// # Safety
///
/// `expected` must be either [`ValueType::TEXT`] or [`ValueType::BLOB`].
#[inline]
unsafe fn column_bytes(stmt: &Statement, column: Column, expected: ValueType) -> Result<&[u8]> {
    let actual = stmt.column_type(column);

    if actual != expected {
        return Err(Error::new(
            Code::MISMATCH,
            format_args!("expected column type {expected} but found {actual}"),
        ));
    }

    unsafe {
        // NB: Per documentation, the pointer must be loaded before the
        // length, since loading text might cause a conversion of the
        // underlying representation, like from UTF-16 to UTF-8.
        let ptr = if expected == ValueType::TEXT {
            ffi::sqlite3_column_text(stmt.as_ptr(), column.raw())
        } else {
            ffi::sqlite3_column_blob(stmt.as_ptr(), column.raw()).cast::<u8>()
        };

        let len = ffi::sqlite3_column_bytes(stmt.as_ptr(), column.raw());

        let Ok(len) = usize::try_from(len) else {
            return Err(Error::new(
                Code::ERROR,
                format_args!("column size {len} exceeds addressable memory"),
            ));
        };

        if ptr.is_null() {
            // NB: Per documentation, an empty blob is returned as a null
            // pointer. A null pointer with a non-zero length would be an
            // allocation failure.
            if len == 0 {
                return Ok(&[]);
            }

            return Err(Error::new(
                Code::NOMEM,
                "failed to load column, out of memory",
            ));
        }

        Ok(slice::from_raw_parts(ptr, len))
    }
}
