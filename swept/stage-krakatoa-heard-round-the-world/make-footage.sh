#!/usr/bin/env bash
# Make this inspiration's media with no hand work (except the short sounds, below): British Pathé's 1930 film of Anak Krakatau erupting
# (YouTube), cut and conformed by scripts/fetch-shots.py (shots.json); the 1883 photograph, the 1888
# lithograph, two photographs of the Java coast after the waves, and Rogier Verbeek's portrait, all
# from Wikimedia Commons; the portrait cut out and printed as a paper sticker, one photograph printed
# whole; the dark locator map and the paper map of the Indian Ocean (scripts/map-svg.py); the paper
# sheet and the yellow card; a music bed, a blast, a rumble, and far-off guns from Freesound; then
# CREDITS.md. Everything lands in assets/generated/, footage/, and .footage/ (not committed).
# About 2 minutes after the AI environment exists. Run from the inspiration folder; SKILL_DIR = the skill root.
set -euo pipefail
SKILL_DIR=${SKILL_DIR:-../..}
S="$SKILL_DIR/scripts"
G=assets/generated
mkdir -p "$G/src" .footage/src
UA="direcfy-video-skill/1.0 (HyperFrames video production)"
get() { [ -s "$1" ] || curl -sfL -A "$UA" -o "$1" "$2"; }

# The picks, from asset-search.py video "Krakatoa volcano eruption newsreel"; image "Krakatau 1883",
# "Krakatau 1883 Tropenmuseum", "Verbeek Rogier"; audio "dark tension drone cinematic", "huge
# explosion distant", "volcano rumble". The stills come as Commons thumbnails: the thumbnail server
# keeps answering when the original-file server throttles (one original per ten minutes while this
# inspiration was built). The Tropenmuseum photographs are 700 px originals, served here at 960 px.
T="https://thumb.wikimedia.org/wikipedia/commons/thumb"
LITHO="Krakatoa_eruption_lithograph.jpg"
PHOTO="De_uitbarsting_van_de_Krakatau%2C_KITLV_5888.tiff"
PORTRAIT="Rogier_Diederick_Marius_Verbeek_-_H._Salzwedel_-_btv1b8453247g_%2801_of_14%29.jpg"
RUINS="COLLECTIE_TROPENMUSEUM_Ru%C3%AFne_van_het_Anjerhotel_na_de_uitbarsting_van_de_Krakatau_in_1883_Java._TMnr_60005528.jpg"
KAMPONG="COLLECTIE_TROPENMUSEUM_Puin_en_brokstukken_op_de_plek_waar_kampong_Pengadeau_was_voor_de_uitbarsting_van_de_Krakatau_in_1883._TMnr_60005543.jpg"
LITHO_URL="$T/4/49/$LITHO/1920px-$LITHO"
PHOTO_URL="$T/a/a3/$PHOTO/lossy-page1-1280px-$PHOTO.jpg"
PORTRAIT_URL="$T/e/e8/$PORTRAIT/1920px-$PORTRAIT"
RUINS_URL="$T/1/1f/$RUINS/960px-$RUINS"
KAMPONG_URL="$T/4/45/$KAMPONG/960px-$KAMPONG"
FILM=.footage/src/krakatoa-1930.mp4
FILM_URL="https://www.youtube.com/watch?v=CcWtycUtO18"
BED_URL="https://cdn.freesound.org/previews/614/614485_155898-hq.mp3"
BLAST_URL="https://cdn.freesound.org/previews/476/476225_9395330-hq.mp3"
RUMBLE_URL="https://cdn.freesound.org/previews/385/385098_5937039-hq.mp3"
GUNS_URL="https://cdn.freesound.org/previews/324/324277_1134415-hq.mp3"
get "$G/src/litho.jpg" "$LITHO_URL"; get "$G/src/eruption-1883.jpg" "$PHOTO_URL"; get "$G/src/verbeek.jpg" "$PORTRAIT_URL"
get "$G/src/ruins.jpg" "$RUINS_URL"; get "$G/src/kampong.jpg" "$KAMPONG_URL"
get "$G/src/bed.mp3" "$BED_URL"; get "$G/src/blast.mp3" "$BLAST_URL"; get "$G/src/rumble.mp3" "$RUMBLE_URL"; get "$G/src/guns.mp3" "$GUNS_URL"
C="https://commons.wikimedia.org/wiki/File"
cat > sources.jsonl <<JSON
{"file": "$FILM", "source": "youtube", "url": "$FILM_URL", "page": "$FILM_URL", "title": "Krakatoa! Aka Volcano (1930)", "creator": "British Pathé", "license": "Standard YouTube Licence"}
{"file": "$G/src/litho.jpg", "source": "wikimedia", "url": "$LITHO_URL", "page": "$C:Krakatoa_eruption_lithograph.jpg", "title": "Krakatoa eruption lithograph (Royal Society report, 1888)", "creator": "Parker & Coward, lithographers", "license": "Public domain"}
{"file": "$G/src/eruption-1883.jpg", "source": "wikimedia", "url": "$PHOTO_URL", "page": "$C:De_uitbarsting_van_de_Krakatau,_KITLV_5888.tiff", "title": "De uitbarsting van de Krakatau, KITLV 5888 (the eruption in May 1883)", "creator": "Unknown photographer; KITLV, Leiden University Libraries", "license": "Public domain"}
{"file": "$G/src/verbeek.jpg", "source": "wikimedia", "url": "$PORTRAIT_URL", "page": "$C:Rogier_Diederick_Marius_Verbeek_-_H._Salzwedel_-_btv1b8453247g_(01_of_14).jpg", "title": "Rogier Diederick Marius Verbeek, cabinet portrait", "creator": "Herman Salzwedel; Bibliothèque nationale de France", "license": "Public domain"}
{"file": "$G/src/ruins.jpg", "source": "wikimedia", "url": "$RUINS_URL", "page": "https://commons.wikimedia.org/w/index.php?curid=8589952", "title": "Ruïne van het Anjerhotel na de uitbarsting van de Krakatau in 1883, Java (TMnr 60005528)", "creator": "Woodbury & Page; Tropenmuseum", "license": "CC BY-SA 3.0"}
{"file": "$G/src/kampong.jpg", "source": "wikimedia", "url": "$KAMPONG_URL", "page": "https://commons.wikimedia.org/w/index.php?curid=8592940", "title": "Puin en brokstukken op de plek waar kampong Pengadeau was voor de uitbarsting van de Krakatau in 1883 (TMnr 60005543)", "creator": "Woodbury & Page; Tropenmuseum", "license": "CC BY-SA 3.0"}
{"file": "$G/src/bed.mp3", "source": "freesound", "url": "$BED_URL", "page": "https://freesound.org/people/secondbody/sounds/614485", "title": "23. Something's Not Right (dark cinematic drone)", "creator": "secondbody", "license": "CC0"}
{"file": "$G/src/blast.mp3", "source": "freesound", "url": "$BLAST_URL", "page": "https://freesound.org/people/Fission9/sounds/476225", "title": "Huge Distant Explosion", "creator": "Fission9", "license": "CC0"}
{"file": "$G/src/rumble.mp3", "source": "freesound", "url": "$RUMBLE_URL", "page": "https://freesound.org/people/morganpurkis/sounds/385098", "title": "Volcano", "creator": "morganpurkis", "license": "CC0"}
{"file": "$G/src/guns.mp3", "source": "freesound", "url": "$GUNS_URL", "page": "https://freesound.org/people/Kostrava/sounds/324277", "title": "distant explosion", "creator": "Kostrava", "license": "CC0"}
{"file": "$G/map.svg", "source": "naturalearth", "url": "https://www.naturalearthdata.com/", "page": "https://github.com/nvkelso/natural-earth-vector", "title": "Natural Earth vector data (map)", "creator": "Natural Earth", "license": "Public domain"}
{"file": "$G/ocean.svg", "source": "naturalearth", "url": "https://www.naturalearthdata.com/", "page": "https://github.com/nvkelso/natural-earth-vector", "title": "Natural Earth vector data (map)", "creator": "Natural Earth", "license": "Public domain"}
JSON

# The film: fetched whole, once (YouTube answered 403 to the sixth of eight segment requests while
# this inspiration was built, and again to the whole file a moment later, so the one download is retried),
# then eight shots are cut and conformed to 1920x1080 by the footage pipeline (shots.json). The source
# is 640x480 with the archive's logo in the top right corner, so every shot takes the lower three
# quarters of the picture ("zoom": 1.04, "cy": 1.0), which leaves the logo wholly outside.
for try in 1 2 3 4; do
  [ -s "$FILM" ] && break
  yt-dlp -q --no-warnings -f "bv*/b" -S res:1080 --remux-video mp4 -o "$FILM" "$FILM_URL" </dev/null || { rm -f "$FILM"; sleep $((try * 20)); }
done
[ -s "$FILM" ] || { echo "could not download $FILM_URL (YouTube refused four times); run this script again in a few minutes" >&2; exit 1; }
python3 "$S/fetch-shots.py" shots.json >/dev/null

# The lithograph is a tall 2.5 MB print: the film shows its lower half, so that part is cut out.
[ -s "$G/src/litho-crop.jpg" ] || ffmpeg -nostdin -v error -y -i "$G/src/litho.jpg" -vf "crop=1920:1180:0:1220" -q:v 3 "$G/src/litho-crop.jpg"
python3 "$S/made.py" "$G/src/litho-crop.jpg" "$G/src/litho.jpg" >/dev/null

# The person: cut out, then printed as a paper sticker (mono, halftone, scissor-cut border). The
# portrait is an oval vignette, and without a box the whole oval is taken as the object: the box
# sits inside the oval, on the man.
[ -s "$G/verbeek.png" ] || { python3 "$S/cutout.py" "$G/src/verbeek.jpg" -o "$G/src/verbeek-cut.png" --box 520,560,1500,2150 --trim
  python3 "$S/paper-cutout.py" "$G/src/verbeek-cut.png" -o "$G/verbeek.png" --max 1600; }
# A landscape does not cut out: the photograph is trimmed inside its rounded mount and printed whole.
[ -s "$G/kampong.png" ] || { ffmpeg -nostdin -v error -y -i "$G/src/kampong.jpg" -vf "crop=900:690:30:26" "$G/src/kampong-crop.png"
  python3 "$S/paper-cutout.py" "$G/src/kampong-crop.png" -o "$G/kampong.png" --border 0.018; }
# The records that let the credits follow each print back to its photograph (the tools write them
# only when they run; these lines keep them when the files are already there).
python3 "$S/made.py" "$G/src/verbeek-cut.png" "$G/src/verbeek.jpg" >/dev/null
python3 "$S/made.py" "$G/verbeek.png" "$G/src/verbeek-cut.png" >/dev/null
python3 "$S/made.py" "$G/src/kampong-crop.png" "$G/src/kampong.jpg" >/dev/null
python3 "$S/made.py" "$G/kampong.png" "$G/src/kampong-crop.png" >/dev/null

# Paper: one neutral sheet, multiplied over the collage block from the host, and the sulphur-yellow card.
[ -s "$G/paper.jpg" ] || python3 "$S/paper-texture.py" -o "$G/paper.jpg" --size 2112x1188 --folds 1,1 --mottle 0.09 --burn 0.3
[ -s "$G/card.jpg" ]  || python3 "$S/paper-texture.py" -o "$G/card.jpg" --size 1000x520 --color "#f2c230" --folds 0,0 --stains 1 --mottle 0.07 --burn 0.12 --seed 5

# The dark locator map: Indonesia, the two provinces on the Sunda Strait (Lampung, Banten) filled, a
# pin on Krakatau. The camera flies in 3x, so the strokes are drawn thin and the coast comes from the
# detailed (10m) set. Then the paper map of the collage block: the Indian Ocean from Africa to
# Australia, with pins that give the diagram its places in frame pixels (ocean.json).
[ -s "$G/map.svg" ] || python3 "$S/map-svg.py" --focus Indonesia --admin1 Indonesia --highlight Banten --highlight Lampung \
  --pin "Krakatau=-6.102,105.423" --anchor 0.5,0.5 --pad 0.06 --detail 10m -o "$G/map.svg" \
  --css ".land{fill:#0d1a22;stroke:#1c3a48;stroke-width:.6}.land.focus{fill:#1f6f8b;stroke:#8fe0f5;stroke-width:1.1}.admin1{fill:none;stroke:#9fdcec;stroke-width:.45;stroke-opacity:.5}.hl{fill:rgba(74,190,128,.6);stroke:#7ff0b0;stroke-width:.9}" >/dev/null
[ -s "$G/ocean.svg" ] || python3 "$S/map-svg.py" --focus Indonesia --extent 32.8,-43.6,133.8,9.65 --pad 0 \
  --pin "Krakatau=-6.102,105.423" --pin "Batavia=-6.175,106.827" --pin "Perth=-31.95,115.86" --pin "Rodrigues=-19.72,63.43" -o "$G/ocean.svg" \
  --css ".land{fill:#f3ecd8;stroke:#3a342a;stroke-width:1.6}.land.focus{fill:#f3ecd8;stroke:#3a342a;stroke-width:1.6}" >/dev/null
# The short sounds the compositions name (Pixabay effects when the film was made; the skill bundles
# no sounds): put your own in assets/sfx/ (scripts/asset-search.py audio finds them). A missing one is
# named here and its slot stays silent.
for f in whoosh whoosh-short pop typing impact-bass-1; do [ -s "assets/sfx/$f.mp3" ] || echo "note: no assets/sfx/$f.mp3, so its hits stay silent" >&2; done
python3 "$S/footage-credits.py" >/dev/null
