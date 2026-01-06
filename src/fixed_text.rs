use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use core::str;

use crate::{CapacityError, FixedBlob, Text};

/// A helper to read at most a fixed number of `N` bytes from a column. This
/// allocates the storage for the bytes read on the stack.
pub struct FixedText<const N: usize> {
    inner: FixedBlob<N>,
}

impl<const N: usize> FixedText<N> {
    /// Construct a new empty [`FixedText`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::FixedText;
    ///
    /// let s = FixedText::<5>::new();
    /// assert_eq!(s.as_text(), "");
    /// ```
    pub const fn new() -> Self {
        Self {
            inner: FixedBlob::new(),
        }
    }

    /// Converts a vector of bytes to a String without checking that the string
    /// contains valid UTF-8.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{FixedBlob, FixedText};
    ///
    /// let bytes = FixedBlob::<16>::try_from(&b"Hello World"[..])?;
    /// let s = unsafe { FixedText::from_inner(bytes) };
    /// assert_eq!(s.as_text(), "Hello World");
    /// # Ok::<_, sqll::CapacityError>(())
    /// ```
    pub const fn from_inner(inner: FixedBlob<N>) -> Self {
        Self { inner }
    }

    /// Coerce into the initialized string slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, FixedText};
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
    /// assert_eq! {
    ///     stmt.iter::<FixedText<6>>().collect::<Vec<_>>(),
    ///     [Ok(FixedText::<6>::try_from("Alice")?), Ok(FixedText::<6>::try_from("Bob")?)]
    /// };
    /// # Ok::<_, Box<dyn core::error::Error>>(())
    /// ```
    pub fn as_text(&self) -> &Text {
        Text::new(self.inner.as_slice())
    }
}

impl<const N: usize> Deref for FixedText<N> {
    type Target = Text;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_text()
    }
}

impl<const N: usize> fmt::Debug for FixedText<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_text().fmt(f)
    }
}

impl<const N: usize> fmt::Display for FixedText<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_text().fmt(f)
    }
}

impl<const N: usize> AsRef<Text> for FixedText<N> {
    #[inline]
    fn as_ref(&self) -> &Text {
        self.as_text()
    }
}

impl<const N: usize> PartialEq for FixedText<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_text() == other.as_text()
    }
}

impl<const N: usize> Eq for FixedText<N> {}

impl<const N: usize> PartialOrd for FixedText<N> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> Ord for FixedText<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_text().cmp(other.as_text())
    }
}

impl<const N: usize> Hash for FixedText<N> {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.as_text().hash(state)
    }
}

impl<const N: usize> Clone for FixedText<N> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Attempt to convert a string slice into a `FixedText<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
/// let s = FixedText::<5>::try_from("Hello")?;
/// assert_eq!(s.as_text(), "Hello");
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> TryFrom<&str> for FixedText<N> {
    type Error = CapacityError;

    #[inline]
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self::from_inner(FixedBlob::try_from(value.as_bytes())?))
    }
}
