//! Every fixture through every backend, compared with `insta` snapshots. The
//! warnings are part of the snapshot, so a new or lost warning shows in review.

mod common;

use common::{FIXTURES, source};
use slintx_convert::{ConvertError, Format, Options, Platform, convert};

fn render(name: &str, format: Format, options: &Options) -> String {
    let file = format!("{name}.slint");
    match convert(&file, &source(name), format, options) {
        Ok(c) => {
            let mut out = c.output;
            if !c.warnings.is_empty() {
                out.push_str("\n--- warnings ---\n");
                for w in &c.warnings {
                    out.push_str(&format!("{w}\n"));
                }
            }
            out
        }
        Err(ConvertError::Failed { diagnostics }) => {
            let mut out = String::from("--- failed ---\n");
            for d in &diagnostics {
                out.push_str(&format!("{d}\n"));
            }
            out
        }
    }
}

fn snapshots(prefix: &str, format: Format, options: &Options) {
    for name in FIXTURES {
        insta::assert_snapshot!(format!("{prefix}__{name}"), render(name, format, options));
    }
}

#[test]
fn xml_output_matches_snapshots() {
    snapshots("xml", Format::Xml, &Options::default());
}

#[test]
fn html_output_matches_snapshots() {
    snapshots("html", Format::Html, &Options::default());
}

#[test]
fn blueprint_output_matches_snapshots() {
    snapshots("blueprint", Format::Blueprint, &Options::default());
}

#[test]
fn uikit_storyboard_output_matches_snapshots() {
    snapshots("uikit", Format::Storyboard, &Options::default());
}

#[test]
fn appkit_storyboard_output_matches_snapshots() {
    let options = Options {
        platform: Platform::AppKit,
        ..Options::default()
    };
    snapshots("appkit", Format::Storyboard, &options);
}

#[test]
fn output_is_deterministic() {
    for format in [
        Format::Xml,
        Format::Html,
        Format::Blueprint,
        Format::Storyboard,
    ] {
        for name in FIXTURES {
            let options = Options::default();
            assert_eq!(
                render(name, format, &options),
                render(name, format, &options)
            );
        }
    }
}

#[test]
fn strict_turns_warnings_into_errors() {
    let options = Options {
        strict: true,
        ..Options::default()
    };
    let result = convert(
        "unsupported.slint",
        &source("unsupported"),
        Format::Xml,
        &options,
    );
    let Err(ConvertError::Failed { diagnostics }) = result else {
        unreachable!("strict conversion of unsupported input must fail")
    };
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity == slintx_convert::Severity::Error)
    );
}
