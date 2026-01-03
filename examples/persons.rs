use sqll::{Connection, Prepare, Result, Row};

#[derive(Row)]
struct Person<'stmt> {
    id: i32,
    name: &'stmt str,
}

fn main() -> Result<()> {
    let conn = Connection::open_in_memory()?;

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS persons (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )
        "#,
    )?;

    let mut stmt = conn.prepare("INSERT INTO persons (name) VALUES (?1), (?2), (?3)")?;
    stmt.bind_value(1, "Steven")?;
    stmt.bind_value(2, "John")?;
    stmt.bind_value(3, "Alex")?;
    assert!(stmt.step()?.is_done());

    let mut stmt = conn.prepare_with("SELECT id, name FROM persons", Prepare::PERSISTENT)?;

    for _ in 0..10 {
        stmt.reset()?;

        println!("Found persons:");

        while let Some(p) = stmt.next::<Person<'_>>()? {
            println!("ID: {}, Name: {}", p.id, p.name);
        }
    }

    Ok(())
}
