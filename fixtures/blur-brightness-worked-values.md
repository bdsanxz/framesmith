# Blur and brightness tweens: the values their pictures must hold (T067)

Written before any fixture exists. Each fixture is a one-second composition on a 64 by 64 frame at 30 frames a second. Its checked-in pictures go in `fixtures/<name>/expected/frame-NNNN.png` for the frames listed in the table at the end, and each checked-in picture is held to the values below, not only to our own output.

Times and tweens: frame `n` is at time `n ÷ 30` s. A linear tween from 0 s to 1 s has progress `p = n ÷ 30`, so its value at frame `n` is `start + (end − start) × n ÷ 30`. A one-second composition has frames 0 to 29, so the last frame of a tween is frame 29, at progress 29 ÷ 30.

Frame and element: background `#102040` = (16, 32, 64). Each fixture has one box, left 16, top 0, width 40, height 64, so it covers columns 16 to 55 and every row, coloured `#E0A030` = (224, 160, 48) (the ink). Its edges are whole pixels while it is sharp, so an unblurred pixel is exact.

Worked from `docs/spec/render-command.md` rule 11, `docs/spec/motion.md` and `docs/spec/notation.md` section 8.1: a blur of standard deviation σ is a Gaussian blur of the element's layer, which is transparent outside the frame; brightness multiplies each premultiplied colour and stops at white; the layer is composited over the background and rounded once, halves up. The values come from an independent model of those rules, which is not part of the repository and does not use the draw code. The rows used are row 32, the middle row, so the frame's top and bottom edges are at least 32 rows away; the model includes their clearing for the blur of 20.

**Tolerance.** A pixel the model gives as the background or the ink to the level is exact (0 levels). Any other pixel is within 1 level per channel of the model's value, the bound `shape-worked-values.md` keeps for partly covered pixels.

## Blur of 20, at the start of blur 20 to 0 (frame 0 of `blur-tween-linear`)

Row 32, columns 0 to 40 in steps of 4: x 0 (56, 57, 61); 4 (67, 64, 60); 8 (80, 71, 59); 12 (93, 79, 58); 16 (106, 87, 57); 20 (118, 95, 56); 24 (128, 101, 55); 28 (136, 106, 55); 32 (141, 109, 54); 36 (142, 110, 54); 40 (140, 108, 54).

## Blur of 10, the edge profile (frame 15 of `blur-tween-linear`, frames 15 and 29 of `blur-set-at-an-instant`)

Row 32, columns 8 to 24: x 8 (63, 61, 60); 9 (70, 65, 60); 10 (76, 69, 59); 11 (84, 74, 59); 12 (91, 78, 58); 13 (99, 83, 58); 14 (107, 88, 57); 15 (116, 93, 56); 16 (124, 98, 56); 17 (132, 104, 55); 18 (140, 109, 54); 19 (148, 113, 54); 20 (156, 118, 53); 21 (163, 123, 53); 22 (170, 127, 52); 23 (177, 131, 52); 24 (183, 134, 51).

The edge sits between columns 15 and 16. The value at 15 and 16 is the one that tells a blur of 10 from its neighbours.

## Blur of 2/3, near the end of blur 20 to 0 (frame 29 of `blur-tween-linear`)

Progress 29 ÷ 30 gives 20 × 1/30 = 2/3. Row 32, columns 13 to 18: x 13 (16, 32, 64); 14 (19, 34, 64); 15 (63, 61, 60); 16 (177, 131, 52); 17 (221, 158, 48); 18 (224, 160, 48). Columns 19 to 24 are ink.

## Blur of 2, the start of blur 2 to −2 (frame 0 of `blur-tween-held-at-zero`)

Row 32, columns 8 to 20: x 8 (16, 32, 64); 9 (16, 32, 64); 10 (17, 32, 64); 11 (19, 34, 64); 12 (24, 37, 63); 13 (38, 46, 62); 14 (63, 61, 60); 15 (99, 83, 58); 16 (141, 109, 54); 17 (177, 131, 52); 18 (202, 146, 50); 19 (216, 155, 49); 20 (221, 158, 48).

## Sharp edges (every chosen frame that is not blurred)

Row 32: x 15 is the background (16, 32, 64) and x 16 is the ink (224, 160, 48), exact; x 55 is the ink and x 56 the background, exact.

## 1. blur-tween-linear

Box `blur="20"` at 0 s, with a `<tween>` of `blur` to 0 over 1 s. Frames 0, 15 and 29.

- Frame 0: the blur of 20 table above.
- Frame 15 (the middle of a linear tween, progress 0.5, blur 10): the blur of 10 edge profile above. This is criterion 1.
- Frame 29: the blur of 2/3 values above.

## 2. blur-set-at-an-instant

Box `blur="0"` (sharp), with a `<set at="0.5" blur="10"/>`. The set lands on frame 15, as the set at 0.5 s does in `motion-set-at-an-instant`. Frames 0, 14, 15 and 29.

- Frames 0 and 14: sharp, the sharp edges above. Frame 14 is the last before the set.
- Frame 15: the blur of 10 edge profile above. Frame 14 to frame 15 is the change from sharp to blurred. This is criterion 3.
- Frame 29: the same blur of 10 values as frame 15, since a set holds.

## 3. blur-tween-held-at-zero

Box `blur="2"` at 0 s, with a `<tween>` of `blur` to −2 over 1 s. The linear value is 0 at frame 15 and below 0 from frame 16 on: −0.9333 at frame 22 and −1.8667 at frame 29. Those frames are held at 0. Frames 0, 15, 22 and 29.

- Frame 0: the blur of 2 values above.
- Frames 15, 22 and 29: sharp, the sharp edges above. A picture that took the absolute value of a negative blur would be blurred at frames 22 and 29, so these frames must equal the sharp picture. This is criterion 4.

## 4. brightness-tween-linear

Box `brightness="1"` at 0 s, with a `<tween>` of `brightness` to 0 over 1 s. Frames 0, 15 and 29. The background is (16, 32, 64) in every frame, because brightness applies to the element and not to what is behind it.

Interior probes (30, 10) and (40, 50), both in the box, which is sharp and so exact:

- Frame 0 (brightness 1): (224, 160, 48).
- Frame 15 (brightness 0.5): (112, 80, 24), each colour halved. This is criterion 2.
- Frame 29 (brightness 1/30): (7, 5, 2), from 224 ÷ 30 = 7.47, 160 ÷ 30 = 5.33 and 48 ÷ 30 = 1.6, each rounded.

## 5. brightness-tween-held-at-zero

Box `brightness="1"` at 0 s, with a `<tween>` of `brightness` to −1 over 1 s. The linear value is 0 at frame 15 and below 0 from frame 16 on: −0.4667 at frame 22 and −0.9333 at frame 29. Those frames are held at 0. Frames 0, 15, 22 and 29.

- Frame 0: interior (224, 160, 48).
- Frames 15, 22 and 29: interior (0, 0, 0). The box is opaque, so it is black over the background. A picture that took the absolute value would show about (209, 149, 45) at frame 29. This is criterion 4.
- Background (16, 32, 64) at every frame.

## Chosen frames

| Fixture | Chosen frames | Criterion |
|---|---|---|
| `blur-tween-linear` | 0, 15, 29 | 1 |
| `blur-set-at-an-instant` | 0, 14, 15, 29 | 3 |
| `blur-tween-held-at-zero` | 0, 15, 22, 29 | 4 |
| `brightness-tween-linear` | 0, 15, 29 | 2 |
| `brightness-tween-held-at-zero` | 0, 15, 22, 29 | 4 |

Criterion 5 is met when each chosen frame of each fixture equals its checked-in picture to the byte. Criterion 6 (the card) is held to the processor's picture, and is not worked here.
