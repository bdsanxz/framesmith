# Look reference (Sprint 11, T008)

One composition proving every Sprint 11 outcome end to end: graded
image, imported and baked LUTs, named media effects, an ordered VFX
chain, a transition bridge, and shuttered motion blur — with a
byte-identical re-render and a baked LUT that re-imports
pixel-identically.

## Contents

- [`composition.kdl`](composition.kdl) — the reference composition.
- [`tune.cube`](tune.cube) — the source LUT (size 2, swaps red and
  blue), imported at half amount by the composition grade.
- [`baked.cube`](baked.cube) — the composition grade chain baked with
  `framesmith grade-bake` (size 33); the third clip re-imports it at
  full amount. Re-baking the reference must reproduce these bytes
  exactly (the proof script checks with `cmp`).

## The composition

- Composition grade (all color-only, hence bakeable): `teal-orange`
  preset, contrast/saturation adjusts, a `wheel`, a `curve`, an `hsl`
  selection, and the `tune.cube` LUT at amount 0.5.
- Clip `a` (moving box): full detail grade (wheel, curve, HSL,
  vignette, grain), two effects (`grayscale`, then `sepia`), and a
  three-stage VFX chain (`noise`, `warp`, `displacement`); a tween
  carries it across the frame in the first half second.
- Clip `b` (full-frame blue): the transition target.
- Third clip: a small box reusing `baked.cube` at amount 1.
- `transition from="a" to="b" duration=0.5 kind="fade"` borrows
  handles without changing the 90-frame timeline.
- `motion-blur shutter=360 samples=8 tier="high"` smears the moving
  box; still corners decode identically with blur off.

## Proof

Run [`n11-look-proof.sh`](../../tools/n11-look-proof.sh) (one
command): it stages this folder, checks green, renders twice and
requires byte-identity, re-bakes and requires byte-identity, proves
the baked LUT renders pixel-identical to the tuned chain, proves the
transition keeps the frame count, proves the blur smears motion,
refuses every invalid look variant before frame one, and archives
the MP4 plus versions and date under
`evidences/proof-n11-T008/`.
