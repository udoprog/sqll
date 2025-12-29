use core::fmt;

use alloc::string::String;
use alloc::vec::Vec;

/// The type of a value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Type {
    Blob,
    Text,
    Float,
    Integer,
    Null,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    Null,
    Blob(Vec<u8>),
    Text(String),
    Float(f64),
    Integer(i64),
}

/// A dynamic value.
#[derive(Clone, PartialEq)]
pub struct Value {
    pub(crate) kind: Kind,
}

impl Value {
    /// Construct a null value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
    ///
    /// let value = Value::null();
    /// assert!(value.is_null());
    /// ```
    #[inline]
    pub const fn null() -> Self {
        Self { kind: Kind::Null }
    }

    /// Construct a blob value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
    ///
    /// let value = Value::blob(Vec::new());
    /// assert_eq!(value.as_blob(), Some(&[][..]));
    /// ```
    #[inline]
    pub const fn blob(value: Vec<u8>) -> Self {
        Self {
            kind: Kind::Blob(value),
        }
    }

    /// Construct a text value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
    ///
    /// let value = Value::text(String::new());
    /// assert_eq!(value.as_text(), Some(""));
    ///
    /// let value = Value::text(String::from("hello"));
    /// assert_eq!(value.as_text(), Some("hello"));
    /// ```
    #[inline]
    pub const fn text(value: String) -> Self {
        Self {
            kind: Kind::Text(value),
        }
    }

    /// Construct a float value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
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

    /// Construct a integer value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
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

    /// Return whether the value is `Null`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
    ///
    /// let value = Value::null();
    /// assert!(value.is_null());
    /// ```
    #[inline]
    pub const fn is_null(&self) -> bool {
        matches!(self.kind, Kind::Null)
    }

    /// Return the binary data if the value is `Binary`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
    ///
    /// let value = Value::blob(Vec::new());
    /// assert_eq!(value.as_blob(), Some(&[][..]));
    /// ```
    #[inline]
    pub const fn as_blob(&self) -> Option<&[u8]> {
        if let Kind::Blob(value) = &self.kind {
            return Some(value.as_slice());
        }

        None
    }

    /// Return the string if the value is `String`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
    ///
    /// let value = Value::text(String::new());
    /// assert_eq!(value.as_text(), Some(""));
    ///
    /// let value = Value::text(String::from("hello"));
    /// assert_eq!(value.as_text(), Some("hello"));
    /// ```
    #[inline]
    pub const fn as_text(&self) -> Option<&str> {
        if let Kind::Text(value) = &self.kind {
            return Some(value.as_str());
        }

        None
    }

    /// Return the floating-point number if the value is `Float`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
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

    /// Return the integer number if the value is `Integer`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Value;
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

    /// Return the type.
    #[inline]
    pub const fn kind(&self) -> Type {
        match &self.kind {
            Kind::Blob(_) => Type::Blob,
            Kind::Float(_) => Type::Float,
            Kind::Integer(_) => Type::Integer,
            Kind::Text(_) => Type::Text,
            Kind::Null => Type::Null,
        }
    }
}

impl fmt::Debug for Value {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
