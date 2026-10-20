//! Types, code blocks and statements.

use super::expr::expr;
use crate::SyntaxKind as K;
use crate::parser::Parser;

pub(crate) fn type_(p: &mut Parser) {
    if !p.enter() {
        return;
    }
    match p.nth(0) {
        Some(K::LBracket) => {
            p.start(K::ArrayType);
            p.bump();
            type_(p);
            p.expect(K::RBracket, "`]`");
            p.finish();
        }
        Some(K::LBrace) => {
            p.start(K::ObjectType);
            p.bump();
            while p.at(K::Ident) {
                p.start(K::ObjectTypeMember);
                p.bump();
                p.expect(K::Colon, "`:`");
                type_(p);
                p.finish();
                if !p.eat(K::Comma) {
                    break;
                }
            }
            p.expect(K::RBrace, "`}`");
            p.finish();
        }
        Some(K::Ident) => {
            p.start(K::NamedType);
            qualified_name(p);
            p.finish();
        }
        _ => p.error("expected a type"),
    }
    p.leave();
}

pub(crate) fn qualified_name(p: &mut Parser) {
    p.start(K::QualifiedName);
    p.expect(K::Ident, "a name");
    while p.at(K::Dot) && p.nth(1) == Some(K::Ident) {
        p.bump();
        p.bump();
    }
    p.finish();
}

pub(crate) fn code_block(p: &mut Parser) {
    p.start(K::CodeBlock);
    // Without `{` this is no block: parsing statements would eat the rest.
    if !p.expect(K::LBrace, "`{`") {
        p.finish();
        return;
    }
    if p.enter() {
        while !p.at(K::RBrace) && !p.at_end() {
            statement(p);
        }
        p.leave();
    }
    p.expect(K::RBrace, "`}`");
    p.finish();
}

fn statement(p: &mut Parser) {
    // `if` is an identifier when an expression would continue after it.
    let if_statement_here = p.at_keyword("if")
        && !matches!(
            p.nth(1),
            Some(K::Dot | K::Comma | K::Semicolon | K::RBrace | K::RBracket | K::RParen)
        );
    if p.eat(K::Semicolon) {
    } else if p.at_keyword("let") && p.nth(1) == Some(K::Ident) {
        p.start(K::LetStatement);
        p.bump();
        p.bump();
        if p.eat(K::Colon) {
            type_(p);
        }
        p.expect(K::Equal, "`=`");
        expr(p);
        semicolon(p);
        p.finish();
    } else if p.at_keyword("return") {
        p.start(K::ReturnStatement);
        p.bump();
        if !matches!(p.nth(0), None | Some(K::Semicolon | K::RBrace)) {
            expr(p);
        }
        semicolon(p);
        p.finish();
    } else if if_statement_here {
        if_statement(p);
    } else {
        expr_statement(p);
    }
}

fn if_statement(p: &mut Parser) {
    if !p.enter() {
        return;
    }
    p.start(K::IfStatement);
    p.bump();
    expr(p);
    code_block(p);
    if p.at_keyword("else") {
        p.bump();
        if p.at_keyword("if") {
            if_statement(p);
        } else {
            code_block(p);
        }
    }
    p.finish();
    p.leave();
}

/// An expression or an assignment, then `;` (optional before `}`). Also the
/// body of `name => expr` handlers.
pub(crate) fn expr_statement(p: &mut Parser) {
    p.start(K::ExprStatement);
    let checkpoint = p.checkpoint();
    let before = p.offset();
    expr(p);
    let assign = matches!(
        p.nth(0),
        Some(K::Equal | K::PlusEqual | K::MinusEqual | K::StarEqual | K::SlashEqual)
    );
    if assign || p.at(K::ColonEqual) {
        if !assign {
            p.error("`:=` cannot declare a local variable; use `let`");
        }
        p.start_at(checkpoint, K::AssignmentExpr);
        p.bump();
        expr(p);
        p.finish();
    } else if p.offset() == before && !matches!(p.nth(0), None | Some(K::Semicolon | K::RBrace)) {
        // `primary` already reported this token; consume it to make progress.
        p.bump_as_error();
        p.finish();
        return;
    }
    semicolon(p);
    p.finish();
}

fn semicolon(p: &mut Parser) {
    if !p.eat(K::Semicolon) && !p.at(K::RBrace) {
        p.error("expected `;`");
    }
}
