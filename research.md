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

## 8. slintx: the Slint language surface for a clean-room compiler and interpreter (2026-10-20)

Research for the roadmap item "Own Slint compiler and interpreter (slintx)": a
parser, compiler and interpreter for the full Slint language, written here, that
renders through `slintx-wgpu` and builds for native targets and `wasm32`. This
section records the language surface as a checklist; the task split is in
`plan.md`.

### 8.1 Version, sources and clean-room rules

- slint-bindings pins `slint`, `slint-build` and `i-slint-core` at `=1.18.1`
  (`Cargo.toml`). 1.18.1 is the newest release on crates.io, published
  2026-09-21 ([crates.io API](https://crates.io/api/v1/crates/slint), checked 2026-10-20).
- The newest versioned language docs are for 1.18.0
  ([docs 1.18.0](https://releases.slint.dev/1.18.0/docs/slint/), checked 2026-10-20);
  `releases.slint.dev/1.18.1/docs/slint/` redirects to the unversioned
  `docs.slint.dev/latest`. The 1.18.1 changelog lists only bug fixes and two
  widget accessibility properties, no language change
  ([CHANGELOG.md at v1.18.1](https://github.com/slint-ui/slint/blob/v1.18.1/CHANGELOG.md)),
  so the 1.18.0 docs describe the 1.18.1 language. Every checklist below cites
  the 1.18.0 pages it came from.
- Language additions in 1.18.0 that the checklist must include (same changelog,
  "Slint language"): `FlexboxLayout`; per-item `cross-axis-self-alignment` and
  `layout-order`; struct field defaults; `push`/`remove`/`insert` on arrays and
  models; `string.starts-with`, `ends-with`, `replace-all`; the `spring` easing;
  `WindowMoveArea`; `Path.point-at`/`angle-at`; `input-method-hints`;
  `max-lines` and `line-height-factor`; `Platform.uses-mock-data`; Flickable
  `content-*` names (old `viewport-*` names stay as deprecated aliases); dynamic
  `z`; enum and color literals inferred from the expected type; binding loops
  through two-way bindings detected at compile time.
- Licences, from the crates.io API (checked 2026-10-20): `slint`,
  `i-slint-core`, `i-slint-compiler`, `i-slint-common` and `slint-interpreter`
  1.18.1 are all `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR
  LicenseRef-Slint-Software-3.0`
  ([slint](https://crates.io/api/v1/crates/slint),
  [i-slint-compiler](https://crates.io/api/v1/crates/i-slint-compiler),
  [slint-interpreter](https://crates.io/api/v1/crates/slint-interpreter)).
  slintx crates must not depend on any of them, normally or transitively.
  Slint may appear only as the test oracle, in an unpublished crate
  (`publish = false`) that no shipped artifact links. Whether that test-only
  use is acceptable is the creator's call; this is not legal advice.
- Clean room: notes come from the documentation pages only. Slint's compiler,
  runtime and std-widget `.slint` sources were not opened, and nothing here is
  copied or translated from them. Implementers follow the same rule: behaviour
  the docs leave open (marked **OPEN** below) is settled by black-box
  comparison with Slint's compiler and renderer in the differential harness,
  never by reading its code. Checklist items paraphrase the docs.
- `slintx-wgpu` (task T72, branch `t72-slintx-wgpu`, not yet on `main`) still
  depends on `i-slint-core` for its renderer traits, the `Brush`/`Color` types
  and `sharedparley` text layout (`crates/slintx-wgpu/Cargo.toml` and
  `src/lib.rs` on that branch). slintx needs a draw API that takes its own
  scene types, with the `i-slint-core` adapter behind a feature.

### 8.2 The parser is a standalone AST library

The creator's requirement: slintx "must work as an AST parser". The first
layer is a public crate that turns `.slint` source into a lossless syntax tree
and a typed AST, with no dependency on the type checker or the interpreter:

- Lossless concrete syntax tree: every byte, comment and whitespace run is kept,
  so printing the tree gives back the input (round-trip tests), and every node
  has a byte span (span tests). Tooling, formatters and code generators that
  check their output use it directly.
- Typed AST: thin typed views over the tree (one type per construct in 8.4.1 and 8.4.2),
  so consumers do not walk raw nodes.
- Error recovery: a syntax error yields an error node plus diagnostics and
  parsing continues, so a half-typed file in a preview still gives a tree.
- Builds and is tested alone for `wasm32-unknown-unknown`, so a browser
  consumer can validate `.slint` without the rest of slintx (penpotx plans
  this; see its `research.md` and `ideas.md`).

Ready crates for this layer (all maintained and permissively licensed;
crates.io API, checked 2026-10-20):

| Crate | Version | Licence | Use |
| --- | --- | --- | --- |
| [rowan](https://crates.io/crates/rowan) | 0.17.0 | MIT OR Apache-2.0 | Lossless green/red syntax tree with spans (rust-analyzer's tree) |
| [logos](https://crates.io/crates/logos) | 0.16.1 | MIT OR Apache-2.0 | Lexer generator; the lexical rules in 8.4.1 fit a token table |
| [annotate-snippets](https://crates.io/crates/annotate-snippets) | 0.12.17 | MIT OR Apache-2.0 | Rendering diagnostics with source snippets (rustc's renderer) |

rowan only stores the tree; the parser that fills it is ours, written as
recursive descent. Slint's `a-b` identifiers, contextual keywords and unit
suffixes (8.4.1, Lexical structure) are easier to get right by hand than in a
combinator grammar, and recursive descent makes error recovery explicit.

### 8.3 Ready crates for the runtime

| Crate | Version | Licence | Use |
| --- | --- | --- | --- |
| [taffy](https://crates.io/crates/taffy) | 0.15.0 | MIT | CSS block, flexbox and grid layout ([README at v0.15.0](https://github.com/DioxusLabs/taffy/blob/v0.15.0/README.md)); candidate for `FlexboxLayout` once the harness confirms its results match Slint's. Box and grid layouts follow Slint's own rules (8.4.3, Layout algorithm) and may need our own code |
| [parley](https://crates.io/crates/parley) / [fontique](https://crates.io/crates/fontique) | 0.12.0 | Apache-2.0 OR MIT | Text shaping, line breaking and font fallback, needed for implicit text sizes and rendering once `slintx-wgpu` stops using `sharedparley` from `i-slint-core` |
| [swash](https://crates.io/crates/swash), [lyon_tessellation](https://crates.io/crates/lyon_tessellation) | 0.2.10, 1.0.23 | Apache-2.0 OR MIT, MIT OR Apache-2.0 | Already used by `slintx-wgpu` (glyphs, paths) |

The property runtime (lazy bindings with dependency tracking, 8.4.2 Bindings and
reactivity) is small and tied to Slint's semantics (two-way links, `changed`
handlers, animations), so it is written here; a generic signals crate would
need its semantics bent to match.

### 8.4 Language surface checklist

Four groups: syntax and types; components, bindings and the runtime model;
built-in elements, input and layouts; std-widgets. **OPEN** items are where
the docs leave behaviour unspecified; each becomes a differential-harness case.

#### 8.4.1 Syntax, types and expressions

##### Lexical structure
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/lexical-structure/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/source-files/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/
- [ ] File extension — `.slint`; encoding UTF-8; a leading BOM (U+FEFF) is accepted and ignored.
- [ ] Line terminators — LF and CRLF are equivalent; a bare CR is not a line terminator (discouraged, not an error).
- [ ] Whitespace — space, tab, line terminators; any run equals one space; only separates tokens.
- [ ] Line comment — `//` to end of line; `///` is an ordinary comment (no doc-comment meaning).
- [ ] Block comment — `/* ... */`, NESTS (depth counter); `/**` is an ordinary comment; comments act as whitespace. Unterminated block comment: error (OPEN: message unspecified).
- [ ] Token order — token kinds are tried in a fixed order, first match wins; so an input readable as number or identifier is a number.
- [ ] Number token — starts with ASCII digit; digits, optional `.` fraction, optional suffix = run of ASCII letters (unit) or single `%`. Suffix is part of the token (`10px`, `50%`, `1.5s`).
- [ ] Identifier — first char XID_Start or `_`; subsequent XID_Continue or `-`; leading `-` never starts an identifier; digits allowed after first char; Unicode identifiers permitted (UAX #31).
- [ ] Identifier normalization — `_` (except as first char) equals `-`; compare normalized forms (`foo_bar` == `foo-bar`, `_foo_bar` == `_foo-bar`); kebab-case is canonical. Applies to ids, properties, callbacks, functions, enum values, struct fields (docs state it for identifiers generally).
- [ ] Contextual keywords — no globally reserved words; a keyword is special only where the grammar expects the construct (e.g. `if` ident quirk, see Statements). Pre-defined names `self`, `parent`, `root`, `true`, `false` cannot be redefined as ids.
- [ ] Punctuation seen in grammar — `{ } ( ) [ ] ; , : := <=> => -> . ? ! = += -= *= /= + - * / == != < > <= >= && ||` and `@` macros, `#` color literal start.
- [ ] OPEN: lexing of `a-b` — since `-` is an identifier char, `a-b` lexes as one identifier; `a - b` (spaces) is subtraction. Implementer must follow identifier-greedy rule; docs do not spell out the number-then-identifier case (`1-2`).
- [ ] OPEN: number with letters that are not a known unit (e.g. `10foo`) — token is valid, error arises at type resolution; exact message unspecified.

##### Files, imports and exports
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/file-structure/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/imports/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/exports/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/file/
- [ ] Top-level items — zero or more of: component, import, export, struct, enum, global; empty/comment-only file is valid and defines nothing.
- [ ] Component forms — `component Name { body }` and `component Name inherits Base { body }`; Base must be built-in, defined EARLIER in file, or imported; instance is an element of type Base, body bindings apply to it, body elements become its children, Base's properties are inherited.
- [ ] Component body — zero or more element instantiations, bindings, property declarations (also callbacks/functions/etc. per other sections); no trailing `;` after `}` (`Text {};` is invalid).
- [ ] Element instantiation — `[id :=] TypeName { body }`; TypeName resolves to built-in or user component; nested instantiations are children.
- [ ] Element id — `:=` names the element; id unique across the whole component including inside `for`/`if` subtrees; compared after normalization; `self`, `parent`, `root` reserved (explicit `root` id = error).
- [ ] Implicit `root` — every component's outermost element has automatic id `root`.
- [ ] Window as element — instantiating `Window` (or a component inheriting it) as a nested element is deprecated (no separate window created).
- [ ] Named import — `import { A, B as C, } from "path";`; one or more items, trailing comma ok; `X as Y` introduces only Y (not X); X must be exported by target; two items may not introduce the same local name; imports components, structs, enums, globals.
- [ ] Import path — double-quoted string taken VERBATIM (no escape decoding; `\` and `/` both separators); `@name/rest` or `@name` = component library resolved via build-configured library paths (`@lib` -> file that exports, `@lib/x.slint` -> file inside dir); otherwise relative to importing file's dir, then compiler include paths; first match wins; missing file = error.
- [ ] File import — `import "./Font.ttf";` (no list) makes file known; `.ttf`/`.otf` registers the font for the app.
- [ ] Privacy — all names private to file unless exported.
- [ ] Export forms — `export component|struct|enum|global ...` at declaration; `export { Name, Other as Ext, };` for names defined in this file (trailing comma ok; local name stays defined; every left-hand name must be defined locally).
- [ ] Re-export — `export { A, B as C } from "path";` and `export * from "path";` (`*` form at most once per file; path resolved like imports).
- [ ] Duplicate exports — same external name exported twice (incl. declaration-site + list) = error.
- [ ] Import/export placement — top level, interleaved with other items; import order vs. use: OPEN (imports presumably hoisted; docs only require inheritance Base be defined earlier or imported).
- [ ] OPEN: export of a name that is itself imported — allowed via `export { X }`? docs say "defined in the current file"; treat imported names as errors unless decided otherwise.
- [ ] OPEN: cyclic imports and std-widgets.slint resolution (`import { Button } from "std-widgets.slint";`) — builtin library path handling not specified in this scope.

##### Name resolution
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/name-resolution/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/name-resolution/
- [ ] Unqualified lookup order (first match wins) — 1 local `let` var or callback/function parameter; 2 `self`/`parent`/`true`/`false`; 3 element id (incl. `root`; visible: current element's id, ids reachable from root, enclosing `for` element id and ids nested in it); 4 declared property/callback/function in scope, searching self, then enclosing element, up to component root; `for` index and model-data names resolve here; 5 global singleton or enum by name; 6 builtin namespace `Colors`, `Easing`, `Math`, `Key`, `FontWeight`, `MouseCursor`; 7 named value of the expected type (CSS color name for color/brush, easing name for easing, enum value for expected enum); 8 builtin free function (math, color functions, `debug`, ...).
- [ ] Declared-only rule — step 4 finds only members declared in source; builtin properties of elements/ancestors are NOT reachable unqualified; must write `self.width` / `id.width`.
- [ ] Shadowing — local shadows member; inner element member shadows outer; id wins over same-named property in scope; earlier lookup kind shadows later kinds.
- [ ] Qualified lookup — `id.`/`self.`/`parent.`/`root.` searches only that element and its base type chain; NO fallback to ancestors; so `f()` != `self.f()`.
- [ ] Visibility on qualified access — cannot access private property/callback/function of another component; cannot read a binding-only special property.
- [ ] Global members — `Name.prop`, `Name.callback(...)`, `Name.function(...)`.
- [ ] `parent` — only valid if the element has an enclosing element within the same component.
- [ ] Function shadowing example — same function name on several elements allowed; innermost wins for unqualified calls (1/3/2 example: component-level binding gets root's, Text with own gets 3, sibling Text sees layout's 2).
- [ ] Enum short names only where the expected type is known: binding, struct field, array element, call argument, return value; in `==` only RHS may omit (`a.suit == hearts` ok, `hearts == a.suit` error).
- [ ] CSS color names usable bare where color/brush is expected, including struct fields, array elements, function args; elsewhere via `Colors.name`.

##### Types
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/, https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/numeric-types/, https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/other-types/
- [ ] `bool` — default false; literals `true`/`false`.
- [ ] `int` — signed integral, default 0 (OPEN: bit width; docs say only "signed integral"; Slint impl is 32-bit, decide overflow behavior).
- [ ] `float` — signed 32-bit float, default 0; `%` suffix literal is divided by 100 (`30%` == 0.30).
- [ ] `percent` — 32-bit float interpreted as percentage, default `0%`; literals assigned must carry `%`.
- [ ] `length` — default `0px`; used for x/y/width/height; logical pixels.
- [ ] `physical-length` — default `0phx`; physical pixels; int -> length by `* 1px`, length -> float by `/ 1phx`.
- [ ] `relative-font-size` — default `0rem`; factor multiplied with `Window.default-font-size`; convertible to length.
- [ ] `angle` — default `0deg`; literals `deg`, `rad`, `turn` (docs show `0.25turn`).
- [ ] `duration` — default `0ms`; literal suffixes `ms`, `s`.
- [ ] `string` — default `""`; UTF-8.
- [ ] `styled-text` — default `""`; produced via `@markdown("...")` macro (see Expressions).
- [ ] `color` — RGBA 8 bits/channel, default transparent; properties `red`,`green`,`blue`,`alpha` each 0-255 (read-only fields).
- [ ] `brush` — color or gradient, default transparent; color converts implicitly to brush.
- [ ] `image` — default empty image (width 0, height 0); fields `width`, `height` (int, pixels).
- [ ] `easing` — default `linear`; see Built-in functions and namespaces for names.
- [ ] `keys` — default empty keys; created by `@keys(Control + Z)` macro; member `to-string() -> string` (platform-native text, e.g. "Ctrl+Z" / macOS symbols).
- [ ] `data-transfer` — opaque payload (text/image/file paths) for drag-drop/clipboard; default empty; cannot be inspected from Slint code.
- [ ] Array type `[T]` — T any type incl. struct and `[[int]]`; default = empty model; value is a model.
- [ ] Struct types — named (declared) and anonymous `{ a: int, b: string }`; defaults per field.
- [ ] Enum types — declared and built-in; default is first value; built-in `MouseCursor` default `default`.
- [ ] Callback/function types — not first-class property types; element types (Rectangle etc.) are not property types.
- [ ] Postfix unit-less access — float/int have members `to-fixed(digits: int) -> string`, `to-precision(digits: int) -> string`, `to-string-unlocalized() -> string`, plus math functions in postfix style on any numeric value (`(-10).abs()`, `x.round()`).
- [ ] OPEN: whether `property <type>` spacing, `property<type>` both accepted — examples use both; accept both.

##### Units and conversions
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/type-conversions/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/expressions/, https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/numeric-types/
- [ ] Length unit literals — `px` (logical), `phx` (physical), `cm`, `mm`, `in`, `pt`, `rem`; type of `px/cm/mm/in/pt` is `length`, `phx` is `physical-length`, `rem` is `relative-font-size`. Fractions allowed (`10.5px`).
- [ ] Angle units — `deg`, `rad`, `turn`. OPEN: `grad` not documented; decide.
- [ ] Duration units — `ms`, `s`.
- [ ] Percent — `%` suffix; float-like in numeric context (divides by 100), `percent` type when property is percent.
- [ ] Literal `0` — converts to any unit-bearing type (length, angle, duration...).
- [ ] int <-> float — implicit both ways; float -> int truncates (`property<int> x: 45.8` = 45).
- [ ] int/float -> string — implicit, uses current locale decimal separator (`Platform.decimal-separator`, default `.`); `to-fixed`, `to-precision` localized too; `to-string-unlocalized()` always dot.
- [ ] string -> float — only via `to-float()` (0 if invalid) and `is-float()`; locale separator only (de: "1,5" ok, "1.5" invalid -> 0); number literals in source always use dot.
- [ ] length / physical-length / relative-font-size — implicitly interconvertible ONLY where device pixel ratio / default font size are known; otherwise error.
- [ ] Unit types -> numbers — no implicit conversion; divide by same unit (`w / 1px` -> float) or multiply a number by a unit (`n * 1px`).
- [ ] Struct conversion — convertible if same field names with convertible types; source may miss fields (take default) OR have extra fields (dropped), NOT both (error, treats as typo).
- [ ] Array conversion — arrays do not convert between element types; array LITERALS convert if each element converts.
- [ ] Named struct vs anonymous — named struct only equal to itself; anonymous with same fields is same type; literal `{...}` adapts to the expected type.
- [ ] color -> brush — implicit; brush -> color: not implicit (OPEN).
- [ ] Mismatched operand types — compile error; no other implicit coercions.

##### Operators and precedence
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/operators/
- [ ] Precedence loosest to tightest — `? :` (right) < `||` `&&` (same level, NON-associative) < `== != < > <= >=` (non-associative) < `+ -` (left) < `* /` (left) < unary `+ - !` (prefix) < `.` `[]` (left); parentheses override.
- [ ] Non-associativity errors — `a == b == c`, `a < b < c` and `a && b || c` (mixed && ||) are errors; require parentheses. (`a && b && c` same op: OPEN whether allowed; doc says non-associative so treat chained same-op as error or accept - decide, Slint-compat likely accepts `&&` chains.)
- [ ] No `%` operator — use `Math.mod`/`mod`; `%` after number is a unit.
- [ ] No bitwise, shift, `++`/`--`, or assignment operators in expressions; `:` binding, `<=>` two-way, `=`/`+=` etc. only as statements.
- [ ] Arithmetic `+ - * /` on int, float, percent and unit types (length, physical-length, duration, angle, rem).
- [ ] `+`/`-` — operands same type, or one plain number; different units cannot be added.
- [ ] `*` — value * number, or unit * unit giving product unit (`1px*1px` is px^2); `/` — value / number, or value / same-unit value -> plain number.
- [ ] String `+` — if either operand is string result is string; int/float operand converted to string; any other type (e.g. length) is an error; use `\{}` interpolation instead.
- [ ] Comparison — `==`,`!=` on common types give bool; `< > <= >=` only on numbers, unit types, strings; other types error.
- [ ] Logical `&& || !` — operands must be bool (no truthiness); `&&`/`||` short-circuit.
- [ ] Unary `+ -` — numbers and unit types; `!` — bool.
- [ ] Ternary `c ? a : b` — c bool; a and b same type (result type); right-assoc (`a?b:c?d:e` = `a?b:(c?d:e)`); can span lines.
- [ ] Member access `.` — property/function/callback of element, field of struct, member of builtin types (`length`, `to-float()` ...).
- [ ] Indexing `[i]` — array element, zero-based int; out-of-range returns element type default (see Arrays).
- [ ] OPEN: division by zero — unspecified (int and float); decide (float inf/NaN per IEEE is Slint-like).
- [ ] OPEN: int overflow, float equality, NaN comparisons, unit-product types (`px*px`) usable only transiently or as property types? — unspecified.
- [ ] OPEN: `Math.mod`/`mod` sign — Euclidean per docs (result in [0,|b|)).

##### Expressions
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/expressions/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/expressions-and-statements/
- [ ] Literals — number (`42`, `3.14` -> int/float by form), number+unit (`100px`, `250ms`), `true`/`false`, string, color, struct literal, array literal.
- [ ] Integer literal — whole number without unit/fraction has type int; `3.14` float.
- [ ] Color literal — `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`; hex case-insensitive; other digit counts error; short forms duplicate digits (`#18f` = `#1188ff`); no alpha = opaque.
- [ ] Length literal — number directly followed by unit (`120px`).
- [ ] String literal — `"..."` with escapes `\"`, `\\`, `\n`, `\u{hex}`, `\{expr}` interpolation (expression result converted to string by + rules); any other `\x` is an error; may span lines (OPEN).
- [ ] String interpolation — `"a \{ expr } b"`; expr may be any expression convertible to string (int/float/string; OPEN: others).
- [ ] Image literal — `@image-url("path")` (string literal only, resolved at compile time); optional second arg `nine-slice(n)` / `nine-slice(v h)` / `nine-slice(t r b l)` (space-separated numbers; 1, 2 or 4 values); empty string = empty image; unlike import paths, escapes (`\"`, `\\`, `\n`, `\u{}`) ARE decoded; resolution: absolute, relative to the .slint file, then include path; also `data:` URIs (base64 or url-encoded) and URLs (http only in SlintPad).
- [ ] Image formats — SVG plus raster via image crate: AVIF, BMP, DDS, Farbfeld, GIF, HDR, ICO, JPEG, EXR, PNG, PNM, QOI, TGA, TIFF, WebP.
- [ ] Gradient macros — `@linear-gradient(angle, color pct, ...)`, `@radial-gradient(circle [radius] [at x y], color pct, ...)`, `@conic-gradient([from angle] [at x y], color angle, ...)`; stops are space-separated `color percentage` (linear/radial) or `color angle` (conic).
- [ ] Linear gradient — first arg an angle (`90deg`), at least one stop; stops colors interpolated along the rotated axis.
- [ ] Radial gradient — first arg literally `circle`; optional radius (length expr) default = half of bounding-box diagonal; optional `at x y` (length exprs, local coords) default center; radius without `at` allowed (`circle 60px at 25px 25px`).
- [ ] Conic gradient — stops `color angle` over 0deg..360deg, 0deg points up, clockwise; optional `from angle` rotates start; optional `at x y`.
- [ ] Other macros — `@markdown("...")` (styled-text; string literals concatenate if several; `\{expr}` interpolation, interpolated text is markdown-escaped; `<font color='...'>` supports names/hex; dynamic color interpolation only in the color attribute and expr must be color; empty invocation = compile error), `@keys(Mod + Key)`, `@tr(...)` (translations; out of scope here), `@image-url`.
- [ ] Property reference — `[self|parent|root|id.]name`; unqualified follows lookup rules; reading yields the property's type; may also resolve to local, callback, function, enum value, global property.
- [ ] Calls — `name(args)` / `elem.name(args)` for callbacks, functions, builtin functions; argument count must match.
- [ ] Struct literal — `{ field: expr, ... }` trailing comma ok; omitted fields take defaults; members that are not fields of the target struct are ignored (but see conversion rule: missing AND extra together = error).
- [ ] Array literal — `[a, b, c]` trailing comma (OPEN); empty `[]`; elements must match element type.
- [ ] Parenthesized expression — grouping only.
- [ ] Reactive evaluation — expressions re-evaluate when read properties change (dependency tracking).

##### Statements
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/statements/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/expressions-and-statements/
- [ ] Code block — `{ stmt; stmt; ... }`; used for function bodies, callback handlers, and binding block form; a binding may be `name: expr;` or `name: { ... }` interchangeably; value of block = value of its last statement (no trailing `;` needed for last expression), unless `return`.
- [ ] Expression statement — `expr;` value discarded (except last in block); used for calls.
- [ ] Assignment — `lhs = expr;` lhs a writable place: property ref, struct field of a struct property (`p.x = ...`), array element of an array property (`a[i] = ...`), or the model value inside a `for` element; assigning to local var, call result, etc. = error; rhs must have lhs type; property must be writable from there (not `in`-from-outside, not linked to read-only, not constexpr-known-at-compile-time).
- [ ] Compound assignment — `+=` `-=` `*=` `/=`; `+=` also on string and length-like types; `-= *= /=` only length-like (OPEN: docs say "length-like"; Slint also accepts plain numbers, implement for all numeric/unit types); otherwise error.
- [ ] `let` — `let name = expr;` or `let name: type = expr;`; initializer required; type inferred if omitted; immutable; scope = rest of enclosing block; redeclaring a name already in scope (local var or handler/function parameter), even in nested block = error.
- [ ] `:=` in statements — error with hint to use `let`.
- [ ] `if` — `if cond { } else if cond2 { } else { }`; parentheses around condition optional; branches are code blocks; `if` statement vs identifier: taken as statement only if not immediately followed by `.` `,` `;` `}` `]` `)`.
- [ ] `if` as expression — OPEN: docs call `if` a statement only; value-yielding `if` in last position of block likely follows "last statement value" rule; decide.
- [ ] No `for`/`while` statement; iteration only via `for` element in element tree.
- [ ] `return` — `return;` or `return expr;`; if enclosing function/callback declares a return type, bare `return` is error and value type must match; without return the last statement's value is the result.
- [ ] Empty statement — lone `;`.
- [ ] Unknown syntax — anything not above is parsed as expression statement; unrecognized = parse error.
- [ ] Purity interaction — assignments and impure calls not allowed in pure contexts (see Functions).

##### Functions and callbacks (declarations)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/functions/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/callbacks/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/functions-and-callbacks/
- [ ] Function declaration — `[public|protected] [pure] function name(p: T, ...) [-> R] { body }`; allowed on component root, any element inside, or in a global; NOT top-level, in struct/enum, or nested in a function.
- [ ] Function params — `name: type`, by value, unique names within function; no `->` means void; body required (declaration without `{}` = error).
- [ ] Function name conflicts — error if equals a property of the same element or overrides a non-shadowable inherited member; same name on different elements OK.
- [ ] Visibility — private default (own component only); `public` (reachable via an element instance of the declaring component; from native code if exported; only root-level functions, not child-element ones); `protected` (only by a component directly inheriting, through its own `self`/`root`; not via child instances, not >1 inheritance level). Same rules for globals.
- [ ] Purity — `pure` marks side-effect free; public/protected functions must say `pure` explicitly (default impure); private unmarked functions have purity inferred from body; pure function that assigns a property or calls impure function/callback = error.
- [ ] Calling functions — unqualified (searched self -> enclosing -> root) or `elem.f()`; return value `return` last-statement rule.
- [ ] Callback declaration — `[pure] callback name[(T1, name2: T2, ...)] [-> R];` ; no parens = no params; trailing comma in param list ok; param names optional/mixable, carry no semantics; `-> R` requires parentheses (`callback c() -> int`); name must not clash with property/function/callback on the element; duplicate = error.
- [ ] Callback call — `cb(args)`; argument count must match; no handler = no-op; return-typed callback without handler returns the type's default value.
- [ ] Callback handler — `name => expr` / `name => { block }` / `name(a, b) => { ... }`; parameter names local and independent of declaration; parens omitted when no params; last expression is return value, type must match; at most one handler per callback (second = error); handlers may target child element callbacks (`clicked => {...}`) and own callbacks.
- [ ] `init` callback — present on every element, handler `init => { }` runs once on instantiation, order outer-to-inner (component's own init runs AFTER contained elements' inits per doc wording; OPEN: confirm order), explicit call is a no-op and deprecated; globals cannot declare or handle `init` (error).
- [ ] Callback alias — `callback name <=> target.cb;` no parameter list allowed in alias declaration; same signature required; invoking/handling either affects both; may target a global callback `Global.cb`; global may alias another global's callback; non-global element cannot set a handler on an alias targeting a global callback (error).
- [ ] Change callback — `changed prop => { ... }`; target must be an existing property of the element (not callback/function/missing); not on another component's private property; one per property per element; handler may be impure; queued and run later in the event loop, only if value differs from last run; changes collapse; chained change callbacks run in loop capped at 10 passes; feedback loops undefined; forces eager evaluation of the property.
- [ ] Functions vs callbacks — callback may lack body and have native handler, may be aliased, always public-reachable; function must have body, no alias, private by default.
- [ ] Named arguments — guide mentions "named arguments" in callback docs; this chapter documents only named params in declarations. OPEN: call-site named arguments not specified here.

##### Structs and enums (declarations)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/structs-and-enums/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/structs-and-enums/
- [ ] Struct declaration — `[export] struct Name { field: type, ... }` top level; comma-separated, trailing comma ok, empty struct allowed, `;` separator = error (hint to use `,`); field type any property type.
- [ ] Struct attributes — may be prefixed with one or more `@rust-attr(...)` (ignorable for non-Rust backends).
- [ ] Redeclaration — struct with a name matching an already-defined type replaces it with a warning.
- [ ] Field defaults — `field: type = expr`; expr must be compile-time constant (literals, casts, operators; no property/element refs); number-to-string only if locale independent (`= 42` ok, `= 42.1` error); only in named structs (error in anonymous); applies to literals omitting the field, unset properties, native construction.
- [ ] Struct default value — every field at its own default unless declared.
- [ ] Anonymous struct type — `{ a: int, b: string }`, trailing comma ok (`{a: int, }`); same-field anonymous types are identical.
- [ ] Struct literal — see Expressions; read field with `.`.
- [ ] Enum declaration — `[export] enum Name { a, b-c, ... }`; at least 1 value; value name != enum name; values distinct under `-`/`_` equivalence; generated names (drop `-`/`_`, capitalize first letter and each word after a separator) must be distinct (`hello-world` vs `HelloWorld` clash = error).
- [ ] Enum use — `Name.value`; default = first value; short form per Name resolution rules.

##### Arrays and models
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/arrays-and-models/
- [ ] `array.length` — int, read-only (cannot be assigned).
- [ ] `array[i]` read — zero-based int index; out of range (i<0 or i>=length) returns element default.
- [ ] `array[i] = v` write — requires writable array; out-of-range is silent no-op.
- [ ] `array.push(v)` — append; v has element type.
- [ ] `array.remove(i)` — valid for 0<=i<length; otherwise no-op.
- [ ] `array.insert(i, v)` — valid for 0<=i<=length (== length appends); otherwise no-op.
- [ ] Native models — array properties may be backed by host model; same operations apply.
- [ ] OPEN: float index (truncate? error) and mutating a constant array literal property — unspecified.

##### Strings
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/strings/
- [ ] `is-empty` — bool property (no parens): `"".is-empty`.
- [ ] `character-count` — int property, count of extended grapheme clusters (UAX #29), not code points.
- [ ] `to-lowercase() -> string`, `to-uppercase() -> string` — full Unicode case mapping (`"tschüß"` -> `"TSCHÜSS"`, final sigma handled).
- [ ] `starts-with(pat: string) -> bool`, `ends-with(pat: string) -> bool`.
- [ ] `to-float() -> float` (0 if invalid), `is-float() -> bool` — locale decimal separator.
- [ ] `replace-all(from: string, to: string) -> string` — all non-overlapping matches; empty `from` inserts `to` between every char and at both ends (`"abc"` -> `"-a-b-c-"`); empty haystack stays empty.
- [ ] Escapes — see Expressions (string literal).

##### Colors and brushes
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/colors-and-brushes/
- [ ] CSS named colors — full CSS Color named list in scope for color/brush expected types and under `Colors.` (e.g. `Colors.aquamarine`, `bisque`, `red`, `transparent`); OPEN: embed the CSS list (148 names + transparent).
- [ ] `rgb(r, g, b)` / `rgba(r, g, b, a)` — aliases accepting 3 or 4 args; r,g,b either 0-255 numbers or percentages (`%`); alpha 0-1; commas mandatory (no CSS space syntax).
- [ ] `hsv(h, s, v)` / `hsv(h, s, v, a)` — h degrees 0-360 wrapping (480 -> 120); s, v, a in 0-1.
- [ ] `oklch(l, c, h)` / `oklch(l, c, h, a)` — l 0-1 or 0%-100%; c 0..~0.4 or 0%-100% (100% = 0.4); h 0-360 or angle unit (`180deg`, `0.5turn`); a 0-1 default 1.
- [ ] Color/brush methods — `brighter(f) -> brush` (HSV value * (1+f)); `darker(f) -> brush` (value / (1+f)); `mix(other: brush, f) -> brush` (f clamped 0..1; f weight on self, 1-f on other); `transparentize(f) -> brush` (alpha * (1-f)); `with-alpha(a) -> brush` (alpha set 0-1); factors may be `%` literals; negative factors invert brighter/darker.
- [ ] `to-hsv() -> {hue, saturation, value, alpha}` (hue 0-360, others 0-1, floats).
- [ ] `to-oklch() -> {lightness, chroma, hue, alpha}` (L 0-1, chroma ~0-0.4, hue 0-360, alpha 0-1).
- [ ] Color fields — `.red .green .blue .alpha` 0-255 (OPEN: int vs float type unspecified).
- [ ] OPEN: color methods on a gradient brush — doc says "all colors and brushes"; define behavior (apply to every stop).
- [ ] OPEN: `brush` default is transparent; brush equality/comparison and brush->color conversion unspecified.

##### Built-in structs and enums
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/builtin-structs/, https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/builtin-enums/
- [ ] Built-in structs (fields) — `Point{x,y: length}`, `Size{width,height: length}`, `Edges{left,top,right,bottom: length}`, `FontMetrics{ascent,descent,x-height,cap-height: length}`, `KeyboardModifiers{alt,control,shift,meta: bool}` (macOS: Command -> `control`, Control -> `meta`; Windows key -> `meta`), `KeyEvent{text: string, modifiers: KeyboardModifiers, repeat: bool}`, `PointerEvent{button: PointerEventButton, kind: PointerEventKind, modifiers, touch-finger-id: int}` (0 = not touch), `PointerScrollEvent{delta-x,delta-y: length, modifiers}`, `InputMethodHints{capitalization: CapitalizationMode (default sentences), auto-correct: bool (true), auto-complete: bool (true)}`, `DropEvent{data: data-transfer, position: LogicalPosition, proposed-action: DragAction}`, `StandardListViewItem{text: string}`, `TableColumn{title: string, min-width: length, horizontal-stretch: float, sort-order: SortOrder, width: length}`.
- [ ] OPEN: `LogicalPosition` (used by DropEvent.position) is not listed on the structs page; presumably same shape as Point.
- [ ] Built-in enums with value counts — AccessibleLiveness 3, AccessibleRole 30, AnimationDirection 4, CapitalizationMode 4, ColorScheme 3, CrossAxisAlignment 5, DialogButtonRole 7, DragAction 4, EventResult 2, FillRule 2, FlexboxLayoutDirection 4, FlexboxLayoutWrap 3, FocusReason 5, ImageFit 4, ImageHorizontalAlignment 3, ImageRendering 2, ImageTiling 3, ImageVerticalAlignment 3, InputType 5, LayoutAlignment 7, LineCap 3, LineJoin 3, OperatingSystemType 6, Orientation 2, PathEvent 6, PointerEventButton 6, PointerEventKind 4, PopupClosePolicy 3, ScrollBarPolicy 3, SortOrder 3, StandardButtonKind 11, TextHorizontalAlignment 5, TextOverflow 2, TextStrokeStyle 2, TextVerticalAlignment 3, TextWrap 3, plus MouseCursor 29 (see Built-in functions and namespaces).
- [ ] Enum value names are kebab-case (`tab-list`, `no-wrap`, `close-on-click-outside`); first listed value is the default (verify listed order = declaration order, e.g. `ImageFit.fill`, `TextWrap.no-wrap`, `FlexboxLayoutWrap.wrap` first listed but doc says Slint defaults to wrap).
- [ ] Notable values — `EventResult{reject,accept}`, `FlexboxLayoutWrap{wrap,no-wrap,wrap-reverse}`, `TextWrap{no-wrap,word-wrap,char-wrap}`, `PointerEventKind{cancel,down,up,move}`, `PointerEventButton{other,left,right,middle,back,forward}`, `LayoutAlignment{stretch,center,start,end,space-between,space-around,space-evenly}`, `StandardButtonKind{ok,cancel,apply,close,reset,help,yes,no,abort,retry,ignore}`.
- [ ] OPEN: per-property defaults of built-in enums for elements are defined in element docs (other scope), not here.

##### Built-in functions and namespaces
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/global-functions/builtinfunctions/, https://releases.slint.dev/1.18.0/docs/slint/reference/global-functions/math/, https://releases.slint.dev/1.18.0/docs/slint/reference/global-namespaces/font-weight/, https://releases.slint.dev/1.18.0/docs/slint/reference/property-types/other-types/
- [ ] `animation-tick() -> duration` — monotonic time; calling in a binding makes it re-evaluate continuously.
- [ ] `debug(...)` — variadic, prints args, returns nothing (void).
- [ ] Math functions — callable as `Math.f(...)`, bare `f(...)`, or postfix `x.f()`; `T` = any numeric type (angle, duration, float, int, length, percent).
- [ ] `abs(T) -> T`, `clamp(T, T, T) -> T` (value, min, max), `min(T, T) -> T`, `max(T, T) -> T` (args same type), `mod(T, T) -> T` (Euclidean remainder, 0..|b|).
- [ ] `ceil(float) -> int`, `floor(float) -> int`, `round(float) -> int` (round half away from zero: 4.5 -> 5, -1.2 -> -1; OPEN: -0.5).
- [ ] `sign(float) -> float` — 1 or -1; returns 1 for 0 and -0.
- [ ] `sqrt(float) -> float`, `pow(float, float) -> float` (base, exponent), `exp(float) -> float`, `ln(float) -> float`, `log(float, float) -> float` (value, base).
- [ ] Trig — `sin(angle)`, `cos(angle)`, `tan(angle) -> float` (arg must carry deg/rad unit); `asin(float)`, `acos(float)`, `atan(float)`, `atan2(float, float) -> angle`.
- [ ] Math constants — OPEN: `Math.PI`/`Math.E` appear in Slint but are not in this page set; decide.
- [ ] Namespaces in scope — `Colors`, `Easing`, `Math`, `Key`, `FontWeight`, `MouseCursor` (`Key` members not covered in this scope).
- [ ] `FontWeight` constants (unitless float) — thin 100, extra-light 200, light 300, normal 400, medium 500, semi-bold 600, bold 700, extra-bold 800, black 900.
- [ ] `Easing` names — linear, ease, ease-in, ease-out, ease-in-out, ease-in/out/in-out-{quad,quart,quint,expo,sine,back,circ,elastic,bounce}, plus `cubic-bezier(a,b,c,d)` and `spring(bounce)`; bare names allowed where type is `easing`.
- [ ] `MouseCursor` values (29) — default, none, help, pointer, progress, wait, crosshair, text, alias, copy, move, no-drop, not-allowed, grab, grabbing, col-resize, row-resize, n-resize, e-resize, s-resize, w-resize, ne-resize, nw-resize, se-resize, sw-resize, ew-resize, ns-resize, nesw-resize, nwse-resize; bare or `MouseCursor.x`.
- [ ] `Platform.decimal-separator` — referenced global (default `.`); full Platform global is out of scope.

#### 8.4.2 Components, properties, bindings and the runtime model

# Slint 1.18.0 language surface: components, properties, bindings, runtime model

Base URL B = https://releases.slint.dev/1.18.0/docs/slint/ . Items marked OPEN are runtime points the docs leave unspecified; the differential test harness must pin them down.

##### Components and elements
- [ ] Element = type name followed by `{ ... }` body — body holds property bindings, declarations, callback handlers, child elements, `states`, `animate`, `changed`; nesting by placing elements in the parent's braces.
- [ ] Element id — `id := Element { }`, ids are element-local names usable in expressions (`ta.pressed`, `indicator.background`); also optional on `for`/`if` bodies (`for x in m : id := E {}`).
- [ ] Scope keywords in expressions — `self` (current element), `parent` (enclosing element), `root` (component root); `parent.width * 50%` used in docs; `self.width * index` used in repeater.
- [ ] Component declaration — `component Name inherits Base { }`, `export component ...`; a component with no `inherits` base defaults to its preferred size and has no built-in properties beyond common ones.
- [ ] Property set of an element = properties of its type incl. inherited base + declared ones; reading yields current value.
- [ ] Name lookup — `-` and `_` are interchangeable in identifiers (`my-prop` == `my_prop`); unqualified names resolve on `self`, then each enclosing element up to the component root.
- [ ] Use-site children belong to the component being used; appended to root element's children by default (see Container components).
- [ ] Visual vs non-visual — common visual properties (x,y,width,height,z,opacity,visible,transform-*,absolute-position,cache-rendering-hint,dialog-button-role) exist on visual items only; non-visual items (Timer) lack them.
- [ ] OPEN: component inheritance rules (what overrides what: re-binding an inherited property in `inherits` use, re-declaring inherited property = error?, overriding callbacks/handlers, which members are shadowable) are only hinted ("non-shadowable member inherited from a base element" is an error for functions); not defined in scope files.
- [ ] OPEN: `parent` for root element and for `@children`-inserted elements; resolution of `self`/`parent`/`root` inside repeated elements and states bodies is not spelled out.
- [ ] Interface declarations are mentioned in passing (functions "implementing" an interface function must be pure) and `for`/`if` forbidden in "interface" — OPEN: interface syntax not in scope files.
Source: https://releases.slint.dev/1.18.0/docs/slint/guide/language/concepts/slint-language/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/properties/, https://releases.slint.dev/1.18.0/docs/slint/reference/common/

##### Properties and visibility
- [ ] Declaration — `[visibility] property <type> name [: binding-expr];` visibility in {`in`, `out`, `in-out`, `private`}; `private` is the default; type written in angle brackets (`property<int>` without space also accepted); ends with `;` (a `{ }` code-block binding needs no trailing `;`).
- [ ] Declaring a property whose name equals an existing property, function or callback of that element is a compile error.
- [ ] `in` — input; set by the enclosing component with a binding, or by native API on a directly instantiated root.
- [ ] `out` — component computes it; the application (native side) reads it only.
- [ ] `in-out` — both sides read/write.
- [ ] `private` — internal; native code can neither get nor set it; a change callback cannot be attached to another component's private property (error).
- [ ] Only non-private properties declared at the top level of a component are visible from outside (as element properties and in the native bindings).
- [ ] Any property type may be declared (struct, array `[T]`, enum, brush, etc.; type list lives in another page).
- [ ] Default value — property without binding holds the default of its type; built-in defaults seen here: length 0px, bool false (animate `enabled` true, Timer `running` true), float 0, int 0, string "", opacity 1, z 0, visible true, transform-scale 100%.
- [ ] Binding on the declaration — `property <int> p: 42;` the binding is initial; later imperative assignment replaces it.
- [ ] Type may be omitted only with a two-way binding (inferred from the other side).
- [ ] Reading a property returns its current value (evaluating a dirty binding lazily first).
- [ ] OPEN: who may assign `out`/`private` properties from inside the component (assumed: the component itself, in handlers) and error text for assigning an `in` property from within its own component are not stated.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/properties/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/properties/

##### Bindings and reactivity
- [ ] Binding form — `name: expression;` or `name: { statements }` inside the element body; name must be a property of the enclosing element (error otherwise).
- [ ] Type rule — expression type must equal the property type or implicitly convert to it; otherwise compile error. Example conversions in docs: int to string via `"Count: " + counter`, `length / 1px` to number.
- [ ] Block bindings — `{ if (c) { return a; } else { return b; } }` equals ternary `c ? a : b`; last expression is the value; `return` allowed.
- [ ] A property holds at most one binding; setting a new binding replaces the old; last binding wins.
- [ ] Lazy evaluation — expression runs when the property is queried, not when the binding is set.
- [ ] Dependency tracking — every property read during evaluation is registered as a dependency of that binding (dynamic: set rebuilt on every evaluation, only reads on the taken path count, e.g. branches of `?:`).
- [ ] Invalidation — when a dependency changes the dependent binding is marked dirty (transitively); next query re-evaluates and rebuilds the dependency set.
- [ ] A binding that reads no properties is never dirtied and evaluates at most once (constant).
- [ ] Imperative assignment (`foo.bar = 42;`, `+=` etc.) removes the property's binding permanently; property keeps working as plain value, reactivity lost.
- [ ] Exception: assignment through a two-way binding writes through the link and keeps the binding.
- [ ] Built-in in-out properties written by user interaction (e.g. TextInput `text`) drop an existing binding like an imperative assignment; use `<=>` or a `changed` handler to keep reactivity.
- [ ] Code-block bindings with statements and assignment in a binding are forbidden (pure context, see Purity); bindings must have no side effects.
- [ ] Bindings can call pure functions/callbacks and built-ins like `clamp(a, lo, hi)`; `ta.mouse-x`, element-id property references track across elements.
- [ ] Binding evaluation inside states: while a state is active its listed properties take the state's value instead of their own binding.
- [ ] Binding loops — OPEN: docs in scope do not mention binding loops, detection, reporting (compile-time vs runtime, which value results). Harness must define: self-referential binding `a: b; b: a;`, and runtime cycle through differently-dirty paths.
- [ ] Constant folding — OPEN: not mentioned beyond "reads no properties is evaluated at most once".
- [ ] Eager vs lazy — bindings lazy; a `changed` handler on a property forces that property to be evaluated eagerly (re-evaluated after dependencies change, even if nobody reads it).
- [ ] OPEN: evaluation order when several bindings are dirtied by one change; whether a re-evaluation producing an equal value notifies dependents (docs say only that changed handlers run if value differs).
- [ ] OPEN: percent bindings — `width: 50%` means `parent.width * 50%`; `preferred-width: 100%` is allowed; other uses of `%` on non-size properties not specified.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/bindings/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/evaluation-and-purity/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/concepts/reactivity/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/properties/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/concepts/reactivity-vs-react/

##### Two-way bindings
- [ ] Syntax — `[in|out|in-out|private] property <type> name <=> target;` or inside an element body `name <=> target;`; works for properties and for callbacks (aliases).
- [ ] Declaration may omit the type: `in property rect-color2 <=> r.background;` type inferred from the target.
- [ ] Both sides must have the same type (otherwise error).
- [ ] Right side may be: a property of same element, another element (`r.background`), `root` component, a global (`Global.prop`, re-exposes the global member), or a field of matching type inside a struct-typed property (`prop.field`).
- [ ] Inside `for`, right side may be the model value (`for x in m` with `x`), kept in sync with the model entry (writes back to the model).
- [ ] Both sides must be compatible in direction: each side must be readable and writable wherever the link reads or writes it (e.g. cannot link an `in`-only to something that must be written).
- [ ] Initial value of the linked pair = value of the right-hand side (RHS wins over LHS binding/default).
- [ ] Writing either side updates the other; assignment through the link keeps the binding in place (no breaking).
- [ ] A property initialized with `<=>` cannot be changed in a state (error).
- [ ] Callback alias via `<=>` (see Callbacks); may target a global's callback including from another global.
- [ ] Purity must match across callback alias/`<=>`: both pure or both impure, else error.
- [ ] OPEN: precedence when a two-way-linked property also has its own `: expr` binding; conversions between convertible-but-different types (docs: must be identical); behaviour when chained links form a cycle.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/two-way-bindings/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/properties/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/states-and-transitions/

##### Callbacks, functions and purity
- [ ] Callback declaration — `[pure] callback name;`, `callback name(T1, T2);`, `callback name(a: T1, b: T2) -> R;` (param names optional, may be mixed, trailing comma allowed, names have no semantics); `-> R` requires parentheses (`callback c() -> int`), `-> R` without parens is an error.
- [ ] Callbacks declared on component root, any element, or global; name collision with property/function/callback of same element is an error; duplicate callback declaration is an error.
- [ ] Invocation — `name(args)`; argument count must match; invoking runs the handler; no handler: no-op, return value is type default (0, "", false, ...).
- [ ] Handler — `name(a, b) => expr-or-block`, parameters in parentheses local to handler (parentheses optional when no parameters); last expression is the return value, type must match declared return type; at most one handler per callback (a second is an error).
- [ ] Handlers may be set on built-in element callbacks (`clicked => { ... }`, `triggered() => ...`) and on callbacks declared in the same component.
- [ ] Callback alias — `callback clicked <=> area.clicked;` — must omit the parameter list (parentheses = error); signatures must match; invoking either invokes the other and a handler on either responds to both.
- [ ] Alias to global callback `Global.cb` allowed; a global may alias another global's callback (the supported way for one global to implement another's); a non-global element may NOT set a handler on an alias targeting a global callback (error).
- [ ] Function declaration — `[public|protected] [pure] function name(p: T, ...) -> R { body }`; allowed on component root, any element, global; not at file top level, not in struct/enum, not nested in a function (errors).
- [ ] Function rules — body required (no `{}` = error); `-> R` absent means void; params by value, unique names; `return expr;` or value of last statement; name may not collide with a property of the element nor override a non-shadowable inherited member (errors).
- [ ] Function visibility — private (default): only inside own component; public: callable from components that hold the declaring component as an element (`my-friend.double(1)`) and from native code if exported; protected: only from a directly inheriting component via its own `self`/`root`, not via a child instance, not across two inheritance levels.
- [ ] A public function on a child element (not the component root) is unreachable from other components.
- [ ] Function call resolution — unqualified: looked up on `self` then each enclosing element up to the root; `elem.fn()` must be defined on that element; same-name functions on different elements may resolve differently than `self.fn()`.
- [ ] Functions vs callbacks — callback handler may be provided by native code, function is Slint-only; callback may lack a body; callback may alias with `<=>`; callback always reachable like public, function default private.
- [ ] Purity — pure contexts: property binding expressions, `for` model expressions, bodies of `pure` functions and handlers of `pure` callbacks.
- [ ] Errors in pure context: assignment or compound assignment (`= += -= *= /=`) to a property; calling a non-pure callback; calling a non-pure function; calling a built-in function with side effects.
- [ ] `pure` required explicitly on public/protected functions (else treated impure); private functions have purity inferred from body; a private function may still be marked `pure` to enforce.
- [ ] Interface function implementation must be pure if declared pure (impure impl = error); a pure impl may satisfy an impure declaration.
- [ ] Purity must match across callback alias or callback `<=>` (both pure or both impure).
- [ ] Pure callbacks/functions may be called from bindings: `property <int> result: hello(1, 2);` uses the type default when no handler.
- [ ] Impure handlers (`clicked`, `changed`) may do side effects and assign properties.
- [ ] OPEN: which built-ins are side-effecting (e.g. `debug`, model `push`/`remove`, `show`/`hide`, `focus`) — not listed in scope files.
- [ ] OPEN: reentrancy (callback invoked from its own handler), assignment of `self` in handlers, error behavior of recursion in functions.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/callbacks/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/functions/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/evaluation-and-purity/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/two-way-bindings/

##### Change handlers and init
- [ ] `changed <property> => { ... }` inside an element body; target must be an existing property of that element (callback, function or unknown member = error); at most one per property per element (duplicate = error); not allowed on another component's private property (error).
- [ ] Handler body may have side effects (not pure-checked) but makes surrounding bindings impure.
- [ ] Not invoked immediately: queued and run in a later event-loop iteration; runs only if the value differs from the value at the previous run (changes within a cycle coalesce; change-and-revert yields no call).
- [ ] Feedback loops are undefined; queued handlers run in a loop that stops after ten passes, remaining handlers not invoked.
- [ ] A change callback forces eager evaluation of its property (dependency tracking stays alive without readers).
- [ ] `init` — implicit callback on every element; `init => { ... }` handler runs once at instantiation, after all properties hold their final binding values (reading a property in init sees the use-site value, e.g. `"Hello"`).
- [ ] Explicit call of `init()` does nothing and is deprecated; cannot be set from native code (no `on_init`); a global cannot declare or handle `init` (errors).
- [ ] Order — doc example prints first, second, third: component-internal init handlers of children run before the use-site `init` of the same element; text says "inside to outside" in the common page but the callbacks page says "outermost inward" with a component's own init after the inits of elements it contains. Net rule: inner-before-outer, the use-site init runs after the init of the component's own contents.
- [ ] `init` of conditional (`if`) / repeated elements runs when the instance is created (e.g. later when the condition becomes true), enabling calls into global callbacks implemented natively.
- [ ] OPEN: precise global order across siblings (in the example "second" from MyCheckBox runs before "third" from the earlier sibling MyButton's use-site init): confirm: use-site init of an element runs after init of all internal descendants, and siblings in declaration order?
- [ ] OPEN: whether both the component-internal `init` and the use-site `init` on the same element run (the example implies yes although "one handler per callback" rule exists).
- [ ] OPEN: relative order of queued `changed` handlers (declaration order? dependency order?), whether handlers queued during pass N run in pass N+1, initial run (does `changed` fire for the first computed value at init?), and what "previous run value" is before the first run.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/callbacks/, https://releases.slint.dev/1.18.0/docs/slint/reference/common/

##### Globals
- [ ] Declaration — `global Name { ... }` at top level of a .slint file; `export global Name` to import elsewhere; never instantiated as an element (`Name { }` in a tree = error).
- [ ] Allowed contents only: properties (in/out/in-out/private, with bindings), callbacks (optionally pure), functions (optionally pure; private/protected/public). Not allowed: elements, children, `@children`, `animate`, `states`, transitions, `init` (each a compile error).
- [ ] Access — `Name.property`, `Name.callback(args)`, `Name.function(args)` from any binding/handler/function in the project and from other globals; `-`/`_` interchangeable.
- [ ] Singleton per component tree; separate windows (and a window plus an exported SystemTrayIcon) each get independent instances initialized separately.
- [ ] No scale factor or default font size in a global: expressions cannot convert px to phx or rem to logical lengths (error).
- [ ] Native exposure — a global re-exported from the file that exports the root component is exposed through the native API (get/set properties, set callback handlers: Rust `app.global::<Logic>().set_the_value(42)`, C++ `app->global<Logic>()`, JS `app.Logic.the_value`, Python `app.Logic`).
- [ ] Global property/callback can be target of `<=>` to re-expose it on a component (`in-out property the-value <=> Logic.the-value;`, `pure callback f <=> Logic.f;`).
- [ ] Property visibility applies to natives: `in` set only, `out` read only (docs: Logic uses `in-out`).
- [ ] OPEN: global initialization time and order between globals with cross-references; property binding evaluation of globals before any window exists.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/globals/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/globals/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/two-way-bindings/

##### Repetition and conditionals
- [ ] `for name[index] in model : id := Element { }` — `name` and `[index]` each optional and independent; `: id :=` optional; body is exactly one element (may contain children, nested `for`/`if`); space before `:` optional.
- [ ] `name` is a read-only pseudo-property in scope in the element and its children; `index` is the zero-based position, type `int`.
- [ ] Model expression is evaluated in the scope outside the `for` (name/index not visible in it).
- [ ] Model kinds — integer N (repeat N times; `name` is the index, type int); array/model (one instance per entry, `name` has element type); array literal `[a, b, c]`; any other type = error.
- [ ] Model changes — instances instantiated, removed or updated to match.
- [ ] Model expression is a pure context (no side effects).
- [ ] `if condition : id := Element { }` — condition must be bool; element instantiated when true, removed when false; single element body.
- [ ] Both only valid as children inside another element; not as component root, not in a global or interface (errors).
- [ ] `@children` cannot appear inside `for` or `if` bodies (error).
- [ ] ListView: only a single `for` as the only child (second `for`, plain element or `if` = error).
- [ ] `for`/`if` allowed inside VerticalLayout/HorizontalLayout and GridLayout (grid: `row`/`col`/`rowspan`/`colspan` can be set at runtime).
- [ ] Two-way binding inside `for` may reference the model value and stays in sync.
- [ ] Models supplied by native code or declared as `[{foo: string, col: color}]` struct arrays; array ops (length, index, push, insert, remove) are documented elsewhere.
- [ ] OPEN: re-instantiation policy — when the model is replaced vs changed row-wise, are existing instances reused and re-bound, are `init` handlers re-run, is element state (focus, scroll, animation) preserved; the docs only say "instantiated, removed, or updated".
- [ ] OPEN: `if` condition flips twice within one frame (instantiate then destroy?); order of instances created from a changing model; out-of-range index; negative integer model; float/NaN integer model.
- [ ] OPEN: lazy instantiation of repeater content (virtualization in ListView) is not specified.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/repetition-and-conditional-elements/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/repetition-and-data-models/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/positioning-and-layouts/

##### Container components
- [ ] `@children` — expression standing for use-site children; no arguments; `@` followed by anything other than `children` = error; appears where a child element would.
- [ ] Default placement without `@children`: use-site children appended after the root element's own children (also true for components with no `@children` at all).
- [ ] With `@children`: children are inserted at that exact position among the enclosing element's children as its children; sibling order around is preserved (e.g. `Row { @children }` in a GridLayout).
- [ ] At most one `@children` across the whole component hierarchy: second in same element or another element of same component = error.
- [ ] Not allowed inside `for`, `if`, `match`, or `ComponentContainer` (errors).
- [ ] Inserted children participate as if written there: inside a layout they become layout cells.
- [ ] Bindings and callbacks of use-site children are evaluated in the use-site scope (their `root`/ids refer to the use-site component, not the container).
- [ ] OPEN: interaction with `inherits` — if base has `@children` and derived adds children; if the derived component also contains `@children`; `parent` of inserted children for percentage sizes and `parent.width`.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/container-components/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/positioning-and-layouts/

##### Geometry and positioning
- [ ] Geometry properties on every visual element — `x`, `y`, `width`, `height` of type length (default value listed as 0px in common, but actual defaults below).
- [ ] `x`,`y` are offsets from the parent's top-left; `width`,`height` extend right/down; absolute position = sum of parent offsets up the tree.
- [ ] Defaults when unbound — `x = (parent.width - self.width) / 2`, `y = (parent.height - self.height) / 2` (centering); width/height depend on type: Rectangle, TouchArea, FocusScope, Flickable, SwipeGestureHandler, ScaleRotateGestureHandler and layouts fill the parent (stretch factor 1); Image, Text, most widgets and custom components without a base use content/preferred size.
- [ ] Elements inside a layout are positioned and sized by the layout, not by these defaults (explicit x/y of a child exempt it from the layout constraints propagation).
- [ ] Root element x/y are meaningless; binding them is deprecated; window size set through `width`/`height`.
- [ ] Percentage — `width: 50%` is `parent.width * 50%`; applies to width/height (and preferred-width/height, e.g. `100%` = parent size).
- [ ] Units — `px` logical, `phx` physical (scaled by device pixel ratio); number with no unit rejected for lengths (`x: 0;` appears in a transform example only for Image: OPEN whether bare 0 converts).
- [ ] Constraints — `min-width`, `min-height`, `max-width`, `max-height`, `preferred-width`, `preferred-height`, `horizontal-stretch`, `vertical-stretch`; preferred size defaults to the bigger preferred size of children whose x/y are unset; computed child to parent like min/max.
- [ ] Layouts: `VerticalLayout`, `HorizontalLayout`, `GridLayout` (children get `row`, `col`, `rowspan`, `colspan`; `Row { }` sub-element); common `spacing`, `padding`, `padding-left/right/top/bottom`, `alignment` (stretch default, start, end, center, space-between, space-around).
- [ ] Stretch algorithm — size to minimum, share the extra space proportional to stretch factor (float); never above max; factor 0 stays at minimum unless all others are 0 or maxed; default factor 0 for content-sized elements, 1 for parent-sized.
- [ ] `z` (float, default 0) — stacking among siblings; higher drawn on top and receives input first; ties keep declaration order.
- [ ] `opacity` (float default 1; 0..1 or percent) — applied to the element and children as an offscreen layer composited at that opacity.
- [ ] `visible` (bool default true) — false hides element and children (not drawn, no mouse input) but still occupies layout space.
- [ ] `absolute-position` (out Point {x,y}) — read-only position within the enclosing Window/PopupWindow; reference frame unspecified.
- [ ] Transforms — `transform-rotation` (angle, default 0deg), `transform-origin` (Point, default centre), `transform-scale` (float, default 100%), `transform-scale-x`/`-y` default to `scale`; not affecting layout geometry; not supported by the software renderer.
- [ ] `cache-rendering-hint` (bool default false) — renderer hint only; `dialog-button-role` enum (none default, accept, reject, apply, reset, help, action).
- [ ] OPEN: width/height default of an element with children but no base; clamping of negative sizes; rounding of fractional px to physical pixels; z-order combined with input hit testing across non-siblings; `opacity` outside 0..1 clamp.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/geometry/, https://releases.slint.dev/1.18.0/docs/slint/reference/common/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/positioning-and-layouts/

##### States and transitions
- [ ] `states [ name when cond : { change; ... } ... ]` — one statement in an element body; allowed on any element (root, child, subcomponent root, `for` element); several `states` blocks on one element merge into one set; not in globals (error).
- [ ] State entry: name, optional `when` boolean expression, `:` and a braced body; state without `when` is never selected (only usable as transition target).
- [ ] State names are element-local identifiers.
- [ ] Condition may reference any property in scope, including other elements'.
- [ ] Selection — at most one active state; if several conditions hold the first in source order wins; if none holds no state is active and all properties keep their own bindings.
- [ ] Property change inside a state — `name: value;` (unqualified = property of the owner element, i.e. same as `root.background`; qualified by element id `indicator.background`); name must resolve to a property; the value is a binding.
- [ ] While active, listed properties take the state's value instead of their own binding.
- [ ] Imperative assignment to a property that a state drives replaces the state-provided binding; the property then keeps the assigned value regardless of the active state.
- [ ] A property that has a `<=>` binding cannot be changed in a state (error).
- [ ] Transitions — inside a state: `in { ... }`, `out { ... }`, `in-out { ... }` (`in_out` spelling accepted); may be repeated; sit alongside property changes.
- [ ] Transition body contains only `animate` clauses (anything else = error); `animate * { ... }` matches every property changed by the state (`*` only valid inside a transition); `animate a, b.c { ... }` lists properties (may be qualified inside transitions).
- [ ] Named properties must be among those changed by that state (else error); a property that already has its own `animate` cannot also be driven by a transition (error).
- [ ] Legacy standalone `transitions [ ... ]` block is not supported.
- [ ] `in` plays when entering the state, `out` when leaving it, `in-out` both.
- [ ] OPEN: transition when going directly state A to state B (does A's `out` or B's `in` apply, both, which wins, how they combine on the same property).
- [ ] OPEN: animation retargeting if state flips mid-transition; interaction between a state-driven value change and a property's own `animate` (docs: error only if explicitly combined with transition animate; plain `animate` on the property still plays for state changes per Animations page).
- [ ] OPEN: whether `when` conditions are evaluated eagerly (state changes fire without anyone reading) and in which order relative to bindings/changed handlers.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/states-and-transitions/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/states/

##### Animations
- [ ] `animate prop1, prop2 { field: value; ... }` — statement in an element body (root, child, subcomponent root); whenever the property value changes it moves to the target over time instead of jumping; applies to changes from bindings, states, or native code.
- [ ] Names refer to properties of the element carrying the block; a qualified name `other.prop` outside a transition is an error; body may only contain `field: value;` bindings, other statements = error; unknown field name = error; all fields optional.
- [ ] Two `animate` blocks for the same property = error; comma list equals one block per property.
- [ ] Animatable types: int, float, length, physical-length, color, brush, angle; any other type (string, bool, image, struct) = error; property must be settable on the element (a private property of another component = error).
- [ ] Fields — `delay` (duration, 0ms), `duration` (duration, 0ms), `iteration-count` (float, 1; negative = infinite; fractional allowed), `easing` (default linear), `direction` (enum AnimationDirection: normal default, reverse, alternate, alternate-reverse, CSS semantics), `enabled` (bool, true; false = target applied immediately, no delay/easing/iteration).
- [ ] Field values are bindings (expressions) evaluated, so `enabled: some-prop` is reactive — OPEN: whether fields are re-read mid-animation.
- [ ] Easing — named curves such as `linear`, `ease-out-bounce` appear here; full set and `cubic-bezier(a,b,c,d)` argument rules live on the easing type page, not in scope — OPEN: list.
- [ ] `spring` / `spring(bounce)` — bounce a number literal in [-1, 1] (else error); `spring` == `spring(0)`; natural frequency w0 = 2*pi/duration, damping ratio zeta = 1 - bounce (mass-spring-damper model).
- [ ] Bounce semantics — 0 critically damped (no overshoot, fastest), >0 underdamped (overshoot, closer to 1 slower to settle), <0 overdamped (slow, no overshoot); bounce 1 gives period exactly `duration`.
- [ ] Spring is not guaranteed settled at `duration`; with finite iteration-count total time clamped to at most (iteration-count + 9) x duration (if unsettled at end of last iteration it is re-damped to settle within a further 9x duration); `iteration-count: -1` never clamped.
- [ ] Spring retarget keeps velocity (smooth), unlike other easings which restart their curve.
- [ ] Repeating springs (iteration-count > 1) switch to the next iteration at exactly `duration`, velocity carries over; same-direction repeat resets position to the start; direction flip (alternate/alternate-reverse) continues from the actual position.
- [ ] Spring with bounce 1 and iteration-count -1 and no alternate = seamless infinite loop.
- [ ] In transitions, `animate` may use qualified names (`animate root.background { }`) and `*`; fields identical.
- [ ] `animation-tick()` built-in is referenced for continuously running values independent of property changes (defined elsewhere).
- [ ] Not allowed in globals (error).
- [ ] OPEN: animation sampling — time source, frame stepping, interpolation of color/brush (linear sRGB or premultiplied?), brush gradient interpolation, angle shortest path, int rounding; behaviour of retargeting a non-spring animation mid-flight (restart from current value?); `delay` with `direction: reverse`; `iteration-count` 0.
- [ ] OPEN: queries during animation return the interpolated current value (assumed) and dependents see intermediate values; whether `changed` handlers fire per frame.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/language/animations/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/animation/, https://releases.slint.dev/1.18.0/docs/slint/reference/language/states-and-transitions/

##### Timer
- [ ] `Timer { interval: 1s; running: true; triggered() => { ... } }` — non-visual pseudo-element; must be placed inside an element body; no common visual properties.
- [ ] `interval` (duration, default 0ms) — mandatory; changing it while running restarts the timer.
- [ ] `running` (bool, default true) — false stops, true starts; may be a binding; can be assigned from handlers (`self.running = false;`, `timer.running = true;`) — assignment from a handler removes any binding on it.
- [ ] `triggered()` callback — fires every tick (each interval).
- [ ] Functions `start()` (same as running = true), `stop()` (running = false), `restart()` (restarts if previously started).
- [ ] Timer accepts own `property` declarations (`property <int> count`) and `self` refers to the Timer.
- [ ] OPEN: first tick at one interval after start (assumed, not stated); interval 0ms semantics (every frame? never?) given "mandatory"; whether `restart()` on a stopped timer is a no-op (doc: only if previously started); catch-up of missed ticks; ordering of timer ticks vs queued `changed` handlers and repaint within one event-loop iteration; `running` binding toggled inside `triggered`.
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/timer/

##### Misc runtime items
- [ ] `debug(...)` built-in prints to console (used in handlers).
- [ ] Reactivity-vs-React: no component destroy/recreate on state change; fine-grained updates of only dependents; property assignment inside handlers uses `count += 1`.
- [ ] Native boundary: `pure` callbacks and public functions of exported components visible to native; `in` set by app, `out` read by app.
- [ ] Models passed from native code to `for` are observed (rows added/removed/changed notify the repeater) — OPEN: exact notification protocol not in scope files.
Source: https://releases.slint.dev/1.18.0/docs/slint/guide/language/concepts/reactivity-vs-react/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/concepts/slint-language/, https://releases.slint.dev/1.18.0/docs/slint/guide/language/coding/repetition-and-data-models/

#### 8.4.3 Built-in elements, input, windows and layouts

# Slint 1.18.0: elements, layouts, input, windows (clean-room surface checklist)

Base URL: https://releases.slint.dev/1.18.0/docs/slint/

##### Common properties (all visual items)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/common/
- [ ] Applies to all visual items (Rectangle, Text, layouts, ...); non-visual items (Timer etc.) do not have these.
- [ ] Geometry — x, y: length (in, default 0px), relative to parent; width, height: length (in, default 0px), explicit value overrides default size.
- [ ] z: float (in, default 0.0) — higher z drawn on top of siblings AND receives input first; equal z keeps declaration order.
- [ ] transform-rotation: angle (default 0deg); transform-origin: Point (default {x: self.width/2, y: self.height/2}); transform-scale: float (default 100%); transform-scale-x, transform-scale-y: float (default self.scale... i.e. transform-scale).
- [ ] Transform semantics — rotation/scale are around transform-origin, apply to the element and all children, do NOT change width/height (render-only), not available in the software renderer.
- [ ] opacity: float (default 1) — 0..1 or percentage; applied to whole subtree as if rendered into an intermediate layer then composited (children overlap is not double-blended).
- [ ] visible: bool (default true) — false: element and children not drawn and get no mouse input, but still occupy layout space.
- [ ] absolute-position: Point (out) — position relative to enclosing Window/PopupWindow; reference frame stated as unspecified.
- [ ] cache-rendering-hint: bool (default false) — hint to cache subtree into an intermediate layer (perf only, no visual change).
- [ ] dialog-button-role: enum DialogButtonRole (default none) — none, accept, reject, apply, reset, help, action; puts element into Dialog button row.
- [ ] Point struct: x, y: length. Edges struct: left, top, right, bottom: length. Size struct: width, height: length.
- [ ] init() callback — on every element; runs after construction once all properties hold final binding values; order inner-to-outer (child before parent; sibling declaration order shown in doc example: MyButton inner init, then MyCheckBox, then outer instance init); cannot be set from host code.
- [ ] init and `if`/`for` — dynamically created components (behind `if`) run init when instantiated.
- [ ] Layout constraint props valid on all visible items (declared in layouts overview page): min-width/min-height, max-width/max-height, preferred-width/preferred-height: length (default 0px); horizontal-stretch/vertical-stretch: float (in-out, default 0.0); col,row,colspan,rowspan: int; layout-order: int (default 0); cross-axis-self-alignment: CrossAxisAlignment (default auto).
- [ ] Accessibility: accessible-role must be set to use any other accessible-* property or callback; should be constant. Enum AccessibleRole: none, button, checkbox, combobox, groupbox, image, list, slider, spinbox, tab, tab-list, tab-panel, text, table, tree, progress-indicator, text-input, switch, list-item, radio-button, radio-group, window-title-bar, banner, complementary, content-info, form, main, navigation, region, search. Default none (Text -> text, Image -> image, TextInput -> text-input).
- [ ] accessible-checkable: bool (false); accessible-checked: bool (false); accessible-description: string (""); accessible-enabled: bool (true); accessible-expandable: bool (false); accessible-expanded: bool (false); accessible-id: string (""); accessible-label: string (""; Text defaults to its text); accessible-orientation: Orientation (horizontal|vertical, first = horizontal); accessible-live-region: AccessibleLiveness (off|polite|assertive, default off); accessible-value-maximum/minimum/step: float (0); accessible-value: string (""); accessible-placeholder-text: string (""); accessible-read-only: bool (false); accessible-item-selectable: bool (false); accessible-item-selected: bool (false); accessible-item-index: int (0); accessible-item-count: int (0).
- [ ] Accessibility callbacks: accessible-action-default(); accessible-action-set-value(string); accessible-action-increment(); accessible-action-decrement(); accessible-action-expand(); accessible-action-set-selection-offsets(anchor: int, focus: int) — UTF-8 byte offsets, focus may precede anchor, equal = caret move.
- [ ] OPEN: percentage values for x/y/width/height (e.g. `width: 50%`) — only "100% of parent" is mentioned for non-layout elements; semantics of other percentages for geometry must be pinned by tests.
- [ ] OPEN: transform-origin default expression uses self.width/height, so it is reactive; behavior with non-integer sizes and fractional pixel rounding unspecified.
- [ ] OPEN: opacity / transforms interaction with hit-testing (does input follow transformed geometry?) not stated.
- [ ] OPEN: z-order and input: ties and nested z across non-siblings (children vs. parent's siblings) unspecified.

##### Rectangle
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/elements/rectangle/
- [ ] Not in layout: width and height default to 100% of parent.
- [ ] Paint: nothing if no background and no border; translucent background shows content below.
- [ ] Properties: background: brush (default transparent), border-color: brush (transparent), border-width: length (0), clip: bool (false; true clips children to element bounds, otherwise children may draw outside).
- [ ] Radius: border-radius: length (0, all four corners); border-top-left-radius, border-top-right-radius, border-bottom-left-radius, border-bottom-right-radius: length (0px); radius = width/2 on square gives circle.
- [ ] Drop shadow: drop-shadow-blur: length (0px, negative ignored, 0 = no blur), drop-shadow-color: color (transparent), drop-shadow-offset-x, drop-shadow-offset-y: length (0px), drop-shadow-spread: length (0px; grows/shrinks before blur; Skia renderer only). Equivalent to CSS box-shadow.
- [ ] Inner shadow (Skia only): inner-shadow-blur, inner-shadow-color, inner-shadow-offset-x, inner-shadow-offset-y, inner-shadow-spread — same types/defaults; CSS `inset` equivalent; drawn inside the element's geometry.
- [ ] OPEN: border is drawn inside the element's box (does border-width reduce the content/clip area? clip with rounded corners?).
- [ ] OPEN: radius clamping when radius exceeds half size; per-corner vs border-radius precedence.
- [ ] OPEN: drop shadow with clip: true and with opacity; blur kernel (Gaussian sigma = blur/2?) unspecified.
- [ ] OPEN: gradient brushes (@linear-gradient, @radial-gradient) rendering precision, belongs to brush spec but used here.

##### Image
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/elements/image/
- [ ] source: image (default empty image, draws nothing); `source.width`, `source.height` give pixel size of the source image.
- [ ] colorize: brush (default transparent) — when set, image is used as alpha mask painted with that brush.
- [ ] image-fit: ImageFit (fill|contain|cover|preserve) — default `contain` when Image is in a layout, `fill` otherwise.
- [ ] ImageFit semantics: fill stretches to w/h; contain scales preserving aspect to fit; cover scales preserving aspect to cover, clipped; preserve keeps logical pixel size (still times window scale factor), extra space blank.
- [ ] image-rendering: ImageRendering (smooth = linear interpolation, pixelated = nearest neighbour), default smooth.
- [ ] horizontal-alignment: ImageHorizontalAlignment (center|left|right, default center); vertical-alignment: ImageVerticalAlignment (center|top|bottom, default center).
- [ ] horizontal-tiling, vertical-tiling: ImageTiling (none|repeat|round, default none); round = repeat and rescale to get integer count.
- [ ] source-clip-x, source-clip-y: int (0); source-clip-width: int (source.width - clip-x); source-clip-height: int (source.height - clip-y) — region of source, in source pixels.
- [ ] Accessibility: accessible-role defaults to image; set accessible-role: none for decorative; accessible-label for alt text.
- [ ] OPEN: implicit (preferred/min) size of an Image in a layout (presumably source size) — docs silent; also SVG sizing, @image-url scale-factor selection (@2x).
- [ ] OPEN: interaction of image-fit/alignment with source-clip and tiling; rounding of scaled size.
- [ ] OPEN: pixel-exact smooth scaling filter (bilinear vs. other) and colorize blend math.

##### Path
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/elements/path/
- [ ] Not in layout: width/height default to 100% of parent when not specified; shape drawn relative to x,y; if width and height non-zero, the whole shape is scaled to fit.
- [ ] fill: brush (transparent); fill-rule: FillRule (nonzero|evenodd, default nonzero); stroke: brush (transparent); stroke-width: length (0px); stroke-line-cap: LineCap (butt|round|square, default butt); stroke-line-join: LineJoin (miter|round|bevel, default miter); stroke-miter-limit: float (4; exceeded -> bevel).
- [ ] width, height: length (0px) — if non-zero path is scaled to fit; fit: ImageFit (default contain) — how the view box maps to the element size; clip: bool (false) — clip to view box; anti-alias: bool (true).
- [ ] viewbox-x, viewbox-y, viewbox-width, viewbox-height: float (0.0); if viewbox-width or -height <= 0 the bounding rect of all path elements is used instead.
- [ ] Functions: point-at(t: float) -> Point (path coordinate space; (0,0) if empty; t outside 0..1 wraps to its fractional part, 1.5->0.5, 2.0->1.0); angle-at(t: float) -> angle (degrees of tangent vs x-axis, direction of definition; 0 if empty; same wrapping).
- [ ] commands: string (default "") — SVG path data; settable only in a binding, cannot be read in expressions.
- [ ] Path sub-elements (unitless coordinates in path space): MoveTo{x,y}; LineTo{x,y}; ArcTo{radius-x, radius-y, x, y, x-rotation (deg), large-arc: bool false, sweep: bool false (true = clockwise)}; CubicTo{control-1-x, control-1-y, control-2-x, control-2-y, x, y}; QuadraticTo{control-x, control-y, x, y}; Close{} — all floats default 0.0. MoveTo closes the current sub-path and starts a new one.
- [ ] OPEN: SVG path grammar coverage (relative commands, H/V/S/T, implicit repeated commands, error handling for malformed strings).
- [ ] OPEN: arc radius scaling when radii too small (SVG spec rule assumed), stroke bounds vs. view box fitting, whether stroke-width scales with fit.
- [ ] OPEN: path implicit size in a layout (bounding box?) and nonzero width with zero height.

##### Text
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/elements/text/
- [ ] Implicit size: min-width, min-height, preferred-width, preferred-height fit the full text laid out on a single line (unless explicit line breaks). With wrap: word-wrap and/or overflow: elide, min-width becomes 0; preferred sizes unchanged.
- [ ] Properties: text: string (""); color: brush (theme dependent); font-size: length (0px -> falls back to Window default-font-size); font-weight: int (0 -> default; 100..900, 400 normal; FontWeight namespace constants); font-family: string (""); font-italic: bool (false).
- [ ] horizontal-alignment: TextHorizontalAlignment (start|end|left|center|right; default first = start); vertical-alignment: TextVerticalAlignment (top|center|bottom; default top).
- [ ] overflow: TextOverflow (clip|elide; default clip; elide uses "..." char); wrap: TextWrap (no-wrap|word-wrap|char-wrap; default no-wrap; char-wrap only Qt and software renderers; word-wrap breaks inside very long words).
- [ ] max-lines: int (0) — <=0 unlimited; wrapped lines count; with elide the ellipsis goes on the last visible line.
- [ ] letter-spacing: length (0px, may be negative); line-height-factor: float (1; also percentage; multiple of natural line height ascent+descent+linegap; 0 collapses lines; negative/non-numeric act like 1).
- [ ] stroke: brush (transparent); stroke-width: length (0px; zero = hairline of 1 physical pixel); stroke-style: TextStrokeStyle (outside|center, default outside).
- [ ] font-metrics: FontMetrics (out) — ascent, descent (usually negative), x-height, cap-height: length (x-height/cap-height 0 if font lacks them), scaled to the element's font pixel size.
- [ ] Accessibility defaults: accessible-role text, accessible-label = text.
- [ ] OPEN: text measurement — glyph advances, kerning, line breaking (Unicode UAX#14?), bidi/shaping; clean-room must use a font stack; exact pixel metrics to be pinned vs Slint.
- [ ] OPEN: default font when font-family empty and no Window default; default size when font-size 0 and default-font-size 0.
- [ ] OPEN: wrapped text preferred-height as function of assigned width (height-for-width); how layouts re-query it.
- [ ] OPEN: vertical placement of baseline within line box, line-height-factor distribution above/below, elide with wrap and max-lines, elision position for rtl.
- [ ] OPEN: `start`/`end` alignment resolution (text direction detection).

##### StyledText
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/elements/styledtext/
- [ ] text: styled-text type (default "") built via @markdown("...") (supports \{expr} interpolation); subset of CommonMark.
- [ ] Supported: italics, strikethrough, inline code, links, ordered/unordered lists (built in); underline via <u>; colour via <font color="...">. Unsupported: headings, images, tables, block quotes, sub/superscript, horizontal rules, footnotes, math, other HTML tags.
- [ ] Properties: default-color: brush (theme); default-font-family: string ("" -> Window default-font-family); default-font-size: length (0px -> Window default-font-size); horizontal-alignment (TextHorizontalAlignment); vertical-alignment (TextVerticalAlignment); link-color: color (#0000ff); max-lines: int (0 = unlimited).
- [ ] Callback: link-clicked(link: string).
- [ ] OPEN: bold (`**`) not listed in the feature table though standard CommonMark; confirm. Paragraph spacing, list indent/bullet glyphs, link hit-testing and cursor, `wrap`/`overflow` presence (none listed), implicit size.

##### TouchArea
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/gestures/toucharea/
- [ ] Not in layout: width/height default to 100% of parent.
- [ ] Properties: enabled: bool (true; disabled -> events pass to elements underneath); has-hover: bool (out); mouse-cursor: MouseCursor (default `default`); mouse-x, mouse-y: length (out); pressed-x, pressed-y: length (out, position at last press); pressed: bool (out).
- [ ] Callbacks: clicked() (press then release on element, finger or left button); double-clicked() (clicked fires first); moved() (only while pressed/touching); pointer-event(event: PointerEvent); scroll-event(event: PointerScrollEvent) -> EventResult (reject = forward to parent, accept = consumed).
- [ ] PointerEvent struct: button: PointerEventButton, kind: PointerEventKind, modifiers: KeyboardModifiers, touch-finger-id: int (0 = not touch).
- [ ] PointerScrollEvent struct: delta-x, delta-y: length, modifiers: KeyboardModifiers. EventResult enum: reject, accept.
- [ ] OPEN: PointerEventButton / PointerEventKind / MouseCursor / KeyboardModifiers enum members are documented elsewhere (builtin enums page); not in this scope's pages.
- [ ] OPEN: which TouchArea gets input when overlapping (topmost by z/declaration), hover/press when pointer leaves while pressed (grab), double-click time/distance thresholds, whether TouchArea blocks events to elements below even if no callback is set.

##### Flickable
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/gestures/flickable/
- [ ] Low-level base of ScrollView/ListView; no scrollbar. Scrollable when content-width/height exceed own width/height.
- [ ] Not in layout: width/height default to 100% of parent. Maximum and preferred size derive from content size.
- [ ] content-width, content-height: length (0px; auto-computed from children when unset, except children generated by `for` — known bug #407); content-x, content-y: length (in-out, 0px, usually negative = scroll offset).
- [ ] interactive: bool (true; false -> no user panning by mouse or touch, events forwarded below); mouse-drag-pan-enabled: bool (true; false -> mouse events forwarded below, touch still pans).
- [ ] Callback flicked() — content-x/y changed by user action (drag, wheel).
- [ ] Press handling over a TouchArea: press stored; if 100ms pass with no event -> delivered to TouchArea; if release <100ms -> press+release delivered immediately; a move starts flicking if before 500ms since press AND distance > 8 logical px in a movable direction; then TouchArea gets an exit event.
- [ ] Press not over a TouchArea: flick immediately when euclidean distance > 8 logical px.
- [ ] Flickable only intercepts a press if it can pan (content larger than self, or currently scrolled off origin); otherwise forwarded below.
- [ ] Wheel/touchpad scroll works regardless of interactive and mouse-drag-pan-enabled; intercepted only if it can scroll in that direction, else forwarded to parent.
- [ ] OPEN: kinetic/inertial scrolling, overscroll bounce, clamping of content-x/y to range, wheel step size, axis lock thresholds.

##### SwipeGestureHandler
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/gestures/swipegesturehandler/
- [ ] Recognises touch swipes and mouse drags within element geometry; wrap it around children (not a preceding sibling).
- [ ] Properties: enabled: bool (true); pressed-position: Point (out); current-position: Point (out); swiping: bool (out, true while recognised); handle-swipe-left/right/up/down: bool (false each).
- [ ] Callbacks: moved(); swiped() (after recognised and pointer released); cancelled() (programmatic cancel or window focus loss). Function: cancel().
- [ ] Press forwarded to children after a small delay; once pointer moves > 8 logical px in an enabled direction the gesture is recognised and children stop receiving events.
- [ ] OPEN: delay length, behavior when move occurs in a disabled direction, sign conventions (swipe-left = pointer moves left).

##### ScaleRotateGestureHandler
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/gestures/scalerotategesturehandler/
- [ ] Pinch/rotate on touchscreens (all platforms) and trackpad (macOS, iOS); recognition limited to element geometry.
- [ ] Properties: enabled: bool (true; disabling cancels ongoing gesture); active: bool (out); scale: float (out, cumulative, 1.0 at start and when inactive); rotation: angle (out, cumulative, 0deg at start/inactive, positive = clockwise); center: Point (out, handler coordinates; midpoint of two fingers, or cursor for trackpad).
- [ ] Callbacks: started(), updated() (scale/rotation/center changed), ended() (fingers lifted), cancelled() (disabled mid-gesture or focus lost).
- [ ] OPEN: threshold to start recognition; rotation angle wrap-around; forwarding to children.

##### DragArea / DropArea
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/drag-and-drop/dragarea/ , https://releases.slint.dev/1.18.0/docs/slint/reference/drag-and-drop/droparea/ , https://releases.slint.dev/1.18.0/docs/slint/guide/development/drag-and-drop/
- [ ] DragArea properties: enabled: bool (true); data: data-transfer (empty); drag-image: image (empty = no overlay); drag-image-offset-x, drag-image-offset-y: int (0, hot spot in image pixels, HTML5 setDragImage convention); allow-copy, allow-move, allow-link: bool (false each; at least one needed or drag never starts, compiler warns); dragging: bool (out).
- [ ] DragArea callback: drag-finished(action: DragAction) — chosen action on drop or DragAction.none if cancelled.
- [ ] Drag start: press then move past a small threshold (value unspecified); plain click doesn't drag so child TouchAreas stay interactive; events still reach children.
- [ ] Proposed action: first allowed of move, copy, link when no modifier; Ctrl -> copy, Shift -> move, Ctrl+Shift -> link (Command instead of Ctrl on macOS).
- [ ] DropArea properties: enabled: bool (true); has-drag: bool (out; accepted drag hovering); current-action: DragAction (out; none when no drag hovering).
- [ ] DropArea callbacks: can-drop(event: DropEvent) -> DragAction (during hover; none = reject); dropped(event: DropEvent) -> DragAction (on release after can-drop returned non-none; return value reported to source via drag-finished). Runtime clamps returned action to source's allowed set (disallowed -> none).
- [ ] DropEvent fields used in docs: data: data-transfer, proposed-action: DragAction (full struct defined in builtin structs page).
- [ ] DragAction enum: none, copy, move, link. data-transfer type is opaque in .slint; created and read via host callbacks (host API: plain text conversion, has_plain_text check).
- [ ] DropArea also accepts drops from other applications on supporting platforms.
- [ ] OPEN: DropEvent has other fields (position?) not shown; drag threshold in px; cursor shapes during drag; drop hit-testing with nested DropAreas.

##### FocusScope, KeyBinding, key events
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/keyboard-input/focusscope/ , https://releases.slint.dev/1.18.0/docs/slint/reference/keyboard-input/overview/ , https://releases.slint.dev/1.18.0/docs/slint/guide/development/focus/
- [ ] FocusScope properties: has-focus: bool (out); enabled: bool (true; false = never takes focus by click, tab or programmatically, but a parent FocusScope still gets key events rejected by child scopes); focus-on-click: bool (true); focus-on-tab-navigation: bool (true).
- [ ] FocusScope functions: focus(), clear-focus().
- [ ] FocusScope callbacks: capture-key-pressed(event: KeyEvent) -> EventResult; capture-key-released(event: KeyEvent) -> EventResult; key-pressed(event) -> EventResult; key-released(event) -> EventResult; focus-changed-event(reason: FocusReason); focus-gained(reason: FocusReason); focus-lost(reason: FocusReason).
- [ ] Key event routing: delivered to focused element. First CAPTURE phase: from Window down towards focused element call capture-key-pressed/released on each; accept stops processing, reject continues. If nobody captured, call key-pressed/released on the focused element; accept = handled, reject = bubble to parent (recursively).
- [ ] A FocusScope handles key events only when it has-focus or contains a FocusScope that has-focus.
- [ ] TextInput key-pressed/key-released: invoked before TextInput's own handling; accept = handled, reject = let TextInput handle.
- [ ] KeyEvent struct: text: string (unicode of key; non-printable keys map to control chars or private-use unicode, see Key namespace), modifiers: KeyboardModifiers, repeat: bool (true for auto-repeat press; always false on release).
- [ ] KeyboardModifiers fields seen: control, plus alt, shift, meta (full struct in builtin structs).
- [ ] KeyBinding (child of FocusScope): keys: keys type (default @keys()); enabled: bool (true; disabled never consumes events or fires); callback activated().
- [ ] @keys(...) macro: Key namespace member plus modifiers joined with `+`; modifiers Meta (Control key on macOS, Windows key on PC), Control (Command on macOS), Shift, Alt (Option on macOS); optional modifier via trailing `?` (Shift?, Alt?) = ignore that modifier.
- [ ] Logical key semantics: matches the character produced on current layout, not physical position; numpad digits not distinguished from main row; AltGr not available as modifier. Key.A..Z are lowercase letters.
- [ ] String-literal keys `@keys(Control + "ä")`: matched against lowercased event text + modifiers, no automatic Shift handling. `@keys(Control + Plus)` == `@keys(Control + Shift? + "+")`.
- [ ] Key namespace: Backspace, Tab, Return, Escape, Backtab, Delete, Shift, Control, Alt, AltGr, CapsLock, ShiftR, ControlR, Meta, MetaR, Space, UpArrow, DownArrow, LeftArrow, RightArrow, F1..F24, Insert, Home, End, PageUp, PageDown, ScrollLock, Pause, SysReq, Stop, Menu, Back, A..Z, Digit0..Digit9, Circumflex, Exclamation, DoubleQuote, Hash, Dollar, Percent, Ampersand, Underscore, OpenParen, CloseParen, Asterisk, Plus, Pipe, HyphenMinus, OpenCurlyBracket, CloseCurlyBracket, Tilde, Colon, Semicolon, LessThan, Equals, GreaterThan, QuestionMark, At, Comma, Period, Slash, BackQuote, OpenBracket, BackSlash, CloseBracket, Quote.
- [ ] OPEN: exact private-use code points for Key constants (needed for conformance; defined in the keys namespace, not on these pages).
- [ ] OPEN: FocusReason enum members (e.g. tab, click, programmatic ...) not listed on these pages.

##### TextInput
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/keyboard-input/textinput/ , https://releases.slint.dev/1.18.0/docs/slint/reference/keyboard-input/textinputinterface/
- [ ] Low-level item; not in layout: width/height default to 100% of parent. Does NOT auto-scroll to keep cursor visible; enclosing widget must do it using cursor-position-changed.
- [ ] Properties: text: string (in-out, ""); font-family: string (""); font-size: length (0px); font-italic: bool (false); font-weight: int (0); color: brush (style dependent); selection-foreground-color, selection-background-color: color (transparent); horizontal-alignment, vertical-alignment (as Text); wrap: TextWrap (no-wrap; only meaningful when single-line false); letter-spacing: length (0); line-height-factor: float (1); page-height: length (0px; scroll amount for PageUp/PageDown); text-cursor-width: length (from widget style); has-focus: bool (out); font-metrics: FontMetrics (out).
- [ ] input-type: InputType (text default | password renders "*" | number accepts only 0-9 | decimal accepts valid decimal chars using locale decimal separator | search = rendered as text, a11y hint).
- [ ] input-method-hints: InputMethodHints {capitalization: CapitalizationMode (default sentences), auto-correct: bool (true), auto-complete: bool (true)}.
- [ ] single-line: bool (true; newline separators ignored in render); read-only: bool (false; no keyboard/mouse editing, selection and programmatic edits still work).
- [ ] Callbacks: accepted() (Enter pressed); edited() (text changed by user); cursor-position-changed(position: Point); key-pressed(event) -> EventResult; key-released(event) -> EventResult.
- [ ] Functions: set-selection-offsets(anchor: int, focus: int) (UTF-8 offsets; equal = move caret), select-all(), clear-selection(), cut(), copy(), paste(), undo(), redo(), focus(), clear-focus().
- [ ] Accessibility defaults: role text-input, accessible-value = text, accessible-enabled = enabled, accessible-read-only = read-only.
- [ ] Global TextInputInterface.text-input-focused: bool (default false) — true while any TextInput has focus (for virtual keyboards).
- [ ] OPEN: editing behavior (word boundaries, caret movement keys, shortcuts per platform, undo coalescing, IME preedit, mouse selection by double/triple click), password masking character width, number/decimal validation details.
- [ ] OPEN: implicit size of TextInput in layouts (preferred-width from text? min height from font); text-cursor-width style default; caret blink.

##### Layout algorithm (HorizontalLayout, VerticalLayout, GridLayout common)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/layouts/overview/ , https://releases.slint.dev/1.18.0/docs/slint/reference/layouts/horizontallayout/ , https://releases.slint.dev/1.18.0/docs/slint/reference/layouts/verticallayout/ , https://releases.slint.dev/1.18.0/docs/slint/reference/layouts/gridlayout/
- [ ] Per-child constraint props: min-width/min-height, max-width/max-height, preferred-width/preferred-height (length, 0px default), horizontal-stretch/vertical-stretch (float, 0.0 default, in-out). Builtin widgets use 0 or 1.
- [ ] A child with explicit width/height set keeps a fixed size on that axis; otherwise size is computed by the layout respecting min, max and stretch.
- [ ] Stretch semantics: stretch 0 means the element isn't stretched unless all elements in the layout are 0 (then free space shared; see alignment: stretch).
- [ ] Spacing: spacing: length (0px) between adjacent children (Horizontal/Vertical).
- [ ] Padding: padding: length (0px, all sides) plus padding-left, padding-right, padding-top, padding-bottom: length (0px) override per side; padding is inside the layout's own box.
- [ ] alignment: LayoutAlignment (default first = stretch) — main-axis distribution: stretch = start from MIN size of all elements and distribute remaining space by *-stretch factors; center = preferred sizes, free space split before first and after last; start = preferred sizes, free space after last; end = preferred, free space before first; space-between = free space between elements; space-around = between elements with half spaces at both ends; space-evenly = equal space before, between and after.
- [ ] cross-axis-alignment: CrossAxisAlignment (values auto|stretch|start|end|center; unset == stretch; setting `auto` explicitly is an error on the container) — stretch fills full cross extent; start/end/center size each item to its preferred cross size clamped to min/max and place it at start/end/centre of the layout's content box (inside padding).
- [ ] Per-child cross-axis-self-alignment: CrossAxisAlignment (default auto = use container's) valid in Horizontal/Vertical/Flexbox children.
- [ ] layout-order: int (0) on children — sort ascending (stable, ties keep declaration order); visual order only, focus order stays declaration order.
- [ ] GridLayout: spacing (both axes), spacing-horizontal, spacing-vertical: length (0px); padding / per-side padding (0px). Cells cover the whole surface, children stretched to the allocated cell unless their min/max constraints oppose; "cells are not aligned".
- [ ] Grid cell props (must be compile-time constants): row: int (auto), col: int (auto), rowspan: int (1), colspan: int (1). `Row {}` wrapper assigns row index implicitly, children get sequential columns; `col` overrides sequence (skip a column; next implicit column continues after it); setting `row` resets column to 0 unless col explicit; without Row/row each new child takes next column in the same row.
- [ ] Elements not in a layout: positioned by their own x,y,width,height; Rectangle/Path/Flickable/TouchArea/TextInput/WindowMoveArea default to 100% of parent size when unspecified (parent = layout-less parent element).
- [ ] Layout element itself: when not inside another layout, takes the size of its parent; as a layout item it reports min/preferred/max aggregated from children (sum along main axis + spacing + padding; max across children on cross axis — inferred, not stated).
- [ ] Window geometry is restricted by root layout constraints: explicit width/height = fixed; min/max honored by window manager; initial size from preferred-width/height.
- [ ] OPEN: aggregation formula of child min/pref/max into a layout's own min/pref/max (sum vs max, spacing and padding inclusion) — docs silent.
- [ ] OPEN: overflow when sum of min sizes exceeds available space (clip? overlap? children keep min) and when max sizes sum below space under alignment: stretch.
- [ ] OPEN: distribution when some items hit max and stretch redistribution; weight with all-zero stretch (even split? by preferred?); how preferred size affects stretch mode (start from min size then grow vs. start from preferred).
- [ ] OPEN: integer vs fractional pixel rounding of child positions/sizes (logical px, physical rounding).
- [ ] OPEN: percentage sizes inside layouts (`width: 50%` of what — parent layout box?) and implicit size of Rectangle with no width/height inside a layout (min 0, pref 0, stretch? 0).
- [ ] OPEN: GridLayout track sizing (per-column min = max of child mins, stretch = max of child stretches?), rowspan/colspan distribution of spanned constraints, empty rows/cols, negative or overlapping cell placement.
- [ ] OPEN: invisible (visible: false) children still take layout space — confirm same for opacity 0; `if`/`for` children ordering relative to siblings.
- [ ] OPEN: layout-order with `for` repeaters; interaction with row-reverse.

##### FlexboxLayout
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/layouts/flexboxlayout/
- [ ] Algorithm is delegated to the taffy crate (CSS Flexbox); a clean-room port must reproduce taffy results (no separate spec in docs). Prefer HorizontalLayout/VerticalLayout for single line unless runtime direction change or reverse directions needed.
- [ ] flex-direction: FlexboxLayoutDirection (row default | row-reverse | column | column-reverse); reverse = right-to-left / bottom-to-top starting at far edge.
- [ ] flex-wrap: FlexboxLayoutWrap (wrap default | no-wrap | wrap-reverse) — default is wrap, unlike CSS nowrap.
- [ ] alignment: LayoutAlignment (default start) — CSS justify-content on the main axis; with stretch, items grow weighted by horizontal-stretch (row) or vertical-stretch (column); all factors 0 -> free space split evenly; max-width/max-height caps growth and space a capped item can't take stays free at line end.
- [ ] cross-axis-line-alignment: LayoutAlignment (default stretch) — CSS align-content; for stretch the flex lines have no stretch factor and share leftover space equally; other values place lines like they place items in `alignment`.
- [ ] cross-axis-alignment: CrossAxisAlignment (default stretch) — CSS align-items within one line; per-item cross-axis-self-alignment (default auto) = align-self.
- [ ] spacing: length (0px, both gaps); spacing-horizontal (column-gap), spacing-vertical (row-gap): length (0px).
- [ ] padding / padding-left / -right / -top / -bottom: length (0px).
- [ ] Per-item: layout-order (CSS order), horizontal-stretch/vertical-stretch (flex-grow weight, needs alignment: stretch), max-width/max-height (growth cap), min-width/min-height (refuses shrink), preferred-width (row) / preferred-height (column) = flex-basis; flex-shrink: every item shrinks in proportion to its preferred size (no opt-out).
- [ ] Wrap column container: reports total width of all columns only when its height is a plain length (px/pt/cm... not phx, not rem, not percentage, not expression, not height assigned by a parent layout, not height on the root of a reused component); otherwise reports width of a single column. A wrap column never wraps into columns wider than its width (overflows downward); wrap-reverse still wraps. Give more width (horizontal-stretch or min-width) to wrap without a set height.
- [ ] OPEN: the compile-time rule "height is a plain length" needs static analysis in the compiler; behavior in all other cases is to be diffed against Slint.
- [ ] OPEN: taffy version/rounding behavior (taffy rounds layout to whole pixels by default), baseline alignment not offered, min-content/auto sizing of Text in flex items, percentage basis.

##### Focus chain and tab order
Source: https://releases.slint.dev/1.18.0/docs/slint/guide/development/focus/ , https://releases.slint.dev/1.18.0/docs/slint/reference/keyboard-input/focusscope/
- [ ] Elements receive key events only when focused; has-focus (out) reflects state. focus() gives focus; clear-focus() removes it, after which keyboard input is discarded until focus is set again (call, click on focusable, or Tab reaching first focusable).
- [ ] forward-focus: element-id property on any component/root (e.g. `forward-focus: input;`) — focus() on the component is redirected to the referenced element.
- [ ] forward-focus on Window or PopupWindow = initial focus element, focused the first time the window receives focus.
- [ ] Tab traversal: focusable items are FocusScope (focus-on-tab-navigation: true) and TextInput; layout-order does not change tab order (declaration order).
- [ ] OPEN: docs don't define tab traversal order across nested components/popups, Backtab (Shift+Tab) wrap-around, whether Tab key events are first delivered to key-pressed handlers, or what FocusReason values exist.
- [ ] OPEN: focus on window activation/deactivation, and has-focus behavior for TextInput when window loses focus.

##### Windows and popups
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/window/window/ , https://releases.slint.dev/1.18.0/docs/slint/reference/window/popupwindow/ , https://releases.slint.dev/1.18.0/docs/slint/reference/window/dialog/ , https://releases.slint.dev/1.18.0/docs/slint/reference/window/contextmenuarea/ , https://releases.slint.dev/1.18.0/docs/slint/reference/window/tooltip/ , https://releases.slint.dev/1.18.0/docs/slint/reference/window/systemtrayicon/ , https://releases.slint.dev/1.18.0/docs/slint/reference/window/windowmovearea/ , https://releases.slint.dev/1.18.0/docs/slint/reference/platform/
- [ ] Window props: width, height: length (in-out, 0px); always-on-top: bool (false); full-screen: bool (in-out; true if env SLINT_FULLSCREEN set, else false); minimized: bool (in-out, false); maximized: bool (in-out, false); background: brush (style dependent; painted first, covers window); default-font-family: string (""); default-font-size: length (0); default-font-weight: int (0); icon: image (empty); no-frame: bool (false); resize-border-width: length (0, for frameless); title: string (default = program name); safe-area-insets: Edges (out); virtual-keyboard-position: Point (out); virtual-keyboard-size: Size (out).
- [ ] Window functions: close() -> bool (triggers close-requested; true if accepted; false for child Window elements), hide() (drops strong ref; last ref quits event loop).
- [ ] Window geometry: fixed width/height when set; min/max constraints honored by window manager; preferred-width/height give initial size. MenuBar excluded from client area, children's x,y relative to client area.
- [ ] close-requested callback is referenced (close()) but its signature/return enum (CloseRequestResponse) lives elsewhere. OPEN: confirm.
- [ ] MenuBar: no properties except visible: bool (true; invisible bar takes no space, shortcuts still work); children are top-level Menu elements; native (macOS top bar) or Slint-rendered depending on platform.
- [ ] Menu (in MenuBar, ContextMenuArea, SystemTrayIcon, or nested Menu): title: string (""), enabled: bool (true; disabled can be selected but not activated), icon: image (empty); children MenuItem, Menu (submenu), MenuSeparator.
- [ ] MenuItem: title: string (""), enabled: bool (true), checkable: bool (false), checked: bool (in-out, false), shortcut: keys (@keys(); only within a MenuBar), icon: image (empty); callback activated(). checkable toggles checked on activation.
- [ ] MenuSeparator: no children/properties/callbacks; leading/trailing separators invisible, consecutive ones merge.
- [ ] PopupWindow: close-policy: PopupClosePolicy (default close-on-click | close-on-click-outside | no-auto-close; first two also close on Escape); is-open: bool (out); functions show(), close(). Positioned with x,y (relative to parent element) and width/height; not a child clipped by parent.
- [ ] Dialog: inherits Window behavior with auto-laid-out button row; one main non-button child; buttons are StandardButton{kind} or any element with constant dialog-button-role; order is platform-dependent; at most one StandardButton per kind; auto callback `<kind>_clicked` (e.g. cancel_clicked) aliasing the button's clicked unless it has its own handler. Same properties/functions as Window.
- [ ] ContextMenuArea: non-visual; opens on right-click, Menu key with a FocusScope inside focused, or long-press on Android; at most one Menu child, other children are regular visual children; show(Point) (relative to area), close(); enabled: bool (true).
- [ ] Tooltip: place inside any element (max one per element); text: styled-text (""), or one custom child element with layout; shown after a short delay near pointer, hidden when pointer leaves.
- [ ] SystemTrayIcon: top-level component (inherits SystemTrayIcon), no window; icon: image (empty; icon created only once non-empty), tooltip: string (""), visible: bool (true), title: string (""); callback clicked() (platform dependent: not fired on macOS when a populated menu attached); child Menu (reactive; shortcuts ignored). Binding API: no window access/event-loop; show/hide set visible; visible tray keeps event loop alive.
- [ ] WindowMoveArea: enabled: bool (true); drag with left button past small threshold moves window (windowing system does it; winit and Qt only; no-op elsewhere); plain click passes through; not in layout -> 100% of parent.
- [ ] Platform namespace: os: OperatingSystemType (out; android, ios, macos, linux, windows, other); uses-mock-data: bool (out, compile-time constant); style-name: string (out, without light/dark suffix); decimal-separator: string (out, default "."); functions open-url(url: string) -> bool, macos-bring-all-windows-to-front() (no-op elsewhere).
- [ ] OPEN: popup placement/flip rules when it would overflow the window, z-order vs main window, popup focus (forward-focus), click-through to elements below on close-on-click, and whether events inside popup reach TouchArea first.
- [ ] OPEN: Window with no explicit size and no layout (size 0? from preferred?), default background per style, resize min/max interplay with preferred.

##### Translations and fonts
Source: https://releases.slint.dev/1.18.0/docs/slint/guide/development/translations/ , https://releases.slint.dev/1.18.0/docs/slint/guide/development/fonts/
- [ ] @tr(...) macro: first argument must be a plain string literal; returns string.
- [ ] Formatting: `{}` placeholders filled in order; `{0}`, `{1}` zero-based positional; mixing positional and non-positional is an error; translations may use positional even if source doesn't; `{{` and `}}` escape literal braces; unterminated placeholder, unescaped `{`/`}`, or more placeholders than args is an error.
- [ ] Plurals: `@tr("singular {n}" | "plural {n}" % count, other_args...)` — both sides plain literals, count converted to int, `{n}` only valid in plural form, other placeholders index args after the count.
- [ ] Context: `@tr("ctx" => "text {}", args)` — plain literal context before `=>`; default context is enclosing component name; opt out via --no-default-translation-context (extractor) and compiler config (DefaultTranslationContext::None / SLINT_NO_DEFAULT_TRANSLATION_CONTEXT).
- [ ] Tooling flow: slint-tr-extractor -> .pot; translators -> .po; msgfmt -> .mo for gettext runtime; or bundled translations from path/<lang>/LC_MESSAGES/<domain>.po compiled in.
- [ ] Gettext lookup: <base>/<locale>/LC_MESSAGES/<domain>.mo; domain = Cargo package name (Rust) or CMake target (C++); locale from OS env vars.
- [ ] Bundled: domain = crate/target name; language auto-selected from locale (std feature); select_bundled_translation / slint.init_translations() to switch at runtime.
- [ ] Decimal separator: float<->string conversions use locale separator; Platform.decimal-separator exposes it (default "."); bundled translations carry their language's separator decided at compile time; gettext mode takes it from system locale.
- [ ] OPEN: plural rules per language (Plural-Forms header in .po), msgctxt encoding of context, behavior when no catalog/translation found (fallback to source with formatting), reactivity of @tr on language change.
- [ ] Fonts: default-font-family/-size/-weight on Window apply to any Text/TextInput/StyledText whose font-* property unset; system fonts picked automatically.
- [ ] Custom fonts: `import "./file.ttf";` (ttf, ttc, otf) in .slint embeds the font and makes its family name globally available to font-family; Window default-font-family selects it.
- [ ] Embedded/bare-metal software renderer: no system fonts, must import all fonts; embed-for-software-renderer resource mode pre-renders glyphs at sizes used in .slint; SLINT_FONT_SIZES env (comma list) adds run-time sizes; SDF fonts optional (sdf-fonts feature); software-renderer-systemfonts feature enables system fonts.
- [ ] font-weight: int 100..900 (400 normal) with FontWeight namespace constants (thin..black; names such as extra-bold used in docs).
- [ ] OPEN: font fallback chain and matching rules (family not found, weight nearest-match, synthetic italic/bold), family-name collision between imported and system fonts, FontWeight constant values.
- [ ] OPEN: relative font sizes ("rem" unit basis = Window default-font-size) and its default value when 0.

#### 8.4.4 std-widgets and styles

##### Import and styles
- [ ] Import path — widgets, Palette, StyleMetrics, layout boxes are imported by name from the virtual file "std-widgets.slint"; unimported builtin elements (Window, Rectangle, Text, TouchArea) need no import
- [ ] Common properties — every std widget supports all properties common to builtin elements (x, y, width, height, min/max/preferred sizes, stretch, visible, opacity, accessible-*, etc.)
- [ ] Style list — fluent, material, cupertino, cosmic (each with -light and -dark variant), qt (no variants), native (alias)
- [ ] Style descriptions — fluent = Fluent Design System; material = Material Design; cupertino = emulates macOS; cosmic = emulates COSMIC desktop; qt = rendered by Qt, requires Qt installed on system
- [ ] native alias — cupertino on macOS, fluent on Windows, material on Android, qt on Linux if Qt available else fluent
- [ ] Default style — fluent on all platforms when nothing selected
- [ ] Color scheme — plain style name follows system dark/light; -light/-dark suffix forces scheme regardless of system
- [ ] Selection time — style fixed at project compile time (not runtime); Palette.color-scheme can force scheme at runtime for non-Qt styles
- [ ] Selection: env var — SLINT_STYLE=<name> set before compilation (Rust, Python at script start before slint import usage, generic LSP, slint-viewer)
- [ ] Selection: Rust APIs — slint_build::compile_with_config() for build scripts; slint_interpreter ComponentCompiler::set_style() for interpreter
- [ ] Selection: C++ — CMake cache variable SLINT_STYLE (cmake -DSLINT_STYLE=material)
- [ ] Selection: NodeJS — `style` option of LoadFileOptions passed to loadFile
- [ ] Selection: Python — os.environ["SLINT_STYLE"] (or setdefault) before loading components
- [ ] Selection: tooling — slint-viewer --style <name>; LSP --style <name>; VS Code setting Extensions > Slint > Preview:Style
- [ ] Style detection in user code — `Platform.style-name` (string) compared against style names (e.g. "fluent") when Palette/StyleMetrics lack the needed value
- [ ] Style-aware custom components — read Palette.* and StyleMetrics.* from user components; values reflect current style
- [ ] OPEN: exact string values of Platform.style-name for -light/-dark and native-alias cases not stated
- [ ] OPEN: behaviour when SLINT_STYLE names an unknown/unavailable style (e.g. qt without Qt) is unspecified
- [ ] Authoring note — only qt needs native toolkit support; fluent, material, cupertino, cosmic are pure-.slint compositions of builtin elements (Rectangle, Text, TouchArea, FocusScope, Flickable, Image, Path, TextInput, Popup/PopupWindow, layouts) and can be authored in .slint
- [ ] Clean-room rule — docs give no pixel metrics, colors, radii, fonts or animation timings per style; visuals must be designed independently; documented contract is limited to the states/props listed below
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/overview/ and https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/style/

##### Palette
- [ ] Global singleton `Palette` — all brush properties are `out` (read-only to user code); values follow current style and color scheme
- [ ] background: brush (out) — default background; use when no specialised background fits
- [ ] foreground: brush (out) — content color on background
- [ ] alternate-background: brush (out) — e.g. text-input controls, panels like a sidebar
- [ ] alternate-foreground: brush (out) — content color on alternate-background
- [ ] control-background: brush (out) — default background of controls (push buttons, combo boxes)
- [ ] control-foreground: brush (out) — content color on control-background
- [ ] accent-background: brush (out) — highlighted controls such as primary buttons
- [ ] accent-foreground: brush (out) — content color on accent-background
- [ ] selection-background: brush (out) — selection highlight (e.g. text selection)
- [ ] selection-foreground: brush (out) — content color on selection-background
- [ ] border: brush (out) — separators and widget borders
- [ ] color-scheme: ColorScheme (in-out, default first enum value = unknown) — read to learn scheme in use; write to force dark/light; not supported by Qt style
- [ ] enum ColorScheme — unknown (follow system setting; may be custom scheme), dark, light
- [ ] OPEN: docs text for dark/light describes the colours in a way that appears inverted (dark described as light background); implement dark = dark backgrounds/light foreground conventionally and verify against Slint behaviour
- [ ] OPEN: does reading color-scheme return resolved dark/light or `unknown` when following system is not stated
- [ ] OPEN: concrete colour values per style/scheme are not documented
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/globals/palette/

##### StyleMetrics
- [ ] Global singleton `StyleMetrics` — properties are `out`
- [ ] layout-spacing: length (out) — default layout spacing; used by VerticalBox, HorizontalBox, GridBox
- [ ] layout-padding: length (out) — default layout padding; used by the same three boxes
- [ ] OPEN: values per style undocumented; no other metrics (e.g. text-cursor-width, window-border) in these docs
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/globals/stylemetrics/

##### Layout boxes (VerticalBox, HorizontalBox, GridBox)
- [ ] VerticalBox — equals VerticalLayout with padding and spacing defaulted from StyleMetrics.layout-padding / layout-spacing
- [ ] HorizontalBox — equals HorizontalLayout with the same defaults
- [ ] GridBox — equals GridLayout (Row children, col/row/colspan/rowspan on cells) with the same defaults
- [ ] Overridable — user may still set padding/spacing/alignment on the boxes; plain *Layout elements give full control
- [ ] Authoring note — trivial pure-.slint wrappers; no native support needed
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/layouts/horizontalbox/ (also verticalbox, gridbox)

##### GroupBox
- [ ] Properties — title: string (in, default ""), content-padding: length (in, default style-dependent; one value for all sides), enabled: bool (default true)
- [ ] Behaviour — container showing children under a common title; disabled state renders non-interactive (children inherit disabled)
- [ ] Children placed in a layout inside the box (example uses VerticalLayout child)
- [ ] OPEN: title position/frame styling, behaviour with empty title, how many children are laid out are unspecified
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/layouts/groupbox/

##### Button
- [ ] Properties — text: string (default ""), icon: image (default empty), icon-size: length (default 0px = style default; button grows to fit large icons), colorize-icon: bool (default false; icon takes text colour), primary: bool (default false; accent colour), checkable: bool (default false), checked: bool (in-out, default false; only effective when checkable), enabled: bool (default true; false = cannot be pressed), has-focus: bool (out), pressed: bool (out)
- [ ] Callback — clicked(): fires when pointer/finger pressed then released on the element
- [ ] States to author — normal, hover, pressed, focused (focus ring/indicator), disabled, checked (when checkable), primary variant
- [ ] Docs note — not all styles draw icons
- [ ] Keyboard — OPEN: activation keys (Space/Enter) and focus-by-Tab not stated in docs but expected; cannot be confirmed from docs
- [ ] OPEN: whether clicked fires when released outside the button; docs say "released on this element"
- [ ] OPEN: toggling of checked on click when checkable; accessibility role/action (1.18.1 adds accessible-enabled on Button) unspecified
- [ ] Authoring note — pure .slint (Rectangle + Text + Image + TouchArea + FocusScope)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/button/

##### StandardButton
- [ ] Properties — kind: StandardButtonKind (default ok), primary: bool (default false), enabled: bool (default true), has-focus: bool (out), pressed: bool (out)
- [ ] Callback — clicked()
- [ ] enum StandardButtonKind — ok, cancel, apply, close, reset, help, yes, no, abort, retry, ignore
- [ ] Semantics — ok accepts dialog and closes; cancel rejects and closes; apply accepts without closing; close closes without reading values; reset restores initial state; help opens context documentation; yes/no confirm/deny; abort aborts; retry retries failed action; ignore ignores failed action
- [ ] Label and icon derive from kind and style (localised text); placement within Dialog depends on platform
- [ ] OPEN: exact label strings/icons per kind and ordering rules in Dialog (the Dialog element is outside this scope)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/standardbutton/

##### CheckBox
- [ ] Properties — text: string (default ""), checked: bool (in-out, default false), enabled: bool (default true), has-focus: bool (out), font-size: length (default 0px = style default), font-weight: int (default 0 = style default)
- [ ] Callback — toggled(): fires when the checked value changed by user
- [ ] States to author — unchecked, checked, hover, pressed, focused, disabled
- [ ] Use guidance — multi-select lists; prefer Switch for on/off actions
- [ ] OPEN: no indeterminate/tri-state; keyboard (Space toggles) unstated; whether toggled fires on programmatic change unstated
- [ ] Authoring note — pure .slint
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/checkbox/

##### Switch
- [ ] Properties — text: string (default ""), checked: bool (in-out, default false), enabled: bool (default true), has-focus: bool (out)
- [ ] Callback — toggled()
- [ ] States to author — off, on, hover, pressed, focused, disabled; thumb/knob animation between positions is a style choice
- [ ] Use guidance — physical-switch metaphor; prefer CheckBox for list selections
- [ ] Reference recipe — guide shows custom toggle: ~40px wide pill indicator, knob inset, 100ms background/200ms knob animation, label to the right (illustrative only, not the std look)
- [ ] OPEN: font-size/font-weight not listed for Switch
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/switch/

##### RadioGroup and RadioButton
- [ ] RadioGroup properties — current-value: string (out; text of selected RadioButton), enabled: bool (default true; disables group and buttons), has-focus: bool (out; true while any radio has focus), title: string (default ""; label above group, hidden when empty), orientation: Orientation (default vertical; horizontal = single row)
- [ ] RadioGroup callback — selected(value: string): user selects a different option; argument = new entry text
- [ ] Children — only RadioButton, either statically listed or produced by a single `for`; `if` inside is unsupported; a `for` must be the sole child (no mixing with static siblings)
- [ ] RadioButton (pseudo-element, only inside RadioGroup) properties — text: string (default ""), enabled: bool (default true; false = not selectable but visible), checked: bool (in-out, default false)
- [ ] RadioButton callback — toggled(): checked state changes, both on becoming selected and when another button takes over
- [ ] Exclusive selection — at most one checked; clicking sets it true and previous one false
- [ ] Programmatic selection — setting checked = true selects on the next event-loop iteration and unchecks previous; setting false on the selected button is reverted; selecting a disabled button is refused
- [ ] Binding semantics — two-way `checked <=> model.field` recommended (also with `for` over a model); one-way binding works for initial state but is severed when the group later writes false
- [ ] Host-side selection — change selection by setting the RadioButton's checked (no settable current-value)
- [ ] enum Orientation — horizontal, vertical
- [ ] OPEN: arrow-key navigation within group, Tab stop behaviour, behaviour when no button is checked initially are unspecified
- [ ] Authoring note — needs compiler/runtime support for the restricted-children rule and group-controlled checked writes; may be approximated in .slint with a group component that inspects children, but "only RadioButton children" enforcement is a language-level feature
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/radiogroup/

##### Slider
- [ ] Properties — value: float (in-out, default 0; "defaults to the minimum"), minimum: float (default 0), maximum: float (default 100), step: float (default 1; change per arrow key press), orientation: Orientation (default horizontal), enabled: bool (default true), has-focus: bool (out)
- [ ] Callbacks — changed(float): value changed; released(float): user finished changing (knob released or arrow keys lifted)
- [ ] Keyboard — arrow keys change value by step
- [ ] States to author — normal, hover, pressed/dragging knob, focused, disabled; vertical variant
- [ ] Reference recipe — custom slider uses 24px min height, 100px min width, handle diameter = height; drag maps pointer delta to value range and clamps to [minimum, maximum]
- [ ] OPEN: value clamping on programmatic set, direction of vertical slider (up vs down increases), Page/Home/End keys, whether `changed` fires on programmatic changes
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/slider/

##### SpinBox
- [ ] Properties — value: int (in-out, default 0; "defaults to the minimum"), minimum: int (default 0), maximum: int (default 100), step-size: int (default 1; applied on increment/decrement), read-only: bool (default false), horizontal-alignment: TextHorizontalAlignment (default left), enabled: bool (default true), has-focus: bool (out)
- [ ] Callback — edited(int): value changed by user
- [ ] enum TextHorizontalAlignment — start, end, left, center, right
- [ ] Behaviour — numeric text field with increment/decrement controls; value stays within [minimum, maximum]
- [ ] OPEN: keyboard (Up/Down arrows, typing digits), mouse wheel, clamping rules, whether edited fires per keystroke or on commit are unspecified
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/spinbox/

##### ComboBox
- [ ] Properties — model: [string] (default []), current-index: int (in-out, default 0), current-value: string (in-out, default ""; selected text), enabled: bool (default true), has-focus: bool (out)
- [ ] Callback — selected(string): user picked a value; argument = selected value
- [ ] Behaviour — button-like control opening a popup list of model values on click; popup selection updates current-index/current-value
- [ ] OPEN: keyboard open/navigation, popup placement/size, relation between setting current-value and current-index, out-of-range index handling unspecified
- [ ] Authoring note — .slint with PopupWindow/Popup + list; popup window support needed from runtime
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/combobox/

##### ProgressIndicator
- [ ] Properties — progress: float (default 0; 0..1, out-of-range values capped), indeterminate: bool (default false; true when completion unknown)
- [ ] Example sets `progress: 80%`; percent literal equals 0.8
- [ ] Behaviour — horizontal bar; indeterminate shows non-value-based animation
- [ ] OPEN: indeterminate animation timing/look unspecified; no callbacks; no accessibility notes
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/progressindicator/

##### Spinner
- [ ] Properties — same as ProgressIndicator: progress: float (default 0, capped to 0..1), indeterminate: bool (default false); differs only in shape (circular)
- [ ] OPEN: determinate arc rendering and indeterminate rotation timing unspecified
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/basic-widgets/spinner/

##### LineEdit
- [ ] Properties — text: string (in-out, default ""), placeholder-text: string (default ""), read-only: bool (default false; no keyboard/mouse edit but selection and programmatic edits still work), enabled: bool (default true; false = nothing can be entered), has-focus: bool (out), font-size: length (default 0px), font-family: string (default ""), font-italic: bool (default false), horizontal-alignment: TextHorizontalAlignment (default left), input-type: InputType (default text), input-method-hints: InputMethodHints (default all defaults)
- [ ] enum InputType — text (normal), password (masked, default mask char `*`), number (accepts only 0-9), decimal (valid decimal chars using current-locale separator), search (rendered normally; tells assistive tech it is a search/filter field)
- [ ] struct InputMethodHints — capitalization: CapitalizationMode (default sentences), auto-correct: bool (default true), auto-complete: bool (default true); advisory, input method may ignore
- [ ] Functions — focus(), clear-focus(), select-all(), clear-selection(), copy(), cut(), paste(), undo(), redo(), set-selection-offsets(anchor: int, focus: int) (UTF-8 byte offsets; anchor stays, focus is moving end and may precede anchor; equal values = caret placement, no selection)
- [ ] Function guarantee — clear-selection, cut, paste, undo, redo work regardless of read-only and enabled
- [ ] Callbacks — accepted(string): Enter pressed; edited(string): text changed by user; key-pressed(KeyEvent) -> EventResult and key-released(KeyEvent) -> EventResult: run before widget handling; return accept to consume, reject to let LineEdit handle
- [ ] Behaviour — single line; selection, clipboard, undo/redo; cursor and selection use Palette selection colours (implied by selection-background)
- [ ] OPEN: CapitalizationMode enum members not listed in these pages; mask character override not documented; keyboard shortcuts (Ctrl/Cmd+A/C/X/V/Z) assumed from TextInput, not in these docs; horizontal scrolling when text overflows unspecified
- [ ] OPEN: whether number/decimal types also filter pasted text; whether edited fires for programmatic text changes (docs say only user)
- [ ] Authoring note — built on builtin TextInput element (needs runtime text engine); frame/placeholder/focus ring authored in .slint
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/views/lineedit/

##### TextEdit
- [ ] Properties — text: string (in-out, default ""), placeholder-text: string (in, default ""), read-only: bool (default false), enabled: bool (default true), has-focus: bool (out), font-size: length (default 0px), font-family: string (default ""), font-italic: bool (default false), wrap: TextWrap (default word-wrap), horizontal-alignment: TextHorizontalAlignment (default first member), input-method-hints: InputMethodHints
- [ ] enum TextWrap — no-wrap (overflow), word-wrap (break at word boundaries, else anywhere for long words), char-wrap (any char; only Qt and Software renderers)
- [ ] Scroll properties — content-width: length (in-out, 0px), content-height: length (in-out, 0px), content-x: length (in-out, 0px; usually negative), content-y: length (in-out, 0px; usually negative), visible-width: length (out; excludes scrollbar), visible-height: length (out); deprecated aliases viewport-width/height/x/y
- [ ] Functions — focus(), clear-focus(), select-all(), clear-selection(), copy(), cut(), paste(), set-selection-offsets(anchor: int, focus: int); no undo()/redo() listed (unlike LineEdit)
- [ ] Callbacks — edited(string), key-pressed(KeyEvent) -> EventResult, key-released(KeyEvent) -> EventResult; no accepted
- [ ] Behaviour — multi-line input with built-in scrolling (scrollbars) and Enter inserting newline
- [ ] 1.18.1 changelog — adds accessible-action-set-value on TextEdit
- [ ] OPEN: no input-type, scrollbar policy or tab-key handling documented; horizontal-alignment default listed as "first enum value" (start) vs LineEdit "left"
- [ ] OPEN: undo/redo availability on TextEdit unspecified
- [ ] Authoring note — composed of ScrollView/Flickable plus TextInput; pure .slint on top of builtin TextInput
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/views/textedit/

##### ScrollView
- [ ] Properties — content-width: length (in-out, 0px), content-height: length (in-out, 0px), content-x: length (in-out, 0px; usually negative), content-y: length (in-out, 0px), visible-width: length (out), visible-height: length (out), vertical-scrollbar-policy: ScrollBarPolicy (default as-needed), horizontal-scrollbar-policy: ScrollBarPolicy (default as-needed), mouse-drag-pan-enabled: bool (in, default false), enabled: bool (default true; visual only), has-focus: bool (in-out, default false; visual only)
- [ ] enum ScrollBarPolicy — as-needed (visible only when needed), always-off, always-on
- [ ] Callback — scrolled(): content-x/content-y changed by user action (drag, scroll)
- [ ] Behaviour — scrolls when content-width/height exceed visible area; a layout child makes default content size equal to the layout's minimum size; Flickable plus interactive scrollbars
- [ ] Input — touch panning and mouse wheel always work; mouse-drag panning only with mouse-drag-pan-enabled
- [ ] Deprecated aliases — viewport-width, viewport-height, viewport-x, viewport-y
- [ ] OPEN: scrollbar thickness, auto-hide/overlay behaviour, keyboard scrolling, wheel step size and kinetic scrolling are unspecified
- [ ] Authoring note — pure .slint over builtin Flickable; scrollbar visuals authored per style
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/views/scrollview/

##### ListView
- [ ] Behaviour — ScrollView whose `for` content is auto-laid out as a vertical list; items instantiated lazily only when visible (virtualised, stable performance for huge models)
- [ ] Properties and callbacks — identical to ScrollView
- [ ] Contrast — ScrollView for free-form content; ListView requires a `for` element
- [ ] OPEN: item height uniformity requirement, horizontal scrolling and delegate width rules (example sets width: parent.width) not specified
- [ ] Authoring note — needs runtime virtualisation support in builtin Flickable/for-repeater (ListView layouting of visible rows is a compiler/runtime feature)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/views/listview/

##### StandardListView
- [ ] Properties — all ListView/ScrollView ones plus model: [StandardListViewItem] (default []), current-item: int (in-out, default -1 = none)
- [ ] struct StandardListViewItem — text: string (shared with StandardTableView rows); OPEN: other fields (icon etc.) not listed here
- [ ] Function — set-current-item(int): select index and scroll into view
- [ ] Callbacks — current-item-changed(int): user changed selection; item-pointer-event(int, PointerEvent, Point): any mouse event like TouchArea; args = item index, event, position within the list view
- [ ] Behaviour — ListView with default row delegate showing item text, highlighted selected row (Palette selection/accent colours)
- [ ] OPEN: keyboard navigation (Up/Down/PageUp/PageDown), index -1 on click outside, behaviour when model shrinks unspecified
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/views/standardlistview/

##### StandardTableView
- [ ] Properties — scroll properties as ScrollView, plus columns: [TableColumn] (in-out, default []), rows: [[StandardListViewItem]] (in-out, default []), current-row: int (in-out, default -1), current-sort-column: int (out; -1 when unsorted)
- [ ] struct TableColumn — title: string, min-width: length (logical), horizontal-stretch: float, sort-order: SortOrder, width: length (actual logical width)
- [ ] struct StandardListViewItem — text: string
- [ ] Callbacks — sort-ascending(int) and sort-descending(int): model should be sorted by the given column (component does not sort itself); row-pointer-event(int, PointerEvent, Point): row index, event, position within table; current-row-changed(int): user changed current row
- [ ] Function — set-current-row(int): select row and scroll into view
- [ ] Behaviour — header row from columns; clickable headers request sort; resizable columns implied by width property; rows model is rows of cells
- [ ] enum SortOrder — referenced by TableColumn.sort-order; OPEN: members (unsorted/ascending/descending) not listed in these pages
- [ ] OPEN: column resize UX, header click cycling, keyboard navigation, selection of cells vs rows, stretch semantics detail unspecified
- [ ] Reference recipe — guide shows a simple ListView-based table with draggable 10px header dividers, elided text, column widths array (non-std look)
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/views/standardtableview/

##### TabWidget
- [ ] Properties — current-index: int (default 0; visible tab), orientation: Orientation (default horizontal; vertical puts the tab bar left of content; must be a constant, not a binding to a property)
- [ ] Tab (child element) — title: string (default ""); Tab content children shown only for the active tab
- [ ] Constraint — only Tab children allowed; exactly one tab visible at a time
- [ ] Behaviour — tab bar with one entry per Tab; selecting an entry changes current-index
- [ ] Guide recipe — hand-built tabs: button row plus clipped content area with 125ms ease slide animation (illustrative)
- [ ] OPEN: callbacks (none listed), enabled per Tab, keyboard navigation, behaviour when current-index out of range unspecified
- [ ] Authoring note — needs compiler support for Tab-only children (pseudo-elements); the visual widget can be .slint
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/views/tabwidget/

##### DatePickerPopup
- [ ] Properties — title: string (default ""; shown at top), date: Date (default struct defaults; initial displayed date)
- [ ] struct Date — day: int (1..31), month: int (1..12), year: int
- [ ] Callbacks — accepted(Date): OK clicked; canceled(): Cancel clicked
- [ ] Popup usage — a popup: shown/closed with show()/close() functions, positioned with x/y, close-policy: PopupClosePolicy (example uses no-auto-close); the host must close it in callbacks
- [ ] Initial date may be partial (example sets year and month only)
- [ ] OPEN: date range limits, first weekday/locale handling, month navigation, keyboard use, default value of date when unset unspecified
- [ ] Authoring note — .slint calendar grid inside PopupWindow; Date struct and locale (month/day names via @tr) need runtime support
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/misc/datepicker/

##### TimePickerPopup
- [ ] Properties — time: Time (default struct defaults; initial time), title: string (default ""), use-24-hour-format: bool (default system setting; true if undeterminable; false = AM/PM mode)
- [ ] struct Time — hour: int (0..23), minute: int (0..59), second: int (0..59)
- [ ] Callbacks — accepted(Time): OK clicked; canceled(): Cancel clicked
- [ ] Popup usage — show()/close() with x/y/width/height set by user (example 360x524px)
- [ ] OPEN: whether the second field is user-editable, input mode (dial vs keypad), AM/PM conversion rules, system 24-hour detection method unspecified
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/misc/timepicker/

##### AboutSlint
- [ ] Element — displays a "Made with Slint" badge; no properties or callbacks documented
- [ ] OPEN: artwork/logo asset needed; a clean-room implementation should provide its own badge rather than reproduce Slint branding
Source: https://releases.slint.dev/1.18.0/docs/slint/reference/std-widgets/misc/aboutslint/

##### Custom controls guide (language-level contract used by widgets)
- [ ] Widget authoring pattern — component inherits Rectangle; exposes `in-out property` text/checked/value, `callback clicked <=> touch.clicked`, `enabled <=> touch-area.enabled`; TouchArea overlays whole control
- [ ] TouchArea members used — pressed, has-hover, mouse-x, mouse-y, pressed-x, pressed-y, enabled, clicked, moved, pointer-event(event) with event.button == PointerEventButton.left and event.kind == PointerEventKind.down, mouse-cursor (e.g. ew-resize)
- [ ] Two-way binding `<=>` for properties and callbacks; hyphen and underscore are equivalent in identifiers (host bindings use underscore)
- [ ] Animation — `animate <prop> { duration; easing: ease|ease-in|ease-in-out; delay }`; states `states [ name when cond: { prop: value; in {animate ...} out {animate ...} } ]`
- [ ] Layout hooks — min-width/min-height, preferred-width/height, horizontal-stretch/vertical-stretch, alignment, @children, row/col/colspan, Text overflow: elide, Rectangle clip: true, Color.darker(%)
- [ ] Globals — `export global` with `pure callback` implemented in host code; string interpolation `\{expr}`; @tr() for translations
- [ ] Popups — Palette used in custom sidebar recipe; breakpoint recipe uses states on width
- [ ] Visual contract hints — pressed state shifts label slightly (1-2px), hover/pressed colour changes, rounded pill radius = height/2, disabled uses greyed colours; these are illustrative only
Source: https://releases.slint.dev/1.18.0/docs/slint/guide/development/custom-controls/
