//! The Slint grammar, as recursive descent over [`Parser`].

mod element;
mod expr;
mod items;
mod macros;
mod stmt;

use crate::SyntaxKind as K;
use crate::parser::Parser;

pub(crate) fn file(p: &mut Parser) {
    p.start_root(K::File);
    while !p.at_end() {
        items::item(p);
    }
    p.finish_root();
}

/// Root of [`crate::parse_binding_expression`]: a binding value alone.
pub(crate) fn binding_root(p: &mut Parser) {
    p.start_root(K::BindingExpression);
    expr::binding_value(p);
    if !p.at_end() {
        p.error("unexpected input after the expression");
        p.start(K::ErrorNode);
        while !p.at_end() {
            p.bump();
        }
        p.finish();
    }
    p.finish_root();
}
