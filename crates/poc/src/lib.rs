//! Proof of concept of the native path: an XML composition is parsed and checked, turned into a
//! timeline model, evaluated frame by frame, drawn by a renderer of our own into a buffer we own,
//! and handed to the encoder as raw pixels. No browser takes part.

pub mod anim;
pub mod audio;
pub mod count;
pub mod cpu;
pub mod decode;
pub mod encode;
#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "gpu")]
pub mod gpu_effects;
pub mod gpu_footage;
pub mod layout;
pub mod lut;
pub mod model;
pub mod noise;
pub mod outline;
pub mod paint;
pub mod project;
pub mod scene;
pub mod shared;
pub mod style;
pub mod text;
pub mod xml;
pub mod yuv;

use scene::Op;
use style::ColorOp;

/// A renderer: the same frame description in, the same picture out.
pub trait Backend {
    fn name(&self) -> &'static str;
    /// Where the next picture of footage `video` is to be written before the frame is drawn:
    /// RGBA, 8 bits each, `width * height * 4` bytes. It keeps its content between frames.
    fn video_buffer(&mut self, video: usize, width: u32, height: u32) -> &mut [u8];
    /// Whether footage with this colour filter may be given as YUV planes, for the renderer to
    /// turn into RGBA and colour itself.
    fn takes_planes(&self, _filter: &[ColorOp]) -> bool {
        false
    }
    /// Where the next picture of footage `video` is to be written before the frame is drawn,
    /// for a renderer that takes planes: all of Y, then Cb, then Cr, `yuv::size` bytes.
    fn video_planes(&mut self, _video: usize, _width: u32, _height: u32, _filter: &[ColorOp]) -> &mut [u8] {
        unreachable!("this renderer does not take planes")
    }
    /// Lets go of the picture of footage `video`: its clip is over.
    fn release_video(&mut self, video: usize);
    /// Takes the description of frame number `frame`. The picture may still be on its way
    /// when this returns.
    fn draw(&mut self, frame: u64, ops: &[Op]) -> Result<(), String>;
    /// How many frames may be drawn before the pixels of the first of them are taken.
    fn ahead(&self) -> usize {
        1
    }
    /// Asks the renderer to hand its frames over as the YUV planes the encoder is given, from
    /// the next frame drawn. Returns whether it will; one that will not goes on with RGBA.
    fn hand_over_planes(&mut self) -> bool {
        false
    }
    /// The pixels of the oldest frame drawn and not yet taken, waited for when they are not
    /// ready: RGBA, 8 bits each, top row first; or, after `hand_over_planes` said yes, the
    /// planes: all of Y, then Cb, then Cr.
    fn pixels(&mut self) -> Result<&[u8], String>;
}
