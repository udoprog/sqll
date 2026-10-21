use core::fmt;

use alloc::format;
use alloc::string::String;

use crate::{Code, Connection, Error, Result, TransactionBehavior};

/// A runner for ordered SQL migrations, each recorded in a table once it has
/// been applied.
///
/// Every migration is a pair of a unique id and the SQL to run, which may hold
/// several statements. [`run`] goes through them in the order given and, for
/// each id not yet recorded, runs its SQL and records the id inside of a single
/// [`Immediate`] transaction, so a migration is either applied and recorded or
/// not applied at all. Migrations are not sorted, so pass them in the order
/// they should be applied, such as sorted by file name.
///
/// The ids are recorded in a table named `migrations` by default, created if it
/// does not exist as:
///
/// ```sql
/// CREATE TABLE IF NOT EXISTS migrations (
///     id         TEXT PRIMARY KEY,
///     applied_at INTEGER NOT NULL
/// )
/// ```
///
/// where `applied_at` is set to the time the migration was recorded, in
/// seconds since the Unix epoch. A table that already exists is used as it is,
/// as long as it has the `id` and `applied_at` columns.
///
/// Migrations are taken as `&str` ids and SQL, so they can come from
/// [`include_str!`] or be embedded in any other way.
///
/// [`run`]: Self::run
/// [`Immediate`]: TransactionBehavior::Immediate
///
/// # Examples
///
/// ```
/// use sqll::{Connection, Migrations};
///
/// const MIGRATIONS: &[(&str, &str)] = &[
///     ("0001_users", "CREATE TABLE users (name TEXT)"),
///     ("0002_email", "ALTER TABLE users ADD COLUMN email TEXT"),
/// ];
///
/// let c = Connection::open_in_memory()?;
///
/// assert_eq!(Migrations::new().run(&c, MIGRATIONS.iter().copied())?, 2);
/// // Running them again does nothing, since they are all recorded.
/// assert_eq!(Migrations::new().run(&c, MIGRATIONS.iter().copied())?, 0);
///
/// c.execute("INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')")?;
/// # Ok::<_, sqll::Error>(())
/// ```
///
/// Migrating the database when opening a [`Pool`], before its statements are
/// prepared:
///
/// ```
/// use sqll::{Migrations, OpenOptions, PoolBuilder, SendStatement, Statements};
///
/// #[derive(Statements)]
/// #[sql(read_only)]
/// struct Read {
///     #[sql = "SELECT name, email FROM users ORDER BY name"]
///     users: SendStatement,
/// }
///
/// #[derive(Statements)]
/// struct Write {
///     #[sql = "INSERT INTO users (name, email) VALUES (?, ?)"]
///     insert: SendStatement,
/// }
///
/// const MIGRATIONS: &[(&str, &str)] = &[
///     ("0001_users", "CREATE TABLE users (name TEXT)"),
///     ("0002_email", "ALTER TABLE users ADD COLUMN email TEXT"),
/// ];
///
/// # let dir = tempfile::tempdir()?;
/// # let path = dir.path().join("example.db");
/// let mut options = OpenOptions::new();
/// options.create();
///
/// let pool = PoolBuilder::new(options, 4)
///     .with_write_setup(|c| Migrations::new().run(c, MIGRATIONS.iter().copied()).map(|_| ()))
///     .open::<Read, Write>(&path)?;
/// # Ok::<_, anyhow::Error>(())
/// ```
///
/// [`Pool`]: crate::Pool
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Migrations<'a> {
    table: &'a str,
    behavior: TransactionBehavior,
    foreign_keys_off: bool,
    fresh_schema: Option<&'a str>,
}

impl<'a> Migrations<'a> {
    /// Construct a migration runner recording migrations in the `migrations`
    /// table.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Migrations};
    ///
    /// let c = Connection::open_in_memory()?;
    /// Migrations::new().run(&c, [("0001_users", "CREATE TABLE users (name TEXT)")])?;
    ///
    /// let mut stmt = c.prepare("SELECT id FROM migrations")?;
    /// assert_eq!(stmt.first::<String>(())?.as_deref(), Some("0001_users"));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub const fn new() -> Self {
        Self {
            table: "migrations",
            behavior: TransactionBehavior::Immediate,
            foreign_keys_off: false,
            fresh_schema: None,
        }
    }

    /// Record migrations in the table named `table` instead of
    /// `migrations`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Migrations};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// Migrations::new()
    ///     .table("schema_history")
    ///     .run(&c, [("0001_users", "CREATE TABLE users (name TEXT)")])?;
    ///
    /// let mut stmt = c.prepare("SELECT COUNT(*) FROM schema_history")?;
    /// assert_eq!(stmt.first::<i64>(())?, Some(1));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub const fn table(mut self, table: &'a str) -> Self {
        self.table = table;
        self
    }

    /// Set the [`TransactionBehavior`] each migration runs with. The default
    /// is [`Immediate`].
    ///
    /// [`Immediate`]: TransactionBehavior::Immediate
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Migrations, TransactionBehavior};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// Migrations::new()
    ///     .behavior(TransactionBehavior::Exclusive)
    ///     .run(&c, [("0001_users", "CREATE TABLE users (name TEXT)")])?;
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub const fn behavior(mut self, behavior: TransactionBehavior) -> Self {
        self.behavior = behavior;
        self
    }

    /// Switch foreign key enforcement off while migrations run, and check
    /// that every foreign key holds with `PRAGMA foreign_key_check` before
    /// each migration commits.
    ///
    /// This allows a migration to rebuild a table the way SQLite recommends
    /// (create a new table, copy the rows, drop the old one and rename the new
    /// one), which would otherwise cascade deletes or fail on the `DROP TABLE`
    /// of a table other tables refer to. Since `PRAGMA foreign_keys` cannot be
    /// changed inside of a transaction, it is switched off before the first
    /// migration and restored after the last, and only if it was on to begin
    /// with.
    ///
    /// A migration that leaves a foreign key violated fails with
    /// [`Code::CONSTRAINT_FOREIGNKEY`] and is rolled back.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Code, Connection, Migrations};
    ///
    /// let c = Connection::open_in_memory()?;
    /// c.execute("PRAGMA foreign_keys = ON")?;
    ///
    /// let migrations = Migrations::new().foreign_keys_off(true);
    ///
    /// migrations.run(&c, [
    ///     ("0001_users", "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)"),
    ///     ("0002_posts", "CREATE TABLE posts (user_id INTEGER REFERENCES users (id))"),
    ///     ("0003_alice", "INSERT INTO users VALUES (1, 'Alice'); INSERT INTO posts VALUES (1)"),
    /// ])?;
    ///
    /// // Rebuild the users table, which posts refers to.
    /// migrations.run(&c, [
    ///     ("0004_users_age", r#"
    ///         CREATE TABLE users_new (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
    ///         INSERT INTO users_new (id, name) SELECT id, name FROM users;
    ///         DROP TABLE users;
    ///         ALTER TABLE users_new RENAME TO users;
    ///     "#),
    /// ])?;
    ///
    /// // A migration which breaks a foreign key is rolled back.
    /// let error = migrations
    ///     .run(&c, [("0005_drop_alice", "DELETE FROM users WHERE id = 1")])
    ///     .unwrap_err();
    ///
    /// assert_eq!(error.code(), Code::CONSTRAINT_FOREIGNKEY);
    ///
    /// let mut stmt = c.prepare("SELECT COUNT(*) FROM users")?;
    /// assert_eq!(stmt.first::<i64>(())?, Some(1));
    ///
    /// // Foreign keys are enforced again afterwards.
    /// assert!(c.execute("DELETE FROM users").is_err());
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub const fn foreign_keys_off(mut self, enabled: bool) -> Self {
        self.foreign_keys_off = enabled;
        self
    }

    /// On a fresh database, run `schema` instead of the migrations and record
    /// every migration as applied.
    ///
    /// This lets a new database be created from the current schema in one
    /// step, instead of replaying every migration that led up to it. `schema`
    /// must therefore produce the same result as running every migration
    /// passed to [`run`].
    ///
    /// A database is fresh if it has no tables, indexes, views or triggers
    /// other than the migrations table, and no recorded migrations. The check,
    /// running `schema` and recording the migrations all happen in one
    /// transaction.
    ///
    /// [`run`]: Self::run
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Migrations};
    ///
    /// const MIGRATIONS: &[(&str, &str)] = &[
    ///     ("0001_users", "CREATE TABLE users (name TEXT)"),
    ///     ("0002_email", "ALTER TABLE users ADD COLUMN email TEXT"),
    /// ];
    ///
    /// const SCHEMA: &str = "CREATE TABLE users (name TEXT, email TEXT)";
    ///
    /// let migrations = Migrations::new().fresh_schema(SCHEMA);
    ///
    /// let c = Connection::open_in_memory()?;
    /// assert_eq!(migrations.run(&c, MIGRATIONS.iter().copied())?, 2);
    ///
    /// let mut stmt = c.prepare("SELECT COUNT(*) FROM migrations")?;
    /// assert_eq!(stmt.first::<i64>(())?, Some(2));
    ///
    /// // An existing database runs the migrations it lacks instead.
    /// let c = Connection::open_in_memory()?;
    /// Migrations::new().run(&c, MIGRATIONS[..1].iter().copied())?;
    /// assert_eq!(migrations.run(&c, MIGRATIONS.iter().copied())?, 1);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    #[inline]
    pub const fn fresh_schema(mut self, schema: &'a str) -> Self {
        self.fresh_schema = Some(schema);
        self
    }

    /// Apply every migration in `migrations` that has not been recorded yet,
    /// in order, returning how many were applied.
    ///
    /// Each migration runs and is recorded in its own transaction. Whether it
    /// has already been recorded is checked again inside of that transaction,
    /// so several connections running the same migrations at once apply each
    /// of them only once.
    ///
    /// If a migration fails, it is rolled back and its error is returned with
    /// the same [`Code`] and the migration's id added to the message.
    /// Migrations applied before it stay applied.
    ///
    /// This takes a shared [`Connection`], so it can be used from a
    /// [`PoolBuilder::with_write_setup`] hook. It must not be called inside of
    /// a transaction.
    ///
    /// [`PoolBuilder::with_write_setup`]: crate::PoolBuilder::with_write_setup
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, Migrations};
    ///
    /// let c = Connection::open_in_memory()?;
    ///
    /// let error = Migrations::new()
    ///     .run(&c, [
    ///         ("0001_users", "CREATE TABLE users (name TEXT)"),
    ///         ("0002_broken", "CREATE TABLE posts (; INSERT INTO users VALUES ('Alice')"),
    ///     ])
    ///     .unwrap_err();
    ///
    /// assert!(error.to_string().starts_with("migration 0002_broken: "));
    ///
    /// let mut stmt = c.prepare("SELECT id FROM migrations")?;
    /// assert_eq!(stmt.first::<String>(())?.as_deref(), Some("0001_users"));
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn run<I, K, S>(&self, c: &Connection, migrations: I) -> Result<usize>
    where
        I: IntoIterator<Item = (K, S)>,
        K: AsRef<str>,
        S: AsRef<str>,
    {
        let table = quote(self.table);

        let _foreign_keys = if self.foreign_keys_off {
            ForeignKeysOff::new(c)?
        } else {
            None
        };

        let mut migrations = migrations.into_iter();

        let fresh = c.transaction_with(self.behavior, |c| {
            c.execute(format!(
                "CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, applied_at INTEGER NOT NULL)"
            ))?;

            let Some(schema) = self.fresh_schema else {
                return Ok(None);
            };

            if !is_fresh(c, self.table, &table)? {
                return Ok(None);
            }

            c.execute(schema)
                .map_err(|e| context(e, format_args!("fresh schema")))?;

            let mut insert = record_statement(c, &table)?;
            let mut count = 0;

            for (id, _) in migrations.by_ref() {
                let id = id.as_ref();

                insert
                    .execute(id)
                    .map_err(|e| context(e, format_args!("recording migration {id}")))?;

                count += 1;
            }

            if self.foreign_keys_off {
                foreign_key_check(c)?;
            }

            Ok(Some(count))
        })?;

        if let Some(count) = fresh {
            return Ok(count);
        }

        let mut select = c.prepare(format!("SELECT 1 FROM {table} WHERE id = ?"))?;
        let mut insert = record_statement(c, &table)?;
        let mut count = 0;

        for (id, sql) in migrations {
            let id = id.as_ref();

            if select.first::<i64>(id)?.is_some() {
                continue;
            }

            let applied = c
                .transaction_with(self.behavior, |c| {
                    // Another connection might have applied it while we
                    // waited for the transaction.
                    if select.first::<i64>(id)?.is_some() {
                        return Ok(false);
                    }

                    c.execute(sql.as_ref())?;

                    if self.foreign_keys_off {
                        foreign_key_check(c)?;
                    }

                    insert.execute(id)?;
                    Ok(true)
                })
                .map_err(|e| context(e, format_args!("migration {id}")))?;

            count += usize::from(applied);
        }

        Ok(count)
    }
}

impl Default for Migrations<'_> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

fn record_statement(c: &Connection, table: &str) -> Result<crate::Statement> {
    c.prepare(format!(
        "INSERT INTO {table} (id, applied_at) VALUES (?, CAST(strftime('%s', 'now') AS INTEGER))"
    ))
}

/// Test if the database holds nothing but an empty migrations table.
fn is_fresh(c: &Connection, name: &str, table: &str) -> Result<bool> {
    let mut stmt = c.prepare(
        "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\' AND tbl_name != ?",
    )?;

    if stmt.first::<i64>(name)?.unwrap_or_default() != 0 {
        return Ok(false);
    }

    let mut stmt = c.prepare(format!("SELECT COUNT(*) FROM {table}"))?;
    Ok(stmt.first::<i64>(())?.unwrap_or_default() == 0)
}

/// Fail if any foreign key is violated.
fn foreign_key_check(c: &Connection) -> Result<()> {
    let mut stmt = c.prepare("PRAGMA foreign_key_check")?;

    // Columns are: table, rowid, parent, fkid.
    if let Some((table, _, parent)) = stmt.first::<(String, Option<i64>, String)>(())? {
        return Err(Error::new(
            Code::CONSTRAINT_FOREIGNKEY,
            format_args!("foreign key from {table} to {parent} violated"),
        ));
    }

    Ok(())
}

/// Quote an identifier.
fn quote(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');

    for c in name.chars() {
        if c == '"' {
            out.push('"');
        }

        out.push(c);
    }

    out.push('"');
    out
}

fn context(error: Error, what: fmt::Arguments<'_>) -> Error {
    Error::new(error.code(), format_args!("{what}: {error}"))
}

/// Switches foreign keys off, and back on when dropped.
struct ForeignKeysOff<'a> {
    c: &'a Connection,
}

impl<'a> ForeignKeysOff<'a> {
    fn new(c: &'a Connection) -> Result<Option<Self>> {
        let mut stmt = c.prepare("PRAGMA foreign_keys")?;

        if stmt.first::<i64>(())?.unwrap_or_default() == 0 {
            return Ok(None);
        }

        c.execute("PRAGMA foreign_keys = OFF")?;
        Ok(Some(Self { c }))
    }
}

impl Drop for ForeignKeysOff<'_> {
    fn drop(&mut self) {
        _ = self.c.execute("PRAGMA foreign_keys = ON");
    }
}
