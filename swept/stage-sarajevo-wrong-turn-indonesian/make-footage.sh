#!/usr/bin/env bash
# Make this inspiration's media with no hand work: download the picks found with scripts/asset-search.py,
# cut the people and the pistol out (scripts/cutout.py) and print them as paper stickers
# (scripts/paper-cutout.py), draw the locator map (scripts/map-svg.py), make the paper
# (scripts/paper-texture.py), fetch the music bed and the shot, then write CREDITS.md. Everything
# lands in assets/generated/ (not committed). About 1 minute after the AI environment exists.
# Run from the inspiration folder; SKILL_DIR = the skill root.
set -euo pipefail
SKILL_DIR=${SKILL_DIR:-../..}
S="$SKILL_DIR/scripts"
G=assets/generated
mkdir -p "$G/src"
UA="direcfy-video-skill/1.0 (HyperFrames video production)"
get() { [ -s "$G/src/$1" ] || curl -sfL -A "$UA" -o "$G/src/$1" "$2"; }

# The picks, from asset-search.py image "Archduke Franz Ferdinand portrait", "Gavrilo Princip",
# "Nedeljko Čabrinović" (the trial photo), "FN Model 1910 pistol", and audio "suspense cinematic",
# "pistol gunshot".
FF_URL="https://upload.wikimedia.org/wikipedia/commons/d/d1/Ferdinand_Schmutzer_-_Franz_Ferdinand_von_%C3%96sterreich-Este%2C_um_1914.jpg"
GP_URL="https://upload.wikimedia.org/wikipedia/commons/6/67/Gavrilo_Princip%2C_cell%2C_headshot.jpg"
TRIAL_URL="https://upload.wikimedia.org/wikipedia/commons/c/cb/Sarajevo_trial%2C_accused%2C_%C4%8Cabrinovi%C4%87%2C_Princip%2C_Ili%C4%87.jpg"
GUN_URL="https://upload.wikimedia.org/wikipedia/commons/6/6f/Browning_1910_%286971783833%29.jpg"
BED_URL="https://cdn.freesound.org/previews/382/382155_2276808-hq.mp3"
SHOT_URL="https://cdn.freesound.org/previews/385/385811_5937039-hq.mp3"
get ff.jpg "$FF_URL"; get princip.jpg "$GP_URL"; get trial.jpg "$TRIAL_URL"; get pistol.jpg "$GUN_URL"
get bed.mp3 "$BED_URL"; get shot.mp3 "$SHOT_URL"
cat > sources.jsonl <<JSON
{"file": "$G/src/ff.jpg", "source": "wikimedia", "url": "$FF_URL", "page": "https://commons.wikimedia.org/wiki/File:Ferdinand_Schmutzer_-_Franz_Ferdinand_von_%C3%96sterreich-Este,_um_1914.jpg", "title": "Franz Ferdinand von Österreich-Este, um 1914", "creator": "Ferdinand Schmutzer", "license": "Public domain"}
{"file": "$G/src/princip.jpg", "source": "wikimedia", "url": "$GP_URL", "page": "https://commons.wikimedia.org/wiki/File:Gavrilo_Princip,_cell,_headshot.jpg", "title": "Gavrilo Princip, cell, headshot", "creator": "Unknown photographer", "license": "Public domain"}
{"file": "$G/src/trial.jpg", "source": "wikimedia", "url": "$TRIAL_URL", "page": "https://commons.wikimedia.org/wiki/File:Sarajevo_trial,_accused,_%C4%8Cabrinovi%C4%87,_Princip,_Ili%C4%87.jpg", "title": "Sarajevo trial, accused, Čabrinović, Princip, Ilić", "creator": "Unknown photographer", "license": "Public domain"}
{"file": "$G/src/pistol.jpg", "source": "wikimedia", "url": "$GUN_URL", "page": "https://commons.wikimedia.org/wiki/File:Browning_1910_(6971783833).jpg", "title": "Browning 1910", "creator": "Askild Antonsen", "license": "CC BY 2.0"}
{"file": "$G/src/bed.mp3", "source": "freesound", "url": "$BED_URL", "page": "https://freesound.org/people/Kodack/sounds/382155", "title": "Tense/Suspenseful Cinematic", "creator": "Kodack", "license": "CC0"}
{"file": "$G/src/shot.mp3", "source": "freesound", "url": "$SHOT_URL", "page": "https://freesound.org/people/morganpurkis/sounds/385811", "title": "Single Pistol Gunshot 3", "creator": "morganpurkis", "license": "CC0"}
{"file": "assets/generated/map.svg", "source": "naturalearth", "url": "https://www.naturalearthdata.com/", "page": "https://github.com/nvkelso/natural-earth-vector", "title": "Natural Earth vector data (map)", "creator": "Natural Earth", "license": "Public domain"}
JSON

# People and objects: cut out, then printed as paper stickers (mono, halftone, scissor-cut border).
[ -s "$G/ff.png" ]      || { python3 "$S/cutout.py" "$G/src/ff.jpg" -o "$G/src/ff-cut.png" --trim; python3 "$S/paper-cutout.py" "$G/src/ff-cut.png" -o "$G/ff.png"; }
[ -s "$G/princip.png" ] || { python3 "$S/cutout.py" "$G/src/princip.jpg" -o "$G/src/princip-cut.png" --trim; python3 "$S/paper-cutout.py" "$G/src/princip-cut.png" -o "$G/princip.png"; }
[ -s "$G/pistol.png" ]  || { python3 "$S/cutout.py" "$G/src/pistol.jpg" -o "$G/src/pistol-cut.png" --trim; python3 "$S/paper-cutout.py" "$G/src/pistol-cut.png" -o "$G/pistol.png" --border 0.008 --max 1600; }
# A crowd does not cut out cleanly (neighbours come along), so the trial photo stays a whole print:
# cropped to the front row and given the same print treatment with a straight white border.
[ -s "$G/trial.png" ] || { ffmpeg -nostdin -v error -y -i "$G/src/trial.jpg" -vf "crop=1700:900:220:100" "$G/src/trial-crop.png"; python3 "$S/paper-cutout.py" "$G/src/trial-crop.png" -o "$G/trial.png" --border 0.018; }

# Paper: one neutral sheet multiplied over every scene, and a coloured sheet (the palette's sheet colour) for the title card.
[ -s "$G/paper.jpg" ] || python3 "$S/paper-texture.py" -o "$G/paper.jpg" --size 2112x1188 --folds 1,1 --mottle 0.09 --burn 0.3
[ -s "$G/sea.jpg" ]   || python3 "$S/paper-texture.py" -o "$G/sea.jpg" --size 2112x1188 --color "#5b6e9e" --folds 0,1 --stains 1 --grid 96 --seed 3

# The locator map: Bosnia and Herzegovina with its neighbours, pin on Sarajevo, country right of centre.
[ -s "$G/map.svg" ] || python3 "$S/map-svg.py" --focus "Bosnia and Herzegovina" --highlight "Sarajevo" \
  --admin1 "Bosnia and Herzegovina" --pin "Sarajevo=43.8563,18.4131" --anchor 0.64,0.5 --pad 0.32 -o "$G/map.svg" \
  --css ".land{fill:#e3e0d5;stroke:#a29c8c;stroke-width:1.3}.land.focus{fill:#f3f2ed;stroke:#0f1e55;stroke-width:2.6}.admin1{fill:none;stroke:#b6b1a2;stroke-width:1}.hl{fill:rgba(227, 162, 26, .3);stroke:#be8a1e;stroke-width:2.2}"
python3 "$S/footage-credits.py" >/dev/null
