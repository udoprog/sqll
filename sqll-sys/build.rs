use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use cc::Build;
use semver::{Version, VersionReq};

const SDK_PATH_ENV: &[&str] = &["CARGO_CFG_SQLL_WASI_SDK_PATH", "WASI_SDK_PATH"];
const WASI_TARGET_ENV: &[&str] = &["CARGO_CFG_SQLL_WASI_TARGET_ENV", "WASI_TARGET_ENV"];
const SQLITE_VERSION: &str = include_str!("sqlite3-version");

fn main() {
    if cfg!(feature = "bundled") {
        bundled();
    } else {
        system();
    }
}

fn env(names: &[&'static str]) -> OsString {
    for &name in names {
        println!("cargo:rerun-if-env-changed={name}");

        if let Some(value) = env::var_os(name) {
            return value;
        }
    }

    let expected = names.join(", ");
    panic!("expected one of these environments to be set: {expected}");
}

fn system() {
    let Some(("version", sqlite3_version)) = SQLITE_VERSION.split_once('-') else {
        panic!("invalid version: {SQLITE_VERSION}");
    };

    let Ok(sqlite3_version_req) = sqlite3_version.parse::<VersionReq>() else {
        panic!("invalid version: {sqlite3_version}");
    };

    let mut errors = Vec::new();

    'vcpkg: {
        _ = match vcpkg::find_package("sqlite3") {
            Ok(library) => library,
            Err(error) => {
                errors.push(format!("vcpkg failed: {error}"));
                break 'vcpkg;
            }
        };

        return;
    };

    'pkg_config: {
        let library = match pkg_config::find_library("sqlite3") {
            Ok(library) => library,
            Err(error) => {
                errors.push(format!("pkg-config failed: {error}"));
                break 'pkg_config;
            }
        };

        let Ok(version) = library.version.parse::<Version>() else {
            panic!("invalid sqlite3 library version: {}", library.version);
        };

        if !sqlite3_version_req.matches(&version) {
            panic!(
                "system sqlite3 library version {} does not match required version {}",
                library.version, sqlite3_version_req
            );
        }

        return;
    };

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

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=source/sqlite3.c");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_FAMILY");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");

    if let Ok(target_family) = env::var("CARGO_CFG_TARGET_FAMILY")
        && let Ok(target_os) = env::var("CARGO_CFG_TARGET_OS")
        && target_family == "wasm"
    {
        let mut sdk_path = PathBuf::from(env(SDK_PATH_ENV));

        sdk_path.push("bin");
        sdk_path.push("clang");

        if !sdk_path.is_file() {
            panic!("not a file: {}", sdk_path.display());
        }

        build.compiler(sdk_path);

        if target_os != "wasi" {
            let target_env = env(WASI_TARGET_ENV);
            let target = format!("wasm32-wasi{}", target_env.to_string_lossy());
            build.target(&target);
        }

        build.define("__wasi__", None);
        build.define("SQLITE_OMIT_LOAD_EXTENSION", "1");
        build.define("SQLITE_THREADSAFE", "0");
        build.flag("-Wno-unused");
        build.flag("-Wno-unused-parameter");
    }

    build.define("NDEBUG", "1");
    build.compile("libsqlite3.a");
}
