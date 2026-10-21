use sqll::OpenOptions;

use anyhow::Context;

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

    // WAL only supports one writer, if you run this in multiple processes this
    // checks at what point the write operation fails.
    loop {
        c.execute("BEGIN").context("Beginning transaction")?;

        c.execute("DELETE FROM persons;")
            .context("Deleting all records")?;

        c.execute("INSERT INTO persons (name) VALUES ('Alice')")
            .context("Inserting Alice")?;

        c.execute("INSERT INTO persons (name) VALUES ('Bob')")
            .context("Inserting Bob")?;

        c.execute("INSERT INTO persons (name) VALUES ('Charlie')")
            .context("Inserting Charlie")?;

        c.execute("COMMIT").context("Committing transaction")?;
    }
}
