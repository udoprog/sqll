use core::error;
use core::ffi::{CStr, c_int};
use core::fmt;

/// A result type.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Error code.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Code(c_int);

impl Code {
    pub const OK: Self = Self(sqlite3_sys::SQLITE_OK);
    pub const ERROR: Self = Self(sqlite3_sys::SQLITE_ERROR);
    pub const INTERNAL: Self = Self(sqlite3_sys::SQLITE_INTERNAL);
    pub const PERM: Self = Self(sqlite3_sys::SQLITE_PERM);
    pub const ABORT: Self = Self(sqlite3_sys::SQLITE_ABORT);
    pub const BUSY: Self = Self(sqlite3_sys::SQLITE_BUSY);
    pub const LOCKED: Self = Self(sqlite3_sys::SQLITE_LOCKED);
    pub const NOMEM: Self = Self(sqlite3_sys::SQLITE_NOMEM);
    pub const READONLY: Self = Self(sqlite3_sys::SQLITE_READONLY);
    pub const INTERRUPT: Self = Self(sqlite3_sys::SQLITE_INTERRUPT);
    pub const IOERR: Self = Self(sqlite3_sys::SQLITE_IOERR);
    pub const CORRUPT: Self = Self(sqlite3_sys::SQLITE_CORRUPT);
    pub const NOTFOUND: Self = Self(sqlite3_sys::SQLITE_NOTFOUND);
    pub const FULL: Self = Self(sqlite3_sys::SQLITE_FULL);
    pub const CANTOPEN: Self = Self(sqlite3_sys::SQLITE_CANTOPEN);
    pub const PROTOCOL: Self = Self(sqlite3_sys::SQLITE_PROTOCOL);
    pub const EMPTY: Self = Self(sqlite3_sys::SQLITE_EMPTY);
    pub const SCHEMA: Self = Self(sqlite3_sys::SQLITE_SCHEMA);
    pub const TOOBIG: Self = Self(sqlite3_sys::SQLITE_TOOBIG);
    pub const CONSTRAINT: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT);
    pub const MISMATCH: Self = Self(sqlite3_sys::SQLITE_MISMATCH);
    pub const MISUSE: Self = Self(sqlite3_sys::SQLITE_MISUSE);
    pub const NOLFS: Self = Self(sqlite3_sys::SQLITE_NOLFS);
    pub const AUTH: Self = Self(sqlite3_sys::SQLITE_AUTH);
    pub const FORMAT: Self = Self(sqlite3_sys::SQLITE_FORMAT);
    pub const RANGE: Self = Self(sqlite3_sys::SQLITE_RANGE);
    pub const NOTADB: Self = Self(sqlite3_sys::SQLITE_NOTADB);
    pub const NOTICE: Self = Self(sqlite3_sys::SQLITE_NOTICE);
    pub const WARNING: Self = Self(sqlite3_sys::SQLITE_WARNING);
    pub const IOERR_READ: Self = Self(sqlite3_sys::SQLITE_IOERR_READ);
    pub const IOERR_SHORT_READ: Self = Self(sqlite3_sys::SQLITE_IOERR_SHORT_READ);
    pub const IOERR_WRITE: Self = Self(sqlite3_sys::SQLITE_IOERR_WRITE);
    pub const IOERR_FSYNC: Self = Self(sqlite3_sys::SQLITE_IOERR_FSYNC);
    pub const IOERR_DIR_FSYNC: Self = Self(sqlite3_sys::SQLITE_IOERR_DIR_FSYNC);
    pub const IOERR_TRUNCATE: Self = Self(sqlite3_sys::SQLITE_IOERR_TRUNCATE);
    pub const IOERR_FSTAT: Self = Self(sqlite3_sys::SQLITE_IOERR_FSTAT);
    pub const IOERR_UNLOCK: Self = Self(sqlite3_sys::SQLITE_IOERR_UNLOCK);
    pub const IOERR_RDLOCK: Self = Self(sqlite3_sys::SQLITE_IOERR_RDLOCK);
    pub const IOERR_DELETE: Self = Self(sqlite3_sys::SQLITE_IOERR_DELETE);
    pub const IOERR_BLOCKED: Self = Self(sqlite3_sys::SQLITE_IOERR_BLOCKED);
    pub const IOERR_NOMEM: Self = Self(sqlite3_sys::SQLITE_IOERR_NOMEM);
    pub const IOERR_ACCESS: Self = Self(sqlite3_sys::SQLITE_IOERR_ACCESS);
    pub const IOERR_CHECKRESERVEDLOCK: Self = Self(sqlite3_sys::SQLITE_IOERR_CHECKRESERVEDLOCK);
    pub const IOERR_LOCK: Self = Self(sqlite3_sys::SQLITE_IOERR_LOCK);
    pub const IOERR_CLOSE: Self = Self(sqlite3_sys::SQLITE_IOERR_CLOSE);
    pub const IOERR_DIR_CLOSE: Self = Self(sqlite3_sys::SQLITE_IOERR_DIR_CLOSE);
    pub const IOERR_SHMOPEN: Self = Self(sqlite3_sys::SQLITE_IOERR_SHMOPEN);
    pub const IOERR_SHMSIZE: Self = Self(sqlite3_sys::SQLITE_IOERR_SHMSIZE);
    pub const IOERR_SHMLOCK: Self = Self(sqlite3_sys::SQLITE_IOERR_SHMLOCK);
    pub const IOERR_SHMMAP: Self = Self(sqlite3_sys::SQLITE_IOERR_SHMMAP);
    pub const IOERR_SEEK: Self = Self(sqlite3_sys::SQLITE_IOERR_SEEK);
    pub const IOERR_DELETE_NOENT: Self = Self(sqlite3_sys::SQLITE_IOERR_DELETE_NOENT);
    pub const IOERR_MMAP: Self = Self(sqlite3_sys::SQLITE_IOERR_MMAP);
    pub const IOERR_GETTEMPPATH: Self = Self(sqlite3_sys::SQLITE_IOERR_GETTEMPPATH);
    pub const IOERR_CONVPATH: Self = Self(sqlite3_sys::SQLITE_IOERR_CONVPATH);
    pub const LOCKED_SHAREDCACHE: Self = Self(sqlite3_sys::SQLITE_LOCKED_SHAREDCACHE);
    pub const BUSY_RECOVERY: Self = Self(sqlite3_sys::SQLITE_BUSY_RECOVERY);
    pub const BUSY_SNAPSHOT: Self = Self(sqlite3_sys::SQLITE_BUSY_SNAPSHOT);
    pub const CANTOPEN_NOTEMPDIR: Self = Self(sqlite3_sys::SQLITE_CANTOPEN_NOTEMPDIR);
    pub const CANTOPEN_ISDIR: Self = Self(sqlite3_sys::SQLITE_CANTOPEN_ISDIR);
    pub const CANTOPEN_FULLPATH: Self = Self(sqlite3_sys::SQLITE_CANTOPEN_FULLPATH);
    pub const CANTOPEN_CONVPATH: Self = Self(sqlite3_sys::SQLITE_CANTOPEN_CONVPATH);
    pub const CORRUPT_VTAB: Self = Self(sqlite3_sys::SQLITE_CORRUPT_VTAB);
    pub const READONLY_RECOVERY: Self = Self(sqlite3_sys::SQLITE_READONLY_RECOVERY);
    pub const READONLY_CANTLOCK: Self = Self(sqlite3_sys::SQLITE_READONLY_CANTLOCK);
    pub const READONLY_ROLLBACK: Self = Self(sqlite3_sys::SQLITE_READONLY_ROLLBACK);
    pub const READONLY_DBMOVED: Self = Self(sqlite3_sys::SQLITE_READONLY_DBMOVED);
    pub const ABORT_ROLLBACK: Self = Self(sqlite3_sys::SQLITE_ABORT_ROLLBACK);
    pub const CONSTRAINT_CHECK: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_CHECK);
    pub const CONSTRAINT_COMMITHOOK: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_COMMITHOOK);
    pub const CONSTRAINT_FOREIGNKEY: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_FOREIGNKEY);
    pub const CONSTRAINT_FUNCTION: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_FUNCTION);
    pub const CONSTRAINT_NOTNULL: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_NOTNULL);
    pub const CONSTRAINT_PRIMARYKEY: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_PRIMARYKEY);
    pub const CONSTRAINT_TRIGGER: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_TRIGGER);
    pub const CONSTRAINT_UNIQUE: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_UNIQUE);
    pub const CONSTRAINT_VTAB: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_VTAB);
    pub const CONSTRAINT_ROWID: Self = Self(sqlite3_sys::SQLITE_CONSTRAINT_ROWID);
    pub const NOTICE_RECOVER_WAL: Self = Self(sqlite3_sys::SQLITE_NOTICE_RECOVER_WAL);
    pub const NOTICE_RECOVER_ROLLBACK: Self = Self(sqlite3_sys::SQLITE_NOTICE_RECOVER_ROLLBACK);
    pub const WARNING_AUTOINDEX: Self = Self(sqlite3_sys::SQLITE_WARNING_AUTOINDEX);
    pub const AUTH_USER: Self = Self(sqlite3_sys::SQLITE_AUTH_USER);
    pub const OK_LOAD_PERMANENTLY: Self = Self(sqlite3_sys::SQLITE_OK_LOAD_PERMANENTLY);
}

impl Code {
    /// Return the numeric representation of the error code.
    #[inline]
    fn number(self) -> c_int {
        self.0
    }

    /// Return the string representation of the error code.
    #[inline]
    fn message(self) -> &'static CStr {
        unsafe { CStr::from_ptr(sqlite3_sys::sqlite3_errstr(self.0)) }
    }
}

impl fmt::Debug for Code {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An error.
pub struct Error {
    /// Error code.
    code: Code,
}

impl Error {
    /// Construct a new error with the specified message.
    pub(crate) fn new(code: c_int) -> Self {
        Self { code: Code(code) }
    }

    /// Construct a new error from the specified code.
    pub(crate) fn from_code(code: Code) -> Self {
        Self { code }
    }

    /// The error code that caused this error.
    pub fn code(&self) -> Code {
        self.code
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut st = f.debug_struct("Error");
        st.field("code", &self.code);

        if let Ok(message) = self.code.message().to_str() {
            st.field("message", &message);
        }

        st.finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "sqlite3 error {}", self.code.number())?;

        if let Ok(string) = self.code.message().to_str() {
            write!(f, ": {string}")?;
        } else {
            write!(f, ": no message")?;
        }

        Ok(())
    }
}

impl error::Error for Error {}
