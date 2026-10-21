use sqll::{Code, Connection, Migrations, Result};

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_users", "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)"),
    (
        "0002_posts",
        "CREATE TABLE posts (user_id INTEGER REFERENCES users (id), body TEXT)",
    ),
    (
        "0003_seed",
        "INSERT INTO users VALUES (1, 'Alice'); INSERT INTO posts VALUES (1, 'Hello')",
    ),
];

fn applied(c: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = c.prepare(format!("SELECT id FROM {table} ORDER BY rowid"))?;
    stmt.iter::<String>().collect()
}

fn count(c: &Connection, table: &str) -> Result<i64> {
    let mut stmt = c.prepare(format!("SELECT COUNT(*) FROM {table}"))?;
    Ok(stmt.first::<i64>(())?.unwrap_or_default())
}

fn foreign_keys(c: &Connection) -> Result<i64> {
    let mut stmt = c.prepare("PRAGMA foreign_keys")?;
    Ok(stmt.first::<i64>(())?.unwrap_or_default())
}

#[test]
fn applies_in_order_and_skips_recorded() -> Result<()> {
    let c = Connection::open_in_memory()?;
    let m = Migrations::new();

    assert_eq!(m.run(&c, MIGRATIONS[..1].iter().copied())?, 1);
    assert_eq!(m.run(&c, MIGRATIONS.iter().copied())?, 2);
    assert_eq!(m.run(&c, MIGRATIONS.iter().copied())?, 0);

    assert_eq!(
        applied(&c, "migrations")?,
        ["0001_users", "0002_posts", "0003_seed"]
    );

    let mut stmt = c.prepare("SELECT applied_at FROM migrations")?;
    assert!(stmt.first::<i64>(())?.unwrap() > 1_600_000_000);
    Ok(())
}

#[test]
fn owned_migrations() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let migrations = vec![(
        String::from("0001_users"),
        String::from("CREATE TABLE users (name TEXT)"),
    )];

    assert_eq!(Migrations::new().run(&c, migrations)?, 1);
    Ok(())
}

#[test]
fn existing_table_with_text_applied_at() -> Result<()> {
    let c = Connection::open_in_memory()?;
    c.execute(
        r#"
        CREATE TABLE migrations (id TEXT PRIMARY KEY, applied_at TEXT NOT NULL);
        INSERT INTO migrations VALUES ('0001_users', '2026-01-01T00:00:00Z');
        CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
        "#,
    )?;

    assert_eq!(Migrations::new().run(&c, MIGRATIONS.iter().copied())?, 2);
    assert_eq!(count(&c, "users")?, 1);
    Ok(())
}

#[test]
fn failing_migration_is_rolled_back() -> Result<()> {
    let c = Connection::open_in_memory()?;
    let m = Migrations::new();

    let broken = [
        MIGRATIONS[0],
        ("0002_broken", "CREATE TABLE t (a); INSERT INTO missing VALUES (1)"),
    ];

    let error = m.run(&c, broken).unwrap_err();
    assert_eq!(error.code(), Code::ERROR);
    assert!(error.to_string().starts_with("migration 0002_broken: "), "{error}");

    assert_eq!(applied(&c, "migrations")?, ["0001_users"]);
    // The part of the migration that ran was rolled back.
    assert!(c.prepare("SELECT * FROM t").is_err());

    let fixed = [MIGRATIONS[0], ("0002_broken", "CREATE TABLE t (a)")];
    assert_eq!(m.run(&c, fixed)?, 1);
    Ok(())
}

#[test]
fn migration_can_drop_tables() -> Result<()> {
    let c = Connection::open_in_memory()?;
    let m = Migrations::new();

    m.run(&c, MIGRATIONS.iter().copied())?;

    let more = [
        ("0004_drop_posts", "DROP TABLE posts"),
        ("0005_drop_users", "DROP TABLE users"),
    ];

    assert_eq!(m.run(&c, MIGRATIONS.iter().copied().chain(more))?, 2);
    Ok(())
}

#[test]
fn custom_table_name_is_quoted() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let m = Migrations::new().table("schema \"history\"");
    assert_eq!(m.run(&c, MIGRATIONS.iter().copied())?, 3);
    assert_eq!(m.run(&c, MIGRATIONS.iter().copied())?, 0);
    assert_eq!(count(&c, "\"schema \"\"history\"\"\"")?, 3);
    Ok(())
}

#[test]
fn separate_connections() -> Result<()> {
    let dir = tempfile::tempdir().map_err(sqll::Error::custom)?;
    let path = dir.path().join("test.db");

    let a = Connection::open(&path)?;
    let b = Connection::open(&path)?;
    let m = Migrations::new();

    assert_eq!(m.run(&a, MIGRATIONS.iter().copied())?, 3);
    assert_eq!(m.run(&b, MIGRATIONS.iter().copied())?, 0);
    assert_eq!(count(&b, "users")?, 1);
    Ok(())
}

#[test]
fn fresh_schema_on_empty_database() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let m = Migrations::new().fresh_schema(
        r#"
        CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE posts (user_id INTEGER REFERENCES users (id), body TEXT);
        INSERT INTO users VALUES (1, 'Alice');
        INSERT INTO posts VALUES (1, 'Hello');
        "#,
    );

    assert_eq!(m.run(&c, MIGRATIONS.iter().copied())?, 3);
    assert_eq!(
        applied(&c, "migrations")?,
        ["0001_users", "0002_posts", "0003_seed"]
    );
    assert_eq!(count(&c, "posts")?, 1);

    // Not fresh any more, so new migrations run.
    let more = [("0004_drop_posts", "DROP TABLE posts")];
    assert_eq!(m.run(&c, MIGRATIONS.iter().copied().chain(more))?, 1);
    Ok(())
}

#[test]
fn fresh_schema_skipped_on_existing_database() -> Result<()> {
    // A database created before migrations were tracked.
    let c = Connection::open_in_memory()?;
    c.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")?;

    let m = Migrations::new().fresh_schema("CREATE TABLE should_not_exist (a)");

    let error = m.run(&c, MIGRATIONS.iter().copied()).unwrap_err();
    assert!(error.to_string().starts_with("migration 0001_users: "), "{error}");
    assert!(c.prepare("SELECT * FROM should_not_exist").is_err());
    Ok(())
}

#[test]
fn failing_fresh_schema_is_rolled_back() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let m = Migrations::new().fresh_schema("CREATE TABLE a (x); CREATE TABLE a (x)");
    let error = m.run(&c, MIGRATIONS.iter().copied()).unwrap_err();
    assert!(error.to_string().starts_with("fresh schema: "), "{error}");

    assert!(c.prepare("SELECT * FROM a").is_err());
    assert!(c.prepare("SELECT * FROM migrations").is_err());

    assert_eq!(Migrations::new().run(&c, MIGRATIONS.iter().copied())?, 3);
    Ok(())
}

#[test]
fn foreign_keys_off_restores_on_error() -> Result<()> {
    let c = Connection::open_in_memory()?;
    c.execute("PRAGMA foreign_keys = ON")?;

    let m = Migrations::new().foreign_keys_off(true);
    m.run(&c, MIGRATIONS.iter().copied())?;
    assert_eq!(foreign_keys(&c)?, 1);

    let error = m
        .run(&c, [("0004_orphan", "INSERT INTO posts VALUES (2, 'Orphan')")])
        .unwrap_err();

    assert_eq!(error.code(), Code::CONSTRAINT_FOREIGNKEY);
    assert!(
        error.to_string().starts_with("migration 0004_orphan: "),
        "{error}"
    );
    assert_eq!(count(&c, "posts")?, 1);
    assert_eq!(foreign_keys(&c)?, 1);
    Ok(())
}

#[test]
fn foreign_keys_off_leaves_them_off() -> Result<()> {
    let c = Connection::open_in_memory()?;
    c.execute("PRAGMA foreign_keys = OFF")?;

    let m = Migrations::new().foreign_keys_off(true);
    m.run(&c, MIGRATIONS.iter().copied())?;
    assert_eq!(foreign_keys(&c)?, 0);
    Ok(())
}

#[test]
fn foreign_keys_off_checks_fresh_schema() -> Result<()> {
    let c = Connection::open_in_memory()?;
    c.execute("PRAGMA foreign_keys = ON")?;

    let m = Migrations::new().foreign_keys_off(true).fresh_schema(
        r#"
        CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
        CREATE TABLE posts (user_id INTEGER REFERENCES users (id), body TEXT);
        INSERT INTO posts VALUES (1, 'Orphan');
        "#,
    );

    let error = m.run(&c, MIGRATIONS.iter().copied()).unwrap_err();
    assert_eq!(error.code(), Code::CONSTRAINT_FOREIGNKEY);
    assert!(c.prepare("SELECT * FROM posts").is_err());
    assert_eq!(foreign_keys(&c)?, 1);
    Ok(())
}
