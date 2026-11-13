# Split-flap board — a live widget rebuilt as a function of time

A split-flap display (a train-station board) found as a live web component, rebuilt as a renderable composition from the idea alone (`references/sampling-the-web.md`, door 2), with the message "SAMPLE THE CRAFT, NOT THE PROPERTY." `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

**Media.** This inspiration uses no media files: every picture is drawn in HTML, CSS, or SVG, it has no sound, and its font (JetBrains Mono) is among the families the renderer bundles, so it renders as it is.
What was sampled (the craft): tile proportions, dark tiles with off-white glyphs, and the mechanic where the old top half folds down and the new bottom half lands to reveal the next letter, cycling through letters before settling.

How it became deterministic (the engine swap):
- A live widget animates on a clock, so two playbacks differ. Here every flip is a pure function of time: tile i flips at `start_i + k × 0.09s`, with `start_i` and its intermediate letters drawn from a seeded PRNG (mulberry32, seed 15).
- Each tile has four half-panels: static top and bottom, and two flaps. A flip is `tl.set` text swaps plus two half-step `rotationX` tweens (top flap 0 → −90°, then bottom flap 90° → 0) with `perspective` on the tile.
- `tl.set` and `tl.to` repeated on the same flap are fine. Only repeated `fromTo` needs a baseline.
- Each half clips half a glyph by design (`data-layout-allow-overflow` on the tile), and flaps cover the static halves (`data-layout-allow-occlusion` on each glyph span; occlusion allowances only count on the text element itself).
- The board settles by ~2.5s, and a slow push keeps the long hold alive.

The rebuild contains none of the original widget's code or brand: sample the craft, not the property.
