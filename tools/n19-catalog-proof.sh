#!/bin/sh
# n19-catalog-proof.sh -- Sprint 19 T009 one-command gate.
#
# Proves the catalog reference end to end on staged fixture copies
# plus the real bundled registry (materialized to a temp dir by the
# n19_shelf_probe helper built from this same checkout; the checkout
# itself is never written):
#   1. all 22 shelves hold original items with counts (probe prints
#      per-shelf counts; coverage is clean: manifests parse, KDL is
#      hygienic, no HyperFrames content);
#   2. a real bundled item shows and installs through the CLI;
#   3. a gated set refuses whole (names item/required/current, zero
#      writes), then the ungated set lands dependency-ordered;
#   4. local edits survive reinstall, --force restores registry bytes;
#   5. meaning consent declined (word search, no download), first ask
#      (explains, no download), granted (one download, on-device
#      ranking, revoke returns to word search);
#   6. a custom-registry-only item installs with its origin shown;
#   7. every starter inits and its untouched snapshot matches its
#      recorded preview digest (plus unknown-example and occupied-dir
#      refusals).
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n19-catalog-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory).
#   Evidence is copied to evidences/proof-n19-T009/ on every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n19-catalog-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n19-T009"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n19-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n19 catalog reference proof gate (T009) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "========================================================================="
echo ""

for NEED in cargo python3; do
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
CUR_VERSION="$("$FSMITH" --version 2>/dev/null | awk '{print $2}')"
echo "current version: ${CUR_VERSION:-unknown}"
echo "python: $(python3 --version 2>&1 || echo MISSING)"
echo ""

echo "--- build shelf probe helper ---"
if (cd "$FRAMESMITH_DIR" && cargo build -p catalog --example n19_shelf_probe >"$WORKDIR/build-probe.log" 2>&1); then
  echo "probe build ok"
else
  fail "probe-build" "cargo build of n19_shelf_probe failed; see $WORKDIR/build-probe.log"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
PROBE="$FRAMESMITH_DIR/target/debug/examples/n19_shelf_probe"

# ---- 1. shelves: all 22 hold originals with counts -------------------------
BUNDLED_REG="$WORKDIR/bundled-registry"
rm -rf "$BUNDLED_REG"
if "$PROBE" "$BUNDLED_REG" >"$WORKDIR/shelves.log" 2>&1; then
  echo "--- shelf counts (bundled registry) ---"
  cat "$WORKDIR/shelves.log"
  echo "---------------------------------------"
else
  fail "shelves-coverage-clean" "probe exited non-zero; see $WORKDIR/shelves.log"
fi
SHELF_LINES="$(grep -cE '^[a-z-]+: [0-9]+ \([0-9]+ block, [0-9]+ component, [0-9]+ example\)$' "$WORKDIR/shelves.log" 2>/dev/null || true)"
TOTAL_LINE="$(grep -E '^Bundled catalog: ' "$WORKDIR/shelves.log" 2>/dev/null || true)"
COVER_LINE="$(grep -E '^coverage: ' "$WORKDIR/shelves.log" 2>/dev/null || true)"
if [ "$SHELF_LINES" = "22" ]; then
  pass "shelves-22-present"
else
  fail "shelves-22-present" "found $SHELF_LINES shelf count lines, want 22"
fi
if [ "$TOTAL_LINE" = "Bundled catalog: 66 items across 22 shelves" ]; then
  pass "shelves-total-66-across-22"
else
  fail "shelves-total-66-across-22" "total line was: $TOTAL_LINE"
fi
THIN="$(python3 -c '
import re,sys
thin=[l for l in open(sys.argv[1]) if re.match(r"^[a-z-]+: \d+ \(", l) and int(l.split(":")[1].split()[0]) < 3]
print(len(thin))
' "$WORKDIR/shelves.log" 2>/dev/null || echo unknown)"
if [ "$THIN" = "0" ]; then
  pass "shelves-every-count-at-least-3"
else
  fail "shelves-every-count-at-least-3" "thin shelves: $THIN"
fi
if [ "$COVER_LINE" = "coverage: clean" ]; then
  pass "shelves-coverage-clean"
else
  fail "shelves-coverage-clean" "coverage line was: $COVER_LINE"
fi

# ---- 2. a real bundled item shows and installs ------------------------------
if "$FSMITH" catalog show ember-title-card --registry "$BUNDLED_REG" >"$WORKDIR/bundled-show.log" 2>&1 \
  && grep -q 'ember-title-card' "$WORKDIR/bundled-show.log" \
  && grep -q '"bundled"' "$WORKDIR/bundled-show.log"; then
  pass "bundled-show-names-item-and-origin"
else
  fail "bundled-show-names-item-and-origin" "see $WORKDIR/bundled-show.log"
fi
rm -rf "$WORKDIR/bundled-proj"
if (cd "$WORKDIR" && "$FSMITH" catalog install bundled-proj ember-title-card --registry "$BUNDLED_REG" >"$WORKDIR/bundled-install.log" 2>&1) \
  && grep -q 'install order: ember-title-card' "$WORKDIR/bundled-install.log" \
  && [ -f "$WORKDIR/bundled-proj/composition.kdl" ] \
  && (cd "$WORKDIR" && "$FSMITH" check bundled-proj >"$WORKDIR/bundled-check.log" 2>&1); then
  pass "bundled-install-lands-and-checks"
else
  fail "bundled-install-lands-and-checks" "see $WORKDIR/bundled-install.log $WORKDIR/bundled-check.log"
fi

# ---- fixture registry: dependency pair + gated item -------------------------
FIXREG="$WORKDIR/fixture-registry"
rm -rf "$FIXREG"
mkdir -p "$FIXREG/base" "$FIXREG/app" "$FIXREG/beacon"
cat >"$FIXREG/base/manifest.json" <<'EOF'
{
    "name": "base-plate",
    "type": "component",
    "title": "Base Plate",
    "description": "An original plate snippet other items build on.",
    "files": [{"src": "snippet.kdl", "target": "snippets/base.kdl"}],
    "tags": ["shapes"]
}
EOF
printf '// An original base plate snippet.\nbox x=80 y=520 width=560 height=120 fill="#ffffff" radius=12 opacity=0.96 start=0 duration=4\n' >"$FIXREG/base/snippet.kdl"
cat >"$FIXREG/app/manifest.json" <<'EOF'
{
    "name": "harbor-app",
    "type": "block",
    "title": "Harbor App",
    "description": "An original card built on the base plate.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 5,
    "tags": ["titles"],
    "registryDependencies": ["base-plate"]
}
EOF
printf 'composition name="app" width=640 height=360 duration=2 fps=30 background="#0b0b10" version=1 {\n    box x=40 y=40 width=260 height=130 fill="#e63946" radius=14 start=0 duration=2\n}\n' >"$FIXREG/app/composition.kdl"
cat >"$FIXREG/beacon/manifest.json" <<'EOF'
{
    "name": "future-beacon",
    "type": "component",
    "title": "Future Beacon",
    "description": "An original piece needing a newer CLI.",
    "files": [{"src": "snippet.kdl", "target": "snippets/future-beacon.kdl"}],
    "minCliVersion": "99.0.0"
}
EOF
printf '// An original future beacon snippet.\nbox x=80 y=120 width=560 height=96 fill="#fefae0" radius=12 start=0 duration=4\n' >"$FIXREG/beacon/snippet.kdl"

# ---- 3. gated set refuses whole, then lands ordered -------------------------
rm -rf "$WORKDIR/gated-proj"
if (cd "$WORKDIR" && "$FSMITH" catalog install gated-proj harbor-app future-beacon --registry "$FIXREG" >"$WORKDIR/gate.log" 2>&1); then
  fail "gated-refuses-whole" "install exited 0 for a gated set; see $WORKDIR/gate.log"
else
  if grep -q 'future-beacon' "$WORKDIR/gate.log" \
    && grep -q '99.0.0' "$WORKDIR/gate.log" \
    && grep -q "$CUR_VERSION" "$WORKDIR/gate.log" \
    && grep -q 'nothing was written' "$WORKDIR/gate.log" \
    && [ ! -e "$WORKDIR/gated-proj" ]; then
    pass "gated-refuses-whole (names item, required $CUR_VERSION-gated, zero writes)"
  else
    fail "gated-refuses-whole" "refusal must name item/required/current with zero writes; see $WORKDIR/gate.log"
  fi
fi
rm -rf "$WORKDIR/ordered-proj"
if (cd "$WORKDIR" && "$FSMITH" catalog install ordered-proj harbor-app --registry "$FIXREG" >"$WORKDIR/ordered.log" 2>&1) \
  && grep -q 'install order: base-plate -> harbor-app' "$WORKDIR/ordered.log" \
  && cmp -s "$WORKDIR/ordered-proj/snippets/base.kdl" "$FIXREG/base/snippet.kdl" \
  && cmp -s "$WORKDIR/ordered-proj/composition.kdl" "$FIXREG/app/composition.kdl"; then
  pass "ordered-install-lands (base-plate -> harbor-app, byte-identical)"
else
  fail "ordered-install-lands" "see $WORKDIR/ordered.log"
fi

# ---- 4. edits survive reinstall; force restores -----------------------------
rm -rf "$WORKDIR/edit-proj"
mkdir -p "$FIXREG/lantern"
cat >"$FIXREG/lantern/manifest.json" <<'EOF'
{
    "name": "lantern-card",
    "type": "block",
    "title": "Lantern Card",
    "description": "An original card for the edit-preservation gate.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 4,
    "tags": ["titles"]
}
EOF
printf 'composition name="lantern" width=1280 height=720 duration=4 fps=30 background="#14141c" version=1 {\n    box x=80 y=200 width=400 height=120 fill="#f4f1de" radius=16 border-width=4 border-color="#e07a5f" start=0 duration=4\n}\n' >"$FIXREG/lantern/composition.kdl"
if (cd "$WORKDIR" && "$FSMITH" catalog install edit-proj lantern-card --registry "$FIXREG" >"$WORKDIR/edit-install.log" 2>&1) \
  && [ -f "$WORKDIR/edit-proj/composition.kdl" ]; then
  pass "edit-gate-install-lands"
else
  fail "edit-gate-install-lands" "see $WORKDIR/edit-install.log"
fi
printf '// maker edit: warmer plate\n' >>"$WORKDIR/edit-proj/composition.kdl"
if (cd "$WORKDIR" && "$FSMITH" catalog install edit-proj lantern-card --registry "$FIXREG" >"$WORKDIR/edit-reinstall.log" 2>&1) \
  && grep -q 'kept 1 edited file(s)' "$WORKDIR/edit-reinstall.log" \
  && grep -q 'composition.kdl' "$WORKDIR/edit-reinstall.log" \
  && tail -n 1 "$WORKDIR/edit-proj/composition.kdl" | grep -q 'maker edit'; then
  pass "edit-survives-reinstall (kept, listed by name)"
else
  fail "edit-survives-reinstall" "see $WORKDIR/edit-reinstall.log"
fi
if (cd "$WORKDIR" && "$FSMITH" catalog install edit-proj lantern-card --force --registry "$FIXREG" >"$WORKDIR/edit-force.log" 2>&1) \
  && grep -q 'force-restored 1 file(s)' "$WORKDIR/edit-force.log" \
  && cmp -s "$WORKDIR/edit-proj/composition.kdl" "$FIXREG/lantern/composition.kdl"; then
  pass "force-restores-registry-bytes"
else
  fail "force-restores-registry-bytes" "see $WORKDIR/edit-force.log"
fi

# ---- 5. meaning consent: declined, first ask, granted, revoke ----------------
MEANREG="$WORKDIR/meaning-registry"
rm -rf "$MEANREG"
mkdir -p "$MEANREG/harbor-light" "$MEANREG/evening-tide"
cat >"$MEANREG/harbor-light/manifest.json" <<'EOF'
{"name": "harbor-light", "type": "block", "title": "Lantern Room",
 "description": "An original miniature about a keeper polishing lenses.",
 "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
 "width": 1280, "height": 720, "duration": 4, "tags": ["beacons"]}
EOF
cat >"$MEANREG/evening-tide/manifest.json" <<'EOF'
{"name": "evening-tide", "type": "block", "title": "Harbor Study",
 "description": "A quiet sketch of gulls over low water.",
 "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
 "width": 1280, "height": 720, "duration": 4, "tags": ["sketches"]}
EOF
MODEL="framesmith-meaning-tiny-v1"
MEAN0="$WORKDIR/meaning-decline"
rm -rf "$MEAN0"; mkdir -p "$MEAN0"
if (cd "$WORKDIR" && FRAMESMITH_MEANING_DIR="$MEAN0" "$FSMITH" catalog meaning-search harbor --consent no --registry "$MEANREG" >"$WORKDIR/consent-no.log" 2>&1) \
  && grep -q 'word search' "$WORKDIR/consent-no.log" \
  && grep -q 'harbor-light' "$WORKDIR/consent-no.log" \
  && [ ! -e "$MEAN0/model.bin" ]; then
  pass "consent-declined-keeps-word-search (no download)"
else
  fail "consent-declined" "see $WORKDIR/consent-no.log"
fi
MEAN1="$WORKDIR/meaning-ask"
rm -rf "$MEAN1"; mkdir -p "$MEAN1"
if (cd "$WORKDIR" && FRAMESMITH_MEANING_DIR="$MEAN1" "$FSMITH" catalog meaning-search harbor --registry "$MEANREG" >"$WORKDIR/consent-ask.log" 2>&1) \
  && grep -q "$MODEL" "$WORKDIR/consent-ask.log" \
  && grep -q -- '--consent yes' "$WORKDIR/consent-ask.log" \
  && grep -q 'harbor-light' "$WORKDIR/consent-ask.log" \
  && [ ! -e "$MEAN1/model.bin" ]; then
  pass "consent-first-ask-explains-and-downloads-nothing"
else
  fail "consent-first-ask" "see $WORKDIR/consent-ask.log"
fi
MEAN2="$WORKDIR/meaning-yes"
rm -rf "$MEAN2"; mkdir -p "$MEAN2"
if (cd "$WORKDIR" && FRAMESMITH_MEANING_DIR="$MEAN2" "$FSMITH" catalog meaning-search harbor --consent yes --registry "$MEANREG" >"$WORKDIR/consent-yes.log" 2>&1) \
  && grep -q 'meaning' "$WORKDIR/consent-yes.log" \
  && grep -q "$MODEL" "$WORKDIR/consent-yes.log" \
  && grep -q 'harbor-light' "$WORKDIR/consent-yes.log" \
  && [ -f "$MEAN2/model.bin" ] && [ -f "$MEAN2/consent.json" ]; then
  pass "consent-granted-downloads-once-and-ranks"
else
  fail "consent-granted" "see $WORKDIR/consent-yes.log"
fi
if (cd "$WORKDIR" && FRAMESMITH_MEANING_DIR="$MEAN2" "$FSMITH" catalog meaning-search harbor --registry "$MEANREG" >"$WORKDIR/consent-again.log" 2>&1) \
  && grep -q 'meaning' "$WORKDIR/consent-again.log" \
  && ! grep -q -- '--consent yes' "$WORKDIR/consent-again.log"; then
  pass "consent-stored-model-ranks-without-reasking"
else
  fail "consent-stored-model" "see $WORKDIR/consent-again.log"
fi
if (cd "$WORKDIR" && FRAMESMITH_MEANING_DIR="$MEAN2" "$FSMITH" catalog meaning-revoke >"$WORKDIR/consent-revoke.log" 2>&1) \
  && grep -q 'word search' "$WORKDIR/consent-revoke.log" \
  && [ ! -e "$MEAN2/model.bin" ]; then
  pass "consent-revoke-returns-to-word-search"
else
  fail "consent-revoke" "see $WORKDIR/consent-revoke.log"
fi

# ---- 6. custom-registry-only item installs ----------------------------------
TEAMREG="$WORKDIR/team-registry"
rm -rf "$TEAMREG" "$WORKDIR/custom-proj"
mkdir -p "$TEAMREG/team-banner" "$WORKDIR/custom-proj"
cat >"$TEAMREG/team-banner/manifest.json" <<'EOF'
{
    "name": "team-banner",
    "type": "component",
    "title": "Team Banner",
    "description": "An original shared banner snippet from the team registry.",
    "files": [{"src": "snippet.kdl", "target": "snippets/team-banner.kdl"}],
    "tags": ["team"]
}
EOF
printf '// An original team banner snippet.\nbox x=80 y=120 width=560 height=96 fill="#fefae0" radius=12 border-width=2 border-color="#606c38" start=0 duration=4\n' >"$TEAMREG/team-banner/snippet.kdl"
printf '{"registries": [{"name": "team", "path": "../team-registry"}]}' >"$WORKDIR/custom-proj/framesmith.json"
if (cd "$WORKDIR" && "$FSMITH" catalog install custom-proj team-banner --registry "$FIXREG" >"$WORKDIR/custom.log" 2>&1) \
  && grep -q 'install order: team-banner' "$WORKDIR/custom.log" \
  && grep -q 'team-banner \[team\]' "$WORKDIR/custom.log" \
  && cmp -s "$WORKDIR/custom-proj/snippets/team-banner.kdl" "$TEAMREG/team-banner/snippet.kdl"; then
  pass "custom-registry-only-item-installs (origin [team], byte-identical)"
else
  fail "custom-registry-only-item" "see $WORKDIR/custom.log"
fi

# ---- 7. starters render matching their digests ------------------------------
STARTERS_OK=1
: >"$WORKDIR/starter-digests.log"
for STARTER in title-card lower-third color-grid; do
  rm -rf "$WORKDIR/starter-$STARTER"
  if ! (cd "$WORKDIR" && "$FSMITH" init "starter-$STARTER" --example "$STARTER" >"$WORKDIR/starter-$STARTER-init.log" 2>&1); then
    fail "starter-init-$STARTER" "see $WORKDIR/starter-$STARTER-init.log"
    STARTERS_OK=0
    continue
  fi
  mkdir -p "$WORKDIR/starter-$STARTER-out"
  if ! (cd "$WORKDIR" && "$FSMITH" snapshot --at 0 --out "$WORKDIR/starter-$STARTER-out" "starter-$STARTER" >"$WORKDIR/starter-$STARTER-snap.log" 2>&1); then
    fail "starter-snapshot-$STARTER" "see $WORKDIR/starter-$STARTER-snap.log"
    STARTERS_OK=0
    continue
  fi
  PNG="$WORKDIR/starter-$STARTER-out/snap-0s.png"
  if [ ! -f "$PNG" ]; then
    fail "starter-snapshot-$STARTER" "no snap-0s.png; see $WORKDIR/starter-$STARTER-snap.log"
    STARTERS_OK=0
    continue
  fi
  DIGEST="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$PNG")"
  echo "$STARTER $DIGEST" >>"$WORKDIR/starter-digests.log"
  EXPECTED="$(grep -A6 "name: \"$STARTER\"" "$FRAMESMITH_DIR/crates/cli/src/starters.rs" | grep -o '[0-9a-f]\{64\}' | head -n 1)"
  if [ -z "$EXPECTED" ]; then
    fail "starter-digest-$STARTER" "no recorded digest in starters.rs"
    STARTERS_OK=0
  elif [ "$DIGEST" = "$EXPECTED" ]; then
    pass "starter-renders-digest-$STARTER ($DIGEST)"
  else
    fail "starter-digest-$STARTER" "rendered $DIGEST, recorded $EXPECTED"
    STARTERS_OK=0
  fi
done
if [ "$STARTERS_OK" = "1" ]; then
  pass "all-starters-render-matching-digests"
fi
if (cd "$WORKDIR" && "$FSMITH" init ghost --example no-such-starter >"$WORKDIR/starter-unknown.log" 2>&1); then
  fail "starter-unknown-refuses" "init exited 0 for an unknown starter"
else
  if grep -q 'title-card' "$WORKDIR/starter-unknown.log" && [ ! -e "$WORKDIR/ghost" ]; then
    pass "starter-unknown-refuses-listing-available (nothing created)"
  else
    fail "starter-unknown-refuses" "see $WORKDIR/starter-unknown.log"
  fi
fi
mkdir -p "$WORKDIR/occupied"
printf 'already here\n' >"$WORKDIR/occupied/keep.txt"
BEFORE_OCCUPIED="$(ls -A "$WORKDIR/occupied")"
if (cd "$WORKDIR" && "$FSMITH" init occupied --example title-card >"$WORKDIR/starter-occupied.log" 2>&1); then
  fail "starter-occupied-refuses" "init exited 0 into a non-empty directory"
else
  if [ "$(ls -A "$WORKDIR/occupied")" = "$BEFORE_OCCUPIED" ]; then
    pass "starter-occupied-refuses (non-empty target, nothing created)"
  else
    fail "starter-occupied-refuses" "refused but modified the directory; see $WORKDIR/starter-occupied.log"
  fi
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
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/shelves.log" "$WORKDIR/starter-digests.log" \
  "$WORKDIR/gate.log" "$WORKDIR/ordered.log" "$WORKDIR/edit-reinstall.log" \
  "$WORKDIR/edit-force.log" "$WORKDIR/consent-no.log" "$WORKDIR/consent-ask.log" \
  "$WORKDIR/consent-yes.log" "$WORKDIR/consent-again.log" "$WORKDIR/consent-revoke.log" \
  "$WORKDIR/custom.log" "$WORKDIR/bundled-show.log" "$WORKDIR/bundled-install.log" \
  "$WORKDIR/starter-unknown.log" "$WORKDIR/starter-occupied.log" "$EVID_DIR/" 2>/dev/null || true
for STARTER in title-card lower-third color-grid; do
  cp "$WORKDIR/starter-$STARTER-init.log" "$WORKDIR/starter-$STARTER-snap.log" "$EVID_DIR/" 2>/dev/null || true
done

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
