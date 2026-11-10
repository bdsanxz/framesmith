#!/bin/sh
# n21-adapters-proof.sh -- Sprint 21 T008 one-command gate.
#
# Proves the adapters reference end to end on a staged fixture copy
# (the checkout itself is never written):
#   1. `check` passes on a four-kind film (lottie + chart + scene3d +
#      map) and lists every source ready, per source and never per
#      frame, with a 4-ready tally;
#   2. `lint` accepts the reference (0 findings across the 13 stable
#      rules) and carries the same readiness tally;
#   3. seeks are time-correct: the t=1.0s snapshot shows the slide at
#      its exact pixel, every quadrant paints ink, distinct times
#      differ, and the loop wrap (t=2.5s) is byte-identical to t=0.5s;
#   4. the fifth kind (custom frame sources) seeks through the same
#      coordinator at the same film time, deterministically, via the
#      registry-level seek test (no Clip::Custom family exists in this
#      sprint; file-backed kinds cover the film);
#   5. a gated failure: a tampered copy with a corrupt lottie refuses
#      `check` and `render` naming the file, and writes no MP4;
#   6. reruns are byte-identical: two renders cmp equal, and a second
#      snapshot run cmps equal with the first.
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n21-adapters-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory).
#   Evidence is copied to evidences/proof-n21-T008/ on every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n21-adapters-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n21-T008"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n21-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n21 adapters reference proof gate (T008) ================"
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
echo "--- stage: four-kind adapters reference ---"
REF="$WORKDIR/ref"
mkdir -p "$REF/tiles/1/0" "$REF/tiles/1/1" || exit 1
cat >"$REF/composition.kdl" <<'EOF'
composition name="main" width=256 height=64 duration=6 fps=30 background="#000000" version=1 {
    lottie path="anim.json" start=0 duration=6 width=64 height=64 loop-mode="loop"
    chart kind="bar" data="sales.csv" start=0 duration=6 x=64 y=0 width=64 height=64
    scene3d start=0 duration=6 x=128 y=0 width=64 height=64 {
        object name="cube" kind="box" size="2,2,2" color="#c02020"
        camera mode="fixed" position="0,1,6" target="0,0,0"
    }
    map pack="tiles" center="0,0" zoom=1 start=0 duration=6 x=192 y=0 width=64 height=64 {
        marker lat=1 lon=1 label="hub" color="#ff0000"
    }
}
EOF
cat >"$REF/anim.json" <<'EOF'
{"v":"5.7.4","fr":30.0,"ip":0.0,"op":60.0,"w":64.0,"h":64.0,"nm":"slide","markers":[{"tm":0.0,"cm":"intro","dr":30.0}],"layers":[{"ddd":0,"ind":1,"ty":4,"nm":"box","sr":1.0,"ks":{"o":{"a":0,"k":100.0},"r":{"a":0,"k":0.0},"p":{"a":1,"k":[{"t":0.0,"s":[10.0,32.0,0.0],"e":[50.0,32.0,0.0]},{"t":30.0,"s":[50.0,32.0,0.0]}]},"a":{"a":0,"k":[0.0,0.0,0.0]},"s":{"a":0,"k":[100.0,100.0,100.0]}},"shapes":[{"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[0.0,0.0]},"s":{"a":0,"k":[20.0,20.0]},"r":{"a":0,"k":0.0}},{"ty":"fl","c":{"a":0,"k":[1.0,0.0,0.0,1.0]},"o":{"a":0,"k":100.0}},{"ty":"tr","p":{"a":0,"k":[0.0,0.0]},"a":{"a":0,"k":[0.0,0.0]},"s":{"a":0,"k":[100.0,100.0]},"r":{"a":0,"k":0.0},"o":{"a":0,"k":100.0}}]}]}]}
EOF
printf 'Q1,12\nQ2,19\n' >"$REF/sales.csv"
printf '{"attribution": "© fixtures", "min-zoom": 1, "max-zoom": 3}' >"$REF/tiles/pack.json"
# Offline tile pack: solid 256x256 PNGs (map tiles must be 256x256).
python3 - "$REF" >"$WORKDIR/tiles.log" 2>&1 <<'PYEOF'
import sys, zlib, struct, os
ref = sys.argv[1]
def chunk(kind, data):
    c = kind + data
    return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)
def solid(edge, rgb):
    raw = b''.join(b'\x00' + bytes(rgb) * edge for _ in range(edge))
    ihdr = struct.pack('>IIBBBBB', edge, edge, 8, 2, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', ihdr) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b'')
for x in (0, 1):
    for y in (0, 1):
        with open(os.path.join(ref, 'tiles', '1', str(x), str(y) + '.png'), 'wb') as f:
            f.write(solid(256, (200, 210, 220)))
PYEOF
if [ $? -eq 0 ] && [ -f "$REF/tiles/1/0/0.png" ]; then
  pass "staged-reference (lottie + chart + scene3d + map + pack)"
else
  fail "stage-tiles" "tile pack generation failed; see tiles.log"
fi

# Minimal PNG probe (stdlib only): pixel or per-region ink on RGB PNGs.
cat >"$WORKDIR/pngprobe.py" <<'PYEOF'
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
mode, path = sys.argv[1], sys.argv[2]
w, h, ch, px = decode(path)
if mode == 'pixel':
    x, y = int(sys.argv[3]), int(sys.argv[4])
    o = (y * w + x) * ch
    print(','.join(str(px[o + i]) for i in range(3)))
elif mode == 'ink':
    x0, x1 = int(sys.argv[3]), int(sys.argv[4])
    n = 0
    for yy in range(h):
        for xx in range(x0, x1):
            o = (yy * w + xx) * ch
            if px[o] != 0 or px[o + 1] != 0 or px[o + 2] != 0:
                n += 1
    print(n)
PYEOF

# ---- gate 1: check passes, every source ready per source ----
echo ""
echo "--- gate 1: check lists all four kinds ready ---"
if "$FSMITH" check "$REF" >"$WORKDIR/check.log" 2>&1; then
  pass "check-exits-0-on-reference"
else
  fail "check-exit" "check exited non-zero; see check.log"
fi
for KIND in lottie chart scene3d map; do
  if grep -q "readiness\\[$KIND\\].*: ready" "$WORKDIR/check.log" 2>/dev/null; then
    pass "check-readiness-$KIND-ready"
  else
    fail "check-readiness-$KIND" "no ready line for $KIND; see check.log"
  fi
done
if grep -q "readiness: 4 source(s): 4 ready, 0 loading, 0 failed" "$WORKDIR/check.log" 2>/dev/null; then
  pass "check-tally-4-ready-once (per source, never per frame)"
else
  fail "check-tally" "tally is not '4 ready, 0 loading, 0 failed'; see check.log"
fi

# ---- gate 2: lint accepts the reference, same readiness ----
echo ""
echo "--- gate 2: lint accepts adapters with stable rules ---"
if "$FSMITH" lint "$REF" >"$WORKDIR/lint.log" 2>&1; then
  pass "lint-exits-0-on-reference"
else
  fail "lint-exit" "lint exited non-zero; see lint.log"
fi
if grep -q "lint: 0 findings across 13 rules" "$WORKDIR/lint.log" 2>/dev/null; then
  pass "lint-0-findings-13-stable-rules"
else
  fail "lint-findings" "finding count/rules changed; see lint.log"
fi
if grep -q "readiness: 4 source(s): 4 ready, 0 loading, 0 failed" "$WORKDIR/lint.log" 2>/dev/null; then
  pass "lint-carries-same-readiness-tally"
else
  fail "lint-readiness" "lint lacks the 4-ready tally; see lint.log"
fi

# ---- gate 3: every file kind seeks time-correctly ----
echo ""
echo "--- gate 3: snapshots prove time-correct seeks + loop wrap ---"
if "$FSMITH" snapshot --at 0.5,1.0,2.5 --out "$WORKDIR/snaps" "$REF" >"$WORKDIR/snapshot.log" 2>&1; then
  pass "snapshot-renders-three-seeks"
else
  fail "snapshot-exit" "snapshot exited non-zero; see snapshot.log"
fi
SNAP15="$WORKDIR/snaps/snap-0.5s.png"
SNAP30="$WORKDIR/snaps/snap-1s.png"
SNAP75="$WORKDIR/snaps/snap-2.5s.png"
for F in "$SNAP15" "$SNAP30" "$SNAP75"; do
  if [ -f "$F" ]; then :; else fail "snapshot-file-$(basename "$F")" "missing $F; see snapshot.log"; fi
done
# The 2s lottie loop wraps: t=2.5s must equal t=0.5s byte for byte.
if [ -f "$SNAP15" ] && [ -f "$SNAP75" ]; then
  if cmp -s "$SNAP15" "$SNAP75"; then
    pass "loop-wrap-seek-t2.5-identical-to-t0.5"
  else
    fail "loop-wrap" "t=2.5s differs from t=0.5s; the loop does not wrap"
  fi
fi
# Distinct film times must differ: the slide moves across seeks.
if [ -f "$SNAP15" ] && [ -f "$SNAP30" ]; then
  if cmp -s "$SNAP15" "$SNAP30"; then
    fail "seek-motion" "t=0.5s equals t=1.0s; seeks show no time-correct frame"
  else
    pass "distinct-times-differ (slide moves with the seek)"
  fi
fi
# Exact pixel: at t=1.0s the slide sits at x=50 (red rect center).
if [ -f "$SNAP30" ]; then
  PIXEL="$(python3 "$WORKDIR/pngprobe.py" pixel "$SNAP30" 50 32 2>/dev/null || echo '?')"
  if [ "$PIXEL" = "255,0,0" ]; then
    pass "lottie-seek-exact-pixel (50,32) red at t=1.0s"
  else
    fail "lottie-pixel" "expected 255,0,0 at (50,32), got $PIXEL"
  fi
  # Every region paints at the seek: lottie | chart | scene3d | map.
  INK_OK=1
  for QUAD in "lottie 0 64" "chart 64 128" "scene3d 128 192" "map 192 256"; do
    set -- $QUAD
    INK="$(python3 "$WORKDIR/pngprobe.py" ink "$SNAP30" "$2" "$3" 2>/dev/null || echo 0)"
    if [ "$INK" -gt 20 ] 2>/dev/null; then
      pass "seek-$1-region-paints ($INK ink px)"
    else
      fail "seek-$1-region" "$1 quadrant has only $INK ink px at t=1.0s"
      INK_OK=0
    fi
  done
fi

# ---- gate 4: the fifth kind (custom sources) seeks at the same film time ----
echo ""
echo "--- gate 4: custom frame source seeks deterministically ---"
# No Clip::Custom family exists in this sprint, so custom sources ride
# the seek coordinator at registry level (T005/T006 contract); the
# seek unit test proves a ready custom source renders the same bytes
# at the same film time as the four file kinds above.
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p render --test n21_seek all_five_kinds_show_time_correct_frames_at_one_seek >"$WORKDIR/custom-seek.log" 2>&1); then
  if grep -q "1 passed" "$WORKDIR/custom-seek.log"; then
    pass "custom-source-seeks-at-film-time-byte-identical"
  else
    fail "custom-seek-count" "test exited 0 without '1 passed'; see custom-seek.log"
  fi
else
  fail "custom-seek" "n21_seek custom test failed; see custom-seek.log"
fi

# ---- gate 5: a gated failure names its file ----
echo ""
echo "--- gate 5: corrupt source refuses check + render by file name ---"
BAD="$WORKDIR/tampered"
cp -r "$REF" "$BAD"
printf '{oops' >"$BAD/anim.json"
if "$FSMITH" check "$BAD" >"$WORKDIR/check-bad.log" 2>&1; then
  fail "tampered-check-nonzero" "check exited 0 on a corrupt lottie; see check-bad.log"
else
  if grep -q "anim.json" "$WORKDIR/check-bad.log"; then
    pass "tampered-check-names-anim.json"
  else
    fail "tampered-check-text" "refusal names no file; see check-bad.log"
  fi
fi
if "$FSMITH" render "$BAD" -o "$WORKDIR/tampered.mp4" >"$WORKDIR/render-bad.log" 2>&1; then
  fail "tampered-render-nonzero" "render exited 0 on a corrupt lottie; see render-bad.log"
else
  if grep -q "anim.json" "$WORKDIR/render-bad.log"; then
    pass "tampered-render-names-anim.json"
  else
    fail "tampered-render-text" "render refusal names no file; see render-bad.log"
  fi
fi
if [ -e "$WORKDIR/tampered.mp4" ]; then
  fail "tampered-no-mp4" "render wrote output despite the gated failure"
else
  pass "tampered-render-writes-no-mp4"
fi

# ---- gate 6: deterministic reruns ----
echo ""
echo "--- gate 6: reruns are byte-identical ---"
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
if "$FSMITH" snapshot --at 0.5,1.0,2.5 --out "$WORKDIR/snaps2" "$REF" >"$WORKDIR/snapshot2.log" 2>&1; then
  SNAPS_SAME=1
  for T in 0.5s 1s 2.5s; do
    if ! cmp -s "$WORKDIR/snaps/snap-$T.png" "$WORKDIR/snaps2/snap-$T.png"; then
      fail "snapshot-rerun-$T" "snapshot at $T differs across runs"
      SNAPS_SAME=0
    fi
  done
  if [ "$SNAPS_SAME" -eq 1 ]; then
    pass "snapshot-rerun-byte-identical (all three seeks)"
  fi
else
  fail "snapshot-rerun-exit" "second snapshot run exited non-zero; see snapshot2.log"
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
cp "$WORKDIR/report.txt" "$WORKDIR/tiles.log" "$WORKDIR/check.log" \
  "$WORKDIR/lint.log" "$WORKDIR/snapshot.log" "$WORKDIR/snapshot2.log" \
  "$WORKDIR/custom-seek.log" "$WORKDIR/check-bad.log" \
  "$WORKDIR/render-bad.log" "$WORKDIR/render1.log" "$WORKDIR/render2.log" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/snaps/snap-0.5s.png" "$WORKDIR/snaps/snap-1s.png" \
  "$WORKDIR/snaps/snap-2.5s.png" "$WORKDIR/run1.mp4" "$WORKDIR/run2.mp4" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$REF/composition.kdl" "$REF/anim.json" "$REF/sales.csv" \
  "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
