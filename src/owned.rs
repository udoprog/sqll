use core::alloc::Layout;
use core::mem;
use core::ptr::{self, NonNull};

use alloc::alloc;

use crate::{Code, Error, Result};

/// An owned pointer with drop glue.
///
/// This is used internally to store opaque types.
pub(crate) struct Owned {
    ptr: NonNull<()>,
    drop: unsafe fn(NonNull<()>),
}

impl Owned {
    pub(crate) fn new<T>(value: T) -> Result<Self> {
        let layout = Layout::new::<T>();

        if layout.size() == 0 {
            // Zero-sized values need no storage, but they may still have drop
            // glue, which runs when this is dropped.
            mem::forget(value);

            return Ok(Self {
                ptr: NonNull::<T>::dangling().cast(),
                drop: zero_sized_drop_glue::<T>,
            });
        }

        let ptr = unsafe {
            let ptr = alloc::alloc(layout);

            if ptr.is_null() {
                return Err(Error::new(Code::NOMEM, "allocation failed"));
            }

            ptr.cast::<T>().write(value);
            NonNull::new_unchecked(ptr.cast())
        };

        Ok(Self {
            ptr,
            drop: drop_glue::<T>,
        })
    }

    #[inline]
    pub(crate) fn as_ptr(&self) -> *mut () {
        self.ptr.as_ptr()
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: The busy callback is constructed in one go.
        unsafe {
            (self.drop)(self.ptr);
        }
    }
}

/// # Safety
///
/// `ptr` must point to an initialized `T` allocated with the global allocator
/// using `Layout::new::<T>()`, and must not be used afterwards.
unsafe fn drop_glue<T>(ptr: NonNull<()>) {
    unsafe {
        let ptr = ptr.cast::<T>().as_ptr();
        ptr::drop_in_place(ptr);
        alloc::dealloc(ptr.cast(), Layout::new::<T>());
    }
}

/// # Safety
///
/// `ptr` must be a well-aligned dangling pointer standing in for a forgotten
/// zero-sized `T` which must not be used afterwards.
unsafe fn zero_sized_drop_glue<T>(ptr: NonNull<()>) {
    unsafe {
        ptr::drop_in_place(ptr.cast::<T>().as_ptr());
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use alloc::sync::Arc;

    use super::Owned;

    struct Counted(Arc<AtomicUsize>);

    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn drops_sized_value() {
        let drops = Arc::new(AtomicUsize::new(0));
        let owned = Owned::new(Counted(drops.clone())).unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(owned);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(Arc::strong_count(&drops), 1);
    }

    #[test]
    fn drops_zero_sized_value() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        struct Zst;

        impl Drop for Zst {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let owned = Owned::new(Zst).unwrap();
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        drop(owned);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }
}
