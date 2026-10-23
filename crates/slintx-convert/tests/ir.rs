//! The IR the lowering builds from real `slintx-syntax` trees: grid cells,
//! stretch, inlined components, bindings, handlers and source positions. The
//! slintx-conformance corpus checks that no input makes a backend fail.

mod common;

use common::source;
use slintx_convert::ir::*;
use slintx_convert::{ConvertError, Format, Options, Platform, convert, mapping, to_ir};

fn root(name: &str) -> Node {
    match to_ir(&format!("{name}.slint"), &source(name)) {
        Ok((
            Document {
                root: Some(root), ..
            },
            _,
        )) => root,
        Ok(_) => unreachable!("{name}: no root"),
        Err(e) => unreachable!("{name}: {e}"),
    }
}

/// Every node in document order.
fn nodes(n: &Node) -> Vec<&Node> {
    let mut out = vec![n];
    for c in &n.children {
        out.extend(nodes(c));
    }
    out
}

fn text(n: &Node) -> Option<&str> {
    n.property("text").and_then(Value::as_text)
}

fn by_text<'a>(root: &'a Node, wanted: &str) -> &'a Node {
    nodes(root)
        .into_iter()
        .find(|n| text(n) == Some(wanted))
        .unwrap_or_else(|| unreachable!("no element with text {wanted:?}"))
}

fn px(v: f64) -> Option<Value> {
    Some(Value::Measure {
        value: v,
        unit: Unit::Px,
    })
}

#[test]
fn grid_rows_and_explicit_cells_resolve() {
    let root = root("nested_layouts");
    let grid = nodes(&root)
        .into_iter()
        .find(|n| n.widget == "GridLayout")
        .unwrap_or_else(|| unreachable!("no grid"));
    let cells: Vec<_> = grid.children.iter().filter_map(|c| c.item.cell).collect();
    let cell = |row, col, colspan| Cell {
        row,
        col,
        rowspan: 1,
        colspan,
    };
    assert_eq!(
        cells,
        [
            cell(0, 0, 1),
            cell(0, 1, 1),
            cell(1, 0, 1),
            cell(1, 1, 2),
            cell(3, 1, 1)
        ]
    );
    let layout = grid.layout.as_ref().unwrap_or_else(|| unreachable!());
    assert_eq!(layout.kind, LayoutKind::Grid);
    assert_eq!(layout.spacing_horizontal, px(6.0));
    assert_eq!(layout.spacing_vertical, px(2.0));
}

#[test]
fn box_layout_settings_and_alignment() {
    let root = root("nested_layouts");
    let outer = &root.children[0];
    let layout = outer.layout.as_ref().unwrap_or_else(|| unreachable!());
    assert_eq!(layout.kind, LayoutKind::Vertical);
    assert_eq!(layout.spacing, px(8.0));
    assert_eq!(layout.padding.left, px(12.0));
    assert_eq!(layout.padding.bottom, px(12.0));
    let header = &outer.children[0];
    assert_eq!(header.id.as_deref(), Some("header"));
    assert_eq!(
        header.layout.as_ref().and_then(|l| l.alignment),
        Some(Alignment::SpaceBetween)
    );
}

#[test]
fn stretch_and_size_limits_are_layout_items() {
    let root = root("stretch");
    let row = &root.children[0].children[0];
    let stretches: Vec<f64> = row
        .children
        .iter()
        .map(|c| mapping::stretch(c, true))
        .collect();
    assert_eq!(stretches, [1.0, 2.0, 0.0]);
    assert_eq!(row.children[2].item.width.min, px(40.0));
    let limited = &root.children[0].children[1].children[0];
    assert_eq!(limited.item.width.preferred, px(50.0));
    assert_eq!(limited.item.width.max, px(80.0));
    // Layout settings never stay behind as plain properties.
    assert!(limited.property("max-width").is_none());
    let placed = &root.children[0].children[2].children[0];
    assert_eq!(placed.item.x, px(10.0));
    assert_eq!(placed.item.y, px(4.0));
}

#[test]
fn widget_defaults_decide_stretch() {
    let root = root("widgets");
    let button = nodes(&root)
        .into_iter()
        .find(|n| n.widget == "Button")
        .unwrap_or_else(|| unreachable!());
    assert_eq!(mapping::stretch(button, true), 0.0);
    let edit = nodes(&root)
        .into_iter()
        .find(|n| n.widget == "LineEdit")
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        (mapping::stretch(edit, true), mapping::stretch(edit, false)),
        (1.0, 0.0)
    );
}

#[test]
fn local_components_are_inlined_with_their_interface() {
    let root = root("components");
    let cards: Vec<&Node> = nodes(&root)
        .into_iter()
        .filter(|n| n.component.as_deref() == Some("Card"))
        .collect();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].widget, "Rectangle");
    // `root.title` became the use-site value, `root.gap` the default or the
    // override.
    assert!(text(&cards[0].children[0].children[0]) == Some("First"));
    assert_eq!(
        cards[0].children[0]
            .layout
            .as_ref()
            .and_then(|l| l.spacing.clone()),
        px(4.0)
    );
    assert_eq!(cards[1].id.as_deref(), Some("second"));
    assert_eq!(
        cards[1].children[0]
            .layout
            .as_ref()
            .and_then(|l| l.spacing.clone()),
        px(8.0)
    );
    // `@children` lands inside the card's layout.
    let badge = by_text(&root, "new");
    assert_eq!(badge.component.as_deref(), Some("Badge"));
    assert_eq!(
        badge.property("color"),
        Some(&Value::Color("white".to_owned()))
    );
    assert_eq!(text(&cards[1].children[0].children[1]), Some("Body"));
}

#[test]
fn bindings_keep_text_and_references() {
    let root = root("bindings");
    let label = nodes(&root)
        .into_iter()
        .find(|n| n.id.as_deref() == Some("label"))
        .unwrap_or_else(|| unreachable!());
    let Some(Value::Binding(b)) = label.property("color") else {
        unreachable!("color is not a binding")
    };
    assert_eq!(b.expr, "root.counter > 3 ? red : black");
    assert!(b.references.contains(&"root.counter".to_owned()));
    let input = nodes(&root)
        .into_iter()
        .find(|n| n.id.as_deref() == Some("input"))
        .unwrap_or_else(|| unreachable!());
    let Some(Value::TwoWay(t)) = input.property("text") else {
        unreachable!("text is not two-way")
    };
    assert_eq!(t.expr, "root.name");
}

#[test]
fn handlers_keep_names_parameters_bodies_and_positions() {
    let root = root("bindings");
    let input = nodes(&root)
        .into_iter()
        .find(|n| n.id.as_deref() == Some("input"))
        .unwrap_or_else(|| unreachable!());
    let edited = &input.handlers[0];
    assert_eq!(edited.kind, HandlerKind::Callback);
    assert_eq!(edited.name, "edited");
    assert_eq!(edited.params, ["value"]);
    assert_eq!(edited.body, "{ root.name = value; }");
    assert_eq!((edited.origin.line, edited.origin.column), (16, 13));
    let echo = nodes(&root)
        .into_iter()
        .find(|n| n.handlers.iter().any(|h| h.kind == HandlerKind::Changed))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(echo.handlers[0].name, "text");
    assert_eq!((echo.origin.line, echo.origin.column), (25, 9));
}

#[test]
fn for_and_if_become_structure() {
    let root = root("repeat_if");
    let layout = &root.children[0];
    let Some(Structure::Repeat { item, index, model }) = &layout.children[0].structure else {
        unreachable!("no repeater")
    };
    assert_eq!(item.as_deref(), Some("name"));
    assert_eq!(index.as_deref(), Some("index"));
    assert_eq!(model.expr, "root.names");
    let Some(Structure::Condition { condition }) = &layout.children[1].structure else {
        unreachable!("no condition")
    };
    assert_eq!(condition.expr, "root.show-footer");
}

#[test]
fn string_escapes_and_translations_are_decoded() {
    let root = root("unicode");
    assert!(
        nodes(&root)
            .iter()
            .any(|n| text(n) == Some("こんにちは世界"))
    );
    by_text(&root, "Quote \" backslash \\ newline\nend \u{263A}");
    let tr = by_text(&root, "Translated <&>");
    assert!(matches!(
        tr.property("text"),
        Some(Value::String {
            translatable: true,
            ..
        })
    ));
}

#[test]
fn unsupported_constructs_are_kept_with_a_warning() {
    let (document, diagnostics) = match to_ir("unsupported.slint", &source("unsupported")) {
        Ok(ir) => ir,
        Err(e) => unreachable!("{e}"),
    };
    let root = document.root.unwrap_or_else(|| unreachable!());
    let notes: Vec<&str> = nodes(&root)
        .iter()
        .flat_map(|n| n.unsupported.iter().map(|u| u.what.as_str()))
        .collect();
    assert!(notes.len() >= 3, "{notes:?}");
    assert!(!diagnostics.is_empty());
    assert!(!diagnostics.has_errors());
}

#[test]
fn parse_errors_stop_before_lowering() {
    let Err(ConvertError::Failed { diagnostics }) = to_ir("broken.slint", &source("broken")) else {
        unreachable!("a broken file must not lower")
    };
    assert!(diagnostics.iter().all(|d| d.origin.file == "broken.slint"));
    assert!(diagnostics.iter().any(|d| d.origin.line == 2));
}

/// Every corpus file that parses converts to every format without an error
/// other than a missing component. Warnings are expected: the corpus covers
/// the whole language.
#[test]
fn conformance_corpus_converts() {
    let corpus = concat!(env!("CARGO_MANIFEST_DIR"), "/../slintx-conformance/corpus");
    let mut files = Vec::new();
    let mut dirs = vec![std::path::PathBuf::from(corpus)];
    while let Some(dir) = dirs.pop() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|e| unreachable!("{}: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "slint") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(files.len() > 100, "corpus not found at {corpus}");
    let mut converted = 0;
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| unreachable!("{e}"));
        let name = path.display().to_string();
        if !slintx_syntax::parse(&text).diagnostics().is_empty() {
            continue;
        }
        for (format, platform) in [
            (Format::Xml, Platform::UiKit),
            (Format::Html, Platform::UiKit),
            (Format::Storyboard, Platform::UiKit),
            (Format::Storyboard, Platform::AppKit),
            (Format::Blueprint, Platform::UiKit),
        ] {
            let options = Options {
                platform,
                ..Options::default()
            };
            if let Err(ConvertError::Failed { diagnostics }) =
                convert(&name, &text, format, &options)
            {
                let real: Vec<_> = diagnostics
                    .iter()
                    .filter(|d| d.severity == slintx_convert::Severity::Error)
                    .filter(|d| !d.message.contains("no component"))
                    .collect();
                assert!(real.is_empty(), "{name} {format:?}: {real:?}");
            }
        }
        converted += 1;
    }
    assert!(converted > 100, "only {converted} corpus files converted");
}
