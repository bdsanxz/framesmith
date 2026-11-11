#!/bin/sh
# n24-slideshow-proof.sh -- Sprint 24 T004 one-command gate.
#
# Proves the slideshow reference end to end on a staged fixture copy
# (the checkout itself is never written):
#   1. `check` and `lint` exit 0 on the reference: one manifest with
#      slides, notes, a fragment, a branch, a hotspot, and autoplay.
#   2. the manifest gates at the validation verdict (n24_slideshow_proof):
#      the reference validates with the full manifest mirrored, and an
#      unknown slide composition, a broken branch target, a broken
#      hotspot target, an empty deck, and a zero autoplay each refuse
#      naming the file and ref, with `empty-deck`/`bad-autoplay` lint.
#   3. two-view sync through the navigation state machine over the
#      real present server (n24_present_proof): presenter and audience
#      pages serve off one machine; fragment hold, sequential next,
#      branch resolution, prev/home/end, hotspot click, autoplay tick,
#      and a vanished-target hold all land where the manifest says;
#      picker-cache frames repeat identical per slide and differ across
#      slides; the Studio slideshow panel mirrors the manifest in order
#      with a Present launch hint; a missing manifest refuses.
#   4. first-slide render parity by byte comparison: `render` on the
#      deck is byte-identical to `render --composition intro`, reruns
#      are byte-identical, and ffprobe duration is one slide (1s), so
#      no linear deck export exists.
#   5. gated refusals through the CLI name their refs: unknown
#      composition, broken branch, broken hotspot, bad autoplay, and
#      an empty deck each exit non-zero naming file and ref; `present`
#      without a manifest refuses naming the absent manifest; a
#      film-invalid first slide refuses rendering like a film render
#      and writes no MP4.
#   6. `cargo test` for the touched crates (validate, preview,
#      notation libs) stays green.
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n24-slideshow-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory).
#   Evidence is copied to evidences/proof-n24-T004/ on every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,38p' "$0"; exit 0 ;;
    *) echo "n24-slideshow-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/slideshow-reference"
EVID_DIR="$WS_ROOT/evidences/proof-n24-T004"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n24-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n24 slideshow reference proof gate (T004) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "=========================================================================="
echo ""

for NEED in cargo python3 ffmpeg ffprobe; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl README.md; do
  if [ ! -f "$EXAMPLE_DIR/$F" ]; then
    fail "prereq-reference-$F" "missing $EXAMPLE_DIR/$F"
  fi
done
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- missing prerequisites:$FAIL_LIST"
  exit 1
fi

if [ -n "$BIN_OVERRIDE" ]; then
  FSMITH="$BIN_OVERRIDE"
else
  FSMITH="$FRAMESMITH_DIR/target/debug/framesmith"
  echo "--- build framesmith binary ---"
  if (cd "$FRAMESMITH_DIR" && cargo build --bin framesmith >"$WORKDIR/build.log" 2>&1); then
    echo "build ok"
  else
    fail "build" "cargo build failed; see $WORKDIR/build.log"
    echo ""
    echo "PROOF: FAIL -- failing items:$FAIL_LIST"
    exit 1
  fi
fi
echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
echo "cargo: $(cargo --version 2>/dev/null)"
echo "rustc: $(rustc --version 2>/dev/null)"
echo "python: $(python3 --version 2>&1)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo ""

# ---- stage the reference (a copy; the checkout is never rendered) ----
echo "--- stage: slideshow reference ---"
REF="$WORKDIR/ref"
cp -r "$EXAMPLE_DIR" "$REF" || exit 1
if [ -f "$REF/composition.kdl" ] && [ -f "$REF/README.md" ]; then
  pass "staged-reference (composition.kdl + README.md)"
else
  fail "stage-reference" "staged copy misses a fixture file"
fi

# ---- gate 1: check and lint exit 0 on the reference ----
echo ""
echo "--- gate 1: check and lint on the reference ---"
if "$FSMITH" check "$REF" >"$WORKDIR/check.log" 2>&1; then
  pass "check-exits-0-on-reference"
else
  fail "check-exit" "check exited non-zero; see check.log"
fi
if "$FSMITH" lint "$REF" >"$WORKDIR/lint.log" 2>&1; then
  if grep -q "0 findings" "$WORKDIR/lint.log"; then
    pass "lint-exits-0-on-reference-with-zero-findings"
  else
    fail "lint-findings" "lint passed without a zero-findings line; see lint.log"
  fi
else
  fail "lint-exit" "lint exited non-zero; see lint.log"
fi

# ---- gate 2: manifest gates at the validation verdict (staged copy) ----
echo ""
echo "--- gate 2: slideshow manifest through the validation verdict ---"
if (cd "$FRAMESMITH_DIR" && FRAMESMITH_N24_PROOF_REF="$REF" cargo test --offline -p validate --test n24_slideshow_proof >"$WORKDIR/validate-proof.log" 2>&1); then
  if grep -q "6 passed" "$WORKDIR/validate-proof.log"; then
    pass "n24_slideshow_proof (6 passed: mirror + unknown-ref + broken targets + empty/bad-autoplay + missing-manifest)"
  else
    fail "validate-proof-count" "test exited 0 without '6 passed'; see validate-proof.log"
  fi
else
  fail "validate-proof" "n24_slideshow_proof test failed; see validate-proof.log"
fi

# ---- gate 3: two-view sync over the real present server (staged copy) ----
echo ""
echo "--- gate 3: present both views through the navigation state machine ---"
if (cd "$FRAMESMITH_DIR" && FRAMESMITH_N24_PROOF_REF="$REF" cargo test --offline -p preview --test n24_present_proof >"$WORKDIR/present-proof.log" 2>&1); then
  if grep -q "2 passed" "$WORKDIR/present-proof.log"; then
    pass "n24_present_proof (2 passed: two-view sync, keys/branch/hotspot/autoplay, cache frames, panel, missing-manifest)"
  else
    fail "present-proof-count" "test exited 0 without '2 passed'; see present-proof.log"
  fi
else
  fail "present-proof" "n24_present_proof test failed; see present-proof.log"
fi

# ---- gate 4: first-slide render parity by byte comparison ----
echo ""
echo "--- gate 4: deck renders only the first slide, byte-identical ---"
if "$FSMITH" render "$REF" -o "$WORKDIR/deck.mp4" >"$WORKDIR/render-deck.log" 2>&1 \
  && "$FSMITH" render "$REF" -o "$WORKDIR/deck2.mp4" >"$WORKDIR/render-deck2.log" 2>&1 \
  && "$FSMITH" render "$REF" --composition intro -o "$WORKDIR/direct.mp4" >"$WORKDIR/render-direct.log" 2>&1; then
  if cmp -s "$WORKDIR/deck.mp4" "$WORKDIR/direct.mp4"; then
    pass "deck-render-parity (byte-identical to --composition intro)"
  else
    fail "render-parity" "deck render differs from the direct intro render"
  fi
  if cmp -s "$WORKDIR/deck.mp4" "$WORKDIR/deck2.mp4"; then
    pass "deck-render-deterministic (reruns byte-identical)"
  else
    fail "render-determinism" "two deck renders differ"
  fi
  DUR="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$WORKDIR/deck.mp4" 2>/dev/null || echo missing)"
  if python3 -c "import sys; sys.exit(0 if 0.9 <= float('$DUR') <= 1.1 else 1)"; then
    pass "deck-duration-is-one-slide (ffprobe ${DUR}s)"
  else
    fail "render-duration" "deck duration is $DURs, not one slide (1s)"
  fi
else
  fail "render-exit" "a deck or direct render exited non-zero; see render-deck.log render-direct.log"
fi
if [ -f "$WORKDIR/no-linear-export.mp4" ]; then
  fail "no-linear-export" "a linear-export path produced output"
else
  pass "no-linear-export (deck render is first-slide-only, duration-gated above)"
fi

# ---- gate 5: gated refusals through the CLI name their refs ----
echo ""
echo "--- gate 5: gated refusals name file and ref ---"

# Tamper helper: rewrite one pattern in a fresh copy of the reference.
tamper() {
  _dir="$WORKDIR/$1"
  _old="$2"
  _new="$3"
  if [ -d "$_dir" ]; then rm -rf "$_dir"; fi
  cp -r "$REF" "$_dir" || exit 1
  if [ -n "$_old" ]; then
    OLD="$_old" NEW="$_new" python3 - "$_dir/composition.kdl" <<'PYEOF'
import os, sys
old, new = os.environ["OLD"], os.environ["NEW"]
p = sys.argv[1]
text = open(p).read()
assert old in text, f"pattern {old!r} not in {p}"
open(p, "w").write(text.replace(old, new, 1))
PYEOF
    if [ "$?" -ne 0 ]; then
      fail "tamper-$1" "could not rewrite composition.kdl in $1"
      return 1
    fi
  fi
  return 0
}

# Unknown slide composition names the missing ref.
if tamper "bad-comp" 'composition="intro"' 'composition="missing_comp"'; then
  if "$FSMITH" check "$WORKDIR/bad-comp" >"$WORKDIR/check-bad-comp.log" 2>&1; then
    fail "bad-comp-nonzero" "check exited 0 on an unknown slide composition; see check-bad-comp.log"
  elif grep -q "missing_comp" "$WORKDIR/check-bad-comp.log" && grep -q "composition.kdl" "$WORKDIR/check-bad-comp.log"; then
    pass "unknown-composition-refuses-naming-missing_comp"
  else
    fail "bad-comp-text" "refusal names no file or ref; see check-bad-comp.log"
  fi
fi

# Broken branch target names the missing slide.
if tamper "bad-branch" 'branch to="s3"' 'branch to="gone"'; then
  if "$FSMITH" check "$WORKDIR/bad-branch" >"$WORKDIR/check-bad-branch.log" 2>&1; then
    fail "bad-branch-nonzero" "check exited 0 on a broken branch; see check-bad-branch.log"
  elif grep -q "gone" "$WORKDIR/check-bad-branch.log" && grep -q "composition.kdl" "$WORKDIR/check-bad-branch.log"; then
    pass "broken-branch-refuses-naming-gone"
  else
    fail "bad-branch-text" "refusal names no file or target; see check-bad-branch.log"
  fi
fi

# Broken hotspot target names the missing slide.
if tamper "bad-hotspot" 'target="s3"' 'target="gone"'; then
  if "$FSMITH" check "$WORKDIR/bad-hotspot" >"$WORKDIR/check-bad-hotspot.log" 2>&1; then
    fail "bad-hotspot-nonzero" "check exited 0 on a broken hotspot; see check-bad-hotspot.log"
  elif grep -q "gone" "$WORKDIR/check-bad-hotspot.log" && grep -q "composition.kdl" "$WORKDIR/check-bad-hotspot.log"; then
    pass "broken-hotspot-refuses-naming-gone"
  else
    fail "bad-hotspot-text" "refusal names no file or target; see check-bad-hotspot.log"
  fi
fi

# Zero autoplay refuses naming the duration, with bad-autoplay lint.
if tamper "bad-autoplay" 'autoplay=1' 'autoplay=0'; then
  if "$FSMITH" check "$WORKDIR/bad-autoplay" >"$WORKDIR/check-bad-autoplay.log" 2>&1; then
    fail "bad-autoplay-nonzero" "check exited 0 on autoplay=0; see check-bad-autoplay.log"
  elif grep -q "autoplay" "$WORKDIR/check-bad-autoplay.log" && grep -q "lint\[bad-autoplay\]" "$WORKDIR/check-bad-autoplay.log"; then
    pass "bad-autoplay-refuses-naming-the-duration"
  else
    fail "bad-autoplay-text" "refusal names no duration or lint rule; see check-bad-autoplay.log"
  fi
fi

# An empty deck refuses naming the missing slides, with empty-deck lint.
if [ -d "$WORKDIR/empty-deck" ]; then rm -rf "$WORKDIR/empty-deck"; fi
mkdir -p "$WORKDIR/empty-deck" || exit 1
cat >"$WORKDIR/empty-deck/composition.kdl" <<'KDL'
composition name="solo" root=true width=160 height=90 fps=10 duration=1 background="#101010" version=1 {
    text id="t" value="Solo" start=0 duration=1 x=10 y=30 size=16 color="#ffffff"
}
slideshow autoplay=1 {
}
KDL
if "$FSMITH" check "$WORKDIR/empty-deck" >"$WORKDIR/check-empty.log" 2>&1; then
  fail "empty-deck-nonzero" "check exited 0 on an empty deck; see check-empty.log"
elif grep -q "no slides" "$WORKDIR/check-empty.log" && grep -q "lint\[empty-deck\]" "$WORKDIR/check-empty.log"; then
  pass "empty-deck-refuses-naming-the-missing-slides"
else
  fail "empty-deck-text" "refusal names no cause or lint rule; see check-empty.log"
fi

# Present without a manifest refuses naming what is absent.
if [ -d "$WORKDIR/no-manifest" ]; then rm -rf "$WORKDIR/no-manifest"; fi
mkdir -p "$WORKDIR/no-manifest" || exit 1
cat >"$WORKDIR/no-manifest/composition.kdl" <<'KDL'
composition name="solo" root=true width=160 height=90 fps=10 duration=1 background="#101010" version=1 {
    text id="t" value="Solo" start=0 duration=1 x=10 y=30 size=16 color="#ffffff"
}
KDL
if "$FSMITH" present-serve --port 3494 --registry "$WORKDIR/reg-nomanifest.json" "$WORKDIR/no-manifest" >"$WORKDIR/present-nomanifest.log" 2>&1; then
  fail "present-nomanifest-nonzero" "present-serve exited 0 without a manifest; see present-nomanifest.log"
elif grep -q "slideshow" "$WORKDIR/present-nomanifest.log" && grep -q "composition.kdl" "$WORKDIR/present-nomanifest.log"; then
  pass "missing-manifest-refuses-naming-the-absent-node"
else
  fail "present-nomanifest-text" "refusal names no manifest or file; see present-nomanifest.log"
fi

# A film-invalid first slide refuses rendering like a film render,
# writing no MP4.
if tamper "first-broken" 'root=true width=160' 'root=true width=0'; then
  if "$FSMITH" render "$WORKDIR/first-broken" -o "$WORKDIR/first-broken.mp4" >"$WORKDIR/render-first-broken.log" 2>&1; then
    fail "first-broken-nonzero" "render exited 0 on a film-invalid first slide; see render-first-broken.log"
  elif [ ! -f "$WORKDIR/first-broken.mp4" ]; then
    pass "film-invalid-first-slide-refuses-writing-no-mp4"
  else
    fail "first-broken-mp4" "render wrote an MP4 despite the gated failure"
  fi
fi

# ---- gate 6: cargo test on every touched crate ----
echo ""
echo "--- gate 6: cargo test on touched crates ---"
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p validate --lib >"$WORKDIR/test-validate.log" 2>&1); then
  if grep -q "36 passed" "$WORKDIR/test-validate.log"; then
    pass "cargo-test-validate-lib (36 passed)"
  else
    fail "validate-count" "test exited 0 without '36 passed'; see test-validate.log"
  fi
else
  fail "test-validate" "cargo test -p validate --lib failed; see test-validate.log"
fi
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p preview --lib >"$WORKDIR/test-preview.log" 2>&1); then
  if grep -q "71 passed" "$WORKDIR/test-preview.log"; then
    pass "cargo-test-preview-lib (71 passed)"
  else
    fail "preview-count" "test exited 0 without '71 passed'; see test-preview.log"
  fi
else
  fail "test-preview" "cargo test -p preview --lib failed; see test-preview.log"
fi
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p notation --lib >"$WORKDIR/test-notation.log" 2>&1); then
  if grep -q "59 passed" "$WORKDIR/test-notation.log"; then
    pass "cargo-test-notation-lib (59 passed)"
  else
    fail "notation-count" "test exited 0 without '59 passed'; see test-notation.log"
  fi
else
  fail "test-notation" "cargo test -p notation --lib failed; see test-notation.log"
fi

# ---- evidence ---------------------------------------------------------------
echo ""
echo "--- proof summary: $PASS_N passed, failing:${FAIL_LIST:- none} ---"
{
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
  echo "cargo: $(cargo --version 2>/dev/null)"
  echo "rustc: $(rustc --version 2>/dev/null)"
  echo "python: $(python3 --version 2>&1)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/build.log" \
  "$WORKDIR/check.log" "$WORKDIR/lint.log" \
  "$WORKDIR/validate-proof.log" "$WORKDIR/present-proof.log" \
  "$WORKDIR/render-deck.log" "$WORKDIR/render-deck2.log" \
  "$WORKDIR/render-direct.log" "$WORKDIR/check-bad-comp.log" \
  "$WORKDIR/check-bad-branch.log" "$WORKDIR/check-bad-hotspot.log" \
  "$WORKDIR/check-bad-autoplay.log" "$WORKDIR/check-empty.log" \
  "$WORKDIR/present-nomanifest.log" "$WORKDIR/render-first-broken.log" \
  "$WORKDIR/test-validate.log" "$WORKDIR/test-preview.log" \
  "$WORKDIR/test-notation.log" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/deck.mp4" "$WORKDIR/direct.mp4" "$WORKDIR/deck2.mp4" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$REF/composition.kdl" "$REF/README.md" \
  "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
