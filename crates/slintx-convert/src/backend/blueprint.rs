//! The `blueprint` backend: GTK 4 Blueprint. Box layouts become `Gtk.Box`,
//! `GridLayout` becomes `Gtk.Grid`, elements outside layouts become overlay
//! children of a `Gtk.Overlay` placed with `halign`/`valign` and margins.
//! Callbacks become signal handlers (`clicked => $on_ok_clicked();`); property
//! bindings, which GTK cannot evaluate, stay as comments next to the object.

use std::collections::HashSet;
use std::fmt::Write as _;

use super::{Backend, number, source_tag, value_text};
use crate::diag::{Diagnostics, Origin};
use crate::ir::*;
use crate::mapping::{self, Role, stretch};

pub struct Blueprint {
    pub source_tags: bool,
}

impl Backend for Blueprint {
    fn emit(&self, document: &Document, diagnostics: &mut Diagnostics) -> String {
        let mut e = Emitter {
            out: String::new(),
            depth: 0,
            diagnostics,
            ids: HashSet::new(),
            handlers: HashSet::new(),
            source_tags: self.source_tags,
        };
        e.line("using Gtk 4.0;");
        e.line("");
        e.line(&format!(
            "// Converted from {} by slint-convert.",
            document.file
        ));
        if let Some(root) = &document.root {
            if let Some(info) = root
                .component
                .as_ref()
                .and_then(|name| document.components.iter().find(|c| &c.name == name))
            {
                e.interface(info, root);
            }
            e.node(root, Parent::Top, "");
        }
        e.out
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Parent {
    Top,
    /// The only child of a window, frame or scrolled window: it fills.
    Single,
    Box {
        horizontal: bool,
        stretches: bool,
        all_zero: bool,
    },
    /// A grid cell, and whether a line it covers grows on either axis.
    Grid {
        hexpand: bool,
        vexpand: bool,
    },
    Overlay,
}

struct Emitter<'a> {
    out: String,
    depth: usize,
    diagnostics: &'a mut Diagnostics,
    ids: HashSet<String>,
    handlers: HashSet<String>,
    source_tags: bool,
}

/// Blueprint lines for one object, collected before it is written.
#[derive(Default)]
struct Obj {
    class: &'static str,
    props: Vec<String>,
    /// Signal handlers, layout blocks and nested property objects.
    extra: Vec<String>,
    comments: Vec<String>,
}

impl Obj {
    fn prop(&mut self, name: &str, value: impl std::fmt::Display) {
        self.props.push(format!("{name}: {value};"));
    }
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn string(value: &Value) -> Option<String> {
    match value {
        Value::String { text, translatable } => Some(if *translatable {
            format!("_({})", quote(text))
        } else {
            quote(text)
        }),
        _ => None,
    }
}

/// Whole logical pixels: GTK sizes, margins and spacing are integers.
fn px(value: &Value) -> Option<i64> {
    value.as_px().map(|v| v.round() as i64)
}

/// A Blueprint identifier: Slint ids may contain `-`, Blueprint ids may not.
fn ident(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// The GTK signal for a Slint callback, by role.
fn signal(role: Option<Role>, callback: &str) -> Option<&'static str> {
    Some(match (role?, callback) {
        (Role::Button, "clicked") => "clicked",
        (Role::CheckBox, "toggled") => "toggled",
        (Role::Switch, "toggled") => "notify::active",
        (Role::LineEdit, "accepted") => "activate",
        (Role::LineEdit, "edited") => "changed",
        (Role::Slider, "changed") => "value-changed",
        (Role::SpinBox, "edited") => "value-changed",
        (Role::ComboBox, "selected") => "notify::selected",
        _ => return None,
    })
}

/// The GTK property a Slint property is shown in, for `changed` handlers.
fn gtk_property(role: Option<Role>, property: &str) -> Option<&'static str> {
    Some(match (role?, property) {
        (Role::Text | Role::Button | Role::CheckBox, "text") => "label",
        (Role::LineEdit, "text") => "text",
        (Role::CheckBox | Role::Switch, "checked") => "active",
        (Role::Slider | Role::SpinBox, "value") => "value",
        (Role::ComboBox, "current-index") => "selected",
        (_, "visible") => "visible",
        (_, "enabled") => "sensitive",
        _ => return None,
    })
}

impl Emitter<'_> {
    fn line(&mut self, text: &str) {
        if !text.is_empty() {
            for _ in 0..self.depth {
                self.out.push_str("  ");
            }
        }
        let _ = writeln!(self.out, "{text}");
    }

    fn warn(&mut self, origin: &Origin, message: String) {
        self.diagnostics.warn(origin, message);
    }

    fn interface(&mut self, info: &ComponentInfo, root: &Node) {
        let mut lines = Vec::new();
        for p in &root.declarations {
            let vis = p
                .visibility
                .as_deref()
                .map(|v| format!("{v} "))
                .unwrap_or_default();
            let default = p
                .default
                .as_ref()
                .map(|d| format!(" = {}", value_text(d)))
                .unwrap_or_default();
            lines.push(format!("//   {vis}property <{}> {}{default}", p.ty, p.name));
        }
        for c in &info.callbacks {
            lines.push(format!("//   callback {}({})", c.name, c.params.join(", ")));
        }
        if !lines.is_empty() {
            self.line(&format!("// Interface of {}:", info.name));
            for l in lines {
                self.line(&l);
            }
        }
        self.line("");
    }

    fn unique_id(&mut self, n: &Node) -> Option<String> {
        let id = ident(n.id.as_ref()?);
        let mut unique = id.clone();
        let mut k = 2;
        while !self.ids.insert(unique.clone()) {
            unique = format!("{id}_{k}");
            k += 1;
        }
        Some(unique)
    }

    /// Writes `n` as an object; `prefix` is `[overlay] ` for overlay children.
    fn node(&mut self, n: &Node, parent: Parent, prefix: &str) {
        let widget = mapping::lookup(&n.widget);
        let role = widget.map(|w| w.role);
        let free_container = n.layout.is_none() && !n.children.is_empty();
        let mut obj = Obj {
            class: match role {
                Some(Role::Container) if free_container => "Gtk.Overlay",
                _ => widget.and_then(|w| w.gtk).unwrap_or("Gtk.Box"),
            },
            ..Obj::default()
        };
        for u in &n.unsupported {
            obj.comments.push(format!(
                "not converted: {} at {}:{}",
                u.what, u.origin.line, u.origin.column
            ));
        }
        match &n.structure {
            Some(Structure::Repeat { item, index, model }) => {
                let item = item.as_deref().unwrap_or("_");
                let index = index.as_ref().map(|i| format!("[{i}]")).unwrap_or_default();
                obj.comments.push(format!(
                    "for {item}{index} in {}: template, emitted once",
                    model.expr
                ));
            }
            Some(Structure::Condition { condition }) => {
                obj.comments.push(format!("if {}", condition.expr));
            }
            None => {}
        }

        if self.source_tags {
            obj.prop("name", quote(&source_tag(&n.origin)));
        }
        self.placement(&mut obj, n, parent, widget.is_none_or(|w| w.fills_parent));
        if let Some(l) = &n.layout {
            self.layout(&mut obj, l, n);
        }
        for p in &n.properties {
            self.property(&mut obj, n, role, p);
        }
        self.finish_role(&mut obj, n, role);
        for h in &n.handlers {
            self.handler(&mut obj, n, role, h);
        }

        let id = self.unique_id(n);
        for c in &obj.comments {
            self.line(&format!("// {c}"));
        }
        let head = match &id {
            Some(id) => format!("{prefix}{} {id} {{", obj.class),
            None => format!("{prefix}{} {{", obj.class),
        };
        self.line(&head);
        self.depth += 1;
        for p in &obj.props {
            self.line(p);
        }
        for x in &obj.extra {
            for l in x.lines() {
                self.line(l);
            }
        }
        self.children(n, role, obj.class);
        self.depth -= 1;
        self.line("}");
    }

    fn children(&mut self, n: &Node, role: Option<Role>, class: &str) {
        if n.children.is_empty() {
            return;
        }
        let single = matches!(role, Some(Role::Window | Role::Scroll | Role::GroupBox));
        if class == "Gtk.Overlay" {
            for c in &n.children {
                self.node(c, Parent::Overlay, "[overlay] ");
            }
            return;
        }
        if single {
            if n.children.len() == 1 {
                self.node(&n.children[0], Parent::Single, "");
            } else {
                // Window, frame and scrolled window take one child; Slint
                // positions several freely, which an overlay keeps.
                self.line("Gtk.Overlay {");
                self.depth += 1;
                for c in &n.children {
                    self.node(c, Parent::Overlay, "[overlay] ");
                }
                self.depth -= 1;
                self.line("}");
            }
            return;
        }
        let parent = match &n.layout {
            Some(l) if l.kind == LayoutKind::Grid => {
                let columns = mapping::grid_growth(n, true);
                let rows = mapping::grid_growth(n, false);
                for c in &n.children {
                    let grows = |lines: &[bool], start: u32, span: u32| {
                        lines
                            .iter()
                            .skip(start as usize)
                            .take(span.max(1) as usize)
                            .any(|g| *g)
                    };
                    let (hexpand, vexpand) = c.item.cell.map_or((false, false), |cell| {
                        (
                            grows(&columns, cell.col, cell.colspan),
                            grows(&rows, cell.row, cell.rowspan),
                        )
                    });
                    self.node(c, Parent::Grid { hexpand, vexpand }, "");
                }
                return;
            }
            Some(l) => {
                let horizontal = l.kind == LayoutKind::Horizontal;
                Parent::Box {
                    horizontal,
                    stretches: matches!(l.alignment, None | Some(Alignment::Stretch)),
                    all_zero: mapping::all_stretch_zero(&n.children, horizontal),
                }
            }
            None => {
                self.warn(
                    &n.origin,
                    format!("children of `{}` are not converted to GTK", n.widget),
                );
                return;
            }
        };
        if let Parent::Box {
            horizontal,
            stretches: true,
            all_zero: false,
        } = parent
        {
            let mut factors: Vec<f64> = n
                .children
                .iter()
                .map(|c| stretch(c, horizontal))
                .filter(|s| *s > 0.0)
                .collect();
            factors.dedup();
            if factors.len() > 1 {
                self.warn(
                    &n.origin,
                    "stretch ratios are not kept: GTK expands children equally".to_owned(),
                );
            }
        }
        for c in &n.children {
            self.node(c, parent, "");
        }
    }

    fn placement(&mut self, obj: &mut Obj, n: &Node, parent: Parent, fills: bool) {
        let item = &n.item;
        for (extent, request, axis) in [
            (&item.width, "width-request", "width"),
            (&item.height, "height-request", "height"),
        ] {
            let size = extent.fixed.as_ref().or(extent.min.as_ref()).and_then(px);
            if let Some(size) = size {
                obj.prop(request, size);
            }
            if let Some(v) = &extent.max {
                self.warn(
                    &n.origin,
                    format!("max-{axis} {} has no GTK equivalent", value_text(v)),
                );
            }
            if let Some(v) = &extent.preferred {
                self.warn(
                    &n.origin,
                    format!("preferred-{axis} {} has no GTK equivalent", value_text(v)),
                );
            }
        }
        match parent {
            Parent::Top if n.widget == "Window" => {
                obj.props.retain(|p| !p.contains("-request:"));
                if let Some(w) = item.width.fixed.as_ref().and_then(px) {
                    obj.prop("default-width", w);
                }
                if let Some(h) = item.height.fixed.as_ref().and_then(px) {
                    obj.prop("default-height", h);
                }
            }
            Parent::Top | Parent::Single => {}
            Parent::Box {
                horizontal,
                stretches,
                all_zero,
            } => {
                let expands = stretches
                    && !mapping::fixed(n, horizontal)
                    && (all_zero || stretch(n, horizontal) > 0.0);
                if expands {
                    obj.prop(if horizontal { "hexpand" } else { "vexpand" }, "true");
                }
                // Slint places what keeps its size across a box at the start.
                if mapping::fixed(n, !horizontal) {
                    obj.prop(if horizontal { "valign" } else { "halign" }, "start");
                }
            }
            Parent::Grid { hexpand, vexpand } => {
                if let Some(c) = item.cell {
                    let mut block = format!("layout {{\n  row: {};\n  column: {};\n", c.row, c.col);
                    if c.rowspan > 1 {
                        let _ = writeln!(block, "  row-span: {};", c.rowspan);
                    }
                    if c.colspan > 1 {
                        let _ = writeln!(block, "  column-span: {};", c.colspan);
                    }
                    block.push('}');
                    obj.extra.push(block);
                }
                // GTK grows a line when a cell in it expands.
                if hexpand && !mapping::fixed(n, true) {
                    obj.prop("hexpand", "true");
                }
                if vexpand && !mapping::fixed(n, false) {
                    obj.prop("vexpand", "true");
                }
            }
            Parent::Overlay => {
                for (pos, extent, align, margin) in [
                    (&item.x, &item.width, "halign", "margin-start"),
                    (&item.y, &item.height, "valign", "margin-top"),
                ] {
                    if let Some(p) = pos.as_ref().and_then(px) {
                        obj.prop(align, "start");
                        obj.prop(margin, p);
                    } else if extent.fixed.is_some() || !fills {
                        obj.prop(align, "center");
                    } else {
                        obj.prop(align, "fill");
                    }
                }
            }
        }
        for (name, value) in [
            ("x", &item.x),
            ("y", &item.y),
            ("width", &item.width.fixed),
            ("height", &item.height.fixed),
        ] {
            if let Some(v @ (Value::Binding(_) | Value::TwoWay(_))) = value {
                obj.comments.push(format!("{name}: {}", value_text(v)));
            }
        }
    }

    fn layout(&mut self, obj: &mut Obj, l: &Layout, n: &Node) {
        let p = &l.padding;
        for (side, value) in [
            ("margin-start", &p.left),
            ("margin-end", &p.right),
            ("margin-top", &p.top),
            ("margin-bottom", &p.bottom),
        ] {
            if let Some(v) = value.as_ref().and_then(px) {
                obj.prop(side, v);
            }
        }
        match l.kind {
            LayoutKind::Horizontal | LayoutKind::Vertical => {
                let horizontal = l.kind == LayoutKind::Horizontal;
                obj.prop(
                    "orientation",
                    if horizontal { "horizontal" } else { "vertical" },
                );
                if let Some(s) = l.spacing.as_ref().and_then(px) {
                    obj.prop("spacing", s);
                }
                let axis = if horizontal { "halign" } else { "valign" };
                match l.alignment.unwrap_or(Alignment::Stretch) {
                    Alignment::Stretch | Alignment::Start => {}
                    Alignment::Center => obj.prop(axis, "center"),
                    Alignment::End => obj.prop(axis, "end"),
                    other => self.warn(
                        &n.origin,
                        format!("alignment {} has no Gtk.Box equivalent", other.as_str()),
                    ),
                }
            }
            LayoutKind::Grid => {
                let spacing = l.spacing.as_ref().and_then(px);
                if let Some(s) = l.spacing_vertical.as_ref().and_then(px).or(spacing) {
                    obj.prop("row-spacing", s);
                }
                if let Some(s) = l.spacing_horizontal.as_ref().and_then(px).or(spacing) {
                    obj.prop("column-spacing", s);
                }
            }
        }
        if let Some(v @ Value::Binding(_)) = &l.spacing {
            obj.comments.push(format!("spacing: {}", value_text(v)));
        }
    }

    fn property(&mut self, obj: &mut Obj, n: &Node, role: Option<Role>, p: &Property) {
        let v = &p.value;
        if let Value::Binding(b) | Value::TwoWay(b) = v {
            let op = if matches!(v, Value::TwoWay(_)) {
                "<=>"
            } else {
                ":"
            };
            obj.comments
                .push(format!("binding {} {op} {}", p.name, b.expr));
            return;
        }
        let done = match (role, p.name.as_str()) {
            (Some(Role::Window), "title") => string(v).map(|s| obj.prop("title", s)).is_some(),
            (Some(Role::Text), "text") => true,
            (Some(Role::Text), "horizontal-alignment") => {
                let x = match value_text(v).as_str() {
                    "center" => "0.5",
                    "right" => "1",
                    _ => "0",
                };
                obj.props.retain(|l| !l.starts_with("xalign:"));
                obj.prop("xalign", x);
                true
            }
            (Some(Role::Text), "vertical-alignment") => {
                let y = match value_text(v).as_str() {
                    "center" => "0.5",
                    "bottom" => "1",
                    _ => "0",
                };
                obj.props.retain(|l| !l.starts_with("yalign:"));
                obj.prop("yalign", y);
                true
            }
            (Some(Role::Text), "wrap") => {
                if value_text(v) != "no-wrap" {
                    obj.prop("wrap", "true");
                }
                true
            }
            (Some(Role::Text), "overflow") => {
                if value_text(v) == "elide" {
                    obj.prop("ellipsize", "end");
                }
                true
            }
            (
                Some(Role::Text),
                "color" | "font-size" | "font-weight" | "font-family" | "font-italic",
            ) => true,
            (Some(Role::Image), "source") => match v {
                Value::Url(url) => {
                    obj.prop("file", quote(url));
                    true
                }
                _ => false,
            },
            (Some(Role::Image), "image-fit") => {
                let fit = match value_text(v).as_str() {
                    "contain" => "contain",
                    "cover" => "cover",
                    "preserve" => "scale-down",
                    _ => "fill",
                };
                obj.prop("content-fit", fit);
                true
            }
            (Some(Role::Button | Role::CheckBox), "text") => {
                string(v).map(|s| obj.prop("label", s)).is_some()
            }
            (Some(Role::CheckBox | Role::Switch), "checked") => {
                obj.prop("active", value_text(v));
                true
            }
            (Some(Role::LineEdit), "text") => string(v).map(|s| obj.prop("text", s)).is_some(),
            (Some(Role::LineEdit), "placeholder-text") => {
                string(v).map(|s| obj.prop("placeholder-text", s)).is_some()
            }
            (Some(Role::LineEdit), "input-type") => {
                if value_text(v) == "password" {
                    obj.prop("visibility", "false");
                }
                true
            }
            (Some(Role::LineEdit), "read-only") => {
                obj.prop(
                    "editable",
                    if v == &Value::Bool(true) {
                        "false"
                    } else {
                        "true"
                    },
                );
                true
            }
            (Some(Role::TextEdit), "text") => string(v)
                .map(|s| {
                    obj.extra
                        .push(format!("buffer: Gtk.TextBuffer {{\n  text: {s};\n}};"))
                })
                .is_some(),
            (Some(Role::TextEdit), "read-only") => {
                obj.prop(
                    "editable",
                    if v == &Value::Bool(true) {
                        "false"
                    } else {
                        "true"
                    },
                );
                true
            }
            (Some(Role::Slider | Role::SpinBox), "minimum" | "maximum" | "value" | "step") => true,
            (Some(Role::Progress), "progress") => {
                obj.prop("fraction", value_text(v));
                true
            }
            (Some(Role::Progress), "indeterminate") => true,
            (Some(Role::ComboBox), "model") => match v {
                Value::Array(items) => {
                    let items: Vec<String> = items.iter().map(|i| quote(&value_text(i))).collect();
                    obj.extra.push(format!(
                        "model: Gtk.StringList {{\n  strings [{}]\n}};",
                        items.join(", ")
                    ));
                    true
                }
                _ => false,
            },
            (Some(Role::ComboBox), "current-index") => {
                obj.prop("selected", value_text(v));
                true
            }
            (Some(Role::GroupBox), "title") => string(v).map(|s| obj.prop("label", s)).is_some(),
            (_, "visible") => {
                obj.prop("visible", value_text(v));
                true
            }
            (_, "enabled") => {
                obj.prop("sensitive", value_text(v));
                true
            }
            (_, "opacity") => v
                .as_number()
                .map(|o| obj.prop("opacity", number(o)))
                .is_some(),
            (_, "accessible-label") => string(v)
                .map(|s| {
                    obj.extra
                        .push(format!("accessibility {{\n  label: {s};\n}}"))
                })
                .is_some(),
            _ => false,
        };
        if !done {
            self.warn(
                &p.origin,
                format!(
                    "property `{}` of `{}` has no GTK equivalent",
                    p.name, n.widget
                ),
            );
        }
    }

    /// Settings that depend on several properties at once.
    fn finish_role(&mut self, obj: &mut Obj, n: &Node, role: Option<Role>) {
        match role {
            Some(Role::Text) => {
                if !obj.props.iter().any(|p| p.starts_with("xalign:")) {
                    obj.prop("xalign", "0");
                }
                if !obj.props.iter().any(|p| p.starts_with("yalign:")) {
                    obj.prop("yalign", "0");
                }
                if let Some(label) = label_markup(n) {
                    obj.props.insert(0, label.0);
                    if label.1 {
                        obj.prop("use-markup", "true");
                    }
                }
            }
            Some(Role::Slider | Role::SpinBox) => {
                let get = |name: &str| n.property(name).and_then(Value::as_number);
                let lower = get("minimum").unwrap_or(0.0);
                let upper = get("maximum").unwrap_or(100.0);
                let value = get("value").unwrap_or(lower);
                let mut block = format!(
                    "adjustment: Gtk.Adjustment {{\n  lower: {};\n  upper: {};\n  value: {};\n",
                    number(lower),
                    number(upper),
                    number(value)
                );
                if let Some(step) = get("step") {
                    let _ = writeln!(block, "  step-increment: {};", number(step));
                } else if role == Some(Role::SpinBox) {
                    block.push_str("  step-increment: 1;\n");
                }
                block.push_str("};");
                obj.extra.insert(0, block);
            }
            Some(Role::Spinner) => obj.prop("spinning", "true"),
            Some(Role::Progress) if n.property("indeterminate") == Some(&Value::Bool(true)) => {
                obj.comments
                    .push("indeterminate: call pulse() from code".to_owned());
            }
            _ => {}
        }
    }

    /// Two elements without ids can have the same callback; each handler
    /// still gets its own function name.
    fn handler_name(&mut self, base: String) -> String {
        let mut name = base.clone();
        let mut k = 2;
        while !self.handlers.insert(name.clone()) {
            name = format!("{base}_{k}");
            k += 1;
        }
        name
    }

    fn handler(&mut self, obj: &mut Obj, n: &Node, role: Option<Role>, h: &Handler) {
        let owner =
            n.id.as_deref()
                .map(ident)
                .unwrap_or_else(|| ident(&n.widget.to_lowercase()));
        match h.kind {
            HandlerKind::Callback => {
                let name = self.handler_name(format!("on_{owner}_{}", ident(&h.name)));
                let params = if h.params.is_empty() {
                    String::new()
                } else {
                    format!("({})", h.params.join(", "))
                };
                let body = format!("// {}{params} => {}", h.name, h.body);
                if let Some(signal) = signal(role, &h.name) {
                    obj.extra.push(format!("{body}\n{signal} => ${name}();"));
                } else if role == Some(Role::Container) && h.name == "clicked" {
                    obj.extra.push(format!(
                        "Gtk.GestureClick {{\n  {body}\n  released => ${name}();\n}}"
                    ));
                } else {
                    self.warn(
                        &h.origin,
                        format!("callback `{}` of `{}` has no GTK signal", h.name, n.widget),
                    );
                    obj.comments
                        .push(format!("not converted: {}", body.trim_start_matches("// ")));
                }
            }
            HandlerKind::Changed => {
                let name = self.handler_name(format!("on_{owner}_{}_changed", ident(&h.name)));
                match gtk_property(role, &h.name) {
                    Some(prop) => obj.extra.push(format!(
                        "// changed {} => {}\nnotify::{prop} => ${name}();",
                        h.name, h.body
                    )),
                    None => {
                        self.warn(
                            &h.origin,
                            format!("`changed {}` has no GTK property to watch", h.name),
                        );
                        obj.comments
                            .push(format!("not converted: changed {} => {}", h.name, h.body));
                    }
                }
            }
        }
    }
}

/// `label: ...;` for a text, as Pango markup when it carries color or font
/// settings. Returns the line and whether it is markup.
fn label_markup(n: &Node) -> Option<(String, bool)> {
    let text_value = n.property("text")?;
    let Value::String { text, translatable } = text_value else {
        return None;
    };
    let mut attrs = Vec::new();
    if let Some(c) = n.property("color").and_then(|v| match v {
        Value::Color(c) => crate::color::hex(c),
        _ => None,
    }) {
        attrs.push(format!("foreground=\"{c}\""));
    }
    if let Some(size) = n.property("font-size").and_then(Value::as_px) {
        // Pango sizes are 1024ths of a point; a logical pixel is 0.75pt.
        attrs.push(format!(
            "size=\"{}\"",
            (size * 0.75 * 1024.0).round() as i64
        ));
    }
    if let Some(w) = n.property("font-weight").and_then(Value::as_number) {
        attrs.push(format!("weight=\"{}\"", number(w)));
    }
    if let Some(f) = n.property("font-family").and_then(Value::as_text) {
        attrs.push(format!("font_family=\"{}\"", super::escape(f)));
    }
    if n.property("font-italic") == Some(&Value::Bool(true)) {
        attrs.push("style=\"italic\"".to_owned());
    }
    if attrs.is_empty() {
        return Some((format!("label: {};", string(text_value)?), false));
    }
    let markup = format!("<span {}>{}</span>", attrs.join(" "), super::escape(text));
    let value = if *translatable {
        format!("_({})", quote(&markup))
    } else {
        quote(&markup)
    };
    Some((format!("label: {value};"), true))
}
