//! A `<shape>`: path data drawn in a view box, into the box of the element (`docs/spec/shape.md`, sections 3 to 5). The box
//! is in the frame's pixels, before the map of the element that moves it, and the path is in the view box's coordinates.

use std::fmt;

use crate::{Path, PathError, Rgba};

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

/// Why a `<shape>`'s attributes are refused (`docs/spec/shape.md`, section 6). Each names the offending value, so that the
/// report can name it with its line.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeError {
    /// No `d`, or one that is empty.
    NoPath,
    /// A `d` that is not path data, with the reason.
    BadPath(PathError),
    /// A `view-box` that is not four numbers with a width and a height above 0.
    ViewBox { value: String },
    /// A `dash` that is not an even count of numbers, above zero, with none below zero and not all zero.
    Dashes { value: String },
}

impl fmt::Display for ShapeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeError::NoPath => write!(formatter, "<shape> needs `d` or a `src` with paths"),
            ShapeError::BadPath(reason) => write!(formatter, "`d` is not path data: {reason}"),
            ShapeError::ViewBox { value } => write!(formatter, "`view-box=\"{value}\"` is not a view box of four numbers"),
            ShapeError::Dashes { value } => write!(
                formatter,
                "`dash=\"{value}\"` is not pairs of lengths: a dash and a gap, in view-box units"
            ),
        }
    }
}

/// The path of a `d` attribute: refused when it is absent or empty, and when it is not path data.
pub fn required_path(data: Option<&str>) -> Result<Path, ShapeError> {
    match data.map(str::trim) {
        None | Some("") => Err(ShapeError::NoPath),
        Some(data) => Path::parse(data).map_err(ShapeError::BadPath),
    }
}

impl ViewBox {
    /// A `view-box` of four numbers, `x y width height`, with a width and a height above 0.
    pub fn parse(value: &str) -> Result<ViewBox, ShapeError> {
        let refused = || ShapeError::ViewBox { value: value.to_string() };
        let numbers = numbers_of(value).ok_or_else(refused)?;
        let [x, y, width, height] = numbers[..] else {
            return Err(refused());
        };
        if width.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) || height.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return Err(refused());
        }
        Ok(ViewBox { x, y, width, height })
    }
}

/// The dashes of a `dash` attribute: pairs of lengths, not all zero, none below zero.
pub fn dashes_of(value: &str) -> Result<Vec<f64>, ShapeError> {
    let refused = || ShapeError::Dashes { value: value.to_string() };
    let numbers = numbers_of(value).ok_or_else(refused)?;
    if numbers.is_empty()
        || numbers.len() % 2 != 0
        || numbers.iter().any(|&number| number < 0.0)
        || numbers.iter().all(|&number| number == 0.0)
    {
        return Err(refused());
    }
    Ok(numbers)
}

/// The numbers of an attribute, separated by spaces or commas, or `None` when one of them does not read.
fn numbers_of(value: &str) -> Option<Vec<f64>> {
    value
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.parse::<f64>().ok().filter(|number| number.is_finite()))
        .collect()
}
