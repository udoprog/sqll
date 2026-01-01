use core::ffi::c_int;
use core::mem;
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
/// use sqll::{Connection, Null};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', NULL), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
/// stmt.bind(1, "Alice")?;
///
/// let mut names = Vec::new();
///
/// while let Some(row) = stmt.next()? {
///     names.push(stmt.get::<Null>(0)?);
/// }
///
/// assert_eq!(names, vec![Null]);
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for Null {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::NULL)?;
        Ok(Null)
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

/// [`Gettable`] implementation for [`f64`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(row) = stmt.next()? {
///     let value = row.get::<f64>(0)?;
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
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(row) = stmt.next()? {
///     let e = row.get::<i64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for f64 {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(unsafe {
            type_check(stmt, index, Type::FLOAT)?;
            ffi::sqlite3_column_double(stmt.as_ptr(), index)
        })
    }
}

/// [`Gettable`] implementation for [`f32`].
///
/// Getting this type requires conversion and might be subject to precision
/// loss. To avoid this, consider using [`f64`] instead.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(row) = stmt.next()? {
///     let value = row.get::<f32>(0)?;
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
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value REAL);
///
///     INSERT INTO numbers (value) VALUES (3.14), (2.71);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(row) = stmt.next()? {
///     let e = row.get::<i32>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for f32 {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        Ok(f64::get(stmt, index)? as f32)
    }
}

/// [`Gettable`] implementation for `i64`.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value INTEGER);
///
///     INSERT INTO numbers (value) VALUES (3), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(row) = stmt.next()? {
///     let value = row.get::<i64>(0)?;
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
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE numbers (value INTEGER);
///
///     INSERT INTO numbers (value) VALUES (3), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT value FROM numbers")?;
///
/// while let Some(row) = stmt.next()? {
///     let e = row.get::<f64>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
impl Gettable<'_> for i64 {
    #[inline]
    fn get(stmt: &Statement, index: c_int) -> Result<Self> {
        type_check(stmt, index, Type::INTEGER)?;
        Ok(unsafe { ffi::sqlite3_column_int64(stmt.as_ptr(), index) })
    }
}

macro_rules! lossless {
    ($ty:ty) => {
        #[doc = concat!(" [`Gettable`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while let Some(row) = stmt.next()? {
        #[doc = concat!("     let value = row.get::<", stringify!($ty), ">(0)?;")]
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
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while let Some(row) = stmt.next()? {
        ///     let e = row.get::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl Gettable<'_> for $ty {
            #[inline]
            #[allow(irrefutable_let_patterns)]
            fn get(stmt: &Statement, index: c_int) -> Result<Self> {
                let value = i64::get(stmt, index)?;
                Ok(value as $ty)
            }
        }
    };
}

macro_rules! lossy {
    ($ty:ty) => {
        #[doc = concat!(" [`Gettable`] implementation for `", stringify!($ty), "`.")]
        ///
        /// # Errors
        ///
        /// Getting this type requires conversion and might fail if the value
        /// cannot be represented by a [`i64`].
        ///
        /// ```
        /// use sqll::{Connection, Code};
        ///
        /// let c = Connection::open_memory()?;
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
        #[doc = concat!(" let e = stmt.get::<", stringify!($ty), ">(0).unwrap_err();")]
        /// assert_eq!(e.code(), Code::MISMATCH);
        /// # Ok::<_, sqll::Error>(())
        /// ```
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while let Some(row) = stmt.next()? {
        #[doc = concat!("     let value = row.get::<", stringify!($ty), ">(0)?;")]
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
        /// let c = Connection::open_memory()?;
        ///
        /// c.execute(r#"
        ///     CREATE TABLE numbers (value INTEGER);
        ///
        ///     INSERT INTO numbers (value) VALUES (3), (2);
        /// "#)?;
        ///
        /// let mut stmt = c.prepare("SELECT value FROM numbers")?;
        ///
        /// while let Some(row) = stmt.next()? {
        ///     let e = row.get::<f64>(0).unwrap_err();
        ///     assert_eq!(e.code(), Code::MISMATCH);
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl Gettable<'_> for $ty {
            #[inline]
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

lossy!(i8);
lossy!(i16);
lossy!(i32);
lossy!(u8);
lossy!(u16);
lossy!(u32);
lossy!(u64);
lossy!(u128);
lossless!(i128);

/// [`Gettable`] implementation which returns a borrowed [`str`].
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT);
///
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let name = stmt.get::<String>(0)?;
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
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let e = row.get::<&str>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
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
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT);
///
///     INSERT INTO users (name) VALUES ('Alice'), ('Bob');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let name = stmt.get::<String>(0)?;
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
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let e = row.get::<String>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
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
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (blob BLOB);
///
///     INSERT INTO users (blob) VALUES (X'aabb'), (X'bbcc');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT blob FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let blob = row.get::<Vec<u8>>(0)?;
///     assert!(matches!(blob.as_slice(), b"\xaa\xbb" | b"\xbb\xcc"));
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Automatic conversion being denied:
///
/// ```
/// use sqll::{Connection, Code};
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let e = row.get::<Vec::<u8>>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
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
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (blob BLOB);
///
///     INSERT INTO users (blob) VALUES (X'aabb'), (X'bbcc');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT blob FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let blob = stmt.get::<&[u8]>(0)?;
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
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (id INTEGER);
///
///     INSERT INTO users (id) VALUES (1), (2);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM users")?;
///
/// while let Some(row) = stmt.next()? {
///     let e = row.get::<&[u8]>(0).unwrap_err();
///     assert_eq!(e.code(), Code::MISMATCH);
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
/// use sqll::{Connection, FixedBytes, Code};
///
/// let c = Connection::open_memory()?;
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
/// use sqll::Connection;
///
/// let c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
/// "#)?;
///
/// let mut stmt = c.prepare("INSERT INTO users (name, age) VALUES (?, ?)")?;
///
/// stmt.reset()?;
/// stmt.bind(1, "Alice")?;
/// stmt.bind(2, None::<i64>)?;
/// assert!(stmt.step()?.is_done());
///
/// stmt.reset()?;
/// stmt.bind(1, "Bob")?;
/// stmt.bind(2, Some(30i64))?;
/// assert!(stmt.step()?.is_done());
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
            return Ok(None);
        }

        Ok(Some(T::get(stmt, index)?))
    }
}

macro_rules! repeat {
    ($macro:path) => {
        $macro!(A a 0);
        $macro!(A a 0, B b 1);
        $macro!(A a 0, B b 1, C c 2);
        $macro!(A a 0, B b 1, C c 2, D d 3);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11, M m 12);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11, M m 12, N n 13);
        $macro!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11, M m 12, N n 13, O o 14);
    };
}

macro_rules! ignore {
    ($var:ident) => {
        ""
    };
}

macro_rules! implement_tuple {
    ($ty0:ident $var0:ident $value0:expr $(, $ty:ident $var:ident $value:expr)* $(,)? ) => {
        impl<'stmt, $ty0 $(, $ty)*> self::sealed::Sealed<'stmt> for ($ty0, $($ty,)*)
        where
            $ty0: self::sealed::Sealed<'stmt>,
            $($ty: self::sealed::Sealed<'stmt>,)*
        {}

        /// [`Gettable`] implementation for a tuple.
        ///
        /// A tuple reads elements one after another, starting at the index
        /// specified in the call to [`Statement::get`].
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_memory()?;
        #[doc = concat!(" c.execute(\"CREATE TABLE users (", stringify!($var0) $(, ", ", stringify!($var), " INTEGER")*, ")\")?;")]
        #[doc = concat!(" c.execute(\"INSERT INTO users VALUES (", stringify!($value0) $(, ", ", stringify!($value))*, ")\")?;")]
        ///
        /// let mut stmt = c.prepare("SELECT * FROM users")?;
        ///
        /// while let Some(row) = stmt.next()? {
        #[doc = concat!("     let (", stringify!($var0), "," $(, " ", stringify!($var), ",")*, ") = row.get::<(", ignore!($var0), "i64," $(, " ", ignore!($var), "i64,")*, ")>(0)?;")]
        #[doc = concat!("     assert_eq!(", stringify!($var0), ", ", stringify!($value0), ");")]
        $(
            #[doc = concat!("     assert_eq!(", stringify!($var), ", ", stringify!($value), ");")]
        )*
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl<'stmt, $ty0, $($ty,)*> Gettable<'stmt> for ($ty0, $($ty,)*)
        where
            $ty0: Gettable<'stmt>,
            $($ty: Gettable<'stmt>,)*
        {
            #[inline]
            fn get(stmt: &'stmt Statement, mut index: c_int) -> Result<Self> {
                let $var0 = Gettable::get(stmt, advance(&mut index))?;

                $(
                    let $var = Gettable::get(stmt, advance(&mut index))?;
                )*

                Ok(($var0, $($var,)*))
            }
        }
    };
}

repeat!(implement_tuple);

fn advance(index: &mut c_int) -> c_int {
    let n = index.wrapping_add(1);
    mem::replace(index, n)
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
