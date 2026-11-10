# Captions reference (Sprint 22, T004)

One composition proving every Sprint 22 outcome end to end: a
captions track with per-word timing, SRT/VTT/JSON import, track
styles with a per-word override, entrance/highlight/exit presets,
behind-subject compositing through a subject matte, validation and
lint gates, and drift-free export round-trips. The one-command gate
is [`tools/n22-captions-proof.sh`](../tools/n22-captions-proof.sh).

## Contents

- [`composition.kdl`](composition.kdl) — the reference composition:
  one `captions` clip (`talk.srt`, styled, one override, all three
  preset families) on a 160x90, 4s, 30fps film.
- [`talk.srt`](talk.srt) / [`talk.vtt`](talk.vtt) /
  [`talk.json`](talk.json) — the same three cues with identical
  word times in all three import formats (SRT, VTT, framesmith
  words JSON). Cue windows: `[0.5, 1.5)` "hello world",
  `[2.0, 3.0)` "goodbye moon", `[3.2, 4.0)` "framesmith captions".
- [`matte.png`](matte.png) — the subject matte for the
  behind-subject order: left half opaque white, right half fully
  transparent, canvas-sized (160x90) so cover/show-through reads
  off exact pixels.
- [`README.md`](README.md) — this file.

## The composition

The `captions` clip carries the track style (`font-size=16`,
`color="#ffd60a"`, 2px black outline, `#141414` backing,
`max-lines=2` on the `rail`), one project-local override (cue 0,
word 1 in `#ff3366` at 1.25x), and one preset per family
(`entrance="fade-up"`, `highlight="pop"`, `exit="fade-out"`).
Presets are pure functions of film time: the same film time
renders the same pixels on every run.

## Proof map (what the gate checks)

| Gate | Proves | How |
| --- | --- | --- |
| word-correct seeks | cue frames paint words, the gap is the bare plate, cues differ | snapshots at 1.0s/2.5s/3.5s vs 1.7s vs bare project, `cmp` |
| import identity | SRT, VTT, JSON decode to identical times | same seeks on `src`-swapped copies, `cmp` |
| styled + preset frames | style/override/presets paint, reruns identical | reference vs unstyled/no-preset copies differ; double snapshot + double render `cmp` equal |
| behind-subject cover | opaque matte covers, transparent shows through | `behind-subject` + `matte.png` copy: left half all white, right half equals the plain render |
| gated failure | overlap refuses naming the file | tampered copy: `check`/`render` exit non-zero naming `talk.srt`, no MP4 |
| export round-trips | export then re-import drifts by zero | `n22_captions_proof` test on the staged copy |
| inspector state | readiness + active cue, nothing invented | `n22_captions_inspector` tests |

`matte.png` is unreferenced by the default composition on
purpose: `check` stays green on checkout, and the proof stages a
`behind-subject` copy to prove the matte order.
