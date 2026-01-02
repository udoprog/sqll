use crate::utils::repeat;
use crate::{Error, Gettable, Row};

/// A helper trait to convert a row into a user-defined type.
///
/// # Examples
///
/// ```
/// use sqll::{Connection, FromRow};
///
/// #[derive(FromRow)]
/// struct Person<'stmt> {
///     name: &'stmt str,
///     age: u32,
/// }
///
/// #[derive(FromRow)]
/// struct PersonTuple<'stmt>(&'stmt str, u32);
///
/// let mut c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users VALUES ('Alice', 42);
///     INSERT INTO users VALUES ('Bob', 69);
/// "#)?;
///
/// let mut results = c.prepare("SELECT name, age FROM users ORDER BY age")?;
///
/// while let Some(person) = results.next_row::<Person<'_>>()? {
///     println!("{} is {} years old", person.name, person.age);
/// }
///
/// results.reset()?;
///
/// while let Some(PersonTuple(name, age)) = results.next_row::<PersonTuple<'_>>()? {
///     println!("{name} is {age} years old");
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Convert into an owned type:
///
/// ```
/// use sqll::{Connection, FromRow};
///
/// #[derive(FromRow)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// #[derive(FromRow)]
/// struct PersonTuple(String, u32);
///
/// let mut c = Connection::open_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE users (name TEXT, age INTEGER);
///
///     INSERT INTO users VALUES ('Alice', 42);
///     INSERT INTO users VALUES ('Bob', 69);
/// "#)?;
///
/// let mut results = c.prepare("SELECT name, age FROM users ORDER BY age")?;
///
/// while let Some(row) = results.next()? {
///     let person = row.as_row::<Person>()?;
///     println!("{} is {} years old", person.name, person.age);
///
///     let PersonTuple(name, age) = row.as_row::<PersonTuple>()?;
///     println!("{name} is {age} years old");
/// }
/// # Ok::<_, sqll::Error>(())
/// ```
pub trait FromRow<'stmt>
where
    Self: Sized,
{
    /// Constructs an instance of `Self` from the given row.
    fn from_row(row: &Row<'stmt>) -> Result<Self, Error>;
}

macro_rules! ignore {
    ($var:ident) => {
        ""
    };
}

macro_rules! implement_tuple {
    ($ty0:ident $var0:ident $value0:expr $(, $ty:ident $var:ident $value:expr)* $(,)? ) => {
        /// [`FromRow`] implementation for a tuple.
        ///
        /// A tuple reads elements one after another, starting at the index
        /// specified in the call to [`Statement::get`].
        ///
        /// [`Statement::get`]: crate::statement::Statement::get
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
        impl<'stmt, $ty0, $($ty,)*> FromRow<'stmt> for ($ty0, $($ty,)*)
        where
            $ty0: Gettable<'stmt>,
            $($ty: Gettable<'stmt>,)*
        {
            #[inline]
            fn from_row(row: &Row<'stmt>) -> Result<Self, Error> {
                let $var0 = Gettable::get(row.as_stmt(), $value0)?;

                $(
                    let $var = Gettable::get(row.as_stmt(), $value)?;
                )*

                Ok(($var0, $($var,)*))
            }
        }
    };
}

repeat!(implement_tuple);
