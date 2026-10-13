# The XML notation of the native path: rules decided so far

Status: draft, for the proof of concept of the native path. This file holds the rules that the comparisons of 2026-10-12 showed the notation must state. It is not yet the whole notation; the reader in `crates/poc/src/xml.rs` lists every element and attribute it accepts. Sources: the product owner's orders of 2026-10-12; `FRAMESMITH_SPEED.md`; CSS as a public standard, for the meaning of the look attributes that carry CSS names; the gaps recorded in `PROGRESS.md` after the film test.

## 1. Backgrounds of several layers

`background` is a list of layers separated by commas. The first layer is on top. A layer is a colour or a gradient.

### Colours and gradients

- A colour is `#rrggbb` or `#rrggbbaa`.
- `linear-gradient(<angle>deg, <stop>, <stop>, …)`. The angle is optional and is 180 without it: 0 points up, 90 to the right, 180 down.
- `radial-gradient(<shape> at <x>% <y>%, <stop>, <stop>, …)`. The shape is `ellipse` or `circle`. `at` and its two values are optional; without them the centre is the middle of the layer. An ellipse may be followed by its two radii as percentages of the layer's width and height. A circle may be followed by its radius in pixels. Without radii the shape reaches the farthest corner of the layer.
- A stop is a colour and a place: `#ffffff 40%` is a share of the gradient's length, `#ffffff 2` is a number of pixels from its start. Two stops at the same place make a hard edge.
- Between two stops the colour changes evenly, with the colours weighted by their alpha. A stop of `#00000000` therefore fades the colour before it to nothing and does not darken it.

### Size, place and repeat

Three attributes place the layers. Each is a list separated by commas with one entry for each layer, in the order of `background`, or one entry that applies to every layer.

| Attribute | An entry | Without it |
|---|---|---|
| `background-size` | `auto`, or a width and a height, each in pixels (`24`) or as a share of the element's box (`100%`) | `auto`: the layer is as large as the box |
| `background-position` | an x and a y. In pixels: the offset of the layer's top left corner from the box's. As a percentage: that share of the space left over, so `50%` centres the layer and `100%` puts it against the far edge | `0 0` |
| `background-repeat` | `repeat` or `no-repeat` | `repeat` |

- A layer with a size is one tile. With `repeat` the tile is laid side by side in both directions, from its place, until the box is covered. With `no-repeat` it is drawn once.
- A gradient is measured against its tile, not against the box.
- Layers never show outside the box, and they follow its rounded corners.
- A list with a number of entries that is neither one nor the number of layers is an error, reported with its line. So is one of these attributes on an element without `background`.

### Examples

A grid of dots, 24 pixels apart:

```xml
<box left="0" top="0" right="0" bottom="0"
     background="radial-gradient(circle, #ffffff29 1.4, #00000000 2), #111314"
     background-size="24 24, auto"/>
```

A cross drawn from two bars, each drawn once:

```xml
<box width="34" height="34"
     background="linear-gradient(#5a5c5e 0%, #5a5c5e 100%), linear-gradient(#5a5c5e 0%, #5a5c5e 100%)"
     background-size="2 100%, 100% 2" background-position="50% 0, 0 50%" background-repeat="no-repeat"/>
```

## 2. A composition in another file

A project may hold several composition files. A composition shows another one with a `<composition>` element that names its file:

```xml
<composition id="film" width="1920" height="1080" fps="30" duration="626.667">
  <video src="footage/presenter.mp4" left="0" top="0" width="1920" height="1080"/>
  <composition src="scenes/captions.xml" start="1.167" duration="156.5"/>
  <composition src="scenes/chapters.xml"/>
</composition>
```

- **The file is a whole composition.** It has its own size, frame rate and length, and `render` can draw it alone.
- **`src` is relative to the project folder**, like every other `src`, wherever the file that names it lies.
- **The element is a clip.** Without `start` it starts at 0 of its parent's time. Without `duration` it lasts as long as the composition in its file. Everything in the file counts its time from the clip's start.
- **Size and place.** The element has the size its file gives, so `width` and `height` are not written on it. It takes the place attributes, the animated properties, the look attributes and `<tween>` and `<set>` children as a `<box>` does, and no other children. What the file draws outside its own frame is cut off.
- **Background.** The `background` of the file is the canvas of its own render. It is not drawn when the file is shown inside another composition.
- **Fonts and sound.** The file's fonts are added to those of the composition that shows it. Its sounds play from the clip's start and are cut at the clip's end.
- **Frame rate.** The file must have the frame rate of the composition that shows it. A different rate is an error.
- **Text properties do not pass into the file.** It looks the same wherever it is shown.
- **Nesting.** A file may be shown more than once, and may itself show other files, up to 16 deep. A file that shows itself, directly or through the files it shows, is an error.
- **Problems.** A problem in a file is reported with the name of the file and the line.

## 3. Times and the frame grid

A time may be written with any number of decimals, and is read exactly. A picture exists only on the frame grid. The native path uses the frame grid rule of `frame-time.md` for every time that decides which picture is shown:

> Multiply the time by the frame rate. If the result is within 0.001 of a whole number, take that whole number. Otherwise round up to the next whole number.

- **A clip's window.** The rule is applied to the clip's start and to its end, each counted on the timeline of the whole composition: the starts of the clips around it are added exactly, as written, and the rule is applied once to the sum. Frame `i` shows the clip when `F(start) ≤ i < F(end)`.
- **A video's `media-start`.** The source picture shown on the clip's first frame is `F(media-start)`. Each later frame shows the next source picture, so the picture never slips against the sound.
- **Tweens and sets.** A tween's value at a frame is its value at that frame's exact time. A `<tween>` or `<set>` written within a thousandth of a frame after a frame counts as on that frame.
- **Sound** is not on the frame grid: a sound starts at the sample nearest its written time.

So at 30 frames a second a clip written to start at 105.0333 s starts on frame 3151, one written to start at 45.6667 s starts on frame 1370, and `media-start="95.0333"` shows source picture 2851 on the clip's first frame. A time written with four decimals lands on the frame its author meant, whichever way it was rounded.

## 4. The sound of a render

- **Length.** The sound is exactly as long as the picture: the number of frames divided by the frame rate. Where no sound plays, there is silence.
- **Mix.** Every sound is decoded to the rate and channels of the output and added to the others, each multiplied by its `volume` and its `<volume>` points.
- **Peak.** The finished mix must not rise above 1 dB under full scale, measured between the samples as well as on them (true peak, −1 dBTP). A mix that stays under is left as it is. A mix that goes over is lowered as a whole by the least amount that brings its highest peak to that limit. Nothing is compressed and the balance between the sounds does not change. `render` says by how much the mix was lowered.

## 5. Blur and brightness of an element

`blur` and `brightness` work on the finished picture of an element: its own background, border, text or picture, and everything inside it, drawn together first.

- **`blur`** is a length in the element's own pixels. The picture is blurred with a Gaussian blur of that standard deviation. 0 leaves it.
- **`brightness`** is a number, 0 or more. Every colour of the picture is multiplied by it and stops at white. 1 leaves the picture, 0 makes it black, 2 makes it twice as bright. The transparency of the picture does not change.
- When an element has both, it is blurred first and brightened second.
- Both come before the element's cut (`overflow`, `clip-path`), its `opacity` and its `mix-blend-mode`.
- Both can be animated with `<tween>` and `<set>`.

Layers that are partly transparent are joined before the brightness counts. A quarter of white over `#202020` is `#585858`; with `brightness="2"` the element shows `#b0b0b0`.

## 6. Shared styles

Attributes that many elements have in common are written once, in a `<style>`, and taken by name:

```xml
<composition width="1920" height="1080" fps="30" duration="10">
  <style id="label" font-family="IBM Plex Mono" font-size="27" letter-spacing="2.43" color="#696b6d"/>
  <style id="loud" color="#ffffff" font-weight="700"/>

  <text class="label" left="262" top="76">TITLE</text>
  <text class="label loud" left="262" top="120">CHAPTER 01</text>
  <text class="label" left="262" top="164" color="#ff4040">LIVE</text>
</composition>
```

- **A `<style>`** stands directly in the `<composition>`. It has an `id`, which is its name, and any number of other attributes. It draws nothing and has no children.
- **`class`** on an element names one style or several, separated by spaces. The element is read as if the attributes of those styles were written on it.
- **Which value counts.** An attribute written on the element itself wins. Among the styles, one named later in `class` wins over one named before it. So the second text above is white and the third is red.
- **Any element may take a style**, and a style may hold any attribute. An attribute is checked on the element that takes it: a style with `object-fit` taken by a `<text>` is an error on that `<text>`.
- **A style belongs to its file.** A composition shown from another file does not see the styles of the composition that shows it, and the other way round.
- **Motion is not shared.** `<tween>` and `<set>` stay inside their element.
- **Errors**, each reported with its line: a `<style>` without `id`; two styles with one `id`; a `class` that names a style the file does not have; a `<style>` inside an element; a `<style>` with `class` or with children.

There are no selectors: an element takes a style only by naming it.

## 7. Text: lines and writing direction

Sources: the Unicode line-breaking rules (UAX #14) and bidirectional algorithm (UAX #9), both public standards; CSS as a public standard, for the collapsing of white space; experiment E3 of `POC_PLAN.md`, which showed that laying text out word by word orders right-to-left words wrongly and never breaks a script written without spaces.

- **One paragraph.** The words of a `<text>` and its spans are one paragraph. A span is a box at its place among the words.
- **White space.** A run of spaces, tabs and line ends between words is one space. White space at the start and at the end of the text is dropped. A no-break space is not white space: it is kept, and no line ends at it.
- **Where a line may end.** Where the Unicode line-breaking rules allow it: at a space, after a hyphen, between two ideographs or kana, and, in scripts written without spaces such as Thai, between words as a dictionary finds them. A word wider than its line is not broken. With `white-space="nowrap"` no line ends before the text does.
- **A line's width** does not count the space it ends at.
- **Direction.** The paragraph runs from left to right. Within a line, a run of a right-to-left script is ordered by the Unicode bidirectional algorithm, so Arabic and Hebrew read correctly, also next to Latin words and numbers. A paragraph that runs from right to left as a whole is left for later.
- **Shaping.** Letters are joined and placed as their script needs, by the font's own tables.
- **A character the named font lacks** is drawn from another font. Today that font is one of the system's, so the picture depends on the machine; a rule that names it is left for later.
- **Colour glyphs** (emoji) are drawn in their own colours; `color` does not change them.

## 8. Turning in space and opening transitions

Sources: CSS as a public standard for the meaning of perspective and of turning about the x and y axes; experiment E6 of `POC_PLAN.md`. Both are done to the finished picture of an element and everything in it, as blur and brightness are (section 5).

### Turning in space

- `perspective` on an element is the distance of the eye from the frame, in pixels, above 0; another number is refused when the composition is read. Without it the element is not turned. The eye is straight above the element's `transform-origin`.
- `rotate-x` and `rotate-y` are animated properties, in degrees, 0 when not written. `rotate-x` turns the element about a horizontal line, top edge away from the eye for a positive angle; `rotate-y` about a vertical line, right edge away from the eye for a positive angle. When both are written the element is turned about the horizontal line first.
- The lines go through the element's `transform-origin`, at the place the element's other transforms put it.
- A point that the turn brings nearer to the eye is drawn larger and further from the origin, one further away smaller: its distance from the origin is multiplied by `perspective / (perspective - z)`, where `z` is how far the turn brought it towards the eye.
- The element is drawn flat first, with its children, its own transforms, its cut, its blur and its opacity; that picture is then turned. Pixels of the turned picture are taken from between the pixels of the flat one.
- An element that is turned edge-on, or whose turn brings a part of it to the eye or behind it, is not drawn.

### Opening transitions

- `transition` on an element names how it opens: `none` (when it is not written), `wipe` (from its left edge to its right, with a soft edge a tenth of its width wide), `cut` (from its left edge to its right with a hard edge: at `reveal` r the element shows from its left edge to r of its width, and nothing of it beyond; added for experiment E9, where words of a sample film come in this way) or `iris` (a circle that grows from its middle, with a soft edge a tenth of its half diagonal wide).
- `cut` is an ordinary cut of the element's box, the same in both renderers, and holds also for an element that is turned in space. An element has one cut: `cut` together with `clip-path`, or with `overflow="hidden"` and rounded corners, is refused.
- `reveal` is an animated property from 0 (nothing of the element shows) to 1 (all of it shows); 1 when not written. It has an effect only on an element with a `transition`.
- What is not yet open shows whatever lies under the element, so a scene laid over another and opened is a transition from the one to the other.
- An element has one of the two: when it is turned in space, its `transition` waits.

### An opening written as a shader

Sources: the WebGPU shading language as a public standard; the idea of a transition as a function of each pixel and of how far the transition has come, which is common to video tools; experiment E6 of `POC_PLAN.md`. The form of the function and its names are ours.

- `transition` may name a file of the project that ends in `.wgsl`. The file holds one function in the WebGPU shading language, and whatever functions of its own it needs:

  ```
  fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32>
  ```

- `place` is where the pixel lies in the element's box, from 0 to 1 across and down; outside the box it is below 0 or above 1. `size` is the box's width and height in pixels. `progress` is the element's `reveal`.
- The result is the pixel's colour with its alpha, the colour already multiplied by the alpha. Each number is brought into 0 to 1.
- The function may call `picture(place: vec2<f32>) -> vec4<f32>`: the colour of the element's flat picture at a place of its box, in the same form, taken from between the four pixels around the place, and transparent outside the picture. It may also call `open_at(progress: f32, place: f32) -> f32`, the share shown by an opening with a soft edge a tenth wide, which the two openings above are made of.
- The function is asked for every pixel of the element's flat picture that lies in the frame, while `reveal` is below 1. At 1 the element is drawn without it, so the function should give `picture(place)` there.
- The two openings above are functions of this form: `wipe` gives `picture(place) * open_at(progress, place.x)`.
- Only the GPU renderer draws such an opening, and this is accepted (the product owner's decision of 2026-10-13): a composition that has one needs a machine with a graphics card, and its picture has no second renderer to be checked against. `check` says so for each such opening, with the line of the first element that names it. A composition that has one is refused by the CPU renderer before the first frame. A file that is not found, or that the card's compiler does not take, stops the render before the first frame, with the file and the compiler's words.
- An element with such an opening cannot be blurred or brightened itself while it opens; what is inside it can.

## 9. A colour table on footage

Sources: the `.cube` text form for colour tables as a public file format (its keywords and the order of its rows); experiment E6 of `POC_PLAN.md`.

- `lut` on a `<video>` names a `.cube` file of the project. Every colour of the video is replaced by the colour the table gives for it.
- The table is applied to the video's colours as they are shown (8 bits a channel, after the conversion from the source's own colour form), before the steps of `filter`. Its result is a whole level before the filter sees it.
- The table has `N` points along each of red, green and blue, `N` from 2 to 128, given by `LUT_3D_SIZE`. Its rows are three numbers each, red first, with red changing quickest, then green, then blue.
- A colour between points takes its place between the eight points around it: between the two along red first, then along green, then along blue. A result outside 0 to 1 is brought to the nearer end.
- Lines that are empty or begin with `#` are passed over, and so is a `TITLE`.
- Refused when the composition is read, with the line of the `<video>`, the file, and the line of the file where there is one: a file that is not found; a table of one dimension (`LUT_1D_SIZE`); a range other than 0 to 1 (`DOMAIN_MIN`, `DOMAIN_MAX`); a keyword that is not known; a row that is not three numbers; a number of rows other than `N * N * N`.
- Both renderers give the same colours to within one level.

## 10. More easing curves

Sources: the sine, exponential and fifth-power easing curves as they are commonly defined in public (the same family as the curves the motion engine has); experiment E9 of `POC_PLAN.md`, where two sample films used them in most of their tweens.

`ease` also takes these nine names. `p` is how far the tween has come, from 0 to 1; the result is how far its value has come.

| Name | Result |
|---|---|
| `in-quint` | `p^5` |
| `out-quint` | `1 - (1 - p)^5` |
| `in-out-quint` | `16 p^5` below one half, `1 - (2 - 2p)^5 / 2` from there |
| `in-sine` | `1 - cos(p * pi / 2)` |
| `out-sine` | `sin(p * pi / 2)` |
| `in-out-sine` | `(1 - cos(p * pi)) / 2` |
| `in-expo` | 0 at 0, else `2^(10p - 10)` |
| `out-expo` | 1 at 1, else `1 - 2^(-10p)` |
| `in-out-expo` | 0 at 0, 1 at 1; `2^(20p - 10) / 2` below one half, `(2 - 2^(10 - 20p)) / 2` from there |

Every curve gives exactly 0 at 0 and exactly 1 at 1.

## 11. A number that counts

Sources: experiment E9 of `POC_PLAN.md`, where three scenes of two sample films show a number that runs up to its value and our render showed its first value throughout; the product owner's word of 2026-10-13 that this must work. The forms of number are ours.

A `<text>` with the attribute `count` shows a number and no words. `count` is the number it starts with, and it is an animated property like the others: a `<tween>` or a `<set>` changes it (`from-count`, `to-count`, `count`). At each frame the text shows the number of that frame's time, written as `count-format` says.

| `count-format` | 1234.567 reads | What it is |
|---|---|---|
| `0` (the default) | `1235` | A whole number. |
| `0.0`, `0.00`, … up to six decimals | `1234.6`, `1234.57` | That many decimals, always shown. |
| `0,0`, `0,0.00`, … | `1,235`, `1,234.57` | The same, with a comma between every three digits of the whole part. |
| `m:ss`, `m:ss.0`, … up to six decimals | `20:35`, `20:34.6` | The number is a count of seconds, shown as minutes and seconds. The seconds always have two digits. |

- The number is rounded to its last shown place first, a half away from zero, and written after that: 59.96 seconds read `1:00.0`, not `0:60.0`.
- A number below zero begins with `-`. A number that rounds to zero has no sign.
- The text is one line and is never broken.
- **Its box does not change while it counts.** The box is as wide as the widest number the text shows in any frame of the composition, so what stands beside it keeps still. Within the box the number stands as `text-align` says (at the left by default).
- Everything else of a text holds: font, colour, shadow, its own tweens.

Refused when the composition is read:

- `count` or `count-format` on an element that is not a `<text>`: `` `count` is for a <text>, not a <box> ``.
- A `<text>` that counts and has words or a `<span>` of its own: `a <text> that counts holds no words of its own: its text is the number`.
- `count-format` without `count`: `` `count-format` needs `count` ``.
- A `<tween>` or `<set>` of `count` on an element without the attribute `count`: `` a tween of `count` needs the attribute `count` on its <text> ``.
- A form that is not in the table: `` `count-format="…"` is not a form of number; the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals ``.

## 12. An outline that draws itself, and dashes

Sources: experiment E9, where a ring and a line on a map draw themselves in the sample films and our render showed them whole from the first frame; the common behaviour of drawing tools and of SVG's dashes as a public standard (a line shown up to a share of its length; dashes measured along the line from its start). The attribute names are ours.

Both are for a `<shape>` that has `stroke-width`.

**`draw`** is an animated property, 1 by default: the share of each outline that is drawn, counted from the outline's start along its length.

- At 1 or more the whole outline is drawn. Below one thousandth nothing is drawn, not even the round end of the line: a line that starts to draw itself is not there as a dot in the frame before.
- Between the two, each outline is drawn from its start to that share of its own length. A `d` that holds several outlines (several `M`) has each of them drawn to the same share.
- An outline starts where its `d` starts and runs the way its `d` runs. A closed outline is drawn round to its start.

**`dash`** makes the line a row of dashes: pairs of lengths, a dash and then a gap, in view-box units, repeated along each outline from its start. `dash="12 8"` is a 12-unit dash and an 8-unit gap. The ends of each dash are round, as the ends of every line of a shape are.

With both, the dashes lie where they would on the whole outline, and `draw` shows them up to its share.

Refused when the composition is read:

- `draw` or `dash` on an element that is not a `<shape>` with `stroke-width`, as an attribute or in a `<tween>` or `<set>`: `` `draw` is for a <shape> with `stroke-width` `` (and the same with `dash`).
- A `dash` that is not an even count of numbers, none below zero and not all zero: `` `dash="…"` is not pairs of lengths: a dash and a gap, in view-box units ``.

## 13. A row spread out, a shared baseline, columns, and a widest width

Sources: experiment E9, where every frame of two sample films was compared with a reference render pixel by pixel (the product owner's word of 2026-10-13: the pictures should match in 99 percent of their pixels). The largest differences of one film were three ways of laying boxes out that the notation lacked. They are the common behaviour of CSS layout as a public standard; what is taken on is said here, in our words, and nothing else of those CSS features exists.

### `justify-content="space-between"`

In a row or a column, the first item lies at the start, the last at the end, and the room left over is shared equally between each two neighbours, on top of `gap`. With one item, or with no room left over, it is the same as `flex-start`.

### `align-items="baseline"` and `align-self="baseline"`

In a row, the items that have it are set so that the baselines of their first lines lie on one line, and the highest of them touches the top of the row.

- The baseline of a `<text>` or `<span>` is the baseline of its first line.
- The baseline of a box that is a row, a column or a set of columns is that of its first item, counted from the box's top.
- Anything else has its baseline at its bottom edge.
- A row with no `height` is as high as its items need when they stand like this.
- In a column, and in a cell of a set of columns, `baseline` is the same as `flex-start`.

### `columns`

A `<box>` with `columns` lays its children out in columns, row by row: the first child in the first column, the next in the second, and after the last column a new row begins.

`columns` is a list of widths, one for each column, with spaces between them. A width is a number of pixels (`640`) or a share of what is left (`1fr`, `2fr`): the room that the pixel columns and the gaps leave is divided among the `fr` columns in proportion to their numbers.

- `gap` is the space between two columns and between two rows.
- An item is as wide as its column, less its margins, unless it has a `width` of its own; then it lies at the left of its column.
- A row is as high as its tallest item. When the box has a height and the rows need less, what is left over is shared equally among the rows.
- In its row an item stands as `align-items` of the box says (`stretch` by default: as high as the row), or as its own `align-self` says.
- `padding` holds as for a row. A child with `position="absolute"` is not an item and is placed by `left`, `top`, `right`, `bottom`.
- A box with no width of its own: an `fr` column is as wide as its widest item.

Refused when the composition is read:

- `columns` together with `flex-direction`: `` a <box> has `columns` or `flex-direction`, not both ``.
- A `columns` that is not a list of widths, or that holds a width that is not above zero: `` `columns="…"` is not a list of column widths: pixels, or shares such as `1fr` ``.
- `columns` on an element that is not a `<box>`: `` `columns` is for a <box> ``.

### `max-width`

`max-width` is a number of pixels: the element is never wider than that when its width comes from somewhere other than its own `width`. It holds for an item that its row, column or cell would make wider, and for a text with no `width`, which then breaks into lines at that width. An element with a `width` of its own ignores it.

## 14. The kind of figures a text uses

Sources: experiment E9, where a clock and a number in the sample films ask for figures of one width and of one height and our render showed the font's own; OpenType's figure features as a public standard. The attribute carries the name and the values CSS gives it.

`font-variant-numeric` is a text property, handed down to children like the others. It is `normal` (the font's own figures, the default) or a list, with spaces between, of:

| Value | The figures are |
|---|---|
| `lining-nums` | all as high as a capital letter |
| `oldstyle-nums` | of the height of small letters, some reaching above and below |
| `tabular-nums` | all of one width, so that a number does not change its width as it counts |
| `proportional-nums` | each of its own width |

A font that does not hold the kind asked for shows its own figures. Anything else is refused: `` `font-variant-numeric="…"` is not `normal` or a list of: lining-nums, oldstyle-nums, tabular-nums, proportional-nums ``.

## 15. A picture fitted whole, and where a picture is held

Sources: experiment E9, where two scenes of a sample film show a photograph cut to fill its box and held at its top, and ours held its middle and showed another part of it (the largest difference of that film); CSS's picture fitting as a public standard, whose names and values the two attributes carry.

Both are for an `<image>`.

`object-fit` says how the picture meets its box:

| Value | The picture is |
|---|---|
| `fill` (the default) | stretched to the box, whatever its shape |
| `cover` | scaled evenly until it covers the box; what reaches beyond the box is cut off |
| `contain` | scaled evenly until all of it is in the box; the rest of the box shows what is under it |

`object-position` says where the picture is held when it does not fill the box exactly: two percentages, across and then down, `50% 50%` by default. `0%` puts the picture's left (or top) edge on the box's, `100%` its right (or bottom) edge on the box's, and a value between them divides what reaches beyond the box (`cover`) or the room left over (`contain`) in that proportion. `object-position="50% 0%"` holds a portrait at its top. With `fill` it does nothing.

Refused when the composition is read: an `object-position` that is not two percentages (`` `object-position="…"` is not two percentages, across and down ``), and either attribute on an element that is not an `<image>`, as any attribute an element does not know.

## 16. More ways to blend

Sources: experiment E9, where a sample film lays a flat olive box over grey footage with the blend `color`, to tint the footage without changing how light it is; ours, lacking the blend, laid the box over as a veil and the picture came out paler. The W3C's Compositing and Blending recommendation as a public standard, whose names and formulas the values carry.

`mix-blend-mode` on an element says how its finished picture (itself, its children, its cut, its blur, its opacity) is mixed with what is under it. Besides `normal`, `multiply` and `screen` it takes:

| Value | Each colour channel of the result, from the element's colour `s` and the colour under it `b` (0 to 1) |
|---|---|
| `overlay` | `hard-light` with the two colours changed about |
| `darken` | the lesser of `s` and `b` |
| `lighten` | the greater of `s` and `b` |
| `color-dodge` | `b / (1 - s)`, at most 1; 0 where `b` is 0 and 1 where `s` is 1 |
| `color-burn` | `1 - (1 - b) / s`, at least 0; 1 where `b` is 1 and 0 where `s` is 0 |
| `hard-light` | `multiply` of `b` and `2s` where `s` is at most a half, `screen` of `b` and `2s - 1` above it |
| `soft-light` | a gentler `hard-light`, by the standard's formula |
| `difference` | the distance between `s` and `b` |
| `exclusion` | `s + b - 2sb` |

and four that work on the colour as a whole, where the lightness of a colour is `0.3 R + 0.59 G + 0.11 B` and its saturation the distance between its greatest and its least channel:

| Value | The result has |
|---|---|
| `hue` | the hue of the element, the saturation and the lightness of what is under it |
| `saturation` | the saturation of the element, the hue and the lightness of what is under it |
| `color` | the hue and the saturation of the element, the lightness of what is under it |
| `luminosity` | the lightness of the element, the hue and the saturation of what is under it |

Where the element is partly clear, the result is mixed with what is under it by the element's alpha, as for every blend. Both renderers draw every one of them. A value that is not in the list is refused, as for every attribute that takes one of a list.

## 17. A shadow that follows any element, a torn edge, and uneven ink

Sources: experiment E9, where a sample film gives a cut-out photograph a paper shadow, tears the edges of its paper scraps and makes the ink of its rubber stamps uneven, all three with filters of its page; ours had none of the three, and they were what was left between the two renders of three of its scenes. CSS's `drop-shadow` and the fractal noise, the moved pixels and the alpha arithmetic of the W3C's Filter Effects recommendation, as public standards: the noise is the standard's own, number for number. How a browser places the noise and moves the pixels was measured on small pages of our own (a plain box, the noise alone, then the noise moving the box), until a prototype of the rules below gave the browser's picture with no pixel differing.

The three attributes work on the **finished picture** of an element: itself and its children, with its own transforms, its cut, its blur and its brightness. They come in this order: `displace`, `grain`, `drop-shadow`; the element's `opacity` comes after them, as CSS has it, so the shadow of a half clear element does not show through it; its `mix-blend-mode` comes last. An element that carries one of them is drawn apart, as one that is blurred is.

### The noise

`noise(FREQUENCY OCTAVES SEED)` names a field of numbers between 0 and 1 over the element's box: the alpha channel of the standard's fractal noise, with the same base frequency in both directions, that many octaves (1 to 8), and the seed (a whole number).

- The noise is read at a point of the element's own box, in its own pixels from its top left corner, **half a pixel on** in both directions: the pixel of the frame whose centre lies at `p` in the element's units reads the noise at `p + (0.5, 0.5)`. The element's transforms and those around it move the noise with the element.
- The number is kept in 256 steps: `round(255 n) / 255`.
- A seed that is not above 0 stands, by the standard, for the seed one above its size: `-7` is `8`, `0` is `1`.

### `displace="AMOUNT noise(…)"`

Every pixel of the finished picture is taken from another place of it: the pixel whose noise is `n` shows what lay `AMOUNT × (n − 0.5)` of the element's pixels further on, **the same distance across and down**. An `AMOUNT` of 14 moves the edge of a paper scrap in and out by up to 7 pixels and makes it look torn.

- The distance is turned and scaled as the element is, and then rounded to whole pixels of the frame, across and down each on its own. No pixel is mixed with its neighbour: the torn edge is hard.
- What is taken from outside the picture is clear.
- `AMOUNT` is a number above 0.

### `grain="noise(…) SLOPE OFFSET"`

The alpha of every pixel of the finished picture is multiplied by `SLOPE × n + OFFSET`, held between 0 and 1. `grain="noise(0.9 2 5) -1.7 1.75"` leaves most of a stamp's ink and takes it away where the noise is high.

### `drop-shadow="X Y BLUR COLOUR"` on any element

Known on a `<shape>` from section 9, where it follows the outlines. On every other element it follows the finished picture: the picture's alpha, blurred with a standard deviation of half of `BLUR`, moved by `X` and `Y`, in `COLOUR`, lies under the picture. `X`, `Y` and `BLUR` are in the element's pixels and are turned and scaled as the element is.

- The blur is three passes of an average over a span of pixels, as the standard allows for a Gaussian: the span is `floor(σ × 3 × sqrt(2π) / 4 + 0.5)` for a standard deviation σ in pixels of the frame.

### Refused when the composition is read

- `` `displace="…"` is not an amount above 0 and a noise: `14 noise(0.035 3 9)` ``
- `` `grain="…"` is not a noise, a slope and an offset: `noise(0.9 2 5) -1.7 1.75` ``
- a noise whose frequency is not above 0, whose octaves are not a whole number from 1 to 8, or whose seed is not a whole number, by the same two messages
- `` `drop-shadow="…"` is not one shadow: x, y, blur and a colour `` (as on a shape)

### What it costs

The picture of an element with one of the three is drawn by the CPU renderer in both renderers, every frame, at the size of the part of it that shows. The GPU renderer puts that picture on its frame.

## 18. A border of two lines

Sources: experiment E9, where a sample film frames its rubber stamps with CSS's `border-style: double`; ours drew one thick line. CSS Backgrounds and Borders, as a public standard.

`border="9 double #b3271e"` draws two lines with a gap between them, in the room one line of that width takes: the outer line, the gap and the inner line are each a third of the width. Everything else is as for a border of one line: where it lies, its corners, its opacity.

- The word `double` stands between the width and the colour. No other word is taken there.
- Refused when the composition is read, as any border that cannot be read: `` `border="…"` is not a width and a colour ``.

