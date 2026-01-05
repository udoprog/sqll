use crate::utils::repeat;
use crate::{Check, Error, FromColumn, Statement};

/// This allows a type to be constructed from a [`Statement`] using [`next`] or
/// [`iter`].
///
/// This is typically implemented with the [`Row` derive].
///
/// [`iter`]: Statement::iter
/// [`next`]: Statement::next
/// [`Row` derive]: derive@crate::Row
///
/// # Examples
///
/// The simplest implementation for [`Row`] is provided by tuples.
///
/// ```
/// use sqll::{Connection, Row};
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
/// let mut results = c.prepare("SELECT name, age FROM users ORDER BY age")?;
///
/// while let Some((name, age)) = results.next::<(&str, u32)>()? {
///     println!("{name} is {age} years old");
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// It can also be derived on a custom struct:
///
/// ```
/// use sqll::{Connection, Row};
///
/// #[derive(Row)]
/// struct Person<'stmt> {
///     name: &'stmt str,
///     age: u32,
/// }
///
/// #[derive(Row)]
/// struct PersonTuple<'stmt>(&'stmt str, u32);
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
/// let mut results = c.prepare("SELECT name, age FROM users ORDER BY age")?;
///
/// while let Some(person) = results.next::<Person<'_>>()? {
///     println!("{} is {} years old", person.name, person.age);
/// }
///
/// results.reset()?;
///
/// while let Some(PersonTuple(name, age)) = results.next::<PersonTuple<'_>>()? {
///     println!("{name} is {age} years old");
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Convert into an owned type:
///
/// ```
/// use sqll::{Connection, Row};
///
/// #[derive(Row)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// #[derive(Row)]
/// struct PersonTuple(String, u32);
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
/// let mut stmt = c.prepare("SELECT name, age FROM users ORDER BY age")?;
///
/// while stmt.step()?.is_row() {
///     let person = stmt.get_row::<Person>()?;
///     println!("{} is {} years old", person.name, person.age);
///
///     let PersonTuple(name, age) = stmt.get_row::<PersonTuple>()?;
///     println!("{name} is {age} years old");
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
pub trait Row<'stmt>
where
    Self: Sized,
{
    /// Constructs an instance of `Self` from the given row.
    fn from_row(stmt: &'stmt mut Statement) -> Result<Self, Error>;
}

impl<'stmt, T> Row<'stmt> for T
where
    T: FromColumn<'stmt>,
{
    #[inline]
    fn from_row(stmt: &'stmt mut Statement) -> Result<Self, Error> {
        let prepare = T::Check::check(stmt, 0)?;
        T::load(stmt, prepare)
    }
}

macro_rules! ignore {
    ($var:ident) => {
        ""
    };
}

macro_rules! implement_tuple {
    ($ty0:ident $var0:ident $value0:literal $value1:literal $(, $ty:ident $var:ident $value0n:literal $value1n:literal)* $(,)? ) => {
        /// [`Row`] implementation for a tuple.
        ///
        /// A tuple reads elements one after another, starting at the first
        /// index.
        ///
        /// [`Statement::get`]: crate::statement::Statement::get
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
        /// let mut stmt = c.prepare("SELECT * FROM users")?;
        ///
        #[doc = concat!("while let Some((", stringify!($var0), "," $(, " ", stringify!($var), ",")*, ")) = stmt.next::<(", ignore!($var0), "i64," $(, " ", ignore!($var), "i64,")*, ")>()? {")]
        #[doc = concat!("    assert_eq!(", stringify!($var0), ", ", stringify!($value0), ");")]
        $(
            #[doc = concat!("    assert_eq!(", stringify!($var), ", ", stringify!($value0n), ");")]
        )*
        /// }
        /// # Ok::<_, sqll::Error>(())
        /// ```
        impl<'stmt, $ty0, $($ty,)*> Row<'stmt> for ($ty0, $($ty,)*)
        where
            $ty0: FromColumn<'stmt>,
            $($ty: FromColumn<'stmt>,)*
        {
            #[inline]
            fn from_row(stmt: &'stmt mut Statement) -> Result<Self, Error> {
                let $var0 = <$ty0>::Check::check(stmt, $value0)?;
                $(let $var = <$ty>::Check::check(stmt, $value0n)?;)*
                let $var0 = <$ty0>::load(stmt, $var0)?;
                $(let $var = <$ty>::load(stmt, $var)?;)*
                Ok(($var0, $($var,)*))
            }
        }
    };
}

repeat!(implement_tuple);
