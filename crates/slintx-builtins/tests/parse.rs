use slintx_syntax::ast::{self, Item};
// The wasm32 runner only sees `#[wasm_bindgen_test]`; natively `#[test]` stays.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;

#[test]
fn the_catalog_parses_cleanly_and_round_trips() {
    let parse = slintx_builtins::parse();
    assert_eq!(parse.diagnostics(), []);
    assert_eq!(parse.syntax().to_string(), slintx_builtins::SOURCE);
}

#[test]
fn every_top_level_name_is_declared_once() {
    let parse = slintx_builtins::parse();
    let mut names: Vec<String> = parse
        .tree::<ast::File>()
        .into_iter()
        .flat_map(|file| file.items())
        .map(|item| match item {
            Item::Export(export) => export.declaration(),
            item => Some(item),
        })
        .filter_map(|item| match item? {
            Item::Component(c) => c.name(),
            Item::Global(g) => g.name(),
            Item::Struct(s) => s.name(),
            Item::Enum(e) => e.name(),
            Item::Import(_) | Item::Export(_) => None,
        })
        .map(|t| t.text().to_owned())
        .collect();
    let count = names.len();
    assert!(count > 80, "{count} names");
    names.sort();
    names.dedup();
    assert_eq!(names.len(), count, "duplicated names");
}
