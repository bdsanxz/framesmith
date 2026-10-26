//! The layout of one paragraph (`docs/spec/text.md`, sections 4 to 6): its white space, its lines, the
//! alignment of each line within the width, and the glyph runs with their places in the block.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use parley::{BaseDirection, FontFamily, FontFamilyName, FontFeatures, FontStyle, FontWeight, PositionedLayoutItem, StyleProperty};

use crate::fonts::Fonts;
use crate::picture;

/// A line height as the notation writes it (`docs/spec/text.md`, section 4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// The font's own line height: ascender, descender and line gap.
    Normal,
    /// That many times the font size.
    Multiple(f32),
    /// That many pixels.
    Pixels(f32),
}

/// The alignment of the lines within the width (`docs/spec/text.md`, section 6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// The text attributes of a paragraph, each already read from the notation.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// The families in the order they are tried (`font-family`).
    pub families: Vec<String>,
    /// The size in pixels, above 0.
    pub size: f32,
    /// The weight, as the notation writes it.
    pub weight: f32,
    /// Whether the text is in the italic style; without an italic face it is sheared.
    pub italic: bool,
    pub line_height: LineHeight,
    /// The space added after each letter, in pixels.
    pub letter_spacing: f32,
    pub align: Align,
    /// Whether the words are drawn in capitals: each character takes its upper-case form under the Unicode default
    /// mapping, before the lines are laid out (`docs/spec/text.md`, section 6.6, rule 2).
    pub uppercase: bool,
    /// Whether the paragraph is one line, however narrow its width (`white-space: nowrap`, section 6.6, rule 3).
    pub nowrap: bool,
    /// The OpenType features of the figures asked for (`font-variant-numeric`, section 6.6, rule 4), as their tags, such as
    /// `lnum`. Empty where the font's own figures are drawn.
    pub figures: Vec<[u8; 4]>,
}

/// A glyph with its place in the block: `x` from the block's left edge, `y` the baseline from its top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    /// Where the glyph comes from in the text, for typing it (`docs/spec/text.md`, section 7.2).
    pub source: Source,
}

/// The character a glyph (or a colour picture) is drawn for: the index of the piece that holds it, its number among the
/// characters of that piece (`docs/spec/text.md`, section 7.2, point 1), the line it is on, its place in the order of the
/// text, and the advance it takes in its line: the advance of its cluster on the cluster's first glyph, 0 on the others.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    pub piece: usize,
    pub character: usize,
    pub line: usize,
    pub order: usize,
    pub advance: f32,
}

/// The glyphs of one run of the lines that share a face and a colour: the bytes of its font file, the index in them,
/// the size, the skew in degrees, the colour (RGBA), and the glyphs.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedRun {
    pub font: Arc<Vec<u8>>,
    pub index: u32,
    pub size: f32,
    pub skew_degrees: f32,
    pub colour: [u8; 4],
    pub glyphs: Vec<PositionedGlyph>,
}

/// A laid-out paragraph: its width and height in pixels, and its runs in the order of the lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub width: f32,
    pub height: f32,
    pub runs: Vec<PositionedRun>,
    /// The colour pictures of the emoji of the block, in the order of the text (`docs/spec/text.md`, section 12.4).
    pub pictures: Vec<PositionedPicture>,
    /// The lines in order, from the top of the block.
    pub lines: Vec<Line>,
    /// The number of characters of each piece given, in the order of the pieces (`docs/spec/text.md`, section 7.2,
    /// point 1): 0 for a piece of room.
    pub characters: Vec<usize>,
    /// The first character of the block that no family of its list and no family of the machine maps, drawn as the
    /// missing-character mark (`docs/spec/text.md`, section 2.4, point 3), if there is one.
    pub missing: Option<char>,
}

/// A colour picture of an emoji on a block (`docs/spec/text.md`, section 12.4): its size in pixels, its premultiplied RGBA8
/// pixels in rows with no padding, and the top-left corner of its place, `x` from the block's left edge and `y` from its
/// top. The corner is not yet a whole pixel: a renderer places it at the whole pixel nearest to it, halves up.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedPicture {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
    pub x: f32,
    pub y: f32,
    /// The character of the glyph the picture is drawn for; its advance is 0, since its glyph carries it.
    pub source: Source,
}

/// One line of a block: its left edge from the block's left edge, its width (which leaves out the space it ends
/// at, section 5.3) and its baseline from the block's top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub left: f32,
    pub width: f32,
    pub baseline: f32,
}

/// Why a paragraph cannot be laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// No name of a `font-family` list is a family of the composition, on a system whose fonts are not read
    /// (`docs/spec/text.md`, section 2.3).
    NoFamily,
    /// A character drawn from a face that is not one of its families: the engine's own choice, which is never drawn.
    MissingCharacter(char),
    /// Room of a span (its margin or its padding) in a paragraph with no character to hold it (`docs/spec/text.md`,
    /// section 6.5, rule 9).
    RoomWithoutWords,
    /// Room of a span that falls where the line may break before the span, after a character that no no-break space
    /// keeps with the room (`docs/spec/text.md`, section 10). `piece` is the index, in the pieces given, of the piece
    /// that holds the room.
    RoomAtALineBreak { piece: usize },
    /// A character with no colour picture of the type `png ` in its font at any size, which this version does not draw
    /// (`docs/spec/text.md`, section 12.4, point 9).
    NoPicture(char),
    /// A character whose picture frx7 refuses, with frx7's reason (`docs/spec/text.md`, section 12.4, point 9).
    PictureRefused { character: char, reason: String },
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::NoFamily => write!(
                f,
                "no family of `font-family` is a font of the composition, and this version reads the fonts of the machine on macOS only"
            ),
            TextError::MissingCharacter(character) => {
                write!(f, "the character {character:?} is in none of the fonts named")
            }
            TextError::RoomWithoutWords => write!(
                f,
                "a margin or a padding of a <span> in a <text> with no word, which this version does not draw"
            ),
            TextError::RoomAtALineBreak { .. } => write!(
                f,
                "a margin or a padding of a <span> where the line may break before the span, after a character other than a space or a hyphen, which this version does not draw"
            ),
            TextError::NoPicture(character) => write!(
                f,
                "the character U+{:04X} has no PNG picture in its font at any size, and this version draws only those pictures",
                *character as u32
            ),
            TextError::PictureRefused { character, reason } => write!(
                f,
                "the picture of the character U+{:04X} is refused: {reason}",
                *character as u32
            ),
        }
    }
}

impl std::error::Error for TextError {}

/// How the width of a block is found: the width given to it, or the widest of its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sizing {
    /// The block is as wide as the width given, or the widest line when none is given.
    Given,
    /// The block is as wide as its widest line, whatever width the lines are broken at.
    Widest,
}

/// One piece of a paragraph, in the order it is written (`docs/spec/text.md`, section 6.5): words in one style and one
/// colour, or room that holds no glyph, such as the margin and padding of a span at one of its edges.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// Words in `style`, drawn in `colour` (RGBA).
    Words { words: String, style: Style, colour: [u8; 4] },
    /// `width` pixels of room after the character before it, at the place where the piece falls: the right space of a span.
    Room { width: f32 },
    /// `width` pixels of room before the first character of the piece that follows it: the left space of a span. The room
    /// is kept where the line may end before that character, on a carrier, so that it moves to the line the span begins
    /// when the line breaks there (`docs/spec/text.md`, section 10).
    Lead { width: f32 },
}

/// The colour the words of [`layout`] and [`layout_fitted`] are drawn in: opaque black (`docs/spec/notation.md`, Q5).
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// Lays out the words of a paragraph in `style`, broken into lines within `width` where it is given. Without a
/// width the text is one line. The white space is collapsed first (section 5.1).
pub fn layout(fonts: &mut Fonts, words: &str, style: &Style, width: Option<f32>) -> Result<Block, TextError> {
    let pieces = [Piece::Words {
        words: words.to_string(),
        style: style.clone(),
        colour: BLACK,
    }];
    lay_out(fonts, &pieces, style.align, width, Sizing::Given)
}

/// Lays out the words of a paragraph in `style` for a widest width `max` (`docs/spec/text.md`, section 5.3): the lines
/// are broken so that none is wider than `max`, except a word that is wider than `max` on its own, which is kept whole.
/// The block is as wide as its widest line, so a paragraph shorter than `max` keeps its own width, and its lines are
/// aligned within that width.
pub fn layout_fitted(fonts: &mut Fonts, words: &str, style: &Style, max: f32) -> Result<Block, TextError> {
    let pieces = [Piece::Words {
        words: words.to_string(),
        style: style.clone(),
        colour: BLACK,
    }];
    lay_out(fonts, &pieces, style.align, Some(max), Sizing::Widest)
}

/// Lays out a paragraph made of `pieces` (`docs/spec/text.md`, section 6.5): its lines are broken within `width` where it
/// is given, and aligned by `align`, the alignment of the paragraph (section 6.2). Without a width the text is one line.
/// Each piece is drawn in its own style and colour, and the lines of the pieces share their baselines.
pub fn layout_pieces(fonts: &mut Fonts, pieces: &[Piece], align: Align, width: Option<f32>) -> Result<Block, TextError> {
    lay_out(fonts, pieces, align, width, Sizing::Given)
}

/// Lays out a paragraph made of `pieces` for a widest width `max`, as [`layout_fitted`] does for one style.
pub fn layout_fitted_pieces(fonts: &mut Fonts, pieces: &[Piece], align: Align, max: f32) -> Result<Block, TextError> {
    lay_out(fonts, pieces, align, Some(max), Sizing::Widest)
}

fn lay_out(fonts: &mut Fonts, pieces: &[Piece], align: Align, width: Option<f32>, sizing: Sizing) -> Result<Block, TextError> {
    // Each name of a list as the family the engine knows, with the absent names left out (section 2.3).
    let mut pieces: Vec<Piece> = pieces.to_vec();
    for piece in pieces.iter_mut() {
        if let Piece::Words { style, .. } = piece {
            style.families = fonts.families_of(&style.families)?;
        }
    }
    let plain = plain(&pieces);
    if plain.text.is_empty() {
        if plain.unplaced > 0.0 {
            return Err(TextError::RoomWithoutWords);
        }
        let width = match sizing {
            Sizing::Given => width.unwrap_or(0.0),
            Sizing::Widest => 0.0,
        };
        return Ok(Block {
            width,
            height: 0.0,
            runs: Vec::new(),
            pictures: Vec::new(),
            lines: Vec::new(),
            characters: vec![0; pieces.len()],
            missing: None,
        });
    }
    let flat = carry(fonts, &pieces, plain)?;
    // A character that no family of its list maps is drawn by the family of the machine that the fall-back gives it, which
    // joins the end of its list; one that no family of the machine maps either is drawn as the missing-character mark of
    // the first family of its list (section 2.4).
    let mut unmapped: Vec<char> = Vec::new();
    for (byte, character) in flat.text.char_indices() {
        if character.is_control() || character == JOINER || flat.carriers.contains(&byte) {
            continue;
        }
        let Piece::Words { style, .. } = &pieces[flat.owner[byte]] else {
            continue;
        };
        if fonts.families_map(&style.families, character) || unmapped.contains(&character) {
            continue;
        }
        match fonts.fall_back(character) {
            Some(family) => {
                if let Piece::Words { style, .. } = &mut pieces[flat.owner[byte]] {
                    if !style.families.contains(&family) {
                        style.families.push(family);
                    }
                }
            }
            None => unmapped.push(character),
        }
    }
    let pieces = &pieces[..];
    // The faces each piece may draw from: those of its own families, by blob and index in the file, so that a face of
    // a collection that is not drawn is never matched.
    let listed: Vec<Vec<(u64, u32)>> = pieces
        .iter()
        .map(|piece| match piece {
            Piece::Words { style, .. } => fonts
                .faces
                .iter()
                .filter(|face| style.families.contains(&face.family))
                .map(|face| (face.blob, face.index))
                .collect(),
            Piece::Room { .. } | Piece::Lead { .. } => Vec::new(),
        })
        .collect();
    let Fonts {
        context,
        layouts,
        faces,
        codecs,
        pictures: cache,
        ..
    } = fonts;
    let mut builder = layouts.ranged_builder(context, &flat.text, 1.0, true);
    // The base direction is left to right for every script (`docs/spec/text.md`, section 12.2, rule 2). Parley would
    // otherwise take it from the first strong character, which makes an Arabic or Hebrew paragraph right to left.
    builder.set_base_direction(BaseDirection::Ltr);
    push_owner_styles(&mut builder, &flat, pieces, None);
    // Room is the spacing after the character before it: it takes its width in the line, moves the words after it, and
    // does not change the height of the line (`docs/spec/text.md`, section 6.5, rule 4). It is not an inline box: the
    // engine ends a line after every inline box, whatever the break classes of the text around it.
    let mut spacing: BTreeMap<usize, f32> = BTreeMap::new();
    for (start, width) in &flat.spacing {
        *spacing.entry(*start).or_insert(0.0) += width;
    }
    for (start, width) in spacing {
        let Some(number) = flat.owner.get(start).copied() else {
            continue;
        };
        let Piece::Words { style, .. } = &pieces[number] else {
            continue;
        };
        let length = flat.text[start..].chars().next().map_or(0, char::len_utf8);
        let range = start..start + length;
        if flat.carriers.contains(&start) {
            // A carrier holds its room and no glyph: it is shaped at size 0, so its advance is 0 whatever the font, and
            // it has no line height of its own, so it does not change the height of the line (section 10).
            push_carrier_styles(&mut builder, range.clone(), width);
        } else {
            builder.push(StyleProperty::LetterSpacing(style.letter_spacing + width), range);
        }
    }
    let mut layout = builder.build(&flat.text);
    // A paragraph with `white-space: nowrap` is one line whatever its width (section 6.6, rule 3).
    let nowrap = pieces
        .iter()
        .any(|piece| matches!(piece, Piece::Words { style, .. } if style.nowrap));
    layout.break_all_lines(if nowrap { None } else { width });

    // First pass: every character must come from a face of its own piece, and the width of each line is taken without
    // the space it ends at (section 5.3).
    let mut widths: Vec<f32> = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            // The run of a carrier is the only one at size 0: no font size of a style is 0 (`docs/spec/text.md`, section 3.1).
            if run.font_size() == 0.0 {
                continue;
            }
            let key = (run.font().data.id(), run.font().index);
            for cluster in run.clusters() {
                let start = cluster.text_range().start;
                let from_listed = listed[flat.owner[start]].contains(&key);
                let character = flat.text[cluster.text_range()].chars().next().unwrap_or('\u{fffd}');
                // A character no font maps is drawn as glyph 0 of the first family of its list (section 2.4, point 3).
                if !from_listed || (cluster.glyphs().any(|glyph| glyph.id == 0) && !unmapped.contains(&character)) {
                    return Err(TextError::MissingCharacter(character));
                }
            }
        }
        let metrics = line.metrics();
        widths.push(metrics.advance - metrics.hanging_advance);
    }

    // Second pass: the place of each line and of each glyph. A glyph of a colour face is drawn as its picture, or refused
    // (`docs/spec/text.md`, section 12.4, point 9); its outlines are never drawn.
    let widest = widths.iter().copied().fold(0.0, f32::max);
    let block_width = match (sizing, width) {
        (Sizing::Given, Some(width)) => width,
        (Sizing::Given, None) | (Sizing::Widest, _) => widest,
    };
    let mut runs: Vec<PositionedRun> = Vec::new();
    let mut placed_pictures: Vec<PositionedPicture> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();
    let mut height = 0.0_f32;
    let (numbers, characters) = character_numbers(&flat, pieces.len());
    for (line_number, (line, line_width)) in layout.lines().zip(widths.iter().copied()).enumerate() {
        let offset = match align {
            Align::Left => 0.0,
            Align::Center => (block_width - line_width) / 2.0,
            Align::Right => block_width - line_width,
        };
        let mut baseline: Option<f32> = None;
        // The clusters of each shaped run, and style, already given to a glyph run of the line: a run of the engine is cut
        // into glyph runs by style, and its clusters are those of the whole run.
        let mut consumed: BTreeMap<(usize, u16), usize> = BTreeMap::new();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            // A carrier's run holds no glyph of the paragraph, so it is not drawn (section 10).
            if run.font_size() == 0.0 {
                continue;
            }
            let key = (run.font().data.id(), run.font().index);
            let Some(face) = faces.iter().find(|face| (face.blob, face.index) == key) else {
                continue;
            };
            let wanted = glyph_run.positioned_glyphs().count();
            let taken = consumed.entry((run.text_range().start, glyph_run.style_index())).or_insert(0);
            let sources = glyph_sources(&flat, &numbers, run, glyph_run.style_index(), taken, wanted, line_number);
            let glyphs: Vec<PositionedGlyph> = glyph_run
                .positioned_glyphs()
                .zip(sources)
                .map(|(glyph, source)| PositionedGlyph {
                    id: glyph.id,
                    x: glyph.x + offset,
                    y: glyph.y,
                    source,
                })
                .collect();
            if baseline.is_none() {
                baseline = glyphs.first().map(|glyph| glyph.y);
            }
            let Some(colour) = &face.colour else {
                runs.push(PositionedRun {
                    font: face.bytes.clone(),
                    index: face.index,
                    size: run.font_size(),
                    skew_degrees: run.synthesis().skew().unwrap_or(0.0),
                    colour: glyph_run.style().brush,
                    glyphs,
                });
                continue;
            };
            // The character each glyph belongs to: the first of its cluster.
            let characters: Vec<char> = glyphs
                .iter()
                .map(|glyph| {
                    flat.text
                        .get(glyph.source.order..)
                        .and_then(|rest| rest.chars().next())
                        .unwrap_or(' ')
                })
                .collect();
            let source = picture::Source {
                blob: face.blob,
                index: face.index,
                bytes: &face.bytes,
                colour,
            };
            for (glyph, character) in glyphs.iter().zip(characters) {
                if matches!(character, ' ' | '\t' | '\n' | '\r') {
                    continue;
                }
                let id = glyph.id as usize;
                let made = picture::glyph_picture(codecs, cache, &source, id, run.font_size()).map_err(|missing| match missing {
                    picture::Missing::NoPicture => TextError::NoPicture(character),
                    picture::Missing::Refused(reason) => TextError::PictureRefused { character, reason },
                })?;
                if made.width == 0 || made.height == 0 {
                    continue;
                }
                let Some((x, y)) = picture::top_left(&source, id, &made, run.font_size(), glyph.x, glyph.y) else {
                    return Err(TextError::PictureRefused {
                        character,
                        reason: "its outline box cannot be read".to_string(),
                    });
                };
                placed_pictures.push(PositionedPicture {
                    width: made.width,
                    height: made.height,
                    pixels: made.pixels.clone(),
                    x,
                    y,
                    source: Source {
                        advance: 0.0,
                        ..glyph.source
                    },
                });
            }
        }
        // A line with no glyph sits at its own top; every line of a text has glyphs.
        lines.push(Line {
            left: offset,
            width: line_width,
            baseline: baseline.unwrap_or(height),
        });
        height += line.metrics().line_height;
    }
    Ok(Block {
        width: block_width,
        height,
        runs,
        pictures: placed_pictures,
        lines,
        characters,
        missing: unmapped.first().copied(),
    })
}

/// The number of the character that begins at each byte of the flat text among the characters of its piece, `None` for a
/// carrier, which is not a character (`docs/spec/text.md`, section 7.2, point 1); and the count of characters of each piece.
fn character_numbers(flat: &Flat, pieces: usize) -> (Vec<Option<usize>>, Vec<usize>) {
    let mut numbers = vec![None; flat.text.len()];
    let mut counts = vec![0; pieces];
    for (start, _) in flat.text.char_indices() {
        if flat.carriers.contains(&start) {
            continue;
        }
        let piece = flat.owner[start];
        numbers[start] = Some(counts[piece]);
        counts[piece] += 1;
    }
    (numbers, counts)
}

/// The source of each of the `wanted` glyphs of a glyph run, in the order its glyphs are placed: the clusters of `run` in
/// the style `style` after the `taken` ones already given to a glyph run before it. Each glyph's source is the character
/// that begins its cluster (or the first character after it, for a cluster that begins with a carrier), with the cluster's
/// advance on its first glyph.
#[allow(clippy::too_many_arguments)]
fn glyph_sources(
    flat: &Flat,
    numbers: &[Option<usize>],
    run: &parley::Run<'_, [u8; 4]>,
    style: u16,
    taken: &mut usize,
    wanted: usize,
    line: usize,
) -> Vec<Source> {
    let mut clusters: Vec<Vec<Source>> = Vec::new();
    let mut count = 0;
    for cluster in run.clusters().filter(|cluster| cluster.style_index() == style).skip(*taken) {
        if count >= wanted {
            break;
        }
        *taken += 1;
        let range = cluster.text_range();
        let start = (range.start..range.end.max(range.start + 1))
            .find(|byte| numbers.get(*byte).copied().flatten().is_some())
            .unwrap_or(range.start);
        let source = Source {
            piece: flat.owner.get(start).copied().unwrap_or(0),
            character: numbers.get(start).copied().flatten().unwrap_or(0),
            line,
            order: start,
            advance: 0.0,
        };
        let glyph_count = cluster.glyphs().count();
        count += glyph_count;
        let mut glyphs = vec![source; glyph_count];
        if let Some(first) = glyphs.first_mut() {
            first.advance = cluster.advance();
        }
        clusters.push(glyphs);
    }
    if run.is_rtl() {
        clusters.reverse();
    }
    clusters.into_iter().flatten().collect()
}

impl Block {
    /// The block typed (`docs/spec/text.md`, section 7.2): `shown[piece]` characters of each piece drawn, the others
    /// left out, and each glyph drawn moved left by the advances of the glyphs left out before it in its line, in the
    /// order of the text. The size, the lines and their places are those of the whole block.
    pub fn typed(&self, shown: &[usize]) -> Block {
        let drawn = |source: &Source| source.character < shown.get(source.piece).copied().unwrap_or(usize::MAX);
        // The glyphs left out, as (line, order, advance).
        let mut hidden: Vec<(usize, usize, f32)> = Vec::new();
        for run in &self.runs {
            for glyph in &run.glyphs {
                if !drawn(&glyph.source) {
                    hidden.push((glyph.source.line, glyph.source.order, glyph.source.advance));
                }
            }
        }
        let shift = |source: &Source| -> f32 {
            hidden
                .iter()
                .filter(|(line, order, _)| *line == source.line && *order < source.order)
                .map(|(_, _, advance)| advance)
                .sum()
        };
        let runs = self
            .runs
            .iter()
            .map(|run| PositionedRun {
                glyphs: run
                    .glyphs
                    .iter()
                    .filter(|glyph| drawn(&glyph.source))
                    .map(|glyph| PositionedGlyph {
                        x: glyph.x - shift(&glyph.source),
                        ..*glyph
                    })
                    .collect(),
                ..run.clone()
            })
            .filter(|run| !run.glyphs.is_empty())
            .collect();
        let pictures = self
            .pictures
            .iter()
            .filter(|picture| drawn(&picture.source))
            .map(|picture| PositionedPicture {
                x: picture.x - shift(&picture.source),
                ..picture.clone()
            })
            .collect();
        Block {
            runs,
            pictures,
            ..self.clone()
        }
    }
}

/// The words of the pieces joined into one text, with the white space collapsed across the pieces (`docs/spec/text.md`,
/// section 5.1): a run of spaces, tabs and line ends is one space, and no space is at either end of the paragraph.
/// A space that lies between two pieces is put where the second piece begins, and room is kept where it falls. This is
/// the text before any carrier of room is put into it (section 10).
struct Plain {
    text: String,
    /// The index of the piece each byte of the text belongs to.
    owner: Vec<usize>,
    /// Room that is spacing after the character that begins at each key, a byte of the text: the room of a span's right
    /// edge that follows that character.
    spacing: BTreeMap<usize, f32>,
    /// The room that waits for the character after it, keyed by the byte where that character begins: the end of the text
    /// when no character follows. Room that falls at the same place adds up here.
    places: BTreeMap<usize, Place>,
    /// The room of a text with no character, which has no place to hold it.
    unplaced: f32,
}

/// The room at one place of a text: its width, and the piece that holds it first.
#[derive(Clone, Copy, Debug)]
struct Place {
    width: f32,
    piece: usize,
}

/// The text with its carriers and each room's spacing, ready for the engine (section 10).
struct Flat {
    text: String,
    /// The index of the piece each byte of the text belongs to.
    owner: Vec<usize>,
    /// Each room as the byte where the character it follows begins, and its width. The room is spacing after that
    /// character, so the words after the room move on by its width. A carrier (below) is one of these characters.
    spacing: Vec<(usize, f32)>,
    /// The byte where each carrier begins: a character that holds room and no glyph (`docs/spec/text.md`, section 10).
    carriers: Vec<usize>,
}

/// The word joiner (U+2060): the carrier of room where no line may end before the character after the room, which
/// allows no break on either side (UAX 14, rule LB11). A joiner is never counted as a missing character and never drawn,
/// whatever the font (section 10).
const JOINER: char = '\u{2060}';

/// The no-break space (U+00A0): the carrier of room where a line may end after a space or a hyphen. A line may end before
/// it after a space, a hyphen or a class BA break character (UAX 14, rule LB12a), and may not end after it (LB12), so the
/// line still ends where it did, and the room goes to the line that begins with the character after it (section 10).
const NO_BREAK_SPACE: char = '\u{a0}';

/// The letter spacing, in pixels, that the analysis of the line breaks gives every character. It is small and positive, so
/// that no character has an advance of 0 and the engine takes every place where a line may end (section 10).
const ANALYSIS_SPACING: f32 = 1.0;

/// The carrier of one place of room: where it falls in the text without carriers, the character, the room it holds, and
/// the piece that holds the room first.
#[derive(Clone, Copy, Debug)]
struct Carrier {
    site: usize,
    character: char,
    width: f32,
    piece: usize,
}

/// Joins the words of the pieces, and records each room at its place, before any carrier is chosen.
fn plain(pieces: &[Piece]) -> Plain {
    let mut plain = Plain {
        text: String::new(),
        owner: Vec::new(),
        spacing: BTreeMap::new(),
        places: BTreeMap::new(),
        unplaced: 0.0,
    };
    let mut pending = false;
    let mut last_words = 0;
    for (number, piece) in pieces.iter().enumerate() {
        match piece {
            Piece::Room { width } => {
                emit_pending_space(&mut plain, &mut pending, last_words);
                if *width > 0.0 {
                    room_after(&mut plain, number, *width);
                }
            }
            Piece::Lead { width } => {
                emit_pending_space(&mut plain, &mut pending, last_words);
                if *width > 0.0 {
                    let end = plain.text.len();
                    let place = plain.places.entry(end).or_insert(Place { width: 0.0, piece: number });
                    place.width += width;
                }
            }
            Piece::Words { words, style, .. } => {
                for character in words.chars() {
                    if matches!(character, ' ' | '\t' | '\n' | '\r') {
                        pending = true;
                        continue;
                    }
                    emit_pending_space(&mut plain, &mut pending, number);
                    if style.uppercase {
                        plain.text.extend(character.to_uppercase());
                    } else {
                        plain.text.push(character);
                    }
                    plain.owner.resize(plain.text.len(), number);
                }
                last_words = number;
            }
        }
    }
    plain.unplaced = if plain.text.is_empty() {
        plain.places.values().map(|place| place.width).sum()
    } else {
        0.0
    };
    plain
}

/// Puts room of a right edge (`Piece::Room`) at the place where it falls: with the room that waits at the end of the
/// text, when there is one, and otherwise as spacing after the last character.
fn room_after(plain: &mut Plain, number: usize, width: f32) {
    let end = plain.text.len();
    if plain.text.is_empty() || plain.places.contains_key(&end) {
        let place = plain.places.entry(end).or_insert(Place { width: 0.0, piece: number });
        place.width += width;
    } else if let Some((start, _)) = plain.text.char_indices().next_back() {
        *plain.spacing.entry(start).or_insert(0.0) += width;
    }
}

/// Puts the space that white space before a character (or a room) asks for, unless the text is empty or already ends
/// with a space. The space belongs to `owner`.
fn emit_pending_space(plain: &mut Plain, pending: &mut bool, owner: usize) {
    if *pending && !plain.text.is_empty() && !plain.text.ends_with(' ') {
        plain.text.push(' ');
        plain.owner.resize(plain.text.len(), owner);
    }
    *pending = false;
}

/// Chooses the carrier of each place of room, and puts the carriers into the text (`docs/spec/text.md`, section 10).
///
/// The choice is made from the line breaks the text has without any carrier: the places where the engine's line breaker
/// may end a line, taken at width 0. At a place with no character before it, room is carried by a word joiner. At a place
/// where no line may end, room after a letter is spacing after that letter, and room after any other character is carried
/// by a word joiner, which allows no break on either side. At a place where a line may end, room is carried by a no-break
/// space, which keeps the line end where it is. The carried text is then broken again, and its places are compared with
/// the places of the text without carriers: a place that a carrier moved or removed is a refusal of the room that holds it.
fn carry(fonts: &mut Fonts, pieces: &[Piece], plain: Plain) -> Result<Flat, TextError> {
    if plain.places.is_empty() {
        return Ok(Flat {
            text: plain.text,
            owner: plain.owner,
            spacing: plain.spacing.into_iter().collect(),
            carriers: Vec::new(),
        });
    }
    let without = Flat {
        text: plain.text.clone(),
        owner: plain.owner.clone(),
        spacing: Vec::new(),
        carriers: Vec::new(),
    };
    let breaks = break_places(fonts, &without, pieces);
    let mut carriers: Vec<Carrier> = Vec::new();
    let mut letters = plain.spacing;
    for (&site, place) in &plain.places {
        let before = plain.text[..site].char_indices().next_back();
        let character = if site == 0 {
            Some(JOINER)
        } else if site == plain.text.len() {
            // No character follows the room, so no line can end after it: it is spacing after the character before it.
            None
        } else if breaks.contains(&site) {
            Some(NO_BREAK_SPACE)
        } else if before.is_some_and(|(_, character)| character.is_alphanumeric()) {
            None
        } else {
            Some(JOINER)
        };
        match character {
            Some(character) => carriers.push(Carrier {
                site,
                character,
                width: place.width,
                piece: place.piece,
            }),
            None => {
                let start = before.map_or(0, |(start, _)| start);
                *letters.entry(start).or_insert(0.0) += place.width;
            }
        }
    }
    let by_site: BTreeMap<usize, Carrier> = carriers.iter().map(|carrier| (carrier.site, *carrier)).collect();
    let mut text = String::new();
    let mut owner: Vec<usize> = Vec::new();
    let mut spacing: BTreeMap<usize, f32> = BTreeMap::new();
    let mut starts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut placed: Vec<Placed> = Vec::new();
    for (start, character) in plain.text.char_indices() {
        let holder = plain.owner[start];
        if let Some(carrier) = by_site.get(&start) {
            let at = text.len();
            text.push(carrier.character);
            owner.resize(text.len(), holder);
            *spacing.entry(at).or_insert(0.0) += carrier.width;
            placed.push(Placed {
                site: start,
                start: at,
                length: carrier.character.len_utf8(),
            });
        }
        starts.insert(start, text.len());
        text.push(character);
        owner.resize(text.len(), holder);
    }
    for (start, width) in letters {
        if let Some(at) = starts.get(&start) {
            *spacing.entry(*at).or_insert(0.0) += width;
        }
    }
    let carried = Flat {
        text,
        owner,
        spacing: spacing.into_iter().collect(),
        carriers: placed.iter().map(|carrier| carrier.start).collect(),
    };
    if placed.is_empty() {
        return Ok(carried);
    }
    let after = break_places(fonts, &carried, pieces);
    match first_difference(&breaks, &placed, &after) {
        None => Ok(carried),
        Some(site) => {
            let piece = by_site.get(&site).map_or(0, |carrier| carrier.piece);
            Err(TextError::RoomAtALineBreak { piece })
        }
    }
}

/// A carrier in the carried text: the place it has in the text without carriers, where it starts in the carried text,
/// and its length in bytes.
#[derive(Clone, Copy, Debug)]
struct Placed {
    site: usize,
    start: usize,
    length: usize,
}

/// Compares the places where a line may end in the carried text, mapped back to the text without carriers, with the
/// places of the text without carriers (`docs/spec/text.md`, section 10). Gives the place of the carrier nearest the first
/// place that differs, or none where the two are the same. A line end just after a carrier is a difference of its own.
fn first_difference(without: &[usize], placed: &[Placed], carried: &[usize]) -> Option<usize> {
    let mut mapped: Vec<usize> = Vec::new();
    let mut differences: Vec<usize> = Vec::new();
    for &end in carried {
        if let Some(carrier) = placed.iter().find(|carrier| carrier.start + carrier.length == end) {
            differences.push(carrier.site);
            continue;
        }
        let removed: usize = placed
            .iter()
            .filter(|carrier| carrier.start + carrier.length <= end)
            .map(|carrier| carrier.length)
            .sum();
        mapped.push(end - removed);
    }
    differences.extend(without.iter().copied().filter(|site| !mapped.contains(site)));
    differences.extend(mapped.iter().copied().filter(|site| !without.contains(site)));
    let first = differences.into_iter().min()?;
    placed
        .iter()
        .min_by_key(|carrier| (carrier.site.abs_diff(first), carrier.site))
        .map(|carrier| carrier.site)
}

/// The places where a line of `flat` may end, as the engine's line breaker takes them at width 0: every place the
/// breaker may end a line at is taken, because each character has an advance above 0 (section 10). Each character gets
/// the analysis letter spacing, and a carrier has font size 0 as in the layout.
fn break_places(fonts: &mut Fonts, flat: &Flat, pieces: &[Piece]) -> Vec<usize> {
    let Fonts { context, layouts, .. } = fonts;
    let mut builder = layouts.ranged_builder(context, &flat.text, 1.0, true);
    builder.set_base_direction(BaseDirection::Ltr);
    push_owner_styles(&mut builder, flat, pieces, Some(ANALYSIS_SPACING));
    for &start in &flat.carriers {
        let length = flat.text[start..].chars().next().map_or(0, char::len_utf8);
        push_carrier_styles(&mut builder, start..start + length, ANALYSIS_SPACING);
    }
    let mut layout = builder.build(&flat.text);
    layout.break_all_lines(Some(0.0));
    let end = flat.text.len();
    layout.lines().map(|line| line.text_range().end).filter(|&at| at < end).collect()
}

/// The style of each run of the text that one piece owns: its family, size, weight, slant, line height, colour, and its
/// letter spacing, which is `letter` where one is given and the piece's own otherwise.
fn push_owner_styles(builder: &mut parley::RangedBuilder<'_, [u8; 4]>, flat: &Flat, pieces: &[Piece], letter: Option<f32>) {
    for (range, number) in runs_of(&flat.owner) {
        let Piece::Words { style, colour, .. } = &pieces[number] else {
            continue;
        };
        builder.push(StyleProperty::FontFamily(family_stack(&style.families)), range.clone());
        builder.push(StyleProperty::FontSize(style.size), range.clone());
        builder.push(StyleProperty::FontWeight(FontWeight::new(style.weight)), range.clone());
        builder.push(
            StyleProperty::FontStyle(if style.italic { FontStyle::Italic } else { FontStyle::Normal }),
            range.clone(),
        );
        builder.push(StyleProperty::LineHeight(engine_line_height(style.line_height)), range.clone());
        builder.push(StyleProperty::LetterSpacing(letter.unwrap_or(style.letter_spacing)), range.clone());
        if !style.figures.is_empty() {
            builder.push(StyleProperty::FontFeatures(figure_features(&style.figures)), range.clone());
        }
        builder.push(StyleProperty::Brush(*colour), range);
    }
}

/// The features of the figures asked for, in the form the engine reads as a list of settings: each tag with the value 1.
fn figure_features(tags: &[[u8; 4]]) -> FontFeatures<'static> {
    let settings: Vec<String> = tags.iter().map(|tag| format!("\"{}\" 1", String::from_utf8_lossy(tag))).collect();
    FontFeatures::Source(std::borrow::Cow::Owned(settings.join(", ")))
}

/// The style of a carrier: font size 0, so that its advance is only its letter spacing, line height 0, so that it adds
/// no height, and the letter spacing `letter`.
fn push_carrier_styles(builder: &mut parley::RangedBuilder<'_, [u8; 4]>, range: Range<usize>, letter: f32) {
    builder.push(StyleProperty::FontSize(0.0), range.clone());
    builder.push(StyleProperty::LineHeight(parley::LineHeight::Absolute(0.0)), range.clone());
    builder.push(StyleProperty::LetterSpacing(letter), range);
}

/// The byte ranges of a text that one piece owns, each with its piece, in the order of the text.
fn runs_of(owner: &[usize]) -> Vec<(Range<usize>, usize)> {
    let mut runs = Vec::new();
    let mut start = 0;
    for end in 1..=owner.len() {
        if end == owner.len() || owner[end] != owner[start] {
            runs.push((start..end, owner[start]));
            start = end;
        }
    }
    runs
}

fn family_stack(names: &[String]) -> FontFamily<'static> {
    FontFamily::List(
        names
            .iter()
            .map(|name| FontFamilyName::Named(name.clone().into()))
            .collect::<Vec<_>>()
            .into(),
    )
}

fn engine_line_height(value: LineHeight) -> parley::LineHeight {
    match value {
        LineHeight::Normal => parley::LineHeight::MetricsRelative(1.0),
        LineHeight::Multiple(multiple) => parley::LineHeight::FontSizeRelative(multiple),
        LineHeight::Pixels(pixels) => parley::LineHeight::Absolute(pixels),
    }
}
