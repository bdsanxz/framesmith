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
use notation::model::{Clip, Composition, Fit, Placement, Rgb};
use timebase::Time;

/// One visible text element at one frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedText {
    /// Document order index into the composition clips.
    pub doc: usize,
    pub order: i64,
    pub value: String,
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
    pub radius: f64,
    pub border_width: f64,
    pub border_color: Rgb,
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

/// One visible element at one frame time, in paint order.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameItem {
    Text(ResolvedText),
    Video(ResolvedVideo),
    Box(ResolvedBox),
    Image(ResolvedImage),
}

impl FrameItem {
    pub fn doc(&self) -> usize {
        match self {
            FrameItem::Text(r) => r.doc,
            FrameItem::Video(r) => r.doc,
            FrameItem::Box(r) => r.doc,
            FrameItem::Image(r) => r.doc,
        }
    }

    pub fn order(&self) -> i64 {
        match self {
            FrameItem::Text(r) => r.order,
            FrameItem::Video(r) => r.order,
            FrameItem::Box(r) => r.order,
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
    let comp = view.comps[root];
    let mut items = collect_items(view, root, now, 0);
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
    }
}

/// Collect the visible items of one composition at one exact time,
/// resolving `nest` clips recursively (Sprint 6 T007). Document ids are
/// global (`bases[ci] + local index`) so the items address the unified
/// prepared media. Unresolvable targets and empty child timelines
/// contribute nothing; valid projects never hit the depth cap.
fn collect_items(view: &NestView, ci: usize, now: Time, depth: u8) -> Vec<FrameItem> {
    use crate::motion_plan::secs_to_time;
    if depth > MAX_NEST_RENDER_DEPTH {
        return Vec::new();
    }
    let comp = view.comps[ci];
    let base = view.bases[ci];
    let t = now.to_f64();
    let mut items: Vec<FrameItem> = Vec::new();
    for (local, clip) in comp.clips.iter().enumerate() {
        // Hidden clips contribute no picture (Sprint 6 T009): the
        // frame shows them as if absent. Sound exclusion rides in the
        // audio plan; ids and windows keep validating regardless.
        if clip.hidden() {
            continue;
        }
        let doc = base + local;
        let start_t = secs_to_time(clip.start());
        let end_t = start_t + secs_to_time(clip.duration());
        if now < start_t || now >= end_t {
            continue;
        }
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
            Clip::Text(tc) => items.push(FrameItem::Text(ResolvedText {
                doc,
                order: tc.order,
                // Sprint 7 text effects (CAP-009): a typewriter reveal
                // or count range shows its own string at this instant;
                // still and merely-split clips show the authored value.
                value: crate::textfx::display_value(tc, t),
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
            })),
            Clip::Video(v) => items.push(FrameItem::Video(ResolvedVideo {
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
            })),
            Clip::Box(b) => items.push(FrameItem::Box(ResolvedBox {
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
                radius: eval.radius,
                border_width: eval.border_width,
                border_color: Rgb {
                    r: channel_byte(eval.border[0]),
                    g: channel_byte(eval.border[1]),
                    b: channel_byte(eval.border[2]),
                },
                opacity: eval.opacity,
                scale: eval.scale,
                rotation: eval.rotation,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: b.turn,
            })),
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
                for mut item in collect_items(view, ti, child_now, depth + 1) {
                    item.shift_order(n.order);
                    items.push(item);
                }
            }
            Clip::Image(im) => items.push(FrameItem::Image(ResolvedImage {
                doc,
                order: im.order,
                path: im.path.clone(),
                x: eval.x,
                y: eval.y,
                // Animated extents win when a `width`/`height` tween or
                // set names the clip; otherwise the authored fields
                // stand, exactly as before (`None` still fills the
                // canvas downstream).
                width: match (im.width, touches(Prop::Width)) {
                    (None, false) => None,
                    (Some(_), false) => im.width,
                    (_, true) => Some(eval.width),
                },
                height: match (im.height, touches(Prop::Height)) {
                    (None, false) => None,
                    (Some(_), false) => im.height,
                    (_, true) => Some(eval.height),
                },
                fit: im.fit,
                placement: im.placement,
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
                rotate_x: eval.rotate_x,
                rotate_y: eval.rotate_y,
                blur: eval.blur,
                turn: im.turn,
            })),
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion_plan::plan_motions;
    use notation::model::{
        Align, BoxClip, Composition, FontStyle, Fps, Rgb, TextClip, TrackKind, VideoClip, Wrap,
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
            replicates: Vec::new(),
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
            split: None,
            typewriter: false,
            count_from: None,
            count_to: None,
            decimals: 0,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
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
            trim_start: 0.0,
            trim_end: None,
            rate: 1.0,
            loop_clip: false,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
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
            radius: 2.0,
            border_width: 1.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            opacity: 1.0,
            order,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
        })
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
            trim_start: 1.0,
            trim_end: Some(3.0),
            rate: 2.0,
            loop_clip: false,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
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
            replicates: Vec::new(),
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
}
