//! Create an in-memory database connection and serve it using [`axum`].
//!
//! [`axum`]: https://docs.rs/axum

use std::fmt::{self, Write};
use std::sync::Arc;

use anyhow::Result;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Extension, Router};
use sqll::{OpenOptions, Prepare, SendStatement};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::{self, JoinError};

struct Statements {
    select: SendStatement,
}

#[derive(Clone)]
struct Database {
    stmts: Arc<Mutex<Statements>>,
}

fn setup_db() -> Result<Database> {
    // SAFETY: We set up an unsynchronized connection which is unsafe, but we
    // provide external syncrhonization so it is fine. This avoids the overhead
    // of sqlite using internal locks.
    let c = OpenOptions::new()
        .create()
        .read_write()
        .no_mutex()
        .open_in_memory()?;

    c.execute(
        r#"
        CREATE TABLE users (name TEXT PRIMARY KEY NOT NULL, age INTEGER);

        INSERT INTO users VALUES ('Alice', 42), ('Bob', 69), ('Charlie', 21);
        "#,
    )?;

    let select = c.prepare_with("SELECT name, age FROM users", Prepare::PERSISTENT)?;

    let inner = unsafe {
        Statements {
            select: select.into_send(),
        }
    };

    Ok(Database {
        stmts: Arc::new(Mutex::new(inner)),
    })
}

struct WebError {
    kind: WebErrorKind,
}

impl IntoResponse for WebError {
    fn into_response(self) -> axum::response::Response {
        match self.kind {
            WebErrorKind::DatabaseError(err) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                err.to_string(),
            )
                .into_response(),
            WebErrorKind::Format => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Formatting error",
            )
                .into_response(),
            WebErrorKind::JoinError(err) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Task join error: {}", err),
            )
                .into_response(),
        }
    }
}

enum WebErrorKind {
    DatabaseError(sqll::Error),
    Format,
    JoinError(JoinError),
}

impl From<sqll::Error> for WebError {
    fn from(err: sqll::Error) -> Self {
        WebError {
            kind: WebErrorKind::DatabaseError(err),
        }
    }
}

impl From<fmt::Error> for WebError {
    fn from(_: fmt::Error) -> Self {
        WebError {
            kind: WebErrorKind::Format,
        }
    }
}

impl From<JoinError> for WebError {
    fn from(error: JoinError) -> Self {
        WebError {
            kind: WebErrorKind::JoinError(error),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let db = setup_db()?;

    let app = Router::new().route("/", get(get_user)).layer(Extension(db));
    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    println!("Listening on http://{}", listener.local_addr()?);
    axum::serve::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn get_user(Extension(db): Extension<Database>) -> Result<Html<String>, WebError> {
    let mut db = db.stmts.lock_owned().await;

    let task = task::spawn_blocking(move || {
        let mut out = String::with_capacity(1024);
        db.select.reset()?;

        writeln!(out, "<!DOCTYPE html>")?;
        writeln!(out, "<html>")?;
        writeln!(out, "<head><title>User List</title></head>")?;
        writeln!(out, "<body>")?;

        while let Some((name, age)) = db.select.next::<(&str, i64)>()? {
            writeln!(out, "<div>Name: {name}, Age: {age}</div>")?;
        }

        writeln!(out, "</body>")?;
        writeln!(out, "</html>")?;
        Ok(Html(out))
    });

    task.await?
}
