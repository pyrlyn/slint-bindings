//! Lists the test fixtures at build time, so the test that keeps the fixture
//! lists complete also runs on wasm32, which has no filesystem at test time.
//! Nothing here touches the library itself.

use std::path::Path;

fn names(dir: &Path) -> String {
    // A copy of the crate without its tests lists nothing rather than failing.
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".slint")
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    names.join(" ")
}

fn main() {
    let dir = Path::new("tests/fixtures");
    println!("cargo::rerun-if-changed={}", dir.display());
    println!("cargo::rustc-env=SLINTX_SYNTAX_FIXTURES={}", names(dir));
    let errors = dir.join("errors");
    println!(
        "cargo::rustc-env=SLINTX_SYNTAX_ERROR_FIXTURES={}",
        names(&errors)
    );
}
