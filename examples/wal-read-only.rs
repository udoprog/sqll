use std::io::{self, Write};

use std::time::Instant;

use sqll::{OpenOptions, Row};

#[derive(Row)]
struct Person<'stmt> {
    id: i32,
    name: &'stmt str,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let c = OpenOptions::new()
        .create()
        .read_write()
        .open("wal-people.db")?;

    c.execute(
        r#"
        pragma journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS persons (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL
        )
        "#,
    )?;

    let mut stmt = c
        .prepare_with("SELECT id, name FROM persons")
        .persistent()
        .build()?;

    // WAL only supports one writer, if you run this in multiple processes this
    // checks at what point the write operation fails.
    loop {
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
    }
}
