# Slint Bindings

- GitHub: https://github.com/listepo/slint-bindings
- Embed Slint UIs in native hosts (SwiftUI/AppKit on macOS, WinUI 3 on Windows) through a
  host-driven custom Slint platform and a C ABI; then render Weft UI trees with it.

## Cloud review findings (2026-10-08)

New bugs, dead code and moves from a read-only Cursor cloud review of `main` (agent `bc-1c31008c-5ecc-5e7e-9c99-50640249894e`; full report: `cloud/slint-bindings.md` in the private `listepo/roadmap` repo). They take ids T19–T40, ordered P0, P1, P2. **confirmed** means seen in the tree; **suspected** means plausible from the code but not proven (the WinUI host was not compiled). Swift files are under `swift/Sources/SlintBindings/` and C# files under `windows/SlintBindings.WinUI/`; line numbers are as of the review. None of these is in the task table or `todo.md` yet: to take one, add its row and card the usual way.

| ID | Priority | Kind | Status | Where | Fix |
| --- | --- | --- | --- | --- | --- |
| T19 | P0 | move | confirmed | `flutter/packages/slint/rust/embed` (`slint-embed`: platform, events, thread) → slint_dart's published crate, or a small repo / git dependency | This copy is the only Cargo path dependency on `flutter/`; upstream slint_dart still inlines the same code. Give `slint-embed` one home that both repos depend on. |
| T20 | P1 | bug | confirmed | `crates/slint-bindings-ffi/src/lib.rs:71-84` | The C ABI never calls the UI-thread guard (the Dart interpreter does), and `slint_embed::thread` is unused. Call `slint_embed::thread::check()` at the start of every `sb_*` and report through `sb_last_error`. |
| T21 | P1 | bug | confirmed | `swift/Sources/SlintBindings/SlintNSView.swift:79-81`; `windows/SlintBindings.WinUI/SlintPanel.cs:75-77`; `crates/slint-bindings-core/src/host.rs:218-221` | Every view calls `sb_tick()` once per vsync, and `sb_tick` advances global timers and animations, so N views tick N times per frame. Tick once per process/thread, then render each host (needed before T10). |
| T22 | P1 | bug | confirmed | `crates/slint-bindings-ffi/src/lib.rs:42-60`, `:229-236`; `swift/Sources/CSlintBindings/slint_bindings.h:37`; `SlintHost.cs:75` | `SbPointerButton` is taken as a Rust enum from C, while the header declares `uint8_t` and C# casts an `int`; an out-of-range value is UB. Take `u8` and `try_from` 0..=2. |
| T23 | P1 | bug | confirmed | `windows/SlintBindings.WinUI/SlintPanel.cs:43-45` | WinUI keyboard input is press-only text (`CharacterReceived` → `KeyPressed`; no `KeyReleased`, no `VirtualKey`). Map `PreviewKeyDown`/`KeyUp`, pair press and release, and do IME through `CoreTextEditContext` with T6. |
| T24 | P1 | bug | confirmed | `SlintPanel.cs:36-39` | The WinUI wheel passes `MouseWheelDelta` (WHEEL_DELTA units, usually 120) as `dy`, but Slint expects logical pixels, and `dx` is always 0. Scale it and read the horizontal wheel. |
| T25 | P1 | bug | confirmed | `SlintPanel.cs:48`, `:65-72` | WinUI ignores DPI / rasterization-scale changes (macOS handles them in `viewDidChangeBackingProperties`). Subscribe to `XamlRoot.Changed`. |
| T26 | P1 | bug | confirmed | `SlintPanel.cs:65-72`; `SlintHost.cs:45-46` | A WinUI resize allocates a new `WriteableBitmap` without painting, and `Host.Resize` can throw with no catch on the UI thread. Catch it, surface the last error, then render and `Invalidate()` at the end of `SyncSize`. |
| T27 | P1 | dead code | confirmed | `flutter/` (464 files; only `flutter/packages/slint/rust/embed` is used, `Cargo.toml:16`); `flutter/.github/workflows/ci.yml:6-8` never runs | Once T19 gives `slint-embed` its home, delete the rest of the `flutter/` tree, including its nested CI. |
| T29 | P2 | bug | confirmed | `crates/slint-bindings-ffi/src/lib.rs:295-321`; `SlintNSView.swift:157-164` | No key-repeat path: Slint 1.18.1 has `WindowEvent::KeyPressRepeated`, the FFI has only press/release and Swift ignores `isARepeat`. Add `sb_host_key_repeated`. |
| T30 | P2 | bug | confirmed | `crates/slint-bindings-ffi/src/lib.rs:95-100` | Invalid UTF-8 C strings silently become `""` (`to_str().unwrap_or_default()`). Return `InvalidArgument` through `sb_last_error`. |
| T31 | P2 | bug | confirmed | `crates/slint-bindings-core/src/platform.rs:17-18` | `InstallError::BufferMismatch` is reported as `ForeignPlatform`. Give it its own error. |
| T32 | P2 | bug | confirmed (churn); use-after-free suspected | `SlintDemoView.swift:31-36`; `SlintHost.swift:137-149` | `onSubmitted` is re-registered on every SwiftUI update, and the old box is released after registering. Register once in `makeNSView`. |
| T33 | P2 | bug | confirmed | `crates/slint-bindings-ffi/tests/e2e.rs:270-271` | The snapshot test passes when the PNG read fails (`read().ok()`, then `return`). Fail on I/O errors. |
| T34 | P2 | bug | confirmed | `SlintNSView.swift:120-121` | Middle-button drag is not forwarded (no `otherMouseDragged`). Route it like `mouseDragged`. |
| T35 | P2 | bug | suspected | `SlintNSView.swift:138-140` | AppKit wheel deltas without precise scrolling are lines, but they are forwarded raw. Multiply by the line height when `!hasPreciseScrollingDeltas`. |
| T36 | P2 | bug | suspected | `crates/slint-bindings-core/src/host.rs:277-279` | First-responder changes are sent as `WindowActiveChanged`, which Slint defines as window focus. Keep it as a documented approximation, or send it only when the host window activates. |
| T37 | P2 | bug | suspected | `crates/slint-bindings-ffi/src/lib.rs:203-211`, `:173`, `:364-368` | A reentrant `sb_host_render` from a callback would alias `&SbHost` and `&mut SbHost` while `submitted` runs inside dispatch. Document "no reentrancy", or put the host in a `RefCell`. |
| T38 | P2 | bug | suspected | `SlintPanel.cs:88-99` | WinUI never calls `CapturePointer`. Capture on press; release on up/cancel. |
| T40 | P2 | move | suspected | FFI `guard` / `sb_last_error` (`crates/slint-bindings-ffi/src/lib.rs:63-84`) → a shared helper | Only if weft-studio-ffi wants the same panic/error slot (about 30 lines; taken from weft's plan, not checked here). |

Already tracked here, not added again: the Metal/D3D12 texture backends should come from slint_dart's `slint-skia-ffi`, not a new copy; T7 and T8 already name slint_dart's `platform/metal.rs` and `platform/d3d.rs`.

Settled: T28 and T39. T12 and T16 were dropped (`done.md`); T70 and T71 host the Slint that weft's `weft-slint` generates instead.

Not added: `slint_embed::events` is used only by slint_dart and stays there; `i-slint-core` is a deliberate dependency (feature unification), not dead.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T4 | done | P1 | 3 | 100% | cursor |
| T5 | todo | P1 | 2 | 0% | |
| T6 | done | P1 | 3 | 100% | cursor |
| T7 | done | P1 | 4 | 100% | cursor |
| T8 | done | P1 | 5 | 100% | cursor |
| T9 | todo | P2 | 3 | 0% | |
| T10 | todo | P2 | 3 | 0% | |
| T13 | todo | P2 | 4 | 0% | |
| T14 | todo | P2 | 5 | 0% | |
| T15 | todo | P3 | 3 | 0% | |
| T17 | todo | P3 | 3 | 0% | |
| T73 | in progress | P0 | 4 | 0% | Claude / opus-5.5 |
| T73.1 | in progress | P0 | 4 | 0% | Claude / opus-5.5 |
| T73.2 | in progress | P0 | 2 | 90% | Claude / opus-5.5 |
| T74 | in progress | P0 | 3 | 0% | Claude / opus-5.5 |
| T75 | todo | P1 | 3 | 0% | |
| T76 | todo | P1 | 3 | 0% | |
| T77 | todo | P1 | 5 | 0% | |
| T78 | todo | P1 | 4 | 0% | |
| T79 | todo | P1 | 4 | 0% | |
| T80 | todo | P1 | 5 | 0% | |
| T81 | todo | P1 | 4 | 0% | |
| T82 | todo | P1 | 5 | 0% | |
| T83 | todo | P1 | 4 | 0% | |
| T84 | todo | P1 | 4 | 0% | |
| T85 | todo | P1 | 3 | 0% | |
| T86 | todo | P2 | 5 | 0% | |
| T87 | todo | P2 | 5 | 0% | |
| T88 | todo | P3 | 4 | 0% | |

## Milestones

Each milestone ends at a go/no-go checkpoint. A "no-go" stops the milestone's
remaining tasks until the creator decides how to proceed.

| Milestone | Goal | Tasks | Go/no-go checkpoint |
| --- | --- | --- | --- |
| M1: CPU spike | Pixels and input cross the boundary on macOS | T1 (done), T2 (done) | The demo runs at 60 fps at 2x scale for a 800×600 pt view with no visible input lag. No-go: frame time above 8 ms on Apple Silicon, which means go straight to M3. |
| M2: Both hosts, shared core | Same core on WinUI; no duplicated code with slint_dart | T3 (done), T4 (done), T5, T6 | WinUI demo builds and runs; text fields take typed text on both hosts; CI is green. |
| M3: GPU | GPU rendering into the host surface | T7, T8 | 4K view under 4 ms/frame; resize without tearing or black frames; no regressions in input. No-go on one OS keeps the CPU path there. |
| M4: Distribution | Consumable packages | T9 | An app outside this repo adds the package (SwiftPM binary target, NuGet) and runs the demo. |
| M5: Real components and Weft | Any `.slint` component, screens generated by Weft's `weft-slint`, and typed host APIs | T11, T13, T70, T71 | A screen generated by `weft-slint` (the login form) renders and takes input in both hosts, from a host built by another library. |
| M6: Platform depth | Multiple views, accessibility, iOS, web | T10, T14, T15, T17 | VoiceOver and Narrator read the embedded UI; iOS demo runs on a device. |
| M7a: slintx front end | Our own parser, module loader, name resolution, type checker and IR for the full Slint 1.18 language | T73, T74, T75, T76, T77, T78 | The parser round-trips every corpus file byte for byte, and slintx accepts or rejects every corpus file exactly as Slint does. |
| M7b: slintx preview | Interpreter, layouts and rendering through `slintx-wgpu`, natively and in `wasm32` | T79, T80, T81, T82, T83, T84, T85 | The T74 corpus, which includes files generated by penpotx's Slint emitter, renders natively and in a browser within the harness pixel budget against Slint. |
| M7c: slintx full surface | Input, std-widgets, hosts on slintx | T86, T87, T88 | The std-widgets gallery fixture matches Slint's behaviour in the harness, and the shipped host build contains no Slint crate. |

## Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Slint has no public API to render into a foreign `NSView`/`HWND` from Rust; only the custom-platform hook. | Every host integration is ours to maintain. | Keep the platform layer small and shared (T3); follow Slint PRs (`ComponentContainer`, C++ platform API, Swift PR #11167). |
| Accessibility: Slint exposes its tree to the OS through the winit backend (AccessKit); a custom platform gets none of it. | Embedded UI is invisible to screen readers. | T14 spike early in M6, earlier if a user needs it; decide between AccessKit adapters per host and a native mirror tree. |
| GPU interop is unstable API (`unstable-wgpu-*`, Skia internals in `i-slint-renderer-skia`). | Breaks on Slint minor releases. | Exact pins; one crate owns the GPU code; slint_dart's `slint-skia-ffi` already solves Metal and D3D12 textures (T3, T7, T8). |
| IME and keyboard mapping differ per host. | CJK input and shortcuts break. | Dedicated tasks (T4, T6) with table-driven key tests. |
| WinUI 3 cannot host a child `HWND` (XAML composition draws over it). | Only `SwapChainPanel` or bitmaps work. | Design for `SwapChainPanel` from the start (T8). |
| `isolated deinit` and Swift 6 strict concurrency move quickly. | Build breaks on new Xcode. | CI on the newest Xcode (T5). |
| slintx copies Slint's code by accident (clean room). | Apache-2.0 release of slintx is no longer possible. | Work from the docs only (`research.md` §8); settle open behaviour by black-box comparison in T74, never by reading Slint's sources. |
| A Slint crate leaks into the slintx dependency graph. | Same. | Licence gate in T73: the check fails when `slint`, `slint-*` or `i-slint-*` enters a slintx crate's normal dependencies. |
| Slint behaviour the docs leave open (`research.md` §8.4, OPEN items). | Previews differ from Slint. | Every OPEN item becomes a T74 harness case before the task that implements it closes. |

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

### T5. CI

GitHub Actions: `macos-26` (arm64) runs `just check`; `windows-latest` runs
`cargo build -p slint-bindings-ffi`, `cargo nextest run` and
`dotnet build windows/SlintDemo.WinUI`. Use the shared workflows from
`pyrlyn/ci` where they fit. Done when both jobs are required checks on `main`.

### T6. WinUI host builds and runs (M2)

Build the code-only skeleton on Windows and fix it; generate
`NativeMethods.cs` from the C header (evaluate csbindgen versus a small
cbindgen-driven generator) so it cannot drift; let the core render BGRA
directly so the C# swizzle goes away; map `VirtualKey` to Slint key text and
route `CharacterReceived`; IME via `CoreTextEditContext`. Done when the WinUI
demo behaves like the macOS one (T2 checklist).

Execution: the WinUI → Slint map lives in `slint-bindings-core` next to the
AppKit map (one table, tested on every `Key`) and the host calls
`sb_virtual_key_command` / `sb_virtual_key_text`. `CoreTextEditContext` updates
go through `sb_host_ime_*`, which keeps only the preedit and commits it into
the field; `GetForCurrentView` failing (Windows 10 desktop, no CoreWindow)
leaves `CharacterReceived` as the text path. `sb_host_render_bgra` writes
BGRA8. `windows/Directory.Build.targets` copies the DLL for the active RID
(`just windows-dll`, or `just windows-dll aarch64-pc-windows-msvc`) into the
output and into `runtimes/<rid>/native`. `just pinvoke-check` diffs `sb_*`
names against `NativeMethods.cs`; marshalling stays hand-written because
csbindgen is not a dependency. The demo window has not been launched.

### T7. GPU path on macOS (M3)

Two candidates, measured against the CPU path:
(a) Skia on Metal through `i-slint-renderer-skia`, reusing slint_dart's
`platform/metal.rs` (renders into an `MTLTexture` over an IOSurface-backed
`CVPixelBuffer`), presented through a `CAMetalLayer` or as IOSurface layer
contents; (b) `FemtoVGWGPURenderer::render_to_texture` (public, `slint::platform::femtovg_renderer`)
with a wgpu Metal surface created from the view's layer
(`SurfaceTargetUnsafe::CoreAnimationLayer`). Prefer (a) if T3 lands, since the
code exists. Done when the M3 checkpoint numbers are met on macOS and live
resize shows no stretched or black frames (`presentsWithTransaction` during resize).

Execution: FemtoVG on wgpu (`renderer-femtovg-wgpu`, `unstable-wgpu-30`), not
Skia. `i-slint-renderer-skia` compiles all of Skia and is not a public renderer.
The GPU window is staged through `slint_embed::platform::set_next_window`, so
CPU and GPU hosts share the one software platform. `sb_demo_new_metal` takes a
`CAMetalLayer` sublayer (the view's own layer is left alone). Live resize sets
`presentsWithTransaction`. A failed device or surface falls back to
`sb_demo_new`. The 4K timing checkpoint was not measured.

### T8. GPU path on Windows (M3)

Replace the `Image` with a `SwapChainPanel`. Candidates: (a) wgpu D3D12 surface
from the panel (`SurfaceTargetUnsafe::SwapChainPanel`, wgpu PR #4191) plus
`FemtoVGWGPURenderer`; (b) Skia on D3D12 rendering into a shared texture
(slint_dart `platform/d3d.rs`) copied into a composition swap chain that the
host binds with `ISwapChainPanelNative::SetSwapChain` on the UI thread.
Handle `CompositionScaleChanged` for DPI. Done when the M3 checkpoint holds on
Windows.

Execution: the same FemtoVG wgpu renderer. `sb_demo_new_swapchain` takes
`ISwapChainPanelNative`; wgpu creates the DXGI swap chain and calls
`SetSwapChain` on the UI thread. `CompositionScaleChanged` resizes the panel.
If the COM pointer or the adapter fails, the panel keeps the `WriteableBitmap`.
The frame-time checkpoint was not measured, and the window was not launched.

### T9. Packaging (M4)

macOS/iOS: build `libslint_bindings_ffi.a` for `aarch64-apple-darwin`,
`aarch64-apple-ios` and `aarch64-apple-ios-sim`, wrap with the header and
module map into `SlintBindings.xcframework`, switch `Package.swift` to a
`binaryTarget` and attach the zip to GitHub releases. Windows: NuGet package
with `runtimes/win-x64/native` and `runtimes/win-arm64/native` DLLs plus the C#
control; MSIX for the demo app. Done when a fresh app consumes each package
from a release.

### T10. Multiple hosts and popups

`slint::platform::set_platform` is once per thread, and each component gets its
own `MinimalSoftwareWindow`. Verify two `SlintNSView`s in one window and two
windows; make Slint `PopupWindow` and menus render inside the host view (no
native child windows) and clip correctly. Done with a demo showing two views
and a combo-box popup in each.

### T13. Typed host APIs

Generate Swift (`@Observable` model with two-way property sync through Slint
`changed` callbacks) and C# (INotifyPropertyChanged) wrappers from a `.slint`
file, the way slint_dart's `slint_generator` produces Dart. Done when the demo
uses only generated wrappers and SwiftUI edits and Slint edits stay in sync.

### T14. Accessibility spike

The custom platform exposes no accessibility tree. Prototype exporting Slint's
item tree through AccessKit adapters inside the host view
(`accesskit_macos` subclassing the `NSView`, `accesskit_windows` for UIA on the
panel's window), or a native mirror tree. Done with a recommendation and a
VoiceOver demo reading the form.

### T15. iOS host

`UIView`/`UIViewRepresentable` host over the same core (touch events,
safe-area insets, keyboard avoidance, `CADisplayLink`). Slint's own iOS port
runs whole apps through winit; this is the embedded variant. Done when the demo
form runs inside a SwiftUI iOS app on a device.

### T17. Web build

Compile the core to `wasm32-unknown-unknown` with the software renderer
drawing into a canvas (`ImageData`), or Slint's own web backend, and render Weft
trees in the browser through the existing Weft WASM tooling. Done when the
login sample renders in a browser from its `.weft` file.

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

### T73.1. slintx-syntax crate: lexer, lossless tree, parser, typed AST

`crates/slintx-syntax`: syntax kinds, a hand-written lexer (nested block
comments, `-` in identifiers, unit suffixes, string interpolation), a rowan
tree built by a recursive-descent parser with error recovery, typed AST views,
and diagnostics with byte spans. Own fixtures under
`crates/slintx-syntax/tests/fixtures/`. Done when round-trip, span and
error-recovery tests pass on those fixtures and the crate has no dependency on
`slint`, `slint-*` or `i-slint-*`.

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

### T74. slintx-conformance: differential harness against Slint

An unpublished crate (`publish = false`) that runs each `.slint` fixture
through slintx and through Slint (`slint-interpreter` and the software
renderer, test-only) and compares: accept or reject and diagnostic positions
first, then property values after scripted steps, layout geometry and pixels
(reusing `pixel-diff`). The corpus is written here from the docs and from
penpotx emitter outputs; every OPEN item in `research.md` §8.4 gets a case.
Slint is the oracle only and never a dependency of a shipped crate. Done when
the harness runs in `just check` with the parser-level comparison live and the
later comparisons stubbed behind the stages that do not exist yet.

### T75. Modules, imports and exports

Load a file graph: relative and include-path imports, `@lib` library paths,
export lists and re-exports, duplicate and cyclic import errors, and a virtual
file system so `wasm32` builds take sources from memory. The std-widgets import
resolves to sources bundled in the crate. Done when import and export cases in
the harness match Slint.

### T76. Name resolution

Scopes and lookup per `research.md` §8.4.1: element ids, `self`, `parent`,
`root`, properties of enclosing elements, globals, built-in namespaces
(`Colors`, `Math`, `Key`, `Platform`, ...), enum short names by expected type,
and `-`/`_` identifier equivalence. Done when the resolution cases in the
harness match Slint, including its errors.

### T77. Type checker

Types and units, implicit conversions, structs with field defaults, enums,
arrays and models, operator typing and precedence errors, purity, property
visibility and assignability, two-way binding rules, and binding loops found
at compile time. Diagnostics carry spans from T73. Done when every corpus file
gets the same accept/reject verdict as Slint (the M7a checkpoint).

### T78. Lowering to an IR

Turn checked files into an IR the interpreter runs: the built-in element
catalog (every element, property, callback and function in §8.4.3), component
inheritance, `@children`, `for` and `if`, states and transitions as bindings,
animations, callback aliases and layout nodes. Done when every corpus file
lowers without error and IR snapshot tests cover each construct.

### T79. Expression evaluator

Values, operators, statements, and every built-in function and macro in
§8.4.1 (math, string, color, array and model functions, `@tr`, `@image-url`,
`@markdown`, `@keys`), including number-to-string formatting. Done when the
harness compares evaluated values with Slint for each built-in.

### T80. Reactive property runtime

Lazy bindings with dependency tracking, dirtying, two-way links, `changed`
handlers, runtime binding-loop detection, animations (all easings including
`cubic-bezier` and `spring`) on an injectable clock, and `Timer`. Done when
the harness value cases with scripted time steps match Slint.

### T81. Component instances and public API

Instantiate component trees: repeaters over models (including `push`,
`remove`, `insert`), conditionals, globals, callbacks and `init` order. Public
API: compile, instantiate, set and get properties, invoke callbacks and set
handlers, with JSON values in the shape T11 uses. Done when the harness drives
the same scripts through slintx and Slint with equal results.

### T82. Layout engine

`HorizontalLayout`, `VerticalLayout`, `GridLayout` and `FlexboxLayout` per
§8.4.3: min/preferred/max/stretch, percentages, padding, spacing, alignment,
`cross-axis-self-alignment`, `layout-order`, implicit sizes and text measuring
with parley. Try taffy for `FlexboxLayout` and keep it only if its results match
Slint. Done when the harness geometry cases match Slint.

### T83. slintx-wgpu without i-slint-core

`slintx-wgpu` (T72) takes Slint's renderer traits, `Brush`/`Color` and
`sharedparley` text from `i-slint-core`. Give it its own scene input types and
parley-based text, and keep the `i-slint-core` adapter behind a feature for
today's hosts. Starts after T72 is on `main`, agreed with T72's owner. Done when
the crate builds without `i-slint-core` and T72's comparison tests still pass
through the adapter.

### T84. Visual items on slintx-wgpu

Draw slintx item trees through `slintx-wgpu`: `Rectangle` (borders, per-corner
radius, shadows, clip), `Text`, `StyledText`, `Image` (fit, nine-slice, tiling,
colorize), `Path`, opacity, transforms and `Window`. Done when the harness pixel
cases match Slint's software renderer within the budget.

### T85. wasm32 build (slintx-web)

A `slintx-web` crate (wasm-bindgen) that compiles `.slint` from memory and
renders into a canvas through WebGPU with WebGL fallback, with a small JS API
(load sources, set and get properties, resize) and a binary size budget.
Consumers such as penpotx's live preview build on this API in their own
projects. Done when the T74 corpus renders in a headless browser test within
the pixel budget (the M7b checkpoint).

### T86. Input and interactive items

Event routing, `TouchArea`, `FocusScope`, the focus chain, `TextInput`
(editing, selection, IME), `Flickable`, gesture handlers, `DragArea` and
`DropArea`, `PopupWindow`, `ContextMenuArea`, menus and tooltips. Done when
scripted input cases in the harness match Slint.

### T87. std-widgets

Our own `.slint` widget library with the documented API of every widget in
§8.4.4, `Palette` and `StyleMetrics`, in one style with light and dark
variants. Visuals are authored here; Slint's style sources are not used. Done
when a gallery fixture with every widget passes the harness behaviour cases.

### T88. Hosts on slintx

Run the slint-bindings hosts' runtime components (T11's `sb_compile` path) on
slintx so the shipped host build drops `slint`, `slint-interpreter` and
`i-slint-*`. Done when the demo runs on both hosts and the licence gate covers
the FFI crate.
