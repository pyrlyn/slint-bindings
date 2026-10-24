//! The loader over an in-memory file system, so these also run on wasm32.
//! The corpus cases Slint judged are checked by slintx-conformance stage 3.

use std::path::{Path, PathBuf};

use slintx_modules::{Config, Graph, Level, MemoryFs, STD_WIDGETS, Symbol, load};
// The wasm32 runner only sees `#[wasm_bindgen_test]`; natively `#[test]` stays.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;

fn memory(files: &[(&str, &str)]) -> MemoryFs {
    let mut fs = MemoryFs::default();
    for (path, source) in files {
        fs.insert(path, *source);
    }
    fs
}

fn errors(graph: &Graph) -> Vec<&str> {
    graph
        .diagnostics
        .iter()
        .filter(|d| d.level == Level::Error)
        .map(|d| d.message.as_str())
        .collect()
}

fn module(graph: &Graph, path: &str) -> Option<usize> {
    let path = Path::new(path);
    graph.modules.iter().position(|m| m.path == path)
}

#[test]
fn imports_resolve_relative_to_the_importer_then_include_paths() {
    let fs = memory(&[
        (
            "app/main.slint",
            r#"import { A } from "ui/a.slint"; import { B } from "b.slint";"#,
        ),
        (
            "app/ui/a.slint",
            r#"import { C } from "../c.slint"; export component A {}"#,
        ),
        ("app/c.slint", "export component C {}"),
        ("shared/b.slint", "export component B {}"),
    ]);
    let config = Config {
        include_paths: vec![PathBuf::from("shared")],
        ..Config::default()
    };
    let graph = load(&fs, &config, Path::new("app/main.slint"));
    assert_eq!(errors(&graph), Vec::<&str>::new());
    let paths: Vec<_> = graph
        .modules
        .iter()
        .map(|m| m.path.to_string_lossy())
        .collect();
    assert_eq!(
        paths,
        [
            "app/main.slint",
            "app/ui/a.slint",
            "app/c.slint",
            "shared/b.slint"
        ]
    );
    let a = Symbol {
        module: 1,
        name: "A".into(),
    };
    assert_eq!(graph.modules[0].imports[0].symbol, a);
}

#[test]
fn library_paths_map_a_file_and_a_directory() {
    let fs = memory(&[
        (
            "main.slint",
            r#"import { W } from "@widgets"; import { I } from "@icons/set/i.slint";"#,
        ),
        ("vendor/widgets.slint", "export component W {}"),
        ("vendor/icons/set/i.slint", "export global I {}"),
    ]);
    let mut config = Config::default();
    config
        .library_paths
        .insert("widgets".into(), "vendor/widgets.slint".into());
    config
        .library_paths
        .insert("icons".into(), "vendor/icons".into());
    let graph = load(&fs, &config, Path::new("main.slint"));
    assert_eq!(errors(&graph), Vec::<&str>::new());
    assert_eq!(graph.modules.len(), 3);

    let unknown = load(
        &memory(&[("main.slint", r#"import { X } from "@nope";"#)]),
        &config,
        Path::new("main.slint"),
    );
    assert_eq!(
        errors(&unknown),
        [r#"Cannot find requested import "@nope" in the include search path"#]
    );
}

#[test]
fn std_widgets_comes_bundled_unless_a_local_file_shadows_it() {
    let source = r#"import { Button, Palette } from "std-widgets.slint";"#;
    let graph = load(
        &memory(&[("main.slint", source)]),
        &Config::default(),
        Path::new("main.slint"),
    );
    assert_eq!(errors(&graph), Vec::<&str>::new());
    assert_eq!(graph.modules[1].path, Path::new(STD_WIDGETS));

    let fs = memory(&[
        ("main.slint", source),
        ("std-widgets.slint", "export component Button {}"),
    ]);
    let shadowed = load(&fs, &Config::default(), Path::new("main.slint"));
    assert_eq!(
        errors(&shadowed),
        [r#"No exported type called 'Palette' found in "std-widgets.slint""#]
    );
}

#[test]
fn a_file_imported_twice_loads_once() {
    let fs = memory(&[
        (
            "main.slint",
            r#"import { A } from "a.slint"; import { B } from "./b.slint";"#,
        ),
        (
            "a.slint",
            r#"import { B } from "b.slint"; export component A {}"#,
        ),
        ("b.slint", "export component B {}"),
    ]);
    let graph = load(&fs, &Config::default(), Path::new("main.slint"));
    assert_eq!(graph.modules.len(), 3);
    let b = module(&graph, "b.slint");
    assert!(
        graph.modules[0]
            .imports
            .iter()
            .any(|n| Some(n.symbol.module) == b)
    );
}

#[test]
fn exports_follow_renames_reexports_and_stars() {
    let fs = memory(&[
        (
            "main.slint",
            r#"import { Card } from "card.slint";
            component Main {}
            export { Main as App, Card }
            export { Badge as Pill } from "card.slint";
            export * from "more.slint";"#,
        ),
        (
            "card.slint",
            "export component Card {} export component Badge {}",
        ),
        (
            "more.slint",
            "export struct Point { x: int } export enum Mode { a }",
        ),
    ]);
    let graph = load(&fs, &Config::default(), Path::new("main.slint"));
    assert_eq!(errors(&graph), Vec::<&str>::new());
    let main = &graph.modules[0];
    let names: Vec<_> = main.exports.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["App", "Card", "Pill", "Point", "Mode"]);
    let card = module(&graph, "card.slint");
    assert!(card.is_some());
    assert_eq!(
        main.export("Pill")
            .map(|s| (Some(s.module), s.name.as_str())),
        Some((card, "Badge"))
    );
    assert_eq!(
        main.export("App"),
        Some(&Symbol {
            module: 0,
            name: "Main".into()
        })
    );
}

#[test]
fn names_match_with_dash_and_underscore_alike() {
    let fs = memory(&[
        (
            "main.slint",
            r#"import { my_card } from "card.slint"; export { my-card as Other }"#,
        ),
        ("card.slint", "export component my-card {}"),
    ]);
    let graph = load(&fs, &Config::default(), Path::new("main.slint"));
    assert_eq!(errors(&graph), Vec::<&str>::new());
    assert!(graph.modules[0].export("Other").is_some());
}

#[test]
fn a_file_without_exports_exports_its_last_component() {
    let fs = memory(&[("main.slint", "component A {} component B {}")]);
    let graph = load(&fs, &Config::default(), Path::new("main.slint"));
    assert!(graph.modules[0].export("B").is_some());
    assert!(graph.modules[0].export("A").is_none());
    assert_eq!(graph.diagnostics[0].level, Level::Warning);
}

#[test]
fn cycles_and_missing_files_are_errors_and_loading_goes_on() {
    let fs = memory(&[
        (
            "main.slint",
            r#"import { A } from "a.slint"; import { X } from "x.slint";"#,
        ),
        (
            "a.slint",
            r#"import { M } from "main.slint"; export component A {}"#,
        ),
    ]);
    let graph = load(&fs, &Config::default(), Path::new("main.slint"));
    assert_eq!(
        errors(&graph),
        [
            r#"Recursive import of "main.slint""#,
            r#"Cannot find requested import "x.slint" in the include search path"#,
        ]
    );
    assert!(graph.modules[0].imports.iter().any(|n| n.name == "A"));
}

#[test]
fn a_missing_entry_is_an_error() {
    let graph = load(
        &MemoryFs::default(),
        &Config::default(),
        Path::new("main.slint"),
    );
    assert!(graph.has_errors());
    assert!(graph.modules.is_empty());
}
