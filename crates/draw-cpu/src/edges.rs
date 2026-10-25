//! The coverage of a box's rounded edges and of the rings of its border (`docs/spec/box-edges.md`, section 5): each shape is
//! filled by vello_cpu in opaque white, in a box of whole pixels of the frame that holds it, and the share of each pixel it
//! covers is read back. The processor and the card both take a shape's coverage from here.

use draw::{Error, Map, Ring, Rounded};
use vello_cpu::kurbo::{self, Affine, BezPath, Shape};
use vello_cpu::peniko::{Color, Fill};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::{affine, Coverage};

/// How far, in pixels before the map, a quarter circle's curve may lie from the true circle.
const TOLERANCE: f64 = 0.01;

/// The coverage of a ring mapped by `map`: the share of each pixel inside its outer edge and not inside its inner edge.
/// `None` when the ring holds no pixel of the frame.
pub fn ring_coverage(ring: Ring, map: Map, width: u32, height: u32) -> Result<Option<Coverage>, Error> {
    let mut path = path_of(ring.outer);
    if let Some(inner) = ring.inner {
        path.extend(path_of(inner));
    }
    coverage(&path, ring.outer, map, width, height)
}

/// The coverage of a rounded rectangle mapped by `map`; `None` when it holds no pixel of the frame.
pub fn rounded_coverage(shape: Rounded, map: Map, width: u32, height: u32) -> Result<Option<Coverage>, Error> {
    coverage(&path_of(shape), shape, map, width, height)
}

/// The outline of a rounded rectangle: its sides and its quarter circles, as kurbo makes a circular arc.
fn path_of(shape: Rounded) -> BezPath {
    kurbo::RoundedRect::new(shape.left, shape.top, shape.right, shape.bottom, shape.radius).to_path(TOLERANCE)
}

/// Fills `path`, which lies inside `bounds`, by the even-odd rule, so that an inner outline cuts a hole in an outer one.
fn coverage(path: &BezPath, bounds: Rounded, map: Map, width: u32, height: u32) -> Result<Option<Coverage>, Error> {
    if !map.is_finite() {
        return Err(Error::new("a box is moved by a map that is not finite"));
    }
    let corners = [
        map.apply(bounds.left, bounds.top),
        map.apply(bounds.right, bounds.top),
        map.apply(bounds.right, bounds.bottom),
        map.apply(bounds.left, bounds.bottom),
    ];
    let least_x = corners.iter().map(|corner| corner.0).fold(f64::INFINITY, f64::min);
    let most_x = corners.iter().map(|corner| corner.0).fold(f64::NEG_INFINITY, f64::max);
    let least_y = corners.iter().map(|corner| corner.1).fold(f64::INFINITY, f64::min);
    let most_y = corners.iter().map(|corner| corner.1).fold(f64::NEG_INFINITY, f64::max);
    let left = least_x.floor().max(0.0);
    let top = least_y.floor().max(0.0);
    let right = most_x.ceil().min(f64::from(width));
    let bottom = most_y.ceil().min(f64::from(height));
    if left >= right || top >= bottom {
        return Ok(None);
    }
    let (box_width, box_height) = ((right - left) as u32, (bottom - top) as u32);
    let mut context = RenderContext::new(box_width as u16, box_height as u16);
    let mut resources = Resources::new();
    context.set_paint(Color::from_rgba8(255, 255, 255, 255));
    context.set_fill_rule(Fill::EvenOdd);
    context.set_transform(Affine::translate((-left, -top)) * affine(map));
    context.fill_path(path);
    context.flush();
    let mut pixmap = Pixmap::new(box_width as u16, box_height as u16);
    context.render(&mut pixmap, &mut resources);
    let alpha = pixmap.data_as_u8_slice().as_chunks::<4>().0.iter().map(|pixel| pixel[3]).collect();
    Ok(Some(Coverage {
        left: left as u32,
        top: top as u32,
        width: box_width,
        height: box_height,
        alpha,
    }))
}
