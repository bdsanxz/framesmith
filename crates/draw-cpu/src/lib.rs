//! The renderer on the processor (`docs/spec/render-command.md` section 4). It defines the correct
//! picture: the renderer on the card must agree with it within the tolerance of `PLAN.md`
//! section 5, and it is tested exactly, against checked-in pictures and worked values.

use draw::{DisplayList, Error, Picture, Renderer, Step};

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
        let pixels = u64::from(list.width) * u64::from(list.height);
        if pixels == 0 || pixels > MOST_PIXELS {
            return Err(Error::new(format!(
                "a frame of {}x{} pixels cannot be drawn: it has none or more than {MOST_PIXELS}",
                list.width, list.height
            )));
        }
        let width = list.width as usize;
        let background = [list.background.red, list.background.green, list.background.blue, 255];
        let mut bytes = Vec::with_capacity(pixels as usize * 4);
        for _ in 0..pixels {
            bytes.extend_from_slice(&background);
        }
        for step in &list.steps {
            match *step {
                Step::Fill { rect, colour } => {
                    let Some(visible) = rect.visible(list.width, list.height) else {
                        continue;
                    };
                    let fill = [colour.red, colour.green, colour.blue, 255];
                    for row in visible.top..visible.bottom {
                        let start = (row as usize * width + visible.left as usize) * 4;
                        let end = (row as usize * width + visible.right as usize) * 4;
                        for pixel in bytes[start..end].as_chunks_mut::<4>().0 {
                            pixel.copy_from_slice(&fill);
                        }
                    }
                }
            }
        }
        Picture::new(list.width, list.height, bytes).ok_or_else(|| Error::new("the picture does not have the bytes of its size"))
    }
}
