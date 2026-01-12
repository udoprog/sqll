use alloc::string::String;
use alloc::vec::Vec;

use crate::ty;
use crate::{FromUnsizedColumn, Result, Statement};

use super::FromColumn;

/// [`FromColumn`] implementation which returns a newly allocated [`String`].
///
/// For a more memory-efficient way of reading bytes, consider using the
/// [`FromUnsizedColumn`] implementation for [`str`].
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
/// assert_eq!(stmt.next::<String>()?, Some(String::from("Alice")));
/// assert_eq!(stmt.next::<String>()?, Some(String::from("Bob")));
/// assert_eq!(stmt.next::<String>()?, None);
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
/// let e = stmt.next::<String>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for String {
    type Type = ty::Text;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Text) -> Result<Self> {
        let mut s = String::with_capacity(index.len());
        s.push_str(<_>::from_unsized_column(stmt, index)?);
        Ok(s)
    }
}

/// [`FromColumn`] implementation which returns a newly allocated [`Vec`].
///
/// For a more memory-efficient way of reading bytes, consider using the
/// [`FromUnsizedColumn`] implementation for a byte slice.
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
/// assert_eq!(stmt.next::<Vec<u8>>()?, Some(vec![0xaa, 0xbb]));
/// assert_eq!(stmt.next::<Vec<u8>>()?, Some(vec![0xbb, 0xcc]));
/// assert_eq!(stmt.next::<Vec<u8>>()?, None);
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
/// let e = stmt.next::<Vec::<u8>>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
/// # Ok::<_, sqll::Error>(())
/// ```
impl FromColumn<'_> for Vec<u8> {
    type Type = ty::Blob;

    #[inline]
    fn from_column(stmt: &Statement, index: ty::Blob) -> Result<Self> {
        let mut buf = Vec::with_capacity(index.len());
        buf.extend_from_slice(<_>::from_unsized_column(stmt, index)?);
        Ok(buf)
    }
}
