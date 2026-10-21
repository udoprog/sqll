use core::fmt;
use core::ops::Deref;

use crate::{Connection, Result};

/// How a transaction acquires its locks, as used by [`Connection::begin`] and
/// [`Connection::transaction_with`].
///
/// See [the SQLite documentation] for the full details.
///
/// The default is [`Immediate`], which is the right choice for any transaction
/// that writes. A [`Deferred`] transaction starts out as a reader and upgrades
/// to a writer on its first write. In WAL mode that upgrade fails with
/// `SQLITE_BUSY_SNAPSHOT` if another connection committed since the
/// transaction's first read, and since the transaction's view of the database
/// is already stale the busy timeout does not retry it. An [`Immediate`]
/// transaction takes the write lock up front, where the busy timeout does
/// apply, so it waits for other writers instead of failing.
///
/// [the SQLite documentation]: https://www.sqlite.org/lang_transaction.html
/// [`Immediate`]: TransactionBehavior::Immediate
/// [`Deferred`]: TransactionBehavior::Deferred
///
/// # Examples
///
/// ```
/// use sqll::{Connection, TransactionBehavior};
///
/// let c = Connection::open_in_memory()?;
/// c.execute("CREATE TABLE users (name TEXT)")?;
///
/// let names = c.transaction_with(TransactionBehavior::Deferred, |c| {
///     let mut stmt = c.prepare("SELECT name FROM users")?;
///     stmt.iter::<String>().collect::<sqll::Result<Vec<_>>>()
/// })?;
///
/// assert!(names.is_empty());
/// # Ok::<_, sqll::Error>(())
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransactionBehavior {
    /// `BEGIN DEFERRED`: no lock is taken until the database is first read
    /// or written.
    Deferred,
    /// `BEGIN IMMEDIATE`: the write lock is taken when the transaction
    /// starts. This is the default.
    #[default]
    Immediate,
    /// `BEGIN EXCLUSIVE`: like [`Immediate`], and outside of WAL mode it
    /// also keeps readers out until the transaction ends.
    ///
    /// [`Immediate`]: TransactionBehavior::Immediate
    Exclusive,
}

impl TransactionBehavior {
    fn begin_sql(self) -> &'static str {
        match self {
            TransactionBehavior::Deferred => "BEGIN DEFERRED",
            TransactionBehavior::Immediate => "BEGIN IMMEDIATE",
            TransactionBehavior::Exclusive => "BEGIN EXCLUSIVE",
        }
    }
}

/// An open transaction, returned by [`Connection::begin`].
///
/// The transaction ends with [`commit`] or [`rollback`]. If it is dropped
/// without either, including when unwinding from a panic, it is rolled back
/// and any error from doing so is ignored.
///
/// It dereferences to the [`Connection`] it was started on.
///
/// [`commit`]: Transaction::commit
/// [`rollback`]: Transaction::rollback
///
/// # Examples
///
/// ```
/// use sqll::{Connection, TransactionBehavior};
///
/// let c = Connection::open_in_memory()?;
/// c.execute("CREATE TABLE users (name TEXT)")?;
///
/// let tx = c.begin(TransactionBehavior::Immediate)?;
/// tx.execute("INSERT INTO users VALUES ('Alice')")?;
/// drop(tx);
///
/// let tx = c.begin(TransactionBehavior::Immediate)?;
/// tx.execute("INSERT INTO users VALUES ('Bob')")?;
/// tx.commit()?;
///
/// let mut stmt = c.prepare("SELECT name FROM users")?;
/// let names = stmt.iter::<String>().collect::<sqll::Result<Vec<_>>>()?;
/// assert_eq!(names, ["Bob"]);
/// # Ok::<_, sqll::Error>(())
/// ```
#[must_use = "a transaction is rolled back when dropped, call commit() to keep its changes"]
pub struct Transaction<'a> {
    c: &'a Connection,
    done: bool,
}

impl<'a> Transaction<'a> {
    pub(crate) fn begin(c: &'a Connection, behavior: TransactionBehavior) -> Result<Self> {
        c.execute(behavior.begin_sql())?;
        Ok(Self { c, done: false })
    }

    /// Commit the transaction.
    ///
    /// If the commit fails the transaction is rolled back, so that the
    /// connection is never left inside of it, and the commit error is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, TransactionBehavior};
    ///
    /// let c = Connection::open_in_memory()?;
    /// c.execute("CREATE TABLE users (name TEXT)")?;
    ///
    /// let tx = c.begin(TransactionBehavior::Immediate)?;
    /// tx.execute("INSERT INTO users VALUES ('Alice')")?;
    /// tx.commit()?;
    ///
    /// let mut stmt = c.prepare("SELECT COUNT(*) FROM users")?;
    /// assert_eq!(stmt.iter::<i64>().collect::<sqll::Result<Vec<_>>>()?, [1]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn commit(mut self) -> Result<()> {
        self.done = true;

        if let Err(error) = self.c.execute("COMMIT") {
            // A failed COMMIT can leave the transaction open (such as on
            // SQLITE_BUSY), or SQLite may already have rolled it back, in which
            // case this fails harmlessly.
            _ = self.c.execute("ROLLBACK");
            return Err(error);
        }

        Ok(())
    }

    /// Roll the transaction back.
    ///
    /// Unlike dropping the transaction, this reports an error from the
    /// rollback.
    ///
    /// # Examples
    ///
    /// ```
    /// use sqll::{Connection, TransactionBehavior};
    ///
    /// let c = Connection::open_in_memory()?;
    /// c.execute("CREATE TABLE users (name TEXT)")?;
    ///
    /// let tx = c.begin(TransactionBehavior::Immediate)?;
    /// tx.execute("INSERT INTO users VALUES ('Alice')")?;
    /// tx.rollback()?;
    ///
    /// let mut stmt = c.prepare("SELECT COUNT(*) FROM users")?;
    /// assert_eq!(stmt.iter::<i64>().collect::<sqll::Result<Vec<_>>>()?, [0]);
    /// # Ok::<_, sqll::Error>(())
    /// ```
    pub fn rollback(mut self) -> Result<()> {
        self.done = true;
        self.c.execute("ROLLBACK")
    }
}

impl Deref for Transaction<'_> {
    type Target = Connection;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.c
    }
}

impl fmt::Debug for Transaction<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transaction").finish_non_exhaustive()
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.done {
            _ = self.c.execute("ROLLBACK");
        }
    }
}

/// Run `f` inside of a transaction on `c`, committing if it returns `Ok` and
/// rolling back if it returns `Err` or panics.
pub(crate) fn run<T, E>(
    c: &Connection,
    behavior: TransactionBehavior,
    f: impl FnOnce() -> Result<T, E>,
) -> Result<T, E>
where
    E: From<crate::Error>,
{
    let tx = Transaction::begin(c, behavior)?;

    match f() {
        Ok(value) => {
            tx.commit()?;
            Ok(value)
        }
        Err(error) => {
            drop(tx);
            Err(error)
        }
    }
}
