//! Loads a Slint file graph. [`load`] parses the entry file, follows its
//! imports and `export ... from` clauses through a [`FileSystem`], and records
//! what each file imports and exports, so later passes look names up across
//! files without touching the file system again.
//!
//! An import path resolves as the Slint docs describe: `@name` and
//! `@name/rest` through [`Config::library_paths`], anything else against the
//! importing file's directory, then each of [`Config::include_paths`]. A
//! `std-widgets.slint` found nowhere else is the copy bundled in this crate.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use slintx_syntax::ast::{self, AstNode, Item};
use slintx_syntax::{Parse, SyntaxKind, SyntaxNode, SyntaxToken, TextRange, normalize_name};

/// Where the bundled std-widgets library sits in the graph. No disk path looks
/// like it, so a `std-widgets.slint` next to the importer still shadows it.
pub const STD_WIDGETS: &str = "builtin:/std-widgets.slint";
const STD_WIDGETS_NAME: &str = "std-widgets.slint";
const STD_WIDGETS_SOURCE: &str = include_str!("std-widgets.slint");

pub trait FileSystem {
    fn read(&self, path: &Path) -> io::Result<String>;
}

/// The real file system. wasm32 has none: use [`MemoryFs`] there.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiskFs;

impl FileSystem for DiskFs {
    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// Sources held in memory, keyed by path.
#[derive(Debug, Clone, Default)]
pub struct MemoryFs(HashMap<PathBuf, String>);

impl MemoryFs {
    pub fn insert(&mut self, path: impl AsRef<Path>, source: impl Into<String>) {
        self.0.insert(clean(path.as_ref()), source.into());
    }
}

impl FileSystem for MemoryFs {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.0
            .get(&clean(path))
            .cloned()
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Searched in order after the importing file's directory.
    pub include_paths: Vec<PathBuf>,
    /// `@name` is the file mapped here; `@name/rest` joins `rest` to it.
    pub library_paths: HashMap<String, PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Level,
    /// The file `range` is in.
    pub file: PathBuf,
    pub range: TextRange,
    pub message: String,
}

/// A declaration: the index of its file in [`Graph::modules`] and the name it
/// was declared with.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub module: usize,
    pub name: String,
}

/// A name as one file spells it, and the declaration it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    pub name: String,
    pub symbol: Symbol,
}

#[derive(Debug, Clone)]
pub struct Module {
    pub path: PathBuf,
    pub parse: Parse,
    /// Names brought in by `import { ... }`, under their `as` name if any.
    pub imports: Vec<Name>,
    /// Names other files can import.
    pub exports: Vec<Name>,
}

impl Module {
    pub fn export(&self, name: &str) -> Option<&Symbol> {
        find(&self.exports, name)
    }
}

fn find<'a>(names: &'a [Name], name: &str) -> Option<&'a Symbol> {
    let key = normalize_name(name);
    names
        .iter()
        .find(|n| normalize_name(&n.name) == key)
        .map(|n| &n.symbol)
}

#[derive(Debug, Clone, Default)]
pub struct Graph {
    /// The entry file first, then each file in the order it was first imported.
    pub modules: Vec<Module>,
    /// The loader's own diagnostics; syntax errors stay in each [`Module::parse`].
    pub diagnostics: Vec<Diagnostic>,
}

impl Graph {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.level == Level::Error)
    }
}

/// Loads `entry` and everything it imports. A missing file or name becomes a
/// diagnostic and the rest of the graph still loads.
pub fn load(fs: &dyn FileSystem, config: &Config, entry: &Path) -> Graph {
    let mut loader = Loader {
        fs,
        config,
        graph: Graph::default(),
        by_path: HashMap::new(),
        loading: Vec::new(),
    };
    let entry = clean(entry);
    match fs.read(&entry) {
        Ok(source) => {
            loader.load(entry, &source);
        }
        Err(err) => loader.graph.diagnostics.push(Diagnostic {
            level: Level::Error,
            message: format!("Cannot read {}: {err}", entry.display()),
            file: entry,
            range: TextRange::default(),
        }),
    }
    loader.graph
}

enum Resolved {
    Known(PathBuf),
    New(PathBuf, String),
    Unreadable(PathBuf, io::Error),
    Missing,
}

struct Loader<'a> {
    fs: &'a dyn FileSystem,
    config: &'a Config,
    graph: Graph,
    by_path: HashMap<PathBuf, usize>,
    /// Files whose imports are still being followed: importing one is a cycle.
    loading: Vec<PathBuf>,
}

impl Loader<'_> {
    fn load(&mut self, path: PathBuf, source: &str) -> usize {
        let parse = slintx_syntax::parse(source);
        let file = parse.tree::<ast::File>();
        let index = self.graph.modules.len();
        self.graph.modules.push(Module {
            path: path.clone(),
            parse,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        self.by_path.insert(path.clone(), index);
        self.loading.push(path);
        if let Some(file) = file {
            self.items(index, &file);
        }
        self.loading.pop();
        index
    }

    fn items(&mut self, index: usize, file: &ast::File) {
        // Imports are hoisted: a name may be used or re-exported above its import.
        let mut locals = Vec::new();
        for item in file.items() {
            match item {
                Item::Import(import) => self.import(index, &import),
                Item::Export(export) => locals.extend(export.declaration().and_then(declared)),
                item => locals.extend(declared(item)),
            }
        }
        let locals: Vec<Name> = locals
            .into_iter()
            .map(|token| local(index, &token))
            .collect();
        let mut exports = Exports::default();
        let mut explicit = false;
        for item in file.items() {
            if let Item::Export(export) = item {
                explicit = true;
                self.export(index, &export, &locals, &mut exports);
            }
        }
        let last_component = file.items().filter_map(|item| match item {
            Item::Component(c) => Some(c),
            _ => None,
        });
        if let (false, Some(component)) = (explicit, last_component.last()) {
            if let Some(name) = component.name() {
                exports.names.push(local(index, &name));
            }
            self.diagnostic(
                Level::Warning,
                index,
                start(component.syntax()),
                "Component is implicitly marked for export. This is deprecated and it should be explicitly exported".into(),
            );
        }
        self.graph.modules[index].exports = exports.names;
    }

    fn import(&mut self, index: usize, import: &ast::Import) {
        // `import "font.ttf";` registers a font; fonts come with text rendering.
        let braces = import
            .syntax()
            .children_with_tokens()
            .any(|e| e.kind() == SyntaxKind::LBrace);
        let Some(path) = import.path().filter(|_| braces) else {
            return;
        };
        let Some(dep) = self.dependency(index, &path) else {
            return;
        };
        for spec in import.specifiers() {
            let Some(name) = spec.name() else { continue };
            let Some(symbol) = self.exported(index, dep, &name, &path) else {
                continue;
            };
            let local = spec.alias().unwrap_or(name);
            let name = local.text().to_owned();
            self.graph.modules[index]
                .imports
                .push(Name { name, symbol });
        }
    }

    fn export(&mut self, index: usize, export: &ast::Export, locals: &[Name], out: &mut Exports) {
        if let Some(token) = export.declaration().and_then(declared) {
            return self.add(index, out, local(index, &token), token.text_range());
        }
        let from = match export.path() {
            Some(path) => match self.dependency(index, &path) {
                Some(dep) => Some((dep, path)),
                None => return,
            },
            None => None,
        };
        if export.star().is_some() {
            // The parser accepts `*` only with `from`.
            let Some((dep, path)) = from else { return };
            for name in self.graph.modules[dep].exports.clone() {
                self.add(index, out, name, path.text_range());
            }
            return;
        }
        for spec in export.specifiers() {
            let Some(name) = spec.name() else { continue };
            let symbol = match &from {
                Some((dep, path)) => self.exported(index, *dep, &name, path),
                None => self.visible(index, locals, &name),
            };
            let Some(symbol) = symbol else { continue };
            let public = spec.alias().unwrap_or(name);
            let name = public.text().to_owned();
            self.add(index, out, Name { name, symbol }, public.text_range());
        }
    }

    fn add(&mut self, index: usize, out: &mut Exports, name: Name, range: TextRange) {
        let key = normalize_name(&name.name);
        if let Some(first) = out
            .names
            .iter()
            .position(|n| normalize_name(&n.name) == key)
        {
            let message = format!("Duplicated export '{}'", name.name);
            self.error(index, range, message.clone());
            // Slint flags the first export too, once per name.
            if !out.reported.contains(&key) {
                self.error(index, out.ranges[first], message);
                out.reported.push(key);
            }
            return;
        }
        out.names.push(name);
        out.ranges.push(range);
    }

    /// A name `export { name }` refers to: declared in this file or imported.
    fn visible(&mut self, index: usize, locals: &[Name], name: &SyntaxToken) -> Option<Symbol> {
        let found = find(locals, name.text())
            .or_else(|| find(&self.graph.modules[index].imports, name.text()))
            .cloned();
        if found.is_none() {
            self.error(
                index,
                name.text_range(),
                format!("'{}' not found", name.text()),
            );
        }
        found
    }

    fn exported(
        &mut self,
        index: usize,
        dep: usize,
        name: &SyntaxToken,
        path: &SyntaxToken,
    ) -> Option<Symbol> {
        let found = self.graph.modules[dep].export(name.text()).cloned();
        if found.is_none() {
            let message = format!(
                "No exported type called '{}' found in {}",
                name.text(),
                path.text()
            );
            self.error(index, path.text_range(), message);
        }
        found
    }

    /// The file `path` names, loaded if it was not yet.
    fn dependency(&mut self, index: usize, path: &SyntaxToken) -> Option<usize> {
        let range = path.text_range();
        match self.resolve(index, unquote(path.text())) {
            Resolved::Known(file) if self.loading.contains(&file) => {
                self.error(index, range, format!("Recursive import of {}", path.text()));
                None
            }
            Resolved::Known(file) => self.by_path.get(&file).copied(),
            Resolved::New(file, source) => Some(self.load(file, &source)),
            Resolved::Unreadable(file, err) => {
                let message = format!("Cannot read {}: {err}", file.display());
                self.error(index, range, message);
                None
            }
            Resolved::Missing => {
                let message = format!(
                    "Cannot find requested import {} in the include search path",
                    path.text()
                );
                self.error(index, range, message);
                None
            }
        }
    }

    fn resolve(&self, index: usize, written: &str) -> Resolved {
        let candidates: Vec<PathBuf> = match written.strip_prefix('@') {
            Some(library) => {
                let (name, rest) = library.split_once('/').unwrap_or((library, ""));
                let root = self.config.library_paths.get(name);
                root.map(|root| {
                    if rest.is_empty() {
                        root.clone()
                    } else {
                        root.join(rest)
                    }
                })
                .into_iter()
                .collect()
            }
            None => {
                let dir = self.graph.modules[index]
                    .path
                    .parent()
                    .unwrap_or(Path::new(""));
                std::iter::once(dir)
                    .chain(self.config.include_paths.iter().map(PathBuf::as_path))
                    .map(|dir| dir.join(written))
                    .collect()
            }
        };
        for candidate in candidates.iter().map(|c| clean(c)) {
            if self.by_path.contains_key(&candidate) {
                return Resolved::Known(candidate);
            }
            match self.fs.read(&candidate) {
                Ok(source) => return Resolved::New(candidate, source),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Resolved::Unreadable(candidate, err),
            }
        }
        if written != STD_WIDGETS_NAME {
            return Resolved::Missing;
        }
        let builtin = PathBuf::from(STD_WIDGETS);
        if self.by_path.contains_key(&builtin) {
            return Resolved::Known(builtin);
        }
        Resolved::New(builtin, STD_WIDGETS_SOURCE.to_owned())
    }

    fn error(&mut self, index: usize, range: TextRange, message: String) {
        self.diagnostic(Level::Error, index, range, message);
    }

    fn diagnostic(&mut self, level: Level, index: usize, range: TextRange, message: String) {
        let file = self.graph.modules[index].path.clone();
        self.graph.diagnostics.push(Diagnostic {
            level,
            file,
            range,
            message,
        });
    }
}

#[derive(Default)]
struct Exports {
    names: Vec<Name>,
    /// Where each of `names` was exported, for duplicate errors.
    ranges: Vec<TextRange>,
    /// Normalized names already reported as duplicated.
    reported: Vec<String>,
}

/// The name a top-level declaration introduces.
fn declared(item: Item) -> Option<SyntaxToken> {
    match item {
        Item::Component(c) => c.name(),
        Item::Global(g) => g.name(),
        Item::Struct(s) => s.name(),
        Item::Enum(e) => e.name(),
        Item::Import(_) | Item::Export(_) => None,
    }
}

fn local(module: usize, token: &SyntaxToken) -> Name {
    let name = token.text().to_owned();
    let symbol = Symbol {
        module,
        name: name.clone(),
    };
    Name { name, symbol }
}

/// The first significant token of `node`: leading comments are not part of
/// where Slint reports an item.
fn start(node: &SyntaxNode) -> TextRange {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
        .map_or(node.text_range(), |t| t.text_range())
}

fn unquote(text: &str) -> &str {
    let text = text.strip_prefix('"').unwrap_or(text);
    text.strip_suffix('"').unwrap_or(text)
}

/// `path` with `.` dropped and `..` folded lexically, so one file reached
/// through two spellings loads once.
fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other),
        }
    }
    out
}
