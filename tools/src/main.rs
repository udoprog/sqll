//! Tool to fetch sqlite3 source and update bindings.
//!
//! Use with:
//!
//! ```
//! cargo run -p tools
//! ```
//!
//! This reads the following files:
//! * `sqll-sys/sqlite3-version` to determine which API to target.
//! * `sqll-sys/sqlite3-version-bundled` to determine which version of sqlite3
//!   to bundle.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use bindgen::Builder;
use bindgen::callbacks::{IntKind, ParseCallbacks};
use clap::Parser;

const URL: &str = "https://github.com/sqlite/sqlite";
const HEADERS: &[&str] = &["sqlite3.h", "sqlite3ext.h"];
const BUNDLED: &[&str] = &["sqlite3.c"];

const CONSTANTS: &[&str] = &[
    "NULL",
    "INTEGER",
    "FLOAT",
    "TEXT",
    "BLOB",
    "OK",
    "DONE",
    "ROW",
    "OPEN_READONLY",
    "OPEN_READWRITE",
    "OPEN_CREATE",
    "OPEN_URI",
    "OPEN_MEMORY",
    "OPEN_NOMUTEX",
    "OPEN_FULLMUTEX",
    "OPEN_SHAREDCACHE",
    "OPEN_PRIVATECACHE",
    "OPEN_NOFOLLOW",
    "OPEN_EXRESCODE",
];

#[derive(Parser)]
struct Opts {
    /// Skip updating the sqlite3 source code.
    #[clap(long)]
    skip_update: bool,
}

macro_rules! cmd {
    (in $path:expr, $cmd:expr $(, $arg:expr)* $(,)?) => {{
        let mut name = Vec::new();
        name.push(Display::to_string($cmd));
        $(name.push(Display::to_string($arg));)*
        let name = name.join(" ");

        println!("{name}");

        let mut c = Command::new($cmd);
        $(c.arg($arg);)*
        c.current_dir(&$path);
        let status = c.status()?;

        if !status.success() {
            return Err(anyhow::anyhow!("{name}: {status}"));
        }
    }};
}

fn main() {
    let opts = Opts::parse();

    if let Err(e) = entry(&opts) {
        println!("Error: {e}");

        let mut cause = e.source();

        while let Some(c) = cause {
            println!("Caused by: {c}");
            cause = c.source();
        }
    }
}

fn entry(opts: &Opts) -> Result<()> {
    let mut root = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .context("CARGO_MANIFEST_DIR environment variable not set")?,
    );
    root.pop();

    let sys_root = root.join("sqll-sys");
    let sqlite_root = root.join("sqlite3");

    let version =
        fs::read_to_string(sys_root.join("sqlite3-version")).context("reading sqlite3-version")?;

    let bundled_version = fs::read_to_string(sys_root.join("sqlite3-version-bundled"))
        .context("reading sqlite3-version-bundled")?;

    let min_rev = format!("refs/tags/{version}");
    let bundled_rev = format!("refs/tags/{bundled_version}");

    let build = |rev: &str, files: &[&str]| -> Result<()> {
        let rev_to = format!("{rev}:{rev}");

        if !sqlite_root.is_dir() {
            println!("Cloning to {}", sqlite_root.display());
            cmd!(in sqlite_root, "git", "clone", "--depth", "1", URL, "--revision", rev);
        }

        cmd!(in sqlite_root, "git", "fetch", "--depth", "1", "origin", &rev_to);
        cmd!(in sqlite_root, "git", "checkout", rev);

        cmd!(in sqlite_root, Path::new("./configure"));
        cmd!(in sqlite_root, "make", "clean");
        cmd!(in sqlite_root, "make", "sqlite3.c");

        let source_dir = sys_root.join("source");

        if !source_dir.is_dir() {
            fs::create_dir(&source_dir).context("creating source directory")?;
        }

        for name in files {
            println!("Copying {name}");

            fs::copy(sqlite_root.join(name), source_dir.join(name))
                .with_context(|| format!("copying {}", name))?;
        }

        Ok(())
    };

    if !opts.skip_update {
        build(&min_rev, &HEADERS)?;
        build(&bundled_rev, &BUNDLED)?;
    }

    println!("Generating bindings");

    let constants = CONSTANTS.join("|");

    Builder::default()
        .header(sys_root.join("source/sqlite3.h").display().to_string())
        .use_core()
        .disable_header_comment()
        .allowlist_item(format!("SQLITE_({constants})"))
        .allowlist_item("SQLITE_PREPARE_.*")
        .allowlist_item("sqlite3_(reset|step|open_v2|close_v2|prepare_v3|finalize)")
        .allowlist_item("sqlite3_(libversion_number|clear_bindings|busy_handler|busy_timeout|changes|total_changes|errstr|extended_result_codes|errmsg)")
        .allowlist_item("sqlite3_bind_parameter_(index|name)")
        .allowlist_item("sqlite3_column_(name|type|count|bytes|text|double|int64|null|blob)")
        .allowlist_item("sqlite3_bind_(bytes|text|double|int64|null|blob)")
        .parse_callbacks(Box::new(CIntCallbacks))
        .generate()?
        .write_to_file(sys_root.join("src/base.rs"))
        .context("generating bindings")?;

    cmd!(in ".", "cargo", "check", "-p", "sqll", "--features", "bundled");
    Ok(())
}

#[derive(Debug)]
struct CIntCallbacks;

impl ParseCallbacks for CIntCallbacks {
    fn int_macro(&self, _name: &str, _value: i64) -> Option<IntKind> {
        Some(IntKind::Int)
    }
}

trait Display {
    fn to_string(&self) -> String;
}

impl<T> Display for &T
where
    T: ?Sized + Display,
{
    #[inline]
    fn to_string(&self) -> String {
        (*self).to_string()
    }
}

impl Display for str {
    #[inline]
    fn to_string(&self) -> String {
        String::from(self)
    }
}

impl Display for String {
    #[inline]
    fn to_string(&self) -> String {
        self.clone()
    }
}

impl Display for Path {
    #[inline]
    fn to_string(&self) -> String {
        self.display().to_string()
    }
}

impl Display for PathBuf {
    #[inline]
    fn to_string(&self) -> String {
        self.display().to_string()
    }
}
