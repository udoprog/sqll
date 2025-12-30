use core::ffi::CStr;
use core::ffi::{c_int, c_uint, c_void};
use core::mem::MaybeUninit;
use core::ptr;
use core::ptr::NonNull;

use alloc::boxed::Box;

#[cfg(feature = "std")]
use std::path::Path;

use crate::State;
use crate::error::{Error, Result};
use crate::statement::Statement;
use crate::utils::sqlite3_try;

use sqlite3_sys as ffi;

/// A SQLite database connection.
pub struct Connection {
    raw: NonNull<ffi::sqlite3>,
    busy_callback: Option<Box<dyn FnMut(usize) -> bool>>,
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
    pub fn execute(&self, statement: impl AsRef<str>) -> Result<()> {
        let statement = statement.as_ref();

        unsafe {
            let mut ptr = statement.as_ptr().cast();
            let mut len = statement.len();

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
    /// By default, this has the `Prepare::PERSISTENT` flag set.
    ///
    /// The database connection will be kept open for the lifetime of this
    /// statement.
    #[inline]
    pub fn prepare(&self, statement: impl AsRef<str>) -> Result<Statement> {
        let mut raw = MaybeUninit::uninit();
        let statement = statement.as_ref();

        unsafe {
            sqlite3_try! {
                self.raw.as_ptr(),
                ffi::sqlite3_prepare_v3(
                    self.raw.as_ptr(),
                    statement.as_ptr().cast(),
                    statement.len() as c_int,
                    ffi::SQLITE_PREPARE_PERSISTENT as c_uint,
                    raw.as_mut_ptr(),
                    ptr::null_mut(),
                )
            };

            let raw = ptr::NonNull::new_unchecked(raw.assume_init());
            return Ok(Statement::from_raw(raw));
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
            let mut callback = Box::new(callback);

            let result = ffi::sqlite3_busy_handler(
                self.raw.as_ptr(),
                Some(busy_callback::<F>),
                callback.as_mut() as *const F as *mut F as *mut _,
            );

            self.busy_callback = Some(callback);

            sqlite3_try! {
                self.raw.as_ptr(),
                result
            }
        }

        Ok(())
    }

    /// Set an implicit callback for handling busy events that tries to repeat
    /// rejected operations until a timeout expires.
    #[inline]
    pub fn set_busy_timeout(&mut self, milliseconds: usize) -> Result<()> {
        unsafe {
            sqlite3_try! {
                self.raw.as_ptr(),
                ffi::sqlite3_busy_timeout(
                    self.raw.as_ptr(),
                    milliseconds as c_int
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
                self.raw.as_ptr(),
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
