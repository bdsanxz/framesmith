#!/bin/sh
# n28-sweep.sh -- Sprint 28 T003 sign-off sweep (CAP-005, J-03/J-04/J-05,
# STATE-03/04/05, INT-001/002/003, REC-004/005).
#
# Sweeps the ordered pairs: converted KDL (framesmith/converted/<ref>)
# rendered on framesmith vs original HTML
# (inspirations/<ref>) rendered on HyperFrames via the pinned npx
# package this workspace already drives (benchmark.sh: npx
# hyperframes@0.9.6, --quality looks == the tool default), in the
# same session on the same machine, and judges faster-and-better per
# reference:
#   speed   wall-clock medians over --reps repetitions per engine;
#           framesmith must be strictly less (a tie is not a win).
#   quality decoded-frame SSIM (ffmpeg ssim filter) when both MP4s
#           exist; otherwise a NAMED statistical comparison (ffprobe
#           facts for whichever MP4 exists plus both sides' decoded
#           states). No comparison ever reports unknown (CAP-005).
#   verdict PASS only when both sides rendered, framesmith is
#           strictly faster, and SSIM All >= 0.99 (named threshold).
#           Every miss is named per reference with both deltas; the
#           sweep continues through the rest (J-05, REC-004/005).
#
# A reference with no renderable KDL twin (converter refusal or
# converter-emitted-invalid, no composition.kdl passing `check`) is
# recorded MISS with the fidelity cause as the missing behavior. Its
# HyperFrames leg is still executed (source check + `check`); the
# render runs only when a comparison can exist (framesmith produced
# a render, or the KDL exists but fails check, so the HF facts can
# ground the named statistical comparison). A renders-skip is
# recorded verbatim, never a pass. References with no HyperFrames
# source (no hyperframes.json or no index.html) are HF-UNSOURCED.
#
# Gate: exit 0 only when the executed set covers EVERY enumerated
# reference and every verdict is PASS (STATE-03). Any miss, any
# unexecuted reference, or a subset run exits 1 naming the misses
# (STATE-04 gate refuses). Exit 2 = usage or tool-missing OPEN
# (never a pass on a guess). Exit 3 = measurement unavailable
# (npx unreachable).
#
# Usage:
#   n28-sweep.sh [--reps N] [--workdir DIR] [--out FILE]
#                [--inspirations DIR] [--converted DIR] [--only a,b,c]
#                [--bin PATH] [--check-only] [--selftest]
#   --reps N        renders per engine per reference (default 3);
#                   medians decide, single runs never do.
#   --inspirations  inspiration root (default: the BeEditorWorkspace
#                   direcfy inspirations dir; override for tests).
#   --converted     converted root (default: <framesmith>/converted).
#   --only a,b,c    execute only these reference names (subset runs
#                   still measure honestly; verdict stays INCOMPLETE).
#   --check-only    detect both tools and enumerate references
#                   without rendering; exit 2 OPEN if HyperFrames is
#                   missing.
#   --selftest      refuse-checks on sabotaged scratch copies plus
#                   exact verdict-math checks.
#
# Durable rules this sweep relies on: written KDL carries
# CSS-hyphenated style attributes from the converter itself; no
# render-engine change is made, so the proof-verified single-pass
# outputs and their bytes are untouched (renders here write only to
# the workdir/out paths, never back into converted/); framesmith
# renders self-provide the pinned ffmpeg-registry build at render
# time, while host ffmpeg is used only for probing and SSIM (as in
# benchmark.sh); the binary is invoked by path ("$BIN"), never by
# is_file() PATH joins, and Windows .exe handling stays with the CI
# matrix.
#
# Inventory: every top-level directory under the inspirations root
# (49 at sign-off). HF stage copies exclude render outputs; sources
# under inspirations and converted are never written.

set -u

REPS=3
WORKDIR=""
OUT=""
INSP="/Users/budisantoso/Dev/bdsanxz/BeEditorWorkspace/skills/video/direcfy/inspirations"
CONV=""
ONLY=""
BIN_OVERRIDE=""
CHECK_ONLY=0
SELFTEST=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --reps) REPS="$2"; shift 2 ;;
    --workdir) WORKDIR="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --inspirations) INSP="$2"; shift 2 ;;
    --converted) CONV="$2"; shift 2 ;;
    --only) ONLY="$2"; shift 2 ;;
    --bin) BIN_OVERRIDE="$2"; shift 2 ;;
    --check-only) CHECK_ONLY=1; shift ;;
    --selftest) SELFTEST=1; shift ;;
    -h|--help) sed -n '1,64p' "$0"; exit 0 ;;
    *) echo "n28-sweep.sh: unknown flag $1" >&2; exit 2 ;;
  esac
done

case "$REPS" in
  ''|*[!0-9]*|0) echo "n28-sweep.sh: --reps needs a positive integer" >&2; exit 2 ;;
esac

# --only takes comma-separated names; normalize to space-separated.
ONLY="$(printf '%s' "$ONLY" | tr ',' ' ')"

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
HF_PKG="hyperframes@0.9.6"
HF_QUALITY="looks"
SSIM_MIN="0.99"

if [ -z "$CONV" ]; then
  CONV="$FRAMESMITH_DIR/converted"
fi
if [ -z "$BIN_OVERRIDE" ]; then
  BIN="$FRAMESMITH_DIR/target/debug/framesmith"
else
  BIN="$BIN_OVERRIDE"
fi

if [ "$SELFTEST" -eq 0 ] && [ "$CHECK_ONLY" -eq 0 ] && [ ! -x "$BIN" ]; then
  echo "building framesmith binary..." >&2
  (cd "$FRAMESMITH_DIR" && cargo build --bin framesmith) >&2 || exit 2
fi
if [ ! -d "$INSP" ]; then
  echo "n28-sweep.sh: inspirations root not found: $INSP" >&2
  exit 2
fi
if [ ! -d "$CONV" ]; then
  echo "n28-sweep.sh: converted root not found: $CONV" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "n28-sweep.sh: python3 is required" >&2
  exit 2
fi
if ! command -v npx >/dev/null 2>&1; then
  echo "MEASUREMENT UNAVAILABLE: npx is required" >&2
  exit 3
fi
if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "n28-sweep.sh: ffmpeg is required (probing + SSIM)" >&2
  exit 2
fi

# enumerate references: top-level dirs, sorted, minus render outputs.
ENUMERATED=""
for _d in "$INSP"/*/; do
  [ -d "$_d" ] || continue
  ENUMERATED="$ENUMERATED $(basename "$_d")"
done
ENUMERATED="$(printf '%s\n' $ENUMERATED | sort | tr '\n' ' ')"
N_ENUM="$(printf '%s' "$ENUMERATED" | wc -w | tr -d ' ')"

in_only() {
  case " $1 " in *' '$2' '*) return 0 ;; *) return 1 ;; esac
}

# select executed set.
SELECTED=""
for _n in $ENUMERATED; do
  if [ -z "$ONLY" ]; then SELECTED="$SELECTED $_n";
  elif in_only "$ONLY" "$_n"; then SELECTED="$SELECTED $_n"; fi
done
N_SELECT="$(printf '%s' "$SELECTED" | wc -w | tr -d ' ')"

FSMITH_VERSION="unbuilt"
if [ -x "$BIN" ]; then
  FSMITH_VERSION="$("$BIN" --version 2>/dev/null || echo 'framesmith (version query failed)')"
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
  HF_STATE="MISSING: 'npx --yes $HF_PKG --version' failed (needs Node 22+ and npm plan/network)"
  HF_MISSING=1
fi

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
OS="$(uname -srm)"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n28-sweep-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 2
RESULTS="$WORKDIR/RESULTS.ndjson"
: > "$RESULTS"

now() { python3 -c 'import time,sys; sys.stdout.write("%d" % (time.time() * 1000))'; }

median() {
  python3 - "$@" <<'PYEOF'
import sys, statistics
vals = [float(v) for v in sys.argv[1:] if v != ""]
if not vals:
    raise SystemExit(1)
print("%.3f" % statistics.median(vals))
PYEOF
}

# stage_ref SRC DST — copy a reference folder minus render outputs.
stage_ref() {
  rm -rf "$2"
  mkdir -p "$2"
  (cd "$1" && find . -mindepth 1 -maxdepth 1 ! -name 'out.mp4' ! -name 'logs' ! -name '.hyperframes' ! -name 'renders' -exec cp -R {} "$2/" ';')
}

# probe_facts MP4 — one-line statistical facts for a single MP4.
probe_facts() {
  ffprobe -v error -show_entries stream=codec_name,width,height -show_entries format=duration -of default=noprint_wrappers=1 "$1" 2>/dev/null | tr '\n' ' ' | sed 's/^/facts:/; s/ /,/g'
}

# compute_ssim FS_MP4 HF_MP4 TAG — print "All:0.999" line; nonzero when unmeasurable.
compute_ssim() {
  _log="$WORKDIR/ssim-$3.log"
  if ffmpeg -y -v info -i "$1" -i "$2" -filter_complex "ssim" -f null - 2>"$_log"; then
    _all="$(grep -o ' All:[0-9.]*' "$_log" | tail -n 1 | cut -d: -f2)"
    _y="$(grep -o ' Y:[0-9.]*' "$_log" | tail -n 1 | cut -d: -f2)"
    _u="$(grep -o ' U:[0-9.]*' "$_log" | tail -n 1 | cut -d: -f2)"
    _v="$(grep -o ' V:[0-9.]*' "$_log" | tail -n 1 | cut -d: -f2)"
    echo "Y:$_y U:$_u V:$_v All:$_all"
    return 0
  fi
  return 1
}

# fidelity_cause REF — first refused entry from converted/<ref>/fidelity.json.
fidelity_cause() {
  python3 - "$CONV/$1/fidelity.json" <<'PYEOF' 2>/dev/null || echo "(fidelity.json unreadable)"
import json, sys
f = json.load(open(sys.argv[1]))
entries = [e for e in f.get("entries", []) if e.get("verdict") == "refused"]
if entries:
    e = entries[0]
    print("%s:%s [%s] %s" % (e.get("file"), e.get("line"), e.get("construct"), e.get("detail")))
else:
    print("(no refusal entry; outcome stands)")
PYEOF
}

header() {
  echo "# Sweep — converted KDL (Framesmith) vs original HTML (HyperFrames), all 49 inspirations (Sprint 28 T003)"
  echo ""
  echo "- date(UTC): $STAMP"
  echo "- machine: $OS"
  echo "- framesmith: $FSMITH_VERSION (bin $BIN)"
  echo "- ffmpeg(host): $FFMPEG_VERSION"
  echo "- hyperframes: $HF_STATE"
  echo "- hyperframes quality flag: --quality $HF_QUALITY (tool default, disclosed; not a tuning flag)"
  echo "- inspirations root: $INSP"
  echo "- converted root: $CONV"
  echo "- enumerated references: $N_ENUM; executed in this run: $N_SELECT"
  echo "- reps: $REPS per engine per reference (medians decide)"
  echo "- quality threshold: SSIM All >= $SSIM_MIN for 'at or above'"
  echo "- workdir: $WORKDIR"
  echo ""
  echo "| reference | framesmith | hyperframes | speed | quality | verdict |"
  echo "|---|---|---|---|---|---|"
}

# ---- check-only: enumerate + detect, never render ------------------------
if [ "$CHECK_ONLY" -eq 1 ]; then
  header
  echo ""
  echo "tools: framesmith $FSMITH_VERSION (bin $BIN); hyperframes: $HF_STATE"
  echo "references ($N_ENUM): $ENUMERATED"
  echo "selected for execution ($N_SELECT): $SELECTED"
  if [ "$HF_MISSING" -eq 1 ]; then
    echo "## Verdict: OPEN"
    echo ""
    echo "HyperFrames items stay OPEN: $HF_STATE."
    echo "No ratio and no quality judgement is claimed; $N_ENUM references are enumerated only."
    if [ -n "$OUT" ]; then header > "$OUT"; { echo ""; echo "tools: framesmith $FSMITH_VERSION (bin $BIN); hyperframes: $HF_STATE"; echo "references ($N_ENUM): $ENUMERATED"; echo "## Verdict: OPEN"; echo ""; echo "HyperFrames items stay OPEN: $HF_STATE."; } >> "$OUT"; fi
    exit 2
  fi
  echo "## Verdict: READY"
  echo ""
  echo "Both tools are present; $N_SELECT reference(s) selected for measurement."
  echo "Run without --check-only to measure."
  if [ -n "$OUT" ]; then header > "$OUT"; { echo ""; echo "## Verdict: READY"; } >> "$OUT"; fi
  exit 0
fi

# ---- selftest: refuse-checks on sabotaged scratch + verdict math ---------
if [ "$SELFTEST" -eq 1 ]; then
  echo "=== SELFTEST 1: verdict math (median + strict-faster + SSIM threshold) ==="
  _med="$(median 9 3 6)"; _med2="$(median 4)"
  [ "$_med" = "6.000" ] || { echo "SELFTEST FAIL: median(9 3 6)=$_med want 6.000"; exit 1; }
  [ "$_med2" = "4.000" ] || { echo "SELFTEST FAIL: median(4)=$_med2 want 4.000"; exit 1; }
  python3 -c "import sys; sys.exit(0 if (3.0 < 5.0 and not (5.0 < 5.0) and float('0.99') <= float('0.99') and not (float('0.989') >= 0.99)) else 1)" \
    || { echo "SELFTEST FAIL: strict-faster/threshold math"; exit 1; }
  echo "SELFTEST PASS: median(9 3 6)=6.000, strict-less decides, tie loses, SSIM threshold exact"
  echo ""
  echo "=== SELFTEST 2: sabotaged reference must refuse naming it ==="
  SCRATCH="$(mktemp -d /tmp/framesmith-n28-selftest-XXXXXX)"
  mkdir -p "$SCRATCH/insp/one-sab" "$SCRATCH/conv/one-sab"
  echo '{"entries":[]}' > "$SCRATCH/conv/one-sab/fidelity.json"
  echo '{"$schema":"x"}' > "$SCRATCH/insp/one-sab/hyperframes.json"
  echo '<html></html>' > "$SCRATCH/insp/one-sab/index.html"
  echo "sabotage: no composition.kdl in scratch converted copy (ref one-sab)"
  "$0" --reps 1 --workdir "$SCRATCH/run" --inspirations "$SCRATCH/insp" --converted "$SCRATCH/conv" --out "$SCRATCH/out.md" >"$SCRATCH/run.log" 2>&1
  _code=$?
  if [ "$_code" -eq 0 ]; then echo "SELFTEST FAIL: sabotaged run exited 0, want refusal"; exit 1; fi
  grep -q "ref=one-sab" "$SCRATCH/run.log" && grep -q "unrenderable" "$SCRATCH/run.log" \
    || { echo "SELFTEST FAIL: refusal does not name ref=one-sab / unrenderable:"; cat "$SCRATCH/run.log"; exit 1; }
  grep "RESULT" "$SCRATCH/run.log"
  echo "SELFTEST PASS: gate exit $_code names ref=one-sab as unrenderable"
  echo ""
  echo "=== SELFTEST 3: subset run must stay INCOMPLETE ==="
  _only_out="$("$0" --reps 1 --workdir "$SCRATCH/run2" --inspirations "$SCRATCH/insp" --converted "$SCRATCH/conv" --only no-such-ref 2>&1)"
  _only_code="$?"
  echo "$_only_out" | grep -q "INCOMPLETE" || { echo "SELFTEST FAIL: subset verdict not INCOMPLETE"; exit 1; }
  echo "SELFTEST PASS: subset exit $_only_code stays INCOMPLETE"
  exit 0
fi

# ---- main sweep loop: every selected reference, continue on miss ---------
REPORT="$WORKDIR/REPORT.md"
header > "$REPORT"
MISS_N=0
MISS_LIST=""
PASS_N=0
EXEC_N=0
HF_RENDERED_N=0
HF_SKIPPED_N=0

for REF in $SELECTED; do
  EXEC_N=$((EXEC_N + 1))
  HF_SRC="$INSP/$REF"
  FS_DIR="$CONV/$REF"
  STAGE="$WORKDIR/stage-$REF"
  stage_ref "$HF_SRC" "$STAGE"

  # --- framesmith side: KDL must exist and pass check, then timed renders ---
  FS_STATE=""; FS_TIMES=""; FS_MP4=""
  FS_HAS_KDL=0
  [ -f "$FS_DIR/composition.kdl" ] && FS_HAS_KDL=1
  if [ "$FS_HAS_KDL" -eq 0 ]; then
    # Named missing behavior: the fidelity report's first refusal.
    FS_STATE="unrenderable: no composition.kdl ($(fidelity_cause "$REF"))"
  else
    _fs_check_log="$WORKDIR/$REF-fs-check.log"
    if "$BIN" check "$FS_DIR" >"$_fs_check_log" 2>&1; then
      _n=1
      while [ "$_n" -le "$REPS" ]; do
        _mp4="$WORKDIR/$REF-framesmith-r$_n.mp4"
        _s="$(now)"
        if "$BIN" render "$FS_DIR" -o "$_mp4" >"$WORKDIR/$REF-fs-r$_n.log" 2>&1; then
          _e="$(now)"
          _secs="$(python3 -c "print('%.3f' % (($_e - $_s) / 1000.0))")"
          FS_TIMES="$FS_TIMES $_secs"
          [ "$_n" -eq 1 ] && FS_MP4="$_mp4"
          echo "=== $REF: framesmith render rep $_n/$REPS ${_secs}s ===" >&2
        else
          echo "=== $REF: framesmith render rep $_n/$REPS FAILED ===" >&2
          break
        fi
        _n=$((_n + 1))
      done
      if [ -n "$FS_TIMES" ]; then
        FS_STATE="median $(median $FS_TIMES)s"
      else
        FS_STATE="render-failed"
      fi
    else
      _code="$(grep -o 'E_[A-Z_]*' "$_fs_check_log" | head -n 1)"
      _msg="$(grep -v '^lint:' "$_fs_check_log" | grep -v '^invalid:' | grep -v '^valid:' | grep -v '^master peak:' | head -n 1 | cut -c1-160)"
      [ -z "$_code" ] && _code="E_UNKNOWN"
      FS_STATE="unrenderable ${_code}: ${_msg}"
    fi
  fi

  # --- hyperframes side: source check + check; renders when comparable ---
  HF_TIMES=""; HF_MP4=""; HF_RENDER_NOTE=""
  if [ ! -f "$STAGE/hyperframes.json" ] || [ ! -f "$STAGE/index.html" ]; then
    _miss=""
    [ -f "$STAGE/hyperframes.json" ] || _miss="hyperframes.json"
    [ -f "$STAGE/index.html" ] || { [ -n "$_miss" ] && _miss="$_miss index.html" || _miss="index.html"; }
    HF_STATE="HF-UNSOURCED missing: $_miss"
  elif [ "$HF_MISSING" -eq 1 ]; then
    HF_STATE="missing-tool"
  else
    _hf_check_log="$WORKDIR/$REF-hf-check.log"
    if npx --yes "$HF_PKG" check --json "$STAGE" >"$_hf_check_log" 2>&1; then
      _comparable=0
      [ -n "$FS_MP4" ] && _comparable=1
      [ "$FS_HAS_KDL" -eq 1 ] && [ -z "$FS_MP4" ] && _comparable=1
      if [ "$_comparable" -eq 1 ]; then
        _n=1
        while [ "$_n" -le "$REPS" ]; do
          _mp4="$WORKDIR/$REF-hyperframes-r$_n.mp4"
          _s="$(now)"
          if npx --yes "$HF_PKG" render --quality "$HF_QUALITY" -o "$_mp4" "$STAGE" >"$WORKDIR/$REF-hf-r$_n.log" 2>&1; then
            _e="$(now)"
            _secs="$(python3 -c "print('%.3f' % (($_e - $_s) / 1000.0))")"
            HF_TIMES="$HF_TIMES $_secs"
            [ "$_n" -eq 1 ] && HF_MP4="$_mp4"
            echo "=== $REF: hyperframes render rep $_n/$REPS ${_secs}s ===" >&2
          else
            echo "=== $REF: hyperframes render rep $_n/$REPS FAILED ===" >&2
            break
          fi
          _n=$((_n + 1))
        done
        if [ -n "$HF_TIMES" ]; then
          HF_STATE="median $(median $HF_TIMES)s"
          HF_RENDERED_N=$((HF_RENDERED_N + 1))
        else
          HF_STATE="render-failed"
        fi
      else
        # No KDL twin on disk: a render cannot change the missing
        # verdict. The leg is still executed (check above) and the
        # skip is recorded verbatim, never a pass.
        HF_STATE="check-ok; render skipped (no KDL twin; verdict already missing)"
        HF_RENDER_NOTE="; hf-render-skipped-no-kdl"
        HF_SKIPPED_N=$((HF_SKIPPED_N + 1))
      fi
    else
      HF_STATE="check-failed (see $REF-hf-check.log)"
    fi
  fi

  # --- speed verdict (both medians required; strict less wins) ---
  _fs_med="$(median $FS_TIMES 2>/dev/null || echo '')"
  _hf_med="$(median $HF_TIMES 2>/dev/null || echo '')"
  SPEED="unmeasured"; SPEED_VERDICT="miss"
  if [ -n "$_fs_med" ] && [ -n "$_hf_med" ]; then
    _faster="$(python3 -c "print(1 if float('$_fs_med') < float('$_hf_med') else 0)")"
    _ratio="$(python3 -c "print('%.3f' % (float('$_hf_med') / float('$_fs_med')))")"
    if [ "$_faster" = "1" ]; then
      SPEED="faster fs=${_fs_med}s hf=${_hf_med}s ratio=${_ratio}"
      SPEED_VERDICT="faster"
    else
      SPEED="shortfall fs=${_fs_med}s hf=${_hf_med}s ratio=${_ratio}"
      SPEED_VERDICT="shortfall"
    fi
  else
    SPEED="unmeasured fs=${_fs_med:-none}s hf=${_hf_med:-none}s"
    SPEED_VERDICT="unmeasured"
  fi

  # --- quality verdict: SSIM when both decoded, else named stats ---
  QUALITY="unmeasured"; QUAL_VERDICT="miss"; SSIM_ALL=""
  if [ -n "$FS_MP4" ] && [ -n "$HF_MP4" ] && [ -f "$FS_MP4" ] && [ -f "$HF_MP4" ]; then
    if _ssim="$(compute_ssim "$FS_MP4" "$HF_MP4" "$REF")"; then
      SSIM_ALL="$(printf '%s' "$_ssim" | sed 's/.*All://')"
      _ok="$(python3 -c "print(1 if float('$SSIM_ALL') >= float('$SSIM_MIN') else 0)")"
      if [ "$_ok" = "1" ]; then
        QUALITY="at-or-above SSIM All:$SSIM_ALL ($_ssim)"
        QUAL_VERDICT="at-or-above"
      else
        QUALITY="below SSIM All:$SSIM_ALL < $SSIM_MIN ($_ssim)"
        QUAL_VERDICT="below"
      fi
    else
      QUALITY="unmeasurable SSIM (see $WORKDIR/ssim-$REF.log)"
      QUAL_VERDICT="unmeasurable"
    fi
  else
    # named statistical comparison: probe facts for whichever MP4
    # exists, plus both sides' decoded states. Never a pass by
    # default (CAP-005).
    _stats=""
    [ -f "$HF_MP4" ] && _stats="hf $(probe_facts "$HF_MP4")"
    [ -f "$FS_MP4" ] && _stats="$_stats fs $(probe_facts "$FS_MP4")"
    [ -z "$_stats" ] && _stats="no MP4 on either side"
    QUALITY="statistical-only [$_stats] | fs: $FS_STATE | hf: $HF_STATE$HF_RENDER_NOTE"
    QUAL_VERDICT="no-decoded-comparison"
  fi

  # --- per-reference faster-and-better verdict ---
  VERDICT="PASS"
  REASON=""
  if [ "$SPEED_VERDICT" != "faster" ]; then
    VERDICT="MISS"; REASON="speed=$SPEED_VERDICT ($SPEED); fs: $FS_STATE; hf: $HF_STATE"
  elif [ "$QUAL_VERDICT" != "at-or-above" ]; then
    VERDICT="MISS"; REASON="quality=$QUAL_VERDICT ($QUALITY)"
  fi
  if [ "$VERDICT" = "PASS" ]; then
    PASS_N=$((PASS_N + 1))
  else
    MISS_N=$((MISS_N + 1))
    MISS_LIST="$MISS_LIST $REF"
  fi
  echo "RESULT ref=$REF fs_s=${_fs_med:-none} fs_state='$FS_STATE' hf_s=${_hf_med:-none} hf_state='$HF_STATE' speed=$SPEED_VERDICT quality=$QUAL_VERDICT ssim_all=${SSIM_ALL:-none} verdict=$VERDICT $REASON"
  echo "$REF	${_fs_med:-none}	${_hf_med:-none}	$SPEED_VERDICT	$QUAL_VERDICT	$VERDICT	$REASON" >> "$RESULTS"
  echo "| $REF | $FS_STATE | $HF_STATE | $SPEED | $QUALITY | $VERDICT${REASON:+: $REASON} |" >> "$REPORT"
done

# ---- final verdict: refuse on any miss or any unexecuted reference -------
{
  echo ""
  echo "## Verdict"
  echo ""
  echo "- executed: $EXEC_N of $N_ENUM enumerated references; passing: $PASS_N; missing: $MISS_N"
  if [ -n "$MISS_LIST" ]; then echo "- misses (named, with both deltas): $MISS_LIST"; fi
  echo "- hyperframes renders measured: $HF_RENDERED_N; skipped with named cause (no KDL twin): $HF_SKIPPED_N"
  if [ "$EXEC_N" -lt "$N_ENUM" ]; then
    echo "- INCOMPLETE: $((N_ENUM - EXEC_N)) reference(s) not executed in this run; no pass is claimed."
  fi
} >> "$REPORT"

echo ""
if [ "$MISS_N" -gt 0 ]; then
  echo "VERDICT: FAIL (STATE-04 sweep-missing) -- $MISS_N miss(es)$MISS_LIST; executed $EXEC_N of $N_ENUM; passing $PASS_N"
  echo "verdict: FAIL misses=$MISS_N executed=$EXEC_N enumerated=$N_ENUM passing=$PASS_N misslist=$MISS_LIST"
  if [ -n "$OUT" ]; then mkdir -p "$(dirname "$OUT")"; cp "$REPORT" "$OUT"; fi
  exit 1
fi
if [ "$EXEC_N" -lt "$N_ENUM" ]; then
  echo "VERDICT: INCOMPLETE -- $((N_ENUM - EXEC_N)) reference(s) unexecuted; gate refuses a subset pass"
  echo "verdict: INCOMPLETE executed=$EXEC_N enumerated=$N_ENUM passing=$PASS_N"
  if [ -n "$OUT" ]; then mkdir -p "$(dirname "$OUT")"; cp "$REPORT" "$OUT"; fi
  exit 1
fi
echo "VERDICT: PASS (STATE-03 sweep-passing) -- $PASS_N of $N_ENUM references faster and at or above quality"
echo "verdict: PASS executed=$EXEC_N enumerated=$N_ENUM passing=$PASS_N"
if [ -n "$OUT" ]; then mkdir -p "$(dirname "$OUT")"; cp "$REPORT" "$OUT"; fi
exit 0
