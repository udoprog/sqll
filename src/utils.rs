#[cfg(feature = "std")]
use alloc::ffi::CString;

#[cfg(feature = "std")]
use std::path::Path;

#[cfg(feature = "std")]
use sqlite3_sys as ffi;

#[cfg(feature = "std")]
use crate::error::{Error, Result};

/// Helper to evaluate sqlite3 statements.
macro_rules! __sqlite3_try {
    ($expr:expr) => {{
        let code = $expr;

        if code != ::sqlite3_sys::SQLITE_OK {
            return Err($crate::error::Error::new(code));
        }
    }};
}

pub(crate) use __sqlite3_try as sqlite3_try;

#[cfg(feature = "std")]
pub(crate) fn path_to_cstring(p: &Path) -> Result<CString> {
    let Some(bytes) = p.to_str() else {
        return Err(Error::new(ffi::SQLITE_MISUSE));
    };

    match CString::new(bytes) {
        Ok(string) => Ok(string),
        Err(..) => Err(Error::new(ffi::SQLITE_MISUSE)),
    }
}
