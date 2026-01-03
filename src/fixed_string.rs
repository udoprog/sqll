use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use core::str;

use crate::{CapacityError, FixedBytes};

/// A helper to read at most a fixed number of `N` bytes from a column. This
/// allocates the storage for the bytes read on the stack.
pub struct FixedString<const N: usize> {
    inner: FixedBytes<N>,
}

impl<const N: usize> FixedString<N> {
    /// Construct a new empty [`FixedString`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::FixedString;
    /// let s = FixedString::<5>::new();
    /// assert_eq!(s.as_str(), "");
    /// ```
    pub const fn new() -> Self {
        Self {
            inner: FixedBytes::new(),
        }
    }

    /// Converts a vector of bytes to a String without checking that the string
    /// contains valid UTF-8.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it does not check that the bytes passed
    /// to it are valid UTF-8. If this constraint is violated, it may cause
    /// memory unsafety issues with future users of the String, as the rest of
    /// the standard library assumes that Strings are valid UTF-8.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{FixedBytes, FixedString};
    ///
    /// let bytes = FixedBytes::<16>::try_from(&b"Hello World"[..])?;
    /// let s = unsafe { FixedString::from_utf8_unchecked(bytes) };
    /// assert_eq!(s.as_str(), "Hello World");
    /// # Ok::<_, sqll::CapacityError>(())
    /// ```
    pub const unsafe fn from_utf8_unchecked(inner: FixedBytes<N>) -> Self {
        Self { inner }
    }

    /// Coerce into the initialized string slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FixedString};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name BLOB);
    ///
    ///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
    /// "#)?;
    ///
    /// let mut stmt = c.prepare("SELECT name FROM users")?;
    ///
    /// for name in stmt.iter::<FixedString<6>>() {
    ///     let name = name?;
    ///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(self.inner.as_slice()) }
    }
}

impl<const N: usize> Deref for FixedString<N> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<const N: usize> fmt::Debug for FixedString<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl<const N: usize> fmt::Display for FixedString<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl<const N: usize> AsRef<str> for FixedString<N> {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> PartialEq for FixedString<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<const N: usize> Eq for FixedString<N> {}

impl<const N: usize> PartialOrd for FixedString<N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for FixedString<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<const N: usize> Hash for FixedString<N> {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.as_str().hash(state)
    }
}

impl<const N: usize> Clone for FixedString<N> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Attempt to convert a string slice into a `FixedString<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedString;
/// let s = FixedString::<5>::try_from("Hello")?;
/// assert_eq!(s.as_str(), "Hello");
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> TryFrom<&str> for FixedString<N> {
    type Error = CapacityError;

    #[inline]
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        unsafe {
            Ok(Self::from_utf8_unchecked(FixedBytes::try_from(
                value.as_bytes(),
            )?))
        }
    }
}
