# Nadiem and our frustration

A 9.5-minute presenter explainer (9:31, 1920×1080, 30 fps, in Indonesian) in the `deep tracery` style (`references/deep-tracery.md`), `tracery` design (`"design": "tracery"`: a warm dark room, lime and mint type, red tags, a yellow highlighter, a paper note for the chapter card; presenter look at brightness 0.7; it was first built in `meridian`, and the user preferred `tracery`). The presenter clip is someone else's talking-head video, used at the user's request: the first 11:38 of Pandji Pragiwaksono's "NADIEM & FRUSTRASI KITA" (YouTube, 22 May 2026), up to the point where he turns to his second topic. It is the pattern for a film made from an existing talk: rough cut, six chapter cards, six news clips cut in with their own sound, and every figure the speaker gives shown at its source.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
python3 ../../scripts/rough-cut.py source/original.mp4 -o footage/aroll.mp4 --lang id --to 698.6 --drop 13.69-27.98 --verify
python3 ../../scripts/speech-timeline.py footage/aroll.mp4   # aroll.srt and aroll.timeline.json: every spoken segment, measured on the sound
python3 ../../scripts/deep-tracery-build.py --words     # the timeline: each segment with its words' numbers ("." opens a sentence)
python3 ../../scripts/presenter-layers.py footage/aroll.mp4 --only "$(python3 ../../scripts/deep-tracery-build.py --on-screen)"   # the presenter cut out, and a still of his room
python3 ../../scripts/backdrop-match.py source/three-somber-windows.jpg --room footage/aroll.room.png --crop 2950,2480,2800 -o assets/backdrop.jpg   # the stand-in photo, fitted to that room
python3 ../../scripts/deep-tracery-build.py             # index.html and compositions/ from cut.json
python3 ../../scripts/duck-under-voice.py index.html --floor-under 23 --open 8
python3 ../../scripts/deep-tracery-sync.py              # every cut in a silence, every scene with its words
npx hyperframes@0.8.103 check
FFMPEG_PROCESS_TIMEOUT_MS=1200000 node ../../scripts/render-in-chunks.mjs -o renders/draft.mp4   # a plain render stalls on this film
python3 ../../scripts/deep-tracery-sync.py renders/draft.mp4   # the render against the timeline
```

| | |
|---|---|
| Rough cut | 11:38.6 of the source became 8:13.6: 237 pieces (silent pauses, filler sounds, and the 14 s bumper, `--drop`) taken out at 235 joins; 125 words a minute before, 177 after |
| Film | 571.3 s: a cold open of 11 s, six chapters, an 8 s end screen |
| Scenes | 115: 44 texts over the presenter, 4 stages, 11 floated pictures, 17 pages, 13 b-roll shots, 6 clips, 6 flows, 3 comparisons, 3 big figures, 8 text cards |
| Shares | presenter on screen 52% (with text or a picture beside him for three quarters of that), pages 13%, b-roll 9%, clips 7%, flows and comparisons 8%, chapter cards 5%, text cards 3% |
| Presenter alone | never longer than 6 s |
| Framings | 123: wide, medium, close in turn at the joins, to one side under each text |
| Seams | 22 seams (streaks of light and white flashes, `tracery`), 6 chapter cards |
| Background | replaced (`"background": "replace"`): the presenter is cut out for the 5.3 minutes he is seen (35 stretches) and set over `assets/backdrop.jpg`, "Three Somber Windows" by Ehud Neuhaus (CC0 1.0), a brick wall with window boxes like his own, cropped to his room's scale and brought to its colour and focus by `backdrop-match.py`. Side texts on a veil that falls on the backdrop only, 9 pictures behind his shoulder, 4 stages (FRUSTRASI, KEADILAN, two icons "kompeten" and "terkenal", TAKUT), each a word on top of his shoulder with its end behind him |

**The manifest.** `cut.json` is the film: the chapters and every scene, placed by the presenter's word numbers (`--words`). `index.html` and `compositions/` are written by the builder: change the manifest and build again, never the HTML.

**Rough cut.** `footage/aroll.cuts.json` lists every kept range and every join. `--verify` transcribed the cut again and named three places where words were "not heard": each is a phrase the speaker says twice in a row ("kita berkomentar, kita berkomentar"), which a transcription writes once. Nobody has listened to the cut yet.

**Where the sources say something else than the speaker.** The film shows the source each time, and never changes his words:

- He says the replacement money is "4,8 triliun, kalau nggak salah, tolong dikoreksi". The demand is Rp 5,68 triliun in total (Rp 809,6 miliar and Rp 4,87 triliun): the Detik page under his sentence carries the full figure, and the label says it.
- He says the verdict is "kayaknya awal Juni". Early June (2 June 2026) was the defence plea: the Kompas page of 15 May is shown with the label "2 JUNI: SIDANG PEMBELAAN". The verdict came on 30 June, after the talk, and is not in the film.
- He says Ibam was "dituntut 22 tahun". The demand was 15 years, with 7,5 more as the substitute for unpaid replacement money: the label says "15 TAHUN + 7,5 TAHUN".
- He gives Hotman Paris the words "bayar gue yang bener". The clip and the page show what Hotman said: "jangan pelit-pelit sama pengacara".
- He says Rhenald Kasali finds the trial odd. No source for that was found; the film shows the portrait with his post ("Guru Besar FEB UI") and makes no claim of its own.
- Mahfud MD's view is sourced for January 2026 (the clip is tagged with its date); by June he had said otherwise.

**Stand-ins**, to tell the user with the draft:

- `b07` is a basketball referee: no clean stock shot of a football referee with a whistle was found. It runs under "wasit dibayar nih".
- `b03` is phone film of a VAR check from the stands, cropped 1.7×, soft.
- `b12` is a staged courtroom, not an Indonesian one.
- No openly licensed photo of Ibrahim Arief exists; he appears only in the two pages.
- The portraits of Rhenald Kasali, Hotman Paris, and Micah Richards come from small originals.
- `c04` (Hotman Paris) is a vertical video with its own burned-in captions; `c06` (the pundits) is from 2024 and about a manager, not a referee.

**Sound.** The presenter's own voice, at the level of the recording. The bed is "Perspectives" by Kevin MacLeod (incompetech.com, CC BY 4.0), one unlooped stretch, set 23 dB under the speaking voice by `duck-under-voice.py`. A chapter card carries a low hit, keys, and a rise; a streak of light a whoosh. The six clips play their own sound, each brought to the presenter's loudness by the builder (the Hotman Paris clip was 12 dB louder than he is), and the music stays down under them.

The renders are not kept in this folder. An earlier build of this film read 17.3 dB with `duck-under-voice.py --mix renders/draft.mp4` (the source films: 17 to 19) and its draft measured −22.6 LUFS, brought to −15.4 LUFS with 7.4 dB of gain and a limiter (the source films: −14.8 to −15.9). After a new render, measure both again: the build has changed since.

**The evidence record.** `evidence/pages-and-clips.md` and `evidence/pictures.md` list every capture, clip, and picture with its address, what it supports, and where it differs from what the speaker says.

`CREDITS.md` lists every source. Its takedown notice holds the placeholder `[the contact address the user gives]`: the address is the user's to give (`footage-credits.py --contact`).
