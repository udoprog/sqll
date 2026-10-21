use core::ffi::CStr;

use crate::Text;

/// Helper to evaluate sqlite3 statements.
macro_rules! __sqlite3_try {
    ($db:expr, $expr:expr) => {{
        let code = $expr;

        if code != $crate::ffi::SQLITE_OK {
            return Err($crate::Error::new(
                $crate::Code::new(code),
                $db.error_message(),
            ));
        }
    }};
}

pub(crate) use __sqlite3_try as sqlite3_try;

macro_rules! __repeat {
    ($macro:path) => {
        $macro!(1, A a 0 1);
        $macro!(2, A a 0 1, B b 1 2);
        $macro!(3, A a 0 1, B b 1 2, C c 2 3);
        $macro!(4, A a 0 1, B b 1 2, C c 2 3, D d 3 4);
        $macro!(5, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5);
        $macro!(6, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6);
        $macro!(7, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7);
        $macro!(8, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8);
        $macro!(9, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9);
        $macro!(10, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10);
        $macro!(11, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11);
        $macro!(12, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12);
        $macro!(13, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13);
        $macro!(14, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14);
        $macro!(15, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15);
        $macro!(16, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16);
        $macro!(17, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17);
        $macro!(18, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18);
        $macro!(19, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19);
        $macro!(20, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20);
        $macro!(21, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21);
        $macro!(22, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22);
        $macro!(23, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23);
        $macro!(24, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24);
        $macro!(25, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25);
        $macro!(26, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25, Z z 25 26);
        $macro!(27, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25, Z z 25 26, AA aa 26 27);
        $macro!(28, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25, Z z 25 26, AA aa 26 27, AB ab 27 28);
        $macro!(29, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25, Z z 25 26, AA aa 26 27, AB ab 27 28, AC ac 28 29);
        $macro!(30, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25, Z z 25 26, AA aa 26 27, AB ab 27 28, AC ac 28 29, AD ad 29 30);
        $macro!(31, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25, Z z 25 26, AA aa 26 27, AB ab 27 28, AC ac 28 29, AD ad 29 30, AE ae 30 31);
        $macro!(32, A a 0 1, B b 1 2, C c 2 3, D d 3 4, E e 4 5, F f 5 6, G g 6 7, H h 7 8, I i 8 9, J j 9 10, K k 10 11, L l 11 12, M m 12 13, N n 13 14, O o 14 15, P p 15 16, Q q 16 17, R r 17 18, S s 18 19, T t 19 20, U u 20 21, V v 21 22, W w 22 23, X x 23 24, Y y 24 25, Z z 25 26, AA aa 26 27, AB ab 27 28, AC ac 28 29, AD ad 29 30, AE ae 30 31, AF af 31 32);
    };
}

/// Repeat a macro to build a tuple implementation.
///
/// This supports up to 32 elements.
pub(crate) use __repeat as repeat;

/// Coerce a null-terminated string into UTF-8, returning `None` if the pointer
/// is null.
pub(crate) unsafe fn c_to_text<'a>(ptr: *const i8) -> Option<&'a Text> {
    if ptr.is_null() {
        return None;
    }

    // SAFETY: The one assumption we are allowed to make about strings is that
    // they are null-terminated.
    let c_str = unsafe { CStr::from_ptr(ptr) };
    Some(Text::new(c_str.to_bytes()))
}

pub(crate) unsafe fn c_to_error_text(ptr: *const i8) -> &'static Text {
    // NB: This is the same message as set by sqlite.
    static DEFAULT_MESSAGE: &Text = Text::from_bytes(b"not an error");
    unsafe { c_to_text(ptr).unwrap_or(DEFAULT_MESSAGE) }
}
