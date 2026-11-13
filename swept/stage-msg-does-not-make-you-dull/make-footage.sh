#!/usr/bin/env bash
# Make this inspiration's media with no hand work: the 42 shots (cut by scripts/fetch-shots.py from
# shots.json: 23 stock uploads on YouTube, five Mixkit clips, and four Wikimedia Commons clips that
# are downloaded once each, because Commons refuses repeated range requests), the music bed
# ("Arcade Aroma" by Touda, Freesound, CC0), then CREDITS.md. It also makes a paper sheet
# (scripts/paper-texture.py) where assets/paper/crumpled-white.jpg is missing. Not shipped and not
# made here: the seam sounds (assets/audio/paper-flash.mp3, paper-tear.mp3,
# assets/sfx/whoosh-short.mp3; find them with scripts/asset-search.py audio, or the seams stay
# silent), the fonts (scripts/fetch-font.mjs), and the narration audio (speak narration.txt again
# with scripts/narrate.sh). Footage lands in footage/ and .footage/ (not committed). About 10
# minutes and 350 MB. Run from the inspiration folder; SKILL_DIR = the skill root.
set -euo pipefail
SKILL_DIR=${SKILL_DIR:-../..}
S="$SKILL_DIR/scripts"
mkdir -p .footage/src assets/audio
UA="direcfy-video-skill/1.0 (HyperFrames video production)"
get() { [ -s "$1" ] || curl -sfL --retry 3 --retry-delay 5 -A "$UA" -o "$1" "$2"; }
get .footage/src/commons-tonkotsu-ramen.webm "https://upload.wikimedia.org/wikipedia/commons/8/8b/Tonkotsu_ramen_-_tokyo_area_June_20_2021.webm"
get .footage/src/commons-miso-soup-kitchen.webm "https://upload.wikimedia.org/wikipedia/commons/f/ff/Preparing_miso_soup_in_a_Japanese_kitchen_%28July_2025%29.webm"
get .footage/src/commons-eating-noodles.webm "https://upload.wikimedia.org/wikipedia/commons/7/7b/Noodle-harassment_8jo_WATANABEhachijo.webm"
get .footage/src/commons-beach-seaweed.webm "https://upload.wikimedia.org/wikipedia/commons/1/1a/Beach_Seaweed_1_2018-06-17.webm"
get assets/audio/bed.mp3 "https://cdn.freesound.org/previews/614/614130_5848783-hq.mp3"
[ -s assets/paper/crumpled-white.jpg ] || { mkdir -p assets/paper; python3 "$S/paper-texture.py" -o assets/paper/crumpled-white.jpg --size 1080x1920 --folds 3,2; }
python3 "$S/fetch-shots.py"
python3 "$S/footage-credits.py" >/dev/null
echo "media ready: footage/, assets/audio/bed.mp3, assets/paper/crumpled-white.jpg, CREDITS.md"
