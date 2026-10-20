# Slint Bindings: instructions for agents

**What this is.** Embedding Slint into native hosts (SwiftUI/AppKit, WinUI 3) through a
host-driven custom Slint platform and a C ABI. Weft rendering builds on it later.

If an `AGENTS.md` or `CLAUDE.md` exists higher in the tree, follow it too; on conflict, ask the creator.

## Read first

- `plan.md`: active tasks and their execution plans.
- `research.md`: what Slint offers for embedding and why this design.

## Rules

- **The host owns the run loop.** The core never spins an event loop or opens a window;
  hosts drive resize, input, `sb_tick` and `sb_host_render`.
- **One UI thread.** Every `sb_*` call comes from the thread that created the host.
  Swift types are `@MainActor`; C# calls stay on the dispatcher thread.
- **The FFI crate only forwards.** Logic goes into `slint-bindings-core`. Every exported
  function catches panics and reports errors through `sb_last_error`.
- **The header is generated.** After changing the C ABI run `just header` and update
  `windows/SlintBindings.WinUI/NativeMethods.cs` in the same commit; `just header-check`
  fails on drift.
- **Every `unsafe` block has a `// SAFETY:` comment**; every `unsafe fn` has a `# Safety` section.
- **Slint is pinned exactly** (`=1.18.x` for `slint`, `slint-interpreter`, `i-slint-core`):
  `i-slint-core` is internal and must match. Bump all three together (and flutter/'s
  workspace, which the vendored interpreter crate inherits from), with creator permission.
- **Shared embedding lives in `slint-embed`.** `flutter/` is a copy of
  [slint_dart](https://github.com/listepo/slint_dart). The software platform,
  the Dart FFI event map and the UI-thread guard are `flutter/packages/slint/rust/embed`.
  `slint-dart-core` re-exports them. Do not grow a second platform.
- **Components are interpreted.** `.slint` is compiled at run time through
  `flutter/packages/slint_build/interpreter` (`slint-dart-interpreter`: compile and the
  JSON value bridge); `slint-bindings-core::runtime` only adapts it to the hosts.

- **No Slint GPU renderer in the build.** The GPU path draws with `crates/slintx-wgpu`,
  written here from scratch. `i-slint-renderer-femtovg` and `i-slint-renderer-skia` must
  not enter the dependency graph on any shipped target, and none of their code is copied;
  `just renderer-check` fails if one does. The software renderer stays: it is the CPU
  path and the reference the slintx-wgpu screenshot tests compare against
  (`crates/slintx-wgpu/tests/compare.rs`). It draws no box shadows or rounded clips and
  always samples images nearest-neighbour, so those are checked by expected pixels.

## Commands

```sh
mise install
just check        # fmt-check, renderer-check, clippy -D warnings, nextest, header-check, swift build, swift test
just header       # regenerate swift/Sources/CSlintBindings/slint_bindings.h
just swift-run    # demo app
```
