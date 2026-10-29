# Still pictures

Status: in force from Sprint 1 (ticket T059), for the element `<image>`: how its file is read, how its box is placed and sized, how the picture is fitted into the box, how it is drawn on both renderers, and what stops a run. Sources: `notation.md` sections 6.6 (the attributes of `<image>`), 7.1 (place and size, and the size an image takes when none is written), 7.10 (`object-fit` and `object-position`) and N3 (the kinds read); frx7's `docs/spec/os-codecs.md` section 4 (the facts, the size and the pixels of a still picture, its kinds, its transparency, its stored turn and its errors), reached through frx7's public interface `codec::Registry` only; `render-command.md` section 4 (the picture, the src-over operation, drawing under a map); `card-renderer.md` sections 4 and 13; the W3C's CSS Images Module Level 3 for the ideas of `object-fit` and `object-position` (fill, cover, contain; a position as a share of the room left over); the PNG and JPEG standards that frx7's spec names; for the filter of section 5, the standard sampling idea of a tent (triangle) filter, widened when a picture is made smaller, written here in our own terms. Decision P10 of `DECISIONS.md` (a picture with a stored turn is not drawn in this version).

## 1. What is taken

An `<image>` takes, in this version: `id`, `class`, `start`, `duration`, `left`, `top`, `right`, `bottom`, `width`, `height`, `position`, `src`, `object-fit` and `object-position`. Its children may be none. Every other attribute of `notation.md` section 6.6, written on an `<image>` or given to it by a style, stops the run on its line. A mask made from a picture is reported unsupported (`FS-0035`) with the entry that names it (section 14, S18). The clip is reported unsupported with its entry (S41, T061), a background with its entry (S59), and a `<tween>` or a `<set>` inside an `<image>` likewise (S51). Any other attribute this version does not draw — the colour filters, any other mask (T060), the borders, corners, shadows and the cut of a box, the movement attributes, `margin`, `align-self`, `flex-grow` and `max-width` — is reported invalid (`FS-0030`), since no entry of the support table names it (`check-command.md` section 3d). An `<image>` that is an item of a row, a column or a set of columns is reported unsupported on its line: in this version an image is placed by its edges (`notation.md` section 7.1) in a box that does not lay out its children, or in the root.

## 2. The file

1. **Where.** `src` is a path from the project folder, or a path that begins with `/` (`notation.md` N3 and section 12), resolved as a font's `src` is. An `<image>` without `src` is the structure's report of `notation.md` (`<image> needs the attribute src`).
2. **What is read.** The file is read through frx7: first its facts (`codec::Registry::still_facts`), then its pixels (`codec::Registry::still`), once for each file in a run, whatever number of elements name it. Framesmith never looks at the bytes of the file itself: what kind it is, its size and whether it is damaged are frx7's to say.
3. **The kinds read** are the kinds frx7's spec lists as read, PNG and JPEG (`notation.md` N3). The kind is told from the content of the file.
4. **The pixels** are frx7's: RGBA of 8 bits a channel, not multiplied by alpha, rows top first, with no colour management. A picture that stores no transparency has every alpha 255. Before drawing, each pixel is multiplied by its alpha: each colour value becomes `round(value * alpha / 255)`, halves up, and alpha is kept. A pixel of alpha 0 is then (0, 0, 0, 0).

## 3. What stops a run

Each of these is found before the first frame, by `check` and by `render` alike, and is reported on the line of the `<image>` with its `src` in backquotes and the reason. `render` then writes no file.

| What | Code | Exit code | Message |
|---|---|---|---|
| No file at the path | `FS-0068` | 1 | ``src="<src>"` names no file: a picture file, from the project folder or a path that begins with `/`` |
| The file is there and frx7 finds it damaged, or it cannot be read | `FS-0069` | 1 | ``src="<src>"` is damaged or cannot be read: <frx7's reason>`` |
| The file is not a picture of a kind read (frx7's `Unsupported`) | `FS-0035` | 2 | ``src="<src>"` is not a picture of a kind this version reads (<frx7's reason>); the kinds read are PNG and JPEG`` |
| The file stores a turn (an orientation other than 1) | `FS-0035` | 2 | ``src="<src>"` stores a turn (orientation <n>), which this version does not apply (S26)`` |

frx7's reason is its error's text as frx7 gives it. A picture damaged after its header has facts, and is found when its pixels are read: that read is also made before the first frame.

## 4. The box

The box of an `<image>` is placed by `notation.md` section 7.1, as a `<box>` with the same `left`, `top`, `right`, `bottom`, `width`, `height` and `position`, with these sizes where none is written. Let the picture be `W` by `H` pixels.

1. **Neither size known.** A size is known when it is written, or, for the width, when both `left` and `right` are written (for the height, `top` and `bottom`). With neither known, the box is `W` by `H`.
2. **One size known.** The box is first placed with the other size written as 0; the known size `k` is then the size of that placement. The other size is `k * H / W` (for a known width) or `k * W / H` (for a known height), written as a number of pixels and rounded as a written size is rounded. The box is then placed again with both sizes written.
3. **Both known.** As written; the picture is fitted to the box by section 5.
4. A picture of 0 pixels in either direction is not drawn; frx7 refuses such a file as damaged (section 3).

## 5. Fitting and drawing

The box is the rectangle `(bx, by, bw, bh)` of section 4, in pixels of the frame before any map. The picture's place is a rectangle `(dx, dy, dw, dh)`, which may have fractional edges:

1. **`fill`** (the default): `dw = bw`, `dh = bh`, `dx = bx`, `dy = by`.
2. **`cover`**: the scale `s = max(bw / W, bh / H)`; `dw = W * s`, `dh = H * s`.
3. **`contain`**: the scale `s = min(bw / W, bh / H)`; `dw = W * s`, `dh = H * s`.
4. **`object-position`** `p% q%` (default `50% 50%`): under `cover` and `contain`, `dx = bx + (bw - dw) * p / 100` and `dy = by + (bh - dh) * q / 100`. `0% 0%` holds the picture at the top left corner of the box and `100% 100%` at the bottom right. Under `fill` it changes nothing. Each value is a percentage; anything else is reported (`FS-0039`, the word or form not taken).

What is drawn is the picture's place cut to the box: the rectangle `D = (dx, dy, dw, dh) ∩ (bx, by, bw, bh)`. Under `cover` the part outside the box is not drawn; under `contain` the rest of the box is not drawn on, and shows what is under it.

**The value at a point.** The element's map `M` (`render-command.md` section 4.5) takes the element's plane to the frame. For a pixel of the frame whose centre is `c`, let `e = M⁻¹(c)` and take the point of the picture

- `u = (e.x - dx) * W / dw`, `v = (e.y - dy) * H / dh`,

in pixels of the picture, where the picture's pixel `(i, j)` covers `[i, i + 1] x [j, j + 1]`. The value there is a weighted mean of the picture's premultiplied pixels (section 2, point 4), separately in each channel, with the weight of pixel `(i, j)`

- `w(i, j) = t((i + 0.5 - u) / ru) * t((j + 0.5 - v) / rv)`, with `t(z) = max(0, 1 - |z|)`,

over the pixels of the picture only (a pixel outside the picture has no weight), and divided by the sum of the weights. Where no pixel of the picture weighs (a point a reach or more outside the picture, at the edge of a turned picture), the value is the nearest pixel's. The radii are `ru = max(1, W / (dw * σx))` and `rv = max(1, H / (dh * σy))`, where `σx` is the length of the map's image of the unit step across, `(a, b)`, and `σy` that of the step down, `(c, d)`. So a picture drawn at its own size, or larger, is sampled bilinearly from the four pixels around the point; a picture drawn smaller is averaged over the pixels its frame pixel spans, and does not drop detail. Each channel of the mean is rounded to the nearest whole level, halves up. A picture drawn at its own size on whole pixels under a move by whole pixels gives its own pixels exactly.

**Coverage.** The share of the frame pixel that `M(D)` covers. Under a move by whole pixels with `D` on whole pixels, it is 1 inside and 0 outside. Under a map with no turn or slant (`b = c = 0`) it is the exact area of the pixel's square inside `M(D)`, times 255, rounded to the nearest whole level, halves up. Under any other map it is the coverage the rasteriser vello_cpu 0.3.0 gives the mapped rectangle, as for the fills of `render-command.md` section 4.5, an integer from 0 to 255.

**The pixel.** Each channel of the value is multiplied by the coverage: `round(value * coverage / 255)`, halves up. The result is laid over the picture below by src-over (`render-command.md` section 4): each channel becomes `value + round(below * (255 - alpha) / 255)`. A transparent part of the picture changes nothing below; a part of alpha between shows what is under it in proportion.

A clip of the element (a canvas shown from another file, a cut) and an opacity of a group it stands in hold as for any mark.

## 6. The card

The card draws the picture of an `<image>` from the processor's layer: `draw_cpu` gives the premultiplied values that section 5 lays (value times coverage) as a picture of the frame's size, and the card lays it over its target by src-over, as `card-renderer.md` section 11 lays a background drawn under a map. The card therefore agrees with the processor by the rounding of its blend alone, within the tolerance of `card-renderer.md` section 4.

## 7. Tests

- Fitting, worked by hand: the rectangles of section 5 for `fill`, `cover` and `contain` with a picture wider and one taller than its box, at the positions `0% 0%`, `50% 50%` and `100% 100%` (`crates/draw/tests/`).
- The value at a point, worked by hand: a picture drawn at its own size gives its pixels; one drawn at twice its size gives the tent weights of its two nearest pixels; one drawn at half its size gives the tent weights widened by the reach; a transparent pixel shows what is under it (`crates/draw-cpu/tests/`); the card on a frame of such pictures, under a move and under a turn with a scale (`crates/draw-gpu/tests/card_still_pictures.rs`).
- Boxes and reports through the scene and the command (`crates/render/tests/`, `crates/cli/tests/`): a picture with no size, with one size, the kinds read, a kind not read, a file missing, a damaged file, a stored turn.
- Fixtures: pictures made by the test with marked corners, whose frames are held to values worked from this section, on the processor to the byte and on the card within `card-renderer.md` section 4.

## 8. Not in this version

An image in a row, a column or a set of columns; an image that moves, or with a colour filter, a mask, a clip, a border, a corner, a shadow or a background (T060, T061 and later); a picture with a stored turn (S26, Sprint 7); animated pictures (S30, Sprint 7) and remote pictures.
