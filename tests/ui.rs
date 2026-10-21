//! Compile-fail tests for the derive macros.
//!
//! These assert the diagnostics emitted for misuse of the `#[sql(...)]`
//! attributes, including that a `read_only` `Statements` collection cannot
//! embed a nested collection that is not itself read-only, and the shapes the
//! `FromColumn` and `BindValue` derives reject (the `value_*` cases).

#[cfg_attr(miri, ignore)]
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
