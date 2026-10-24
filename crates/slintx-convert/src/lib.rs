//! Converts `.slint` files to XML, HTML, Apple Storyboard and GTK Blueprint.
//!
//! The pipeline is parse ([`slintx_syntax`]), lower to a format-free IR
//! ([`ir`]), then one [`backend::Backend`] per format. Structure and meaning
//! are kept: the element tree, layouts, alignment, stretch, texts, bindings and
//! handlers. Slint logic is never run; bindings and handlers become the target's
//! hooks (ids and `data-*`, outlets and actions, signal handlers).

pub mod backend;
pub mod color;
pub mod diag;
pub mod ir;
mod lower;
pub mod mapping;

use thiserror::Error;

pub use backend::{Backend, Format, Platform};
pub use diag::{Diagnostic, Diagnostics, Severity};

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub platform: Platform,
    /// Turns every warning into an error.
    pub strict: bool,
    /// Tags every element with its `.slint` line and column where the target
    /// keeps it at run time: the GTK widget `name`, the AppKit view
    /// `identifier`, the UIKit `accessibilityIdentifier`. HTML always has
    /// `data-at`. Lets a test find each element on screen.
    pub source_tags: bool,
}

#[derive(Debug, Clone)]
pub struct Conversion {
    pub output: String,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Error)]
pub enum ConvertError {
    /// Syntax errors, lowering errors, or warnings under `strict`; every
    /// diagnostic is kept so the caller can print all of them.
    #[error("conversion failed with {} error(s)", count_errors(.diagnostics))]
    Failed { diagnostics: Vec<Diagnostic> },
}

fn count_errors(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count()
}

/// Parses and lowers one file. Syntax errors stop here: no IR is built from a
/// broken tree.
pub fn to_ir(file: &str, source: &str) -> Result<(ir::Document, Diagnostics), ConvertError> {
    let parse = slintx_syntax::parse(source);
    let index = diag::LineIndex::new(file, source);
    let mut diagnostics = Diagnostics::default();
    for d in parse.diagnostics() {
        diagnostics.error(&index.origin(d.range), d.message.clone());
    }
    if diagnostics.has_errors() {
        return Err(ConvertError::Failed {
            diagnostics: diagnostics.sorted(),
        });
    }
    let Some(root) = parse.tree::<slintx_syntax::ast::File>() else {
        diagnostics.error(&index.origin(Default::default()), "no file root");
        return Err(ConvertError::Failed {
            diagnostics: diagnostics.sorted(),
        });
    };
    let document = lower::lower(file, source, &root, &mut diagnostics);
    if document.root.is_none() {
        diagnostics.error(&index.origin(Default::default()), "no component to convert");
    }
    Ok((document, diagnostics))
}

pub fn convert(
    file: &str,
    source: &str,
    format: Format,
    options: &Options,
) -> Result<Conversion, ConvertError> {
    let (document, mut diagnostics) = to_ir(file, source)?;
    let output = backend::backend(format, options).emit(&document, &mut diagnostics);
    let mut all = diagnostics.sorted();
    if options.strict {
        for d in &mut all {
            d.severity = Severity::Error;
        }
    }
    if all.iter().any(|d| d.severity == Severity::Error) {
        return Err(ConvertError::Failed { diagnostics: all });
    }
    Ok(Conversion {
        output,
        warnings: all,
    })
}
