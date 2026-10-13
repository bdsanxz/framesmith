//! Text: fonts, shaping of words, and the vertical measures of a line. Done once when the
//! composition is loaded, never per frame.

use crate::model::{GlyphRun, LineHeight, PlacedGlyph, TextStyle};
use parley::{
    BaseDirection, FontContext, FontFamily, FontStyle, FontWeight, InlineBox, InlineBoxKind, Layout, LayoutContext, PositionedLayoutItem,
    RangedBuilder, StyleProperty, VerticalAlign,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use vello_cpu::peniko::Blob;

/// A word (or any text without a break in it) with its glyphs placed from the left end of its
/// baseline.
pub struct Shaped {
    pub runs: Vec<GlyphRun>,
    pub width: f64,
}

/// A paragraph flowed into lines. See `docs/spec/native-notation.md`, section 7.
pub struct Flowed {
    pub lines: Vec<FlowedLine>,
}

pub struct FlowedLine {
    /// Its width without the space it ends at.
    pub width: f64,
    /// Its pieces from left to right.
    pub pieces: Vec<Piece>,
}

pub enum Piece {
    /// Glyphs placed from the left end of the line's baseline.
    Run(GlyphRun),
    /// The box with this place among the boxes given, and where its left edge is.
    Box(usize, f64),
}

/// How a line of a style stands around its baseline, in whole pixels as a browser lays it out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineBox {
    pub above: f64,
    pub below: f64,
}

pub struct Shaper {
    fonts: FontContext,
    scratch: LayoutContext<()>,
    words: HashMap<(String, String), Rc<Shaped>>,
    lines: HashMap<String, LineBox>,
}

/// What tells two styles apart for shaping.
fn shaping_key(style: &TextStyle) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        style.family, style.weight, style.italic, style.size, style.letter_spacing, style.figures
    )
}

impl Default for Shaper {
    fn default() -> Shaper {
        Shaper::new()
    }
}

impl Shaper {
    pub fn new() -> Shaper {
        Shaper {
            fonts: FontContext::new(),
            scratch: LayoutContext::new(),
            words: HashMap::new(),
            lines: HashMap::new(),
        }
    }

    /// Makes a font file known: TrueType or OpenType as it is, WOFF2 after it is unpacked.
    /// `family` is the name it is asked for by, when that is not the name inside the file.
    pub fn register(&mut self, bytes: Vec<u8>, family: Option<&str>) -> Result<(), String> {
        let bytes = if bytes.starts_with(b"wOF2") {
            wuff::decompress_woff2(&bytes).map_err(|error| format!("the WOFF2 font could not be unpacked: {error:?}"))?
        } else {
            bytes
        };
        let info = family.map(|name| parley::fontique::FontInfoOverride {
            family_name: Some(name),
            ..Default::default()
        });
        let registered = self.fonts.collection.register_fonts(Blob::from(bytes), info);
        if registered.is_empty() {
            return Err("the file holds no font".to_string());
        }
        Ok(())
    }

    fn layout(&mut self, text: &str, style: &TextStyle) -> Layout<()> {
        let mut builder = self.scratch.ranged_builder(&mut self.fonts, text, 1.0, false);
        styled(&mut builder, style);
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout
    }

    /// Flows `text` as one paragraph into lines no wider than `limit`. Each of `boxes` stands
    /// at a byte of the text and has a width.
    pub fn flow(&mut self, text: &str, boxes: &[(usize, f64)], style: &TextStyle, limit: Option<f64>) -> Flowed {
        let space_width = self.space(style) as f32;
        let mut builder = self.scratch.ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.set_base_direction(BaseDirection::Ltr);
        styled(&mut builder, style);
        for (place, (index, width)) in boxes.iter().enumerate() {
            builder.push_inline_box(InlineBox {
                id: place as u64,
                kind: InlineBoxKind::InFlow,
                index: *index,
                width: *width as f32,
                height: 0.0,
                baseline: None,
                vertical_align: VerticalAlign::BASELINE,
            });
        }
        let mut layout: Layout<()> = builder.build(text);
        // A little room, so that a text given exactly its own width stays on its lines.
        layout.break_all_lines(limit.map(|limit| limit as f32 + 0.05));
        let lines = layout.lines().map(|line| {
            let metrics = line.metrics();
            // The space a line ends at hangs past its end and is not part of its width. Where
            // the line's last run is written from right to left, the library leaves that space
            // at the run's left end, among the words, and counts it; everything to its right
            // is moved over it, so that the words stand where they belong.
            let range = line.text_range();
            let ends_at_space = text.get(range.clone()).is_some_and(|line| line.ends_with(' '));
            let space = line
                .items()
                .filter_map(|item| match item {
                    PositionedLayoutItem::GlyphRun(run) => Some(run),
                    PositionedLayoutItem::InlineBox(_) => None,
                })
                .find(|run| ends_at_space && run.run().is_rtl() && run.run().text_range().end == range.end)
                .map(|run| run.offset());
            let hang = if space.is_some() { space_width } else { 0.0 };
            let moved = |x: f32| match space {
                Some(space) if x > space + 0.01 => x - hang,
                _ => x,
            };
            let pieces = line.items().map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => {
                    let mut placed = glyph_run(&run);
                    for glyph in &mut placed.glyphs {
                        glyph.x = moved(glyph.x);
                    }
                    Piece::Run(placed)
                }
                PositionedLayoutItem::InlineBox(placed) => Piece::Box(placed.id as usize, f64::from(moved(placed.x))),
            });
            FlowedLine {
                width: f64::from(metrics.advance - metrics.hanging_advance - hang),
                pieces: pieces.collect(),
            }
        });
        Flowed { lines: lines.collect() }
    }

    /// Shapes `text` as one unbroken piece.
    pub fn shape(&mut self, text: &str, style: &TextStyle) -> Rc<Shaped> {
        let key = (text.to_string(), shaping_key(style));
        if let Some(shaped) = self.words.get(&key) {
            return Rc::clone(shaped);
        }
        let layout = self.layout(text, style);
        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else { continue };
                runs.push(glyph_run(&run));
            }
        }
        let shaped = Rc::new(Shaped {
            runs,
            width: f64::from(layout.full_width()),
        });
        self.words.insert(key, Rc::clone(&shaped));
        shaped
    }

    /// The width of a space between two words of `style`.
    pub fn space(&mut self, style: &TextStyle) -> f64 {
        self.shape(" ", style).width
    }

    /// How a line of `style` stands around its baseline: the font's ascent and descent rounded
    /// to whole pixels, and the rest of the line height shared above and below them.
    pub fn line_box(&mut self, style: &TextStyle) -> LineBox {
        let key = format!("{}|{:?}", shaping_key(style), style.line_height);
        if let Some(line) = self.lines.get(&key) {
            return *line;
        }
        let layout = self.layout("x", style);
        let mut measures = (f64::from(style.size) * 0.8, f64::from(style.size) * 0.2, 0.0);
        if let Some(run) = layout.lines().next().and_then(|line| line.runs().next()) {
            let metrics = run.font_metrics();
            measures = (f64::from(metrics.ascent), f64::from(metrics.descent), f64::from(metrics.leading));
        }
        let (ascent, descent) = (measures.0.round(), measures.1.round());
        let height = match style.line_height {
            LineHeight::Normal => ascent + descent + measures.2.round(),
            LineHeight::Multiple(times) => f64::from(times) * f64::from(style.size),
            LineHeight::Px(px) => f64::from(px),
        };
        let above = ((height - (ascent + descent)) / 2.0).floor() + ascent;
        let line = LineBox {
            above,
            below: height - above,
        };
        self.lines.insert(key, line);
        line
    }
}

/// Gives the builder the look of `style`.
fn styled(builder: &mut RangedBuilder<'_, ()>, style: &TextStyle) {
    builder.push_default(StyleProperty::FontFamily(FontFamily::Source(Cow::Borrowed(&style.family))));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(style.weight)));
    let slant = if style.italic { FontStyle::Italic } else { FontStyle::Normal };
    builder.push_default(StyleProperty::FontStyle(slant));
    builder.push_default(StyleProperty::FontSize(style.size));
    builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing));
    if !style.figures.is_empty() {
        builder.push_default(StyleProperty::FontFeatures(style.figures.as_str().into()));
    }
}

/// The glyphs of a run, placed from the left end of its line's baseline.
fn glyph_run(run: &parley::GlyphRun<'_, ()>) -> GlyphRun {
    let baseline = run.baseline();
    let font_run = run.run();
    GlyphRun {
        font: font_run.font().clone(),
        size: font_run.font_size(),
        glyphs: run
            .positioned_glyphs()
            .map(|g| PlacedGlyph {
                id: g.id,
                x: g.x,
                y: g.y - baseline,
            })
            .collect(),
        coords: font_run.normalized_coords().iter().map(|coord| coord.to_bits()).collect(),
        skew: font_run.synthesis().skew().map(|degrees| f64::from(degrees).to_radians().tan()),
    }
}
