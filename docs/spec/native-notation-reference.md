# The XML notation: a reference for writing compositions

Status: reference, written from the reader (`crates/poc/src/xml.rs`) and the code that uses what it reads. It says what the program accepts and what it does with it. Where it is not sure, it says so in section 9.

The long rules (backgrounds of several layers, compositions in other files, the frame grid, sound, blur and brightness, shared styles, text lines, turning in space and opening transitions, colour tables) are also in `native-notation.md`; this file gives the working form of each and names the section for the full rule.

## 0. How to read this file

- A composition is one XML file. Its root element is `<composition>`. Everything it shows is a child element. There are no scripts and no selectors.
- Attributes carry the names and, mostly, the meanings that CSS gives them (`left`, `width`, `background`, `border-radius`, `flex-direction`, `font-size`, `color`). The values are simpler than CSS: see "Value forms" below.
- Write the file as well-formed XML: every attribute value in quotes, every element closed, and `&lt;` and `&amp;` for `<` and `&` in text. Comments and an XML declaration are fine.
- The reader collects every problem it finds and reports them all, each with its line number. A file with one problem does not render.

### Value forms

| Form | What it is |
|---|---|
| number | A plain decimal number with no unit: `12`, `-4`, `0.5`. `12px` and `50%` are not numbers. |
| length | A number of pixels (`40`), or, where the attribute says so, a percentage (`50%`) of the parent's size. |
| time | Seconds as a decimal: `0`, `0.5`, `2.25`. Up to nine digits after the point. No unit, no exponent, not negative. |
| colour | `#rrggbb` or `#rrggbbaa` (hexadecimal, with the `#`). Nothing else: no names (`red`), no `#rgb`, no `rgb()`. |
| shadow | `x y blur colour`, four values separated by spaces: `0 24 70 #00000099`. Several shadows are separated by commas. There is no spread and no `inset`. |
| line | `width colour`, a number and a colour separated by a space: `2 #ffffff`. |
| sides | One to four numbers, as CSS reads a shorthand: one for all; two for vertical and horizontal; three for top, horizontal, bottom; four for top, right, bottom, left. |

## 1. A complete small example

This file is valid. The files it names (`clips/intro.mp4`, `images/logo.png`, `audio/bed.mp3`) must exist in the project folder when it is rendered; the reader does not check for them.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<composition id="demo" width="1280" height="720" fps="30" duration="6" background="#101820">
  <style id="label" font-size="40" color="#ffffff"/>

  <audio id="bed" src="audio/bed.mp3" start="0" duration="6" volume="0.8">
    <volume at="0" value="0"/>
    <volume at="1" value="1"/>
  </audio>

  <video id="footage" src="clips/intro.mp4" left="0" top="0" width="1280" height="720"
         media-start="0" filter="brightness(0.6)"/>

  <box id="panel" left="140" top="200" width="600" height="300" border-radius="24"
       background="linear-gradient(90deg, #ff5a36 0%, #ffb347 100%)"
       box-shadow="0 12 30 #00000099" start="0.5" duration="5">
    <tween at="0.5" duration="0.6" ease="out-cubic" from-y="40" from-opacity="0" to-y="0" to-opacity="1"/>
  </box>

  <text id="title" class="label" left="180" top="240" width="520"
        font-weight="700" font-size="56" line-height="1.1" start="0.5" duration="5">
    Hello <span color="#101820" font-style="italic">frames</span> today
    <tween at="0.5" duration="0.6" ease="out-back(1.7)" from-scale="0.8" to-scale="1"/>
  </text>

  <image id="logo" src="images/logo.png" right="40" top="40" width="120" opacity="0">
    <set at="2" opacity="1"/>
  </image>
</composition>
```

What it does: the footage fills the frame, darkened; a rounded orange panel slides up and fades in at 0.5 s; the title (white, 56 px) grows from 80% over it, with one word in dark italic; the logo is invisible until 2 s, then appears; a sound fades in over the first second.

## 2. The composition and the project folder

### The project folder and paths

- A project is a folder. Every `src` in a composition (pictures, footage, sound, fonts, shapes, other compositions) is a path relative to the project folder, whatever folder the XML file is in. Use `/` in paths.
- The folder may hold `framesmith.yaml`, which names the composition to render and sets how it is rendered. It is optional. Without it the composition is the file `composition.xml` in the folder. See `project-file.md`.
- The program is run as `poc render <project folder | framesmith.yaml | composition.xml>`. When given a `.xml` file with no `framesmith.yaml` above it, the folder of that file is the project folder.
- Reading the file checks the XML and every attribute. The existence and content of `src` files is checked when the project is loaded for rendering, except for `<shape src>` and `<composition src>`, which are read while the file is read.

### The `<composition>` root element

| Attribute | Value | Default | Notes |
|---|---|---|---|
| `id` | text | none | A name. Not checked for uniqueness. |
| `width` | whole number, even, 2 to 8192 | required | Pixels. |
| `height` | whole number, even, 2 to 8192 | required | Pixels. |
| `fps` | whole number, 1 to 240 | `30` | Frames a second. |
| `duration` | time, above 0 | required | Length of the video, in seconds. Rounded up to whole frames. |
| `background` | one colour | `#000000` | The canvas colour. A single colour, not a gradient. |

- No other attribute is allowed on the root (other than `class`, section 8).
- Children of the root, in any order: `<style>`, `<font>`, `<audio>`, and the drawn elements `<box>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>`. `<font>`, `<audio>` and `<style>` may stand only directly in the root.
- Drawn elements are drawn in document order: a later element is over an earlier one. There is no z-index.
- The frame is `width` by `height` pixels. A drawn element is placed in it (section 4.1). Anything outside the frame is not seen.

## 3. The elements

Every drawn element accepts the common attributes of section 4, a class (section 8), and `<tween>` and `<set>` children (section 7). An attribute that an element does not take is reported: `<box> has no attribute ...`.

### 3.1 `<box>` (and `<rect>`)

A rectangle. `<rect>` is another name for it. It draws its `background`, `border` and `box-shadow` if written, and holds children. With no `background` it draws nothing but its children.

- Attributes: the common attributes only.
- Children: `<box>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>`, `<tween>`, `<set>`.
- Text written directly in a box is ignored.
- A box with no `width` and no `height` is as large as its parent (not the size of its children). A box with `left` and `right` (or `top` and `bottom`) and no size is as large as the space between them.
- A box becomes a row or a column of its children when it has `flex-direction` (section 4.3).

### 3.2 `<text>`

A block of text. Its words and its `<span>` children make one paragraph.

- Attributes: the common attributes. The text attributes of section 4.4 are the ones that matter.
- Children: words (character data), `<span>`, `<tween>`, `<set>`. Any other element is an error: `<box> is not known inside a text; only <span> is`.
- White space: a run of spaces, tabs and line ends between words is one space; white space at the start and end is dropped. A no-break space is kept. See `native-notation.md`, section 7.
- Size: with a `width` (or `left` and `right`), the text wraps at that width less the horizontal `padding`. With no width it is one line as wide as its text and does not wrap, unless it is an item of a flex column that is narrower than it.
- Height, with no `height`: the height of its lines plus the vertical `padding`.
- Lines end where the Unicode line-breaking rules allow (spaces, after hyphens, between ideographs). `white-space="nowrap"` stops wrapping.
- It also takes the look attributes: a `<text>` with a `background` and `padding` is a label with a plate.
- `count` makes the text a number that counts (`native-notation.md`, section 11): the text then has no words of its own, shows the number `count` written as `count-format` says (`0`, `0.00`, `0,0`, `0,0.00`, `m:ss`, `m:ss.0`, with up to six decimals; `0` by default), and a tween of `count` changes it. Its box is as wide as the widest number it shows, so what stands beside it keeps still. `<text count="0" count-format="0.00"><tween at="1" duration="1.4" to-count="99.98"/></text>`.
- `typed` shows only the first N characters of the text (section 7). It counts the text's own words, not the words inside spans. Spans are always drawn; while the text is being typed a span sits right after the characters shown so far.

### 3.3 `<span>`

A piece of a text. It stands inside a `<text>` (or inside another span) and nowhere else: outside a text, `<span>` is reported as `<span> is not a known element`.

- Attributes: the common attributes.
- Children: words, `<span>`, `<tween>`, `<set>`.
- It starts from the text properties of its parent and changes the ones it writes (colour, weight, size and so on).
- It sits among the words as a box, on the baseline. `left` and `top` do not place it; `margin`, `padding`, `background`, `border-radius` and the motion attributes work on it.
- A span can be moved, scaled and faded on its own with `<tween>`.

### 3.4 `<image>`

A still picture.

| Attribute | Value | Default | Notes |
|---|---|---|---|
| `src` | path | required | Relative to the project folder. A picture format the system reads; PNG and JPEG are the ones tried. |
| `object-fit` | `fill`, `cover`, `contain` | `fill` | `fill` stretches to the box. `cover` scales evenly until the box is covered; the rest is cut off. `contain` scales evenly until the whole picture fits; the rest of the box stays open (`native-notation.md`, section 15). |
| `object-position` | two percentages, across and down | `50% 50%` | Where a picture that is `cover` or `contain` is held in its box: `0% 0%` its top left on the box's top left, `100% 100%` its bottom right on the box's bottom right. No effect with `fill`. |
| `filter` | colour filter list | none | See section 4.5. |
| `mask` | one gradient | none | Fades the picture's transparency. Only the alpha of the colours counts: `linear-gradient(180deg, #000000ff 0%, #00000000 100%)` fades the picture out downwards. |

Plus the common attributes. No children except `<tween>` and `<set>`.

Size: with `width` and `height`, that size. With one of them, the other follows the picture's own proportions. With neither, the picture's own size in pixels.

### 3.5 `<video>`

Footage, decoded through frx7 and stretched to the box. The file is of the MP4 family and holds H.264 or HEVC pictures (`native-render-pipeline.md`, section 4).

| Attribute | Value | Default | Notes |
|---|---|---|---|
| `src` | path | required | Relative to the project folder. |
| `width`, `height` | whole numbers, 2 or more | required | Pixels. Not percentages, not fractions. They are also the size the footage is decoded at. |
| `media-start` | time | `0` | The time in the source file shown on the clip's first frame. |
| `filter` | colour filter list | none | See section 4.5. |
| `lut` | path of a `.cube` file | none | A colour table of three dimensions, relative to the project folder. Every colour of the video is replaced by the colour the table gives, before the steps of `filter`. Size 2 to 128 points a side, for colours from 0 to 1. `native-notation.md`, section 9. |

Plus the other common attributes. No children except `<tween>` and `<set>`. The sound of a video is not played; add an `<audio>` for sound.

With `start` and `duration` the video is a clip: it is shown only in that window, and plays from `media-start` at normal speed. Without them it is shown for the whole composition. The footage is converted to the composition's frame rate.

### 3.6 `<shape>`

Vector outlines, filled or stroked, from an SVG path written in the element or from an SVG file.

| Attribute | Value | Default | Notes |
|---|---|---|---|
| `d` | SVG path data | none | For example `M 4 4 L 20 4 L 20 20 L 4 20 Z`. |
| `src` | path | none | An SVG file in the project folder. Every `<path d="...">` in it is used. |
| `view-box` | four numbers `x y width height` | `0 0 24 24` | The coordinate space of the outlines. Width and height above 0. |
| `color` | colour | `#000000` | Fill colour, or stroke colour with `stroke-width`. Not inherited from a parent. |
| `stroke-width` | number | none (filled) | Draws the outline as a line of this width, in view-box units, instead of filling it. |
| `drop-shadow` | one shadow | none | `x y blur colour`, in view-box units. It follows the outline. |
| `dash` | pairs of numbers | none (a whole line) | With `stroke-width` only. A dash and then a gap, in view-box units, repeated along each outline from its start: `dash="12 8"`. |
| `draw` | 0 to 1 | `1` | With `stroke-width` only. Animated (section 7.1): the share of each outline that is drawn, from its start. |

- `d` and `src` can both be given: the outline from `d` comes first, then those of the file.
- With `src`, the file's own `viewBox` replaces `view-box` (and if the file has none, the default `0 0 24 24` is used). If the file's root has `fill="none"` and a `stroke-width`, and the element writes no `stroke-width`, the file's stroke width is used.
- The shape is scaled evenly to fit the element's box and centred in it. Give it `width` and `height`; with none it has no size and shows nothing.
- It needs `d` or a `src` with paths, or the reader says so.

Plus the common attributes. No children except `<tween>` and `<set>`.

### 3.7 `<composition src="...">` (a composition in another file)

Shows another composition file as a clip. Full rule: `native-notation.md`, section 2.

| Attribute | Value | Default | Notes |
|---|---|---|---|
| `src` | path | required | Relative to the project folder. The file's root must be `<composition>`. |
| `start` | time | `0` | Start of the clip on the parent's timeline. |
| `duration` | time | the whole length of that file | |

Plus the common attributes except `width` and `height`: the element has the size the file gives. The other file must have the same `fps`. What the file draws outside its frame is cut off. Its `background` is not drawn when it is shown inside another composition. Its fonts join the parent's; its sounds start with the clip and stop at its end. Text properties do not pass into it. A file may show other files, up to 16 deep; a file that shows itself is an error.

Children: only `<tween>` and `<set>`. Any other child is an error.

### 3.8 `<audio>`

A sound. Stands directly in the root.

| Attribute | Value | Default | Notes |
|---|---|---|---|
| `id` | text | none | |
| `src` | path | required | Relative to the project folder. A sound file the system reads (WAV, AAC, MP3 and others), with one, two or six channels. |
| `start` | time | `0` | When the sound begins, on the composition's timeline. |
| `duration` | time | required | How long it plays. |
| `media-start` | time | `0` | Where in the source file it begins. |
| `volume` | number | `1` | A multiplier of the sound's level. |

Children: `<volume at="..." value="..."/>`, any number.

- `at` is a time counted from the start of the sound (default `0` if omitted); `value` is a number and is required. The level is `volume` times the value of the points. Between two points it changes evenly; before the first point it holds the first value, after the last it holds the last value.
- The points must be in order of time.
- Sounds are added together. If the sum would go over -1 dB under full scale, the whole mix is lowered; see `native-notation.md`, section 4. The sound is as long as the picture.
- The other attributes of audio (`start`, `duration`) are not on the frame grid: a sound begins at the nearest sample.

### 3.9 `<font>`

Makes a font file available to `font-family`. Stands directly in the root.

| Attribute | Value | Default | Notes |
|---|---|---|---|
| `src` | path | required | A TrueType, OpenType or WOFF2 file in the project folder. |
| `family` | text | the name inside the file | The name `font-family` uses for it. |

No children. Without a `<font>`, `font-family` names a font installed on the machine. The same `src` and `family` written twice are used once.

### 3.10 `<style>`

A named set of attributes. See section 8.

### 3.11 `<tween>` and `<set>`

Motion. Children of the element they move. See section 7.

## 4. Attributes that several elements share

All of these are accepted by `<box>`, `<rect>`, `<text>`, `<span>`, `<image>`, `<video>`, `<shape>` and (except where noted) `<composition src>`.

### 4.1 Place and size

| Attribute | Value | Default | What it does |
|---|---|---|---|
| `id` | text | none | A name for the element. Not checked for uniqueness; not used to refer to it. |
| `left`, `top` | length (px or %) | none | Distance of the element's left or top edge from its parent's. `%` is of the parent's width (`left`) or height (`top`). |
| `right`, `bottom` | length (px or %) | none | Distance of the element's right or bottom edge from its parent's. Used only when `left` (or `top`) is not written. |
| `width`, `height` | length (px or %) | see below | `%` is of the parent's width or height. |
| `max-width` | number | none | The element is never wider than this many pixels when its width does not come from its own `width` (`native-notation.md`, section 13). |
| `position` | `static`, `absolute` | `static` | Matters only inside a flex box (4.3): `absolute` takes the element out of the row or column and places it by `left`, `top`, `right`, `bottom`. |

- With no `left`, `right`: the element is at the parent's left edge. With no `top`, `bottom`: at the parent's top edge.
- `left` with no `width`: the width is not reduced by `left`. Write `width`, or both `left` and `right`.
- `left` and `right` together, with no `width`: the width is the space between them. The same for `top`, `bottom` and `height`.
- Default size: a `<box>` is as large as its parent; a `<text>`/`<span>` fits its text; an `<image>` follows its picture; a `<shape>` has size 0 unless given; a `<video>` must be given one.
- The parent of a first-level element is the frame.
- Percentages are of the parent's size. In a flex box with no `width`/`height` of its own, percentages of its items count as 0.
- Margins and `left`/`top` do not move an item of a row or column (4.3).
- Moving, turning and scaling with `x`, `y`, `rotation`, `scale` (7.1) do not change the layout.

### 4.2 Timing

| Attribute | Value | Default | What it does |
|---|---|---|---|
| `start` | time | none | When the element first appears. |
| `duration` | time | none | How long it is shown. |

- Write both or neither. One without the other is reported: `<box> needs both `start` and `duration`, or neither`.
- With both, the element (and everything in it) is drawn only from `start` until `start + duration`. Without them it is drawn throughout its parent's life.
- `start` is counted from the start of the parent clip, or from the start of the composition if the parent is not a clip. Time inside an element with a `start` begins at that start: the `at` of its children's tweens and its children's own `start` count from there.
- The `<tween>`/`<set>` children of an element are in the same time as the element's own `start`: the time of its parent, not of the element itself. Example: an element with `start="0.8"` that should fade in as it appears has `<tween at="0.8" .../>`, not `at="0"`. The element's children, on the other hand, count from 0.8.
- Times are placed on frames by the frame-grid rule (`frame-time.md`, `native-notation.md` section 3): in short, a time is turned into the first whole frame at or after it, with a tolerance of a thousandth of a frame.
- `<video>` and `<audio>` also have `media-start`; `<audio>` has `start` and `duration` in its own sense (3.8).

### 4.3 Flex layout

A box lays out its children as a row or a column when it has `flex-direction`. Without it the children are placed by `left`, `top`, `right`, `bottom` (4.1).

| Attribute | Value | Default | Set on | What it does |
|---|---|---|---|---|
| `flex-direction` | `row`, `column` | none (no flex) | the container | Direction of the items. |
| `justify-content` | `flex-start`, `center`, `flex-end`, `space-between` | `flex-start` | the container | Where the items lie along the direction. Read only when `flex-direction` is written. `space-between`: the first item at the start, the last at the end, the room left over shared between neighbours (`native-notation.md`, section 13). |
| `align-items` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | `stretch` | the container | Where the items lie across. `stretch` makes an item as wide (column) or tall (row) as the container when that is known. |
| `gap` | number | `0` | the container | Space between items, in pixels. |
| `padding` | sides | `0` | a flex container, a text, a span | Space inside the edge. |
| `margin` | sides | `0` | an item of a flex container, a span | Space around the item. |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | an item | Overrides `align-items` for this item. `baseline`: in a row, the first lines of the items that have it stand on one line (`native-notation.md`, section 13). |
| `columns` | list of widths (`640 1fr`) | none | a `<box>` | The children are laid out in columns, row by row (`native-notation.md`, section 13). Not together with `flex-direction`. |
| `flex-grow` | number | `0` | an item | An item with a value above 0 takes a share of the spare space along the direction, in proportion to the values. Only works when the container has a width (row) or height (column). |
| `position` | `static`, `absolute` | `static` | an item | `absolute` leaves the flow (4.1). |

- Items are placed in document order. There is no wrapping and no shrinking.
- A container with no `width`/`height` is as large as its items, gaps and padding.
- Items of a row or column ignore their own `left`, `top`, `right`, `bottom` (unless `position="absolute"`).
- `padding` has no effect on the children of a plain box (one without `flex-direction`).
- The values `space-around`, `flex-wrap`, `flex-basis`, `flex-shrink`, `order` and `display` do not exist.

### 4.4 Text

These pass from an element to all of its children (the children may change them). At the root the values are the defaults in the table below. They do not pass into a `<composition src>`.

| Attribute | Value | Default | What it does |
|---|---|---|---|
| `font-family` | text | `sans-serif` | A family name: one installed on the machine, or the `family` of a `<font>`. |
| `font-weight` | number | `400` | `700` is bold. A number, not a word (`bold` is an error). |
| `font-style` | `normal`, `italic` | `normal` | |
| `font-size` | number | `16` | Pixels. |
| `line-height` | `normal`, a number, or a number with `px` | `normal` | A bare number is a multiple of the font size (`1.2`). `40px` is pixels. `40` alone means 40 times the size. `normal` is what the font asks for. |
| `letter-spacing` | number, or a number with `em` | `0` | Pixels, or ems of the font size where written (`0.05em`, `-0.01em`). |
| `color` | colour | `#000000` | The colour of the text. On a `<shape>` it is the shape's colour (not inherited). |
| `text-transform` | `none`, `uppercase` | `none` | |
| `text-align` | `left`, `center`, `right` | `left` | Of the lines inside the text's width. |
| `text-shadow` | shadow list, or `none` | none | Shadows of the glyphs. |
| `white-space` | `normal`, `nowrap` | `normal` | |
| `font-variant-numeric` | `normal`, or a list of `lining-nums`, `oldstyle-nums`, `tabular-nums`, `proportional-nums` | `normal` | The kind of figures (`native-notation.md`, section 14). |
| `text-stroke` | line (`width colour`) | none | An outline around each glyph. |

- Text outside a `<text>` or `<span>` is ignored, but the text attributes still pass through a box to a text inside it.
- Colour glyphs (emoji) keep their own colours. A character the named font lacks is drawn from another installed font.

### 4.5 Look

| Attribute | Value | Default | What it does |
|---|---|---|---|
| `background` | a list of layers, separated by commas | none | Each layer is a colour or a gradient. The first layer is on top. See below. |
| `background-size` | list: `auto`, or `width height` each in px or % | `auto` | Per layer, or one entry for all. `auto` is as large as the box. A sized layer is a tile. |
| `background-position` | list: `x y` each in px or % | `0 0` | Per layer, or one entry for all. In px: the tile's offset. In %: a share of the free room (`50%` centres). |
| `background-repeat` | list: `repeat`, `no-repeat` | `repeat` | Per layer, or one entry for all. |
| `border-radius` | number | `0` | Corner radius in pixels. Rounds the background, border, shadow, a picture and footage; with `overflow="hidden"`, the cut of the children. |
| `border` | line (`width colour`, or `width double colour`) | none | A line of that width round the box, inside its edge. It does not change the box's size. With `double`, two lines and the gap between them, each a third of the width (`native-notation.md`, section 18). |
| `box-shadow` | shadow list | none | Shadows of the box, following its radius. The first listed is on top. The blur value is as CSS counts it. |
| `overflow` | `visible`, `hidden` | `visible` | `hidden` cuts the children (and the element's own content) at the box. |
| `clip-path` | `polygon(x% y%, x% y%, ...)` | none | Cuts the element and its children to a polygon of three or more corners, each as percentages of the box. A box with `clip-path` shows no `box-shadow`. |
| `mix-blend-mode` | `normal`, `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity` | `normal` | How the element blends with what is under it (`native-notation.md`, section 16). |
| `opacity` | number, 0 to 1 | `1` | Fades the element with its children, as one picture. At 0 nothing is drawn. Animatable. |
| `blur` | number | `0` | Gaussian blur of the element with its children; the number is the standard deviation in pixels. Animatable. |
| `brightness` | number, 0 or more | `1` | Multiplies the colours of the finished element; stops at white. Animatable. |
| `displace` | an amount above 0 and a noise | none | A torn edge: every pixel of the finished element is taken from as far on, across and down, as the noise at it says: `14 noise(0.035 3 9)` moves an edge in and out by up to 7 pixels. The edge is hard. Not animatable. |
| `grain` | a noise, a slope and an offset | none | Uneven ink: the alpha of the finished element is multiplied by `slope × noise + offset`, held between 0 and 1: `noise(0.9 2 5) -1.7 1.75`. Not animatable. |
| `drop-shadow` | one shadow | none | `x y blur colour`: a shadow that follows the finished element and its children, whatever their outline, under them. On a `<shape>` it follows the outlines instead and is in view-box units. Not animatable. |
| `transform-origin` | two lengths `x y`, in px or % | `50% 50%` | The point `scale`, `scale-x`, `rotation`, `skew-y`, `rotate-x` and `rotate-y` turn about, measured in the element's box. No keywords. |
| `perspective` | number above 0 | none | The distance of the eye from the frame, in pixels, straight above `transform-origin`. Needed for `rotate-x` and `rotate-y` to have an effect. Smaller is a stronger perspective; 800 to 1500 is usual for a card. |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | How the element opens as `reveal` goes from 0 to 1: `wipe` from its left edge to its right and `iris` as a circle growing from its middle, both with a soft edge; `cut` from its left edge to its right with a hard edge, as a cut of the element's box (not together with `clip-path`, or with `overflow="hidden"` and rounded corners). A `.wgsl` file of the project holds a function `fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32>` of the author's own, which may call `picture(place)` and `open_at(progress, place)`; **only the GPU renderer draws it**, and the CPU renderer refuses the composition. `native-notation.md`, section 8. Has no effect on an element that is turned in space. |
| `filter` | list of colour steps | none | **`<image>` and `<video>` only.** A space-separated list of `brightness(n)`, `contrast(n)`, `saturate(n)`, `grayscale(n)`, each with a plain number: `brightness(0.6) contrast(1.14) saturate(0.8)`. Applied in order to the picture's colours. |

Blur and brightness: `native-notation.md`, section 5 (blur first, then brightness; both before the cut, opacity and blend). A torn edge, uneven ink and a shadow that follows the element: `native-notation.md`, section 17 (`noise(FREQUENCY OCTAVES SEED)` is the fractal noise of the W3C's Filter Effects recommendation, with 1 to 8 octaves and a whole seed; the three come after blur and brightness, in the order `displace`, `grain`, `drop-shadow`, before opacity and blend; an element with one of them is drawn by the CPU renderer in both renderers). Turning in space and opening: `native-notation.md`, section 8 (the element is drawn flat with its children first; that picture is turned or opened).

**Gradients and colours in `background` (and `mask`).**

- `linear-gradient(<angle>deg, <stop>, <stop>, ...)`. The angle is in degrees and written with `deg`; 0 points up, 90 right, 180 down. Without an angle the first item is a stop and the angle is 180.
- `radial-gradient(<shape>, <stop>, <stop>, ...)`. The shape is `ellipse`, `circle`, `ellipse <rx>% <ry>%` (radii as shares of the width and height), or `circle <radius>` (radius in pixels, above 0). Any shape may be followed by ` at <x>% <y>%` (percentages are required) to set the centre: `ellipse 62% 78% at 50% 42%`, `circle 12 at 20% 30%`. Without radii the shape reaches the farthest corner; without `at` the centre is the middle.
- A stop is a colour and a place, both required: `#ffffff 40%` (a share of the gradient's length) or `#ffffff 2` (pixels from its start). At least two stops. Two stops at one place make a hard edge. Colours are mixed with their alpha counted, so `#00000000` fades to nothing without darkening.
- Layers never show outside the box and follow its rounded corners. The tiling rules are in `native-notation.md`, section 1.
- A list of layer settings that has neither one entry nor one per layer is an error, as is a `background-size`, `background-position` or `background-repeat` on an element with no `background`.

**Drawing order inside one element:** box shadows, then the background layers (last listed first), then the picture, footage or shape, then the border, then text shadows, then the text, then the children in document order.

## 5. The animated properties as plain attributes

Each animated property (section 7.1) may also be written as an attribute on the element. It then sets the value the element has when no tween applies: `<image opacity="0">` starts invisible; `<box x="100">` is moved 100 pixels right.

## 6. Which attributes go on which element

| Element | Takes |
|---|---|
| `<composition>` (root) | `id`, `width`, `height`, `fps`, `duration`, `background` |
| `<box>`, `<rect>`, `<text>`, `<span>` | the common attributes (4.1 to 4.5 without `filter`) and the animated properties |
| `<image>` | common + `src`, `filter`, `mask`, `object-fit`, `object-position` |
| `<video>` | common + `src`, `media-start`, `filter`, `lut` |
| `<shape>` | common + `src`, `d`, `view-box`, `stroke-width`, `dash`, `draw` (its `drop-shadow` follows its outlines) |
| `<composition src>` | common without `width`, `height`, + `src` |
| `<audio>` | `id`, `src`, `start`, `duration`, `media-start`, `volume` |
| `<volume>` | `at`, `value` |
| `<font>` | `src`, `family` |
| `<style>` | `id`, and any other attributes |
| `<tween>` | `at`, `duration`, `ease`, `from-<property>`, `to-<property>` |
| `<set>` | `at`, `<property>` |

`class` is accepted on every element (section 8).

## 7. Motion: `<tween>` and `<set>`

Motion is written as child elements of the element it moves. A `<tween>` changes properties over a time. A `<set>` changes them at one instant.

### 7.1 The animated properties

These fifteen properties can be animated. The same names are used as attributes (section 5) and in `from-`/`to-` forms.

| Property | Unit | Starting value (when nothing sets it) | Meaning |
|---|---|---|---|
| `x` | pixels | `0` | Moves the element right (negative: left). |
| `y` | pixels | `0` | Moves the element down (negative: up). |
| `scale` | factor | `1` | Scales evenly about `transform-origin`. |
| `scale-x` | factor | `1` | Scales the width only, about `transform-origin`. Multiplied with `scale`. |
| `rotation` | degrees | `0` | Turns about `transform-origin`; positive turns clockwise on the screen. |
| `skew-y` | degrees | `0` | Slants vertically about `transform-origin`: points to the right of the origin move down for a positive angle. |
| `opacity` | 0 to 1 | `1` | Clamped to that range. At 0 nothing of the element, or of its children, is drawn. |
| `blur` | pixels | `0` | Standard deviation of a Gaussian blur. |
| `brightness` | factor | `1` | Multiplies colours; 0 or more. |
| `rotate-x` | degrees | `0` | Turns the element in space about a level line through `transform-origin`; positive takes the top edge away from the eye. Needs `perspective`. |
| `rotate-y` | degrees | `0` | Turns the element in space about an upright line through `transform-origin`; positive takes the right edge away from the eye. Needs `perspective`. When both are set, the turn about the level line comes first. |
| `reveal` | 0 to 1 | `1` | How far an element with a `transition` has opened: 0 shows nothing of it, 1 all of it. |
| `count` | number | none | On a `<text>` only: the number the text shows, written as its `count-format` says. `from-count="0" to-count="99.98"` counts up. See 3.2. |
| `draw` | 0 to 1 | `1` | On a `<shape>` with `stroke-width` only: the share of each outline that is drawn, from its start. `from-draw="0" to-draw="1"` draws the line. |
| `typed` | characters | all of them | How many characters of a `<text>` or `<span>`'s own words show. Fractions are rounded down. `from-typed="0" to-typed="30"` types the text out. |

`typed` has an effect only on a `<text>` that has a `<tween>` or `<set>` on `typed`; a `typed` attribute alone does nothing.

Not animatable: `left`, `top`, `right`, `bottom`, `width`, `height`, `background`, `color` and the other attributes. Move an element with `x` and `y`; there is no way to animate its size other than `scale` and `scale-x`.

The order of the transform, about `transform-origin`: scale, then skew, then rotate, then move by `x`, `y`. A turn in space (`rotate-x`, `rotate-y`) comes after all of these: it is applied to the finished picture of the element.

### 7.2 `<tween>`

```xml
<tween at="0.5" duration="0.6" ease="out-cubic" from-y="40" from-opacity="0" to-y="0" to-opacity="1"/>
```

| Attribute | Value | Default | |
|---|---|---|---|
| `at` | time | `0` | When it begins. |
| `duration` | time, above 0 | required | How long it takes. Missing or 0 is an error. |
| `ease` | an easing name | `linear` | See 7.4. |
| `from-<property>` | number | | The value at the start. |
| `to-<property>` | number | | The value at the end. |

`<property>` is one of the names of 7.1: `from-x`, `to-x`, `from-scale-x`, `to-opacity`, `to-typed` and so on. Any number of properties may be changed in one tween; they all share its `at`, `duration` and `ease`. A tween that changes no property is an error (`<tween> changes no property`).

- `from-` and `to-` both given: the value goes from one to the other.
- Only `to-`: it starts from the value the property has at `at` (the element's own value, or the value an earlier tween or set has left, or is still giving).
- Only `from-`: it ends at the value the property has at `at` in the same sense.
- A property has the value of the last tween or set that has started. After a tween ends the property keeps its end value for good. Before the first tween of a property, the property has the element's own value, except that if the first tween has `from-` the `from-` value holds from time 0.
- Tweens are placed in the time of the element's parent (see 4.2).
- A tween's value at a frame is its value at the exact time of that frame. A tween written a thousandth of a frame late counts as on the frame.
- Animation of `opacity` to 0 hides the element and everything in it.

### 7.3 `<set>`

```xml
<set at="2" opacity="1" x="300"/>
```

| Attribute | Value | Default | |
|---|---|---|---|
| `at` | time | `0` | When the value changes. |
| `<property>` | number | | The value from then on. |

The property names are written as they are, without `from-` or `to-`. `<set>` takes no `duration` and no `ease`. At least one property is needed. A `<set>` is a change that takes no time.

### 7.4 Easings

`ease` takes exactly one of these names, in lower case:

`linear`, `in-quad`, `out-quad`, `in-out-quad`, `in-cubic`, `out-cubic`, `in-out-cubic`, `in-quart`, `out-quart`, `in-out-quart`

`in-quint`, `out-quint`, `in-out-quint`, `in-sine`, `out-sine`, `in-out-sine`, `in-expo`, `out-expo`, `in-out-expo` (`native-notation.md`, section 10)

or `out-back(n)`, where `n` is a number: a curve that runs past its end value and settles back. `n` is how far it overshoots; `out-back(1.7)` is a gentle overshoot, `out-back(0)` has none. Spaces inside the brackets are accepted.

`in-` starts slowly, `out-` ends slowly, `in-out-` does both. The number is the power: quad (2), cubic (3), quart (4), quint (5); `sine` is a quarter of a wave and `expo` a power of two, the sharpest. No other name is known: `ease-in`, `ease-out`, `bounce`, `elastic`, `power2.out` and CSS cubic-béziers are errors.

### 7.5 When tweens overlap

- Tweens and sets on one property are put in order of `at`. Two with the same `at` keep their document order.
- At any moment the latest one in that order that has started controls the property. An earlier one that has not finished is cut off there; the two are not added or mixed.
- A later tween with no `from-` starts from the value the property has at that moment, so nothing jumps. One with a `from-` jumps to it.
- Tweens on different properties do not affect each other, whatever their order or timing.
- Example: `<set at="2" x="300"/>` and then `<tween at="2.5" duration="1" to-x="500"/>`: `x` is 300 from 2 s, and moves to 500 between 2.5 s and 3.5 s.
- Example: `<tween at="0" duration="2" to-x="100"/>` and `<tween at="1" duration="1" to-x="0"/>`: `x` rises for the first second to 50, then falls from 50 to 0 by 2 s. The first tween is cut at 1 s.
- Nothing is reported for overlap.

## 8. Shared styles

A `<style>` names a set of attributes. An element takes them with `class`. Full rule: `native-notation.md`, section 6.

```xml
<style id="label" font-size="27" color="#696b6d"/>
<style id="loud" color="#ffffff" font-weight="700"/>
<text class="label loud" left="262" top="76">TITLE</text>
```

- `<style>` stands directly in the `<composition>` and draws nothing. It needs an `id`. Its other attributes are any attributes, checked on the element that takes them.
- `class` is one or more style names separated by spaces. The attributes of those styles are treated as if written on the element.
- Which value wins: one written on the element itself; otherwise the style named later in `class`.
- `<tween>` and `<set>` cannot be shared.
- A style belongs to its file; a composition shown from another file has its own.
- There are no selectors.

## 9. Not sure

Things the code does not settle, or settles only by an outside program:

1. `font-family` is passed to the text engine as written. Whether a list such as `Inter, Arial, sans-serif` is read as a list of fallbacks is not settled by this code.
2. The other font a missing character is drawn from is one of the machine's own fonts; which one is not settled.
3. The picture and sound formats that work are those the system reads; the reader does not check them. Footage is looked at before a render starts, and a kind that is not read is refused there.
4. What happens when a video or sound is shorter than its clip window (the source ends first) is decided by the decoders, not by the reader.
5. A `<style>` could hold motion attributes (`x`, `opacity`) as plain attributes; this works by construction, but no example exists.
6. The reader does not check for negative or zero widths and heights, negative `gap`, or `font-size` of 0; their result is not described here.
7. `id` is recorded but not checked or used for anything else.
8. Time attributes accept a form such as `.5` (no leading digit); write `0.5`.
9. Numbers are read by the standard decimal reader of the language; forms such as `1e3` or `+5` may be accepted, but are not part of the notation.
10. Whether `margin` has an effect on an `position="absolute"` item or on an item of a plain (non-flex) box: from the layout code it has none, but this was not exercised.

## 10. The messages for the commonest mistakes

Each message is given with its line number as `line N: message` (and `file, line N: message` for a problem in a file shown by `<composition src>`). `poc render` prints them as `<file> is not a valid composition:` followed by one indented line for each.

| Mistake | Message |
|---|---|
| Not XML | `the file is not well-formed XML: ...` |
| Root is not `<composition>` | `the root element is <html>, not <composition>` |
| Unknown element | `<circle> is not a known element` |
| Unknown attribute | `<rect> has no attribute `colour`` |
| Unknown attribute on a tween | `<tween> has no attribute `to-width``; on a set: `<set> has no attribute `duration`` |
| Other element in a text | `<box> is not known inside a text; only <span> is` |
| `<span>` outside a text | `<span> is not a known element` |
| `<style>` inside an element | `a <style> stands directly in the <composition>, not inside an element` |
| Not a number | `` `x="a"` is not a number `` |
| Not a length | `` `width="ten"` is not a number or a percentage `` |
| Not a time | `` `at="-1"` is not a time in seconds `` |
| Not a colour | `` `color="red"` is not a colour written as #rrggbb or #rrggbbaa `` |
| Value not in the list | `` `text-align="justify"` is not one of: left, center, right `` (similar for `flex-direction`, `justify-content`, `align-items`, `align-self`, `position`, `overflow`, `font-style`, `text-transform`, `text-align`, `white-space`, `object-fit`) |
| Bad background | `` `background="red"` is not a colour or a list of gradients `` |
| Bad shadow | `` `box-shadow="..."` is not a list of shadows `` (and the same for `text-shadow`) |
| Bad border or text-stroke | `` `border="2px red"` is not a width and a colour `` |
| Bad padding or margin | `` `padding="a"` is not one to four numbers `` |
| Bad line height | `` `line-height="x"` is not `normal`, a number or pixels `` |
| Bad letter spacing | `` `letter-spacing="x"` is not a number or ems `` |
| Bad transform origin | `` `transform-origin="..."` is not two lengths `` |
| Bad clip path | `` `clip-path="..."` is not a polygon of three or more corners in percent `` |
| Bad filter | `` `filter="..."` is not a list of: brightness(), contrast(), saturate(), grayscale() `` |
| Bad mask | `` `mask="..."` is not one gradient `` |
| Bad torn edge | `` `displace="..."` is not an amount above 0 and a noise: `14 noise(0.035 3 9)` `` |
| Bad uneven ink | `` `grain="..."` is not a noise, a slope and an offset: `noise(0.9 2 5) -1.7 1.75` `` |
| Bad shadow of an element | `` `drop-shadow="..."` is not one shadow: x, y, blur and a colour `` |
| Bad perspective | `` `perspective` must be above 0, not 0 `` |
| Columns not widths, not on a box, or with a direction | `` `columns="wide 1fr"` is not a list of column widths: pixels, or shares such as `1fr` ``; `` `columns` is for a <box> ``; `` a <box> has `columns` or `flex-direction`, not both `` |
| Kind of figures not known | `` `font-variant-numeric="slashed-zero"` is not `normal` or a list of: lining-nums, oldstyle-nums, tabular-nums, proportional-nums `` |
| Picture place not two percentages | `` `object-position="top"` is not two percentages, across and down `` |
| Hard-edged opening joined with another cut | `` `transition="cut"` cannot be joined with `clip-path` `` or `` ... with `overflow="hidden"` and rounded corners `` |
| Shader opening not found | `` the transition `wave.wgsl` could not be read: ... `` (the path is the full one) |
| Shader opening the card does not take | `` the transition `wave.wgsl` is not a shader the card takes: ... `` followed by the compiler's words, with the line counted as the file counts it; said when the renderer starts |
| Shader opening with the CPU renderer | `` the transition `wave.wgsl` is a shader, which only the GPU renderer draws; render with `--backend gpu` `` |
| Bad colour table | `` the colour table `grade.cube`: it has 100 rows of colours; a table of size 33 has 35937 `` (the path is the full one; other reasons: the file could not be read, a table of one dimension, a range other than 0 to 1, a keyword that is not known, a row that is not three numbers) |
| Start without duration | `<box> needs both `start` and `duration`, or neither` |
| Missing file name | `<image> needs the attribute `src``; the same for `<video>`, `<audio>`, `<font>` |
| Video size | `` `width` of a <video> must be a whole number of 2 or more, not 10.5 `` |
| Audio without duration | `<audio> needs a `duration`` |
| Sound points out of order | `the <volume> points of an <audio> must be in order of time` |
| Anything but `<volume>` in an audio | `<tween> is not known inside <audio>` (with the name found) |
| Missing attribute | `<composition> needs the attribute `width`` |
| Bad frame size | `` `width` must be an even whole number from 2 to 8192, not 641 `` |
| Bad frame rate | `` `fps` must be a whole number from 1 to 240, not 0 `` |
| No length | `<composition> needs a `duration` above 0` |
| Tween without duration | `<tween> needs a `duration` above 0` |
| Tween or set without a property | `<tween> changes no property` (or `<set> ...`) |
| Unknown easing | `` `ease="bouncy"` is not a known easing `` |
| Background layers | `` `background-size` has 2 entries for 3 layers; it takes one for each layer, or one for all ``; `` `background-repeat` needs a `background` to place ``; `` `background-size="8"` is not a list of `auto` or a width and a height ``; `` `background-position="..."` is not a list of an x and a y ``; `` `background-repeat="round"` is not a list of `repeat` or `no-repeat` `` |
| A number that counts | `` `count` is for a <text>, not a <box> ``; `a <text> that counts holds no words of its own: its text is the number`; `` `count-format` needs `count` ``; `` a tween of `count` needs the attribute `count` on its <text> ``; `` `count-format="..."` is not a form of number; the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals `` |
| Shape | `` `draw` is for a <shape> with `stroke-width` `` (and the same for `dash`); `` `dash="..."` is not pairs of lengths: a dash and a gap, in view-box units ``; `<shape> needs `d` or a `src` with paths`; `` `...` is not a view box of four numbers ``; `` `drop-shadow="..."` is not one shadow: x, y, blur and a colour `` |
| Style without id | `<style> needs the attribute `id`: the name elements take it by` |
| Two styles, one id | `two <style> elements have the id `label`` |
| Unknown class | `` `class` names the style `big`, and this file has no <style> with that id `` |
| Style with class or children | `the style `x` has `class`; a style cannot take another style`; `the style `x` holds an element; a style holds attributes only` |
| Composition file | `a <composition> inside a composition needs the attribute `src``; `the composition `...` could not be read: ...`; `` `a.xml` shows itself, directly or through the files it shows ``; `` `a.xml` is more than 16 compositions deep ``; `<box> is not known in a <composition> that names a file; only <tween> and <set> are`; `` `a.xml` has 24 frames a second and the composition that shows it has 30; they must be the same `` |

The fault text for a failed `src` that is read later (fonts, pictures, footage, sound) is not given by the reader; it appears when the project is loaded, for example `line 4: the font `...`: ...`.
