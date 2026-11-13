#!/bin/sh
# n28-convert-all.sh -- Sprint 28 T002 batch conversion (CAP-004, J-02, STATE-02, REC-001).
#
# Converts every inspiration reference in the inspirations root into its
# Framesmith version under the output root: one `framesmith convert` per
# reference, each yielding either a KDL project tree (composition.kdl plus
# fidelity.json/FIDELITY.md plus copied assets/) or a named refusal
# (fidelity.json naming the file/call-site/asset, exit 1, no KDL). The
# sweep (T003) never marks an unconverted reference as passed; this
# script records each outcome honestly and continues through refusals.
#
# Usage:
#   n28-convert-all.sh [--inspirations DIR] [--out DIR] [--bin PATH] [--only a,b,c]
#   --inspirations  inspiration root (default: the BeEditorWorkspace
#                   direcfy inspirations dir; override for tests).
#   --out           output root (default: <framesmith>/converted).
#   --bin           framesmith binary, invoked by path (never PATH joins;
#                   Windows .exe handling stays with the CI matrix).
#   --only a,b,c    convert only these reference names (subset runs
#                   still account honestly; the summary says so).
#
# Gate: exit 0 only when every selected reference is accounted for
# (converted: composition.kdl whose KDL passes `check`; emitted-invalid:
# converter exit 0 but check fails; refused: fidelity.json, no KDL).
# "Converted" reuses the engine parser via `check` — converter success
# alone never counts as converted.
# Any unaccounted reference exits 1 naming it. Exit 2 = usage or
# tool-missing (never a pass on a guess). The output root is fresh per
# converted reference (rm -rf before convert) so a refused conversion
# never leaves a stale composition.kdl from an older run.
#
# Durable rules this script relies on: written KDL carries
# CSS-hyphenated style attributes from the converter itself; no render
# engine is touched, so the proof-verified single-pass outputs stay as
# proven; the binary is invoked by path ("$BIN").

set -u

INSP="/Users/budisantoso/Dev/bdsanxz/BeEditorWorkspace/skills/video/direcfy/inspirations"
OUTDIR=""
BIN_OVERRIDE=""
ONLY=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --inspirations) INSP="$2"; shift 2 ;;
    --out) OUTDIR="$2"; shift 2 ;;
    --bin) BIN_OVERRIDE="$2"; shift 2 ;;
    --only) ONLY="$2"; shift 2 ;;
    -h|--help) sed -n '1,45p' "$0"; exit 0 ;;
    *) echo "n28-convert-all.sh: unknown flag $1" >&2; exit 2 ;;
  esac
done

# --only takes comma-separated names; normalize to space-separated.
ONLY="$(printf '%s' "$ONLY" | tr ',' ' ')"

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"

if [ -z "$OUTDIR" ]; then
  OUTDIR="$FRAMESMITH_DIR/converted"
fi
if [ -z "$BIN_OVERRIDE" ]; then
  BIN="$FRAMESMITH_DIR/target/debug/framesmith"
else
  BIN="$BIN_OVERRIDE"
fi

if [ ! -x "$BIN" ]; then
  echo "building framesmith binary..." >&2
  (cd "$FRAMESMITH_DIR" && cargo build --bin framesmith) >&2 || exit 2
fi
if [ ! -d "$INSP" ]; then
  echo "n28-convert-all.sh: inspirations root not found: $INSP" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "n28-convert-all.sh: python3 is required" >&2
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

FSMITH_VERSION="$("$BIN" --version 2>/dev/null || echo 'framesmith (version query failed)')"
STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
OS="$(uname -srm)"

mkdir -p "$OUTDIR" || exit 2
RESULTS="$OUTDIR/RESULTS.ndjson"
: > "$RESULTS"

header() {
  echo "# Conversion summary -- 49 inspirations into Framesmith projects (Sprint 28 T002)"
  echo ""
  echo "- date(UTC): $STAMP"
  echo "- machine: $OS"
  echo "- framesmith: $FSMITH_VERSION (bin $BIN)"
  echo "- inspirations root: $INSP"
  echo "- enumerated references: $N_ENUM; converted in this run: $N_SELECT"
  echo "- output root: $OUTDIR"
  echo ""
  echo "| reference | outcome | translated | approximated | refused | assets | check | fidelity report |"
  echo "|---|---|---|---|---|---|---|---|---|"
}

REPORT="$OUTDIR/SUMMARY.md"
header > "$REPORT"

CONV_N=0
INV_N=0
REFUSE_N=0
UNACC_N=0
MISS_LIST=""
REFUSE_LIST=""

# ---- main loop: every selected reference, continue on refusal ----
for REF in $SELECTED; do
  SRC="$INSP/$REF"
  OUT="$OUTDIR/$REF"
  # Fresh per reference: a refused conversion must never leave a stale
  # composition.kdl from an older run.
  rm -rf "$OUT"
  mkdir -p "$OUT"

  _conv_log="$OUTDIR/$REF.convert.txt"
  if "$BIN" convert "$SRC" --out "$OUT" --json >"$_conv_log" 2>&1; then
    _code=0
  else
    _code=$?
  fi
  _json="$(grep '^{' "$_conv_log" | head -n 1)"
  _t="$(printf '%s' "$_json" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("translated","?"))' 2>/dev/null || echo '?')"
  _a="$(printf '%s' "$_json" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("approximated","?"))' 2>/dev/null || echo '?')"
  _r="$(printf '%s' "$_json" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("refused","?"))' 2>/dev/null || echo '?')"
  _assets="$(printf '%s' "$_json" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("assets_copied","?"))' 2>/dev/null || echo '?')"
  # Refusals print no --json line; fall back to the fidelity.json counts.
  if [ "$_t" = "?" ] && [ -f "$OUT/fidelity.json" ]; then
    _t="$(python3 -c "import json; print(json.load(open('$OUT/fidelity.json'))['counts'].get('translated','?'))" 2>/dev/null || echo '?')"
    _a="$(python3 -c "import json; print(json.load(open('$OUT/fidelity.json'))['counts'].get('approximated','?'))" 2>/dev/null || echo '?')"
    _r="$(python3 -c "import json; print(json.load(open('$OUT/fidelity.json'))['counts'].get('refused','?'))" 2>/dev/null || echo '?')"
    _assets="-"
  fi

  if [ "$_code" -eq 0 ] && [ -f "$OUT/composition.kdl" ] && [ -f "$OUT/fidelity.json" ]; then
    # "converted" means the emitted KDL parses: reuse the engine parser
    # via `check`. A converter-success output that fails check is
    # reported as emitted-invalid (named, never passed as converted).
    _chk_log="$OUTDIR/$REF.check.txt"
    if "$BIN" check "$OUT" >"$_chk_log" 2>&1; then
      _chk="ok"
    else
      _chk="fail"
    fi
    _ec="$(grep -o 'E_[A-Z_]*' "$_chk_log" | head -n 1)"
    if [ "$_chk" = "ok" ]; then
      CONV_N=$((CONV_N + 1))
      echo "RESULT ref=$REF outcome=converted t=$_t a=$_a r=$_r assets=$_assets check=ok"
      echo "$REF	converted	$_t	$_a	$_r	$_assets	ok" >> "$RESULTS"
      echo "| $REF | converted | $_t | $_a | $_r | $_assets | ok | $REF/fidelity.json, $REF/FIDELITY.md |" >> "$REPORT"
    else
      INV_N=$((INV_N + 1))
      echo "RESULT ref=$REF outcome=emitted-invalid t=$_t a=$_a r=$_r assets=$_assets check=fail:${_ec:-none}"
      echo "$REF	emitted-invalid	$_t	$_a	$_r	$_assets	fail:${_ec:-none}" >> "$RESULTS"
      echo "| $REF | emitted-invalid (converter exit 0, check fails ${_ec:-unknown}) | $_t | $_a | $_r | $_assets | fail ${_ec:-unknown} | $REF/fidelity.json, $REF.check.txt |" >> "$REPORT"
    fi
  elif [ "$_code" -eq 1 ] && [ -f "$OUT/fidelity.json" ] && [ ! -f "$OUT/composition.kdl" ]; then
    REFUSE_N=$((REFUSE_N + 1))
    REFUSE_LIST="$REFUSE_LIST $REF"
    _cause="$(python3 -c "import json; e=[x for x in json.load(open('$OUT/fidelity.json')).get('entries',[]) if x.get('verdict')=='refused']; print((e[0]['file']+':'+str(e[0]['line'])+' ['+e[0]['construct']+'] '+e[0]['detail'])[:300] if e else '(no refusal entry)')" 2>/dev/null || echo '(cause in fidelity.json)')"
    echo "RESULT ref=$REF outcome=refused t=$_t a=$_a r=$_r assets=$_assets check=- cause='$_cause'"
    echo "$REF	refused	$_t	$_a	$_r	$_assets	-" >> "$RESULTS"
    echo "| $REF | refused | $_t | $_a | $_r | $_assets | - | $REF/fidelity.json: $_cause |" >> "$REPORT"
  else
    UNACC_N=$((UNACC_N + 1))
    MISS_LIST="$MISS_LIST $REF"
    echo "RESULT ref=$REF outcome=unaccounted exit=$_code (see $REF.convert.txt)"
    echo "$REF	unaccounted	$_t	$_a	$_r	$_assets	-" >> "$RESULTS"
    echo "| $REF | unaccounted (exit $_code) | $_t | $_a | $_r | $_assets | - | $REF.convert.txt |" >> "$REPORT"
  fi
done

# ---- final verdict: refuse on any unaccounted reference ----
{
  echo ""
  echo "## Verdict"
  echo ""
  echo "- executed: $N_SELECT of $N_ENUM enumerated references; converted (emitted KDL passes check): $CONV_N; emitted-invalid (converter exit 0, check fails): $INV_N; refused with named cause: $REFUSE_N; unaccounted: $UNACC_N"
  if [ -n "$REFUSE_LIST" ]; then echo "- refused (named, not passed):$REFUSE_LIST"; fi
  if [ -n "$MISS_LIST" ]; then echo "- unaccounted (needs investigation):$MISS_LIST"; fi
  if [ "$N_SELECT" -lt "$N_ENUM" ]; then
    echo "- SUBSET: $((N_ENUM - N_SELECT)) reference(s) not converted in this run; no all-49 claim is made."
  fi
} >> "$REPORT"

echo ""
if [ "$UNACC_N" -gt 0 ]; then
  echo "VERDICT: FAIL -- $UNACC_N unaccounted reference(s)$MISS_LIST; executed $N_SELECT of $N_ENUM; converted $CONV_N; refused $REFUSE_N"
  exit 1
fi
if [ "$N_SELECT" -lt "$N_ENUM" ]; then
  echo "VERDICT: INCOMPLETE -- $((N_ENUM - N_SELECT)) reference(s) unconverted; gate refuses a subset pass"
  exit 1
fi
echo "VERDICT: DONE -- $N_ENUM references accounted for; converted $CONV_N, emitted-invalid $INV_N, refused $REFUSE_N"
exit 0

