use core::alloc::Layout;
use core::cmp::Ordering;
use core::ffi::c_int;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::mem;
use core::ops::Deref;
use core::ptr::NonNull;
use core::slice;

use alloc::alloc::handle_alloc_error;

use crate::ffi;
use crate::{Code, Error, Result};

const MAX_CAP: usize = if mem::size_of::<isize>() > mem::size_of::<c_int>() {
    c_int::MAX as usize
} else {
    isize::MAX as usize
};

struct AllocError;

impl fmt::Display for AllocError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to allocate memory")
    }
}

/// A slice of bytes that has been allocated with the sqlite allocator.
///
/// This dereferences to a byte slice.
pub struct OwnedBytes {
    ptr: NonNull<u8>,
    len: usize,
    cap: usize,
}

impl OwnedBytes {
    /// Creates a new, empty `OwnedBytes`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OwnedBytes;
    ///
    /// let bytes = OwnedBytes::new();
    /// assert_eq!(bytes.len(), 0);
    /// assert!(bytes.is_empty());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
        }
    }

    /// Creates a new `OwnedBytes` with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OwnedBytes;
    ///
    /// let bytes = OwnedBytes::with_capacity(100)?;
    /// assert!(bytes.capacity() >= 100);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn with_capacity(cap: usize) -> Result<Self> {
        let mut this = Self::new();

        if let Err(error) = this.reserve(cap) {
            return Err(Error::new(Code::NOMEM, error));
        }

        Ok(this)
    }

    /// Returns the length of the byte slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OwnedBytes;
    ///
    /// let mut bytes = OwnedBytes::new();
    /// assert_eq!(bytes.len(), 0);
    ///
    /// bytes.extend_from_slice(b"hello ")?;
    /// assert_eq!(bytes.len(), 6);
    ///
    /// assert_eq!(bytes[..], b"hello "[..]);
    /// bytes.extend_from_slice(b"world")?;
    /// assert_eq!(bytes.len(), 11);
    /// assert_eq!(bytes[..], b"hello world"[..]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the byte slice is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OwnedBytes;
    ///
    /// let mut bytes = OwnedBytes::new();
    /// assert!(bytes.is_empty());
    ///
    /// bytes.extend_from_slice(b"hello world")?;
    /// assert!(!bytes.is_empty());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Creates a new `OwnedBytes` from the given slice, copying the provided
    /// data into it.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OwnedBytes;
    ///
    /// let mut bytes = OwnedBytes::new();
    /// bytes.extend_from_slice(b"hello world")?;
    /// assert_eq!(bytes[..], b"hello world"[..]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn extend_from_slice(&mut self, bytes: &[u8]) -> Result<()> {
        if let Err(error) = self.try_extend_from_slice(bytes) {
            return Err(Error::new(Code::NOMEM, error));
        }

        Ok(())
    }

    fn try_extend_from_slice(&mut self, bytes: &[u8]) -> Result<(), AllocError> {
        self.reserve(bytes.len())?;

        // SAFETY: We trust that the provided sqlite_* methods work as
        // advertised, and are abiding by the guarantees provided by the passed
        // in slice.
        unsafe {
            bytes
                .as_ptr()
                .copy_to_nonoverlapping(self.ptr.as_ptr().add(self.len), bytes.len());

            self.len += bytes.len();
            Ok(())
        }
    }

    /// Returns the capacity of the byte slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OwnedBytes;
    ///
    /// let bytes = OwnedBytes::with_capacity(100)?;
    /// assert!(bytes.capacity() >= 100);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Creates a new `OwnedBytes` from the given raw parts.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the provided pointer is initialized up to
    /// `len`, and that it has been constructed using the sqlite allocator.
    #[inline]
    pub(super) unsafe fn from_raw(ptr: NonNull<u8>, len: usize) -> Self {
        Self { ptr, len, cap: len }
    }

    /// Ensure that the buffer is backed by an allocation from the sqlite
    /// allocator, even if it is empty.
    ///
    /// An empty buffer otherwise holds a dangling pointer, which must never be
    /// handed to sqlite for it to free.
    pub(crate) fn ensure_allocated(&mut self) -> Result<()> {
        if self.cap == 0
            && let Err(error) = self.grow_to(1)
        {
            return Err(Error::new(Code::NOMEM, error));
        }

        Ok(())
    }

    fn reserve(&mut self, additional: usize) -> Result<(), AllocError> {
        let Some(needed) = self.len.checked_add(additional) else {
            return Err(AllocError);
        };

        if needed <= self.cap {
            return Ok(());
        }

        self.grow_to(needed)
    }

    /// Grow the allocation so that it can hold at least `needed` bytes, where
    /// `needed` is larger than the current capacity.
    fn grow_to(&mut self, needed: usize) -> Result<(), AllocError> {
        let Some(new_cap) = grown_capacity(needed) else {
            return Err(AllocError);
        };

        debug_assert!(new_cap > self.cap);

        // NB: `grown_capacity` bounds the capacity by `MAX_CAP`, which always
        // fits in a positive `c_int`. Passing a non-positive size to
        // `sqlite3_realloc` would free the allocation.
        let Ok(size) = c_int::try_from(new_cap) else {
            return Err(AllocError);
        };

        // SAFETY: The pointer is either unallocated (when `cap == 0`) or was
        // allocated by the sqlite allocator. If reallocation fails the old
        // allocation is left untouched, as documented for `sqlite3_realloc`.
        unsafe {
            let ptr = if self.cap == 0 {
                ffi::sqlite3_malloc(size)
            } else {
                ffi::sqlite3_realloc(self.ptr.as_ptr().cast(), size)
            };

            let Some(ptr) = NonNull::new(ptr) else {
                return Err(AllocError);
            };

            self.ptr = ptr.cast();
            self.cap = new_cap;
            Ok(())
        }
    }
}

/// Compute the capacity to grow to so that at least `needed` bytes fit.
///
/// This rounds up to the next power of two, but never past [`MAX_CAP`], and
/// returns `None` if `needed` itself exceeds it.
fn grown_capacity(needed: usize) -> Option<usize> {
    if needed > MAX_CAP {
        return None;
    }

    let cap = needed
        .max(16)
        .checked_next_power_of_two()
        .unwrap_or(MAX_CAP)
        .min(MAX_CAP);

    Some(cap)
}

impl Clone for OwnedBytes {
    #[inline]
    fn clone(&self) -> Self {
        let mut this = Self::new();

        if let Err(AllocError) = this.try_extend_from_slice(self) {
            let layout = unsafe { Layout::from_size_align_unchecked(self.len, 1) };
            handle_alloc_error(layout);
        }

        this
    }
}

impl fmt::Debug for OwnedBytes {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self[..].fmt(f)
    }
}

impl AsRef<[u8]> for OwnedBytes {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl Deref for OwnedBytes {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for OwnedBytes {
    #[inline]
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }

        // SAFETY: All ways we have to construct OwnedBytes require that it's
        // done through sqlite's allocator.
        unsafe {
            ffi::sqlite3_free(self.ptr.as_ptr().cast());
        }
    }
}

impl PartialEq for OwnedBytes {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self[..] == other[..]
    }
}

impl Eq for OwnedBytes {}

impl PartialOrd for OwnedBytes {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OwnedBytes {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self[..].cmp(&other[..])
    }
}

impl Hash for OwnedBytes {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self[..].hash(state);
    }
}

impl PartialEq<[u8]> for OwnedBytes {
    #[inline]
    fn eq(&self, other: &[u8]) -> bool {
        self[..] == other[..]
    }
}

impl PartialEq<OwnedBytes> for [u8] {
    #[inline]
    fn eq(&self, other: &OwnedBytes) -> bool {
        self[..] == other[..]
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CAP, OwnedBytes, grown_capacity};

    #[test]
    fn grown_capacity_is_bounded() {
        assert_eq!(grown_capacity(0), Some(16));
        assert_eq!(grown_capacity(17), Some(32));
        assert_eq!(grown_capacity(1 << 20), Some(1 << 20));
        assert_eq!(grown_capacity(MAX_CAP), Some(MAX_CAP));
        assert_eq!(grown_capacity(MAX_CAP + 1), None);
        assert_eq!(grown_capacity(usize::MAX), None);

        // Crossing 2^30 used to round up to 2^31, which does not fit in a
        // c_int and was passed to sqlite3_realloc as a negative size.
        assert_eq!(grown_capacity((1 << 30) + 1), Some(MAX_CAP));

        for needed in [0, 1, 16, 1000, 1 << 30, (1 << 30) + 1, MAX_CAP] {
            let cap = grown_capacity(needed).unwrap();
            assert!(cap >= needed);
            assert!(cap <= MAX_CAP);
            assert!(i32::try_from(cap).is_ok());
        }
    }

    #[test]
    fn failed_growth_keeps_buffer() {
        let mut bytes = OwnedBytes::new();
        bytes.extend_from_slice(b"hello").unwrap();
        let cap = bytes.capacity();

        // Requesting more than sqlite can allocate must fail without freeing
        // the existing allocation, which is then dropped normally.
        assert!(bytes.reserve((1 << 30) + 1).is_err());
        assert!(bytes.reserve(MAX_CAP).is_err());
        assert!(bytes.reserve(usize::MAX).is_err());

        assert_eq!(&bytes[..], b"hello");
        assert_eq!(bytes.capacity(), cap);

        bytes.extend_from_slice(b" world").unwrap();
        assert_eq!(&bytes[..], b"hello world");
    }

    #[test]
    fn extend_within_capacity_does_not_reallocate() {
        let mut bytes = OwnedBytes::with_capacity(64).unwrap();
        let ptr = bytes.as_ptr();
        let cap = bytes.capacity();

        for _ in 0..8 {
            bytes.extend_from_slice(b"12345678").unwrap();
        }

        assert_eq!(bytes.len(), 64);
        assert_eq!(bytes.capacity(), cap);
        assert_eq!(bytes.as_ptr(), ptr);
    }

    #[test]
    fn ensure_allocated() {
        let mut bytes = OwnedBytes::new();
        assert_eq!(bytes.capacity(), 0);
        bytes.ensure_allocated().unwrap();
        assert!(bytes.capacity() > 0);
        assert!(bytes.is_empty());
    }
}
