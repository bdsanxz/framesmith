//! What both renderers share when they turn a drawing step into paint.

use crate::model::{Fit, FitKind};
use crate::style::{self, Paint, Rgba, Stop};
use vello_cpu::filter_effects::{Filter, FilterFunction};
use vello_cpu::kurbo::{Affine, Point, Rect};
use vello_cpu::peniko::color::{AlphaColor, Srgb};
use vello_cpu::peniko::{ColorStop, Gradient};

pub fn color(color: Rgba, opacity: f32) -> AlphaColor<Srgb> {
    let alpha = (f32::from(color[3]) * opacity).round().clamp(0.0, 255.0) as u8;
    AlphaColor::from_rgba8(color[0], color[1], color[2], alpha)
}

pub enum Brush {
    Solid(AlphaColor<Srgb>),
    /// A gradient and where it lies in the element's space.
    Gradient(Gradient, Affine),
}

fn stops(stops: &[Stop], length: f64, opacity: f32) -> Vec<ColorStop> {
    // A stop never lies before the one ahead of it.
    let mut last = 0.0_f32;
    let mut placed: Vec<ColorStop> = Vec::with_capacity(stops.len() + 1);
    for (at, stop) in stops {
        last = (at.of(length) as f32).clamp(last, 1.0);
        let stop = color(*stop, opacity);
        // The GPU renderer takes the first stop to lie at the start whatever its place, so a
        // gradient that begins later is given a stop of the same colour at the start.
        if placed.is_empty() && last > 0.0 {
            placed.push(ColorStop::from((0.0, stop)));
        }
        placed.push(ColorStop::from((last, stop)));
    }
    placed
}

/// The paint that fills `rect` with `paint`.
pub fn brush(paint: &Paint, rect: Rect, opacity: f32) -> Brush {
    let (width, height) = (rect.width(), rect.height());
    match paint {
        Paint::Solid(solid) => Brush::Solid(color(*solid, opacity)),
        Paint::Linear { angle, stops: listed } => {
            // The gradient line runs through the centre and is as long as the box is in its
            // direction, corner to corner.
            let (sin, cos) = angle.to_radians().sin_cos();
            let length = style::line_length(*angle, width, height);
            let (half, centre) = (length / 2.0, rect.center());
            let start = Point::new(centre.x - sin * half, centre.y + cos * half);
            let end = Point::new(centre.x + sin * half, centre.y - cos * half);
            let gradient = Gradient::new_linear(start, end).with_stops(stops(listed, length, opacity).as_slice());
            Brush::Gradient(gradient, Affine::IDENTITY)
        }
        Paint::Radial {
            reach,
            centre,
            stops: listed,
        } => {
            let (rx, ry) = style::radii(*reach, *centre, width, height);
            // A place in pixels is counted along the first radius.
            let gradient = Gradient::new_radial(Point::ZERO, 1.0).with_stops(stops(listed, rx, opacity).as_slice());
            let placed = Affine::translate((rect.x0 + centre.0 * width, rect.y0 + centre.1 * height))
                * Affine::scale_non_uniform(rx.max(1e-6), ry.max(1e-6));
            Brush::Gradient(gradient, placed)
        }
    }
}

/// Where a picture of `width` by `height` lies in `rect` as `fit` says, and the part of `rect`
/// it is drawn in: all of it, or, for a picture fitted whole, the picture's own place.
pub fn fit(rect: Rect, width: f64, height: f64, fit: Fit) -> (Affine, Rect) {
    let (sx, sy) = (rect.width() / width, rect.height() / height);
    let scale = match fit.kind {
        FitKind::Fill => return (Affine::translate((rect.x0, rect.y0)) * Affine::scale_non_uniform(sx, sy), rect),
        FitKind::Cover => sx.max(sy),
        FitKind::Contain => sx.min(sy),
    };
    let (x, y) = (
        rect.x0 + (rect.width() - width * scale) * fit.hold.0,
        rect.y0 + (rect.height() - height * scale) * fit.hold.1,
    );
    let own = Rect::new(x, y, x + width * scale, y + height * scale);
    (Affine::translate((x, y)) * Affine::scale(scale), own.intersect(rect))
}

/// A blur of the given standard deviation.
pub fn blur(deviation: f32) -> Filter {
    Filter::from_function(FilterFunction::Blur { radius: deviation })
}
