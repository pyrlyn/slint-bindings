//! The `html` backend: one self-contained page. Box layouts become CSS flex,
//! `GridLayout` becomes CSS grid, elements outside layouts are positioned the
//! way Slint does (explicit `x`/`y`, otherwise centred, filling the parent when
//! the element fills by default). Bindings, handlers, `for` and `if` become
//! `data-*` attributes; ids stay ids.

use std::collections::HashSet;

use super::{Backend, Markup, escape, number, source_tag, value_text};
use crate::diag::Diagnostics;
use crate::ir::*;
use crate::mapping::{self, Role, Widget, stretch};

pub struct Html;

impl Backend for Html {
    fn emit(&self, document: &Document, diagnostics: &mut Diagnostics) -> String {
        let root_callbacks = document
            .root
            .as_ref()
            .and_then(|r| r.component.as_ref())
            .and_then(|name| document.components.iter().find(|c| &c.name == name))
            .map(|c| c.callbacks.iter().map(|cb| cb.name.clone()).collect())
            .unwrap_or_default();
        let mut e = Emitter {
            m: Markup::default(),
            diagnostics,
            ids: HashSet::new(),
            root_callbacks,
        };
        let title = document
            .root
            .as_ref()
            .and_then(|r| r.property("title"))
            .and_then(Value::as_text)
            .map(str::to_owned)
            .or_else(|| document.root.as_ref().and_then(|r| r.component.clone()))
            .unwrap_or_default();
        e.m.raw_line("<!DOCTYPE html>");
        e.m.open("html", &[]);
        e.m.open("head", &[]);
        e.m.raw_line(r#"<meta charset="utf-8"/>"#);
        e.m.raw_line(r#"<meta name="generator" content="slint-convert"/>"#);
        e.m.raw_line(&format!("<title>{}</title>", escape(&title)));
        e.m.raw_line(&format!("<style>{BASE_CSS}</style>"));
        e.m.close("head");
        e.m.open("body", &[]);
        if let Some(root) = &document.root {
            e.node(root, Parent::Root);
        }
        e.m.close("body");
        e.m.close("html");
        e.m.finish()
    }
}

/// Resets that make browser defaults behave like Slint's: no margins, border
/// boxes, controls without their own minimum sizes leaking in.
const BASE_CSS: &str = "html,body{margin:0;height:100%}\
body{font-family:system-ui,sans-serif;font-size:14px}\
[data-slint]{box-sizing:border-box;margin:0}\
[data-slint=Text]{display:flex;flex-direction:column;white-space:pre}";

#[derive(Clone, Copy)]
enum Parent {
    Root,
    /// A non-layout element: children are positioned absolutely.
    Free,
    Flex {
        horizontal: bool,
        /// Stretch factors are ignored when the layout aligns instead.
        stretches: bool,
        /// Slint stretches every child when all factors are zero.
        all_zero: bool,
    },
    Grid,
}

struct Emitter<'a> {
    m: Markup,
    diagnostics: &'a mut Diagnostics,
    ids: HashSet<String>,
    /// The main component's callbacks, listed on the root element.
    root_callbacks: Vec<String>,
}

/// The element being built for one node.
struct El {
    tag: &'static str,
    attrs: Vec<(String, String)>,
    style: Vec<(&'static str, String)>,
    /// Elements written before the children: an `<input>` inside a `<label>`,
    /// `<option>`s, a `<legend>`.
    inner: Vec<Inner>,
    text: Option<String>,
    void: bool,
}

struct Inner {
    tag: &'static str,
    attrs: Vec<(String, String)>,
    text: Option<String>,
}

impl El {
    fn attr(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attrs.push((key.into(), value.into()));
    }

    fn css(&mut self, key: &'static str, value: impl Into<String>) {
        self.style.push((key, value.into()));
    }
}

/// A length or number the CSS side can use as is.
fn css_length(value: &Value) -> Option<String> {
    match value {
        Value::Measure { value, unit } => Some(match unit {
            Unit::Phx => format!("{}px", number(*value)),
            Unit::Px | Unit::Cm | Unit::Mm | Unit::In | Unit::Pt | Unit::Rem | Unit::Percent => {
                format!("{}{}", number(*value), unit.as_str())
            }
            _ => return None,
        }),
        Value::Number(n) if *n == 0.0 => Some("0".to_owned()),
        _ => None,
    }
}

impl Emitter<'_> {
    fn node(&mut self, n: &Node, parent: Parent) {
        let widget = mapping::lookup(&n.widget);
        let mut el = El {
            tag: widget.map_or("div", |w| if w.html.is_empty() { "div" } else { w.html }),
            attrs: Vec::new(),
            style: Vec::new(),
            inner: Vec::new(),
            text: None,
            void: false,
        };
        if let Some(id) = &n.id {
            let mut unique = id.clone();
            let mut k = 2;
            while !self.ids.insert(unique.clone()) {
                unique = format!("{id}-{k}");
                k += 1;
            }
            el.attr("id", unique);
        }
        el.attr("data-slint", n.widget.clone());
        if let Some(c) = &n.component {
            el.attr("data-component", c.clone());
        }
        el.attr("data-at", source_tag(&n.origin));
        match &n.structure {
            Some(Structure::Repeat { item, index, model }) => {
                el.attr("data-for", model.expr.clone());
                if let Some(item) = item {
                    el.attr("data-for-item", item.clone());
                }
                if let Some(index) = index {
                    el.attr("data-for-index", index.clone());
                }
            }
            Some(Structure::Condition { condition }) => {
                el.attr("data-if", condition.expr.clone());
            }
            None => {}
        }

        self.placement(&mut el, n, parent, widget);
        if let Some(l) = &n.layout {
            self.layout(&mut el, l, n);
        } else if !n.children.is_empty() || matches!(parent, Parent::Root) {
            el.css("position", "relative");
        }

        let role = widget.map(|w| w.role);
        // A `ComboBox` needs its options before `current-index` selects one.
        let mut properties: Vec<&Property> = n.properties.iter().collect();
        properties.sort_by_key(|p| p.name != "model");
        for p in properties {
            self.property(&mut el, n, role, p);
        }
        self.finish_role(&mut el, n, role);
        for d in &n.declarations {
            let value = d.default.as_ref().map(value_text).unwrap_or_default();
            el.attr(format!("data-prop-{}", d.name), value);
        }
        if matches!(parent, Parent::Root) && !self.root_callbacks.is_empty() {
            el.attr("data-callbacks", self.root_callbacks.join(" "));
        }
        for h in &n.handlers {
            let key = match h.kind {
                HandlerKind::Callback => format!("data-on-{}", h.name),
                HandlerKind::Changed => format!("data-changed-{}", h.name),
            };
            if !h.params.is_empty() {
                el.attr(format!("data-params-{}", h.name), h.params.join(" "));
            }
            el.attr(key, h.body.clone());
        }

        for u in &n.unsupported {
            self.m.comment(&format!(
                "not converted: {} at {}:{}",
                u.what, u.origin.line, u.origin.column
            ));
        }
        self.write(el, n);
    }

    /// Position and size inside the parent.
    fn placement(&mut self, el: &mut El, n: &Node, parent: Parent, widget: Option<&Widget>) {
        let item = &n.item;
        match parent {
            Parent::Root => {
                if item.width.fixed.is_none() {
                    el.css("width", "100%");
                }
                if item.height.fixed.is_none() {
                    el.css("height", "100%");
                }
            }
            Parent::Free => {
                let fills = widget.is_none_or(|w| w.fills_parent);
                el.css("position", "absolute");
                let mut translate = (false, false);
                for (horizontal, pos, extent, start, end) in [
                    (true, &item.x, &item.width, "left", "right"),
                    (false, &item.y, &item.height, "top", "bottom"),
                ] {
                    if let Some(p) = pos.as_ref().and_then(css_length) {
                        el.css(start, p);
                    } else if extent.fixed.is_none() && fills {
                        el.css(start, "0");
                        el.css(end, "0");
                    } else {
                        el.css(start, "50%");
                        if horizontal {
                            translate.0 = true;
                        } else {
                            translate.1 = true;
                        }
                    }
                }
                match translate {
                    (true, true) => el.css("transform", "translate(-50%,-50%)"),
                    (true, false) => el.css("transform", "translateX(-50%)"),
                    (false, true) => el.css("transform", "translateY(-50%)"),
                    (false, false) => {}
                }
            }
            Parent::Flex {
                horizontal,
                stretches,
                all_zero,
            } => {
                let main = if horizontal {
                    &item.width
                } else {
                    &item.height
                };
                let grow = if !stretches || mapping::fixed(n, horizontal) {
                    0.0
                } else if all_zero {
                    1.0
                } else {
                    stretch(n, horizontal)
                };
                let basis = main
                    .fixed
                    .as_ref()
                    .or(main.preferred.as_ref())
                    .and_then(css_length)
                    .unwrap_or_else(|| "auto".to_owned());
                let shrink = if main.fixed.is_some() { "0" } else { "1" };
                el.css("flex", format!("{} {shrink} {basis}", number(grow)));
            }
            Parent::Grid => {
                if let Some(c) = item.cell {
                    el.css("grid-row", format!("{} / span {}", c.row + 1, c.rowspan));
                    el.css("grid-column", format!("{} / span {}", c.col + 1, c.colspan));
                }
            }
        }
        for (extent, size, min, max) in [
            (&item.width, "width", "min-width", "max-width"),
            (&item.height, "height", "min-height", "max-height"),
        ] {
            if let Some(v) = extent.fixed.as_ref().and_then(css_length) {
                el.css(size, v);
            } else if !matches!(parent, Parent::Flex { .. })
                && let Some(v) = extent.preferred.as_ref().and_then(css_length)
            {
                el.css(size, v);
            }
            if let Some(v) = extent.min.as_ref().and_then(css_length) {
                el.css(min, v);
            }
            if let Some(v) = extent.max.as_ref().and_then(css_length) {
                el.css(max, v);
            }
        }
        for (name, value) in [
            ("x", &item.x),
            ("y", &item.y),
            ("width", &item.width.fixed),
            ("height", &item.height.fixed),
        ] {
            if let Some(v @ (Value::Binding(_) | Value::TwoWay(_))) = value {
                el.attr(format!("data-bind-{name}"), value_text(v));
            }
        }
    }

    fn layout(&mut self, el: &mut El, l: &Layout, n: &Node) {
        let pad = |v: &Option<Value>| {
            v.as_ref()
                .and_then(css_length)
                .unwrap_or_else(|| "0".to_owned())
        };
        let p = &l.padding;
        if [&p.left, &p.right, &p.top, &p.bottom]
            .iter()
            .any(|v| v.is_some())
        {
            el.css(
                "padding",
                format!(
                    "{} {} {} {}",
                    pad(&p.top),
                    pad(&p.right),
                    pad(&p.bottom),
                    pad(&p.left)
                ),
            );
        }
        match l.kind {
            LayoutKind::Horizontal | LayoutKind::Vertical => {
                let horizontal = l.kind == LayoutKind::Horizontal;
                el.css("display", "flex");
                el.css("flex-direction", if horizontal { "row" } else { "column" });
                if let Some(s) = l.spacing.as_ref().and_then(css_length) {
                    el.css("gap", s);
                }
                let justify = match l.alignment.unwrap_or(Alignment::Stretch) {
                    Alignment::Stretch | Alignment::Start => None,
                    Alignment::Center => Some("center"),
                    Alignment::End => Some("flex-end"),
                    Alignment::SpaceBetween => Some("space-between"),
                    Alignment::SpaceAround => Some("space-around"),
                    Alignment::SpaceEvenly => Some("space-evenly"),
                };
                if let Some(j) = justify {
                    el.css("justify-content", j);
                }
            }
            LayoutKind::Grid => {
                el.css("display", "grid");
                let gap = l.spacing.as_ref().and_then(css_length);
                let column_gap = l
                    .spacing_horizontal
                    .as_ref()
                    .and_then(css_length)
                    .or(gap.clone());
                let row_gap = l.spacing_vertical.as_ref().and_then(css_length).or(gap);
                if let Some(g) = row_gap {
                    el.css("row-gap", g);
                }
                if let Some(g) = column_gap {
                    el.css("column-gap", g);
                }
                el.css("grid-template-columns", tracks(n, true));
                el.css("grid-template-rows", tracks(n, false));
            }
        }
        for (name, value) in [("spacing", &l.spacing), ("alignment", &None)] {
            if let Some(v @ Value::Binding(_)) = value {
                el.attr(format!("data-bind-{name}"), value_text(v));
            }
        }
    }

    fn property(&mut self, el: &mut El, n: &Node, role: Option<Role>, p: &Property) {
        if matches!(p.value, Value::Binding(_) | Value::TwoWay(_)) {
            let key = match p.value {
                Value::TwoWay(_) => format!("data-sync-{}", p.name),
                _ => format!("data-bind-{}", p.name),
            };
            let expr = match &p.value {
                Value::Binding(b) | Value::TwoWay(b) => b.expr.clone(),
                _ => String::new(),
            };
            el.attr(key, expr);
            return;
        }
        if let Value::String {
            translatable: true, ..
        } = p.value
        {
            el.attr(format!("data-tr-{}", p.name), "");
        }
        if self.role_property(el, role, p) || common_property(el, p) {
            return;
        }
        // Window title is already the document title.
        if role == Some(Role::Window) && p.name == "title" {
            return;
        }
        self.diagnostics.warn(
            &p.origin,
            format!(
                "property `{}` of `{}` has no HTML equivalent; kept as data-prop-{}",
                p.name, n.widget, p.name
            ),
        );
        el.attr(format!("data-prop-{}", p.name), value_text(&p.value));
    }

    /// Properties whose meaning depends on the widget. Returns whether `p`
    /// was taken.
    fn role_property(&mut self, el: &mut El, role: Option<Role>, p: &Property) -> bool {
        let v = &p.value;
        match (role, p.name.as_str()) {
            (Some(Role::Text), "text") => el.text = v.as_text().map(str::to_owned),
            (Some(Role::Text), "horizontal-alignment") => {
                el.css("text-align", value_text(v));
            }
            (Some(Role::Text), "vertical-alignment") => {
                let j = match value_text(v).as_str() {
                    "center" => "center",
                    "bottom" => "flex-end",
                    _ => "flex-start",
                };
                el.css("justify-content", j);
            }
            (Some(Role::Text), "wrap") => {
                if value_text(v) != "no-wrap" {
                    el.css("white-space", "pre-wrap");
                }
            }
            (Some(Role::Text), "overflow") => {
                if value_text(v) == "elide" {
                    el.css("overflow", "hidden");
                    el.css("text-overflow", "ellipsis");
                }
            }
            (Some(Role::Image), "source") => match v {
                Value::Url(url) => el.attr("src", url.clone()),
                _ => return false,
            },
            (Some(Role::Image), "image-fit") => {
                let fit = match value_text(v).as_str() {
                    "contain" => "contain",
                    "cover" => "cover",
                    "preserve" => "none",
                    _ => "fill",
                };
                el.css("object-fit", fit);
            }
            (Some(Role::Button), "text") => el.text = v.as_text().map(str::to_owned),
            (Some(Role::CheckBox | Role::Switch), "text") => {
                el.text = v.as_text().map(str::to_owned);
            }
            (Some(Role::CheckBox | Role::Switch), "checked") => {
                if v == &Value::Bool(true) {
                    el.inner_attr("checked", "");
                }
            }
            (Some(Role::LineEdit), "text") => el.attr("value", value_text(v)),
            (Some(Role::LineEdit), "placeholder-text") => el.attr("placeholder", value_text(v)),
            (Some(Role::LineEdit), "input-type") => {
                if value_text(v) == "password" {
                    el.attr("type", "password");
                }
            }
            (Some(Role::LineEdit | Role::TextEdit), "read-only") => {
                if v == &Value::Bool(true) {
                    el.attr("readonly", "");
                }
            }
            (Some(Role::TextEdit), "text") => el.text = v.as_text().map(str::to_owned),
            (Some(Role::TextEdit), "placeholder-text") => el.attr("placeholder", value_text(v)),
            (Some(Role::Slider | Role::SpinBox), "minimum") => el.attr("min", value_text(v)),
            (Some(Role::Slider | Role::SpinBox), "maximum") => el.attr("max", value_text(v)),
            (Some(Role::Slider | Role::SpinBox | Role::Progress), "value") => {
                el.attr("value", value_text(v));
            }
            (Some(Role::Slider), "step") => el.attr("step", value_text(v)),
            (Some(Role::Progress), "progress") => el.attr("value", value_text(v)),
            (Some(Role::Progress), "indeterminate") => {}
            (Some(Role::ComboBox), "model") => {
                let Value::Array(items) = v else { return false };
                for item in items {
                    el.inner.push(Inner {
                        tag: "option",
                        attrs: Vec::new(),
                        text: Some(value_text(item)),
                    });
                }
            }
            (Some(Role::ComboBox), "current-index") => {
                let index = v.as_number().map_or(0, |i| i as usize);
                if let Some(o) = el.inner.iter_mut().filter(|i| i.tag == "option").nth(index) {
                    o.attrs.push(("selected".to_owned(), String::new()));
                }
            }
            (Some(Role::ComboBox), "current-value") => {}
            (Some(Role::GroupBox), "title") => el.inner.push(Inner {
                tag: "legend",
                attrs: Vec::new(),
                text: Some(value_text(v)),
            }),
            (Some(Role::Window), "background") => el.css("background", value_text(v)),
            _ => return false,
        }
        true
    }

    /// Per-role attributes that do not come from a property.
    fn finish_role(&mut self, el: &mut El, n: &Node, role: Option<Role>) {
        match role {
            Some(Role::Image) => {
                el.void = true;
                el.attr("alt", "");
            }
            Some(Role::Button) => el.attr("type", "button"),
            Some(Role::CheckBox | Role::Switch) => {
                el.tag = "label";
                let mut attrs = vec![("type".to_owned(), "checkbox".to_owned())];
                if role == Some(Role::Switch) {
                    attrs.push(("role".to_owned(), "switch".to_owned()));
                }
                let checked = el.attrs.iter().position(|(k, _)| k == "data-inner-checked");
                if let Some(i) = checked {
                    el.attrs.remove(i);
                    attrs.push(("checked".to_owned(), String::new()));
                }
                el.inner.insert(
                    0,
                    Inner {
                        tag: "input",
                        attrs,
                        text: None,
                    },
                );
            }
            Some(Role::LineEdit) => {
                el.void = true;
                if !el.attrs.iter().any(|(k, _)| k == "type") {
                    el.attr("type", "text");
                }
            }
            Some(Role::Slider) => {
                el.void = true;
                el.attr("type", "range");
            }
            Some(Role::SpinBox) => {
                el.void = true;
                el.attr("type", "number");
            }
            Some(Role::Progress) => {
                if !el.attrs.iter().any(|(k, _)| k == "max") {
                    el.attr("max", "1");
                }
            }
            Some(Role::Spinner) => {
                el.attr("role", "progressbar");
                el.attr("aria-busy", "true");
            }
            Some(Role::Scroll) => el.css("overflow", "auto"),
            _ => {}
        }
        if el.void && !n.children.is_empty() {
            self.diagnostics.warn(
                &n.origin,
                format!("children of `{}` are not converted to HTML", n.widget),
            );
        }
    }

    fn write(&mut self, el: El, n: &Node) {
        let mut attrs: Vec<(&str, String)> = el
            .attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        if !el.style.is_empty() {
            let style: Vec<String> = el.style.iter().map(|(k, v)| format!("{k}:{v}")).collect();
            attrs.push(("style", style.join(";")));
        }
        if el.void {
            self.m.empty(el.tag, &attrs);
            return;
        }
        let parent = child_parent(n);
        if el.inner.is_empty() && n.children.is_empty() {
            self.m
                .text_element(el.tag, &attrs, el.text.as_deref().unwrap_or_default());
            return;
        }
        self.m.open(el.tag, &attrs);
        for inner in &el.inner {
            let attrs: Vec<(&str, String)> = inner
                .attrs
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect();
            match &inner.text {
                Some(t) => self.m.text_element(inner.tag, &attrs, t),
                None => self.m.empty(inner.tag, &attrs),
            }
        }
        if let Some(t) = &el.text {
            self.m.raw_line(&escape(t));
        }
        for child in &n.children {
            self.node(child, parent);
        }
        self.m.close(el.tag);
    }
}

impl El {
    /// Marks an attribute for the inner `<input>` built in `finish_role`.
    fn inner_attr(&mut self, key: &str, value: &str) {
        self.attrs
            .push((format!("data-inner-{key}"), value.to_owned()));
    }
}

fn child_parent(n: &Node) -> Parent {
    match &n.layout {
        Some(l) if l.kind == LayoutKind::Grid => Parent::Grid,
        Some(l) => {
            let horizontal = l.kind == LayoutKind::Horizontal;
            Parent::Flex {
                horizontal,
                stretches: matches!(l.alignment, None | Some(Alignment::Stretch)),
                all_zero: mapping::all_stretch_zero(&n.children, horizontal),
            }
        }
        None => Parent::Free,
    }
}

/// `grid-template-columns` (or rows): a track per line, `Nfr` for a line
/// with a stretch, `auto` otherwise, and `0px` for a line no cell covers.
/// CSS grows `auto` tracks alike when no track has an `fr`, as Slint grows
/// its lines when none has a stretch.
fn tracks(n: &Node, horizontal: bool) -> String {
    let lines = mapping::grid_tracks(n, horizontal);
    if lines.is_empty() {
        return "auto".to_owned();
    }
    lines
        .iter()
        .map(|line| match line {
            Some(s) if *s > 0.0 => format!("{}fr", number(*s)),
            Some(_) => "auto".to_owned(),
            None => "0px".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Visual properties every widget understands. Returns whether `p` was taken.
fn common_property(el: &mut El, p: &Property) -> bool {
    let v = &p.value;
    let length = || css_length(v);
    match p.name.as_str() {
        "background" => el.css("background", value_text(v)),
        "color" => el.css("color", value_text(v)),
        "opacity" => el.css("opacity", value_text(v)),
        "visible" => {
            if v == &Value::Bool(false) {
                el.css("visibility", "hidden");
            }
        }
        "clip" => {
            if v == &Value::Bool(true) {
                el.css("overflow", "hidden");
            }
        }
        "enabled" => {
            if v == &Value::Bool(false) {
                el.attr("disabled", "");
            }
        }
        "border-radius" => match length() {
            Some(l) => el.css("border-radius", l),
            None => return false,
        },
        "border-width" => match length() {
            Some(l) => {
                el.css("border-width", l);
                el.css("border-style", "solid");
            }
            None => return false,
        },
        "border-color" => el.css("border-color", value_text(v)),
        "font-size" => match length() {
            Some(l) => el.css("font-size", l),
            None => return false,
        },
        "font-weight" => el.css("font-weight", value_text(v)),
        "font-family" => el.css("font-family", format!("\"{}\"", value_text(v))),
        "font-italic" => {
            if v == &Value::Bool(true) {
                el.css("font-style", "italic");
            }
        }
        "letter-spacing" => match length() {
            Some(l) => el.css("letter-spacing", l),
            None => return false,
        },
        "accessible-label" => el.attr("aria-label", value_text(v)),
        "accessible-description" => el.attr("aria-description", value_text(v)),
        _ => return false,
    }
    true
}
