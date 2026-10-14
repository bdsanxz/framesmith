//! How many pixels of two pictures differ: the count of `pixelmatch` at its defaults, written again
//! here from the published methods (`docs/dev/comparison.md`, section 4.2).
//!
//! A pixel counts as different when the distance between its two colours is more than
//! [`THRESHOLD`] and it is not an anti-aliased pixel of either picture. The distance is the hybrid
//! distance of lightness and chroma in the OKLab space; anti-aliasing is found by the slope
//! detector of Vyšniauskas. Both pictures are opaque, so alpha plays no part.

use measure::Frame;
use std::sync::OnceLock;

/// The distance above which two colours differ (`pixelmatch`'s `threshold` default). A distance of
/// 1 is black against white.
pub const THRESHOLD: f64 = 0.1;

/// What was found when two pictures were set side by side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The pixels in each picture.
    pub pixels: u64,
    /// The pixels that differ.
    pub differing: u64,
    /// One entry for each pixel, row by row: true where it differs.
    pub mask: Vec<bool>,
}

/// Counts the pixels that differ. The pictures must have the same size.
pub fn compare(first: &Frame, second: &Frame) -> Result<Difference, String> {
    if (first.width, first.height) != (second.width, second.height) {
        return Err(format!(
            "pictures of {}x{} and {}x{} cannot be set side by side",
            first.width, first.height, second.width, second.height
        ));
    }
    let (width, height) = (first.width as usize, first.height as usize);
    let (a, b) = (&first.rgb, &second.rgb);
    let mut mask = vec![false; width * height];
    let mut differing = 0u64;
    for y in 0..height {
        for x in 0..width {
            let at = y * width + x;
            if a[at * 3..at * 3 + 3] == b[at * 3..at * 3 + 3] {
                continue;
            }
            if distance(colour(a, at), colour(b, at)) > THRESHOLD
                && !(antialiased(a, x, y, width, height, b) || antialiased(b, x, y, width, height, a))
            {
                mask[at] = true;
                differing += 1;
            }
        }
    }
    Ok(Difference {
        pixels: (width * height) as u64,
        differing,
        mask,
    })
}

fn colour(rgb: &[u8], at: usize) -> [u8; 3] {
    [rgb[at * 3], rgb[at * 3 + 1], rgb[at * 3 + 2]]
}

/// The linear light of each 8-bit sRGB value.
fn linear_light() -> &'static [f64; 256] {
    static TABLE: OnceLock<[f64; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0; 256];
        for (value, light) in table.iter_mut().enumerate() {
            let unit = value as f64 / 255.0;
            *light = if unit <= 0.04045 {
                unit / 12.92
            } else {
                ((unit + 0.055) / 1.055).powf(2.4)
            };
        }
        table
    })
}

/// The lightness (through Ottosson's toe) and the two opponent axes of a colour in OKLab.
fn oklab(rgb: [u8; 3]) -> [f64; 3] {
    const K1: f64 = 0.206;
    const K2: f64 = 0.03;
    const K3: f64 = (1.0 + K1) / (1.0 + K2);
    let table = linear_light();
    let (r, g, b) = (table[rgb[0] as usize], table[rgb[1] as usize], table[rgb[2] as usize]);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let lightness = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
    let toe = 0.5 * (K3 * lightness - K1 + ((K3 * lightness - K1).powi(2) + 4.0 * K2 * K3 * lightness).sqrt());
    [
        toe,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

/// The distance between two colours: the difference of the lightnesses plus the length of the
/// difference of the chromas.
pub fn distance(first: [u8; 3], second: [u8; 3]) -> f64 {
    let (p, q) = (oklab(first), oklab(second));
    (p[0] - q[0]).abs() + (p[1] - q[1]).hypot(p[2] - q[2])
}

/// The difference in brightness the slope detector uses: the luma of the 8-bit numbers.
fn brightness(first: [u8; 3], second: [u8; 3]) -> f64 {
    let delta = |k: usize| f64::from(i32::from(first[k]) - i32::from(second[k]));
    0.29889531 * delta(0) + 0.58662247 * delta(1) + 0.11448223 * delta(2)
}

/// The pixel's eight neighbours that exist, as a rectangle `(left, top, right, bottom)` of the picture.
fn around(x: usize, y: usize, width: usize, height: usize) -> (usize, usize, usize, usize) {
    (
        x.saturating_sub(1),
        y.saturating_sub(1),
        (x + 1).min(width - 1),
        (y + 1).min(height - 1),
    )
}

/// Whether the pixel has three or more neighbours of the same colour (a pixel on the border has
/// one for free).
fn has_many_siblings(rgb: &[u8], x: usize, y: usize, width: usize, height: usize) -> bool {
    let (left, top, right, bottom) = around(x, y, width, height);
    let mut equal = usize::from(x == left || x == right || y == top || y == bottom);
    let own = (y * width + x) * 3;
    for ny in top..=bottom {
        for nx in left..=right {
            if (nx, ny) == (x, y) {
                continue;
            }
            let other = (ny * width + nx) * 3;
            if rgb[own..own + 3] == rgb[other..other + 3] {
                equal += 1;
                if equal > 2 {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether the pixel at `(x, y)` of `rgb` is anti-aliasing: it has a darker and a lighter
/// neighbour, no more than two neighbours of its own colour, and the darkest or the lightest
/// neighbour has many siblings in both pictures.
fn antialiased(rgb: &[u8], x: usize, y: usize, width: usize, height: usize, other: &[u8]) -> bool {
    let (left, top, right, bottom) = around(x, y, width, height);
    let mut equal = usize::from(x == left || x == right || y == top || y == bottom);
    let (mut darkest, mut lightest) = (0.0f64, 0.0f64);
    let (mut darkest_at, mut lightest_at) = (None, None);
    let own = colour(rgb, y * width + x);
    // Columns first: where two neighbours are equally dark (or light) the first one met is kept.
    for nx in left..=right {
        for ny in top..=bottom {
            if (nx, ny) == (x, y) {
                continue;
            }
            let delta = brightness(own, colour(rgb, ny * width + nx));
            if delta == 0.0 {
                equal += 1;
                if equal > 2 {
                    return false;
                }
            } else if delta < darkest {
                darkest = delta;
                darkest_at = Some((nx, ny));
            } else if delta > lightest {
                lightest = delta;
                lightest_at = Some((nx, ny));
            }
        }
    }
    if darkest == 0.0 || lightest == 0.0 {
        return false;
    }
    let many = |at: Option<(usize, usize)>| {
        at.is_some_and(|(nx, ny)| has_many_siblings(rgb, nx, ny, width, height) && has_many_siblings(other, nx, ny, width, height))
    };
    many(darkest_at) || many(lightest_at)
}
