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
| T14 | in progress | P2 | 5 | 10% | Claude / opus-5.5 |
| T15 | todo | P3 | 3 | 0% | |
| T17 | todo | P3 | 3 | 0% | |

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

## Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Slint has no public API to render into a foreign `NSView`/`HWND` from Rust; only the custom-platform hook. | Every host integration is ours to maintain. | Keep the platform layer small and shared (T3); follow Slint PRs (`ComponentContainer`, C++ platform API, Swift PR #11167). |
| Accessibility: Slint exposes its tree to the OS through the winit backend (AccessKit); a custom platform gets none of it. | Embedded UI is invisible to screen readers. | T14 spike early in M6, earlier if a user needs it; decide between AccessKit adapters per host and a native mirror tree. |
| GPU interop is unstable API (`unstable-wgpu-*`, Skia internals in `i-slint-renderer-skia`). | Breaks on Slint minor releases. | Exact pins; one crate owns the GPU code; slint_dart's `slint-skia-ffi` already solves Metal and D3D12 textures (T3, T7, T8). |
| IME and keyboard mapping differ per host. | CJK input and shortcuts break. | Dedicated tasks (T4, T6) with table-driven key tests. |
| WinUI 3 cannot host a child `HWND` (XAML composition draws over it). | Only `SwapChainPanel` or bitmaps work. | Design for `SwapChainPanel` from the start (T8). |
| `isolated deinit` and Swift 6 strict concurrency move quickly. | Build breaks on new Xcode. | CI on the newest Xcode (T5). |

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
2. macOS: `accesskit_macos::SubclassingAdapter` on the host's `NSView`, pushed
   from the host after each render; `sb_host_accessibility_attach` in the FFI
   (header regenerated, `NativeMethods.cs` stub), `SlintNSView` attaches.
   Swift test walks `accessibilityChildren()` of the view and finds the
   form's text field and button.
3. Demo: run `just swift-run`, read the form with VoiceOver (or, without
   VoiceOver, through the AX API); record the steps and the result.
4. Recommendation (AccessKit per host vs native mirror tree, Windows plan) in
   this card, then `done.md`. Slint's winit backend stays out of the build.

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
