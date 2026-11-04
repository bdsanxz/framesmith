//! One frame description per output frame time (T001 / CAP-001).
//!
//! For a valid composition, [`describe`] derives exactly one
//! [`FrameDescription`] from the composition plus the exact rational
//! time of one output frame. The description covers every visible
//! element for that instant — geometry, paint order, evaluated motion
//! values, text layout parameters, and placed media — and never state
//! from any other frame time. Both renderers draw only from the
//! description: the CPU blitter consumes it in `render.rs`, and the
//! verification entry point in `gpu.rs` fingerprints it, so a single
//! frame rendered alone equals the same frame from a full render.

use crate::motion_plan::ClipMotion;
use motion::Prop;
use notation::model::{Clip, Composition, Fit, Placement, ResolvedGrade, Rgb};
use timebase::Time;

/// One visible item's resolved look (Sprint 11 grade, CAP-001/T001+T002):
/// the inward-out merge (composition, then lane, then clip) for the
/// clip at global document id `doc`. Flex children share their
/// container's `doc`, so they share its look. Clone, not Copy: detail
/// stages own heap curve lists. `effects` holds the clip's `effect`
/// children in document order (Sprint 11 effects, CAP-003/T004),
/// applied after the grade chain; `vfx` holds the clip's `vfx` chain
/// (Sprint 11 VFX, CAP-004/T005), applied after the effects in the
/// fixed order noise, warp, displacement, luma matte.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameGrade {
    pub doc: usize,
    pub grade: ResolvedGrade,
    pub effects: Vec<notation::model::Effect>,
    pub vfx: notation::model::VfxChain,
}

/// One visible text element at one frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedText {
    /// Document order index into the composition clips.
    pub doc: usize,
    pub order: i64,
    pub value: String,
    /// True when the clip carries `span` runs (Sprint 8 look, T002):
    /// a value-less span text still paints its shaped runs, so the
    /// empty-value skip in the painters must not drop it.
    pub has_spans: bool,
    /// Evaluated canvas position (authored plus motion offset).
    pub x: f64,
    pub y: f64,
    /// Evaluated font size in canvas pixels (authored, or animated).
    pub size: f64,
    pub color: Rgb,
    pub style: notation::model::FontStyle,
    pub line_height: Option<f64>,
    /// Evaluated letter spacing in canvas pixels (authored, or
    /// animated; `None` means the font default).
    pub letter_spacing: Option<f64>,
    /// Evaluated motion (T010): uniform scale about the top-left,
    /// rotation in degrees about the top-left, opacity 0 to 1.
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
    /// Evaluated 3D turn in degrees plus static perspective/origin
    /// (Sprint 7 motion, CAP-005) and blur sigma (CAP-006).
    pub rotate_x: f64,
    pub rotate_y: f64,
    pub blur: f64,
    pub turn: notation::model::Turn3D,
}

/// One visible video element at one frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedVideo {
    pub doc: usize,
    pub order: i64,
    /// Source seconds for this frame (`frame time - clip start`).
    pub src_t: f64,
    pub fit: Fit,
    /// Evaluated motion (T009/T010): position offset of the picture,
    /// uniform scale and rotation (degrees) about the picture center,
    /// opacity 0 to 1.
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
    /// Evaluated 3D turn plus static perspective/origin (CAP-005) and
    /// blur sigma (CAP-006).
    pub rotate_x: f64,
    pub rotate_y: f64,
    pub blur: f64,
    pub turn: notation::model::Turn3D,
}

/// One visible box element at one frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBox {
    pub doc: usize,
    pub order: i64,
    pub x: f64,
    pub y: f64,
    /// Evaluated extents, fill, and border (authored, or animated).
    pub width: f64,
    pub height: f64,
    pub fill: Rgb,
    /// Gradient or image fill (Sprint 8 look, T003): `Solid` paints
    /// `fill` above; any other variant paints clipped to the box
    /// instead of it. Unaffected by `fill` motion tweens, which ride
    /// the solid color.
    pub fill_paint: notation::model::FillPaint,
    pub radius: f64,
    pub border_width: f64,
    pub border_color: Rgb,
    /// Paint stack (Sprint 8 look, T004): stroke position, shadow,
    /// blend, backdrop, overflow, and resolved mask shapes. Defaults
    /// reproduce the historical border-only paint byte-identically.
    pub stroke_position: notation::model::StrokePosition,
    pub box_shadow: Option<notation::model::BoxShadow>,
    pub blend_mode: notation::model::BlendMode,
    pub backdrop_blur: f64,
    pub backdrop_brightness: f64,
    pub overflow: notation::model::Overflow,
    pub mask: Option<notation::model::MaskShape>,
    pub clip_path: Option<notation::model::MaskShape>,
    pub opacity: f64,
    /// Evaluated motion (T010): uniform scale about the top-left,
    /// rotation in degrees about the box center.
    pub scale: f64,
    pub rotation: f64,
    /// Evaluated 3D turn plus static perspective/origin (CAP-005) and
    /// blur sigma (CAP-006).
    pub rotate_x: f64,
    pub rotate_y: f64,
    pub blur: f64,
    pub turn: notation::model::Turn3D,
}

impl ResolvedBox {
    /// True when any Sprint 8 paint-stack feature needs the layered
    /// paint path (Sprint 8 look, T004): a shadow, an active backdrop,
    /// a non-normal blend, a mask, or a non-inside stroke position.
    /// `overflow` alone stays on the fast path: with an inside stroke
    /// the body never leaves the box, so clipping is a no-op.
    pub fn paint_stack_active(&self) -> bool {
        use notation::model::{BlendMode, StrokePosition};
        self.box_shadow.is_some()
            || crate::render::blur_on(self.backdrop_blur)
            || self.backdrop_brightness != 1.0
            || self.blend_mode != BlendMode::Normal
            || self.mask.is_some()
            || self.clip_path.is_some()
            || self.stroke_position != StrokePosition::Inside
    }
}

/// Resolve a mask reference to its shape (Sprint 8 look, T004): the
/// referenced `box` clip's authored rounded rect. Forward references
/// resolve because every clip is validated before the first frame;
/// dangling references and cycles refuse in validation, so a miss
/// here resolves to no mask rather than guessing.
pub fn resolve_mask_shape(comp: &Composition, target: &str) -> Option<notation::model::MaskShape> {
    comp.clips.iter().find_map(|c| match c {
        Clip::Box(b) if b.id.as_deref() == Some(target) => {
            Some(notation::model::MaskShape {
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
                radius: b.radius,
            })
        }
        _ => None,
    })
}

/// One visible vector path element at one frame time (Sprint 8 look,
/// CAP-005/T005): SVG-subset `d` with a fill rule, the shared stroke
/// set, and the shared paint stack. `d` carries no motion of its
/// own; scale, offset, fill, stroke, and opacity evaluate like boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPath {
    pub doc: usize,
    pub order: i64,
    /// Evaluated canvas-pixel translation of the whole shape.
    pub x: f64,
    pub y: f64,
    /// Authored SVG-subset path data (parsed once per distinct string
    /// through the shared [`crate::path_paint::PathCache`]).
    pub d: String,
    /// Evaluated fill color (authored, or animated when a `fill`
    /// tween or set names the clip).
    pub fill: Rgb,
    /// Interior rule (`nonzero` fills by winding, `evenodd` alternates).
    pub fill_rule: notation::model::FillRule,
    /// Evaluated stroke width and color (authored, or animated when a
    /// `border-width`/`border-color` tween or set names the clip).
    pub border_width: f64,
    pub border_color: Rgb,
    /// Paint stack (Sprint 8 look, T004/T005): stroke position, shadow,
    /// blend, backdrop, overflow, and resolved mask shapes. Defaults
    /// paint the fill with an inside stroke, like boxes.
    pub stroke_position: notation::model::StrokePosition,
    pub box_shadow: Option<notation::model::BoxShadow>,
    pub blend_mode: notation::model::BlendMode,
    pub backdrop_blur: f64,
    pub backdrop_brightness: f64,
    pub overflow: notation::model::Overflow,
    pub mask: Option<notation::model::MaskShape>,
    pub clip_path: Option<notation::model::MaskShape>,
    pub opacity: f64,
    /// Evaluated motion: uniform scale about the shape's own
    /// top-left, rotation in degrees about its own center.
    pub scale: f64,
    pub rotation: f64,
    /// Evaluated 3D turn plus static perspective/origin (CAP-005) and
    /// blur sigma (CAP-006).
    pub rotate_x: f64,
    pub rotate_y: f64,
    pub blur: f64,
    pub turn: notation::model::Turn3D,
}

impl ResolvedPath {
    /// True when any paint-stack feature needs the layered paint path:
    /// a shadow, an active backdrop, a non-normal blend, a mask, or a
    /// non-inside stroke position. `overflow` alone stays on the fast
    /// path: with an inside stroke the body never leaves the shape, so
    /// clipping is a no-op.
    pub fn paint_stack_active(&self) -> bool {
        use notation::model::{BlendMode, StrokePosition};
        self.box_shadow.is_some()
            || crate::render::blur_on(self.backdrop_blur)
            || self.backdrop_brightness != 1.0
            || self.blend_mode != BlendMode::Normal
            || self.mask.is_some()
            || self.clip_path.is_some()
            || self.stroke_position != StrokePosition::Inside
    }
}

/// One visible image element at one frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedImage {
    pub doc: usize,
    pub order: i64,
    pub path: String,
    pub x: f64,
    pub y: f64,
    /// Evaluated target extents (authored, or animated when a
    /// `width`/`height` tween or set names the clip; `None` fills the
    /// canvas as before).
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fit: Fit,
    pub placement: Placement,
    /// Evaluated motion (T010): uniform scale about the top-left of
    /// the target rect, rotation in degrees about its center, opacity
    /// 0 to 1.
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
    /// Evaluated 3D turn plus static perspective/origin (CAP-005) and
    /// blur sigma (CAP-006).
    pub rotate_x: f64,
    pub rotate_y: f64,
    pub blur: f64,
    pub turn: notation::model::Turn3D,
}

/// Round one straight-RGBA channel to a byte, deterministically.
pub fn channel_byte(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Whether a clip path names an SVG picture: the `.svg` extension,
/// case-insensitive — the same rule validation enforces.
fn is_svg_source(path: &str) -> bool {
    path.to_lowercase().ends_with(".svg")
}

/// Resolved fit of an `image` clip (Sprint 8 look, T005): SVG sources
/// decode pre-composed, so the blitter draws them with `fill`.
fn svg_fit(im: &notation::model::ImageClip) -> Fit {
    if is_svg_source(&im.path) {
        Fit::Fill
    } else {
        im.fit
    }
}

/// Resolved placement of an `image` clip (Sprint 8 look, T005):
/// centered for pre-composed SVG sources, authored for raster.
fn svg_placement(im: &notation::model::ImageClip) -> Placement {
    if is_svg_source(&im.path) {
        Placement::center()
    } else {
        im.placement
    }
}

/// One visible element at one frame time, in paint order.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameItem {
    Text(ResolvedText),
    Video(ResolvedVideo),
    Box(ResolvedBox),
    Path(ResolvedPath),
    Image(ResolvedImage),
}

impl FrameItem {
    pub fn doc(&self) -> usize {
        match self {
            FrameItem::Text(r) => r.doc,
            FrameItem::Video(r) => r.doc,
            FrameItem::Box(r) => r.doc,
            FrameItem::Path(r) => r.doc,
            FrameItem::Image(r) => r.doc,
        }
    }

    pub fn order(&self) -> i64 {
        match self {
            FrameItem::Text(r) => r.order,
            FrameItem::Video(r) => r.order,
            FrameItem::Box(r) => r.order,
            FrameItem::Path(r) => r.order,
            FrameItem::Image(r) => r.order,
        }
    }

    /// Shift the paint order (Sprint 6 T007): nested items stack
    /// shifted by their nest clip's own order, so one use layers as a
    /// unit while the child's internal layering stays intact.
    pub fn shift_order(&mut self, by: i64) {
        match self {
            FrameItem::Text(r) => r.order = r.order.saturating_add(by),
            FrameItem::Video(r) => r.order = r.order.saturating_add(by),
            FrameItem::Box(r) => r.order = r.order.saturating_add(by),
            FrameItem::Path(r) => r.order = r.order.saturating_add(by),
            FrameItem::Image(r) => r.order = r.order.saturating_add(by),
        }
    }
}

/// One render's composition tree (Sprint 6 T007): the root plus every
/// transitively nested composition, with per-composition motions and
/// each composition's global document base. Global document ids
/// (`bases[ci] + local index`) address the unified prepared media
/// (layouts, videos, ranges); motions stay per composition.
pub struct NestView<'a> {
    pub comps: Vec<&'a Composition>,
    pub motions: Vec<&'a [ClipMotion]>,
    pub bases: Vec<usize>,
}

impl<'a> NestView<'a> {
    /// A single-composition view: global ids are the clip indexes, and
    /// a `nest` clip resolves to nothing, so it stays an empty window
    /// exactly as before nesting executes.
    pub fn single(comp: &'a Composition, motions: &'a [ClipMotion]) -> Self {
        Self {
            comps: vec![comp],
            motions: vec![motions],
            bases: vec![0],
        }
    }

    /// Resolve a `nest` target to a tree position by composition `name`
    /// or `id`, in tree order.
    pub fn resolve(&self, target: &str) -> Option<usize> {
        self.comps
            .iter()
            .position(|c| c.name == target || c.id.as_deref() == Some(target))
    }

    /// Whether the clip at global document id `global` hides its picture
    /// and sound (Sprint 6 T009): hidden clips render as if absent. A
    /// global id outside every composition is never hidden.
    pub fn is_hidden(&self, global: usize) -> bool {
        for (ci, comp) in self.comps.iter().enumerate() {
            let base = self.bases[ci];
            if global >= base && global - base < comp.clips.len() {
                return comp.clips[global - base].hidden();
            }
        }
        false
    }
}

/// Defensive recursion cap for picture resolution (Sprint 6 T007):
/// valid projects nest at most 8 deep and refuse before frame one, so
/// this cap never triggers on valid input.
pub const MAX_NEST_RENDER_DEPTH: u8 = 16;

/// The single reference both renderers draw frame `frame` from.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameDescription {
    pub frame: u64,
    /// Exact rational time of the output frame.
    pub time: Time,
    /// Float view of `time` for the f64 clip bounds.
    pub t: f64,
    pub width: u32,
    pub height: u32,
    pub background: Rgb,
    /// Visible items in paint order: written `order` first, document
    /// order between equal orders.
    pub items: Vec<FrameItem>,
    /// Resolved look per visible item (Sprint 11 grade, CAP-001/T001),
    /// keyed by global document id and independent of paint order.
    pub grades: Vec<FrameGrade>,
    /// Composition-wide look (Sprint 11 grade, CAP-001/T001): the
    /// composition `grade` alone, carried by the background.
    pub composition_grade: ResolvedGrade,
}

impl FrameDescription {
    /// Resolved look for one visible item: the composition, lane, and
    /// clip merge for the clip at `doc`. Items without an entry (never
    /// on valid derivations) show the identity look.
    pub fn grade_for(&self, doc: usize) -> ResolvedGrade {
        // NOTE: grades clone here (Sprint 11 T002 detail stages own
        // heap curve lists, so the resolved look is Clone, not Copy).
        self.grades
            .iter()
            .find(|g| g.doc == doc)
            .map(|g| g.grade.clone())
            .unwrap_or_else(ResolvedGrade::identity)
    }

    /// Authored effects for one visible item (Sprint 11 effects,
    /// CAP-003/T004): the `effect` children of the clip at `doc` in
    /// document order. Items without an entry carry no effects.
    pub fn effects_for(&self, doc: usize) -> &[notation::model::Effect] {
        self.grades
            .iter()
            .find(|g| g.doc == doc)
            .map(|g| g.effects.as_slice())
            .unwrap_or(&[])
    }

    /// Authored VFX chain for one visible item (Sprint 11 VFX,
    /// CAP-004/T005): the `vfx` child of the clip at `doc`. Items
    /// without an entry carry the empty chain.
    pub fn vfx_for(&self, doc: usize) -> &notation::model::VfxChain {
        static EMPTY: notation::model::VfxChain = notation::model::VfxChain {
            noise: None,
            warp: None,
            displacement: None,
            luma_matte: None,
        };
        self.grades
            .iter()
            .find(|g| g.doc == doc)
            .map(|g| &g.vfx)
            .unwrap_or(&EMPTY)
    }
}

/// Derive the description of output frame `frame`: exact time
/// `frame / fps`, one entry per clip visible in its half-open window,
/// motion evaluated at that same instant, painted in written order.
///
/// Visibility is exact rational membership (Sprint 6 CAP-003): the
/// written seconds read exactly, so a clip ending exactly at the
/// composition end is on the final frame — the frame at the
/// composition end minus one frame interval — by construction rather
/// than by float luck. Mid-timeline boundaries behave exactly as the
/// half-open rule states; only float dust at the boundary changes.
pub fn describe(comp: &Composition, motions: &[ClipMotion], frame: u64) -> FrameDescription {
    let view = NestView::single(comp, motions);
    describe_nested(&view, 0, frame)
}

/// Derive the description of output frame `frame` of the tree root
/// (Sprint 6 T007): like [`describe`], but `nest` clips play their
/// target composition's local timeline with per-use trim and rate, and
/// past its content the picture holds the child's last state. The
/// child's paint orders ride shifted by the nest clip's own order.
pub fn describe_nested(view: &NestView, root: usize, frame: u64) -> FrameDescription {
    use crate::motion_plan::frame_time;
    let comp = view.comps[root];
    describe_nested_at(view, root, frame, frame_time(&comp.fps, frame))
}

/// Derive the description of the tree root at one exact composition
/// time (Sprint 6 T008): the same derivation as [`describe_nested`],
/// but the instant is given directly, so a sub-range render draws its
/// output frame `out_frame` at composition time `now`. Exact rational
/// times keep ranged frames identical to the full render's frames at
/// the same instants.
pub fn describe_nested_at(
    view: &NestView,
    root: usize,
    out_frame: u64,
    now: Time,
) -> FrameDescription {
    describe_nested_at_filtered(view, root, out_frame, now, None)
}

/// Derive the description of the tree root at one exact composition
/// time with a top-level clip filter (Sprint 11 transitions,
/// CAP-005/T006): like [`describe_nested_at`], but `keep_top` decides
/// which root clips contribute. A `None` filter keeps every clip and
/// describes exactly like [`describe_nested_at`]; a provided filter
/// receives each root clip's local index (nested expansions of a kept
/// clip still resolve fully, so a kept `nest` draws its whole
/// subtree). Transition bridges describe their isolated pictures
/// through this filter; ordinary frames pass `None` and render
/// exactly as before.
pub fn describe_nested_at_filtered(
    view: &NestView,
    root: usize,
    out_frame: u64,
    now: Time,
    keep_top: Option<&dyn Fn(usize) -> bool>,
) -> FrameDescription {
    let comp = view.comps[root];
    let (mut items, grades) = collect_items(
        view,
        root,
        now,
        0,
        &ResolvedGrade::identity(),
        keep_top,
    );
    // Written order decides; document order breaks ties, and no other
    // stacking (element kind or timing) overrides it. `sort_by` is
    // stable, so equal orders keep document order.
    items.sort_by_key(|item| item.order());
    FrameDescription {
        frame: out_frame,
        time: now,
        t: now.to_f64(),
        width: comp.width,
        height: comp.height,
        background: comp.background,
        items,
        grades,
        composition_grade: ResolvedGrade::identity().layer(&comp.grade),
    }
}

/// Collect the visible items of one composition at one exact time,
/// resolving `nest` clips recursively (Sprint 6 T007). Document ids are
/// global (`bases[ci] + local index`) so the items address the unified
/// prepared media. Unresolvable targets and empty child timelines
/// contribute nothing; valid projects never hit the depth cap.
fn collect_items(
    view: &NestView,
    ci: usize,
    now: Time,
    depth: u8,
    outer: &ResolvedGrade,
    keep_top: Option<&dyn Fn(usize) -> bool>,
) -> (Vec<FrameItem>, Vec<FrameGrade>) {
    use crate::motion_plan::secs_to_time;
    if depth > MAX_NEST_RENDER_DEPTH {
        return (Vec::new(), Vec::new());
    }
    let comp = view.comps[ci];
    let base = view.bases[ci];
    let t = now.to_f64();
    // Grade base for this composition: the parent chain (identity at
    // the root) layered with this composition's own grade. Each clip
    // then layers its lane grade and its own grade inward-out, with
    // missing pieces inherited, never zeroed.
    let comp_base = outer.layer(&comp.grade);
    let lane_grade = |clip: &Clip| {
        comp.lane_grades
            .iter()
            .find(|l| l.track == clip.track())
            .map(|l| &l.grade)
    };
    let mut items: Vec<FrameItem> = Vec::new();
    let mut grades: Vec<FrameGrade> = Vec::new();
    for (local, clip) in comp.clips.iter().enumerate() {
        // Hidden clips contribute no picture (Sprint 6 T009): the
        // frame shows them as if absent. Sound exclusion rides in the
        // audio plan; ids and windows keep validating regardless.
        if clip.hidden() {
            continue;
        }
        // Transition isolation (Sprint 11 transitions, CAP-005/T006):
        // the top-level filter drops every root clip the bridge does
        // not draw, so the bridge pictures hold exactly one side.
        // Nested subtrees are untouched (a kept `nest` still expands
        // fully), and `None` keeps everything, as before.
        if depth == 0 {
            if let Some(keep) = keep_top {
                if !keep(local) {
                    continue;
                }
            }
        }
        let doc = base + local;
        let start_t = secs_to_time(clip.start());
        let end_t = start_t + secs_to_time(clip.duration());
        if now < start_t || now >= end_t {
            continue;
        }
        // Inward-out look for this clip: composition, then lane, then
        // clip, with missing pieces inherited, never zeroed.
        let grade = comp_base.layer_opt(lane_grade(clip)).layer(clip.grade());
        let motion = view.motions[ci].get(local);
        let eval = motion.map(|m| m.values_at(now)).unwrap_or_else(|| {
            // No motion entry: the clip holds its own values.
            let comp = view.comps[ci];
            crate::motion_plan::ClipMotion::still_on(
                local,
                clip,
                30,
                (f64::from(comp.width), f64::from(comp.height)),
            )
            .values_at(now)
        });
        // Whether a tween or set names the property: evaluated values
        // then win over the authored fields (Sprint 7 motion, CAP-006).
        // Clips without a motion entry never touch anything.
        let touches = |prop: Prop| motion.is_some_and(|m| m.touches(prop));
        match clip {
            Clip::Text(tc) => {
                items.push(FrameItem::Text(ResolvedText {
                doc,
                order: tc.order,
                // Sprint 7 text effects (CAP-009): a typewriter reveal
                // or count range shows its own string at this instant;
                // still and merely-split clips show the authored value.
                value: crate::textfx::display_value(tc, t),
                // Split, typewriter, and count never combine with spans
                // (refused at validation), so an empty value with spans
                // is always a still span text with ink to draw.
                has_spans: !tc.spans.is_empty(),
                x: eval.x,
                y: eval.y,
                size: eval.font_size,
                color: tc.color,
                style: tc.style,
                line_height: tc.line_height,
                letter_spacing: match (tc.letter_spacing, touches(Prop::LetterSpacing)) {
                    (Some(_), _) | (None, true) => Some(eval.letter_spacing),
                    (None, false) => None,
                },
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: tc.turn,
            }));
            grades.push(FrameGrade { doc, grade, effects: clip.effects().to_vec(), vfx: clip.vfx().clone() });
            }
            Clip::Video(v) => {
                items.push(FrameItem::Video(ResolvedVideo {
                doc,
                order: v.order,
                // Sprint 5 T002: the clip consumes its source window at
                // the declared rate — composition seconds past the clip
                // start advance `rate` source seconds from `trim_start`.
                src_t: v.trim_start + (t - v.start).max(0.0) * v.rate,
                fit: v.fit,
                x: eval.x,
                y: eval.y,
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: v.turn,
            }));
            grades.push(FrameGrade { doc, grade, effects: clip.effects().to_vec(), vfx: clip.vfx().clone() });
            }
            Clip::Box(b) => {
                items.push(FrameItem::Box(ResolvedBox {
                doc,
                order: b.order,
                x: eval.x,
                y: eval.y,
                width: eval.width,
                height: eval.height,
                fill: Rgb {
                    r: channel_byte(eval.fill[0]),
                    g: channel_byte(eval.fill[1]),
                    b: channel_byte(eval.fill[2]),
                },
                fill_paint: b.fill_paint.clone(),
                radius: eval.radius,
                border_width: eval.border_width,
                border_color: Rgb {
                    r: channel_byte(eval.border[0]),
                    g: channel_byte(eval.border[1]),
                    b: channel_byte(eval.border[2]),
                },
                stroke_position: b.stroke_position,
                box_shadow: b.box_shadow,
                blend_mode: b.blend_mode,
                backdrop_blur: b.backdrop_blur,
                backdrop_brightness: b.backdrop_brightness,
                overflow: b.overflow,
                mask: b
                    .mask
                    .as_ref()
                    .and_then(|m| resolve_mask_shape(comp, &m.target)),
                clip_path: b
                    .clip_path
                    .as_ref()
                    .and_then(|m| resolve_mask_shape(comp, &m.target)),
                opacity: eval.opacity,
                scale: eval.scale,
                rotation: eval.rotation,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: b.turn,
            }));
            grades.push(FrameGrade { doc, grade, effects: clip.effects().to_vec(), vfx: clip.vfx().clone() });
            }
            // A `flex` container (Sprint 8 layout, T001) emits one box
            // item per laid-out child: the shared layout function
            // positions them, so checked offsets and painted pixels
            // agree. Children ride still (no per-child motion in T001)
            // with the container order shifting their paint order.
            Clip::Flex(f) => {
                let laid = notation::flex::layout_flex(
                    f.direction,
                    eval.x,
                    eval.y,
                    match touches(Prop::Width) {
                        true => eval.width,
                        false => f.width,
                    },
                    match touches(Prop::Height) {
                        true => eval.height,
                        false => f.height,
                    },
                    f.gap,
                    f.padding,
                    f.margin,
                    f.align_items,
                    f.justify_content,
                    &f.children
                        .iter()
                        .map(|c| notation::flex::FlexItem {
                            base_w: c.width,
                            base_h: c.height,
                            grow: c.flex_grow,
                        })
                        .collect::<Vec<_>>(),
                );
                for (c, r) in f.children.iter().zip(laid.iter()) {
                    items.push(FrameItem::Box(ResolvedBox {
                        doc,
                        order: f.order.saturating_add(c.order),
                        x: r.x,
                        y: r.y,
                        width: r.w,
                        height: r.h,
                        fill: c.fill,
                        fill_paint: notation::model::FillPaint::Solid,
                        radius: c.radius,
                        border_width: c.border_width,
                        border_color: c.border_color,
                        stroke_position: c.stroke_position,
                        box_shadow: None,
                        blend_mode: notation::model::BlendMode::Normal,
                        backdrop_blur: 0.0,
                        backdrop_brightness: 1.0,
                        overflow: notation::model::Overflow::Visible,
                        mask: None,
                        clip_path: None,
                        opacity: c.opacity,
                        scale: 1.0,
                        rotation: 0.0,
                        rotate_x: 0.0,
                        rotate_y: 0.0,
                        blur: 0.0,
                        turn: notation::model::Turn3D::flat(),
                    }));
                    grades.push(FrameGrade {
                        doc,
                        grade: grade.clone(),
                        effects: clip.effects().to_vec(),
                        vfx: clip.vfx().clone(),
                    });
                }
            }
            // Audio tracks draw no picture: they ride in the composition
            // for the T008 mixer and never emit a frame item.
            Clip::Audio(_) => {}
            // A `nest` clip plays its target's local timeline at the
            // mapped child time; past the content the hold shows the
            // child's last state (the sound side stays silent there).
            Clip::Nest(n) => {
                let Some(ti) = view.resolve(&n.target) else {
                    continue;
                };
                let child = view.comps[ti];
                if !child.duration.is_finite() || child.duration <= 0.0 {
                    continue;
                }
                let content_end = n
                    .trim_end
                    .map(|e| e.min(child.duration))
                    .unwrap_or(child.duration);
                let c = n.trim_start + (t - n.start).max(0.0) * n.rate;
                let child_now = if c < content_end {
                    secs_to_time(c)
                } else {
                    let interval = 1.0 / child.fps.value().max(1.0);
                    secs_to_time((content_end - interval).max(n.trim_start))
                };
                // The child's items resolve under the nest use: the
                // parent chain plus this composition, its lane for the
                // nest clip's track, and the nest clip's own grade form
                // the outer chain the child composition layers under.
                let child_outer = comp_base.layer_opt(lane_grade(clip)).layer(&n.grade);
                let (mut child_items, mut child_grades) =
                    collect_items(view, ti, child_now, depth + 1, &child_outer, keep_top);
                for item in child_items.iter_mut() {
                    item.shift_order(n.order);
                }
                // The nest clip's own effects finish the nested content
                // (Sprint 11 effects, CAP-003/T004): outer look last,
                // like the grade chain, in the nest clip's document
                // order.
                if !n.effects.is_empty() {
                    for g in child_grades.iter_mut() {
                        g.effects.extend(n.effects.iter().cloned());
                    }
                }
                // The nest clip's own VFX chain finishes the nested
                // content (Sprint 11 VFX, CAP-004/T005): each stage
                // the nested item already defines wins, each missing
                // stage inherits the nest use, so the merged chain
                // still holds every stage at most once.
                if !n.vfx.is_empty() {
                    for g in child_grades.iter_mut() {
                        g.vfx = g.vfx.layer_outer(&n.vfx);
                    }
                }
                items.append(&mut child_items);
                grades.extend(child_grades);
            }
            // A vector path resolves like a box: every paint field
            // evaluates per frame (motion tweens win), while `d` and
            // the fill rule ride authored — path data carries no
            // motion of its own in T005.
            Clip::Path(p) => {
                items.push(FrameItem::Path(ResolvedPath {
                doc,
                order: p.order,
                x: eval.x,
                y: eval.y,
                d: p.d.clone(),
                fill: Rgb {
                    r: channel_byte(eval.fill[0]),
                    g: channel_byte(eval.fill[1]),
                    b: channel_byte(eval.fill[2]),
                },
                fill_rule: p.fill_rule,
                border_width: eval.border_width,
                border_color: Rgb {
                    r: channel_byte(eval.border[0]),
                    g: channel_byte(eval.border[1]),
                    b: channel_byte(eval.border[2]),
                },
                stroke_position: p.stroke_position,
                box_shadow: p.box_shadow,
                blend_mode: p.blend_mode,
                backdrop_blur: p.backdrop_blur,
                backdrop_brightness: p.backdrop_brightness,
                overflow: p.overflow,
                mask: p
                    .mask
                    .as_ref()
                    .and_then(|m| resolve_mask_shape(comp, &m.target)),
                clip_path: p
                    .clip_path
                    .as_ref()
                    .and_then(|m| resolve_mask_shape(comp, &m.target)),
                opacity: eval.opacity,
                scale: eval.scale,
                rotation: eval.rotation,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: p.turn,
            }));
            grades.push(FrameGrade { doc, grade, effects: clip.effects().to_vec(), vfx: clip.vfx().clone() });
            }
            Clip::Image(im) => {
                // SVG sources (Sprint 8 look, T005) decode pre-composed
                // at a concrete size, so an omitted extent falls back
                // to the canvas instead of `None`, and they resolve
                // with `fill` placement. Raster sources keep their
                // historical `None` (the blitter fills the canvas).
                let via_svg = is_svg_source(&im.path);
                let img_width = match (im.width, touches(Prop::Width)) {
                    (None, false) => via_svg.then(|| f64::from(comp.width)),
                    (Some(_), false) => im.width,
                    (_, true) => Some(eval.width),
                };
                let img_height = match (im.height, touches(Prop::Height)) {
                    (None, false) => via_svg.then(|| f64::from(comp.height)),
                    (Some(_), false) => im.height,
                    (_, true) => Some(eval.height),
                };
                items.push(FrameItem::Image(ResolvedImage {
                    doc,
                    order: im.order,
                    path: im.path.clone(),
                    x: eval.x,
                    y: eval.y,
                    // Animated extents win when a `width`/`height` tween
                    // or set names the clip; otherwise the authored
                    // fields stand, exactly as before.
                    width: img_width,
                    height: img_height,
                    fit: svg_fit(im),
                    placement: svg_placement(im),
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: im.turn,
                }));
                grades.push(FrameGrade { doc, grade, effects: clip.effects().to_vec(), vfx: clip.vfx().clone() });
            }
            // An `svg` clip resolves to an image item: validation
            // guarantees `width`/`height`, and preparation decodes the
            // source pre-composed at that declared size, so the shared
            // blitter draws it with `fill` placement.
            Clip::Svg(sv) => {
                items.push(FrameItem::Image(ResolvedImage {
                doc,
                order: sv.order,
                path: sv.path.clone(),
                x: eval.x,
                y: eval.y,
                width: Some(match touches(Prop::Width) {
                    true => eval.width,
                    false => sv.width,
                }),
                height: Some(match touches(Prop::Height) {
                    true => eval.height,
                    false => sv.height,
                }),
                fit: Fit::Fill,
                placement: Placement::center(),
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: sv.turn,
            }));
            grades.push(FrameGrade { doc, grade, effects: clip.effects().to_vec(), vfx: clip.vfx().clone() });
            }
        }
    }
    (items, grades)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion_plan::plan_motions;
    use notation::model::{
        Align, BoxClip, Composition, FlexAlign, FlexChild, FlexClip, FlexDirection, FlexJustify,
        FontStyle, Fps, Rgb, TextClip, TrackKind, VideoClip, Wrap,
    };

    fn comp_with(clips: Vec<Clip>) -> Composition {
        Composition {
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 320,
            height: 240,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 1, g: 2, b: 3 },
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

    fn text(start: f64, duration: f64, order: i64, x: f64) -> Clip {
        Clip::Text(TextClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            value: "Hi".to_string(),
            start,
            duration,
            x,
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
            order,
            line_height: None,
            letter_spacing: Some(2.0),
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
        })
    }

    fn video(start: f64, duration: f64, order: i64) -> Clip {
        Clip::Video(VideoClip {
            id: None,
            track: TrackKind::Video,
            label: None,
            start_after: None,
            hidden: false,
            path: "clip.mp4".to_string(),
            start,
            duration,
            duration_inferred: false,
            fit: Fit::Contain,
            order,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            crossfade: notation::model::Crossfade::default(),
            duck_under: None,
            fx: Default::default(),
            waveform: Default::default(),
            trim_start: 0.0,
            trim_end: None,
            rate: 1.0,
            loop_clip: false,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })
    }

    fn boxed(start: f64, duration: f64, order: i64) -> Clip {
        Clip::Box(BoxClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            start,
            duration,
            x: 5.0,
            y: 6.0,
            width: 40.0,
            height: 30.0,
            fill: Rgb { r: 255, g: 0, b: 0 },
            fill_paint: notation::model::FillPaint::Solid,
            radius: 2.0,
            border_width: 1.0,
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
            order,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })
    }

    // --- Sprint 8 look (T004): paint stack and masks ---

    fn masked_box(id: Option<&str>, mask: Option<&str>) -> Clip {
        let mut b = boxed(0.0, 2.0, 0);
        if let Clip::Box(inner) = &mut b {
            inner.id = id.map(str::to_string);
            inner.mask = mask.map(|t| notation::model::MaskRef {
                target: t.to_string(),
                line: 1,
                column: 1,
            });
        }
        b
    }

    #[test]
    fn mask_forward_reference_resolves_to_shape() {
        // The mask names a later box: resolution reads the authored
        // rounded rect of the target, whatever the declaration order.
        let comp = comp_with(vec![
            masked_box(Some("top"), Some("shape")),
            masked_box(Some("shape"), None),
        ]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 15);
        assert_eq!(desc.items.len(), 2);
        match &desc.items[0] {
            FrameItem::Box(r) => {
                let m = r.mask.expect("mask resolves");
                assert_eq!(m.x, 5.0);
                assert_eq!(m.y, 6.0);
                assert_eq!(m.width, 40.0);
                assert_eq!(m.height, 30.0);
                assert_eq!(m.radius, 2.0);
                assert!(r.paint_stack_active());
            }
            other => panic!("expected a box, got {other:?}"),
        }
        match &desc.items[1] {
            FrameItem::Box(r) => {
                assert!(r.mask.is_none());
                assert!(!r.paint_stack_active());
            }
            other => panic!("expected a box, got {other:?}"),
        }
    }

    #[test]
    fn dangling_mask_resolves_to_no_shape() {
        // Validation refuses dangling references before frame one; the
        // frame layer treats a miss as unmasked rather than guessing.
        let comp = comp_with(vec![masked_box(Some("top"), Some("ghost"))]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 15);
        match &desc.items[0] {
            FrameItem::Box(r) => assert!(r.mask.is_none()),
            other => panic!("expected a box, got {other:?}"),
        }
    }

    #[test]
    fn same_frame_number_yields_the_identical_description() {
        let comp = comp_with(vec![text(0.0, 2.0, 0, 80.0), boxed(0.0, 2.0, 1)]);
        let motions = plan_motions(&comp, 30);
        let a = describe(&comp, &motions, 15);
        let b = describe(&comp, &motions, 15);
        assert_eq!(a, b, "one frame time yields one description");
        assert_eq!(a.time.to_exact(), "1/2");
        assert_eq!(a.items.len(), 2);
    }

    #[test]
    fn description_holds_only_its_own_instant() {
        // Second clip starts at 1 s; frame 0 (t=0) must not see it,
        // frame 30 (t=1) must not see the first clip's expired window.
        let comp = comp_with(vec![text(0.0, 1.0, 0, 80.0), text(1.0, 1.0, 0, 90.0)]);
        let motions = plan_motions(&comp, 30);
        let early = describe(&comp, &motions, 0);
        assert_eq!(early.items.len(), 1);
        assert_eq!(early.items[0].doc(), 0);
        let late = describe(&comp, &motions, 30);
        assert_eq!(late.items.len(), 1);
        assert_eq!(late.items[0].doc(), 1);
        assert_ne!(early, late);
    }

    #[test]
    fn boundary_clip_is_visible_on_the_final_frame() {
        // Sprint 6 CAP-003: a clip ending exactly at the composition end
        // shows on the final frame (the frame at the composition end
        // minus one frame interval); a clip starting exactly at the end
        // does not. A rational-fps grid exercises the same rule without
        // float luck.
        let comp = comp_with(vec![text(0.0, 2.0, 0, 80.0), text(2.0, 1.0, 0, 90.0)]);
        let motions = plan_motions(&comp, 30);
        let last = describe(&comp, &motions, 59);
        let docs: Vec<usize> = last.items.iter().map(FrameItem::doc).collect();
        assert_eq!(docs, vec![0], "final frame shows the boundary clip only");
        let mut ntsc = comp_with(vec![text(0.0, 2.0, 0, 80.0)]);
        ntsc.fps = Fps {
            num: 30000,
            den: 1001,
        };
        let frames = ntsc.fps.frame_count(ntsc.duration) as u64;
        assert_eq!(frames, 60);
        let motions = plan_motions(&ntsc, 30);
        let last = describe(&ntsc, &motions, frames - 1);
        assert_eq!(last.items.len(), 1, "rational grid keeps the final instant");
        assert_eq!(last.items[0].doc(), 0);
    }

    // --- Sprint 7 motion (T005/T006): animated visual properties ---

    fn tween_on(
        target: usize,
        property: &str,
        kind: notation::model::TweenKind,
        from: Option<f64>,
        to: Option<f64>,
        from_color: Option<Rgb>,
        to_color: Option<Rgb>,
    ) -> notation::model::TweenDecl {
        notation::model::TweenDecl {
            target,
            property: property.to_string(),
            kind,
            from,
            to,
            from_color,
            to_color,
            start: 0.0,
            duration: 1.0,
            ease: notation::model::EaseDecl::Linear,
            stagger: 0.0,
            stagger_from: notation::model::StaggerOrigin::Start,
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
        }
    }

    fn comp_with_motion(clips: Vec<Clip>, tweens: Vec<notation::model::TweenDecl>) -> Composition {
        Composition {
            tweens,
            ..comp_with(clips)
        }
    }

    #[test]
    fn box_description_carries_animated_geometry_and_paint() {
        // T006: width 40->80, fill red->blue, radius 2->10, all over
        // [0, 1) linear; frame 15 is exactly halfway.
        use notation::model::TweenKind;
        let comp = comp_with_motion(
            vec![boxed(0.0, 2.0, 0)],
            vec![
                tween_on(
                    0,
                    "width",
                    TweenKind::FromTo,
                    Some(40.0),
                    Some(80.0),
                    None,
                    None,
                ),
                tween_on(
                    0,
                    "fill",
                    TweenKind::FromTo,
                    None,
                    None,
                    Some(Rgb { r: 255, g: 0, b: 0 }),
                    Some(Rgb { r: 0, g: 0, b: 255 }),
                ),
                tween_on(
                    0,
                    "radius",
                    TweenKind::FromTo,
                    Some(2.0),
                    Some(10.0),
                    None,
                    None,
                ),
            ],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| match &describe(&comp, &motions, k).items[0] {
            FrameItem::Box(r) => r.clone(),
            other => panic!("expected box, got {other:?}"),
        };
        let mid = at(15);
        assert_eq!(mid.width, 60.0, "halfway width");
        assert_eq!(
            mid.fill,
            Rgb {
                r: 128,
                g: 0,
                b: 128
            },
            "straight-RGBA midpoint rounds half away (127.5 -> 128)"
        );
        assert_eq!(mid.radius, 6.0, "halfway radius");
        assert_eq!(at(30).width, 80.0, "end lands");
        assert_eq!(at(30).fill, Rgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn text_description_carries_animated_metrics_and_turn() {
        // T005/T006: font-size 24->48, rotation-x 0->90 over [0, 1);
        // letter-spacing stays the authored 2.0 (Some) without motion.
        use notation::model::TweenKind;
        let comp = comp_with_motion(
            vec![text(0.0, 2.0, 0, 80.0)],
            vec![
                tween_on(
                    0,
                    "font-size",
                    TweenKind::FromTo,
                    Some(24.0),
                    Some(48.0),
                    None,
                    None,
                ),
                tween_on(
                    0,
                    "rotation-x",
                    TweenKind::FromTo,
                    Some(0.0),
                    Some(90.0),
                    None,
                    None,
                ),
            ],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| match &describe(&comp, &motions, k).items[0] {
            FrameItem::Text(r) => r.clone(),
            other => panic!("expected text, got {other:?}"),
        };
        let mid = at(15);
        assert_eq!(mid.size, 36.0, "halfway font size");
        assert_eq!(mid.rotate_x, 45.0, "halfway turn");
        assert_eq!(mid.rotate_y, 0.0, "untouched axis stays 0");
        assert_eq!(mid.letter_spacing, Some(2.0), "authored spacing stands");
        assert_eq!(mid.blur, 0.0, "no blur without motion");
    }

    #[test]
    fn static_turn_and_filter_reach_the_description() {
        // T005/T006: static rotation and filter blur land in every
        // resolved item with the perspective and origin intact.
        let mut clip = boxed(0.0, 2.0, 0);
        if let Clip::Box(bc) = &mut clip {
            bc.turn.rotation_x = 10.0;
            bc.turn.perspective = Some(300.0);
            bc.filter_blur = 2.5;
        }
        let comp = comp_with(vec![clip]);
        let motions = plan_motions(&comp, 30);
        match &describe(&comp, &motions, 0).items[0] {
            FrameItem::Box(r) => {
                assert_eq!(r.rotate_x, 10.0);
                assert_eq!(r.blur, 2.5);
                assert_eq!(r.turn.perspective, Some(300.0));
                assert_eq!(r.turn.origin, notation::model::Placement::center());
            }
            other => panic!("expected box, got {other:?}"),
        }
    }

    #[test]
    fn video_src_t_follows_trim_and_rate() {
        // Sprint 5 T002: composition seconds past the clip start advance
        // `rate` source seconds from `trim_start` (rate 2 over a
        // 1..3 s window: frame 15 at t=0.5 maps to source 2.0 s).
        let clip = Clip::Video(VideoClip {
            id: None,
            track: TrackKind::Video,
            label: None,
            start_after: None,
            hidden: false,
            path: "clip.mp4".to_string(),
            start: 0.0,
            duration: 2.0,
            duration_inferred: false,
            fit: Fit::Contain,
            order: 0,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            crossfade: notation::model::Crossfade::default(),
            duck_under: None,
            fx: Default::default(),
            waveform: Default::default(),
            trim_start: 1.0,
            trim_end: Some(3.0),
            rate: 2.0,
            loop_clip: false,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        });
        let comp = comp_with(vec![clip]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| match &describe(&comp, &motions, k).items[0] {
            FrameItem::Video(r) => r.src_t,
            other => panic!("expected video, got {other:?}"),
        };
        assert!((at(0) - 1.0).abs() < 1e-9, "clip start maps to trim-start");
        assert!(
            (at(15) - 2.0).abs() < 1e-9,
            "t=0.5 at rate 2 maps to source 2.0"
        );
        assert!((at(29) - (1.0 + 29.0 / 30.0 * 2.0)).abs() < 1e-9);
    }

    #[test]
    fn written_order_wins_over_document_order() {
        // Box written first in the document but ordered after the text.
        let comp = comp_with(vec![
            boxed(0.0, 2.0, 5),
            text(0.0, 2.0, -1, 80.0),
            video(0.0, 2.0, 5),
        ]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let docs: Vec<usize> = desc.items.iter().map(FrameItem::doc).collect();
        // order -1 first, then the two order-5 clips in document order.
        assert_eq!(
            docs,
            vec![1, 0, 2],
            "paint follows written order, ties keep document order"
        );
    }

    #[test]
    fn evaluated_positions_ride_in_the_description() {
        let comp = comp_with(vec![text(0.0, 2.0, 0, 80.0)]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 7);
        match &desc.items[0] {
            FrameItem::Text(r) => {
                assert!(
                    (r.x - 80.0).abs() < 1e-9,
                    "still clips hold authored values, got {}",
                    r.x
                );
                assert!((r.y - 10.0).abs() < 1e-9);
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    // --- Sprint 2 motion descriptions (T006, T009–T012) ---------------

    use notation::model::{EaseDecl, SetDecl, StaggerOrigin, TweenDecl, TweenKind};

    fn image_clip(start: f64, duration: f64, order: i64) -> Clip {
        Clip::Image(notation::model::ImageClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            path: "photo.png".to_string(),
            start,
            duration,
            x: 7.0,
            y: 8.0,
            width: Some(40.0),
            height: Some(30.0),
            fit: Fit::Contain,
            placement: notation::model::Placement::center(),
            order,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })
    }

    fn motion_comp(clips: Vec<Clip>, tweens: Vec<TweenDecl>, sets: Vec<SetDecl>) -> Composition {
        Composition {
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 320,
            height: 240,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 1, g: 2, b: 3 },
            version: 1,
            clips,
            fonts: Vec::new(),
            tweens,
            sets,
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

    fn tween(
        target: usize,
        property: &str,
        kind: TweenKind,
        from: Option<f64>,
        to: Option<f64>,
        ease: EaseDecl,
    ) -> TweenDecl {
        TweenDecl {
            target,
            property: property.to_string(),
            kind,
            from,
            to,
            start: 0.0,
            duration: 1.0,
            ease,
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
            from_color: None,
            to_color: None,
        }
    }

    #[test]
    fn paint_order_ignores_kind_and_timing() {
        // T006: no element kind or timing overrides the written order.
        // Box (early, long), video (late start), image and text (equal
        // orders, scattered starts): paint follows `order` alone.
        let comp = motion_comp(
            vec![
                video(1.0, 1.0, 3),
                image_clip(0.5, 1.0, 1),
                boxed(0.0, 2.0, 0),
                text(0.0, 2.0, 1, 80.0),
            ],
            vec![],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        // Frame 30 (t=1): every clip is inside its half-open window.
        let desc = describe(&comp, &motions, 30);
        let docs: Vec<usize> = desc.items.iter().map(FrameItem::doc).collect();
        // Visible at t=1: box order 0, image+text order 1 (doc order),
        // video order 3 — regardless of kind or start time.
        assert_eq!(
            docs,
            vec![2, 1, 3, 0],
            "order decides, ties keep document order"
        );
    }

    #[test]
    fn equal_orders_paint_in_document_order() {
        // T006: clips with equal (or absent-equal default) orders keep
        // document order even when kinds interleave.
        let comp = motion_comp(
            vec![
                video(0.0, 2.0, 0),
                boxed(0.0, 2.0, 0),
                text(0.0, 2.0, 0, 80.0),
            ],
            vec![],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let docs: Vec<usize> = desc.items.iter().map(FrameItem::doc).collect();
        assert_eq!(docs, vec![0, 1, 2]);
    }

    #[test]
    fn position_tweens_move_box_text_and_video() {
        // T009: `to` on a box, `from` on text, `from-to` on video, all
        // over [0, 1) at 30 fps: exact starts, halfway midpoints, exact
        // ends at the exact rational frame times.
        let comp = motion_comp(
            vec![
                boxed(0.0, 2.0, 0),
                text(0.0, 2.0, 0, 10.0),
                video(0.0, 2.0, 0),
            ],
            vec![
                tween(0, "x", TweenKind::To, None, Some(105.0), EaseDecl::Linear),
                tween(1, "y", TweenKind::From, Some(60.0), None, EaseDecl::Linear),
                tween(
                    2,
                    "x",
                    TweenKind::FromTo,
                    Some(0.0),
                    Some(90.0),
                    EaseDecl::Linear,
                ),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let desc = |k: u64| describe(&comp, &motions, k);
        // Starts: box authored 5, text given 60, video given 0.
        match &desc(0).items[0] {
            FrameItem::Box(r) => assert_eq!(r.x, 5.0),
            other => panic!("expected box first, got {other:?}"),
        }
        match &desc(0).items[1] {
            FrameItem::Text(r) => assert_eq!(r.y, 60.0),
            other => panic!("expected text, got {other:?}"),
        }
        match &desc(0).items[2] {
            FrameItem::Video(r) => assert_eq!(r.x, 0.0),
            other => panic!("expected video, got {other:?}"),
        }
        // Midpoint frame 15 (t=1/2): box 55, text 35, video 45.
        match &desc(15).items[0] {
            FrameItem::Box(r) => assert_eq!(r.x, 55.0),
            other => panic!("expected box, got {other:?}"),
        }
        match &desc(15).items[1] {
            FrameItem::Text(r) => assert_eq!(r.y, 35.0),
            other => panic!("expected text, got {other:?}"),
        }
        match &desc(15).items[2] {
            FrameItem::Video(r) => assert_eq!(r.x, 45.0),
            other => panic!("expected video, got {other:?}"),
        }
        // End frame 30 (t=1): box 105, text authored 10, video 90.
        match &desc(30).items[0] {
            FrameItem::Box(r) => assert_eq!(r.x, 105.0),
            other => panic!("expected box, got {other:?}"),
        }
        match &desc(30).items[1] {
            FrameItem::Text(r) => assert_eq!(r.y, 10.0),
            other => panic!("expected text, got {other:?}"),
        }
        match &desc(30).items[2] {
            FrameItem::Video(r) => assert_eq!(r.x, 90.0),
            other => panic!("expected video, got {other:?}"),
        }
    }

    #[test]
    fn motion_rides_text_and_video_descriptions() {
        // T009–T011: text and video clips carry evaluated scale,
        // rotation, opacity, and (for video) position.
        let comp = motion_comp(
            vec![text(0.0, 2.0, 0, 10.0), video(0.0, 2.0, 0)],
            vec![
                tween(0, "scale", TweenKind::To, None, Some(2.0), EaseDecl::Linear),
                tween(
                    0,
                    "opacity",
                    TweenKind::From,
                    Some(0.25),
                    None,
                    EaseDecl::Linear,
                ),
                tween(
                    1,
                    "x",
                    TweenKind::FromTo,
                    Some(0.0),
                    Some(60.0),
                    EaseDecl::Linear,
                ),
                tween(
                    1,
                    "rotation",
                    TweenKind::To,
                    None,
                    Some(45.0),
                    EaseDecl::Linear,
                ),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let desc = |k: u64| describe(&comp, &motions, k);
        match &desc(15).items[0] {
            FrameItem::Text(r) => {
                assert_eq!(r.scale, 1.5);
                assert_eq!(r.opacity, 0.625);
            }
            other => panic!("expected text, got {other:?}"),
        }
        match &desc(15).items[1] {
            FrameItem::Video(r) => {
                assert_eq!(r.x, 30.0);
                assert_eq!(r.rotation, 22.5);
                assert_eq!((r.scale, r.opacity), (1.0, 1.0));
            }
            other => panic!("expected video, got {other:?}"),
        }
    }

    #[test]
    fn span_only_text_resolves_with_spans_flag() {
        // Sprint 8 look (T002/T007): a value-less text with `span` runs
        // resolves an empty display value but still carries its runs,
        // so the painters must not apply the empty-value skip to it.
        // (Split, typewriter, and count never combine with spans: the
        // validator refuses them, so empty value plus spans is always
        // a still span text with ink to draw.)
        let mut clip = text(0.0, 2.0, 0, 10.0);
        let Clip::Text(tc) = &mut clip else {
            panic!("expected text")
        };
        tc.value.clear();
        tc.spans.push(notation::model::TextSpan {
            value: "Hi".to_string(),
            font_family: None,
            font_size: None,
            font_weight: None,
            italic: None,
            color: None,
            letter_spacing: None,
            line_height: None,
            children: Vec::new(),
            line: 1,
            column: 1,
        });
        let comp = motion_comp(vec![clip], vec![], vec![]);
        let motions = plan_motions(&comp, 30);
        match &describe(&comp, &motions, 0).items[0] {
            FrameItem::Text(r) => {
                assert!(r.value.is_empty());
                assert!(r.has_spans);
            }
            other => panic!("expected text, got {other:?}"),
        }
        // A plain value-less text without spans keeps the old shape:
        // empty value and no runs, so the painters still skip it.
        let mut bare = text(0.0, 2.0, 0, 10.0);
        let Clip::Text(tc) = &mut bare else {
            panic!("expected text")
        };
        tc.value.clear();
        let comp = motion_comp(vec![bare], vec![], vec![]);
        let motions = plan_motions(&comp, 30);
        match &describe(&comp, &motions, 0).items[0] {
            FrameItem::Text(r) => {
                assert!(r.value.is_empty());
                assert!(!r.has_spans);
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn scale_rotation_opacity_ride_in_descriptions() {
        // T010: from-to scale, to rotation, from opacity on a box.
        let comp = motion_comp(
            vec![boxed(0.0, 2.0, 0)],
            vec![
                tween(
                    0,
                    "scale",
                    TweenKind::FromTo,
                    Some(1.0),
                    Some(3.0),
                    EaseDecl::Linear,
                ),
                tween(
                    0,
                    "rotation",
                    TweenKind::To,
                    None,
                    Some(90.0),
                    EaseDecl::Linear,
                ),
                tween(
                    0,
                    "opacity",
                    TweenKind::From,
                    Some(0.0),
                    None,
                    EaseDecl::Linear,
                ),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let desc = |k: u64| describe(&comp, &motions, k);
        let at = |k: u64| match &desc(k).items[0] {
            FrameItem::Box(r) => (r.scale, r.rotation, r.opacity),
            other => panic!("expected box, got {other:?}"),
        };
        assert_eq!(at(0), (1.0, 0.0, 0.0));
        assert_eq!(at(15), (2.0, 45.0, 0.5));
        assert_eq!(at(30), (3.0, 90.0, 1.0));
    }

    #[test]
    fn set_windows_hold_and_release_at_edges() {
        // T011: x holds 50 for [0.5, 1.0): frame 15 already holds,
        // frame 29 still holds, frame 30 already released.
        let comp = motion_comp(
            vec![boxed(0.0, 2.0, 0)],
            vec![],
            vec![SetDecl {
                target: 0,
                property: "x".to_string(),
                value: 50.0,
                start: 0.5,
                duration: 0.5,
                line: 1,
                column: 1,
            }],
        );
        let motions = plan_motions(&comp, 30);
        let x = |k: u64| match &describe(&comp, &motions, k).items[0] {
            FrameItem::Box(r) => r.x,
            other => panic!("expected box, got {other:?}"),
        };
        assert_eq!(x(14), 5.0);
        assert_eq!(x(15), 50.0);
        assert_eq!(x(29), 50.0);
        assert_eq!(x(30), 5.0);
    }

    #[test]
    fn eased_midpoints_differ_between_curves() {
        // T012: in-quad and out-quad agree at the ends and differ in
        // the middle; out-back passes its end and settles exactly.
        let run = |ease: EaseDecl| -> (f64, f64, f64) {
            let comp = motion_comp(
                vec![boxed(0.0, 2.0, 0)],
                vec![tween(0, "x", TweenKind::To, None, Some(105.0), ease)],
                vec![],
            );
            let motions = plan_motions(&comp, 30);
            let x = |k: u64| match &describe(&comp, &motions, k).items[0] {
                FrameItem::Box(r) => r.x,
                other => panic!("expected box, got {other:?}"),
            };
            (x(0), x(15), x(30))
        };
        let (s0, m0, e0) = run(EaseDecl::Linear);
        let (s1, m1, e1) = run(EaseDecl::InQuad);
        let (s2, m2, e2) = run(EaseDecl::OutQuad);
        assert_eq!((s0, e0), (5.0, 105.0));
        assert_eq!((s1, e1), (5.0, 105.0));
        assert_eq!((s2, e2), (5.0, 105.0));
        assert_eq!(m0, 55.0);
        assert!(
            m1 < m0 && m0 < m2,
            "in-quad lags, out-quad leads: {m1} {m0} {m2}"
        );
        let comp = motion_comp(
            vec![boxed(0.0, 2.0, 0)],
            vec![tween(
                0,
                "x",
                TweenKind::To,
                None,
                Some(105.0),
                EaseDecl::OutBack(1.7),
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let peak = (0..30)
            .map(|k| match &describe(&comp, &motions, k).items[0] {
                FrameItem::Box(r) => r.x,
                other => panic!("expected box, got {other:?}"),
            })
            .fold(0.0f64, f64::max);
        assert!(peak > 105.0, "out-back passes its end, peak {peak}");
    }

    #[test]
    fn hidden_clips_leave_no_frame_item() {
        use crate::motion_plan::{frame_time, secs_to_time};
        // A hidden fullscreen box over the background draws nothing: the
        // frame holds the background with no items.
        let mut hide = boxed(0.0, 2.0, 0);
        match &mut hide {
            Clip::Box(b) => b.hidden = true,
            _ => unreachable!(),
        }
        let comp = comp_with(vec![hide]);
        let motions = plan_motions(&comp, 30);
        let view = NestView::single(&comp, &motions);
        for k in [0, 15, 59] {
            let desc = describe(&comp, &motions, k);
            assert!(desc.items.is_empty(), "hidden box at frame {k}");
            let at = describe_nested_at(&view, 0, k, secs_to_time(0.0) + frame_time(&comp.fps, k));
            assert!(at.items.is_empty(), "hidden box at ranged frame {k}");
        }
    }

    // --- Sprint 7 text effects and nested motion (T002 + T003) -----

    fn fx_text(
        value: &str,
        split: notation::model::SplitMode,
        typewriter: bool,
        count: Option<(f64, f64, u32)>,
    ) -> Clip {
        let mut c = text(0.0, 2.0, 0, 80.0);
        let Clip::Text(tc) = &mut c else {
            panic!("fx_text builds a text clip");
        };
        tc.value = value.to_string();
        tc.split = Some(split);
        tc.typewriter = typewriter;
        if let Some((from, to, decimals)) = count {
            tc.count_from = Some(from);
            tc.count_to = Some(to);
            tc.decimals = decimals;
        }
        c
    }

    fn text_value(desc: &FrameDescription) -> &str {
        match desc.items.as_slice() {
            [FrameItem::Text(r)] => r.value.as_str(),
            other => panic!("expected one text item, got {other:?}"),
        }
    }

    #[test]
    fn typewriter_reveal_shows_piece_prefixes_per_frame() {
        use notation::model::SplitMode;
        // Sprint 7 T002: "abcd" with chars + typewriter over [0, 2) at
        // 30 fps: frame 0 shows nothing, then one more piece every
        // half second, all four by the final frame.
        let comp = comp_with(vec![fx_text("abcd", SplitMode::Chars, true, None)]);
        let motions = plan_motions(&comp, 30);
        let desc = |k: u64| describe(&comp, &motions, k);
        assert_eq!(text_value(&desc(0)), "");
        assert_eq!(text_value(&desc(15)), "a");
        assert_eq!(text_value(&desc(30)), "ab");
        assert_eq!(text_value(&desc(45)), "abc");
        assert_eq!(text_value(&desc(59)), "abcd");
    }

    #[test]
    fn count_clips_show_the_interpolated_number() {
        use notation::model::SplitMode;
        // Sprint 7 T002: 0 to 100 over [0, 2) at 30 fps renders the
        // interpolated whole number; 0 to 1 with two decimals renders
        // the fractional value. The authored value never shows.
        let comp = comp_with(vec![fx_text(
            "ignored",
            SplitMode::Chars,
            false,
            Some((0.0, 100.0, 0)),
        )]);
        let motions = plan_motions(&comp, 30);
        let desc = |k: u64| describe(&comp, &motions, k);
        assert_eq!(text_value(&desc(0)), "0");
        assert_eq!(text_value(&desc(30)), "50");
        assert_eq!(text_value(&desc(59)), "98");

        let comp = comp_with(vec![fx_text(
            "ignored",
            SplitMode::Words,
            false,
            Some((0.0, 1.0, 2)),
        )]);
        let motions = plan_motions(&comp, 30);
        assert_eq!(text_value(&describe(&comp, &motions, 15)), "0.25");
        assert_eq!(text_value(&describe(&comp, &motions, 59)), "0.98");
    }

    #[test]
    fn nested_motion_plays_scaled_through_nest_rate() {
        use notation::model::{EaseDecl, NestClip, StaggerOrigin, TweenDecl, TweenKind};
        // Sprint 7 T003: a child tween from 0 to 100 over child [0, 2)
        // plays at double speed under a nest `rate=2` — parent t=0.5
        // (frame 15) already shows the child t=1.0 value.
        let mut child = comp_with(vec![boxed(0.0, 4.0, 0)]);
        child.duration = 4.0;
        child.name = "kid".to_string();
        let child_tween = TweenDecl {
            target: 0,
            property: "x".to_string(),
            kind: TweenKind::FromTo,
            from: Some(0.0),
            to: Some(100.0),
            start: 0.0,
            duration: 2.0,
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
            from_color: None,
            to_color: None,
        };
        child.tweens = vec![child_tween];
        let root = comp_with(vec![Clip::Nest(NestClip {
            target: "kid".to_string(),
            start: 0.0,
            duration: 2.0,
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            trim_start: 0.0,
            trim_end: None,
            rate: 2.0,
            order: 0,
            hidden: false,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
            line: 1,
            column: 1,
        })]);
        let root_motions = plan_motions(&root, 30);
        let child_motions = plan_motions(&child, 30);
        let view = NestView {
            comps: vec![&root, &child],
            motions: vec![&root_motions, &child_motions],
            bases: vec![0, 1],
        };
        let at = crate::motion_plan::frame_time(&root.fps, 15);
        let desc = describe_nested_at(&view, 0, 15, at);
        match desc.items.as_slice() {
            [FrameItem::Box(r)] => assert!(
                (r.x - 50.0).abs() < 1e-9,
                "rate 2 maps parent t=0.5 to child t=1.0, x=50, got {}",
                r.x
            ),
            other => panic!("expected one nested box, got {other:?}"),
        }
        // At rate 1 the same instant would show x=25: the scale comes
        // from the nest rate, not the child timeline alone.
        let at0 = crate::motion_plan::frame_time(&root.fps, 0);
        let desc0 = describe_nested_at(&view, 0, 0, at0);
        match desc0.items.as_slice() {
            [FrameItem::Box(r)] => assert!(
                (r.x - 0.0).abs() < 1e-9,
                "both timelines start at x=0, got {}",
                r.x
            ),
            other => panic!("expected one nested box, got {other:?}"),
        }
    }

    #[test]
    fn ranged_instants_match_full_render_instants_exactly() {
        use crate::motion_plan::{frame_time, secs_to_time};
        // Sprint 6 T008: a sub-range starting on a frame boundary draws
        // the identical description the full render draws at the same
        // composition instants — exact rational times, no float drift.
        let mut comp = comp_with(vec![text(0.0, 4.0, 0, 80.0), boxed(2.0, 2.0, 1)]);
        comp.duration = 4.0;
        let motions = plan_motions(&comp, 30);
        let view = NestView::single(&comp, &motions);
        let base = secs_to_time(1.0);
        for k in [0, 1, 29, 30, 59, 89] {
            let full = describe(&comp, &motions, 30 + k);
            let mut ranged = describe_nested_at(&view, 0, k, base + frame_time(&comp.fps, k));
            // The output index is the range's own; everything a renderer
            // consumes — the instant and the items — must match exactly.
            ranged.frame = full.frame;
            assert_eq!(
                ranged,
                full,
                "ranged frame {k} must equal full frame {}",
                30 + k
            );
        }
    }

    // --- Sprint 8 layout (T001): flex rows and columns ---

    fn flex_child(w: f64, h: f64, grow: f64) -> FlexChild {
        FlexChild {
            width: w,
            height: h,
            flex_grow: grow,
            fill: Rgb { r: 255, g: 0, b: 0 },
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: notation::model::StrokePosition::Inside,
            opacity: 1.0,
            order: 0,
            line: 1,
            column: 1,
        }
    }

    fn flexed(
        direction: FlexDirection,
        gap: f64,
        align: FlexAlign,
        justify: FlexJustify,
        kids: Vec<FlexChild>,
    ) -> Clip {
        Clip::Flex(FlexClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
            start: 0.0,
            duration: 2.0,
            x: 10.0,
            y: 20.0,
            width: 340.0,
            height: 100.0,
            direction,
            gap,
            padding: 0.0,
            margin: 0.0,
            align_items: align,
            justify_content: justify,
            order: 0,
            children: kids,
            line: 1,
            column: 1,
        })
    }

    fn boxes_of(desc: &FrameDescription) -> Vec<(f64, f64, f64, f64)> {
        desc.items
            .iter()
            .map(|item| match item {
                FrameItem::Box(b) => (b.x, b.y, b.width, b.height),
                other => panic!("flex must describe box items, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn flex_row_describes_hand_math_positions() {
        // Three 100-wide boxes with gap 10 from x 10: hand math lands
        // at 10, 120, 230 along y 20.
        let comp = comp_with(vec![flexed(
            FlexDirection::Row,
            10.0,
            FlexAlign::Start,
            FlexJustify::Start,
            vec![
                flex_child(100.0, 50.0, 0.0),
                flex_child(100.0, 50.0, 0.0),
                flex_child(100.0, 50.0, 0.0),
            ],
        )]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let got = boxes_of(&desc);
        assert_eq!(got.len(), 3);
        for (i, want_x) in [10.0, 120.0, 230.0].iter().enumerate() {
            assert!(
                (got[i].0 - want_x).abs() <= 1.0,
                "child {i} x: got {:?}, want {want_x}",
                got[i]
            );
            assert!((got[i].1 - 20.0).abs() <= 1.0, "{got:?}");
            assert!((got[i].2 - 100.0).abs() <= 1.0, "{got:?}");
            assert!((got[i].3 - 50.0).abs() <= 1.0, "{got:?}");
        }
    }

    #[test]
    fn flex_column_grow_describes_grown_sizes() {
        let mut comp = comp_with(vec![flexed(
            FlexDirection::Column,
            10.0,
            FlexAlign::Start,
            FlexJustify::Start,
            vec![
                flex_child(100.0, 50.0, 1.0),
                flex_child(100.0, 50.0, 2.0),
                flex_child(100.0, 50.0, 1.0),
            ],
        )]);
        if let Clip::Flex(f) = &mut comp.clips[0] {
            f.x = 0.0;
            f.y = 0.0;
            f.width = 200.0;
            f.height = 300.0;
            f.padding = 5.0;
        }
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let got = boxes_of(&desc);
        assert_eq!(got.len(), 3);
        // Content height 290, gaps 20, base 150: free 120 over 4
        // shares gives heights 80, 110, 80 at y 5, 95, 215.
        for (i, (want_y, want_h)) in [(5.0, 80.0), (95.0, 110.0), (215.0, 80.0)]
            .iter()
            .enumerate()
        {
            assert!(
                (got[i].1 - want_y).abs() <= 1.0,
                "child {i} y: got {:?}, want {want_y}",
                got[i]
            );
            assert!((got[i].3 - want_h).abs() <= 1.0, "{got:?}");
            assert!((got[i].0 - 5.0).abs() <= 1.0, "{got:?}");
        }
    }

    #[test]
    fn flex_centered_describes_centered_offsets() {
        let mut comp = comp_with(vec![flexed(
            FlexDirection::Row,
            0.0,
            FlexAlign::Center,
            FlexJustify::Center,
            vec![flex_child(100.0, 40.0, 0.0), flex_child(100.0, 40.0, 0.0)],
        )]);
        if let Clip::Flex(f) = &mut comp.clips[0] {
            f.x = 0.0;
            f.y = 0.0;
            f.width = 320.0;
        }
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let got = boxes_of(&desc);
        assert_eq!(got.len(), 2);
        // Leftover 120 centers the line at x 60; cross leftover 60
        // centers each 40-high child at y 30.
        assert!((got[0].0 - 60.0).abs() <= 1.0, "{got:?}");
        assert!((got[1].0 - 160.0).abs() <= 1.0, "{got:?}");
        for r in &got {
            assert!((r.1 - 30.0).abs() <= 1.0, "{got:?}");
        }
    }

    #[test]
    fn flex_empty_and_single_describe_cleanly_both_directions() {
        for direction in [FlexDirection::Row, FlexDirection::Column] {
            let comp = comp_with(vec![flexed(
                direction,
                8.0,
                FlexAlign::Center,
                FlexJustify::Center,
                vec![],
            )]);
            let motions = plan_motions(&comp, 30);
            assert!(describe(&comp, &motions, 0).items.is_empty());
            let comp = comp_with(vec![flexed(
                direction,
                8.0,
                FlexAlign::Center,
                FlexJustify::Center,
                vec![flex_child(50.0, 60.0, 0.0)],
            )]);
            let motions = plan_motions(&comp, 30);
            let desc = describe(&comp, &motions, 0);
            assert_eq!(desc.items.len(), 1);
        }
    }

    // --- Sprint 8 look (T003): gradient and image fills ---

    #[test]
    fn gradient_fill_paint_rides_the_frame_description() {
        let stops = vec![
            notation::model::GradientStop {
                offset: 0.0,
                color: Rgb { r: 255, g: 0, b: 0 },
                line: 1,
                column: 1,
            },
            notation::model::GradientStop {
                offset: 1.0,
                color: Rgb { r: 0, g: 0, b: 255 },
                line: 2,
                column: 1,
            },
        ];
        let paint = notation::model::FillPaint::Linear { angle: 90.0, stops };
        let mut clip = boxed(0.0, 2.0, 0);
        if let Clip::Box(b) = &mut clip {
            b.fill_paint = paint.clone();
        }
        let comp = comp_with(vec![clip]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        assert_eq!(desc.items.len(), 1);
        match &desc.items[0] {
            FrameItem::Box(r) => assert_eq!(r.fill_paint, paint),
            other => panic!("expected a box item, got {other:?}"),
        }
    }

    #[test]
    fn image_fill_paint_rides_the_frame_description() {
        let paint = notation::model::FillPaint::Image {
            src: "media/bg.png".to_string(),
            fit: notation::model::Fit::Cover,
            placement: notation::model::Placement::center(),
        };
        let mut clip = boxed(0.0, 2.0, 0);
        if let Clip::Box(b) = &mut clip {
            b.fill_paint = paint.clone();
        }
        let comp = comp_with(vec![clip]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        match &desc.items[0] {
            FrameItem::Box(r) => assert_eq!(r.fill_paint, paint),
            other => panic!("expected a box item, got {other:?}"),
        }
    }

    // --- Sprint 11 grade (T001): inward-out level resolution ---

    fn grade(
        preset: Option<notation::model::GradePreset>,
        exposure: Option<f64>,
        contrast: Option<f64>,
        saturation: Option<f64>,
        temperature: Option<f64>,
        tint: Option<f64>,
    ) -> notation::model::Grade {
        notation::model::Grade {
            preset,
            exposure,
            contrast,
            saturation,
            temperature,
            tint,
            ..Default::default()
        }
    }

    fn boxed_on_track(track: TrackKind, start: f64, duration: f64, order: i64) -> Clip {
        let mut clip = boxed(start, duration, order);
        if let Clip::Box(b) = &mut clip {
            b.track = track;
        }
        clip
    }

    #[test]
    fn grade_resolves_inward_out_with_clip_winning() {
        use notation::model::GradePreset;
        // Composition filmic +1 EV, lane overrides exposure, clip
        // overrides exposure again and clears the preset: the clip's
        // pieces win, the lane's saturation and the composition's
        // contrast inherit.
        let mut comp = comp_with(vec![boxed_on_track(TrackKind::Video, 0.0, 2.0, 0)]);
        comp.grade = grade(Some(GradePreset::Filmic), Some(1.0), Some(0.2), None, None, None);
        comp.lane_grades = vec![notation::model::LaneGrade {
            track: TrackKind::Video,
            grade: grade(None, Some(-0.5), None, Some(0.5), None, None),
        }];
        if let Clip::Box(b) = &mut comp.clips[0] {
            b.grade = grade(
                Some(GradePreset::None),
                Some(-1.0),
                None,
                None,
                Some(0.25),
                None,
            );
        }
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        assert_eq!(desc.items.len(), 1);
        let resolved = desc.grade_for(0);
        assert_eq!(resolved.preset, GradePreset::None);
        assert_eq!(resolved.exposure, -1.0);
        assert_eq!(resolved.contrast, 0.2);
        assert_eq!(resolved.saturation, 0.5);
        assert_eq!(resolved.temperature, 0.25);
        assert_eq!(resolved.tint, 0.0);
        assert_eq!(desc.grades.len(), 1);
        assert_eq!(desc.grades[0].doc, 0);
        // The composition grade alone rides the background entry.
        assert_eq!(desc.composition_grade.preset, GradePreset::Filmic);
        assert_eq!(desc.composition_grade.exposure, 1.0);
    }

    #[test]
    fn grade_lane_applies_per_track_only() {
        // A video-lane grade touches the video-track box only; the
        // graphics-track box keeps the composition look.
        let mut comp = comp_with(vec![
            boxed_on_track(TrackKind::Video, 0.0, 2.0, 0),
            boxed_on_track(TrackKind::Graphics, 0.0, 2.0, 1),
        ]);
        comp.grade = grade(None, Some(0.5), None, None, None, None);
        comp.lane_grades = vec![notation::model::LaneGrade {
            track: TrackKind::Video,
            grade: grade(None, Some(2.0), None, None, None, None),
        }];
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        assert_eq!(desc.grade_for(0).exposure, 2.0);
        assert_eq!(desc.grade_for(1).exposure, 0.5);
    }

    #[test]
    fn grade_nest_use_layers_parent_chain_outside_child() {
        use notation::model::{GradePreset, NestClip};
        // A nested item resolves under the nest use: the root
        // composition grade, the root lane for the nest clip's track,
        // and the nest clip's own grade form the outer chain the child
        // composition layers under.
        let mut child = comp_with(vec![boxed(0.0, 2.0, 0)]);
        child.name = "kid".to_string();
        if let Clip::Box(b) = &mut child.clips[0] {
            b.grade = grade(None, Some(1.0), None, None, None, None);
        }
        let mut root = comp_with(vec![Clip::Nest(NestClip {
            target: "kid".to_string(),
            start: 0.0,
            duration: 2.0,
            id: None,
            track: TrackKind::Video,
            label: None,
            start_after: None,
            trim_start: 0.0,
            trim_end: None,
            rate: 1.0,
            order: 0,
            hidden: false,
            grade: grade(None, None, None, Some(-0.5), None, None),
            effects: Vec::new(),
            vfx: Default::default(),
            line: 1,
            column: 1,
        })]);
        root.grade = grade(None, None, None, None, Some(0.5), None);
        root.lane_grades = vec![notation::model::LaneGrade {
            track: TrackKind::Video,
            grade: grade(None, None, Some(0.3), None, None, None),
        }];
        let root_motions = plan_motions(&root, 30);
        let child_motions = plan_motions(&child, 30);
        let view = NestView {
            comps: vec![&root, &child],
            motions: vec![&root_motions, &child_motions],
            bases: vec![0, 1],
        };
        let at = crate::motion_plan::frame_time(&root.fps, 0);
        let desc = describe_nested_at(&view, 0, 0, at);
        assert_eq!(desc.items.len(), 1);
        // The child box is the only item, at global doc 1.
        let resolved = desc.grade_for(1);
        assert_eq!(resolved.preset, GradePreset::None);
        assert_eq!(resolved.exposure, 1.0);
        assert_eq!(resolved.contrast, 0.3);
        assert_eq!(resolved.saturation, -0.5);
        assert_eq!(resolved.temperature, 0.5);
        assert_eq!(resolved.tint, 0.0);
    }

    #[test]
    fn vfx_nest_use_merges_inner_wins_per_stage() {
        use notation::model::{NestClip, VfxChain, VfxNoise, VfxWarp};
        // The nested box defines noise; the nest use defines noise
        // (losing seed) and warp. The merged chain keeps the inner
        // noise and inherits the outer warp.
        let mut child = comp_with(vec![boxed(0.0, 2.0, 0)]);
        child.name = "kid".to_string();
        if let Clip::Box(b) = &mut child.clips[0] {
            b.vfx = VfxChain {
                noise: Some(VfxNoise { amount: 0.5, seed: Some(7) }),
                ..VfxChain::empty()
            };
        }
        let root = comp_with(vec![Clip::Nest(NestClip {
            target: "kid".to_string(),
            start: 0.0,
            duration: 2.0,
            id: None,
            track: TrackKind::Video,
            label: None,
            start_after: None,
            trim_start: 0.0,
            trim_end: None,
            rate: 1.0,
            order: 0,
            hidden: false,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: VfxChain {
                noise: Some(VfxNoise { amount: 1.0, seed: Some(9) }),
                warp: Some(VfxWarp { amount: 1.0, grid: Some(8), strength: None }),
                ..VfxChain::empty()
            },
            line: 1,
            column: 1,
        })]);
        let root_motions = plan_motions(&root, 30);
        let child_motions = plan_motions(&child, 30);
        let view = NestView {
            comps: vec![&root, &child],
            motions: vec![&root_motions, &child_motions],
            bases: vec![0, 1],
        };
        let at = crate::motion_plan::frame_time(&root.fps, 0);
        let desc = describe_nested_at(&view, 0, 0, at);
        assert_eq!(desc.items.len(), 1);
        let vfx = desc.vfx_for(1);
        // Inner noise wins over the outer noise.
        assert_eq!(vfx.noise.as_ref().expect("inner noise").seed, Some(7));
        assert_eq!(vfx.noise.as_ref().expect("inner noise").amount, 0.5);
        // Missing warp inherits the outer one.
        assert_eq!(vfx.warp.as_ref().expect("outer warp").grid, Some(8));
        assert!(vfx.displacement.is_none());
        assert!(vfx.luma_matte.is_none());
    }

    #[test]
    fn grade_missing_levels_are_skipped_never_zeroed() {
        // No lane grade, no clip grade: the composition values stand.
        // An unknown doc id resolves to the identity look.
        let mut comp = comp_with(vec![boxed(0.0, 2.0, 0)]);
        comp.grade = grade(
            Some(notation::model::GradePreset::Noir),
            None,
            None,
            Some(-0.5),
            None,
            None,
        );
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let resolved = desc.grade_for(0);
        assert_eq!(resolved.preset, notation::model::GradePreset::Noir);
        assert_eq!(resolved.saturation, -0.5);
        assert_eq!(resolved.exposure, 0.0);
        assert_eq!(desc.grade_for(99), notation::model::ResolvedGrade::identity());
    }
}
