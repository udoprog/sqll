use core::ffi::{CStr, c_int};
use core::str;

use crate::ffi;

/// Return the version string of the SQLite library in use.
///
/// This may return a version string like `"3.51.1"`.
///
/// # Examples
///
/// ```
/// assert!(sqll::lib_version().starts_with("3."));
/// assert_eq!(sqll::lib_version(), "3.51.1");
/// ```
#[inline]
pub fn lib_version() -> &'static str {
    unsafe {
        let c_str = ffi::sqlite3_libversion();
        let bytes = CStr::from_ptr(c_str).to_bytes();
        str::from_utf8_unchecked(bytes)
    }
}

/// Return the version number of the SQLite library in use.
///
/// The version `3.8.11.1` would correspond to the integer `3008011`.
///
/// # Examples
///
/// ```
/// assert_eq!(sqll::lib_version_number() / 1000000, 3);
/// ```
#[inline]
pub fn lib_version_number() -> c_int {
    unsafe { ffi::sqlite3_libversion_number() }
}
