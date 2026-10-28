#!/usr/bin/env bash
# The control of the system-free gate (docs/dev/gates.md, section 1; scripts/gates/system-free.sh). It makes a small
# workspace of its own in a temporary folder, with the three system crates (backend-os, gpu and device), a crate that
# holds none of them, and crates that hold each of them. It runs the check of system-free.sh on that workspace with
# records that are right and wrong, and each expected outcome is worked from the rule in the header of system-free.sh:
# a crate is system-free when its dependency tree (normal, build and development dependencies) holds none of the three.
# The gate itself is controlled too: a copy of scripts/gates.sh runs the gate on a workspace of its own, and the
# default run of the gates must name the system-free gate. The checked-in record is never changed.
#
# Exit codes: 0 every control passed, 1 a control failed (each named on a line "control problem: ..."), 2 wrong usage.

set -u

HERE=$(cd "$(dirname "$0")/../.." && pwd) || exit 2
WORK=$(mktemp -d) || exit 2
trap 'rm -rf "$WORK"' EXIT

problems=0
problem() {
    echo "control problem: $1"
    problems=$((problems + 1))
}

# A workspace with the crates the rule is worked on. $1 is its folder.
make_workspace() {
    local ws="$1" crate
    mkdir -p "$ws/scripts/gates" "$ws/crates"
    cp "$HERE/scripts/gates/system-free.sh" "$ws/scripts/gates/system-free.sh"
    printf '[workspace]\nresolver = "2"\nmembers = ["crates/*"]\n' >"$ws/Cargo.toml"
    for crate in backend-os gpu device plain via-backend via-gpu via-device via-gpu-dev; do
        mkdir -p "$ws/crates/$crate/src"
        : >"$ws/crates/$crate/src/lib.rs"
        printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2021"\n\n' "$crate" >"$ws/crates/$crate/Cargo.toml"
    done
    printf '[dependencies]\nbackend-os = { path = "../backend-os" }\n' >>"$ws/crates/via-backend/Cargo.toml"
    printf '[dependencies]\ngpu = { path = "../gpu" }\n' >>"$ws/crates/via-gpu/Cargo.toml"
    printf '[dependencies]\ndevice = { path = "../device" }\n' >>"$ws/crates/via-device/Cargo.toml"
    printf '[dev-dependencies]\ngpu = { path = "../gpu" }\n' >>"$ws/crates/via-gpu-dev/Cargo.toml"
}

# Runs the check on the workspace $1 with the record text $2. Its output is left in $WORK/out and its exit code in $code.
check_with() {
    local ws="$1" record="$2"
    printf '%s' "$record" >"$ws/scripts/gates/system-free.txt"
    bash "$ws/scripts/gates/system-free.sh" check >"$WORK/out" 2>&1
    code=$?
}

# A control that expects the check to fail with exit 1 and to name each of the given words.
expect_fail() {
    local name="$1"
    shift
    if [ "$code" -ne 1 ]; then
        problem "$name: the check ended with exit $code, expected 1 (its output is in the log of this gate)"
        sed 's/^/    /' "$WORK/out"
        return
    fi
    local word
    for word in "$@"; do
        grep -qF -- "$word" "$WORK/out" || problem "$name: the output does not name \"$word\""
    done
}

ws="$WORK/ws"
make_workspace "$ws"

# The free crates of this workspace, worked by hand: only plain holds none of the three system crates.
check_with "$ws" "plain
"
if [ "$code" -ne 0 ]; then
    problem "a_record_that_matches_the_free_crates_passes: the check ended with exit $code, expected 0"
    sed 's/^/    /' "$WORK/out"
elif ! grep -qF "PASS: the list equals" "$WORK/out"; then
    problem "a_record_that_matches_the_free_crates_passes: the output does not say the list equals the free crates"
fi

check_with "$ws" "plain
via-backend
"
expect_fail "a_record_that_lists_a_crate_holding_backend_os_fails_and_names_it" "FAIL: via-backend" "holds backend-os"

check_with "$ws" "plain
via-gpu
"
expect_fail "a_record_that_lists_a_crate_holding_gpu_fails_and_names_it" "FAIL: via-gpu" "holds gpu"

check_with "$ws" "plain
via-device
"
expect_fail "a_record_that_lists_a_crate_holding_device_fails_and_names_it" "FAIL: via-device" "holds device"

check_with "$ws" "plain
via-gpu-dev
"
expect_fail "a_system_crate_reached_only_through_a_development_dependency_fails_and_is_named" "FAIL: via-gpu-dev" "holds gpu"

check_with "$ws" ""
expect_fail "a_record_that_leaves_out_a_free_crate_fails_and_names_it" "FAIL: plain touches no system interface"

# The default run of the gates names the system-free gate, after the six others, in the order of docs/dev/gates.md,
# section 1: the list is written out here by hand.
default_gates=$(sed -n 's/^\[ -z "\$WANTED" \] && WANTED=" \(.*\)"$/\1/p' "$HERE/scripts/gates.sh")
if [ "$default_gates" != "format lint test core licence pin free" ]; then
    problem "the_default_run_of_the_gates_includes_the_system_free_gate: the default gates are \"$default_gates\", expected \"format lint test core licence pin free\""
fi

# The gate, run as scripts/gates.sh runs it, in a copy of the gate script with a workspace of its own. The control
# step of the gate is a stand-in in that copy, so that the self-test does not start itself again: the checks above
# cover the control. The gate's verdict is what this control checks: it follows the check of the record.
gate_fixture="$WORK/gate"
make_workspace "$gate_fixture"
cp "$HERE/scripts/gates.sh" "$gate_fixture/scripts/gates.sh"
printf '#!/usr/bin/env bash\necho "PASS: the control is not run in this copy of the gate"\nexit 0\n' >"$gate_fixture/scripts/gates/system-free-selftest.sh"
gate_run() {
    printf '%s' "$1" >"$gate_fixture/scripts/gates/system-free.txt"
    (cd "$gate_fixture" && env -u CARGO_TARGET_DIR bash scripts/gates.sh --gate free) >"$WORK/gate-out" 2>&1
    gcode=$?
}

gate_run "plain
"
if [ "$gcode" -ne 3 ] || ! grep -qF "PASS system-free crates" "$WORK/gate-out"; then
    problem "the_gate_passes_a_record_that_matches_the_free_crates: the gate ended with exit $gcode, expected 3 with PASS system-free crates"
    sed 's/^/    /' "$WORK/gate-out"
fi

gate_run "plain
via-backend
"
if [ "$gcode" -ne 1 ]; then
    problem "the_gate_fails_a_record_that_lists_a_crate_holding_backend_os: the gate ended with exit $gcode, expected 1"
    sed 's/^/    /' "$WORK/gate-out"
else
    for word in "FAIL system-free crates" "via-backend is in scripts/gates/system-free.txt" "holds backend-os" "a part failed"; do
        grep -qF -- "$word" "$WORK/gate-out" || problem "the_gate_fails_a_record_that_lists_a_crate_holding_backend_os: the output does not say \"$word\""
    done
fi

# The picture check's lines in a test run's log (system-free.sh verify-log): both lines, with a count above 0, pass; a
# log without a line, or with a count of 0, fails and names what is missing.
verify_with() {
    printf '%s' "$1" >"$WORK/log"
    bash "$HERE/scripts/gates/system-free.sh" verify-log "$WORK/log" >"$WORK/vout" 2>&1
    vcode=$?
}

verify_with "running 1 test
checked-in pictures compared: 202
largest difference found: 0 of 255 (no value differs)
test result: ok.
"
if [ "$vcode" -ne 0 ]; then
    problem "a_log_with_both_picture_lines_passes: the check ended with exit $vcode, expected 0"
    sed 's/^/    /' "$WORK/vout"
fi

verify_with "checked-in pictures compared: 202
"
if [ "$vcode" -ne 1 ] || ! grep -qF "printed no \"largest difference found:\" line" "$WORK/vout"; then
    problem "a_log_without_the_largest_difference_line_fails_and_names_it: exit $vcode, or the line is not named"
fi

verify_with "largest difference found: 0 of 255 (no value differs)
"
if [ "$vcode" -ne 1 ] || ! grep -qF "printed no \"checked-in pictures compared:\" line" "$WORK/vout"; then
    problem "a_log_without_the_compared_line_fails_and_names_it: exit $vcode, or the line is not named"
fi

verify_with "checked-in pictures compared: 0
largest difference found: 0 of 255 (no value differs)
"
if [ "$vcode" -ne 1 ] || ! grep -qF "compared 0 checked-in pictures" "$WORK/vout"; then
    problem "a_log_whose_count_is_0_fails_and_names_it: exit $vcode, or the count is not named"
fi

verify_with "checked-in pictures compared: many
largest difference found: 0 of 255 (no value differs)
"
if [ "$vcode" -ne 1 ]; then
    problem "a_log_whose_count_is_not_a_number_fails: exit $vcode, expected 1"
fi

if [ "$problems" -eq 0 ]; then
    echo "PASS: every control of the system-free gate passed"
    exit 0
fi
echo "FAIL: $problems control problem(s) in the system-free gate"
exit 1
