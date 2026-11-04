//! Named media effects (Sprint 11 effects, CAP-003/T004).
//!
//! Each clip's `effect` children apply after the grade chain in
//! document order. Every effect blends its full-strength look with the
//! input by `amount` (0 passes through, 1 applies fully), so a zero
//! amount renders pixel-identical to the effect removed and a half
//! amount lands halfway between no-effect and full-effect pixels.
//! Alpha never moves: effects reshape color only, like the grade.
//! Seeded effects (grain, noise) hash integer texel coordinates, so
//! re-renders are byte-identical with no RNG state.

use crate::blur::{gaussian_kernel, Premult};
use notation::model::{Effect, EffectPreset, EFFECT_GRAIN_PEAK};

/// Apply one clip's effect stack to a premultiplied-RGBA layer in
/// place (Sprint 11 effects, CAP-003/T004): every texel
/// unpremultiplies to straight RGB, takes each effect in document
/// order, and premultiplies back with its alpha untouched.
/// Transparent texels stay transparent. An empty stack (or one whose
/// every amount is zero) is a no-op. Layers are canvas-sized, so the
/// texel index maps straight onto frame coordinates for the spatial
/// presets (blur, vignette, grain, noise, pixelate, edge-detect,
/// emboss, chromatic-aberration). Pure closed-form math plus integer
/// hashing, no RNG state, so effected layers re-render
/// byte-identically.
pub fn apply_effects_to_premult(pm: &mut Premult, effects: &[Effect]) {
    if effects.iter().all(|e| e.is_empty()) {
        return;
    }
    let (w, h) = (pm.w, pm.h);
    if w == 0 || h == 0 {
        return;
    }
    // Straight RGB (0..255) plus alpha working planes.
    let mut rgb = vec![0.0f64; w * h * 3];
    let mut alpha = vec![0.0f64; w * h];
    for (t, cell) in pm.px.chunks_exact(4).enumerate() {
        let a = cell[3].clamp(0.0, 1.0) as f64;
        alpha[t] = a;
        if a <= 0.0 {
            continue;
        }
        let inv = 1.0 / a;
        rgb[t * 3] = cell[0] as f64 * inv;
        rgb[t * 3 + 1] = cell[1] as f64 * inv;
        rgb[t * 3 + 2] = cell[2] as f64 * inv;
    }
    for effect in effects {
        if effect.amount == 0.0 {
            continue;
        }
        apply_one(&mut rgb, &alpha, w, h, effect);
    }
    for (t, cell) in pm.px.chunks_exact_mut(4).enumerate() {
        let a = alpha[t];
        if a <= 0.0 {
            continue;
        }
        cell[0] = (rgb[t * 3] * a) as f32;
        cell[1] = (rgb[t * 3 + 1] * a) as f32;
        cell[2] = (rgb[t * 3 + 2] * a) as f32;
    }
}

/// Luma of a normalized (0..1) pixel.
fn luma(rgb: [f64; 3]) -> f64 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// RGB (0..1) to HSL (hue degrees 0..360, saturation and lightness
/// 0..1). Achromatic pixels report hue 0 deterministically.
fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let mut h = if max == r {
        ((g - b) / d) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    if h < 0.0 {
        h += 360.0;
    }
    (h, s, l)
}

/// HSL back to RGB (0..1).
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (r + m, g + m, b + m)
}

/// One deterministic grain value in [-1, 1) for a (`seed`, cell)
/// pair: FNV-1a over the little-endian bytes, top 53 bits scaled to
/// [0, 1), then centered — the same hash as grade grain, so the
/// `grain` preset at full visibility matches the grade `grain` stage
/// cell for cell. Wrapping arithmetic throughout, so debug and
/// release builds agree bit-exactly.
fn grain_value(seed: i64, cx: i64, cy: i64) -> f64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for word in [seed as u64, cx as u64, cy as u64] {
        for byte in word.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    let f = ((h >> 11) as f64) * (1.0 / 9007199254740992.0);
    f * 2.0 - 1.0
}

/// One deterministic noise value in [-1, 1) for a (`seed`, pixel)
/// pair: its own FNV-1a stream (a different offset basis than grain),
/// so `noise` patterns differ from `grain` at the same seed.
fn noise_value(seed: i64, x: i64, y: i64) -> f64 {
    let mut h: u64 = 0x9e3779b97f4a7c15;
    for word in [seed as u64, x as u64, y as u64] {
        for byte in word.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    let f = ((h >> 11) as f64) * (1.0 / 9007199254740992.0);
    f * 2.0 - 1.0
}

/// Blend one channel from its input toward its full-strength look:
/// the full look clamps first, then `amount` lerps between the two,
/// so a half amount lands exactly halfway between no-effect and
/// full-effect pixels (up to float dust). The outer clamp is a
/// float-dust guard only.
fn blend(px: f64, full: f64, amount: f64) -> f64 {
    (px + (full.clamp(0.0, 1.0) - px) * amount).clamp(0.0, 1.0) * 255.0
}

/// Apply one effect to the straight-RGB working plane (0..255),
/// blended by its amount: each texel moves from its input toward its
/// full-strength look. Only texels with positive alpha move;
/// transparent texels keep their (zero) color. Alpha never moves.
fn apply_one(rgb: &mut [f64], alpha: &[f64], w: usize, h: usize, effect: &Effect) {
    let amount = effect.amount;
    if amount == 0.0 {
        return;
    }
    let at = |buf: &[f64], i: i64, j: i64| -> [f64; 3] {
        let i = i.clamp(0, w as i64 - 1) as usize;
        let j = j.clamp(0, h as i64 - 1) as usize;
        let o = (j * w + i) * 3;
        [buf[o] / 255.0, buf[o + 1] / 255.0, buf[o + 2] / 255.0]
    };
    match effect.preset {
        EffectPreset::Grayscale | EffectPreset::Sepia | EffectPreset::Invert => {
            for t in 0..w * h {
                if alpha[t] <= 0.0 {
                    continue;
                }
                let o = t * 3;
                let px = [rgb[o] / 255.0, rgb[o + 1] / 255.0, rgb[o + 2] / 255.0];
                let full = match effect.preset {
                    EffectPreset::Grayscale => {
                        let l = luma(px);
                        [l, l, l]
                    }
                    EffectPreset::Sepia => [
                        (0.393 * px[0] + 0.769 * px[1] + 0.189 * px[2]).clamp(0.0, 1.0),
                        (0.349 * px[0] + 0.686 * px[1] + 0.168 * px[2]).clamp(0.0, 1.0),
                        (0.272 * px[0] + 0.534 * px[1] + 0.131 * px[2]).clamp(0.0, 1.0),
                    ],
                    _ => [1.0 - px[0], 1.0 - px[1], 1.0 - px[2]],
                };
                for c in 0..3 {
                    rgb[o + c] = blend(px[c], full[c], amount);
                }
            }
        }
        EffectPreset::Brightness | EffectPreset::Contrast | EffectPreset::Saturate => {
            let (kind, level) = match effect.preset {
                EffectPreset::Brightness => (0, effect.params.brightness()),
                EffectPreset::Contrast => (1, effect.params.contrast()),
                _ => (2, effect.params.saturate()),
            };
            for t in 0..w * h {
                if alpha[t] <= 0.0 {
                    continue;
                }
                let o = t * 3;
                let px = [rgb[o] / 255.0, rgb[o + 1] / 255.0, rgb[o + 2] / 255.0];
                let full = match kind {
                    0 => [px[0] * level, px[1] * level, px[2] * level],
                    1 => [
                        (px[0] - 0.5) * level + 0.5,
                        (px[1] - 0.5) * level + 0.5,
                        (px[2] - 0.5) * level + 0.5,
                    ],
                    _ => {
                        let l = luma(px);
                        [
                            l + (px[0] - l) * level,
                            l + (px[1] - l) * level,
                            l + (px[2] - l) * level,
                        ]
                    }
                };
                for c in 0..3 {
                    rgb[o + c] = blend(px[c], full[c], amount);
                }
            }
        }
        EffectPreset::HueRotate => {
            let angle = effect.params.angle();
            for t in 0..w * h {
                if alpha[t] <= 0.0 {
                    continue;
                }
                let o = t * 3;
                let px = [rgb[o] / 255.0, rgb[o + 1] / 255.0, rgb[o + 2] / 255.0];
                let (hh, s, l) = rgb_to_hsl(px[0], px[1], px[2]);
                let (r, g, b) = hsl_to_rgb((hh + angle) % 360.0, s, l);
                let full = [r, g, b];
                for c in 0..3 {
                    rgb[o + c] = blend(px[c], full[c], amount);
                }
            }
        }
        EffectPreset::Blur | EffectPreset::Sharpen => {
            // Both read a blurred copy: `blur` smears by it, `sharpen`
            // pushes away from it (unsharp mask around sigma 1).
            let sigma = match effect.preset {
                EffectPreset::Blur => effect.params.radius(),
                _ => 1.0,
            };
            let kernel = gaussian_kernel(sigma);
            let r = kernel.len() / 2;
            let src = rgb.to_vec();
            // Horizontal pass into scratch, then vertical into blurred.
            let mut tmp = vec![0.0f64; w * h * 3];
            for j in 0..h {
                for i in 0..w {
                    for c in 0..3 {
                        let mut acc = 0.0;
                        for (k, weight) in kernel.iter().enumerate() {
                            let s =
                                (i as i64 + k as i64 - r as i64).clamp(0, w as i64 - 1) as usize;
                            acc += src[(j * w + s) * 3 + c] * weight;
                        }
                        tmp[(j * w + i) * 3 + c] = acc;
                    }
                }
            }
            let mut blurred = vec![0.0f64; w * h * 3];
            for j in 0..h {
                for i in 0..w {
                    for c in 0..3 {
                        let mut acc = 0.0;
                        for (k, weight) in kernel.iter().enumerate() {
                            let s =
                                (j as i64 + k as i64 - r as i64).clamp(0, h as i64 - 1) as usize;
                            acc += tmp[(s * w + i) * 3 + c] * weight;
                        }
                        blurred[(j * w + i) * 3 + c] = acc;
                    }
                }
            }
            let strength = effect.params.strength();
            for t in 0..w * h {
                if alpha[t] <= 0.0 {
                    continue;
                }
                let o = t * 3;
                for c in 0..3 {
                    let px = src[o + c] / 255.0;
                    let full = match effect.preset {
                        EffectPreset::Blur => blurred[o + c] / 255.0,
                        _ => px + (px - blurred[o + c] / 255.0) * strength,
                    };
                    rgb[o + c] = blend(px, full, amount);
                }
            }
        }
        EffectPreset::Vignette => {
            let softness = effect.params.softness().clamp(0.0, 1.0);
            let (w2, h2) = (w as f64, h as f64);
            let (cx, cy) = (w2 / 2.0, h2 / 2.0);
            let max_d = cx.hypot(cy);
            if !(max_d > 0.0) {
                return;
            }
            let e0 = 1.0 - softness;
            for j in 0..h {
                for i in 0..w {
                    let t = j * w + i;
                    if alpha[t] <= 0.0 {
                        continue;
                    }
                    let d = (i as f64 + 0.5 - cx).hypot(j as f64 + 0.5 - cy) / max_d;
                    let fall = if e0 >= 1.0 {
                        if d >= 1.0 {
                            1.0
                        } else {
                            0.0
                        }
                    } else {
                        let x = ((d - e0) / (1.0 - e0)).clamp(0.0, 1.0);
                        x * x * (3.0 - 2.0 * x)
                    };
                    // The full look darkens corners toward black; the
                    // center (fall 0) never moves.
                    let o = t * 3;
                    for c in 0..3 {
                        let px = rgb[o + c] / 255.0;
                        let full = px * (1.0 - fall);
                        rgb[o + c] = blend(px, full, amount);
                    }
                }
            }
        }
        EffectPreset::Grain | EffectPreset::Noise => {
            let seed = effect.params.seed();
            let peak = EFFECT_GRAIN_PEAK / 255.0;
            match effect.preset {
                EffectPreset::Grain => {
                    let size = effect.params.grain_size().max(1) as f64;
                    for j in 0..h {
                        for i in 0..w {
                            let t = j * w + i;
                            if alpha[t] <= 0.0 {
                                continue;
                            }
                            let cx = (i as f64 / size).floor() as i64;
                            let cy = (j as f64 / size).floor() as i64;
                            let n = grain_value(seed, cx, cy);
                            let o = t * 3;
                            for c in 0..3 {
                                let px = rgb[o + c] / 255.0;
                                rgb[o + c] = blend(px, px + peak * n, amount);
                            }
                        }
                    }
                }
                _ => {
                    for j in 0..h {
                        for i in 0..w {
                            let t = j * w + i;
                            if alpha[t] <= 0.0 {
                                continue;
                            }
                            let n = noise_value(seed, i as i64, j as i64);
                            let o = t * 3;
                            for c in 0..3 {
                                let px = rgb[o + c] / 255.0;
                                rgb[o + c] = blend(px, px + peak * n, amount);
                            }
                        }
                    }
                }
            }
        }
        EffectPreset::Pixelate => {
            let size = effect.params.pixel_size().max(1) as usize;
            if size <= 1 {
                return;
            }
            let src = rgb.to_vec();
            for bj in (0..h).step_by(size) {
                for bi in (0..w).step_by(size) {
                    // Block mean over contributing (opaque) texels.
                    let mut sum = [0.0f64; 3];
                    let mut n = 0usize;
                    for j in bj..(bj + size).min(h) {
                        for i in bi..(bi + size).min(w) {
                            let t = j * w + i;
                            if alpha[t] <= 0.0 {
                                continue;
                            }
                            let o = t * 3;
                            sum[0] += src[o];
                            sum[1] += src[o + 1];
                            sum[2] += src[o + 2];
                            n += 1;
                        }
                    }
                    if n == 0 {
                        continue;
                    }
                    let mean = [sum[0] / n as f64, sum[1] / n as f64, sum[2] / n as f64];
                    for j in bj..(bj + size).min(h) {
                        for i in bi..(bi + size).min(w) {
                            let t = j * w + i;
                            if alpha[t] <= 0.0 {
                                continue;
                            }
                            let o = t * 3;
                            for c in 0..3 {
                                let px = src[o + c] / 255.0;
                                let full = mean[c] / 255.0;
                                rgb[o + c] = blend(px, full, amount);
                            }
                        }
                    }
                }
            }
        }
        EffectPreset::Posterize => {
            let levels = effect.params.levels().max(2) as f64;
            for t in 0..w * h {
                if alpha[t] <= 0.0 {
                    continue;
                }
                let o = t * 3;
                for c in 0..3 {
                    let px = rgb[o + c] / 255.0;
                    let full = (px * (levels - 1.0)).round() / (levels - 1.0);
                    rgb[o + c] = blend(px, full, amount);
                }
            }
        }
        EffectPreset::Threshold => {
            let cutoff = effect.params.threshold();
            for t in 0..w * h {
                if alpha[t] <= 0.0 {
                    continue;
                }
                let o = t * 3;
                let px = [rgb[o] / 255.0, rgb[o + 1] / 255.0, rgb[o + 2] / 255.0];
                let v = if luma(px) >= cutoff { 1.0 } else { 0.0 };
                for c in 0..3 {
                    rgb[o + c] = blend(px[c], v, amount);
                }
            }
        }
        EffectPreset::EdgeDetect | EffectPreset::Emboss => {
            let src = rgb.to_vec();
            let strength = effect.params.strength();
            for jj in 0..h as i64 {
                for ii in 0..w as i64 {
                    let t = jj as usize * w + ii as usize;
                    if alpha[t] <= 0.0 {
                        continue;
                    }
                    let (i, j) = (ii, jj);
                    let gx = |di: i64, dj: i64| -> f64 { luma(at(&src, i + di, j + dj)) };
                    let o = t * 3;
                    let px = [src[o] / 255.0, src[o + 1] / 255.0, src[o + 2] / 255.0];
                    let center = luma(px);
                    let full = match effect.preset {
                        EffectPreset::EdgeDetect => {
                            // Sobel magnitude on luma: a hard edge
                            // answers ~4, so quartering maps it to
                            // white at unit strength.
                            let sx = -gx(-1, -1) + gx(1, -1) - 2.0 * gx(-1, 0) + 2.0 * gx(1, 0)
                                - gx(-1, 1)
                                + gx(1, 1);
                            let sy = -gx(-1, -1) - 2.0 * gx(0, -1) - gx(1, -1)
                                + gx(-1, 1)
                                + 2.0 * gx(0, 1)
                                + gx(1, 1);
                            let m = (sx * sx + sy * sy).sqrt() * 0.25 * strength;
                            let v = m.clamp(0.0, 1.0);
                            [v, v, v]
                        }
                        _ => {
                            // Emboss relief on luma with the
                            // [[-2,-1,0],[-1,1,1],[0,1,2]] kernel: it
                            // sums to 1, so subtracting the center
                            // leaves flat areas at middle gray.
                            let conv = -2.0 * gx(-1, -1) - gx(0, -1) - gx(-1, 0)
                                + center
                                + gx(1, 0)
                                + gx(0, 1)
                                + 2.0 * gx(1, 1);
                            let v = (0.5 + (conv - center) * strength * 0.25).clamp(0.0, 1.0);
                            [v, v, v]
                        }
                    };
                    for c in 0..3 {
                        rgb[o + c] = blend(px[c], full[c], amount);
                    }
                }
            }
        }
        EffectPreset::ChromaticAberration => {
            let d = effect.params.distance().round() as i64;
            if d == 0 {
                return;
            }
            let src = rgb.to_vec();
            for jj in 0..h as i64 {
                for ii in 0..w as i64 {
                    let t = jj as usize * w + ii as usize;
                    if alpha[t] <= 0.0 {
                        continue;
                    }
                    let (i, j) = (ii, jj);
                    let o = t * 3;
                    let px = [src[o] / 255.0, src[o + 1] / 255.0, src[o + 2] / 255.0];
                    let red = at(&src, i - d, j)[0];
                    let blue = at(&src, i + d, j)[2];
                    let full = [red, px[1], blue];
                    for c in 0..3 {
                        rgb[o + c] = blend(px[c], full[c], amount);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{EffectParams, EffectPreset};

    fn fx(preset: EffectPreset) -> Effect {
        Effect {
            preset,
            amount: 1.0,
            params: EffectParams::default(),
        }
    }

    fn fx_amount(preset: EffectPreset, amount: f64) -> Effect {
        Effect {
            preset,
            amount,
            params: EffectParams::default(),
        }
    }

    /// 8x8 opaque test card: a red ramp across, a green ramp down,
    /// constant blue — every preset has room to move.
    fn card() -> Premult {
        let (w, h) = (8usize, 8usize);
        let mut layer = vec![0u8; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let o = (j * w + i) * 4;
                layer[o] = (i * 32) as u8;
                layer[o + 1] = (j * 32) as u8;
                layer[o + 2] = 128;
                layer[o + 3] = 255;
            }
        }
        Premult::from_layer(&layer, w, h)
    }

    /// Straight-RGB readback of one texel (0..255), with alpha.
    fn px(pm: &Premult, i: usize, j: usize) -> [f64; 4] {
        let o = (j * pm.w + i) * 4;
        let a = pm.px[o + 3] as f64;
        if a <= 0.0 {
            return [0.0, 0.0, 0.0, 0.0];
        }
        [
            pm.px[o] as f64 / a,
            pm.px[o + 1] as f64 / a,
            pm.px[o + 2] as f64 / a,
            a,
        ]
    }

    /// Largest straight-channel move between two layers.
    fn max_diff(a: &Premult, b: &Premult) -> f64 {
        a.px.iter()
            .zip(b.px.iter())
            .map(|(x, y)| (x - y).abs() as f64)
            .fold(0.0, f64::max)
    }

    #[test]
    fn every_preset_renders_its_visible_effect_at_full_amount() {
        for preset in EffectPreset::ACCEPTED
            .iter()
            .map(|n| EffectPreset::parse(n).expect("accepted preset parses"))
        {
            let mut pm = card();
            let before = pm.clone();
            apply_effects_to_premult(&mut pm, &[fx(preset)]);
            let moved = max_diff(&before, &pm);
            assert!(
                moved > 1.0,
                "{preset:?} shows no visible effect, max move {moved}"
            );
            // Alpha never moves: the card stays opaque.
            for j in 0..8 {
                for i in 0..8 {
                    assert_eq!(px(&pm, i, j)[3], 1.0, "{preset:?} moves alpha");
                }
            }
        }
    }

    #[test]
    fn half_amount_lands_halfway_between_none_and_full() {
        for preset in EffectPreset::ACCEPTED
            .iter()
            .map(|n| EffectPreset::parse(n).expect("accepted preset parses"))
        {
            let base = card();
            let mut full = card();
            apply_effects_to_premult(&mut full, &[fx(preset)]);
            let mut half = card();
            apply_effects_to_premult(&mut half, &[fx_amount(preset, 0.5)]);
            for j in 0..8 {
                for i in 0..8 {
                    let n = px(&base, i, j);
                    let f = px(&full, i, j);
                    let h = px(&half, i, j);
                    for c in 0..3 {
                        let want = n[c] + (f[c] - n[c]) * 0.5;
                        assert!(
                            (h[c] - want).abs() < 2.0,
                            "{preset:?} half amount misses the midpoint at ({i},{j}) ch {c}: got {}, want {want}",
                            h[c]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn zero_amount_renders_pixel_identical_to_the_effect_removed() {
        for preset in EffectPreset::ACCEPTED
            .iter()
            .map(|n| EffectPreset::parse(n).expect("accepted preset parses"))
        {
            let base = card();
            let mut pm = card();
            apply_effects_to_premult(&mut pm, &[fx_amount(preset, 0.0)]);
            assert_eq!(pm.px, base.px, "{preset:?} at zero moves pixels");
        }
        // An empty stack is a no-op too.
        let base = card();
        let mut pm = card();
        apply_effects_to_premult(&mut pm, &[]);
        assert_eq!(pm.px, base.px);
    }

    #[test]
    fn document_order_decides_application_order() {
        // Threshold-then-blur keeps soft gray edges; blur-then-threshold
        // quantizes the smeared picture back to hard black and white.
        let order_a = [fx(EffectPreset::Threshold), fx(EffectPreset::Blur)];
        let order_b = [fx(EffectPreset::Blur), fx(EffectPreset::Threshold)];
        let mut a = card();
        apply_effects_to_premult(&mut a, &order_a);
        let mut b = card();
        apply_effects_to_premult(&mut b, &order_b);
        assert!(
            max_diff(&a, &b) > 5.0,
            "effect order shows no difference, max {}",
            max_diff(&a, &b)
        );
    }

    #[test]
    fn seeded_effects_rerender_byte_identically() {
        for preset in [EffectPreset::Grain, EffectPreset::Noise] {
            let mut seeded = EffectParams::default();
            seeded.seed = Some(7);
            let e = Effect {
                preset,
                amount: 1.0,
                params: seeded,
            };
            let mut a = card();
            apply_effects_to_premult(&mut a, &[e.clone()]);
            let mut b = card();
            apply_effects_to_premult(&mut b, &[e]);
            assert_eq!(a.px, b.px, "{preset:?} re-render differs");
            // A different seed patterns differently.
            let mut other_params = EffectParams::default();
            other_params.seed = Some(8);
            let mut c = card();
            apply_effects_to_premult(
                &mut c,
                &[Effect {
                    preset,
                    amount: 1.0,
                    params: other_params,
                }],
            );
            assert!(max_diff(&a, &c) > 0.0, "{preset:?} ignores its seed");
        }
    }

    #[test]
    fn transparent_texels_stay_transparent() {
        let (w, h) = (8usize, 8usize);
        let mut layer = vec![0u8; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let o = (j * w + i) * 4;
                layer[o] = 200;
                layer[o + 1] = 100;
                layer[o + 2] = 50;
                layer[o + 3] = if i < 4 { 255 } else { 0 };
            }
        }
        let mut pm = Premult::from_layer(&layer, w, h);
        let all: Vec<Effect> = EffectPreset::ACCEPTED
            .iter()
            .map(|n| fx(EffectPreset::parse(n).expect("accepted preset parses")))
            .collect();
        apply_effects_to_premult(&mut pm, &all);
        for j in 0..h {
            for i in 4..w {
                let o = (j * w + i) * 4;
                assert_eq!(pm.px[o + 3], 0.0, "alpha moves at ({i},{j})");
                assert_eq!(pm.px[o], 0.0, "color leaks at ({i},{j})");
                assert_eq!(pm.px[o + 1], 0.0, "color leaks at ({i},{j})");
                assert_eq!(pm.px[o + 2], 0.0, "color leaks at ({i},{j})");
            }
        }
    }
}
