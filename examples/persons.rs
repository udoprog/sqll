use sqll::{Connection, FromRow, Prepare, Result};

#[derive(FromRow)]
struct Person<'stmt> {
    id: i32,
    name: &'stmt str,
}

fn main() -> Result<()> {
    let conn = Connection::open_memory()?;

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS persons (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )
        "#,
    )?;

    let mut stmt = conn.prepare("INSERT INTO persons (name) VALUES (?1), (?2), (?3)")?;
    stmt.bind(1, "Steven")?;
    stmt.bind(2, "John")?;
    stmt.bind(3, "Alex")?;
    stmt.execute()?;

    let mut stmt = conn.prepare_with("SELECT id, name FROM persons", Prepare::PERSISTENT)?;

    for _ in 0..10 {
        stmt.reset()?;

        println!("Found persons:");

        while let Some(row) = stmt.next()? {
            let p = row.as_row::<Person<'_>>()?;
            println!("ID: {}, Name: {}", p.id, p.name);
        }
    }

    Ok(())
}
