# Studio Panels reference project (Sprint 18, T009)

One reference Studio project exercising every Sprint 18 panel behavior
end to end. `tools/n18-studio-proof.sh` stages this directory to a
workdir (plus generated media and a sibling project), so the checkout
itself is never written.

- compositions: `main` (root) plus `b-roll`; the proof lists cards,
  hovers a PNG preview, places `b-roll` into `main`, and queues a
  render-one (stale generations refuse first)
- layers: `hero` (styled text with a tween, a key, and a variable
  binding), `pic` (image), `card` and `badge` (boxes, `badge` holds a
  set); the proof reorders `pic`, groups `card`+`badge`, and ungroups
  again, each as a named history turn with sources byte-identical
- assets: `pic` shows `media/card.png` as used; `media/extra.png`,
  `media/idle.mp3`, `media/spare.png`, and `fonts/Body.ttf` stage as
  unused; the proof imports, inserts (one turn), reveals, removes, and
  renames, with referenced removes refusing by name
- code: the proof lists the tree, reads the file, jumps to `hero`,
  refuses an invalid save keeping the last good bytes, then saves a
  valid edit as a history turn
- variables: `accent` and `tagline` bind onto `hero`; the proof
  declares, previews, copies exact values, and binds, each guarded turn
  recorded
- audio rack: `music` (2 s of generated sine in `s.wav`) carries a
  three-stop `volume` lane; the proof (via the `n18_audio_probe`
  example helper, since the rack is library-only) auditions without
  writing, applies a preset, rewrites the lane, and round-trips a lane
  copy/paste, with refusals named
- clipboard: the proof copies `hero`+`card` from `main` and pastes
  across projects into the sibling's `b` composition, landing renamed
  with styles and motion carried
- renders: the proof queues the render-one MP4 plus a looping GIF,
  waits for both to succeed, and checks the MP4 download is
  byte-identical while the GIF loops as chosen

Media (`s.wav`, `media/card.png`, `media/extra.png`) is generated into
the staged copy by `tools/n18-studio-proof.sh` (Python sine WAV plus
ffmpeg testsrc PNGs); the remaining bytes (`media/idle.mp3`,
`media/spare.png`, `fonts/Body.ttf`) are staged as inert placeholders
the proof removes, renames, or refuses. The sibling project (an empty
root composition `b`) is written by the proof script itself.
