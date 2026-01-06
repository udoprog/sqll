use core::ffi::c_int;
use core::fmt;

use crate::ffi;

/// The type of a column.
///
/// This type exists in contrast to [`Type`], which is a compile-time trait
/// defining a particular value type.
///
/// See [`Statement::column_type`] and [`Value::column_type`].
///
/// [`Type`]: crate::ty::Type
/// [`Statement::column_type`]: crate::Statement::column_type
/// [`Value::column_type`]: crate::Value::column_type
///
/// # Examples
///
/// ```
/// use sqll::{Connection, ValueType};
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value INTEGER, text_value TEXT);
///
///     INSERT INTO test (value, text_value) VALUES (42, 'Hello, world!');
/// "#)?;
///
/// let mut select = c.prepare("SELECT value, text_value FROM test")?;
///
/// assert!(select.step()?.is_row());
/// assert_eq!(select.column_type(0), ValueType::INTEGER);
/// assert_eq!(select.column_type(1), ValueType::TEXT);
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct ValueType {
    raw: c_int,
}

impl ValueType {
    /// Construct from a raw type.
    #[inline]
    pub(crate) const fn new(raw: c_int) -> Self {
        Self { raw }
    }

    /// The integer type.
    ///
    /// This is represented in rust by the [`i64`] value and corresponds to the
    /// [`Integer`][type] compile-time type.
    ///
    /// [type]: crate::ty::Integer
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, ValueType};
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (value INTEGER);
    ///
    ///     INSERT INTO test (value) VALUES (42);
    /// "#)?;
    ///
    /// let mut select = c.prepare("SELECT value FROM test")?;
    /// assert_eq!(select.column_type(0), ValueType::NULL);
    /// assert!(select.step()?.is_row());
    ///
    /// assert_eq!(select.column_type(0), ValueType::INTEGER);
    /// assert!(select.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub const INTEGER: Self = Self::new(ffi::SQLITE_INTEGER);

    /// The floating-point type.
    ///
    /// This is represented in rust by the [`f64`] value and corresponds to the
    /// [`Float`][type] compile-time type.
    ///
    /// [type]: crate::ty::Float
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, ValueType};
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (value FLOAT);
    ///
    ///     INSERT INTO test (value) VALUES (42.0);
    /// "#)?;
    ///
    /// let mut select = c.prepare("SELECT value FROM test")?;
    /// assert_eq!(select.column_type(0), ValueType::NULL);
    /// assert!(select.step()?.is_row());
    ///
    /// assert_eq!(select.column_type(0), ValueType::FLOAT);
    /// assert!(select.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub const FLOAT: Self = Self::new(ffi::SQLITE_FLOAT);

    /// The text type.
    ///
    /// This is represented in rust by the [`Text`] value and corresponds to the
    /// [`Text`][type] compile-time type.
    ///
    /// [`Text`]: crate::Text
    /// [type]: crate::ty::Text
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, ValueType};
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (value TEXT);
    ///
    ///     INSERT INTO test (value) VALUES ('Hello, world!');
    /// "#)?;
    ///
    /// let mut select = c.prepare("SELECT value FROM test")?;
    /// assert_eq!(select.column_type(0), ValueType::NULL);
    /// assert!(select.step()?.is_row());
    ///
    /// assert_eq!(select.column_type(0), ValueType::TEXT);
    /// assert!(select.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub const TEXT: Self = Self::new(ffi::SQLITE_TEXT);

    /// The blob type.
    ///
    /// This is represented in rust by the `[u8]` slice and corresponds to the
    /// [`Blob`][type] compile-time type.
    ///
    /// [type]: crate::ty::Blob
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, ValueType};
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (value TEXT);
    ///
    ///     INSERT INTO test (value) VALUES (X'DEADBEEF');
    /// "#)?;
    ///
    /// let mut select = c.prepare("SELECT value FROM test")?;
    /// assert_eq!(select.column_type(0), ValueType::NULL);
    /// assert!(select.step()?.is_row());
    ///
    /// assert_eq!(select.column_type(0), ValueType::BLOB);
    /// assert!(select.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub const BLOB: Self = Self::new(ffi::SQLITE_BLOB);

    /// The null type.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, ValueType};
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (value);
    ///
    ///     INSERT INTO test (value) VALUES (NULL);
    /// "#)?;
    ///
    /// let mut select = c.prepare("SELECT value FROM test")?;
    /// assert_eq!(select.column_type(0), ValueType::NULL);
    /// assert!(select.step()?.is_row());
    ///
    /// assert_eq!(select.column_type(0), ValueType::NULL);
    /// assert!(select.step()?.is_done());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub const NULL: Self = Self::new(ffi::SQLITE_NULL);
}

/// Display implementation for [`ValueType`].
///
/// # Examples
///
/// ```
/// use sqll::ValueType;
///
/// assert_eq!(ValueType::INTEGER.to_string(), "INTEGER");
/// assert_eq!(ValueType::FLOAT.to_string(), "FLOAT");
/// assert_eq!(ValueType::TEXT.to_string(), "TEXT");
/// assert_eq!(ValueType::BLOB.to_string(), "BLOB");
/// assert_eq!(ValueType::NULL.to_string(), "NULL");
/// ```
impl fmt::Display for ValueType {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.raw {
            ffi::SQLITE_BLOB => write!(f, "BLOB"),
            ffi::SQLITE_TEXT => write!(f, "TEXT"),
            ffi::SQLITE_FLOAT => write!(f, "FLOAT"),
            ffi::SQLITE_INTEGER => write!(f, "INTEGER"),
            ffi::SQLITE_NULL => write!(f, "NULL"),
            raw => write!(f, "UNKNOWN({raw})"),
        }
    }
}

impl fmt::Debug for ValueType {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.raw {
            ffi::SQLITE_BLOB => write!(f, "BLOB"),
            ffi::SQLITE_TEXT => write!(f, "TEXT"),
            ffi::SQLITE_FLOAT => write!(f, "FLOAT"),
            ffi::SQLITE_INTEGER => write!(f, "INTEGER"),
            ffi::SQLITE_NULL => write!(f, "NULL"),
            raw => write!(f, "UNKNOWN({raw})"),
        }
    }
}
