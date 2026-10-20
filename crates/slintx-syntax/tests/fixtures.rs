//! Fixture corpus: valid files round-trip with no diagnostics; broken files
//! round-trip and produce exactly the diagnostics in their `.expected` file.
//! Fixtures are embedded with `include_str!` so the tests also run on wasm32,
//! which has no filesystem.

mod common;

use slintx_syntax::{SyntaxKind, parse};

macro_rules! fixtures {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("fixtures/", $name, ".slint")))),*]
    };
}

macro_rules! error_fixtures {
    ($($name:literal),* $(,)?) => {
        &[$((
            $name,
            include_str!(concat!("fixtures/errors/", $name, ".slint")),
            include_str!(concat!("fixtures/errors/", $name, ".expected")),
        )),*]
    };
}

const VALID: &[(&str, &str)] = fixtures![
    "animate",
    "callbacks",
    "changed",
    "children",
    "component",
    "conditional",
    "empty",
    "enum",
    "exports",
    "expressions",
    "functions",
    "global",
    "imports",
    "lexical",
    "macros",
    "properties",
    "repeater",
    "statements",
    "states",
    "strings",
    "struct",
    "types",
];

const BROKEN: &[(&str, &str, &str)] = error_fixtures![
    "bad-color",
    "bad-escape",
    "bad-item",
    "bad-member",
    "chained-comparison",
    "interpolation",
    "let-colon-equal",
    "missing-body",
    "missing-semicolon",
    "mixed-logical",
    "stray-semicolon",
    "struct-semicolon",
    "unbalanced",
    "unclosed-element",
    "unexpected-characters",
    "unknown-macro",
    "unterminated-comment",
    "unterminated-string",
];

/// Top-level item kinds, then one diagnostic per line: what an `.expected`
/// file holds. The item line shows how far recovery got.
fn report(source: &str) -> String {
    let parse = parse(source);
    let items: Vec<String> = parse
        .syntax()
        .children()
        .map(|n| format!("{:?}", n.kind()))
        .collect();
    let mut out = format!("items: {}\n", items.join(" "));
    for message in common::messages(&parse) {
        out += &message;
        out.push('\n');
    }
    out
}

#[test]
fn valid_fixtures_round_trip_without_diagnostics() {
    for (name, source) in VALID {
        let parse = parse(source);
        assert_eq!(
            parse.syntax().to_string(),
            *source,
            "{name} does not round-trip"
        );
        assert!(
            parse.diagnostics().is_empty(),
            "{name}: {:#?}",
            common::messages(&parse)
        );
        let errors = parse
            .syntax()
            .descendants()
            .filter(|n| n.kind() == SyntaxKind::ErrorNode);
        assert_eq!(errors.count(), 0, "{name} has error nodes");
    }
}

#[test]
fn broken_fixtures_round_trip_with_expected_diagnostics() {
    for (name, source, expected) in BROKEN {
        assert_eq!(
            parse(source).syntax().to_string(),
            *source,
            "{name} does not round-trip"
        );
        let actual = report(source);
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var_os("SLINTX_BLESS").is_some() {
            let path = format!(
                "{}/tests/fixtures/errors/{name}.expected",
                env!("CARGO_MANIFEST_DIR")
            );
            std::fs::write(path, &actual).unwrap_or_else(|e| unreachable!("{e}"));
            continue;
        }
        assert!(actual.lines().count() > 1, "{name} produced no diagnostics");
        assert_eq!(
            actual, *expected,
            "{name}: rerun with SLINTX_BLESS=1 to update"
        );
    }
}

/// Every construct has at least one fixture: together the valid fixtures
/// contain every node kind except the error node.
#[test]
fn valid_fixtures_cover_every_node_kind() {
    let mut seen = std::collections::BTreeSet::new();
    for (_, source) in VALID {
        seen.extend(parse(source).syntax().descendants().map(|n| n.kind()));
    }
    let missing: Vec<SyntaxKind> = (0..)
        .map_while(SyntaxKind::from_raw)
        .filter(|k| !k.is_token() && *k != SyntaxKind::ErrorNode && !seen.contains(k))
        .collect();
    assert!(missing.is_empty(), "no fixture contains {missing:?}");
}

/// Span test: trivia sits between nodes, never at a node's edge, so a node's
/// range is exactly its source text from first to last significant token.
#[test]
fn nodes_start_and_end_at_significant_tokens() {
    for (name, source, _) in BROKEN
        .iter()
        .copied()
        .chain(VALID.iter().map(|&(n, s)| (n, s, "")))
    {
        let root = parse(source).syntax();
        for node in root.descendants().skip(1) {
            let edge = |token: Option<slintx_syntax::SyntaxToken>| {
                token.is_some_and(|t| t.kind().is_trivia())
            };
            assert!(
                !edge(node.first_token()) && !edge(node.last_token()),
                "{name}: {:?} at {:?} has trivia at its edge",
                node.kind(),
                node.text_range()
            );
            assert_eq!(node.text().to_string(), source[node.text_range()], "{name}");
        }
        assert_eq!(usize::from(root.text_range().end()), source.len(), "{name}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn every_fixture_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let names = |dir: std::path::PathBuf| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                e.file_name()
                    .to_str()?
                    .strip_suffix(".slint")
                    .map(str::to_owned)
            })
            .collect();
        names.sort();
        names
    };
    let valid: Vec<&str> = VALID.iter().map(|(n, _)| *n).collect();
    let broken: Vec<&str> = BROKEN.iter().map(|(n, _, _)| *n).collect();
    assert_eq!(names(dir.join("")), valid);
    assert_eq!(names(dir.join("errors")), broken);
}
