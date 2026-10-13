# Slint Bindings

Embed [Slint](https://slint.dev) UIs inside native apps: an `NSView` / SwiftUI
view on macOS and a WinUI 3 control on Windows, driven by the host's own run
loop. Later the same core renders Weft UI trees (the sibling `weft` project)
natively, and the web build reuses it through WebAssembly.

Slint has no official Swift or WinUI integration (see [research.md](research.md)).
Its supported embedding hook is a custom platform (`slint::platform`), which is
what this project builds on.

**Status:** M3. macOS presents through a `CAMetalLayer` (FemtoVG on wgpu's
Metal backend) and WinUI through a `SwapChainPanel` (the same renderer on
D3D12). Either host falls back to the CPU buffer when the GPU adapter or the
surface cannot be created. Keyboard, IME and the per-RID DLL from M2 are
unchanged. The M3 frame-time checkpoint (4K under 4 ms) has not been measured
here, and the WinUI window itself has not been launched.

## Layout

| Path | What it is |
| --- | --- |
| `crates/slint-bindings-core` | Host-driven Slint platform: CPU renderer into a caller-owned RGBA buffer, and FemtoVG on wgpu into a host surface. Input forwarding, timers, the demo component. |
| `crates/slint-bindings-ffi` | `sb_*` C ABI over the core (static library for Swift, DLL for WinUI). Forwards only. |
| `flutter/` | Copy of [slint_dart](https://github.com/listepo/slint_dart). Its `slint-embed` crate (`flutter/packages/slint/rust/embed`) is the software platform, Dart event map and UI-thread guard shared with the core. |
| `swift/` | SwiftPM package: `CSlintBindings` (C module over the generated header), `SlintBindings` (`SlintHost`, `SlintNSView`, `SlintDemoView`), `SlintDemo` app. |
| `windows/` | WinUI 3 `SlintPanel` control and demo app. |

```text
SwiftUI / AppKit ─┐                       ┌─ WinUI 3 (C#)
  SlintDemoView   │   sb_* C ABI          │   SlintPanel
  SlintNSView     ├──▶ slint-bindings-ffi ◀┤   SlintHost (P/Invoke)
  SlintHost       │         │             │
                  │   slint-bindings-core │
                  │   CPU buffer, or      │
                  │   FemtoVG on wgpu     │
```

## Build and run (macOS, Apple Silicon)

```sh
mise install      # Rust 1.99, cargo-nextest, just; cbindgen via `cargo install cbindgen`
just check        # fmt, clippy, tests, header drift, P/Invoke drift, swift build, swift test
just swift-run    # opens the SwiftUI demo window
```

Windows: see [windows/README.md](windows/README.md).

## Docs

- [docs/README.md](docs/README.md): documentation index
- [docs/frame-lifecycle.md](docs/frame-lifecycle.md): how a frame is driven, rendered and presented
- [windows/README.md](windows/README.md): building and running on Windows
- [plan.md](plan.md): milestones and active tasks
- [research.md](research.md): how Slint can be embedded, with sources
- [toolchain.md](toolchain.md): programs and packages
- [Русский](docs/ru/README.md), [Українська](docs/uk/README.md)

## License

Apache-2.0. See [LICENSE](LICENSE).
