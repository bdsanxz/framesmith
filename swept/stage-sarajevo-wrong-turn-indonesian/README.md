# Sarajevo, one wrong turn — Indonesian

`inspirations/sarajevo-wrong-turn/` re-voiced and re-timed in Indonesian: a 28 s history short (1920×1080) showing the collage half of the `dark archival with animation` style (`references/dark-archival-with-animation.md`). The photos, cut-outs, map, paper, music, and sounds are the English inspiration's. The narration, on-screen text, and every timing are new.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
./make-footage.sh                      # photos, cut-outs, prints, paper, map, music, gunshot -> assets/generated/
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

The narration was made with `scripts/narrate.sh narration.txt "a deep middle-aged man, grave and measured history documentary narrator" . --lang id`:
- **Voice.** VoxCPM2 designed the voice from that description. It saved the voice as `assets/voice-ref.wav` and cloned it for every line.
- **Numbers.** Numbers are written as words ("sembilan belas empat belas"), so the voice reads them the Indonesian way.
- **Transcript.** `assets/transcript.json` is Whisper large-v3 (`-l id`), corrected by `scripts/snap-transcript.py`. Raw, it put every sentence start 0.4–0.6 s early, inside the pause before it. Fitted to the line bounds the engine measured, the starts are exact: 4.44, 7.22, 12.27, 15.56, 20.17, and 24.32 s, where the silences end.

| Scene | Starts on | Shows |
|---|---|---|
| `map.html` | "Dua puluh delapan Juni" (0 s) | the map settles out of a soft focus, the canton fills, the pin drops, SARAJEVO types as it is spoken (2.83 s) |
| `title.html` | "Satu belokan" (4.44 s) | BELOKAN SALAH slapped onto indigo paper; the marker draws under SALAH on "salah" (5.26 s); the route turns |
| `archduke.html` | "Di kursi belakang" (7.22 s) | the cut-out whips in; the name sharpens on "Adipati", underlined on "Ferdinand"; the role types on "pewaris" |
| `assassins.html` | "Di sepanjang rutenya" (12.27 s) | "6" stamped on "enam"; PEMBUNUH MUDA types; the trial print; bars across the eyes |
| `street.html` | "Di Jembatan Latin" (15.56 s) | the car turns on "berbelok"; SALAH ARAH! on "salah"; it brakes on "mengerem"; ring and MESIN MATI on "mati"; Princip pinned on "Gavrilo" |
| `shots.html` | "Ia menembak" (24.32 s) | the pistol; "2 TEMBAKAN" on "dua"; two bursts on the measured shots; flash, then black |
| `paper.html` | 0–25.88 s | the paper sheet multiplied over every scene |

Palette: indigo sheet `#5b6e9e`, saffron accent `#e3a21a`, grey paper `#e3e0d6`, navy ink `#141a2e`. Saffron is light, so the stamp ink is darkened to `#8a5c06` (3:1 on the paper), and the river's label is light on the indigo water.

Technique notes:
- **Fonts.** Anton and Special Elite cover Indonesian (plain Latin), so the fonts are unchanged.
- **Title.** BELOKAN SALAH (Indonesian puts the adjective after the noun) is wider than ONE WRONG TURN. The strip moves 160 px left to stay inside the title-safe box.
- **Cuts and cues.** Every scene starts on a sentence start from the transcript. Each cue moved to the word that carries it in Indonesian word order: the underline waits for "salah", the role for "pewaris".
