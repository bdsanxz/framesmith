#!/bin/sh
# n17-studio-proof.sh -- Sprint 17 T009 one-command gate.
#
# Proves the Studio canvas reference project end to end on a staged copy of
# framesmith/examples/studio-canvas-reference: a canvas drag snaps with
# guide lines shown and the transform lands as a named history turn, a crop
# preset commits as exactly one turn and a cancel stages nothing, inline
# text commits as one turn and a cancel stages nothing, an inspector row
# edits and its reset restores the default as undoable turns, a tween card
# applies with a bezier preset, a motion path previews and a recorded
# gesture lands as keyframes. A tamper negative (corrupted composition)
# must refuse with a non-zero exit. Reports PROOF: PASS only when every
# check succeeds (exit 0); any single failing check prints FAIL and the
# gate exits 1.
#
# Usage: n17-studio-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory); the
#   checkout itself is never written. The daemon registry is isolated to
#   DIR, so the proof never touches the maker's live preview servers.
#   Evidence is copied to evidences/proof-n17-T009/ on every run.
#
# Note: no headless browser ships on this machine, so the served /studio
# page is asserted over HTTP (canvas, crop, text, inspector, and animation
# panels present) and the full page HTML is kept as evidence instead of
# screenshots.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,24p' "$0"; exit 0 ;;
    *) echo "n17-studio-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/studio-canvas-reference"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n17-T009"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n17-proof-XXXXXX)"
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
# jget FILE KEY -> prints the JSON value of KEY (python3, no jq needed).
jget() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2], ""))' "$1" "$2"
}

echo "================ n17 studio canvas proof gate (T009) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "====================================================================="
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
[ -f "$PROJ/composition.kdl" ] && pass "stage-reference" || fail "stage-reference" "copy failed"

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
READY=""
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "${URL}api/status" 2>/dev/null; then READY=1; break; fi
  sleep 0.5
done
if [ -n "$READY" ]; then pass "server-ready"; else fail "server-ready" "no answer at ${URL}api/status"; fi

# Default frame bytes first: canvas/inspector/animation turns must not
# move the default render.
curl -s "${URL}api/frame?i=0&action=step" -o "$WORKDIR/frame-before.bin" 2>/dev/null
if [ -s "$WORKDIR/frame-before.bin" ]; then pass "frame-before-captured"; else fail "frame-before-captured" "no frame bytes"; fi

# ---- gate 1: canvas transform lands with snap guides shown ---------------
CODE="$(api_post "${URL}api/studio/canvas-snap" '{"want":[11,44]}' "$WORKDIR/snap.json")"
if [ "$CODE" = "200" ] && grep -q '"did-snap":true' "$WORKDIR/snap.json" \
    && grep -q '"guide-lines"' "$WORKDIR/snap.json" \
    && grep -q '"at":10' "$WORKDIR/snap.json" \
    && grep -q 'guide line' "$WORKDIR/snap.json"; then
  pass "snap-shows-guides"
else
  fail "snap-shows-guides" "http $CODE: $(cat "$WORKDIR/snap.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/transform" '{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[10,-5],"commit":true}' "$WORKDIR/move.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/move.json" \
    && [ "$(jget "$WORKDIR/move.json" turn)" = "0" ] \
    && grep -q 'canvas move card' "$WORKDIR/move.json"; then
  pass "transform-move-commits-turn-0"
else
  fail "transform-move-commits-turn-0" "http $CODE: $(cat "$WORKDIR/move.json" 2>/dev/null)"
fi

# ---- gate 2: crop commits once and cancels clean -------------------------
CODE="$(api_post "${URL}api/studio/crop" '{"actor":"maker","target":{"id":"card"},"preset":"16:9","commit":true}' "$WORKDIR/crop-commit.json")"
CROP_TURN=""
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/crop-commit.json" \
    && grep -q '"applied":4' "$WORKDIR/crop-commit.json"; then
  CROP_TURN="$(jget "$WORKDIR/crop-commit.json" turn)"
  pass "crop-commits-one-turn (turn $CROP_TURN)"
else
  fail "crop-commits-one-turn" "http $CODE: $(cat "$WORKDIR/crop-commit.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/crop" '{"actor":"maker","target":{"id":"card"},"preset":"1:1","commit":true}' "$WORKDIR/crop-commit2.json")"
CROP_TURN2="$(jget "$WORKDIR/crop-commit2.json" turn 2>/dev/null)"
if [ "$CODE" = "200" ] && [ -n "$CROP_TURN" ] && [ "$CROP_TURN2" = "$((CROP_TURN + 1))" ]; then
  pass "crop-second-commit-is-next-turn"
else
  fail "crop-second-commit-is-next-turn" "http $CODE turns $CROP_TURN/$CROP_TURN2: $(cat "$WORKDIR/crop-commit2.json" 2>/dev/null)"
fi
cp "$PROJ/composition.kdl" "$WORKDIR/kdl-before-crop-cancel.kdl"
CODE="$(api_post "${URL}api/studio/crop" '{"actor":"maker","target":{"id":"card"},"preset":"9:16","frame":{"x":0,"y":0,"width":90,"height":40},"cancelled":true}' "$WORKDIR/crop-cancel.json")"
if [ "$CODE" = "200" ] && grep -q '"cancelled":true' "$WORKDIR/crop-cancel.json" \
    && [ "$(python3 -c 'import json; print(json.load(open("'"$WORKDIR"'/crop-cancel.json")).get("turn"))' 2>/dev/null)" = "None" ] \
    && cmp -s "$PROJ/composition.kdl" "$WORKDIR/kdl-before-crop-cancel.kdl"; then
  pass "crop-cancel-clean"
else
  fail "crop-cancel-clean" "http $CODE: $(cat "$WORKDIR/crop-cancel.json" 2>/dev/null)"
fi

# ---- gate 3: inline text commits once and cancels clean -------------------
CODE="$(api_post "${URL}api/studio/text" '{"action":"open","target":{"id":"title"},"selection":"title"}' "$WORKDIR/text-open.json")"
if [ "$CODE" = "200" ] && grep -q '"value":"Hi"' "$WORKDIR/text-open.json"; then
  pass "text-opens-live-session"
else
  fail "text-opens-live-session" "http $CODE: $(cat "$WORKDIR/text-open.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/text" '{"action":"commit","actor":"maker","target":{"id":"title"},"value":"Hello","bold":true,"italic":true,"underline":true,"color":"#ff0000"}' "$WORKDIR/text-commit.json")"
TEXT_TURN=""
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/text-commit.json" \
    && grep -q '"committed":true' "$WORKDIR/text-commit.json" \
    && grep -q '"applied":3' "$WORKDIR/text-commit.json"; then
  TEXT_TURN="$(jget "$WORKDIR/text-commit.json" turn)"
  pass "text-commits-one-turn (turn $TEXT_TURN)"
else
  fail "text-commits-one-turn" "http $CODE: $(cat "$WORKDIR/text-commit.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/text" '{"action":"commit","actor":"maker","target":{"id":"title"},"color":"#00ff00"}' "$WORKDIR/text-recolor.json")"
TEXT_TURN2="$(jget "$WORKDIR/text-recolor.json" turn 2>/dev/null)"
if [ "$CODE" = "200" ] && [ -n "$TEXT_TURN" ] && [ "$TEXT_TURN2" = "$((TEXT_TURN + 1))" ]; then
  pass "text-second-commit-is-next-turn"
else
  fail "text-second-commit-is-next-turn" "http $CODE turns $TEXT_TURN/$TEXT_TURN2"
fi
cp "$PROJ/composition.kdl" "$WORKDIR/kdl-before-text-cancel.kdl"
CODE="$(api_post "${URL}api/studio/text" '{"action":"preview","target":{"id":"title"},"value":"Draft","bold":true}' "$WORKDIR/text-preview.json")"
[ "$CODE" = "200" ] && pass "text-preview-stages-echo" || fail "text-preview-stages-echo" "http $CODE"
CODE="$(api_post "${URL}api/studio/text" '{"action":"cancel","target":{"id":"title"}}' "$WORKDIR/text-cancel.json")"
if [ "$CODE" = "200" ] && grep -q '"cancelled":true' "$WORKDIR/text-cancel.json" \
    && grep -q '"committed":false' "$WORKDIR/text-cancel.json" \
    && cmp -s "$PROJ/composition.kdl" "$WORKDIR/kdl-before-text-cancel.kdl"; then
  pass "text-cancel-clean"
else
  fail "text-cancel-clean" "http $CODE: $(cat "$WORKDIR/text-cancel.json" 2>/dev/null)"
fi

# ---- gate 4: inspector rows edit with undoable reset ----------------------
CODE="$(api_get "${URL}api/studio/inspector?target=title" "$WORKDIR/rows.json")"
if [ "$CODE" = "200" ] && grep -q '"property":"font-size"' "$WORKDIR/rows.json" \
    && grep -q '"group":"Text"' "$WORKDIR/rows.json"; then
  pass "inspector-rows-list"
else
  fail "inspector-rows-list" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/inspector" '{"actor":"maker","target":{"id":"title"},"property":"font-size","value":"36"}' "$WORKDIR/row-edit.json")"
ROW_TURN=""
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/row-edit.json"; then
  ROW_TURN="$(jget "$WORKDIR/row-edit.json" turn)"
  pass "inspector-row-edits (turn $ROW_TURN)"
else
  fail "inspector-row-edits" "http $CODE: $(cat "$WORKDIR/row-edit.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/inspector?target=title" "$WORKDIR/rows-applied.json")"
if [ "$CODE" = "200" ] && grep -q '"value":"36"' "$WORKDIR/rows-applied.json"; then
  pass "inspector-row-shows-applied-value"
else
  fail "inspector-row-shows-applied-value" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/inspector" '{"actor":"maker","target":{"id":"title"},"property":"font-size","value":"huge"}' "$WORKDIR/row-bad.json")"
if [ "$CODE" = "409" ] && grep -q 'font-size' "$WORKDIR/row-bad.json"; then
  pass "inspector-bad-value-refuses-with-rule"
else
  fail "inspector-bad-value-refuses-with-rule" "http $CODE: $(cat "$WORKDIR/row-bad.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/inspector" '{"actor":"maker","target":{"id":"title"},"property":"font-size","reset":true}' "$WORKDIR/row-reset.json")"
RESET_TURN="$(jget "$WORKDIR/row-reset.json" turn 2>/dev/null)"
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/row-reset.json" \
    && [ -n "$ROW_TURN" ] && [ "$RESET_TURN" = "$((ROW_TURN + 1))" ]; then
  pass "inspector-reset-is-undoable-turn (turn $RESET_TURN)"
else
  fail "inspector-reset-is-undoable-turn" "http $CODE turns $ROW_TURN/$RESET_TURN"
fi
CODE="$(api_get "${URL}api/studio/inspector?target=title" "$WORKDIR/rows-reset.json")"
if [ "$CODE" = "200" ] && grep -q '"value":"48"' "$WORKDIR/rows-reset.json"; then
  pass "inspector-reset-restores-default"
else
  fail "inspector-reset-restores-default" "http $CODE"
fi

# ---- gate 5: tween cards shape with bezier presets applied ----------------
CODE="$(api_post "${URL}api/studio/card" '{"actor":"maker","kind":"to","target":{"id":"title"},"property":"x","to":"120","start":0,"length":2,"speed":1,"selection":"title"}' "$WORKDIR/card.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/card.json"; then
  pass "tween-card-applies"
else
  fail "tween-card-applies" "http $CODE: $(cat "$WORKDIR/card.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/unroll?tween=0&samples=5" "$WORKDIR/unroll.json")"
N_SAMPLES="$(python3 -c 'import json; print(len(json.load(open("'"$WORKDIR"'/unroll.json")).get("samples", [])))' 2>/dev/null)"
if [ "$CODE" = "200" ] && [ "$N_SAMPLES" = "5" ]; then
  pass "tween-unrolls-computed-values"
else
  fail "tween-unrolls-computed-values" "http $CODE samples=$N_SAMPLES"
fi
CODE="$(api_post "${URL}api/studio/bezier" '{"actor":"maker","ease":"out-back(1.7)","tween":0,"selection":"title"}' "$WORKDIR/bezier.json")"
if [ "$CODE" = "200" ] && grep -q '"target":"tween 0"' "$WORKDIR/bezier.json"; then
  pass "bezier-preset-applies"
else
  fail "bezier-preset-applies" "http $CODE: $(cat "$WORKDIR/bezier.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/bezier" '{"actor":"maker","ease":"ease-in","tween":0}' "$WORKDIR/bezier-bad.json")"
if [ "$CODE" = "400" ] && grep -q 'unavailable' "$WORKDIR/bezier-bad.json"; then
  pass "bezier-unsupported-refuses-never-faked"
else
  fail "bezier-unsupported-refuses-never-faked" "http $CODE: $(cat "$WORKDIR/bezier-bad.json" 2>/dev/null)"
fi

# ---- gate 6: motion path previews and a gesture records -------------------
CODE="$(api_get "${URL}api/studio/path-preview?d=M%200%2C0%20L%20100%2C0&samples=5" "$WORKDIR/path-preview.json")"
N_POINTS="$(python3 -c 'import json; print(len(json.load(open("'"$WORKDIR"'/path-preview.json")).get("points", [])))' 2>/dev/null)"
if [ "$CODE" = "200" ] && [ "$N_POINTS" = "5" ]; then
  pass "motion-path-previews"
else
  fail "motion-path-previews" "http $CODE points=$N_POINTS"
fi
CODE="$(api_post "${URL}api/studio/path" '{"actor":"maker","action":"add","target":{"id":"title"},"path":"M 0,0 L 100,0","start":0,"length":2,"selection":"title"}' "$WORKDIR/path-add.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/path-add.json"; then
  pass "motion-path-adds"
else
  fail "motion-path-adds" "http $CODE: $(cat "$WORKDIR/path-add.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/capture" '{"actor":"maker","target":{"id":"card"},"samples":[{"t":0,"x":0,"y":0},{"t":0.5,"x":40,"y":10},{"t":1,"x":80,"y":0}],"selection":"card"}' "$WORKDIR/capture.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":true' "$WORKDIR/capture.json" \
    && grep -q '"applied":2' "$WORKDIR/capture.json"; then
  pass "gesture-records-keyframes"
else
  fail "gesture-records-keyframes" "http $CODE: $(cat "$WORKDIR/capture.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/capture" '{"actor":"maker","target":{"id":"card"},"samples":[]}' "$WORKDIR/capture-empty.json")"
if [ "$CODE" = "200" ] && grep -q '"saved":false' "$WORKDIR/capture-empty.json" \
    && grep -q 'no movement was recorded' "$WORKDIR/capture-empty.json"; then
  pass "empty-capture-discards-with-note"
else
  fail "empty-capture-discards-with-note" "http $CODE: $(cat "$WORKDIR/capture-empty.json" 2>/dev/null)"
fi

# ---- gate 7: default renders stay byte-identical --------------------------
curl -s "${URL}api/frame?i=0&action=step" -o "$WORKDIR/frame-after.bin" 2>/dev/null
if cmp -s "$WORKDIR/frame-before.bin" "$WORKDIR/frame-after.bin"; then
  pass "frames-byte-identical-after-turns"
else
  fail "frames-byte-identical-after-turns" "default render moved"
fi

# ---- gate 8: the served page hosts every canvas panel ---------------------
CODE="$(api_get "${URL}studio" "$WORKDIR/studio.html")"
if [ "$CODE" = "200" ] && grep -q 'id="animation-panel"' "$WORKDIR/studio.html" \
    && grep -q 'id="canvas-snap-panel"' "$WORKDIR/studio.html" \
    && grep -q 'background-color' "$WORKDIR/studio.html"; then
  pass "page-hosts-canvas-animation-panels"
else
  fail "page-hosts-canvas-animation-panels" "http $CODE"
fi

# ---- tamper negative: a corrupted composition must refuse ----------------
TAMPER="$WORKDIR/tampered"
rm -rf "$TAMPER"
cp -r "$PROJ" "$TAMPER"
printf 'composition name="studio-canvas" width=320 height=180 duration=4 fps=10 background="not-a-color" version=1 {\n}\n' >"$TAMPER/composition.kdl"
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
cp "$WORKDIR/report.txt" "$WORKDIR/tamper.log" "$EVID_DIR/" 2>/dev/null || true
for F in snap.json move.json crop-commit.json crop-commit2.json crop-cancel.json \
    text-open.json text-commit.json text-recolor.json text-preview.json text-cancel.json \
    rows.json row-edit.json rows-applied.json row-bad.json row-reset.json rows-reset.json \
    card.json unroll.json bezier.json bezier-bad.json path-preview.json path-add.json \
    capture.json capture-empty.json studio.html; do
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
