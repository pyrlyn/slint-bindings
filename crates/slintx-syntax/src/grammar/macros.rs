//! String literals with escapes and interpolation, and the `@` macros.

use std::ops::Range;

use super::expr::{expr, node, primary};
use crate::parser::{Parser, same_name};
use crate::{Diagnostic, SyntaxKind as K};

pub(crate) fn string_literal(p: &mut Parser) {
    check_escapes(p);
    node(p, K::Literal);
}

/// `"a \{x} b"`: the pieces around each interpolation, then its expression.
pub(crate) fn string_template(p: &mut Parser) {
    p.start(K::StringTemplate);
    check_escapes(p);
    p.bump();
    loop {
        expr(p);
        let at_piece = |p: &Parser| matches!(p.nth(0), Some(K::StringMiddle | K::StringEnd));
        if !at_piece(p) && !p.at_end() {
            p.error("expected `}` to close the interpolation");
            p.start(K::ErrorNode);
            while !at_piece(p) && !p.at_end() {
                p.bump();
            }
            p.finish();
        }
        let end = p.at(K::StringEnd);
        if !at_piece(p) {
            p.error("unterminated string interpolation");
            break;
        }
        check_escapes(p);
        p.bump();
        if end {
            break;
        }
    }
    p.finish();
}

/// Reports every escape other than `\"`, `\\`, `\n`, `\u{hex}` and `\{` in
/// the current string token.
fn check_escapes(p: &mut Parser) {
    let (text, offset) = (p.nth_text(0), p.offset());
    for bad in invalid_escapes(text) {
        let range = offset + bad.start..offset + bad.end;
        p.push_diagnostic(Diagnostic::new("invalid escape sequence", range));
    }
}

fn invalid_escapes(text: &str) -> Vec<Range<usize>> {
    let mut bad = Vec::new();
    let mut i = 0;
    // Byte steps are safe: `\` is ASCII, so it never sits inside a char.
    while i < text.len() {
        if text.as_bytes()[i] != b'\\' {
            i += 1;
            continue;
        }
        let rest = &text[i + 1..];
        let len = escape_len(rest).unwrap_or_else(|| {
            let len = rest.chars().next().map_or(0, char::len_utf8);
            bad.push(i..i + 1 + len);
            len
        });
        i += 1 + len;
    }
    bad
}

fn escape_len(rest: &str) -> Option<usize> {
    match rest.chars().next()? {
        '"' | '\\' | 'n' | '{' => Some(1),
        'u' => {
            let body = rest.strip_prefix("u{")?;
            let hex = &body[..body.find('}')?];
            let digits = (1..=6).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit());
            let valid = digits
                && u32::from_str_radix(hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .is_some();
            valid.then_some(hex.len() + 3)
        }
        _ => None,
    }
}

pub(crate) fn macro_call(p: &mut Parser) {
    let name = p.nth_text(1);
    let is = |known: &str| p.nth(1) == Some(K::Ident) && same_name(name, known);
    if is("tr") {
        tr_macro(p);
    } else if is("image-url") {
        image_url_macro(p);
    } else if is("linear-gradient") || is("radial-gradient") || is("conic-gradient") {
        gradient_macro(p);
    } else if is("markdown") {
        markdown_macro(p);
    } else if is("keys") {
        keys_macro(p);
    } else {
        p.error(format!("unknown macro `@{name}`"));
        p.start(K::ErrorNode);
        p.bump();
        p.eat(K::Ident);
        skip_parens(p);
        p.finish();
    }
}

/// Skips a balanced `( ... )` group, if one starts here.
pub(crate) fn skip_parens(p: &mut Parser) {
    if !p.at(K::LParen) {
        return;
    }
    let mut depth = 0u32;
    while let Some(kind) = p.nth(0) {
        depth = match kind {
            K::LParen => depth + 1,
            K::RParen => depth - 1,
            _ => depth,
        };
        p.bump();
        if depth == 0 {
            return;
        }
    }
    p.error("expected `)`");
}

fn macro_head(p: &mut Parser, kind: K) {
    p.start(kind);
    p.bump();
    p.bump();
    p.expect(K::LParen, "`(`");
}

/// Closes a macro; junk before its `)` becomes one error node, so a bad
/// argument costs one diagnostic.
fn macro_tail(p: &mut Parser) {
    let stop = |p: &Parser| matches!(p.nth(0), None | Some(K::RParen | K::Semicolon | K::RBrace));
    if !p.at(K::RParen) {
        p.error("expected `)`");
        if !stop(p) {
            p.start(K::ErrorNode);
            while !stop(p) {
                p.bump();
            }
            p.finish();
        }
    }
    p.eat(K::RParen);
    p.finish();
}

fn string_arg(p: &mut Parser) {
    if p.at(K::String) {
        string_literal(p);
    } else {
        p.error("expected a string literal");
    }
}

/// `@tr("context" => "one {n}" | "many {n}" % n, args...)`.
fn tr_macro(p: &mut Parser) {
    macro_head(p, K::TrMacro);
    string_arg(p);
    if p.eat(K::FatArrow) {
        string_arg(p);
    }
    if p.eat(K::Pipe) {
        string_arg(p);
        p.expect(K::Percent, "`%`");
        expr(p);
    }
    while p.eat(K::Comma) && !p.at(K::RParen) {
        expr(p);
    }
    macro_tail(p);
}

/// `@image-url("path")` or `@image-url("path", nine-slice(1 2 3 4))`.
fn image_url_macro(p: &mut Parser) {
    macro_head(p, K::ImageUrlMacro);
    string_arg(p);
    if p.eat(K::Comma) {
        if p.at_keyword("nine-slice") {
            p.start(K::NineSlice);
            p.bump();
            p.expect(K::LParen, "`(`");
            while p.eat(K::Number) {}
            p.expect(K::RParen, "`)`");
            p.finish();
        } else {
            p.error("expected `nine-slice(...)`");
        }
    }
    macro_tail(p);
}

/// Gradient arguments are comma-separated, and each is a run of
/// space-separated expressions: `90deg`, `red 50%`, `circle 4px at 1px 2px`.
fn gradient_macro(p: &mut Parser) {
    macro_head(p, K::GradientMacro);
    while !p.at(K::RParen) && !p.at_end() {
        p.start(K::MacroArg);
        while !matches!(p.nth(0), None | Some(K::Comma | K::RParen)) {
            let before = p.offset();
            expr(p);
            if p.offset() == before {
                break;
            }
        }
        p.finish();
        if !p.eat(K::Comma) {
            break;
        }
    }
    macro_tail(p);
}

/// `@markdown("a" "\{b}")`: one or more adjacent strings.
fn markdown_macro(p: &mut Parser) {
    macro_head(p, K::MarkdownMacro);
    if !matches!(p.nth(0), Some(K::String | K::StringStart)) {
        p.error("expected a string literal");
    }
    while matches!(p.nth(0), Some(K::String | K::StringStart)) {
        primary(p);
    }
    macro_tail(p);
}

/// `@keys(Control + Shift? + "z")`.
fn keys_macro(p: &mut Parser) {
    macro_head(p, K::KeysMacro);
    while !p.at(K::RParen) && !p.at_end() {
        p.start(K::KeyPart);
        match p.nth(0) {
            Some(K::Ident) => p.bump(),
            Some(K::String) => {
                check_escapes(p);
                p.bump();
            }
            _ => p.error("expected a key name or a string"),
        }
        p.eat(K::Question);
        p.finish();
        if !p.eat(K::Plus) {
            break;
        }
    }
    macro_tail(p);
}
