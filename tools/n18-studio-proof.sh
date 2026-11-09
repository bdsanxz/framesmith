#!/bin/sh
# n18-studio-proof.sh -- Sprint 18 T009 one-command gate.
#
# Proves the Studio panels reference project end to end on staged
# copies of framesmith/examples/studio-panels-reference (plus a
# generated sibling project and generated media): all five panels act
# with receipts, Layers restructures land as history turns, the audio
# rack rewrites lanes exactly, a cross-project paste lands renamed
# with styles carried, a queued render downloads byte-identical and a
# GIF loops as chosen. A tamper negative (corrupted composition) must
# refuse with a non-zero exit. Reports PROOF: PASS only when every
# check succeeds (exit 0); any single failing check prints FAIL and
# the gate exits 1.
#
# The audio rack and the cross-project style check are library-only
# surfaces (no HTTP routes), so the script drives them through the
# crates/preview/examples/n18_audio_probe helper built from this same
# checkout; everything else goes over the real served HTTP routes.
#
# Usage: n18-studio-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory);
#   the checkout itself is never written. The daemon registry is
#   isolated to DIR, so the proof never touches the maker's live
#   preview servers. Evidence is copied to
#   evidences/proof-n18-T009/ on every run.
#
# Note: no headless browser ships on this machine, so the served
# /studio page is asserted over HTTP (every panel slot present) and
# the full page HTML is kept as evidence instead of screenshots.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n18-studio-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/studio-panels-reference"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n18-T009"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n18-proof-XXXXXX)"
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
api_post_file() {
  # $1=url $2=bodyfile $3=outfile -> prints HTTP code
  curl -s -o "$3" -X POST --data-binary "@$2" "$1" -w "%{http_code}"
}
# jget FILE KEY -> prints the JSON value of KEY (python3, no jq needed).
jget() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2], ""))' "$1" "$2"
}

echo "================ n18 studio panels proof gate (T009) ================"
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

echo "--- build audio probe helper ---"
if (cd "$FRAMESMITH_DIR" && cargo build -p preview --example n18_audio_probe >"$WORKDIR/build-probe.log" 2>&1); then
  echo "probe build ok"
else
  fail "probe-build" "cargo build of n18_audio_probe failed; see $WORKDIR/build-probe.log"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
PROBE="$FRAMESMITH_DIR/target/debug/examples/n18_audio_probe"
FFMPEG="$(command -v ffmpeg)"

# ---- stage the reference: the checkout is never served or rendered into --
PROJ="$WORKDIR/proj"
PROJ2="$WORKDIR/sibling"
DROP="$WORKDIR/drop"
rm -rf "$PROJ" "$PROJ2" "$DROP"
mkdir -p "$PROJ/media" "$PROJ/fonts" "$PROJ2" "$DROP"
cp "$EXAMPLE_DIR/composition.kdl" "$EXAMPLE_DIR/README.md" "$PROJ/"
[ -f "$PROJ/composition.kdl" ] && pass "stage-reference" || fail "stage-reference" "copy failed"
printf 'composition name="b" root=true width=320 height=180 duration=8 fps=10 background="#223344" version=1 {\n}\n' >"$PROJ2/composition.kdl"
[ -f "$PROJ2/composition.kdl" ] && pass "stage-sibling" || fail "stage-sibling" "sibling copy failed"

# Generated media: a real 2 s stereo sine WAV plus real PNGs so the
# served session decodes; inert placeholders for the remove/rename and
# refusal gates.
python3 - "$PROJ/s.wav" <<'EOF'
import struct, math, sys
rate = 48000; secs = 2.0; frames = int(rate * secs)
data = bytearray()
for i in range(frames * 2):
    s = int(math.sin(i * 440.0 * 2 * math.pi / rate) * 10000)
    data += struct.pack('<h', s)
hdr = struct.pack('<4sI4s4sIHHIIHH4sI', b'RIFF', 36 + len(data), b'WAVE',
                  b'fmt ', 16, 1, 2, rate, rate * 4, 4, 16, b'data', len(data))
open(sys.argv[1], 'wb').write(hdr + bytes(data))
EOF
[ -s "$PROJ/s.wav" ] && pass "stage-sine-wav" || fail "stage-sine-wav" "wav generation failed"
ffmpeg -y -v error -f lavfi -i testsrc=size=320x180:rate=10:duration=1 -frames:v 1 "$PROJ/media/card.png" \
  && ffmpeg -y -v error -f lavfi -i testsrc=size=320x180:rate=10:duration=1 -frames:v 1 "$PROJ/media/extra.png" \
  && pass "stage-testsrc-pngs" || fail "stage-testsrc-pngs" "ffmpeg png generation failed"
printf 'unused-mp3-bytes' >"$PROJ/media/idle.mp3"
printf 'unused-png-bytes' >"$PROJ/media/spare.png"
printf 'unused-font-bytes' >"$PROJ/fonts/Body.ttf"
printf '\x89PNG\r\n\x1a\nfresh' >"$DROP/fresh.png"
printf 'nope' >"$DROP/run.exe"
[ -s "$PROJ/media/card.png" ] && pass "stage-placeholders" || fail "stage-placeholders" "placeholder staging failed"

# The staged project must validate clean and lint clean: render-one
# refuses when lint findings exist, so this gates the fixture itself.
if "$FSMITH" check "$PROJ" >"$WORKDIR/check.log" 2>&1; then
  pass "staged-project-validates"
else
  fail "staged-project-validates" "check failed; see evidence check.log"
fi
if "$FSMITH" lint "$PROJ" >"$WORKDIR/lint.log" 2>&1; then
  if grep -q "0 findings" "$WORKDIR/lint.log"; then
    pass "staged-project-lint-clean"
  else
    fail "staged-project-lint-clean" "$(cat "$WORKDIR/lint.log")"
  fi
else
  fail "staged-project-lint-clean" "lint exited non-zero"
fi

cleanup() {
  export FRAMESMITH_PREVIEW_REGISTRY="$WORKDIR/registry.json"
  "$FSMITH" preview-stop-all >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

"$FSMITH" preview-stop-all >/dev/null 2>&1 || true

# ---- servers: the reference and its cross-project paste sibling -------
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
START2_OUT="$("$FSMITH" preview "$PROJ2" 2>&1)"
URL2="$(echo "$START2_OUT" | grep -o 'http://127\.0\.0\.1:[0-9]*/' | head -n 1)"
if [ -z "$URL2" ]; then
  fail "preview-sibling-url" "no loopback URL in: $START2_OUT"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
pass "preview-sibling-start ($URL2)"
READY=""
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "${URL}api/status" 2>/dev/null && curl -s -o /dev/null "${URL2}api/status" 2>/dev/null; then READY=1; break; fi
  sleep 0.5
done
if [ -n "$READY" ]; then pass "servers-ready"; else fail "servers-ready" "no answer at ${URL}api/status or ${URL2}api/status"; fi

# Every mutating panel receipt below must chain the shared history:
# each recorded turn is exactly one past the previous one.
EXPECT_TURN=0
expect_turn() {
  # $1=receipt-file $2=gate-name -> checks turn == EXPECT_TURN, then advances
  GOT="$(jget "$1" turn)"
  if [ "$GOT" = "$EXPECT_TURN" ]; then
    pass "$2 (turn $GOT)"
  else
    fail "$2" "expected turn $EXPECT_TURN, got '$GOT': $(cat "$1" 2>/dev/null)"
  fi
  EXPECT_TURN=$((EXPECT_TURN + 1))
}

cp "$PROJ/composition.kdl" "$WORKDIR/kdl-pristine.kdl"

# ---- gate 1: compositions panel acts ------------------------------------
CODE="$(api_get "${URL}api/studio/compositions" "$WORKDIR/cards.json")"
CARDS="$(python3 -c 'import json; d=json.load(open("'"$WORKDIR"'/cards.json")); print(" ".join(c["name"] for c in d["cards"]))' 2>/dev/null)"
GEN="$(jget "$WORKDIR/cards.json" generation)"
LINT0="$(python3 -c 'import json; d=json.load(open("'"$WORKDIR"'/cards.json")); print(all(c["lint"]==0 for c in d["cards"]))' 2>/dev/null)"
if [ "$CODE" = "200" ] && [ "$CARDS" = "main b-roll" ] && [ -n "$GEN" ] && [ "$LINT0" = "True" ]; then
  pass "compositions-list-two-clean-cards"
else
  fail "compositions-list-two-clean-cards" "http $CODE cards='$CARDS' lint-clean=$LINT0"
fi
curl -s "${URL}api/studio/compositions/preview?composition=b-roll" -o "$WORKDIR/hover.png" 2>/dev/null
if python3 -c 'import sys; sys.exit(0 if open("'"$WORKDIR"'/hover.png","rb").read(8)==b"\x89PNG\r\n\x1a\n" else 1)' 2>/dev/null; then
  pass "compositions-hover-preview-is-png"
else
  fail "compositions-hover-preview-is-png" "no PNG bytes for b-roll"
fi
CODE="$(api_get "${URL}api/studio/compositions/preview?composition=nope" "$WORKDIR/hover-missing.txt")"
if [ "$CODE" = "404" ]; then
  pass "compositions-unknown-preview-refuses"
else
  fail "compositions-unknown-preview-refuses" "http $CODE"
fi

# ---- gate 2: layers restructure lands as history turns -----------------
layer_keys() {
  # $1=outfile -> prints space-separated layer keys
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(" ".join(l["key"] for l in d["layers"]))' "$1"
}
CODE="$(api_get "${URL}api/studio/layers" "$WORKDIR/layers-before.json")"
if [ "$CODE" = "200" ] && [ "$(layer_keys "$WORKDIR/layers-before.json")" = "hero pic card badge music" ]; then
  pass "layers-list-document-order"
else
  fail "layers-list-document-order" "http $CODE keys='$(layer_keys "$WORKDIR/layers-before.json" 2>/dev/null)'"
fi
CODE="$(api_post "${URL}api/studio/layers" '{"actor":"maker","action":"reorder","key":"pic","to":2}' "$WORKDIR/reorder.json")"
if [ "$CODE" = "200" ] && grep -q 'move clip id \\"pic\\" to index 2' "$WORKDIR/reorder.json"; then
  expect_turn "$WORKDIR/reorder.json" "layers-reorder-commits"
else
  fail "layers-reorder-commits" "http $CODE: $(cat "$WORKDIR/reorder.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi
CODE="$(api_get "${URL}api/studio/layers" "$WORKDIR/layers-reordered.json")"
if [ "$CODE" = "200" ] && [ "$(layer_keys "$WORKDIR/layers-reordered.json")" = "hero card pic badge music" ]; then
  pass "layers-reorder-moves-draw-order"
else
  fail "layers-reorder-moves-draw-order" "http $CODE keys='$(layer_keys "$WORKDIR/layers-reordered.json" 2>/dev/null)'"
fi
if cmp -s "$PROJ/composition.kdl" "$WORKDIR/kdl-pristine.kdl"; then
  pass "layers-reorder-keeps-sources-byte-identical"
else
  fail "layers-reorder-keeps-sources-byte-identical" "composition.kdl moved"
fi
CODE="$(api_post "${URL}api/studio/layers" '{"actor":"maker","action":"group","keys":["card","badge"],"name":"promo"}' "$WORKDIR/group.json")"
if [ "$CODE" = "200" ] && grep -q 'group clip id \\"card\\", clip id \\"badge\\" as \\"promo\\"' "$WORKDIR/group.json"; then
  expect_turn "$WORKDIR/group.json" "layers-group-collects"
else
  fail "layers-group-collects" "http $CODE: $(cat "$WORKDIR/group.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi
CODE="$(api_get "${URL}api/studio/layers" "$WORKDIR/layers-grouped.json")"
if [ "$CODE" = "200" ] && grep -q '"groups":\[{"name":"promo","members":\["card","badge"\]}\]' "$WORKDIR/layers-grouped.json" \
    && grep -q '"status-line":"[^"]*1 group(s)' "$WORKDIR/layers-grouped.json"; then
  pass "layers-group-lists-promo"
else
  fail "layers-group-lists-promo" "http $CODE: $(cat "$WORKDIR/layers-grouped.json" 2>/dev/null)"
fi
if cmp -s "$PROJ/composition.kdl" "$WORKDIR/kdl-pristine.kdl"; then
  pass "layers-group-keeps-sources-byte-identical"
else
  fail "layers-group-keeps-sources-byte-identical" "composition.kdl moved"
fi
CODE="$(api_post "${URL}api/studio/layers" '{"actor":"maker","action":"ungroup","name":"promo"}' "$WORKDIR/ungroup.json")"
if [ "$CODE" = "200" ] && grep -q 'ungroup \\"promo\\"' "$WORKDIR/ungroup.json"; then
  expect_turn "$WORKDIR/ungroup.json" "layers-ungroup-restores"
else
  fail "layers-ungroup-restores" "http $CODE: $(cat "$WORKDIR/ungroup.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi
CODE="$(api_get "${URL}api/studio/layers" "$WORKDIR/layers-ungrouped.json")"
if [ "$CODE" = "200" ] && grep -q '"groups":\[\]' "$WORKDIR/layers-ungrouped.json"; then
  pass "layers-ungroup-clears-groups"
else
  fail "layers-ungroup-clears-groups" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/layers" '{"actor":"maker","action":"ungroup","name":"nope"}' "$WORKDIR/ungroup-missing.txt")"
if [ "$CODE" = "404" ] && grep -q 'names no grouped members' "$WORKDIR/ungroup-missing.txt"; then
  pass "layers-unknown-ungroup-refuses"
else
  fail "layers-unknown-ungroup-refuses" "http $CODE"
fi

# ---- gate 3: assets panel acts ------------------------------------------
CODE="$(api_get "${URL}api/studio/assets" "$WORKDIR/assets.json")"
CARD_USED="$(python3 -c 'import json; d=json.load(open("'"$WORKDIR"'/assets.json")); c=[a for a in d["assets"] if a["name"]=="media/card.png"][0]; print(c["type"], c["used"], "pic" in " ".join(c.get("used-by",[])))' 2>/dev/null)"
if [ "$CODE" = "200" ] && [ "$CARD_USED" = "image True True" ]; then
  pass "assets-list-marks-card-used-by-pic"
else
  fail "assets-list-marks-card-used-by-pic" "http $CODE card='$CARD_USED'"
fi
CODE="$(api_get "${URL}api/studio/assets?type=image" "$WORKDIR/assets-images.json")"
if [ "$CODE" = "200" ] && [ "$(python3 -c 'import json; print(len(json.load(open("'"$WORKDIR"'/assets-images.json"))["assets"]))' 2>/dev/null)" = "3" ]; then
  pass "assets-type-filter-lists-three-images"
else
  fail "assets-type-filter-lists-three-images" "http $CODE"
fi
CODE="$(api_get "${URL}api/studio/assets?used=unused" "$WORKDIR/assets-unused.json")"
if [ "$CODE" = "200" ] && grep -q 'media/idle.mp3' "$WORKDIR/assets-unused.json"; then
  pass "assets-unused-filter-names-idle"
else
  fail "assets-unused-filter-names-idle" "http $CODE"
fi
CODE="$(api_get "${URL}api/studio/assets?type=nope" "$WORKDIR/assets-badtype.txt")"
if [ "$CODE" = "400" ] && grep -q 'unknown asset `type`' "$WORKDIR/assets-badtype.txt"; then
  pass "assets-unknown-type-refuses"
else
  fail "assets-unknown-type-refuses" "http $CODE"
fi
CODE="$(api_get "${URL}api/studio/assets?scope=all-projects" "$WORKDIR/assets-all.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/assets-all.json" scope)" = "all-projects" ] \
    && [ "$(python3 -c 'import json; print(len(json.load(open("'"$WORKDIR"'/assets-all.json"))["projects"]))' 2>/dev/null)" = "2" ]; then
  pass "assets-all-projects-lists-both"
else
  fail "assets-all-projects-lists-both" "http $CODE"
fi
DROPSRC="$(python3 -c 'import json; print(json.dumps("'"$DROP"'/fresh.png"))')"
CODE="$(api_post "${URL}api/studio/assets/import" "{\"source\":$DROPSRC}" "$WORKDIR/import.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/import.json" stored-name)" = "media/fresh.png" ] \
    && [ "$(jget "$WORKDIR/import.json" duplicate)" = "False" ] && [ -f "$PROJ/media/fresh.png" ]; then
  pass "assets-import-stores-fresh-png"
else
  fail "assets-import-stores-fresh-png" "http $CODE: $(cat "$WORKDIR/import.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/assets/import" "{\"source\":$DROPSRC}" "$WORKDIR/import-dup.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/import-dup.json" duplicate)" = "True" ]; then
  pass "assets-reimport-reports-duplicate"
else
  fail "assets-reimport-reports-duplicate" "http $CODE"
fi
BADEXE="$(python3 -c 'import json; print(json.dumps("'"$DROP"'/run.exe"))')"
CODE="$(api_post "${URL}api/studio/assets/import" "{\"source\":$BADEXE}" "$WORKDIR/import-bad.txt")"
if [ "$CODE" = "409" ] && grep -q 'accepted types are' "$WORKDIR/import-bad.txt"; then
  pass "assets-unsupported-import-refuses"
else
  fail "assets-unsupported-import-refuses" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/assets" '{"action":"insert","asset":"media/extra.png","actor":"maker"}' "$WORKDIR/insert.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/insert.json" inserted)" = "True" ]; then
  expect_turn "$WORKDIR/insert.json" "assets-insert-lands-as-turn"
else
  fail "assets-insert-lands-as-turn" "http $CODE: $(cat "$WORKDIR/insert.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi
CODE="$(api_post "${URL}api/studio/assets" '{"action":"reveal","asset":"media/card.png"}' "$WORKDIR/reveal.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/reveal.json" path)" = "media/card.png" ]; then
  pass "assets-reveal-reports-path"
else
  fail "assets-reveal-reports-path" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/assets" '{"action":"remove","asset":"media/card.png"}' "$WORKDIR/remove-used.txt")"
if [ "$CODE" = "409" ] && grep -q 'still referenced' "$WORKDIR/remove-used.txt" && grep -q 'pic' "$WORKDIR/remove-used.txt" \
    && [ -f "$PROJ/media/card.png" ]; then
  pass "assets-referenced-remove-refuses-naming-pic"
else
  fail "assets-referenced-remove-refuses-naming-pic" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/assets" '{"action":"remove","asset":"media/idle.mp3"}' "$WORKDIR/remove.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/remove.json" removed)" = "True" ] && [ ! -e "$PROJ/media/idle.mp3" ]; then
  pass "assets-unused-remove-applies"
else
  fail "assets-unused-remove-applies" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/assets" '{"action":"rename","asset":"media/spare.png","name":"spare2.png"}' "$WORKDIR/rename.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/rename.json" to)" = "media/spare2.png" ] && [ -f "$PROJ/media/spare2.png" ]; then
  pass "assets-unused-rename-applies"
else
  fail "assets-unused-rename-applies" "http $CODE: $(cat "$WORKDIR/rename.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/assets" '{"action":"insert","asset":"fonts/Body.ttf"}' "$WORKDIR/insert-font.txt")"
if [ "$CODE" = "409" ] && grep -q 'declarations in the notation' "$WORKDIR/insert-font.txt"; then
  pass "assets-font-insert-refuses"
else
  fail "assets-font-insert-refuses" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/assets?project=other" '{"action":"remove","asset":"media/card.png"}' "$WORKDIR/assets-readonly.txt")"
if [ "$CODE" = "409" ] && grep -q 'read-only' "$WORKDIR/assets-readonly.txt"; then
  pass "assets-other-project-refuses-read-only"
else
  fail "assets-other-project-refuses-read-only" "http $CODE"
fi

# ---- gate 4: code panel acts --------------------------------------------
CODE="$(api_get "${URL}api/studio/code/tree" "$WORKDIR/tree.json")"
if [ "$CODE" = "200" ] && grep -q 'composition.kdl' "$WORKDIR/tree.json"; then
  pass "code-tree-names-root"
else
  fail "code-tree-names-root" "http $CODE"
fi
CODE="$(api_get "${URL}api/studio/code/file?path=composition.kdl" "$WORKDIR/file.json")"
if [ "$CODE" = "200" ] && grep -q 'var(--tagline)' "$WORKDIR/file.json"; then
  pass "code-file-carries-notation"
else
  fail "code-file-carries-notation" "http $CODE"
fi
CODE="$(api_get "${URL}api/studio/code/jump?selection=hero" "$WORKDIR/jump.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/jump.json" line)" = "4" ]; then
  pass "code-jump-lands-on-hero"
else
  fail "code-jump-lands-on-hero" "http $CODE: $(cat "$WORKDIR/jump.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/code/jump?selection=nope" "$WORKDIR/jump-missing.txt")"
if [ "$CODE" = "404" ]; then
  pass "code-unknown-jump-refuses"
else
  fail "code-unknown-jump-refuses" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/code/tree" '{"actor":"maker","action":"new","path":"panel-note.md","content":"# panels\n"}' "$WORKDIR/tree-new.json")"
if [ "$CODE" = "200" ] && [ -f "$PROJ/panel-note.md" ]; then
  pass "code-tree-creates-note"
else
  fail "code-tree-creates-note" "http $CODE"
fi
cp "$PROJ/composition.kdl" "$WORKDIR/kdl-before-badsave.kdl"
CODE="$(api_post "${URL}api/studio/code/save" '{"actor":"maker","path":"composition.kdl","content":"}\n"}' "$WORKDIR/save-bad.txt")"
if [ "$CODE" = "409" ] && cmp -s "$PROJ/composition.kdl" "$WORKDIR/kdl-before-badsave.kdl"; then
  pass "code-invalid-save-refuses-keeping-last-good"
else
  fail "code-invalid-save-refuses-keeping-last-good" "http $CODE"
fi
python3 - "$WORKDIR/kdl-before-badsave.kdl" "$WORKDIR/save-good-body.json" <<'EOF'
import json, sys
base = open(sys.argv[1]).read()
good = base.replace('x=10 y=10 size=24', 'x=10 y=10 size=28', 1)
assert good != base, "hero size must change"
json.dump({"actor": "maker", "path": "composition.kdl", "content": good}, open(sys.argv[2], 'w'))
EOF
CODE="$(api_post_file "${URL}api/studio/code/save" "$WORKDIR/save-good-body.json" "$WORKDIR/save-good.json")"
if [ "$CODE" = "200" ] && grep -q 'x=10 y=10 size=28' "$PROJ/composition.kdl"; then
  expect_turn "$WORKDIR/save-good.json" "code-valid-save-commits"
else
  fail "code-valid-save-commits" "http $CODE: $(cat "$WORKDIR/save-good.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi

# ---- gate 5: variables panel acts ---------------------------------------
CODE="$(api_get "${URL}api/studio/variables" "$WORKDIR/vars.json")"
if [ "$CODE" = "200" ] && grep -q '"id":"tagline"' "$WORKDIR/vars.json"; then
  pass "variables-list-names-tagline"
else
  fail "variables-list-names-tagline" "http $CODE"
fi
cp "$PROJ/composition.kdl" "$WORKDIR/kdl-before-declare.kdl"
CODE="$(api_post "${URL}api/studio/variables" '{"action":"declare","actor":"maker","id":"kicker","type":"string","default":"Go"}' "$WORKDIR/declare.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/declare.json" saved)" = "True" ] \
    && [ "$(jget "$WORKDIR/declare.json" id)" = "kicker" ]; then
  expect_turn "$WORKDIR/declare.json" "variables-declare-records-turn"
else
  fail "variables-declare-records-turn" "http $CODE: $(cat "$WORKDIR/declare.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi
if cmp -s "$PROJ/composition.kdl" "$WORKDIR/kdl-before-declare.kdl"; then
  pass "variables-declare-keeps-sources-byte-identical"
else
  fail "variables-declare-keeps-sources-byte-identical" "composition.kdl moved"
fi
CODE="$(api_post "${URL}api/studio/variables" '{"action":"preview","overrides":{"tagline":"Flash","ghost":1}}' "$WORKDIR/preview-vars.json")"
if [ "$CODE" = "200" ] && grep -q '"applied":\["tagline"\]' "$WORKDIR/preview-vars.json" \
    && grep -q '"variable":"ghost"' "$WORKDIR/preview-vars.json"; then
  pass "variables-preview-applies-known-warns-unknown"
else
  fail "variables-preview-applies-known-warns-unknown" "http $CODE: $(cat "$WORKDIR/preview-vars.json" 2>/dev/null)"
fi
CODE="$(api_get "${URL}api/studio/variables?view=values&overrides=%7B%22tagline%22%3A%22Flash%22%7D" "$WORKDIR/values.json")"
if [ "$CODE" = "200" ] && [ "$(cat "$WORKDIR/values.json")" = '{"tagline":"Flash"}' ]; then
  pass "variables-values-copy-is-exact"
else
  fail "variables-values-copy-is-exact" "http $CODE: $(cat "$WORKDIR/values.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/variables" '{"action":"bind","actor":"maker","target":{"id":"hero"},"property":"value","variable":"ghost"}' "$WORKDIR/bind-ghost.txt")"
if [ "$CODE" = "409" ] && grep -q -- '--ghost' "$WORKDIR/bind-ghost.txt"; then
  pass "variables-undeclared-bind-refuses"
else
  fail "variables-undeclared-bind-refuses" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/variables" '{"action":"bind","actor":"maker","target":{"id":"hero"},"property":"value","variable":"kicker"}' "$WORKDIR/bind.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/bind.json" saved)" = "True" ]; then
  expect_turn "$WORKDIR/bind.json" "variables-bind-records-turn"
else
  fail "variables-bind-records-turn" "http $CODE: $(cat "$WORKDIR/bind.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi

# ---- gate 6: audio rack rewrites lanes exactly (library surface) -------
if "$PROBE" audio "$PROJ" "$FFMPEG" "$EXPECT_TURN" "$WORKDIR/audio.json" 2>"$WORKDIR/audio-stderr.txt"; then
  if [ "$(jget "$WORKDIR/audio.json" apply_turn)" = "$EXPECT_TURN" ] \
      && grep -q '"audition_changed":true' "$WORKDIR/audio.json"; then
    pass "audio-preset-lane-roundtrip-with-refusals (turns $EXPECT_TURN..$((EXPECT_TURN + 2)))"
  else
    fail "audio-preset-lane-roundtrip-with-refusals" "report mismatch: $(cat "$WORKDIR/audio.json" 2>/dev/null)"
  fi
else
  fail "audio-preset-lane-roundtrip-with-refusals" "$(cat "$WORKDIR/audio-stderr.txt" 2>/dev/null)"
fi
EXPECT_TURN=$((EXPECT_TURN + 3))

# ---- gate 7: cross-project paste lands renamed with styles carried -----
CODE="$(api_post "${URL}api/studio/clipboard/copy" '{"composition":"main","clips":[{"id":"hero"},{"id":"card"}]}' "$WORKDIR/copied.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/copied.json" clipboard)" = "1" ] \
    && [ "$(jget "$WORKDIR/copied.json" kind)" = "clips" ] \
    && [ "$(python3 -c 'import json; print(len(json.load(open("'"$WORKDIR"'/copied.json"))["clips"]))' 2>/dev/null)" = "2" ]; then
  pass "clipboard-copy-carries-two-clips"
else
  fail "clipboard-copy-carries-two-clips" "http $CODE"
fi
python3 - "$WORKDIR/copied.json" "$WORKDIR/paste-body.json" <<'EOF'
import json, sys
env = json.load(open(sys.argv[1]))
body = {"composition": "b", "playhead": 3.0, "actor": "maker", "clipboard": env}
json.dump(body, open(sys.argv[2], "w"))
EOF
CODE="$(api_post_file "${URL2}api/studio/clipboard/paste" "$WORKDIR/paste-body.json" "$WORKDIR/pasted.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/pasted.json" pasted)" = "True" ] \
    && grep -q '"renamed":\["hero-paste","card-paste"\]' "$WORKDIR/pasted.json" \
    && [ "$(jget "$WORKDIR/pasted.json" turn)" = "0" ]; then
  pass "clipboard-cross-project-paste-renames (sibling turn 0)"
else
  fail "clipboard-cross-project-paste-renames" "http $CODE: $(cat "$WORKDIR/pasted.json" 2>/dev/null)"
fi
CODE="$(api_post "${URL}api/studio/clipboard/copy" '{"composition":"main","clips":[{"id":"ghost"}]}' "$WORKDIR/copy-ghost.txt")"
if [ "$CODE" = "404" ] && grep -q 'ghost' "$WORKDIR/copy-ghost.txt"; then
  pass "clipboard-unknown-copy-refuses"
else
  fail "clipboard-unknown-copy-refuses" "http $CODE"
fi
CODE="$(api_post "${URL}api/studio/clipboard/paste" '{}' "$WORKDIR/paste-empty.txt")"
if [ "$CODE" = "400" ]; then
  pass "clipboard-empty-paste-refuses"
else
  fail "clipboard-empty-paste-refuses" "http $CODE"
fi
if "$PROBE" paste "$PROJ" "$PROJ2" 3.0 "$WORKDIR/paste-check.json" 2>"$WORKDIR/paste-check-stderr.txt"; then
  if grep -q '"styles_match":true' "$WORKDIR/paste-check.json"; then
    pass "clipboard-styles-and-motion-carried"
  else
    fail "clipboard-styles-and-motion-carried" "report mismatch: $(cat "$WORKDIR/paste-check.json" 2>/dev/null)"
  fi
else
  fail "clipboard-styles-and-motion-carried" "$(cat "$WORKDIR/paste-check-stderr.txt" 2>/dev/null)"
fi

# ---- gate 8: compositions place nests b-roll and render-one queues -----
CODE="$(api_get "${URL}api/studio/compositions" "$WORKDIR/cards-fresh.json")"
GEN2="$(jget "$WORKDIR/cards-fresh.json" generation)"
if [ "$CODE" = "200" ] && [ -n "$GEN2" ]; then
  pass "compositions-generation-refreshes"
else
  fail "compositions-generation-refreshes" "http $CODE"
fi
cp "$PROJ/composition.kdl" "$WORKDIR/kdl-before-place.kdl"
CODE="$(api_post "${URL}api/studio/compositions/place" "{\"actor\":\"maker\",\"source\":\"b-roll\",\"target\":\"main\",\"at\":1.0,\"generation\":$GEN2}" "$WORKDIR/place.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/place.json" placed)" = "True" ]; then
  expect_turn "$WORKDIR/place.json" "compositions-place-nests-broll"
else
  fail "compositions-place-nests-broll" "http $CODE: $(cat "$WORKDIR/place.json" 2>/dev/null)"
  EXPECT_TURN=$((EXPECT_TURN + 1))
fi
if cmp -s "$PROJ/composition.kdl" "$WORKDIR/kdl-before-place.kdl"; then
  pass "compositions-place-keeps-sources-byte-identical"
else
  fail "compositions-place-keeps-sources-byte-identical" "a nest turn rewrote sources (turns live in hidden state)"
fi
CODE="$(api_post "${URL}api/studio/compositions/render-one" "{\"actor\":\"maker\",\"composition\":\"b-roll\",\"generation\":$GEN2}" "$WORKDIR/render-stale.txt")"
if [ "$CODE" = "409" ] && grep -q '"stale":true' "$WORKDIR/render-stale.txt"; then
  pass "compositions-stale-generation-refuses"
else
  fail "compositions-stale-generation-refuses" "http $CODE"
fi
GEN3="$(python3 -c 'import json; print(json.load(open("'"$WORKDIR"'/render-stale.txt"))["cards"]["generation"])' 2>/dev/null)"
CODE="$(api_post "${URL}api/studio/compositions/render-one" "{\"actor\":\"maker\",\"composition\":\"b-roll\",\"generation\":$GEN3}" "$WORKDIR/render-one.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/render-one.json" seq)" = "1" ] \
    && grep -q '"composition":"b-roll"' "$WORKDIR/render-one.json"; then
  pass "compositions-render-one-queues-mp4 (seq 1)"
else
  fail "compositions-render-one-queues-mp4" "http $CODE gen=$GEN3: $(cat "$WORKDIR/render-one.json" 2>/dev/null)"
fi

# ---- gate 9: queued render downloads byte-identical, GIF loops ---------
CODE="$(api_post "${URL}api/studio/renders" '{"actor":"maker","format":"gif","composition":"b-roll","loop_count":2}' "$WORKDIR/enqueue-gif.json")"
if [ "$CODE" = "200" ] && [ "$(jget "$WORKDIR/enqueue-gif.json" seq)" = "2" ]; then
  pass "renders-gif-loop-2-queues (seq 2)"
else
  fail "renders-gif-loop-2-queues" "http $CODE: $(cat "$WORKDIR/enqueue-gif.json" 2>/dev/null)"
fi
poll_job() {
  # $1=seq -> prints status once finished (up to ~180 s)
  for _ in $(seq 1 360); do
    if curl -s "${URL}api/studio/renders" -o "$WORKDIR/queue.json" 2>/dev/null; then
      ST="$(python3 -c 'import json,sys; d=json.load(open("'"$WORKDIR"'/queue.json")); j=[x for x in d["jobs"] if x["seq"]==int(sys.argv[1])]; print(j[0]["status"] if j else "missing")' "$1" 2>/dev/null)"
      if [ "$ST" = "succeeded" ]; then echo "$ST"; return 0; fi
      if [ "$ST" = "failed" ] || [ "$ST" = "cancelled" ] || [ "$ST" = "missing" ]; then echo "$ST"; return 0; fi
    fi
    sleep 0.5
  done
  echo "timeout"
  return 0
}
if [ "$(poll_job 1)" = "succeeded" ] && [ "$(poll_job 2)" = "succeeded" ]; then
  pass "renders-queue-runs-both-jobs-to-succeeded"
else
  fail "renders-queue-runs-both-jobs-to-succeeded" "statuses: $(cat "$WORKDIR/queue.json" 2>/dev/null)"
fi
if grep -q '"available":true' "$WORKDIR/queue.json" \
    && [ "$(python3 -c 'import json; d=json.load(open("'"$WORKDIR"'/queue.json")); print(len(d["jobs"]))' 2>/dev/null)" = "2" ] \
    && python3 -c 'import json,sys; d=json.load(open("'"$WORKDIR"'/queue.json")); sys.exit(0 if all((j.get("output_bytes") or 0)>0 for j in d["jobs"]) else 1)'; then
  pass "renders-queue-reports-ffmpeg-and-output-bytes"
else
  fail "renders-queue-reports-ffmpeg-and-output-bytes" "$(cat "$WORKDIR/queue.json" 2>/dev/null)"
fi
MP4OUT="$(python3 -c 'import json; d=json.load(open("'"$WORKDIR"'/queue.json")); print([j["output"] for j in d["jobs"] if j["seq"]==1][0])' 2>/dev/null)"
curl -s "${URL}api/studio/renders/1/download" -o "$WORKDIR/dl1.mp4" 2>/dev/null
if [ -n "$MP4OUT" ] && [ -f "$MP4OUT" ] && cmp -s "$WORKDIR/dl1.mp4" "$MP4OUT" && [ -s "$WORKDIR/dl1.mp4" ] \
    && python3 -c 'import sys; sys.exit(0 if open("'"$WORKDIR"'/dl1.mp4","rb").read()[4:8]==b"ftyp" else 1)'; then
  pass "renders-mp4-download-byte-identical"
else
  fail "renders-mp4-download-byte-identical" "output=$MP4OUT"
fi
curl -s "${URL}api/studio/renders/2/download" -o "$WORKDIR/dl2.gif" 2>/dev/null
if python3 - "$WORKDIR/dl2.gif" <<'EOF'
import sys
d = open(sys.argv[1], 'rb').read()
assert d[:6] == b'GIF89a', "not a GIF"
i = d.find(b'NETSCAPE2.0')
assert i >= 0, "no loop extension"
assert int.from_bytes(d[i+13:i+15], 'little') == 2, "loop count must be 2"
EOF
then
  pass "renders-gif-loops-twice-as-chosen"
else
  fail "renders-gif-loops-twice-as-chosen" "download the GIF and check the NETSCAPE loop count"
fi

# ---- gate 10: the served page hosts every panel -------------------------
CODE="$(api_get "${URL}studio" "$WORKDIR/studio.html")"
PAGE_OK=1
for NEEDLE in panel-compositions panel-renders code-panel background-color paintLayers paintCompositions paintRenders; do
  grep -q "$NEEDLE" "$WORKDIR/studio.html" 2>/dev/null || PAGE_OK=0
done
if [ "$CODE" = "200" ] && [ "$PAGE_OK" = "1" ]; then
  pass "page-hosts-every-panel"
else
  fail "page-hosts-every-panel" "http $CODE"
fi

# ---- shared history: eleven named turns ----------------------------------
TURNS="$("$FSMITH" history list --json "$PROJ" 2>/dev/null | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["turns"]))' 2>/dev/null)"
if [ "$TURNS" = "11" ] && [ "$EXPECT_TURN" = "11" ]; then
  pass "history-holds-eleven-named-turns"
else
  fail "history-holds-eleven-named-turns" "turns=$TURNS expected-chain=$EXPECT_TURN"
fi

# ---- tamper negative: a corrupted composition must refuse ---------------
TAMPER="$WORKDIR/tampered"
rm -rf "$TAMPER"
cp -r "$PROJ" "$TAMPER"
printf 'composition name="main" width=320 height=180 duration=4 fps=10 background="not-a-color" version=1 {\n}\n' >"$TAMPER/composition.kdl"
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
  echo "sibling-url: ${URL2:-none}"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
  echo "browser: none on this machine; /studio page asserted over HTTP, full HTML kept as evidence"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/tamper.log" "$WORKDIR/check.log" "$WORKDIR/lint.log" "$EVID_DIR/" 2>/dev/null || true
for F in cards.json hover.png layers-before.json reorder.json layers-reordered.json group.json layers-grouped.json \
    ungroup.json layers-ungrouped.json assets.json import.json insert.json reveal.json remove.json rename.json \
    tree.json file.json jump.json save-good.json vars.json declare.json preview-vars.json values.json bind.json \
    audio.json copied.json pasted.json paste-check.json place.json render-one.json enqueue-gif.json queue.json \
    dl2.gif studio.html; do
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
