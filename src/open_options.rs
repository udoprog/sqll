use core::ffi::CStr;
use core::ffi::c_int;
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};
use core::time::Duration;

#[cfg(feature = "std")]
use alloc::ffi::CString;

#[cfg(feature = "std")]
use std::path::Path;

use crate::ffi;
use crate::utils::c_to_error_text;
use crate::{Code, Connection, Error, Result};

/// Opening an SQLite connection.
///
/// When using [`new`] by default only the [`extended_result_codes`] option is
/// set. There is currently no known reason to disable the default options, but
/// if you really want to you can use [`empty`] instead.
///
/// [`new`]: Self::new
/// [`empty`]: Self::empty
/// [`extended_result_codes`]: Self::extended_result_codes
///
/// # Connection settings
///
/// Settings that applications otherwise apply with `PRAGMA`s right after
/// opening can be set here, so that every connection opened with these options
/// gets them, including the connections of a [`PoolBuilder`]. They are applied
/// right after the connection is opened, in this fixed order:
///
/// 1. [`busy_timeout`], first, so that the settings after it wait on a
///    contended database instead of failing with [`Code::BUSY`].
/// 2. [`foreign_keys`].
/// 3. [`journal_mode`], skipped if the connection is read-only.
/// 4. [`synchronous`].
///
/// If applying one fails, the connection is closed and opening returns the
/// error.
///
/// ```
/// use std::time::Duration;
///
/// use sqll::{JournalMode, OpenOptions, Synchronous};
///
/// let dir = tempfile::tempdir()?;
///
/// let c = OpenOptions::new()
///     .read_write()
///     .create()
///     .busy_timeout(Duration::from_secs(5))
///     .foreign_keys(true)
///     .journal_mode(JournalMode::Wal)
///     .synchronous(Synchronous::Normal)
///     .open(dir.path().join("example.db"))?;
/// # Ok::<_, Box<dyn std::error::Error>>(())
/// ```
///
/// [`PoolBuilder`]: crate::PoolBuilder
/// [`busy_timeout`]: Self::busy_timeout
/// [`foreign_keys`]: Self::foreign_keys
/// [`journal_mode`]: Self::journal_mode
/// [`synchronous`]: Self::synchronous
///
/// # Thread safety
///
/// When the `threadsafe` feature is enabled, [`Connection`] implements `Send`.
/// Since a [`Statement`] does not borrow the connection it was prepared from,
/// a connection and its statements can end up being used from different
/// threads at the same time. To keep this sound, every connection opened
/// through the safe `open*` methods uses the "serialized" [threading mode]:
///
/// * [`full_mutex`] is always set, even if it was not requested.
/// * `no_mutex` has no effect and is deprecated. Use the `unsafe`
///   [`no_mutex_unchecked`] to opt out of the per-connection mutex.
/// * Opening fails with [`Code::MISUSE`] if the linked sqlite library was
///   built without thread safety (`SQLITE_THREADSAFE=0`). The exception is
///   WebAssembly targets without the `atomics` target feature, which cannot
///   spawn threads.
///
/// Without the `threadsafe` feature, [`Connection`] does not implement `Send`
/// and `no_mutex` and [`full_mutex`] are passed on to sqlite as-is.
///
/// To send a [`Statement`] across threads, use the `unsafe`
/// [`Statement::into_send`]. This requires either `no_mutex` or
/// [`full_mutex`] to be in effect. With the `threadsafe` feature that is
/// always the case.
///
/// [`full_mutex`]: Self::full_mutex
/// [`no_mutex_unchecked`]: Self::no_mutex_unchecked
/// [`Statement`]: crate::Statement
/// [`Statement::into_send`]: crate::Statement::into_send
/// [threading mode]: https://sqlite.org/threadsafe.html
///
/// # Asynchronous usage
///
/// A SQLite connection is synchronous. There is no getting away from that. What
/// that means for use in asynchronous contexts is that you must make use of
/// mechanisms such as Tokio's [`spawn_blocking`] to ensure any database
/// operations are run on dedicated worker threads.
///
/// For examples on how to do this, see [`into_send`].
///
/// [`into_send`]: crate::Statement::into_send
/// [`spawn_blocking`]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
///
/// # Database locking
///
/// Certain operations over the database require that it is exclusively held.
/// This can manifest itself as errors when performing certain operations like
/// dropping a table that has a prepared statement associated with it.
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///    CREATE TABLE users (name TEXT);
///
///    INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
/// assert!(stmt.step()?.is_row());
///
/// let e = c.execute("DROP TABLE users").unwrap_err();
/// assert_eq!(e.code(), Code::LOCKED);
///
/// drop(stmt);
/// c.execute("DROP TABLE users")?;
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// # Using `sqlite3_config` is not supported
///
/// The [`sqlite3_config` function] is a way that allows for users of sqlite to
/// globally configure the library. Any use of this mechanism is out of scope of
/// this library. In particular it can be used to forcibly disable the effect of
/// [`full_mutex`] by setting the the [`SQLITE_CONFIG_SINGLETHREAD`] option.
///
/// We cannot guard against this. Any use of the `sqlite3_config` mechanism is
/// considered to be the responsibility of the user of this library.
///
/// [`full_mutex`]: Self::full_mutex
/// [`SQLITE_CONFIG_SINGLETHREAD`]: https://sqlite.org/c3ref/c_config_covering_index_scan.html#sqliteconfigsinglethread
/// [`sqlite3_config` function]: https://www.sqlite.org/c3ref/config.html
#[derive(Clone, Copy, Debug)]
pub struct OpenOptions {
    raw: c_int,
    /// Set by [`OpenOptions::no_mutex_unchecked`], allows `SQLITE_OPEN_NOMUTEX`
    /// to take effect when the `threadsafe` feature is enabled.
    unsynchronized: bool,
    /// Busy timeout in milliseconds, applied first after opening.
    busy_timeout: Option<c_int>,
    /// `PRAGMA foreign_keys`.
    foreign_keys: Option<bool>,
    /// `PRAGMA journal_mode`, skipped on read-only connections.
    journal_mode: Option<JournalMode>,
    /// `PRAGMA synchronous`.
    synchronous: Option<Synchronous>,
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
            unsynchronized: false,
            busy_timeout: None,
            foreign_keys: None,
            journal_mode: None,
            synchronous: None,
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
        Self {
            raw: 0,
            unsynchronized: false,
            busy_timeout: None,
            foreign_keys: None,
            journal_mode: None,
            synchronous: None,
        }
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
    /// let e = OpenOptions::new()
    ///     .create()
    ///     .open_in_memory()
    ///     .unwrap_err();
    ///
    /// assert_eq!(e.code(), Code::MISUSE);
    ///
    /// let c = OpenOptions::new()
    ///     .create()
    ///     .read_write()
    ///     .open_in_memory()?;
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

    // Unset the create option if it is set.
    //
    // This is only used internally.
    #[cfg(feature = "pool")]
    pub(super) fn no_create(&mut self) -> &mut Self {
        self.raw &= !ffi::SQLITE_OPEN_CREATE;
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
    /// When the `threadsafe` feature is enabled this option has no effect and
    /// is deprecated. Connections then always use the "serialized" threading
    /// mode, since [`Connection`] is `Send` and its statements could otherwise
    /// be used concurrently with it from another thread. It stays defined so
    /// that the feature is additive. See [`no_mutex_unchecked`] for how to opt into
    /// the "multi-thread" mode anyway. See the [thread safety] section for
    /// details.
    ///
    /// [threading mode]: https://www.sqlite.org/threadsafe.html
    /// [`no_mutex_unchecked`]: Self::no_mutex_unchecked
    /// [thread safety]: Self#thread-safety
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .no_mutex()
    ///     .open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[cfg_attr(
        feature = "threadsafe",
        deprecated(
            note = "has no effect with the `threadsafe` feature; use the unsafe `no_mutex_unchecked` to opt out of the connection mutex"
        )
    )]
    #[inline]
    pub fn no_mutex(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_NOMUTEX;
        self
    }

    /// The new database connection will use the "multi-thread" [threading
    /// mode], even when the `threadsafe` feature is enabled.
    ///
    /// This is the same as `no_mutex`, which has no effect with the
    /// `threadsafe` feature, except that it also takes effect when the
    /// `threadsafe` feature is enabled, which avoids the cost of the
    /// per-connection mutex.
    ///
    /// [threading mode]: https://www.sqlite.org/threadsafe.html
    ///
    /// # Safety
    ///
    /// When the `threadsafe` feature is enabled, [`Connection`] implements
    /// `Send`, while statements prepared from it do not borrow it. The caller
    /// must ensure that a connection opened with this option, and every
    /// statement prepared from it, are never used from more than one thread at
    /// the same time. This includes dropping them.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// // SAFETY: The connection and its statements stay on this thread.
    /// let c = unsafe {
    ///     OpenOptions::new()
    ///         .read_write()
    ///         .create()
    ///         .no_mutex_unchecked()
    ///         .open_in_memory()?
    /// };
    ///
    /// c.execute("CREATE TABLE users (name TEXT)")?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub unsafe fn no_mutex_unchecked(&mut self) -> &mut Self {
        self.raw |= ffi::SQLITE_OPEN_NOMUTEX;
        self.unsynchronized = true;
        self
    }

    /// The new database connection will use the "serialized" [threading mode].
    /// This means the multiple threads can safely attempt to use the same
    /// database connection at the same time. Mutexes will block any actual
    /// concurrency, but in this mode there is no harm in trying.
    ///
    /// When the `threadsafe` feature is enabled this is always set. See the
    /// [thread safety] section for details.
    ///
    /// [threading mode]: https://sqlite.org/threadsafe.html
    /// [thread safety]: Self#thread-safety
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

    /// Set a [busy timeout] on the connection as soon as it is opened, the
    /// same as calling [`Connection::busy_timeout`].
    ///
    /// When a table is locked by another connection, sqlite retries for up to
    /// this long before failing with [`Code::BUSY`]. The timeout is applied
    /// before any of the other connection settings, such as [`journal_mode`],
    /// so that they wait for a contended database too instead of failing at
    /// once. See [connection settings].
    ///
    /// The duration is rounded down to whole milliseconds and saturates at
    /// `c_int::MAX` milliseconds. A zero duration turns busy handling off.
    ///
    /// [busy timeout]: https://sqlite.org/c3ref/busy_timeout.html
    /// [`journal_mode`]: Self::journal_mode
    /// [connection settings]: Self#connection-settings
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .busy_timeout(Duration::from_secs(5))
    ///     .open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn busy_timeout(&mut self, timeout: Duration) -> &mut Self {
        let ms = timeout.as_millis();
        let ms = if ms > c_int::MAX as u128 {
            c_int::MAX
        } else {
            ms as c_int
        };

        self.busy_timeout = Some(ms);
        self
    }

    /// Enable or disable enforcement of [foreign key constraints] as soon as
    /// the connection is opened, using `PRAGMA foreign_keys`.
    ///
    /// SQLite does not enforce foreign keys unless asked to, and the setting
    /// applies to one connection at a time, so it has to be set on every
    /// connection that is opened. See [connection settings].
    ///
    /// [foreign key constraints]: https://sqlite.org/foreignkeys.html
    /// [connection settings]: Self#connection-settings
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, OpenOptions};
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .foreign_keys(true)
    ///     .open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (id INTEGER PRIMARY KEY);
    ///     CREATE TABLE posts (user_id INTEGER REFERENCES users (id));
    /// "#)?;
    ///
    /// let e = c.execute("INSERT INTO posts (user_id) VALUES (1)").unwrap_err();
    /// assert_eq!(e.code(), Code::CONSTRAINT_FOREIGNKEY);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn foreign_keys(&mut self, enabled: bool) -> &mut Self {
        self.foreign_keys = Some(enabled);
        self
    }

    /// Set the [journal mode] of the database as soon as the connection is
    /// opened, using `PRAGMA journal_mode`.
    ///
    /// This is skipped on a connection whose main database is read-only,
    /// since changing the journal mode is a write. That way the same options
    /// can be used for a writer and its read-only readers, as [`PoolBuilder`]
    /// does. The [`Wal`] mode is persistent, so readers opened after a writer
    /// has set it use it too.
    ///
    /// An in-memory database only supports [`Memory`] and [`Off`]; sqlite
    /// silently keeps its current mode when asked for any other, and so does
    /// this option. See [connection settings].
    ///
    /// [journal mode]: https://sqlite.org/pragma.html#pragma_journal_mode
    /// [`PoolBuilder`]: crate::PoolBuilder
    /// [`Wal`]: JournalMode::Wal
    /// [`Memory`]: JournalMode::Memory
    /// [`Off`]: JournalMode::Off
    /// [connection settings]: Self#connection-settings
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{JournalMode, OpenOptions};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let path = dir.path().join("example.db");
    ///
    /// let mut options = OpenOptions::new();
    /// options.journal_mode(JournalMode::Wal);
    ///
    /// let c = options.clone().read_write().create().open(&path)?;
    ///
    /// let mut stmt = c.prepare("PRAGMA journal_mode")?;
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.column::<String>(0)?, "wal");
    ///
    /// // Skipped on a read-only connection, which still sees WAL since the
    /// // mode is stored in the database.
    /// let r = options.clone().read_only().open(&path)?;
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn journal_mode(&mut self, mode: JournalMode) -> &mut Self {
        self.journal_mode = Some(mode);
        self
    }

    /// Set how hard sqlite works to make writes [durable] as soon as the
    /// connection is opened, using `PRAGMA synchronous`.
    ///
    /// The setting applies to one connection at a time. [`Synchronous::Normal`]
    /// is the usual choice together with [`JournalMode::Wal`]. See
    /// [connection settings].
    ///
    /// [durable]: https://sqlite.org/pragma.html#pragma_synchronous
    /// [connection settings]: Self#connection-settings
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Synchronous};
    ///
    /// let c = OpenOptions::new()
    ///     .read_write()
    ///     .create()
    ///     .synchronous(Synchronous::Normal)
    ///     .open_in_memory()?;
    ///
    /// let mut stmt = c.prepare("PRAGMA synchronous")?;
    /// assert!(stmt.step()?.is_row());
    /// assert_eq!(stmt.column::<i64>(0)?, 1);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn synchronous(&mut self, synchronous: Synchronous) -> &mut Self {
        self.synchronous = Some(synchronous);
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
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
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

    /// Compute the flags to open the connection with.
    ///
    /// With the `threadsafe` feature [`Connection`] is `Send`, so unless the
    /// caller opted out with the unsafe [`no_mutex_unchecked`], the connection
    /// must be serialized, which requires both `SQLITE_OPEN_FULLMUTEX` and a
    /// sqlite library built with thread safety.
    ///
    /// [`no_mutex_unchecked`]: Self::no_mutex_unchecked
    fn flags(&self) -> Result<c_int> {
        let mut flags = self.raw;

        if cfg!(feature = "threadsafe") {
            // A WebAssembly target without atomics cannot spawn threads, so
            // `Send` has no effect there. This is also where the bundled
            // library is always built without thread safety.
            let can_thread = !cfg!(all(target_family = "wasm", not(target_feature = "atomics")));

            // SAFETY: This only reads a compile-time constant of the library.
            if can_thread && unsafe { ffi::sqlite3_threadsafe() } == 0 {
                return Err(Error::new(
                    Code::MISUSE,
                    "the threadsafe feature is enabled but sqlite was built without thread safety",
                ));
            }

            if !self.unsynchronized {
                flags &= !ffi::SQLITE_OPEN_NOMUTEX;
                flags |= ffi::SQLITE_OPEN_FULLMUTEX;
            }
        }

        Ok(flags)
    }

    fn _open(&self, name: &CStr) -> Result<Connection> {
        let flags = self.flags()?;

        unsafe {
            let mut raw = MaybeUninit::uninit();

            let code = ffi::sqlite3_open_v2(name.as_ptr(), raw.as_mut_ptr(), flags, ptr::null());
            let raw = raw.assume_init();

            if code != ffi::SQLITE_OK {
                let error = Error::new(Code::new(code), c_to_error_text(ffi::sqlite3_errmsg(raw)));
                ffi::sqlite3_close_v2(raw);
                return Err(error);
            }

            let is_thread_safe = ffi::sqlite3_threadsafe() != 0
                && (flags & (ffi::SQLITE_OPEN_NOMUTEX | ffi::SQLITE_OPEN_FULLMUTEX)) != 0;

            let mut c = Connection::from_raw(NonNull::new_unchecked(raw), is_thread_safe);
            // NB: On error the connection is dropped, which closes it.
            self.apply(&mut c)?;
            Ok(c)
        }
    }

    /// Apply connection settings in a fixed order.
    ///
    /// The busy timeout goes first so that everything after it waits on a
    /// contended database instead of failing immediately with `SQLITE_BUSY`.
    fn apply(&self, c: &mut Connection) -> Result<()> {
        if let Some(ms) = self.busy_timeout {
            c.busy_timeout(ms)?;
        }

        if let Some(enabled) = self.foreign_keys {
            c.execute(if enabled {
                "PRAGMA foreign_keys = ON"
            } else {
                "PRAGMA foreign_keys = OFF"
            })?;
        }

        if let Some(mode) = self.journal_mode {
            // Changing the journal mode is a write, which a read-only
            // connection cannot perform.
            if !matches!(c.database_read_only(c"main"), Ok(true)) {
                c.execute(mode.pragma())?;
            }
        }

        if let Some(synchronous) = self.synchronous {
            c.execute(synchronous.pragma())?;
        }

        Ok(())
    }
}

/// A [journal mode] for [`OpenOptions::journal_mode`].
///
/// [journal mode]: https://sqlite.org/pragma.html#pragma_journal_mode
///
/// # Examples
///
/// ```
/// use sqll::{JournalMode, OpenOptions};
///
/// let c = OpenOptions::new()
///     .read_write()
///     .create()
///     .journal_mode(JournalMode::Memory)
///     .open_in_memory()?;
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JournalMode {
    /// The rollback journal is deleted at the end of each transaction. This
    /// is the default.
    Delete,
    /// The rollback journal is truncated to zero length instead of deleted.
    Truncate,
    /// The rollback journal header is overwritten with zeros instead of the
    /// journal being deleted.
    Persist,
    /// The rollback journal is kept in memory.
    Memory,
    /// A [write-ahead log] is used instead of a rollback journal. This mode is
    /// persistent: it is stored in the database file.
    ///
    /// [write-ahead log]: https://sqlite.org/wal.html
    Wal,
    /// No rollback journal is kept at all, so transactions cannot be rolled
    /// back safely.
    Off,
}

impl JournalMode {
    fn pragma(self) -> &'static str {
        match self {
            JournalMode::Delete => "PRAGMA journal_mode = DELETE",
            JournalMode::Truncate => "PRAGMA journal_mode = TRUNCATE",
            JournalMode::Persist => "PRAGMA journal_mode = PERSIST",
            JournalMode::Memory => "PRAGMA journal_mode = MEMORY",
            JournalMode::Wal => "PRAGMA journal_mode = WAL",
            JournalMode::Off => "PRAGMA journal_mode = OFF",
        }
    }
}

/// A [synchronous] setting for [`OpenOptions::synchronous`].
///
/// [synchronous]: https://sqlite.org/pragma.html#pragma_synchronous
///
/// # Examples
///
/// ```
/// use sqll::{OpenOptions, Synchronous};
///
/// let c = OpenOptions::new()
///     .read_write()
///     .create()
///     .synchronous(Synchronous::Full)
///     .open_in_memory()?;
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Synchronous {
    /// Data is handed to the operating system without syncing. Fastest, but a
    /// power loss can corrupt the database.
    Off,
    /// Sync at the most critical moments. Safe from corruption in
    /// [`JournalMode::Wal`], where a power loss may roll back the latest
    /// commits.
    Normal,
    /// Sync after every commit. The default.
    Full,
    /// Like [`Full`], and also syncs the directory after a rollback journal is
    /// unlinked.
    ///
    /// [`Full`]: Self::Full
    Extra,
}

impl Synchronous {
    fn pragma(self) -> &'static str {
        match self {
            Synchronous::Off => "PRAGMA synchronous = OFF",
            Synchronous::Normal => "PRAGMA synchronous = NORMAL",
            Synchronous::Full => "PRAGMA synchronous = FULL",
            Synchronous::Extra => "PRAGMA synchronous = EXTRA",
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
