use crate::ty::{Any, Blob, ColumnType, Float, Integer, Text};

mod sealed {
    use crate::ty::{Any, Blob, Float, Integer, Text};

    pub trait Sealed
    where
        Self: Sized,
    {
    }

    impl Sealed for Any {}
    impl Sealed for Integer {}
    impl Sealed for Float {}
    impl Sealed for Text {}
    impl Sealed for Blob {}
}

/// Trait used to constrain type markers to non-nullable types.
///
/// Non-nullable types are types which will error in case a null value is
/// returned:
///
/// ```
/// use sqll::{Connection, Code, FromColumn, Statement, Result};
/// use sqll::ty;
///
/// #[derive(Debug)]
/// struct MyType(i64);
///
/// impl FromColumn<'_> for MyType {
///     type Type = ty::Integer;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Integer) -> Result<Self> {
///         Ok(MyType(i64::from_column(stmt, index)?))
///     }
/// }
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE nulls (value INTEGER);
///
///     INSERT INTO nulls (value) VALUES (NULL);
/// "#)?;
///
/// let mut select = c.prepare("SELECT value FROM nulls")?;
/// let e = select.next::<MyType>().unwrap_err();
/// assert_eq!(e.code(), Code::MISMATCH);
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// To make a [`NotNull`] type nullable, put it inside of the [`Nullable`] type
/// marker.
///
/// ```
/// use sqll::{Connection, Code, FromColumn, Statement, Result};
/// use sqll::ty;
///
/// #[derive(Debug, PartialEq)]
/// struct MyType(Option<i64>);
///
/// impl FromColumn<'_> for MyType {
///     type Type = ty::Nullable<ty::Integer>;
///
///     #[inline]
///     fn from_column(stmt: &Statement, index: ty::Nullable<ty::Integer>) -> Result<Self> {
///         Ok(MyType(<_>::from_column(stmt, index)?))
///     }
/// }
///
/// let mut c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///     CREATE TABLE nulls (value INTEGER);
///
///     INSERT INTO nulls (value) VALUES (NULL), (42);
/// "#)?;
///
/// let mut select = c.prepare("SELECT value FROM nulls")?;
/// assert_eq!(select.next::<MyType>(), Ok(Some(MyType(None))));
/// assert_eq!(select.next::<MyType>(), Ok(Some(MyType(Some(42)))));
/// assert_eq!(select.next::<MyType>(), Ok(None));
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// [`Nullable`]: crate::ty::Nullable
pub trait NotNull
where
    Self: self::sealed::Sealed + ColumnType,
{
}

impl NotNull for Any {}
impl NotNull for Integer {}
impl NotNull for Float {}
impl NotNull for Text {}
impl NotNull for Blob {}
