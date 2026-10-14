# The `render` command: a first composition to an MP4, drawn on the processor

Status: in force from Sprint 1, for the first form of `framesmith render`. Sources: `notation.md` (section 3 for the project and the drawing order, section 4 for the value forms, section 5.1 for the frame count, sections 6.1 and 6.2 for the root and the box, section 7.1 for place and size, section 7.4 for the root's background, N6 for a size below 0, N9 for numbers); `check-command.md` and `project-location.md` (a project is opened as `check` opens it); `project-file.md` (the name and the place of the finished file, and the settings); `error-catalogue.md` (the codes, the kinds and the form of a report); `run-log.md`; decisions P2 and A19 of `DECISIONS.md` (the exit codes), P5 (a file that exists is replaced only when a new render succeeds) and D22 (the encoder choices); `PLAN.md` section 3 (a frame is a pure function of the composition and the frame's number; the processor's renderer defines the picture; which crate holds what) and section 5.2 (the processor's renderer against its checked-in pictures: equal to the byte); `CLAUDE.md` rules 1, 2 and 5; ITU-T H.264 for the stream and ITU-R BT.709 for its colour marks (both through frx7: `frx7/docs/spec/pixel.md` and `frx7/docs/spec/os-codecs.md`). The proofs of concept's `native-render-pipeline.md` (a record read under decision B4) is the origin of the stages and their order, taken as behaviour and not as text.

This is the tracer through the whole product: the project is read, drawn on the processor, handed to frx7 and written as a file. It draws the root's background and `<box>` elements with a background colour, placed in pixels. Everything else is named and refused before the first frame (section 3), so that no wrong video is made in silence.

## 1. The command line

```
framesmith render <path>
```

- **One argument, the path of the project**, in the three forms `project-location.md` gives (a folder, a `framesmith.yaml`, a composition file). No option is taken in this version: the options of `render` come with the tickets for the settings.
- **Anything else is a command line that cannot be read** (`FS-0017`): no path, more than one word, an option. It is told on standard error as `check` tells it, with the usage line `usage: framesmith render <path>`; nothing is printed on standard output, nothing is written, and the exit code is 1. A command line with no command, or with a word that is not a command, is told with both usage lines, `usage: framesmith check <path>` and `usage: framesmith render <path>`, in that order.

## 2. What is read, and what is written when it is not drawn

- The project is opened by `Project::open`, the one reading `check` uses. When it has any report, or there is no project, the command prints exactly what `check` prints for it (the reports and the closing line, or the `No project` line), writes no file and makes no folder, and ends as `check` ends: with 1, or with 2 when every report is of the kind unsupported (decision P2).
- A project that opens is then looked at for what this version draws and applies (section 3). When anything in it is refused, every refusal is printed as a report, in the order of the files (the project file first) and of their lines, then the closing line `Invalid: the project is not valid (composition.xml)` or, when every report is of the kind unsupported, `Unsupported: the project uses what this version does not support (composition.xml)`, with the files that have a report in the brackets. Nothing is written, no folder is made, and the exit code is 1 or 2 by the same rule. The refusals are listed by `render` and not by `check` in this version: the table of what is not supported (`notation.md` section 14) is built later and `check` lists from it, so a project that `check` calls ready can be refused by `render` until then.
- Only after both, the first frame is drawn. A failure after that point (section 5) leaves no file at the place of the finished file.

## 3. What is drawn

The judgement of the form of the whole file (`notation.md` section 3) has been passed already. This section is about meaning: it says which elements, attributes and forms are drawn in this version. A form outside the table is refused; it is never repaired, read as something near, or ignored.

| Where | Drawn | Refused as unsupported (`FS-0035`) |
|---|---|---|
| The root `<composition>` | `id`, `version`, `width`, `height`, `fps`, `duration`, and `background` when it is one opaque colour: `#rrggbb`, or `#rrggbbaa` with `aa` equal to `ff` | every other attribute (`class`); a `background` with an alpha below `ff` |
| Elements in the root | `<box>` | every other element: `<style>`, `<font>`, `<audio>`, `<rect>`, `<text>`, `<image>`, `<video>`, `<shape>`, `<composition src>` |
| `<box>` | `id` (a name, not used), `left`, `top`, `width`, `height`, `background` | every other attribute; every element inside a box (`<box>`, `<tween>`, `<set>` and the rest) |
| `left`, `top` | a whole number of pixels from -1000000 to 1000000. The box may lie partly or wholly outside the frame: the part inside it is seen. Not written: 0 | a number with a fraction; a percentage; a number beyond the limit |
| `width`, `height` | a whole number of pixels from 0 to 1000000; 0 draws nothing of the box. Not written: the size of the frame (`notation.md` section 7.1: a `<box>` is as large as its parent, and the parent of a box in the root is the frame) | a number with a fraction; a percentage; a number beyond the limit |
| `background` of a box | one opaque colour, as for the root. Not written: the box draws nothing | an alpha below `ff`; a gradient; a list of layers; anything else that is not one colour |

The attributes `left`, `top`, `width` and `height` are lengths (`notation.md` section 4: a number of pixels, or a percentage, with a number read by N9). A value that is not a length (`abc`, `10px`), and a `width` or `height` below 0 (a percentage included), are not unsupported but wrong (N6) and are reported as invalid with `FS-0034`. A length that is a length and is not a whole number of pixels within the limit is unsupported.

A project file (`project-file.md`) that writes a setting this version does not apply is refused the same way, with the line of the value: `render.backend` other than `auto` or `cpu`; `render.quality` other than `standard`; `render.encoder` other than `hardware`; `render.codec` other than `h264`; `render.workers` other than `auto` or `1`; `render.audio-bitrate` other than `192`; since the renderer is the processor's, the encoder the system's own at `standard`, one thread draws and encodes and there is no sound, and a value that differs would be ignored in silence. A key written with a value this version applies is accepted. `name`, `composition`, `paths.renders` and `render.output` are applied: they choose the project's name, the composition and the place of the finished file (section 6).

The message of a report of `FS-0035` is one of:

- ``<text> is not supported by this version, which draws <box> only`` for an element in the root, with `<text>` as the notation writes it (``<composition src>`` for a composition shown from a file);
- ``<text> inside a <box> is not supported by this version, which draws a <box> with no element inside it`` for an element in a box;
- ``<text> has an attribute this version does not support: `border-radius` `` for an attribute, with `<text>` the element it is written on (`<composition>` for the root);
- ``` `left="10.5"` on <box> is not a form this version supports: a whole number of pixels from -1000000 to 1000000 ``` for a value, with `a whole number of pixels from 0 to 1000000` for a size and `one opaque colour, #rrggbb` for a colour;
- ``` `render.quality: high` is not supported by this version, which takes standard ``` for a setting, with what it takes after `takes`.

The message of a report of `FS-0034` is ``` `left="abc"` is not a length: a number of pixels or a percentage, such as 40 or 50% ``` or ``` `width="-5"` is below 0; a size is 0 or more ```.

A report is on the line of the element, of the attribute or of the value it names. An element that is reported is not looked into: what is inside it is not reported.

## 4. The picture

A frame is a picture of `width` by `height` pixels, each pixel four bytes in the order red, green, blue, alpha, rows from the top, no padding between rows. It is drawn as follows, and the result is the correct picture of this version:

1. Every pixel is the root's background (black, `#000000`, when it is not written) with alpha 255.
2. The boxes are drawn in document order, each over what is there: a box with a `background` replaces every pixel with `left <= x < left + width` and `top <= y < top + height` that lies inside the frame by the box's colour, alpha 255. A box written later covers one written earlier where they overlap (`notation.md` section 3, drawing order). A box with no `background`, or with a size of 0, changes nothing.

There is no smoothing: a box's edge is a whole pixel boundary, since `left`, `top`, `width` and `height` are whole numbers.

**A frame is a pure function of the composition and the frame's number.** In this version nothing moves or appears: every frame of a composition is the same picture. The picture of a frame never depends on the frames drawn before it, on the order they are asked for, on the clock, or on the number of workers. A frame number at or past the frame count is not a frame; it is refused when asked for.

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
- **The form.** AAC-LC, 48 kHz, two channels, at a constant **192 kbit/s**. The track's timescale is 48,000; frx7's writer marks the encoder's lead-in in the edit list, so that the track's length, and the sound read back from it, is the mix's length to the sample. `frx7 probe` shows it as a `sound, aac, 48000 Hz, 2 channels` track with the sample entry `mp4a`.
- **Silence read back.** Every sample of a silent mix that is read back from the track through frx7's sound reader is 0.
- **The bit rate (stated, decision A29).** The track's bit rate, the bytes of all its packets over its length (as `frx7 probe` gives it), is **within 10 % of 192 kbit/s**: from 172,800 to 211,200 bits a second. Measured on a silent track of renders at 30 frames a second: about 196 kbit/s for 2 seconds and within the margin for 1 and 10 seconds (`crates/render/tests/silent_sound_track.rs`); frx7 measured about 193 kbit/s over ten seconds and about 200 kbit/s over one, where the packets of the lead-in weigh more.
- **Plays.** The system's media framework opens the file as playable, decodes every picture to the end and reads the sound track to its end (decision A10).
- **A refusal.** The AAC encoder is started before the file is made; an encoder that refuses ends the run with `FS-0036` as below, and no file is left.

## 6. The place of the finished file, and what the author's files are

- The finished file is `render.output` of the project file, or `<paths.renders>/<name>.mp4`, as `project-file.md` section 3 says; the path is taken from the project folder (`project-location.md` section 4). The folder it is to be in is made when it is missing, and is left in place when the render then fails.
- **The author's files are not touched.** The composition, the project file and every other file of the project have the same bytes after the run as before. The things a run adds are the finished file, its folder when it was missing, and the run log under the folder the command is run from (`run-log.md` section 1).

## 7. The summary

A render that completes prints, on standard output, these lines, in this order, and ends with the exit code 0:

```
Rendered demo/renders/demo.mp4
frames: 150
size: 1280x720
frame rate: 30 fps
length: 5.000 s
renderer: processor
elapsed: 1.234 s
```

- The first line is the word `Rendered` and the path of the finished file: the project folder as it was written on the command line (or the folder found from the file written there), joined with the path of the file. `framesmith render demo` prints `demo/renders/demo.mp4`.
- `frames` is the number of frames written; `size` is `width` by `height`; `frame rate` is `fps` of the root; `length` is the frames divided by the frame rate, in seconds with three decimals; `renderer` is `processor`; `elapsed` is the seconds the run took from the start of the drawing to the end of the file, with three decimals.
- Nothing is printed on standard error.

| Outcome | Standard output | Exit code |
|---|---|---|
| The film is written | The summary above | 0 |
| The project has a report, or there is no project | What `check` prints (section 2) | 1 |
| Every report is of the kind unsupported (the reader's, or `FS-0035`) | The reports and the `Unsupported:` line | 2 |
| A report of `FS-0034`, or of another invalid kind, with or without reports of the kind unsupported | The reports and the `Invalid:` line | 1 |
| The film could not be made | The report of `FS-0036`, then the line `Failed: the video could not be made` | 1 |
| The command line cannot be read | Nothing (standard error: `check-command.md` section 1) | 1 |

## 8. A frame on request

`render` draws frames in order. Any single frame can also be drawn on request, with no command for it: the render crate's `Scene` holds a composition ready to draw, and `Scene::picture(number, renderer)` gives the picture of one frame, for the tests of this version and for the preview later (Sprint 3 brings the command). A frame asked for is the picture `render` draws for that number, whatever was asked for before: frames 0, 7 and the last, in that order or in reverse, give the same pictures. A number at or past the frame count is refused, naming the number and the last frame.

## 9. The checked-in pictures

The pictures that define the correct output of this version are checked in under `fixtures/` as PNG files of 8 bits and four channels, one folder for each fixture composition: `fixtures/<name>/composition.xml` and `fixtures/<name>/expected/frame-NNNN.png`, with `NNNN` the frame's number in four digits. The test draws the frames that have a picture on the processor and compares them with the pictures to the byte, pixel by pixel; a pixel that differs fails the test and the message names the frame's number and the first pixel that differs, with both values. A picture is made from the fixture with the example `crates/render/examples/write_expected_pictures.rs` and is read before it is checked in. The test reads the fixtures from `fixtures/`, or from the folder the environment variable `FRAMESMITH_FIXTURES` names, so that a check can show that a picture which is not the checked-in one fails the test. The example `crates/render/examples/read_back_film.rs` (development only) decodes a written film with frx7 and puts chosen frames on one labelled sheet, with the checked-in pictures under them, for the evidence of a ticket.

## 10. The log

The lines of a `render` are in `run-log.md` section 4. They hold counts, names of stages and codes, never the project's content, its paths or the words of a report.

## 11. Where the code lives

- `notation` reads the tree of the file (the elements in document order, with their attributes and lines) and the value forms a drawing part reads (a length, a colour).
- `project` holds the project's tree, and the lines of the render settings the project file wrote.
- `layout` places a box: the rectangle in the frame from the written `left`, `top`, `width` and `height`.
- `draw` holds the list of drawing steps and the trait both renderers will implement; `draw-cpu` is the renderer on the processor.
- `mix` makes the sound of the film (section 5.2): its rate, its channels, its length and, in this version, its silence.
- `timeline` makes the scene from a project (and refuses what is not drawn) and draws any frame of it on request (`timeline.md`); `render` takes that scene and writes the film through frx7.
- `cli` is the command: it reads the arguments, calls `project` and `render`, prints, and sets the exit code.
