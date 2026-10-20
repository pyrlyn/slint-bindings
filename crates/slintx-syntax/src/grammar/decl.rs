//! Declarations in element bodies: properties, callbacks, functions,
//! `changed`, `animate`, `states`, `for` and `if`.

use super::element::{binding, binding_tail, element, handler_body, recover_member};
use super::expr::expr;
use super::items::close;
use super::stmt::{code_block, qualified_name, type_};
use crate::SyntaxKind as K;
use crate::parser::Parser;

pub(crate) fn keyword_member(p: &mut Parser) {
    let visibility = ["in", "out", "in-out", "private"]
        .iter()
        .any(|v| p.at_keyword(v));
    let modifiers = (0..3)
        .take_while(|&i| {
            ["pure", "public", "protected"]
                .iter()
                .any(|m| p.nth_keyword(i, m))
        })
        .count();
    if p.at_keyword("property") || (visibility && p.nth_keyword(1, "property")) {
        property(p);
    } else if p.nth_keyword(modifiers, "callback") || p.nth_keyword(modifiers, "function") {
        callable(p);
    } else if p.at_keyword("changed") && p.nth(1) == Some(K::Ident) {
        p.start(K::ChangedHandler);
        p.bump();
        p.bump();
        p.expect(K::FatArrow, "`=>`");
        handler_body(p);
        p.finish();
    } else if p.at_keyword("animate") && matches!(p.nth(1), Some(K::Ident | K::Star)) {
        animate(p);
    } else if p.at_keyword("states") && p.nth(1) == Some(K::LBracket) {
        states(p);
    } else if p.at_keyword("for") {
        repeater(p);
    } else if p.at_keyword("if") {
        p.start(K::Conditional);
        p.bump();
        expr(p);
        p.expect(K::Colon, "`:`");
        sub_element(p);
        p.finish();
    } else {
        recover_member(p);
    }
}

/// `[in|out|in-out|private] property <type> name [: value | <=> target];`
fn property(p: &mut Parser) {
    p.start(K::PropertyDeclaration);
    if !p.at_keyword("property") {
        p.bump();
    }
    p.bump();
    if p.eat(K::Less) {
        type_(p);
        p.expect(K::Greater, "`>`");
    }
    p.expect(K::Ident, "a property name");
    if p.eat(K::Colon) {
        binding_tail(p);
    } else if p.eat(K::DoubleArrow) {
        expr(p);
        p.expect(K::Semicolon, "`;`");
    } else {
        p.expect(K::Semicolon, "`;`");
    }
    p.finish();
}

/// Callbacks and functions with their `pure`/`public`/`protected` modifiers.
fn callable(p: &mut Parser) {
    let checkpoint = p.checkpoint();
    while ["pure", "public", "protected"]
        .iter()
        .any(|m| p.at_keyword(m))
    {
        p.bump();
    }
    let callback = p.at_keyword("callback");
    p.start_at(
        checkpoint,
        if callback {
            K::CallbackDeclaration
        } else {
            K::FunctionDeclaration
        },
    );
    p.bump();
    p.expect(K::Ident, "a name");
    if callback {
        if p.eat(K::LParen) {
            while !p.at(K::RParen) && !p.at_end() {
                p.start(K::CallbackParam);
                if p.at(K::Ident) && p.nth(1) == Some(K::Colon) {
                    p.bump();
                    p.bump();
                }
                type_(p);
                p.finish();
                if !p.eat(K::Comma) {
                    break;
                }
            }
            p.expect(K::RParen, "`)`");
        }
    } else {
        p.expect(K::LParen, "`(`");
        while p.at(K::Ident) {
            p.start(K::Param);
            p.bump();
            p.expect(K::Colon, "`:`");
            type_(p);
            p.finish();
            if !p.eat(K::Comma) {
                break;
            }
        }
        p.expect(K::RParen, "`)`");
    }
    if p.eat(K::Arrow) {
        type_(p);
    }
    if !callback {
        if p.at(K::LBrace) {
            code_block(p);
        } else if p.at(K::Semicolon) {
            // `function f();` reads as a forward declaration, which Slint
            // has none of; one diagnostic covers the missing body.
            p.error_bump("expected a function body `{ ... }`");
        } else {
            p.error("expected a function body `{ ... }`");
        }
    } else if p.eat(K::DoubleArrow) {
        expr(p);
        p.expect(K::Semicolon, "`;`");
    } else {
        p.expect(K::Semicolon, "`;`");
    }
    p.finish();
}

/// `animate a, b.c { duration: 1s; }` or, in a transition, `animate * {}`.
fn animate(p: &mut Parser) {
    p.start(K::Animate);
    p.bump();
    if !p.eat(K::Star) {
        qualified_name(p);
        while p.eat(K::Comma) {
            qualified_name(p);
        }
    }
    if p.expect(K::LBrace, "`{`") {
        while p.at(K::Ident) && p.nth(1) == Some(K::Colon) {
            binding(p);
        }
        close(p, K::RBrace, "`}`");
    }
    p.finish();
}

fn states(p: &mut Parser) {
    p.start(K::States);
    p.bump();
    p.bump();
    while p.at(K::Ident) {
        state(p);
    }
    close(p, K::RBracket, "`]`");
    p.finish();
}

/// `name [when cond] : { prop: value; in { animate ... } }`.
fn state(p: &mut Parser) {
    p.start(K::State);
    p.bump();
    if p.at_keyword("when") {
        p.bump();
        expr(p);
    }
    p.expect(K::Colon, "`:`");
    if p.expect(K::LBrace, "`{`") {
        loop {
            let transition = ["in", "out", "in-out"].iter().any(|k| p.at_keyword(k))
                && p.nth(1) == Some(K::LBrace);
            if transition {
                p.start(K::Transition);
                p.bump();
                p.bump();
                while p.at_keyword("animate") {
                    animate(p);
                }
                close(p, K::RBrace, "`}`");
                p.finish();
            } else if p.at(K::Ident) {
                p.start(K::StatePropertyChange);
                qualified_name(p);
                p.expect(K::Colon, "`:`");
                binding_tail(p);
                p.finish();
            } else {
                break;
            }
        }
        close(p, K::RBrace, "`}`");
    }
    p.finish();
}

/// `for name[index] in model : [id :=] Element { }`; name and index optional.
fn repeater(p: &mut Parser) {
    p.start(K::Repeater);
    p.bump();
    if p.at(K::Ident) && !p.at_keyword("in") {
        p.bump();
    }
    if p.at(K::LBracket) {
        p.start(K::RepeaterIndex);
        p.bump();
        p.expect(K::Ident, "an index name");
        p.expect(K::RBracket, "`]`");
        p.finish();
    }
    p.expect_keyword("in");
    expr(p);
    p.expect(K::Colon, "`:`");
    sub_element(p);
    p.finish();
}

fn sub_element(p: &mut Parser) {
    if p.at(K::Ident) && matches!(p.nth(1), Some(K::ColonEqual | K::LBrace)) {
        element(p);
    } else {
        p.error("expected an element");
    }
}
