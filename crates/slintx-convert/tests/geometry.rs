//! Each fixture laid out by Slint itself (the reference) and by the
//! converted output on its real target, compared element by element.
//!
//! Elements are matched by their `.slint` line and column: HTML carries it as
//! `data-at`, the other outputs through `Options::source_tags`, and Slint's
//! interpreter maps the same position back to the element's rectangle. Both
//! sides leave out elements of zero size. Repeated and inlined elements are
//! compared instance by instance in reading order.
//!
//! Rectangles must agree within `TOLERANCE`. Pixels are only a soft check:
//! the targets draw their own widgets, so a frame past `SOFT_PIXEL_SHARE` is
//! reported, not failed; a blank frame fails. Every frame, with its diff, is
//! written under `target/tmp/slintx-convert-geometry/`.
//!
//! The platform tests need their tools and are ignored by default: the
//! Docker image in `tests/platform/` runs the GTK and HTML ones
//! (`just convert-platforms`), `just convert-apple` the AppKit one, and the
//! UIKit one needs a booted iOS Simulator besides.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use slint::platform::software_renderer::RepaintBufferType;
use slint::{ComponentHandle, PhysicalSize};
use slintx_convert::ir::Node;
use slintx_convert::{Format, Options, Platform, convert};

/// Logical pixels two rectangles' edges may differ by.
const TOLERANCE: f64 = 2.0;
/// Largest per-channel difference that still counts as the same pixel.
const CHANNEL_THRESHOLD: u8 = 48;
/// Share of differing pixels past which a frame is reported.
const SOFT_PIXEL_SHARE: f64 = 0.25;
/// The window size for a fixture whose root sets none.
const DEFAULT_VIEWPORT: (u32, u32) = (400, 300);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn close_to(&self, other: &Rect) -> bool {
        let edges = |r: &Rect| [r.x, r.y, r.x + r.w, r.y + r.h];
        edges(self)
            .iter()
            .zip(edges(other))
            .all(|(a, b)| (a - b).abs() <= TOLERANCE)
    }
}

impl std::fmt::Display for Rect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} at ({}, {})", self.w, self.h, self.x, self.y)
    }
}

/// One laid-out fixture: every tagged element's rectangles, in reading
/// order, and the frame as RGBA8.
struct Frame {
    rects: BTreeMap<String, Vec<Rect>>,
    pixels: Option<(u32, u32, Vec<u8>)>,
}

fn sort_reading_order(rects: &mut BTreeMap<String, Vec<Rect>>) {
    for list in rects.values_mut() {
        list.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        list.dedup();
    }
}

/// `key x y width height` lines, as the measurers print them.
fn parse_rects(text: &str) -> BTreeMap<String, Vec<Rect>> {
    let mut rects: BTreeMap<String, Vec<Rect>> = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [key, x, y, w, h] = fields[..] else {
            continue;
        };
        let num = |s: &str| {
            s.parse::<f64>()
                .unwrap_or_else(|e| unreachable!("{line:?}: {e}"))
        };
        rects.entry(key.to_owned()).or_default().push(Rect {
            x: num(x),
            y: num(y),
            w: num(w),
            h: num(h),
        });
    }
    sort_reading_order(&mut rects);
    rects
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.slint"))
}

fn root_node(name: &str) -> Node {
    match slintx_convert::to_ir(name, &common::source(name)) {
        Ok((doc, _)) => doc.root.unwrap_or_else(|| unreachable!("{name}: no root")),
        Err(e) => unreachable!("{name}: {e}"),
    }
}

/// The root's own size when it sets one, else `DEFAULT_VIEWPORT`; every
/// target gets the same.
fn viewport(root: &Node) -> (u32, u32) {
    let px = |e: &slintx_convert::ir::Extent| e.fixed.as_ref().and_then(|v| v.as_px());
    (
        px(&root.item.width).map_or(DEFAULT_VIEWPORT.0, |v| v.round() as u32),
        px(&root.item.height).map_or(DEFAULT_VIEWPORT.1, |v| v.round() as u32),
    )
}

/// Every node below the root, which is the window itself.
fn tagged(root: &Node) -> Vec<&Node> {
    fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
        for c in &n.children {
            out.push(c);
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// The byte offset of the element's type name: the interpreter looks
/// elements up by it, and the IR's origin may point at `id :=` before it.
fn type_name_offset(source: &str, line: u32, column: u32) -> usize {
    let start = source
        .split_inclusive('\n')
        .take(line as usize - 1)
        .map(str::len)
        .sum::<usize>()
        + column as usize
        - 1;
    let rest = &source[start..];
    let ident = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    let after = rest[ident..].trim_start();
    match after.strip_prefix(":=") {
        Some(tail) => start + (rest.len() - tail.trim_start().len()),
        None => start,
    }
}

/// The fixture laid out and drawn by Slint, or `None` when Slint rejects
/// it (the `unsupported` fixture imports a file that does not exist).
fn reference(name: &str) -> Option<Frame> {
    let source = common::source(name);
    let path = fixture_path(name);
    let root = root_node(name);
    let (width, height) = viewport(&root);
    let instance = shown(&source, &path, width, height)?;
    let window = instance.window();
    let mut rects: BTreeMap<String, Vec<Rect>> = BTreeMap::new();
    for n in tagged(&root) {
        let offset = type_name_offset(&source, n.origin.line, n.origin.column);
        for h in instance.component_positions(&path, offset as u32) {
            rects
                .entry(format!("{}:{}", n.origin.line, n.origin.column))
                .or_default()
                .push(Rect {
                    x: h.rect.origin.x.into(),
                    y: h.rect.origin.y.into(),
                    w: h.rect.size.width.into(),
                    h: h.rect.size.height.into(),
                });
        }
    }
    sort_reading_order(&mut rects);
    let snapshot = window
        .take_snapshot()
        .unwrap_or_else(|e| unreachable!("{name}: {e}"));
    let pixels = (
        snapshot.width(),
        snapshot.height(),
        snapshot.as_bytes().to_vec(),
    );
    if let Err(e) = instance.hide() {
        unreachable!("{name}: {e}");
    }
    Some(Frame {
        rects,
        pixels: Some(pixels),
    })
}

/// `source`'s last component built by Slint's interpreter and shown on the
/// software platform at `width` x `height`, or `None` when Slint rejects it.
fn shown(
    source: &str,
    path: &Path,
    width: u32,
    height: u32,
) -> Option<slint_interpreter::ComponentInstance> {
    if let Err(e) = slint_embed::platform::ensure_installed(RepaintBufferType::NewBuffer) {
        unreachable!("{e}");
    }
    let result = pollster::block_on(
        slint_interpreter::Compiler::default().build_from_source(source.to_owned(), path.into()),
    );
    if result.has_errors() {
        for d in result.diagnostics() {
            eprintln!("{}: {}", path.display(), d.message());
        }
        return None;
    }
    let component = result.component_names().last()?.to_owned();
    let instance = result
        .component(&component)?
        .create()
        .unwrap_or_else(|e| unreachable!("{}: {e}", path.display()));
    instance.window().set_size(PhysicalSize::new(width, height));
    if let Err(e) = instance.show() {
        unreachable!("{}: {e}", path.display());
    }
    Some(instance)
}

/// Rectangles that differ, or that only one side has.
fn divergences(reference: &Frame, frame: &Frame) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let keys: std::collections::BTreeSet<&String> =
        reference.rects.keys().chain(frame.rects.keys()).collect();
    for key in keys {
        let want = reference.rects.get(key).map_or(&[][..], Vec::as_slice);
        let got = frame.rects.get(key).map_or(&[][..], Vec::as_slice);
        let show = |list: &[Rect]| {
            list.iter()
                .map(Rect::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        if want.len() != got.len() || want.iter().zip(got).any(|(a, b)| !a.close_to(b)) {
            out.insert(
                key.clone(),
                format!("Slint [{}], target [{}]", show(want), show(got)),
            );
        }
    }
    out
}

fn scratch(target: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("slintx-convert-geometry")
        .join(target);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| unreachable!("{}: {e}", dir.display()));
    dir
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::write(path, bytes).unwrap_or_else(|e| unreachable!("{}: {e}", path.display()));
}

/// Any PNG as RGBA8: Chromium writes RGB.
fn read_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let fail = |e: &dyn std::fmt::Display| -> ! { unreachable!("{}: {e}", path.display()) };
    let bytes = std::fs::read(path).unwrap_or_else(|e| fail(&e));
    let header = rgba_png::read_png_header(&bytes, 1 << 28).unwrap_or_else(|e| fail(&e));
    let (width, height) = header.size();
    (
        width,
        height,
        header.decode_rgba().unwrap_or_else(|e| fail(&e)),
    )
}

fn run(command: &mut Command) -> String {
    let out = command
        .output()
        .unwrap_or_else(|e| unreachable!("{command:?}: {e}"));
    assert!(
        out.status.success(),
        "{command:?} failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn converted(name: &str, format: Format, platform: Platform) -> String {
    let options = Options {
        platform,
        source_tags: true,
        ..Options::default()
    };
    match convert(
        &format!("{name}.slint"),
        &common::source(name),
        format,
        &options,
    ) {
        Ok(c) => c.output,
        Err(e) => unreachable!("{name}: {e}"),
    }
}

/// The frame as it shows on a white page: AppKit leaves a view's background
/// transparent where Slint and the others paint one.
fn over_white(rgba: &[u8]) -> Vec<u8> {
    let (pixels, _) = rgba.as_chunks::<4>();
    pixels
        .iter()
        .flat_map(|[r, g, b, a]| {
            let a = u16::from(*a);
            let blend = |c: &u8| ((u16::from(*c) * a + 255 * (255 - a)) / 255) as u8;
            [blend(r), blend(g), blend(b), 255]
        })
        .collect()
}

/// Reports the share of differing pixels and writes the frames; never
/// fails, since the targets draw their own widgets. A measurer that drew
/// nothing also measured nothing, which the rectangles catch.
fn soft_pixel_check(name: &str, dir: &Path, reference: &Frame, frame: &Frame) {
    let (Some((rw, rh, want)), Some((w, h, got))) = (&reference.pixels, &frame.pixels) else {
        return;
    };
    if (rw, rh) != (w, h) {
        eprintln!("{name}: frame is {w}x{h}, Slint's {rw}x{rh}; pixels not compared");
        return;
    }
    let (want, got) = (over_white(want), over_white(got));
    let differing = rgba_png::differing_pixels(&want, &got, CHANNEL_THRESHOLD);
    let share = differing as f64 / f64::from(w * h);
    let png = |pixels: &[u8]| {
        rgba_png::encode_png(*w, *h, pixels).unwrap_or_else(|e| unreachable!("{name}: {e}"))
    };
    write(&dir.join(format!("{name}-slint.png")), png(&want));
    write(
        &dir.join(format!("{name}-diff.png")),
        png(&rgba_png::diff_image(&want, &got, CHANNEL_THRESHOLD)),
    );
    let note = if share > SOFT_PIXEL_SHARE {
        " (past the soft threshold)"
    } else {
        ""
    };
    eprintln!("{name}: {:.1}% of pixels differ{note}", share * 100.0);
}

/// `(fixture, why)`: fixtures a target is known to lay out differently from
/// Slint. Their differences are printed, not failed; a listed fixture that
/// agrees again fails, so the list stays true. Kept per fixture because a
/// native control's own size moves every element after it.
type Known = &'static [(&'static str, &'static str)];

/// Measures every fixture Slint accepts with `measure(name, width, height,
/// dir)` and compares it with the reference.
fn check(target: &str, known: Known, measure: impl Fn(&str, u32, u32, &Path) -> Frame) {
    let dir = scratch(target);
    let mut failures = Vec::new();
    for name in common::valid() {
        let Some(reference) = reference(name) else {
            eprintln!("{name}: Slint rejects it, skipped");
            continue;
        };
        let (width, height) = viewport(&root_node(name));
        let frame = measure(name, width, height, &dir);
        soft_pixel_check(name, &dir, &reference, &frame);
        let found = divergences(&reference, &frame);
        let why = known.iter().find(|(f, _)| *f == name).map(|(_, why)| why);
        match why {
            Some(why) if found.is_empty() => {
                failures.push(format!("{name}: listed as known ({why}) but agrees now"));
            }
            Some(why) => {
                eprintln!("{name}: known to differ, {why}:");
                for (key, what) in &found {
                    eprintln!("  {key}: {what}");
                }
            }
            None => failures.extend(
                found
                    .iter()
                    .map(|(key, what)| format!("{name} {key}: {what}")),
            ),
        }
    }
    assert!(
        failures.is_empty(),
        "{target} differs from Slint (frames in {}):\n{}",
        dir.display(),
        failures.join("\n")
    );
}

/// The reference runs everywhere, so a fixture Slint can no longer lay out
/// shows up without any platform tool.
#[test]
fn slint_lays_out_every_fixture() {
    for name in common::valid().filter(|n| *n != "unsupported") {
        let frame = reference(name).unwrap_or_else(|| unreachable!("{name}: Slint rejects it"));
        let elements = tagged(&root_node(name)).len();
        assert!(
            !frame.rects.is_empty(),
            "{name}: none of {elements} elements has a rectangle"
        );
    }
}

/// Slint's own default stretch for each widget the mapping table lists,
/// measured beside a `Rectangle` (stretch 1, preferred size 0) in a 1000 px
/// layout: the widget grows past its preferred size by its stretch times the
/// `Rectangle`'s size. Layouts are left out; theirs comes from their children.
#[test]
fn default_stretch_matches_slint() {
    use slintx_convert::mapping::{Role, WIDGETS};
    const SIZE: u32 = 1000;
    let std_widgets: Vec<&str> = WIDGETS
        .iter()
        .filter(|w| w.slint.chars().next().is_some_and(char::is_uppercase))
        .map(|w| w.slint)
        .filter(|w| {
            ![
                "Window",
                "Rectangle",
                "Empty",
                "TouchArea",
                "FocusScope",
                "Flickable",
                "Text",
                "Image",
                "TextInput",
                "Row",
            ]
            .contains(w)
                && !w.ends_with("Layout")
        })
        .collect();
    let mut wrong = Vec::new();
    for w in WIDGETS {
        // A `GroupBox` lays its children out in a grid, so its stretch comes
        // from them like a layout's; empty, it measures 0 across.
        if matches!(w.role, Role::Window | Role::Layout(_) | Role::GridRow) || w.slint == "GroupBox"
        {
            continue;
        }
        for horizontal in [true, false] {
            let (outer, inner) = if horizontal {
                ("VerticalLayout", "HorizontalLayout")
            } else {
                ("HorizontalLayout", "VerticalLayout")
            };
            let lines = [
                format!(
                    "import {{ {} }} from \"std-widgets.slint\";",
                    std_widgets.join(", ")
                ),
                format!(
                    "export component Probe inherits Window {{ width: {SIZE}px; height: {SIZE}px; {outer} {{"
                ),
                format!("{inner} {{ alignment: start; {} {{ }} }}", w.slint),
                format!("{inner} {{ {} {{ }} Rectangle {{ }} }}", w.slint),
                "} }".to_owned(),
            ];
            let source = lines.join("\n");
            let path = scratch("stretch").join(format!("{}.slint", w.slint));
            let Some(instance) = shown(&source, &path, SIZE, SIZE) else {
                // `Empty` is not public in Slint 1.18; the table keeps it for
                // newer sources.
                eprintln!("{}: Slint rejects it, not measured", w.slint);
                break;
            };
            // The size along `inner` of the first element named `what` on `line`.
            let size = |line: usize, what: &str| {
                let start = lines[..line].iter().map(|l| l.len() + 1).sum::<usize>();
                let column = lines[line]
                    .find(&format!("{inner} {{ "))
                    .map(|i| i + inner.len() + 3)
                    .and_then(|i| lines[line][i..].find(what).map(|j| i + j))
                    .unwrap_or_else(|| unreachable!("{what} on {line}"));
                instance
                    .component_positions(&path, (start + column) as u32)
                    .first()
                    .map_or(0.0, |h| {
                        f64::from(if horizontal {
                            h.rect.size.width
                        } else {
                            h.rect.size.height
                        })
                    })
            };
            let preferred = size(2, w.slint);
            let grown = size(3, w.slint);
            let filler = size(3, "Rectangle {");
            let measured = (grown - preferred) / filler;
            let listed = if horizontal { w.stretch.0 } else { w.stretch.1 };
            if (measured - listed).abs() > 0.05 {
                let axis = if horizontal { "horizontal" } else { "vertical" };
                wrong.push(format!(
                    "{} {axis}: Slint {measured:.2}, table {listed}",
                    w.slint
                ));
            }
            if let Err(e) = instance.hide() {
                unreachable!("{e}");
            }
        }
    }
    assert!(wrong.is_empty(), "default stretch:\n{}", wrong.join("\n"));
}

/// Measures the page in headless Chromium: a script at the end of the body
/// prints each `data-at` element's rectangle into the dumped DOM. Chromium
/// keeps a minimum window size, so the root takes the viewport's size itself
/// in a larger window and the screenshot is cropped to it.
fn chromium(name: &str, width: u32, height: u32, dir: &Path) -> Frame {
    const MEASURE: &str = r#"<script>
(() => {
  const root = document.querySelector("[data-slint]").getBoundingClientRect();
  const lines = [];
  for (const e of document.querySelectorAll("[data-at]")) {
    const r = e.getBoundingClientRect();
    if (e !== document.querySelector("[data-slint]") && r.width > 0 && r.height > 0)
      lines.push([e.dataset.at, r.x - root.x, r.y - root.y, r.width, r.height].join(" "));
  }
  const out = document.createElement("pre");
  out.id = "slintx-geometry";
  out.textContent = "\n" + lines.join("\n") + "\n";
  document.body.append(out);
})();
</script>
"#;
    let viewport = format!(
        "<style>main[data-slint=Window]{{width:{width}px!important;height:{height}px!important}}</style></head>"
    );
    let html = converted(name, Format::Html, Platform::UiKit).replace("</head>", &viewport);
    let page = dir.join(format!("{name}.html"));
    let measured = dir.join(format!("{name}-measure.html"));
    write(&page, &html);
    write(
        &measured,
        html.replace("</body>", &format!("{MEASURE}</body>")),
    );
    let png = dir.join(format!("{name}-html.png"));
    let chrome = |extra: &[String], file: &Path| {
        let mut c = Command::new("chromium");
        c.args([
            "--headless",
            "--no-sandbox",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            &format!("--window-size={},{}", width.max(800), height + 200),
        ])
        .args(extra)
        .arg(format!("file://{}", file.display()));
        run(&mut c)
    };
    let dom = chrome(&["--dump-dom".to_owned()], &measured);
    chrome(&[format!("--screenshot={}", png.display())], &page);
    let geometry = dom
        .split_once(r#"<pre id="slintx-geometry">"#)
        .and_then(|(_, rest)| rest.split_once("</pre>"))
        .map(|(text, _)| text)
        .unwrap_or_else(|| unreachable!("{name}: no geometry in the dumped DOM"));
    let (shot_width, _, shot) = read_png(&png);
    let row = |y: u32| {
        let start = (y * shot_width * 4) as usize;
        &shot[start..start + (width * 4) as usize]
    };
    Frame {
        rects: parse_rects(geometry),
        pixels: Some((width, height, (0..height).flat_map(row).copied().collect())),
    }
}

/// Measures the Blueprint in GTK 4: `blueprint-compiler`, then
/// `gtk_geometry.py` under the X server the caller provides.
fn gtk(name: &str, width: u32, height: u32, dir: &Path) -> Frame {
    let blp = dir.join(format!("{name}.blp"));
    let ui = dir.join(format!("{name}.ui"));
    let png = dir.join(format!("{name}-gtk.png"));
    write(&blp, converted(name, Format::Blueprint, Platform::UiKit));
    run(Command::new("blueprint-compiler")
        .arg("compile")
        .arg("--output")
        .arg(&ui)
        .arg(&blp));
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/platform/gtk_geometry.py");
    let text = run(Command::new("python3")
        .arg(script)
        .arg(&ui)
        .arg(&png)
        .arg(width.to_string())
        .arg(height.to_string()));
    Frame {
        rects: parse_rects(&text),
        pixels: Some(read_png(&png)),
    }
}

const HTML_KNOWN: Known = &[
    (
        "nested_layouts",
        "the browser's font sets texts 3 to 5 px wider than Slint's",
    ),
    (
        "properties",
        "native form controls are 21 px high where Fluent's are 32 px, \
         so everything below them moves up",
    ),
    (
        "repeat_if",
        "a `for` is emitted once, as a template, and Slint lays out every instance",
    ),
    (
        "widgets",
        "native form controls have their own sizes (a button is 21 px, Fluent's 32 px)",
    ),
];

#[test]
#[ignore = "needs headless Chromium"]
fn html_matches_slint() {
    check("html", HTML_KNOWN, chromium);
}

const GTK_KNOWN: Known = &[
    (
        "nested_layouts",
        "`space-between` has no Gtk.Box equivalent, `alignment: end` shrinks the box \
         to its content, a spanning cell grows every column it covers, \
         and GTK's font sets texts 3 to 5 px wider",
    ),
    (
        "properties",
        "GTK will not make a window smaller than its content, which is taller \
         than the 480 px Slint squeezes it into; GTK's controls are 34 px high",
    ),
    (
        "repeat_if",
        "a `for` is emitted once, as a template, and Slint lays out every instance",
    ),
    (
        "stretch",
        "GTK expands children equally, so the 1:2 stretch becomes 1:1",
    ),
    (
        "unicode",
        "GTK's font sets these lines up to 4 px taller or shorter",
    ),
    (
        "widgets",
        "GTK's controls have their own sizes (a button is 34 px, Fluent's 32 px), \
         so the window grows past 300 px",
    ),
];

#[test]
#[ignore = "needs blueprint-compiler, GTK 4 and an X server"]
fn gtk_matches_slint() {
    check("gtk", GTK_KNOWN, gtk);
}
