//! Hand-written lexer. Every byte of the input lands in exactly one token, so
//! the tree built from the tokens prints back the input unchanged.

use crate::{Diagnostic, SyntaxKind};

/// One token: its kind and byte range in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: SyntaxKind,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        src: source,
        pos: 0,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
        interpolations: Vec::new(),
    };
    lexer.run();
    (lexer.tokens, lexer.diagnostics)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    // One entry per open `\{`: the depth of plain `{` inside it, so the `}`
    // that closes the interpolation is told apart from a nested object's.
    interpolations: Vec<u32>,
}

impl Lexer<'_> {
    fn run(&mut self) {
        if self.src.starts_with('\u{feff}') {
            self.pos = '\u{feff}'.len_utf8();
            self.push(SyntaxKind::Whitespace, 0);
        }
        while let Some(c) = self.peek() {
            let start = self.pos;
            let kind = self.token(c, start);
            if kind == SyntaxKind::Error {
                self.error(start, "unexpected character");
            }
            self.push(kind, start);
        }
    }

    fn token(&mut self, c: char, start: usize) -> SyntaxKind {
        use SyntaxKind as K;
        self.bump(c);
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                self.eat_while(|c| matches!(c, ' ' | '\t' | '\n' | '\r'));
                K::Whitespace
            }
            '/' if self.eat('/') => {
                self.eat_while(|c| c != '\n');
                K::Comment
            }
            '/' if self.eat('*') => self.block_comment(start),
            '0'..='9' => self.number(),
            '"' => self.string(start, K::String, K::StringStart),
            '#' => self.color(start),
            c if is_ident_start(c) => {
                self.eat_while(|c| is_ident_continue(c) || c == '-');
                K::Ident
            }
            '{' => {
                if let Some(depth) = self.interpolations.last_mut() {
                    *depth += 1;
                }
                K::LBrace
            }
            '}' => match self.interpolations.last_mut() {
                Some(0) => {
                    self.interpolations.pop();
                    self.string(start, K::StringEnd, K::StringMiddle)
                }
                Some(depth) => {
                    *depth -= 1;
                    K::RBrace
                }
                None => K::RBrace,
            },
            '(' => K::LParen,
            ')' => K::RParen,
            '[' => K::LBracket,
            ']' => K::RBracket,
            ';' => K::Semicolon,
            ',' => K::Comma,
            '.' => K::Dot,
            '?' => K::Question,
            '@' => K::At,
            '%' => K::Percent,
            ':' => self.pick(&[("=", K::ColonEqual)], K::Colon),
            '<' => self.pick(&[("=>", K::DoubleArrow), ("=", K::LessEqual)], K::Less),
            '=' => self.pick(&[(">", K::FatArrow), ("=", K::EqualEqual)], K::Equal),
            '-' => self.pick(&[(">", K::Arrow), ("=", K::MinusEqual)], K::Minus),
            '+' => self.pick(&[("=", K::PlusEqual)], K::Plus),
            '*' => self.pick(&[("=", K::StarEqual)], K::Star),
            '/' => self.pick(&[("=", K::SlashEqual)], K::Slash),
            '!' => self.pick(&[("=", K::NotEqual)], K::Bang),
            '>' => self.pick(&[("=", K::GreaterEqual)], K::Greater),
            '|' => self.pick(&[("|", K::OrOr)], K::Pipe),
            '&' => self.pick(&[("&", K::AndAnd)], K::Error),
            _ => K::Error,
        }
    }

    /// Comments nest: `/* a /* b */ c */` is one comment.
    fn block_comment(&mut self, start: usize) -> SyntaxKind {
        let mut depth = 1u32;
        while let Some(c) = self.peek() {
            self.bump(c);
            if c == '/' && self.eat('*') {
                depth += 1;
            } else if c == '*' && self.eat('/') {
                depth -= 1;
                if depth == 0 {
                    return SyntaxKind::Comment;
                }
            }
        }
        self.error(start, "unterminated block comment");
        SyntaxKind::Comment
    }

    /// Digits, an optional `.digits` fraction, then a unit: a run of ASCII
    /// letters or one `%`. Whether the unit exists is the type checker's call.
    fn number(&mut self) -> SyntaxKind {
        self.eat_while(|c| c.is_ascii_digit());
        let rest = &self.src[self.pos..];
        if rest.starts_with('.') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            self.pos += 1;
            self.eat_while(|c| c.is_ascii_digit());
        }
        if !self.eat('%') {
            self.eat_while(|c| c.is_ascii_alphabetic());
        }
        SyntaxKind::Number
    }

    /// Scans string text after `"` or after the `}` closing an interpolation.
    /// Escapes are only skipped here; the parser validates them, because an
    /// import path takes its text verbatim.
    fn string(&mut self, start: usize, closed: SyntaxKind, open: SyntaxKind) -> SyntaxKind {
        while let Some(c) = self.peek() {
            self.bump(c);
            match c {
                '"' => return closed,
                '\\' if self.eat('{') => {
                    self.interpolations.push(0);
                    return open;
                }
                '\\' => {
                    if let Some(next) = self.peek() {
                        self.bump(next);
                    }
                }
                _ => {}
            }
        }
        self.error(start, "unterminated string literal");
        closed
    }

    /// `#` and the alphanumeric run after it, so `#12g` is one bad literal
    /// rather than a color followed by an identifier.
    fn color(&mut self, start: usize) -> SyntaxKind {
        self.eat_while(|c| c.is_ascii_alphanumeric());
        let digits = &self.src[start + 1..self.pos];
        let hex = digits.bytes().all(|b| b.is_ascii_hexdigit());
        if !hex || ![3, 4, 6, 8].contains(&digits.len()) {
            self.error(
                start,
                "invalid color literal: expected #rgb, #rgba, #rrggbb or #rrggbbaa",
            );
        }
        SyntaxKind::Color
    }

    fn pick(&mut self, options: &[(&str, SyntaxKind)], fallback: SyntaxKind) -> SyntaxKind {
        for &(suffix, kind) in options {
            if self.src[self.pos..].starts_with(suffix) {
                self.pos += suffix.len();
                return kind;
            }
        }
        fallback
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self, c: char) {
        self.pos += c.len_utf8();
    }

    fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(c);
        if found {
            self.bump(c);
        }
        found
    }

    fn eat_while(&mut self, pred: impl Fn(char) -> bool) {
        while let Some(c) = self.peek().filter(|&c| pred(c)) {
            self.bump(c);
        }
    }

    fn push(&mut self, kind: SyntaxKind, start: usize) {
        self.tokens.push(Token {
            kind,
            start,
            end: self.pos,
        });
    }

    fn error(&mut self, start: usize, message: &str) {
        self.diagnostics
            .push(Diagnostic::new(message, start..self.pos));
    }
}

// The docs ask for UAX #31 (XID_Start / XID_Continue); std's alphabetic and
// alphanumeric classes approximate them closely and keep the crate at one
// dependency.
fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}
