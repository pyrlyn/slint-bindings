# slintx-convert

Converts `.slint` files to XML, HTML, Apple storyboards (UIKit or AppKit, with
Auto Layout) and GTK 4 Blueprint. It keeps structure and meaning: the element
tree, layouts, alignment, stretch, texts, bindings and handlers. The output is
meant to be laid out like the Slint UI, not to match it pixel for pixel.

Parsing uses [`slintx-syntax`](../slintx-syntax), never the Slint compiler.
Slint logic is never run. Bindings and handlers become hooks the target
understands: ids and `data-*` attributes in HTML, outlets and actions in a
storyboard, signal handlers in Blueprint.

## Command line

```bash
cargo run -p slintx-convert -- ui/app.slint --to html -o app.html
```

| Option | Meaning |
| --- | --- |
| `--to xml\|html\|storyboard\|blueprint` | Output format. |
| `-o, --output <file>` | Where to write; standard output when omitted. |
| `--platform uikit\|appkit` | Storyboard target (default `uikit`). |
| `--strict` | Treat every warning as an error. |

Parse errors are printed with their position and stop the run. Constructs an
output cannot express are printed as warnings (`file:line:col: warning: …`)
and left in the output as comments. The exit code is non-zero only when there
are errors.

## Library

```rust
use slintx_convert::{Format, Options, convert};

let source = std::fs::read_to_string("app.slint")?;
let result = convert("app.slint", &source, Format::Blueprint, &Options::default())?;
println!("{}", result.output);
for warning in &result.warnings {
    eprintln!("{warning}");
}
```

`to_ir` returns the format-free intermediate representation (`ir::Document`).
It holds the widget tree, the layout model, typed values with units, bindings,
handlers, and the file, line and column of every node. Each output format
implements the `Backend` trait. The widget table in `mapping.rs` is plain
data shared by all backends.

## Support

| Feature | XML | HTML | Storyboard | Blueprint |
| --- | --- | --- | --- | --- |
| Element tree, ids, source positions | yes | yes (`data-at`) | yes (ids as outlets) | yes |
| `HorizontalLayout` / `VerticalLayout` | yes | flexbox | stack views | `Gtk.Box` |
| `GridLayout`, `Row`, spans | yes | CSS grid | row stacks; spans approximated | `Gtk.Grid` |
| Spacing, padding | yes | yes | yes | yes |
| `alignment` | yes | yes | yes, with spacer views | stretch, start, center, end |
| Stretch factors | yes | yes | hugging + proportional constraints | expand only; ratios warn |
| Size limits (`min`/`max`/`preferred`) | yes | yes | constraints | minimum only |
| Absolute `x`/`y` | yes | yes | constraints | overlay margins |
| Texts, Unicode, `@tr` | yes | yes | yes | yes (`_()`) |
| Text color and font | yes | yes | yes (AppKit: no italic) | Pango markup |
| std-widgets controls | yes | native form controls | native controls | native widgets |
| Property bindings | yes (with references) | `data-bind-*` | outlet + comment | comment |
| Two-way bindings | yes | `data-sync-*` | outlet + comment | comment |
| Callbacks | yes | `data-on-*` | actions | signal handlers |
| `changed` handlers | yes | `data-changed-*` | outlet + comment, warns | `notify::` handlers |
| `for` / `if` | yes | `data-for` / `data-if` | template once + comment | template once + comment |
| Local components, `@children` | inlined | inlined | inlined | inlined |

[`docs/mapping.md`](docs/mapping.md) lists the mapping in full, per widget,
layout, property and handler.

## Limitations

- Only the main component is converted: the last exported one, or the last
  one when none is exported. Local components are inlined; imports from files
  other than `std-widgets.slint` are not resolved.
- `animate`, `states`, transitions and functions are not converted.
- Expressions are kept as text. A property whose value is computed has no
  value in the output, only the binding.
- `for` and `if` emit their element once, as a template. The controller or
  page code must repeat or hide it.
- `rem` and `%` lengths are kept in XML and HTML only.
- HTML uses a 14 px system font. Storyboards use 14 pt where the source sets
  no size, so that outputs can be compared with each other.
- Storyboard: stack views cannot span grid cells. AppKit has no backgrounds,
  scrolling or `TouchArea` events here. UIKit labels centre their text
  vertically.
- Blueprint: GTK expands children equally, so stretch ratios other than 0 and
  1 are lost. `max-*` and `preferred-*` sizes have no equivalent.
- Slint grows equal-stretch children by the same amount past their own size;
  storyboards make them the same size instead. Native controls keep their
  own sizes (an `NSButton` is 24 px high, Fluent's `Button` 32 px).
- A hidden element keeps its place in Slint, HTML and AppKit. UIKit stack
  views and GTK boxes drop it.

## Tests

```bash
cargo nextest run -p slintx-convert
```

- `tests/snapshots.rs`: every fixture through every backend, with `insta`.
- `tests/ir.rs`: the IR the lowering builds.
- `tests/cli.rs`: the command line, with `trycmd`.
- `tests/validate.rs`: XML and HTML parse as XML. Storyboards compile with
  `ibtool` and survive an Interface Builder round trip with every attribute
  kept. Blueprint compiles with `blueprint-compiler`.

- `tests/geometry.rs`: every fixture laid out by Slint itself (through
  `slint-interpreter`, a dev-dependency only) and by the converted output on
  its real target, compared element by element. Elements are matched by
  their `.slint` line and column (`--source-tags`); rectangles must agree
  within 2 px, and the share of differing pixels is reported, not failed.
  Known differences are listed per fixture with the reason, and a listed
  fixture that agrees again fails. Frames and diffs are written under
  `target/tmp/slintx-convert-geometry/`. The default stretch of every widget
  is checked against Slint here too.

The external tools are not installed everywhere, so these tests are
`#[ignore]`d. The geometry tests need `cargo nextest`: Slint takes one
platform per process.

```bash
# Blueprint and GTK 4 under xvfb, HTML in headless Chromium: the Docker image
# in tests/platform, on Colima.
just convert-platforms
# ibtool and the AppKit geometry, on macOS.
just convert-apple
# The UIKit geometry, in a booted iOS Simulator.
cargo nextest run -p slintx-convert --run-ignored only -E 'test(uikit)'
```
