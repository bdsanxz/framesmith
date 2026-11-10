#!/bin/sh
# n22-captions-proof.sh -- Sprint 22 T004 one-command gate.
#
# Proves the captions reference end to end on a staged fixture copy
# (the checkout itself is never written):
#   1. `check` passes on the reference and lists the captions source
#      ready, with a 1-ready tally; `lint` exits 0 with the same tally.
#   2. seeks are word-correct across cue boundaries: cue frames paint
#      words, the gap frame is byte-identical to the bare plate, and
#      the three cue frames all differ.
#   3. SRT, VTT, and JSON imports decode to identical times: the same
#      seeks on `src`-swapped copies are byte-identical.
#   4. styled and preset frames paint (the reference differs from a
#      no-preset copy mid-entrance and from an unstyled copy), and
#      reruns are byte-identical (snapshots and full renders `cmp`).
#   5. behind-subject cover pixels: with the half-opaque matte the
#      opaque half renders all white (caption covered) and the
#      transparent half renders exactly the plain caption paint.
#   6. gated failures: an overlap-tampered copy refuses `check` and
#      `render` naming talk.srt and writes no MP4; a behind-subject
#      order with no matte refuses naming the missing matte.
#   7. export round-trips are drift-free: the n22_captions_proof test
#      runs against the staged copy and proves SRT/VTT/JSON
#      export-then-reimport matches with zero drift.
#   8. the Studio inspector reports readiness and the active cue
#      (n22_captions_inspector tests).
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n22-captions-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory).
#   Evidence is copied to evidences/proof-n22-T004/ on every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n22-captions-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/captions-reference"
EVID_DIR="$WS_ROOT/evidences/proof-n22-T004"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n22-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n22 captions reference proof gate (T004) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "=========================================================================="
echo ""

for NEED in cargo python3 ffmpeg; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl talk.srt talk.vtt talk.json matte.png README.md; do
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
echo "cargo: $(cargo --version 2>/dev/null)"
echo "rustc: $(rustc --version 2>/dev/null)"
echo "python: $(python3 --version 2>&1)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo ""

# ---- stage the reference (a copy; the checkout is never rendered) ----
echo "--- stage: captions reference ---"
REF="$WORKDIR/ref"
cp -r "$EXAMPLE_DIR" "$REF" || exit 1
if [ -f "$REF/composition.kdl" ] && [ -f "$REF/talk.srt" ] \
  && [ -f "$REF/talk.vtt" ] && [ -f "$REF/talk.json" ] \
  && [ -f "$REF/matte.png" ]; then
  pass "staged-reference (composition + srt/vtt/json + matte)"
else
  fail "stage-reference" "staged copy misses a fixture file"
fi

# Minimal PNG decoder plus the behind-subject cover check (stdlib only).
cat >"$WORKDIR/capcheck.py" <<'PYEOF'
import struct, sys, zlib
def decode(path):
    d = open(path, 'rb').read()
    assert d[:8] == b'\x89PNG\r\n\x1a\n', 'not a png'
    pos, idat = 8, b''
    while pos < len(d):
        n = struct.unpack('>I', d[pos:pos + 4])[0]
        kind, data = d[pos + 4:pos + 8], d[pos + 8:pos + 8 + n]
        if kind == b'IHDR':
            w, h, bd, ct, _, _, _ = struct.unpack('>IIBBBBB', data)
        elif kind == b'IDAT':
            idat += data
        pos += 12 + n
    raw = zlib.decompress(idat)
    ch = {0: 1, 2: 3, 6: 4}[ct]
    stride = w * ch
    out = bytearray(w * h * ch)
    prev, p = bytearray(stride), 0
    for y in range(h):
        f = raw[p]; p += 1
        line = bytearray(raw[p:p + stride]); p += stride
        if f == 1:
            for i in range(ch, stride):
                line[i] = (line[i] + line[i - ch]) & 255
        elif f == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 255
        elif f == 3:
            for i in range(stride):
                a = line[i - ch] if i >= ch else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 255
        elif f == 4:
            for i in range(stride):
                a = line[i - ch] if i >= ch else 0
                b, c = prev[i], prev[i - ch] if i >= ch else 0
                q = a + b - c
                pa, pb, pc = abs(q - a), abs(q - b), abs(q - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, ch, bytes(out)
mode = sys.argv[1]
if mode == 'mattecheck':
    mw, mh, mch, mp = decode(sys.argv[2])
    pw, ph, pch, pp = decode(sys.argv[3])
    assert (mw, mh) == (pw, ph) == (160, 90), 'canvas is 160x90, got %s %s' % ((mw, mh), (pw, ph))
    assert mch == pch == 3, 'snapshots are RGB'
    left_bad = right_bad = 0
    for y in range(mh):
        for x in range(mw):
            o = (y * mw + x) * 3
            if x < mw // 2:
                if mp[o:o + 3] != b'\xff\xff\xff':
                    left_bad += 1
            elif mp[o:o + 3] != pp[o:o + 3]:
                right_bad += 1
    print('left-not-white=%d right-differs=%d' % (left_bad, right_bad))
    sys.exit(0 if (left_bad == 0 and right_bad == 0) else 1)
print('unknown mode ' + mode)
sys.exit(2)
PYEOF

# ---- gate 1: check passes, lint exits 0, same readiness tally ----
echo ""
echo "--- gate 1: check + lint accept the reference ---"
if "$FSMITH" check "$REF" >"$WORKDIR/check.log" 2>&1; then
  pass "check-exits-0-on-reference"
else
  fail "check-exit" "check exited non-zero; see check.log"
fi
if grep -q 'readiness\[captions\].*: ready (talk.srt)' "$WORKDIR/check.log" 2>/dev/null; then
  pass "check-readiness-captions-ready (talk.srt)"
else
  fail "check-readiness" "no captions-ready line naming talk.srt; see check.log"
fi
if grep -q "readiness: 1 source(s): 1 ready, 0 loading, 0 failed" "$WORKDIR/check.log" 2>/dev/null; then
  pass "check-tally-1-ready"
else
  fail "check-tally" "tally is not '1 ready, 0 loading, 0 failed'; see check.log"
fi
if "$FSMITH" lint "$REF" >"$WORKDIR/lint.log" 2>&1; then
  pass "lint-exits-0-on-reference"
else
  fail "lint-exit" "lint exited non-zero; see lint.log"
fi
if grep -q "readiness: 1 source(s): 1 ready, 0 loading, 0 failed" "$WORKDIR/lint.log" 2>/dev/null; then
  pass "lint-carries-same-readiness-tally"
else
  fail "lint-readiness" "lint lacks the 1-ready tally; see lint.log"
fi

# ---- gate 2: word-correct seeks across cue boundaries ----
echo ""
echo "--- gate 2: seeks are word-correct across cue boundaries ---"
if "$FSMITH" snapshot --at 0.6,1.0,1.7,2.5,3.5 --out "$WORKDIR/snaps" "$REF" >"$WORKDIR/snapshot.log" 2>&1; then
  pass "snapshot-renders-five-seeks"
else
  fail "snapshot-exit" "snapshot exited non-zero; see snapshot.log"
fi
for T in 0.6s 1s 1.7s 2.5s 3.5s; do
  if [ -f "$WORKDIR/snaps/snap-$T.png" ]; then :; else fail "snapshot-file-$T" "missing snap-$T.png; see snapshot.log"; fi
done
mkdir -p "$WORKDIR/bare"
printf 'composition name="main" width=160 height=90 fps=30 duration=4 background="#101010" version=1 {\n}\n' >"$WORKDIR/bare/composition.kdl"
if "$FSMITH" snapshot --at 1.7 --out "$WORKDIR/baresnaps" "$WORKDIR/bare" >"$WORKDIR/snapshot-bare.log" 2>&1 \
  && [ -f "$WORKDIR/baresnaps/snap-1.7s.png" ]; then
  if cmp -s "$WORKDIR/snaps/snap-1.7s.png" "$WORKDIR/baresnaps/snap-1.7s.png"; then
    pass "gap-frame-is-the-bare-plate (t=1.7s)"
  else
    fail "gap-frame" "t=1.7s differs from the bare plate; the gap paints words"
  fi
else
  fail "bare-snapshot" "bare-plate snapshot failed; see snapshot-bare.log"
fi
if cmp -s "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps/snap-1.7s.png" 2>/dev/null; then
  fail "cue1-paints" "t=1.0s equals the gap; cue 1 paints no words"
else
  pass "cue-1-paints-words (t=1.0s differs from the gap)"
fi
if cmp -s "$WORKDIR/snaps/snap-2.5s.png" "$WORKDIR/snaps/snap-3.5s.png" 2>/dev/null; then
  fail "cues-differ" "t=2.5s equals t=3.5s; cues paint the same words"
else
  pass "cues-paint-different-words (t=2.5s vs t=3.5s)"
fi
if cmp -s "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps/snap-2.5s.png" 2>/dev/null; then
  fail "cue-boundary" "t=1.0s equals t=2.5s; no boundary between cues 1 and 2"
else
  pass "cue-boundary-crosses (t=1.0s vs t=2.5s)"
fi

# ---- gate 3: identical times from SRT/VTT/JSON imports ----
echo ""
echo "--- gate 3: srt/vtt/json imports decode to identical times ---"
for FMT in vtt json; do
  VDIR="$WORKDIR/fmt-$FMT"
  cp -r "$REF" "$VDIR"
  if python3 - "$VDIR" "$FMT" <<'PYEOF' >>"$WORKDIR/snapshot.log" 2>&1
import sys
p = sys.argv[1] + '/composition.kdl'
s = open(p).read()
old = 'src="talk.srt"'
new = 'src="talk.%s"' % sys.argv[2]
assert old in s, 'expected %s in composition.kdl' % old
open(p, 'w').write(s.replace(old, new))
PYEOF
  then
    if "$FSMITH" snapshot --at 1.0,2.5,3.5 --out "$WORKDIR/snaps-$FMT" "$VDIR" >"$WORKDIR/snapshot-$FMT.log" 2>&1; then
      SAME=1
      for T in 1s 2.5s 3.5s; do
        if ! cmp -s "$WORKDIR/snaps/snap-$T.png" "$WORKDIR/snaps-$FMT/snap-$T.png" 2>/dev/null; then
          fail "import-$FMT-$T" "$FMT import differs from srt at t=$T"
          SAME=0
        fi
      done
      if [ "$SAME" -eq 1 ]; then
        pass "$FMT-import-identical-to-srt (t=1.0s, 2.5s, 3.5s)"
      fi
    else
      fail "import-$FMT-snapshot" "$FMT snapshot failed; see snapshot-$FMT.log"
    fi
  else
    fail "import-$FMT-stage" "could not point the copy at talk.$FMT"
  fi
done

# ---- gate 4: styled + preset frames with byte-identical reruns ----
echo ""
echo "--- gate 4: styled/preset frames paint, reruns are identical ---"
cp -r "$REF" "$WORKDIR/nopreset"
if python3 - "$WORKDIR/nopreset" <<'PYEOF' >>"$WORKDIR/snapshot.log" 2>&1
import sys
p = sys.argv[1] + '/composition.kdl'
s = open(p).read()
old = 'entrance="fade-up" highlight="pop" exit="fade-out"'
assert old in s, 'expected presets in composition.kdl'
open(p, 'w').write(s.replace(old, 'entrance="none" highlight="none" exit="none"'))
PYEOF
then
  if "$FSMITH" snapshot --at 0.6 --out "$WORKDIR/snaps-nopreset" "$WORKDIR/nopreset" >"$WORKDIR/snapshot-nopreset.log" 2>&1 \
    && [ -f "$WORKDIR/snaps-nopreset/snap-0.6s.png" ]; then
    if cmp -s "$WORKDIR/snaps/snap-0.6s.png" "$WORKDIR/snaps-nopreset/snap-0.6s.png" 2>/dev/null; then
      fail "preset-effect" "presets change nothing at t=0.6s mid-entrance"
    else
      pass "presets-paint (reference differs with no presets at t=0.6s)"
    fi
  else
    fail "preset-snapshot" "no-preset snapshot failed; see snapshot-nopreset.log"
  fi
else
  fail "preset-stage" "could not strip presets from the copy"
fi
cp -r "$REF" "$WORKDIR/plain"
if python3 - "$WORKDIR/plain" <<'PYEOF' >>"$WORKDIR/snapshot.log" 2>&1
import re, sys
p = sys.argv[1] + '/composition.kdl'
s = open(p).read()
old = ' font-size=16 color="#ffd60a" outline-width=2 outline-color="#000000" background="#141414" max-lines=2 entrance="fade-up" highlight="pop" exit="fade-out"'
assert old in s, 'expected the styled clip line'
s = s.replace(old, ' entrance="none" highlight="none" exit="none"')
s = re.sub(r'\n\s*override cue=0 word=1[^\n]*', '', s)
open(p, 'w').write(s)
PYEOF
then
  if "$FSMITH" snapshot --at 1.0 --out "$WORKDIR/snaps-plain" "$WORKDIR/plain" >"$WORKDIR/snapshot-plain.log" 2>&1 \
    && [ -f "$WORKDIR/snaps-plain/snap-1s.png" ]; then
    if cmp -s "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps-plain/snap-1s.png" 2>/dev/null; then
      fail "style-effect" "style and override change nothing at t=1.0s"
    else
      pass "style-and-override-paint (reference differs unstyled at t=1.0s)"
    fi
  else
    fail "plain-snapshot" "unstyled snapshot failed; see snapshot-plain.log"
  fi
else
  fail "plain-stage" "could not strip style from the copy"
fi
if "$FSMITH" snapshot --at 0.6,1.0,1.7,2.5,3.5 --out "$WORKDIR/snaps2" "$REF" >"$WORKDIR/snapshot2.log" 2>&1; then
  SNAPS_SAME=1
  for T in 0.6s 1s 1.7s 2.5s 3.5s; do
    if ! cmp -s "$WORKDIR/snaps/snap-$T.png" "$WORKDIR/snaps2/snap-$T.png" 2>/dev/null; then
      fail "snapshot-rerun-$T" "snapshot at $T differs across runs"
      SNAPS_SAME=0
    fi
  done
  if [ "$SNAPS_SAME" -eq 1 ]; then
    pass "snapshot-rerun-byte-identical (all five seeks)"
  fi
else
  fail "snapshot-rerun-exit" "second snapshot run failed; see snapshot2.log"
fi
if "$FSMITH" render "$REF" -o "$WORKDIR/run1.mp4" >"$WORKDIR/render1.log" 2>&1 \
  && "$FSMITH" render "$REF" -o "$WORKDIR/run2.mp4" >"$WORKDIR/render2.log" 2>&1; then
  if cmp -s "$WORKDIR/run1.mp4" "$WORKDIR/run2.mp4"; then
    pass "two-renders-byte-identical ($(wc -c <"$WORKDIR/run1.mp4") bytes)"
  else
    fail "render-determinism" "two renders of the reference differ"
  fi
else
  fail "render-exit" "render exited non-zero; see render1.log render2.log"
fi

# ---- gate 5: behind-subject cover pixels from the matte ----
echo ""
echo "--- gate 5: subject matte covers where opaque ---"
cp -r "$REF" "$WORKDIR/subj"
if python3 - "$WORKDIR/subj" <<'PYEOF' >>"$WORKDIR/snapshot.log" 2>&1
import sys
p = sys.argv[1] + '/composition.kdl'
s = open(p).read()
old = 'captions src="talk.srt"'
assert old in s, 'expected the captions clip'
open(p, 'w').write(s.replace(old, 'captions src="talk.srt" behind-subject=true matte="matte.png"'))
PYEOF
then
  if "$FSMITH" check "$WORKDIR/subj" >"$WORKDIR/check-subj.log" 2>&1 \
    && "$FSMITH" snapshot --at 1.0 --out "$WORKDIR/snaps-subj" "$WORKDIR/subj" >"$WORKDIR/snapshot-subj.log" 2>&1 \
    && [ -f "$WORKDIR/snaps-subj/snap-1s.png" ]; then
    if python3 "$WORKDIR/capcheck.py" mattecheck "$WORKDIR/snaps-subj/snap-1s.png" "$WORKDIR/snaps/snap-1s.png" >"$WORKDIR/mattecheck.log" 2>&1; then
      pass "matte-covers-opaque-shows-through ($(cat "$WORKDIR/mattecheck.log"))"
    else
      fail "matte-pixels" "cover/show-through mismatch; see mattecheck.log"
    fi
  else
    fail "matte-snapshot" "behind-subject check/snapshot failed; see check-subj.log snapshot-subj.log"
  fi
else
  fail "matte-stage" "could not order the copy behind the subject"
fi

# ---- gate 6: gated failures name their file ----
echo ""
echo "--- gate 6: broken imports refuse naming the file ---"
BAD="$WORKDIR/tampered"
cp -r "$REF" "$BAD"
printf '1\n00:00:00,500 --> 00:00:02,000\nhello\n\n2\n00:00:01,000 --> 00:00:03,000\noverlap\n\n' >"$BAD/talk.srt"
if "$FSMITH" check "$BAD" >"$WORKDIR/check-bad.log" 2>&1; then
  fail "tampered-check-nonzero" "check exited 0 on overlapping cues; see check-bad.log"
else
  if grep -q "talk.srt" "$WORKDIR/check-bad.log"; then
    pass "tampered-check-names-talk.srt"
  else
    fail "tampered-check-text" "refusal names no file; see check-bad.log"
  fi
fi
if "$FSMITH" render "$BAD" -o "$WORKDIR/tampered.mp4" >"$WORKDIR/render-bad.log" 2>&1; then
  fail "tampered-render-nonzero" "render exited 0 on overlapping cues; see render-bad.log"
else
  if grep -q "talk.srt" "$WORKDIR/render-bad.log"; then
    pass "tampered-render-names-talk.srt"
  else
    fail "tampered-render-text" "render refusal names no file; see render-bad.log"
  fi
fi
if [ -e "$WORKDIR/tampered.mp4" ]; then
  fail "tampered-no-mp4" "render wrote output despite the gated failure"
else
  pass "tampered-render-writes-no-mp4"
fi
cp -r "$REF" "$WORKDIR/nomatte"
if python3 - "$WORKDIR/nomatte" <<'PYEOF' >>"$WORKDIR/snapshot.log" 2>&1
import sys
p = sys.argv[1] + '/composition.kdl'
s = open(p).read()
old = 'captions src="talk.srt"'
assert old in s, 'expected the captions clip'
open(p, 'w').write(s.replace(old, 'captions src="talk.srt" behind-subject=true'))
PYEOF
then
  if "$FSMITH" check "$WORKDIR/nomatte" >"$WORKDIR/check-nomatte.log" 2>&1; then
    fail "nomatte-check-nonzero" "check exited 0 on a behind-subject order with no matte"
  else
    if grep -q "matte" "$WORKDIR/check-nomatte.log"; then
      pass "missing-matte-refusal-names-matte"
    else
      fail "nomatte-check-text" "refusal names no matte; see check-nomatte.log"
    fi
  fi
else
  fail "nomatte-stage" "could not strip the matte from the copy"
fi

# ---- gate 7: export round-trips with zero drift (on the staged copy) ----
echo ""
echo "--- gate 7: export round-trips with zero drift ---"
if FRAMESMITH_N22_PROOF_REF="$REF" cargo test --offline -p validate --test n22_captions_proof >"$WORKDIR/roundtrip.log" 2>&1; then
  if grep -q "1 passed" "$WORKDIR/roundtrip.log"; then
    pass "export-round-trips-zero-drift (srt/vtt/json on staged copy)"
  else
    fail "roundtrip-count" "test exited 0 without '1 passed'; see roundtrip.log"
  fi
else
  fail "roundtrip" "n22_captions_proof test failed; see roundtrip.log"
fi

# ---- gate 8: inspector reports readiness and the active cue ----
echo ""
echo "--- gate 8: inspector reports caption state ---"
if cargo test --offline -p preview --test n22_captions_inspector >"$WORKDIR/inspector.log" 2>&1; then
  if grep -q "2 passed" "$WORKDIR/inspector.log"; then
    pass "inspector-readiness-plus-active-cue (2 passed)"
  else
    fail "inspector-count" "test exited 0 without '2 passed'; see inspector.log"
  fi
else
  fail "inspector" "n22_captions_inspector tests failed; see inspector.log"
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
cp "$WORKDIR/report.txt" "$WORKDIR/check.log" \
  "$WORKDIR/lint.log" "$WORKDIR/snapshot.log" "$WORKDIR/snapshot2.log" \
  "$WORKDIR/snapshot-bare.log" "$WORKDIR/snapshot-vtt.log" \
  "$WORKDIR/snapshot-json.log" "$WORKDIR/snapshot-nopreset.log" \
  "$WORKDIR/snapshot-plain.log" "$WORKDIR/snapshot-subj.log" \
  "$WORKDIR/check-subj.log" "$WORKDIR/mattecheck.log" \
  "$WORKDIR/check-bad.log" "$WORKDIR/render-bad.log" \
  "$WORKDIR/check-nomatte.log" "$WORKDIR/roundtrip.log" \
  "$WORKDIR/inspector.log" "$WORKDIR/render1.log" "$WORKDIR/render2.log" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/snaps/snap-0.6s.png" "$WORKDIR/snaps/snap-1s.png" \
  "$WORKDIR/snaps/snap-1.7s.png" "$WORKDIR/snaps/snap-2.5s.png" \
  "$WORKDIR/snaps/snap-3.5s.png" "$WORKDIR/snaps-subj/snap-1s.png" \
  "$WORKDIR/run1.mp4" "$WORKDIR/run2.mp4" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$REF/composition.kdl" "$REF/talk.srt" "$REF/talk.vtt" "$REF/talk.json" \
  "$REF/matte.png" "$REF/README.md" \
  "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
