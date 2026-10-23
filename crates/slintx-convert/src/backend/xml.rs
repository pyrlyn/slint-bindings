//! The `xml` backend: the IR written out in full, one element per widget named
//! after the Slint element, with the layout model, values, bindings and
//! handlers as lowercase child elements. Lossless with respect to the IR, so
//! it is also the reference the other backends are reviewed against.

use super::{Attrs, Backend, Markup, number, value_text};
use crate::diag::{Diagnostics, Origin};
use crate::ir::*;

pub struct Xml;

impl Backend for Xml {
    fn emit(&self, document: &Document, _: &mut Diagnostics) -> String {
        let mut m = Markup::default();
        m.raw_line(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        m.open("slint", &[("file", document.file.clone())]);
        for c in &document.components {
            component(&mut m, c);
        }
        if let Some(root) = &document.root {
            node(&mut m, root);
        }
        m.close("slint");
        m.finish()
    }
}

fn at(origin: &Origin) -> (&'static str, String) {
    ("at", format!("{}:{}", origin.line, origin.column))
}

fn component(m: &mut Markup, c: &ComponentInfo) {
    let attrs = [
        ("name", c.name.clone()),
        ("base", c.base.clone()),
        ("exported", c.exported.to_string()),
        at(&c.origin),
    ];
    if c.properties.is_empty() && c.callbacks.is_empty() {
        m.empty("component", &attrs);
        return;
    }
    m.open("component", &attrs);
    for p in &c.properties {
        declaration(m, p);
    }
    for cb in &c.callbacks {
        let mut attrs = vec![("name", cb.name.clone()), ("params", cb.params.join(", "))];
        if let Some(r) = &cb.return_type {
            attrs.push(("returns", r.clone()));
        }
        attrs.push(at(&cb.origin));
        m.empty("callback", &attrs);
    }
    m.close("component");
}

fn declaration(m: &mut Markup, p: &PropertyDecl) {
    let mut attrs = vec![("name", p.name.clone()), ("type", p.ty.clone())];
    if let Some(v) = &p.visibility {
        attrs.push(("visibility", v.clone()));
    }
    if let Some(d) = &p.default {
        value_attrs(&mut attrs, d);
    }
    attrs.push(at(&p.origin));
    m.empty("declare", &attrs);
}

/// `kind` and `value` (plus `unit` or `translatable`) for a plain value;
/// arrays and bindings get their own attributes.
fn value_attrs(attrs: &mut Vec<(&str, String)>, value: &Value) {
    let kind = match value {
        Value::Number(_) => "number",
        Value::Measure { .. } => "measure",
        Value::String { .. } => "string",
        Value::Bool(_) => "bool",
        Value::Color(_) => "color",
        Value::Enum(_) => "enum",
        Value::Url(_) => "url",
        Value::Array(_) => "array",
        Value::Binding(_) => "binding",
        Value::TwoWay(_) => "two-way",
    };
    attrs.push(("kind", kind.to_owned()));
    match value {
        Value::Measure { value, unit } => {
            attrs.push(("value", number(*value)));
            attrs.push(("unit", unit.as_str().to_owned()));
        }
        Value::String { text, translatable } => {
            attrs.push(("value", text.clone()));
            if *translatable {
                attrs.push(("translatable", "true".to_owned()));
            }
        }
        Value::Binding(b) | Value::TwoWay(b) => {
            attrs.push(("expr", b.expr.clone()));
            if !b.references.is_empty() {
                attrs.push(("references", b.references.join(" ")));
            }
        }
        other => attrs.push(("value", value_text(other))),
    }
}

fn node(m: &mut Markup, n: &Node) {
    let mut attrs = Vec::new();
    if let Some(id) = &n.id {
        attrs.push(("id", id.clone()));
    }
    if let Some(c) = &n.component {
        attrs.push(("component", c.clone()));
    }
    attrs.push(at(&n.origin));
    let name = element_name(&n.widget);
    if name != n.widget {
        attrs.push(("type", n.widget.clone()));
    }
    m.open(&name, &attrs);

    for u in &n.unsupported {
        m.comment(&format!(
            "not converted: {} at {}:{}",
            u.what, u.origin.line, u.origin.column
        ));
        m.empty("unsupported", &[("what", u.what.clone()), at(&u.origin)]);
    }
    match &n.structure {
        Some(Structure::Repeat { item, index, model }) => {
            let mut attrs = vec![("model", model.expr.clone())];
            if let Some(item) = item {
                attrs.push(("item", item.clone()));
            }
            if let Some(index) = index {
                attrs.push(("index", index.clone()));
            }
            m.empty("for", &attrs);
        }
        Some(Structure::Condition { condition }) => {
            m.empty("if", &[("condition", condition.expr.clone())]);
        }
        None => {}
    }
    if let Some(l) = &n.layout {
        layout(m, l);
    }
    item(m, &n.item);
    for p in &n.declarations {
        declaration(m, p);
    }
    for p in &n.properties {
        property(m, p);
    }
    for h in &n.handlers {
        let kind = match h.kind {
            HandlerKind::Callback => "callback",
            HandlerKind::Changed => "changed",
        };
        let mut attrs = vec![("event", h.name.clone()), ("kind", kind.to_owned())];
        if !h.params.is_empty() {
            attrs.push(("params", h.params.join(", ")));
        }
        attrs.push(("body", h.body.clone()));
        attrs.push(at(&h.origin));
        m.empty("handler", &attrs);
    }
    for child in &n.children {
        node(m, child);
    }
    m.close(&name);
}

/// The Slint name when it is a valid XML name (`Text`, `HorizontalLayout`),
/// otherwise `Element` with the name kept in `type`.
fn element_name(widget: &str) -> String {
    let valid = widget.chars().next().is_some_and(|c| c.is_alphabetic())
        && widget
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    if valid {
        widget.to_owned()
    } else {
        "Element".to_owned()
    }
}

fn property(m: &mut Markup, p: &Property) {
    let mut attrs = vec![("name", p.name.clone())];
    value_attrs(&mut attrs, &p.value);
    attrs.push(at(&p.origin));
    match &p.value {
        Value::Array(items) => {
            m.open("property", &attrs);
            for v in items {
                let mut attrs = Vec::new();
                value_attrs(&mut attrs, v);
                m.empty("item", &attrs);
            }
            m.close("property");
        }
        _ => m.empty("property", &attrs),
    }
}

fn layout(m: &mut Markup, l: &Layout) {
    let kind = match l.kind {
        LayoutKind::Horizontal => "horizontal",
        LayoutKind::Vertical => "vertical",
        LayoutKind::Grid => "grid",
    };
    let mut attrs = vec![("kind", kind.to_owned())];
    let fields = [
        ("spacing", &l.spacing),
        ("spacing-horizontal", &l.spacing_horizontal),
        ("spacing-vertical", &l.spacing_vertical),
        ("padding-left", &l.padding.left),
        ("padding-right", &l.padding.right),
        ("padding-top", &l.padding.top),
        ("padding-bottom", &l.padding.bottom),
    ];
    push_values(&mut attrs, &fields);
    if let Some(a) = l.alignment {
        attrs.push(("alignment", a.as_str().to_owned()));
    }
    m.empty("layout", &attrs);
}

fn push_values<'a>(attrs: &mut Vec<(&'a str, String)>, fields: &[(&'a str, &Option<Value>)]) {
    for (name, value) in fields {
        if let Some(v) = value {
            attrs.push((name, value_text(v)));
        }
    }
}

fn item(m: &mut Markup, i: &LayoutItem) {
    for (axis, e) in [("width", &i.width), ("height", &i.height)] {
        if e.is_empty() {
            continue;
        }
        let mut attrs: Vec<(&str, String)> = vec![("axis", axis.to_owned())];
        let fields = [
            ("fixed", &e.fixed),
            ("min", &e.min),
            ("max", &e.max),
            ("preferred", &e.preferred),
        ];
        push_values(&mut attrs, &fields);
        if let Some(s) = e.stretch {
            attrs.push(("stretch", number(s)));
        }
        m.empty("size", &attrs);
    }
    if i.x.is_some() || i.y.is_some() {
        let mut attrs = Vec::new();
        push_values(&mut attrs, &[("x", &i.x), ("y", &i.y)]);
        m.empty("position", &attrs);
    }
    if let Some(c) = i.cell {
        let attrs: &Attrs<'_> = &[
            ("row", c.row.to_string()),
            ("col", c.col.to_string()),
            ("rowspan", c.rowspan.to_string()),
            ("colspan", c.colspan.to_string()),
        ];
        m.empty("cell", attrs);
    }
}
