use core::ffi::c_int;
use core::ptr;

use alloc::string::String;
use alloc::vec::Vec;

use crate::ffi;
use crate::{Borrowable, Code, Error, FixedBytes, Null, Result, Sink, Statement, Type, Value};

mod sealed {
    use alloc::string::String;
    use alloc::vec::Vec;

    use crate::{FixedBytes, Null, Value};

    pub trait Sealed<'stmt> {}
    impl Sealed<'_> for i8 {}
    impl Sealed<'_> for i16 {}
    impl Sealed<'_> for i32 {}
    impl Sealed<'_> for i64 {}
    impl Sealed<'_> for i128 {}
    impl Sealed<'_> for u8 {}
    impl Sealed<'_> for u16 {}
    impl Sealed<'_> for u32 {}
    impl Sealed<'_> for u64 {}
    impl Sealed<'_> for u128 {}
    impl Sealed<'_> for f32 {}
    impl Sealed<'_> for f64 {}
    impl Sealed<'_> for Null {}
    impl<'stmt> Sealed<'stmt> for &'stmt str {}
    impl Sealed<'_> for String {}
    impl<'stmt> Sealed<'stmt> for &'stmt [u8] {}
    impl Sealed<'_> for Vec<u8> {}
    impl<'stmt, T> Sealed<'stmt> for Option<T> where T: Sealed<'stmt> {}
    impl<const N: usize> Sealed<'_> for FixedBytes<N> {}
    impl Sealed<'_> for Value {}
}

/// A type suitable for reading from a prepared statement.
///
/// Use with [`Statement::get`].
pub trait Gettable<'stmt>
where
    Self: self::sealed::Sealed<'stmt> + Sized,
{
    #[doc(hidden)]
    fn get(stmt: &'stmt Statement, index: c_int) -> Result<Self>;
}

/// [`Gettable`] implementation for [`Null`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Null, State};
///
/// let c = Connection::open_memory()?;
/// c.execute("
/// CREATE TABLE users (name TEXT, age INTEGER);
/// INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind(1, "Alice")?;
///
/// let mut names = Vec::new();
///
/// while let State::Row = stmt.step()? {
///     names.push(stmt.get::<Null>(0)?);
/// }
///
/// assert_eq!(names, vec![Null]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for Null {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == Type::NULL {
            Ok(Null)
        } else {
            Err(Error::new(Code::MISMATCH))
        }
    }
}

/// [`Gettable`] implementation for [`Value`].
impl Gettable<'_> for Value {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        let value = match stmt.column_type(index) {
            Type::BLOB => Value::blob(<_>::get(stmt, index)?),
            Type::TEXT => Value::text(<_>::get(stmt, index)?),
            Type::FLOAT => Value::float(<_>::get(stmt, index)?),
            Type::INTEGER => Value::integer(<_>::get(stmt, index)?),
            Type::NULL => Value::null(),
            _ => return Err(Error::new(Code::MISMATCH)),
        };

        Ok(value)
    }
}

/// [`Gettable`] implementation for `f64`.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE numbers (value REAL);
/// INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let State::Row = stmt.step()? {
///     let value = stmt.get::<f64>(0)?;
///     assert!(matches!(value, 3.14 | 2.71));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for f64 {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_double(stmt.as_ptr(), index) })
    }
}

/// [`Gettable`] implementation for `i64`.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE numbers (value INTEGER);
/// INSERT INTO numbers (value) VALUES (3), (2);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let State::Row = stmt.step()? {
///     let value = stmt.get::<i64>(0)?;
///     assert!(matches!(value, 3 | 2));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for i64 {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.as_ptr(), index) })
    }
}

macro_rules! integer {
    ($ty:ty) => {
        #[doc = concat!(" [`Gettable`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::{Connection, State};
        ///
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute("
        /// CREATE TABLE numbers (value INTEGER);
        /// INSERT INTO numbers (value) VALUES (3), (2);
        /// ")?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while let State::Row = stmt.step()? {
        #[doc = concat!("     let value = stmt.get::<", stringify!($ty), ">(0)?;")]
        ///     assert!(matches!(value, 3 | 2));
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl Gettable<'_> for $ty {
            #[inline]
            #[allow(irrefutable_let_patterns)]
            fn get(stmt: &Statement, index: c_int) -> Result<Self> {
                let value = i64::get(stmt, index)?;

                let Ok(value) = <$ty>::try_from(value) else {
                    return Err(Error::new(Code::MISMATCH));
                };

                Ok(value)
            }
        }
    };
}

integer!(i8);
integer!(i16);
integer!(i32);
integer!(i128);
integer!(u8);
integer!(u16);
integer!(u32);
integer!(u64);
integer!(u128);

/// [`Gettable`] implementation which returns a borrowed [`str`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// ")?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.get::<String>(0)?;
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let name = row.get::<&str>(0)?;
///     assert!(matches!(name, "1" | "2"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> Gettable<'stmt> for &'stmt str {
    #[inline]
    fn get(stmt: &'stmt Statement, index: c_int) -> Result<Self> {
        Borrowable::borrow(stmt, index)
    }
}

/// [`Gettable`] implementation which returns a newly allocated [`String`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Sink`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// ")?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.get::<String>(0)?;
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let name = row.get::<String>(0)?;
///     assert!(matches!(name.as_str(), "1" | "2"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for String {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut s = String::new();
        s.write(stmt, index)?;
        Ok(s)
    }
}

/// [`Gettable`] implementation which returns a newly allocated [`Vec`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Sink`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// ")?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.get::<Vec<u8>>(0)?;
///     assert!(matches!(name.as_slice(), b"Alice" | b"Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let name = row.get::<Vec::<u8>>(0)?;
///     assert!(matches!(name.as_slice(), b"1" | b"2"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for Vec<u8> {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut buf = Vec::new();
        buf.write(stmt, index)?;
        Ok(buf)
    }
}

/// [`Gettable`] implementation which returns a borrowed `[u8]`.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (name TEXT);
/// INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// ")?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let State::Row = stmt.step()? {
///     let name = stmt.get::<Vec<u8>>(0)?;
///     assert!(matches!(name.as_slice(), b"Alice" | b"Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion:
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
///
/// c.execute("
/// CREATE TABLE users (id INTEGER);
/// INSERT INTO users (id) VALUES (1), (2);
/// ")?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let name = row.get::<&[u8]>(0)?;
///     assert!(matches!(name, b"1" | b"2"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> Gettable<'stmt> for &'stmt [u8] {
    #[inline]
    fn get(stmt: &'stmt Statement, index: c_int) -> Result<Self> {
        Borrowable::borrow(stmt, index)
    }
}

/// [`Gettable`] implementation for [`FixedBytes`] which reads at most `N`
/// bytes.
///
/// If the column contains more than `N` bytes, a [`Code::MISMATCH`] error is
/// returned.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State, FixedBytes, Code};
///
/// let c = Connection::open_memory()?;
/// c.execute("
/// CREATE TABLE users (id BLOB);
/// INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
/// ")?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// assert_eq!(stmt.step()?, State::Row);
/// let bytes = stmt.get::<FixedBytes<4>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[1, 2, 3, 4]);
///
/// assert_eq!(stmt.step()?, State::Row);
/// let e = stmt.get::<FixedBytes<4>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.get::<FixedBytes<5>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[5, 6, 7, 8, 9]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> Gettable<'_> for FixedBytes<N> {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut bytes = FixedBytes::new();

        unsafe {
            let ptr = ffi::sqlite3_column_blob(stmt.as_ptr(), index);

            if ptr.is_null() {
                return Ok(bytes);
            }

            let Ok(len) = usize::try_from(ffi::sqlite3_column_bytes(stmt.as_ptr(), index)) else {
                return Err(Error::new(Code::MISMATCH));
            };

            if len > N {
                return Err(Error::new(Code::MISMATCH));
            }

            ptr::copy_nonoverlapping(ptr.cast::<u8>(), bytes.as_mut_ptr(), len);

            bytes.set_len(len);
            Ok(bytes)
        }
    }
}

/// [`Gettable`] implementation for [`Option`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, State};
///
/// let c = Connection::open_memory()?;
/// c.execute("
/// CREATE TABLE users (name TEXT, age INTEGER);
/// ")?;
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
/// while let Some(row) = stmt.next()? {
///     let name = row.get::<String>(0)?;
///     let age = row.get::<Option<i64>>(1)?;
///     names_and_ages.push((name, age));
/// }
///
/// names_and_ages.sort();
/// assert_eq!(names_and_ages, vec![(String::from("Alice"), None), (String::from("Bob"), Some(30))]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt, T> Gettable<'stmt> for Option<T>
where
    T: Gettable<'stmt>,
{
    #[inline]
    fn get(stmt: &'stmt Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == Type::NULL {
            Ok(None)
        } else {
            T::get(stmt, index).map(Some)
        }
    }
}
