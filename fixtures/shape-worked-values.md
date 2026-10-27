# Shape fixtures: the values their pictures must hold (T062)

Written before any fixture exists. Each fixture is a still (one frame, at 0 s, no motion), and its checked-in picture goes in `fixtures/<name>/expected/frame-0000.png`. The checked-in picture is held to the values below, not only to our own output.

Worked from `docs/spec/shape.md` (sections 3 to 5) by hand, in whole pixels: a pixel's centre is at +0.5, and a pixel is inked when its whole area lies in the shape. Where an edge falls inside a pixel, the value is the coverage worked by an independent model, which is not part of the repository: exact circle membership for the curve, and dash runs with half-disc caps for the line, sampled 1000 by 1000 inside each pixel.

Colours: background `#102040` = (16, 32, 64); ink `#E0A030` = (224, 160, 48). Each channel of a partly covered pixel is `background + coverage × (ink − background)`, rounded.

**Tolerance.** A pixel the model gives as fully inked or fully background is exact (0 levels). A partly covered pixel is within 1 level per channel of the model's value, the bound the text coverage keeps.

## 1. shape-corners

Frame 64 by 64, background. One shape: box left 8, top 8, width 48, height 48; `view-box="0 0 24 24"`; `d="M 0 0 H 24 V 24 H 0 Z"`; `color` ink.

The scale is 48 ÷ 24 = 2 and the view box has no offset, so the square is drawn at 8 to 56 on both axes. Every edge is a whole pixel, so no pixel is partly covered.

- Ink: (8, 8), (55, 8), (8, 55), (55, 55), (32, 32), (20, 40).
- Background: (7, 7), (56, 7), (7, 56), (56, 56), (7, 20), (20, 7), (56, 40), (40, 56).
- Whole frame: 2304 inked pixels (48 by 48); every other pixel is background.

## 2. shape-fill-and-stroke

Frame 64 by 64, background. Two shapes, each in box 0, 0, 64, 64 with `view-box="0 0 64 64"` (scale 1, no offset). They do not overlap.

**Fill:** `d="M 4 4 H 28 V 28 H 4 Z"`, no `stroke-width`. The fill covers 4 to 27 on both axes: 576 pixels.
- Ink: (4, 4), (27, 27), (15, 15).
- Background: (3, 3), (28, 28), (3, 15), (15, 28).

**Stroke:** `d="M 36 36 H 60 V 60 H 36 Z"`, `stroke-width="4"`. Each side of the square is a line of width 4 centred on its path: the outer edges are at 34 and 62, the inner edges at 38 and 58. The centre line of the left side is at 36, with two pixels on each side (34, 35 and 36, 37). The outer corner is mitred, so it is square at (34, 34) and (61, 61); the inner corner is at (38, 38).
- Ink: (34, 40), (37, 40), (58, 40), (61, 40), (45, 34), (45, 37), (45, 58), (45, 61), (34, 34), (61, 61).
- Background: (33, 40), (38, 40), (57, 40), (62, 40), (45, 33), (45, 38), (45, 57), (45, 62), (38, 38), (57, 57).
- Inked pixels: 28 × 28 − 20 × 20 = 384.

**Whole frame:** 576 + 384 = 960 inked pixels, and every other pixel is background.

## 3. shape-view-box-scaled

Frame 64 by 64, background. One shape: box left 8, top 8, width 48, height 48; `view-box="-6 -6 12 12"`; `d="M 0 0 H 6 V 6 H 0 Z"`; ink.

The scale is 48 ÷ 12 = 4. The view box fits the box evenly and has no slack, so the offset is 8 + 6 × 4 = 32 on both axes. The path's square 0 to 6 is drawn at 32 to 56, so it is the box's lower-right quarter.

- Ink: (32, 32), (55, 55), (40, 45).
- Background: (31, 31), (56, 56), (31, 40), (40, 56), (8, 8), (20, 20).
- Whole frame: 576 inked pixels (24 by 24), and every other pixel is background.

## 4. shape-dashes

Frame 64 by 64, background. One shape: box 0, 0, 64, 64; `view-box="0 0 64 64"` (scale 1); `d="M 8 16 H 56"`; `stroke-width="4"`; `dash="10 5"`; ink.

The line is 4 wide about y = 16, so it covers rows 14 to 17 (rows 13 and 18 are background along the whole line). Its length is 48, from x = 8 to x = 56. Along the path, the pattern is on for 0 to 10, off for 10 to 15, and so on, so the runs are:

| Run | Distance along path | Columns (x) | Ends |
|---|---|---|---|
| dash 1 | 0 to 10 | 8 to 17 | start: round cap |
| gap 1 | 10 to 15 | 18 to 22 | none |
| dash 2 | 15 to 25 | 23 to 32 | butt |
| gap 2 | 25 to 30 | 33 to 37 | none |
| dash 3 | 30 to 40 | 38 to 47 | butt |
| gap 3 | 40 to 45 | 48 to 52 | none |
| dash 4 | 45 to 48 | 53 to 55 | end: round cap (the pattern is on at 48) |

The round caps are discs of radius 2 at (8, 16) and (56, 16). Columns 6 and 7 are partly covered at the start, and columns 56 and 57 at the end (see below). Columns 6, 7, 56 and 57 are not counted in any total.

- Ink (exact): (8, 14), (8, 17), (12, 14), (12, 15), (12, 17), (17, 15), (23, 15), (32, 15), (38, 15), (47, 15), (53, 15), (55, 15).
- Background (exact): (5, 16), (12, 13), (12, 18), (18, 15), (22, 15), (33, 15), (37, 15), (48, 15), (52, 15), (58, 16), (30, 30).
- Caps, row 15 (model): (6, 15) = (206, 149, 49); (7, 15) = ink; (56, 15) = ink; (57, 15) = (206, 149, 49). The model gives column 6 with coverage 0.913, so the half-disc is 91 % covered there.

## 5. shape-curve

Frame 100 by 100, background. One shape: box 0, 0, 100, 100; `view-box="0 0 100 100"` (scale 1); `d="M 10 50 A 40 40 0 0 1 90 50 A 40 40 0 0 1 10 50 Z"`; a fill in ink.

The shape is a circle of radius 40 centred at (50, 50).

- Ink (exact): (11, 50), (89, 50), (50, 11), (50, 89), (50, 50).
- Background (exact): (9, 50), (90, 50), (50, 9), (50, 90), (15, 15), (10, 40), (10, 60), (40, 10), (60, 10).
- Edge pixels (model): (10, 50) = (223, 159, 48); (50, 10) = (223, 159, 48); (89, 50) = (223, 159, 48); (50, 89) = (223, 159, 48). Coverage 0.9959 at each.
- True left edge at pixel-row centres, for the edge check: y 50.5 gives x 10.0031; y 60.5 gives x 11.4027; y 70.5 gives x 15.6525; y 80.5 gives x 24.1205. Each row's edge in the picture is within 0.25 pixel of its true x.
- Largest flat piece: a straight piece of the drawn curve may not be longer than 8.93 pixels at this radius (the longest chord within 0.25 pixel of a circle of radius 40: 2 × 40 × sin(θ / 2) with cos(θ / 2) = 1 − 0.25 ÷ 40).

## 6. shadow (the offset and the blur of a shadow)

Worked by hand from `shape.md` section 8, with the blur of `text.md` section 6.6 rule 7 (three passes, each a horizontal average and then a vertical one, rounded to the nearest level with halves up). The shadow is the layer of a shape alone, premultiplied, before any picture is laid under it.

Case A, the offset: frame 48 by 48, nothing under the shape. One shape: box 0, 0, 48, 48; `view-box="0 0 24 24"` (scale 2); `d="M 4 4 H 20 V 20 H 4 Z"`, filled in ink. `drop-shadow="1 1 0 #000000"`: one view-box unit is 2 pixels, so the shadow covers pixels 10 to 41 on each axis, and the shape covers pixels 8 to 39.

- Ink (exact): (9, 9), (10, 10), (39, 39).
- Shadow only, black at alpha 255 (exact): (40, 40), (41, 41), (41, 10).
- Clear (exact): (42, 42), (9, 42), (5, 5).

Case B, the blur: frame 64 by 64, nothing under the shape. One shape: box 0, 0, 64, 64; `view-box="0 0 64 64"` (scale 1); `d="M 16 16 H 48 V 48 H 16 Z"`, filled in ink. `drop-shadow="0 0 2 #285078"`. The blur is 2 pixels, so the standard deviation is 1 and the span is `floor(1 * 3 * sqrt(2 * pi) / 4 + 0.5)` = 2: each pixel becomes the average of itself and the pixel after it. Row 32 lies far from the top and bottom edges, so the vertical passes leave it as the horizontal ones make it:

- Pass 1: x 14 0, x 15 128 (127.5 rounds up), x 16 255.
- Pass 2: x 13 0, x 14 64, x 15 192 (191.5 rounds up), x 16 255.
- Pass 3: x 12 0, x 13 32, x 14 128, x 15 224 (223.5 rounds up), x 16 255.

- Shadow alpha along row 32 (exact): x 12 0, x 13 32, x 14 128, x 15 224; the shape's ink from x 16 covers the shadow.
- Premultiplied colour at (15, 32) is `#285078` times 224 / 255, to the level: (35, 70, 105, 224).

## Method and limits

- The exact pixel values (corners, fill and stroke, view box, dash runs) are worked by hand from the integer geometry of `shape.md` sections 3 to 5.
- The partly covered values (the curve's edge and the two caps) come from the model. The model is independent of `draw-cpu`: it uses the circle's equation and the dash pattern, and not the path reader, the flattening or vello.
- The checked-in pictures are held to these values on the processor. The card is held to the processor's picture separately (`crates/draw-gpu/tests/card_shapes_agree_with_the_processor.rs`), within the agreement of `card-renderer.md` section 4.
- These fixtures are not yet rendered. Until notation and scene (after Bima's T092 hand-over) can draw a `<shape>`, no picture exists, and these values are the only check on the pictures when they are first made.
