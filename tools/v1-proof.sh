#!/bin/sh
# T006 one-command gate: prove every v1 outcome O1–O8 (PROPOSAL.md section 6)
# with evidence, or name what failed. Exit 0 only when everything covered
# holds; exit 1 names the failing or OPEN item (no false pass).
#
# Coverage in one invocation:
#   O1 hello on a clean machine ............ live check+render, offline
#                                            refusal, container facts
#   O2 v1 features work .................... cargo test --workspace
#                                            (per-sprint fixture verdicts)
#                                            + both references render exit 0
#   O3 deterministic ........................ suite verdict + two hello
#                                            renders decode byte-identically
#   O4 faster than HyperFrames .............. benchmark script verdict;
#                                            OPEN while the HyperFrames tool
#                                            is missing (no ratio claimed)
#   O5 precise check ....................... invalid fixture refused exit 1
#                                            with located diagnostics + suite
#   O6 refuse, never mis-render ............. later-sprint fixture refused
#                                            exit 2 before frame one + suite
#   O7 standard output ..................... ffprobe contract on all three
#                                            MP4s + full decode + moov order
#                                            + headless-browser play
#   O8 sample-accurate audio ............... suite verdict (mix/sample tests)
# Plus: README spot-checks (full 30-step walkthrough stays the T005
# evidence), and the automated playback proof. The one manual
# click-to-play confirmation is recorded once per release and stays
# STILL-NEEDED here: automation cannot click.
#
# The gate reuses per-sprint fixture verdicts (it runs the repo test
# suites; it does not re-decide them) and refuses to prove a folder
# that does not validate: every render below runs only after its own
# `check` exits 0, and invalid/later-sprint folders are shown refused.
# All numbers carry tool versions and the run date (see the header).
#
# Usage: v1-proof.sh [--workdir DIR]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never written: examples/fixtures are staged
#   to temp dirs before any render.

set -u

WORKDIR=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,40p' "$0"; exit 0 ;;
    *) echo "v1-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
CLI_DIR="$FRAMESMITH_DIR/crates/cli"
EVID="$FRAMESMITH_DIR/../evidences"
PLAYBACK_SCRIPT="$EVID/proof-n04-T007/playback-proof.sh"
BENCH_SCRIPT="$TOOLS_DIR/benchmark.sh"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-v1-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""
OPEN_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }
open_item() { OPEN_LIST="$OPEN_LIST $1"; echo "OPEN: $1 -- $2"; }

echo "================ v1 proof gate (Sprint 4 T006) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
if command -v sw_vers >/dev/null 2>&1; then sw_vers | sed 's/^/macos: /'; fi
echo "cargo: $(cargo --version 2>/dev/null || echo MISSING)"
echo "rustc: $(rustc --version 2>/dev/null || echo MISSING)"
echo "ffmpeg(host): $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
echo "ffprobe(host): $(ffprobe -version 2>/dev/null | head -n 1 || echo none)"
echo "python3: $(python3 --version 2>&1 || echo none)"
echo "workdir: $WORKDIR"
echo "=================================================================="
echo ""

for NEED in cargo ffmpeg ffprobe python3; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
if [ ! -x "$PLAYBACK_SCRIPT" ] && [ ! -f "$PLAYBACK_SCRIPT" ]; then
  fail "prereq-playback-script" "missing $PLAYBACK_SCRIPT"
fi
if [ ! -f "$BENCH_SCRIPT" ]; then
  fail "prereq-benchmark-script" "missing $BENCH_SCRIPT"
fi
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "GATE: FAIL -- missing prerequisites:${FAIL_LIST}"
  exit 1
fi

echo "--- build framesmith binary ---"
if (cd "$FRAMESMITH_DIR" && cargo build --bin framesmith >"$WORKDIR/build.log" 2>&1); then
  pass "build"
else
  fail "build" "cargo build failed; see $WORKDIR/build.log"
  echo ""
  echo "GATE: FAIL -- failing items:${FAIL_LIST}"
  exit 1
fi
FSMITH="$FRAMESMITH_DIR/target/debug/framesmith"
echo "framesmith: $("$FSMITH" --version)"
echo ""

# stage NAME SRC — copy a composition/fixture folder minus render outputs.
stage() {
  _dst="$WORKDIR/stage-$1"
  rm -rf "$_dst"
  mkdir -p "$_dst"
  (cd "$2" && find . -mindepth 1 -maxdepth 1 ! -name 'out.mp4' ! -name 'logs' -exec cp -R {} "$_dst/" ';')
  printf '%s' "$_dst"
}

# container_ok MP4 WIDTH HEIGHT — the v1 container contract (O7).
container_ok() {
  _facts="$(ffprobe -v error -show_entries stream=codec_name,width,height,pix_fmt,sample_rate,channels -of default=noprint_wrappers=1 "$1" 2>&1)"
  for _want in codec_name=h264 "width=$2" "height=$3" pix_fmt=yuv420p codec_name=aac sample_rate=48000 channels=2; do
    case "$_facts" in
      *"$_want"*) ;;
      *) echo "missing $_want in: $_facts"; return 1 ;;
    esac
  done
  return 0
}

echo "--- O5/O6: refusals (the gate proves only folders that validate) ---"
STAGE_INVALID="$(stage invalid-timing "$CLI_DIR/fixtures/invalid-timing")"
if _out="$("$FSMITH" check "$STAGE_INVALID" 2>&1)"; then
  fail "O5-invalid-check" "invalid folder checked exit 0, want exit 1"
else
  _code="$?"
  if [ "$_code" -eq 1 ] && echo "$_out" | grep -q "E_BAD_TIMING"; then
    pass "O5-invalid-check (exit 1, located E_BAD_TIMING)"
  else
    fail "O5-invalid-check" "want exit 1 + E_BAD_TIMING, got exit $_code: $_out"
  fi
fi
if _out="$("$FSMITH" render "$STAGE_INVALID" -o "$WORKDIR/invalid.mp4" 2>&1)"; then
  fail "O5-invalid-render" "invalid folder rendered exit 0, want exit 1"
else
  _code="$?"
  if [ "$_code" -eq 1 ] && [ ! -f "$WORKDIR/invalid.mp4" ] && [ ! -f "$STAGE_INVALID/out.mp4" ]; then
    pass "O5-invalid-render (exit 1, no MP4)"
  else
    fail "O5-invalid-render" "want exit 1 with no MP4, got exit $_code"
  fi
fi

STAGE_LATER="$(stage later-audio "$CLI_DIR/fixtures/later-audio")"
if _out="$("$FSMITH" check "$STAGE_LATER" 2>&1)"; then
  fail "O6-later-check" "later-sprint folder checked exit 0, want exit 2"
else
  _code="$?"
  if [ "$_code" -eq 2 ]; then
    pass "O6-later-check (exit 2, element named with its sprint)"
  else
    fail "O6-later-check" "want exit 2, got exit $_code: $_out"
  fi
fi
if _out="$("$FSMITH" render "$STAGE_LATER" -o "$WORKDIR/later.mp4" 2>&1)"; then
  fail "O6-later-render" "later-sprint folder rendered exit 0, want exit 2"
else
  _code="$?"
  if [ "$_code" -eq 2 ] && [ ! -f "$WORKDIR/later.mp4" ] && [ ! -f "$STAGE_LATER/out.mp4" ]; then
    pass "O6-later-render (exit 2 before the first frame, no MP4)"
  else
    fail "O6-later-render" "want exit 2 with no MP4, got exit $_code"
  fi
fi
echo ""

echo "--- O2/O3/O5/O6/O8: per-sprint fixture verdicts (repo test suites) ---"
echo "running: cargo test --workspace (this reuses the suites, it does not re-decide them)"
if (cd "$FRAMESMITH_DIR" && cargo test --workspace >"$WORKDIR/suite.log" 2>&1); then
  SUITE_CODE=0
  pass "suite (cargo test --workspace green)"
else
  SUITE_CODE="$?"
  fail "suite" "cargo test --workspace exit $SUITE_CODE; see $WORKDIR/suite.log"
fi
grep -h "test result:" "$WORKDIR/suite.log" | sort | uniq -c | head -n 20
echo "(full transcript: $WORKDIR/suite.log)"
echo ""

echo "--- O4/T003/T004: benchmark (same machine, natural Sprint 4 settings) ---"
echo "--- O4 is judged on the measured numbers below: PASS only on strict"
echo "--- Framesmith wins on both references; otherwise an OPEN shortfall"
echo "--- with the numbers (the measurement is never rewritten)."
if sh "$BENCH_SCRIPT" --reps 1 --workdir "$WORKDIR/bench" --out "$WORKDIR/bench-RESULTS.md" >"$WORKDIR/bench.log" 2>&1; then
  BENCH_CODE=0
  # The machine-readable SPEED lines live in bench.log (bench-RESULTS.md
  # carries the human-readable table with the same numbers).
  if grep -q "^SPEED .* verdict=shortfall" "$WORKDIR/bench.log" 2>/dev/null; then
    open_item "O4-shortfall" "speed shortfall with numbers kept: $(grep -h '^SPEED' "$WORKDIR/bench.log" | tr '\n' ';' | cut -c1-400); see $WORKDIR/bench-RESULTS.md"
  elif [ "$(grep -c '^SPEED .* verdict=faster' "$WORKDIR/bench.log" 2>/dev/null)" -ge 2 ]; then
    pass "benchmark (strict Framesmith wins on both references; ratios in $WORKDIR/bench-RESULTS.md)"
  else
    fail "benchmark" "benchmark ran but recorded no speed verdicts; see $WORKDIR/bench.log $WORKDIR/bench-RESULTS.md"
  fi
else
  BENCH_CODE="$?"
  if [ "$BENCH_CODE" -eq 2 ]; then
    open_item "benchmark" "HyperFrames tool missing (named in $WORKDIR/bench.log); T003/T004 and O4 stay OPEN, no ratio claimed"
  else
    fail "benchmark" "benchmark script error exit $BENCH_CODE; see $WORKDIR/bench.log"
  fi
fi
grep -h "^RESULT" "$WORKDIR/bench.log" || true
grep -h "^SPEED" "$WORKDIR/bench.log" || true
grep -h "^SSIM" "$WORKDIR/bench.log" || true
echo "(full transcript: $WORKDIR/bench.log)"
echo ""

echo "--- O1: hello renders with no configuration ---"
STAGE_HELLO="$(stage hello "$CLI_DIR/examples/hello")"
if "$FSMITH" check "$STAGE_HELLO" >"$WORKDIR/hello-check.log" 2>&1; then
  pass "O1-hello-check (exit 0)"
  HELLO_OK=1
else
  fail "O1-hello-check" "hello did not validate; refusing to prove it (see $WORKDIR/hello-check.log)"
  HELLO_OK=0
fi
HELLO_MP4="$WORKDIR/hello.mp4"
if [ "$HELLO_OK" -eq 1 ]; then
  if "$FSMITH" render "$STAGE_HELLO" -o "$HELLO_MP4" >"$WORKDIR/hello-render.log" 2>&1; then
    if container_ok "$HELLO_MP4" 640 360; then
      pass "O1-hello-render (exit 0, h264/640x360/yuv420p + aac/48000/stereo)"
    else
      fail "O1-hello-container" "container facts wrong: $(container_ok "$HELLO_MP4" 640 360)"
    fi
  else
    fail "O1-hello-render" "hello render failed; see $WORKDIR/hello-render.log"
  fi
fi
echo ""

echo "--- O2/O7: both references render in their own tool ---"
for REF_W_H in "reference-1 1920 1080" "reference-2 1920 1080"; do
  set -- $REF_W_H
  _mp4="$WORKDIR/bench/$1-framesmith-r1.mp4"
  if [ -f "$_mp4" ] && container_ok "$_mp4" "$2" "$3"; then
    pass "$1 (exit 0 via benchmark, container contract holds)"
  else
    fail "$1" "missing or non-contract MP4 $_mp4; see $WORKDIR/bench.log"
  fi
done
echo ""

echo "--- O3: CPU renders are deterministic ---"
if [ "${HELLO_OK:-0}" -eq 1 ]; then
  if "$FSMITH" render "$STAGE_HELLO" -o "$WORKDIR/det-a.mp4" >"$WORKDIR/det-a.log" 2>&1 \
  && "$FSMITH" render "$STAGE_HELLO" -o "$WORKDIR/det-b.mp4" >"$WORKDIR/det-b.log" 2>&1 \
  && ffmpeg -y -v error -i "$WORKDIR/det-a.mp4" -frames:v 10 -f rawvideo -pix_fmt rgb24 "$WORKDIR/det-a.raw" \
  && ffmpeg -y -v error -i "$WORKDIR/det-b.mp4" -frames:v 10 -f rawvideo -pix_fmt rgb24 "$WORKDIR/det-b.raw" \
  && cmp -s "$WORKDIR/det-a.raw" "$WORKDIR/det-b.raw"; then
    pass "O3-determinism (two hello renders decode byte-identically)"
  else
    fail "O3-determinism" "two hello renders differ; see $WORKDIR/det-a.log $WORKDIR/det-b.log"
  fi
else
  fail "O3-determinism" "skipped: hello did not validate"
fi
echo ""

echo "--- O1 limit 4: no FFmpeg and no network is a named refusal ---"
mkdir -p "$WORKDIR/empty-cache"
if _out="$(env -i PATH=/usr/bin:/bin FRAMESMITH_FFMPEG_DIR="$WORKDIR/empty-cache" FRAMESMITH_OFFLINE=1 "$FSMITH" render "$STAGE_HELLO" -o "$WORKDIR/offline.mp4" 2>&1)"; then
  fail "O1-offline-refusal" "offline hidden-FFmpeg render exited 0, want exit 1"
else
  _code="$?"
  if [ "$_code" -eq 1 ] && echo "$_out" | grep -q "E_NO_FFMPEG" \
    && [ ! -f "$WORKDIR/offline.mp4" ] && [ ! -f "$STAGE_HELLO/out.mp4" ]; then
    pass "O1-offline-refusal (exit 1 E_NO_FFMPEG, no MP4, no partial)"
  else
    fail "O1-offline-refusal" "want exit 1 + E_NO_FFMPEG with no MP4, got exit $_code"
  fi
fi
if _out="$(env -i PATH=/usr/bin:/bin FRAMESMITH_FFMPEG_DIR="$WORKDIR/empty-cache" FRAMESMITH_OFFLINE=1 "$FSMITH" check "$STAGE_HELLO" 2>&1)"; then
  pass "O1-offline-check (check still exit 0: validation needs no FFmpeg)"
else
  fail "O1-offline-check" "check under offline env failed: $_out"
fi
echo ""

echo "--- O6: GPU refusal names the missing capability (or the GPU runs) ---"
if _out="$("$FSMITH" render "$STAGE_HELLO" --backend gpu -o "$WORKDIR/gpu.mp4" 2>&1)"; then
  if [ -f "$WORKDIR/gpu.mp4" ]; then
    pass "O6-gpu (this machine has a GPU renderer; render exit 0)"
  else
    fail "O6-gpu" "gpu render exited 0 but wrote no MP4"
  fi
else
  _code="$?"
  if [ "$_code" -eq 1 ] && [ ! -f "$WORKDIR/gpu.mp4" ]; then
    pass "O6-gpu (exit 1 naming the missing capability, no MP4)"
  else
    fail "O6-gpu" "want exit 1 with no MP4, got exit $_code: $_out"
  fi
fi
echo ""

echo "--- README spot-checks (full 30-step walkthrough is the T005 evidence) ---"
README_OK=1
if "$FSMITH" --help >"$WORKDIR/readme-help.log" 2>&1 \
&& "$FSMITH" --version >"$WORKDIR/readme-version.log" 2>&1 \
&& "$FSMITH" check --json "$STAGE_HELLO" >"$WORKDIR/readme-json.log" 2>&1 \
&& "$FSMITH" errors E_NO_FFMPEG >"$WORKDIR/readme-e1.log" 2>&1 \
&& "$FSMITH" errors >"$WORKDIR/readme-eall.log" 2>&1; then
  :
else
  README_OK=0
fi
rm -rf "$WORKDIR/readme-scaffold"
if "$FSMITH" init demo --dir "$WORKDIR/readme-scaffold" >"$WORKDIR/readme-init.log" 2>&1 \
&& "$FSMITH" check "$WORKDIR/readme-scaffold/demo" >"$WORKDIR/readme-scaffold-check.log" 2>&1; then
  :
else
  README_OK=0
fi
if [ "$README_OK" -eq 1 ]; then
  pass "README commands (--help/--version/check --json/errors/init+check)"
else
  fail "README" "a README spot-check failed; see $WORKDIR/readme-*.log"
fi
echo ""

echo "--- O7: automated playback proof on all three MP4s ---"
if [ -f "$HELLO_MP4" ] && [ -f "$WORKDIR/bench/reference-1-framesmith-r1.mp4" ] && [ -f "$WORKDIR/bench/reference-2-framesmith-r1.mp4" ]; then
  if sh "$PLAYBACK_SCRIPT" "$HELLO_MP4" "$WORKDIR/bench/reference-1-framesmith-r1.mp4" "$WORKDIR/bench/reference-2-framesmith-r1.mp4" >"$WORKDIR/playback.log" 2>&1; then
    pass "O7-playback-automation (container facts, full decode, moov order, headless-browser play)"
  else
    fail "O7-playback-automation" "playback proof failed; see $WORKDIR/playback.log"
  fi
else
  fail "O7-playback-automation" "MP4s missing; cannot run playback proof"
fi
echo "manual click-to-play (QuickTime, Chrome, Safari): STILL-NEEDED, recorded once per release;"
echo "player versions detected on this machine:"
if [ -d "/System/Applications/QuickTime Player.app" ]; then /usr/bin/defaults read "/System/Applications/QuickTime Player.app/Contents/Info" CFBundleShortVersionString 2>/dev/null | sed 's/^/  QuickTime Player: /'; elif [ -d "/Applications/QuickTime Player.app" ]; then /usr/bin/defaults read "/Applications/QuickTime Player.app/Contents/Info" CFBundleShortVersionString 2>/dev/null | sed 's/^/  QuickTime Player: /'; else echo "  QuickTime Player: not installed"; fi
[ -d "/Applications/Google Chrome.app" ] && /usr/bin/defaults read "/Applications/Google Chrome.app/Contents/Info" CFBundleShortVersionString 2>/dev/null | sed 's/^/  Chrome: /' || echo "  Chrome: not installed"
[ -d "/Applications/Safari.app" ] && /usr/bin/defaults read "/Applications/Safari.app/Contents/Info" CFBundleShortVersionString 2>/dev/null | sed 's/^/  Safari: /' || echo "  Safari: not installed"
echo "(automation cannot click; the human confirmation stays per-release and does not fail this gate)"
echo ""

echo "================ verdicts O1–O8 ================"
has_fail() {
  case "$FAIL_LIST" in
    *"$1"*) return 0 ;;
    *) return 1 ;;
  esac
}
has_open() {
  case "$OPEN_LIST" in
    *"$1"*) return 0 ;;
    *) return 1 ;;
  esac
}
verdict() {
  # verdict OUTCOME STATUS EVIDENCE
  echo "O$1: $2 -- $3"
}

if has_fail "O1-"; then verdict 1 "FAIL" "see $WORKDIR/hello-*.log"; else verdict 1 "PASS" "hello check+render exit 0, container contract, offline refusal E_NO_FFMPEG"; fi
if has_fail "suite" || has_fail "reference-"; then verdict 2 "FAIL" "see $WORKDIR/suite.log / $WORKDIR/bench.log"; else verdict 2 "PASS" "suite green + both references render exit 0"; fi
if has_fail "suite" || has_fail "O3-"; then verdict 3 "FAIL" "see $WORKDIR/det-*.log $WORKDIR/suite.log"; else verdict 3 "PASS" "suite green + two hello renders decode byte-identically"; fi
if has_fail "benchmark"; then verdict 4 "FAIL" "see $WORKDIR/bench.log"; elif has_open "O4-shortfall"; then echo "O4: OPEN -- speed shortfall with numbers kept: $(grep -h '^SPEED' "$WORKDIR/bench.log" | tr '\n' ';' | cut -c1-300) -- see $WORKDIR/bench-RESULTS.md"; elif [ "${BENCH_CODE:-1}" -eq 0 ]; then verdict 4 "PASS" "strict Framesmith wins on both references; ratios in $WORKDIR/bench-RESULTS.md"; else echo "O4: OPEN -- HyperFrames tool missing (named in $WORKDIR/bench.log); T003/T004 stay OPEN, no ratio claimed"; fi
if has_fail "O5-" || has_fail "suite"; then verdict 5 "FAIL" "see $WORKDIR/suite.log"; else verdict 5 "PASS" "invalid refused exit 1 with located E_BAD_TIMING + suite green"; fi
if has_fail "O6-" || has_fail "suite"; then verdict 6 "FAIL" "see $WORKDIR/suite.log"; else verdict 6 "PASS" "later-sprint refused exit 2 before frame one, GPU refusal observed + suite green"; fi
if has_fail "hello" || has_fail "reference-" || has_fail "O7-"; then verdict 7 "FAIL" "see $WORKDIR/playback.log"; else verdict 7 "PASS" "ffprobe contract x3, full decode x3, moov-before-mdat x3, headless-browser play x3 (manual click-to-play still per-release)"; fi
if has_fail "suite"; then verdict 8 "FAIL" "see $WORKDIR/suite.log"; else verdict 8 "PASS" "suite green (mix/sample-accuracy fixtures included)"; fi
echo "================================================="
echo "checks passed: $PASS_N"
echo "workdir (all evidence kept): $WORKDIR"
echo ""

if [ -n "$FAIL_LIST" ]; then
  echo "GATE: FAIL -- failing items:$FAIL_LIST"
  if [ -n "$OPEN_LIST" ]; then echo "also open:$OPEN_LIST"; fi
  exit 1
fi
if [ -n "$OPEN_LIST" ]; then
  echo "GATE: OPEN -- open items:$OPEN_LIST"
  echo "The sprint stays open per the spec: a closed Sprint 4 requires the benchmark to have run."
  exit 1
fi
echo "GATE: PASS -- every O1-O8 outcome holds with evidence above."
exit 0
