//! AST to IR. Three passes: read every component into a raw tree (the AST
//! only), inline local components into the main one, then split layout
//! properties into the layout model and resolve grid cells. Expressions are
//! classified into plain values or bindings; none is evaluated.

use std::collections::{HashMap, HashSet};

use slintx_syntax::ast::{self, AstNode, Expr, Item, Member};
use slintx_syntax::{NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken};

use crate::color;
use crate::diag::{Diagnostics, LineIndex, Origin};
use crate::ir::*;
use crate::mapping::{self, Role};

/// The placeholder `@children` leaves in a raw tree until inlining fills it.
const CHILDREN: &str = "@children";

/// The widget names the standard library provides; importing them is fine.
const STD_WIDGETS: &str = "std-widgets.slint";

/// Properties whose bare identifiers are enum values, not references.
const ENUM_PROPERTIES: &[&str] = &[
    "alignment",
    "horizontal-alignment",
    "vertical-alignment",
    "wrap",
    "overflow",
    "image-fit",
    "image-rendering",
    "orientation",
    "mouse-cursor",
    "input-type",
    "text-direction",
    "stroke-line-cap",
    "accessible-role",
];

/// Properties of type `color` or `brush`, where a bare CSS color name is a value.
const COLOR_PROPERTIES: &[&str] = &[
    "background",
    "color",
    "border-color",
    "fill",
    "stroke",
    "selection-background-color",
    "selection-foreground-color",
];

/// Enum types whose `Type.value` paths are plain values.
const ENUM_TYPES: &[&str] = &[
    "LayoutAlignment",
    "TextHorizontalAlignment",
    "TextVerticalAlignment",
    "TextWrap",
    "TextOverflow",
    "ImageFit",
    "ImageRendering",
    "Orientation",
    "MouseCursor",
    "InputType",
    "AccessibleRole",
    "StandardButtonKind",
];

pub fn lower(file: &str, source: &str, root: &ast::File, diags: &mut Diagnostics) -> Document {
    let mut cx = Lowerer {
        index: LineIndex::new(file, source),
        diags,
        components: Vec::new(),
        imported: HashMap::new(),
    };
    for item in root.items() {
        cx.item(item, false);
    }
    let infos = cx.components.iter().map(|c| c.info.clone()).collect();
    let main = cx
        .components
        .iter()
        .rposition(|c| c.info.exported)
        .or_else(|| cx.components.len().checked_sub(1));
    let root = main.map(|i| {
        let mut node = cx.expand_main(i);
        cx.normalize(&mut node, None);
        node
    });
    Document {
        file: file.to_owned(),
        components: infos,
        root,
    }
}

struct RawComponent {
    info: ComponentInfo,
    root: Node,
}

struct Lowerer<'a> {
    index: LineIndex<'a>,
    diags: &'a mut Diagnostics,
    components: Vec<RawComponent>,
    /// Names imported from files other than `std-widgets.slint`, which are
    /// not resolved.
    imported: HashMap<String, String>,
}

impl Lowerer<'_> {
    fn origin(&self, node: &SyntaxNode) -> Origin {
        self.index.origin(node.text_range())
    }

    fn item(&mut self, item: Item, exported: bool) {
        match item {
            Item::Component(c) => self.component(&c, exported),
            Item::Export(e) => {
                if let Some(declaration) = e.declaration() {
                    self.item(declaration, true);
                }
                for spec in e.specifiers() {
                    let Some(name) = spec.name() else { continue };
                    if let Some(c) = self
                        .components
                        .iter_mut()
                        .find(|c| c.info.name == name.text())
                    {
                        c.info.exported = true;
                    }
                }
            }
            Item::Import(i) => {
                let path = i.path().map(|p| unquote(p.text())).unwrap_or_default();
                if path == STD_WIDGETS {
                    return;
                }
                for spec in i.specifiers() {
                    let local = spec.alias().or_else(|| spec.name());
                    if let Some(local) = local {
                        self.imported.insert(local.text().to_owned(), path.clone());
                    }
                }
            }
            // Globals, structs and enums only matter to expressions, which
            // are kept as text.
            Item::Global(_) | Item::Struct(_) | Item::Enum(_) => {}
        }
    }

    fn component(&mut self, c: &ast::Component, exported: bool) {
        let origin = self.origin(c.syntax());
        let name = c.name().map(|t| t.text().to_owned()).unwrap_or_default();
        let base = c
            .base()
            .map(|q| qualified(&q))
            .unwrap_or_else(|| "Empty".to_owned());
        let mut root = self.empty_node(base.clone(), origin.clone());
        let mut callbacks = Vec::new();
        if let Some(body) = c.body() {
            self.body(&body, &mut root, Some(&mut callbacks));
        }
        let properties = std::mem::take(&mut root.declarations);
        let info = ComponentInfo {
            name,
            exported,
            base,
            properties,
            callbacks,
            origin,
        };
        self.components.push(RawComponent { info, root });
    }

    fn empty_node(&self, widget: String, origin: Origin) -> Node {
        Node {
            widget,
            id: None,
            component: None,
            layout: None,
            item: LayoutItem::default(),
            properties: Vec::new(),
            handlers: Vec::new(),
            declarations: Vec::new(),
            structure: None,
            unsupported: Vec::new(),
            children: Vec::new(),
            origin,
        }
    }

    fn element(&mut self, e: &ast::Element) -> Node {
        let origin = self.origin(e.syntax());
        let widget = e.base().map(|q| qualified(&q)).unwrap_or_default();
        let mut node = self.empty_node(widget, origin);
        node.id = e.id().map(|t| t.text().to_owned());
        if let Some(body) = e.body() {
            self.body(&body, &mut node, None);
        }
        node
    }

    /// Reads an element body into `node`. `callbacks` is the component's list
    /// when `node` is a component root; elsewhere a callback declaration is
    /// reported as not converted.
    fn body(
        &mut self,
        body: &ast::ElementBody,
        node: &mut Node,
        mut callbacks: Option<&mut Vec<CallbackDecl>>,
    ) {
        for member in body.members() {
            let origin = self.origin(member.syntax());
            match member {
                Member::Element(e) => {
                    let child = self.element(&e);
                    node.children.push(child);
                }
                Member::Binding(b) => {
                    let Some(name) = b.name() else { continue };
                    let name = name.text().to_owned();
                    let value = match b.value() {
                        Some(v) => self.binding_expression(&name, &v),
                        None => continue,
                    };
                    node.properties.push(Property {
                        name,
                        value,
                        origin,
                    });
                }
                Member::TwoWayBinding(b) => {
                    let (Some(name), Some(target)) = (b.name(), b.target()) else {
                        continue;
                    };
                    node.properties.push(Property {
                        name: name.text().to_owned(),
                        value: Value::TwoWay(binding(target.syntax())),
                        origin,
                    });
                }
                Member::CallbackConnection(c) => {
                    let Some(name) = c.name() else { continue };
                    let body = c
                        .block()
                        .map(|b| b.syntax().clone())
                        .or_else(|| c.statement().map(|s| s.syntax().clone()));
                    node.handlers.push(Handler {
                        kind: HandlerKind::Callback,
                        name: name.text().to_owned(),
                        params: c.params().map(|t| t.text().to_owned()).collect(),
                        body: body.map(|b| normalized(&b)).unwrap_or_default(),
                        origin,
                    });
                }
                Member::ChangedHandler(c) => {
                    let Some(property) = c.property() else {
                        continue;
                    };
                    let body = c
                        .block()
                        .map(|b| b.syntax().clone())
                        .or_else(|| c.statement().map(|s| s.syntax().clone()));
                    node.handlers.push(Handler {
                        kind: HandlerKind::Changed,
                        name: property.text().to_owned(),
                        params: Vec::new(),
                        body: body.map(|b| normalized(&b)).unwrap_or_default(),
                        origin,
                    });
                }
                Member::PropertyDeclaration(p) => {
                    let Some(name) = p.name() else { continue };
                    let name = name.text().to_owned();
                    let default = match (p.value(), p.alias()) {
                        (Some(v), _) => Some(self.binding_expression(&name, &v)),
                        (None, Some(alias)) => Some(Value::TwoWay(binding(alias.syntax()))),
                        (None, None) => None,
                    };
                    node.declarations.push(PropertyDecl {
                        name,
                        ty: p.ty().map(|t| normalized(t.syntax())).unwrap_or_default(),
                        visibility: p.visibility().map(|t| t.text().to_owned()),
                        default,
                        origin,
                    });
                }
                Member::CallbackDeclaration(c) => {
                    let Some(name) = c.name() else { continue };
                    let decl = CallbackDecl {
                        name: name.text().to_owned(),
                        params: c
                            .params()
                            .map(|p| p.ty().map(|t| normalized(t.syntax())).unwrap_or_default())
                            .collect(),
                        return_type: c.return_type().map(|t| normalized(t.syntax())),
                        origin: origin.clone(),
                    };
                    match callbacks.as_deref_mut() {
                        Some(list) => list.push(decl),
                        None => self.unsupported(
                            node,
                            &origin,
                            "callback declaration on an inner element",
                        ),
                    }
                }
                // Functions are logic; the converter never runs or translates it.
                Member::FunctionDeclaration(_) => {}
                Member::Animate(_) => self.unsupported(node, &origin, "animation"),
                Member::States(_) => self.unsupported(node, &origin, "states"),
                Member::Repeater(r) => {
                    let Some(e) = r.element() else { continue };
                    let mut child = self.element(&e);
                    child.structure = Some(Structure::Repeat {
                        item: r.variable().map(|t| t.text().to_owned()),
                        index: r
                            .index()
                            .and_then(|i| i.name())
                            .map(|t| t.text().to_owned()),
                        model: r
                            .model()
                            .map(|m| binding(m.syntax()))
                            .unwrap_or_else(empty_binding),
                    });
                    node.children.push(child);
                }
                Member::Conditional(c) => {
                    let Some(e) = c.element() else { continue };
                    let mut child = self.element(&e);
                    child.structure = Some(Structure::Condition {
                        condition: c
                            .condition()
                            .map(|m| binding(m.syntax()))
                            .unwrap_or_else(empty_binding),
                    });
                    node.children.push(child);
                }
                Member::ChildrenPlaceholder(_) => {
                    node.children
                        .push(self.empty_node(CHILDREN.to_owned(), origin));
                }
            }
        }
    }

    fn unsupported(&mut self, node: &mut Node, origin: &Origin, what: &str) {
        self.diags.warn(origin, format!("not converted: {what}"));
        node.unsupported.push(Unsupported {
            what: what.to_owned(),
            origin: origin.clone(),
        });
    }

    fn binding_expression(&self, property: &str, b: &ast::BindingExpression) -> Value {
        match b.expr() {
            Some(e) if b.block().is_none() => value(property, &e),
            _ => Value::Binding(binding(b.syntax())),
        }
    }

    // --- inlining -----------------------------------------------------------

    fn find(&self, name: &str) -> Option<usize> {
        self.components.iter().rposition(|c| c.info.name == name)
    }

    /// The main component keeps its own property references: they are its
    /// interface, which backends turn into outlets and template properties.
    fn expand_main(&mut self, index: usize) -> Node {
        let component = &self.components[index];
        let mut node = component.root.clone();
        let declarations = component.info.properties.clone();
        let name = component.info.name.clone();
        let mut stack = vec![name.clone()];
        self.expand_children(&mut node, &mut stack);
        if let Some(base) = self.find(&node.widget) {
            node = self.inline(base, node, &mut stack);
        }
        let inherited = std::mem::take(&mut node.declarations);
        node.declarations = declarations;
        node.declarations.extend(inherited);
        node.component = Some(name);
        node
    }

    fn expand_children(&mut self, node: &mut Node, stack: &mut Vec<String>) {
        let children = std::mem::take(&mut node.children);
        for child in children {
            let expanded = self.expand(child, stack);
            node.children.push(expanded);
        }
    }

    fn expand(&mut self, mut node: Node, stack: &mut Vec<String>) -> Node {
        match self.find(&node.widget) {
            Some(_) if stack.contains(&node.widget) => {
                let origin = node.origin.clone();
                self.diags.error(
                    &origin,
                    format!("component `{}` contains itself", node.widget),
                );
                node.children.clear();
                node
            }
            Some(index) => self.inline(index, node, stack),
            None => {
                self.expand_children(&mut node, stack);
                node
            }
        }
    }

    /// Replaces a use of a local component with the component's body: its
    /// properties under the use site's, its children with the use site's in
    /// place of `@children`, and `root.x` references to its interface
    /// replaced by the values the use site gives.
    fn inline(&mut self, index: usize, site: Node, stack: &mut Vec<String>) -> Node {
        let component = &self.components[index];
        let name = component.info.name.clone();
        let mut node = component.root.clone();
        let mut declarations = component.info.properties.clone();

        let mut site_properties = Vec::new();
        for p in site.properties {
            match declarations.iter_mut().find(|d| d.name == p.name) {
                Some(d) => d.default = Some(p.value),
                None => site_properties.push(p),
            }
        }
        substitute(&mut node, &declarations, true);

        for p in site_properties {
            node.properties.retain(|q| q.name != p.name);
            node.properties.push(p);
        }
        node.handlers.extend(site.handlers);
        node.unsupported.extend(site.unsupported);
        node.declarations = declarations;
        node.id = site.id.or(node.id);
        node.structure = site.structure;
        node.origin = site.origin;
        node.component = Some(name.clone());

        let mut site_children = Some(site.children);
        fill_children(&mut node, &mut site_children);
        node.children.extend(site_children.unwrap_or_default());

        stack.push(name.clone());
        let mut node = self.expand(node, stack);
        stack.pop();
        node.component = Some(name);
        node
    }

    // --- layout model -------------------------------------------------------

    fn normalize(&mut self, node: &mut Node, parent: Option<Role>) {
        let widget = mapping::lookup(&node.widget);
        if widget.is_none() && node.widget != CHILDREN {
            let what = match self.imported.get(&node.widget) {
                Some(path) => format!("element `{}` imported from \"{path}\"", node.widget),
                None => format!("unknown element `{}`", node.widget),
            };
            let origin = node.origin.clone();
            self.unsupported(node, &origin, &what);
        }
        if matches!(widget.map(|w| w.role), Some(Role::GridRow))
            && parent != Some(Role::Layout(LayoutKind::Grid))
        {
            let origin = node.origin.clone();
            self.unsupported(node, &origin, "`Row` outside a `GridLayout`");
        }
        node.children.retain(|c| c.widget != CHILDREN);
        let cell = node.item.cell;
        node.item = take_item(&mut node.properties);
        node.item.cell = cell;
        let role = widget.map(|w| w.role);
        if let Some(Role::Layout(kind)) = role {
            node.layout = Some(take_layout(kind, &mut node.properties));
        }
        if role == Some(Role::Layout(LayoutKind::Grid)) {
            self.resolve_grid(node);
        }
        for child in &mut node.children {
            self.normalize(child, role);
        }
    }

    /// Flattens `Row`s and gives every grid child its cell, following Slint:
    /// each `Row` starts a new row, a child without `col` takes the column after
    /// the previous one, and a child without `row` stays in the current row.
    fn resolve_grid(&mut self, grid: &mut Node) {
        let mut row = 0u32;
        let mut col = 0u32;
        let mut started = false;
        let mut row_closed = false;
        let mut cells = Vec::new();
        for child in std::mem::take(&mut grid.children) {
            let is_row = mapping::lookup(&child.widget).map(|w| w.role) == Some(Role::GridRow);
            if is_row {
                if started {
                    row += 1;
                }
                col = 0;
                started = true;
                if !child.properties.is_empty() || !child.handlers.is_empty() {
                    let origin = child.origin.clone();
                    self.diags
                        .warn(&origin, "properties on `Row` are not converted");
                }
                for mut cell_child in child.children {
                    let cell = place(&mut cell_child.properties, &mut row, &mut col, true);
                    cell_child.item.cell = Some(cell);
                    cells.push(cell_child);
                }
                row_closed = true;
            } else {
                if row_closed {
                    row += 1;
                    col = 0;
                    row_closed = false;
                }
                started = true;
                let mut child = child;
                let cell = place(&mut child.properties, &mut row, &mut col, false);
                child.item.cell = Some(cell);
                cells.push(child);
            }
        }
        grid.children = cells;
    }
}

/// Puts the use site's children where the component wrote `@children`, at any
/// depth; whatever is left over is appended by the caller.
fn fill_children(node: &mut Node, site_children: &mut Option<Vec<Node>>) {
    let children = std::mem::take(&mut node.children);
    for mut child in children {
        if child.widget == CHILDREN {
            node.children
                .extend(site_children.take().unwrap_or_default());
        } else {
            fill_children(&mut child, site_children);
            node.children.push(child);
        }
    }
}

/// Reads `row`, `col`, `rowspan` and `colspan`, advancing the cursor.
fn place(properties: &mut Vec<Property>, row: &mut u32, col: &mut u32, in_row: bool) -> Cell {
    let mut number = |name: &str| {
        take(properties, name)
            .and_then(|v| v.as_number())
            .filter(|n| *n >= 0.0 && n.fract() == 0.0)
            .map(|n| n as u32)
    };
    let explicit_row = number("row");
    let explicit_col = number("col");
    let rowspan = number("rowspan").unwrap_or(1).max(1);
    let colspan = number("colspan").unwrap_or(1).max(1);
    if let Some(r) = explicit_row.filter(|_| !in_row) {
        if r != *row {
            *col = 0;
        }
        *row = r;
    }
    let cell = Cell {
        row: explicit_row.unwrap_or(*row),
        col: explicit_col.unwrap_or(*col),
        rowspan,
        colspan,
    };
    *col = cell.col + colspan;
    cell
}

fn take(properties: &mut Vec<Property>, name: &str) -> Option<Value> {
    let index = properties.iter().position(|p| p.name == name)?;
    Some(properties.remove(index).value)
}

fn take_item(properties: &mut Vec<Property>) -> LayoutItem {
    let mut extent = |axis: &str, size: &str| Extent {
        fixed: take(properties, size),
        min: take(properties, &format!("min-{size}")),
        max: take(properties, &format!("max-{size}")),
        preferred: take(properties, &format!("preferred-{size}")),
        stretch: take(properties, &format!("{axis}-stretch")).and_then(|v| v.as_number()),
    };
    let width = extent("horizontal", "width");
    let height = extent("vertical", "height");
    LayoutItem {
        width,
        height,
        x: take(properties, "x"),
        y: take(properties, "y"),
        cell: None,
    }
}

fn take_layout(kind: LayoutKind, properties: &mut Vec<Property>) -> Layout {
    let all = take(properties, "padding");
    let mut side = |name: &str| take(properties, name).or_else(|| all.clone());
    let padding = Padding {
        left: side("padding-left"),
        right: side("padding-right"),
        top: side("padding-top"),
        bottom: side("padding-bottom"),
    };
    let alignment = take(properties, "alignment").and_then(|v| match v {
        Value::Enum(name) => Alignment::parse(&name),
        _ => None,
    });
    Layout {
        kind,
        spacing: take(properties, "spacing"),
        spacing_horizontal: take(properties, "spacing-horizontal"),
        spacing_vertical: take(properties, "spacing-vertical"),
        padding,
        alignment,
    }
}

/// Replaces `root.name` (and, on the component root, a bare `name`) where
/// `name` is a declared property with a plain value. Inner component uses are
/// not entered: their `root` is a different element.
fn substitute(node: &mut Node, declarations: &[PropertyDecl], is_root: bool) {
    let lookup = |expr: &str| -> Option<Value> {
        let name = expr
            .strip_prefix("root.")
            .or_else(|| is_root.then_some(expr))?;
        let value = declarations
            .iter()
            .find(|d| d.name == name)?
            .default
            .clone()?;
        (!matches!(value, Value::TwoWay(_))).then_some(value)
    };
    for p in &mut node.properties {
        if let Value::Binding(b) = &p.value
            && let Some(v) = lookup(&b.expr)
        {
            p.value = v;
        }
    }
    for child in &mut node.children {
        substitute(child, declarations, false);
    }
}

// --- expressions --------------------------------------------------------------

fn qualified(q: &ast::QualifiedName) -> String {
    q.segments()
        .map(|t| t.text().to_owned())
        .collect::<Vec<_>>()
        .join(".")
}

fn empty_binding() -> Binding {
    Binding {
        expr: String::new(),
        references: Vec::new(),
    }
}

/// Source text with comments dropped and every run of trivia turned into one
/// space, so formatting changes do not change the output.
pub(crate) fn normalized(node: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut gap = false;
    for token in node
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
    {
        if token.kind().is_trivia() {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push(' ');
        }
        gap = false;
        out.push_str(token.text());
    }
    out
}

pub(crate) fn binding(node: &SyntaxNode) -> Binding {
    let mut references = Vec::new();
    let mut receivers = HashSet::new();
    for n in node.descendants() {
        if let Some(f) = ast::FieldAccess::cast(n.clone())
            && let (Some(Expr::NameRef(r)), Some(field)) = (f.receiver(), f.field())
            && let Some(name) = r.name()
        {
            references.push(format!("{}.{}", name.text(), field.text()));
            receivers.insert(r.syntax().clone());
        }
    }
    for n in node.descendants() {
        if let Some(r) = ast::NameRef::cast(n)
            && !receivers.contains(r.syntax())
            && let Some(name) = r.name()
            && !matches!(name.text(), "true" | "false")
        {
            references.push(name.text().to_owned());
        }
    }
    references.sort();
    references.dedup();
    Binding {
        expr: normalized(node),
        references,
    }
}

/// A plain value when the expression is a literal the targets can carry,
/// otherwise a binding.
fn value(property: &str, expr: &Expr) -> Value {
    plain(property, expr).unwrap_or_else(|| Value::Binding(binding(expr.syntax())))
}

fn plain(property: &str, expr: &Expr) -> Option<Value> {
    match expr {
        Expr::Literal(l) => literal(&l.token()?),
        Expr::ParenExpr(p) => plain(property, &p.expr()?),
        Expr::UnaryExpr(u) if u.op()?.kind() == SyntaxKind::Minus => {
            match plain(property, &u.operand()?)? {
                Value::Number(n) => Some(Value::Number(-n)),
                Value::Measure { value, unit } => Some(Value::Measure {
                    value: -value,
                    unit,
                }),
                _ => None,
            }
        }
        Expr::NameRef(r) => {
            let name = r.name()?;
            match name.text() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                text if ENUM_PROPERTIES.contains(&property) => Some(Value::Enum(text.to_owned())),
                text if COLOR_PROPERTIES.contains(&property) && color::is_named(text) => {
                    Some(Value::Color(text.to_owned()))
                }
                _ => None,
            }
        }
        Expr::FieldAccess(f) => {
            let Expr::NameRef(r) = f.receiver()? else {
                return None;
            };
            let ty = r.name()?;
            let field = f.field()?.text().to_owned();
            match ty.text() {
                "Colors" => Some(Value::Color(field)),
                t if ENUM_TYPES.contains(&t) => Some(Value::Enum(field)),
                _ => None,
            }
        }
        Expr::TrMacro(t) => {
            if t.plural().is_some() || t.args().next().is_some() {
                return None;
            }
            let text = unescape(t.message()?.token()?.text())?;
            Some(Value::String {
                text,
                translatable: true,
            })
        }
        Expr::ImageUrlMacro(i) => {
            if i.nine_slice().is_some() {
                return None;
            }
            Some(Value::Url(unescape(i.path()?.token()?.text())?))
        }
        Expr::ArrayLiteral(a) => a
            .elements()
            .map(|e| plain(property, &e))
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
        _ => None,
    }
}

fn literal(token: &SyntaxToken) -> Option<Value> {
    let text = token.text();
    match token.kind() {
        SyntaxKind::Number => {
            let split = text
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(text.len());
            let (number, suffix) = text.split_at(split);
            let value: f64 = number.parse().ok()?;
            if suffix.is_empty() {
                Some(Value::Number(value))
            } else {
                Some(Value::Measure {
                    value,
                    unit: Unit::parse(suffix)?,
                })
            }
        }
        SyntaxKind::Color => Some(Value::Color(text.to_owned())),
        SyntaxKind::String => Some(Value::String {
            text: unescape(text)?,
            translatable: false,
        }),
        _ => None,
    }
}

fn unquote(text: &str) -> String {
    unescape(text).unwrap_or_default()
}

/// The contents of a string token: quotes removed, `\"`, `\\`, `\n` and
/// `\u{...}` decoded. `None` for an escape Slint rejects.
fn unescape(token: &str) -> Option<String> {
    let inner = token.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_decodes_slint_escapes() {
        assert_eq!(
            unescape(r#""a\"b\\c\nd\u{1F600}""#).as_deref(),
            Some("a\"b\\c\nd😀")
        );
        assert_eq!(unescape(r#""\q""#), None);
    }

    #[test]
    fn numbers_keep_their_unit() {
        let parse = slintx_syntax::parse_binding_expression("12.5px");
        let Some(b) = parse.tree::<ast::BindingExpression>() else {
            unreachable!("binding root")
        };
        let Some(expr) = b.expr() else {
            unreachable!("expression")
        };
        assert_eq!(
            value("width", &expr),
            Value::Measure {
                value: 12.5,
                unit: Unit::Px
            }
        );
    }

    #[test]
    fn bindings_list_the_paths_they_read() {
        let parse = slintx_syntax::parse_binding_expression("root.count + step * 2");
        let Some(b) = parse.tree::<ast::BindingExpression>() else {
            unreachable!("binding root")
        };
        let found = binding(b.syntax());
        assert_eq!(found.expr, "root.count + step * 2");
        assert_eq!(found.references, ["root.count", "step"]);
    }
}
