//! Ordered VFX chain (Sprint 11 VFX, CAP-004/T005).
//!
//! Each clip's `vfx` child holds the four stages at most once and they
//! apply after the effects stack in the fixed order noise, warp,
//! displacement, luma matte, regardless of document order. Every
//! stage blends its full-strength look with the input by `amount` (0
//! passes through, 1 applies fully), so a zero amount renders
//! pixel-identical to the stage removed and a half amount lands
//! halfway between no-stage and full-stage pixels. Alpha never grows:
//! stages reshape color or cut alpha only, and transparent texels stay
//! transparent. Seeded noise hashes integer texel coordinates, so
//! re-renders are byte-identical with no RNG state.

use crate::blur::Premult;
use notation::model::{VfxChain, VFX_NOISE_PEAK};

/// One decoded stage picture (Sprint 11 VFX, CAP-004/T005): straight
/// RGBA bytes backing a `displacement` map or `luma-matte` source.
/// Borrowed from the shared decoded-image list, so maps never copy.
#[derive(Debug)]
pub struct VfxMap<'a> {
    pub w: usize,
    pub h: usize,
    pub rgba: &'a [u8],
}

/// Resolved stage pictures for one chain (Sprint 11 VFX, CAP-004/T005):
/// each present file-backed stage carries its decoded picture; a
/// `displacement` without a `map` steers by the layer's own luma and
/// carries none. The render resolve step refuses a vanished source
/// naming its path, so a stage that needs a picture never draws
/// without one.
#[derive(Debug)]
pub struct VfxMaps<'a> {
    pub displacement: Option<VfxMap<'a>>,
    pub luma_matte: Option<VfxMap<'a>>,
}

impl<'a> VfxMaps<'a> {
    /// No stage pictures: file-less chains only.
    pub fn empty() -> Self {
        Self {
            displacement: None,
            luma_matte: None,
        }
    }
}

/// Apply one clip's VFX chain to a premultiplied-RGBA layer in place
/// (Sprint 11 VFX, CAP-004/T005): every texel unpremultiplies to
/// straight RGB, takes each active stage in the fixed order noise,
/// warp, displacement, luma matte, and premultiplies back with its
/// (possibly cut) alpha. Transparent texels stay transparent. An
/// empty chain (or one whose every stage is inert) is a no-op.
/// Layers are canvas-sized, so the texel index maps straight onto
/// frame coordinates. Pure closed-form math plus integer hashing, no
/// RNG state, so chained layers re-render byte-identically.
pub fn apply_vfx_to_premult(pm: &mut Premult, vfx: &VfxChain, maps: &VfxMaps) {
    if vfx.is_empty() {
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
    if let Some(noise) = &vfx.noise {
        if !noise.is_empty() {
            apply_noise(&mut rgb, &alpha, w, h, noise.amount, noise.seed());
        }
    }
    if let Some(warp) = &vfx.warp {
        if !warp.is_empty() {
            apply_resample(
                &mut rgb,
                &mut alpha,
                w,
                h,
                warp.amount,
                warp_offset(warp.grid(), warp.strength(), w, h),
            );
        }
    }
    if let Some(displacement) = &vfx.displacement {
        if !displacement.is_empty() {
            let coords =
                displacement_offset(displacement.scale(), maps.displacement.as_ref(), &rgb, w, h);
            apply_resample(&mut rgb, &mut alpha, w, h, displacement.amount, coords);
        }
    }
    if let Some(matte) = &vfx.luma_matte {
        if !matte.is_empty() {
            if let Some(map) = maps.luma_matte.as_ref() {
                apply_luma_matte(&mut alpha, w, h, matte.amount, map);
            }
        }
    }
    for (t, cell) in pm.px.chunks_exact_mut(4).enumerate() {
        let a = alpha[t];
        if a <= 0.0 {
            cell[0] = 0.0;
            cell[1] = 0.0;
            cell[2] = 0.0;
            cell[3] = 0.0;
            continue;
        }
        cell[0] = (rgb[t * 3] * a) as f32;
        cell[1] = (rgb[t * 3 + 1] * a) as f32;
        cell[2] = (rgb[t * 3 + 2] * a) as f32;
        cell[3] = a as f32;
    }
}

/// One deterministic noise value in [-1, 1) for a (`seed`, pixel)
/// pair: its own FNV-1a stream (a different offset basis than the
/// effects presets), so VFX noise patterns differ from effect noise
/// at the same seed. Wrapping arithmetic throughout, so debug and
/// release builds agree bit-exactly.
fn noise_value(seed: i64, x: i64, y: i64) -> f64 {
    let mut h: u64 = 0x517cc1b727220a95;
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
/// so a half amount lands exactly halfway between no-stage and
/// full-stage pixels (up to float dust). The outer clamp is a
/// float-dust guard only.
fn blend(px: f64, full: f64, amount: f64) -> f64 {
    (px + (full.clamp(0.0, 1.0) - px) * amount).clamp(0.0, 1.0) * 255.0
}

/// Luma of a normalized (0..1) pixel.
fn luma(rgb: [f64; 3]) -> f64 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// Bilinear sample of one channel of an interleaved plane.
fn bilinear(buf: &[f64], w: usize, h: usize, stride: usize, ch: usize, x: f64, y: f64) -> f64 {
    let x = x.clamp(0.0, w as f64 - 1.0);
    let y = y.clamp(0.0, h as f64 - 1.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let at = |i: usize, j: usize| buf[(j * w + i) * stride + ch];
    at(x0, y0) * (1.0 - fx) * (1.0 - fy)
        + at(x1, y0) * fx * (1.0 - fy)
        + at(x0, y1) * (1.0 - fx) * fy
        + at(x1, y1) * fx * fy
}

/// Bilinear sample of one channel of straight RGBA bytes (0..255).
fn bilinear_byte(map: &VfxMap, ch: usize, x: f64, y: f64) -> f64 {
    let (w, h) = (map.w, map.h);
    if w == 0 || h == 0 {
        return 0.0;
    }
    let x = x.clamp(0.0, w as f64 - 1.0);
    let y = y.clamp(0.0, h as f64 - 1.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let at = |i: usize, j: usize| map.rgba[(j * w + i) * 4 + ch] as f64 / 255.0;
    at(x0, y0) * (1.0 - fx) * (1.0 - fy)
        + at(x1, y0) * fx * (1.0 - fy)
        + at(x0, y1) * (1.0 - fx) * fy
        + at(x1, y1) * fx * fy
}

/// Achromatic per-pixel noise (Sprint 11 VFX, CAP-004/T005): every
/// opaque texel moves its channels by the seeded pattern scaled to
/// `amount`, blended toward the noised look. Transparent texels keep
/// their (zero) color and alpha.
fn apply_noise(rgb: &mut [f64], alpha: &[f64], w: usize, h: usize, amount: f64, seed: i64) {
    if amount == 0.0 {
        return;
    }
    let peak = VFX_NOISE_PEAK / 255.0;
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

/// Per-texel source coordinate for the warp stage: a smooth
/// deterministic field over `grid`-pixel cells peaking at `strength`
/// canvas pixels of offset. The field is periodic in the cells, so a
/// larger grid smooths the bend and a zero strength samples in place.
fn warp_offset(grid: u32, strength: f64, w: usize, h: usize) -> Vec<(f64, f64)> {
    const TAU: f64 = std::f64::consts::TAU;
    let g = grid.max(1) as f64;
    let mut out = Vec::with_capacity(w * h);
    for j in 0..h {
        for i in 0..w {
            let x = i as f64;
            let y = j as f64;
            let dx = strength * (TAU * (x * 0.5 + y) / g).sin();
            let dy = strength * (TAU * (x - y * 0.5) / g).cos();
            out.push((x + dx, y + dy));
        }
    }
    out
}

/// Per-texel source coordinate for the displacement stage: the
/// `map` picture's red channel steers x and its green channel steers
/// y around the middle (`scale` peaks the offset); without a map the
/// layer's own luma steers both axes. Map bytes sample across the
/// normalized layer position, so any picture size steers any canvas.
fn displacement_offset(
    scale: f64,
    map: Option<&VfxMap>,
    rgb: &[f64],
    w: usize,
    h: usize,
) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(w * h);
    for j in 0..h {
        for i in 0..w {
            let (r, g) = match map {
                Some(m) => {
                    let u = (i as f64 + 0.5) / w as f64 * m.w as f64 - 0.5;
                    let v = (j as f64 + 0.5) / h as f64 * m.h as f64 - 0.5;
                    (bilinear_byte(m, 0, u, v), bilinear_byte(m, 1, u, v))
                }
                None => {
                    let o = (j * w + i) * 3;
                    let l = luma([rgb[o] / 255.0, rgb[o + 1] / 255.0, rgb[o + 2] / 255.0]);
                    (l, l)
                }
            };
            out.push((
                i as f64 + (r - 0.5) * 2.0 * scale,
                j as f64 + (g - 0.5) * 2.0 * scale,
            ));
        }
    }
    out
}

/// Resample the working planes through per-texel source coordinates
/// (Sprint 11 VFX, CAP-004/T005): color and alpha ride the same
/// bilinear weights, then each texel blends from its input toward
/// its resampled look by `amount`. Shared by warp and displacement,
/// so both bend the layer's own pixels the same way.
fn apply_resample(
    rgb: &mut [f64],
    alpha: &mut [f64],
    w: usize,
    h: usize,
    amount: f64,
    coords: Vec<(f64, f64)>,
) {
    if amount == 0.0 {
        return;
    }
    let src_rgb = rgb.to_vec();
    let src_alpha = alpha.to_vec();
    // Every texel resamples, transparent ones included: a bend pulls
    // neighboring content across the alpha edge, so skipping the
    // transparent texels would freeze the edge in place.
    for (t, (sx, sy)) in coords.iter().enumerate() {
        let px = [
            src_rgb[t * 3] / 255.0,
            src_rgb[t * 3 + 1] / 255.0,
            src_rgb[t * 3 + 2] / 255.0,
        ];
        let full = [
            bilinear(&src_rgb, w, h, 3, 0, *sx, *sy) / 255.0,
            bilinear(&src_rgb, w, h, 3, 1, *sx, *sy) / 255.0,
            bilinear(&src_rgb, w, h, 3, 2, *sx, *sy) / 255.0,
        ];
        let o = t * 3;
        for c in 0..3 {
            rgb[o + c] = blend(px[c], full[c], amount);
        }
        let sampled_alpha = bilinear(&src_alpha, w, h, 1, 0, *sx, *sy).clamp(0.0, 1.0);
        alpha[t] = src_alpha[t] + (sampled_alpha - src_alpha[t]) * amount;
    }
}

/// Cut the layer alpha toward the matte picture's luma (Sprint 11
/// VFX, CAP-004/T005): a white matte keeps the texel, a black matte
/// removes it, blended by `amount`. Color never moves; transparent
/// texels stay transparent.
fn apply_luma_matte(alpha: &mut [f64], w: usize, h: usize, amount: f64, map: &VfxMap) {
    if amount == 0.0 {
        return;
    }
    for j in 0..h {
        for i in 0..w {
            let t = j * w + i;
            if alpha[t] <= 0.0 {
                continue;
            }
            let u = (i as f64 + 0.5) / w as f64 * map.w as f64 - 0.5;
            let v = (j as f64 + 0.5) / h as f64 * map.h as f64 - 0.5;
            let r = bilinear_byte(map, 0, u, v);
            let g = bilinear_byte(map, 1, u, v);
            let b = bilinear_byte(map, 2, u, v);
            let matte = luma([r, g, b]);
            alpha[t] = (alpha[t] * (1.0 - amount * (1.0 - matte))).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{VfxDisplacement, VfxLumaMatte, VfxNoise, VfxWarp};

    fn chain(
        noise: Option<VfxNoise>,
        warp: Option<VfxWarp>,
        displacement: Option<VfxDisplacement>,
        luma_matte: Option<VfxLumaMatte>,
    ) -> VfxChain {
        VfxChain {
            noise,
            warp,
            displacement,
            luma_matte,
        }
    }

    fn noise_only(amount: f64, seed: i64) -> VfxChain {
        chain(
            Some(VfxNoise {
                amount,
                seed: Some(seed),
            }),
            None,
            None,
            None,
        )
    }

    /// 8x8 opaque test card: a red ramp across, a green ramp down,
    /// constant blue — every stage has room to move.
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

    /// Largest absolute move (any channel) between two layers.
    fn max_abs_diff(a: &Premult, b: &Premult) -> f64 {
        a.px.iter()
            .zip(b.px.iter())
            .map(|(x, y)| (x - y).abs() as f64)
            .fold(0.0, f64::max)
    }

    /// Largest straight-channel move (color only) between two layers.
    fn max_color_diff(a: &Premult, b: &Premult) -> f64 {
        a.px.chunks_exact(4)
            .zip(b.px.chunks_exact(4))
            .map(|(x, y)| {
                (x[0] - y[0])
                    .abs()
                    .max((x[1] - y[1]).abs())
                    .max((x[2] - y[2]).abs()) as f64
            })
            .fold(0.0, f64::max)
    }

    /// 4x4 half-white, half-black matte: left columns white.
    /// Leaked on purpose: the test map outlives the function.
    fn half_matte() -> VfxMap<'static> {
        let buf: &'static mut [u8] = Box::leak(Box::new([0u8; 4 * 4 * 4]));
        for j in 0..4 {
            for i in 0..4 {
                let o = (j * 4 + i) * 4;
                let v = if i < 2 { 255 } else { 0 };
                buf[o] = v;
                buf[o + 1] = v;
                buf[o + 2] = v;
                buf[o + 3] = 255;
            }
        }
        VfxMap {
            w: 4,
            h: 4,
            rgba: buf,
        }
    }

    #[test]
    fn every_stage_visibly_changes_pixels_at_full_amount() {
        let maps = VfxMaps::empty();
        let stages: Vec<(&str, VfxChain)> = vec![
            ("noise", noise_only(1.0, 7)),
            (
                "warp",
                chain(
                    None,
                    Some(VfxWarp {
                        amount: 1.0,
                        grid: Some(4),
                        strength: Some(3.0),
                    }),
                    None,
                    None,
                ),
            ),
            (
                "displacement",
                chain(
                    None,
                    None,
                    Some(VfxDisplacement {
                        amount: 1.0,
                        map: None,
                        scale: Some(3.0),
                    }),
                    None,
                ),
            ),
        ];
        for (name, vfx) in stages {
            let mut pm = card();
            let before = pm.clone();
            apply_vfx_to_premult(&mut pm, &vfx, &maps);
            let moved = max_color_diff(&before, &pm);
            assert!(
                moved > 1.0,
                "{name} shows no visible effect, max move {moved}"
            );
            for j in 0..8 {
                for i in 0..8 {
                    assert_eq!(px(&pm, i, j)[3], 1.0, "{name} moves alpha");
                }
            }
        }
        // The luma matte cuts alpha on the black half only.
        let matte = half_matte();
        let maps = VfxMaps {
            displacement: None,
            luma_matte: Some(matte),
        };
        let vfx = chain(
            None,
            None,
            None,
            Some(VfxLumaMatte {
                amount: 1.0,
                src: "m.png".to_string(),
            }),
        );
        let mut pm = card();
        apply_vfx_to_premult(&mut pm, &vfx, &maps);
        assert_eq!(px(&pm, 0, 0)[3], 1.0, "white matte keeps alpha");
        assert_eq!(px(&pm, 7, 7)[3], 0.0, "black matte cuts alpha");
    }

    #[test]
    fn stages_apply_in_the_fixed_order() {
        // The full chain equals noise, then warp, then displacement,
        // then matte applied one at a time in that order.
        let matte = half_matte();
        let with_map = VfxMaps {
            displacement: None,
            luma_matte: Some(matte),
        };
        let full = VfxChain {
            noise: Some(VfxNoise {
                amount: 1.0,
                seed: Some(7),
            }),
            warp: Some(VfxWarp {
                amount: 1.0,
                grid: Some(4),
                strength: Some(2.0),
            }),
            displacement: Some(VfxDisplacement {
                amount: 1.0,
                map: None,
                scale: Some(2.0),
            }),
            luma_matte: Some(VfxLumaMatte {
                amount: 0.5,
                src: "m.png".to_string(),
            }),
        };
        let mut together = card();
        apply_vfx_to_premult(&mut together, &full, &with_map);
        let mut staged = card();
        apply_vfx_to_premult(
            &mut staged,
            &chain(
                Some(VfxNoise {
                    amount: 1.0,
                    seed: Some(7),
                }),
                None,
                None,
                None,
            ),
            &VfxMaps::empty(),
        );
        apply_vfx_to_premult(
            &mut staged,
            &chain(
                None,
                Some(VfxWarp {
                    amount: 1.0,
                    grid: Some(4),
                    strength: Some(2.0),
                }),
                None,
                None,
            ),
            &VfxMaps::empty(),
        );
        apply_vfx_to_premult(
            &mut staged,
            &chain(
                None,
                None,
                Some(VfxDisplacement {
                    amount: 1.0,
                    map: None,
                    scale: Some(2.0),
                }),
                None,
            ),
            &VfxMaps::empty(),
        );
        let matte2 = half_matte();
        apply_vfx_to_premult(
            &mut staged,
            &chain(
                None,
                None,
                None,
                Some(VfxLumaMatte {
                    amount: 0.5,
                    src: "m.png".to_string(),
                }),
            ),
            &VfxMaps {
                displacement: None,
                luma_matte: Some(matte2),
            },
        );
        // The staged path round-trips through f32 premultiplication
        // once per stage while the chain does it once total, so the
        // two agree to float dust (far below one 0..255 level), not to
        // the last bit.
        assert!(
            max_abs_diff(&together, &staged) < 0.01,
            "chain differs from its fixed-order stages, max {}",
            max_abs_diff(&together, &staged)
        );
        // The reverse stage order renders differently: order matters.
        let mut reversed = card();
        let matte3 = half_matte();
        apply_vfx_to_premult(
            &mut reversed,
            &chain(
                None,
                None,
                None,
                Some(VfxLumaMatte {
                    amount: 0.5,
                    src: "m.png".to_string(),
                }),
            ),
            &VfxMaps {
                displacement: None,
                luma_matte: Some(matte3),
            },
        );
        apply_vfx_to_premult(
            &mut reversed,
            &chain(
                None,
                None,
                Some(VfxDisplacement {
                    amount: 1.0,
                    map: None,
                    scale: Some(2.0),
                }),
                None,
            ),
            &VfxMaps::empty(),
        );
        assert!(
            max_color_diff(&together, &reversed) > 1.0,
            "stage order shows no difference"
        );
    }

    #[test]
    fn inert_stages_render_pixel_identical_to_the_stage_removed() {
        let maps = VfxMaps::empty();
        let full = VfxChain {
            noise: Some(VfxNoise {
                amount: 1.0,
                seed: Some(7),
            }),
            warp: Some(VfxWarp {
                amount: 1.0,
                grid: Some(4),
                strength: Some(2.0),
            }),
            displacement: Some(VfxDisplacement {
                amount: 1.0,
                map: None,
                scale: Some(2.0),
            }),
            luma_matte: None,
        };
        // Zeroing any stage matches the chain written without it.
        let variants: Vec<VfxChain> = vec![
            VfxChain {
                noise: Some(VfxNoise {
                    amount: 0.0,
                    seed: Some(7),
                }),
                warp: full.warp.clone(),
                displacement: full.displacement.clone(),
                luma_matte: None,
            },
            VfxChain {
                noise: full.noise.clone(),
                warp: Some(VfxWarp {
                    amount: 1.0,
                    grid: Some(4),
                    strength: Some(0.0),
                }),
                displacement: full.displacement.clone(),
                luma_matte: None,
            },
            VfxChain {
                noise: full.noise.clone(),
                warp: full.warp.clone(),
                displacement: Some(VfxDisplacement {
                    amount: 1.0,
                    map: None,
                    scale: Some(0.0),
                }),
                luma_matte: None,
            },
        ];
        let removed: Vec<VfxChain> = vec![
            VfxChain {
                noise: None,
                warp: full.warp.clone(),
                displacement: full.displacement.clone(),
                luma_matte: None,
            },
            VfxChain {
                noise: full.noise.clone(),
                warp: None,
                displacement: full.displacement.clone(),
                luma_matte: None,
            },
            VfxChain {
                noise: full.noise.clone(),
                warp: full.warp.clone(),
                displacement: None,
                luma_matte: None,
            },
        ];
        for (zeroed, without) in variants.into_iter().zip(removed.into_iter()) {
            let mut a = card();
            apply_vfx_to_premult(&mut a, &zeroed, &maps);
            let mut b = card();
            apply_vfx_to_premult(&mut b, &without, &maps);
            assert_eq!(a.px, b.px, "inert stage differs from the stage removed");
        }
        // The empty chain is a no-op.
        let base = card();
        let mut pm = card();
        apply_vfx_to_premult(&mut pm, &VfxChain::empty(), &maps);
        assert_eq!(pm.px, base.px);
    }

    #[test]
    fn seeded_noise_rerenders_byte_identically() {
        let maps = VfxMaps::empty();
        let mut a = card();
        apply_vfx_to_premult(&mut a, &noise_only(1.0, 7), &maps);
        let mut b = card();
        apply_vfx_to_premult(&mut b, &noise_only(1.0, 7), &maps);
        assert_eq!(a.px, b.px, "re-render differs");
        // A different seed patterns differently.
        let mut c = card();
        apply_vfx_to_premult(&mut c, &noise_only(1.0, 8), &maps);
        assert!(max_color_diff(&a, &c) > 0.0, "noise ignores its seed");
    }

    #[test]
    fn half_amount_lands_halfway_between_none_and_full() {
        let maps = VfxMaps::empty();
        let base = card();
        let mut full = card();
        apply_vfx_to_premult(&mut full, &noise_only(1.0, 7), &maps);
        let mut half = card();
        apply_vfx_to_premult(&mut half, &noise_only(0.5, 7), &maps);
        for j in 0..8 {
            for i in 0..8 {
                let n = px(&base, i, j);
                let f = px(&full, i, j);
                let h = px(&half, i, j);
                for c in 0..3 {
                    let want = n[c] + (f[c] - n[c]) * 0.5;
                    assert!(
                        (h[c] - want).abs() < 2.0,
                        "half amount misses the midpoint at ({i},{j}) ch {c}"
                    );
                }
            }
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
        // Per-texel stages (noise, matte) never touch transparent
        // texels: the cleared half stays cleared.
        let mut pm = Premult::from_layer(&layer, w, h);
        let vfx = chain(
            Some(VfxNoise {
                amount: 1.0,
                seed: Some(7),
            }),
            None,
            None,
            Some(VfxLumaMatte {
                amount: 1.0,
                src: "m.png".to_string(),
            }),
        );
        let matte = half_matte();
        apply_vfx_to_premult(
            &mut pm,
            &vfx,
            &VfxMaps {
                displacement: None,
                luma_matte: Some(matte),
            },
        );
        for j in 0..h {
            for i in 4..w {
                let o = (j * w + i) * 4;
                assert_eq!(pm.px[o + 3], 0.0, "alpha moves at ({i},{j})");
                assert_eq!(pm.px[o], 0.0, "color leaks at ({i},{j})");
                assert_eq!(pm.px[o + 1], 0.0, "color leaks at ({i},{j})");
                assert_eq!(pm.px[o + 2], 0.0, "color leaks at ({i},{j})");
            }
        }
        // Resampling stages bend the layer's own pixels, so a fully
        // transparent layer still draws nothing: there is no content
        // to pull across.
        let mut pm = Premult::from_layer(&vec![0u8; w * h * 4], w, h);
        let vfx = VfxChain {
            noise: None,
            warp: Some(VfxWarp {
                amount: 1.0,
                grid: Some(4),
                strength: Some(2.0),
            }),
            displacement: Some(VfxDisplacement {
                amount: 1.0,
                map: None,
                scale: Some(2.0),
            }),
            luma_matte: None,
        };
        apply_vfx_to_premult(&mut pm, &vfx, &VfxMaps::empty());
        assert!(
            pm.px.iter().all(|&v| v == 0.0),
            "resampling a transparent layer paints content"
        );
        // A bend keeps alpha bounded everywhere on real content.
        let mut pm = Premult::from_layer(&layer, w, h);
        apply_vfx_to_premult(&mut pm, &vfx, &VfxMaps::empty());
        for (t, cell) in pm.px.chunks_exact(4).enumerate() {
            assert!(
                (0.0..=1.0).contains(&cell[3]),
                "alpha escapes 0..1 at texel {t}"
            );
        }
    }
}
