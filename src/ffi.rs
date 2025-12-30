pub(crate) use sqll_sys::*;

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
