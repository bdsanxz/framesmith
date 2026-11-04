#!/bin/sh
# n09-variables-proof.sh — Sprint 9 T007 one-command gate.
#
# Proves the variables reference project end to end: one composition
# declaring all seven variable types plus fallback/strict-demo
# declarations, text/media/attribute/motion bindings, two themes, and
# batch rows with one bad row. Stages the reference from
# framesmith/examples/variables-reference, renders every variant
# twice, checks SHA-256 byte-identity per variant, asserts every
# refusal path, archives variant MP4s as sprint evidence, and reports
# PROOF: PASS only when every check succeeds (exit 0). Any single
# failing check prints FAIL and the gate exits 1 (no false pass).
#
# Usage: n09-variables-proof.sh [--workdir DIR] [--bin PATH]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; the reference is
#   staged to DIR first. Evidence is copied to
#   evidences/proof-n09-T007/ on every run.
#
# Tamper check: the script also stages a tampered copy (unknown
# variable, no fallback) and requires a non-zero refusal, so editing
# the reference cannot silently stay green.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,24p' "$0"; exit 0 ;;
    *) echo "n09-variables-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/variables-reference"
EVID_DIR="$FRAMESMITH_DIR/../evidences/proof-n09-T007"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n09-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

# sha256 of one file (portable across macOS/Linux coreutils).
sha() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

echo "================ n09 variables proof gate (T007) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "=================================================================="
echo ""

for NEED in cargo ffmpeg; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl rows-pass.json rows-mixed.json; do
  if [ ! -f "$EXAMPLE_DIR/$F" ]; then
    fail "prereq-example-$F" "missing $EXAMPLE_DIR/$F"
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
  if [ ! -x "$FSMITH" ]; then
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
fi
echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
echo "cargo: $(cargo --version 2>/dev/null || echo MISSING)"
echo "rustc: $(rustc --version 2>/dev/null || echo MISSING)"
echo ""

# --- stage the reference ------------------------------------------------
echo "--- stage reference ---"
rm -rf "$WORKDIR/proj"
mkdir -p "$WORKDIR/proj/fonts"
cp "$EXAMPLE_DIR/composition.kdl" "$WORKDIR/proj/composition.kdl"
cp "$EXAMPLE_DIR/rows-pass.json" "$WORKDIR/rows-pass.json"
cp "$EXAMPLE_DIR/rows-mixed.json" "$WORKDIR/rows-mixed.json"

# Stills: deterministic, generated with the proving toolchain.
ffmpeg -hide_banner -loglevel error -y -f lavfi \
  -i testsrc=size=96x72:rate=1:duration=1 -frames:v 1 \
  "$WORKDIR/proj/hero.png" || fail "stage-hero" "ffmpeg could not write hero.png"
ffmpeg -hide_banner -loglevel error -y -f lavfi \
  -i "color=c=blue:size=96x72:rate=1:duration=1" -frames:v 1 \
  "$WORKDIR/proj/hero-alt.png" || fail "stage-hero-alt" "ffmpeg could not write hero-alt.png"

# Project font: Arial on macOS, DejaVu Sans on Linux (family name comes
# from the font file, so the staged declaration default follows it).
FONT_FAMILY=""
for CAND in "Arial|/System/Library/Fonts/Supplemental/Arial.ttf" \
    "DejaVu Sans|/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"; do
  FAM="${CAND%%|*}"
  PTH="${CAND#*|}"
  if [ -z "$FONT_FAMILY" ] && [ -f "$PTH" ]; then
    FONT_FAMILY="$FAM"
    cp "$PTH" "$WORKDIR/proj/fonts/Body.ttf"
  fi
done
if [ -n "${FRAMESMITH_PROOF_TTF:-}" ] && [ -f "${FRAMESMITH_PROOF_TTF:-}" ]; then
  cp "$FRAMESMITH_PROOF_TTF" "$WORKDIR/proj/fonts/Body.ttf"
  FONT_FAMILY="${FRAMESMITH_PROOF_FONT_FAMILY:-Custom}"
fi
if [ -z "$FONT_FAMILY" ]; then
  fail "stage-font" "no system TTF found (Arial or DejaVu Sans); set FRAMESMITH_PROOF_TTF=/path/to/font.ttf"
else
  if [ "$FONT_FAMILY" != "Arial" ]; then
    sed "s/default=\"Arial\"/default=\"$FONT_FAMILY\"/" \
      "$WORKDIR/proj/composition.kdl" > "$WORKDIR/proj/composition.kdl.tmp"
    mv "$WORKDIR/proj/composition.kdl.tmp" "$WORKDIR/proj/composition.kdl"
  fi
  echo "font: $FONT_FAMILY"
fi
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
pass "stage-reference"
KDL_BEFORE="$(sha "$WORKDIR/proj/composition.kdl")"
echo ""

cd "$WORKDIR" || exit 1

OVERRIDES='{"title":"Sale","accent":"#0000ff","count":40,"show":true,"align":"left","hero":"hero-alt.png"}'

# --- default check + render ----------------------------------------------
"$FSMITH" check proj >check-default.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && grep -q "ok:" check-default.log && grep -q -- "--spare" check-default.log; then
  pass "check-default-green-with-unused-warning"
else
  fail "check-default-green-with-unused-warning" "exit=$CODE (want 0 with ok: and --spare warning)"
fi

"$FSMITH" check --variables "$OVERRIDES" proj >check-override.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && grep -q "ok:" check-override.log; then
  pass "check-override-green"
else
  fail "check-override-green" "exit=$CODE (want 0 with ok:)"
fi

"$FSMITH" render proj -o default.mp4 >render-default.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ -s default.mp4 ]; then
  H_DEFAULT="$(sha default.mp4)"
  pass "render-default"
else
  fail "render-default" "exit=$CODE (want 0 with default.mp4)"
fi

"$FSMITH" render proj -o default.mp4 >render-default-2.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ "$(sha default.mp4)" = "${H_DEFAULT:-missing}" ]; then
  pass "render-default-byte-identical"
else
  fail "render-default-byte-identical" "re-render hash differs"
fi

# --- overridden variant ----------------------------------------------------
"$FSMITH" render proj -o override.mp4 --variables "$OVERRIDES" >render-override.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ -s override.mp4 ]; then
  H_OVERRIDE="$(sha override.mp4)"
  pass "render-override"
else
  fail "render-override" "exit=$CODE (want 0 with override.mp4)"
fi

if [ -n "${H_OVERRIDE:-}" ] && [ -n "${H_DEFAULT:-}" ]; then
  if cmp -s default.mp4 override.mp4; then
    fail "override-resolves-visibly" "override.mp4 is byte-equal to default.mp4; overrides had no effect"
  else
    pass "override-resolves-visibly"
  fi
fi

"$FSMITH" render proj -o override.mp4 --variables "$OVERRIDES" >render-override-2.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ "$(sha override.mp4)" = "${H_OVERRIDE:-missing}" ]; then
  pass "render-override-byte-identical"
else
  fail "render-override-byte-identical" "re-render hash differs"
fi

# --- themes ----------------------------------------------------------------
"$FSMITH" render proj -o bold.mp4 --theme bold >render-bold.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ -s bold.mp4 ]; then
  pass "render-theme-bold"
else
  fail "render-theme-bold" "exit=$CODE (want 0 with bold.mp4)"
fi

if [ -f bold.mp4 ] && [ -f default.mp4 ]; then
  if cmp -s bold.mp4 default.mp4; then
    fail "theme-reskins" "bold.mp4 is byte-equal to default.mp4; theme had no effect"
  else
    pass "theme-reskins"
  fi
fi

# Override wins over the theme token (precedence holds).
"$FSMITH" render proj -o bold-override.mp4 --theme bold \
  --variables '{"accent":"#0000ff"}' >render-bold-override.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ -s bold-override.mp4 ]; then
  if cmp -s bold-override.mp4 bold.mp4; then
    fail "override-beats-theme" "themed override is byte-equal to theme-only; override had no effect"
  else
    pass "override-beats-theme"
  fi
else
  fail "override-beats-theme" "exit=$CODE (want 0 with bold-override.mp4)"
fi

# --- strict mode + invalid variants -----------------------------------------
"$FSMITH" check --strict-variables proj >check-strict.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q -- "--spare" check-strict.log; then
  pass "strict-refuses-unused-declaration"
else
  fail "strict-refuses-unused-declaration" "exit=$CODE (want non-zero naming --spare)"
fi

"$FSMITH" check --strict-variables --variables '{"ghost":"x"}' proj >check-strict-stale.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q -- "--ghost" check-strict-stale.log; then
  pass "strict-refuses-stale-key"
else
  fail "strict-refuses-stale-key" "exit=$CODE (want non-zero naming --ghost)"
fi

rm -f bad-accent.mp4
"$FSMITH" render proj -o bad-accent.mp4 --variables '{"accent":"notacolor"}' >render-bad-accent.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q -- "--accent" render-bad-accent.log && [ ! -f bad-accent.mp4 ]; then
  pass "invalid-override-refused-before-frame-one"
else
  fail "invalid-override-refused-before-frame-one" "exit=$CODE (want non-zero naming --accent, no MP4)"
fi

"$FSMITH" check --theme nosuch proj >check-unknown-theme.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q "nosuch" check-unknown-theme.log; then
  pass "unknown-theme-refused"
else
  fail "unknown-theme-refused" "exit=$CODE (want non-zero naming nosuch)"
fi

# Tampered copy (unknown variable, no fallback) must refuse too.
rm -rf proj-tamper
cp -r proj proj-tamper
sed 's/Now showing: var(--title)/Broken var(--nosuch)/' \
  proj/composition.kdl > proj-tamper/composition.kdl
"$FSMITH" check proj-tamper >check-tamper.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q -- "--nosuch" check-tamper.log; then
  pass "tampered-reference-refused"
else
  fail "tampered-reference-refused" "exit=$CODE (want non-zero naming --nosuch)"
fi
echo ""

# --- batch -------------------------------------------------------------------
rm -f proj/clip-*.mp4
"$FSMITH" render proj --batch rows-pass.json \
  --name-template "clip-{title}-{index}" >batch-pass.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && grep -q "3 passed, 0 failed, 0 skipped" batch-pass.log \
  && [ -s proj/clip-Alpha-0.mp4 ] && [ -s proj/clip-Beta-1.mp4 ] && [ -s proj/clip-Gamma-2.mp4 ]; then
  H_B0="$(sha proj/clip-Alpha-0.mp4)"
  H_B1="$(sha proj/clip-Beta-1.mp4)"
  H_B2="$(sha proj/clip-Gamma-2.mp4)"
  pass "batch-all-pass-with-per-row-summary"
else
  fail "batch-all-pass-with-per-row-summary" "exit=$CODE (want 0, 3 passed, three MP4s)"
fi

"$FSMITH" render proj --batch rows-pass.json \
  --name-template "clip-{title}-{index}" >batch-pass-2.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] \
  && [ "$(sha proj/clip-Alpha-0.mp4)" = "${H_B0:-missing}" ] \
  && [ "$(sha proj/clip-Beta-1.mp4)" = "${H_B1:-missing}" ] \
  && [ "$(sha proj/clip-Gamma-2.mp4)" = "${H_B2:-missing}" ]; then
  pass "batch-re-renders-byte-identical"
else
  fail "batch-re-renders-byte-identical" "exit=$CODE or a row hash differs"
fi

mkdir -p keep
for F in proj/clip-*.mp4; do [ -f "$F" ] && cp "$F" keep/; done
rm -f proj/clip-*.mp4
"$FSMITH" render proj --batch rows-mixed.json \
  --name-template "clip-{title}-{index}" >batch-mixed.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q "batch: row 1: failed" batch-mixed.log \
  && grep -q "2 passed, 1 failed, 0 skipped" batch-mixed.log \
  && [ -s proj/clip-Good-0.mp4 ] && [ -s proj/clip-Later-2.mp4 ] \
  && [ ! -f "proj/clip--1.mp4" ]; then
  pass "batch-mixed-keeps-good-mp4s"
else
  fail "batch-mixed-keeps-good-mp4s" "exit=$CODE (want non-zero, row 1 failed, good MP4s kept)"
fi

for F in proj/clip-*.mp4; do [ -f "$F" ] && cp "$F" keep/; done
rm -f proj/clip-*.mp4
"$FSMITH" render proj --batch rows-mixed.json \
  --name-template "clip-{title}-{index}" --fail-fast >batch-failfast.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q "batch: row 1: failed" batch-failfast.log \
  && grep -q "1 passed, 1 failed, 1 skipped" batch-failfast.log \
  && [ -s proj/clip-Good-0.mp4 ] && [ ! -f proj/clip-Later-2.mp4 ]; then
  pass "batch-fail-fast-stops-and-keeps-earlier"
else
  fail "batch-fail-fast-stops-and-keeps-earlier" "exit=$CODE (want non-zero, row 2 skipped, row 0 kept)"
fi

# The reference itself must be untouched by every render.
if [ "$(sha proj/composition.kdl)" = "$KDL_BEFORE" ]; then
  pass "reference-unchanged-on-disk"
else
  fail "reference-unchanged-on-disk" "composition.kdl changed during renders"
fi
echo ""

# --- evidence ------------------------------------------------------------------
mkdir -p "$EVID_DIR"
{
  echo "n09 variables proof gate (T007)"
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
  echo "font: $FONT_FAMILY"
  echo ""
  echo "sha256:"
  for F in default.mp4 override.mp4 bold.mp4 bold-override.mp4 keep/*.mp4; do
    if [ -f "$F" ]; then echo "  $(sha "$F")  $F"; fi
  done
} >"$WORKDIR/REPORT.md"
cp "$WORKDIR/REPORT.md" "$EVID_DIR/REPORT.md"
cp proj/composition.kdl rows-pass.json rows-mixed.json "$EVID_DIR/" 2>/dev/null
for F in default.mp4 override.mp4 bold.mp4 bold-override.mp4; do
  [ -f "$F" ] && cp "$F" "$EVID_DIR/"
done
for F in keep/*.mp4; do
  [ -f "$F" ] && cp "$F" "$EVID_DIR/"
done
# Keep one full transcript of the last gate-relevant runs.
cat batch-pass.log batch-mixed.log batch-failfast.log >"$WORKDIR/proof-tail.log" 2>/dev/null
cp "$WORKDIR/proof-tail.log" "$EVID_DIR/proof.log" 2>/dev/null
echo "evidence: $EVID_DIR"
echo ""

# --- gate ----------------------------------------------------------------------
echo "checks passed: $PASS_N"
if [ -z "$FAIL_LIST" ]; then
  echo "PROOF: PASS"
  exit 0
else
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
