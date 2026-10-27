//! Blur and brightness of an element's picture (`docs/spec/render-command.md`, section 4.5, rule 11): the weights of the
//! blur, which both renderers use, and the standard deviation of the blur in pixels of the frame.

use crate::Map;

/// The blur and brightness of a group's picture: a standard deviation in the element's pixels, and the factor its colours
/// are multiplied by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub blur: f64,
    pub brightness: f64,
}

impl Filter {
    /// The filter that leaves a picture as it is.
    pub const NONE: Filter = Filter {
        blur: 0.0,
        brightness: 1.0,
    };

    /// Whether the filter leaves the picture as it is: no blur and a brightness of 1.
    pub fn is_none(&self) -> bool {
        self.blur == 0.0 && self.brightness == 1.0
    }

    /// The standard deviation of the blur in pixels of the frame, for an element drawn with `map`: the blur times the square
    /// root of how much the map scales an area.
    pub fn sigma(&self, map: Map) -> f64 {
        self.blur * map.area_scale().sqrt()
    }
}

/// The weights of a blur with the standard deviation `sigma` in pixels, from `-R` to `R` with `R = ⌈4σ⌉`, at most `reach`
/// (a pixel further than `reach` lies outside the frame from every pixel in it): the share of a Gaussian of that standard
/// deviation over each pixel, `Φ((k + ½) / σ) − Φ((k − ½) / σ)`, divided by their sum. One weight of 1 for a `sigma` that
/// is not above 0.
pub fn blur_weights(sigma: f64, reach: u32) -> Vec<f64> {
    if sigma.is_nan() || sigma <= 0.0 {
        return vec![1.0];
    }
    let reach = (4.0 * sigma).ceil().min(f64::from(reach)) as i64;
    let weights: Vec<f64> = (-reach..=reach)
        .map(|k| normal((k as f64 + 0.5) / sigma) - normal((k as f64 - 0.5) / sigma))
        .collect();
    let sum: f64 = weights.iter().sum();
    if sum > 0.0 {
        weights.into_iter().map(|weight| weight / sum).collect()
    } else {
        vec![1.0]
    }
}

/// The distribution function of the standard normal distribution, `Φ(x)`: `erfc(−x / √2) / 2` below 0 and
/// `1 − erfc(x / √2) / 2` from 0, so that the small shares in the tails keep their precision.
fn normal(x: f64) -> f64 {
    let z = x.abs() / std::f64::consts::SQRT_2;
    if x < 0.0 {
        0.5 * complementary_error_function(z)
    } else {
        1.0 - 0.5 * complementary_error_function(z)
    }
}

/// The complementary error function `erfc(z) = 1 − erf(z)` for `z` of 0 or more. Below 3, one less the error function's
/// power series `erf(z) = 2/√π Σ (−1)ⁿ z²ⁿ⁺¹ / (n! (2n + 1))`, summed until a term no longer counts; from 3, where that series
/// loses its precision, the continued fraction `erfc(z) = e^(−z²)/√π · 1/(z + ½/(z + 1/(z + (3/2)/(z + …))))`, worked from
/// its 120th term back.
fn complementary_error_function(z: f64) -> f64 {
    if z.is_nan() {
        return f64::NAN;
    }
    if z < 3.0 {
        let square = z * z;
        let (mut sum, mut power) = (0.0, z);
        for n in 0..200 {
            let term = power / f64::from(2 * n + 1);
            sum += term;
            if term.abs() < 1e-17 * sum.abs().max(1e-300) {
                break;
            }
            power *= -square / f64::from(n + 1);
        }
        return 1.0 - 2.0 / std::f64::consts::PI.sqrt() * sum;
    }
    let mut tail = 0.0;
    for n in (1..=120).rev() {
        tail = (f64::from(n) / 2.0) / (z + tail);
    }
    (-z * z).exp() / std::f64::consts::PI.sqrt() / (z + tail)
}
