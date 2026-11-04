# Check reference (Sprint 13, T008)

One composition proving every Sprint 13 outcome end to end: static
layout findings, motion assertions plus frozen-span detection,
contrast and audio-health findings, pixel-exact snapshots, keyframe
listing, render comparison, grade comparison, project health, machine
benchmark, and release gating under strictness — with a default
render that still ships with warnings intact.

## Contents

- [`composition.kdl`](composition.kdl) — the reference composition.
- [`tone.wav`](tone.wav) — 1s of 48 kHz stereo 16-bit 440 Hz tone.
  Both audio clips discard it (one hidden, one zero-volume), so the
  mix measures digital silence. Deterministic bytes, no encoder
  needed.
- Proof: [`n13-check-proof.sh`](../../tools/n13-check-proof.sh)
  (one command, see below).

## The composition

`review`, 640x480, 6s, 10 fps, background `#112233`, graded with the
`noir` preset (so `grade-compare` has something to split). Twelve
lint findings, zero notes, across eleven of the thirteen rules:

- Overlap: boxes `lower` (100,100 200x120, 0-2s) and `upper`
  (200,160 200x120, 0-2s) share 6000px² (100x60px) on one track and
  time → `overlap`, naming both ids and the area.
- Off-canvas: box `lost` (-300,100 200x120) lies fully outside the
  frame, 100px past the left edge → `off-canvas` with its bounds.
- Caption touch: text `cap` (100,440 size 32, white) reaches into the
  caption band (bottom 12%, y>=422.4px) → `caption-zone` naming the
  band.
- Late appearance: text `late` (starts at 3s) carries
  `assert-appear by=2` → `appear-by` with the measured 3s.
- Escape: box `escape` (600,100 100x60, partially past the right
  edge) carries `assert-inside` → `stay-inside` with the measured
  60px escape. Partial, so `off-canvas` stays silent.
- Stillness: box `stuck` (100,300 100x60, 2-4s, no motion) carries
  `assert-moving` → `keep-moving` with the sampled stillness.
- Frozen span: motion runs only in the first second (the `slide`
  tween below), so frames 1s-5.9s are identical → `frozen-timeline`
  with its time range, no assertion needed.
- Low contrast: text `dim` (#777777 on #112233, 3.6:1) misses WCAG
  AA 4.5:1 → `contrast` with the measured ratio and the bar. The
  white `cap` and `late` texts stay silent.
- Silent mix plus discards: `dead-hidden` (hidden) and `dead-quiet`
  (volume `-inf`) → two `discarded-audio` findings naming each clip
  and cause, plus `silent-audio` (measured peak -inf dBFS, below the
  -60 dBFS bar).
- Unused variable: `stale` is declared but never bound →
  `unused-variable`. No unused style or theme is seeded.

Motion probe: box `slide` (first clip, 0-1s) plus
`tween target=0 property="x" ... to=140` moves 0 to 140px over the
first second, so `keyframes --clip slide --property x` reports
`x = 14 * frame` (frame 5 → 70) while the frozen span still covers
the rest of the timeline.

## Proof

Run [`n13-check-proof.sh`](../../tools/n13-check-proof.sh) (one
command): it stages this folder, checks green twice with identical
finding sets, proves `lint` parity, snapshots three times and
requires decoded-pixel identity with the same full-render (`pngseq`)
frames, lists keyframes and requires frame 5 at x=70, compares the
reference against an identical copy (SSIM 1.0, pass), splits graded
and ungraded halves (halves must differ), reports `doctor` and
`benchmark` facts, gates `check` and `render` under `--strict` and
`--strict-all` (errors naming the elevated rules, no output
written), ships the default render twice with byte identity and
warnings intact, refuses tampered copies, and reports PROOF: PASS
only when every check succeeds (exit 0). Any single failing check
prints FAIL and the gate exits 1 (no false pass).

Evidence is copied to `evidences/proof-n13-T008/` on every run.
