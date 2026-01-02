# sqll

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/sqll-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/sqll)
[<img alt="crates.io" src="https://img.shields.io/crates/v/sqll.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/sqll)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-sqll-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/sqll)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/sqll/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/sqll/actions?query=branch%3Amain)

Efficient interface interface to [SQLite] that doesn't get in your way.

<br>

## Usage

The two primary methods to interact with an SQLite database through this
crate is through [`execute`] and [`prepare`].

[`execute`] is used for batch statements, and allows for multiple queries to
be specified. [`prepare`] only allows for a single statement to be
specified, but in turn permits [reading rows] and [binding query
parameters].

Special consideration needs to be taken about the thread safety of
connections. You can read more about that in the [`Connection`]
documentation.

You can find simple examples of this below.

<br>

## Examples

* [`examples/persons.rs`] - A simple table with users, a primary key,
  inserting and querying.
* [`examples/axum.rs`] - Create an in-memory database connection and serve
  it using [`axum`]. This showcases how do properly handle external
  synchronization for the best performance.

<br>

## Features

* `std` - Enable usage of the Rust standard library. Enabled by default.
* `alloc` - Enable usage of the Rust alloc library. This is required and is
  enabled by default. Disabling this option will currently cause a compile
  error.
* `bundled` - Use a bundled version of sqlite. The bundle is provided by the
  [`sqll-sys`] crate and the sqlite version used is part of the build
  metadata of that crate.
* `threadsafe` - Enable usage of sqlite with the threadsafe option set. We
  assume any system level libraries have this build option enabled, if this
  is disabled the `bundled` feature has to be enabled. If `threadsafe` is
  disabled, `Connection` and `Statement` does not implement `Send`. But it
  is also important to understand that if this option is not set, sqlite
  **may not be used by multiple threads at all** even if threads have
  distinct connections. To disable mutexes instead which allows for
  efficient one connection per thread the [`OpenOptions::no_mutex`] option
  should be used instead.

<br>

## Example

Open an in-memory connection, create a table, insert and query some rows:

```rust
use sqll::{Connection, Result};

let c = Connection::open_memory()?;

c.execute(r#"
    CREATE TABLE users (name TEXT, age INTEGER);

    INSERT INTO users VALUES ('Alice', 42);
    INSERT INTO users VALUES ('Bob', 69);
"#)?;

let results = c.prepare("SELECT name, age FROM users ORDER BY age")?
    .iter::<(String, u32)>()
    .collect::<Result<Vec<_>>>()?;

assert_eq!(results, [("Alice".to_string(), 42), ("Bob".to_string(), 69)]);
```

<br>

## The [`FromRow`] helper trait.

For the example below, we can define a `Person` struct that binds to the row
conveniently using the [`FromRow` derive] macro.

```rust
use sqll::{Connection, FromRow, Result};

#[derive(FromRow)]
struct Person<'stmt> {
    name: &'stmt str,
    age: u32,
}

let mut c = Connection::open_memory()?;

c.execute(r#"
    CREATE TABLE users (name TEXT, age INTEGER);

    INSERT INTO users VALUES ('Alice', 42);
    INSERT INTO users VALUES ('Bob', 69);
"#)?;

let mut results = c.prepare("SELECT name, age FROM users ORDER BY age")?;

while let Some(person) = results.next_row::<Person<'_>>()? {
    println!("{} is {} years old", person.name, person.age);
}
```

<br>

#### Prepared Statements

Correct handling of prepared statements are crucial to get good performance
out of sqlite. They contain all the state associated with a query and are
expensive to construct so they should be re-used.

Using a [`Prepare::PERSISTENT`] prepared statement to perform multiple
queries:

```rust
use sqll::{Connection, Prepare};

let c = Connection::open_memory()?;

c.execute(r#"
    CREATE TABLE users (name TEXT, age INTEGER);

    INSERT INTO users VALUES ('Alice', 42);
    INSERT INTO users VALUES ('Bob', 69);
"#)?;

let mut stmt = c.prepare_with("SELECT * FROM users WHERE age > ?", Prepare::PERSISTENT)?;

let mut results = Vec::new();

for age in [40, 50] {
    stmt.reset()?;
    stmt.bind(1, age)?;

    while let Some(row) = stmt.next()? {
        results.push((row.get::<String>(0)?, row.get::<i64>(1)?));
    }
}

let expected = vec![
    (String::from("Alice"), 42),
    (String::from("Bob"), 69),
    (String::from("Bob"), 69),
];

assert_eq!(results, expected);
```

<br>

## Why do we need another sqlite interface?

For other low-level crates, it is difficult to set up and use prepared
statements, They are mostly implemented in a manner which requires the
caller to borrow the connection in use.

This library implements database objects through the v2 API which ensures
that the database remains alive for as long as objects associated with it
are alive. This is implemented in the SQLite library itself.

Prepared statements can be expensive to create and *should* be cached and
re-used to achieve the best performance. This is something that crates like
`rusqlite` implements, but can easily be done manually by simply storing the
[`Statement`] object directly. Statements can also benefit from using the
[`Prepare::PERSISTENT`] option which this library supports through
[`prepare_with`].

This library is designed to the extent possible to avoid intermediary
allocations. For example [calling `execute`] doesn't allocate externally of
the sqlite3 bindings or we require that c-strings are used when SQLite
itself doesn't provide an API for using Rust strings directly.

<br>

## License

This is a rewrite of the [`sqlite` crate], and components used from there
have been copied under the MIT license.

[`axum`]: https://docs.rs/axum
[`Connection`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#thread-safety
[`examples/axum.rs`]: https://github.com/udoprog/sqll/blob/main/examples/axum.rs
[`examples/persons.rs`]: https://github.com/udoprog/sqll/blob/main/examples/persons.rs
[`execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
[`FromRow` derive]: https://docs.rs/sqll/latest/sqll/derive.FromRow.html
[`FromRow`]: https://docs.rs/sqll/latest/sqll/trait.FromRow.html
[`OpenOptions::no_mutex`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex
[`prepare_with`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare_with
[`Prepare::PERSISTENT`]: https://docs.rs/sqll/latest/sqll/struct.Prepare.html#associatedconstant.PERSISTENT
[`prepare`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare
[`sqlite` crate]: https://github.com/stainless-steel/sqlite
[`sqll-sys`]: https://crates.io/crates/sqll-sys
[`Statement`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html
[binding query parameters]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
[calling `execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
[reading rows]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
[SQLite]: https://www.sqlite.org
