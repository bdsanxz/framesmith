# Studio canvas reference (Sprint 17, T009)

One small project exercising every Sprint 17 canvas, inspector, and
animation outcome through the shared Sprint 15 patch and history
machinery. Proved end to end by `tools/n17-studio-proof.sh`.

- Two boxes (`card` on top, `panel` beneath) for selection,
  transform, snap guides, and crop presets.
- One text clip (`title`) for inline text sessions and inspector rows.
- 320x180, 4 seconds, 10 fps: snap lands on `card left` (x=10) and
  `card center-y` (y=45) from a `[11,44]` drag want.

The checkout copy is never served or rendered into: the proof stages
this directory to a fresh workdir and serves the stage.
