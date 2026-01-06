use core::fmt;

use crate::{Text, ValueType};

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

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Kind<'stmt> {
    Integer(i64),
    Float(f64),
    Text(&'stmt Text),
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
    /// use sqll::{Value, Text};
    ///
    /// let value = Value::text("");
    /// assert_eq!(value.as_text(), Some(Text::new("")));
    ///
    /// let value = Value::text("hello");
    /// assert_eq!(value.as_text(), Some(Text::new("hello")));
    /// ```
    #[inline]
    pub fn text<T>(value: &'stmt T) -> Self
    where
        T: ?Sized + AsRef<Text>,
    {
        Self {
            kind: Kind::Text(value.as_ref()),
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
    /// use sqll::{Value, Text};
    ///
    /// let value = Value::text("");
    /// assert_eq!(value.as_text(), Some(Text::new("")));
    ///
    /// let value = Value::text("hello");
    /// assert_eq!(value.as_text(), Some(Text::new("hello")));
    /// ```
    #[inline]
    pub const fn as_text(&self) -> Option<&'stmt Text> {
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

    /// Return the [`ValueType`] of the value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, ValueType, Value};
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (value);
    ///
    ///     INSERT INTO test (value) VALUES (42), (3.14), ('Hello, world!'), (X'DEADBEEF');
    /// "#)?;
    ///
    /// let mut select = c.prepare("SELECT value FROM test")?;
    ///
    /// let value = select.next::<Value<'_>>()?.map(|v| v.column_type());
    /// assert_eq!(value, Some(ValueType::INTEGER));
    ///
    /// let value = select.next::<Value<'_>>()?.map(|v| v.column_type());
    /// assert_eq!(value, Some(ValueType::FLOAT));
    ///
    /// let value = select.next::<Value<'_>>()?.map(|v| v.column_type());
    /// assert_eq!(value, Some(ValueType::TEXT));
    ///
    /// let value = select.next::<Value<'_>>()?.map(|v| v.column_type());
    /// assert_eq!(value, Some(ValueType::BLOB));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub const fn column_type(&self) -> ValueType {
        match &self.kind {
            Kind::Blob(_) => ValueType::BLOB,
            Kind::Float(_) => ValueType::FLOAT,
            Kind::Integer(_) => ValueType::INTEGER,
            Kind::Text(_) => ValueType::TEXT,
        }
    }
}

/// Debug implementation for [`Value`].
///
/// # Examples
///
/// ```
/// use sqll::Value;
///
/// let value = Value::integer(42);
/// assert_eq!(format!("{:?}", value), "42");
///
/// let value = Value::float(3.14);
/// assert_eq!(format!("{:?}", value), "3.14");
///
/// let value = Value::text("hello");
/// assert_eq!(format!("{:?}", value), "\"hello\"");
///
/// let value = Value::blob(&[0xDE, 0xAD, 0xBE, 0xEF]);
/// assert_eq!(format!("{:?}", value), "b\"\\xde\\xad\\xbe\\xef\"");
/// ```
impl fmt::Debug for Value<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            Kind::Integer(value) => write!(f, "{value}"),
            Kind::Float(value) => write!(f, "{value}"),
            Kind::Text(value) => write!(f, "{value:?}"),
            Kind::Blob(value) => {
                write!(f, "b\"")?;

                for byte in value {
                    write!(f, "\\x{:02x}", byte)?;
                }

                write!(f, "\"")
            }
        }
    }
}
