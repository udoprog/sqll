use core::error::Error;

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use cc::Build;
use semver::{Version, VersionReq};

/// Updated automatically. DO NOT TOUCH.
const SQLITE_VERSION: &str = "3.37.0";
const CLANG_ENV: &[&str] = &["SQLL_CLANG_PATH", "CLANG_PATH"];
const TARGET_ENV: &[&str] = &["SQLL_TARGET", "TARGET"];

fn main() {
    if cfg!(feature = "bundled") {
        bundled();
    } else {
        system();
    }
}

fn env(names: &[&'static str]) -> Option<OsString> {
    for &name in names {
        println!("cargo:rerun-if-env-changed={name}");

        if let Some(value) = env::var_os(name) {
            return Some(value);
        }
    }

    None
}

fn system() {
    let Ok(version_req) = SQLITE_VERSION.parse::<VersionReq>() else {
        panic!("invalid version: {SQLITE_VERSION}");
    };

    let mut errors = Vec::new();

    match pkg_config::find_library("sqlite3") {
        Ok(library) => {
            let Ok(version) = library.version.parse::<Version>() else {
                panic!("invalid sqlite3 library version: {}", library.version);
            };

            if !version_req.matches(&version) {
                panic!(
                    "system sqlite3 library version {} does not match required version {}",
                    library.version, version_req
                );
            }

            return;
        }
        Err(error) => {
            errors.push(format!("pkg-config: {error}"));
        }
    }

    match vcpkg::find_package("sqlite3") {
        Ok(..) => {
            return;
        }
        Err(error) => {
            errors.push(format!("vcpkg: {error}"));

            let mut cause = error.source();

            while let Some(inner) = cause {
                errors.push(format!("vcpkg: {inner}"));
                cause = inner.source();
            }
        }
    }

    for error in errors {
        println!("{error}");
    }

    panic!("No configuration method for system sqlite3 succeeded")
}

fn bundled() {
    let mut build = Build::new();

    build.file("source/sqlite3.c");

    for (name, value) in env::vars() {
        if name.starts_with("SQLITE_") {
            build.define(&name, value.as_str());
            println!("cargo:rerun-if-env-changed={name}");
        }
    }

    if let Some(mut sdk_path) = env(CLANG_ENV).map(PathBuf::from) {
        sdk_path.push("bin");
        sdk_path.push("clang");

        if !sdk_path.is_file() {
            panic!("Not a file: {}", sdk_path.display());
        }

        build.compiler(sdk_path);
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=source/sqlite3.c");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_FAMILY");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");

    let mut is_wasm = false;

    if let Ok(target_family) = env::var("CARGO_CFG_TARGET_FAMILY")
        && target_family == "wasm"
    {
        is_wasm = true;
    }

    if let Some(target_env) = env(TARGET_ENV) {
        let target = format!("{}", target_env.to_string_lossy());
        build.target(&target);
    }

    if !cfg!(feature = "threadsafe") || is_wasm {
        build.define("SQLITE_THREADSAFE", "0");
    }

    if is_wasm {
        build.define("SQLITE_OMIT_LOAD_EXTENSION", "1");
    }

    if cfg!(feature = "strict") {
        build.flags(["-Wall", "-Wextra", "-Werror"]);
    }

    if cfg!(not(debug_assertions)) {
        build.define("NDEBUG", "1");
        build.flag("-O3");
    }

    build.compile("libsqlite3.a");
}
