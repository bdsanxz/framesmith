//! Exact times, frame rates and the frame grid rule.
//!
//! Behaviour is specified in `docs/spec/frame-time.md`.

use std::cmp::Ordering;
use std::fmt;

/// Lowest and highest frame rate accepted in this version.
pub const MIN_FPS: u32 = 1;
pub const MAX_FPS: u32 = 240;
/// Frame rate used when the composition writes none.
pub const DEFAULT_FPS: u32 = 30;
/// A time within one part in this many of a frame counts as that frame.
const TOLERANCE_PARTS: i128 = 1000;
/// Most digits accepted after the decimal point of a written time.
const MAX_FRACTION_DIGITS: usize = 9;
/// Most digits accepted before the decimal point of a written time.
const MAX_WHOLE_DIGITS: usize = 9;

/// A time in seconds, held as an exact fraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Time {
    num: i128,
    den: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Time {
    pub const ZERO: Time = Time { num: 0, den: 1 };

    /// Builds `num / den` in lowest terms. `den` must not be zero.
    pub fn new(num: i128, den: i128) -> Time {
        assert!(den != 0, "a time cannot have a zero denominator");
        let sign = if den < 0 { -1 } else { 1 };
        let g = gcd(num, den).max(1);
        Time {
            num: sign * num / g,
            den: sign * den / g,
        }
    }

    pub fn from_secs(secs: i64) -> Time {
        Time::new(secs as i128, 1)
    }

    /// The time of frame `frame` at `fps` frames per second.
    pub fn of_frame(frame: i64, fps: u32) -> Time {
        Time::new(frame as i128, fps as i128)
    }

    /// Reads a written time: a decimal number of seconds, read exactly.
    pub fn parse(text: &str) -> Option<Time> {
        let text = text.trim();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = match digits.split_once('.') {
            Some((w, f)) => (w, f),
            None => (digits, ""),
        };
        if whole.is_empty() && fraction.is_empty() {
            return None;
        }
        if digits.contains('.') && fraction.is_empty() {
            return None;
        }
        if whole.len() > MAX_WHOLE_DIGITS || fraction.len() > MAX_FRACTION_DIGITS {
            return None;
        }
        if !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut num: i128 = 0;
        for b in whole.bytes().chain(fraction.bytes()) {
            num = num * 10 + (b - b'0') as i128;
        }
        let den = 10i128.pow(fraction.len() as u32);
        Some(Time::new(if negative { -num } else { num }, den))
    }

    pub fn is_zero(self) -> bool {
        self.num == 0
    }

    pub fn is_negative(self) -> bool {
        self.num < 0
    }

    pub fn is_positive(self) -> bool {
        self.num > 0
    }

    pub fn to_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// `self ÷ other` as a binary number. `other` must not be zero.
    pub fn ratio(self, other: Time) -> f64 {
        (self.num * other.den) as f64 / (self.den * other.num) as f64
    }

    /// The frame grid rule: `time × fps`, taken as the whole number it is
    /// within the tolerance of, otherwise rounded up.
    pub fn grid_frame(self, fps: u32) -> i64 {
        let n = self.num * fps as i128;
        let d = self.den;
        let q = n.div_euclid(d);
        let r = n.rem_euclid(d);
        if r * TOLERANCE_PARTS <= d {
            q as i64
        } else {
            (q + 1) as i64
        }
    }

    /// Moves the time onto the frame grid when it is within the tolerance
    /// of a frame time; otherwise returns it unchanged.
    pub fn snapped(self, fps: u32) -> Time {
        let n = self.num * fps as i128;
        let d = self.den;
        let q = n.div_euclid(d);
        let r = n.rem_euclid(d);
        if r * TOLERANCE_PARTS <= d {
            Time::new(q, fps as i128)
        } else if (d - r) * TOLERANCE_PARTS <= d {
            Time::new(q + 1, fps as i128)
        } else {
            self
        }
    }

    /// The fraction as `"num/den"`, the exact form used in JSON.
    pub fn to_exact(self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    pub fn from_exact(text: &str) -> Option<Time> {
        let (n, d) = text.split_once('/')?;
        let num: i128 = n.parse().ok()?;
        let den: i128 = d.parse().ok()?;
        if den <= 0 {
            return None;
        }
        Some(Time::new(num, den))
    }
}

impl Ord for Time {
    fn cmp(&self, other: &Time) -> Ordering {
        (self.num * other.den).cmp(&(other.num * self.den))
    }
}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Time) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::ops::Add for Time {
    type Output = Time;
    fn add(self, other: Time) -> Time {
        Time::new(self.num * other.den + other.num * self.den, self.den * other.den)
    }
}

impl std::ops::Sub for Time {
    type Output = Time;
    fn sub(self, other: Time) -> Time {
        Time::new(self.num * other.den - other.num * self.den, self.den * other.den)
    }
}

/// Shows the time as a decimal number of seconds: exactly when it has at
/// most nine decimal places, otherwise rounded to six.
impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scale = 10i128.pow(MAX_FRACTION_DIGITS as u32);
        let text = if scale % self.den == 0 {
            let scaled = self.num.abs() * (scale / self.den);
            let whole = scaled / scale;
            let frac = format!("{:09}", scaled % scale);
            let frac = frac.trim_end_matches('0');
            let sign = if self.num < 0 { "-" } else { "" };
            if frac.is_empty() {
                format!("{sign}{whole}")
            } else {
                format!("{sign}{whole}.{frac}")
            }
        } else {
            let rounded = format!("{:.6}", self.to_f64());
            rounded.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        f.write_str(&text)
    }
}

impl serde::Serialize for Time {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_exact())
    }
}

impl<'de> serde::Deserialize<'de> for Time {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Time, D::Error> {
        let text = String::deserialize(deserializer)?;
        Time::from_exact(&text).ok_or_else(|| serde::de::Error::custom(format!("\"{text}\" is not an exact time")))
    }
}

/// Why a written frame rate was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FpsError {
    /// Written as a ratio such as `30000/1001`: recognised, not supported yet.
    Ratio,
    /// Not a whole number from 1 to 240.
    Invalid,
}

/// Reads a written frame rate.
pub fn parse_fps(text: &str) -> Result<u32, FpsError> {
    let text = text.trim();
    if let Some((n, d)) = text.split_once('/') {
        let is_number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if is_number(n.trim()) && is_number(d.trim()) {
            return Err(FpsError::Ratio);
        }
        return Err(FpsError::Invalid);
    }
    if text.is_empty() || text.len() > 3 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FpsError::Invalid);
    }
    match text.parse::<u32>() {
        Ok(fps) if (MIN_FPS..=MAX_FPS).contains(&fps) => Ok(fps),
        _ => Err(FpsError::Invalid),
    }
}

/// The number of frames in a composition of `length` at `fps`.
pub fn frame_count(length: Time, fps: u32) -> u64 {
    length.grid_frame(fps).max(1) as u64
}
