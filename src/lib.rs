//! [<img alt="github" src="https://img.shields.io/badge/github-udoprog/sqll-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/sqll)
//! [<img alt="crates.io" src="https://img.shields.io/crates/v/sqll.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/sqll)
//! [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-sqll-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/sqll)
//!
//! Efficient interface to [SQLite] that doesn't get in your way.
//!
//! <br>
//!
//! ## Usage
//!
//! The two primary methods to interact with an SQLite database through this
//! crate is through [`execute`] and [`prepare`].
//!
//! The [`execute`] function is used for batch statements, and allows for
//! multiple queries to be specified. [`prepare`] only allows for a single
//! statement to be specified, but in turn permits [reading rows] and [binding
//! query parameters].
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
//!   it using [`axum`]. This showcases how to properly handle external
//!   synchronization for the best performance in a real-world scenario.
//! * [`examples/tokio_async.rs`] - Using `sqll` in an asynchronous context with
//!   `tokio`.
//! * [`examples/pool.rs`] - Using the high-level [`Pool`] to share read-only
//!   connections and serialize writes from asynchronous tasks.
//!
//! <br>
//!
//! #### Connecting and querying
//!
//! Here is a simple example of setting up an in-memory connection, creating a
//! table, inserting and querying back some rows:
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
//!
//! stmt.execute(Person { name: "Alice", age: 30 })?;
//! stmt.reset()?;
//!
//! stmt.execute(Person { name: "Bob", age: 40 })?;
//! stmt.reset()?;
//!
//! let mut query = c.prepare("SELECT name, age FROM persons ORDER BY age")?;
//!
//! let p = query.next::<Person<'_>>()?;
//! assert_eq!(p, Some(Person { name: "Alice", age: 30 }));
//!
//! let p = query.next::<Person<'_>>()?;
//! assert_eq!(p, Some(Person { name: "Bob", age: 40 }));
//!
//! query.reset()?;
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
//! Using a [`PrepareWith::persistent`] prepared statement to perform multiple
//! queries:
//!
//! ```
//! use sqll::Connection;
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
//! let mut stmt = c.prepare_with("SELECT * FROM users WHERE age > ?")
//!     .persistent()
//!     .build()?;
//!
//! let mut rows = Vec::new();
//!
//! for age in [40, 50] {
//!     stmt.bind(age)?;
//!
//!     while let Some(row) = stmt.next::<(String, i64)>()? {
//!         rows.push(row);
//!     }
//!
//!     stmt.reset()?;
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
//! #### Use in asynchronous contexts
//!
//! In order for sqlite to be used in asynchronous contexts, the [`Statement`]
//! object usually needs to be `Send` and external synchronization necessary.
//! Since sqlite is a synchronous library we have to defer any work done to a
//! thread pool such as the one provided by [`tokio::task::spawn_blocking`]. To
//! make a [`Statement`] `Send` you can use [`Statement::into_send`], but using
//! it is `unsafe` since correct behavior depends on the build and runtime
//! configuration of the sqlite library in use.
//!
//! See the [`tokio_async` example] for a complete example.
//!
//! <br>
//!
//! ## Features
//!
//! * `std` - Enable usage of the Rust standard library. Enabled by default.
//! * `alloc` - Enable usage of the Rust alloc library. Enabled by default
//!   through `std`, and also enabled by `pool`. The crate builds without it,
//!   but APIs which need to allocate are then unavailable, such as
//!   [`OwnedBytes`], [`Connection::serialize`], [`Connection::deserialize`],
//!   [`Connection::busy_handler`] and reading columns into `String` or
//!   `Vec<u8>`.
//! * `derive` - Add a dependency to and re-export of the [`Row` derive]
//!   macro.
//! * `bundled` - Use a bundled version of sqlite. The bundle is provided by the
//!   [`sqll-sys`] crate and the sqlite version used is part of the build
//!   metadata of that crate[^sqll-sys].
//! * `threadsafe` - Enable usage of sqlite with the threadsafe option set. We
//!   assume any system level libraries have this build option enabled, and
//!   opening a connection fails if they do not. If this is disabled the
//!   `bundled` feature has to be enabled. With this feature `Connection`
//!   implements `Send`, and every connection is opened in the "serialized"
//!   threading mode, so [`OpenOptions::no_mutex`] has no effect and the unsafe
//!   [`OpenOptions::no_mutex_unchecked`] is needed to avoid the per-connection
//!   mutex. If `threadsafe` is disabled, `Connection` does not implement
//!   `Send`. But it is also important to understand that if this option is not
//!   set, sqlite **may not be used by multiple threads at all** even if threads
//!   have distinct connections[^sqll-sys].
//! * `strict` - Enable usage of sqlite with the strict compiler options
//!   enabled[^sqll-sys].
//! * `pool` - Enable the high-level connection [`Pool`] and the
//!   [`Statements` derive] for declaring reusable collections of prepared
//!   statements. This pulls in a dependency on the `sync` feature of [`tokio`].
//!   Enabled by default.
//! * `typed` - Enable [`TypedStatement`] and the [`typed`] module, wrappers
//!   around statements which encode their bind parameter and column counts in
//!   the type system and reset themselves between uses. Used by the
//!   [`Statements` derive]. Enabled by default.
//!
//! [^sqll-sys]: This is a forwarded sqll-sys option, see <https://docs.rs/sqll-sys>.
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
//! [`Connection::busy_handler`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.busy_handler
//! [`Connection::deserialize`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.deserialize
//! [`Connection::serialize`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.serialize
//! [`examples/axum.rs`]: https://github.com/udoprog/sqll/blob/main/examples/axum.rs
//! [`examples/persons.rs`]: https://github.com/udoprog/sqll/blob/main/examples/persons.rs
//! [`examples/pool.rs`]: https://github.com/udoprog/sqll/blob/main/examples/pool.rs
//! [`examples/tokio_async.rs`]: https://github.com/udoprog/sqll/blob/main/examples/tokio_async.rs
//! [`execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
//! [`next`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
//! [`OpenOptions::no_mutex`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex
//! [`OpenOptions::no_mutex_unchecked`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex_unchecked
//! [`OwnedBytes`]: https://docs.rs/sqll/latest/sqll/struct.OwnedBytes.html
//! [`Pool`]: https://docs.rs/sqll/latest/sqll/struct.Pool.html
//! [`Statements` derive]: https://docs.rs/sqll/latest/sqll/derive.Statements.html
//! [`tokio`]: https://docs.rs/tokio
//! [`prepare_with`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare_with
//! [`prepare`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare
//! [`PrepareWith::persistent`]: https://docs.rs/sqll/latest/sqll/struct.PrepareWith.html#method.persistent
//! [`Row` derive]: https://docs.rs/sqll/latest/sqll/derive.Row.html
//! [`Row`]: https://docs.rs/sqll/latest/sqll/trait.Row.html
//! [`sqlite` crate]: https://github.com/stainless-steel/sqlite
//! [`sqll-sys`]: https://crates.io/crates/sqll-sys
//! [`Statement::into_send`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.into_send
//! [`Statement`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html
//! [`tokio_async` example]: https://github.com/udoprog/sqll/blob/main/examples/tokio_async.rs
//! [`tokio::task::spawn_blocking`]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
//! [`TypedStatement`]: https://docs.rs/sqll/latest/sqll/struct.TypedStatement.html
//! [`typed`]: https://docs.rs/sqll/latest/sqll/typed/index.html
//! [binding query parameters]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
//! [calling `execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
//! [reading rows]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
//! [SQLite]: https://www.sqlite.org

#![no_std]
#![allow(clippy::module_inception)]
#![allow(clippy::new_without_default)]
#![allow(clippy::should_implement_trait)]
#![warn(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(test)]
mod tests;

mod bind;
mod bind_value;
mod bytes;
mod code;
mod column;
mod connection;
mod error;
mod ffi;
mod fixed_blob;
mod fixed_text;
mod from_column;
mod from_unsized_column;
mod index;
mod open_options;
#[cfg(feature = "alloc")]
mod owned;
#[cfg(feature = "alloc")]
mod owned_bytes;
#[cfg(feature = "pool")]
mod pool;
mod row;
mod statement;
mod text;
pub mod ty;
#[cfg(feature = "typed")]
pub mod typed;
mod utils;
mod value;
mod value_type;
mod version;

#[cfg(feature = "pool")]
#[cfg_attr(docsrs, doc(cfg(feature = "pool")))]
#[doc(inline)]
pub use self::pool::{
    ConnectionSetup, EmptySetup, ExclusiveGuard, IsReadOnly, Pool, PoolBuilder, PoolError,
    SharedGuard, Statements,
};

#[cfg(feature = "typed")]
#[cfg_attr(docsrs, doc(cfg(feature = "typed")))]
#[doc(inline)]
pub use self::typed::{BoundStatement, TryFromSendStatementError, TypedStatement};

#[doc(inline)]
pub use self::bind::Bind;
#[doc(inline)]
pub use self::bind_value::BindValue;
#[doc(inline)]
pub use self::code::Code;
#[doc(inline)]
pub use self::column::Column;
#[doc(inline)]
pub use self::connection::{Connection, Prepare, PrepareWith, SendConnection};
#[doc(inline)]
pub use self::error::{CapacityError, DatabaseNotFound, Error, NotThreadSafe, Result};
#[doc(inline)]
pub use self::fixed_blob::FixedBlob;
#[doc(inline)]
pub use self::fixed_text::FixedText;
#[doc(inline)]
pub use self::from_column::FromColumn;
#[doc(inline)]
pub use self::from_unsized_column::FromUnsizedColumn;
#[doc(inline)]
pub use self::index::Index;
#[doc(inline)]
pub use self::open_options::OpenOptions;
#[doc(inline)]
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, cfg(feature = "alloc"))]
pub use self::owned_bytes::OwnedBytes;
#[doc(inline)]
pub use self::row::Row;
#[doc(inline)]
pub use self::statement::{Null, SendStatement, State, Statement};
#[doc(inline)]
pub use self::text::Text;
#[doc(inline)]
pub use self::value::Value;
#[doc(inline)]
pub use self::value_type::ValueType;
#[doc(inline)]
pub use self::version::{lib_version, lib_version_number};

/// Derive macro for [`Bind`].
///
/// This can be used to automatically implement [`Bind`] for a struct and allows
/// the struct to be used for structured binding of multiple parameters into a
/// [`Statement`] using [`bind`].
///
/// This relies on [`BindValue`] being called for each field in the struct. By
/// default the `#[sql(index)]` used starts at 0 and is incremented for each
/// field. This behavior can be modified with attributes.
///
/// This also derive also supports convenient use of named parameters through
/// `[sql(named)]` or a per-field `[sql(name = ..)]`.
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
/// ## Field attributes
///
/// <br>
///
/// #### `#[sql(index = ..)]`
///
/// This allows the index being used for a particular row to be overriden. Note
/// that the underlying binding indexes are 1-based, so this is translated to
/// that by adding 1 to the specified index in order to be compatible with other
/// derives.
///
/// ```
/// use sqll::{Bind, Connection};
///
/// #[derive(Bind)]
/// struct Person<'stmt> {
///     #[sql(index = 1)]
///     name: &'stmt str,
///     #[sql(index = 0)]
///     age: u32,
/// }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///    CREATE TABLE persons (name TEXT, age INTEGER);
/// "#)?;
///
/// let mut stmt = c.prepare("INSERT INTO persons (age, name) VALUES (?, ?)")?;
///
/// stmt.execute(Person { name: "Alice", age: 30 })?;
/// stmt.reset()?;
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// <br>
///
/// #### `#[sql(name = c"..")]`
///
/// This allows for specifying an explicit binding name to use, instead of the
/// default which is to bind by index or a field derived name if `#[sql(named)]`
/// is set.
///
/// Note that the name are literal references to what is expected by the SQLite
/// API, so must be prefixed with `:`. If a string literal is used, it will be
/// checked so that it doesn't contain any null bytes.
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
/// # #[derive(Bind)]
/// # struct PersonStr<'stmt> {
/// #     #[sql(name = ":notname")]
/// #     name: &'stmt str,
/// #     #[sql(name = ":notage")]
/// #     age: u32,
/// # }
///
/// let c = Connection::open_in_memory()?;
///
/// c.execute(r#"
///    CREATE TABLE persons (name TEXT, age INTEGER);
/// "#)?;
///
/// let mut stmt = c.prepare("INSERT INTO persons (name, age) VALUES (:notname, :notage)")?;
/// stmt.execute(Person { name: "Alice", age: 30 })?;
/// stmt.reset()?;
/// # stmt.execute(PersonStr { name: "Alice", age: 30 })?;
/// # stmt.reset()?;
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
/// ## Field attributes
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
///
/// <br>
///
/// #### Missing `#[sql(name)]`?
///
/// Unlike the `Bind` derive, there is no `#[sql(name = ..)]` attribute for the
/// `Row` derive. This is due to the underlying API not efficiently supporting
/// this, and implementing it in `sqll` would either require additional
/// allocations or inefficient per-row lookups.
///
/// Note that it's also not uncommon for column names to simply *not* have a
/// name, such as when computing expressions like `COUNT(*)` or `column + 1`.
///
/// # Errors
///
/// Trying to use the same index multiple times results in an error. This is to
/// ensure that the implementation follows the safety requirements of the [`Row`
/// trait].
///
/// ```compile_fail
/// use sqll::Row;
///
/// #[derive(Row)]
/// struct InvalidRow {
///     #[sql(index = 0)]
///     a: u32,
///     #[sql(index = 0)]
///     b: u32,
/// }
/// ```
///
/// [`Row` trait]: crate::Row
#[cfg(feature = "derive")]
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use sqll_macros::Row;

/// Derive macro for the [`Statements`] trait.
///
/// This implements [`Statements`] for a struct whose fields are the prepared
/// statements you want to re-use. Each statement is prepared as [persistent]
/// and converted into a [`SendStatement`] when the collection is built from a
/// [`Connection`], which makes it suitable for use with the high-level
/// [`Pool`].
///
/// Both named structs and tuple structs are supported.
///
/// # Attributes
///
/// * `#[sql = "..."]` (field) - the SQL of the statement to prepare for this
///   field. The attribute may be repeated on a single field, in which case the
///   fragments are trimmed and joined with a single space. This is convenient
///   for splitting a long query over multiple lines.
/// * `#[sql(read_only)]` (container) - assert that every statement in the
///   collection is read-only. Building the collection fails with a
///   [`PoolError`] if any statement turns out to mutate the database, and
///   the type additionally implements [`IsReadOnly`] so that it can be used as
///   the read side `R` of a [`Pool`].
/// * `#[sql(statements)]` (field) - the field is itself a nested [`Statements`]
///   collection that is built recursively rather than prepared from a query
///   string. This lets you compose larger collections out of smaller, reusable
///   ones. The field type must implement [`Statements`], and a `statements`
///   field cannot also carry a `#[sql = "..."]` query. If the containing struct
///   is `#[sql(read_only)]`, the nested type must also implement [`IsReadOnly`]
///   (i.e. be `#[sql(read_only)]` itself), otherwise the read-only guarantee
///   would be unsound.
///
/// # Examples
///
/// ```no_run
/// use sqll::{SendStatement, Statements, PoolBuilder, OpenOptions};
///
/// // The read side only contains statements that do not mutate the database,
/// // so it can be marked `read_only` and used concurrently.
/// #[derive(Statements)]
/// #[sql(read_only)]
/// struct Read {
///     #[sql = "SELECT name, age"]
///     #[sql = "FROM users"]
///     #[sql = "ORDER BY age"]
///     all_users: SendStatement,
/// }
///
/// #[derive(Statements)]
/// struct Write {
///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
///     insert_user: SendStatement,
/// }
///
/// let mut options = OpenOptions::new();
/// options.no_mutex().create();
/// let pool = PoolBuilder::new(options, 4).open::<Read, Write>("example.db")?;
/// # Ok::<_, sqll::PoolError>(())
/// ```
///
/// ## Nested statements
///
/// A field marked `#[sql(statements)]` is itself a [`Statements`] collection,
/// letting you share a common set of queries across the read and write sides
/// instead of repeating them. Here the write side reuses the read collection so
/// it can both query and mutate through a single value:
///
/// ```no_run
/// use sqll::{SendStatement, Statements, PoolBuilder, OpenOptions};
///
/// #[derive(Statements)]
/// #[sql(read_only)]
/// struct Read {
///     #[sql = "SELECT name, age FROM users ORDER BY age"]
///     all_users: SendStatement,
/// }
///
/// #[derive(Statements)]
/// struct Write {
///     // Reuse every statement in `Read`, accessible as `write.read.all_users`.
///     #[sql(statements)]
///     read: Read,
///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
///     insert_user: SendStatement,
/// }
///
/// let mut options = OpenOptions::new();
/// options.no_mutex().create();
/// let pool = PoolBuilder::new(options, 4).open::<Read, Write>("example.db")?;
/// # Ok::<_, sqll::PoolError>(())
/// ```
///
/// A `read_only` collection may only nest other `read_only` collections, since
/// embedding a mutating collection would make its [`IsReadOnly`] implementation
/// unsound. The following therefore fails to compile:
///
/// ```compile_fail
/// use sqll::{SendStatement, Statements};
///
/// #[derive(Statements)]
/// struct Write {
///     #[sql = "INSERT INTO users (name, age) VALUES (?, ?)"]
///     insert_user: SendStatement,
/// }
///
/// // error: the trait bound `Write: IsReadOnly` is not satisfied
/// #[derive(Statements)]
/// #[sql(read_only)]
/// struct Read {
///     #[sql(statements)]
///     write: Write,
///     #[sql = "SELECT name, age FROM users ORDER BY age"]
///     all_users: SendStatement,
/// }
/// ```
///
/// [persistent]: crate::PrepareWith::persistent
/// [`Connection`]: crate::Connection
/// [`PoolError`]: crate::PoolError
/// [`IsReadOnly`]: crate::IsReadOnly
/// [`Pool`]: crate::Pool
/// [`SendStatement`]: crate::SendStatement
/// [`Statements`]: crate::Statements
#[cfg(all(feature = "derive", feature = "pool"))]
#[cfg_attr(docsrs, doc(cfg(all(feature = "derive", feature = "pool"))))]
pub use sqll_macros::Statements;
