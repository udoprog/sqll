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

/// Deref to `Text`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let ft = FixedText::from(*b"invalid: \xF0\x90\x80\xF0\x90\x80");
/// assert_eq!(ft.as_bytes(), b"invalid: \xF0\x90\x80\xF0\x90\x80");
/// assert_eq!(ft.to_string(), "invalid: ��");
/// ```
impl<const N: usize> Deref for FixedText<N> {
    type Target = Text;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_text()
    }
}

/// Format as `Text`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let ft = FixedText::<5>::try_from("Hello")?;
/// assert_eq!(format!("{:?}", ft), "\"Hello\"");
/// assert_eq!(format!("{}", ft), "Hello");
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> fmt::Debug for FixedText<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_text().fmt(f)
    }
}

/// The display implementation for `Text` will convert it into a UTF-8 string
/// lossily, replacing invalid sequences with the replacement character `�`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let text = FixedText::from(b"before\xF0\x90\x80after");
/// assert_eq!(text.to_string(), "before�after");
///
/// let text = FixedText::from(b"before\xF0\x90\x80\xF0\x90\x80");
/// assert_eq!(text.to_string(), "before��");
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> fmt::Display for FixedText<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_text().fmt(f)
    }
}

/// Coerce into [`Text`].
///
/// # Examples
///
/// ```
/// use sqll::{FixedText, Text};
///
/// let text = FixedText::from(*b"example");
/// let text: &Text = text.as_ref();
/// assert_eq!(text, "example");
/// ```
impl<const N: usize> AsRef<Text> for FixedText<N> {
    #[inline]
    fn as_ref(&self) -> &Text {
        self.as_text()
    }
}

/// Compare the text for equality with another `Text`. This performs a byte-wise
/// comparison.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let t1 = FixedText::from(*b"example");
/// let t2 = FixedText::from(*b"example");
/// let t3 = FixedText::from(*b"different");
///
/// assert_eq!(t1, t2);
/// assert_ne!(t1, t3);
/// ```
impl<const N: usize, const U: usize> PartialEq<FixedText<U>> for FixedText<N> {
    #[inline]
    fn eq(&self, other: &FixedText<U>) -> bool {
        self.as_text() == other.as_text()
    }
}

impl<const N: usize> Eq for FixedText<N> {}

/// Compare for ordering.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let a = FixedText::<16>::try_from("Apple")?;
/// let b = FixedText::<16>::try_from("Banana")?;
///
/// assert!(a < b);
/// assert!(b > a);
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> PartialOrd for FixedText<N> {
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
/// use sqll::FixedText;
/// use std::collections::BTreeSet;
///
/// let a = FixedText::<16>::try_from("Apple")?;
/// let b = FixedText::<16>::try_from("Banana")?;
///
/// let set = BTreeSet::from([a, b]);
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> Ord for FixedText<N> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_text().cmp(other.as_text())
    }
}

/// Hash the `FixedText<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
/// use std::collections::HashSet;
///
/// let a = FixedText::<16>::try_from("Apple")?;
/// let b = FixedText::<16>::try_from("Banana")?;
///
/// let mut set = HashSet::from([a, b]);
///
/// let c = FixedText::<16>::try_from("Banana")?;
/// assert!(set.contains(&c));
/// assert!(!set.insert(c));
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> Hash for FixedText<N> {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.as_text().hash(state)
    }
}

/// Clone the `FixedText<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let ft1 = FixedText::<5>::try_from("Hello")?;
/// let ft2 = ft1.clone();
/// assert_eq!(ft1, ft2);
/// # Ok::<_, sqll::CapacityError>(())
/// ```
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

/// Attempt to convert a byte slice into a `FixedText<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let ft = FixedText::<5>::try_from(&b"Hello"[..])?;
/// assert_eq!(ft.as_text(), "Hello");
/// # Ok::<_, sqll::CapacityError>(())
/// ```
impl<const N: usize> TryFrom<&[u8]> for FixedText<N> {
    type Error = CapacityError;

    #[inline]
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self::from_inner(FixedBlob::try_from(value)?))
    }
}

/// Attempt to convert a byte array into a `FixedText<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let ft = FixedText::from(b"Hello");
/// assert_eq!(ft.as_bytes(), b"Hello");
/// assert_eq!(ft.as_text(), "Hello");
/// ```
impl<const N: usize> From<&[u8; N]> for FixedText<N> {
    #[inline]
    fn from(value: &[u8; N]) -> Self {
        Self::from_inner(FixedBlob::from(value))
    }
}

/// Attempt to convert a byte array into a `FixedText<N>`.
///
/// # Examples
///
/// ```
/// use sqll::FixedText;
///
/// let ft = FixedText::from(*b"Hello");
/// assert_eq!(ft.as_text(), "Hello");
/// ```
impl<const N: usize> From<[u8; N]> for FixedText<N> {
    #[inline]
    fn from(value: [u8; N]) -> Self {
        Self::from_inner(FixedBlob::from(value))
    }
}
