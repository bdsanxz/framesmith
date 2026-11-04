//! The render clock and per-element motion: exact frame times from
//! `timebase` and per-clip `motion` evaluation.
//!
//! Validated `tween`/`set` directives (plus Sprint 7 `key` chains) ride
//! in the composition and are fed into the motion engine here: tweens
//! become engine tweens over the clip's own values (to/from/from-to
//! with the declared easing), keys expand into one from-to tween per
//! stop-to-stop segment, while sets layer on top as stepwise half-open
//! holds with later-wins resolution. Evaluation is always at the exact
//! frame time, so one frame number always yields one value.

use motion::{ElementMotion, Prop, Tween, PROP_COUNT};
use notation::model::{
    Clip, Composition, EaseDecl, Fps, KeyDecl, KeyStop, SetDecl, StaggerGrid, StaggerOrigin,
    TweenDecl, TweenKind,
};
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
/// for text, box, and image clips, authored opacity for boxes, the
/// clip's static 3D turn and filter blur, authored geometry and paint
/// for boxes and images, authored metrics for text, unit scales and
/// full opacity otherwise. A clip holds these before and without
/// tweens; `canvas` bounds the unset image extents (a bare image fills
/// the canvas, so its animated width starts there).
pub fn own_values(clip: &Clip, canvas: (f64, f64)) -> [f64; PROP_COUNT] {
    let mut own = [0.0f64; PROP_COUNT];
    for (i, prop) in motion::PROPS.iter().enumerate() {
        own[i] = prop.own_value();
    }
    match clip {
        Clip::Text(tc) => {
            own[Prop::X.index()] = tc.x;
            own[Prop::Y.index()] = tc.y;
            own[Prop::FontSize.index()] = tc.size;
            own[Prop::LetterSpacing.index()] = tc.letter_spacing.unwrap_or(0.0);
            own[Prop::RotateX.index()] = tc.turn.rotation_x;
            own[Prop::RotateY.index()] = tc.turn.rotation_y;
            own[Prop::Blur.index()] = tc.filter_blur;
        }
        Clip::Box(bc) => {
            own[Prop::X.index()] = bc.x;
            own[Prop::Y.index()] = bc.y;
            own[Prop::Opacity.index()] = bc.opacity;
            own[Prop::Width.index()] = bc.width;
            own[Prop::Height.index()] = bc.height;
            own[Prop::FillR.index()] = f64::from(bc.fill.r);
            own[Prop::FillG.index()] = f64::from(bc.fill.g);
            own[Prop::FillB.index()] = f64::from(bc.fill.b);
            own[Prop::Radius.index()] = bc.radius;
            own[Prop::BorderWidth.index()] = bc.border_width;
            own[Prop::BorderR.index()] = f64::from(bc.border_color.r);
            own[Prop::BorderG.index()] = f64::from(bc.border_color.g);
            own[Prop::BorderB.index()] = f64::from(bc.border_color.b);
            own[Prop::RotateX.index()] = bc.turn.rotation_x;
            own[Prop::RotateY.index()] = bc.turn.rotation_y;
            own[Prop::Blur.index()] = bc.filter_blur;
        }
        Clip::Image(ic) => {
            own[Prop::X.index()] = ic.x;
            own[Prop::Y.index()] = ic.y;
            own[Prop::Width.index()] = ic.width.unwrap_or(canvas.0);
            own[Prop::Height.index()] = ic.height.unwrap_or(canvas.1);
            own[Prop::RotateX.index()] = ic.turn.rotation_x;
            own[Prop::RotateY.index()] = ic.turn.rotation_y;
            own[Prop::Blur.index()] = ic.filter_blur;
        }
        // A vector path owns its offset, paint, and opacity like a
        // box: motion tweens ride these channels, while `d` and the
        // fill rule stay authored.
        Clip::Path(pc) => {
            own[Prop::X.index()] = pc.x;
            own[Prop::Y.index()] = pc.y;
            own[Prop::Opacity.index()] = pc.opacity;
            own[Prop::FillR.index()] = f64::from(pc.fill.r);
            own[Prop::FillG.index()] = f64::from(pc.fill.g);
            own[Prop::FillB.index()] = f64::from(pc.fill.b);
            own[Prop::BorderWidth.index()] = pc.border_width;
            own[Prop::BorderR.index()] = f64::from(pc.border_color.r);
            own[Prop::BorderG.index()] = f64::from(pc.border_color.g);
            own[Prop::BorderB.index()] = f64::from(pc.border_color.b);
            own[Prop::RotateX.index()] = pc.turn.rotation_x;
            own[Prop::RotateY.index()] = pc.turn.rotation_y;
            own[Prop::Blur.index()] = pc.filter_blur;
        }
        // An SVG picture owns its rect like an image: motion tweens
        // ride the offset and the required extents.
        Clip::Svg(sc) => {
            own[Prop::X.index()] = sc.x;
            own[Prop::Y.index()] = sc.y;
            own[Prop::Width.index()] = sc.width;
            own[Prop::Height.index()] = sc.height;
            own[Prop::RotateX.index()] = sc.turn.rotation_x;
            own[Prop::RotateY.index()] = sc.turn.rotation_y;
            own[Prop::Blur.index()] = sc.filter_blur;
        }
        Clip::Video(vc) => {
            own[Prop::RotateX.index()] = vc.turn.rotation_x;
            own[Prop::RotateY.index()] = vc.turn.rotation_y;
            own[Prop::Blur.index()] = vc.filter_blur;
        }
        Clip::Flex(fc) => {
            own[Prop::X.index()] = fc.x;
            own[Prop::Y.index()] = fc.y;
            own[Prop::Width.index()] = fc.width;
            own[Prop::Height.index()] = fc.height;
        }
        Clip::Audio(_) | Clip::Nest(_) => {}
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

/// The engine property of a validated numeric directive name
/// (Sprint 7 motion, CAP-005/006): validation guarantees a canonical
/// `MOTION_PROPERTIES` name, and color properties (`fill`,
/// `border-color`) expand to three channel props instead of one, so
/// this returns `None` for them by design.
fn prop_of(name: &str) -> Option<Prop> {
    match name {
        "x" => Some(Prop::X),
        "y" => Some(Prop::Y),
        "scale" => Some(Prop::Scale),
        "rotation" => Some(Prop::Rotation),
        "opacity" => Some(Prop::Opacity),
        "rotation-x" => Some(Prop::RotateX),
        "rotation-y" => Some(Prop::RotateY),
        "width" => Some(Prop::Width),
        "height" => Some(Prop::Height),
        "radius" => Some(Prop::Radius),
        "border-width" => Some(Prop::BorderWidth),
        "blur" => Some(Prop::Blur),
        "font-size" => Some(Prop::FontSize),
        "letter-spacing" => Some(Prop::LetterSpacing),
        _ => None,
    }
}

/// The three channel props of a validated color directive name, in
/// straight-RGBA order (Sprint 7 motion, CAP-006).
fn color_channels(name: &str) -> Option<[Prop; 3]> {
    match name {
        "fill" => Some([Prop::FillR, Prop::FillG, Prop::FillB]),
        "border-color" => Some([Prop::BorderR, Prop::BorderG, Prop::BorderB]),
        _ => None,
    }
}

/// One validated `tween` as engine tweens (Sprint 7 motion): numeric
/// properties ride one tween; `fill`/`border-color` ride three
/// (one per straight-RGBA channel, same window, kind, and easing).
/// Expression tweens ride no segments: their windows evaluate per
/// frame in [`planned_exprs`], so the engine never holds a phantom
/// track for them.
fn engine_tweens(decl: &TweenDecl) -> Vec<Tween> {
    if decl.expr.is_some() {
        return Vec::new();
    }
    let kind = match decl.kind {
        TweenKind::To => motion::Kind::To,
        TweenKind::From => motion::Kind::From,
        TweenKind::FromTo => motion::Kind::FromTo,
    };
    let at = secs_to_time(decl.start);
    let duration = secs_to_time(decl.duration);
    let ease = ease_of(&decl.ease);
    if let Some(channels) = color_channels(&decl.property) {
        let from = decl
            .from_color
            .map(|c| [f64::from(c.r), f64::from(c.g), f64::from(c.b)]);
        let to = decl
            .to_color
            .map(|c| [f64::from(c.r), f64::from(c.g), f64::from(c.b)]);
        return channels
            .iter()
            .enumerate()
            .map(|(i, prop)| {
                let mut from_a = [None; PROP_COUNT];
                let mut to_a = [None; PROP_COUNT];
                if let Some(f) = from {
                    from_a[prop.index()] = Some(f[i]);
                }
                if let Some(t) = to {
                    to_a[prop.index()] = Some(t[i]);
                }
                Tween {
                    target: decl.target.to_string(),
                    kind,
                    from: from_a,
                    to: to_a,
                    at,
                    duration,
                    ease,
                    reversed: false,
                }
            })
            .collect();
    }
    let Some(prop) = prop_of(&decl.property) else {
        return Vec::new();
    };
    let mut from = [None; PROP_COUNT];
    let mut to = [None; PROP_COUNT];
    if let Some(v) = decl.from {
        from[prop.index()] = Some(v);
    }
    if let Some(v) = decl.to {
        to[prop.index()] = Some(v);
    }
    vec![Tween {
        target: decl.target.to_string(),
        kind,
        from,
        to,
        at,
        duration,
        ease,
        reversed: false,
    }]
}

pub(crate) fn ease_of(decl: &EaseDecl) -> motion::Ease {
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
        EaseDecl::CubicBezier(x1, y1, x2, y2) => Ease::CubicBezier(x1, y1, x2, y2),
        EaseDecl::Steps(n) => Ease::Steps(n),
        EaseDecl::Spring(s, d) => Ease::Spring(s, d),
        EaseDecl::Bounce => Ease::Bounce,
        EaseDecl::Elastic => Ease::Elastic,
        EaseDecl::Wiggle(n) => Ease::Wiggle(n),
        EaseDecl::Hold => Ease::Hold,
    }
}

/// One stop-to-stop segment as engine tweens (Sprint 7 motion): numeric
/// properties ride one `from-to` tween; `fill`/`border-color` ride
/// three (one per straight-RGBA channel, same window and easing).
fn segment_tweens(
    property: &str,
    from_value: f64,
    from_color: Option<[f64; 3]>,
    to_value: f64,
    to_color: Option<[f64; 3]>,
    target: usize,
    at: Time,
    duration: Time,
    ease: motion::Ease,
) -> Vec<Tween> {
    if let Some(channels) = color_channels(property) {
        return channels
            .iter()
            .enumerate()
            .map(|(i, prop)| {
                let mut from = [None; PROP_COUNT];
                let mut to = [None; PROP_COUNT];
                if let Some(f) = from_color {
                    from[prop.index()] = Some(f[i]);
                }
                if let Some(t) = to_color {
                    to[prop.index()] = Some(t[i]);
                }
                Tween {
                    target: target.to_string(),
                    kind: motion::Kind::FromTo,
                    from,
                    to,
                    at,
                    duration,
                    ease,
                    reversed: false,
                }
            })
            .collect();
    }
    let Some(prop) = prop_of(property) else {
        return Vec::new();
    };
    let mut from = [None; PROP_COUNT];
    let mut to = [None; PROP_COUNT];
    from[prop.index()] = Some(from_value);
    to[prop.index()] = Some(to_value);
    vec![Tween {
        target: target.to_string(),
        kind: motion::Kind::FromTo,
        from,
        to,
        at,
        duration,
        ease,
        reversed: false,
    }]
}

/// A validated `key` as engine tweens (Sprint 7 motion): one `from-to`
/// tween per stop-to-stop segment (three for color properties), in stop
/// order. Each segment carries its starting stop's `ease`, defaulting
/// to the key easing, then linear; validation guarantees at least two
/// stops in rising order, so every duration is positive.
pub fn key_tweens(decl: &KeyDecl) -> Vec<Tween> {
    if prop_of(&decl.property).is_none() && color_channels(&decl.property).is_none() {
        return Vec::new();
    }
    let key_ease = decl.ease.as_ref().map(ease_of);
    let mut tweens = Vec::with_capacity(decl.stops.len().saturating_sub(1));
    for pair in decl.stops.windows(2) {
        let (from_stop, to_stop) = (&pair[0], &pair[1]);
        let ease = from_stop
            .ease
            .as_ref()
            .map(ease_of)
            .or(key_ease)
            .unwrap_or(motion::Ease::Linear);
        let channels = |stop: &KeyStop| {
            stop.value_color
                .map(|c| [f64::from(c.r), f64::from(c.g), f64::from(c.b)])
        };
        tweens.extend(segment_tweens(
            &decl.property,
            from_stop.value,
            channels(from_stop),
            to_stop.value,
            channels(to_stop),
            decl.target,
            secs_to_time(from_stop.at),
            secs_to_time(to_stop.at - from_stop.at),
            ease,
        ));
    }
    tweens
}

/// A validated `tween` as an engine tween (Sprint 2 shape, kept for
/// callers that only animate numeric properties): one property, exact
/// window, declared easing, written order preserved by the caller.
/// Color tweens expand to three channel tweens via [`engine_tweens`].
pub fn engine_tween(decl: &TweenDecl) -> Option<Tween> {
    engine_tweens(decl).into_iter().next()
}

/// The repeated engine tweens of one validated `tween` (Sprint 7
/// motion, CAP-004): `repeat` passes of the window separated by
/// `repeat-delay` gaps (gaps hold end values through the track's end
/// clamp, so no segment covers them); with `yoyo`, alternate passes
/// play backward, tracing the forward values exactly in reverse. One
/// pass reproduces the unrepeated tween exactly.
pub fn expand_tween(decl: &TweenDecl) -> Vec<Tween> {
    let base = engine_tweens(decl);
    if base.is_empty() {
        return base;
    }
    let passes = motion::repeat_passes(decl.duration, decl.repeat, decl.repeat_delay, decl.yoyo);
    if passes.len() <= 1 {
        return base;
    }
    let mut out = Vec::with_capacity(base.len() * passes.len());
    for pass in &passes {
        for tween in &base {
            out.push(Tween {
                at: tween.at + secs_to_time(pass.offset),
                reversed: pass.reversed,
                ..tween.clone()
            });
        }
    }
    out
}

/// The repeated engine tweens of one validated `key` (Sprint 7 motion,
/// CAP-004): each pass replays the whole stop-to-stop chain, passes
/// separated by `repeat-delay` gaps holding end values; with `yoyo`,
/// alternate passes run the chain backward (segments in reverse
/// order, each reversed), so the pass traces the forward chain
/// exactly in reverse. One pass reproduces the unrepeated key exactly.
pub fn expand_key(decl: &KeyDecl) -> Vec<Tween> {
    let base = key_tweens(decl);
    if base.is_empty() {
        return base;
    }
    let first = decl.stops.first().map(|s| s.at).unwrap_or(0.0);
    let last = decl.stops.last().map(|s| s.at).unwrap_or(first);
    let unit = (last - first).max(0.0);
    let passes = motion::repeat_passes(unit, decl.repeat, decl.repeat_delay, decl.yoyo);
    if passes.len() <= 1 {
        return base;
    }
    let mut out = Vec::with_capacity(base.len() * passes.len());
    for pass in &passes {
        let shift = secs_to_time(pass.offset);
        if pass.reversed {
            for tween in base.iter().rev() {
                out.push(Tween {
                    at: tween.at + shift,
                    reversed: true,
                    ..tween.clone()
                });
            }
        } else {
            for tween in &base {
                out.push(Tween {
                    at: tween.at + shift,
                    ..tween.clone()
                });
            }
        }
    }
    out
}

/// One planned tween: its repeated engine tweens plus the per-piece
/// delay distribution of its declaration (Sprint 7 motion, CAP-002).
/// Whole-clip motion evaluates the tweens unshifted; piece `i` of
/// `count` shifts every window later by its delay.
#[derive(Clone, Debug)]
pub struct PlannedTween {
    pub tweens: Vec<Tween>,
    pub stagger: f64,
    pub origin: StaggerOrigin,
    pub grid: Option<StaggerGrid>,
}

/// The delay of one piece (Sprint 7 motion, CAP-002): `stagger`
/// seconds times its order index from the origin (row-major in
/// grids). Out-of-range pieces and non-positive staggers delay
/// nothing, so unstaggered motion never shifts.
pub fn piece_delay(
    piece: usize,
    count: usize,
    stagger: f64,
    origin: StaggerOrigin,
    grid: Option<StaggerGrid>,
) -> f64 {
    if count == 0 || piece >= count {
        return 0.0;
    }
    let motion_origin = match origin {
        StaggerOrigin::Start => motion::StaggerOrigin::Start,
        StaggerOrigin::Center => motion::StaggerOrigin::Center,
        StaggerOrigin::End => motion::StaggerOrigin::End,
    };
    match grid {
        None => motion::stagger_delay(piece, count, stagger, motion_origin),
        Some(g) => match g.rows {
            None => motion::stagger_grid_delay(piece, count, stagger, motion_origin, g.cols),
            Some(rows) => {
                motion::stagger_grid_delay_rows(piece, count, stagger, motion_origin, g.cols, rows)
            }
        },
    }
}

/// Engine tweens shifted later for one piece: every window moves by
/// `delay_secs`, so the piece starts its motion exactly that late.
/// Durations, values, easings, and reversal flags ride unchanged.
fn shift_for_piece(base: &[Tween], delay_secs: f64) -> Vec<Tween> {
    if delay_secs == 0.0 {
        return base.to_vec();
    }
    let shift = secs_to_time(delay_secs);
    base.iter()
        .map(|tween| Tween {
            at: tween.at + shift,
            ..tween.clone()
        })
        .collect()
}

/// One path window: eased travel along the (possibly morphing)
/// path over `[at, end)`. The heading adds to the tracked rotation
/// only when `orient` holds. A `reversed` pass mirrors progress
/// before easing, tracing the forward values exactly in reverse.
#[derive(Clone, Debug)]
pub struct PathWindow {
    pub at: Time,
    pub end: Time,
    pub ease: motion::Ease,
    /// The flattened travel table when the window rides one path.
    table: Option<motion::path::Table>,
    /// The morph endpoints when the window interpolates two
    /// same-structure paths (checked compatible at plan time).
    morph: Option<(motion::path::Path, motion::path::Path)>,
    pub orient: bool,
    pub reversed: bool,
}

impl PathWindow {
    /// Position plus optional tangent heading at `t`: the eased
    /// distance along the true curve length of the morphed path.
    /// Past the window the end holds, mirroring engine tracks.
    fn sample_at(&self, t: Time) -> (f64, f64, Option<f64>) {
        let span = self.end - self.at;
        let raw = if t <= self.at || span <= Time::ZERO {
            0.0
        } else if t >= self.end {
            1.0
        } else {
            (t - self.at).ratio(span)
        };
        let progress = if self.reversed { 1.0 - raw } else { raw }.clamp(0.0, 1.0);
        let eased = self.ease.apply(progress);
        // The morph cannot fail: planning checks the structure, and
        // `check` gates every render. The fallback still travels.
        let owned;
        let table = match (&self.table, &self.morph) {
            (Some(table), _) => table,
            (None, Some((from, to))) => {
                owned = motion::path::flatten(
                    &motion::path::morph(from, to, eased).unwrap_or_else(|_| from.clone()),
                );
                &owned
            }
            (None, None) => return (0.0, 0.0, None),
        };
        let (pos, heading) = motion::path::sample(table, eased * table.total);
        (pos[0], pos[1], self.orient.then_some(heading))
    }
}

/// One validated path window (Sprint 7 motion, CAP-008): the travel
/// path flattened once when it rides one path, kept as a morph pair
/// otherwise. `None` names no usable window: unparseable data or a
/// cross-structure morph, both refused by `check` before render.
fn path_window(
    at: Time,
    end: Time,
    ease: motion::Ease,
    from: motion::path::Path,
    to: motion::path::Path,
    orient: bool,
) -> Option<PathWindow> {
    if motion::path::morph(&from, &to, 0.0).is_err() {
        return None;
    }
    let (table, morph) = if from == to {
        (Some(motion::path::flatten(&from)), None)
    } else {
        (None, Some((from, to)))
    };
    Some(PathWindow {
        at,
        end,
        ease,
        table,
        morph,
        orient,
        reversed: false,
    })
}

/// The repeated path windows of one validated path `tween` (Sprint 7
/// motion, CAP-008): `repeat` passes of the window separated by
/// `repeat-delay` gaps (gaps hold end values, like engine tweens);
/// with `yoyo`, alternate passes play backward. One pass reproduces
/// the unrepeated travel exactly. Tweens without `path` ride no
/// windows.
pub fn expand_path_tween(decl: &TweenDecl) -> Vec<PathWindow> {
    let Some(raw) = decl.path.as_deref() else {
        return Vec::new();
    };
    let (Ok(from), to) = (
        motion::path::parse(raw),
        match decl.path_to.as_deref() {
            Some(raw_to) => motion::path::parse(raw_to),
            None => motion::path::parse(raw),
        },
    ) else {
        return Vec::new();
    };
    let Ok(to) = to else {
        return Vec::new();
    };
    let Some(base) = path_window(
        secs_to_time(decl.start),
        secs_to_time(decl.start + decl.duration),
        ease_of(&decl.ease),
        from,
        to,
        decl.orient,
    ) else {
        return Vec::new();
    };
    let passes = motion::repeat_passes(decl.duration, decl.repeat, decl.repeat_delay, decl.yoyo);
    if passes.len() <= 1 {
        return vec![base];
    }
    let mut out = Vec::with_capacity(passes.len());
    for pass in &passes {
        let shift = secs_to_time(pass.offset);
        out.push(PathWindow {
            at: base.at + shift,
            end: base.end + shift,
            reversed: pass.reversed,
            ..base.clone()
        });
    }
    out.sort_by_key(|w| w.at);
    out
}

/// The repeated path windows of one validated path `key` (Sprint 7
/// motion, CAP-008): one window per consecutive path-stop pair, each
/// carrying its starting stop's `ease` (key easing, then linear).
/// Repeats replay the whole chain with gaps holding end values;
/// `yoyo` reverses every segment in place, like engine keys. Keys
/// without path stops ride no windows.
pub fn expand_path_key(decl: &KeyDecl) -> Vec<PathWindow> {
    let key_ease = decl.ease.as_ref().map(ease_of);
    let mut base = Vec::new();
    for pair in decl.stops.windows(2) {
        let (Some(a), Some(b)) = (pair[0].path.as_deref(), pair[1].path.as_deref()) else {
            continue;
        };
        let (Ok(from), Ok(to)) = (motion::path::parse(a), motion::path::parse(b)) else {
            continue;
        };
        let ease = pair[0]
            .ease
            .as_ref()
            .map(ease_of)
            .or(key_ease)
            .unwrap_or(motion::Ease::Linear);
        if let Some(window) = path_window(
            secs_to_time(pair[0].at),
            secs_to_time(pair[1].at),
            ease,
            from,
            to,
            decl.orient,
        ) {
            base.push(window);
        }
    }
    if base.is_empty() {
        return base;
    }
    let first = decl.stops.first().map(|s| s.at).unwrap_or(0.0);
    let last = decl.stops.last().map(|s| s.at).unwrap_or(first);
    let unit = (last - first).max(0.0);
    let passes = motion::repeat_passes(unit, decl.repeat, decl.repeat_delay, decl.yoyo);
    if passes.len() <= 1 {
        return base;
    }
    let mut out = Vec::with_capacity(base.len() * passes.len());
    for pass in &passes {
        let shift = secs_to_time(pass.offset);
        for window in &base {
            out.push(PathWindow {
                at: window.at + shift,
                end: window.end + shift,
                reversed: pass.reversed,
                ..window.clone()
            });
        }
    }
    out.sort_by_key(|w| w.at);
    out
}

/// One planned path motion: its repeated windows plus the per-piece
/// delay distribution of its declaration (Sprint 7 motion, CAP-002).
/// Whole-clip motion evaluates the windows unshifted; piece `i` of
/// `count` shifts every window later by its delay.
#[derive(Clone, Debug)]
pub struct PlannedPath {
    pub windows: Vec<PathWindow>,
    pub stagger: f64,
    pub origin: StaggerOrigin,
    pub grid: Option<StaggerGrid>,
}

/// Path windows shifted later for one piece: every window moves by
/// `delay_secs`, so the piece starts its travel exactly that late.
/// Durations, easings, morphs, and reversal flags ride unchanged.
fn shift_path_for_piece(base: &[PathWindow], delay_secs: f64) -> Vec<PathWindow> {
    if delay_secs == 0.0 {
        return base.to_vec();
    }
    let shift = secs_to_time(delay_secs);
    base.iter()
        .map(|window| PathWindow {
            at: window.at + shift,
            end: window.end + shift,
            ..window.clone()
        })
        .collect()
}

/// All repeated path windows targeting one clip, with their stagger
/// shapes, in written order (tweens, then keys).
pub fn planned_paths(comp: &Composition, clip_idx: usize) -> Vec<PlannedPath> {
    let mut planned: Vec<PlannedPath> = comp
        .tweens
        .iter()
        .filter(|decl| decl.target == clip_idx && decl.path.is_some())
        .map(|decl| PlannedPath {
            windows: expand_path_tween(decl),
            stagger: decl.stagger,
            origin: decl.stagger_from,
            grid: decl.stagger_grid,
        })
        .collect();
    for decl in comp
        .keys
        .iter()
        .filter(|decl| decl.target == clip_idx && decl.stops.iter().any(|s| s.path.is_some()))
    {
        planned.push(PlannedPath {
            windows: expand_path_key(decl),
            stagger: decl.stagger,
            origin: decl.stagger_from,
            grid: decl.stagger_grid,
        });
    }
    planned
}

/// All repeated engine tweens targeting one clip, with their stagger
/// shapes, in written order (tweens, then keys).
pub fn planned_tweens(comp: &Composition, clip_idx: usize) -> Vec<PlannedTween> {
    let mut planned: Vec<PlannedTween> = comp
        .tweens
        .iter()
        .filter(|decl| decl.target == clip_idx)
        .map(|decl| PlannedTween {
            tweens: expand_tween(decl),
            stagger: decl.stagger,
            origin: decl.stagger_from,
            grid: decl.stagger_grid,
        })
        .collect();
    for decl in comp.keys.iter().filter(|decl| decl.target == clip_idx) {
        planned.push(PlannedTween {
            tweens: expand_key(decl),
            stagger: decl.stagger,
            origin: decl.stagger_from,
            grid: decl.stagger_grid,
        });
    }
    planned
}

/// Every animated value of one clip for one piece at one exact time
/// (Sprint 7 motion, CAP-002): the declaration windows shifted by the
/// piece delay, evaluated like whole-clip motion (repeats, gaps, and
/// windowed sets included). Piece 0 of a single piece, and any
/// unstaggered directive, matches `ClipMotion::values_at` exactly.
/// Expression windows evaluate with `piece` as `i` (Sprint 7 motion,
/// CAP-010).
#[allow(clippy::too_many_arguments)]
pub fn piece_values(
    clip: &Clip,
    planned: &[PlannedTween],
    sets: &[SetWindow],
    fps: u32,
    t: Time,
    piece: usize,
    count: usize,
    exprs: &[ExprWindow],
    inputs: &[f64],
) -> Evaluated {
    piece_values_with_paths(
        clip,
        planned,
        &[],
        sets,
        fps,
        t,
        piece,
        count,
        exprs,
        inputs,
    )
}

/// Every animated value of one clip for one piece at one exact time,
/// with path travel (Sprint 7 motion, CAP-008): like [`piece_values`],
/// plus the declaration path windows shifted by the piece delay.
#[allow(clippy::too_many_arguments)]
pub fn piece_values_with_paths(
    clip: &Clip,
    planned: &[PlannedTween],
    planned_paths: &[PlannedPath],
    sets: &[SetWindow],
    fps: u32,
    t: Time,
    piece: usize,
    count: usize,
    exprs: &[ExprWindow],
    inputs: &[f64],
) -> Evaluated {
    let mut shifted: Vec<Tween> = Vec::new();
    for p in planned {
        shifted.extend(shift_for_piece(
            &p.tweens,
            piece_delay(piece, count, p.stagger, p.origin, p.grid),
        ));
    }
    let mut shifted_paths: Vec<PathWindow> = Vec::new();
    for p in planned_paths {
        shifted_paths.extend(shift_path_for_piece(
            &p.windows,
            piece_delay(piece, count, p.stagger, p.origin, p.grid),
        ));
    }
    shifted_paths.sort_by_key(|w| w.at);
    ClipMotion::with_directives(0, clip, &shifted, sets, fps)
        .with_paths(shifted_paths)
        .with_exprs(exprs.to_vec(), piece as f64, inputs.to_vec())
        .values_at(t)
}

/// One validated `expr` tween as a value window (Sprint 7 motion,
/// CAP-010): the property evaluates the pure expression per frame
/// for `start <= t < end`, in written order for later-wins. The
/// expression reads composition seconds as `t`, the clip's replicate
/// (or piece) index as `i`, and declared inputs by position.
#[derive(Clone, Debug)]
pub struct ExprWindow {
    pub prop: Prop,
    pub start: Time,
    pub end: Time,
    pub expr: motion::expr::Expr,
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
    Some(SetWindow {
        prop,
        start,
        end: end.max(start),
        value: decl.value,
    })
}

/// Every animated value of one clip at one exact time (T009–T011
/// plus Sprint 7 motion): tweened (or authored) values with windowed
/// sets applied on top. Colors ride straight-RGBA channels 0–255; the
/// frame layer rounds them to bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evaluated {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
    /// 3D turn angles in degrees about the horizontal/vertical axes
    /// (Sprint 7 motion, CAP-005): the static clip turn plus any
    /// `rotation-x`/`rotation-y` motion.
    pub rotate_x: f64,
    pub rotate_y: f64,
    /// Animated box/image extents in canvas pixels (CAP-006).
    pub width: f64,
    pub height: f64,
    /// Animated box fill color, straight RGBA channels (CAP-006).
    pub fill: [f64; 3],
    /// Animated box corner radius in canvas pixels (CAP-006).
    pub radius: f64,
    /// Animated box border width in canvas pixels (CAP-006).
    pub border_width: f64,
    /// Animated box border color, straight RGBA channels (CAP-006).
    pub border: [f64; 3],
    /// Animated Gaussian blur sigma in element pixels (CAP-006).
    pub blur: f64,
    /// Animated text font size in canvas pixels (CAP-006).
    pub font_size: f64,
    /// Animated text letter spacing in canvas pixels (CAP-006).
    pub letter_spacing: f64,
}

/// One clip's motion: its evaluator over its tweens, its stepwise set
/// windows, its path travel windows, plus its own values for offset math.
pub struct ClipMotion {
    element: ElementMotion,
    sets: Vec<SetWindow>,
    paths: Vec<PathWindow>,
    own: [f64; PROP_COUNT],
    exprs: Vec<ExprWindow>,
    /// The `i` expressions read for this clip: its replicate clone
    /// offset, or 0 for ordinary clips.
    piece: f64,
    /// Declared input values in declaration order, answering the
    /// expression variables by position.
    inputs: Vec<f64>,
}

impl ClipMotion {
    fn build(
        index: usize,
        clip: &Clip,
        tweens: &[Tween],
        sets: Vec<SetWindow>,
        fps: u32,
        canvas: (f64, f64),
    ) -> ClipMotion {
        let own = own_values(clip, canvas);
        ClipMotion {
            element: ElementMotion::build(index, tweens, own, fps),
            sets,
            paths: Vec::new(),
            own,
            exprs: Vec::new(),
            piece: 0.0,
            inputs: Vec::new(),
        }
    }

    /// Motion with expression windows attached (Sprint 7 motion,
    /// CAP-010): each window evaluates its pure expression per frame
    /// over its half-open span, reading `piece` as `i` and `inputs`
    /// by position. Windows must arrive in written order (later wins
    /// where they overlap). Expression-free motion evaluates exactly
    /// as before.
    pub fn with_exprs(
        mut self,
        exprs: Vec<ExprWindow>,
        piece: f64,
        inputs: Vec<f64>,
    ) -> ClipMotion {
        self.exprs = exprs;
        self.piece = piece;
        self.inputs = inputs;
        self
    }

    /// Motion with path travel windows attached (Sprint 7 motion,
    /// CAP-008): the windows override placement (and, with `orient`,
    /// rotation) at their times. Windows must arrive sorted by start.
    pub fn with_paths(mut self, paths: Vec<PathWindow>) -> ClipMotion {
        self.paths = paths;
        self
    }

    /// The path placement at `t`: the last window starting at or
    /// before `t` samples it (holding its end past the window, like
    /// engine tracks). `None` when no window has started yet.
    fn path_at(&self, t: Time) -> Option<(f64, f64, Option<f64>)> {
        let started = self.paths.partition_point(|w| w.at <= t);
        if started == 0 {
            return None;
        }
        Some(self.paths[started - 1].sample_at(t))
    }

    /// Motion of clip `index` with no written tweens: every animated
    /// value is the clip's own value at every time.
    pub fn still(index: usize, clip: &Clip, fps: u32) -> ClipMotion {
        Self::build(index, clip, &[], Vec::new(), fps, (0.0, 0.0))
    }

    /// Motion of clip `index` with no written tweens and a known
    /// canvas: like [`ClipMotion::still`], but an image clip with no
    /// authored `width`/`height` owns the full canvas extents.
    pub fn still_on(index: usize, clip: &Clip, fps: u32, canvas: (f64, f64)) -> ClipMotion {
        Self::build(index, clip, &[], Vec::new(), fps, canvas)
    }

    /// Motion of clip `index` with written engine `tweens`: the
    /// evaluator blends them over the clip's own values.
    pub fn with_tweens(index: usize, clip: &Clip, tweens: &[Tween], fps: u32) -> ClipMotion {
        Self::build(index, clip, tweens, Vec::new(), fps, (0.0, 0.0))
    }

    /// Motion of clip `index` with validated directives: engine tweens
    /// below, windowed sets layered on top. `canvas` bounds the unset
    /// image extents for their animated widths.
    pub fn with_directives(
        index: usize,
        clip: &Clip,
        tweens: &[Tween],
        sets: &[SetWindow],
        fps: u32,
    ) -> ClipMotion {
        Self::build(index, clip, tweens, sets.to_vec(), fps, (0.0, 0.0))
    }

    /// Motion of clip `index` with validated directives and a known
    /// canvas: like [`ClipMotion::with_directives`], but an image clip
    /// with no authored `width`/`height` owns the full canvas extents.
    pub fn with_directives_on(
        index: usize,
        clip: &Clip,
        tweens: &[Tween],
        sets: &[SetWindow],
        fps: u32,
        canvas: (f64, f64),
    ) -> ClipMotion {
        Self::build(index, clip, tweens, sets.to_vec(), fps, canvas)
    }

    fn tracked(&self, prop: Prop, t: Time) -> f64 {
        let base = self.element.values(t)[prop.index()].unwrap_or(self.own[prop.index()]);
        // Expression windows win over the tweened value: the last
        // covering window in written order evaluates its pure
        // function at this exact instant. A non-finite draw (missed
        // by validation sampling) falls back to the tweened value;
        // validation refuses every sampled instant before frame one.
        let mut expressed: Option<f64> = None;
        if !self.exprs.is_empty() {
            let instant = t.to_f64();
            for window in &self.exprs {
                if window.prop == prop && window.start <= t && t < window.end {
                    let v = window.expr.eval(instant, self.piece, &self.inputs);
                    expressed = Some(if v.is_finite() { v } else { base });
                }
            }
        }
        let value = expressed.unwrap_or(base);
        // Windowed sets win over the tweened (or expressed) value,
        // with no easing and no blending; overlapping sets resolve
        // to the later-written value wherever they overlap.
        let mut held: Option<f64> = None;
        for window in &self.sets {
            if window.prop == prop && window.start <= t && t < window.end {
                held = Some(window.value);
            }
        }
        held.unwrap_or(value)
    }

    /// True when a tween, a set, an expression window, or a path
    /// window names `prop` on this clip (Sprint 7 motion,
    /// CAP-006/008/010): the frame layer then prefers the evaluated
    /// value over the authored field, even where they agree. Every
    /// path window places `x` and `y`; only `orient` windows turn
    /// `rotation`.
    pub fn touches(&self, prop: Prop) -> bool {
        if self.element.tracks[prop.index()].is_some() {
            return true;
        }
        if self.sets.iter().any(|w| w.prop == prop) {
            return true;
        }
        if self.exprs.iter().any(|w| w.prop == prop) {
            return true;
        }
        match prop {
            Prop::X | Prop::Y => !self.paths.is_empty(),
            Prop::Rotation => self.paths.iter().any(|w| w.orient),
            _ => false,
        }
    }

    /// Every animated value of the clip at `t`: tweened (or authored)
    /// values with windowed sets on top, then path travel overriding
    /// placement (and, with `orient`, adding the tangent heading to
    /// the rotation).
    pub fn values_at(&self, t: Time) -> Evaluated {
        let mut eval = Evaluated {
            x: self.tracked(Prop::X, t),
            y: self.tracked(Prop::Y, t),
            scale: self.tracked(Prop::Scale, t),
            rotation: self.tracked(Prop::Rotation, t),
            opacity: self.tracked(Prop::Opacity, t),
            rotate_x: self.tracked(Prop::RotateX, t),
            rotate_y: self.tracked(Prop::RotateY, t),
            width: self.tracked(Prop::Width, t),
            height: self.tracked(Prop::Height, t),
            fill: [
                self.tracked(Prop::FillR, t),
                self.tracked(Prop::FillG, t),
                self.tracked(Prop::FillB, t),
            ],
            radius: self.tracked(Prop::Radius, t),
            border_width: self.tracked(Prop::BorderWidth, t),
            border: [
                self.tracked(Prop::BorderR, t),
                self.tracked(Prop::BorderG, t),
                self.tracked(Prop::BorderB, t),
            ],
            blur: self.tracked(Prop::Blur, t),
            font_size: self.tracked(Prop::FontSize, t),
            letter_spacing: self.tracked(Prop::LetterSpacing, t),
        };
        // Path travel (Sprint 7 motion, CAP-008): the latest started
        // window places the clip; `orient` adds its tangent heading.
        if let Some((x, y, heading)) = self.path_at(t) {
            eval.x = x;
            eval.y = y;
            if let Some(add) = heading {
                eval.rotation += add;
            }
        }
        eval
    }

    /// Position offset at `t`: evaluated x/y minus the clip's own x/y.
    /// Zero for still clips; the exact tween blend once written.
    pub fn offset_at(&self, t: Time) -> (f64, f64) {
        let v = self.values_at(t);
        (
            v.x - self.own[Prop::X.index()],
            v.y - self.own[Prop::Y.index()],
        )
    }
}

/// Expression windows of one clip, in written order (Sprint 7
/// motion, CAP-010): one window per `expr` tween targeting the clip.
/// Validation guarantees every source parses against the composition
/// inputs, so planning replays that parse; an unparseable source
/// (unreachable after validation) rides no window.
pub fn planned_exprs(comp: &Composition, clip_idx: usize) -> Vec<ExprWindow> {
    let names: Vec<&str> = comp.inputs.iter().map(|d| d.name.as_str()).collect();
    comp.tweens
        .iter()
        .filter(|decl| decl.target == clip_idx && decl.expr.is_some())
        .filter_map(|decl| {
            let source = decl.expr.as_deref()?;
            let prop = prop_of(&decl.property)?;
            let expr = motion::expr::parse(source, &names).ok()?;
            Some(ExprWindow {
                prop,
                start: secs_to_time(decl.start),
                end: secs_to_time(decl.start + decl.duration).max(secs_to_time(decl.start)),
                expr,
            })
        })
        .collect()
}

/// Motions of all clips of a composition, in clip order: each clip's
/// own tweens (targets naming it) feed its evaluator in written order,
/// keyframes expand below them in written order (each repeated per its
/// own `repeat`/`yoyo`/`repeat-delay`), each clip's own sets layer on
/// top in written order, and path windows ride alongside sorted by
/// start. Expression windows evaluate per frame over their spans,
/// reading the clip's replicate index as `i`. Compositions without
/// keys, repeats, paths, or expressions plan exactly the motion they
/// always did.
pub fn plan_motions(comp: &Composition, fps: u32) -> Vec<ClipMotion> {
    let canvas = (f64::from(comp.width), f64::from(comp.height));
    let inputs = comp.input_values();
    comp.clips
        .iter()
        .enumerate()
        .map(|(i, clip)| {
            let mut tweens: Vec<Tween> = Vec::new();
            for planned in planned_tweens(comp, i) {
                tweens.extend(planned.tweens);
            }
            let sets: Vec<SetWindow> = comp
                .sets
                .iter()
                .filter(|decl| decl.target == i)
                .filter_map(set_window)
                .collect();
            let mut paths: Vec<PathWindow> = Vec::new();
            for planned in planned_paths(comp, i) {
                paths.extend(planned.windows);
            }
            paths.sort_by_key(|w| w.at);
            let exprs = planned_exprs(comp, i);
            let piece = comp.replicate_index(i) as f64;
            ClipMotion::with_directives_on(i, clip, &tweens, &sets, fps, canvas)
                .with_paths(paths)
                .with_exprs(exprs, piece, inputs.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion::{Ease, Kind};
    use notation::model::{Align, FontStyle, KeyStop, Rgb, TextClip, TrackKind, Wrap};

    fn text_clip(x: f64, y: f64) -> Clip {
        Clip::Text(TextClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            value: "Hi".to_string(),
            start: 0.0,
            duration: 2.0,
            x,
            y,
            size: 48.0,
            color: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            align: Align::Left,
            wrap: Wrap::Off,
            style: FontStyle::Regular,
            order: 0,
            line_height: None,
            letter_spacing: None,
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
        })
    }

    #[test]
    fn frame_times_are_exact() {
        let fps = Fps::from_int(30);
        assert_eq!(frame_time(&fps, 0).to_string(), "0");
        assert_eq!(frame_time(&fps, 90).to_string(), "3");
        // Rational rate stays exact: 30000 frames at 30000/1001.
        let ntsc = Fps {
            num: 30000,
            den: 1001,
        };
        assert_eq!(frame_time(&ntsc, 30000).to_string(), "1001");
    }

    #[test]
    fn still_clips_hold_zero_offset() {
        let clip = text_clip(80.0, 320.0);
        let motion = ClipMotion::still(0, &clip, 30);
        for k in [0, 15, 30, 60] {
            let (dx, dy) = motion.offset_at(Time::of_frame(k, 30));
            assert!(
                (dx, dy) == (0.0, 0.0),
                "still clip must not drift at frame {k}"
            );
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
            reversed: false,
        };
        let motion = ClipMotion::with_tweens(0, &clip, &[tween], 30);
        // At the tween midpoint the clip has moved half the distance.
        let (dx, _) = motion.offset_at(Time::new(3, 2));
        assert!(
            (dx - 50.0).abs() < 1e-9,
            "halfway tween must offset x by 50, got {dx}"
        );
        // Before the tween the clip holds its own position.
        let (dx0, _) = motion.offset_at(Time::ZERO);
        assert!((dx0).abs() < 1e-9, "got {dx0}");
    }

    // --- Sprint 2 directive evaluation (T009–T012) --------------------

    fn box_clip(x: f64, opacity: f64) -> Clip {
        Clip::Box(notation::model::BoxClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            start: 0.0,
            duration: 4.0,
            x,
            y: 5.0,
            width: 40.0,
            height: 30.0,
            fill: Rgb { r: 255, g: 0, b: 0 },
            fill_paint: notation::model::FillPaint::Solid,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            opacity,
            order: 0,
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

    fn path_tween_decl(
        target: usize,
        path: &str,
        path_to: Option<&str>,
        orient: bool,
        start: f64,
        duration: f64,
        ease: EaseDecl,
    ) -> TweenDecl {
        TweenDecl {
            target,
            property: String::new(),
            kind: TweenKind::To,
            from: None,
            to: None,
            start,
            duration,
            ease,
            stagger: 0.0,
            stagger_from: StaggerOrigin::Start,
            stagger_grid: None,
            repeat: 1,
            yoyo: false,
            repeat_delay: 0.0,
            path: Some(path.to_string()),
            path_to: path_to.map(str::to_string),
            orient,
            expr: None,
            line: 1,
            column: 1,
            from_color: None,
            to_color: None,
        }
    }

    fn path_stop(at: f64, path: &str) -> KeyStop {
        KeyStop {
            at,
            value: 0.0,
            value_color: None,
            path: Some(path.to_string()),
            ease: None,
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
            id: None,
            canvas: None,
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
            keys: Vec::new(),
            marks: Vec::new(),
            inputs: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Default::default(),
            normalize_loc: None,
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
            vec![tween_decl(
                0,
                "x",
                TweenKind::To,
                None,
                Some(110.0),
                1.0,
                1.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 10.0, "before the window the authored value holds");
        assert_eq!(
            at(30),
            10.0,
            "the start frame shows exactly the start value"
        );
        assert_eq!(
            at(45),
            60.0,
            "the midpoint frame is exactly halfway (linear)"
        );
        assert_eq!(at(60), 110.0, "the end frame shows exactly the end value");
        assert_eq!(at(90), 110.0, "past the window the end value holds");
    }

    #[test]
    fn from_tween_holds_given_then_returns_to_authored() {
        // T009: from 60 to authored 10 over [1, 2).
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(
                0,
                "x",
                TweenKind::From,
                Some(60.0),
                None,
                1.0,
                1.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 60.0);
        assert_eq!(
            at(30),
            60.0,
            "the start frame shows exactly the given value"
        );
        assert_eq!(at(45), 35.0, "linear midpoint between given and authored");
        assert_eq!(at(60), 10.0, "the end frame is back at authored");
    }

    #[test]
    fn from_to_tween_moves_between_givens() {
        // T009: 0 -> 100 over [0, 1), authored 10 ignored inside.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(
                0,
                "x",
                TweenKind::FromTo,
                Some(0.0),
                Some(100.0),
                0.0,
                1.0,
                EaseDecl::Linear,
            )],
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
            vec![tween_decl(
                1,
                "x",
                TweenKind::To,
                None,
                Some(120.0),
                0.0,
                1.0,
                EaseDecl::Linear,
            )],
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
                tween_decl(
                    0,
                    "scale",
                    TweenKind::FromTo,
                    Some(1.0),
                    Some(3.0),
                    0.0,
                    2.0,
                    EaseDecl::Linear,
                ),
                tween_decl(
                    0,
                    "rotation",
                    TweenKind::To,
                    None,
                    Some(90.0),
                    0.0,
                    2.0,
                    EaseDecl::Linear,
                ),
                tween_decl(
                    0,
                    "opacity",
                    TweenKind::From,
                    Some(0.0),
                    None,
                    0.0,
                    2.0,
                    EaseDecl::Linear,
                ),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        let start = at(0);
        assert_eq!(
            (start.scale, start.rotation, start.opacity),
            (1.0, 0.0, 0.0)
        );
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
        assert_eq!(
            at(67),
            70.0,
            "after the first window the later hold still shows"
        );
        assert_eq!(
            at(75),
            10.0,
            "after both windows the authored value returns"
        );
    }

    #[test]
    fn set_overrides_tween_inside_its_window_only() {
        // T011: outside the set window the tweened value shows.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![tween_decl(
                0,
                "x",
                TweenKind::To,
                None,
                Some(100.0),
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![set_decl(0, "x", 40.0, 0.5, 0.5)],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert!(
            (at(7) - 100.0 * 7.0 / 60.0).abs() < 1e-9,
            "tweened before the set"
        );
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
                vec![tween_decl(
                    0,
                    "x",
                    TweenKind::To,
                    None,
                    Some(100.0),
                    0.0,
                    2.0,
                    ease,
                )],
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
        assert!(
            mids.len() >= 11,
            "curves must shape middles distinctly, got {mids:?}"
        );
    }

    #[test]
    fn out_back_overshoot_varies_with_n_and_settles_exact() {
        // T012: out-back passes its end by an n-set amount, then settles.
        let run = |n: f64| -> Vec<f64> {
            let comp = comp_with(
                vec![box_clip(0.0, 1.0)],
                vec![tween_decl(
                    0,
                    "x",
                    TweenKind::To,
                    None,
                    Some(100.0),
                    0.0,
                    1.0,
                    EaseDecl::OutBack(n),
                )],
                vec![],
            );
            let motions = plan_motions(&comp, 30);
            (0..=30)
                .map(|k| motions[0].values_at(frame_time(&comp.fps, k)).x)
                .collect()
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

    // --- Sprint 7 keyframes (T001) and curves (T007) ---------------------

    fn key_stop(at: f64, value: f64, ease: Option<EaseDecl>) -> KeyStop {
        KeyStop {
            at,
            value,
            ease,
            line: 1,
            column: 1,
            value_color: None,
            path: None,
        }
    }

    fn key_decl(
        target: usize,
        property: &str,
        ease: Option<EaseDecl>,
        stops: Vec<KeyStop>,
    ) -> KeyDecl {
        KeyDecl {
            target,
            property: property.to_string(),
            ease,
            stops,
            stagger: 0.0,
            stagger_from: StaggerOrigin::Start,
            stagger_grid: None,
            repeat: 1,
            yoyo: false,
            repeat_delay: 0.0,
            orient: false,
            line: 1,
            column: 1,
        }
    }

    fn comp_with_keys(clips: Vec<Clip>, keys: Vec<KeyDecl>) -> Composition {
        Composition {
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 320,
            height: 240,
            duration: 4.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips,
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
            keys,
            marks: Vec::new(),
            inputs: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Default::default(),
            normalize_loc: None,
        }
    }

    #[test]
    fn keyframes_chain_segments_with_per_stop_easing() {
        // T001: stops 0/100/200 at 0s/1s/2s; segment one runs in-quad,
        // segment two runs linear.
        let comp = comp_with_keys(
            vec![box_clip(0.0, 1.0)],
            vec![key_decl(
                0,
                "x",
                None,
                vec![
                    key_stop(0.0, 0.0, Some(EaseDecl::InQuad)),
                    key_stop(1.0, 100.0, Some(EaseDecl::Linear)),
                    key_stop(2.0, 200.0, None),
                ],
            )],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 0.0, "the first stop shows exactly");
        assert_eq!(at(15), 25.0, "in-quad eases the first segment to a quarter");
        assert_eq!(at(30), 100.0, "the middle stop lands exactly");
        assert_eq!(at(45), 150.0, "linear eases the second segment halfway");
        assert_eq!(at(60), 200.0, "the last stop lands exactly");
        assert_eq!(at(90), 200.0, "the end value holds past the last stop");
    }

    #[test]
    fn key_ease_falls_back_from_stop_to_key_to_linear() {
        // T001: the segment ease is the stop's, else the key's, else linear.
        let run = |stop_ease: Option<EaseDecl>, key_ease: Option<EaseDecl>| -> f64 {
            let comp = comp_with_keys(
                vec![box_clip(0.0, 1.0)],
                vec![key_decl(
                    0,
                    "x",
                    key_ease,
                    vec![key_stop(0.0, 0.0, stop_ease), key_stop(2.0, 200.0, None)],
                )],
            );
            let motions = plan_motions(&comp, 30);
            motions[0].values_at(frame_time(&comp.fps, 30)).x
        };
        assert_eq!(
            run(Some(EaseDecl::InQuad), Some(EaseDecl::OutQuad)),
            50.0,
            "the stop easing wins over the key easing"
        );
        assert_eq!(
            run(None, Some(EaseDecl::OutQuad)),
            150.0,
            "the key easing covers stops without one"
        );
        assert_eq!(
            run(None, None),
            100.0,
            "linear covers segments without any easing"
        );
    }

    #[test]
    fn key_expansion_is_one_from_to_tween_per_segment() {
        // T001: three stops expand to two ordered from-to tweens with
        // the stop-to-key-to-linear easings attached.
        let decl = key_decl(
            0,
            "x",
            Some(EaseDecl::OutQuad),
            vec![
                key_stop(0.0, 10.0, Some(EaseDecl::InQuad)),
                key_stop(1.0, 60.0, None),
                key_stop(3.0, 110.0, Some(EaseDecl::Hold)),
            ],
        );
        let tweens = key_tweens(&decl);
        assert_eq!(tweens.len(), 2);
        assert!(tweens.iter().all(|t| t.kind == motion::Kind::FromTo));
        assert_eq!(tweens[0].at, secs_to_time(0.0));
        assert_eq!(tweens[0].end(), secs_to_time(1.0));
        assert_eq!(tweens[1].end(), secs_to_time(3.0));
        assert_eq!(tweens[0].ease, motion::Ease::InQuad);
        assert_eq!(tweens[1].ease, motion::Ease::OutQuad);
        assert_eq!(tweens[0].from[Prop::X.index()], Some(10.0));
        assert_eq!(tweens[0].to[Prop::X.index()], Some(60.0));
        assert_eq!(tweens[1].from[Prop::X.index()], Some(60.0));
        assert_eq!(tweens[1].to[Prop::X.index()], Some(110.0));
    }

    #[test]
    fn hold_keeps_the_prior_value_until_the_next_stop() {
        // T007: a hold segment shows its starting value, then jumps.
        let comp = comp_with_keys(
            vec![box_clip(0.0, 1.0)],
            vec![key_decl(
                0,
                "x",
                None,
                vec![
                    key_stop(0.0, 0.0, Some(EaseDecl::Hold)),
                    key_stop(1.0, 100.0, None),
                ],
            )],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 0.0);
        assert_eq!(at(15), 0.0, "hold keeps the prior value mid-segment");
        assert_eq!(at(29), 0.0, "hold keeps the prior value to the last frame");
        assert_eq!(at(30), 100.0, "the next stop lands exactly");
    }

    #[test]
    fn every_new_curve_shapes_a_keyframe_segment() {
        // T007: each new curve runs a 0 -> 100 segment over [0, 1) with
        // exact ends and a distinguishing middle.
        let run = |ease: EaseDecl| -> Vec<f64> {
            let comp = comp_with_keys(
                vec![box_clip(0.0, 1.0)],
                vec![key_decl(
                    0,
                    "x",
                    None,
                    vec![key_stop(0.0, 0.0, Some(ease)), key_stop(1.0, 100.0, None)],
                )],
            );
            let motions = plan_motions(&comp, 30);
            (0..=30)
                .map(|k| motions[0].values_at(frame_time(&comp.fps, k)).x)
                .collect()
        };
        let ends_exact = |v: &[f64], what: &str| {
            assert_eq!(v[0], 0.0, "{what} starts exactly");
            assert_eq!(v[30], 100.0, "{what} ends exactly");
        };
        let bez = run(EaseDecl::CubicBezier(0.0, 0.0, 1.0, 1.0));
        ends_exact(&bez, "cubic-bezier");
        assert!(
            (bez[15] - 50.0).abs() < 1e-6,
            "identity bezier midpoints, got {}",
            bez[15]
        );
        let steps = run(EaseDecl::Steps(4));
        ends_exact(&steps, "steps");
        assert_eq!(steps[7], 0.0, "steps(4) holds the first interval");
        assert_eq!(steps[8], 25.0, "steps(4) jumps at the interval end");
        let spring = run(EaseDecl::Spring(100.0, 5.0));
        ends_exact(&spring, "spring");
        assert!(
            spring.iter().cloned().fold(0.0f64, f64::max) > 100.0,
            "spring overshoots before it settles"
        );
        let bounce = run(EaseDecl::Bounce);
        ends_exact(&bounce, "bounce");
        assert!(
            (bounce[15] - 76.5625).abs() < 1e-9,
            "bounce midpoints, got {}",
            bounce[15]
        );
        let elastic = run(EaseDecl::Elastic);
        ends_exact(&elastic, "elastic");
        assert!(
            (elastic[15] - 101.5625).abs() < 1e-9,
            "elastic midpoints, got {}",
            elastic[15]
        );
        let wiggle = run(EaseDecl::Wiggle(1));
        ends_exact(&wiggle, "wiggle");
        assert!(
            (wiggle[15] - 50.0).abs() < 1e-9,
            "wiggle crosses the diagonal mid-segment, got {}",
            wiggle[15]
        );
        let hold = run(EaseDecl::Hold);
        ends_exact(&hold, "hold");
        assert_eq!(hold[15], 0.0, "hold keeps the prior value");
    }

    fn repeated_tween(repeat: u32, yoyo: bool, gap: f64) -> TweenDecl {
        let mut decl = tween_decl(
            0,
            "x",
            TweenKind::FromTo,
            Some(0.0),
            Some(100.0),
            0.0,
            1.0,
            EaseDecl::Linear,
        );
        decl.repeat = repeat;
        decl.yoyo = yoyo;
        decl.repeat_delay = gap;
        decl
    }

    #[test]
    fn repeat_replays_tween_values_across_passes() {
        // T004: from-to 0 -> 100 over [0, 1), three passes back to
        // back; every pass restarts at 0 and lands on 100.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![repeated_tween(3, false, 0.0)],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 0.0);
        assert_eq!(at(15), 50.0);
        assert_eq!(at(30), 0.0, "pass two restarts at the start value");
        assert_eq!(at(45), 50.0);
        assert_eq!(at(60), 0.0, "pass three restarts at the start value");
        assert_eq!(at(75), 50.0);
        assert_eq!(at(90), 100.0, "the last pass ends exactly");
        assert_eq!(at(120), 100.0, "past the last pass the end holds");
    }

    #[test]
    fn repeat_gap_holds_end_values_between_passes() {
        // T004: two passes with a 0.5s gap; the gap shows 100, the
        // second pass runs [1.5, 2.5).
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![repeated_tween(2, false, 0.5)],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(30), 100.0, "the first pass ends exactly");
        assert_eq!(at(37), 100.0, "the gap holds the end value");
        assert_eq!(at(44), 100.0, "the gap holds to its last frame");
        assert_eq!(at(45), 0.0, "the second pass restarts after the gap");
        assert_eq!(at(60), 50.0, "the second pass midpoints");
        assert_eq!(at(75), 100.0, "the second pass ends exactly");
    }

    #[test]
    fn yoyo_reverses_alternate_passes_exactly() {
        // T004: in-quad 0 -> 100 over [0, 1), two passes with yoyo;
        // the second pass traces the first backward: at local 0.25
        // it reads in-quad(0.75) = 56.25, not 1 - in-quad(0.25).
        let mut decl = repeated_tween(2, true, 0.0);
        decl.ease = EaseDecl::InQuad;
        let comp = comp_with(vec![box_clip(0.0, 1.0)], vec![decl], vec![]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        // Frame 8 is progress 8/30 through the pass; frame 38 is the
        // same local progress through the reversed pass, where the
        // easing runs mirrored at 1 - 8/30.
        let forward = 100.0 * (8.0f64 / 30.0).powi(2);
        let mirrored = 100.0 * (22.0f64 / 30.0).powi(2);
        assert!(
            (at(8) - forward).abs() < 1e-9,
            "forward pass eases in, got {}",
            at(8)
        );
        assert_eq!(at(30), 100.0, "the turnaround shows the end value");
        assert!(
            (at(38) - mirrored).abs() < 1e-9,
            "reversed pass mirrors the easing, got {}",
            at(38)
        );
        assert!(
            (at(45) - 25.0).abs() < 1e-9,
            "reversed midpoint, got {}",
            at(45)
        );
        assert_eq!(at(60), 0.0, "the reversed pass ends at the start value");
        assert_eq!(at(90), 0.0, "past the last pass the start value holds");
    }

    #[test]
    fn repeat_key_replays_the_stop_chain() {
        // T004: key stops 0 -> 100 over [0, 1), two passes; the chain
        // replays whole, then holds its end.
        let mut decl = key_decl(
            0,
            "x",
            None,
            vec![key_stop(0.0, 0.0, None), key_stop(1.0, 100.0, None)],
        );
        decl.repeat = 2;
        let comp = comp_with_keys(vec![box_clip(0.0, 1.0)], vec![decl]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(15), 50.0);
        assert_eq!(at(30), 0.0, "the second pass replays the chain");
        assert_eq!(at(45), 50.0);
        assert_eq!(at(60), 100.0, "the last pass lands on the last stop");
    }

    #[test]
    fn single_pass_reproduces_the_unrepeated_tween() {
        // T004: repeat 1 (or yoyo alone) plans exactly the authored
        // window, so unrepeated motion renders exactly as before.
        let decl = repeated_tween(1, true, 0.0);
        let expanded = expand_tween(&decl);
        let base = engine_tween(&decl).expect("valid tween");
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].at, base.at);
        assert_eq!(expanded[0].duration, base.duration);
        assert!(!expanded[0].reversed);
        let key = key_decl(
            0,
            "x",
            None,
            vec![key_stop(0.0, 0.0, None), key_stop(1.0, 100.0, None)],
        );
        assert_eq!(expand_key(&key).len(), 1);
    }

    fn staggered_tween(
        stagger: f64,
        origin: StaggerOrigin,
        grid: Option<StaggerGrid>,
    ) -> TweenDecl {
        let mut decl = tween_decl(
            0,
            "x",
            TweenKind::To,
            None,
            Some(110.0),
            0.0,
            1.0,
            EaseDecl::Linear,
        );
        decl.stagger = stagger;
        decl.stagger_from = origin;
        decl.stagger_grid = grid;
        decl
    }

    #[test]
    fn stagger_delays_pieces_from_each_origin() {
        // T009: box x 10 -> 110 over [0, 1), stagger 0.5 across four
        // pieces from the start: piece i starts i * 0.5s late.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![staggered_tween(0.5, StaggerOrigin::Start, None)],
            vec![],
        );
        let planned = planned_tweens(&comp, 0);
        let clip = &comp.clips[0];
        let piece = |k: u64, i: usize| {
            piece_values(
                clip,
                &planned,
                &[],
                30,
                frame_time(&comp.fps, k),
                i,
                4,
                &[],
                &[],
            )
            .x
        };
        assert_eq!(piece(0, 0), 10.0);
        assert_eq!(piece(15, 0), 60.0);
        assert_eq!(piece(30, 0), 110.0);
        assert_eq!(piece(15, 1), 10.0, "piece one has not started yet");
        assert_eq!(piece(30, 1), 60.0, "piece one runs half a second late");
        assert_eq!(piece(45, 1), 110.0);
        assert_eq!(piece(45, 3), 10.0, "piece three starts at 1.5s");
        assert_eq!(piece(75, 3), 110.0, "piece three ends at 2.5s");
        // From the end the delays flip: piece three moves at once,
        // piece zero starts 1.5s late.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![staggered_tween(0.5, StaggerOrigin::End, None)],
            vec![],
        );
        let planned = planned_tweens(&comp, 0);
        let clip = &comp.clips[0];
        let piece = |k: u64, i: usize| {
            piece_values(
                clip,
                &planned,
                &[],
                30,
                frame_time(&comp.fps, k),
                i,
                4,
                &[],
                &[],
            )
            .x
        };
        assert_eq!(piece(15, 3), 60.0, "the last piece moves first");
        assert_eq!(piece(15, 0), 10.0, "the first piece waits 1.5s");
        assert_eq!(piece(60, 0), 60.0, "the first piece runs 1.5s late");
        // From the center the middle pieces move first: delays
        // 0.75, 0.25, 0.25, 0.75 at stagger 0.5.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![staggered_tween(0.5, StaggerOrigin::Center, None)],
            vec![],
        );
        let planned = planned_tweens(&comp, 0);
        let clip = &comp.clips[0];
        let piece = |k: u64, i: usize| {
            piece_values(
                clip,
                &planned,
                &[],
                30,
                frame_time(&comp.fps, k),
                i,
                4,
                &[],
                &[],
            )
            .x
        };
        // Frame 7 (t=0.233s) precedes piece one's 0.25s start; frame
        // 23 runs 23/30 - 1/4 = 31/60 through its window.
        assert_eq!(piece(7, 1), 10.0, "piece one starts at 0.25s");
        let late = 10.0 + 100.0 * (23.0f64 / 30.0 - 0.25);
        assert!(
            (piece(23, 1) - late).abs() < 1e-9,
            "piece one runs a quarter late, got {}",
            piece(23, 1)
        );
        assert_eq!(piece(8, 0), 10.0, "piece zero waits 0.75s");
        let zero = 10.0 + 100.0 * (38.0f64 / 30.0 - 0.75);
        assert!(
            (piece(38, 0) - zero).abs() < 1e-9,
            "piece zero runs late, got {}",
            piece(38, 0)
        );
    }

    #[test]
    fn stagger_grid_spreads_piece_delays() {
        // T009: seven pieces in three columns from the center at
        // stagger 1.0 — delays 2, 1, 2, 1, 0, 1, 2; the middle piece
        // moves at once while the corners wait two seconds.
        let grid = Some(StaggerGrid {
            cols: 3,
            rows: None,
        });
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![staggered_tween(1.0, StaggerOrigin::Center, grid)],
            vec![],
        );
        let planned = planned_tweens(&comp, 0);
        let clip = &comp.clips[0];
        let piece = |k: u64, i: usize| {
            piece_values(
                clip,
                &planned,
                &[],
                30,
                frame_time(&comp.fps, k),
                i,
                7,
                &[],
                &[],
            )
            .x
        };
        assert_eq!(piece(15, 4), 60.0, "the middle piece moves at once");
        assert_eq!(piece(15, 0), 10.0, "a corner piece waits two seconds");
        assert_eq!(piece(75, 0), 60.0, "a corner piece runs two seconds late");
        assert_eq!(piece(30, 1), 10.0, "an edge piece starts at one second");
        assert_eq!(piece(45, 1), 60.0, "an edge piece runs one second late");
    }

    // --- Sprint 7 motion (T005/T006): 3D turns and visual properties ---

    fn color_tween_decl(
        target: usize,
        property: &str,
        kind: TweenKind,
        from: Option<Rgb>,
        to: Option<Rgb>,
        start: f64,
        duration: f64,
    ) -> TweenDecl {
        TweenDecl {
            target,
            property: property.to_string(),
            kind,
            from: None,
            to: None,
            from_color: from,
            to_color: to,
            start,
            duration,
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
        }
    }

    #[test]
    fn visual_tweens_interpolate_frame_exactly() {
        // T006: width 20->60, radius 0->8, border-width 0->4, blur
        // 0->4, all over [0, 1) linear; frame 15 is exactly halfway.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![
                tween_decl(
                    0,
                    "width",
                    TweenKind::FromTo,
                    Some(20.0),
                    Some(60.0),
                    0.0,
                    1.0,
                    EaseDecl::Linear,
                ),
                tween_decl(
                    0,
                    "radius",
                    TweenKind::FromTo,
                    Some(0.0),
                    Some(8.0),
                    0.0,
                    1.0,
                    EaseDecl::Linear,
                ),
                tween_decl(
                    0,
                    "border-width",
                    TweenKind::FromTo,
                    Some(0.0),
                    Some(4.0),
                    0.0,
                    1.0,
                    EaseDecl::Linear,
                ),
                tween_decl(
                    0,
                    "blur",
                    TweenKind::To,
                    None,
                    Some(4.0),
                    0.0,
                    1.0,
                    EaseDecl::Linear,
                ),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!(at(0).width, 20.0);
        assert_eq!(at(15).width, 40.0, "halfway width");
        assert_eq!(at(30).width, 60.0);
        assert_eq!(at(15).radius, 4.0, "halfway radius");
        assert_eq!(at(15).border_width, 2.0, "halfway border width");
        assert_eq!(at(15).blur, 2.0, "halfway blur");
        assert_eq!(at(60).blur, 4.0, "end holds");
    }

    #[test]
    fn fill_tween_blends_straight_rgba_channels() {
        // T006: #ff0000 -> #0000ff over [0, 1); each channel lerps
        // independently (straight RGBA): the midpoint is exactly
        // (127.5, 0.0, 127.5).
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![color_tween_decl(
                0,
                "fill",
                TweenKind::FromTo,
                Some(Rgb { r: 255, g: 0, b: 0 }),
                Some(Rgb { r: 0, g: 0, b: 255 }),
                0.0,
                1.0,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).fill;
        assert_eq!(at(0), [255.0, 0.0, 0.0]);
        assert_eq!(at(15), [127.5, 0.0, 127.5], "straight midpoint");
        assert_eq!(at(30), [0.0, 0.0, 255.0]);
        assert_eq!(at(60), [0.0, 0.0, 255.0], "end holds");
    }

    #[test]
    fn fill_to_tween_starts_from_authored_and_border_rides_too() {
        // T006: a `to` fill starts at the authored box fill; a
        // `border-color` tween rides three channel tweens the same way.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![
                color_tween_decl(
                    0,
                    "fill",
                    TweenKind::To,
                    None,
                    Some(Rgb { r: 0, g: 255, b: 0 }),
                    0.0,
                    2.0,
                ),
                color_tween_decl(
                    0,
                    "border-color",
                    TweenKind::FromTo,
                    Some(Rgb { r: 0, g: 0, b: 0 }),
                    Some(Rgb {
                        r: 255,
                        g: 255,
                        b: 255,
                    }),
                    0.0,
                    2.0,
                ),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!(at(0).fill, [255.0, 0.0, 0.0], "authored fill starts");
        assert_eq!(at(30).fill, [127.5, 127.5, 0.0], "halfway toward green");
        assert_eq!(at(30).border, [127.5, 127.5, 127.5], "halfway border gray");
        // A color `to` expands to three channel tweens with the same
        // window and kind.
        let expanded = engine_tweens(&color_tween_decl(
            0,
            "fill",
            TweenKind::To,
            None,
            Some(Rgb { r: 0, g: 255, b: 0 }),
            0.0,
            2.0,
        ));
        assert_eq!(expanded.len(), 3, "one tween per channel");
        assert!(expanded.iter().all(|t| t.kind == motion::Kind::To));
        assert_eq!(expanded[1].to[Prop::FillG.index()], Some(255.0));
        assert_eq!(expanded[0].to[Prop::FillG.index()], None);
    }

    #[test]
    fn fill_key_segments_interpolate_channels() {
        // T006: #000000 -> #ffffff key over [0, 1); the midpoint frame
        // is exactly (127.5, 127.5, 127.5) on every channel.
        let comp = comp_with_keys(
            vec![box_clip(10.0, 1.0)],
            vec![key_decl(
                0,
                "fill",
                None,
                vec![
                    KeyStop {
                        at: 0.0,
                        value: 0.0,
                        value_color: Some(Rgb { r: 0, g: 0, b: 0 }),
                        path: None,
                        ease: None,
                        line: 1,
                        column: 1,
                    },
                    KeyStop {
                        at: 1.0,
                        value: 0.0,
                        value_color: Some(Rgb {
                            r: 255,
                            g: 255,
                            b: 255,
                        }),
                        path: None,
                        ease: None,
                        line: 1,
                        column: 1,
                    },
                ],
            )],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).fill;
        assert_eq!(at(0), [0.0, 0.0, 0.0]);
        assert_eq!(at(15), [127.5, 127.5, 127.5]);
        assert_eq!(at(30), [255.0, 255.0, 255.0]);
    }

    #[test]
    fn set_on_width_holds_window_then_releases() {
        // T006: a width set holds 50 over [0.5, 1.5), then the authored
        // 40 returns. A color set expands to nothing (validate refuses
        // it; the engine treats it as no window).
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![],
            vec![set_decl(0, "width", 50.0, 0.5, 1.0)],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).width;
        assert_eq!(at(0), 40.0, "before the window the authored width holds");
        assert_eq!(at(15), 50.0, "the window start holds");
        assert_eq!(at(44), 50.0, "the window holds until its end");
        assert_eq!(at(45), 40.0, "past the window the authored width returns");
        assert!(motions[0].touches(Prop::Width));
        assert!(!motions[0].touches(Prop::Height));
        // Defense in depth: a fill set carries no window, so a fill
        // that slipped past validation would hold its authored color.
        assert!(set_window(&SetDecl {
            target: 0,
            property: "fill".to_string(),
            value: 3.0,
            start: 0.0,
            duration: 1.0,
            line: 1,
            column: 1,
        })
        .is_none());
    }

    #[test]
    fn rotation_xy_and_font_metrics_evaluate() {
        // T005/T006: rotation-x 0->90, font-size 16->32 (from authored
        // 48? no: from-to givens), letter-spacing 0->4, all linear.
        let comp = comp_with(
            vec![text_clip(10.0, 20.0)],
            vec![
                tween_decl(
                    0,
                    "rotation-x",
                    TweenKind::FromTo,
                    Some(0.0),
                    Some(90.0),
                    0.0,
                    1.0,
                    EaseDecl::Linear,
                ),
                tween_decl(
                    0,
                    "font-size",
                    TweenKind::FromTo,
                    Some(16.0),
                    Some(32.0),
                    0.0,
                    1.0,
                    EaseDecl::Linear,
                ),
                tween_decl(
                    0,
                    "letter-spacing",
                    TweenKind::To,
                    None,
                    Some(4.0),
                    0.0,
                    1.0,
                    EaseDecl::Linear,
                ),
            ],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!(at(0).rotate_x, 0.0);
        assert_eq!(at(15).rotate_x, 45.0);
        assert_eq!(at(30).rotate_x, 90.0);
        assert_eq!(at(15).font_size, 24.0);
        assert_eq!(at(15).letter_spacing, 2.0, "own spacing 0 blends to 4");
        assert!(motions[0].touches(Prop::FontSize));
        assert!(!motions[0].touches(Prop::Width), "text touches no width");
    }

    #[test]
    fn static_turn_and_filter_are_the_motion_own_values() {
        // T005/T006: a clip's static turn and filter blur are the
        // motion baseline — tweens blend from there, and still clips
        // hold them at every frame.
        let mut clip = box_clip(10.0, 1.0);
        if let Clip::Box(bc) = &mut clip {
            bc.turn.rotation_x = 10.0;
            bc.turn.rotation_y = -20.0;
            bc.filter_blur = 2.5;
        }
        let comp = comp_with(vec![clip], vec![], vec![]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!(at(0).rotate_x, 10.0);
        assert_eq!(at(29).rotate_y, -20.0);
        assert_eq!(at(0).blur, 2.5);
    }

    #[test]
    fn image_width_owns_canvas_when_unset() {
        // T006: a bare image fills the canvas (320x240 here), so its
        // animated width starts there; a declared width starts there.
        use notation::model::{Fit, ImageClip};
        let bare = Clip::Image(ImageClip {
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            path: "media/photo.png".to_string(),
            start: 0.0,
            duration: 4.0,
            x: 0.0,
            y: 0.0,
            width: None,
            height: None,
            fit: Fit::Contain,
            placement: notation::model::Placement::center(),
            order: 0,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
        });
        let comp = comp_with(vec![bare], vec![], vec![]);
        let motions = plan_motions(&comp, 30);
        let eval = motions[0].values_at(frame_time(&comp.fps, 0));
        assert_eq!(eval.width, 320.0, "unset width owns the canvas");
        assert_eq!(eval.height, 240.0, "unset height owns the canvas");
    }

    #[test]
    fn unstaggered_piece_values_match_whole_clip_motion() {
        // T009: without stagger every piece evaluates exactly like the
        // whole clip, so unstaggered motion renders exactly as before.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(
                0,
                "x",
                TweenKind::To,
                None,
                Some(110.0),
                0.0,
                1.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let planned = planned_tweens(&comp, 0);
        let clip = &comp.clips[0];
        for k in [0, 7, 15, 30, 45, 60] {
            let t = frame_time(&comp.fps, k);
            let whole = motions[0].values_at(t).x;
            assert_eq!(
                piece_values(clip, &planned, &[], 30, t, 0, 1, &[], &[]).x,
                whole,
                "single piece matches at frame {k}"
            );
            assert_eq!(
                piece_values(clip, &planned, &[], 30, t, 2, 4, &[], &[]).x,
                whole,
                "piece of an unstaggered cascade matches at frame {k}"
            );
        }
    }

    // --- Sprint 7 path travel (T008) ------------------------------------

    #[test]
    fn path_tween_places_clip_at_eased_distance() {
        // T008: travel "M0,0 L100,0" over [0, 2) linear at 30 fps.
        // Path coordinates are absolute: the clip sits at the eased
        // distance along the true length (100px).
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M0,0 L100,0",
                None,
                false,
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!((at(0).x, at(0).y), (0.0, 0.0));
        assert_eq!((at(30).x, at(30).y), (50.0, 0.0));
        assert_eq!((at(60).x, at(60).y), (100.0, 0.0));
        assert_eq!(
            (at(90).x, at(90).y),
            (100.0, 0.0),
            "past the window the path end holds"
        );
        // The eased distance shapes with the easing: in-quad squares
        // the progress, so the midpoint frame sits at a quarter.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M0,0 L100,0",
                None,
                false,
                0.0,
                2.0,
                EaseDecl::InQuad,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!((at(30).x, at(30).y), (25.0, 0.0));
    }

    #[test]
    fn path_tween_holds_authored_values_outside_its_window() {
        // T008: travel over [1, 2); before the window the authored
        // box position (10, 5) holds.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M0,0 L100,0",
                None,
                false,
                1.0,
                1.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!((at(0).x, at(0).y), (10.0, 5.0));
        assert_eq!((at(30).x, at(30).y), (0.0, 0.0));
        assert_eq!((at(45).x, at(45).y), (50.0, 0.0));
    }

    #[test]
    fn orient_adds_the_tangent_heading_to_rotation() {
        // T008: due south the heading is 90; it adds to the authored
        // rotation of 0, with placement following the path.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M10,10 L10,110",
                None,
                true,
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!((at(30).x, at(30).y), (10.0, 60.0));
        assert_eq!(at(30).rotation, 90.0);
        // Without `orient` the rotation stays authored.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M10,10 L10,110",
                None,
                false,
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        assert_eq!(
            motions[0].values_at(frame_time(&comp.fps, 30)).rotation,
            0.0
        );
    }

    #[test]
    fn path_corners_follow_one_side_each() {
        // T008: "M0,0 L100,0 L100,100" totals 200. Frame 30 lands
        // exactly on the corner with the incoming heading; frame 31
        // runs down the second segment.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M0,0 L100,0 L100,100",
                None,
                true,
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!((at(30).x, at(30).y), (100.0, 0.0));
        assert_eq!(at(30).rotation, 0.0, "the corner still heads east");
        let mid = at(31);
        assert!(
            (mid.x - 100.0).abs() < 1e-9 && (mid.y - 10.0 / 3.0).abs() < 1e-6,
            "got ({}, {})",
            mid.x,
            mid.y
        );
        assert_eq!(mid.rotation, 90.0, "past the corner the heading runs south");
    }

    #[test]
    fn morph_interpolates_command_numbers_per_frame() {
        // T008: "M0,0 L100,0" morphing to "M0,100 L100,100" over
        // [0, 1). Halfway the path runs at y=50 and the clip sits at
        // (50, 50); at the end it lands on the destination path.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M0,0 L100,0",
                Some("M0,100 L100,100"),
                false,
                0.0,
                1.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!((at(15).x, at(15).y), (50.0, 50.0));
        assert_eq!((at(30).x, at(30).y), (100.0, 100.0));
    }

    #[test]
    fn repeated_path_replays_and_yoyo_reverses_in_place() {
        // T008: three passes replay the travel; then yoyo mirrors.
        let mut tween = path_tween_decl(0, "M0,0 L100,0", None, false, 0.0, 1.0, EaseDecl::Linear);
        tween.repeat = 3;
        let comp = comp_with(vec![box_clip(0.0, 1.0)], vec![tween], vec![]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(15), 50.0);
        assert_eq!(at(45), 50.0, "pass two midpoints like pass one");
        assert_eq!(at(75), 50.0, "pass three midpoints like pass one");
        let mut tween = path_tween_decl(0, "M0,0 L100,0", None, false, 0.0, 1.0, EaseDecl::Linear);
        tween.repeat = 2;
        tween.yoyo = true;
        let comp = comp_with(vec![box_clip(0.0, 1.0)], vec![tween], vec![]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(15), 50.0);
        assert_eq!(at(45), 50.0, "the reversed pass mirrors the forward one");
        assert!((at(59) - 100.0 / 30.0).abs() < 1e-9, "got {}", at(59));
    }

    #[test]
    fn repeat_gap_holds_the_path_end() {
        // T008: a 0.5s gap between two passes holds (100, 0).
        let mut tween = path_tween_decl(0, "M0,0 L100,0", None, false, 0.0, 1.0, EaseDecl::Linear);
        tween.repeat = 2;
        tween.repeat_delay = 0.5;
        let comp = comp_with(vec![box_clip(0.0, 1.0)], vec![tween], vec![]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        assert_eq!((at(30).x, at(30).y), (100.0, 0.0));
        assert_eq!((at(37).x, at(37).y), (100.0, 0.0), "the gap holds the end");
        assert_eq!((at(45).x, at(45).y), (0.0, 0.0), "pass two restarts");
    }

    #[test]
    fn path_key_chains_stop_to_stop_with_morphs() {
        // T008: three closed squares morph downward while the clip
        // laps each. Closed loops chain seamlessly: every segment
        // ends where the next begins (its path start, reached again
        // by the close). Open paths snap across joints instead — each
        // segment spans its own morphed length.
        let decl = KeyDecl {
            target: 0,
            property: String::new(),
            ease: None,
            stops: vec![
                path_stop(0.0, "M0,0 L100,0 L100,100 L0,100 Z"),
                path_stop(1.0, "M0,100 L100,100 L100,200 L0,200 Z"),
                path_stop(2.0, "M0,200 L100,200 L100,300 L0,300 Z"),
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
        let comp = comp_with_keys(vec![box_clip(0.0, 1.0)], vec![decl]);
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k));
        // Halfway through segment one the square sits 50 down and the
        // clip has lapped halfway round it: (100, 150).
        assert_eq!((at(15).x, at(15).y), (100.0, 150.0));
        // The joint lands exactly on the next stop's path start, where
        // the next segment begins: no snap.
        assert_eq!((at(30).x, at(30).y), (0.0, 100.0));
        assert_eq!((at(45).x, at(45).y), (100.0, 250.0));
        assert_eq!((at(60).x, at(60).y), (0.0, 200.0));
    }

    #[test]
    fn staggered_path_pieces_cascade_along_the_curve() {
        // T008 + CAP-002: two pieces staggered 0.5s on a 1s travel —
        // piece 1 starts as piece 0 midpoints.
        let mut tween = path_tween_decl(0, "M0,0 L100,0", None, false, 0.0, 1.0, EaseDecl::Linear);
        tween.stagger = 0.5;
        let comp = comp_with(vec![box_clip(0.0, 1.0)], vec![tween], vec![]);
        let planned = planned_tweens(&comp, 0);
        let planned_paths = planned_paths(&comp, 0);
        let clip = &comp.clips[0];
        let at = |k: u64, piece: usize| {
            piece_values_with_paths(
                clip,
                &planned,
                &planned_paths,
                &[],
                30,
                frame_time(&comp.fps, k),
                piece,
                2,
                &[],
                &[],
            )
            .x
        };
        assert_eq!(at(15, 0), 50.0);
        assert_eq!(at(15, 1), 0.0, "piece one starts as piece zero midpoints");
        assert_eq!(at(30, 1), 50.0);
    }

    #[test]
    fn still_clips_without_paths_evaluate_untouched() {
        // T008 regression: a clip with no path windows evaluates
        // exactly as before, path or no path in the composition.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![path_tween_decl(
                0,
                "M0,0 L100,0",
                None,
                false,
                0.0,
                1.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        assert!(motions[0].touches(Prop::X), "path windows touch placement");
        assert!(motions[0].touches(Prop::Y), "path windows touch placement");
        assert!(
            !motions[0].touches(Prop::Rotation),
            "no orient leaves rotation alone"
        );
    }

    // --- Sprint 7 expressions and replicates (T010) --------------------

    fn expr_tween_decl(target: usize, property: &str, source: &str) -> TweenDecl {
        TweenDecl {
            target,
            property: property.to_string(),
            kind: TweenKind::To,
            from: None,
            to: None,
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
            expr: Some(source.to_string()),
            line: 1,
            column: 1,
            from_color: None,
            to_color: None,
        }
    }

    #[test]
    fn expr_tween_evaluates_per_frame_from_t() {
        // T010: box x reads `t*30+10` over [0, 2): frame k sits at
        // 10+k exactly, and the window end releases to the own value.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![expr_tween_decl(0, "x", "t*30+10")],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 10.0);
        assert_eq!(at(15), 25.0, "t=0.5s reads 25");
        assert_eq!(at(30), 40.0, "t=1s reads 40");
        assert_eq!(at(59), 10.0 + 59.0, "t=59/30 reads exactly");
        assert_eq!(at(60), 10.0, "past the window end the own value holds");
        assert!(
            motions[0].touches(Prop::X),
            "expression windows touch their property"
        );
        assert!(
            !motions[0].touches(Prop::Y),
            "untouched properties stay still"
        );
    }

    #[test]
    fn expr_reads_replicate_index_and_declared_inputs() {
        // T010: three clones read `i*10+amp` with amp=5, so clones sit
        // at 5/15/25 for every frame of the window.
        let mut comp = comp_with(
            vec![box_clip(0.0, 1.0), box_clip(0.0, 1.0), box_clip(0.0, 1.0)],
            vec![
                expr_tween_decl(0, "x", "i*10+amp"),
                expr_tween_decl(1, "x", "i*10+amp"),
                expr_tween_decl(2, "x", "i*10+amp"),
            ],
            vec![],
        );
        comp.inputs = vec![notation::model::InputDecl {
            name: "amp".to_string(),
            value: 5.0,
            line: 1,
            column: 1,
        }];
        comp.replicates = vec![notation::model::ReplicateGroup { first: 0, count: 3 }];
        assert_eq!(comp.replicate_index(1), 1);
        assert_eq!(comp.replicate_index(2), 2);
        let motions = plan_motions(&comp, 30);
        let at = |clip: usize, k: u64| motions[clip].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!((at(0, 0), at(1, 0), at(2, 0)), (5.0, 15.0, 25.0));
        assert_eq!((at(0, 30), at(1, 30), at(2, 30)), (5.0, 15.0, 25.0));
    }

    #[test]
    fn expr_seeded_random_matches_hand_value() {
        // T010: `random(7,0,100)` is constant across frames and equals
        // the SplitMix64 hand value, so workers and re-renders agree.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![expr_tween_decl(0, "x", "random(7,0,100)")],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        let hand = motion::expr::seeded_unit(7.0) * 100.0;
        assert_eq!(at(0), hand);
        assert_eq!(at(30), hand, "seeded draws never drift with t");
        assert!((0.0..100.0).contains(&hand));
    }

    #[test]
    fn set_wins_over_expr_and_expr_wins_over_tween() {
        // T010 layering: the segment tween blends 0->100, the
        // expression holds 50 over [0, 2), and the set holds 70 over
        // [0.5, 1.5). Each layer wins over the one below it.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![
                tween_decl(
                    0,
                    "x",
                    TweenKind::FromTo,
                    Some(0.0),
                    Some(100.0),
                    0.0,
                    2.0,
                    EaseDecl::Linear,
                ),
                expr_tween_decl(0, "x", "50"),
            ],
            vec![set_decl(0, "x", 70.0, 0.5, 1.0)],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 50.0, "expression wins over the segment tween");
        assert_eq!(at(30), 70.0, "the set wins over the expression");
        assert_eq!(at(59), 50.0, "past the set the expression holds again");
    }

    #[test]
    fn to_tween_without_from_animates_from_the_styled_base() {
        // T006: validation bakes a style into the clip, so a `to`
        // tween with no `from` opens at the styled base (here x=10)
        // and interpolates from there; the end holds past the window
        // like every tween.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(
                0,
                "x",
                TweenKind::To,
                None,
                Some(110.0),
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 10.0, "the window opens at the styled base");
        assert_eq!(at(30), 60.0, "t=1s rides the midpoint 10->110");
        assert_eq!(at(60), 110.0, "the end frame shows exactly the end value");
        assert_eq!(at(90), 110.0, "past the window the end value holds");
    }

    #[test]
    fn set_on_styled_property_releases_without_stale_restore() {
        // T006: a set over a styled property holds its window, then
        // releases to the live value below — the tween's value when
        // one runs, the styled base otherwise — never a stale copy.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(
                0,
                "x",
                TweenKind::To,
                None,
                Some(110.0),
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![set_decl(0, "x", 50.0, 1.0, 0.5)],
        );
        let motions = plan_motions(&comp, 30);
        let at = |k: u64| motions[0].values_at(frame_time(&comp.fps, k)).x;
        assert_eq!(at(0), 10.0, "before the set the tween runs from base");
        assert_eq!(at(30), 50.0, "inside the set window the set wins");
        assert_eq!(at(44), 50.0, "the last set frame still holds");
        assert_eq!(
            at(45),
            85.0,
            "past the set the live tween value shows, not a stale restore"
        );
    }

    #[test]
    fn piece_values_reads_piece_as_i() {
        // T010: per-piece evaluation reads the piece index as `i`, so
        // piece 2 of 4 evaluates `i*5` to 10 at any instant.
        let comp = comp_with(
            vec![box_clip(0.0, 1.0)],
            vec![expr_tween_decl(0, "x", "i*5")],
            vec![],
        );
        let planned = planned_tweens(&comp, 0);
        let exprs = planned_exprs(&comp, 0);
        assert_eq!(exprs.len(), 1);
        let clip = &comp.clips[0];
        let at = |k: u64, piece: usize| {
            piece_values(
                clip,
                &planned,
                &[],
                30,
                frame_time(&comp.fps, k),
                piece,
                4,
                &exprs,
                &[],
            )
            .x
        };
        assert_eq!(at(0, 0), 0.0);
        assert_eq!(at(0, 2), 10.0);
        assert_eq!(at(30, 3), 15.0);
    }

    #[test]
    fn expr_free_motion_plans_exactly_as_before() {
        // T010/T011 regression: with no `expr` anywhere, planning
        // produces no windows and values match the segment blend.
        let comp = comp_with(
            vec![box_clip(10.0, 1.0)],
            vec![tween_decl(
                0,
                "x",
                TweenKind::FromTo,
                Some(10.0),
                Some(110.0),
                0.0,
                2.0,
                EaseDecl::Linear,
            )],
            vec![],
        );
        assert!(planned_exprs(&comp, 0).is_empty());
        let motions = plan_motions(&comp, 30);
        assert_eq!(motions[0].values_at(frame_time(&comp.fps, 30)).x, 60.0);
    }
}
