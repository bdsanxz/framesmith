# Seeked, not played — talking-head recut

An 18s talking-head clip packaged with designed cards synced to the transcript: a kinetic title, two stat panels in a split-screen, a quote card, a kinetic close, a persistent URL bug, and word-synced captions with every graphic kept above them. The clip itself plays untouched. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with.

There is no real footage or avatar here, so the A-roll is a stand-in rendered from the narration's own transcript (the presenter's mouth opens on every word). Its narration is a female voice (macOS `say`), kept on purpose so the inspirations include one; a new video stays male unless the user asks for another. Recreate it, then check:

```bash
node ../../scripts/make-standin-aroll.mjs assets/transcript.json assets/narration.mp3 footage/aroll.mp4
npx hyperframes@0.8.103 check
```

`assets/narration.mp3` is not shipped, so the stand-in comes out silent, its mouth still moving on the words; to give it a voice, speak `narration.txt` again with `../../scripts/narrate.sh` and pass the narration it writes (re-time the cards when the transcript changes). `../../scripts/render-inspirations.sh seeked-not-played-recut` builds the stand-in and renders a draft. To use real footage instead, re-encode it with one keyframe per second of frames (`-g 30 -keyint_min 30`), transcribe it, correct the transcript, and re-time the cards from the new word starts.

| Piece | Where | Timing source |
|---|---|---|
| Card plan | `storyboard.json` (the format in `references/talking-head-recut.md`: id, intent, start/end, zone, cues, content) | word starts in `assets/transcript.json` |
| Speaker moves | main timeline in `index.html` tweens the untimed `#video-wrap` (slide to the left half @2.6, back @9.4, push-in @15.25) | a sub-composition cannot reach host-root media |
| Cards | `compositions/cards/*.html`, mounted with `data-track-kind="graphics"` | card-local = global − card start |
| Captions | `compositions/captions.html` from `scripts/word-captions.mjs --text captions.txt` | ASR timing, corrected spelling ("HyperFrames", "seeked", "families") |
| Audio | separate `<audio>` on the same MP4; the `<video>` is muted | — |

Two bugs found while building it are now rules in `references/composition-contract.md` and `references/cli.md`: the host's `#root` style must be scoped to `#root[data-composition-id="main"]`, and Studio stamps `data-hf-id` into source files when the preview is open.
