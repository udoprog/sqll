use core::ffi::CStr;

/// Helper to evaluate sqlite3 statements.
macro_rules! __sqlite3_try {
    ($expr:expr) => {{
        let code = $expr;

        if code != $crate::ffi::SQLITE_OK {
            return Err($crate::error::Error::from_raw(code));
        }
    }};
}

pub(crate) use __sqlite3_try as sqlite3_try;

/// Coerce a null-terminated string into UTF-8, returning `None` if the pointer
/// is null.
pub(crate) unsafe fn c_to_str<'a>(ptr: *const i8) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }

    unsafe {
        let c_str = CStr::from_ptr(ptr);
        Some(str::from_utf8_unchecked(c_str.to_bytes()))
    }
}
