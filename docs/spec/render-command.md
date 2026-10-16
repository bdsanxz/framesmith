# The `render` command: a first composition to an MP4, drawn on the processor

Status: in force from Sprint 1, for the first form of `framesmith render`. Sources: `card-renderer.md` (the renderer on the card, the choice of `--backend gpu`, and the comparison of the two renderers: ticket T039); `notation.md` (section 3 for the project and the drawing order, section 4 for the value forms, section 5.1 for the frame count, sections 6.1 and 6.2 for the root and the box, section 7.1 for place and size, Q1, Q2 and N10 for the edges, the drawing order and `margin`, section 7.4 for the root's background, N6 for a size below 0, N9 for numbers); `check-command.md` and `project-location.md` (a project is opened as `check` opens it); `project-file.md` (the name and the place of the finished file, and the settings); `error-catalogue.md` (the codes, the kinds and the form of a report); `run-log.md`; decisions P2 and A19 of `DECISIONS.md` (the exit codes; A19 also for an option value outside its list), the ticket T038 of Sprint 1 (the options, the rule that an option wins over the project file and the file over the default, the summary of the settings used), the ticket T041 of Sprint 1 (the places and sizes of a box, section 4.1), the ticket T040 of Sprint 1 (the choice of the renderer by `auto`, `cpu` and `gpu`, section 1.2, and the refusal of `gpu` without a card) and `ROADMAP.md` line 1.48 (the option names), P5 (a file that exists is replaced only when a new render succeeds) and D22 (the encoder choices); `PLAN.md` section 3 (a frame is a pure function of the composition and the frame's number; the processor's renderer defines the picture; which crate holds what) and section 5.2 (the processor's renderer against its checked-in pictures: equal to the byte); `CLAUDE.md` rules 1, 2 and 5; ITU-T H.264 for the stream and ITU-R BT.709 for its colour marks (both through frx7: `frx7/docs/spec/pixel.md` and `frx7/docs/spec/os-codecs.md`). The proofs of concept's `native-render-pipeline.md` (a record read under decision B4) is the origin of the stages and their order, taken as behaviour and not as text.

This is the tracer through the whole product: the project is read, drawn on the processor, handed to frx7 and written as a file. It draws the root's background and `<box>` elements with a background colour, placed by their edges in pixels or percentages of their parent (section 4.1). Everything else is named and refused before the first frame (section 3), so that no wrong video is made in silence.

## 1. The command line

```
framesmith render <path> [--output <file>] [--backend <word>] [--quality <word>] [--encoder <word>] [--codec <word>] [--workers <value>]
```

- **One argument, the path of the project**, in the three forms `project-location.md` gives (a folder, a `framesmith.yaml`, a composition file), and any of the options below, before or after it, in any order.
- **The options.** Each sets the one setting of the project file it names, for this run only (section 1.1). An option is written as `--quality high` or as `--quality=high`; the word after an option that takes a value is its value, even when it begins with `-`.

| Option | The setting it replaces | Values taken |
|---|---|---|
| `--output <file>` | `render.output` | A path that is not empty |
| `--backend <word>` | `render.backend` | `auto`, `cpu`, `gpu` |
| `--quality <word>` | `render.quality` | `draft`, `standard`, `high` |
| `--encoder <word>` | `render.encoder` | `hardware`, `software` |
| `--codec <word>` | `render.codec` | `h264`, `hevc` |
| `--workers <value>` | `render.workers` | `auto`, or a whole number from 1 to 16 written in digits |

  The values are those of the project file's keys (`project-file.md` section 3), letter for letter. `--backend` chooses the renderer, by section 1.2: `cpu` draws on the processor, `gpu` draws on the graphics card (`card-renderer.md`), and `auto`, the default, draws on the card when one is there and on the processor when none is. What `--codec`, `--encoder`, `--quality` and `--workers` do to the film is stated by their own tickets (T084, T085, T096); a value this version does not apply is refused before the first frame (section 3). `--json`, `--strict`, `--debug` and settings by environment variable are not taken in this version. `render.audio-bitrate` has no option in this version (`ROADMAP.md` line 1.48 lists none): it is set in the project file.
- **A command line that cannot be read** (`FS-0017`): no path, or more than one path. Its remedy is about the path of the project.
- **An option that is not taken** (`FS-0037`, of the kind invalid): an option that is not in the table, an option with no value after it, an option given twice, and a value outside the values of its option. Its remedy is ``Give one of the options the message lists, with one of the values it names, or leave the option out.``, so that the author is sent to the option, not to the path. The message names what was written and the options or values taken:
  - ``--colour` is not an option of `render`; the options are: --output, --backend, --quality, --encoder, --codec, --workers``;
  - ``--quality` must be one of draft, standard, high, not ultra``;
  - ``--workers` must be auto or a whole number from 1 to 16, not 17``;
  - ``--output` must be a path that is not empty``;
  - ``--quality` needs a value: one of draft, standard, high``;
  - ``--quality` is given twice; give it once``.

  Both are told on standard error as `check` tells a command line it cannot read (`framesmith: ` and the statement of the code), followed by the usage line of `render`, which names every option and its values:

  ```
  usage: framesmith render <path> [--output <path>] [--backend auto|cpu|gpu] [--quality draft|standard|high] [--encoder hardware|software] [--codec h264|hevc] [--workers auto|1-16]
  ```

  then nothing is printed on standard output, the project is not opened, nothing is written, and the exit code is 1 (decision A19: a value outside its list is invalid and ends with 1). A command line with no command, or with a word that is not a command, is told with both usage lines, `usage: framesmith check <path>` and the usage line of `render` above, in that order.

### 1.1 Which value is used

Every setting is taken by one rule: **an option on the command line wins over the project file, and the project file wins over the default** (`project-file.md` section 1). An option replaces only the setting it names; every other setting keeps the value of the project file, or its default. The setting in use is what the rest of this spec means by the setting: what is refused (section 3), the place of the finished file (section 6), the sound's bit rate (section 5.2) and the summary (section 7). `framesmith.yaml` is read and never written: an option changes the run, not the file.

### 1.2 Which renderer draws

The renderer is chosen once, from `render.backend` (or `--backend`) as section 1.1 gives its value, before the film is opened and before the first frame. The choice is made in one place of the program, and nothing else chooses.

- **`cpu`** draws on the processor (`draw-cpu`). The card is not started.
- **`gpu`** draws on the graphics card (`card-renderer.md`). When this machine has no card that frx7 can draw with, or the card could not be started, `render` refuses the film: the report of `FS-0038` (section 7) and no file. A `gpu` is never drawn on the processor: Framesmith does not fall back from `gpu` to `cpu`.
- **`auto`**, the default, starts the card first. When the card starts, `auto` draws on it. When it does not start, `auto` draws on the processor, and the summary says so (section 7). So `auto` never refuses a film for want of a card.

The renderer that draws is named in the summary (section 7) and in the run log (`run-log.md` section 4), so that a choice Framesmith made is always shown. Choosing among several cards is not in this version.

## 2. What is read, and what is written when it is not drawn

- The project is opened by `Project::open`, the one reading `check` uses. When it has any report, or there is no project, the command prints exactly what `check` prints for it (the reports and the closing line, or the `No project` line), writes no file and makes no folder, and ends as `check` ends: with 1, or with 2 when every report is of the kind unsupported (decision P2).
- A project that opens is then looked at for what this version draws and applies (section 3). When anything in it is refused, every refusal is printed as a report, in the order of the files (the command line first, then the project file) and of their lines, then the closing line `Invalid: the project is not valid (composition.xml)` or, when every report is of the kind unsupported, `Unsupported: the project uses what this version does not support (composition.xml)`, with the files that have a report in the brackets. Nothing is written, no folder is made, and the exit code is 1 or 2 by the same rule. The refusals are listed by `render` and not by `check` in this version: the table of what is not supported (`notation.md` section 14) is built later and `check` lists from it, so a project that `check` calls ready can be refused by `render` until then.
- Only after both, the first frame is drawn. A failure after that point (section 5) leaves no file at the place of the finished file. The renderer is chosen before the first frame, and the card of `--backend gpu` or of `--backend auto` is started then (section 1.2): a `gpu` on a machine with no card that frx7 can draw with (`card-renderer.md` section 2) is refused with the report of `FS-0038` and the closing line of section 7, and no file is written. With `auto` the same machine draws on the processor, and the film is written.

## 3. What is drawn

The judgement of the form of the whole file (`notation.md` section 3) has been passed already. This section is about meaning: it says which elements, attributes and forms are drawn in this version. A form outside the table is refused; it is never repaired, read as something near, or ignored.

| Where | Drawn | Refused as unsupported (`FS-0035`) |
|---|---|---|
| The root `<composition>` | `id`, `version`, `width`, `height`, `fps`, `duration`, and `background` when it is one opaque colour: `#rrggbb`, or `#rrggbbaa` with `aa` equal to `ff` | every other attribute (`class`); a `background` with an alpha below `ff` |
| Elements in the root | `<box>` | every other element: `<style>`, `<font>`, `<audio>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>` |
| `<box>` | `id` (a name, not used), `left`, `top`, `right`, `bottom`, `width`, `height`, `position`, `margin`, `background`, `start` and `duration` (the window of section 4.2, written both or neither); a `<box>` inside a `<box>` (section 4.1); `flex-direction`, `justify-content`, `align-items`, `align-self`, `gap` and `padding`, which lay out the children of a box as a row or a column (section 4.3) | every other attribute (`max-width`, `flex-grow`, `columns` and the rest); every element inside a box other than a `<box>` (`<tween>`, `<set>`, `<rect>` and the rest) |
| `position` | `static` or `absolute`. Accepted: a box is placed by its edges whether or not it is written (`notation.md` section 7.1, Q2) | any other word, reported as FS-0039 |
| `margin` | one to four numbers, as the sides of `notation.md` section 4 (`4`, `4 8`, `4 8 4 8`). Accepted; it moves an item of a row or column (section 4.3) and has no effect on any other box, absolutely placed or not (N10) | a value that is not one to four numbers, reported as FS-0032 |
| `flex-direction`, `justify-content`, `align-items`, `align-self` | the words of section 4.3: `row` or `column`; `flex-start`, `center`, `flex-end` (and for `justify-content` `space-between`; for `align-items` and `align-self` `stretch`) | a word that is not one of them, reported as FS-0039 with the words it takes; `space-between` of `justify-content`, and `baseline` of `align-items` or `align-self`, which are not drawn in this version (FS-0035) |
| `gap`, `padding` | `gap`: a number of 0 or more, in pixels. `padding`: one to four numbers of 0 or more, as the sides of `notation.md` section 4 (section 4.3) | `gap` or a value of `padding` below 0, reported as FS-0034; a `padding` that is not one to four numbers, reported as FS-0032 |
| `left`, `top`, `right`, `bottom` | a whole number of pixels from -1000000 to 1000000, or a percentage of the parent's width (`left`, `right`) or height (`top`, `bottom`), worked out as section 4.1 says. A negative distance is taken. The box may lie partly or wholly outside its parent or the frame: the part inside the frame is seen. Not written: no edge; where `left` and `top` are both not written, the box starts at its parent's edge | a number with a fraction, such as `10.5`; a value beyond the limit (FS-0035, section 4.1) |
| `width`, `height` | a whole number of pixels from 0 to 1000000, or a percentage of the parent's width or height from `0%` up, worked out as section 4.1 says; 0 draws nothing of the box, and its children are still placed and drawn. Not written: the size of section 4.1 (a `<box>` is as large as its parent, or the space between two edges) | a number with a fraction, such as `10.5`; a value beyond the limit (FS-0035, section 4.1) |
| `background` of a box | one opaque colour, as for the root. Not written: the box draws nothing | an alpha below `ff`; a gradient; a list of layers; anything else that is not one colour |

The attributes `left`, `top`, `right`, `bottom`, `width` and `height` are lengths (`notation.md` section 4: a number of pixels, or a percentage, with a number read by N9). A value that is not a length (`abc`, `10px`), and a `width` or `height` below 0 (a percentage included), are not unsupported but wrong (N6) and are reported as invalid with `FS-0034`. A length in pixels that is not a whole number within the limit is unsupported; a percentage is taken with any value it has, and what it works out to is section 4.1. The attribute `margin` must be one to four numbers, and `position` one of its two words: a `margin` that is not is reported as invalid with `FS-0032`, and a `position` that is not with `FS-0039`. The words of `flex-direction`, `justify-content`, `align-items` and `align-self` must be among the words of section 4.3, reported as `FS-0039` with the words that are taken; `gap` must be 0 or more and `padding` must hold numbers of 0 or more, reported as `FS-0034` (`gap` as ``gap="-2" must be 0 or more``).

A setting in use (section 1.1) that this version does not apply is refused the same way: `render.quality` other than `standard`; `render.encoder` other than `hardware`; `render.codec` other than `h264`; `render.workers` other than `auto` or `1`; `render.audio-bitrate` outside 64 to 320; since the encoder is the system's own at `standard`, one thread draws and encodes, and the system's AAC encoder takes 64 to 320 kbit/s at 48 kHz in stereo (`frx7/docs/spec/os-codecs.md` section 3.1, measured), and a value that differs would be ignored in silence or fail after the drawing began. A value the project file writes is reported on the line of the value in `framesmith.yaml`. A value an option gives is reported with the place `command line` and no line: ``command line: FS-0035: unsupported: `--quality high` is not supported by this version, which takes standard. <remedy>``, and the closing line names `command line` among the files. A value of the project file that an option replaces is not in use and is not refused. A setting written with a value this version applies is accepted: `render.backend` takes `auto`, `cpu` and `gpu` (section 1). `name`, `composition`, `paths.renders` and `render.output` are applied: they choose the project's name, the composition and the place of the finished file (section 6).

The message of a report of `FS-0035` is one of:

- ``<text> is not supported by this version, which draws <box> only`` for an element in the root, with `<text>` as the notation writes it (``<composition src>`` for a composition shown from a file);
- ``<text> inside a <box> is not supported by this version, which draws a <box> inside a <box> and no other element`` for an element in a box;
- ``<text> has an attribute this version does not support: `border-radius` `` for an attribute, with `<text>` the element it is written on (`<composition>` for the root);
- ``` `left="10.5"` on <box> is not a form this version supports: a whole number of pixels from -1000000 to 1000000, or a percentage such as 50% ``` for a value, with `a whole number of pixels from 0 to 1000000, or a percentage such as 50%` for a size and `one opaque colour, #rrggbb` for a colour;
- ``` a place or a size of <box> works out beyond 1000000 pixels, which this version does not place ``` for a box whose edge or size lies beyond the limit (section 4.1);
- ``` `render.quality: high` is not supported by this version, which takes standard ``` for a setting of the project file, and ``` `--quality high` is not supported by this version, which takes standard ``` for an option, with what it takes after `takes` (`64 to 320` for the sound's bit rate).

The message of a report of `FS-0034` is ``` `left="abc"` is not a length: a number of pixels or a percentage, such as 40 or 50% ``` or ``` `width="-5"` is below 0; a size is 0 or more ```. The message of a report of `FS-0032` is ``` `margin="10%"` is not one to four numbers, such as 4 or 4 8 ```, and of `FS-0039` is ``` `position="fixed"` is not one of: static, absolute ```.

A report is on the line of the element, of the attribute or of the value it names. An element that is reported is not looked into: what is inside it is not reported.

## 4. The picture

A frame is a picture of `width` by `height` pixels, each pixel four bytes in the order red, green, blue, alpha, rows from the top, no padding between rows. It is drawn as follows, and the result is the correct picture of this version:

1. Every pixel is the root's background (black, `#000000`, when it is not written) with alpha 255.
2. The boxes are drawn in document order, a parent before its children, each over what is there: a box with a `background` replaces every pixel with `left <= x < left + width` and `top <= y < top + height`, the rectangle of section 4.1, that lies inside the frame by the box's colour, alpha 255. A box written later covers one written earlier where they overlap, and a box inside a box is drawn over the box that holds it (`notation.md` section 3, drawing order; Q2). A box with no `background`, or with a size of 0, changes nothing. A box is drawn in the frames of its window (section 4.2) and in no other frame; in a frame outside its window it is not drawn at all, and its children with it.

There is no smoothing: a box's edge is a whole pixel boundary, since every edge is worked out to whole pixels (section 4.1).

**A frame is a pure function of the composition and the frame's number.** In this version nothing moves: a box appears and disappears only with its window (section 4.2), and every other part of the picture is the same in every frame. The picture of a frame never depends on the frames drawn before it, on the order they are asked for, on the clock, or on the number of workers. A frame number at or past the frame count is not a frame; it is refused when asked for.

The renderer on the card (`--backend gpu`, and `--backend auto` when a card is there: section 1.2) draws the same display list, and its picture must agree with this one within the tolerance of `card-renderer.md` section 4. The picture of the processor is the correct one; the card's is checked against it on every frame of the fixture, and is not checked in.

### 4.1 The place and the size of a box

The place and the size of every `<box>` are worked out once, before the first frame, and are the same in every frame (`PLAN.md`, section 3). The rules are those of `notation.md` section 7.1, with Q1 (`bottom` and `right` with a size), Q2 (a box counted from its parent's outer edges, drawn in document order) and N10 (`margin`, which has no effect here). The parent of a box in the root is the frame. The parent of a box inside a box is that box, and its rectangle is the one worked out here: a parent is worked out before its children.

Each axis is worked out on its own: across, from `left`, `right` and `width`, with the parent's width; down, from `top`, `bottom` and `height`, with the parent's height. Let L be the parent's width or height in pixels. An amount is a number of pixels, as written, or a percentage p of L, which is p times L divided by 100 pixels, exactly and not rounded.

1. **Size.** A written `width` or `height` is its amount. Otherwise, with both edges of the axis written (`left` and `right`, or `top` and `bottom`), the size is L less the two distances. Otherwise it is L: a box with no size is as large as its parent, and a `left` or a `top` alone does not reduce it. A size that comes out below 0 from the two distances is 0. A size written below 0 is reported (N6).
2. **Start.** A written `left` or `top` is the distance of the near edge from the parent's near edge. With no near edge and a far edge written (`right` or `bottom`), the far edge lies that distance from the parent's far edge, and the near edge is L less that distance less the size. With neither, the near edge is the parent's near edge. A written near edge is used even when a far edge is written too, and a written size is kept even when a far edge is written (Q1).
3. **Whole pixels.** Exactly, the near edge a is the parent's near edge plus the start, and the far edge b is a plus the size. Each of a and b is rounded to a whole pixel, a half up: the whole number nearest to it, and a value of exactly half a pixel goes to the larger whole number. The box covers the pixels from round(a) up to round(b) less one, so its size is round(b) less round(a), and two boxes that meet share one edge. A box of size 0 covers no pixel. Example: in a frame 96 pixels wide, `left="10%" width="50%"` gives a = 9.6 and b = 57.6, which round to 10 and 58, so the box covers the columns 10 to 57, 48 pixels; in a parent 99 pixels wide, `width="50%"` gives 49.5, which rounds to 50.
4. **Nesting.** A box inside a box is placed in the same way, from its parent's rectangle as rule 3 worked it out: L is the parent's width or height in whole pixels, and its near edge is the parent's whole-pixel edge. A box with `left="10"` inside a parent at `left="20"` lies at 30. Moving the parent moves the box with it. A child of a box with `flex-direction` is placed by the rows and columns of section 4.3 instead: its rectangle is worked out with its siblings, not by its own edges alone.
5. **Zero and negative.** A `width` or `height` of 0, or of `0%`, draws nothing of the box, and its children are still placed and drawn (N6). A `width` or `height` below 0 is reported with `FS-0034` (section 3) and the box is not drawn.
6. **Limit.** When a rounded edge, or the size, lies beyond 1000000 pixels, or an amount cannot be worked out as a finite number, the box is not placed and is reported with `FS-0035` (section 3).

**Settled by.** T041 of Sprint 1 (brigade order t041): the whole-pixel rule of 3, the size of 0 for a size below 0 that comes from two distances (1) and the limit of 6 are not in `notation.md`. They are a draft for the owner to confirm; the report of the order says so.

### 4.2 The window of a box

A `<box>` written with `start` and `duration` (both, or neither: `notation.md` section 5.2) is drawn only in the frames of its window. A `<box>` written with neither is drawn in every frame of its parent's life. The window is worked out once, before the first frame, by the frame-grid rule of `frame-time.md`:

1. **Origin.** A `start` counts from the start of the nearest clip around the element: the nearest parent that has both `start` and `duration`. With no clip around it, it counts from the start of the composition, which is time 0. An element between the clip and the box that has neither `start` nor `duration` passes the origin on unchanged. The start of a clip is its origin plus its written `start`, added exactly; the origin of everything inside that clip is that start.
2. **Window.** For a box with `start` and `duration`, let `s` be its start (its origin plus its `start`) and `e` be `s` plus its `duration`, both exact. Its window is the frames `i` with `F(s) <= i < F(e)`, where `F` is the frame-grid rule. A `duration` of 0 gives an empty window: the box is not drawn.
3. **Within the parent.** A box's window is its own window cut to its parent's: a frame is in the window only when it is in both. A box with neither `start` nor `duration` has its parent's window. The root's window is every frame of the composition. So a child is never drawn outside its parent's window, whatever its own window says.
4. **Place.** A window changes no place: the place of section 4.1 is the same in every frame, and a box outside its window keeps its place.

At 30 frames a second, a clip from 1 s for 2 s is drawn on frames 30 to 89. A box inside it with `start="0.5"` begins at 1.5 s and first shows on frame 45. A box inside it with neither `start` nor `duration` shows on frames 30 to 89 as well. A box with `start="0.33333"` and one with `start="0.33334"` both first show on frame 10; one with `start="0.34"` first shows on frame 11.

**Settled by.** T042 of Sprint 1 (brigade order t042): the origin of rule 1 (the nearest clip, passed through an element that has neither), the cut of rule 3 to the parent's window, and the empty window of a duration of 0 are not in `notation.md` in these words; section 5.2 names only the parent clip. They are a draft for the owner to confirm; the report of the order says so.

### 4.3 Rows and columns

A `<box>` with `flex-direction` lays out its children as a row (`row`) or a column (`column`). Its children are the `<box>` elements it holds, in document order. A `<box>` without `flex-direction` places its children by their edges (section 4.1). The values of `justify-content`, `align-items`, `align-self`, `gap` and `padding` are judged on every `<box>`, and they have an effect only on a box with `flex-direction` (rule 10).

Along the **main axis** of a row, which is across, the width counts; of a column, down, the height. The **cross axis** is the other one. Each item's **outer size** is its size on the main axis plus its `margin` on the two sides of that axis. The **content box** of a container is its rectangle less its `padding`.

1. **The container's size.** A row or column is placed by section 4.1 like any box, with one difference: an axis on which it has no size of its own (no written size, and not both edges) is as large as its items. On the main axis it is the sum of the items' outer sizes, the gaps between them and the padding on that axis. On the cross axis it is the largest of the items' outer sizes across, and the padding on that axis. With no items it is the padding alone. A container with a size of its own keeps it.
2. **Items.** The items are the `<box>` children of the container, in document order, each one that is not `position="absolute"` (rule 6). An item takes no notice of its own `left`, `top`, `right` and `bottom`. There is no wrapping and no shrinking: an item keeps its size, and the items are never cut or moved to a new line.
3. **Size of an item.** On the main axis an item is its written `width` (row) or `height` (column). A percentage is of the container's size on that axis, when the container has a size of its own there (section 7.1); otherwise a percentage counts as 0. An item that writes no size on the main axis is 0 there. On the cross axis an item is its written size there, with a percentage resolved as on the main axis; without one it is 0, unless rule 7 stretches it.
4. **Gap.** `gap` (default `0`) is the space between two neighbouring items: none before the first and none after the last. A gap of 0 leaves the items touching, and draws no gap. A gap below 0 is reported (`FS-0034`, section 3) and nothing is drawn. Absolute items take no room and are not counted in the gaps (rule 6).
5. **Along the direction.** `justify-content` (default `flex-start`) places the group of items, from the first item's outer edge to the last item's, with the gaps between them. `flex-start` puts the group at the start edge of the content box, `flex-end` at its end edge, and `center` leaves the same room on both sides. Room that is not used is not shared between items: that is `space-between`, which is not drawn in this version. A group larger than the content box is placed as the value says and runs over the edges, as an absolute item may.
6. **Absolute items.** An item with `position="absolute"` takes no room, is not counted in the gaps, and does not move the others: the other items close up. It is placed by its own `left`, `top`, `right`, `bottom`, `width` and `height` as section 4.1 says, counted from the container's outer edges and not inside its padding (Q2), and it is drawn in document order with its siblings (Q2). Its `margin` has no effect (N10).
7. **Across.** `align-items` (default `stretch`), or `align-self` on one item, places each item across the content box, with its margins on the two sides across. `flex-start` puts the item at the start edge, `flex-end` at the end edge and `center` in the middle. `stretch` sizes an item that writes no size across to the content box across, but only when the container has a size of its own across; an item that writes a size keeps it and is placed at the start edge. Without a container size across, `stretch` gives an item that writes no size 0 across. `baseline` is not drawn in this version (`FS-0035`).
8. **Padding.** `padding` (default `0`) is one to four numbers, read as the sides of section 4: `4` for all four sides, `4 8` for the top and bottom and then the left and right, `4 8 2` for the top, then the sides, then the bottom, `4 8 2 6` for the top, right, bottom and left. The items are placed inside the content box, so a padding moves them in from the box's edge by its amount on each side written. A padding below 0 is reported (`FS-0034`).
9. **Items' edges.** Each item's edges are worked out as section 4.1 rule 3 says: exactly, the item's edges on each axis are placed as above, then each is rounded a half up to a whole pixel, so that two neighbours with a gap of 0 meet on one pixel edge.
10. **Values.** `flex-direction` is `row` or `column`. `justify-content` is `flex-start`, `center`, `flex-end` or `space-between`. `align-items` and `align-self` are `flex-start`, `center`, `flex-end`, `stretch` or `baseline`. A word that is not one of these is reported (`FS-0039`, section 3). The words `space-between` and `baseline`, and the attributes `flex-grow`, `max-width` and `columns`, are not drawn in this version (`FS-0035`). On a `<box>` without `flex-direction`, `justify-content`, `align-items`, `align-self`, `gap` and `padding` are judged for their form and have no effect.
11. **Clips.** A child with `start` and `duration` takes its room in the row or column during the whole composition, and it is only not drawn outside its window (Q3, section 4.2). The place of an item is the same in every frame.

The sources of these rules are `notation.md` sections 7.1 and 7.2, with N6 for the zero and the negative values, N10 for `margin`, and Q2 and Q3 for absolute items and clips. The proofs' reference of the same rules, `native-notation-reference.md` section 4.3 in the proofs' tree, was read for the behaviour only.

**Settled by.** The rules that the sources do not state in these words are a maintainer draft written under this ticket (T043 of Sprint 1, brigade order t043), for the owner to confirm: the item sizes of rule 3 (an item with no size is 0 on the main axis, and on the cross axis unless stretched); the container size of rule 1, which counts the items' margins; the stretch of rule 7, which needs the container's own size across and leaves an item with its own size where it is; the padding inside the container size of rule 8; and the room of a group that is larger than its container (rule 5). The report of the order says so.

## 5. The film

The frames, numbered 0 to the frame count less 1 (`notation.md` section 5.1), are drawn in order on the processor, brought to the form the encoder takes and handed to frx7, which encodes them and writes the file. Nothing else is started.

- **Colour.** A frame is converted from red, green and blue to luma and chroma in 4:2:0, 8 bits, with the matrix of BT.709 and the narrow range, by frx7's own arithmetic (`pixel::convert`, `frx7/docs/spec/pixel.md`), and the file is marked BT.709. Alpha is not used: a finished frame is opaque.
- **The stream.** H.264, 8 bits, the composition's width and height, the composition's frame rate (a whole number of frames a second), through the system's own encoder, hardware first when the machine has one (decision D22), at a quality of 0.65 on a scale from 0 to 1, which is what `render.quality: standard` asks for, with a key picture at least every two seconds and pictures that may be encoded out of the order they are shown in.
- **The file.** An MP4 with one video track and one sound track (section 5.2), written by frx7's MP4 writer. It is written at its place; staging of the output, so that an earlier file is replaced only when the new render succeeds (decision P5), comes with its own ticket. A render that fails removes the file it began.
- **One thread draws and encodes, frame after frame.** Several workers, and drawing and encoding at the same time, are later tickets.
- **Exactly the frames the length calls for.** The track holds exactly the frame count of `notation.md` section 5.1: the root's `duration` times its `fps`, taken as the whole number when it is within 0.001 of one and rounded up otherwise, at least 1. No frame is added at the end and none is dropped. Five seconds give 120 frames at `fps="24"`, 150 at `fps="30"` and 300 at `fps="60"`; `duration="1.01"` at 30 gives 31 (30.3 is rounded up) and `duration="1.00001"` at 30 gives 30 (30.0003 is within 0.001 of 30). The track's frame rate is the root's `fps` (30 when it is not written), as a whole number of frames a second over 1. The count `check` prints (`check-command.md`), the `frames` line of the summary (section 7) and the number of pictures in the file are the same number, because all three are the one frame count of the scene.

### 5.1 The form of the finished file

What a player needs to show the film correctly and to start it before it is fully loaded. Sources: ITU-T H.264 (the sequence parameter set: `bit_depth_luma_minus8`, `chroma_format_idc`, and the colour description of its VUI, Annex E); ITU-T H.273 Table 2 to 4 (the code point 1 for BT.709 primaries, transfer and matrix); ITU-R BT.709-6 (the narrow range: luma from 16 to 235, chroma from 16 to 240); ISO/IEC 14496-12 (the top-level boxes `ftyp`, `moov` and `mdat`, the sample table and its sync samples, the composition time offsets); decision D22 of `DECISIONS.md` (a key picture at least every two seconds, reordering); decisions A10 (the standard player) and A29 (a tolerance the ticket calls stated is stated here); decision B6 (whether pictures are reordered is read from the sample times when the headers are silent); `frx7/docs/spec/os-codecs.md` (key distance in time, and what the system's encoder was measured to keep of flat pictures); `frx7/docs/spec/pixel.md` section 12.1 (the worked values of white, black and mid grey). The proofs of concept's `native-render-pipeline.md` (a record, decision B4) asked for the same form, taken as behaviour.

- **The stream.** H.264 whose own headers say 8 bits of luma and of chroma, chroma in 4:2:0, not interlaced, and the colour description of BT.709: primaries 1, transfer 1, matrix 1, narrow (limited) range. The track of the file carries the same marks. `frx7 probe` shows them on the track's line and on its `headers:` line, read by frx7's own code.
- **The tables in front.** The file's top-level boxes are, in this order, `ftyp`, `moov` (the tables: what the samples are and where they lie), and then `mdat` (the media). A box of free room may lie between `moov` and `mdat`. No `moov` follows the media. A player can therefore read the tables and start before the file is fully loaded. frx7's MP4 writer keeps room for the tables in front because the render tells it, before the first picture, how many pictures the track will hold (the frame count of section 5).
- **Key pictures.** The first picture of the film is a key picture (a sync sample, at which decoding may start). The encoder is asked for at most `key_distance_within(2.0, fps)` frames from one key picture to the next: the most whole frames that fit in two seconds (48 at 24, 60 at 30, 120 at 60). In the written file no two key pictures that follow one another are further apart than two seconds in the times they are shown; the encoder may put a key picture sooner.
- **Reordering.** Pictures may be encoded out of the order they are shown in (B frames), and they are: in the written file some picture is shown later than a picture decoded after it. The system's encoder writes no statement of reordering in the headers, so `frx7 probe` gives the answer from the sample times and says so (`pictures reordered (from the sample times)`, decision B6).
- **Plays in the standard player.** The file opens in the system's media framework, which says it is playable, and every picture decodes to the end, in the order of increasing times, as many pictures as the frame count. This scripted decode stands for the standard player (decision A10).
- **Flat colours read back.** The conversion of section 5 gives, for an opaque colour with red, green and blue equal, Cb and Cr of 128 and the luma of BT.709's narrow range: white `#ffffff` gives Y 235, black `#000000` gives Y 16, and mid grey `#808080` gives Y 126 (16 + 219 x 128 / 255 = 125.93, rounded). **Stated tolerance (decision A29):** inside a flat box whose edges lie on the 16-pixel grid of the encoder's blocks (its `left`, `top`, `width` and `height` are multiples of 16), at least 4 pixels from its edges, in a film of at least 96 by 64 pixels, every luma and every chroma sample of every picture decoded from the file is within **2 levels** of those values. Source: `frx7/docs/spec/os-codecs.md`, "What the software H.264 encoder was seen to do": from 96 by 64 up every flat picture was within two levels of what was asked with the encoder on the processor, and within one with the hardware encoder. The 4 pixels keep the edge of a box, where a lossy encoder spreads one colour into the next, out of the measure. A box off the grid shares a block with its surroundings, and there the encoder spreads the colour of the surroundings further in: measured with boxes of 32 by 32 at `top="38"` on a canvas of `#204060`, Cb lay up to 4 levels off in the samples 4 and 5 pixels inside the box's lower edge, in some pictures of a film of one second at 30 frames a second, so the tolerance is stated for boxes on the grid only. On the grid the most was 1 level, for each of the three boxes, over every picture of that film (`crates/render/tests/standard_mp4_form.rs`).

A failure of frx7 or of the machine (the encoder refuses the stream, the file cannot be written, the folder cannot be made) ends the run with `FS-0036`: `FS-0036: failure: the video could not be made: <reason>. <remedy>`, then the line `Failed: the video could not be made`, and the exit code 1.

### 5.2 The sound track

Every film holds exactly one sound track, whether the composition has sound or not: players and editors treat a file with no sound track differently from a file whose sound is silent. Sources: ISO/IEC 14496-3 (AAC-LC, packets of 1,024 sample frames); ISO/IEC 14496-12, clauses 8.6.5 and 8.6.6 (the edit list, which leaves the encoder's lead-in out and ends the track at the last real sample); `frx7/docs/spec/os-codecs.md` section 3.1 (AAC through the system's encoder at a constant bit rate, its lead-in, its read-back to the sample, and the bit rate it was measured to keep); `frx7/docs/spec/mp4.md` section 2.2; decision A29 of `DECISIONS.md` (the AAC bit-rate margin) and A10 (the standard player). The proofs of concept's `native-render-pipeline.md` (a record, decision B4) gave the mix as 48,000 samples a second in two channels, as long as the film to the sample, taken as behaviour.

- **The mix.** The `mix` crate makes the sound of the film: 48,000 sample frames a second, two channels (left, then right), 32-bit floating point. This version places no sound (placing sounds is a later ticket), so every sample of the mix is 0, silence.
- **Its length.** The picture's length in seconds, the frame count over the frame rate (section 5), times 48,000, rounded to the nearest sample frame, a half up. At a frame rate that divides 48,000 (24, 30, 60 and the like) it is exact: 5 seconds at 24, 30 and 60 frames a second are 240,000 sample frames; 31 frames at 30 are 49,600.
- **Handed over with the frames.** After each frame is handed to the picture encoder, the mix up to the end of that frame (the length of frames 0 to that one, by the rule above) is handed to the system's AAC encoder through frx7's `codec` interface, and its packets are written into the same file, so that picture and sound lie side by side in it.
- **The form.** AAC-LC, 48 kHz, two channels, at a constant bit rate: `render.audio-bitrate` in kbit/s (1 kbit/s is 1,000 bits a second), **192 kbit/s** when it is not written (section 1.1; values outside 64 to 320 are refused before the first frame, section 3). The track's timescale is 48,000; frx7's writer marks the encoder's lead-in in the edit list, so that the track's length, and the sound read back from it, is the mix's length to the sample. `frx7 probe` shows it as a `sound, aac, 48000 Hz, 2 channels` track with the sample entry `mp4a`.
- **Silence read back.** Every sample of a silent mix that is read back from the track through frx7's sound reader is 0.
- **The bit rate (stated, decision A29).** The track's bit rate, the bytes of all its packets over its length (as `frx7 probe` gives it), is **within 10 % of the rate asked for**: from 172,800 to 211,200 bits a second at the default 192 kbit/s, from 115,200 to 140,800 at 128 kbit/s. Measured on a silent track of renders at 30 frames a second: about 196 kbit/s for 2 seconds and within the margin for 1 and 10 seconds (`crates/render/tests/silent_sound_track.rs`); frx7 measured about 193 kbit/s over ten seconds and about 200 kbit/s over one, where the packets of the lead-in weigh more.
- **Plays.** The system's media framework opens the file as playable, decodes every picture to the end and reads the sound track to its end (decision A10).
- **A refusal.** The AAC encoder is started before the file is made; an encoder that refuses ends the run with `FS-0036` as below, and no file is left.

## 6. The place of the finished file, and what the author's files are

- The finished file is, by the rule of section 1.1: the path of `--output`; else `render.output` of the project file; else `<paths.renders>/<name>.mp4`, with `paths.renders` (default `renders`) and `name` (default the folder's name) as `project-file.md` sections 3 and 6 say. A relative path, the option's included, is taken from the project folder, never from the folder the command is run from (`project-location.md` section 4); a path that begins with `/` is taken as it is. The folder it is to be in is made when it is missing, with every folder above it that is missing, and is left in place when the render then fails.
- **The author's files are not touched.** The composition, the project file and every other file of the project have the same bytes after the run as before. Options change nothing in them: `framesmith.yaml` is never written. The things a run adds are the finished file, its folder when it was missing, and the run log under the folder the command is run from (`run-log.md` section 1).

## 7. The summary

A render that completes prints, on standard output, these lines, in this order, and ends with the exit code 0:

```
Rendered demo/renders/demo.mp4
frames: 150
size: 1280x720
frame rate: 30 fps
length: 5.000 s
renderer: processor
output: renders/demo.mp4 (default)
backend: cpu (option)
quality: standard (project file)
encoder: hardware (default)
codec: h264 (default)
workers: auto (default)
audio bitrate: 128 kbit/s (project file)
elapsed: 1.234 s
```

- The first line is the word `Rendered` and the path of the finished file: the project folder as it was written on the command line (or the folder found from the file written there), joined with the path of the file. `framesmith render demo` prints `demo/renders/demo.mp4`.
- `frames` is the number of frames written; `size` is `width` by `height`; `frame rate` is `fps` of the root; `length` is the frames divided by the frame rate, in seconds with three decimals; `renderer` is the renderer that drew: `processor` for `cpu`, `card` for `gpu`; for `auto` it is `card (chosen by auto)` when the card started, and `processor (chosen by auto: no suitable card)` when none did (section 1.2; the names are those of `card-renderer.md` section 3); `elapsed` is the seconds the run took from the start of the drawing to the end of the file, with three decimals.
- **The settings used**, one line each, between `renderer` and `elapsed`, in the order above: `output` (the path of the finished file as it is written, before it is taken from the project folder), `backend`, `quality`, `encoder`, `codec`, `workers` and `audio bitrate` (in kbit/s). Each shows the value in use (section 1.1) and, in brackets, where it came from: `option` when an option gave it, `project file` when `framesmith.yaml` wrote it, `default` otherwise. A setting an option changed shows the option's value. `output` is `project file` when the project file wrote `render.output`, `paths.renders` or `name`, the keys it is made from.
- Nothing is printed on standard error.

| Outcome | Standard output | Exit code |
|---|---|---|
| The film is written | The summary above | 0 |
| The project has a report, or there is no project | What `check` prints (section 2) | 1 |
| Every report is of the kind unsupported (the reader's, or `FS-0035`) | The reports and the `Unsupported:` line | 2 |
| A report of `FS-0034`, or of another invalid kind, with or without reports of the kind unsupported | The reports and the `Invalid:` line | 1 |
| The film could not be made | The report of `FS-0036`, then the line `Failed: the video could not be made` | 1 |
| `gpu` is asked for, and no card that frx7 can draw with is there (before the first frame, no file) | The report of `FS-0038`, then the line `Refused: the video was not made, because the graphics card that `gpu` needs is not available` | 2 |
| The command line cannot be read | Nothing (standard error: `check-command.md` section 1) | 1 |

The report of `FS-0038` carries the reason the card gave, which frx7 gives (for example, that this machine has no graphics card frx7 can draw with), and the remedy of the catalogue: the setting to change, `render.backend` or `--backend`, to `auto` or `cpu` (`error-catalogue.md` section 4). Its outcome in the run log is `refused` (`run-log.md` section 4).

## 8. A frame on request

`render` draws frames in order. Any single frame can also be drawn on request, with no command for it: the render crate's `Scene` holds a composition ready to draw, and `Scene::picture(number, renderer)` gives the picture of one frame, for the tests of this version and for the preview later (Sprint 3 brings the command). A frame asked for is the picture `render` draws for that number, whatever was asked for before: frames 0, 7 and the last, in that order or in reverse, give the same pictures. A number at or past the frame count is refused, naming the number and the last frame.

## 9. The checked-in pictures

The pictures that define the correct output of this version are checked in under `fixtures/` as PNG files of 8 bits and four channels, one folder for each fixture composition: `fixtures/<name>/composition.xml` and `fixtures/<name>/expected/frame-NNNN.png`, with `NNNN` the frame's number in four digits. The test draws the frames that have a picture on the processor and compares them with the pictures to the byte, pixel by pixel; a pixel that differs fails the test and the message names the frame's number and the first pixel that differs, with both values. A picture is made from the fixture with the example `crates/render/examples/write_expected_pictures.rs` and is read before it is checked in. The test reads the fixtures from `fixtures/`, or from the folder the environment variable `FRAMESMITH_FIXTURES` names, so that a check can show that a picture which is not the checked-in one fails the test. The example `crates/render/examples/read_back_film.rs` (development only) decodes a written film with frx7 and puts chosen frames on one labelled sheet, with the checked-in pictures under them, for the evidence of a ticket.

## 10. The log

The lines of a `render` are in `run-log.md` section 4. They hold counts, names of stages and codes, never the project's content, its paths or the words of a report.

## 11. Where the code lives

- `notation` reads the tree of the file (the elements in document order, with their attributes and lines) and the value forms a drawing part reads (a length, a colour).
- `project` holds the project's tree, and the lines of the render settings the project file wrote.
- `layout` places a box: its rectangle in its parent's rectangle (the frame, for a box of the root) from the written `left`, `top`, `right`, `bottom`, `width` and `height` (section 4.1).
- `draw` holds the list of drawing steps and the trait both renderers implement; `draw-cpu` is the renderer on the processor and `draw-gpu` the renderer on the card (`card-renderer.md`), which the command starts for `--backend gpu`, and for `--backend auto` to see whether a card is there (section 1.2).
- `mix` makes the sound of the film (section 5.2): its rate, its channels, its length and, in this version, its silence.
- `timeline` makes the scene from a project (and refuses what is not drawn) and draws any frame of it on request (`timeline.md`); `render` takes that scene and writes the film through frx7. For the forms of a box's place, size, margin and position, `timeline` also gives `check` its reports (`place_reports`, `check-command.md` section 3b), so the two commands judge them by one code.
- `project` also judges the value of each option (`Options`) and takes the settings by the rule of section 1.1 (`Project::apply`, `Settings::source`).
- `cli` is the command: it reads the arguments (the path and the words of the options), calls `project` and `render`, prints, and sets the exit code.
