use core::ffi::CStr;
use core::ffi::{c_char, c_int, c_void};
use core::mem::MaybeUninit;
use core::ptr;
use core::ptr::NonNull;

use alloc::boxed::Box;
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::path::Path;

use crate::error::{Error, Result};
use crate::statement::Statement;
use crate::utils::{self, sqlite3_try};

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
        unsafe {
            sqlite3_try! {
                self.raw.as_ptr(),
                ffi::sqlite3_exec(
                    self.raw.as_ptr(),
                    utils::string_to_cstring(statement.as_ref())?.as_ptr(),
                    None,
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
        }

        Ok(())
    }

    /// Execute a statement and process the resulting rows as plain text.
    ///
    /// The callback is triggered for each row. If the callback returns `false`,
    /// no more rows will be processed. For large queries and non-string data
    /// types, prepared statement are highly preferable; see `prepare`.
    #[inline]
    pub fn iterate<F>(&self, statement: impl AsRef<str>, mut callback: F) -> Result<()>
    where
        F: FnMut(&[(&str, Option<&str>)]) -> bool,
    {
        unsafe {
            sqlite3_try! {
                self.raw.as_ptr(),
                ffi::sqlite3_exec(
                    self.raw.as_ptr(),
                    utils::string_to_cstring(statement.as_ref())?.as_ptr(),
                    Some(process_callback::<F>),
                    &mut callback as *mut F as *mut _,
                    ptr::null_mut(),
                )
            };
        }

        Ok(())
    }

    /// Create a prepared statement.
    ///
    /// The database connection will be kept open for the lifetime of this
    /// statement.
    #[inline]
    pub fn prepare(&self, statement: impl AsRef<str>) -> Result<Statement> {
        Statement::new(self.raw.as_ptr(), statement)
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

// TODO: remove unwraps.
extern "C" fn process_callback<F>(
    callback: *mut c_void,
    count: c_int,
    values: *mut *mut c_char,
    columns: *mut *mut c_char,
) -> c_int
where
    F: FnMut(&[(&str, Option<&str>)]) -> bool,
{
    unsafe {
        let mut pairs = Vec::with_capacity(count as usize);

        for i in 0..(count as isize) {
            let column = {
                let pointer = *columns.offset(i);
                debug_assert!(!pointer.is_null());
                utils::cstr_to_str(pointer).unwrap()
            };

            let value = {
                let pointer = *values.offset(i);

                if pointer.is_null() {
                    None
                } else {
                    Some(utils::cstr_to_str(pointer).unwrap())
                }
            };

            pairs.push((column, value));
        }

        if (*(callback as *mut F))(&pairs) {
            0
        } else {
            1
        }
    }
}
