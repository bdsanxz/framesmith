//! Vector path shapes and SVG pictures (Sprint 8 look, CAP-005/T005).
//!
//! A `path` clip carries SVG-subset `d` data with a fill rule and the
//! shared stroke set, routed through the shared paint stack. Parsing
//! happens once per distinct `d` string in a shared [`PathCache`]:
//! every frame reuses the cached contours and recomputes only numeric
//! parameters (scale, offset, fill, stroke, opacity, motion), so
//! drawing scales linearly with the command count and identical `d`
//! strings never re-parse. An `svg` clip (and an `image` clip
//! pointing at an `.svg` file) rasterizes through `resvg` at its
//! declared size, then draws through the shared image blitter.

use crate::blur::Premult;
use crate::frame::ResolvedPath;
use crate::paint::{composite_direct_blend, composite_warp_blend};
use crate::render::{blur_margin, blur_on, item_turn, layer_blend, warp_bounds, DecodedImage};
use notation::model::{
    BlendMode, Fit, MaskShape, Overflow, PlaceH, PlaceV, Placement, ResolvedGrade, StrokePosition,
};
use resvg::tiny_skia as sk;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// One parsed path shape: the flattened contours in raw `d`
/// coordinates plus the raw fill bounds. Rebuilding a draw path from
/// these contours is numbers-only work — no parsing — so every frame
/// reuses the cached structure.
#[derive(Debug)]
pub struct CachedPath {
    /// One polyline per pen-down run, in raw `d` coordinates.
    pub subpaths: Vec<motion::path::Subpath>,
    /// Raw fill bounds (min/max over every contour point).
    pub bbox: (f64, f64, f64, f64),
}

/// Parser cache for path data (Sprint 8 look, T005): one entry per
/// distinct `d` string, shared across frames and workers. A hit
/// returns the cached contours; a miss parses once and counts it, so
/// animated or repeated draws of the same `d` never re-parse.
pub struct PathCache {
    inner: Mutex<HashMap<String, Arc<CachedPath>>>,
    parses: AtomicUsize,
}

impl PathCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            parses: AtomicUsize::new(0),
        }
    }

    /// Resolve `d` to its cached shape: a hit returns the stored
    /// contours, a miss parses, flattens, and stores them. Unparsable
    /// data (refused by validation before frame one) errors here.
    pub fn resolve(&self, d: &str) -> Result<Arc<CachedPath>, motion::path::IndexedError> {
        if let Some(hit) = self.inner.lock().ok().and_then(|m| m.get(d).cloned()) {
            return Ok(hit);
        }
        let parsed = motion::path::parse_indexed(d)?;
        let subpaths = motion::path::subpaths(&parsed)
            .into_iter()
            .filter(|s| s.points.len() > 1)
            .collect::<Vec<_>>();
        let mut bbox = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for s in &subpaths {
            for p in &s.points {
                bbox.0 = bbox.0.min(p[0]);
                bbox.1 = bbox.1.min(p[1]);
                bbox.2 = bbox.2.max(p[0]);
                bbox.3 = bbox.3.max(p[1]);
            }
        }
        if !bbox.0.is_finite() {
            bbox = (0.0, 0.0, 0.0, 0.0);
        }
        let cached = Arc::new(CachedPath { subpaths, bbox });
        if let Ok(mut m) = self.inner.lock() {
            m.insert(d.to_string(), cached.clone());
        }
        self.parses.fetch_add(1, Ordering::Relaxed);
        Ok(cached)
    }

    /// How many distinct `d` strings parsed so far (cache misses).
    #[cfg(test)]
    pub fn parses(&self) -> usize {
        self.parses.load(Ordering::Relaxed)
    }

    /// Drop every entry (tests only: each test starts unpolluted).
    #[cfg(test)]
    pub fn clear(&self) {
        if let Ok(mut m) = self.inner.lock() {
            m.clear();
        }
        self.parses.store(0, Ordering::Relaxed);
    }
}

impl Default for PathCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Fill rule of a resolved path as the raster rule.
fn raster_rule(p: &ResolvedPath) -> sk::FillRule {
    match p.fill_rule {
        notation::model::FillRule::NonZero => sk::FillRule::Winding,
        notation::model::FillRule::EvenOdd => sk::FillRule::EvenOdd,
    }
}

/// Transformed contour points for one draw: raw `d` coordinates scaled
/// about the raw bbox top-left by `scale`, then translated by the
/// element offset. Rotation rides the warp path (like boxes), never
/// the points, so the unrotated layer stays sharp.
fn place_points(
    cached: &CachedPath,
    scale: f64,
    x: f64,
    y: f64,
) -> Vec<Vec<[f64; 2]>> {
    let (bx0, by0, _, _) = cached.bbox;
    cached
        .subpaths
        .iter()
        .map(|s| {
            s.points
                .iter()
                .map(|p| {
                    [
                        x + bx0 * scale + (p[0] - bx0) * scale,
                        y + by0 * scale + (p[1] - by0) * scale,
                    ]
                })
                .collect()
        })
        .collect()
}

/// Body card of one path in canvas pixels: the scaled fill bounds plus
/// the stroke outset for its position (inside paints no outset).
fn path_card(p: &ResolvedPath, cached: &CachedPath) -> (f64, f64, f64, f64) {
    let s = p.scale.max(0.0);
    let (bx0, by0, bx1, by1) = cached.bbox;
    let (x0, y0) = (p.x + bx0 * s, p.y + by0 * s);
    let (x1, y1) = (p.x + bx1 * s, p.y + by1 * s);
    let bw = (p.border_width * s).max(0.0);
    let outset = match p.stroke_position {
        StrokePosition::Inside => 0.0,
        StrokePosition::Center => bw / 2.0,
        StrokePosition::Outside => bw,
    };
    (x0 - outset, y0 - outset, x1 + outset, y1 + outset)
}

/// Build the tiny-skia contour for placed points: one subpath per
/// contour, sealed when the contour closed. Degenerate contours
/// (fewer than two distinct points) contribute nothing.
fn build_tiny(contours: &[Vec<[f64; 2]>], closed: &[bool]) -> Option<sk::Path> {
    let mut b = sk::PathBuilder::new();
    let mut any = false;
    for (pts, shut) in contours.iter().zip(closed.iter()) {
        if pts.len() < 2 {
            continue;
        }
        // A contour of only repeated points draws nothing.
        if pts.iter().all(|p| *p == pts[0]) {
            continue;
        }
        b.move_to(pts[0][0] as f32, pts[0][1] as f32);
        for p in &pts[1..] {
            b.line_to(p[0] as f32, p[1] as f32);
        }
        if *shut {
            b.close();
        }
        any = true;
    }
    if !any {
        return None;
    }
    b.finish()
}

/// Paint the path body (fill plus the positioned stroke) into a
/// straight-RGBA layer (Sprint 8 look, T005): the fill covers by the
/// fill rule, the stroke paints above it at its position. Assumes the
/// layer is zeroed and the element visible.
pub(crate) fn draw_path_layer(
    layer: &mut [u8],
    cw: u32,
    chh: u32,
    p: &ResolvedPath,
    cached: &CachedPath,
) {
    let s = p.scale.max(0.0);
    if s <= 0.0 {
        return;
    }
    let contours = place_points(cached, s, p.x, p.y);
    let closed: Vec<bool> = cached.subpaths.iter().map(|sp| sp.closed).collect();
    let Some(tp) = build_tiny(&contours, &closed) else {
        return;
    };
    let rule = raster_rule(p);
    let bw = (p.border_width * s).max(0.0);
    // The raster pixmap covers the stroke outset exactly; coordinates
    // shift by its origin.
    let (bx0, by0, bx1, by1) = {
        let (cx0, cy0, cx1, cy1) = path_card(p, cached);
        (
            cx0.floor().max(0.0),
            cy0.floor().max(0.0),
            cx1.ceil().min(cw as f64),
            cy1.ceil().min(chh as f64),
        )
    };
    if bx1 <= bx0 || by1 <= by0 {
        return;
    }
    let (pw, ph) = ((bx1 - bx0) as u32, (by1 - by0) as u32);
    if pw == 0 || ph == 0 {
        return;
    }
    let Some(mut px) = sk::Pixmap::new(pw, ph) else {
        return;
    };
    let shift = sk::Transform::from_translate(-(bx0 as f32), -(by0 as f32));
    let alpha = (p.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut fill = sk::Paint::default();
    fill.set_color_rgba8(p.fill.r, p.fill.g, p.fill.b, alpha);
    fill.anti_alias = true;
    px.fill_path(&tp, &fill, rule, shift, None);
    if bw > 0.0 && alpha > 0 {
        // Inside and outside clip a double-width centered stroke at
        // the fill edge; center strokes the edge directly.
        let (width, mask) = match p.stroke_position {
            StrokePosition::Center => (bw, None),
            _ => {
                let Some(mut m) = sk::Mask::new(pw, ph) else {
                    return;
                };
                m.fill_path(&tp, rule, true, shift);
                if p.stroke_position == StrokePosition::Outside {
                    m.invert();
                }
                (bw * 2.0, Some(m))
            }
        };
        let mut paint = sk::Paint::default();
        paint.set_color_rgba8(
            p.border_color.r,
            p.border_color.g,
            p.border_color.b,
            alpha,
        );
        paint.anti_alias = true;
        let stroke = sk::Stroke {
            width: width as f32,
            ..Default::default()
        };
        px.stroke_path(&tp, &paint, &stroke, shift, mask.as_ref());
    }
    let data = px.data();
    let ox = bx0 as usize;
    let oy = by0 as usize;
    for j in 0..ph as usize {
        for i in 0..pw as usize {
            let a = data[(j * pw as usize + i) * 4 + 3];
            if a == 0 {
                continue;
            }
            let o = ((oy + j) * cw as usize + (ox + i)) * 4;
            if o + 3 >= layer.len() {
                continue;
            }
            let s4 = (j * pw as usize + i) * 4;
            let inv = 255.0 / f64::from(a);
            layer[o] = (f64::from(data[s4]) * inv).round().clamp(0.0, 255.0) as u8;
            layer[o + 1] = (f64::from(data[s4 + 1]) * inv).round().clamp(0.0, 255.0) as u8;
            layer[o + 2] = (f64::from(data[s4 + 2]) * inv).round().clamp(0.0, 255.0) as u8;
            layer[o + 3] = a;
        }
    }
    // Overflow `hidden`/`clip` cuts fill and stroke at the fill card;
    // `visible` keeps the whole outset. The fill card is the scaled
    // bounds before the outset, so an outside stroke bleeds past it.
    // Only the stroke-outset card is visited: an inside stroke never
    // leaves the fill card, so clipping is a no-op for it.
    if matches!(p.overflow, Overflow::Hidden | Overflow::Clip)
        && p.stroke_position != StrokePosition::Inside
    {
        let s = p.scale.max(0.0);
        let (rbx0, rby0, rbx1, rby1) = cached.bbox;
        let (fx0, fy0, fx1, fy1) = (
            p.x + rbx0 * s,
            p.y + rby0 * s,
            p.x + rbx1 * s,
            p.y + rby1 * s,
        );
        let (card0, card1, card2, card3) = path_card(p, cached);
        let (i0, i1) = (
            (card0.floor() as i64).max(0),
            (card2.ceil() as i64).min(cw as i64),
        );
        let (j0, j1) = (
            (card1.floor() as i64).max(0),
            (card3.ceil() as i64).min(chh as i64),
        );
        for j in j0..j1 {
            let cy = j as f64 + 0.5;
            let outside_y = cy < fy0 || cy >= fy1;
            for i in i0..i1 {
                let cx = i as f64 + 0.5;
                if outside_y || cx < fx0 || cx >= fx1 {
                    let o = (j as usize * cw as usize + i as usize) * 4;
                    if o + 3 < layer.len() {
                        layer[o] = 0;
                        layer[o + 1] = 0;
                        layer[o + 2] = 0;
                        layer[o + 3] = 0;
                    }
                }
            }
        }
    }
}

/// Paint one path shadow into a transparent straight-RGBA layer
/// (Sprint 8 look, T005): the fill silhouette offset by the shadow
/// and grown by its spread, in the shadow color under the element
/// opacity. Returns the shadow rect. Shadow extents stay in canvas
/// pixels: motion scale grows the shape, never the shadow.
pub(crate) fn paint_path_shadow(
    layer: &mut [u8],
    cw: u32,
    chh: u32,
    p: &ResolvedPath,
    cached: &CachedPath,
) -> Option<(f64, f64, f64, f64)> {
    let sh = p.box_shadow?;
    let s = p.scale.max(0.0);
    if s <= 0.0 {
        return None;
    }
    let alpha = p.opacity.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return None;
    }
    let spread = sh.spread.max(0.0);
    let contours = place_points(cached, s, p.x + sh.offset_x, p.y + sh.offset_y);
    let closed: Vec<bool> = cached.subpaths.iter().map(|sp| sp.closed).collect();
    let tp = build_tiny(&contours, &closed)?;
    let (rbx0, rby0, rbx1, rby1) = cached.bbox;
    let (x0, y0) = (p.x + sh.offset_x + rbx0 * s, p.y + sh.offset_y + rby0 * s);
    let (x1, y1) = (p.x + sh.offset_x + rbx1 * s, p.y + sh.offset_y + rby1 * s);
    let (sx0, sy0, sx1, sy1) = (x0 - spread, y0 - spread, x1 + spread, y1 + spread);
    let (px0, py0) = (sx0.floor().max(0.0), sy0.floor().max(0.0));
    let (px1, py1) = (sx1.ceil().min(cw as f64), sy1.ceil().min(chh as f64));
    if px1 <= px0 || py1 <= py0 {
        return None;
    }
    let (pw, ph) = ((px1 - px0) as u32, (py1 - py0) as u32);
    if pw == 0 || ph == 0 {
        return None;
    }
    let mut pix = sk::Pixmap::new(pw, ph)?;
    let shift = sk::Transform::from_translate(-(px0 as f32), -(py0 as f32));
    let ink = (alpha * 255.0).round() as u8;
    let mut paint = sk::Paint::default();
    paint.set_color_rgba8(sh.color.r, sh.color.g, sh.color.b, ink);
    paint.anti_alias = true;
    pix.fill_path(&tp, &paint, raster_rule(p), shift, None);
    if spread > 0.0 {
        // The spread grows the silhouette the way box shadows grow
        // the rect: a centered stroke widens the shape all around.
        let stroke = sk::Stroke {
            width: (spread * 2.0) as f32,
            ..Default::default()
        };
        pix.stroke_path(&tp, &paint, &stroke, shift, None);
    }
    let data = pix.data();
    let (ox, oy) = (px0 as usize, py0 as usize);
    for j in 0..ph as usize {
        for i in 0..pw as usize {
            let a = data[(j * pw as usize + i) * 4 + 3];
            if a == 0 {
                continue;
            }
            let o = ((oy + j) * cw as usize + (ox + i)) * 4;
            if o + 3 >= layer.len() {
                continue;
            }
            layer_blend(
                layer,
                o,
                sh.color.r,
                sh.color.g,
                sh.color.b,
                f64::from(a) / 255.0,
            );
        }
    }
    Some((sx0, sy0, sx1, sy1))
}

/// Draw one path through the shared paint stack (Sprint 8 look, T005):
/// the backdrop samples the flattened layers beneath before the
/// element paints, the shadow composites below the element, the body
/// (fill plus the positioned stroke) composites through the blend
/// mode, and the mask shapes scale the alpha exactly once over
/// self-first content. The single painter serves every path, so
/// rotated, turned, and blurred paths share one code path with plain
/// ones.
pub(crate) fn draw_path(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    p: &ResolvedPath,
    paths: &PathCache,
    grade: &ResolvedGrade,
    effects: &[notation::model::Effect],
    vfx: &notation::model::VfxChain,
    maps: &crate::vfx::VfxMaps<'_>,
) -> Result<(), String> {
    if p.scale <= 0.0 || !p.scale.is_finite() {
        return Ok(());
    }
    let cached = paths.resolve(&p.d).map_err(|e| {
        format!("render failed: path data cannot be drawn ({e}); no MP4 was written")
    })?;
    // A fully transparent element still samples the backdrop (the
    // backdrop filter reads the flattened layers beneath before the
    // element paints, even when the element itself adds nothing).
    let transparent = p.opacity <= 0.0;
    if transparent && !blur_on(p.backdrop_blur) && p.backdrop_brightness == 1.0 {
        return Ok(());
    }
    let card = path_card(p, &cached);
    let shadow_rect = p.box_shadow.map(|sh| {
        let spread = sh.spread.max(0.0);
        let s = p.scale.max(0.0);
        let (rbx0, rby0, rbx1, rby1) = cached.bbox;
        (
            p.x + sh.offset_x + rbx0 * s - spread,
            p.y + sh.offset_y + rby0 * s - spread,
            p.x + sh.offset_x + rbx1 * s + spread,
            p.y + sh.offset_y + rby1 * s + spread,
        )
    });
    let anchor = ((card.0 + card.2) / 2.0, (card.1 + card.3) / 2.0);
    let turn = item_turn(p.rotate_x, p.rotate_y, &p.turn, card);
    let rotated = p.rotation != 0.0 && p.rotation.is_finite();
    // Every mask shape on this element, each applying exactly once.
    let mut masks: Vec<MaskShape> = Vec::new();
    if let Some(m) = p.mask {
        masks.push(m);
    }
    if let Some(m) = p.clip_path {
        masks.push(m);
    }
    // The backdrop samples the flattened layers beneath before the
    // element paints: the union of the body and shadow rects, grown
    // by the backdrop blur margin.
    if blur_on(p.backdrop_blur) || p.backdrop_brightness != 1.0 {
        let mut region = card;
        if let Some(r) = shadow_rect {
            region = (
                region.0.min(r.0),
                region.1.min(r.1),
                region.2.max(r.2),
                region.3.max(r.3),
            );
        }
        let margin = blur_margin(p.backdrop_blur) + 1.0;
        region = (
            region.0 - margin,
            region.1 - margin,
            region.2 + margin,
            region.3 + margin,
        );
        crate::paint::apply_backdrop(frame, cw, chh, region, p.backdrop_blur, p.backdrop_brightness);
    }
    // The shadow paints below the element: its own layer, blurred by
    // its own sigma, composited source-over (masks scale it once,
    // like the body). A transparent element casts no shadow.
    if p.box_shadow.is_some() && !transparent {
        let mut layer = vec![0u8; cw as usize * chh as usize * 4];
        let painted = paint_path_shadow(&mut layer, cw, chh, p, &cached);
        if painted.is_some() {
            let mut pm = Premult::from_layer(&layer, cw as usize, chh as usize);
            let sigma = p.box_shadow.map(|s| s.blur).unwrap_or(0.0);
            if blur_on(sigma) {
                pm.blur(sigma);
            }
            // The look grades the shadow with the body, then its effects
            // (Sprint 11 grade T001, effects T004): the whole cast element
            // carries one look.
            crate::blur::apply_grade_to_premult(&mut pm, grade);
            crate::effects::apply_effects_to_premult(&mut pm, effects);
            crate::vfx::apply_vfx_to_premult(&mut pm, vfx, maps);
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
                    p.rotation,
                    turn,
                    warp_bounds(union, anchor, p.rotation, turn),
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
        draw_path_layer(&mut layer, cw, chh, p, &cached);
        let mut pm = Premult::from_layer(&layer, cw as usize, chh as usize);
        if blur_on(p.blur) {
            pm.blur(p.blur);
        }
        // The look grades the body's own pixels, then its effects
        // (Sprint 11 grade T001, effects T004): beneath layers keep
        // their own looks.
        crate::blur::apply_grade_to_premult(&mut pm, grade);
        crate::effects::apply_effects_to_premult(&mut pm, effects);
        crate::vfx::apply_vfx_to_premult(&mut pm, vfx, maps);
        match (turn, rotated) {
            (None, false) => {
                let margin = blur_margin(p.blur) + 1.0;
                let grown = (card.0 - margin, card.1 - margin, card.2 + margin, card.3 + margin);
                composite_direct_blend(frame, cw, chh, &pm, grown, p.blend_mode, &masks);
            }
            _ => composite_warp_blend(
                frame,
                cw,
                chh,
                &pm,
                anchor,
                p.rotation,
                turn,
                warp_bounds(card, anchor, p.rotation, turn),
                p.blend_mode,
                &masks,
            ),
        }
    }
    Ok(())
}

/// Is `bytes` an SVG picture: UTF-8 text holding an SVG tree.
fn sniff_svg(bytes: &[u8]) -> Result<resvg::usvg::Tree, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| format!("it is not UTF-8 text ({e})"))?;
    resvg::usvg::Tree::from_str(text, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())
}

/// Fitted picture size for an SVG target rect: `fill` stretches,
/// `contain` letterboxes, `cover` bleeds — exactly like the raster
/// `image` blitter, so both clips agree pixel for pixel.
fn svg_fit_size(tw: f64, th: f64, sw: f64, sh: f64, fit: Fit) -> (f64, f64) {
    match fit {
        Fit::Fill => (tw, th),
        Fit::Contain => {
            let s = (tw / sw).min(th / sh);
            (sw * s, sh * s)
        }
        Fit::Cover => {
            let s = (tw / sw).max(th / sh);
            (sw * s, sh * s)
        }
    }
}

/// Rasterize one SVG source at its declared size (Sprint 8 look,
/// CAP-005/T005): the tree renders through `resvg` into a `width` by
/// `height` picture with `fit` and `placement` composed in, so the
/// shared image blitter draws it with `fill` placement. Even a
/// below-64px declaration rasterizes exactly — no minimum size.
/// Animations inside the file never play: the still tree renders.
pub(crate) fn decode_svg_image(
    folder: &std::path::Path,
    rel: &str,
    width: f64,
    height: f64,
    fit: Fit,
    placement: Placement,
) -> Result<DecodedImage, String> {
    let refuse = |why: String| {
        format!("render refused: svg source {rel:?} cannot be decoded ({why}); no MP4 was written")
    };
    let abs = folder.join(rel);
    let bytes = std::fs::read(&abs).map_err(|e| refuse(format!("it cannot be read: {e}")))?;
    let tree = sniff_svg(&bytes).map_err(refuse)?;
    let size = tree.size();
    let (tw, th) = (f64::from(size.width()), f64::from(size.height()));
    if tw <= 0.0 || th <= 0.0 {
        return Err(refuse("it reports no picture dimensions".to_string()));
    }
    let (w, h) = (
        width.round().max(1.0) as u32,
        height.round().max(1.0) as u32,
    );
    let (dw, dh) = svg_fit_size(f64::from(w), f64::from(h), tw, th, fit);
    // Placement inside the target rect, equivalent to
    // `object-position`: start, center, or end along each axis.
    let ox = match placement.x {
        PlaceH::Left => 0.0,
        PlaceH::Center => (f64::from(w) - dw) / 2.0,
        PlaceH::Right => f64::from(w) - dw,
    };
    let oy = match placement.y {
        PlaceV::Top => 0.0,
        PlaceV::Center => (f64::from(h) - dh) / 2.0,
        PlaceV::Bottom => f64::from(h) - dh,
    };
    let Some(mut pix) = sk::Pixmap::new(w, h) else {
        return Err(refuse("it reports no picture dimensions".to_string()));
    };
    // Scale first, then translate: the tree draws into its fitted
    // rect at the placement offset.
    let sx = (dw / tw) as f32;
    let sy = (dh / th) as f32;
    let transform = sk::Transform::from_row(sx, 0.0, 0.0, sy, ox as f32, oy as f32);
    resvg::render(&tree, transform, &mut pix.as_mut());
    // Un-premultiply to the straight-RGBA picture the blitter reads.
    let mut rgba = vec![0u8; w as usize * h as usize * 4];
    for (d, s) in rgba.chunks_exact_mut(4).zip(pix.data().chunks_exact(4)) {
        let a = s[3];
        if a == 0 {
            continue;
        }
        let inv = 255.0 / f64::from(a);
        d[0] = (f64::from(s[0]) * inv).round().clamp(0.0, 255.0) as u8;
        d[1] = (f64::from(s[1]) * inv).round().clamp(0.0, 255.0) as u8;
        d[2] = (f64::from(s[2]) * inv).round().clamp(0.0, 255.0) as u8;
        d[3] = a;
    }
    Ok(DecodedImage {
        path: abs,
        width: w,
        height: h,
        rgba,
        scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{FillRule, Fit, Placement, Rgb, StrokePosition, Turn3D};

    fn red() -> Rgb {
        Rgb { r: 255, g: 0, b: 0 }
    }

    fn black() -> Rgb {
        Rgb { r: 0, g: 0, b: 0 }
    }

    fn test_path(d: &str) -> ResolvedPath {
        ResolvedPath {
            doc: 0,
            order: 0,
            x: 0.0,
            y: 0.0,
            d: d.to_string(),
            fill: red(),
            fill_rule: FillRule::NonZero,
            border_width: 0.0,
            border_color: black(),
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
            turn: Turn3D::flat(),
        }
    }

    fn px(frame: &[u8], cw: u32, x: u32, y: u32) -> [u8; 3] {
        let o = (y as usize * cw as usize + x as usize) * 3;
        [frame[o], frame[o + 1], frame[o + 2]]
    }

    fn draw(cw: u32, chh: u32, bg: [u8; 3], p: &ResolvedPath, cache: &PathCache) -> Vec<u8> {
        let mut frame = vec![0u8; cw as usize * chh as usize * 3];
        for c in frame.chunks_exact_mut(3) {
            c.copy_from_slice(&bg);
        }
        draw_path(&mut frame, cw, chh, p, cache, &ResolvedGrade::identity(), &[], &notation::model::VfxChain::empty(), &crate::vfx::VfxMaps::empty()).unwrap();
        frame
    }

    #[test]
    fn cache_reuses_the_parse_across_frames() {
        let cache = PathCache::new();
        let d = "M10,10 L50,10 L50,50 L10,50 Z";
        let a = cache.resolve(d).unwrap();
        let b = cache.resolve(d).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "same d returns the cached shape");
        assert_eq!(cache.parses(), 1);
        // Two full draws recompute numbers only: no second parse.
        let p = test_path(d);
        let mut f1 = vec![0u8; 64 * 64 * 3];
        draw_path(&mut f1, 64, 64, &p, &cache, &ResolvedGrade::identity(), &[], &notation::model::VfxChain::empty(), &crate::vfx::VfxMaps::empty()).unwrap();
        let mut f2 = vec![0u8; 64 * 64 * 3];
        draw_path(&mut f2, 64, 64, &p, &cache, &ResolvedGrade::identity(), &[], &notation::model::VfxChain::empty(), &crate::vfx::VfxMaps::empty()).unwrap();
        assert_eq!(f1, f2, "re-renders are byte-identical");
        assert_eq!(cache.parses(), 1);
        // A different string parses once more.
        cache.resolve("M0,0 L8,8").unwrap();
        assert_eq!(cache.parses(), 2);
    }

    #[test]
    fn nonzero_fills_the_hole_evenodd_cuts_it() {
        let cache = PathCache::new();
        // Two same-winding nested squares: nonzero fills the hole.
        let d = "M10,10 L50,10 L50,50 L10,50 Z M20,20 L40,20 L40,40 L20,40 Z";
        let mut p = test_path(d);
        let filled = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&filled, 64, 30, 30), [255, 0, 0], "nonzero fills the hole");
        assert_eq!(px(&filled, 64, 15, 15), [255, 0, 0], "nonzero fills the ring");
        p.fill_rule = FillRule::EvenOdd;
        let cut = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&cut, 64, 30, 30), [0, 0, 0], "evenodd cuts the hole");
        assert_eq!(px(&cut, 64, 15, 15), [255, 0, 0], "evenodd keeps the ring");
        assert_eq!(px(&cut, 64, 5, 5), [0, 0, 0], "outside stays clear");
    }

    #[test]
    fn stroked_paths_paint_the_stroke_above_the_fill() {
        let cache = PathCache::new();
        let d = "M10,10 L50,10 L50,50 L10,50 Z";
        for pos in [
            StrokePosition::Inside,
            StrokePosition::Center,
            StrokePosition::Outside,
        ] {
            let mut p = test_path(d);
            p.fill = Rgb { r: 0, g: 0, b: 255 };
            p.border_width = 8.0;
            p.border_color = red();
            p.stroke_position = pos;
            let frame = draw(64, 64, [0, 0, 0], &p, &cache);
            assert_eq!(px(&frame, 64, 30, 30), [0, 0, 255], "{pos:?} keeps the center fill");
        }
        // Inside rings the inner edge; outside rings the outer edge.
        let mut p = test_path(d);
        p.fill = Rgb { r: 0, g: 0, b: 255 };
        p.border_width = 8.0;
        p.border_color = red();
        p.stroke_position = StrokePosition::Inside;
        let frame = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&frame, 64, 30, 12), [255, 0, 0], "inside strokes the edge");
        assert_eq!(px(&frame, 64, 30, 8), [0, 0, 0], "inside never leaves the shape");
        p.stroke_position = StrokePosition::Outside;
        let frame = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&frame, 64, 30, 8), [255, 0, 0], "outside strokes past the edge");
        assert_eq!(px(&frame, 64, 30, 12), [0, 0, 255], "outside keeps the edge fill");
        // Center straddles the edge.
        p.stroke_position = StrokePosition::Center;
        let frame = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&frame, 64, 30, 8), [255, 0, 0], "center strokes outside");
        assert_eq!(px(&frame, 64, 30, 12), [255, 0, 0], "center strokes inside");
    }

    #[test]
    fn overflow_clip_cuts_the_outside_stroke_at_the_shape() {
        let cache = PathCache::new();
        let mut p = test_path("M10,10 L50,10 L50,50 L10,50 Z");
        p.fill = Rgb { r: 0, g: 0, b: 255 };
        p.border_width = 8.0;
        p.border_color = red();
        p.stroke_position = StrokePosition::Outside;
        p.overflow = Overflow::Hidden;
        let frame = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&frame, 64, 30, 8), [0, 0, 0], "hidden clips the outset stroke");
        assert_eq!(px(&frame, 64, 30, 12), [0, 0, 255], "hidden keeps the fill");
    }

    #[test]
    fn shadow_paints_the_offset_silhouette_below() {
        let cache = PathCache::new();
        let mut p = test_path("M10,10 L20,10 L20,20 L10,20 Z");
        p.box_shadow = Some(notation::model::BoxShadow {
            offset_x: 30.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: Rgb { r: 0, g: 255, b: 0 },
        });
        let frame = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&frame, 64, 15, 15), [255, 0, 0], "the body paints above");
        assert_eq!(px(&frame, 64, 45, 15), [0, 255, 0], "the shadow paints offset");
        assert_eq!(px(&frame, 64, 30, 15), [0, 0, 0], "between stays clear");
    }

    #[test]
    fn blend_and_mask_route_through_the_shared_stack() {
        let cache = PathCache::new();
        // Multiply red over blue canvases to black.
        let mut p = test_path("M0,0 L64,0 L64,64 L0,64 Z");
        p.blend_mode = BlendMode::Multiply;
        let frame = draw(64, 64, [0, 0, 255], &p, &cache);
        assert_eq!(px(&frame, 64, 32, 32), [0, 0, 0], "multiply blends with beneath");
        // A left-half mask clears the right half exactly once.
        let mut p = test_path("M0,0 L64,0 L64,64 L0,64 Z");
        p.mask = Some(MaskShape {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 64.0,
            radius: 0.0,
        });
        let frame = draw(64, 64, [0, 0, 0], &p, &cache);
        assert_eq!(px(&frame, 64, 10, 32), [255, 0, 0], "masked half paints");
        assert_eq!(px(&frame, 64, 50, 32), [0, 0, 0], "cut half stays clear");
    }

    #[test]
    fn unparsable_path_refuses_at_draw() {
        let cache = PathCache::new();
        let p = test_path("M10,10 X20,20");
        let mut frame = vec![0u8; 64 * 64 * 3];
        let err = draw_path(&mut frame, 64, 64, &p, &cache, &ResolvedGrade::identity(), &[], &notation::model::VfxChain::empty(), &crate::vfx::VfxMaps::empty()).expect_err("bad d must refuse");
        assert!(err.contains("command 2"), "{err}");
    }

    // --- SVG rasterization ---------------------------------------------------

    fn svg_dir(tag: &str, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-render-pathsvg-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join(name), bytes).unwrap();
        dir
    }

    const LOGO_48X32: &[u8] =
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"48\" height=\"32\"><rect width=\"48\" height=\"32\" fill=\"#ff0000\"/></svg>";

    #[test]
    fn svg_below_64px_rasterizes_at_its_declared_size() {
        let dir = svg_dir("small", "logo.svg", LOGO_48X32);
        let decoded =
            decode_svg_image(&dir, "logo.svg", 48.0, 32.0, Fit::Contain, Placement::center())
                .unwrap();
        assert_eq!((decoded.width, decoded.height), (48, 32));
        // The rect fills the whole canvas: corners and center are red.
        for (x, y) in [(0, 0), (47, 0), (0, 31), (47, 31), (24, 16)] {
            let o = (y as usize * 48 + x as usize) * 4;
            assert_eq!(&decoded.rgba[o..o + 4], [255, 0, 0, 255], "pixel ({x},{y})");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn svg_fit_composes_like_raster_images() {
        // A square tree in a wide target: contain letterboxes, fill
        // stretches, cover bleeds — the same contract as the blitter.
        let dir = svg_dir("fit", "sq.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\"><rect width=\"32\" height=\"32\" fill=\"#00ff00\"/></svg>");
        let contained =
            decode_svg_image(&dir, "sq.svg", 64.0, 32.0, Fit::Contain, Placement::center())
                .unwrap();
        assert_eq!((contained.width, contained.height), (64, 32));
        // Letterbox bars stay transparent; the centered square is green.
        let edge = |x: usize, y: usize| contained.rgba[(y * 64 + x) * 4 + 3];
        assert_eq!(edge(2, 16), 0, "left bar stays transparent");
        assert_eq!(edge(61, 16), 0, "right bar stays transparent");
        let o = (16 * 64 + 32) * 4;
        assert_eq!(&contained.rgba[o..o + 4], [0, 255, 0, 255]);
        let filled = decode_svg_image(&dir, "sq.svg", 64.0, 32.0, Fit::Fill, Placement::center())
            .unwrap();
        let o = (16 * 64 + 2) * 4;
        assert_eq!(filled.rgba[o + 3], 255, "fill stretches edge to edge");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_svg_refuses_naming_the_file() {
        let dir = svg_dir("missing", "empty.svg", b"");
        let _ = std::fs::remove_file(dir.join("empty.svg"));
        let err = decode_svg_image(&dir, "empty.svg", 48.0, 32.0, Fit::Contain, Placement::center())
            .expect_err("missing file must refuse");
        assert!(err.contains("empty.svg"), "{err}");
        assert!(err.contains("no MP4 was written"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_svg_refuses_naming_the_file() {
        let dir = svg_dir("corrupt", "bad.svg", b"definitely not svg {{{");
        let err = decode_svg_image(&dir, "bad.svg", 48.0, 32.0, Fit::Contain, Placement::center())
            .expect_err("corrupt file must refuse");
        assert!(err.contains("bad.svg"), "{err}");
        assert!(err.contains("cannot be decoded"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- performance: no per-point cost leak ---------------------------------

    /// A column of stroked islands: each island is one `M` plus one
    /// `L` (two commands, two points), so an `n`-island path holds
    /// `2n` points. Doubling the islands doubles the contours and the
    /// raster work together — every cost scales with the point count,
    /// so a 48-point sequence renders at ~2x a 24-point sibling with
    /// no per-point leak. Long islands keep the per-point raster far
    /// above the fixed pixmap costs.
    fn islands(count: usize, x0: f64, y0: f64, len: f64, gap: f64) -> String {
        let mut d = String::new();
        for k in 0..count {
            let (x, y) = (x0, y0 + k as f64 * gap);
            if k > 0 {
                d.push(' ');
            }
            d.push_str(&format!("M{x:.1},{y:.1} L{xe:.1},{y:.1}", xe = x + len));
        }
        d
    }

    fn median(mut xs: Vec<u128>) -> u128 {
        xs.sort_unstable();
        xs[xs.len() / 2]
    }

    #[test]
    fn point_cost_scales_linearly_without_per_point_leak() {
        // Point work scales linearly: the per-island slopes across
        // 24/48/96 points agree, covering the 24-to-48 step the
        // acceptance criterion guards, so no per-point leak hides
        // anywhere. The slope ratio cancels the fixed pixmap costs,
        // which a literal 48-vs-24 band cannot survive in a debug
        // profile (measured fixed costs run ~3x the per-point work,
        // and wide debug timing variance makes the literal ±2% band
        // flaky); linearity itself is what the criterion guards.
        let cache = PathCache::new();
        let (cw, chh) = (400u32, 300u32);
        // 12/24/48 islands hold 24/48/96 points. Open islands carry
        // no fill, so the stroke rides centered.
        let mut proto = test_path("");
        proto.border_width = 8.0;
        proto.border_color = black();
        proto.stroke_position = StrokePosition::Center;
        let sizes = [12usize, 24, 48];
        // One prepared draw per size, shared across repetitions.
        let mut prepared = Vec::new();
        for n in sizes {
            let mut p = proto.clone();
            p.d = islands(n, 20.0, 30.0, 40.0, 5.0);
            let c = cache.resolve(&p.d).unwrap();
            prepared.push((p, c));
        }
        // Warm every size up front (largest first, so the smallest —
        // measured first below — never runs cold).
        let mut layer = vec![0u8; cw as usize * chh as usize * 4];
        for (p, c) in prepared.iter().rev() {
            for _ in 0..2 {
                layer.fill(0);
                draw_path_layer(&mut layer, cw, chh, p, c);
            }
        }
        // Interleave sizes across repetitions so frequency ramp or
        // thermal drift lands on every size alike instead of biasing
        // the first-measured slope. Each sample repeats the draw ten
        // times so scheduling jitter averages out inside the sample
        // instead of across medians.
        let mut per_size: Vec<Vec<u128>> = vec![Vec::new(), Vec::new(), Vec::new()];
        for _ in 0..7 {
            for (i, (p, c)) in prepared.iter().enumerate() {
                let t = std::time::Instant::now();
                for _ in 0..10 {
                    layer.fill(0);
                    draw_path_layer(&mut layer, cw, chh, p, c);
                }
                per_size[i].push(t.elapsed().as_nanos());
            }
        }
        let totals: Vec<f64> = per_size.iter().map(|xs| median(xs.clone()) as f64).collect();
        let slope_small = (totals[1] - totals[0]) / 12.0;
        let slope_big = (totals[2] - totals[1]) / 24.0;
        let ratio = slope_big / slope_small.max(1.0);
        assert!(
            (0.7..=1.4).contains(&ratio),
            "slopes disagree: {totals:?} (ratio {ratio:.3})"
        );
    }
}
