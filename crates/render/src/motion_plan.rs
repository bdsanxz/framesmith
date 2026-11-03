//! The render clock and per-element motion: exact frame times from
//! `timebase` and per-clip `motion` evaluation.
//!
//! Validated Sprint 2 `tween`/`set` directives ride in the composition
//! and are fed into the motion engine here: tweens become engine
//! tweens over the clip's own values (to/from/from-to with the declared
//! easing), while sets layer on top as stepwise half-open holds with
//! later-wins resolution. Evaluation is always at the exact frame time,
//! so one frame number always yields one value.

use motion::{ElementMotion, Prop, Tween, PROP_COUNT};
use notation::model::{Clip, Composition, EaseDecl, Fps, SetDecl, TweenDecl, TweenKind};
use timebase::Time;

/// Integer frame rate for grid snapping: exact for integer fps, the
/// nearest whole rate for rational fps (tween timing refines to the
/// exact grid when the notation migrates to `Time`).
pub fn snap_fps(fps: &Fps) -> u32 {
    if fps.den == 1 {
        (fps.num.max(1) as u64).min(240) as u32
    } else {
        (((fps.num + fps.den / 2) / fps.den.max(1)).max(1).min(240)) as u32
    }
}

/// Exact time of output frame `k`: frame count over the exact rate, in
/// lowest terms. No float drift across the composition.
pub fn frame_time(fps: &Fps, k: u64) -> Time {
    Time::new(k as i128 * fps.den as i128, fps.num.max(1) as i128)
}

/// The element's own value of each animated property: authored position
/// for text, box, and image clips, authored opacity for boxes, unit
/// scales and full opacity otherwise. A clip holds these before and
/// without tweens.
pub fn own_values(clip: &Clip) -> [f64; PROP_COUNT] {
    let mut own = [0.0f64; PROP_COUNT];
    for (i, prop) in motion::PROPS.iter().enumerate() {
        own[i] = prop.own_value();
    }
    match clip {
        Clip::Text(tc) => {
            own[Prop::X.index()] = tc.x;
            own[Prop::Y.index()] = tc.y;
        }
        Clip::Box(bc) => {
            own[Prop::X.index()] = bc.x;
            own[Prop::Y.index()] = bc.y;
            own[Prop::Opacity.index()] = bc.opacity;
        }
        Clip::Image(ic) => {
            own[Prop::X.index()] = ic.x;
            own[Prop::Y.index()] = ic.y;
        }
        Clip::Video(_) | Clip::Audio(_) => {}
    }
    own
}

/// The exact decimal reading of written seconds as a [`Time`]: the
/// shortest round-trip decimal of `v` is read exactly (so `1.1` means
/// `11/10`, aligned with exact frame times), with a nanosecond-scale
/// fallback for magnitudes no decimal spelling covers.
pub fn secs_to_time(v: f64) -> Time {
    let text = format!("{v:?}");
    if let Some(t) = Time::parse(&text) {
        return t;
    }
    Time::new((v * 1_000_000_000.0).round() as i128, 1_000_000_000)
}

/// The Sprint 2 tweenable property of a validated directive name.
/// Validation guarantees one of the five; anything else is ignored.
fn prop_of(name: &str) -> Option<Prop> {
    match name {
        "x" => Some(Prop::X),
        "y" => Some(Prop::Y),
        "scale" => Some(Prop::Scale),
        "rotation" => Some(Prop::Rotation),
        "opacity" => Some(Prop::Opacity),
        _ => None,
    }
}

fn ease_of(decl: &EaseDecl) -> motion::Ease {
    use motion::Ease;
    match *decl {
        EaseDecl::Linear => Ease::Linear,
        EaseDecl::InQuad => Ease::InQuad,
        EaseDecl::OutQuad => Ease::OutQuad,
        EaseDecl::InOutQuad => Ease::InOutQuad,
        EaseDecl::InCubic => Ease::InCubic,
        EaseDecl::OutCubic => Ease::OutCubic,
        EaseDecl::InOutCubic => Ease::InOutCubic,
        EaseDecl::InQuart => Ease::InQuart,
        EaseDecl::OutQuart => Ease::OutQuart,
        EaseDecl::InOutQuart => Ease::InOutQuart,
        EaseDecl::InQuint => Ease::InQuint,
        EaseDecl::OutQuint => Ease::OutQuint,
        EaseDecl::InOutQuint => Ease::InOutQuint,
        EaseDecl::InSine => Ease::InSine,
        EaseDecl::OutSine => Ease::OutSine,
        EaseDecl::InOutSine => Ease::InOutSine,
        EaseDecl::InExpo => Ease::InExpo,
        EaseDecl::OutExpo => Ease::OutExpo,
        EaseDecl::InOutExpo => Ease::InOutExpo,
        EaseDecl::OutBack(n) => Ease::OutBack(n),
    }
}

/// A validated `tween` as an engine tween: one property, exact window,
/// declared easing, written order preserved by the caller.
pub fn engine_tween(decl: &TweenDecl) -> Option<Tween> {
    let prop = prop_of(&decl.property)?;
    let kind = match decl.kind {
        TweenKind::To => motion::Kind::To,
        TweenKind::From => motion::Kind::From,
        TweenKind::FromTo => motion::Kind::FromTo,
    };
    let mut from = [None; PROP_COUNT];
    let mut to = [None; PROP_COUNT];
    if let Some(v) = decl.from {
        from[prop.index()] = Some(v);
    }
    if let Some(v) = decl.to {
        to[prop.index()] = Some(v);
    }
    Some(Tween {
        target: decl.target.to_string(),
        kind,
        from,
        to,
        at: secs_to_time(decl.start),
        duration: secs_to_time(decl.duration),
        ease: ease_of(&decl.ease),
    })
}

/// One validated `set` as a windowed hold: the property holds `value`
/// exactly for `start <= t < end`, in written order for later-wins.
#[derive(Clone, Debug)]
pub struct SetWindow {
    pub prop: Prop,
    pub start: Time,
    pub end: Time,
    pub value: f64,
}

pub fn set_window(decl: &SetDecl) -> Option<SetWindow> {
    let prop = prop_of(&decl.property)?;
    let start = secs_to_time(decl.start);
    let end = secs_to_time(decl.start + decl.duration);
    Some(SetWindow { prop, start, end: end.max(start), value: decl.value })
}

/// Every animated value of one clip at one exact time (T009–T011):
/// tweened (or authored) values with windowed sets applied on top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evaluated {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
}

/// One clip's motion: its evaluator over its tweens, its stepwise set
/// windows, plus its own values for offset math.
pub struct ClipMotion {
    element: ElementMotion,
    sets: Vec<SetWindow>,
    ox: f64,
    oy: f64,
    own_scale: f64,
    own_rotation: f64,
    own_opacity: f64,
}

impl ClipMotion {
    /// Motion of clip `index` with no written tweens: every animated
    /// value is the clip's own value at every time.
    pub fn still(index: usize, clip: &Clip, fps: u32) -> ClipMotion {
        let own = own_values(clip);
        ClipMotion {
            element: ElementMotion::build(index, &[], own, fps),
            sets: Vec::new(),
            ox: own[Prop::X.index()],
            oy: own[Prop::Y.index()],
            own_scale: own[Prop::Scale.index()],
            own_rotation: own[Prop::Rotation.index()],
            own_opacity: own[Prop::Opacity.index()],
        }
    }

    /// Motion of clip `index` with written engine `tweens`: the
    /// evaluator blends them over the clip's own values.
    pub fn with_tweens(index: usize, clip: &Clip, tweens: &[Tween], fps: u32) -> ClipMotion {
        let own = own_values(clip);
        ClipMotion {
            element: ElementMotion::build(index, tweens, own, fps),
            sets: Vec::new(),
            ox: own[Prop::X.index()],
            oy: own[Prop::Y.index()],
            own_scale: own[Prop::Scale.index()],
            own_rotation: own[Prop::Rotation.index()],
            own_opacity: own[Prop::Opacity.index()],
        }
    }

    /// Motion of clip `index` with validated Sprint 2 directives:
    /// engine tweens below, windowed sets layered on top.
    pub fn with_directives(
        index: usize,
        clip: &Clip,
        tweens: &[Tween],
        sets: &[SetWindow],
        fps: u32,
    ) -> ClipMotion {
        let own = own_values(clip);
        ClipMotion {
            element: ElementMotion::build(index, tweens, own, fps),
            sets: sets.to_vec(),
            ox: own[Prop::X.index()],
            oy: own[Prop::Y.index()],
            own_scale: own[Prop::Scale.index()],
            own_rotation: own[Prop::Rotation.index()],
            own_opacity: own[Prop::Opacity.index()],
        }
    }

    fn tracked(&self, prop: Prop, t: Time, own: f64) -> f64 {
        let base = self.element.values(t)[prop.index()].unwrap_or(own);
        // Windowed sets win over the tweened value, with no easing and
        // no blending; overlapping sets resolve to the later-written
        // value wherever they overlap.
        let mut held: Option<f64> = None;
        for window in &self.sets {
            if window.prop == prop && window.start <= t && t < window.end {
                held = Some(window.value);
            }
        }
        held.unwrap_or(base)
    }

    /// Every animated value of the clip at `t`.
    pub fn values_at(&self, t: Time) -> Evaluated {
        Evaluated {
            x: self.tracked(Prop::X, t, self.ox),
            y: self.tracked(Prop::Y, t, self.oy),
            scale: self.tracked(Prop::Scale, t, self.own_scale),
            rotation: self.tracked(Prop::Rotation, t, self.own_rotation),
            opacity: self.tracked(Prop::Opacity, t, self.own_opacity),
        }
    }

    /// Position offset at `t`: evaluated x/y minus the clip's own x/y.
    /// Zero for still clips; the exact tween blend once written.
    pub fn offset_at(&self, t: Time) -> (f64, f64) {
        let v = self.values_at(t);
        (v.x - self.ox, v.y - self.oy)
    }
}

/// Motions of all clips of a composition, in clip order: each clip's
/// own tweens (targets naming it) feed its evaluator in written order,
/// and each clip's own sets layer on top in written order.
pub fn plan_motions(comp: &Composition, fps: u32) -> Vec<ClipMotion> {
    comp.clips
        .iter()
        .enumerate()
        .map(|(i, clip)| {
            let tweens: Vec<Tween> = comp
                .tweens
                .iter()
                .filter(|decl| decl.target == i)
                .filter_map(engine_tween)
                .collect();
            let sets: Vec<SetWindow> = comp
                .sets
                .iter()
                .filter(|decl| decl.target == i)
                .filter_map(set_window)
                .collect();
            ClipMotion::with_directives(i, clip, &tweens, &sets, fps)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion::{Ease, Kind};
    use notation::model::{Align, FontStyle, Rgb, TextClip, Wrap};

    fn text_clip(x: f64, y: f64) -> Clip {
        Clip::Text(TextClip {
            value: "Hi".to_string(),
            start: 0.0,
            duration: 2.0,
            x,
            y,
            size: 48.0,
            color: Rgb { r: 255, g: 255, b: 255 },
            align: Align::Left,
            wrap: Wrap::Off,
            style: FontStyle::Regular,
            order: 0,
            line_height: None,
            letter_spacing: None,
            font_family: None,
            line: 1,
            column: 1,
        })
    }

    #[test]
    fn frame_times_are_exact() {
        let fps = Fps::from_int(30);
        assert_eq!(frame_time(&fps, 0).to_string(), "0");
        assert_eq!(frame_time(&fps, 90).to_string(), "3");
        // Rational rate stays exact: 30000 frames at 30000/1001.
        let ntsc = Fps { num: 30000, den: 1001 };
        assert_eq!(frame_time(&ntsc, 30000).to_string(), "1001");
    }

    #[test]
    fn still_clips_hold_zero_offset() {
        let clip = text_clip(80.0, 320.0);
        let motion = ClipMotion::still(0, &clip, 30);
        for k in [0, 15, 30, 60] {
            let (dx, dy) = motion.offset_at(Time::of_frame(k, 30));
            assert!((dx, dy) == (0.0, 0.0), "still clip must not drift at frame {k}");
        }
    }

    #[test]
    fn written_tween_blends_position() {
        let clip = text_clip(10.0, 20.0);
        let tween = Tween {
            target: "t".to_string(),
            kind: Kind::To,
            from: [None; PROP_COUNT],
            to: {
                let mut to = [None; PROP_COUNT];
                to[Prop::X.index()] = Some(110.0);
                to
            },
            at: Time::from_secs(1),
            duration: Time::from_secs(1),
            ease: Ease::Linear,
        };
        let motion = ClipMotion::with_tweens(0, &clip, &[tween], 30);
        // At the tween midpoint the clip has moved half the distance.
        let (dx, _) = motion.offset_at(Time::new(3, 2));
        assert!((dx - 50.0).abs() < 1e-9, "halfway tween must offset x by 50, got {dx}");
        // Before the tween the clip holds its own position.
        let (dx0, _) = motion.offset_at(Time::ZERO);
        assert!((dx0).abs() < 1e-9, "got {dx0}");
    }

    // --- Sprint 2 directive evaluation (T009–T012) --------------------

    fn box_clip(x: f64, opacity: f64) -> Clip {
        Clip::Box(notation::model::BoxClip {
            start: 0.0,
            duration: 4.0,
            x,
            y: 5.0,
            width: 40.0,
            height: 30.0,
            fill: Rgb { r: 255, g: 0, b: 0 },
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            opacity,
            order: 0,
            line: 1,
            column: 1,
        })
    }

    fn tween_decl(
        target: usize,
        property: &str,
        kind: TweenKind,
        from: Option<f64>,
        to: Option<f64>,
        start: f64,
        duration: f64,
        ease: EaseDecl,
    ) -> TweenDecl {
        TweenDecl {
            target,
            property: property.to_string(),
            kind,
            from,
            to,
            start,
            duration,
            ease,
            line: 1,
            column: 1,
        }
    }

    fn set_decl(target: usize, property: &str, value: f64, start: f64, duration: f64) -> SetDecl {
        SetDecl {
            target,
            property: property.to_string(),
            value,
            start,
            duration,
            line: 1,
            column: 1,
        }
    }

    fn comp_with(clips: Vec<Clip>, tweens: Vec<TweenDecl>, sets: Vec<SetDecl>) -> Composition {
        Composition {
            name: "t".to_string(),
            width: 320,
            height: 240,
            duration: 4.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips,
            fonts: Vec::new(),
            tweens,
            sets,
        }
    }

    #[test]
    fn seconds_read_as_exact_decimals() {
        assert_eq!(secs_to_time(1.0), Time::from_secs(1));
        assert_eq!(secs_to_time(0.1), Time::new(1, 10));
        assert_eq!(secs_to_time(2.5), Time::new(5, 2));
    }

    #[test]
    fn to_tween_runs_authored_to_given_at_exact_frames() {
        // T009: box x 10 -> 110 over [1, 2) at 30 fps.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(0, "x", TweenKind::To, None, Some(110.0), 1.0, 1.0, EaseDecl::Linear)],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 10.0, "before the window the authored value holds");
        assert_eq!(at(30), 10.0, "the start frame shows exactly the start value");
        assert_eq!(at(45), 60.0, "the midpoint frame is exactly halfway (linear)");
        assert_eq!(at(60), 110.0, "the end frame shows exactly the end value");
        assert_eq!(at(90), 110.0, "past the window the end value holds");
    }

    #[test]
    fn from_tween_holds_given_then_returns_to_authored() {
        // T009: from 60 to authored 10 over [1, 2).
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(0, "x", TweenKind::From, Some(60.0), None, 1.0, 1.0, EaseDecl::Linear)],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 60.0);
        assert_eq!(at(30), 60.0, "the start frame shows exactly the given value");
        assert_eq!(at(45), 35.0, "linear midpoint between given and authored");
        assert_eq!(at(60), 10.0, "the end frame is back at authored");
    }

    #[test]
    fn from_to_tween_moves_between_givens() {
        // T009: 0 -> 100 over [0, 1), authored 10 ignored inside.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(0, "x", TweenKind::FromTo, Some(0.0), Some(100.0), 0.0, 1.0, EaseDecl::Linear)],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 0.0);
        assert_eq!(at(15), 50.0);
        assert_eq!(at(30), 100.0);
    }

    #[test]
    fn tween_targets_only_its_clip() {
        // T009: a tween naming clip 1 leaves clip 0 still.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0), box_clip(20.0, 1.0)],
            vec![tween_decl(1, "x", TweenKind::To, None, Some(120.0), 0.0, 1.0, EaseDecl::Linear)],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        assert_eq!(motions[0].values_at(frame_time(&comp.fps, 30)).x, 10.0);
        assert_eq!(motions[1].values_at(frame_time(&comp.fps, 30)).x, 120.0);
    }

    #[test]
    fn scale_rotation_opacity_tween_in_all_forms() {
        // T010: from-to scale 1->3, to rotation 0->90, from opacity 0->authored 1.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![
                tween_decl(0, "scale", TweenKind::FromTo, Some(1.0), Some(3.0), 0.0, 2.0, EaseDecl::Linear),
                tween_decl(0, "rotation", TweenKind::To, None, Some(90.0), 0.0, 2.0, EaseDecl::Linear),
                tween_decl(0, "opacity", TweenKind::From, Some(0.0), None, 0.0, 2.0, EaseDecl::Linear),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        let start = at(0);
        assert_eq!((start.scale, start.rotation, start.opacity), (1.0, 0.0, 0.0));
        let mid = at(30);
        assert_eq!((mid.scale, mid.rotation, mid.opacity), (2.0, 45.0, 0.5));
        let end = at(60);
        assert_eq!((end.scale, end.rotation, end.opacity), (3.0, 90.0, 1.0));
    }

    #[test]
    fn set_holds_window_then_releases() {
        // T011: x holds 50 for [1, 2): first window frame already held,
        // first frame after the end already released, no blending.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![],
            vec![set_decl(0, "x", 50.0, 1.0, 1.0)],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(29), 10.0, "frame before the window shows authored");
        assert_eq!(at(30), 50.0, "first window frame already holds");
        assert_eq!(at(45), 50.0, "no easing or blending inside the window");
        assert_eq!(at(59), 50.0, "last window frame still holds");
        assert_eq!(at(60), 10.0, "first frame after the end already released");
    }

    #[test]
    fn later_set_wins_where_windows_overlap() {
        // T011: written-second set wins over the overlap.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![],
            vec![
                set_decl(0, "x", 50.0, 1.0, 1.0),
                set_decl(0, "x", 70.0, 1.5, 1.0),
            ],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(37), 50.0, "before the overlap the first hold shows");
        assert_eq!(at(52), 70.0, "inside the overlap the later hold wins");
        assert_eq!(at(67), 70.0, "after the first window the later hold still shows");
        assert_eq!(at(75), 10.0, "after both windows the authored value returns");
    }

    #[test]
    fn set_overrides_tween_inside_its_window_only() {
        // T011: outside the set window the tweened value shows.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![tween_decl(0, "x", TweenKind::To, None, Some(100.0), 0.0, 2.0, EaseDecl::Linear)],
            vec![set_decl(0, "x", 40.0, 0.5, 0.5)],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert!((at(7) - 100.0 * 7.0 / 60.0).abs() < 1e-9, "tweened before the set");
        assert_eq!(at(15), 40.0, "set wins at its start frame");
        assert_eq!(at(29), 40.0, "set wins at its last frame");
        assert!((at(30) - 50.0).abs() < 1e-9, "tweened again after the set");
    }

    #[test]
    fn all_nineteen_easings_start_end_exact_and_shape_middles() {
        // T012: every named curve starts and ends exactly; middles differ.
        let easings = [
            EaseDecl::Linear,
            EaseDecl::InQuad,
            EaseDecl::OutQuad,
            EaseDecl::InOutQuad,
            EaseDecl::InCubic,
            EaseDecl::OutCubic,
            EaseDecl::InOutCubic,
            EaseDecl::InQuart,
            EaseDecl::OutQuart,
            EaseDecl::InOutQuart,
            EaseDecl::InQuint,
            EaseDecl::OutQuint,
            EaseDecl::InOutQuint,
            EaseDecl::InSine,
            EaseDecl::OutSine,
            EaseDecl::InOutSine,
            EaseDecl::InExpo,
            EaseDecl::OutExpo,
            EaseDecl::InOutExpo,
        ];
        assert_eq!(easings.len(), 19);
        let mut mids = Vec::new();
        for ease in easings {
            let comp = comp_with(
                vec![box_clip(0.0, 1.0)],
                vec![tween_decl(0, "x", TweenKind::To, None, Some(100.0), 0.0, 2.0, ease)],
                vec![],
            );
            let motions = plan_motions(&comp, 30);
            let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
            assert_eq!(at(0), 0.0, "start exact under {ease:?}");
            assert_eq!(at(60), 100.0, "end exact under {ease:?}");
            mids.push((at(30) * 1e6).round() as i64);
        }
        mids.sort();
        mids.dedup();
        assert!(mids.len() >= 11, "curves must shape middles distinctly, got {mids:?}");
    }

    #[test]
    fn out_back_overshoot_varies_with_n_and_settles_exact() {
        // T012: out-back passes its end by an n-set amount, then settles.
        let run = |n: f64| -> Vec<f64> {
            let comp = comp_with(
                vec![box_clip(0.0, 1.0)],
                vec![tween_decl(0, "x", TweenKind::To, None, Some(100.0), 0.0, 1.0, EaseDecl::OutBack(n))],
                vec![],
            );
            let motions = plan_motions(&comp, 30);
            (0..=30).map(|k| motions[0].values_at(frame_time(&comp.fps, k)).x).collect()
        };
        let small = run(1.0);
        let big = run(3.0);
        assert_eq!(small[0], 0.0);
        assert_eq!(big[0], 0.0);
        assert_eq!(small[30], 100.0, "out-back settles exactly");
        assert_eq!(big[30], 100.0, "out-back settles exactly");
        let peak = |v: &[f64]| v.iter().cloned().fold(0.0f64, f64::max);
        assert!(peak(&small) > 100.0, "out-back passes its end");
        assert!(peak(&big) > peak(&small), "larger n passes further");
    }
}
