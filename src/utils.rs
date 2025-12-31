use core::ffi::CStr;

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
