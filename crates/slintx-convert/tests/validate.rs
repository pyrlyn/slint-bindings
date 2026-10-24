//! Each output checked by something other than this crate: XML and the
//! polyglot HTML by an XML parser, storyboards by Xcode's `ibtool`, Blueprint
//! by `blueprint-compiler`. The external tools are not on every machine, so
//! those tests are ignored by default. CI runs them with `--run-ignored`:
//! `just convert-apple` on macOS, the `tests/platform` image on Linux.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{source, valid};
use quick_xml::Reader;
use quick_xml::events::Event;
use slintx_convert::{Format, Options, Platform, convert};

fn output(name: &str, format: Format, platform: Platform) -> String {
    let options = Options {
        platform,
        ..Options::default()
    };
    match convert(&format!("{name}.slint"), &source(name), format, &options) {
        Ok(c) => c.output,
        Err(e) => unreachable!("{name}: {e}"),
    }
}

fn assert_well_formed(what: &str, text: &str) {
    let mut reader = Reader::from_str(text);
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth -= 1,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => unreachable!(
                "{what}: not well-formed at {}: {e}",
                reader.buffer_position()
            ),
        }
    }
    assert_eq!(depth, 0, "{what}: unclosed elements");
}

#[test]
fn xml_html_and_storyboards_are_well_formed() {
    for name in valid() {
        for (label, format, platform) in [
            ("xml", Format::Xml, Platform::UiKit),
            ("html", Format::Html, Platform::UiKit),
            ("uikit", Format::Storyboard, Platform::UiKit),
            ("appkit", Format::Storyboard, Platform::AppKit),
        ] {
            assert_well_formed(&format!("{name} {label}"), &output(name, format, platform));
        }
    }
}

/// A fresh directory under Cargo's per-test temporary root.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| unreachable!("{}: {e}", dir.display()));
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap_or_else(|e| unreachable!("{}: {e}", path.display()));
}

fn run(command: &mut Command) -> String {
    let out = command
        .output()
        .unwrap_or_else(|e| unreachable!("{command:?}: {e}"));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{command:?} failed:\n{text}");
    text
}

const PLATFORMS: [(&str, Platform); 2] = [("uikit", Platform::UiKit), ("appkit", Platform::AppKit)];

#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs Xcode's ibtool"]
fn storyboards_compile_with_ibtool() {
    let dir = scratch("ibtool-compile");
    for name in valid() {
        for (label, platform) in PLATFORMS {
            let input = dir.join(format!("{name}-{label}.storyboard"));
            let compiled = dir.join(format!("{name}-{label}.storyboardc"));
            write(&input, &output(name, Format::Storyboard, platform));
            let report = run(Command::new("xcrun")
                .args([
                    "ibtool",
                    "--errors",
                    "--warnings",
                    "--output-format",
                    "human-readable-text",
                    "--compile",
                ])
                .arg(&compiled)
                .arg(&input));
            let problems: Vec<&str> = report
                .lines()
                .filter(|l| l.contains(": error:") || l.contains(": warning:"))
                .collect();
            assert!(
                problems.is_empty(),
                "{name} {label}:\n{}",
                problems.join("\n")
            );
            assert!(
                compiled.is_dir(),
                "{name} {label}: no {}",
                compiled.display()
            );
        }
    }
}

/// Attributes Interface Builder leaves out when it saves, because they hold
/// its default (or, for stack margins, because it derives them).
const IB_DEFAULTS: &[(&str, &str)] = &[
    ("axis", "horizontal"),
    ("distribution", "fill"),
    ("doubleValue", "0"),
    ("editable", "NO"),
    ("focusRingType", "none"),
    ("horizontalHuggingPriority", "250"),
    ("increment", "1"),
    ("layoutMarginsRelativeArrangement", "YES"),
    ("lineBreakMode", "wordWrapping"),
    ("metaFont", "menu"),
    ("minValue", "0"),
    ("on", "NO"),
    ("progress", "0"),
    ("progressViewStyle", "default"),
    ("spacing", "0"),
    // NSStackView's default; UIStackView's is 0.
    ("spacing", "8"),
    ("state", "off"),
    ("stepValue", "1"),
    ("title", ""),
    ("verticalHuggingPriority", "250"),
];

/// Every element with an `id`, with its attributes.
fn attributes_by_id(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut reader = Reader::from_str(text);
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let attrs: Vec<(String, String)> = e
                    .attributes()
                    .filter_map(Result::ok)
                    .map(|a| {
                        let value = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map(|v| v.into_owned())
                            .unwrap_or_default();
                        let key: &str = a.key.as_ref();
                        (key.to_owned(), value)
                    })
                    .collect();
                if let Some((_, id)) = attrs.iter().find(|(k, _)| k == "id") {
                    out.push((id.clone(), attrs.clone()));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => unreachable!("{e}"),
        }
    }
    out
}

/// Interface Builder drops attributes it does not know when it re-saves a
/// document, so a round trip through it finds misspelt or misplaced ones,
/// which compiling alone silently ignores.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs Xcode's ibtool"]
fn interface_builder_keeps_every_attribute() {
    let dir = scratch("ibtool-round-trip");
    let mut lost = Vec::new();
    for name in valid() {
        for (label, platform) in PLATFORMS {
            let input = dir.join(format!("{name}-{label}.storyboard"));
            let saved = dir.join(format!("{name}-{label}.saved.storyboard"));
            let text = output(name, Format::Storyboard, platform);
            write(&input, &text);
            run(Command::new("xcrun")
                .args(["ibtool", "--upgrade"])
                .arg(&input)
                .arg("--write")
                .arg(&saved));
            let saved_text = std::fs::read_to_string(&saved)
                .unwrap_or_else(|e| unreachable!("{}: {e}", saved.display()));
            let after = attributes_by_id(&saved_text);
            for (id, attrs) in attributes_by_id(&text) {
                let Some((_, kept)) = after.iter().find(|(i, _)| *i == id) else {
                    lost.push(format!("{name} {label}: element {id}"));
                    continue;
                };
                for (key, value) in attrs {
                    let now = kept
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, v)| v.as_str());
                    let same = match now {
                        Some(v) => {
                            v == value
                                || v.parse::<f64>()
                                    .ok()
                                    .zip(value.parse::<f64>().ok())
                                    .is_some_and(|(a, b)| (a - b).abs() < 1e-6)
                        }
                        // A constraint's first item is implied when it is the
                        // view that owns the constraint.
                        None => {
                            key == "firstItem"
                                || IB_DEFAULTS.contains(&(key.as_str(), value.as_str()))
                        }
                    };
                    if !same {
                        lost.push(format!(
                            "{name} {label}: {id} {key}={value:?} became {now:?}"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        lost.is_empty(),
        "Interface Builder changed:\n{}",
        lost.join("\n")
    );
}

#[test]
#[ignore = "needs blueprint-compiler and the GTK 4 typelibs"]
fn blueprints_compile() {
    let dir = scratch("blueprint");
    for name in valid() {
        let input = dir.join(format!("{name}.blp"));
        let compiled = dir.join(format!("{name}.ui"));
        write(&input, &output(name, Format::Blueprint, Platform::UiKit));
        run(Command::new("blueprint-compiler")
            .arg("compile")
            .arg("--output")
            .arg(&compiled)
            .arg(&input));
        let ui = std::fs::read_to_string(&compiled)
            .unwrap_or_else(|e| unreachable!("{}: {e}", compiled.display()));
        assert_well_formed(&format!("{name} ui"), &ui);
    }
}
