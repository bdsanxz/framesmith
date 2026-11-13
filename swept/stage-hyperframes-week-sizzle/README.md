# This week in HyperFrames — beat-synced sizzle

A 16s typographic sizzle cut to a music track's beat map, following the music-to-video method in `references/music-to-video.md`. The copy recaps the first four lessons of a course (install, motion language, PR video, beat cut), so nothing on screen is invented. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`, and a draft render has video and audio streams of exactly 16.0s.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with.

```bash
python3 ../../scripts/synth-drum-track.py assets/bgm.wav   # synthesized 120 BPM drums, stdlib only, deterministic
python3 ../../scripts/analyze-beatgrid.py assets/bgm.wav -o audiomap.json --print   # installs librosa and soundfile itself on first use
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/video.mp4
```

The WAV is not committed because the script regenerates it byte for byte. `audiomap.json` is committed, so the composition can be checked without re-running the analyzer. Its numbers are the timing truth: the analyzer reads the synthesized 120 BPM track as 117.5 BPM, with downbeats at 2.02 / 4.017 / 6.037 / 8.034 / 10.031s, and every cut uses those values verbatim rather than the "intended" 2.0 / 4.0 / 6.0.

| Frame | Span (s) | Pacing | Anchors used |
|---|---|---|---|
| `01-cold-open` | 0–2.02 | phrase_flow | words on the kick @0 and grid beats @0.534, @1.533 |
| `02-cards` | 2.02–10.031 | beat_cut | 4 cards hard-cut on the downbeats; lime pill punches on the next grid beat; the SURGE downbeat @4.017 gets the palette flip |
| `03-roll-counter` | 10.031–14.0 | beat_cut | phrase-seam flip; a DAY counter steps 05 → 30 on each of 25 roll events from `audiomap.events` |
| `04-banner` | 14.0–16.0 | phrase_flow | lime banner sweeps on the final hit @14.0 and holds through the hard stop @15.0 and the silence |

`frame.md` encodes the look the brief asked for: lime on sage, Inter 900, one lime element per card, hard 0ms cuts.
