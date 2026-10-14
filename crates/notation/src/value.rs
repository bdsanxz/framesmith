//! The value forms (`docs/spec/notation.md` section 4): a number, a number that is whole, a time, a
//! colour. Each reads a written value that has already lost its surrounding white space.

use timebase::Time;

/// Why a written value is not a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotANumber {
    /// It is not written as an optional sign, digits with an optional point, and an optional
    /// exponent: a word, a unit, a percent sign, `inf`, `nan`, an empty value.
    Form,
    /// It is written as a number and is too large to be one: a number must be finite (N9).
    TooLarge,
}

/// The value of a number (N9): an optional `+` or `-`, digits with an optional point (`.5` and `5.`
/// are numbers), and an optional exponent. `inf`, `infinity` and `nan` are not numbers, and neither
/// is a value that is too large to be finite.
pub(crate) fn number(text: &str) -> Result<f64, NotANumber> {
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer = take_digits(bytes, &mut at).len();
    let mut fraction = 0;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        fraction = take_digits(bytes, &mut at).len();
    }
    if integer == 0 && fraction == 0 {
        return Err(NotANumber::Form);
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        if take_digits(bytes, &mut at).is_empty() {
            return Err(NotANumber::Form);
        }
    }
    if at != bytes.len() {
        return Err(NotANumber::Form);
    }
    // The text has the form of a decimal number, which the standard reader takes whole.
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        Ok(_) => Err(NotANumber::TooLarge),
        Err(_) => Err(NotANumber::Form),
    }
}

/// The value of a whole number, or `None` when the text is not a number or has a fraction.
///
/// A number is an optional sign, digits with an optional point (`.5` and `5.` are numbers), and an
/// optional exponent (N9). A value too large to hold is returned as `i64::MAX` or `i64::MIN`: it is
/// whole, and out of every range a caller asks for.
pub(crate) fn whole_number(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let negative = match bytes.first() {
        Some(b'-') => {
            at += 1;
            true
        }
        Some(b'+') => {
            at += 1;
            false
        }
        _ => false,
    };

    let integer = take_digits(bytes, &mut at);
    let mut fraction = &bytes[0..0];
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        fraction = take_digits(bytes, &mut at);
    }
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }

    let mut exponent: i64 = 0;
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        let exponent_negative = match bytes.get(at) {
            Some(b'-') => {
                at += 1;
                true
            }
            Some(b'+') => {
                at += 1;
                false
            }
            _ => false,
        };
        let digits = take_digits(bytes, &mut at);
        if digits.is_empty() {
            return None;
        }
        for &digit in digits {
            // An exponent beyond a billion decides nothing more than "very large or very small".
            exponent = (exponent * 10 + i64::from(digit - b'0')).min(1_000_000_000);
        }
        if exponent_negative {
            exponent = -exponent;
        }
    }
    if at != bytes.len() {
        return None;
    }

    // The value is `digits` times ten to the power `scale`.
    let mut digits: Vec<u8> = integer.iter().chain(fraction).copied().skip_while(|&digit| digit == b'0').collect();
    let scale = exponent - fraction.len() as i64;
    if digits.is_empty() {
        return Some(0);
    }
    let magnitude = if scale >= 0 {
        if digits.len() as i64 + scale > 18 {
            None
        } else {
            digits.extend(std::iter::repeat_n(b'0', scale as usize));
            Some(digits_to_u64(&digits))
        }
    } else {
        let cut = scale.unsigned_abs();
        if cut >= digits.len() as u64 {
            // Something between 0 and 1, not zero.
            return None;
        }
        let keep = digits.len() - cut as usize;
        if digits[keep..].iter().any(|&digit| digit != b'0') {
            return None;
        }
        if keep > 18 {
            None
        } else {
            Some(digits_to_u64(&digits[..keep]))
        }
    };
    Some(match (magnitude, negative) {
        (Some(value), false) => value as i64,
        (Some(value), true) => -(value as i64),
        (None, false) => i64::MAX,
        (None, true) => i64::MIN,
    })
}

fn take_digits<'a>(bytes: &'a [u8], at: &mut usize) -> &'a [u8] {
    let start = *at;
    while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
        *at += 1;
    }
    &bytes[start..*at]
}

/// At most eighteen digits, so the value fits.
fn digits_to_u64(digits: &[u8]) -> u64 {
    digits.iter().fold(0u64, |value, &digit| value * 10 + u64::from(digit - b'0'))
}

/// A written time: digits with an optional point, at most nine digits on each side, no sign, no
/// exponent. `.5` is a time; `5.` is not.
pub(crate) fn time(text: &str) -> Option<Time> {
    if text.starts_with('-') {
        return None;
    }
    Time::parse(text)
}

/// A colour written `#rrggbb` or `#rrggbbaa`, as red, green, blue and alpha.
pub(crate) fn colour(text: &str) -> Option<[u8; 4]> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut parts = [0, 0, 0, 255];
    for (part, pair) in parts.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *part = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(parts)
}
