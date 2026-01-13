use core::ffi::c_int;

use crate::utils::repeat;
use crate::{BindValue, Error, Statement};

/// The first index used when binding parameters into a [`Statement`].
///
/// This is useful when implementing [`Bind`] for a primitive type which
/// implements [`BindValue`].
pub const BIND_INDEX: c_int = 1;

/// This allows a type to be used for structured binding of multiple parameters
/// into a [`Statement`] using [`bind`].
///
/// This is typically implemented through the [`Bind` derive].
///
/// [`bind`]: Statement::bind
/// [`Bind` derive]: derive@crate::Bind
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Bind};
///
/// #[derive(Bind)]
/// struct Binding<'a> {
///     name: &'a str,
///     age: u32,
///     order_by: &'a str,
/// }
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users VALUES ('Alice', 42);
///     INSERT INTO users VALUES ('Bob', 72);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name, age FROM users WHERE name = ? AND age = ? ORDER BY ?")?;
/// stmt.bind(Binding { name: "Bob", age: 72, order_by: "age" })?;
///
/// assert_eq!(stmt.next::<(String, u32)>()?, Some(("Bob".to_string(), 72)));
/// assert_eq!(stmt.next::<(String, u32)>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
pub trait Bind {
    /// Bind this value into the given [`Statement`].
    fn bind(&self, stmt: &mut Statement) -> Result<(), Error>;
}

/// [`Bind`] implementation for references.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Bind};
///
/// #[derive(Bind)]
/// struct Binding<'a> {
///     name: &'a str,
///     age: u32,
///     order_by: &'a str,
/// }
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users VALUES ('Alice', 42);
///     INSERT INTO users VALUES ('Bob', 72);
/// "#)?;
///
/// let mut stmt = c.prepare("SELECT name, age FROM users WHERE name = ? AND age = ? ORDER BY ?")?;
/// stmt.bind(&Binding { name: "Bob", age: 72, order_by: "age" })?;
///
/// assert_eq!(stmt.next::<(String, u32)>()?, Some(("Bob".to_string(), 72)));
/// assert_eq!(stmt.next::<(String, u32)>()?, None);
/// # Ok::<_, sqll::Error>(())
/// ```
impl<T> Bind for &T
where
    T: ?Sized + Bind,
{
    #[inline]
    fn bind(&self, stmt: &mut Statement) -> Result<(), Error> {
        (*self).bind(stmt)
    }
}

/// [`Bind`] implementation for an empty binding.
///
/// Calling something with this argument causes no parameters to be bound.
///
/// # Examples
///
/// ```
/// use sqll::Connection;
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE config (key TEXT, value TEXT);
/// "#)?;
///
/// let mut insert = c.prepare("INSERT INTO config VALUES ('version', '1.0.0')")?;
/// insert.execute(())?;
/// # Ok::<_, sqll::Error>(())
/// ```
impl Bind for () {
    #[inline]
    fn bind(&self, _stmt: &mut Statement) -> Result<(), Error> {
        Ok(())
    }
}

macro_rules! ty {
    ($_:ident) => {
        "i64"
    };
}

macro_rules! implement_tuple {
    ($ty0:ident $var0:ident $value0:literal $value1:literal $(, $ty:ident $var:ident $value0n:literal $value1n:literal)* $(,)? ) => {
        /// [`Bind`] implementation for a tuple.
        ///
        /// A tuple binds elements one after another starting from the first index.
        ///
        /// [`Statement::column`]: crate::statement::Statement::column
        ///
        /// # Examples
        ///
        /// ```
        /// use sqll::Connection;
        ///
        /// let c = Connection::open_in_memory()?;
        #[doc = concat!("c.execute(\"CREATE TABLE users (", stringify!($var0), " INTEGER" $(, ", ", stringify!($var), " INTEGER")*, ")\")?;")]
        #[doc = concat!("c.execute(\"INSERT INTO users VALUES (", stringify!($value0) $(, ", ", stringify!($value0n))*, ")\")?;")]
        ///
        #[doc = concat!("let mut stmt = c.prepare(\"SELECT * FROM users WHERE ", stringify!($var0), " = ?" $(, " AND ", stringify!($var), " = ?")*, "\")?;")]
        #[doc = concat!("stmt.bind((", stringify!($value0), "," $(, " ", stringify!($value0n), ",")*, "))?;")]
        #[doc = concat!("let v = stmt.next::<(", ty!($ty0), ",", $(" ", ty!($ty), ",",)* ")>()?.expect(\"missing\");")]
        /// assert_eq!(v.0, 0);
        $(#[doc = concat!("assert_eq!(v.", stringify!($value0n), ", ", stringify!($value0n), ");")])*
        ///
        #[doc = concat!("assert!(stmt.step()?.is_done());")]
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl<$ty0, $($ty,)*> Bind for ($ty0, $($ty,)*)
        where
            $ty0: BindValue,
            $($ty: BindValue,)*
        {
            #[inline]
            fn bind(&self, stmt: &mut Statement) -> Result<(), Error> {
                let ($var0, $($var,)*) = self;
                BindValue::bind_value($var0, stmt, $value1)?;
                $(BindValue::bind_value($var, stmt, $value1n)?;)*
                Ok(())
            }
        }
    };
}

repeat!(implement_tuple);
