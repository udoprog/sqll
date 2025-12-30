use core::ffi::CStr;
use core::ffi::c_char;

#[cfg(feature = "std")]
use std::path::Path;

use sqlite3_sys as ffi;

use crate::error::Result;

/// Helper to run sqlite3 statement.
macro_rules! __sqlite3_try {
    ($c:expr, $expr:expr) => {
        if $expr != ::sqlite3_sys::SQLITE_OK {
            let code = ::sqlite3_sys::sqlite3_errcode($c);
            return Err($crate::error::Error::new(code));
        }
    };
}

pub(crate) use __sqlite3_try as sqlite3_try;

/// Convert a c-string into a rust string.
pub(crate) unsafe fn cstr_to_str<'a>(s: *const c_char) -> Result<&'a str> {
    unsafe {
        match CStr::from_ptr(s).to_str() {
            Ok(s) => Ok(s),
            Err(..) => Err(crate::error::Error::new(ffi::SQLITE_MISUSE)),
        }
    }
}

#[cfg(feature = "std")]
#[cfg(unix)]
pub(crate) fn path_to_cstring(p: &Path) -> Result<CString> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let p: &OsStr = p.as_ref();

    match CString::new(p.as_bytes()) {
        Ok(string) => Ok(string),
        Err(..) => Err(crate::error::Error::new(ffi::SQLITE_MISUSE)),
    }
}

#[cfg(feature = "std")]
#[cfg(not(unix))]
pub(crate) fn path_to_cstring(p: &Path) -> Result<CString> {
    let s = match p.to_str() {
        Some(s) => s,
        None => return Err(crate::error::Error::new(ffi::SQLITE_MISUSE)),
    };

    match CString::new(s.as_bytes()) {
        Ok(string) => Ok(string),
        Err(..) => Err(crate::error::Error::new(ffi::SQLITE_MISUSE)),
    }
}
