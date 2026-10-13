//! A number that counts: how a `<text>` with `count` writes its number. See
//! `docs/spec/native-notation.md`, section 11.

/// A form of number, as `count-format` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    /// The number is a count of seconds, written as minutes and seconds.
    minutes: bool,
    /// A comma between every three digits of the whole part.
    grouped: bool,
    decimals: u32,
}

/// The most decimals a form may ask for.
const DECIMALS: usize = 6;

impl Default for Format {
    /// `0`: a whole number.
    fn default() -> Format {
        Format {
            minutes: false,
            grouped: false,
            decimals: 0,
        }
    }
}

impl Format {
    /// `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` or `m:ss.0`, with up to six decimals.
    pub fn from_name(name: &str) -> Option<Format> {
        let (whole, decimals) = match name.split_once('.') {
            Some((whole, zeros)) => (whole, zeros),
            None => (name, ""),
        };
        let written = name.contains('.');
        if written && (decimals.is_empty() || decimals.len() > DECIMALS || decimals.chars().any(|c| c != '0')) {
            return None;
        }
        let (minutes, grouped) = match whole {
            "0" => (false, false),
            "0,0" => (false, true),
            "m:ss" => (true, false),
            _ => return None,
        };
        Some(Format {
            minutes,
            grouped,
            decimals: decimals.len() as u32,
        })
    }

    /// The number as the text shows it.
    pub fn write(self, value: f64) -> String {
        let value = if value.is_finite() { value } else { 0.0 };
        let scale = 10_u64.pow(self.decimals);
        // Rounded to the last shown place first, a half away from zero; no number is so large
        // that its places are lost.
        let units = (value.abs() * scale as f64).round().min(9.0e15) as u64;
        let (whole, part) = (units / scale, units % scale);
        let mut text = String::new();
        if value < 0.0 && units > 0 {
            text.push('-');
        }
        if self.minutes {
            text.push_str(&format!("{}:{:02}", whole / 60, whole % 60));
        } else if self.grouped {
            let digits = whole.to_string();
            for (place, digit) in digits.chars().enumerate() {
                if place > 0 && (digits.len() - place) % 3 == 0 {
                    text.push(',');
                }
                text.push(digit);
            }
        } else {
            text.push_str(&whole.to_string());
        }
        if self.decimals > 0 {
            text.push_str(&format!(".{part:0width$}", width = self.decimals as usize));
        }
        text
    }
}
