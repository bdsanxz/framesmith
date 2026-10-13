//! The fractal noise of the W3C's Filter Effects recommendation, written from the standard's
//! definition, and what is done with it to the finished picture of an element. See
//! `docs/spec/native-notation.md`, section 17.

use crate::model::{Finish, NoiseField};
use crate::style::Shadow;
use vello_cpu::kurbo::{Affine, Point, Rect};

/// How many lattice points the noise repeats after, and the shift that keeps every coordinate
/// positive.
const SIZE: usize = 256;
const SHIFT: f64 = 4096.0;

/// The standard's own generator of the lattice: the next number after `seed`.
fn next(seed: i64) -> i64 {
    let result = 16807 * (seed % 127_773) - 2836 * (seed / 127_773);
    if result <= 0 {
        result + 2_147_483_647
    } else {
        result
    }
}

/// The lattice of one seed: the alpha channel of the standard's noise.
pub struct Noise {
    lattice: [usize; SIZE + SIZE + 2],
    /// The direction at each lattice point, for the fourth channel.
    gradient: [[f64; 2]; SIZE + SIZE + 2],
}

impl Noise {
    pub fn new(seed: i32) -> Noise {
        let mut seed = i64::from(seed);
        if seed <= 0 {
            seed = -(seed % 2_147_483_646) + 1;
        }
        let mut lattice = [0; SIZE + SIZE + 2];
        let mut gradient = [[0.0; 2]; SIZE + SIZE + 2];
        // The four channels are made one after another from the same run of numbers; only the
        // last one is kept.
        for _channel in 0..4 {
            for (index, direction) in gradient.iter_mut().enumerate().take(SIZE) {
                lattice[index] = index;
                for part in direction.iter_mut() {
                    seed = next(seed);
                    *part = ((seed % (2 * SIZE as i64)) - SIZE as i64) as f64 / SIZE as f64;
                }
                let length = direction[0].hypot(direction[1]);
                *direction = [direction[0] / length, direction[1] / length];
            }
        }
        for index in (1..SIZE).rev() {
            seed = next(seed);
            lattice.swap(index, (seed % SIZE as i64) as usize);
        }
        for index in 0..SIZE + 2 {
            lattice[SIZE + index] = lattice[index];
            gradient[SIZE + index] = gradient[index];
        }
        Noise { lattice, gradient }
    }

    /// One octave at a point of the lattice's own units: between -1 and 1.
    fn octave(&self, x: f64, y: f64) -> f64 {
        let along = |value: f64| {
            let shifted = value + SHIFT;
            let whole = shifted.floor();
            let cell = (whole as i64 & 0xff) as usize;
            (cell, (cell + 1) & 0xff, shifted - whole)
        };
        let (x0, x1, fx) = along(x);
        let (y0, y1, fy) = along(y);
        let (i, j) = (self.lattice[x0], self.lattice[x1]);
        let corner = |column: usize, row: usize, dx: f64, dy: f64| {
            let direction = self.gradient[self.lattice[column + row]];
            dx * direction[0] + dy * direction[1]
        };
        let eased = |t: f64| t * t * (3.0 - 2.0 * t);
        let mix = |t: f64, a: f64, b: f64| a + t * (b - a);
        let top = mix(eased(fx), corner(i, y0, fx, fy), corner(j, y0, fx - 1.0, fy));
        let bottom = mix(eased(fx), corner(i, y1, fx, fy - 1.0), corner(j, y1, fx - 1.0, fy - 1.0));
        mix(eased(fy), top, bottom)
    }

    /// The noise of `field` at a point of the element's box, in 256 steps between 0 and 1.
    pub fn at(&self, field: &NoiseField, x: f64, y: f64) -> f64 {
        let (mut vx, mut vy, mut ratio, mut sum) = (x * field.frequency, y * field.frequency, 1.0, 0.0);
        for _ in 0..field.octaves {
            sum += self.octave(vx, vy) / ratio;
            vx *= 2.0;
            vy *= 2.0;
            ratio *= 2.0;
        }
        (((sum + 1.0) / 2.0).clamp(0.0, 1.0) * 255.0).round() / 255.0
    }
}

/// How far around an element its finished picture can reach because of its finishes, in pixels
/// of the frame.
pub fn room(finish: &Finish, transform: Affine) -> f64 {
    let [a, b, c, d, ..] = transform.as_coeffs();
    let scale = a.hypot(b).max(c.hypot(d)).max(1.0);
    let torn = finish
        .displace
        .map_or(0.0, |(amount, _)| amount / 2.0 * scale * std::f64::consts::SQRT_2 + 1.0);
    let shaded = finish
        .shadow
        .map_or(0.0, |shadow| (shadow.x.hypot(shadow.y) + 1.5 * shadow.blur) * scale + 2.0);
    torn + shaded
}

/// Gives a finished picture its finishes. `pixels` is RGBA with the colour multiplied by the
/// alpha, `width` wide; its top left pixel lies at `corner` of the frame, and `transform` takes
/// a point of the element's box to the frame.
pub fn finish(pixels: &mut [u8], width: usize, corner: Point, transform: Affine, finish: &Finish) {
    let height = pixels.len() / 4 / width.max(1);
    if width == 0 || height == 0 {
        return;
    }
    let back = transform.inverse();
    let [a, b, c, d, ..] = transform.as_coeffs();
    // The noise under the middle of each pixel, half a pixel of the element on.
    let read = |noise: &Noise, field: &NoiseField, x: usize, y: usize| {
        let own = back * Point::new(corner.x + x as f64 + 0.5, corner.y + y as f64 + 0.5);
        noise.at(field, own.x + 0.5, own.y + 0.5)
    };
    if let Some((amount, field)) = finish.displace {
        let noise = Noise::new(field.seed);
        let before = pixels.to_vec();
        for y in 0..height {
            for x in 0..width {
                let by = amount * (read(&noise, &field, x, y) - 0.5);
                // The same distance across and down in the element's units, as the frame sees it.
                let (dx, dy) = ((a * by + c * by + 0.5).floor() as i64, (b * by + d * by + 0.5).floor() as i64);
                let (from_x, from_y) = (x as i64 + dx, y as i64 + dy);
                let to = (y * width + x) * 4;
                if from_x < 0 || from_y < 0 || from_x >= width as i64 || from_y >= height as i64 {
                    pixels[to..to + 4].fill(0);
                } else {
                    let from = (from_y as usize * width + from_x as usize) * 4;
                    pixels[to..to + 4].copy_from_slice(&before[from..from + 4]);
                }
            }
        }
    }
    if let Some((field, slope, offset)) = finish.grain {
        let noise = Noise::new(field.seed);
        for y in 0..height {
            for x in 0..width {
                let at = (y * width + x) * 4;
                if pixels[at + 3] == 0 {
                    continue;
                }
                let kept = (slope * read(&noise, &field, x, y) + offset).clamp(0.0, 1.0);
                for channel in &mut pixels[at..at + 4] {
                    *channel = (f64::from(*channel) * kept).round() as u8;
                }
            }
        }
    }
    if let Some(shadow) = finish.shadow {
        shade(pixels, width, height, transform, &shadow);
    }
}

/// One pass of an average over `span` pixels along rows of `width`, with the middle of the span
/// `lead` pixels after its first.
fn average(from: &[f32], to: &mut [f32], width: usize, stride: usize, count: usize, span: usize, lead: usize) {
    for line in 0..count {
        let start = if stride == 1 { line * width } else { line };
        let length = if stride == 1 { width } else { from.len() / width };
        let at = |index: i64| {
            if index < 0 || index >= length as i64 {
                0.0
            } else {
                from[start + index as usize * stride]
            }
        };
        let mut sum: f32 = (0..span as i64).map(|k| at(k - lead as i64)).sum();
        for index in 0..length {
            to[start + index * stride] = sum / span as f32;
            let first = index as i64 - lead as i64;
            sum += at(first + span as i64) - at(first);
        }
    }
}

/// Puts the shadow of the picture under it: its alpha, blurred, moved and coloured.
fn shade(pixels: &mut [u8], width: usize, height: usize, transform: Affine, shadow: &Shadow) {
    let [a, b, c, d, ..] = transform.as_coeffs();
    let scale = (a * d - b * c).abs().sqrt();
    let deviation = shadow.blur / 2.0 * scale;
    let mut alpha: Vec<f32> = pixels.as_chunks::<4>().0.iter().map(|pixel| f32::from(pixel[3]) / 255.0).collect();
    let span = (deviation * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0 + 0.5).floor() as usize;
    if span > 1 {
        let mut other = vec![0.0; alpha.len()];
        // An odd span is centred three times; an even one lies half a pixel to one side, then to
        // the other, and the third pass is one wider and centred.
        let passes = if span % 2 == 1 {
            [(span, span / 2), (span, span / 2), (span, span / 2)]
        } else {
            [(span, span / 2), (span, span / 2 - 1), (span + 1, span / 2)]
        };
        for (stride, count) in [(1, height), (width, width)] {
            for (span, lead) in passes {
                average(&alpha, &mut other, width, stride, count, span, lead);
                std::mem::swap(&mut alpha, &mut other);
            }
        }
    }
    // Where the shadow of each pixel is read from: back by the shadow's own move, as the frame
    // sees it, between four pixels.
    let (move_x, move_y) = (a * shadow.x + c * shadow.y, b * shadow.x + d * shadow.y);
    let sample = |x: f64, y: f64| {
        let (left, top) = (x.floor(), y.floor());
        let (fx, fy) = ((x - left) as f32, (y - top) as f32);
        let at = |column: f64, row: f64| {
            if column < 0.0 || row < 0.0 || column >= width as f64 || row >= height as f64 {
                0.0
            } else {
                alpha[row as usize * width + column as usize]
            }
        };
        let upper = at(left, top) * (1.0 - fx) + at(left + 1.0, top) * fx;
        let lower = at(left, top + 1.0) * (1.0 - fx) + at(left + 1.0, top + 1.0) * fx;
        upper * (1.0 - fy) + lower * fy
    };
    let ink = shadow.color.map(|channel| f32::from(channel) / 255.0);
    for y in 0..height {
        for x in 0..width {
            let under = sample(x as f64 - move_x, y as f64 - move_y) * ink[3];
            if under <= 0.0 {
                continue;
            }
            let pixel = &mut pixels[(y * width + x) * 4..][..4];
            let clear = 1.0 - f32::from(pixel[3]) / 255.0;
            for channel in 0..3 {
                pixel[channel] = (f32::from(pixel[channel]) + ink[channel] * under * clear * 255.0)
                    .round()
                    .min(255.0) as u8;
            }
            pixel[3] = (f32::from(pixel[3]) + under * clear * 255.0).round().min(255.0) as u8;
        }
    }
}

/// The part of the frame a picture of `bounds` needs so that its finishes are whole.
pub fn widened(bounds: Rect, finish: &Finish, transform: Affine) -> Rect {
    let room = room(finish, transform);
    bounds.inflate(room, room)
}
