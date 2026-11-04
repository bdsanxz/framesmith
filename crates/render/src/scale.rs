//! Output scaling (Sprint 12 outputs, T002): the `--resolution`
//! presets and integer `--supersample` one composition serves every
//! screen with.
//!
//! A preset names the output frame size: the canvas aspect is kept
//! (the frame fits inside the preset box) and both sides land even,
//! so every encoder accepts the size. `source` keeps the canvas size
//! exactly. Rendering happens internally at the supersample multiple
//! of the output size — vectors re-rasterize there through a scaled
//! composition ([`scale_composition`]) — then each frame downscales
//! to the output size ([`downscale_rgb24`]) before the encoder sees
//! it. `supersample=1` is the identity path: the composition renders
//! untouched and no scaling work runs, so default bytes never drift.
//!
//! Alpha composites at draw time (Sprint 12 outputs, T001), so every
//! frame arriving here is an opaque composite: semi-transparent
//! sources already blended, and the downscaler moves their colors
//! exactly like the unscaled path. Formats that carry alpha (PNG
//! sequences) therefore keep it through the scale step.

use notation::model::{
    BoxClip, Clip, Composition, Effect, EffectParams, FillPaint, FlexChild, FlexClip, ImageClip,
    KeyDecl, PathClip, SetDecl, SvgClip, TextClip, TextSpan, Turn3D, TweenDecl, VideoClip, Wrap,
};

/// One `--resolution` preset (Sprint 12 outputs, CAP-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolution {
    /// Keep the canvas size exactly.
    #[default]
    Source,
    /// Fit inside 1920x1080, aspect kept, even sides.
    P1080,
    /// Fit inside 1280x720, aspect kept, even sides.
    P720,
    /// Fit inside 854x480, aspect kept, even sides.
    P480,
    /// Fit inside 1080x1080, aspect kept, even sides.
    Square1080,
}

impl Resolution {
    /// Exactly the five accepted spellings; anything else is refused
    /// before the first frame with no output written.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "source" => Some(Resolution::Source),
            "1080p" => Some(Resolution::P1080),
            "720p" => Some(Resolution::P720),
            "480p" => Some(Resolution::P480),
            "square-1080" => Some(Resolution::Square1080),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Resolution::Source => "source",
            Resolution::P1080 => "1080p",
            Resolution::P720 => "720p",
            Resolution::P480 => "480p",
            Resolution::Square1080 => "square-1080",
        }
    }

    /// The accepted-values wording every refusal names.
    pub fn accepted() -> &'static str {
        "source, 1080p, 720p, 480p, or square-1080"
    }

    /// True for the identity preset (the canvas renders as authored).
    pub fn is_source(self) -> bool {
        matches!(self, Resolution::Source)
    }

    /// Output frame size for a canvas: the aspect-preserving fit of
    /// the canvas inside the preset box, rounded to even sides (every
    /// encoder needs even dimensions). `source` keeps the canvas size
    /// exactly, even when the canvas itself is odd.
    pub fn output_size(self, canvas_w: u32, canvas_h: u32) -> (u32, u32) {
        let (box_w, box_h) = match self {
            Resolution::Source => return (canvas_w, canvas_h),
            Resolution::P1080 => (1920.0, 1080.0),
            Resolution::P720 => (1280.0, 720.0),
            Resolution::P480 => (854.0, 480.0),
            Resolution::Square1080 => (1080.0, 1080.0),
        };
        let cw = canvas_w.max(1) as f64;
        let ch = canvas_h.max(1) as f64;
        let s = (box_w / cw).min(box_h / ch);
        (even_dim(cw * s), even_dim(ch * s))
    }
}

/// Round a floating dimension to an even whole size (at least 2):
/// the nearest even integer, so aspect-fit outputs stay encoder-safe
/// without drifting more than a pixel from the true fit.
pub fn even_dim(v: f64) -> u32 {
    let r = v.round() as u32;
    (r / 2 * 2).max(2)
}

/// Parse one `--supersample` value (Sprint 12 outputs, CAP-002): a
/// whole number from 1 to 4. Anything else is refused before the
/// first frame naming the accepted range. Only the canonical digit
/// spellings pass, matching the `--crf` strictness (`2.0`, `+2`, and
/// ` 2 ` trims aside, `2.5` and `0` never do).
pub fn parse_supersample(raw: &str) -> Option<u8> {
    match raw.trim().parse::<u8>() {
        Ok(n) if (1..=4).contains(&n) => Some(n),
        _ => None,
    }
}

/// The accepted-range wording every supersample refusal names.
pub fn supersample_accepted() -> &'static str {
    "a whole number from 1 to 4"
}

/// One render's scaling work (Sprint 12 outputs, T002): the output
/// size the encoder sees, the internal size vectors re-rasterize at,
/// and the per-axis composition multipliers between the canvas and
/// the internal size. The multipliers agree up to the even-side
/// rounding (at most one pixel), so x-like fields ride `kx`, y-like
/// fields ride `ky`, and isotropic fields (font sizes, blur sigmas,
/// radii) ride `kx` by rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalePlan {
    /// Output frame size (what the encoder and the probe see).
    pub out_w: u32,
    pub out_h: u32,
    /// Internal frame size (what the draw pipeline rasterizes).
    pub internal_w: u32,
    pub internal_h: u32,
    /// Composition multipliers: canvas px to internal px.
    pub kx: f64,
    pub ky: f64,
    /// True when no scaling work runs at all (`source` at 1x).
    pub identity: bool,
}

impl ScalePlan {
    /// The identity plan: canvas renders as authored, untouched.
    pub fn identity(canvas_w: u32, canvas_h: u32) -> Self {
        Self {
            out_w: canvas_w,
            out_h: canvas_h,
            internal_w: canvas_w,
            internal_h: canvas_h,
            kx: 1.0,
            ky: 1.0,
            identity: true,
        }
    }

    /// Plan one render from its flags and canvas: fit the preset,
    /// multiply by the supersample for the internal size. `source`
    /// at 1x is the identity path (no scaling work, byte-identical).
    pub fn from_flags(resolution: Resolution, supersample: u8, canvas_w: u32, canvas_h: u32) -> Self {
        let (out_w, out_h) = resolution.output_size(canvas_w, canvas_h);
        let ss = u32::from(supersample.max(1));
        let internal_w = out_w.saturating_mul(ss).max(2);
        let internal_h = out_h.saturating_mul(ss).max(2);
        let kx = internal_w as f64 / canvas_w.max(1) as f64;
        let ky = internal_h as f64 / canvas_h.max(1) as f64;
        let identity = resolution.is_source() && supersample <= 1;
        Self {
            out_w,
            out_h,
            internal_w,
            internal_h,
            kx,
            ky,
            identity,
        }
    }

    /// True when frames must downscale after drawing (the internal
    /// size exceeds the output size).
    pub fn needs_downscale(self) -> bool {
        self.internal_w != self.out_w || self.internal_h != self.out_h
    }

    /// Pixel-value expression results ride this multiplier at draw
    /// time (see `ClipMotion`): 1.0 on the identity path, so
    /// expression-free and expression-driven renders agree exactly
    /// with the unscaled path there.
    pub fn expr_scale(self) -> f64 {
        if self.identity {
            1.0
        } else {
            self.kx
        }
    }
}

/// Scale one validated composition to the internal render size
/// (Sprint 12 outputs, T002): every canvas-pixel field rides the
/// plan multipliers, so the whole pipeline downstream — motion
/// planning, text shaping, describe, drawing, waveform overlays —
/// runs unchanged at the higher resolution. Times, frame counts,
/// colors, angles, opacities, and normalized factors never move, so
/// the scaled render is the canvas render times the multipliers.
pub fn scale_composition(comp: &Composition, plan: ScalePlan) -> Composition {
    if plan.identity {
        return comp.clone();
    }
    let mut out = comp.clone();
    // Proportional (not assigned): the selected composition lands
    // exactly on the internal size, nested compositions ride the same
    // multipliers (their frames composite into the root buffer, so
    // only the root needs encoder-even sides).
    out.width = (f64::from(comp.width) * plan.kx).round().clamp(1.0, 16384.0) as u32;
    out.height = (f64::from(comp.height) * plan.ky).round().clamp(1.0, 16384.0) as u32;
    for clip in &mut out.clips {
        scale_clip(clip, plan);
    }
    for tween in &mut out.tweens {
        scale_tween(tween, plan);
    }
    for key in &mut out.keys {
        scale_key(key, plan);
    }
    for set in &mut out.sets {
        scale_set(set, plan);
    }
    out
}

/// True for the canonical motion properties naming canvas pixels
/// (Sprint 12 outputs, T002): expression results and directive values
/// on these ride the multipliers, while angles, opacities, scales,
/// and colors never do. Aliases normalize to canonical names at
/// validation, so only these spellings arrive here.
pub fn is_pixel_property(property: &str) -> bool {
    matches!(
        property,
        "x" | "y"
            | "width"
            | "height"
            | "radius"
            | "border-width"
            | "blur"
            | "font-size"
            | "letter-spacing"
    )
}

fn scale_clip(clip: &mut Clip, plan: ScalePlan) {
    // Nest effects ride on the use itself; every other clip carries
    // its effects on the shared field (scaled below with the clip).
    if let Clip::Nest(n) = clip {
        for e in &mut n.effects {
            scale_effect(e, plan);
        }
        return;
    }
    match clip {
        Clip::Text(t) => scale_text(t, plan),
        Clip::Video(v) => scale_video(v, plan),
        Clip::Box(b) => scale_box_clip(b, plan),
        Clip::Flex(f) => scale_flex(f, plan),
        Clip::Image(i) => scale_image(i, plan),
        Clip::Path(p) => scale_path_clip(p, plan),
        Clip::Svg(s) => scale_svg(s, plan),
        Clip::Audio(_) => {}
        Clip::Nest(_) => unreachable!("handled above"),
    }
    let effects = match clip {
        Clip::Text(c) => &mut c.effects,
        Clip::Video(c) => &mut c.effects,
        Clip::Box(c) => &mut c.effects,
        Clip::Flex(c) => &mut c.effects,
        Clip::Image(c) => &mut c.effects,
        Clip::Path(c) => &mut c.effects,
        Clip::Svg(c) => &mut c.effects,
        Clip::Audio(c) => &mut c.effects,
        Clip::Nest(_) => unreachable!("handled above"),
    };
    for e in effects {
        scale_effect(e, plan);
    }
}

fn scale_turn(turn: &mut Turn3D, plan: ScalePlan) {
    // Angles ride untouched (degrees, never pixels); only the
    // projection distance scales. The origin is keyword placement.
    if let Some(p) = turn.perspective {
        turn.perspective = Some(p * plan.kx);
    }
}

fn scale_shadow(
    shadow: &mut notation::model::BoxShadow,
    plan: ScalePlan,
) {
    shadow.offset_x *= plan.kx;
    shadow.offset_y *= plan.ky;
    shadow.blur *= plan.kx;
    shadow.spread *= plan.kx;
}

fn scale_effect(effect: &mut Effect, plan: ScalePlan) {
    let params: &mut EffectParams = &mut effect.params;
    // Blur radii and cell sizes live in canvas pixels; factors,
    // angles, levels, cutoffs, and seeds are resolution-independent.
    if let Some(r) = params.radius {
        params.radius = Some(r * plan.kx);
    }
    if let Some(s) = params.size {
        params.size = Some(((s as f64 * plan.kx).round() as u32).max(1));
    }
}

fn scale_fill_paint(fill: &mut FillPaint, plan: ScalePlan) {
    // Gradient stops ride offsets 0..1 plus colors (never pixels);
    // only the radial center and ring radius name canvas pixels.
    if let FillPaint::Radial { cx, cy, radius, .. } = fill {
        if let Some(v) = cx {
            *v *= plan.kx;
        }
        if let Some(v) = cy {
            *v *= plan.ky;
        }
        if let Some(v) = radius {
            *v *= plan.kx;
        }
    }
}

fn scale_text(t: &mut TextClip, plan: ScalePlan) {
    t.x *= plan.kx;
    t.y *= plan.ky;
    t.size *= plan.kx;
    if let Some(v) = t.line_height.as_mut() {
        *v *= plan.ky;
    }
    if let Some(v) = t.letter_spacing.as_mut() {
        *v *= plan.kx;
    }
    if let Wrap::Width(w) = t.wrap {
        t.wrap = Wrap::Width(w * plan.kx);
    }
    for span in &mut t.spans {
        scale_span(span, plan);
    }
    scale_turn(&mut t.turn, plan);
    t.filter_blur *= plan.kx;
}

fn scale_span(span: &mut TextSpan, plan: ScalePlan) {
    if let Some(v) = span.font_size.as_mut() {
        *v *= plan.kx;
    }
    if let Some(v) = span.letter_spacing.as_mut() {
        *v *= plan.kx;
    }
    if let Some(v) = span.line_height.as_mut() {
        *v *= plan.ky;
    }
    for child in &mut span.children {
        scale_span(child, plan);
    }
}

fn scale_video(v: &mut VideoClip, plan: ScalePlan) {
    scale_turn(&mut v.turn, plan);
    v.filter_blur *= plan.kx;
}

fn scale_box_clip(b: &mut BoxClip, plan: ScalePlan) {
    b.x *= plan.kx;
    b.y *= plan.ky;
    b.width *= plan.kx;
    b.height *= plan.ky;
    b.radius *= plan.kx;
    b.border_width *= plan.kx;
    scale_fill_paint(&mut b.fill_paint, plan);
    if let Some(s) = b.box_shadow.as_mut() {
        scale_shadow(s, plan);
    }
    b.backdrop_blur *= plan.kx;
    scale_turn(&mut b.turn, plan);
    b.filter_blur *= plan.kx;
}

fn scale_flex(f: &mut FlexClip, plan: ScalePlan) {
    f.x *= plan.kx;
    f.y *= plan.ky;
    f.width *= plan.kx;
    f.height *= plan.ky;
    f.gap *= plan.kx;
    f.padding *= plan.kx;
    f.margin *= plan.kx;
    for child in &mut f.children {
        scale_flex_child(child, plan);
    }
}

fn scale_flex_child(c: &mut FlexChild, plan: ScalePlan) {
    c.width *= plan.kx;
    c.height *= plan.ky;
    c.radius *= plan.kx;
    c.border_width *= plan.kx;
}

fn scale_image(i: &mut ImageClip, plan: ScalePlan) {
    i.x *= plan.kx;
    i.y *= plan.ky;
    if let Some(w) = i.width.as_mut() {
        *w *= plan.kx;
    }
    if let Some(h) = i.height.as_mut() {
        *h *= plan.ky;
    }
    scale_turn(&mut i.turn, plan);
    i.filter_blur *= plan.kx;
}

fn scale_path_clip(p: &mut PathClip, plan: ScalePlan) {
    p.x *= plan.kx;
    p.y *= plan.ky;
    p.d = scale_path_data(&p.d, plan);
    p.border_width *= plan.kx;
    if let Some(s) = p.box_shadow.as_mut() {
        scale_shadow(s, plan);
    }
    p.backdrop_blur *= plan.kx;
    scale_turn(&mut p.turn, plan);
    p.filter_blur *= plan.kx;
}

fn scale_svg(s: &mut SvgClip, plan: ScalePlan) {
    s.x *= plan.kx;
    s.y *= plan.ky;
    s.width *= plan.kx;
    s.height *= plan.ky;
    scale_turn(&mut s.turn, plan);
    s.filter_blur *= plan.kx;
}

/// Scale one SVG-subset path data string to the internal size
/// (Sprint 12 outputs, T002): parse through the shared path parser
/// (relative forms already normalize to absolute there, and arc
/// flags stay flags, never numbers), multiply every coordinate, and
/// re-emit canonical absolute commands the same parser accepts.
/// Unparseable data rides through untouched: validation refuses it
/// before the first frame, so the renderer never sees it.
pub fn scale_path_data(d: &str, plan: ScalePlan) -> String {
    let Ok(path) = motion::path::parse(d) else {
        return d.to_string();
    };
    let mut out = String::with_capacity(d.len() + 16);
    for cmd in &path.cmds {
        match *cmd {
            motion::path::Cmd::Move { to } => {
                out.push_str(&format!("M {} {} ", to[0] * plan.kx, to[1] * plan.ky));
            }
            motion::path::Cmd::Line { to } => {
                out.push_str(&format!("L {} {} ", to[0] * plan.kx, to[1] * plan.ky));
            }
            motion::path::Cmd::Cubic { c1, c2, to } => {
                out.push_str(&format!(
                    "C {} {} {} {} {} {} ",
                    c1[0] * plan.kx,
                    c1[1] * plan.ky,
                    c2[0] * plan.kx,
                    c2[1] * plan.ky,
                    to[0] * plan.kx,
                    to[1] * plan.ky
                ));
            }
            motion::path::Cmd::Quad { c, to } => {
                out.push_str(&format!(
                    "Q {} {} {} {} ",
                    c[0] * plan.kx,
                    c[1] * plan.ky,
                    to[0] * plan.kx,
                    to[1] * plan.ky
                ));
            }
            motion::path::Cmd::Arc {
                rx,
                ry,
                rot,
                large,
                sweep,
                to,
            } => {
                out.push_str(&format!(
                    "A {} {} {} {} {} {} {} ",
                    rx * plan.kx,
                    ry * plan.kx,
                    rot,
                    if large { 1 } else { 0 },
                    if sweep { 1 } else { 0 },
                    to[0] * plan.kx,
                    to[1] * plan.ky
                ));
            }
            motion::path::Cmd::Close { .. } => {
                out.push_str("Z ");
            }
        }
    }
    out.trim_end().to_string()
}

fn scale_tween(t: &mut TweenDecl, plan: ScalePlan) {
    // Expression tweens evaluate per frame at draw time (scaled
    // there by property); only literal endpoints scale here. Path
    // travel rides absolute canvas coordinates, like clip `d`.
    if t.expr.is_none() && is_pixel_property(&t.property) {
        if let Some(v) = t.from.as_mut() {
            *v *= plan.kx;
        }
        if let Some(v) = t.to.as_mut() {
            *v *= plan.kx;
        }
    }
    if let Some(p) = t.path.as_mut() {
        *p = scale_path_data(p, plan);
    }
    if let Some(p) = t.path_to.as_mut() {
        *p = scale_path_data(p, plan);
    }
}

fn scale_key(k: &mut KeyDecl, plan: ScalePlan) {
    if is_pixel_property(&k.property) {
        for stop in &mut k.stops {
            if stop.value_color.is_none() {
                stop.value *= plan.kx;
            }
            if let Some(p) = stop.path.as_mut() {
                *p = scale_path_data(p, plan);
            }
        }
    } else {
        // Path stops on non-pixel properties still name canvas
        // coordinates (path travel overrides placement).
        for stop in &mut k.stops {
            if let Some(p) = stop.path.as_mut() {
                *p = scale_path_data(p, plan);
            }
        }
    }
}

fn scale_set(s: &mut SetDecl, plan: ScalePlan) {
    if is_pixel_property(&s.property) {
        s.value *= plan.kx;
    }
}

/// Downscale one composited frame to the output size (Sprint 12
/// outputs, T002): separable-correct bilinear sampling over the
/// opaque RGB24 composite, deterministic to the bit. The internal
/// size always covers the output size, so this only ever shrinks
/// (1x renders skip it entirely). Alpha arrives already composited
/// at draw time, so semi-transparent sources keep their blended
/// colors exactly like the unscaled path.
pub fn downscale_rgb24(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<u8> {
    assert_eq!(src.len(), src_w as usize * src_h as usize * 3);
    assert!(dst_w >= 1 && dst_h >= 1);
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }
    let sw = src_w as f64;
    let sh = src_h as f64;
    let dw = dst_w as f64;
    let dh = dst_h as f64;
    let mut out = vec![0u8; dst_w as usize * dst_h as usize * 3];
    for y in 0..dst_h {
        // Source coordinate of the destination pixel center, mapped
        // back through the pixel grids (half-pixel centers align the
        // grids, so a 2x shrink averages exact 2x2 blocks).
        let sy = ((y as f64 + 0.5) * sh / dh - 0.5).clamp(0.0, sh - 1.0);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(src_h - 1);
        let fy = sy - y0 as f64;
        for x in 0..dst_w {
            let sx = ((x as f64 + 0.5) * sw / dw - 0.5).clamp(0.0, sw - 1.0);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let fx = sx - x0 as f64;
            for c in 0..3 {
                let at = |xx: u32, yy: u32| src[(yy * src_w + xx) as usize * 3 + c] as f64;
                let v = at(x0, y0) * (1.0 - fx) * (1.0 - fy)
                    + at(x1, y0) * fx * (1.0 - fy)
                    + at(x0, y1) * (1.0 - fx) * fy
                    + at(x1, y1) * fx * fy;
                out[(y * dst_w + x) as usize * 3 + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{
        Align, Clip, Composition, EaseDecl, FontStyle, Fps, KeyDecl, KeyStop, Rgb,
        SetDecl, StaggerOrigin, TextClip, TextDirection, TrackKind, Turn3D, TweenDecl,
        TweenKind, Wrap,
    };

    fn rgb() -> Rgb {
        Rgb { r: 1, g: 2, b: 3 }
    }

    fn comp_with(clips: Vec<Clip>, tweens: Vec<TweenDecl>, keys: Vec<KeyDecl>) -> Composition {
        Composition {
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 160,
            height: 120,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: rgb(),
            version: 1,
            clips,
            fonts: Vec::new(),
            tweens,
            sets: Vec::new(),
            keys,
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

    fn text_clip() -> TextClip {
        TextClip {
            value: "Hi".to_string(),
            start: 0.0,
            duration: 2.0,
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            x: 10.0,
            y: 20.0,
            size: 24.0,
            color: rgb(),
            align: Align::Left,
            wrap: Wrap::Width(100.0),
            style: FontStyle::Regular,
            order: 0,
            line_height: Some(30.0),
            letter_spacing: Some(2.0),
            font_family: None,
            direction: TextDirection::Ltr,
            spans: Vec::new(),
            split: None,
            typewriter: false,
            count_from: None,
            count_to: None,
            decimals: 0,
            turn: Turn3D {
                rotation_x: 0.0,
                rotation_y: 0.0,
                perspective: Some(500.0),
                origin: notation::model::Placement::center(),
            },
            filter_blur: 1.5,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: notation::model::VfxChain::empty(),
            hidden: false,
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn presets_parse_to_exactly_five_spellings() {
        for (raw, name) in [
            ("source", "source"),
            ("1080p", "1080p"),
            ("720p", "720p"),
            ("480p", "480p"),
            ("square-1080", "square-1080"),
        ] {
            let r = Resolution::parse(raw).unwrap();
            assert_eq!(r.name(), name, "{raw}");
        }
        for bad in ["", "SOURCE", "4k", "1080", "720", "square", " 720p "] {
            assert_eq!(Resolution::parse(bad), None, "{bad:?}");
        }
        assert!(Resolution::accepted().contains("square-1080"));
        assert!(Resolution::Source.is_source());
        assert!(!Resolution::P720.is_source());
    }

    #[test]
    fn supersample_accepts_only_whole_1_to_4() {
        for raw in ["1", "2", "3", "4"] {
            assert_eq!(parse_supersample(raw), Some(raw.parse().unwrap()), "{raw}");
        }
        // `+2` parses like `--crf` (Rust integer syntax); `2.0` never does.
        for bad in ["", "0", "5", "9", "2.5", "1.5", "abc", "2.0", "  "] {
            assert_eq!(parse_supersample(bad), None, "{bad:?}");
        }
        assert!(supersample_accepted().contains("1 to 4"));
    }

    #[test]
    fn preset_sizes_keep_aspect_with_even_sides() {
        // 4:3 canvas: every preset fits the box, aspect kept, even.
        assert_eq!(Resolution::Source.output_size(160, 120), (160, 120));
        assert_eq!(Resolution::P1080.output_size(160, 120), (1440, 1080));
        assert_eq!(Resolution::P720.output_size(160, 120), (960, 720));
        assert_eq!(Resolution::P480.output_size(160, 120), (640, 480));
        assert_eq!(Resolution::Square1080.output_size(160, 120), (1080, 810));
        // Square canvas on the square preset is exact.
        assert_eq!(Resolution::Square1080.output_size(100, 100), (1080, 1080));
        // Odd canvases still land even everywhere (except `source`,
        // which keeps the canvas exactly).
        for preset in [
            Resolution::P1080,
            Resolution::P720,
            Resolution::P480,
            Resolution::Square1080,
        ] {
            for (cw, ch) in [(161u32, 121u32), (333, 200), (1920, 1081)] {
                let (w, h) = preset.output_size(cw, ch);
                assert_eq!((w % 2, h % 2), (0, 0), "{:?} {cw}x{ch}", preset.name());
                let drift = (w as f64 / h as f64 - cw as f64 / ch as f64).abs();
                assert!(drift < 0.01, "{:?} {cw}x{ch}: {w}x{h}", preset.name());
            }
        }
    }

    #[test]
    fn scale_plan_marks_only_source_at_1x_identity() {
        let id = ScalePlan::from_flags(Resolution::Source, 1, 160, 120);
        assert!(id.identity);
        assert!(!id.needs_downscale());
        assert_eq!((id.out_w, id.out_h), (160, 120));
        assert_eq!((id.internal_w, id.internal_h), (160, 120));
        assert_eq!(id.expr_scale(), 1.0);
        // Source at 2x renders internally doubled, delivers canvas size.
        let ss = ScalePlan::from_flags(Resolution::Source, 2, 160, 120);
        assert!(!ss.identity);
        assert!(ss.needs_downscale());
        assert_eq!((ss.out_w, ss.out_h), (160, 120));
        assert_eq!((ss.internal_w, ss.internal_h), (320, 240));
        assert_eq!((ss.kx, ss.ky), (2.0, 2.0));
        // A preset at 1x draws at the output size with no downscale.
        let p = ScalePlan::from_flags(Resolution::P720, 1, 160, 120);
        assert!(!p.identity);
        assert!(!p.needs_downscale());
        assert_eq!((p.out_w, p.out_h), (960, 720));
        assert_eq!((p.internal_w, p.internal_h), (960, 720));
        assert_eq!((p.kx, p.ky), (6.0, 6.0));
        // A preset at 4x renders four times the output size.
        let p4 = ScalePlan::from_flags(Resolution::P480, 4, 160, 120);
        assert_eq!((p4.out_w, p4.out_h), (640, 480));
        assert_eq!((p4.internal_w, p4.internal_h), (2560, 1920));
    }

    #[test]
    fn scale_composition_scales_geometry_not_time() {
        let tween = TweenDecl {
            target: 0,
            property: "x".to_string(),
            kind: TweenKind::FromTo,
            from: Some(10.0),
            to: Some(50.0),
            from_color: None,
            to_color: None,
            start: 0.0,
            duration: 1.0,
            ease: EaseDecl::Linear,
            stagger: 0.0,
            stagger_from: StaggerOrigin::Start,
            stagger_grid: None,
            repeat: 1,
            yoyo: false,
            repeat_delay: 0.0,
            path: None,
            path_to: None,
            orient: false,
            expr: None,
            line: 1,
            column: 1,
        };
        let key = KeyDecl {
            target: 0,
            property: "font-size".to_string(),
            ease: None,
            stops: vec![
                KeyStop {
                    at: 0.0,
                    value: 24.0,
                    value_color: None,
                    path: None,
                    ease: None,
                    line: 1,
                    column: 1,
                },
                KeyStop {
                    at: 1.0,
                    value: 48.0,
                    value_color: None,
                    path: None,
                    ease: None,
                    line: 1,
                    column: 1,
                },
            ],
            stagger: 0.0,
            stagger_from: StaggerOrigin::Start,
            stagger_grid: None,
            repeat: 1,
            yoyo: false,
            repeat_delay: 0.0,
            orient: false,
            line: 1,
            column: 1,
        };
        let set = SetDecl {
            target: 0,
            property: "opacity".to_string(),
            value: 0.5,
            start: 0.0,
            duration: 1.0,
            line: 1,
            column: 1,
        };
        let mut comp = comp_with(vec![Clip::Text(text_clip())], vec![tween], vec![key]);
        comp.sets.push(set);
        let plan = ScalePlan::from_flags(Resolution::Source, 2, 160, 120);
        let scaled = scale_composition(&comp, plan);
        assert_eq!((scaled.width, scaled.height), (320, 240));
        // Time never moves: duration, fps, windows, and motion timing.
        assert_eq!(scaled.duration, 2.0);
        assert_eq!(scaled.tweens[0].start, 0.0);
        assert_eq!(scaled.tweens[0].duration, 1.0);
        assert_eq!(scaled.keys[0].stops[0].at, 0.0);
        let Clip::Text(t) = &scaled.clips[0] else {
            panic!("text clip survives scaling");
        };
        // Canvas pixels double; colors, angles, and opacity never do.
        assert_eq!((t.x, t.y), (20.0, 40.0));
        assert_eq!(t.size, 48.0);
        assert_eq!(t.line_height, Some(60.0));
        assert_eq!(t.letter_spacing, Some(4.0));
        assert!(matches!(t.wrap, Wrap::Width(w) if w == 200.0));
        assert_eq!(t.turn.perspective, Some(1000.0));
        assert_eq!(t.filter_blur, 3.0);
        assert_eq!((t.color.r, t.color.g, t.color.b), (1, 2, 3));
        // Literal directive endpoints ride along; angles and opacities
        // stay put.
        assert_eq!(
            (scaled.tweens[0].from, scaled.tweens[0].to),
            (Some(20.0), Some(100.0))
        );
        assert_eq!(scaled.keys[0].stops[0].value, 48.0);
        assert_eq!(scaled.keys[0].stops[1].value, 96.0);
        assert_eq!(scaled.sets[0].value, 0.5);
        // The identity path clones untouched.
        let id = ScalePlan::identity(160, 120);
        let same = scale_composition(&comp, id);
        let Clip::Text(t) = &same.clips[0] else {
            panic!("text clip survives identity");
        };
        assert_eq!((t.x, t.y, t.size), (10.0, 20.0, 24.0));
        assert_eq!((same.width, same.height), (160, 120));
        // Pixel-property naming covers the geometric set only.
        for p in [
            "x",
            "width",
            "radius",
            "border-width",
            "blur",
            "font-size",
            "letter-spacing",
        ] {
            assert!(is_pixel_property(p), "{p}");
        }
        for p in ["scale", "rotation", "opacity", "fill", "rotation-x"] {
            assert!(!is_pixel_property(p), "{p}");
        }
    }

    #[test]
    fn scale_path_data_scales_coordinates_not_flags() {
        let plan = ScalePlan::from_flags(Resolution::Source, 2, 160, 120);
        let d = "M 0 0 L 10 0 A 5 5 0 1 0 20 0 Z";
        let scaled = scale_path_data(d, plan);
        // Coordinates double; the arc rotation and both flags ride.
        assert!(scaled.contains("A 10 10 0 1 0 40 0"), "{scaled}");
        // The scaled data parses to the same structure.
        let before = motion::path::parse(d).unwrap();
        let after = motion::path::parse(&scaled).unwrap();
        assert_eq!(
            motion::path::structure(&before),
            motion::path::structure(&after)
        );
    }

    #[test]
    fn downscale_averages_blocks_and_roundtrips_identity() {
        // A 2x2 block averages exactly (half-pixel centers align grids).
        let src: Vec<u8> = vec![
            0, 0, 0, 100, 100, 100, //
            200, 200, 200, 255, 255, 255, //
        ];
        // Mean of the four corners: (0 + 100 + 200 + 255) / 4 = 138.75.
        let out = downscale_rgb24(&src, 2, 2, 1, 1);
        assert_eq!(out, vec![139, 139, 139]);
        // Same size round-trips byte-identically.
        let same = downscale_rgb24(&src, 2, 2, 2, 2);
        assert_eq!(same, src);
        // Uniform fields survive any downscale exactly.
        let flat = vec![77u8; 8 * 6 * 3];
        assert_eq!(downscale_rgb24(&flat, 8, 6, 4, 3), vec![77u8; 4 * 3 * 3]);
    }
}
