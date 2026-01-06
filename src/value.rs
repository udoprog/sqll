use core::ffi::c_int;
use core::fmt;

use crate::ffi;

/// The type of a value.
#[derive(Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct Type {
    raw: c_int,
}

impl Type {
    /// Construct from a raw type.
    #[inline]
    pub(crate) const fn new(raw: c_int) -> Self {
        Self { raw }
    }

    /// The blob type.
    pub const BLOB: Self = Self::new(ffi::SQLITE_BLOB);
    /// The text type.
    pub const TEXT: Self = Self::new(ffi::SQLITE_TEXT);
    /// The floating-point type.
    pub const FLOAT: Self = Self::new(ffi::SQLITE_FLOAT);
    /// The integer type.
    pub const INTEGER: Self = Self::new(ffi::SQLITE_INTEGER);
    /// The null type.
    pub const NULL: Self = Self::new(ffi::SQLITE_NULL);
}

impl fmt::Display for Type {
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

impl fmt::Debug for Type {
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

/// A dynamic value.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Value};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value);
///
///     INSERT INTO test (value) VALUES ('Hello, world!'), (42), (3.14), (X'DEADBEEF');
/// "#)?;
///
/// let mut select = c.prepare("SELECT value FROM test")?;
/// assert_eq!(select.next::<Value<'_>>()?, Some(Value::text("Hello, world!")));
/// assert_eq!(select.next::<Value<'_>>()?, Some(Value::integer(42)));
/// assert_eq!(select.next::<Value<'_>>()?, Some(Value::float(3.14)));
/// assert_eq!(select.next::<Value<'_>>()?, Some(Value::blob(&[0xDE, 0xAD, 0xBE, 0xEF])));
/// assert_eq!(select.next::<Value<'_>>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Cannot inhabit null values:
///
/// ```
/// use sqll::{Connection, Code, Value, Null};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value);
///
///     INSERT INTO test (value) VALUES (NULL);
/// "#)?;
///
/// let mut select = c.prepare("SELECT value FROM test")?;
/// let e = select.next::<Value<'_>>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// select.reset()?;
/// assert_eq!(select.iter::<Null>().collect::<Vec<_>>(), [Ok(Null)]);
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Clone, PartialEq)]
pub struct Value<'stmt> {
    kind: Kind<'stmt>,
}

impl<'stmt> Value<'stmt> {
    /// Return the kind of the value.
    #[inline]
    pub(crate) fn kind(&self) -> &Kind<'stmt> {
        &self.kind
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind<'stmt> {
    Integer(i64),
    Float(f64),
    Text(&'stmt str),
    Blob(&'stmt [u8]),
}

impl<'stmt> Value<'stmt> {
    /// Construct a integer value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::integer(42);
    /// assert_eq!(value.as_integer(), Some(42));
    /// ```
    #[inline]
    pub const fn integer(value: i64) -> Self {
        Self {
            kind: Kind::Integer(value),
        }
    }

    /// Construct a float value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::float(42.0);
    /// assert_eq!(value.as_float(), Some(42.0));
    /// ```
    #[inline]
    pub const fn float(value: f64) -> Self {
        Self {
            kind: Kind::Float(value),
        }
    }
    /// Construct a text value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::text("");
    /// assert_eq!(value.as_text(), Some(""));
    ///
    /// let value = Value::text("hello");
    /// assert_eq!(value.as_text(), Some("hello"));
    /// ```
    #[inline]
    pub const fn text(value: &'stmt str) -> Self {
        Self {
            kind: Kind::Text(value),
        }
    }

    /// Construct a blob value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::blob(&[]);
    /// assert_eq!(value.as_blob(), Some(&[][..]));
    /// ```
    #[inline]
    pub const fn blob(value: &'stmt [u8]) -> Self {
        Self {
            kind: Kind::Blob(value),
        }
    }

    /// Return the integer number if the value is `Integer`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::integer(42);
    /// assert_eq!(value.as_integer(), Some(42));
    /// ```
    #[inline]
    pub const fn as_integer(&self) -> Option<i64> {
        if let Kind::Integer(value) = self.kind {
            return Some(value);
        }

        None
    }

    /// Return the floating-point number if the value is `Float`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::float(42.0);
    /// assert_eq!(value.as_float(), Some(42.0));
    /// ```
    #[inline]
    pub const fn as_float(&self) -> Option<f64> {
        if let Kind::Float(value) = self.kind {
            return Some(value);
        }

        None
    }

    /// Return the string if the value is `String`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::text("");
    /// assert_eq!(value.as_text(), Some(""));
    ///
    /// let value = Value::text("hello");
    /// assert_eq!(value.as_text(), Some("hello"));
    /// ```
    #[inline]
    pub const fn as_text(&self) -> Option<&'stmt str> {
        if let Kind::Text(value) = self.kind {
            return Some(value);
        }

        None
    }

    /// Return the binary data if the value is `Binary`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Value;
    ///
    /// let value = Value::blob(&[]);
    /// assert_eq!(value.as_blob(), Some(&[][..]));
    /// ```
    #[inline]
    pub const fn as_blob(&self) -> Option<&'stmt [u8]> {
        if let Kind::Blob(value) = self.kind {
            return Some(value);
        }

        None
    }

    /// Return the type of the value.
    #[inline]
    pub const fn ty(&self) -> Type {
        match &self.kind {
            Kind::Blob(_) => Type::BLOB,
            Kind::Float(_) => Type::FLOAT,
            Kind::Integer(_) => Type::INTEGER,
            Kind::Text(_) => Type::TEXT,
        }
    }
}

impl fmt::Debug for Value<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
