#!/usr/bin/env bash
# The parts of Framesmith that touch no system interface, and their tests on other systems.
#
#   scripts/gates/system-free.sh list    the workspace crates that are system-free, worked out by cargo
#   scripts/gates/system-free.sh check   the record scripts/gates/system-free.txt equals that list
#   scripts/gates/system-free.sh test    check, then build and test exactly the recorded crates
#
# A crate touches a system interface when its dependency tree (normal, build and development
# dependencies) holds one of frx7's system crates: backend-os, gpu, device.
# Exit codes: 0 passed, 1 failed (each failure named on a line "FAIL: ..."), 2 wrong usage.
# Nothing is packaged or kept: the tests are built in the debug profile into target/ and nothing else is made.
# Written for bash 3.2 and for the Git bash of Windows.

set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd) || exit 2
cd "$ROOT" || exit 2
RECORD="$ROOT/scripts/gates/system-free.txt"
SYSTEM_CRATES="backend-os gpu device"
export CARGO_TERM_COLOR=never

summary() {
    [ "${GITHUB_ACTIONS:-}" = "true" ] && [ -n "${GITHUB_STEP_SUMMARY:-}" ] && printf '%s\n' "$1" >>"$GITHUB_STEP_SUMMARY"
    return 0
}

members() {
    # The package names of the workspace members, from their Cargo.toml files.
    local manifest
    for manifest in "$ROOT"/crates/*/Cargo.toml; do
        sed -n '/^\[package\]/,/^\[/s/^name *= *"\(.*\)".*$/\1/p' "$manifest" | head -n 1
    done
}

computed() {
    local crate tree system found
    for crate in $(members); do
        tree=$(cargo tree -p "$crate" -e normal,build,dev --prefix none --format '{p}' 2>&1) || {
            echo "FAIL: cargo tree could not read the dependencies of $crate:" >&2
            printf '%s\n' "$tree" >&2
            return 1
        }
        found=""
        for system in $SYSTEM_CRATES; do
            printf '%s\n' "$tree" | grep -q "^$system v" && found="$found $system"
        done
        [ -z "$found" ] && echo "$crate"
    done
    return 0
}

recorded() { grep -v '^#' "$RECORD" | grep -v '^[[:space:]]*$' | tr -d '\r'; }

do_check() {
    local want have missing extra
    want=$(computed | sort) || return 1
    have=$(recorded | sort)
    missing=$(comm -23 <(printf '%s\n' "$want") <(printf '%s\n' "$have") | grep -v '^$')
    extra=$(comm -13 <(printf '%s\n' "$want") <(printf '%s\n' "$have") | grep -v '^$')
    echo "system-free crates of the workspace: $(printf '%s ' $want)"
    echo "recorded in scripts/gates/system-free.txt: $(printf '%s ' $have)"
    local code=0 crate
    for crate in $missing; do
        echo "FAIL: $crate touches no system interface and is not in scripts/gates/system-free.txt"
        summary "- FAIL: $crate touches no system interface and is not in the list"
        code=1
    done
    for crate in $extra; do
        echo "FAIL: $crate is in scripts/gates/system-free.txt but is not a system-free crate of the workspace"
        summary "- FAIL: $crate is in the list but is not a system-free crate of the workspace"
        code=1
    done
    [ "$code" -eq 0 ] && echo "PASS: the list equals the system-free crates of the workspace"
    return "$code"
}

do_test() {
    local scope=() crate code log
    do_check || return 1
    for crate in $(recorded); do scope+=(-p "$crate"); done
    log="$ROOT/target/system-free-test.log"
    mkdir -p "$ROOT/target"
    echo "\$ cargo test --locked ${scope[*]} --no-fail-fast"
    ( cargo test --locked "${scope[@]}" --no-fail-fast 2>&1; echo $? >"$ROOT/target/system-free-test.code" ) | tee "$log"
    code=$(cat "$ROOT/target/system-free-test.code")
    summary "### System-free crates on $(uname -s)"
    summary "Crates: $(recorded | tr '\n' ' ')"
    if [ "$code" -eq 0 ]; then
        echo "PASS: the system-free crates built and their tests passed"
        summary "PASS: built, and every test passed"
        return 0
    fi
    local failed
    failed=$(grep '^test .* \.\.\. FAILED$' "$log" | sed -e 's/^test //' -e 's/ \.\.\. FAILED$//')
    if [ -n "$failed" ]; then
        printf '%s\n' "$failed" | while read -r name; do echo "FAIL: test $name failed"; summary "- FAIL: test $name failed"; done
    else
        grep -E '^error(\[[^]]*\])?: ' "$log" | sort -u | while read -r line; do echo "FAIL: $line"; summary "- FAIL: $line"; done
        echo "FAIL: cargo test ended with exit code $code"
        summary "- FAIL: cargo test ended with exit code $code"
    fi
    return 1
}

case "${1:-}" in
    list) computed ;;
    check) do_check ;;
    test) do_test ;;
    *) echo "usage: scripts/gates/system-free.sh list|check|test" >&2; exit 2 ;;
esac
