#!/usr/bin/env bash
# Make this inspiration's media with no hand work (except the short sounds, below): the Universal newsreel of 10 May 1937 (Commons),
# transcoded once and cut by scripts/fetch-shots.py (shots.json); four stills; the cut-outs of
# Hugo Eckener and the airship printed as paper stickers; the dark locator map and the paper map
# (scripts/map-svg.py); the paper sheet; a music bed, a blast, and a fire from Freesound; then
# CREDITS.md. Everything lands in assets/generated/, footage/, and .footage/ (not committed).
# About 2 minutes after the AI environment exists. Run from the inspiration folder; SKILL_DIR = the skill root.
set -euo pipefail
SKILL_DIR=${SKILL_DIR:-../..}
S="$SKILL_DIR/scripts"
G=assets/generated
mkdir -p "$G/src" .footage/src
UA="direcfy-video-skill/1.0 (HyperFrames video production)"
get() { [ -s "$1" ] || curl -sfL -A "$UA" -o "$1" "$2"; }

# The picks, from asset-search.py video "Hindenburg disaster newsreel 1937", image "Hindenburg
# disaster 1937 photograph", "Hindenburg interior lounge", "Hindenburg airship 1936", "Hugo Eckener
# portrait", and audio "dark suspense cinematic ambient", "explosion", "fire roar".
FILM_URL="https://upload.wikimedia.org/wikipedia/commons/e/e4/1937-05-10_Special_Release_-_Zeppelin_Explodes_Scores_Dead.ogv"
FIRE_URL="https://upload.wikimedia.org/wikipedia/commons/1/1c/Hindenburg_disaster.jpg"
LOUNGE_URL="https://upload.wikimedia.org/wikipedia/commons/a/a5/LZ_129_Hindenburg_interior.jpg"
SHIP_URL="https://upload.wikimedia.org/wikipedia/commons/4/41/Hindenburg_observation_windows_1936.jpg"
ECK_URL="https://images.rawpixel.com/editor_1024/cHJpdmF0ZS9sci9pbWFnZXMvd2Vic2l0ZS8yMDIyLTA1L2xvYzIwMTY4NDk2NjUtaW1hZ2UuanBn.jpg"
BED_URL="https://cdn.freesound.org/previews/475/475737_6900479-hq.mp3"
BLAST_URL="https://cdn.freesound.org/previews/264/264031_3797507-hq.mp3"
ROAR_URL="https://cdn.freesound.org/previews/386/386560_7201455-hq.mp3"
get .footage/src/universal.ogv "$FILM_URL"
get "$G/src/fire.jpg" "$FIRE_URL"; get "$G/src/lounge.jpg" "$LOUNGE_URL"; get "$G/src/ship.jpg" "$SHIP_URL"; get "$G/src/eckener.jpg" "$ECK_URL"
get "$G/src/bed.mp3" "$BED_URL"; get "$G/src/explosion.mp3" "$BLAST_URL"; get "$G/src/fire-roar.mp3" "$ROAR_URL"
cat > sources.jsonl <<JSON
{"file": ".footage/src/universal.ogv", "source": "wikimedia", "url": "$FILM_URL", "page": "https://commons.wikimedia.org/wiki/File:1937-05-10_Special_Release_-_Zeppelin_Explodes_Scores_Dead.ogv", "title": "Special Release: Zeppelin Explodes, Scores Dead (10 May 1937)", "creator": "Universal Newsreel", "license": "Public domain"}
{"file": "$G/src/fire.jpg", "source": "wikimedia", "url": "$FIRE_URL", "page": "https://commons.wikimedia.org/wiki/File:Hindenburg_disaster.jpg", "title": "Hindenburg disaster", "creator": "Sam Shere", "license": "Public domain"}
{"file": "$G/src/lounge.jpg", "source": "wikimedia", "url": "$LOUNGE_URL", "page": "https://commons.wikimedia.org/wiki/File:LZ_129_Hindenburg_interior.jpg", "title": "LZ 129 Hindenburg interior", "creator": "Nationaal Archief", "license": "No restrictions"}
{"file": "$G/src/ship.jpg", "source": "wikimedia", "url": "$SHIP_URL", "page": "https://commons.wikimedia.org/wiki/File:Hindenburg_observation_windows_1936.jpg", "title": "Hindenburg observation windows 1936", "creator": "U.S. Army, 8th Photo Section", "license": "Public domain"}
{"file": "$G/src/eckener.jpg", "source": "openverse/rawpixel", "url": "$ECK_URL", "page": "https://www.rawpixel.com/image/6977424/dr-hugo-eckener-101624", "title": "Dr. Hugo Eckener, 10/16/24", "creator": "Library of Congress", "license": "CC0"}
{"file": "$G/src/bed.mp3", "source": "freesound", "url": "$BED_URL", "page": "https://freesound.org/people/Magmi.Soundtracks/sounds/475737", "title": "Suspenseful Strings Music 01", "creator": "Magmi.Soundtracks", "license": "CC0"}
{"file": "$G/src/explosion.mp3", "source": "freesound", "url": "$BLAST_URL", "page": "https://freesound.org/people/Logicogonist/sounds/264031", "title": "low mid afar explosion 1", "creator": "Logicogonist", "license": "CC0"}
{"file": "$G/src/fire-roar.mp3", "source": "freesound", "url": "$ROAR_URL", "page": "https://freesound.org/people/laptaper/sounds/386560", "title": "fire roar and crackle", "creator": "laptaper", "license": "CC0"}
{"file": "assets/generated/map.svg", "source": "naturalearth", "url": "https://www.naturalearthdata.com/", "page": "https://github.com/nvkelso/natural-earth-vector", "title": "Natural Earth vector data (map)", "creator": "Natural Earth", "license": "Public domain"}
JSON

# The newsreel is Theora: a cut that starts mid-file can open on a green frame. Transcode it once,
# record what the copy was made from, and cut the eight shots from the copy (shots.json).
[ -s .footage/src/universal.mp4 ] || ffmpeg -nostdin -v error -y -i .footage/src/universal.ogv -an -c:v libx264 -crf 15 -preset fast -g 15 -pix_fmt yuv420p .footage/src/universal.mp4
python3 "$S/made.py" .footage/src/universal.mp4 .footage/src/universal.ogv >/dev/null
python3 "$S/fetch-shots.py" shots.json >/dev/null

# The cut-outs, printed as paper stickers (mono, halftone, scissor-cut border). Eckener stands in a
# crowd, so the cut-out looks only inside his box; the 1024 px source is doubled before printing so
# the halftone dots stay fine at the size shown.
[ -s "$G/eckener.png" ] || { python3 "$S/cutout.py" "$G/src/eckener.jpg" -o "$G/src/eckener-cut.png" --box 290,165,745,715 --trim
  ffmpeg -nostdin -v error -y -i "$G/src/eckener-cut.png" -vf "scale=iw*2:ih*2:flags=lanczos" "$G/src/eckener-cut2.png"
  python3 "$S/made.py" "$G/src/eckener-cut2.png" "$G/src/eckener-cut.png" >/dev/null
  python3 "$S/paper-cutout.py" "$G/src/eckener-cut2.png" -o "$G/eckener.png"; }
[ -s "$G/ship.png" ] || { python3 "$S/cutout.py" "$G/src/ship.jpg" -o "$G/src/ship-cut.png" --trim; python3 "$S/paper-cutout.py" "$G/src/ship-cut.png" -o "$G/ship.png"; }

# One neutral paper sheet, multiplied over the collage block from the host.
[ -s "$G/paper.jpg" ] || python3 "$S/paper-texture.py" -o "$G/paper.jpg" --size 2112x1188 --mottle 0.07 --burn 0.22 --seed 11

# The dark locator map: the contiguous USA, New Jersey filled, a pin on Lakehurst. The camera flies
# in 3x, so the strokes are drawn thin. Then the paper map of the collage block: Texas filled, a pin on Amarillo.
[ -s "$G/map.svg" ] || python3 "$S/map-svg.py" --focus "United States of America" --main-part --proj albers \
  --admin1 "United States of America" --highlight "New Jersey" --pin "Lakehurst=40.033,-74.353" --anchor 0.5,0.5 --pad 0.08 -o "$G/map.svg" \
  --css ".land{fill:#0d1a22;stroke:#1c3a48;stroke-width:.6}.land.focus{fill:#1f6f8b;stroke:#8fe0f5;stroke-width:1.1}.admin1{fill:none;stroke:#9fdcec;stroke-width:.45;stroke-opacity:.5}.hl{fill:rgba(74,190,128,.6);stroke:#7ff0b0;stroke-width:.9}"
[ -s "$G/usmap.svg" ] || python3 "$S/map-svg.py" --focus "United States of America" --main-part --proj albers \
  --admin1 "United States of America" --highlight "Texas" --pin "Amarillo=35.22,-101.83" --anchor 0.5,0.5 --pad 0.1 -o "$G/usmap.svg" \
  --css ".land{fill:#d9cdb0;stroke:#a89878;stroke-width:1}.land.focus{fill:#efe6cf;stroke:#5b5140;stroke-width:2}.admin1{fill:none;stroke:#7d7058;stroke-width:.9;stroke-opacity:.55}.hl{fill:#e0554a;stroke:#8c2a22;stroke-width:2}"
# The short sounds the compositions name (Pixabay effects when the film was made; the skill bundles
# no sounds): put your own in assets/sfx/ (scripts/asset-search.py audio finds them). A missing one is
# named here and its slot stays silent.
for f in whoosh whoosh-short pop typing impact-bass-1; do [ -s "assets/sfx/$f.mp3" ] || echo "note: no assets/sfx/$f.mp3, so its hits stay silent" >&2; done
python3 "$S/footage-credits.py" >/dev/null
