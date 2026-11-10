//! Clip-bridge transitions (Sprint 11 transitions, CAP-005/T006).
//!
//! A `transition` element replays the tail of its `from` clip against
//! the head of its `to` clip over the window `[to.start, to.start +
//! duration)`. Neither clip's range moves and the timeline length is
//! unchanged: the bridge only changes which pictures blend inside its
//! window. This module owns the window math (which bridges are active
//! at one instant) and the pixel blending (twenty kinds over recovered
//! pictures); the frame traversal in [`crate::render`] draws the
//! pictures and composites the result.
//!
//! Pictures arrive as opaque RGB draws over flat black and flat white.
//! Every item painter composites with standard source-over onto the
//! background, so one pixel's pair `(over black, over white)`
//! recovers its straight-alpha color exactly (modulo byte rounding):
//! `1 - alpha` is the white-minus-black lift, and the color is the
//! over-black value divided by alpha. Clips using non-standard blend
//! modes bridge approximately (their pair no longer inverts cleanly),
//! which only softens those pixels' coverage, never the frame count.

use notation::model::{Composition, TransitionKind};
use timebase::Time;

/// One bridge active at one frame: document indexes into the render
/// root's clips plus the remapped instants the pictures draw at.
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    /// Local clip index of the outgoing clip.
    pub from: usize,
    /// Local clip index of the incoming clip.
    pub to: usize,
    /// Visible motion of the bridge.
    pub kind: TransitionKind,
    /// Bridge progress in 0..1 at this frame.
    pub progress: f64,
    /// Remapped time of the `from` tail at this frame: the tail
    /// `[from.end - duration, from.end)` replays across the window,
    /// so every borrowed frame shows exactly once.
    pub from_time: Time,
}

/// Bridges active at composition time `now` on the render root: every
/// validated `transition` whose window `[to.start, to.start +
/// duration)` holds `now`, with its progress and remapped `from`
/// time. Membership runs on exact rational times, so the first window
/// frame agrees with the incoming clip's own visibility and the last
/// window frame hands back to it with no gap and no overlap.
pub fn active_transitions(comp: &Composition, now: Time) -> Vec<ActiveTransition> {
    use crate::motion_plan::secs_to_time;
    let mut out = Vec::new();
    for t in &comp.transitions {
        let (Some(to_idx), Some(from_idx)) = (
            comp.clips
                .iter()
                .position(|c| c.id() == Some(t.to.as_str())),
            comp.clips
                .iter()
                .position(|c| c.id() == Some(t.from.as_str())),
        ) else {
            continue;
        };
        if from_idx == to_idx {
            continue;
        }
        let ws = secs_to_time(comp.clips[to_idx].start());
        let we = ws + secs_to_time(t.duration);
        if now < ws || now >= we {
            continue;
        }
        let from_start = secs_to_time(comp.clips[from_idx].start());
        let tail = secs_to_time(comp.clips[from_idx].duration() - t.duration);
        let from_time = (from_start + tail + (now - ws)).max(from_start);
        let progress = (now - ws).to_f64() / t.duration;
        out.push(ActiveTransition {
            from: from_idx,
            to: to_idx,
            kind: t.kind,
            progress: progress.clamp(0.0, 1.0),
            from_time,
        });
    }
    out
}

/// Premultiplied picture: straight color times alpha per pixel, all
/// channels in 0..1. Blending and resampling run here, then the
/// overlay composites over the base frame in one pass.
struct Premult {
    w: usize,
    h: usize,
    px: Vec<[f64; 4]>,
}

impl Premult {
    fn blank(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
        }
    }
}

/// Recover straight-alpha pictures from an opaque RGB draw over flat
/// black and the same draw over flat white. Opaque pixels recover
/// exactly; transparent pixels recover their alpha to byte rounding;
/// semi-transparent edge pixels are exact wherever the painters use
/// standard source-over.
fn recover(black: &[u8], white: &[u8], w: usize, h: usize) -> Premult {
    let mut pm = Premult::blank(w, h);
    for i in 0..w * h {
        let mut lift = 0.0;
        for c in 0..3 {
            let b = black[i * 3 + c] as f64 / 255.0;
            let wh = white[i * 3 + c] as f64 / 255.0;
            lift += wh - b;
        }
        let alpha = (1.0 - lift / 3.0).clamp(0.0, 1.0);
        if alpha * 255.0 < 0.5 {
            continue;
        }
        let mut px = [0.0; 4];
        for c in 0..3 {
            let b = black[i * 3 + c] as f64 / 255.0;
            px[c] = (b / alpha).clamp(0.0, 1.0) * alpha;
        }
        px[3] = alpha;
        pm.px[i] = px;
    }
    pm
}

/// Clamp-to-edge bilinear sample of one premultiplied picture.
fn sample(pm: &Premult, x: f64, y: f64) -> [f64; 4] {
    let w = pm.w as f64;
    let h = pm.h as f64;
    if pm.w == 0 || pm.h == 0 {
        return [0.0; 4];
    }
    let x = x.clamp(0.0, w - 1.0);
    let y = y.clamp(0.0, h - 1.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(pm.w - 1);
    let y1 = (y0 + 1).min(pm.h - 1);
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let mut out = [0.0; 4];
    for c in 0..4 {
        let v00 = pm.px[y0 * pm.w + x0][c];
        let v10 = pm.px[y0 * pm.w + x1][c];
        let v01 = pm.px[y1 * pm.w + x0][c];
        let v11 = pm.px[y1 * pm.w + x1][c];
        out[c] = v00 * (1.0 - fx) * (1.0 - fy)
            + v10 * fx * (1.0 - fy)
            + v01 * (1.0 - fx) * fy
            + v11 * fx * fy;
    }
    out
}

/// Separable box blur over premultiplied channels (clamp edges): one
/// horizontal pass, one vertical pass. Radius 0 returns the picture
/// untouched.
fn box_blur(pm: &Premult, radius: usize) -> Premult {
    if radius == 0 || pm.w == 0 || pm.h == 0 {
        return Premult {
            w: pm.w,
            h: pm.h,
            px: pm.px.clone(),
        };
    }
    let r = radius as isize;
    let mut tmp = Premult::blank(pm.w, pm.h);
    for y in 0..pm.h {
        for x in 0..pm.w {
            let mut acc = [0.0; 4];
            for dx in -r..=r {
                let sx = (x as isize + dx).clamp(0, pm.w as isize - 1) as usize;
                for c in 0..4 {
                    acc[c] += pm.px[y * pm.w + sx][c];
                }
            }
            let n = (2 * r + 1) as f64;
            for c in 0..4 {
                tmp.px[y * pm.w + x][c] = acc[c] / n;
            }
        }
    }
    let mut out = Premult::blank(pm.w, pm.h);
    for y in 0..pm.h {
        for x in 0..pm.w {
            let mut acc = [0.0; 4];
            for dy in -r..=r {
                let sy = (y as isize + dy).clamp(0, pm.h as isize - 1) as usize;
                for c in 0..4 {
                    acc[c] += tmp.px[sy * pm.w + x][c];
                }
            }
            let n = (2 * r + 1) as f64;
            for c in 0..4 {
                out.px[y * pm.w + x][c] = acc[c] / n;
            }
        }
    }
    out
}

fn smoothstep(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

fn lerp_px(a: [f64; 4], b: [f64; 4], w: f64) -> [f64; 4] {
    [
        a[0] + (b[0] - a[0]) * w,
        a[1] + (b[1] - a[1]) * w,
        a[2] + (b[2] - a[2]) * w,
        a[3] + (b[3] - a[3]) * w,
    ]
}

/// Standard source-over: `top` composites over `bot` (premultiplied).
fn over(top: [f64; 4], bot: [f64; 4]) -> [f64; 4] {
    let a = top[3] + bot[3] * (1.0 - top[3]);
    [
        top[0] + bot[0] * (1.0 - top[3]),
        top[1] + bot[1] * (1.0 - top[3]),
        top[2] + bot[2] * (1.0 - top[3]),
        a,
    ]
}

/// Blend two premultiplied pictures at progress `p` in 0..1 per
/// `kind`. At `p = 0` the result is the outgoing picture exactly; at
/// `p = 1` it is the incoming picture exactly; between, every kind
/// blends or moves, never cuts.
fn blend_kind(a: &Premult, b: &Premult, kind: TransitionKind, p: f64) -> Premult {
    debug_assert_eq!((a.w, a.h), (b.w, b.h));
    // Exact window edges: the first bridge frame shows the outgoing
    // picture bit-for-bit and the last hands back to the incoming
    // one, so no pop hides at either seam whatever the kind's curve.
    if p <= 0.0 {
        return Premult {
            w: a.w,
            h: a.h,
            px: a.px.clone(),
        };
    }
    if p >= 1.0 {
        return Premult {
            w: b.w,
            h: b.h,
            px: b.px.clone(),
        };
    }
    let (w, h) = (a.w, a.h);
    let mut out = Premult::blank(w, h);
    let wf = w as f64;
    let hf = h as f64;
    // Blur-dissolve peaks its blur mid-bridge and lands sharp.
    let (a_blur, b_blur);
    let (a_ref, b_ref) = match kind {
        TransitionKind::BlurDissolve => {
            let r = (std::f64::consts::PI * p).sin() * 8.0;
            let ri = r.floor() as usize;
            let f = r - ri as f64;
            let blur_one = |pm: &Premult| {
                let lo = box_blur(pm, ri);
                let hi = box_blur(pm, ri + 1);
                let mut mixed = Premult::blank(pm.w, pm.h);
                for i in 0..pm.px.len() {
                    mixed.px[i] = lerp_px(lo.px[i], hi.px[i], f);
                }
                mixed
            };
            a_blur = blur_one(a);
            b_blur = blur_one(b);
            (&a_blur, &b_blur)
        }
        _ => (a, b),
    };
    match kind {
        // Full-frame blends: smooth (fade), linear (dissolve), through
        // black or white (dips), through peak blur (blur-dissolve).
        TransitionKind::Fade => {
            let wgt = smoothstep(p);
            for i in 0..a_ref.px.len() {
                out.px[i] = lerp_px(a_ref.px[i], b_ref.px[i], wgt);
            }
        }
        TransitionKind::Dissolve | TransitionKind::BlurDissolve => {
            for i in 0..a_ref.px.len() {
                out.px[i] = lerp_px(a_ref.px[i], b_ref.px[i], p);
            }
        }
        TransitionKind::DipBlack | TransitionKind::DipWhite => {
            let k = match kind {
                TransitionKind::DipBlack => [0.0, 0.0, 0.0, 1.0],
                _ => [1.0, 1.0, 1.0, 1.0],
            };
            for i in 0..a_ref.px.len() {
                out.px[i] = if p < 0.5 {
                    lerp_px(a_ref.px[i], k, p * 2.0)
                } else {
                    lerp_px(k, b_ref.px[i], (p - 0.5) * 2.0)
                };
            }
        }
        // Moving edges: the incoming picture replaces the outgoing
        // one behind a vertical, horizontal, or circular front.
        TransitionKind::WipeLeft => {
            for y in 0..h {
                for x in 0..w {
                    out.px[y * w + x] = if (x as f64 + 0.5) < p * wf {
                        b_ref.px[y * w + x]
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        TransitionKind::WipeRight => {
            for y in 0..h {
                for x in 0..w {
                    out.px[y * w + x] = if (x as f64 + 0.5) > (1.0 - p) * wf {
                        b_ref.px[y * w + x]
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        TransitionKind::WipeDown => {
            for y in 0..h {
                for x in 0..w {
                    out.px[y * w + x] = if (y as f64 + 0.5) < p * hf {
                        b_ref.px[y * w + x]
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        TransitionKind::WipeUp => {
            for y in 0..h {
                for x in 0..w {
                    out.px[y * w + x] = if (y as f64 + 0.5) > (1.0 - p) * hf {
                        b_ref.px[y * w + x]
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        // Traveling pictures: slide covers (incoming over static
        // outgoing), push shoves both, cover rises from the bottom.
        TransitionKind::Slide | TransitionKind::CssSlideLeft => {
            let shift = (1.0 - p) * wf;
            let fade = match kind {
                TransitionKind::CssSlideLeft => smoothstep(p),
                _ => 1.0,
            };
            for y in 0..h {
                for x in 0..w {
                    let sx = x as f64 - shift;
                    out.px[y * w + x] = if sx >= 0.0 {
                        let mut incoming = sample(b_ref, sx, y as f64);
                        incoming[3] *= fade;
                        incoming[0] *= fade;
                        incoming[1] *= fade;
                        incoming[2] *= fade;
                        over(incoming, a_ref.px[y * w + x])
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        TransitionKind::CssSlideRight => {
            let shift = (1.0 - p) * wf;
            let fade = smoothstep(p);
            for y in 0..h {
                for x in 0..w {
                    let sx = x as f64 + shift;
                    out.px[y * w + x] = if sx < wf {
                        let mut incoming = sample(b_ref, sx, y as f64);
                        incoming[3] *= fade;
                        incoming[0] *= fade;
                        incoming[1] *= fade;
                        incoming[2] *= fade;
                        over(incoming, a_ref.px[y * w + x])
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        TransitionKind::Push => {
            for y in 0..h {
                for x in 0..w {
                    let ax = x as f64 + p * wf;
                    out.px[y * w + x] = if ax < wf {
                        sample(a_ref, ax, y as f64)
                    } else {
                        sample(b_ref, x as f64 - (1.0 - p) * wf, y as f64)
                    };
                }
            }
        }
        TransitionKind::Cover => {
            let shift = (1.0 - p) * hf;
            for y in 0..h {
                for x in 0..w {
                    let sy = y as f64 - shift;
                    out.px[y * w + x] = if sy >= 0.0 {
                        sample(b_ref, x as f64, sy)
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        // Opening fronts: circle-opens hard, iris feathers.
        TransitionKind::CircleOpen => {
            let diag = (wf * wf + hf * hf).sqrt();
            let r = p * 0.75 * diag;
            for y in 0..h {
                for x in 0..w {
                    let dx = x as f64 + 0.5 - wf / 2.0;
                    let dy = y as f64 + 0.5 - hf / 2.0;
                    out.px[y * w + x] = if dx * dx + dy * dy <= r * r {
                        b_ref.px[y * w + x]
                    } else {
                        a_ref.px[y * w + x]
                    };
                }
            }
        }
        TransitionKind::Iris => {
            let diag = (wf * wf + hf * hf).sqrt();
            let feather = 8.0;
            let r = -feather + p * (0.75 * diag + feather);
            for y in 0..h {
                for x in 0..w {
                    let dx = x as f64 + 0.5 - wf / 2.0;
                    let dy = y as f64 + 0.5 - hf / 2.0;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let wgt = smoothstep((r - dist + feather) / (2.0 * feather));
                    let i = y * w + x;
                    out.px[i] = lerp_px(a_ref.px[i], b_ref.px[i], wgt);
                }
            }
        }
        // CSS gallery: the incoming picture fades in while it rises,
        // sinks, or settles from a zoom.
        TransitionKind::CssFadeUp => {
            let fade = smoothstep(p);
            let rise = (1.0 - p) * 0.12 * hf;
            for y in 0..h {
                for x in 0..w {
                    let mut incoming = sample(b_ref, x as f64, y as f64 + rise);
                    incoming[0] *= fade;
                    incoming[1] *= fade;
                    incoming[2] *= fade;
                    incoming[3] *= fade;
                    out.px[y * w + x] = over(incoming, a_ref.px[y * w + x]);
                }
            }
        }
        TransitionKind::CssFadeDown => {
            let fade = smoothstep(p);
            let sink = (1.0 - p) * 0.12 * hf;
            for y in 0..h {
                for x in 0..w {
                    let mut incoming = sample(b_ref, x as f64, y as f64 - sink);
                    incoming[0] *= fade;
                    incoming[1] *= fade;
                    incoming[2] *= fade;
                    incoming[3] *= fade;
                    out.px[y * w + x] = over(incoming, a_ref.px[y * w + x]);
                }
            }
        }
        TransitionKind::CssZoomIn => {
            let fade = smoothstep(p);
            let s = 0.92 + 0.08 * p;
            for y in 0..h {
                for x in 0..w {
                    let bx = wf / 2.0 + (x as f64 - wf / 2.0) / s;
                    let by = hf / 2.0 + (y as f64 - hf / 2.0) / s;
                    let mut incoming = sample(b_ref, bx, by);
                    incoming[0] *= fade;
                    incoming[1] *= fade;
                    incoming[2] *= fade;
                    incoming[3] *= fade;
                    out.px[y * w + x] = over(incoming, a_ref.px[y * w + x]);
                }
            }
        }
        TransitionKind::CssZoomOut => {
            let fade = smoothstep(p);
            let s = 1.08 - 0.08 * p;
            for y in 0..h {
                for x in 0..w {
                    let bx = wf / 2.0 + (x as f64 - wf / 2.0) / s;
                    let by = hf / 2.0 + (y as f64 - hf / 2.0) / s;
                    let mut incoming = sample(b_ref, bx, by);
                    incoming[0] *= fade;
                    incoming[1] *= fade;
                    incoming[2] *= fade;
                    incoming[3] *= fade;
                    out.px[y * w + x] = over(incoming, a_ref.px[y * w + x]);
                }
            }
        }
    }
    out
}

/// One bridge layer ready to composite: the blended overlay pictures
/// draw at their own instants and arrive as black/white pairs.
pub struct TransitionLayer<'a> {
    pub kind: TransitionKind,
    pub progress: f64,
    pub a_black: &'a [u8],
    pub a_white: &'a [u8],
    pub b_black: &'a [u8],
    pub b_white: &'a [u8],
}

/// Composite bridge overlays over the base frame: each layer blends
/// its recovered pictures per its kind and progress, then composites
/// over the running frame in written order. The base frame keeps its
/// bytes where no layer covers.
pub fn composite_transition_frame(
    base: &[u8],
    w: usize,
    h: usize,
    layers: &[TransitionLayer<'_>],
) -> Vec<u8> {
    let mut frame = base.to_vec();
    if w == 0 || h == 0 || base.len() != w * h * 3 {
        return frame;
    }
    for layer in layers {
        if layer.a_black.len() != base.len()
            || layer.a_white.len() != base.len()
            || layer.b_black.len() != base.len()
            || layer.b_white.len() != base.len()
        {
            continue;
        }
        let a = recover(layer.a_black, layer.a_white, w, h);
        let b = recover(layer.b_black, layer.b_white, w, h);
        let overlay = blend_kind(&a, &b, layer.kind, layer.progress);
        // Later layers composite over the running frame, so the
        // first layer blends over `base` and each next layer over
        // the previous layer's output.
        for i in 0..w * h {
            let o = overlay.px[i];
            let inv = 1.0 - o[3];
            for c in 0..3 {
                let under = frame[i * 3 + c] as f64 / 255.0;
                let v = (o[c] + under * inv) * 255.0;
                frame[i * 3 + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::TransitionKind;

    fn solid(w: usize, h: usize, r: f64, g: f64, b: f64, a: f64) -> Premult {
        Premult {
            w,
            h,
            px: vec![[r * a, g * a, b * a, a]; w * h],
        }
    }

    fn flat_rgb(w: usize, h: usize, r: u8, g: u8, b: u8) -> Vec<u8> {
        let mut out = vec![0u8; w * h * 3];
        for i in 0..w * h {
            out[i * 3] = r;
            out[i * 3 + 1] = g;
            out[i * 3 + 2] = b;
        }
        out
    }

    #[test]
    fn recovery_round_trips_opaque_and_transparent() {
        let w = 8;
        let h = 8;
        let red = flat_rgb(w, h, 255, 0, 0);
        let pm = recover(&red, &red, w, h);
        for px in &pm.px {
            assert_eq!(*px, [1.0, 0.0, 0.0, 1.0]);
        }
        let black = flat_rgb(w, h, 0, 0, 0);
        let white = flat_rgb(w, h, 255, 255, 255);
        let pm = recover(&black, &white, w, h);
        for px in &pm.px {
            assert_eq!(*px, [0.0, 0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn recovery_finds_semi_transparent_coverage() {
        // Half white over black / over white: alpha ~0.5, color white.
        let w = 4;
        let h = 4;
        let black = flat_rgb(w, h, 128, 128, 128);
        let white = flat_rgb(w, h, 255, 255, 255);
        let pm = recover(&black, &white, w, h);
        for px in &pm.px {
            assert!((px[3] - 0.5).abs() < 0.01, "{px:?}");
            let c = px[0] / px[3];
            assert!((c - 1.0).abs() < 0.02, "{px:?}");
        }
    }

    #[test]
    fn every_kind_starts_outgoing_ends_incoming_and_moves_midway() {
        // Opaque red against opaque blue on an even canvas: every
        // kind returns A at p=0 and B at p=1 exactly, and its p=0.5
        // frame differs from both (blend or motion, never a cut).
        let a = solid(16, 16, 1.0, 0.0, 0.0, 1.0);
        let b = solid(16, 16, 0.0, 0.0, 1.0, 1.0);
        for kind in [
            TransitionKind::Fade,
            TransitionKind::Dissolve,
            TransitionKind::WipeLeft,
            TransitionKind::WipeRight,
            TransitionKind::WipeUp,
            TransitionKind::WipeDown,
            TransitionKind::Slide,
            TransitionKind::Push,
            TransitionKind::Cover,
            TransitionKind::CircleOpen,
            TransitionKind::Iris,
            TransitionKind::DipBlack,
            TransitionKind::DipWhite,
            TransitionKind::BlurDissolve,
            TransitionKind::CssFadeUp,
            TransitionKind::CssFadeDown,
            TransitionKind::CssZoomIn,
            TransitionKind::CssZoomOut,
            TransitionKind::CssSlideLeft,
            TransitionKind::CssSlideRight,
        ] {
            let start = blend_kind(&a, &b, kind, 0.0);
            let end = blend_kind(&a, &b, kind, 1.0);
            let mid = blend_kind(&a, &b, kind, 0.5);
            assert_eq!(start.px, a.px, "{kind:?} must start at A exactly");
            assert_eq!(end.px, b.px, "{kind:?} must end at B exactly");
            assert_ne!(mid.px, a.px, "{kind:?} mid must move past A");
            assert_ne!(mid.px, b.px, "{kind:?} mid must not jump to B");
        }
    }

    #[test]
    fn fade_mid_is_the_half_mix() {
        let a = solid(4, 4, 1.0, 0.0, 0.0, 1.0);
        let b = solid(4, 4, 0.0, 0.0, 1.0, 1.0);
        // smoothstep(0.5) is exactly 0.5.
        let mid = blend_kind(&a, &b, TransitionKind::Fade, 0.5);
        assert_eq!(mid.px[0], [0.5, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn wipe_halves_follow_the_edge() {
        let a = solid(16, 16, 1.0, 0.0, 0.0, 1.0);
        let b = solid(16, 16, 0.0, 0.0, 1.0, 1.0);
        let mid = blend_kind(&a, &b, TransitionKind::WipeLeft, 0.5);
        // B wipes in from the left: left half incoming, right half
        // outgoing.
        assert_eq!(mid.px[8 * 16 + 2], [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(mid.px[8 * 16 + 13], [1.0, 0.0, 0.0, 1.0]);
        let mid = blend_kind(&a, &b, TransitionKind::WipeDown, 0.5);
        assert_eq!(mid.px[2 * 16 + 8], [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(mid.px[13 * 16 + 8], [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn circle_opens_from_the_center() {
        let a = solid(16, 16, 1.0, 0.0, 0.0, 1.0);
        let b = solid(16, 16, 0.0, 0.0, 1.0, 1.0);
        let mid = blend_kind(&a, &b, TransitionKind::CircleOpen, 0.5);
        // r = 0.375 * diag beats the center but not the corner.
        assert_eq!(mid.px[8 * 16 + 8], [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(mid.px[0], [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn iris_feathers_its_front() {
        let a = solid(32, 32, 1.0, 0.0, 0.0, 1.0);
        let b = solid(32, 32, 0.0, 0.0, 1.0, 1.0);
        // Near the front radius the weight is strictly partial, so a
        // feathered ring of true purple separates the two sides.
        let mid = blend_kind(&a, &b, TransitionKind::Iris, 0.5);
        let partial = mid
            .px
            .iter()
            .any(|px| px[0] > 0.01 && px[0] < 0.99 && (px[0] - (1.0 - px[2])).abs() < 1e-9);
        assert!(partial, "iris must feather between red and blue");
    }

    #[test]
    fn dip_black_passes_through_black() {
        let a = solid(4, 4, 1.0, 0.0, 0.0, 1.0);
        let b = solid(4, 4, 0.0, 0.0, 1.0, 1.0);
        let mid = blend_kind(&a, &b, TransitionKind::DipBlack, 0.5);
        assert_eq!(mid.px[0], [0.0, 0.0, 0.0, 1.0]);
        let quarter = blend_kind(&a, &b, TransitionKind::DipBlack, 0.25);
        assert_eq!(quarter.px[0], [0.5, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn blur_dissolve_blurs_but_dissolve_does_not() {
        // A hard red/blue split blurred at mid-bridge softens, while
        // the plain dissolve keeps the seam crisp.
        let mut a = solid(16, 16, 1.0, 0.0, 0.0, 1.0);
        let mut b = solid(16, 16, 1.0, 0.0, 0.0, 1.0);
        for y in 0..16 {
            for x in 8..16 {
                a.px[y * 16 + x] = [0.0, 0.0, 1.0, 1.0];
                b.px[y * 16 + x] = [0.0, 0.0, 1.0, 1.0];
            }
        }
        let seam = 8 * 16 + 7;
        let plain = blend_kind(&a, &b, TransitionKind::Dissolve, 0.5);
        assert_eq!(plain.px[seam], [1.0, 0.0, 0.0, 1.0]);
        let soft = blend_kind(&a, &b, TransitionKind::BlurDissolve, 0.5);
        assert!(
            soft.px[seam][0] < 1.0 && soft.px[seam][2] > 0.0,
            "blurred seam must mix, got {:?}",
            soft.px[seam]
        );
    }

    #[test]
    fn css_gallery_fades_while_it_travels() {
        let a = solid(16, 16, 1.0, 0.0, 0.0, 1.0);
        let b = solid(16, 16, 0.0, 0.0, 1.0, 1.0);
        // The incoming picture is translucent mid-bridge (faded) and
        // offset (traveled): a covered pixel mixes both sides. The
        // from-the-right arrivals cover the center; the from-the-left
        // arrival covers the left half at p=0.5, so it probes there.
        for (kind, x, y) in [
            (TransitionKind::CssFadeUp, 8, 8),
            (TransitionKind::CssFadeDown, 8, 8),
            (TransitionKind::CssZoomIn, 8, 8),
            (TransitionKind::CssZoomOut, 8, 8),
            (TransitionKind::CssSlideLeft, 8, 8),
            (TransitionKind::CssSlideRight, 4, 8),
        ] {
            let mid = blend_kind(&a, &b, kind, 0.5);
            let c = mid.px[y * 16 + x];
            assert!(
                c[3] > 0.99,
                "{kind:?} stays opaque over an opaque A, got {c:?}"
            );
            assert!(
                c[0] > 0.01 && c[2] > 0.01,
                "{kind:?} covered pixel must mix both sides, got {c:?}"
            );
        }
    }

    #[test]
    fn composite_keeps_the_base_where_nothing_covers() {
        let w = 8;
        let h = 8;
        let base = flat_rgb(w, h, 0, 128, 0);
        // Empty layer list: the base passes through untouched.
        let out = composite_transition_frame(&base, w, h, &[]);
        assert_eq!(out, base);
        // A fully transparent bridge leaves the base alone too.
        let clear = flat_rgb(w, h, 0, 0, 0);
        let white = flat_rgb(w, h, 255, 255, 255);
        let layer = TransitionLayer {
            kind: TransitionKind::Fade,
            progress: 0.5,
            a_black: &clear,
            a_white: &white,
            b_black: &clear,
            b_white: &white,
        };
        let out = composite_transition_frame(&base, w, h, &[layer]);
        assert_eq!(out, base);
    }

    #[test]
    fn composite_blends_opaque_pictures_over_the_base() {
        let w = 4;
        let h = 4;
        let base = flat_rgb(w, h, 0, 128, 0);
        let red = flat_rgb(w, h, 255, 0, 0);
        let blue = flat_rgb(w, h, 0, 0, 255);
        let layer = TransitionLayer {
            kind: TransitionKind::Fade,
            progress: 0.5,
            a_black: &red,
            a_white: &red,
            b_black: &blue,
            b_white: &blue,
        };
        let out = composite_transition_frame(&base, w, h, &[layer]);
        // smoothstep(0.5) is 0.5: purple over the whole canvas.
        assert_eq!(out.len(), w * h * 3);
        assert_eq!((out[0], out[1], out[2]), (128, 0, 128));
    }

    #[test]
    fn active_windows_follow_the_incoming_start() {
        use notation::model::{BoxClip, Clip, Composition, FillPaint, Fps, Rgb, Transition};
        use timebase::Time;
        fn bbox(id: &str, start: f64, duration: f64) -> Clip {
            Clip::Box(BoxClip {
                asserts: Default::default(),
                id: Some(id.to_string()),
                track: notation::model::TrackKind::Video,
                label: None,
                start_after: None,
                hidden: false,
                start,
                duration,
                x: 0.0,
                y: 0.0,
                width: 64.0,
                height: 64.0,
                fill: Rgb { r: 255, g: 0, b: 0 },
                fill_paint: FillPaint::Solid,
                radius: 0.0,
                border_width: 0.0,
                border_color: Rgb { r: 0, g: 0, b: 0 },
                stroke_position: notation::model::StrokePosition::Inside,
                box_shadow: None,
                blend_mode: notation::model::BlendMode::Normal,
                backdrop_blur: 0.0,
                backdrop_brightness: 1.0,
                overflow: notation::model::Overflow::Visible,
                mask: None,
                clip_path: None,
                opacity: 1.0,
                order: 0,
                line: 1,
                column: 1,
                turn: notation::model::Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::default(),
                effects: Vec::new(),
                vfx: Default::default(),
            })
        }
        let comp = Composition {
            line: 1,
            column: 1,
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 64,
            height: 64,
            duration: 3.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: vec![bbox("a", 0.0, 1.0), bbox("b", 1.0, 1.0)],
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
            keys: Vec::new(),
            marks: Vec::new(),
            inputs: Vec::new(),
            variables: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Default::default(),
            normalize_loc: None,
            grade: notation::model::Grade::default(),
            lane_grades: Vec::new(),
            transitions: vec![Transition {
                from: "a".to_string(),
                to: "b".to_string(),
                duration: 1.0,
                kind: TransitionKind::Fade,
                line: 1,
                column: 1,
            }],
            motion_blur: None,
        };
        // Outside the window: nothing active.
        assert!(active_transitions(&comp, Time::new(29, 30)).is_empty());
        assert!(active_transitions(&comp, Time::new(60, 30)).is_empty());
        // Inside: one bridge, remapped tail time and progress agree.
        let all = active_transitions(&comp, Time::new(45, 30));
        assert_eq!(all.len(), 1);
        let a = &all[0];
        assert_eq!((a.from, a.to), (0, 1));
        assert!((a.progress - 0.5).abs() < 1e-12);
        assert_eq!(a.from_time, Time::new(1, 2));
        let first = &active_transitions(&comp, Time::new(30, 30))[0];
        assert_eq!(first.progress, 0.0);
        assert_eq!(first.from_time, Time::new(0, 1));
    }

    // --- Full draw path: plan + draw through both backends ------------

    use crate::frame::{describe_nested_at, NestView};
    use crate::motion_plan::{frame_time, plan_motions};
    use notation::model::{BoxClip, Clip, Composition, FillPaint, Fps, Rgb, Transition};

    fn full_box(id: &str, start: f64, fill: (u8, u8, u8)) -> Clip {
        Clip::Box(BoxClip {
            asserts: Default::default(),
            id: Some(id.to_string()),
            track: notation::model::TrackKind::Video,
            label: None,
            start_after: None,
            hidden: false,
            start,
            duration: 1.0,
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 32.0,
            fill: Rgb {
                r: fill.0,
                g: fill.1,
                b: fill.2,
            },
            fill_paint: FillPaint::Solid,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: notation::model::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: notation::model::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: notation::model::Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            order: 0,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })
    }

    fn bridge_comp(kind: TransitionKind) -> Composition {
        Composition {
            line: 1,
            column: 1,
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 32,
            height: 32,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: vec![
                full_box("a", 0.0, (255, 0, 0)),
                full_box("b", 1.0, (0, 0, 255)),
            ],
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
            keys: Vec::new(),
            marks: Vec::new(),
            inputs: Vec::new(),
            variables: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Default::default(),
            normalize_loc: None,
            grade: notation::model::Grade::default(),
            lane_grades: Vec::new(),
            transitions: vec![Transition {
                from: "a".to_string(),
                to: "b".to_string(),
                duration: 1.0,
                kind,
                line: 1,
                column: 1,
            }],
            motion_blur: None,
        }
    }

    fn draw_frame_cpu(comp: &Composition, frame: u64) -> Vec<u8> {
        use crate::render::{draw_planned_transition, plan_transition_frame, Backend};
        let motions = plan_motions(comp, 1);
        let nest = NestView::single(comp, &motions);
        let at = frame_time(&comp.fps, frame);
        let desc = describe_nested_at(&nest, 0, frame, at);
        let active = active_transitions(comp, at);
        let mut raster = std::collections::HashMap::new();
        let paths = crate::path_paint::PathCache::new();
        if active.is_empty() {
            let mut out = vec![0u8; 32 * 32 * 3];
            crate::render::draw_description(
                &desc,
                &mut raster,
                &[],
                &crate::render::DynLayouts::default(),
                false,
                &[],
                &[],
                &[],
                &[],
                &paths,
                std::path::Path::new("."),
                &mut out,
            )
            .unwrap();
            return out;
        }
        let planned = plan_transition_frame(&nest, 0, frame, &desc, &active);
        draw_planned_transition(
            &[],
            &[],
            &[],
            &[],
            &[],
            &crate::render::DynLayouts::default(),
            false,
            &mut raster,
            &paths,
            std::path::Path::new("."),
            Backend::Cpu,
            &planned,
        )
        .unwrap()
    }

    #[test]
    fn bridged_frames_blend_midway_and_meet_at_the_seams() {
        // Red [0,1) into blue [1,2) over a 1s fade: the last
        // pre-window frame is full red, the first window frame meets
        // it exactly (p=0 shows the tail bit-for-bit), the middle is
        // purple, and the tail leans blue.
        let comp = bridge_comp(TransitionKind::Fade);
        let pre = draw_frame_cpu(&comp, 29);
        assert_eq!((pre[0], pre[1], pre[2]), (255, 0, 0));
        let first = draw_frame_cpu(&comp, 30);
        assert_eq!(first, pre, "p=0 must meet the outgoing frame exactly");
        let mid = draw_frame_cpu(&comp, 45);
        let o = (16 * 32 + 16) * 3;
        assert_eq!(
            (mid[o], mid[o + 1], mid[o + 2]),
            (128, 0, 128),
            "fade mid must half-mix red and blue"
        );
        let last = draw_frame_cpu(&comp, 59);
        assert!(
            last[o] < 16 && last[o + 2] > 240,
            "fade tail must lean blue, got {:?}",
            (last[o], last[o + 1], last[o + 2])
        );
    }

    #[test]
    fn every_kind_moves_midway_on_the_draw_path() {
        // The full describe-plan-draw path, per kind: the mid-window
        // frame differs from both the outgoing-only and the
        // incoming-only frames (blend or motion, never a cut).
        let plain_red = draw_frame_cpu(&bridge_comp(TransitionKind::Fade), 29);
        let comp_blue = Composition {
            line: 1,
            column: 1,
            transitions: Vec::new(),
            ..bridge_comp(TransitionKind::Fade)
        };
        let plain_blue = draw_frame_cpu(&comp_blue, 45);
        assert_eq!((plain_blue[0], plain_blue[1], plain_blue[2]), (0, 0, 255));
        for kind in [
            TransitionKind::Fade,
            TransitionKind::Dissolve,
            TransitionKind::WipeLeft,
            TransitionKind::WipeRight,
            TransitionKind::WipeUp,
            TransitionKind::WipeDown,
            TransitionKind::Slide,
            TransitionKind::Push,
            TransitionKind::Cover,
            TransitionKind::CircleOpen,
            TransitionKind::Iris,
            TransitionKind::DipBlack,
            TransitionKind::DipWhite,
            TransitionKind::BlurDissolve,
            TransitionKind::CssFadeUp,
            TransitionKind::CssFadeDown,
            TransitionKind::CssZoomIn,
            TransitionKind::CssZoomOut,
            TransitionKind::CssSlideLeft,
            TransitionKind::CssSlideRight,
        ] {
            let mid = draw_frame_cpu(&bridge_comp(kind), 45);
            assert_ne!(mid, plain_red, "{kind:?} mid must move past red");
            assert_ne!(mid, plain_blue, "{kind:?} mid must not jump to blue");
        }
    }

    #[test]
    fn bridged_frames_match_across_backends() {
        // The same planned bridge draws identically on both
        // traversals for flat boxes.
        use crate::render::{draw_planned_transition, plan_transition_frame, Backend};
        let comp = bridge_comp(TransitionKind::WipeLeft);
        let motions = plan_motions(&comp, 1);
        let nest = NestView::single(&comp, &motions);
        let at = frame_time(&comp.fps, 45);
        let desc = describe_nested_at(&nest, 0, 45, at);
        let active = active_transitions(&comp, at);
        assert_eq!(active.len(), 1);
        let planned = plan_transition_frame(&nest, 0, 45, &desc, &active);
        let paths = crate::path_paint::PathCache::new();
        let folder = std::path::Path::new(".");
        let mut raster = std::collections::HashMap::new();
        let cpu = draw_planned_transition(
            &[],
            &[],
            &[],
            &[],
            &[],
            &crate::render::DynLayouts::default(),
            false,
            &mut raster,
            &paths,
            folder,
            Backend::Cpu,
            &planned,
        )
        .unwrap();
        let mut raster = std::collections::HashMap::new();
        let gpu = draw_planned_transition(
            &[],
            &[],
            &[],
            &[],
            &[],
            &crate::render::DynLayouts::default(),
            false,
            &mut raster,
            &paths,
            folder,
            Backend::Gpu,
            &planned,
        )
        .unwrap();
        let cmp = crate::gpu::compare_frames(&cpu, &gpu).unwrap();
        assert!(cmp.pass(), "{}", cmp.report("wipe", 45));
    }
}
