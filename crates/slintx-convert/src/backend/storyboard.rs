//! The `storyboard` backend: an Interface Builder storyboard for UIKit or
//! AppKit with Auto Layout. The main component becomes one view controller
//! scene. Box layouts become stack views, `GridLayout` becomes a vertical stack
//! of row stacks whose columns are tied by equal-width constraints, and
//! elements outside layouts are pinned with constraints. Stretch maps to
//! content hugging plus proportional constraints, leftover-space alignment to
//! spacer views. Ids and bound properties become outlets, callbacks become
//! actions; binding expressions stay as comments for the controller's code.

use std::collections::HashSet;

use super::{Backend, Markup, Platform, number, value_text};
use crate::color;
use crate::diag::{Diagnostics, Origin};
use crate::ir::*;
use crate::mapping::{self, Role, stretch};

pub struct Storyboard {
    pub platform: Platform,
}

/// Interface Builder's default text size is 17pt; Slint's is the platform's,
/// usually smaller. 14 matches the HTML backend so outputs compare.
const DEFAULT_FONT_SIZE: f64 = 14.0;

/// Required, but breakable before a size limit the author wrote.
const STRONG: u32 = 999;
/// `preferred-width` and friends: kept unless something stronger disagrees.
const PREFERRED: u32 = 700;
const HUG: u32 = 750;
const EXPAND: u32 = 250;
const SPACER: u32 = 1;

impl Backend for Storyboard {
    fn emit(&self, document: &Document, diagnostics: &mut Diagnostics) -> String {
        let mut b = Builder {
            ui: self.platform == Platform::UiKit,
            diagnostics,
            next: 0,
            vc: String::new(),
            outlets: Vec::new(),
            names: HashSet::new(),
            objects: Vec::new(),
            images: Vec::new(),
        };
        let scene = b.id();
        b.vc = b.id();
        let mut vc = Elem::new("viewController").attr("id", b.vc.clone());
        let mut m = Markup::default();
        m.raw_line(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        m.comment(&format!(
            "Converted from {} by slint-convert.",
            document.file
        ));
        if let Some(root) = &document.root {
            let info = root
                .component
                .as_ref()
                .and_then(|name| document.components.iter().find(|c| &c.name == name));
            let class = info.map_or("Main", |i| i.name.as_str());
            vc.set(
                "customClass",
                format!("{}ViewController", ident(class, true)),
            );
            vc.set("sceneMemberID", "viewController");
            if let Some(title) = root
                .property("title")
                .and_then(Value::as_text)
                .filter(|_| root.widget == "Window")
            {
                vc.set("title", title);
            }
            if let Some(info) = info {
                vc.comments = interface(info, root);
            }
            let view = b.root(root);
            vc.children.push(view.into_elem());
        }
        if !b.outlets.is_empty() {
            vc.children
                .push(Elem::new("connections").with(std::mem::take(&mut b.outlets)));
        }
        let first_responder = if b.ui {
            Elem::new("placeholder")
                .attr("placeholderIdentifier", "IBFirstResponder")
                .attr("id", b.id())
                .attr("userLabel", "First Responder")
                .attr("sceneMemberID", "firstResponder")
        } else {
            Elem::new("customObject")
                .attr("id", b.id())
                .attr("userLabel", "First Responder")
                .attr("customClass", "NSResponder")
                .attr("sceneMemberID", "firstResponder")
        };
        let mut objects = vec![vc, first_responder];
        objects.append(&mut b.objects);
        let scene = Elem::new("scene")
            .attr("sceneID", scene)
            .child(Elem::new("objects").with(objects))
            .child(
                Elem::new("point")
                    .attr("key", "canvasLocation")
                    .attr("x", "0")
                    .attr("y", "0"),
            );
        let mut doc = if b.ui {
            Elem::new("document")
                .attr(
                    "type",
                    "com.apple.InterfaceBuilder3.CocoaTouch.Storyboard.XIB",
                )
                .attr("version", "3.0")
                .attr("toolsVersion", "23504")
                .attr("targetRuntime", "iOS.CocoaTouch")
                .attr("propertyAccessControl", "none")
                .attr("useAutolayout", "YES")
                .attr("useTraitCollections", "YES")
                .attr("useSafeAreas", "YES")
                .attr("colorMatched", "YES")
        } else {
            Elem::new("document")
                .attr("type", "com.apple.InterfaceBuilder3.Cocoa.Storyboard.XIB")
                .attr("version", "3.0")
                .attr("toolsVersion", "23504")
                .attr("targetRuntime", "MacOSX.Cocoa")
                .attr("propertyAccessControl", "none")
                .attr("useAutolayout", "YES")
        };
        doc.set("initialViewController", b.vc.clone());
        let mut deps = Elem::new("dependencies");
        if b.ui {
            deps.children.push(
                Elem::new("plugIn")
                    .attr(
                        "identifier",
                        "com.apple.InterfaceBuilder.IBCocoaTouchPlugin",
                    )
                    .attr("version", "23506"),
            );
            for (name, version) in [
                ("Safe area layout guides", "9.0"),
                ("documents saved in the Xcode 8 format", "8.0"),
            ] {
                deps.children.push(
                    Elem::new("capability")
                        .attr("name", name)
                        .attr("minToolsVersion", version),
                );
            }
        } else {
            deps.children.push(
                Elem::new("plugIn")
                    .attr("identifier", "com.apple.InterfaceBuilder.CocoaPlugin")
                    .attr("version", "23504"),
            );
        }
        doc.children.push(deps);
        doc.children.push(Elem::new("scenes").child(scene));
        if !b.images.is_empty() {
            let images = b
                .images
                .iter()
                .map(|(name, (w, h))| {
                    Elem::new("image")
                        .attr("name", name.clone())
                        .attr("width", number(*w))
                        .attr("height", number(*h))
                })
                .collect();
            doc.children.push(Elem::new("resources").with(images));
        }
        doc.write(&mut m);
        m.finish()
    }
}

/// One element of the output, built in full before it is written so that
/// constraints and outlets can point at views emitted elsewhere.
#[derive(Default)]
struct Elem {
    name: &'static str,
    attrs: Vec<(&'static str, String)>,
    comments: Vec<String>,
    text: Option<String>,
    children: Vec<Elem>,
}

impl Elem {
    fn new(name: &'static str) -> Elem {
        Elem {
            name,
            ..Elem::default()
        }
    }

    fn attr(mut self, key: &'static str, value: impl Into<String>) -> Elem {
        self.set(key, value);
        self
    }

    /// Sets `key`, replacing an earlier value.
    fn set(&mut self, key: &'static str, value: impl Into<String>) {
        self.attrs.retain(|(k, _)| *k != key);
        self.attrs.push((key, value.into()));
    }

    fn child(mut self, child: Elem) -> Elem {
        self.children.push(child);
        self
    }

    fn with(mut self, children: Vec<Elem>) -> Elem {
        self.children.extend(children);
        self
    }

    fn write(&self, m: &mut Markup) {
        if let Some(text) = &self.text {
            m.text_element(self.name, &self.attrs, text);
        } else if self.children.is_empty() && self.comments.is_empty() {
            m.empty(self.name, &self.attrs);
        } else {
            m.open(self.name, &self.attrs);
            for c in &self.comments {
                m.comment(c);
            }
            for c in &self.children {
                c.write(m);
            }
            m.close(self.name);
        }
    }
}

/// A text attribute; a value with a line break goes into a `<string>` child,
/// because XML attribute normalisation would turn it into a space.
fn put_text(e: &mut Elem, key: &'static str, text: &str) {
    if text.contains('\n') {
        e.children.push(Elem {
            text: Some(text.to_owned()),
            ..Elem::new("string").attr("key", key)
        });
    } else {
        e.set(key, text);
    }
}

/// A view with what Interface Builder keeps beside it: its subviews, the
/// constraints it owns, and its connections.
struct View {
    id: String,
    elem: Elem,
    subviews: Vec<View>,
    constraints: Vec<Elem>,
    connections: Vec<Elem>,
    /// The controller's own view keeps its autoresizing frame.
    root: bool,
}

impl View {
    fn new(id: String, elem: Elem) -> View {
        View {
            id,
            elem,
            subviews: Vec::new(),
            constraints: Vec::new(),
            connections: Vec::new(),
            root: false,
        }
    }

    fn hug(&mut self, horizontal: bool, priority: u32) {
        let key = if horizontal {
            "horizontalHuggingPriority"
        } else {
            "verticalHuggingPriority"
        };
        self.elem.set(key, priority.to_string());
    }

    fn into_elem(self) -> Elem {
        let mut e = self.elem;
        if !self.root {
            e.set("translatesAutoresizingMaskIntoConstraints", "NO");
        }
        e.set("id", self.id);
        if !self.subviews.is_empty() {
            let subviews = self.subviews.into_iter().map(View::into_elem).collect();
            e.children.push(Elem::new("subviews").with(subviews));
        }
        if !self.constraints.is_empty() {
            e.children
                .push(Elem::new("constraints").with(self.constraints));
        }
        if !self.connections.is_empty() {
            e.children
                .push(Elem::new("connections").with(self.connections));
        }
        e
    }
}

/// One constraint, in Interface Builder's terms. Without `first` it belongs
/// to the view whose `<constraints>` holds it.
#[derive(Default)]
struct Rule<'a> {
    first: Option<&'a str>,
    first_attr: &'static str,
    relation: Option<&'static str>,
    second: Option<&'a str>,
    second_attr: &'static str,
    multiplier: Option<f64>,
    constant: f64,
    priority: Option<u32>,
}

/// One arranged subview of a stack, with what the stack needs to know.
struct Arranged {
    view: View,
    /// `None` for spacers and empty grid cells: they absorb space and take
    /// no part in the stretch rules.
    stretch: Option<f64>,
    fixed_main: bool,
    /// A fixed or maximum size across the stack, so it cannot fill it.
    fixed_cross: bool,
}

struct Builder<'a> {
    ui: bool,
    diagnostics: &'a mut Diagnostics,
    next: u32,
    vc: String,
    outlets: Vec<Elem>,
    /// Outlet and action names already used.
    names: HashSet<String>,
    /// Scene objects beside the controller: gesture recognizers.
    objects: Vec<Elem>,
    images: Vec<(String, (f64, f64))>,
}

fn px(value: &Value) -> Option<f64> {
    value.as_px()
}

fn is_binding(value: &Value) -> bool {
    matches!(value, Value::Binding(_) | Value::TwoWay(_))
}

/// An Objective-C identifier from a Slint name: `show-footer` becomes
/// `showFooter` (or `ShowFooter` when `upper`).
fn ident(text: &str, upper: bool) -> String {
    let mut out = String::new();
    let mut capital = upper;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if capital {
                out.extend(c.to_uppercase());
            } else if out.is_empty() {
                out.extend(c.to_lowercase());
            } else {
                out.push(c);
            }
            capital = false;
        } else {
            capital = !out.is_empty();
        }
    }
    if out.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn interface(info: &ComponentInfo, root: &Node) -> Vec<String> {
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
        lines.push(format!("{vis}property <{}> {}{default}", p.ty, p.name));
    }
    for c in &info.callbacks {
        lines.push(format!("callback {}({})", c.name, c.params.join(", ")));
    }
    if !lines.is_empty() {
        lines.insert(0, format!("Interface of {}:", info.name));
    }
    lines
}

fn color_elem(key: &'static str, value: &str) -> Option<Elem> {
    let [r, g, b, a] = color::rgba(value)?;
    let c = |v: u8| number((f64::from(v) / 255.0 * 1000.0).round() / 1000.0);
    Some(
        Elem::new("color")
            .attr("key", key)
            .attr("red", c(r))
            .attr("green", c(g))
            .attr("blue", c(b))
            .attr("alpha", c(a))
            .attr("colorSpace", "custom")
            .attr("customColorSpace", "sRGB"),
    )
}

fn catalog_color(key: &'static str, name: &str) -> Elem {
    Elem::new("color")
        .attr("key", key)
        .attr("name", name)
        .attr("catalog", "System")
        .attr("colorSpace", "catalog")
}

struct Font {
    size: f64,
    weight: Option<f64>,
    italic: bool,
    family: Option<String>,
}

fn font_of(n: &Node) -> Font {
    Font {
        size: n
            .property("font-size")
            .and_then(Value::as_px)
            .unwrap_or(DEFAULT_FONT_SIZE),
        weight: n.property("font-weight").and_then(Value::as_number),
        italic: n.property("font-italic") == Some(&Value::Bool(true)),
        family: n
            .property("font-family")
            .and_then(Value::as_text)
            .map(str::to_owned),
    }
}

fn enum_text(n: &Node, name: &str) -> Option<String> {
    match n.property(name)? {
        v @ (Value::Enum(_) | Value::String { .. }) => Some(value_text(v)),
        _ => None,
    }
}

fn text_of(n: &Node, name: &str) -> Option<String> {
    n.property(name).and_then(Value::as_text).map(str::to_owned)
}

fn number_of(n: &Node, name: &str) -> Option<f64> {
    n.property(name).and_then(Value::as_number)
}

fn bool_of(n: &Node, name: &str) -> Option<bool> {
    match n.property(name)? {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn yes(b: bool) -> &'static str {
    if b { "YES" } else { "NO" }
}

/// Properties a role's builder reads; everything else on the element warns.
fn handled(role: Option<Role>, ui: bool, name: &str) -> bool {
    use Role::*;
    match (role, name) {
        (_, "visible" | "opacity") => true,
        (_, "accessible-label") => ui,
        (Some(Window), "title") => true,
        (Some(Window | Container), "background") => ui,
        (
            Some(Text),
            "text"
            | "color"
            | "font-size"
            | "font-weight"
            | "font-family"
            | "horizontal-alignment"
            | "vertical-alignment"
            | "wrap"
            | "overflow",
        ) => true,
        (Some(Text), "font-italic") => ui,
        (Some(Image), "source" | "image-fit") => true,
        (Some(Button), "text") => true,
        (Some(CheckBox | Switch), "text" | "checked") => true,
        (
            Some(LineEdit),
            "text" | "placeholder-text" | "input-type" | "read-only" | "font-size",
        ) => true,
        (Some(TextEdit), "text" | "font-size" | "read-only") => true,
        (Some(Slider), "minimum" | "maximum" | "value") => true,
        (Some(SpinBox), "minimum" | "maximum" | "value" | "step") => true,
        (Some(Progress), "progress" | "indeterminate") => true,
        (Some(ComboBox), "model" | "current-index") => true,
        // Warned about once, by the builder.
        (Some(GroupBox), "title") => true,
        (Some(Button | CheckBox | Switch | LineEdit | Slider | SpinBox | ComboBox), "enabled") => {
            true
        }
        _ => false,
    }
}

impl Builder<'_> {
    fn id(&mut self) -> String {
        let k = self.next;
        self.next += 1;
        format!(
            "s{:02}-{:02}-{:03}",
            k / 100_000 % 100,
            k / 1000 % 100,
            k % 1000
        )
    }

    fn platform(&self) -> &'static str {
        if self.ui { "UIKit" } else { "AppKit" }
    }

    fn warn(&mut self, origin: &Origin, message: String) {
        self.diagnostics.warn(origin, message);
    }

    fn rule(&mut self, r: Rule<'_>) -> Elem {
        let mut e = Elem::new("constraint");
        if let Some(first) = r.first {
            e.set("firstItem", first);
        }
        e.set("firstAttribute", r.first_attr);
        if let Some(rel) = r.relation {
            e.set("relation", rel);
        }
        if let Some(second) = r.second {
            e.set("secondItem", second);
            e.set("secondAttribute", r.second_attr);
        }
        if let Some(m) = r.multiplier.filter(|m| *m != 1.0) {
            e.set("multiplier", number(m));
        }
        if r.constant != 0.0 {
            e.set("constant", number(r.constant));
        }
        if let Some(p) = r.priority {
            e.set("priority", p.to_string());
        }
        e.attr("id", self.id())
    }

    /// `a.attr == b.attr + constant`.
    fn pin(&mut self, a: &str, b: &str, attr: &'static str, constant: f64) -> Elem {
        self.rule(Rule {
            first: Some(a),
            first_attr: attr,
            second: Some(b),
            second_attr: attr,
            constant,
            ..Rule::default()
        })
    }

    fn unique(&mut self, base: String) -> String {
        let mut name = base.clone();
        let mut k = 2;
        while !self.names.insert(name.clone()) {
            name = format!("{base}{k}");
            k += 1;
        }
        name
    }

    fn outlet(&mut self, n: &Node, destination: &str) {
        let base = ident(n.id.as_deref().unwrap_or(&n.widget), false);
        let property = self.unique(base);
        let outlet = Elem::new("outlet")
            .attr("property", property)
            .attr("destination", destination)
            .attr("id", self.id());
        self.outlets.push(outlet);
    }

    /// The controller's view. A `Window` root is that view; any other root
    /// fills a plain one.
    fn root(&mut self, root: &Node) -> View {
        let is_window = root.widget == "Window";
        let size = |e: &Extent, default: f64| e.fixed.as_ref().and_then(px).unwrap_or(default);
        let (w, h) = if self.ui {
            (375.0, 667.0)
        } else {
            (480.0, 270.0)
        };
        let (w, h) = if is_window {
            (size(&root.item.width, w), size(&root.item.height, h))
        } else {
            (w, h)
        };
        let id = self.id();
        let mut e = Elem::new("view").attr("key", "view");
        if self.ui {
            e.set("contentMode", "scaleToFill");
        }
        e.children.push(
            Elem::new("rect")
                .attr("key", "frame")
                .attr("x", "0.0")
                .attr("y", "0.0")
                .attr("width", number(w))
                .attr("height", number(h)),
        );
        let mut mask = Elem::new("autoresizingMask").attr("key", "autoresizingMask");
        if self.ui {
            mask = mask
                .attr("widthSizable", "YES")
                .attr("heightSizable", "YES");
        }
        e.children.push(mask);
        let guide = if self.ui {
            let guide = self.id();
            e.children.push(
                Elem::new("viewLayoutGuide")
                    .attr("key", "safeArea")
                    .attr("id", guide.clone()),
            );
            guide
        } else {
            id.clone()
        };
        let mut view = View::new(id, e);
        view.root = true;
        if is_window {
            self.decorate(&mut view, root, Some(Role::Window));
            if self.ui && !view.elem.children.iter().any(|c| c.name == "color") {
                view.elem
                    .children
                    .extend(color_elem("backgroundColor", "#ffffff"));
            }
            self.place_free(&mut view, &guide, &root.children);
        } else {
            if self.ui {
                view.elem
                    .children
                    .extend(color_elem("backgroundColor", "#ffffff"));
            }
            self.place_free(&mut view, &guide, std::slice::from_ref(root));
        }
        view
    }

    /// Builds `n` and everything inside it. Where it sits in its parent is
    /// the parent's business.
    fn build(&mut self, n: &Node) -> View {
        let role = mapping::lookup(&n.widget).map(|w| w.role);
        let (mut view, control) = self.control(n, role);
        self.decorate(&mut view, n, role);
        if needs_outlet(n) {
            self.outlet(n, &control);
        }
        self.sizes(&mut view, n);
        match &n.layout {
            Some(l) if l.kind == LayoutKind::Grid => self.grid(&mut view, n, l),
            Some(l) => self.boxed(&mut view, n, l),
            None if n.children.is_empty() => {}
            None => match role {
                None | Some(Role::Window | Role::Container | Role::GroupBox) => {
                    let id = view.id.clone();
                    self.place_free(&mut view, &id, &n.children);
                }
                Some(Role::Scroll) if self.ui => self.scroll(&mut view, &n.children),
                Some(Role::Scroll) => {
                    self.warn(
                        &n.origin,
                        "scrolling is not converted for AppKit: the content is placed in a plain view"
                            .to_owned(),
                    );
                    let id = view.id.clone();
                    self.place_free(&mut view, &id, &n.children);
                }
                Some(_) => self.warn(
                    &n.origin,
                    format!(
                        "children of `{}` are not converted to {}",
                        n.widget,
                        self.platform()
                    ),
                ),
            },
        }
        view
    }

    /// Comments, bindings and the properties every view shares.
    fn decorate(&mut self, view: &mut View, n: &Node, role: Option<Role>) {
        let e = &mut view.elem;
        for u in &n.unsupported {
            e.comments.push(format!(
                "not converted: {} at {}:{}",
                u.what, u.origin.line, u.origin.column
            ));
        }
        match &n.structure {
            Some(Structure::Repeat { item, index, model }) => {
                let item = item.as_deref().unwrap_or("_");
                let index = index.as_ref().map(|i| format!("[{i}]")).unwrap_or_default();
                e.comments.push(format!(
                    "for {item}{index} in {}: template, emitted once",
                    model.expr
                ));
            }
            Some(Structure::Condition { condition }) => {
                e.comments.push(format!("if {}", condition.expr));
            }
            None => {}
        }
        for (name, value) in [
            ("x", &n.item.x),
            ("y", &n.item.y),
            ("width", &n.item.width.fixed),
            ("height", &n.item.height.fixed),
        ] {
            if let Some(v) = value.as_ref().filter(|v| is_binding(v)) {
                e.comments.push(format!("{name}: {}", value_text(v)));
            }
        }
        let mut warnings = Vec::new();
        for p in &n.properties {
            if let Value::Binding(b) | Value::TwoWay(b) = &p.value {
                let op = if matches!(p.value, Value::TwoWay(_)) {
                    "<=>"
                } else {
                    ":"
                };
                e.comments
                    .push(format!("binding {} {op} {}", p.name, b.expr));
                continue;
            }
            if !handled(role, self.ui, &p.name) {
                warnings.push((
                    p.origin.clone(),
                    format!(
                        "property `{}` of `{}` has no {} equivalent",
                        p.name,
                        n.widget,
                        self.platform()
                    ),
                ));
                continue;
            }
            match p.name.as_str() {
                "visible" if p.value == Value::Bool(false) => e.set("hidden", "YES"),
                "opacity" => {
                    if let Some(o) = p.value.as_number() {
                        e.set(if self.ui { "alpha" } else { "alphaValue" }, number(o));
                    }
                }
                "background" => {
                    if let Value::Color(c) = &p.value {
                        e.children.extend(color_elem("backgroundColor", c));
                    }
                }
                "accessible-label" => {
                    if let Some(label) = p.value.as_text() {
                        e.children.push(
                            Elem::new("accessibility")
                                .attr("key", "accessibilityConfiguration")
                                .attr("label", label),
                        );
                    }
                }
                _ => {}
            }
        }
        for (origin, message) in warnings {
            self.warn(&origin, message);
        }
    }

    /// Size limits as constraints the view owns.
    fn sizes(&mut self, view: &mut View, n: &Node) {
        for (extent, attr, axis) in [
            (&n.item.width, "width", "width"),
            (&n.item.height, "height", "height"),
        ] {
            let limits = [
                (&extent.fixed, None, None),
                (&extent.min, Some("greaterThanOrEqual"), None),
                (&extent.max, Some("lessThanOrEqual"), None),
                (&extent.preferred, None, Some(PREFERRED)),
            ];
            for (value, relation, priority) in limits {
                let Some(v) = value else { continue };
                match px(v) {
                    Some(size) => {
                        let c = self.rule(Rule {
                            first_attr: attr,
                            relation,
                            constant: size,
                            priority,
                            ..Rule::default()
                        });
                        view.constraints.push(c);
                    }
                    None if is_binding(v) => {}
                    None => self.warn(
                        &n.origin,
                        format!("{axis} {} is not an absolute length", value_text(v)),
                    ),
                }
            }
        }
    }

    /// The element for a role, with its own properties and handlers. Returns
    /// the outer view and the id of the control inside it, which differ when
    /// a label is placed beside a control that has no text of its own.
    fn control(&mut self, n: &Node, role: Option<Role>) -> (View, String) {
        let ui = self.ui;
        let id = self.id();
        let mut e = match role {
            Some(Role::Layout(_)) => self.stack_elem(n),
            Some(Role::Text) => self.label(n),
            Some(Role::Image) => self.image(n),
            Some(Role::Button) => self.button(n),
            Some(Role::CheckBox) if !ui => self.check(n),
            Some(Role::CheckBox | Role::Switch) => {
                let on = bool_of(n, "checked") == Some(true);
                if ui {
                    Elem::new("switch")
                        .attr("opaque", "NO")
                        .attr("contentMode", "scaleToFill")
                        .attr("contentHorizontalAlignment", "center")
                        .attr("contentVerticalAlignment", "center")
                        .attr("on", yes(on))
                } else {
                    Elem::new("switch")
                        .attr("baseWritingDirection", "leftToRight")
                        .attr("alignment", "left")
                        .attr("state", if on { "on" } else { "off" })
                }
            }
            Some(Role::LineEdit) => self.line_edit(n),
            Some(Role::TextEdit) => self.text_edit(n),
            Some(Role::Slider) => self.slider(n),
            Some(Role::Progress) => self.progress(n),
            Some(Role::Spinner) if ui => Elem::new("activityIndicatorView")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("animating", "YES")
                .attr("style", "medium"),
            Some(Role::Spinner) => Elem::new("progressIndicator")
                .attr("maxValue", "100")
                .attr("indeterminate", "YES")
                .attr("style", "spinning"),
            Some(Role::SpinBox) => self.stepper(n),
            Some(Role::ComboBox) => self.combo(n),
            Some(Role::Scroll) if ui => Elem::new("scrollView")
                .attr("clipsSubviews", "YES")
                .attr("multipleTouchEnabled", "YES")
                .attr("contentMode", "scaleToFill"),
            _ if ui => Elem::new("view").attr("contentMode", "scaleToFill"),
            _ => Elem::new("customView"),
        };
        if role == Some(Role::GroupBox)
            && let Some(title) = text_of(n, "title")
        {
            e.comments.push(format!("GroupBox title: {title}"));
            self.warn(
                &n.origin,
                format!(
                    "GroupBox title is not shown: {} has no titled container here",
                    self.platform()
                ),
            );
        }
        if bool_of(n, "enabled") == Some(false) && handled(role, ui, "enabled") {
            // AppKit keeps the enabled state on the control's cell.
            let cell = e
                .children
                .iter_mut()
                .find(|c| c.attrs.iter().any(|(k, v)| *k == "key" && v == "cell"));
            match cell {
                Some(cell) if !ui => cell.set("enabled", "NO"),
                _ => e.set("enabled", "NO"),
            }
        }
        let mut control = View::new(id.clone(), e);
        self.handlers(&mut control, n, role);
        let beside = match role {
            Some(Role::CheckBox) if ui => text_of(n, "text"),
            Some(Role::Switch) => text_of(n, "text"),
            Some(Role::SpinBox) => Some(number(
                number_of(n, "value")
                    .or(number_of(n, "minimum"))
                    .unwrap_or(0.0),
            )),
            _ => None,
        };
        let Some(text) = beside.filter(|t| !t.is_empty()) else {
            return (control, id);
        };
        // The label goes after a switch and before a stepper, as on each
        // platform's own forms.
        let label_id = self.id();
        let label = View::new(label_id, self.plain_label(&text));
        let before = role == Some(Role::SpinBox);
        let mut row = self.stack(true, 8.0, [0.0; 4]);
        control.hug(true, HUG);
        let items = if before {
            vec![label, control]
        } else {
            vec![control, label]
        };
        let items = items
            .into_iter()
            .map(|view| Arranged {
                view,
                stretch: Some(0.0),
                fixed_main: false,
                fixed_cross: true,
            })
            .collect();
        self.arrange(&mut row, true, Alignment::Start, (0.0, 0.0), items);
        (row, id)
    }

    fn font(&self, f: &Font) -> Elem {
        if self.ui {
            let mut e = Elem::new("fontDescription").attr("key", "fontDescription");
            if let Some(family) = &f.family {
                e.set("name", family.clone());
                e.set("family", family.clone());
            } else if f.italic {
                e.set("type", "italicSystem");
            } else {
                match f.weight.map(|w| w.round() as i64) {
                    Some(651..=750) => e.set("type", "boldSystem"),
                    Some(w) if !(351..=450).contains(&w) => {
                        e.set("type", "system");
                        let name = match w {
                            ..=150 => "ultraLight",
                            151..=250 => "thin",
                            251..=350 => "light",
                            451..=550 => "medium",
                            551..=650 => "semibold",
                            751..=850 => "heavy",
                            _ => "black",
                        };
                        e.set("weight", name);
                    }
                    _ => e.set("type", "system"),
                }
            }
            e.attr("pointSize", number(f.size))
        } else {
            let mut e = Elem::new("font").attr("key", "font");
            if let Some(family) = &f.family {
                e.set("size", number(f.size));
                e.set("name", family.clone());
            } else {
                let bold = f.weight.is_some_and(|w| w >= 600.0);
                e.set("metaFont", if bold { "systemBold" } else { "system" });
                e.set("size", number(f.size));
            }
            e
        }
    }

    /// A non-editable label with the default font.
    fn plain_label(&mut self, text: &str) -> Elem {
        let font = Font {
            size: DEFAULT_FONT_SIZE,
            weight: None,
            italic: false,
            family: None,
        };
        if self.ui {
            let mut e = Elem::new("label")
                .attr("opaque", "NO")
                .attr("userInteractionEnabled", "NO")
                .attr("contentMode", "left");
            put_text(&mut e, "text", text);
            e.child(self.font(&font))
                .child(Elem::new("nil").attr("key", "highlightedColor"))
        } else {
            let mut cell = Elem::new("textFieldCell")
                .attr("key", "cell")
                .attr("lineBreakMode", "clipping");
            put_text(&mut cell, "title", text);
            cell.set("id", self.id());
            let cell = cell
                .child(self.font(&font))
                .child(catalog_color("textColor", "labelColor"))
                .child(catalog_color("backgroundColor", "textBackgroundColor"));
            Elem::new("textField")
                .attr("focusRingType", "none")
                .child(cell)
        }
    }

    fn label(&mut self, n: &Node) -> Elem {
        let text = text_of(n, "text").unwrap_or_default();
        let font = font_of(n);
        let wraps = enum_text(n, "wrap").is_some_and(|w| w != "no-wrap");
        let elide = enum_text(n, "overflow").as_deref() == Some("elide");
        let align = enum_text(n, "horizontal-alignment");
        let valign = enum_text(n, "vertical-alignment");
        let native_valign = if self.ui { "center" } else { "top" };
        if let Some(v) = valign.as_deref().filter(|v| *v != native_valign) {
            let origin = n
                .properties
                .iter()
                .find(|p| p.name == "vertical-alignment")
                .map_or_else(|| n.origin.clone(), |p| p.origin.clone());
            self.warn(
                &origin,
                format!(
                    "vertical-alignment {v} has no {} label equivalent; text is placed at the {native_valign}",
                    self.platform()
                ),
            );
        }
        let color = match n.property("color") {
            Some(Value::Color(c)) => color_elem("textColor", c),
            _ => None,
        };
        if self.ui {
            let mut e = Elem::new("label")
                .attr("opaque", "NO")
                .attr("userInteractionEnabled", "NO")
                .attr("contentMode", "left");
            put_text(&mut e, "text", &text);
            if let Some(a) = align.filter(|a| matches!(a.as_str(), "left" | "center" | "right")) {
                e.set("textAlignment", a);
            }
            if wraps {
                e.set("lineBreakMode", "wordWrap");
            } else if !elide {
                e.set("lineBreakMode", "clip");
            }
            if wraps || text.contains('\n') {
                e.set("numberOfLines", "0");
            }
            e.children.push(self.font(&font));
            e.children.extend(color);
            e.children
                .push(Elem::new("nil").attr("key", "highlightedColor"));
            e
        } else {
            let mode = if wraps {
                "wordWrapping"
            } else if elide {
                "truncatingTail"
            } else {
                "clipping"
            };
            let mut cell = Elem::new("textFieldCell")
                .attr("key", "cell")
                .attr("lineBreakMode", mode);
            put_text(&mut cell, "title", &text);
            if let Some(a) = align.filter(|a| matches!(a.as_str(), "left" | "center" | "right")) {
                cell.set("alignment", a);
            }
            cell.set("id", self.id());
            cell.children.push(self.font(&font));
            cell.children
                .push(color.unwrap_or_else(|| catalog_color("textColor", "labelColor")));
            cell.children
                .push(catalog_color("backgroundColor", "textBackgroundColor"));
            Elem::new("textField")
                .attr("focusRingType", "none")
                .child(cell)
        }
    }

    fn image(&mut self, n: &Node) -> Elem {
        let fit = enum_text(n, "image-fit");
        let source = match n.property("source") {
            Some(Value::Url(url)) => Some(url.clone()),
            _ => None,
        };
        if let Some(name) = &source
            && !self.images.iter().any(|(i, _)| i == name)
        {
            let side = |e: &Extent| e.fixed.as_ref().and_then(px).unwrap_or(16.0);
            self.images
                .push((name.clone(), (side(&n.item.width), side(&n.item.height))));
        }
        if self.ui {
            let mode = match fit.as_deref() {
                Some("contain") => "scaleAspectFit",
                Some("cover") => "scaleAspectFill",
                Some("preserve") => "center",
                _ => "scaleToFill",
            };
            let mut e = Elem::new("imageView")
                .attr("clipsSubviews", "YES")
                .attr("userInteractionEnabled", "NO")
                .attr("contentMode", mode);
            if let Some(name) = source {
                e.set("image", name);
            }
            e
        } else {
            let scaling = match fit.as_deref() {
                Some("contain") => "proportionallyUpOrDown",
                Some("preserve") => "none",
                Some("cover") => {
                    self.warn(
                        &n.origin,
                        "image-fit cover has no NSImageView equivalent".to_owned(),
                    );
                    "proportionallyUpOrDown"
                }
                _ => "axesIndependently",
            };
            let mut cell = Elem::new("imageCell")
                .attr("key", "cell")
                .attr("refusesFirstResponder", "YES")
                .attr("alignment", "left")
                .attr("imageScaling", scaling);
            if let Some(name) = source {
                cell.set("image", name);
            }
            cell.set("id", self.id());
            Elem::new("imageView").child(cell)
        }
    }

    fn button(&mut self, n: &Node) -> Elem {
        let title = text_of(n, "text").unwrap_or_default();
        if self.ui {
            let mut state = Elem::new("state").attr("key", "normal");
            put_text(&mut state, "title", &title);
            Elem::new("button")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("contentHorizontalAlignment", "center")
                .attr("contentVerticalAlignment", "center")
                .attr("buttonType", "system")
                .attr("lineBreakMode", "middleTruncation")
                .child(state)
        } else {
            let mut cell = Elem::new("buttonCell")
                .attr("key", "cell")
                .attr("type", "push");
            put_text(&mut cell, "title", &title);
            for (k, v) in [
                ("bezelStyle", "rounded"),
                ("alignment", "center"),
                ("borderStyle", "border"),
                ("imageScaling", "proportionallyDown"),
                ("inset", "2"),
            ] {
                cell.set(k, v);
            }
            cell.set("id", self.id());
            let cell = cell
                .child(
                    Elem::new("behavior")
                        .attr("key", "behavior")
                        .attr("pushIn", "YES")
                        .attr("lightByBackground", "YES")
                        .attr("lightByGray", "YES"),
                )
                .child(
                    Elem::new("font")
                        .attr("key", "font")
                        .attr("metaFont", "system"),
                );
            Elem::new("button").child(cell)
        }
    }

    /// An AppKit check box: a button with a check cell.
    fn check(&mut self, n: &Node) -> Elem {
        let mut cell = Elem::new("buttonCell")
            .attr("key", "cell")
            .attr("type", "check");
        put_text(&mut cell, "title", &text_of(n, "text").unwrap_or_default());
        cell.set("bezelStyle", "regularSquare");
        cell.set("imagePosition", "left");
        if bool_of(n, "checked") == Some(true) {
            cell.set("state", "on");
        }
        cell.set("inset", "2");
        cell.set("id", self.id());
        let cell = cell
            .child(
                Elem::new("behavior")
                    .attr("key", "behavior")
                    .attr("changeContents", "YES")
                    .attr("doesNotDimImage", "YES")
                    .attr("lightByContents", "YES"),
            )
            .child(
                Elem::new("font")
                    .attr("key", "font")
                    .attr("metaFont", "system"),
            );
        Elem::new("button").child(cell)
    }

    fn line_edit(&mut self, n: &Node) -> Elem {
        let text = text_of(n, "text");
        let placeholder = text_of(n, "placeholder-text");
        let password = enum_text(n, "input-type").as_deref() == Some("password");
        let read_only = bool_of(n, "read-only") == Some(true);
        let font = font_of(n);
        if self.ui {
            let mut e = Elem::new("textField")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("contentHorizontalAlignment", "left")
                .attr("contentVerticalAlignment", "center")
                .attr("borderStyle", "roundedRect");
            if let Some(p) = &placeholder {
                e.set("placeholder", p.clone());
            }
            if let Some(t) = &text {
                e.set("text", t.clone());
            }
            e.set("textAlignment", "natural");
            e.set("minimumFontSize", "17");
            if read_only {
                e.set("enabled", "NO");
                self.warn(
                    &n.origin,
                    "read-only is shown as a disabled UITextField".to_owned(),
                );
            }
            let mut traits = Elem::new("textInputTraits").attr("key", "textInputTraits");
            if password {
                traits.set("secureTextEntry", "YES");
            }
            e.child(self.font(&font)).child(traits)
        } else {
            let mut cell = Elem::new("textFieldCell")
                .attr("key", "cell")
                .attr("scrollable", "YES")
                .attr("lineBreakMode", "clipping")
                .attr("selectable", "YES")
                .attr("editable", yes(!read_only))
                .attr("sendsActionOnEndEditing", "YES")
                .attr("borderStyle", "bezel");
            if let Some(p) = &placeholder {
                cell.set("placeholderString", p.clone());
            }
            put_text(&mut cell, "title", text.as_deref().unwrap_or_default());
            cell.set("drawsBackground", "YES");
            cell.set("id", self.id());
            let cell = cell
                .child(self.font(&font))
                .child(catalog_color("textColor", "controlTextColor"))
                .child(catalog_color("backgroundColor", "textBackgroundColor"));
            if password {
                Elem::new("secureTextField").child(Elem {
                    name: "secureTextFieldCell",
                    ..cell
                })
            } else {
                Elem::new("textField").child(cell)
            }
        }
    }

    fn text_edit(&mut self, n: &Node) -> Elem {
        let text = text_of(n, "text").unwrap_or_default();
        let read_only = bool_of(n, "read-only") == Some(true);
        let font = font_of(n);
        if self.ui {
            let mut e = Elem::new("textView")
                .attr("clipsSubviews", "YES")
                .attr("multipleTouchEnabled", "YES")
                .attr("contentMode", "scaleToFill")
                .attr("textAlignment", "natural");
            if read_only {
                e.set("editable", "NO");
            }
            e.children.push(Elem {
                text: Some(text),
                ..Elem::new("string").attr("key", "text")
            });
            e.child(self.font(&font)).child(
                Elem::new("textInputTraits")
                    .attr("key", "textInputTraits")
                    .attr("autocapitalizationType", "sentences"),
            )
        } else {
            // A wrapping text field: NSTextView needs a scroll view, clip
            // view and text container around it, more than the layout needs.
            let mut cell = Elem::new("textFieldCell")
                .attr("key", "cell")
                .attr("lineBreakMode", "wordWrapping")
                .attr("selectable", "YES")
                .attr("editable", yes(!read_only))
                .attr("sendsActionOnEndEditing", "YES")
                .attr("borderStyle", "bezel");
            put_text(&mut cell, "title", &text);
            cell.set("drawsBackground", "YES");
            cell.set("id", self.id());
            let cell = cell
                .child(self.font(&font))
                .child(catalog_color("textColor", "controlTextColor"))
                .child(catalog_color("backgroundColor", "textBackgroundColor"));
            Elem::new("textField").child(cell)
        }
    }

    fn slider(&mut self, n: &Node) -> Elem {
        let min = number_of(n, "minimum").unwrap_or(0.0);
        let max = number_of(n, "maximum").unwrap_or(100.0);
        let value = number_of(n, "value").unwrap_or(min);
        if self.ui {
            Elem::new("slider")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("contentHorizontalAlignment", "center")
                .attr("contentVerticalAlignment", "center")
                .attr("value", number(value))
                .attr("minValue", number(min))
                .attr("maxValue", number(max))
        } else {
            let cell = Elem::new("sliderCell")
                .attr("key", "cell")
                .attr("continuous", "YES")
                .attr("state", "on")
                .attr("alignment", "left")
                .attr("minValue", number(min))
                .attr("maxValue", number(max))
                .attr("doubleValue", number(value))
                .attr("tickMarkPosition", "above")
                .attr("sliderType", "linear")
                .attr("id", self.id());
            Elem::new("slider").child(cell)
        }
    }

    fn progress(&mut self, n: &Node) -> Elem {
        let value = number_of(n, "progress").unwrap_or(0.0);
        let indeterminate = bool_of(n, "indeterminate") == Some(true);
        if self.ui {
            if indeterminate {
                self.warn(
                    &n.origin,
                    "indeterminate has no UIProgressView equivalent".to_owned(),
                );
            }
            Elem::new("progressView")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("progressViewStyle", "default")
                .attr("progress", number(value))
        } else {
            let mut e = Elem::new("progressIndicator")
                .attr("maxValue", "1")
                .attr("doubleValue", number(value));
            if indeterminate {
                e.set("indeterminate", "YES");
            }
            e.attr("style", "bar")
        }
    }

    fn stepper(&mut self, n: &Node) -> Elem {
        let min = number_of(n, "minimum").unwrap_or(0.0);
        let max = number_of(n, "maximum").unwrap_or(100.0);
        let value = number_of(n, "value").unwrap_or(min);
        let step = number_of(n, "step").unwrap_or(1.0);
        if self.ui {
            Elem::new("stepper")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("contentHorizontalAlignment", "center")
                .attr("contentVerticalAlignment", "center")
                .attr("value", number(value))
                .attr("minimumValue", number(min))
                .attr("maximumValue", number(max))
                .attr("stepValue", number(step))
        } else {
            let cell = Elem::new("stepperCell")
                .attr("key", "cell")
                .attr("continuous", "YES")
                .attr("alignment", "left")
                .attr("increment", number(step))
                .attr("minValue", number(min))
                .attr("maxValue", number(max))
                .attr("doubleValue", number(value))
                .attr("id", self.id());
            Elem::new("stepper").child(cell)
        }
    }

    fn combo(&mut self, n: &Node) -> Elem {
        let items: Vec<String> = match n.property("model") {
            Some(Value::Array(items)) => items.iter().map(value_text).collect(),
            _ => Vec::new(),
        };
        let selected = number_of(n, "current-index")
            .map(|i| i as usize)
            .unwrap_or(0);
        let title = items.get(selected).cloned().unwrap_or_default();
        if self.ui {
            let mut commands = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let mut c = Elem::new("command").attr("title", item.clone());
                if i == selected {
                    c.set("menuElementState", "on");
                }
                commands.push(c.attr("id", self.id()));
            }
            let menu = Elem::new("menu")
                .attr("key", "menu")
                .attr("id", self.id())
                .child(Elem::new("children").with(commands));
            Elem::new("button")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("contentHorizontalAlignment", "center")
                .attr("contentVerticalAlignment", "center")
                .attr("showsMenuAsPrimaryAction", "YES")
                .attr("changesSelectionAsPrimaryAction", "YES")
                .attr("buttonType", "system")
                .attr("lineBreakMode", "middleTruncation")
                .child(menu)
                .child(
                    Elem::new("state")
                        .attr("key", "normal")
                        .attr("title", title),
                )
        } else {
            let mut menu_items = Vec::new();
            let mut selected_id = None;
            for (i, item) in items.iter().enumerate() {
                let id = self.id();
                let mut m = Elem::new("menuItem").attr("title", item.clone());
                if i == selected {
                    m.set("state", "on");
                    selected_id = Some(id.clone());
                }
                menu_items.push(m.attr("id", id));
            }
            let menu = Elem::new("menu")
                .attr("key", "menu")
                .attr("id", self.id())
                .child(Elem::new("items").with(menu_items));
            let mut cell = Elem::new("popUpButtonCell")
                .attr("key", "cell")
                .attr("type", "push")
                .attr("title", title)
                .attr("bezelStyle", "rounded")
                .attr("alignment", "left")
                .attr("lineBreakMode", "truncatingTail")
                .attr("state", "on")
                .attr("borderStyle", "borderAndBezel")
                .attr("imageScaling", "proportionallyDown")
                .attr("inset", "2");
            if let Some(id) = selected_id {
                cell.set("selectedItem", id);
            }
            cell.set("id", self.id());
            let cell = cell
                .child(
                    Elem::new("behavior")
                        .attr("key", "behavior")
                        .attr("lightByBackground", "YES")
                        .attr("lightByGray", "YES"),
                )
                .child(
                    Elem::new("font")
                        .attr("key", "font")
                        .attr("metaFont", "menu"),
                )
                .child(menu);
            Elem::new("popUpButton").child(cell)
        }
    }

    fn handlers(&mut self, view: &mut View, n: &Node, role: Option<Role>) {
        let owner = ident(n.id.as_deref().unwrap_or(&n.widget), false);
        let mut has_action = false;
        for h in &n.handlers {
            let params = if h.params.is_empty() {
                String::new()
            } else {
                format!("({})", h.params.join(", "))
            };
            match h.kind {
                HandlerKind::Callback => {
                    let note = format!("{}{params} => {}", h.name, h.body);
                    let event = event(role, &h.name, self.ui);
                    let gesture = role == Some(Role::Container) && h.name == "clicked" && self.ui;
                    let no_event = event.is_none() && !gesture;
                    if no_event || (!self.ui && has_action) {
                        let why = if no_event {
                            format!("no {} event", self.platform())
                        } else {
                            "AppKit controls send one action".to_owned()
                        };
                        self.warn(
                            &h.origin,
                            format!(
                                "callback `{}` of `{}` is not converted: {why}",
                                h.name, n.widget
                            ),
                        );
                        view.elem.comments.push(format!("not converted: {note}"));
                        continue;
                    }
                    has_action = true;
                    let selector = self.unique(format!("{owner}{}", ident(&h.name, true))) + ":";
                    let mut action = Elem::new("action").attr("selector", selector);
                    if self.ui {
                        action.set("destination", self.vc.clone());
                    } else {
                        action.set("target", self.vc.clone());
                    }
                    if let Some(event) = event.filter(|_| self.ui) {
                        action.set("eventType", event);
                    }
                    let action = action.attr("id", self.id());
                    if gesture {
                        let tap = self.id();
                        self.objects.push(
                            Elem::new("tapGestureRecognizer")
                                .attr("id", tap.clone())
                                .child(Elem {
                                    comments: vec![note],
                                    ..Elem::new("connections").child(action)
                                }),
                        );
                        let collection = Elem::new("outletCollection")
                            .attr("property", "gestureRecognizers")
                            .attr("destination", tap)
                            .attr("appends", "YES")
                            .attr("id", self.id());
                        view.connections.push(collection);
                    } else {
                        view.elem.comments.push(note);
                        view.connections.push(action);
                    }
                }
                HandlerKind::Changed => {
                    view.elem
                        .comments
                        .push(format!("changed {} => {}", h.name, h.body));
                    self.warn(
                        &h.origin,
                        format!(
                            "`changed {}` needs code: observe the outlet in the controller",
                            h.name
                        ),
                    );
                }
            }
        }
    }

    /// A stack view element for a box or grid layout node, before its
    /// arrangement is known.
    fn stack_elem(&self, n: &Node) -> Elem {
        let horizontal = n
            .layout
            .as_ref()
            .is_some_and(|l| l.kind == LayoutKind::Horizontal);
        let spacing = n
            .layout
            .as_ref()
            .and_then(|l| match l.kind {
                LayoutKind::Grid => l.spacing_vertical.as_ref().or(l.spacing.as_ref()),
                _ => l.spacing.as_ref(),
            })
            .and_then(px)
            .unwrap_or(0.0);
        let pad = n.layout.as_ref().map_or([0.0; 4], |l| insets(&l.padding));
        self.stack_with(horizontal, spacing, pad)
    }

    fn stack(&mut self, horizontal: bool, spacing: f64, pad: [f64; 4]) -> View {
        let e = self.stack_with(horizontal, spacing, pad);
        View::new(self.id(), e)
    }

    /// `pad` is top, leading, bottom, trailing.
    fn stack_with(&self, horizontal: bool, spacing: f64, pad: [f64; 4]) -> Elem {
        let axis = if horizontal { "horizontal" } else { "vertical" };
        let [top, leading, bottom, trailing] = pad;
        let padded = pad.iter().any(|p| *p != 0.0);
        if self.ui {
            let mut e = Elem::new("stackView")
                .attr("opaque", "NO")
                .attr("contentMode", "scaleToFill")
                .attr("axis", axis)
                .attr("spacing", number(spacing));
            if padded {
                e.set("layoutMarginsRelativeArrangement", "YES");
                e.children.push(
                    Elem::new("directionalEdgeInsets")
                        .attr("key", "directionalLayoutMargins")
                        .attr("top", number(top))
                        .attr("leading", number(leading))
                        .attr("bottom", number(bottom))
                        .attr("trailing", number(trailing)),
                );
            }
            e
        } else {
            let mut e = Elem::new("stackView")
                .attr("distribution", "fill")
                .attr("orientation", axis)
                .attr("alignment", if horizontal { "centerY" } else { "centerX" })
                .attr("spacing", number(spacing))
                .attr("detachesHiddenViews", "YES");
            if padded {
                e.children.push(
                    Elem::new("edgeInsets")
                        .attr("key", "edgeInsets")
                        .attr("left", number(leading))
                        .attr("right", number(trailing))
                        .attr("top", number(top))
                        .attr("bottom", number(bottom)),
                );
            }
            e
        }
    }

    /// An empty view that takes leftover space along a stack.
    fn spacer(&mut self, horizontal: bool) -> View {
        let e = if self.ui {
            Elem::new("view")
                .attr("contentMode", "scaleToFill")
                .attr("userLabel", "Spacer")
        } else {
            Elem::new("customView").attr("userLabel", "Spacer")
        };
        let mut v = View::new(self.id(), e);
        v.hug(horizontal, SPACER);
        v
    }

    fn boxed(&mut self, stack: &mut View, n: &Node, l: &Layout) {
        let horizontal = l.kind == LayoutKind::Horizontal;
        let items = n
            .children
            .iter()
            .map(|c| {
                let (main, cross) = if horizontal {
                    (&c.item.width, &c.item.height)
                } else {
                    (&c.item.height, &c.item.width)
                };
                Arranged {
                    view: self.build(c),
                    stretch: Some(stretch(c, horizontal)),
                    fixed_main: main.fixed.is_some(),
                    fixed_cross: cross.fixed.is_some() || cross.max.is_some(),
                }
            })
            .collect();
        let [top, leading, bottom, trailing] = insets(&l.padding);
        let cross = if horizontal {
            (top, bottom)
        } else {
            (leading, trailing)
        };
        self.arrange(
            stack,
            horizontal,
            l.alignment.unwrap_or(Alignment::Stretch),
            cross,
            items,
        );
    }

    /// Puts `items` into `stack` the way a Slint box layout would size them.
    fn arrange(
        &mut self,
        stack: &mut View,
        horizontal: bool,
        alignment: Alignment,
        cross_insets: (f64, f64),
        items: Vec<Arranged>,
    ) {
        let (main, cross) = if horizontal {
            ("width", "height")
        } else {
            ("height", "width")
        };
        let stretches = alignment == Alignment::Stretch;
        let all_zero = items.iter().filter_map(|i| i.stretch).all(|s| s == 0.0);
        let mut expanding: Vec<(String, f64)> = Vec::new();
        let mut arranged: Vec<(View, bool)> = Vec::new();
        for mut item in items {
            match item.stretch {
                Some(s) => {
                    let grows = stretches && !item.fixed_main && (all_zero || s > 0.0);
                    if grows {
                        expanding.push((item.view.id.clone(), if all_zero { 1.0 } else { s }));
                    }
                    item.view.hug(horizontal, if grows { EXPAND } else { HUG });
                }
                None => item.view.hug(horizontal, SPACER),
            }
            arranged.push((item.view, !item.fixed_cross));
        }
        if let Some(((base, base_factor), rest)) = expanding.split_first() {
            for (id, factor) in rest {
                let c = self.rule(Rule {
                    first: Some(id),
                    first_attr: main,
                    second: Some(base),
                    second_attr: main,
                    multiplier: Some(factor / base_factor),
                    priority: Some(STRONG),
                    ..Rule::default()
                });
                stack.constraints.push(c);
            }
        }

        // Leftover space: spacers stand in for Slint's alignment, sized
        // against the first one.
        let distribution = match alignment {
            Alignment::SpaceBetween => "equalSpacing",
            _ => "fill",
        };
        stack.elem.set("distribution", distribution);
        let pattern: &[f64] = match alignment {
            Alignment::Stretch | Alignment::SpaceBetween => &[],
            Alignment::Start => &[0.0, 1.0],
            Alignment::End => &[1.0, 0.0],
            Alignment::Center => &[1.0, 1.0],
            Alignment::SpaceAround => &[1.0, 2.0, 1.0],
            Alignment::SpaceEvenly => &[1.0, 1.0, 1.0],
        };
        let mut views: Vec<(View, bool)> = Vec::new();
        let mut spacers: Vec<(String, f64)> = Vec::new();
        let count = arranged.len();
        for (i, item) in arranged.into_iter().enumerate() {
            let weight = match pattern {
                [] => 0.0,
                [lead, ..] if i == 0 => *lead,
                [_, between, _] => *between,
                _ => 0.0,
            };
            if weight > 0.0 {
                let s = self.spacer(horizontal);
                spacers.push((s.id.clone(), weight));
                views.push((s, true));
            }
            views.push(item);
            let trail = pattern.last().copied().unwrap_or(0.0);
            if i + 1 == count && trail > 0.0 {
                let s = self.spacer(horizontal);
                spacers.push((s.id.clone(), trail));
                views.push((s, true));
            }
        }
        if let Some(((base, base_weight), rest)) = spacers.split_first() {
            for (id, weight) in rest {
                let c = self.rule(Rule {
                    first: Some(id),
                    first_attr: main,
                    second: Some(base),
                    second_attr: main,
                    multiplier: Some(weight / base_weight),
                    ..Rule::default()
                });
                stack.constraints.push(c);
            }
        }

        // Across the stack Slint fills unless a size limit stops it. A fill
        // alignment would break those limits, so a centred stack ties the
        // other views to its size instead (AppKit always works this way).
        let centred = !self.ui || views.iter().any(|(_, fills)| !fills);
        if centred {
            if self.ui {
                stack.elem.set("alignment", "center");
            }
            let constant = -(cross_insets.0 + cross_insets.1);
            for (v, fills) in &views {
                if *fills {
                    let c = self.rule(Rule {
                        first: Some(&v.id),
                        first_attr: cross,
                        second: Some(&stack.id),
                        second_attr: cross,
                        constant,
                        ..Rule::default()
                    });
                    stack.constraints.push(c);
                }
            }
        }
        stack.subviews.extend(views.into_iter().map(|(v, _)| v));
    }

    /// A vertical stack of row stacks. Columns line up through equal-width
    /// constraints; empty cells get placeholder views.
    fn grid(&mut self, outer: &mut View, n: &Node, l: &Layout) {
        let spacing = l.spacing.as_ref().and_then(px);
        let column_spacing = l
            .spacing_horizontal
            .as_ref()
            .and_then(px)
            .or(spacing)
            .unwrap_or(0.0);
        let cells: Vec<(Cell, &Node)> = n
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let cell = c.item.cell.unwrap_or(Cell {
                    row: u32::try_from(i).unwrap_or(u32::MAX),
                    col: 0,
                    rowspan: 1,
                    colspan: 1,
                });
                (cell, c)
            })
            .collect();
        let rows = cells
            .iter()
            .map(|(c, _)| c.row + c.rowspan.max(1))
            .max()
            .unwrap_or(0);
        let cols = cells
            .iter()
            .map(|(c, _)| c.col + c.colspan.max(1))
            .max()
            .unwrap_or(0);
        let mut columns: Vec<Vec<String>> = vec![Vec::new(); cols as usize];
        let mut row_items = Vec::new();
        for r in 0..rows {
            let mut row = self.stack(true, column_spacing, [0.0; 4]);
            let mut items = Vec::new();
            let mut row_stretch: f64 = 0.0;
            let mut c = 0;
            while c < cols {
                let found = cells
                    .iter()
                    .find(|(cell, _)| cell.row == r && cell.col == c);
                let Some((cell, node)) = found else {
                    let v = self.spacer(true);
                    columns[c as usize].push(v.id.clone());
                    items.push(Arranged {
                        view: v,
                        stretch: None,
                        fixed_main: false,
                        fixed_cross: false,
                    });
                    c += 1;
                    continue;
                };
                if cell.rowspan > 1 || cell.colspan > 1 {
                    self.warn(
                        &node.origin,
                        format!(
                            "row-span {} and col-span {} are approximated: stack views cannot span",
                            cell.rowspan, cell.colspan
                        ),
                    );
                }
                let v = self.build(node);
                if cell.colspan <= 1 {
                    columns[c as usize].push(v.id.clone());
                }
                row_stretch = row_stretch.max(stretch(node, false));
                items.push(Arranged {
                    view: v,
                    stretch: Some(stretch(node, true)),
                    fixed_main: node.item.width.fixed.is_some(),
                    fixed_cross: node.item.height.fixed.is_some() || node.item.height.max.is_some(),
                });
                c += cell.colspan.max(1);
            }
            self.arrange(&mut row, true, Alignment::Stretch, (0.0, 0.0), items);
            row_items.push(Arranged {
                view: row,
                stretch: Some(row_stretch),
                fixed_main: false,
                fixed_cross: false,
            });
        }
        let [_, leading, _, trailing] = insets(&l.padding);
        self.arrange(
            outer,
            false,
            Alignment::Stretch,
            (leading, trailing),
            row_items,
        );
        for members in &columns {
            if let Some((base, rest)) = members.split_first() {
                for id in rest {
                    let c = self.rule(Rule {
                        first: Some(id),
                        first_attr: "width",
                        second: Some(base),
                        second_attr: "width",
                        priority: Some(STRONG),
                        ..Rule::default()
                    });
                    outer.constraints.push(c);
                }
            }
        }
    }

    /// Children outside layouts: at their `x`/`y`, else centred when they
    /// keep their own size, else filling `guide`.
    fn place_free(&mut self, parent: &mut View, guide: &str, children: &[Node]) {
        for c in children {
            let v = self.build(c);
            let fills = mapping::lookup(&c.widget).is_none_or(|w| w.fills_parent);
            for (pos, extent, start, end, center) in [
                (&c.item.x, &c.item.width, "leading", "trailing", "centerX"),
                (&c.item.y, &c.item.height, "top", "bottom", "centerY"),
            ] {
                if let Some(p) = pos.as_ref().and_then(px) {
                    let r = self.pin(&v.id, guide, start, p);
                    parent.constraints.push(r);
                } else if extent.fixed.is_some() || !fills {
                    let r = self.pin(&v.id, guide, center, 0.0);
                    parent.constraints.push(r);
                } else {
                    let a = self.pin(&v.id, guide, start, 0.0);
                    let b = self.pin(guide, &v.id, end, 0.0);
                    parent.constraints.extend([a, b]);
                }
            }
            parent.subviews.push(v);
        }
    }

    /// UIKit scroll content: pinned to the scroll view's edges, as wide as
    /// it, so the content height decides how far it scrolls.
    fn scroll(&mut self, scroll: &mut View, children: &[Node]) {
        for c in children {
            let v = self.build(c);
            for attr in ["leading", "trailing", "top", "bottom", "width"] {
                let r = self.pin(&v.id, &scroll.id, attr, 0.0);
                scroll.constraints.push(r);
            }
            scroll.subviews.push(v);
        }
    }
}

/// Padding as top, leading, bottom, trailing pixels.
fn insets(p: &Padding) -> [f64; 4] {
    let side = |v: &Option<Value>| v.as_ref().and_then(px).unwrap_or(0.0);
    [side(&p.top), side(&p.left), side(&p.bottom), side(&p.right)]
}

/// Whether the controller needs a reference to `n`: it has an id, something
/// computed, or a `for`/`if` the code must replay.
fn needs_outlet(n: &Node) -> bool {
    n.id.is_some()
        || n.structure.is_some()
        || n.properties.iter().any(|p| is_binding(&p.value))
        || n.handlers.iter().any(|h| h.kind == HandlerKind::Changed)
        || [
            &n.item.x,
            &n.item.y,
            &n.item.width.fixed,
            &n.item.height.fixed,
        ]
        .into_iter()
        .any(|v| v.as_ref().is_some_and(is_binding))
}

/// The UIKit control event for a Slint callback; on AppKit any `Some` means
/// the control's single action.
fn event(role: Option<Role>, callback: &str, ui: bool) -> Option<&'static str> {
    Some(match (role?, callback) {
        (Role::Button, "clicked") => "touchUpInside",
        (Role::CheckBox | Role::Switch, "toggled") => "valueChanged",
        (Role::LineEdit, "accepted") => "primaryActionTriggered",
        (Role::LineEdit, "edited") if ui => "editingChanged",
        (Role::Slider, "changed") => "valueChanged",
        (Role::SpinBox, "edited") => "valueChanged",
        (Role::ComboBox, "selected") => "primaryActionTriggered",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_objective_c_names() {
        assert_eq!(ident("show-footer", false), "showFooter");
        assert_eq!(ident("NestedLayouts", true), "NestedLayouts");
        assert_eq!(ident("2nd", false), "_2nd");
        assert_eq!(ident("Button", false), "button");
    }
}
