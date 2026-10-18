# The test font

Status: v1 (Sprint 1, T046 part 1, and the figure variants of T050, section 7). Sources: `DECISIONS.md` A8 (text in tests is drawn with a test font made by us, two weights, checked in as a test fixture only, named with `<font>` in every text fixture) and A29 (the font states no value under it); `CLAUDE.md` rule 3 (outlines, metrics and names are our own; no font of the machine is read to make one) and rule 11 (a test font made by us and checked in as a test fixture is not bundling); the OpenType specification of Microsoft, the clauses for the tables named in section 5; plain geometry: a circular arc is approximated by quadratic curves whose control points lie where the tangents meet.

This is the test font of Framesmith's tests: what it holds, how its outlines are made, its metrics, its tables, and how it is made again. The font is a development fixture. It is never in the product, never in the starter, and never read by a command of Framesmith.

## 1. What the font holds

| Item | Value |
|---|---|
| Family name | `Framesmith Test Stroke` (name ID 1), not the name of any typeface |
| Faces | two: Regular (weight class 400) and Bold (weight class 700), one file each |
| Files | `fixtures/fonts/framesmith-test-stroke-400.ttf`, `fixtures/fonts/framesmith-test-stroke-700.ttf` |
| Stroke width | 80 units in Regular, 140 units in Bold (units of the em, section 3) |
| Glyphs | 116 in each face: the 96 of section 2, then the 20 figure variants of section 7 |
| Format | TrueType outlines (`glyf`), one table set, one layout table (`GSUB`, two features, section 7), no hinting instructions, no kerning |

The two faces have the same centrelines and the same advance widths. Only the stroke width differs, so a line of text takes the same width in both weights and its strokes are wider in Bold.

Each face is about 19 KB, under 100 KB.

## 2. The glyphs

| Glyph index | Character | Notes |
|---|---|---|
| 0 | none (`.notdef`) | The missing-character glyph: a rectangle outline, 400 units wide and 620 units high in its centreline box (from the baseline at 40 to the cap height at 660). A character the font does not map is drawn with it. |
| 1 | U+0020 space | No outline. Advance 300 units. |
| 2 to 95 | U+0021 to U+007E | The printable ASCII characters: `A` to `Z`, `a` to `z`, `0` to `9`, and the 32 punctuation and symbol characters of ASCII. The glyph index of a character is its code less 0x1F: U+0021 is glyph 2, U+007E is glyph 95. |
| 96 to 105 | U+0030 to U+0039, tabular | The tabular figures 0 to 9 of section 7, each of one width. Reached through the feature `tnum`. Not mapped by the character map. |
| 106 to 115 | U+0030 to U+0039, old style | The old-style figures 0 to 9 of section 7. Reached through the feature `onum`. Not mapped by the character map. |

The figures 0 to 9 (U+0030 to U+0039, glyphs 17 to 26) are the lining figures of the cap height, and they are the default figures. Glyphs 96 to 105 are the tabular figures and glyphs 106 to 115 the old-style figures of section 7; no character maps to them. No ligature and no alternate form is in this version.

## 3. Design grid

Every glyph is designed on one grid, in units of the em (1000 to the em). The designs are centrelines of strokes. The ink of a stroke is the centreline widened by half the stroke width on each side.

| Line | Centreline | Regular ink | Bold ink |
|---|---|---|---|
| Descender | −160 | −200 | −230 |
| Baseline | 40 | 0 | −30 |
| x-height | 460 | 500 | 530 |
| Cap height | 660 | 700 | 730 |
| Ascender | 710 | 750 | 780 |

Bold ink is 70 units further out than the centreline on each side; regular ink is 40 units. The middle of the capitals is at 350 and the middle of the lower case at 250.

Horizontal positions are free in the design. For each glyph, its designed centrelines are moved sideways so that the leftmost centreline is at x = 100. The advance width is then the width of the centreline box plus 240 units. The ink then lies 60 units from the origin in Regular and 30 in Bold at its left, and 100 units in Regular and 70 in Bold from its advance at its right. A glyph with no width (a dot, a single vertical stroke) has an advance of 240. The figure variants of section 7 are placed as that section says.

## 4. How an outline is made

The outline of each glyph is made by our code from its centreline pieces. A piece is a straight stroke (a line from one point to another) or a circular arc (a centre, a radius, a start angle and an end angle; a negative sweep runs clockwise). A zero-length stroke is a square dot of the stroke width, centred on its point.

1. **Straight strokes.** A stroke becomes a rectangle of the stroke's width. Its ends are flat, cut on the centreline's ends, except where the rule below extends them.
2. **Joins of two straight strokes.** Where the end of a straight stroke meets the end of another straight stroke at the same point, both are extended past that point by half the stroke width, so that a corner is square on the outside. A straight stroke whose end lies inside another straight stroke (a T-junction) is not extended; the other stroke covers its end. Other meetings are not joined: the flat ends stay, and where they meet at an angle a small notch remains (section 8).
3. **Arcs.** An arc becomes an annular sector: its outer edge and inner edge are the arc at the radius plus and minus half the stroke width, closed by the two radial ends. An arc is cut into pieces of at most 30 degrees. Each piece's outer and inner edges are quadratic curves whose control point is where the tangents at the piece's two ends meet (the control point lies on the bisecting radius, at the radius divided by the cosine of half the piece's angle). The error of this curve is below 0.1 per cent of the radius for a piece of 30 degrees.
4. **Joins of an arc.** An arc is not extended. Where an arc meets a straight stroke at its end, the designs meet the stroke at the arc's end point, so the two outlines share a radial or tangent edge.
5. **Contours and winding.** Each rectangle and each annular sector is one closed contour. Every contour is written clockwise, the outer-contour direction of TrueType. Contours may overlap: the font's glyphs carry the OVERLAP_SIMPLE flag when they have more than one contour, and an overlap is filled because the rendering rule is non-zero winding. A hole, such as the counter of an `O`, is a region that no contour covers.
6. **Rounding.** Every point is rounded to the nearest whole unit of the em after the sideways move of section 3. No point is placed in a different place by hinting: the font has no instructions.

The outline of a glyph of the Bold face is made by the same code from the same centrelines with the stroke width 140. Nothing is taken from an existing font: the centrelines are written for this font, and the outlines are computed from them.

## 5. Tables and metrics

The file is a TrueType file: a table directory, then the tables in the order of their tags, each padded to four bytes. The tables are the eleven the font needs and no other: `GSUB`, `OS/2`, `cmap`, `glyf`, `head`, `hhea`, `hmtx`, `loca`, `maxp`, `name` and `post`. The layout of each follows the OpenType specification of Microsoft.

| Table | Content |
|---|---|
| `GSUB` | Version 1.0. One script, `DFLT`, whose default language system asks for both features; a feature list of `onum` then `tnum`, in tag order, each feature with one lookup; a lookup list of two lookups of type 1 (single substitution), each with one subtable of format 2 and a coverage of format 1. The substitutions are those of section 7. |
| `head` | Version 1.0, font revision 1.0, units per em 1000. The bounding box of the glyphs of the face, computed from their points. Short-form offsets are not used: `indexToLocFormat` is 1 (long). `macStyle` bit 0 is set for the Bold face. Created and modified dates are 0, so the file depends on nothing but the design. The check-sum adjustment is the value that makes the sum of the file come to 0xB1B0AFBA. |
| `hhea` | Ascender 850, descender −250, line gap 0. Number of horizontal metrics equal to the glyph count. |
| `maxp` | Version 1.0 with the glyph count and the largest number of points and contours of a glyph. |
| `OS/2` | Version 4. Weight class 400 or 700. Width class 5. Average character width: the mean of the advance widths of all glyphs, rounded down. Typographic ascender 850, descender −250, line gap 0; Windows ascent 850 and descent 250 (both cover the ink of every glyph). x-height and cap height per face: Regular 500 and 700; Bold 530 and 730, the ink lines of section 3. `fsSelection` has REGULAR (bit 6) for the Regular face, BOLD (bit 5) for the Bold face, and USE_TYPO_METRICS (bit 7) for both. Character range: Basic Latin (bit 0 of the first range). First character 0x20, last 0x7E. Vendor identifier `FSTS`. The default character and the break character are the space. The panose, the subscript, superscript and strikeout fields, the family class, the other range bits and the code pages are zero. |
| `cmap` | One subtable, format 4, platform 3 and encoding 1. Maps U+0020 to glyph 1 and U+0021 to U+007E to glyphs 2 to 95. Nothing else is mapped: glyphs 96 to 115 are reached through `GSUB` only. |
| `hmtx` | An advance width for every glyph (sections 3 and 7), and a left side bearing equal to the glyph's leftmost ink point. |
| `loca` | Long offsets, one per glyph and one at the end. The space has no outline and an empty entry. |
| `glyf` | Simple glyphs, one per glyph of sections 2 and 7. Each point is written with its flags and its coordinates in the short form where the value is small. |
| `name` | Format 0, platform 3, encoding 1, language 0x0409. Name IDs: 0 the copyright note `Framesmith test font: a test fixture, not part of any product`; 1 the family; 2 the style, `Regular` or `Bold`; 3 the unique identifier `1.000;FSTS;FramesmithTestStroke-` and the style; 4 the full name, the family and the style; 5 `Version 1.000`; 6 the PostScript name `FramesmithTestStroke-` and the style. |
| `post` | Version 3.0: no glyph names. The underline position is −75 and its thickness 50. |

## 6. Regeneration and checks

The font is made again by one command, from the root of the repository:

    cargo run -p measure --example write_test_font -- fixtures/fonts

The command writes the two files into the folder it is given. The test `regenerated_faces_equal_the_checked_in_files` (`crates/measure/tests/font_regeneration.rs`) regenerates both faces in memory and compares the bytes with the checked-in files, so a change to the generator that is not followed by a new file fails it, and so does a file edited by hand. Another test there holds the generator to the same bytes on every run, and a third holds each file under 100 KB.

The test file `crates/measure/tests/font_readback.rs` reads each checked-in file back with the font reader `skrifa` (a development dependency of `measure`, MIT or Apache-2.0) and checks:

- the family name, the style name and the weight class of each face;
- the glyph count of each face, 116;
- for each character of section 2, that the character maps to its glyph, that the glyph has an outline with points (the space has none), and that U+00E9 maps to none (so that it is drawn with the missing-character glyph);
- that the missing-character glyph is the ink box of a rectangle (60, 0, 540, 700 in Regular; 30, −30, 570, 730 in Bold);
- that the advance widths of the two faces are equal for every glyph;
- that the stem of the lower-case `l` is the stroke width of section 3 in each face (80 and 140 units), and that it is wider in the Bold face.

The test file `crates/measure/tests/font_figure_variants.rs` reads the figure variants of each face back with `skrifa` and checks the rules of section 7: the layout table has the features `onum` and `tnum` and no other; every tabular figure has an advance of 560 units and lies inside it; every old-style figure has the advance of its lining figure; the lining figures keep the advances of section 7, point 3; the lining figures, and the old-style 0, 1, 2, 6 and 8, do not reach below the baseline by more than half a stroke, within a tolerance of 2 units; the old-style 3, 4, 5, 7 and 9 reach below -120 units; and the outlines of the old-style 0 and 2 reach 420 units plus half a stroke at the top, and those of the old-style 6 and 8 reach 660 units plus half a stroke, each within a tolerance of 2 units.

The sheet of the glyphs of each face is made by `crates/measure/examples/glyph_sheet.rs`, a test-only rasterizer of 12 by 10 cells of 180 pixels, one cell per glyph index in order (the last four cells are empty), with the baseline of each cell drawn in grey. It reads the font back through `skrifa`, checks the cmap as section 5 states it, and prints the character of each cell. The sheet is evidence and is not compared with a stored picture.

## 7. Figure variants

The font has two sets of figure variants beside its lining figures (section 2): the tabular figures, glyphs 96 to 105, and the old-style figures, glyphs 106 to 115. Both are drawn from designs of our own, on the grid and in the stroke of sections 3 and 4. The character map does not reach them; the layout table does.

1. **The features.** The layout table is the `GSUB` table of section 5. It holds two features, `onum` and `tnum`, and each has one lookup of type 1 (single substitution). `onum` replaces the lining figure of each digit (glyphs 17 to 26) by its old-style figure (glyphs 106 to 115), and `tnum` replaces it by its tabular figure (glyphs 96 to 105). The font has no `lnum` and no `pnum`. Its default figures are the lining figures, each in its own advance (section 3), so they are proportional too, and no variant exists for either feature to reach. A text that asks for `lining-nums` or `proportional-nums` is drawn with the default figures.
2. **The tabular figures.** Glyph 96 plus d, for each digit d from 0 to 9, is that digit in an advance of 560 units, the advance of the lining 8, so that all ten figures have one width. Its centrelines are those of the lining figure of the digit, and they are placed so that the centreline box is centred in the 320 units from x = 100 to x = 420 (the advance less the 240 of the side gaps, from the left edge of 100). The left edge of the box is not put at x = 100 as it is for a lining figure. Every tabular figure lies inside its advance.
3. **The old-style figures.** Glyph 106 plus d is drawn from the `OLDSTYLE` table of `crates/measure/src/font/design.rs`, which has its own centrelines for each digit. It has the advance of the lining figure of the same digit, which is 500, 440, 540, 539, 540, 529, 540, 540, 560 and 540 units for the digits 0 to 9, and its centreline box is centred in the room that advance leaves, as point 2 says.
4. **Heights.** In centreline values (the ink lies half a stroke beyond them on each side, section 3):
   - the lining figures do not reach below the baseline (40), and neither do the old-style 0, 1, 2, 6 and 8, which stand on it;
   - the old-style 0, 1 and 2 reach 420 at the top, and the old-style 6 and 8 reach 660, the cap height;
   - the old-style 3, 4, 5, 7 and 9 reach 420 at the top and descend to -160, the descender line of section 3.
5. **Use.** The fixtures `fixtures/text-figure-widths` and `fixtures/text-figure-oldstyle` draw these figures through the features, as `text.md`, section 6.6, rule 4 says.

## 8. What the font does not do

- It is not a font for display, reading or print. It has one design and no hinting.
- Its joins are simple (section 4, rule 2). Strokes that meet at an angle at points other than their ends leave small notches, visible in the sheets, for example at V, W, v, w, y, `@`, `&` and `*`. The glyphs are read for their shapes and their metrics.
- It has no kerning, no ligatures, no marks and no characters beyond the printable ASCII of section 2.
- Its files are not copied into a product, a starter or a document. Only tests and the sheet tool read them.
