#!/usr/bin/env bash
# Make this inspiration's media with no hand work: 55 public-domain and open-licence pictures from
# Wikimedia Commons (engravings, woodcuts, paintings, manuscript pages, two museum photos) and 3
# pages of an 1832 book from the Internet Archive, cut into 16:9 stills; one screenshot of a web
# page (hyperframes capture); four Mixkit stock clips, downloaded once each and cut by
# scripts/fetch-shots.py from shots.json; three locator maps (scripts/map-svg.py); nine minutes of
# a White Bat Audio mix from YouTube as the music bed (yt-dlp); then CREDITS.md. Everything lands
# in assets/generated/, footage/, and .footage/ (not committed). About 15 minutes. Run from the
# inspiration folder; SKILL_DIR = the skill root.
set -euo pipefail
SKILL_DIR=${SKILL_DIR:-../..}
S="$SKILL_DIR/scripts"
G=assets/generated
mkdir -p "$G/src" .footage/src
UA="direcfy-video-skill/1.0 (HyperFrames video production)"
# Commons limits how fast one address may download: a pause after each file, and curl waits and
# tries again when the server answers 429.
get() { [ -s "$1" ] || { curl -sfL -A "$UA" --retry 5 --retry-delay 20 --retry-max-time 900 -o "$1.part" "$2" && mv "$1.part" "$1"; sleep 2; }; }

# The picks, from asset-search.py image searches ("Hondius Bruegel dancing mania Molenbeek 1642",
# "Strasbourg Nuremberg Chronicle Argentina woodcut", "Paracelsus portrait") and searches of
# Commons for each subject of the narration; video "candles", "church interior", "sun", "clouds"
# (Mixkit). The music was chosen by the user. Commons serves its fixed thumbnail widths (960,
# 1280, 1920, 3840) freely and rate-limits originals, so the pictures are taken at those widths:
# 3840 px where a shot is a detail, the widest the file allows otherwise. One row per file:
#   path  address
while read -r file url; do
  [ -n "$file" ] && get "$file" "$url"
done <<'FILES'
assets/generated/src/procession.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/7/75/Twee_epileptici_en_hun_begeleiders_op_weg_naar_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-525.jpg/3840px-Twee_epileptici_en_hun_begeleiders_op_weg_naar_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-525.jpg
assets/generated/src/footbridge.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/c/c9/Epileptici_en_hun_begeleiders_op_weg_naar_de_brug_bij_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-526.jpg/3840px-Epileptici_en_hun_begeleiders_op_weg_naar_de_brug_bij_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-526.jpg
assets/generated/src/pipers.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/6/67/Twee_doedelzakspelers_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-OB-75.242.jpg/3840px-Twee_doedelzakspelers_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-OB-75.242.jpg
assets/generated/src/strasbourg.png https://upload.wikimedia.org/wikipedia/commons/thumb/4/40/Strasbourg1493.png/1920px-Strasbourg1493.png
assets/generated/src/bridge.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/4/4b/Wenceslas_Hollar_-_Strasbourg_3.jpg/1920px-Wenceslas_Hollar_-_Strasbourg_3.jpg
assets/generated/src/ill.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/0/01/Strassburg._An_der_Ill_-_btv1b10218184r.jpg/1920px-Strassburg._An_der_Ill_-_btv1b10218184r.jpg
assets/generated/src/rhine.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/b/b0/Gezicht_op_de_Rijn_bij_Straatsburg_Am_Rhein_bey_Strassburg_Stadsgezichten_en_landschappen_%28serietitel%29_Amoenissimae_Aliquot_Locorum_In_Diversis_Provincijs_iacentiu_Effigies_%28serietitel%29%2C_RP-P-1907-2845.jpg/3840px-Gezicht_op_de_Rijn_bij_Straatsburg_Am_Rhein_bey_Strassburg_Stadsgezichten_en_landschappen_%28serietitel%29_Amoenissimae_Aliquot_Locorum_In_Diversis_Provincijs_iacentiu_Effigies_%28serietitel%29%2C_RP-P-1907-2845.jpg
assets/generated/src/paracelsus.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/4/4a/Paracelsus.jpg/1920px-Paracelsus.jpg
assets/generated/src/horsemen.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/7/70/Albrecht_D%C3%BCrer%2C_The_Four_Horsemen%2C_1498%2C_NGA_142352.jpg/1920px-Albrecht_D%C3%BCrer%2C_The_Four_Horsemen%2C_1498%2C_NGA_142352.jpg
assets/generated/src/dance.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/2/25/Albrecht_D%C3%BCrer%2C_Peasant_Couple_Dancing%2C_1514%2C_NGA_6647.jpg/1920px-Albrecht_D%C3%BCrer%2C_Peasant_Couple_Dancing%2C_1514%2C_NGA_6647.jpg
assets/generated/src/gersdorff.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/b/b7/Feldtbuch_der_Wundartzney%2C_newlich_getruckt_Wellcome_L0031659.jpg/1920px-Feldtbuch_der_Wundartzney%2C_newlich_getruckt_Wellcome_L0031659.jpg
assets/generated/src/chapel.png https://upload.wikimedia.org/wikipedia/commons/thumb/2/21/La_chapelle_Saint-Guy_1820_Henri-Charles_M%C3%BCller.png/1920px-La_chapelle_Saint-Guy_1820_Henri-Charles_M%C3%BCller.png
assets/generated/src/vitus.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/d/d3/Vitus_in_Kettle%2C_c._1500_-_Zeppelin_Museum_Friedrichshafen_-_DSC06869.JPG/1920px-Vitus_in_Kettle%2C_c._1500_-_Zeppelin_Museum_Friedrichshafen_-_DSC06869.JPG
assets/generated/src/civitates.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/8/89/CivitatesOrbisTerrarum_Strasbourg_%28cropped%29.jpg/3840px-CivitatesOrbisTerrarum_Strasbourg_%28cropped%29.jpg
assets/generated/src/pfalz.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/a/ab/AVES_1_PL_1_d%C3%A9tail_Pfalz.jpg/960px-AVES_1_PL_1_d%C3%A9tail_Pfalz.jpg
assets/generated/src/maximilian.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/3/35/Albrecht_D%C3%BCrer_-_Portrait_of_Maximilian_I_-_Google_Art_Project.jpg/1920px-Albrecht_D%C3%BCrer_-_Portrait_of_Maximilian_I_-_Google_Art_Project.jpg
assets/generated/src/bundschuh.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/1/1c/Bundtschu.jpg/1920px-Bundtschu.jpg
assets/generated/src/seals.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/f/f7/Albrecht_D%C3%BCrer%2C_The_Opening_of_the_Fifth_and_Sixth_Seals%2C_probably_c._1496-1498%2C_NGA_52558.jpg/1920px-Albrecht_D%C3%BCrer%2C_The_Opening_of_the_Fifth_and_Sixth_Seals%2C_probably_c._1496-1498%2C_NGA_52558.jpg
assets/generated/src/peasants.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/7/7b/Albrecht_D%C3%BCrer%2C_Three_Peasants_in_Conversation%2C_c._1497%2C_NGA_6591.jpg/1920px-Albrecht_D%C3%BCrer%2C_Three_Peasants_in_Conversation%2C_c._1497%2C_NGA_6591.jpg
assets/generated/src/market.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/b/ba/Albrecht_D%C3%BCrer%2C_Peasant_Couple_at_Market%2C_1519%2C_NGA_6660.jpg/1920px-Albrecht_D%C3%BCrer%2C_Peasant_Couple_at_Market%2C_1519%2C_NGA_6660.jpg
assets/generated/src/bagpiper.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/4/4d/Albrecht_D%C3%BCrer%2C_Bagpiper%2C_1514%2C_NGA_6645.jpg/1920px-Albrecht_D%C3%BCrer%2C_Bagpiper%2C_1514%2C_NGA_6645.jpg
assets/generated/src/brant.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/1/15/Sebastian_Brant.jpg/1280px-Sebastian_Brant.jpg
assets/generated/src/zabern.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/c/c3/Zabern_%28Elsa%C3%9F%29.jpg/1920px-Zabern_%28Elsa%C3%9F%29.jpg
assets/generated/src/hecker.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/c/c9/Justus_Friedrich_Karl_Hecker.jpg/960px-Justus_Friedrich_Karl_Hecker.jpg
assets/generated/src/syphilis.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/4/4d/Woodcut%3B_Syphilis%3B_1496_Wellcome_L0024996.jpg/960px-Woodcut%3B_Syphilis%3B_1496_Wellcome_L0024996.jpg
assets/generated/src/ergot.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/8/87/K%C3%B6hler%27s_Medizinal-Pflanzen_in_naturgetreuen_Abbildungen_mit_kurz_erl%C3%A4uterndem_Texte_%28Plate_165%29_%288232811024%29.jpg/1280px-K%C3%B6hler%27s_Medizinal-Pflanzen_in_naturgetreuen_Abbildungen_mit_kurz_erl%C3%A4uterndem_Texte_%28Plate_165%29_%288232811024%29.jpg
assets/generated/src/carpenter.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/3/32/Der_Zimmermann_from_Jost_Amman%27s_St%C3%A4nde_und_Handwerker_Wellcome_L0069600.jpg/1920px-Der_Zimmermann_from_Jost_Amman%27s_St%C3%A4nde_und_Handwerker_Wellcome_L0069600.jpg
assets/generated/src/schilling.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/2/23/Diebold_Schilling_Chronik_Folio_2r.jpg/1280px-Diebold_Schilling_Chronik_Folio_2r.jpg
assets/generated/src/cathedral.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/8/80/Wenceslas_Hollar_-_Strasbourg_cathedral.jpg/1920px-Wenceslas_Hollar_-_Strasbourg_cathedral.jpg
assets/generated/src/narrenschiff.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/1/17/Sebastian_Brant%2C_Das_Narrenschiff%2C_Basel%2C_1494%2C_f._2r.jpg/1280px-Sebastian_Brant%2C_Das_Narrenschiff%2C_Basel%2C_1494%2C_f._2r.jpg
assets/generated/src/dancers1.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/3/31/Nuremberg_chronicles_-_dancer_%28CLXXXVIIIr%29.jpg/1920px-Nuremberg_chronicles_-_dancer_%28CLXXXVIIIr%29.jpg
assets/generated/src/dancers2.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/f/f3/Nuremberg_chronicles_-_dancer_%28CCXVIIr%29.jpg/1920px-Nuremberg_chronicles_-_dancer_%28CCXVIIr%29.jpg
assets/generated/src/kolbeck.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/0/07/Kolbeck_T%C3%A4ntzer.jpg/1280px-Kolbeck_T%C3%A4ntzer.jpg
assets/generated/src/specklin.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/2/22/Daniel_Specklin_portrait.jpg/1280px-Daniel_Specklin_portrait.jpg
assets/generated/src/cirurgia.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/c/c0/Patient_Examined_by_Surgeons_from_Das_Buch_der_Cirurgia_Wellcome_L0069537.jpg/1920px-Patient_Examined_by_Surgeons_from_Das_Buch_der_Cirurgia_Wellcome_L0069537.jpg
assets/generated/src/zodiac.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/6/64/De_Astrologia._Astrological_Man%3B_1503_Wellcome_L0012390.jpg/1920px-De_Astrologia._Astrological_Man%3B_1503_Wellcome_L0012390.jpg
assets/generated/src/ostendorfer.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/2/2a/Holzstich_-_Regensburg_-_Pilgerfahrt_zur_Kirche_der_Sch%C3%B6nen_Maria_-_Ostendorfer_-_1523.jpg/1280px-Holzstich_-_Regensburg_-_Pilgerfahrt_zur_Kirche_der_Sch%C3%B6nen_Maria_-_Ostendorfer_-_1523.jpg
assets/generated/src/bening.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/f/f3/Simon_Bening_-_August-_Mowing_Wheat%2C_Binding_Sheaves_-_Google_Art_Project.jpg/1920px-Simon_Bening_-_August-_Mowing_Wheat%2C_Binding_Sheaves_-_Google_Art_Project.jpg
assets/generated/src/beham.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/3/3c/Dansend_boerenpaar%2C_RP-P-OB-10.905.jpg/1920px-Dansend_boerenpaar%2C_RP-P-OB-10.905.jpg
assets/generated/src/wedding.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/4/40/Pieter_Bruegel_the_Elder_-_Wedding_Dance_in_the_Open_Air_-_WGA03505.jpg/1920px-Pieter_Bruegel_the_Elder_-_Wedding_Dance_in_the_Open_Air_-_WGA03505.jpg
assets/generated/src/anthony.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/4/44/Woodcut%3B_St._Antony_abbot_with_cripple_using_crutches._Wellcome_L0007795.jpg/960px-Woodcut%3B_St._Antony_abbot_with_cripple_using_crutches._Wellcome_L0007795.jpg
assets/generated/src/paracelsus2.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/2/25/Aureolus_Theophrastus_Bombastus_von_Hohenheim_%28Paracelsus%29._Wellcome_V0004455.jpg/1920px-Aureolus_Theophrastus_Bombastus_von_Hohenheim_%28Paracelsus%29._Wellcome_V0004455.jpg
assets/generated/src/ban1518.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/1/18/Interdiction_danse_Strasbourg_Sebastien_Brant_1518.jpg/1280px-Interdiction_danse_Strasbourg_Sebastien_Brant_1518.jpg
assets/generated/src/spindler.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/9/91/Danse_Saint_Guy_1518_Charles_Spindler.jpg/1280px-Danse_Saint_Guy_1518_Charles_Spindler.jpg
assets/generated/src/statue1518.png https://upload.wikimedia.org/wikipedia/commons/d/dd/Sculpture_poluchrom%C3%A9e_saint_Guy_Vierge_%C3%A0_l%27Enfant_et_pape_1518.png
assets/generated/src/hornmusic.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/b/bb/Cart_with_Horn_Musicians%2C_The_Triumphal_Procession_of_Emperor_Maximilian_I_MET_DP834076.jpg/1920px-Cart_with_Horn_Musicians%2C_The_Triumphal_Procession_of_Emperor_Maximilian_I_MET_DP834076.jpg
assets/generated/src/musicians.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/d/de/Triumphzug_Kaiser_Maximilian.jpg/1280px-Triumphzug_Kaiser_Maximilian.jpg
assets/generated/src/cart.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/6/6b/Vijf_mannen_duwen_een_kar_een_heuvel_op%2C_RP-P-OB-22.295.jpg/1920px-Vijf_mannen_duwen_een_kar_een_heuvel_op%2C_RP-P-OB-22.295.jpg
assets/generated/src/hunters.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/d/d8/Pieter_Bruegel_the_Elder_-_Hunters_in_the_Snow_%28Winter%29_-_Google_Art_Project.jpg/3840px-Pieter_Bruegel_the_Elder_-_Hunters_in_the_Snow_%28Winter%29_-_Google_Art_Project.jpg
assets/generated/src/deathdance.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/a/a7/Nuremberg_chronicles_-_Dance_of_Death_%28CCLXIIIIv%29.jpg/1920px-Nuremberg_chronicles_-_Dance_of_Death_%28CCLXIIIIv%29.jpg
assets/generated/src/brant2.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/d/df/Duerer_brant.jpg/960px-Duerer_brant.jpg
assets/generated/src/vitusmart.png https://upload.wikimedia.org/wikipedia/commons/thumb/b/b1/Martyrdom_of_saint_Vitus.PNG/1280px-Martyrdom_of_saint_Vitus.PNG
assets/generated/src/molenbeek.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/1/1c/Saint_John%E2%80%99s_Dancers_in_Molenbeeck%E2%80%99_%281592%29_by_Pieter_Brueghel_II.jpg/960px-Saint_John%E2%80%99s_Dancers_in_Molenbeeck%E2%80%99_%281592%29_by_Pieter_Brueghel_II.jpg
assets/generated/src/grotto.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/5/5c/Int%C3%A9rieur_de_la_grotte_de_St._Vit_et_vue_du_chateau_de_G%C3%A9roldseck_-_btv1b102021633.jpg/1280px-Int%C3%A9rieur_de_la_grotte_de_St._Vit_et_vue_du_chateau_de_G%C3%A9roldseck_-_btv1b102021633.jpg
assets/generated/src/hecker-title.jpg https://archive.org/download/b30797160/page/n2.jpg
assets/generated/src/hecker-poem.jpg https://archive.org/download/b30797160/page/n16.jpg
assets/generated/src/hecker-kinds.jpg https://archive.org/download/b30797160/page/n27.jpg
assets/generated/src/drummer.jpg https://upload.wikimedia.org/wikipedia/commons/thumb/d/de/Albrecht_D%C3%BCrer_028.jpg/960px-Albrecht_D%C3%BCrer_028.jpg
.footage/src/candles.mp4 https://assets.mixkit.co/videos/3461/3461-1080.mp4
.footage/src/church.mp4 https://assets.mixkit.co/videos/22723/22723-1080.mp4
.footage/src/sun.mp4 https://assets.mixkit.co/videos/1709/1709-720.mp4
.footage/src/clouds.mp4 https://assets.mixkit.co/videos/2408/2408-1080.mp4
FILES

# The page capture: the Europe PMC record of John Waller's article in The Lancet (2009).
[ -s "$G/src/epmc.png" ] || { rm -rf .capture/epmc
  npx --yes hyperframes@0.8.103 capture "https://europepmc.org/article/MED/19238695" -o .capture/epmc --json --skip-vision --skip-assets --max-screenshots 2 >/dev/null
  cp .capture/epmc/screenshots/scroll-000.png "$G/src/epmc.png"; }

# The music: one bed for the whole film, with no loop. Nine minutes of the White Bat Audio mix
# "Dystopian Synthwave Playlist - Planet Zero", from 22:44 (tracks 7 to 9: "Black Lotus", "Quantum
# Leap", "More Human Than Human"). The mix is free to use with the credit "Karl Casey @ White Bat
# Audio"; only this section is downloaded. scripts/duck-under-voice.py sets its level.
[ -s "$G/src/whitebat.m4a" ] || yt-dlp -q --no-warnings --no-playlist -f "ba[ext=m4a]/ba" --download-sections "*1364-1914" \
  -o "$G/src/whitebat.%(ext)s" "https://www.youtube.com/watch?v=WH2vlG0b2CM" </dev/null

# Where every file came from, for CREDITS.md.
cat > sources.jsonl <<'JSON'
{"file": "assets/generated/src/procession.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/7/75/Twee_epileptici_en_hun_begeleiders_op_weg_naar_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-525.jpg/3840px-Twee_epileptici_en_hun_begeleiders_op_weg_naar_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-525.jpg", "page": "https://commons.wikimedia.org/wiki/File:Twee_epileptici_en_hun_begeleiders_op_weg_naar_Molenbeek_Pelgrimage_naar_Molenbeek_(serietitel),_RP-P-1904-525.jpg", "title": "Pilgrimage of the epileptics to Molenbeek: two women and their helpers (engraving, 1642, after a drawing of 1564)", "creator": "Hendrick Hondius I after Pieter Bruegel the Elder; Rijksmuseum", "license": "CC0"}
{"file": "assets/generated/src/footbridge.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/c/c9/Epileptici_en_hun_begeleiders_op_weg_naar_de_brug_bij_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-526.jpg/3840px-Epileptici_en_hun_begeleiders_op_weg_naar_de_brug_bij_Molenbeek_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-1904-526.jpg", "page": "https://commons.wikimedia.org/wiki/File:Epileptici_en_hun_begeleiders_op_weg_naar_de_brug_bij_Molenbeek_Pelgrimage_naar_Molenbeek_(serietitel),_RP-P-1904-526.jpg", "title": "Pilgrimage of the epileptics to Molenbeek: on the way to the footbridge (engraving, 1642, after a drawing of 1564)", "creator": "Hendrick Hondius I after Pieter Bruegel the Elder; Rijksmuseum", "license": "CC0"}
{"file": "assets/generated/src/pipers.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/6/67/Twee_doedelzakspelers_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-OB-75.242.jpg/3840px-Twee_doedelzakspelers_Pelgrimage_naar_Molenbeek_%28serietitel%29%2C_RP-P-OB-75.242.jpg", "page": "https://commons.wikimedia.org/wiki/File:Twee_doedelzakspelers_Pelgrimage_naar_Molenbeek_(serietitel),_RP-P-OB-75.242.jpg", "title": "Pilgrimage of the epileptics to Molenbeek: two bagpipers (engraving, 1642, after a drawing of 1564)", "creator": "Hendrick Hondius I after Pieter Bruegel the Elder; Rijksmuseum", "license": "CC0"}
{"file": "assets/generated/src/strasbourg.png", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/40/Strasbourg1493.png/1920px-Strasbourg1493.png", "page": "https://commons.wikimedia.org/wiki/File:Strasbourg1493.png", "title": "Argentina (Strasbourg), woodcut from the Nuremberg Chronicle, 1493", "creator": "Workshop of Michael Wolgemut and Wilhelm Pleydenwurff, for Hartmann Schedel", "license": "Public domain"}
{"file": "assets/generated/src/bridge.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/4b/Wenceslas_Hollar_-_Strasbourg_3.jpg/1920px-Wenceslas_Hollar_-_Strasbourg_3.jpg", "page": "https://commons.wikimedia.org/wiki/File:Wenceslas_Hollar_-_Strasbourg_3.jpg", "title": "Die Rhein-Zoll-Bruck bey Strasburg (the Rhine toll bridge near Strasbourg), etching after a drawing of 1629", "creator": "Wenceslaus Hollar; University of Toronto Hollar Collection", "license": "Public domain"}
{"file": "assets/generated/src/ill.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/0/01/Strassburg._An_der_Ill_-_btv1b10218184r.jpg/1920px-Strassburg._An_der_Ill_-_btv1b10218184r.jpg", "page": "https://commons.wikimedia.org/wiki/File:Strassburg._An_der_Ill_-_btv1b10218184r.jpg", "title": "Strassburg. An der Ill (Strasbourg from the river Ill), 17th-century etching in a 1931 facsimile", "creator": "Wenceslaus Hollar; Bibliothèque nationale de France (Gallica)", "license": "Public domain"}
{"file": "assets/generated/src/rhine.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/b/b0/Gezicht_op_de_Rijn_bij_Straatsburg_Am_Rhein_bey_Strassburg_Stadsgezichten_en_landschappen_%28serietitel%29_Amoenissimae_Aliquot_Locorum_In_Diversis_Provincijs_iacentiu_Effigies_%28serietitel%29%2C_RP-P-1907-2845.jpg/3840px-Gezicht_op_de_Rijn_bij_Straatsburg_Am_Rhein_bey_Strassburg_Stadsgezichten_en_landschappen_%28serietitel%29_Amoenissimae_Aliquot_Locorum_In_Diversis_Provincijs_iacentiu_Effigies_%28serietitel%29%2C_RP-P-1907-2845.jpg", "page": "https://commons.wikimedia.org/wiki/File:Gezicht_op_de_Rijn_bij_Straatsburg_Am_Rhein_bey_Strassburg_Stadsgezichten_en_landschappen_(serietitel)_Amoenissimae_Aliquot_Locorum_In_Diversis_Provincijs_iacentiu_Effigies_(serietitel),_RP-P-1907-2845.jpg", "title": "Am Rhein bey Strassburg (the Rhine near Strasbourg), etching, 1635", "creator": "Wenceslaus Hollar; Rijksmuseum", "license": "CC0"}
{"file": "assets/generated/src/paracelsus.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/4a/Paracelsus.jpg/1920px-Paracelsus.jpg", "page": "https://commons.wikimedia.org/wiki/File:Paracelsus.jpg", "title": "Presumed portrait of Paracelsus, 17th-century copy after Quentin Matsys", "creator": "Anonymous, after Quentin Matsys", "license": "Public domain"}
{"file": "assets/generated/src/horsemen.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/7/70/Albrecht_D%C3%BCrer%2C_The_Four_Horsemen%2C_1498%2C_NGA_142352.jpg/1920px-Albrecht_D%C3%BCrer%2C_The_Four_Horsemen%2C_1498%2C_NGA_142352.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer,_The_Four_Horsemen,_1498,_NGA_142352.jpg", "title": "The Four Horsemen, woodcut from the Apocalypse, 1498", "creator": "Albrecht Dürer; National Gallery of Art, Washington", "license": "CC0"}
{"file": "assets/generated/src/dance.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/2/25/Albrecht_D%C3%BCrer%2C_Peasant_Couple_Dancing%2C_1514%2C_NGA_6647.jpg/1920px-Albrecht_D%C3%BCrer%2C_Peasant_Couple_Dancing%2C_1514%2C_NGA_6647.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer,_Peasant_Couple_Dancing,_1514,_NGA_6647.jpg", "title": "Peasant Couple Dancing, engraving, 1514", "creator": "Albrecht Dürer; National Gallery of Art, Washington", "license": "CC0"}
{"file": "assets/generated/src/gersdorff.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/b/b7/Feldtbuch_der_Wundartzney%2C_newlich_getruckt_Wellcome_L0031659.jpg/1920px-Feldtbuch_der_Wundartzney%2C_newlich_getruckt_Wellcome_L0031659.jpg", "page": "https://commons.wikimedia.org/wiki/File:Feldtbuch_der_Wundartzney,_newlich_getruckt_Wellcome_L0031659.jpg", "title": "Bloodletting figure (Lassman), Feldtbuch der Wundartzney, Strasbourg: H. Schott, 1530, fol. 16v", "creator": "Hans von Gersdorff; Wellcome Collection", "license": "CC BY 4.0"}
{"file": "assets/generated/src/chapel.png", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/2/21/La_chapelle_Saint-Guy_1820_Henri-Charles_M%C3%BCller.png/1920px-La_chapelle_Saint-Guy_1820_Henri-Charles_M%C3%BCller.png", "page": "https://commons.wikimedia.org/wiki/File:La_chapelle_Saint-Guy_1820_Henri-Charles_M%C3%BCller.png", "title": "La chapelle Saint-Guy (Sankt Veit) near Saverne, about 1820", "creator": "Henri-Charles Müller; Cabinet des estampes et des dessins, Strasbourg", "license": "Public domain"}
{"file": "assets/generated/src/vitus.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/d/d3/Vitus_in_Kettle%2C_c._1500_-_Zeppelin_Museum_Friedrichshafen_-_DSC06869.JPG/1920px-Vitus_in_Kettle%2C_c._1500_-_Zeppelin_Museum_Friedrichshafen_-_DSC06869.JPG", "page": "https://commons.wikimedia.org/wiki/File:Vitus_in_Kettle,_c._1500_-_Zeppelin_Museum_Friedrichshafen_-_DSC06869.JPG", "title": "Saint Vitus in the kettle, painted wood, South Germany, about 1500 (Zeppelin Museum Friedrichshafen)", "creator": "Anonymous sculptor; photo by Daderot", "license": "Public domain"}
{"file": "assets/generated/src/civitates.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/8/89/CivitatesOrbisTerrarum_Strasbourg_%28cropped%29.jpg/3840px-CivitatesOrbisTerrarum_Strasbourg_%28cropped%29.jpg", "page": "https://commons.wikimedia.org/wiki/File:CivitatesOrbisTerrarum_Strasbourg_(cropped).jpg", "title": "Argentoratum, Strassburg: bird's-eye plan from Civitates Orbis Terrarum, 1572", "creator": "Georg Braun and Frans Hogenberg", "license": "Public domain"}
{"file": "assets/generated/src/pfalz.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/a/ab/AVES_1_PL_1_d%C3%A9tail_Pfalz.jpg/960px-AVES_1_PL_1_d%C3%A9tail_Pfalz.jpg", "page": "https://commons.wikimedia.org/wiki/File:AVES_1_PL_1_d%C3%A9tail_Pfalz.jpg", "title": "The Pfalz, Strasbourg's town hall: detail of a copy of Conrad Morant's plan of 1548", "creator": "After Conrad Morant; Archives de la Ville et de l'Eurométropole de Strasbourg", "license": "Public domain"}
{"file": "assets/generated/src/maximilian.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/3/35/Albrecht_D%C3%BCrer_-_Portrait_of_Maximilian_I_-_Google_Art_Project.jpg/1920px-Albrecht_D%C3%BCrer_-_Portrait_of_Maximilian_I_-_Google_Art_Project.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer_-_Portrait_of_Maximilian_I_-_Google_Art_Project.jpg", "title": "Emperor Maximilian I, painting, 1519", "creator": "Albrecht Dürer; Kunsthistorisches Museum, Vienna", "license": "Public domain"}
{"file": "assets/generated/src/bundschuh.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/1/1c/Bundtschu.jpg/1920px-Bundtschu.jpg", "page": "https://commons.wikimedia.org/wiki/File:Bundtschu.jpg", "title": "Der Bundtschu, title woodcut of the pamphlet, Basel, 1514", "creator": "Pamphilus Gengenbach", "license": "Public domain"}
{"file": "assets/generated/src/seals.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/f/f7/Albrecht_D%C3%BCrer%2C_The_Opening_of_the_Fifth_and_Sixth_Seals%2C_probably_c._1496-1498%2C_NGA_52558.jpg/1920px-Albrecht_D%C3%BCrer%2C_The_Opening_of_the_Fifth_and_Sixth_Seals%2C_probably_c._1496-1498%2C_NGA_52558.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer,_The_Opening_of_the_Fifth_and_Sixth_Seals,_probably_c._1496-1498,_NGA_52558.jpg", "title": "The Opening of the Fifth and Sixth Seals, woodcut from the Apocalypse, about 1497", "creator": "Albrecht Dürer; National Gallery of Art, Washington", "license": "CC0"}
{"file": "assets/generated/src/peasants.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/7/7b/Albrecht_D%C3%BCrer%2C_Three_Peasants_in_Conversation%2C_c._1497%2C_NGA_6591.jpg/1920px-Albrecht_D%C3%BCrer%2C_Three_Peasants_in_Conversation%2C_c._1497%2C_NGA_6591.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer,_Three_Peasants_in_Conversation,_c._1497,_NGA_6591.jpg", "title": "Three Peasants in Conversation, engraving, about 1497", "creator": "Albrecht Dürer; National Gallery of Art, Washington", "license": "CC0"}
{"file": "assets/generated/src/market.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/b/ba/Albrecht_D%C3%BCrer%2C_Peasant_Couple_at_Market%2C_1519%2C_NGA_6660.jpg/1920px-Albrecht_D%C3%BCrer%2C_Peasant_Couple_at_Market%2C_1519%2C_NGA_6660.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer,_Peasant_Couple_at_Market,_1519,_NGA_6660.jpg", "title": "Peasant Couple at Market, engraving, 1519", "creator": "Albrecht Dürer; National Gallery of Art, Washington", "license": "CC0"}
{"file": "assets/generated/src/bagpiper.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/4d/Albrecht_D%C3%BCrer%2C_Bagpiper%2C_1514%2C_NGA_6645.jpg/1920px-Albrecht_D%C3%BCrer%2C_Bagpiper%2C_1514%2C_NGA_6645.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer,_Bagpiper,_1514,_NGA_6645.jpg", "title": "The Bagpiper, engraving, 1514", "creator": "Albrecht Dürer; National Gallery of Art, Washington", "license": "CC0"}
{"file": "assets/generated/src/brant.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/1/15/Sebastian_Brant.jpg/1280px-Sebastian_Brant.jpg", "page": "https://commons.wikimedia.org/wiki/File:Sebastian_Brant.jpg", "title": "Sebastian Brant, painting, about 1508", "creator": "Hans Burgkmair the Elder; Staatliche Kunsthalle Karlsruhe", "license": "Public domain"}
{"file": "assets/generated/src/zabern.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/c/c3/Zabern_%28Elsa%C3%9F%29.jpg/1920px-Zabern_%28Elsa%C3%9F%29.jpg", "page": "https://commons.wikimedia.org/wiki/File:Zabern_(Elsa%C3%9F).jpg", "title": "Elsass Zabern (Saverne), engraving from Topographia Alsatiae, 1644", "creator": "Matthäus Merian", "license": "Public domain"}
{"file": "assets/generated/src/hecker.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/c/c9/Justus_Friedrich_Karl_Hecker.jpg/960px-Justus_Friedrich_Karl_Hecker.jpg", "page": "https://commons.wikimedia.org/wiki/File:Justus_Friedrich_Karl_Hecker.jpg", "title": "Justus Friedrich Karl Hecker, lithograph", "creator": "A. Remy after Rinck", "license": "Public domain"}
{"file": "assets/generated/src/syphilis.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/4d/Woodcut%3B_Syphilis%3B_1496_Wellcome_L0024996.jpg/960px-Woodcut%3B_Syphilis%3B_1496_Wellcome_L0024996.jpg", "page": "https://commons.wikimedia.org/wiki/File:Woodcut%3B_Syphilis%3B_1496_Wellcome_L0024996.jpg", "title": "Woodcut from Sebastian Brant's broadsheet on the French disease, 1496", "creator": "Wellcome Collection", "license": "CC BY 4.0"}
{"file": "assets/generated/src/ergot.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/8/87/K%C3%B6hler%27s_Medizinal-Pflanzen_in_naturgetreuen_Abbildungen_mit_kurz_erl%C3%A4uterndem_Texte_%28Plate_165%29_%288232811024%29.jpg/1280px-K%C3%B6hler%27s_Medizinal-Pflanzen_in_naturgetreuen_Abbildungen_mit_kurz_erl%C3%A4uterndem_Texte_%28Plate_165%29_%288232811024%29.jpg", "page": "https://commons.wikimedia.org/wiki/File:K%C3%B6hler's_Medizinal-Pflanzen_in_naturgetreuen_Abbildungen_mit_kurz_erl%C3%A4uterndem_Texte_(Plate_165)_(8232811024).jpg", "title": "Plate from Köhler's Medizinal-Pflanzen, 1890", "creator": "Franz Eugen Köhler", "license": "Public domain"}
{"file": "assets/generated/src/carpenter.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/3/32/Der_Zimmermann_from_Jost_Amman%27s_St%C3%A4nde_und_Handwerker_Wellcome_L0069600.jpg/1920px-Der_Zimmermann_from_Jost_Amman%27s_St%C3%A4nde_und_Handwerker_Wellcome_L0069600.jpg", "page": "https://commons.wikimedia.org/wiki/File:Der_Zimmermann_from_Jost_Amman's_St%C3%A4nde_und_Handwerker_Wellcome_L0069600.jpg", "title": "Der Zimmermann (the carpenter), woodcut from the Ständebuch, 1568", "creator": "Jost Amman; Wellcome Collection", "license": "CC BY 4.0"}
{"file": "assets/generated/src/schilling.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/2/23/Diebold_Schilling_Chronik_Folio_2r.jpg/1280px-Diebold_Schilling_Chronik_Folio_2r.jpg", "page": "https://commons.wikimedia.org/wiki/File:Diebold_Schilling_Chronik_Folio_2r.jpg", "title": "Lucerne Chronicle, fol. 2r, 1513", "creator": "Diebold Schilling the Younger", "license": "Public domain"}
{"file": "assets/generated/src/cathedral.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/8/80/Wenceslas_Hollar_-_Strasbourg_cathedral.jpg/1920px-Wenceslas_Hollar_-_Strasbourg_cathedral.jpg", "page": "https://commons.wikimedia.org/wiki/File:Wenceslas_Hollar_-_Strasbourg_cathedral.jpg", "title": "Strasbourg Cathedral, etching, 1630", "creator": "Wenceslaus Hollar; University of Toronto Hollar Collection", "license": "Public domain"}
{"file": "assets/generated/src/narrenschiff.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/1/17/Sebastian_Brant%2C_Das_Narrenschiff%2C_Basel%2C_1494%2C_f._2r.jpg/1280px-Sebastian_Brant%2C_Das_Narrenschiff%2C_Basel%2C_1494%2C_f._2r.jpg", "page": "https://commons.wikimedia.org/wiki/File:Sebastian_Brant,_Das_Narrenschiff,_Basel,_1494,_f._2r.jpg", "title": "Das Narrenschiff (The Ship of Fools), Basel, 1494, fol. 2r", "creator": "Sebastian Brant", "license": "Public domain"}
{"file": "assets/generated/src/dancers1.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/3/31/Nuremberg_chronicles_-_dancer_%28CLXXXVIIIr%29.jpg/1920px-Nuremberg_chronicles_-_dancer_%28CLXXXVIIIr%29.jpg", "page": "https://commons.wikimedia.org/wiki/File:Nuremberg_chronicles_-_dancer_(CLXXXVIIIr).jpg", "title": "Dancers, woodcut from the Nuremberg Chronicle, fol. 188r, 1493", "creator": "Workshop of Michael Wolgemut and Wilhelm Pleydenwurff, for Hartmann Schedel", "license": "Public domain"}
{"file": "assets/generated/src/dancers2.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/f/f3/Nuremberg_chronicles_-_dancer_%28CCXVIIr%29.jpg/1920px-Nuremberg_chronicles_-_dancer_%28CCXVIIr%29.jpg", "page": "https://commons.wikimedia.org/wiki/File:Nuremberg_chronicles_-_dancer_(CCXVIIr).jpg", "title": "Dancers, woodcut from the Nuremberg Chronicle, fol. 217r, 1493", "creator": "Workshop of Michael Wolgemut and Wilhelm Pleydenwurff, for Hartmann Schedel", "license": "Public domain"}
{"file": "assets/generated/src/kolbeck.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/0/07/Kolbeck_T%C3%A4ntzer.jpg/1280px-Kolbeck_T%C3%A4ntzer.jpg", "page": "https://commons.wikimedia.org/wiki/File:Kolbeck_T%C3%A4ntzer.jpg", "title": "The dancers of Kölbigk, engraving from Gottfried's Historische Chronica, 1674", "creator": "After Matthäus Merian, for Johann Ludwig Gottfried", "license": "Public domain"}
{"file": "assets/generated/src/specklin.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/2/22/Daniel_Specklin_portrait.jpg/1280px-Daniel_Specklin_portrait.jpg", "page": "https://commons.wikimedia.org/wiki/File:Daniel_Specklin_portrait.jpg", "title": "Daniel Specklin, engraving (reprint of 1791)", "creator": "Théodore de Bry", "license": "Public domain"}
{"file": "assets/generated/src/cirurgia.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/c/c0/Patient_Examined_by_Surgeons_from_Das_Buch_der_Cirurgia_Wellcome_L0069537.jpg/1920px-Patient_Examined_by_Surgeons_from_Das_Buch_der_Cirurgia_Wellcome_L0069537.jpg", "page": "https://commons.wikimedia.org/wiki/File:Patient_Examined_by_Surgeons_from_Das_Buch_der_Cirurgia_Wellcome_L0069537.jpg", "title": "A patient examined by surgeons, woodcut from Das Buch der Cirurgia, Strasbourg: Grüninger, 1497", "creator": "Hieronymus Brunschwig; Wellcome Collection", "license": "CC BY 4.0"}
{"file": "assets/generated/src/zodiac.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/6/64/De_Astrologia._Astrological_Man%3B_1503_Wellcome_L0012390.jpg/1920px-De_Astrologia._Astrological_Man%3B_1503_Wellcome_L0012390.jpg", "page": "https://commons.wikimedia.org/wiki/File:De_Astrologia._Astrological_Man%3B_1503_Wellcome_L0012390.jpg", "title": "Zodiac man, woodcut from Gregor Reisch, Margarita philosophica, 1503", "creator": "Gregor Reisch; Wellcome Collection", "license": "CC BY 4.0"}
{"file": "assets/generated/src/ostendorfer.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/2/2a/Holzstich_-_Regensburg_-_Pilgerfahrt_zur_Kirche_der_Sch%C3%B6nen_Maria_-_Ostendorfer_-_1523.jpg/1280px-Holzstich_-_Regensburg_-_Pilgerfahrt_zur_Kirche_der_Sch%C3%B6nen_Maria_-_Ostendorfer_-_1523.jpg", "page": "https://commons.wikimedia.org/wiki/File:Holzstich_-_Regensburg_-_Pilgerfahrt_zur_Kirche_der_Sch%C3%B6nen_Maria_-_Ostendorfer_-_1523.jpg", "title": "The pilgrimage to the Fair Virgin of Regensburg, woodcut, about 1520", "creator": "Michael Ostendorfer", "license": "Public domain"}
{"file": "assets/generated/src/bening.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/f/f3/Simon_Bening_-_August-_Mowing_Wheat%2C_Binding_Sheaves_-_Google_Art_Project.jpg/1920px-Simon_Bening_-_August-_Mowing_Wheat%2C_Binding_Sheaves_-_Google_Art_Project.jpg", "page": "https://commons.wikimedia.org/wiki/File:Simon_Bening_-_August-_Mowing_Wheat,_Binding_Sheaves_-_Google_Art_Project.jpg", "title": "August: mowing wheat, binding sheaves (Da Costa Hours), about 1515", "creator": "Simon Bening; The Morgan Library & Museum", "license": "Public domain"}
{"file": "assets/generated/src/beham.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/3/3c/Dansend_boerenpaar%2C_RP-P-OB-10.905.jpg/1920px-Dansend_boerenpaar%2C_RP-P-OB-10.905.jpg", "page": "https://commons.wikimedia.org/wiki/File:Dansend_boerenpaar,_RP-P-OB-10.905.jpg", "title": "Dancing peasant couple, engraving, 1522", "creator": "Hans Sebald Beham; Rijksmuseum", "license": "CC0"}
{"file": "assets/generated/src/wedding.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/40/Pieter_Bruegel_the_Elder_-_Wedding_Dance_in_the_Open_Air_-_WGA03505.jpg/1920px-Pieter_Bruegel_the_Elder_-_Wedding_Dance_in_the_Open_Air_-_WGA03505.jpg", "page": "https://commons.wikimedia.org/wiki/File:Pieter_Bruegel_the_Elder_-_Wedding_Dance_in_the_Open_Air_-_WGA03505.jpg", "title": "The Wedding Dance, painting, about 1566", "creator": "Pieter Bruegel the Elder; Detroit Institute of Arts", "license": "Public domain"}
{"file": "assets/generated/src/anthony.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/4/44/Woodcut%3B_St._Antony_abbot_with_cripple_using_crutches._Wellcome_L0007795.jpg/960px-Woodcut%3B_St._Antony_abbot_with_cripple_using_crutches._Wellcome_L0007795.jpg", "page": "https://commons.wikimedia.org/wiki/File:Woodcut%3B_St._Antony_abbot_with_cripple_using_crutches._Wellcome_L0007795.jpg", "title": "Saint Anthony with a victim of Saint Anthony's fire, woodcut from Hans von Gersdorff, Feldtbuch der Wundartzney, Strasbourg, 1517", "creator": "Hans Wechtlin, for Hans von Gersdorff; Wellcome Collection", "license": "CC BY 4.0"}
{"file": "assets/generated/src/paracelsus2.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/2/25/Aureolus_Theophrastus_Bombastus_von_Hohenheim_%28Paracelsus%29._Wellcome_V0004455.jpg/1920px-Aureolus_Theophrastus_Bombastus_von_Hohenheim_%28Paracelsus%29._Wellcome_V0004455.jpg", "page": "https://commons.wikimedia.org/wiki/File:Aureolus_Theophrastus_Bombastus_von_Hohenheim_(Paracelsus)._Wellcome_V0004455.jpg", "title": "Paracelsus, 1927 reproduction of the etching of 1538 by Augustin Hirschvogel", "creator": "Wellcome Collection", "license": "CC BY 4.0"}
{"file": "assets/generated/src/ban1518.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/1/18/Interdiction_danse_Strasbourg_Sebastien_Brant_1518.jpg/1280px-Interdiction_danse_Strasbourg_Sebastien_Brant_1518.jpg", "page": "https://commons.wikimedia.org/wiki/File:Interdiction_danse_Strasbourg_Sebastien_Brant_1518.jpg", "title": "The council's ban on dancing, written by Sebastian Brant, 2 August 1518", "creator": "Sebastian Brant; Archives de la Ville et de l'Eurométropole de Strasbourg", "license": "Public domain"}
{"file": "assets/generated/src/spindler.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/9/91/Danse_Saint_Guy_1518_Charles_Spindler.jpg/1280px-Danse_Saint_Guy_1518_Charles_Spindler.jpg", "page": "https://commons.wikimedia.org/wiki/File:Danse_Saint_Guy_1518_Charles_Spindler.jpg", "title": "La danse de Saint-Guy, 1518 (an imagined scene), drawing, 1893", "creator": "Charles Spindler", "license": "Public domain"}
{"file": "assets/generated/src/statue1518.png", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/d/dd/Sculpture_poluchrom%C3%A9e_saint_Guy_Vierge_%C3%A0_l%27Enfant_et_pape_1518.png", "page": "https://commons.wikimedia.org/wiki/File:Sculpture_poluchrom%C3%A9e_saint_Guy_Vierge_%C3%A0_l'Enfant_et_pape_1518.png", "title": "Saint Vitus, the Virgin and Child, and a pope: carved group from the chapel of Saint Vitus near Saverne, about 1518", "creator": "Workshop of Hans Wydyz, Strasbourg; photo by Sophie Guillot de Suduiraut", "license": "CC BY 4.0"}
{"file": "assets/generated/src/hornmusic.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/b/bb/Cart_with_Horn_Musicians%2C_The_Triumphal_Procession_of_Emperor_Maximilian_I_MET_DP834076.jpg/1920px-Cart_with_Horn_Musicians%2C_The_Triumphal_Procession_of_Emperor_Maximilian_I_MET_DP834076.jpg", "page": "https://commons.wikimedia.org/wiki/File:Cart_with_Horn_Musicians,_The_Triumphal_Procession_of_Emperor_Maximilian_I_MET_DP834076.jpg", "title": "Cart with horn musicians, woodcut from The Triumphal Procession of Emperor Maximilian I, about 1516-18", "creator": "Hans Burgkmair the Elder; The Metropolitan Museum of Art", "license": "CC0"}
{"file": "assets/generated/src/musicians.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/d/de/Triumphzug_Kaiser_Maximilian.jpg/1280px-Triumphzug_Kaiser_Maximilian.jpg", "page": "https://commons.wikimedia.org/wiki/File:Triumphzug_Kaiser_Maximilian.jpg", "title": "Musica Canterey (the cart of singers), woodcuts 25 and 26 of The Triumphal Procession of Emperor Maximilian I, 1517-18, coloured in 1765", "creator": "Hans Burgkmair the Elder; colouring by Joseph Hoeger", "license": "Public domain"}
{"file": "assets/generated/src/cart.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/6/6b/Vijf_mannen_duwen_een_kar_een_heuvel_op%2C_RP-P-OB-22.295.jpg/1920px-Vijf_mannen_duwen_een_kar_een_heuvel_op%2C_RP-P-OB-22.295.jpg", "page": "https://commons.wikimedia.org/wiki/File:Vijf_mannen_duwen_een_kar_een_heuvel_op,_RP-P-OB-22.295.jpg", "title": "Five men push a cart uphill, woodcut, 1531", "creator": "Hans Weiditz (the Petrarch Master); Rijksmuseum", "license": "CC0"}
{"file": "assets/generated/src/hunters.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/d/d8/Pieter_Bruegel_the_Elder_-_Hunters_in_the_Snow_%28Winter%29_-_Google_Art_Project.jpg/3840px-Pieter_Bruegel_the_Elder_-_Hunters_in_the_Snow_%28Winter%29_-_Google_Art_Project.jpg", "page": "https://commons.wikimedia.org/wiki/File:Pieter_Bruegel_the_Elder_-_Hunters_in_the_Snow_(Winter)_-_Google_Art_Project.jpg", "title": "The Hunters in the Snow (Winter), painting, 1565", "creator": "Pieter Bruegel the Elder; Kunsthistorisches Museum, Vienna", "license": "Public domain"}
{"file": "assets/generated/src/deathdance.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/a/a7/Nuremberg_chronicles_-_Dance_of_Death_%28CCLXIIIIv%29.jpg/1920px-Nuremberg_chronicles_-_Dance_of_Death_%28CCLXIIIIv%29.jpg", "page": "https://commons.wikimedia.org/wiki/File:Nuremberg_chronicles_-_Dance_of_Death_(CCLXIIIIv).jpg", "title": "Imago mortis (the dance of the dead), woodcut from the Nuremberg Chronicle, 1493", "creator": "Workshop of Michael Wolgemut, for Hartmann Schedel", "license": "Public domain"}
{"file": "assets/generated/src/brant2.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/d/df/Duerer_brant.jpg/960px-Duerer_brant.jpg", "page": "https://commons.wikimedia.org/wiki/File:Duerer_brant.jpg", "title": "Sebastian Brant, silverpoint drawing, about 1520", "creator": "Albrecht Dürer; Kupferstichkabinett, Berlin", "license": "Public domain"}
{"file": "assets/generated/src/vitusmart.png", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/b/b1/Martyrdom_of_saint_Vitus.PNG/1280px-Martyrdom_of_saint_Vitus.PNG", "page": "https://commons.wikimedia.org/wiki/File:Martyrdom_of_saint_Vitus.PNG", "title": "The Martyrdom of Saint Vitus, panel painting, about 1450", "creator": "Anonymous German painter; National Museum, Warsaw", "license": "Public domain"}
{"file": "assets/generated/src/molenbeek.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/1/1c/Saint_John%E2%80%99s_Dancers_in_Molenbeeck%E2%80%99_%281592%29_by_Pieter_Brueghel_II.jpg/960px-Saint_John%E2%80%99s_Dancers_in_Molenbeeck%E2%80%99_%281592%29_by_Pieter_Brueghel_II.jpg", "page": "https://commons.wikimedia.org/wiki/File:Saint_John%E2%80%99s_Dancers_in_Molenbeeck%E2%80%99_(1592)_by_Pieter_Brueghel_II.jpg", "title": "Saint John's Dancers in Molenbeek, painting, 1592", "creator": "Pieter Brueghel the Younger", "license": "Public domain"}
{"file": "assets/generated/src/grotto.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/5/5c/Int%C3%A9rieur_de_la_grotte_de_St._Vit_et_vue_du_chateau_de_G%C3%A9roldseck_-_btv1b102021633.jpg/1280px-Int%C3%A9rieur_de_la_grotte_de_St._Vit_et_vue_du_chateau_de_G%C3%A9roldseck_-_btv1b102021633.jpg", "page": "https://commons.wikimedia.org/wiki/File:Int%C3%A9rieur_de_la_grotte_de_St._Vit_et_vue_du_chateau_de_G%C3%A9roldseck_-_btv1b102021633.jpg", "title": "Inside the grotto of Saint Vitus near Saverne, lithograph, 1828", "creator": "Jean-Baptiste Arnout and Louis-Philippe-Alphonse Bichebois; Bibliothèque nationale de France (Gallica)", "license": "Public domain"}
{"file": "assets/generated/src/hecker-title.jpg", "source": "archive", "url": "https://archive.org/download/b30797160/page/n2.jpg", "page": "https://archive.org/details/b30797160", "title": "Die Tanzwuth, eine Volkskrankheit im Mittelalter, Berlin: Enslin, 1832: title page", "creator": "Justus Friedrich Karl Hecker; Wellcome Library", "license": "Public Domain Mark"}
{"file": "assets/generated/src/hecker-poem.jpg", "source": "archive", "url": "https://archive.org/download/b30797160/page/n16.jpg", "page": "https://archive.org/details/b30797160", "title": "Die Tanzwuth, eine Volkskrankheit im Mittelalter, Berlin: Enslin, 1832: page 7, with the verse from the Strasbourg chronicle", "creator": "Justus Friedrich Karl Hecker; Wellcome Library", "license": "Public Domain Mark"}
{"file": "assets/generated/src/hecker-kinds.jpg", "source": "archive", "url": "https://archive.org/download/b30797160/page/n27.jpg", "page": "https://archive.org/details/b30797160", "title": "Die Tanzwuth, eine Volkskrankheit im Mittelalter, Berlin: Enslin, 1832: page 18, on the three kinds of Saint Vitus's dance in Paracelsus", "creator": "Justus Friedrich Karl Hecker; Wellcome Library", "license": "Public Domain Mark"}
{"file": "assets/generated/src/drummer.jpg", "source": "wikimedia", "url": "https://upload.wikimedia.org/wikipedia/commons/thumb/d/de/Albrecht_D%C3%BCrer_028.jpg/960px-Albrecht_D%C3%BCrer_028.jpg", "page": "https://commons.wikimedia.org/wiki/File:Albrecht_D%C3%BCrer_028.jpg", "title": "Piper and drummer, panel of the Jabach Altarpiece, about 1504", "creator": "Albrecht Dürer; Wallraf-Richartz-Museum, Cologne", "license": "Public domain"}
{"file": "assets/generated/src/epmc.png", "source": "web capture", "url": "https://europepmc.org/article/MED/19238695", "page": "https://europepmc.org/article/MED/19238695", "title": "Europe PMC record of J. Waller, A forgotten plague: making sense of dancing mania (The Lancet, 2009), page screenshot", "creator": "Europe PMC", "license": "Website screenshot, shown as a citation"}
{"file": ".footage/src/candles.mp4", "source": "mixkit", "url": "https://assets.mixkit.co/videos/3461/3461-1080.mp4", "page": "https://mixkit.co/free-stock-video/lighting-candles-in-the-dark-3461/", "title": "Lighting candles in the dark", "creator": "Mixkit", "license": "Mixkit Free License"}
{"file": ".footage/src/church.mp4", "source": "mixkit", "url": "https://assets.mixkit.co/videos/22723/22723-1080.mp4", "page": "https://mixkit.co/free-stock-video/interior-of-a-huge-gothic-church-with-large-columns-22723/", "title": "Interior of a huge gothic church with large columns", "creator": "Mixkit", "license": "Mixkit Free License"}
{"file": ".footage/src/sun.mp4", "source": "mixkit", "url": "https://assets.mixkit.co/videos/1709/1709-720.mp4", "page": "https://mixkit.co/free-stock-video/sunlight-crossing-the-branches-of-trees-1709/", "title": "Sunlight crossing the branches of trees", "creator": "Mixkit", "license": "Mixkit Free License"}
{"file": ".footage/src/clouds.mp4", "source": "mixkit", "url": "https://assets.mixkit.co/videos/2408/2408-1080.mp4", "page": "https://mixkit.co/free-stock-video/clouds-and-blue-sky-background-2408/", "title": "Clouds and blue sky background", "creator": "Mixkit", "license": "Mixkit Free License"}
{"file": "assets/generated/src/whitebat.m4a", "source": "youtube", "url": "https://www.youtube.com/watch?v=WH2vlG0b2CM", "page": "https://www.youtube.com/watch?v=WH2vlG0b2CM", "title": "Dystopian Synthwave Playlist - Planet Zero // Royalty Free Copyright Safe Music (from 22:44: Black Lotus, Quantum Leap, More Human Than Human)", "creator": "Karl Casey @ White Bat Audio", "license": "Royalty free with credit: Karl Casey @ White Bat Audio"}
{"file": "assets/generated/map-europe.svg", "source": "naturalearth", "url": "https://www.naturalearthdata.com/", "page": "https://github.com/nvkelso/natural-earth-vector", "title": "Natural Earth vector data (map)", "creator": "Natural Earth", "license": "Public domain"}
{"file": "assets/generated/map-alsace.svg", "source": "naturalearth", "url": "https://www.naturalearthdata.com/", "page": "https://github.com/nvkelso/natural-earth-vector", "title": "Natural Earth vector data (map)", "creator": "Natural Earth", "license": "Public domain"}
{"file": "assets/generated/map-africa.svg", "source": "naturalearth", "url": "https://www.naturalearthdata.com/", "page": "https://github.com/nvkelso/natural-earth-vector", "title": "Natural Earth vector data (map)", "creator": "Natural Earth", "license": "Public domain"}
JSON

# The stock clips: cut to their slots (shots.json).
python3 "$S/fetch-shots.py" shots.json >/dev/null

# The stills: each shot is a 16:9 window of its picture, cut here so no frame, museum ground, or
# collector's mark is left in view, and scaled to the frame. One row per still:
#   name  source  x  y  width  (height = width * 9/16, in the source's pixels)
while read -r name src x y w; do
  [ -n "$name" ] || continue
  [ -s "$G/$name.jpg" ] || ffmpeg -nostdin -v error -y -i "$G/src/$src" -vf "crop=${w}:$((w * 9 / 16)):${x}:${y},scale=1920:1080:flags=lanczos" -q:v 3 "$G/$name.jpg"
  python3 "$S/made.py" "$G/$name.jpg" "$G/src/$src" >/dev/null
done <<'ROWS'
stras-wide strasbourg.png 0 0 1920
stras-wide2 strasbourg.png 60 120 1800
stras-city strasbourg.png 760 440 1160
stras-city2 strasbourg.png 640 300 1280
stras-gate strasbourg.png 20 450 1130
stras-gate2 strasbourg.png 0 520 900
hon-left footbridge.jpg 500 2000 1900
hon-left2 footbridge.jpg 300 1950 2300
hon-right footbridge.jpg 1900 2000 1900
hon-right2 footbridge.jpg 1700 1900 2100
hon-bridge footbridge.jpg 900 500 2800
hon-wide footbridge.jpg 80 1950 3700
hon-wide2 footbridge.jpg 200 2300 3400
hon-feet footbridge.jpg 700 3500 2400
hon-feet2 footbridge.jpg 1400 3600 2200
pro-group procession.jpg 100 2150 3600
pro-group2 procession.jpg 0 2500 3840
pro-group3 procession.jpg 300 2200 3000
pro-group4 procession.jpg 0 2000 3840
pro-faces2 procession.jpg 1500 2100 2300
pro-faces3 procession.jpg 1900 2200 1600
pro-left procession.jpg 150 2200 1700
pro-feet procession.jpg 900 4050 2000
pipers pipers.jpg 200 180 3200
piper-left pipers.jpg 200 250 1700
bridge bridge.jpg 180 38 1480
ill ill.jpg 70 24 1780
ill2 ill.jpg 300 250 1300
rhine rhine.jpg 160 80 3560
paracelsus-face paracelsus.jpg 260 700 1400
horsemen horsemen.jpg 0 470 1920
horsemen-death horsemen.jpg 0 1300 1500
dance dance.jpg 0 300 1920
dance-feet dance.jpg 0 1900 1920
dance-feet2 dance.jpg 200 1850 1600
lassman gersdorff.jpg 60 20 1800
heart gersdorff.jpg 500 330 1000
lassman-legs gersdorff.jpg 0 1700 1920
chapel chapel.png 0 150 1920
chapel2 chapel.png 200 200 1600
vitus vitus.jpg 0 300 1920
vitus-kettle vitus.jpg 0 1900 1920
civ-wide civitates.jpg 0 453 3840
civ-wide2 civitates.jpg 300 500 3200
civ-wide3 civitates.jpg 500 600 2900
civ-north civitates.jpg 900 700 1800
civ-north2 civitates.jpg 1300 600 1700
civ-cathedral civitates.jpg 1700 1500 1400
civ-market civitates.jpg 1000 1300 1700
pfalz pfalz.jpg 0 180 960
maximilian maximilian.jpg 520 450 1400
bundschuh bundschuh.jpg 100 480 1700
seals-top seals.jpg 0 850 1920
seals-people seals.jpg 0 1575 1920
peasants peasants.jpg 0 200 1920
peasants2 peasants.jpg 250 250 1400
market market.jpg 0 250 1920
bagpiper bagpiper.jpg 0 480 1920
brant brant.jpg 0 150 1280
zabern zabern.jpg 200 30 1340
syphilis syphilis.jpg 0 330 960
ergot ergot.jpg 0 120 1280
carpenter carpenter.jpg 150 430 1620
carpenter2 carpenter.jpg 300 500 1200
schilling schilling.jpg 0 250 1280
schilling-close schilling.jpg 300 380 900
cath-base cathedral.jpg 0 1150 1920
cath-spire cathedral.jpg 500 60 1100
cath-mid cathedral.jpg 250 500 1500
narrenschiff narrenschiff.jpg 0 60 1280
dancers2 dancers2.jpg 0 200 1920
dancers2-lute dancers2.jpg 0 150 1100
kolbeck kolbeck.jpg 0 120 1280
kolbeck2 kolbeck.jpg 250 250 1000
specklin specklin.jpg 0 120 1280
cirurgia cirurgia.jpg 0 560 1920
cirurgia-close cirurgia.jpg 150 600 1000
zodiac zodiac.jpg 200 330 1520
ostendorfer ostendorfer.jpg 0 1000 1280
ostendorfer-close ostendorfer.jpg 300 1050 900
bening bening.jpg 0 100 1920
beham beham.jpg 0 500 1920
wedding wedding.jpg 0 200 1920
anthony anthony.jpg 0 700 960
anthony-top anthony.jpg 0 0 960
paracelsus2 paracelsus2.jpg 260 330 1400
paracelsus2b paracelsus2.jpg 100 200 1720
ban ban1518.jpg 0 480 1280
ban-top ban1518.jpg 0 60 1280
ban-detail ban1518.jpg 150 250 900
ban-sign ban1518.jpg 300 760 800
spindler spindler.jpg 30 620 800
statue statue1518.png 0 130 960
hornmusic hornmusic.jpg 0 260 1920
musicians musicians.jpg 0 0 1210
cart cart.jpg 0 330 1920
cart2 cart.jpg 300 400 1400
hunters hunters.jpg 0 286 3840
deathdance deathdance.jpg 0 250 1920
deathdance-close deathdance.jpg 300 150 1100
deathdance2 deathdance.jpg 500 300 1400
brant2 brant2.jpg 0 100 960
vitusmart-wide vitusmart.png 0 200 1280
vitusmart-wide2 vitusmart.png 0 500 1280
vitusmart-close vitusmart.png 200 650 1080
vitusmart-close2 vitusmart.png 300 600 900
molenbeek molenbeek.jpg 0 90 960
grotto grotto.jpg 0 130 1280
grotto2 grotto.jpg 300 250 980
grotto-cross grotto.jpg 400 250 860
hecker-title-wide hecker-title.jpg 300 3250 1700
hecker-title hecker-title.jpg 60 330 2000
hecker-poem hecker-poem.jpg 300 2950 1800
hecker-kinds hecker-kinds.jpg 400 1830 1860
hecker-kinds-top hecker-kinds.jpg 300 60 1800
drummer drummer.jpg 0 620 960
hecker-title2 hecker-title.jpg 0 150 2230
hon-wide3 footbridge.jpg 80 1800 3700
hecker-note hecker-poem.jpg 150 2450 2050
dancers1-close dancers1.jpg 500 150 1100
rhine2 rhine.jpg 600 300 2900
epmc epmc.png 0 0 1920
ROWS

# The framed portraits: the head and shoulders, 4:5.
while read -r name src x y w; do
  [ -n "$name" ] || continue
  [ -s "$G/$name.jpg" ] || ffmpeg -nostdin -v error -y -i "$G/src/$src" -vf "crop=${w}:$((w * 5 / 4)):${x}:${y},scale=680:850:flags=lanczos" -q:v 3 "$G/$name.jpg"
  python3 "$S/made.py" "$G/$name.jpg" "$G/src/$src" >/dev/null
done <<'ROWS'
f-paracelsus paracelsus.jpg 300 330 1400
f-specklin specklin.jpg 250 130 800
f-hecker hecker.jpg 160 150 640
ROWS

# The mix is silent for 1.5 s at the start of track 7 and for 6.3 s between tracks 8 and 9 (437 to
# 443 s of the section), and this film keeps its music on from first frame to last. So the lead-in
# is trimmed and the two tracks are joined over the gap: the tail of track 8 fades under the start
# of track 9 for 6 s. Nothing else is changed; record what the copy was made from.
[ -s "$G/bed.m4a" ] || ffmpeg -nostdin -v error -y -i "$G/src/whitebat.m4a" \
  -filter_complex "[0]atrim=1.5:436,asetpts=PTS-STARTPTS[a];[0]atrim=start=443.2,asetpts=PTS-STARTPTS[b];[a][b]acrossfade=d=6:c1=tri:c2=tri" -c:a aac -b:a 192k "$G/bed.m4a"
python3 "$S/made.py" "$G/bed.m4a" "$G/src/whitebat.m4a" >/dev/null

# The maps, in the style's colours. Pins give frame coordinates only (the .json beside each map):
# the compositions place the dots, the labels, and the course of the Rhine (r01..r16) from them.
# Borders are today's.
CSS=".land{fill:#c9792f;stroke:#2a2118;stroke-width:1.6}.land.focus{fill:#c9792f;stroke:#2a2118;stroke-width:1.6}"
[ -s "$G/map-europe.svg" ] || python3 "$S/map-svg.py" --focus France --focus Germany --extent=-5.5,42.5,17.5,55.2 --anchor 0.5,0.5 --pad 0.02 -o "$G/map-europe.svg" --css "$CSS" \
  --pin "Strasbourg=48.583,7.746" --pin "Aachen=50.776,6.084" --pin "Koeln=50.938,6.960" --pin "Metz=49.119,6.176" --pin "Utrecht=52.091,5.122" --pin "Trier=49.757,6.641" \
  --pin "r01=46.85,9.53" --pin "r02=47.50,9.60" --pin "r03=47.66,9.18" --pin "r04=47.56,7.59" --pin "r05=48.57,7.80" --pin "r06=49.03,8.30" --pin "r07=49.49,8.45" --pin "r08=50.00,8.27" \
  --pin "r09=49.97,7.90" --pin "r10=50.36,7.60" --pin "r11=50.94,6.96" --pin "r12=51.43,6.76" --pin "r13=51.83,6.25" --pin "r14=51.87,5.20" --pin "r15=51.92,4.48" --pin "r16=51.98,4.12" >/dev/null
[ -s "$G/map-alsace.svg" ] || python3 "$S/map-svg.py" --focus France --extent=5.4,47.3,10.0,49.75 --anchor 0.5,0.5 --pad 0.02 --detail 10m -o "$G/map-alsace.svg" --css "$CSS" \
  --pin "Strasbourg=48.583,7.746" --pin "Saverne=48.741,7.362" >/dev/null
[ -s "$G/map-africa.svg" ] || python3 "$S/map-svg.py" --focus Tanzania --extent=22,-13,48,3.5 --anchor 0.5,0.5 --pad 0.02 -o "$G/map-africa.svg" --css "$CSS" \
  --pin "Kashasha=-1.45,31.75" >/dev/null
python3 "$S/footage-credits.py" >/dev/null
echo "media ready: footage/, $G/, CREDITS.md"
