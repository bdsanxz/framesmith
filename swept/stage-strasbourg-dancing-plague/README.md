# The dancing plague of Strasbourg

An 8 min 54 s documentary (1920×1080, narrated in Indonesian) in the `documentary room` style (`references/documentary-room.md`): the "art and history" case of the style's three proof films. It has the style's whole shape: a disclaimer card, a narrated cold open that ends on four questions and the channel line, the channel sting, four chapters (each a chapter card and twelve beats), and the sources. Almost every picture is a painting, an engraving, a woodcut, or a page of a manuscript or an old book under a slow push; six shots are stock film. Every picture carries the style's teal-and-amber grade, and one film gate (vignette, edge marks, light leaks, dust) lies over the whole film.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

The subject is the dancing plague of July to September 1518: a woman known as Frau Troffea begins to dance in a street of Strasbourg, others join her within days, the city council first has the dancers dance it out with hired musicians, then bans music and sends them to the shrine of Saint Vitus near Saverne.

```bash
./make-footage.sh                      # 55 pictures, 3 book pages, a page capture, 4 stock clips, 3 maps, music -> footage/ and assets/generated/ (about 9 minutes, 350 MB)
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4      # about 20 minutes on an idle machine
```

## Narration

`narration.txt` holds one paragraph per beat (54 paragraphs, 953 words), written in the calm-ellipsis formula of the Qwen voice family `sawi` (`references/qwen-voices.md`): every sentence starts with `[calm]` and ends with `...`, numbers are spelled as words, and nothing else is tagged.

```bash
python3 ../../scripts/qwen-say.py narration.txt --voice sawi --check
bash ../../scripts/narrate.sh narration.txt sawi . --lang id --engine qwen --opening 6
```

- **Voice:** `sawif-plus` for the six paragraphs of the cold open, `sawif` for the four chapters.
- **Pace:** the voice speaks 125 words a minute (118 with the half-second between paragraphs), slower than the 130–145 the style measured. The script was first voiced at 62 paragraphs (9 min 55 s with the cards); eight paragraphs were then cut to bring the film under nine minutes. Their takes were paid for and are not used. No kept paragraph was spoken again.
- **Cost:** the estimate was US$0.114 for 7,595 characters at the flash rate; the script has no price for the plus model, which spoke the cold open.
- **Nobody has listened to it.** The checks are a transcript comparison and a silence test, not ears. Listen first to the segments where the transcript differs from the text, all of them foreign names or rare words: `s03` ("Tabib Paracelsus" was heard as "Tapi para Celsius"), `s08` ("Suci"), `s14` ("Bundschuh" heard as "Bunshu"), `s21` and `s39` ("Saverne" heard as "Savern" and "Safern"; the first take of `s21` was retaken for that word), `s23` ("khotbah"), `s38` ("genderang" heard as "jenderang"), `s42` ("mereda" heard as "meredas"). The first takes of `s29` and `s38` had a silence of more than two seconds and were retaken by the script; no take is marked as failed.

## Shape

| Part | Time | Length |
|---|---|---|
| Disclaimer card (`disclaimer.html`), music only | 0 s | 8.0 s |
| Cold open, 6 beats | 8.0 s | 54.8 s |
| Sting (`sting.html`) | 62.8 s | 8.0 s |
| Chapter card 1 (`c1-card.html`) and chapter 1, 12 beats | 70.8 s | 4.0 s + 106.0 s |
| Chapter card 2 (`c2-card.html`) and chapter 2, 12 beats | 180.8 s | 4.2 s + 105.6 s |
| Chapter card 3 (`c3-card.html`) and chapter 3, 12 beats | 290.6 s | 4.2 s + 103.9 s |
| Chapter card 4 (`c4-card.html`) and chapter 4, 12 beats | 398.7 s | 4.2 s + 114.3 s |
| Sources (`sources.html`), rolling at 38 px a second | 517.2 s | 17.0 s |
| `gate.html` | whole film | 534.2 s |

The cold open's four questions are answered in chapter order: what kind of city (chapter 1), how it spread (chapter 2), why the council ordered more dancing (chapter 3), how it is explained today (chapter 4).

141 shots in 485 s of narrated film: median 3.4 s, shortest 1.5 s, longest 8.3 s (the humours diagram). 127 are stills, 6 are stock film, 8 are maps and diagrams. Eight red names and years (1518, Frau Troffea, 1262, Bundschuh, Santo Vitus, 1374, 1832, Ergot) and five red figures (142 meter, sekitar 14 Juli 1518, 400, 15 kematian sehari, sekitar 7 pekan): about one and a half a minute. 25 captions, 4 lower thirds, 3 framed portraits, 5 print cards with a marker, 1 page capture with a marker, 4 maps, 4 diagram shots.

## Scenes

Every beat is one paragraph of `narration.txt` and one composition. Cuts are word times from `assets/transcript.json`; a red name is on screen from the first frame of its shot.

**Cold open** (8.0–62.8 s)

| Scene | Starts on | Shows |
|---|---|---|
| `co-01-july.html` | "Pada bulan Juli seribu" (8.00 s) | Strasbourg in the Nuremberg Chronicle (1493), the red name 1518; Hondius, the footbridge plate (1642), captioned on "seorang" (11.64 s) |
| `co-02-spread.html` | "Ia menari berhari-hari Dalam" (15.94 s) | stock film: candles burning in the dark; Hondius, the procession plate (1642) on "Dalam" (18.21 s) |
| `co-03-troffea.html` | "Tabib Paracelsus, yang datang" (24.68 s) | Hollar's Rhine toll bridge near Strasbourg (1629), framed portrait of Paracelsus; Strasbourg in the Nuremberg Chronicle (1493), the red name Frau Troffea on "Frau" (30.57 s) |
| `co-04-council.html` | "Dewan kota memanggil para" (32.38 s) | the surgeons of Brunschwig's Cirurgia (Strasbourg, 1497); Hondius, the bagpipers plate (1642) on "menyewa" (34.84 s); Saint Vitus in the kettle (carving, about 1500) on "pertolongan" (36.55 s); the Braun and Hogenberg plan of Strasbourg (1572) on "Peristiwa" (38.19 s) |
| `co-05-questions.html` | "Kota seperti apa yang" (43.04 s) | Strasbourg in the Nuremberg Chronicle (1493); Hondius, the footbridge plate (1642) on "Bagaimana" (46.40 s); Burgkmair's cart of singers (about 1517) on "Mengapa" (48.80 s); the Hirschvogel portrait of Paracelsus (1540, reproduction) on "Dan" (53.22 s) |
| `co-06-invite.html` | "Bersama Ruang Dokumenter, mari" (56.50 s) | Hondius, the footbridge plate (1642) |

**Chapter 1, STRASBOURG MENJELANG WABAH: KELAPARAN DAN KETAKUTAN** (74.8–180.8 s)

| Scene | Starts on | Shows |
|---|---|---|
| `c1-01-city.html` | "Strasbourg saat itu adalah" (74.80 s) | the map of Europe with Strasbourg on the Rhine; Dürer's Four Horsemen (1498) on "yang" (79.73 s) |
| `c1-02-empire.html` | "Kota ini bagian dari" (83.22 s) | Dürer's Maximilian I (1519), lower third Maximilian I; the Braun and Hogenberg plan of Strasbourg (1572) on "tetapi" (86.67 s); Strasbourg in the Nuremberg Chronicle (1493), the red name 1262 on "sejak" (88.48 s) |
| `c1-03-cathedral.html` | "Di tengahnya berdiri katedral" (91.56 s) | Hollar's Strasbourg Cathedral (1630); Hollar's Strasbourg Cathedral (1630), the red figure 142 meter on "menara" (94.34 s); Hollar's Strasbourg Cathedral (1630) on "yang" (97.40 s) |
| `c1-04-brant.html` | "Sekretaris kotanya adalah Sebastian" (101.34 s) | Burgkmair's Sebastian Brant (about 1508), lower third Sebastian Brant; the preface page of Das Narrenschiff (Basel, 1494), captioned on "penulis" (104.81 s) |
| `c1-05-hardship.html` | "Namun tahun-tahun sebelum wabah" (108.16 s) | Dürer's Four Horsemen (1498); Bening's August calendar page (about 1515), captioned on "Sejarawan" (112.88 s); Bruegel's Hunters in the Snow (1565) on "musim" (115.81 s); Dürer's Peasant Couple at Market (1519) on "dan" (117.62 s) |
| `c1-06-gates.html` | "Ribuan petani dan penggarap" (121.46 s) | Dürer's Three Peasants in Conversation (about 1497); Strasbourg in the Nuremberg Chronicle (1493) on "datang" (125.53 s) |
| `c1-07-disease.html` | "Penyakit ikut menekan Kusta" (128.68 s) | the dance of the dead, Nuremberg Chronicle (1493); the dance of the dead, Nuremberg Chronicle (1493) on "Kusta" (130.85 s); the woodcut of Brant's syphilis broadsheet (1496), captioned on "dan" (133.88 s) |
| `c1-08-bundschuh.html` | "Di pedesaan sekitarnya, para" (136.54 s) | Weiditz's men pushing a cart (1531); the Bundschuh pamphlet's title woodcut (1514), the red name Bundschuh on "dalam" (142.22 s) |
| `c1-09-omens.html` | "Bagi orang masa itu" (144.72 s) | stock film: clouds crossing the sky; Dürer's Opening of the Fifth and Sixth Seals (about 1497) on "bencana" (146.74 s); Dürer's Opening of the Fifth and Sixth Seals (about 1497) on "Kelaparan" (149.03 s) |
| `c1-10-vitus.html` | "Salah satu yang paling" (152.58 s) | the Martyrdom of Saint Vitus (panel, about 1450); Saint Vitus in the kettle (carving, about 1500), the red name Santo Vitus on "Santo" (155.08 s); the Martyrdom of Saint Vitus (panel, about 1450) on "setempat" (157.16 s); the dancers of Kölbigk (Gottfried's chronicle, 1674), captioned on "mengutuk" (158.75 s) |
| `c1-11-1374.html` | "Kutukan itu punya riwayat" (162.44 s) | the dancers and the priest, Nuremberg Chronicle fol. 217r (1493); Hollar's Rhine near Strasbourg (1635), the red name 1374 on "Pada" (164.79 s); the map of the Rhine towns (Utrecht, Aachen, Cologne, Trier, Metz, Strasbourg) on "wabah" (167.40 s) |
| `c1-12-record.html` | "Sejak itu, menurut Waller" (171.02 s) | the dancers on a bridge, Nuremberg Chronicle fol. 188r (1493), captioned; Brueghel the Younger's dancers of Molenbeek (1592) on "kawasan" (173.63 s); Strasbourg in the Nuremberg Chronicle (1493) on "Wabah" (177.57 s) |

**Chapter 2, DARI SATU PENARI MENJADI RATUSAN** (185.0–290.6 s)

| Scene | Starts on | Shows |
|---|---|---|
| `c2-01-begins.html` | "Menurut John Waller, semuanya" (184.96 s) | the Braun and Hogenberg plan of Strasbourg (1572); Hollar's Strasbourg from the Ill (about 1630), the red figure sekitar 14 Juli 1518 on "sekitar" (187.71 s); the Pfalz in Morant's plan of 1548 (copy), captioned on "Frau" (189.78 s) |
| `c2-02-third-day.html` | "Keesokan paginya ia menari" (193.70 s) | Dürer's Peasant Couple Dancing (1514); Spindler's imagined scene of 1518 (1893), captioned on "Pada" (198.09 s) |
| `c2-03-saverne.html` | "Tarian itu berlangsung empat" (201.88 s) | Hondius, the footbridge plate (1642); Weiditz's men pushing a cart (1531) on "Pemerintah" (206.25 s); the map of the road from Strasbourg to Saverne, the route drawing on "Saverne" (208.80 s); the Saint Vitus chapel near Saverne (Müller, about 1820), captioned on "tempat" (212.39 s) |
| `c2-04-too-late.html` | "Tetapi sudah terlambat Dalam" (214.78 s) | Hondius, the procession plate (1642); Hondius, the footbridge plate (1642) on "Dalam" (216.96 s) |
| `c2-05-sources.html` | "Kita mengetahuinya dari catatan" (220.80 s) | Brant's dance ban of 2 August 1518, captioned; stock film: the nave of a gothic church on "khotbah" (223.97 s); the Braun and Hogenberg plan of Strasbourg (1572) on "Salah" (226.47 s); the Braun and Hogenberg plan of Strasbourg (1572), framed portrait of Daniel Specklin on "arsitek" (230.02 s) |
| `c2-06-poem.html` | "Sebuah syair dalam kronik" (233.30 s) | page 7 of Die Tanzwuth with the chronicle's verse; page 7 of Die Tanzwuth with the chronicle's verse, captioned on "ratusan" (237.93 s) |
| `c2-07-imlin.html` | "Kronik keluarga Imlin mencatat" (245.16 s) | the chart of dancers by the chronicles: 1, more than 30, 400 |
| `c2-08-deaths.html` | "Satu kronik bahkan menyebut" (253.10 s) | the dance of the dead, Nuremberg Chronicle (1493); Strasbourg in the Nuremberg Chronicle (1493), the red figure 15 kematian sehari menurut satu kronik on "lima" (257.02 s) |
| `c2-09-debate.html` | "Angka-angka ini masih diperdebatkan" (259.52 s) | Brant's dance ban of 2 August 1518; the chart again with the band of 50 to 200 dancers on "Sejarawan" (262.06 s) |
| `c2-10-no-deaths.html` | "Para penulis yang hidup" (269.38 s) | Dürer's drawing of Sebastian Brant (about 1520), lower third Sebastian Brant; de Bry's Daniel Specklin, captioned on "Kisah" (274.64 s) |
| `c2-11-illness.html` | "Yang disepakati para saksi" (279.16 s) | Hondius, the footbridge plate (1642); Hondius, the procession plate (1642) on "tarian" (282.70 s) |
| `c2-12-must-cure.html` | "Dan bagi dewan kota" (286.46 s) | the Lucerne council in Schilling's chronicle (1513), captioned |

**Chapter 3, RESEP DEWAN KOTA: LEBIH BANYAK TARIAN** (294.8–398.7 s)

| Scene | Starts on | Shows |
|---|---|---|
| `c3-01-physicians.html` | "Menurut Waller, para tabib" (294.80 s) | the surgeons of Brunschwig's Cirurgia (Strasbourg, 1497); the zodiac man of the Margarita philosophica (1503), captioned on "penjelasan" (297.22 s) |
| `c3-02-hot-blood.html` | "Mereka menyebutnya penyakit alami" (300.74 s) | Gersdorff's bloodletting figure (Strasbourg, 1530 edition); Gersdorff's bloodletting figure (Strasbourg, 1530 edition) on "akibat" (303.56 s); Dürer's Peasant Couple Dancing (1514) on "Obatnya" (306.97 s) |
| `c3-03-humours.html` | "Bagi kedokteran masa itu" (310.52 s) | the diagram of the four humours, blood filling red |
| `c3-04-halls.html` | "Maka dewan mengosongkan dua" (318.86 s) | the Braun and Hogenberg plan of Strasbourg (1572); Amman's carpenter (1568), captioned on "lalu" (323.07 s) |
| `c3-05-musicians.html` | "Para penari dikumpulkan di" (326.96 s) | Hondius, the procession plate (1642); Burgkmair's cart of horn players (about 1517), captioned on "Dewan" (330.26 s); Beham's dancing peasant couple (1522) on "penari" (332.32 s) |
| `c3-06-backfire.html` | "Hasilnya berlawanan dengan harapan" (335.86 s) | Dürer's Bagpiper (1514); Hondius, the procession plate (1642) on "Menurut" (338.24 s); the Martyrdom of Saint Vitus (panel, about 1450) on "bahwa" (342.35 s); Hondius, the footbridge plate (1642) on "dan" (344.02 s) |
| `c3-07-reversal.html` | "Dewan pun berbalik arah" (347.40 s) | the Lucerne council in Schilling's chronicle (1513); Amman's carpenter (1568) on "Panggung" (350.12 s); Hondius, the bagpipers plate (1642) on "dan" (351.61 s) |
| `c3-08-ban.html` | "Larangan itu dicatat oleh" (356.06 s) | Brant's dance ban of 2 August 1518, captioned; Bruegel's Wedding Dance (about 1566), captioned on "Pesta" (360.21 s); the dancers and the priest, Nuremberg Chronicle fol. 217r (1493) on "alat" (362.33 s); Dürer's piper and drummer (about 1504) on "tidak" (363.89 s) |
| `c3-09-wagons.html` | "Kini penyakit itu ditangani" (366.24 s) | stock film: the nave of a gothic church; Weiditz's men pushing a cart (1531) on "Para" (369.96 s); Merian's view of Saverne (1644), captioned on "dibawa" (372.76 s) |
| `c3-10-grotto.html` | "Di sana, di sebuah" (374.50 s) | the Saint Vitus chapel near Saverne (Müller, about 1820); the grotto of Saint Vitus (lithograph, 1828), captioned on "para" (377.63 s); the carved Saint Vitus group from the Saverne chapel (1518), captioned on "patung" (379.45 s) |
| `c3-11-red-shoes.html` | "Tangan mereka diberi salib" (381.72 s) | the grotto of Saint Vitus (lithograph, 1828); Hondius, the procession plate (1642) on "Kaki" (384.15 s); stock film: candles burning in the dark on "yang" (386.92 s) |
| `c3-12-september.html` | "Pada awal September, wabah" (390.62 s) | Hollar's Strasbourg from the Ill (about 1630); Hollar's Rhine near Strasbourg (1635), the red figure sekitar 7 pekan on "sekitar" (394.05 s) |

**Chapter 4, PENJELASAN: DARI KUTUKAN SAMPAI PENYAKIT PSIKOGENIK MASSAL** (402.9–517.2 s)

| Scene | Starts on | Shows |
|---|---|---|
| `c4-01-why.html` | "Mengapa begitu banyak orang" (402.88 s) | Hondius, the procession plate (1642); the timeline of five centuries of explanations on "Selama" (406.46 s) |
| `c4-02-curse.html` | "Bagi warga Strasbourg, jawabannya" (410.82 s) | the Braun and Hogenberg plan of Strasbourg (1572); Saint Vitus in the kettle (carving, about 1500) on "kutukan" (415.28 s); the grotto of Saint Vitus (lithograph, 1828) on "Karena" (417.51 s) |
| `c4-03-paracelsus.html` | "Paracelsus termasuk yang pertama" (420.84 s) | the Hirschvogel portrait of Paracelsus (1540, reproduction), lower third Paracelsus; the Paracelsus portrait after Matsys on "mencari" (423.16 s) |
| `c4-04-three-kinds.html` | "Ia membagi penyakit menari" (427.10 s) | page 18 of Die Tanzwuth, captioned; page 18 of Die Tanzwuth on "yang" (430.75 s) |
| `c4-05-hecker.html` | "Pada tahun seribu delapan" (434.80 s) | the title page of Hecker's Die Tanzwuth (1832), the red name 1832; the dancers of Kölbigk (Gottfried's chronicle, 1674), framed portrait of Justus Hecker on "dokter" (438.91 s); the title page of Hecker's Die Tanzwuth (1832) on "dan" (443.04 s) |
| `c4-06-ergot.html` | "Pada abad kedua puluh" (445.54 s) | stock film: sunlight through the branches of trees; the ergot plate of Köhler's Medizinal-Pflanzen (1890), the red name Ergot, captioned on "ergot" (448.60 s); Gersdorff's Saint Anthony with a victim of Saint Anthony's fire (1517), captioned on "yang" (451.88 s) |
| `c4-07-not-ergot.html` | "Waller menolak dugaan itu" (454.76 s) | Gersdorff's Saint Anthony with a victim of Saint Anthony's fire (1517); Gersdorff's bloodletting figure (Strasbourg, 1530 edition) on "Keracunan" (457.55 s); Dürer's Peasant Couple Dancing (1514) on "sehingga" (461.21 s) |
| `c4-08-psychogenic.html` | "Penjelasan Waller sendiri adalah" (464.62 s) | the Europe PMC record of Waller's Lancet article; Hondius, the procession plate (1642) on "gangguan" (468.60 s); Hondius, the footbridge plate (1642) on "keyakinan" (471.29 s) |
| `c4-09-belief.html` | "Warga yang lapar, sakit" (474.40 s) | Dürer's Three Peasants in Conversation (about 1497); the Martyrdom of Saint Vitus (panel, about 1450) on "percaya" (477.77 s); Ostendorfer's pilgrimage to Regensburg (about 1520), captioned on "Menurut" (481.33 s) |
| `c4-10-trance.html` | "Dalam keadaan trans, rasa" (486.34 s) | Ostendorfer's pilgrimage to Regensburg (about 1520); Hondius, the footbridge plate (1642) on "Itu" (489.85 s); Hondius, the footbridge plate (1642) on "menari" (492.63 s) |
| `c4-11-tanganyika.html` | "Waller membandingkannya dengan wabah" (495.08 s) | the Europe PMC record of Waller's Lancet article; the map of East Africa with Kashasha on "Tanganyika" (497.92 s) |
| `c4-12-agreed.html` | "Belum ada penjelasan yang" (504.86 s) | the title page of Hecker's Die Tanzwuth (1832); Brant's dance ban of 2 August 1518 on "Tetapi" (508.92 s); Hondius, the footbridge plate (1642) on "pada" (512.43 s) |

## Look

Palette: card ground `#111517`, chapter ground `#0c0f10`, white `#e9edea`, the red `#d13c22` (labels `rgba(198,58,36,0.92)`, on dark `#f04a2e`), disclaimer heading `#c56a3c` on a blue glow `#1d4159`, marker yellow `rgba(255,214,64,0.55)`, map land `#c9792f`, sea `#3f7f9f`, borders `#2a2118`, diagram ground `#111517` with lines `#e9eae2` and the red `#e0482c`.

- **The grade** is the reference's JSON on every still and clip. A shot that carries a white caption over a pale print adds `exposure` −0.25 to it, so the picture is a mid-tone and the caption can be read. Book pages and the page capture take the lighter page grade.
- **The maps and the diagrams** are drawn in HTML and are not graded. No big red name sits on a map: red on the orange land measured 1.07:1.
- **Stills** start at scale 1.014 and drift a few pixels while they are pushed, so no edge shows.

## Sound

One piece of music runs under the whole film and is never off: nine minutes of "Dystopian Synthwave Playlist - Planet Zero" by Karl Casey @ White Bat Audio (https://www.youtube.com/watch?v=WH2vlG0b2CM), the mix the user chose, taken from 22:44 of the mix: "Black Lotus", "Quantum Leap", and into "More Human Than Human". `make-footage.sh` downloads that section only and joins the two tracks over the silence between them with a 6 s crossfade; nothing is looped. The mix is free to use with the credit "Karl Casey @ White Bat Audio"; on YouTube the credit must be placed in the video's description, so an upload of this film has to carry it there. The credit is in `CREDITS.md` and in the sources roll.

Under the narrator the music is turned down, and it rises under the disclaimer, the sting, the chapter cards, and the sources:

```bash
python3 ../../scripts/duck-under-voice.py index.html --floor-under 13 --open 9 --fade-out 2.5
python3 ../../scripts/duck-under-voice.py index.html --mix renders/draft.mp4
```

The first command follows the music's level as it goes, so the tracks sit at one heard level under the voice. The second, on the last draft render (renders are not kept in this folder, so render and measure again), reported the music 13.5 dB under the speaking voice on average, inside the 12–15 dB measured on the source films:

| Stretch | Music floor | Speaking level | Music under the voice |
|---|---|---|---|
| Cold open, 8.0–62.5 s | −32.3 dB | −18.5 dB | 13.9 dB |
| Chapter 1, 74.8–180.5 s | −32.5 dB | −19.2 dB | 13.3 dB |
| Chapter 2, 185.0–290.3 s | −32.6 dB | −18.6 dB | 14.0 dB |
| Chapter 3, 294.8–398.4 s | −32.5 dB | −19.1 dB | 13.3 dB |
| Chapter 4, 402.9–516.7 s | −32.1 dB | −19.1 dB | 13.0 dB |

Music alone: −21.6 to −22.9 dB in the sting, at the chapter cards, and under the sources; −28.1 dB under the disclaimer, where "Black Lotus" opens softly. Video and audio are both 534.2 s. Nobody has listened to the film.

## Facts in the film and where they come from

I read these sources for this film: John Waller's article in The Psychologist (2009), the published summaries of his articles in Endeavour (2008) and The Lancet (2009), Ned Pennant-Rea's essay in The Public Domain Review (2018), which follows Waller's book, the scan of Justus Hecker's Die Tanzwuth (1832), and the Wikipedia articles named below. I did not read Waller's book itself; where the film says "menurut Waller", the claim reached me through the texts above.

- **July 1518, a woman dances in a street of Strasbourg; more than thirty join within a week; four to six days; taken by wagon to Saverne**: Waller (Endeavour summary: 34 others within a week) and Pennant-Rea. The film says "sekitar tanggal empat belas Juli": sources give 12, 14, and 15 July.
- **The name Frau Troffea comes from Paracelsus, who came eight years later**: Pennant-Rea. Paracelsus's view that she danced to shame her husband is from the same essay; the paragraph that said so was cut for length.
- **Free imperial city since 1262, cathedral spire of 142 m finished in 1439, Sebastian Brant as city secretary**: Wikipedia, "History of Strasbourg", "Strasbourg Cathedral", "Sebastian Brant".
- **Failed harvests, hard winters, the highest bread prices in a generation, thousands of starving farmers and vine growers at the gates, leprosy, plague, and the new disease syphilis**: Waller, The Psychologist.
- **The Bundschuh conspiracies** (1493 to 1517 on the Upper Rhine): Wikipedia, "Bundschuh movement".
- **Saint Vitus as the saint who could curse people to dance; the outbreak of 1374 in dozens of towns along the Rhine; about ten outbreaks before 1518**: Waller, The Psychologist; Pennant-Rea ("more than ten").
- **The verse about hundreds dancing in the market, the lanes, and the streets, day and night**: read on page 7 of Hecker (1832), who quotes it from Schilter's edition of the Königshoven chronicle (Strasbourg, 1698). The film shows that page.
- **400 dancers within a month (the Imlin family chronicle); 15 deaths a day (one chronicle)**: Pennant-Rea and the Lancet summary. Both are attributed on screen and in the narration.
- **50 to 200 dancers in the contemporary sources, and no contemporary author recording a death**: the historian Élisabeth Clementz (2018), as cited by the French Wikipedia article "Épidémie dansante de 1518". I did not read Clementz herself.
- **The physicians ruled out the stars and the supernatural and named a natural disease of overheated blood; the council cleared two guildhalls and the grain market, built a stage opposite the horse market, and paid musicians and dancers**: Waller, Endeavour summary.
- **The stages pulled down, music and dance banned until September, stringed instruments still allowed at weddings but not tambourines and drums (Brant)**: Pennant-Rea. The ban itself, in Brant's hand and dated 2 August 1518, is the document shown in chapter 3.
- **The shrine: a grotto chapel above Saverne, a wooden carving of Vitus, small crosses, red shoes sprinkled with holy water and marked with crosses in oil; the stream of pilgrims thinning within a week; the end by early September**: Pennant-Rea; Wikipedia, "Dancing plague of 1518".
- **Paracelsus's three kinds of Saint Vitus's dance (from imagination, from desire, from the body)**: read on page 18 of Hecker (1832), which the film shows with the three names marked.
- **Hecker's book of 1832 and its name for the disease**: its title page, shown.
- **Ergot as a suspect of the twentieth century, and Waller's objection that it cuts the blood supply to the limbs**: Pennant-Rea; Wikipedia, "Dancing mania".
- **Mass psychogenic illness, belief in the curse, trance, and the comparison with the laughter epidemic in Tanganyika**: Waller, The Psychologist (he dates it 1962; Pennant-Rea writes 1963; the film says "awal tahun enam puluhan").
- **No explanation agreed by all; the sources agree that people danced**: the French and English Wikipedia articles.

## What each picture really is

No picture of the dancers of 1518 made in 1518 is known. The film says what its pictures are in captions, and here.

- **The dancers** in most shots are from three engravings by Hendrick Hondius (1642) after a drawing by Pieter Bruegel the Elder of 1564: pilgrims with the "dancing sickness" on their way to Molenbeek near Brussels, forty-six years after Strasbourg and four hundred kilometres away. The first of these shots carries the caption; later ones do not repeat it. Pieter Brueghel the Younger's painting of the same scene (1592) is shown once.
- **Charles Spindler's drawing** (1893) is an artist's idea of the scene 375 years later; its caption says so.
- **The dancing couples** by Dürer (1514) and Beham (1522) are peasants at a feast, not sick people. **Bruegel's Wedding Dance** (about 1566) is shown under the sentence about weddings.
- **The two coloured woodcuts of dancers** are from the Nuremberg Chronicle (1493), fol. 188r and 217r. The second, with the priest and the broken bridge, fits the old story of the dancers on the bridge at Maastricht; I did not read the page's text, so the caption says only "para penari di atas jembatan". **The dancers of Kölbigk** are an engraving of 1674 of a legend set about 1020.
- **Strasbourg** is seen in the Nuremberg Chronicle (1493), in Braun and Hogenberg's plan (1572), in a copy of Conrad Morant's plan of 1548, and in three etchings by Wenceslaus Hollar of about 1630. None is from 1518.
- **The town council** in two shots is the council of Lucerne in Diebold Schilling's chronicle (1513); the caption says so. No picture of Strasbourg's council was found.
- **The physicians** are the surgeons of Hieronymus Brunschwig's Cirurgia (Strasbourg, 1497); **the bloodletting figure** is from the 1530 Strasbourg edition of Hans von Gersdorff's Feldtbuch (first printed there in 1517); **the zodiac man** is from Gregor Reisch's Margarita philosophica (1503).
- **The musicians** are bagpipers by Hondius and Dürer, Burgkmair's carts of singers and horn players from the Triumphal Procession of Maximilian (the colouring of one is from 1765), and the drummer of Dürer's Jabach altarpiece. None shows the players hired in Strasbourg.
- **The wagon** is Hans Weiditz's woodcut of men pushing a cart uphill (1531).
- **Saint Vitus** is a South German carving of about 1500 and a panel of about 1450. The carved group of about 1518 from the Saverne chapel (workshop of Hans Wydyz) is shown when the narration reaches the carving; whether it is the carving the dancers stood under, I do not know. The chapel is a drawing of about 1820 and the grotto a lithograph of 1828.
- **Paracelsus** is a seventeenth-century copy of a presumed portrait after Quentin Matsys, and a 1927 reproduction of Hirschvogel's etching of 1538. **Sebastian Brant** is Burgkmair's painting (about 1508) and Dürer's drawing (about 1520). **Daniel Specklin** is an engraving by Théodore de Bry in a reprint of 1791.
- **Hardship** is shown with Dürer's Four Horsemen (1498) and Fifth and Sixth Seals, a harvest page from a Flemish book of hours (about 1515), Bruegel's Hunters in the Snow (1565), Dürer's peasants, the dance of the dead from the Nuremberg Chronicle, and the woodcut of Brant's syphilis broadsheet (1496). They show the fears of the time, not Strasbourg in 1517.
- **Ergot** is a botanical plate of 1890; the victim of Saint Anthony's fire is Gersdorff's woodcut of 1517.
- **The page capture** is the Europe PMC record of Waller's Lancet article, which has no abstract; the marker runs over its title.
- **Stock film:** candles (two shots), the nave of a gothic church (two shots, not Strasbourg Cathedral), clouds, and sunlight through trees.
- **The maps** use today's borders. In 1518 Strasbourg and Saverne lay in the Holy Roman Empire. The course of the Rhine is drawn from sixteen points and is a sketch. Lake Victoria is not drawn on the map of East Africa.
- **The chart** joins three reported counts with straight lines; nobody counted the dancers day by day. **The diagram of the four humours** is the textbook scheme, not a drawing from 1518.

## Gaps

- Nobody has listened to the narration or the mix.
- The script is 953 words, under the 1,150 to 1,250 first planned, because this voice is slower than the style's source. Chapter 4 runs 114 s, four seconds over the planned ceiling.
- Hecker printed the Strasbourg verse under the year 1418, which later writers take for a slip for 1518. The page is shown as he printed it, and "1418" is in view above the verse.
- Clementz's figures and Waller's book reached this film through other texts, as said above.
- Three pictures are 960 px wide at the source and are enlarged twice (the syphilis woodcut, Saint Anthony, Dürer's Brant); Spindler's drawing and the carved group are enlarged 2.4 and 2 times. They are soft.
- Eight Commons pictures are CC BY 4.0 (seven from the Wellcome Collection, one photograph by Sophie Guillot de Suduiraut); the rest are public domain or CC0. The Europe PMC screenshot is a web page shown as a citation, not open-licence material; it is recorded in `CREDITS.md` for the user's legal review.
- `CREDITS.md` ends with a takedown notice that holds a placeholder in place of the address: the address to publish must come from the user.
