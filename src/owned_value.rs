use alloc::string::String;
use alloc::vec::Vec;

use crate::ty;
use crate::value::Kind;
use crate::{
    Bind, BindValue, Code, Error, FromColumn, Index, Null, Result, Statement, Value, ValueType,
};

/// An owned dynamic value, which unlike [`Value`] can also be `NULL`.
///
/// This implements [`BindValue`] and [`FromColumn`], so it can be used to
/// collect parameters of different types while composing SQL at runtime and
/// bind them with [`Statement::bind_iter`], or to read a column of any type
/// including `NULL`.
///
/// Text is stored as a [`String`], so reading text which is not valid UTF-8
/// into an `OwnedValue` errors, like reading it into a [`String`] does.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, OwnedValue};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (value);
/// "#)?;
///
/// let values = [
///     OwnedValue::Null,
///     OwnedValue::Integer(42),
///     OwnedValue::Float(3.14),
///     OwnedValue::Text(String::from("Hello, world!")),
///     OwnedValue::Blob(vec![0xDE, 0xAD, 0xBE, 0xEF]),
/// ];
///
/// let mut insert = c.prepare("INSERT INTO test (value) VALUES (?)")?;
///
/// for value in &values {
///     insert.execute(value)?;
/// }
///
/// let mut select = c.prepare("SELECT value FROM test ORDER BY rowid")?;
/// let read = select.iter::<OwnedValue>().collect::<Result<Vec<_>, _>>()?;
/// assert_eq!(read, values);
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum OwnedValue {
    /// A `NULL` value.
    Null,
    /// An integer value.
    Integer(i64),
    /// A floating-point value.
    Float(f64),
    /// A text value.
    Text(String),
    /// A blob value.
    Blob(Vec<u8>),
}

impl OwnedValue {
    /// Test if the value is `NULL`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OwnedValue;
    ///
    /// assert!(OwnedValue::Null.is_null());
    /// assert!(!OwnedValue::Integer(42).is_null());
    /// ```
    #[inline]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Borrow this value as a [`Value`], or `None` if it is `NULL`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OwnedValue, Value};
    ///
    /// let value = OwnedValue::from("hello");
    /// assert_eq!(value.as_value(), Some(Value::text("hello")));
    /// assert_eq!(OwnedValue::Null.as_value(), None);
    /// ```
    #[inline]
    pub fn as_value(&self) -> Option<Value<'_>> {
        match self {
            Self::Null => None,
            Self::Integer(value) => Some(Value::integer(*value)),
            Self::Float(value) => Some(Value::float(*value)),
            Self::Text(value) => Some(Value::text(value.as_str())),
            Self::Blob(value) => Some(Value::blob(value)),
        }
    }

    /// Return the [`ValueType`] of the value.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OwnedValue, ValueType};
    ///
    /// assert_eq!(OwnedValue::Null.column_type(), ValueType::NULL);
    /// assert_eq!(OwnedValue::Integer(42).column_type(), ValueType::INTEGER);
    /// assert_eq!(OwnedValue::Float(4.2).column_type(), ValueType::FLOAT);
    /// assert_eq!(OwnedValue::from("hello").column_type(), ValueType::TEXT);
    /// assert_eq!(OwnedValue::from(&b"hello"[..]).column_type(), ValueType::BLOB);
    /// ```
    #[inline]
    pub const fn column_type(&self) -> ValueType {
        match self {
            Self::Null => ValueType::NULL,
            Self::Integer(_) => ValueType::INTEGER,
            Self::Float(_) => ValueType::FLOAT,
            Self::Text(_) => ValueType::TEXT,
            Self::Blob(_) => ValueType::BLOB,
        }
    }
}

/// [`BindValue`] implementation for [`OwnedValue`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Index, OwnedValue};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
///
/// stmt.bind_value(Index::BIND, OwnedValue::Null)?;
/// assert_eq!(stmt.iter::<String>().collect::<Vec<_>>(), [Ok(String::from("Alice"))]);
///
/// stmt.reset()?;
/// stmt.bind_value(Index::BIND, OwnedValue::Integer(30))?;
/// assert_eq!(stmt.iter::<String>().collect::<Vec<_>>(), [Ok(String::from("Bob"))]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl BindValue for OwnedValue {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: Index) -> Result<()> {
        match self {
            Self::Null => Null.bind_value(stmt, index),
            Self::Integer(value) => value.bind_value(stmt, index),
            Self::Float(value) => value.bind_value(stmt, index),
            Self::Text(value) => value.bind_value(stmt, index),
            Self::Blob(value) => value.bind_value(stmt, index),
        }
    }
}

/// [`Bind`] implementation for [`OwnedValue`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, OwnedValue};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users WHERE age IS ?")?;
/// stmt.bind(OwnedValue::Null)?;
/// assert_eq!(stmt.iter::<String>().collect::<Vec<_>>(), [Ok(String::from("Alice"))]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl Bind for OwnedValue {
    const COUNT: usize = 1;

    #[inline]
    fn bind(&self, stmt: &mut Statement) -> Result<()> {
        self.bind_value(stmt, Index::BIND)
    }
}

/// [`FromColumn`] implementation for [`OwnedValue`].
///
/// Reads a column of any type, including `NULL`.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, OwnedValue};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name, age FROM users ORDER BY name")?;
///
/// assert_eq!(
///     stmt.next::<(OwnedValue, OwnedValue)>()?,
///     Some((OwnedValue::from("Alice"), OwnedValue::Null))
/// );
///
/// assert_eq!(
///     stmt.next::<(OwnedValue, OwnedValue)>()?,
///     Some((OwnedValue::from("Bob"), OwnedValue::Integer(30)))
/// );
///
/// assert_eq!(stmt.next::<(OwnedValue, OwnedValue)>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for OwnedValue {
    type Type = ty::Nullable<ty::Any>;

    #[inline]
    fn from_column(stmt: &'stmt Statement, index: Self::Type) -> Result<Self> {
        let Some(value) = Option::<Value<'stmt>>::from_column(stmt, index)? else {
            return Ok(Self::Null);
        };

        Ok(match *value.kind() {
            Kind::Integer(value) => Self::Integer(value),
            Kind::Float(value) => Self::Float(value),
            Kind::Text(value) => {
                let Ok(string) = value.to_str() else {
                    return Err(Error::new(Code::MISMATCH, "column is not valid UTF-8"));
                };

                Self::Text(String::from(string))
            }
            Kind::Blob(value) => Self::Blob(Vec::from(value)),
        })
    }
}

/// Convert a [`Null`] into [`OwnedValue::Null`].
///
/// # Examples
///
/// ```
/// use sqll::{Null, OwnedValue};
///
/// assert_eq!(OwnedValue::from(Null), OwnedValue::Null);
/// ```
impl From<Null> for OwnedValue {
    #[inline]
    fn from(_: Null) -> Self {
        Self::Null
    }
}

/// Convert an [`Option`] into an [`OwnedValue`], where `None` is
/// [`OwnedValue::Null`].
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from(Some(42i64)), OwnedValue::Integer(42));
/// assert_eq!(OwnedValue::from(None::<i64>), OwnedValue::Null);
/// ```
impl<T> From<Option<T>> for OwnedValue
where
    OwnedValue: From<T>,
{
    #[inline]
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::from(value),
            None => Self::Null,
        }
    }
}

macro_rules! from_integer {
    ($($ty:ty),* $(,)?) => {
        $(
            #[doc = concat!("Convert an [`", stringify!($ty), "`] into [`OwnedValue::Integer`].")]
            ///
            /// # Examples
            ///
            /// ```
            /// use sqll::OwnedValue;
            ///
            #[doc = concat!("assert_eq!(OwnedValue::from(42", stringify!($ty), "), OwnedValue::Integer(42));")]
            /// ```
            impl From<$ty> for OwnedValue {
                #[inline]
                fn from(value: $ty) -> Self {
                    Self::Integer(i64::from(value))
                }
            }
        )*
    };
}

from_integer!(i8, i16, i32, i64, u8, u16, u32);

/// Convert a [`bool`] into [`OwnedValue::Integer`], `1` for `true` and `0`
/// for `false`.
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from(true), OwnedValue::Integer(1));
/// assert_eq!(OwnedValue::from(false), OwnedValue::Integer(0));
/// ```
impl From<bool> for OwnedValue {
    #[inline]
    fn from(value: bool) -> Self {
        Self::Integer(i64::from(value))
    }
}

/// Convert an [`f64`] into [`OwnedValue::Float`].
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from(4.2f64), OwnedValue::Float(4.2));
/// ```
impl From<f64> for OwnedValue {
    #[inline]
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// Convert an [`f32`] into [`OwnedValue::Float`].
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from(0.5f32), OwnedValue::Float(0.5));
/// ```
impl From<f32> for OwnedValue {
    #[inline]
    fn from(value: f32) -> Self {
        Self::Float(f64::from(value))
    }
}

/// Convert a [`String`] into [`OwnedValue::Text`].
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from(String::from("hello")), OwnedValue::Text(String::from("hello")));
/// ```
impl From<String> for OwnedValue {
    #[inline]
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

/// Convert a [`str`] into [`OwnedValue::Text`].
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from("hello"), OwnedValue::Text(String::from("hello")));
/// ```
impl From<&str> for OwnedValue {
    #[inline]
    fn from(value: &str) -> Self {
        Self::Text(String::from(value))
    }
}

/// Convert a [`Vec<u8>`] into [`OwnedValue::Blob`].
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from(vec![1u8, 2, 3]), OwnedValue::Blob(vec![1, 2, 3]));
/// ```
impl From<Vec<u8>> for OwnedValue {
    #[inline]
    fn from(value: Vec<u8>) -> Self {
        Self::Blob(value)
    }
}

/// Convert a byte slice into [`OwnedValue::Blob`].
///
/// # Examples
///
/// ```
/// use sqll::OwnedValue;
///
/// assert_eq!(OwnedValue::from(&b"abc"[..]), OwnedValue::Blob(b"abc".to_vec()));
/// ```
impl From<&[u8]> for OwnedValue {
    #[inline]
    fn from(value: &[u8]) -> Self {
        Self::Blob(Vec::from(value))
    }
}
