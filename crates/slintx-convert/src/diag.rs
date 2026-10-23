//! Source positions and the warnings and errors every stage reports. One module
//! so the parser's diagnostics, lowering and the backends print the same way.

use std::fmt;

use slintx_syntax::TextRange;

/// Where an IR node or a diagnostic comes from: 1-based line and column, the
/// column counted in characters so Unicode text points at the right place.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Origin {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// Turns byte offsets of one source file into [`Origin`]s.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    file: String,
    source: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(file: &str, source: &'a str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        LineIndex {
            file: file.to_owned(),
            source,
            line_starts,
        }
    }

    pub fn origin(&self, range: TextRange) -> Origin {
        let offset = usize::from(range.start()).min(self.source.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let start = self.line_starts[line];
        let column = self
            .source
            .get(start..offset)
            .map_or(0, |prefix| prefix.chars().count());
        Origin {
            file: self.file.clone(),
            line: to_u32(line + 1),
            column: to_u32(column + 1),
        }
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    pub origin: Origin,
    pub severity: Severity,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{}: {severity}: {}", self.origin, self.message)
    }
}

/// Collects diagnostics in the order stages report them; [`Diagnostics::sorted`]
/// gives the stable order the CLI prints.
#[derive(Debug, Default, Clone)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn warn(&mut self, origin: &Origin, message: impl Into<String>) {
        self.push(origin, Severity::Warning, message);
    }

    pub fn error(&mut self, origin: &Origin, message: impl Into<String>) {
        self.push(origin, Severity::Error, message);
    }

    fn push(&mut self, origin: &Origin, severity: Severity, message: impl Into<String>) {
        self.items.push(Diagnostic {
            origin: origin.clone(),
            severity,
            message: message.into(),
        });
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn sorted(mut self) -> Vec<Diagnostic> {
        self.items.sort();
        self.items.dedup();
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slintx_syntax::TextSize;

    fn at(source: &str, offset: u32) -> Origin {
        let range = TextRange::empty(TextSize::from(offset));
        LineIndex::new("a.slint", source).origin(range)
    }

    #[test]
    fn origin_counts_lines_and_characters_from_one() {
        assert_eq!(at("ab\ncd", 0).to_string(), "a.slint:1:1");
        assert_eq!(at("ab\ncd", 4).to_string(), "a.slint:2:2");
    }

    #[test]
    fn origin_column_counts_characters_not_bytes() {
        // "Привет" is 12 bytes; the offset after it is column 7.
        assert_eq!(at("Привет x", 13).column, 8);
    }
}
