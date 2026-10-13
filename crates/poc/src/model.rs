//! The timeline model of a composition, as the renderer reads it: a tree of elements.

use crate::anim::{Track, Values};
use crate::count::Format;
use crate::style::{ColorOp, Paint, Rgba, Shadow};
use std::path::PathBuf;
use timebase::Time;
use vello_cpu::kurbo::{BezPath, Rect};
use vello_cpu::peniko::FontData;

pub struct Composition {
    pub width: u16,
    pub height: u16,
    pub fps: u32,
    pub duration: Time,
    pub background: Rgba,
    /// The frame itself: a box that holds every element, in document order. A later element is
    /// drawn over an earlier one.
    pub root: Node,
    pub fonts: Vec<FontSource>,
    pub audio: Vec<Audio>,
    /// How many videos and still pictures the elements name.
    pub videos: usize,
    pub stills: Vec<Still>,
    /// The openings the composition's elements name that are written as shaders.
    pub shaders: Vec<Shader>,
}

/// An opening written as a shader: the file's name as the composition gives it, and its text.
pub struct Shader {
    pub name: String,
    pub source: String,
    /// The line of the first element that names it.
    pub line: u32,
}

/// A font file, and the family name it is used under when that is not the file's own.
pub struct FontSource {
    pub source: PathBuf,
    pub family: Option<String>,
    pub line: u32,
}

/// A still picture file. Every element that shows the same file the same way shares one.
#[derive(Clone, Debug, PartialEq)]
pub struct Still {
    pub source: PathBuf,
    pub filter: Vec<ColorOp>,
    /// A fade of the picture's alpha, as a gradient over its box.
    pub mask: Option<Paint>,
}

/// One sound on the timeline. Times are in seconds of the composition.
#[derive(Clone, Debug, PartialEq)]
pub struct Audio {
    pub id: String,
    pub source: PathBuf,
    pub start: f64,
    pub duration: f64,
    pub media_start: f64,
    pub volume: f64,
    /// Volume over time, counted from the start of the sound: `(seconds, gain)`, in order.
    /// The gain between two points changes evenly; it multiplies `volume`.
    pub automation: Vec<(f64, f64)>,
    pub line: u32,
}

/// A length that is a number of pixels or a share of the parent's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dim {
    Px(f64),
    Share(f64),
}

impl Dim {
    pub fn of(self, whole: f64) -> f64 {
        match self {
            Dim::Px(value) => value,
            Dim::Share(share) => whole * share,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geometry {
    pub left: Option<Dim>,
    pub top: Option<Dim>,
    pub right: Option<Dim>,
    pub bottom: Option<Dim>,
    pub width: Option<Dim>,
    pub height: Option<Dim>,
    /// The widest the element may be when its width is not its own `width`, in pixels.
    pub max_width: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Centre,
    End,
    Stretch,
    /// Along a row or column only: the room left over lies between the items.
    Between,
    /// Across a row only: the first lines of the items stand on one line.
    Baseline,
}

/// A box that lays its children out in a row or a column, as a CSS flex container does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flex {
    pub direction: Direction,
    pub justify: Align,
    pub align: Align,
    pub gap: f64,
}

/// The width of one column of a box with `columns`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Column {
    Px(f64),
    /// A share of the room the pixel columns and the gaps leave.
    Share(f64),
}

/// A box that lays its children out in columns, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Columns {
    pub widths: Vec<Column>,
    pub align: Align,
    pub gap: f64,
}

/// How an element takes part in its parent's layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub flex: Option<Flex>,
    pub columns: Option<Columns>,
    /// Top, right, bottom, left.
    pub padding: [f64; 4],
    pub margin: [f64; 4],
    pub align_self: Option<Align>,
    pub grow: f64,
    /// Placed by `left`, `top`, `right`, `bottom` even inside a row or a column.
    pub absolute: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Blend {
    /// Every blend with the name it is written by. See `docs/spec/native-notation.md`,
    /// section 16.
    pub const NAMED: [(&'static str, Blend); 16] = [
        ("normal", Blend::Normal),
        ("multiply", Blend::Multiply),
        ("screen", Blend::Screen),
        ("overlay", Blend::Overlay),
        ("darken", Blend::Darken),
        ("lighten", Blend::Lighten),
        ("color-dodge", Blend::ColorDodge),
        ("color-burn", Blend::ColorBurn),
        ("hard-light", Blend::HardLight),
        ("soft-light", Blend::SoftLight),
        ("difference", Blend::Difference),
        ("exclusion", Blend::Exclusion),
        ("hue", Blend::Hue),
        ("saturation", Blend::Saturation),
        ("color", Blend::Color),
        ("luminosity", Blend::Luminosity),
    ];
}

/// One layer of a background.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub paint: Paint,
    /// The size of one tile. Without it the layer is as large as the box.
    pub size: Option<(Dim, Dim)>,
    /// Where the tile lies: pixels from the box's corner, or a share of the space left over.
    pub position: (Dim, Dim),
    pub repeat: bool,
}

impl Layer {
    /// A layer as large as its box.
    pub fn whole(paint: Paint) -> Layer {
        Layer {
            paint,
            size: None,
            position: (Dim::Px(0.0), Dim::Px(0.0)),
            repeat: true,
        }
    }

    /// The tile of the layer in a box of this size, when it is not simply the box itself.
    pub fn tile(&self, width: f64, height: f64) -> Option<Tile> {
        let (w, h) = self.size.map_or((width, height), |(w, h)| (w.of(width), h.of(height)));
        let place = |at: Dim, room: f64| match at {
            Dim::Px(px) => px,
            Dim::Share(share) => room * share,
        };
        let (x, y) = (place(self.position.0, width - w), place(self.position.1, height - h));
        let rect = Rect::new(x, y, x + w, y + h);
        let same = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let is_box = same(x, 0.0) && same(y, 0.0) && same(w, width) && same(h, height);
        (!is_box).then_some(Tile { rect, repeat: self.repeat })
    }
}

/// One tile of a background layer, in the element's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub rect: Rect,
    pub repeat: bool,
}

/// What an element's own box looks like.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// The first is on top.
    pub background: Vec<Layer>,
    pub radius: f64,
    pub border: Option<(f64, Rgba)>,
    /// Whether the border is two lines with a gap, each a third of its width.
    pub double: bool,
    pub shadows: Vec<Shadow>,
    /// Whether the children are cut off at the box.
    pub clip: bool,
    /// A polygon the element and its children are cut to: its corners, as shares of the box.
    pub clip_path: Option<Vec<(f64, f64)>>,
    pub blend: Blend,
    /// The distance of the eye from the frame, for an element that can be turned in space.
    pub perspective: Option<f64>,
    /// How the element opens as its `reveal` goes from 0 to 1.
    pub transition: Option<Opening>,
    /// What is done to the finished picture of the element and its children.
    pub finish: Finish,
}

/// A field of numbers between 0 and 1 over an element's box: the fractal noise of the W3C's
/// Filter Effects recommendation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseField {
    /// The base frequency, the same across and down.
    pub frequency: f64,
    pub octaves: u32,
    pub seed: i32,
}

/// What is done to the finished picture of an element, in this order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Finish {
    /// Every pixel is taken from as far on as the noise says: the amount, and the noise.
    pub displace: Option<(f64, NoiseField)>,
    /// The alpha is multiplied by the noise times a slope plus an offset.
    pub grain: Option<(NoiseField, f64, f64)>,
    /// A shadow of the picture, under it.
    pub shadow: Option<Shadow>,
}

impl Finish {
    pub fn any(&self) -> bool {
        self.displace.is_some() || self.grain.is_some() || self.shadow.is_some()
    }
}

/// How an element with a transition opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// From its left edge to its right.
    Wipe,
    /// From its left edge to its right, with a hard edge: a cut of its box.
    Cut,
    /// A circle that grows from its middle.
    Iris,
    /// As a shader of the composition says, counted from 0 in the composition's list.
    Shader(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    /// What the font itself asks for.
    Normal,
    Multiple(f32),
    Px(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Centre,
    Right,
}

/// The text properties. A child starts from its parent's.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    pub weight: f32,
    pub italic: bool,
    pub size: f32,
    pub line_height: LineHeight,
    /// In pixels.
    pub letter_spacing: f32,
    pub color: Rgba,
    pub upper: bool,
    pub align: TextAlign,
    pub shadows: Vec<Shadow>,
    pub nowrap: bool,
    /// An outline around every glyph: width and colour.
    pub stroke: Option<(f64, Rgba)>,
    /// The kind of figures asked for, as a list of OpenType features; empty for the font's own.
    pub figures: String,
}

impl Default for TextStyle {
    fn default() -> TextStyle {
        TextStyle {
            family: "sans-serif".to_string(),
            weight: 400.0,
            italic: false,
            size: 16.0,
            line_height: LineHeight::Normal,
            letter_spacing: 0.0,
            color: [0, 0, 0, 255],
            upper: false,
            align: TextAlign::Left,
            shadows: Vec::new(),
            nowrap: false,
            stroke: None,
            figures: String::new(),
        }
    }
}

/// A part of the transform origin: pixels from the box's corner, or a share of its size.
pub type Origin = (Dim, Dim);

pub enum Kind {
    Box,
    /// A block of text. Its words and its `<span>` children flow in lines.
    Text,
    /// A piece of a text that keeps together and can be moved on its own.
    Span,
    /// `still` is the picture's place in `Composition::stills`.
    Image {
        still: usize,
        fit: Fit,
    },
    /// Footage. `video` counts the videos of the composition from 0.
    Video {
        source: PathBuf,
        video: usize,
        media_start: Time,
        filter: Vec<ColorOp>,
    },
    /// Outlines, in the units of `view` and scaled to the element's box.
    Shape {
        paths: Vec<BezPath>,
        view: Rect,
        /// Stroke width in the units of `view`; `None` fills the outlines.
        stroke: Option<f64>,
        color: Rgba,
        /// A shadow that follows the outlines, in the units of `view`.
        shadow: Option<Shadow>,
        /// Pairs of lengths, a dash and a gap, in the units of `view`; empty for a whole line.
        dash: Vec<f64>,
        /// The outlines as dashes, when there are dashes.
        dashed: Vec<BezPath>,
    },
}

/// How a picture meets its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub kind: FitKind,
    /// Where the picture is held, across and down, as shares: 0 at its left or top edge.
    pub hold: (f64, f64),
}

impl Fit {
    /// Stretched to the box.
    pub const FILL: Fit = Fit {
        kind: FitKind::Fill,
        hold: (0.5, 0.5),
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitKind {
    /// Stretched to the box.
    Fill,
    /// Scaled evenly until it covers the box.
    Cover,
    /// Scaled evenly until all of it is in the box.
    Contain,
}

/// What a text or a span holds, in order.
pub enum Inline {
    Words(String),
    /// A child, by its place among the children.
    Child(usize),
}

pub struct Node {
    pub id: String,
    pub line: u32,
    /// Unique in the composition; names the element in caches.
    pub key: usize,
    pub kind: Kind,
    pub geometry: Geometry,
    /// Start and duration, when the element is a clip. Its children count their times from its
    /// start; the element's own tweens are in its parent's time, as `start` is.
    pub window: Option<(Time, Time)>,
    /// The value of each animated property without the tweens.
    pub base: Values,
    pub origin: Origin,
    pub tracks: Vec<Track>,
    pub look: Look,
    pub flow: Flow,
    pub text: TextStyle,
    pub inline: Vec<Inline>,
    pub children: Vec<Node>,

    // Filled in by the layout.
    /// The element's box, relative to its parent's.
    pub rect: Rect,
    /// The glyphs of the element's own words, placed relative to its box.
    pub runs: Vec<GlyphRun>,
    /// For a text or a span: from the top of its box to the baseline of its first line.
    pub baseline: f64,
    /// Whether the element is drawn apart from what is under it, so that a child that blends
    /// (multiply, screen) blends only with the element's own content. An element that is moved,
    /// scaled or turned does this, as in CSS.
    pub isolates: bool,
    /// For a typed text: the layout for each count of characters shown, made when first needed.
    pub typed: Vec<Option<TypedLayout>>,
    /// For a text that counts: how its number is written.
    pub count: Option<Format>,
    /// For a text that counts: every text it shows in a frame of the composition, in order,
    /// each with its glyphs.
    pub counted: Vec<(String, Vec<GlyphRun>)>,
}

/// The layout of a text of which only the first characters show.
pub struct TypedLayout {
    pub runs: Vec<GlyphRun>,
    /// The boxes of the children, in their order.
    pub children: Vec<Rect>,
}

/// Glyphs of one font, placed relative to the top left corner of the element's box.
#[derive(Clone)]
pub struct GlyphRun {
    pub font: FontData,
    pub size: f32,
    pub glyphs: Vec<PlacedGlyph>,
    /// Where the font stands on its variation axes.
    pub coords: Vec<i16>,
    /// The slant of an italic the font does not have, as the tangent of its angle.
    pub skew: Option<f64>,
}

#[derive(Clone, Copy)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}
