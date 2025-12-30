use core::ffi::c_int;
use core::ptr;

use alloc::string::String;
use alloc::vec::Vec;

use sqlite3_sys as ffi;

use crate::{Error, FixedBytes, Null, Result, Statement, Type, Value, Writable};

mod sealed {
    use alloc::string::String;
    use alloc::vec::Vec;

    use crate::{FixedBytes, Null, Value};

    pub trait Sealed {}
    impl Sealed for i64 {}
    impl Sealed for f64 {}
    impl Sealed for Null {}
    impl Sealed for String {}
    impl Sealed for Vec<u8> {}
    impl<T> Sealed for Option<T> where T: Sealed {}
    impl<const N: usize> Sealed for FixedBytes<N> {}
    impl Sealed for Value {}
}

/// A type suitable for reading from a prepared statement.
///
/// Use with [`Statement::read`].
pub trait Readable
where
    Self: self::sealed::Sealed + Sized,
{
    #[doc(hidden)]
    fn read(stmt: &Statement, index: c_int) -> Result<Self>;
}

impl Readable for Value {
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let value = match stmt.column_type(index) {
            Type::BLOB => Value::blob(<_>::read(stmt, index)?),
            Type::TEXT => Value::text(<_>::read(stmt, index)?),
            Type::FLOAT => Value::float(<_>::read(stmt, index)?),
            Type::INTEGER => Value::integer(<_>::read(stmt, index)?),
            Type::NULL => Value::null(),
            _ => return Err(Error::new(ffi::SQLITE_MISMATCH)),
        };

        Ok(value)
    }
}

/// [`Readable`] implementation for `f64`.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r##"
/// CREATE TABLE numbers (value REAL);
/// INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let State::Row = stmt.step()? {
///     let value = stmt.read::<f64>(0)?;
///     assert!(matches!(value, 3.14 | 2.71));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Readable for f64 {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_double(stmt.as_ptr(), index) })
    }
}

/// [`Readable`] implementation for `i64`.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r##"
/// CREATE TABLE numbers (value INTEGER);
/// INSERT INTO numbers (value) VALUES (3), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let State::Row = stmt.step()? {
///     let value = stmt.read::<i64>(0)?;
///     assert!(matches!(value, 3 | 2));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Readable for i64 {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.as_ptr(), index) })
    }
}

/// [`Readable`] implementation which returns a newly allocated [`String`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Writable`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<String>(0)?;
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<String>(0)?;
///     assert!(matches!(name.as_str(), "1" | "2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Readable for String {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut s = String::new();
        s.write(stmt, index)?;
        Ok(s)
    }
}

/// [`Readable`] implementation which returns a newly allocated [`Vec`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Writable`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<Vec<u8>>(0)?;
///     assert!(matches!(name.as_slice(), b"Alice" | b"Bob"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r##"
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.read::<Vec::<u8>>(0)?;
///     assert!(matches!(name.as_slice(), b"1" | b"2"));
/// }
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Readable for Vec<u8> {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut buf = Vec::new();
        buf.write(stmt, index)?;
        Ok(buf)
    }
}

/// [`Readable`] implementation for [`FixedBytes`] which reads at most `N`
/// bytes.
///
/// If the column contains more than `N` bytes, a [`Code::MISMATCH`] error is
/// returned.
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State, FixedBytes, Code};
///
/// let c = Connection::open_memory()?;
/// c.execute(r##"
/// CREATE TABLE users (id BLOB);
/// INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// assert_eq!(stmt.step()?, State::Row);
/// let bytes = stmt.read::<FixedBytes<4>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[1, 2, 3, 4]);
///
/// assert_eq!(stmt.step()?, State::Row);
/// let e = stmt.read::<FixedBytes<4>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.read::<FixedBytes<5>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[5, 6, 7, 8, 9]);
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl<const N: usize> Readable for FixedBytes<N> {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut bytes = FixedBytes::new();

        unsafe {
            let ptr = ffi::sqlite3_column_blob(stmt.as_ptr(), index);

            if ptr.is_null() {
                return Ok(bytes);
            }

            let Ok(len) = usize::try_from(ffi::sqlite3_column_bytes(stmt.as_ptr(), index)) else {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            };

            if len > N {
                return Err(Error::new(ffi::SQLITE_MISMATCH));
            }

            ptr::copy_nonoverlapping(ptr.cast::<u8>(), bytes.as_mut_ptr(), len);

            bytes.set_len(len);
            Ok(bytes)
        }
    }
}

/// [`Readable`] implementation for [`Null`].
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, Null, State};
///
/// let c = Connection::open_memory()?;
/// c.execute(r##"
/// CREATE TABLE users (name TEXT, age INTEGER);
/// INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "##)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind(1, "Alice")?;
///
/// let mut names = Vec::new();
///
/// while let State::Row = stmt.step()? {
///     names.push(stmt.read::<Null>(0)?);
/// }
///
/// assert_eq!(names, vec![Null]);
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl Readable for Null {
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == Type::NULL {
            Ok(Null)
        } else {
            Err(Error::new(ffi::SQLITE_MISMATCH))
        }
    }
}

/// [`Readable`] implementation for [`Option`].
///
/// # Examples
///
/// ```
/// use sqlite_ll::{Connection, State};
///
/// let c = Connection::open_memory()?;
/// c.execute(r##"
/// CREATE TABLE users (name TEXT, age INTEGER);
/// "##)?;
///
/// let mut stmt = c.prepare("INSERT INTO users (name, age) VALUES (?, ?)")?;
///
/// stmt.reset()?;
/// stmt.bind(1, "Alice")?;
/// stmt.bind(2, None::<i64>)?;
/// assert_eq!(stmt.step()?, State::Done);
///
/// stmt.reset()?;
/// stmt.bind(1, "Bob")?;
/// stmt.bind(2, Some(30i64))?;
/// assert_eq!(stmt.step()?, State::Done);
///
/// let mut stmt = c.prepare("SELECT name, age FROM users")?;
///
/// let mut names_and_ages = Vec::new();
///
/// while let State::Row = stmt.step()? {
///     let name: String = stmt.read(0)?;
///     let age: Option<i64> = stmt.read(1)?;
///     names_and_ages.push((name, age));
/// }
///
/// names_and_ages.sort();
/// assert_eq!(names_and_ages, vec![(String::from("Alice"), None), (String::from("Bob"), Some(30))]);
/// # Ok::<_, sqlite_ll::Error>(())
/// ```
impl<T> Readable for Option<T>
where
    T: Readable,
{
    #[inline]
    fn read(stmt: &Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == Type::NULL {
            Ok(None)
        } else {
            T::read(stmt, index).map(Some)
        }
    }
}
