use core::error;
use core::ffi::c_int;
use core::fmt;

use alloc::format;
use alloc::string::String;

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

    /// Get the base code this error code belongs to.
    ///
    /// If this is an extended error code, this returns the family the code
    /// belongs to.
    ///
    /// ```
    /// use sqll::Code;
    ///
    /// let code = Code::IOERR_READ;
    /// assert_eq!(code.base(), Code::IOERR);
    ///
    /// let code = Code::IOERR;
    /// assert_eq!(code.base(), Code::IOERR);
    /// ```
    #[inline]
    pub fn base(self) -> Self {
        Self::new(self.raw & 0xff)
    }
}

macro_rules! define_codes {
    ($(
        $(#[doc = $doc:literal])*
        $vis:vis const $name:ident = $value:expr;
    )*) => {
        impl Code {
            $(
                $(#[doc = $doc])*
                $vis const $name: Code = Code::new($value);
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

// NB: We define the literal values in code here because allowlisting the
// constants was too much work and they would be based on outdated sqlite3
// versions since bindgen uses the headers.
define_codes! {
    /// Successful result.
    pub const OK = 0;
    /// Generic error.
    pub const ERROR =  1;
    /// Internal logic error in SQLite.
    pub const INTERNAL =  2;
    /// Access permission denied.
    pub const PERM =  3;
    /// Callback routine requested an abort.
    pub const ABORT =  4;
    /// The database file is locked.
    pub const BUSY =  5;
    /// A table in the database is locked.
    pub const LOCKED =  6;
    /// A malloc() failed.
    pub const NOMEM =  7;
    /// Attempt to write a readonly database.
    pub const READONLY =  8;
    /// Operation terminated by sqlite3_interrupt(.
    pub const INTERRUPT =  9;
    /// Some kind of disk I/O error occurred.
    pub const IOERR = 10;
    /// The database disk image is malformed.
    pub const CORRUPT = 11;
    /// Unknown opcode in sqlite3_file_control().
    pub const NOTFOUND = 12;
    /// Insertion failed because database is full.
    pub const FULL = 13;
    /// Unable to open the database file.
    pub const CANTOPEN = 14;
    /// Database lock protocol error.
    pub const PROTOCOL = 15;
    /// Internal use only.
    pub const EMPTY = 16;
    /// The database schema changed.
    pub const SCHEMA = 17;
    /// String or BLOB exceeds size limit.
    pub const TOOBIG = 18;
    /// Abort due to constraint violation.
    pub const CONSTRAINT = 19;
    /// Data type mismatch.
    pub const MISMATCH = 20;
    /// Library used incorrectly.
    pub const MISUSE = 21;
    /// Uses OS features not supported on host.
    pub const NOLFS = 22;
    /// Authorization denied.
    pub const AUTH = 23;
    /// Not used.
    pub const FORMAT = 24;
    /// 2nd parameter to sqlite3_bind out of range.
    pub const RANGE = 25;
    /// File opened that is not a database file.
    pub const NOTADB = 26;
    /// Notifications from SQLite log.
    pub const NOTICE = 27;
    /// Warnings from SQLite log.
    pub const WARNING = 28;
    /// [`Statement::step`] has another row ready.
    ///
    /// [`Statement::step`]: crate::Statement::step
    pub const ROW = 100;
    /// [`Statement::step`] has finished executing.
    ///
    /// [`Statement::step`]: crate::Statement::step
    pub const DONE = 101;
    pub const ERROR_MISSING_COLLSEQ = Self::ERROR.raw | (1 << 8);
    pub const ERROR_RETRY = Self::ERROR.raw | (2 << 8);
    pub const ERROR_SNAPSHOT = Self::ERROR.raw | (3 << 8);
    pub const ERROR_RESERVESIZE = Self::ERROR.raw | (4 << 8);
    pub const ERROR_KEY = Self::ERROR.raw | (5 << 8);
    pub const ERROR_UNABLE = Self::ERROR.raw | (6 << 8);
    pub const IOERR_READ = Self::IOERR.raw | (1 << 8);
    pub const IOERR_SHORT_READ = Self::IOERR.raw | (2 << 8);
    pub const IOERR_WRITE = Self::IOERR.raw | (3 << 8);
    pub const IOERR_FSYNC = Self::IOERR.raw | (4 << 8);
    pub const IOERR_DIR_FSYNC = Self::IOERR.raw | (5 << 8);
    pub const IOERR_TRUNCATE = Self::IOERR.raw | (6 << 8);
    pub const IOERR_FSTAT = Self::IOERR.raw | (7 << 8);
    pub const IOERR_UNLOCK = Self::IOERR.raw | (8 << 8);
    pub const IOERR_RDLOCK = Self::IOERR.raw | (9 << 8);
    pub const IOERR_DELETE = Self::IOERR.raw | (10 << 8);
    pub const IOERR_BLOCKED = Self::IOERR.raw | (11 << 8);
    pub const IOERR_NOMEM = Self::IOERR.raw | (12 << 8);
    pub const IOERR_ACCESS = Self::IOERR.raw | (13 << 8);
    pub const IOERR_CHECKRESERVEDLOCK = Self::IOERR.raw | (14 << 8);
    pub const IOERR_LOCK = Self::IOERR.raw | (15 << 8);
    pub const IOERR_CLOSE = Self::IOERR.raw | (16 << 8);
    pub const IOERR_DIR_CLOSE = Self::IOERR.raw | (17 << 8);
    pub const IOERR_SHMOPEN = Self::IOERR.raw | (18 << 8);
    pub const IOERR_SHMSIZE = Self::IOERR.raw | (19 << 8);
    pub const IOERR_SHMLOCK = Self::IOERR.raw | (20 << 8);
    pub const IOERR_SHMMAP = Self::IOERR.raw | (21 << 8);
    pub const IOERR_SEEK = Self::IOERR.raw | (22 << 8);
    pub const IOERR_DELETE_NOENT = Self::IOERR.raw | (23 << 8);
    pub const IOERR_MMAP = Self::IOERR.raw | (24 << 8);
    pub const IOERR_GETTEMPPATH = Self::IOERR.raw | (25 << 8);
    pub const IOERR_CONVPATH = Self::IOERR.raw | (26 << 8);
    pub const IOERR_VNODE = Self::IOERR.raw | (27 << 8);
    pub const IOERR_AUTH = Self::IOERR.raw | (28 << 8);
    pub const IOERR_BEGIN_ATOMIC = Self::IOERR.raw | (29 << 8);
    pub const IOERR_COMMIT_ATOMIC = Self::IOERR.raw | (30 << 8);
    pub const IOERR_ROLLBACK_ATOMIC = Self::IOERR.raw | (31 << 8);
    pub const IOERR_DATA = Self::IOERR.raw | (32 << 8);
    pub const IOERR_CORRUPTFS = Self::IOERR.raw | (33 << 8);
    pub const IOERR_IN_PAGE = Self::IOERR.raw | (34 << 8);
    pub const IOERR_BADKEY = Self::IOERR.raw | (35 << 8);
    pub const IOERR_CODEC = Self::IOERR.raw | (36 << 8);
    pub const LOCKED_SHAREDCACHE = Self::LOCKED.raw | (1 << 8);
    pub const LOCKED_VTAB = Self::LOCKED.raw | (2 << 8);
    pub const BUSY_RECOVERY = Self::BUSY.raw | (1 << 8);
    pub const BUSY_SNAPSHOT = Self::BUSY.raw | (2 << 8);
    pub const BUSY_TIMEOUT = Self::BUSY.raw | (3 << 8);
    pub const CANTOPEN_NOTEMPDIR = Self::CANTOPEN.raw | (1 << 8);
    pub const CANTOPEN_ISDIR = Self::CANTOPEN.raw | (2 << 8);
    pub const CANTOPEN_FULLPATH = Self::CANTOPEN.raw | (3 << 8);
    pub const CANTOPEN_CONVPATH = Self::CANTOPEN.raw | (4 << 8);
    pub const CANTOPEN_DIRTYWAL = Self::CANTOPEN.raw | (5 << 8);
    pub const CANTOPEN_SYMLINK = Self::CANTOPEN.raw | (6 << 8);
    pub const CORRUPT_VTAB = Self::CORRUPT.raw | (1 << 8);
    pub const CORRUPT_SEQUENCE = Self::CORRUPT.raw | (2 << 8);
    pub const CORRUPT_INDEX = Self::CORRUPT.raw | (3 << 8);
    pub const READONLY_RECOVERY = Self::READONLY.raw | (1 << 8);
    pub const READONLY_CANTLOCK = Self::READONLY.raw | (2 << 8);
    pub const READONLY_ROLLBACK = Self::READONLY.raw | (3 << 8);
    pub const READONLY_DBMOVED = Self::READONLY.raw | (4 << 8);
    pub const READONLY_CANTINIT = Self::READONLY.raw | (5 << 8);
    pub const READONLY_DIRECTORY = Self::READONLY.raw | (6 << 8);
    pub const ABORT_ROLLBACK = Self::ABORT.raw | (2 << 8);
    pub const CONSTRAINT_CHECK = Self::CONSTRAINT.raw | (1 << 8);
    pub const CONSTRAINT_COMMITHOOK = Self::CONSTRAINT.raw | (2 << 8);
    pub const CONSTRAINT_FOREIGNKEY = Self::CONSTRAINT.raw | (3 << 8);
    pub const CONSTRAINT_FUNCTION = Self::CONSTRAINT.raw | (4 << 8);
    pub const CONSTRAINT_NOTNULL = Self::CONSTRAINT.raw | (5 << 8);
    pub const CONSTRAINT_PRIMARYKEY = Self::CONSTRAINT.raw | (6 << 8);
    pub const CONSTRAINT_TRIGGER = Self::CONSTRAINT.raw | (7 << 8);
    pub const CONSTRAINT_UNIQUE = Self::CONSTRAINT.raw | (8 << 8);
    pub const CONSTRAINT_VTAB = Self::CONSTRAINT.raw | (9 << 8);
    pub const CONSTRAINT_ROWID = Self::CONSTRAINT.raw | (10 << 8);
    pub const CONSTRAINT_PINNED = Self::CONSTRAINT.raw | (11 << 8);
    pub const CONSTRAINT_DATATYPE = Self::CONSTRAINT.raw | (12 << 8);
    pub const NOTICE_RECOVER_WAL = Self::NOTICE.raw | (1 << 8);
    pub const NOTICE_RECOVER_ROLLBACK = Self::NOTICE.raw | (2 << 8);
    pub const NOTICE_RBU = Self::NOTICE.raw | (3 << 8);
    pub const WARNING_AUTOINDEX = Self::WARNING.raw | (1 << 8);
    pub const AUTH_USER = Self::AUTH.raw | (1 << 8);
    pub const OK_LOAD_PERMANENTLY = Self::OK.raw | (1 << 8);
    pub const OK_SYMLINK = Self::OK.raw | (2 << 8);
}

/// An error.
#[derive(PartialEq, Eq)]
pub struct Error {
    /// Error code.
    code: Code,
    message: String,
}

impl Error {
    /// Construct a new error from the specified code and message.
    #[inline]
    pub fn new(code: Code, message: impl fmt::Display) -> Self {
        Self {
            code,
            message: format!("{message}"),
        }
    }

    /// Construct a new error from the specified raw code.
    #[inline]
    pub(crate) fn from_raw(code: c_int, message: impl fmt::Display) -> Self {
        Self {
            code: Code::new(code),
            message: format!("{message}"),
        }
    }

    /// The error code that caused this error.
    #[inline]
    pub fn code(&self) -> Code {
        self.code
    }
}

impl fmt::Debug for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut st = f.debug_struct("Error");
        st.field("code", &self.code);
        st.field("message", &self.message);
        st.finish()
    }
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl error::Error for Error {}

/// Indicates that a database was not found.
#[derive(Debug)]
#[non_exhaustive]
pub struct DatabaseNotFound;

impl fmt::Display for DatabaseNotFound {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "database not found")
    }
}

impl core::error::Error for DatabaseNotFound {}
