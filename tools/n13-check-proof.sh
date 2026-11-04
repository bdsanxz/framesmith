#!/bin/sh
# n13-check-proof.sh — Sprint 13 T008 one-command gate.
#
# Proves the check reference project end to end: one composition
# carrying a seeded overlap, an off-canvas element, a caption touch,
# late/escaping/static elements with assertions, a frozen span,
# low-contrast text, a silent mix, two discarded clips, and an unused
# variable. The gate stages the reference from
# framesmith/examples/check-reference, checks green twice with
# identical finding sets, proves lint parity, snapshots three times
# and requires decoded-pixel identity with the same full-render
# (pngseq) frames, lists keyframes and requires frame 5 at x=70,
# compares the reference against an identical copy (SSIM 1.0, pass),
# splits graded and ungraded halves (halves must differ), reports
# doctor and benchmark facts, gates check and render under --strict
# and --strict-all (errors naming the elevated rules, no output
# written), ships the default render twice with byte identity and
# warnings intact, refuses tampered copies, and reports PROOF: PASS
# only when every check succeeds (exit 0). Any single failing check
# prints FAIL and the gate exits 1 (no false pass).
#
# Usage: n13-check-proof.sh [--workdir DIR] [--bin PATH]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; the reference is
#   staged to DIR first. Evidence is copied to
#   evidences/proof-n13-T008/ on every run.
#
# Tamper coverage: one corrupted copy (bad background) must refuse
# check and render with non-zero exits and no MP4; one silenced copy
# (unused variable removed) must report 11 findings instead of 12,
# proving the finding-count gate is sensitive — so editing the
# reference cannot stay green.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n13-check-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/check-reference"
EVID_DIR="$FRAMESMITH_DIR/../evidences/proof-n13-T008"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n13-proof-XXXXXX)"
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

# Count of `lint[` finding lines in a report file.
finding_count() {
  grep -c '^lint\[' "$1" 2>/dev/null || echo 0
}

echo "================ n13 check proof gate (T008) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "=============================================================="
echo ""

for NEED in cargo ffmpeg python3; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl tone.wav; do
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

KDL_BEFORE="$(sha "$EXAMPLE_DIR/composition.kdl")"

# --- one full gate pass -------------------------------------------------------
# $1 = pass number (1 or 2). Every check runs in pass dir p$1 so the two
# passes never share artifacts; cross-pass equality is asserted after.
run_pass() {
  N="$1"
  P="$WORKDIR/p$N"
  mkdir -p "$P"
  cd "$P" || exit 1

  rm -rf proj projcopy
  mkdir -p proj projcopy
  cp "$EXAMPLE_DIR/composition.kdl" proj/composition.kdl
  cp "$EXAMPLE_DIR/tone.wav" proj/tone.wav
  cp proj/composition.kdl proj/tone.wav projcopy/
  pass "p$N-stage-reference"

  # --- check green with every seeded finding ---------------------------------
  "$FSMITH" check proj >check.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] && grep -q "ok: proj is valid" check.log \
    && grep -q "lint: 12 finding(s) and 0 note(s) across 13 rules" check.log; then
    pass "p$N-check-green-12-findings-0-notes"
  else
    fail "p$N-check-green-12-findings-0-notes" "exit=$CODE (want 0 with ok: and 12 findings, 0 notes)"
  fi

  for RULE in overlap off-canvas caption-zone appear-by stay-inside keep-moving frozen-timeline contrast silent-audio discarded-audio unused-variable; do
    if grep -q "lint\\[$RULE\\]" check.log && grep "lint\\[$RULE\\]" check.log | grep -q "(fix:"; then
      pass "p$N-finding-$RULE-with-fix"
    else
      fail "p$N-finding-$RULE-with-fix" "rule $RULE missing or without fix in check.log"
    fi
  done
  if [ "$(grep -c 'lint\[discarded-audio\]' check.log)" -eq 2 ]; then
    pass "p$N-two-discarded-audio-findings"
  else
    fail "p$N-two-discarded-audio-findings" "want exactly 2 discarded-audio findings"
  fi

  # --- lint parity ------------------------------------------------------------
  "$FSMITH" lint proj >lint.log 2>&1
  CODE=$?
  grep '^lint\[' check.log | sort >check-lines.txt
  grep '^lint\[' lint.log | sort >lint-lines.txt
  if [ $CODE -eq 0 ] && cmp -s check-lines.txt lint-lines.txt; then
    pass "p$N-lint-parity-with-check"
  else
    fail "p$N-lint-parity-with-check" "exit=$CODE or lint lines differ from check lines"
  fi

  # --- snapshots pixel-identical to full-render frames -------------------------
  "$FSMITH" snapshot --at 0,3,5.9 --out snaps proj >snapshot.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] \
    && [ -f snaps/snap-0s.png ] && [ -f snaps/snap-3s.png ] && [ -f snaps/snap-5.9s.png ] \
    && grep -q "snapshot: wrote snap-0s.png (frame 0 at 0s)" snapshot.log \
    && grep -q "snapshot: wrote snap-3s.png (frame 30 at 3s)" snapshot.log \
    && grep -q "snapshot: wrote snap-5.9s.png (frame 59 at 5.9s)" snapshot.log; then
    pass "p$N-snapshot-three-pngs-named-for-frames"
  else
    fail "p$N-snapshot-three-pngs-named-for-frames" "exit=$CODE (want snap-0s/3s/5.9s at frames 0/30/59)"
  fi
  if ls snaps/*.mp4 >/dev/null 2>&1; then
    fail "p$N-snapshot-writes-no-mp4" "snapshot wrote an MP4"
  else
    pass "p$N-snapshot-writes-no-mp4"
  fi

  "$FSMITH" render proj --format pngseq -o seq >pngseq.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] && [ -f seq/frame-0000.png ] && [ -f seq/frame-0030.png ] && [ -f seq/frame-0059.png ]; then
    pass "p$N-full-pngseq-render"
  else
    fail "p$N-full-pngseq-render" "exit=$CODE (want seq/frame-0000/0030/0059.png)"
  fi

  cat >pix.py <<'EOF'
import subprocess, sys
def dec(p):
    r = subprocess.run(['ffmpeg','-y','-v','error','-i',p,
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    if r.returncode != 0:
        print("decode failed:", p); sys.exit(2)
    return r.stdout
pairs = [('snaps/snap-0s.png','seq/frame-0000.png'),
         ('snaps/snap-3s.png','seq/frame-0030.png'),
         ('snaps/snap-5.9s.png','seq/frame-0059.png')]
for a, b in pairs:
    da, db = dec(a), dec(b)
    assert len(da) == 640*480*3, (a, len(da))
    if da != db:
        print("MISMATCH:", a, b); sys.exit(1)
    print("identical:", a, b)
f0, f5 = dec('seq/frame-0000.png'), dec('seq/frame-0005.png')
if f0 == f5:
    print("STILL: frames 0 and 5 identical, sensitivity check void"); sys.exit(1)
print("sensitivity: frame 0 differs from frame 5 (motion present)")
EOF
  if python3 pix.py >pix.log 2>&1 && grep -q "sensitivity:" pix.log; then
    pass "p$N-snapshots-pixel-identical-to-render"
  else
    fail "p$N-snapshots-pixel-identical-to-render" "$(tr '\n' ';' <pix.log 2>/dev/null | cut -c1-200)"
  fi

  # --- keyframes ---------------------------------------------------------------
  "$FSMITH" keyframes --clip slide --property x proj >keyframes.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] \
    && grep -q "frame 5 at 0.5s: slide x=70" keyframes.log \
    && [ "$(grep -c '^frame ' keyframes.log)" -eq 60 ]; then
    pass "p$N-keyframes-slide-x-frame5-70"
  else
    fail "p$N-keyframes-slide-x-frame5-70" "exit=$CODE (want 60 rows with frame 5 x=70)"
  fi

  # --- compare identical copies -------------------------------------------------
  "$FSMITH" compare -o cmp.png proj projcopy >compare.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] && [ -f cmp.png ] \
    && grep -q "ssim 1 against bar 0.96: pass" compare.log \
    && grep -q "left proj, right projcopy" compare.log; then
    pass "p$N-compare-identical-ssim-1-pass"
  else
    fail "p$N-compare-identical-ssim-1-pass" "exit=$CODE (want ssim 1 pass with cmp.png)"
  fi

  cat >halves.py <<'EOF'
import subprocess, sys
def dec(p, w, h):
    r = subprocess.run(['ffmpeg','-y','-v','error','-i',p,
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    if r.returncode != 0:
        print("decode failed:", p); sys.exit(2)
    assert len(r.stdout) == w*h*3, (p, len(r.stdout))
    return r.stdout
raw = dec('cmp.png', 1280, 480)
left = bytearray(); right = bytearray()
for y in range(480):
    left += raw[y*3840:y*3840+1920]
    right += raw[y*3840+1920:y*3840+3840]
if left == right:
    print("halves byte-equal")
else:
    print("halves DIFFER"); sys.exit(1)
EOF
  if python3 halves.py >halves.log 2>&1 && grep -q "halves byte-equal" halves.log; then
    pass "p$N-compare-halves-byte-equal"
  else
    fail "p$N-compare-halves-byte-equal" "$(tr '\n' ';' <halves.log 2>/dev/null | cut -c1-160)"
  fi

  # --- grade-compare splits graded from ungraded ---------------------------------
  "$FSMITH" grade-compare -o gc.png proj >grade-compare.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] && [ -f gc.png ] \
    && grep -q "left ungraded" grade-compare.log \
    && grep -q "right graded" grade-compare.log; then
    pass "p$N-grade-compare-splits-halves"
  else
    fail "p$N-grade-compare-splits-halves" "exit=$CODE (want left ungraded / right graded with gc.png)"
  fi

  cat >halves2.py <<'EOF'
import subprocess, sys
def dec(p, w, h):
    r = subprocess.run(['ffmpeg','-y','-v','error','-i',p,
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    if r.returncode != 0:
        print("decode failed:", p); sys.exit(2)
    assert len(r.stdout) == w*h*3, (p, len(r.stdout))
    return r.stdout
raw = dec('gc.png', 1280, 480)
left = bytearray(); right = bytearray()
for y in range(480):
    left += raw[y*3840:y*3840+1920]
    right += raw[y*3840+1920:y*3840+3840]
if left != right:
    print("halves differ (grade is on the pixels)")
else:
    print("halves EQUAL, grade had no effect"); sys.exit(1)
EOF
  if python3 halves2.py >halves2.log 2>&1 && grep -q "halves differ" halves2.log; then
    pass "p$N-grade-compare-halves-differ"
  else
    fail "p$N-grade-compare-halves-differ" "$(tr '\n' ';' <halves2.log 2>/dev/null | cut -c1-160)"
  fi

  # --- doctor and benchmark ------------------------------------------------------
  "$FSMITH" doctor proj >doctor.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] \
    && grep -q "doctor: framesmith " doctor.log \
    && grep -q "doctor: ffmpeg: " doctor.log \
    && grep -q "doctor: gpu renderer " doctor.log \
    && grep -q "doctor: text shaping " doctor.log \
    && grep -q "doctor: host " doctor.log \
    && grep -q "proj is valid" doctor.log \
    && grep -q 'composition "review"' doctor.log; then
    pass "p$N-doctor-env-and-project"
  else
    fail "p$N-doctor-env-and-project" "exit=$CODE (want env facts plus valid review composition)"
  fi

  "$FSMITH" benchmark >benchmark.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] \
    && grep -q "benchmark: framesmith " benchmark.log \
    && grep -q 'benchmark: reference "still"' benchmark.log \
    && grep -q 'benchmark: reference "slide"' benchmark.log \
    && grep -q "(deterministic yes)" benchmark.log \
    && grep -q "machine-specific" benchmark.log \
    && grep -q "date 20" benchmark.log; then
    pass "p$N-benchmark-versions-date-deterministic"
  else
    fail "p$N-benchmark-versions-date-deterministic" "exit=$CODE (want versions, date, deterministic refs)"
  fi

  # --- strict gates ---------------------------------------------------------------
  "$FSMITH" check --strict proj >check-strict.log 2>&1
  CODE=$?
  if [ $CODE -eq 1 ] \
    && grep -q "5 elevated to errors by --strict" check-strict.log \
    && grep "lint\[off-canvas\]" check-strict.log | grep -q "error" \
    && grep "lint\[contrast\]" check-strict.log | grep -q "error" \
    && grep "lint\[unused-variable\]" check-strict.log | grep -q "error" \
    && grep "lint\[discarded-audio\]" check-strict.log | grep -q "error" \
    && grep "lint\[silent-audio\]" check-strict.log | grep -q "warning"; then
    pass "p$N-strict-check-elevates-named-only"
  else
    fail "p$N-strict-check-elevates-named-only" "exit=$CODE (want 1 with named errors, silent-audio still warning)"
  fi

  "$FSMITH" check --strict-all proj >check-strict-all.log 2>&1
  CODE=$?
  if [ $CODE -eq 1 ] && grep -q "lint: 12 finding(s) and 0 note(s)" check-strict-all.log; then
    pass "p$N-strict-all-check-gates"
  else
    fail "p$N-strict-all-check-gates" "exit=$CODE (want 1 with all 12 findings)"
  fi

  "$FSMITH" render --strict proj -o gated.mp4 >render-strict.log 2>&1
  CODE=$?
  if [ $CODE -eq 1 ] \
    && grep -q "render refused before the first frame" render-strict.log \
    && grep -q "Nothing was rendered" render-strict.log \
    && grep -q "off-canvas" render-strict.log \
    && grep -q "contrast" render-strict.log \
    && grep -q "unused-variable" render-strict.log \
    && grep -q "discarded-audio" render-strict.log \
    && [ ! -f gated.mp4 ]; then
    pass "p$N-strict-render-stops-naming-rules"
  else
    fail "p$N-strict-render-stops-naming-rules" "exit=$CODE (want 1 naming elevated rules, no MP4)"
  fi

  # --- default render ships with warnings intact ------------------------------------
  "$FSMITH" render proj -o ship.mp4 >render.log 2>&1
  CODE=$?
  if [ $CODE -eq 0 ] && [ -s ship.mp4 ]; then
    pass "p$N-default-render-ships"
  else
    fail "p$N-default-render-ships" "exit=$CODE (want 0 with ship.mp4)"
  fi

  # --- no inspection writes into the project ----------------------------------------
  if find proj projcopy -name '*.png' -o -name '*.mp4' | grep -q .; then
    fail "p$N-inspections-leave-projects-clean" "stray media inside proj or projcopy"
  else
    pass "p$N-inspections-leave-projects-clean"
  fi

  echo "$N done"
}

run_pass 1
echo ""
run_pass 2
echo ""

cd "$WORKDIR" || exit 1

# --- cross-pass determinism: every check twice, identical results -------------------
if [ "$(finding_count p1/check.log)" -eq 12 ] \
  && cmp -s p1/check-lines.txt p2/check-lines.txt; then
  pass "checks-twice-identical-findings"
else
  fail "checks-twice-identical-findings" "finding sets differ between passes"
fi

if [ "$(sha p1/snaps/snap-3s.png)" = "$(sha p2/snaps/snap-3s.png)" ] \
  && [ "$(sha p1/snaps/snap-0s.png)" = "$(sha p2/snaps/snap-0s.png)" ] \
  && [ "$(sha p1/snaps/snap-5.9s.png)" = "$(sha p2/snaps/snap-5.9s.png)" ]; then
  pass "snapshots-twice-identical"
else
  fail "snapshots-twice-identical" "snapshot bytes differ between passes"
fi

if [ "$(sha p1/ship.mp4)" = "$(sha p2/ship.mp4)" ]; then
  pass "renders-twice-byte-identical"
else
  fail "renders-twice-byte-identical" "default renders differ between passes"
fi
echo ""

# --- tamper coverage ------------------------------------------------------------------
# A corrupted copy must refuse check and render with no MP4.
rm -rf tamper-case && mkdir -p tamper-case
cp p1/proj/composition.kdl p1/proj/tone.wav tamper-case/
sed 's/background="#112233"/background="nope"/' tamper-case/composition.kdl >tamper-case/composition.kdl.tmp
mv tamper-case/composition.kdl.tmp tamper-case/composition.kdl
"$FSMITH" check tamper-case >tamper-check.log 2>&1
CODE_CHECK=$?
"$FSMITH" render tamper-case -o tampered.mp4 >tamper-render.log 2>&1
CODE_RENDER=$?
if [ $CODE_CHECK -ne 0 ] && grep -q "error" tamper-check.log \
  && [ $CODE_RENDER -ne 0 ] && [ ! -f tampered.mp4 ]; then
  pass "tampered-copy-refused-without-mp4"
else
  fail "tampered-copy-refused-without-mp4" "check=$CODE_CHECK render=$CODE_RENDER (want both non-zero, no MP4)"
fi

# A silenced copy (unused variable removed) must report 11 findings, not
# 12 — proving the finding-count gate is sensitive to a fixed reference.
rm -rf silenced-case && mkdir -p silenced-case
cp p1/proj/composition.kdl p1/proj/tone.wav silenced-case/
grep -v 'variable id="stale"' silenced-case/composition.kdl >silenced-case/composition.kdl.tmp
mv silenced-case/composition.kdl.tmp silenced-case/composition.kdl
"$FSMITH" lint silenced-case >silenced-lint.log 2>&1
CODE=$?
SILENCED_COUNT="$(finding_count silenced-lint.log)"
if [ $CODE -eq 0 ] && [ "$SILENCED_COUNT" -eq 11 ] \
  && ! grep -q 'lint\[unused-variable\]' silenced-lint.log; then
  pass "silenced-copy-reports-11-not-12"
else
  fail "silenced-copy-reports-11-not-12" "exit=$CODE count=$SILENCED_COUNT (want 0 with 11 findings)"
fi
echo ""

# --- the reference itself must be untouched ----------------------------------------------
if [ "$(sha "$EXAMPLE_DIR/composition.kdl")" = "$KDL_BEFORE" ]; then
  pass "reference-unchanged-on-disk"
else
  fail "reference-unchanged-on-disk" "examples/check-reference/composition.kdl changed during the proof"
fi
echo ""

# --- evidence ------------------------------------------------------------------------------
mkdir -p "$EVID_DIR"
{
  echo "n13 check proof gate (T008)"
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
  echo "cargo: $(cargo --version 2>/dev/null || echo MISSING)"
  echo "rustc: $(rustc --version 2>/dev/null || echo MISSING)"
  echo ""
  echo "sha256:"
  echo "  $(sha p1/ship.mp4)  ship.mp4 (default render, pass 1)"
  echo "  $(sha p2/ship.mp4)  ship.mp4 (default render, pass 2)"
  echo "  $(sha p1/snaps/snap-3s.png)  snap-3s.png"
  echo "  $(sha p1/cmp.png)  cmp.png (identical compare)"
  echo "  $(sha p1/gc.png)  gc.png (grade-compare)"
  echo "  $(sha "$EXAMPLE_DIR/composition.kdl")  composition.kdl"
  echo "  $(sha "$EXAMPLE_DIR/tone.wav")  tone.wav"
} >"$WORKDIR/REPORT.md"
cp "$WORKDIR/REPORT.md" "$EVID_DIR/REPORT.md"
cp "$EXAMPLE_DIR/composition.kdl" "$EXAMPLE_DIR/tone.wav" "$EVID_DIR/"
cp p1/ship.mp4 "$EVID_DIR/check-reference.mp4"
cp p1/snaps/snap-3s.png "$EVID_DIR/snap-3s.png"
cat p1/check.log p1/snapshot.log p1/compare.log p1/grade-compare.log p1/doctor.log p1/benchmark.log p1/render.log p1/render-strict.log p2/check.log p2/render.log >"$WORKDIR/proof.log" 2>/dev/null
cp "$WORKDIR/proof.log" "$EVID_DIR/proof.log" 2>/dev/null
echo "evidence: $EVID_DIR"
echo ""

# --- gate -------------------------------------------------------------------------------------
echo "checks passed: $PASS_N"
if [ -z "$FAIL_LIST" ]; then
  echo "PROOF: PASS"
  exit 0
else
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
