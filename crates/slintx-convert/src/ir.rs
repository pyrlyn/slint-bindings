//! The format-free intermediate representation every backend reads: a widget
//! tree with its layout model, typed property values, bindings and handlers,
//! each node tagged with where it came from. Nothing here knows about XML,
//! HTML, UIKit or GTK.

use crate::diag::Origin;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub file: String,
    pub components: Vec<ComponentInfo>,
    /// The expanded tree of the main component (the last exported one), with
    /// local components inlined; `None` when the file declares no component.
    pub root: Option<Node>,
}

/// A component's interface, kept even after its body is inlined so backends
/// can name outlets, actions and template properties after it.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentInfo {
    pub name: String,
    pub exported: bool,
    pub base: String,
    pub properties: Vec<PropertyDecl>,
    pub callbacks: Vec<CallbackDecl>,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDecl {
    pub name: String,
    pub ty: String,
    /// `in`, `out`, `in-out` or `private`, as written.
    pub visibility: Option<String>,
    pub default: Option<Value>,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallbackDecl {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Option<String>,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The base type as written: `Text`, `HorizontalLayout`, `Button`, or an
    /// unknown name. For an inlined component it is the component's base.
    pub widget: String,
    pub id: Option<String>,
    /// The local component this node was inlined from.
    pub component: Option<String>,
    pub layout: Option<Layout>,
    pub item: LayoutItem,
    /// Properties that are not part of the layout model, in source order.
    pub properties: Vec<Property>,
    pub handlers: Vec<Handler>,
    /// Properties and callbacks declared on this element itself.
    pub declarations: Vec<PropertyDecl>,
    pub structure: Option<Structure>,
    /// Constructs that were read but not converted, for comment stubs.
    pub unsupported: Vec<Unsupported>,
    pub children: Vec<Node>,
    pub origin: Origin,
}

impl Node {
    pub fn property(&self, name: &str) -> Option<&Value> {
        self.properties
            .iter()
            .find(|p| p.name == name)
            .map(|p| &p.value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: Value,
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    Px,
    Phx,
    Cm,
    Mm,
    In,
    Pt,
    Rem,
    Percent,
    Ms,
    S,
    Deg,
    Grad,
    Rad,
    Turn,
}

impl Unit {
    pub fn parse(suffix: &str) -> Option<Unit> {
        Some(match suffix.to_ascii_lowercase().as_str() {
            "px" => Unit::Px,
            "phx" => Unit::Phx,
            "cm" => Unit::Cm,
            "mm" => Unit::Mm,
            "in" => Unit::In,
            "pt" => Unit::Pt,
            "rem" => Unit::Rem,
            "%" => Unit::Percent,
            "ms" => Unit::Ms,
            "s" => Unit::S,
            "deg" => Unit::Deg,
            "grad" => Unit::Grad,
            "rad" => Unit::Rad,
            "turn" => Unit::Turn,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Unit::Px => "px",
            Unit::Phx => "phx",
            Unit::Cm => "cm",
            Unit::Mm => "mm",
            Unit::In => "in",
            Unit::Pt => "pt",
            Unit::Rem => "rem",
            Unit::Percent => "%",
            Unit::Ms => "ms",
            Unit::S => "s",
            Unit::Deg => "deg",
            Unit::Grad => "grad",
            Unit::Rad => "rad",
            Unit::Turn => "turn",
        }
    }

    /// Logical pixels per unit for the absolute length units; `None` for units
    /// that need context (`rem`, `%`) or are not lengths.
    pub fn px_per_unit(self) -> Option<f64> {
        match self {
            Unit::Px | Unit::Phx => Some(1.0),
            Unit::In => Some(96.0),
            Unit::Cm => Some(96.0 / 2.54),
            Unit::Mm => Some(96.0 / 25.4),
            Unit::Pt => Some(96.0 / 72.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Measure {
        value: f64,
        unit: Unit,
    },
    String {
        text: String,
        /// Written as `@tr(...)`.
        translatable: bool,
    },
    Bool(bool),
    /// `#rgb`, `#rrggbbaa` and friends, or a `Colors.name`.
    Color(String),
    /// `TextHorizontalAlignment.center`, or a bare `center` for a property
    /// whose type is a known enum.
    Enum(String),
    /// `@image-url("...")`.
    Url(String),
    /// An array of plain values, as for a `ComboBox` model.
    Array(Vec<Value>),
    /// Anything that needs evaluating; kept as text, never run.
    Binding(Binding),
    /// `name <=> target`.
    TwoWay(Binding),
}

impl Value {
    /// Pixels of an absolute length, the only kind every target can express.
    pub fn as_px(&self) -> Option<f64> {
        match self {
            Value::Measure { value, unit } => unit.px_per_unit().map(|k| value * k),
            Value::Number(n) if *n == 0.0 => Some(0.0),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::String { text, .. } => Some(text),
            _ => None,
        }
    }
}

/// An expression as normalised source text plus the names it reads, so a
/// backend can connect it to an outlet or a template property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub expr: String,
    /// `name` and `name.field` paths read by the expression, sorted.
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerKind {
    /// `clicked => { ... }`
    Callback,
    /// `changed text => { ... }`
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handler {
    pub kind: HandlerKind,
    pub name: String,
    pub params: Vec<String>,
    pub body: String,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Structure {
    /// `for item[index] in model: ...`; the node is the template, emitted once.
    Repeat {
        item: Option<String>,
        index: Option<String>,
        model: Binding,
    },
    /// `if condition: ...`
    Condition { condition: Binding },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub what: String,
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    Horizontal,
    Vertical,
    Grid,
}

/// `LayoutAlignment`: how a box layout distributes leftover main-axis space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alignment {
    Stretch,
    Center,
    Start,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Alignment {
    pub fn parse(name: &str) -> Option<Alignment> {
        Some(match name {
            "stretch" => Alignment::Stretch,
            "center" => Alignment::Center,
            "start" => Alignment::Start,
            "end" => Alignment::End,
            "space-between" => Alignment::SpaceBetween,
            "space-around" => Alignment::SpaceAround,
            "space-evenly" => Alignment::SpaceEvenly,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Alignment::Stretch => "stretch",
            Alignment::Center => "center",
            Alignment::Start => "start",
            Alignment::End => "end",
            Alignment::SpaceBetween => "space-between",
            Alignment::SpaceAround => "space-around",
            Alignment::SpaceEvenly => "space-evenly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Padding {
    pub left: Option<Value>,
    pub right: Option<Value>,
    pub top: Option<Value>,
    pub bottom: Option<Value>,
}

/// A layout element's own settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub kind: LayoutKind,
    pub spacing: Option<Value>,
    /// `spacing-horizontal` / `spacing-vertical` of a `GridLayout`.
    pub spacing_horizontal: Option<Value>,
    pub spacing_vertical: Option<Value>,
    pub padding: Padding,
    pub alignment: Option<Alignment>,
}

/// Size constraints along one axis.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Extent {
    pub fixed: Option<Value>,
    pub min: Option<Value>,
    pub max: Option<Value>,
    pub preferred: Option<Value>,
    pub stretch: Option<f64>,
}

impl Extent {
    pub fn is_empty(&self) -> bool {
        *self == Extent::default()
    }
}

/// Grid placement after `Row` and implicit columns are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub row: u32,
    pub col: u32,
    pub rowspan: u32,
    pub colspan: u32,
}

/// How an element sits in its parent: its size constraints, stretch, absolute
/// position (outside layouts) and grid cell.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutItem {
    pub width: Extent,
    pub height: Extent,
    pub x: Option<Value>,
    pub y: Option<Value>,
    pub cell: Option<Cell>,
}
