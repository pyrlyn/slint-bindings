# Research: embedding Slint in SwiftUI/AppKit and WinUI 3

All sources checked on 2026-10-07 against Slint 1.18.1 unless noted. Facts backed
only by blogs, forum threads or other people's issues are marked **unverified**.

## 1. Official bindings: verdict

**Slint has no official Swift/SwiftUI/AppKit or WinUI/C# integration.**

- Official language APIs in the repository: `api/cpp`, `api/node`, `api/python`,
  `api/rs`, `api/slint-sc`, `api/wasm-interpreter`. No Swift, no .NET.
  ([api/](https://github.com/slint-ui/slint/tree/master/api), master)
- iOS: Rust-only "tech preview" since 1.12, through the winit backend and the Skia
  renderer; whole apps, not views embedded in a Swift app.
  ([iOS guide](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios/),
  [1.12 release](https://slint.dev/blog/slint-1.12-released)). C++ can target iOS
  since PR [#13580](https://github.com/slint-ui/slint/pull/13580) (merged 2026-10-05, unreleased).
- Swift bindings: open **draft** PR [#11167](https://github.com/slint-ui/slint/pull/11167)
  "Swift Bridge Implementation" (+8499 lines, community author, label `language-binding`,
  closes issue [#1005](https://github.com/slint-ui/slint/issues/1005)). The maintainer asked
  to discuss design first because of its size; not merged as of 2026-10-07. A separate
  community proof of concept: [illucid-matthew/slint-swift-bindings](https://github.com/illucid-matthew/slint-swift-bindings).
- C#/.NET: issue [#4207](https://github.com/slint-ui/slint/issues/4207) (label `roadmap`) is
  open; the upstream PR [#3913](https://github.com/slint-ui/slint/pull/3913) was closed as
  stale. Third-party alpha: [microhobby/slint-dotnet](https://github.com/microhobby/slint-dotnet)
  (NuGet `SlintDotnet` 1.7.1). Neither embeds into WinUI.
- Embedding into a foreign native window from Rust: no documented API. Discussion
  [#6585](https://github.com/slint-ui/slint/discussions/6585) (parent HWND / NSView) has no
  official answer beyond a pointer to a third-party plugin project; issue
  [#4640](https://github.com/slint-ui/slint/issues/4640) (the reverse: native window inside
  Slint) is an open RFC.

## 2. What Slint offers for embedding

| Mechanism | Status | Use here |
| --- | --- | --- |
| `slint::platform::Platform` + `WindowAdapter` + `set_platform` ([docs](https://docs.rs/slint/1.18.1/slint/platform/index.html)) | Stable | The core: the host owns the loop, forwards `WindowEvent`s, calls `update_timers_and_animations` |
| `SoftwareRenderer` / `MinimalSoftwareWindow` ([docs](https://docs.rs/slint/1.18.1/slint/platform/software_renderer/index.html)) | Stable | M1: render into a host-owned buffer (`PremultipliedRgbaColor` is `repr(C)`) |
| `FemtoVGWGPURenderer::render_to_texture[_view]` ([docs](https://docs.rs/slint/1.18.1/slint/platform/femtovg_renderer/struct.FemtoVGWGPURenderer.html)) | Public in a custom platform; needs `unstable-wgpu-*` types | M3 candidate: render into a wgpu texture we present |
| `unstable-wgpu-29/30`, `BackendSelector::require_wgpu_30`, `Image::try_from(wgpu::Texture)` ([docs](https://docs.rs/slint/1.18.1/slint/wgpu_30/index.html)) | Unstable, versioned by wgpu major | Sharing a wgpu device with the host renderer |
| `raw-window-handle-06`, `Window::window_handle()` ([features](https://docs.rs/slint/1.18.1/slint/docs/cargo_features/index.html)) | Stable feature | Only for Slint-owned windows (winit); not for embedding into ours |
| C++ `slint::platform::SkiaRenderer` + `NativeWindowHandle::from_appkit/from_win32` ([header](https://github.com/slint-ui/slint/blob/507b650e/api/cpp/include/slint-platform.h), [example](https://github.com/slint-ui/slint/tree/master/examples/cpp/platform_native)) | Public C++ API | Proof that rendering into a foreign `NSView`/`HWND` works; Rust has no public `SkiaRenderer` equivalent (`i-slint-renderer-skia` is internal) |
| `ComponentContainer` / `ComponentFactory` ([docs](https://snapshots.slint.dev/master/docs/slint/guide/experimental/component-container/), [#2390](https://github.com/slint-ui/slint/issues/2390)) | Experimental, Rust only | Slint-in-Slint composition, not native embedding |
| Backends: winit, Qt, linuxkms, android; renderers femtovg, skia, software, experimental vello ([features](https://docs.rs/slint/1.18.1/slint/docs/cargo_features/index.html), [1.18 release](https://slint.dev/blog/slint-1.18-released)) | — | We use none of the backends; we are the backend |

Threading: "the event loop must run in the main thread, in most backends, and all the
components must be created in the same thread" ([crate docs](https://docs.rs/slint/1.18.1/slint/index.html)).
With a custom platform there is no Slint event loop; the host's UI thread plays that role.

Accessibility: the `accessibility` feature exposes the tree to the OS through the
built-in backends ([features](https://docs.rs/slint/1.18.1/slint/docs/cargo_features/index.html));
a custom platform has to provide it itself (plan T14).

## 3. Prior art in this workspace (reuse first)

- `packages/slint_dart`: `slint-dart-core` (thread-affinity guard, FFI event encoding →
  `WindowEvent`), `slint-interpreter-ffi` (software renderer → RGBA frames),
  `slint-skia-ffi` (Skia → Metal texture over an IOSurface `CVPixelBuffer` on Apple, D3D12
  shared texture + NT handle on Windows, EGL elsewhere). Checked at commit `12de634`.
- `packages/slint-flutter`: `native/rust/embedded.rs`, the same host-driven software path
  for Dart/Flutter. Checked at commit `4807387`.
- `apps/weft`: `weft-core`, `weft-catalog`, `weft-swiftui` crates for the Weft milestone.

## 4. Analogous projects

- egui in SwiftUI through wgpu on a `CAMetalLayer`-backed `NSView`, `NSViewRepresentable`,
  swift-bridge, XCFramework: [alex566/swiftui-egui-demo](https://github.com/alex566/swiftui-egui-demo)
  and its article (**unverified** details).
- wgpu Metal surfaces from a layer, and why not to replace an `NSView`'s own layer:
  [wgpu #6107](https://github.com/gfx-rs/wgpu/commit/fb0cb1eb11663f8de023f6dd64128ed1f5342ec7),
  [raw-window-metal](https://docs.rs/raw-window-metal/latest/raw_window_metal/).
- wgpu on a WinUI 3 `SwapChainPanel`: `SurfaceTargetUnsafe::SwapChainPanel`
  ([PR #4191](https://github.com/gfx-rs/wgpu/pull/4191),
  [surface.rs](https://docs.rs/wgpu/latest/src/wgpu/api/surface.rs.html)).
- Godot's proposal for the same WinUI embedding problem:
  [godot-proposals #14848](https://github.com/godotengine/godot-proposals/issues/14848) (**unverified**).

## 5. Host facts

### macOS

- Layer-backed `NSView` with CGImage contents is the lowest-risk CPU path; a flipped view
  matches Slint's top-left origin. `NSView.displayLink(target:selector:)` (macOS 14) ticks
  per display refresh.
- GPU path: `CAMetalLayer` (wgpu `CoreAnimationLayer` target) or an IOSurface texture
  rendered by Skia (slint_dart).

### Windows

- WinUI 3 has no supported way to host a child `HWND`; XAML composition draws over it
  ([WindowsAppSDK discussion #3879](https://github.com/microsoft/WindowsAppSDK/discussions/3879), **unverified** maintainer-adjacent answer).
- `SwapChainPanel` + `ISwapChainPanelNative::SetSwapChain`, which must be called on the
  panel's UI thread; Microsoft recommends at most four swap chains per app
  ([SetSwapChain](https://learn.microsoft.com/en-us/windows/win32/api/windows.ui.xaml.media.dxinterop/nf-windows-ui-xaml-media-dxinterop-iswapchainpanelnative-setswapchain),
  [SwapChainPanel](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.swapchainpanel?view=windows-app-sdk-1.7)).
- Windows App SDK on NuGet: 2.5.1 is the latest stable
  ([NuGet API](https://api.nuget.org/v3-flatcontainer/microsoft.windowsappsdk/index.json)).

## 6. FFI choices

| Option | For | Against | Decision |
| --- | --- | --- | --- |
| Hand-written C ABI + cbindgen header | One ABI for Swift (module map) and C# (P/Invoke); no runtime; full control over threading and buffers | Wrappers written by hand per language | **M1–M4** |
| UniFFI ([repo](https://github.com/mozilla/uniffi-rs), [Swift guide](https://mozilla.github.io/uniffi-rs/next/swift/overview.html)) | Generated Swift/Kotlin, C# via third party; already used by cox | Object model and per-call overhead are a poor fit for frame buffers and raw surface pointers; Swift 6 support partial per its docs | Revisit for the typed API (T13) |
| swift-bridge | Idiomatic Swift both ways | Swift only | No |
| Swift C++ interop over Slint's C++ API | Reuses `slint-platform.h`, `SkiaRenderer` | Drags CMake and the C++ SDK into SwiftPM; the community PoC hit `extern "C"` import limits | No |
| csbindgen / generated P/Invoke | Keeps C# in sync with the header | Extra tool | T6 |

## 7. Recheck for weft T68 (2026-10-15)

Section 1 still holds. Checked on 2026-10-15 through the GitHub API and the
crates.io API (slint-ui/slint `master` at `6ba819d`):

- The latest `slint` is still 1.18.1 (published 2026-09-21); there is no 1.19
  ([crates.io API](https://crates.io/api/v1/crates/slint),
  [releases](https://github.com/slint-ui/slint/releases)).
- `api/` holds `cpp`, `node`, `python`, `rs`, `slint-sc` and `wasm-interpreter`;
  `internal/backends` holds `android-activity`, `linuxkms`, `qt`, `selector`,
  `testing` and `winit`. There is no Swift, AppKit, .NET or WinUI binding or backend
  ([api/](https://github.com/slint-ui/slint/tree/master/api)).
- The Swift PR [#11167](https://github.com/slint-ui/slint/pull/11167) is still an open
  draft (last updated 2026-08-21). Issues [#1005](https://github.com/slint-ui/slint/issues/1005)
  (Swift) and [#4207](https://github.com/slint-ui/slint/issues/4207) (C#) are open.
- No open or recently merged PR adds Swift, AppKit, WinUI or .NET support. The nearest
  platform work is C++ only: [#13193](https://github.com/slint-ui/slint/pull/13193)
  (C++ platform API: native menus and input methods, open) and
  [#14047](https://github.com/slint-ui/slint/pull/14047) ("core: Let several contexts
  share one platform", merged 2026-10-14, unreleased), which bears on T10.
- Discussion [#6585](https://github.com/slint-ui/slint/discussions/6585) (attach to a
  parent `HWND` / `NSView`) has no chosen answer; issue
  [#4640](https://github.com/slint-ui/slint/issues/4640) is unchanged.
- C++ on iOS ([#13580](https://github.com/slint-ui/slint/pull/13580)) is merged but in
  no release yet.

So weft consumes these bindings rather than official ones. Weft's T67 converter
(`weft-slint`) produces plain `.slint` with `std-widgets` and a
`perform(action, id)` callback; the hosts compile it at run time (T11) and show
it (T70 on macOS, T71 on Windows).

## 8. Accessibility (T14, 2026-10-20)

Sources: the published crate sources from crates.io (the same files as the
`v1.18.1` tag of [slint-ui/slint](https://github.com/slint-ui/slint/tree/v1.18.1)),
the crates.io API and docs.rs, all checked 2026-10-20.

### What Slint 1.18.1 gives a custom platform

- No public accessibility API. Everything a screen-reader bridge needs is in
  `i-slint-core` 1.18.1, which this repo already depends on with an exact pin:
  - `accessibility.rs`: `AccessibleStringProperty` (line 19),
    `AccessibilityAction` (52), `accessible_descendents` (84),
    `nearest_accessible_item` (128), `find_text_input` (137)
    ([source](https://github.com/slint-ui/slint/blob/v1.18.1/internal/core/accessibility.rs)).
  - `item_tree.rs`: `ItemRc::accessible_role` (546),
    `accessible_string_property` (551), `accessible_action` (561),
    `supported_accessibility_actions` (566); geometry through `geometry` and
    `map_to_window`
    ([source](https://github.com/slint-ui/slint/blob/v1.18.1/internal/core/item_tree.rs)).
  - `window.rs`: `WindowInner::try_component`, `focus_item`, `active_popups`,
    `set_focus_item`
    ([source](https://github.com/slint-ui/slint/blob/v1.18.1/internal/core/window.rs)).
- Structure changes (`register_item_tree`, `unregister_item_tree`,
  `handle_focus_change`) are announced only through `WindowAdapterInternal`
  (`window.rs` line 225), which is `#[doc(hidden)]`. The prototype does not
  implement it; it rebuilds after a redrawn frame and uses a
  `PropertyTracker` to catch property changes without a redraw.
- `i-slint-core`'s own `accesskit` feature (Cargo.toml line 51) only adds
  `AccessKit` glue for parley text; it is not a platform bridge.

### How the winit backend does it

- `i-slint-backend-winit` 1.18.1 depends on `accesskit` 0.24 and
  `accesskit_winit` 0.33 (Cargo.toml lines 268–275), one version behind the
  current crates. Its `accesskit.rs`
  ([source](https://github.com/slint-ui/slint/blob/v1.18.1/internal/backends/winit/accesskit.rs)):
  node ids pack a component id and an item index (line 462); actions are
  deferred with `Timer::single_shot` (line 313) so they run outside the
  platform's accessibility call.
- Using it here would bring winit and Slint's GPU renderers into the build
  (see T72, PR pyrlyn/slint-bindings#32), so the bridge is our own.
- The backend is GPL-3.0 / Slint royalty-free / Slint commercial
  (Cargo.toml line 27); this repo is Apache-2.0, so its role mapping was
  reimplemented from the public `AccessibleRole` and AccessKit `Role` docs,
  not copied.

### AccessKit crates

| Crate | Version | Released | License | Notes |
| --- | --- | --- | --- | --- |
| [accesskit](https://crates.io/crates/accesskit) | 0.25.1 | 2026-09-25 | MIT OR Apache-2.0 | Tree schema, no platform code |
| [accesskit_macos](https://crates.io/crates/accesskit_macos) | 0.27.1 | 2026-09-25 | MIT OR Apache-2.0 | `rust-version` 1.87; objc2 0.5 / objc2-app-kit 0.2 (already in `Cargo.lock`) |
| [accesskit_windows](https://crates.io/crates/accesskit_windows) | 0.35.1 | 2026-09-25 | MIT OR Apache-2.0 | UIA provider for a whole `HWND` |
| [accesskit_consumer](https://crates.io/crates/accesskit_consumer) | 0.39.1 | 2026-09-25 | MIT OR Apache-2.0 | Pulled in by the adapters |
| [accesskit_winit](https://crates.io/crates/accesskit_winit) | 0.34.1 | 2026-09-25 | MIT OR Apache-2.0 | Not used |

Repository: [AccessKit/accesskit](https://github.com/AccessKit/accesskit).

### accesskit_macos 0.27.1 facts the host relies on

- Two entry points: `Adapter` (the view forwards `accessibilityChildren`,
  `accessibilityFocusedUIElement` and `accessibilityHitTest:`; `adapter.rs`
  lines 223, 248, 275) and `SubclassingAdapter`, which swaps the view's class
  at run time (`subclass.rs`). The host uses `Adapter`: `SlintNSView` is a
  Swift class it owns.
- Bounds are physical pixels from the view's top-left; `util.rs` divides by
  the window's `backingScaleFactor` (line 44) and calls
  `view.window().unwrap()` (line 39), so a frame query on a view with no
  window panics.
- `Role::Label` exposes its label as the value (`node.rs` line 233).

### Windows options

- `accesskit_windows` 0.35.1: `Adapter::new(hwnd, is_window_focused,
  action_handler)` and `handle_wm_getobject` answer `WM_GETOBJECT` for the
  whole `HWND`
  ([docs.rs](https://docs.rs/accesskit_windows/0.35.1/accesskit_windows/struct.Adapter.html)).
  A WinUI 3 `SwapChainPanel` has no `HWND` of its own, so the adapter would
  replace XAML's UIA tree for the whole window.
- WinUI's own route: a custom `AutomationPeer` from `OnCreateAutomationPeer`,
  with `GetChildrenCore` returning peers for a custom child structure
  ([Custom automation peers](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/custom-automation-peers),
  ms.date 2026-08-21).
