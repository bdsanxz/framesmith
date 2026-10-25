//! The list of drawing steps of one frame, the picture a renderer makes of it, and the trait every
//! renderer implements (`docs/spec/render-command.md` section 4).
//!
//! A frame is a pure function of the composition and the frame's number: a renderer is given the
//! whole list of a frame and nothing else, and gives the same picture for the same list.

use std::fmt;
use std::sync::Arc;

use layout::Rect;

/// An opaque colour: red, green and blue, each from 0 to 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// One step of a frame. The steps of a list are carried out in order, each over what is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Every pixel of the rectangle that lies inside the frame becomes `colour`.
    Fill { rect: Rect, colour: Colour },
}

/// Everything that decides the picture of one frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayList {
    pub width: u32,
    pub height: u32,
    /// The colour every pixel has before the first step.
    pub background: Colour,
    pub steps: Vec<Step>,
}

/// A font face a glyph run is drawn with: the bytes of a TrueType font file (`docs/spec/text.md`, section 2.1)
/// and the index of the face in them.
#[derive(Clone, Debug)]
pub struct Font {
    pub data: Arc<Vec<u8>>,
    pub index: u32,
}

/// One glyph of a run: its index in the font, and the place of its origin on the frame, in pixels, with `y`
/// the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// A run of glyphs that share a font, a size, a skew and a colour. `skew_degrees` shears each glyph about its
/// origin: a point `h` pixels above the baseline moves right by `h` times the tangent of the angle (0 for none).
#[derive(Clone, Debug)]
pub struct GlyphRun {
    pub font: Font,
    pub size: f32,
    pub skew_degrees: f32,
    pub colour: Colour,
    pub glyphs: Vec<Glyph>,
}

/// A colour picture of an emoji (`docs/spec/text.md`, section 12.4): premultiplied RGBA8 pixels in rows with no padding,
/// its size, and the top-left corner of its place on the frame in pixels, not yet a whole pixel. A renderer places the
/// corner at the whole pixel nearest to it, halves up, and draws the picture in its own colours, whatever the colour of its
/// text.
#[derive(Clone, Debug)]
pub struct GlyphPicture {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
    pub x: f32,
    pub y: f32,
}

/// A shadow of a paragraph (`docs/spec/text.md`, section 6.6): the alpha of its glyphs moved by `across` pixels to the
/// right and `down` pixels down, blurred by `blur` as notation 7.6 blurs a picture, and laid in `colour` under the glyphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub across: f32,
    pub down: f32,
    pub blur: f32,
    pub colour: Colour,
}

/// An outline of a paragraph (`docs/spec/text.md`, section 6.6): a line `width` pixels wide, centred on the outline of each
/// glyph and laid in `colour` over the glyph fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub width: f32,
    pub colour: Colour,
}

/// The glyph runs and the colour pictures of one paragraph, drawn together: the shadows first, the last listed first so that
/// the first listed is on top, then the runs in the order they are listed, the outline over the fills, and the pictures in
/// the order of the text. Only the pixels of `clip` change; `None` for the whole frame. A paragraph in a composition shown
/// from another file is cut to the rectangle that file is shown in (`docs/spec/text.md`, section 8).
#[derive(Clone, Debug)]
pub struct TextMark {
    pub runs: Vec<GlyphRun>,
    pub pictures: Vec<GlyphPicture>,
    pub clip: Option<Rect>,
    pub shadows: Vec<Shadow>,
    pub outline: Option<Outline>,
}

/// One mark of a frame: a drawing step, or the glyph runs of a paragraph of text. The marks are carried out in
/// order, each over what is there.
#[derive(Clone, Debug)]
pub enum Mark {
    Step(Step),
    Text(TextMark),
}

/// Everything that decides the picture of one frame, with the text of the frame: the same frame gives the same
/// picture, whatever was drawn before.
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub background: Colour,
    pub marks: Vec<Mark>,
}

impl Frame {
    /// The frame of a list of steps, with no text.
    pub fn from_display_list(list: &DisplayList) -> Frame {
        Frame {
            width: list.width,
            height: list.height,
            background: list.background,
            marks: list.steps.iter().map(|step| Mark::Step(*step)).collect(),
        }
    }

    /// Whether the frame holds a paragraph of text.
    pub fn has_text(&self) -> bool {
        self.marks.iter().any(|mark| matches!(mark, Mark::Text(_)))
    }

    /// The steps of the frame, when it holds no text; `None` for a frame with text.
    pub fn display_list(&self) -> Option<DisplayList> {
        if self.has_text() {
            return None;
        }
        Some(DisplayList {
            width: self.width,
            height: self.height,
            background: self.background,
            steps: self
                .marks
                .iter()
                .filter_map(|mark| match mark {
                    Mark::Step(step) => Some(*step),
                    Mark::Text(_) => None,
                })
                .collect(),
        })
    }
}

/// A picture: `width` by `height` pixels of four bytes each, red, green, blue and alpha, row by row
/// from the top, with no padding between rows. This version draws opaque pictures: alpha is 255.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Picture {
    /// A picture from its bytes; `None` when their number is not `width * height * 4`.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Picture> {
        (pixels.len() as u64 == u64::from(width) * u64::from(height) * 4).then_some(Picture { width, height, pixels })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The bytes of the picture: red, green, blue, alpha, pixel after pixel.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The frame mark of the picture (`docs/spec/render-command.md`, section 7.1): the 64-bit FNV-1a hash of the width and
    /// the height, each as four bytes with the lowest first, then of the pixels' bytes. Equal pictures have equal marks.
    pub fn mark(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let size = self.width.to_le_bytes().into_iter().chain(self.height.to_le_bytes());
        size.chain(self.pixels.iter().copied())
            .fold(OFFSET, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(PRIME))
    }

    /// The four bytes of the pixel in column `x` of row `y`, or `None` outside the picture.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        Some([self.pixels[at], self.pixels[at + 1], self.pixels[at + 2], self.pixels[at + 3]])
    }
}

/// Why a renderer could not make a picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Error {
        Error(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// A renderer: it makes the picture of a list of steps.
pub trait Renderer {
    /// The renderer's name as a summary says it: `processor` for the renderer on the processor.
    fn name(&self) -> &'static str;

    /// Draws a frame. The same list gives the same picture, whatever was drawn before.
    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error>;

    /// Whether this renderer draws the glyph runs of text (`docs/spec/text.md`, section 9). A renderer that does not
    /// refuses a frame with text, and `render` refuses such a composition before its first frame.
    fn draws_text(&self) -> bool {
        false
    }

    /// Draws a frame with its marks. A frame with no text is drawn by [`Renderer::draw`]; a frame with text
    /// is refused by a renderer that does not draw text.
    fn draw_frame(&mut self, frame: &Frame) -> Result<Picture, Error> {
        match frame.display_list() {
            Some(list) => self.draw(&list),
            None => Err(Error::new(format!("the renderer {} does not draw text", self.name()))),
        }
    }
}
