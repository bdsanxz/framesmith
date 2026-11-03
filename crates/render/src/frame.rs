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
    pub size: f64,
    pub color: Rgb,
    pub style: notation::model::FontStyle,
    pub line_height: Option<f64>,
    pub letter_spacing: Option<f64>,
    /// Evaluated motion (T010): uniform scale about the top-left,
    /// rotation in degrees about the top-left, opacity 0 to 1.
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
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
}

/// One visible box element at one frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBox {
    pub doc: usize,
    pub order: i64,
    pub x: f64,
    pub y: f64,
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
}

/// One visible image element at one frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedImage {
    pub doc: usize,
    pub order: i64,
    pub path: String,
    pub x: f64,
    pub y: f64,
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
}

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
pub fn describe(comp: &Composition, motions: &[ClipMotion], frame: u64) -> FrameDescription {
    use crate::motion_plan::frame_time;
    let now = frame_time(&comp.fps, frame);
    let t = now.to_f64();
    let mut items: Vec<FrameItem> = Vec::new();
    for (doc, clip) in comp.clips.iter().enumerate() {
        if !notation::model::clip_visible(clip.start(), clip.duration(), t) {
            continue;
        }
        let eval = motions.get(doc).map(|m| m.values_at(now)).unwrap_or_else(|| {
            // No motion entry: the clip holds its own values.
            let own = crate::motion_plan::own_values(clip);
            crate::motion_plan::Evaluated {
                x: own[Prop::X.index()],
                y: own[Prop::Y.index()],
                scale: own[Prop::Scale.index()],
                rotation: own[Prop::Rotation.index()],
                opacity: own[Prop::Opacity.index()],
            }
        });
        match clip {
            Clip::Text(tc) => items.push(FrameItem::Text(ResolvedText {
                doc,
                order: tc.order,
                value: tc.value.clone(),
                x: eval.x,
                y: eval.y,
                size: tc.size,
                color: tc.color,
                style: tc.style,
                line_height: tc.line_height,
                letter_spacing: tc.letter_spacing,
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
            })),
            Clip::Video(v) => items.push(FrameItem::Video(ResolvedVideo {
                doc,
                order: v.order,
                src_t: (t - v.start).max(0.0),
                fit: v.fit,
                x: eval.x,
                y: eval.y,
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
            })),
            Clip::Box(b) => items.push(FrameItem::Box(ResolvedBox {
                doc,
                order: b.order,
                x: eval.x,
                y: eval.y,
                width: b.width,
                height: b.height,
                fill: b.fill,
                radius: b.radius,
                border_width: b.border_width,
                border_color: b.border_color,
                opacity: eval.opacity,
                scale: eval.scale,
                rotation: eval.rotation,
            })),
            // Audio tracks draw no picture: they ride in the composition
            // for the T008 mixer and never emit a frame item.
            Clip::Audio(_) => {}
            Clip::Image(im) => items.push(FrameItem::Image(ResolvedImage {
                doc,
                order: im.order,
                path: im.path.clone(),
                x: eval.x,
                y: eval.y,
                width: im.width,
                height: im.height,
                fit: im.fit,
                placement: im.placement,
                scale: eval.scale,
                rotation: eval.rotation,
                opacity: eval.opacity,
            })),
        }
    }
    // Written order decides; document order breaks ties, and no other
    // stacking (element kind or timing) overrides it. `sort_by` is
    // stable, so equal orders keep document order.
    items.sort_by_key(|item| item.order());
    FrameDescription {
        frame,
        time: now,
        t,
        width: comp.width,
        height: comp.height,
        background: comp.background,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion_plan::plan_motions;
    use notation::model::{Align, BoxClip, Composition, FontStyle, Fps, Rgb, TextClip, VideoClip, Wrap};

    fn comp_with(clips: Vec<Clip>) -> Composition {
        Composition {
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
        }
    }

    fn text(start: f64, duration: f64, order: i64, x: f64) -> Clip {
        Clip::Text(TextClip {
            value: "Hi".to_string(),
            start,
            duration,
            x,
            y: 10.0,
            size: 24.0,
            color: Rgb { r: 255, g: 255, b: 255 },
            align: Align::Left,
            wrap: Wrap::Off,
            style: FontStyle::Regular,
            order,
            line_height: None,
            letter_spacing: Some(2.0),
            font_family: None,
            line: 1,
            column: 1,
        })
    }

    fn video(start: f64, duration: f64, order: i64) -> Clip {
        Clip::Video(VideoClip {
            path: "clip.mp4".to_string(),
            start,
            duration,
            fit: Fit::Contain,
            order,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            line: 1,
            column: 1,
        })
    }

    fn boxed(start: f64, duration: f64, order: i64) -> Clip {
        Clip::Box(BoxClip {
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
    fn written_order_wins_over_document_order() {
        // Box written first in the document but ordered after the text.
        let comp = comp_with(vec![boxed(0.0, 2.0, 5), text(0.0, 2.0, -1, 80.0), video(0.0, 2.0, 5)]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        let docs: Vec<usize> = desc.items.iter().map(FrameItem::doc).collect();
        // order -1 first, then the two order-5 clips in document order.
        assert_eq!(docs, vec![1, 0, 2], "paint follows written order, ties keep document order");
    }

    #[test]
    fn evaluated_positions_ride_in_the_description() {
        let comp = comp_with(vec![text(0.0, 2.0, 0, 80.0)]);
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 7);
        match &desc.items[0] {
            FrameItem::Text(r) => {
                assert!((r.x - 80.0).abs() < 1e-9, "still clips hold authored values, got {}", r.x);
                assert!((r.y - 10.0).abs() < 1e-9);
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    // --- Sprint 2 motion descriptions (T006, T009–T012) ---------------

    use notation::model::{EaseDecl, SetDecl, TweenDecl, TweenKind};

    fn image_clip(start: f64, duration: f64, order: i64) -> Clip {
        Clip::Image(notation::model::ImageClip {
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
        })
    }

    fn motion_comp(clips: Vec<Clip>, tweens: Vec<TweenDecl>, sets: Vec<SetDecl>) -> Composition {
        Composition {
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
            line: 1,
            column: 1,
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
        assert_eq!(docs, vec![2, 1, 3, 0], "order decides, ties keep document order");
    }

    #[test]
    fn equal_orders_paint_in_document_order() {
        // T006: clips with equal (or absent-equal default) orders keep
        // document order even when kinds interleave.
        let comp = motion_comp(
            vec![video(0.0, 2.0, 0), boxed(0.0, 2.0, 0), text(0.0, 2.0, 0, 80.0)],
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
            vec![boxed(0.0, 2.0, 0), text(0.0, 2.0, 0, 10.0), video(0.0, 2.0, 0)],
            vec![
                tween(0, "x", TweenKind::To, None, Some(105.0), EaseDecl::Linear),
                tween(1, "y", TweenKind::From, Some(60.0), None, EaseDecl::Linear),
                tween(2, "x", TweenKind::FromTo, Some(0.0), Some(90.0), EaseDecl::Linear),
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
                tween(0, "opacity", TweenKind::From, Some(0.25), None, EaseDecl::Linear),
                tween(1, "x", TweenKind::FromTo, Some(0.0), Some(60.0), EaseDecl::Linear),
                tween(1, "rotation", TweenKind::To, None, Some(45.0), EaseDecl::Linear),
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
                tween(0, "scale", TweenKind::FromTo, Some(1.0), Some(3.0), EaseDecl::Linear),
                tween(0, "rotation", TweenKind::To, None, Some(90.0), EaseDecl::Linear),
                tween(0, "opacity", TweenKind::From, Some(0.0), None, EaseDecl::Linear),
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
        assert!(m1 < m0 && m0 < m2, "in-quad lags, out-quad leads: {m1} {m0} {m2}");
        let comp = motion_comp(
            vec![boxed(0.0, 2.0, 0)],
            vec![tween(0, "x", TweenKind::To, None, Some(105.0), EaseDecl::OutBack(1.7))],
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
}
