#!/usr/bin/env bash
# Proves `slintx-check.sh license`, and its wasm exemptions, against throwaway workspaces in
# fixtures/license-check.
# Every dependency there is a local stub named like a Slint crate, so this runs offline.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
check="$here/slintx-check.sh"
# A copy outside the repository: no Cargo.lock or target/ lands in the fixtures, and cargo
# cannot attach them to this repository's workspace.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cp -R "$here/fixtures/license-check/." "$tmp/"
export CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$tmp/target"

failed=0
expect() {
    local want="$1" case="$2" mode="${3:-license}" got=0 out
    out="$("$check" "$mode" "$tmp/$case/Cargo.toml" 2>&1)" || got=$?
    # fail = the gate found a Slint crate (exit 1); error = cargo itself failed (any other code).
    if { [ "$want" = pass ] && [ "$got" -eq 0 ]; } || { [ "$want" = fail ] && [ "$got" -eq 1 ]; } \
        || { [ "$want" = error ] && [ "$got" -gt 1 ]; }; then
        echo "ok    $mode $case ($want)"
    else
        echo "FAIL  $mode $case: expected $want, exit $got"
        echo "$out" | sed 's/^/      /'
        failed=1
    fi
}

expect pass none       # no slintx-* crate: a no-op
expect pass clean      # Slint only as a dev-dependency, or in a crate that is not slintx-*
expect pass exempt     # slintx-wgpu until T83; slintx-conformance has Slint as a dev-dependency only
expect fail direct     # i-slint-core as a normal dependency
expect fail transitive # slint through a helper crate
expect fail build      # slint-build as a build dependency
expect fail wasm-only  # slint only under cfg(target_arch = "wasm32")
expect error broken    # a manifest cargo cannot load fails the check, not passes it
# Neither exempt crate is wired for wasm-bindgen-test, so wasm-check would fail without the
# exemptions; with them it builds nothing, which keeps this offline.
expect pass exempt wasm

exit "$failed"
