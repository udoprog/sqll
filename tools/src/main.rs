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
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use bindgen::Builder;
use bindgen::callbacks::{IntKind, ParseCallbacks};
use clap::Parser;
use regex::RegexSet;
use relative_path::Component;
use relative_path::RelativePath;
use zip::ZipArchive;

const URL: &str = "https://github.com/sqlite/sqlite";
const HEADERS: &[&str] = &["sqlite3.h"];
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

// NB: Excluding these files causes the source file to include a massive comment
// about changed sources.
const EXCLUDE: &[&str] = &[
    // "^.fossil-settings/.*$",
    // "^art/.*$",
    // "^test/.*$",
    // "^mptest/.*$",
    // "^tsrc/.*$",
    // "^contrib/.*$",
    // "^doc/.*$",
];

#[derive(Parser)]
struct Opts {
    /// Skip updating the sqlite3 source code.
    #[clap(long)]
    skip_update: bool,
    /// Force updating the sqlite3 source code even if it already exists.
    #[clap(long)]
    force_update: bool,
    /// Generate bindings without any filtering.
    #[clap(long)]
    unfiltered: bool,
    /// Additional regex patterns to exclude files when extracting a source
    /// archive.
    #[clap(long)]
    exclude: Vec<String>,
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

#[tokio::main]
async fn main() {
    let opts = Opts::parse();

    if let Err(e) = entry(&opts).await {
        println!("Error: {e}");

        let mut cause = e.source();

        while let Some(c) = cause {
            println!("Caused by: {c}");
            cause = c.source();
        }
    }
}

async fn entry(opts: &Opts) -> Result<()> {
    let mut exclude = Vec::new();

    for &e in EXCLUDE {
        exclude.push(e.to_owned());
    }

    for e in &opts.exclude {
        exclude.push(e.to_owned());
    }

    let exclude = RegexSet::new(exclude)?;

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

    let build = async |version: &str, files: &[&str]| -> Result<()> {
        let version_dir = sqlite_root.join(version);
        let work_dir = sqlite_root.join(format!(".work-{version}"));

        if version_dir.is_dir() && !opts.force_update {
            return Ok(());
        }

        let url = format!("{URL}/archive/refs/tags/{version}.zip");

        println!("Downloading sqlite3 from {url}");

        let bytes = reqwest::get(&url)
            .await
            .context("downloading sqlite3 source")?
            .bytes()
            .await
            .context("reading sqlite3 source")?;

        if work_dir.is_dir() {
            fs::remove_dir_all(&work_dir).context("removing existing work directory")?;
        }

        fs::create_dir_all(&work_dir).context("creating work directory")?;
        extract_archive(&work_dir, &bytes, &exclude)?;

        if version_dir.is_dir() {
            fs::remove_dir_all(&version_dir).context("removing existing sqlite3 source")?;
        }

        fs::rename(&work_dir, &version_dir).context("renaming sqlite3 source directory")?;

        cmd!(in version_dir, Path::new("./configure"));
        cmd!(in version_dir, "make", "clean");
        cmd!(in version_dir, "make", "sqlite3.c");

        let source_dir = sys_root.join("source");

        if !source_dir.is_dir() {
            fs::create_dir(&source_dir).context("creating source directory")?;
        }

        for name in files {
            println!("Copying {name}");

            fs::copy(version_dir.join(name), source_dir.join(name))
                .with_context(|| format!("copying {}", name))?;
        }

        Ok(())
    };

    if !opts.skip_update {
        build(&version, &HEADERS).await?;
        build(&bundled_version, &BUNDLED).await?;
    }

    println!("Generating bindings");

    let constants = CONSTANTS.join("|");

    let mut builder = Builder::default()
        .header(sys_root.join("source/sqlite3.h").display().to_string())
        .use_core()
        .disable_header_comment()
        .derive_copy(false)
        .derive_debug(false)
        .derive_eq(false)
        .derive_hash(false)
        .derive_ord(false)
        .derive_partialeq(false)
        .derive_partialord(false)
        .parse_callbacks(Box::new(IntegerDefines));

    if !opts.unfiltered {
        builder = builder
            .allowlist_item(format!("SQLITE_({constants})"))
            .allowlist_item("SQLITE_PREPARE_.*")
            .allowlist_item("sqlite3_(libversion_number|libversion)")
            .allowlist_item("sqlite3_(reset|step|open_v2|close_v2|prepare_v3|finalize)")
            .allowlist_item("sqlite3_db_readonly")
            .allowlist_item("sqlite3_(errstr|errmsg|extended_result_codes)")
            .allowlist_item("sqlite3_(clear_bindings|busy_handler|busy_timeout|changes|total_changes|last_insert_rowid)")
            .allowlist_item("sqlite3_bind_parameter_(index|name)")
            .allowlist_item("sqlite3_column_(name|type|count|bytes|text|double|int64|null|blob)")
            .allowlist_item("sqlite3_bind_(bytes|text|double|int64|null|blob)");
    }

    builder
        .generate()?
        .write_to_file(sys_root.join("src/base.rs"))
        .context("generating bindings")?;

    cmd!(in ".", "cargo", "check", "-p", "sqll", "--features", "bundled");
    Ok(())
}

fn extract_archive(out: &Path, data: &[u8], exclude: &RegexSet) -> Result<(), anyhow::Error> {
    let mut archive = ZipArchive::new(Cursor::new(data)).context("reading sqlite3 zip archive")?;
    let mut contents = Vec::new();

    'outer: for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        let name = RelativePath::new(file.name());
        let mut it = name.components();
        it.next();
        let name = it.as_relative_path();

        if name.components().next().is_none() {
            continue;
        }

        // Exported archives have a top-level directory we need to skip.
        let mut out_path = out.to_path_buf();

        for c in name.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    continue 'outer;
                }
                Component::Normal(c) => {
                    out_path.push(c);
                }
            }
        }

        if !exclude.is_empty() && exclude.is_match(name.as_str()) {
            continue;
        }

        if file.is_dir() {
            if !out_path.is_dir() {
                fs::create_dir_all(&out_path)
                    .with_context(|| format!("creating {}", out_path.display()))?;
            }

            continue;
        }

        contents.clear();
        file.read_to_end(&mut contents)?;

        let f = File::create_new(&out_path)?;

        #[cfg(unix)]
        if let Some(m) = file.unix_mode() {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            let p = Permissions::from_mode(m);

            f.set_permissions(p)
                .with_context(|| format!("setting permissions on {}", out_path.display()))?;
        }

        fs::write(&out_path, &contents)
            .with_context(|| format!("writing {}", out_path.display()))?;
    }

    Ok(())
}

#[derive(Debug)]
struct IntegerDefines;

impl ParseCallbacks for IntegerDefines {
    fn int_macro(&self, _: &str, _: i64) -> Option<IntKind> {
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
