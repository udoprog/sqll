use core::ffi::{CStr, c_char, c_double, c_int};
use core::fmt;
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

/// A marker type representing a NULL value.
pub struct Null;

/// A prepared statement.
#[repr(transparent)]
pub struct Statement {
    raw: ptr::NonNull<ffi::sqlite3_stmt>,
}

impl fmt::Debug for Statement {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Statement").finish_non_exhaustive()
    }
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
    fn bind(&self, _: &mut Statement, _: usize) -> Result<()>;
}

impl<T> Bindable for &T
where
    T: ?Sized + Bindable,
{
    #[inline]
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        (**self).bind(stmt, i)
    }
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
    pub fn bind<T>(&mut self, i: usize, value: T) -> Result<()>
    where
        T: Bindable,
    {
        value.bind(self, i)
    }

    /// Bind a value to a parameter by name.
    ///
    /// # Examples
    ///
    /// ```
    /// # let c = sqlite_ll::Connection::open(":memory:")?;
    /// # c.execute("CREATE TABLE users (name STRING)");
    /// let mut statement = unsafe { c.prepare("SELECT * FROM users WHERE name = :name")? };
    /// statement.bind_by_name(c":name", "Bob")?;
    /// # Ok::<_, sqlite_ll::Error>(())
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

    /// Return an iterator of column names.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::Connection;
    ///
    /// let c = Connection::memory()?;
    /// c.execute("CREATE TABLE users (name TEXT, age INTEGER);")?;
    /// let stmt = c.prepare("SELECT * FROM users;")?;
    ///
    /// let column_names: Vec<&str> = stmt.column_names().into_iter().collect();
    /// assert_eq!(column_names, vec!["name", "age"]);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn column_names(&self) -> ColumnNames<'_> {
        ColumnNames {
            stmt: self,
            start: 0,
            end: self.column_count(),
        }
    }

    /// Return the type of a column.
    ///
    /// The first column has index 0. The type becomes available after taking a
    /// step.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqlite_ll::{Connection, Type, State};
    ///
    /// let mut c = Connection::memory()?;
    ///
    /// c.execute(r##"
    /// CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age REAL, photo BLOB);
    /// "##)?;
    ///
    /// c.execute(r##"
    /// INSERT INTO users (id, name, age, photo) VALUES (1, 'Bob', 30.5, X'01020304');
    /// "##)?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users")?;
    ///
    /// assert_eq!(stmt.column_type(0), Type::NULL);
    /// assert_eq!(stmt.column_type(1), Type::NULL);
    /// assert_eq!(stmt.column_type(2), Type::NULL);
    /// assert_eq!(stmt.column_type(3), Type::NULL);
    ///
    /// assert_eq!(stmt.step()?, State::Row);
    ///
    /// assert_eq!(stmt.column_type(0), Type::INTEGER);
    /// assert_eq!(stmt.column_type(1), Type::TEXT);
    /// assert_eq!(stmt.column_type(2), Type::FLOAT);
    /// assert_eq!(stmt.column_type(3), Type::BLOB);
    /// // Since the fifth column does not exist it is always `Null`.
    /// assert_eq!(stmt.column_type(4), Type::NULL);
    /// # Ok::<_, sqlite_ll::Error>(())
    /// ```
    #[inline]
    pub fn column_type(&self, i: usize) -> Type {
        unsafe {
            let i = c_int::try_from(i).unwrap_or(c_int::MAX);
            Type::from_raw(ffi::sqlite3_column_type(self.raw.as_ptr(), i))
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
    /// # let c = sqlite_ll::Connection::open(":memory:")?;
    /// c.execute("CREATE TABLE users (name STRING)");
    /// let stmt = c.prepare("SELECT * FROM users WHERE name = :name")?;
    /// assert_eq!(stmt.parameter_index(c":name"), Some(1));
    /// assert_eq!(stmt.parameter_index(c":asdf"), None);
    /// # Ok::<_, sqlite_ll::Error>(())
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
    /// let c = Connection::memory()?;
    ///
    /// c.execute(
    ///     "
    ///     CREATE TABLE users (name TEXT, age INTEGER);
    ///     INSERT INTO users VALUES ('Alice', 72);
    ///     INSERT INTO users VALUES ('Bob', 40);
    ///     ",
    /// )?;
    ///
    /// let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
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

impl Bindable for Value {
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        match &self.kind {
            Kind::Blob(value) => value.as_slice().bind(stmt, i),
            Kind::Float(value) => value.bind(stmt, i),
            Kind::Integer(value) => value.bind(stmt, i),
            Kind::Text(value) => value.as_str().bind(stmt, i),
            Kind::Null => Null.bind(stmt, i),
        }
    }
}

impl Bindable for &[u8] {
    #[inline]
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");
        let (data, dealloc) = bytes::alloc(self)?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(stmt.raw.as_ptr()),
                ffi::sqlite3_bind_blob(
                    stmt.raw.as_ptr(),
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
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(stmt.raw.as_ptr()),
                ffi::sqlite3_bind_double(
                    stmt.raw.as_ptr(),
                    i as c_int,
                    *self as c_double
                )
            };
        }

        Ok(())
    }
}

impl Bindable for i64 {
    #[inline]
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(stmt.raw.as_ptr()),
                ffi::sqlite3_bind_int64(
                    stmt.raw.as_ptr(),
                    i as c_int,
                    *self as ffi::sqlite3_int64
                )
            };
        }

        Ok(())
    }
}

impl Bindable for str {
    #[inline]
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");
        let (data, dealloc) = bytes::alloc(self.as_bytes())?;

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(stmt.raw.as_ptr()),
                ffi::sqlite3_bind_text(
                    stmt.raw.as_ptr(),
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

impl Bindable for Null {
    #[inline]
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");

        unsafe {
            sqlite3_try! {
                ffi::sqlite3_db_handle(stmt.raw.as_ptr()),
                ffi::sqlite3_bind_null(stmt.raw.as_ptr(), i as c_int)
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
    fn bind(&self, stmt: &mut Statement, i: usize) -> Result<()> {
        debug_assert!(i > 0, "the indexing starts from 1");
        match self {
            Some(inner) => Bindable::bind(inner, stmt, i),
            None => Bindable::bind(&Null, stmt, i),
        }
    }
}

impl Readable for Value {
    fn read(stmt: &Statement, i: usize) -> Result<Self> {
        let value = match stmt.column_type(i) {
            Type::BLOB => Value::blob(Readable::read(stmt, i)?),
            Type::TEXT => Value::text(Readable::read(stmt, i)?),
            Type::FLOAT => Value::float(Readable::read(stmt, i)?),
            Type::INTEGER => Value::integer(Readable::read(stmt, i)?),
            Type::NULL => Value::null(),
            _ => return Err(Error::new(ffi::SQLITE_MISMATCH)),
        };

        Ok(value)
    }
}

impl Readable for f64 {
    #[inline]
    fn read(stmt: &Statement, i: usize) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_double(stmt.raw.as_ptr(), i as c_int) })
    }
}

impl Readable for i64 {
    #[inline]
    fn read(stmt: &Statement, i: usize) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.raw.as_ptr(), i as c_int) })
    }
}

impl Readable for String {
    #[inline]
    fn read(stmt: &Statement, i: usize) -> Result<Self> {
        unsafe {
            let pointer = ffi::sqlite3_column_text(stmt.raw.as_ptr(), i as c_int);

            if pointer.is_null() {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            }

            Ok(String::from(utils::cstr_to_str(pointer as *const c_char)?))
        }
    }
}

impl Readable for Vec<u8> {
    #[inline]
    fn read(stmt: &Statement, i: usize) -> Result<Self> {
        unsafe {
            let pointer = ffi::sqlite3_column_blob(stmt.raw.as_ptr(), i as c_int);

            if pointer.is_null() {
                return Ok(Vec::new());
            }

            let count = ffi::sqlite3_column_bytes(stmt.raw.as_ptr(), i as c_int) as usize;
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
    fn read(stmt: &Statement, i: usize) -> Result<Self> {
        let mut bytes = FixedBytes {
            // SAFETY: this is safe as per `MaybeUninit::uninit_array`, which isn't stable (yet).
            data: unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() },
            init: 0,
        };

        unsafe {
            let pointer = ffi::sqlite3_column_blob(stmt.raw.as_ptr(), i as c_int);

            if pointer.is_null() {
                return Ok(bytes);
            }

            let count = ffi::sqlite3_column_bytes(stmt.raw.as_ptr(), i as c_int) as usize;
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
    fn read(stmt: &Statement, i: usize) -> Result<Self> {
        if stmt.column_type(i) == Type::NULL {
            Ok(None)
        } else {
            T::read(stmt, i).map(Some)
        }
    }
}

pub struct ColumnNames<'a> {
    stmt: &'a Statement,
    start: usize,
    end: usize,
}

impl<'a> Iterator for ColumnNames<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }

        let name = self.stmt.column_name(self.start).ok()?;
        self.start += 1;
        Some(name)
    }
}

impl<'a> ExactSizeIterator for ColumnNames<'a> {
    fn len(&self) -> usize {
        self.end - self.start
    }
}

impl<'a> DoubleEndedIterator for ColumnNames<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }

        self.end -= 1;
        let name = self.stmt.column_name(self.end).ok()?;
        Some(name)
    }
}
