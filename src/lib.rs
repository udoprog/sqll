//! [<img alt="github" src="https://img.shields.io/badge/github-udoprog/sqlite--ll-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/sqlite-ll)
//! [<img alt="crates.io" src="https://img.shields.io/crates/v/sqlite-ll.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/sqlite-ll)
//! [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-sqlite--ll-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/sqlite-ll)
//!
//! Low-level interface to the [SQLite] database.
//!
//! This is a rewrite of the [sqlite crate], and components used from there have
//! been copied under the MIT license.
//!
//! <br>
//!
//! ## Why do we need another sqlite interface?
//!
//! It is difficult to use prepared statements with existing crates, because
//! they are all implemented in a manner which requires the caller to borrow the
//! connection in use.
//!
//! Prepared statements can be expensive to create and *should* be cached and
//! re-used to achieve the best performance. This library uses
//! `sqlite3_close_v2` when the connection is dropped, causing the closing of
//! the connection to be delayed until resources associated with it has been
//! closed.
//!
//! The way this crate gets around this is by making the `prepare` function
//! `unsafe`, so the impetus is on the caller to ensure that the connection it's
//! related to stays alive for the duration of the prepared statement.
//!
//! <br>
//!
//! ## Example
//!
//! Open a connection, create a table, and insert some rows:
//!
//! ```
//! use sqlite_ll::Connection;
//!
//! let c = Connection::memory()?;
//!
//! c.execute(
//!     "
//!     CREATE TABLE users (name TEXT, age INTEGER);
//!     INSERT INTO users VALUES ('Alice', 42);
//!     INSERT INTO users VALUES ('Bob', 69);
//!     ",
//! )?;
//! # Ok::<_, sqlite_ll::Error>(())
//! ```
//!
//! Querying data using a parepared statement with bindings:
//!
//! ```
//! use sqlite_ll::State;
//! # use sqlite_ll::Connection;
//! # let c = Connection::memory()?;
//! # c.execute(
//! #     "
//! #     CREATE TABLE users (name TEXT, age INTEGER);
//! #     INSERT INTO users VALUES ('Alice', 42);
//! #     INSERT INTO users VALUES ('Bob', 69);
//! #     ",
//! # )?;
//! let mut stmt = c.prepare("SELECT * FROM users WHERE age > ?")?;
//!
//! let mut results = Vec::new();
//!
//! for age in [40, 50] {
//!     stmt.reset()?;
//!     stmt.bind(1, age)?;
//!
//!     while let State::Row = stmt.step()? {
//!         results.push((stmt.read::<String>(0)?, stmt.read::<i64>(1)?));
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
//! # Ok::<_, sqlite_ll::Error>(())
//! ```
//!
//! [sqlite crate]: https://github.com/stainless-steel/sqlite
//! [SQLite]: https://www.sqlite.org

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(not(feature = "alloc"))]
compile_error!("The `alloc` feature must be enabled to use this crate.");

#[cfg(test)]
mod tests;

mod bytes;
mod connection;
mod error;
mod owned;
mod statement;
mod utils;
mod value;

pub use self::connection::{Connection, OpenOptions, Prepare};
pub use self::error::{Code, Error, Result};
pub use self::statement::{Bindable, FixedBytes, Null, Readable, State, Statement};
pub use self::value::{Type, Value};

/// Return the version number of SQLite.
///
/// For instance, the version `3.8.11.1` corresponds to the integer `3008011`.
#[inline]
pub fn version() -> u64 {
    unsafe { sqlite3_sys::sqlite3_libversion_number() as u64 }
}
