//! The `slint-convert` command line: arguments, output, warnings and exit
//! codes. Each case is a `tests/cmd/*.toml` file, with the expected output
//! beside it; the inputs are the shared fixtures.

#[test]
fn cli() {
    trycmd::TestCases::new().case("tests/cmd/*.toml");
}
