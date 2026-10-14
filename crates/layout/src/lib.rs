//! Places and sizes (`docs/spec/render-command.md` sections 3 and 4, `docs/spec/notation.md`
//! section 7.1).
//!
//! This version places an element that stands directly in the frame by its own `left`, `top`,
//! `width` and `height`, each a whole number of pixels. Rows, columns, percentages and the other
//! edges come with the tickets that bring them.

/// The most pixels an edge or a size may be in this version: beyond it a value is not placed.
pub const LIMIT: i64 = 1_000_000;

/// A rectangle in the plane of the frame, in whole pixels. `left` and `top` may be negative or
/// beyond the frame; the part inside the frame is what is seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// The pixels of a rectangle that lie inside the frame: columns `left` to `right` less one, and
/// rows `top` to `bottom` less one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Rect {
    /// The part of the rectangle that lies inside a frame of `width` by `height` pixels, or `None`
    /// when no pixel of it does (a size of 0 has none).
    pub fn visible(&self, width: u32, height: u32) -> Option<Visible> {
        let left = i64::from(self.left).max(0);
        let top = i64::from(self.top).max(0);
        let right = (i64::from(self.left) + i64::from(self.width)).min(i64::from(width));
        let bottom = (i64::from(self.top) + i64::from(self.height)).min(i64::from(height));
        if left >= right || top >= bottom {
            return None;
        }
        Some(Visible {
            left: left as u32,
            top: top as u32,
            right: right as u32,
            bottom: bottom as u32,
        })
    }
}

/// What an element writes of its place. A value not written is `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Written {
    pub left: Option<i32>,
    pub top: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// The place of an element that stands directly in a frame of `width` by `height` pixels: `left`
/// and `top` are the distance of its edges from the frame's (0 when not written), and a size not
/// written is the frame's size (`notation.md` section 7.1: a `<box>` is as large as its parent).
pub fn place(width: u32, height: u32, written: Written) -> Rect {
    Rect {
        left: written.left.unwrap_or(0),
        top: written.top.unwrap_or(0),
        width: written.width.unwrap_or(width),
        height: written.height.unwrap_or(height),
    }
}
