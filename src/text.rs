use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt::{self, Write};
use core::hash::{Hash, Hasher};
use core::str::Utf8Error;

/// A byte string wrapper around what is effectively the canonical UTF-8 text
/// type.
///
/// This stems from the fact that sqlite provides no guarantees that the
/// underlying string type is well-formed UTF-8. Something Rust depends on for
/// its `str` type. A database might for example be modified by an external tool
/// which does not produce valid UTF-8.
///
/// In effect, we need our own private text-like type which can represent any
/// text value from the database.
///
/// If you want a string slice, we still implement the relevant traits, but be
/// aware that those APIs require valid UTF-8 and perform validation that has an
/// overhead and may fail.
///
/// See <https://www.sqlite.org/invalidutf.html>
///
/// # Examples
///
/// ```
/// use sqll::{Code, Connection, Text};
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE example (data TEXT);
/// "#)?;
///
/// let mut insert = c.prepare("INSERT INTO example (data) VALUES (?)")?;
/// insert.execute(Text::new(b"invalid: \xF0\x90\x80\xF0\x90\x80"))?;
/// insert.execute(Text::new(b"valid: \xe2\x9d\xa4\xef\xb8\x8f"))?;
///
/// let mut stmt = c.prepare("SELECT data FROM example")?;
/// let e = stmt.next::<&str>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// stmt.reset()?;
///
/// let text = stmt.next::<&Text>()?.expect("expected value");
/// assert_eq!(text.as_bytes(), b"invalid: \xF0\x90\x80\xF0\x90\x80");
/// assert_eq!(text.to_string(), "invalid: ��");
///
/// let text = stmt.next::<&Text>()?.expect("expected value");
/// assert_eq!(text.as_bytes(), b"valid: \xe2\x9d\xa4\xef\xb8\x8f");
/// assert_eq!(text.to_str()?, "valid: ❤️");
/// # Ok::<_, Box<dyn core::error::Error>>(())
/// ```
#[repr(transparent)]
pub struct Text {
    bytes: [u8],
}

impl Text {
    /// Create a new `Text` from the given type coerced into a byte slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Text;
    ///
    /// let a = Text::new(b"hello");
    /// assert_eq!(a, "hello");
    /// ```
    pub fn new<T>(bytes: &T) -> &Self
    where
        T: ?Sized + AsRef<[u8]>,
    {
        Self::from_bytes(bytes.as_ref())
    }

    /// Create a new `Text` from the given byte slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Text;
    ///
    /// let t = Text::from_bytes(b"example");
    /// assert_eq!(t, "example");
    /// ```
    pub const fn from_bytes(bytes: &[u8]) -> &Self {
        // SAFETY: Text is #[repr(transparent)] over [u8].
        unsafe { &*(bytes as *const [u8] as *const Text) }
    }

    /// Get the underlying byte slice for this `Text`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Text;
    ///
    /// let t = Text::new(b"example");
    /// assert_eq!(t.as_bytes(), b"example");
    /// ```
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Attempt to convert this `Text` into a UTF-8 string slice.
    ///
    /// Returns an error if the underlying bytes are not valid UTF-8.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Text;
    ///
    /// let t = Text::new(b"example");
    /// assert_eq!(t.to_str()?, "example");
    ///
    /// let invalid = Text::new(b"\xF0\x90\x80");
    /// assert!(invalid.to_str().is_err());
    /// # Ok::<_, core::str::Utf8Error>(())
    /// ```
    #[inline]
    pub fn to_str(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(&self.bytes)
    }
}

/// Compare the text for equality with another `Text`. This performs a byte-wise
/// comparison.
///
/// # Examples
///
/// ```
/// use sqll::Text;
///
/// let t1 = Text::new(b"example");
/// let t2 = Text::new(b"example");
/// let t3 = Text::new(b"different");
///
/// assert_eq!(t1, t2);
/// assert_ne!(t1, t3);
/// ```
impl PartialEq for Text {
    #[inline]
    fn eq(&self, other: &Text) -> bool {
        self.bytes == other.bytes
    }
}

/// Texts are equal if their bytes are equal.
///
/// # Examples
///
/// ```
/// use sqll::Text;
///
/// let t1 = Text::new(b"example");
/// let t2 = Text::new(b"example");
///
/// assert!(t1 == t2);
/// ```
impl Eq for Text {}

/// Texts are ordered by their byte sequences.
///
/// # Examples
///
/// ```
/// use sqll::Text;
///
/// let t1 = Text::new(b"apple");
/// let t2 = Text::new(b"banana");
///
/// assert!(t1 < t2);
/// ```
impl PartialOrd for Text {
    #[inline]
    fn partial_cmp(&self, other: &Text) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compare for ordering.
///
/// # Examples
///
/// ```
/// use sqll::Text;
/// use std::collections::BTreeSet;
///
/// let a = Text::new("Apple");
/// let b = Text::new("Banana");
///
/// let mut set = BTreeSet::from([a, b]);
///
/// let c = Text::new("Banana");
/// assert!(set.contains(&c));
/// assert!(!set.insert(c));
/// ```
impl Ord for Text {
    #[inline]
    fn cmp(&self, other: &Text) -> Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

/// Hash the text based on its byte sequence.
///
/// # Examples
///
/// ```
/// use sqll::Text;
/// use std::collections::hash_map::DefaultHasher;
/// use std::hash::{Hash, Hasher};
///
/// let t1 = Text::new(b"example");
/// let t2 = Text::new(b"example");
///
/// let mut hasher1 = DefaultHasher::new();
/// let mut hasher2 = DefaultHasher::new();
///
/// t1.hash(&mut hasher1);
/// t2.hash(&mut hasher2);
///
/// assert_eq!(hasher1.finish(), hasher2.finish());
/// ```
///
/// Inserting into a has set:
///
/// ```
/// use sqll::Text;
/// use std::collections::HashSet;
///
/// let a = Text::new("Apple");
/// let b = Text::new("Banana");
///
/// let mut set = HashSet::from([a, b]);
///
/// let c = Text::new("Banana");
/// assert!(set.contains(&c));
/// assert!(!set.insert(c));
/// ```
impl Hash for Text {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.bytes.hash(state);
    }
}

/// Allow borrowing the underlying byte slice.
///
/// # Examples
///
/// ```
/// use sqll::Text;
/// use core::borrow::Borrow;
///
/// let t = Text::new(b"example");
/// let b: &[u8] = t.borrow();
/// assert_eq!(b, b"example");
/// ```
impl Borrow<[u8]> for Text {
    #[inline]
    fn borrow(&self) -> &[u8] {
        &self.bytes
    }
}

/// Allow getting a reference to the underlying byte slice.
///
/// # Examples
///
/// ```
/// use sqll::Text;
///
/// let t = Text::new(b"example");
/// let b: &[u8] = t.as_ref();
/// assert_eq!(b, b"example");
/// ```
impl AsRef<[u8]> for Text {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

/// Compare the text for equality with a `str`.
///
/// This performs a byte-wise comparison.
///
/// # Examples
///
/// ```
/// use sqll::Text;
/// let t = Text::new(b"example");
///
/// assert_eq!(t, "example");
/// assert_ne!(t, "different");
/// ```
impl PartialEq<str> for Text {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        &self.bytes == other.as_bytes()
    }
}

/// The display implementation for `Text` will convert it into a UTF-8 string
/// lossily, replacing invalid sequences with the replacement character `�`.
///
/// # Examples
///
/// ```
/// use sqll::Text;
///
/// let text = Text::new(b"before\xF0\x90\x80after");
/// assert_eq!(text.to_string(), "before�after");
///
/// let text = Text::new(b"before\xF0\x90\x80\xF0\x90\x80");
/// assert_eq!(text.to_string(), "before��");
/// ```
impl fmt::Display for Text {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for chunk in self.bytes.utf8_chunks() {
            f.write_str(chunk.valid())?;

            if !chunk.invalid().is_empty() {
                f.write_char('\u{FFFD}')?;
            }
        }

        Ok(())
    }
}

/// The debug implementation for `Text` will output a string literal style
/// representation of the text, escaping invalid UTF-8 bytes as `\xNN` escapes.
///
/// # Examples
///
/// ```
/// use sqll::Text;
///
/// assert_eq! {
///     format!("{:?}", Text::new(b"Hello, \xF0\x90\x80World!")),
///     "\"Hello, \\xF0\\x90\\x80World!\"",
/// };
/// ```
impl fmt::Debug for Text {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"")?;

        for chunk in self.bytes.utf8_chunks() {
            for c in chunk.valid().chars() {
                for c in c.escape_debug() {
                    f.write_char(c)?;
                }
            }

            for b in chunk.invalid() {
                write!(f, "\\x{:02X}", b)?;
            }
        }

        write!(f, "\"")?;
        Ok(())
    }
}
