//! Exact amounts for the edges of a box (`docs/spec/render-command.md`, section 4.1, rule 3). A written amount is the decimal
//! it is written as: `1.4%` is 14 tenths of a percent, so 1.4% of 2750 pixels is exactly 38.5 pixels, and a half is found
//! without a float's error. A value a float computed (a measured size, for one) is read from its shortest decimal form.
//! The arithmetic is in whole numbers; an amount too large for it is beyond the limit, as an amount beyond
//! [`crate::LIMIT`] is.

/// A number as a fraction: `numerator` over `denominator`, which is above 0 and shares no factor with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Exact {
    numerator: i128,
    denominator: i128,
}

impl Exact {
    /// A whole number of pixels.
    pub(crate) fn whole(value: i128) -> Exact {
        Exact {
            numerator: value,
            denominator: 1,
        }
    }

    /// The decimal a float stands for, as its shortest form writes it (`{:e}`): `1.4` is 14 tenths, and a value written
    /// with at most 15 significant digits is read exactly. `None` for a value that is not finite or whose digits do not fit.
    pub(crate) fn of(value: f64) -> Option<Exact> {
        if !value.is_finite() {
            return None;
        }
        let text = format!("{value:e}");
        let (mantissa, exponent) = text.split_once('e')?;
        let exponent: i32 = exponent.parse().ok()?;
        let negative = mantissa.starts_with('-');
        let digits: String = mantissa
            .trim_start_matches('-')
            .chars()
            .filter(|character| *character != '.')
            .collect();
        let magnitude: i128 = digits.parse().ok()?;
        // The value is `digits` times ten to the power `shift`.
        let shift = exponent - (digits.len() as i32 - 1);
        let exact = if shift >= 0 {
            Exact::whole(magnitude.checked_mul(10_i128.checked_pow(u32::try_from(shift).ok()?)?)?)
        } else {
            Exact {
                numerator: magnitude,
                denominator: 10_i128.checked_pow(u32::try_from(-shift).ok()?)?,
            }
        };
        Some(if negative { exact.negated()? } else { exact })
    }

    /// The nearest float, for a size that a float is to carry on (a value a row or a column computes in floats).
    pub(crate) fn to_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    pub(crate) fn add(self, other: Exact) -> Option<Exact> {
        let numerator = self
            .numerator
            .checked_mul(other.denominator)?
            .checked_add(other.numerator.checked_mul(self.denominator)?)?;
        let denominator = self.denominator.checked_mul(other.denominator)?;
        Some(Exact::reduced(numerator, denominator))
    }

    pub(crate) fn sub(self, other: Exact) -> Option<Exact> {
        self.add(other.negated()?)
    }

    pub(crate) fn times(self, other: Exact) -> Option<Exact> {
        Some(Exact::reduced(
            self.numerator.checked_mul(other.numerator)?,
            self.denominator.checked_mul(other.denominator)?,
        ))
    }

    /// This amount over a whole number above 0.
    pub(crate) fn over(self, divisor: i128) -> Option<Exact> {
        Some(Exact::reduced(self.numerator, self.denominator.checked_mul(divisor)?))
    }

    pub(crate) fn is_below_zero(self) -> bool {
        self.numerator < 0
    }

    /// The smaller of two amounts, or `None` when the comparison does not fit.
    pub(crate) fn min(self, other: Exact) -> Option<Exact> {
        Some(if self.less_than(other)? { self } else { other })
    }

    pub(crate) fn max(self, other: Exact) -> Option<Exact> {
        Some(if other.less_than(self)? { self } else { other })
    }

    /// The whole number nearest to this amount, a half up: the larger whole number when the amount lies exactly half way.
    /// That is the floor of `(2 * numerator + denominator) / (2 * denominator)`.
    pub(crate) fn round_half_up(self) -> Option<i128> {
        let twice = self.numerator.checked_mul(2)?.checked_add(self.denominator)?;
        Some(twice.div_euclid(self.denominator.checked_mul(2)?))
    }

    fn negated(self) -> Option<Exact> {
        Some(Exact {
            numerator: self.numerator.checked_neg()?,
            denominator: self.denominator,
        })
    }

    fn less_than(self, other: Exact) -> Option<bool> {
        Some(self.numerator.checked_mul(other.denominator)? < other.numerator.checked_mul(self.denominator)?)
    }

    /// The fraction with its common factor taken out, so that its numbers stay as small as they can.
    fn reduced(numerator: i128, denominator: i128) -> Exact {
        let factor = greatest_common_divisor(numerator.unsigned_abs(), denominator.unsigned_abs()).max(1) as i128;
        Exact {
            numerator: numerator / factor,
            denominator: denominator / factor,
        }
    }
}

fn greatest_common_divisor(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
