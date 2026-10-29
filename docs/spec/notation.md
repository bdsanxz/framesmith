# The notation, version 1

Status: version 1 of the notation. This is the one document that describes it. Everything an author, an agent or the scene writer needs to write a composition is here: the elements, the attributes each element takes with their value forms and defaults, the timing rule, the motion rules, the messages for the common mistakes, the rule for each point that was recorded as open, and the table of what is known and not supported. The code of the reader, the layout, the text, the drawing and the motion engine is written from this document and from nothing else.

A composition is a folder of plain files: XML in this notation, media, fonts and an optional project file. There is no HTML, no style sheet, no script and no selector. Nothing is installed by the author.

## 1. How to read this document

- **Version.** The root element of every composition file carries `version="1"`. Section 2 says what happens to any other value.
- **Names.** The names of attributes and of values are CSS's wherever CSS has the same thing (`left`, `width`, `background`, `border-radius`, `flex-direction`, `font-size`, `color`). The values are simpler than CSS's and are listed in section 4. A name this document does not list is not taken (section 14 says how a known but deferred name differs from an unknown one).
- **Tables.** Section 6 has one table for every element. Each row is an attribute the element takes: its value form (section 4), its default, and the section that says what it does. An attribute an element's table does not list is not taken by that element.
- **Rules.** The fifteen points that were recorded as open are settled in section 12, one rule each. Elsewhere the document points to the rule and does not state it a second time.
- **Who decided.** A rule that the product owner decided cites its row in `DECISIONS.md`. A rule the maintainer drafted, following what the proofs' reader did where nothing on record settles the point (decision B4), says so; the owner may change it. Section 15 lists them.
- **Words.** "Must" and "is reported" mean: a composition that breaks the rule is not rendered, and `check` and `render` report the problem with its file and line. "Unsupported" means the form is on the table of section 14; anything else the reader does not accept is "invalid".

## 2. The version

- The root `<composition>` of every composition file has the attribute `version`. Its only value in this work is `1`. The starter, the fixtures and the benchmark compositions carry it.
- A root with no `version`, or with a value other than `1`, is reported with its line and the versions that are read. The message is in section 13.
- A composition shown from another file (section 10) is a composition file and carries `version` too. Its version must be the same as that of the file that shows it; since only `1` is read, the question does not arise yet.
- The version names the notation, not the program. A later notation gets a new number; a file written for version 1 is never read as another version.

## 3. A composition and its project folder

A small composition that is valid:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<composition version="1" id="demo" width="1280" height="720" fps="30" duration="6" background="#101820">
  <style id="label" font-size="40" color="#ffffff"/>
  <font src="fonts/Display.ttf" family="Display"/>

  <audio id="bed" src="audio/bed.wav" start="0" duration="6" volume="0.8">
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

  <text id="title" class="label" left="180" top="240" width="520" font-family="Display, sans-serif"
        font-weight="700" font-size="56" line-height="1.1" start="0.5" duration="5">
    Hello <span color="#101820" font-style="italic">frames</span> today
    <tween at="0.5" duration="0.6" ease="out-back(1.7)" from-scale="0.8" to-scale="1"/>
  </text>

  <image id="logo" src="images/logo.png" right="40" top="40" width="120" opacity="0">
    <set at="2" opacity="1"/>
  </image>
</composition>
```

The footage fills the frame, darkened; a rounded orange panel slides up and fades in at 0.5 s; the title (white, 56 pixels) grows from 80 % over it with one word in dark italic; the logo is invisible until 2 s and then appears; the sound fades in over the first second.

- **A composition is one XML file.** Write it as well-formed XML: every attribute value in quotes, every element closed, `&lt;` and `&amp;` for `<` and `&` in text. Comments and an XML declaration are fine.
- **A project is a folder.** It holds the composition file, the media, the fonts and, optionally, `framesmith.yaml`, whose keys have a spec of their own. Every `src` in a composition is a path relative to the project folder, wherever the XML file lies, written with `/`. Whether a `src` file exists and can be read is checked when the project is looked at, before the first frame, except for `<shape src>` and `<composition src>`, which are read with the composition.
- **The root** is `<composition>`. Its children, in any order, are `<style>`, `<font>`, `<audio>` and the drawn elements `<box>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>` and `<composition src>`. `<style>`, `<font>` and `<audio>` stand directly in the root and nowhere else.
- **Drawing order.** Drawn elements are drawn in document order: a later element is over an earlier one. There is no stacking order apart from it (section 14, S05). Within one element the order is: box shadows, the background layers (the last listed first), the picture, footage or shape, the border, the text shadows, the text, then the children in document order.
- **The frame** is `width` by `height` pixels. Whatever lies outside it is not seen.
- **The notation is closed.** The reader refuses every form this document does not list, for every element at once: an element that is not on the tables of section 6 (`<circle>`, `<html>`), an attribute that the table of its element does not list (`<rect colour="...">`, `<tween to-width="...">`, `<set duration="...">`), and an element in a place its parent does not hold (a `<box>` inside a `<text>`, a `<style>` inside a `<box>`, a `<span>` outside a text, a `<composition>` without `src` inside a composition). Each is reported with the line of the element or of the attribute, and the message names what is taken in its place (section 13). A number and a time are read by the forms of section 4 and the rules N8 and N9, whatever the attribute; what an attribute means, and the range of values it takes, are for the section that describes it. Nothing is skipped, repaired or taken to mean something near; an element that is reported is not read further, since what is inside it has no table to be judged by. A `class` or an `id` is a name and is judged by nothing but its form (N7, section 9).
- **Every problem at once.** The reader collects every problem it finds and reports all of them, each with its file and line; a composition with one problem does not render. A file that is not well-formed XML is reported once, with the reader's reason and line, since nothing else can be read from it. A problem in a file shown from another file names that file and its own line.

## 4. Value forms

| Form | What it is |
|---|---|
| text | Any characters. |
| number | A decimal number, with an optional sign and an optional exponent: `12`, `-4`, `+0.5`, `.5`, `1e3`, `2.5E-1`. No unit: `12px` and `50%` are not numbers. The value must be finite; `inf` and `nan` are not numbers, and neither is a value too large to be finite (`1e999`). See N9. |
| whole number | A number whose value has no fraction: `1280`, `1.28e3`. |
| length | A number of pixels (`40`), or a percentage of the parent's size (`50%`). Where an attribute says "length, 0 or more", a negative one is invalid (N6). |
| time | Seconds as decimal digits: `0`, `0.5`, `.5`, `2.25`. At most nine digits after the point and nine before. No sign, no exponent, no unit, never negative. See N8 and `frame-time.md`. |
| colour | `#rrggbb` or `#rrggbbaa`, hexadecimal, with the `#`. Nothing else: no names (`red`), no `#rgb`, no `rgb()`. |
| shadow | `x y blur colour`, four values separated by spaces: `0 24 70 #00000099`. Pixels, except on a `<shape>` (view-box units). There is no spread and no `inset`. A shadow list separates several shadows by commas; the first listed is on top. |
| line | `width colour`: a number of 0 or more and a colour, separated by a space: `2 #ffffff`. For `border`, also `width double colour` (section 7.5). |
| sides | One to four numbers, as CSS reads a shorthand: one for all four; two for vertical and horizontal; three for top, horizontal and bottom; four for top, right, bottom and left. |
| path | A file of the project, relative to the project folder, written with `/`. |
| percentage pair | Two numbers each followed by `%`, across and then down: `50% 0%`. |
| family list | One or more family names separated by commas (N1). |
| layer list, size list, position list, repeat list | The forms of section 7.4. |
| colour filter list | A space-separated list of `brightness(n)`, `contrast(n)`, `saturate(n)` and `grayscale(n)`, each with a plain number: `brightness(0.6) contrast(1.14)`. Applied in order. |
| easing name | One of the names of section 8.4. |
| a choice | One word out of the words listed in the table, in lower case. Anything else is invalid and the message names the words taken. |

Surrounding white space in a value is not part of it. An attribute written twice is not possible in XML and is reported as a malformed file.

## 5. Times and the frame grid

A time may be written with any number of decimals up to nine and is read exactly, as a fraction: `0.1` is one tenth, not the nearest binary number. All comparisons between times use exact arithmetic. A picture exists only on the frame grid. The frame count and the first and last frame of a clip follow one rule.

### 5.1 The frame-grid rule

> Multiply the time by the frame rate. If the result is within 0.001 of a whole number, take that whole number. Otherwise round up to the next whole number.

"Within 0.001" includes exactly 0.001. The rule is applied in exact arithmetic. At 30 frames a second a clip written to start at 105.0333 s starts on frame 3151, one written to start at 45.6667 s on frame 1370. A time written with four decimals lands on the frame its author meant, whichever way it was rounded.

- **The frame rate** is a whole number from 1 to 240; 30 when `fps` is not written. A frame rate written as a ratio is on the table of section 14 (S03).
- **Frame `i`** shows the composition at time `i / fps`, exactly. The first frame is frame 0.
- **The frame count** is the rule applied to the root's `duration`, at least 1. Six seconds at 24, 30 and 60 frames a second give 144, 180 and 360 frames; 6.01 seconds at 30 give 181. The video is never lengthened by a clip or a tween that runs past the end.
- `frame-time.md` states the same rule for the `timebase` crate, which does the arithmetic; this section states what the notation does with it.

### 5.2 A clip's window

`start` and `duration` are written both or neither, on each element that takes them (section 6). One without the other is reported as `FS-0040`, on the line of the element: `<box> needs both start and duration, or neither`.

- With both, the element and everything in it is drawn from its start frame up to, but not including, its end frame: frame `i` shows it when `F(start) <= i < F(end)`, with `F` the rule of 5.1, `start` and `end = start + duration` each counted on the timeline of the whole composition. The starts of the clips around it are added exactly as written and the rule is applied once to the sum. At 30 frames a second a clip from 1 s for 2 s shows on frames 30 to 89.
- Without them the element is drawn throughout its parent's life.
- `start` is counted from the start of the parent clip, or from the start of the composition when the parent is not a clip. Time inside an element that has a `start` begins at that start: the `start` of its children and the `at` of its children's tweens count from there.
- A start written relative to another clip is unsupported (S01).

### 5.3 Other times

- **Tweens and sets.** A tween's value at a frame is its value at that frame's exact time. A `<tween>` or `<set>` written within a thousandth of a frame after a frame counts as on that frame. The `at` of a `<tween>` or `<set>` is in the time of the parent of the element it moves (8.5).
- **`media-start`.** The source picture shown on a clip's first frame is the source's picture at `F(media-start)` on the source's own picture grid; each later frame shows the next source picture (decision A22). Which picture every other frame shows is the picture rule of the footage ticket: the latest source picture at or before the frame's time.
- **Sound** is not on the frame grid: a sound starts at the sample nearest its written time (section 11).

## 6. The elements and their attributes

One table for every element. The columns are the attribute, its value form (section 4), its default, and the section that says what it does. A form that has a word list names the words. An element takes the attributes of its table and no others; an attribute it does not take is reported (section 13). Where a default says "none", the attribute has no effect when it is not written. Where it says "required", the element is reported without it.

Every drawn element (`box`, `rect`, `text`, `span`, `image`, `video`, `shape`, `composition src`) takes the movement attributes `x`, `y`, `scale`, `scale-x`, `rotation`, `skew-y`, `opacity`, `blur`, `brightness`, `rotate-x`, `rotate-y` and `reveal` (section 8.1), as the starting values its own tweens and sets move away from.

**Where an element may stand.** Each element's section names its children, and an element is read only where its parent holds it. The same, in one table (words are not elements, and any number of each child is allowed):

| Parent | Holds |
|---|---|
| `<composition>` (the root) | `<style>`, `<font>`, `<audio>`, `<box>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>` |
| `<box>`, `<rect>` | `<box>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>`, `<tween>`, `<set>` |
| `<text>`, `<span>` | `<span>`, `<tween>`, `<set>` |
| `<image>`, `<video>`, `<shape>`, `<composition src>` | `<tween>`, `<set>` |
| `<audio>` | `<volume>` |
| `<style>`, `<font>`, `<volume>`, `<tween>`, `<set>` | nothing |

A `<composition>` without `src` is a root and stands in no other place; with `src` it stands where the table says `<composition src>`. A `<span>` stands only in a `<text>` or a `<span>`.

### 6.1 `<composition>` (the root)

The root of a file. Children: `<style>`, `<font>`, `<audio>`, `<box>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>`. No other attribute is taken. A `class` on the root names styles whose attributes the root takes.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `version` | whole number | required; the only value is `1` | 2 |
| `width` | even whole number, 2 to 8192 | required | 12 (N6) |
| `height` | even whole number, 2 to 8192 | required | 12 (N6) |
| `fps` | whole number, 1 to 240 | `30` | 5.1 |
| `duration` | time above 0 | required | 5.1 |
| `background` | colour | `#000000`, fully opaque | 7.4 |

### 6.2 `<box>`

A rectangle. It draws its `background`, `border` and `box-shadow` where written, and holds children. With no `background` it draws nothing but its children. Children: `<box>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>`, `<tween>`, `<set>`. Text written directly in a box is ignored; the text attributes still pass through the box to a text inside it. A box with `flex-direction` is a row or a column; a box with `columns` is a set of columns; a box with neither places its children by their own `left`, `top`, `right`, `bottom`.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | none; with `duration` or neither | 5.2 |
| `duration` | time | none; with `start` or neither | 5.2 |
| `left` | length | none | 7.1 |
| `top` | length | none | 7.1 |
| `right` | length | none | 7.1 |
| `bottom` | length | none | 7.1 |
| `width` | length, 0 or more | see 7.1 | 7.1 |
| `height` | length, 0 or more | see 7.1 | 7.1 |
| `max-width` | number above 0 | none | 7.2 |
| `position` | `static`, `absolute` | `static` | 7.1 |
| `margin` | sides | `0` | 7.2 |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | 7.2 |
| `flex-grow` | number, 0 or more | `0` | 7.2 |
| `flex-direction` | `row`, `column` | none (no row or column) | 7.2 |
| `justify-content` | `flex-start`, `center`, `flex-end`, `space-between` | `flex-start` | 7.2 |
| `align-items` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | `stretch` | 7.2 |
| `gap` | number, 0 or more | `0` | 7.2 |
| `padding` | sides, each 0 or more | `0` | 7.2 |
| `columns` | column widths | none | 7.2 |
| `font-family` | family list | `sans-serif` | 7.3 |
| `font-weight` | number | `400` | 7.3 |
| `font-style` | `normal`, `italic` | `normal` | 7.3 |
| `font-size` | number above 0 | `16` | 7.3 |
| `line-height` | `normal`, a number, or a number with `px` | `normal` | 7.3 |
| `letter-spacing` | number, or a number with `em` | `0` | 7.3 |
| `color` | colour | `#000000` | 7.3 |
| `text-transform` | `none`, `uppercase` | `none` | 7.3 |
| `text-align` | `left`, `center`, `right` | `left` | 7.3 |
| `text-shadow` | shadow list, or `none` | none | 7.3 |
| `white-space` | `normal`, `nowrap` | `normal` | 7.3 |
| `font-variant-numeric` | `normal`, or a list of figure kinds | `normal` | 7.3 |
| `text-stroke` | line | none | 7.3 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow | none | 7.6 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.3 `<rect>`

Another name for `<box>`: same attributes, same children, same drawing.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | none; with `duration` or neither | 5.2 |
| `duration` | time | none; with `start` or neither | 5.2 |
| `left` | length | none | 7.1 |
| `top` | length | none | 7.1 |
| `right` | length | none | 7.1 |
| `bottom` | length | none | 7.1 |
| `width` | length, 0 or more | see 7.1 | 7.1 |
| `height` | length, 0 or more | see 7.1 | 7.1 |
| `max-width` | number above 0 | none | 7.2 |
| `position` | `static`, `absolute` | `static` | 7.1 |
| `margin` | sides | `0` | 7.2 |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | 7.2 |
| `flex-grow` | number, 0 or more | `0` | 7.2 |
| `flex-direction` | `row`, `column` | none (no row or column) | 7.2 |
| `justify-content` | `flex-start`, `center`, `flex-end`, `space-between` | `flex-start` | 7.2 |
| `align-items` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | `stretch` | 7.2 |
| `gap` | number, 0 or more | `0` | 7.2 |
| `padding` | sides, each 0 or more | `0` | 7.2 |
| `columns` | column widths | none | 7.2 |
| `font-family` | family list | `sans-serif` | 7.3 |
| `font-weight` | number | `400` | 7.3 |
| `font-style` | `normal`, `italic` | `normal` | 7.3 |
| `font-size` | number above 0 | `16` | 7.3 |
| `line-height` | `normal`, a number, or a number with `px` | `normal` | 7.3 |
| `letter-spacing` | number, or a number with `em` | `0` | 7.3 |
| `color` | colour | `#000000` | 7.3 |
| `text-transform` | `none`, `uppercase` | `none` | 7.3 |
| `text-align` | `left`, `center`, `right` | `left` | 7.3 |
| `text-shadow` | shadow list, or `none` | none | 7.3 |
| `white-space` | `normal`, `nowrap` | `normal` | 7.3 |
| `font-variant-numeric` | `normal`, or a list of figure kinds | `normal` | 7.3 |
| `text-stroke` | line | none | 7.3 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow | none | 7.6 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.4 `<text>`

A block of text. Its words and its `<span>` children make one paragraph. Children: words, `<span>`, `<tween>`, `<set>`; any other element is reported. A `<text>` that has `count` holds no words and no span.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | none; with `duration` or neither | 5.2 |
| `duration` | time | none; with `start` or neither | 5.2 |
| `left` | length | none | 7.1 |
| `top` | length | none | 7.1 |
| `right` | length | none | 7.1 |
| `bottom` | length | none | 7.1 |
| `width` | length, 0 or more | see 7.1 | 7.1 |
| `height` | length, 0 or more | see 7.1 | 7.1 |
| `max-width` | number above 0 | none | 7.2 |
| `position` | `static`, `absolute` | `static` | 7.1 |
| `margin` | sides | `0` | 7.2 |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | 7.2 |
| `flex-grow` | number, 0 or more | `0` | 7.2 |
| `padding` | sides, each 0 or more | `0` | 7.2 |
| `font-family` | family list | `sans-serif` | 7.3 |
| `font-weight` | number | `400` | 7.3 |
| `font-style` | `normal`, `italic` | `normal` | 7.3 |
| `font-size` | number above 0 | `16` | 7.3 |
| `line-height` | `normal`, a number, or a number with `px` | `normal` | 7.3 |
| `letter-spacing` | number, or a number with `em` | `0` | 7.3 |
| `color` | colour | `#000000` | 7.3 |
| `text-transform` | `none`, `uppercase` | `none` | 7.3 |
| `text-align` | `left`, `center`, `right` | `left` | 7.3 |
| `text-shadow` | shadow list, or `none` | none | 7.3 |
| `white-space` | `normal`, `nowrap` | `normal` | 7.3 |
| `font-variant-numeric` | `normal`, or a list of figure kinds | `normal` | 7.3 |
| `text-stroke` | line | none | 7.3 |
| `typed` | number, 0 or more | all characters | 7.12 |
| `count` | number | none | 7.12 |
| `count-format` | number format | `0` | 7.12 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow | none | 7.6 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.5 `<span>`

A piece of a text. It stands inside a `<text>` or another `<span>` and nowhere else; outside a text it is reported as not known. Children: words, `<span>`, `<tween>`, `<set>`. It starts from the text properties of its parent and changes the ones it writes. It sits among the words as a box on the baseline; `left` and `top` do not place it, so it does not take the place attributes.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | none; with `duration` or neither | 5.2 |
| `duration` | time | none; with `start` or neither | 5.2 |
| `margin` | sides | `0` | 7.2 |
| `padding` | sides, each 0 or more | `0` | 7.2 |
| `font-family` | family list | `sans-serif` | 7.3 |
| `font-weight` | number | `400` | 7.3 |
| `font-style` | `normal`, `italic` | `normal` | 7.3 |
| `font-size` | number above 0 | `16` | 7.3 |
| `line-height` | `normal`, a number, or a number with `px` | `normal` | 7.3 |
| `letter-spacing` | number, or a number with `em` | `0` | 7.3 |
| `color` | colour | `#000000` | 7.3 |
| `text-transform` | `none`, `uppercase` | `none` | 7.3 |
| `text-align` | `left`, `center`, `right` | `left` | 7.3 |
| `text-shadow` | shadow list, or `none` | none | 7.3 |
| `white-space` | `normal`, `nowrap` | `normal` | 7.3 |
| `font-variant-numeric` | `normal`, or a list of figure kinds | `normal` | 7.3 |
| `text-stroke` | line | none | 7.3 |
| `typed` | number, 0 or more | all characters | 7.12 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow | none | 7.6 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.6 `<image>`

A still picture. Children: `<tween>`, `<set>`.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | none; with `duration` or neither | 5.2 |
| `duration` | time | none; with `start` or neither | 5.2 |
| `left` | length | none | 7.1 |
| `top` | length | none | 7.1 |
| `right` | length | none | 7.1 |
| `bottom` | length | none | 7.1 |
| `width` | length, 0 or more | see 7.1 | 7.1 |
| `height` | length, 0 or more | see 7.1 | 7.1 |
| `max-width` | number above 0 | none | 7.2 |
| `position` | `static`, `absolute` | `static` | 7.1 |
| `margin` | sides | `0` | 7.2 |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | 7.2 |
| `flex-grow` | number, 0 or more | `0` | 7.2 |
| `src` | path | required | 12 (N3) |
| `object-fit` | `fill`, `cover`, `contain` | `fill` | 7.10 |
| `object-position` | two percentages | `50% 50%` | 7.10 |
| `filter` | colour filter list | none | 7.9 |
| `mask` | one gradient | none | 7.9 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow | none | 7.6 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.7 `<video>`

Footage, decoded through frx7, stretched to the box. The sound of a video is not played (section 14, S08); write an `<audio>` for sound. Children: `<tween>`, `<set>`.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | none; with `duration` or neither | 5.2 |
| `duration` | time | none; with `start` or neither | 5.2 |
| `left` | length | none | 7.1 |
| `top` | length | none | 7.1 |
| `right` | length | none | 7.1 |
| `bottom` | length | none | 7.1 |
| `width` | whole number, 2 or more | required | 7.1 |
| `height` | whole number, 2 or more | required | 7.1 |
| `max-width` | number above 0 | none | 7.2 |
| `position` | `static`, `absolute` | `static` | 7.1 |
| `margin` | sides | `0` | 7.2 |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | 7.2 |
| `flex-grow` | number, 0 or more | `0` | 7.2 |
| `src` | path | required | 12 (N3) |
| `media-start` | time | `0` | 10 |
| `filter` | colour filter list | none | 7.9 |
| `lut` | path of a `.cube` file | none | 7.9 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow | none | 7.6 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.8 `<shape>`

Vector outlines, filled or stroked, from path data written in the element or from a file. Children: `<tween>`, `<set>`.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | none; with `duration` or neither | 5.2 |
| `duration` | time | none; with `start` or neither | 5.2 |
| `left` | length | none | 7.1 |
| `top` | length | none | 7.1 |
| `right` | length | none | 7.1 |
| `bottom` | length | none | 7.1 |
| `width` | length, 0 or more | see 7.1 | 7.1 |
| `height` | length, 0 or more | see 7.1 | 7.1 |
| `max-width` | number above 0 | none | 7.2 |
| `position` | `static`, `absolute` | `static` | 7.1 |
| `margin` | sides | `0` | 7.2 |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | 7.2 |
| `flex-grow` | number, 0 or more | `0` | 7.2 |
| `src` | path | none | 7.11 |
| `d` | path data | none | 7.11 |
| `view-box` | four numbers | `0 0 24 24` | 7.11 |
| `stroke-width` | number above 0 | none (filled) | 7.11 |
| `color` | colour | `#000000` | 7.11 |
| `dash` | pairs of lengths | none (a whole line) | 7.11 |
| `draw` | number, 0 to 1 | `1` | 7.11 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow, in view-box units | none | 7.11 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.9 `<composition src>` (a composition shown from another file)

Shows another composition file as a clip (section 10). It is the element `<composition>` with the attribute `src`; a `<composition>` without `src` is a root and a `<composition>` inside a composition without `src` is reported. It has the size its file gives, so it takes no `width` and no `height`. Children: `<tween>`, `<set>`; any other child is reported.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `start` | time | `0` | 10 |
| `duration` | time | the whole length of the file | 10 |
| `left` | length | none | 7.1 |
| `top` | length | none | 7.1 |
| `right` | length | none | 7.1 |
| `bottom` | length | none | 7.1 |
| `max-width` | number above 0 | none | 7.2 |
| `position` | `static`, `absolute` | `static` | 7.1 |
| `margin` | sides | `0` | 7.2 |
| `align-self` | `flex-start`, `center`, `flex-end`, `stretch`, `baseline` | the container's `align-items` | 7.2 |
| `flex-grow` | number, 0 or more | `0` | 7.2 |
| `src` | path | required | 10 |
| `background` | layer list | none | 7.4 |
| `background-size` | size list | `auto` | 7.4 |
| `background-position` | position list | `0 0` | 7.4 |
| `background-repeat` | repeat list | `repeat` | 7.4 |
| `border-radius` | number, 0 or more | `0` | 7.5 |
| `border` | line, or width `double` colour | none | 7.5 |
| `box-shadow` | shadow list | none | 7.5 |
| `overflow` | `visible`, `hidden` | `visible` | 7.5 |
| `clip-path` | polygon | none | 7.5 |
| `mix-blend-mode` | blend name | `normal` | 7.7 |
| `displace` | amount and noise | none | 7.6 |
| `grain` | noise, slope and offset | none | 7.6 |
| `drop-shadow` | one shadow | none | 7.6 |
| `transform-origin` | two lengths | `50% 50%` | 7.8 |
| `perspective` | number above 0 | none | 7.8 |
| `transition` | `none`, `wipe`, `cut`, `iris`, or the path of a `.wgsl` file | `none` | 7.8 |
| `x` | number | `0` | 8.1 |
| `y` | number | `0` | 8.1 |
| `scale` | number | `1` | 8.1 |
| `scale-x` | number | `1` | 8.1 |
| `rotation` | number | `0` | 8.1 |
| `skew-y` | number | `0` | 8.1 |
| `opacity` | number, 0 to 1 | `1` | 7.7 |
| `blur` | number, 0 or more | `0` | 7.6 |
| `brightness` | number, 0 or more | `1` | 7.6 |
| `rotate-x` | number | `0` | 7.8 |
| `rotate-y` | number | `0` | 7.8 |
| `reveal` | number, 0 to 1 | `1` | 7.8 |

### 6.10 `<audio>`

A sound. Stands directly in the root. Children: `<volume>`, any number.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | none | 12 (N7) |
| `class` | style names, separated by spaces | none | 9 |
| `src` | path | required | 11 |
| `start` | time | `0` | 11 |
| `duration` | time above 0 | required | 11 |
| `media-start` | time | `0` | 11 |
| `volume` | number, 0 or more | `1` | 11 |

### 6.11 `<volume>`

A point of the level curve of its `<audio>`. No children.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `at` | time, counted from the start of the sound | `0` | 11 |
| `value` | number, 0 or more | required | 11 |

### 6.12 `<font>`

Makes a font file available to `font-family`. Stands directly in the root. No children. The same `src` and `family` written twice are used once.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `class` | style names, separated by spaces | none | 9 |
| `src` | path | required | 7.3 |
| `family` | text | the name inside the file | 7.3 |

### 6.13 `<style>`

A named set of attributes taken by `class` (section 9). Stands directly in the root; draws nothing; has no children.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `id` | text | required | 9 |
| `(any attribute of the element that takes the style)` | any value that attribute takes | none | 9 |

### 6.14 `<tween>`

A change of values over a time, child of the element it moves (section 8.2). `<property>` stands for each of the fifteen names of section 8.1; the table lists them all.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `at` | time | `0` | 8.2 |
| `duration` | time above 0 | required | 8.2 |
| `ease` | easing name | `linear` | 8.4 |
| `from-x` | number | none (see 8.2) | 8.1 |
| `from-y` | number | none (see 8.2) | 8.1 |
| `from-scale` | number | none (see 8.2) | 8.1 |
| `from-scale-x` | number | none (see 8.2) | 8.1 |
| `from-rotation` | number | none (see 8.2) | 8.1 |
| `from-skew-y` | number | none (see 8.2) | 8.1 |
| `from-opacity` | number | none (see 8.2) | 8.1 |
| `from-blur` | number | none (see 8.2) | 8.1 |
| `from-brightness` | number | none (see 8.2) | 8.1 |
| `from-rotate-x` | number | none (see 8.2) | 8.1 |
| `from-rotate-y` | number | none (see 8.2) | 8.1 |
| `from-reveal` | number | none (see 8.2) | 8.1 |
| `from-count` | number | none (see 8.2) | 8.1 |
| `from-draw` | number | none (see 8.2) | 8.1 |
| `from-typed` | number | none (see 8.2) | 8.1 |
| `to-x` | number | none (see 8.2) | 8.1 |
| `to-y` | number | none (see 8.2) | 8.1 |
| `to-scale` | number | none (see 8.2) | 8.1 |
| `to-scale-x` | number | none (see 8.2) | 8.1 |
| `to-rotation` | number | none (see 8.2) | 8.1 |
| `to-skew-y` | number | none (see 8.2) | 8.1 |
| `to-opacity` | number | none (see 8.2) | 8.1 |
| `to-blur` | number | none (see 8.2) | 8.1 |
| `to-brightness` | number | none (see 8.2) | 8.1 |
| `to-rotate-x` | number | none (see 8.2) | 8.1 |
| `to-rotate-y` | number | none (see 8.2) | 8.1 |
| `to-reveal` | number | none (see 8.2) | 8.1 |
| `to-count` | number | none (see 8.2) | 8.1 |
| `to-draw` | number | none (see 8.2) | 8.1 |
| `to-typed` | number | none (see 8.2) | 8.1 |

### 6.15 `<set>`

A change of values at one instant, child of the element it moves (section 8.3). It takes no `duration` and no `ease`.

| Attribute | Value form | Default | See |
|---|---|---|---|
| `at` | time | `0` | 8.3 |
| `x` | number | none (at least one property) | 8.1 |
| `y` | number | none (at least one property) | 8.1 |
| `scale` | number | none (at least one property) | 8.1 |
| `scale-x` | number | none (at least one property) | 8.1 |
| `rotation` | number | none (at least one property) | 8.1 |
| `skew-y` | number | none (at least one property) | 8.1 |
| `opacity` | number | none (at least one property) | 8.1 |
| `blur` | number | none (at least one property) | 8.1 |
| `brightness` | number | none (at least one property) | 8.1 |
| `rotate-x` | number | none (at least one property) | 8.1 |
| `rotate-y` | number | none (at least one property) | 8.1 |
| `reveal` | number | none (at least one property) | 8.1 |
| `count` | number | none | 8.1 |
| `draw` | number | none (at least one property) | 8.1 |
| `typed` | number | none (at least one property) | 8.1 |

## 7. What the shared attributes do

### 7.1 Place and size

- **Edges.** `left` and `top` are the distance of the element's left or top edge from its parent's; `right` and `bottom` the distance of its right or bottom edge from its parent's. A percentage is of the parent's width (`left`, `right`, `width`) or height (`top`, `bottom`, `height`). The parent of a first-level element is the frame. An edge distance may be negative; the element then lies partly outside.
- **Which edge counts.** Q1 settles how `bottom` and `right` place a box that has a size and how they combine with `top` and `left`; Q2 settles where an absolutely placed child is counted from.
- **Size.** `width` and `height` are lengths of 0 or more. With `left` and `right` and no `width`, the width is the space between them; the same for `top`, `bottom` and `height`. A `left` with no `width` does not reduce the width.
- **Size when none is written.** A `<box>` is as large as its parent (not as large as its children). A `<text>` and a `<span>` fit their text. An `<image>` follows its picture: with one of the two sizes the other follows the picture's proportions, with neither the picture's own size in pixels. A `<shape>` has size 0 and shows nothing. A `<video>` must be given both sizes. A row or column with no size is as large as its items, gaps and padding.
- **Zero and negative.** N6.
- **`max-width`** is a number above 0: the element is never wider than that many pixels when its width does not come from its own `width`. It holds for an item that its row, column or cell would make wider and for a text with no `width`, which then wraps at it. An element with a `width` of its own ignores it.
- **`position`** is `static` or `absolute`. It matters in a row, column or set of columns: `absolute` takes the element out of the flow and places it by its edges (Q2). Elsewhere it changes nothing.
- **A plain box has no flow.** The children of a box with no `flex-direction` and no `columns` are absolutely placed, whether or not they write `position`, and CSS's rules for an absolutely placed box hold for them in full (Q2, N10). This is the one way a plain box differs from CSS, where such a box lays its children out one after another in a normal flow: here each child is placed by its own edges alone, and none takes room from another. A normal flow in a plain box is not in this version (B41).
- **Movement does not move the layout.** `x`, `y`, `rotation` and `scale` move the picture, not the places of the other elements.
- A percentage of an item in a flex box that has no size of its own counts as 0.

### 7.2 Rows, columns and spacing

A box with `flex-direction` lays its children out as a row or a column; a box with `columns` as a set of columns; without either its children are placed by their edges. `flex-direction` and `columns` together are reported.

- **Items** stand in document order. There is no wrapping and no shrinking. An item ignores its own `left`, `top`, `right` and `bottom` unless it is `position="absolute"`.
- **`justify-content`** (default `flex-start`) places the items along the direction; read only when `flex-direction` is written. `space-between` puts the first item at the start, the last at the end and shares the room left over equally between each two neighbours, on top of `gap`; with one item or no room it is the same as `flex-start`.
- **`align-items`** (default `stretch`) places the items across; `stretch` makes an item as wide (column) or as tall (row) as the container when that is known; a row or column with no size across has the size its items give it, and `stretch` fills that. `align-self` overrides it for one item. `baseline`, in a row, sets the items that have it so that the baselines of their first lines lie on one line and the highest touches the top of the row. The baseline of a `<text>` or `<span>` is that of its first line; of a box that is a row, column or set of columns, that of its first item, counted from its top; of anything else, its bottom edge. A row with no height is as high as its items need. In a column and in a cell of a set of columns, `baseline` is the same as `flex-start`.
- **`gap`** (default `0`) is the space between items, and in a set of columns between columns and between rows.
- **`padding`** is the space inside the edge of a flex container, a set of columns, a `<text>` or a `<span>`. On a plain box (no `flex-direction`, no `columns`) it has no effect. A written `width` or `height` of a row or column includes its padding, as CSS's `border-box` does (B17, `DECISIONS.md`): `width="80" padding="8"` is 80 wide, and its items have the 64 that remain.
- **`margin`** is the space around an item of a flex container or of a set of columns, and around a `<span>`. On an absolutely placed item (a `<box>`, a `<text>` or a `<composition src>` that is `position="absolute"`, or that stands in a plain box) it moves the item in from each edge it is placed by, and on an axis with no edge from its static position (N10). On an element that is none of these (neither an item of a flex container or of a set of columns, nor a span, nor absolutely placed) it has no effect (N10).
- **`flex-grow`** (default `0`): an item with a value above 0 takes a share of the spare room along the direction, in proportion to the values. Only when the container has a width (row) or a height (column).
- **`columns`** is a list of column widths separated by spaces, each a number of pixels (`640`) or a share of what is left (`1fr`, `2fr`): the room the pixel columns and the gaps leave is divided among the `fr` columns in proportion to their numbers. The children fill the first row from left to right, then the next. An item is as wide as its column less its margins, unless it has a `width` of its own; then it lies at the left of its column. A row is as high as its tallest item; when the box has a height and the rows need less, the rest is shared equally among the rows. In its row an item stands as `align-items` or its `align-self` says. A box with no width of its own: an `fr` column is as wide as its widest item. Every width must be above 0.
- **Not in the notation:** `space-around`, `space-evenly`, `flex-wrap`, `flex-basis`, `flex-shrink`, `order`, `display`, `grid` (section 14).

### 7.3 Text

These attributes pass from an element to everything inside it; a child may change them. At the root the defaults of the table apply. They do not pass into a `<composition src>`.

- **`font-family`** is a family list (N1). **`font-weight`** is a number, `700` for bold; a word such as `bold` is reported. **`font-style`**: `normal` or `italic`. **`font-size`** is in pixels, above 0 (N6). **`line-height`**: `normal` (what the font asks for), a number (a multiple of the font size: `1.2`), or a number with `px` (pixels: `40px`). **`letter-spacing`**: a number (pixels) or a number with `em` (`0.05em`, `-0.01em`). **`color`**: the text colour; fully opaque `#000000` when none is named (Q5). **`text-transform`**: `none` or `uppercase`. **`text-align`**: the lines inside the text's width. **`white-space`**: `normal`, or `nowrap`, with which no line ends before the text does.
- **`text-shadow`**: a shadow list or `none`; shadows of the glyphs. **`text-stroke`**: a line (`width colour`), an outline round each glyph.
- **`font-variant-numeric`**: `normal` (the font's own figures) or a list, with spaces between, of `lining-nums` (all as high as a capital), `oldstyle-nums` (of the height of small letters, some reaching above and below), `tabular-nums` (all of one width) and `proportional-nums` (each of its own width). A font without the kind asked for shows its own figures.
- **White space.** A run of spaces, tabs and line ends between words is one space; white space at the start and end of the text is dropped. A no-break space is kept and no line ends at it.
- **Lines.** The words of a `<text>` and its spans are one paragraph. A line may end where the Unicode line-breaking rules allow (UAX 14): at a space, after a hyphen, between two ideographs or kana, and, in scripts written without spaces such as Thai, between words as a dictionary finds them. A word wider than its line is not broken. A line's width does not count the space it ends at. With a `width` (or `left` and `right`) the text wraps at that width less the horizontal `padding`; without one it is a single line as wide as its text, unless it is an item of a column narrower than it. With no `height` the height is that of the lines plus the vertical `padding`.
- **Direction.** The paragraph runs from left to right. Within a line a run of a right-to-left script is ordered by the Unicode bidirectional algorithm (UAX 9), so Arabic and Hebrew read correctly beside Latin words and numbers. A paragraph that runs right to left as a whole is on the table of section 14 (S21).
- **Shaping.** Letters are joined and placed as their script needs, by the font's own tables.
- **Colour glyphs** (emoji) keep their own colours; `color` does not change them.
- **A character the named fonts lack:** N2. **A font file:** the `<font>` element makes a TrueType, OpenType or WOFF2 file of the project available under its `family` (or the name inside the file).
- A `<text>` with a `background` and `padding` is a label with a plate.

### 7.4 Backgrounds

`background` is a list of layers separated by commas; the first layer is on top. A layer is a colour or a gradient.

- **Linear gradient** `linear-gradient(<angle>deg, <stop>, <stop>, ...)`: the angle is written with `deg`, 0 points up, 90 right, 180 down; without it the first item is a stop and the angle is 180.
- **Radial gradient** `radial-gradient(<shape>, <stop>, <stop>, ...)`: the shape is `ellipse`, `circle`, `ellipse <rx>% <ry>%` (radii as shares of the layer's width and height) or `circle <radius>` (pixels, above 0), and may be followed by ` at <x>% <y>%` (both percentages) to set the centre. Without radii the shape reaches the farthest corner; without `at` the centre is the middle.
- **A stop** is a colour and a place, both required: `#ffffff 40%` (a share of the gradient's length) or `#ffffff 2` (pixels from its start). At least two stops. Two stops at one place make a hard edge. Between two stops the colour changes evenly with the colours weighted by their alpha, so `#00000000` fades to nothing without darkening. A conic gradient is on the table of section 14 (S17).
- **`background-size`**: a list of `auto` (as large as the box) or `width height`, each in pixels or percent of the box. **`background-position`**: a list of `x y`; in pixels the offset of the layer's top left corner from the box's, in percent that share of the room left over (`50%` centres, `100%` against the far edge). **`background-repeat`**: a list of `repeat` or `no-repeat`. Each list has one entry for every layer, in the order of `background`, or one entry for all.
- A layer with a size is one tile; with `repeat` it is laid side by side in both directions from its place until the box is covered, with `no-repeat` it is drawn once. A gradient is measured against its tile. Layers never show outside the box and follow its rounded corners.
- A list with neither one entry nor one for each layer is reported, and so is `background-size`, `background-position` or `background-repeat` on an element with no `background`.
- On the root, `background` is a single colour, the canvas.

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

### 7.5 Corners, borders, shadows and cuts

- **`border-radius`**: the corner radius in pixels. It rounds the background, the border, the shadow, a picture and footage and, with `overflow="hidden"`, the cut of the children.
- **`border`**: a line (`width colour`) round the box, inside its edge. It does not change the box's size. `width double colour` draws two lines with a gap between them in the room one line of that width takes: the outer line, the gap and the inner line are each a third of the width. The word `double` stands between the width and the colour and no other word is taken there.
- **`box-shadow`**: a shadow list, following the box's radius; the first listed is on top. The blur is counted as CSS counts it: a Gaussian blur whose standard deviation is half the blur, drawn as `drop-shadow` is drawn in 7.6 (three passes of an average over the span given there). A box with `clip-path` shows no `box-shadow`.
- **`overflow`**: `hidden` cuts the element's children and its own content at the box, inside its border, following the inner rounded corners.

How a box draws its border, corners and cut, pixel by pixel, and the reports of a value in another form, are in `box-edges.md`. A border by side is S20 and a corner written as an ellipse is S33 (section 14); each is reported as unsupported on the line of the attribute.
- **`clip-path`**: `polygon(x% y%, x% y%, ...)`, three or more corners, each in percent of the box; it cuts the element and its children to the polygon.

### 7.6 Blur, brightness, torn edges, uneven ink and shadows that follow

`blur`, `brightness`, `displace`, `grain` and `drop-shadow` work on the **finished picture** of an element: its own background, border, text or picture and everything inside it, drawn together first, with the element's own transforms and cut. They come in this order: `blur`, `brightness`, `displace`, `grain`, `drop-shadow`; then the cut (`overflow`, `clip-path`), the element's `opacity`, and its `mix-blend-mode`.

- **`blur`** (animated): a Gaussian blur with that standard deviation in the element's pixels. 0 leaves the picture.
- **`brightness`** (animated): a number, 0 or more; every colour is multiplied by it and stops at white; 1 leaves the picture, 0 makes it black. The transparency does not change. Layers that are partly clear are joined before the brightness counts: a quarter of white over `#202020` is `#585858`, and with `brightness="2"` the element shows `#b0b0b0`.
- **Noise.** `noise(FREQUENCY OCTAVES SEED)` names a field of numbers from 0 to 1 over the element's box: the alpha channel of the fractal noise of the W3C's Filter Effects recommendation, with one base frequency in both directions (above 0), that many octaves (a whole number, 1 to 8) and a whole seed. A seed that is not above 0 stands, by the recommendation, for the seed one above its size (`-7` is `8`, `0` is `1`). The noise is read at the point of the element's box (in the element's own pixels from its top left corner) half a pixel on in both directions; the element's transforms move the noise with it. The number is kept in 256 steps: `round(255 n) / 255`.
- **`displace="AMOUNT noise(...)"`** (not animatable): every pixel is taken from another place of the picture: the pixel whose noise is `n` shows what lay `AMOUNT * (n - 0.5)` of the element's pixels further on, the same distance across and down. `14 noise(0.035 3 9)` moves an edge in and out by up to 7 pixels. The distance is turned and scaled as the element is, then rounded to whole frame pixels across and down each on its own; no pixel is mixed with its neighbour, so the edge is hard. What is taken from outside the picture is clear. `AMOUNT` is above 0.
- **`grain="noise(...) SLOPE OFFSET"`** (not animatable): the alpha of every pixel is multiplied by `SLOPE * n + OFFSET`, held between 0 and 1. `noise(0.9 2 5) -1.7 1.75` leaves most of a stamp's ink and takes it away where the noise is high.
- **`drop-shadow="X Y BLUR COLOUR"`** (not animatable): the picture's alpha, blurred with a standard deviation of half of `BLUR`, moved by `X` and `Y`, in `COLOUR`, lies under the picture, whatever its outline. `X`, `Y` and `BLUR` are in the element's pixels and are turned and scaled as the element is. The blur is three passes of an average over a span of `floor(sigma * 3 * sqrt(2 * pi) / 4 + 0.5)` pixels, `sigma` being the standard deviation in frame pixels. On a `<shape>` it follows the outlines and is in view-box units (7.11).
- An element with one of `displace`, `grain` or `drop-shadow` is drawn by the processor's renderer in both renderers, every frame, at the size of the part that shows; the card's renderer puts that picture on its frame.

### 7.7 Opacity and blending

- **`opacity`** (animated): 0 to 1, clamped to that range; it fades the element with its children as one picture. At 0 nothing of the element or its children is drawn. It comes after `drop-shadow`, so the shadow of a half-clear element does not show through it.
- **`mix-blend-mode`** says how the finished picture of the element is mixed with what is under it; `normal` by default. The values and their formulas follow the W3C's Compositing and Blending recommendation. For each colour channel of the result, with `s` the element's colour and `b` the colour under it (both 0 to 1):

| Value | Result |
|---|---|
| `normal` | the element's colour |
| `multiply` | `s * b` |
| `screen` | `s + b - s * b` |
| `overlay` | `hard-light` with the two colours changed about |
| `darken` | the lesser of `s` and `b` |
| `lighten` | the greater of `s` and `b` |
| `color-dodge` | `b / (1 - s)`, at most 1; 0 where `b` is 0 and 1 where `s` is 1 |
| `color-burn` | `1 - (1 - b) / s`, at least 0; 1 where `b` is 1 and 0 where `s` is 0 |
| `hard-light` | `multiply` of `b` and `2s` where `s` is at most a half, `screen` of `b` and `2s - 1` above it |
| `soft-light` | a gentler `hard-light`, by the recommendation's formula |
| `difference` | the distance between `s` and `b` |
| `exclusion` | `s + b - 2 * s * b` |

Four work on the colour as a whole, where the lightness of a colour is `0.3 R + 0.59 G + 0.11 B` and its saturation the distance between its greatest and its least channel:

| Value | The result has |
|---|---|
| `hue` | the hue of the element, the saturation and lightness of what is under it |
| `saturation` | the saturation of the element, the hue and lightness of what is under it |
| `color` | the hue and saturation of the element, the lightness of what is under it |
| `luminosity` | the lightness of the element, the hue and saturation of what is under it |

Where the element is partly clear the result is mixed with what is under it by the element's alpha. Both renderers draw every value.

### 7.8 Turning, opening and the point they act about

- **`transform-origin`**: two lengths `x y`, in pixels or percent of the element's box; `50% 50%` by default. No keywords. The point `scale`, `scale-x`, `rotation`, `skew-y`, `rotate-x` and `rotate-y` act about.
- **Order of the transform**, about the origin: scale, then skew, then rotate, then move by `x`, `y`. A turn in space comes after all of these and acts on the element's finished picture.
- **`perspective`**: the distance of the eye from the frame, in pixels, straight above `transform-origin`; above 0 (0 or less is reported). Without it the element is not turned in space. Smaller is a stronger perspective; 800 to 1500 is usual for a card.
- **`rotate-x`** (animated, degrees, `0`): turns the element about a horizontal line through the origin; a positive angle takes the top edge away from the eye. **`rotate-y`** (degrees, `0`): about an upright line; a positive angle takes the right edge away from the eye. With both, the turn about the horizontal line comes first. A point the turn brings nearer to the eye is drawn larger and further from the origin, one further away smaller: its distance from the origin is multiplied by `perspective / (perspective - z)`, `z` being how far the turn brought it towards the eye. The element is drawn flat first (children, own transforms, cut, blur, opacity); that picture is then turned, with pixels taken from between the pixels of the flat one. An element turned edge-on, or one whose turn brings a part of it to the eye or behind it, is not drawn. Turned past 90 degrees an element is drawn mirrored, as CSS draws it by default (decision A23).
- **`transition`** says how an element opens as `reveal` (animated, 0 to 1, default 1) goes from 0 to 1: `none` (not written); `wipe`, from the element's left edge to its right with a soft edge a tenth of its width wide; `cut`, from the left edge to the right with a hard edge (at `reveal` r the element shows from its left edge to r of its width and nothing beyond; an ordinary cut of the element's box, the same in both renderers); `iris`, a circle that grows from the middle with a soft edge a tenth of its half diagonal wide. `reveal` has an effect only on an element with a `transition`. What is not yet open shows whatever lies under the element, so a scene laid over another and opened is a transition from the one to the other. `cut` together with `clip-path`, or with `overflow="hidden"` and rounded corners, is reported (an element has one cut). An element that is turned in space does not open: its `transition` waits.
- **An opening written as a shader.** `transition` may name a file of the project that ends in `.wgsl`, holding one function in the WebGPU shading language, and any functions of its own it needs:

  `fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32>`

  `place` is where the pixel lies in the element's box, 0 to 1 across and down (below 0 or above 1 outside the box); `size` is the box's width and height in pixels; `progress` is the element's `reveal`. The result is the pixel's colour and alpha, the colour already multiplied by the alpha; each number is brought into 0 to 1. The function may call `picture(place: vec2<f32>) -> vec4<f32>` (the colour of the element's flat picture at a place of its box, taken from between the four pixels around it, transparent outside the picture) and `open_at(progress: f32, place: f32) -> f32` (the share shown by an opening with a soft edge a tenth wide). It is asked for every pixel of the flat picture that lies in the frame while `reveal` is below 1; at 1 the element is drawn without it, so it should give `picture(place)` there. `wipe` gives `picture(place) * open_at(progress, place.x)`. An element with such an opening cannot itself be blurred or brightened while it opens; what is inside it can.
- **Only the card draws a shader opening**, which the product owner accepted on 2026-10-13: a composition that has one needs a machine with a graphics card and has no second renderer to be checked against. `check` says so for each such opening with the line of the first element that names it; the processor's renderer refuses the composition before the first frame; a file that is not found, or that the card's compiler does not take, stops the render before the first frame with the file and the compiler's words.

### 7.9 Colour filters, masks and colour tables

- **`filter`** (on `<image>` and `<video>`): a space-separated list of `brightness(n)`, `contrast(n)`, `saturate(n)` and `grayscale(n)`, each with a plain number, applied in order to the picture's colours.
- **`mask`** (on `<image>`): one gradient; it fades the picture's transparency. Only the alpha of the colours counts: `linear-gradient(180deg, #000000ff 0%, #00000000 100%)` fades the picture out downwards. A mask by a picture is on the table of section 14 (S18).
- **`lut`** (on `<video>`): the path of a `.cube` file of the project, a colour table of three dimensions. Every colour of the video, as shown (8 bits a channel, after the conversion from the source's own colour form), is replaced by the colour the table gives, before the steps of `filter`; the result is a whole level before `filter` sees it. The table has `N` points along each of red, green and blue, `N` from 2 to 128 given by `LUT_3D_SIZE`; its rows are three numbers each, red first, red changing quickest, then green, then blue. A colour between points takes its place between the eight points around it: along red first, then green, then blue; a result outside 0 to 1 is brought to the nearer end. Empty lines, lines that begin with `#` and `TITLE` lines are passed over. Reported when the composition is read, with the line of the `<video>`, the file and the line of the file where there is one: a file not found; a table of one dimension (`LUT_1D_SIZE`); a range other than 0 to 1 (`DOMAIN_MIN`, `DOMAIN_MAX`); a keyword that is not known; a row that is not three numbers; a number of rows other than `N * N * N`. Both renderers give the same colours to within one level.

### 7.10 Fitting a picture

For an `<image>`. **`object-fit`**: `fill` (default) stretches the picture to the box whatever its shape; `cover` scales it evenly until it covers the box and cuts off what reaches beyond; `contain` scales it evenly until all of it is in the box and the rest of the box shows what is under it. **`object-position`**: two percentages, across and then down, `50% 50%` by default: `0%` puts the picture's left (or top) edge on the box's, `100%` its right (or bottom) edge on the box's, and a value between divides what reaches beyond the box (`cover`) or the room left over (`contain`) in that proportion; `50% 0%` holds a portrait at its top. With `fill` it does nothing. Neither is taken by `<video>` (S10).

### 7.11 Shapes

- **`d`**: SVG path data (`M 4 4 L 20 4 L 20 20 L 4 20 Z`). **`src`**: an SVG file of the project; every `<path d="...">` in it is used. `d` and `src` can both be given: the outlines of `d` come first, then those of the file. A `<shape>` needs `d`, or a `src` with paths.
- **`view-box`**: four numbers `x y width height`, the coordinate space of the outlines; width and height above 0; `0 0 24 24` by default. With `src`, the file's own `viewBox` replaces it (if the file has none, the default is used). If the file's root has `fill="none"` and a `stroke-width` and the element writes no `stroke-width`, the file's stroke width is used.
- The shape is scaled evenly to fit the element's box and centred in it. Give it `width` and `height`; with none it has no size and shows nothing.
- **`color`**: the fill colour, or the stroke colour with `stroke-width`; `#000000` by default; not inherited from a parent. **`stroke-width`**: a number above 0 in view-box units; the outline is drawn as a line of that width instead of being filled. The ends of every line are round. No drawn curve lies further than 0.25 pixel from the true one (a value stated under decision A29).
- **`dash`** (with `stroke-width` only): pairs of lengths, a dash and then a gap, in view-box units, repeated along each outline from its start: `dash="12 8"`. An even count of numbers, none below zero, not all zero.
- **`draw`** (animated, with `stroke-width` only): the share of each outline that is drawn, counted from the outline's start along its length. At 1 or more the whole outline is drawn; below one thousandth nothing is drawn, not even the round end of the line, so that a line that starts to draw itself is not there as a dot in the frame before. Between, each outline is drawn from its start to that share of its own length; a `d` with several outlines (several `M`) has each drawn to the same share. An outline starts where its `d` starts and runs the way it runs; a closed outline is drawn round to its start. With both `dash` and `draw`, the dashes lie where they would on the whole outline and `draw` shows them up to its share.
- **`drop-shadow`** on a `<shape>` follows the outlines and is in view-box units.
- `draw` or `dash` on an element that is not a `<shape>` with `stroke-width`, as an attribute or in a `<tween>` or `<set>`, is reported.

### 7.12 A number that counts, and typed text

**`count`.** A `<text>` with the attribute `count` shows a number and no words. `count` is the number it starts with and an animated property: a `<tween>` or a `<set>` changes it (`from-count`, `to-count`, `count`). At each frame the text shows the number of that frame's time, written as `count-format` says (default `0`):

| `count-format` | 1234.567 reads | What it is |
|---|---|---|
| `0` | `1235` | A whole number. |
| `0.0`, `0.00`, ... up to six decimals | `1234.6`, `1234.57` | That many decimals, always shown. |
| `0,0`, `0,0.00`, ... | `1,235`, `1,234.57` | The same, with a comma between every three digits of the whole part. |
| `m:ss`, `m:ss.0`, ... up to six decimals | `20:35`, `20:34.6` | The number is a count of seconds, shown as minutes and seconds. The seconds always have two digits. |

- The number is rounded to its last shown place first, a half away from zero, and written after that: 59.96 seconds read `1:00.0`, not `0:60.0`. A number below zero begins with `-`; one that rounds to zero has no sign.
- The text is one line and never broken. Its box does not change while it counts: it is as wide as the widest number the text shows in any frame of the composition, so what stands beside it keeps still; within the box the number stands as `text-align` says. Everything else of a text holds: font, colour, shadow, its own tweens.
- Reported: `count` or `count-format` on an element that is not a `<text>`; a `<text>` that counts and has words or a `<span>`; `count-format` without `count`; a `<tween>` or `<set>` of `count` on an element without the attribute `count`; a form not in the table.

**`typed`.** On a `<text>` or `<span>`, `typed` is the number of characters of its own words that show; a fraction is rounded down; all of them by default. It counts the element's own words, not the words inside spans. Spans are always drawn; while the text is typed, a span takes its place in the layout, as its characters do (B83). `from-typed="0" to-typed="30"` types the text out. The attribute `typed` is the number of characters shown before the element's first tween or set of `typed` starts; alone, with neither, it is the number of characters it gives, and a text with no `typed` shows all of it (N5, B58).

A typed text is laid out with all its characters, and only those shown are drawn, so no shown character moves whatever the alignment, and a span keeps its place in the layout too (B59, B83).

## 8. Motion: `<tween>` and `<set>`

Motion is written as child elements of the element it moves. A `<tween>` changes properties over a time; a `<set>` changes them at one instant. Animation is put at a time and never played: the value of a property at a frame depends on the composition and the frame's number only (`motion.md` describes the engine; where this section lists more properties or easings than that file does, this document governs the notation and the tickets that build them extend the engine's file). Authors write no code; there is no expression, no selector and no timeline object.

### 8.1 The fifteen animated properties

The same names are used as attributes (the starting value, section 6), in `from-` and `to-` forms of a `<tween>`, and in a `<set>`. A property that is not on the table cannot be animated: `left`, `top`, `right`, `bottom`, `width`, `height`, `background`, `color` and every other attribute are not. Move an element with `x` and `y`; there is no way to animate its size except `scale` and `scale-x` (animated size and colour are on the table of section 14, S24).

| Property | Unit | Starting value | Meaning |
|---|---|---|---|
| `x` | pixels | `0` | Moves the element right (negative: left). |
| `y` | pixels | `0` | Moves the element down (negative: up). |
| `scale` | factor | `1` | Scales evenly about `transform-origin`. |
| `scale-x` | factor | `1` | Scales the width only, about `transform-origin`; multiplied with `scale`. |
| `rotation` | degrees | `0` | Turns about `transform-origin`; positive is clockwise on the screen. |
| `skew-y` | degrees | `0` | Slants vertically about `transform-origin`: points to the right of the origin move down for a positive angle. |
| `opacity` | 0 to 1 | `1` | Clamped to that range. At 0 nothing of the element or its children is drawn. |
| `blur` | pixels | `0` | Standard deviation of a Gaussian blur (7.6). |
| `brightness` | factor, 0 or more | `1` | Multiplies colours (7.6). |
| `rotate-x` | degrees | `0` | Turns the element in space about a level line; needs `perspective` (7.8). |
| `rotate-y` | degrees | `0` | Turns the element in space about an upright line; needs `perspective` (7.8). |
| `reveal` | 0 to 1 | `1` | How far an element with a `transition` has opened (7.8). |
| `count` | number | none | On a `<text>` only: the number the text shows (7.12). |
| `draw` | 0 to 1 | `1` | On a `<shape>` with `stroke-width` only: the share of each outline drawn (7.11). |
| `typed` | characters | all of them | On a `<text>` or `<span>`: how many of its own characters show (7.12). |

Fractions of `typed` are rounded down. `opacity` and the properties marked 0 to 1 are clamped to their range; a number outside it is not reported.

### 8.2 `<tween>`

```xml
<tween at="0.5" duration="0.6" ease="out-cubic" from-y="40" from-opacity="0" to-y="0" to-opacity="1"/>
```

The attributes are in the table of 6.14. `<property>` is one of the names of 8.1: `from-x`, `to-x`, `from-scale-x`, `to-opacity`, `to-typed` and so on. Any number of properties may change in one tween; they share its `at`, `duration` and `ease`.

- A `<tween>` needs a `duration` above 0 and at least one `from-` or `to-` attribute.
- `from-` and `to-` both given: the value goes from one to the other.
- Only `to-`: it starts from the value the property has at `at` (the element's own value, or the value an earlier tween or set has left or is still giving).
- Only `from-`: it ends at the value the property has at `at`, in the same sense.
- During the tween the value is `start + (end - start) * ease(progress)`, with `progress` the share of the duration that has passed, 0 to 1. After a tween ends the property keeps its end value for good.
- Before the first tween or set of a property, the property has the element's own value, except that if the first tween has a `from-` value, that value holds from time 0.
- A tween may run past the end of the composition. The video is not lengthened; the last frame shows the value for its own time.

### 8.3 `<set>`

```xml
<set at="2" opacity="1" x="300"/>
```

The attributes are in the table of 6.15. A `<set>` changes properties at one instant: the value holds from the frame at `at` onward. The property names are written as they are, without `from-` or `to-`. At least one property is needed.

- The frame at `at` is the first frame whose time is `at` or later, by the frame grid rule (5.3): a set written within a thousandth of a frame after a frame's time is on that frame; one written later than that is on the next frame.
- A set is a tween of no length in the order of 8.5: a later tween with only `to-` starts from the set's value, and a set and a tween with the same `at` keep their document order, the later one controlling the property.
- A set with no property is reported on its line (`FS-0061`). A property its element does not take as an attribute (8.1: `count` on a `<text>` only, `typed` on a `<text>` or `<span>`, `draw` on a `<shape>`) is reported on the attribute (`FS-0062`). An attribute named as an attribute of the element tables that is not an animated property, such as `width`, is unsupported (`FS-0035`, S24); a name that no element takes, such as `z-index`, is one the set does not take (`FS-0030`). Either counts as the set's property for `FS-0061`, so that the one mistake has one report.

### 8.4 Easings

`ease` takes exactly one of these names, in lower case (default `linear`):

`linear`; `in-quad`, `out-quad`, `in-out-quad`; `in-cubic`, `out-cubic`, `in-out-cubic`; `in-quart`, `out-quart`, `in-out-quart`; `in-quint`, `out-quint`, `in-out-quint`; `in-sine`, `out-sine`, `in-out-sine`; `in-expo`, `out-expo`, `in-out-expo`

or `out-back(n)`, `n` a finite number, spaces inside the brackets accepted. `p` is how far the tween has come, from 0 to 1; the result is how far its value has come.

| Name | Result |
|---|---|
| `linear` | `p` |
| `in-quad`, `in-cubic`, `in-quart` | `p^n` for n = 2, 3, 4 |
| `out-quad`, `out-cubic`, `out-quart` | `1 - (1 - p)^n` |
| `in-out-quad`, `in-out-cubic`, `in-out-quart` | `2^(n-1) p^n` below one half, `1 - (2 - 2p)^n / 2` from there |
| `in-quint` | `p^5` |
| `out-quint` | `1 - (1 - p)^5` |
| `in-out-quint` | `16 p^5` below one half, `1 - (2 - 2p)^5 / 2` from there |
| `in-sine` | `1 - cos(p * pi / 2)` |
| `out-sine` | `sin(p * pi / 2)` |
| `in-out-sine` | `(1 - cos(p * pi)) / 2` |
| `in-expo` | 0 at 0, else `2^(10p - 10)` |
| `out-expo` | 1 at 1, else `1 - 2^(-10p)` |
| `in-out-expo` | 0 at 0, 1 at 1; `2^(20p - 10) / 2` below one half, `(2 - 2^(10 - 20p)) / 2` from there |
| `out-back(n)` | 0 at 0; otherwise with `q = p - 1`, `q^2 ((n + 1) q + n) + 1`: a curve that runs past 1 by an amount set by `n` and settles back to 1; `out-back(1.7)` is a gentle overshoot and `out-back(0)` is the curve of `out-cubic` |

`in-` starts slowly, `out-` ends slowly, `in-out-` does both. Every curve gives exactly 0 at `p = 0` and exactly 1 at `p = 1`. A curve as computed agrees with its formula to within 1e-12 (a value stated under decision A29). No other name is taken: `ease-in`, `ease-out`, `bounce`, `elastic`, `power2.out` and cubic-bezier curves are not (more easings are on the table of section 14, S16; other forms are invalid).

An `ease` that is none of these, in a `<tween>` that stands where it may, is reported on the line of the attribute (`FS-0063`, invalid), with the names taken: ``` `ease="bouncy"` is not a known easing; the easings taken are linear, in-quad, out-quad, in-out-quad, in-cubic, out-cubic, in-out-cubic, in-quart, out-quart, in-out-quart, in-quint, out-quint, in-out-quint, in-sine, out-sine, in-out-sine, in-expo, out-expo, in-out-expo and out-back(n), with n a number ```. The `n` of `out-back(n)` is read as any number of the notation (N9); `out-back()` or `out-back(x)` is not a known easing. The engine's curves are those of `motion.md`.

### 8.5 Whose time, and when tweens overlap

- **Time.** The `at` of a `<tween>` or `<set>` is in the time of the parent of the element it moves, the same time as the element's own `start`; it is not the time inside the element. An element with `start="0.8"` that should fade in as it appears has `<tween at="0.8" .../>`, not `at="0"`. The element's children, on the other hand, count their `start` and the `at` of their tweens from 0.8 (5.2). In a span the same holds (Q4).
- **Order.** Tweens and sets on one property are put in order of `at`; two with the same `at` keep their document order and the later one wins.
- **Which controls.** At any moment the latest one in that order that has started controls the property. An earlier one that has not finished is cut off there; the two are not added or mixed.
- **No jump.** A later tween with no `from-` starts from the value the property has at that moment, so nothing jumps. One with a `from-` jumps to it.
- **Independent.** Tweens on different properties do not affect each other, whatever their order or timing.
- `<set at="2" x="300"/>` and then `<tween at="2.5" duration="1" to-x="500"/>`: `x` is 300 from 2 s and moves to 500 between 2.5 s and 3.5 s. `<tween at="0" duration="2" to-x="100"/>` and `<tween at="1" duration="1" to-x="0"/>`: `x` rises for the first second to 50, then falls from 50 to 0 by 2 s; the first tween is cut at 1 s.
- Nothing is reported for overlap.
- `opacity` animated to 0 hides the element and everything in it.

## 9. Shared styles

A `<style>` names a set of attributes; an element takes them with `class`.

```xml
<style id="label" font-size="27" color="#696b6d"/>
<style id="loud" color="#ffffff" font-weight="700"/>
<text class="label loud" left="262" top="76">TITLE</text>
<text class="label" left="262" top="120" color="#ff4040">LIVE</text>
```

- **`<style>`** stands directly in the `<composition>`. It has an `id`, which is its name, and any number of other attributes. It draws nothing, has no children and no `class`.
- **`class`** on an element is one or more style names separated by spaces. The element is read as if the attributes of those styles were written on it.
- **Which value counts.** One written on the element itself wins. Among styles, one named later in `class` wins over one named before it. In the example the first text is white and bold and the second is red.
- **Any element that takes `class` may name a style**, and a style may hold any attribute. An attribute is checked on the element that takes it: a style with `object-fit` taken by a `<text>` is reported on that `<text>`, with the `<text>`'s line.
- **Motion.** `<tween>` and `<set>` stay inside their element and are never shared. The movement attributes in a style follow N5.
- **A style belongs to its file.** A composition shown from another file does not see the styles of the file that shows it, and the other way round.
- **Reported, each with its line:** a `<style>` without `id`; two styles with one `id`; a `class` that names a style the file does not have; a `<style>` inside an element; a `<style>` with `class` or with children.
- There are no selectors: an element takes a style only by naming it.

## 10. A composition in another file

A project may hold several composition files. A composition shows another one with a `<composition>` element that names its file:

```xml
<composition version="1" id="film" width="1920" height="1080" fps="30" duration="626.667">
  <video src="footage/presenter.mp4" left="0" top="0" width="1920" height="1080"/>
  <composition src="scenes/captions.xml" start="1.167" duration="156.5"/>
  <composition src="scenes/chapters.xml"/>
</composition>
```

- **The file is a whole composition.** It has its own `version`, size, frame rate and length, and can be rendered alone.
- **`src`** is relative to the project folder, like every other `src`, wherever the file that names it lies. The file is read when the composition that shows it is read.
- **The element is a clip.** Without `start` it starts at 0 of its parent's time; without `duration` it lasts as long as the composition in its file. Everything in the file counts its time from the clip's start.
- **Size and place.** The element has the size its file gives, so `width` and `height` are not written on it. It takes the place attributes, the movement attributes, the look attributes and `<tween>` and `<set>` children as a `<box>` does, and no other children. What the file draws outside its own frame is cut off.
- **Background.** The `background` of the file is the canvas of its own render. It is not drawn when the file is shown inside another composition.
- **Fonts and sound.** The file's fonts are added to those of the composition that shows it. Its sounds play from the clip's start and are cut at the clip's end.
- **Frame rate.** The file must have the frame rate of the composition that shows it; a file that writes no `fps` has 30. A different rate is reported with both rates and the line of the element that names the file (`FS-0046`).
- **Text properties do not pass into the file.** It looks the same wherever it is shown.
- **Nesting.** A file may be shown more than once and may itself show other files. The root file is the first file of a chain, so a chain holds at most 16 files. The element that would make a 17th file is reported on its own line, in the file that holds it (`FS-0045`). A file that shows itself, directly or through the files it shows, is reported on the line of the element that closes the loop, with the files of the loop named there (`FS-0044`); the loop is not followed. A file shown more than once is not a loop.
- **A file longer than its clip** is cut at the clip's end (the clip's window, 5.2, applies to everything in it). **A file shorter than the clip** shows nothing after its own end; its last picture does not hold.
- **Problems** in a file shown from another file are reported with the name of that file and its own line.
- **A file that is not in the project folder** is reported on the line of the element that names it, with the path as written (`FS-0047`). **A file that is there and cannot be read** is reported on that line too, with the reason the system gives (`FS-0048`). Neither is followed further.
- **Each file once.** A shown file is read once, however many elements name it, and its own problems are reported once, under the name of the first element that names it. The reports come in the order the files are reached: the root's first, then each shown file when its element is reached, each file's problems in the order of their lines.
- **One run.** The problems of the file that shows, and of every file it shows, are reported together in one run. A file with a problem is still read for the files it shows.
- A different `version` in the shown file is reported (section 2).

## 11. Sound

- **`<audio>`** stands directly in the root. `start` (default `0`) is when the sound begins on the composition's timeline; `duration` (required, above 0) how long it plays; `media-start` (default `0`) where in the source file it begins; `volume` (default `1`, 0 or more) a multiplier of its level. Times are not on the frame grid: a sound begins at the sample nearest its written time.
- **Kinds read:** N3. A sound whose number of channels is not one its kind allows is reported with the element's line, the file and the number found. Other kinds are on the table of section 14 (S28).
- **`<volume at="..." value="..."/>`**, any number, children of an `<audio>`. `at` is a time counted from the start of the sound (default `0`); `value` a number, 0 or more, required. The level is `volume` times the value of the curve. Between two points the level changes evenly; before the first point it holds the first value, after the last it holds the last. The points must be in order of time. Anything but `<volume>` inside an `<audio>` is reported.
- **A sound shorter than its clip** (N4).
- **Mix.** Every sound is decoded to the rate and channels of the output (48 kHz, two channels) and added to the others, each multiplied by its `volume` and its `<volume>` points. A source with a different rate keeps its pitch and length.
- **Length.** The sound of a render is exactly as long as the picture: the number of frames divided by the frame rate. Where no sound plays there is silence.
- **Peak.** The finished mix must not rise above 1 dB under full scale, measured between the samples as well as on them (true peak, -1 dBTP). A mix that stays under is left as it is. A mix that goes over is lowered as a whole by the least amount that brings its highest peak to that limit, to within 0.1 dB (a value stated under decision A29). Nothing is compressed and the balance between the sounds does not change. `render` says by how much the mix was lowered.
- The sound of a `<video>` is not played (S08).

## 12. The points that were recorded as open, each settled

Fifteen points were recorded as open: ten the earlier text was "not sure" about (N1 to N10) and five that a first reader found unclear (Q1 to Q5). Each has exactly one rule here, under its own heading; the rest of the document points to it. Each rule says what settles it: a decision of `DECISIONS.md`, or a **maintainer draft**, a rule the maintainer wrote under decision B4 by following what the proofs' reader did where nothing on record settles the point, which the owner may change (section 15 lists them). The seven further unclear points whose record is lost are not settled here; they come back through experiment X5 (decisions P8 and A20).

### N1. A `font-family` that is a list

**Rule.** The value is a list of families separated by commas, tried in order. A name is the `family` of a `<font>` of the composition, a family of the machine's fonts, or one of `sans-serif`, `serif`, `monospace`, looked up in that order (`text.md`, section 2.3). A name with spaces may be written as it is (`IBM Plex Mono`) or in single quotes. Each character is drawn by the first family of the list that has it. One name is a list of one. An empty entry (`A,,B`) and an empty value are reported with the element's line. A name that is neither is absent and is passed over: a text whose first family is absent from the machine and the composition is drawn with the second; one whose first family is present is drawn with the first. The fonts of the machine, and the families the three generic names stand for, are stated in `text.md`, section 2.7; they are read on macOS only in this work.

**Settled by.** Maintainer draft: the proofs passed the value to the text engine, which reads the CSS list. The lookup order, the folders and the generic families are T052's (`text.md`, sections 2.3 and 2.7).

### N2. Which font draws a character the named font lacks

**Rule.** A character that none of the families of the list has is drawn from a font of the machine. Which font that is, is not left to the system: Framesmith states it, as the first family that maps the character in a stated order of the machine's families (`text.md`, sections 2.4 and 2.7), so that one machine gives the same picture on every run. The picture of such a character still depends on the machine's fonts; this is a known gap, recorded as such until the stand-in font set of Sprint 5 (decision A9; `DEFERS.md`). A character that no font of the machine has either is drawn as the missing-character mark: glyph 0 of the first family of the list. No rule that names a font for it can be given in this work, because the stand-in fonts are not chosen; for that reason `check` reports each text that holds a character neither the listed families nor the machine's fonts can draw, as a note with the element's line (a note does not change the exit code, section 13). A text that uses only the families of its list, such as every fixture with the test font made by us (decision A8), does not depend on the machine.

**Settled by.** Decision A9 for the machine's font; the stated order is T052's (`text.md`, section 2.4), agreed by the lead on 2026-10-25 in place of the machine's own choice; the note is a maintainer draft.

### N3. Which picture and sound kinds are read

**Rule.** Only the kinds on this list are read; any other kind is not claimed as read, even where the system could read it. Judged by the content of the file, not by its name.

| For | Kinds read |
|---|---|
| `<image src>` | PNG and JPEG |
| `<video src>` | files of the MP4 family, `.mp4` and `.mov`, holding H.264 or HEVC pictures |
| `<audio src>` | WAV and AIFF of linear PCM (one, two or six channels), MP3 (one or two) and AAC-LC in an MP4 file, `.m4a` (one or two) |

A file of a kind not on the list is unsupported: `check` lists it with the element's line, the file and the kinds read, exits with 2, and `render` stops before the first frame and writes no file. A file that is missing or damaged is invalid (exit code 1). The first picture of every video is decoded when the video is looked at.

**Settled by.** The kinds frx7's spec lists as read at the time of writing (the list of sound kinds is the one there; the list of pictures is the two kinds the proofs tried); the refusals follow the project's exit codes (decision P2). Maintainer draft for the picture list.

### N4. A video or a sound shorter than its clip

**Rule.** A video whose source ends before its clip does shows its last source picture on every frame after the source's last picture, until the clip's window ends. A sound whose source ends before its `duration` plays to the source's end and is silent after. `check` passes such a project and prints a note with the element's line, the source's length and the clip's length. A `media-start` at or past the end of the source is reported with the element's line, the path and the source's length. (A source picture shown on a clip's first frame, and on every other frame, follows 5.3.)

**Settled by.** Maintainer draft: the proofs left it to the decoder. Holding the last picture is the choice that never shows an empty frame; the owner may prefer to refuse the case.

### N5. Motion attributes in a shared style

**Rule.** A `<style>` may hold the twelve movement attributes `x`, `y`, `scale`, `scale-x`, `rotation`, `skew-y`, `opacity`, `blur`, `brightness`, `rotate-x`, `rotate-y` and `reveal`, and `typed`. Their value is the element's starting value, exactly as if written on the element; a value written on the element itself wins, and the `<tween>` and `<set>` children of the element move away from it as they do from a value written on the element. `<tween>` and `<set>` themselves are never shared (section 9). `count` and `draw` in a style are checked on the element that takes the style like any other attribute.

**Settled by.** Maintainer draft: the proofs gave the same result by construction but had no example. The attribute `typed` alone had no effect in the proofs; here it is the number of characters shown before any tween or set of `typed` has started (ticket T069, criterion 6; B58): with no tween and no set of `typed`, the attribute shows the number of characters it gives, and a text with no `typed` shows all of it. On an element with a tween or a set, it is the number of characters shown before the first of them has started.

### N6. A zero or negative size, gap or font size

**Rule.**

| Attribute | Taken | Reported |
|---|---|---|
| `width`, `height` (pixels or percent) | 0 or more; an element of size 0 draws nothing of its own, its children are laid out and drawn as for any box (unless `overflow="hidden"`) | below 0 |
| `max-width` | above 0 | 0 or below |
| `gap`, `padding`, `flex-grow`, `border-radius`, `border` width, `text-stroke` width | 0 or more | below 0 |
| `font-size`, `line-height` | above 0 | 0 or below |
| `stroke-width`, `perspective`, `dash` total | above 0 | 0 or below |
| `<video>` `width`, `height` | a whole number of 2 or more | anything else |
| root `width`, `height` | an even whole number from 2 to 8192 | anything else |

`margin`, `left`, `top`, `right`, `bottom` and `letter-spacing` may be negative. A text whose `width` is 0 wraps at every place a line may end. A reported value is reported with the element's line, the attribute, the value and what is taken.

**Settled by.** B63 (`DECISIONS.md`, 2026-10-27): the owner confirmed this rule as written. The proofs did not check these and described no result; the rule only refuses what has no meaning and draws what has one.

### N7. Whether an `id` must be unique

**Rule.** No. An `id` is a name and refers to nothing in version 1. Two elements, or an element and an `<audio>`, with one `id` are accepted and nothing is reported. The ids of `<style>` elements are a separate matter: they must be unique within the file (section 9). Unique, stable ids come in Sprint 2 (`DEFERS.md`); a composition that repeats an `id` then needs a new version of the notation.

**Settled by.** Maintainer draft: the proofs recorded `id` and checked nothing. The root's name is `id` (decision A29).

### N8. A time written without a leading digit

**Rule.** It is read: `.5` is the time 0.5. A time is digits, or digits, a point and digits, or a point and digits; a point with no digits after it (`5.`, `.`) is reported, and so is an empty value. This holds for `start`, `duration`, `media-start`, `at` and every other time.

**Settled by.** Maintainer draft, the reading of the `timebase` crate (`Time::parse`), whose tests hold the accepted and refused forms.

### N9. A number written with an exponent or a sign

**Rule.** A number may carry a leading `+` or `-` and an exponent: `+5` is 5, `-4` is -4, `1e3` is 1000, `2.5E-1` is 0.25. A point with digits on one side only is allowed: `.5` and `5.` are the numbers 0.5 and 5. A number must be finite: `inf`, `infinity` and `nan`, in any case, are reported as not numbers, and so is a number too large to be finite (`1e999`), which is reported as too large. An exponent with no digits (`1e`, `1e+`), a number with a unit (`12px`), with a percent sign or with a thousands separator, and a value that is empty are not numbers. A time never takes a sign or an exponent (N8, section 4). A whole number may be written with an exponent when its value has no fraction (`1.28e3`). A number is kept as written, with every digit it is written with and its exponent: a length, or a percentage, is worked out from those digits and not from a float, exactly as `render-command.md` section 4.1 says.

**Settled by.** Maintainer draft: the proofs read numbers with the standard decimal reader, which also let non-finite words through; those are now refused so that no unknown form changes a picture.

### N10. `margin` on an absolutely placed item

**Rule.** It acts as in CSS. An absolutely placed item is a `<box>`, a `<text>` or a `<composition src>` with `position="absolute"` in a row, a column or a set of columns, or any of them in a plain box, whether or not it writes `position` (7.1). Its `margin` is read as the sides of section 4, and it moves the item in from each edge of its parent that places it, by the margin of that side, a number of pixels. The edges of the parent are its outer edges, as Q2 says.

- With `left`, the item's left edge lies at `left` plus its left margin from the parent's left edge.
- With `right` and no `left`, its right edge lies at `right` plus its right margin from the parent's right edge.
- With `top`, its top edge lies at `top` plus its top margin from the parent's top edge.
- With `bottom` and no `top`, its bottom edge lies at `bottom` plus its bottom margin from the parent's bottom edge.
- With `left` and `right` and no `width`, its width is what lies between the two edges less the left and the right margins; with `top` and `bottom` and no `height`, its height is what lies between them less the top and the bottom margins. A size that comes out below 0 is 0.
- On an axis with no edge (neither `left` nor `right`, or neither `top` nor `bottom`), the item lies at its static position, moved in by its margin on the near side (its left margin across, its top margin down). The static position is the place the item would have if it were not absolutely placed: in a plain box, the parent's near edge (its left or its top); in a row, a column or a set of columns, the place the container would give the item if it were its only item, its margins counted as for any item, at the size the item has as an absolutely placed one.
- A side whose edge places nothing (a far edge that a near edge or a written size makes unused, as Q1 says) moves nothing.

So `left="50" top="10" margin="30"` places the item's top left corner at (80, 40) of its parent; `right="0" bottom="0" margin="2 4"` places its bottom right corner 4 in from the parent's right edge and 2 up from its bottom edge; and an item of a plain box with no edge and `margin="4 6"` has its top left corner at (6, 4) of its parent.

**Settled by.** B37 (`DECISIONS.md`, 2026-10-26): the owner chose that `margin` on an absolutely placed item acts as in CSS. B41 (`DECISIONS.md`, 2026-10-26): the owner chose that the children of a plain box are absolutely placed and that CSS's rules for them hold in full, the static position on an axis with no edge included. Source: CSS 2.1, sections 10.3.7 and 10.6.4 (the widths, heights and offsets of absolutely positioned boxes, where the margins lie between the containing block's edges and the box, and the static position stands in for a missing offset), CSS Flexible Box Layout Level 1, section 4.1 (the static position of an absolutely positioned child of a flex container is that of its sole flex item), and CSS Positioned Layout Level 3, read for their meaning only and written here in our own words.

### Q1. Whether `bottom` places a box that has a height

**Rule.** Yes. With `bottom` and no `top`, the box's bottom edge lies `bottom` from the parent's bottom edge, and its top edge is the parent's height less `bottom` less the box's height. The same holds for `right` and `width`. When `top` is also written, `top` places the box and `bottom` changes nothing if a `height` is written; with no `height`, `top` and `bottom` together give the height (7.1).

**Settled by.** B63 (`DECISIONS.md`, 2026-10-27): the owner confirmed this rule as written.

### Q2. Where an absolutely placed child is drawn and counted from

**Rule.** It is counted from the parent box's own edges: the outer box, with the parent's `padding` not subtracted, and percentages are of the parent's full width and height. It takes no room in the row, column or set of columns and does not count in `gap`. It is drawn in document order with its siblings: over the siblings written before it, under those after it. Every child of a plain box is placed the same way, whether or not it writes `position`, its margin included (N10).

**Settled by.** Maintainer draft, following the proofs' layout code; the children of a plain box by B41 (`DECISIONS.md`, 2026-10-26).

### Q3. Whether a clip not yet shown keeps its place in a column

**Rule.** Yes. Layout is worked out once for the whole composition and does not change from frame to frame. A clip that is outside its window takes its room in a row, column or set of columns during the whole composition; it is only not drawn. Layout that follows clips and motion waits until Sprint 4 (`DEFERS.md`).

**Settled by.** Maintainer draft, following the proofs' layout code.

### Q4. In whose time a tween inside a span counts

**Rule.** In the time of the span's parent, the same time as the span's own `start`, as for every element (8.5). A `<tween at="0.2">` in a span that stands in a `<text start="0.8">` begins at 1.0 s of the composition, whatever `start` the span has itself.

**Settled by.** Maintainer draft, following the proofs' timing code.

### Q5. The colour of a text that names none

**Rule.** `#000000`, fully opaque (black), unless a parent, a style or the element itself names one. The same holds for a `<shape>`'s `color` and a `<span>` with no colour of its own (it takes its parent's). The canvas of a root with no `background` is also `#000000`, so a text that names no colour on such a canvas cannot be seen.

**Settled by.** Maintainer draft, following the proofs' defaults.

## 13. The messages for the common mistakes

Each message is given with its line as `line N: message`, and `file, line N: message` for a problem in a file shown by `<composition src>`. The reader collects every problem and prints them all, one line each. The code and the remedy of each message belong to the error catalogue; the wording below is what an author reads. A line that begins `note` is a note: a form accepted and drawn as this document states, that an author may not have meant. A note does not change the exit code. Its form is `note: <file>, line N: <message>`, and `check` prints the notes before its closing line. The one note of this version is that of N2, a `<text>` with a character no font maps (`text.md`, section 2.4, point 3).

| Mistake | Message |
|---|---|
| Not XML | `the file is not well-formed XML: ...` |
| Root is not `<composition>` | `the root element is <html>, not <composition>` |
| No `version` | `<composition> needs the attribute `version`; the versions read are: 1` |
| A version not read | `` `version="2"` is not a version this program reads; the versions read are: 1 `` |
| Unknown element | `<circle> is not a known element; inside <box> the elements taken are <box>, <rect>, <text>, <image>, <video>, <shape>, <composition src>, <tween> and <set>` (the elements taken where it stands; `no element is taken inside <tween>` where none is) |
| Unknown attribute | `<rect> has no attribute `colour`` |
| Unknown attribute on a tween | `<tween> has no attribute `to-width``; on a set: `<set> has no attribute `duration`` |
| Other element in a text | `<box> is not known inside <text>; only <span>, <tween> and <set> are; a <box> stands directly in <composition>, <box> or <rect>` (the same form for every element in a place it may not stand: what the place holds, then where the element may stand; `<font> holds no element` where it holds none) |
| `<span>` outside a text | `<span> is not a known element here; a <span> stands directly in <text> or <span>` |
| `<style>`, `<font>` or `<audio>` inside an element | `<style> is not known inside <box>; a <style> stands only directly in the <composition>, never inside an element` |
| Not a number | `` `x="a"` is not a number: an optional sign, digits with an optional point and an optional exponent, such as 12, -4, .5 or 2.5e-1 ``; `` `x="1e999"` is too large: a number must be finite ``; `` `width="10.5"` is not a whole number `` (the `width` and `height` of a `<video>`) |
| Not a length | `` `width="ten"` is not a number or a percentage `` |
| Not a time | `` `at="-1"` is not a time in seconds: digits with an optional point, such as 5 or 1.25 `` |
| Not a colour | `` `color="red"` is not a colour written as #rrggbb or #rrggbbaa `` |
| Value not in the list | `` `text-align="justify"` is not one of: left, center, right `` (the same for `flex-direction`, `justify-content`, `align-items`, `align-self`, `position`, `overflow`, `font-style`, `text-transform`, `white-space`, `object-fit`) |
| Below the range | `` `font-size="0"` must be above 0 ``; `` `width="-5"` must be 0 or more ``; `` `gap="-2"` must be 0 or more `` (N6) |
| Bad background | `` `background="red"` is not a colour or a list of gradients `` |
| Bad shadow | `` `box-shadow="..."` is not a list of shadows `` (the same for `text-shadow`) |
| Bad border or text-stroke | `` `border="2px red"` is not a width and a colour `` |
| Bad padding or margin | `` `padding="a"` is not one to four numbers `` |
| Bad line height | `` `line-height="x"` is not `normal`, a number or pixels `` |
| Bad letter spacing | `` `letter-spacing="x"` is not a number or ems `` |
| Bad font list | `` `font-family="A,,B"` has an empty family `` |
| Bad transform origin | `` `transform-origin="..."` is not two lengths `` |
| Bad clip path | `` `clip-path="..."` is not a polygon of three or more corners in percent `` |
| Bad filter | `` `filter="..."` is not a list of: brightness(), contrast(), saturate(), grayscale() `` |
| Bad mask | `` `mask="..."` is not one gradient `` |
| Bad torn edge | `` `displace="..."` is not an amount above 0 and a noise: `14 noise(0.035 3 9)` `` |
| Bad uneven ink | `` `grain="..."` is not a noise, a slope and an offset: `noise(0.9 2 5) -1.7 1.75` `` |
| Bad shadow of an element | `` `drop-shadow="..."` is not one shadow: x, y, blur and a colour `` |
| Bad perspective | `` `perspective` must be above 0, not 0 `` |
| Columns | `` `columns="wide 1fr"` is not a list of column widths: pixels, or shares such as `1fr` ``; `` `columns` is for a <box> ``; `` a <box> has `columns` or `flex-direction`, not both `` |
| Kind of figures | `` `font-variant-numeric="slashed-zero"` is not `normal` or a list of: lining-nums, oldstyle-nums, tabular-nums, proportional-nums `` |
| Picture place | `` `object-position="top"` is not two percentages, across and down `` |
| Two cuts | `` `transition="cut"` cannot be joined with `clip-path` ``; `` `transition="cut"` cannot be joined with `overflow="hidden"` and rounded corners `` |
| Shader opening | `` the transition `wave.wgsl` could not be read: ... ``; `` the transition `wave.wgsl` is not a shader the card takes: ... `` (with the compiler's words); `` the transition `wave.wgsl` is a shader, which only the GPU renderer draws; render with `--backend gpu` `` |
| Colour table | `` the colour table `grade.cube`: it has 100 rows of colours; a table of size 33 has 35937 `` (other reasons: the file could not be read, a table of one dimension, a range other than 0 to 1, a keyword that is not known, a row that is not three numbers) |
| Start without duration | `<box> needs both `start` and `duration`, or neither` |
| Duration without start | `<box> needs both `start` and `duration`, or neither`; the same message, on the line of the element (section 5.2) |
| Missing file name | `<image> needs the attribute `src``; the same for `<video>`, `<audio>`, `<font>` |
| Picture, footage or sound not read | `` the picture `logo.bmp` is of a kind that is not read; the kinds read are: png, jpeg ``; the same for footage and sound, with their kinds (N3) |
| Source missing or damaged | `` the picture `logo.png` could not be read: ... `` (the file, the reason) |
| Video size | `` `width` of a <video> must be a whole number of 2 or more, not 10.5 `` |
| `media-start` | `` `media-start="40"` is past the end of `clip.mp4`, which is 12.5 s long `` (N4) |
| Sound longer than its source | `` note: `bed.wav` is 4 s long and its <audio> lasts 6 s; it is silent after 4 s `` (N4); the same for footage: `` note: `clip.mp4` is 4 s long and its <video> lasts 6 s; its last picture holds `` |
| Sound with other channels | `` `bed.wav` has 4 channels; sounds of 1, 2 or 6 are read `` |
| Audio without duration | `<audio> needs a `duration`` |
| Sound points out of order | `the <volume> points of an <audio> must be in order of time` |
| Anything but `<volume>` in an audio | `<tween> is not known inside <audio>; only <volume> is; a <tween> stands directly in <box>, <rect>, <text>, <span>, <image>, <video>, <shape> or <composition src>` (with the name found) |
| Missing attribute | `<composition> needs the attribute `width`` |
| Bad frame size | `` `width` must be an even whole number from 2 to 8192, not 641 `` |
| Bad frame rate | `` `fps` must be a whole number from 1 to 240, not 0 `` |
| No length | `<composition> needs a `duration` above 0` |
| Tween without duration | `<tween> needs a `duration` above 0` |
| Tween or set without a property | `<tween> changes no property` (or `<set> ...`) |
| Unknown easing | `` `ease="bouncy"` is not a known easing; the easings taken are linear, in-quad, ... and out-back(n), with n a number `` (every name, as section 8.4 gives it) |
| Background layers | `` `background-size` has 2 entries for 3 layers; it takes one for each layer, or one for all ``; `` `background-repeat` needs a `background` to place ``; `` `background-size="8"` is not a list of `auto` or a width and a height ``; `` `background-position="..."` is not a list of an x and a y ``; `` `background-repeat="round"` is not a list of `repeat` or `no-repeat` `` |
| A number that counts | `` `count` is for a <text>, not a <box> ``; `a <text> that counts holds no words of its own: its text is the number`; `` `count-format` needs `count` ``; `` a tween of `count` needs the attribute `count` on its <text> ``; `` `count-format="..."` is not a form of number; the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals `` |
| Shape | `` `draw` is for a <shape> with `stroke-width` `` (the same for `dash`); `` `dash="..."` is not pairs of lengths: a dash and a gap, in view-box units ``; `<shape> needs `d` or a `src` with paths`; `` `...` is not a view box of four numbers `` |
| Style without id | `<style> needs the attribute `id`: the name elements take it by` |
| Two styles, one id | `two <style> elements have the id `label`` |
| Unknown class | `` `class` names the style `big`, and this file has no <style> with that id `` |
| Style with class or children | `the style `x` has `class`; a style cannot take another style`; `the style `x` holds an element; a style holds attributes only` |
| Composition file | `a <composition> inside a composition needs the attribute `src`; without one it stands only as the root of a file`; `the composition `...` could not be read: ...`; `the composition `...` shown here could not be read: ...`; `the composition `...` shown here is not in the project folder`; `` `a.xml` shows itself, directly or through the files it shows: the loop is `a.xml`, `b.xml`, `a.xml` ``; `` `b.xml` would be the 17th composition of its chain, and a chain holds at most 16, the root counted `` ; `<box> is not known inside <composition src>; only <tween> and <set> are; a <box> stands directly in <composition>, <box> or <rect>`; `` `a.xml` has 24 frames a second and the composition that shows it has 30; they must be the same `` |
| A form on the support table | `` `z-index` is not supported in version 1: a stacking order apart from document order; it comes in Sprint 2 `` (section 14; `check` exits with 2 and `render` refuses) |

The text of a file that cannot be read when the project is loaded (fonts, pictures, footage, sound) is `line N: ` followed by the element's own words and the reason from the system.

## 14. The support table

A form on this table is known and is not supported in version 1. It is listed by `check`, which exits with 2 when nothing but such forms stop the render, and refused by `render` before the first frame with no output: the form is never rendered in part. Anything that is not on this table and not in the tables of sections 6 to 12 is **invalid**, and `check` exits with 1. Every form that has a name today is on the table; a later feature that has no name yet (variables, themes, batches, sound effects, grading, captions, speech, background removal, drawn content, 3D, the catalog) cannot be written, so it is invalid until its sprint gives it a form.

The names and recognition rules of the entries are **proposed by the maintainer** and drafted for the product owner's approval, once, before the ticket that lists and refuses them is worked (decision A28; `DEFERS.md`). The owner may strike an entry, add one, or change the reason or the sprint.

| # | The form an author can write | How `check` recognises it | Reason | Waits until |
|---|---|---|---|---|
| S01 | A start placed relative to another clip: `start="intro"`, `start="intro+1"` | A `start` that begins with a letter or `@` | Needs unique, stable ids and a worked-out timeline | Sprint 2 |
| S02 | A duration worked out from the content: `duration="auto"` | `duration` with the value `auto` | Needs the same timeline | Sprint 2 |
| S03 | A frame rate as a ratio: `fps="30000/1001"` | `fps` with a `/` | The notation and the encoder settings take whole rates; the ratio waits for them | Sprint 2 |
| S04 | A canvas preset: `canvas="landscape-1080p"` | The attribute `canvas` on the root | The presets are not chosen | Sprint 2 |
| S05 | A stacking order: `z-index` | The attribute `z-index` on a drawn element | Drawing order is document order; a second order changes the drawing rules | Sprint 2 |
| S06 | Trimming footage or sound at the end: `media-end` | The attribute `media-end` on `<video>` or `<audio>` | Needs the timeline of S01 and S02 | Sprint 2 |
| S07 | A playback rate: `playback-rate` | The attribute on `<video>` or `<audio>` | Needs resampling of pictures and sound at a rate | Sprint 2 |
| S08 | A video's own sound: `volume` or `muted` on a `<video>` | Either attribute on `<video>` | The sound of footage is not mixed; write an `<audio>` | Sprint 2 |
| S09 | Looping or holding footage or sound: `loop`, `hold` | Either attribute on `<video>` or `<audio>` | Needs S06 and S07 | Sprint 2 |
| S10 | Fitting footage: `object-fit`, `object-position` on `<video>` | Either attribute on `<video>` | Not in the proofs (decision A21) | Sprint 5 |
| S11 | Several compositions rendered by name; a timeline: the keys `compositions` and `timeline` of `framesmith.yaml` | The keys in the project file | The project file names one composition | Sprint 2 |
| S12 | Keyframes: a `<keyframes>` element | The element `<keyframes>` in an element | The motion engine has tweens and sets only | Sprint 4 |
| S13 | Staggered tweens: `stagger` | The attribute on `<tween>` | Needs several targets for one tween | Sprint 4 |
| S14 | Labels: `label` on a tween or set, `at="intro"` | The attribute `label`, or an `at` that begins with a letter | Needs a timeline of named points | Sprint 4 |
| S15 | Repeats: `repeat`, `yoyo` | Either attribute on `<tween>` | Needs a repeat in the evaluator | Sprint 4 |
| S16 | More easings: `bounce`, `elastic`, `in-back`, `out-bounce`, `in-circ`, `steps(4)`, `cubic-bezier(...)` and the like | An `ease` that is not on the list of 8.4 and is built from `in-`, `out-`, `in-out-`, `bounce`, `elastic`, `back`, `circ`, `steps(` or `cubic-bezier(`, except a bare `out-back`, which is `out-back(n)` written without its number and so an easing not known (`FS-0063`) | The engine has the nineteen curves and `out-back(n)` | Sprint 4 |
| S17 | Conic gradients: `conic-gradient(...)` | The word in `background` or `mask` | Not drawn by either renderer | Sprint 5 |
| S18 | A mask made from a picture: `mask="masks/shape.png"` | A `mask` that is not a gradient and ends in a picture name | Needs a picture as a mask in both renderers | Sprint 5 |
| S19 | The rest of flex and a grid: `flex-wrap`, `flex-basis`, `flex-shrink`, `order`, `display`, `grid-template-columns`, `row-gap`, `column-gap`; `justify-content="space-around"` or `"space-evenly"` | Those attributes, and those values | Layout has rows, columns and a set of columns only | Sprint 5 |
| S20 | Borders by side: `border-top`, `border-right`, `border-bottom`, `border-left`, `border-width`, `border-color`, `border-style` | Any of the attributes | One border round the box only | Sprint 5 |
| S21 | A paragraph that runs from right to left: `direction="rtl"` | The attribute `direction` or `dir` on a text | Lines are ordered inside, the paragraph runs left to right; the whole-paragraph direction is Sprint 5's (decision B36) | Sprint 5 |
| S22 | Motion along a path: `motion-path` | The attribute on `<tween>` | The motion engine moves along straight lines | Sprint 4 |
| S23 | Expressions: `from-x="=width/2"` | A value of a `<tween>` or `<set>` that begins with `=` | A composition carries no code | Sprint 4 |
| S24 | Animated size and colour: `to-width`, `from-color`, `to-background`, `<set width="...">` | A `from-`, `to-` or `<set>` attribute named as any attribute of the element tables that is not one of the fifteen of 8.1, nor the time `start` or `duration` | Layout does not follow motion and colours are not mixed by the engine | Sprint 4 |
| S25 | Hiding the back of a turned element: `backface-visibility` | The attribute | A turned element is drawn mirrored (decision A23) | Sprint 4 |
| S26 | A still picture whose file stores a turn | An `<image src>` file with the turn mark; told by reading the media file (T087) | The turn must be applied to the picture (decision P10) | Sprint 7 |
| S27 | Footage in other kinds of file or other codecs: Matroska, WebM, VP9, AV1, ProRes | A `<video src>` whose file or codec is not on the list of N3; told by reading the media file (T089) | Their codecs are not written yet | Sprint 7 |
| S28 | Sound in other kinds: Opus, FLAC and the like | An `<audio src>` of a kind not on the list of N3; told by reading the media file (T089) | Their decoders are not written yet | Sprint 7 |
| S29 | Footage with transparency or HDR | A `<video src>` marked so; told by reading the media file (T089) | Both are held to 8 bits and opaque in this work | Sprint 7 |
| S30 | Animated pictures: GIF, animated PNG, animated WebP in `<image>` | An `<image src>` of such a file; told by reading the media file (T089) | A still picture is one frame | Sprint 7 |
| S31 | A remote source: `src="https://..."` | A `src` that begins with a scheme and `://` | Nothing is read from a network (decision P6) | Sprint 7 |
| S32 | Footage whose edit list has more than one playing entry or a media rate other than 1; footage in fragments | The file, when it is looked at; told by reading the media file (T089) | Refused by name (decision P12) | Sprint 7 |
| S33 | A corner written as an ellipse: `border-radius="24 / 12"` or `"24 12"` | A `border-radius` of two numbers, or of two numbers with a `/` between them | One circular radius per corner only | Sprint 5 |
| S34 | A film written to a path that does not end in `.mp4` or `.m4v`: `render.output: renders/film.mov`, `framesmith render --output film.mov` | A `render.output` of the project file, or an `--output` of `render`, whose extension, in any case, is not `mp4` or `m4v`, a path with no extension among them | A film is written as an MP4 file only; a QuickTime movie is Sprint 9's, and no other kind of file is planned (decision B40) | Sprint 9 (`.mov`); not assigned (any other) |
| S35 | Border, border-radius and overflow on a `<text>` or a `<span>`; opacity, blur and brightness on a `<span>`: `border`, `border-radius`, `overflow`, `opacity`, `blur`, `brightness` | The attribute on a text or a span | Not drawn on a text or a span yet | Sprint 5 (n05-layout-text-look), acceptance criterion 1: every attribute value of the notation has a fixture with a checked-in picture on the processor and agreement on the card (PLAN.md); roadmap 5.23 |
| S37 | Animated blur and brightness: `blur` or `brightness` in a `<tween>` or a `<set>` | The attribute on a `<tween>` or a `<set>` | Animated blur and brightness are not drawn yet | Sprint 1, T067 |
| S38 | Blends: `mix-blend-mode` | The attribute on an element | Blending with what is under is not drawn yet | Sprint 1, T057 |
| S39 | Torn edges and grain: `displace`, `grain` | Either attribute on an element | Not drawn yet | Sprint 1, T058 |
| S40 | Still pictures: `<image>` | The element `<image>` | Still pictures are not read or drawn yet | Sprint 1, T059 |
| S41 | A cut to a polygon: `clip-path` | The attribute on an element | Not drawn yet | Sprint 1, T061 |
| S42 | Shapes: `<shape>` | The element `<shape>` | Path data is not drawn yet | Sprint 1, T062 |
| S43 | A counting or typed text, and motion of a span: `count`, `count-format`, `typed`; `x`, `y`, `scale`, `scale-x`, `rotation`, `skew-y`, `rotate-x`, `rotate-y`, `perspective`, `transform-origin` on `<span>` | Those attributes on `<text>` or `<span>`, and the motion attributes on `<span>` | Counting, typing and the motion of a span are not built yet | Sprint 1, T069 |
| S44 | Transitions: `transition`, `reveal` | Either attribute on an element | Openings over the scene are not drawn yet | Sprint 1, T072 |
| S45 | Footage started inside its source: `media-start` | The attribute on `<video>` | The start in the source is not applied yet | Sprint 1, T075 |
| S46 | A colour table or a filter on footage: `lut`, `filter` | Either attribute on `<video>` | Not applied yet | Sprint 1, T078 |
| S50 | Several workers: `render.workers` other than `auto` or `1`, and `--workers` | The setting or option with such a value | So far one worker draws and encodes | Sprint 1, T096 |
| S51 | An element inside `<composition src>`, `<video>` or `<text>` other than its words, spans, tweens and sets | An element child of `<composition src>` or `<video>`; an element other than `<span>`, `<tween>`, `<set>` inside `<text>` | A shown composition is drawn from its file, footage is a picture, a text is its words | Not assigned |
| S52 | Footage in a flow: a `<video>` in a row, a column or a set of columns | A `<video>` child of a `<box>` with `flex-direction` or `columns` | Footage is placed by its own edges | Not assigned |
| S53 | Motion of an element that holds a `<composition src>` | A `<tween>` or `<set>` on an element with a `<composition src>` inside it | A shown composition moves only as itself | Not assigned |
| S54 | A text with `left` and `right` and no `width` in a flow | Such a `<text>` child of a row, a column or a set of columns | Its width cannot be worked from both edges inside a flow | Not assigned |
| S55 | A gap as a percentage: `gap="5%"` | `gap` with `%` | Gaps are whole pixels | Not assigned |
| S56 | A fraction of a pixel in a place or a size: `left="10.5"` | A place or size attribute that is a number with a fraction | Edges lie on whole pixels | Not assigned |
| S57 | A place or a size beyond 1,000,000 pixels | A place or size that works out beyond the limit | The limit of a place | Not assigned |
| S58 | A font file other than TrueType or a collection: `.otf`, `.woff`, `.woff2`, or a collection that cannot be read | `<font src>` of such a kind, or a collection the reader refuses | Only `.ttf` and `.ttc` are read | Not assigned |
| S59 | A background or padding on an element other than a box: `background`, `background-size`, `background-position`, `background-repeat` on `<text>`, `<span>`, `<video>` or `<composition src>`; `padding` on `<text>` | Any of those attributes on such an element | Backgrounds are drawn behind boxes only, and a text is not padded | Not assigned |
| S60 | `<rect>` | The element `<rect>` | `<rect>` is not read yet, though section 6.3 names it | Not assigned |
| S61 | A colour with alpha below `ff` anywhere but a box's `background`: the root's `background`, a text's `color`, `text-shadow`, `text-stroke` | A colour of eight hex digits whose last two are below `ff` in one of those attributes | Only a box's background is laid over what is under it; a film's ground is opaque | Not assigned |
| S62 | The sound bit rate outside 64 to 320: `render.audio-bitrate` | The setting with a value below 64 or above 320 | The system's sound encoder takes 64 to 320 kbit/s | Not assigned |
| S63 | A line alignment on a span: `text-align` on `<span>` | The attribute on `<span>` | Lines are aligned by their text, and a span lies inside a line | Not assigned |
| S64 | Movement of footage and of a shown composition: `x`, `y`, `scale`, `scale-x`, `rotation`, `skew-y`, `transform-origin`, `rotate-x`, `rotate-y`, `perspective` on `<video>` or `<composition src>`, `opacity` on `<composition src>`, or a `<tween>` or `<set>` inside them | Such an attribute on `<video>` or `<composition src>`, or a `<tween>` or `<set>` inside one | So far footage and a shown composition are drawn in their place only | Sprint 1, the order picture-motion |
| S65 | A span's window and dressing: `start`, `duration`, `text-shadow`, `text-stroke`, `font-variant-numeric` on `<span>`; `white-space` other than `normal` and `text-transform` other than `none` on `<span>` | Any of those attributes on `<span>`, the last two with such a value | So far a span is drawn for its text's whole life and in its plain ink only | Sprint 1, the order span-forms |
| S66 | The item forms of a video or a shown composition: `flex-grow`, `max-width`, `align-self` on `<video>` or `<composition src>` | Any of those attributes on such an element | So far these are taken on boxes and texts only | Sprint 1, the order item-forms |
| S68 | A margin or a padding of a span as a percentage, or a negative margin of a span | `margin` or `padding` with `%` on `<span>`, or a `margin` of `<span>` below 0 | A span's room is a number of pixels, 0 or more | Not assigned |
| S69 | Room of a span with no word to hold it, or at a line break after a character that is not a space or a hyphen | A `margin` or `padding` of a `<span>` in a `<text>` with no word, or one that falls where the line may break before the span after such a character | The room has no place in the line there | Not assigned |
| S70 | A percentage in the size or the room of a text inside a flow | `width`, `height`, `padding` or `margin` with `%` on a `<text>` that is an item of a row, a column or a set of columns | Inside a flow a text is measured in whole pixels | Not assigned |
| S72 | An emoji whose colour picture is not a PNG, or that frx7 cannot read | A character drawn from a colour font with no PNG picture for it, or one whose picture frx7 refuses | Only PNG colour pictures are drawn | Not assigned |
| S73 | An inner shadow: a shadow written with the word `inset`, in `box-shadow` | The word `inset` in a shadow of a `box-shadow` | Only shadows outside the box are drawn (ROADMAP 5.13) | Sprint 5 |
| S74 | A shadow with a spread: a fifth value, a length, between the blur and the colour: `box-shadow="0 24 70 8 #00000099"` | A shadow of five values, the fourth a length | Only the four values `X Y BLUR COLOUR` are drawn (ROADMAP 5.13) | Sprint 5 |
| S75 | A shadow colour with an alpha below `ff`: a `box-shadow` or `drop-shadow` whose colour is `#rrggbbaa` with `aa` below `ff` | A shadow of four values whose colour is `#rrggbbaa` with `aa` below `ff` | Only opaque shadow colours are drawn | Not assigned |

S71 is left free: its form, a character no font of the composition draws, is not reached on macOS, where the machine's fonts are read. It comes back with the sprint for another system (DEFERS.md); the refusal stays in the program meanwhile.

**Entries that wait on Sprint 1.** An entry whose sprint is "Sprint 1, T0xx" or "Sprint 1, the order ..." is temporary (decisions B42 and B43): it leaves this table when that ticket or order closes, and its fixture then becomes a test of the supported form. The entries "Not assigned" are limits no ticket plans (decision B42). S33 is added by T054 (decision B35).

**Not unsupported, but limited.** A shader opening is supported on the card's renderer only (7.8). A character drawn from a font of the machine (N2) is drawn. Repeated `id`s are accepted (N7). These are not refused.

## 15. Sources, and what the owner may change

### Sources

- **Public standards.** CSS (the meaning of `left`, `width`, `background`, gradients, `object-fit`, `perspective`, shadows, `font-variant-numeric`, `border-style: double`); SVG (path data, view boxes, dashes); the Unicode line-breaking rules (UAX 14) and the bidirectional algorithm (UAX 9); the W3C's Filter Effects recommendation (fractal noise, moved pixels, alpha arithmetic); the W3C's Compositing and Blending recommendation (the blend formulas); the WebGPU shading language (the shader opening); the `.cube` text form for colour tables; the common definitions of the quint, sine and exponential easing curves and of the back curve.
- **Decisions of `DECISIONS.md`:** A8, A9, A21, A22, A23, A28, A29, B4, B33, B35, B36, B38, B40, B42, B43, P1, P2, P6, P8, P10, P12, P13 (radial gradients are drawn in Sprint 1; conic gradients stay in Sprint 5).
- **Other specs of this repository:** `frame-time.md` (the frame-grid rule and the reading of times) and `motion.md` (the motion engine). frx7's spec of the system's codecs for the kinds read.
- **`PLAN.md`:** section 5 (the checks and tolerances), the account of the sprints for the support table, and `DEFERS.md` for what waits.
- **Records of the proofs of concept** (`POC_PLAN.md`, experiments E3, E6 and E9), for the reasons the forms exist; and the behaviour of the proofs' reader, read as a record and rewritten here (decision B4).
- The attribute names are CSS's where CSS has them; every other name (the elements, `count`, `typed`, `draw`, `displace`, `grain`, `reveal`, `transition`, `lut`, `media-start`, the easing names with `out-back(n)`) is ours. No text, code, asset, name or message of any other product's documentation or source is used here.

### What is a maintainer draft

These rules were not settled by a decision; the maintainer wrote them from what the proofs did (decision B4), and the owner may change any: N1; the note of N2; the picture list of N3; N4; N5; N6; N7; N8; N9; Q1; Q2; Q3; Q4; Q5; the whole support table of section 14 (decision A28); and these further choices: a style may hold `typed` (N5); a `<composition src>` file shorter than its clip shows nothing after its end (section 10); a mix is lowered by the least amount to reach the peak limit within 0.1 dB (section 11); a `class` is not taken by `<tween>`, `<set>`, `<volume>` or `<style>`; non-finite numbers are refused (N9); a `<span>` does not take the place attributes.

### Open questions this text leaves to the owner

- Whether holding a video's last picture (N4) is wanted, or the case is refused.
- Whether `margin` on an absolutely placed item should act as in CSS (N10) and not be ignored.
- The support table (section 14): each entry, its recognition rule and its sprint.

