//! The renderer on the processor (`docs/spec/render-command.md` section 4). It defines the correct
//! picture: the renderer on the card must agree with it within the tolerance of `PLAN.md`
//! section 5, and it is tested exactly, against checked-in pictures and worked values.
//!
//! The marks of a frame are carried out in order. A fill is written into the picture. A paragraph of text
//! is filled by vello_cpu into a layer of its own, which is then laid over the picture with each pixel's
//! coverage, the same integer rounding for every pixel, so that the picture does not depend on anything but
//! the frame.

use std::sync::Arc;

use draw::{DisplayList, Error, Frame, GlyphRun, Mark, Picture, Renderer, Step, TextMark};
use vello_cpu::kurbo::Affine;
use vello_cpu::peniko::{Blob, Color, FontData};
use vello_cpu::{Glyph, Pixmap, RenderContext, Resources};

/// The most pixels of a frame this renderer holds (a frame of 8192 by 8192 has fewer).
const MOST_PIXELS: u64 = 8192 * 8192;

/// The renderer on the processor.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuRenderer;

impl Renderer for CpuRenderer {
    fn name(&self) -> &'static str {
        "processor"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        self.draw_frame(&Frame::from_display_list(list))
    }

    fn draws_text(&self) -> bool {
        true
    }

    fn draw_frame(&mut self, frame: &Frame) -> Result<Picture, Error> {
        let pixels = u64::from(frame.width) * u64::from(frame.height);
        if pixels == 0 || pixels > MOST_PIXELS {
            return Err(Error::new(format!(
                "a frame of {}x{} pixels cannot be drawn: it has none or more than {MOST_PIXELS}",
                frame.width, frame.height
            )));
        }
        let background = [frame.background.red, frame.background.green, frame.background.blue, 255];
        let mut bytes = Vec::with_capacity(pixels as usize * 4);
        for _ in 0..pixels {
            bytes.extend_from_slice(&background);
        }
        for mark in &frame.marks {
            match mark {
                Mark::Step(step) => fill(&mut bytes, frame.width, frame.height, step),
                Mark::Text(text) => draw_text(&mut bytes, frame.width, frame.height, text)?,
            }
        }
        Picture::new(frame.width, frame.height, bytes).ok_or_else(|| Error::new("the picture does not have the bytes of its size"))
    }
}

/// Writes one fill step into the picture: every pixel of its rectangle that lies inside the frame.
fn fill(bytes: &mut [u8], width: u32, height: u32, step: &Step) {
    let Step::Fill { rect, colour } = *step;
    let Some(visible) = rect.visible(width, height) else {
        return;
    };
    let row_length = width as usize;
    let fill = [colour.red, colour.green, colour.blue, 255];
    for row in visible.top..visible.bottom {
        let start = (row as usize * row_length + visible.left as usize) * 4;
        let end = (row as usize * row_length + visible.right as usize) * 4;
        for pixel in bytes[start..end].as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&fill);
        }
    }
}

/// Lays the glyph runs of a paragraph over the picture: they are filled into a layer of the frame's size,
/// which is then composited with each pixel's coverage, inside the paragraph's clip.
fn draw_text(bytes: &mut [u8], width: u32, height: u32, text: &TextMark) -> Result<(), Error> {
    let region = match &text.clip {
        None => (0, 0, width, height),
        Some(clip) => match clip.visible(width, height) {
            Some(visible) => (visible.left, visible.top, visible.right, visible.bottom),
            None => return Ok(()),
        },
    };
    let mut context = RenderContext::new(width as u16, height as u16);
    let mut resources = Resources::new();
    for run in &text.runs {
        draw_run(&mut context, &mut resources, run)?;
    }
    context.flush();
    let mut layer = Pixmap::new(width as u16, height as u16);
    context.render(&mut layer, &mut resources);
    composite(bytes, width, layer.data_as_u8_slice(), region);
    Ok(())
}

/// Fills the glyphs of one run in its colour, sheared by its skew about each glyph's origin.
fn draw_run(context: &mut RenderContext, resources: &mut Resources, run: &GlyphRun) -> Result<(), Error> {
    let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = run.font.data.clone();
    let font = FontData::new(Blob::new(bytes), run.font.index);
    context.set_paint(Color::from_rgba8(run.colour.red, run.colour.green, run.colour.blue, 255));
    let shear = f64::from(run.skew_degrees).to_radians().tan();
    let transform = Affine::new([1.0, 0.0, -shear, 1.0, 0.0, 0.0]);
    let glyphs = run.glyphs.iter().map(|glyph| Glyph {
        id: glyph.id,
        x: glyph.x,
        y: glyph.y,
    });
    context
        .glyph_run(resources, &font)
        .font_size(run.size)
        .glyph_transform(transform)
        .fill_glyphs(glyphs)
        .map_err(|error| Error::new(format!("the glyphs of a run cannot be drawn: {error}")))
}

/// Lays a layer of premultiplied red, green, blue and alpha over the picture's bytes, which are opaque, in the
/// region (`left`, `top`, `right`, `bottom`; the right and bottom edges are not in it). The colour of a pixel is
/// its layer colour plus the picture's colour times what the layer leaves clear, each rounded to the nearest
/// whole level.
fn composite(bytes: &mut [u8], width: u32, layer: &[u8], (left, top, right, bottom): (u32, u32, u32, u32)) {
    for row in top..bottom {
        for column in left..right {
            let at = (row as usize * width as usize + column as usize) * 4;
            let over = &layer[at..at + 4];
            let alpha = u32::from(over[3]);
            if alpha == 0 {
                continue;
            }
            let pixel = &mut bytes[at..at + 4];
            for channel in 0..3 {
                let kept = u32::from(pixel[channel]) * (255 - alpha);
                pixel[channel] = (u32::from(over[channel]) + divide_by_255(kept)) as u8;
            }
            pixel[3] = 255;
        }
    }
}

/// `value / 255`, rounded to the nearest whole number, for values up to 255 times 255.
fn divide_by_255(value: u32) -> u32 {
    let rounded = value + 128;
    (rounded + (rounded >> 8)) >> 8
}
