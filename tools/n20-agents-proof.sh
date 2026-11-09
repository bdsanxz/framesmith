#!/bin/sh
# n20-agents-proof.sh -- Sprint 20 T009 one-command gate.
#
# Proves the agent reference end to end on staged fixture copies (the
# checkout itself is never written):
#   1. skill install lands clean and `check` reports zero drift
#      explicitly (plus the 21-skill list and full-install workflow
#      contracts with gates and deferred sprints);
#   2. drift is named by file, `update` refuses without the maker
#      say-so (`--yes`), restores with it, and extras are named;
#   3. unknown hosts and unwritable locations refuse by name;
#   4. a routed request names its workflow with reasons and a
#      five-concept pitch, while ambiguity offers general-video plus
#      a Not-sure path instead of guessing;
#   5. the storyboard approval gate holds: status reads not approved,
#      the board locks stills without touching the film, render
#      refuses before approval, and approve lands the gate;
#   6. a staged Studio agent tool edit (POST /api/studio/agent)
#      returns a receipt and a shared-history turn the maker undoes
#      from that same history, restoring the film;
#   7. a resumed session reopens the brief plus locked storyboard,
#      reports the pending gate, writes nothing, then may proceed
#      once approved.
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n20-agents-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory).
#   Evidence is copied to evidences/proof-n20-T009/ on every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n20-agents-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EVID_DIR="$WS_ROOT/evidences/proof-n20-T009"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n20-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n20 agent reference proof gate (T009) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "========================================================================="
echo ""

for NEED in cargo curl python3 ffmpeg; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
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
CUR_VERSION="$("$FSMITH" --version 2>/dev/null | awk '{print $2}')"
echo "current version: ${CUR_VERSION:-unknown}"
echo "cargo: $(cargo --version 2>/dev/null)"
echo "rustc: $(rustc --version 2>/dev/null)"
echo "python: $(python3 --version 2>&1)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
echo ""

cleanup() {
  "$FSMITH" preview-stop-all >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM
"$FSMITH" preview-stop-all >/dev/null 2>&1 || true

# ---- gate 1: install lands clean, drift named, say-so restores ----
echo "--- gate 1: skill install, drift check, say-so update ---"
SKA="$WORKDIR/skills-a"
SKB="$WORKDIR/skills-b"
if "$FSMITH" skills install --host claude-code --dest "$SKA" >"$WORKDIR/install-a.log" 2>&1; then
  pass "install-clean (claude-code)"
else
  fail "install-clean" "install exited non-zero; see install-a.log"
fi
if "$FSMITH" skills check --host claude-code --dest "$SKA" >"$WORKDIR/check-clean.log" 2>&1; then
  if grep -q "zero drift" "$WORKDIR/check-clean.log"; then
    pass "check-reports-zero-drift-explicitly"
  else
    fail "check-zero-drift-text" "exit 0 but no 'zero drift' line; see check-clean.log"
  fi
else
  fail "check-clean-exit" "check exited non-zero on a clean install; see check-clean.log"
fi
if "$FSMITH" skills check --host claude-code --dest "$SKA" --json >"$WORKDIR/check-clean.json" 2>&1; then
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("clean") is True and not d.get("drifted") and not d.get("missing") and not d.get("extra") else 1)' "$WORKDIR/check-clean.json" 2>/dev/null; then
    pass "check-json-clean-true"
  else
    fail "check-json-clean" "JSON check is not clean:true; see check-clean.json"
  fi
else
  fail "check-json-exit" "json check exited non-zero; see check-clean.json"
fi
if "$FSMITH" skills list --json >"$WORKDIR/list.json" 2>&1; then
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); s=d["skills"]; sys.exit(0 if d.get("count")==21 and len(s)==21 and sum(1 for x in s if x["kind"]=="workflow")==10 and sum(1 for x in s if x["kind"]=="domain")==11 and all(x.get("purpose") for x in s) else 1)' "$WORKDIR/list.json" 2>/dev/null; then
    pass "list-names-21-skills (10 workflows + 11 domain, every purpose)"
  else
    fail "list-21-skills" "expected 10 workflows + 11 domain with purposes; see list.json"
  fi
else
  fail "list-exit" "skills list exited non-zero; see list.json"
fi
# Full install carries the ten creation-workflow contracts: gates plus
# deferred steps with the owning sprint named.
if "$FSMITH" skills update --all --host claude-code --dest "$SKA" --yes >"$WORKDIR/update-all.log" 2>&1; then
  DOC="$SKA/product-launch/SKILL.md"
  if [ -f "$DOC" ] && grep -q "Gates:" "$DOC" && grep -q "Deferred:" "$DOC" && grep -q "caption sprint" "$DOC"; then
    pass "workflow-contract-states-gates-and-deferred-sprint"
  else
    fail "workflow-contract-text" "product-launch/SKILL.md lacks Gates/Deferred/caption sprint; see update-all.log"
  fi
else
  fail "update-all" "update --all --yes exited non-zero; see update-all.log"
fi

# Drift on a second location: tamper, name, refuse-without-say-so,
# restore-with-say-so, extras named.
if "$FSMITH" skills install --host claude-code --dest "$SKB" >"$WORKDIR/install-b.log" 2>&1; then
  printf '\n<!-- local maker edit -->\n' >>"$SKB/router/SKILL.md"
  if "$FSMITH" skills check --host claude-code --dest "$SKB" >"$WORKDIR/check-drift.log" 2>&1; then
    fail "drift-check-nonzero" "check exited 0 on a tampered file; see check-drift.log"
  else
    if grep -q "router/SKILL.md" "$WORKDIR/check-drift.log"; then
      pass "drift-named-by-file (router/SKILL.md)"
    else
      fail "drift-names-file" "check failed without naming router/SKILL.md; see check-drift.log"
    fi
  fi
  if "$FSMITH" skills update --core --host claude-code --dest "$SKB" >"$WORKDIR/update-nosay.log" 2>&1; then
    fail "update-needs-say-so" "update without --yes exited 0; see update-nosay.log"
  else
    if grep -q -- "--yes" "$WORKDIR/update-nosay.log" && grep -q "local maker edit" "$SKB/router/SKILL.md"; then
      pass "update-without-say-so-refuses-and-changes-nothing"
    else
      fail "update-say-so-refusal" "refusal names no --yes or changed the file; see update-nosay.log"
    fi
  fi
  if "$FSMITH" skills update --skill router --host claude-code --dest "$SKB" --yes >"$WORKDIR/update-one.log" 2>&1; then
    if ! grep -q "local maker edit" "$SKB/router/SKILL.md" \
      && "$FSMITH" skills check --host claude-code --dest "$SKB" >"$WORKDIR/check-restored.log" 2>&1 \
      && grep -q "zero drift" "$WORKDIR/check-restored.log"; then
      pass "update-one-skill-restores-then-zero-drift"
    else
      fail "update-one-restore" "scoped restore did not return to zero drift; see update-one.log check-restored.log"
    fi
  else
    fail "update-one-exit" "update --skill router --yes exited non-zero; see update-one.log"
  fi
  printf 'extra\n' >"$SKB/router/EXTRA.md"
  if "$FSMITH" skills check --host claude-code --dest "$SKB" >"$WORKDIR/check-extra.log" 2>&1; then
    fail "extra-check-nonzero" "check exited 0 with an extra file present; see check-extra.log"
  else
    if grep -q "EXTRA.md" "$WORKDIR/check-extra.log"; then
      pass "extra-file-named-by-check"
    else
      fail "extra-names-file" "check failed without naming EXTRA.md; see check-extra.log"
    fi
  fi
  rm -f "$SKB/router/EXTRA.md"
else
  fail "install-second-dest" "install to second dest exited non-zero; see install-b.log"
fi

# Refusals name their cause.
if "$FSMITH" skills install --host hal9000 >"$WORKDIR/unknown-host.log" 2>&1; then
  fail "unknown-host-refuses" "install exited 0 for an unknown host"
else
  if grep -q "supported hosts" "$WORKDIR/unknown-host.log" && grep -q "claude-code" "$WORKDIR/unknown-host.log"; then
    pass "unknown-host-refuses-listing-supported-hosts"
  else
    fail "unknown-host-text" "refusal lists no supported hosts; see unknown-host.log"
  fi
fi
turns() {
  "$FSMITH" history list --json "$1" 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d.get("turns") or []))' 2>/dev/null
}
printf 'blocker\n' >"$WORKDIR/blocker-file"
if "$FSMITH" skills install --host claude-code --dest "$WORKDIR/blocker-file/nope" >"$WORKDIR/unwritable.log" 2>&1; then
  fail "unwritable-refuses" "install exited 0 into an unwritable location"
else
  if grep -q "blocker-file/nope" "$WORKDIR/unwritable.log"; then
    pass "unwritable-location-refuses-naming-the-path"
  else
    fail "unwritable-text" "refusal names no path; see unwritable.log"
  fi
fi

# ---- gate 2: routed workflow with the approval gate held ----
echo ""
echo "--- gate 2: route, storyboard gate, approve, render ---"
if "$FSMITH" route --request "a product launch film for our new headphones" --json >"$WORKDIR/route.json" 2>"$WORKDIR/route.err"; then
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); c=d.get("choice") or {}; rs=c.get("reasons") or []; ps=d.get("concepts") or []; sys.exit(0 if c.get("workflow")=="product-launch" and any("request mentions" in r for r in rs) and len(ps)==5 and all("Proposal:" in (p.get("hook") or "") for p in ps) else 1)' "$WORKDIR/route.json" 2>/dev/null; then
    pass "route-names-product-launch-with-reasons-and-five-concepts"
  else
    fail "route-choice" "choice/reasons/concepts wrong; see route.json"
  fi
else
  fail "route-exit" "route exited non-zero; see route.err"
fi
if "$FSMITH" route --request "make something nice for friday" --json >"$WORKDIR/route-amb.json" 2>"$WORKDIR/route-amb.err"; then
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("choice") is None and d.get("offered")==["general-video","not-sure"] else 1)' "$WORKDIR/route-amb.json" 2>/dev/null \
    && "$FSMITH" route --request "launch the slideshow" >"$WORKDIR/route-amb2.log" 2>&1 \
    && grep -q "chosen workflow: none (ambiguous; not guessing)" "$WORKDIR/route-amb2.log" \
    && grep -q "offer: general-video, not-sure" "$WORKDIR/route-amb2.log"; then
    pass "ambiguity-offers-general-video-plus-not-sure-never-guesses"
  else
    fail "route-ambiguity" "ambiguous request did not offer general-video + not-sure; see route-amb.json route-amb2.log"
  fi
else
  fail "route-ambiguity-exit" "ambiguous route exited non-zero; see route-amb.err"
fi
if "$FSMITH" route --request "a product launch film" --workflow warp-cut >"$WORKDIR/route-badwf.log" 2>&1; then
  fail "route-unknown-workflow-refuses" "route exited 0 for an unknown workflow"
else
  if grep -q "product-launch" "$WORKDIR/route-badwf.log"; then
    pass "unknown-workflow-refuses-listing-every-workflow"
  else
    fail "route-unknown-workflow-text" "refusal lists no workflows; see route-badwf.log"
  fi
fi

# The film project carries an id-addressable text clip for the agent.
FILM="$WORKDIR/film"
if (cd "$WORKDIR" && "$FSMITH" init film >"$WORKDIR/init-film.log" 2>&1); then
  if [ -f "$FILM/AGENTS.md" ] && [ -f "$FILM/CLAUDE.md" ] && grep -q "skills install" "$FILM/AGENTS.md"; then
    pass "new-project-receives-agent-guides (AGENTS.md + CLAUDE.md)"
  else
    fail "agent-guides" "guide files missing or silent on skills; see init-film.log"
  fi
else
  fail "init-film" "init film exited non-zero; see init-film.log"
fi
cat >"$FILM/composition.kdl" <<'EOF'
composition name="film" width=320 height=180 duration=4 fps=10 background="#112233" version=1 {
    text id="title" value="Hi" start=0 duration=4 x=10 y=40 size=24 color="#ffffff"
    box id="card" x=10 y=20 width=100 height=50 fill="#ffffff" start=2 duration=2
}
EOF
if "$FSMITH" check "$FILM" >"$WORKDIR/check-film.log" 2>&1; then
  pass "staged-film-validates"
else
  fail "staged-film-check" "check failed on the staged film; see check-film.log"
fi
if "$FSMITH" storyboard status "$FILM" >"$WORKDIR/status-missing.log" 2>&1; then
  if grep -q "not approved" "$WORKDIR/status-missing.log"; then
    pass "missing-approval-reads-as-not-approved"
  else
    fail "status-missing-text" "status names no not-approved; see status-missing.log"
  fi
else
  fail "status-missing-exit" "storyboard status exited non-zero; see status-missing.log"
fi
BEFORE_FILM="$(shasum -a 256 "$FILM/composition.kdl" | awk '{print $1}')"
BOARD_OUT="$WORKDIR/board"
if "$FSMITH" storyboard board --at 0 --out "$BOARD_OUT" "$FILM" >"$WORKDIR/board.log" 2>&1; then
  AFTER_FILM="$(shasum -a 256 "$FILM/composition.kdl" | awk '{print $1}')"
  if [ "$BEFORE_FILM" = "$AFTER_FILM" ] && grep -q "locked still(s)" "$WORKDIR/board.log" \
    && grep -q "film is unchanged" "$WORKDIR/board.log" \
    && [ -f "$BOARD_OUT/board.html" ] \
    && python3 -c 'import sys; sys.exit(0 if open(sys.argv[1],"rb").read(8)==b"\x89PNG\r\n\x1a\n" else 1)' "$BOARD_OUT/still-0000.png" 2>/dev/null; then
    pass "board-locks-stills-film-byte-identical-one-page-png"
  else
    fail "board-locks" "board changed the film or missed stills/page; see board.log"
  fi
else
  fail "board-exit" "storyboard board exited non-zero; see board.log"
fi
EARLY_MP4="$WORKDIR/early.mp4"
if "$FSMITH" render "$FILM" -o "$EARLY_MP4" >"$WORKDIR/render-early.log" 2>&1; then
  fail "unapproved-render-refuses" "render exited 0 before approval"
else
  if grep -q "storyboard" "$WORKDIR/render-early.log" && grep -q "approve" "$WORKDIR/render-early.log" \
    && [ ! -e "$EARLY_MP4" ] \
    && [ "$(shasum -a 256 "$FILM/composition.kdl" | awk '{print $1}')" = "$BEFORE_FILM" ]; then
    pass "unapproved-render-refused-at-gate-no-mp4-film-untouched"
  else
    fail "unapproved-render-text" "refusal names no gate or wrote output; see render-early.log"
  fi
fi
if "$FSMITH" storyboard approve --json "$FILM" >"$WORKDIR/approve.json" 2>"$WORKDIR/approve.err"; then
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("staged") is True and d.get("tool")=="approve-stills" and d.get("approved") is True and (d.get("turn") or 0)>=1 else 1)' "$WORKDIR/approve.json" 2>/dev/null \
    && "$FSMITH" storyboard status "$FILM" >"$WORKDIR/status-approved.log" 2>&1 \
    && grep -q "approved" "$WORKDIR/status-approved.log"; then
    pass "approve-lands-gate-as-shared-history-turn"
  else
    fail "approve-receipt" "approve receipt/status wrong; see approve.json status-approved.log"
  fi
else
  fail "approve-exit" "storyboard approve exited non-zero; see approve.err"
fi

# ---- gate 3: staged agent edit undone by the maker ----
echo ""
echo "--- gate 3: Studio agent tool edit + maker undo from shared history ---"
START_OUT="$("$FSMITH" preview "$FILM" 2>&1)"
START_CODE=$?
echo "$START_OUT" | head -n 3
if [ $START_CODE -ne 0 ]; then
  fail "preview-start" "exit $START_CODE: $START_OUT"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
URL="$(echo "$START_OUT" | grep -o 'http://127\.0\.0\.1:[0-9]*/' | head -n 1)"
if [ -z "$URL" ]; then
  fail "preview-start-url" "no loopback URL in preview output"
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
if curl -s "${URL}api/studio/context" -o "$WORKDIR/context.json" 2>/dev/null \
  && python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("composition")=="film" and "selection" in d else 1)' "$WORKDIR/context.json" 2>/dev/null; then
  pass "context-readout-names-composition-and-selection"
else
  fail "context-readout" "context names no composition/selection; see context.json"
fi
TURNS_BEFORE="$(turns "$FILM")"
printf '%s' '{"tool":"set-text","actor":"agent","target-id":"title","value":"Changed by agent"}' >"$WORKDIR/agent-set-text.json"
CODE="$(curl -s -o "$WORKDIR/agent-set-text.out" -w "%{http_code}" -X POST --data-binary "@$WORKDIR/agent-set-text.json" "${URL}api/studio/agent" 2>/dev/null)"
if [ "$CODE" = "200" ] \
  && python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("applied") is True and d.get("receipt") and (d.get("rows") is not None) else 1)' "$WORKDIR/agent-set-text.out" 2>/dev/null; then
  pass "agent-set-text-stages-with-receipt"
else
  fail "agent-set-text" "HTTP $CODE without an applied receipt; see agent-set-text.out"
fi
TURNS_AFTER="$(turns "$FILM")"
if [ -n "$TURNS_AFTER" ] && [ "$TURNS_AFTER" = "$((TURNS_BEFORE + 1))" ] \
  && "$FSMITH" history list --json "$FILM" >"$WORKDIR/history.json" 2>&1 \
  && python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); ts=d.get("turns") or []; sys.exit(0 if ts and ts[-1].get("actor")=="agent" else 1)' "$WORKDIR/history.json" 2>/dev/null; then
  pass "agent-edit-recorded-as-bracketed-agent-turn"
else
  fail "agent-history-turn" "no single agent turn chained the history (before=$TURNS_BEFORE after=$TURNS_AFTER); see history.json"
fi
# A write at a missing target refuses and stages nothing.
printf '%s' '{"tool":"set-text","actor":"agent","target-id":"ghost","value":"Nope"}' >"$WORKDIR/agent-missing.json"
CODE_MISS="$(curl -s -o "$WORKDIR/agent-missing.out" -w "%{http_code}" -X POST --data-binary "@$WORKDIR/agent-missing.json" "${URL}api/studio/agent" 2>/dev/null)"
TURNS_MISS="$(turns "$FILM")"
if [ "$CODE_MISS" = "409" ] && [ "$TURNS_MISS" = "$TURNS_AFTER" ] \
  && grep -q "ghost" "$WORKDIR/agent-missing.out"; then
  pass "agent-tool-at-missing-target-refuses-nothing-staged"
else
  fail "agent-missing-refusal" "HTTP $CODE_MISS, turns $TURNS_MISS; see agent-missing.out"
fi
# The maker undoes the agent turn from shared history: value restored.
if "$FSMITH" history undo --json "$FILM" >"$WORKDIR/undo.json" 2>"$WORKDIR/undo.err"; then
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("actor")=="agent" else 1)' "$WORKDIR/undo.json" 2>/dev/null \
    && grep -q 'value="Hi"' "$FILM/composition.kdl" \
    && "$FSMITH" check "$FILM" >"$WORKDIR/check-after-undo.log" 2>&1; then
    pass "maker-undo-reverses-agent-turn-film-restored"
  else
    fail "maker-undo-restores" "undo names no agent turn or value unrestored; see undo.json"
  fi
else
  fail "maker-undo-exit" "history undo exited non-zero; see undo.err"
fi

# ---- gate 4: resume from locked brief and storyboard ----
echo ""
echo "--- gate 4: resume from brief and storyboard ---"
RESUME_PROJ="$WORKDIR/tomorrow"
if (cd "$WORKDIR" && "$FSMITH" init tomorrow >"$WORKDIR/init-tomorrow.log" 2>&1); then
  pass "resume-project-inits"
else
  fail "init-tomorrow" "init tomorrow exited non-zero; see init-tomorrow.log"
fi
cat >"$RESUME_PROJ/brief.md" <<'EOF'
# Framesmith brief
Request: a product launch film about tea for teens
Workflow: product-launch
EOF
if "$FSMITH" storyboard board --at 0,2 --out "$WORKDIR/board2" "$RESUME_PROJ" >"$WORKDIR/board2.log" 2>&1; then
  pass "resume-project-board-locks"
else
  fail "board2-exit" "board on resume project exited non-zero; see board2.log"
fi
tree_sha() {
  (cd "$1" && find . -type f -not -path './board*' | sort | xargs shasum -a 256)
}
tree_sha "$RESUME_PROJ" >"$WORKDIR/tree-before.txt"
if "$FSMITH" resume --json "$RESUME_PROJ" >"$WORKDIR/resume-pending.json" 2>"$WORKDIR/resume-pending.err"; then
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("status")=="continue" and d.get("approved") is False and len(d.get("locked-frames") or [])==2 else 1)' "$WORKDIR/resume-pending.json" 2>/dev/null \
    && "$FSMITH" resume "$RESUME_PROJ" >"$WORKDIR/resume-pending.log" 2>&1 \
    && grep -q "awaits maker approval" "$WORKDIR/resume-pending.log"; then
    pass "resume-names-locks-and-pending-gate"
  else
    fail "resume-pending-text" "resume names no locks/pending gate; see resume-pending.json resume-pending.log"
  fi
else
  fail "resume-pending-exit" "resume exited non-zero; see resume-pending.err"
fi
tree_sha "$RESUME_PROJ" >"$WORKDIR/tree-after.txt"
if cmp -s "$WORKDIR/tree-before.txt" "$WORKDIR/tree-after.txt"; then
  pass "resume-writes-nothing"
else
  fail "resume-writes-nothing" "resume changed project files; see tree-before.txt tree-after.txt"
fi
if "$FSMITH" storyboard approve "$RESUME_PROJ" >"$WORKDIR/approve2.log" 2>&1 \
  && "$FSMITH" resume "$RESUME_PROJ" >"$WORKDIR/resume-may.log" 2>&1 \
  && grep -q "may proceed" "$WORKDIR/resume-may.log"; then
  pass "resume-after-approval-may-proceed"
else
  fail "resume-may-proceed" "resume after approval names no may-proceed; see approve2.log resume-may.log"
fi

# ---- gate 5: approved film renders ----
echo ""
echo "--- gate 5: approved render ---"
FINAL_MP4="$WORKDIR/final.mp4"
if "$FSMITH" render "$FILM" -o "$FINAL_MP4" >"$WORKDIR/render-final.log" 2>&1; then
  if [ -f "$FINAL_MP4" ] && [ "$(wc -c <"$FINAL_MP4")" -gt 1000 ]; then
    pass "approved-build-renders-real-mp4"
  else
    fail "approved-render-output" "render exited 0 but no real MP4; see render-final.log"
  fi
else
  fail "approved-render-exit" "render exited non-zero after approval; see render-final.log"
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
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/install-a.log" "$WORKDIR/check-clean.log" \
  "$WORKDIR/check-clean.json" "$WORKDIR/list.json" "$WORKDIR/update-all.log" \
  "$WORKDIR/install-b.log" "$WORKDIR/check-drift.log" "$WORKDIR/update-nosay.log" \
  "$WORKDIR/update-one.log" "$WORKDIR/check-restored.log" "$WORKDIR/check-extra.log" \
  "$WORKDIR/unknown-host.log" "$WORKDIR/unwritable.log" "$WORKDIR/route.json" \
  "$WORKDIR/route-amb.json" "$WORKDIR/route-amb2.log" "$WORKDIR/route-badwf.log" \
  "$WORKDIR/init-film.log" "$WORKDIR/check-film.log" "$WORKDIR/status-missing.log" \
  "$WORKDIR/board.log" "$WORKDIR/render-early.log" "$WORKDIR/approve.json" \
  "$WORKDIR/status-approved.log" "$WORKDIR/context.json" "$WORKDIR/agent-set-text.json" \
  "$WORKDIR/agent-set-text.out" "$WORKDIR/history.json" "$WORKDIR/agent-missing.json" \
  "$WORKDIR/agent-missing.out" "$WORKDIR/undo.json" "$WORKDIR/check-after-undo.log" \
  "$WORKDIR/init-tomorrow.log" "$WORKDIR/board2.log" "$WORKDIR/resume-pending.json" \
  "$WORKDIR/resume-pending.log" "$WORKDIR/tree-before.txt" "$WORKDIR/tree-after.txt" \
  "$WORKDIR/approve2.log" "$WORKDIR/resume-may.log" "$WORKDIR/render-final.log" \
  "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
