use core::error::Error;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::ptr;
use core::slice;

/// A helper to read at most a fixed number of `N` bytes from a column. This
/// allocates the storage for the bytes read on the stack.
pub struct FixedBytes<const N: usize> {
    /// Storage to read to.
    data: [MaybeUninit<u8>; N],
    /// Number of bytes initialized.
    init: usize,
}

impl<const N: usize> FixedBytes<N> {
    /// Construct a new empty [`FixedBytes`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::FixedBytes;
    ///
    /// let s = FixedBytes::<5>::new();
    /// assert_eq!(s.as_slice(), &[]);
    /// ```
    pub const fn new() -> Self {
        Self {
            // SAFETY: this is safe as per `MaybeUninit::uninit_array`, which isn't stable (yet).
            data: unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() },
            init: 0,
        }
    }

    /// Return a mutable pointer to the underlying bytes.
    pub(super) fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr().cast()
    }

    /// Set the number of initialized bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `len` does not exceed `N` and that at least
    /// `len` bytes have been initialized.
    pub(super) unsafe fn set_len(&mut self, len: usize) {
        self.init = len;
    }

    /// Coerce into the underlying bytes if all of them have been initialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FixedBytes};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (id BLOB);
    ///
    ///     INSERT INTO users (id) VALUES (X'01020304'), (X'05060708');
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM users")?;
    ///
    /// while stmt.step()?.is_row() {
    ///     let bytes = stmt.get::<FixedBytes<4>>(0)?;
    ///     assert!(matches!(bytes.into_bytes(), Some([1, 2, 3, 4] | [5, 6, 7, 8])));
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn into_bytes(self) -> Option<[u8; N]> {
        if self.init == N {
            // SAFETY: All of the bytes in the sequence have been initialized
            // and can be safety transmuted.
            //
            // Method of transmuting comes from the implementation of
            // `MaybeUninit::array_assume_init` which is not yet stable.
            unsafe { Some((&self.data as *const _ as *const [u8; N]).read()) }
        } else {
            None
        }
    }

    /// Coerce into the slice of initialized memory which is present.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FixedBytes};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (id BLOB);
    ///
    ///     INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM users")?;
    ///
    /// while stmt.step()?.is_row() {
    ///     let bytes = stmt.get::<FixedBytes<10>>(0)?;
    ///     assert!(matches!(bytes.as_slice(), &[1, 2, 3, 4] | &[5, 6, 7, 8, 9]));
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn as_slice(&self) -> &[u8] {
        if self.init == 0 {
            return &[];
        }

        // SAFETY: We've asserted that `initialized` accounts for the number of
        // bytes that have been initialized.
        unsafe { slice::from_raw_parts(self.data.as_ptr() as *const u8, self.init) }
    }
}

impl<const N: usize> Deref for FixedBytes<N> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<const N: usize> fmt::Debug for FixedBytes<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<const N: usize> AsRef<[u8]> for FixedBytes<N> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl<const N: usize> PartialEq for FixedBytes<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<const N: usize> Eq for FixedBytes<N> {}

impl<const N: usize> PartialOrd for FixedBytes<N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for FixedBytes<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl<const N: usize> Hash for FixedBytes<N> {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.as_slice().hash(state)
    }
}

impl<const N: usize> Clone for FixedBytes<N> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            init: self.init,
            data: self.data,
        }
    }
}

/// Error raised when failing to convert a string into a `FixedBytes`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBytes;
///
/// let e = FixedBytes::<3>::try_from(&b"abcd"[..]).unwrap_err();
/// assert_eq!(e.to_string(), "size 4 exceeds fixed buffer size 3");
/// ```
pub struct CapacityError {
    kind: CapacityErrorKind,
}

impl fmt::Display for CapacityError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            CapacityErrorKind::Capacity { len, max } => {
                write!(f, "size {len} exceeds fixed buffer size {max}")
            }
        }
    }
}

impl fmt::Debug for CapacityError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl Error for CapacityError {}

#[derive(Debug)]
enum CapacityErrorKind {
    Capacity { len: usize, max: usize },
}

/// Attempt to convert a byte slice into a `FixedBytes<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBytes;
/// let s = FixedBytes::<5>::try_from(&b"abcd"[..])?;
/// assert_eq!(s.as_slice(), b"abcd");
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> TryFrom<&[u8]> for FixedBytes<N> {
    type Error = CapacityError;

    #[inline]
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        unsafe {
            let mut string = FixedBytes::<N>::new();

            if value.len() > N {
                return Err(CapacityError {
                    kind: CapacityErrorKind::Capacity {
                        len: value.len(),
                        max: N,
                    },
                });
            }

            ptr::copy_nonoverlapping(value.as_ptr(), string.as_mut_ptr(), value.len());
            string.set_len(value.len());
            Ok(string)
        }
    }
}
