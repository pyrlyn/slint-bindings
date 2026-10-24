//! The slintx-builtins catalog against Slint: one generated file uses every
//! catalogued name with its catalogued type, and Slint must accept it. A name
//! Slint does not have, a wrong type or a wrong arity fails here. Names Slint
//! has and the catalog lacks are not found this way; they come from the docs.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::fmt::Write as _;

use slint_interpreter::{Compiler, DiagnosticLevel};
use slintx_syntax::ast::{self, AstNode, Item, Member};

#[derive(Debug, Default)]
struct Members {
    base: Option<String>,
    properties: Vec<(String, String, String)>,
    callbacks: Vec<Signature>,
    functions: Vec<Signature>,
}

#[derive(Debug)]
struct Signature {
    name: String,
    params: Vec<String>,
    returns: Option<String>,
}

#[derive(Debug, Default)]
struct Catalog {
    /// Exported components first in source order, then the helper bases.
    components: Vec<(String, bool, Members)>,
    globals: Vec<(String, bool, Members)>,
    structs: HashMap<String, Vec<(String, String)>>,
    enums: HashMap<String, Vec<String>>,
    /// Exported structs and enums, the ones user code can name.
    struct_order: Vec<String>,
    enum_order: Vec<String>,
}

fn text(node: &impl AstNode) -> String {
    node.syntax().to_string().trim().to_owned()
}

fn members(body: Option<ast::ElementBody>) -> Members {
    let mut out = Members::default();
    for member in body.iter().flat_map(ast::ElementBody::members) {
        match member {
            Member::PropertyDeclaration(p) => {
                let (Some(name), Some(ty)) = (p.name(), p.ty()) else {
                    continue;
                };
                let visibility = p
                    .visibility()
                    .map_or("private".into(), |v| v.text().to_owned());
                out.properties
                    .push((name.text().to_owned(), text(&ty), visibility));
            }
            Member::CallbackDeclaration(c) => {
                let Some(name) = c.name() else { continue };
                out.callbacks.push(Signature {
                    name: name.text().to_owned(),
                    params: c
                        .params()
                        .filter_map(|p| p.ty())
                        .map(|t| text(&t))
                        .collect(),
                    returns: c.return_type().map(|t| text(&t)),
                });
            }
            Member::FunctionDeclaration(f) => {
                let Some(name) = f.name() else { continue };
                out.functions.push(Signature {
                    name: name.text().to_owned(),
                    params: f
                        .params()
                        .filter_map(|p| p.ty())
                        .map(|t| text(&t))
                        .collect(),
                    returns: f.return_type().map(|t| text(&t)),
                });
            }
            _ => {}
        }
    }
    out
}

fn catalog() -> Catalog {
    let parse = slintx_builtins::parse();
    let mut catalog = Catalog::default();
    for item in parse.tree::<ast::File>().iter().flat_map(ast::File::items) {
        let (exported, item) = match item {
            Item::Export(e) => (true, e.declaration()),
            item => (false, Some(item)),
        };
        match item {
            Some(Item::Component(c)) => {
                let Some(name) = c.name() else { continue };
                let mut m = members(c.body());
                m.base = c.base().map(|b| text(&b));
                catalog
                    .components
                    .push((name.text().to_owned(), exported, m));
            }
            Some(Item::Global(g)) => {
                let Some(name) = g.name() else { continue };
                catalog
                    .globals
                    .push((name.text().to_owned(), exported, members(g.body())));
            }
            Some(Item::Struct(s)) => {
                let Some(name) = s.name() else { continue };
                let fields = s
                    .fields()
                    .filter_map(|f| Some((f.name()?.text().to_owned(), text(&f.ty()?))))
                    .collect();
                if exported {
                    catalog.struct_order.push(name.text().to_owned());
                }
                catalog.structs.insert(name.text().to_owned(), fields);
            }
            Some(Item::Enum(e)) => {
                let Some(name) = e.name() else { continue };
                let values = e
                    .values()
                    .filter_map(|v| v.name())
                    .map(|v| v.text().to_owned());
                if exported {
                    catalog.enum_order.push(name.text().to_owned());
                }
                catalog
                    .enums
                    .insert(name.text().to_owned(), values.collect());
            }
            _ => {}
        }
    }
    catalog
}

impl Catalog {
    /// A constant of type `ty`, if one can be written.
    fn literal(&self, ty: &str) -> Option<String> {
        let simple = match ty {
            "length" => "0px",
            "float" | "int" | "numeric" => "0",
            "bool" => "false",
            "string" => "\"\"",
            "brush" | "color" => "#000000",
            "angle" => "0deg",
            "duration" => "0ms",
            "easing" => "ease",
            "keys" => "@keys(A)",
            "styled-text" => "@markdown(\"\")",
            "image" => "@image-url(\"\")",
            _ => "",
        };
        if !simple.is_empty() {
            return Some(simple.into());
        }
        if let Some(values) = self.enums.get(ty) {
            return Some(format!("{ty}.{}", values.first()?));
        }
        let fields = self.structs.get(ty)?;
        let fields = fields
            .iter()
            .map(|(name, ty)| Some(format!("{name}: {}", self.literal(ty)?)))
            .collect::<Option<Vec<_>>>()?;
        Some(format!("{{ {} }}", fields.join(", ")))
    }

    fn nameable(&self, ty: &str) -> bool {
        let hidden = |order: &[String], known: bool| known && !order.iter().any(|n| n == ty);
        !hidden(&self.struct_order, self.structs.contains_key(ty))
            && !hidden(&self.enum_order, self.enums.contains_key(ty))
    }

    fn args(&self, params: &[String]) -> Option<String> {
        let args = params
            .iter()
            .map(|p| self.literal(p))
            .collect::<Option<Vec<_>>>()?;
        Some(args.join(", "))
    }
}

/// Generated Slint, with a counter for unique property names.
#[derive(Default)]
struct Out {
    root: String,
    extra: String,
    n: usize,
}

impl Out {
    fn next(&mut self) -> usize {
        self.n += 1;
        self.n
    }

    fn read(&mut self, ty: &str, expr: &str) {
        // A `numeric` result has the type of its arguments, which are floats here.
        let ty = if ty == "numeric" { "float" } else { ty };
        let n = self.next();
        let _ = writeln!(self.root, "    out property <{ty}> r{n}: {expr};");
    }
}

/// Where an element can stand: the code around it and the id to reach it by.
fn placement(element: &str) -> (&'static str, &'static str) {
    match element {
        "MoveTo" | "LineTo" | "ArcTo" | "CubicTo" | "QuadraticTo" | "Close" => ("Path {", "}"),
        "Row" => ("GridLayout {", "}"),
        "KeyBinding" => ("FocusScope {", "}"),
        _ => ("", ""),
    }
}

/// Properties Slint takes only in some places (a grid cell, a box layout, a
/// dialog), or not with their first enum value; they are read instead.
const CONTEXTUAL: &[&str] = &[
    "col",
    "row",
    "colspan",
    "rowspan",
    "layout-order",
    "cross-axis-self-alignment",
    "dialog-button-role",
    "cross-axis-alignment",
];

/// A window takes one menu bar, so the menu elements are checked nested in it.
const MENUS: &[&str] = &["MenuBar", "Menu", "MenuItem", "MenuSeparator"];

/// Bindings and handlers for `element`'s own members, plus reads into `out`.
fn element_body(catalog: &Catalog, id: &str, m: &Members, out: &mut Out) -> String {
    let mut body = String::new();
    for (name, ty, visibility) in &m.properties {
        match catalog.literal(ty) {
            // Path.commands can only be set; `out` ones only read.
            Some(value) if visibility != "out" && !CONTEXTUAL.contains(&name.as_str()) => {
                let _ = writeln!(body, "        {name}: {value};");
            }
            _ => out.read(ty, &format!("{id}.{name}")),
        }
    }
    for c in &m.callbacks {
        let params: Vec<_> = (0..c.params.len()).map(|i| format!("a{i}")).collect();
        let ret = match &c.returns {
            Some(ty) => format!("return {};", catalog.literal(ty).unwrap_or_default()),
            None => String::new(),
        };
        let _ = writeln!(
            body,
            "        {}({}) => {{ {ret} }}",
            c.name,
            params.join(", ")
        );
    }
    for f in &m.functions {
        let args = catalog.args(&f.params).unwrap_or_default();
        let call = match &f.returns {
            Some(ty) => format!("let v{}: {ty} = {id}.{}({args});", out.next(), f.name),
            None => format!("{id}.{}({args});", f.name),
        };
        let _ = writeln!(out.extra, "        {call}");
    }
    body
}

fn generate(catalog: &Catalog) -> String {
    let mut out = Out::default();
    let mut children = String::new();
    let mut tops = String::new();
    let mut menus = HashMap::new();
    let helpers: HashMap<&str, &Members> = catalog
        .components
        .iter()
        .filter(|(_, exported, _)| !exported)
        .map(|(name, _, m)| (name.as_str(), m))
        .collect();
    for (i, (name, exported, m)) in catalog.components.iter().enumerate() {
        if !exported {
            continue;
        }
        let id = format!("e{i}");
        // Each helper base is checked once, through the first element built on it.
        let mut own = vec![m];
        let mut base = m.base.as_deref();
        while let Some(b) = base.and_then(|b| helpers.get(b).map(|h| (b, h))) {
            if !children.contains(&format!("// base {}", b.0)) {
                children.push_str(&format!("    // base {}\n", b.0));
                own.push(b.1);
            }
            base = b.1.base.as_deref();
        }
        match name.as_str() {
            "PropertyAnimation" => {
                let fields: String = m
                    .properties
                    .iter()
                    .filter_map(|(f, ty, _)| Some(format!("{f}: {}; ", catalog.literal(ty)?)))
                    .collect();
                let _ = writeln!(children, "    Rectangle {{ animate x {{ {fields}}} }}");
            }
            "Window" | "Dialog" | "SystemTrayIcon" => {
                let mut top = Out {
                    n: out.n,
                    ..Out::default()
                };
                let body: String = own
                    .iter()
                    .map(|m| element_body(catalog, "self", m, &mut top))
                    .collect();
                out.n = top.n;
                let child = if name == "Dialog" {
                    "    Rectangle {}\n"
                } else {
                    ""
                };
                let init = if top.extra.is_empty() {
                    String::new()
                } else {
                    format!("    init => {{\n{}    }}\n", top.extra)
                };
                let _ = write!(
                    tops,
                    "export component Probe{name} inherits {name} {{\n{body}{}{child}{init}}}\n\n",
                    top.root
                );
            }
            _ => {
                let mut body: String = own
                    .iter()
                    .map(|m| element_body(catalog, &id, m, &mut out))
                    .collect();
                if MENUS.contains(&name.as_str()) {
                    menus.insert(name.as_str(), format!("{id} := {name} {{\n{body}"));
                    continue;
                }
                if name == "ContextMenuArea" {
                    body.push_str("        Menu {}\n");
                }
                let (open, close) = placement(name);
                let _ = writeln!(
                    children,
                    "    {open} {id} := {name} {{\n{body}    }} {close}"
                );
            }
        }
    }
    let menu = |name: &str| menus.get(name).cloned().unwrap_or_default();
    let _ = writeln!(
        children,
        "    {} {} {} }} {} }} }} }}",
        menu("MenuBar"),
        menu("Menu"),
        menu("MenuItem"),
        menu("MenuSeparator")
    );
    for name in &catalog.enum_order {
        for value in &catalog.enums[name] {
            out.read(name, &format!("{name}.{value}"));
        }
    }
    for name in &catalog.struct_order {
        let var = format!("s{}", out.next());
        let _ = writeln!(out.root, "    property <{name}> {var};");
        for (field, ty) in &catalog.structs[name] {
            if catalog.nameable(ty) {
                out.read(ty, &format!("{var}.{field}"));
            }
        }
    }
    for (name, exported, m) in &catalog.globals {
        let prefix = if *exported {
            format!("{name}.")
        } else {
            String::new()
        };
        for (p, ty, _) in &m.properties {
            out.read(ty, &format!("{prefix}{p}"));
        }
        for c in &m.callbacks {
            let args = catalog.args(&c.params).unwrap_or_default();
            out.read(
                c.returns.as_deref().unwrap_or("void"),
                &format!("{prefix}{}({args})", c.name),
            );
        }
        for f in &m.functions {
            let args = catalog.args(&f.params).unwrap_or_default();
            let call = match &f.returns {
                Some(ty) => format!("let v{}: {ty} = {prefix}{}({args});", out.next(), f.name),
                None => format!("{prefix}{}({args});", f.name),
            };
            let _ = writeln!(out.extra, "        {call}");
        }
    }
    format!(
        "{tops}export component Probe inherits Window {{\n{}{children}    init => {{\n{}    }}\n}}\n",
        out.root, out.extra
    )
}

#[test]
fn slint_accepts_every_catalogued_name() {
    let catalog = catalog();
    assert!(catalog.components.len() > 40 && catalog.enums.len() > 30);
    let source = generate(&catalog);
    let mut compiler = Compiler::default();
    compiler.set_style("fluent".into());
    let result =
        pollster::block_on(compiler.build_from_source(source.clone(), "probe.slint".into()));
    let errors: Vec<String> = result
        .diagnostics()
        .filter(|d| d.level() == DiagnosticLevel::Error)
        .map(|d| {
            let (line, _) = d.line_column();
            let code = source
                .lines()
                .nth(line.saturating_sub(1))
                .unwrap_or_default();
            format!("{line}: {}\n    {}", d.message(), code.trim())
        })
        .collect();
    assert!(
        errors.is_empty(),
        "{} errors:\n{}",
        errors.len(),
        errors.join("\n")
    );
}
