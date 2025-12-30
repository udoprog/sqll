use core::ffi::{CStr, c_char, c_double, c_int};
use core::mem::MaybeUninit;
use core::ptr;
use core::slice;

use alloc::string::String;
use alloc::vec::Vec;

use sqlite3_sys as ffi;

use crate::bytes;
use crate::error::{Error, Result};
use crate::utils::{self, sqlite3_try};
use crate::value::{Kind, Type, Value};

/// A prepared statement.
#[repr(transparent)]
pub struct Statement {
    raw: ptr::NonNull<ffi::sqlite3_stmt>,
}

/// A prepared statement is `Send`.
unsafe impl Send for Statement {}

/// A state of a prepared statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum State {
    /// There is a row available for reading.
    Row,
    /// The statement has been entirely evaluated.
    Done,
}

/// A type suitable for binding to a prepared statement.
pub trait Bindable {
    /// Bind to a parameter.
    ///
    /// The first parameter has index 1.
    fn bind(self, _: &mut Statement, _: usize) -> Result<()>;
}

/// A type suitable for reading from a prepared statement.
pub trait Readable: Sized {
    /// Read from a column.
    ///
    /// The first column has index 0.
    fn read(_: &Statement, _: usize) -> Result<Self>;
}

impl Statement {
    /// Construct a statement from a raw pointer.
    #[inline]
    pub(crate) fn from_raw(raw: ptr::NonNull<ffi::sqlite3_stmt>) -> Statement {
        Statement { raw }
    }

    /// Bind a value to a parameter by index.
    ///
    /// The first parameter has index 1.
    #[inline]
    pub fn bind<T: Bindable>(&mut self, i: usize, value: T) -> Result<()> {
        value.bind(self, i)
    }

    /// Bind a value to a parameter by name.
    ///
    /// # Examples
    ///
    /// ```
    /// # let connection = sqlite_ll::Connection::open(":memory:")?;
    /// # connection.execute("CREATE TABLE users (name STRING)");
    /// let mut statement = unsafe { connection.prepare("SELECT * FROM users WHERE name = :name")? };
    /// statement.bind_by_name(c":name", "Bob")?;
    /// # Ok::<(), sqlite_ll::Error>(())
    /// ```
    pub fn bind_by_name(&mut self, name: impl AsRef<CStr>, value: impl Bindable) -> Result<()> {
        if let Some(i) = self.parameter_index(name) {
            self.bind(i, value)?;
            Ok(())
        } else {
            Err(Error::new(ffi::SQLITE_MISMATCH))
        }
    }

    /// Return the number of columns.
    #[inline]
    pub fn column_count(&self) -> usize {
        unsafe { ffi::sqlite3_column_count(self.raw.as_ptr()) as usize }
    }

    /// Return the name of a column.
    ///
    /// The first column has index 0.
    #[inline]
    pub fn column_name(&self, i: usize) -> Result<&str> {
        debug_assert!(
            i < self.column_count(),
            "the index is out of bounds 0..{}",
            self.column_count()
        );

        unsafe {
            let pointer = ffi::sqlite3_column_name(self.raw.as_ptr(), i as c_int);

            if pointer.is_null() {
                let handle = ffi::sqlite3_db_handle(self.raw.as_ptr());
                let code = ffi::sqlite3_errcode(handle);
                return Err(Error::new(code));
            }

            utils::cstr_to_str(pointer)
        }
    }

    /// Return column names.
    #[inline]
    pub fn column_names(&self) -> Result<Vec<&str>> {
        (0..self.column_count())
            .map(|i| self.column_name(i))
            .collect()
    }

    /// Return the type of a column.
    ///
    /// The first column has index 0. The type becomes available after taking a step.
    pub fn column_type(&self, i: usize) -> Type {
        debug_assert!(
            i < self.column_count(),
            "the index is out of bounds 0..{}",
            self.column_count()
        );

        match unsafe { ffi::sqlite3_column_type(self.raw.as_ptr(), i as c_int) } {
            ffi::SQLITE_BLOB => Type::Blob,
            ffi::SQLITE_FLOAT => Type::Float,
            ffi::SQLITE_INTEGER => Type::Integer,
            ffi::SQLITE_TEXT => Type::Text,
            ffi::SQLITE_NULL => Type::Null,
            _ => Type::Unknown,
        }
    }

    /// Step to the next state.
    ///
    /// The function should be called multiple times until `State::Done` is
    /// reached in order to evaluate the statement entirely.
    pub fn step(&mut self) -> Result<State> {
        unsafe {
            match ffi::sqlite3_step(self.raw.as_ptr()) {
                ffi::SQLITE_ROW => Ok(State::Row),
                ffi::SQLITE_DONE => Ok(State::Done),
                _ => {
                    let handle = ffi::sqlite3_db_handle(self.raw.as_ptr());
                    let code = ffi::sqlite3_errcode(handle);
                    Err(Error::new(code))
                }
            }
        }
    }

    /// Return the index for a named parameter if exists.
    ///
    /// Note that this takes a c-string as the parameter name since that is what
    /// the underlying API expects. To accomodate this, you can make use of the
    /// `c"string"` syntax.
    ///
    /// # Examples
    ///
    /// ```
    /// # let connection = sqlite_ll::Connection::open(":memory:")?;
    /// # connection.execute("CREATE TABLE users (name STRING)");
    /// let statement = unsafe { connection.prepare("SELECT * FROM users WHERE name = :name")? };
    /// assert_eq!(statement.parameter_index(c":name"), Some(1));
    /// assert_eq!(statement.parameter_index(c":asdf"), None);
    /// # Ok::<(), sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn parameter_index(&self, parameter: impl AsRef<CStr>) -> Option<usize> {
        let index = unsafe {
            ffi::sqlite3_bind_parameter_index(self.raw.as_ptr(), parameter.as_ref().as_ptr())
        };

        match index {
            0 => None,
            _ => Some(index as usize),
        }
    }

    /// Read a value from a column.
    ///
    /// The first column has index 0.
    #[inline]
    pub fn read<T>(&self, i: usize) -> Result<T>
    where
        T: Readable,
    {
        debug_assert!(
            i < self.column_count(),
            "the index is out of bounds 0..{}",
            self.column_count()
        );
        Readable::read(self, i)
    }

    /// Reset the statement allowing it to be re-used.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, State};
    ///
    /// let connection = Connection::memory()?;
    ///
    /// connection.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = connection.prepare("SELECT * FROM users WHERE age > ?")?;
    ///
    /// let mut results = Vec::new();
    ///
    /// for age in [30, 50] {
    ///     stmt.reset()?;
    ///     stmt.bind(1, age)?;
    ///
    ///     while let State::Row = stmt.step()? {
    ///         results.push((stmt.read::<String>(0)?, stmt.read::<i64>(1)?));
    ///     }
    /// }
    ///
    /// let expected = vec![
    ///     (String::from("Alice"), 72),
    ///     (String::from("Bob"), 40),
    ///     (String::from("Alice"), 72),
    /// ];
    ///
    /// assert_eq!(results, expected);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn reset(&mut self) -> Result<()> {
        unsafe { ffi::sqlite3_reset(self.raw.as_ptr()) };
        Ok(())
    }
}

impl Drop for Statement {
    #[inline]
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_finalize(self.raw.as_ptr()) };
    }
}

impl Bindable for &Value {
    fn bind(self, statement: &mut Statement, i: usize) -> Result<()> {
        match &self.kind {
            Kind::Blob(value) => value.as_slice().bind(statement, i),
            Kind::Float(value) => value.bind(statement, i),
            Kind::Integer(value) => value.bind(statement, i),
            Kind::Text(value) => value.as_str().bind(statement, i),
            Kind::Null => ().bind(statement, i),
        }
    }
}

impl Bindable for &[u8] {
    #[inline]
    fn bind(self, statement: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");
        let (data, dealloc) = bytes::alloc(self)?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(statement.raw.as_ptr()),
                ffi::sqlite3_bind_blob(
                    statement.raw.as_ptr(),
                    i as c_int,
                    data,
                    self.len() as c_int,
                    dealloc,
                )
            };
        }

        Ok(())
    }
}

impl Bindable for f64 {
    #[inline]
    fn bind(self, statement: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(statement.raw.as_ptr()),
                ffi::sqlite3_bind_double(
                    statement.raw.as_ptr(),
                    i as c_int,
                    self as c_double
                )
            };
        }

        Ok(())
    }
}

impl Bindable for i64 {
    #[inline]
    fn bind(self, statement: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(statement.raw.as_ptr()),
                ffi::sqlite3_bind_int64(
                    statement.raw.as_ptr(),
                    i as c_int,
                    self as ffi::sqlite3_int64
                )
            };
        }

        Ok(())
    }
}

impl Bindable for &str {
    #[inline]
    fn bind(self, statement: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");
        let (data, dealloc) = bytes::alloc(self.as_bytes())?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(statement.raw.as_ptr()),
                ffi::sqlite3_bind_text(
                    statement.raw.as_ptr(),
                    i as c_int,
                    data.cast(),
                    self.len() as c_int,
                    dealloc,
                )
            };
        }

        Ok(())
    }
}

impl Bindable for () {
    #[inline]
    fn bind(self, statement: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(statement.raw.as_ptr()),
                ffi::sqlite3_bind_null(statement.raw.as_ptr(), i as c_int)
            };
        }

        Ok(())
    }
}

impl<T> Bindable for Option<T>
where
    T: Bindable,
{
    #[inline]
    fn bind(self, statement: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");
        match self {
            Some(inner) => Bindable::bind(inner, statement, i),
            None => Bindable::bind((), statement, i),
        }
    }
}

impl Readable for Value {
    fn read(statement: &Statement, i: usize) -> Result<Self> {
        let value = match statement.column_type(i) {
            Type::Blob => Value::blob(Readable::read(statement, i)?),
            Type::Text => Value::text(Readable::read(statement, i)?),
            Type::Float => Value::float(Readable::read(statement, i)?),
            Type::Integer => Value::integer(Readable::read(statement, i)?),
            Type::Null => Value::null(),
            Type::Unknown => return Err(Error::new(ffi::SQLITE_MISMATCH)),
        };

        Ok(value)
    }
}

impl Readable for f64 {
    #[inline]
    fn read(statement: &Statement, i: usize) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_double(statement.raw.as_ptr(), i as c_int) })
    }
}

impl Readable for i64 {
    #[inline]
    fn read(statement: &Statement, i: usize) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_int64(statement.raw.as_ptr(), i as c_int) })
    }
}

impl Readable for String {
    #[inline]
    fn read(statement: &Statement, i: usize) -> Result<Self> {
        unsafe {
            let pointer = ffi::sqlite3_column_text(statement.raw.as_ptr(), i as c_int);

            if pointer.is_null() {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            }

            Ok(String::from(utils::cstr_to_str(pointer as *const c_char)?))
        }
    }
}

impl Readable for Vec<u8> {
    #[inline]
    fn read(statement: &Statement, i: usize) -> Result<Self> {
        unsafe {
            let pointer = ffi::sqlite3_column_blob(statement.raw.as_ptr(), i as c_int);

            if pointer.is_null() {
                return Ok(Vec::new());
            }

            let count = ffi::sqlite3_column_bytes(statement.raw.as_ptr(), i as c_int) as usize;
            let mut buffer = Vec::with_capacity(count);
            ptr::copy_nonoverlapping(pointer as *const u8, buffer.as_mut_ptr(), count);
            buffer.set_len(count);
            Ok(buffer)
        }
    }
}

/// A helper to read at most a fixed number of `N` bytes from a column. This
/// allocates the storage for the bytes read on the stack.
pub struct FixedBytes<const N: usize> {
    /// Storage to read to.
    data: [MaybeUninit<u8>; N],
    /// Number of bytes initialized.
    init: usize,
}

impl<const N: usize> FixedBytes<N> {
    /// Coerce into the underlying bytes if all of them have been initialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use sqlite_ll::{Connection, State, FixedBytes};
    ///
    /// let c: Connection = todo!();
    /// let stmt = unsafe { c.prepare("SELECT id FROM users")? };
    ///
    /// while let State::Row = stmt.step()? {
    ///     let id = stmt.read::<FixedBytes<16>>(0)?;
    ///
    ///     // Note: we have to check the result of `into_bytes` to ensure that the field contained exactly 16 bytes.
    ///     let bytes: [u8; 16] = match id.into_bytes() {
    ///         Some(bytes) => bytes,
    ///         None => continue,
    ///     };
    ///
    ///     /* use bytes */
    /// }
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    pub fn into_bytes(self) -> Option<[u8; N]> {
        if self.init == N {
            // SAFETY: All of the bytes in the sequence have been initialized
            // and can be safety transmuted.
            //
            // Method of transmuting comes from the implementation of
            // `MaybeUninit::array_assume_init` which is not yet stable.
            unsafe { Some((&self.data as *const _ as *const [u8; N]).read()) }
        } else {
            None
        }
    }

    /// Coerce into the slice of initialized memory which is present.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use sqlite_ll::{Connection, State, FixedBytes};
    ///
    /// let c: Connection = todo!();
    /// let stmt = unsafe { c.prepare("SELECT id FROM users")? };
    ///
    /// while let State::Row = stmt.step()? {
    ///     let id = stmt.read::<FixedBytes<16>>(0)?;
    ///     let bytes: &[u8] = id.as_bytes();
    ///
    ///     /* use bytes */
    /// }
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        if self.init == 0 {
            return &[];
        }

        // SAFETY: We've asserted that `initialized` accounts for the number of
        // bytes that have been initialized.
        unsafe { slice::from_raw_parts(self.data.as_ptr() as *const u8, self.init) }
    }
}

impl<const N: usize> Readable for FixedBytes<N> {
    #[inline]
    fn read(statement: &Statement, i: usize) -> Result<Self> {
        let mut bytes = FixedBytes {
            // SAFETY: this is safe as per `MaybeUninit::uninit_array`, which isn't stable (yet).
            data: unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() },
            init: 0,
        };

        unsafe {
            let pointer = ffi::sqlite3_column_blob(statement.raw.as_ptr(), i as c_int);

            if pointer.is_null() {
                return Ok(bytes);
            }

            let count = ffi::sqlite3_column_bytes(statement.raw.as_ptr(), i as c_int) as usize;
            let copied = usize::min(N, count);

            ptr::copy_nonoverlapping(
                pointer as *const u8,
                bytes.data.as_mut_ptr() as *mut u8,
                copied,
            );

            bytes.init = copied;
            Ok(bytes)
        }
    }
}

impl<T> Readable for Option<T>
where
    T: Readable,
{
    #[inline]
    fn read(statement: &Statement, i: usize) -> Result<Self> {
        if statement.column_type(i) == Type::Null {
            Ok(None)
        } else {
            T::read(statement, i).map(Some)
        }
    }
}
