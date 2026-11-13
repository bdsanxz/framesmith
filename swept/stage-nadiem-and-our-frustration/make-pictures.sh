#!/usr/bin/env bash
# make-pictures.sh: makes this project's sourced pictures again from nothing.
#   assets/pic/orig/* (Wikimedia Commons originals) and the crops assets/pic/<slug>.jpg made from them;
#   footage/b01.mp4 ... b15.mp4 (b-roll cut by fetch-shots.py from shots.json; footage/raw/ holds b03's source);
#   assets/icons/*.svg (Tabler outline icons, stroke #000000, width 1.5).
# Every step is skipped when its output exists. Needs curl, ffmpeg, python3. Sources and limits: evidence/pictures.md
set -euo pipefail
cd "$(dirname "$0")"

UA='direcfy-make-pictures/1.0 (video project; asset download)'
C='https://upload.wikimedia.org/wikipedia/commons'
Q='?utm_source=commons.wikimedia.org&utm_campaign=imageinfo&utm_content=original'
W='https://commons.wikimedia.org/wiki/File:'

# get <file> <url>: download once; a failed download leaves no file behind. The pause keeps Commons from throttling.
get() {
  [ -s "$1" ] || { mkdir -p "$(dirname "$1")"; curl -fsSL --retry 3 --retry-delay 15 -A "$UA" -o "$1.part" "$2"; mv "$1.part" "$1"; sleep 2; }
}

# src <file> <origin> <url> <page> <title> <creator> <licence>: the download's row in sources.jsonl, written once per file
src() {
  python3 - "$@" <<'PY'
import json, os, sys
f, origin, url, page, title, creator, lic = sys.argv[1:8]
rows = [json.loads(l) for l in open("sources.jsonl") if l.strip()] if os.path.exists("sources.jsonl") else []
if not any(r.get("file") == f and r.get("url") for r in rows):
    row = {"file": f, "source": origin, "url": url, "page": page, "title": title, "creator": creator, "license": lic}
    open("sources.jsonl", "a").write(json.dumps(row, ensure_ascii=False) + "\n")
PY
}

# orig <slug> <origin> <url> <page> <title> <creator> <licence>: one original into assets/pic/orig/, and its ledger row
orig() {
  get "assets/pic/orig/$1.jpg" "$3"
  src "assets/pic/orig/$1.jpg" "$2" "$3" "$4" "$5" "$6" "$7"
}

# pic <slug> <ffmpeg filter>: assets/pic/<slug>.jpg from its original, and the made.py record (written once)
pic() {
  local in="assets/pic/orig/$1.jpg" out="assets/pic/$1.jpg"
  [ -s "$out" ] || ffmpeg -nostdin -loglevel error -y -i "$in" -vf "$2" -q:v 2 "$out" || { rm -f "$out"; exit 1; }
  python3 ../../scripts/made.py "$out" "$in" >/dev/null
}

# ---------- 1. Photos: originals ----------
orig nadiem-makarim  wikimedia "$C/2/25/Nadiem_Makarim.jpg$Q" "${W}Nadiem_Makarim.jpg" \
     "Nadiem Makarim.jpg" "World Economic Forum; Photo by Sikarin Thanachaiary" "CC BY-SA 2.0"
orig mahfud-md       wikimedia "$C/1/1b/Mahfud_MD_%28cropped%29.jpg$Q" "${W}Mahfud_MD_(cropped).jpg" \
     "Mahfud MD (cropped).jpg" "Coordinating Ministry of Political, Legal, and Security Affairs (Indonesia)" "Public domain"
orig rhenald-kasali  wikimedia "$C/6/6a/Rhenald_Kasali_2025.jpg$Q" "${W}Rhenald_Kasali_2025.jpg" \
     "Rhenald Kasali 2025.jpg" "Juxlos" "CC BY-SA 4.0"
orig hotman-paris    wikimedia "$C/6/6b/Hotman_Paris.jpg$Q" "${W}Hotman_Paris.jpg" \
     "Hotman Paris.jpg" "FHF Law School" "CC BY 3.0"
orig ferry-irwandi   wikimedia "$C/6/6b/Potret_Ferry_Irwandi.jpg$Q" "${W}Potret_Ferry_Irwandi.jpg" \
     "Potret Ferry Irwandi.jpg" "2ndhart" "CC0"
orig gary-neville    wikimedia "$C/8/8e/Gary_Neville_2014.jpg$Q" "${W}Gary_Neville_2014.jpg" \
     "Gary Neville 2014.jpg" "University of Salford Press Office" "CC BY 2.0"
orig micah-richards  wikimedia "$C/d/d8/Richards_crop.JPG$Q" "${W}Richards_crop.JPG" \
     "Richards crop.JPG" "Oldelpaso" "CC BY-SA 3.0"
orig thierry-henry   openverse/wikimedia "$C/9/92/Thierry_Henry_Arsenal_U19s_Vs_Olympiacos_%28cropped%29.jpg" \
     "https://commons.wikimedia.org/w/index.php?curid=44077957" \
     "Thierry Henry Arsenal U19s Vs Olympiacos (cropped)" "joshjdss" "CC BY 2.0"
orig jamie-carragher wikimedia "$C/c/c8/Football_against_poverty_2014_-_Jamie_Carragher_%28cropped%29.jpg$Q" \
     "${W}Football_against_poverty_2014_-_Jamie_Carragher_(cropped).jpg" \
     "Football against poverty 2014 - Jamie Carragher (cropped).jpg" "Ludovic Péron" "CC BY-SA 3.0"
orig chromebook      wikimedia "$C/7/7e/Chromebook_serie_3.jpg$Q" "${W}Chromebook_serie_3.jpg" \
     "Chromebook serie 3.jpg" "Nahusm" "CC BY-SA 3.0"
orig chromebook-lid  wikimedia "$C/8/89/Acer_Chromebook_15_CB3-532%2C_Winschoten_%282021%29_03.jpg$Q" \
     "${W}Acer_Chromebook_15_CB3-532,_Winschoten_(2021)_03.jpg" \
     "Acer Chromebook 15 CB3-532, Winschoten (2021) 03.jpg" "Donald Trung Quoc Don" "CC BY-SA 4.0"
orig kejaksaan-agung wikimedia "$C/4/44/Gedung_Kejaksaan_Agung_%282025%29.jpg$Q" "${W}Gedung_Kejaksaan_Agung_(2025).jpg" \
     "Gedung Kejaksaan Agung (2025).jpg" "Medelam" "CC BY-SA 4.0"

# ---------- 1. Photos: crops (portraits 800x1000, face in the upper middle; objects kept whole) ----------
P='scale=800:1000:flags=lanczos'
pic nadiem-makarim  "crop=1500:1875:47:256,$P"
pic mahfud-md       "crop=1920:2400:0:0,$P"
pic rhenald-kasali  "crop=564:705:43:0,$P"
pic hotman-paris    "crop=480:600:0:0,$P"
pic ferry-irwandi   "crop=1080:1350:0:0,$P"
pic gary-neville    "crop=1280:1600:1091:0,$P"
pic micah-richards  "crop=546:682:21:0,$P"
pic thierry-henry   "crop=1400:1750:181:0,$P"
pic jamie-carragher "crop=1040:1300:0:0,$P"
pic chromebook      "null"
pic chromebook-lid  "scale=1920:-2:flags=lanczos"
pic kejaksaan-agung "null"

# ---------- 2. B-roll ----------
# b03 is cut from a whole Commons file (Commons throttles the range requests a remote cut would make).
V='Samsunspor-Manisa_ma%C3%A7%C4%B1_%2805.09.22%29_VAR_penalt%C4%B1.webm'
get footage/raw/commons-var-penalty.webm "$C/6/6b/$V$Q"
src footage/raw/commons-var-penalty.webm wikimedia "$C/6/6b/$V$Q" \
    "${W}Samsunspor-Manisa_ma%C3%A7%C4%B1_(05.09.22)_VAR_penalt%C4%B1.webm" \
    "Samsunspor-Manisa maçı (05.09.22) VAR penaltı.webm" "𐰇𐱅𐰚𐰤" "CC BY-SA 4.0"

# The cut list. Written only when shots.json is missing: the file in the project is the source of truth.
[ -s shots.json ] || cat > shots.json <<'JSON'
{"width": 1920, "height": 1080, "fps": 30, "out": "footage", "handle": 0.2,
 "shots": [
  {"id": "b01", "start": 0.0, "end": 6.0, "source": "https://videos.pexels.com/video-files/3558720/3558720-hd_1920_1080_30fps.mp4", "in_point": 0.5, "subject": "football referee talking to the two captains before kick-off", "credit": "Nazim Zafri", "license": "Pexels License"},
  {"id": "b02", "start": 6.0, "end": 12.0, "source": "https://videos.pexels.com/video-files/3195653/3195653-hd_1920_1080_25fps.mp4", "in_point": 8.5, "subject": "face-painted football fans shouting in the stands", "credit": "Pressmaster", "license": "Pexels License"},
  {"id": "b03", "start": 12.0, "end": 18.0, "source": "footage/raw/commons-var-penalty.webm", "in_point": 10.0, "zoom": 1.7, "cx": 0.31, "cy": 0.71, "subject": "players crowd the referees at the pitch-side VAR monitor (stand view)"},
  {"id": "b04", "start": 18.0, "end": 24.0, "source": "https://videos.pexels.com/video-files/6104279/6104279-hd_1920_1080_25fps.mp4", "in_point": 3.0, "subject": "two fans on a sofa reacting in frustration to a match on TV", "credit": "Kampus Production", "license": "Pexels License"},
  {"id": "b05", "start": 24.0, "end": 30.0, "source": "https://videos.pexels.com/video-files/3558720/3558720-hd_1920_1080_30fps.mp4", "in_point": 11.0, "subject": "captain holds his head while the referee explains", "credit": "Nazim Zafri", "license": "Pexels License"},
  {"id": "b06", "start": 30.0, "end": 36.0, "source": "https://videos.pexels.com/video-files/35053395/14849030_1920_1080_30fps.mp4", "in_point": 0.5, "zoom": 1.07, "subject": "floodlit football stadium, wide, at night", "credit": "Necip Duman", "license": "Pexels License"},
  {"id": "b07", "start": 36.0, "end": 42.0, "source": "https://videos.pexels.com/video-files/6767271/6767271-uhd_4096_2160_25fps.mp4", "in_point": 0.0, "subject": "referee with a whistle signalling (indoor court, stand-in for a football referee)", "credit": "cottonbro studio", "license": "Pexels License"},
  {"id": "b08", "start": 42.0, "end": 48.0, "source": "https://assets.mixkit.co/videos/4911/4911-1080.mp4", "in_point": 0.5, "subject": "woman scrolling a photo feed on her phone, over the shoulder"},
  {"id": "b09", "start": 48.0, "end": 54.0, "source": "https://videos.pexels.com/video-files/946146/946146-hd_1920_1080_30fps.mp4", "in_point": 4.0, "subject": "hands typing on a laptop keyboard, close", "credit": "Carlos Arribas", "license": "Pexels License"},
  {"id": "b10", "start": 54.0, "end": 60.0, "source": "https://assets.mixkit.co/videos/41272/41272-1080.mp4", "in_point": 7.0, "subject": "creator talking to his camera in a studio"},
  {"id": "b11", "start": 60.0, "end": 66.0, "source": "https://videos.pexels.com/video-files/6699964/6699964-hd_1920_1080_24fps.mp4", "in_point": 3.0, "subject": "hand strikes a judge's gavel", "credit": "pickarick", "license": "Pexels License"},
  {"id": "b12", "start": 66.0, "end": 72.0, "source": "https://videos.pexels.com/video-files/34550101/14640400_2160_3840_50fps.mp4", "in_point": 15.0, "cy": 0.53, "subject": "courtroom interior with people seated (staged)", "credit": "khezez", "license": "Pexels License"},
  {"id": "b13", "start": 72.0, "end": 78.0, "source": "https://videos.pexels.com/video-files/5636967/5636967-hd_1920_1080_24fps.mp4", "in_point": 1.0, "subject": "scales of justice on a desk", "credit": "Sora Shimazaki", "license": "Pexels License"},
  {"id": "b14", "start": 78.0, "end": 84.0, "source": "https://videos.pexels.com/video-files/5243246/5243246-hd_1920_1080_24fps.mp4", "in_point": 7.4, "subject": "prison cell bars, slow pan", "credit": "Teela Hudak", "license": "Pexels License"},
  {"id": "b15", "start": 84.0, "end": 90.0, "source": "https://videos.pexels.com/video-files/39681507/16921228_1920_1080_25fps.mp4", "in_point": 2.0, "subject": "commuters leaving Dukuh Atas station, Jakarta, at dusk", "credit": "Daneswara Eka", "license": "Pexels License"}
 ]}
JSON

# Where each stock clip came from, in the form footage-credits.py matches shot URLs against (page, author, licence).
PICKS=.assets/search/video-pexels-picks/candidates.json
[ -s "$PICKS" ] || { mkdir -p "$(dirname "$PICKS")"; python3 - "$PICKS" <<'PY'
import json, sys
PX, MK = "Pexels License (https://www.pexels.com/license/)", "Mixkit Free License (stock video)"
V, P, M = "https://videos.pexels.com/video-files/", "https://www.pexels.com/video/", "https://mixkit.co/free-stock-video/"
rows = [  # origin, title, page, author, licence, file
 ("pexels", "A Referee Tossing A Coin To Determine First Ball Possession Between Competing Teams In A Soccer Match", P + "a-referee-tossing-a-coin-to-determine-first-ball-possession-between-competing-teams-in-a-soccer-match-3558720/", "Nazim Zafri", PX, V + "3558720/3558720-hd_1920_1080_30fps.mp4"),
 ("pexels", "Loyal Fans Supporting Their Team In A Game", P + "loyal-fans-supporting-their-team-in-a-game-3195653/", "Pressmaster", PX, V + "3195653/3195653-hd_1920_1080_25fps.mp4"),
 ("pexels", "Sports Fans Cheering on the Couch", P + "sports-fans-cheering-on-the-couch-6104279/", "Kampus Production", PX, V + "6104279/6104279-hd_1920_1080_25fps.mp4"),
 ("pexels", "Vibrant Night Football Match at a Stadium", P + "vibrant-night-football-match-at-a-stadium-35053395/", "Necip Duman", PX, V + "35053395/14849030_1920_1080_30fps.mp4"),
 ("pexels", "A Referee Officiating a Basketball Game", P + "a-referee-officiating-a-basketball-game-6767271/", "cottonbro studio", PX, V + "6767271/6767271-uhd_4096_2160_25fps.mp4"),
 ("mixkit", "Young woman lying down watching images on her cell phone", M + "young-woman-lying-down-watching-images-on-her-cell-phone-4911/", "arvizum", MK, "https://assets.mixkit.co/videos/4911/4911-1080.mp4"),
 ("pexels", "Person Typing", P + "person-typing-946146/", "Carlos Arribas", PX, V + "946146/946146-hd_1920_1080_30fps.mp4"),
 ("mixkit", "Youtuber vlogging in his studio", M + "youtuber-vlogging-in-his-studio-41272/", "mixkit", MK, "https://assets.mixkit.co/videos/41272/41272-1080.mp4"),
 ("pexels", "Judge Using a Gavel", P + "judge-using-a-gavel-6699964/", "pickarick", PX, V + "6699964/6699964-hd_1920_1080_24fps.mp4"),
 ("pexels", "Courtroom Proceedings with Diverse Attendees", P + "courtroom-proceedings-with-diverse-attendees-34550101/", "khezez", PX, V + "34550101/14640400_2160_3840_50fps.mp4"),
 ("pexels", "Scale of Justice", P + "scale-of-justice-5636967/", "Sora Shimazaki", PX, V + "5636967/5636967-hd_1920_1080_24fps.mp4"),
 ("pexels", "Empty Jail", P + "empty-jail-5243246/", "Teela Hudak", PX, V + "5243246/5243246-hd_1920_1080_24fps.mp4"),
 ("pexels", "Commuters exiting Dukuh Atas Station at Dusk", P + "commuters-exiting-dukuh-atas-station-at-dusk-39681507/", "Daneswara Eka", PX, V + "39681507/16921228_1920_1080_25fps.mp4"),
]
json.dump([dict(zip(("source", "title", "page", "creator", "license", "url"), r)) for r in rows],
          open(sys.argv[1], "w"), indent=1, ensure_ascii=False)
PY
}

# Cut only the clips that are missing (a clip named on the command line is cut again, so existing ones are not named).
need=""
for i in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15; do
  [ -s "footage/b$i.mp4" ] || need="$need b$i"
done
# shellcheck disable=SC2086
[ -z "$need" ] || python3 ../../scripts/fetch-shots.py shots.json $need

# ---------- 3. Icons: Tabler outline (MIT), stroke #000000 and width 1.5, for use as masks ----------
T='https://raw.githubusercontent.com/tabler/tabler-icons/main/icons/outline'
mkdir -p assets/icons
for n in gavel scale message-circle messages user users ball-football cards eye school certificate license \
         coin cash lock speakerphone microphone question-mark check x brand-youtube brand-instagram device-tv \
         news book building-bank clock calendar music; do
  f="assets/icons/$n.svg"
  [ -s "$f" ] || { curl -fsSL --retry 3 "$T/$n.svg" \
      | sed -e 's/stroke="currentColor"/stroke="#000000"/' -e 's/stroke-width="2"/stroke-width="1.5"/' > "$f.part"; mv "$f.part" "$f"; }
done
ROW='{"source": "Tabler Icons", "url": "https://github.com/tabler/tabler-icons", "page": "https://tabler.io/icons", "title": "Tabler Icons (outline)", "creator": "Paweł Kuna and contributors", "license": "MIT"}'
grep -qsF '"source": "Tabler Icons"' credits-extra.jsonl || echo "$ROW" >> credits-extra.jsonl

echo "pictures ready: $(ls assets/pic/*.jpg | wc -l | tr -d ' ') photos, $(ls footage/b??.mp4 | wc -l | tr -d ' ') b-roll clips, $(ls assets/icons/*.svg | wc -l | tr -d ' ') icons"
