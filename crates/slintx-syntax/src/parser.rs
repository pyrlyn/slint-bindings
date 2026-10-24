//! Parser core: a cursor over the lexer's tokens that feeds a rowan builder.
//! The grammar lives in `grammar`; this file knows nothing about Slint.

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder, GreenToken, NodeOrToken};

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
        let root = self.builder.finish();
        let children: Vec<_> = root
            .children()
            .flat_map(|child| hoist(child.to_owned()))
            .collect();
        (GreenNode::new(root.kind(), children), self.diagnostics)
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
        // The lexer already reported this token; a parse error on top of it
        // repeats the same mistake.
        if self.at(SyntaxKind::Error) {
            return;
        }
        let range = self
            .significant(0)
            .map_or(self.src.len()..self.src.len(), |t| t.start..t.end);
        let diagnostic = Diagnostic::new(message, range);
        // Nested constructs left unclosed each report the same missing closer
        // at the same token; one is enough.
        if self.diagnostics.last() != Some(&diagnostic) {
            self.diagnostics.push(diagnostic);
        }
    }

    pub fn expect(&mut self, kind: SyntaxKind, what: &str) -> bool {
        let found = self.eat(kind);
        if !found {
            self.error(format!("expected {what}"));
        }
        found
    }

    pub fn expect_keyword(&mut self, keyword: &str) -> bool {
        let found = self.at_keyword(keyword);
        if found {
            self.bump();
        } else {
            self.error(format!("expected `{keyword}`"));
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
    a.chars()
        .enumerate()
        .map(normalize)
        .eq(b.chars().enumerate().map(normalize))
}

/// `name` spelled so that names [`same_name`] treats as equal are equal
/// strings, for use as a map key.
pub fn normalize_name(name: &str) -> String {
    name.chars().enumerate().map(normalize).collect()
}

fn normalize((i, c): (usize, char)) -> char {
    if i > 0 && c == '_' { '-' } else { c }
}

type GreenElement = NodeOrToken<GreenNode, GreenToken>;

/// Moves trivia off the edges of `element` and its descendants, returning
/// the leading trivia, the node, then the trailing trivia. Trivia is flushed
/// eagerly while parsing, so a node that ends early on an error (or stays
/// empty) can otherwise keep the whitespace that precedes the next token.
fn hoist(element: GreenElement) -> Vec<GreenElement> {
    let node = match element {
        NodeOrToken::Node(node) => node,
        token => return vec![token],
    };
    let mut inner: Vec<GreenElement> = node.children().flat_map(|c| hoist(c.to_owned())).collect();
    let is_trivia = |e: &GreenElement| {
        e.as_token()
            .is_some_and(|t| SyntaxKind::from_raw(t.kind().0).is_some_and(SyntaxKind::is_trivia))
    };
    let lead = inner.iter().take_while(|e| is_trivia(e)).count();
    let trail = inner[lead..]
        .iter()
        .rev()
        .take_while(|e| is_trivia(e))
        .count();
    let trailing = inner.split_off(inner.len() - trail);
    let leading: Vec<GreenElement> = inner.drain(..lead).collect();
    let mut out = leading;
    out.push(NodeOrToken::Node(GreenNode::new(node.kind(), inner)));
    out.extend(trailing);
    out
}
