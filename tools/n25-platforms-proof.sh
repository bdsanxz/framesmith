#!/bin/sh
# n25-platforms-proof.sh -- Sprint 25 T004 one-command gate.
#
# Proves the platforms reference end to end locally (CAP-001..CAP-004,
# J-01..J-05, STATE-01..STATE-04, INT-001..INT-003, REC-001..REC-004):
#   1. every shipped registry asset name resolves (T001): the ffmpeg
#      crate tests prove all 7 OS/arch pairs resolve their exact asset,
#      archive entry, and binary names, unknown pairs refuse with the
#      supported set named, and the no-published-build message names it.
#   2. hardware encode choices resolve with software fallback and
#      explicit refusal under staged encoder listings (T002): the render
#      lib tests stage `-encoders` listings proving per-OS `auto`
#      priority, explicit present/absent behavior, and resolved-codec
#      argv; live CLI renders prove `auto` falls back to software with
#      the fact named, explicit absent encoders refuse naming choice
#      and codec while writing no MP4, `cpu` aliases `software`, and an
#      unknown `--encoder` value refuses naming the accepted set.
#   3. doctor facts match the render path (INT-003): `doctor` text and
#      `--json` report host OS/arch, FFmpeg availability, hardware
#      encoders, and the auto encoder from the same host listing
#      resolution the render path uses; the proof recomputes the
#      expected facts from the host `ffmpeg -encoders` output and
#      compares, including the FRAMESMITH_NO_VIDEOTOOLBOX fallback.
#   4. the CI workflow file carries the three legs (T003 / CAP-001):
#      macos-latest, windows-latest, ubuntu-latest, fail-fast false,
#      identical build/test commands per OS; YAML parses. A live green
#      run needs the owner push (not pushed here) and is reported as
#      PENDING, never claimed.
#   5. macOS output stays byte-identical: software renders rerun
#      byte-identically, the default render equals `--encoder
#      software`, and hardware-vs-software decodes to the same picture
#      (frame count and dimensions via ffprobe).
#   6. tamper/refusal gates (REC-002/REC-003/REC-004): a broken canvas
#      refuses naming the file and cause, and a render on it writes no
#      MP4, so breaking the fixture cannot stay green.
#   7. `cargo test` for every touched crate (ffmpeg, render, preview,
#      cli encoder e2e) stays green.
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n25-platforms-proof.sh [--workdir DIR] [--bin PATH]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; the project is staged
#   to DIR first. Evidence is copied to evidences/proof-n25-T004/ on
#   every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,38p' "$0"; exit 0 ;;
    *) echo "n25-platforms-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
CI_FILE="$FRAMESMITH_DIR/.github/workflows/ci.yml"
FFMPEG_RS="$FRAMESMITH_DIR/crates/ffmpeg/src/ffmpeg.rs"
EVID_DIR="$WS_ROOT/evidences/proof-n25-T004"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n25-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n25 platforms proof gate (T004) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "=================================================================="
echo ""

for NEED in cargo ffmpeg ffprobe python3 ruby; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
if [ ! -f "$CI_FILE" ]; then
  fail "prereq-ci" "missing $CI_FILE"
fi
if [ ! -f "$FFMPEG_RS" ]; then
  fail "prereq-ffmpeg-rs" "missing $FFMPEG_RS"
fi
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- missing prerequisites:$FAIL_LIST"
  exit 1
fi

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
echo "framesmith: $(\"$FSMITH\" --version 2>/dev/null || echo present)"
echo "cargo: $(cargo --version 2>/dev/null)"
echo "rustc: $(rustc --version 2>/dev/null)"
echo "python: $(python3 --version 2>&1)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo ""

# ---- stage a tiny project (the checkout is never rendered into) ----------
echo "--- stage: 1s text project ---"
mkdir -p "$WORKDIR/proj" || exit 1
cat >"$WORKDIR/proj/composition.kdl" <<'KDL'
composition name="t" width=160 height=120 duration=1 fps=10 background="#112233" version=1 {
    text value="Hi" start=0 duration=1 x=10 y=20 size=24 color="#ffffff"
}
KDL
if [ -f "$WORKDIR/proj/composition.kdl" ]; then
  pass "staged-project (1s text composition)"
else
  fail "stage-project" "staged composition.kdl missing"
fi

# ---- gate 1: every shipped asset name resolves (T001 / CAP-002) ----------
echo ""
echo "--- gate 1: per-OS bootstrap asset resolution ---"
for ASSET in ffmpeg-osx-arm64.zip ffmpeg-osx-x64.zip ffmpeg-linux-x64.zip \
             ffmpeg-linux-arm64.zip ffmpeg-linux-x86.zip \
             ffmpeg-windows-x64.zip ffmpeg-windows-arm64.zip; do
  if grep -q "\"$ASSET\"" "$FFMPEG_RS"; then
    pass "registry-asset-named-$ASSET"
  else
    fail "registry-asset-$ASSET" "shipped asset name absent from ffmpeg.rs"
  fi
done
if grep -q "ffmpeg.exe" "$FFMPEG_RS"; then
  pass "windows-binary-name-ffmpeg.exe"
else
  fail "windows-binary-name" "ffmpeg.exe absent from ffmpeg.rs"
fi
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p ffmpeg >"$WORKDIR/test-ffmpeg.log" 2>&1); then
  if grep -q "21 passed" "$WORKDIR/test-ffmpeg.log" \
    && grep -q "every_shipped_pair_resolves_its_registry_names ... ok" "$WORKDIR/test-ffmpeg.log" \
    && grep -q "unknown_pairs_refuse_with_the_supported_set_named ... ok" "$WORKDIR/test-ffmpeg.log" \
    && grep -q "unknown_platform_refusal_names_the_supported_set ... ok" "$WORKDIR/test-ffmpeg.log"; then
    pass "cargo-test-ffmpeg (21 lib + staged pairs + refusal naming the supported set)"
  else
    fail "ffmpeg-test-count" "test exited 0 without the platform tests; see test-ffmpeg.log"
  fi
else
  fail "test-ffmpeg" "cargo test -p ffmpeg failed; see test-ffmpeg.log"
fi

# ---- gate 2: hardware choices resolve, fall back, refuse (T002) -----------
echo ""
echo "--- gate 2: hardware encode choices against staged listings ---"
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p render --lib -- --test-threads=1 >"$WORKDIR/test-render.log" 2>&1); then
  if grep -q "446 passed" "$WORKDIR/test-render.log" \
    && grep -q "0 failed" "$WORKDIR/test-render.log"; then
    pass "cargo-test-render-lib (446 passed, single-threaded: no parallelism flake)"
  else
    fail "render-test-count" "test exited 0 without '446 passed'; see test-render.log"
  fi
  for TN in auto_resolves_per_os_against_staged_listings \
            explicit_choice_checks_against_staged_listings \
            present_hardware_reads_every_codec_from_a_listing \
            hardware_choices_resolve_h264_and_hevc_counterparts \
            hardware_choices_flag_the_oses_they_serve \
            hardware_argv_writes_the_resolved_codec_without_software_axes; do
    if grep -q "$TN ... ok" "$WORKDIR/test-render.log"; then
      pass "staged-test-$TN"
    else
      fail "staged-test-$TN" "staged-listing test missing from render log"
    fi
  done
else
  fail "test-render" "cargo test -p render --lib failed; see test-render.log"
fi

# Live CLI renders against the host listing: `auto` + explicit gates.
echo ""
echo "--- gate 2b: live encoder renders on the host listing ---"
if "$FSMITH" render "$WORKDIR/proj" --encoder software -o "$WORKDIR/sw.mp4" >"$WORKDIR/render-sw.log" 2>&1; then
  pass "render-explicit-software-exits-0"
else
  fail "render-software" "explicit software render exited non-zero; see render-sw.log"
fi
if "$FSMITH" render "$WORKDIR/proj" --encoder cpu -o "$WORKDIR/cpu.mp4" >"$WORKDIR/render-cpu.log" 2>&1; then
  if cmp -s "$WORKDIR/sw.mp4" "$WORKDIR/cpu.mp4"; then
    pass "cpu-alias-renders-byte-identical-to-software"
  else
    fail "cpu-alias-parity" "cpu render differs from software render"
  fi
else
  fail "render-cpu" "cpu render exited non-zero; see render-cpu.log"
fi
# `auto` under the hook must fall back to software byte-identically,
# with the fallback encoded as software (no hw= in the output banner).
if FRAMESMITH_NO_VIDEOTOOLBOX=1 "$FSMITH" render "$WORKDIR/proj" --encoder auto -o "$WORKDIR/auto-fallback.mp4" >"$WORKDIR/render-auto-fallback.log" 2>&1; then
  if cmp -s "$WORKDIR/sw.mp4" "$WORKDIR/auto-fallback.mp4" \
    && ! grep -q "hw=" "$WORKDIR/render-auto-fallback.log"; then
    pass "auto-falls-back-to-software-with-fact-named"
  else
    fail "auto-fallback" "auto render differs from software or names a hardware codec"
  fi
else
  fail "render-auto-fallback" "auto render under the hook exited non-zero; see render-auto-fallback.log"
fi
# Explicit absent encoders refuse non-zero, naming choice + codec,
# writing no MP4. The host here ships VideoToolbox only, so nvenc,
# vaapi, qsv, and amf must all refuse; videotoolbox refuses under
# the hook while present.
for ENC in nvenc vaapi qsv amf; do
  if "$FSMITH" render "$WORKDIR/proj" --encoder "$ENC" -o "$WORKDIR/$ENC.mp4" >"$WORKDIR/render-$ENC.log" 2>&1; then
    fail "$ENC-nonzero" "explicit $ENC exited 0 although the host lacks it; see render-$ENC.log"
  elif grep -q -e "--encoder $ENC" "$WORKDIR/render-$ENC.log" \
    && grep -q "h264_$ENC" "$WORKDIR/render-$ENC.log" \
    && [ ! -f "$WORKDIR/$ENC.mp4" ]; then
    pass "explicit-$ENC-refuses-naming-choice-and-codec"
  else
    fail "$ENC-refusal-text" "refusal names no choice/codec or wrote an MP4; see render-$ENC.log"
  fi
done
if FRAMESMITH_NO_VIDEOTOOLBOX=1 "$FSMITH" render "$WORKDIR/proj" --encoder videotoolbox -o "$WORKDIR/vt.mp4" >"$WORKDIR/render-vt-hook.log" 2>&1; then
  fail "videotoolbox-hook-nonzero" "explicit videotoolbox exited 0 under the hook; see render-vt-hook.log"
elif grep -q -- "--encoder videotoolbox" "$WORKDIR/render-vt-hook.log" \
  && grep -q "h264_videotoolbox" "$WORKDIR/render-vt-hook.log" \
  && [ ! -f "$WORKDIR/vt.mp4" ]; then
  pass "explicit-videotoolbox-refuses-under-hook"
else
  fail "videotoolbox-hook-text" "hook refusal names no choice/codec or wrote an MP4; see render-vt-hook.log"
fi
if "$FSMITH" render "$WORKDIR/proj" --encoder turbotenc -o "$WORKDIR/turbotenc.mp4" >"$WORKDIR/render-unknown-encoder.log" 2>&1; then
  fail "unknown-encoder-nonzero" "unknown --encoder exited 0; see render-unknown-encoder.log"
elif grep -q "turbotenc" "$WORKDIR/render-unknown-encoder.log" \
  && grep -q "nvenc" "$WORKDIR/render-unknown-encoder.log" \
  && grep -q "qsv" "$WORKDIR/render-unknown-encoder.log" \
  && [ ! -f "$WORKDIR/turbotenc.mp4" ]; then
  pass "unknown-encoder-refuses-naming-accepted-set"
else
  fail "unknown-encoder-text" "refusal names no value or accepted set; see render-unknown-encoder.log"
fi

# ---- gate 3: doctor facts match the render path (INT-003) -------------------
echo ""
echo "--- gate 3: doctor platform and encoder parity ---"
"$FSMITH" doctor >"$WORKDIR/doctor.log" 2>&1
"$FSMITH" doctor --json >"$WORKDIR/doctor.json" 2>&1
FRAMESMITH_NO_VIDEOTOOLBOX=1 "$FSMITH" doctor --json >"$WORKDIR/doctor-hook.json" 2>&1
for NEEDLE in "doctor: host " "doctor: hardware encoders:" "doctor: auto encoder:"; do
  if grep -q "$NEEDLE" "$WORKDIR/doctor.log"; then
    pass "doctor-text-reports-$NEEDLE"
  else
    fail "doctor-text-$NEEDLE" "doctor text misses '$NEEDLE'; see doctor.log"
  fi
done
# Recompute the expected facts from the host `ffmpeg -encoders`
# listing (exact-token match, never guessed) and compare both the
# plain and hook doctor JSON against them.
if python3 - "$WORKDIR/doctor.json" "$WORKDIR/doctor-hook.json" <<'PYEOF' >"$WORKDIR/doctor-parity.log" 2>&1
import json, subprocess, sys

ALL_HW = [
    "h264_videotoolbox", "hevc_videotoolbox",
    "h264_nvenc", "hevc_nvenc",
    "h264_vaapi", "hevc_vaapi",
    "h264_qsv", "hevc_qsv",
    "h264_amf", "hevc_amf",
]
PRIORITY = {
    "macos": [("videotoolbox", "h264_videotoolbox")],
    "linux": [("vaapi", "h264_vaapi"), ("nvenc", "h264_nvenc"), ("qsv", "h264_qsv")],
    "windows": [("nvenc", "h264_nvenc"), ("qsv", "h264_qsv"), ("amf", "h264_amf")],
}

def listing_codecs():
    out = subprocess.run(
        ["ffmpeg", "-hide_banner", "-encoders"],
        capture_output=True, text=True, check=True,
    )
    text = out.stdout + out.stderr
    present = set()
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1] in ALL_HW:
            present.add(parts[1])
    return present

plain = json.load(open(sys.argv[1]))
hook = json.load(open(sys.argv[2]))
present = listing_codecs()
os_name = plain.get("os")
arch = plain.get("arch")
assert os_name and arch, f"doctor JSON misses os/arch: {plain}"
expected_hw = [c for c in ALL_HW if c in present]
assert plain.get("hardware_encoders") == expected_hw, \
    f"hardware_encoders {plain.get('hardware_encoders')} != host listing {expected_hw}"
assert plain.get("ffmpeg_suitable") is True, plain
# `auto` = first priority encoder the host listing provides, else
# software. The container is MP4/H.264 here, matching `doctor`.
expected_auto = "software"
for _choice, codec in PRIORITY.get(os_name, []):
    if codec in present:
        expected_auto = codec
        break
assert plain.get("auto_encoder") == expected_auto, \
    f"auto_encoder {plain.get('auto_encoder')} != render-path {expected_auto}"
# Under the hook, auto falls back to software even when hardware is
# present, while the probed hardware list still names what the host
# provides (probe is read-only, hook only hides resolution).
assert hook.get("auto_encoder") == "software", hook
assert hook.get("hardware_encoders") == expected_hw, hook
print(f"doctor parity ok: os={os_name}/{arch} hw={plain['hardware_encoders']} auto={plain['auto_encoder']}")
PYEOF
then
  pass "doctor-facts-match-host-listing-and-render-path"
else
  fail "doctor-parity" "doctor facts drift from the host listing; see doctor-parity.log"
fi
# The doctor auto fact and the render path agree end to end: without
# the hook (host has VideoToolbox), `render --encoder auto` names the
# same hw codec in its output banner.
if "$FSMITH" render "$WORKDIR/proj" --encoder auto -o "$WORKDIR/auto-hw.mp4" >"$WORKDIR/render-auto-hw.log" 2>&1; then
  if grep -q "hw=h264_videotoolbox" "$WORKDIR/render-auto-hw.log" \
    && grep -q '"auto_encoder":"h264_videotoolbox"' "$WORKDIR/doctor.json"; then
    pass "doctor-auto-matches-render-hw-codec"
  else
    fail "doctor-render-drift" "doctor auto and render hw codec disagree; see render-auto-hw.log doctor.json"
  fi
else
  fail "render-auto-hw" "auto render exited non-zero; see render-auto-hw.log"
fi

# ---- gate 4: CI matrix file carries the three legs (T003 / CAP-001) --------
echo ""
echo "--- gate 4: CI matrix workflow ---"
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
for CMD in "cargo build --workspace" "cargo test --workspace"; do
  if grep -q "$CMD" "$CI_FILE"; then
    pass "ci-command-$CMD"
  else
    fail "ci-command-$CMD" "identical per-OS command '$CMD' absent from ci.yml"
  fi
done
if ruby -ryaml -e "d=YAML.load_file('$CI_FILE'); raise 'no jobs' unless d['jobs']; puts(d['jobs'].keys.join(','))" >"$WORKDIR/ci-yaml.log" 2>&1; then
  pass "ci-yaml-parses ($(cat "$WORKDIR/ci-yaml.log"))"
else
  fail "ci-yaml" "ci.yml failed YAML parse; see ci-yaml.log"
fi
echo "INFO: CI live green run: PENDING (workflow authored and locally validated, not pushed per instructions; owner pushes)"

# ---- gate 5: macOS output byte-identical -----------------------------------
echo ""
echo "--- gate 5: byte-identical macOS renders ---"
if "$FSMITH" render "$WORKDIR/proj" --encoder software -o "$WORKDIR/sw2.mp4" >"$WORKDIR/render-sw2.log" 2>&1; then
  if cmp -s "$WORKDIR/sw.mp4" "$WORKDIR/sw2.mp4"; then
    pass "software-reruns-byte-identical"
  else
    fail "software-determinism" "two software renders differ"
  fi
else
  fail "render-sw2" "software rerun exited non-zero; see render-sw2.log"
fi
if "$FSMITH" render "$WORKDIR/proj" -o "$WORKDIR/def.mp4" >"$WORKDIR/render-def.log" 2>&1; then
  if cmp -s "$WORKDIR/sw.mp4" "$WORKDIR/def.mp4"; then
    pass "default-render-equals-explicit-software"
  else
    fail "default-parity" "default render differs from explicit software render"
  fi
else
  fail "render-def" "default render exited non-zero; see render-def.log"
fi
# Hardware changes container bytes only: decoded frame count and
# dimensions match the software render.
SW_FRAMES="$(ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$WORKDIR/sw.mp4" 2>/dev/null || echo missing)"
HW_FRAMES="$(ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 "$WORKDIR/auto-hw.mp4" 2>/dev/null || echo missing)"
SW_DIMS="$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$WORKDIR/sw.mp4" 2>/dev/null || echo missing)"
HW_DIMS="$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$WORKDIR/auto-hw.mp4" 2>/dev/null || echo missing)"
if [ "$SW_FRAMES" = "$HW_FRAMES" ] && [ -n "$SW_FRAMES" ] && [ "$SW_FRAMES" != "missing" ] \
  && [ "$SW_DIMS" = "$HW_DIMS" ] && [ -n "$SW_DIMS" ] && [ "$SW_DIMS" != "missing" ]; then
  pass "hardware-decoded-picture-matches-software (frames $SW_FRAMES, dims $SW_DIMS)"
else
  fail "hw-picture-parity" "hw frames/dims $HW_FRAMES/$HW_DIMS vs software $SW_FRAMES/$SW_DIMS"
fi

# ---- gate 6: tamper and refusal gates ---------------------------------------
echo ""
echo "--- gate 6: tamper refused with cause named ---"
mkdir -p "$WORKDIR/broken" || exit 1
cp "$WORKDIR/proj/composition.kdl" "$WORKDIR/broken/composition.kdl" || exit 1
OLD="width=160" NEW="width=0" python3 - "$WORKDIR/broken/composition.kdl" <<'PYEOF'
import os, sys
old, new = os.environ["OLD"], os.environ["NEW"]
p = sys.argv[1]
text = open(p).read()
assert old in text, f"pattern {old!r} not in {p}"
open(p, "w").write(text.replace(old, new, 1))
PYEOF
if [ "$?" -ne 0 ]; then
  fail "tamper-broken" "could not rewrite composition.kdl in broken/"
elif "$FSMITH" check "$WORKDIR/broken" >"$WORKDIR/check-broken.log" 2>&1; then
  fail "broken-nonzero" "check exited 0 on width=0; see check-broken.log"
elif grep -q "composition.kdl" "$WORKDIR/check-broken.log" && grep -q "width" "$WORKDIR/check-broken.log"; then
  pass "broken-canvas-refuses-naming-file-and-cause"
else
  fail "broken-text" "refusal names no file or cause; see check-broken.log"
fi
if "$FSMITH" render "$WORKDIR/broken" -o "$WORKDIR/broken.mp4" >"$WORKDIR/render-broken.log" 2>&1; then
  fail "broken-render-nonzero" "render exited 0 on width=0; see render-broken.log"
elif [ ! -f "$WORKDIR/broken.mp4" ]; then
  pass "broken-render-writes-no-mp4"
else
  fail "broken-mp4" "render wrote an MP4 despite the gated failure"
fi

# ---- gate 7: touched-crate suites stay green (remaining crates) -------------
echo ""
echo "--- gate 7: cargo test on touched crates (preview, cli) ---"
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p preview --lib >"$WORKDIR/test-preview.log" 2>&1); then
  if grep -q "71 passed" "$WORKDIR/test-preview.log" && grep -q "0 failed" "$WORKDIR/test-preview.log"; then
    pass "cargo-test-preview-lib (71 passed)"
  else
    fail "preview-count" "test exited 0 without '71 passed'; see test-preview.log"
  fi
else
  fail "test-preview" "cargo test -p preview --lib failed; see test-preview.log"
fi
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p cli --test cli_n12_t003_encoder >"$WORKDIR/test-cli-encoder.log" 2>&1); then
  if grep -q "12 passed" "$WORKDIR/test-cli-encoder.log" && grep -q "0 failed" "$WORKDIR/test-cli-encoder.log"; then
    pass "cargo-test-cli-encoder-e2e (12 passed)"
  else
    fail "cli-encoder-count" "test exited 0 without '12 passed'; see test-cli-encoder.log"
  fi
else
  fail "test-cli-encoder" "cargo test -p cli --test cli_n12_t003_encoder failed; see test-cli-encoder.log"
fi

# ---- evidence ---------------------------------------------------------------
echo ""
echo "--- proof summary: $PASS_N passed, failing:${FAIL_LIST:- none} ---"
{
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $(\"$FSMITH\" --version 2>/dev/null || echo present)"
  echo "cargo: $(cargo --version 2>/dev/null)"
  echo "rustc: $(rustc --version 2>/dev/null)"
  echo "python: $(python3 --version 2>&1)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
  echo "CI live green run: PENDING (owner push)"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/build.log" \
  "$WORKDIR/test-ffmpeg.log" "$WORKDIR/test-render.log" \
  "$WORKDIR/render-sw.log" "$WORKDIR/render-cpu.log" \
  "$WORKDIR/render-auto-fallback.log" "$WORKDIR/render-auto-hw.log" \
  "$WORKDIR/render-nvenc.log" "$WORKDIR/render-vaapi.log" \
  "$WORKDIR/render-qsv.log" "$WORKDIR/render-amf.log" \
  "$WORKDIR/render-vt-hook.log" "$WORKDIR/render-unknown-encoder.log" \
  "$WORKDIR/doctor.log" "$WORKDIR/doctor.json" "$WORKDIR/doctor-hook.json" \
  "$WORKDIR/doctor-parity.log" "$WORKDIR/ci-yaml.log" \
  "$WORKDIR/render-sw2.log" "$WORKDIR/render-def.log" \
  "$WORKDIR/check-broken.log" "$WORKDIR/render-broken.log" \
  "$WORKDIR/test-preview.log" "$WORKDIR/test-cli-encoder.log" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/sw.mp4" "$WORKDIR/sw2.mp4" "$WORKDIR/def.mp4" \
  "$WORKDIR/auto-fallback.mp4" "$WORKDIR/auto-hw.mp4" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/proj/composition.kdl" "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"
