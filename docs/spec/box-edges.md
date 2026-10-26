# Borders, rounded corners and the cut of a box

Status: in force from Sprint 1 (ticket T054), for `<box>` and every element whose table in `notation.md` section 6 takes `border`, `border-radius` and `overflow`, as far as this version draws that element. Drawn on both renderers: the processor's picture is the correct one, and the card's agrees with it within `card-renderer.md` section 4.

Sources: `notation.md` sections 3 (the drawing order inside one element), 4 (the form "line") and 7.5 (`border-radius`, `border`, `overflow`); the proofs' record of the border of two lines, `native-notation.md` section 18 on the proofs' branch (behaviour only, decision B4); CSS Backgrounds and Borders Module Level 3, as a public standard, for the meanings CSS gives: a border lies inside the border box, the inner radius of a corner is its outer radius less the border's width and never below 0, radii too large for the box are scaled down by one factor for all corners, a `double` border is two lines and a gap whose sum is the border's width, and `overflow` cuts at the padding box, inside the border, with the inner radii (CSS Overflow Module Level 3); the plane geometry of circles and rectangles, written out here by us. The coverage of a shape is the share of each pixel it covers, as vello_cpu 0.3.0 fills it, as in `render-command.md` section 4.5, rule 7.

## 1. What the author writes

| Attribute | Form | Default |
|---|---|---|
| `border` | `w c`, a number of 0 or more and a colour; or `w double c` | none |
| `border-radius` | a number of 0 or more, in pixels | `0` |
| `overflow` | `visible` or `hidden` | `visible` |

- In `border`, the parts are separated by one or more spaces. `w` is a number (`notation.md` section 4, N9), with no unit; `c` is a colour, `#rrggbb` or `#rrggbbaa`. The only word taken between the two is `double`.
- A `border` in any other form (`2px #ffffff`, `#ffffff 2`, `2 solid #ffffff`, `2`, `-2 #ffffff`, a colour that is not one) is invalid: it is reported with the line of the attribute and the forms taken, `width colour` and `width double colour`, and nothing is drawn (section 6).
- A `border-radius` that is not a number, or that is below 0, is invalid, reported with its line and the form taken (a number of 0 or more).
- An `overflow` other than the two words is invalid, reported with its line and the words taken.
- A border by side (`border-top`, `border-right`, `border-bottom`, `border-left`, `border-width`, `border-color`, `border-style`) is known and not supported in this version: entry S20 of `notation.md` section 14. It is reported as unsupported with the line of the attribute, and `render` refuses the project before the first frame.
- A corner written as an ellipse (`border-radius="24 / 12"` or `"24 12"`: two numbers, apart or with a `/` between them) is known and not supported in this version: entry S33 of `notation.md` section 14 (decision B35). It is reported as unsupported with the line of the attribute, in the same way.
- A border of width 0 draws nothing. A `border-radius` of 0 is a square corner.

## 2. The geometry of a box

Let the box's rectangle (`render-command.md` section 4.1) run from `x0` to `x1` across and from `y0` to `y1` down, whole pixels, with width `W = x1 − x0` and height `H = y1 − y0`. All the numbers below are exact real numbers until a shape is filled.

1. **The radius used.** The written radius is `r`. The radius used at every corner is `R = min(r, W / 2, H / 2)`. This is CSS's rule for four equal radii: when two corners on one side would overlap, every radius is scaled by the same factor until they meet. A radius larger than half the shorter side therefore makes the shorter sides one half circle each: a square box becomes a circle, and a box twice as wide as it is high becomes a capsule whose ends are half circles of radius `H / 2`. A box with `W` or `H` of 0 draws nothing.
2. **A rounded rectangle.** The rounded rectangle `Q(a, ρ)` is the rectangle inset by `a` on every side, from `x0 + a` to `x1 − a` and from `y0 + a` to `y1 − a`, with each corner replaced by a quarter circle of radius `ρ` that touches both sides. When `a` is at least half of `W` or `H`, `Q(a, ρ)` is empty.
3. **The outer edge** is `Q(0, R)`.
4. **The border's width** is `w`. A border wider than half the shorter side is drawn as if it were `min(w, W / 2, H / 2)` wide: it fills the box and leaves no inside.
5. **The inner edge** is `Q(w, max(R − w, 0))`: the inner radius is the outer radius less the width, and a border at least as wide as the radius has a square inner corner. The inside of the box, its padding box, is the inner edge; a box with no border has its outer edge as its inner edge.

## 3. What is drawn, in order

The order is the one of `notation.md` section 3: the box shadows (T055), the background layers, the picture, footage or shape, the border, the text, and then the children in document order.

1. **The background.** Every layer of the background is drawn as `render-command.md` gives it, and then cut to the outer edge `Q(0, R)`, so that each pixel of the background is weighed once, by the coverage of `Q(0, R)` alone. Under a map that is not a move of whole pixels, the layers as `render-command.md` section 4.6 draws them already carry the coverage `k₁` of the box's mapped rectangle (rule 9). The outer edge lies inside that rectangle, so the pixels it covers are a part of the pixels the rectangle covers, and its coverage `k₂` is never more than `k₁`. The cut therefore weighs each pixel of the drawn layers by `k₂ / k₁`, written as a whole level `min(255, round(255 · k₂ / k₁))` (0 where `k₁` is 0), and the pixel is laid with the share `k₂` and not `k₂ · k₁`. A box moved by whole pixels has `k₁ = 255` inside its rectangle, and the weight is `k₂`. Under a border the background is still there; the border is drawn over it. Pixels outside the rounded corners keep what was under the box.
2. **The border of one line** is the ring between the outer and the inner edge: the coverage of `Q(0, R)` less the coverage of the inner edge, filled in the border's colour and laid over the picture by src-over (with the colour's own alpha, `#rrggbbaa`). It does not change the box's size, its place, or the place of its children.
3. **The border of two lines** (`w double c`) is two rings and a gap, each a third of the width: the outer line between `Q(0, R)` and `Q(w/3, max(R − w/3, 0))`, the gap between that and `Q(2w/3, max(R − 2w/3, 0))`, where nothing of the border is drawn and the background shows, and the inner line between that and the inner edge `Q(w, max(R − w, 0))`. A border of `9 double c` on a box with square corners is a line of 3 pixels at the edge, a gap of 3 pixels, and a line of 3 pixels inside it.
4. **The children** are drawn in document order, over the box and its border.

## 4. The cut

`overflow="hidden"` cuts the box's children, and anything the box draws inside its border, at the inner edge (section 2, rule 5), following its rounded corners:

1. The children are drawn, with all their marks and their own cuts, into a layer of their own, transparent at first.
2. Each pixel of the layer is weighed by the coverage of the inner edge at that pixel: a value is multiplied by the coverage and rounded to the nearest whole level, halves up.
3. The layer is laid over the picture by src-over.

So no part of a child shows outside the box's inner edge, and none outside a rounded corner. A box with `overflow="visible"` (the default) draws every child whole, wherever it lies. A cut inside a cut cuts by both. The cut does not change any place or size. The cut is not drawn on the box's own background or border, which section 3 already keeps inside the outer edge.

A box that moves, scales, turns, slants or fades (`render-command.md` section 4.5) takes its border, its rounded corners and its cut with it: the shapes of sections 2 to 4 are made in the box's own coordinates and are filled through the box's map; the cut is made before the box's opacity is applied (`notation.md` section 7.6).

## 5. Pixels

- A shape is filled by vello_cpu 0.3.0 with its anti-aliased coverage, as `render-command.md` section 4.5, rule 7 fills a mapped shape. A quarter circle is drawn as the curve vello_cpu's geometry library (kurbo) makes of a circular arc. Every shape is mapped first and then flattened by kurbo into straight pieces that lie within 0.01 of a pixel of the frame from the curve, and vello_cpu fills those straight pieces, so the share of each pixel is the share of the true circle within a level or two (vello_cpu's own flattening of a curve, coarser than this, would leave a share up to about a quarter of a pixel short along every arc). The coverage of a pixel is a whole level from 0 to 255.
- A shape whose edges all lie on whole pixels (a radius of 0, a border of a whole width, a box not mapped, or mapped by a move of whole pixels) covers each pixel wholly or not at all, and is the same as the fills of `render-command.md` section 4: a border of 6 pixels on a box with square corners is the 6 columns and the 6 rows inside its edge, exactly, in the border's colour.
- A colour laid by a coverage `k` is laid with its alpha times `k / 255`, rounded to the nearest whole level, halves up, by src-over.

## 6. Reports

| Code | Kind | When |
|---|---|---|
| `FS-0067` | invalid | A `border` in a form section 1 does not list. The message names the value and the forms `width colour` and `width double colour`. |
| `FS-0032` | invalid | A `border-radius` that is not a number. |
| `FS-0034` | invalid | A `border-radius` below 0 (`notation.md` N6). |
| `FS-0039` | invalid | An `overflow` that is not `visible` or `hidden`. |
| `FS-0035` | unsupported | A border by side, S20. |
| `FS-0035` | unsupported | A corner written as an ellipse, S33. |

`check` lists every one with the line of its attribute; `render` refuses the project before the first frame and writes no file.

## 7. The card

The card draws the same shapes from the same coverage. The processor makes the coverage of each rounded rectangle and ring into a mask of whole pixels with the function `draw_cpu` uses (`card-renderer.md` section 10.3), and the card blends the colour through it, cuts a background layer by it, and weighs a cut layer by it. A shape whose edges lie on whole pixels is drawn as a fill of whole pixels, with no mask. The two pictures differ only by the rounding of the blends, within `card-renderer.md` section 4.

## 8. Tests

- `crates/draw/tests/` and `crates/draw-cpu/tests/`: the geometry of section 2 (the radius used, the inner edge, the thirds of a double border); the pixels of a border of 6 and of `9 double`; a rounded corner showing what lies under the box; a cut child at a rounded corner; a radius larger than half the shorter side.
- `crates/draw-gpu/tests/`: the same marks agree with the processor within `card-renderer.md` section 4. Reference machine only.
- `crates/render/tests/`: the fixtures `fixtures/box-borders/`, `fixtures/box-corners/` and `fixtures/box-cut/` at their chosen frames equal their checked-in pictures to the byte on the processor, and agree on the card.
- `crates/cli/tests/`: the reports of section 6 from `check` and `render`, each with its line.

## 9. What is not covered

- A corner written as an ellipse (S33): not in this version.
- Borders by side (S20), dashed and dotted borders (not listed by the notation), and a radius per corner: not in this version.
- The shadows that follow the corners: T055.
