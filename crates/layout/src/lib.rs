//! Places and sizes (`docs/spec/render-command.md` sections 3 and 4.1, `docs/spec/notation.md`
//! section 7.1).
//!
//! A box is placed in its parent's rectangle. Across, its `left`, `right` and `width` are worked out
//! with the parent's width; down, its `top`, `bottom` and `height` with the parent's height. Each
//! written amount is a number of pixels, or a percentage of that parent size. The place is worked out
//! exactly first, then each edge is rounded to a whole pixel (a half up), so the rectangle covers
//! whole pixels. Rows, columns and the other forms come with the tickets that bring them.

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

/// An amount written for one axis: a number of pixels, or a percentage of the parent's size on that
/// axis (`notation.md` section 4: a length).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    /// A number of pixels, as written.
    Pixels(f64),
    /// A percentage of the parent's width (across) or height (down): `Percent(50.0)` is half of it.
    Percent(f64),
}

/// What a box writes of its place and size. A value not written is `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Written {
    pub left: Option<Amount>,
    pub top: Option<Amount>,
    pub right: Option<Amount>,
    pub bottom: Option<Amount>,
    pub width: Option<Amount>,
    pub height: Option<Amount>,
}

/// A place that cannot be worked out within the limit: an edge or a size lies beyond [`LIMIT`]
/// pixels, or an amount is not a finite number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beyond;

/// The rectangle of a frame of `width` by `height` pixels: its corner is the origin.
pub fn frame(width: u32, height: u32) -> Rect {
    Rect {
        left: 0,
        top: 0,
        width,
        height,
    }
}

/// The place of a box in its parent's rectangle `parent`, in whole pixels (`render-command.md`
/// section 4.1). A size below 0 is 0: the scene reports a written one before it is placed, and a
/// size that two distances leave below 0 is 0.
pub fn place(parent: Rect, written: Written) -> Result<Rect, Beyond> {
    let (left, width) = axis(parent.left, parent.width, written.left, written.right, written.width)?;
    let (top, height) = axis(parent.top, parent.height, written.top, written.bottom, written.height)?;
    Ok(Rect { left, top, width, height })
}

/// One axis of a box: its whole-pixel start and size, in a parent that starts at `start` and is
/// `length` pixels long. `near` is the distance of the near edge (`left` or `top`), `far` that of
/// the far edge (`right` or `bottom`).
fn axis(start: i32, length: u32, near: Option<Amount>, far: Option<Amount>, size: Option<Amount>) -> Result<(i32, u32), Beyond> {
    let length = f64::from(length);
    let near = near.map(|amount| pixels(amount, length)).transpose()?;
    let far = far.map(|amount| pixels(amount, length)).transpose()?;
    let size = size.map(|amount| pixels(amount, length)).transpose()?;
    // The size: as written, or the space between two written edges, or the parent's whole size.
    let size = match (size, near, far) {
        (Some(size), _, _) => size,
        (None, Some(near), Some(far)) => length - near - far,
        _ => length,
    };
    if !size.is_finite() {
        return Err(Beyond);
    }
    let size = size.max(0.0);
    // The near edge: as written from the parent's near edge; or, with only a far edge, the far edge
    // lies that distance from the parent's far edge; or the parent's near edge.
    let relative = match (near, far) {
        (Some(near), _) => near,
        (None, Some(far)) => length - far - size,
        (None, None) => 0.0,
    };
    let edge = f64::from(start) + relative;
    let whole_near = whole(edge)?;
    let whole_far = whole(edge + size)?;
    let width = whole_far - whole_near;
    if width > LIMIT {
        return Err(Beyond);
    }
    Ok((whole_near as i32, width as u32))
}

/// An amount in pixels: a percentage of the parent's length, `percent` times `length` divided by 100.
fn pixels(amount: Amount, length: f64) -> Result<f64, Beyond> {
    let value = match amount {
        Amount::Pixels(pixels) => pixels,
        Amount::Percent(percent) => percent * length / 100.0,
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Beyond)
    }
}

/// A position in pixels rounded to a whole pixel, a half up: the larger whole number when it lies
/// exactly half way. Beyond the limit it is not placed.
fn whole(position: f64) -> Result<i64, Beyond> {
    let floor = position.floor();
    let rounded = if position - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded.abs() <= LIMIT as f64 {
        Ok(rounded as i64)
    } else {
        Err(Beyond)
    }
}
