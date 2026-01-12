use core::error;
use core::fmt;

#[cfg(feature = "alloc")]
use alloc::format;
#[cfg(feature = "alloc")]
use alloc::string::String;

use crate::Code;

/// A result type alias.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// An error.
#[derive(PartialEq, Eq)]
pub struct Error {
    /// Error code.
    code: Code,
    #[cfg(feature = "alloc")]
    message: String,
}

impl Error {
    /// Construct a new error from the specified code and message.
    #[inline]
    pub fn new(
        code: Code,
        #[cfg_attr(not(feature = "alloc"), allow(unused_variables))] message: impl fmt::Display,
    ) -> Self {
        Self {
            code,
            #[cfg(feature = "alloc")]
            message: format!("{message}"),
        }
    }

    /// Construct a new generic error with the specified message.
    #[inline]
    pub fn custom(message: impl fmt::Display) -> Self {
        Self::new(Code::ERROR, message)
    }

    /// The error code that caused this error.
    #[inline]
    pub fn code(&self) -> Code {
        self.code
    }
}

impl fmt::Debug for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut st = f.debug_struct("Error");
        st.field("code", &self.code);
        #[cfg(feature = "alloc")]
        st.field("message", &self.message);
        #[cfg(not(feature = "alloc"))]
        st.field("message", &self.code.message());
        st.finish()
    }
}

#[cfg(feature = "alloc")]
impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.message.fmt(f)
    }
}

#[cfg(not(feature = "alloc"))]
impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.code.message().fmt(f)
    }
}

impl error::Error for Error {}

/// Indicates that a database was not found.
#[derive(Debug)]
#[non_exhaustive]
pub struct DatabaseNotFound;

impl fmt::Display for DatabaseNotFound {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "database not found")
    }
}

impl core::error::Error for DatabaseNotFound {}

/// Error raised when attempting to convert a database object into a thread-safe
/// container, but the database is not configured to be thread-safe.
pub struct NotThreadSafe {
    kind: NotThreadSafeKind,
}

impl NotThreadSafe {
    /// Construct a new not thread safe error for a connection.
    #[inline]
    pub(super) const fn connection() -> Self {
        Self {
            kind: NotThreadSafeKind::Connection,
        }
    }

    /// Construct a new not thread safe error for a statement.
    #[inline]
    pub(super) const fn statement() -> Self {
        Self {
            kind: NotThreadSafeKind::Statement,
        }
    }
}

impl fmt::Debug for NotThreadSafe {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Display for NotThreadSafe {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a {} object is not thread safe", self.kind)
    }
}

impl core::error::Error for NotThreadSafe {}

#[derive(Debug)]
enum NotThreadSafeKind {
    Connection,
    Statement,
}

impl fmt::Display for NotThreadSafeKind {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotThreadSafeKind::Connection => write!(f, "connection"),
            NotThreadSafeKind::Statement => write!(f, "statement"),
        }
    }
}

/// Error raised when failing to convert a string into a `FixedBlob`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let e = FixedBlob::<3>::try_from(&b"abcd"[..]).unwrap_err();
/// assert_eq!(e.to_string(), "size 4 exceeds fixed buffer size 3");
/// ```
pub struct CapacityError {
    kind: CapacityErrorKind,
}

#[derive(Debug)]
enum CapacityErrorKind {
    Capacity { len: usize, max: usize },
}

impl CapacityError {
    /// Construct a new capacity error.
    #[inline]
    pub(super) fn capacity(len: usize, max: usize) -> Self {
        Self {
            kind: CapacityErrorKind::Capacity { len, max },
        }
    }
}

impl fmt::Display for CapacityError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            CapacityErrorKind::Capacity { len, max } => {
                write!(f, "size {len} exceeds fixed buffer size {max}")
            }
        }
    }
}

impl fmt::Debug for CapacityError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl core::error::Error for CapacityError {}
