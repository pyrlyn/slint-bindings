//! Parser core: a cursor over the lexer's tokens that feeds a rowan builder.
//! The grammar lives in `grammar`; this file knows nothing about Slint.

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder};

use crate::lexer::Token;
use crate::{Diagnostic, SyntaxKind};

// Nesting limit for elements, blocks, types and expressions. The parser
// recurses, and input may come from an untrusted plugin, so deep nesting must
// end in a diagnostic rather than a stack overflow (wasm stacks are small).
const MAX_DEPTH: u32 = 128;

pub(crate) struct Parser<'a> {
    src: &'a str,
    tokens: &'a [Token],
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    diagnostics: Vec<Diagnostic>,
    depth: u32,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, tokens: &'a [Token]) -> Self {
        Parser {
            src,
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
            diagnostics: Vec::new(),
            depth: 0,
        }
    }

    pub fn finish_tree(self) -> (GreenNode, Vec<Diagnostic>) {
        (self.builder.finish(), self.diagnostics)
    }

    /// The root takes all trivia, leading and trailing, so it spans the
    /// whole input.
    pub fn start_root(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
    }

    pub fn finish_root(&mut self) {
        self.flush_trivia();
        self.builder.finish_node();
    }

    fn significant(&self, n: usize) -> Option<&'a Token> {
        self.tokens[self.pos..]
            .iter()
            .filter(|t| !t.kind.is_trivia())
            .nth(n)
    }

    /// Kind of the `n`th token ahead, trivia skipped; `None` at the end.
    pub fn nth(&self, n: usize) -> Option<SyntaxKind> {
        self.significant(n).map(|t| t.kind)
    }

    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.nth(0) == Some(kind)
    }

    pub fn at_end(&self) -> bool {
        self.nth(0).is_none()
    }

    pub fn nth_text(&self, n: usize) -> &'a str {
        self.significant(n)
            .map_or("", |t| &self.src[t.start..t.end])
    }

    /// Slint has no reserved words: a keyword is an identifier with that
    /// text where the grammar expects it. `_` and `-` are the same letter
    /// after the first, so `in_out` is `in-out`.
    pub fn nth_keyword(&self, n: usize, keyword: &str) -> bool {
        self.nth(n) == Some(SyntaxKind::Ident) && same_name(self.nth_text(n), keyword)
    }

    pub fn at_keyword(&self, keyword: &str) -> bool {
        self.nth_keyword(0, keyword)
    }

    // Trivia goes to whichever node is open when the next real token or node
    // starts, so nodes begin at their first significant token.
    fn flush_trivia(&mut self) {
        while let Some(token) = self.tokens.get(self.pos).filter(|t| t.kind.is_trivia()) {
            self.builder
                .token(token.kind.into(), &self.src[token.start..token.end]);
            self.pos += 1;
        }
    }

    pub fn bump(&mut self) {
        self.flush_trivia();
        if let Some(token) = self.tokens.get(self.pos) {
            self.builder
                .token(token.kind.into(), &self.src[token.start..token.end]);
            self.pos += 1;
        }
    }

    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        let found = self.at(kind);
        if found {
            self.bump();
        }
        found
    }

    pub fn start(&mut self, kind: SyntaxKind) {
        self.flush_trivia();
        self.builder.start_node(kind.into());
    }

    pub fn finish(&mut self) {
        self.builder.finish_node();
    }

    pub fn checkpoint(&mut self) -> Checkpoint {
        self.flush_trivia();
        self.builder.checkpoint()
    }

    pub fn start_at(&mut self, checkpoint: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(checkpoint, kind.into());
    }

    /// Reports at the current token, or at the end of the input.
    pub fn error(&mut self, message: impl Into<String>) {
        let range = self
            .significant(0)
            .map_or(self.src.len()..self.src.len(), |t| t.start..t.end);
        self.diagnostics.push(Diagnostic::new(message, range));
    }

    pub fn expect(&mut self, kind: SyntaxKind, what: &str) -> bool {
        let found = self.eat(kind);
        if !found {
            self.error(format!("expected {what}"));
        }
        found
    }

    /// Reports `message` and wraps the current token in an error node, which
    /// guarantees the caller's loop makes progress.
    pub fn error_bump(&mut self, message: impl Into<String>) {
        self.error(message);
        self.bump_as_error();
    }

    /// Wraps the current token in an error node without a new diagnostic,
    /// for tokens whose problem was already reported.
    pub fn bump_as_error(&mut self) {
        self.start(SyntaxKind::ErrorNode);
        self.bump();
        self.finish();
    }

    /// Enters one nesting level; past the limit, swallows the rest of the
    /// input into an error node and returns false.
    pub fn enter(&mut self) -> bool {
        self.depth += 1;
        if self.depth <= MAX_DEPTH {
            return true;
        }
        self.depth -= 1;
        self.error("nesting is too deep");
        self.start(SyntaxKind::ErrorNode);
        while !self.at_end() {
            self.bump();
        }
        self.finish();
        false
    }

    pub fn leave(&mut self) {
        self.depth -= 1;
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Byte offset of the current significant token.
    pub fn offset(&self) -> usize {
        self.significant(0).map_or(self.src.len(), |t| t.start)
    }
}

/// Compares identifiers the way Slint does: `_` after the first character
/// equals `-`.
pub(crate) fn same_name(a: &str, b: &str) -> bool {
    let normalize = |(i, c): (usize, char)| if i > 0 && c == '_' { '-' } else { c };
    a.chars()
        .enumerate()
        .map(normalize)
        .eq(b.chars().enumerate().map(normalize))
}
