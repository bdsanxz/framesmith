# Prabowo's incomplete speech

A 10.5-minute presenter explainer (10:32, 1920×1080, 30 fps, in Indonesian) in the `deep tracery` style (`references/deep-tracery.md`), `tracery` design (`"design": "tracery"`: a warm dark room, lime and mint type, red tags, a yellow highlighter, a paper note for the chapter card; presenter look at brightness 0.7; it was first built in `meridian`, and the user preferred `tracery`). The presenter clip is someone else's talking-head video, used at the user's request: the first 11:00 of Pandji Pragiwaksono's "PIDATO TIDAK PARIPURNA PRABOWO" (YouTube, 28 May 2026), up to the end of his reading of the speech's framing. It is the pattern for a film about a speech: the speaker's own words cut in as clips, the official figures shown as rankings, and the presenter's memory of them checked against the record.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
python3 ../../scripts/rough-cut.py source/original.mp4 -o footage/aroll.mp4 --lang id --to 660.25 --drop 52.6-66.72 --verify
python3 ../../scripts/speech-timeline.py footage/aroll.mp4   # aroll.srt and aroll.timeline.json: every spoken segment, measured on the sound
python3 ../../scripts/deep-tracery-build.py --words     # the timeline: each segment with its words' numbers ("." opens a sentence)
python3 ../../scripts/presenter-layers.py footage/aroll.mp4 --track   # where the presenter is in the frame; his background is kept
python3 ../../scripts/deep-tracery-build.py             # index.html and compositions/ from cut.json
python3 ../../scripts/duck-under-voice.py index.html --floor-under 24 --open 8
python3 ../../scripts/deep-tracery-sync.py              # every cut in a silence, every scene with its words
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
python3 ../../scripts/deep-tracery-sync.py renders/draft.mp4   # the render against the timeline
```

| | |
|---|---|
| Rough cut | 11:00.3 of the source became 9:02.3: 175 pieces (silent pauses, filler sounds, and the 14 s bumper, `--drop`) taken out at 173 joins; 135 words a minute before, 164 after |
| Film | 631.9 s: a cold open of 38 s, six chapters, an 8 s end screen |
| Scenes | 93: 36 texts over the presenter, 4 stages, 6 floated pictures, 14 pages, 11 b-roll shots, 10 clips, 5 flows, 3 rankings, 2 comparisons, 2 text cards |
| Shares | presenter on screen 45%, pages 13%, clips 10%, b-roll 9%, flows 8%, rankings 7%, chapter cards 5% |
| Presenter alone | never longer than 6 s |
| Background | kept as filmed (`"background": "keep"`, the default): no cut-out, the clip plays as it is. The texts and the 6 pictures stand beside the presenter, and the 4 stages (PARIPURNA, ENGAGING, two icons "geopolitik" and "geoekonomi", SPEAK DOANG) each put a word beside him at the height of his shoulder, in front of the room and stopping short of him. The track of where he is (`--track`) places them |
| Clips | seven lines of the speech cut in with their own sound, each brought to the presenter's loudness; three more play silent under his voice (the round table of "Prabowo Menjawab", the chart, the folder in Washington) |

**The manifest.** `cut.json` is the film: the chapters and every scene, placed by the presenter's word numbers (`--words`). `index.html` and `compositions/` are written by the builder: change the manifest and build again, never the HTML.

**Rough cut.** `--verify` named six places where words were "not heard" after the cut: five are fillers of speech ("ya juga", "gitu ya", "itu itu") or a phrase said twice, and one ("aring dan") is a slip of the tongue the second transcription dropped. Nobody has listened to the cut yet.

**Where the record says something else than the presenter.** The film shows the record each time, and never changes his words:

- He says the Netherlands had the world's highest GDP "selama 300 tahun". Prabowo said 400 years, per capita: the clip of that line follows the presenter's sentence. Prabowo's own slide (the chart clip) puts the Netherlands third in 1500 and first in 1600, 1700, and 1800.
- He says the rupiah had touched "17.845". The record before his upload was 17.748, on the morning of the speech, and it closed at 17.654 that day: the two Kontan pages carry those figures in their labels.
- He says "tiga jam untuk diwawancara, dua jam". Detik reports four hours for "Prabowo Menjawab" in April 2025; the March 2026 session at Hambalang ran from 19.00 to 01.30. Both pages are shown.
- He says "disuruh megangin map sama Presiden Amerika Serikat": on 19 February 2026 in Washington, Donald Trump handed Prabowo the signed Board of Peace folder to hold. The Kompas.com clip of that moment plays under his sentence, with its date.
- The speech ran 1 h 38 min on the official stream; he says an hour and a half. No article states its length, so the film shows no figure.
- "Lagi ada banyak meme": no source for the line having become a meme was found; the film shows the line itself, from the speech.

**Stand-ins**, to tell the user with the draft:

- `b01` is a legislator at a chamber microphone, not a president at a podium; `b04` is a chamber that looks like a US state senate; `b02` is a desktop teleprompter.
- `b05` shows US dollars being counted (no clean film of rupiah); `b06` is a stock price board, not a currency board.
- The three colonial pictures are stills (a Deli plantation, VOC ships at Batavia, Batavia castle), shown grey under the captions.
- In `c10` (the joke off the script) three words are uncertain: "Kau" (also heard as "kok"), "di-reshuffle" (also "di-shuffle"), and the final "ya". The moment is 65 minutes into the speech, later than the passage the presenter is describing.
- The clips carry the marks of TVR Parlemen and the Presidential Secretariat; `c12` carries Kompas.com's logo and caption.

**Sound.** The presenter's own voice, at the level of the recording. The bed is three tracks by Kevin MacLeod (incompetech.com, CC BY 4.0), "Dispersion Relation", "Floating Cities", and "Dream Culture", levelled and joined with 4 s crossfades, set with `duck-under-voice.py --floor-under 24` (the style starts at 23 and moves the value by what `--mix` reads; this film was set at 24). A chapter card carries a low hit, keys, and a rise; a streak of light a whoosh. The renders are not kept in this folder. An earlier build of this film read 17.3 dB with `duck-under-voice.py --mix renders/draft.mp4` (the source films: 17 to 19) and its draft measured −20.5 LUFS, brought to −15.3 LUFS with 5.3 dB of gain and a limiter (the source films: −14.8 to −15.9). After a new render, measure both again: the build has changed since.

**The evidence record.** `evidence/pages-and-clips.md` and `evidence/pictures.md` list every capture, clip, and picture with its address, what it supports, and where it differs from what the presenter says.

`CREDITS.md` lists every source. Its takedown notice holds the placeholder `[the contact address the user gives]`: the address is the user's to give (`footage-credits.py --contact`).
