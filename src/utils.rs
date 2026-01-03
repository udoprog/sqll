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
        $macro!(A a 0 1);
        $macro!(A a 0 1, B b 1 2);
        $macro!(A a 0 1, B b 1 2, C c 2 3);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14);
        $macro!(A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15);
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
