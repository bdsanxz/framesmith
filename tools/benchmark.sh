#!/bin/sh
# T003 benchmark: render both reference compositions with Framesmith
# and with HyperFrames on the same machine, recording wall time per tool
# per reference plus SSIM between their pictures, with tool versions and
# the repetition count next to the numbers.
#
# Both tools run at natural Sprint 4 settings; no per-tool tuning flags
# are used. Times are measured, never modeled. HyperFrames runs via
# `npx --yes hyperframes@0.9.6` (pinned package, NOT a PATH binary);
# `--quality looks` is passed explicitly but equals the tool default
# (disclosed here, not a tuning flag).
#
# When HyperFrames is absent or not runnable, the benchmark items stay
# OPEN with the missing tool named: the Framesmith-side times are still
# recorded, but no ratio is claimed (exit 2). Exit 0 only when a full
# comparison with ratios was recorded. Exit 1 is a usage or measurement
# error (e.g. a reference that does not validate in its own tool).
# A lost or tied reference is recorded as a speed SHORTFALL with both
# numbers (T004/REC-004); the measurement is never rewritten.
#
# Usage:
#   benchmark.sh [--reps N] [--workdir DIR] [--out FILE] [--check-only]
#
#   --reps N      render each reference N times per tool (default 1).
#                 The repetition count is reported with the numbers.
#   --workdir DIR stage references and write MP4s under DIR
#                 (default: a fresh mktemp directory, kept for inspection).
#   --out FILE    also write the results table to FILE (markdown).
#   --check-only  detect both tools and report OPEN/done without rendering.

set -u

REPS=1
WORKDIR=""
OUT=""
CHECK_ONLY=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --reps) REPS="$2"; shift 2 ;;
    --workdir) WORKDIR="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --check-only) CHECK_ONLY=1; shift ;;
    -h|--help) sed -n '1,25p' "$0"; exit 0 ;;
    *) echo "benchmark.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

case "$REPS" in
  ''|*[!0-9]*|0) echo "benchmark.sh: --reps needs a positive integer" >&2; exit 1 ;;
esac

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
CLI_DIR="$FRAMESMITH_DIR/crates/cli"
REF1="$CLI_DIR/examples/reference-1-title-card"
REF2="$CLI_DIR/examples/reference-2-lower-third"
REF1_HF="$CLI_DIR/examples/reference-1-title-card-hf"
REF2_HF="$CLI_DIR/examples/reference-2-lower-third-hf"

HF_PKG="hyperframes@0.9.6"
HF_QUALITY="looks"

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
OS="$(uname -srm)"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-bench-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

FSMITH="$FRAMESMITH_DIR/target/debug/framesmith"
if [ "$CHECK_ONLY" -eq 0 ] && [ ! -x "$FSMITH" ]; then
  echo "building framesmith binary..." >&2
  (cd "$FRAMESMITH_DIR" && cargo build --bin framesmith) >&2 || exit 1
fi

FSMITH_VERSION="unbuilt"
if [ -x "$FSMITH" ]; then
  FSMITH_VERSION="$("$FSMITH" --version 2>/dev/null || echo 'framesmith (version query failed)')"
fi
FFMPEG_VERSION="$(ffmpeg -version 2>/dev/null | head -n 1)"
if [ -z "$FFMPEG_VERSION" ]; then
  FFMPEG_VERSION="(no ffmpeg on PATH; Framesmith self-provides its pinned build at render time)"
fi
NODE_VERSION="$(node --version 2>/dev/null || echo '(no node on PATH)')"

if HF_VERSION="$(npx --yes "$HF_PKG" --version 2>/dev/null)"; then
  HF_STATE="present: hyperframes $HF_VERSION via npx $HF_PKG (node $NODE_VERSION)"
  HF_MISSING=0
else
  HF_STATE="MISSING: 'npx --yes $HF_PKG --version' failed (needs Node 22+ and npm plan/network to fetch the pinned package)"
  HF_MISSING=1
fi
HF_USAGE="$(npx --yes "$HF_PKG" usage --json 2>/dev/null || echo '{"status":"unknown","reason":"usage-query-failed"}')"
case "$HF_USAGE" in
  *'"status":"unknown"'*) HF_PLAN="unknown (tool reports unsupported_harness)" ;;
  *) HF_PLAN="$HF_USAGE" ;;
esac

now() { python3 -c 'import time,sys; sys.stdout.write("%d" % (time.time() * 1000))'; }

# stage_example NAME SRC DST — copy a composition folder minus render outputs.
stage_example() {
  rm -rf "$3"
  mkdir -p "$3"
  (cd "$2" && find . -mindepth 1 -maxdepth 1 ! -name 'out.mp4' ! -name 'logs' ! -name '.hyperframes' -exec cp -R {} "$3/" ';')
}

# timed_render SRC DST_MP4 — render once, print elapsed seconds to stdout.
# The render transcript goes to stderr so timings stay machine-readable.
timed_render() {
  _start="$(now)"
  "$FSMITH" render "$1" -o "$2" >&2
  _code="$?"
  _end="$(now)"
  if [ "$_code" -ne 0 ]; then
    echo "RENDER-FAILED exit=$_code" >&2
    return "$_code"
  fi
  python3 -c "print('%.1f' % (($_end - $_start) / 1000.0))"
}

# timed_hf_render SRC DST_MP4 — render one HyperFrames folder, print seconds.
timed_hf_render() {
  _start="$(now)"
  npx --yes "$HF_PKG" render --quality "$HF_QUALITY" -o "$2" "$1" >&2
  _code="$?"
  _end="$(now)"
  if [ "$_code" -ne 0 ]; then
    echo "HF-RENDER-FAILED exit=$_code" >&2
    return "$_code"
  fi
  python3 -c "print('%.1f' % (($_end - $_start) / 1000.0))"
}

# compute_ssim FS_MP4 HF_MP4 TAG — print "Y U V All" SSIM values to stdout,
# full ffmpeg transcript to stderr. Returns nonzero when unmeasurable.
compute_ssim() {
  # The ffmpeg ssim line carries dB values in parentheses between the
  # components (e.g. "SSIM Y:0.95 (13.8) U:..."), so each component is
  # extracted separately.
  _log="$WORKDIR/ssim-$3.log"
  _pick() { grep -o " $1:[0-9.]*" "$_log" | tail -n 1 | cut -d: -f2; }
  if ffmpeg -y -v info -i "$1" -i "$2" -filter_complex "ssim" -f null - 2>"$_log"; then
    echo "SSIM Y:$(_pick Y) U:$(_pick U) V:$(_pick V) All:$(_pick All)"
    return 0
  fi
  echo "direct SSIM failed; retrying with normalized scale/fps on input 1:" >&2
  cat "$_log" >&2
  if ffmpeg -y -v info -i "$1" -i "$2" -filter_complex "[1:v]scale=1920:1080,fps=30[ref];[0:v][ref]ssim" -f null - 2>"$_log"; then
    echo "SSIM Y:$(_pick Y) U:$(_pick U) V:$(_pick V) All:$(_pick All)"
    echo "(normalized comparison disclosed)" >&2
    return 0
  fi
  echo "SSIM-FAILED for $3; see $_log" >&2
  return 1
}

header() {
  echo "# Benchmark — Framesmith vs HyperFrames (Sprint 4 T003)"
  echo ""
  echo "- date(UTC): $STAMP"
  echo "- machine: $OS"
  echo "- framesmith: $FSMITH_VERSION"
  echo "- ffmpeg(host): $FFMPEG_VERSION"
  echo "- hyperframes: $HF_STATE"
  echo "- hyperframes plan usage: $HF_PLAN"
  echo "- hyperframes quality flag: --quality $HF_QUALITY (tool default, disclosed; not a tuning flag)"
  echo "- reps: $REPS (each reference rendered this many times per tool)"
  echo ""
}

if [ "$CHECK_ONLY" -eq 1 ]; then
  header
  if [ "$HF_MISSING" -eq 1 ]; then
    echo "## Verdict: OPEN"
    echo ""
    echo "HyperFrames items stay OPEN: $HF_STATE."
    echo "No ratio is claimed. Provide the tool and re-run without --check-only."
    if [ -n "$OUT" ]; then header > "$OUT"; {
      echo "## Verdict: OPEN"
      echo ""
      echo "HyperFrames items stay OPEN: $HF_STATE."
      echo "No ratio is claimed. Provide the tool and re-run without --check-only."
    } >> "$OUT"; fi
    exit 2
  fi
  echo "## Verdict: READY"
  echo ""
  echo "Both tools are present; run without --check-only to measure."
  exit 0
fi

FAIL=0
REPORT="$WORKDIR/RESULTS.md"
header > "$REPORT"
echo "## Measured times (seconds, wall clock, same machine same session)" >> "$REPORT"
echo "" >> "$REPORT"
echo "| reference | tool | rep | seconds | mp4 |" >> "$REPORT"
echo "|---|---|---|---|---|" >> "$REPORT"

for REF in "reference-1:$REF1:$REF1_HF" "reference-2:$REF2:$REF2_HF"; do
  NAME="${REF%%:*}"
  REST="${REF#*:}"
  SRC="${REST%%:*}"
  SRC_HF="${REST#*:}"
  STAGE="$WORKDIR/$NAME"
  STAGE_HF="$WORKDIR/$NAME-hf"
  stage_example "$NAME" "$SRC" "$STAGE"
  stage_example "$NAME-hf" "$SRC_HF" "$STAGE_HF"

  echo "=== $NAME: framesmith check ===" >&2
  if ! "$FSMITH" check "$STAGE" >&2; then
    echo "BLOCKED: $NAME does not validate in its own tool; benchmark stays open for it." | tee -a "$REPORT"
    FAIL=1
    continue
  fi

  N=1
  while [ "$N" -le "$REPS" ]; do
    MP4="$WORKDIR/${NAME}-framesmith-r${N}.mp4"
    echo "=== $NAME: framesmith render rep $N/$REPS ===" >&2
    SECS="$(timed_render "$STAGE" "$MP4")" || { FAIL=1; break; }
    echo "RESULT ref=$NAME tool=framesmith rep=$N seconds=$SECS mp4=$MP4"
    echo "| $NAME | framesmith | $N | $SECS | \`$MP4\` |" >> "$REPORT"
    N=$((N + 1))
  done
done

echo ""
echo "## HyperFrames side"
if [ "$HF_MISSING" -eq 1 ]; then
  echo "OPEN: $HF_STATE."
  echo "Both HyperFrames sides stay OPEN with the missing tool recorded;"
  echo "nothing was authored, rendered, faked, or measured for them."
  echo "No SSIM and no speed ratio is claimed."
  {
    echo ""
    echo "## HyperFrames side: OPEN"
    echo ""
    echo "OPEN: $HF_STATE."
    echo "No SSIM and no speed ratio is claimed."
  } >> "$REPORT"
else
  {
    echo ""
    echo "## HyperFrames side: measured"
    echo ""
    echo "| reference | tool | rep | seconds | mp4 |"
    echo "|---|---|---|---|---|"
  } >> "$REPORT"
  for REF in "reference-1:$REF1_HF" "reference-2:$REF2_HF"; do
    NAME="${REF%%:*}"
    SRC_HF="${REF#*:}"
    STAGE_HF="$WORKDIR/$NAME-hf"
    if [ ! -d "$STAGE_HF" ]; then
      stage_example "$NAME-hf" "$SRC_HF" "$STAGE_HF"
    fi
    echo "=== $NAME: hyperframes check ===" >&2
    if ! npx --yes "$HF_PKG" check --json "$STAGE_HF" >&2; then
      echo "BLOCKED: $NAME-hf does not validate in its own tool (hyperframes check failed); benchmark stays open for it." | tee -a "$REPORT"
      FAIL=1
      continue
    fi
    N=1
    while [ "$N" -le "$REPS" ]; do
      MP4="$WORKDIR/${NAME}-hyperframes-r${N}.mp4"
      echo "=== $NAME: hyperframes render rep $N/$REPS ===" >&2
      SECS="$(timed_hf_render "$STAGE_HF" "$MP4")" || { FAIL=1; break; }
      echo "RESULT ref=$NAME tool=hyperframes rep=$N seconds=$SECS mp4=$MP4"
      echo "| $NAME | hyperframes | $N | $SECS | \`$MP4\` |" >> "$REPORT"
      N=$((N + 1))
    done
  done
  echo ""
  echo "## Picture similarity (SSIM, framesmith vs hyperframes video streams)"
  {
    echo ""
    echo "## Picture similarity (SSIM, framesmith vs hyperframes video streams)"
    echo ""
    echo "| reference | SSIM Y | SSIM U | SSIM V | SSIM All |"
    echo "|---|---|---|---|---|"
  } >> "$REPORT"
  for NAME in reference-1 reference-2; do
    FS_MP4="$WORKDIR/${NAME}-framesmith-r1.mp4"
    HF_MP4="$WORKDIR/${NAME}-hyperframes-r1.mp4"
    if [ -f "$FS_MP4" ] && [ -f "$HF_MP4" ]; then
      echo "=== $NAME: SSIM ===" >&2
      if SSIM_LINE="$(compute_ssim "$FS_MP4" "$HF_MP4" "$NAME")"; then
        echo "SSIM ref=$NAME ${SSIM_LINE#SSIM }"
        # shellcheck disable=SC2086
        set -- $SSIM_LINE
        # $1=SSIM $2=Y:.. $3=U:.. $4=V:.. $5=All:..
        echo "| $NAME | ${2#Y:} | ${3#U:} | ${4#V:} | ${5#All:} |" >> "$REPORT"
      else
        echo "SSIM ref=$NAME UNMEASURABLE (see $WORKDIR/ssim-$NAME.log)" | tee -a "$REPORT"
        FAIL=1
      fi
    else
      echo "SSIM ref=$NAME SKIPPED (missing MP4)" | tee -a "$REPORT"
      FAIL=1
    fi
  done
  echo ""
  echo "## Speed verdict (T004: strictly less wall time wins; a tie is not a win)"
  {
    echo ""
    echo "## Speed verdict (T004: strictly less wall time wins; a tie is not a win)"
    echo ""
  } >> "$REPORT"
  for NAME in reference-1 reference-2; do
    FS_MP4="$WORKDIR/${NAME}-framesmith-r1.mp4"
    HF_MP4="$WORKDIR/${NAME}-hyperframes-r1.mp4"
    FS_SECS="$(grep "^| $NAME | framesmith | 1 |" "$REPORT" | sed 's/^|[^|]*|[^|]*|[^|]*| \([0-9.]*\) |.*/\1/')"
    HF_SECS="$(grep "^| $NAME | hyperframes | 1 |" "$REPORT" | sed 's/^|[^|]*|[^|]*|[^|]*| \([0-9.]*\) |.*/\1/')"
    if [ -n "$FS_SECS" ] && [ -n "$HF_SECS" ]; then
      RATIO="$(python3 -c "print('%.3f' % ($HF_SECS / $FS_SECS))")"
      FASTER="$(python3 -c "print('faster' if float('$FS_SECS') < float('$HF_SECS') else 'shortfall')")"
      if [ "$FASTER" = "faster" ]; then
        LINE="- $NAME: framesmith ${FS_SECS}s vs hyperframes ${HF_SECS}s, ratio HF/FS = ${RATIO} -> FRAMESMITH FASTER"
      else
        LINE="- $NAME: framesmith ${FS_SECS}s vs hyperframes ${HF_SECS}s, ratio HF/FS = ${RATIO} -> SHORTFALL (framesmith not strictly faster; blocks a closed Sprint 4, measurement kept)"
      fi
      echo "$LINE"
      echo "$LINE" >> "$REPORT"
      echo "SPEED ref=$NAME framesmith_s=$FS_SECS hyperframes_s=$HF_SECS ratio_hf_over_fs=$RATIO verdict=$FASTER"
    else
      echo "SPEED ref=$NAME verdict=unmeasured (missing timing rows)"
      echo "- $NAME: verdict UNMEASURED (missing timing rows)" >> "$REPORT"
      FAIL=1
    fi
  done
fi

echo ""
echo "results table: $REPORT"
echo "workdir (MP4s kept): $WORKDIR"
if [ -n "$OUT" ]; then
  cp "$REPORT" "$OUT"
  echo "results copy: $OUT"
fi

if [ "$FAIL" -eq 1 ]; then
  echo "VERDICT: ERROR — a measurement failed; see lines above."
  exit 1
fi
if [ "$HF_MISSING" -eq 1 ]; then
  echo "VERDICT: OPEN — HyperFrames missing; benchmark items stay open."
  exit 2
fi
echo "VERDICT: measured (both sides ran; read ratios off the table)."
exit 0
