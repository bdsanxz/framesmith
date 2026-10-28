# Shapes from path data, filled or stroked, with dashes

Status: in force from Sprint 1 (ticket T062), for a `<shape>` written with `d`, `view-box`, `stroke-width`, `color`, `dash` and `drop-shadow`, with the look of a box (sections 8 and 9). A `<shape>` with `src` (T063) or `draw` (T070) is not drawn by this version; `notation.md` section 14 names each until its ticket. Drawn on both renderers: the processor's picture is the correct one, and the card's agrees with it within `card-renderer.md` section 4.

Sources: `notation.md` sections 6.8 (the element), 7.11 (the attributes, which this file makes exact), 7.1 (placing a box), 14 (the support table); decision A29, the curve tolerance of 0.25 pixel; decision B44, the join of a stroke (mitre, limit 4, bevel past it), which this file takes for a shape too, since a stroke has one join in this product; `render-command.md` section 4.5 (the placement of a box under a map) and `card-renderer.md` section 9.9 (a processor picture placed by the card). The path data grammar is the public SVG 2 grammar for the commands listed in section 2, written here in our own words; the arc conversion is the standard endpoint-to-centre form, written out by us. Nothing is taken from any implementation.

## 1. What the author writes

| Attribute | Form | Default | Drawn |
|---|---|---|---|
| `d` | path data (section 2) | none | the outlines |
| `view-box` | four numbers `x y width height`, width and height above 0 | `0 0 24 24` | the coordinate space of the outlines (section 3) |
| `stroke-width` | a number above 0, in view-box units | none | with it, each outline is a line of that width instead of a fill (section 4) |
| `color` | a colour | `#000000` | the fill, or the line with `stroke-width` |
| `dash` | pairs of lengths (section 5) | none | the line's dashes; with `stroke-width` only |
| `drop-shadow` | one shadow, `X Y BLUR COLOUR`, in view-box units (section 8) | none | the shadow of the outlines |

A `<shape>` with no `d` shows nothing and is reported (section 6). A shape with no `width` or no `height` has no size and shows nothing (`notation.md` 7.11). The look of its box (`background`, `border-radius`, `border`, `box-shadow`) is section 9.

## 2. Path data

The path data is a string of commands and numbers in the SVG grammar:

- The commands `M m L l H h V v C c S s Q q T t A a Z z`. Upper case takes absolute coordinates, lower case takes relative ones, from the current point; a relative `m` after `z` starts from the start of the closed outline.
- A command may be followed by more numbers of the same kind, which repeat it (for `M`, further pairs are lines). Numbers are separated by spaces, commas, or nothing where the sign or the point separates them.
- A number is an optional sign, digits with at most one point, and an optional exponent `e` or `E` with its sign and digits. A number may begin with a point (`.5`).
- An arc (`A` and `a`) takes radius x, radius y, rotation, the large-arc flag and the sweep flag, the last two as the single characters `0` or `1` (so `a1 1 0 00 2 2` is valid). An arc with a radius of 0, or an end point equal to its start, is a straight line. A radius too small for the two end points is raised until they fit, as the standard endpoint-to-centre conversion gives it; the arc is then cut into cubic curves.
- The path starts with `M` or `m`. Anything else before the first command, a command letter with no numbers where numbers are needed, a number that does not read, or a flag that is not `0` or `1`, makes the whole `d` not path data (section 6).
- `S`, `s`, `T` and `t` reflect the control point of the previous curve only when that curve is of the same kind (cubic for `S`, quadratic for `T`); otherwise the control point is the current point.

Each `M` (or `m`) starts a new outline. An outline is closed by `Z`; an open outline is filled as if closed, and stroked as open.

## 3. The view box

The outlines are in the view box's coordinates. The shape is scaled evenly by one factor, the smaller of the box's width over the view box's width and the box's height over the view box's height, and centred in its box: the centre of the view box lies at the centre of the element's box. A square view box in a square box puts the outline's corners at the box's corners (the box is the element's placed rectangle, as `notation.md` 7.1 places it). A view box of another size scales the outline to the box in the same way.

## 4. Fill and stroke

- **Fill.** Without `stroke-width`, each outline is filled with `color`, by the nonzero winding rule (the rule SVG gives a fill by default). The inside of the outline is `color`, and nothing outside it is drawn.
- **Stroke.** With `stroke-width`, the outlines are not filled: each is drawn as a line of the written width, in view-box units scaled by the factor of section 3, centred on the path, in `color`. The two ends of an open outline are round (`notation.md` 7.11). Where two segments meet, the join is a mitre with a limit of 4, bevelled past it (B44). A closed outline has no ends and is joined where it closes.
- **Curves.** A curve is drawn so that no drawn edge lies more than 0.25 pixel from the true curve (A29). This is the processor's tolerance, measured in the frame's pixels after the shape is scaled and placed.
- **Zero-length outlines.** A subpath with no length (a lone `M`, or `M` followed by `Z` at the same point) is drawn as a dot with round ends when stroked, of the line's width, and is not filled.

## 5. Dashes

`dash` is pairs of lengths: a dash, then a gap, in view-box units, repeated along each outline from its start: `dash="12 8"` is a run of 12 units of line and then 8 units of gap, and so on. Each outline starts its pattern at its own start point. The pattern is scaled by the factor of section 3 like the line. The count of numbers is even, no number is below zero, and not all of them are zero (otherwise the shape is reported, section 6). A closed outline's pattern runs round to its start.

The caps of a dashed line are set apart from its ends. Every dash end is butt, so a run of 10 with a gap of 5 is exactly 10 units of line and 5 units of gap along the centre line. The two ends of an open outline are round, as section 4 says: a dash that holds an end of the outline takes that end's round cap, reaching half the line's width past the end. A dash holds an end when the pattern is on at that point, so a gap at an end takes no cap, and a dash that only starts at the end (its length inside the outline is nothing) takes none either. A solid line (no `dash`) has its round caps from the stroke itself.

## 6. Reports

Each is reported on the line of the attribute, except the missing path, which is reported on the element's line, as the other attribute reports are (`notation.md` 7.11 and its table in section 14 of the check spec):

| Case | Code | Report (the texts of `notation.md` section 13, and N6) |
|---|---|---|
| no `d`, or a `d` that is empty or only spaces (and no `src`, which this version does not draw) | `FS-0088` | `<shape> needs `d` or a `src` with paths` |
| `view-box` not four numbers (each a finite number), or with a width or height of 0 or less | `FS-0089` | `` `...` is not a view box of four numbers `` |
| `dash` with no numbers, an odd count, a number below 0, or all zero | `FS-0089` | `` `dash="..."` is not pairs of lengths: a dash and a gap, in view-box units `` |
| `stroke-width` of 0 or less | `FS-0053` | the N6 report of `notation.md` (a zero or negative size) |
| `d` that is not path data (section 2) | `FS-0089` | `` `d` is not path data: <reason> `` (new, see below) |
| `dash`, or `draw`, on a `<shape>` with no `stroke-width`, or on an element that is not a `<shape>` | `FS-0087` | `` `dash` is for a <shape> with `stroke-width` `` (the same for `draw`; `notation.md` section 13) |
| `drop-shadow` that is not four values `X Y BLUR COLOUR` | `FS-0089` | `` `drop-shadow="..."` is not one shadow: x, y, blur and a colour `` (`notation.md` section 13) |
| a shadow blur below 0 | `FS-0034` | the N6 report of `notation.md` for a value below 0 |

The `d` report is new, and `notation.md` gets its row when this spec is in force. Its reason is one of: `no M at the start`, `unknown command <letter>`, `a number has no command`, `a number is missing`, `a number does not read`, `a flag is not 0 or 1`.

## 7. Drawing order

A `<shape>` is one drawn element. Inside it, its outlines are drawn in document order of the `d`; with several outlines, a later one lies over an earlier one. Its children (`<tween>` and `<set>`, for movement) act as for any element. The box's margin, its place and its clip are those of a box (T041, T054).

## 8. Shadow

`drop-shadow="X Y BLUR COLOUR"` on a `<shape>` (`notation.md` 7.6 and 7.11): `X` is the offset across, `Y` the offset down, `BLUR` the blur and `COLOUR` the colour, the first three in view-box units. A `<shape>` with no `drop-shadow` has no shadow.

- **What is drawn.** The shadow is the shape's own coverage, as sections 4 and 5 draw it (the fill, or the stroke with its dashes and its round ends), moved by `X` and `Y` and laid in `COLOUR` under the shape. It follows the outlines, not the box: a shadow lies where the outline of the shape lies, moved.
- **Units.** `X`, `Y` and `BLUR` are scaled into the frame's pixels by the factor of section 3, so a shadow moves and blurs with the shape when its view box is scaled. Under the element's map (`render-command.md`, section 4.5), the offset moves with the shape, and the blur is scaled by the square root of how much the map scales an area, as `text.md` section 6.6 rule 7 does for text.
- **The blur.** The blur in pixels is `BLUR` times the factor of section 3. Its standard deviation is half of it, and it is the method of `notation.md` 7.6 as `text.md` section 6.6 rule 7 states it: three passes, each a horizontal average and then a vertical one, each over a span of `floor(sigma * 3 * sqrt(2 * pi) / 4 + 0.5)` pixels, where a span below 2 leaves the shadow as it is. A pixel outside the frame counts as clear.
- **The colour.** `COLOUR` is opaque in this version: the shadow's alpha is the shape's coverage only. A colour with an alpha below `ff` is refused with FS-0035 on its line, as a `text-shadow` colour is (`text.md` section 6.6, rule 7).
- **The card.** The processor draws the shadow in the shape's layer, and the card places that layer (`card-renderer.md`, section 9.9), so both renderers agree within the tolerance of `card-renderer.md` section 4.

## 9. The look of a box

A `<shape>` takes the look of its attribute table, each drawn as on a box (`box-edges.md`; `render-command.md`, section 4.6): its `background` (`notation.md` 7.4), its `border-radius` and `border` (7.5), and its `box-shadow` (7.5). The look is drawn in this order: the `box-shadow` under the background, the background, and the border on the box's edge. The shape is drawn last, over the look, in its box. The `border-radius` rounds the background, the border and the box-shadow, as it does for a box (7.5); the shape's outlines are not rounded.

## 10. Placing, windows and marks

A `<shape>` is placed as a box is (`notation.md` 7.1): its `left`, `top`, `width`, `height` and `margin` put its box in its parent, and it may stand wherever a box stands, as one of the items of a row, a column or a set. A shape with no `width` and no `height` has no size and shows nothing. Its window is a box's window (`render-command.md` 4.2): `start` with `duration`, both or neither, and outside its window only its room shows, as a box outside its window keeps its place (4.2 rule 4).

Its marks are its box-look first and the shape over it: the background, the border and the corners by the edges of a box (`box-edges.md`), then one mark of the outlines in the element's box (`docs/spec/render-command.md` section 4). The shape is drawn last, over the look.

A shape takes no element of its own: an element inside a `<shape>` is refused, as inside a composition shown from another file. Movement on a shape is refused until it is wired: a movement attribute on it, and a `<tween>` or `<set>` inside it, are refused, as on a shown element. A colour with an alpha below `ff` is refused with FS-0035, as a `text-shadow` colour is.
