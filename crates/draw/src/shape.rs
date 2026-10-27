//! A `<shape>`: path data drawn in a view box, into the box of the element (`docs/spec/shape.md`, sections 3 to 5). The box
//! is in the frame's pixels, before the map of the element that moves it, and the path is in the view box's coordinates.

use crate::{Path, Rgba};

/// The view box of a shape: the rectangle of the path's coordinates that is drawn in the element's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A shape to draw: its path, filled with `colour`, or with `stroke_width` as a line of its outlines, dashed by `dashes`.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeMark {
    /// The box the shape is drawn in: its left and top edges and its size, in the frame's pixels.
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub view_box: ViewBox,
    pub path: Path,
    pub colour: Rgba,
    /// The width of the line, in view-box units; `None` for a fill.
    pub stroke_width: Option<f64>,
    /// Pairs of a dash and a gap, in view-box units; empty for a solid line.
    pub dashes: Vec<f64>,
}

/// How a view box is put in its box (`docs/spec/shape.md`, section 3): the scale of one view-box unit in pixels, and the
/// pixels the origin of the view box moves by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub scale: f64,
    pub across: f64,
    pub down: f64,
}

impl ShapeMark {
    /// The even scale of the view box in its box, the smaller of the two ratios, and the centring of the view box in the box.
    pub fn fit(&self) -> Fit {
        let scale = (self.width / self.view_box.width).min(self.height / self.view_box.height);
        Fit {
            scale,
            across: self.left + (self.width - self.view_box.width * scale) / 2.0 - self.view_box.x * scale,
            down: self.top + (self.height - self.view_box.height * scale) / 2.0 - self.view_box.y * scale,
        }
    }
}
