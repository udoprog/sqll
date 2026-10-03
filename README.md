# sqll

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/sqll-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/sqll)
[<img alt="crates.io" src="https://img.shields.io/crates/v/sqll.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/sqll)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-sqll-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/sqll)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/sqll/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/sqll/actions?query=branch%3Amain)

Efficient interface to [SQLite] that doesn't get in your way.

<br>

## Usage

The two primary methods to interact with an SQLite database through this
crate is through [`execute`] and [`prepare`].

The [`execute`] function is used for batch statements, and allows for
multiple queries to be specified. [`prepare`] only allows for a single
statement to be specified, but in turn permits [reading rows] and [binding
query parameters].

Special consideration needs to be taken about the thread safety of
connections. You can read more about that in the [`Connection`]
documentation.

You can find simple examples of this below.

<br>

#### Examples

* [`examples/persons.rs`] - A simple table with users, a primary key,
  inserting and querying.
* [`examples/axum.rs`] - Create an in-memory database connection and serve
  it using [`axum`]. This showcases how to properly handle external
  synchronization for the best performance in a real-world scenario.
* [`examples/tokio_async.rs`] - Using `sqll` in an asynchronous context with
  `tokio`.
* [`examples/pool.rs`] - Using the high-level [`Pool`] to share read-only
  connections and serialize writes from asynchronous tasks.

<br>

#### Connecting and querying

Here is a simple example of setting up an in-memory connection, creating a
table, inserting and querying back some rows:

```rust
use sqll::{Connection, Result};

let c = Connection::open_in_memory()?;

c.execute(r#"
    CREATE TABLE users (name TEXT, age INTEGER);

    INSERT INTO users VALUES ('Alice', 42);
    INSERT INTO users VALUES ('Bob', 52);
"#)?;

let results = c.prepare("SELECT name, age FROM users ORDER BY age")?
    .iter::<(String, u32)>()
    .collect::<Result<Vec<_>>>()?;

assert_eq!(results, [("Alice".to_string(), 42), ("Bob".to_string(), 52)]);
```

<br>

#### The [`Row`] trait.

The [`Row`] trait can be used to conveniently read rows from a statement
using [`next`]. It can be conveniently implemented using the [`Row`
derive].

```rust
use sqll::{Connection, Row};

#[derive(Row)]
struct Person<'stmt> {
    name: &'stmt str,
    age: u32,
}

let mut c = Connection::open_in_memory()?;

c.execute(r#"
    CREATE TABLE users (name TEXT, age INTEGER);

    INSERT INTO users VALUES ('Alice', 42);
    INSERT INTO users VALUES ('Bob', 52);
"#)?;

let mut results = c.prepare("SELECT name, age FROM users ORDER BY age")?;

while let Some(person) = results.next::<Person<'_>>()? {
    println!("{} is {} years old", person.name, person.age);
}
```

<br>

#### The [`Bind`] trait.

The [`Bind`] trait can be used to conveniently [`bind`] parameters to
prepared statements, and it can conveniently be implemented for structs
using the [`Bind` derive].

```rust
use sqll::{Bind, Connection, Row};

#[derive(Bind, Row, PartialEq, Debug)]
#[sql(named)]
struct Person<'stmt> {
    name: &'stmt str,
    age: u32,
}

let c = Connection::open_in_memory()?;

c.execute(r#"
   CREATE TABLE persons (name TEXT, age INTEGER);
"#)?;

let mut stmt = c.prepare("INSERT INTO persons (name, age) VALUES (:name, :age)")?;

stmt.execute(Person { name: "Alice", age: 30 })?;
stmt.reset()?;

stmt.execute(Person { name: "Bob", age: 40 })?;
stmt.reset()?;

let mut query = c.prepare("SELECT name, age FROM persons ORDER BY age")?;

let p = query.next::<Person<'_>>()?;
assert_eq!(p, Some(Person { name: "Alice", age: 30 }));

let p = query.next::<Person<'_>>()?;
assert_eq!(p, Some(Person { name: "Bob", age: 40 }));

query.reset()?;
```

<br>

#### Efficient use of prepared Statements

Correct handling of prepared statements are crucial to get good performance
out of sqlite. They contain all the state associated with a query and are
expensive to construct so they should be re-used.

Using a [`PrepareWith::persistent`] prepared statement to perform multiple
queries:

```rust
use sqll::Connection;

let c = Connection::open_in_memory()?;

c.execute(r#"
    CREATE TABLE users (name TEXT, age INTEGER);

    INSERT INTO users VALUES ('Alice', 42);
    INSERT INTO users VALUES ('Bob', 52);
"#)?;

let mut stmt = c.prepare_with("SELECT * FROM users WHERE age > ?")
    .persistent()
    .build()?;

let mut rows = Vec::new();

for age in [40, 50] {
    stmt.bind(age)?;

    while let Some(row) = stmt.next::<(String, i64)>()? {
        rows.push(row);
    }

    stmt.reset()?;
}

let expected = vec![
    (String::from("Alice"), 42),
    (String::from("Bob"), 52),
    (String::from("Bob"), 52),
];

assert_eq!(rows, expected);
```

<br>

#### Use in asynchronous contexts

In order for sqlite to be used in asynchronous contexts, the [`Statement`]
object usually needs to be `Send` and external synchronization necessary.
Since sqlite is a synchronous library we have to defer any work done to a
thread pool such as the one provided by [`tokio::task::spawn_blocking`]. To
make a [`Statement`] `Send` you can use [`Statement::into_send`], but using
it is `unsafe` since correct behavior depends on the build and runtime
configuration of the sqlite library in use.

See the [`tokio_async` example] for a complete example.

<br>

## Features

* `std` - Enable usage of the Rust standard library. Enabled by default.
* `alloc` - Enable usage of the Rust alloc library. Enabled by default
  through `std`, and also enabled by `pool`. The crate builds without it,
  but APIs which need to allocate are then unavailable, such as
  [`OwnedBytes`], [`Connection::serialize`], [`Connection::deserialize`],
  [`Connection::busy_handler`] and reading columns into `String` or
  `Vec<u8>`.
* `derive` - Add a dependency to and re-export of the [`Row` derive]
  macro.
* `bundled` - Use a bundled version of sqlite. The bundle is provided by the
  [`sqll-sys`] crate and the sqlite version used is part of the build
  metadata of that crate[^sqll-sys].
* `threadsafe` - Enable usage of sqlite with the threadsafe option set. We
  assume any system level libraries have this build option enabled, and
  opening a connection fails if they do not. If this is disabled the
  `bundled` feature has to be enabled. With this feature `Connection`
  implements `Send`, and every connection is opened in the "serialized"
  threading mode, so [`OpenOptions::no_mutex`] has no effect and the unsafe
  [`OpenOptions::no_mutex_unchecked`] is needed to avoid the per-connection
  mutex. If `threadsafe` is disabled, `Connection` does not implement
  `Send`. But it is also important to understand that if this option is not
  set, sqlite **may not be used by multiple threads at all** even if threads
  have distinct connections[^sqll-sys].
* `strict` - Enable usage of sqlite with the strict compiler options
  enabled[^sqll-sys].
* `pool` - Enable the high-level connection [`Pool`] and the
  [`Statements` derive] for declaring reusable collections of prepared
  statements. This pulls in a dependency on the `sync` feature of [`tokio`].
  Enabled by default.
* `typed` - Enable [`TypedStatement`] and the [`typed`] module, wrappers
  around statements which encode their bind parameter and column counts in
  the type system and reset themselves between uses. Used by the
  [`Statements` derive]. Enabled by default.

[^sqll-sys]: This is a forwarded sqll-sys option, see <https://docs.rs/sqll-sys>.

<br>

## License

This is a rewrite of the [`sqlite` crate], and components used from there
have been copied under the MIT license.

[`axum`]: https://docs.rs/axum
[`Bind` derive]: https://docs.rs/sqll/latest/sqll/derive.Bind.html
[`bind`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
[`Bind`]: https://docs.rs/sqll/latest/sqll/trait.Bind.html
[`Connection`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#thread-safety
[`Connection::busy_handler`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.busy_handler
[`Connection::deserialize`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.deserialize
[`Connection::serialize`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.serialize
[`examples/axum.rs`]: https://github.com/udoprog/sqll/blob/main/examples/axum.rs
[`examples/persons.rs`]: https://github.com/udoprog/sqll/blob/main/examples/persons.rs
[`examples/pool.rs`]: https://github.com/udoprog/sqll/blob/main/examples/pool.rs
[`examples/tokio_async.rs`]: https://github.com/udoprog/sqll/blob/main/examples/tokio_async.rs
[`execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
[`next`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
[`OpenOptions::no_mutex`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex
[`OpenOptions::no_mutex_unchecked`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex_unchecked
[`OwnedBytes`]: https://docs.rs/sqll/latest/sqll/struct.OwnedBytes.html
[`Pool`]: https://docs.rs/sqll/latest/sqll/struct.Pool.html
[`Statements` derive]: https://docs.rs/sqll/latest/sqll/derive.Statements.html
[`tokio`]: https://docs.rs/tokio
[`prepare_with`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare_with
[`prepare`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.prepare
[`PrepareWith::persistent`]: https://docs.rs/sqll/latest/sqll/struct.PrepareWith.html#method.persistent
[`Row` derive]: https://docs.rs/sqll/latest/sqll/derive.Row.html
[`Row`]: https://docs.rs/sqll/latest/sqll/trait.Row.html
[`sqlite` crate]: https://github.com/stainless-steel/sqlite
[`sqll-sys`]: https://crates.io/crates/sqll-sys
[`Statement::into_send`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.into_send
[`Statement`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html
[`tokio_async` example]: https://github.com/udoprog/sqll/blob/main/examples/tokio_async.rs
[`tokio::task::spawn_blocking`]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
[`TypedStatement`]: https://docs.rs/sqll/latest/sqll/struct.TypedStatement.html
[`typed`]: https://docs.rs/sqll/latest/sqll/typed/index.html
[binding query parameters]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
[calling `execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
[reading rows]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
[SQLite]: https://www.sqlite.org
