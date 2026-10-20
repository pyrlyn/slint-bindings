//! Stage 2: slintx parses every case Slint accepted. Reads only the recorded
//! `.expected` files, never Slint itself.
//!
//! When `slintx-syntax` lands, implement `Frontend` for it here and pass it to
//! `check_parse` in place of `NoFrontend`.

#![cfg(not(target_arch = "wasm32"))]

use std::error::Error;

use slintx_conformance::{Frontend, NoFrontend, Parsed, cases, check_parse, corpus_root};

#[test]
fn slintx_parses_every_case_slint_accepts() -> Result<(), Box<dyn Error>> {
    let report = check_parse(&NoFrontend, &cases(&corpus_root())?)?;
    if report.skipped {
        eprintln!("stage 2 skipped: no slintx front end yet");
    }
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    Ok(())
}

/// Prints the source back, so the stage's own plumbing is tested before slintx exists.
struct Echo;

impl Frontend for Echo {
    fn parse(&self, source: &str) -> Option<Parsed> {
        Some(Parsed {
            errors: Vec::new(),
            text: source.to_owned(),
        })
    }
}

/// Loses the last byte and reports an error on every file.
struct Lossy;

impl Frontend for Lossy {
    fn parse(&self, source: &str) -> Option<Parsed> {
        let mut text = source.to_owned();
        text.pop();
        Some(Parsed {
            errors: std::iter::once(0..1).collect(),
            text,
        })
    }
}

#[test]
fn a_faithful_front_end_passes_and_a_lossy_one_fails() -> Result<(), Box<dyn Error>> {
    let all = cases(&corpus_root())?;
    let echo = check_parse(&Echo, &all)?;
    assert!(!echo.skipped);
    assert!(echo.checked > 0);
    assert!(echo.failures.is_empty(), "{:#?}", echo.failures);

    let lossy = check_parse(&Lossy, &all)?;
    assert_eq!(lossy.checked, echo.checked);
    assert_eq!(lossy.failures.len(), 2 * echo.checked);
    Ok(())
}
