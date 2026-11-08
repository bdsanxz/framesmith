#!/bin/sh
# n16-studio-proof.sh -- Sprint 16 T009 one-command gate.
#
# Proves the Studio reference project end to end on a staged copy of
# framesmith/examples/studio-reference: the shell arranges with theme and
# per-project layout restore, visual timeline edits land as named history
# turns with receipts, keyframe and beat edits retime with beat-only undo,
# clip visuals read truthfully with named placeholders, a remapped shortcut
# works immediately with defaults underneath, and a conflict resolves by
# pick with both versions kept. A tamper negative (corrupted composition)
# must refuse with a non-zero exit. Reports PROOF: PASS only when every
# check succeeds (exit 0); any single failing check prints FAIL and the
# gate exits 1.
#
# Usage: n16-studio-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory); the
#   checkout itself is never written. The daemon registry is isolated to
#   DIR, so the proof never touches the maker's live preview servers.
#   Evidence is copied to evidences/proof-n16-T009/ on every run.
#
# Note: no headless browser ships on this machine, so the served /studio
# page is asserted over HTTP (panels, theme, banners present) and the full
# page HTML is kept as evidence instead of screenshots.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,24p' "$0"; exit 0 ;;
    *) echo "n16-studio-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/studio-reference"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n16-T009"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n16-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

# HTTP helpers: bodies travel through files, never echo pipes, so JSON
# escape sequences survive byte-identically on every shell.
api_get() {
  # $1=url $2=outfile -> prints HTTP code
  curl -s -o "$2" -w "%{http_code}" "$1"
}
api_post() {
  # $1=url $2=body $3=outfile -> prints HTTP code
  printf '%s' "$2" | curl -s -o "$3" -X POST --data-binary @- "$1" -w "%{http_code}"
}

echo "================ n16 studio proof gate (T009) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "================================================================"
echo ""

for NEED in cargo curl python3 ffmpeg; do
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
cp "$EXAMPLE_DIR/composition.kdl" "$EXAMPLE_DIR/README.md" "$PROJ/"

# Sounding media: hand-written 48 kHz stereo s16 sine so the proof needs
# no encoder for audio.
python3 - "$PROJ/s.wav" <<'EOF'
import math, struct, sys
rate, secs = 48000, 4.0
frames = int(round(secs * rate))
pcm = b"".join(
  struct.pack("<h", int(math.sin(i * 440.0 * 2.0 * math.pi / rate) * 10000))
  for i in range(frames * 2)
)
with open(sys.argv[1], "wb") as f:
  f.write(b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt ")
  f.write(struct.pack("<IHHIIHH", 16, 1, 2, rate, rate * 4, 4, 16))
  f.write(b"data" + struct.pack("<I", len(pcm)) + pcm)
EOF
if [ -f "$PROJ/s.wav" ]; then pass "stage-s.wav"; else fail "stage-s.wav" "wav not written"; fi
# Graded video: tiny deterministic testsrc for the filmstrip.
if ffmpeg -y -v error -f lavfi -i "testsrc=duration=2:size=64x48:rate=10" \
    -c:v libx264 -pix_fmt yuv420p "$PROJ/v.mp4" 2>"$WORKDIR/ffmpeg.log"; then
  pass "stage-v.mp4"
else
  fail "stage-v.mp4" "see $WORKDIR/ffmpeg.log"
fi

# A sibling project proving layout and theme restore per project.
PROJ2="$WORKDIR/proj2"
rm -rf "$PROJ2"
mkdir -p "$PROJ2"
printf 'composition name="t" width=320 height=180 duration=2 fps=10 background="#112233" version=1 {\n    text id="title" value="Hi" start=0 duration=2 x=10 y=40 size=24 color="#ffffff"\n}\n' >"$PROJ2/composition.kdl"

cleanup() {
  export FRAMESMITH_PREVIEW_REGISTRY="$WORKDIR/registry.json"
  "$FSMITH" preview-stop-all >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

"$FSMITH" preview-stop-all >/dev/null 2>&1 || true

# ---- server: start on the staged reference -------------------------------
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
pass "preview-start ($URL)"
# Wait until the server answers.
READY=""
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "${URL}api/status" 2>/dev/null; then READY=1; break; fi
  sleep 0.5
done
if [ -n "$READY" ]; then pass "server-ready"; else fail "server-ready" "no answer at ${URL}api/status"; fi

# ---- gate 1: shell arranges with theme, layout restores per project -----
CODE="$(api_get "${URL}api/studio/theme" "$WORKDIR/theme0.json")"
if [ "$CODE" = "200" ] && grep -q '"theme":"light"' "$WORKDIR/theme0.json"; then
  pass "theme-defaults-light"
else
  fail "theme-defaults-light" "http $CODE: $(cat "$WORKDIR/theme0.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/theme" '{"theme":"dark"}' "$WORKDIR/theme1.json")"
if [ "$CODE" = "200" ] && grep -q '"theme":"dark"' "$WORKDIR/theme1.json"; then
  pass "theme-switches-dark"
else
  fail "theme-switches-dark" "http $CODE: $(cat "$WORKDIR/theme1.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/layout" '{"version":1,"panels":{"preview":{"dock":"left"},"timeline":{"dock":"right"}}}' "$WORKDIR/layout-save.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/layout-save.json"; then
  pass "layout-arranges-and-saves"
else
  fail "layout-arranges-and-saves" "http $CODE: $(cat "$WORKDIR/layout-save.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/layout" "$WORKDIR/layout-get.json")"
if [ "$CODE" = "200" ] && grep -q '"restored":true' "$WORKDIR/layout-get.json" \
    && grep -q '"dock":"left"' "$WORKDIR/layout-get.json"; then
  pass "layout-restores-arrangement"
else
  fail "layout-restores-arrangement" "http $CODE: $(cat "$WORKDIR/layout-get.json" 2>/dev/null)"
fi
if [ -f "$PROJ/.framesmith/studio-layout.json" ] && [ -f "$PROJ/.framesmith/studio-theme.json" ]; then
  pass "layout-and-theme-live-per-project-hidden-dir"
else
  fail "layout-and-theme-live-per-project-hidden-dir" "missing files under $PROJ/.framesmith"
fi
CODE="$(api_get "${URL}studio" "$WORKDIR/studio-dark.html")"
if [ "$CODE" = "200" ] && grep -q 'data-theme="dark"' "$WORKDIR/studio-dark.html" \
    && grep -q 'id="panel-preview"' "$WORKDIR/studio-dark.html" \
    && grep -q 'id="panel-timeline"' "$WORKDIR/studio-dark.html" \
    && grep -q 'layout restored' "$WORKDIR/studio-dark.html"; then
  pass "page-paints-arranged-dark-shell"
else
  fail "page-paints-arranged-dark-shell" "http $CODE"
fi
# Per-project: a second project still opens the default arrangement + light.
START2_OUT="$("$FSMITH" preview "$PROJ2" 2>&1)"
URL2="$(echo "$START2_OUT" | grep -o 'http://127\.0\.0\.1:[0-9]*/' | head -n 1)"
if [ -z "$URL2" ]; then
  fail "preview-second-project" "no loopback URL in: $START2_OUT"
else
  pass "preview-second-project ($URL2)"
  for _ in $(seq 1 60); do
    if curl -s -o /dev/null "${URL2}api/status" 2>/dev/null; then break; fi
    sleep 0.5
  done
  CODE="$(api_get "${URL2}api/studio/layout" "$WORKDIR/layout-proj2.json")"
  if [ "$CODE" = "200" ] && grep -q '"restored":false' "$WORKDIR/layout-proj2.json"; then
    pass "layout-is-per-project"
  else
    fail "layout-is-per-project" "http $CODE: $(cat "$WORKDIR/layout-proj2.json" 2>/dev/null)"
  fi
  CODE="$(api_get "${URL2}api/studio/theme" "$WORKDIR/theme-proj2.json")"
  if [ "$CODE" = "200" ] && grep -q '"theme":"light"' "$WORKDIR/theme-proj2.json"; then
    pass "theme-is-per-project"
  else
    fail "theme-is-per-project" "http $CODE: $(cat "$WORKDIR/theme-proj2.json" 2>/dev/null)"
  fi
fi

# ---- gate 2: visual timeline edits land as named history turns -----------
CODE="$(api_post "${URL}api/studio/tool" '{"tool":"select","clip":"title","to":1.0}' "$WORKDIR/tool-move.json")"
if [ "$CODE" = "200" ] && grep -q '"applied":true' "$WORKDIR/tool-move.json" \
    && grep -q 'select moved' "$WORKDIR/tool-move.json" \
    && grep -q '"turn":' "$WORKDIR/tool-move.json" \
    && grep -q '\[1.000,3.000\]' "$WORKDIR/tool-move.json"; then
  pass "tool-move-applies-with-turn-receipt"
else
  fail "tool-move-applies-with-turn-receipt" "http $CODE: $(cat "$WORKDIR/tool-move.json" 2>/dev/null)"
fi
if [ -f "$PROJ/.framesmith/history/turns.json" ] \
    && grep -q '"actor": *"studio"' "$PROJ/.framesmith/history/turns.json"; then
  pass "tool-move-recorded-as-named-studio-turn"
else
  fail "tool-move-recorded-as-named-studio-turn" "no studio turn in history"
fi
CODE="$(api_get "${URL}api/studio/timeline" "$WORKDIR/rows-moved.json")"
if [ "$CODE" = "200" ] && grep -q '"id":"title"' "$WORKDIR/rows-moved.json"; then
  pass "timeline-rows-show-moved-clip"
else
  fail "timeline-rows-show-moved-clip" "http $CODE"
fi

# ---- gate 3: keyframes retime, beats edit with beat-only undo ------------
CODE="$(api_post "${URL}api/studio/keyframe" '{"actor":"maker","action":"retime","key":0,"stop":1,"at":1.5}' "$WORKDIR/key-retime.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/key-retime.json" \
    && grep -q 'retime key 0 stop 1' "$WORKDIR/key-retime.json" \
    && grep -q '"turn":' "$WORKDIR/key-retime.json" \
    && grep -q '"clamped":false' "$WORKDIR/key-retime.json"; then
  pass "keyframe-retimes-with-turn-receipt"
else
  fail "keyframe-retimes-with-turn-receipt" "http $CODE: $(cat "$WORKDIR/key-retime.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/lanes" "$WORKDIR/lanes.json")"
if [ "$CODE" = "200" ] && grep -q '"at":1.500' "$WORKDIR/lanes.json"; then
  pass "lanes-show-retimed-diamond"
else
  fail "lanes-show-retimed-diamond" "http $CODE: $(cat "$WORKDIR/lanes.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/beats" '{"action":"add","name":"downbeat","at":1}' "$WORKDIR/beat-add.json")"
[ "$CODE" = "200" ] && pass "beat-adds" || fail "beat-adds" "http $CODE"
CODE="$(api_post "${URL}api/studio/beats" '{"action":"add","name":"upbeat","at":3}' "$WORKDIR/beat-add2.json")"
[ "$CODE" = "200" ] && pass "beat-adds-second" || fail "beat-adds-second" "http $CODE"
CODE="$(api_post "${URL}api/studio/beats" '{"action":"delete","name":"downbeat"}' "$WORKDIR/beat-del.json")"
[ "$CODE" = "200" ] && pass "beat-deletes" || fail "beat-deletes" "http $CODE: $(cat "$WORKDIR/beat-del.json" 2>/dev/null)"
CODE="$(api_post "${URL}api/studio/beats" '{"action":"undo"}' "$WORKDIR/beat-undo.json")"
if [ "$CODE" = "200" ] && grep -q 'restored 2 beat(s)' "$WORKDIR/beat-undo.json"; then
  pass "beat-undo-restores-beats-only"
else
  fail "beat-undo-restores-beats-only" "http $CODE: $(cat "$WORKDIR/beat-undo.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/beats" "$WORKDIR/beats.json")"
if [ "$CODE" = "200" ] && grep -q '"name":"downbeat"' "$WORKDIR/beats.json" \
    && grep -q '"name":"upbeat"' "$WORKDIR/beats.json"; then
  pass "beat-strip-holds-both-beats"
else
  fail "beat-strip-holds-both-beats" "http $CODE: $(cat "$WORKDIR/beats.json" 2>/dev/null)"
fi
# Beat undo never retimes clips: the moved title window is untouched.
CODE="$(api_get "${URL}api/studio/timeline" "$WORKDIR/rows-after-beats.json")"
if [ "$CODE" = "200" ] && grep -q '\[1.000,3.000\]' "$WORKDIR/rows-after-beats.json"; then
  pass "beat-undo-leaves-clips-untouched"
else
  fail "beat-undo-leaves-clips-untouched" "http $CODE: $(cat "$WORKDIR/rows-after-beats.json" 2>/dev/null)"
fi

# ---- gate 4: clip visuals read truthfully, placeholders named ------------
CODE="$(api_get "${URL}api/studio/visuals" "$WORKDIR/visuals.json")"
if [ "$CODE" = "200" ] \
    && grep -q '"kind":"filmstrip"' "$WORKDIR/visuals.json" \
    && grep -q '"kind":"waveform"' "$WORKDIR/visuals.json" \
    && grep -q '"content":"Hello timeline"' "$WORKDIR/visuals.json" \
    && grep -q '"effects":\["grayscale"\]' "$WORKDIR/visuals.json"; then
  pass "visuals-truthful-filmstrip-waveform-text-badges"
else
  fail "visuals-truthful-filmstrip-waveform-text-badges" "http $CODE: $(cat "$WORKDIR/visuals.json" 2>/dev/null)"
fi
# Break both media files mid-session: REC-005 named placeholder boxes.
rm -f "$PROJ/s.wav" "$PROJ/v.mp4"
CODE="$(api_get "${URL}api/studio/visuals" "$WORKDIR/visuals-broken.json")"
if [ "$CODE" = "200" ] \
    && grep -q '"kind":"placeholder","name":"s"' "$WORKDIR/visuals-broken.json" \
    && grep -q '"kind":"placeholder","name":"v"' "$WORKDIR/visuals-broken.json" \
    && grep -q 'is missing' "$WORKDIR/visuals-broken.json" \
    && grep -q '"content":"Hello timeline"' "$WORKDIR/visuals-broken.json"; then
  pass "visuals-name-placeholders-around-truthful-text"
else
  fail "visuals-name-placeholders-around-truthful-text" "http $CODE: $(cat "$WORKDIR/visuals-broken.json" 2>/dev/null)"
fi

# Media restored after the placeholder gate: the conflict pick below must
# validate a whole project, so the session heals before moving on.
python3 - "$PROJ/s.wav" <<'EOF'
import math, struct, sys
rate, secs = 48000, 4.0
frames = int(round(secs * rate))
pcm = b"".join(
  struct.pack("<h", int(math.sin(i * 440.0 * 2.0 * math.pi / rate) * 10000))
  for i in range(frames * 2)
)
with open(sys.argv[1], "wb") as f:
  f.write(b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt ")
  f.write(struct.pack("<IHHIIHH", 16, 1, 2, rate, rate * 4, 4, 16))
  f.write(b"data" + struct.pack("<I", len(pcm)) + pcm)
EOF
ffmpeg -y -v error -f lavfi -i "testsrc=duration=2:size=64x48:rate=10" \
  -c:v libx264 -pix_fmt yuv420p "$PROJ/v.mp4" 2>>"$WORKDIR/ffmpeg.log"
CODE="$(api_get "${URL}api/studio/visuals" "$WORKDIR/visuals-healed.json")"
if [ "$CODE" = "200" ] && grep -q '"kind":"filmstrip"' "$WORKDIR/visuals-healed.json" \
    && grep -q '"kind":"waveform"' "$WORKDIR/visuals-healed.json"; then
  pass "visuals-heal-when-media-returns"
else
  fail "visuals-heal-when-media-returns" "http $CODE: $(cat "$WORKDIR/visuals-healed.json" 2>/dev/null)"
fi

# ---- gate 5: remapped shortcut works, defaults stay underneath -----------
CODE="$(api_post "${URL}api/studio/shortcuts" '{"version":1,"bindings":{"tool-razor":"x"}}' "$WORKDIR/remap.json")"
if [ "$CODE" = "200" ] && grep -q '"restored":true' "$WORKDIR/remap.json"; then
  pass "project-remap-saves"
else
  fail "project-remap-saves" "http $CODE: $(cat "$WORKDIR/remap.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/shortcut" '{"chord":"X"}' "$WORKDIR/dispatch-x.json")"
if [ "$CODE" = "200" ] && grep -q '"action":"tool-razor"' "$WORKDIR/dispatch-x.json"; then
  pass "remapped-shortcut-acts-immediately"
else
  fail "remapped-shortcut-acts-immediately" "http $CODE: $(cat "$WORKDIR/dispatch-x.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/shortcuts" "$WORKDIR/shortcuts.json")"
if [ "$CODE" = "200" ] && grep -q '"defaults"' "$WORKDIR/shortcuts.json" \
    && grep -q '"chord":"b"' "$WORKDIR/shortcuts.json" \
    && grep -q '"action":"tool-razor"' "$WORKDIR/shortcuts.json"; then
  pass "default-map-stays-visible-underneath"
else
  fail "default-map-stays-visible-underneath" "http $CODE: $(cat "$WORKDIR/shortcuts.json" 2>/dev/null)"
fi
if [ -f "$PROJ/.framesmith/studio-shortcuts.json" ]; then
  pass "remap-lives-in-hidden-history-dir"
else
  fail "remap-lives-in-hidden-history-dir" "no studio-shortcuts.json under $PROJ/.framesmith"
fi

# ---- gate 6: a conflict resolves by pick with both versions kept ---------
CODE="$(api_post "${URL}api/studio/queue" '{"paused":true}' "$WORKDIR/queue-pause.json")"
[ "$CODE" = "200" ] && pass "queue-pauses" || fail "queue-pauses" "http $CODE"
CODE="$(api_post "${URL}api/studio/edit" '{"actor":"maker","source":"visual","edits":[{"op":"setlabel","target":{"id":"title","index":null},"label":"Staged"}]}' "$WORKDIR/staged.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":false' "$WORKDIR/staged.json"; then
  pass "staged-edit-held-unsaved"
else
  fail "staged-edit-held-unsaved" "http $CODE: $(cat "$WORKDIR/staged.json" 2>/dev/null)"
fi
# External change lands on disk while the staged edit waits.
printf 'composition name="studio" width=320 height=180 duration=4 fps=10 background="#112233" version=1 {\n    text id="title" value="Hello timeline" start=1 duration=2 x=10 y=40 size=24 color="#ffffff"\n    text id="bye" label="Disk" value="Second line" start=2 duration=2 x=10 y=80 size=24 color="#ffffff"\n    audio id="s" path="s.wav" start=0 duration=4 volume=-6 fade-in=0.5 fade-out=0.5\n    video id="v" path="v.mp4" start=1 duration=2 {\n        effect preset="grayscale"\n    }\n    transition from="title" to="bye" duration=0.5 kind="fade"\n    key target=0 property="x" {\n        frame at=0 value=10\n        frame at=1.5 value=60 ease="in-quad"\n        frame at=2 value=110\n    }\n    key target=0 property="opacity" {\n        frame at=0 value=0\n        frame at=2 value=1\n    }\n}\n' >"$PROJ/composition.kdl"
CODE="$(api_get "${URL}api/studio/conflict" "$WORKDIR/conflict.json")"
if [ "$CODE" = "200" ] && grep -q '"conflict":true' "$WORKDIR/conflict.json" \
    && grep -q 'changed on disk' "$WORKDIR/conflict.json" \
    && grep -q 'Staged' "$WORKDIR/conflict.json" \
    && grep -q '"staged":1' "$WORKDIR/conflict.json"; then
  pass "conflict-banners-disk-versus-unsaved"
else
  fail "conflict-banners-disk-versus-unsaved" "http $CODE: $(cat "$WORKDIR/conflict.json" 2>/dev/null)"
fi
# Editing pauses until the pick lands.
CODE="$(api_post "${URL}api/studio/edit" '{"actor":"maker","source":"visual","edits":[{"op":"setlabel","target":{"id":"title","index":null},"label":"Later"}]}' "$WORKDIR/conflict-refused.json")"
if [ "$CODE" = "409" ] && grep -q 'waiting for your pick' "$WORKDIR/conflict-refused.json"; then
  pass "editing-pauses-while-conflicted"
else
  fail "editing-pauses-while-conflicted" "http $CODE: $(cat "$WORKDIR/conflict-refused.json" 2>/dev/null)"
fi
# The maker keeps the unsaved version; the disk version rests in history.
CODE="$(api_post "${URL}api/studio/conflict" '{"pick":"unsaved","actor":"maker"}' "$WORKDIR/pick.json")"
if [ "$CODE" = "200" ] && grep -q '"resolved":true' "$WORKDIR/pick.json" \
    && grep -q '"pick":"unsaved"' "$WORKDIR/pick.json" \
    && grep -q 'disk version is kept in turn' "$WORKDIR/pick.json"; then
  pass "pick-unsaved-resolves-keeping-disk"
else
  fail "pick-unsaved-resolves-keeping-disk" "http $CODE: $(cat "$WORKDIR/pick.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/timeline" "$WORKDIR/rows-resolved.json")"
if [ "$CODE" = "200" ] && grep -q '"label":"Staged"' "$WORKDIR/rows-resolved.json" \
    && grep -q '"label":"Disk"' "$WORKDIR/rows-resolved.json"; then
  pass "both-versions-survive-the-pick"
else
  fail "both-versions-survive-the-pick" "http $CODE: $(cat "$WORKDIR/rows-resolved.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/conflict" "$WORKDIR/conflict-clear.json")"
if [ "$CODE" = "200" ] && grep -q '"conflict":false' "$WORKDIR/conflict-clear.json"; then
  pass "conflict-clears-after-pick"
else
  fail "conflict-clears-after-pick" "http $CODE: $(cat "$WORKDIR/conflict-clear.json" 2>/dev/null)"
fi

# ---- tamper negative: a corrupted composition must refuse ----------------
TAMPER="$WORKDIR/tampered"
rm -rf "$TAMPER"
cp -r "$PROJ" "$TAMPER"
printf 'composition name="studio" width=320 height=180 duration=6 fps=10 background="not-a-color" version=1 {\n}\n' >"$TAMPER/composition.kdl"
if "$FSMITH" check "$TAMPER" >"$WORKDIR/tamper.log" 2>&1; then
  fail "tamper-refused-non-zero" "check exited 0 on a corrupted project"
else
  pass "tamper-refused-non-zero (exit $?)"
fi

# ---- evidence -------------------------------------------------------------
echo ""
echo "--- proof summary: $PASS_N passed, failing:${FAIL_LIST:- none} ---"
{
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
  echo "cargo: $(cargo --version 2>/dev/null)"
  echo "rustc: $(rustc --version 2>/dev/null)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "url: ${URL:-none}"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
  echo "browser: none on this machine; /studio page asserted over HTTP, full HTML kept as evidence"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/tamper.log" "$WORKDIR/ffmpeg.log" "$EVID_DIR/" 2>/dev/null || true
for F in theme0.json theme1.json layout-save.json layout-get.json studio-dark.html \
    layout-proj2.json theme-proj2.json tool-move.json rows-moved.json \
    key-retime.json lanes.json beat-add.json beat-add2.json beat-del.json \
    beat-undo.json beats.json rows-after-beats.json visuals.json \
    visuals-broken.json visuals-healed.json remap.json dispatch-x.json shortcuts.json \
    queue-pause.json staged.json conflict.json conflict-refused.json \
    pick.json rows-resolved.json conflict-clear.json; do
  cp "$WORKDIR/$F" "$EVID_DIR/" 2>/dev/null || true
done
cp "$EXAMPLE_DIR/composition.kdl" "$EVID_DIR/composition-reference.kdl" 2>/dev/null || true
cp "$EXAMPLE_DIR/README.md" "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
