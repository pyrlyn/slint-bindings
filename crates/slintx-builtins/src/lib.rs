//! Slint 1.18's built-in elements, structs, enums and namespaces as Slint
//! declarations, written from the docs. Name resolution loads [`SOURCE`] like
//! any other file: its exported items are the names user code sees, the rest
//! are helpers (shared bases, and the functions callable without a namespace).
//! slintx-conformance checks every name and type in it against Slint.

/// The catalog's text.
pub const SOURCE: &str = include_str!("builtins.slint");

/// The catalog's syntax tree.
pub fn parse() -> slintx_syntax::Parse {
    slintx_syntax::parse(SOURCE)
}
