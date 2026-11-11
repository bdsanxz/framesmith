#!/bin/sh
# n23-runners-proof.sh -- Sprint 23 T005 one-command gate.
#
# Proves the ML runners reference end to end on a staged fixture copy
# (the checkout itself is never written):
#   1. `models list` shows every known model with size, engine and
#      install state; installs download nothing without consent,
#      verify SHA256 before caching, and re-installs skip with a note.
#   2. stub-driven `transcribe` feeds the captions track with word
#      times intact (transcript JSON regenerates byte-identical, word
#      windows parse exactly, cue frames paint, SRT interchange is
#      snapshot-identical at both seeks).
#   3. stub-driven `tts` renders deterministic voice audio (byte-
#      identical reruns, RIFF bytes) landing on an audio track whose
#      render carries an AAC stream and rerenders byte-identical.
#   4. stub-driven `remove-background` emits subject alpha plus the
#      inverse plate; behind-subject snapshots prove cover pixels
#      (opaque matte paints white, transparent shows the plain paint).
#   5. pure-Rust `beats` reports 120 BPM with snapped times and the
#      leading silence marked on the reference music.
#   6. gated refusals prove: missing model, offline install, low-
#      confidence audio, unknown voice and a missing engine each exit
#      non-zero naming the piece and the fix, with nothing staged.
#   7. `doctor` health matches runner resolution (models list states
#      equal doctor states, stub engine present).
#   8. zero network: every install resolves through the staged local
#      registry under a dead proxy (cache bytes equal registry bytes,
#      no download URL appears in any log).
#   9. `cargo test` for the touched crates (ml 21, cli n23 5) stays
#      green.
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n23-runners-proof.sh [--workdir DIR] [--bin PATH]
#   The reference is staged to DIR (default: fresh mktemp directory).
#   Evidence is copied to evidences/proof-n23-T005/ on every run.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,36p' "$0"; exit 0 ;;
    *) echo "n23-runners-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/runners-reference"
EVID_DIR="$WS_ROOT/evidences/proof-n23-T005"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n23-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

echo "================ n23 ml runners proof gate (T005) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "=================================================================="
echo ""

for NEED in cargo python3 ffmpeg ffprobe; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in composition.kdl README.md speech.wav shot.png click.wav speech.transcript.json speech.transcript.srt voice.wav bg/subject-alpha.png bg/background-plate.png click.beats.json; do
  if [ ! -f "$EXAMPLE_DIR/$F" ]; then
    fail "prereq-reference-$F" "missing $EXAMPLE_DIR/$F"
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
echo "cargo: $(cargo --version 2>/dev/null)"
echo "rustc: $(rustc --version 2>/dev/null)"
echo "python: $(python3 --version 2>&1)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo ""

# Isolated ML environment per call: staged cache, staged local registry,
# fixture checksums (computed in gate 1 before the first ML call), and a
# dead proxy so any network attempt would fail closed. Offline is pinned
# off and self-download pinned on so a caller's shell cannot change the
# gate; the offline refusal gate sets it explicitly instead.
ML_CACHE="$WORKDIR/cache"
ML_EMPTY="$WORKDIR/cache-empty"
ML_REG="$WORKDIR/reg"
PROXY="http://127.0.0.1:9"
SHA_WHISPER=""
SHA_TTS=""
SHA_REMBG=""

ML() {
  env PATH="${FSMITH_PATH:-$PATH}" \
    FRAMESMITH_ML_DIR="$ML_CACHE" \
    FRAMESMITH_ML_REGISTRY_DIR="$ML_REG" \
    FRAMESMITH_ML_SHA256_WHISPER_TINY_EN="$SHA_WHISPER" \
    FRAMESMITH_ML_SHA256_TTS_EN_ARIA="$SHA_TTS" \
    FRAMESMITH_ML_SHA256_REMBG_BASE="$SHA_REMBG" \
    FRAMESMITH_ML_AUTO_DOWNLOAD=1 FRAMESMITH_OFFLINE=0 \
    http_proxy="$PROXY" https_proxy="$PROXY" \
    HTTP_PROXY="$PROXY" HTTPS_PROXY="$PROXY" \
    "$FSMITH" "$@"
}

ML_EMPTY() {
  env PATH="${FSMITH_PATH:-$PATH}" \
    FRAMESMITH_ML_DIR="$ML_EMPTY" \
    FRAMESMITH_ML_REGISTRY_DIR="$ML_REG" \
    FRAMESMITH_ML_SHA256_WHISPER_TINY_EN="$SHA_WHISPER" \
    FRAMESMITH_ML_SHA256_TTS_EN_ARIA="$SHA_TTS" \
    FRAMESMITH_ML_SHA256_REMBG_BASE="$SHA_REMBG" \
    FRAMESMITH_ML_AUTO_DOWNLOAD=1 FRAMESMITH_OFFLINE=0 \
    http_proxy="$PROXY" https_proxy="$PROXY" \
    HTTP_PROXY="$PROXY" HTTPS_PROXY="$PROXY" \
    "$FSMITH" "$@"
}

ML_OFFLINE() {
  env FRAMESMITH_ML_DIR="$ML_EMPTY" \
    FRAMESMITH_ML_REGISTRY_DIR="$ML_REG" \
    FRAMESMITH_ML_SHA256_WHISPER_TINY_EN="$SHA_WHISPER" \
    FRAMESMITH_ML_SHA256_TTS_EN_ARIA="$SHA_TTS" \
    FRAMESMITH_ML_SHA256_REMBG_BASE="$SHA_REMBG" \
    FRAMESMITH_ML_AUTO_DOWNLOAD=1 FRAMESMITH_OFFLINE=1 \
    http_proxy="$PROXY" https_proxy="$PROXY" \
    HTTP_PROXY="$PROXY" HTTPS_PROXY="$PROXY" \
    "$FSMITH" "$@"
}

# ---- gate 1: stage the reference plus a local model registry ----
echo "--- gate 1: stage reference + local registry (zero network) ---"
REF="$WORKDIR/ref"
rm -rf "$REF"
cp -r "$EXAMPLE_DIR" "$REF" || exit 1
mkdir -p "$ML_REG" "$WORKDIR/gen" "$WORKDIR/gates" "$WORKDIR/emptybin" || exit 1
for N in whisper-tiny-en tts-en-aria rembg-base; do
  printf "framesmith-n23-fixture-model-%s" "$N" >"$ML_REG/$N" || exit 1
done
python3 - "$ML_REG" >"$WORKDIR/registry-sha.txt" <<'PYEOF'
import hashlib, sys
reg = sys.argv[1]
for n in ["whisper-tiny-en", "tts-en-aria", "rembg-base"]:
    b = open(reg + "/" + n, "rb").read()
    print(hashlib.sha256(b).hexdigest())
PYEOF
SHA_WHISPER="$(sed -n '1p' "$WORKDIR/registry-sha.txt")"
SHA_TTS="$(sed -n '2p' "$WORKDIR/registry-sha.txt")"
SHA_REMBG="$(sed -n '3p' "$WORKDIR/registry-sha.txt")"
if [ "${#SHA_WHISPER}" -eq 64 ] && [ "${#SHA_TTS}" -eq 64 ] && [ "${#SHA_REMBG}" -eq 64 ]; then
  pass "staged-reference-plus-local-registry (fixture SHA256 computed)"
else
  fail "stage-registry" "fixture checksums did not compute; see registry-sha.txt"
fi
# Silence fixture for the low-confidence beats refusal (stdlib wave).
if python3 - "$WORKDIR/gates/silence.wav" <<'PYEOF'
import struct, sys, wave
with wave.open(sys.argv[1], "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(48000)
    w.writeframes(b"\x00\x00" * (48000 * 2))
PYEOF
then
  pass "staged-silence-fixture (low-confidence beats input)"
else
  fail "stage-silence" "silence.wav did not generate"
fi
for F in composition.kdl speech.wav shot.png click.wav speech.transcript.json speech.transcript.srt voice.wav bg/subject-alpha.png bg/background-plate.png click.beats.json; do
  if [ -f "$REF/$F" ]; then :; else fail "stage-file-$F" "staged copy misses $F"; fi
done

# Minimal PNG decoder (stdlib only), shared by the matte gates.
cat >"$WORKDIR/pnglib.py" <<'PYEOF'
import struct, zlib
def decode(path):
    d = open(path, "rb").read()
    assert d[:8] == b"\x89PNG\r\n\x1a\n", "not a png"
    pos, idat = 8, b""
    while pos < len(d):
        n = struct.unpack(">I", d[pos:pos + 4])[0]
        kind, data = d[pos + 4:pos + 8], d[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, bd, ct, _, _, _ = struct.unpack(">IIBBBBB", data)
        elif kind == b"IDAT":
            idat += data
        pos += 12 + n
    raw = zlib.decompress(idat)
    ch = {0: 1, 2: 3, 6: 4}[ct]
    stride = w * ch
    out = bytearray(w * h * ch)
    prev, p = bytearray(stride), 0
    for y in range(h):
        f = raw[p]; p += 1
        line = bytearray(raw[p:p + stride]); p += stride
        if f == 1:
            for i in range(ch, stride):
                line[i] = (line[i] + line[i - ch]) & 255
        elif f == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 255
        elif f == 3:
            for i in range(stride):
                a = line[i - ch] if i >= ch else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 255
        elif f == 4:
            for i in range(stride):
                a = line[i - ch] if i >= ch else 0
                b, c = prev[i], prev[i - ch] if i >= ch else 0
                q = a + b - c
                pa, pb, pc = abs(q - a), abs(q - b), abs(q - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, ch, bytes(out)
PYEOF

# ---- gate 2: model manager with consent (T001) ----
echo ""
echo "--- gate 2: models list, consent, verified install ---"
if ML models list >"$WORKDIR/models-list.log" 2>&1; then
  pass "models-list-exits-0"
else
  fail "models-list-exit" "models list exited non-zero; see models-list.log"
fi
for N in whisper-tiny-en whisper-base-en parakeet-tdt-0720 tts-en-aria rembg-fast rembg-base rembg-precise; do
  if grep "model $N" "$WORKDIR/models-list.log" 2>/dev/null | grep -q "missing"; then
    pass "models-list-missing-$N"
  else
    fail "models-list-$N" "clean cache must mark $N missing; see models-list.log"
  fi
done
if ML models install whisper-tiny-en < /dev/null >"$WORKDIR/install-noconsent.log" 2>&1; then
  fail "no-consent-nonzero" "install exited 0 without --yes in a non-interactive shell"
else
  if grep -q "E_NO_CONSENT" "$WORKDIR/install-noconsent.log" && grep -q -e "--yes" "$WORKDIR/install-noconsent.log"; then
    pass "no-consent-refusal-names-yes-fix"
  else
    fail "no-consent-text" "refusal names no E_NO_CONSENT/--yes fix; see install-noconsent.log"
  fi
fi
if [ -e "$ML_CACHE/whisper-tiny-en/model.bin" ]; then
  fail "no-consent-no-staging" "a cached model file appeared despite the refusal"
else
  pass "no-consent-stages-nothing"
fi
for N in whisper-tiny-en tts-en-aria rembg-base; do
  if ML models install "$N" --yes >"$WORKDIR/install-$N.log" 2>&1 && grep -q "SHA256 verified" "$WORKDIR/install-$N.log"; then
    pass "install-verified-$N"
  else
    fail "install-$N" "consented install failed or did not verify; see install-$N.log"
  fi
  if cmp -s "$ML_CACHE/$N/model.bin" "$ML_REG/$N"; then
    pass "cache-equals-registry-$N"
  else
    fail "cache-bytes-$N" "cached bytes differ from the registry file (not a local copy)"
  fi
done
if ML models install whisper-tiny-en --yes >"$WORKDIR/install-reinstall.log" 2>&1 && grep -q "skipping" "$WORKDIR/install-reinstall.log"; then
  pass "reinstall-skips-verified-cache"
else
  fail "reinstall-skip" "re-install did not skip with a note; see install-reinstall.log"
fi
if ML models list >"$WORKDIR/models-list-ready.log" 2>&1; then
  READY_OK=1
  for N in whisper-tiny-en tts-en-aria rembg-base; do
    if grep "model $N" "$WORKDIR/models-list-ready.log" 2>/dev/null | grep -q "ready"; then :; else READY_OK=0; fi
  done
  if grep "model parakeet-tdt-0720" "$WORKDIR/models-list-ready.log" 2>/dev/null | grep -q "missing"; then :; else READY_OK=0; fi
  if [ "$READY_OK" -eq 1 ]; then
    pass "models-list-ready-states"
  else
    fail "models-list-states" "installed models must read ready, parakeet missing; see models-list-ready.log"
  fi
else
  fail "models-list-ready-exit" "models list failed after installs"
fi

# ---- gate 3: transcription into captions with word times (T002) ----
echo ""
echo "--- gate 3: stub transcription into captions ---"
if (cd "$REF" && ML transcribe speech.wav --engine stub --to srt -o "$WORKDIR/gen/speech.transcript.json") >"$WORKDIR/transcribe.log" 2>&1; then
  pass "transcribe-exits-0"
else
  fail "transcribe-exit" "transcribe exited non-zero; see transcribe.log"
fi
cp "$WORKDIR/gen/speech.transcript.json" "$WORKDIR/gen/speech.transcript.json.first" 2>/dev/null || :
if cmp -s "$WORKDIR/gen/speech.transcript.json" "$EXAMPLE_DIR/speech.transcript.json" && cmp -s "$WORKDIR/gen/speech.transcript.srt" "$EXAMPLE_DIR/speech.transcript.srt"; then
  pass "transcript-matches-checked-in-fixture (json+srt)"
else
  fail "transcript-fixture" "regenerated transcript differs from the checked-in fixture"
fi
if (cd "$REF" && ML transcribe speech.wav --engine stub --to srt -o "$WORKDIR/gen/speech.transcript.json") >"$WORKDIR/transcribe2.log" 2>&1 \
  && cmp -s "$WORKDIR/gen/speech.transcript.json.first" "$WORKDIR/gen/speech.transcript.json"; then
  pass "transcribe-rerun-byte-identical"
else
  fail "transcribe-determinism" "rerun bytes differ; see transcribe2.log"
fi
if python3 - "$WORKDIR/gen/speech.transcript.json" "$WORKDIR/gen/speech.transcript.srt" >"$WORKDIR/transcriptcheck.log" 2>&1 <<'PYEOF'
import json, sys
j = json.load(open(sys.argv[1]))
srt = open(sys.argv[2]).read()
cues = j["cues"]
assert len(cues) == 2, "want 2 cues, got %d" % len(cues)
assert (cues[0]["start"], cues[0]["end"]) == (0.0, 2.0), cues[0]
assert (cues[1]["start"], cues[1]["end"]) == (2.0, 4.0), cues[1]
words = [w for c in cues for w in c["words"]]
assert len(words) == 8, "want 8 words, got %d" % len(words)
assert words[0] == {"text": "lights", "start": 0.0, "end": 0.5}, words[0]
assert words[-1] == {"text": "take", "start": 3.5, "end": 4.0}, words[-1]
for c in cues:
    for w in c["words"]:
        assert c["start"] <= w["start"] < w["end"] <= c["end"], w
times = [(w["start"], w["end"]) for w in words]
assert all(a < b for a, b in zip(times, times[1:])), "word times must increase"
assert "lights camera framesmith action" in srt, srt
assert "00:00:00,000 --> 00:00:02,000" in srt, srt
assert "rolling sound marker take" in srt, srt
print("cues=2 words=8 cue-windows=[0,2)[2,4) first=lights[0,0.5] last=take[3.5,4]")
PYEOF
then
  pass "word-times-intact ($(cat "$WORKDIR/transcriptcheck.log"))"
else
  fail "word-times" "transcript word times wrong; see transcriptcheck.log"
fi
if "$FSMITH" check "$REF" >"$WORKDIR/check.log" 2>&1; then
  pass "check-exits-0-on-reference"
else
  fail "check-exit" "check exited non-zero; see check.log"
fi
if grep -q 'readiness\[captions\].*: ready (speech.transcript.json)' "$WORKDIR/check.log" 2>/dev/null; then
  pass "check-captions-ready (speech.transcript.json)"
else
  fail "check-readiness" "no captions-ready line naming speech.transcript.json; see check.log"
fi
if grep -q "readiness: 1 source(s): 1 ready, 0 loading, 0 failed" "$WORKDIR/check.log" 2>/dev/null; then
  pass "check-tally-1-ready"
else
  fail "check-tally" "tally is not '1 ready, 0 loading, 0 failed'; see check.log"
fi
if "$FSMITH" lint "$REF" >"$WORKDIR/lint.log" 2>&1; then
  pass "lint-exits-0-on-reference"
else
  fail "lint-exit" "lint exited non-zero; see lint.log"
fi
if grep -q "readiness: 1 source(s): 1 ready, 0 loading, 0 failed" "$WORKDIR/lint.log" 2>/dev/null && grep -q "lint: 0 findings" "$WORKDIR/lint.log" 2>/dev/null; then
  pass "lint-clean-carries-same-tally"
else
  fail "lint-clean" "lint lacks the 0-findings/1-ready tally; see lint.log"
fi
if "$FSMITH" snapshot --at 1.0,3.0 --out "$WORKDIR/snaps" "$REF" >"$WORKDIR/snapshot.log" 2>&1; then
  pass "snapshot-renders-two-cue-seeks"
else
  fail "snapshot-exit" "snapshot exited non-zero; see snapshot.log"
fi
mkdir -p "$WORKDIR/bare"
printf 'composition name="main" width=64 height=48 fps=10 duration=4 background="#101010" version=1 {\n}\n' >"$WORKDIR/bare/composition.kdl"
if "$FSMITH" snapshot --at 1.0 --out "$WORKDIR/snaps-bare" "$WORKDIR/bare" >"$WORKDIR/snapshot-bare.log" 2>&1 && [ -f "$WORKDIR/snaps-bare/snap-1s.png" ]; then
  if cmp -s "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps-bare/snap-1s.png"; then
    fail "words-paint" "t=1.0s equals the bare plate; captions paint no words"
  else
    pass "words-paint (cue frame differs from the bare plate)"
  fi
else
  fail "bare-snapshot" "bare-plate snapshot failed; see snapshot-bare.log"
fi
if cmp -s "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps/snap-3s.png" 2>/dev/null; then
  fail "cue-boundary" "t=1.0s equals t=3.0s; the transcript cues paint the same words"
else
  pass "cue-frames-differ (transcript words change at t=2.0s)"
fi
if "$FSMITH" snapshot --at 1.0,3.0 --out "$WORKDIR/snaps2" "$REF" >"$WORKDIR/snapshot2.log" 2>&1; then
  if cmp -s "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps2/snap-1s.png" && cmp -s "$WORKDIR/snaps/snap-3s.png" "$WORKDIR/snaps2/snap-3s.png"; then
    pass "snapshot-rerun-byte-identical"
  else
    fail "snapshot-rerun" "snapshot bytes differ across runs"
  fi
else
  fail "snapshot-rerun-exit" "second snapshot run failed; see snapshot2.log"
fi
# SRT interchange: the same transcript as .srt must paint identically.
VDIR="$WORKDIR/via-srt"
rm -rf "$VDIR"
cp -r "$REF" "$VDIR"
if python3 - "$VDIR/composition.kdl" <<'PYEOF' >>"$WORKDIR/snapshot.log" 2>&1
import sys
p = sys.argv[1]
s = open(p).read()
old = 'src="speech.transcript.json"'
assert old in s, "expected the transcript src in composition.kdl"
open(p, "w").write(s.replace(old, 'src="speech.transcript.srt"'))
PYEOF
then
  if "$FSMITH" snapshot --at 1.0,3.0 --out "$WORKDIR/snaps-srt" "$VDIR" >"$WORKDIR/snapshot-srt.log" 2>&1; then
    if cmp -s "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps-srt/snap-1s.png" && cmp -s "$WORKDIR/snaps/snap-3s.png" "$WORKDIR/snaps-srt/snap-3s.png"; then
      pass "srt-interchange-identical (zero drift at t=1.0s, 3.0s)"
    else
      fail "srt-drift" "srt import paints different frames than the json import"
    fi
  else
    fail "srt-snapshot" "srt snapshot failed; see snapshot-srt.log"
  fi
else
  fail "srt-stage" "could not point the copy at speech.transcript.srt"
fi

# ---- gate 4: TTS audio rendering on an audio track (T003) ----
echo ""
echo "--- gate 4: stub TTS into an audio track ---"
if ML tts hello framesmith --engine stub -o "$WORKDIR/gen/voice.wav" >"$WORKDIR/tts.log" 2>&1; then
  pass "tts-exits-0"
else
  fail "tts-exit" "tts exited non-zero; see tts.log"
fi
cp "$WORKDIR/gen/voice.wav" "$WORKDIR/gen/voice.wav.first" 2>/dev/null || :
if python3 - "$WORKDIR/gen/voice.wav" >"$WORKDIR/ttscheck.log" 2>&1 <<'PYEOF'
import sys
d = open(sys.argv[1], "rb").read()
assert d[:4] == b"RIFF" and d[8:12] == b"WAVE", "not a RIFF/WAVE file"
print("bytes=%d" % len(d))
PYEOF
then
  pass "tts-writes-wav ($(cat "$WORKDIR/ttscheck.log"))"
else
  fail "tts-wav" "tts output is not a WAV file"
fi
if cmp -s "$WORKDIR/gen/voice.wav" "$EXAMPLE_DIR/voice.wav"; then
  pass "voice-matches-checked-in-fixture"
else
  fail "voice-fixture" "regenerated voice differs from the checked-in fixture"
fi
if ML tts hello framesmith --engine stub -o "$WORKDIR/gen/voice.wav" >"$WORKDIR/tts2.log" 2>&1 \
  && cmp -s "$WORKDIR/gen/voice.wav.first" "$WORKDIR/gen/voice.wav"; then
  pass "tts-rerun-byte-identical"
else
  fail "tts-determinism" "rerun bytes differ; see tts2.log"
fi
if grep -q "master peak:" "$WORKDIR/check.log" 2>/dev/null; then
  pass "audio-track-contributes-to-mix ($(grep 'master peak' "$WORKDIR/check.log" | head -n 1))"
else
  fail "audio-mix" "check reports no master peak: the voice track is silent; see check.log"
fi
if "$FSMITH" render "$REF" -o "$WORKDIR/run1.mp4" >"$WORKDIR/render1.log" 2>&1 \
  && "$FSMITH" render "$REF" -o "$WORKDIR/run2.mp4" >"$WORKDIR/render2.log" 2>&1; then
  if cmp -s "$WORKDIR/run1.mp4" "$WORKDIR/run2.mp4"; then
    pass "two-renders-byte-identical ($(wc -c <"$WORKDIR/run1.mp4") bytes)"
  else
    fail "render-determinism" "two renders of the reference differ"
  fi
else
  fail "render-exit" "render exited non-zero; see render1.log render2.log"
fi
if ffprobe -v error -show_entries stream=codec_type,codec_name -of csv "$WORKDIR/run1.mp4" >"$WORKDIR/streams.log" 2>&1 && grep -q "aac,audio" "$WORKDIR/streams.log" && grep -q "h264,video" "$WORKDIR/streams.log"; then
  pass "render-carries-video-and-aac (voice audio on the track)"
else
  fail "render-streams" "render lacks h264 video or aac audio; see streams.log"
fi

# ---- gate 5: matte plus plate with cover pixels (T004) ----
echo ""
echo "--- gate 5: subject matte + inverse plate ---"
if (cd "$REF" && ML remove-background shot.png --engine stub --quality balanced -o "$WORKDIR/gen/bg") >"$WORKDIR/remove-background.log" 2>&1; then
  pass "remove-background-exits-0"
else
  fail "remove-background-exit" "remove-background exited non-zero; see remove-background.log"
fi
cp "$WORKDIR/gen/bg/subject-alpha.png" "$WORKDIR/gen/subject-alpha.png.first" 2>/dev/null || :
cp "$WORKDIR/gen/bg/background-plate.png" "$WORKDIR/gen/background-plate.png.first" 2>/dev/null || :
if cmp -s "$WORKDIR/gen/bg/subject-alpha.png" "$EXAMPLE_DIR/bg/subject-alpha.png" && cmp -s "$WORKDIR/gen/bg/background-plate.png" "$EXAMPLE_DIR/bg/background-plate.png"; then
  pass "matte-and-plate-match-checked-in-fixtures"
else
  fail "matte-fixture" "regenerated matte/plate differ from the checked-in fixtures"
fi
if (cd "$REF" && ML remove-background shot.png --engine stub --quality balanced -o "$WORKDIR/gen/bg") >"$WORKDIR/remove-background2.log" 2>&1 \
  && cmp -s "$WORKDIR/gen/subject-alpha.png.first" "$WORKDIR/gen/bg/subject-alpha.png" \
  && cmp -s "$WORKDIR/gen/background-plate.png.first" "$WORKDIR/gen/bg/background-plate.png"; then
  pass "remove-background-rerun-byte-identical"
else
  fail "matte-determinism" "rerun bytes differ; see remove-background2.log"
fi
if python3 - "$WORKDIR/pnglib.py" "$WORKDIR/gen/bg/subject-alpha.png" "$WORKDIR/gen/bg/background-plate.png" "$REF/shot.png" >"$WORKDIR/mattecheck.log" 2>&1 <<'PYEOF'
import sys
lib = open(sys.argv[1]).read()
ns = {}
exec(lib, ns)
decode = ns["decode"]
aw, ah, ach, alpha = decode(sys.argv[2])
pw, ph, pch, plate = decode(sys.argv[3])
sw, sh, sch, shot = decode(sys.argv[4])
assert (aw, ah, pw, ph, sw, sh) == (64, 48, 64, 48, 64, 48), "canvas is 64x48"
assert ach == 4 and pch == 3 and sch == 4
def a_at(x, y):
    i = (y * aw + x) * 4
    return alpha[i + 3]
assert a_at(32, 24) == 255, "subject center must be opaque"
assert a_at(0, 0) == 0 and a_at(63, 0) == 0 and a_at(0, 47) == 0 and a_at(63, 47) == 0, "corners must be transparent"
def rgb(buf, ch, x, y):
    i = (y * aw + x) * ch
    return tuple(buf[i:i + 3])
assert rgb(plate, 3, 0, 0) == rgb(shot, 4, 0, 0) == (255, 0, 0), "plate corner must equal the shot background"
assert rgb(plate, 3, 63, 47) == (255, 0, 0), "plate far corner must equal the shot background"
center = rgb(plate, 3, 32, 24)
assert center != (0, 0, 255), "plate center must carry no subject blue, got %r" % (center,)
assert rgb(alpha, 4, 32, 24) == (255, 255, 255), "opaque matte paint must be white"
print("alpha-center=255 corners=0 plate-corners=red plate-center=%r no-subject-blue" % (center,))
PYEOF
then
  pass "matte-and-plate-pixels ($(cat "$WORKDIR/mattecheck.log"))"
else
  fail "matte-pixels" "alpha/plate pixel contract broken; see mattecheck.log"
fi
# Cover pixels: the reference orders captions behind the subject matte.
PDIR="$WORKDIR/plain"
rm -rf "$PDIR"
cp -r "$REF" "$PDIR"
if python3 - "$PDIR/composition.kdl" <<'PYEOF' >>"$WORKDIR/snapshot.log" 2>&1
import sys
p = sys.argv[1]
s = open(p).read()
old = ' behind-subject=true matte="bg/subject-alpha.png"'
assert old in s, "expected the behind-subject matte on the captions clip"
open(p, "w").write(s.replace(old, ""))
PYEOF
then
  if "$FSMITH" snapshot --at 1.0 --out "$WORKDIR/snaps-plain" "$PDIR" >"$WORKDIR/snapshot-plain.log" 2>&1; then
    if python3 - "$WORKDIR/pnglib.py" "$PDIR/bg/subject-alpha.png" "$WORKDIR/snaps/snap-1s.png" "$WORKDIR/snaps-plain/snap-1s.png" >"$WORKDIR/covercheck.log" 2>&1 <<'PYEOF'
import sys
exec(open(sys.argv[1]).read())
mw, mh, mch, mp = decode(sys.argv[2])
cw, chh, cch, cp = decode(sys.argv[3])
pw, ph, pch, pp = decode(sys.argv[4])
assert (mw, mh) == (cw, chh) == (pw, ph) == (64, 48), "canvas/matte mismatch"
assert mch == 4 and cch == pch == 3
opaque_not_white = transparent_differs = semi = opaque_n = 0
for y in range(mh):
    for x in range(mw):
        mi = (y * mw + x) * 4
        ci = (y * cw + x) * 3
        a = mp[mi + 3]
        if a == 255:
            opaque_n += 1
            if cp[ci:ci + 3] != b"\xff\xff\xff":
                opaque_not_white += 1
        elif a == 0:
            if cp[ci:ci + 3] != pp[ci:ci + 3]:
                transparent_differs += 1
        else:
            semi += 1
print("opaque=%d opaque-not-white=%d transparent-differs=%d feathered=%d" % (
    opaque_n, opaque_not_white, transparent_differs, semi))
assert opaque_not_white == 0 and transparent_differs == 0 and opaque_n > 0
PYEOF
    then
      pass "matte-cover-pixels ($(cat "$WORKDIR/covercheck.log"))"
    else
      fail "cover-pixels" "matte cover/show-through mismatch; see covercheck.log"
    fi
  else
    fail "plain-snapshot" "matte-free snapshot failed; see snapshot-plain.log"
  fi
else
  fail "plain-stage" "could not strip the matte from the copy"
fi

# ---- gate 6: beat grid with BPM on the reference music (T004) ----
echo ""
echo "--- gate 6: pure-Rust beats on reference music ---"
if "$FSMITH" beats "$REF/click.wav" -o "$WORKDIR/gen/click.beats.json" >"$WORKDIR/beats.log" 2>&1; then
  pass "beats-exits-0 ($(cat "$WORKDIR/beats.log"))"
else
  fail "beats-exit" "beats exited non-zero; see beats.log"
fi
cp "$WORKDIR/gen/click.beats.json" "$WORKDIR/gen/click.beats.json.first" 2>/dev/null || :
if python3 - "$WORKDIR/gen/click.beats.json" >"$WORKDIR/beatcheck.log" 2>&1 <<'PYEOF'
import json, sys
j = json.load(open(sys.argv[1]))
assert j["kind"] == "beats", j
assert j["model"] == "none" and j["engine"] == "signal", j
assert j["source"] == "click.wav", j
assert abs(j["bpm"] - 120.0) <= 0.5, j["bpm"]
beats = j["beats"]
assert 6 <= len(beats) <= 10, beats
assert abs(beats[0] - 0.5) <= 0.05, beats
assert all(b < a for a, b in zip(beats[1:], beats)), "beats must increase"
for a, b in zip(beats, beats[1:]):
    assert abs((b - a) - 0.5) <= 0.06, (a, b)
silent = j["silent"]
assert len(silent) >= 1, j
assert abs(silent[0][0] - 0.0) <= 0.01 and abs(silent[0][1] - 0.5) <= 0.05, silent
assert j["confidence"] >= 0.35, j
print("bpm=%.1f beats=%d first=%.3f silent=%r confidence=%.2f" % (
    j["bpm"], len(beats), beats[0], silent, j["confidence"]))
PYEOF
then
  pass "beat-grid-with-bpm ($(cat "$WORKDIR/beatcheck.log"))"
else
  fail "beat-grid" "beat file contract broken; see beatcheck.log"
fi
if cmp -s "$WORKDIR/gen/click.beats.json" "$EXAMPLE_DIR/click.beats.json"; then
  pass "beat-file-matches-checked-in-fixture"
else
  fail "beats-fixture" "regenerated beat file differs from the checked-in fixture"
fi
if "$FSMITH" beats "$REF/click.wav" -o "$WORKDIR/gen/click.beats.json" >"$WORKDIR/beats2.log" 2>&1 \
  && cmp -s "$WORKDIR/gen/click.beats.json.first" "$WORKDIR/gen/click.beats.json"; then
  pass "beats-rerun-byte-identical"
else
  fail "beats-determinism" "rerun bytes differ; see beats2.log"
fi

# ---- gate 7: gated refusals name their fix with nothing staged ----
echo ""
echo "--- gate 7: gated refusals ---"
if ML_EMPTY transcribe "$REF/speech.wav" --engine stub -o "$WORKDIR/gates/miss.json" >"$WORKDIR/refuse-missing-model.log" 2>&1; then
  fail "missing-model-nonzero" "transcribe exited 0 with no model installed"
else
  if grep -q "E_NO_MODEL" "$WORKDIR/refuse-missing-model.log" && grep -q "framesmith models install whisper-tiny-en --yes" "$WORKDIR/refuse-missing-model.log"; then
    pass "missing-model-refusal-names-install-fix"
  else
    fail "missing-model-text" "refusal names no install fix; see refuse-missing-model.log"
  fi
fi
if [ -e "$WORKDIR/gates/miss.json" ]; then
  fail "missing-model-no-staging" "a transcript file appeared despite the refusal"
else
  pass "missing-model-stages-nothing"
fi
if ML_OFFLINE models install tts-en-aria --yes >"$WORKDIR/refuse-offline.log" 2>&1; then
  fail "offline-nonzero" "install exited 0 on an offline machine with an empty cache"
else
  if grep -q "offline" "$WORKDIR/refuse-offline.log" && grep -q "provision it manually" "$WORKDIR/refuse-offline.log"; then
    pass "offline-refusal-names-manual-provision-fix"
  else
    fail "offline-text" "refusal names no offline/manual-provision fix; see refuse-offline.log"
  fi
fi
if [ -e "$ML_EMPTY/tts-en-aria/model.bin" ]; then
  fail "offline-no-staging" "a cached model file appeared despite the offline refusal"
else
  pass "offline-stages-nothing"
fi
if "$FSMITH" beats "$WORKDIR/gates/silence.wav" -o "$WORKDIR/gates/sil.beats.json" >"$WORKDIR/refuse-silence.log" 2>&1; then
  fail "silence-nonzero" "beats exited 0 on silence (invented a grid)"
else
  if grep -q "E_NO_GRID" "$WORKDIR/refuse-silence.log" && grep -q "low confidence" "$WORKDIR/refuse-silence.log"; then
    pass "low-confidence-refusal-names-reason"
  else
    fail "low-confidence-text" "refusal names no reason; see refuse-silence.log"
  fi
fi
if [ -e "$WORKDIR/gates/sil.beats.json" ]; then
  fail "silence-no-staging" "a beat file appeared despite the refusal"
else
  pass "low-confidence-stages-nothing"
fi
if ML tts hello --engine stub --voice nope -o "$WORKDIR/gates/v.wav" >"$WORKDIR/refuse-voice.log" 2>&1; then
  fail "unknown-voice-nonzero" "tts exited 0 with an unknown voice"
else
  if grep -q "unknown voice" "$WORKDIR/refuse-voice.log" && grep -q "aria" "$WORKDIR/refuse-voice.log"; then
    pass "unknown-voice-refusal-names-voice"
  else
    fail "unknown-voice-text" "refusal names no voice; see refuse-voice.log"
  fi
fi
if [ -e "$WORKDIR/gates/v.wav" ]; then
  fail "unknown-voice-no-staging" "a voice file appeared despite the refusal"
else
  pass "unknown-voice-stages-nothing"
fi
if FSMITH_PATH="$WORKDIR/emptybin" ML transcribe "$REF/speech.wav" --engine auto -o "$WORKDIR/gates/none.json" >"$WORKDIR/refuse-engine.log" 2>&1; then
  fail "missing-engine-nonzero" "transcribe exited 0 with no engine on PATH"
else
  if grep -q "E_NO_ENGINE" "$WORKDIR/refuse-engine.log" && grep -q -- "--engine stub" "$WORKDIR/refuse-engine.log"; then
    pass "missing-engine-refusal-names-fix"
  else
    fail "missing-engine-text" "refusal names no engine fix; see refuse-engine.log"
  fi
fi
if [ -e "$WORKDIR/gates/none.json" ]; then
  fail "missing-engine-no-staging" "a transcript file appeared despite the refusal"
else
  pass "missing-engine-stages-nothing"
fi

# ---- gate 8: doctor health matches runner resolution (INT-003) ----
echo ""
echo "--- gate 8: doctor health ---"
if ML doctor >"$WORKDIR/doctor.log" 2>&1; then
  pass "doctor-exits-0"
else
  fail "doctor-exit" "doctor exited non-zero; see doctor.log"
fi
if grep -q "doctor: ml engine stub" "$WORKDIR/doctor.log" 2>/dev/null; then
  pass "doctor-stub-engine-present"
else
  fail "doctor-stub" "no stub engine line in doctor output; see doctor.log"
fi
if python3 - "$WORKDIR/models-list-ready.log" "$WORKDIR/doctor.log" >"$WORKDIR/doctorcheck.log" 2>&1 <<'PYEOF'
import sys
def states(lines, prefix, key):
    out = {}
    for line in lines:
        line = line.strip()
        if line.startswith(prefix):
            for tok in ("ready", "missing"):
                if " " + tok in line:
                    name = line.split()[key]
                    out[name] = tok
    return out
list_states = states(open(sys.argv[1]).readlines(), "model ", 1)
doc_states = states(open(sys.argv[2]).readlines(), "doctor: ml model ", 3)
assert list_states == doc_states, "list %r vs doctor %r" % (list_states, doc_states)
assert len(list_states) == 7, list_states
print(sorted("%s=%s" % kv for kv in list_states.items()))
PYEOF
then
  pass "doctor-states-match-models-list ($(cat "$WORKDIR/doctorcheck.log"))"
else
  fail "doctor-parity" "doctor states disagree with models list; see doctorcheck.log"
fi
if ML doctor --json >"$WORKDIR/doctor-json.log" 2>&1 && grep -q '"ml_models"' "$WORKDIR/doctor-json.log" && grep -q '"ml_engines"' "$WORKDIR/doctor-json.log"; then
  pass "doctor-json-carries-ml-health"
else
  fail "doctor-json" "doctor --json lacks ml_models/ml_engines; see doctor-json.log"
fi

# ---- gate 9: zero network for the whole gate ----
echo ""
echo "--- gate 9: zero network ---"
echo "dead proxy: $PROXY; local registry: $ML_REG" >"$WORKDIR/network.log"
echo "cache-bytes-equal-registry: gate 2 cmps" >>"$WORKDIR/network.log"
if grep -q "framesmith-models" "$WORKDIR"/*.log 2>/dev/null; then
  fail "zero-network" "a download URL appears in the gate logs (a network fetch ran)"
else
  pass "no-download-url-in-any-log"
fi
if grep -q "SHA256 verified" "$WORKDIR/install-whisper-tiny-en.log" 2>/dev/null \
  && grep -q "SHA256 verified" "$WORKDIR/install-tts-en-aria.log" 2>/dev/null \
  && grep -q "SHA256 verified" "$WORKDIR/install-rembg-base.log" 2>/dev/null; then
  pass "all-installs-verified-from-local-registry-under-dead-proxy"
else
  fail "zero-network-installs" "an install did not verify from the local registry; see install logs"
fi
cat "$WORKDIR/network.log"

# ---- gate 10: cargo tests for the touched crates ----
echo ""
echo "--- gate 10: cargo tests (ml, cli n23) ---"
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p ml) >"$WORKDIR/mltest.log" 2>&1; then
  if grep -q "test result: ok" "$WORKDIR/mltest.log" && grep -q "failed" "$WORKDIR/mltest.log"; then
    pass "cargo-test-ml-green ($(grep 'test result' "$WORKDIR/mltest.log" | tr '\n' ' '))"
  else
    fail "mltest-shape" "ml test log has no ok result; see mltest.log"
  fi
else
  fail "mltest" "cargo test -p ml failed; see mltest.log"
fi
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p cli --test cli_n23_ml_runners) >"$WORKDIR/clitest.log" 2>&1; then
  if grep -q "test result: ok" "$WORKDIR/clitest.log"; then
    pass "cargo-test-cli-n23-green ($(grep 'test result' "$WORKDIR/clitest.log" | tr '\n' ' '))"
  else
    fail "clitest-shape" "cli test log has no ok result; see clitest.log"
  fi
else
  fail "clitest" "cargo test -p cli --test cli_n23_ml_runners failed; see clitest.log"
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
  echo "dead proxy: $PROXY"
  echo "local registry: $ML_REG"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/registry-sha.txt" "$WORKDIR/network.log" \
  "$WORKDIR/check.log" "$WORKDIR/lint.log" \
  "$WORKDIR/models-list.log" "$WORKDIR/models-list-ready.log" \
  "$WORKDIR/install-noconsent.log" "$WORKDIR/install-whisper-tiny-en.log" \
  "$WORKDIR/install-tts-en-aria.log" "$WORKDIR/install-rembg-base.log" \
  "$WORKDIR/install-reinstall.log" \
  "$WORKDIR/transcribe.log" "$WORKDIR/transcribe2.log" "$WORKDIR/transcriptcheck.log" \
  "$WORKDIR/snapshot.log" "$WORKDIR/snapshot2.log" "$WORKDIR/snapshot-bare.log" \
  "$WORKDIR/snapshot-srt.log" "$WORKDIR/snapshot-plain.log" \
  "$WORKDIR/tts.log" "$WORKDIR/tts2.log" "$WORKDIR/render1.log" "$WORKDIR/render2.log" \
  "$WORKDIR/streams.log" \
  "$WORKDIR/remove-background.log" "$WORKDIR/remove-background2.log" \
  "$WORKDIR/mattecheck.log" "$WORKDIR/covercheck.log" \
  "$WORKDIR/beats.log" "$WORKDIR/beats2.log" "$WORKDIR/beatcheck.log" \
  "$WORKDIR/refuse-missing-model.log" "$WORKDIR/refuse-offline.log" \
  "$WORKDIR/refuse-silence.log" "$WORKDIR/refuse-voice.log" \
  "$WORKDIR/refuse-engine.log" \
  "$WORKDIR/doctor.log" "$WORKDIR/doctor-json.log" "$WORKDIR/doctorcheck.log" \
  "$WORKDIR/mltest.log" "$WORKDIR/clitest.log" "$EVID_DIR/" 2>/dev/null
cp "$WORKDIR/gen/speech.transcript.json" "$WORKDIR/gen/speech.transcript.srt" \
  "$WORKDIR/gen/voice.wav" "$WORKDIR/gen/click.beats.json" "$EVID_DIR/" 2>/dev/null
cp "$WORKDIR/gen/bg/subject-alpha.png" "$WORKDIR/gen/bg/background-plate.png" "$EVID_DIR/" 2>/dev/null
echo "evidence: $EVID_DIR"
echo ""
if [ -n "$FAIL_LIST" ]; then
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo "PROOF: PASS -- $PASS_N checks green (stub-driven runners + every gate, zero network)"
exit 0
