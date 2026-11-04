#!/bin/sh
# n11-look-proof.sh — Sprint 11 T008 one-command gate.
#
# Proves the look reference project end to end: one composition
# carrying a composition grade (preset, adjusts, wheel, curve, HSL,
# LUT), a fully graded/effected/VFX-chained moving clip, a LUT baked
# from the composition chain and reused on a third clip, a transition
# into a second clip, and shuttered motion blur. Stages the reference
# from framesmith/examples/look-reference, renders twice with SHA-256
# byte-identity, re-bakes byte-identically, proves the baked LUT
# renders pixel-identical (worst LSB drift) to the tuned chain, proves
# the transition keeps the frame count, proves the blur smears motion
# while still pixels match, refuses every invalid look variant before
# frame one, and reports PROOF: PASS only when every check succeeds
# (exit 0). Any single failing check prints FAIL and the gate exits 1
# (no false pass).
#
# Usage: n11-look-proof.sh [--workdir DIR] [--bin PATH]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; the reference is
#   staged to DIR first. Evidence is copied to
#   evidences/proof-n11-T008/ on every run.
#
# Tamper coverage: fifteen tampered copies (bad preset/adjust/curve,
# unknown effect, bad effect amount, duplicated/unknown VFX stage,
# matte without src, matte with missing file, unknown transition
# kind, short handles, unknown transition id, bad shutter, duplicated
# motion-blur, missing LUT) must each refuse with a non-zero exit
# naming the expected code, plus one tampered render must exit
# non-zero with no MP4 — so editing the reference cannot stay green.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n11-look-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/look-reference"
EVID_DIR="$FRAMESMITH_DIR/../evidences/proof-n11-T008"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n11-proof-XXXXXX)"
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

echo "================ n11 look proof gate (T008) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "============================================================"
echo ""

for NEED in cargo ffmpeg python3; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl tune.cube baked.cube; do
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

# --- stage the reference -----------------------------------------------------
echo "--- stage reference ---"
rm -rf "$WORKDIR/proj"
mkdir -p "$WORKDIR/proj"
cp "$EXAMPLE_DIR/composition.kdl" "$WORKDIR/proj/composition.kdl"
cp "$EXAMPLE_DIR/tune.cube" "$WORKDIR/proj/tune.cube"
cp "$EXAMPLE_DIR/baked.cube" "$WORKDIR/proj/baked.cube"
KDL_BEFORE="$(sha "$WORKDIR/proj/composition.kdl")"
pass "stage-reference"
echo ""

cd "$WORKDIR" || exit 1

# --- check + render twice with byte-identity ----------------------------------
"$FSMITH" check proj >check.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && grep -q "ok:" check.log; then
  pass "check-green"
else
  fail "check-green" "exit=$CODE (want 0 with ok:)"
fi

"$FSMITH" render proj -o look1.mp4 >render1.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ -s look1.mp4 ]; then
  H1="$(sha look1.mp4)"
  pass "render-first"
else
  fail "render-first" "exit=$CODE (want 0 with look1.mp4)"
fi

"$FSMITH" render proj -o look2.mp4 >render2.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ -s look2.mp4 ] && [ "$(sha look2.mp4)" = "${H1:-missing}" ]; then
  pass "render-twice-byte-identical"
else
  fail "render-twice-byte-identical" "exit=$CODE or re-render hash differs"
fi

# --- bake determinism: re-bake must reproduce the shipped cube -----------------
"$FSMITH" grade-bake proj -o rebaked.cube >bake.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && [ -s rebaked.cube ] \
  && [ "$(wc -l <bake.log | tr -d ' ')" = "1" ] \
  && grep -q "baked" bake.log && grep -q "33" bake.log \
  && grep -q "preset=teal-orange" bake.log && grep -q "tune.cube" bake.log; then
  pass "grade-bake-one-line-report"
else
  fail "grade-bake-one-line-report" "exit=$CODE (want 0, one line naming baked/33/preset/tune.cube)"
fi

if [ -f rebaked.cube ] && cmp -s rebaked.cube proj/baked.cube; then
  pass "bake-reproduces-shipped-cube"
else
  fail "bake-reproduces-shipped-cube" "re-baked bytes differ from examples/look-reference/baked.cube"
fi

# --- baked LUT renders pixel-identical to the tuned chain ----------------------
# probe-tuned mirrors the reference composition grade chain; probe-baked
# imports only the baked file. Both carry the same plain box.
mkdir -p pt pb
cat > pt/composition.kdl <<'EOF'
composition name="main" root=true width=64 height=64 duration=1 background="#101010" version=1 {
  grade preset="teal-orange" contrast=0.2 saturation=0.3 {
    wheel shadows=0.15 midtones=0.1 highlights=-0.1
    curve red="0,0 0.5,0.6 1,1" master="0,0 1,1"
    hsl hue-min=330 hue-max=30 saturation=0.2 lightness=0.2
    lut src="tune.cube" amount=0.5
  }
  box x=8 y=8 width=48 height=48 fill="#c86432" start=0 duration=1
}
EOF
cat > pb/composition.kdl <<'EOF'
composition name="main" root=true width=64 height=64 duration=1 background="#101010" version=1 {
  grade {
    lut src="baked.cube" amount=1
  }
  box x=8 y=8 width=48 height=48 fill="#c86432" start=0 duration=1
}
EOF
cp proj/tune.cube pt/tune.cube
cp proj/baked.cube pb/baked.cube

if "$FSMITH" check pt >check-pt.log 2>&1 && "$FSMITH" check pb >check-pb.log 2>&1 \
  && "$FSMITH" render pt -o pt.mp4 >render-pt.log 2>&1 \
  && "$FSMITH" render pb -o pb.mp4 >render-pb.log 2>&1; then
  pass "probe-renders"
else
  fail "probe-renders" "tuned/baked probe check or render failed"
fi

cat > drift.py <<'EOF'
import subprocess, sys
def dec(mp4):
    p = subprocess.run(['ffmpeg','-y','-v','error','-i',mp4,
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    if p.returncode != 0:
        print("decode failed:", mp4); sys.exit(2)
    return p.stdout
a, b = dec('pt.mp4'), dec('pb.mp4')
assert len(a) == len(b) == 30*64*64*3, (len(a), len(b))
worst = max(abs(x-y) for x, y in zip(a, b))
print("worst LUT drift:", worst)
sys.exit(0 if worst <= 8 else 1)
EOF
if python3 drift.py >drift.log 2>&1; then
  pass "baked-lut-pixel-identical-to-tuned-chain"
else
  fail "baked-lut-pixel-identical-to-tuned-chain" "$(cat drift.log | head -2 | tr '\n' ' ') (want worst drift <= 8 LSB)"
fi
echo ""

# --- frame counts: transition borrows handles, timeline unchanged ----------------
grep -v '^  transition ' proj/composition.kdl > proj-notrans.kdl
rm -rf notrans && mkdir -p notrans
cp proj-notrans.kdl notrans/composition.kdl
cp proj/tune.cube proj/baked.cube notrans/
if "$FSMITH" check notrans >check-notrans.log 2>&1 \
  && "$FSMITH" render notrans -o notrans.mp4 >render-notrans.log 2>&1; then
  pass "untransitioned-variant-renders"
else
  fail "untransitioned-variant-renders" "notrans check or render failed"
fi

cat > frames.py <<'EOF'
import subprocess, sys
def dec(mp4):
    p = subprocess.run(['ffmpeg','-y','-v','error','-i',mp4,
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    if p.returncode != 0:
        print("decode failed:", mp4); sys.exit(2)
    return p.stdout
a, b = dec('look1.mp4'), dec('notrans.mp4')
fa, fb = len(a)//(64*64*3), len(b)//(64*64*3)
print("frames: with-transition=%d without-transition=%d" % (fa, fb))
sys.exit(0 if fa == fb == 90 else 1)
EOF
if [ -f notrans.mp4 ] && python3 frames.py >frames.log 2>&1; then
  pass "transition-keeps-frame-count"
else
  fail "transition-keeps-frame-count" "$(cat frames.log 2>/dev/null | tr '\n' ' ') (want 90 == 90)"
fi
echo ""

# --- the authored look is on the pixels ------------------------------------------
cat > look.py <<'EOF'
import subprocess, sys
def dec(mp4):
    p = subprocess.run(['ffmpeg','-y','-v','error','-i',mp4,
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    assert p.returncode == 0, mp4
    return p.stdout
W = H = 64; F = W*H*3
raw = dec('look1.mp4')
assert len(raw) == 90*F, len(raw)
def frame(n): return raw[n*F:(n+1)*F]
def px(f, x, y):
    o = (y*W+x)*3
    return (f[o], f[o+1], f[o+2])
def close(got, want, tol):
    return all(abs(g-w) <= tol for g, w in zip(got, want))
def mad(a, b):
    return sum(abs(x-y) for x, y in zip(a, b))/len(a)
ok = True
def check(name, cond, detail=""):
    global ok
    print(("PASS-ASSERT " if cond else "FAIL-ASSERT ") + name + " " + detail)
    if not cond: ok = False
f30 = frame(30)
box = px(f30, 50, 32)
check("graded-effected-box", box[0] > 150 and box[0] >= box[1] >= box[2],
      "f30 box=%s (want warm sepia r>=g>=b, r>150)" % (box,))
check("graded-effected-box-value", close(box, (230, 217, 183), 40),
      "f30 box=%s" % (box,))
xs = [f30[(y*W+x)*3] for y in range(24, 40) for x in range(44, 56)]
m = sum(xs)/len(xs)
var = sum((v-m)**2 for v in xs)/len(xs)
check("vfx-noise-variance", var > 300, "var=%.0f (want >300)" % var)
bg = px(f30, 32, 8)
check("composition-grade-background", close(bg, (28, 47, 27), 40),
      "f30 bg=%s" % (bg,))
f59, f67, f80 = frame(59), frame(67), frame(80)
check("bridge-moves-past-tail", mad(f67, f59) > 2.0,
      "mad=%.2f" % mad(f67, f59))
check("bridge-not-yet-head", mad(f67, f80) > 2.0,
      "mad=%.2f" % mad(f67, f80))
mid = px(f67, 50, 32)
check("bridge-blends", close(mid, (191, 138, 163), 45),
      "f67 mid=%s" % (mid,))
clip = px(f80, 52, 52)
blue = px(f80, 32, 32)
check("baked-lut-third-clip", close(clip, (179, 195, 176), 45),
      "f80 clip=%s" % (clip,))
check("third-clip-differs-from-blue", abs(clip[0]-blue[0]) > 20,
      "clip=%s blue=%s" % (clip, blue))
print("ALL-ASSERTS-PASS" if ok else "ASSERTS-FAILED")
sys.exit(0 if ok else 1)
EOF
if python3 look.py >look.log 2>&1 && grep -q "ALL-ASSERTS-PASS" look.log; then
  pass "authored-look-on-pixels"
else
  fail "authored-look-on-pixels" "$(grep "FAIL-ASSERT" look.log 2>/dev/null | head -3 | tr '\n' ';') (see look.log)"
fi
echo ""

# --- motion blur smears motion, still pixels match --------------------------------
grep -v '^  motion-blur ' proj/composition.kdl > proj-noblur.kdl
rm -rf noblur && mkdir -p noblur
cp proj-noblur.kdl noblur/composition.kdl
cp proj/tune.cube proj/baked.cube noblur/
if "$FSMITH" check noblur >check-noblur.log 2>&1 \
  && "$FSMITH" render noblur -o sharp.mp4 >render-sharp.log 2>&1; then
  pass "blur-off-variant-renders"
else
  fail "blur-off-variant-renders" "noblur check or render failed"
fi

cat > blur.py <<'EOF'
import subprocess, sys
def dec(mp4):
    p = subprocess.run(['ffmpeg','-y','-v','error','-i',mp4,
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    assert p.returncode == 0, mp4
    return p.stdout
W = H = 64; F = W*H*3
blurred, sharp = dec('look1.mp4'), dec('sharp.mp4')
assert len(blurred) == len(sharp) == 90*F, (len(blurred), len(sharp))
def frame(raw, n): return raw[n*F:(n+1)*F]
def mad(a, b):
    return sum(abs(x-y) for x, y in zip(a, b))/len(a)
ok = True
def check(name, cond, detail=""):
    global ok
    print(("PASS-ASSERT " if cond else "FAIL-ASSERT ") + name + " " + detail)
    if not cond: ok = False
fb7, fs7 = frame(blurred, 7), frame(sharp, 7)
check("shutter-smears-mid-motion", mad(fb7, fs7) > 1.5,
      "f7 mad=%.3f (want >1.5)" % mad(fb7, fs7))
o = (30*W+2)*3
still_same = frame(blurred, 30)[o:o+3] == frame(sharp, 30)[o:o+3]
check("still-corner-matches-blur-off", still_same,
      "bg corner blurred=%s sharp=%s" %
      (tuple(frame(blurred, 30)[o:o+3]), tuple(frame(sharp, 30)[o:o+3])))
print("ALL-ASSERTS-PASS" if ok else "ASSERTS-FAILED")
sys.exit(0 if ok else 1)
EOF
if [ -f sharp.mp4 ] && python3 blur.py >blur.log 2>&1 && grep -q "ALL-ASSERTS-PASS" blur.log; then
  pass "motion-blur-smears-and-keeps-still"
else
  fail "motion-blur-smears-and-keeps-still" "$(grep "FAIL-ASSERT" blur.log 2>/dev/null | head -3 | tr '\n' ';') (see blur.log)"
fi
echo ""

# --- every invalid look variant refuses before frame one ---------------------------
tamper() {
  # $1=name $2=sed-expr $3=want-code [$4=want-text]
  rm -rf tamper-case && mkdir -p tamper-case
  cp proj/composition.kdl proj/tune.cube proj/baked.cube tamper-case/
  sed "$2" tamper-case/composition.kdl > tamper-case/composition.kdl.tmp
  mv tamper-case/composition.kdl.tmp tamper-case/composition.kdl
  OUT="$("$FSMITH" check tamper-case 2>&1)"
  CODE=$?
  if [ $CODE -ne 0 ] && echo "$OUT" | grep -q "$3" \
    && { [ -z "${4:-}" ] || echo "$OUT" | grep -q "$4"; }; then
    pass "refuses-$1"
  else
    fail "refuses-$1" "exit=$CODE (want non-zero naming $3${4:+ + $4})"
  fi
}
tamper bad-preset 's/preset="teal-orange"/preset="vintage"/' 'E_BAD_GRADE' 'teal-orange'
tamper bad-adjust 's/contrast=0.2/contrast=9/' 'E_BAD_GRADE' 'contrast'
tamper bad-curve 's/curve red="0,0 0.5,0.6 1,1"/curve red="0,0 0.5"/' 'E_BAD_GRADE' 'curve'
tamper bad-effect 's/effect preset="grayscale"/effect preset="vintage"/' 'E_BAD_EFFECT' 'grayscale'
tamper bad-effect-amount 's/effect preset="sepia" amount=0.8/effect preset="sepia" amount=2/' 'E_BAD_EFFECT' '0 to 1'
tamper dup-vfx-stage 's/noise amount=0.6 seed=7/noise amount=0.6 seed=7\
      noise seed=8/' 'E_BAD_VFX' 'at most once'
tamper unknown-vfx-stage 's/warp grid=8 strength=4/glow amount=1/' 'E_BAD_VFX' 'warp'
tamper matte-without-src 's/displacement scale=4/luma-matte amount=0.5/' 'E_BAD_VFX' 'src='
tamper matte-missing-file 's/displacement scale=4/luma-matte src="gone.png"/' 'E_MISSING_FILE' 'gone.png'
tamper unknown-transition-kind 's/kind="fade"/kind="swirl"/' 'E_BAD_TRANSITION' 'fade'
tamper short-handles 's/duration=0.5 kind/duration=1.5 kind/' 'E_BAD_TRANSITION'
tamper unknown-transition-id 's/from="a"/from="ghost"/' 'E_UNKNOWN_REF' 'ghost'
tamper bad-shutter 's/shutter=360/shutter=400/' 'E_BAD_BLUR' '0 to 360'
tamper dup-motion-blur 's/  motion-blur.*/  motion-blur shutter=360 samples=8 tier="high"\
  motion-blur shutter=90 samples=4 tier="low"/' 'E_BAD_BLUR' 'at most one'
tamper missing-lut 's|src="tune.cube"|src="gone.cube"|' 'E_MISSING_FILE' 'gone.cube'

# A tampered render must also fail without writing an MP4.
rm -rf tamper-render tampered.mp4 && mkdir -p tamper-render
cp proj/tune.cube proj/baked.cube tamper-render/
sed 's/preset="teal-orange"/preset="vintage"/' proj/composition.kdl > tamper-render/composition.kdl
"$FSMITH" render tamper-render -o tampered.mp4 >render-tampered.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && grep -q "E_BAD_GRADE" render-tampered.log && [ ! -f tampered.mp4 ]; then
  pass "tampered-render-refused-without-mp4"
else
  fail "tampered-render-refused-without-mp4" "exit=$CODE (want non-zero naming E_BAD_GRADE, no MP4)"
fi
echo ""

# --- the reference itself must be untouched by every render -------------------------
if [ "$(sha proj/composition.kdl)" = "$KDL_BEFORE" ]; then
  pass "reference-unchanged-on-disk"
else
  fail "reference-unchanged-on-disk" "composition.kdl changed during renders"
fi
echo ""

# --- evidence ------------------------------------------------------------------------
mkdir -p "$EVID_DIR"
{
  echo "n11 look proof gate (T008)"
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
  echo "cargo: $(cargo --version 2>/dev/null || echo MISSING)"
  echo "rustc: $(rustc --version 2>/dev/null || echo MISSING)"
  echo ""
  echo "sha256:"
  for F in look1.mp4 pt.mp4 pb.mp4 notrans.mp4 sharp.mp4; do
    if [ -f "$F" ]; then echo "  $(sha "$F")  $F"; fi
  done
  if [ -f proj/baked.cube ]; then echo "  $(sha proj/baked.cube)  baked.cube"; fi
  if [ -f rebaked.cube ]; then echo "  $(sha rebaked.cube)  rebaked.cube"; fi
} >"$WORKDIR/REPORT.md"
cp "$WORKDIR/REPORT.md" "$EVID_DIR/REPORT.md"
cp proj/composition.kdl proj/tune.cube proj/baked.cube "$EVID_DIR/"
[ -f look1.mp4 ] && cp look1.mp4 "$EVID_DIR/look-reference.mp4"
cat check.log render1.log render2.log bake.log drift.log frames.log look.log blur.log >"$WORKDIR/proof.log" 2>/dev/null
cp "$WORKDIR/proof.log" "$EVID_DIR/proof.log" 2>/dev/null
echo "evidence: $EVID_DIR"
echo ""

# --- gate ----------------------------------------------------------------------------
echo "checks passed: $PASS_N"
if [ -z "$FAIL_LIST" ]; then
  echo "PROOF: PASS"
  exit 0
else
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
