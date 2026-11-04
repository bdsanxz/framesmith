//! Sprint 8 look paint stack (T004): stroke positions, box shadows,
//! blend modes, backdrop sampling, overflow clipping, and masks.
//!
//! A `box` with any paint-stack feature paints in one
//! layout-then-paint pass in this order: the backdrop samples the
//! flattened layers beneath before the element paints, the shadow
//! paints below the element (the background stays transparent so it
//! shows through), the stroke paints above the fill at its position,
//! the blend composites with the layers beneath, overflow clips the
//! body at the box, and the mask applies once over self-first
//! content. Boxes without any feature keep the historical direct
//! paths byte-identically (see `render::draw_box`).

use crate::blur::Premult;
use crate::frame::ResolvedBox;
use crate::render::{
    blur_margin, blur_on, draw_box_layer, fill_image_decoded, item_turn, layer_blend,
    rr_coverage, warp_bounds,
};
use crate::turn::Turn;
use notation::model::{BlendMode, MaskShape, Overflow, StrokePosition};
use std::path::Path;

/// One blended texel (Sprint 8 look, T004): `src` is the straight
/// element color, `dst` the canvas color beneath, `a` the element
/// alpha (0 transparent to 1 opaque). `normal` is source-over;
/// `multiply`, `screen`, and `overlay` blend channel by channel, then
/// composite under the alpha — a zero alpha always keeps `dst`.
pub fn blend_texel(dst: [u8; 3], src: [u8; 3], a: f64, mode: BlendMode) -> [u8; 3] {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.0 {
        return dst;
    }
    let mut out = [0u8; 3];
    for c in 0..3 {
        let (s, d) = (f64::from(src[c]), f64::from(dst[c]));
        let m = match mode {
            BlendMode::Normal => s,
            BlendMode::Multiply => s * d / 255.0,
            BlendMode::Screen => 255.0 - (255.0 - s) * (255.0 - d) / 255.0,
            BlendMode::Overlay => {
                if d < 128.0 {
                    2.0 * s * d / 255.0
                } else {
                    255.0 - 2.0 * (255.0 - s) * (255.0 - d) / 255.0
                }
            }
        };
        // Same rounding as `blend_over`: half-up via truncation, so a
        // normal blend matches the historical source-over exactly.
        out[c] = (m * a + d * (1.0 - a) + 0.5).clamp(0.0, 255.0) as u8;
    }
    out
}

/// Stroke rects of one scaled box (Sprint 8 look, T004): the outer
/// paint rect with its radius, the fill rect with its radius, and the
/// fill inset (the positioned stroke width inside the box).
pub type StrokeRects = ((f64, f64, f64, f64, f64), (f64, f64, f64, f64, f64), f64);

/// Stroke geometry of one scaled box (Sprint 8 look, T004): the outer
/// paint rect with its radius, the fill rect with its radius, and the
/// fill inset (the border width the image fill shrinks by). `(x0, y0)`
/// is the scaled top-left, `(x1, y1)` the scaled bottom-right, `r`
/// the scaled radius clamped to the box, `bw` the scaled border
/// width. `inside` rings the inner edge (the historical border:
/// outer is the box, fill insets by `bw`); `center` straddles the
/// edge (outer grows `bw/2`, fill shrinks `bw/2`); `outside` rings
/// the outer edge (outer grows `bw`, fill is the box). A zero width
/// paints the box in every position.
#[allow(clippy::too_many_arguments)]
pub fn stroke_geometry(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    r: f64,
    bw: f64,
    pos: StrokePosition,
) -> StrokeRects {
    let bw = bw.max(0.0);
    if bw <= 0.0 || pos == StrokePosition::Inside {
        let r_in = (r - bw).max(0.0);
        return (
            (x0, y0, x1, y1, r),
            (x0 + bw, y0 + bw, x1 - bw, y1 - bw, r_in),
            bw,
        );
    }
    match pos {
        StrokePosition::Inside => unreachable!("handled above"),
        StrokePosition::Center => {
            let h = bw / 2.0;
            (
                (x0 - h, y0 - h, x1 + h, y1 + h, r + h),
                (x0 + h, y0 + h, x1 - h, y1 - h, (r - h).max(0.0)),
                h,
            )
        }
        StrokePosition::Outside => (
            (x0 - bw, y0 - bw, x1 + bw, y1 + bw, r + bw),
            (x0, y0, x1, y1, r),
            0.0,
        ),
    }
}

/// Mask factor at one canvas pixel center (Sprint 8 look, T004): the
/// product of every mask shape's rounded-rect coverage. An empty
/// shape list keeps 1.0; each shape applies exactly once.
pub fn mask_factor(shapes: &[MaskShape], i: i64, j: i64) -> f64 {
    let mut m = 1.0;
    for s in shapes {
        m *= rr_coverage(i, j, s.x, s.y, s.x + s.width, s.y + s.height, s.radius);
        if m <= 0.0 {
            return 0.0;
        }
    }
    m
}

/// Whether a box clips its body at its edge (Sprint 8 look, T004):
/// `hidden` and `clip` both cut fill and stroke at the box.
pub fn overflow_clips(overflow: Overflow) -> bool {
    matches!(overflow, Overflow::Hidden | Overflow::Clip)
}

/// Sample box-local fill coordinates (Sprint 8 look, T004): the pixel
/// center relative to the scaled box top-left.
pub fn fill_local(i: i64, j: i64, x0: f64, y0: f64) -> (f64, f64) {
    (i as f64 + 0.5 - x0, j as f64 + 0.5 - y0)
}

/// Sample the flattened layers beneath through backdrop blur and
/// brightness (Sprint 8 look, T004): blurs the canvas region in place
/// with `blur_sigma`, then scales it by `brightness`. A flat region
/// blurs to itself; brightness 1 keeps it. `(x0, y0, x1, y1)` names
/// the region in canvas pixels, clamped to the canvas.
pub fn apply_backdrop(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    bounds: (f64, f64, f64, f64),
    blur_sigma: f64,
    brightness: f64,
) {
    let i0 = (bounds.0.floor() as i64).max(0);
    let j0 = (bounds.1.floor() as i64).max(0);
    let i1 = (bounds.2.ceil() as i64).min(cw as i64);
    let j1 = (bounds.3.ceil() as i64).min(chh as i64);
    if i1 <= i0 || j1 <= j0 {
        return;
    }
    let w = (i1 - i0) as usize;
    let h = (j1 - j0) as usize;
    let mut region = vec![0u8; w * h * 3];
    for j in 0..h {
        for i in 0..w {
            let o = (((j0 + j as i64) as u32 * cw + (i0 + i as i64) as u32) as usize) * 3;
            let d = (j * w + i) * 3;
            region[d] = frame[o];
            region[d + 1] = frame[o + 1];
            region[d + 2] = frame[o + 2];
        }
    }
    if blur_on(blur_sigma) {
        // The region is opaque canvas: premultiply with alpha 1,
        // blur, and read the straight channels back.
        let mut px = vec![0.0f32; w * h * 4];
        for t in 0..w * h {
            let (s, d) = (t * 4, t * 3);
            px[s] = region[d] as f32;
            px[s + 1] = region[d + 1] as f32;
            px[s + 2] = region[d + 2] as f32;
            px[s + 3] = 1.0;
        }
        let mut pm = Premult { w, h, px };
        pm.blur(blur_sigma);
        for t in 0..w * h {
            let (s, d) = (t * 4, t * 3);
            region[d] = pm.px[s].round().clamp(0.0, 255.0) as u8;
            region[d + 1] = pm.px[s + 1].round().clamp(0.0, 255.0) as u8;
            region[d + 2] = pm.px[s + 2].round().clamp(0.0, 255.0) as u8;
        }
    }
    if brightness != 1.0 {
        let f = brightness.max(0.0);
        for v in region.iter_mut() {
            *v = (*v as f64 * f + 0.5).clamp(0.0, 255.0) as u8;
        }
    }
    for j in 0..h {
        for i in 0..w {
            let o = (((j0 + j as i64) as u32 * cw + (i0 + i as i64) as u32) as usize) * 3;
            let d = (j * w + i) * 3;
            frame[o] = region[d];
            frame[o + 1] = region[d + 1];
            frame[o + 2] = region[d + 2];
        }
    }
}

/// Paint one box shadow into a transparent straight-RGBA layer
/// (Sprint 8 look, T004): the rounded rect offset by the shadow and
/// grown by its spread, in the shadow color under the element
/// opacity. The caller premultiplies, blurs by the shadow sigma, and
/// composites below the element. Shadow extents stay in canvas
/// pixels: motion scale grows the box, never the shadow.
pub fn paint_shadow_layer(
    layer: &mut [u8],
    cw: u32,
    chh: u32,
    b: &ResolvedBox,
) -> Option<(f64, f64, f64, f64)> {
    let sh = b.box_shadow?;
    if b.opacity <= 0.0 || b.width <= 0.0 || b.height <= 0.0 {
        return None;
    }
    if b.scale <= 0.0 || !b.scale.is_finite() {
        return None;
    }
    let s = b.scale;
    let (sw, shw) = (b.width * s, b.height * s);
    let spread = sh.spread.max(0.0);
    let (x0, y0) = (b.x + sh.offset_x - spread, b.y + sh.offset_y - spread);
    let (x1, y1) = (b.x + sh.offset_x + sw + spread, b.y + sh.offset_y + shw + spread);
    let r = (b.radius * s).max(0.0) + spread;
    let alpha = b.opacity.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return None;
    }
    let i0 = (x0.floor() as i64).max(0);
    let i1 = (x1.ceil() as i64).min(cw as i64);
    let j0 = (y0.floor() as i64).max(0);
    let j1 = (y1.ceil() as i64).min(chh as i64);
    for j in j0..j1 {
        for i in i0..i1 {
            let cov = rr_coverage(i, j, x0, y0, x1, y1, r);
            if cov <= 0.0 {
                continue;
            }
            let o = (j as u32 * cw + i as u32) as usize * 4;
            if o + 3 >= layer.len() {
                continue;
            }
            layer_blend(
                layer,
                o,
                sh.color.r,
                sh.color.g,
                sh.color.b,
                cov * alpha,
            );
        }
    }
    Some((x0, y0, x1, y1))
}

/// Body card of one box in canvas pixels (Sprint 8 look, T004): the
/// scaled box plus the stroke outset for its position.
pub fn body_card(b: &ResolvedBox) -> (f64, f64, f64, f64) {
    let s = b.scale;
    let (sw, shw) = (b.width * s, b.height * s);
    let bw = (b.border_width * s).max(0.0);
    let outset = match b.stroke_position {
        StrokePosition::Inside => 0.0,
        StrokePosition::Center => bw / 2.0,
        StrokePosition::Outside => bw,
    };
    (
        b.x - outset,
        b.y - outset,
        b.x + sw + outset,
        b.y + shw + outset,
    )
}

/// Composite one premultiplied layer through a blend mode (Sprint 8
/// look, T004): like `composite_premult_direct`, but every texel
/// blends with the canvas under `mode`, and every mask shape scales
/// the texel alpha exactly once. Only the grown element box is
/// visited.
#[allow(clippy::too_many_arguments)]
pub fn composite_direct_blend(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    pm: &Premult,
    bbox: (f64, f64, f64, f64),
    mode: BlendMode,
    masks: &[MaskShape],
) {
    let i0 = (bbox.0.floor() as i64).max(0);
    let i1 = (bbox.2.ceil() as i64).min(cw as i64);
    let j0 = (bbox.1.floor() as i64).max(0);
    let j1 = (bbox.3.ceil() as i64).min(chh as i64);
    for j in j0..j1 {
        for i in i0..i1 {
            let s = ((j as usize) * pm.w + i as usize) * 4;
            if s + 3 >= pm.px.len() {
                continue;
            }
            let mut a = pm.px[s + 3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            if !masks.is_empty() {
                a *= mask_factor(masks, i, j) as f32;
                if a <= 0.0 {
                    continue;
                }
            }
            let a64 = f64::from(a);
            let straight = [
                (f64::from(pm.px[s]) / a64).round().clamp(0.0, 255.0) as u8,
                (f64::from(pm.px[s + 1]) / a64).round().clamp(0.0, 255.0) as u8,
                (f64::from(pm.px[s + 2]) / a64).round().clamp(0.0, 255.0) as u8,
            ];
            let o = (j as u32 * cw + i as u32) as usize * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            let dst = [frame[o], frame[o + 1], frame[o + 2]];
            let out = blend_texel(dst, straight, a64, mode);
            frame[o] = out[0];
            frame[o + 1] = out[1];
            frame[o + 2] = out[2];
        }
    }
}

/// Composite one premultiplied layer through 2D rotation and an
/// optional 3D turn with a blend mode (Sprint 8 look, T004): like
/// `composite_premult_warp`, but every texel blends under `mode` and
/// every mask shape scales the texel alpha exactly once. The mask
/// shapes live in screen space, so the factor is read at the visited
/// screen pixel.
#[allow(clippy::too_many_arguments)]
pub fn composite_warp_blend(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    pm: &Premult,
    anchor: (f64, f64),
    rotation_deg: f64,
    turn: Option<Turn>,
    bounds: (f64, f64, f64, f64),
    mode: BlendMode,
    masks: &[MaskShape],
) {
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let rotated = rotation_deg != 0.0 && rotation_deg.is_finite();
    let i0 = (bounds.0.floor() as i64).max(0);
    let i1 = (bounds.2.ceil() as i64).min(cw as i64);
    let j0 = (bounds.1.floor() as i64).max(0);
    let j1 = (bounds.3.ceil() as i64).min(chh as i64);
    for j in j0..=j1 {
        if j < 0 || j >= chh as i64 {
            continue;
        }
        for i in i0..=i1 {
            if i < 0 || i >= cw as i64 {
                continue;
            }
            let qx = i as f64 + 0.5;
            let qy = j as f64 + 0.5;
            let (mut rx, mut ry) = if rotated {
                let dx = qx - anchor.0;
                let dy = qy - anchor.1;
                (
                    anchor.0 + dx * cos + dy * sin,
                    anchor.1 - dx * sin + dy * cos,
                )
            } else {
                (qx, qy)
            };
            if let Some(t) = turn {
                let Some((ux, uy)) = t.inverse(rx, ry) else {
                    continue;
                };
                rx = ux;
                ry = uy;
            }
            let rgba = pm.sample(rx - 0.5, ry - 0.5);
            let mut a = rgba[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            if !masks.is_empty() {
                a *= mask_factor(masks, i, j);
                if a <= 0.0 {
                    continue;
                }
            }
            let straight = [
                (rgba[0] / a).round().clamp(0.0, 255.0) as u8,
                (rgba[1] / a).round().clamp(0.0, 255.0) as u8,
                (rgba[2] / a).round().clamp(0.0, 255.0) as u8,
            ];
            let o = (j as u32 * cw + i as u32) as usize * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            let dst = [frame[o], frame[o + 1], frame[o + 2]];
            let out = blend_texel(dst, straight, a, mode);
            frame[o] = out[0];
            frame[o + 1] = out[1];
            frame[o + 2] = out[2];
        }
    }
}

/// Draw one box through the paint stack (Sprint 8 look, T004): the
/// backdrop samples the flattened layers beneath before the element
/// paints, the shadow composites below the element, the body
/// (fill plus the positioned stroke) composites through the blend
/// mode, and the mask shapes scale the alpha exactly once over
/// self-first content. Only runs when [`ResolvedBox::paint_stack_active`]
/// holds; every other box keeps the historical paths.
pub(crate) fn draw_box_paint(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    b: &ResolvedBox,
    images: &[crate::render::DecodedImage],
    folder: &Path,
) -> Result<(), String> {
    if b.width <= 0.0 || b.height <= 0.0 {
        return Ok(());
    }
    if b.scale <= 0.0 || !b.scale.is_finite() {
        return Ok(());
    }
    // A fully transparent element still samples the backdrop (the
    // backdrop filter reads the flattened layers beneath before the
    // element paints, even when the element itself adds nothing).
    let transparent = b.opacity <= 0.0;
    if transparent
        && !blur_on(b.backdrop_blur)
        && b.backdrop_brightness == 1.0
    {
        return Ok(());
    }
    // The image fill resolves once per box, with the same vanish
    // check as the direct path: a source deleted mid-render refuses
    // here with its path named and no partial MP4.
    let decoded = match &b.fill_paint {
        notation::model::FillPaint::Image { src, .. } => {
            Some(fill_image_decoded(images, folder, src)?)
        }
        _ => None,
    };
    let card = body_card(b);
    let shadow_rect = b.box_shadow.map(|sh| {
        let spread = sh.spread.max(0.0);
        (
            b.x + sh.offset_x - spread,
            b.y + sh.offset_y - spread,
            b.x + b.width * b.scale + sh.offset_x + spread,
            b.y + b.height * b.scale + sh.offset_y + spread,
        )
    });
    let anchor = ((card.0 + card.2) / 2.0, (card.1 + card.3) / 2.0);
    let turn = item_turn(b.rotate_x, b.rotate_y, &b.turn, card);
    let rotated = b.rotation != 0.0 && b.rotation.is_finite();
    // Every mask shape on this element, each applying exactly once.
    let mut masks: Vec<MaskShape> = Vec::new();
    if let Some(m) = b.mask {
        masks.push(m);
    }
    if let Some(m) = b.clip_path {
        masks.push(m);
    }
    // The backdrop samples the flattened layers beneath before the
    // element paints: the union of the body and shadow rects, grown
    // by the backdrop blur margin.
    if blur_on(b.backdrop_blur) || b.backdrop_brightness != 1.0 {
        let mut region = card;
        if let Some(r) = shadow_rect {
            region = (
                region.0.min(r.0),
                region.1.min(r.1),
                region.2.max(r.2),
                region.3.max(r.3),
            );
        }
        let margin = blur_margin(b.backdrop_blur) + 1.0;
        region = (
            region.0 - margin,
            region.1 - margin,
            region.2 + margin,
            region.3 + margin,
        );
        apply_backdrop(
            frame,
            cw,
            chh,
            region,
            b.backdrop_blur,
            b.backdrop_brightness,
        );
    }
    // The shadow paints below the element: its own layer, blurred by
    // its own sigma, composited source-over (masks scale it once,
    // like the body). A transparent element casts no shadow.
    if b.box_shadow.is_some() && !transparent {
        let mut layer = vec![0u8; cw as usize * chh as usize * 4];
        let painted = paint_shadow_layer(&mut layer, cw, chh, b);
        if painted.is_some() {
            let mut pm = Premult::from_layer(&layer, cw as usize, chh as usize);
            let sigma = b.box_shadow.map(|s| s.blur).unwrap_or(0.0);
            if blur_on(sigma) {
                pm.blur(sigma);
            }
            let margin = blur_margin(sigma) + 1.0;
            let grown = shadow_rect.map(|r| (r.0 - margin, r.1 - margin, r.2 + margin, r.3 + margin));
            // The shadow warps about the body anchor, so a rotated or
            // turned element carries its shadow along.
            let union = match shadow_rect {
                Some(r) => (card.0.min(r.0), card.1.min(r.1), card.2.max(r.2), card.3.max(r.3)),
                None => card,
            };
            match (turn, rotated) {
                (None, false) => composite_direct_blend(
                    frame,
                    cw,
                    chh,
                    &pm,
                    grown.unwrap_or(card),
                    BlendMode::Normal,
                    &masks,
                ),
                _ => composite_warp_blend(
                    frame,
                    cw,
                    chh,
                    &pm,
                    anchor,
                    b.rotation,
                    turn,
                    warp_bounds(union, anchor, b.rotation, turn),
                    BlendMode::Normal,
                    &masks,
                ),
            }
        }
    }
    // The body paints above the shadow through the element blend:
    // self-first content (fill plus the positioned stroke under the
    // element opacity), blurred by the element sigma, then blended
    // with the layers beneath and masked exactly once.
    if !transparent {
        let mut layer = vec![0u8; cw as usize * chh as usize * 4];
        draw_box_layer(&mut layer, cw, chh, b, decoded);
        let mut pm = Premult::from_layer(&layer, cw as usize, chh as usize);
        if blur_on(b.blur) {
            pm.blur(b.blur);
        }
        match (turn, rotated) {
            (None, false) => {
                let margin = blur_margin(b.blur) + 1.0;
                let grown = (card.0 - margin, card.1 - margin, card.2 + margin, card.3 + margin);
                composite_direct_blend(frame, cw, chh, &pm, grown, b.blend_mode, &masks);
            }
            _ => composite_warp_blend(
                frame,
                cw,
                chh,
                &pm,
                anchor,
                b.rotation,
                turn,
                warp_bounds(card, anchor, b.rotation, turn),
                b.blend_mode,
                &masks,
            ),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> notation::model::Rgb {
        notation::model::Rgb { r, g, b }
    }

    #[test]
    fn normal_blend_is_source_over() {
        assert_eq!(
            blend_texel([10, 20, 30], [200, 210, 220], 1.0, BlendMode::Normal),
            [200, 210, 220]
        );
        assert_eq!(
            blend_texel([10, 20, 30], [200, 210, 220], 0.0, BlendMode::Multiply),
            [10, 20, 30]
        );
        // Half alpha over black: the source halves.
        assert_eq!(
            blend_texel([0, 0, 0], [200, 100, 50], 0.5, BlendMode::Normal),
            [100, 50, 25]
        );
    }

    #[test]
    fn multiply_screen_overlay_match_hand_math() {
        // Multiply: 200*100/255 = 78.43 -> 78 at full alpha.
        assert_eq!(
            blend_texel([100, 100, 100], [200, 200, 200], 1.0, BlendMode::Multiply),
            [78, 78, 78]
        );
        // Screen: 255-(55*155)/255 = 255-33.43 -> 222 (221.57 rounds to 222).
        assert_eq!(
            blend_texel([100, 100, 100], [200, 200, 200], 1.0, BlendMode::Screen),
            [222, 222, 222]
        );
        // Overlay with dark dst (<128): 2*200*100/255 = 156.86 -> 157.
        assert_eq!(
            blend_texel([100, 0, 0], [200, 0, 0], 1.0, BlendMode::Overlay),
            [157, 0, 0]
        );
        // Overlay with light dst (>=128): 255-2*55*155/255 = 188.14 -> 188.
        assert_eq!(
            blend_texel([200, 0, 0], [100, 0, 0], 1.0, BlendMode::Overlay),
            [188, 0, 0]
        );
        // Multiply under half alpha: 78*0.5 + 100*0.5 = 89.
        assert_eq!(
            blend_texel([100, 100, 100], [200, 200, 200], 0.5, BlendMode::Multiply),
            [89, 89, 89]
        );
    }

    #[test]
    fn stroke_geometry_matches_inside_history() {
        // Inside reproduces the historical border: outer is the box,
        // fill insets by the full width.
        let (outer, fill, inset) = stroke_geometry(10.0, 20.0, 110.0, 60.0, 5.0, 4.0, StrokePosition::Inside);
        assert_eq!(outer, (10.0, 20.0, 110.0, 60.0, 5.0));
        assert_eq!(fill, (14.0, 24.0, 106.0, 56.0, 1.0));
        assert_eq!(inset, 4.0);
        // Center straddles: half out, half in.
        let (outer, fill, inset) = stroke_geometry(10.0, 20.0, 110.0, 60.0, 5.0, 4.0, StrokePosition::Center);
        assert_eq!(outer, (8.0, 18.0, 112.0, 62.0, 7.0));
        assert_eq!(fill, (12.0, 22.0, 108.0, 58.0, 3.0));
        assert_eq!(inset, 2.0);
        // Outside rings the edge: fill is the box.
        let (outer, fill, inset) = stroke_geometry(10.0, 20.0, 110.0, 60.0, 5.0, 4.0, StrokePosition::Outside);
        assert_eq!(outer, (6.0, 16.0, 114.0, 64.0, 9.0));
        assert_eq!(fill, (10.0, 20.0, 110.0, 60.0, 5.0));
        assert_eq!(inset, 0.0);
        // Zero width paints the box in every position.
        for pos in [StrokePosition::Inside, StrokePosition::Center, StrokePosition::Outside] {
            let (outer, fill, inset) = stroke_geometry(10.0, 20.0, 110.0, 60.0, 5.0, 0.0, pos);
            assert_eq!(outer, (10.0, 20.0, 110.0, 60.0, 5.0));
            assert_eq!(fill, (10.0, 20.0, 110.0, 60.0, 5.0));
            assert_eq!(inset, 0.0);
        }
    }

    #[test]
    fn backdrop_brightness_zero_blacks_the_region() {
        let (cw, chh) = (8, 8);
        let mut frame = vec![100u8; cw * chh * 3];
        apply_backdrop(&mut frame, cw as u32, chh as u32, (2.0, 2.0, 6.0, 6.0), 0.0, 0.0);
        // Inside the region: black.
        assert_eq!(frame[(3 * cw + 3) * 3], 0);
        assert_eq!(frame[(4 * cw + 4) * 3 + 1], 0);
        // Outside the region: untouched.
        assert_eq!(frame[0], 100);
        assert_eq!(frame[(7 * cw + 7) * 3], 100);
    }

    #[test]
    fn backdrop_flat_region_blur_is_identity() {
        let (cw, chh) = (16, 16);
        let mut frame = vec![77u8; cw * chh * 3];
        apply_backdrop(&mut frame, cw as u32, chh as u32, (0.0, 0.0, 16.0, 16.0), 2.0, 1.0);
        for v in frame {
            assert!((v as i16 - 77).abs() <= 1, "flat blur drifts at most 1 LSB, got {v}");
        }
    }

    #[test]
    fn mask_factor_covers_once() {
        let shape = MaskShape { x: 0.0, y: 0.0, width: 16.0, height: 16.0, radius: 0.0 };
        // Deep inside: full coverage.
        assert_eq!(mask_factor(&[shape], 8, 8), 1.0);
        // Far outside: zero.
        assert_eq!(mask_factor(&[shape], 20, 20), 0.0);
        // No shapes: untouched.
        assert_eq!(mask_factor(&[], 8, 8), 1.0);
        // Two identical shapes multiply once each (idempotent at 0/1).
        assert_eq!(mask_factor(&[shape, shape], 8, 8), 1.0);
        assert_eq!(mask_factor(&[shape, shape], 20, 20), 0.0);
    }

    fn paint_box(
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: notation::model::Rgb,
        stroke_w: f64,
        stroke: notation::model::Rgb,
    ) -> ResolvedBox {
        ResolvedBox {
            doc: 0,
            order: 0,
            x,
            y,
            width: w,
            height: h,
            fill,
            fill_paint: notation::model::FillPaint::Solid,
            radius: 0.0,
            border_width: stroke_w,
            border_color: stroke,
            stroke_position: StrokePosition::Inside,
            box_shadow: None,
            blend_mode: BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
        }
    }

    fn pixel(frame: &[u8], cw: usize, i: usize, j: usize) -> [u8; 3] {
        let o = (j * cw + i) * 3;
        [frame[o], frame[o + 1], frame[o + 2]]
    }

    fn paint_frame(b: &ResolvedBox, cw: u32, chh: u32, bg: [u8; 3]) -> Vec<u8> {
        let mut frame = vec![0u8; cw as usize * chh as usize * 3];
        for px in frame.chunks_exact_mut(3) {
            px.copy_from_slice(&bg);
        }
        draw_box_paint(
            &mut frame,
            cw,
            chh,
            b,
            &[],
            std::path::Path::new("."),
        )
        .unwrap();
        frame
    }

    #[test]
    fn stroke_positions_paint_above_fill() {
        let cw = 48;
        // Red fill with a blue 8px stroke at (10,10) 20x20.
        let red = rgb(255, 0, 0);
        let blue = rgb(0, 0, 255);
        // Inside: the ring hugs the inner edge; the center stays fill.
        let mut b = paint_box(10.0, 10.0, 20.0, 20.0, red, 8.0, blue);
        b.stroke_position = StrokePosition::Inside;
        let f = paint_frame(&b, cw, 40, [0, 0, 0]);
        assert_eq!(pixel(&f, cw as usize, 11, 20), [0, 0, 255]);
        assert_eq!(pixel(&f, cw as usize, 20, 20), [255, 0, 0]);
        assert_eq!(pixel(&f, cw as usize, 9, 20), [0, 0, 0]);
        // Outside: the ring leaves the box; the edge pixel stays fill.
        let mut b = paint_box(10.0, 10.0, 20.0, 20.0, red, 8.0, blue);
        b.stroke_position = StrokePosition::Outside;
        let f = paint_frame(&b, cw, 40, [0, 0, 0]);
        assert_eq!(pixel(&f, cw as usize, 9, 20), [0, 0, 255]);
        assert_eq!(pixel(&f, cw as usize, 11, 20), [255, 0, 0]);
        assert_eq!(pixel(&f, cw as usize, 20, 20), [255, 0, 0]);
        // Center: the ring straddles the edge on both sides.
        let mut b = paint_box(10.0, 10.0, 20.0, 20.0, red, 8.0, blue);
        b.stroke_position = StrokePosition::Center;
        let f = paint_frame(&b, cw, 40, [0, 0, 0]);
        assert_eq!(pixel(&f, cw as usize, 9, 20), [0, 0, 255]);
        assert_eq!(pixel(&f, cw as usize, 11, 20), [0, 0, 255]);
        assert_eq!(pixel(&f, cw as usize, 20, 20), [255, 0, 0]);
    }

    #[test]
    fn shadow_paints_below_body_with_transparent_center() {
        use notation::model::BoxShadow;
        let mut b = paint_box(10.0, 10.0, 20.0, 20.0, rgb(255, 0, 0), 0.0, rgb(0, 0, 0));
        b.box_shadow = Some(BoxShadow {
            offset_x: 6.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: rgb(0, 0, 255),
        });
        let f = paint_frame(&b, 48, 40, [0, 0, 0]);
        // Deep inside the box the body covers the shadow: fill, not blue.
        assert_eq!(pixel(&f, 48, 12, 20), [255, 0, 0]);
        // Past the box edge the shadow shows through the background.
        assert_eq!(pixel(&f, 48, 32, 20), [0, 0, 255]);
        // Far away: untouched background.
        assert_eq!(pixel(&f, 48, 45, 20), [0, 0, 0]);
    }

    #[test]
    fn every_blend_mode_composites_with_beneath() {
        // Mid-gray beneath, (200,100,50) above at full alpha.
        let cases = [
            (BlendMode::Normal, [200, 100, 50]),
            (BlendMode::Multiply, [100, 50, 25]),
            (BlendMode::Screen, [228, 178, 153]),
            (BlendMode::Overlay, [200, 101, 51]),
        ];
        for (mode, want) in cases {
            let mut b = paint_box(4.0, 4.0, 8.0, 8.0, rgb(200, 100, 50), 0.0, rgb(0, 0, 0));
            b.blend_mode = mode;
            let f = paint_frame(&b, 16, 16, [128, 128, 128]);
            assert_eq!(pixel(&f, 16, 8, 8), want, "mode {:?}", mode);
        }
    }

    #[test]
    fn backdrop_brightness_samples_beneath_before_paint() {
        // A transparent element still samples the backdrop: white
        // beneath halves to gray, with no body painted over it.
        let mut b = paint_box(2.0, 2.0, 4.0, 4.0, rgb(255, 0, 0), 0.0, rgb(0, 0, 0));
        b.opacity = 0.0;
        b.backdrop_brightness = 0.5;
        let f = paint_frame(&b, 16, 16, [255, 255, 255]);
        assert_eq!(pixel(&f, 16, 4, 4), [128, 128, 128]);
        assert_eq!(pixel(&f, 16, 0, 0), [255, 255, 255]);
    }

    #[test]
    fn backdrop_blur_softens_the_sampled_edge() {
        // A sharp black/white edge beneath, sampled through blur 2:
        // the texel next to the edge mixes both sides.
        let (cw, chh) = (16, 4);
        let mut frame = vec![0u8; cw * chh * 3];
        for j in 0..chh {
            for i in 0..cw {
                let o = (j * cw + i) * 3;
                let v = if i < 8 { 0u8 } else { 255u8 };
                frame[o] = v;
                frame[o + 1] = v;
                frame[o + 2] = v;
            }
        }
        apply_backdrop(&mut frame, cw as u32, chh as u32, (0.0, 0.0, 16.0, 4.0), 2.0, 1.0);
        // Deep on each side the flats survive; the edge texels mix.
        assert_eq!(pixel(&frame, cw, 0, 2)[0], 0);
        assert_eq!(pixel(&frame, cw, 15, 2)[0], 255);
        let edge = pixel(&frame, cw, 7, 2)[0];
        assert!(edge > 0 && edge < 255, "edge softens, got {edge}");
        let edge = pixel(&frame, cw, 8, 2)[0];
        assert!(edge > 0 && edge < 255, "edge softens, got {edge}");
    }

    #[test]
    fn overflow_hidden_clips_the_outside_stroke() {
        let mut visible = paint_box(10.0, 10.0, 20.0, 20.0, rgb(255, 0, 0), 8.0, rgb(0, 0, 255));
        visible.stroke_position = StrokePosition::Outside;
        visible.overflow = Overflow::Visible;
        let f = paint_frame(&visible, 48, 40, [0, 0, 0]);
        assert_eq!(pixel(&f, 48, 9, 20), [0, 0, 255]);
        let mut hidden = paint_box(10.0, 10.0, 20.0, 20.0, rgb(255, 0, 0), 8.0, rgb(0, 0, 255));
        hidden.stroke_position = StrokePosition::Outside;
        hidden.overflow = Overflow::Hidden;
        let f = paint_frame(&hidden, 48, 40, [0, 0, 0]);
        assert_eq!(pixel(&f, 48, 9, 20), [0, 0, 0]);
        assert_eq!(pixel(&f, 48, 11, 20), [255, 0, 0]);
    }

    #[test]
    fn mask_applies_once_over_self_first_content() {
        // Half-opaque red body masked to the left half: the left blends
        // with the background, the right shows the background exactly.
        let mut b = paint_box(4.0, 4.0, 8.0, 8.0, rgb(255, 0, 0), 0.0, rgb(0, 0, 0));
        b.opacity = 0.5;
        b.mask = Some(MaskShape { x: 0.0, y: 0.0, width: 8.0, height: 16.0, radius: 0.0 });
        let f = paint_frame(&b, 16, 16, [0, 0, 255]);
        // Left (masked in): half-opaque red over blue. The byte
        // alpha (128/255) lands red at 128.5 -> 128 and blue at
        // 127.5 -> 127 under half-up truncation.
        assert_eq!(pixel(&f, 16, 6, 8), [128, 0, 127]);
        // Right (masked out): background untouched.
        assert_eq!(pixel(&f, 16, 10, 8), [0, 0, 255]);
    }

    #[test]
    fn shadow_layer_paints_offset_shape() {
        use notation::model::{BoxShadow, FillPaint};
        let b = ResolvedBox {
            doc: 0,
            order: 0,
            x: 4.0,
            y: 4.0,
            width: 8.0,
            height: 8.0,
            fill: rgb(255, 0, 0),
            fill_paint: FillPaint::Solid,
            radius: 0.0,
            border_width: 0.0,
            border_color: rgb(0, 0, 0),
            stroke_position: StrokePosition::Inside,
            box_shadow: Some(BoxShadow {
                offset_x: 4.0,
                offset_y: 0.0,
                blur: 0.0,
                spread: 0.0,
                color: rgb(0, 0, 255),
            }),
            blend_mode: BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
        };
        let (cw, chh) = (24, 16);
        let mut layer = vec![0u8; cw * chh * 4];
        let rect = paint_shadow_layer(&mut layer, cw as u32, chh as u32, &b).unwrap();
        assert_eq!(rect, (8.0, 4.0, 16.0, 12.0));
        // Shadow texel inside: blue, opaque.
        let o = (6 * cw + 10) * 4;
        assert_eq!(&layer[o..o + 3], &[0, 0, 255]);
        assert_eq!(layer[o + 3], 255);
        // Body origin outside the shadow: transparent.
        let o = (6 * cw + 5) * 4;
        assert_eq!(layer[o + 3], 0);
    }
}
