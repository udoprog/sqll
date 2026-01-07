use std::io::{self, Write};
use std::time::Instant;

use sqll::{OpenOptions, Prepare, Row};

#[derive(Row)]
struct Person<'stmt> {
    id: i32,
    name: &'stmt str,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = OpenOptions::new()
        .create()
        .read_write()
        .no_mutex()
        .open_in_memory()?;

    c.execute(
        r#"
        CREATE TABLE IF NOT EXISTS persons (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )
        "#,
    )?;

    let mut stmt = c.prepare("INSERT INTO persons (name) VALUES (?), (?), (?)")?;
    stmt.execute(("Steven", "John", "Alex"))?;

    let mut stmt = c.prepare_with("SELECT id, name FROM persons", Prepare::PERSISTENT)?;

    let mut o = io::sink();

    let start = Instant::now();
    let mut c = 0;

    for _ in 0..100_000 {
        stmt.reset()?;

        writeln!(o, "Found persons:")?;

        while let Some(p) = stmt.next::<Person<'_>>()? {
            c += 1;
            writeln!(o, "ID: {}, Name: {}", p.id, p.name)?;
        }
    }

    println!("Elapsed: {:?}", start.elapsed());
    println!("Total persons found: {c}");
    Ok(())
}
