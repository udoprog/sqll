//! Module used to statically define a column type in combination with a
//! [`FromColumn`] or [`FromUnsizedColumn`] implementation.
//!
//! [`FromColumn`]: crate::FromColumn
//! [`FromUnsizedColumn`]: crate::FromUnsizedColumn

mod not_null;
mod ty;

#[doc(inline)]
pub use self::not_null::NotNull;
pub(crate) use self::ty::AnyKind;
#[doc(inline)]
pub use self::ty::{Any, Blob, Float, Integer, Nullable, Text, Type};
