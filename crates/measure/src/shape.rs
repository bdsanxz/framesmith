//! Measuring a flat-colour rectangle painted on a flat background: where
//! its corners are and how opaque it is, to a fraction of a pixel.

use crate::{Frame, Rgb};

/// A painted rectangle, as measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub centre: (f64, f64),
    /// The longer side.
    pub length: f64,
    /// The shorter side.
    pub breadth: f64,
    /// The direction of the longer side, in degrees clockwise from the x axis, from -90 to 90.
    pub angle: f64,
    /// How much of the element's colour shows in its inside: 1 is solid.
    pub opacity: f64,
    pub corners: [(f64, f64); 4],
}

/// How much of `colour` each pixel holds over `background`, from 0 to 1.
fn share(pixel: Rgb, background: Rgb, colour: Rgb) -> f64 {
    let (mut along, mut whole) = (0.0, 0.0);
    for channel in 0..3 {
        let span = f64::from(colour[channel]) - f64::from(background[channel]);
        along += (f64::from(pixel[channel]) - f64::from(background[channel])) * span;
        whole += span * span;
    }
    if whole == 0.0 {
        0.0
    } else {
        (along / whole).clamp(0.0, 1.0)
    }
}

/// Measures the one rectangle of `colour` painted over `background`.
/// `None` when nothing of that colour shows.
pub fn shape(frame: &Frame, background: Rgb, colour: Rgb) -> Option<Shape> {
    let mut shares: Vec<f64> = Vec::with_capacity((frame.width * frame.height) as usize);
    for y in 0..frame.height {
        for x in 0..frame.width {
            shares.push(share(frame.pixel(x, y), background, colour));
        }
    }
    // The inside of the rectangle holds the largest share; its edges hold less.
    let mut sorted: Vec<f64> = shares.iter().copied().filter(|&s| s > 0.02).collect();
    if sorted.len() < 4 {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    let opacity = sorted[sorted.len() * 9 / 10];
    if opacity < 0.02 {
        return None;
    }
    let (mut mass, mut sum_x, mut sum_y) = (0.0, 0.0, 0.0);
    let weight = |share: f64| (share / opacity).min(1.0);
    for (at, &share) in shares.iter().enumerate() {
        let w = weight(share);
        if w > 0.0 {
            mass += w;
            sum_x += w * ((at as u32 % frame.width) as f64 + 0.5);
            sum_y += w * ((at as u32 / frame.width) as f64 + 0.5);
        }
    }
    let centre = (sum_x / mass, sum_y / mass);
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for (at, &share) in shares.iter().enumerate() {
        let w = weight(share);
        if w > 0.0 {
            let dx = (at as u32 % frame.width) as f64 + 0.5 - centre.0;
            let dy = (at as u32 / frame.width) as f64 + 0.5 - centre.1;
            xx += w * dx * dx;
            yy += w * dy * dy;
            xy += w * dx * dy;
        }
    }
    // Each pixel is a unit square, not a point: that adds a twelfth to each spread.
    let (xx, yy, xy) = (xx / mass + 1.0 / 12.0, yy / mass + 1.0 / 12.0, xy / mass);
    let mean = (xx + yy) / 2.0;
    let swing = (((xx - yy) / 2.0).powi(2) + xy * xy).sqrt();
    let length = (12.0 * (mean + swing)).sqrt();
    let breadth = (12.0 * (mean - swing).max(0.0)).sqrt();
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    let (cos, sin) = (angle.cos(), angle.sin());
    let corner = |along: f64, across: f64| {
        (
            centre.0 + along * length / 2.0 * cos - across * breadth / 2.0 * sin,
            centre.1 + along * length / 2.0 * sin + across * breadth / 2.0 * cos,
        )
    };
    Some(Shape {
        centre,
        length,
        breadth,
        angle: angle.to_degrees(),
        opacity,
        corners: [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)],
    })
}

/// Where the corners of a box should be: a box laid out at `left`, `top`
/// with the given size, moved by `x` and `y`, then turned by `rotation`
/// degrees clockwise and scaled by `scale` about its centre.
pub fn expected_corners(left: f64, top: f64, width: f64, height: f64, values: [f64; 4]) -> [(f64, f64); 4] {
    let [x, y, scale, rotation] = values;
    let centre = (left + width / 2.0 + x, top + height / 2.0 + y);
    let (sin, cos) = rotation.to_radians().sin_cos();
    let corner = |dx: f64, dy: f64| {
        let (dx, dy) = (dx * width / 2.0 * scale, dy * height / 2.0 * scale);
        (centre.0 + dx * cos - dy * sin, centre.1 + dx * sin + dy * cos)
    };
    [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)]
}

/// The largest distance from an expected corner to the nearest measured one.
pub fn corner_distance(expected: &[(f64, f64); 4], measured: &[(f64, f64); 4]) -> f64 {
    expected
        .iter()
        .map(|e| measured.iter().map(|m| (e.0 - m.0).hypot(e.1 - m.1)).fold(f64::INFINITY, f64::min))
        .fold(0.0, f64::max)
}
