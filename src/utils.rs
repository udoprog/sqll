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

macro_rules! __repeat {
    ($macro:path) => {
        $macro!(A a 0);
        $macro!(A a 0, B b 1);
        $macro!(A a 0, B b 1, C c 2);
        $macro!(A a 0, B b 1, C c 2, D d 3);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11, M m 12);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11, M m 12, N n 13);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11, M m 12, N n 13, O o 14);
    };
}

pub(crate) use __repeat as repeat;

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
