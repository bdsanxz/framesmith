#!/usr/bin/env bash
# Makes this project's sourced pictures again from nothing: the photos in assets/pic/ (each
# original downloaded by URL, then cropped), the b-roll footage/b01.mp4 to b15.mp4 (cut by
# scripts/fetch-shots.py from shots.json), and the Tabler icons in assets/icons/.
# A step whose output file exists is skipped, so a stopped run can be started again.
set -euo pipefail
cd "$(dirname "$0")"

for tool in curl ffmpeg python3 yt-dlp; do
  command -v "$tool" >/dev/null || { echo "$tool not found" >&2; exit 1; }
done

UA="direcfy-video-skill/1.0 (HyperFrames video production)"
COMMONS="https://upload.wikimedia.org/wikipedia/commons"

# get <file> <path under $COMMONS> <Commons file title> <creator> <licence>
# Downloads one original and records it in sources.jsonl, as `asset-search.py --get` does.
# Commons answers 429 after a burst: wait ten minutes and run the script again.
get() {
  [ -s "$1" ] || {
    mkdir -p "$(dirname "$1")"
    curl -fL --retry 2 --retry-delay 30 -sS -A "$UA" -o "$1.part" "$COMMONS/$2"
    mv "$1.part" "$1"
    python3 - "$1" "$COMMONS/$2" "$3" "$4" "$5" <<'PY'
import json, os, sys
f, url, title, creator, lic = sys.argv[1:6]
p = "sources.jsonl"
old = open(p, encoding="utf-8").read() if os.path.exists(p) else ""
if not any(l.strip() and json.loads(l).get("file") == f and json.loads(l).get("url") for l in old.split("\n")):
    row = {"file": f, "source": "wikimedia", "url": url,
           "page": "https://commons.wikimedia.org/wiki/File:" + title.replace(" ", "_"),
           "title": title, "creator": creator, "license": lic}
    with open(p, "a", encoding="utf-8") as o:
        o.write(("" if not old or old.endswith("\n") else "\n") + json.dumps(row, ensure_ascii=False) + "\n")
PY
    sleep 4
  }
}

# pic <slug> <original in assets/pic/src, without .jpg> <crop w:h:x:y> <size w:h>
pic() {
  [ -s "assets/pic/$1.jpg" ] || {
    ffmpeg -nostdin -v error -y -i "assets/pic/src/$2.jpg" -vf "crop=$3,scale=$4:flags=lanczos" -q:v 2 "assets/pic/$1.jpg"
    python3 ../../scripts/made.py "assets/pic/$1.jpg" "assets/pic/src/$2.jpg"
  }
}

# ---------- 1. Photos: originals ----------
# Files wider or taller than 4096 px are taken as Commons' 3840 px copy (the /thumb/ paths); the
# crops below are measured on these exact files.
S=assets/pic/src
get $S/prabowo.jpg "thumb/0/0c/Prabowo_Subianto_2024_official_portrait_%283x4_cropped%29.jpg/3840px-Prabowo_Subianto_2024_official_portrait_%283x4_cropped%29.jpg" \
  "Prabowo Subianto 2024 official portrait (3x4 cropped).jpg" "Ministry of State Secretariat" "Public domain"
get $S/soekarno.jpg "e/e4/Sukarno_giving_a_speech%2C_Bung_Karno_Penjambung_Lidah_Rakjat_242.jpg" \
  "Sukarno giving a speech, Bung Karno Penjambung Lidah Rakjat 242.jpg" "Uncredited" "Public domain"
get $S/soekarno-portrait.jpg "thumb/0/01/Presiden_Sukarno.jpg/3840px-Presiden_Sukarno.jpg" \
  "Presiden Sukarno.jpg" "Onbekend/Anonymous" "Public domain"
get $S/soeharto.jpg "thumb/2/27/Official_portrait_of_Soeharto_%281980s%29.jpg/3840px-Official_portrait_of_Soeharto_%281980s%29.jpg" \
  "Official portrait of Soeharto (1980s).jpg" "Government of Indonesia (uncredited)" "Public domain"
get $S/sby.jpg "4/42/Susilo_Bambang_Yudhoyono%2C_official_presidential_portrait_%282004%29.jpg" \
  "Susilo Bambang Yudhoyono, official presidential portrait (2004).jpg" "Govt. of Indonesia" "Public domain"
get $S/trump.jpg "1/19/January_2025_Official_Presidential_Portrait_of_Donald_J._Trump.jpg" \
  "January 2025 Official Presidential Portrait of Donald J. Trump.jpg" "Daniel Torok" "Public domain"
get $S/dpr-building.jpg "9/9a/MPRDPRBuilding.jpg" \
  "MPRDPRBuilding.jpg" "Davidelit" "Public domain"
get $S/plenary-hall.jpg "1/12/Ruang_MPR.jpg" \
  "Ruang MPR.jpg" "U.S. Department of State's Flickr account" "Public domain"
get $S/plenary-session.jpg "thumb/1/1a/Sidang_Paripurna_DPR_ke-9_2015.jpg/3840px-Sidang_Paripurna_DPR_ke-9_2015.jpg" \
  "Sidang Paripurna DPR ke-9 2015.jpg" "Andylala Waluyo (VOA)" "Public domain"
get $S/rupiah.jpg "d/dc/Uang_rupiah_kertas.jpg" \
  "Uang rupiah kertas.jpg" "Pak.syafda" "CC BY-SA 4.0"
get $S/gallery-frames.jpg "thumb/f/f5/Gallery_of_Polish_Painting_and_European_Decorative_Arts%2C_National_Museum_in_Kielce%2C_czerwiec_2025%2C_SOULinPIX_092.jpg/3840px-Gallery_of_Polish_Painting_and_European_Decorative_Arts%2C_National_Museum_in_Kielce%2C_czerwiec_2025%2C_SOULinPIX_092.jpg" \
  "Gallery of Polish Painting and European Decorative Arts, National Museum in Kielce, czerwiec 2025, SOULinPIX 092.jpg" "Krzysztof Popławski" "CC BY 4.0"
get $S/gallery-visitors.jpg "8/84/Rijksmuseum_Amsterdam_-_Gallery_of_Honour_Left_1.3.jpg" \
  "Rijksmuseum Amsterdam - Gallery of Honour Left 1.3.jpg" "Oursana" "CC0"
get $S/teleprompter-studio.jpg "8/80/Studio_Teleprompter.jpg" \
  "Studio Teleprompter.jpg" "Enchantmentvideo" "CC BY-SA 4.0"
get $S/teleprompter-glass.jpg "thumb/9/91/Teleprompter_presidenziale.jpg/3840px-Teleprompter_presidenziale.jpg" \
  "Teleprompter presidenziale.jpg" "Tlpitalia" "CC BY 3.0"
get $S/batavia-castle.jpg "thumb/e/ef/Andries_Beeckman_-_The_Castle_of_Batavia.jpg/3840px-Andries_Beeckman_-_The_Castle_of_Batavia.jpg" \
  "Andries Beeckman - The Castle of Batavia.jpg" "Andries Beeckman" "Public domain"
get $S/voc-ships-batavia.jpg "thumb/3/3b/Gezicht_op_Batavia_Rijksmuseum_SK-A-2513.jpeg/3840px-Gezicht_op_Batavia_Rijksmuseum_SK-A-2513.jpeg" \
  "Gezicht op Batavia Rijksmuseum SK-A-2513.jpeg" "Hendrick Dubbels" "Public domain"
get $S/deli-tobacco-harvest.jpg "thumb/d/d9/KITLV_-_88269_-_Kleingrothe%2C_C.J._-_Medan_-_Harvesting_tobacco_on_the_Medan_company_of_the_Deli_Company_in_Deli_-_1905.tif/lossy-page1-3840px-KITLV_-_88269_-_Kleingrothe%2C_C.J._-_Medan_-_Harvesting_tobacco_on_the_Medan_company_of_the_Deli_Company_in_Deli_-_1905.tif.jpg" \
  "KITLV - 88269 - Kleingrothe, C.J. - Medan - Harvesting tobacco on the Medan company of the Deli Company in Deli - 1905.tif" "Kleingrothe, C.J. (Kleingrothe, Carl Josef, 1864-1925) / Medan" "Public domain"

# ---------- 2. Photos: crops (portraits 800x1000, the rest 1920x1080) ----------
pic prabowo           prabowo              3840:4800:0:0      800:1000
pic soekarno          soekarno             1200:1500:360:0    800:1000
pic soekarno-portrait soekarno-portrait    3840:4800:0:0      800:1000
pic soeharto          soeharto             3840:4800:0:0      800:1000
pic sby               sby                  1764:2206:22:0     800:1000
pic trump             trump                2550:3186:0:0      800:1000
pic dpr-building      dpr-building         2288:1287:0:130    1920:1080
pic plenary-hall      plenary-hall         3131:1761:0:100    1920:1080
pic plenary-session   plenary-session      3840:2160:0:0      1920:1080
pic rupiah            rupiah               2624:1476:0:0      1920:1080
pic gallery-frames    gallery-frames       3840:2160:0:200    1920:1080
pic gallery-visitors  gallery-visitors     3266:1837:544:1129 1920:1080
pic gilded-frame      gallery-visitors     1468:826:528:1240  1920:1080
pic teleprompter      teleprompter-studio  4032:2268:0:450    1920:1080
pic teleprompter-glass teleprompter-glass  3840:2160:0:0      1920:1080
pic batavia-castle    batavia-castle       3840:2160:0:450    1920:1080
pic voc-ships-batavia voc-ships-batavia    3840:2160:0:780    1920:1080
pic deli-plantation   deli-tobacco-harvest 3840:2160:0:110    1920:1080

# ---------- 3. B-roll ----------
# The manifest, written only when it is missing. The Mixkit rows carry their credit here because
# a fresh checkout has no .assets/search/ for the ledger to read it from.
[ -s shots.json ] || cat > shots.json <<'JSON'
{"width": 1920, "height": 1080, "fps": 30, "out": "footage", "handle": 0.2,
 "shots": [
  {"id": "b01", "start": 0.0, "end": 6.0, "source": "8PFLM3s_95o", "in_point": 2.0, "subject": "speaker standing at a chamber microphone reading from notes, public gallery behind"},
  {"id": "b02", "start": 6.0, "end": 12.0, "source": "InvGGLiA2P4", "in_point": 225.0, "subject": "teleprompter glass on a tripod showing its script, hand with remote"},
  {"id": "b03", "start": 12.0, "end": 18.0, "source": "h0ec0O7z8is", "in_point": 1.0, "subject": "audience in a large theatre hall applauding, seen from behind"},
  {"id": "b04", "start": 18.0, "end": 24.0, "source": "8PFLM3s_95o", "in_point": 31.0, "subject": "legislative chamber, wide view of members at their desks"},
  {"id": "b05", "start": 24.0, "end": 30.0, "source": "https://assets.mixkit.co/videos/18296/18296-1080.mp4", "in_point": 1.0, "subject": "hands counting a wad of US 100-dollar bills, close",
   "credit": "arvizum, Mixkit: https://mixkit.co/free-stock-video/man-counting-a-wad-of-bills-seen-very-closely-18296/", "license": "Mixkit Free License (stock video)"},
  {"id": "b06", "start": 30.0, "end": 36.0, "source": "UcUGICKidOQ", "in_point": 2.0, "subject": "stock exchange price board, rows of changing numbers"},
  {"id": "b07", "start": 36.0, "end": 42.0, "source": "zdX7mWoSQR0", "in_point": 2.0, "subject": "busy traditional market in Indonesia, walking through the stalls"},
  {"id": "b08", "start": 42.0, "end": 48.0, "source": "https://assets.mixkit.co/videos/4011/4011-1080.mp4", "in_point": 3.0, "subject": "container ship stacked with containers under gantry cranes at a port",
   "credit": "dubassy, Mixkit: https://mixkit.co/free-stock-video/cargo-ship-full-of-containers-4011/", "license": "Mixkit Free License (stock video)"},
  {"id": "b09", "start": 48.0, "end": 54.0, "source": "footage/raw/rijksmuseum-1st-floor.webm", "in_point": 29.0, "subject": "museum gallery wall of paintings in gilded frames"},
  {"id": "b10", "start": 54.0, "end": 60.0, "source": "https://openbeelden.nl/files/14/80/1480904.1480138.DE_THEE_VAN_-FHD00Z03Q3K.mp4", "in_point": 327.0, "subject": "archive 1920s: tea pickers at work on a plantation in Java",
   "credit": "Willy Mullens / Haghe Film (1920s), Nederlands Instituut voor Beeld en Geluid via Open Beelden: https://openbeelden.nl/media/1480134", "license": "Public Domain Mark 1.0", "cy": 0.8},
  {"id": "b11", "start": 60.0, "end": 66.0, "source": "footage/raw/rijksmuseum-1st-floor.webm", "in_point": 40.5, "subject": "visitors seated in front of a large painting in a museum gallery"},
  {"id": "b12", "start": 66.0, "end": 72.0, "source": "https://openbeelden.nl/files/14/80/1480904.1480138.DE_THEE_VAN_-FHD00Z03Q3K.mp4", "in_point": 269.3, "subject": "archive 1920s: European planter in white suit and pith helmet walking through the tea fields",
   "credit": "Willy Mullens / Haghe Film (1920s), Nederlands Instituut voor Beeld en Geluid via Open Beelden: https://openbeelden.nl/media/1480134", "license": "Public Domain Mark 1.0"},
  {"id": "b13", "start": 72.0, "end": 78.0, "source": "JuqqUtoRqlo", "in_point": 20.0, "subject": "fishing boats with Indonesian flags beached at a harbour"},
  {"id": "b14", "start": 78.0, "end": 84.0, "source": "ggXWCnvgWVU", "in_point": 5.0, "subject": "Jakarta from the air at dusk"},
  {"id": "b15", "start": 84.0, "end": 90.0, "source": "https://openbeelden.nl/files/14/80/1480904.1480138.DE_THEE_VAN_-FHD00Z03Q3K.mp4", "in_point": 505.0, "subject": "archive 1920s: line of workers carrying baskets of tea into the plantation factory",
   "credit": "Willy Mullens / Haghe Film (1920s), Nederlands Instituut voor Beeld en Geluid via Open Beelden: https://openbeelden.nl/media/1480134", "license": "Public Domain Mark 1.0", "cy": 1.0}
 ]}
JSON

# The whole source of b09 and b11 (4K, 38 MB): fetch-shots.py cuts it from this local copy.
get footage/raw/rijksmuseum-1st-floor.webm "b/be/RIJKSMUSEUM_Amsterdam_1st_floor_104112.webm" \
  "RIJKSMUSEUM Amsterdam 1st floor 104112.webm" "Quahadi" "CC BY-SA 4.0"

# Channel and licence of each YouTube source, for the ledger.
for id in 8PFLM3s_95o InvGGLiA2P4 h0ec0O7z8is UcUGICKidOQ zdX7mWoSQR0 JuqqUtoRqlo ggXWCnvgWVU; do
  [ -s ".footage/meta/$id.json" ] || ../../scripts/footage-meta.sh "$id"
done

# Only the clips that are missing are named, so an existing clip is never cut again.
missing=()
for n in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15; do
  [ -s "footage/b$n.mp4" ] || missing+=("b$n")
done
[ "${#missing[@]}" -eq 0 ] || python3 ../../scripts/fetch-shots.py "${missing[@]}"

# ---------- 4. Icons (Tabler outline, MIT), stroke set for use as masks ----------
mkdir -p assets/icons
for name in chart-line trending-up percentage coin cash currency-dollar building-bank briefcase users user \
            microphone speakerphone presentation frame photo world ship flag eye clock calendar book \
            question-mark check x arrow-right shield heart-handshake; do
  [ -s "assets/icons/$name.svg" ] || {
    curl -fsSL -o "assets/icons/$name.svg.part" "https://raw.githubusercontent.com/tabler/tabler-icons/main/icons/outline/$name.svg"
    sed -e 's/stroke="currentColor"/stroke="#000000"/' -e 's/stroke-width="2"/stroke-width="1.5"/' \
      "assets/icons/$name.svg.part" > "assets/icons/$name.svg"
    rm "assets/icons/$name.svg.part"
  }
done
grep -qs '"source": "Tabler Icons"' credits-extra.jsonl || \
  echo '{"source": "Tabler Icons", "url": "https://github.com/tabler/tabler-icons", "page": "https://tabler.io/icons", "title": "Tabler Icons (outline)", "creator": "Paweł Kuna and contributors", "license": "MIT"}' >> credits-extra.jsonl

echo "pictures are in place: $(ls assets/pic/*.jpg | wc -l | tr -d ' ') photos, $(ls footage/b??.mp4 | wc -l | tr -d ' ') b-roll clips, $(ls assets/icons/*.svg | wc -l | tr -d ' ') icons"
