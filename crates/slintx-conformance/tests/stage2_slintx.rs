//! Stage 2: slintx parses every case Slint accepted. Reads only the recorded
//! `.expected` files, never Slint itself.

#![cfg(not(target_arch = "wasm32"))]

use std::error::Error;

use slintx_conformance::{Frontend, Parsed, cases, check_parse, corpus_root};

struct Slintx;

impl Frontend for Slintx {
    fn parse(&self, source: &str) -> Parsed {
        let parse = slintx_syntax::parse(source);
        Parsed {
            errors: parse
                .diagnostics()
                .iter()
                .map(|d| d.range.start().into()..d.range.end().into())
                .collect(),
            text: parse.syntax().to_string(),
        }
    }
}

#[test]
fn slintx_parses_every_case_slint_accepts() -> Result<(), Box<dyn Error>> {
    let report = check_parse(&Slintx, &cases(&corpus_root())?)?;
    assert!(report.checked > 0);
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    Ok(())
}

/// Prints the source back, so a pass of the stage cannot come from the stage
/// checking nothing.
struct Echo;

impl Frontend for Echo {
    fn parse(&self, source: &str) -> Parsed {
        Parsed {
            errors: Vec::new(),
            text: source.to_owned(),
        }
    }
}

/// Loses the last byte and reports an error on every file.
struct Lossy;

impl Frontend for Lossy {
    fn parse(&self, source: &str) -> Parsed {
        let mut text = source.to_owned();
        text.pop();
        Parsed {
            errors: std::iter::once(0..1).collect(),
            text,
        }
    }
}

#[test]
fn a_faithful_front_end_passes_and_a_lossy_one_fails() -> Result<(), Box<dyn Error>> {
    let all = cases(&corpus_root())?;
    let echo = check_parse(&Echo, &all)?;
    assert!(echo.checked > 0);
    assert!(echo.failures.is_empty(), "{:#?}", echo.failures);

    let lossy = check_parse(&Lossy, &all)?;
    assert_eq!(lossy.checked, echo.checked);
    assert_eq!(lossy.failures.len(), 2 * echo.checked);
    Ok(())
}
