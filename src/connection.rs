use core::ffi::CStr;
use core::ffi::{c_int, c_longlong, c_uint, c_void};
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::BitOr;
use core::ptr::{self, NonNull};

#[cfg(feature = "std")]
use alloc::ffi::CString;

#[cfg(feature = "std")]
use std::path::Path;

use crate::ffi;
use crate::owned::Owned;
use crate::utils::{c_to_str, sqlite3_try};
use crate::{Code, DatabaseNotFound, Error, Result, State, Statement};

/// A collection of flags use to prepare a statement.
pub struct Prepare(c_uint);

impl Prepare {
    /// No flags.
    ///
    /// This provides the default behavior when preparing a statement.
    pub const EMPTY: Self = Self(0);

    /// The PERSISTENT flag is a hint to the query planner that the prepared
    /// statement will be retained for a long time and probably reused many
    /// times. Without this flag, [`Connection::prepare`] assume that the
    /// prepared statement will be used just once or at most a few times and
    /// then destroyed relatively soon.
    ///
    /// The current implementation acts on this hint by avoiding the use of
    /// lookaside memory so as not to deplete the limited store of lookaside
    /// memory. Future versions of SQLite may act on this hint differently.
    pub const PERSISTENT: Self = Self(ffi::SQLITE_PREPARE_PERSISTENT as c_uint);

    /// The NORMALIZE flag is a no-op. This flag used to be required for any
    /// prepared statement that wanted to use the normalized sql interface.
    /// However, the normalized sql interface is now available to all prepared
    /// statements, regardless of whether or not they use this flag.
    pub const NORMALIZE: Self = Self(ffi::SQLITE_PREPARE_NORMALIZE as c_uint);

    /// The NO_VTAB flag causes the SQL compiler to return an error if the
    /// statement uses any virtual tables.
    pub const NO_VTAB: Self = Self(ffi::SQLITE_PREPARE_NO_VTAB as c_uint);
}

impl BitOr for Prepare {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// A SQLite database connection.
///
/// # Thread safety
///
/// The [`Connection`] implements `Send` when the `threadsafe` feature is
/// enabled and it is safe to use one [`Connection`] and [`Statement`] instances
/// per thread unless [`OpenOptions::no_mutex`] is used during opening. If
/// [`OpenOptions::no_mutex`] is set, then all database objects like
/// [`Statement`] can only be used by a single thread at a time.
///
/// If the `threadsafe` feature is not enabled, it is not valid to use any
/// [`Connection`] instances across multiple threads *in any capacity*. Doing so
/// would be undefined behavior. This is typically because the SQLite library
/// might use static data internally. This is typically only relevant in
/// single-threaded environments.
///
/// By default the connection is set up using the serialized threading mode
/// which performs internal locking through [`OpenOptions::full_mutex`].
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
/// # Examples
///
/// Opening a connection to a filesystem path:
///
/// ```no_run
/// use sqll::Connection;
///
/// let c = Connection::open("database.db")?;
///
/// c.execute(r#"
///     CREATE TABLE test (id INTEGER);
/// "#)?;
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Opening an in-memory database:
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE test (id INTEGER);
/// "#)?;
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Connection {
    raw: NonNull<ffi::sqlite3>,
    busy_callback: Option<Owned>,
}

/// Connection is `Send`.
#[cfg(feature = "threadsafe")]
unsafe impl Send for Connection {}

impl Connection {
    /// Open a database to the given path.
    ///
    /// Note that it is possible to open an in-memory database by passing
    /// `":memory:"` here, this call might require allocating depending on the
    /// platform, so it should be avoided in favor of using [`open_in_memory`]. To avoid
    /// allocating for regular paths, you can use [`open_c_str`], however you
    /// are responsible for ensuring the c-string is a valid path.
    ///
    /// This is the same as calling:
    ///
    /// ```
    /// use sqll::OpenOptions;
    /// # let path = ":memory:";
    ///
    /// let c = OpenOptions::new()
    ///     .extended_result_codes()
    ///     .read_write()
    ///     .create()
    ///     .open(path)?;
    ///
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// [`open_in_memory`]: Self::open_in_memory
    /// [`open_c_str`]: Self::open_c_str
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, cfg(feature = "std"))]
    #[inline]
    pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
        OpenOptions::new()
            .extended_result_codes()
            .read_write()
            .create()
            .open(path)
    }

    /// Open a database connection with a raw c-string.
    ///
    /// This can be used to open in-memory databases by passing `c":memory:"` or
    /// a regular open call with a filesystem path like
    /// `c"/path/to/database.sql"`.
    ///
    /// This is the same as calling:
    ///
    /// ```
    /// use sqll::OpenOptions;
    /// # let name = c":memory:";
    ///
    /// let c = OpenOptions::new()
    ///     .extended_result_codes()
    ///     .read_write()
    ///     .create()
    ///     .open_c_str(name)?;
    ///
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn open_c_str(name: &CStr) -> Result<Connection> {
        OpenOptions::new()
            .extended_result_codes()
            .read_write()
            .create()
            .open_c_str(name)
    }

    /// Open an in-memory database.
    ///
    /// This is the same as calling
    ///
    /// This is the same as calling:
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new()
    ///     .extended_result_codes()
    ///     .read_write()
    ///     .create()
    ///     .open_in_memory()?;
    ///
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn open_in_memory() -> Result<Connection> {
        OpenOptions::new()
            .extended_result_codes()
            .read_write()
            .create()
            .open_in_memory()
    }

    /// Check if the database connection is read-only.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Code, OpenOptions, DatabaseNotFound};
    ///
    /// let c = OpenOptions::new().read_write().open_in_memory()?;
    ///
    /// assert!(!c.database_read_only(c"main")?);
    /// let e = c.database_read_only(c"not a db").unwrap_err();
    /// assert!(matches!(e, DatabaseNotFound { .. }));
    ///
    /// let c = OpenOptions::new().read_only().open_in_memory()?;
    ///
    /// assert!(c.database_read_only(c"main")?);
    /// let e = c.database_read_only(c"not a db").unwrap_err();
    /// assert!(matches!(e, DatabaseNotFound { .. }));
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn database_read_only(&self, name: &CStr) -> Result<bool, DatabaseNotFound> {
        unsafe {
            match ffi::sqlite3_db_readonly(self.raw.as_ptr(), name.as_ptr()) {
                1 => Ok(true),
                0 => Ok(false),
                _ => Err(DatabaseNotFound),
            }
        }
    }

    /// Execute a batch of statements.
    ///
    /// Unlike [`prepare`], this can be used to execute multiple statements
    /// separated by a semi-colon `;` and is internally optimized for one-off
    /// queries.
    ///
    /// [`prepare`]: Self::prepare
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Result};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 42);
    ///     INSERT INTO users VALUES ('Bob', 72);
    /// "#)?;
    ///
    /// let results = c.prepare("SELECT name, age FROM users")?
    ///     .iter::<(String, u32)>()
    ///     .collect::<Result<Vec<_>>>()?;
    ///
    /// assert_eq!(results, [("Alice".to_string(), 42), ("Bob".to_string(), 72)]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn execute(&self, stmt: impl AsRef<str>) -> Result<()> {
        self._execute(stmt.as_ref())
    }

    fn _execute(&self, stmt: &str) -> Result<()> {
        unsafe {
            let mut ptr = stmt.as_ptr().cast();
            let mut len = stmt.len();

            while len > 0 {
                let mut raw = MaybeUninit::uninit();
                let mut rest = MaybeUninit::uninit();

                let l = i32::try_from(len).unwrap_or(i32::MAX);

                sqlite3_try!(ffi::sqlite3_prepare_v3(
                    self.raw.as_ptr(),
                    ptr,
                    l,
                    0,
                    raw.as_mut_ptr(),
                    rest.as_mut_ptr(),
                ));

                let rest = rest.assume_init();

                // If statement is null then it's simply empty, so we can safely
                // skip it, otherwise iterate over all rows.
                if let Some(raw) = NonNull::new(raw.assume_init()) {
                    let mut statement = Statement::from_raw(raw);
                    while let State::Row = statement.step()? {}
                }

                // Skip over empty statements.
                let o = rest.offset_from_unsigned(ptr);
                len -= o;
                ptr = rest;
            }

            Ok(())
        }
    }

    /// Enable or disable extended result codes.
    ///
    /// This can also be set during construction with
    /// [`OpenOptions::extended_result_codes`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{OpenOptions, Code};
    ///
    /// let mut c = OpenOptions::new().create().read_write().open_in_memory()?;
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
    pub fn extended_result_codes(&mut self, enabled: bool) -> Result<()> {
        unsafe {
            let onoff = i32::from(enabled);
            sqlite3_try!(ffi::sqlite3_extended_result_codes(self.raw.as_ptr(), onoff));
        }

        Ok(())
    }

    /// Get the last error message for this connection.
    ///
    /// When operating in multi-threaded environment, the error message seen
    /// here might not correspond to the query that failed unless some kind of
    /// external synchronization is in use which is the recommended way to use
    /// sqlite.
    ///
    /// This is only meaningful if an error has occured. If no errors have
    /// occured, this returns a non-erronous message like `"not an error"`
    /// (default for sqlite3).
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Code};
    ///
    /// let c = Connection::open_in_memory()?;
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
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn error_message(&self) -> &str {
        // NB: This is the same message as set by sqlite.
        static DEFAULT_MESSAGE: &str = "not an error";

        unsafe { c_to_str(ffi::sqlite3_errmsg(self.raw.as_ptr())).unwrap_or(DEFAULT_MESSAGE) }
    }

    /// Build a prepared statement.
    ///
    /// This is the same as calling `prepare_with` with `Prepare::EMPTY`.
    ///
    /// The database connection will be kept open for the lifetime of this
    /// statement.
    ///
    /// # Errors
    ///
    /// If the prepare call contains multiple statements, it will error. To
    /// execute multiple statements, use [`execute`] instead.
    ///
    /// ```
    /// use sqll::{Connection, Code};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// let e = c.prepare("CREATE TABLE test (id INTEGER) /* test */; INSERT INTO test (id) VALUES (1);").unwrap_err();
    ///
    /// assert_eq!(e.code(), Code::ERROR);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// [`execute`]: Self::execute
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Prepare};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (id INTEGER);
    /// "#)?;
    ///
    /// let mut insert_stmt = c.prepare("INSERT INTO test (id) VALUES (?);")?;
    /// let mut query_stmt = c.prepare("SELECT id FROM test;")?;
    ///
    /// drop(c);
    ///
    /// insert_stmt.reset()?;
    /// insert_stmt.bind_value(1, 42)?;
    /// assert!(insert_stmt.step()?.is_done());
    ///
    /// query_stmt.reset()?;
    ///
    /// while let Some(id) = query_stmt.next::<i64>()? {
    ///     assert_eq!(id, 42);
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn prepare(&self, stmt: impl AsRef<str>) -> Result<Statement> {
        self.prepare_with(stmt, Prepare::EMPTY)
    }

    /// Build a prepared statement with custom flags.
    ///
    /// For long-running statements it is recommended that they have the
    /// [`Prepare::PERSISTENT`] flag set.
    ///
    /// The database connection will be kept open for the lifetime of this
    /// statement.
    ///
    /// # Errors
    ///
    /// If the prepare call contains multiple statements, it will error. To
    /// execute multiple statements, use [`execute`] instead.
    ///
    /// ```
    /// use sqll::{Connection, Code, Prepare};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// let e = c.prepare_with("CREATE TABLE test (id INTEGER); INSERT INTO test (id) VALUES (1);", Prepare::PERSISTENT).unwrap_err();
    /// assert_eq!(e.code(), Code::ERROR);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// [`execute`]: Self::execute
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Prepare};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE test (id INTEGER);
    /// "#)?;
    ///
    /// let mut insert_stmt = c.prepare_with("INSERT INTO test (id) VALUES (?)", Prepare::PERSISTENT)?;
    /// let mut query_stmt = c.prepare_with("SELECT id FROM test", Prepare::PERSISTENT)?;
    ///
    /// drop(c);
    ///
    /// /* .. */
    ///
    /// insert_stmt.reset()?;
    /// insert_stmt.bind_value(1, 42)?;
    /// assert!(insert_stmt.step()?.is_done());
    ///
    /// query_stmt.reset()?;
    ///
    /// while let Some(id) = query_stmt.next::<i64>()? {
    ///     assert_eq!(id, 42);
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn prepare_with(&self, stmt: impl AsRef<str>, flags: Prepare) -> Result<Statement> {
        let stmt = stmt.as_ref();

        unsafe {
            let mut raw = MaybeUninit::uninit();
            let mut rest = MaybeUninit::uninit();

            let ptr = stmt.as_ptr().cast();
            let len = i32::try_from(stmt.len()).unwrap_or(i32::MAX);

            sqlite3_try! {
                ffi::sqlite3_prepare_v3(
                    self.raw.as_ptr(),
                    ptr,
                    len,
                    flags.0,
                    raw.as_mut_ptr(),
                    rest.as_mut_ptr(),
                )
            };

            let rest = rest.assume_init();

            let o = rest.offset_from_unsigned(ptr);

            if o != stmt.len() {
                return Err(Error::new(Code::ERROR));
            }

            let raw = ptr::NonNull::new_unchecked(raw.assume_init());
            Ok(Statement::from_raw(raw))
        }
    }

    /// Return the number of rows inserted, updated, or deleted by the most
    /// recent INSERT, UPDATE, or DELETE statement.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 42);
    ///     INSERT INTO users VALUES ('Bob', 72);
    /// "#)?;
    ///
    /// assert_eq!(c.changes(), 1);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn changes(&self) -> usize {
        unsafe { ffi::sqlite3_changes(self.raw.as_ptr()) as usize }
    }

    /// Return the total number of rows inserted, updated, and deleted by all
    /// INSERT, UPDATE, and DELETE statements since the connection was opened.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///
    ///     INSERT INTO users VALUES ('Alice', 42);
    ///     INSERT INTO users VALUES ('Bob', 72);
    /// "#)?;
    ///
    /// assert_eq!(c.total_changes(), 2);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn total_changes(&self) -> usize {
        unsafe { ffi::sqlite3_total_changes(self.raw.as_ptr()) as usize }
    }

    /// Return the rowid of the most recent successful INSERT into a rowid table
    /// or virtual table.
    ///
    /// # Examples
    ///
    /// If there is no primary key, the last inserted row id is an internal
    /// identifier for the row:
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (name TEXT);
    ///
    ///     INSERT INTO users VALUES ('Alice');
    ///     INSERT INTO users VALUES ('Bob');
    /// "#)?;
    /// assert_eq!(c.last_insert_rowid(), 2);
    ///
    /// c.execute(r#"
    ///     INSERT INTO users VALUES ('Charlie');
    /// "#)?;
    /// assert_eq!(c.last_insert_rowid(), 3);
    ///
    /// let mut stmt = c.prepare("INSERT INTO users VALUES (?)")?;
    /// stmt.bind_value(1, "Dave")?;
    /// stmt.execute()?;
    ///
    /// assert_eq!(c.last_insert_rowid(), 4);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    ///
    /// If there is a primary key, the last inserted row id corresponds to it:
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// c.execute(r#"
    ///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
    ///
    ///     INSERT INTO users (name) VALUES ('Alice');
    ///     INSERT INTO users (name) VALUES ('Bob');
    /// "#)?;
    /// assert_eq!(c.last_insert_rowid(), 2);
    ///
    /// c.execute(r#"
    ///     INSERT INTO users (name) VALUES ('Charlie')
    /// "#)?;
    /// assert_eq!(c.last_insert_rowid(), 3);
    ///
    /// c.execute(r#"
    ///     INSERT INTO users (name) VALUES ('Dave')
    /// "#)?;
    /// assert_eq!(c.last_insert_rowid(), 4);
    ///
    /// let mut select = c.prepare("SELECT id FROM users WHERE name = ?")?;
    /// select.bind_value(1, "Dave")?;
    ///
    /// while let Some(id) = select.next::<i64>()? {
    ///     assert_eq!(id, 4);
    /// }
    ///
    /// c.execute(r#"
    ///     DELETE FROM users WHERE id = 3
    /// "#)?;
    /// assert_eq!(c.last_insert_rowid(), 4);
    ///
    /// c.execute(r#"
    ///     INSERT INTO users (name) VALUES ('Charlie')
    /// "#)?;
    /// assert_eq!(c.last_insert_rowid(), 5);
    ///
    /// select.reset()?;
    /// select.bind_value(1, "Charlie")?;
    ///
    /// while let Some(id) = select.next::<i64>()? {
    ///     assert_eq!(id, 5);
    /// }
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn last_insert_rowid(&self) -> c_longlong {
        unsafe { ffi::sqlite3_last_insert_rowid(self.raw.as_ptr()) }
    }

    /// Set a callback for handling busy events.
    ///
    /// The callback is triggered when the database cannot perform an operation
    /// due to processing of some other request. If the callback returns `true`,
    /// the operation will be repeated.
    ///
    /// The busy callback should not take any actions which modify the database
    /// connection that invoked the busy handler. In other words, the busy
    /// handler is not reentrant. Any such actions result in undefined behavior.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.busy_handler(|attempts| {
    ///     println!("busy attempt: {attempts}");
    ///     attempts < 5
    /// })?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn busy_handler<F>(&mut self, callback: F) -> Result<()>
    where
        F: FnMut(usize) -> bool + Send + 'static,
    {
        extern "C" fn glue<F>(callback: *mut c_void, attempts: c_int) -> c_int
        where
            F: FnMut(usize) -> bool,
        {
            unsafe {
                if (*(callback as *mut F))(attempts as usize) {
                    1
                } else {
                    0
                }
            }
        }

        unsafe {
            let callback = Owned::new(callback)?;

            let result = ffi::sqlite3_busy_handler(
                self.raw.as_ptr(),
                Some(glue::<F>),
                callback.as_ptr().cast(),
            );

            // NB: Old callback will be dropped and freed when we set the new
            // one here.
            self.busy_callback = Some(callback);
            sqlite3_try!(result);
        }

        Ok(())
    }

    /// Clear any previously registered busy handler.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.busy_handler(|attempts| {
    ///     println!("busy attempt: {attempts}");
    ///     attempts < 5
    /// })?;
    ///
    /// c.clear_busy_handler()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn clear_busy_handler(&mut self) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_busy_handler(
                    self.raw.as_ptr(),
                    None,
                    ptr::null_mut()
                )
            };
        }

        self.busy_callback = None;
        Ok(())
    }

    /// Set an implicit callback for handling busy events that tries to repeat
    /// rejected operations until a timeout expires.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Connection;
    ///
    /// let mut c = Connection::open_in_memory()?;
    ///
    /// c.busy_timeout(5000)?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn busy_timeout(&mut self, ms: c_int) -> Result<()> {
        unsafe {
            sqlite3_try! {
                ffi::sqlite3_busy_timeout(
                    self.raw.as_ptr(),
                    ms
                )
            };
        }

        Ok(())
    }
}

impl fmt::Debug for Connection {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

impl Drop for Connection {
    #[inline]
    #[allow(unused_must_use)]
    fn drop(&mut self) {
        self.clear_busy_handler();

        // Will close the connection unconditionally. The database will stay
        // alive until all associated prepared statements have been closed since
        // we're using v2.
        let code = unsafe { ffi::sqlite3_close_v2(self.raw.as_ptr()) };
        debug_assert_eq!(code, ffi::SQLITE_OK);
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
        return Err(Error::new(Code::MISUSE));
    };

    match CString::new(bytes) {
        Ok(string) => Ok(string),
        Err(..) => Err(Error::new(Code::MISUSE)),
    }
}

/// Options that can be used to customize the opening of a SQLite database.
///
/// When using [`new`] the database is opened with the [`full_mutex`] and
/// [`extended_result_codes`] options set which makes [`Connection`] and related
/// database objects thread-safe by serializing access.
///
/// This can be disabled at runtime through [`no_mutex`], but is unsafe since if
/// set the caller has to guarantee that access to *all* database objects are
/// synchronized with the connection. Even with [`no_mutex`] long as the
/// `threadsafe` feature is set, you can correctly use one [`Connection`] per
/// thread as long as they are distinct instances and they don't reference the
/// same database.
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
    /// Create flags for opening a database connection with no options set.
    ///
    /// # Safety
    ///
    /// This is unsafe since the [`full_mutex`] option is not set, meaning the
    /// `Send` implementations for [`Connection`] and [`Statement`] are not
    /// valid leaving it up to the caller to ensure proper synchronization.
    ///
    /// [`full_mutex`]: Self::full_mutex
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = unsafe {
    ///     OpenOptions::empty().read_write().create().open_in_memory()?
    /// };
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub unsafe fn empty() -> Self {
        Self { raw: 0 }
    }

    /// Create flags for opening a database connection with default safe
    /// options.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = OpenOptions::new().read_write().create().open_in_memory()?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self {
            raw: ffi::SQLITE_OPEN_FULLMUTEX | ffi::SQLITE_OPEN_EXRESCODE,
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
    /// let c = OpenOptions::new().read_only().open_in_memory()?;
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
    /// let c = OpenOptions::new().read_write().open_in_memory()?;
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
    /// let mut opts = OpenOptions::new();
    /// opts.create();
    ///
    /// let e = opts.open_in_memory().unwrap_err();
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
    /// let c = OpenOptions::new().read_write().create().open_in_memory()?;
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
    /// let c = OpenOptions::new().read_write().create().uri().open("file:memorydb?mode=memory")?;
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
    /// let c1 = OpenOptions::new().read_write().memory().open("database")?;
    /// let c2 = OpenOptions::new().read_write().memory().open("database")?;
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
    /// # Safety
    ///
    /// This is unsafe, since it requires that the caller ensures that access to
    /// the any objects associated with the connection such as [`Statement`] is
    /// synchronized with the connection that constructed them.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::OpenOptions;
    ///
    /// let c = unsafe {
    ///     OpenOptions::new()
    ///         .no_mutex()
    ///         .read_write()
    ///         .create()
    ///         .open_in_memory()?
    /// };
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub unsafe fn no_mutex(&mut self) -> &mut Self {
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
    /// let c = OpenOptions::new().full_mutex().read_write().create().open_in_memory()?;
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
    pub fn open(&self, path: impl AsRef<Path>) -> Result<Connection> {
        let path = path_to_cstring(path.as_ref())?;
        self._open(&path)
    }

    /// Open a database connection with a raw c-string.
    ///
    /// This can be used to open in-memory databases by passing `c":memory:"` or
    /// a regular open call with a filesystem path like
    /// `c"/path/to/database.sql"`.
    pub fn open_c_str(&self, name: &CStr) -> Result<Connection> {
        self._open(name)
    }

    /// Open an in-memory database.
    pub fn open_in_memory(&self) -> Result<Connection> {
        self._open(c":memory:")
    }

    fn _open(&self, name: &CStr) -> Result<Connection> {
        unsafe {
            let mut raw = MaybeUninit::uninit();

            let code = ffi::sqlite3_open_v2(name.as_ptr(), raw.as_mut_ptr(), self.raw, ptr::null());

            let raw = raw.assume_init();

            if code != ffi::SQLITE_OK {
                ffi::sqlite3_close_v2(raw);
                return Err(Error::from_raw(code));
            }

            Ok(Connection {
                raw: NonNull::new_unchecked(raw),
                busy_callback: None,
            })
        }
    }
}
