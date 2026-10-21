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

stmt.execute(Person { name: "Bob", age: 40 })?;

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

#### Transactions

[`Connection::transaction`] runs a closure inside of a transaction, committing
it if the closure returns `Ok` and rolling it back if it returns `Err` or
panics. It uses `BEGIN IMMEDIATE`, which takes the write lock up front where
the busy timeout applies, see [`TransactionBehavior`] for why that is the right
default for writers. [`ExclusiveGuard::transaction`] does the same for the write
side of a [`Pool`].

```rust
use sqll::{Connection, Error};

let c = Connection::open_in_memory()?;
c.execute("CREATE TABLE users (name TEXT, age INTEGER)")?;

let result = c.transaction(|c| {
    c.execute("INSERT INTO users VALUES ('Alice', 42)")?;
    Err::<(), _>(Error::custom("changed my mind"))
});

assert!(result.is_err());

let mut stmt = c.prepare("SELECT COUNT(*) FROM users")?;
assert_eq!(stmt.first::<i64>(())?, Some(0));
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

The default features are `std`, `derive`, `threadsafe`, `pool` and `typed`.
Each item which needs a feature is marked with it in the documentation.

* `std` - Enable usage of the Rust standard library. Enabled by default.
  Needed by the functions which open a database from a [`Path`], such as
  [`Connection::open`], [`OpenOptions::open`], [`PoolBuilder::open`],
  [`Pool::new`] and [`Prepared::open`]. Without it, open databases by a C
  string through [`Connection::open_c_str`], [`OpenOptions::open_c_str`],
  [`PoolBuilder::open_c_str`], [`Pool::new_c_str`] and
  [`Prepared::open_c_str`], or in memory.
* `alloc` - Enable usage of the Rust alloc library. Enabled by default
  through `std`, and also enabled by `pool`. The crate builds without it,
  but APIs which need to allocate are then unavailable, such as
  [`OwnedBytes`], [`OwnedValue`], [`Migrations`], [`Error::get_ref`],
  [`Connection::serialize`], [`Connection::deserialize`],
  [`Connection::busy_handler`] and reading or binding `String`, `Vec<u8>`
  and `Box<T>`. Errors then keep only their [`Code`], and drop their message,
  source and column context.
* `derive` - Add a dependency to and re-export of the [`Row` derive],
  [`Bind` derive], [`FromColumn` derive] and [`BindValue` derive] macros, and
  with `pool` the [`Statements` derive]. Enabled by default.
* `bundled` - Use a bundled version of sqlite. The bundle is provided by the
  [`sqll-sys`] crate and the sqlite version used is part of the build
  metadata of that crate[^sqll-sys].
* `threadsafe` - Enable usage of sqlite with the threadsafe option set. We
  assume any system level libraries have this build option enabled, and
  opening a connection fails if they do not. If this is disabled the
  `bundled` feature has to be enabled. Enabled by default. With this feature
  `Connection` implements `Send`, and every connection is opened in the
  "serialized" threading mode, so [`OpenOptions::no_mutex`] has no effect and
  is deprecated, and the unsafe [`OpenOptions::no_mutex_unchecked`] is needed
  to avoid the per-connection mutex. It also enables the methods of the
  [`Pool`] guards which hand out their [`Connection`], such as
  [`ExclusiveGuard::transaction`]. If `threadsafe` is disabled, `Connection`
  does not implement `Send`. But it is also important to understand that if
  this option is not set, sqlite **may not be used by multiple threads at
  all** even if threads have distinct connections[^sqll-sys].
* `strict` - Enable usage of sqlite with the strict compiler options
  enabled[^sqll-sys].
* `pool` - Enable the high-level connection [`Pool`], [`Pool::read`] and
  [`Pool::write`] which run a closure on a blocking thread, and [`Prepared`]
  for using a collection of statements without a pool. With `derive` it also
  enables the [`Statements` derive] for declaring such reusable collections
  of prepared statements. Implies `alloc`, and pulls in a dependency on the
  `sync` and `rt` features of [`tokio`]. Enabled by default.
* `typed` - Enable [`TypedStatement`] and the [`typed`] module, wrappers
  around statements which encode their bind parameter and column counts in
  the type system and reset themselves between uses. Used by the
  [`Statements` derive]. Enabled by default.

The rest of the crate needs no feature of its own, including
[`Connection::transaction`], [`JournalMode`] and [`Synchronous`], and the
[`FromColumn`], [`BindValue`], [`Bind`] and [`Row`] traits. A crate which only
implements those traits can depend on sqll with `default-features = false`,
though [`sqll-sys`] currently refuses to build unless one of `threadsafe` or
`bundled` is enabled somewhere in the build.

[^sqll-sys]: This is a forwarded sqll-sys option, see <https://docs.rs/sqll-sys>.

<br>

## License

This is a rewrite of the [`sqlite` crate], and components used from there
have been copied under the MIT license.

[`axum`]: https://docs.rs/axum
[`Bind` derive]: https://docs.rs/sqll/latest/sqll/derive.Bind.html
[`BindValue`]: https://docs.rs/sqll/latest/sqll/trait.BindValue.html
[`BindValue` derive]: https://docs.rs/sqll/latest/sqll/derive.BindValue.html
[`bind`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
[`Bind`]: https://docs.rs/sqll/latest/sqll/trait.Bind.html
[`Code`]: https://docs.rs/sqll/latest/sqll/struct.Code.html
[`Connection`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#thread-safety
[`Connection::busy_handler`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.busy_handler
[`Connection::open`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.open
[`Connection::open_c_str`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.open_c_str
[`Connection::transaction`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.transaction
[`Connection::deserialize`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.deserialize
[`Connection::serialize`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.serialize
[`Error::get_ref`]: https://docs.rs/sqll/latest/sqll/struct.Error.html#method.get_ref
[`ExclusiveGuard::transaction`]: https://docs.rs/sqll/latest/sqll/struct.ExclusiveGuard.html#method.transaction
[`examples/axum.rs`]: https://github.com/udoprog/sqll/blob/main/examples/axum.rs
[`examples/persons.rs`]: https://github.com/udoprog/sqll/blob/main/examples/persons.rs
[`examples/pool.rs`]: https://github.com/udoprog/sqll/blob/main/examples/pool.rs
[`examples/tokio_async.rs`]: https://github.com/udoprog/sqll/blob/main/examples/tokio_async.rs
[`execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
[`FromColumn`]: https://docs.rs/sqll/latest/sqll/trait.FromColumn.html
[`FromColumn` derive]: https://docs.rs/sqll/latest/sqll/derive.FromColumn.html
[`JournalMode`]: https://docs.rs/sqll/latest/sqll/enum.JournalMode.html
[`Migrations`]: https://docs.rs/sqll/latest/sqll/struct.Migrations.html
[`next`]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
[`OpenOptions::no_mutex`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex
[`OpenOptions::no_mutex_unchecked`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.no_mutex_unchecked
[`OpenOptions::open`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.open
[`OpenOptions::open_c_str`]: https://docs.rs/sqll/latest/sqll/struct.OpenOptions.html#method.open_c_str
[`OwnedBytes`]: https://docs.rs/sqll/latest/sqll/struct.OwnedBytes.html
[`OwnedValue`]: https://docs.rs/sqll/latest/sqll/enum.OwnedValue.html
[`Path`]: https://doc.rust-lang.org/std/path/struct.Path.html
[`Pool`]: https://docs.rs/sqll/latest/sqll/struct.Pool.html
[`Prepared`]: https://docs.rs/sqll/latest/sqll/struct.Prepared.html
[`Prepared::open`]: https://docs.rs/sqll/latest/sqll/struct.Prepared.html#method.open
[`Prepared::open_c_str`]: https://docs.rs/sqll/latest/sqll/struct.Prepared.html#method.open_c_str
[`Pool::new`]: https://docs.rs/sqll/latest/sqll/struct.Pool.html#method.new
[`Pool::new_c_str`]: https://docs.rs/sqll/latest/sqll/struct.Pool.html#method.new_c_str
[`Pool::read`]: https://docs.rs/sqll/latest/sqll/struct.Pool.html#method.read
[`Pool::write`]: https://docs.rs/sqll/latest/sqll/struct.Pool.html#method.write
[`PoolBuilder::open`]: https://docs.rs/sqll/latest/sqll/struct.PoolBuilder.html#method.open
[`PoolBuilder::open_c_str`]: https://docs.rs/sqll/latest/sqll/struct.PoolBuilder.html#method.open_c_str
[`Statements` derive]: https://docs.rs/sqll/latest/sqll/derive.Statements.html
[`Synchronous`]: https://docs.rs/sqll/latest/sqll/enum.Synchronous.html
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
[`TransactionBehavior`]: https://docs.rs/sqll/latest/sqll/enum.TransactionBehavior.html
[`tokio_async` example]: https://github.com/udoprog/sqll/blob/main/examples/tokio_async.rs
[`tokio::task::spawn_blocking`]: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
[`TypedStatement`]: https://docs.rs/sqll/latest/sqll/struct.TypedStatement.html
[`typed`]: https://docs.rs/sqll/latest/sqll/typed/index.html
[binding query parameters]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.bind
[calling `execute`]: https://docs.rs/sqll/latest/sqll/struct.Connection.html#method.execute
[reading rows]: https://docs.rs/sqll/latest/sqll/struct.Statement.html#method.next
[SQLite]: https://www.sqlite.org
