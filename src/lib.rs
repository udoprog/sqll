//! [<img alt="github" src="https://img.shields.io/badge/github-udoprog/sqll-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/sqll)
//! [<img alt="crates.io" src="https://img.shields.io/crates/v/sqll.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/sqll)
//! [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-sqll-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/sqll)
//!
//! Efficient interface interface to [SQLite] that doesn't get in your way.
//!
//! <br>
//!
//! ## Examples
//!
//! * [`examples/persons.rs`] - A simple table with users, a primary key,
//!   inserting and querying.
//! * [`examples/axum.rs`] - Create an in-memory database connection and serve
//!   it using [`axum`]. This showcases how do properly handle external
//!   synchronization for the best performance.
//!
//! <br>
//!
//! ## Features
//!
//! * `std` - Enable usage of the Rust standard library. Enabled by default.
//! * `alloc` - Enable usage of the Rust alloc library. This is required and is
//!   enabled by default. Disabling this option will currently cause a compile
//!   error.
//! * `bundled` - Use a bundled version of sqlite. The bundle is provided by the
//!   [`sqll-sys`] crate and the sqlite version used is part of the build
//!   metadata of that crate.
//!
//! <br>
//!
//! ## Why do we need another sqlite interface?
//!
//! It is difficult to set up and use prepared statements with existing crates,
//! because they are all implemented in a manner which requires the caller to
//! borrow the connection in use.
//!
//! Prepared statements can be expensive to create and *should* be cached and
//! re-used to achieve the best performance. Statements can also benefit from
//! using the [`Prepare::PERSISTENT`] option This library uses
//! `sqlite3_close_v2` when the connection is dropped, causing the closing of
//! the connection to be delayed until resources associated with it has been
//! closed.
//!
//! We've also designed this library to avoid intermediary allocations. So for
//! example [calling `execute`] doesn't allocate externally of the sqlite3
//! bindings. This was achieved by porting the execute implementation from the
//! sqlite library and works because sqlite actually uses UTF-8 internally but
//! this is not exposed in the legacy C API that other crates use to execute
//! statements.
//!
//! <br>
//!
//! ## Example
//!
//! Open an in-memory connection, create a table, and insert some rows:
//!
//! ```
//! use sqll::Connection;
//!
//! let c = Connection::open_memory()?;
//!
//! c.execute("
//!     CREATE TABLE users (name TEXT, age INTEGER);
//!
//!     INSERT INTO users VALUES ('Alice', 42);
//!     INSERT INTO users VALUES ('Bob', 69);
//! ")?;
//! # Ok::<_, sqll::Error>(())
//! ```
//!
//! <br>
//!
//! #### Prepared Statements
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
//! let c = Connection::open_memory()?;
//! c.execute("
//!     CREATE TABLE users (name TEXT, age INTEGER);
//!
//!     INSERT INTO users VALUES ('Alice', 42);
//!     INSERT INTO users VALUES ('Bob', 69);
//! ")?;
//!
//! let mut stmt = c.prepare_with("SELECT * FROM users WHERE age > ?", Prepare::PERSISTENT)?;
//!
//! let mut results = Vec::new();
//!
//! for age in [40, 50] {
//!     stmt.reset()?;
//!     stmt.bind(1, age)?;
//!
//!     while let Some(row) = stmt.next()? {
//!         results.push((row.get::<String>(0)?, row.get::<i64>(1)?));
//!     }
//! }
//!
//! let expected = vec![
//!     (String::from("Alice"), 42),
//!     (String::from("Bob"), 69),
//!     (String::from("Bob"), 69),
//! ];
//!
//! assert_eq!(results, expected);
//! # Ok::<_, sqll::Error>(())
//! ```
//!
//! <br>
//!
//! ## License
//!
//! This is a rewrite of the [`sqlite` crate], and components used from there
//! have been copied under the MIT license.
//!
//! [`axum`]: https://docs.rs/axum
//! [`examples/axum.rs`]: https://github.com/udoprog/sqll/blob/main/examples/axum.rs
//! [`examples/persons.rs`]: https://github.com/udoprog/sqll/blob/main/examples/persons.rs
//! [`Prepare::PERSISTENT`]: https://docs.rs/sqll/latest/sqll/struct.Prepare.html#associatedconstant.PERSISTENT
//! [`sqlite` crate]: https://github.com/stainless-steel/sqlite
//! [`sqll-sys`]: https://crates.io/crates/sqll-sys
//! [calling `execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
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

mod bindable;
mod borrowable;
mod bytes;
mod connection;
mod error;
mod ffi;
mod fixed_bytes;
mod gettable;
mod owned;
mod sink;
mod statement;
#[cfg(test)]
mod tests;
mod utils;
mod value;

#[doc(inline)]
pub use self::bindable::Bindable;
#[doc(inline)]
pub use self::borrowable::Borrowable;
#[doc(inline)]
pub use self::connection::{Connection, OpenOptions, Prepare};
#[doc(inline)]
pub use self::error::{Code, Error, Result};
#[doc(inline)]
pub use self::fixed_bytes::FixedBytes;
#[doc(inline)]
pub use self::gettable::Gettable;
#[doc(inline)]
pub use self::sink::Sink;
#[doc(inline)]
pub use self::statement::{Null, State, Statement};
#[doc(inline)]
pub use self::value::{Type, Value};

/// Return the version number of SQLite.
///
/// For instance, the version `3.8.11.1` corresponds to the integer `3008011`.
#[inline]
pub fn version() -> u64 {
    unsafe { crate::ffi::sqlite3_libversion_number() as u64 }
}
