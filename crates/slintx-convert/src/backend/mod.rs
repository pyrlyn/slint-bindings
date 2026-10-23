//! The `Backend` trait every output format implements, plus what the formats
//! share: number formatting, value text and an indenting markup writer for the
//! XML-shaped outputs.

mod blueprint;
mod html;
mod storyboard;
mod xml;

use std::fmt::Write as _;

use crate::diag::Diagnostics;
use crate::ir::{Document, Value};

pub use blueprint::Blueprint;
pub use html::Html;
pub use storyboard::Storyboard;
pub use xml::Xml;

/// One output format. Backends never fail: what they cannot express becomes a
/// warning with a position and a comment stub in the output.
pub trait Backend {
    fn emit(&self, document: &Document, diagnostics: &mut Diagnostics) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Xml,
    Html,
    Storyboard,
    Blueprint,
}

/// Which Apple framework a storyboard targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Platform {
    #[default]
    UiKit,
    AppKit,
}

pub fn backend(format: Format, platform: Platform) -> Box<dyn Backend> {
    match format {
        Format::Xml => Box::new(Xml),
        Format::Html => Box::new(Html),
        Format::Storyboard => Box::new(Storyboard { platform }),
        Format::Blueprint => Box::new(Blueprint),
    }
}

/// Integers without a fraction, everything else as Rust prints it: stable and
/// short.
pub(crate) fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// A value as one line of text: `8px`, `"Hello"` without quotes, `{expr}` for
/// a binding.
pub(crate) fn value_text(value: &Value) -> String {
    match value {
        Value::Number(n) => number(*n),
        Value::Measure { value, unit } => format!("{}{}", number(*value), unit.as_str()),
        Value::String { text, .. } => text.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Color(c) | Value::Enum(c) | Value::Url(c) => c.clone(),
        Value::Array(items) => {
            let items: Vec<_> = items.iter().map(value_text).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Binding(b) => format!("{{{}}}", b.expr),
        Value::TwoWay(b) => format!("<=> {}", b.expr),
    }
}

/// Builds indented markup. Attribute values and text are escaped here, so
/// backends never handle raw `<` or `&`.
#[derive(Debug, Default)]
pub(crate) struct Markup {
    out: String,
    depth: usize,
}

pub(crate) type Attrs<'a> = [(&'a str, String)];

impl Markup {
    pub fn raw_line(&mut self, line: &str) {
        self.indent();
        self.out.push_str(line);
        self.out.push('\n');
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }

    fn tag(&mut self, name: &str, attrs: &Attrs<'_>, end: &str) {
        self.indent();
        let _ = write!(self.out, "<{name}");
        for (key, value) in attrs {
            let _ = write!(self.out, " {key}=\"{}\"", escape(value));
        }
        self.out.push_str(end);
        self.out.push('\n');
    }

    pub fn open(&mut self, name: &str, attrs: &Attrs<'_>) {
        self.tag(name, attrs, ">");
        self.depth += 1;
    }

    pub fn close(&mut self, name: &str) {
        self.depth = self.depth.saturating_sub(1);
        self.raw_line(&format!("</{name}>"));
    }

    pub fn empty(&mut self, name: &str, attrs: &Attrs<'_>) {
        self.tag(name, attrs, "/>");
    }

    /// `<name attrs>text</name>` on one line.
    pub fn text_element(&mut self, name: &str, attrs: &Attrs<'_>, text: &str) {
        self.indent();
        let _ = write!(self.out, "<{name}");
        for (key, value) in attrs {
            let _ = write!(self.out, " {key}=\"{}\"", escape(value));
        }
        let _ = writeln!(self.out, ">{}</{name}>", escape(text));
    }

    /// A comment; `--` cannot appear inside one, so it is split.
    pub fn comment(&mut self, text: &str) {
        let safe = text.replace("--", "- -");
        self.raw_line(&format!("<!-- {safe} -->"));
    }

    pub fn finish(self) -> String {
        self.out
    }
}

pub(crate) fn escape(text: &str) -> String {
    quick_xml::escape::escape(text).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_without_needless_fraction() {
        assert_eq!(number(8.0), "8");
        assert_eq!(number(-0.5), "-0.5");
    }

    #[test]
    fn markup_escapes_attributes_and_comments() {
        let mut m = Markup::default();
        m.open("a", &[("b", "x<\"&".to_owned())]);
        m.comment("a -- b");
        m.close("a");
        assert_eq!(
            m.finish(),
            "<a b=\"x&lt;&quot;&amp;\">\n  <!-- a - - b -->\n</a>\n"
        );
    }
}
