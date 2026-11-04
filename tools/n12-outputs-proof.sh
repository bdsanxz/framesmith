#!/bin/sh
# n12-outputs-proof.sh — Sprint 12 T006 one-command gate.
#
# Proves the outputs reference project end to end: one composition
# (text over video with alpha content, plus an HDR variant) that
# validates and delivers HEVC, WebM, ProRes, GIF, PNG sequence, and
# HLS destinations at two presets, with keyframe and peak-bitrate
# control, an HDR Main10 variant plus tone-mapped SDR, and a
# provenance stamp on every output — re-rendering byte-identically
# on software paths, refusing every invalid destination before
# frame one, and stopping a mid-render failure with the cause named
# and nothing presented as complete. Stages the reference from
# framesmith/examples/outputs-reference, builds its media with
# pinned host ffmpeg commands, delivers all destinations twice,
# checks byte-identity and probes, and reports PROOF: PASS only
# when every check succeeds (exit 0). Any single failing check
# prints FAIL and the gate exits 1 (no false pass).
#
# Usage: n12-outputs-proof.sh [--workdir DIR] [--bin PATH]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; the reference is
#   staged to DIR first (media generated there). Evidence is copied
#   to evidences/proof-n12-T006/ on every run.
#
# Tamper coverage: a bad canvas edit must refuse with a non-zero
# exit and no MP4, and a vanished source must stop naming the path
# with no partial file, sequence dir, or playlist — so editing or
# breaking the reference cannot stay green.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "n12-outputs-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
EXAMPLE_DIR="$FRAMESMITH_DIR/examples/outputs-reference"
EVID_DIR="$FRAMESMITH_DIR/../evidences/proof-n12-T006"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n12-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

# sha256 of one file (portable across macOS/Linux coreutils).
sha() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

echo "================ n12 outputs proof gate (T006) ================"
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "==============================================================="
echo ""

for NEED in cargo ffmpeg ffprobe python3; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
if [ ! -f "$EXAMPLE_DIR/composition.kdl" ]; then
  fail "prereq-example" "missing $EXAMPLE_DIR/composition.kdl"
fi
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
VERSION="$("$FSMITH" --version 2>/dev/null | awk '{print $NF}')"
echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
echo "cargo: $(cargo --version 2>/dev/null || echo MISSING)"
echo "rustc: $(rustc --version 2>/dev/null || echo MISSING)"
echo ""

# --- stage the reference -------------------------------------------------------
echo "--- stage reference ---"
rm -rf "$WORKDIR/proj"
mkdir -p "$WORKDIR/proj"
cp "$EXAMPLE_DIR/composition.kdl" "$WORKDIR/proj/composition.kdl"
if ffmpeg -y -v error -f lavfi -i "nullsrc=size=160x120:rate=15" \
    -vf "format=rgba,geq=r=255:g=0:b=0:a=128" -t 2 -r 15 -c:v png \
    -pix_fmt rgba "$WORKDIR/proj/clip.mov" \
  && ffmpeg -y -v error -f lavfi -i "sine=frequency=440:duration=2" \
    -ar 48000 -ac 2 "$WORKDIR/proj/tone.wav" \
  && ffmpeg -y -v error -f lavfi -i "testsrc=s=160x120:r=15:d=2" \
    -c:v libx265 -pix_fmt yuv420p10le \
    -x265-params "colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc" \
    -t 2 "$WORKDIR/proj/hdr-pq.mp4"; then
  pass "stage-reference"
else
  fail "stage-reference" "host ffmpeg could not generate clip.mov/tone.wav/hdr-pq.mp4"
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
KDL_BEFORE="$(sha "$WORKDIR/proj/composition.kdl")"
echo ""

cd "$WORKDIR" || exit 1

# --- small probe helpers ---------------------------------------------------------
# Container comment stamp (empty when the muxer dropped it).
comment_of() {
  ffprobe -v error -show_entries format_tags=comment \
    -of default=noprint_wrappers=1:nokey=1 "$1" 2>/dev/null | tr -d '\r\n'
}
# Video-stream entries for one file.
streams_of() {
  ffprobe -v error -show_entries \
    "stream=codec_name,width,height,pix_fmt,sample_rate,channels" \
    -of default=noprint_wrappers=1 "$1" 2>/dev/null
}
# 16-hex digest after `inputs=` inside a stamp string.
digest_of() {
  printf '%s' "$1" | sed -n 's/.*inputs=\([0-9a-f]*\).*/\1/p'
}
# Decoded frame count of one video file.
nframes() {
  ffprobe -v error -count_frames -select_streams v:0 \
    -show_entries stream=nb_read_frames -of csv=p=0 "$1" 2>/dev/null | tr -d ' \r\n'
}

# --- the reference validates -------------------------------------------------------
"$FSMITH" check proj >check.log 2>&1
CODE=$?
if [ $CODE -eq 0 ] && grep -q "ok:" check.log; then
  pass "check-green"
else
  fail "check-green" "exit=$CODE (want 0 with ok:)"
fi
echo ""

# --- all six non-default destinations, each twice, byte-identical ------------------
echo "--- deliver every destination twice ---"
"$FSMITH" render proj --format hevc --encoder software -o hevc1.mp4 >r-hevc1.log 2>&1
C1=$?
"$FSMITH" render proj --format hevc --encoder software -o hevc2.mp4 >r-hevc2.log 2>&1
C2=$?
if [ $C1 -eq 0 ] && [ $C2 -eq 0 ] && [ -s hevc1.mp4 ] \
  && [ "$(sha hevc1.mp4)" = "$(sha hevc2.mp4)" ]; then
  pass "hevc-twice-byte-identical"
else
  fail "hevc-twice-byte-identical" "exits=$C1/$C2 (want 0/0 with equal hashes)"
fi
P="$(streams_of hevc1.mp4)"
if echo "$P" | grep -q "codec_name=hevc" && echo "$P" | grep -q "width=160" \
  && echo "$P" | grep -q "height=120" && echo "$P" | grep -q "codec_name=aac"; then
  pass "hevc-probe-codec-size-audio"
else
  fail "hevc-probe-codec-size-audio" "$(echo "$P" | tr '\n' ' ')"
fi

"$FSMITH" render proj --format webm --encoder software -o a1.webm >r-webm1.log 2>&1
C1=$?
"$FSMITH" render proj --format webm --encoder software -o a2.webm >r-webm2.log 2>&1
C2=$?
if [ $C1 -eq 0 ] && [ $C2 -eq 0 ] && [ -s a1.webm ] \
  && [ "$(sha a1.webm)" = "$(sha a2.webm)" ]; then
  pass "webm-twice-byte-identical"
else
  fail "webm-twice-byte-identical" "exits=$C1/$C2 (want 0/0 with equal hashes)"
fi
P="$(streams_of a1.webm)"
if echo "$P" | grep -q "codec_name=vp9" && echo "$P" | grep -q "width=160" \
  && echo "$P" | grep -q "height=120" && echo "$P" | grep -q "codec_name=opus"; then
  pass "webm-probe-codec-size-audio"
else
  fail "webm-probe-codec-size-audio" "$(echo "$P" | tr '\n' ' ')"
fi

"$FSMITH" render proj --format prores --prores-profile hq --encoder software -o p1.mov >r-prores1.log 2>&1
C1=$?
"$FSMITH" render proj --format prores --prores-profile hq --encoder software -o p2.mov >r-prores2.log 2>&1
C2=$?
if [ $C1 -eq 0 ] && [ $C2 -eq 0 ] && [ -s p1.mov ] \
  && [ "$(sha p1.mov)" = "$(sha p2.mov)" ] && grep -q "profile=hq" r-prores1.log; then
  pass "prores-hq-twice-byte-identical"
else
  fail "prores-hq-twice-byte-identical" "exits=$C1/$C2 (want 0/0, equal hashes, profile=hq)"
fi
P="$(streams_of p1.mov)"
if echo "$P" | grep -q "codec_name=prores" && echo "$P" | grep -q "width=160" \
  && echo "$P" | grep -q "height=120"; then
  pass "prores-probe-codec-size"
else
  fail "prores-probe-codec-size" "$(echo "$P" | tr '\n' ' ')"
fi

"$FSMITH" render proj --format gif --encoder software -o g1.gif >r-gif1.log 2>&1
C1=$?
"$FSMITH" render proj --format gif --encoder software -o g2.gif >r-gif2.log 2>&1
C2=$?
if [ $C1 -eq 0 ] && [ $C2 -eq 0 ] && [ -s g1.gif ] \
  && cmp -s g1.gif g2.gif \
  && grep -q "NETSCAPE2.0" g1.gif && [ "$(nframes g1.gif)" = "30" ] \
  && grep -q -- "--format gif" r-gif1.log; then
  pass "gif-loops-full-composition-byte-identical-with-warning"
else
  fail "gif-loops-full-composition-byte-identical-with-warning" "exits=$C1/$C2 (want 0/0, equal bytes, NETSCAPE loop, 30 frames, --format gif warning)"
fi
P="$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of default=noprint_wrappers=1 g1.gif 2>/dev/null)"
if echo "$P" | grep -q "codec_name=gif"; then
  pass "gif-probe-codec"
else
  fail "gif-probe-codec" "$(echo "$P" | tr '\n' ' ')"
fi

"$FSMITH" render proj --format pngseq --encoder software -o stills1 >r-png1.log 2>&1
C1=$?
"$FSMITH" render proj --format pngseq --encoder software -o stills2 >r-png2.log 2>&1
C2=$?
N1="$(ls stills1 2>/dev/null | wc -l | tr -d ' ')"
N2="$(ls stills2 2>/dev/null | wc -l | tr -d ' ')"
if [ $C1 -eq 0 ] && [ $C2 -eq 0 ] && [ "$N1" = "30" ] && [ "$N2" = "30" ] \
  && [ -f stills1/frame-0000.png ] && [ -f stills1/frame-0029.png ] \
  && cmp -s stills1/frame-0000.png stills2/frame-0000.png \
  && cmp -s stills1/frame-0014.png stills2/frame-0014.png \
  && cmp -s stills1/frame-0029.png stills2/frame-0029.png; then
  pass "pngseq-thirty-frames-twice-frame-identical"
else
  fail "pngseq-thirty-frames-twice-frame-identical" "exits=$C1/$C2 frames=$N1/$N2 (want 0/0, 30/30, identical spot frames)"
fi
P="$(streams_of stills1/frame-0000.png)"
if echo "$P" | grep -q "codec_name=png" && echo "$P" | grep -q "width=160" \
  && echo "$P" | grep -q "height=120"; then
  pass "pngseq-probe-codec-size"
else
  fail "pngseq-probe-codec-size" "$(echo "$P" | tr '\n' ' ')"
fi

"$FSMITH" render proj --format hls --encoder software -o hls1 >r-hls1.log 2>&1
C1=$?
"$FSMITH" render proj --format hls --encoder software -o hls2 >r-hls2.log 2>&1
C2=$?
SEGS_OK=1
for S in hls1 hls2; do
  if [ ! -f "$S/index.m3u8" ]; then SEGS_OK=0; fi
  for SEG in $(grep -v '^#' "$S/index.m3u8" 2>/dev/null | grep -v '^$'); do
    if [ ! -f "$S/$SEG" ]; then SEGS_OK=0; fi
  done
done
SEGDIFF=0
for SEG in hls1/seg-*.ts; do
  B="hls2/$(basename "$SEG")"
  if [ ! -f "$B" ] || ! cmp -s "$SEG" "$B"; then SEGDIFF=1; fi
done
if [ $C1 -eq 0 ] && [ $C2 -eq 0 ] && [ $SEGS_OK -eq 1 ] && [ $SEGDIFF -eq 0 ] \
  && cmp -s hls1/index.m3u8 hls2/index.m3u8 \
  && grep -q "#EXTM3U" hls1/index.m3u8 && grep -q "#EXT-X-ENDLIST" hls1/index.m3u8; then
  pass "hls-playlist-plus-segments-twice-byte-identical"
else
  fail "hls-playlist-plus-segments-twice-byte-identical" "exits=$C1/$C2 segs_ok=$SEGS_OK segdiff=$SEGDIFF (want 0/0, 1, 0, identical playlists)"
fi
SEG0="$(grep -v '^#' hls1/index.m3u8 | grep -v '^$' | head -n 1)"
P="$(streams_of "hls1/$SEG0")"
if [ -n "$SEG0" ] && echo "$P" | grep -q "codec_name=h264"; then
  pass "hls-segment-probe-codec"
else
  fail "hls-segment-probe-codec" "seg=$SEG0 $(echo "$P" | tr '\n' ' ')"
fi
echo ""

# --- provenance: one digest on every destination -----------------------------------
echo "--- provenance on every output ---"
"$FSMITH" render proj --encoder software -o default.mp4 >r-default.log 2>&1
C1=$?
HEVC_STAMP="$(comment_of hevc1.mp4)"
WEBM_STAMP="$(comment_of a1.webm)"
MOV_STAMP="$(comment_of p1.mov)"
MP4_STAMP="$(comment_of default.mp4)"
PLAYLIST_STAMP="$(grep '^# framesmith ' hls1/index.m3u8 | head -n 1 | sed 's/^# //')"
SIDECAR_DIGEST=""
if [ -f stills1.provenance.json ]; then
  SIDECAR_DIGEST="$(sed -n 's/.*"digest":"\([0-9a-f]*\)".*/\1/p' stills1.provenance.json)"
fi
D_HEVC="$(digest_of "$HEVC_STAMP")"
D_WEBM="$(digest_of "$WEBM_STAMP")"
D_MOV="$(digest_of "$MOV_STAMP")"
D_MP4="$(digest_of "$MP4_STAMP")"
D_HLS="$(digest_of "$PLAYLIST_STAMP")"
WANT_PREFIX="framesmith $VERSION inputs="
PROV_OK=1
for S in "$HEVC_STAMP" "$WEBM_STAMP" "$MOV_STAMP" "$MP4_STAMP" "$PLAYLIST_STAMP"; do
  case "$S" in "$WANT_PREFIX"*) ;; *) PROV_OK=0 ;; esac
done
if [ "$C1" -eq 0 ] && [ $PROV_OK -eq 1 ] \
  && [ -n "$D_HEVC" ] && [ "$D_HEVC" = "$D_WEBM" ] && [ "$D_HEVC" = "$D_MOV" ] \
  && [ "$D_HEVC" = "$D_MP4" ] && [ "$D_HEVC" = "$D_HLS" ] \
  && [ "$D_HEVC" = "$SIDECAR_DIGEST" ] \
  && [ "${#D_HEVC}" = "16" ]; then
  pass "provenance-one-digest-everywhere"
else
  fail "provenance-one-digest-everywhere" "exit=$C1 prov_ok=$PROV_OK digests=[$D_HEVC][$D_WEBM][$D_MOV][$D_MP4][$D_HLS][$SIDECAR_DIGEST] (want one shared 16-hex digest)"
fi
echo "stamp: $HEVC_STAMP"
echo ""

# --- two presets -------------------------------------------------------------------
echo "--- two resolution presets ---"
"$FSMITH" render proj --resolution 720p --encoder software -o p720.mp4 >r-720.log 2>&1
C1=$?
"$FSMITH" render proj --resolution 480p --encoder software -o p480.mp4 >r-480.log 2>&1
C2=$?
P7="$(streams_of p720.mp4)"
P4="$(streams_of p480.mp4)"
if [ $C1 -eq 0 ] && echo "$P7" | grep -q "width=960" && echo "$P7" | grep -q "height=720"; then
  pass "preset-720p-size"
else
  fail "preset-720p-size" "exit=$C1 $(echo "$P7" | tr '\n' ' ') (want 960x720)"
fi
if [ $C2 -eq 0 ] && echo "$P4" | grep -q "width=640" && echo "$P4" | grep -q "height=480"; then
  pass "preset-480p-size"
else
  fail "preset-480p-size" "exit=$C2 $(echo "$P4" | tr '\n' ' ') (want 640x480)"
fi
echo ""

# --- rate control --------------------------------------------------------------------
echo "--- keyframe and peak-bitrate control ---"
"$FSMITH" render proj --encoder software --keyint 5 --maxrate 8M -o capped.mp4 >r-capped.log 2>&1
C1=$?
if [ $C1 -eq 0 ] && [ -s capped.mp4 ] && grep -q "keyint=5" r-capped.log \
  && grep -q "maxrate=8M" r-capped.log && [ "$(nframes capped.mp4)" = "30" ]; then
  pass "rate-control-report-and-frames"
else
  fail "rate-control-report-and-frames" "exit=$C1 (want 0, keyint=5 + maxrate=8M named, 30 frames)"
fi
cat > keygaps.py <<'EOF'
import subprocess, sys
p = subprocess.run(['ffprobe','-v','error','-select_streams','v:0',
                    '-show_entries','frame=pict_type','-of','csv=p=0','capped.mp4'],
                   capture_output=True, text=True)
last, gaps = None, []
for i, line in enumerate(p.stdout.splitlines()):
    if line.strip().split(',')[0] == 'I':
        if last is not None: gaps.append(i - last)
        last = i
print("gaps:", gaps)
sys.exit(0 if len(gaps) >= 5 and all(g <= 5 for g in gaps) else 1)
EOF
if python3 keygaps.py >keygaps.log 2>&1; then
  pass "keyint-gaps-honored"
else
  fail "keyint-gaps-honored" "$(cat keygaps.log 2>/dev/null | tr '\n' ' ') (want >=5 gaps, each <=5)"
fi
echo ""

# --- HDR variant -----------------------------------------------------------------------
echo "--- HDR end to end ---"
"$FSMITH" render proj --composition hdr --hdr auto --encoder software -o hdr1.mp4 >r-hdr1.log 2>&1
C1=$?
"$FSMITH" render proj --composition hdr --hdr auto --encoder software -o hdr2.mp4 >r-hdr2.log 2>&1
C2=$?
HP="$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,pix_fmt,color_transfer,color_primaries,color_space -of default=noprint_wrappers=1 hdr1.mp4 2>/dev/null)"
if [ $C1 -eq 0 ] && [ $C2 -eq 0 ] && [ "$(sha hdr1.mp4)" = "$(sha hdr2.mp4)" ] \
  && echo "$HP" | grep -q "codec_name=hevc" && echo "$HP" | grep -q "pix_fmt=yuv420p10le" \
  && echo "$HP" | grep -q "color_transfer=smpte2084" && echo "$HP" | grep -q "color_primaries=bt2020" \
  && grep -q "Main10" r-hdr1.log && grep -q "smpte2084" r-hdr1.log; then
  pass "hdr-auto-hevc-main10-bt2020-byte-identical"
else
  fail "hdr-auto-hevc-main10-bt2020-byte-identical" "exits=$C1/$C2 $(echo "$HP" | tr '\n' ' ') (want hevc/yuv420p10le/smpte2084/bt2020 + Main10 report + equal hashes)"
fi
"$FSMITH" render proj --composition hdr --hdr pq --encoder software -o hdr-pq.mp4 >r-hdrpq.log 2>&1
C1=$?
if [ $C1 -eq 0 ] && grep -q "matched PQ" r-hdrpq.log \
  && ffprobe -v error -select_streams v:0 -show_entries stream=color_transfer -of csv=p=0 hdr-pq.mp4 2>/dev/null | grep -q "smpte2084"; then
  pass "hdr-explicit-pq"
else
  fail "hdr-explicit-pq" "exit=$C1 (want 0 with matched PQ + smpte2084)"
fi
"$FSMITH" render proj --composition hdr --format gif --encoder software -o hdr.gif >r-hdrgif.log 2>&1
C1=$?
if [ $C1 -eq 0 ] && [ -s hdr.gif ] && grep -q "tone-mapped" r-hdrgif.log \
  && grep -q "smpte2084" r-hdrgif.log && grep -q -- "--format gif" r-hdrgif.log \
  && [ "$(nframes hdr.gif)" = "30" ]; then
  pass "hdr-tone-mapped-gif"
else
  fail "hdr-tone-mapped-gif" "exit=$C1 (want 0, tone-mapped smpte2084 to --format gif, 30 frames)"
fi
echo ""

# --- the pictures: text mark on every codec, alpha composited like the MP4 ------------
echo "--- pictures survive every destination ---"
"$FSMITH" render proj --encoder software -o ref.mp4 >r-ref.log 2>&1
C1=$?
if [ $C1 -ne 0 ]; then
  fail "pictures-reference-render" "exit=$C1 (want 0)"
fi
cat > pixels.py <<'EOF'
import subprocess, sys
def dec(path, frames, w, h):
    p = subprocess.run(['ffmpeg','-y','-v','error','-i',path,
                        '-frames:v',str(frames),
                        '-f','rawvideo','-pix_fmt','rgb24','-'],
                       capture_output=True)
    assert p.returncode == 0, path
    assert len(p.stdout) == frames*w*h*3, (path, len(p.stdout))
    return p.stdout
def marked(raw, bg):
    n = 0
    for i in range(0, len(raw), 3):
        if abs(raw[i]-bg[0]) + abs(raw[i+1]-bg[1]) + abs(raw[i+2]-bg[2]) > 100:
            n += 1
    return n
def mad(a, b):
    return sum(abs(x-y) for x, y in zip(a, b))/len(a)
ok = True
def check(name, cond, detail=""):
    global ok
    print(("PASS-ASSERT " if cond else "FAIL-ASSERT ") + name + " " + detail)
    if not cond: ok = False
BG = (0x11, 0x22, 0x33)
for path, w, h in [('hevc1.mp4',160,120), ('a1.webm',160,120),
                   ('p720.mp4',960,720), ('p480.mp4',640,480)]:
    m = marked(dec(path,1,w,h), BG)
    check("text-mark-"+path, m > 50, "marked=%d (want >50)" % m)
w = dec('a1.webm',1,160,120)
r = dec('ref.mp4',1,160,120)
check("webm-alpha-like-mp4", mad(w,r) < 12.0, "mad=%.3f (want <12)" % mad(w,r))
print("ALL-ASSERTS-PASS" if ok else "ASSERTS-FAILED")
sys.exit(0 if ok else 1)
EOF
if [ $C1 -eq 0 ] && python3 pixels.py >pixels.log 2>&1 && grep -q "ALL-ASSERTS-PASS" pixels.log; then
  pass "pictures-text-and-alpha-everywhere"
else
  fail "pictures-text-and-alpha-everywhere" "$(grep "FAIL-ASSERT" pixels.log 2>/dev/null | head -3 | tr '\n' ';') (see pixels.log)"
fi
echo ""

# --- hardware axis: auto succeeds, explicit without hardware refuses --------------------
echo "--- encoder axis ---"
"$FSMITH" render proj --encoder auto -o auto.mp4 >r-auto.log 2>&1
C1=$?
if [ $C1 -eq 0 ] && [ -s auto.mp4 ] && [ "$(nframes auto.mp4)" = "30" ]; then
  pass "encoder-auto-succeeds"
else
  fail "encoder-auto-succeeds" "exit=$C1 (want 0 with 30 frames)"
fi
FRAMESMITH_NO_VIDEOTOOLBOX=1 "$FSMITH" render proj --encoder videotoolbox -o vtb.mp4 >r-vtb.log 2>&1
C1=$?
if [ $C1 -ne 0 ] && grep -q -- "--encoder videotoolbox" r-vtb.log \
  && grep -q "software" r-vtb.log && [ ! -e vtb.mp4 ] \
  && [ ! -e vtb.mp4.framesmith-part ]; then
  pass "refuses-videotoolbox-without-hardware"
else
  fail "refuses-videotoolbox-without-hardware" "exit=$C1 (want non-zero naming --encoder videotoolbox + software fallback, nothing written)"
fi
echo ""

# --- every invalid destination refuses before frame one ----------------------------------
echo "--- invalid destinations refuse ---"
refuse() {
  # $1=name $2=want-exit, then [--out=F]... plus grep patterns, then `--`,
  # then the render args. Renders to r-refuse-<name>.log and requires the
  # exit, every pattern, and no output file (nor staged partial) for --out.
  NAME="$1"; WANT="$2"; shift 2
  OUT="r-refuse-$NAME.log"
  OUTFILES=""
  PATFILE="$OUT.pats"
  : > "$PATFILE"
  while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
    case "$1" in
      --out=*) OUTFILES="$OUTFILES ${1#--out=}" ;;
      *) printf '%s\n' "$1" >> "$PATFILE" ;;
    esac
    shift
  done
  if [ "$#" -gt 0 ]; then shift; fi
  "$FSMITH" render proj "$@" >"$OUT" 2>&1
  CODE=$?
  OK=1
  case "$WANT" in
    nonzero) [ $CODE -ne 0 ] || OK=0 ;;
    *) [ $CODE -eq "$WANT" ] || OK=0 ;;
  esac
  while IFS= read -r PAT; do
    grep -q -- "$PAT" "$OUT" || OK=0
  done < "$PATFILE"
  for F in $OUTFILES; do
    if [ -e "$F" ] || [ -e "$F.framesmith-part" ]; then OK=0; fi
  done
  if [ $OK -eq 1 ]; then
    pass "refuses-$NAME"
  else
    fail "refuses-$NAME" "exit=$CODE (want $WANT; see $OUT)"
  fi
}
refuse unknown-format nonzero --out=x-fmt.mp4 -- "--format" "mp4, hevc, webm, prores, gif, pngseq, or hls" \
  -- --format avi -o x-fmt.mp4
refuse prores-profile-on-webm nonzero --out=x-prof.webm -- "--prores-profile" "--format prores" \
  -- --format webm --prores-profile hq -o x-prof.webm
refuse unknown-preset nonzero --out=x-res.mp4 -- "--resolution" "720p" \
  -- --resolution 4k -o x-res.mp4
refuse bad-supersample nonzero --out=x-ss.mp4 -- "--supersample" \
  -- --supersample 0 -o x-ss.mp4
refuse unknown-encoder nonzero --out=x-enc.mp4 -- "--encoder" \
  -- --encoder cuda -o x-enc.mp4
refuse bad-keyint nonzero --out=x-key.mp4 -- "--keyint" \
  -- --keyint 0 -o x-key.mp4
refuse bad-maxrate nonzero --out=x-max.mp4 -- "--maxrate" \
  -- --maxrate bogus -o x-max.mp4
refuse hdr-pq-on-sdr nonzero --out=x-hdr.mp4 -- "--hdr pq" "SDR" \
  -- --hdr pq -o x-hdr.mp4
refuse hdr-pq-to-gif nonzero --out=x-hdrg.gif -- "cannot carry HDR" "--format gif" \
  -- --composition hdr --format gif --hdr pq -o x-hdrg.gif
refuse hdr-webm-auto nonzero --out=x-hdrw.webm -- "cannot carry HDR" "--format webm" \
  -- --composition hdr --format webm -o x-hdrw.webm
refuse duplicate-flag 2 --out=x-dup.mp4 -- "duplicate --format" \
  -- --format mp4 --format hevc -o x-dup.mp4
printf 'no' > blocker
refuse unwritable-output nonzero "blocker" \
  -- --format webm -o blocker/out.webm
rm -f blocker
echo ""

# --- tampered reference refuses; vanished source stops with nothing complete --------------
echo "--- tamper and mid-render failure ---"
rm -rf tamper-case && mkdir -p tamper-case
cp proj/composition.kdl proj/clip.mov proj/tone.wav proj/hdr-pq.mp4 tamper-case/
sed 's/width=160/width=0/' tamper-case/composition.kdl > tamper-case/composition.kdl.tmp
mv tamper-case/composition.kdl.tmp tamper-case/composition.kdl
"$FSMITH" render tamper-case -o tampered.mp4 >r-tampered.log 2>&1
CODE=$?
if [ $CODE -ne 0 ] && [ ! -f tampered.mp4 ] && [ ! -f tampered.mp4.framesmith-part ]; then
  pass "tampered-reference-refused-without-mp4"
else
  fail "tampered-reference-refused-without-mp4" "exit=$CODE (want non-zero, no MP4)"
fi

"$FSMITH" render proj -o ok.mp4 >r-ok.log 2>&1
CODE=$?
if [ $CODE -ne 0 ]; then
  fail "mid-render-baseline" "exit=$CODE (want 0 before removing the source)"
fi
rm proj/tone.wav
"$FSMITH" render proj -o gone.mp4 >r-gone.log 2>&1
C1=$?
"$FSMITH" render proj --format pngseq -o gone-seq >r-goneseq.log 2>&1
C2=$?
"$FSMITH" render proj --format hls -o gone-hls >r-gonehls.log 2>&1
C3=$?
if [ $C1 -ne 0 ] && grep -q "tone.wav" r-gone.log && [ ! -e gone.mp4 ] \
  && [ ! -e gone.mp4.framesmith-part ] \
  && [ $C2 -ne 0 ] && grep -q "tone.wav" r-goneseq.log && [ ! -e gone-seq ] \
  && [ $C3 -ne 0 ] && grep -q "tone.wav" r-gonehls.log && [ ! -e gone-hls/index.m3u8 ]; then
  pass "vanished-source-stops-naming-cause-nothing-complete"
else
  fail "vanished-source-stops-naming-cause-nothing-complete" "exits=$C1/$C2/$C3 (want non-zero naming tone.wav, no file/dir/playlist)"
fi
echo ""

# --- the reference itself must be untouched by every render --------------------------------
if [ "$(sha proj/composition.kdl)" = "$KDL_BEFORE" ]; then
  pass "reference-unchanged-on-disk"
else
  fail "reference-unchanged-on-disk" "composition.kdl changed during renders"
fi
echo ""

# --- evidence --------------------------------------------------------------------------------
mkdir -p "$EVID_DIR"
{
  echo "n12 outputs proof gate (T006)"
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $("$FSMITH" --version 2>/dev/null || echo present)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1 || echo none)"
  echo "cargo: $(cargo --version 2>/dev/null || echo MISSING)"
  echo "rustc: $(rustc --version 2>/dev/null || echo MISSING)"
  echo "stamp: $HEVC_STAMP"
  echo ""
  echo "sha256:"
  for F in hevc1.mp4 a1.webm p1.mov g1.gif hdr1.mp4 capped.mp4 p720.mp4 p480.mp4 default.mp4; do
    if [ -f "$F" ]; then echo "  $(sha "$F")  $F"; fi
  done
  for F in stills1/frame-0000.png stills1/frame-0029.png; do
    if [ -f "$F" ]; then echo "  $(sha "$F")  $F"; fi
  done
  if [ -f hls1/index.m3u8 ]; then echo "  $(sha hls1/index.m3u8)  hls1/index.m3u8"; fi
  for S in hls1/seg-*.ts; do
    if [ -f "$S" ]; then echo "  $(sha "$S")  $S"; fi
  done
} >"$WORKDIR/REPORT.md"
cp "$WORKDIR/REPORT.md" "$EVID_DIR/REPORT.md"
cp proj/composition.kdl "$EVID_DIR/composition.kdl"
[ -f hevc1.mp4 ] && cp hevc1.mp4 "$EVID_DIR/outputs-reference-hevc.mp4"
[ -f a1.webm ] && cp a1.webm "$EVID_DIR/outputs-reference.webm"
[ -f p1.mov ] && cp p1.mov "$EVID_DIR/outputs-reference-prores.mov"
[ -f g1.gif ] && cp g1.gif "$EVID_DIR/outputs-reference.gif"
[ -f stills1/frame-0000.png ] && cp stills1/frame-0000.png "$EVID_DIR/outputs-reference-frame-0000.png"
[ -f stills1.provenance.json ] && cp stills1.provenance.json "$EVID_DIR/outputs-reference.provenance.json"
[ -f hls1/index.m3u8 ] && cp hls1/index.m3u8 "$EVID_DIR/outputs-reference-index.m3u8"
cat check.log r-hevc1.log r-webm1.log r-prores1.log r-gif1.log r-png1.log r-hls1.log \
  r-default.log r-720.log r-480.log r-capped.log keygaps.log r-hdr1.log r-hdrpq.log \
  r-hdrgif.log pixels.log r-auto.log r-vtb.log r-gone.log >"$WORKDIR/proof.log" 2>/dev/null
cp "$WORKDIR/proof.log" "$EVID_DIR/proof.log" 2>/dev/null
echo "evidence: $EVID_DIR"
echo ""

# --- gate --------------------------------------------------------------------------------------
echo "checks passed: $PASS_N"
if [ -z "$FAIL_LIST" ]; then
  echo "PROOF: PASS"
  exit 0
else
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
