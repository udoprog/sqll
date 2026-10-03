# sqll-sys

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/sqll-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/sqll)
[<img alt="crates.io" src="https://img.shields.io/crates/v/sqll-sys.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/sqll-sys)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-sqll--sys-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/sqll-sys)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/sqll/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/sqll/actions?query=branch%3Amain)

Bindings to [sqlite] for [sqll].

Note that the metadata field of this crate specified which version of sqlite
is provided *if* the `bundled` feature is enabled, like `+sqlite-3.53.4`.

<br>

## Features

* `bundled` - Use the bundled sqlite3 source code. If this feature is not
  enabled see the building with system dependencies section below.
* `threadsafe` - Build sqlite3 with threadsafe support. If this is not set
  then the `bundled` feature has to be set since we otherwise cannot control
  how sqlite is built.
* `strict` - Build sqlite3 with strict compiler flags enabled. This is only
  used when the `bundled` feature is enabled.

<br>

## Building

When linking to a system sqlite library there is a minimum required version.
This is specified as `minimum` in the [`versions`] file.

If the `bundled` feature is not set, this will attempt to find the native
sqlite3 library using the following methods, in order:
* Finding the library through `pkg-config`. This also checks that the found
  library is at least the minimum required version. It can be disabled by
  setting the `SQLITE3_NO_PKG_CONFIG` environment variable.
* Calling `vcpkg`. The version of a library found this way is not checked.
  It can be disabled by setting the `VCPKGRS_DISABLE`, `NO_VCPKG` or
  `SQLITE3_NO_VCPKG` environment variables.

<br>

## Building under WASM

This is only supported when the `bundled` feature is enabled. When the
target family is `wasm`, sqlite is always built with `SQLITE_THREADSAFE=0`
and `SQLITE_OMIT_LOAD_EXTENSION=1`.

The following environment variables can be set to modify how the bundled
sqlite is compiled:
* `SQLL_TARGET` or `TARGET` to specify the target passed to the C compiler.
  Cargo sets `TARGET` to the target being built, so `SQLL_TARGET` is only
  needed to override it, such as with `wasm32-wasip1`.
* `SQLL_CLANG_PATH` or `CLANG_PATH` to specify the root of a custom clang
  installation, such as a WASI SDK. The compiler used is `bin/clang` under
  that path.

[`versions`]: https://github.com/udoprog/sqll/blob/main/sqll-sys/versions
[sqlite]: https://www.sqlite.org
[sqll]: https://docs.rs/sqll
