# Krakatoa, heard round the world

A 67.5 s history film (1920×1080) in the `dark archival with animation` style (`references/dark-archival-with-animation.md`), with both halves of the style in one film and sulphur yellow as the collage accent. It tells the 1883 eruption of Krakatoa through its sound. The dark half carries it: a locator map that flies from Indonesia to the Sunda Strait, an 1883 photograph and British Pathé's 1930 film of Anak Krakatau cut on the narration with gold tags and subtitles, a stat that counts up to the death toll, a statement card, and a figure that counts over the film. A three-scene block of animated paper collage in the middle gives the measure of it: a person card, a number card with a photograph as a print, and a paper map of the Indian Ocean on which the sound spreads in rings to Batavia, Perth, and Rodrigues. All media was found by search; nothing was traced or prepared by hand.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again, except the short sound effects: put those in `assets/sfx/` yourself (the script names any that are missing). Put each file back in its slot to see the film as it was.

```bash
./make-footage.sh                      # film, stills, cut-out, print, maps, paper, music, sounds -> footage/ and assets/generated/
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

The narration (`narration.txt`, one paragraph per scene) was made with `scripts/narrate.sh narration.txt am_michael . --engine local` (Kokoro, male). Names the voice did not know (Krakatoa, Sunda, Rogier Verbeek, Batavia, Rodrigues, Anak Krakatau) are written with inline phonemes. It runs 142 words in 65.5 s, 130 words a minute.

| Scene | Half | Starts on | Shows |
|---|---|---|---|
| `map.html` | dark | "August twenty-seventh" (0 s) | Indonesia settles out of a soft focus, the camera flies to the Sunda Strait, Lampung and Banten fill, the pin drops on "Krakatoa" (2.65 s), the name types, the leader line reaches the pin, the date line types |
| `eruption.html` | dark | "The volcano has rumbled" (5.55 s) | the photograph of the eruption in May 1883 with the tag ERUPTING SINCE MAY; on "At two minutes past ten" (7.82 s) the quiet strait with the tag 10:02 AM; on "the third and largest explosion" (9.72 s) a white flash and the explosion, then on "tears" (11.35 s) the blast along the water, with the tag THE THIRD EXPLOSION; subtitles |
| `waves.html` | dark | "Waves more than thirty metres" (14.00 s) | the blast cloud rolling out over the sea; the tag WAVES OVER 30 METRES on "thirty"; subtitles |
| `stat.html` | dark | "They kill" (17.93 s) | 36,417 counting up across the spoken number over a blurred, dimmed photograph of the ruins of Anjer; a kicker, an italic line, a source line |
| `statement.html` | dark | "But what the world remembers" (22.82 s) | the 1888 lithograph, sharp for the first sentence, then dimmed; "The loudest sound in recorded history" rises word by word |
| `verbeek.html` | collage | "Rogier Verbeek" (28.46 s) | the printed cut-out whips in; the name sharpens over a yellow marker line; the role types on "a Dutch geologist"; his report types on "surveys"; TWO THIRDS GONE is stamped on "two thirds" (32.12 s) |
| `villages.html` | collage | "One hundred and sixty-five" (35.01 s) | a yellow card slaps down and 165 is stamped on the number; a photograph of a destroyed village whips in as a taped print on "coastal villages"; the label and the caption type |
| `rings.html` | collage | "The sound keeps going" (39.81 s) | the Indian Ocean on paper, pushing in 16%: a burst on Krakatoa; a yellow ring grows outward without stopping, with two arrows and a THE SOUND tag riding its front; each place gets a name strip and a dashed ring on its name (Batavia 41.36 s, Perth 44.61 s, Rodrigues 46.85 s) and a yellow distance card on its spoken distance (160 KM 42.41 s, 3,110 KM 45.49 s, 4,800 KM 47.58 s); "the chief of police reports:" types; ROAR OF HEAVY GUNS is stamped on "the roar" (50.42 s) |
| `barographs.html` | dark | "Barographs around the world" (52.98 s) | the eruption column from the air; a figure counts up to 7 on "seven" (55.90 s), with a gold tick and a caption; subtitles |
| `ending.html` | dark | "In nineteen twenty-seven" (58.06 s) | an island at dusk with the tag 1927; on "island" (60.15 s) the sea breaking open, seen from the air; on "Anak Krakatau" (62.12 s) the new volcano erupting, with the tag CHILD OF KRAKATOA on "the child"; the film falls to black |
| `paper.html` | collage | 28.46–52.98 s | one paper sheet and vignette multiplied over the collage block from the host |
| `subs.html` | dark | over the photograph and the film only | one clause per cue; no cue over the map, the cards, or the collage |

The collage block runs 24.5 s, 36% of the film.

Palette: the dark half is near-black `#070b0f`, teal land `#1f6f8b` with a cyan coast `#8fe0f5`, a green fill for the two provinces, a coral pin `#f2604f`, gold tags `#e9b04a`, and bone-white serif type `#f3efe4`. The collage half is cream paper `#e6dcc3` (sea paper `#d9cdb0`, land `#f3ecd8`), black ink `#1c1a16`, and sulphur yellow `#f2c230` for the cards, the burst, the arrows, the ring, and the marker line. Stamps are in black ink: yellow ink does not read on cream.

Facts on screen and in the narration, each checked before it was used:
- **27 August 1883, 10:02 AM, the third explosion**: Wikipedia, "1883 eruption of Krakatoa": four explosions on 27 August, at 5:30, 6:44, 10:02, and 10:41 in the morning; "the third and largest explosion, at 10:02 a.m." The brief for this film had the largest as the fourth; it is the third.
- **Erupting since May**: the same article: the eruption began on 20 May 1883. The photograph is of that first phase.
- **Waves over 30 metres**: the sources give different heights for different coasts: up to 24 m on Sumatra and 42 m on Java, 46 m at Merak (Wikipedia); 40 m (Oregon State University, Volcano World). The film says "more than thirty".
- **36,417 dead**: the official toll of the Dutch colonial authorities (Wikipedia; Volcano World: "At least 36,417 people were killed, most by the giant sea waves").
- **The loudest sound in recorded history**: Wikipedia: "the loudest sound that has ever been measured", about 180 dB at 160 km.
- **Two thirds gone**: Volcano World: "only 1/3 of Krakatau, formerly 5×9 km, remained above sea level" (Wikipedia: over 70% of the island and its archipelago).
- **Rogier Verbeek**: Wikipedia, "Rogier Verbeek": Dutch geologist (1845–1926), living at Buitenzorg on Java in 1883; his monograph *Krakatau* was published in 1884 and 1885.
- **165 coastal villages**: Volcano World: "165 coastal villages were destroyed" (from the colonial records, which also list 132 seriously damaged).
- **Batavia, 160 km; Perth, 3,110 km; Rodrigues, 4,800 km**: Wikipedia: heard "3,110 kilometres away in Perth, Western Australia, and Rodrigues near Mauritius, 4,800 kilometres away"; Batavia is 160 km from Krakatoa. The narration rounds Perth to "about three thousand one hundred"; the card shows 3,110.
- **"The roar of heavy guns"**: the report of James Wallis, chief of police on Rodrigues, quoted in the Royal Society's 1888 report: "reports were heard coming from the eastward, like the distant roar of heavy guns".
- **Seven passes in five days**: Wikipedia: barographs around the world recorded the pressure wave seven times over five days, as it went three and a half times round the globe.
- **1927, child of Krakatoa**: Wikipedia, "Anak Krakatoa": it first broke the surface on 29 December 1927; "Anak Krakatau" is Indonesian for "child of Krakatoa". It stayed above the sea for good from August 1930.

Sourcing and technique notes:
- **What the pictures are.** Nothing was filmed in 1883. From the time itself the film uses the photograph of the eruption in May 1883 (KITLV), the 1888 Royal Society lithograph, two 1883 photographs of the Java coast by Woodbury & Page (Tropenmuseum), and Verbeek's cabinet portrait. The moving film is old black-and-white film of Anak Krakatau erupting: British Pathé's "Krakatoa!" of 1930, shot in the strait three years after the new volcano appeared. It stands in for 1883 in the first half and shows itself in the ending.
- **The steamship Berouw is not in the film.** Commons has only a later photograph of its boiler lying in the Koeripan river, which does not read as a ship, so the number card carries a destroyed village instead.
- **The film's logo.** The Pathé upload is 640×480 with the archive's logo in the top right corner. Every shot takes the lower three quarters of the picture (`"zoom": 1.04, "cy": 1.0` in `shots.json`), which leaves the logo and the pillarbox wholly outside the frame; `shots-sheet.py` shows all eight shots clean. The cost is a 3.1× upscale, so the film is soft.
- **One download, then local cuts.** YouTube answered 403 partway through eight segment requests, so `make-footage.sh` fetches the film whole once, with retries, and `shots.json` cuts from that file; a row in `sources.jsonl` carries its credit.
- **Stills come as Commons thumbnails.** The original-file server allowed one download and then answered 429 for ten minutes, also to `asset-search.py --get`. The thumbnail server (`thumb.wikimedia.org`, at the widths 960, 1280, and 1920) kept answering, and it serves the 700 px Tropenmuseum photographs at 960 px.
- **The portrait is an oval vignette.** Without a box, `cutout.py` takes the whole dark oval as the object. A box set inside the oval, on the man, cuts him out cleanly.
- **The diagram is to scale.** `map-svg.py --extent` draws the ocean and gives the four places as pins in frame pixels (`ocean.json`); the three rings are circles round Krakatoa through Batavia, Perth, and Rodrigues (26, 548, and 821 px: one pixel is about 5.8 km on all three). Each dashed ring is an arc revealed through a mask. The front is one SVG circle whose radius is tweened (`attr: { r }`), so it never stops; the arrows are wrappers turned once in CSS, with a shaft that stretches and a head that slides.
- **The map's strokes are thin** (1.1 for the coast) because the camera flies in 3×, and the coast comes from the detailed set (`--detail 10m`); the pin and its rings sit outside the camera, at the place the pin ends.
- **Sound:** the music bed stops dead on the explosion and returns with the count of the dead. `scripts/duck-under-voice.py` sets the bed and the volcano's rumble under the voice; the blast and the far-off guns under the stamp carry `data-duck="off"`. The Pixabay bass impact is a full-scale tone two seconds long, so each one has a volume lane that brings it level with the voice and fades it within a second. The closing fade of the music was a point added to the bed's lane by hand after ducking; `duck-under-voice.py --fade-out <s>` now writes it.
- **Subtitles:** `scripts/subtitle-cues.py` lists a cue for every clause of the narration; `subs.html` keeps the ones over the photograph and the film, split where a cut falls inside a sentence.
