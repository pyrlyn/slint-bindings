//! A standalone parser for the Slint language.
//!
//! [`parse`] turns `.slint` source into a lossless [rowan] tree plus
//! diagnostics. The tree keeps every byte, whitespace and comments included,
//! so printing it gives back the input; broken input still yields a full tree,
//! with error nodes where the parser recovered. [`ast`] has typed views over
//! that tree.

pub mod ast;
mod grammar;
mod kind;
mod lexer;
mod parser;

use std::fmt;
use std::ops::Range;

pub use kind::{SlintLanguage, SyntaxKind};
pub use parser::normalize_name;
pub use rowan::{NodeOrToken, TextRange, TextSize};

pub type SyntaxNode = rowan::SyntaxNode<SlintLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<SlintLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<SlintLanguage>;

/// A problem found while lexing or parsing, with the byte range it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange,
}

impl Diagnostic {
    pub(crate) fn new(message: impl Into<String>, range: Range<usize>) -> Self {
        Diagnostic {
            message: message.into(),
            range: text_range(range),
        }
    }
}

// Callers only pass offsets inside a source already checked to fit u32.
fn text_range(range: Range<usize>) -> TextRange {
    let size = |offset: usize| TextSize::from(u32::try_from(offset).unwrap_or(u32::MAX));
    TextRange::new(size(range.start), size(range.end))
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (start, end) = (u32::from(self.range.start()), u32::from(self.range.end()));
        write!(f, "{start}..{end}: {}", self.message)
    }
}

/// The result of [`parse`]: the tree and every diagnostic, in source order.
#[derive(Debug, Clone)]
pub struct Parse {
    green: rowan::GreenNode,
    diagnostics: Vec<Diagnostic>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The root as a typed view: [`ast::File`] after [`parse`],
    /// [`ast::BindingExpression`] after [`parse_binding_expression`].
    pub fn tree<N: ast::AstNode>(&self) -> Option<N> {
        N::cast(self.syntax())
    }
}

/// Parses a whole `.slint` file. Never fails: errors become error nodes in
/// the tree and entries in [`Parse::diagnostics`].
pub fn parse(source: &str) -> Parse {
    run(source, grammar::file, SyntaxKind::File)
}

/// Parses what follows `name:` in a binding on its own: an expression or a
/// `{ ... }` code block. The root is a `BindingExpression` node.
pub fn parse_binding_expression(source: &str) -> Parse {
    run(source, grammar::binding_root, SyntaxKind::BindingExpression)
}

fn run(source: &str, entry: fn(&mut parser::Parser), root: SyntaxKind) -> Parse {
    if u32::try_from(source.len()).is_err() {
        // rowan offsets are u32; such a file cannot be represented at all.
        let mut builder = rowan::GreenNodeBuilder::new();
        builder.start_node(root.into());
        builder.finish_node();
        return Parse {
            green: builder.finish(),
            diagnostics: vec![Diagnostic::new("source is larger than 4 GiB", 0..0)],
        };
    }
    let (tokens, mut diagnostics) = lexer::lex(source);
    let mut parser = parser::Parser::new(source, &tokens);
    entry(&mut parser);
    let (green, parse_diagnostics) = parser.finish_tree();
    diagnostics.extend(parse_diagnostics);
    diagnostics.sort_by_key(|d| d.range.start());
    Parse { green, diagnostics }
}

/// Splits source into tokens, trivia included, with their byte ranges.
pub fn lex(source: &str) -> (Vec<(SyntaxKind, TextRange)>, Vec<Diagnostic>) {
    if u32::try_from(source.len()).is_err() {
        return (
            Vec::new(),
            vec![Diagnostic::new("source is larger than 4 GiB", 0..0)],
        );
    }
    let (tokens, diagnostics) = lexer::lex(source);
    let tokens = tokens
        .into_iter()
        .map(|t| (t.kind, text_range(t.start..t.end)))
        .collect();
    (tokens, diagnostics)
}
