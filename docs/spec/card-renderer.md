# The card's renderer, and its agreement with the processor's

Status: in force from Sprint 1 (ticket T039), for the renderer on the graphics card in `draw-gpu` and for the comparison of the two renderers. Sources: `PLAN.md` section 1, point 2 (the processor's renderer defines the picture, the card's is the default and must agree with it), section 3 (the table of crates: `draw-gpu` is the renderer on the card; `wgpu` is the outside crate the card uses, and frx7's `gpu` crate is the system boundary through which Framesmith reaches it), and section 5.2 (the card's renderer against the processor's: 0.995 or better, and no more than 0.05 % of values further apart than 4 levels of 255); decision A29 of `DECISIONS.md` (the measure called "similarity": the mean structural similarity of brightness over 8x8 windows stepped by 4 pixels, as the `measure` part computes it); decision A27 (a test that needs a graphics card is named "reference machine only" with its reason); `CLAUDE.md` rules 1, 10 and 11 (the card's calls stay in frx7's system crates behind safe types); `docs/spec/render-command.md` sections 4, 7 and 9 (the picture, the summary, the checked-in pictures). The graphics API's public specification gives the rule that a triangle covers the pixels whose centres lie inside it (a rule of rasterisation, taken as a published standard and not as code). frx7's `gpu` crate (`frx7/docs/spec/export-pipeline.md`, section 2, and its own module documentation) gives the card, the wait for submitted work and the read-back of a texture, which this crate uses without copying their code.

This is the behaviour of the card's renderer, and the test of its agreement with the processor's. What a frame is drawn from, and what the processor draws, are in `render-command.md` sections 3 and 4 and are not repeated here: the card draws the same display list, and must give the same picture within the tolerance of section 4 below.

## 1. What the card draws

The card draws the steps of a display list (`draw` crate, `DisplayList`) in order, for this version the `Fill` steps only, exactly as the processor does (`render-command.md` section 4):

1. The picture is `width` by `height` pixels and is opaque: every pixel's alpha is 255.
2. Every pixel first takes the list's background colour.
3. Each `Fill` step, in order, makes every pixel of its rectangle that lies inside the frame take the step's colour. The rectangle is clipped to the frame with `layout::Rect::visible`: its right and bottom edges are exclusive. A rectangle that does not lie in the frame, or has no size, changes nothing.
4. A `Fill` written later covers one written earlier where they overlap. Nothing is blended: a pixel takes the colour of the last step that covers it, exactly.

The card gives the picture of a display list, not of a frame number: a renderer is handed the list and nothing else (`render-command.md` section 8). Two calls with the same list give the same picture, whatever was drawn before.

How the card does this, in words:

- The picture is a texture of 8 bits a channel, read as the colours red, green, blue and alpha, with no conversion between colour spaces: a colour value `v` from 0 to 255 is drawn as the number `v / 255`, and the card writes that number to the 8-bit channel, which it stores as `v`. The texture is not in a gamma-corrected format, so the card and the processor write the same bytes for the same colour.
- Each visible rectangle is drawn as two triangles whose corners lie on the whole pixel boundaries of the frame. A pixel is written exactly when its centre lies inside the rectangle, so the set of pixels a rectangle covers is the set the processor fills.
- The rectangles are drawn in the order of the list, in one pass with no blending and no depth test.
- The picture is read back row by row, with the padding the card adds at the end of each row left out, and gives the same byte layout as a `Picture` of `draw`.

## 2. When the card is not there

The card's renderer is started by `CardRenderer::new()`. A machine with no graphics card that frx7 can draw with, or a card that could not be started, gives an error with the reason, and no picture is drawn. `render --backend gpu` on such a machine refuses the film before the first frame: the report of `FS-0038` with the reason, which names the setting to change, the closing line `Refused:`, exit code 2, and no file (`render-command.md` sections 1.2 and 7). `render --backend auto` (the default) takes the processor on such a machine, and says so in its summary; it never refuses for want of a card (`render-command.md` section 1.2). Nothing falls back from `gpu` to `cpu`.

## 3. The name of each renderer

The renderer's name is what the summary of `render` prints on the line `renderer:` and what the log of the run names (`run-log.md`). The name of the renderer on the processor is `processor`; the name of the renderer on the card is `card`. The summary adds how `auto` came to its choice, in brackets (`render-command.md` section 7).

## 4. The agreement of the two renderers

The card's picture of a frame agrees with the processor's picture of the same display list when both of the following hold. They are checked on every frame of the fixture, and on any frame of a comparison.

| Figure | Measure | Tolerance |
|---|---|---|
| **Similarity** | `measure::similarity` of the two pictures: the mean structural similarity of brightness over 8x8 windows stepped by 4 pixels, of the brightness of the red, green and blue values with the weights 0.2126, 0.7152 and 0.0722 (decision A29) | 0.995 or better |
| **Share of values further apart** | The number of colour values, of the red, green and blue bytes of every pixel (the alpha byte is left out: both pictures have 255 there), whose absolute difference is more than 4 levels of 255, divided by the number of colour values, which is `width` times `height` times 3 | At most 0.05 %: a count `n` of values out of `t` in all is within the tolerance when `n * 2000 <= t` |

The two figures are measured for every frame and reported for every frame, whether the frame passes or not. The share is counted on the whole picture, in whole numbers, and is only compared with the tolerance in that form, so that no rounding can move a frame across the line. A frame whose similarity cannot be measured (the `measure` part gives no figure for it, which happens when the two pictures differ in size or are smaller than one 8x8 window) is not within the tolerance: a comparison never passes a frame it could not measure.

The tolerance is the one of `PLAN.md` section 5.2: the 0.995, the 4 levels and the 0.05 % are its stated values. The similarity is the measure of decision A29. The way the values are counted (the red, green and blue bytes of each pixel and not the alpha byte, and a difference of more than 4 levels, not 4 or more) is not written in `PLAN.md` or in A29; this spec states it, and the owner confirms it.

### 4.1 What the comparison reports

For each frame of the fixture, one line, in the order of the frames:

```
frame 0004: similarity 1.000000, values further apart than 4 levels 0.0000 % (0 of 15552), within the tolerance
```

- `frame` is the frame's number, counted from 0, four digits.
- `similarity` is the figure of section 4, with six decimals.
- `values further apart than 4 levels` is the share, with four decimals and the sign `%`, and in brackets the count of values out of the number of values.
- The last words are `within the tolerance` or `outside the tolerance`.

When any frame is outside the tolerance, the comparison fails and names each frame that is, with the same two figures and the words of the tolerance that frame missed (`similarity 0.812345 is below 0.995`, `12.3456 % of values are further apart than 4 levels, above 0.05 %`). It is never reported as passed.

## 5. The tests

- The tests that need a graphics card are named `reference_machine_only_<what they check>` and carry `#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]` (decision A27; `docs/dev/gates.md` section 4). A plain `cargo test` lists them as not run. The local gate runs them, and a test that fails, or a card that cannot be started, fails the gate. None is reported as passed on a machine without a card.
- The arithmetic of the comparison (the two figures, the strict "more than 4 levels", the exact tolerance, the naming of a frame that is out of it) is tested without a card, on pictures made in the test.

## 6. Doing the comparison by hand

The development-only example `crates/render/examples/compare_renderers.rs` draws the frames of a project on both renderers, prints the lines of section 4.1, and exits 0 when every frame is within the tolerance and 1 when one is not. It is not a command of `framesmith` and is not part of the product. Its option `--doctor <frame>` makes the card's picture of that frame differ on purpose (section 7) so that a check can show the comparison fails.

## 7. A card that draws differently

The comparison must be able to fail. A test makes a card that differs from the processor on one frame: for that frame the card's picture has the red value of every pixel raised by a fixed number of levels (40 in the test and in the example, with the value stopped at 255), and the frames it draws otherwise are the card's own. The comparison then fails naming that frame, with both figures. This is a test device of the comparison and not a setting of the product: no command or option of `framesmith` makes the card draw differently.

## 8. What is not covered

- The card's pictures on cards other than the reference machine's: experiment X1 (`PLAN.md` section 10), not this version.
- The speed of the card's renderer (T111) and the agreement on the whole benchmark set (T112).
- The shapes, text and motion the later tickets add: each of them is proven on both renderers by its own ticket.
