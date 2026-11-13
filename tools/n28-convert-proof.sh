#!/bin/sh
# n28-convert-proof.sh -- Sprint 28 T004 one-command sign-off gate.
#
# Proves the inspiration conversion + final sign-off end to end locally
# (CAP-001..CAP-005, J-01..J-05, STATE-01..STATE-05, INT-001..INT-003,
# REC-001..REC-005):
#   1. conversion completeness (T002 / CAP-004, CAP-003, J-02, STATE-02,
#      REC-001): every enumerated reference carries a fidelity.json
#      report and the batch accounting is complete (converted +
#      emitted-invalid + refused = enumerated, 0 unaccounted), with
#      measured deltas on every approximated entry and CSS-hyphenated
#      style attributes in every emitted KDL. "Converted" reuses the
#      engine parser via `check` (converter success alone never counts).
#   2. converted-KDL render leg (INT-001): framesmith renders every
#      check-clean converted project from a staged copy; the checkout
#      itself is never rendered into.
#   3. sweep outcomes (T003 / CAP-005, J-03/J-04/J-05, STATE-03/04/05,
#      REC-004/005): INSTANT checks off T003's recorded measurements
#      in swept/RESULTS.ndjson + swept/SWEEP.md -- this proof does NOT
#      re-measure the whole sweep. It recomputes the faster-and-better
#      verdict from the recorded medians, verifies agreement with the
#      recorded verdicts, and fails naming the residual when fewer than
#      49 references were executed.
#   4. single-pass byte-identical spot checks (durability): two renders
#      of one project are byte-identical; a re-conversion reproduces
#      the on-disk output byte-for-byte.
#   5. CI file shape: the three legs (macOS, Windows, Linux) with
#      fail-fast false and identical cargo commands; YAML parses.
#      A live green run needs the owner push and is reported as
#      PENDING, never claimed.
#   6. targeted cargo tests for the touched crates (cli n28 suite).
#      The full --workspace suite is NOT run here.
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n28-convert-proof.sh [--workdir DIR] [--bin PATH]
#          [--inspirations DIR] [--converted DIR] [--swept DIR]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; converted projects are
#   staged to DIR first. Evidence is copied to
#   evidences/proof-n28-convert/ on every run. The committed reports
#   in converted/ and swept/ are never overwritten: fresh proof output
#   lands in the workdir and is copied under evidences/.
#
# Subset disclosure (never a full-sweep claim):
#   gate 2 executes on the check-clean converted subset only (2 of 49);
#   emitted-invalid refs have no renderable KDL by design; refused refs
#   have no KDL to render. Gate 3 executes 49 of 49 RECORDED sweep
#   outcomes off T003's files -- 4 HyperFrames renders were measured
#   with decodable comparisons trusted from T003, the rest are named
#   statistical-only misses. Gate 4 re-converts 1 of 4
#   converter-success refs plus 1 refusal spot-check.
#
# Durable rules this gate relies on: KDL style attributes are
# CSS-hyphenated (checked in gate 1); the glyph and layout crates are
# untouched by this sprint (only the cli crate is touched: convert
# command + tests); single-pass renders stay byte-identical (checked
# here); ffmpeg resolves from the ERR own registry latest (the local
# gate uses PATH ffmpeg for probing, the binary bootstraps its pinned
# build at render time); the script runs on POSIX runners -- Windows
# path joins and ffmpeg.exe handling are covered by the CI matrix legs
# (gate 5), not by this shell. The binary is invoked by path
# ("$FSMITH"), never by PATH joins.
#
set -u

WORKDIR=""
BIN_OVERRIDE=""
INSP="/Users/budisantoso/Dev/bdsanxz/BeEditorWorkspace/skills/video/direcfy/inspirations"
CONV=""
SWEPT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    --inspirations) INSP="${2:-}"; shift 2 ;;
    --converted) CONV="${2:-}"; shift 2 ;;
    --swept) SWEPT="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,54p' "$0"; exit 0 ;;
    *) echo "n28-convert-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
CI_FILE="$FRAMESMITH_DIR/.github/workflows/ci.yml"
REL_FILE="$FRAMESMITH_DIR/.github/workflows/release.yml"
CONVERT_ALL="$FRAMESMITH_DIR/tools/n28-convert-all.sh"
SWEEP="$FRAMESMITH_DIR/tools/n28-sweep.sh"
if [ -z "$CONV" ]; then
  CONV="$FRAMESMITH_DIR/converted"
fi
if [ -z "$SWEPT" ]; then
  SWEPT="$FRAMESMITH_DIR/swept"
fi
EVID_DIR="$WS_ROOT/evidences/proof-n28-convert"
CONV_RESULTS="$CONV/RESULTS.ndjson"
CONV_SUMMARY="$CONV/SUMMARY.md"
SWEEP_RESULTS="$SWEPT/RESULTS.ndjson"
SWEEP_REPORT="$SWEPT/SWEEP.md"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n28-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n28 conversion proof gate (T004) =============="
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "================================================================"
echo ""

for NEED in cargo ffmpeg ffprobe python3 ruby; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in "$CI_FILE" "$REL_FILE" "$CONVERT_ALL" "$SWEEP" \
  "$CONV_RESULTS" "$CONV_SUMMARY" "$SWEEP_RESULTS" "$SWEEP_REPORT"; do
  if [ ! -f "$F" ]; then
    fail "prereq-file" "missing $F"
  fi
done
if [ ! -d "$INSP" ]; then
  fail "prereq-inspirations" "missing $INSP"
fi
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- missing prerequisites:$FAIL_LIST"
  exit 1
fi

# sha256_of FILE — hex digest.
sha256_of() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

# ---- build ---------------------------------------------------------------
if [ -n "$BIN_OVERRIDE" ]; then
  FSMITH="$BIN_OVERRIDE"
else
  FSMITH="$FRAMESMITH_DIR/target/debug/framesmith"
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
VER_LINE="$( "$FSMITH" --version 2>/dev/null || echo present )"
echo "framesmith: $VER_LINE"
echo "cargo: $(cargo --version 2>/dev/null)"
echo "rustc: $(rustc --version 2>/dev/null)"
echo "python: $(python3 --version 2>&1)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo ""

# ---- stage a tiny project (the checkout is never rendered into) ----------
echo "--- stage: 2s project (60 frames) ---"
mkdir -p "$WORKDIR/proj" || exit 1
cat >"$WORKDIR/proj/composition.kdl" <<'KDL'
composition name="t" width=320 height=180 duration=2 fps=30 background="#112233" version=1 {
    box x=10 y=10 width=120 height=60 fill="#ff0000" radius=4 border-width=2 border-color="#ffffff" opacity=1 order=1 start=0 duration=2
    text value="Hi" start=0 duration=2 x=140 y=40 size=20 color="#ffffff"
}
KDL
if [ -f "$WORKDIR/proj/composition.kdl" ]; then
  pass "staged-project (2s 60-frame composition, CSS-hyphenated attrs)"
else
  fail "stage-project" "staged composition.kdl missing"
fi
echo ""

# ---- gate 1: conversion completeness with fidelity reports (T002) ----------
echo "--- gate 1: every reference converts with a fidelity report ---"
python3 - "$CONV" "$INSP" "$CONV_RESULTS" "$CONV_SUMMARY" "$WORKDIR/gate1-counts.txt" >"$WORKDIR/gate1-audit.log" 2>&1 <<'PYEOF'
import glob, json, os, re, sys
conv, insp, results_path, summary_path, out_path = sys.argv[1:6]
def names_in(d):
    return sorted(x for x in os.listdir(d) if os.path.isdir(os.path.join(d, x)))
enumerated = names_in(insp)
out_dirs = [x for x in names_in(conv) if (conv + "/" + x + "/fidelity.json")] and sorted(
    x for x in names_in(conv) if os.path.exists(os.path.join(conv, x, "fidelity.json")))
results = {}
for line in open(results_path):
    parts = line.rstrip("\n").split("\t")
    if len(parts) >= 2:
        results[parts[0]] = parts[1]
summary = open(summary_path).read()
from collections import Counter
cnt = Counter(results.values())
has_summary_line = "executed: 49 of 49" in summary
total_entries = 0
approx_nod = 0
for f in sorted(glob.glob(conv + "/*/fidelity.json")):
    d = json.load(open(f))
    entries = d.get("entries", [])
    total_entries += len(entries)
    for e in entries:
        if e.get("verdict") == "approximated" and not (e.get("delta") or "").strip():
            approx_nod += 1
underscore = 0
for f in sorted(glob.glob(conv + "/*/composition.kdl")):
    txt = open(f).read()
    for a in re.findall(r'([A-Za-z][A-Za-z0-9_-]*)=', txt):
        if '_' in a:
            underscore += 1
fid_nonconverted = sorted(r for r, o in results.items() if o != "converted")
lines = {
    "N_ENUM": str(len(enumerated)),
    "N_FIDELITY": str(len(out_dirs)),
    "CONV_N": str(cnt.get("converted", 0)),
    "INV_N": str(cnt.get("emitted-invalid", 0)),
    "REFUSE_N": str(cnt.get("refused", 0)),
    "UNACC_N": str(cnt.get("unaccounted", 0)),
    "TOTAL_ENTRIES": str(total_entries),
    "APPROX_NOD": str(approx_nod),
    "UNDERSCORE": str(underscore),
    "HAS_SUMMARY_LINE": "1" if has_summary_line else "0",
    "RESIDUAL": " ".join(fid_nonconverted),
    "ENUMERATED": " ".join(enumerated),
    "OUT_DIRS": " ".join(out_dirs),
}
open(out_path, "w").write("\n".join("%s=%s" % (k, lines[k]) for k in sorted(lines)) + "\n")
print("audit ok: enumerated=%d fidelity=%d converted=%d invalid=%d refused=%d unaccounted=%d entries=%d approx_nod=%d underscore=%d" % (
    len(enumerated), len(out_dirs), cnt.get("converted", 0), cnt.get("emitted-invalid", 0),
    cnt.get("refused", 0), cnt.get("unaccounted", 0), total_entries, approx_nod, underscore))
PYEOF
G1="$WORKDIR/gate1-counts.txt"
if [ -f "$G1" ]; then
  pass "gate1-audit-ran ($(cat "$WORKDIR/gate1-audit.log"))"
else
  fail "gate1-audit" "gate1 python audit crashed; see gate1-audit.log"
fi
get1() { sed -n "s/^$1=//p" "$G1" 2>/dev/null; }
N_ENUM="$(get1 N_ENUM)"; N_FIDELITY="$(get1 N_FIDELITY)"
CONV_N="$(get1 CONV_N)"; INV_N="$(get1 INV_N)"; REFUSE_N="$(get1 REFUSE_N)"; UNACC_N="$(get1 UNACC_N)"
TOTAL_ENTRIES="$(get1 TOTAL_ENTRIES)"; APPROX_NOD="$(get1 APPROX_NOD)"; UNDERSCORE="$(get1 UNDERSCORE)"
RESIDUAL="$(get1 RESIDUAL)"
echo "gate1 counts: enumerated=$N_ENUM fidelity=$N_FIDELITY converted=$CONV_N emitted-invalid=$INV_N refused=$REFUSE_N unaccounted=$UNACC_N entries=$TOTAL_ENTRIES approx_nod=$APPROX_NOD underscore=$UNDERSCORE"
if [ "$N_ENUM" = "49" ] && [ "$N_FIDELITY" = "49" ] && [ "$UNACC_N" = "0" ] && [ "$(get1 HAS_SUMMARY_LINE)" = "1" ]; then
  pass "gate1-accounting-49-fidelity-reports (49/49 dirs carry fidelity.json, 0 unaccounted)"
else
  fail "gate1-accounting" "enumerated=$N_ENUM fidelity=$N_FIDELITY unaccounted=$UNACC_N summary-line=$(get1 HAS_SUMMARY_LINE); see gate1-audit.log"
fi
if [ "$APPROX_NOD" = "0" ]; then
  pass "gate1-delta-audit ($TOTAL_ENTRIES entries, 0 approximated-without-delta)"
else
  fail "gate1-delta" "$APPROX_NOD approximated entries lack a measured delta; see gate1-audit.log"
fi
if [ "$UNDERSCORE" = "0" ]; then
  pass "gate1-hyphenated-style-attrs (0 underscore attributes in emitted KDL)"
else
  fail "gate1-hyphenated" "$UNDERSCORE underscore attributes found in emitted KDL; see gate1-audit.log"
fi
if [ "$CONV_N" = "49" ]; then
  pass "gate1-49-converted (all 49 references converted on disk)"
else
  fail "gate1-49-converted" "only $CONV_N of 49 references converted (emitted-invalid $INV_N, refused $REFUSE_N); residual not converted:$RESIDUAL"
fi
# Parser reuse: live `check` must agree with the recorded check verdicts.
for REF in media-ledger-card name-card-template; do
  if "$FSMITH" check "$CONV/$REF" >"$WORKDIR/check-$REF.log" 2>&1; then
    pass "gate1-check-$REF (check exit 0, matches RESULTS ok)"
  else
    fail "gate1-check-$REF" "check exited non-zero for recorded-clean $REF; see check-$REF.log"
  fi
  if [ -f "$CONV/$REF/FIDELITY.md" ]; then
    pass "gate1-fidelity-md-$REF"
  else
    fail "gate1-fidelity-md-$REF" "FIDELITY.md missing for converted $REF"
  fi
done
if "$FSMITH" check "$CONV/lottie-json-showcase" >"$WORKDIR/check-lottie-json-showcase.log" 2>&1; then
  fail "gate1-check-lottie" "check exited 0 but RESULTS records fail:E_PARSE"
elif grep -q "E_PARSE" "$WORKDIR/check-lottie-json-showcase.log"; then
  pass "gate1-check-lottie-refuses-E_PARSE (matches recorded fail:E_PARSE)"
else
  fail "gate1-check-lottie" "check failed without E_PARSE; see check-lottie-json-showcase.log"
fi
if "$FSMITH" check "$CONV/one-card-many-videos" >"$WORKDIR/check-one-card-many-videos.log" 2>&1; then
  fail "gate1-check-one-card" "check exited 0 but RESULTS records fail:E_DUP_ID"
elif grep -q "E_DUP_ID" "$WORKDIR/check-one-card-many-videos.log"; then
  pass "gate1-check-one-card-refuses-E_DUP_ID (matches recorded fail:E_DUP_ID)"
else
  fail "gate1-check-one-card" "check failed without E_DUP_ID; see check-one-card-many-videos.log"
fi

# ---- gate 2: converted-KDL render leg -- subset 2 (INT-001) -----------------
echo ""
echo "--- gate 2: framesmith renders the converted projects cleanly (subset 2) ---"
echo "subset executed: $CONV_N of 49 (only check-clean converted refs; emitted-invalid refs fail check by design, refused refs have no KDL)"
for REF in media-ledger-card name-card-template; do
  STAGE="$WORKDIR/stage-$REF"
  rm -rf "$STAGE"; mkdir -p "$STAGE" || exit 1
  cp -R "$CONV/$REF/." "$STAGE/" || exit 1
  rm -rf "$STAGE/logs"
  MP4="$WORKDIR/$REF.mp4"
  if "$FSMITH" render "$STAGE" -o "$MP4" >"$WORKDIR/render-$REF.log" 2>&1 && [ -f "$MP4" ]; then
    if ffprobe -v error -show_entries stream=codec_name,width,height -of default=noprint_wrappers=1 "$MP4" 2>/dev/null | grep -q h264 \
      && ffprobe -v error -show_entries stream=width,height -of csv "$MP4" 2>/dev/null | grep -q 1920; then
      pass "gate2-render-$REF (exit 0, h264 1920-wide MP4 from staged copy)"
    else
      fail "gate2-probe-$REF" "MP4 probe misses h264/1920; see render-$REF.log"
    fi
  else
    fail "gate2-render-$REF" "render exited non-zero or wrote no MP4; see render-$REF.log"
  fi
done

# ---- gate 3: sweep recompute from T003's recorded measurements ----------------
echo ""
echo "--- gate 3: sweep recompute from T003 recorded measurements (no re-measure) ---"
echo "this proof does not invoke $SWEEP for measurement; verdicts are instant checks off swept/RESULTS.ndjson + swept/SWEEP.md"
python3 - "$SWEEP_RESULTS" "$SWEEP_REPORT" "$INSP" "$WORKDIR/sweep-recompute.txt" >"$WORKDIR/sweep-recompute.log" 2>&1 <<'PYEOF'
import os, re, sys
results_path, report_path, insp, out_path = sys.argv[1:5]
enumerated = sorted(x for x in os.listdir(insp) if os.path.isdir(os.path.join(insp, x)))
rows = [l.rstrip("\n").split("\t") for l in open(results_path) if l.strip()]
def recompute(r):
    fs, hf, speed, qual, verdict = r[1], r[2], r[3], r[4], r[5]
    if fs != "none" and hf != "none":
        speed_r = "faster" if float(fs) < float(hf) else "shortfall"
    else:
        speed_r = "unmeasured"
    verdict_r = "PASS" if (speed == "faster" and qual == "at-or-above") else "MISS"
    speed_ok = (speed_r == speed)
    return verdict_r, speed_ok
pass_n = miss_n = incons = 0
swept_names = []
faster_below = []
for r in rows:
    swept_names.append(r[0])
    vr, sok = recompute(r)
    if vr == "PASS":
        pass_n += 1
    else:
        miss_n += 1
    if vr != r[5] or not sok:
        incons += 1
        print("INCONSISTENT %s recorded verdict=%s recomputed=%s speed_recorded=%s speed_check=%s" % (r[0], r[5], vr, r[3], sok))
    if r[3] == "faster" and r[4] == "below":
        faster_below.append(r[0])
report = open(report_path).read()
exec_line = [l for l in report.splitlines() if "executed:" in l]
has_verdict_line = bool(exec_line) and ("49 of 49" in exec_line[0] and "passing: 0" in exec_line[0] and "missing: 49" in exec_line[0])
has_hf_counts = "hyperframes renders measured: 4" in report and "skipped with named cause" in report
missing = sorted(set(enumerated) - set(swept_names))
extra = sorted(set(swept_names) - set(enumerated))
_ssim = {}
for line in report.splitlines():
    if line.startswith("| media-ledger-card ") or line.startswith("| name-card-template "):
        ref = line.split("|")[1].strip()
        m = re.search(r"All:([0-9.]+)", line)
        if m:
            _ssim[ref] = m.group(1)
out = {
    "N_ENUM": str(len(enumerated)),
    "N_EXEC": str(len(rows)),
    "PASS_R": str(pass_n),
    "MISS_R": str(miss_n),
    "INCONS": str(incons),
    "HAS_VERDICT": "1" if has_verdict_line else "0",
    "HAS_HF": "1" if has_hf_counts else "0",
    "MISSING": " ".join(missing),
    "EXTRA": " ".join(extra),
    "FASTER_BELOW": " ".join(faster_below),
    "SSIM_MLC": _ssim.get("media-ledger-card", "?"),
    "SSIM_NCT": _ssim.get("name-card-template", "?"),
}
open(out_path, "w").write("\n".join("%s=%s" % (k, out[k]) for k in sorted(out)) + "\n")
print("recompute ok: enumerated=%d executed=%d pass_recomputed=%d miss_recomputed=%d inconsistent=%d missing=%d extra=%d faster_below=%s ssim(media-ledger-card=%s name-card-template=%s)" % (
    len(enumerated), len(rows), pass_n, miss_n, incons, len(missing), len(extra), out["FASTER_BELOW"], out["SSIM_MLC"], out["SSIM_NCT"]))
PYEOF
S3="$WORKDIR/sweep-recompute.txt"
if [ -f "$S3" ]; then
  pass "gate3-recompute-ran ($(cat "$WORKDIR/sweep-recompute.log"))"
else
  fail "gate3-recompute" "recompute crashed; see sweep-recompute.log"
fi
get3() { sed -n "s/^$1=//p" "$S3" 2>/dev/null; }
N_EXEC="$(get3 N_EXEC)"; PASS_R="$(get3 PASS_R)"; MISS_R="$(get3 MISS_R)"; INCONS="$(get3 INCONS)"
MISSING="$(get3 MISSING)"; EXTRA="$(get3 EXTRA)"; FASTER_BELOW="$(get3 FASTER_BELOW)"
echo "gate3 counts: executed=$N_EXEC pass_recomputed=$PASS_R miss_recomputed=$MISS_R inconsistent=$INCONS missing=${MISSING:-none} extra=${EXTRA:-none} faster-below=$FASTER_BELOW ssim(media-ledger-card=$(get3 SSIM_MLC) name-card-template=$(get3 SSIM_NCT))"
if [ "$N_EXEC" != "49" ]; then
  fail "gate3-executed-49" "only $N_EXEC of 49 recorded as executed; residual unexecuted:$MISSING"
else
  pass "gate3-executed-49 (all 49 enumerated references recorded in swept/RESULTS.ndjson)"
fi
if [ -n "$MISSING" ]; then
  fail "gate3-missing-residual" "sweep misses references:$MISSING"
else
  pass "gate3-no-missing-residual (swept names cover the 49 enumerated, no extras: extra=${EXTRA:-none})"
fi
if [ "$INCONS" = "0" ] && [ "$(get3 HAS_VERDICT)" = "1" ] && [ "$(get3 HAS_HF)" = "1" ]; then
  pass "gate3-recorded-verdicts-agree (recomputed faster-and-better matches recorded verdicts; SWEEP.md executed 49 passing 0 missing 49, hf measured 4/skipped 37)"
else
  fail "gate3-verdicts-agree" "inconsistent=$INCONS verdict-line=$(get3 HAS_VERDICT) hf-counts=$(get3 HAS_HF); see sweep-recompute.log"
fi
if [ "$PASS_R" = "49" ]; then
  pass "gate3-49-faster-and-better (every reference faster and at-or-above)"
else
  fail "gate3-49-faster-and-better" "$PASS_R of 49 pass faster-and-better recomputed, $MISS_R misses named (faster-but-below:$FASTER_BELOW; ssim media-ledger-card=$(get3 SSIM_MLC) name-card-template=$(get3 SSIM_NCT))"
fi

# ---- gate 4: single-pass byte-identical spot checks --------------------------
echo ""
echo "--- gate 4: single-pass byte-identical spot checks ---"
echo "subset: tiny project rendered twice; re-conversion 1 of 4 converter-success refs + 1 refusal spot-check"
if (cd "$WORKDIR" && "$FSMITH" render proj -o spot1.mp4 --backend cpu >"$WORKDIR/render-spot1.log" 2>&1) \
  && [ -f "$WORKDIR/spot1.mp4" ]; then
  pass "gate4-spot1-rendered"
else
  fail "gate4-spot1" "first spot render failed; see render-spot1.log"
fi
if (cd "$WORKDIR" && "$FSMITH" render proj -o spot2.mp4 --backend cpu >"$WORKDIR/render-spot2.log" 2>&1) \
  && [ -f "$WORKDIR/spot2.mp4" ]; then
  H1="$(sha256_of "$WORKDIR/spot1.mp4")"
  H2="$(sha256_of "$WORKDIR/spot2.mp4")"
  if [ "$H1" = "$H2" ]; then
    pass "gate4-single-pass-byte-identical (sha256 ${H1})"
  else
    fail "gate4-byte-identical" "two single-pass renders differ: ${H1} vs ${H2}"
  fi
else
  fail "gate4-spot2" "second spot render failed; see render-spot2.log"
fi
RECONV="$WORKDIR/reconvert-name-card-template"
rm -rf "$RECONV"; mkdir -p "$RECONV" || exit 1
if "$FSMITH" convert "$INSP/name-card-template" --out "$RECONV" --json >"$WORKDIR/reconvert.log" 2>&1 \
  && [ -f "$RECONV/composition.kdl" ] && [ -f "$RECONV/fidelity.json" ]; then
  RK1="$(sha256_of "$RECONV/composition.kdl")"; RK0="$(sha256_of "$CONV/name-card-template/composition.kdl")"
  RF1="$(sha256_of "$RECONV/fidelity.json")"; RF0="$(sha256_of "$CONV/name-card-template/fidelity.json")"
  if [ "$RK1" = "$RK0" ] && [ "$RF1" = "$RF0" ]; then
    pass "gate4-reconvert-byte-identical (composition.kdl + fidelity.json sha256 match)"
  else
    fail "gate4-reconvert" "re-convert differs from converted/ copy: ${RK1} vs ${RK0}; ${RF1} vs ${RF0}"
  fi
else
  fail "gate4-reconvert" "re-convert of name-card-template to temp failed; see reconvert.log"
fi
REFUSE_OUT="$WORKDIR/refuse-one-take-journey"
rm -rf "$REFUSE_OUT"; mkdir -p "$REFUSE_OUT" || exit 1
"$FSMITH" convert "$INSP/one-take-journey" --out "$REFUSE_OUT" --json >"$WORKDIR/refuse.log" 2>&1
REFUSE_CODE=$?
if [ "$REFUSE_CODE" -eq 1 ] && [ -f "$REFUSE_OUT/fidelity.json" ] && [ ! -f "$REFUSE_OUT/composition.kdl" ]; then
  pass "gate4-refusal-honest (exit 1 names cause in fidelity.json, no composition.kdl for one-take-journey)"
else
  fail "gate4-refusal" "refusal shape wrong (exit $REFUSE_CODE); see refuse.log"
fi
if [ ! -f "$CONV/name-card-template/composition.kdl" ] || [ ! -f "$CONV/media-ledger-card/composition.kdl" ]; then
  fail "gate4-converted-untouched" "converted/ KDL missing after proof staging (edited checkout?)"
else
  pass "gate4-converted-untouched (converted/ KDLs still present; all proof renders write to WORKDIR only)"
fi

# ---- gate 5: CI workflow shape --------------------------------------------
echo ""
echo "--- gate 5: CI workflow shape ---"
for RUNNER in macos-latest windows-latest ubuntu-latest; do
  if grep -q "$RUNNER" "$CI_FILE"; then
    pass "ci-leg-$RUNNER"
  else
    fail "ci-leg-$RUNNER" "runner $RUNNER absent from ci.yml"
  fi
done
if grep -q "fail-fast: false" "$CI_FILE"; then
  pass "ci-fail-fast-false"
else
  fail "ci-fail-fast" "fail-fast false absent from ci.yml"
fi
for CMD in "cargo build --workspace" "cargo test --workspace --exclude render" "cargo test -p render -- --test-threads=1"; do
  if grep -q "$CMD" "$CI_FILE"; then
    pass "ci-command-$CMD"
  else
    fail "ci-command-$CMD" "identical per-OS command '$CMD' absent from ci.yml"
  fi
done
if grep -q "9.0.2" "$CI_FILE" && grep -q "ffmpeg-registry" "$CI_FILE"; then
  pass "ci-ffmpeg-registry-pin-9.0.2"
else
  fail "ci-ffmpeg" "pinned ffmpeg-registry 9.0.2 absent from ci.yml"
fi
for SLUG in macos-arm64 macos-x64 windows-x64 linux-x64; do
  if grep -q "$SLUG" "$REL_FILE"; then
    pass "release-leg-$SLUG"
  else
    fail "release-leg-$SLUG" "slug $SLUG absent from release.yml"
  fi
done
if grep -q '"v\*"' "$REL_FILE" && grep -q "build-package.sh" "$REL_FILE"; then
  pass "release-tagged-only-via-build-package"
else
  fail "release-trigger" "v* tag trigger or build-package.sh absent from release.yml"
fi
if ruby -ryaml -e "ci=YAML.load_file('$CI_FILE'); rel=YAML.load_file('$REL_FILE'); raise 'ci no jobs' unless ci['jobs']; raise 'release no jobs' unless rel['jobs']; puts('ci jobs: ' + ci['jobs'].keys.join(',') + '; release jobs: ' + rel['jobs'].keys.join(','))" >"$WORKDIR/ci-yaml.log" 2>&1; then
  pass "ci-yaml-parses ($(cat "$WORKDIR/ci-yaml.log"))"
else
  fail "ci-yaml" "CI/release YAML failed parse; see ci-yaml.log"
fi
echo "INFO: CI live green run: PENDING (workflow authored and locally validated, not pushed per instructions; owner pushes)"

# ---- gate 6: targeted cargo tests (no --workspace) ---------------------------
echo ""
echo "--- gate 6: targeted cargo tests on touched crates (cli n28 suite) ---"
echo "touched crates: cli (convert_cmd + cli_n28_convert); glyph/layout untouched by this sprint; full --workspace suite NOT run here"
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p cli --test cli_n28_convert >"$WORKDIR/test-cli-n28.log" 2>&1); then
  if grep -q "test result: ok" "$WORKDIR/test-cli-n28.log" && grep -q "0 failed" "$WORKDIR/test-cli-n28.log"; then
    pass "cargo-test-cli-n28 ($(grep -h 'test result:' "$WORKDIR/test-cli-n28.log" | tr '\n' ';'))"
  else
    fail "cli-n28-summary" "cli n28 tests exited 0 without an ok/0-failed summary; see test-cli-n28.log"
  fi
  for TN in convert_one_reference_renders_and_land_timings convert_unparseable_refuses_naming_file convert_unsupported_call_refuses_naming_site convert_missing_asset_refuses_naming_asset convert_approximation_names_deltas convert_is_byte_deterministic; do
    if grep -q "$TN ... ok" "$WORKDIR/test-cli-n28.log"; then
      pass "targeted-test-$TN"
    else
      fail "targeted-test-$TN" "test did not report ok; see test-cli-n28.log"
    fi
  done
else
  fail "test-cli-n28" "cargo test -p cli --test cli_n28_convert failed; see test-cli-n28.log"
fi
grep -h "test result:" "$WORKDIR/test-cli-n28.log" 2>/dev/null | sed 's/^/INFO: cli-n28 /'

# ---- evidence ---------------------------------------------------------------
echo ""
echo "--- proof summary: $PASS_N passed, failing:${FAIL_LIST:- none} ---"
{
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $VER_LINE"
  echo "cargo: $(cargo --version 2>/dev/null)"
  echo "rustc: $(rustc --version 2>/dev/null)"
  echo "python: $(python3 --version 2>&1)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
  echo "gate1 subset: enumerated=$N_ENUM fidelity=$N_FIDELITY converted=$CONV_N emitted-invalid=$INV_N refused=$REFUSE_N unaccounted=$UNACC_N entries=$TOTAL_ENTRIES"
  echo "gate2 subset executed: 2 of 49 (media-ledger-card, name-card-template staged copies rendered)"
  echo "gate3 subset: 49 of 49 T003 recorded outcomes recomputed (hf renders measured 4, skipped 37); no re-measure"
  echo "gate4 subset: tiny project x2 + name-card-template re-convert + one-take-journey refusal spot-check"
  echo "CI live green run: PENDING (owner push)"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/build.log" \
  "$WORKDIR/gate1-counts.txt" "$WORKDIR/gate1-audit.log" \
  "$WORKDIR/check-media-ledger-card.log" "$WORKDIR/check-name-card-template.log" \
  "$WORKDIR/check-lottie-json-showcase.log" "$WORKDIR/check-one-card-many-videos.log" \
  "$WORKDIR/render-media-ledger-card.log" "$WORKDIR/render-name-card-template.log" \
  "$WORKDIR/sweep-recompute.txt" "$WORKDIR/sweep-recompute.log" \
  "$WORKDIR/render-spot1.log" "$WORKDIR/render-spot2.log" \
  "$WORKDIR/reconvert.log" "$WORKDIR/refuse.log" \
  "$WORKDIR/test-cli-n28.log" "$WORKDIR/ci-yaml.log" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/proj/composition.kdl" "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"

