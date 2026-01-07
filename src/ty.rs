//! Module used which provides marker types for use with the `Type` associated
//! type in [`FromColumn`] or [`FromUnsizedColumn`].
//!
//! These marker types determine what the supported column type is used when
//! reading a particular value.
//!
//! [`FromColumn`]: crate::FromColumn
//! [`FromUnsizedColumn`]: crate::FromUnsizedColumn
//!
//! # Examples
//!
//! ```
//! use sqll::{Connection, FromColumn, Result, Statement};
//! use sqll::ty;
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct Timestamp {
//!     seconds: i64,
//! }
//!
//! impl FromColumn<'_> for Timestamp {
//!     type Type = ty::Nullable<ty::Integer>;
//!
//!     #[inline]
//!     fn from_column(stmt: &Statement, index: ty::Nullable<ty::Integer>) -> Result<Self> {
//!         Ok(Timestamp {
//!             seconds: Option::<i64>::from_column(stmt, index)?.unwrap_or(i64::MIN),
//!         })
//!     }
//! }
//!
//! let c = Connection::open_in_memory()?;
//!
//! c.execute(r#"
//!     CREATE TABLE test (ts INTEGER);
//!
//!     INSERT INTO test (ts) VALUES (1767675413), (NULL);
//! "#)?;
//!
//! let mut stmt = c.prepare("SELECT ts FROM test")?;
//!
//! assert_eq!(stmt.next::<Timestamp>()?, Some(Timestamp { seconds: 1767675413 }));
//! assert_eq!(stmt.next::<Timestamp>()?, Some(Timestamp { seconds: i64::MIN }));
//! # Ok::<_, sqll::Error>(())
//! ```

mod not_null;
mod ty;

#[doc(inline)]
pub use self::not_null::NotNull;
pub(crate) use self::ty::AnyKind;
#[doc(inline)]
pub use self::ty::{Any, Blob, Float, Integer, Nullable, Text, Type};
