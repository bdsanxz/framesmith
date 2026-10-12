#!/bin/sh
# The quality gates: formatting, static analysis, tests, licences.
#
# The four gates run twice: once with a full browser build and once with a headless-only
# build (the two kinds differ in ways that have broken renders before). Each pass names its
# browser at its head and reports its own result, gate by gate. The tests are told which
# browser to use through FRAMESMITH_BROWSER. The script exits 0 only when both passes are
# green; a pass whose build cannot be found is a FAIL, never a skip. Every gate of every
# pass always runs, even when one fails, and the tests run to the end so that every failing
# test is listed, not only the first. An optional CARGO_TARGET_DIR is honoured.
#
# Finding the builds: nothing is installed. Each build is looked for among what a
# development machine already has; the maintainer can name either one:
#   FRAMESMITH_GATES_FULL_BROWSER      path of a full browser program (not a headless shell)
#   FRAMESMITH_GATES_HEADLESS_BROWSER  path of a headless-only program (its file name holds "headless")
# The kind is told by the program's file name, as the tests do (browser_in_use in
# crates/cli/tests/common/sprint1.rs), because both kinds print the same version line.
# Searched when not named, in this order; the first place that has a build wins, and within
# a place the newest version folder:
#   full:     /Applications and ~/Applications (Google Chrome, then Chromium), then the
#             ms-playwright cache, then the puppeteer cache (Google Chrome for Testing)
#   headless: the ms-playwright cache, then the puppeteer cache (chrome-headless-shell)
#
# Maintainer switch, for trying the script itself: FRAMESMITH_GATES_TEST_FILTER=<text> runs
# only the tests whose name holds <text>. Such a run never counts as green: the tests gate is
# reported as NARROWED (or FAIL), the summary and the last line say the run does not count,
# and "all four gates passed" is never printed.
#
# Exit code: 0 both passes green (a whole run); 1 a gate failed or a build was not found;
# 2 the script could not start; 3 a narrowed run in which nothing failed (does not count).
# A failure in a narrowed run is still 1. A push is refused on any code but 0.
#
# Needs only the Rust toolchain, cargo-deny, a browser of each kind and ffmpeg (for the tests).

cd "$(dirname "$0")/.." || exit 2

summary=""
failed_passes=""
partial=""
ok=""

# The newest existing executable among the arguments (globs that match nothing stay
# literal and are skipped). Version folders sort as numbers, so 1223 comes after 999.
newest() {
    for candidate in "$@"; do
        if [ -f "$candidate" ] && [ -x "$candidate" ]; then
            printf '%s\n' "$candidate"
        fi
    done | sort -V | tail -n 1
}

find_full() {
    found=$(newest "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
        "$HOME/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
    [ -n "$found" ] || found=$(newest "/Applications/Chromium.app/Contents/MacOS/Chromium" \
        "$HOME/Applications/Chromium.app/Contents/MacOS/Chromium")
    [ -n "$found" ] || found=$(newest \
        "$HOME/Library/Caches/ms-playwright"/chromium-*/chrome-mac*/"Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" \
        "$HOME/Library/Caches/ms-playwright"/chromium-*/chrome-mac*/"Chromium.app/Contents/MacOS/Chromium" \
        "$HOME/.cache/puppeteer"/chrome/*/chrome-mac*/"Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing")
    printf '%s\n' "$found"
}

find_headless() {
    found=$(newest "$HOME/Library/Caches/ms-playwright"/chromium_headless_shell-*/chrome-headless-shell-mac*/chrome-headless-shell)
    [ -n "$found" ] || found=$(newest "$HOME/.cache/puppeteer"/chrome-headless-shell/*/chrome-headless-shell-mac*/chrome-headless-shell)
    printf '%s\n' "$found"
}

gate() {
    name=$1
    shift
    echo "---- [$label] $name: $*"
    if "$@"; then
        echo "[$label] $name: ${ok:-pass}"
    else
        echo "[$label] $name: FAIL"
        failed="${failed:+$failed, }$name"
    fi
}

# pass <label> <kind: full|headless> <variable that names the build> <what to search>
pass() {
    label=$1
    kind=$2
    var=$3
    searched=$4
    failed=""
    echo "================================================================"
    echo "PASS: $label"
    eval "named=\${$var:-}"
    if [ -n "$named" ]; then
        browser=$named
        how="named by $var"
    elif [ "$kind" = full ]; then
        browser=$(find_full)
        how="found"
    else
        browser=$(find_headless)
        how="found"
    fi
    problem=""
    if [ -z "$browser" ]; then
        problem="no $label found. Looked in: $searched."
    elif [ ! -f "$browser" ] || [ ! -x "$browser" ]; then
        problem="$var names $browser, which is not an executable file."
    else
        lower=$(basename "$browser" | tr '[:upper:]' '[:lower:]')
        case "$lower:$kind" in
            *headless*:full) problem="$var names $browser, a headless-only program; name a full build." ;;
            *headless*:headless) ;;
            *:headless) problem="$var names $browser, whose file name does not hold \"headless\"; the kind is told by the name." ;;
        esac
        if [ -z "$problem" ]; then
            version=$("$browser" --version 2>/dev/null | head -n 1 | sed 's/[[:space:]]*$//')
            [ -n "$version" ] || problem="$browser does not run (--version printed nothing)."
        fi
    fi
    if [ -n "$problem" ]; then
        echo "  build:   NOT FOUND"
        echo "  $problem"
        echo "  Name one: $var=/path/to/program scripts/gates.sh"
        echo "[$label] gates: FAIL (not run: no usable build)"
        summary="$summary$label: FAIL (no usable build)
"
        failed_passes="${failed_passes:+$failed_passes; }$label (no usable build)"
        return
    fi
    echo "  kind:    $kind"
    echo "  product: ${version% *}"
    echo "  version: ${version##* }"
    echo "  path:    $browser ($how)"
    echo "================================================================"
    FRAMESMITH_BROWSER=$browser
    export FRAMESMITH_BROWSER
    gate "formatting" cargo fmt --all -- --check
    gate "static analysis" cargo clippy --workspace --all-targets -- -D warnings
    if [ -n "${FRAMESMITH_GATES_TEST_FILTER:-}" ]; then
        partial=yes
        ok="NARROWED (not a full run)"
        gate "tests" cargo test --workspace --no-fail-fast -- "$FRAMESMITH_GATES_TEST_FILTER"
        ok=""
    else
        gate "tests" cargo test --workspace --no-fail-fast
    fi
    gate "licences" cargo deny check licenses
    if [ -n "$failed" ]; then
        summary="$summary$label: FAIL ($failed)
"
        failed_passes="${failed_passes:+$failed_passes; }$label ($failed)"
    elif [ -n "${FRAMESMITH_GATES_TEST_FILTER:-}" ]; then
        summary="$summary$label: NARROWED, does not count (the tests were narrowed to \"$FRAMESMITH_GATES_TEST_FILTER\")
"
    else
        summary="$summary$label: pass (all four gates)
"
    fi
}

pass "full browser build" full FRAMESMITH_GATES_FULL_BROWSER \
    "/Applications and ~/Applications (Google Chrome.app, Chromium.app), ~/Library/Caches/ms-playwright/chromium-*, ~/.cache/puppeteer/chrome/*"
pass "headless-only browser build" headless FRAMESMITH_GATES_HEADLESS_BROWSER \
    "~/Library/Caches/ms-playwright/chromium_headless_shell-*, ~/.cache/puppeteer/chrome-headless-shell/*"

echo "================================================================"
printf '%s' "$summary"
if [ -n "$failed_passes" ]; then
    echo "GATES FAILED: $failed_passes"
    exit 1
fi
if [ -n "$partial" ]; then
    echo "NARROWED RUN, DOES NOT COUNT as green: FRAMESMITH_GATES_TEST_FILTER is set, so the tests did not all run (exit code 3)"
    exit 3
fi
echo "all four gates passed with both browser builds"
exit 0
