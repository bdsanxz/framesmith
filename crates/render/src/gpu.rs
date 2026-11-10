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
    FrameDescription, FrameItem, ResolvedBox, ResolvedImage, ResolvedLottie, ResolvedPath,
    ResolvedScene3d, ResolvedText, ResolvedVideo,
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
    // Sprint 8 look (T003): the fill paint rides the fingerprint, so
    // a gradient or image fill never aliases a solid or another fill.
    fn fill_paint_desc(paint: &notation::model::FillPaint) -> String {
        use notation::model::FillPaint;
        match paint {
            FillPaint::Solid => "solid".to_string(),
            FillPaint::Linear { angle, stops } => {
                let stops: Vec<String> = stops
                    .iter()
                    .map(|s| {
                        format!(
                            "{}:{:02x}{:02x}{:02x}",
                            num(s.offset),
                            s.color.r,
                            s.color.g,
                            s.color.b
                        )
                    })
                    .collect();
                format!("linear({angle},{})", stops.join(","))
            }
            FillPaint::Radial {
                cx,
                cy,
                radius,
                stops,
            } => {
                let stops: Vec<String> = stops
                    .iter()
                    .map(|s| {
                        format!(
                            "{}:{:02x}{:02x}{:02x}",
                            num(s.offset),
                            s.color.r,
                            s.color.g,
                            s.color.b
                        )
                    })
                    .collect();
                format!(
                    "radial({},{},{},{})",
                    cx.map(num).unwrap_or_else(|| "-".to_string()),
                    cy.map(num).unwrap_or_else(|| "-".to_string()),
                    radius.map(num).unwrap_or_else(|| "-".to_string()),
                    stops.join(",")
                )
            }
            FillPaint::Image {
                src,
                fit,
                placement,
            } => format!("image({src:?},{fit:?},{:?}/{:?})", placement.x, placement.y),
        }
    }
    let mut out = String::new();
    let _ = write!(
        out,
        "frame={} time={} canvas={}x{} bg={} cg={} items={} [",
        desc.frame,
        desc.time.to_exact(),
        desc.width,
        desc.height,
        hex((desc.background.r, desc.background.g, desc.background.b)),
        desc.composition_grade.fingerprint(),
        desc.items.len()
    );
    for item in &desc.items {
        match item {
            FrameItem::Text(r) => {
                let _ = write!(
                    out,
                    "text#{} order={} x={} y={} size={} color={} lh={} ls={} scale={} rot={} op={} rx={} ry={} blur={} pers={} org={:?} g={};",
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
                    num(r.rotate_x),
                    num(r.rotate_y),
                    num(r.blur),
                    r.turn.perspective.map(num).unwrap_or_else(|| "-".to_string()),
                    r.turn.origin,
                    desc.grade_for(r.doc).fingerprint(),
                );
            }
            FrameItem::Video(r) => {
                let _ = write!(
                    out,
                    "video#{} order={} src_t={} fit={:?} x={} y={} scale={} rot={} op={} rx={} ry={} blur={} pers={} org={:?} g={};",
                    r.doc,
                    r.order,
                    num(r.src_t),
                    r.fit,
                    num(r.x),
                    num(r.y),
                    num(r.scale),
                    num(r.rotation),
                    num(r.opacity),
                    num(r.rotate_x),
                    num(r.rotate_y),
                    num(r.blur),
                    r.turn.perspective.map(num).unwrap_or_else(|| "-".to_string()),
                    r.turn.origin,
                    desc.grade_for(r.doc).fingerprint(),
                );
            }
            // Sprint 21 T001 Lottie (shape owned by the Lottie slice,
            // which refines it): the identity fields mirror the
            // video arm so seeks fingerprint deterministically.
            FrameItem::Lottie(r) => {
                let _ = write!(
                    out,
                    "lottie#{} order={} {:?} src_t={} loop={:?} x={} y={} w={} h={} fit={:?} place={:?}/{:?} scale={} rot={} op={} rx={} ry={} blur={} pers={} org={:?} g={};",
                    r.doc,
                    r.order,
                    r.path,
                    num(r.src_t),
                    r.loop_mode,
                    num(r.x),
                    num(r.y),
                    num(r.width),
                    num(r.height),
                    r.fit,
                    r.placement.x,
                    r.placement.y,
                    num(r.scale),
                    num(r.rotation),
                    num(r.opacity),
                    num(r.rotate_x),
                    num(r.rotate_y),
                    num(r.blur),
                    r.turn.perspective.map(num).unwrap_or_else(|| "-".to_string()),
                    r.turn.origin,
                    desc.grade_for(r.doc).fingerprint(),
                );
            }
            // Sprint 21 T003 scene (shape owned by the 3D slice): the
            // clip-local instant plus the full authored scene ride
            // the fingerprint, so seeks and edits verify
            // deterministically.
            FrameItem::Scene3d(r) => {
                let _ = write!(
                    out,
                    "scene3d#{} order={} local_t={} x={} y={} w={} h={} fit={:?} place={:?}/{:?} scale={} rot={} op={} rx={} ry={} blur={} pers={} org={:?} cam={:?}|{:?}|{} objs=[{}] lights=[{}] g={};",
                    r.doc,
                    r.order,
                    num(r.local_t),
                    num(r.x),
                    num(r.y),
                    num(r.width),
                    num(r.height),
                    r.fit,
                    r.placement.x,
                    r.placement.y,
                    num(r.scale),
                    num(r.rotation),
                    num(r.opacity),
                    num(r.rotate_x),
                    num(r.rotate_y),
                    num(r.blur),
                    r.turn.perspective.map(num).unwrap_or_else(|| "-".to_string()),
                    r.turn.origin,
                    r.camera.position,
                    r.camera.target,
                    num(r.camera.fov_deg),
                    r.objects
                        .iter()
                        .map(|o| format!(
                            "{:?}:{}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
                            o.name,
                            o.kind.name(),
                            o.position,
                            o.rotation,
                            o.size,
                            (o.radius, o.height, o.segments),
                            (o.color.r, o.color.g, o.color.b),
                            (o.metalness, o.roughness),
                            o.vertices,
                            o.faces,
                        ))
                        .collect::<Vec<_>>()
                        .join(";"),
                    r.lights
                        .iter()
                        .map(|l| format!(
                            "{:?}:{}:{:?}:{:?}:{:?}:{:?}",
                            l.name,
                            l.kind.name(),
                            l.position,
                            l.direction,
                            (l.color.r, l.color.g, l.color.b),
                            l.intensity,
                        ))
                        .collect::<Vec<_>>()
                        .join(";"),
                    desc.grade_for(r.doc).fingerprint(),
                );
            }
            FrameItem::Box(r) => {
                let _ = write!(
                    out,
                    "box#{} order={} x={} y={} {}x{} fill={} paint={} radius={} border={}:{} stroke={} shadow={} blend={} backdrop={}/{} overflow={} mask={} opacity={} scale={} rot={} rx={} ry={} blur={} pers={} org={:?} g={};",
                    r.doc,
                    r.order,
                    num(r.x),
                    num(r.y),
                    num(r.width),
                    num(r.height),
                    hex((r.fill.r, r.fill.g, r.fill.b)),
                    fill_paint_desc(&r.fill_paint),
                    num(r.radius),
                    num(r.border_width),
                    hex((r.border_color.r, r.border_color.g, r.border_color.b)),
                    r.stroke_position.name(),
                    r.box_shadow
                        .map(|s| format!(
                            "{}:{}:{}:{}:{}",
                            num(s.offset_x),
                            num(s.offset_y),
                            num(s.blur),
                            num(s.spread),
                            hex((s.color.r, s.color.g, s.color.b))
                        ))
                        .unwrap_or_else(|| "-".to_string()),
                    r.blend_mode.name(),
                    num(r.backdrop_blur),
                    num(r.backdrop_brightness),
                    r.overflow.name(),
                    match (&r.mask, &r.clip_path) {
                        (None, None) => "-".to_string(),
                        (m, c) => format!(
                            "{}:{}:{}:{}:{}|{}",
                            m.map(|s| num(s.x)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.y)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.width)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.height)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.radius)).unwrap_or_else(|| "-".to_string()),
                            c.map(|s| num(s.x)).unwrap_or_else(|| "-".to_string()),
                        ),
                    },
                    num(r.opacity),
                    num(r.scale),
                    num(r.rotation),
                    num(r.rotate_x),
                    num(r.rotate_y),
                    num(r.blur),
                    r.turn.perspective.map(num).unwrap_or_else(|| "-".to_string()),
                    r.turn.origin,
                    desc.grade_for(r.doc).fingerprint(),
                );
            }
            FrameItem::Path(r) => {
                let _ = write!(
                    out,
                    "path#{} order={} {:?} x={} y={} rule={} fill={} stroke={}:{}:{} shadow={} blend={} backdrop={}/{} overflow={} mask={} opacity={} scale={} rot={} rx={} ry={} blur={} pers={} org={:?} g={};",
                    r.doc,
                    r.order,
                    r.d,
                    num(r.x),
                    num(r.y),
                    r.fill_rule.name(),
                    hex((r.fill.r, r.fill.g, r.fill.b)),
                    num(r.border_width),
                    hex((r.border_color.r, r.border_color.g, r.border_color.b)),
                    r.stroke_position.name(),
                    r.box_shadow
                        .map(|s| format!(
                            "{}:{}:{}:{}:{}",
                            num(s.offset_x),
                            num(s.offset_y),
                            num(s.blur),
                            num(s.spread),
                            hex((s.color.r, s.color.g, s.color.b))
                        ))
                        .unwrap_or_else(|| "-".to_string()),
                    r.blend_mode.name(),
                    num(r.backdrop_blur),
                    num(r.backdrop_brightness),
                    r.overflow.name(),
                    match (&r.mask, &r.clip_path) {
                        (None, None) => "-".to_string(),
                        (m, c) => format!(
                            "{}:{}:{}:{}:{}|{}",
                            m.map(|s| num(s.x)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.y)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.width)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.height)).unwrap_or_else(|| "-".to_string()),
                            m.map(|s| num(s.radius)).unwrap_or_else(|| "-".to_string()),
                            c.map(|s| num(s.x)).unwrap_or_else(|| "-".to_string()),
                        ),
                    },
                    num(r.opacity),
                    num(r.scale),
                    num(r.rotation),
                    num(r.rotate_x),
                    num(r.rotate_y),
                    num(r.blur),
                    r.turn.perspective.map(num).unwrap_or_else(|| "-".to_string()),
                    r.turn.origin,
                    desc.grade_for(r.doc).fingerprint(),
                );
            }
            FrameItem::Image(r) => {
                let _ = write!(
                    out,
                    "image#{} order={} {:?} x={} y={} w={} h={} fit={:?} place={:?}/{:?} scale={} rot={} op={} rx={} ry={} blur={} pers={} org={:?} g={};",
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
                    num(r.rotate_x),
                    num(r.rotate_y),
                    num(r.blur),
                    r.turn.perspective.map(num).unwrap_or_else(|| "-".to_string()),
                    r.turn.origin,
                    desc.grade_for(r.doc).fingerprint(),
                );
            }
            FrameItem::Caption(r) => {
                // Sprint 22 captions: the cue, its clip-local time,
                // its words, and the resolved look fingerprint the
                // same pixels the painter draws on both backends.
                let words = r
                    .words
                    .iter()
                    .map(|w| format!("{:?}[{},{}]", w.text, num(w.start), num(w.end)))
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = write!(
                    out,
                    "captions#{} order={} cue={} t={} x={} y={} size={} op={} style={}/{}/{}/{}/{} presets={}/{}/{} matte={:?} behind={} words=[{}] g={};",
                    r.doc,
                    r.order,
                    r.cue,
                    num(r.local_t),
                    num(r.x),
                    num(r.y),
                    num(r.size),
                    num(r.opacity),
                    hex((r.style.color.r, r.style.color.g, r.style.color.b)),
                    num(r.style.font_size),
                    num(r.style.font_weight),
                    r.style.position.name(),
                    r.style
                        .background
                        .map(|c| hex((c.r, c.g, c.b)))
                        .unwrap_or_else(|| "-".to_string()),
                    r.entrance.as_deref().unwrap_or("none"),
                    r.highlight.as_deref().unwrap_or("none"),
                    r.exit.as_deref().unwrap_or("none"),
                    r.matte,
                    r.behind_subject,
                    words,
                    desc.grade_for(r.doc).fingerprint(),
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
    let view = prep.nest_view();
    let at = prep.range_start + crate::motion_plan::frame_time(&prep.composition().fps, frame);
    let desc = crate::frame::describe_nested_at(&view, 0, frame, at);
    let active = crate::transitions::active_transitions(prep.composition(), at);
    if active.is_empty() {
        return render_description(prep, &desc);
    }
    // Bridged frame (Sprint 11 transitions, CAP-005/T006): describe
    // the base plus every bridge pair first (owned descriptions, so
    // the tree borrow ends here), then draw through the GPU split
    // path like an ordinary frame.
    let planned = crate::render::plan_transition_frame(&view, 0, frame, &desc, &active);
    let Prepared {
        videos,
        ranges,
        images,
        lotties,
        layouts,
        dyn_layouts,
        caption_layouts,
        bitmap_fallback,
        raster,
        paths,
        folder,
        ..
    } = prep;
    crate::render::draw_planned_transition(
        videos,
        ranges,
        images,
        lotties,
        layouts,
        dyn_layouts,
        caption_layouts,
        *bitmap_fallback,
        raster,
        paths,
        folder.as_path(),
        crate::render::Backend::Gpu,
        &planned,
    )
}

/// Draw one frame description on the GPU picture path (T013/T014).
/// The traversal is the GPU side's own: background fill, written-order
/// dispatch, and per-item picture lookup with `gpu render failed`
/// diagnostics. Item painters are the verified raster primitives shared
/// with the CPU blitter, fed with [`quantize`]-quantized motion values,
/// so the comparison measures the backend gap rather than a second
/// drawing algorithm.
pub fn render_description(prep: &mut Prepared, desc: &FrameDescription) -> Result<Vec<u8>, String> {
    // Disjoint field borrows: the media stays shared while the raster
    // cache goes to the traversal mutably.
    let Prepared {
        videos,
        ranges,
        images,
        lotties,
        layouts,
        dyn_layouts,
        caption_layouts,
        bitmap_fallback,
        raster,
        paths,
        folder,
        ..
    } = prep;
    render_description_split(
        videos,
        ranges,
        images,
        lotties,
        layouts,
        dyn_layouts,
        caption_layouts,
        *bitmap_fallback,
        raster,
        paths,
        folder.as_path(),
        desc,
    )
}

/// Split-borrow core of [`render_description`]: the same traversal over
/// shared media plus a caller-owned raster cache, so Sprint 3 parallel
/// workers (T003) can draw GPU pictures from shared borrows with
/// thread-local caches and stay byte-identical at any worker count.
pub(crate) fn render_description_split(
    videos: &[crate::render::ProbedVideo],
    ranges: &[crate::render::DecodedRange],
    images: &[crate::render::DecodedImage],
    lotties: &[crate::render::PreparedLottie],
    layouts: &[Option<layout::Block>],
    dyn_layouts: &crate::render::DynLayouts,
    captions: &crate::captions::CaptionLayouts,
    bitmap_fallback: bool,
    raster: &mut crate::render::RasterCache,
    paths: &crate::path_paint::PathCache,
    folder: &std::path::Path,
    desc: &FrameDescription,
) -> Result<Vec<u8>, String> {
    use crate::render::fill_graded_background;
    use crate::render::{
        blit_dest, blit_fit, blur_on, draw_bitmap_advanced, draw_bitmap_text_item,
        draw_bitmap_text_layer, draw_box, draw_box_advanced, draw_image, draw_image_advanced,
        draw_text_advanced, draw_text_block, draw_video_advanced, item_turn, layer_ink_bbox,
        pick_picture_or_hold, turn_probe, wrap_loop_time,
    };
    let (cw, chh) = (desc.width, desc.height);
    let mut frame = vec![0u8; cw as usize * chh as usize * 3];
    // LUT grids resolve once per frame from disk (Sprint 11 grade,
    // CAP-002/T003), exactly like the CPU blitter: the freshness is
    // the mid-render vanish check.
    let mut luts = crate::render::LutFrameCache::new();
    // Background fill, carrying the composition-wide grade alone
    // (Sprint 11 grade, T001+T002), exactly like the CPU blitter:
    // spatial grades paint per pixel through the shared helper.
    let comp_grade = luts.grade(folder, &desc.composition_grade)?;
    fill_graded_background(
        &mut frame,
        desc.width,
        desc.height,
        desc.background,
        &comp_grade,
    );
    for item in &desc.items {
        match item {
            FrameItem::Video(v) => {
                // Inward-out look for this clip (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(v.doc))?;
                let effects = desc.effects_for(v.doc);
                let vfx = desc.vfx_for(v.doc);
                let maps = crate::render::resolve_vfx_maps(images, folder, vfx)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                let vi = videos
                    .iter()
                    .position(|p| p.clip_idx == v.doc)
                    .ok_or_else(|| {
                        format!(
                            "gpu render failed: video clip #{} was not probed (decode stage)",
                            v.doc
                        )
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
                    (
                        range.width,
                        range.height,
                        range.frames[j * fs..(j + 1) * fs].to_vec(),
                    )
                };
                // Sprint 7 motion (T005/T006), same branch as the CPU
                // blitter: blurred or turned video draws through the
                // layer path. Motion values quantize exactly as the
                // blit does; blur and the turn ride through exact.
                let qv = ResolvedVideo {
                    x: quantize(v.x),
                    y: quantize(v.y),
                    scale: quantize(v.scale),
                    rotation: quantize(v.rotation),
                    opacity: quantize(v.opacity),
                    ..v.clone()
                };
                let card = blit_dest(cw, chh, sw, sh, v.fit, qv.scale, qv.x, qv.y)
                    .map(|(dx, dy, dw, dh)| (dx, dy, dx + dw, dy + dh));
                let turn = card.and_then(|c| item_turn(qv.rotate_x, qv.rotate_y, &qv.turn, c));
                if blur_on(qv.blur)
                    || turn.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_video_advanced(
                        &mut frame, cw, chh, &qv, &pic, sw, sh, turn, &grade, effects, vfx, &maps,
                    );
                } else {
                    blit_fit(
                        &mut frame,
                        cw,
                        chh,
                        &pic,
                        sw,
                        sh,
                        v.fit,
                        qv.x,
                        qv.y,
                        qv.scale,
                        qv.rotation,
                        qv.opacity,
                    );
                }
            }
            FrameItem::Text(tc) => {
                // Inward-out look for this clip (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(tc.doc))?;
                let effects = desc.effects_for(tc.doc);
                let vfx = desc.vfx_for(tc.doc);
                let maps = crate::render::resolve_vfx_maps(images, folder, vfx)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                if tc.value.is_empty() && !tc.has_spans {
                    // A typewriter reveal at progress 0 shows no pieces:
                    // nothing to draw at this instant. A value-less span
                    // text still paints its shaped runs (Sprint 8, T002).
                    continue;
                }
                let q = ResolvedText {
                    scale: quantize(tc.scale),
                    rotation: quantize(tc.rotation),
                    opacity: quantize(tc.opacity),
                    x: quantize(tc.x),
                    y: quantize(tc.y),
                    ..tc.clone()
                };
                // Frame-varying text (typewriter, count, animated
                // metrics) draws its pre-shaped string; still text
                // draws its static layout. The shaped block is looked
                // up at the exact evaluated metrics (unquantized: the
                // block was shaped at those bits).
                let block: Option<layout::Block> = dyn_layouts
                    .get(q.doc, &q.value, q.size, q.letter_spacing)
                    .cloned()
                    .or_else(|| layouts.get(q.doc).and_then(|laid| laid.clone()));
                match block {
                    Some(block) => {
                        let card = (
                            q.x,
                            q.y,
                            q.x + block.width as f64 * q.scale,
                            q.y + block.height as f64 * q.scale,
                        );
                        let turn = item_turn(q.rotate_x, q.rotate_y, &q.turn, card);
                        if blur_on(q.blur)
                            || turn.is_some()
                            || !grade.is_empty()
                            || !effects.is_empty()
                            || !vfx.is_empty()
                        {
                            draw_text_advanced(
                                &mut frame, cw, chh, raster, &block, &q, turn, card, &grade,
                                effects, vfx, &maps,
                            )
                            .map_err(|e| format!("gpu render failed: {e}"))?;
                        } else {
                            draw_text_block(&mut frame, cw, chh, raster, &block, &q)
                                .map_err(|e| format!("gpu render failed: {e}"))?;
                        }
                    }
                    None if bitmap_fallback => {
                        let probe = turn_probe(q.rotate_x, q.rotate_y, q.turn.perspective);
                        if !(blur_on(q.blur)
                            || probe.is_some()
                            || !grade.is_empty()
                            || !effects.is_empty()
                            || !vfx.is_empty())
                        {
                            draw_bitmap_text_item(&mut frame, cw, chh, &q);
                        } else if q.scale <= 0.0 || !q.scale.is_finite() || q.opacity <= 0.0 {
                            // Nothing draws, exactly like the CPU path.
                        } else {
                            let mut layer = vec![0u8; cw as usize * chh as usize * 4];
                            draw_bitmap_text_layer(
                                &mut layer,
                                cw,
                                chh,
                                &q,
                                q.scale,
                                q.opacity.clamp(0.0, 1.0),
                            );
                            if let Some((x0, y0, x1, y1)) = layer_ink_bbox(&layer, cw, chh) {
                                let card = (x0 as f64, y0 as f64, x1 as f64 + 1.0, y1 as f64 + 1.0);
                                let turn = item_turn(q.rotate_x, q.rotate_y, &q.turn, card);
                                draw_bitmap_advanced(
                                    &mut frame, cw, chh, &layer, &q, turn, card, &grade, effects,
                                    vfx, &maps,
                                );
                            }
                        }
                    }
                    None => {
                        return Err("gpu render failed: text layout is missing and no bitmap fallback applies".to_string());
                    }
                }
            }
            FrameItem::Box(b) => {
                // Inward-out look for this box (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(b.doc))?;
                let effects = desc.effects_for(b.doc);
                let vfx = desc.vfx_for(b.doc);
                let maps = crate::render::resolve_vfx_maps(images, folder, vfx)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
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
                // Sprint 8 look (T004): same paint-stack branch as the
                // CPU blitter, so both traversals paint the same
                // stacking. Sprint 7 motion (T005/T006), same branch
                // as the CPU blitter; blur and the turn ride through
                // exact. Sprint 8 fills (T003) resolve through the
                // folder with the vanish check, exactly like the CPU
                // path.
                let probe = turn_probe(q.rotate_x, q.rotate_y, q.turn.perspective);
                if q.paint_stack_active() {
                    crate::paint::draw_box_paint(
                        &mut frame, cw, chh, &q, images, folder, &grade, effects, vfx, &maps,
                    )
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                } else if blur_on(q.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_box_advanced(
                        &mut frame, cw, chh, &q, images, folder, &grade, effects, vfx, &maps,
                    )
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                } else {
                    draw_box(&mut frame, cw, chh, &q, images, folder)
                        .map_err(|e| format!("gpu render failed: {e}"))?;
                }
            }
            FrameItem::Path(q) => {
                // Inward-out look for this path (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(q.doc))?;
                let effects = desc.effects_for(q.doc);
                let vfx = desc.vfx_for(q.doc);
                let maps = crate::render::resolve_vfx_maps(images, folder, vfx)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                let r = ResolvedPath {
                    x: quantize(q.x),
                    y: quantize(q.y),
                    opacity: quantize(q.opacity),
                    scale: quantize(q.scale),
                    rotation: quantize(q.rotation),
                    ..q.clone()
                };
                // Sprint 8 look (T005): the single path painter serves
                // both traversals, so CPU and GPU agree pixel for pixel.
                crate::path_paint::draw_path(
                    &mut frame, cw, chh, &r, paths, &grade, effects, vfx, &maps,
                )
                .map_err(|e| format!("gpu render failed: {e}"))?;
            }
            FrameItem::Image(im) => {
                // Inward-out look for this image (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(im.doc))?;
                let effects = desc.effects_for(im.doc);
                let vfx = desc.vfx_for(im.doc);
                let maps = crate::render::resolve_vfx_maps(images, folder, vfx)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                let decoded = images
                    .iter()
                    .find(|d| d.path.ends_with(&im.path))
                    .ok_or_else(|| {
                        format!(
                            "gpu render failed: image source {:?} was not decoded (decode stage)",
                            im.path
                        )
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
                // Sprint 7 motion (T005/T006), same branch as the CPU
                // blitter; blur and the turn ride through exact. A graded
                // image takes the layer path, exactly like the CPU side.
                let probe = turn_probe(q.rotate_x, q.rotate_y, q.turn.perspective);
                if blur_on(q.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_image_advanced(
                        &mut frame, cw, chh, &q, &decoded, &grade, effects, vfx, &maps,
                    );
                } else {
                    draw_image(&mut frame, cw, chh, &q, &decoded);
                }
            }
            FrameItem::Lottie(lo) => {
                // Sprint 21 adapters (T001): the GPU picture path
                // rasterizes the same prepared doc at the same
                // loop-mapped instant, then rides the shared image
                // path with quantized placement, exactly like images.
                let grade = luts.grade(folder, &desc.grade_for(lo.doc))?;
                let effects = desc.effects_for(lo.doc);
                let vfx = desc.vfx_for(lo.doc);
                let maps = crate::render::resolve_vfx_maps(images, folder, vfx)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                let prep = lotties
                    .iter()
                    .find(|d| d.clip_idx == lo.doc)
                    .ok_or_else(|| {
                        format!(
                            "gpu render failed: lottie source {:?} was not decoded (decode stage)",
                            lo.path
                        )
                    })?;
                let select_t = lottie::sample_time(prep.doc.len_secs(), lo.loop_mode, lo.src_t);
                let decoded = crate::lottie::rasterize(
                    &prep.doc,
                    select_t,
                    lo.width,
                    lo.height,
                    lo.fit,
                    lo.placement,
                    &lo.path,
                )
                .map_err(|e| format!("gpu render failed: {e}"))?;
                let q = ResolvedLottie {
                    x: quantize(lo.x),
                    y: quantize(lo.y),
                    scale: quantize(lo.scale),
                    rotation: quantize(lo.rotation),
                    opacity: quantize(lo.opacity),
                    ..lo.clone()
                };
                let im = ResolvedImage {
                    doc: q.doc,
                    order: q.order,
                    path: q.path.clone(),
                    x: q.x,
                    y: q.y,
                    width: Some(q.width),
                    height: Some(q.height),
                    fit: notation::model::Fit::Fill,
                    placement: notation::model::Placement::center(),
                    scale: q.scale,
                    rotation: q.rotation,
                    opacity: q.opacity,
                    rotate_x: q.rotate_x,
                    rotate_y: q.rotate_y,
                    blur: q.blur,
                    turn: q.turn,
                };
                let probe = turn_probe(im.rotate_x, im.rotate_y, im.turn.perspective);
                if blur_on(im.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_image_advanced(
                        &mut frame, cw, chh, &im, &decoded, &grade, effects, vfx, &maps,
                    );
                } else {
                    draw_image(&mut frame, cw, chh, &im, &decoded);
                }
            }
            FrameItem::Scene3d(s3) => {
                // Sprint 21 adapters (T003): the GPU picture path
                // rasterizes the same scene at the same clip-local
                // instant, then rides the shared image path with
                // quantized placement, exactly like images.
                let grade = luts.grade(folder, &desc.grade_for(s3.doc))?;
                let effects = desc.effects_for(s3.doc);
                let vfx = desc.vfx_for(s3.doc);
                let maps = crate::render::resolve_vfx_maps(images, folder, vfx)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
                let (rw, rh) = crate::scene3d::raster_px(s3.width, s3.height);
                let decoded = crate::scene3d::render_scene(
                    s3.doc,
                    &s3.objects,
                    &s3.camera,
                    &s3.lights,
                    rw,
                    rh,
                );
                let q = ResolvedScene3d {
                    x: quantize(s3.x),
                    y: quantize(s3.y),
                    scale: quantize(s3.scale),
                    rotation: quantize(s3.rotation),
                    opacity: quantize(s3.opacity),
                    ..s3.clone()
                };
                let im = ResolvedImage {
                    doc: q.doc,
                    order: q.order,
                    path: decoded.path.display().to_string(),
                    x: q.x,
                    y: q.y,
                    width: Some(q.width),
                    height: Some(q.height),
                    fit: notation::model::Fit::Fill,
                    placement: notation::model::Placement::center(),
                    scale: q.scale,
                    rotation: q.rotation,
                    opacity: q.opacity,
                    rotate_x: q.rotate_x,
                    rotate_y: q.rotate_y,
                    blur: q.blur,
                    turn: q.turn,
                };
                let probe = turn_probe(im.rotate_x, im.rotate_y, im.turn.perspective);
                if blur_on(im.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_image_advanced(
                        &mut frame, cw, chh, &im, &decoded, &grade, effects, vfx, &maps,
                    );
                } else {
                    draw_image(&mut frame, cw, chh, &im, &decoded);
                }
            }
            FrameItem::Caption(rc) => {
                // Sprint 22 captions: the GPU picture path draws the
                // same prepared word blocks through the same caption
                // painter, so both backends agree byte for byte. The
                // look resolves first for vanish parity with the CPU
                // blitter (a lost LUT refuses before any word paints).
                let _ = luts.grade(folder, &desc.grade_for(rc.doc))?;
                crate::captions::draw_caption(&mut frame, cw, chh, raster, captions, images, rc)
                    .map_err(|e| format!("gpu render failed: {e}"))?;
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
        return Err(
            "comparison refused: pictures are not whole RGB24 frames; no pass granted".to_string(),
        );
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
            format!(
                "fixture `{fixture}` frame {frame}: PASS ({})",
                self.distance()
            )
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
        Align, BoxClip, Composition, FontStyle, Fps, Rgb, TextClip, TrackKind, Wrap,
    };

    fn composition() -> Composition {
        Composition {
            line: 1,
            column: 1,
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 320,
            height: 240,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 1, g: 2, b: 3 },
            version: 1,
            clips: vec![
                notation::model::Clip::Text(TextClip {
                    asserts: Default::default(),
                    id: None,
                    track: TrackKind::Graphics,
                    label: None,
                    start_after: None,
                    hidden: false,
                    value: "Hi".to_string(),
                    start: 0.0,
                    duration: 2.0,
                    x: 80.0,
                    y: 10.0,
                    size: 24.0,
                    color: Rgb {
                        r: 255,
                        g: 255,
                        b: 255,
                    },
                    align: Align::Left,
                    wrap: Wrap::Off,
                    style: FontStyle::Regular,
                    order: 2,
                    line_height: Some(30.0),
                    letter_spacing: Some(1.5),
                    font_family: None,
                    direction: notation::model::TextDirection::Ltr,
                    spans: Vec::new(),
                    split: None,
                    typewriter: false,
                    count_from: None,
                    count_to: None,
                    decimals: 0,
                    line: 3,
                    column: 1,
                    turn: notation::model::Turn3D::flat(),
                    filter_blur: 0.0,
                    grade: notation::model::Grade::default(),
                    effects: Vec::new(),
                    vfx: Default::default(),
                }),
                notation::model::Clip::Box(BoxClip {
                    asserts: Default::default(),
                    id: None,
                    track: TrackKind::Graphics,
                    label: None,
                    start_after: None,
                    hidden: false,
                    start: 0.0,
                    duration: 2.0,
                    x: 5.0,
                    y: 6.0,
                    width: 40.0,
                    height: 30.0,
                    fill: Rgb { r: 255, g: 0, b: 0 },
                    fill_paint: notation::model::FillPaint::Solid,
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
                    opacity: 0.5,
                    order: 1,
                    line: 2,
                    column: 1,
                    turn: notation::model::Turn3D::flat(),
                    filter_blur: 0.0,
                    grade: notation::model::Grade::default(),
                    effects: Vec::new(),
                    vfx: Default::default(),
                }),
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
            transitions: Vec::new(),
            motion_blur: None,
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
        assert!(
            a.contains("frame=0") && b.contains("frame=29"),
            "{a} vs {b}"
        );
    }

    #[test]
    fn motion_values_fingerprint_with_the_description() {
        // T009–T012: the animated values ride the GPU picture so the
        // twin-renderer check compares motion, not just stills.
        use notation::model::{EaseDecl, SetDecl, StaggerOrigin, TweenDecl, TweenKind};
        let mut comp = composition();
        comp.tweens.push(TweenDecl {
            target: 1,
            property: "x".to_string(),
            kind: TweenKind::To,
            from: None,
            to: Some(105.0),
            path: None,
            path_to: None,
            orient: false,
            expr: None,
            start: 0.0,
            duration: 1.0,
            ease: EaseDecl::Linear,
            stagger: 0.0,
            stagger_from: StaggerOrigin::Start,
            stagger_grid: None,
            repeat: 1,
            yoyo: false,
            repeat_delay: 0.0,
            line: 1,
            column: 1,
            from_color: None,
            to_color: None,
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
            line: 1,
            column: 1,
            id: None,
            canvas: None,
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
            transitions: Vec::new(),
            motion_blur: None,
        }
    }

    fn red_box(x: f64, y: f64, order: i64) -> Clip {
        Clip::Box(BoxClip {
            asserts: Default::default(),
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            start: 0.0,
            duration: 2.0,
            x,
            y,
            width: 20.0,
            height: 20.0,
            fill: Rgb { r: 255, g: 0, b: 0 },
            fill_paint: notation::model::FillPaint::Solid,
            radius: 4.0,
            border_width: 2.0,
            border_color: Rgb { r: 0, g: 0, b: 255 },
            opacity: 1.0,
            order,
            line: 1,
            column: 1,
            stroke_position: notation::model::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: notation::model::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: notation::model::Overflow::Visible,
            mask: None,
            clip_path: None,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })
    }

    #[test]
    fn paint_stack_agrees_on_both_traversals() {
        // T004: a stroked, shadowed, blended box draws through the
        // paint stack on both traversals, so the pictures agree.
        let mut shadowed = red_box(20.0, 20.0, 1);
        if let Clip::Box(b) = &mut shadowed {
            b.stroke_position = notation::model::StrokePosition::Outside;
            b.box_shadow = Some(notation::model::BoxShadow {
                offset_x: 4.0,
                offset_y: 4.0,
                blur: 2.0,
                spread: 1.0,
                color: Rgb { r: 0, g: 255, b: 0 },
            });
            b.blend_mode = notation::model::BlendMode::Multiply;
            b.backdrop_brightness = 0.8;
        }
        let comp = still_comp(vec![red_box(10.0, 10.0, 0), shadowed]);
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        assert_agree(&mut prep, "paint-stack", 0);
    }

    #[test]
    fn paint_stack_fingerprints_differ() {
        // T004: every paint-stack knob rides the fingerprint, so two
        // looks never share one.
        let comp = still_comp(vec![red_box(10.0, 10.0, 0)]);
        let motions = plan_motions(&comp, 30);
        let base = fingerprint(&describe(&comp, &motions, 0));
        let mut varied = red_box(10.0, 10.0, 0);
        if let Clip::Box(b) = &mut varied {
            b.blend_mode = notation::model::BlendMode::Screen;
        }
        let comp = still_comp(vec![varied]);
        let motions = plan_motions(&comp, 30);
        let other = fingerprint(&describe(&comp, &motions, 0));
        assert_ne!(base, other, "blend rides the fingerprint");
    }

    #[test]
    fn graded_boxes_agree_within_tolerance() {
        // Sprint 11 grade (T001): a clip look plus a composition look
        // draw through the same layer path on both backends, so the
        // pictures agree within tolerance and re-runs are identical.
        let mut graded = red_box(10.0, 10.0, 0);
        if let Clip::Box(b) = &mut graded {
            b.grade = notation::model::Grade {
                preset: Some(notation::model::GradePreset::Filmic),
                exposure: Some(0.5),
                contrast: None,
                saturation: Some(-0.25),
                temperature: None,
                tint: None,
                ..Default::default()
            };
        }
        let mut comp = still_comp(vec![graded]);
        comp.grade = notation::model::Grade {
            preset: None,
            exposure: None,
            contrast: None,
            saturation: None,
            temperature: Some(0.25),
            tint: None,
            ..Default::default()
        };
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        assert!(!desc.grade_for(0).is_empty(), "the clip grade must resolve");
        let fp = fingerprint(&desc);
        assert!(fp.contains("filmic"), "{fp}");
        assert!(fp.contains("cg="), "{fp}");
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        assert_agree(&mut prep, "grade", 0);
        // A graded frame re-renders byte-identically on each backend.
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        let a = prep.cpu_frame(0).expect("cpu frame draws");
        let b = prep.cpu_frame(0).expect("cpu frame draws again");
        assert_eq!(a, b, "graded cpu re-renders are byte-identical");
    }

    #[test]
    fn detail_graded_boxes_agree_within_tolerance() {
        // Sprint 11 grade (T002): a clip look with wheels, a curve, an
        // HSL selection, and seeded grain over a vignetted composition
        // background draws through the shared spatial helpers on both
        // backends, so the pictures agree and re-runs are identical.
        let mut graded = red_box(10.0, 10.0, 0);
        if let Clip::Box(b) = &mut graded {
            b.grade.wheel.shadows = Some(0.2);
            b.grade.curve.red = Some(vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)]);
            b.grade.hsl.hue_min = Some(330.0);
            b.grade.hsl.hue_max = Some(30.0);
            b.grade.hsl.saturation = Some(0.2);
            b.grade.hsl.lightness = Some(0.1);
            b.grade.grain.amount = Some(0.5);
            b.grade.grain.size = Some(1);
            b.grade.grain.seed = Some(7);
        }
        let mut comp = still_comp(vec![graded]);
        comp.grade.vignette.amount = Some(0.6);
        comp.grade.vignette.softness = Some(0.5);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        assert!(
            desc.grade_for(0).is_spatial(),
            "the clip grade must resolve spatial"
        );
        assert!(
            desc.composition_grade.is_spatial(),
            "the composition grade must resolve spatial"
        );
        let fp = fingerprint(&desc);
        assert!(fp.contains("|w:"), "{fp}");
        assert!(fp.contains("|g:"), "{fp}");
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        assert_agree(&mut prep, "detail-grade", 0);
        // A detail-graded frame re-renders byte-identically on each backend.
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        let a = prep.cpu_frame(0).expect("cpu frame draws");
        let b = prep.cpu_frame(0).expect("cpu frame draws again");
        assert_eq!(a, b, "detail-graded cpu re-renders are byte-identical");
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
                asserts: Default::default(),
                id: None,
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                hidden: false,
                start: 0.0,
                duration: 2.0,
                x: 30.0,
                y: 30.0,
                width: 24.0,
                height: 24.0,
                fill: Rgb { r: 0, g: 255, b: 0 },
                fill_paint: notation::model::FillPaint::Solid,
                radius: 6.0,
                border_width: 3.0,
                border_color: Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                stroke_position: notation::model::StrokePosition::Inside,
                box_shadow: None,
                blend_mode: notation::model::BlendMode::Normal,
                backdrop_blur: 0.0,
                backdrop_brightness: 1.0,
                overflow: notation::model::Overflow::Visible,
                mask: None,
                clip_path: None,
                opacity: 0.5,
                order: 2,
                line: 2,
                column: 1,
                turn: notation::model::Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::default(),
                effects: Vec::new(),
                vfx: Default::default(),
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
    fn stroked_paths_agree_within_tolerance_and_reruns_are_identical() {
        // Sprint 8 look (T005): filled and stroked paths in each fill
        // rule, plus a shadowed path, draw identically on both
        // traversals — the single path painter serves CPU and GPU.
        use notation::model::{FillRule, PathClip, StrokePosition};
        fn path(d: &str, rule: FillRule, order: i64) -> Clip {
            Clip::Path(PathClip {
                asserts: Default::default(),
                id: None,
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                hidden: false,
                start: 0.0,
                duration: 2.0,
                x: 0.0,
                y: 0.0,
                d: d.to_string(),
                fill: Rgb { r: 255, g: 0, b: 0 },
                fill_rule: rule,
                border_width: 4.0,
                border_color: Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                stroke_position: StrokePosition::Center,
                box_shadow: None,
                blend_mode: notation::model::BlendMode::Normal,
                backdrop_blur: 0.0,
                backdrop_brightness: 1.0,
                overflow: notation::model::Overflow::Visible,
                mask: None,
                clip_path: None,
                opacity: 1.0,
                order,
                turn: notation::model::Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::default(),
                effects: Vec::new(),
                vfx: Default::default(),
                line: 1,
                column: 1,
            })
        }
        let comp = still_comp(vec![
            path("M8,8 L56,8 L56,56 L8,56 Z", FillRule::NonZero, 1),
            path(
                "M10,10 L54,10 L54,54 L10,54 Z M22,22 L42,22 L42,42 L22,42 Z",
                FillRule::EvenOdd,
                2,
            ),
            Clip::Path(PathClip {
                asserts: Default::default(),
                id: None,
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                hidden: false,
                start: 0.0,
                duration: 2.0,
                x: 0.0,
                y: 0.0,
                d: "M20,44 L44,44 L32,24 Z".to_string(),
                fill: Rgb { r: 0, g: 0, b: 255 },
                fill_rule: FillRule::NonZero,
                border_width: 0.0,
                border_color: Rgb { r: 0, g: 0, b: 0 },
                stroke_position: StrokePosition::Inside,
                box_shadow: Some(notation::model::BoxShadow {
                    offset_x: 6.0,
                    offset_y: 6.0,
                    blur: 2.0,
                    spread: 0.0,
                    color: Rgb { r: 0, g: 255, b: 0 },
                }),
                blend_mode: notation::model::BlendMode::Normal,
                backdrop_blur: 0.0,
                backdrop_brightness: 1.0,
                overflow: notation::model::Overflow::Visible,
                mask: None,
                clip_path: None,
                opacity: 1.0,
                order: 3,
                turn: notation::model::Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::default(),
                effects: Vec::new(),
                vfx: Default::default(),
                line: 3,
                column: 1,
            }),
        ]);
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        for k in [0u64, 15, 59] {
            assert_agree(&mut prep, "stroked-paths", k);
        }
        let a = prep.cpu_frame(15).expect("cpu frame");
        let b = prep.cpu_frame(15).expect("cpu rerun");
        assert_eq!(a, b, "cpu path reruns are byte-identical");
        // The fingerprint names the path element and its fill rule.
        let motions = plan_motions(&comp, 30);
        let fp = fingerprint(&describe(&comp, &motions, 0));
        assert!(fp.contains("path#"), "{fp}");
        assert!(fp.contains("evenodd"), "{fp}");
    }

    #[test]
    fn written_paint_order_is_shared_by_both_traversals() {
        // T013 via T006: the blue box is documented first but painted
        // second, so it must cover the red one on both renderers.
        let comp = still_comp(vec![
            Clip::Box(BoxClip {
                asserts: Default::default(),
                id: None,
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                hidden: false,
                start: 0.0,
                duration: 2.0,
                x: 10.0,
                y: 10.0,
                width: 30.0,
                height: 30.0,
                fill: Rgb { r: 0, g: 0, b: 255 },
                fill_paint: notation::model::FillPaint::Solid,
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
                order: 2,
                line: 1,
                column: 1,
                turn: notation::model::Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::default(),
                effects: Vec::new(),
                vfx: Default::default(),
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
            asserts: Default::default(),
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            value: "Hi Ag".to_string(),
            start: 0.0,
            duration: 2.0,
            x: 4.0,
            y: 4.0,
            size: 12.0,
            color: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            align: Align::Left,
            wrap: Wrap::Off,
            style: FontStyle::Regular,
            order: 1,
            line_height: Some(20.0),
            letter_spacing: Some(3.0),
            font_family: None,
            direction: notation::model::TextDirection::Ltr,
            spans: Vec::new(),
            split: None,
            typewriter: false,
            count_from: None,
            count_to: None,
            decimals: 0,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })]);
        let mut prep = prepare(&comp, Path::new("."), Path::new("ffmpeg")).expect("prepare");
        assert_agree(&mut prep, "spaced-text", 0);
        assert_agree(&mut prep, "spaced-text", 30);
    }

    #[test]
    fn motion_frames_agree_within_tolerance() {
        // T014: a to-tween on x plus a set hold on opacity; the mid
        // frame must show motion on both sides, still inside tolerance.
        use notation::model::{EaseDecl, SetDecl, StaggerOrigin, TweenDecl, TweenKind};
        let mut comp = still_comp(vec![red_box(4.0, 4.0, 1)]);
        comp.tweens.push(TweenDecl {
            target: 0,
            property: "x".to_string(),
            kind: TweenKind::To,
            from: None,
            to: Some(44.0),
            path: None,
            path_to: None,
            orient: false,
            expr: None,
            start: 0.0,
            duration: 1.0,
            ease: EaseDecl::Linear,
            stagger: 0.0,
            stagger_from: StaggerOrigin::Start,
            stagger_grid: None,
            repeat: 1,
            yoyo: false,
            repeat_delay: 0.0,
            line: 1,
            column: 1,
            from_color: None,
            to_color: None,
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
        assert_ne!(
            row_red(&cpu0, 0),
            row_red(&cpu29, 29),
            "cpu shows the tween"
        );
        assert_ne!(
            row_red(&cpu0, 0),
            row_red(&gpu29, 29),
            "gpu shows the tween"
        );
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

    // --- Sprint 8 look (T003): gradient and image fills ---

    #[test]
    fn gpu_split_matches_cpu_blitter_for_gradient_and_image_fills() {
        use crate::render::{draw_description, DecodedImage, DynLayouts, RasterCache};
        use notation::model::{FillPaint, GradientStop};
        fn gradient_box() -> notation::model::Clip {
            notation::model::Clip::Box(BoxClip {
                asserts: Default::default(),
                id: None,
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                hidden: false,
                start: 0.0,
                duration: 2.0,
                x: 4.0,
                y: 4.0,
                width: 24.0,
                height: 16.0,
                fill: Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                fill_paint: FillPaint::Linear {
                    angle: 90.0,
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: Rgb { r: 255, g: 0, b: 0 },
                            line: 1,
                            column: 1,
                        },
                        GradientStop {
                            offset: 1.0,
                            color: Rgb { r: 0, g: 0, b: 255 },
                            line: 1,
                            column: 1,
                        },
                    ],
                },
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
        fn image_box() -> notation::model::Clip {
            notation::model::Clip::Box(BoxClip {
                asserts: Default::default(),
                id: None,
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                hidden: false,
                start: 0.0,
                duration: 2.0,
                x: 40.0,
                y: 4.0,
                width: 24.0,
                height: 16.0,
                fill: Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                fill_paint: FillPaint::Image {
                    src: "bg.png".to_string(),
                    fit: notation::model::Fit::Cover,
                    placement: notation::model::Placement::center(),
                },
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
        let mut comp = composition();
        comp.width = 64;
        comp.height = 32;
        comp.clips = vec![gradient_box(), image_box()];
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        assert_eq!(desc.items.len(), 2);
        // The GPU traversal stats the same folder with the same vanish
        // check, so a present file draws and a missing one refuses.
        let dir = std::env::temp_dir().join(format!("framesmith-gpu-fill-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("bg.png"), b"stat-only").unwrap();
        let images = vec![DecodedImage {
            path: dir.join("bg.png"),
            width: 4,
            height: 4,
            rgba: vec![10u8, 20, 30, 255].repeat(16),
            scaled: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }];
        let mut raster = RasterCache::new();
        let mut cpu = vec![0u8; 64 * 32 * 3];
        draw_description(
            &desc,
            &mut raster,
            &[],
            &DynLayouts::default(),
            &crate::captions::CaptionLayouts::new(),
            false,
            &[],
            &[],
            &images,
            &[],
            &crate::path_paint::PathCache::new(),
            dir.as_path(),
            &mut cpu,
        )
        .unwrap();
        let mut raster = RasterCache::new();
        let gpu = render_description_split(
            &[],
            &[],
            &images,
            &[],
            &[],
            &DynLayouts::default(),
            &crate::captions::CaptionLayouts::new(),
            false,
            &mut raster,
            &crate::path_paint::PathCache::new(),
            dir.as_path(),
            &desc,
        )
        .unwrap();
        assert_eq!(cpu, gpu);
        // The fingerprint names the fill, so two different fills never
        // alias one solid or each other in comparisons.
        let fp = fingerprint(&desc);
        assert!(fp.contains("linear"), "{fp}");
        assert!(fp.contains("bg.png"), "{fp}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gpu_split_refuses_a_vanished_fill_source() {
        use crate::render::{draw_description, DynLayouts, RasterCache};
        use notation::model::FillPaint;
        let mut comp = composition();
        comp.width = 64;
        comp.height = 32;
        comp.clips = vec![notation::model::Clip::Box(BoxClip {
            asserts: Default::default(),
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            start: 0.0,
            duration: 2.0,
            x: 4.0,
            y: 4.0,
            width: 24.0,
            height: 16.0,
            fill: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            fill_paint: FillPaint::Image {
                src: "gone.png".to_string(),
                fit: notation::model::Fit::Cover,
                placement: notation::model::Placement::center(),
            },
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
        })];
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let dir = std::env::temp_dir().join(format!("framesmith-gpu-gone-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut raster = RasterCache::new();
        let mut cpu = vec![0u8; 64 * 32 * 3];
        let err = draw_description(
            &desc,
            &mut raster,
            &[],
            &DynLayouts::default(),
            &crate::captions::CaptionLayouts::new(),
            false,
            &[],
            &[],
            &[],
            &[],
            &crate::path_paint::PathCache::new(),
            dir.as_path(),
            &mut cpu,
        )
        .expect_err("vanished fill must refuse");
        assert!(err.contains("gone.png"), "{err}");
        let mut raster = RasterCache::new();
        let err = render_description_split(
            &[],
            &[],
            &[],
            &[],
            &[],
            &DynLayouts::default(),
            &crate::captions::CaptionLayouts::new(),
            false,
            &mut raster,
            &crate::path_paint::PathCache::new(),
            dir.as_path(),
            &desc,
        )
        .expect_err("vanished fill must refuse on the GPU path");
        assert!(err.contains("gone.png"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
