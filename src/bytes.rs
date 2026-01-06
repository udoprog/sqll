use core::ffi::{c_int, c_void};
use core::ptr::{copy_nonoverlapping, dangling_mut};

use crate::ffi;
use crate::{Code, Error, Result};

#[cfg(test)]
mod tests;

type DeallocFn = unsafe extern "C" fn(*mut c_void);

pub(crate) fn alloc(bytes: &[u8]) -> Result<(*mut c_void, c_int, Option<DeallocFn>)> {
    if bytes.is_empty() {
        // Avoid allocating empty collections entirely by simply using a
        // dangling pointer. This is correctly aligned so it should be usable by
        // sqlite.
        return Ok((dangling_mut(), 0, None));
    }

    // SAFETY: We are receiving a valid byte slice.
    unsafe {
        let Ok(n) = c_int::try_from(bytes.len()) else {
            return Err(Error::new(
                Code::ERROR,
                format_args!("allocation size {} exceeds addressable memory", bytes.len()),
            ));
        };

        let ptr = ffi::sqlite3_malloc(n);

        if ptr.is_null() {
            return Err(Error::new(Code::NOMEM, "allocation failed"));
        }

        copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len());
        Ok((ptr, n, Some(ffi::sqlite3_free)))
    }
}
