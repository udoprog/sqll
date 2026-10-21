//! Describe the schema of a database and compare it to another.
//!
//! See [`Schema`] for how to use it.

use core::fmt::{self, Write};

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{Connection, Result};

/// The name of the table [`Migrations`] records applied migrations in by
/// default, which [`Schema::describe`] leaves out.
///
/// [`Migrations`]: crate::Migrations
const MIGRATIONS_TABLE: &str = "migrations";

/// A description of the schema of a database, which can be compared to
/// another.
///
/// It is read from the `main` database of a [`Connection`] with
/// [`describe`], and covers its tables with their columns, indexes and foreign
/// keys, and the names of its views and triggers. Everything is held in a
/// fixed order, so two descriptions of databases with the same schema are
/// equal no matter in which order things were created, and [`diff`] lists
/// what differs between two of them.
///
/// This is meant for checking that a database built by running migrations
/// matches the schema it is meant to have, such as one built by running a
/// reference schema on an in-memory database with [`from_sql`].
///
/// SQLite internal tables, whose names start with `sqlite_`, the shadow tables
/// of virtual tables and the `migrations` table used by [`Migrations`] are
/// left out. Use [`describe_excluding`] to leave out other tables.
///
/// Only what SQLite reports through its schema pragmas is described. Notably,
/// the expressions of `CHECK` constraints, generated columns, partial index
/// conditions, views and triggers are not compared. Declared column types and
/// default values are compared as written, so `TEXT` and `text` differ.
///
/// [`describe`]: Self::describe
/// [`describe_excluding`]: Self::describe_excluding
/// [`diff`]: Self::diff
/// [`from_sql`]: Self::from_sql
/// [`Migrations`]: crate::Migrations
///
/// # Examples
///
/// Checking that a set of migrations produces the expected schema:
///
/// ```
/// use sqll::{Connection, Migrations, Schema};
///
/// const MIGRATIONS: &[(&str, &str)] = &[
///     ("0001_users", "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"),
///     ("0002_email", "ALTER TABLE users ADD COLUMN email TEXT"),
///     ("0003_email_index", "CREATE UNIQUE INDEX users_email ON users (email)"),
/// ];
///
/// const SCHEMA: &str = r#"
///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT);
///     CREATE UNIQUE INDEX users_email ON users (email);
/// "#;
///
/// let c = Connection::open_in_memory()?;
/// Migrations::new().run(&c, MIGRATIONS.iter().copied())?;
///
/// let migrated = Schema::describe(&c)?;
/// let expected = Schema::from_sql(SCHEMA)?;
///
/// let diff = migrated.diff(&expected);
/// assert!(diff.is_empty(), "schema differs:\n{diff}");
/// assert_eq!(migrated, expected);
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// A difference is listed readably, with `-` for what is only in the first
/// schema, `+` for what is only in the second and `~` for what changed:
///
/// ```
/// use sqll::Schema;
///
/// let a = Schema::from_sql("CREATE TABLE users (name TEXT, age INTEGER)")?;
/// let b = Schema::from_sql("CREATE TABLE users (name TEXT NOT NULL, email TEXT)")?;
///
/// assert_eq!(a.diff(&b).to_string(), "\
/// - column users.age INTEGER
/// + column users.email TEXT
/// ~ column users.name: not null false -> true");
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Schema {
    /// The tables, sorted by name.
    pub tables: Vec<Table>,
    /// The names of the views, sorted.
    pub views: Vec<String>,
    /// The triggers, sorted by name.
    pub triggers: Vec<Trigger>,
}

impl Schema {
    /// Describe the schema of the `main` database of `c`, leaving out the
    /// `migrations` table used by [`Migrations`].
    ///
    /// [`Migrations`]: crate::Migrations
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Schema};
    ///
    /// let c = Connection::open_in_memory()?;
    /// c.execute(r#"
    ///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL DEFAULT 'anonymous');
    ///     CREATE INDEX users_name ON users (name);
    /// "#)?;
    ///
    /// let schema = Schema::describe(&c)?;
    /// let users = schema.table("users").expect("users table");
    ///
    /// assert_eq!(users.columns.len(), 2);
    /// assert_eq!(users.columns[0].primary_key, 1);
    /// assert_eq!(users.columns[1].ty, "TEXT");
    /// assert!(users.columns[1].not_null);
    /// assert_eq!(users.columns[1].default.as_deref(), Some("'anonymous'"));
    /// assert_eq!(users.indexes[0].name.as_deref(), Some("users_name"));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn describe(c: &Connection) -> Result<Self> {
        Self::describe_excluding(c, &[MIGRATIONS_TABLE])
    }

    /// Describe the schema of the `main` database of `c`, leaving out the
    /// tables named in `exclude`, along with their indexes and triggers.
    ///
    /// Unlike [`describe`], this does not leave out the `migrations` table
    /// unless it is named, so pass the table given to [`Migrations::table`]
    /// when using another one. Names are matched ignoring ASCII case, the same
    /// way SQLite matches them.
    ///
    /// [`describe`]: Self::describe
    /// [`Migrations::table`]: crate::Migrations::table
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Migrations, Schema};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// Migrations::new()
    ///     .table("schema_history")
    ///     .run(&c, [("0001_users", "CREATE TABLE users (name TEXT)")])?;
    ///
    /// let schema = Schema::describe_excluding(&c, &["schema_history"])?;
    /// assert_eq!(schema, Schema::from_sql("CREATE TABLE users (name TEXT)")?);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn describe_excluding(c: &Connection, exclude: &[&str]) -> Result<Self> {
        let excluded = |name: &str| exclude.iter().any(|e| e.eq_ignore_ascii_case(name));

        let mut tables = Vec::new();
        let mut views = Vec::new();

        let mut list = c.prepare(
            "SELECT name, type, wr, strict FROM pragma_table_list \
             WHERE schema = 'main' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
        )?;

        for row in list.iter::<(String, String, bool, bool)>() {
            let (name, ty, without_rowid, strict) = row?;

            match ty.as_str() {
                "view" => views.push(name),
                "table" | "virtual" if !excluded(&name) => tables.push(Table {
                    name,
                    columns: Vec::new(),
                    indexes: Vec::new(),
                    foreign_keys: Vec::new(),
                    without_rowid,
                    strict,
                    virtual_table: ty == "virtual",
                }),
                _ => {}
            }
        }

        let mut columns = c.prepare(
            "SELECT name, type, \"notnull\", dflt_value, pk, hidden \
             FROM pragma_table_xinfo(?, 'main') ORDER BY cid",
        )?;

        let mut index_list = c.prepare(
            "SELECT name, \"unique\", origin, partial FROM pragma_index_list(?, 'main')",
        )?;

        let mut index_columns = c.prepare(
            "SELECT name, \"desc\", coll FROM pragma_index_xinfo(?, 'main') \
             WHERE key ORDER BY seqno",
        )?;

        let mut foreign_keys = c.prepare(
            "SELECT id, \"table\", \"from\", \"to\", on_update, on_delete \
             FROM pragma_foreign_key_list(?, 'main') ORDER BY id, seq",
        )?;

        for table in &mut tables {
            columns.reset()?;
            columns.bind(table.name.as_str())?;

            for row in columns.iter::<(String, String, bool, Option<String>, u32, i64)>() {
                let (name, ty, not_null, default, primary_key, hidden) = row?;

                table.columns.push(Column {
                    name,
                    ty,
                    not_null,
                    default,
                    primary_key,
                    kind: match hidden {
                        1 => ColumnKind::Hidden,
                        2 => ColumnKind::Virtual,
                        3 => ColumnKind::Stored,
                        _ => ColumnKind::Normal,
                    },
                });
            }

            index_list.reset()?;
            index_list.bind(table.name.as_str())?;

            let mut indexes = Vec::new();

            for row in index_list.iter::<(String, bool, String, bool)>() {
                indexes.push(row?);
            }

            for (name, unique, origin, partial) in indexes {
                index_columns.reset()?;
                index_columns.bind(name.as_str())?;

                let mut columns = Vec::new();

                for row in index_columns.iter::<(Option<String>, bool, String)>() {
                    let (name, descending, collation) = row?;

                    columns.push(IndexColumn {
                        name,
                        descending,
                        collation,
                    });
                }

                let origin = match origin.as_str() {
                    "u" => IndexOrigin::Unique,
                    "pk" => IndexOrigin::PrimaryKey,
                    _ => IndexOrigin::Create,
                };

                table.indexes.push(Index {
                    // The names SQLite gives to the indexes it creates for
                    // constraints depend on the order the constraints are
                    // declared in, so they are left out.
                    name: (origin == IndexOrigin::Create).then_some(name),
                    origin,
                    unique,
                    partial,
                    columns,
                });
            }

            table.indexes.sort_by(|a, b| {
                (a.origin, &a.name, &a.columns).cmp(&(b.origin, &b.name, &b.columns))
            });

            foreign_keys.reset()?;
            foreign_keys.bind(table.name.as_str())?;

            let mut last = None;

            for row in foreign_keys.iter::<(i64, String, String, Option<String>, String, String)>()
            {
                let (id, parent, from, to, on_update, on_delete) = row?;

                if last != Some(id) {
                    last = Some(id);

                    table.foreign_keys.push(ForeignKey {
                        columns: Vec::new(),
                        table: parent,
                        references: Vec::new(),
                        on_update,
                        on_delete,
                    });
                }

                if let Some(fk) = table.foreign_keys.last_mut() {
                    fk.columns.push(from);
                    fk.references.push(to);
                }
            }

            // Foreign keys are numbered in the reverse order of their
            // declaration, so sort them by what they are.
            table.foreign_keys.sort();
        }

        tables.sort_by(|a, b| a.name.cmp(&b.name));
        views.sort();

        let mut triggers = Vec::new();

        let mut stmt = c.prepare(
            "SELECT name, tbl_name FROM main.sqlite_master WHERE type = 'trigger' ORDER BY name",
        )?;

        for row in stmt.iter::<(String, String)>() {
            let (name, table) = row?;

            if !excluded(&table) {
                triggers.push(Trigger { name, table });
            }
        }

        Ok(Self {
            tables,
            views,
            triggers,
        })
    }

    /// Describe the schema produced by running `sql` on a new in-memory
    /// database, as with [`describe`].
    ///
    /// This is a convenient way to describe a reference schema to compare a
    /// migrated database to.
    ///
    /// [`describe`]: Self::describe
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Schema;
    ///
    /// let schema = Schema::from_sql(r#"
    ///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
    ///     CREATE TABLE posts (user_id INTEGER REFERENCES users (id) ON DELETE CASCADE);
    ///     CREATE VIEW names AS SELECT name FROM users;
    /// "#)?;
    ///
    /// assert_eq!(schema.tables.len(), 2);
    /// assert_eq!(schema.views, ["names"]);
    ///
    /// let posts = schema.table("posts").expect("posts table");
    /// assert_eq!(posts.foreign_keys[0].table, "users");
    /// assert_eq!(posts.foreign_keys[0].on_delete, "CASCADE");
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn from_sql(sql: &str) -> Result<Self> {
        let c = Connection::open_in_memory()?;
        c.execute(sql)?;
        Self::describe(&c)
    }

    /// Get the table named `name`, matched ignoring ASCII case.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Schema;
    ///
    /// let schema = Schema::from_sql("CREATE TABLE users (name TEXT)")?;
    ///
    /// assert!(schema.table("users").is_some());
    /// assert!(schema.table("USERS").is_some());
    /// assert!(schema.table("posts").is_none());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// List what differs between this schema and `other`.
    ///
    /// The result reads as the changes which turn this schema into `other`:
    /// what is only in this schema is marked with `-`, what is only in `other`
    /// with `+`, and what changed with `~`. It is empty exactly when the two
    /// schemas are equal.
    ///
    /// Tables, columns, named indexes, views and triggers are matched by name,
    /// while the indexes SQLite creates for `UNIQUE` and `PRIMARY KEY`
    /// constraints and foreign keys are matched by what they are. Column order
    /// matters, since `SELECT *` and inserts without column names depend on
    /// it.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Schema;
    ///
    /// let a = Schema::from_sql(r#"
    ///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
    ///     CREATE TABLE legacy (value BLOB);
    /// "#)?;
    ///
    /// let b = Schema::from_sql(r#"
    ///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT UNIQUE) STRICT;
    ///     CREATE TABLE posts (user_id INTEGER REFERENCES users (id));
    ///     CREATE INDEX posts_user ON posts (user_id);
    /// "#)?;
    ///
    /// let diff = a.diff(&b);
    /// assert!(!diff.is_empty());
    ///
    /// assert_eq!(diff.to_string(), "\
    /// - table legacy
    /// + table posts
    /// ~ table users: strict false -> true
    /// + index users: UNIQUE (name)");
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn diff(&self, other: &Schema) -> Diff {
        let mut diff = Diff { lines: Vec::new() };

        for a in &self.tables {
            if other.table(&a.name).is_none() {
                diff.push(format_args!("- table {}", a.name));
            }
        }

        for b in &other.tables {
            if self.table(&b.name).is_none() {
                diff.push(format_args!("+ table {}", b.name));
            }
        }

        for a in &self.tables {
            if let Some(b) = other.table(&a.name) {
                diff_table(&mut diff, a, b);
            }
        }

        for a in &self.views {
            if !other.views.iter().any(|b| b.eq_ignore_ascii_case(a)) {
                diff.push(format_args!("- view {a}"));
            }
        }

        for b in &other.views {
            if !self.views.iter().any(|a| a.eq_ignore_ascii_case(b)) {
                diff.push(format_args!("+ view {b}"));
            }
        }

        for a in &self.triggers {
            match other
                .triggers
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(&a.name))
            {
                None => diff.push(format_args!("- trigger {} ON {}", a.name, a.table)),
                Some(b) if a.table != b.table => diff.push(format_args!(
                    "~ trigger {}: table {} -> {}",
                    a.name, a.table, b.table
                )),
                Some(_) => {}
            }
        }

        for b in &other.triggers {
            if !self
                .triggers
                .iter()
                .any(|a| a.name.eq_ignore_ascii_case(&b.name))
            {
                diff.push(format_args!("+ trigger {} ON {}", b.name, b.table));
            }
        }

        diff
    }
}

fn diff_table(diff: &mut Diff, a: &Table, b: &Table) {
    let t = &a.name;

    for (what, x, y) in [
        ("without rowid", a.without_rowid, b.without_rowid),
        ("strict", a.strict, b.strict),
        ("virtual", a.virtual_table, b.virtual_table),
    ] {
        if x != y {
            diff.push(format_args!("~ table {t}: {what} {x} -> {y}"));
        }
    }

    let find = |columns: &'_ [Column], name: &str| -> Option<usize> {
        columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
    };

    for x in &a.columns {
        if find(&b.columns, &x.name).is_none() {
            diff.push(format_args!("- column {t}.{x}"));
        }
    }

    for y in &b.columns {
        if find(&a.columns, &y.name).is_none() {
            diff.push(format_args!("+ column {t}.{y}"));
        }
    }

    for x in &a.columns {
        let Some(y) = find(&b.columns, &x.name).map(|i| &b.columns[i]) else {
            continue;
        };

        let c = &x.name;

        if x.ty != y.ty {
            diff.push(format_args!(
                "~ column {t}.{c}: type {} -> {}",
                OrNone(&x.ty),
                OrNone(&y.ty)
            ));
        }

        if x.not_null != y.not_null {
            diff.push(format_args!(
                "~ column {t}.{c}: not null {} -> {}",
                x.not_null, y.not_null
            ));
        }

        if x.default != y.default {
            diff.push(format_args!(
                "~ column {t}.{c}: default {} -> {}",
                OrNone(x.default.as_deref().unwrap_or_default()),
                OrNone(y.default.as_deref().unwrap_or_default())
            ));
        }

        if x.primary_key != y.primary_key {
            diff.push(format_args!(
                "~ column {t}.{c}: primary key {} -> {}",
                x.primary_key, y.primary_key
            ));
        }

        if x.kind != y.kind {
            diff.push(format_args!(
                "~ column {t}.{c}: kind {} -> {}",
                x.kind.as_str(),
                y.kind.as_str()
            ));
        }
    }

    // Compare the order of the columns which are in both tables.
    let common = |from: &[Column], to: &[Column]| -> Vec<String> {
        from.iter()
            .filter(|c| find(to, &c.name).is_some())
            .map(|c| c.name.to_ascii_lowercase())
            .collect()
    };

    let order_a = common(&a.columns, &b.columns);
    let order_b = common(&b.columns, &a.columns);

    if order_a != order_b {
        diff.push(format_args!(
            "~ table {t}: column order ({}) -> ({})",
            Names(&a.columns, &b.columns),
            Names(&b.columns, &a.columns),
        ));
    }

    let named = |indexes: &'_ [Index], name: &str| -> Option<usize> {
        indexes.iter().position(|i| {
            i.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
    };

    for x in &a.indexes {
        match &x.name {
            Some(name) => match named(&b.indexes, name) {
                None => diff.push(format_args!("- index {t}: {x}")),
                Some(i) if b.indexes[i] != *x => {
                    diff.push(format_args!("~ index {t}: {x} -> {}", b.indexes[i]))
                }
                Some(_) => {}
            },
            None if !b.indexes.contains(x) => diff.push(format_args!("- index {t}: {x}")),
            None => {}
        }
    }

    for y in &b.indexes {
        let missing = match &y.name {
            Some(name) => named(&a.indexes, name).is_none(),
            None => !a.indexes.contains(y),
        };

        if missing {
            diff.push(format_args!("+ index {t}: {y}"));
        }
    }

    for x in &a.foreign_keys {
        if !b.foreign_keys.contains(x) {
            diff.push(format_args!("- foreign key {t}: {x}"));
        }
    }

    for y in &b.foreign_keys {
        if !a.foreign_keys.contains(y) {
            diff.push(format_args!("+ foreign key {t}: {y}"));
        }
    }
}

/// A table in a [`Schema`].
///
/// # Examples
///
/// ```
/// use sqll::Schema;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE tags (name TEXT PRIMARY KEY, color TEXT) WITHOUT ROWID, STRICT;
/// "#)?;
///
/// let tags = &schema.tables[0];
/// assert_eq!(tags.name, "tags");
/// assert_eq!(tags.columns.len(), 2);
/// assert!(tags.without_rowid);
/// assert!(tags.strict);
/// assert!(!tags.virtual_table);
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Table {
    /// The name of the table.
    pub name: String,
    /// The columns of the table, in the order they are declared in.
    pub columns: Vec<Column>,
    /// The indexes on the table, including those SQLite creates for `UNIQUE`
    /// and `PRIMARY KEY` constraints, in a fixed order.
    pub indexes: Vec<Index>,
    /// The foreign keys of the table, sorted.
    pub foreign_keys: Vec<ForeignKey>,
    /// If the table is declared `WITHOUT ROWID`.
    pub without_rowid: bool,
    /// If the table is declared `STRICT`.
    pub strict: bool,
    /// If the table is a virtual table.
    pub virtual_table: bool,
}

/// A column of a [`Table`].
///
/// Its [`Display`] implementation shows its name followed by its declared
/// type and constraints.
///
/// [`Display`]: fmt::Display
///
/// # Examples
///
/// ```
/// use sqll::Schema;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL DEFAULT 'anonymous', age)
/// "#)?;
///
/// let columns = &schema.tables[0].columns;
///
/// assert_eq!(columns[0].name, "id");
/// assert_eq!(columns[0].primary_key, 1);
/// assert_eq!(columns[1].ty, "TEXT");
/// assert!(columns[1].not_null);
/// assert_eq!(columns[1].default.as_deref(), Some("'anonymous'"));
/// assert_eq!(columns[2].ty, "");
///
/// assert_eq!(columns[1].to_string(), "name TEXT NOT NULL DEFAULT 'anonymous'");
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Column {
    /// The name of the column.
    pub name: String,
    /// The declared type of the column as written, which is empty if it has
    /// none.
    pub ty: String,
    /// If the column is declared `NOT NULL`.
    pub not_null: bool,
    /// The default value of the column as written, if it has one.
    pub default: Option<String>,
    /// The 1-based position of the column in the primary key of the table, or
    /// 0 if it is not part of it.
    pub primary_key: u32,
    /// What kind of column it is.
    pub kind: ColumnKind,
}

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;

        if !self.ty.is_empty() {
            write!(f, " {}", self.ty)?;
        }

        if self.primary_key != 0 {
            write!(f, " PRIMARY KEY")?;

            if self.primary_key > 1 {
                write!(f, " ({})", self.primary_key)?;
            }
        }

        if self.not_null {
            write!(f, " NOT NULL")?;
        }

        if let Some(default) = &self.default {
            write!(f, " DEFAULT {default}")?;
        }

        match self.kind {
            ColumnKind::Normal => {}
            kind => write!(f, " ({})", kind.as_str())?,
        }

        Ok(())
    }
}

/// The kind of a [`Column`].
///
/// # Examples
///
/// ```
/// use sqll::Schema;
/// use sqll::schema::ColumnKind;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE t (
///         a INTEGER,
///         b INTEGER GENERATED ALWAYS AS (a * 2) VIRTUAL,
///         c INTEGER GENERATED ALWAYS AS (a * 3) STORED
///     )
/// "#)?;
///
/// let columns = &schema.tables[0].columns;
/// assert_eq!(columns[0].kind, ColumnKind::Normal);
/// assert_eq!(columns[1].kind, ColumnKind::Virtual);
/// assert_eq!(columns[2].kind, ColumnKind::Stored);
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ColumnKind {
    /// An ordinary column.
    Normal,
    /// A hidden column of a virtual table.
    Hidden,
    /// A virtual generated column.
    Virtual,
    /// A stored generated column.
    Stored,
}

impl ColumnKind {
    fn as_str(self) -> &'static str {
        match self {
            ColumnKind::Normal => "normal",
            ColumnKind::Hidden => "hidden",
            ColumnKind::Virtual => "generated virtual",
            ColumnKind::Stored => "generated stored",
        }
    }
}

/// An index on a [`Table`].
///
/// Its [`Display`] implementation shows it in a form close to how it is
/// declared.
///
/// [`Display`]: fmt::Display
///
/// # Examples
///
/// ```
/// use sqll::Schema;
/// use sqll::schema::IndexOrigin;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE users (name TEXT, email TEXT UNIQUE);
///     CREATE INDEX users_name ON users (name COLLATE NOCASE DESC) WHERE name IS NOT NULL;
/// "#)?;
///
/// let indexes = &schema.tables[0].indexes;
///
/// assert_eq!(indexes[0].name.as_deref(), Some("users_name"));
/// assert_eq!(indexes[0].origin, IndexOrigin::Create);
/// assert!(indexes[0].partial);
/// assert!(!indexes[0].unique);
/// assert_eq!(indexes[0].to_string(), "INDEX users_name (name COLLATE NOCASE DESC) WHERE ...");
///
/// // The index created for the UNIQUE constraint.
/// assert_eq!(indexes[1].name, None);
/// assert_eq!(indexes[1].origin, IndexOrigin::Unique);
/// assert!(indexes[1].unique);
/// assert_eq!(indexes[1].to_string(), "UNIQUE (email)");
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Index {
    /// The name of the index, or `None` for an index SQLite creates for a
    /// `UNIQUE` or `PRIMARY KEY` constraint, since its name depends on the
    /// order constraints are declared in.
    pub name: Option<String>,
    /// How the index was created.
    pub origin: IndexOrigin,
    /// If the index is unique.
    pub unique: bool,
    /// If the index is partial, that is it has a `WHERE` clause.
    pub partial: bool,
    /// The columns the index is on, in order.
    pub columns: Vec<IndexColumn>,
}

impl fmt::Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.origin {
            IndexOrigin::Create => {
                if self.unique {
                    f.write_str("UNIQUE ")?;
                }

                f.write_str("INDEX")?;

                if let Some(name) = &self.name {
                    write!(f, " {name}")?;
                }
            }
            IndexOrigin::Unique => f.write_str("UNIQUE")?,
            IndexOrigin::PrimaryKey => f.write_str("PRIMARY KEY")?,
        }

        f.write_str(" (")?;

        for (n, column) in self.columns.iter().enumerate() {
            if n > 0 {
                f.write_str(", ")?;
            }

            column.fmt(f)?;
        }

        f.write_char(')')?;

        if self.partial {
            f.write_str(" WHERE ...")?;
        }

        Ok(())
    }
}

/// How an [`Index`] was created.
///
/// # Examples
///
/// ```
/// use sqll::Schema;
/// use sqll::schema::IndexOrigin;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE tags (name TEXT PRIMARY KEY, slug TEXT UNIQUE);
///     CREATE INDEX tags_slug ON tags (slug DESC);
/// "#)?;
///
/// let origins = schema.tables[0]
///     .indexes
///     .iter()
///     .map(|i| i.origin)
///     .collect::<Vec<_>>();
///
/// assert_eq!(origins, [IndexOrigin::Create, IndexOrigin::Unique, IndexOrigin::PrimaryKey]);
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum IndexOrigin {
    /// Created by a `CREATE INDEX` statement.
    Create,
    /// Created for a `UNIQUE` constraint.
    Unique,
    /// Created for a `PRIMARY KEY` constraint.
    PrimaryKey,
}

/// A column of an [`Index`].
///
/// # Examples
///
/// ```
/// use sqll::Schema;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE users (name TEXT);
///     CREATE INDEX users_name ON users (name COLLATE NOCASE DESC, length(name));
/// "#)?;
///
/// let columns = &schema.tables[0].indexes[0].columns;
///
/// assert_eq!(columns[0].name.as_deref(), Some("name"));
/// assert!(columns[0].descending);
/// assert_eq!(columns[0].collation, "NOCASE");
///
/// // An expression has no name.
/// assert_eq!(columns[1].name, None);
/// assert_eq!(columns[1].collation, "BINARY");
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct IndexColumn {
    /// The name of the column, or `None` if it is an expression.
    pub name: Option<String>,
    /// If the column is sorted in descending order.
    pub descending: bool,
    /// The name of the collation of the column.
    pub collation: String,
}

impl fmt::Display for IndexColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => f.write_str(name)?,
            None => f.write_str("<expression>")?,
        }

        if !self.collation.eq_ignore_ascii_case("BINARY") {
            write!(f, " COLLATE {}", self.collation)?;
        }

        if self.descending {
            f.write_str(" DESC")?;
        }

        Ok(())
    }
}

/// A foreign key of a [`Table`].
///
/// Its [`Display`] implementation shows it as it would be declared as a table
/// constraint, leaving out actions which are `NO ACTION`.
///
/// [`Display`]: fmt::Display
///
/// # Examples
///
/// ```
/// use sqll::Schema;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE users (id INTEGER PRIMARY KEY);
///     CREATE TABLE posts (
///         user_id INTEGER REFERENCES users ON DELETE CASCADE,
///         editor_id INTEGER REFERENCES users (id)
///     );
/// "#)?;
///
/// let posts = schema.table("posts").expect("posts table");
/// let fks = &posts.foreign_keys;
///
/// assert_eq!(fks[0].columns, ["editor_id"]);
/// assert_eq!(fks[0].table, "users");
/// assert_eq!(fks[0].references, [Some(String::from("id"))]);
/// assert_eq!(fks[0].to_string(), "FOREIGN KEY (editor_id) REFERENCES users (id)");
///
/// // Referring to the primary key implicitly.
/// assert_eq!(fks[1].references, [None]);
/// assert_eq!(fks[1].on_delete, "CASCADE");
/// assert_eq!(fks[1].to_string(), "FOREIGN KEY (user_id) REFERENCES users ON DELETE CASCADE");
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ForeignKey {
    /// The columns in the table which refer to the parent table.
    pub columns: Vec<String>,
    /// The name of the parent table.
    pub table: String,
    /// The columns in the parent table referred to, one for each of
    /// [`columns`], each of which is `None` if the foreign key refers to the
    /// primary key of the parent table without naming its columns.
    ///
    /// [`columns`]: Self::columns
    pub references: Vec<Option<String>>,
    /// The `ON UPDATE` action, such as `NO ACTION` or `CASCADE`.
    pub on_update: String,
    /// The `ON DELETE` action, such as `NO ACTION` or `CASCADE`.
    pub on_delete: String,
}

impl fmt::Display for ForeignKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FOREIGN KEY ({}) REFERENCES {}",
            self.columns.join(", "),
            self.table
        )?;

        if self.references.iter().any(Option::is_some) {
            f.write_str(" (")?;

            for (n, column) in self.references.iter().enumerate() {
                if n > 0 {
                    f.write_str(", ")?;
                }

                f.write_str(column.as_deref().unwrap_or("<primary key>"))?;
            }

            f.write_char(')')?;
        }

        if !self.on_update.eq_ignore_ascii_case("NO ACTION") {
            write!(f, " ON UPDATE {}", self.on_update)?;
        }

        if !self.on_delete.eq_ignore_ascii_case("NO ACTION") {
            write!(f, " ON DELETE {}", self.on_delete)?;
        }

        Ok(())
    }
}

/// A trigger in a [`Schema`].
///
/// # Examples
///
/// ```
/// use sqll::Schema;
///
/// let schema = Schema::from_sql(r#"
///     CREATE TABLE users (name TEXT);
///     CREATE TABLE log (message TEXT);
///     CREATE TRIGGER users_log AFTER INSERT ON users BEGIN
///         INSERT INTO log VALUES ('added ' || NEW.name);
///     END;
/// "#)?;
///
/// assert_eq!(schema.triggers[0].name, "users_log");
/// assert_eq!(schema.triggers[0].table, "users");
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct Trigger {
    /// The name of the trigger.
    pub name: String,
    /// The name of the table or view the trigger is on.
    pub table: String,
}

/// The differences between two [`Schema`]s, as returned by [`Schema::diff`].
///
/// Its [`Display`] implementation lists one difference per line.
///
/// [`Display`]: fmt::Display
///
/// # Examples
///
/// ```
/// use sqll::Schema;
///
/// let a = Schema::from_sql("CREATE TABLE users (name TEXT)")?;
/// let b = Schema::from_sql("CREATE TABLE users (name TEXT, email TEXT)")?;
///
/// assert!(a.diff(&a).is_empty());
///
/// let diff = a.diff(&b);
/// assert_eq!(diff.iter().collect::<Vec<_>>(), ["+ column users.email TEXT"]);
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    lines: Vec<String>,
}

impl Diff {
    /// Test if there are no differences.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Schema;
    ///
    /// let a = Schema::from_sql("CREATE TABLE users (name TEXT); CREATE TABLE posts (body TEXT)")?;
    /// let b = Schema::from_sql("CREATE TABLE posts (body TEXT); CREATE TABLE users (name TEXT)")?;
    ///
    /// assert!(a.diff(&b).is_empty());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Iterate over the differences, one line each.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::Schema;
    ///
    /// let a = Schema::from_sql("CREATE TABLE users (name TEXT)")?;
    /// let b = Schema::from_sql("CREATE TABLE posts (body TEXT)")?;
    ///
    /// let diff = a.diff(&b);
    /// let lines = diff.iter().collect::<Vec<_>>();
    /// assert_eq!(lines, ["- table users", "+ table posts"]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }

    fn push(&mut self, line: fmt::Arguments<'_>) {
        self.lines.push(format!("{line}"));
    }
}

impl fmt::Display for Diff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, line) in self.lines.iter().enumerate() {
            if n > 0 {
                f.write_char('\n')?;
            }

            f.write_str(line)?;
        }

        Ok(())
    }
}

/// Show `<none>` for an empty string.
struct OrNone<'a>(&'a str);

impl fmt::Display for OrNone<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            f.write_str("<none>")
        } else {
            f.write_str(self.0)
        }
    }
}

/// The names of the columns in `columns` which are also in `other`, separated
/// by commas.
struct Names<'a>(&'a [Column], &'a [Column]);

impl fmt::Display for Names<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        for c in self.0 {
            if !self.1.iter().any(|o| o.name.eq_ignore_ascii_case(&c.name)) {
                continue;
            }

            if !first {
                f.write_str(", ")?;
            }

            first = false;
            f.write_str(&c.name)?;
        }

        Ok(())
    }
}
