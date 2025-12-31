use core::fmt;
use core::mem::MaybeUninit;
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
    /// Construct a new empty `FixedBytes`.
    pub(super) const fn new() -> Self {
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
    /// use sqll::{Connection, State, FixedBytes};
    ///
    /// let c = Connection::open_memory()?;
    /// c.execute("
    /// CREATE TABLE users (id BLOB);
    /// INSERT INTO users (id) VALUES (X'01020304'), (X'05060708');
    /// ")?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM users")?;
    ///
    /// while let State::Row = stmt.step()? {
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
    /// use sqll::{Connection, State, FixedBytes};
    ///
    /// let c = Connection::open_memory()?;
    /// c.execute("
    /// CREATE TABLE users (id BLOB);
    /// INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
    /// ")?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM users")?;
    ///
    /// while let State::Row = stmt.step()? {
    ///     let bytes = stmt.get::<FixedBytes<10>>(0)?;
    ///     assert!(matches!(bytes.as_bytes(), &[1, 2, 3, 4] | &[5, 6, 7, 8, 9]));
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        if self.init == 0 {
            return &[];
        }

        // SAFETY: We've asserted that `initialized` accounts for the number of
        // bytes that have been initialized.
        unsafe { slice::from_raw_parts(self.data.as_ptr() as *const u8, self.init) }
    }
}

impl<const N: usize> fmt::Debug for FixedBytes<N> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_bytes().fmt(f)
    }
}
