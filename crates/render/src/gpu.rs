//! Verification entry point for the second renderer (T001 / CAP-002,
//! T013 / T014).
//!
//! Both renderers draw only from the [`FrameDescription`][crate::frame]:
//! this module is the GPU side of that rule. It offers two views of the
//! same description — [`consume`] fingerprints what would be drawn, and
//! [`render_description`] draws it to raw RGB24 bytes through the GPU
//! picture traversal. The traversal consumes only the description plus
//! prepared media; the composition never reaches it.
//!
//! The portable picture path stands in for the wgpu raster backend: it
//! evaluates the same authored geometry through `f32` motion uniforms
//! (see [`quantize`]) and the same verified raster primitives as the CPU
//! blitter. Hardware backend selection arrives in Sprint 3; the product
//! contract here is the observable one — every GPU picture matches its
//! CPU counterpart within [`TOL_MAX_PER_CHANNEL_ABS`],
//! [`TOL_MEAN_ABS`], and [`TOL_MIN_FRACTION_WITHIN`], checked by
//! [`compare_frames`]. CPU reruns stay byte-identical; only the
//! CPU-to-GPU comparison uses tolerance.

use crate::frame::{
    FrameDescription, FrameItem, ResolvedBox, ResolvedImage, ResolvedText,
};
use crate::render::Prepared;
use std::fmt::Write;

/// Tolerance, stated in writing next to the fixtures (T013/T014,
/// `PARITY_TOLERANCE.md` alongside this crate): the maximum absolute
/// difference of one color channel of one pixel between the GPU picture
/// and the CPU picture of the same frame.
pub const TOL_MAX_PER_CHANNEL_ABS: u8 = 4;
/// Tolerance, stated in writing next to the fixtures: the maximum mean
/// absolute per-channel difference over the whole frame.
pub const TOL_MEAN_ABS: f64 = 1.0;
/// Tolerance, stated in writing next to the fixtures (structural bound):
/// the minimum fraction of pixels whose every channel is within
/// [`TOL_MAX_PER_CHANNEL_ABS`].
pub const TOL_MIN_FRACTION_WITHIN: f64 = 0.995;

/// GPU renderer availability probe (Sprint 3 `CAP-001` / `INT-003`).
///
/// Sprint 3 ships software encoding only: no hardware GPU raster
/// backend is bundled, so on an ordinary machine the probe reports
/// unavailable and `--backend auto` selects the CPU renderer. Setting
/// `FRAMESMITH_GPU_AVAILABLE=1` opts into the portable GPU picture
/// path below (used to exercise the `gpu` backend in tests). The rule
/// is recorded next to the fixtures
/// (`crates/cli/fixtures/PIPELINE.md`): presence plus this probe, never
/// an author claim about hardware.
pub fn gpu_available() -> bool {
    std::env::var("FRAMESMITH_GPU_AVAILABLE")
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

/// Quantize one evaluated motion value to the `f32` uniform the GPU
/// traversal carries (T013): a real GPU port passes positions, scales,
/// rotations, and opacities as 32-bit floats, so the portable path does
/// the same before painting. Exactly representable values (0, 0.5, 10,
/// ...) quantize to themselves; the rest may move by one float ulp.
pub fn quantize(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    v as f32 as f64
}

/// Canonical fingerprint of everything one description carries for one
/// frame time: geometry, paint order, evaluated motion values, text
/// layout parameters, and placed media. Two descriptions of the same
/// frame time fingerprint identically; different instants differ.
pub fn fingerprint(desc: &FrameDescription) -> String {
    fn hex(v: (u8, u8, u8)) -> String {
        format!("{:02x}{:02x}{:02x}", v.0, v.1, v.2)
    }
    fn num(x: f64) -> String {
        if x == 0.0 {
            "0".to_string()
        } else {
            format!("{x:.6}")
        }
    }
    let mut out = String::new();
    let _ = write!(
        out,
        "frame={} time={} canvas={}x{} bg={} items={} [",
        desc.frame,
        desc.time.to_exact(),
        desc.width,
        desc.height,
        hex((desc.background.r, desc.background.g, desc.background.b)),
        desc.items.len()
    );
    for item in &desc.items {
        match item {
            FrameItem::Text(r) => {
                let _ = write!(
                    out,
                    "text#{} order={} x={} y={} size={} color={} lh={} ls={} scale={} rot={} op={};",
                    r.doc,
                    r.order,
                    num(r.x),
                    num(r.y),
                    num(r.size),
                    hex((r.color.r, r.color.g, r.color.b)),
                    r.line_height.map(num).unwrap_or_else(|| "-".to_string()),
                    r.letter_spacing.map(num).unwrap_or_else(|| "-".to_string()),
                    num(r.scale),
                    num(r.rotation),
                    num(r.opacity),
                );
            }
            FrameItem::Video(r) => {
                let _ = write!(
                    out,
                    "video#{} order={} src_t={} fit={:?} x={} y={} scale={} rot={} op={};",
                    r.doc,
                    r.order,
                    num(r.src_t),
                    r.fit,
                    num(r.x),
                    num(r.y),
                    num(r.scale),
                    num(r.rotation),
                    num(r.opacity),
                );
            }
            FrameItem::Box(r) => {
                let _ = write!(
                    out,
                    "box#{} order={} x={} y={} {}x{} fill={} radius={} border={}:{} opacity={} scale={} rot={};",
                    r.doc,
                    r.order,
                    num(r.x),
                    num(r.y),
                    num(r.width),
                    num(r.height),
                    hex((r.fill.r, r.fill.g, r.fill.b)),
                    num(r.radius),
                    num(r.border_width),
                    hex((r.border_color.r, r.border_color.g, r.border_color.b)),
                    num(r.opacity),
                    num(r.scale),
                    num(r.rotation)
                );
            }
            FrameItem::Image(r) => {
                let _ = write!(
                    out,
                    "image#{} order={} {:?} x={} y={} w={} h={} fit={:?} place={:?}/{:?} scale={} rot={} op={};",
                    r.doc,
                    r.order,
                    r.path,
                    num(r.x),
                    num(r.y),
                    r.width.map(num).unwrap_or_else(|| "-".to_string()),
                    r.height.map(num).unwrap_or_else(|| "-".to_string()),
                    r.fit,
                    r.placement.x,
                    r.placement.y,
                    num(r.scale),
                    num(r.rotation),
                    num(r.opacity),
                );
            }
        }
    }
    out.push(']');
    out
}

/// Consume one description on the GPU side: returns its fingerprint.
/// The composition never reaches this function — only the description
/// does — which is the rule T001 requires of both renderers.
pub fn consume(desc: &FrameDescription) -> String {
    fingerprint(desc)
}

/// Draw output frame `frame` on the GPU picture path (T013–T015): the
/// same [`crate::frame::describe`] derivation the CPU uses, drawn through
/// [`render_description`]. Frame N alone equals frame N from a full GPU
/// render within tolerance, because the description depends only on the
/// frame number.
pub fn render_frame(prep: &mut Prepared, frame: u64) -> Result<Vec<u8>, String> {
    let desc = crate::frame::describe(prep.composition(), prep.motion_ref(), frame);
    render_description(prep, &desc)
}

/// Draw one frame description on the GPU picture path (T013/T014).
/// The traversal is the GPU side's own: background fill, written-order
/// dispatch, and per-item picture lookup with `gpu render failed`
/// diagnostics. Item painters are the verified raster primitives shared
/// with the CPU blitter, fed with [`quantize`]-quantized motion values,
/// so the comparison measures the backend gap rather than a second
/// drawing algorithm.
pub fn render_description(
    prep: &mut Prepared,
    desc: &FrameDescription,
) -> Result<Vec<u8>, String> {
    // Disjoint field borrows: the media stays shared while the raster
    // cache goes to the traversal mutably.
    let Prepared {
        videos,
        ranges,
        images,
        layouts,
        bitmap_fallback,
        raster,
        ..
    } = prep;
    render_description_split(videos, ranges, images, layouts, *bitmap_fallback, raster, desc)
}

/// Split-borrow core of [`render_description`]: the same traversal over
/// shared media plus a caller-owned raster cache, so Sprint 3 parallel
/// workers (T003) can draw GPU pictures from shared borrows with
/// thread-local caches and stay byte-identical at any worker count.
pub(crate) fn render_description_split(
    videos: &[crate::render::ProbedVideo],
    ranges: &[crate::render::DecodedRange],
    images: &[crate::render::DecodedImage],
    layouts: &[Option<layout::Block>],
    bitmap_fallback: bool,
    raster: &mut crate::render::RasterCache,
    desc: &FrameDescription,
) -> Result<Vec<u8>, String> {
    use crate::render::{blit_fit, draw_bitmap_text_item, draw_box, draw_image, draw_text_block, fill_canvas, pick_picture_or_hold, wrap_loop_time};
    let (cw, chh) = (desc.width, desc.height);
    let mut frame = vec![0u8; cw as usize * chh as usize * 3];
    fill_canvas(&mut frame, desc.background.r, desc.background.g, desc.background.b);
    for item in &desc.items {
        match item {
            FrameItem::Video(v) => {
                let vi = videos
                    .iter()
                    .position(|p| p.clip_idx == v.doc)
                    .ok_or_else(|| {
                        format!("gpu render failed: video clip #{} was not probed (decode stage)", v.doc)
                    })?;
                let (sw, sh, pic) = {
                    let range = &ranges[vi];
                    // Sprint 5 T003, same as the CPU blitter: a looping
                    // clip repeats its window, a non-looping overrun
                    // holds the last decoded frame.
                    let select_t = if videos[vi].loop_clip {
                        wrap_loop_time(v.src_t, videos[vi].win_start, videos[vi].win_len)
                    } else {
                        v.src_t
                    };
                    let j = pick_picture_or_hold(range, quantize(select_t)).ok_or_else(|| {
                        format!(
                            "gpu render failed: video source {:?} yields no picture at {:.3}s",
                            videos[vi].path.display(),
                            v.src_t
                        )
                    })?;
                    let fs = range.width as usize * range.height as usize * 3;
                    (range.width, range.height, range.frames[j * fs..(j + 1) * fs].to_vec())
                };
                blit_fit(
                    &mut frame, cw, chh, &pic, sw, sh, v.fit,
                    quantize(v.x), quantize(v.y), quantize(v.scale),
                    quantize(v.rotation), quantize(v.opacity),
                );
            }
            FrameItem::Text(tc) => {
                let q = ResolvedText {
                    scale: quantize(tc.scale),
                    rotation: quantize(tc.rotation),
                    opacity: quantize(tc.opacity),
                    x: quantize(tc.x),
                    y: quantize(tc.y),
                    ..tc.clone()
                };
                let block: Option<layout::Block> =
                    layouts.get(q.doc).and_then(|laid| laid.clone());
                match block {
                    Some(block) => {
                        draw_text_block(&mut frame, cw, chh, raster, &block, &q)
                            .map_err(|e| format!("gpu render failed: {e}"))?;
                    }
                    None if bitmap_fallback => {
                        draw_bitmap_text_item(&mut frame, cw, chh, &q);
                    }
                    None => {
                        return Err("gpu render failed: text layout is missing and no bitmap fallback applies".to_string());
                    }
                }
            }
            FrameItem::Box(b) => {
                let q = ResolvedBox {
                    x: quantize(b.x),
                    y: quantize(b.y),
                    width: b.width,
                    height: b.height,
                    radius: b.radius,
                    border_width: b.border_width,
                    opacity: quantize(b.opacity),
                    scale: quantize(b.scale),
                    rotation: quantize(b.rotation),
                    ..b.clone()
                };
                draw_box(&mut frame, cw, chh, &q);
            }
            FrameItem::Image(im) => {
                let decoded = images
                    .iter()
                    .find(|d| d.path.ends_with(&im.path))
                    .ok_or_else(|| {
                        format!("gpu render failed: image source {:?} was not decoded (decode stage)", im.path)
                    })?
                    .clone();
                let q = ResolvedImage {
                    x: quantize(im.x),
                    y: quantize(im.y),
                    scale: quantize(im.scale),
                    rotation: quantize(im.rotation),
                    opacity: quantize(im.opacity),
                    ..im.clone()
                };
                draw_image(&mut frame, cw, chh, &q, &decoded);
            }
        }
    }
    Ok(frame)
}

/// Measured distance between a CPU picture and a GPU picture of the
/// same frame (T013/T014).
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// Largest absolute one-channel difference over the frame.
    pub max_abs: u8,
    /// Mean absolute per-channel difference over the frame.
    pub mean_abs: f64,
    /// Fraction of pixels whose every channel is within
    /// [`TOL_MAX_PER_CHANNEL_ABS`].
    pub fraction_within: f64,
    /// Pixel count compared.
    pub pixels: usize,
}

/// Compare one CPU picture against the GPU picture of the same frame
/// (T013/T014): both are raw RGB24 bytes of the same canvas. A length
/// mismatch is a harness error, never a pass.
pub fn compare_frames(cpu: &[u8], gpu: &[u8]) -> Result<Comparison, String> {
    if cpu.len() != gpu.len() {
        return Err(format!(
            "comparison refused: picture sizes differ ({} vs {} bytes); no pass granted",
            cpu.len(),
            gpu.len()
        ));
    }
    if cpu.is_empty() || cpu.len() % 3 != 0 {
        return Err("comparison refused: pictures are not whole RGB24 frames; no pass granted".to_string());
    }
    let pixels = cpu.len() / 3;
    let mut max_abs: u8 = 0;
    let mut total: u64 = 0;
    let mut within: usize = 0;
    for (cp, gp) in cpu.chunks_exact(3).zip(gpu.chunks_exact(3)) {
        let mut pixel_ok = true;
        for c in 0..3 {
            let d = cp[c].abs_diff(gp[c]);
            max_abs = max_abs.max(d);
            total += d as u64;
            if d > TOL_MAX_PER_CHANNEL_ABS {
                pixel_ok = false;
            }
        }
        if pixel_ok {
            within += 1;
        }
    }
    Ok(Comparison {
        max_abs,
        mean_abs: total as f64 / cpu.len() as f64,
        fraction_within: within as f64 / pixels as f64,
        pixels,
    })
}

impl Comparison {
    /// True only inside every stated bound (T013/T014).
    pub fn pass(&self) -> bool {
        self.max_abs <= TOL_MAX_PER_CHANNEL_ABS
            && self.mean_abs <= TOL_MEAN_ABS
            && self.fraction_within >= TOL_MIN_FRACTION_WITHIN
    }

    /// One-line measured distance, for reports and proof artifacts.
    pub fn distance(&self) -> String {
        format!(
            "max_abs={} (bound {}), mean_abs={:.4} (bound {:.4}), within={:.5} (bound {:.5}) over {} px",
            self.max_abs,
            TOL_MAX_PER_CHANNEL_ABS,
            self.mean_abs,
            TOL_MEAN_ABS,
            self.fraction_within,
            TOL_MIN_FRACTION_WITHIN,
            self.pixels,
        )
    }

    /// Verdict line naming the fixture, the frame, and the measured
    /// distance (REC-003): a failure is a product defect recorded with
    /// no pass granted for that fixture.
    pub fn report(&self, fixture: &str, frame: u64) -> String {
        if self.pass() {
            format!("fixture `{fixture}` frame {frame}: PASS ({})", self.distance())
        } else {
            format!(
                "fixture `{fixture}` frame {frame}: FAIL — outside tolerance ({}); defect belongs to the renderer, no pass granted",
                self.distance()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::describe;
    use crate::motion_plan::plan_motions;
    use notation::model::{
        Align, BoxClip, Composition, FontStyle, Fps, Rgb, TextClip, Wrap,
    };

    fn composition() -> Composition {
        Composition {
            name: "t".to_string(),
            width: 320,
            height: 240,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 1, g: 2, b: 3 },
            version: 1,
            clips: vec![
                notation::model::Clip::Text(TextClip {
                    value: "Hi".to_string(),
                    start: 0.0,
                    duration: 2.0,
                    x: 80.0,
                    y: 10.0,
                    size: 24.0,
                    color: Rgb { r: 255, g: 255, b: 255 },
                    align: Align::Left,
                    wrap: Wrap::Off,
                    style: FontStyle::Regular,
                    order: 2,
                    line_height: Some(30.0),
                    letter_spacing: Some(1.5),
                    font_family: None,
                    line: 3,
                    column: 1,
                }),
                notation::model::Clip::Box(BoxClip {
                    start: 0.0,
                    duration: 2.0,
                    x: 5.0,
                    y: 6.0,
                    width: 40.0,
                    height: 30.0,
                    fill: Rgb { r: 255, g: 0, b: 0 },
                    radius: 0.0,
                    border_width: 0.0,
                    border_color: Rgb { r: 0, g: 0, b: 0 },
                    opacity: 0.5,
                    order: 1,
                    line: 2,
                    column: 1,
                }),
            ],
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
        }
    }

    #[test]
    fn both_sides_fingerprint_the_identical_description() {
        let comp = composition();
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 15);
        // Twice from the same frame time: identical fingerprints.
        assert_eq!(consume(&desc), consume(&describe(&comp, &motions, 15)));
        let fp = consume(&desc);
        assert!(fp.contains("frame=15"), "{fp}");
        assert!(fp.contains("box#1"), "{fp}");
        assert!(fp.contains("text#2") || fp.contains("text#0"), "{fp}");
        // Paint order is visible: the box (order 1) precedes the text.
        assert!(fp.find("box#").unwrap() < fp.find("text#").unwrap(), "{fp}");
    }

    #[test]
    fn different_instants_fingerprint_differently() {
        let comp = composition();
        let motions = plan_motions(&comp, 30);
        let a = consume(&describe(&comp, &motions, 0));
        let b = consume(&describe(&comp, &motions, 29));
        assert_ne!(a, b);
        assert!(a.contains("frame=0") && b.contains("frame=29"), "{a} vs {b}");
    }

    #[test]
    fn motion_values_fingerprint_with_the_description() {
        // T009–T012: the animated values ride the GPU picture so the
        // twin-renderer check compares motion, not just stills.
        use notation::model::{EaseDecl, SetDecl, TweenDecl, TweenKind};
        let mut comp = composition();
        comp.tweens.push(TweenDecl {
            target: 1,
            property: "x".to_string(),
            kind: TweenKind::To,
            from: None,
            to: Some(105.0),
            start: 0.0,
            duration: 1.0,
            ease: EaseDecl::Linear,
            line: 1,
            column: 1,
        });
        comp.sets.push(SetDecl {
            target: 1,
            property: "opacity".to_string(),
            value: 0.25,
            start: 0.0,
            duration: 1.0,
            line: 1,
            column: 1,
        });
        let motions = plan_motions(&comp, 30);
        let still = consume(&describe(&comp, &motions, 0));
        let mid = consume(&describe(&comp, &motions, 15));
        assert_ne!(still, mid, "tweened frames fingerprint differently");
        assert!(mid.contains("x=55.000000"), "{mid}");
        assert!(mid.contains("opacity=0.250000"), "{mid}");
        assert!(mid.contains("scale=1.000000"), "{mid}");
        assert!(mid.contains("rot=0"), "{mid}");
    }

    // T013–T015 picture tests: no media, no fonts on disk, no FFmpeg
    // calls. `prepare` touches FFmpeg only for video/image/font-emoji
    // inputs, none of which these compositions use.
    use crate::render::prepare;
    use notation::model::Clip;
    use std::path::Path;

    fn still_comp(clips: Vec<Clip>) -> Composition {
        Composition {
            name: "t".to_string(),
            width: 64,
            height: 64,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips,
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
        }
    }

    fn red_box(x: f64, y: f64, order: i64) -> Clip {
        Clip::Box(BoxClip {
            start: 0.0,
            duration: 2.0,
            x,
            y,
            width: 20.0,
            height: 20.0,
            fill: Rgb { r: 255, g: 0, b: 0 },
            radius: 4.0,
            border_width: 2.0,
            border_color: Rgb { r: 0, g: 0, b: 255 },
            opacity: 1.0,
            order,
            line: 1,
            column: 1,
        })
    }

    /// CPU and GPU pictures of the same frame, compared with the stated
    /// tolerance; asserts the verdict passes.
    fn assert_agree(prep: &mut crate::render::Prepared, fixture: &str, frame: u64) {
        let cpu = prep.cpu_frame(frame).expect("cpu frame draws");
        let gpu = render_frame(prep, frame).expect("gpu frame draws");
        let cmp = compare_frames(&cpu, &gpu).expect("pictures compare");
        assert!(cmp.pass(), "{}", cmp.report(fixture, frame));
    }

    #[test]
    fn still_boxes_agree_within_tolerance_and_reruns_are_identical() {
        // T013: solid, bordered/rounded, and half-transparent boxes.
        let comp = still_comp(vec![
            red_box(4.0, 4.0, 1),
            Clip::Box(BoxClip {
                start: 0.0,
                duration: 2.0,
                x: 30.0,
                y: 30.0,
                width: 24.0,
                height: 24.0,
                fill: Rgb { r: 0, g: 255, b: 0 },
                radius: 6.0,
                border_width: 3.0,
                border_color: Rgb { r: 255, g: 255, b: 255 },
                opacity: 0.5,
                order: 2,
                line: 2,
                column: 1,
            }),
        ]);
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        for k in [0u64, 15, 59] {
            assert_agree(&mut prep, "still-boxes", k);
        }
        // CPU reruns stay byte-identical rather than merely within
        // tolerance (T013: only the CPU-to-GPU comparison uses it).
        let a = prep.cpu_frame(15).expect("cpu frame");
        let b = prep.cpu_frame(15).expect("cpu rerun");
        assert_eq!(a, b, "cpu reruns are byte-identical");
    }

    #[test]
    fn written_paint_order_is_shared_by_both_traversals() {
        // T013 via T006: the blue box is documented first but painted
        // second, so it must cover the red one on both renderers.
        let comp = still_comp(vec![
            Clip::Box(BoxClip {
                start: 0.0,
                duration: 2.0,
                x: 10.0,
                y: 10.0,
                width: 30.0,
                height: 30.0,
                fill: Rgb { r: 0, g: 0, b: 255 },
                radius: 0.0,
                border_width: 0.0,
                border_color: Rgb { r: 0, g: 0, b: 0 },
                opacity: 1.0,
                order: 2,
                line: 1,
                column: 1,
            }),
            red_box(20.0, 20.0, 1),
        ]);
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        assert_agree(&mut prep, "paint-order", 0);
        for (label, pic) in [
            ("cpu", prep.cpu_frame(0).expect("cpu")),
            ("gpu", render_frame(&mut prep, 0).expect("gpu")),
        ] {
            // Overlap pixel (25, 25): blue (order 2) wins over red.
            let o = (25 * 64 + 25) as usize * 3;
            assert_eq!(
                (pic[o], pic[o + 1], pic[o + 2]),
                (0, 0, 255),
                "{label} paints written order second on top"
            );
        }
    }

    #[test]
    fn spaced_text_agrees_within_tolerance() {
        // T013 via T007/T008: multi-line text with declared line height
        // and letter spacing. Both sides share the same prepared
        // layouts, so the assertion holds on any machine font set.
        let comp = still_comp(vec![Clip::Text(TextClip {
            value: "Hi Ag".to_string(),
            start: 0.0,
            duration: 2.0,
            x: 4.0,
            y: 4.0,
            size: 12.0,
            color: Rgb { r: 255, g: 255, b: 255 },
            align: Align::Left,
            wrap: Wrap::Off,
            style: FontStyle::Regular,
            order: 1,
            line_height: Some(20.0),
            letter_spacing: Some(3.0),
            font_family: None,
            line: 1,
            column: 1,
        })]);
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        assert_agree(&mut prep, "spaced-text", 0);
        assert_agree(&mut prep, "spaced-text", 30);
    }

    #[test]
    fn motion_frames_agree_within_tolerance() {
        // T014: a to-tween on x plus a set hold on opacity; the mid
        // frame must show motion on both sides, still inside tolerance.
        use notation::model::{EaseDecl, SetDecl, TweenDecl, TweenKind};
        let mut comp = still_comp(vec![red_box(4.0, 4.0, 1)]);
        comp.tweens.push(TweenDecl {
            target: 0,
            property: "x".to_string(),
            kind: TweenKind::To,
            from: None,
            to: Some(44.0),
            start: 0.0,
            duration: 1.0,
            ease: EaseDecl::Linear,
            line: 1,
            column: 1,
        });
        comp.sets.push(SetDecl {
            target: 0,
            property: "opacity".to_string(),
            value: 0.5,
            start: 0.0,
            duration: 0.5,
            line: 2,
            column: 1,
        });
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        for k in [0u64, 15, 29, 59] {
            assert_agree(&mut prep, "motion-box", k);
        }
        // The tween visibly moves the box on both sides.
        let y = 14usize;
        let row_red = |pic: &[u8], k: u64| -> usize {
            let _ = k;
            pic.chunks_exact(3)
                .skip(y * 64)
                .take(64)
                .filter(|p| p[0] > 150 && p[1] < 100 && p[2] < 100)
                .count()
        };
        let cpu0 = prep.cpu_frame(0).expect("cpu 0");
        let cpu29 = prep.cpu_frame(29).expect("cpu 29");
        let gpu29 = render_frame(&mut prep, 29).expect("gpu 29");
        assert_ne!(row_red(&cpu0, 0), row_red(&cpu29, 29), "cpu shows the tween");
        assert_ne!(row_red(&cpu0, 0), row_red(&gpu29, 29), "gpu shows the tween");
    }

    #[test]
    fn mistuned_picture_is_reported_with_fixture_frame_and_distance() {
        // REC-003: a deliberately mistuned picture must fail with the
        // fixture, the frame, and the measured distance — never a pass.
        let comp = still_comp(vec![red_box(4.0, 4.0, 1)]);
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        let cpu = prep.cpu_frame(7).expect("cpu");
        let mut bad = cpu.clone();
        for i in (0..bad.len()).step_by(97) {
            bad[i] = bad[i].saturating_add(50);
        }
        let cmp = compare_frames(&cpu, &bad).expect("pictures compare");
        assert!(!cmp.pass(), "mistuned picture must not pass");
        let report = cmp.report("still-boxes", 7);
        assert!(report.contains("still-boxes"), "{report}");
        assert!(report.contains("frame 7"), "{report}");
        assert!(report.contains("max_abs="), "{report}");
        assert!(report.contains("no pass granted"), "{report}");
    }

    #[test]
    fn size_mismatch_is_refused_never_passed() {
        let err = compare_frames(&[0u8; 12], &[0u8; 6]).expect_err("must refuse");
        assert!(err.contains("no pass granted"), "{err}");
    }

    #[test]
    fn quantize_keeps_representable_values_exact() {
        assert_eq!(quantize(10.0), 10.0);
        assert_eq!(quantize(0.5), 0.5);
        assert_eq!(quantize(0.0), 0.0);
        assert!(quantize(f64::INFINITY).is_infinite());
        // A non-representable motion value stays within one float ulp.
        let v = 0.1f64;
        assert!((quantize(v) - v).abs() < 1e-7, "{}", quantize(v));
    }
}
