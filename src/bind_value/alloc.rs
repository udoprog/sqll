use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{Bind, Index, Result, Statement};

use super::BindValue;

/// [`BindValue`] implementation for a [`Vec<u8>`] byte vector.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Index};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
///     INSERT INTO files (id, data) VALUES (2, X'');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
///
/// stmt.bind_value(Index::BIND, vec![b'H', b'e', b'l', b'l', b'o'])?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [1]);
/// stmt.reset()?;
///
/// stmt.bind_value(Index::BIND, vec![])?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [2]);
/// stmt.reset()?;
/// # Ok::<_, sqll::Error>(())
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl BindValue for Vec<u8> {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: Index) -> Result<()> {
        self.as_slice().bind_value(stmt, index)
    }
}

/// [`Bind`] implementation for a [`Vec<u8>`] byte vector.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
///     INSERT INTO files (id, data) VALUES (2, X'');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
///
/// stmt.bind(vec![b'H', b'e', b'l', b'l', b'o'])?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [1]);
/// stmt.reset()?;
///
/// stmt.bind(vec![])?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [2]);
/// stmt.reset()?;
/// # Ok::<_, sqll::Error>(())
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl Bind for Vec<u8> {
    const COUNT: usize = 1;

    #[inline]
    fn bind(&self, stmt: &mut Statement) -> Result<()> {
        self.bind_value(stmt, Index::BIND)
    }
}

/// [`BindValue`] implementation for a [`String`].
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Index};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
///
/// stmt.reset()?;
/// stmt.bind_value(Index::BIND, String::from("Alice"))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [42]);
/// # Ok::<_, sqll::Error>(())
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl BindValue for String {
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: Index) -> Result<()> {
        self.as_str().bind_value(stmt, index)
    }
}

/// [`Bind`] implementation for a [`String`].
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
///
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
///
/// stmt.bind(String::from("Alice"))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [42]);
/// # Ok::<_, sqll::Error>(())
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl Bind for String {
    const COUNT: usize = 1;

    #[inline]
    fn bind(&self, stmt: &mut Statement) -> Result<()> {
        self.bind_value(stmt, Index::BIND)
    }
}

/// [`BindValue`] implementation for a boxed value.
///
/// # Examples
///
/// Using a boxed byte slice:
///
/// ```
/// use sqll::{Connection, Index};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
///     INSERT INTO files (id, data) VALUES (2, X'');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
///
/// stmt.bind_value(Index::BIND, Box::<[u8]>::from([b'H', b'e', b'l', b'l', b'o']))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [1]);
/// stmt.reset()?;
///
/// stmt.bind_value(Index::BIND, Box::<[u8]>::from([]))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [2]);
/// stmt.reset()?;
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Using a boxed string:
///
/// ```
/// use sqll::{Connection, Index};
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
///
/// stmt.bind_value(Index::BIND, Box::<str>::from("Alice"))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [42]);
/// stmt.reset()?;
/// # Ok::<_, sqll::Error>(())
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl<T> BindValue for Box<T>
where
    T: ?Sized + BindValue,
{
    #[inline]
    fn bind_value(&self, stmt: &mut Statement, index: Index) -> Result<()> {
        self.as_ref().bind_value(stmt, index)
    }
}

/// [`Bind`] implementation for a boxed value.
///
/// # Examples
///
/// Using a boxed byte slice:
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE files (id INTEGER, data BLOB);
///
///     INSERT INTO files (id, data) VALUES (0, X'48656C6C6F20576F726C6421');
///     INSERT INTO files (id, data) VALUES (1, X'48656C6C6F');
///     INSERT INTO files (id, data) VALUES (2, X'');
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT id FROM files WHERE data = ?")?;
///
/// stmt.bind(Box::<[u8]>::from([b'H', b'e', b'l', b'l', b'o']))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [1]);
/// stmt.reset()?;
///
/// stmt.bind(Box::<[u8]>::from([]))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [2]);
/// stmt.reset()?;
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Using a boxed string:
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users (name, age) VALUES ('Alice', 42), ('Bob', 30);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT age FROM users WHERE name = ?")?;
///
/// stmt.bind(Box::<str>::from("Alice"))?;
/// assert_eq!(stmt.iter::<i64>().collect::<Result<Vec<_>, _>>()?, [42]);
/// # Ok::<_, sqll::Error>(())
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
impl<T> Bind for Box<T>
where
    T: ?Sized + BindValue,
{
    const COUNT: usize = 1;

    #[inline]
    fn bind(&self, stmt: &mut Statement) -> Result<()> {
        self.bind_value(stmt, Index::BIND)
    }
}
