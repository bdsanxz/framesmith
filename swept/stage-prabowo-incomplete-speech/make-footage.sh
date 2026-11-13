#!/usr/bin/env bash
# Everything this inspiration's film is cut from, made again from nothing: the presenter clip (the
# source video, then its rough cut), the fonts, the music (three tracks joined into one bed), and
# through make-evidence.sh and make-pictures.sh the page captures, the clips of the speech, the
# photos, the b-roll, and the icons. Needs yt-dlp, ffmpeg, Node.js, and python3. A step is skipped
# when its file is already there. The videos and the pages are live: a later run may differ.
set -euo pipefail
cd "$(dirname "$0")"
K=../../scripts

# the presenter: Pandji Pragiwaksono, "PIDATO TIDAK PARIPURNA PRABOWO" (28 May 2026), used at the user's request
mkdir -p source footage assets/audio/src
[ -s source/original.mp4 ] || yt-dlp -f "bv*[height<=1080][ext=mp4]+ba[ext=m4a]/b[height<=1080]" --merge-output-format mp4 \
  -o source/original.mp4 "https://www.youtube.com/watch?v=6yWfCnyOP2Q"
# the rough cut: the talk up to "public speaking wise, bagus" (11:00.2), the bumper dropped, pauses and filler sounds out
[ -s footage/aroll.mp4 ] || python3 $K/rough-cut.py source/original.mp4 -o footage/aroll.mp4 --lang id --to 660.25 --drop 52.6-66.72 --verify

# the four faces
[ -s assets/fonts/archivo-latin.woff2 ] || node $K/fetch-font.mjs "Archivo" 500,700,800 > /dev/null
[ -s assets/fonts/instrument-serif-italic-latin.woff2 ] || node $K/fetch-font.mjs "Instrument Serif" 400i > /dev/null
[ -s assets/fonts/bricolage-grotesque-latin.woff2 ] || node $K/fetch-font.mjs "Bricolage Grotesque" 800 > /dev/null
[ -s assets/fonts/jetbrains-mono-latin.woff2 ] || node $K/fetch-font.mjs "JetBrains Mono" 500 > /dev/null

# the music: three tracks by Kevin MacLeod (incompetech.com), CC BY 4.0, levelled and joined with 4 s crossfades
get() { [ -s "assets/audio/src/$1.mp3" ] || curl -fsSL -o "assets/audio/src/$1.mp3" "https://incompetech.com/music/royalty-free/mp3-royaltyfree/$2.mp3"; }
get dispersion-relation "Dispersion%20Relation"
get floating-cities "Floating%20Cities"
get dream-culture "Dream%20Culture"
[ -s assets/audio/bed.mp3 ] || ffmpeg -nostdin -v error -y -i assets/audio/src/dispersion-relation.mp3 -i assets/audio/src/floating-cities.mp3 -i assets/audio/src/dream-culture.mp3 \
  -filter_complex "[0:a]loudnorm=I=-20:TP=-2:LRA=11[a];[1:a]loudnorm=I=-20:TP=-2:LRA=11[b];[2:a]loudnorm=I=-20:TP=-2:LRA=11[c];[a][b]acrossfade=d=4:c1=tri:c2=tri[ab];[ab][c]acrossfade=d=4:c1=tri:c2=tri[out]" \
  -map "[out]" -ar 48000 -b:a 192k assets/audio/bed.mp3

# where the presenter is in the frame: the film keeps his background, so no cut-out is made
[ -s footage/aroll.layers.json ] || python3 $K/presenter-layers.py footage/aroll.mp4 --track

bash make-evidence.sh
bash make-pictures.sh
echo "footage ready: python3 $K/deep-tracery-build.py, then duck-under-voice.py, check, render (README.md)"
