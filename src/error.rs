use core::error;
use core::ffi::{CStr, c_int};
use core::fmt;

/// A result type.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Error code.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Code {
    raw: c_int,
}

impl Code {
    /// Construct a new code from the specified raw code.
    #[inline]
    pub(crate) const fn new(raw: c_int) -> Self {
        Self { raw }
    }
}

macro_rules! define_codes {
    ($(
        $vis:vis const $name:ident = $value:ident;
    )*) => {
        impl Code {
            $(
                $vis const $name: Code = Code::new($crate::ffi::$value);
            )*
        }

        impl fmt::Display for Code {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match *self {
                    $(Code::$name => write!(f, stringify!($name)),)*
                    Code { raw } => write!(f, "UNKNOWN({raw})"),
                }
            }
        }

        impl fmt::Debug for Code {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match *self {
                    $(Code::$name => write!(f, stringify!($name)),)*
                    Code { raw } => write!(f, "UNKNOWN({raw})"),
                }
            }
        }
    };
}

define_codes! {
    pub const OK = SQLITE_OK;
    pub const ERROR = SQLITE_ERROR;
    pub const INTERNAL = SQLITE_INTERNAL;
    pub const PERM = SQLITE_PERM;
    pub const ABORT = SQLITE_ABORT;
    pub const BUSY = SQLITE_BUSY;
    pub const LOCKED = SQLITE_LOCKED;
    pub const NOMEM = SQLITE_NOMEM;
    pub const READONLY = SQLITE_READONLY;
    pub const INTERRUPT = SQLITE_INTERRUPT;
    pub const IOERR = SQLITE_IOERR;
    pub const CORRUPT = SQLITE_CORRUPT;
    pub const NOTFOUND = SQLITE_NOTFOUND;
    pub const FULL = SQLITE_FULL;
    pub const CANTOPEN = SQLITE_CANTOPEN;
    pub const PROTOCOL = SQLITE_PROTOCOL;
    pub const EMPTY = SQLITE_EMPTY;
    pub const SCHEMA = SQLITE_SCHEMA;
    pub const TOOBIG = SQLITE_TOOBIG;
    pub const CONSTRAINT = SQLITE_CONSTRAINT;
    pub const MISMATCH = SQLITE_MISMATCH;
    pub const MISUSE = SQLITE_MISUSE;
    pub const NOLFS = SQLITE_NOLFS;
    pub const AUTH = SQLITE_AUTH;
    pub const FORMAT = SQLITE_FORMAT;
    pub const RANGE = SQLITE_RANGE;
    pub const NOTADB = SQLITE_NOTADB;
    pub const NOTICE = SQLITE_NOTICE;
    pub const WARNING = SQLITE_WARNING;
    pub const IOERR_READ = SQLITE_IOERR_READ;
    pub const IOERR_SHORT_READ = SQLITE_IOERR_SHORT_READ;
    pub const IOERR_WRITE = SQLITE_IOERR_WRITE;
    pub const IOERR_FSYNC = SQLITE_IOERR_FSYNC;
    pub const IOERR_DIR_FSYNC = SQLITE_IOERR_DIR_FSYNC;
    pub const IOERR_TRUNCATE = SQLITE_IOERR_TRUNCATE;
    pub const IOERR_FSTAT = SQLITE_IOERR_FSTAT;
    pub const IOERR_UNLOCK = SQLITE_IOERR_UNLOCK;
    pub const IOERR_RDLOCK = SQLITE_IOERR_RDLOCK;
    pub const IOERR_DELETE = SQLITE_IOERR_DELETE;
    pub const IOERR_BLOCKED = SQLITE_IOERR_BLOCKED;
    pub const IOERR_NOMEM = SQLITE_IOERR_NOMEM;
    pub const IOERR_ACCESS = SQLITE_IOERR_ACCESS;
    pub const IOERR_CHECKRESERVEDLOCK = SQLITE_IOERR_CHECKRESERVEDLOCK;
    pub const IOERR_LOCK = SQLITE_IOERR_LOCK;
    pub const IOERR_CLOSE = SQLITE_IOERR_CLOSE;
    pub const IOERR_DIR_CLOSE = SQLITE_IOERR_DIR_CLOSE;
    pub const IOERR_SHMOPEN = SQLITE_IOERR_SHMOPEN;
    pub const IOERR_SHMSIZE = SQLITE_IOERR_SHMSIZE;
    pub const IOERR_SHMLOCK = SQLITE_IOERR_SHMLOCK;
    pub const IOERR_SHMMAP = SQLITE_IOERR_SHMMAP;
    pub const IOERR_SEEK = SQLITE_IOERR_SEEK;
    pub const IOERR_DELETE_NOENT = SQLITE_IOERR_DELETE_NOENT;
    pub const IOERR_MMAP = SQLITE_IOERR_MMAP;
    pub const IOERR_GETTEMPPATH = SQLITE_IOERR_GETTEMPPATH;
    pub const IOERR_CONVPATH = SQLITE_IOERR_CONVPATH;
    pub const LOCKED_SHAREDCACHE = SQLITE_LOCKED_SHAREDCACHE;
    pub const BUSY_RECOVERY = SQLITE_BUSY_RECOVERY;
    pub const BUSY_SNAPSHOT = SQLITE_BUSY_SNAPSHOT;
    pub const CANTOPEN_NOTEMPDIR = SQLITE_CANTOPEN_NOTEMPDIR;
    pub const CANTOPEN_ISDIR = SQLITE_CANTOPEN_ISDIR;
    pub const CANTOPEN_FULLPATH = SQLITE_CANTOPEN_FULLPATH;
    pub const CANTOPEN_CONVPATH = SQLITE_CANTOPEN_CONVPATH;
    pub const CORRUPT_VTAB = SQLITE_CORRUPT_VTAB;
    pub const READONLY_RECOVERY = SQLITE_READONLY_RECOVERY;
    pub const READONLY_CANTLOCK = SQLITE_READONLY_CANTLOCK;
    pub const READONLY_ROLLBACK = SQLITE_READONLY_ROLLBACK;
    pub const READONLY_DBMOVED = SQLITE_READONLY_DBMOVED;
    pub const ABORT_ROLLBACK = SQLITE_ABORT_ROLLBACK;
    pub const CONSTRAINT_CHECK = SQLITE_CONSTRAINT_CHECK;
    pub const CONSTRAINT_COMMITHOOK = SQLITE_CONSTRAINT_COMMITHOOK;
    pub const CONSTRAINT_FOREIGNKEY = SQLITE_CONSTRAINT_FOREIGNKEY;
    pub const CONSTRAINT_FUNCTION = SQLITE_CONSTRAINT_FUNCTION;
    pub const CONSTRAINT_NOTNULL = SQLITE_CONSTRAINT_NOTNULL;
    pub const CONSTRAINT_PRIMARYKEY = SQLITE_CONSTRAINT_PRIMARYKEY;
    pub const CONSTRAINT_TRIGGER = SQLITE_CONSTRAINT_TRIGGER;
    pub const CONSTRAINT_UNIQUE = SQLITE_CONSTRAINT_UNIQUE;
    pub const CONSTRAINT_VTAB = SQLITE_CONSTRAINT_VTAB;
    pub const CONSTRAINT_ROWID = SQLITE_CONSTRAINT_ROWID;
    pub const NOTICE_RECOVER_WAL = SQLITE_NOTICE_RECOVER_WAL;
    pub const NOTICE_RECOVER_ROLLBACK = SQLITE_NOTICE_RECOVER_ROLLBACK;
    pub const WARNING_AUTOINDEX = SQLITE_WARNING_AUTOINDEX;
    pub const AUTH_USER = SQLITE_AUTH_USER;
    pub const OK_LOAD_PERMANENTLY = SQLITE_OK_LOAD_PERMANENTLY;
}

impl Code {
    /// Return the numeric representation of the error code.
    #[inline]
    fn as_raw(self) -> c_int {
        self.raw
    }

    /// Return the string representation of the error code.
    #[inline]
    fn message(self) -> &'static CStr {
        unsafe { CStr::from_ptr(crate::ffi::sqlite3_errstr(self.raw)) }
    }
}

/// An error.
pub struct Error {
    /// Error code.
    code: Code,
}

impl Error {
    /// Construct a new error from the specified code.
    #[inline]
    pub(crate) fn new(code: Code) -> Self {
        Self { code }
    }

    /// Construct a new error from the specified raw code.
    #[inline]
    pub(crate) fn from_raw(code: c_int) -> Self {
        Self {
            code: Code::new(code),
        }
    }

    /// The error code that caused this error.
    #[inline]
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
        write!(f, "sqlite3 error {}", self.code.as_raw())?;

        if let Ok(string) = self.code.message().to_str() {
            write!(f, ": {string}")?;
        } else {
            write!(f, ": no message")?;
        }

        Ok(())
    }
}

impl error::Error for Error {}
