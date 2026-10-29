//! Compile-time tests: valid declarations build, invalid ones fail with the
//! diagnostics recorded in `tests/ui/fail/*.stderr`.
//!
//! Regenerate expectations with `TRYBUILD=overwrite cargo test --test ui`.

#[test]
fn ui() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/*.rs");
    cases.compile_fail("tests/ui/fail/*.rs");
}
