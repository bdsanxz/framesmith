//! A picture worked out from the specs alone, to hold checked-in pictures to: no product code is used. Shapes are convex
//! polygons in the frame, and each pixel takes the exact share of its square a shape covers. A group's shapes are drawn
//! into a layer of their own, which is faded as one picture and laid by src-over (`docs/spec/render-command.md`, sections
//! 4.1, 4.2 and 4.5, rules 3 and 6). The easings (`docs/spec/motion.md`), the frame grid (`docs/spec/frame-time.md`) and
//! the map (section 4.5, rule 3) are written out here again from their specs. A text's ink is held through what the specs
//! of the test font (`docs/spec/test-font.md`, section 3) and of line height (`docs/spec/text.md`, section 4) give.

#![allow(dead_code)]

use std::f64::consts::PI;

use draw::Picture;

pub type Point = (f64, f64);

/// A rectangle as (left, top, right, bottom), right and bottom not included.
pub type Area = (f64, f64, f64, f64);

/// The value of an easing of `motion.md` at progress `p`, written again from its definition.
pub fn ease(name: &str, p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    if p == 0.0 {
        return 0.0;
    }
    if p == 1.0 {
        return 1.0;
    }
    match name {
        "linear" => p,
        "out-cubic" => 1.0 - (1.0 - p).powi(3),
        "in-out-sine" => (1.0 - (p * PI).cos()) / 2.0,
        "in-expo" => 2f64.powf(10.0 * p - 10.0),
        "in-out-quint" if p < 0.5 => 16.0 * p.powi(5),
        "in-out-quint" => 1.0 - (2.0 - 2.0 * p).powi(5) / 2.0,
        _ => {
            let n: f64 = name
                .strip_prefix("out-back(")
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("`{name}` is not an easing worked here"));
            let q = p - 1.0;
            q * q * ((n + 1.0) * q + n) + 1.0
        }
    }
}

/// The value at time `t` of a tween from `from` to `to` that starts at `at` and lasts `duration`: the start before it,
/// the end after it, and eased between.
pub fn tween(t: f64, at: f64, duration: f64, from: f64, to: f64, ease_name: &str) -> f64 {
    from + (to - from) * ease(ease_name, (t - at) / duration)
}

/// The frame of a time on the grid of `frame-time.md`: the nearest frame when the time is within a thousandth of a frame
/// of it, otherwise the next frame after the time.
pub fn grid(time: f64, fps: f64) -> i64 {
    let frames = time * fps;
    let nearest = frames.round();
    if (frames - nearest).abs() <= 0.001 + 1e-9 {
        nearest as i64
    } else {
        frames.ceil() as i64
    }
}

/// Whether frame `number` is in the window of a box that starts at `start` and lasts `duration` (section 4.2).
pub fn shown(number: u64, fps: f64, start: f64, duration: f64) -> bool {
    let number = number as i64;
    grid(start, fps) <= number && number < grid(start + duration, fps)
}

/// The values of the movement of an element, with its origin in the frame (section 4.5, rules 2 and 3).
#[derive(Clone, Copy, Debug)]
pub struct Movement {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub scale_x: f64,
    pub rotation: f64,
    pub skew_y: f64,
    pub origin: Point,
}

impl Movement {
    /// The movement that leaves an element where it is.
    pub fn still(origin: Point) -> Movement {
        Movement {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            scale_x: 1.0,
            rotation: 0.0,
            skew_y: 0.0,
            origin,
        }
    }

    /// Where a point of the element's picture goes: scaled, slanted, turned about the origin, then moved.
    pub fn apply(&self, (px, py): Point) -> Point {
        let (u, v) = (px - self.origin.0, py - self.origin.1);
        let (u, v) = (u * self.scale * self.scale_x, v * self.scale);
        let v = v + u * self.skew_y.to_radians().tan();
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let (u, v) = (u * cos - v * sin, u * sin + v * cos);
        (u + self.origin.0 + self.x, v + self.origin.1 + self.y)
    }

    /// How much the map multiplies an area by.
    pub fn area_scale(&self) -> f64 {
        self.scale * self.scale * self.scale_x
    }
}

/// The corners of a rectangle, clockwise on the screen.
pub fn rect(left: f64, top: f64, width: f64, height: f64) -> Vec<Point> {
    vec![(left, top), (left + width, top), (left + width, top + height), (left, top + height)]
}

/// The corners of a rectangle moved by a movement.
pub fn moved_rect(left: f64, top: f64, width: f64, height: f64, movement: &Movement) -> Vec<Point> {
    rect(left, top, width, height)
        .into_iter()
        .map(|corner| movement.apply(corner))
        .collect()
}

/// The part of a polygon on the side of a line where `inside` holds, by the Sutherland–Hodgman step.
fn cut(polygon: &[Point], inside: impl Fn(Point) -> bool, cross: impl Fn(Point, Point) -> Point) -> Vec<Point> {
    let mut kept = Vec::new();
    for (index, &here) in polygon.iter().enumerate() {
        let next = polygon[(index + 1) % polygon.len()];
        match (inside(here), inside(next)) {
            (true, true) => kept.push(next),
            (true, false) => kept.push(cross(here, next)),
            (false, true) => {
                kept.push(cross(here, next));
                kept.push(next);
            }
            (false, false) => {}
        }
    }
    kept
}

/// The area of a polygon, by the shoelace formula.
fn area(polygon: &[Point]) -> f64 {
    let mut twice = 0.0;
    for (index, &(x0, y0)) in polygon.iter().enumerate() {
        let (x1, y1) = polygon[(index + 1) % polygon.len()];
        twice += x0 * y1 - x1 * y0;
    }
    twice.abs() / 2.0
}

/// The share of the square of pixel (x, y) that a convex polygon covers, exactly.
pub fn share(polygon: &[Point], x: f64, y: f64) -> f64 {
    let at_x = |edge: f64| move |a: Point, b: Point| (edge, a.1 + (b.1 - a.1) * (edge - a.0) / (b.0 - a.0));
    let at_y = |edge: f64| move |a: Point, b: Point| (a.0 + (b.0 - a.0) * (edge - a.1) / (b.1 - a.1), edge);
    let mut kept = polygon.to_vec();
    kept = cut(&kept, |p| p.0 >= x, at_x(x));
    kept = cut(&kept, |p| p.0 <= x + 1.0, at_x(x + 1.0));
    kept = cut(&kept, |p| p.1 >= y, at_y(y));
    kept = cut(&kept, |p| p.1 <= y + 1.0, at_y(y + 1.0));
    if kept.len() < 3 {
        0.0
    } else {
        area(&kept)
    }
}

/// A convex shape filled with one opaque colour.
pub struct Fill {
    pub corners: Vec<Point>,
    pub colour: [u8; 3],
}

/// A picture worked out from shapes, in straight values from 0 to 255, opaque.
pub struct Worked {
    pub width: u32,
    pub height: u32,
    values: Vec<[f64; 3]>,
}

impl Worked {
    /// A picture of one colour.
    pub fn new(width: u32, height: u32, background: [u8; 3]) -> Worked {
        let value = background.map(f64::from);
        Worked {
            width,
            height,
            values: vec![value; (width * height) as usize],
        }
    }

    /// Draws the fills, in order, into a layer of their own, transparent at first, and lays the layer over the picture
    /// faded as one picture by `opacity`.
    pub fn lay(&mut self, fills: &[Fill], opacity: f64) {
        // Only the pixels the fills' corners reach can change.
        let corners = fills.iter().flat_map(|fill| fill.corners.iter());
        let (mut left, mut top, mut right, mut bottom) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in corners {
            (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
        }
        let xs = (left.floor().max(0.0) as u32)..(right.ceil().max(0.0) as u32).min(self.width);
        let ys = (top.floor().max(0.0) as u32)..(bottom.ceil().max(0.0) as u32).min(self.height);
        for y in ys {
            for x in xs.clone() {
                let (mut layer, mut alpha) = ([0.0; 3], 0.0);
                for fill in fills {
                    let covered = share(&fill.corners, x as f64, y as f64);
                    if covered == 0.0 {
                        continue;
                    }
                    for (channel, value) in layer.iter_mut().enumerate() {
                        *value = f64::from(fill.colour[channel]) * covered + *value * (1.0 - covered);
                    }
                    alpha = covered + alpha * (1.0 - covered);
                }
                let at = (y * self.width + x) as usize;
                for (channel, value) in self.values[at].iter_mut().enumerate() {
                    *value = *value * (1.0 - alpha * opacity) + layer[channel] * opacity;
                }
            }
        }
    }

    /// Draws one fill over the picture.
    pub fn fill(&mut self, corners: Vec<Point>, colour: [u8; 3]) {
        self.lay(&[Fill { corners, colour }], 1.0);
    }

    /// The worked value of a pixel.
    pub fn value(&self, x: u32, y: u32) -> [f64; 3] {
        self.values[(y * self.width + x) as usize]
    }
}

/// Whether pixel (x, y) touches an area.
fn touches(area: &Area, x: u32, y: u32) -> bool {
    let (x, y) = (x as f64, y as f64);
    x + 1.0 > area.0 && x < area.2 && y + 1.0 > area.1 && y < area.3
}

/// Holds a picture to the worked one: every pixel that touches none of the areas `aside` is opaque and within
/// `tolerance` levels of its worked value in each colour. A failure names the first pixel out, its values, and how many
/// are out.
pub fn hold(picture: &Picture, worked: &Worked, aside: &[Area], tolerance: f64) -> Result<(), String> {
    if (picture.width(), picture.height()) != (worked.width, worked.height) {
        return Err(format!(
            "the picture is {}x{} and the worked one {}x{}",
            picture.width(),
            picture.height(),
            worked.width,
            worked.height
        ));
    }
    let mut out = Vec::new();
    for y in 0..picture.height() {
        for x in 0..picture.width() {
            if aside.iter().any(|area| touches(area, x, y)) {
                continue;
            }
            let drawn = picture.pixel(x, y).unwrap();
            let value = worked.value(x, y);
            let apart = (0..3)
                .map(|channel| (f64::from(drawn[channel]) - value[channel]).abs())
                .fold(0.0, f64::max);
            if apart > tolerance || drawn[3] != 255 {
                out.push((x, y, drawn, value));
            }
        }
    }
    match out.first() {
        None => Ok(()),
        Some(&(x, y, drawn, value)) => Err(format!(
            "pixel ({x}, {y}) is {drawn:?} and the worked value is [{:.1}, {:.1}, {:.1}]; {} pixels are more than {tolerance} \
             levels apart",
            value[0],
            value[1],
            value[2],
            out.len()
        )),
    }
}

/// The ink of a text within an area: how much of each pixel it covers, judged from how far the pixel lies from the worked
/// picture under it toward the text's colour, then the sum of those shares and their centre.
#[derive(Debug)]
pub struct Ink {
    pub mass: f64,
    pub centre: Point,
    /// The pixels more than half covered, as (left, top, right, bottom), right and bottom not included.
    pub extent: (u32, u32, u32, u32),
}

/// The ink of a text of one colour within an area. A pixel's share is the length of its step from the worked value
/// toward the colour, over the whole step, from 0 to 1.
pub fn ink(picture: &Picture, worked: &Worked, colour: [u8; 3], area: Area) -> Ink {
    ink_by(picture, worked, area, |drawn, under| {
        let (mut along, mut whole) = (0.0, 0.0);
        for channel in 0..3 {
            let step = f64::from(colour[channel]) - under[channel];
            along += (drawn[channel] - under[channel]) * step;
            whole += step * step;
        }
        (along / whole).clamp(0.0, 1.0)
    })
}

/// The ink of a text of several colours within an area: a pixel's weight is how far it lies from the worked value, over
/// the farthest a value can lie (765). Its centre moves with the text's map and its mass scales by the map's area.
pub fn ink_of_colours(picture: &Picture, worked: &Worked, area: Area) -> Ink {
    ink_by(picture, worked, area, |drawn, under| {
        (0..3).map(|channel| (drawn[channel] - under[channel]).abs()).sum::<f64>() / 765.0
    })
}

fn ink_by(picture: &Picture, worked: &Worked, area: Area, share_of: impl Fn([f64; 3], [f64; 3]) -> f64) -> Ink {
    let (mut mass, mut sum_x, mut sum_y) = (0.0, 0.0, 0.0);
    let mut extent: Option<(u32, u32, u32, u32)> = None;
    let first_x = area.0.floor().max(0.0) as u32;
    let first_y = area.1.floor().max(0.0) as u32;
    let end_x = (area.2.ceil() as u32).min(picture.width());
    let end_y = (area.3.ceil() as u32).min(picture.height());
    for y in first_y..end_y {
        for x in first_x..end_x {
            let drawn = picture.pixel(x, y).unwrap();
            let drawn = [drawn[0], drawn[1], drawn[2]].map(f64::from);
            let share = share_of(drawn, worked.value(x, y));
            mass += share;
            sum_x += share * (x as f64 + 0.5);
            sum_y += share * (y as f64 + 0.5);
            if share > 0.5 {
                extent = Some(match extent {
                    None => (x, y, x + 1, y + 1),
                    Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)),
                });
            }
        }
    }
    Ink {
        mass,
        centre: (sum_x / mass, sum_y / mass),
        extent: extent.unwrap_or((0, 0, 0, 0)),
    }
}

/// The left edge of the ink of a text of the test font at rest, whose paragraph's left edge is `left`, at `size` pixels:
/// the first glyph's ink starts 60 units of the em after its origin in the Regular face (`test-font.md`, section 3).
pub fn ink_left_at_rest(left: f64, size: f64) -> f64 {
    left + 0.06 * size
}

/// How far across the centre of the ink of a line of the test font that reads the same both ways (`HIIH`) lies from the
/// middle of its paragraph, at `size` pixels: each glyph's ink is 60 units from its origin and 100 from its advance in the
/// Regular face (`test-font.md`, section 3), so the ink is centred 20 units left of the middle.
pub fn palindrome_centre_across_from_middle(size: f64) -> f64 {
    -0.02 * size
}

/// Holds a measured number to a worked one within a tolerance, naming what it is.
pub fn near(what: &str, measured: f64, worked: f64, tolerance: f64) -> Result<(), String> {
    if (measured - worked).abs() <= tolerance {
        Ok(())
    } else {
        Err(format!(
            "{what} is {measured:.3} and the worked value is {worked:.3} (tolerance {tolerance})"
        ))
    }
}
