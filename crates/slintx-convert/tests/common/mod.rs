//! Fixtures shared by the integration tests.

#![allow(dead_code)]

pub const FIXTURES: &[&str] = &[
    "bindings",
    "broken",
    "components",
    "nested_layouts",
    "properties",
    "repeat_if",
    "stretch",
    "unicode",
    "unsupported",
    "widgets",
];

/// Fixtures that parse; `broken` exists to show the parse-error path.
pub fn valid() -> impl Iterator<Item = &'static str> {
    FIXTURES.iter().copied().filter(|n| *n != "broken")
}

pub fn source(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}.slint", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| unreachable!("{path}: {e}"))
}
