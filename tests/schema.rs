use sqll::schema::{ColumnKind, IndexOrigin};
use sqll::{Connection, Migrations, Result, Schema};

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_users",
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT UNIQUE)",
    ),
    (
        "0002_posts",
        "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users (id), body TEXT)",
    ),
    (
        "0003_posts_index",
        "CREATE INDEX posts_user ON posts (user_id)",
    ),
    (
        "0004_users_age",
        r#"
        CREATE TABLE users_new (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT UNIQUE,
            age INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO users_new (id, name, email) SELECT id, name, email FROM users;
        DROP TABLE users;
        ALTER TABLE users_new RENAME TO users;
        "#,
    ),
    (
        "0005_posts_cascade",
        r#"
        CREATE TABLE posts_new (
            id INTEGER PRIMARY KEY,
            user_id INTEGER REFERENCES users (id) ON DELETE CASCADE,
            body TEXT
        );
        INSERT INTO posts_new SELECT * FROM posts;
        DROP TABLE posts;
        ALTER TABLE posts_new RENAME TO posts;
        CREATE INDEX posts_user ON posts (user_id);
        "#,
    ),
];

const SCHEMA: &str = r#"
CREATE TABLE posts (
    id INTEGER PRIMARY KEY,
    user_id INTEGER REFERENCES users (id) ON DELETE CASCADE,
    body TEXT
);

CREATE INDEX posts_user ON posts (user_id);

CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    age INTEGER NOT NULL DEFAULT 0
);
"#;

#[test]
fn migrations_match_schema() -> Result<()> {
    let c = Connection::open_in_memory()?;
    Migrations::new()
        .foreign_keys_off(true)
        .run(&c, MIGRATIONS.iter().copied())?;

    let migrated = Schema::describe(&c)?;
    let expected = Schema::from_sql(SCHEMA)?;

    let diff = migrated.diff(&expected);
    assert!(diff.is_empty(), "{diff}");
    assert_eq!(migrated, expected);

    // Through fresh_schema as well.
    let c = Connection::open_in_memory()?;
    Migrations::new()
        .fresh_schema(SCHEMA)
        .run(&c, MIGRATIONS.iter().copied())?;
    assert_eq!(Schema::describe(&c)?, expected);
    Ok(())
}

#[test]
fn missing_migration_is_reported() -> Result<()> {
    let c = Connection::open_in_memory()?;
    Migrations::new()
        .foreign_keys_off(true)
        .run(&c, MIGRATIONS[..4].iter().copied())?;

    let migrated = Schema::describe(&c)?;
    let expected = Schema::from_sql(SCHEMA)?;

    assert_eq!(
        migrated.diff(&expected).to_string(),
        "\
- foreign key posts: FOREIGN KEY (user_id) REFERENCES users (id)
+ foreign key posts: FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE"
    );

    Ok(())
}

#[test]
fn column_changes() -> Result<()> {
    let a = Schema::from_sql(
        "CREATE TABLE t (a INTEGER PRIMARY KEY, b TEXT, c TEXT DEFAULT 'x', d, e INTEGER)",
    )?;
    let b = Schema::from_sql(
        "CREATE TABLE t (a INTEGER, b BLOB NOT NULL, c TEXT, e INTEGER, d, f INTEGER GENERATED ALWAYS AS (e + 1))",
    )?;

    assert_eq!(
        a.diff(&b).to_string(),
        "\
+ column t.f INTEGER (generated virtual)
~ column t.a: primary key 1 -> 0
~ column t.b: type TEXT -> BLOB
~ column t.b: not null false -> true
~ column t.c: default 'x' -> <none>
~ table t: column order (a, b, c, d, e) -> (a, b, c, e, d)"
    );

    assert_eq!(
        b.diff(&a).to_string(),
        "\
- column t.f INTEGER (generated virtual)
~ column t.a: primary key 0 -> 1
~ column t.b: type BLOB -> TEXT
~ column t.b: not null true -> false
~ column t.c: default <none> -> 'x'
~ table t: column order (a, b, c, e, d) -> (a, b, c, d, e)"
    );

    let f = &b.tables[0].columns[5];
    assert_eq!(f.kind, ColumnKind::Virtual);
    Ok(())
}

#[test]
fn index_changes() -> Result<()> {
    let a = Schema::from_sql(
        r#"
        CREATE TABLE t (a TEXT, b TEXT, UNIQUE (a, b));
        CREATE INDEX t_a ON t (a);
        CREATE INDEX t_b ON t (b);
        "#,
    )?;

    let b = Schema::from_sql(
        r#"
        CREATE TABLE t (a TEXT, b TEXT, UNIQUE (b, a));
        CREATE UNIQUE INDEX t_a ON t (a);
        CREATE INDEX t_ab ON t (a, b) WHERE a IS NOT NULL;
        "#,
    )?;

    assert_eq!(
        a.diff(&b).to_string(),
        "\
~ index t: INDEX t_a (a) -> UNIQUE INDEX t_a (a)
- index t: INDEX t_b (b)
- index t: UNIQUE (a, b)
+ index t: INDEX t_ab (a, b) WHERE ...
+ index t: UNIQUE (b, a)"
    );

    let origins = b.tables[0]
        .indexes
        .iter()
        .map(|i| i.origin)
        .collect::<Vec<_>>();
    assert_eq!(
        origins,
        [
            IndexOrigin::Create,
            IndexOrigin::Create,
            IndexOrigin::Unique
        ]
    );
    Ok(())
}

#[test]
fn constraint_index_names_are_ignored() -> Result<()> {
    // The automatic indexes get different names depending on declaration
    // order, which is not a difference in the schema.
    let a = Schema::from_sql("CREATE TABLE t (a TEXT UNIQUE, b TEXT UNIQUE)")?;
    let b = Schema::from_sql("CREATE TABLE t (a TEXT, b TEXT, UNIQUE (b), UNIQUE (a))")?;
    assert_eq!(a, b);
    Ok(())
}

#[test]
fn foreign_keys_are_sorted() -> Result<()> {
    let a = Schema::from_sql(
        r#"
        CREATE TABLE p (id INTEGER PRIMARY KEY, x INTEGER, y INTEGER, UNIQUE (x, y));
        CREATE TABLE c (
            a INTEGER REFERENCES p (id),
            x INTEGER,
            y INTEGER,
            FOREIGN KEY (x, y) REFERENCES p (x, y)
        );
        "#,
    )?;

    let b = Schema::from_sql(
        r#"
        CREATE TABLE p (id INTEGER PRIMARY KEY, x INTEGER, y INTEGER, UNIQUE (x, y));
        CREATE TABLE c (
            a INTEGER,
            x INTEGER,
            y INTEGER,
            FOREIGN KEY (x, y) REFERENCES p (x, y),
            FOREIGN KEY (a) REFERENCES p (id)
        );
        "#,
    )?;

    assert_eq!(a, b);

    let fk = &a.table("c").unwrap().foreign_keys[1];
    assert_eq!(fk.columns, ["x", "y"]);
    assert_eq!(fk.references, [Some("x".into()), Some("y".into())]);
    Ok(())
}

#[test]
fn views_and_triggers() -> Result<()> {
    let a = Schema::from_sql(
        r#"
        CREATE TABLE t (a TEXT);
        CREATE TABLE u (a TEXT);
        CREATE VIEW v1 AS SELECT a FROM t;
        CREATE TRIGGER tr1 AFTER INSERT ON t BEGIN SELECT 1; END;
        CREATE TRIGGER tr2 AFTER INSERT ON t BEGIN SELECT 1; END;
        "#,
    )?;

    let b = Schema::from_sql(
        r#"
        CREATE TABLE t (a TEXT);
        CREATE TABLE u (a TEXT);
        CREATE VIEW v2 AS SELECT a FROM t;
        CREATE TRIGGER tr1 AFTER INSERT ON u BEGIN SELECT 1; END;
        CREATE TRIGGER tr3 AFTER INSERT ON t BEGIN SELECT 1; END;
        "#,
    )?;

    assert_eq!(
        a.diff(&b).to_string(),
        "\
- view v1
+ view v2
~ trigger tr1: table t -> u
- trigger tr2 ON t
+ trigger tr3 ON t"
    );
    Ok(())
}

#[test]
fn table_flags() -> Result<()> {
    let a = Schema::from_sql("CREATE TABLE t (a TEXT PRIMARY KEY)")?;
    let b = Schema::from_sql("CREATE TABLE t (a TEXT PRIMARY KEY) WITHOUT ROWID, STRICT")?;

    // A WITHOUT ROWID table implies NOT NULL on its primary key.
    assert_eq!(
        a.diff(&b).to_string(),
        "\
~ table t: without rowid false -> true
~ table t: strict false -> true
~ column t.a: not null false -> true"
    );
    Ok(())
}

#[test]
fn excluded_tables() -> Result<()> {
    let c = Connection::open_in_memory()?;

    c.execute(
        r#"
        CREATE TABLE t (a TEXT);
        CREATE TABLE history (id TEXT);
        CREATE INDEX history_id ON history (id);
        CREATE TRIGGER history_trigger AFTER INSERT ON history BEGIN SELECT 1; END;
        "#,
    )?;

    Migrations::new().run(&c, [("0001", "CREATE TABLE m (a TEXT)")])?;

    let all = Schema::describe_excluding(&c, &[])?;
    let names = all.tables.iter().map(|t| &t.name[..]).collect::<Vec<_>>();
    assert_eq!(names, ["history", "m", "migrations", "t"]);

    let schema = Schema::describe(&c)?;
    let names = schema
        .tables
        .iter()
        .map(|t| &t.name[..])
        .collect::<Vec<_>>();
    assert_eq!(names, ["history", "m", "t"]);

    let schema = Schema::describe_excluding(&c, &["HISTORY", "migrations"])?;
    assert_eq!(
        schema,
        Schema::from_sql("CREATE TABLE t (a TEXT); CREATE TABLE m (a TEXT)")?
    );
    assert!(schema.triggers.is_empty());
    Ok(())
}

#[test]
fn quoted_names() -> Result<()> {
    let schema = Schema::from_sql(
        r#"
        CREATE TABLE "odd ""name""" ("a b" TEXT);
        CREATE INDEX "odd index" ON "odd ""name""" ("a b");
        "#,
    )?;

    let table = &schema.tables[0];
    assert_eq!(table.name, "odd \"name\"");
    assert_eq!(table.columns[0].name, "a b");
    assert_eq!(table.indexes[0].name.as_deref(), Some("odd index"));
    assert_eq!(table.indexes[0].columns[0].name.as_deref(), Some("a b"));
    Ok(())
}
