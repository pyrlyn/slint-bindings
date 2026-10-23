//! The widget correspondence table: which element each Slint widget becomes in
//! every target, plus the layout defaults the converter needs. Kept as plain
//! data so `docs/mapping.md` is generated from it and backends share one source.

use crate::ir::{LayoutKind, Node};

/// What a widget is, independent of any target; backends pick properties and
/// events by role, not by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Window,
    /// A plain box: `Rectangle`, `TouchArea`, `FocusScope` and friends.
    Container,
    Layout(LayoutKind),
    /// `Row` inside a `GridLayout`; dissolved into cells during lowering.
    GridRow,
    Text,
    Image,
    Button,
    CheckBox,
    Switch,
    LineEdit,
    TextEdit,
    Slider,
    Progress,
    SpinBox,
    ComboBox,
    Scroll,
    Spinner,
    GroupBox,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Widget {
    pub slint: &'static str,
    pub role: Role,
    /// Default `horizontal-stretch` and `vertical-stretch`.
    pub stretch: (f64, f64),
    /// Without a layout parent and an explicit size, Slint makes the element
    /// as large as its parent; otherwise it keeps its preferred size.
    pub fills_parent: bool,
    /// The property holding the visible text, if any.
    pub text: Option<&'static str>,
    pub html: &'static str,
    pub uikit: Option<&'static str>,
    pub appkit: Option<&'static str>,
    pub gtk: Option<&'static str>,
}

const fn widget(
    slint: &'static str,
    role: Role,
    stretch: (f64, f64),
    fills_parent: bool,
    text: Option<&'static str>,
    targets: [&'static str; 4],
) -> Widget {
    let [html, uikit, appkit, gtk] = targets;
    Widget {
        slint,
        role,
        stretch,
        fills_parent,
        text,
        html,
        uikit: non_empty(uikit),
        appkit: non_empty(appkit),
        gtk: non_empty(gtk),
    }
}

const fn non_empty(s: &'static str) -> Option<&'static str> {
    if s.is_empty() { None } else { Some(s) }
}

use LayoutKind::{Grid, Horizontal, Vertical};
use Role::*;

/// Every widget the converter knows: Slint's builtin elements and the
/// `std-widgets.slint` controls. `targets` is `[html, uikit, appkit, gtk]`; an
/// empty name means the target has no counterpart.
pub const WIDGETS: &[Widget] = &[
    widget(
        "Window",
        Window,
        (1.0, 1.0),
        true,
        None,
        ["main", "UIView", "NSView", "Gtk.Window"],
    ),
    widget(
        "Rectangle",
        Container,
        (1.0, 1.0),
        true,
        None,
        ["div", "UIView", "NSView", "Gtk.Box"],
    ),
    widget(
        "Empty",
        Container,
        (1.0, 1.0),
        true,
        None,
        ["div", "UIView", "NSView", "Gtk.Box"],
    ),
    widget(
        "TouchArea",
        Container,
        (1.0, 1.0),
        true,
        None,
        ["div", "UIView", "NSView", "Gtk.Box"],
    ),
    widget(
        "FocusScope",
        Container,
        (1.0, 1.0),
        true,
        None,
        ["div", "UIView", "NSView", "Gtk.Box"],
    ),
    widget(
        "Flickable",
        Scroll,
        (1.0, 1.0),
        true,
        None,
        ["div", "UIScrollView", "NSView", "Gtk.ScrolledWindow"],
    ),
    widget(
        "ScrollView",
        Scroll,
        (1.0, 1.0),
        true,
        None,
        ["div", "UIScrollView", "NSView", "Gtk.ScrolledWindow"],
    ),
    widget(
        "HorizontalLayout",
        Layout(Horizontal),
        (1.0, 1.0),
        true,
        None,
        ["div", "UIStackView", "NSStackView", "Gtk.Box"],
    ),
    widget(
        "VerticalLayout",
        Layout(Vertical),
        (1.0, 1.0),
        true,
        None,
        ["div", "UIStackView", "NSStackView", "Gtk.Box"],
    ),
    widget(
        "GridLayout",
        Layout(Grid),
        (1.0, 1.0),
        true,
        None,
        ["div", "UIStackView", "NSStackView", "Gtk.Grid"],
    ),
    widget(
        "HorizontalBox",
        Layout(Horizontal),
        (1.0, 1.0),
        true,
        None,
        ["div", "UIStackView", "NSStackView", "Gtk.Box"],
    ),
    widget(
        "VerticalBox",
        Layout(Vertical),
        (1.0, 1.0),
        true,
        None,
        ["div", "UIStackView", "NSStackView", "Gtk.Box"],
    ),
    widget(
        "GridBox",
        Layout(Grid),
        (1.0, 1.0),
        true,
        None,
        ["div", "UIStackView", "NSStackView", "Gtk.Grid"],
    ),
    widget("Row", GridRow, (1.0, 1.0), false, None, ["", "", "", ""]),
    widget(
        "Text",
        Text,
        (0.0, 0.0),
        false,
        Some("text"),
        ["span", "UILabel", "NSTextField", "Gtk.Label"],
    ),
    widget(
        "Image",
        Image,
        (0.0, 0.0),
        false,
        None,
        ["img", "UIImageView", "NSImageView", "Gtk.Picture"],
    ),
    widget(
        "TextInput",
        LineEdit,
        (1.0, 0.0),
        false,
        Some("text"),
        ["input", "UITextField", "NSTextField", "Gtk.Entry"],
    ),
    widget(
        "Button",
        Button,
        (0.0, 0.0),
        false,
        Some("text"),
        ["button", "UIButton", "NSButton", "Gtk.Button"],
    ),
    widget(
        "CheckBox",
        CheckBox,
        (0.0, 0.0),
        false,
        Some("text"),
        ["input", "UISwitch", "NSButton", "Gtk.CheckButton"],
    ),
    widget(
        "Switch",
        Switch,
        (0.0, 0.0),
        false,
        Some("text"),
        ["input", "UISwitch", "NSSwitch", "Gtk.Switch"],
    ),
    widget(
        "LineEdit",
        LineEdit,
        (1.0, 0.0),
        false,
        Some("text"),
        ["input", "UITextField", "NSTextField", "Gtk.Entry"],
    ),
    widget(
        "TextEdit",
        TextEdit,
        (1.0, 1.0),
        false,
        Some("text"),
        ["textarea", "UITextView", "NSTextField", "Gtk.TextView"],
    ),
    widget(
        "Slider",
        Slider,
        (1.0, 0.0),
        false,
        None,
        ["input", "UISlider", "NSSlider", "Gtk.Scale"],
    ),
    widget(
        "ProgressIndicator",
        Progress,
        (1.0, 0.0),
        false,
        None,
        [
            "progress",
            "UIProgressView",
            "NSProgressIndicator",
            "Gtk.ProgressBar",
        ],
    ),
    widget(
        "Spinner",
        Spinner,
        (0.0, 0.0),
        false,
        None,
        [
            "div",
            "UIActivityIndicatorView",
            "NSProgressIndicator",
            "Gtk.Spinner",
        ],
    ),
    widget(
        "SpinBox",
        SpinBox,
        (1.0, 0.0),
        false,
        None,
        ["input", "UIStepper", "NSStepper", "Gtk.SpinButton"],
    ),
    widget(
        "ComboBox",
        ComboBox,
        (1.0, 0.0),
        false,
        None,
        ["select", "UIButton", "NSPopUpButton", "Gtk.DropDown"],
    ),
    widget(
        "GroupBox",
        GroupBox,
        (1.0, 1.0),
        true,
        Some("title"),
        ["fieldset", "UIView", "NSView", "Gtk.Frame"],
    ),
];

pub fn lookup(name: &str) -> Option<&'static Widget> {
    WIDGETS.iter().find(|w| w.slint == name)
}

/// The stretch factor of `node` along one axis: its own setting, else the
/// widget's default, else 1 (Slint's default for elements without one).
pub fn stretch(node: &Node, horizontal: bool) -> f64 {
    let explicit = if horizontal {
        node.item.width.stretch
    } else {
        node.item.height.stretch
    };
    explicit.unwrap_or_else(|| {
        lookup(&node.widget).map_or(1.0, |w| if horizontal { w.stretch.0 } else { w.stretch.1 })
    })
}

/// The `docs/mapping.md` widget table, generated so it cannot drift.
pub fn markdown_table() -> String {
    let mut out = String::from(
        "| Slint | HTML | UIKit | AppKit | GTK 4 | Default stretch (h, v) |\n\
         | --- | --- | --- | --- | --- | --- |\n",
    );
    let cell = |s: Option<&str>| s.map_or_else(|| "—".to_owned(), |s| format!("`{s}`"));
    for w in WIDGETS {
        let html = (!w.html.is_empty()).then_some(w.html);
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {}, {} |\n",
            w.slint,
            cell(html),
            cell(w.uikit),
            cell(w.appkit),
            cell(w.gtk),
            w.stretch.0,
            w.stretch.1,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_names_are_unique() {
        let mut names: Vec<_> = WIDGETS.iter().map(|w| w.slint).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), WIDGETS.len());
    }

    #[test]
    fn docs_mapping_table_is_current() {
        let doc = include_str!("../docs/mapping.md");
        assert!(
            doc.contains(&markdown_table()),
            "docs/mapping.md is stale; paste mapping::markdown_table() into it:\n{}",
            markdown_table()
        );
    }
}
