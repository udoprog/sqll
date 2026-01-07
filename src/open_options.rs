use core::ffi::CStr;
use core::ffi::c_int;
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};

#[cfg(feature = "std")]
use alloc::ffi::CString;

#[cfg(feature = "std")]
use std::path::Path;

use crate::ffi;
use crate::utils::c_to_error_text;
use crate::{Code, Connection, Error, Result};

/// Options that can be used to customize the opening of a SQLite database.
///
/// When using [`new`] the database is opened with [`extended_result_codes`]
/// options set.
///
/// To support [`Connection::into_send`] and similar methods, either
/// [`no_mutex`] or [`full_mutex`] has to be set.
///
/// Typically you should just set [`no_mutex`], which will allow you to send
/// database objects across threads but still require synchronization.
///
/// When [`full_mutex`] is set, each individual database object can be used
/// without synchronization but might block with respect to other threads
/// accessing the database simultaenously.
///
/// [`new`]: Self::new
/// [`full_mutex`]: Self::full_mutex
/// [`no_mutex`]: Self::no_mutex
/// [`extended_result_codes`]: Self::extended_result_codes
#[derive(Clone, Copy, Debug)]
pub struct OpenOptions {
    raw: c_int,
}

impl OpenOptions {
    /// Create flags for opening a database connection with default safe
    /// options.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self {
            raw: ffi::SQLITE_OPEN_EXRESCODE,
        }
    }

    /// Create flags for opening a database connection with no options set.
    ///
    /// Normally you want to use [`new`] unless you have to exclude the default
    /// options for some reason.
    ///
    /// [`new`]: Self::new
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::empty()
    ///     .read_write()
    ///     .create()
    ///     .open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn empty() -> Self {
        Self { raw: 0 }
    }

    /// The database is opened in read-only mode. If the database does not
    /// already exist, an error is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_only()
    ///     .open_in_memory()?;
    ///
    /// assert!(c.database_read_only(c"main")?);
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn read_only(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_READONLY;
        self
    }

    /// The database is opened for reading and writing if possible, or reading
    /// only if the file is write protected by the operating system.
    ///
    /// In either case the database must already exist, otherwise an error is
    /// returned. For historical reasons, if opening in read-write mode fails
    /// due to OS-level permissions, an attempt is made to open it in read-only
    /// mode. [`Connection::database_read_only`] can be used to determine
    /// whether the database is actually read-write.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .open_in_memory()?;
    ///
    /// assert!(!c.database_read_only(c"main")?);
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn read_write(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_READWRITE;
        self
    }

    /// The database is opened for reading and writing, and is created if it
    /// does not already exist.
    ///
    /// # Errors
    ///
    /// Note that a mode option like [`read_write`] must be set, otherwise this
    /// will cause an error when opening.
    ///
    /// ```
    /// use sqll::{OpenOptions, Code};
    ///
    /// let mut open = OpenOptions::new()
    ///     .create();
    ///
    /// let e = open.open_in_memory().unwrap_err();
    /// assert_eq!(e.code(), Code::MISUSE);
    ///
    /// opts.read_write();
    /// let c = opts.open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// [`read_write`]: Self::read_write
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .open_in_memory()?;
    ///
    /// assert!(!c.database_read_only(c"main")?);
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn create(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_CREATE;
        self
    }

    /// The filename can be interpreted as a URI if this flag is set.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .uri()
    ///     .open("file:memorydb?mode=memory")?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn uri(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_URI;
        self
    }

    /// The database will be opened as an in-memory database. The database is
    /// named by the "filename" argument for the purposes of cache-sharing, if
    /// shared cache mode is enabled, but the "filename" is otherwise ignored.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Code};
    ///
    /// let c1 = OpenOptions::new()
    ///     .read_write()
    ///     .memory()
    ///     .open("database")?;
    ///
    /// let c2 = OpenOptions::new()
    ///     .read_write()
    ///     .memory()
    ///     .open("database")?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn memory(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_MEMORY;
        self
    }

    /// The new database connection will use the "multi-thread" [threading
    /// mode]. This means that separate threads are allowed to use SQLite at the
    /// same time, as long as each thread is using a different database
    /// connection.
    ///
    /// [threading mode]: https://www.sqlite.org/threadsafe.html
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .no_mutex()
    ///     .open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn no_mutex(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_NOMUTEX;
        self
    }

    /// The new database connection will use the "serialized" [threading mode].
    /// This means the multiple threads can safely attempt to use the same
    /// database connection at the same time. Mutexes will block any actual
    /// concurrency, but in this mode there is no harm in trying.
    ///
    /// [threading mode]: https://sqlite.org/threadsafe.html
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .full_mutex()
    ///     .read_write()
    ///     .create()
    ///     .open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn full_mutex(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_FULLMUTEX;
        self
    }

    /// The database is opened with shared cache enabled, overriding the default
    /// shared cache setting provided. The use of shared cache mode is
    /// discouraged and hence shared cache capabilities may be omitted from many
    /// builds of SQLite. In such cases, this option is a no-op.
    #[inline]
    pub fn shared_cache(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_SHAREDCACHE;
        self
    }

    /// The database is opened with shared cache disabled, overriding the
    /// default shared cache setting provided.
    #[inline]
    pub fn private_cache(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_PRIVATECACHE;
        self
    }

    /// The database filename is not allowed to contain a symbolic link.
    #[inline]
    pub fn no_follow(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_NOFOLLOW;
        self
    }

    /// The database connection comes up in "extended result code mode". In
    /// other words, the database behaves as if
    /// [`Connection::extended_result_codes`] were called on the database
    /// connection as soon as the connection is created. In addition to setting
    /// the extended result code mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Code};
    ///
    /// let mut c = unsafe {
    ///     OpenOptions::empty()
    ///         .extended_result_codes()
    ///         .create()
    ///         .read_write()
    ///         .open_in_memory()?
    /// };
    ///
    /// let e = c.execute("
    ///     CREATE TABLE users (name TEXT);
    ///     CREATE UNIQUE INDEX idx_users_name ON users (name);
    ///
    ///     INSERT INTO users VALUES ('Bob');
    /// ");
    ///
    /// let e = c.execute("INSERT INTO users VALUES ('Bob')").unwrap_err();
    /// assert_eq!(e.code(), Code::CONSTRAINT_UNIQUE);
    /// assert_eq!(c.error_message(), "UNIQUE constraint failed: users.name");
    ///
    /// c.extended_result_codes(false)?;
    /// let e = c.execute("INSERT INTO users VALUES ('Bob')").unwrap_err();
    /// assert_eq!(e.code(), Code::CONSTRAINT);
    /// assert_eq!(c.error_message(), "UNIQUE constraint failed: users.name");
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn extended_result_codes(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_EXRESCODE;
        self
    }

    /// Open a database to the given path.
    ///
    /// Note that it is possible to open an in-memory database by passing
    /// `":memory:"` here, this call might require allocating depending on the
    /// platform, so it should be avoided in favor of using [`open_in_memory`]. To avoid
    /// allocating for regular paths, you can use [`open_c_str`], however you
    /// are responsible for ensuring the c-string is a valid path.
    ///
    /// [`open_in_memory`]: Self::open_in_memory
    /// [`open_c_str`]: Self::open_c_str
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, cfg(feature = "std"))]
    #[inline]
    pub fn open(&self, path: impl AsRef<Path>) -> Result<Connection> {
        let path = path_to_cstring(path.as_ref())?;
        self._open(&path)
    }

    /// Open a database connection with a raw c-string.
    ///
    /// This can be used to open in-memory databases by passing `c":memory:"` or
    /// a regular open call with a filesystem path like
    /// `c"/path/to/database.sql"`.
    #[inline]
    pub fn open_c_str(&self, name: &CStr) -> Result<Connection> {
        self._open(name)
    }

    /// Open an in-memory database.
    #[inline]
    pub fn open_in_memory(&self) -> Result<Connection> {
        self._open(c":memory:")
    }

    fn _open(&self, name: &CStr) -> Result<Connection> {
        unsafe {
            let mut raw = MaybeUninit::uninit();

            let code = ffi::sqlite3_open_v2(name.as_ptr(), raw.as_mut_ptr(), self.raw, ptr::null());
            let raw = raw.assume_init();

            if code != ffi::SQLITE_OK {
                let error = Error::new(Code::new(code), c_to_error_text(ffi::sqlite3_errmsg(raw)));
                ffi::sqlite3_close_v2(raw);
                return Err(error);
            }

            let is_thread_safe = cfg!(feature = "threadsafe")
                && (self.raw & (ffi::SQLITE_OPEN_NOMUTEX | ffi::SQLITE_OPEN_FULLMUTEX)) != 0;

            Ok(Connection::from_raw(
                NonNull::new_unchecked(raw),
                is_thread_safe,
            ))
        }
    }
}

/// Convert a filesystem path to a c-string.
///
/// This used to have a platform-specific implementation, particularly unix is
/// guaranteed to have a byte-sequence representation.
///
/// However, we realized that the behavior is identical to simply calling
/// `to_str`, with the addition that we check that the string is valid UTF-8.
#[cfg(feature = "std")]
pub(crate) fn path_to_cstring(p: &Path) -> Result<CString> {
    let Some(bytes) = p.to_str() else {
        return Err(Error::new(Code::MISUSE, "path is not valid utf-8"));
    };

    let Ok(string) = CString::new(bytes) else {
        return Err(Error::new(
            Code::MISUSE,
            "path utf-8 contains internal null",
        ));
    };

    Ok(string)
}
