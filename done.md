# Done

### T1. Scaffold and M1 CPU spike

Rust workspace with `slint-bindings-core` (custom Slint platform, software
renderer into a caller-owned RGBA8 buffer, input, timers, demo form compiled
with `slint-build`) and `slint-bindings-ffi` (`sb_*` C ABI, cbindgen header);
SwiftPM package with `SlintHost`, `SlintNSView` (CGImage layer contents,
`CADisplayLink`, mouse, scroll, keys, focus), `SlintDemoView`
(`NSViewRepresentable`) and a SwiftUI demo app; WinUI 3 skeleton (code only);
`research.md` with sources. `just check` passes on macOS (fmt, clippy, 9 tests,
header drift, `swift build`). The demo window itself was not opened (T2).

### T2. Run and verify the macOS demo

Headless end-to-end tests drive the demo form through the C ABI and through
`SlintHost`: the form paints, ASCII input lands in the name field, Continue and
Enter submit that name, and a repeated resize does not repaint. `SlintNSView`
presents the new frame inside `setFrameSize` and pins it to the top-left, so a
live resize does not stretch the previous image. A non-positive scale is
rejected, a null host is reported through `sb_last_error`, and the WinUI host
publishes a size only after the core accepts it. Replacing the pixel buffer
forces a full repaint; the reused-buffer renderer would otherwise leave the new
storage transparent.

`sb_host_render` at 800×600 pt, 2× (1600×1200), Apple Silicon. The first frame
includes font setup and a full raster; the dirty frame is a later paint after
the name changes. Wall time around the render call:

| Build | First frame | Dirty frame |
| --- | --- | --- |
| debug (what `just swift-run` links) | 14.8 ms | 6.2 ms |
| release | 8.4 ms | 1.0 ms |

The steady frame is under the 8 ms M1 bar in both builds. The demo process
started and stayed up. A captured frame shows the sign-in form at the top-left
of the 2× buffer.

### T3. Vendor slint_dart and share the embedding core

`slint_dart` (https://github.com/listepo/slint_dart) lives in this repo under
`flutter/`. The checkout at `packages/slint_dart` is not modified. Shared
embedding — the process-wide software platform, the Dart FFI event encoding,
and the UI-thread guard — lives in `slint-embed`
(`flutter/packages/slint/rust/embed`). `slint-dart-core` re-exports it, so the
interpreter, the generated AOT glue and Skia keep calling `slint_dart_core::`.
`slint-bindings-core` uses the same platform (reused buffer, host-owned
pixels). The copy's Slint pin matches this repo (`=1.18.1`). Intel macOS
(`x86_64-apple-darwin`) is not a build target.

`just check` passes. `cargo test` for `slint-embed`, `slint-dart-core` and
`slint-interpreter-ffi` passes in `flutter/`.

### T18. Snapshot and screenshot tests

The headless e2e checks that some pixel is opaque. A change to the form can
pass that and still paint the wrong screen. The demo frame is compared to
committed PNG snapshots, and the macOS view's presented image is compared to
the same files.

The caret blinks, so snapshots are the unfocused frame: the idle sign-in form
at 1× and 2×, and the form after the name is set to "Ada".
`SB_UPDATE_SNAPSHOTS=1` rewrites the PNGs when that picture is meant to change.
`just check` passes.

### T4. macOS keyboard, IME and focus

Adopt `NSTextInputClient` in `SlintNSView` so composition (CJK, dead keys,
emoji picker) reaches Slint as preedit and commit text; complete the AppKit →
Slint key map (function keys, Home/End, Page Up/Down, modifiers as separate
key events) with table-driven tests; make Tab move focus out of the Slint view
into SwiftUI when Slint has no next focus item. Done when a Japanese input
method commits text into the demo field and the key-map tests cover every
`slint::platform::Key`.

Execution: the AppKit → Slint map lives in `slint-bindings-core` (one table,
tested on every `Key`). `SlintNSView` adopts `NSTextInputClient`: marked text
is a preedit, `insertText` commits, and a rejected Tab or Backtab calls
`selectNextKeyView` / `selectPreviousKeyView`.

### T6. WinUI host builds and runs (M2)

The WinUI host compiles in CI. Virtual keys, IME composition and the per-RID
DLL are in place. The demo window was not launched (no Windows desktop here).

`VirtualKey` maps to Slint key text in `slint-bindings-core`, one row for every
`slint::platform::Key`, same bar as the AppKit map. Shift+Tab is Backtab, the
two sides of Shift, Control, Alt and the Windows key stay distinct, and
Control names the physical key. The panel sends named keys from `KeyDown` /
`KeyUp` and printable characters from `CharacterReceived`. A rejected Tab or
Backtab moves focus with `FocusManager`.

`CoreTextEditContext` is attached when `GetForCurrentView` succeeds. Its text
store is only the preedit: replacements call `sb_host_ime_*`, which shows the
reading and commits it on `CompositionCompleted`. A cancel drops it. Windows 10
desktop has no CoreWindow, so that call fails and `CharacterReceived` still
inserts text. A headless test commits 日 through the session and inserts a
following character.

`sb_host_render_bgra` writes premultiplied BGRA8, so `SlintHost.RenderBgra` no
longer swizzles. `just windows-dll` (and the ARM64 triple) produces the DLL.
`windows/Directory.Build.targets` copies it beside the app and under
`runtimes/<rid>/native`. `just pinvoke-check` fails if `NativeMethods.cs` and
the header disagree on `sb_*` names. Marshalling stays hand-written.

### T5. CI

GitHub Actions: `macos-26` (arm64) runs `just check`; `windows-latest` runs
`cargo build -p slint-bindings-ffi`, `cargo nextest run` and
`dotnet build windows/SlintDemo.WinUI`. Use the shared workflows from
`pyrlyn/ci` where they fit. Done when both jobs are required checks on `main`.

Result: `.github/workflows/ci.yml` already covered the card, so nothing in it
changed. Checked on the green `main` run for `9c1b353` (run 39153182955):

- `macos`: `just check` after `cargo install cbindgen` (`header-check`). The
  runner comes from pyrlyn/ci's `macos-runner` action: `MACOS_RUNNER` unset or
  `hosted` gives `macos-26`; it is set to the self-hosted `xcode-27-local` for
  now. `just renderer-check` from PR #32 needs no extra tool.
- `windows-host`: `just windows-dll` (`cargo rustc -p slint-bindings-ffi
  --crate-type cdylib`), the C# host tests, then `dotnet build
  windows/SlintDemo.WinUI` and a WinUI window smoke run.
- `rust / x86_64-pc-windows-msvc` (pyrlyn/ci `ci-rust.yml`): `cargo nextest run
  --workspace` on `windows-latest`, 60 tests passed.
- Required checks: the `protect-main` ruleset requires `gate`, which needs
  `rust`, `macos` and `windows-host` and fails if any of them fails or is
  cancelled. The one allowed skip is `macos` under `MACOS_CI_DISABLED=true`.

### T7. GPU path on macOS (M3)

FemtoVG renders on wgpu's Metal backend into a `CAMetalLayer` sublayer of
`SlintNSView`. The view's own layer is not replaced. `sb_demo_new_metal` builds
the host; `sb_host_gpu_render` presents. Live resize sets
`presentsWithTransaction` on the layer. If the adapter or the surface fails,
the view drops the layer and uses the CPU `CGImage` path. CPU and GPU hosts
share `slint-embed`'s platform: `set_next_window` stages the GPU adapter for
one component. The feature is off on Linux. The 4K frame-time checkpoint was
not measured.

### T8. GPU path on Windows (M3)

The same renderer presents into a `SwapChainPanel`. The panel passes
`ISwapChainPanelNative` to `sb_demo_new_swapchain`; wgpu creates the DXGI swap
chain and calls `SetSwapChain` on the UI thread. `CompositionScaleChanged`
resizes it. A failed COM query or adapter keeps the `WriteableBitmap` path.
The window was not launched, and the frame-time checkpoint was not measured.

### T72. slintx-wgpu: our own Slint renderer on wgpu

The GPU path draws through FemtoVG (`renderer-femtovg-wgpu`), which is Slint's
renderer code. Replace it with our own renderer, written from scratch on wgpu:
the build must not contain Slint's GPU renderers (`i-slint-renderer-femtovg`,
`i-slint-renderer-skia`), and no code is copied from them. Done when
`crates/slintx-wgpu` draws Slint scenes, tests compare its frames with the
software renderer's (the original) within a tolerance, the core's GPU host uses
it, and `cargo tree` shows no FemtoVG or Skia renderer.

Execution:

1. `crates/slintx-wgpu`: `WgpuRenderer` implements `i_slint_core`'s
   `RendererSealed` (text measuring through the default `sharedparley` methods
   and a `TextLayoutCache`) and, per frame, `ItemRenderer` + `GlyphRenderer`.
   Items become a display list of primitives in physical pixels: rounded
   rectangles and border rings (SDF), box shadows (Gaussian SDF), images and
   nine-slices (`graphics::fit`, `fit9slice`), glyphs (swash masks in an R8
   atlas), paths (lyon tessellation). Brushes: solid, linear, radial, conic
   (a gradient ramp texture). Clips: an intersected rectangle plus the latest
   corner radii. Opacity multiplies into every primitive. One WGSL pipeline,
   4× MSAA, premultiplied blending in sRGB-encoded space like the software
   renderer. `render_to_texture` draws into a caller's texture;
   `take_snapshot` reads an offscreen frame back.
   Not in the first cut: rotation and scale transforms
   (`supports_transformations` is false, as in the software renderer) and
   layers (children of `Opacity` are faded one by one).
2. Tests (`crates/slintx-wgpu/tests`): scenes built with `slint!` (rectangles,
   borders, radii, gradients, clips, shadows, text, images, paths, opacity)
   and the demo form, each drawn by the software renderer and by slintx-wgpu
   through `slint_embed`'s platform, compared with `Window::take_snapshot`.
   The comparison passes when the share of pixels whose largest channel
   difference exceeds a threshold stays under a budget; a failure writes the
   two frames and a diff PNG under `target/tmp/slintx-wgpu/`. No GPU adapter is a
   skip, as in `gpu_smoke.rs`.
3. `slint-bindings-core/src/gpu.rs` switches to `WgpuRenderer`; the
   `renderer-femtovg-wgpu` and `unstable-wgpu-30` features go away;
   `just renderer-check` (part of `just check`) fails when `cargo tree` finds
   `i-slint-renderer-femtovg` or `i-slint-renderer-skia`.
4. `toolchain.md`, `README.md`, `AGENTS.md`; `just check` passes.

Found while comparing: Slint 1.18's software renderer draws no box shadows,
ignores clip radii and samples images nearest-neighbour from the pixel's left
edge. Shadows and rounded clips are checked against expected pixels instead,
compared images are `pixelated`, and the image scene's budget covers the
one-pixel shift of scaled texel edges. The workspace `rust.md` lists the new
crates. `header-check` was not run (no cbindgen on that machine); the C ABI did
not change.

### T73. slintx-syntax: a standalone AST parser

slintx's first layer, and a public library on its own: `.slint` source in, a
lossless syntax tree (rowan) and a typed AST out, with byte spans on every node
and error recovery (an error node plus a diagnostic, then parsing goes on). It
covers the whole syntax in `research.md` §8.4.1 and the declaration forms of
§8.4.2, and depends on no checker or runtime. It also adds the licence gate:
a `just` check, part of `just check`, that fails when `slint`, `slint-*` or
`i-slint-*` enters the normal dependency graph of any `slintx-*` crate. Done
when round-trip tests (tree printed back equals the input, byte for byte) and
span tests pass on the fixture corpus, broken inputs produce a tree plus
diagnostics, and the crate builds and its tests run for `wasm32-unknown-unknown`
on its own.

Execution: split so that parallel sessions touch different files. T73.1 is
the crate; T73.2 is the gate and the wasm32 build around it. T73 closes when
both are on `main` and the parser round-trips the T74 corpus.

Result: T73.1 landed through pyrlyn/slint-bindings#40 and #33, T73.2 through #34, and the parser round-trips the T74 corpus.

### T73.1. slintx-syntax crate: lexer, lossless tree, parser, typed AST

`crates/slintx-syntax`: syntax kinds, a hand-written lexer (nested block
comments, `-` in identifiers, unit suffixes, string interpolation), a rowan
tree built by a recursive-descent parser with error recovery, typed AST views,
and diagnostics with byte spans. Own fixtures under
`crates/slintx-syntax/tests/fixtures/`. Done when round-trip, span and
error-recovery tests pass on those fixtures and the crate has no dependency on
`slint`, `slint-*` or `i-slint-*`.

Split at claim time: the whole crate is about 2300 lines of code, so it goes
in six commits of at most 500 lines each (T73.1.1–T73.1.6), stacked on this
branch; T73.1.4 was split in two when its code passed 500 lines. Each commit builds, passes its own tests and adds fixtures.

Execution plan:

1. T73.1.1 — `Cargo.toml` (rowan from `[workspace.dependencies]`, workspace
   lints), `src/kind.rs`, `src/lexer.rs`, `src/lib.rs` (`parse`, `lex`,
   `Parse`, `Diagnostic { message, range }`).
2. T73.1.2 — `src/parser.rs` (token cursor, trivia attachment, error nodes,
   nesting limit), `src/grammar/expr.rs`, `parse_binding_expression`.
3. T73.1.3 — `src/grammar/macros.rs` (strings, escapes, interpolation, `@`
   macros) and `src/grammar/stmt.rs` (types, code blocks, statements).
4. T73.1.4 — `src/grammar/items.rs` (top-level items) and
   `src/grammar/element.rs` (child elements, bindings, handlers).
5. T73.1.5 — `src/grammar/decl.rs`: declarations in element bodies.
6. T73.1.6 — `src/ast.rs`: typed views, one per node kind.
7. Tests in `tests/`: every fixture in `tests/fixtures/*.slint` round-trips
   byte for byte and parses with no diagnostics; every `tests/fixtures/errors/`
   file round-trips and yields the diagnostics in its `.expected` file (message
   and byte range); span tests check node ranges against substrings.
8. Verify: `cargo fmt --check`, `cargo clippy -p slintx-syntax --all-targets
   -- -D warnings`, `cargo nextest run -p slintx-syntax`, `cargo build -p
   slintx-syntax --target wasm32-unknown-unknown`, the tests on wasm32 through
   `wasm-bindgen-test-runner` (T73.2's wasm-check), `cargo tree -p
   slintx-syntax` shows no Slint crate.

### T73.1.1. slintx-syntax: kinds, lexer and lossless tree API

Token and node kinds, the hand-written lexer (BOM, nested block comments,
identifiers with `-`, numbers with unit suffixes and `%`, `#` colors, strings
with escapes and `\{expr}` interpolation, `@`, punctuation), the rowan
language and the public `parse` and `lex` API. Done when lexer tests pass and
any input prints back byte for byte.

### T73.1.2. slintx-syntax: parser core and expressions

The parser core (trivia attachment, error nodes, nesting limit against stack
overflow on untrusted input) and expressions with Slint's precedence and the
non-associativity diagnostics, postfix chains, array and object literals.
Done when expression tests round-trip with no diagnostics and broken
expressions recover.

### T73.1.3. slintx-syntax: strings, macros, types and statements

String templates and escape checks; the `@tr`, `@image-url`, gradient,
`@markdown` and `@keys` macros; types; code blocks and statements (`let`,
`return`, `if`/`else`, assignments). Done when their tests round-trip with no
diagnostics and broken input recovers.

### T73.1.4. slintx-syntax: items and element bodies

Imports, exports, components with `inherits`, globals, structs with field
defaults, enums, elements with ids, bindings, two-way bindings, callback
handlers and `@children`. Done when one fixture per construct round-trips with
no diagnostics and broken items recover.

### T73.1.5. slintx-syntax: declarations in element bodies

Property, callback and function declarations with visibility and purity,
`changed`, `states` with transitions, `animate`, `for` and `if`, plus the
fixtures that need them. Done when one fixture per construct round-trips with
no diagnostics, the valid fixtures cover every node kind and broken
declarations recover.

### T73.1.6. slintx-syntax: typed AST views

One typed view per node kind with accessors, plus span tests through the
views. Done when every fixture can be walked through the typed AST.

### T73.2. Licence gate and wasm32 build for slintx crates

`justfile`, CI and `toolchain.md` only: a `license-check` recipe in `just
check` that fails when `slint`, `slint-*` or `i-slint-*` appears in the normal
(non-dev) dependency graph of any workspace crate named `slintx-*`, proven by a
test against a throwaway crate that depends on one; and a `wasm-check` recipe
plus CI job that builds every `slintx-*` crate for `wasm32-unknown-unknown`
and runs its tests there. Done when both recipes run in CI and pass with no
`slintx-*` crate yet and with T73.1's crate once it lands.

Execution plan:

1. `scripts/slintx-check.sh license|wasm [Cargo.toml]`: lists workspace members
   named `slintx-*` (`cargo tree --workspace --depth 0`); `license` runs
   `cargo tree -p <crate> --target all -e normal,build` and fails on `slint`,
   `slint-*`, `i-slint-*`; `wasm` builds the crates for `wasm32-unknown-unknown`
   and runs `cargo test --tests` there through `wasm-bindgen-test-runner` on
   Node, failing a crate with no wasm32 `wasm-bindgen-test` dev-dependency
   (the runner would pass it untested). No `slintx-*` crate: both print that
   and exit 0. `slintx-wgpu` (T72) is exempt from `license` until T83 and from
   `wasm` until T85; `slintx-conformance` (T74, a native-only harness) from
   `wasm` for good.
2. `scripts/slintx-check-test.sh` with offline path-only fixture workspaces in
   `scripts/fixtures/license-check/` (stub crates named `slint`,
   `i-slint-core`, `slint-build`): none, clean (dev-only or non-slintx use) and
   exempt pass; direct, transitive, build-dependency and wasm-only dependencies
   fail; a manifest cargo cannot load errors instead of passing.
3. `justfile`: `license-check`, `license-check-test`, `wasm-check`, all in
   `just check`. `mise.toml`: `wasm32-unknown-unknown` rustup target, Node LTS,
   `wasm-bindgen` CLI at the lockfile's `wasm-bindgen` version. `toolchain.md`
   rows.
4. CI: a Linux `slintx` job (mise, `just license-check license-check-test
   wasm-check`) in `ci.yml`, required by the gate.

Verify: `just check` locally; the fixture test fails if the gate is weakened.

Result: pyrlyn/slint-bindings#34; the `slintx` CI job passes on `main` with `slintx-syntax` in the workspace.

### T74. slintx-conformance: differential harness against Slint

An unpublished crate (`publish = false`) that runs each `.slint` fixture
through slintx and through Slint (`slint-interpreter` and the software
renderer, test-only) and compares: accept or reject and diagnostic positions
first, then property values after scripted steps, layout geometry and pixels
(reusing `rgba-png`). The corpus is written here from the docs and from
penpotx emitter outputs; every OPEN item in `research.md` §8.4 gets a case.
Slint is the oracle only and never a dependency of a shipped crate. Done when
the harness runs in `just check` with the parser-level comparison live and the
later comparisons stubbed behind the stages that do not exist yet.

Execution:

1. `crates/slintx-conformance`, `publish = false`, no normal dependencies.
   `slint-interpreter` (workspace pin) and `pollster` are dev-dependencies
   for non-wasm targets only, so T73.2's gate (normal graph) needs no
   exception and its wasm32 build sees a Slint-free crate with no tests.
2. `corpus/<group>/*.slint`: one construct per file, valid and invalid, a
   header comment naming the docs page and the `research.md` §8.4 OPEN item
   it pins. A directory holding `main.slint` is one multi-file case.
3. Each case has a checked-in `<name>.expected` beside it: Slint's verdict and
   every diagnostic (level, file, line, column, message). Stage 1
   (`tests/stage1_slint.rs`) compiles each case with Slint and fails on any
   drift; `SLINTX_BLESS=1` rewrites the files. A small own format, not
   `insta`, because later stages read the expectations back as data.
4. Stage 2 (`tests/stage2_slintx.rs`): a `Frontend` trait in `src/lib.rs`,
   stubbed until T73.1 lands; then every case Slint accepts must parse with
   no syntax errors and round-trip byte for byte. Values, geometry and pixels
   stay documented stubs.
5. `just check` picks the tests up through `cargo nextest`; no recipe change.

State: stage 1 is live on 229 cases in 16 groups; `corpus/OPEN.md` numbers
the 60 OPEN items of §8.4.1 and §8.4.2 and a test checks each has a case.
Result: stage 2 is live on `slintx-syntax`: every file of every case Slint
accepts parses without syntax errors and round-trips. It found three parser
gaps, fixed under T73.1 (the pre-1.0 `Name := Base { }` component and export
lists without `;`). penpotx has no Slint emitter yet (checked at penpotx
`f090cc4`), so no emitter outputs are in the corpus; they join it when one
exists.

### T19. Drop Intel macOS leftovers from the Flutter copy

`flutter/` still listed Intel targets after the workspace dropped Intel macOS:
the copied CI installed `x86_64-apple-darwin` for a universal macOS release
build and `x86_64-apple-darwin`/`x86_64-apple-ios` for the Skia job, and
`rustTriple` mapped iOS x64 (a simulator that only runs on Intel Macs) to
`x86_64-apple-ios`. The macOS x64 mapping was already gone.

Plan: remove both Intel targets from `flutter/.github/workflows/ci.yml`, drop
the `(OS.iOS, Architecture.x64)` arm from `cargo_builder.dart`, so an Intel
request fails with the existing `UnsupportedError`. Check with `git grep`
that no `x86_64-apple-*` target remains under `flutter/`.

Result: the CI installs `aarch64-apple-darwin` and `aarch64-apple-ios-sim`
only; `rustTriple` knows only arm64 Apple targets.

### T11.1. Runtime components: core and C ABI

T11 (runtime components, generic API), Rust half. `.slint` is compiled at run
time with `slint-interpreter` (`=1.18.1`, the same pin as `slint`; one
`i-slint-core`), and properties and callbacks cross as JSON through slint_dart's
`slint-dart-interpreter`, reused as a path dependency
(`flutter/packages/slint_build/interpreter`) and excluded from this workspace so
it keeps flutter/'s lints and dependency versions. Closes weft T69.1's G1 and G2.

- Core: `runtime::compile(source, path)` returns the last exported component
  (`source: None` reads the file); `runtime::instantiate` hosts a
  `ComponentInstance` in `EmbeddedHost` or `GpuHost` and returns its JSON
  bridge. `DemoForm`, `build.rs` and `slint-build` are gone; core tests run the
  interpreted `ui/demo.slint`, and the GPU smoke test renders it offscreen.
- C ABI: `sb_compile`, `sb_definition_free`, `sb_component_new`,
  `sb_component_new_metal`, `sb_component_new_swapchain`,
  `sb_set_property_json`, `sb_get_property_json`, `sb_invoke_json`,
  `sb_on_callback`, `sb_string_free`.
- Rust: `SbHost::new`, `SbHost::new_gpu`, `SbHost::component`,
  `SbHost::bridge`, `SbHost::into_raw`. A scratch staticlib that links
  `slint-bindings-ffi` as an rlib and calls `SbHost::into_raw` exported
  `sb_compile`, `sb_component_new`, `sb_host_render` and `sb_on_callback` (`nm`);
  that check is not a committed test.
- Tests: the FFI e2e loads the demo from the file and keeps the snapshots
  pixel-identical; new cases cover JSON round trips, typed input read back,
  bad names, values and UTF-8, invoke, clearing a callback, compile
  diagnostics, a missing file, two hosts from one definition, a host built in
  Rust and handed to C, and Metal without a layer.

### T11.2. Runtime components: hosts load the demo from a file

The Swift and WinUI hosts load `demo.slint` from a file at run time and the
`sb_demo_*` calls left the ABI. Swift: `SlintDefinition` (`sb_compile`),
`SlintHost(definition:…)` with `setProperty(_:json:)`, `property(_:)`,
`invoke(_:argsJson:)` and `onCallback(_:_:)`; `SlintNSView` and
`SlintDemoView` take the definition; the demo app reads the first `.slint`
argument or the repository's `crates/slint-bindings-core/ui/demo.slint`. Swift
tests cover the file-loaded form, JSON errors and invoke. WinUI: the same in
`SlintDefinition`/`SlintHost` (C#), `SlintPanel.Definition`, and the demo copies
`demo.slint` next to the app. `just check` passes on macOS; the WinUI projects
were not built or run (no Windows machine here), only `just pinvoke-check`.

### T70. SwiftUI hosts a component built elsewhere (weft G3, T68 on macOS)

Weft's editor builds its `SbHost*` in its own Rust library and shows it in
`SlintNSView` (weft T69.1d). Closes weft T69.1's G3 and the macOS half of weft
T68.

- `SlintHost(adopting:)` takes ownership of an `SbHost*` another C function
  returned; nil throws that call's `sb_last_error`. The definition initializers
  go through it. `sb_host_pixel_size` (new in the C ABI and `NativeMethods.cs`)
  gives an adopted host's size.
- `SlintNSView(frame:prefersGpu:makeHost:)` takes the host from the caller. With
  `prefersGpu` the factory is offered a `CAMetalLayer`; a CPU host drops the
  layer, and a throw is retried without it. `SlintNSView(frame:host:)` and the
  definition initializer build on it. `SlintView` is the same as a SwiftUI view.
- The `SlintBindings` library target links no Rust library; the demo and test
  targets do. `just swift-package-check` (in `just check`) fails when the
  library target has unsafe flags.
- `fixtures/weft/login.slint` is weft's `weft-slint` output for the login screen
  (weft `b623a5b9`). The Rust e2e renders it at 480×800 (not one colour), clicks
  the email field, types into it, reads `email` back, and gets
  `perform("auth.submit", "form")` from the button and from Enter and
  `perform("nav.signup", "signup")` from the link; the button does nothing while
  the email is empty. The Swift test shows the same screen in `SlintView` inside
  an `NSHostingView` in an `NSWindow`, with a host adopted from raw `sb_*` calls,
  and sends `NSEvent` mouse and key events to the view: the frame is not one
  colour, the view becomes first responder, the typed email lands, and the
  clicks raise `perform`. The events go to the view's responder methods, not
  `NSWindow.sendEvent`: a test process is not the active app, so its window
  never becomes key, and AppKit keeps a click in a window that is not key.

`just check` passes on macOS.

### T71. WinUI 3 hosts a component built elsewhere (weft G4, T68 on Windows)

Weft's Windows shell (weft T69.1e) hands `SlintPanel` a host from its own
library. The panel created hosts only from `Definition`, and CI compiled the
WinUI code without running any of it.

- `SlintHost.Adopt(IntPtr)` takes ownership of an `SbHost*` another library
  made and reads its size from `sb_host_pixel_size`; zero, or a failed size
  read, throws that library's last error (the host is freed first). The public
  constructors go through it.
- `SlintPanel.HostFactory` (`Func<IntPtr, SlintHost>`) makes the host instead of
  `Definition`. It gets the `ISwapChainPanelNative` pointer, or zero for a CPU
  host; a CPU host returned for the pointer is kept, and a throw for the pointer
  asks again with zero. `Definition` is now a factory too.
- `windows/SlintBindings.Tests` (MSTest.Sdk, Microsoft.Testing.Platform, run with
  `dotnet run --project`): compiles `SlintHost.cs` and `NativeMethods.cs` rather
  than referencing the WinUI library, so it needs no Windows App Runtime. On the
  weft login screen it checks the BGRA frame is not one colour, walks the
  columns to click (the reset link while submit is disabled, then submit and
  "Create an account"), types into both fields with Backtab between them, and
  presses Enter; `perform` gets each action. An adopted host keeps its size and
  renders; adopting a failed constructor throws.
- The demo takes a `.slint` path argument and shows that file through
  `HostFactory`, listing its `perform` calls. `windows/smoke.ps1` starts it on
  the weft screen, captures the window and fails when the Slint panel is one
  colour.
- CI `windows-host`: the host tests, a self-contained build
  (`WindowsAppSDKSelfContained`), the smoke run, and the capture as the
  `weft-winui` artifact.

### T10. Multiple hosts and popups

`slint::platform::set_platform` is once per thread, and each component gets its
own `MinimalSoftwareWindow`. Verify two `SlintNSView`s in one window and two
windows; make Slint `PopupWindow` and menus render inside the host view (no
native child windows) and clip correctly. Done with a demo showing two views
and a combo-box popup in each.

Execution: Slint 1.18 renders a popup as a child window inside its parent's
buffer when the window adapter cannot make one of its own, which
`MinimalSoftwareWindow` cannot, so no native windows appear. CPU path only.
1. Fixture `crates/slint-bindings-core/ui/popups.slint`: a `ComboBox` near the
   top, one near the bottom edge (its popup must stay inside the view) and a
   `ContextMenuArea`, with `out` properties for what was picked.
2. Core (`host.rs`): `focus_changed(false)` closes the view's auto-close popups.
   Slint closes them on a click elsewhere in the same window only; a click in
   another host view or in SwiftUI never reaches it.
3. Core tests: two hosts on one thread stay independent; a popup draws into
   its host's buffer and no window adapter is created; the bottom popup fits
   inside the frame; closing it restores the frame pixel for pixel (partial
   repaint); focus loss closes it; the context menu draws inline.
4. Swift tests: two `SlintNSView`s in one `NSWindow` and one in each of two
   windows; opening a popup adds no `NSWindow`.
5. Demo: a second window with two CPU `SlintView`s of `popups.slint`.
Verify with `just check` and `just swift-run`.

Result: no native windows needed fixing; Slint 1.18 already draws every popup
inside the host's buffer and places it inside the view. The one change is in
`EmbeddedHost::focus_changed`: losing focus closes the view's auto-close
popups. `GpuHost::focus_changed` still needs the same `close_auto_popups` call
once the GPU path is replaced (T72). The demo opens a second window,
"Slint popups", with two CPU views.

### T12. Weft → Slint renderer (dropped)

Dropped, not done. It would have built `slint-bindings-weft`, a second Weft →
Slint mapping beside weft's own `weft-slint` (weft T67), which already
generates every corpus screen. The creator approved dropping it (weft T69.1,
decision 3). The hosts show `weft-slint`'s output instead: T70 and T71.

### T16. Slint desktop app for Weft (dropped)

Dropped, not done. Weft builds its desktop editor itself (weft T69,
WeftStudio) on these bindings; a second app here would duplicate it. Approved
with T12 (weft T69.1, decision 3).

### T90. Screenshot helpers from the shared rgba-png crate

Merged as 5e8fae1 under the id T89, which the open slintx-convert PR had already
taken; renumbered so that id stays with it.

`crates/pixel-diff` (T72) is the same code as `rgba-png` in pyrlyn/crates
(`rust/rgba-png`), which scull also uses. The workspace rule moves code shared by
two projects into one package, so the crate is deleted and the dev-dependencies of
`slint-bindings-ffi` and `slintx-wgpu` pin `rgba-png` by git rev (it is not on
crates.io). `encode_png`, `decode_png`, `differing_pixels` and `diff_image` are
byte-for-byte the functions `pixel-diff` had, so no snapshot changed.

Result: `cargo nextest run` passes all 131 tests, including the FFI e2e snapshot
and the slintx-wgpu comparisons that call the crate; clippy with warnings denied,
the format check and `just license-check` pass (`rgba-png` is a dev-dependency,
outside the gate's normal and build graph).

### T83. slintx-wgpu without i-slint-core

PR: https://github.com/pyrlyn/slint-bindings/pull/54 (split into T83.1–T83.3).

slintx-wgpu now has its own scene input types, a `Canvas` drawing API and
parley text, so it builds and draws without any Slint crate. The Slint
renderer (`WgpuRenderer`) is an adapter over `Canvas` behind the non-default
`slint` feature (`dep:i-slint-core`), which slint-bindings-core enables. The
slintx-wgpu licence exemption in `scripts/slintx-check.sh` is gone.

Checked: `cargo build -p slintx-wgpu --no-default-features` builds and
`cargo tree -p slintx-wgpu --no-default-features -e normal,build` has no
slint, slint-* or i-slint-* crate; `tests/compare.rs` passes through the
adapter with every tolerance unchanged, and all 14 frames are byte-identical
to `main`'s; `just renderer-check`, `just license-check`,
`just license-check-test`, `just clippy`, `just nextest`, the format check,
wasm-check, pinvoke-check, swift-package-check, swift-build and swift-test
pass. `just header-check` was not run: cbindgen is not installed on that
machine.

### T83.1. slintx-wgpu: own scene input types and a drawing API

`src/types.rs` holds `Color`, `GradientStop`, `Brush` (solid, linear, radial,
conic), `Pixels`, `Image`, `ImageFit`, `Tiling`, `FillRule`, `LineCap`,
`LineJoin`, `PathPlacement` and `Stroke`. `src/canvas.rs` is `Canvas`: a
save/restore stack of origin, clip (rounded or not) and opacity, with
`paint`, `fill_rect`, `stroke_rect`, `box_shadow`, `draw_image`, `fill_path`,
`stroke_path` and `draw_glyph_run`. `Renderer` (`render`,
`render_to_pixels`, `headless`) draws a closure on it; the image cache is keyed
by the caller's `u64`. Checked by `tests/canvas.rs` and the unchanged
comparison tests.

### T83.2. slintx-wgpu: text on parley

`Canvas::draw_layout` draws a `parley::Layout<Brush>` line by line through
`draw_glyph_run`, with no sharedparley. parley is a workspace dependency at
0.11.1, the version Slint 1.18.1 locks, so the adapter passes Slint's layouts
through unchanged. Checked by `tests/text.rs` (ink stays inside the layout,
covered pixels take the brush) and the comparison text frames.

### T83.3. slintx-wgpu: the i-slint-core adapter behind a feature

`src/adapter/` (`WgpuRenderer`, `Frame` as `ItemRenderer` over `Canvas`)
compiles only with `slint = ["dep:i-slint-core"]`; a self dev-dependency keeps
the comparison tests built. The license-check fixtures gain a host enabling the
feature (passes) and `feature-default`, a crate turning it on by default
(fails). The image cache also skips pixels larger than the device's
`max_texture_dimension_2d`, which would otherwise fail the whole frame.
