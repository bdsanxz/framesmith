# The card's renderer, and its agreement with the processor's

Status: in force from Sprint 1 (ticket T039), for the renderer on the graphics card in `draw-gpu`, which draws the fills of section 1 and the text of section 9, and for the comparison of the two renderers. Sources: `PLAN.md` section 1, point 2 (the processor's renderer defines the picture, the card's is the default and must agree with it), section 3 (the table of crates: `draw-gpu` is the renderer on the card; `wgpu` is the outside crate the card uses, and frx7's `gpu` crate is the system boundary through which Framesmith reaches it), and section 5.2 (the card's renderer against the processor's: 0.995 or better, and no more than 0.05 % of values further apart than 4 levels of 255); decision A29 of `DECISIONS.md` (the measure called "similarity": the mean structural similarity of brightness over 8x8 windows stepped by 4 pixels, as the `measure` part computes it); decision A27 (a test that needs a graphics card is named "reference machine only" with its reason); `CLAUDE.md` rules 1, 10 and 11 (the card's calls stay in frx7's system crates behind safe types); `docs/spec/render-command.md` sections 4, 7 and 9 (the picture, the summary, the checked-in pictures). The graphics API's public specification gives the rule that a triangle covers the pixels whose centres lie inside it (a rule of rasterisation, taken as a published standard and not as code). frx7's `gpu` crate (`frx7/docs/spec/export-pipeline.md`, section 2, and its own module documentation) gives the card, the wait for submitted work and the read-back of a texture, which this crate uses without copying their code. From T046 part 3 (order t046g): `text.md` sections 1, 9 and 10; the OpenType `head` table, for the box of the glyphs (section 9.3); and the rasteriser vello_cpu 0.3.0, the processor's, which makes the card's glyph masks through the calls `draw-cpu` makes (section 9.2).

This is the behaviour of the card's renderer, and the test of its agreement with the processor's. What a frame is drawn from, and what the processor draws, are in `render-command.md` sections 3 and 4 and are not repeated here: the card draws the same display list, and must give the same picture within the tolerance of section 4 below.

## 1. What the card draws

The card draws the marks of a frame (`draw` crate, `Frame`) in order, exactly as the processor does (`render-command.md` section 4). A mark is a `Fill` step or a paragraph of text (section 9). For a frame with fills only, the picture is that of the frame's display list:

1. The picture is `width` by `height` pixels and is opaque: every pixel's alpha is 255.
2. Every pixel first takes the list's background colour.
3. Each `Fill` step, in order, makes every pixel of its rectangle that lies inside the frame take the step's colour. The rectangle is clipped to the frame with `layout::Rect::visible`: its right and bottom edges are exclusive. A rectangle that does not lie in the frame, or has no size, changes nothing.
4. A mark written later covers one written earlier where they overlap. A `Fill` is not blended: a pixel takes the colour of the last step that covers it, exactly. A paragraph of text is blended over what is there, by the coverage of each glyph (section 9).

The card gives the picture of a frame, not of a frame number: a renderer is handed the frame and nothing else (`render-command.md` section 8). Two calls with the same frame give the same picture, whatever was drawn before.

How the card does this, in words:

- The picture is a texture of 8 bits a channel, read as the colours red, green, blue and alpha, with no conversion between colour spaces: a colour value `v` from 0 to 255 is drawn as the number `v / 255`, and the card writes that number to the 8-bit channel, which it stores as `v`. The texture is not in a gamma-corrected format, so the card and the processor write the same bytes for the same colour.
- Each visible rectangle is drawn as two triangles whose corners lie on the whole pixel boundaries of the frame. A pixel is written exactly when its centre lies inside the rectangle, so the set of pixels a rectangle covers is the set the processor fills.
- The marks are drawn in the order of the frame, in one pass with no depth test: a rectangle is drawn without blending, a glyph with the blending of section 9.
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
- The shapes and the motion the later tickets add: each of them is proven on both renderers by its own ticket. The text of this version is proven on the card by section 9; the text features the later tickets add (spans, transforms, figures, shadows, outlines, other scripts) are proven on both renderers by their own tickets.

## 9. Text on the card

A paragraph of text is a mark of the frame (`draw`, `TextMark`): its glyph runs, in order, and its clip. The card draws it as follows. Its picture agrees with the processor's, and the agreement is measured by section 4.

### 9.1 What is drawn

- The runs of the paragraph are drawn in their order, and the glyphs of each run in their order.
- A glyph is drawn at its origin, in pixels of the frame, at the size of its run, sheared about its origin by the run's skew (`draw`, `GlyphRun`), in the run's colour.
- Its coverage is the share of each pixel that the glyph's outline covers, as the processor measures it: an integer from 0 to 255 for each pixel. A glyph does not change a pixel it does not cover.
- The paragraph changes only the pixels of its clip, which is the clip given by `layout::Rect::visible` for the frame, or the whole frame when there is no clip. A paragraph whose clip has no visible pixel changes nothing.

### 9.2 The coverage of a glyph, kept as a mask

The card makes the coverage of a glyph once for each key, and keeps it for the life of its renderer. The key is:

- the bytes of the font, by the identity of their shared data, and the face index;
- the glyph's index in the font;
- the size and the skew, each as its exact bit pattern;
- the fraction of the origin's x coordinate and of its y coordinate, each the part after the whole part, in the range 0 to 1, as its exact bit pattern.

A mask is made by the rasteriser of the processor, vello_cpu 0.3.0, with the calls the processor makes for a run (`draw-cpu`): one glyph of the font, at the size and with the skew of the key, filled in opaque white, so that the alpha of each pixel of the mask is the glyph's coverage. The glyph's origin is put at a whole pixel of the mask, plus the fraction of the key.

The fraction is in the key because the processor fills a glyph at its exact origin. A mask made for a rounded origin, or for a quantised one, would move every edge of the glyph by part of a pixel, and the edges are what the comparison of section 4 counts. The placement of a mask on whole pixels is exact, so only the fraction needs a key of its own. The fraction itself is exact: subtracting the whole part from a 32-bit number leaves no rounding.

### 9.3 The box of a mask

The box of a mask comes from the font's `head` table, which gives the box of all its glyphs (OpenType specification, `head`). Its numbers are read in big-endian order: `unitsPerEm` as an unsigned 16-bit integer at byte 18 of the table, and `xMin`, `yMin`, `xMax` and `yMax` as signed 16-bit integers at bytes 36, 38, 40 and 42. The table is found through the table directory of the font, which begins at byte 0 of a file of one face. For a size `s` the scale is `s / unitsPerEm`, and `t` is the tangent of the skew:

- `bx = max(|xMin|, |xMax|) * scale + |t| * max(|yMin|, |yMax|) * scale`;
- `by = max(|yMin|, |yMax|) * scale`.

The origin of the mask is `ox = ceil(bx) + 2` pixels from its left edge and `oy = ceil(by) + 2` pixels from its top edge; the mask is `2 * ox + 2` pixels wide and `2 * oy + 2` high. The margin of two pixels absorbs the rounding of the box, and the last pixel holds the fraction of the origin.

Several checks guard the picture, and each stops the draw with an error that gives its reason and draws no picture:

- a font that is a collection (its first four bytes are not the tag of one face: `0x00010000`, `true` or `OTTO`), or whose `head` table is missing or cannot be read, or whose `unitsPerEm` is 0;
- a glyph of a face other than the first face of its file, which the card does not draw;
- a size that is not a finite number of zero or more, or an origin that is not finite;
- a mask whose outermost ring of pixels holds any coverage, which means that the outline lies outside its box, so that a glyph is never drawn clipped in silence;
- a mask larger than the card's limit on the sides of a texture.

A glyph with no coverage at all (a space, or a size of zero) draws nothing.

### 9.4 Drawing the glyphs

For each glyph of a run, in order, the mask of its key is made if it is not kept yet (sections 9.2 and 9.3). Its box is drawn as two triangles whose corners lie on whole pixels, at `floor(x) - ox` and `floor(y) - oy`, with the size of the mask, as a fill is drawn. A pixel of the box takes the coverage of the mask at the same position, read without filtering. The run's colour is blended over the picture by that coverage: the picture's colour becomes the colour times the coverage, plus the picture's colour times what the coverage leaves clear; the alpha stays 255. The clip of the paragraph is the scissor of these draws.

A pixel `(a, b)` of the mask is the frame pixel `(floor(x) - ox + a, floor(y) - oy + b)`. The glyph's origin lies at `ox` plus its fraction inside the mask, so the mask is the processor's picture of the glyph moved by a whole number of pixels.

### 9.5 Agreement with the processor

The processor composes each paragraph in a layer and then lays the layer over the picture. Laying a layer over a picture is the same operation as laying each glyph over the picture in turn, because the src-over operation is associative. So the card's picture equals the processor's in exact arithmetic, for any colours of overlapping glyphs. The two differ only by rounding: the processor rounds the layer and the picture to whole levels, and the card rounds at each write. The difference is measured by section 4; no other tolerance is stated for text.

### 9.6 What is kept

The masks are kept for the life of the renderer, so a glyph that stays in place is made once. No mask is evicted in this version: the memory of a composition grows with the number of different keys it draws. The picture does not depend on what was kept: a frame drawn from kept masks is the frame drawn from new ones.

### 9.7 Tests

- `crates/render/tests/card_agreement.rs`: every frame of each of the five text fixtures of T046 is within the tolerance of section 4. Reference machine only.
- `crates/draw-gpu/tests/card_text.rs`: a frame of the test font, with origins at fractions, a skew, a clip and a fill over the text, agrees with the processor's frame within the tolerance; and a font whose table directory cannot be read is refused by the card with its reason, and draws no picture. Reference machine only.
- The emoji tests of section 9.8.

### 9.8 Pictures of emoji

A picture of an emoji (`draw`, `GlyphPicture`; `text.md`, section 12.4) is drawn after the glyph runs of its paragraph, in the order of the paragraph's pictures, within the paragraph's clip (section 9.1). Its pixels are premultiplied RGBA of 8 bits, as the text crate gives them, and it is drawn as two triangles whose corners lie on whole pixels, at the place `text.md` section 12.4, point 6, gives it.

- Each frame pixel takes the picture's pixel at the same position, read without filtering, so the card changes no pixel of the picture.
- The picture is blended over the frame by the premultiplied src-over operation: the frame's colour becomes the picture's colour plus the frame's colour times the share of the frame the picture's alpha leaves clear. The alpha stays 255.
- A glyph of a colour-picture face is never a mask: the card makes no mask of it, and so the picture needs none of the checks of section 9.3.
- A picture larger than the card's limit on the sides of a texture is refused with its reason, and draws no picture, as a mask is (section 9.3).

The two pictures agree in the way section 9.5 gives: the processor composes the picture into the paragraph's layer and the card composes it at each write, which is the same in exact arithmetic, so the difference is rounding, measured by section 4. The fixture `text-emoji` (`text.md`, section 12.5) is in `crates/render/tests/script_fixtures_on_the_card.rs`, reference machine only, in three tests. `reference_machine_only_the_card_agrees_with_the_processor_on_emoji_text` compares the card's picture of every frame of the fixture with the processor's, as section 4 gives. `reference_machine_only_the_card_draws_the_emoji_in_its_own_colours_whatever_the_text_colour` checks frame 0 at the emoji's place: the card draws the emoji's yellow there and no pixel takes the blue of the text. `reference_machine_only_the_card_draws_the_emoji_within_the_tolerance_whatever_the_text_colour` draws the fixture with its text in black, in its blue and in a yellow close to the emoji's own, and compares the card's pictures of the emoji with one another, on frames 0 and 9, within the tolerance of section 4. No test in `crates/draw-gpu/tests/` draws an emoji picture.
