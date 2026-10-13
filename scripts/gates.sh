#!/bin/sh
# The four quality gates: formatting, static analysis, tests, licences.
# Run from anywhere; prints one line per gate and exits 0 only when all four pass.
# All four always run, even when one fails. An optional CARGO_TARGET_DIR is honoured.
# Needs the Rust toolchain, cargo-deny and ffmpeg (the tests of the comparison tool make their test videos with it).

cd "$(dirname "$0")/.." || exit 2

failed=""

gate() {
    name=$1
    shift
    echo "---- $name: $*"
    if "$@"; then
        echo "$name: pass"
    else
        echo "$name: FAIL"
        failed="${failed:+$failed, }$name"
    fi
}

gate "formatting" cargo fmt --all -- --check
gate "static analysis" cargo clippy --workspace --all-targets -- -D warnings
gate "tests" cargo test --workspace
gate "licences" cargo deny check licenses

echo "================================"
if [ -z "$failed" ]; then
    echo "all four gates passed"
    exit 0
fi
echo "gates failed: $failed"
exit 1
