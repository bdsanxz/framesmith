# Text: paragraphs, fonts and their drawing

Status: v1, Sprint 1, T046 parts 2 (processor) and 3 (card, order t046g), and T048 (the scripts of section 12, order t048). Owner of the rules: the maintainer, under `CLAUDE.md` section 1 (a composition is plain files; animation and drawing are ours) and `PLAN.md` section 3 (the crate `text`, and the two drawing renderers). The notation's rules for text are `docs/spec/notation.md`, sections 6.4, 6.5, 6.12, 7.3, N1, N2, N6, Q5 and the messages of section 13; where this file says a rule is ours, it is stated as such.

## 1. What this version draws

A `<text>` is one paragraph. Its words are its text, and it is drawn on the processor's renderer (`draw-cpu`) with:

- the family, size, weight, style, colour, alignment, line height and letter spacing of section 7.3 of the notation, for the characters that the declared families map (section 2.4): Latin, Cyrillic, Arabic, Hebrew, Japanese and Thai, each shaped, ordered and broken as section 12 states;
- a width that wraps its lines, or none, in which case it is one line as wide as its text; a `max-width` that wraps its lines and leaves it as wide as its widest line (section 5.3);
- its place as an item of a row or a column (`render-command.md`, section 4.3, rule 14), with `align-items` and `align-self`, including `baseline`, which lines up the baselines of the first lines of the texts in a row (section 8);
- the inheritance of the text attributes from the boxes that hold it (section 8).

Not in this version, and reported by `framesmith check` and refused by `framesmith render` before the first frame, with the element's line (FS-0035):

- a family that no `<font>` of the composition declares, including the default `sans-serif`, `serif`, `monospace` and every family of the machine (section 2.3);
- a character that none of the declared families of the list has (section 2.4), including an emoji, which is drawn in no colour in this version (section 12.4);
- a font file that is not a TrueType font, such as an OpenType font with PostScript outlines, a WOFF2 file or a collection (section 2.1);
- a `<span>` (T049), a `direction` attribute (S21 of the notation), and a text attribute beyond those of section 6.4 that this version draws (section 6.4 below);
- an absolute `<text>` with `left` and `right` and no `width`, in a row, a column or a set of columns (section 8);

## 2. Fonts

### 2.1 The `<font>` element and the files it names

A `<font src="..." family="...">` stands in the root (notation 6.12). Its `src` is taken by `Project::resolve` (T032): a path written from the project folder, or, when it begins with `/`, a path on the machine that is read as it is. Its file is read in full and must be a TrueType font: its first four bytes are `0x00010000` or the tag `true`. A file whose first four bytes are the tag `OTTO` (OpenType with PostScript outlines), `wOF2` (WOFF2) or `ttcf` (a collection) is a font of a kind this version does not draw: it is unsupported (FS-0035). Any other file is not a font: it is invalid (FS-0051, exit code 1). A `src` that names no file, at the path it gives from the project folder or on the machine, is reported with FS-0049; a file that exists and cannot be read is reported with FS-0050, a failure. Each report carries the element's line and the path as written.

The face of a file is the one of the font (a file of a collection is not read). Two `<font>` elements with the same `src` and `family` name the same face and are used once (notation 6.12). Two files of one family name are two faces of one family; the test font of decision A8 is such a pair (Regular, weight class 400, and Bold, weight class 700, under the same family).

### 2.2 The family name of a face

A `<font>` with a `family` names its face by that name. Without one, the family is the first family name the file's own name table gives, as the font library reads it (for the test font, `Framesmith Test Stroke`). The name is kept as written; matching of names is exact, case included.

Source: notation 6.12 (`family` default: the name inside the file); the OpenType specification, the `name` table.

### 2.3 The family list of `font-family`

The value is a list of family names separated by commas, tried in order (notation N1). Every name in the list must be the family of a `<font>` of the composition in this version. A name that is a family of the machine, or the generic `sans-serif`, `serif` or `monospace`, is unsupported; the text refuses with the element's line. An empty entry (`A,,B`) and an empty value are reported as the notation's section 13 states. A text with no `font-family` has the default `sans-serif` and so is unsupported until a `<font>` is named for it.

The reason is not a taste. The rule that draws from the machine's fonts (N2, section 14 of the notation) needs the machine's font interfaces, which are the work of T048 and T052 through frx7's system crates. Until then a family of the machine would be drawn by a font we cannot name, so the text is refused rather than drawn wrong (`CLAUDE.md` section 2: never render a wrong video silently).

### 2.4 Characters

Each character of the text, after the white-space rule of section 5.1, is drawn by the first family of the list that has it (N1). A character that none of the listed families maps is unsupported in this version, for the same reason as section 2.3: N2 would draw it from a font of the machine. The test font maps the printable ASCII characters U+0020 to U+007E and no other (test-font.md, section 2); a text in a fixture that uses the test font uses those characters only. The scripts of section 12 are written in a font the composition declares that maps them, which for those fixtures is a font of the machine named by its path (section 12.1).

### 2.5 Glyphs and shaping

Letters are shaped by the font's own tables through the shaping engine (section 10). The test font has no layout features: no kerning, no ligature and no alternate form. A text in a fixture therefore has the advances the font's `hmtx` table gives, plus the letter spacing of section 6.

## 3. Font size, weight and style

### 3.1 Size

`font-size` is a number of pixels above 0 (N6, notation 7.3); the default is 16. Zero or a negative value is reported with the element's line as `` `font-size="0"` must be above 0 `` (FS-0053, invalid, the wording of the notation's section 13). The composition is refused with exit code 1 and no video is written. A font size of zero is reported, never drawn: this is the rule criterion t046-c8 holds.

### 3.2 Weight: which face is drawn

`font-weight` is a number (the default 400, notation 7.3). Any finite number is taken as written; a word such as `bold` is reported as FS-0032, the notation's "is not a number" (section 13). The face is chosen from the faces of the family by the font-weight matching of CSS Fonts Level 4, section 5.2, which is the rule of the font library the shaper uses:

- a desired weight from 400 to 500 inclusive: the faces from the desired weight up to 500 in ascending order, then the faces below the desired weight in descending order, then the faces above 500 in ascending order;
- a desired weight below 400: the faces at or below it in descending order, then the faces above it in ascending order;
- a desired weight above 500: the faces at or above it in ascending order, then the faces below it in descending order.

With the test font (weights 400 and 700): 400 and 500 draw the Regular face; 600, 700, 800 and 900 draw the Bold face; 300 draws the Regular face.

**Our rule, not CSS's:** no synthetic bold. CSS may embolden a face to a weight it does not have; the notation has no attribute for that, so the face is drawn as it is, with no emboldening, for every weight. A weight heavier than every face of the family draws the heaviest face. This is a known difference from browsers, stated here.

### 3.3 Style

`font-style` is `normal` or `italic`. A face of the family with an italic style is drawn for `italic`. Without one, `italic` is an oblique: the upright face is sheared about the baseline by 14 degrees, which is the default angle of the oblique style in CSS Fonts Level 4. The shear moves a point of a glyph at height `h` above the baseline to the right by `h × tan 14°`; the advances are not changed. The shear is applied to the glyph outlines, not to the layout.

## 4. Line height

`line-height` is `normal`, a number, or a number with `px` (notation 7.3).

- **`normal`**: the line's natural height from the font's metrics. The ascender and the descender are the typographic ones when the font's OS/2 `fsSelection` has USE_TYPO_METRICS (bit 7) set; otherwise the `hhea` ones. The leading is added to the height (the line gap of the same table). This is the rule of the font library, which follows the usual reading of the OpenType specification (`OS/2`, `hhea`). For the test font, both tables give an ascender of 850, a descender of −250 and a line gap of 0, so `normal` is 1.1 times the font size at every size.
- **A number**: that many times the font size. `1.5` at 100 pixels is 150 pixels.
- **A number with `px`**: that many pixels. A value of 0 or less is reported with FS-0053 (N6). Any other form, such as a percentage, is reported with FS-0052.

Within a line the baseline sits at the line's top, plus half of the difference between the line height and the ascender plus descender, plus the ascender. This is the half-leading rule of CSS Inline Layout Level 3. Consecutive baselines are one line height apart. The first line's top is the top of the text.

## 5. Lines

### 5.1 White space

A run of spaces, tabs and line ends between words is one space; white space at the start and the end of the text is dropped (notation 7.3). The rule is applied before shaping, so a tab or a line end of the source is a space. A no-break space (U+00A0) is not in the test font and is unsupported (section 2.4).

### 5.2 Where a line may end

A line may end where the Unicode line-breaking rules allow (UAX 14) and nowhere else. The data is the line-break data of Unicode 15.1.0. The segmenter is the engine's dictionary segmenter (section 12.3, the `complex-scripts` feature of section 10), which uses a word dictionary for the scripts that have one and the rules of UAX 14 for the rest. The options are the engine's defaults. For Latin, Cyrillic, Hebrew and Arabic text the line ends at a space, and after a hyphen, as UAX 14 gives.

A word wider than its line is not broken (notation 7.3), as CSS's `overflow-wrap: normal` gives it (decision B19 of `DECISIONS.md`, 2026-10-17). It stays whole on a line of its own, and that line runs past the width. Criterion t046-c3 is judged on a text that has places to break: there no line is wider than the width, except a line that holds one such word and nothing else.

A width of 0 wraps at every place a line may end (notation N6): each word is then a line of its own.

### 5.3 Width of a line and of the text

A line's width does not count the space it ends at, nor the letter spacing of that space (notation 7.3). The width of the text, when it has a `width`, is that width. Without one the text is as wide as its widest line. The line's position across the text is given by `text-align` (section 6.4).

With a `max-width` (notation 7.1) and no `width` of its own, the text breaks its lines so that no line is wider than the `max-width`, at the places the line-breaking rules of section 5.2 allow, and it is as wide as its widest line: a text shorter than the `max-width` keeps its own width. A word that is wider than the `max-width` has no such place, so it is kept whole and its line is wider than the `max-width` (decision B19, `DECISIONS.md`). A `width` of the text's own makes the `max-width` have no effect. The alignment of section 6.2 is within the widest line.

### 5.4 Height of the text

The height of the text is the sum of its line heights (section 4), unless a `height` is written; then the box is that high, and the lines start at its top.

## 6. Letter spacing and alignment

### 6.1 Letter spacing

`letter-spacing` is a number of pixels, or a number with `em` that is multiplied by the font size of the element that writes it, so a child that inherits the value takes it in pixels, as CSS does. A negative value is allowed (N6). Any other form is reported with FS-0052. A letter is a typographic character unit (CSS Text Level 4, letter-spacing): an extended grapheme cluster of UAX 29, which for the text of this version is one printable ASCII character, a space included. The spacing is added after every letter, the last letter of each line included, so a line of `n` letters is wider by `n` times the spacing.

The space that ends a wrapped line is the exception: neither its advance nor its spacing counts toward the line's width (section 5.3).

This is the rule of the shaping engine (spacing after each atom, as Blink applies it). The engine's alignment counts the trailing space of an unwrapped line, but no unwrapped line ends in a space, because of the white-space rule (section 5.1).

### 6.2 Alignment

`text-align` is `left`, `center` or `right` (notation 7.3); any other word is reported as FS-0039, the notation's "not one of" (section 13). Each line lies at that side of the text's width: its left edge at 0, at `(width − line width) / 2`, or at `width − line width`, where the line width is that of section 5.3. For a text with no `width` every line is as wide as the text, so the alignment has no effect.

### 6.3 Colour

`color` is a colour (notation 7.3, `#rrggbb` or `#rrggbbaa`). A text whose colour is written with an alpha below `ff` is refused as unsupported in this version: the colours of the picture have no alpha (the existing colour type of the drawing crate). The colour of a text that names none is black, `#000000`, fully opaque (Q5 of the notation, a maintainer draft).

### 6.4 `text-align`, and the other attributes of this version

The text attributes of this version are: `font-family`, `font-weight`, `font-style`, `font-size`, `line-height`, `letter-spacing`, `color` and `text-align`, with `white-space` only as `normal` and `text-transform` only as `none`. Every other attribute of the `<text>` table of notation 6.4 that is not a placement (section 8) is unsupported.

## 7. Animation

A text takes no `<tween>` or `<set>` in this version. Animated colour and size of text are Sprint 4 (the ticket's out-of-scope list); a `<tween>` or `<set>` in a `<text>` is unsupported.

## 8. Placement, and the inheritance of text attributes

A `<text>` is placed by the rules of placement of section 7.1 of the notation (`left`, `top`, `right`, `bottom`, `position`, `margin`, `width`, `height`, `max-width`), exactly as a box is placed (T041). Its `start` and `duration` hold it to a window as for a box (T042).

In a row or a column a `<text>` is an item of the flow (`render-command.md`, section 4.3, rule 14). Its size is that of its text: a text with no `width` is as wide as its widest line, and one with no `height` is as high as its lines (section 5.4), each rounded up to a whole pixel; its `max-width`, `margin`, `align-self` and `flex-grow` act as for a box. It is not stretched across: `stretch` leaves it at its own size and at the start edge. Its baseline, for `align-items="baseline"` in a row, is the baseline of its first line from its top (section 4), so two texts of different sizes in a row share the baseline of their first lines. A `<text>` that is `position="absolute"` is placed by its edges, as a box is. A percentage `width` or `height` of a `<text>` in a row or a column is not drawn in this version: it is reported by `framesmith check` and refused by `framesmith render` with FS-0035 on the line of the attribute, as an attribute this version does not draw. An absolute `<text>` in a row, a column or a set of columns that has `left` and `right` and no `width` is not drawn in this version either: `framesmith check` reports it and `framesmith render` refuses it with FS-0035 on the line of the `<text>`. The percentage case above is reported on the line of its attribute instead. Every other absolute `<text>` in them is placed by its edges as this section says.

The text attributes of section 3 to 6 pass from a box to everything inside it, and from a text to its children, unless the child writes its own value. A child that writes one wins (criterion t046-c7). At the root the defaults of the table apply. A box takes no `overflow` in this version and `framesmith render` refuses it with FS-0035; `framesmith check` reports it only when a `<style>` gives it (`check-command.md`, section 3c). So no box clips a text: a text is drawn inside its canvas only. A `<text>` with a `background`, a `padding` or another attribute of section 6.4 that this version does not draw is unsupported: `framesmith check` and `framesmith render` both report it with FS-0035 on the line of the attribute, as they report an element inside a `<text>` (section 1).

## 9. Renderers and refusals

Both renderers draw text. The processor's renderer (`draw-cpu`, name `processor`) draws a paragraph by its glyph runs (`render-command.md`, section 4). The card's renderer (`draw-gpu`, name `card`) draws the same paragraphs, by the method of `card-renderer.md` section 9, and its picture agrees with the processor's within the tolerance of `card-renderer.md` section 4. So `--backend gpu`, and `--backend auto` with a card, draw a composition with a `<text>`, and `framesmith check` and `framesmith render` agree on it.

A renderer that draws no text refuses a composition with a `<text>` before any frame and before the film is opened: the report of FS-0035 on the line of the first `<text>`, the closing line of an unsupported form, and exit code 2. The rule is kept for a renderer that does not draw text, and its test uses such a renderer (`crates/cli/tests/text_refused_by_a_renderer_without_text.rs`). It is not a silent fallback (T040): no renderer is swapped for another. The check is made after the renderer is chosen and started, so a `gpu` without a card is still refused with FS-0038 first.

With `--backend auto` the card is chosen when it starts (`render-command.md`, section 1.2), and it draws the text on the card. The processor is chosen only when no card starts, and it draws the text too.

## 10. The shaping and line-breaking engine

The text crate (`crates/text`) draws its paragraphs with the shaping engine parley, version 0.12.0, with its default features off (`Cargo.toml`, the `parley` line of the workspace) and with its feature `complex-scripts` on, which the crate's own `Cargo.toml` turns on: the engine then breaks lines and words with its dictionary segmenters (section 12.3). The feature adds no crate to the lock file. The build holds no platform font backend, since parley's `system` feature is left out, so no option of the font collection reads a font of the machine. The tests of `crates/text/tests/no_font_backend_of_the_system_is_built.rs` hold this: the lock file, which lists the dependencies of every system, has no font interface of a system, and the engine's font library depends on none of them; with the `system` feature turned on they fail. The test `a_new_collection_lists_no_family_and_resolves_no_family_of_the_machine` (`crates/text/tests/unknown_families_and_characters.rs`) asks the engine's own collection, which a new context leaves with no family. The engine's own font library, the shaper and the segmenter are its dependencies and are named in section 11. The card's glyph masks use the same rasteriser as the processor, vello_cpu 0.3.0, through the calls of `card-renderer.md` section 9; no other engine draws text on the card. No type of the engine is in a public interface of our crates.

The engine's behaviour this spec relies on was read from the source of the libraries named in section 11 and measured by a scratch program outside the repository (not kept): the weight matching of section 3.2, the oblique of section 3.3, the line height of section 4, the spacing of section 6.1 and the hanging space of section 5.3. The values measured for the test font at 100 pixels were: `normal` line height 110, so baselines 110 apart at `normal` (the rule of section 4 and the test `the_normal_line_height_is_the_font_size_times_one_point_one` of `crates/text/tests/line_height.rs`); baselines 150 apart at `1.5`; 40 apart at `40px`; `AB` with a letter spacing of 10 widens by 20; the weight 500 face is the Regular face and 600 is the Bold face.

## 11. Sources

- CSS Fonts Level 4: font-weight matching (section 5.2); the oblique style and its default angle (14 degrees).
- CSS Text Level 3 and Level 4: white space, line breaking, letter spacing and typographic character units.
- CSS Inline Layout Level 3: half-leading and line boxes.
- Unicode Standard Annex 14 (line breaking), with the Unicode 15.1.0 data; Unicode Standard Annex 29 (grapheme clusters); Unicode Standard Annex 9 (the bidirectional algorithm, which orders the runs of a line, section 12.2); the Unicode character classes of section 12.3.
- The OpenType specification, `GSUB` (the contextual forms `init`, `medi`, `fina` and `isol` of the Arabic letters, section 12.2).
- The OpenType specification (Microsoft): `head`, `hhea`, `OS/2` (USE_TYPO_METRICS), `name`, `hmtx`, `cmap`; the TrueType outline format (`0x00010000`).
- `docs/spec/notation.md`: 6.2, 6.4, 6.12, 7.1, 7.3, N1, N2, N6, Q5, sections 13 and 14.
- `docs/spec/error-catalogue.md` and its record: FS-0032, FS-0035, FS-0039, FS-0049 to FS-0053 (the codes of this part; the meaning of FS-0034 is a length below 0 where 0 or more is taken, so it is not used for a size that must be above 0).
- `DECISIONS.md` A8 (the test font), A9 (the machine's fonts are a known gap), A29 (the stated values of the card's tolerance, which this part does not state), B4 (the proofs are a record, not a source of files), B19 (a word wider than its line, section 5.2).
- The libraries the spec relies on, at the versions that `Cargo.lock` locks: parley 0.12.0, the shaping and line-breaking engine (sections 2.5, 5.2 and 10), with the crates it is built from, parley_engine 0.12.0 and parley_data 0.12.0; fontique 0.12.0, the font collection and the weight matching (sections 2 and 3.2); skrifa 0.44.0, the reader of the font tables (sections 2.2 and 4); harfrust 0.12.0, the shaper (section 2.5); icu_segmenter 2.3.0, the line segmenter, which follows Unicode 15.1.0 (section 5.2); vello_cpu 0.3.0 and glifo 0.4.0, which fill the glyph outlines on the processor (sections 3.3 and 9), and vello_cpu 0.3.0 again, which makes the card's glyph masks (`card-renderer.md` section 9).
- `docs/spec/test-font.md`: the test font's glyphs, advances and tables.
- `PLAN.md` section 3 (the crates) and section 5 (the picture rules).

## 12. Scripts: fonts, shaping, order, breaks and emoji

This section states what the scripts of the ticket T048 do: Cyrillic, Arabic, Hebrew, Japanese, Thai and emoji, with Latin as before. Their pictures are judged by behaviour, on macOS only, and not byte for byte (decision A9). Sources: section 11.

### 12.1 The fonts of the scripts

The fixtures of the scripts (section 12.5) draw their scripts with the machine's font `Arial Unicode`, named by its path in the composition: `<font src="/System/Library/Fonts/Supplemental/Arial Unicode.ttf" family="Arial Unicode"/>`. Its file is read as it is (section 2.1), and its family is the one the `<font>` element declares, so `font-family="Arial Unicode"` names a family of the composition (section 2.3). The file is a TrueType font. It maps the Latin, Cyrillic, Hebrew, Thai, Japanese kana and kanji characters, and most Arabic letters; it maps no emoji (section 12.4). Because the file belongs to the machine, the tests that read it are compiled for macOS only, and no test of another system reads it (decision A9).

In this version a character that the named font lacks is refused, as section 2.4 says. The rule that draws such a character from the machine's font is T052's; until then it is a known gap (decision A9).

### 12.2 Shaping, order and direction

1. Shaping. A run of text in one script and one direction is shaped by the engine with the font's own tables, with the features the font gives for the script. The letters of Arabic take the contextual forms the font gives them (the `init`, `medi`, `fina` and `isol` forms of GSUB). So a letter inside a word is drawn with the glyph of its joined form, which is not the glyph the font has for that letter alone.
2. Base direction. The base direction of a paragraph is left to right in this version, for every script. The notation has no attribute for the direction (S21 of the notation), and the proofs took the same rule (`_swe/poc/tree/docs/spec/native-notation.md`, section 7, a record only). The runs of a paragraph are put in visual order by the bidirectional algorithm of UAX 9 for that base direction. Each line is drawn in that order.
3. What follows. A run of Arabic or Hebrew letters is drawn from its last letter at the left to its first letter at the right. A run of Latin letters keeps its own order. In a line that mixes them, the runs are placed in the order UAX 9 gives for a left-to-right base: a right-to-left run stays between the runs that come before and after it in the text, and is drawn reversed. A line of Arabic or Hebrew still starts at the left edge of the text; a start at the right edge is S21's, not this version's.

### 12.3 Where a line may end

1. Latin, Cyrillic, Hebrew and Arabic text ends a line at a space, and after a hyphen, as UAX 14 gives (classes SP, HY and AL). The rule of section 5.2 applies: a word wider than its line is kept whole.
2. Japanese text has no spaces. A line may end between any two characters where UAX 14 allows it. It never ends before a character of a class that may not start a line: the closing marks (such as `。`, `、`, `」` and `）`, class CL and CP) and the other classes that rule LB13 keeps with the character before them. It never ends after an opening bracket (such as `「` or `（`, class OP; rule LB14).
3. Thai text has no spaces between its words. A line may end at a space, or at a place that the dictionary of the segmenter gives as a boundary between two words. It never ends inside a word of the dictionary (a line that breaks there is wrong).
4. These places come from the engine's dictionary segmenter, the `complex-scripts` feature of section 10. Without that feature the Thai text of this section would have no place to break, and the Japanese text would be broken by the class rules alone.

### 12.4 Emoji

1. An emoji is drawn in its own colours, whatever the colour of its text. That is the ticket's aim. This version does not draw it. The emoji font of the machine, `Apple Color Emoji.ttc`, is a collection, which section 2.1 refuses; its glyphs are colour bitmaps, which the outline engine draws only with a PNG decoder that the build does not have (the `png` feature of vello_cpu, which needs a crate that the lock file does not hold). Colour glyphs from another font form are not in the build either.
2. So an emoji is a character that no declared family maps. It is refused with FS-0035 on the element's line, by `framesmith check` and by `framesmith render`, before the first frame (section 2.4). It is never drawn in black or in the colour of its text.

### 12.5 The fixtures of the scripts

Each fixture is a one-second composition at 10 frames a second, on a white canvas of 320 by 300 pixels, with one `<text>` of 300 pixels' width at 20 pixels, a line height of twice the size, black, in the Arial Unicode font of section 12.1. The pictures are judged at frames 0 and 9, which are the same picture, since nothing moves.

| Fixture | Script | Judged by |
|---|---|---|
| `text-cyrillic` | Cyrillic | the letters in reading order; the line breaks at spaces inside the width |
| `text-arabic` | Arabic | the letters drawn right to left; the joined forms; the line breaks at word boundaries |
| `text-hebrew-latin` | Hebrew and Latin in one paragraph | each run in its own direction; the runs in UAX 9 order |
| `text-japanese` | Japanese | the breaks between characters; no line starts with a closing mark |
| `text-thai` | Thai, with no spaces | the breaks at word boundaries only |
| `text-emoji` | an emoji | refused with FS-0035 (section 12.4) |

The fixtures have no checked-in pictures (decision A9): a judgement by behaviour is made on the frames the processor draws.
The card's pictures agree with the processor's on every frame of each fixture, the frames 0 and 9 among them, at the tolerance of `PLAN.md`, section 5 (`card-renderer.md`, sections 4 and 7). That test needs a graphics card, so it is reference machine only. The `check` and `render` commands accept the five fixtures that name the machine's font and refuse `text-emoji` with its line (`cli` tests of the same ticket).
