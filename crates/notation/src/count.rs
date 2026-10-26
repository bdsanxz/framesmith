//! The forms of a number that counts, and the words a number is written as in each (`docs/spec/notation.md` section
//! 7.12; `docs/spec/text.md` section 7.1, point 1).

/// A form of `count-format`: a number with `decimals` places, with a comma between every three figures of its whole
/// part when `thousands`; or a count of seconds shown as minutes and seconds, with `decimals` places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountFormat {
    Number { decimals: u8, thousands: bool },
    Minutes { decimals: u8 },
}

/// The most decimals a form may ask for.
const MOST_DECIMALS: usize = 6;

impl CountFormat {
    /// The form a `count-format` writes: `0` or `0,0`, or `m:ss`, each with `.` and one to six `0`s after it or none.
    /// `None` for any other value.
    pub fn read(value: &str) -> Option<CountFormat> {
        let (head, decimals) = match value.split_once('.') {
            Some((head, tail)) => {
                if tail.is_empty() || tail.len() > MOST_DECIMALS || tail.bytes().any(|byte| byte != b'0') {
                    return None;
                }
                (head, tail.len() as u8)
            }
            None => (value, 0),
        };
        match head {
            "0" => Some(CountFormat::Number {
                decimals,
                thousands: false,
            }),
            "0,0" => Some(CountFormat::Number { decimals, thousands: true }),
            "m:ss" => Some(CountFormat::Minutes { decimals }),
            _ => None,
        }
    }

    /// The words `value` is shown as: rounded to its last shown place first, a half away from zero, as the decimal that
    /// writes the number is rounded; a minus sign before a number below zero that does not round to zero.
    pub fn write(self, value: f64) -> String {
        let decimals = match self {
            CountFormat::Number { decimals, .. } | CountFormat::Minutes { decimals } => usize::from(decimals),
        };
        let (whole, fraction) = rounded(value.abs(), decimals);
        let zero = whole.bytes().all(|byte| byte == b'0') && fraction.bytes().all(|byte| byte == b'0');
        let mut words = String::new();
        if value < 0.0 && !zero {
            words.push('-');
        }
        match self {
            CountFormat::Number { thousands, .. } => {
                if thousands {
                    words.push_str(&grouped(&whole));
                } else {
                    words.push_str(&whole);
                }
            }
            CountFormat::Minutes { .. } => {
                let (minutes, seconds) = minutes_and_seconds(&whole);
                words.push_str(&minutes);
                words.push(':');
                if seconds < 10 {
                    words.push('0');
                }
                words.push_str(&seconds.to_string());
            }
        }
        if decimals > 0 {
            words.push('.');
            words.push_str(&fraction);
        }
        words
    }
}

/// The whole part and the `decimals` figures after the point of `value`, which is 0 or more, rounded a half away from
/// zero. The number is taken as the shortest decimal that reads back as it (Rust's own writing of an `f64`), and that
/// decimal is rounded figure by figure, so 2.5 rounds to 3 and 1234.565 to 1234.57.
fn rounded(value: f64, decimals: usize) -> (String, String) {
    let written = format!("{value}");
    let (whole, after) = written.split_once('.').unwrap_or((&written, ""));
    let mut figures: Vec<u8> = whole
        .bytes()
        .chain(after.bytes().chain(std::iter::repeat(b'0')).take(decimals))
        .collect();
    let up = after.as_bytes().get(decimals).is_some_and(|figure| *figure >= b'5');
    if up {
        let mut index = figures.len();
        loop {
            if index == 0 {
                figures.insert(0, b'1');
                break;
            }
            index -= 1;
            if figures[index] == b'9' {
                figures[index] = b'0';
            } else {
                figures[index] += 1;
                break;
            }
        }
    }
    let split = figures.len() - decimals;
    let whole = String::from_utf8(figures[..split].to_vec()).unwrap_or_default();
    let fraction = String::from_utf8(figures[split..].to_vec()).unwrap_or_default();
    // A whole part has no figure 0 before its first other figure, and is `0` at least.
    let trimmed = whole.trim_start_matches('0');
    let whole = if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() };
    (whole, fraction)
}

/// The whole part written with a comma between every three figures, counted from the right.
fn grouped(whole: &str) -> String {
    let mut words = String::new();
    for (index, figure) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            words.push(',');
        }
        words.push(figure);
    }
    words
}

/// A whole number of seconds, written in figures, as whole minutes in figures and the seconds left over.
fn minutes_and_seconds(whole: &str) -> (String, u32) {
    // Division by 60 figure by figure, so that no number of seconds is too long to divide.
    let mut minutes = String::new();
    let mut left: u32 = 0;
    for figure in whole.bytes() {
        let current = left * 10 + u32::from(figure - b'0');
        let quotient = current / 60;
        left = current % 60;
        if !(minutes.is_empty() && quotient == 0) {
            minutes.push(char::from(b'0' + quotient as u8));
        }
    }
    if minutes.is_empty() {
        minutes.push('0');
    }
    (minutes, left)
}
