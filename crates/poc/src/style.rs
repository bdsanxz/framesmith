//! The values of the attributes that are more than a number: colours, gradients, shadows, colour
//! filters. They take the form CSS gives them, with every colour written as `#rrggbb` or
//! `#rrggbbaa` and every length as a plain number of pixels.

use crate::lut::Lut;
use std::sync::Arc;

/// A colour as written: red, green, blue, alpha.
pub type Rgba = [u8; 4];

pub fn color(text: &str) -> Option<Rgba> {
    let hex = text.trim().strip_prefix('#')?;
    let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    match hex.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

/// A place along a gradient: a share of its length, or pixels from its start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Along {
    Share(f64),
    Px(f64),
}

impl Along {
    /// The place as a share of a gradient that is `length` pixels long.
    pub fn of(self, length: f64) -> f64 {
        match self {
            Along::Share(share) => share,
            Along::Px(px) => px / length.max(1e-9),
        }
    }
}

/// A place along a gradient and the colour there.
pub type Stop = (Along, Rgba);

/// How far a radial gradient reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reach {
    /// An ellipse through the farthest corner.
    EllipseToCorner,
    /// An ellipse with these radii, as shares of the width and the height.
    Ellipse(f64, f64),
    /// A circle through the farthest corner.
    CircleToCorner,
    /// A circle with this radius in pixels.
    Circle(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Rgba),
    /// `angle` as in CSS: 0 points up, 90 to the right.
    Linear {
        angle: f64,
        stops: Vec<Stop>,
    },
    /// `centre` as shares of the width and the height of what is painted.
    Radial {
        reach: Reach,
        centre: (f64, f64),
        stops: Vec<Stop>,
    },
}

/// The length of the line a linear gradient of `angle` runs along, in a box of this size: from
/// corner to corner in its direction.
pub fn line_length(angle: f64, width: f64, height: f64) -> f64 {
    let (sin, cos) = angle.to_radians().sin_cos();
    (width * sin).abs() + (height * cos).abs()
}

/// The two radii of a radial gradient in a box of this size, in pixels.
pub fn radii(reach: Reach, centre: (f64, f64), width: f64, height: f64) -> (f64, f64) {
    // The farthest corner, seen from the centre.
    let far = |at: f64| at.max(1.0 - at);
    match reach {
        Reach::EllipseToCorner => (
            far(centre.0) * std::f64::consts::SQRT_2 * width,
            far(centre.1) * std::f64::consts::SQRT_2 * height,
        ),
        Reach::Ellipse(rx, ry) => (rx * width, ry * height),
        Reach::CircleToCorner => {
            let radius = (far(centre.0) * width).hypot(far(centre.1) * height);
            (radius, radius)
        }
        Reach::Circle(radius) => (radius, radius),
    }
}

/// Splits at the commas that are not inside brackets.
pub fn split_top(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut from) = (0_i32, 0);
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == separator && depth == 0 => {
                parts.push(text[from..at].trim());
                from = at + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[from..].trim());
    parts
}

fn percent(text: &str) -> Option<f64> {
    text.trim().strip_suffix('%')?.trim().parse::<f64>().ok().map(|value| value / 100.0)
}

fn stops(parts: &[&str]) -> Option<Vec<Stop>> {
    let mut stops = Vec::with_capacity(parts.len());
    for part in parts {
        let mut words = part.split_whitespace();
        let color = color(words.next()?)?;
        let place = words.next()?;
        let at = match percent(place) {
            Some(share) => Along::Share(share),
            None => Along::Px(place.parse().ok()?),
        };
        if words.next().is_some() {
            return None;
        }
        stops.push((at, color));
    }
    (stops.len() >= 2).then_some(stops)
}

fn paint(text: &str) -> Option<Paint> {
    if let Some(color) = color(text) {
        return Some(Paint::Solid(color));
    }
    if let Some(inner) = text.strip_prefix("linear-gradient(").and_then(|rest| rest.strip_suffix(')')) {
        let parts = split_top(inner, ',');
        let (angle, colors) = match parts.first()?.strip_suffix("deg") {
            Some(degrees) => (degrees.trim().parse().ok()?, &parts[1..]),
            None => (180.0, &parts[..]),
        };
        return Some(Paint::Linear {
            angle,
            stops: stops(colors)?,
        });
    }
    if let Some(inner) = text.strip_prefix("radial-gradient(").and_then(|rest| rest.strip_suffix(')')) {
        let parts = split_top(inner, ',');
        // `ellipse 62% 78% at 50% 42%`, `ellipse at 30% 20%`, `circle 12 at 20% 30%`, `circle`.
        let shape = parts.first()?.trim();
        let (size, place) = match shape.split_once(" at ") {
            Some((size, place)) => (size.trim(), Some(place)),
            None => (shape, None),
        };
        let centre = match place {
            None => (0.5, 0.5),
            Some(place) => {
                let mut place = place.split_whitespace();
                let centre = (percent(place.next()?)?, percent(place.next()?)?);
                if place.next().is_some() {
                    return None;
                }
                centre
            }
        };
        let words: Vec<&str> = size.split_whitespace().collect();
        let reach = match words[..] {
            ["ellipse"] => Reach::EllipseToCorner,
            ["ellipse", x, y] => Reach::Ellipse(percent(x)?, percent(y)?),
            ["circle"] => Reach::CircleToCorner,
            ["circle", radius] => Reach::Circle(radius.parse().ok().filter(|radius| *radius > 0.0)?),
            _ => return None,
        };
        return Some(Paint::Radial {
            reach,
            centre,
            stops: stops(&parts[1..])?,
        });
    }
    None
}

/// The layers of a `background`, the first on top, as CSS lists them.
pub fn paints(text: &str) -> Option<Vec<Paint>> {
    split_top(text, ',').into_iter().map(paint).collect()
}

/// A `box-shadow` or a `text-shadow`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub x: f64,
    pub y: f64,
    /// The blur radius as CSS counts it: twice the standard deviation.
    pub blur: f64,
    pub color: Rgba,
}

/// `0 24 70 #00000099`, and more of them after commas.
pub fn shadows(text: &str) -> Option<Vec<Shadow>> {
    split_top(text, ',')
        .into_iter()
        .map(|part| {
            let words: Vec<&str> = part.split_whitespace().collect();
            let [x, y, blur, color_text] = words[..] else { return None };
            Some(Shadow {
                x: x.parse().ok()?,
                y: y.parse().ok()?,
                blur: blur.parse().ok()?,
                color: color(color_text)?,
            })
        })
        .collect()
}

/// One step of a colour filter, with the meaning CSS gives it.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorOp {
    Brightness(f32),
    Contrast(f32),
    Saturate(f32),
    Grayscale(f32),
    /// A colour table. It is the first step of a filter when the filter has one.
    Table(Arc<Lut>),
}

/// `brightness(0.6) contrast(1.14) saturate(0.8)`.
pub fn color_ops(text: &str) -> Option<Vec<ColorOp>> {
    text.split_whitespace()
        .map(|word| {
            let (name, rest) = word.split_once('(')?;
            let amount: f32 = rest.strip_suffix(')')?.parse().ok()?;
            match name {
                "brightness" => Some(ColorOp::Brightness(amount)),
                "contrast" => Some(ColorOp::Contrast(amount)),
                "saturate" => Some(ColorOp::Saturate(amount)),
                "grayscale" => Some(ColorOp::Grayscale(amount)),
                _ => None,
            }
        })
        .collect()
}

/// Applies a colour filter to RGBA pixels in place, each step clamped as CSS clamps it.
/// Alpha is left as it is: the filters here are used on opaque pictures.
pub fn apply_color_ops(ops: &[ColorOp], pixels: &mut [u8]) {
    let grade = Grade::new(ops);
    if grade.leaves_colours() {
        return;
    }
    // Every pixel is changed on its own, so a large picture is shared out over the cores.
    let threads = std::thread::available_parallelism().map_or(1, |cores| cores.get().min(6));
    if threads < 2 || pixels.len() < (1 << 20) {
        return grade.apply(pixels);
    }
    let share = (pixels.len() / 4).div_ceil(threads) * 4;
    std::thread::scope(|scope| {
        for part in pixels.chunks_mut(share) {
            let grade = &grade;
            scope.spawn(move || grade.apply(part));
        }
    });
}

/// The places of the table that follows a matrix: a colour is looked up to one part in 4095.
const FINE: usize = 4096;

/// A colour filter made ready for many pixels.
///
/// The steps that treat each channel alone fold into a table; a step that mixes the channels
/// is a matrix. Steps, a matrix, and steps again is one pass with two tables.
pub struct Grade(Form);

enum Form {
    Plain,
    Table(Box<[u8; 256]>),
    /// Whole numbers with 15 bits of fraction keep this pass quick.
    Matrix {
        first: Box<[i32; 256]>,
        matrix: [i32; 9],
        last: Option<Box<[u8; FINE]>>,
    },
    /// Any other order: every step for every pixel.
    Steps {
        first: Box<[f32; 256]>,
        rest: Vec<ColorOp>,
    },
    /// A colour table, and the steps after it.
    Tabled(Arc<Lut>, Box<Grade>),
}

/// A step that treats each channel alone, clamped as CSS clamps it.
fn alone(op: &ColorOp) -> Option<impl Fn(f32) -> f32> {
    let (scale, offset) = match *op {
        ColorOp::Brightness(amount) => (amount, 0.0),
        ColorOp::Contrast(amount) => (amount, 0.5 - 0.5 * amount),
        _ => return None,
    };
    Some(move |colour: f32| (colour * scale + offset).clamp(0.0, 1.0))
}

impl Grade {
    pub fn new(ops: &[ColorOp]) -> Grade {
        if ops.is_empty() {
            return Grade(Form::Plain);
        }
        if let Some(ColorOp::Table(table)) = ops.first() {
            return Grade(Form::Tabled(Arc::clone(table), Box::new(Grade::new(&ops[1..]))));
        }
        let mut first: Box<[f32; 256]> = Box::new(std::array::from_fn(|value| value as f32 / 255.0));
        let mut at = 0;
        while let Some(step) = ops.get(at).and_then(alone) {
            first.iter_mut().for_each(|value| *value = step(*value));
            at += 1;
        }
        let Some(mixed) = ops.get(at).and_then(matrix) else {
            return Grade(if at == ops.len() {
                Form::Table(Box::new(std::array::from_fn(|value| (first[value] * 255.0).round() as u8)))
            } else {
                Form::Steps {
                    first,
                    rest: ops[at..].to_vec(),
                }
            });
        };
        let after = &ops[at + 1..];
        if !after.iter().all(|op| alone(op).is_some()) {
            return Grade(Form::Steps {
                first,
                rest: ops[at..].to_vec(),
            });
        }
        let last = (!after.is_empty()).then(|| {
            let mut last: Box<[f32; FINE]> = Box::new(std::array::from_fn(|value| value as f32 / (FINE - 1) as f32));
            for step in after.iter().filter_map(alone) {
                last.iter_mut().for_each(|value| *value = step(*value));
            }
            Box::new(std::array::from_fn(|value| (last[value] * 255.0).round() as u8))
        });
        Grade(Form::Matrix {
            first: Box::new(std::array::from_fn(|value| (first[value] * 32767.0).round() as i32)),
            matrix: mixed.map(|value| (value * 32768.0).round() as i32),
            last,
        })
    }

    /// Whether the filter has no step, so pixels need not be looked at.
    pub fn leaves_colours(&self) -> bool {
        matches!(self.0, Form::Plain)
    }

    /// Applies the filter to RGBA pixels in place.
    pub fn apply(&self, pixels: &mut [u8]) {
        if let Form::Tabled(table, rest) = &self.0 {
            table.apply(pixels);
            return rest.apply(pixels);
        }
        let pixels = pixels.as_chunks_mut::<4>().0;
        match &self.0 {
            Form::Plain | Form::Tabled(..) => {}
            Form::Table(table) => {
                for pixel in pixels {
                    pixel[0] = table[pixel[0] as usize];
                    pixel[1] = table[pixel[1] as usize];
                    pixel[2] = table[pixel[2] as usize];
                }
            }
            Form::Matrix { first, matrix: m, last } => {
                // The mixed colour, clamped, with 15 bits of fraction.
                let mixed = |pixel: &[u8; 4]| -> [i32; 3] {
                    let (r, g, b) = (first[pixel[0] as usize], first[pixel[1] as usize], first[pixel[2] as usize]);
                    [
                        ((m[0] * r + m[1] * g + m[2] * b) >> 15).clamp(0, 32767),
                        ((m[3] * r + m[4] * g + m[5] * b) >> 15).clamp(0, 32767),
                        ((m[6] * r + m[7] * g + m[8] * b) >> 15).clamp(0, 32767),
                    ]
                };
                match last {
                    None => {
                        for pixel in pixels {
                            let [r, g, b] = mixed(pixel).map(|value| ((value * 255 + 16384) >> 15) as u8);
                            (pixel[0], pixel[1], pixel[2]) = (r, g, b);
                        }
                    }
                    Some(last) => {
                        let fine = (FINE - 1) as i32;
                        for pixel in pixels {
                            let [r, g, b] = mixed(pixel).map(|value| last[((value * fine + 16384) >> 15) as usize]);
                            (pixel[0], pixel[1], pixel[2]) = (r, g, b);
                        }
                    }
                }
            }
            Form::Steps { first, rest } => {
                for pixel in pixels {
                    let mut c = [first[pixel[0] as usize], first[pixel[1] as usize], first[pixel[2] as usize]];
                    for op in rest {
                        c = match (alone(op), matrix(op)) {
                            (Some(step), _) => c.map(step),
                            (_, Some(m)) => [
                                m[0] * c[0] + m[1] * c[1] + m[2] * c[2],
                                m[3] * c[0] + m[4] * c[1] + m[5] * c[2],
                                m[6] * c[0] + m[7] * c[1] + m[8] * c[2],
                            ]
                            .map(|v| v.clamp(0.0, 1.0)),
                            _ => c,
                        };
                    }
                    pixel[0] = (c[0] * 255.0).round() as u8;
                    pixel[1] = (c[1] * 255.0).round() as u8;
                    pixel[2] = (c[2] * 255.0).round() as u8;
                }
            }
        }
    }
}

/// The matrix of a step that mixes the channels.
fn matrix(op: &ColorOp) -> Option<[f32; 9]> {
    // Both keep `s` of the colour and replace the rest by its grey.
    let s = match *op {
        ColorOp::Saturate(amount) => amount,
        ColorOp::Grayscale(amount) => 1.0 - amount.clamp(0.0, 1.0),
        _ => return None,
    };
    let (r, g, b) = (0.2126_f32, 0.7152_f32, 0.0722_f32);
    Some([
        r + (1.0 - r) * s,
        g - g * s,
        b - b * s,
        r - r * s,
        g + (1.0 - g) * s,
        b - b * s,
        r - r * s,
        g - g * s,
        b + (1.0 - b) * s,
    ])
}
