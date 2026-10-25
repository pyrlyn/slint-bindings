# Slint Bindings

- GitHub: https://github.com/listepo/slint-bindings
- Embed Slint UIs in native hosts (SwiftUI/AppKit on macOS, WinUI 3 on Windows) through a
  host-driven custom Slint platform and a C ABI; then render the UI description format UI trees with it.

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
| T40 | P2 | move | suspected | FFI `guard` / `sb_last_error` (`crates/slint-bindings-ffi/src/lib.rs:63-84`) → a shared helper | Only if weft-studio-ffi wants the same panic/error slot (about 30 lines; taken from the UI description format's plan, not checked here). |

Already tracked here, not added again: the Metal/D3D12 texture backends should come from slint_dart's `slint-skia-ffi`, not a new copy; T7 and T8 already name slint_dart's `platform/metal.rs` and `platform/d3d.rs`.

Settled: T28 and T39. T12 and T16 were dropped (`done.md`); T70 and T71 host the Slint that UI description format's `weft-slint` generates instead.

Not added: `slint_embed::events` is used only by slint_dart and stays there; `i-slint-core` is a deliberate dependency (feature unification), not dead.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T9 | todo | P2 | 3 | 0% | |
| T13 | todo | P2 | 4 | 0% | |
| T14 | in progress | P2 | 5 | 90% | Claude / opus-5.5 |
| T15 | todo | P3 | 3 | 0% | |
| T17 | todo | P3 | 3 | 0% | |
| T76 | todo | P1 | 3 | 0% | |
| T77 | todo | P1 | 5 | 0% | |
| T78 | todo | P1 | 4 | 0% | |
| T79 | todo | P1 | 4 | 0% | |
| T80 | todo | P1 | 5 | 0% | |
| T81 | todo | P1 | 4 | 0% | |
| T82 | todo | P1 | 5 | 0% | |
| T84 | todo | P1 | 4 | 0% | |
| T85 | todo | P1 | 3 | 0% | |
| T86 | todo | P2 | 5 | 0% | |
| T87 | todo | P2 | 5 | 0% | |
| T88 | todo | P3 | 4 | 0% | |

### T91. Project review and regression fixes

Status: in progress (implementation verified, awaiting review). Priority: P1.
Complexity: 4. Readiness: 100%. Agent: OpenAI / GPT-6.
Branch: `codex/review-20261024`, based on `907c42c`.

PR #59 CI follow-up: Windows exposed a platform-specific path assertion in
the module loader test. Compare paths as Path values, rerun module tests and
the PR checks, then merge after successful validation. The shared CI cancels
the run after a failed test, so cancelled job steps must also be inspected.

Execution: review the host/FFI, renderer, image utilities and compiler/test
boundaries; run the workspace baseline; reproduce confirmed failures; apply
narrow fixes; verify Rust, Swift, wasm and ABI/dependency gates. Preserve other
sessions' work and dependency pins.

Confirmed and fixed:

- P1: reentrant FFI resize/render could construct an exclusive host reference
  while input dispatch still held a shared one. Use checked backend borrows;
  reject host destruction during active calls, including callbacks.
- P1: size/GPU queries and destructors bypassed the UI-thread/panic guard.
  Guard them, preserving an earlier constructor diagnostic after successful
  cleanup. A rejected free leaves ownership with the original thread.
- P1: excessive CPU dimensions panicked after changing host geometry; GPU
  dimensions were submitted without checking the device limit. Validate buffer
  arithmetic, allocation and finite logical sizes before publishing geometry.
- P2: UTF-16 offsets inside a final surrogate pair were accepted by the IME
  replacement path. Reject them and preserve the preedit and selection.
- P2: glyphs too large for the atlas grew it from 1 MiB to 16 MiB and marked it
  full, causing repeated cache resets. Reject them before growth, keeping useful
  entries. Genuine capacity exhaustion still clears on the next frame.
- Performance: WinUI creates a PixelBuffer stream only when a CPU frame changed.

Reuse: retained the shared slint-embed platform, existing scale validation,
FFI guard, IME conversion and glyph allocator. Used standard-library RefCell and
fallible Vec allocation; no new project dependency or parallel implementation.
The old checkout's pixel-diff decoder is no longer shipped: current main uses
shared rgba-png. No change was made to that other repository.

Validation: the baseline passed 188 tests. Three new regressions failed before
fixes (IME boundary, atlas growth, impossible resize). Final workspace nextest:
194 passed, 7 platform tests skipped. Swift build and all 20 tests passed; wasm
build and all 45 tests passed. Formatting, clippy with warnings denied, renderer
and licence gates (including gate fixtures), header regeneration/drift,
P/Invoke export names and Swift package linkage checks passed.

Limits: WinUI was inspected but not built/run on Windows. The seven ignored
converter platform tests (external browser/GTK/Xcode/simulator environments)
were not run. Vendored Flutter app/package suites were not run; its embedding
and interpreter crates were exercised through the workspace host tests.
No performance benchmark was used to claim an FPS improvement.

## Milestones

Each milestone ends at a go/no-go checkpoint. A "no-go" stops the milestone's
remaining tasks until the creator decides how to proceed.

| Milestone | Goal | Tasks | Go/no-go checkpoint |
| --- | --- | --- | --- |
| M1: CPU spike | Pixels and input cross the boundary on macOS | T1 (done), T2 (done) | The demo runs at 60 fps at 2x scale for a 800×600 pt view with no visible input lag. No-go: frame time above 8 ms on Apple Silicon, which means go straight to M3. |
| M2: Both hosts, shared core | Same core on WinUI; no duplicated code with slint_dart | T3 (done), T4 (done), T5 (done), T6 | WinUI demo builds and runs; text fields take typed text on both hosts; CI is green. |
| M3: GPU | GPU rendering into the host surface | T7, T8 | 4K view under 4 ms/frame; resize without tearing or black frames; no regressions in input. No-go on one OS keeps the CPU path there. |
| M4: Distribution | Consumable packages | T9 | An app outside this repo adds the package (SwiftPM binary target, NuGet) and runs the demo. |
| M5: Real components and the UI description format | Any `.slint` component, screens generated by the UI description format's `weft-slint`, and typed host APIs | T11, T13, T70, T71 | A screen generated by `weft-slint` (the login form) renders and takes input in both hosts, from a host built by another library. |
| M6: Platform depth | Multiple views, accessibility, iOS, web | T10 (done), T14, T15, T17 | VoiceOver and Narrator read the embedded UI; iOS demo runs on a device. |
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

### T9. Packaging (M4)

macOS/iOS: build `libslint_bindings_ffi.a` for `aarch64-apple-darwin`,
`aarch64-apple-ios` and `aarch64-apple-ios-sim`, wrap with the header and
module map into `SlintBindings.xcframework`, switch `Package.swift` to a
`binaryTarget` and attach the zip to GitHub releases. Windows: NuGet package
with `runtimes/win-x64/native` and `runtimes/win-arm64/native` DLLs plus the C#
control; MSIX for the demo app. Done when a fresh app consumes each package
from a release.

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

Execution: research first (`research.md`, "Accessibility"): what Slint 1.18.1
gives a custom platform (`i_slint_core::accessibility`, `ItemRc` accessible
properties, `WindowInner` focus and popups) and how `i-slint-backend-winit`'s
`accesskit.rs` maps them; current AccessKit crates. Then:

1. `slint-bindings-core::a11y`: build an `accesskit::TreeUpdate` from a
   component's window (role, label, value, bounds in view coordinates,
   focus, actions) and turn an `accesskit::ActionRequest` into a Slint action
   (focus, default, set value, increment, decrement), deferred to the next
   `sb_tick` the way the winit backend does. Platform-neutral, tested with
   nextest on the demo form (tree shape, labels, focus, a click and a
   set-value action reaching the component).
2. macOS: `accesskit_macos::Adapter` behind the host's `NSView` (the view
   forwards `accessibilityChildren`, `accessibilityFocusedUIElement` and
   `accessibilityHitTest:`), pushed from the host after each render; `sb_host_accessibility_attach` in the FFI
   (header regenerated, `NativeMethods.cs` stub), `SlintNSView` attaches.
   Swift test walks `accessibilityChildren()` of the view and finds the
   form's text field and button.
3. Demo: run `just swift-run`, read the form with VoiceOver (or, without
   VoiceOver, through the AX API); record the steps and the result.
4. Recommendation (AccessKit per host vs native mirror tree, Windows plan) in
   this card, then `done.md`. Slint's winit backend stays out of the build.

Result: `slint-bindings-core::a11y` builds the AccessKit tree (6 nextest
tests); `SlintNSView` serves it to VoiceOver through `accesskit_macos`
(Swift suite `Accessibility`: the heading, the labelled field and the button
are found with their frames, pressing and setting a value reach the
component, focus follows Slint). Left: the creator listens to the demo with
VoiceOver on (Cmd-F5, `just swift-run`, VO-arrows over "Sign in", "Name",
"Continue"); the agent may not toggle VoiceOver and this environment blocks
out-of-process AX queries. The code is about 790 lines over the 500-line task
limit, committed as two commits (core tree; macOS bridge) for two PRs.

Recommendation:

- One platform-neutral AccessKit `TreeUpdate` builder in the core, shared by
  every host.
- macOS: `accesskit_macos::Adapter` behind the host view, as built here.
- Windows (WinUI 3): not `accesskit_windows`, which answers `WM_GETOBJECT`
  for the whole `HWND` and would hide XAML's own UIA tree. Instead a custom
  `AutomationPeer` on `SlintPanel` (`OnCreateAutomationPeer`,
  `GetChildrenCore`) mirroring the core tree through the C ABI (flat node
  list plus an action call). A follow-up task, with Narrator as the check.
- Known gaps: full rebuild per redrawn frame; no text runs, caret or
  selection; structure changes seen only through redraws (Slint's
  `WindowAdapterInternal` hooks are hidden); `accesskit_macos` panics if
  AppKit asks for a frame while the view has no window; landmark roles map to
  generic containers.

### T15. iOS host

`UIView`/`UIViewRepresentable` host over the same core (touch events,
safe-area insets, keyboard avoidance, `CADisplayLink`). Slint's own iOS port
runs whole apps through winit; this is the embedded variant. Done when the demo
form runs inside a SwiftUI iOS app on a device.

### T17. Web build

Compile the core to `wasm32-unknown-unknown` with the software renderer
drawing into a canvas (`ImageData`), or Slint's own web backend, and render the UI description format
trees in the browser through the existing the UI description format WASM tooling. Done when the
login sample renders in a browser from its `.weft` file.

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
