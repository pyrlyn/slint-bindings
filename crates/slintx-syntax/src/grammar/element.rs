//! Element bodies: child elements, bindings and callback handlers.

use super::decl::keyword_member;
use super::expr::{binding_value, expr};
use super::items::ITEM_KEYWORDS;
use super::stmt::{code_block, expr_statement, qualified_name};
use crate::SyntaxKind as K;
use crate::parser::Parser;

pub(crate) fn element_body(p: &mut Parser) {
    p.start(K::ElementBody);
    if p.expect(K::LBrace, "`{`") {
        if p.enter() {
            while !p.at(K::RBrace) && !p.at_end() && !at_item(p) {
                member(p);
            }
            p.leave();
        }
        p.expect(K::RBrace, "`}`");
    }
    p.finish();
}

/// An item keyword where no member can start: the body was left unclosed,
/// and the item belongs to the file, not to the body.
fn at_item(p: &Parser) -> bool {
    let next = p.nth(1);
    match next {
        Some(K::Ident) => ITEM_KEYWORDS.iter().any(|k| p.at_keyword(k)),
        Some(K::LBrace) => p.at_keyword("import") || p.at_keyword("export"),
        Some(K::Star) => p.at_keyword("export"),
        Some(K::String) => p.at_keyword("import"),
        _ => false,
    }
}

fn member(p: &mut Parser) {
    match (p.nth(0), p.nth(1)) {
        (Some(K::At), _) if p.nth_keyword(1, "children") => {
            p.start(K::ChildrenPlaceholder);
            p.bump();
            p.bump();
            p.finish();
        }
        (Some(K::Ident), Some(K::ColonEqual | K::LBrace)) => element(p),
        (Some(K::Ident), Some(K::Colon)) => binding(p),
        (Some(K::Ident), Some(K::DoubleArrow)) => {
            p.start(K::TwoWayBinding);
            p.bump();
            p.bump();
            expr(p);
            p.expect(K::Semicolon, "`;`");
            p.finish();
        }
        (Some(K::Ident), Some(K::FatArrow)) => callback_connection(p),
        (Some(K::Ident), Some(K::LParen)) if is_connection(p) => callback_connection(p),
        (Some(K::Ident), _) => keyword_member(p),
        (Some(K::Semicolon), _) => p.error_bump("unexpected `;`: no `;` is needed after `}`"),
        _ => recover_member(p),
    }
}

/// Skips one unrecognised member, up to its `;` or the body's `}`.
pub(crate) fn recover_member(p: &mut Parser) {
    p.error("expected an element, a binding or a declaration");
    p.start(K::ErrorNode);
    let mut depth = 0u32;
    while let Some(kind) = p.nth(0) {
        match kind {
            K::RBrace if depth == 0 => break,
            K::Semicolon if depth == 0 => {
                p.bump();
                break;
            }
            K::LBrace | K::LParen | K::LBracket => depth += 1,
            K::RBrace | K::RParen | K::RBracket => depth = depth.saturating_sub(1),
            _ => {}
        }
        p.bump();
    }
    p.finish();
}

/// `name(a, b) =>`; the scan is capped so huge argument lists stay linear.
fn is_connection(p: &Parser) -> bool {
    for i in 2..256 {
        match p.nth(i) {
            Some(K::Ident | K::Comma) => {}
            Some(K::RParen) => return p.nth(i + 1) == Some(K::FatArrow),
            _ => return false,
        }
    }
    false
}

pub(crate) fn element(p: &mut Parser) {
    p.start(K::Element);
    if p.nth(1) == Some(K::ColonEqual) {
        p.bump();
        p.bump();
    }
    qualified_name(p);
    element_body(p);
    p.finish();
}

pub(crate) fn binding(p: &mut Parser) {
    p.start(K::Binding);
    p.bump();
    p.bump();
    binding_tail(p);
    p.finish();
}

/// After `:`: the value, then `;`, which a code block may omit.
pub(crate) fn binding_tail(p: &mut Parser) {
    p.start(K::BindingExpression);
    let block = binding_value(p);
    p.finish();
    if block {
        p.eat(K::Semicolon);
    } else {
        p.expect(K::Semicolon, "`;`");
    }
}

pub(crate) fn handler_body(p: &mut Parser) {
    if p.at(K::LBrace) {
        code_block(p);
        p.eat(K::Semicolon);
    } else {
        expr_statement(p);
    }
}

fn callback_connection(p: &mut Parser) {
    p.start(K::CallbackConnection);
    p.bump();
    if p.eat(K::LParen) {
        while p.eat(K::Ident) && p.eat(K::Comma) {}
        p.expect(K::RParen, "`)`");
    }
    p.expect(K::FatArrow, "`=>`");
    handler_body(p);
    p.finish();
}
