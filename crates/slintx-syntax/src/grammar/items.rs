//! Top-level items: imports, exports, components, globals, structs, enums.

use super::element::element_body;
use super::expr::expr;
use super::macros::skip_parens;
use super::stmt::{qualified_name, type_};
use crate::SyntaxKind as K;
use crate::parser::Parser;

pub(crate) const ITEM_KEYWORDS: &[&str] =
    &["import", "export", "component", "global", "struct", "enum"];

pub(crate) fn item(p: &mut Parser) {
    let checkpoint = p.checkpoint();
    rust_attrs(p);
    if p.at_keyword("import") {
        import(p);
    } else if p.at_keyword("export") {
        export(p, checkpoint);
    } else if !declaration(p, checkpoint) {
        recover_item(p);
    }
}

/// `@rust-attr(...)` before a struct or enum; its content is opaque here.
fn rust_attrs(p: &mut Parser) {
    while p.at(K::At) && p.nth_keyword(1, "rust-attr") {
        p.start(K::RustAttr);
        p.bump();
        p.bump();
        if p.at(K::LParen) {
            skip_parens(p);
        } else {
            p.error("expected `(`");
        }
        p.finish();
    }
}

/// Skips to the next item keyword, outside any brackets, as one error.
fn recover_item(p: &mut Parser) {
    p.error("expected an item: import, export, component, global, struct or enum");
    p.start(K::ErrorNode);
    let mut depth = 0u32;
    while let Some(kind) = p.nth(0) {
        match kind {
            K::LBrace | K::LParen | K::LBracket => depth += 1,
            K::RBrace | K::RParen | K::RBracket => depth = depth.saturating_sub(1),
            _ => {}
        }
        p.bump();
        if depth == 0 && (p.at(K::At) || ITEM_KEYWORDS.iter().any(|k| p.at_keyword(k))) {
            break;
        }
    }
    p.finish();
}

fn import(p: &mut Parser) {
    p.start(K::Import);
    p.bump();
    if p.at(K::LBrace) {
        specifiers(p, K::ImportSpecifier);
        p.expect_keyword("from");
        p.expect(K::String, "a file path string");
    } else {
        p.expect(K::String, "`{` or a file path string");
    }
    p.expect(K::Semicolon, "`;`");
    p.finish();
}

/// `{ A, B as C, }`.
fn specifiers(p: &mut Parser, kind: K) {
    p.bump();
    while p.at(K::Ident) {
        p.start(kind);
        p.bump();
        if p.at_keyword("as") {
            p.bump();
            p.expect(K::Ident, "a name");
        }
        p.finish();
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace, "`}`");
}

fn export(p: &mut Parser, checkpoint: rowan::Checkpoint) {
    p.start_at(checkpoint, K::Export);
    p.bump();
    if p.at(K::LBrace) {
        specifiers(p, K::ExportSpecifier);
        if p.at_keyword("from") {
            p.bump();
            p.expect(K::String, "a file path string");
            p.expect(K::Semicolon, "`;`");
        } else {
            // Slint takes a local export list with or without the `;`.
            p.eat(K::Semicolon);
        }
    } else if p.eat(K::Star) {
        p.expect_keyword("from");
        p.expect(K::String, "a file path string");
        p.expect(K::Semicolon, "`;`");
    } else {
        let inner = p.checkpoint();
        rust_attrs(p);
        if !declaration(p, inner) {
            p.error("expected `{`, `*`, or a component, global, struct or enum");
        }
    }
    p.finish();
}

/// Component, global, struct or enum; false if none starts here. The node
/// starts at `checkpoint`, so attributes before it become its children.
fn declaration(p: &mut Parser, checkpoint: rowan::Checkpoint) -> bool {
    let kind = [
        ("component", K::Component),
        ("global", K::Global),
        ("struct", K::Struct),
        ("enum", K::Enum),
    ]
    .into_iter()
    .find_map(|(keyword, kind)| p.at_keyword(keyword).then_some(kind));
    let Some(kind) = kind else {
        // The pre-1.0 `Name := Base { }` component, which Slint still accepts.
        if !(p.at(K::Ident) && p.nth(1) == Some(K::ColonEqual)) {
            return false;
        }
        p.start_at(checkpoint, K::Component);
        p.bump();
        p.bump();
        qualified_name(p);
        element_body(p);
        p.finish();
        return true;
    };
    p.start_at(checkpoint, kind);
    p.bump();
    p.expect(K::Ident, "a name");
    match kind {
        K::Component => {
            if p.at_keyword("inherits") {
                p.bump();
                qualified_name(p);
            }
            element_body(p);
        }
        K::Global => element_body(p),
        K::Struct => struct_body(p),
        _ => enum_body(p),
    }
    p.finish();
    true
}

fn struct_body(p: &mut Parser) {
    if !p.expect(K::LBrace, "`{`") {
        return;
    }
    while p.at(K::Ident) {
        p.start(K::StructField);
        p.bump();
        p.expect(K::Colon, "`:`");
        type_(p);
        if p.eat(K::Equal) {
            expr(p);
        }
        p.finish();
        if p.at(K::Semicolon) {
            p.error_bump("struct fields are separated by `,`, not `;`");
        } else if !p.eat(K::Comma) {
            break;
        }
    }
    close(p, K::RBrace, "`}`");
}

fn enum_body(p: &mut Parser) {
    if !p.expect(K::LBrace, "`{`") {
        return;
    }
    while p.at(K::Ident) {
        p.start(K::EnumValue);
        p.bump();
        p.finish();
        if !p.eat(K::Comma) {
            break;
        }
    }
    close(p, K::RBrace, "`}`");
}

/// Expects `kind`; on anything else, reports once and skips to it (outside
/// nested brackets), stopping early at a closer that belongs to a parent.
pub(crate) fn close(p: &mut Parser, kind: K, what: &str) {
    if p.eat(kind) {
        return;
    }
    p.error(format!("expected {what}"));
    let mut depth = 0u32;
    let mut skipped = false;
    while let Some(next) = p.nth(0) {
        match next {
            _ if depth == 0 && next == kind => break,
            K::LBrace | K::LParen | K::LBracket => depth += 1,
            K::RBrace | K::RParen | K::RBracket if depth == 0 => break,
            K::RBrace | K::RParen | K::RBracket => depth -= 1,
            _ => {}
        }
        if !skipped {
            p.start(K::ErrorNode);
            skipped = true;
        }
        p.bump();
    }
    if skipped {
        p.finish();
    }
    p.eat(kind);
}
