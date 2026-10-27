//! Path data read into moves, straight lines and cubic curves: the SVG grammar of a `<shape d>` (`docs/spec/shape.md`,
//! section 2). A quadratic curve is written as the cubic of the same curve, and an arc as cubics of at most 45 degrees each,
//! so that every curve is one kind of segment for the renderers.

use std::fmt;

/// A point in the view box's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// One step of an outline: a move to the start of a new outline, a straight line, a cubic curve, or the close of the outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(Point),
    Line(Point),
    Cubic(Point, Point, Point),
    Close,
}

/// All the outlines of a path, in the order the path data writes them.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub segments: Vec<Segment>,
}

/// Why path data is not path data. Its text is the reason given after `d` is not path data (`docs/spec/shape.md`, section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    NoMoveAtStart,
    UnknownCommand(char),
    NumberWithoutCommand,
    MissingNumber,
    BadNumber,
    BadFlag,
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathError::NoMoveAtStart => write!(formatter, "no M at the start"),
            PathError::UnknownCommand(letter) => write!(formatter, "unknown command {letter}"),
            PathError::NumberWithoutCommand => write!(formatter, "a number has no command"),
            PathError::MissingNumber => write!(formatter, "a number is missing"),
            PathError::BadNumber => write!(formatter, "a number does not read"),
            PathError::BadFlag => write!(formatter, "a flag is not 0 or 1"),
        }
    }
}

/// The most an arc is cut into one piece of, in radians: a cubic of 45 degrees stays within 0.25 pixel of its arc for any
/// radius up to about 60,000 pixels.
const LARGEST_PIECE: f64 = std::f64::consts::FRAC_PI_4;

impl Path {
    /// Reads path data. Separators are spaces and commas; a command letter may be followed by more numbers, which repeat
    /// it (a repeated `M` is an `L`). The first command must be `M` or `m`.
    pub fn parse(data: &str) -> Result<Path, PathError> {
        let mut reader = Reader {
            bytes: data.as_bytes(),
            at: 0,
        };
        let mut segments = Vec::new();
        let mut current = Point { x: 0.0, y: 0.0 };
        let mut start = current;
        let mut last_cubic: Option<Point> = None;
        let mut last_quadratic: Option<Point> = None;
        let mut command: Option<u8> = None;
        let mut first = true;
        loop {
            reader.skip_separators();
            let Some(next) = reader.peek() else {
                break;
            };
            if next.is_ascii_alphabetic() {
                reader.at += 1;
                command = Some(next);
                if first && !matches!(next, b'M' | b'm') {
                    return Err(PathError::NoMoveAtStart);
                }
                if matches!(next, b'Z' | b'z') {
                    segments.push(Segment::Close);
                    current = start;
                    last_cubic = None;
                    last_quadratic = None;
                    first = false;
                    continue;
                }
            } else {
                let Some(letter) = command else {
                    return Err(PathError::NoMoveAtStart);
                };
                if matches!(letter, b'Z' | b'z') {
                    return Err(PathError::NumberWithoutCommand);
                }
                // A number after a command repeats it: a repeated move is a line.
                if letter == b'M' {
                    command = Some(b'L');
                } else if letter == b'm' {
                    command = Some(b'l');
                }
            }
            let letter = command.ok_or(PathError::NoMoveAtStart)?;
            first = false;
            let relative = letter.is_ascii_lowercase();
            let offset = |point: Point| if relative { absolute(point, current) } else { point };
            match letter.to_ascii_uppercase() {
                b'M' => {
                    let to = offset(reader.point()?);
                    segments.push(Segment::Move(to));
                    current = to;
                    start = to;
                    last_cubic = None;
                    last_quadratic = None;
                }
                b'L' => {
                    let to = offset(reader.point()?);
                    segments.push(Segment::Line(to));
                    current = to;
                    last_cubic = None;
                    last_quadratic = None;
                }
                b'H' => {
                    let x = reader.number()?;
                    let to = Point {
                        x: if relative { current.x + x } else { x },
                        y: current.y,
                    };
                    segments.push(Segment::Line(to));
                    current = to;
                    last_cubic = None;
                    last_quadratic = None;
                }
                b'V' => {
                    let y = reader.number()?;
                    let to = Point {
                        x: current.x,
                        y: if relative { current.y + y } else { y },
                    };
                    segments.push(Segment::Line(to));
                    current = to;
                    last_cubic = None;
                    last_quadratic = None;
                }
                b'C' => {
                    let first_control = offset(reader.point()?);
                    let second_control = offset(reader.point()?);
                    let to = offset(reader.point()?);
                    segments.push(Segment::Cubic(first_control, second_control, to));
                    current = to;
                    last_cubic = Some(second_control);
                    last_quadratic = None;
                }
                b'S' => {
                    let first_control = match last_cubic {
                        Some(previous) => reflect(previous, current),
                        None => current,
                    };
                    let second_control = offset(reader.point()?);
                    let to = offset(reader.point()?);
                    segments.push(Segment::Cubic(first_control, second_control, to));
                    current = to;
                    last_cubic = Some(second_control);
                    last_quadratic = None;
                }
                b'Q' => {
                    let control = offset(reader.point()?);
                    let to = offset(reader.point()?);
                    segments.push(quadratic(current, control, to));
                    current = to;
                    last_cubic = None;
                    last_quadratic = Some(control);
                }
                b'T' => {
                    let control = match last_quadratic {
                        Some(previous) => reflect(previous, current),
                        None => current,
                    };
                    let to = offset(reader.point()?);
                    segments.push(quadratic(current, control, to));
                    current = to;
                    last_cubic = None;
                    last_quadratic = Some(control);
                }
                b'A' => {
                    let radius_x = reader.number()?;
                    let radius_y = reader.number()?;
                    let rotation = reader.number()?;
                    let large = reader.flag()?;
                    let sweep = reader.flag()?;
                    let to = offset(reader.point()?);
                    segments.extend(arc(current, to, radius_x, radius_y, rotation, large, sweep));
                    current = to;
                    last_cubic = None;
                    last_quadratic = None;
                }
                other => return Err(PathError::UnknownCommand(char::from(other))),
            }
        }
        if first {
            return Err(PathError::NoMoveAtStart);
        }
        Ok(Path { segments })
    }
}

/// A cursor over path data.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_separators(&mut self) {
        while self
            .peek()
            .is_some_and(|byte| byte == b' ' || byte == b',' || byte == b'\t' || byte == b'\n' || byte == b'\r')
        {
            self.at += 1;
        }
    }

    fn point(&mut self) -> Result<Point, PathError> {
        let x = self.number()?;
        let y = self.number()?;
        Ok(Point { x, y })
    }

    /// The next number, in the SVG form: an optional sign, digits with at most one point, and an optional exponent.
    fn number(&mut self) -> Result<f64, PathError> {
        self.skip_separators();
        let begin = self.at;
        if self.peek().is_none() {
            return Err(PathError::MissingNumber);
        }
        if self.peek().is_some_and(|byte| byte == b'+' || byte == b'-') {
            self.at += 1;
        }
        let mut digits = 0;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
            digits += 1;
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.at += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            // A sign, a point or a letter where a number was due: a letter is a command the caller refuses.
            return match self.peek() {
                Some(byte) if byte.is_ascii_alphabetic() && self.at == begin => Err(PathError::MissingNumber),
                _ => Err(PathError::BadNumber),
            };
        }
        if self.peek().is_some_and(|byte| byte == b'e' || byte == b'E') {
            let exponent = self.at;
            self.at += 1;
            if self.peek().is_some_and(|byte| byte == b'+' || byte == b'-') {
                self.at += 1;
            }
            let mut exponent_digits = 0;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.at += 1;
                exponent_digits += 1;
            }
            if exponent_digits == 0 {
                self.at = exponent;
            }
        }
        let text = std::str::from_utf8(&self.bytes[begin..self.at]).map_err(|_| PathError::BadNumber)?;
        text.parse::<f64>().map_err(|_| PathError::BadNumber)
    }

    fn flag(&mut self) -> Result<bool, PathError> {
        self.skip_separators();
        match self.peek() {
            Some(b'0') => {
                self.at += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.at += 1;
                Ok(true)
            }
            None => Err(PathError::MissingNumber),
            Some(_) => Err(PathError::BadFlag),
        }
    }
}

/// A point written relative to `current`, as an absolute point.
fn absolute(point: Point, current: Point) -> Point {
    Point {
        x: point.x + current.x,
        y: point.y + current.y,
    }
}

/// The reflection of `control` in `current`: where a smooth curve's control point lies.
fn reflect(control: Point, current: Point) -> Point {
    Point {
        x: 2.0 * current.x - control.x,
        y: 2.0 * current.y - control.y,
    }
}

/// A quadratic curve written as the cubic of the same curve.
fn quadratic(from: Point, control: Point, to: Point) -> Segment {
    let first = Point {
        x: from.x + 2.0 / 3.0 * (control.x - from.x),
        y: from.y + 2.0 / 3.0 * (control.y - from.y),
    };
    let second = Point {
        x: to.x + 2.0 / 3.0 * (control.x - to.x),
        y: to.y + 2.0 / 3.0 * (control.y - to.y),
    };
    Segment::Cubic(first, second, to)
}

/// An elliptical arc from `from` to `to`, by the standard conversion from end points to a centre (the arc's radii are raised
/// when they are too small for the end points). An arc of no length is nothing; an arc with a radius of 0 is a line.
fn arc(from: Point, to: Point, radius_x: f64, radius_y: f64, rotation: f64, large: bool, sweep: bool) -> Vec<Segment> {
    if from == to {
        return Vec::new();
    }
    let (mut rx, mut ry) = (radius_x.abs(), radius_y.abs());
    if rx == 0.0 || ry == 0.0 {
        return vec![Segment::Line(to)];
    }
    let phi = rotation.to_radians();
    let (sin, cos) = phi.sin_cos();
    let half_x = (from.x - to.x) / 2.0;
    let half_y = (from.y - to.y) / 2.0;
    let x1 = cos * half_x + sin * half_y;
    let y1 = -sin * half_x + cos * half_y;
    let lambda = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let (rx2, ry2) = (rx * rx, ry * ry);
    let numerator = rx2 * ry2 - rx2 * y1 * y1 - ry2 * x1 * x1;
    let denominator = rx2 * y1 * y1 + ry2 * x1 * x1;
    let mut coefficient = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        coefficient = -coefficient;
    }
    let cx1 = coefficient * rx * y1 / ry;
    let cy1 = -coefficient * ry * x1 / rx;
    let centre = Point {
        x: cos * cx1 - sin * cy1 + (from.x + to.x) / 2.0,
        y: sin * cx1 + cos * cy1 + (from.y + to.y) / 2.0,
    };
    // The angles of the two end points seen from the centre, in the ellipse's own coordinates.
    let (ux, uy) = ((x1 - cx1) / rx, (y1 - cy1) / ry);
    let (vx, vy) = ((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let start_angle = uy.atan2(ux);
    let mut sweep_angle = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    if !sweep && sweep_angle > 0.0 {
        sweep_angle -= 2.0 * std::f64::consts::PI;
    } else if sweep && sweep_angle < 0.0 {
        sweep_angle += 2.0 * std::f64::consts::PI;
    }
    let pieces = (sweep_angle.abs() / LARGEST_PIECE).ceil().max(1.0) as usize;
    let step = sweep_angle / pieces as f64;
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let point_at = |angle: f64| Point {
        x: centre.x + rx * angle.cos() * cos - ry * angle.sin() * sin,
        y: centre.y + rx * angle.cos() * sin + ry * angle.sin() * cos,
    };
    let tangent_at = |angle: f64| Point {
        x: -rx * angle.sin() * cos - ry * angle.cos() * sin,
        y: -rx * angle.sin() * sin + ry * angle.cos() * cos,
    };
    let mut segments = Vec::with_capacity(pieces);
    for piece in 0..pieces {
        let angle_from = start_angle + step * piece as f64;
        let angle_to = angle_from + step;
        let begin = point_at(angle_from);
        let end_point = if piece + 1 == pieces { to } else { point_at(angle_to) };
        let tangent_from = tangent_at(angle_from);
        let tangent_to = tangent_at(angle_to);
        segments.push(Segment::Cubic(
            Point {
                x: begin.x + handle * tangent_from.x,
                y: begin.y + handle * tangent_from.y,
            },
            Point {
                x: end_point.x - handle * tangent_to.x,
                y: end_point.y - handle * tangent_to.y,
            },
            end_point,
        ));
    }
    segments
}
