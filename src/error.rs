use core::error;
use core::fmt;

use alloc::format;
use alloc::string::String;

use crate::Code;

/// A result type alias.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// An error.
#[derive(PartialEq, Eq)]
pub struct Error {
    /// Error code.
    code: Code,
    message: String,
}

impl Error {
    /// Construct a new error from the specified code and message.
    #[inline]
    pub fn new(code: Code, message: impl fmt::Display) -> Self {
        Self {
            code,
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
        st.field("message", &self.message);
        st.finish()
    }
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.message.fmt(f)
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
