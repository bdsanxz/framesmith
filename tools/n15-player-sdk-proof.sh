#!/bin/sh
# n15-player-sdk-proof.sh -- Sprint 15 T009 one-command gate.
#
# Proves the player-SDK reference project end to end: the staged copy
# of framesmith/examples/player-sdk-reference embeds with ordered
# events, round-trips a headless edit through exact undo and redo,
# declares/validates/scans variables, plans timeline verbs with counts
# and applies them with undo receipts, and interleaves guarded history
# turns across named writers -- with a tamper negative proving the
# gate exits non-zero when any single check fails. Reports PROOF: PASS
# only when every check succeeds (exit 0).
#
# Usage: n15-player-sdk-proof.sh [--workdir DIR]
#   The reference is staged to DIR (default: fresh mktemp directory);
#   the checkout itself is never written. Evidence is copied to
#   evidences/proof-n15-T009/ on every run.

set -u

WORKDIR=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,21p' "$0"; exit 0 ;;
    *) echo "n15-player-sdk-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/player-sdk-reference"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n15-T009"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n15-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n15 player-sdk proof gate (T009) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "================================================================"
echo ""

for NEED in cargo; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl README.md; do
  if [ ! -f "$EXAMPLE_DIR/$F" ]; then
    fail "prereq-example-$F" "missing $EXAMPLE_DIR/$F"
  fi
done
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- missing prerequisites:$FAIL_LIST"
  exit 1
fi

# Stage the reference: the checkout is never written or rendered into.
PROJ="$WORKDIR/proj"
rm -rf "$PROJ"
mkdir -p "$PROJ"
cp "$EXAMPLE_DIR/composition.kdl" "$PROJ/"

# Build the proof harness (a dev-only example, never shipped).
echo "--- build proof harness ---"
if (cd "$FRAMESMITH_DIR" && cargo build -p sdk --example n15_proof >"$WORKDIR/build.log" 2>&1); then
  pass "harness-builds"
else
  fail "harness-builds" "cargo build failed; see $WORKDIR/build.log"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
HARNESS="$FRAMESMITH_DIR/target/debug/examples/n15_proof"

# Run the full gate on the staged reference.
echo "--- proof run ---"
if "$HARNESS" "$PROJ" >"$WORKDIR/harness.log" 2>&1; then
  pass "harness-exit-0"
else
  fail "harness-exit-0" "exit non-zero; see $WORKDIR/harness.log"
fi
cat "$WORKDIR/harness.log"
if grep -q "^PROOF: PASS" "$WORKDIR/harness.log"; then
  pass "harness-reports-proof-pass"
else
  fail "harness-reports-proof-pass" "no PROOF: PASS line in harness output"
fi
if grep -q "FAIL" "$WORKDIR/harness.log"; then
  fail "harness-no-fail-lines" "FAIL lines present in harness output"
else
  pass "harness-no-fail-lines"
fi
for GATE in sdk-open-ready player-begins-loading player-loads-ready \
    player-events-ordered player-embed-snippet player-playback-ordered \
    edit-apply-visible edit-undo-exact edit-redo-exact \
    var-validate-clean var-declare var-declare-refuses-duplicate \
    var-validate-flags-unbound var-scan \
    timeline-plan-count timeline-apply-receipt timeline-undo-exact \
    history-turns-recorded history-guard-begin history-guard-held \
    history-guard-released history-interleaved-order; do
  if grep -q "^CHECK $GATE: PASS" "$WORKDIR/harness.log"; then
    pass "gate-$GATE"
  else
    fail "gate-$GATE" "no CHECK $GATE: PASS line in harness output"
  fi
done

# Tamper negative: a corrupted composition must refuse (non-zero exit).
echo "--- tamper negative ---"
TAMPER="$WORKDIR/tampered"
rm -rf "$TAMPER"
cp -r "$PROJ" "$TAMPER"
printf 'composition name="player-sdk" width=320 height=180 duration=6 fps=10 background="not-a-color" version=1 {\n}\n' >"$TAMPER/composition.kdl"
if "$HARNESS" "$TAMPER" >"$WORKDIR/tamper.log" 2>&1; then
  fail "tamper-refused-non-zero" "harness exited 0 on a corrupted project"
else
  pass "tamper-refused-non-zero (exit $?)"
fi

# ---- evidence ------------------------------------------------------------
echo ""
echo "--- proof summary: $PASS_N passed, failing:${FAIL_LIST:- none} ---"
{
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "sdk: $(grep -E '^sdk ' "$WORKDIR/harness.log" | head -n 1)"
  echo "cargo: $(cargo --version 2>/dev/null)"
  echo "rustc: $(rustc --version 2>/dev/null)"
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/harness.log" "$WORKDIR/tamper.log" \
  "$WORKDIR/build.log" "$EVID_DIR/" 2>/dev/null || true
cp "$EXAMPLE_DIR/composition.kdl" "$EVID_DIR/composition-reference.kdl" 2>/dev/null || true
cp "$EXAMPLE_DIR/README.md" "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
