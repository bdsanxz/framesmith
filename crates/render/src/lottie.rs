//! Native Lottie frame rasterizer (Sprint 21 adapters, T001 / CAP-001).
//!
//! Pure Rust, no browser, no JavaScript, no network: a decoded
//! [`lottie::LottieDoc`] renders through `tiny-skia` (the same engine
//! behind SVG rasterization) into a straight-RGBA picture the shared
//! image blitter draws. Same inputs render byte-identical pixels on
//! every run: every channel is a pure function of the file bytes, the
//! animation instant, and the declared size — no wall-clock, no
//! randomness, no expression programs (expressions bake at decode).
//!
//! The animation pre-composes into the declared `width`/`height`
//! with `fit` and `placement`, exactly like an `svg` clip, so the
//! blitter draws the result with `fill` placement afterwards.

use lottie::{Fill, Group, Layer, LottieDoc, ShapeItem, Stroke, Transform};
use notation::model::{Fit, PlaceH, PlaceV, Placement};
use resvg::tiny_skia as sk;

use crate::render::DecodedImage;

/// One 2D affine map (`x' = a*x + c*y + e`, `y' = b*x + d*y + f`),
/// composed in f64 and converted once per draw call, so repeated
/// group nesting never accumulates f32 rounding frame to frame.
#[derive(Debug, Clone, Copy)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Affine {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// This map after `o` (points flow through `o` first).
    fn then(&self, o: &Affine) -> Affine {
        Affine {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    fn translate(x: f64, y: f64) -> Affine {
        Affine {
            e: x,
            f: y,
            ..Affine::identity()
        }
    }

    fn scale(sx: f64, sy: f64) -> Affine {
        Affine {
            a: sx,
            d: sy,
            ..Affine::identity()
        }
    }

    fn rotate(degrees: f64) -> Affine {
        let r = degrees.to_radians();
        let (s, c) = r.sin_cos();
        Affine {
            a: c,
            b: s,
            c: -s,
            d: c,
            ..Affine::identity()
        }
    }

    fn to_sk(self) -> sk::Transform {
        sk::Transform::from_row(
            self.a as f32,
            self.b as f32,
            self.c as f32,
            self.d as f32,
            self.e as f32,
            self.f as f32,
        )
    }
}

/// Bodymovin transform block evaluated at one animation frame
/// (Sprint 21, T001): `T(pos) * R(rot) * S(scale) * T(-anchor)`.
fn eval_transform(t: &Transform, frame: f64) -> (Affine, f64) {
    let pos = t.position.at(frame);
    let anchor = t.anchor.at(frame);
    let scale = t.scale.at(frame);
    let rot = t.rotation.at(frame);
    let opacity = t.opacity.at(frame);
    let px = *pos.first().unwrap_or(&0.0);
    let py = *pos.get(1).unwrap_or(&0.0);
    let ax = *anchor.first().unwrap_or(&0.0);
    let ay = *anchor.get(1).unwrap_or(&0.0);
    let sx = *scale.first().unwrap_or(&100.0) / 100.0;
    let sy = *scale.get(1).unwrap_or(&100.0) / 100.0;
    let m = Affine::translate(px, py)
        .then(&Affine::rotate(rot))
        .then(&Affine::scale(sx, sy))
        .then(&Affine::translate(-ax, -ay));
    (m, opacity / 100.0)
}

/// Round one straight-RGBA channel to a byte, deterministically.
fn channel_byte(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Combined paint alpha 0-1 from the accumulated layer/group alpha
/// (already 0-1), the style opacity (0-100), and the color alpha
/// (0-1).
fn combined_alpha(alpha: f64, style_opacity: f64, color_alpha: f64) -> f64 {
    (alpha * style_opacity / 100.0 * color_alpha).clamp(0.0, 1.0)
}

/// Cubic-arc constant for ellipse quadrants.
const KAPPA: f64 = 0.5522847498307936;

/// Ellipse path centered at `(cx, cy)` with radii `(rx, ry)`,
/// drawn from four cubic quadrants (Sprint 21, T001).
fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> Option<sk::Path> {
    if !rx.is_finite() || !ry.is_finite() || rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    let mut pb = sk::PathBuilder::new();
    let k = KAPPA;
    let f = |v: f64| v as f32;
    pb.move_to(f(cx + rx), f(cy));
    pb.cubic_to(
        f(cx + rx),
        f(cy - k * ry),
        f(cx + k * rx),
        f(cy - ry),
        f(cx),
        f(cy - ry),
    );
    pb.cubic_to(
        f(cx - k * rx),
        f(cy - ry),
        f(cx - rx),
        f(cy - k * ry),
        f(cx - rx),
        f(cy),
    );
    pb.cubic_to(
        f(cx - rx),
        f(cy + k * ry),
        f(cx - k * rx),
        f(cy + ry),
        f(cx),
        f(cy + ry),
    );
    pb.cubic_to(
        f(cx + k * rx),
        f(cy + ry),
        f(cx + rx),
        f(cy + k * ry),
        f(cx + rx),
        f(cy),
    );
    pb.close();
    pb.finish()
}

/// Rectangle path centered at `(cx, cy)` with size `(w, h)` and
/// corner radius `r` (clamped to the half-edges), drawn with
/// quadratic corners (Sprint 21, T001).
fn rect_path(cx: f64, cy: f64, w: f64, h: f64, r: f64) -> Option<sk::Path> {
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let (x0, y0) = (cx - w / 2.0, cy - h / 2.0);
    let (x1, y1) = (cx + w / 2.0, cy + h / 2.0);
    let r = r.clamp(0.0, w.min(h) / 2.0);
    let mut pb = sk::PathBuilder::new();
    let f = |v: f64| v as f32;
    if r <= 0.0 {
        pb.move_to(f(x0), f(y0));
        pb.line_to(f(x1), f(y0));
        pb.line_to(f(x1), f(y1));
        pb.line_to(f(x0), f(y1));
        pb.close();
        return pb.finish();
    }
    pb.move_to(f(x0 + r), f(y0));
    pb.line_to(f(x1 - r), f(y0));
    pb.quad_to(f(x1), f(y0), f(x1), f(y0 + r));
    pb.line_to(f(x1), f(y1 - r));
    pb.quad_to(f(x1), f(y1), f(x1 - r), f(y1));
    pb.line_to(f(x0 + r), f(y1));
    pb.quad_to(f(x0), f(y1), f(x0), f(y1 - r));
    pb.line_to(f(x0), f(y0 + r));
    pb.quad_to(f(x0), f(y0), f(x0 + r), f(y0));
    pb.close();
    pb.finish()
}

/// Solid paint with straight-RGBA channels and one combined alpha.
fn solid_paint(r: f64, g: f64, b: f64, alpha: f64) -> sk::Paint<'static> {
    sk::Paint {
        shader: sk::Shader::SolidColor(sk::Color::from_rgba8(
            channel_byte(r * 255.0),
            channel_byte(g * 255.0),
            channel_byte(b * 255.0),
            channel_byte(alpha * 255.0),
        )),
        blend_mode: sk::BlendMode::SourceOver,
        anti_alias: true,
        force_hq_pipeline: false,
    }
}

/// Draw one geometry item with the in-scope fill/stroke through the
/// accumulated map (Sprint 21, T001).
#[allow(clippy::too_many_arguments)]
fn draw_item(
    pix: &mut sk::Pixmap,
    item: &ShapeItem,
    base: &Affine,
    fill: Option<&Fill>,
    stroke: Option<&Stroke>,
    alpha: f64,
) {
    let path = match item {
        ShapeItem::Rect { pos, size, round } => rect_path(pos[0], pos[1], size[0], size[1], *round),
        ShapeItem::Ellipse { pos, size } => {
            ellipse_path(pos[0], pos[1], size[0] / 2.0, size[1] / 2.0)
        }
        ShapeItem::Group(_) => None,
    };
    let Some(path) = path else { return };
    if let Some(f) = fill {
        let a = combined_alpha(alpha, f.opacity, f.color[3]);
        if a > 0.0 {
            let paint = solid_paint(f.color[0], f.color[1], f.color[2], a);
            pix.fill_path(&path, &paint, sk::FillRule::Winding, base.to_sk(), None);
        }
    }
    if let Some(s) = stroke {
        let a = combined_alpha(alpha, s.opacity, 1.0);
        if a > 0.0 && s.width.is_finite() && s.width > 0.0 {
            let paint = solid_paint(s.color[0], s.color[1], s.color[2], a);
            // The width rides in device pixels after the path
            // transform (fixture scales stay 100%, so authored
            // widths paint exactly).
            let stroke = sk::Stroke {
                width: s.width as f32,
                ..Default::default()
            };
            pix.stroke_path(&path, &paint, &stroke, base.to_sk(), None);
        }
    }
}

/// Draw one shape group with the accumulated parent map, style, and
/// opacity (Sprint 21, T001): a nested group inherits the in-scope
/// fill/stroke unless it declares its own, and opacities multiply
/// down the tree.
#[allow(clippy::too_many_arguments)]
fn draw_group(
    pix: &mut sk::Pixmap,
    group: &Group,
    frame: f64,
    parent: &Affine,
    parent_fill: Option<&Fill>,
    parent_stroke: Option<&Stroke>,
    parent_alpha: f64,
) {
    let (local, opacity) = eval_transform(&group.transform, frame);
    let base = parent.then(&local);
    let alpha = (parent_alpha * opacity).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let fill = group.fill.as_ref().or(parent_fill);
    let stroke = group.stroke.as_ref().or(parent_stroke);
    for item in &group.items {
        match item {
            ShapeItem::Group(nested) => draw_group(pix, nested, frame, &base, fill, stroke, alpha),
            ShapeItem::Rect { .. } | ShapeItem::Ellipse { .. } => {
                draw_item(pix, item, &base, fill, stroke, alpha)
            }
        }
    }
}

/// Draw one layer at one animation frame through the outer fit map
/// (Sprint 21, T001): layers outside their `[in_frame, out_frame)`
/// window draw nothing; the layer clock runs `start_offset` frames
/// behind the animation clock.
fn draw_layer(pix: &mut sk::Pixmap, layer: &Layer, frame: f64, outer: &Affine) {
    if !(frame >= layer.in_frame && frame < layer.out_frame) {
        return;
    }
    let local = frame - layer.start_offset;
    let (base, opacity) = eval_transform(&layer.transform, local);
    let alpha = opacity.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let base = outer.then(&base);
    for item in &layer.shapes {
        match item {
            ShapeItem::Group(group) => draw_group(pix, group, local, &base, None, None, alpha),
            ShapeItem::Rect { .. } | ShapeItem::Ellipse { .. } => {
                draw_item(pix, item, &base, None, None, alpha)
            }
        }
    }
}

/// Fitted animation size inside a `w` by `h` target (Sprint 21,
/// T001): `fill` covers exactly, `contain` letterboxes, `cover`
/// bleeds — the same contract as the SVG rasterizer, so both clips
/// agree pixel for pixel.
fn lottie_fit_size(tw: f64, th: f64, sw: f64, sh: f64, fit: Fit) -> (f64, f64) {
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

/// Rasterize one animation instant (Sprint 21, T001): `anim_secs`
/// counts animation seconds from the `ip` edge (already loop-mapped
/// by the caller through [`lottie::sample_time`]). The animation
/// pre-composes into a `width` by `height` picture with `fit` and
/// `placement`, so the shared image blitter draws the result with
/// `fill` placement. A transparent animation draws a transparent
/// picture; an empty layer list draws nothing. Refusals name the
/// file — never a partial MP4.
pub(crate) fn rasterize(
    doc: &LottieDoc,
    anim_secs: f64,
    width: f64,
    height: f64,
    fit: Fit,
    placement: Placement,
    display: &str,
) -> Result<DecodedImage, String> {
    let refuse = |why: String| {
        format!(
            "render refused: lottie source {display:?} cannot be rasterized ({why}); no MP4 was written"
        )
    };
    if !anim_secs.is_finite() {
        return Err(refuse("the animation instant is not finite".to_string()));
    }
    let (w, h) = (
        width.round().max(1.0) as u32,
        height.round().max(1.0) as u32,
    );
    if doc.width <= 0.0 || doc.height <= 0.0 {
        return Err(refuse("it reports no picture dimensions".to_string()));
    }
    let (dw, dh) = lottie_fit_size(f64::from(w), f64::from(h), doc.width, doc.height, fit);
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
    // Scale first, then translate: the animation draws into its
    // fitted rect at the placement offset.
    let outer = Affine {
        a: dw / doc.width,
        d: dh / doc.height,
        e: ox,
        f: oy,
        ..Affine::identity()
    };
    let frame = doc.in_point + anim_secs * doc.frame_rate;
    // Document order paints layer 0 on top, so later layers draw
    // first and earlier layers cover them.
    for layer in doc.layers.iter().rev() {
        draw_layer(&mut pix, layer, frame, &outer);
    }
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
        path: std::path::PathBuf::from(display),
        width: w,
        height: h,
        rgba,
        scaled: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::LoopMode;

    fn slide_doc() -> LottieDoc {
        let bytes = lottie_slide_fixture();
        lottie::decode_source("slide.json", &bytes).expect("fixture decodes")
    }

    /// The shared slide fixture, inlined here so render tests never
    /// depend on another crate's test helper.
    fn lottie_slide_fixture() -> Vec<u8> {
        serde_json::json!({
            "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 60.0, "w": 64.0, "h": 64.0,
            "nm": "slide",
            "markers": [{"tm": 0.0, "cm": "intro", "dr": 30.0}],
            "layers": [{
                "ddd": 0, "ind": 1, "ty": 4, "nm": "box", "sr": 1.0,
                "ks": {
                    "o": {"a": 0, "k": 100.0},
                    "r": {"a": 0, "k": 0.0},
                    "p": {"a": 1, "k": [
                        {"t": 0.0, "s": [10.0, 32.0, 0.0], "e": [50.0, 32.0, 0.0]},
                        {"t": 30.0, "s": [50.0, 32.0, 0.0]}
                    ]},
                    "a": {"a": 0, "k": [0.0, 0.0, 0.0]},
                    "s": {"a": 0, "k": [100.0, 100.0, 100.0]}
                },
                "shapes": [{
                    "ty": "gr", "it": [
                        {"ty": "rc", "p": {"a": 0, "k": [0.0, 0.0]}, "s": {"a": 0, "k": [20.0, 20.0]}, "r": {"a": 0, "k": 0.0}},
                        {"ty": "fl", "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]}, "o": {"a": 0, "k": 100.0}},
                        {"ty": "tr", "p": {"a": 0, "k": [0.0, 0.0]}, "a": {"a": 0, "k": [0.0, 0.0]},
                         "s": {"a": 0, "k": [100.0, 100.0]}, "r": {"a": 0, "k": 0.0}, "o": {"a": 0, "k": 100.0}}
                    ]
                }]
            }]
        })
        .to_string()
        .into_bytes()
    }

    fn red_at(rgba: &[u8], w: u32, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let o = (y * w + x) as usize * 4;
        (rgba[o], rgba[o + 1], rgba[o + 2], rgba[o + 3])
    }

    #[test]
    fn seek_moves_the_rect_and_reruns_match() {
        let doc = slide_doc();
        let pic = Placement::center();
        let a = rasterize(&doc, 0.0, 64.0, 64.0, Fit::Fill, pic, "slide.json").expect("raster");
        let b = rasterize(&doc, 1.0, 64.0, 64.0, Fit::Fill, pic, "slide.json").expect("raster");
        assert_ne!(a.rgba, b.rgba, "different instants paint differently");
        let again = rasterize(&doc, 1.0, 64.0, 64.0, Fit::Fill, pic, "slide.json").expect("raster");
        assert_eq!(b.rgba, again.rgba, "same inputs are byte-identical");
        // At t=0 the rect sits at x=10 (20px wide, centered at 10):
        // pixel (10,32) is red, pixel (50,32) is transparent.
        assert_eq!(red_at(&a.rgba, 64, 10, 32), (255, 0, 0, 255));
        assert_eq!(red_at(&a.rgba, 64, 50, 32).3, 0);
        // At t=1 the rect sits at x=50: pixel (50,32) is red.
        assert_eq!(red_at(&b.rgba, 64, 50, 32), (255, 0, 0, 255));
    }

    #[test]
    fn loop_wrap_matches_base_instant() {
        let doc = slide_doc();
        let pic = Placement::center();
        let len = doc.len_secs();
        let base = rasterize(
            &doc,
            lottie::sample_time(len, LoopMode::Loop, 0.5),
            64.0,
            64.0,
            Fit::Fill,
            pic,
            "s",
        )
        .expect("raster");
        let wrapped = rasterize(
            &doc,
            lottie::sample_time(len, LoopMode::Loop, 0.5 + len),
            64.0,
            64.0,
            Fit::Fill,
            pic,
            "s",
        )
        .expect("raster");
        assert_eq!(base.rgba, wrapped.rgba, "loop wrap is time-correct");
    }
}
