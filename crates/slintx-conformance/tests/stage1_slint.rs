//! Stage 1: Slint's verdict on every corpus case must equal its `.expected`
//! file. A failure means either the corpus changed (re-record with
//! `SLINTX_BLESS=1 cargo nextest run -p slintx-conformance`, then review the
//! diff) or Slint's behaviour drifted under the pin.
//!
//! Slint is run, never read: its output is the only thing recorded here.

#![cfg(not(target_arch = "wasm32"))]

use std::error::Error;
use std::fs;

use slint_interpreter::{Compiler, DiagnosticLevel};
use slintx_conformance::{Case, Diagnostic, Expectation, Level, cases, corpus_root, relative};

fn slint_verdict(case: &Case) -> Expectation {
    let mut compiler = Compiler::default();
    // Pinned so std-widgets cases do not depend on the machine's default style.
    compiler.set_style("fluent".into());
    let result = pollster::block_on(compiler.build_from_path(&case.entry));
    let dir_prefix = format!("{}{}", case.dir().display(), std::path::MAIN_SEPARATOR);
    let diagnostics = result
        .diagnostics()
        .map(|d| {
            let (line, column) = d.line_column();
            Diagnostic {
                level: match d.level() {
                    DiagnosticLevel::Error => Level::Error,
                    _ => Level::Warning,
                },
                file: d
                    .source_file()
                    .map(|f| relative(case.dir(), f))
                    .unwrap_or_default(),
                line,
                column,
                // Some messages quote absolute paths, which differ per checkout.
                message: d.message().replace(&dir_prefix, ""),
            }
        })
        .collect();
    Expectation {
        accepted: !result.has_errors(),
        diagnostics,
    }
}

#[test]
fn slint_verdicts_match_the_recorded_expectations() -> Result<(), Box<dyn Error>> {
    let bless = std::env::var_os("SLINTX_BLESS").is_some();
    let mut drift = Vec::new();
    let all = cases(&corpus_root())?;
    for case in &all {
        let actual = slint_verdict(case).render();
        let recorded = fs::read_to_string(&case.expected).ok();
        if recorded.as_deref() == Some(actual.as_str()) {
            continue;
        }
        if bless {
            fs::write(&case.expected, &actual)?;
        } else {
            drift.push(format!(
                "{}\n--- recorded\n{}--- slint\n{actual}",
                case.name,
                recorded.as_deref().unwrap_or("(missing)\n")
            ));
        }
    }
    assert!(
        drift.is_empty(),
        "{} of {} cases drifted (SLINTX_BLESS=1 re-records):\n\n{}",
        drift.len(),
        all.len(),
        drift.join("\n")
    );
    Ok(())
}

#[test]
fn every_expectation_file_belongs_to_a_case() -> Result<(), Box<dyn Error>> {
    let all = cases(&corpus_root())?;
    let mut stray = Vec::new();
    let mut dirs = vec![corpus_root()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "expected")
                && all.iter().all(|c| c.expected != path)
            {
                stray.push(relative(&corpus_root(), &path));
            }
        }
    }
    assert!(stray.is_empty(), "expectations without a case: {stray:?}");
    Ok(())
}
