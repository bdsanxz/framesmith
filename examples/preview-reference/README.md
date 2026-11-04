# Preview reference (Sprint 14, T007)

One composition proving every Sprint 14 outcome end to end: exact
frames on demand, transport bars, transport with display, the render
mix as the playback clock, hot reload keeping the playhead, and exact
daemon management — all behind one proof command (see below).

## Contents

- [`composition.kdl`](composition.kdl) — the reference composition.
- [`tone.wav`](tone.wav) — 1s of 48 kHz stereo 16-bit 440 Hz tone
  (copied from `../check-reference/tone.wav`; deterministic bytes, no
  encoder needed). The `tone` clip plays it audibly, so the mix is a
  real sounding track and `play` reports `audio=sound`.
- Proof: [`n14-preview-proof.sh`](../tools/n14-preview-proof.sh)
  (one command, see below). The browser half is driven by
  [`n14-preview-browser.py`](../tools/n14-preview-browser.py), which
  needs only the Python standard library plus a local
  Chrome/Chromium binary.

## The composition

`preview`, 320x180, 2s, 10 fps, background `#112233`, zero lint
findings:

- Motion: box `slide` (10,60 40x40, 0-2s) plus
  `tween target=0 property="x" ... to=200` moves 10 to 200px over the
  full 2 seconds, so every frame differs from its neighbours.
- Text: `title` ("Preview", 10,20 size 28, white) stays clear of the
  caption band with contrast to spare.
- Audio: clip `tone` (the 1s tone at 0-1s) makes the mix sound for
  the first second; the second second is honest mix silence.

## Proof

Run [`n14-preview-proof.sh`](../tools/n14-preview-proof.sh) (one
command): it stages this folder, starts the preview daemon under an
isolated registry, requires served frames byte-identical to the same
`snapshot` frames, requires warm step/scrub/jump medians inside
50/50/100 ms labeled warm versus cold, drives the review page in a
real headless browser through step, scrub, loop, speed, play, pause,
and the mix-clock sync with per-control timings, requires play to
follow the sounding mix, lands a valid hot reload inside 500 ms with
the playhead kept (and the new look equal to a fresh snapshot),
requires a broken edit to report diagnostics while last good frames
keep showing, checks status/list/stop/stop-all lifecycle counts
including already-stopped and empty stop-all, refuses a tampered copy
before any server starts, and reports PROOF: PASS only when every
check succeeds (exit 0). Any single failing check prints FAIL and the
gate exits 1 (no false pass).

Evidence is copied to `evidences/proof-n14-T007/` on every run.
