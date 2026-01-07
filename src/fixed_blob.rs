use core::fmt;
use core::hash::{Hash, Hasher};
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::ptr;
use core::slice;

use crate::CapacityError;

/// A byte slice type which can store at most `N` bytes from a column.
///
/// The data is stored inline the type which typically means on the stack.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FixedBlob, Result};
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
/// let ids = stmt.iter::<FixedBlob<10>>().collect::<Result<Vec<_>>>()?;
/// assert_eq!(ids[0].as_slice(), &[1, 2, 3, 4]);
/// assert_eq!(ids[1].as_slice(), &[5, 6, 7, 8, 9]);
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct FixedBlob<const N: usize> {
    /// Storage to read to.
    data: [MaybeUninit<u8>; N],
    /// Number of bytes initialized.
    init: usize,
}

impl<const N: usize> FixedBlob<N> {
    /// Construct a new empty [`FixedBlob`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::FixedBlob;
    ///
    /// let blob = FixedBlob::<5>::new();
    /// assert_eq!(blob.len(), 0);
    /// assert!(blob.is_empty());
    /// assert_eq!(blob.as_slice(), &[]);
    /// ```
    pub const fn new() -> Self {
        Self {
            // SAFETY: this is safe as per `MaybeUninit::uninit_array`, which isn't stable (yet).
            data: unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() },
            init: 0,
        }
    }

    /// Construct a new [`FixedBlob`] from an array of bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::FixedBlob;
    ///
    /// let blob = FixedBlob::from(*b"abcde");
    /// assert_eq!(blob.len(), 5);
    /// assert!(!blob.is_empty());
    /// assert_eq!(blob.as_slice(), b"abcde");
    /// ```
    pub const fn from_array(data: [u8; N]) -> Self {
        // SAFETY: Transmuting from [u8; N] to [MaybeUninit<u8>; N] is safe
        // since their layouts are identical.
        Self {
            data: unsafe { ptr::read(data.as_ptr().cast()) },
            init: N,
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
    /// use sqll::{Connection, FixedBlob};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (id BLOB);
    ///
    ///     INSERT INTO users (id) VALUES (X'01020304'), (X'050607');
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM users")?;
    ///
    /// for id in stmt.iter::<FixedBlob<4>>() {
    ///     let id = id?;
    ///     assert!(matches!(id.into_bytes(), Some([1, 2, 3, 4]) | None));
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn into_bytes(self) -> Option<[u8; N]> {
        if self.init != N {
            return None;
        }

        // SAFETY: All of the bytes in the sequence have been initialized and
        // can be safety transmuted.
        //
        // Method of transmuting comes from the implementation of
        // `MaybeUninit::array_assume_init` which is not yet stable.
        unsafe { Some((&self.data as *const _ as *const [u8; N]).read()) }
    }

    /// Coerce into the slice of initialized memory which is present.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FixedBlob};
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
    /// for id in stmt.iter::<FixedBlob<10>>() {
    ///     let id = id?;
    ///     assert!(matches!(id.as_slice(), &[1, 2, 3, 4] | &[5, 6, 7, 8, 9]));
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

/// Coerce into a byte slice.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob = FixedBlob::from(*b"ABCD");
/// assert!(blob.eq_ignore_ascii_case(b"ABCD"));
/// assert!(blob.eq_ignore_ascii_case(b"ABcd"));
/// ```
impl<const N: usize> Deref for FixedBlob<N> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

/// Format as a byte slice.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob = FixedBlob::from(*b"abcde");
/// assert_eq!(format!("{:?}", blob), r#"[97, 98, 99, 100, 101]"#);
/// ```
impl<const N: usize> fmt::Debug for FixedBlob<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

/// Coerce into a byte slice.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob = FixedBlob::from(*b"abcde");
/// assert_eq!(blob.as_ref(), b"abcde");
/// ```
impl<const N: usize> AsRef<[u8]> for FixedBlob<N> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

/// Compare for equality.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob1 = FixedBlob::from(*b"abcde");
/// let blob2 = FixedBlob::from(*b"abcde");
/// let blob3 = FixedBlob::from(*b"abcdx");
///
/// assert_eq!(blob1, blob2);
/// assert_ne!(blob1, blob3);
/// ```
impl<const N: usize> PartialEq for FixedBlob<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<const N: usize> Eq for FixedBlob<N> {}

/// Compare for ordering.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob1 = FixedBlob::from(*b"abcde");
/// let blob2 = FixedBlob::from(*b"abcdx");
/// assert!(blob1 < blob2);
/// ```
impl<const N: usize> PartialOrd for FixedBlob<N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Compare for ordering.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
/// use std::collections::BTreeSet;
///
/// let a = FixedBlob::<16>::try_from(&b"Apple"[..])?;
/// let b = FixedBlob::<16>::try_from(&b"Banana"[..])?;
///
/// let set = BTreeSet::from([a, b]);
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> Ord for FixedBlob<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

/// Compute a hash of the `FixedBlob<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
/// use std::collections::HashSet;
///
/// let a = FixedBlob::<16>::try_from(&b"Apple"[..])?;
/// let b = FixedBlob::<16>::try_from(&b"Banana"[..])?;
///
/// let mut set = HashSet::from([a, b]);
///
/// let c = FixedBlob::<16>::try_from(&b"Banana"[..])?;
/// assert!(set.contains(&c));
/// assert!(!set.insert(c));
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> Hash for FixedBlob<N> {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.as_slice().hash(state)
    }
}

/// Clone the `FixedBlob<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob = FixedBlob::from(b"abcde");
/// let clone = blob.clone();
/// assert_eq!(blob, clone);
/// ```
impl<const N: usize> Clone for FixedBlob<N> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            init: self.init,
            data: self.data,
        }
    }
}

/// Convert an array into a `FixedBlob<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob = FixedBlob::from(*b"abcde");
/// assert_eq!(blob.as_slice(), b"abcde");
/// ```
impl<const N: usize> From<[u8; N]> for FixedBlob<N> {
    #[inline]
    fn from(value: [u8; N]) -> Self {
        Self::from_array(value)
    }
}

/// Convert an array reference into a `FixedBlob<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob = FixedBlob::from(b"abcde");
/// assert_eq!(blob.as_slice(), b"abcde");
/// ```
impl<const N: usize> From<&[u8; N]> for FixedBlob<N> {
    #[inline]
    fn from(value: &[u8; N]) -> Self {
        Self::from_array(*value)
    }
}

/// Attempt to convert a byte slice into a `FixedBlob<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let blob = FixedBlob::<5>::try_from(&b"abcd"[..])?;
/// assert_eq!(blob.as_slice(), b"abcd");
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> TryFrom<&[u8]> for FixedBlob<N> {
    type Error = CapacityError;

    #[inline]
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        unsafe {
            let mut string = FixedBlob::<N>::new();

            if value.len() > N {
                return Err(CapacityError::capacity(value.len(), N));
            }

            ptr::copy_nonoverlapping(value.as_ptr(), string.as_mut_ptr(), value.len());
            string.set_len(value.len());
            Ok(string)
        }
    }
}
