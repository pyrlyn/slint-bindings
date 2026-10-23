#!/usr/bin/env bash
# Checks for the clean-room slintx-* crates (T73.2).
#
#   slintx-check.sh license [Cargo.toml]  fail when slint, slint-* or i-slint-* is in the
#                                          normal or build dependency graph of a slintx-* crate
#   slintx-check.sh wasm [Cargo.toml]     build the slintx-* crates for wasm32-unknown-unknown
#                                          and run their tests there (wasm-bindgen-test on Node)
#
# The manifest defaults to the workspace in the current directory; the fixture test points it
# at throwaway workspaces.
set -euo pipefail

usage() {
    echo "usage: $0 license|wasm [path/to/Cargo.toml]" >&2
    exit 2
}

mode="${1:-}"
manifest="${2:-Cargo.toml}"
[ -f "$manifest" ] || { echo "no manifest at $manifest" >&2; exit 2; }

# Crates a check skips for now. Each entry names the task that removes it.
# license: slintx-wgpu (T72) implements i-slint-core's renderer traits until T83.
# wasm: slintx-wgpu's tests need a native GPU adapter; its wasm32 path comes with T85.
# wasm: slintx-conformance (T74) is a native-only harness: a native Slint oracle and a corpus
# read from disk. Not a removal task, it never ships for wasm32.
# wasm: slintx-convert (T89) is a native CLI whose tests read fixtures from disk and run the
# binary. Not a removal task either.
# shellcheck disable=SC2034 # read through ${!list} in checked()
exempt_license=" slintx-wgpu "
# shellcheck disable=SC2034
exempt_wasm=" slintx-wgpu slintx-conformance slintx-convert "

# Prints the slintx-* crates the current mode does not skip.
checked() {
    local list="exempt_$mode" all crate
    all="$(members)" || return
    for crate in $all; do
        if [[ "${!list}" == *" $crate "* ]]; then
            echo "$mode-check: $crate is exempt (see exempt_$mode in $0)" >&2
        else
            echo "$crate"
        fi
    done
}

members() {
    cargo tree -q --manifest-path "$manifest" --workspace --depth 0 --prefix none -f '{p}' \
        | awk '$1 ~ /^slintx-/ { print $1 }' | sort -u
}

license() {
    local crates="$1" status=0 crate graph hits hit
    for crate in $crates; do
        # --target all: a dependency behind any cfg (wasm32 included) ships on some target.
        # Build dependencies count too: slintx must not run Slint's code while it builds.
        # A separate step, so a cargo failure fails the check instead of reading as no hits.
        graph="$(cargo tree -q --manifest-path "$manifest" -p "$crate" --target all -e normal,build \
            --prefix none -f '{p}')"
        hits="$(awk '{ print $1 }' <<<"$graph" | grep -E '^(slint|slint-.+|i-slint-.+)$' | sort -u || true)"
        for hit in $hits; do
            echo "license-check: $hit is in the dependency graph of $crate:"
            cargo tree -q --manifest-path "$manifest" -p "$crate" --target all -e normal,build -i "$hit"
            status=1
        done
    done
    [ "$status" -eq 0 ] && echo "license-check: no Slint crate in the graph of ${crates//$'\n'/ }"
    return "$status"
}

wasm() {
    local crates="$1" args=() crate missing=0
    for crate in $crates; do
        args+=(-p "$crate")
        # The runner only sees #[wasm_bindgen_test] tests and calls a binary without them a pass
        # ("no tests to run!"), so a crate that is not wired for it would pass untested.
        if ! cargo tree -q --manifest-path "$manifest" -p "$crate" --target wasm32-unknown-unknown \
            -e dev --depth 1 --prefix none -f '{p}' | grep -q '^wasm-bindgen-test '; then
            echo "wasm-check: $crate has no wasm32 dev-dependency on wasm-bindgen-test, its tests would not run"
            missing=1
        fi
    done
    [ "$missing" -eq 0 ] || return 1
    cargo build --manifest-path "$manifest" --target wasm32-unknown-unknown "${args[@]}"
    # --tests: unit and integration tests only; doctests are plain host programs.
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
        cargo test --manifest-path "$manifest" --target wasm32-unknown-unknown --tests "${args[@]}"
}

case "$mode" in
    license | wasm) ;;
    *) usage ;;
esac

crates="$(checked)"
if [ -z "$crates" ]; then
    echo "$mode-check: no slintx-* crate to check in the workspace, nothing to do"
    exit 0
fi
"$mode" "$crates"
