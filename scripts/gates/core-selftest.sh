#!/usr/bin/env bash
# The control of the core gate: before the real core suite is believed, this makes a small
# workspace of tests that misbehave on purpose and shows that the core suite's machinery
# (scripts/gates/core-suite.sh) fails them by name, and lets the ones that behave pass. A gate whose
# trap no longer closes would otherwise pass for ever. See docs/dev/gates.md, section 10.
#
# Every problem is a line that starts with "core self-test problem:". Exit code 0 when there
# is none, 1 otherwise.
#
# Written for the bash 3.2 that macOS ships.

set -u

REPO=$(cd "$(dirname "$0")/../.." && pwd -P) || exit 2
export CARGO_TERM_COLOR=never
export NO_COLOR=1
# The deliberate failures of this control reach no runner (docs/dev/gates.md, section 12).
unset GITHUB_ACTIONS GITHUB_STEP_SUMMARY GITHUB_OUTPUT GITHUB_ENV GITHUB_PATH

BASE=$(cargo metadata --no-deps --format-version 1 --manifest-path "$REPO/Cargo.toml" | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
[ -n "$BASE" ] || { echo "core self-test problem: cargo could not name the target folder"; exit 1; }
SELF="$BASE/gates/core-selftest"
rm -rf "$SELF/crates" "$SELF/decoy" "$SELF/Cargo.toml" "$SELF/Cargo.lock"
mkdir -p "$SELF/crates/offenders/tests" "$SELF/crates/offenders/src" "$SELF/decoy/outside/bin" || exit 2

# The workspace: one crate whose tests start programs.
cat >"$SELF/Cargo.toml" <<'EOT'
[workspace]
resolver = "2"
members = ["crates/offenders"]
EOT
cat >"$SELF/crates/offenders/Cargo.toml" <<'EOT'
[package]
name = "offenders"
version = "0.0.0"
edition = "2021"
publish = false
EOT
: >"$SELF/crates/offenders/src/lib.rs"
# Programs outside the target folder and outside the system's, to start by their paths. They do
# nothing; they only have to be files that can be run. One is named ffmpeg; the other stands for
# the reference, a command that is installed in a folder of its own outside the repository.
printf '#!/bin/sh\nexit 0\n' >"$SELF/decoy/ffmpeg"
printf '#!/bin/sh\nexit 0\n' >"$SELF/decoy/outside/bin/outside-command"
chmod +x "$SELF/decoy/ffmpeg" "$SELF/decoy/outside/bin/outside-command"
cat >"$SELF/crates/offenders/tests/offending.rs" <<'EOT'
use std::process::Command;

#[test]
fn starts_ffmpeg_by_name() {
    Command::new("ffmpeg").arg("-version").output().unwrap();
}

#[test]
fn starts_ffprobe_by_name() {
    Command::new("ffprobe").arg("-version").output().unwrap();
}

#[test]
fn starts_ffmpeg_by_its_path() {
    Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../decoy/ffmpeg")).output().unwrap();
}

#[test]
fn starts_a_command_installed_outside_the_target_and_the_system() {
    Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../decoy/outside/bin/outside-command")).output().unwrap();
}

#[test]
fn starts_a_system_tool() {
    assert!(Command::new("/bin/echo").arg("hello").output().unwrap().status.success());
}

#[test]
fn takes_ffmpeg_not_being_found_for_success() {
    assert!(Command::new("ffmpeg").output().is_err());
}

#[test]
fn starts_nothing() {}
EOT

OUT="$BASE/gates/core-selftest-suite.log"
echo "\$ scripts/gates/core-suite.sh --hosted --root $SELF      (a workspace of tests that start programs on purpose)"
CARGO_TARGET_DIR="$SELF/target" "$REPO/scripts/gates/core-suite.sh" --hosted --root "$SELF" >"$OUT" 2>&1
CODE=$?
echo "[exit $CODE]"

FAILED=0
problem() { echo "core self-test problem: $1"; FAILED=1; }

[ "$CODE" -eq 1 ] || problem "the core suite of the misbehaving workspace ended with exit code $CODE, and not 1"

# Each test that starts a program it may not must fail, by name; the rest must pass.
for name in starts_ffmpeg_by_name starts_ffprobe_by_name starts_ffmpeg_by_its_path starts_a_command_installed_outside_the_target_and_the_system; do
    if grep -q "^test $name \.\.\. FAILED$" "$OUT"; then
        echo "failed by name, as it must: $name"
    else
        problem "the test $name starts a program it may not and was not failed by name"
    fi
done
for name in starts_a_system_tool takes_ffmpeg_not_being_found_for_success starts_nothing; do
    if grep -q "^test $name \.\.\. ok$" "$OUT"; then
        echo "passed, as it must: $name"
    else
        problem "the test $name was not run and passed"
    fi
done
FAILING=$(grep -c '^test .* \.\.\. FAILED$' "$OUT")
[ "$FAILING" -eq 4 ] || problem "$FAILING tests failed and 4 were to"

# The failures name the test with the place of its panic, and the programs the system refused are named.
if grep -q "panicked at crates/offenders/tests/offending.rs" "$OUT"; then
    echo "the failures are named with the place of their panic"
else
    problem "no failure was named with the place of its panic in the test file"
fi
if grep -q "^core problem: the system refused to start .*decoy/ffmpeg" "$OUT" &&
    grep -q "^core problem: the system refused to start .*decoy/outside/bin/outside-command" "$OUT"; then
    echo "the programs the system refused to start are named"
else
    problem "the programs the system refused to start were not both named (the system log could not be read, or the profile did not refuse them)"
fi

# A test that takes "not found" for success starts nothing the sandbox sees; its text is found.
if grep -q '^core problem: the core test file crates/offenders/tests/offending.rs names ffmpeg or ffprobe at line [0-9]* (in fn takes_ffmpeg_not_being_found_for_success)' "$OUT"; then
    echo "found by its text, as it must: takes_ffmpeg_not_being_found_for_success"
else
    problem "the placement check did not name takes_ffmpeg_not_being_found_for_success"
fi

[ "$FAILED" -eq 0 ] && echo "the core gate's machinery closes its trap: every misbehaving test is named, every other one passes"
exit "$FAILED"
