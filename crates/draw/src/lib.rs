//! The list of drawing steps of one frame, the picture a renderer makes of it, and the trait every
//! renderer implements (`docs/spec/render-command.md` section 4).
//!
//! A frame is a pure function of the composition and the frame's number: a renderer is given the
//! whole list of a frame and nothing else, and gives the same picture for the same list.

use std::fmt;

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
}
