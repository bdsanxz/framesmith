//! The layout of one paragraph (`docs/spec/text.md`, sections 4 to 6): its white space, its lines, the
//! alignment of each line within the width, and the glyph runs with their places in the block.

use std::fmt;
use std::sync::Arc;

use parley::{FontFamily, FontFamilyName, FontStyle, FontWeight, PositionedLayoutItem, StyleProperty};

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

/// The glyphs of one run of the lines that share a face: the bytes of its font file, the index in them, the
/// size, the skew in degrees, and the glyphs.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedRun {
    pub font: Arc<Vec<u8>>,
    pub index: u32,
    pub size: f32,
    pub skew_degrees: f32,
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

/// Lays out the words of a paragraph in `style`, broken into lines within `width` where it is given. Without a
/// width the text is one line. The white space is collapsed first (section 5.1).
pub fn layout(fonts: &mut Fonts, words: &str, style: &Style, width: Option<f32>) -> Result<Block, TextError> {
    lay_out(fonts, words, style, width, Sizing::Given)
}

/// Lays out the words of a paragraph in `style` for a widest width `max` (`docs/spec/text.md`, section 5.3): the lines
/// are broken so that none is wider than `max`, except a word that is wider than `max` on its own, which is kept whole.
/// The block is as wide as its widest line, so a paragraph shorter than `max` keeps its own width, and its lines are
/// aligned within that width.
pub fn layout_fitted(fonts: &mut Fonts, words: &str, style: &Style, max: f32) -> Result<Block, TextError> {
    lay_out(fonts, words, style, Some(max), Sizing::Widest)
}

fn lay_out(fonts: &mut Fonts, words: &str, style: &Style, width: Option<f32>, sizing: Sizing) -> Result<Block, TextError> {
    for family in &style.families {
        if !fonts.has_family(family) {
            return Err(TextError::UnknownFamily(family.clone()));
        }
    }
    let text = collapse(words);
    if text.is_empty() {
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
    let listed: Vec<u64> = fonts
        .faces
        .iter()
        .filter(|face| style.families.contains(&face.family))
        .map(|face| face.blob)
        .collect();
    let Fonts { context, layouts, faces } = fonts;
    let mut builder = layouts.ranged_builder(context, &text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(family_stack(&style.families)));
    builder.push_default(StyleProperty::FontSize(style.size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(style.weight)));
    builder.push_default(StyleProperty::FontStyle(if style.italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    }));
    builder.push_default(StyleProperty::LineHeight(engine_line_height(style.line_height)));
    builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing));
    let mut layout = builder.build(&text);
    layout.break_all_lines(width);

    // First pass: every character must come from a listed family, and the width of each line is taken without
    // the space it ends at (section 5.3).
    let mut widths: Vec<f32> = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let from_listed = listed.contains(&run.font().data.id());
            for cluster in run.clusters() {
                if !from_listed || cluster.glyphs().any(|glyph| glyph.id == 0) {
                    let character = text[cluster.text_range()].chars().next().unwrap_or('\u{fffd}');
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
        let offset = match style.align {
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
                size: style.size,
                skew_degrees: run.synthesis().skew().unwrap_or(0.0),
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

/// The words with each run of spaces, tabs and line ends made one space, and the white space at either end
/// dropped (`docs/spec/text.md`, section 5.1). No other character is white space here.
fn collapse(words: &str) -> String {
    let mut out = String::with_capacity(words.len());
    for word in words.split([' ', '\t', '\n', '\r']).filter(|word| !word.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
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
