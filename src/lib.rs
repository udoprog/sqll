//! [<img alt="github" src="https://img.shields.io/badge/github-udoprog/sqll-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/sqll)
//! [<img alt="crates.io" src="https://img.shields.io/crates/v/sqll.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/sqll)
//! [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-sqll-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/sqll)
//!
//! Efficient interface interface to [SQLite] that doesn't get in your way.
//!
//! <br>
//!
//! ## Usage
//!
//! The two primary methods to interact with an SQLite database through this
//! crate is through [`execute`] and [`prepare`].
//!
//! [`execute`] is used for batch statements, and allows for multiple queries to
//! be specified. [`prepare`] only allows for a single statement to be
//! specified, but in turn permits [reading rows] and [binding query
//! parameters].
//!
//! Special consideration needs to be taken about the thread safety of
//! connections. You can read more about that in the [`Connection`]
//! documentation.
//!
//! You can find simple examples of this below.
//!
//! <br>
//!
//! #### Examples
//!
//! * [`examples/persons.rs`] - A simple table with users, a primary key,
//!   inserting and querying.
//! * [`examples/axum.rs`] - Create an in-memory database connection and serve
//!   it using [`axum`]. This showcases how do properly handle external
//!   synchronization for the best performance.
//!
//! <br>
//!
//! #### Connecting and querying
//!
//! Here is a simple example of setting up an in-memory connection, creating a
//! table, insert and query some rows:
//!
//! ```
//! use sqll::{Connection, Result};
//!
//! let c = Connection::open_in_memory()?;
//!
//! c.execute(r#"
//!     CREATE TABLE users (name TEXT, age INTEGER);
//!
//!     INSERT INTO users VALUES ('Alice', 42);
//!     INSERT INTO users VALUES ('Bob', 52);
//! "#)?;
//!
//! let results = c.prepare("SELECT name, age FROM users ORDER BY age")?
//!     .iter::<(String, u32)>()
//!     .collect::<Result<Vec<_>>>()?;
//!
//! assert_eq!(results, [("Alice".to_string(), 42), ("Bob".to_string(), 52)]);
//! # Ok::<_, sqll::Error>(())
//! ```
//!
//! <br>
//!
//! #### The [`Row`] trait.
//!
//! The [`Row`] trait can be used to conveniently read rows from a statement
//! using [`next`]. It can be conveniently implemented using the [`Row`
//! derive].
//!
//! ```
//! use sqll::{Connection, Row};
//!
//! #[derive(Row)]
//! struct Person<'stmt> {
//!     name: &'stmt str,
//!     age: u32,
//! }
//!
//! let mut c = Connection::open_in_memory()?;
//!
//! c.execute(r#"
//!     CREATE TABLE users (name TEXT, age INTEGER);
//!
//!     INSERT INTO users VALUES ('Alice', 42);
//!     INSERT INTO users VALUES ('Bob', 52);
//! "#)?;
//!
//! let mut results = c.prepare("SELECT name, age FROM users ORDER BY age")?;
//!
//! while let Some(person) = results.next::<Person<'_>>()? {
//!     println!("{} is {} years old", person.name, person.age);
//! }
//! # Ok::<_, sqll::Error>(())
//! ```
//!
//! <br>
//!
//! #### The [`Bind`] trait.
//!
//! The [`Bind`] trait can be used to conveniently [`bind`] parameters to
//! prepared statements, and it can conveniently be implemented for structs
//! using the [`Bind` derive].
//!
//! ```
//! use sqll::{Bind, Connection, Row};
//!
//! #[derive(Bind, Row, PartialEq, Debug)]
//! #[sql(named)]
//! struct Person<'stmt> {
//!     name: &'stmt str,
//!     age: u32,
//! }
//!
//! let c = Connection::open_in_memory()?;
//!
//! c.execute(r#"
//!    CREATE TABLE persons (name TEXT, age INTEGER);
//! "#)?;
//!
//! let mut stmt = c.prepare("INSERT INTO persons (name, age) VALUES (:name, :age)")?;
//! stmt.execute(Person { name: "Alice", age: 30 })?;
//! stmt.execute(Person { name: "Bob", age: 40 })?;
//!
//! let mut query = c.prepare("SELECT name, age FROM persons ORDER BY age")?;
//!
//! let p = query.next::<Person<'_>>()?;
//! assert_eq!(p, Some(Person { name: "Alice", age: 30 }));
//!
//! let p = query.next::<Person<'_>>()?;
//! assert_eq!(p, Some(Person { name: "Bob", age: 40 }));
//! # Ok::<_, sqll::Error>(())
//! ```
//!
//! <br>
//!
//! #### Efficient use of prepared Statements
//!
//! Correct handling of prepared statements are crucial to get good performance
//! out of sqlite. They contain all the state associated with a query and are
//! expensive to construct so they should be re-used.
//!
//! Using a [`Prepare::PERSISTENT`] prepared statement to perform multiple
//! queries:
//!
//! ```
//! use sqll::{Connection, Prepare};
//!
//! let c = Connection::open_in_memory()?;
//!
//! c.execute(r#"
//!     CREATE TABLE users (name TEXT, age INTEGER);
//!
//!     INSERT INTO users VALUES ('Alice', 42);
//!     INSERT INTO users VALUES ('Bob', 52);
//! "#)?;
//!
//! let mut stmt = c.prepare_with("SELECT * FROM users WHERE age > ?", Prepare::PERSISTENT)?;
//!
//! let mut rows = Vec::new();
//!
//! for age in [40, 50] {
//!     stmt.bind(age)?;
//!
//!     while let Some(row) = stmt.next::<(String, i64)>()? {
//!         rows.push(row);
//!     }
//! }
//!
//! let expected = vec![
//!     (String::from("Alice"), 42),
//!     (String::from("Bob"), 52),
//!     (String::from("Bob"), 52),
//! ];
//!
//! assert_eq!(rows, expected);
//! # Ok::<_, sqll::Error>(())
//! ```
//!
//! <br>
//!
//! ## Features
//!
//! * `std` - Enable usage of the Rust standard library. Enabled by default.
//! * `alloc` - Enable usage of the Rust alloc library. This is required and is
//!   enabled by default. Disabling this option will currently cause a compile
//!   error.
//! * `derive` - Add a dependency to and re-export of the [`Row` derive]
//!   macro.
//! * `bundled` - Use a bundled version of sqlite. The bundle is provided by the
//!   [`sqll-sys`] crate and the sqlite version used is part of the build
//!   metadata of that crate[^sqll-sys].
//! * `threadsafe` - Enable usage of sqlite with the threadsafe option set. We
//!   assume any system level libraries have this build option enabled, if this
//!   is disabled the `bundled` feature has to be enabled. If `threadsafe` is
//!   disabled, `Connection` and `Statement` does not implement `Send`. But it
//!   is also important to understand that if this option is not set, sqlite
//!   **may not be used by multiple threads at all** even if threads have
//!   distinct connections. To disable mutexes instead which allows for
//!   efficient one connection per thread the [`OpenOptions::no_mutex`] option
//!   should be used instead[^sqll-sys].
//! * `strict` - Enable usage of sqlite with the strict compiler options
//!   enabled[^sqll-sys].
//!
//! [^sqll-sys]: This is a forwarded sqll-sys option, see <https://docs.rs/sqll-sys>.
//!
//! <br>
//!
//! ## Why do we need another sqlite interface?
//!
//! For other low-level crates, it is difficult to set up and use prepared
//! statements, They are mostly implemented in a manner which requires the
//! caller to borrow the connection in use.
//!
//! This library implements database objects through the v2 API which ensures
//! that the database remains alive for as long as objects associated with it
//! are alive. This is implemented in the SQLite library itself.
//!
//! Prepared statements can be expensive to create and *should* be stored and
//! re-used to achieve the best performance. This is something that crates like
//! `rusqlite` implements, but can easily be done manually, with no overhead, by
//! simply storing the [`Statement`] object directly behind a mutex. Statements
//! can also benefit from using the [`Prepare::PERSISTENT`] option which this
//! library supports through [`prepare_with`].
//!
//! This library is designed to the extent possible to avoid intermediary
//! allocations. For example [calling `execute`] doesn't allocate externally of
//! the sqlite3 bindings or we require that c-strings are used when SQLite
//! itself doesn't provide an API for using Rust strings directly. It's also
//! implemented as a thing layer on top of SQLite with minimal added
//! abstractions ensuring you get the best possible performance.
//!
//! <br>
//!
//! ## License
//!
//! This is a rewrite of the [`sqlite` crate], and components used from there
//! have been copied under the MIT license.
//!
//! [`axum`]: https://docs.rs/axum
//! [`Bind` derive]: https://docs.rs/sqll/latest/sqll/derive.Bind.html
//! [`bind`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
//! [`Bind`]: https://docs.rs/sqll/latest/sqll/trait.Bind.html
//! [`Connection`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#thread-safety
//! [`examples/axum.rs`]: https://github.com/udoprog/sqll/blob/main/examples/axum.rs
//! [`examples/persons.rs`]: https://github.com/udoprog/sqll/blob/main/examples/persons.rs
//! [`execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
//! [`Row` derive]: https://docs.rs/sqll/latest/sqll/derive.Row.html
//! [`Row`]: https://docs.rs/sqll/latest/sqll/trait.Row.html
//! [`next`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
//! [`OpenOptions::no_mutex`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex
//! [`prepare_with`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare_with
//! [`Prepare::PERSISTENT`]: https://docs.rs/sqll/latest/sqll/struct.Prepare.html#associatedconstant.PERSISTENT
//! [`prepare`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare
//! [`sqlite` crate]: https://github.com/stainless-steel/sqlite
//! [`sqll-sys`]: https://crates.io/crates/sqll-sys
//! [`Statement`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html
//! [binding query parameters]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
//! [calling `execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
//! [reading rows]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
//! [SQLite]: https://www.sqlite.org

#![no_std]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::new_without_default)]
#![warn(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(not(feature = "alloc"))]
compile_error!("The `alloc` feature must be enabled to use this crate.");

#[cfg(test)]
mod tests;

mod bind;
mod bind_value;
mod bytes;
mod connection;
mod error;
mod ffi;
mod fixed_blob;
mod fixed_text;
mod from_column;
mod from_unsized_column;
mod owned;
mod row;
mod sink;
mod statement;
mod utils;
mod value;
mod version;

#[doc(inline)]
pub use self::bind::Bind;
#[doc(inline)]
pub use self::bind_value::BindValue;
#[doc(inline)]
pub use self::connection::{Connection, OpenOptions, Prepare};
#[doc(inline)]
pub use self::error::{Code, DatabaseNotFound, Error, Result};
#[doc(inline)]
pub use self::fixed_blob::{CapacityError, FixedBlob};
#[doc(inline)]
pub use self::fixed_text::FixedText;
#[doc(inline)]
pub use self::from_column::FromColumn;
#[doc(inline)]
pub use self::from_unsized_column::FromUnsizedColumn;
#[doc(inline)]
pub use self::row::Row;
#[doc(inline)]
pub use self::sink::Sink;
#[doc(inline)]
pub use self::statement::{Null, State, Statement};
#[doc(inline)]
pub use self::value::{Type, Value};
#[doc(inline)]
pub use self::version::{lib_version, lib_version_number};

/// Derive macro for [`Bind`].
///
/// This can be used to automatically implement [`Bind`] for a struct and allows
/// the struct to be used for structured binding of multiple parameters into a
/// [`Statement`] using [`bind`].
///
/// This relies on [`BindValue`] being called for each field in the struct. By
/// default the `#[sql(index)]` used starts at 1 and is incremented for each
/// field. This behavior can be modified with attributes. Notably this also
/// supports convenient use of named parameters through `[sql(named)]`.
///
/// ```
/// use sqll::Bind;
///
/// #[derive(Bind)]
/// struct Person<'stmt> {
///     name: &'stmt str,
///     age: u32,
/// }
/// ```
///
/// [`bind`]: Statement::bind
///
/// <br>
///
/// ## Container attributes
///
/// <br>
///
/// #### `#[sql(crate = ..)]`
///
/// This attributes allows specifying an alternative path to the `sqll` crate.
///
/// This is useful when the crate is renamed from the default `::sqll`.
///
/// ```
/// # extern crate sqll as my_sqll;
/// use my_sqll::Bind;
///
/// #[derive(Bind)]
/// #[sql(crate = ::my_sqll)]
/// struct Person<'stmt> {
///     name: &'stmt str,
///     age: u32,
/// }
/// ```
///
/// <br>
///
/// #### `#[sql(named)]`
///
/// This attribute enabled bindings to use field names instead of go by the
/// default index.
///
/// When using `named`, the default binding names are the field names prefixed
/// with a `:`. So a field named `name` will bind to `:name`.
///
/// ```
/// use sqll::{Bind, Connection};
///
/// #[derive(Bind)]
/// #[sql(named)]
/// struct Person<'stmt> {
///     name: &'stmt str,
///     age: u32,
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///    CREATE TABLE persons (name TEXT, age INTEGER);
/// "#)?;
///
/// let mut stmt = c.prepare("INSERT INTO persons (name, age) VALUES (:name, :age)")?;
/// let person = Person { name: "Alice", age: 30 };
/// stmt.bind(person)?;
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// <br>
///
/// ## Field attribuets
///
/// <br>
///
/// #### `#[sql(index = ..)]`
///
/// This allows the index being used for a particular row to be overriden. Note
/// that binding indexes are 1-based. Setting a particular index will cause
/// subsequent indexes to continue from that index.
///
/// ```
/// use sqll::Bind;
///
/// #[derive(Bind)]
/// struct Person<'stmt> {
///     #[sql(index = 2)]
///     name: &'stmt str,
///     #[sql(index = 1)]
///     age: u32,
/// }
/// ```
///
/// <br>
///
/// #### `#[sql(name = "..")]`
///
/// This allows for specifying an explicit binding name to use, instead of the
/// default which is to bind by integer.
///
/// ```
/// use sqll::{Bind, Connection};
///
/// #[derive(Bind)]
/// struct Person<'stmt> {
///     #[sql(name = c":notname")]
///     name: &'stmt str,
///     #[sql(name = c":notage")]
///     age: u32,
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///    CREATE TABLE persons (name TEXT, age INTEGER);
/// "#)?;
///
/// let mut stmt = c.prepare("INSERT INTO persons (name, age) VALUES (:notname, :notage)")?;
/// let person = Person { name: "Alice", age: 30 };
/// stmt.bind(person)?;
/// # Ok::<_, sqll::Error>(())
/// ```
#[cfg(feature = "derive")]
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use sqll_macros::Bind;

/// Derive macro for [`Row`].
///
/// This can be used to automatically implement [`Row`] for a struct and allows
/// the struct to be constructed from a [`Statement`] using [`next`] or
/// [`iter`].
///
/// This relies on [`FromColumn`] being called to construct each field in the
/// struct. By default the `#[sql(index)]` used starts at `0` and is incremented
/// for each field. This behavior can be modified with attributes.
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
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// [`iter`]: Statement::iter
/// [`next`]: Statement::next
///
/// <br>
///
/// ## Container attributes
///
/// <br>
///
/// #### `#[sql(crate = ..)]`
///
/// This attributes allows specifying an alternative path to the `sqll` crate.
///
/// This is useful when the crate is renamed from the default `::sqll`.
///
/// ```
/// # extern crate sqll as my_sqll;
/// use my_sqll::Row;
///
/// #[derive(Row)]
/// #[sql(crate = ::my_sqll)]
/// struct Person<'stmt> {
///     name: &'stmt str,
///     age: u32,
/// }
/// ```
///
/// <br>
///
/// ## Field attribuets
///
/// <br>
///
/// #### `#[sql(index = ..)]`
///
/// This allows the index being used for a particular row to be overriden.
///
/// ```
/// use sqll::Row;
///
/// #[derive(Row)]
/// struct Person<'stmt> {
///     #[sql(index = 1)]
///     name: &'stmt str,
///     #[sql(index = 0)]
///     age: u32,
/// }
/// ```
#[cfg(feature = "derive")]
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use sqll_macros::Row;
