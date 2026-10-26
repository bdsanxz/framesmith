# Blends: an element mixed with what is under it

Status: in force from Sprint 1 (ticket T057), for the attribute `mix-blend-mode` of a `<box>` and a `<text>`: which names are read, what colour each gives where the element lies over what is under it, how the element's opacity and transparency come in, how both renderers draw it, and what is reported. Sources: `notation.md` section 7.7 (the sixteen names, the attribute's place among the look attributes, and the lightness `0.3 R + 0.59 G + 0.11 B`); the W3C's Compositing and Blending recommendation, Level 1, sections 5.3 (the general formula of compositing with a blend), 10.1 (the separable blends) and 10.2 (the non-separable blends and their helper functions), whose formulas are the standard's and are written here in our own words; `render-command.md` section 4.5 (groups, opacity, the processor's rounding of a faded layer); `card-renderer.md` section 10.1 (groups on the card) and section 4 (the tolerance).

## 1. What is read

1. `mix-blend-mode` is written on a `<box>` or a `<text>`, directly or through a style (`notation.md` N5). Its value is one of the sixteen names: `normal`, `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`. Not written, it is `normal`.
2. Any other value is wrong, not unsupported: it is reported as `FS-0039`, invalid, on the line of the attribute, with the sixteen names in that order: ``` `mix-blend-mode="burn"` is not one of: normal, multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue, saturation, color, luminosity ```. `check` reports it and `render` stops with it before the first frame (exit 1, no film).
3. A blend is not animated: a `<tween>` or a `<set>` that changes `mix-blend-mode` is refused as section 3 of `render-command.md` refuses one that changes another property (`notation.md` section 8.1 does not list it among the properties that move).
4. A `<box>` that holds a `<composition src>` and writes a blend other than `normal` is refused as unsupported (`FS-0035`) as such a box that moves is (`render-command.md` section 3); so is a blend on a `<composition src>` and on a `<span>`.

## 2. What is blended

The blend acts on the **finished picture** of the element: its own background or text and everything inside it, drawn together with the element's motion, cut and opacity (`notation.md` section 7.6), over **what is under it**: the picture as it stands when the element is drawn, which is everything drawn before it in the order of the file, inside the same parent group. A blended element inside a group that fades (an opacity below 1) or turns is blended with what that group has drawn before it, since a group draws into a picture of its own (`render-command.md` section 4.5). An element with `normal` is laid by src-over, exactly as before this section.

## 3. The colour of one pixel

For one pixel, with values 0 to 1:

- the element's picture gives a premultiplied colour `cs` and an alpha `as` (its opacity already counted, section 5);
- what is under it gives a premultiplied colour `cb` and an alpha `ab` (on the frame, `ab` is 1);
- `Cs = cs / as` and `Cb = cb / ab` are the two colours with their alpha taken out (0 where the alpha is 0).

Where `as` is 0 the pixel is left as it was. Otherwise the result is, for each colour channel,

`co = cs · (1 − ab) + cb · (1 − as) + as · ab · B(Cb, Cs)`

and its alpha `ao = as + ab − as · ab`, where `B` is the blend's function of section 4. (With `B(Cb, Cs) = Cs` this is src-over.) Each of `co` and `ao` is clamped to 0 to 1 and kept as a whole level by `floor(255 x + 0.5)`.

## 4. The sixteen functions

`s` is a channel of `Cs`, `b` the same channel of `Cb`. The first twelve work on each channel alone.

| Name | `B(b, s)` |
|---|---|
| `normal` | `s` |
| `multiply` | `b · s` |
| `screen` | `b + s − b · s` |
| `overlay` | `hard-light(s, b)`: the hard light with the two colours exchanged |
| `darken` | the least of `b` and `s` |
| `lighten` | the greatest of `b` and `s` |
| `color-dodge` | 0 where `b` is 0; else 1 where `s` is 1; else the least of 1 and `b / (1 − s)` |
| `color-burn` | 1 where `b` is 1; else 0 where `s` is 0; else `1 −` the least of 1 and `(1 − b) / s` |
| `hard-light` | where `s` is at most 0.5, `multiply(b, 2s)`; above, `screen(b, 2s − 1)` |
| `soft-light` | where `s` is at most 0.5, `b − (1 − 2s) · b · (1 − b)`; above, `b + (2s − 1) · (D(b) − b)`, where `D(b)` is `((16 b − 12) · b + 4) · b` for `b` at most 0.25 and `√b` above it |
| `difference` | the absolute value of `b − s` |
| `exclusion` | `b + s − 2 · b · s` |

The last four work on the colour as a whole, with these helpers on a colour `C = (r, g, b)`:

- `Lum(C) = 0.3 r + 0.59 g + 0.11 b`.
- `Sat(C)` is the greatest of the three channels less the least.
- `Clip(C)`: with `L = Lum(C)`, `n` the least channel and `x` the greatest: where `n < 0`, each channel becomes `L + (c − L) · L / (L − n)`; then where `x > 1`, each becomes `L + (c − L) · (1 − L) / (x − L)`.
- `WithLum(C, l)`: each channel plus `l − Lum(C)`, then `Clip`.
- `WithSat(C, t)`: where the greatest channel is above the least, the greatest becomes `t`, the middle one `(middle − least) · t / (greatest − least)` and the least 0; where all three are equal, all become 0. When two channels are equal, the one of them listed first in red, green, blue is taken as the least or the middle, so that each channel has one role.

| Name | `B(Cb, Cs)` |
|---|---|
| `hue` | `WithLum(WithSat(Cs, Sat(Cb)), Lum(Cb))` |
| `saturation` | `WithLum(WithSat(Cb, Sat(Cs)), Lum(Cb))` |
| `color` | `WithLum(Cs, Lum(Cb))` |
| `luminosity` | `WithLum(Cb, Lum(Cs))` |

## 5. Opacity and transparency

An element's opacity multiplies its finished picture first, as `render-command.md` section 4.5 says for a faded group: each premultiplied value `v` of the picture becomes `floor(v · opacity + 0.5)`. The blend of section 3 then takes those values. So a blend at an opacity `o` over an opaque pixel gives `o · B(Cb, Cs) + (1 − o) · Cb` when the element is opaque there, up to the rounding of whole levels. Where the element is partly clear (an edge of a glyph, a colour with an alpha), its alpha is part of `as` in the same way.

## 6. The two renderers

1. **The processor** (`draw-cpu`) draws a group whose blend is not `normal` into a picture of the frame's size of its own, cleared to transparent, fades it by the opacity (section 5), and then lays it over the picture within the group's cut by section 3, in 64-bit floating point. The functions of section 4 live in the `draw` crate (`draw::Blend`), written once for both renderers' tests.
2. **The card** (`draw-gpu`) draws such a group into a texture of its own, as it draws a group that fades (`card-renderer.md` section 10.1). Before it lays the texture, it copies what is under it (the picture or the parent group's texture) into a texture of the same size, and one quad over the group's cut runs a shader, `blend.wgsl`, that reads both, fades the group's value by the opacity and rounds it as the processor does, applies section 3 and section 4 in 32-bit floating point, and writes the result in place of what was there. The two pictures differ only by the rounding of 32-bit against 64-bit arithmetic, within `card-renderer.md` section 4.
3. A blend of `normal` draws nothing new on either renderer: the group is drawn as before.

## 7. Tests

- The functions, worked by hand (`crates/draw/tests/blend_functions.rs`): for each of the sixteen names, a named case with at least two pairs of colours, the expected colour worked from section 4 on paper.
- Boxes over boxes, on the processor (`crates/render/tests/blends_worked_by_hand.rs`): for each name its own case, a coloured box over another, the overlap at the worked colour within one level, and outside it each box at its own colour; a blend at an opacity, worked by section 5.
- The reports (`crates/cli/tests/blend_reports.rs`): an unknown name, by `check` and `render`, with its line and the sixteen names.
- The fixture `fixtures/blends-sixteen/`: its frames on the processor equal its checked-in pictures to the byte, those pictures hold the worked colours, and on the card they agree within `card-renderer.md` section 4 (reference machine only).
