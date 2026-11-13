# Pictures: photos, b-roll, icons

Sourced on 2026-10-09 for the recut of the talking-head video about the sentencing demand against Nadiem Makarim.
Every file below has a named source page and the licence that page states. The machine-readable record is `sources.jsonl` (downloads and what was made from them), `shots.json` (b-roll), and `credits-extra.jsonl` (icons).

## 1. Photos (`assets/pic/`)

Each portrait is an 800x1000 crop of the downloaded original in `assets/pic/orig/` (same file name). The name was checked on each Commons page (title, description, category).

| File | Shows | Source page | Author | Licence |
|---|---|---|---|---|
| `nadiem-makarim.jpg` | Nadiem Makarim speaking on a panel, World Economic Forum on ASEAN, Hanoi, 12 Sep 2018 (as Gojek CEO, before he was minister) | https://commons.wikimedia.org/wiki/File:Nadiem_Makarim.jpg | World Economic Forum / Sikarin Thanachaiary | CC BY-SA 2.0 |
| `mahfud-md.jpg` | Mahfud MD, close portrait, Jan 2020 (as coordinating minister) | https://commons.wikimedia.org/wiki/File:Mahfud_MD_(cropped).jpg | Coordinating Ministry for Political, Legal, and Security Affairs (Indonesia) | Public domain (Indonesian government work) |
| `rhenald-kasali.jpg` | Rhenald Kasali speaking with a microphone, 2025 | https://commons.wikimedia.org/wiki/File:Rhenald_Kasali_2025.jpg | Juxlos | CC BY-SA 4.0 |
| `hotman-paris.jpg` | Hotman Paris Hutapea holding a microphone, Dec 2019 | https://commons.wikimedia.org/wiki/File:Hotman_Paris.jpg | FHF Law School (YouTube screenshot, CC upload) | CC BY 3.0 |
| `ferry-irwandi.jpg` | Ferry Irwandi on stage at Gadjah Mada University, Oct 2024 | https://commons.wikimedia.org/wiki/File:Potret_Ferry_Irwandi.jpg | 2ndhart | CC0 |
| `gary-neville.jpg` | Gary Neville, 6 Nov 2014, holding an artwork board (lower third of the crop) | https://commons.wikimedia.org/wiki/File:Gary_Neville_2014.jpg | University of Salford Press Office | CC BY 2.0 |
| `micah-richards.jpg` | Micah Richards at Manchester City's title parade, 14 May 2012 (as a player) | https://commons.wikimedia.org/wiki/File:Richards_crop.JPG | Oldelpaso | CC BY-SA 3.0 |
| `thierry-henry.jpg` | Thierry Henry as Arsenal youth coach, 29 Sep 2015 | https://commons.wikimedia.org/wiki/File:Thierry_Henry_Arsenal_U19s_Vs_Olympiacos_(cropped).jpg | joshjdss | CC BY 2.0 |
| `jamie-carragher.jpg` | Jamie Carragher in a charity-match shirt, 4 Mar 2014, three-quarter profile | https://commons.wikimedia.org/wiki/File:Football_against_poverty_2014_-_Jamie_Carragher_(cropped).jpg | Ludovic Péron | CC BY-SA 3.0 |
| `chromebook.jpg` | Samsung Chromebook Series 3, open, in its box (1474x1747, not cropped) | https://commons.wikimedia.org/wiki/File:Chromebook_serie_3.jpg | Nahusm | CC BY-SA 3.0 |
| `chromebook-lid.jpg` | Close view of an Acer Chromebook lid with the Chrome mark (1920x1080) | https://commons.wikimedia.org/wiki/File:Acer_Chromebook_15_CB3-532,_Winschoten_(2021)_03.jpg | Donald Trung Quoc Don | CC BY-SA 4.0 |
| `kejaksaan-agung.jpg` | Attorney General's Office tower, Jakarta, with its name sign, 2025 (1980x1980, not cropped) | https://commons.wikimedia.org/wiki/File:Gedung_Kejaksaan_Agung_(2025).jpg | Medelam | CC BY-SA 4.0 |

Limits to know before using them:

- **Small originals, upscaled:** `rhenald-kasali.jpg` (613x706 original), `hotman-paris.jpg` (525x600, a video screenshot), and `micah-richards.jpg` (572x684) are soft at 800x1000. Keep them small in the frame.
- **Old pictures:** Micah Richards (2012) and Jamie Carragher (2014) are shown as players, not as the TV pundits they are now; Nadiem's photo is from 2018.
- **Attribution:** the CC BY and CC BY-SA photos need the author and licence named in the credits; CC BY-SA also asks that an adapted picture be shared under the same licence.
- **Rhenald Kasali:** the file is named and described as him by the uploader, but the page's only category is "Unidentified men of Indonesia". The face matches his public pictures; the page alone is weaker proof than the others.
- **Chromebook:** neither picture is a studio product shot (none exists on Commons); the units in the case were other brands.

## 2. B-roll (`footage/b01.mp4` to `b15.mp4`)

All clips: 1920x1080, 30 fps, no audio, 6.2 s (a 6 s slot plus a 0.2 s handle), cut with `fetch-shots.py` from `shots.json`. Reviewed on the shot sheet (`.footage/shots-sheet.png`): no burned-in text, logo, or watermark, and no cut inside any slot. Text that appears was filmed in the scene.

| File | Shows | Source page | Author | Licence | In-point | Limits |
|---|---|---|---|---|---|---|
| `b01.mp4` | Football referee in a blue shirt talks to the two team captains before kick-off, gesturing, with his assistant beside him (amateur match, daylight) | https://www.pexels.com/video/a-referee-tossing-a-coin-to-determine-first-ball-possession-between-competing-teams-in-a-soccer-match-3558720/ | Nazim Zafri | Pexels License | 0.5 s | Faces of the referee, assistant, and one captain are clear. Shirt sponsor print and referee badge are in the scene. No whistle or card is shown. |
| `b02.mp4` | Face-painted football fans in wigs shouting and waving their arms in the stands | https://www.pexels.com/video/loyal-fans-supporting-their-team-in-a-game-3195653/ | Pressmaster | Pexels License | 8.5 s | Staged with actors; they read as loud and excited, not angry. Faces are close. The first 8 s of the source show a hand-held sign with words, so do not move the in-point earlier. |
| `b03.mp4` | From the stand: players and coaches crowd the referees on the touchline beside the pitch-side VAR monitor booth, while a player lies injured (the file's title: Samsunspor v Manisa match, 5 Sep 2022, VAR penalty) | https://commons.wikimedia.org/wiki/File:Samsunspor-Manisa_ma%C3%A7%C4%B1_(05.09.22)_VAR_penalt%C4%B1.webm | Commons user 𐰇𐱅𐰚𐰤 (own phone footage) | CC BY-SA 4.0 | 10.0 s | **Soft source:** hand-held phone footage, cropped in 1.7x (zoom 1.7, cx 0.31, cy 0.71), so it is soft and shaky at full frame; best darkened under a caption. People are small; no face is recognisable. Real match of real clubs. CC BY-SA: credit the author, and share an adapted clip under the same licence. The whole 26 s original is in `footage/raw/commons-var-penalty.webm`. |
| `b04.mp4` | Two fans on a sofa (one in a red football shirt) throw their hands to their heads at a match on TV; the TV is off screen | https://www.pexels.com/video/sports-fans-cheering-on-the-couch-6104279/ | Kampus Production | Pexels License | 3.0 s | Staged with actors; both faces are clear. A Portugal flag and popcorn are in frame. |
| `b05.mp4` | Same scene as b01: the captain in black rubs his forehead while the referee keeps explaining | https://www.pexels.com/video/a-referee-tossing-a-coin-to-determine-first-ball-possession-between-competing-teams-in-a-soccer-match-3558720/ | Nazim Zafri | Pexels License | 11.0 s | Same source and framing as b01: do not place the two back to back. It is a calm pre-match talk, not a protest; it only reads as frustration because of the gesture. Clear faces. |
| `b06.mp4` | Wide view of a full, floodlit football stadium at night with the crowd's phone lights on | https://www.pexels.com/video/vibrant-night-football-match-at-a-stadium-35053395/ | Necip Duman | Pexels License | 0.5 s | Short clean stretch: the source flashes bright at about 7.3 s, so this cut ends at 6.7 s and cannot be lengthened. Cropped in 1.07x to remove thin bars. Advertising boards (Audi, Skoda, Doğuş) and a portrait of Atatürk are part of the stadium. |
| `b07.mp4` | A man with a whistle in his mouth points and signals like a referee in an indoor sports hall; players' heads pass in the foreground | https://www.pexels.com/video/a-referee-officiating-a-basketball-game-6767271/ | cottonbro studio | Pexels License | 0.0 s | **Stand-in, not football:** this is a basketball referee indoors, in plain black clothes, staged. It is the only clean whistle shot found. Use it only where a whistle matters more than the sport. His face is clear. |
| `b08.mp4` | Over the shoulder of a red-haired woman scrolling a photo feed on her phone | https://mixkit.co/free-stock-video/young-woman-lying-down-watching-images-on-her-cell-phone-4911/ | arvizum | Mixkit Free License (stock video) | 0.5 s | The source is 10 s long. The feed shows pictures, not comments. No face. |
| `b09.mp4` | Close view of two hands typing on a laptop keyboard in low light | https://www.pexels.com/video/person-typing-946146/ | Carlos Arribas | Pexels License | 4.0 s | Already dark. The screen is not visible, so it does not show a comment being written. No face. |
| `b10.mp4` | A young creator talks to his camera in a blue-lit studio; the camera's furry microphone moves into the foreground | https://mixkit.co/free-stock-video/youtuber-vlogging-in-his-studio-41272/ | Mixkit | Mixkit Free License (stock video) | 7.0 s | His face is clear and fills the frame. No ring light. The source ends at 14 s, so this cut uses its last usable seconds. |
| `b11.mp4` | A hand lifts a judge's gavel and strikes the block, close | https://www.pexels.com/video/judge-using-a-gavel-6699964/ | pickarick | Pexels License | 3.0 s | Dark picture already. A generic studio desk, not an Indonesian court. No face. |
| `b12.mp4` | Wood-panelled courtroom: a woman in a white suit at the front table, people seated in the gallery behind her | https://www.pexels.com/video/courtroom-proceedings-with-diverse-attendees-34550101/ | khezez | Pexels License | 15.0 s | **Staged film set, not Indonesia.** Cut as a 16:9 band (cy 0.53) from a vertical 4K source. The actress's face is clear and central, and she stands up near the end. A "QUIET" sign on the wall is part of the set. Clean stretch is about 9 s to 45 s of the source: a clapperboard appears before it and studio lights after. |
| `b13.mp4` | Brass scales of justice on a desk by a window | https://www.pexels.com/video/scale-of-justice-5636967/ | Sora Shimazaki | Pexels License | 1.0 s | Bright picture: needs more darkening than the others. The source is 10 s long. No face. |
| `b14.mp4` | Slow hand-held pan along white prison cell bars with an empty cell behind (tagged Alcatraz on its page) | https://www.pexels.com/video/empty-jail-5243246/ | Teela Hudak | Pexels License | 7.4 s | Hand-held, a little jittery (the sheet flags it as cuts; frame by frame it is one take). 6.17 s long. A museum cell in the United States, not an Indonesian prison. No people. |
| `b15.mp4` | A crowd of commuters walks down the steps out of Dukuh Atas BNI station, Jakarta, at dusk | https://www.pexels.com/video/commuters-exiting-dukuh-atas-station-at-dusk-39681507/ | Daneswara Eka | Pexels License | 2.0 s | Real passers-by, several faces recognisable (some in masks): keep it dark and do not let a caption about guilt or ignorance sit on one person. The station name sign is in the scene. A station exit, not an open street. |

Licence notes: the Pexels License and the Mixkit Free License allow free use, commercial included, without attribution; the Pexels License forbids showing an identifiable person in a bad light or an offensive way, which matters for the clips with clear faces. Pexels clips were found on pexels.com in the browser (the search tool has no Pexels source) and cut from the file addresses on each clip's page; `.assets/search/video-pexels-picks/candidates.json` holds their page, author, and licence so `footage-credits.py` lists them.

## 3. Icons (`assets/icons/`)

28 outline icons from Tabler Icons, MIT licence, fetched from `https://raw.githubusercontent.com/tabler/tabler-icons/main/icons/outline/<name>.svg`. Each file was edited to `stroke="#000000"` and `stroke-width="1.5"` (for use as a mask). Source: https://github.com/tabler/tabler-icons, catalogue https://tabler.io/icons, by Paweł Kuna and contributors. One line in `credits-extra.jsonl` covers them all.

| File | Shows |
|---|---|
| `gavel.svg` | judge's gavel |
| `scale.svg` | balance scale |
| `message-circle.svg` | one speech bubble |
| `messages.svg` | two speech bubbles |
| `user.svg` | one person |
| `users.svg` | group of people |
| `ball-football.svg` | football |
| `cards.svg` | two overlapping cards (referee cards) |
| `eye.svg` | eye |
| `school.svg` | graduation cap |
| `certificate.svg` | certificate with a seal |
| `license.svg` | licence document |
| `coin.svg` | coin |
| `cash.svg` | banknotes |
| `lock.svg` | padlock |
| `speakerphone.svg` | megaphone |
| `microphone.svg` | microphone |
| `question-mark.svg` | question mark |
| `check.svg` | tick |
| `x.svg` | cross |
| `brand-youtube.svg` | YouTube mark |
| `brand-instagram.svg` | Instagram mark |
| `device-tv.svg` | television |
| `news.svg` | newspaper |
| `book.svg` | open book |
| `building-bank.svg` | columned building (court, institution) |
| `clock.svg` | clock |
| `calendar.svg` | calendar |

`brand-youtube.svg` and `brand-instagram.svg` draw trademarks: the MIT licence covers the drawing, not the marks.

## Not found with a usable licence

- **Ibrahim Arief (Ibam): no photo.** The one openly licensed picture (Commons, CC BY 4.0, a press-conference photo) has been deleted from Commons; the download answers 404. Nothing else on Commons or Openverse shows him. News-site photos were not taken.
- **Jakarta corruption court building (Pengadilan Tipikor / PN Jakarta Pusat): no photo.** Commons has only documents under those names. The Attorney General's Office building is supplied in its place (`kejaksaan-agung.jpg`).
- **A football referee blowing a whistle or showing a card: no clip.** Pexels, Pixabay, Mixkit, and Commons have none (Pixabay's "referee" clips are cartoons). b01 and b05 show a real football referee talking; b07 is a whistle from basketball.
- **Players arguing with a referee, close up: no clip.** b03 (wide, soft) and b05 (calm talk, frustrated gesture) are the nearest.
- **A referee looking at the VAR monitor: no clip.** b03 shows the monitor booth and the officials beside it from the stand, but nobody is seen watching the screen.
- **Angry fans:** stock fans cheer. b02 is shouting, b04 is frustration at home.
- **A studio product photo of a Chromebook:** not on Commons; the two supplied are ordinary photographs.
