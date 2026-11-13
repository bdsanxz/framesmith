#!/usr/bin/env bash
# Everything this inspiration's film is cut from, made again from nothing: the presenter clip (the
# source video, then its rough cut), the fonts, the music, and through make-evidence.sh and
# make-pictures.sh the page captures, the news clips, the photos, the b-roll, and the icons.
# Needs yt-dlp, ffmpeg, Node.js, and python3. A step is skipped when its file is already there.
# The source video and the pages are live: a capture or a download made later may differ.
set -euo pipefail
cd "$(dirname "$0")"
K=../../scripts

# the presenter: Pandji Pragiwaksono, "NADIEM & FRUSTRASI KITA" (22 May 2026), used at the user's request
mkdir -p source footage assets/audio
[ -s source/original.mp4 ] || yt-dlp -f "bv*[height<=1080][ext=mp4]+ba[ext=m4a]/b[height<=1080]" --merge-output-format mp4 \
  -o source/original.mp4 "https://www.youtube.com/watch?v=69Qh0Bhil4M"
# the rough cut: the talk up to his second topic (11:38.6), the bumper dropped, pauses and filler sounds out
[ -s footage/aroll.mp4 ] || python3 $K/rough-cut.py source/original.mp4 -o footage/aroll.mp4 --lang id --to 698.6 --drop 13.69-27.98 --verify

# the four faces
[ -s assets/fonts/archivo-latin.woff2 ] || node $K/fetch-font.mjs "Archivo" 500,700,800 > /dev/null
[ -s assets/fonts/instrument-serif-italic-latin.woff2 ] || node $K/fetch-font.mjs "Instrument Serif" 400i > /dev/null
[ -s assets/fonts/bricolage-grotesque-latin.woff2 ] || node $K/fetch-font.mjs "Bricolage Grotesque" 800 > /dev/null
[ -s assets/fonts/jetbrains-mono-latin.woff2 ] || node $K/fetch-font.mjs "JetBrains Mono" 500 > /dev/null

# the music: "Perspectives" by Kevin MacLeod (incompetech.com), CC BY 4.0
[ -s assets/audio/bed.mp3 ] || curl -fsSL -o assets/audio/bed.mp3 "https://incompetech.com/music/royalty-free/mp3-royaltyfree/Perspectives.mp3"

# the presenter cut out of his room, for the stretches of the clip the film shows him in, and a still of that room
[ -s footage/aroll.subject.webm ] || python3 $K/presenter-layers.py footage/aroll.mp4 --only "$(python3 $K/deep-tracery-build.py --on-screen)"
# the background he is set over: "Three Somber Windows" by Ehud Neuhaus (Unsplash, 2017, through Wikimedia
# Commons), CC0 1.0, a brick wall with window boxes like his own, cropped, coloured, and softened to his room
[ -s source/three-somber-windows.jpg ] || curl -fsSL -A "direcfy-inspiration/1.0" -o source/three-somber-windows.jpg \
  "https://commons.wikimedia.org/wiki/Special:FilePath/Three_Somber_Windows_(Unsplash).jpg"
[ -s assets/backdrop.jpg ] || python3 $K/backdrop-match.py source/three-somber-windows.jpg --room footage/aroll.room.png --crop 2950,2480,2800 -o assets/backdrop.jpg

bash make-evidence.sh
bash make-pictures.sh
echo "footage ready: python3 $K/deep-tracery-build.py, then duck-under-voice.py, check, render (README.md)"
