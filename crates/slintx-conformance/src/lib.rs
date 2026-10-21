//! Differential conformance harness for slintx, with Slint as the oracle.
//!
//! The corpus under `corpus/` is written from the Slint 1.18.0 docs, never from
//! Slint's sources. Every case carries a checked-in `.expected` file with the
//! verdict Slint gave for it, so slintx is compared with recorded behaviour and
//! drift in Slint itself shows up as a failing test.
//!
//! Stages, each comparing more of slintx with Slint:
//!
//! | Stage | What is compared | State |
//! | --- | --- | --- |
//! | 1. Verdict | Slint's accept/reject and diagnostic positions against `.expected` | live (`tests/stage1_slint.rs`) |
//! | 2. Parse | Every case Slint accepts parses without syntax errors and round-trips | hook ready, [`Frontend`] stubbed until `slintx-syntax` lands |
//! | 3. Values | Property values after scripted steps | stub: needs the slintx evaluator and runtime |
//! | 4. Geometry | Layout geometry of every element | stub: needs the slintx layout engine |
//! | 5. Pixels | Rendered frames within a pixel budget (`pixel-diff`) | stub: needs `slintx-wgpu` |

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// The file a directory case starts from; its siblings are only imported.
pub const MULTI_FILE_ENTRY: &str = "main.slint";

/// One compilation unit of the corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// Path relative to the corpus root, `/`-separated: the file for a
    /// single-file case, the directory for a multi-file one.
    pub name: String,
    /// The file the compiler starts from.
    pub entry: PathBuf,
    /// Every `.slint` file of the case, entry first.
    pub files: Vec<PathBuf>,
    /// Where the recorded verdict lives.
    pub expected: PathBuf,
}

impl Case {
    /// Directory diagnostics are reported relative to.
    pub fn dir(&self) -> &Path {
        self.entry.parent().unwrap_or(Path::new(""))
    }
}

pub fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

/// All cases under `root`, sorted by name so reports and blessing are stable.
pub fn cases(root: &Path) -> io::Result<Vec<Case>> {
    let mut out = Vec::new();
    collect(root, root, &mut out)?;
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<Case>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort();
    let entry = dir.join(MULTI_FILE_ENTRY);
    if dir != root && entry.is_file() {
        let mut files = vec![entry.clone()];
        files.extend(slint_files(&entries).filter(|p| *p != entry));
        out.push(Case {
            name: relative(root, dir),
            expected: entry.with_extension("expected"),
            entry,
            files,
        });
        return Ok(());
    }
    for path in &entries {
        if path.is_dir() {
            collect(root, path, out)?;
        }
    }
    for path in slint_files(&entries) {
        out.push(Case {
            name: relative(root, &path),
            expected: path.with_extension("expected"),
            files: vec![path.clone()],
            entry: path,
        });
    }
    Ok(())
}

fn slint_files(entries: &[PathBuf]) -> impl Iterator<Item = PathBuf> + '_ {
    entries
        .iter()
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "slint"))
        .cloned()
}

/// `path` relative to `base`, `/`-separated so `.expected` files match on every OS.
pub fn relative(base: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Level,
    /// Relative to the case directory.
    pub file: String,
    pub line: usize,
    pub column: usize,
    /// Slint's wording, kept for readers; slintx is compared on positions only.
    pub message: String,
}

/// A recorded verdict: the content of one `.expected` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectation {
    pub accepted: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl Expectation {
    pub fn render(&self) -> String {
        let verdict = if self.accepted { "accept" } else { "reject" };
        let mut out = format!("verdict: {verdict}\n");
        for d in &self.diagnostics {
            // One diagnostic per line keeps diffs readable when Slint drifts.
            let message = d.message.replace('\\', "\\\\").replace('\n', "\\n");
            let _ = writeln!(
                out,
                "{} {}:{}:{}: {message}",
                d.level.as_str(),
                d.file,
                d.line,
                d.column
            );
        }
        out
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let accepted = match lines.next() {
            Some("verdict: accept") => true,
            Some("verdict: reject") => false,
            other => return Err(format!("bad verdict line: {other:?}")),
        };
        let diagnostics = lines.map(parse_diagnostic).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            accepted,
            diagnostics,
        })
    }
}

fn parse_diagnostic(line: &str) -> Result<Diagnostic, String> {
    let bad = || format!("bad diagnostic line: {line:?}");
    let (level, rest) = line.split_once(' ').ok_or_else(bad)?;
    let level = match level {
        "error" => Level::Error,
        "warning" => Level::Warning,
        _ => return Err(bad()),
    };
    let (position, message) = rest.split_once(": ").ok_or_else(bad)?;
    let mut parts = position.rsplitn(3, ':');
    let column = parts.next().and_then(|c| c.parse().ok()).ok_or_else(bad)?;
    let line_no = parts.next().and_then(|l| l.parse().ok()).ok_or_else(bad)?;
    let file = parts.next().ok_or_else(bad)?.to_owned();
    Ok(Diagnostic {
        level,
        file,
        line: line_no,
        column,
        message: unescape(message),
    })
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// What a slintx front end reports for one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// Byte ranges of syntax errors.
    pub errors: Vec<Range<usize>>,
    /// The syntax tree printed back.
    pub text: String,
}

/// The slintx side of stage 2. `slintx-syntax` implements it once it lands.
pub trait Frontend {
    /// `None` while the front end does not exist, so the stage reports
    /// itself as skipped instead of failing.
    fn parse(&self, source: &str) -> Option<Parsed>;
}

/// Stand-in until `slintx-syntax` (T73.1) is on `main`.
pub struct NoFrontend;

impl Frontend for NoFrontend {
    fn parse(&self, _source: &str) -> Option<Parsed> {
        None
    }
}

/// Stage 2 result over the corpus.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParseReport {
    pub checked: usize,
    pub skipped: bool,
    pub failures: Vec<String>,
}

/// Every file of every case Slint accepted must parse cleanly and round-trip.
/// Rejected cases are not checked: their errors may be syntactic or semantic,
/// and splitting them is the job of later stages.
pub fn check_parse(frontend: &dyn Frontend, cases: &[Case]) -> io::Result<ParseReport> {
    let mut report = ParseReport::default();
    for case in cases {
        let expected = Expectation::parse(&fs::read_to_string(&case.expected)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if !expected.accepted {
            continue;
        }
        for file in &case.files {
            let source = fs::read_to_string(file)?;
            let Some(parsed) = frontend.parse(&source) else {
                report.skipped = true;
                return Ok(report);
            };
            report.checked += 1;
            let name = relative(&corpus_root(), file);
            if !parsed.errors.is_empty() {
                report
                    .failures
                    .push(format!("{name}: syntax errors at {:?}", parsed.errors));
            }
            if parsed.text != source {
                report.failures.push(format!("{name}: does not round-trip"));
            }
        }
    }
    Ok(report)
}

// wasm32-unknown-unknown has no file system to read the corpus from.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn expectation_round_trips_through_text() {
        let expectation = Expectation {
            accepted: false,
            diagnostics: vec![
                Diagnostic {
                    level: Level::Error,
                    file: "sub/a.slint".into(),
                    line: 3,
                    column: 14,
                    message: "two\nlines with a \\ and: colon".into(),
                },
                Diagnostic {
                    level: Level::Warning,
                    file: "main.slint".into(),
                    line: 1,
                    column: 1,
                    message: "w".into(),
                },
            ],
        };
        let text = expectation.render();
        assert_eq!(text.lines().count(), 3);
        assert_eq!(Expectation::parse(&text), Ok(expectation));
    }

    #[test]
    fn malformed_expectations_are_rejected() {
        assert!(Expectation::parse("").is_err());
        assert!(Expectation::parse("verdict: maybe\n").is_err());
        assert!(Expectation::parse("verdict: accept\nnote x:1:1: m\n").is_err());
        assert!(Expectation::parse("verdict: accept\nerror x:one:1: m\n").is_err());
    }

    /// `O<n>` ids in `text`, scanning only lines that start with `prefix`.
    fn open_ids(text: &str, prefix: &str) -> Vec<String> {
        text.lines()
            .filter(|l| l.starts_with(prefix))
            .flat_map(|l| l.split(|c: char| !c.is_ascii_alphanumeric()))
            .filter(|w| {
                w.len() > 1 && w.starts_with('O') && w[1..].bytes().all(|b| b.is_ascii_digit())
            })
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn cases_cover_every_open_item() -> io::Result<()> {
        let listed = open_ids(&fs::read_to_string(corpus_root().join("OPEN.md"))?, "- O");
        assert!(listed.len() >= 60, "OPEN.md lists {} items", listed.len());
        let mut named = Vec::new();
        for case in cases(&corpus_root())? {
            for file in &case.files {
                named.extend(open_ids(&fs::read_to_string(file)?, "// OPEN"));
            }
        }
        let missing: Vec<_> = listed.iter().filter(|id| !named.contains(id)).collect();
        assert!(missing.is_empty(), "OPEN items without a case: {missing:?}");
        let unknown: Vec<_> = named.iter().filter(|id| !listed.contains(id)).collect();
        assert!(
            unknown.is_empty(),
            "cases name unlisted OPEN items: {unknown:?}"
        );
        Ok(())
    }

    #[test]
    fn corpus_cases_are_discovered_with_their_files() -> io::Result<()> {
        let all = cases(&corpus_root())?;
        assert!(!all.is_empty());
        let mut names: Vec<_> = all.iter().map(|c| c.name.as_str()).collect();
        names.dedup();
        assert_eq!(names.len(), all.len(), "case names are unique");
        for case in &all {
            assert_eq!(case.files.first(), Some(&case.entry));
            if case.entry.ends_with(MULTI_FILE_ENTRY) && case.files.len() > 1 {
                // Imported siblings belong to their directory case only.
                for file in &case.files[1..] {
                    let name = relative(&corpus_root(), file);
                    assert!(all.iter().all(|c| c.name != name), "{name} is a case");
                }
            }
        }
        Ok(())
    }
}
