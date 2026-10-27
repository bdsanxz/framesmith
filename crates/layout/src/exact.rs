//! Decimal amounts for the edges of a box (`docs/spec/render-command.md`, section 4.1, rule 3). A written amount is the decimal
//! it is written as: `1.4%` is 14 tenths of a percent, so 1.4% of 2750 pixels is exactly 38.5 pixels, and a half is found
//! without a float's error. A value a float computed (a measured size, for one) is read from its shortest decimal form.
//! The arithmetic is in decimal digits, so no amount is too large or too small for it: an edge beyond the limit is
//! refused by [`crate::LIMIT`], not by the arithmetic.

use std::cmp::Ordering;

/// The most digits an amount may be shifted by before the arithmetic stops (a guard against an amount that asks for
/// a row of zeros it cannot hold, beyond any written value).
const MAX_DIGITS: usize = 100_000;

/// A decimal number: `sign × digits × 10^exponent`. The digits are kept least significant first, with no zero at
/// either end, so each amount has one form; zero has no digits and no sign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    exponent: i32,
}

impl Decimal {
    /// A whole number of pixels.
    pub fn whole(value: i128) -> Decimal {
        Decimal::from_digits(value < 0, digits_of(value.unsigned_abs()), 0)
    }

    /// The decimal a float stands for, as its shortest form writes it (`{:e}`): `1.4` is 14 tenths, and `1e-40` is one
    /// part in 10^40. `None` for a value that is not finite.
    pub fn of(value: f64) -> Option<Decimal> {
        if !value.is_finite() {
            return None;
        }
        let text = format!("{value:e}");
        let (mantissa, exponent) = text.split_once('e')?;
        let exponent: i32 = exponent.parse().ok()?;
        let negative = mantissa.starts_with('-');
        let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).map(|byte| byte - b'0').rev().collect();
        // The mantissa's digits are a whole number; its first digit stands for 10 to the power of the exponent.
        let shift = exponent.checked_sub(i32::try_from(digits.len()).ok()?.checked_sub(1)?)?;
        Some(Decimal::from_digits(negative, digits, shift))
    }

    /// The decimal a written amount stands for: its sign, its digits as written (most significant first, `0` to `9`), and
    /// its exponent, so `38.5` is `false`, `"385"`, `-1`. `None` for a text that is not digits.
    pub fn from_parts(negative: bool, digits: &str, exponent: i32) -> Option<Decimal> {
        let digits = digits
            .bytes()
            .map(|byte| byte.is_ascii_digit().then(|| byte - b'0'))
            .rev()
            .collect::<Option<Vec<u8>>>()?;
        Some(Decimal::from_digits(negative, digits, exponent))
    }

    /// The nearest float, for a size that a float is to carry on (a value a row or a column computes in floats).
    pub fn to_f64(&self) -> f64 {
        if self.digits.is_empty() {
            return 0.0;
        }
        let mut text = String::new();
        if self.negative {
            text.push('-');
        }
        text.extend(self.digits.iter().rev().map(|digit| char::from(b'0' + digit)));
        text.push_str(&format!("e{}", self.exponent));
        text.parse().unwrap_or(f64::NAN)
    }

    pub fn add(self, other: Decimal) -> Option<Decimal> {
        let (mine, theirs, exponent) = aligned(&self, &other)?;
        if self.negative == other.negative {
            return Some(Decimal::from_digits(self.negative, add_digits(&mine, &theirs), exponent));
        }
        Some(match compare_digits(&mine, &theirs) {
            Ordering::Less => Decimal::from_digits(other.negative, subtract_digits(&theirs, &mine), exponent),
            _ => Decimal::from_digits(self.negative, subtract_digits(&mine, &theirs), exponent),
        })
    }

    pub fn sub(self, other: Decimal) -> Option<Decimal> {
        self.add(other.negated())
    }

    pub fn times(self, other: Decimal) -> Option<Decimal> {
        if self.digits.len() + other.digits.len() > MAX_DIGITS {
            return None;
        }
        let mut products = vec![0_u32; self.digits.len() + other.digits.len()];
        for (at, mine) in self.digits.iter().enumerate() {
            for (to, theirs) in other.digits.iter().enumerate() {
                products[at + to] += u32::from(mine * theirs);
            }
        }
        let mut digits = Vec::with_capacity(products.len() + 1);
        let mut carry = 0_u32;
        for product in products {
            let total = product + carry;
            digits.push((total % 10) as u8);
            carry = total / 10;
        }
        while carry > 0 {
            digits.push((carry % 10) as u8);
            carry /= 10;
        }
        let exponent = self.exponent.checked_add(other.exponent)?;
        Some(Decimal::from_digits(self.negative != other.negative, digits, exponent))
    }

    /// This amount divided by 100: a percentage's share of the length, once multiplied (`render-command.md`, section 4.1).
    pub fn per_hundred(self) -> Option<Decimal> {
        let exponent = self.exponent.checked_sub(2)?;
        Some(Decimal::from_digits(self.negative, self.digits, exponent))
    }

    pub fn is_below_zero(&self) -> bool {
        self.negative
    }

    pub fn is_above_zero(&self) -> bool {
        !self.negative && !self.digits.is_empty()
    }

    /// The smaller of two amounts, or `None` when the arithmetic does not fit.
    pub fn min(self, other: Decimal) -> Option<Decimal> {
        Some(if self.less_than(&other)? { self } else { other })
    }

    pub fn max(self, other: Decimal) -> Option<Decimal> {
        Some(if other.less_than(&self)? { self } else { other })
    }

    /// The whole number nearest to this amount, a half up: the larger whole number when the amount lies exactly half way.
    /// That is the floor of this amount plus a half, which is worked out exactly.
    pub fn round_half_up(self) -> Option<i128> {
        self.add(Decimal::from_digits(false, vec![5], -1))?.floor()
    }

    /// The whole number at or below this amount. `None` when it is beyond an `i128`.
    fn floor(&self) -> Option<i128> {
        let places = usize::try_from(self.exponent.checked_neg()?.max(0)).ok()?;
        let whole_digits = self.digits.get(places..).unwrap_or(&[]);
        let fraction_nonzero = self.digits.iter().take(places).any(|digit| *digit != 0);
        let mut magnitude = whole_digits
            .iter()
            .rev()
            .try_fold(0_u128, |sum, digit| sum.checked_mul(10)?.checked_add(u128::from(*digit)))?;
        if self.exponent > 0 {
            magnitude = magnitude.checked_mul(10_u128.checked_pow(u32::try_from(self.exponent).ok()?)?)?;
        }
        let magnitude = i128::try_from(magnitude).ok()?;
        match (self.negative, fraction_nonzero) {
            (false, _) => Some(magnitude),
            (true, false) => magnitude.checked_neg(),
            (true, true) => magnitude.checked_add(1)?.checked_neg(),
        }
    }

    fn negated(self) -> Decimal {
        Decimal::from_digits(!self.negative, self.digits, self.exponent)
    }

    fn less_than(&self, other: &Decimal) -> Option<bool> {
        Some(self.clone().sub(other.clone())?.negative)
    }

    /// The amount from its sign, its digits (least significant first) and its exponent, in the one form every amount has.
    fn from_digits(negative: bool, mut digits: Vec<u8>, mut exponent: i32) -> Decimal {
        while digits.last() == Some(&0) {
            digits.pop();
        }
        let zeros = digits.iter().take_while(|digit| **digit == 0).count();
        if zeros == digits.len() {
            return Decimal {
                negative: false,
                digits: Vec::new(),
                exponent: 0,
            };
        }
        digits.drain(..zeros);
        exponent = exponent.saturating_add(zeros as i32);
        Decimal {
            negative,
            digits,
            exponent,
        }
    }
}

/// The digits of a whole number, least significant first.
fn digits_of(mut value: u128) -> Vec<u8> {
    let mut digits = Vec::new();
    while value > 0 {
        digits.push((value % 10) as u8);
        value /= 10;
    }
    digits
}

/// The digits of two amounts, each shifted to the smaller of the two exponents, with that exponent.
fn aligned(first: &Decimal, second: &Decimal) -> Option<(Vec<u8>, Vec<u8>, i32)> {
    let exponent = first.exponent.min(second.exponent);
    Some((shifted(first, exponent)?, shifted(second, exponent)?, exponent))
}

fn shifted(value: &Decimal, exponent: i32) -> Option<Vec<u8>> {
    let zeros = usize::try_from(i64::from(value.exponent) - i64::from(exponent)).ok()?;
    if zeros + value.digits.len() > MAX_DIGITS {
        return None;
    }
    let mut digits = vec![0; zeros];
    digits.extend_from_slice(&value.digits);
    Some(digits)
}

fn add_digits(mine: &[u8], theirs: &[u8]) -> Vec<u8> {
    let mut digits = Vec::with_capacity(mine.len().max(theirs.len()) + 1);
    let mut carry = 0_u8;
    for at in 0..mine.len().max(theirs.len()) {
        let total = mine.get(at).copied().unwrap_or(0) + theirs.get(at).copied().unwrap_or(0) + carry;
        digits.push(total % 10);
        carry = total / 10;
    }
    if carry > 0 {
        digits.push(carry);
    }
    digits
}

/// The digits of `larger` less those of `smaller`, which must not be the larger.
fn subtract_digits(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut digits = Vec::with_capacity(larger.len());
    let mut borrow = 0_i8;
    for (at, digit) in larger.iter().enumerate() {
        let mut total = i8::try_from(*digit).unwrap_or(0) - borrow - smaller.get(at).map_or(0, |d| i8::try_from(*d).unwrap_or(0));
        borrow = 0;
        if total < 0 {
            total += 10;
            borrow = 1;
        }
        digits.push(total as u8);
    }
    digits
}

fn compare_digits(mine: &[u8], theirs: &[u8]) -> Ordering {
    let length = |digits: &[u8]| digits.iter().rposition(|digit| *digit != 0).map_or(0, |top| top + 1);
    let (mine_top, theirs_top) = (length(mine), length(theirs));
    mine_top
        .cmp(&theirs_top)
        .then_with(|| mine[..mine_top].iter().rev().cmp(theirs[..theirs_top].iter().rev()))
}

impl From<f64> for Decimal {
    /// The decimal a finite float stands for, as [`Decimal::of`] reads it. A written amount is always finite (the notation
    /// refuses any other before it is placed), so the conversion is for finite values; it panics on any other.
    fn from(value: f64) -> Decimal {
        Decimal::of(value).expect("an amount is a finite number")
    }
}
