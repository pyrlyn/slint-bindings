//! Expressions: operators by precedence, postfix chains and literals.

use super::macros::{macro_call, string_literal, string_template};
use super::stmt::code_block;
use crate::SyntaxKind as K;
use crate::parser::Parser;

/// What follows `name:` in a binding: a code block, or an expression. A
/// `{` opens an object literal only when `name:` follows it. Returns whether
/// it was a block, since a block needs no `;` after it.
pub(crate) fn binding_value(p: &mut Parser) -> bool {
    let block = p.at(K::LBrace) && !(p.nth(1) == Some(K::Ident) && p.nth(2) == Some(K::Colon));
    if block {
        code_block(p);
    } else {
        expr(p);
    }
    block
}

/// Ternary, the loosest level; right-associative.
pub(crate) fn expr(p: &mut Parser) {
    if !p.enter() {
        return;
    }
    let checkpoint = p.checkpoint();
    binary(p, 0);
    if p.at(K::Question) {
        p.start_at(checkpoint, K::ConditionalExpr);
        p.bump();
        expr(p);
        p.expect(K::Colon, "`:`");
        expr(p);
        p.finish();
    }
    p.leave();
}

fn level(kind: K) -> Option<u8> {
    match kind {
        K::AndAnd | K::OrOr => Some(0),
        K::EqualEqual | K::NotEqual | K::Less | K::Greater | K::LessEqual | K::GreaterEqual => {
            Some(1)
        }
        K::Plus | K::Minus => Some(2),
        K::Star | K::Slash => Some(3),
        _ => None,
    }
}

/// Binary operators from loosest (0) to tightest (3). Sums and products are
/// left-associative; comparisons do not chain and `&&` does not mix with
/// `||`. Both still parse, with a diagnostic, so one typo costs one error.
fn binary(p: &mut Parser, current: u8) {
    let operand = |p: &mut Parser| {
        if current == 3 {
            unary(p)
        } else {
            binary(p, current + 1)
        }
    };
    let checkpoint = p.checkpoint();
    operand(p);
    let mut previous = None;
    let mut levels = 0;
    while let Some(op) = p.nth(0).filter(|&k| level(k) == Some(current)) {
        match (current, previous) {
            (0, Some(prev)) if prev != op => {
                p.error("`&&` and `||` cannot be mixed without parentheses");
            }
            (1, Some(_)) => p.error("comparison operators cannot be chained; add parentheses"),
            _ => {}
        }
        previous = Some(op);
        // A long chain nests one node per operator, so it counts as depth.
        if !p.enter() {
            break;
        }
        levels += 1;
        p.start_at(checkpoint, K::BinaryExpr);
        p.bump();
        operand(p);
        p.finish();
    }
    (0..levels).for_each(|_| p.leave());
}

fn unary(p: &mut Parser) {
    if !matches!(p.nth(0), Some(K::Bang | K::Minus | K::Plus)) {
        return postfix(p);
    }
    if p.enter() {
        p.start(K::UnaryExpr);
        p.bump();
        unary(p);
        p.finish();
        p.leave();
    }
}

fn postfix(p: &mut Parser) {
    let checkpoint = p.checkpoint();
    if !primary(p) {
        return;
    }
    let mut levels = 0;
    while let Some(kind @ (K::Dot | K::LParen | K::LBracket)) = p.nth(0) {
        if !p.enter() {
            break;
        }
        levels += 1;
        match kind {
            K::Dot => {
                p.start_at(checkpoint, K::FieldAccess);
                p.bump();
                p.expect(K::Ident, "a member name");
            }
            K::LParen => {
                p.start_at(checkpoint, K::CallExpr);
                arg_list(p);
            }
            _ => {
                p.start_at(checkpoint, K::IndexExpr);
                p.bump();
                expr(p);
                p.expect(K::RBracket, "`]`");
            }
        }
        p.finish();
    }
    (0..levels).for_each(|_| p.leave());
}

/// Returns false, after a diagnostic, when no expression starts here.
pub(crate) fn primary(p: &mut Parser) -> bool {
    match p.nth(0) {
        Some(K::Number | K::Color) => node(p, K::Literal),
        Some(K::String) => string_literal(p),
        Some(K::StringStart) => string_template(p),
        Some(K::Ident) => node(p, K::NameRef),
        Some(K::LParen) => {
            p.start(K::ParenExpr);
            p.bump();
            expr(p);
            p.expect(K::RParen, "`)`");
            p.finish();
        }
        Some(K::LBracket) => array_literal(p),
        Some(K::LBrace) => object_literal(p),
        Some(K::At) => macro_call(p),
        // Leave closing tokens to the construct that owns them.
        None | Some(K::Semicolon | K::RBrace | K::RParen | K::RBracket | K::Comma | K::Colon) => {
            p.error("expected an expression");
            return false;
        }
        Some(_) => {
            p.error_bump("expected an expression");
            return false;
        }
    }
    true
}

pub(crate) fn node(p: &mut Parser, kind: K) {
    p.start(kind);
    p.bump();
    p.finish();
}

fn array_literal(p: &mut Parser) {
    p.start(K::ArrayLiteral);
    p.bump();
    while !p.at(K::RBracket) && !p.at_end() {
        expr(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBracket, "`]`");
    p.finish();
}

fn object_literal(p: &mut Parser) {
    p.start(K::ObjectLiteral);
    p.bump();
    while p.at(K::Ident) {
        p.start(K::ObjectMember);
        p.bump();
        p.expect(K::Colon, "`:`");
        expr(p);
        p.finish();
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace, "`}`");
    p.finish();
}

fn arg_list(p: &mut Parser) {
    p.start(K::ArgList);
    p.bump();
    while !p.at(K::RParen) && !p.at_end() {
        expr(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RParen, "`)`");
    p.finish();
}
