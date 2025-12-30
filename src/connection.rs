use core::ffi::CStr;
use core::ffi::{c_int, c_uint, c_void};
use core::mem::MaybeUninit;
use core::ops::BitOr;
use core::ptr;
use core::ptr::NonNull;

#[cfg(feature = "std")]
use std::path::Path;

use crate::State;
use crate::error::{Error, Result};
use crate::owned::Owned;
use crate::statement::Statement;
use crate::utils::sqlite3_try;

use sqlite3_sys as ffi;

/// A collection of flags use to prepare a statement.
pub struct Prepare(c_uint);

impl Prepare {
    /// No flags.
    ///
    /// This provides the default behavior when preparing a statement.
    pub const EMPTY: Self = Self(0);

    /// The PERSISTENT flag is a hint to the query planner that the prepared
    /// statement will be retained for a long time and probably reused many
    /// times. Without this flag, sqlite3_prepare_v3() and
    /// sqlite3_prepare16_v3() assume that the prepared statement will be used
    /// just once or at most a few times and then destroyed using
    /// sqlite3_finalize() relatively soon. The current implementation acts on
    /// this hint by avoiding the use of lookaside memory so as not to deplete
    /// the limited store of lookaside memory. Future versions of SQLite may act
    /// on this hint differently.
    pub const PERSISTENT: Self = Self(ffi::SQLITE_PREPARE_PERSISTENT as c_uint);

    /// The NORMALIZE flag is a no-op. This flag used to be required for any
    /// prepared statement that wanted to use the sqlite3_normalized_sql()
    /// interface. However, the sqlite3_normalized_sql() interface is now
    /// available to all prepared statements, regardless of whether or not they
    /// use this flag.
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

/// A sqlite database connection.
///
/// Connections are not thread-safe objects.
///
/// # Examples
///
/// Opening a connection to a filesystem path:
///
/// ```no_run
/// use sqlite_ll::Connection;
///
/// let c = Connection::open("database.db")?;
/// c.execute("CREATE TABLE test (id INTEGER);")?;
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Opening an in-memory database:
///
/// ```
/// use sqlite_ll::Connection;
///
/// let c = Connection::memory()?;
/// c.execute("CREATE TABLE test (id INTEGER);")?;
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
pub struct Connection {
    raw: NonNull<ffi::sqlite3>,
    busy_callback: Option<Owned>,
}

/// Connection is `Send`.
unsafe impl Send for Connection {}

impl Connection {
    /// Open a read-write connection to a new or existing database.
    #[cfg(feature = "std")]
    pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
        OpenOptions::new().set_create().set_read_write().open(path)
    }

    /// Open an in-memory database.
    pub fn memory() -> Result<Connection> {
        OpenOptions::new().set_create().set_read_write().memory()
    }

    /// Execute a statement without processing the resulting rows if any.
    #[inline]
    pub fn execute(&self, stmt: impl AsRef<str>) -> Result<()> {
        let stmt = stmt.as_ref();

        unsafe {
            let mut ptr = stmt.as_ptr().cast();
            let mut len = stmt.len();

            while len > 0 {
                let mut raw = MaybeUninit::uninit();
                let mut rest = MaybeUninit::uninit();

                let l = i32::try_from(len).unwrap_or(i32::MAX);

                let res = ffi::sqlite3_prepare_v3(
                    self.raw.as_ptr(),
                    ptr,
                    l,
                    0,
                    raw.as_mut_ptr(),
                    rest.as_mut_ptr(),
                );

                if res != ffi::SQLITE_OK {
                    return Err(Error::new(ffi::sqlite3_errcode(self.raw.as_ptr())));
                }

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
    /// use sqlite_ll::{Connection, Code};
    ///
    /// let c = Connection::memory()?;
    ///
    /// let e = c.prepare(
    ///     "
    ///     CREATE TABLE test (id INTEGER) /* test */;
    ///     INSERT INTO test (id) VALUES (1);
    ///     "
    /// ).unwrap_err();
    ///
    /// assert_eq!(e.code(), Code::ERROR);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    ///
    /// [`execute`]: Self::execute
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State, Prepare};
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE test (id INTEGER);")?;
    ///
    /// let mut insert_stmt = c.prepare("INSERT INTO test (id) VALUES (?);")?;
    /// let mut query_stmt = c.prepare("SELECT id FROM test;")?;
    ///
    /// drop(c);
    ///
    /// insert_stmt.reset()?;
    /// insert_stmt.bind(1, 42)?;
    /// assert_eq!(insert_stmt.step()?, State::Done);
    ///
    /// query_stmt.reset()?;
    ///
    /// while let State::Row = query_stmt.step()? {
    ///     let id: i64 = query_stmt.read(0)?;
    ///     assert_eq!(id, 42);
    /// }
    /// # Ok::<_, sqlite_ll::Error>(())
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
    /// use sqlite_ll::{Connection, Code, Prepare};
    ///
    /// let c = Connection::memory()?;
    ///
    /// let e = c.prepare_with(
    ///     "
    ///     CREATE TABLE test (id INTEGER) /* test */;
    ///     INSERT INTO test (id) VALUES (1);
    ///     ",
    ///     Prepare::PERSISTENT
    /// ).unwrap_err();
    /// assert_eq!(e.code(), Code::ERROR);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    ///
    /// [`execute`]: Self::execute
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State, Prepare};
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE test (id INTEGER);")?;
    ///
    /// let mut insert_stmt = c.prepare_with("INSERT INTO test (id) VALUES (?);", Prepare::PERSISTENT)?;
    /// let mut query_stmt = c.prepare_with("SELECT id FROM test;", Prepare::PERSISTENT)?;
    ///
    /// drop(c);
    ///
    /// /* .. */
    ///
    /// insert_stmt.reset()?;
    /// insert_stmt.bind(1, 42)?;
    /// assert_eq!(insert_stmt.step()?, State::Done);
    ///
    /// query_stmt.reset()?;
    ///
    /// while let State::Row = query_stmt.step()? {
    ///     let id: i64 = query_stmt.read(0)?;
    ///     assert_eq!(id, 42);
    /// }
    /// # Ok::<_, sqlite_ll::Error>(())
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
                return Err(Error::new(ffi::SQLITE_ERROR));
            }

            let raw = ptr::NonNull::new_unchecked(raw.assume_init());
            Ok(Statement::from_raw(raw))
        }
    }

    /// Return the number of rows inserted, updated, or deleted by the most
    /// recent INSERT, UPDATE, or DELETE statement.
    #[inline]
    pub fn change_count(&self) -> usize {
        unsafe { ffi::sqlite3_changes(self.raw.as_ptr()) as usize }
    }

    /// Return the total number of rows inserted, updated, and deleted by all
    /// INSERT, UPDATE, and DELETE statements since the connection was opened.
    #[inline]
    pub fn total_change_count(&self) -> usize {
        unsafe { ffi::sqlite3_total_changes(self.raw.as_ptr()) as usize }
    }

    /// Set a callback for handling busy events.
    ///
    /// The callback is triggered when the database cannot perform an operation
    /// due to processing of some other request. If the callback returns `true`,
    /// the operation will be repeated.
    pub fn set_busy_handler<F>(&mut self, callback: F) -> Result<()>
    where
        F: FnMut(usize) -> bool + Send + 'static,
    {
        self.remove_busy_handler()?;

        unsafe {
            let callback = Owned::new(callback)?;

            let result = ffi::sqlite3_busy_handler(
                self.raw.as_ptr(),
                Some(busy_callback::<F>),
                callback.as_ptr().cast(),
            );

            self.busy_callback = Some(callback);
            sqlite3_try!(result);
        }

        Ok(())
    }

    /// Set an implicit callback for handling busy events that tries to repeat
    /// rejected operations until a timeout expires.
    #[inline]
    pub fn set_busy_timeout(&mut self, ms: c_int) -> Result<()> {
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

    /// Remove the callback handling busy events.
    #[inline]
    pub fn remove_busy_handler(&mut self) -> Result<()> {
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
}

impl Drop for Connection {
    #[inline]
    #[allow(unused_must_use)]
    fn drop(&mut self) {
        self.remove_busy_handler();

        // Will close the connection unconditionally. The database will stay
        // alive until all associated prepared statements have been closed since
        // we're using v2.
        let code = unsafe { ffi::sqlite3_close_v2(self.raw.as_ptr()) };
        debug_assert_eq!(code, sqlite3_sys::SQLITE_OK);
    }
}

/// Options that can be used to customize the opening of a SQLite database.
#[derive(Default, Clone, Copy, Debug)]
pub struct OpenOptions {
    raw: c_int,
}

impl OpenOptions {
    /// Create flags for opening a database connection.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a database connection with current flags.
    ///
    /// `path` can be a filesystem path, or `:memory:` to construct an in-memory
    /// database.
    #[cfg(feature = "std")]
    pub fn open(&self, path: impl AsRef<Path>) -> Result<Connection> {
        let path = crate::utils::path_to_cstring(path.as_ref())?;
        self._open(&path)
    }

    /// Open an in-memory database connection with current flags.
    pub fn memory(&self) -> Result<Connection> {
        self._open(c":memory:")
    }

    fn _open(&self, path: &CStr) -> Result<Connection> {
        unsafe {
            let mut raw = MaybeUninit::uninit();
            let code = ffi::sqlite3_open_v2(path.as_ptr(), raw.as_mut_ptr(), self.raw, ptr::null());
            let raw = raw.assume_init();

            if code != ffi::SQLITE_OK {
                let code = ffi::sqlite3_errcode(raw);
                ffi::sqlite3_close(raw);
                return Err(Error::new(code));
            }

            Ok(Connection {
                raw: NonNull::new_unchecked(raw),
                busy_callback: None,
            })
        }
    }

    /// Create the database if it does not already exist.
    pub fn set_create(mut self) -> Self {
        self.raw |= ffi::SQLITE_OPEN_CREATE;
        self
    }

    /// Open the database in the serialized [threading mode][1].
    ///
    /// [1]: https://www.sqlite.org/threadsafe.html
    pub fn set_full_mutex(mut self) -> Self {
        self.raw |= ffi::SQLITE_OPEN_FULLMUTEX;
        self
    }

    /// Opens the database in the multi-thread [threading mode][1].
    ///
    /// [1]: https://www.sqlite.org/threadsafe.html
    pub fn set_no_mutex(mut self) -> Self {
        self.raw |= ffi::SQLITE_OPEN_NOMUTEX;
        self
    }

    /// Open the database for reading only.
    pub fn set_read_only(mut self) -> Self {
        self.raw |= ffi::SQLITE_OPEN_READONLY;
        self
    }

    /// Open the database for reading and writing.
    pub fn set_read_write(mut self) -> Self {
        self.raw |= ffi::SQLITE_OPEN_READWRITE;
        self
    }
}

extern "C" fn busy_callback<F>(callback: *mut c_void, attempts: c_int) -> c_int
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
