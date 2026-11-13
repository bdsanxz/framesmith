# Pictures: photos, b-roll, icons

Sourced on 2026-10-09 for the recut of the commentary on President Prabowo's speech at the DPR plenary session.
Every file has a stated source page and licence. Nothing was taken from a news site.

Where each kind is recorded:

- Photos: `sources.jsonl` (one row per download, one `from` row per crop made with `scripts/made.py`).
- B-roll: `shots.json` (source and in-point per clip; `credit` and `license` in the row where the source is not a search candidate). YouTube details are in `.footage/meta/`, Mixkit and Commons candidates in `.assets/search/`. Keep both folders until `CREDITS.md` is final.
- Icons: one line in `credits-extra.jsonl`.

## 1. Photos (`assets/pic/`)

Portraits are 800x1000, the others 1920x1080. The untouched downloads are in `assets/pic/src/`.

| File | Shows | Source page | Licence | Note |
|---|---|---|---|---|
| `prabowo.jpg` | Prabowo Subianto, official presidential portrait, 2024 | [Commons: Prabowo Subianto 2024 official portrait (3x4 cropped)](https://commons.wikimedia.org/wiki/File:Prabowo_Subianto_2024_official_portrait_(3x4_cropped).jpg), Ministry of State Secretariat | Public domain (Indonesian government work) | A tight headshot: the face fills the frame, so it sits in the middle, not the upper middle. See "Rejected" for why the wider file was not used. |
| `soekarno.jpg` | Soekarno speaking into a bank of microphones before a large crowd, 1950s | [Commons: Sukarno giving a speech, Bung Karno Penjambung Lidah Rakjat 242](https://commons.wikimedia.org/wiki/File:Sukarno_giving_a_speech,_Bung_Karno_Penjambung_Lidah_Rakjat_242.jpg), uncredited, from the 1966 book | Public domain | A scan of a printed page: the halftone dots are visible at this size. He is in profile. The searches found no podium photo with an open licence; this is the closest. |
| `soekarno-portrait.jpg` | Soekarno, official portrait in white uniform | [Commons: Presiden Sukarno](https://commons.wikimedia.org/wiki/File:Presiden_Sukarno.jpg), anonymous | Public domain | Clean alternative to the speech photo. |
| `soeharto.jpg` | Soeharto, official portrait, about 1987 | [Commons: Official portrait of Soeharto (1980s)](https://commons.wikimedia.org/wiki/File:Official_portrait_of_Soeharto_(1980s).jpg), Government of Indonesia | Public domain | Scan of a government poster; fine print grain. |
| `sby.jpg` | Susilo Bambang Yudhoyono, official presidential portrait, 2004 | [Commons: Susilo Bambang Yudhoyono, official presidential portrait (2004)](https://commons.wikimedia.org/wiki/File:Susilo_Bambang_Yudhoyono,_official_presidential_portrait_(2004).jpg), Government of Indonesia | Public domain | Scanned from a government guidebook; a small speck on the forehead is in the source. |
| `trump.jpg` | Donald Trump, official portrait, January 2025 | [Commons: January 2025 Official Presidential Portrait of Donald J. Trump](https://commons.wikimedia.org/wiki/File:January_2025_Official_Presidential_Portrait_of_Donald_J._Trump.jpg), Daniel Torok | Public domain (US federal government work) | The inaugural portrait, not the later June 2025 one. |
| `dpr-building.jpg` | The green-domed Nusantara building of the DPR/MPR complex, Jakarta | [Commons: MPRDPRBuilding](https://commons.wikimedia.org/wiki/File:MPRDPRBuilding.jpg), Davidelit, 2008 | Public domain (released by the author) | Taken in 2008, hazy white sky. |
| `plenary-hall.jpg` | The main plenary hall of the Nusantara building, full, seen from the balcony | [Commons: Ruang MPR](https://commons.wikimedia.org/wiki/File:Ruang_MPR.jpg), U.S. Department of State | Public domain (US federal government work) | Taken on 20 October 2014 at an MPR session (the Jokowi inauguration). The screen in the hall reads "Selamat Datang Anggota MPR ... 2014". Not the May 2026 session. |
| `plenary-session.jpg` | A DPR plenary session in the Nusantara II chamber, leadership table and Garuda | [Commons: Sidang Paripurna DPR ke-9 2015](https://commons.wikimedia.org/wiki/File:Sidang_Paripurna_DPR_ke-9_2015.jpg), Andylala Waluyo (VOA) | Public domain (Voice of America) | Taken on 30 October 2015; the screen in the hall shows that date. Many seats are empty. |
| `rupiah.jpg` | A fan of rupiah banknotes, 2,000 to 20,000 | [Commons: Uang rupiah kertas](https://commons.wikimedia.org/wiki/File:Uang_rupiah_kertas.jpg), Pak.syafda | CC BY-SA 4.0 | Small denominations only, on a grey carpet. Share-alike. |
| `gallery-frames.jpg` | A museum room hung with portraits in gilded frames | [Commons: Gallery of Polish Painting ..., National Museum in Kielce, SOULinPIX 092](https://commons.wikimedia.org/wiki/File:Gallery_of_Polish_Painting_and_European_Decorative_Arts,_National_Museum_in_Kielce,_czerwiec_2025,_SOULinPIX_092.jpg), Krzysztof Popławski | CC BY 4.0 | An empty room in Kielce, Poland. A green exit sign is visible. |
| `gallery-visitors.jpg` | Visitors looking at framed paintings, Rijksmuseum Gallery of Honour | [Commons: Rijksmuseum Amsterdam - Gallery of Honour Left 1.3](https://commons.wikimedia.org/wiki/File:Rijksmuseum_Amsterdam_-_Gallery_of_Honour_Left_1.3.jpg), Oursana | CC0 | Visitors are seen from behind; a face on the left of the original was cropped out. |
| `gilded-frame.jpg` | A painting in a wide gilded frame on a dark gallery wall, two visitors in front | Same file as `gallery-visitors.jpg` | CC0 | A tight crop, upscaled 1.3x, so slightly soft. The painting is a Rubens triptych (public domain). |
| `teleprompter.jpg` | A studio camera teleprompter with its script monitor | [Commons: Studio Teleprompter](https://commons.wikimedia.org/wiki/File:Studio_Teleprompter.jpg), Enchantmentvideo | CC BY-SA 4.0 | The script on the monitor is mirrored English text. Share-alike. |
| `teleprompter-glass.jpg` | A conference ("presidential") teleprompter glass on its stand | [Commons: Teleprompter presidenziale](https://commons.wikimedia.org/wiki/File:Teleprompter_presidenziale.jpg), Tlpitalia | CC BY 3.0 | A product photo on pure white; it cuts out easily. This is the kind used beside a podium. |
| `batavia-castle.jpg` | The Castle of Batavia with the market under palm trees, painted about 1662 | [Commons: Andries Beeckman - The Castle of Batavia](https://commons.wikimedia.org/wiki/File:Andries_Beeckman_-_The_Castle_of_Batavia.jpg), Rijksmuseum | Public domain | The top of the sky is cropped off. |
| `voc-ships-batavia.jpg` | Dutch ships at anchor before Batavia, painted 1640 to 1676 | [Commons: Gezicht op Batavia, Rijksmuseum SK-A-2513](https://commons.wikimedia.org/wiki/File:Gezicht_op_Batavia_Rijksmuseum_SK-A-2513.jpeg), Hendrick Dubbels, Rijksmuseum | Public domain | The top of the sky is cropped off. |
| `deli-plantation.jpg` | A European in white and a pith helmet directing a worker in a tobacco field, Deli, Sumatra, 1905 | [Commons: KITLV 88269, Kleingrothe, Harvesting tobacco ... Deli Company, 1905](https://commons.wikimedia.org/wiki/File:KITLV_-_88269_-_Kleingrothe,_C.J._-_Medan_-_Harvesting_tobacco_on_the_Medan_company_of_the_Deli_Company_in_Deli_-_1905.tif), C.J. Kleingrothe, KITLV collection (Leiden University Libraries) | Public domain | From KITLV, not the Tropenmuseum: the Tropenmuseum plantation photos on Commons are only 700 px wide. |

## 2. B-roll (`footage/b01.mp4` to `b15.mp4`)

All clips are 1920x1080, 30 fps, silent, one continuous shot each, with no added text, logo, or watermark in any frame of the review sheet (`.footage/shots-sheet.png`). Each file is 6.2 s: the 6 s slot plus the tool's 0.2 s handle. There are 15 clips, one more than asked, because the colonial section of the script is the longest.

| File | Shows | Source and licence | Limits |
|---|---|---|---|
| `b01.mp4` | A legislator standing at a chamber microphone, reading from notes, the public gallery behind him | YouTube [8PFLM3s_95o](https://www.youtube.com/watch?v=8PFLM3s_95o) "Senate Floor B Roll", channel M H, CC BY | **Visible faces** of real, identifiable people. It looks like a US state senate; the source does not name the chamber. Side view, not from the audience, and no podium. The uploader is an anonymous account, so the provenance is soft. A wall sign ("Senator ... Gallery") is in the scene. |
| `b02.mp4` | A small teleprompter on a tripod showing its script, a hand working the remote | YouTube [InvGGLiA2P4](https://www.youtube.com/watch?v=InvGGLiA2P4) "Mini Teleprompter Review", channel Conveyor of Randomness with Andre, CC BY | A desktop teleprompter, not a podium glass. English script text is readable on the screen (scene text). The source cuts to another shot soon after the clip ends: do not extend it. |
| `b03.mp4` | A theatre audience applauding, seen from behind, facing an empty stage | YouTube [h0ec0O7z8is](https://www.youtube.com/watch?v=h0ec0O7z8is), channel Royalty FREE Videos. Music and Sound Effects, CC BY | **Very dark**: silhouettes only, before any darkening in the edit. A stock re-upload channel, so the provenance is soft. |
| `b04.mp4` | A legislative chamber, wide: members at their desks, staff at the back | Same source as `b01` (at 31 s) | **Visible faces.** Not an Indonesian chamber: wood panelling and a US-style layout. Same soft provenance as `b01`. |
| `b05.mp4` | Hands counting a wad of US 100-dollar bills, very close | Mixkit [18296](https://mixkit.co/free-stock-video/man-counting-a-wad-of-bills-seen-very-closely-18296/), by arvizum, Mixkit Free License | **US dollars, not rupiah.** The tool flags three cuts; they are the bills flipping, the shot is continuous. |
| `b06.mp4` | A stock exchange price board: rows of numbers flickering yellow and red | YouTube [UcUGICKidOQ](https://www.youtube.com/watch?v=UcUGICKidOQ), channel Raw Footage Video, CC BY | A rendered board, not a real exchange and not an exchange-rate board. Stock re-upload channel, soft provenance. |
| `b07.mp4` | Walking through a traditional market in Indonesia behind a shopper, fruit and vegetable stalls | YouTube [zdX7mWoSQR0](https://www.youtube.com/watch?v=zdX7mWoSQR0) "Free Footage Video Stock Pasar Indonesia 01", channel deBurung, CC BY | The uploader's description says the footage belongs to Ferry Irwandi and was re-shared: **the CC BY tag comes from the re-uploader, not the author.** The shopper is seen from behind, no face. |
| `b08.mp4` | A container ship stacked with containers under blue gantry cranes | Mixkit [4011](https://mixkit.co/free-stock-video/cargo-ship-full-of-containers-4011/), by dubassy, Mixkit Free License | Not an Indonesian port. A lens flare sits at the bottom of the frame. A shipping line's name is on the containers (scene text). |
| `b09.mp4` | A museum wall of paintings in gilded frames, with marble busts | Wikimedia Commons [RIJKSMUSEUM Amsterdam 1st floor 104112](https://commons.wikimedia.org/wiki/File:RIJKSMUSEUM_Amsterdam_1st_floor_104112.webm), by Quahadi, CC BY-SA 4.0 (at 29 s) | Handheld and panning fast, with motion blur and a shift in colour as the camera adjusts. Share-alike. |
| `b10.mp4` | Archive, 1920s: a row of women picking tea on a plantation in Java | "De thee, van plantage tot het pakje (acte 1)", Willy Mullens / Haghe Film, Nederlands Instituut voor Beeld en Geluid, [Open Beelden 1480134](https://openbeelden.nl/media/1480134), Public Domain Mark 1.0 (also on [Commons](https://commons.wikimedia.org/wiki/File:De_thee,_van_plantage_tot_het_pakje_(acte_1)_-WM.webm)) (at 327 s) | Black and white, 4:3 source cropped to 16:9 and upscaled 1.33x: soft, with film grain. The film has Dutch intertitles between its shots; none is inside the clip. |
| `b11.mp4` | Visitors seated on a bench in front of a very large painting in a museum gallery | Same source as `b09` (at 40.5 s) | Handheld, moving forward and panning. Visitors are small and seen from the side or behind. Share-alike. |
| `b12.mp4` | Archive, 1920s: a European planter in a white suit and pith helmet walking through the tea fields toward the camera | Same film as `b10` (at 269.3 s) | Same softness. His face is visible by the end. 6.17 s long (one frame short of the handle; the 6 s slot is covered). An intertitle follows a few seconds after the clip: do not extend it. |
| `b13.mp4` | Painted fishing boats flying Indonesian flags, beached at a river mouth | YouTube [JuqqUtoRqlo](https://www.youtube.com/watch?v=JuqqUtoRqlo) "Kapal ikan di Laut Surut", channel GEONESIA, CC BY | A static shot, almost no movement. Boats stranded at low tide, not a working harbour. |
| `b14.mp4` | Jakarta from the air under a dusk sky, skyline on the horizon | YouTube [ggXWCnvgWVU](https://www.youtube.com/watch?v=ggXWCnvgWVU) "SENJA JAKARTA CITY, FREE FOOTAGE DRONE", channel WARTAWAN KESIANGAN, CC BY | **Soft source**: 720p, upscaled 1.5x. The skyline is distant and hazy. |
| `b15.mp4` | Archive, 1920s: a line of workers carrying baskets of tea on their heads past the plantation factory | Same film as `b10` (at 505 s) | Same softness. The camera pans; the workers are at the bottom edge and their feet are cut by the 16:9 crop. The factory sign ("1912 Santosa") is in the scene at the start. |

The source of `b09` and `b11` is kept whole in `footage/raw/rijksmuseum-1st-floor.webm` (4K, 97 s) for re-cutting.

## 3. Icons (`assets/icons/`)

28 outline icons from [Tabler Icons](https://github.com/tabler/tabler-icons) (page: https://tabler.io/icons), by Paweł Kuna and contributors, MIT licence. Each SVG was fetched from `icons/outline/<name>.svg` and edited to `stroke="#000000"` and `stroke-width="1.5"`, for use as a mask.

`arrow-right`, `book`, `briefcase`, `building-bank`, `calendar`, `cash`, `chart-line`, `check`, `clock`, `coin`, `currency-dollar`, `eye`, `flag`, `frame`, `heart-handshake`, `microphone`, `percentage`, `photo`, `presentation`, `question-mark`, `shield`, `ship`, `speakerphone`, `trending-up`, `user`, `users`, `world`, `x`.

Note on `frame.svg`: Tabler's "frame" is the design-tool frame (four crossing lines, like crop marks), not a picture frame. For a picture frame, `photo.svg` reads closer.

## 4. Not found with a usable licence

- **Rupiah banknotes being counted** (footage). `b05` shows US dollars instead; the rupiah still is `assets/pic/rupiah.jpg`.
- **A currency exchange board.** `b06` is a stock price board.
- **A speaker at a podium seen from the audience**, and **a podium teleprompter in use**. `b01` and `b02` are the closest.
- **An Indonesian parliament chamber** (footage). `b04` is a US state senate; the stills `plenary-hall.jpg` and `plenary-session.jpg` are the real rooms.
- **Hands framing or hanging a picture.** Nothing usable.
- **An oil rig.** The one Commons clip shows platforms as dots on the horizon.
- **Archive film of a colonial harbour or of colonial officials at work.** All three archive clips come from one plantation film; `b12` (the planter) is the nearest to an official. The other Open Beelden films of the Indies that I checked on Commons are pillarboxed 576p.
- **A Jakarta skyline at dusk in full HD.** `b14` is 720p.
- **A Soekarno podium photo** and **a Tropenmuseum plantation photo large enough for 1920 px** (see the photo table).

## 5. Rejected, and why

- Commons `Prabowo Subianto 2024 official portrait.jpg` (the uncropped original): its page had been altered when checked (the description named another person and a 2026 date). The crop of it with an intact page was used instead.
- Commons `Aerial views of a container terminal at the Port of Cork`: tagged CC BY 3.0, but its licence review is still pending and the YouTube original no longer shows a CC licence.
- Commons `Aerial view of Jakarta, 4 July 2024` (CC0): filmed through an aircraft window in haze, no skyline.
- Mixkit clips under the Mixkit Restricted License (non-commercial, 720p): most of Mixkit's podium, applause, parliament, port, and gallery clips.
- YouTube compilations of banknotes: dollars, euros, and rupees only, collected from other sites.

## 6. Before using the files

- **CC BY and CC BY-SA need attribution**; the two share-alike photos (`rupiah.jpg`, `teleprompter.jpg`) and the Rijksmuseum footage (`b09`, `b11`) carry CC BY-SA 4.0.
- **Soft provenance** on five YouTube clips (`b01`, `b03`, `b04`, `b06`, `b07`): the licence is the one the uploader states, and the uploader is a stock re-upload or anonymous channel. `b07` names another person as the owner of the footage.
- **Real people** are recognisable in `b01`, `b04`, and `b12`. Use them as pictures of a chamber and of a plantation, never so that they seem to speak or endorse the narration.
- Re-cutting from Wikimedia Commons: its file server answered 429 (too many requests) after a few requests for most of this session, so `fetch-shots.py` could not read Commons video URLs. The archive film is cut from Open Beelden, the original host, and the Rijksmuseum video from the local copy.
