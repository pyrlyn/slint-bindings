//! Stage 3: slintx's module loader finds the import and export errors Slint
//! does, and no others. Reads only the recorded `.expected` files.

#![cfg(not(target_arch = "wasm32"))]

use std::error::Error;

use slintx_conformance::{
    Case, Diagnostic, Level, Loader, cases, check_modules, corpus_root, relative,
};
use slintx_modules::{Config, DiskFs};

struct Slintx;

impl Loader for Slintx {
    fn errors(&self, case: &Case) -> Vec<Diagnostic> {
        let graph = slintx_modules::load(&DiskFs, &Config::default(), &case.entry);
        graph
            .diagnostics
            .iter()
            .filter(|d| d.level == slintx_modules::Level::Error)
            .map(|d| {
                let source = graph
                    .modules
                    .iter()
                    .find(|m| m.path == d.file)
                    .map(|m| m.parse.syntax().to_string())
                    .unwrap_or_default();
                let (line, column) = line_column(&source, d.range.start().into());
                Diagnostic {
                    level: Level::Error,
                    file: relative(case.dir(), &d.file),
                    line,
                    column,
                    message: d.message.clone(),
                }
            })
            .collect()
    }
}

/// 1-based, counting characters, as Slint reports positions.
fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let before = source.get(..offset).unwrap_or(source);
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (
        before.matches('\n').count() + 1,
        before[line_start..].chars().count() + 1,
    )
}

#[test]
fn slintx_finds_the_module_errors_slint_does() -> Result<(), Box<dyn Error>> {
    let all = cases(&corpus_root())?;
    let report = check_modules(&Slintx, &all)?;
    assert!(report.checked > 0);
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    Ok(())
}

/// Reports an error on every case, so the stage is shown to fail on extra
/// errors rather than pass by checking nothing.
struct Noisy;

impl Loader for Noisy {
    fn errors(&self, _: &Case) -> Vec<Diagnostic> {
        vec![Diagnostic {
            level: Level::Error,
            file: "nowhere.slint".into(),
            line: 1,
            column: 1,
            message: String::new(),
        }]
    }
}

/// Reports nothing: accepted everywhere except under `imports/`, where
/// Slint's errors must all be found.
struct Silent;

impl Loader for Silent {
    fn errors(&self, _: &Case) -> Vec<Diagnostic> {
        Vec::new()
    }
}

#[test]
fn the_stage_fails_on_extra_and_on_missing_errors() -> Result<(), Box<dyn Error>> {
    let all = cases(&corpus_root())?;
    assert_eq!(check_modules(&Noisy, &all)?.failures.len(), all.len());
    let silent = check_modules(&Silent, &all)?;
    assert!(!silent.failures.is_empty());
    assert!(silent.failures.iter().all(|f| f.starts_with("imports/")));
    Ok(())
}
