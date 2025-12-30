use core::alloc::Layout;
use core::ffi::{c_int, c_void};
use core::mem::{align_of, size_of};
use core::ptr::dangling_mut;

use crate::error::{Error, Result};
use crate::ffi;

#[cfg(test)]
mod tests;

type DeallocFn = unsafe extern "C" fn(*mut c_void);

pub(crate) extern "C" fn dealloc(p: *mut c_void) {
    if p == dangling_mut::<c_void>() {
        return;
    }

    // SAFETY: We are assuming the data was allocated using alloc_bytes, and abides by the same layout.
    unsafe {
        let p = p.cast::<u8>().wrapping_sub(size_of::<usize>());
        let len = p.cast::<usize>().read();

        // NB: We assume the layout is valid, since it is assumed to have been
        // allocated with alloc_bytes.
        let layout =
            Layout::from_size_align_unchecked(size_of::<usize>() + len, align_of::<usize>());

        alloc::alloc::dealloc(p.cast(), layout);
    }
}

pub(crate) fn alloc(bytes: &[u8]) -> Result<(*mut c_void, c_int, Option<DeallocFn>)> {
    if bytes.is_empty() {
        // Avoid allocating empty collections entirely by simply using a
        // dangling pointer. This is correctly aligned so it should be usable by
        // sqlite.
        return Ok((dangling_mut(), 0, None));
    }

    // SAFETY: We are receiving a valid byte slice.
    unsafe {
        let layout = Layout::from_size_align(size_of::<usize>() + bytes.len(), align_of::<usize>());

        let Ok(layout) = layout else {
            return Err(Error::new(ffi::SQLITE_NOMEM));
        };

        let ptr = alloc::alloc::alloc(layout);

        if ptr.is_null() {
            return Err(Error::new(ffi::SQLITE_NOMEM));
        }

        ptr.cast::<usize>().write(bytes.len());
        let data = ptr.add(size_of::<usize>());
        data.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
        let len = i32::try_from(bytes.len()).unwrap_or(i32::MAX);
        Ok((data.cast(), len, Some(dealloc)))
    }
}
