# Toolchain

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| rust (rustc, cargo, rustfmt, clippy) | mise (`mise.toml`, 1.99.0) | Builds the core and the C ABI; the `wasm32-unknown-unknown` target for `just wasm-check` | https://github.com/rust-lang/rust |
| wasm-bindgen CLI (wasm-bindgen-test-runner) | mise (`github:wasm-bindgen/wasm-bindgen`, same version as `wasm-bindgen` in `Cargo.lock`) | Runs the `slintx-*` tests on `wasm32-unknown-unknown` (`just wasm-check`) | https://github.com/wasm-bindgen/wasm-bindgen |
| Node.js | mise (`node`, Active LTS) | JavaScript host for wasm-bindgen-test-runner | https://github.com/nodejs/node |
| cargo-nextest | mise (`cargo:cargo-nextest`) | Test runner (`just test`) | https://github.com/nextest-rs/nextest |
| just | mise | Recipes (`justfile`) | https://github.com/casey/just |
| cbindgen | global (`cargo install cbindgen`) | Generates the C header from the FFI crate (`just header`) | https://github.com/mozilla/cbindgen |
| Xcode (swift, SwiftPM) | Mac App Store | Builds the Swift package and the macOS demo | https://developer.apple.com/xcode/ |
| jq | global (preinstalled on GitHub's macOS runners; `brew install jq`) | Reads `swift package dump-package` in `just swift-package-check` | https://github.com/jqlang/jq |
| .NET SDK 10 | mise (`dotnet`, global) | Builds the WinUI projects (Windows only) | https://github.com/dotnet/sdk |
| dunnage | ketch | Post-test `target/` cleanup in `just test`; a no-op when missing | https://github.com/listepo/dunnage |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| slint | local | https://github.com/slint-ui/slint | UI runtime; software renderer and custom platform API |
| slint-interpreter | local | https://github.com/slint-ui/slint | Compiles `.slint` at run time (`sb_compile`); the oracle in `slintx-conformance`'s tests (dev only) |
| pollster | local | https://github.com/zesterer/pollster | Blocks on wgpu adapter and device requests (`slint-bindings-core`, `slintx-wgpu`) and on Slint's async compiler API (`slintx-conformance` tests, dev only) |
| slint-dart-interpreter | local (path, `flutter/packages/slint_build/interpreter`) | https://github.com/listepo/slint_dart | Shared compile helper and JSON value bridge for properties and callbacks |
| i-slint-core | local | https://github.com/slint-ui/slint | Image and SVG decoding without a backend; the renderer traits and shared parley text layout slintx-wgpu implements |
| thiserror | local | https://github.com/dtolnay/thiserror | Core error enum |
| accesskit | local | https://github.com/AccessKit/accesskit | Platform-neutral accessibility tree built from the Slint item tree (T14) |
| accesskit_macos | local (macOS) | https://github.com/AccessKit/accesskit | VoiceOver adapter behind the host `NSView` (T14) |
| slintx-wgpu | local (path, `crates/slintx-wgpu`) | this repository | Our Slint renderer on wgpu; the only GPU renderer in the build |
| wgpu | local | https://github.com/gfx-rs/wgpu | GPU API slintx-wgpu draws with; Metal and D3D12 surfaces in the core |
| swash | local | https://github.com/dfrg/swash | Glyph rasterizing in slintx-wgpu |
| lyon_tessellation | local | https://github.com/nical/lyon | Path fill and stroke tessellation in slintx-wgpu |
| bytemuck | local | https://github.com/Lokathor/bytemuck | Plain-data vertex and storage buffers in slintx-wgpu |
| pixel-diff | local (dev, path, `crates/pixel-diff`) | this repository | PNG snapshots and pixel comparison in the screenshot tests |
| png | local (via pixel-diff) | https://github.com/image-rs/image-png | Encode and decode the frame snapshots |
| rowan | local | https://github.com/rust-analyzer/rowan | Lossless syntax tree of `slintx-syntax` |
| wasm-bindgen-test | local | https://github.com/wasm-bindgen/wasm-bindgen | Runs the `slintx-syntax` tests on `wasm32-unknown-unknown` |

## NuGet

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| Microsoft.WindowsAppSDK | local (`windows/*.csproj`) | https://github.com/microsoft/WindowsAppSDK | WinUI 3 |
| MSTest.Sdk | local (`windows/SlintBindings.Tests`, project SDK 4.5.1) | https://github.com/microsoft/testfx | C# host tests on Microsoft.Testing.Platform |

## SwiftPM

No external packages.

## Linkers

- **Windows (MSVC):** `rust-lld` through `.cargo/config.toml` (`[target.*-pc-windows-msvc] linker`). It ships with rustup, nothing to install.
- **Linux:** `mold`, pinned in `mise.toml` for Linux only and driven by `clang`. mise `[env]` sets `CARGO_TARGET_{X86_64,AARCH64}_UNKNOWN_LINUX_GNU_RUSTFLAGS` to `-C linker=clang -C link-arg=-fuse-ld=mold` only when mise has mold installed (`mise install`); otherwise the variable is empty and the default linker stays. A Linux machine without mold (or without mise) builds as before; to use mold, run `mise install` and make sure `clang` is on PATH (`apt install clang`). `RUSTFLAGS`, when set, overrides these flags. CI installs mold on Linux runners.
- **macOS:** Apple's native linker (mold has no macOS support).
- **Cranelift:** not used. It needs a nightly toolchain and this repository is on stable.
