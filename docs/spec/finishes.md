# Finishes: a torn edge and grain

Status: in force from Sprint 1 (ticket T058), for the attributes `displace` and `grain` of a `<box>` and a `<text>`: the noise both read, what each does to the element's finished picture, how both renderers draw it, and what is reported. Sources: `notation.md` section 7.6 (the forms `noise(FREQUENCY OCTAVES SEED)`, `displace="AMOUNT noise(...)"` and `grain="noise(...) SLOPE OFFSET"`, the reading point half a pixel on, the 256 steps, the order of the finishes, and that the processor draws them for both renderers); the W3C's Filter Effects Module Level 1, section 9.21 (`feTurbulence`): its fractal noise, its random numbers and their seed, and the order in which they are drawn, whose procedure is the standard's and is described here in our own words; `render-command.md` section 4.5 (groups, maps, opacity); `card-renderer.md` sections 4 (the tolerance) and 9.9 (placing a picture of the processor's).

## 1. The noise

`noise(F O S)` names a field `n(x, y)` of numbers from 0 to 1. `F` is the base frequency, the same in both directions; `O` the number of octaves; `S` the seed. It is the alpha channel (the fourth of the four channels) of the standard's fractal noise, with no stitching of tiles:

1. **The seed.** A seed `S` that is 0 or below becomes `-(S mod (m - 1)) + 1`, where `m = 2147483647` and `mod` keeps the sign of `S` (so `-7` becomes 8 and `0` becomes 1); one above `m - 1` becomes `m - 1`.
2. **The random numbers.** Each next number is `r' = a (r mod q) - p (r div q)`, with `a = 16807`, `q = 127773`, `p = 2836`, plus `m` when that is 0 or below; the first is drawn from the seed.
3. **The lattice.** For each channel `k` from 0 to 3, for each `i` from 0 to 255, two numbers are drawn in turn, each giving a component `((r mod 512) - 256) / 256` of the gradient `g[k][i]`, which is then divided by its length. Then a list `L` of 0 to 255 in order is shuffled: for `i` from 255 down to 1, a number is drawn, `j = r mod 256`, and `L[i]` and `L[j]` change places. The list and the gradients are then repeated: entry `256 + i` is entry `i`, for `i` from 0 to 257.
4. **The noise at a point** `(u, v)` of channel `k`: with `t = u + 4096`, the whole part `w = trunc(t)`, `x0 = w & 255`, `x1 = (x0 + 1) & 255`, `rx0 = t - w`, `rx1 = rx0 - 1`; the same for `v` giving `y0`, `y1`, `ry0`, `ry1`. With `i = L[x0]`, `j = L[x1]`, the corners are `c00 = L[i + y0]`, `c10 = L[j + y0]`, `c01 = L[i + y1]`, `c11 = L[j + y1]`. With `s(t) = t² (3 - 2t)`, `sx = s(rx0)`, `sy = s(ry0)`, and `mix(t, a, b) = a + t (b - a)`:
   `a = mix(sx, g[k][c00] · (rx0, ry0), g[k][c10] · (rx1, ry0))`, `b = mix(sx, g[k][c01] · (rx0, ry1), g[k][c11] · (rx1, ry1))`, and the noise is `mix(sy, a, b)`, where `·` is the dot product.
5. **The octaves.** Starting from `(u, v) = (F x, F y)` and a ratio of 1, the sum of `noise(u, v) / ratio` over `O` octaves, the point and the ratio doubled after each.
6. **The value.** `n = (sum · 255 + 255) / 2 / 255`, held between 0 and 1, then kept in 256 steps: `floor(255 n + 0.5) / 255`.

All of it is worked in 64-bit floating point and whole numbers of at least 32 bits. At a point where `F x` and `F y` are whole numbers, every octave falls on the lattice, where the noise is 0, so `n` is `(0 + 255) / 2 / 255 = 0.5`, kept as `128 / 255`.

`(x, y)` is a point of the element's box, in the element's own pixels from its top left corner. For a pixel of the frame, it is the centre of the pixel, `(px + 0.5, py + 0.5)`, taken back through the element's map (`render-command.md` section 4.5) and less the top left corner of the element's rectangle: for an element that does not move, the pixel `(i, j)` of its box is read at `(i + 0.5, j + 0.5)`. The noise moves, turns and scales with the element.

## 2. What is read

1. `displace` and `grain` are written on a `<box>` or a `<text>`, directly or through a style. They are not animated: a `<tween>` or a `<set>` that changes one is refused as `render-command.md` section 3 refuses one that changes another property.
2. `displace="AMOUNT noise(F O S)"` and `grain="noise(F O S) SLOPE OFFSET"`: words separated by spaces, `noise(` and `)` written so, numbers read by N9 of `notation.md`. `F` is above 0; `O` a whole number from 1 to 8; `S` a whole number; `AMOUNT` above 0; `SLOPE` and `OFFSET` any numbers.
3. A value in another form (a word missing or added, another function than `noise`, a number that is not a number, an `O` or `S` with a fraction) is reported as `FS-0071`, invalid, on the line of the attribute: ``` `displace="7 noise(0.1 3)"` is not in the form AMOUNT noise(FREQUENCY OCTAVES SEED) ``` (for `grain`, `noise(FREQUENCY OCTAVES SEED) SLOPE OFFSET`). An `O` that is a whole number outside 1 to 8 is reported as `FS-0071`: ``` `grain="noise(0.5 9 1) 1 0"` has 9 octaves; the octaves are a whole number from 1 to 8 ```. An `AMOUNT` or an `F` of 0 or below is reported as `FS-0053`: ``` `displace="0 noise(0.1 3 1)"` has an amount that is not above 0 ``` (`a frequency` for `F`). `check` reports each and `render` stops with it before the first frame (exit 1, no film).
4. A `<box>` that holds a `<composition src>` and writes either is refused as unsupported (`FS-0035`), as such a box that moves is; so is either on a `<composition src>` or a `<span>`.
5. Either makes the element a group of its own (`timeline.md` section 5), as a movement does, so that its picture is finished alone.

## 3. What each does

Both act on the element's finished picture: the element and its children drawn with the element's map, and its turn in space when it has one, into a clear picture of the frame's size (`render-command.md` section 4.5), before the element's opacity. Where both are written, `displace` comes first.

1. **`displace`.** For each pixel `P` of the frame, with `n` the noise read for `P` (section 1) and `d = AMOUNT · (n - 0.5)` of the element's pixels, the vector `(d, d)` is taken through the linear part of the element's map (`(a d + c d, b d + d d)` in the terms of section 4.5), and each of its two components is rounded to a whole number of frame pixels, halves away from 0. The pixel `P` takes the value of the finished picture at `P` plus that vector, exactly, or clear where that lies outside the frame. No value is mixed with another, so the edge is hard. For an element that does not move, every pixel is taken from at most `round(AMOUNT / 2)` pixels away across and down.
2. **`grain`.** Each premultiplied value `v` of a pixel (its three colours and its alpha) becomes `floor(v · g + 0.5)`, where `g = SLOPE · n + OFFSET` held between 0 and 1, with `n` the noise read for that pixel. So the alpha is multiplied by `g`, and the colour keeps its hue.
3. Then the picture is faded by the element's opacity (`render-command.md` section 4.5, rule 6) and laid over the frame by src-over, cut to the element's cut.

A frame is a function of the composition and its number alone: the noise depends on nothing but its three numbers and the point, so two renders give the same frames, and the same seed gives the same picture.

## 4. The two renderers

1. **The processor** (`draw-cpu`) draws a group with a finish as section 3 says: the group's marks into a clear picture of the frame's size, then `displace`, then `grain`, then the opacity and the cut. The noise lives in the `draw` crate (`draw::Noise`).
2. **The card** (`draw-gpu`) puts the processor's picture of the group, finished and faded (`draw_cpu::finished_layer`), on its frame as a picture (`card-renderer.md` sections 9.9 and 10.7), cut to the group's cut. As `notation.md` section 7.6 says, the processor draws such an element for both renderers; the card's picture differs from the processor's only by the rounding of laying that picture.

## 5. Tests

- The noise (`crates/draw/tests/fractal_noise.rs`): the value 128 / 255 at points on the lattice, worked by hand; the seeds `-7` and 8, and 0 and 1, giving the same field; and the field at chosen points for chosen numbers equal to a second procedure written in the test from section 1, step by step, with no code of the product.
- The finishes through the scene (`crates/render/tests/finishes_worked_by_hand.rs`): grain at worked places and alphas, held between 0 and 1; a torn edge that leaves the straight line by no more than half the amount and keeps the fill away from it, with every pixel either the fill or what is under it; another seed giving another picture and the same seed the same; on both renderers.
- The reports (`crates/cli/tests/finish_reports.rs`): octaves outside 1 to 8, an amount (and a frequency) of 0, and a form not listed, each with its line, by `check` and `render`; the forms listed passing `check`; and two renders of the fixture giving the same frame marks.
- A torn edge scaled with its element, its distance scaled before it is rounded (`finishes_worked_by_hand.rs`); grain on the lattice, where the noise is 128 / 255, at an opacity of 1 and of 0.5.
- The fixture `fixtures/finishes-torn-and-grain/`: its frames on the processor equal its checked-in pictures to the byte, and on the card they agree within `card-renderer.md` section 4.
