# One take, one wire

A 17.5 s narrated explainer shot as **one continuous camera move**: no cuts. A single wide world holds four regions side by side (markup, chart, waveform, render). One wire runs through all of them and changes role on the way: code line → chart baseline → waveform → connector into the finished file. A chip rides the wire's drawing head, and a ruler outside the camera tracks playback time. Every move starts on a spoken word from `assets/transcript.json`. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103` (0 errors, 0 warnings).

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with.

`PROMPT.md` is the single binding prompt for the film, written in the six-decision form; `scripts/prompt-check.mjs PROMPT.md` reports all six present.

| Cue (word start) | Camera leg | What develops |
|---|---|---|
| 0.01 "Every" | opens punched in at 1.7× | the markup line types, with a stepping caret |
| 2.82 "watch" / 4.33 "wire" | pull back to 1× / pan right | the caret goes out, the chip appears, and the wire draws down to the baseline |
| 5.72 "chart" / 6.56 "bars" | holds | title, then six bars stand up off the wire: the real seconds per beat of this film |
| 7.53 "then" / 8.57 "waveform" / 9.41 "voice" | pan + push to 1.1× | the waveform grows out of the wire (scaleY 0 → 0.35), then swells to full height |
| 10.51 "now" / 11.47 "walks" | whip (0.6 s, `power4.inOut`) | frame cells light; the counter runs to 525 (= 17.5 s × 30 fps) |
| 13.07 "the wire ends" / 13.84 "finished" | landing push to 1.2× (`power4.out`) | the chip reaches the file card; the take rests on the result |

Re-skin the whole take with one flag (the scene declares no variables, so it reads the host's):

```bash
npx hyperframes@0.8.103 render -q draft --variables '{"accent":"#ff9f43","canvas":"#17120e"}' --strict-variables -o renders/amber.mp4
```

Build notes:
- **Camera math**: one `cam {cx, cy, s}` object and one `applyCamera()` writer, with the world's `transform-origin: 0 0`. Tweening the center and scale keeps the target centered through zooms.
- **The draw head and the chip share one progress value** (arc length). Arc length at each region boundary is measured once at setup with `getPointAtLength`, so the stroke and the chip never disagree.
- **The caret steps with the characters** (`tl.set` per character at 20.4 px, the monospace advance) and goes out when the line becomes the wire. Left on, it sat over the last character and `check` flagged `text_occluded`.
- **The first landing pose cropped the counter mid-word, and the next one put the region label under the ruler.** The final pose (`cx 6650, cy 500, s 1.2`) frames the counter, the strip, and the card whole. Check every resting pose in a snapshot.
- `check` reports `connector_orphan` as *info* while the wire is drawn and its end card is off stage. That is expected for a progressive thread.
- The narration is a female voice on purpose (macOS `say`, Samantha), so the inspirations show a female voice as well as the male default; a new video stays male unless the user asks for another. Re-running `narrate.sh` with another voice gives a new transcript; update the cue times to it.
