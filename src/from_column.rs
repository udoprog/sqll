use core::ffi::c_int;
use core::ptr;

use alloc::string::String;
use alloc::vec::Vec;

use crate::ffi;
use crate::{
    Code, Error, FixedBytes, FromUnsizedColumn, Null, Result, Sink, Statement, Type, Value,
};

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

/// A type suitable for reading a single value from a prepared statement.
///
/// Use with [`Statement::get`].
pub trait FromColumn<'stmt>
where
    Self: self::sealed::Sealed<'stmt> + Sized,
{
    #[doc(hidden)]
    fn from_column(stmt: &'stmt Statement, index: c_int) -> Result<Self>;
}

/// [`FromColumn`] implementation for [`Null`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Null};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind_value(1, "Alice")?;
///
/// let mut names = Vec::new();
///
/// while let Some(value) = stmt.next::<Null>()? {
///     names.push(value);
/// }
///
/// assert_eq!(names, vec![Null]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for Null {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::NULL)?;
        Ok(Null)
    }
}

/// [`FromColumn`] implementation for [`Value`].
impl FromColumn<'_> for Value {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
        let value = match stmt.column_type(index) {
            Type::BLOB => Value::blob(<_>::from_column(stmt, index)?),
            Type::TEXT => Value::text(<_>::from_column(stmt, index)?),
            Type::FLOAT => Value::float(<_>::from_column(stmt, index)?),
            Type::INTEGER => Value::integer(<_>::from_column(stmt, index)?),
            Type::NULL => Value::null(),
            _ => return Err(Error::new(Code::MISMATCH)),
        };

        Ok(value)
    }
}

/// [`FromColumn`] implementation for [`f64`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(value) = stmt.next::<f64>()? {
///     assert!(matches!(value, 3.14 | 2.71));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<i64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for f64 {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe {
            type_check(stmt, index, Type::FLOAT)?;
            ffi::sqlite3_column_double(stmt.as_ptr(), index)
        })
    }
}

/// [`FromColumn`] implementation for [`f32`].
///
/// Getting this type requires conversion and might be subject to precision
/// loss. To avoid this, consider using [`f64`] instead.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(value) = stmt.next::<f32>()? {
///     assert!(matches!(value, 3.14 | 2.71));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<i32>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for f32 {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(f64::from_column(stmt, index)? as f32)
    }
}

/// [`FromColumn`] implementation for `i64`.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value INTEGER);
///
///     INSERT INTO numbers (value) VALUES (3), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(value) = stmt.next::<i64>()? {
///     assert!(matches!(value, 3 | 2));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value INTEGER);
///
///     INSERT INTO numbers (value) VALUES (3), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<f64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for i64 {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::INTEGER)?;
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.as_ptr(), index) })
    }
}

macro_rules! lossless {
    ($ty:ty) => {
        #[doc = concat!("[`FromColumn`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        #[doc = concat!("while let Some(value) = stmt.next::<", stringify!($ty), ">()? {")]
        ///     assert!(matches!(value, 3 | 2));
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        ///
        /// Automatic conversion being denied:
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while stmt.step()?.is_row() {
        ///     let e = stmt.get::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl FromColumn<'_> for $ty {
            #[inline]
            #[allow(irrefutable_let_patterns)]
            fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
                let value = i64::from_column(stmt, index)?;
                Ok(value as $ty)
            }
        }
    };
}

macro_rules! lossy {
    ($ty:ty) => {
        #[doc = concat!("[`FromColumn`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Errors
        ///
        /// Getting this type requires conversion and might fail if the value
        /// cannot be represented by a [`i64`].
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (-9223372036854775808);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// assert!(stmt.step()?.is_row());
        #[doc = concat!("let e = stmt.get::<", stringify!($ty), ">(0).unwrap_err();")]
        /// assert_eq!(e.code(), Code::MISMATCH);
        /// # Ok::<_, sqll::Error>(())
        /// ```
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        #[doc = concat!("while let Some(value) = stmt.next::<", stringify!($ty), ">()? {")]
        ///     assert!(matches!(value, 3 | 2));
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        ///
        /// Automatic conversion being denied:
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_in_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while stmt.step()?.is_row() {
        ///     let e = stmt.get::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl FromColumn<'_> for $ty {
            #[inline]
            fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
                let value = i64::from_column(stmt, index)?;

                let Ok(value) = <$ty>::try_from(value) else {
                    return Err(Error::new(Code::MISMATCH));
                };

                Ok(value)
            }
        }
    };
}

lossy!(i8);
lossy!(i16);
lossy!(i32);
lossy!(u8);
lossy!(u16);
lossy!(u32);
lossy!(u64);
lossy!(u128);
lossless!(i128);

/// [`FromColumn`] implementation which returns a borrowed [`str`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT);
///
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let Some(name) = stmt.next::<String>()? {
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<&str>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for &'stmt str {
    #[inline]
    fn from_column(stmt: &'stmt Statement, index: c_int) -> Result<Self> {
        FromUnsizedColumn::from_unsized_column(stmt, index)
    }
}

/// [`FromColumn`] implementation which returns a newly allocated [`String`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Sink`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT);
///
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let Some(name) = stmt.next::<String>()? {
///     assert!(matches!(name.as_str(), "Alice" | "Bob"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<String>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for String {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut s = String::new();
        s.write(stmt, index)?;
        Ok(s)
    }
}

/// [`FromColumn`] implementation which returns a newly allocated [`Vec`].
///
/// For a more memory-efficient way of reading bytes, consider using its
/// [`Sink`] implementation instead.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (blob BLOB);
///
///     INSERT INTO users (blob) VALUES (X'aabb'), (X'bbcc');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT blob FROM users")?;
///
/// while let Some(value) = stmt.next::<Vec<u8>>()? {
///     assert!(matches!(value.as_slice(), b"\xaa\xbb" | b"\xbb\xcc"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<Vec::<u8>>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for Vec<u8> {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
        let mut buf = Vec::new();
        buf.write(stmt, index)?;
        Ok(buf)
    }
}

/// [`FromColumn`] implementation which returns a borrowed `[u8]`.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (blob BLOB);
///
///     INSERT INTO users (blob) VALUES (X'aabb'), (X'bbcc');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT blob FROM users")?;
///
/// while let Some(blob) = stmt.next::<&[u8]>()? {
///     assert!(matches!(blob, b"\xaa\xbb" | b"\xbb\xcc"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while stmt.step()?.is_row() {
///     let e = stmt.get::<&[u8]>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt> FromColumn<'stmt> for &'stmt [u8] {
    #[inline]
    fn from_column(stmt: &'stmt Statement, index: c_int) -> Result<Self> {
        FromUnsizedColumn::from_unsized_column(stmt, index)
    }
}

/// [`FromColumn`] implementation for [`FixedBytes`] which reads at most `N`
/// bytes.
///
/// If the column contains more than `N` bytes, a [`Code::MISMATCH`] error is
/// returned.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FixedBytes, Code};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id BLOB);
///
///     INSERT INTO users (id) VALUES (X'01020304'), (X'0506070809');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// assert!(stmt.step()?.is_row());
/// let bytes = stmt.get::<FixedBytes<4>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[1, 2, 3, 4]);
///
/// assert!(stmt.step()?.is_row());
/// let e = stmt.get::<FixedBytes<4>>(0).unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
///
/// let bytes = stmt.get::<FixedBytes<5>>(0)?;
/// assert_eq!(bytes.as_bytes(), &[5, 6, 7, 8, 9]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<const N: usize> FromColumn<'_> for FixedBytes<N> {
    #[inline]
    fn from_column(stmt: &Statement, index: c_int) -> Result<Self> {
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

/// [`FromColumn`] implementation for [`Option`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
/// "#)?;
///
/// let mut stmt = c.prepare("INSERT INTO users (name, age) VALUES (?, ?)")?;
///
/// stmt.reset()?;
/// stmt.bind_value(1, "Alice")?;
/// stmt.bind_value(2, None::<i64>)?;
/// assert!(stmt.step()?.is_done());
///
/// stmt.reset()?;
/// stmt.bind_value(1, "Bob")?;
/// stmt.bind_value(2, Some(30i64))?;
/// assert!(stmt.step()?.is_done());
///
/// let mut stmt = c.prepare("SELECT name, age FROM users")?;
///
/// let mut names_and_ages = Vec::new();
///
/// while let Some((name, age)) = stmt.next::<(String, Option<i64>)>()? {
///     names_and_ages.push((name, age));
/// }
///
/// names_and_ages.sort();
/// assert_eq!(names_and_ages, vec![(String::from("Alice"), None), (String::from("Bob"), Some(30))]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<'stmt, T> FromColumn<'stmt> for Option<T>
where
    T: FromColumn<'stmt>,
{
    #[inline]
    fn from_column(stmt: &'stmt Statement, index: c_int) -> Result<Self> {
        if stmt.column_type(index) == Type::NULL {
            return Ok(None);
        }

        Ok(Some(T::from_column(stmt, index)?))
    }
}

// NB: We have to perform strict type checking to avoid auto-conversion, if we
// permit it, the pointers that have previously been fetched for a given column
// may become invalidated.
//
// See: https://sqlite.org/c3ref/column_blob.html
#[inline(always)]
pub(crate) fn type_check(stmt: &Statement, index: c_int, expected: Type) -> Result<()> {
    if stmt.column_type(index) != expected {
        return Err(Error::new(Code::MISMATCH));
    }

    Ok(())
}
