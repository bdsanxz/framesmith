//! The layout of one paragraph (`docs/spec/text.md`, sections 4 to 6): its white space, its lines, the
//! alignment of each line within the width, and the glyph runs with their places in the block.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use parley::{BaseDirection, FontFamily, FontFamilyName, FontStyle, FontWeight, PositionedLayoutItem, StyleProperty};

use crate::fonts::Fonts;

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
}

/// A glyph with its place in the block: `x` from the block's left edge, `y` the baseline from its top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
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
    /// The lines in order, from the top of the block.
    pub lines: Vec<Line>,
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
    /// A family the paragraph names has no face in the collection.
    UnknownFamily(String),
    /// A character that none of the listed families has (`docs/spec/text.md`, section 2.4).
    MissingCharacter(char),
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::UnknownFamily(name) => write!(f, "no font of the composition is named {name}"),
            TextError::MissingCharacter(character) => {
                write!(f, "the character {character:?} is in none of the fonts named")
            }
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
    /// `width` pixels of room in the line, at the place where the piece falls.
    Room { width: f32 },
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
    for piece in pieces {
        if let Piece::Words { style, .. } = piece {
            for family in &style.families {
                if !fonts.has_family(family) {
                    return Err(TextError::UnknownFamily(family.clone()));
                }
            }
        }
    }
    let flat = flatten(pieces);
    if flat.text.is_empty() {
        let width = match sizing {
            Sizing::Given => width.unwrap_or(0.0),
            Sizing::Widest => 0.0,
        };
        return Ok(Block {
            width,
            height: 0.0,
            runs: Vec::new(),
            lines: Vec::new(),
        });
    }
    // The faces each piece may draw from: those of its own families.
    let listed: Vec<Vec<u64>> = pieces
        .iter()
        .map(|piece| match piece {
            Piece::Words { style, .. } => fonts
                .faces
                .iter()
                .filter(|face| style.families.contains(&face.family))
                .map(|face| face.blob)
                .collect(),
            Piece::Room { .. } => Vec::new(),
        })
        .collect();
    let Fonts { context, layouts, faces } = fonts;
    let mut builder = layouts.ranged_builder(context, &flat.text, 1.0, true);
    // The base direction is left to right for every script (`docs/spec/text.md`, section 12.2, rule 2). Parley would
    // otherwise take it from the first strong character, which makes an Arabic or Hebrew paragraph right to left.
    builder.set_base_direction(BaseDirection::Ltr);
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
        builder.push(StyleProperty::LetterSpacing(style.letter_spacing), range.clone());
        builder.push(StyleProperty::Brush(*colour), range);
    }
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
        builder.push(StyleProperty::LetterSpacing(style.letter_spacing + width), start..start + length);
    }
    let mut layout = builder.build(&flat.text);
    layout.break_all_lines(width);

    // First pass: every character must come from a face of its own piece, and the width of each line is taken without
    // the space it ends at (section 5.3).
    let mut widths: Vec<f32> = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            for cluster in run.clusters() {
                let start = cluster.text_range().start;
                let from_listed = listed[flat.owner[start]].contains(&run.font().data.id());
                if !from_listed || cluster.glyphs().any(|glyph| glyph.id == 0) {
                    let character = flat.text[cluster.text_range()].chars().next().unwrap_or('\u{fffd}');
                    return Err(TextError::MissingCharacter(character));
                }
            }
        }
        let metrics = line.metrics();
        widths.push(metrics.advance - metrics.hanging_advance);
    }

    // Second pass: the place of each line and of each glyph.
    let widest = widths.iter().copied().fold(0.0, f32::max);
    let block_width = match (sizing, width) {
        (Sizing::Given, Some(width)) => width,
        (Sizing::Given, None) | (Sizing::Widest, _) => widest,
    };
    let mut runs: Vec<PositionedRun> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();
    let mut height = 0.0_f32;
    for (line, line_width) in layout.lines().zip(widths.iter().copied()) {
        let offset = match align {
            Align::Left => 0.0,
            Align::Center => (block_width - line_width) / 2.0,
            Align::Right => block_width - line_width,
        };
        let mut baseline: Option<f32> = None;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let Some(face) = faces.iter().find(|face| face.blob == run.font().data.id()) else {
                continue;
            };
            let glyphs: Vec<PositionedGlyph> = glyph_run
                .positioned_glyphs()
                .map(|glyph| PositionedGlyph {
                    id: glyph.id,
                    x: glyph.x + offset,
                    y: glyph.y,
                })
                .collect();
            if baseline.is_none() {
                baseline = glyphs.first().map(|glyph| glyph.y);
            }
            runs.push(PositionedRun {
                font: face.bytes.clone(),
                index: face.index,
                size: run.font_size(),
                skew_degrees: run.synthesis().skew().unwrap_or(0.0),
                colour: glyph_run.style().brush,
                glyphs,
            });
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
        lines,
    })
}

/// The words of the pieces joined into one text, with the white space collapsed across the pieces (`docs/spec/text.md`,
/// section 5.1): a run of spaces, tabs and line ends is one space, and no space is at either end of the paragraph.
/// A space that lies between two pieces is put where the second piece begins, and room is kept where it falls.
struct Flat {
    text: String,
    /// The index of the piece each byte of the text belongs to.
    owner: Vec<usize>,
    /// Each room as the byte where the character it follows begins, and its width. The room is spacing after that
    /// character, so the words after the room move on by its width.
    spacing: Vec<(usize, f32)>,
}

/// The word joiner (U+2060). Room that comes before the first character of a text has no character to follow, so it
/// is kept on a joiner put at the start of the text. The engine shapes the joiner to a glyph of no advance, and the
/// test font's glyph for it has no ink.
const JOINER: char = '\u{2060}';

fn flatten(pieces: &[Piece]) -> Flat {
    let mut flat = Flat {
        text: String::new(),
        owner: Vec::new(),
        spacing: Vec::new(),
    };
    let mut pending = false;
    let mut last_words = 0;
    // The room before the first character, when the text has none yet.
    let mut leading = 0.0_f32;
    for (number, piece) in pieces.iter().enumerate() {
        match piece {
            Piece::Room { width } => {
                emit_pending_space(&mut flat, &mut pending, last_words);
                match flat.text.char_indices().next_back() {
                    Some((start, _)) => flat.spacing.push((start, *width)),
                    None => leading += width,
                }
            }
            Piece::Words { words, .. } => {
                for character in words.chars() {
                    if matches!(character, ' ' | '\t' | '\n' | '\r') {
                        pending = true;
                        continue;
                    }
                    emit_pending_space(&mut flat, &mut pending, number);
                    if flat.text.is_empty() && leading > 0.0 {
                        flat.text.push(JOINER);
                        flat.owner.resize(flat.text.len(), number);
                        flat.spacing.push((0, leading));
                        leading = 0.0;
                    }
                    flat.text.push(character);
                    flat.owner.resize(flat.text.len(), number);
                }
                last_words = number;
            }
        }
    }
    flat
}

/// Puts the space that white space before a character (or a room) asks for, unless the text is empty or already ends
/// with a space. The space belongs to `owner`.
fn emit_pending_space(flat: &mut Flat, pending: &mut bool, owner: usize) {
    if *pending && !flat.text.is_empty() && !flat.text.ends_with(' ') {
        flat.text.push(' ');
        flat.owner.resize(flat.text.len(), owner);
    }
    *pending = false;
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
