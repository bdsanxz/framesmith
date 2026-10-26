//! Still pictures in their boxes (`docs/spec/still-pictures.md`): the picture's pixels, how it is fitted into its box,
//! and the rectangle it is drawn in.

use std::sync::Arc;

use layout::Rect;

use crate::Mark;

/// The pixels of a still picture, premultiplied by alpha: RGBA of 8 bits a channel, rows top first, with no padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Still {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Still {
    /// The picture of pixels not multiplied by alpha, as frx7 gives them, with each colour value multiplied by its alpha,
    /// rounded to the nearest whole level, halves up (section 2, point 4). `None` when the pixels are not `width` by
    /// `height` of four bytes.
    pub fn from_straight(width: u32, height: u32, straight: &[u8]) -> Option<Still> {
        if straight.len() as u64 != u64::from(width) * u64::from(height) * 4 {
            return None;
        }
        let mut pixels = straight.to_vec();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = u32::from(pixel[3]);
            for value in &mut pixel[..3] {
                *value = ((u32::from(*value) * alpha * 2 + 255) / 510) as u8;
            }
        }
        Some(Still { width, height, pixels })
    }
}

/// How a picture is fitted into its box (`notation.md` section 7.10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// Stretched to the box.
    #[default]
    Fill,
    /// Scaled evenly until it covers the box; what reaches beyond is not drawn.
    Cover,
    /// Scaled evenly until all of it is in the box.
    Contain,
}

/// A rectangle whose edges may lie between pixels, in pixels of the frame before any map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    pub fn right(&self) -> f64 {
        self.left + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.top + self.height
    }

    /// The part of this rectangle inside `other`, or `None` when they do not overlap.
    pub fn within(&self, other: &Area) -> Option<Area> {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (left < right && top < bottom).then_some(Area {
            left,
            top,
            width: right - left,
            height: bottom - top,
        })
    }
}

/// A still picture in its box, with its fit and its place (`docs/spec/still-pictures.md`, section 5).
#[derive(Clone, Debug, PartialEq)]
pub struct StillMark {
    /// The element's box.
    pub rect: Rect,
    pub still: Arc<Still>,
    pub fit: Fit,
    /// The place across and down, as shares of the room: `0.0` for `0%`, `1.0` for `100%`.
    pub position: (f64, f64),
}

impl StillMark {
    /// The mark of the picture in its box.
    pub fn mark(self) -> Mark {
        Mark::Still(self)
    }

    /// The box as a rectangle of the plane.
    pub fn box_area(&self) -> Area {
        Area {
            left: f64::from(self.rect.left),
            top: f64::from(self.rect.top),
            width: f64::from(self.rect.width),
            height: f64::from(self.rect.height),
        }
    }

    /// The picture's place, `(dx, dy, dw, dh)` of section 5, before it is cut to the box. `None` for a box or a picture
    /// of no size.
    pub fn place(&self) -> Option<Area> {
        let area = self.box_area();
        let (width, height) = (f64::from(self.still.width), f64::from(self.still.height));
        if area.width <= 0.0 || area.height <= 0.0 || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let scale = match self.fit {
            Fit::Fill => return Some(area),
            Fit::Cover => (area.width / width).max(area.height / height),
            Fit::Contain => (area.width / width).min(area.height / height),
        };
        let (drawn_width, drawn_height) = (width * scale, height * scale);
        Some(Area {
            left: area.left + (area.width - drawn_width) * self.position.0,
            top: area.top + (area.height - drawn_height) * self.position.1,
            width: drawn_width,
            height: drawn_height,
        })
    }

    /// What is drawn: the picture's place cut to the box, `D` of section 5.
    pub fn shown(&self) -> Option<Area> {
        self.place()?.within(&self.box_area())
    }
}
