# Recipes for slint-bindings. Run inside the mise environment (`mise install` first).

header_path := "swift/Sources/CSlintBindings/slint_bindings.h"

default: check

# Everything CI runs on macOS. Cleanup is last so it does not invalidate the Swift link.
check: fmt-check license-check license-check-test clippy nextest wasm-check header-check pinvoke-check swift-package-check swift-build swift-test tidy

fmt-check:
    cargo fmt --check

# Fail when slint, slint-* or i-slint-* enters the normal or build dependencies of a
# slintx-* crate on any target: slintx is a clean-room Apache-2.0 implementation.
license-check:
    scripts/slintx-check.sh license

# The gate against throwaway workspaces that do and do not depend on Slint crates.
license-check-test:
    scripts/slintx-check-test.sh

# Build every slintx-* crate for wasm32-unknown-unknown and run its tests there on Node.
wasm-check:
    scripts/slintx-check.sh wasm

clippy:
    cargo clippy --all-targets -- -D warnings

nextest:
    cargo nextest run

# Tests, then shrink `target/` once nothing still has to link against it.
test: nextest tidy

tidy:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v dunnage >/dev/null || exit 0
    dunnage run target || test $? -eq 2

# Regenerate the C header both hosts consume.
header:
    cbindgen --config crates/slint-bindings-ffi/cbindgen.toml --crate slint-bindings-ffi --output {{header_path}} crates/slint-bindings-ffi

# Fail when the committed header no longer matches the Rust source.
header-check:
    #!/usr/bin/env bash
    set -euo pipefail
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    cbindgen --config crates/slint-bindings-ffi/cbindgen.toml --crate slint-bindings-ffi --output "$tmp" crates/slint-bindings-ffi
    diff -u {{header_path}} "$tmp"

# Fail when NativeMethods.cs and the generated header disagree on sb_* exports.
pinvoke-check:
    #!/usr/bin/env bash
    set -euo pipefail
    header="{{header_path}}"
    cs="windows/SlintBindings.WinUI/NativeMethods.cs"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    grep -oE '\bsb_[a-z0-9_]+\(' "$header" | sed 's/($//' | sort -u > "$tmp/header"
    grep -oE '\bsb_[a-z0-9_]+\(' "$cs" | sed 's/($//' | sort -u > "$tmp/cs"
    diff -u "$tmp/header" "$tmp/cs"

# Fail when the SlintBindings library target links anything through unsafe flags.
# The app links the Rust library; a library that also did would pull in a second Slint.
swift-package-check:
    cd swift && swift package dump-package | jq -e '[.targets[] | select(.name == "SlintBindings") | .settings[] | select(.kind | has("unsafeFlags"))] | length == 0' > /dev/null

# The static library the SwiftPM demo and tests link. Built once per `just` invocation.
swift-lib:
    cargo rustc -p slint-bindings-ffi --crate-type staticlib

swift-build: swift-lib
    cd swift && swift build

swift-test: swift-lib
    cd swift && swift test

# Opens the SwiftUI demo window.
swift-run: swift-build
    cd swift && swift run SlintDemo

# On Windows: the DLL the WinUI projects load.
# x64 is the default. ARM64: `just windows-dll aarch64-pc-windows-msvc`.
windows-dll triple="x86_64-pc-windows-msvc":
    cargo rustc -p slint-bindings-ffi --crate-type cdylib --target {{triple}}
