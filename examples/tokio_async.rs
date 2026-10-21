use anyhow::Result;
use sqll::{OpenOptions, SendStatement};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task;

struct Statements {
    select: SendStatement,
    update: SendStatement,
}

#[derive(Clone)]
struct Database {
    stmts: Arc<Mutex<Statements>>,
}

fn setup_database() -> Result<Database> {
    let c = OpenOptions::new().create().read_write().open_in_memory()?;

    c.execute(
        r#"
         CREATE TABLE users (name TEXT PRIMARY KEY NOT NULL, age INTEGER);

         INSERT INTO users VALUES ('Alice', 60), ('Bob', 70), ('Charlie', 20);
         "#,
    )?;

    let select = c
        .prepare_with("SELECT age FROM users ORDER BY age")
        .persistent()
        .build()?;

    let update = c
        .prepare_with("UPDATE users SET age = age + ?")
        .persistent()
        .build()?;

    // SAFETY: We serialize all accesses to the statements behind a mutex.
    let inner = unsafe {
        Statements {
            select: select.into_send()?,
            update: update.into_send()?,
        }
    };

    Ok(Database {
        stmts: Arc::new(Mutex::new(inner)),
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let db = setup_database()?;

    let mut tasks = Vec::new();

    for _ in 0..10 {
        _ = task::spawn({
            let db = db.clone();

            async move {
                let mut stmts = db.stmts.lock_owned().await;

                let task = task::spawn_blocking(move || {
                    stmts.update.execute(2)?;
                    Ok::<_, anyhow::Error>(())
                });

                task.await?
            }
        });

        let t = task::spawn({
            let db = db.clone();

            async move {
                let mut stmts = db.stmts.lock_owned().await;

                let task = task::spawn_blocking(move || -> Result<Option<i64>> {
                    stmts.select.reset()?;
                    Ok(stmts.select.next::<i64>()?)
                });

                task.await?
            }
        });

        tasks.push(t);
    }

    for t in tasks {
        let first = t.await??;
        assert!(matches!(first, Some(20..=40)));
    }

    Ok(())
}
