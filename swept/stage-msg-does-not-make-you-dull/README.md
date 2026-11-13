# MSG does not make you dull

A 40-second vertical Short (40.24 s, 1080×1920, 30 fps, narrated in Indonesian) in the `paper flash transitions` style (`references/paper-flash-transitions.md`), in the shape "a belief corrected": the belief, what the thing is, where it occurs naturally, who found it, where the scare began, the evidence, a closing question. It is the pattern for a film with a paper tear after the hook, a zoom seam on the turn, a source card, and two stickers.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
./make-footage.sh                                   # 42 shots from 32 sources, the music, a paper sheet -> footage/, assets/audio/bed.mp3, assets/paper/ (about 10 minutes, 350 MB)
python3 ../../scripts/paper-flash-build.py          # index.html and compositions/ from shots.json, cut.json, captions.txt
python3 ../../scripts/duck-under-voice.py index.html --floor-under 20 --open 3
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

| | |
|---|---|
| Narration | 89 words in 5 paragraphs, 133 words a minute |
| Shots | 42, median 0.85 s, shortest 0.37 s, longest 3.77 s (the shot under the source card) |
| Caption cards | 81 |
| Opening | the first shot pulls back out of blur |
| Paper flashes | 6: at 2.20 s ("padahal"), 10.30, 15.40, 26.67, 32.00, 36.27 s |
| Paper tear | opens 4.14–4.47 s, into "Micin itu MSG" |
| Zoom seam | 20.90 s ("Soal pusing") |
| Source card | 23.10–26.60 s, over the typewriter shot blurred and darkened |
| Stickers | "1908" at 18.53–19.80 s, "1/3" at 33.73–34.67 s |

**The manifests.** `shots.json` is the cut, `cut.json` names each seam by the shot it opens, `captions.txt` holds the 81 cards. `index.html` and `compositions/` are written by the builder: change a manifest and build again, never the HTML.

**Narration.** `narration.txt` is written in the plain-exclaim formula of the Qwen voice `bayam` (`references/qwen-voices.md`) and was spoken with `scripts/narrate.sh narration.txt bayam . --lang id --engine qwen --gap 0.3`: the hook by `bayamf-plus`, the other four paragraphs by `bayamf`. Nobody has listened to the takes yet. The local transcript shows these, for the user's ear:

- `s03`: one "micin" is written "mikin"; a pause of 0.4 s falls inside "Yang pertama, | nemuin"; pauses of 0.5 s and 0.4 s fall inside "Dan uji | ilmiah setelahnya nggak | pernah".
- `s05`: the first take held a 3.8 s silence; the retake passed.

**Captions.** Four cards were corrected by hand: "MIKIN" to "MICIN", "ANTI-MICIN" to "ANTI MICIN", and the three words heard as "1 per 3" to one card, "SEPERTIGA".

**The source card.** The headline is the title of the letter itself: Robert Ho Man Kwok, "Chinese-Restaurant Syndrome", The New England Journal of Medicine, 4 April 1968. The card is set in type, not captured from the journal's page.

**Sound.** The bed is "Arcade Aroma" by Touda (Freesound, CC0), 108 beats a minute, set by `duck-under-voice.py --floor-under 20`; `--mix` on the last render read 18.0 dB between the floor and the speaking voice (the source: 14 to 20). Each paper flash carries the paper swish, the tear the rip, the zoom seam a whoosh. That draft measured −16.6 LUFS and was brought to −14.3 LUFS with 2.4 dB of gain and a limiter. The renders are not kept in this folder: render again and measure both again.

**Stand-ins and soft shots**, to tell the user with the draft:

- No clean footage of MSG crystals was found: `s02`, `s10`, `s37` show salt from a spoon, `s06`, `s07`, `s36` sugar.
- `s23`, `s24`: today's Osaka and Tokyo under "Jepang tahun 1908"; nothing from the period.
- `s26`: kelp on a beach, not kombu. `s25`: miso soup being ladled, not dashi.
- `s07`, `s25`: 1080p landscape sources cropped to vertical (1.78×), for 0.6 s and 0.4 s.
- `s17`: a glass of wine stands behind the hand grating the cheese.
- `s22`, `s34`: laboratory monitors in the scene show interface text.
- `s12`: an identifiable person from a Wikimedia Commons clip, shown eating noodles as filmed.

**Facts.** Glutamate was isolated from kombu broth by Kikunae Ikeda in 1908. It occurs naturally in tomatoes, cheese, mushrooms, and human milk. The 1968 letter is the one on the card. MSG is about 12% sodium by weight and table salt about 39%, so "about a third". The script says the tests "never consistently proved" the symptoms, which is how the reviews word it; it does not say MSG is harmless in any amount.

`CREDITS.md` lists every source. Its takedown notice holds the placeholder `[the contact address the user gives]`: the address is the user's to give (`footage-credits.py --contact`).
