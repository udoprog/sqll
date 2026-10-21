use core::error;
#[cfg(feature = "alloc")]
use core::ffi::c_int;
use core::fmt;

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::format;
#[cfg(feature = "alloc")]
use alloc::string::String;

use crate::{Code, Column, Statement};

/// A result type alias.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// An error.
///
/// Every error carries a [`Code`], and with the `alloc` feature a message or a
/// source error.
///
/// To tell transient failures apart from real ones, use the classification
/// methods such as [`Error::is_busy`], [`Error::is_constraint`] and
/// [`Error::is_full`] rather than matching on [`Error::code`]. They look at
/// the code's [base] family, so they give the same answer whether or not the
/// connection has [extended result codes] enabled.
///
/// A failed conversion in a [`FromColumn`] implementation is reported with
/// [`Error::mismatch`] or [`Error::conversion`]. Errors raised while reading a
/// column name it, by index and by name when SQLite knows the name, and the
/// [`Row`] derive adds the field. Use [`Error::with_column`] and
/// [`Error::with_field`] to do the same in hand-written code.
///
/// `Error` does not implement `PartialEq`, since it can hold an arbitrary
/// source error. Compare [`Error::code`] instead.
///
/// [base]: Code::base
/// [extended result codes]: crate::OpenOptions::extended_result_codes
/// [`FromColumn`]: crate::FromColumn
/// [`Row`]: crate::Row
///
/// # Examples
///
/// ```
/// use sqll::{Code, Connection};
///
/// let c = Connection::open_in_memory()?;
/// c.execute("CREATE TABLE users (name TEXT UNIQUE)")?;
/// c.execute("INSERT INTO users VALUES ('Alice')")?;
///
/// let e = c.execute("INSERT INTO users VALUES ('Alice')").unwrap_err();
/// assert!(e.is_constraint());
/// assert!(!e.is_busy());
/// assert_eq!(e.code().base(), Code::CONSTRAINT);
/// # Ok::<_, sqll::Error>(())
/// ```
pub struct Error {
    /// Error code.
    code: Code,
    #[cfg(feature = "alloc")]
    repr: Repr,
    /// The column and field the error was raised for, if known.
    #[cfg(feature = "alloc")]
    context: Option<Box<Context>>,
}

#[cfg(feature = "alloc")]
#[derive(Debug, Default)]
struct Context {
    column: Option<ColumnContext>,
    field: Option<&'static str>,
}

#[cfg(feature = "alloc")]
#[derive(Debug)]
struct ColumnContext {
    index: c_int,
    name: Option<String>,
}

#[cfg(feature = "alloc")]
impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(field) = self.field {
            write!(f, "field `{field}`")?;

            if self.column.is_some() {
                write!(f, " from ")?;
            }
        }

        if let Some(column) = &self.column {
            write!(f, "column {}", column.index)?;

            if let Some(name) = &column.name {
                write!(f, " (`{name}`)")?;
            }
        }

        Ok(())
    }
}

#[cfg(feature = "alloc")]
enum Repr {
    Message(String),
    Source(Box<dyn error::Error + Send + Sync + 'static>),
}

impl Error {
    /// Construct a new error from the specified code and message.
    ///
    /// Without the `alloc` feature the message is discarded, and the error
    /// displays the description SQLite has for `code`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// let e = Error::new(Code::MISUSE, format_args!("bad index {}", 4));
    /// assert_eq!(e.code(), Code::MISUSE);
    /// assert_eq!(e.to_string(), "bad index 4");
    /// ```
    #[inline]
    pub fn new(code: Code, _message: impl fmt::Display) -> Self {
        Self {
            code,
            #[cfg(feature = "alloc")]
            repr: Repr::Message(format!("{_message}")),
            #[cfg(feature = "alloc")]
            context: None,
        }
    }

    /// Construct a new generic error with the specified message, using
    /// [`Code::ERROR`].
    ///
    /// The message is anything implementing [`Display`], so there is no need
    /// to call `format!` first: pass the value itself or use `format_args!`.
    ///
    /// [`Display`]: fmt::Display
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// let e = Error::custom(format_args!("{} is not ready", "cache"));
    /// assert_eq!(e.code(), Code::ERROR);
    /// assert_eq!(e.to_string(), "cache is not ready");
    /// ```
    #[inline]
    pub fn custom(message: impl fmt::Display) -> Self {
        Self::new(Code::ERROR, message)
    }

    /// Construct an error reporting that a column value could not be
    /// converted, using [`Code::MISMATCH`].
    ///
    /// This is the error a [`FromColumn`] implementation returns when the
    /// value it read is not one it accepts. When the failure comes from
    /// another error, use [`Error::conversion`] to keep it as the source.
    ///
    /// [`FromColumn`]: crate::FromColumn
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// let e = Error::mismatch(format_args!("unknown color {}", 7));
    /// assert_eq!(e.code(), Code::MISMATCH);
    /// assert_eq!(e.to_string(), "unknown color 7");
    /// ```
    #[inline]
    pub fn mismatch(message: impl fmt::Display) -> Self {
        Self::new(Code::MISMATCH, message)
    }

    /// Construct an error reporting that a column value could not be
    /// converted because of `source`, using [`Code::MISMATCH`].
    ///
    /// With the `alloc` feature the source is kept: the error displays as
    /// `source` does, [`core::error::Error::source`] continues with the
    /// source's own source, and [`Error::get_ref`] returns it so it can be
    /// downcast. This mirrors how `std::io::Error` wraps a custom error.
    /// Without `alloc` the source is discarded.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// let source = "x".parse::<u32>().unwrap_err();
    /// let e = Error::conversion(source.clone());
    ///
    /// assert_eq!(e.code(), Code::MISMATCH);
    /// assert_eq!(e.to_string(), source.to_string());
    ///
    /// let inner = e.get_ref().and_then(|e| e.downcast_ref::<std::num::ParseIntError>());
    /// assert_eq!(inner, Some(&source));
    /// ```
    #[inline]
    pub fn conversion<E>(source: E) -> Self
    where
        E: error::Error + Send + Sync + 'static,
    {
        #[cfg(feature = "alloc")]
        {
            Self {
                code: Code::MISMATCH,
                repr: Repr::Source(Box::new(source)),
                context: None,
            }
        }

        #[cfg(not(feature = "alloc"))]
        {
            _ = source;
            Self {
                code: Code::MISMATCH,
            }
        }
    }

    /// The error code that caused this error.
    ///
    /// This is an extended code if the connection has [extended result codes]
    /// enabled. Use [`Code::base`] or the classification methods such as
    /// [`Error::is_busy`] to get the same answer either way.
    ///
    /// [extended result codes]: crate::OpenOptions::extended_result_codes
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// let e = Error::mismatch("not a color");
    /// assert_eq!(e.code(), Code::MISMATCH);
    /// ```
    #[inline]
    pub fn code(&self) -> Code {
        self.code
    }

    /// Attach the column the error was raised for.
    ///
    /// The error then displays the column index and, when SQLite knows it,
    /// the column name from [`Statement::column_name`] before its message.
    /// The code, source and [`Error::get_ref`] stay those of the original
    /// error. If the error already names a column, it is kept, since the
    /// innermost column is the one that failed.
    ///
    /// Type checks and the [`Row`] implementations sqll provides already do
    /// this, so it is only needed in hand-written code that reads columns.
    /// Without the `alloc` feature the column is discarded.
    ///
    /// [`Row`]: crate::Row
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Connection, Error};
    ///
    /// let c = Connection::open_in_memory()?;
    /// let mut stmt = c.prepare("SELECT 'red' AS color")?;
    /// assert!(stmt.step()?.is_row());
    ///
    /// let e = Error::mismatch("unknown color").with_column(&stmt, 0);
    /// assert_eq!(e.code(), Code::MISMATCH);
    /// assert_eq!(e.to_string(), "column 0 (`color`): unknown color");
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn with_column(self, stmt: &Statement, column: impl Into<Column>) -> Self {
        #[cfg(feature = "alloc")]
        {
            let mut this = self;
            let context = this.context.get_or_insert_with(Box::default);

            if context.column.is_none() {
                let index = column.into().raw();
                let name = stmt.column_name(Column::from_raw(index));

                context.column = Some(ColumnContext {
                    index,
                    name: name.map(|name| format!("{name}")),
                });
            }

            this
        }

        #[cfg(not(feature = "alloc"))]
        {
            _ = (stmt, column);
            self
        }
    }

    /// Attach the name of the field the error was raised for.
    ///
    /// The [`Row`] derive does this for each field, so an error from a field's
    /// [`FromColumn`] implementation names both the field and the column it
    /// was read from. The code, source and [`Error::get_ref`] stay those of
    /// the original error, and a field name already attached is kept. Without
    /// the `alloc` feature the field name is discarded.
    ///
    /// [`Row`]: crate::Row
    /// [`FromColumn`]: crate::FromColumn
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Error;
    ///
    /// let e = Error::mismatch("unknown color").with_field("color");
    /// assert_eq!(e.to_string(), "field `color`: unknown color");
    /// ```
    #[inline]
    pub fn with_field(self, field: &'static str) -> Self {
        #[cfg(feature = "alloc")]
        {
            let mut this = self;
            let context = this.context.get_or_insert_with(Box::default);

            if context.field.is_none() {
                context.field = Some(field);
            }

            this
        }

        #[cfg(not(feature = "alloc"))]
        {
            _ = field;
            self
        }
    }

    /// Get the source error this error was constructed with through
    /// [`Error::conversion`], if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Error;
    ///
    /// let e = Error::conversion("x".parse::<u32>().unwrap_err());
    /// assert!(e.get_ref().is_some());
    ///
    /// let e = Error::mismatch("not a color");
    /// assert!(e.get_ref().is_none());
    /// ```
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    #[inline]
    pub fn get_ref(&self) -> Option<&(dyn error::Error + Send + Sync + 'static)> {
        match &self.repr {
            Repr::Message(..) => None,
            Repr::Source(source) => Some(&**source),
        }
    }

    /// Test if this is a [`Code::BUSY`] or [`Code::LOCKED`] error, with any
    /// extended code.
    ///
    /// These mean that another connection held a lock the operation needed,
    /// so it can be tried again later. See [`Code::is_busy`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// assert!(Error::new(Code::BUSY_SNAPSHOT, "busy").is_busy());
    /// assert!(Error::new(Code::LOCKED, "locked").is_busy());
    /// assert!(!Error::new(Code::CONSTRAINT, "constraint").is_busy());
    /// ```
    #[inline]
    pub fn is_busy(&self) -> bool {
        self.code.is_busy()
    }

    /// Test if this is a [`Code::CONSTRAINT`] error, with any extended code.
    ///
    /// See [`Code::is_constraint`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// assert!(Error::new(Code::CONSTRAINT_UNIQUE, "unique").is_constraint());
    /// assert!(!Error::new(Code::BUSY, "busy").is_constraint());
    /// ```
    #[inline]
    pub fn is_constraint(&self) -> bool {
        self.code.is_constraint()
    }

    /// Test if this is a [`Code::FULL`] error, meaning the database or a
    /// temporary file could not grow.
    ///
    /// See [`Code::is_full`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Error};
    ///
    /// assert!(Error::new(Code::FULL, "full").is_full());
    /// assert!(!Error::new(Code::BUSY, "busy").is_full());
    /// ```
    #[inline]
    pub fn is_full(&self) -> bool {
        self.code.is_full()
    }
}

impl fmt::Debug for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut st = f.debug_struct("Error");
        st.field("code", &self.code);

        #[cfg(feature = "alloc")]
        match &self.repr {
            Repr::Message(message) => {
                st.field("message", message);
            }
            Repr::Source(source) => {
                st.field("source", source);
            }
        }

        #[cfg(feature = "alloc")]
        if let Some(context) = &self.context {
            st.field("context", context);
        }

        #[cfg(not(feature = "alloc"))]
        st.field("message", &self.code.message());
        st.finish()
    }
}

#[cfg(feature = "alloc")]
impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(context) = &self.context {
            write!(f, "{context}: ")?;
        }

        match &self.repr {
            Repr::Message(message) => message.fmt(f),
            Repr::Source(source) => source.fmt(f),
        }
    }
}

#[cfg(not(feature = "alloc"))]
impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.code.message().fmt(f)
    }
}

impl error::Error for Error {
    #[inline]
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        #[cfg(feature = "alloc")]
        if let Repr::Source(source) = &self.repr {
            return source.source();
        }

        None
    }
}

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

/// Error raised when attempting to convert a database object into a thread-safe
/// container, but the database is not configured to be thread-safe.
pub struct NotThreadSafe {
    kind: NotThreadSafeKind,
}

impl NotThreadSafe {
    /// Construct a new not thread safe error for a connection.
    #[inline]
    pub(super) const fn connection() -> Self {
        Self {
            kind: NotThreadSafeKind::Connection,
        }
    }

    /// Construct a new not thread safe error for a statement.
    #[inline]
    pub(super) const fn statement() -> Self {
        Self {
            kind: NotThreadSafeKind::Statement,
        }
    }
}

impl fmt::Debug for NotThreadSafe {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Display for NotThreadSafe {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a {} object is not thread safe", self.kind)
    }
}

impl core::error::Error for NotThreadSafe {}

#[derive(Debug)]
enum NotThreadSafeKind {
    Connection,
    Statement,
}

impl fmt::Display for NotThreadSafeKind {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotThreadSafeKind::Connection => write!(f, "connection"),
            NotThreadSafeKind::Statement => write!(f, "statement"),
        }
    }
}

/// Error raised when failing to convert a string into a `FixedBlob`.
///
/// # Examples
///
/// ```
/// use sqll::FixedBlob;
///
/// let e = FixedBlob::<3>::try_from(&b"abcd"[..]).unwrap_err();
/// assert_eq!(e.to_string(), "size 4 exceeds fixed buffer size 3");
/// ```
pub struct CapacityError {
    kind: CapacityErrorKind,
}

#[derive(Debug)]
enum CapacityErrorKind {
    Capacity { len: usize, max: usize },
}

impl CapacityError {
    /// Construct a new capacity error.
    #[inline]
    pub(super) fn capacity(len: usize, max: usize) -> Self {
        Self {
            kind: CapacityErrorKind::Capacity { len, max },
        }
    }
}

impl fmt::Display for CapacityError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            CapacityErrorKind::Capacity { len, max } => {
                write!(f, "size {len} exceeds fixed buffer size {max}")
            }
        }
    }
}

impl fmt::Debug for CapacityError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl core::error::Error for CapacityError {}
