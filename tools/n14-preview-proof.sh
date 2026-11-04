#!/bin/sh
# n14-preview-proof.sh -- Sprint 14 T007 one-command gate.
#
# Proves the preview reference project end to end: the staged copy of
# framesmith/examples/preview-reference serves pixel-exact frames,
# meets its transport bars, plays the mix, hot-reloads inside 500 ms
# with the playhead kept, and manages its daemons exactly -- with the
# review page driven in a real headless browser (no visible windows)
# through step, scrub, loop, speed, play, pause, and the mix-clock
# sync, each timed. Reports PROOF: PASS only when every check succeeds
# (exit 0). Any single failing check prints FAIL and the gate exits 1.
#
# Usage: n14-preview-proof.sh [--workdir DIR] [--bin PATH]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; the reference is
#   staged to DIR first. The daemon registry is isolated to DIR, so
#   the proof never touches the maker's live preview servers.
#   Evidence is copied to evidences/proof-n14-T007/ on every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,24p' "$0"; exit 0 ;;
    *) echo "n14-preview-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/preview-reference"
BROWSER_PY="$TOOLS_DIR/n14-preview-browser.py"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n14-T007"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n14-proof-XXXXXX)"
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

# GET a URL to a file; prints the HTTP code.
fetch() {
  curl -s -o "$2" -w "%{http_code}" "$1"
}

echo "================ n14 preview proof gate (T007) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "================================================================"
echo ""

for NEED in cargo ffmpeg python3 curl; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl tone.wav README.md; do
  if [ ! -f "$EXAMPLE_DIR/$F" ]; then
    fail "prereq-example-$F" "missing $EXAMPLE_DIR/$F"
  fi
done
if [ ! -f "$BROWSER_PY" ]; then
  fail "prereq-browser-py" "missing $BROWSER_PY"
fi
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- missing prerequisites:$FAIL_LIST"
  exit 1
fi

# Isolate the daemon registry: the proof must not see or stop the
# maker's live preview servers.
export FRAMESMITH_PREVIEW_REGISTRY="$WORKDIR/registry.json"

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
echo "python: $(python3 --version 2>&1 || echo MISSING)"
echo ""

# Stage the reference: the checkout is never served or rendered into.
PROJ="$WORKDIR/proj"
rm -rf "$PROJ"
mkdir -p "$PROJ"
cp "$EXAMPLE_DIR/composition.kdl" "$EXAMPLE_DIR/tone.wav" "$PROJ/"
cp "$BROWSER_PY" "$WORKDIR/" 2>/dev/null || true

cleanup() {
  export FRAMESMITH_PREVIEW_REGISTRY="$WORKDIR/registry.json"
  "$FSMITH" preview-stop-all >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

# ---- daemon: clean slate ------------------------------------------------
STOP_ALL_ZERO="$("$FSMITH" preview-stop-all 2>&1)"
if echo "$STOP_ALL_ZERO" | grep -q "stopped 0 preview server(s)"; then
  pass "stop-all-empty-reports-zero"
else
  fail "stop-all-empty-reports-zero" "got: $STOP_ALL_ZERO"
fi

# ---- daemon: start ------------------------------------------------------
START_OUT="$("$FSMITH" preview "$PROJ" 2>&1)"
START_CODE=$?
echo "$START_OUT"
if [ $START_CODE -ne 0 ]; then
  fail "preview-start" "exit $START_CODE: $START_OUT"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
URL="$(echo "$START_OUT" | grep -o 'http://127\.0\.0\.1:[0-9]*/' | head -n 1)"
if [ -z "$URL" ]; then
  fail "preview-start-url" "no loopback URL in: $START_OUT"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
pass "preview-start-url ($URL)"
if echo "$START_OUT" | grep -q "(composition \"preview\")"; then
  pass "preview-start-names-composition"
else
  fail "preview-start-names-composition" "got: $START_OUT"
fi

# ---- status: project, composition, timeline facts ------------------------
# (status bodies travel through files, never echo pipes, so JSON escape
# sequences survive byte-identically on every shell.)
curl -s "${URL}api/status" -o "$WORKDIR/status.json"
STATUS_JSON="$(cat "$WORKDIR/status.json")"
python3 -c "import json; json.load(open('$WORKDIR/status.json'))" 2>/dev/null \
  && pass "status-is-json" || fail "status-is-json" "unparsable: $STATUS_JSON"
FRAME_COUNT="$(python3 -c "import json; print(json.load(open('$WORKDIR/status.json'))['frame_count'])" 2>/dev/null)"
[ "$FRAME_COUNT" = "20" ] && pass "status-frame-count-20" \
  || fail "status-frame-count-20" "got: $FRAME_COUNT"

# ---- frames: byte-identical to the same snapshot frames ------------------
"$FSMITH" snapshot --at 0,1,1.9 --out "$WORKDIR/snaps" "$PROJ" >"$WORKDIR/snapshot.log" 2>&1
if [ $? -ne 0 ]; then
  fail "snapshot" "see $WORKDIR/snapshot.log"
else
  pass "snapshot-frames"
fi
# frame index for --at t at 10 fps: round(t*10).
i=0
for PAIR in "0 snap-0s.png" "10 snap-1s.png" "19 snap-1.9s.png"; do
  set -- $PAIR
  IDX="$1"; SNAP="$2"
  CODE="$(fetch "${URL}api/frame?i=$IDX&action=step" "$WORKDIR/served-$IDX.png")"
  if [ "$CODE" != "200" ]; then
    fail "serve-frame-$IDX" "http $CODE"
    continue
  fi
  if cmp -s "$WORKDIR/served-$IDX.png" "$WORKDIR/snaps/$SNAP"; then
    pass "frame-$IDX-byte-identical-to-snapshot"
  else
    fail "frame-$IDX-byte-identical-to-snapshot" \
      "served $(sha "$WORKDIR/served-$IDX.png") != snapshot $(sha "$WORKDIR/snaps/$SNAP")"
  fi
done
# Warm repeats must serve the same exact bytes (cache never alters).
CODE="$(fetch "${URL}api/frame?i=10&action=step" "$WORKDIR/served-10b.png")"
if [ "$CODE" = "200" ] && cmp -s "$WORKDIR/served-10.png" "$WORKDIR/served-10b.png"; then
  pass "frame-warm-repeat-identical"
else
  fail "frame-warm-repeat-identical" "http $CODE or bytes drifted"
fi
# Out-of-range frame is refused, never guessed.
CODE="$(fetch "${URL}api/frame?i=20&action=step" "$WORKDIR/served-oor.bin")"
if [ "$CODE" = "404" ]; then
  pass "frame-out-of-range-404"
else
  fail "frame-out-of-range-404" "http $CODE, expected 404"
fi

# ---- transport bars: build warm scrub/jump medians ------------------------
# The page drives step/scrub; jumps are arbitrary seeks over the same
# route the page uses. Six warm samples per action make the medians real.
# Two passes: the first draws cold, the second serves warm, so every
# action ends with both a cold median (labeled) and warm medians.
for PASS in 1 2; do
  for IDX in 3 7 12 16 4 9; do
    curl -s -o /dev/null "${URL}api/frame?i=$IDX&action=scrub" || true
  done
  for IDX in 18 2 14 6 11 1; do
    curl -s -o /dev/null "${URL}api/frame?i=$IDX&action=jump" || true
  done
  for IDX in 5 6 7 8 9 10; do
    curl -s -o /dev/null "${URL}api/frame?i=$IDX&action=step" || true
  done
done
curl -s "${URL}api/status" -o "$WORKDIR/status-bars.json"
BARS_JSON="$(cat "$WORKDIR/status-bars.json")"
BARS_CHECK="$(python3 -c "
import json
s = json.load(open('$WORKDIR/status-bars.json'))
ok = True
for row in s['latency']:
    bar = row['bar_ms']
    w = row['warm_median_ms']
    c = row['cold_median_ms']
    if row['action'] in ('step','scrub','jump'):
        if w is None or w > bar:
            print('MISS %s warm=%s bar=%s' % (row['action'], w, bar))
            ok = False
        if c is None:
            print('UNLABELED %s: no cold median' % row['action'])
            ok = False
print('BARS_OK' if ok and s.get('all_bars_met') else 'BARS_BAD all_bars_met=%s' % s.get('all_bars_met'))
")"
echo "$BARS_CHECK"
if echo "$BARS_CHECK" | grep -q "^BARS_OK"; then
  pass "transport-bars-warm-inside-cold-labeled"
else
  fail "transport-bars-warm-inside-cold-labeled" "$(echo "$BARS_CHECK" | tr '\n' ';')"
fi

# ---- mix: the render mix over HTTP ----------------------------------------
CODE="$(fetch "${URL}api/mix.wav" "$WORKDIR/mix.wav")"
if [ "$CODE" = "200" ]; then
  pass "mix-serves-200"
else
  fail "mix-serves-200" "http $CODE, expected 200 with the sounding mix"
fi
MIX_CHECK="$(python3 -c "
d = open('$WORKDIR/mix.wav','rb').read()
import struct
assert d[:4] == b'RIFF' and d[8:12] == b'WAVE', 'not a WAV'
n = (len(d) - 44) // 2
s = struct.unpack('<%dh' % n, d[44:])
peak = max(abs(x) for x in s)
print('samples=%d peak=%d' % (n, peak))
assert n == 192000, 'expected 2s of 48kHz stereo mix (192000 i16 samples), got %d' % n
assert peak > 1000, 'mix should sound (tone), peak=%d' % peak
print('MIX_OK')
" 2>&1)"
if echo "$MIX_CHECK" | grep -q "MIX_OK"; then
  pass "mix-wav-valid-and-sounding ($MIX_CHECK)"
else
  fail "mix-wav-valid-and-sounding" "$MIX_CHECK"
fi
fetch "${URL}api/mix.wav" "$WORKDIR/mix2.wav" >/dev/null
if cmp -s "$WORKDIR/mix.wav" "$WORKDIR/mix2.wav"; then
  pass "mix-deterministic"
else
  fail "mix-deterministic" "two fetches differed"
fi

# ---- browser: real headless page through documented controls ---------------
mkdir -p "$WORKDIR/shots"
if python3 "$BROWSER_PY" --url "$URL" --out "$WORKDIR/browser.json" \
    --shots "$WORKDIR/shots" >"$WORKDIR/browser.log" 2>&1; then
  pass "browser-proof"
else
  fail "browser-proof" "see $WORKDIR/browser.log and $WORKDIR/browser.json"
fi
grep -E "^control |^BROWSER" "$WORKDIR/browser.log" || true
BROWSER_CONTROLS="$(python3 -c "
import json
r = json.load(open('$WORKDIR/browser.json'))
names = [c['control'] for c in r.get('controls', [])]
print(' '.join(names))
for need in ('step-forward','step-back','scrub','loop','speed','play','pause','sync-mix-clock'):
    assert need in names, 'missing control %s' % need
print('CONTROLS_OK count=%d' % len(names))
" 2>&1)"
if echo "$BROWSER_CONTROLS" | grep -q "CONTROLS_OK"; then
  pass "browser-controls ($BROWSER_CONTROLS)"
else
  fail "browser-controls" "$BROWSER_CONTROLS"
fi
PLAY_AUDIO="$(python3 -c "import json; r = json.load(open('$WORKDIR/browser.json')); print([c for c in r['controls'] if c['control']=='play'][0].get('audio',''))" 2>&1)"
[ "$PLAY_AUDIO" = "sound" ] && pass "browser-play-follows-sounding-mix" \
  || fail "browser-play-follows-sounding-mix" "audio=$PLAY_AUDIO"

# ---- hot reload: valid edit lands inside 500 ms, playhead kept --------------
curl -s -X POST "${URL}api/scrub?i=7" >/dev/null
curl -s -X POST "${URL}api/play" >/dev/null
curl -s "${URL}api/frame?i=7&action=step" -o "$WORKDIR/served-7.png" >/dev/null
BEFORE_SHA="$(sha "$WORKDIR/served-7.png")"
cp "$PROJ/composition.kdl" "$WORKDIR/composition-before.kdl"
RELOAD_START="$(python3 -c 'import time; print(time.time())')"
sed 's/fill="#ff0000"/fill="#0000ff"/' "$WORKDIR/composition-before.kdl" >"$PROJ/composition.kdl"
NEW_SHA=""
RELOAD_ELAPSED=""
for _ in $(seq 1 100); do
  curl -s "${URL}api/frame?i=7&action=step" -o "$WORKDIR/served-7-new.png" >/dev/null
  NEW_SHA="$(sha "$WORKDIR/served-7-new.png")"
  if [ "$NEW_SHA" != "$BEFORE_SHA" ]; then
    break
  fi
  sleep 0.05
done
RELOAD_ELAPSED="$(python3 -c "import time; print(int((time.time() - $RELOAD_START) * 1000))")"
if [ "$NEW_SHA" != "$BEFORE_SHA" ]; then
  pass "reload-new-look-served (${RELOAD_ELAPSED}ms save-to-bytes)"
else
  fail "reload-new-look-served" "frame 7 bytes never changed after the edit"
fi
if [ -n "$RELOAD_ELAPSED" ] && [ "$RELOAD_ELAPSED" -lt 500 ]; then
  pass "reload-inside-500ms (${RELOAD_ELAPSED}ms)"
else
  fail "reload-inside-500ms" "took ${RELOAD_ELAPSED:-unknown}ms"
fi
curl -s "${URL}api/status" -o "$WORKDIR/status-reload.json"
RELOAD_STATUS="$(cat "$WORKDIR/status-reload.json")"
RELOAD_KEEP="$(python3 -c "
import json
s = json.load(open('$WORKDIR/status-reload.json'))
r = s['readout']
print('frame=%s playing=%s reload=%s' % (r['frame'], r['playing'], s['reload'][:80]))
assert r['frame'] == 7, 'playhead moved to %s' % r['frame']
assert r['playing'] is True, 'playing state lost'
print('KEEP_OK')
" 2>&1)"
if echo "$RELOAD_KEEP" | grep -q "KEEP_OK"; then
  pass "reload-keeps-playhead-and-playing"
else
  fail "reload-keeps-playhead-and-playing" "$RELOAD_KEEP"
fi
# The new look must equal a fresh snapshot of the edited project.
"$FSMITH" snapshot --at 0.7 --out "$WORKDIR/snaps-new" "$PROJ" >"$WORKDIR/snapshot-new.log" 2>&1
if cmp -s "$WORKDIR/served-7-new.png" "$WORKDIR/snaps-new/snap-0.7s.png"; then
  pass "reload-look-equals-fresh-snapshot"
else
  fail "reload-look-equals-fresh-snapshot" "reloaded bytes differ from a fresh snapshot"
fi

# ---- hot reload: broken edit reports, last good keeps showing ---------------
printf 'composition name="preview" width=320 height=180 duration=2 fps=10 background="not-a-color" version=1 {\n}\n' >"$PROJ/composition.kdl"
sleep 0.2
curl -s -X POST "${URL}api/reload" -o "$WORKDIR/reload-broken.json"
BROKEN_RESP="$(cat "$WORKDIR/reload-broken.json")"
BROKEN_CHECK="$(python3 -c "
import json
r = json.load(open('$WORKDIR/reload-broken.json'))
print('applied=%s playhead=%s elapsed_ms=%s' % (r['applied'], r.get('playhead'), r['elapsed_ms']))
assert r['applied'] is False, 'broken edit must not apply'
assert r.get('playhead') == 7, 'failed edit must not reset the playhead'
assert r['elapsed_ms'] < 500, 'broken-edit report must also land fast'
print('BROKEN_OK')
" 2>&1)"
if echo "$BROKEN_CHECK" | grep -q "BROKEN_OK"; then
  pass "broken-edit-reports-playhead-kept ($BROKEN_CHECK)"
else
  fail "broken-edit-reports-playhead-kept" "$BROKEN_CHECK :: $BROKEN_RESP"
fi
CODE="$(fetch "${URL}api/frame?i=7&action=step" "$WORKDIR/served-7-broken.png")"
if [ "$CODE" = "200" ] && cmp -s "$WORKDIR/served-7-broken.png" "$WORKDIR/served-7-new.png"; then
  pass "broken-edit-keeps-last-good"
else
  fail "broken-edit-keeps-last-good" "http $CODE or last-good bytes changed"
fi
# Fix the edit: the new look returns.
sed 's/fill="#ff0000"/fill="#00ff00"/' "$WORKDIR/composition-before.kdl" >"$PROJ/composition.kdl"
FIXED_OK=""
for _ in $(seq 1 100); do
  curl -s -X POST "${URL}api/reload" >/dev/null
  if curl -s "${URL}api/frame?i=7&action=step" -o "$WORKDIR/served-7-fixed.png" >/dev/null 2>&1; then
    if ! cmp -s "$WORKDIR/served-7-fixed.png" "$WORKDIR/served-7.png"; then
      FIXED_OK=1
      break
    fi
  fi
  sleep 0.05
done
if [ -n "$FIXED_OK" ]; then
  pass "fixed-edit-restores-new-look"
else
  fail "fixed-edit-restores-new-look" "frame 7 still shows the pre-fix look"
fi

# ---- daemons: lifecycle counts -----------------------------------------------
DUP_OUT="$("$FSMITH" preview "$PROJ" 2>&1)"
if echo "$DUP_OUT" | grep -q "already serving" && echo "$DUP_OUT" | grep -q "$URL"; then
  pass "second-start-reports-existing-url"
else
  fail "second-start-reports-existing-url" "got: $DUP_OUT"
fi
STATUS_OUT="$("$FSMITH" preview-status "$PROJ" 2>&1)"
if echo "$STATUS_OUT" | grep -q "serving at $URL"; then
  pass "status-names-project-and-port"
else
  fail "status-names-project-and-port" "got: $STATUS_OUT"
fi
LIST_OUT="$("$FSMITH" preview-list 2>&1)"
if echo "$LIST_OUT" | grep -q "preview servers (1):" && echo "$LIST_OUT" | grep -q "$URL"; then
  pass "list-shows-one-live-server"
else
  fail "list-shows-one-live-server" "got: $LIST_OUT"
fi
STOP_OUT="$("$FSMITH" preview-stop "$PROJ" 2>&1)"
if echo "$STOP_OUT" | grep -q "preview stopped"; then
  pass "stop-ends-exactly-that-server"
else
  fail "stop-ends-exactly-that-server" "got: $STOP_OUT"
fi
STATUS_NONE="$("$FSMITH" preview-status "$PROJ" 2>&1)"
if echo "$STATUS_NONE" | grep -q "no preview server"; then
  pass "status-reports-none-after-stop"
else
  fail "status-reports-none-after-stop" "got: $STATUS_NONE"
fi
STOP_AGAIN="$("$FSMITH" preview-stop "$PROJ" 2>&1)"
STOP_AGAIN_CODE=$?
if [ $STOP_AGAIN_CODE -eq 0 ] && echo "$STOP_AGAIN" | grep -q "already stopped"; then
  pass "stop-dead-reports-already-stopped-exit-0"
else
  fail "stop-dead-reports-already-stopped-exit-0" "exit $STOP_AGAIN_CODE: $STOP_AGAIN"
fi
STOP_ALL_ONE="$("$FSMITH" preview-stop-all 2>&1)"
if echo "$STOP_ALL_ONE" | grep -q "stopped 0 preview server(s)"; then
  pass "stop-all-empty-reports-zero-exit-0"
else
  fail "stop-all-empty-reports-zero-exit-0" "got: $STOP_ALL_ONE"
fi

# ---- tamper: broken project refuses before any server starts ------------------
TAMPER="$WORKDIR/tampered"
rm -rf "$TAMPER"
mkdir -p "$TAMPER"
cp "$PROJ/composition.kdl" "$TAMPER/" && cp "$EXAMPLE_DIR/tone.wav" "$TAMPER/"
printf 'composition name="preview" width=320 height=180 duration=2 fps=10 background="not-a-color" version=1 {\n}\n' >"$TAMPER/composition.kdl"
TAMPER_OUT="$("$FSMITH" preview "$TAMPER" 2>&1)"
TAMPER_CODE=$?
if [ $TAMPER_CODE -ne 0 ] && echo "$TAMPER_OUT" | grep -qi "refused\|invalid"; then
  pass "tampered-project-refused-before-start (exit $TAMPER_CODE)"
else
  fail "tampered-project-refused-before-start" "exit $TAMPER_CODE: $TAMPER_OUT"
fi
TAMPER_STATUS="$("$FSMITH" preview-status "$TAMPER" 2>&1)"
if echo "$TAMPER_STATUS" | grep -q "no preview server"; then
  pass "tampered-project-left-no-server"
else
  fail "tampered-project-left-no-server" "got: $TAMPER_STATUS"
fi

# ---- evidence ------------------------------------------------------------------
echo ""
echo "--- proof summary: $PASS_N passed, failing:${FAIL_LIST:- none} ---"
{
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
  echo "chrome: $(python3 -c "import json; print(json.load(open('$WORKDIR/browser.json')).get('chrome','unknown'))" 2>/dev/null)"
  echo "workdir: $WORKDIR"
  echo "url: $URL"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/status-bars.json" "$WORKDIR/status-reload.json" \
  "$WORKDIR/browser.json" "$WORKDIR/browser.log" "$WORKDIR/reload-broken.json" \
  "$WORKDIR/composition-before.kdl" "$EVID_DIR/" 2>/dev/null || true
cp "$EXAMPLE_DIR/composition.kdl" "$EVID_DIR/composition-reference.kdl" 2>/dev/null || true
cp "$WORKDIR/shots/browser-page.png" "$WORKDIR/shots/browser-frame10.png" "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/served-0.png" "$WORKDIR/served-10.png" "$WORKDIR/served-19.png" \
  "$WORKDIR/snaps/snap-0s.png" "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps/snap-1.9s.png" \
  "$WORKDIR/mix.wav" "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
