# Toolchain

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| rust (rustc, cargo, rustfmt, clippy) | mise (`mise.toml`, 1.99.0) | Builds the core and the C ABI; the `wasm32-unknown-unknown` target for `just wasm-check` | https://github.com/rust-lang/rust |
| wasm-bindgen CLI (wasm-bindgen-test-runner) | mise (`github:wasm-bindgen/wasm-bindgen`, same version as `wasm-bindgen` in `Cargo.lock`) | Runs the `slintx-*` tests on `wasm32-unknown-unknown` (`just wasm-check`) | https://github.com/wasm-bindgen/wasm-bindgen |
| Node.js | mise (`node`, Active LTS) | JavaScript host for wasm-bindgen-test-runner | https://github.com/nodejs/node |
| cargo-nextest | mise (`cargo:cargo-nextest`) | Test runner (`just test`) | https://github.com/nextest-rs/nextest |
| just | mise | Recipes (`justfile`) | https://github.com/casey/just |
| cbindgen | global (`cargo install cbindgen`) | Generates the C header from the FFI crate (`just header`) | https://github.com/mozilla/cbindgen |
| Xcode (swift, SwiftPM, ibtool) | Mac App Store | Builds the Swift package and the macOS demo; `ibtool` checks the storyboards `slintx-convert` writes (ignored tests) | https://developer.apple.com/xcode/ |
| jq | global (preinstalled on GitHub's macOS runners; `brew install jq`) | Reads `swift package dump-package` in `just swift-package-check` | https://github.com/jqlang/jq |
| .NET SDK 10 | mise (`dotnet`, global) | Builds the WinUI projects (Windows only) | https://github.com/dotnet/sdk |
| blueprint-compiler | global (`apt install blueprint-compiler`, `brew install blueprint-compiler`) | Checks the GTK Blueprint `slintx-convert` writes (ignored test) | https://gitlab.gnome.org/GNOME/blueprint-compiler |
| dunnage | ketch | Post-test `target/` cleanup in `just test`; a no-op when missing | https://github.com/listepo/dunnage |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| slint | local | https://github.com/slint-ui/slint | UI runtime; software renderer and custom platform API |
| slint-interpreter | local | https://github.com/slint-ui/slint | Compiles `.slint` at run time (`sb_compile`); the oracle in `slintx-conformance`'s tests (dev only) |
| pollster | local | https://github.com/zesterer/pollster | Blocks on wgpu adapter and device requests (`slint-bindings-core`, `slintx-wgpu`) and on Slint's async compiler API (`slintx-conformance` tests, dev only) |
| slint-dart-interpreter | local (path, `flutter/packages/slint_build/interpreter`) | https://github.com/listepo/slint_dart | Shared compile helper and JSON value bridge for properties and callbacks |
| i-slint-core | local | https://github.com/slint-ui/slint | Image and SVG decoding without a backend; the renderer traits and shared parley text layout behind slintx-wgpu's `slint` feature |
| thiserror | local | https://github.com/dtolnay/thiserror | Core and slintx-wgpu error enums |
| accesskit | local | https://github.com/AccessKit/accesskit | Platform-neutral accessibility tree built from the Slint item tree (T14) |
| accesskit_macos | local (macOS) | https://github.com/AccessKit/accesskit | VoiceOver adapter behind the host `NSView` (T14) |
| slintx-wgpu | local (path, `crates/slintx-wgpu`) | this repository | Our Slint renderer on wgpu; the only GPU renderer in the build |
| wgpu | local | https://github.com/gfx-rs/wgpu | GPU API slintx-wgpu draws with; Metal and D3D12 surfaces in the core |
| swash | local | https://github.com/dfrg/swash | Glyph rasterizing in slintx-wgpu |
| lyon_tessellation | local | https://github.com/nical/lyon | Path fill and stroke tessellation in slintx-wgpu |
| parley | local | https://github.com/linebender/parley | Text glyph runs and layouts slintx-wgpu draws, the same parley Slint locks |
| bytemuck | local | https://github.com/Lokathor/bytemuck | Plain-data vertex and storage buffers in slintx-wgpu |
| rgba-png | local (dev, git rev) | https://github.com/pyrlyn/crates | PNG snapshots and pixel comparison in the screenshot tests; shared with scull |
| png | local (via rgba-png) | https://github.com/image-rs/image-png | Encode and decode the frame snapshots |
| rowan | local | https://github.com/rust-analyzer/rowan | Lossless syntax tree of `slintx-syntax` |
| wasm-bindgen-test | local | https://github.com/wasm-bindgen/wasm-bindgen | Runs the `slintx-syntax` and `slintx-modules` tests on `wasm32-unknown-unknown` |
| quick-xml | local | https://github.com/tafia/quick-xml | Escaping in `slintx-convert`'s XML, HTML and storyboard output; well-formedness checks in its tests |
| clap | local | https://github.com/clap-rs/clap | Arguments of the `slint-convert` binary |
| anyhow | local | https://github.com/dtolnay/anyhow | The one error that reaches `slint-convert`'s `main` |
| insta | local (dev) | https://github.com/mitsuhiko/insta | Snapshots of every `slintx-convert` backend |
| trycmd | local (dev) | https://github.com/assert-rs/trycmd | `slint-convert` command-line cases |

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
