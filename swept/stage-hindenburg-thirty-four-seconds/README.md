# Hindenburg, thirty-four seconds

A 68 s history film (1920×1080) in the `dark archival with animation` style (`references/dark-archival-with-animation.md`), with both halves of the style in one film. The dark half carries it: a locator map that flies in to Lakehurst, the 1937 newsreel cut on the narration with gold tags and subtitles, a stat that counts up, a statement card, and a figure that counts over the film. A four-scene block of animated paper collage in the middle explains the cause: a person card, a paper map with a stamp, a number card with the airship as a printed cut-out, and the landing field from above with the ship flying its last circuit. All media was found by search; nothing was traced or prepared by hand.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again, except the short sound effects: put those in `assets/sfx/` yourself (the script names any that are missing). Put each file back in its slot to see the film as it was.

```bash
./make-footage.sh                      # newsreel, stills, cut-outs, maps, paper, music, sounds -> footage/ and assets/generated/
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

The narration (`narration.txt`, one paragraph per scene) was made with `scripts/narrate.sh narration.txt bm_george . --engine local`. Names the voice did not know are written with inline phonemes.

| Scene | Half | Starts on | Shows |
|---|---|---|---|
| `map.html` | dark | "Thursday, May sixth" (0 s) | the country settles out of a soft focus, the camera flies to New Jersey, the state fills, the pin drops on "Lakehurst", the name types, the leader line reaches the pin |
| `arrival.html` | dark | "At seven twenty-one" (5.93 s) | the ship over Manhattan with the tag LZ 129 HINDENBURG; on "drops" (9.95 s) the ship over the landing field with the tag 7:21 PM; subtitles |
| `flame.html` | dark | "Four minutes later" (12.81 s) | flame pouring past the hull; the tag 7:25 PM on "a flame"; subtitles |
| `stat.html` | dark | "About thirty-four seconds" (17.08 s) | 34 counting up across the spoken number over the blurred, dimmed photo of the fire; a kicker, an italic line, a source line |
| `statement.html` | dark | "Until that evening" (21.73 s) | the passenger lounge, sharp for the first clause, then dimmed; the sentence rises word by word |
| `eckener.html` | collage | "Hugo Eckener" (26.99 s) | the printed cut-out whips in; the name sharpens; the role types; HELIUM is stamped on "helium"; a taped note types on "cannot burn" |
| `helium.html` | collage | "But America" (34.03 s) | a paper map of the USA; Texas fills on "holds"; a pin and a name strip for Amarillo; NOT FOR EXPORT is stamped on "forbids" |
| `hydrogen.html` | collage | "So the Hindenburg" (39.99 s) | the airship as a printed cut-out whips in; a red card slaps down; 7,000,000 is stamped on "seven million"; the label types |
| `field.html` | collage | "It reaches Lakehurst" (45.17 s) | the landing field from above: the ship arrives, 12 HOURS LATE is stamped on "twelve", the circuit is drawn and flown, a storm scrap flashes and clears, the lines drop, a burst at the tail |
| `fire.html` | dark | "the tail catches fire" (51.20 s) | a white flash, the tail on fire, then the whole ship settling to the ground; subtitles |
| `survivors.html` | dark | "Ninety-seven people" (55.91 s) | the wreck burning; a figure counts up to 97, then falls to 62 on "sixty-two"; subtitles |
| `ending.html` | dark | "The age of the passenger airship" (61.29 s) | the wreck from the air the next day; the tag THE END OF AN AGE; the film falls to black |
| `paper.html` | collage | 26.99–51.20 s | one paper sheet and vignette multiplied over the collage block from the host |
| `subs.html` | dark | over the film only | one clause per cue; no cue over the map, the cards, or the collage |

The collage block runs 24.2 s, 36% of the film.

Palette: the dark half is near-black `#070b0f`, teal land `#1f6f8b` with a cyan coast `#8fe0f5`, a green state fill, a coral pin `#f2604f`, gold tags `#e9b04a`, and bone-white serif type `#f3efe4`. The collage half is cream paper `#e6dcc3`, ink `#1c1a16`, red cards `#e0554a`, red stamps `#b3271e`, and one green stamp `#1f7a4c`.

Facts on screen, checked against Wikipedia's "Hindenburg disaster" and "LZ 129 Hindenburg" and the Texas Historical Commission's page on the Amarillo Helium Plant:
- **7:21 PM and 7:25 PM**: the mooring lines were dropped at 7:21 and the ship caught fire at 7:25 local time.
- **34 seconds**: the time from the first sign of fire to the bow on the ground is reported as 32, 34, or 37 seconds. The narration says "about".
- **97 on board, 62 survivors**: 36 passengers and 61 crew; 35 of them died, and one man on the ground.
- **7,000,000 cubic feet** of gas (7,062,000 cu ft, 200,000 m³) and **245 metres** long.
- **12 hours late**: the ship reached Lakehurst almost half a day behind schedule, and thunderstorms held the landing back again.
- **Helium**: helium was chosen for the design because it does not burn; the Helium Control Act of 1927 barred its export from the United States, the only country producing it in quantity, and the Bureau of Mines plant at Amarillo, Texas, was then the world's only commercial helium plant.
- **No passenger killed before**: the Zeppelin company advertised that no passenger had been injured on any of its airships.

Sourcing and technique notes:
- **One newsreel carries the film.** Universal's special release of 10 May 1937 is on Commons as Theora (`.ogv`). Cuts from it opened on a green frame, so `make-footage.sh` transcodes it once and `shots.json` cuts from the copy; `scripts/made.py` records what the copy came from, and `CREDITS.md` lists the newsreel.
- **The crop:** the film is 4:3, cover-cropped to 16:9 (a 2× upscale; soft, as archive film is).
- **Eckener stands in a crowd,** so `cutout.py --box` looks only at him. The 1024 px source is doubled before `paper-cutout.py`, which keeps the halftone dots fine at the size shown.
- **The map's strokes are thin** (1.1 for the coast) because the camera flies in 3×; the pin and its rings sit outside the camera, at the place the pin ends.
- **The ship token turns as it travels** (`data-layout-allow-orbit`); its name tag is a separate element on the same path.
- **Sound:** the music bed stops dead on the flash and returns under the count of survivors. `scripts/duck-under-voice.py` sets the bed and the fire under the voice; the blast carries `data-duck="off"` so it is heard at its written level.
