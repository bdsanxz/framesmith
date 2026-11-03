//! Separable Gaussian blur (Sprint 7 motion, CAP-006).
//!
//! Elements blur as premultiplied RGBA: the straight-RGBA layer is
//! premultiplied (transparent texels contribute no color, so edges
//! soften without dark halos), blurred with a true Gaussian kernel
//! (radius `ceil(3σ)`, normalized to sum exactly 1), and composited
//! premultiplied over the canvas. A zero sigma is the identity. All
//! math is closed-form and deterministic — no RNG, no worker
//! dependence — so blurred frames re-render byte-identically.

/// One premultiplied-RGBA picture: 4 `f32` texels (r, g, b premultiplied
/// by alpha, alpha 0–1) in row-major order over a canvas-sized plane.
#[derive(Clone, Debug)]
pub struct Premult {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Premult {
    /// Premultiply one canvas-sized straight-RGBA layer (`rgb` raw,
    /// alpha in `layer[3]/255`, exactly the text-layer convention).
    pub fn from_layer(layer: &[u8], w: usize, h: usize) -> Self {
        let mut px = vec![0.0f32; w * h * 4];
        for (t, cell) in px.chunks_exact_mut(4).enumerate() {
            let s = t * 4;
            if s + 3 >= layer.len() {
                break;
            }
            let a = layer[s + 3] as f32 / 255.0;
            cell[0] = layer[s] as f32 * a;
            cell[1] = layer[s + 1] as f32 * a;
            cell[2] = layer[s + 2] as f32 * a;
            cell[3] = a;
        }
        Premult { w, h, px }
    }

    fn texel(&self, i: i64, j: i64) -> [f32; 4] {
        let i = i.clamp(0, self.w as i64 - 1).max(0) as usize;
        let j = j.clamp(0, self.h as i64 - 1).max(0) as usize;
        if self.w == 0 || self.h == 0 {
            return [0.0; 4];
        }
        let s = (j * self.w + i) * 4;
        if s + 3 >= self.px.len() {
            return [0.0; 4];
        }
        [self.px[s], self.px[s + 1], self.px[s + 2], self.px[s + 3]]
    }

    /// Bilinear sample in texel space (centers at integers), clamped at
    /// the borders. Used by the rotated/projected composite paths.
    pub fn sample(&self, x: f64, y: f64) -> [f64; 4] {
        if self.w == 0 || self.h == 0 {
            return [0.0; 4];
        }
        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let p00 = self.texel(x0, y0);
        let p10 = self.texel(x0 + 1, y0);
        let p01 = self.texel(x0, y0 + 1);
        let p11 = self.texel(x0 + 1, y0 + 1);
        let mut out = [0.0f64; 4];
        for c in 0..4 {
            let top = f64::from(p00[c]) * (1.0 - fx) + f64::from(p10[c]) * fx;
            let bot = f64::from(p01[c]) * (1.0 - fx) + f64::from(p11[c]) * fx;
            out[c] = top * (1.0 - fy) + bot * fy;
        }
        out
    }

    /// Blur in place with a Gaussian of standard deviation `sigma`
    /// texels (Sprint 7 motion, CAP-006: the `blur` sigma). A
    /// non-positive or non-finite sigma draws sharp (no-op).
    pub fn blur(&mut self, sigma: f64) {
        if !(sigma > 0.0) || !sigma.is_finite() || self.w == 0 || self.h == 0 {
            return;
        }
        let kernel = gaussian_kernel(sigma);
        let r = kernel.len() / 2;
        let (w, h) = (self.w, self.h);
        // Horizontal pass into a scratch plane, then vertical back.
        let mut tmp = vec![0.0f32; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let mut acc = [0.0f64; 4];
                for (k, weight) in kernel.iter().enumerate() {
                    let s = (i as i64 + k as i64 - r as i64).clamp(0, w as i64 - 1) as usize;
                    let o = (j * w + s) * 4;
                    for (c, slot) in acc.iter_mut().enumerate() {
                        *slot += f64::from(self.px[o + c]) * weight;
                    }
                }
                let o = (j * w + i) * 4;
                for (c, slot) in acc.iter().enumerate() {
                    tmp[o + c] = *slot as f32;
                }
            }
        }
        for j in 0..h {
            for i in 0..w {
                let mut acc = [0.0f64; 4];
                for (k, weight) in kernel.iter().enumerate() {
                    let t = (j as i64 + k as i64 - r as i64).clamp(0, h as i64 - 1) as usize;
                    let o = (t * w + i) * 4;
                    for (c, slot) in acc.iter_mut().enumerate() {
                        *slot += f64::from(tmp[o + c]) * weight;
                    }
                }
                let o = (j * w + i) * 4;
                for (c, slot) in acc.iter().enumerate() {
                    self.px[o + c] = *slot as f32;
                }
            }
        }
    }
}

/// Normalized Gaussian kernel for `sigma`: `2*ceil(3σ)+1` taps summing
/// to exactly 1 (the tail beyond 3σ is cut, like every analytic blur).
pub fn gaussian_kernel(sigma: f64) -> Vec<f64> {
    if !(sigma > 0.0) || !sigma.is_finite() {
        return vec![1.0];
    }
    let r = (3.0 * sigma).ceil() as usize;
    let mut kernel: Vec<f64> = (0..=2 * r)
        .map(|i| {
            let x = i as f64 - r as f64;
            (-x * x / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    let sum: f64 = kernel.iter().sum();
    if sum > 0.0 {
        for v in &mut kernel {
            *v /= sum;
        }
    }
    kernel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_sums_to_one_and_matches_hand_values() {
        // σ=1: weights exp(-x²/2)/sum, x in -3..=3.
        let k = gaussian_kernel(1.0);
        assert_eq!(k.len(), 7);
        let sum: f64 = k.iter().sum();
        assert!((sum - 1.0).abs() < 1e-12, "kernel sums to 1, got {sum}");
        let raw = |x: f64| (-x * x / 2.0).exp();
        let total: f64 = [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0]
            .iter()
            .map(|x| raw(*x))
            .sum();
        for (i, x) in [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0].iter().enumerate() {
            assert!(
                (k[i] - raw(*x) / total).abs() < 1e-12,
                "tap {i} matches the hand value"
            );
        }
        // Symmetry: the kernel reads the same both ways.
        for i in 0..7 {
            assert!((k[i] - k[6 - i]).abs() < 1e-15);
        }
    }

    #[test]
    fn zero_sigma_is_the_identity() {
        assert_eq!(gaussian_kernel(0.0), vec![1.0]);
        assert_eq!(gaussian_kernel(-2.0), vec![1.0]);
        assert_eq!(gaussian_kernel(f64::NAN), vec![1.0]);
    }

    #[test]
    fn blur_spreads_a_dot_and_keeps_its_mass() {
        // One opaque white texel in a 15x15 plane, σ=1.5: the blur
        // spreads the energy (peak drops, neighbors rise) while the
        // total alpha mass stays 1.
        let (w, h) = (15usize, 15usize);
        let mut layer = vec![0u8; w * h * 4];
        let c = (7 * w + 7) * 4;
        layer[c] = 255;
        layer[c + 1] = 255;
        layer[c + 2] = 255;
        layer[c + 3] = 255;
        let mut p = Premult::from_layer(&layer, w, h);
        p.blur(1.5);
        let at = |i: usize, j: usize| -> [f32; 4] {
            let o = (j * w + i) * 4;
            [p.px[o], p.px[o + 1], p.px[o + 2], p.px[o + 3]]
        };
        let peak = at(7, 7)[3];
        assert!(peak < 1.0 && peak > 0.0, "peak softens, got {peak}");
        assert!(at(8, 7)[3] > 0.0, "neighbors gain energy");
        assert!(at(0, 0)[3] < 1e-4, "far corners stay empty");
        let mass: f32 = (0..h)
            .map(|j| (0..w).map(|i| at(i, j)[3]).sum::<f32>())
            .sum();
        assert!(
            (mass - 1.0).abs() < 1e-3,
            "alpha mass conserved, got {mass}"
        );
        // Uniform white stays achromatic: each channel holds 255 times
        // the alpha (premultiplied white).
        for j in 5..=9 {
            for i in 5..=9 {
                let t = at(i, j);
                for c in 0..3 {
                    assert!(
                        (t[c] - 255.0 * t[3]).abs() < 0.05,
                        "premultiplied white {t:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn sharp_premultiply_round_trips() {
        // σ=0 changes nothing: the center texel stays fully opaque.
        let (w, h) = (5usize, 5usize);
        let mut layer = vec![0u8; w * h * 4];
        let c = (2 * w + 2) * 4;
        layer[c] = 10;
        layer[c + 1] = 20;
        layer[c + 2] = 30;
        layer[c + 3] = 255;
        let mut p = Premult::from_layer(&layer, w, h);
        p.blur(0.0);
        let o = (2 * w + 2) * 4;
        assert_eq!(&p.px[o..o + 4], &[10.0, 20.0, 30.0, 1.0]);
    }
}
