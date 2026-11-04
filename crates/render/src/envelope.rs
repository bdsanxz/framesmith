//! Per-sample sound envelopes (Sprint 10 sound, CAP-001/CAP-002).
//!
//! `key`/`tween`/`set` directives over `volume` (0..2) and `pan`
//! (−1..1) resolve here to time-domain segments, then to
//! sample-boundary segments evaluated once per placed sample. The same
//! segment evaluator backs `fade-in`/`fade-out` sugar (CAP-002), so
//! fades and explicit envelopes null-test equal and pre-existing fade
//! outputs stay byte-identical.
//!
//! Sample-boundary convention: sample `i` of a placed clip spans
//! boundaries `[i, i + 1]` relative to the clip start and evaluates at
//! its end boundary `b = i + 1`. A segment `[q0, q1]` covers samples
//! with `q0 < b <= q1`; outside every segment the chain holds its
//! edges (the opening `from` before everything starts, the latest-ended
//! segment's `to` afterwards and across gaps). Overlapping segments
//! resolve later-in-document-order wins, like windowed `set`s.
//!
//! Composition per sample: the static gain times the sugared fades
//! times the explicit volume envelope times the pan balance. Fades
//! multiply with volume (edge smoothing survives a level ride, and
//! overlapping fade pairs keep their historical product); explicit
//! volume segments compose last-wins, so a fade pair null-tests equal
//! to its explicit twin exactly when the pair does not overlap.
//!
//! Linear segments evaluate sample-exactly in `f32`: rising segments
//! interpolate from the left boundary, falling segments from the
//! right, so `fade-in`/`fade-out` sugar reproduces the historical
//! `fade_factor` bits exactly. Eased segments evaluate in `f64` and
//! round once to `f32`. Evaluated levels clamp to the property range
//! (`volume` 0..2, `pan` −1..1), mirroring the draw-time clamp for
//! eased overshoot; `hold` easing keeps the segment's `from` value
//! until the next stop, then jumps.
//!
//! Stops rounding to the same sample boundary collapse (the degenerate
//! segment drops); fade/envelope equivalence is sample-exact when
//! every boundary lands on a sample, which exact-second fixtures do.
//! The mix sums in composition document order exactly like before, so
//! re-renders stay byte-identical across reruns and worker counts.

use crate::motion_plan::ease_of;
use motion::Ease;
use notation::model::{Composition, EaseDecl};

/// Mix sample rate: envelope boundaries quantize to these samples.
const RATE: f64 = 48_000.0;

/// One envelope segment in absolute composition seconds, in document
/// order (tweens, sets, and keys merged by their `line`/`column`).
#[derive(Debug, Clone)]
pub struct SoundSeg {
    pub start: f64,
    pub end: f64,
    pub from: f64,
    pub to: f64,
    pub ease: EaseDecl,
    pub line: usize,
    pub col: usize,
}

/// The envelope of one clip: volume multiplier segments plus pan
/// segments, each in document order. Empty means the static behavior
/// (unity gain, centered pan).
#[derive(Debug, Clone, Default)]
pub struct SoundEnvelope {
    pub vol: Vec<SoundSeg>,
    pub pan: Vec<SoundSeg>,
}

/// One envelope segment quantized to sample boundaries relative to
/// the clip start. `linear` segments run the sample-exact `f32` path;
/// eased segments run the `f64` path.
#[derive(Debug, Clone)]
pub struct QSeg {
    pub q0: i128,
    pub q1: i128,
    pub from: f64,
    pub to: f64,
    pub ease: Ease,
    pub linear: bool,
}

/// Collect the sound envelope of clip `target` (Sprint 10, CAP-001):
/// every `tween`/`set`/`key` over `volume`/`pan` naming the clip, in
/// document order. Tween endpoints a form omits resolve to the
/// property's own value (unity volume, centered pan), exactly like
/// visual `to`/`from` tweens. Validation guarantees well-formed
/// directives; the `unwrap_or` fallbacks only guard direct API misuse.
pub fn resolve_envelope(comp: &Composition, target: usize) -> SoundEnvelope {
    let mut vol: Vec<SoundSeg> = Vec::new();
    let mut pan: Vec<SoundSeg> = Vec::new();
    macro_rules! push_seg {
        ($property:expr, $seg:expr) => {
            if $property == "pan" {
                pan.push($seg);
            } else {
                vol.push($seg);
            }
        };
    }
    for t in comp
        .tweens
        .iter()
        .filter(|t| t.target == target && (t.property == "volume" || t.property == "pan"))
    {
        let own = notation::model::sound_own_value(&t.property);
        let (from, to) = match t.kind {
            notation::model::TweenKind::To => (own, t.to.unwrap_or(own)),
            notation::model::TweenKind::From => (t.from.unwrap_or(own), own),
            notation::model::TweenKind::FromTo => (t.from.unwrap_or(own), t.to.unwrap_or(own)),
        };
        let seg = SoundSeg {
            start: t.start,
            end: t.start + t.duration,
            from,
            to,
            ease: t.ease,
            line: t.line,
            col: t.column,
        };
        push_seg!(t.property, seg);
    }
    for s in comp
        .sets
        .iter()
        .filter(|s| s.target == target && (s.property == "volume" || s.property == "pan"))
    {
        let seg = SoundSeg {
            start: s.start,
            end: s.start + s.duration,
            from: s.value,
            to: s.value,
            ease: EaseDecl::Linear,
            line: s.line,
            col: s.column,
        };
        push_seg!(s.property, seg);
    }
    for k in comp.keys.iter().filter(|k| {
        k.target == target && (k.property == "volume" || k.property == "pan") && !k.stops.is_empty()
    }) {
        for pair in k.stops.windows(2) {
            let ease = pair[0].ease.or(k.ease).unwrap_or(EaseDecl::Linear);
            let seg = SoundSeg {
                start: pair[0].at,
                end: pair[1].at,
                from: pair[0].value,
                to: pair[1].value,
                ease,
                line: pair[0].line,
                col: pair[0].column,
            };
            push_seg!(k.property, seg);
        }
    }
    vol.sort_by_key(|s| (s.line, s.col));
    pan.sort_by_key(|s| (s.line, s.col));
    SoundEnvelope { vol, pan }
}

/// Quantize time-domain segments to sample boundaries relative to
/// `clip_start` (Sprint 10, CAP-001/CAP-002): boundary `q` is the end
/// of sample `q - 1`. Segments collapsing to zero width drop, so
/// evaluation never divides by zero.
pub fn quantize(segs: &[SoundSeg], clip_start: f64) -> Vec<QSeg> {
    segs.iter()
        .filter_map(|s| {
            let q0 = ((s.start - clip_start) * RATE).round() as i128;
            let q1 = ((s.end - clip_start) * RATE).round() as i128;
            if q1 <= q0 {
                return None;
            }
            Some(QSeg {
                q0,
                q1,
                from: s.from,
                to: s.to,
                ease: ease_of(&s.ease),
                linear: matches!(s.ease, EaseDecl::Linear),
            })
        })
        .collect()
}

/// Evaluate one linear segment sample-exactly (the single gain
/// primitive behind fades and envelopes): rising segments interpolate
/// from the left boundary, falling segments from the right, each in
/// one `f32` division. `fade-in` sugar `(0 → 1 over [0, n))` and
/// `fade-out` sugar `(1 → 0 over [n − m, n))` reproduce the historical
/// `fade_factor` bits exactly. Differences arrive as `i128` so absurd
/// inputs saturate into comparisons instead of panicking.
pub fn eval_linear(from: f32, to: f32, b: i128, q0: i128, q1: i128) -> f32 {
    let span = (q1 - q0) as f32;
    if to >= from {
        from + (to - from) * ((b - q0) as f32 / span)
    } else {
        to + (from - to) * ((q1 - b) as f32 / span)
    }
}

/// Evaluate one quantized segment at end boundary `b`, clamped to
/// `[lo, hi]` (Sprint 10, CAP-001): linear segments run sample-exactly
/// via [`eval_linear`]; eased segments run in `f64` and round once.
/// The caller guarantees `q0 < b <= q1`.
fn eval_seg(s: &QSeg, b: i128, lo: f32, hi: f32) -> f32 {
    let v = if s.linear {
        eval_linear(s.from as f32, s.to as f32, b, s.q0, s.q1)
    } else {
        let p = (b - s.q0) as f64 / (s.q1 - s.q0) as f64;
        (s.from + (s.to - s.from) * s.ease.apply(p)) as f32
    };
    v.clamp(lo, hi)
}

/// Evaluate one envelope chain at end boundary `b`, clamped to
/// `[lo, hi]` (Sprint 10, CAP-001): the last covering segment in
/// document order wins; with no covering segment the chain holds the
/// latest-ended segment's `to` (or the opening `from` before
/// everything starts). The caller guarantees a non-empty chain.
pub fn eval_chain(segs: &[QSeg], b: i128, lo: f32, hi: f32) -> f32 {
    debug_assert!(!segs.is_empty());
    for s in segs.iter().rev() {
        if s.q0 < b && b <= s.q1 {
            return eval_seg(s, b, lo, hi);
        }
    }
    let mut best: Option<&QSeg> = None;
    for s in segs {
        if s.q1 < b {
            match best {
                // A strictly later end wins; end ties keep document
                // order (later iterated replaces), matching last-wins.
                Some(cur) if cur.q1 > s.q1 => {}
                _ => best = Some(s),
            }
        }
    }
    match best {
        Some(s) => (s.to as f32).clamp(lo, hi),
        None => (segs[0].from as f32).clamp(lo, hi),
    }
}

/// Equal-gain balance for a pan value (Sprint 10, CAP-001): center
/// holds both channels at unity, edges silence exactly one side.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    if pan <= 0.0 {
        (1.0, 1.0 + pan)
    } else {
        (1.0 - pan, 1.0)
    }
}

/// One crossfade gain segment (Sprint 10 sound, CAP-003), quantized to
/// sample boundaries relative to the clip start under the same
/// boundary convention as [`QSeg`]: sample `i` evaluates at end
/// boundary `b = i + 1`, and the segment covers `q0 < b <= q1`.
/// Outgoing segments belong to the earlier clip (they fall 1 → 0, or
/// cut to exact silence for `none`); incoming segments belong to the
/// later clip (they rise 0 → 1). A `none` pair carries no incoming
/// segment: the later clip plays full while the earlier cuts.
#[derive(Debug, Clone)]
pub struct XSeg {
    pub q0: i128,
    pub q1: i128,
    pub shape: notation::model::Crossfade,
    pub outgoing: bool,
}

/// One voice-duck gate (Sprint 10 sound, CAP-004): composition samples
/// `v0 <= k < v1` (`k` the absolute mix index) scale by `amount`;
/// every other sample plays full. Membership follows the voice
/// timeline window exactly — no signal detection — so the gate is
/// deterministic by construction.
#[derive(Debug, Clone, Copy)]
pub struct DuckWin {
    pub v0: usize,
    pub v1: usize,
    pub amount: f32,
}

/// Sample-exact crossfade gain of one segment at end boundary `b` (the
/// caller guarantees `q0 < b <= q1`): linear pairs run the shared
/// [`eval_linear`] primitive (direction-aware, like fades);
/// constant-power pairs run the equal-power cosine/sine pair in `f64`
/// rounded once to `f32`, with exact endpoints (full at the handoff
/// start, silent/full at its end); `none` cuts the outgoing side to
/// exact silence.
pub fn xfade_gain(s: &XSeg, b: i128) -> f32 {
    use notation::model::Crossfade;
    match (s.shape, s.outgoing) {
        (Crossfade::None, true) => 0.0,
        (Crossfade::None, false) => 1.0,
        (Crossfade::Linear, true) => eval_linear(1.0, 0.0, b, s.q0, s.q1),
        (Crossfade::Linear, false) => eval_linear(0.0, 1.0, b, s.q0, s.q1),
        (Crossfade::ConstantPower, outgoing) => {
            if b == s.q1 {
                return if outgoing { 0.0 } else { 1.0 };
            }
            let p = (b - s.q0) as f64 / (s.q1 - s.q0) as f64;
            let angle = p * std::f64::consts::FRAC_PI_2;
            if outgoing {
                angle.cos() as f32
            } else {
                angle.sin() as f32
            }
        }
    }
}

/// Plan one composition's crossfade gain segments (Sprint 10 sound,
/// CAP-003): per-clip [`XSeg`] lists in document order, quantized to
/// each clip's own start exactly like [`quantize`]. Sub-sample overlaps
/// collapse (the degenerate segment drops) and sum instead. The plan
/// is deterministic, so re-renders blend identical gains.
pub fn plan_xfade(comp: &Composition) -> Vec<Vec<XSeg>> {
    use notation::model::Crossfade;
    let mut segs: Vec<Vec<XSeg>> = comp.clips.iter().map(|_| Vec::new()).collect();
    for over in notation::model::sound_overlaps(&comp.clips, comp.duration) {
        if over.early_fades {
            let cs = comp.clips[over.early].start();
            let q0 = ((over.eff_start - cs) * RATE).round() as i128;
            let q1 = ((over.eff_end - cs) * RATE).round() as i128;
            if q1 > q0 {
                segs[over.early].push(XSeg {
                    q0,
                    q1,
                    shape: over.shape,
                    outgoing: true,
                });
            }
        }
        if over.shape != Crossfade::None {
            let cs = comp.clips[over.late].start();
            let q0 = ((over.eff_start - cs) * RATE).round() as i128;
            let q1 = ((over.eff_end - cs) * RATE).round() as i128;
            if q1 > q0 {
                segs[over.late].push(XSeg {
                    q0,
                    q1,
                    shape: over.shape,
                    outgoing: false,
                });
            }
        }
    }
    segs
}

/// Quantize one bed clip's duck gate to composition samples (Sprint 10
/// sound, CAP-004): the voice window bounds round exactly like clip
/// placement, so the gate opens and closes on the voice's own
/// samples. An empty window ducks nothing.
pub fn plan_duck(comp: &Composition, bed: usize) -> Option<DuckWin> {
    let (vs, ve, amount) = notation::model::duck_target_window(&comp.clips, bed)?;
    let v0 = (vs * RATE).round().max(0.0) as usize;
    let v1 = (ve * RATE).round().max(0.0) as usize;
    if v1 <= v0 {
        return None;
    }
    Some(DuckWin { v0, v1, amount })
}

/// Run one feedback echo over a mono signal in place (Sprint 10 sound,
/// CAP-005): the first `dry_len` samples carry the dry clip, the rest
/// is zeroed tail room. Tap `k >= 1` lands exactly `k * delay_secs`
/// later with gain `mix * feedback^(k-1)`; the dry signal passes at
/// unity, so `mix = 0` (or zero spacing) leaves the buffer untouched.
/// A single `f32` pass in sample order, so re-renders agree bit for
/// bit at any worker count.
pub fn apply_delay(signal: &mut [f32], dry_len: usize, delay_secs: f64, feedback: f64, mix: f64) {
    if !delay_secs.is_finite()
        || delay_secs <= 0.0
        || !mix.is_finite()
        || mix <= 0.0
        || signal.is_empty()
    {
        return;
    }
    let spaced = (delay_secs * RATE).round();
    if spaced < 1.0 {
        return;
    }
    let d = spaced as usize;
    let fb = feedback as f32;
    let wet = mix as f32;
    let mut line = vec![0.0f32; d];
    let mut pos = 0usize;
    for (n, sample) in signal.iter_mut().enumerate() {
        let x = if n < dry_len { *sample } else { 0.0 };
        let echo = line[pos];
        *sample = x + wet * echo;
        line[pos] = x + fb * echo;
        pos += 1;
        if pos == d {
            pos = 0;
        }
    }
}

/// Bloom one deterministic room tail over a mono signal in place
/// (Sprint 10 sound, CAP-005): a fixed 30 ms feedback comb whose first
/// tap carries `reverb` and recirculates at `reverb / 2`, so tap
/// `k >= 1` lands at `k * 30 ms` with gain `reverb * (reverb/2)^(k-1)`.
/// The dry signal passes at unity; `reverb = 0` leaves the buffer
/// untouched. Seeded by nothing and ordered by sample, so re-renders
/// agree bit for bit.
pub fn apply_reverb(signal: &mut [f32], dry_len: usize, reverb: f64) {
    if !reverb.is_finite() || reverb <= 0.0 || signal.is_empty() {
        return;
    }
    let d = notation::model::REVERB_COMB_SAMPLES;
    let fb = (reverb / 2.0) as f32;
    let wet = reverb as f32;
    let mut line = vec![0.0f32; d];
    let mut pos = 0usize;
    for (n, sample) in signal.iter_mut().enumerate() {
        let x = if n < dry_len { *sample } else { 0.0 };
        let room = line[pos];
        *sample = x + wet * room;
        line[pos] = x + fb * room;
        pos += 1;
        if pos == d {
            pos = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::fade_factor;
    use notation::model::{Fps, Rgb};

    fn qseg(q0: i128, q1: i128, from: f64, to: f64, ease: Ease) -> QSeg {
        QSeg {
            q0,
            q1,
            from,
            to,
            ease,
            linear: matches!(ease, Ease::Linear),
        }
    }

    #[test]
    fn linear_segments_hit_hand_computed_samples() {
        // T001: enveloped levels match hand math at exact binary points.
        assert_eq!(eval_linear(0.0, 1.0, 1, 0, 4), 0.25);
        assert_eq!(eval_linear(0.0, 1.0, 4, 0, 4), 1.0);
        assert_eq!(eval_linear(1.0, 0.0, 3, 0, 4), 0.25);
        assert_eq!(eval_linear(1.0, 0.0, 4, 0, 4), 0.0);
        assert_eq!(eval_linear(0.2, 0.2, 7, 0, 10), 0.2);
        // Direction-aware forms agree at shared endpoints.
        assert_eq!(eval_linear(0.0, 2.0, 48000, 0, 48000), 2.0);
        assert_eq!(eval_linear(2.0, 0.0, 48000, 0, 48000), 0.0);
    }

    #[test]
    fn chains_hold_edges_gaps_and_last_wins() {
        // One segment: covered samples interpolate, edges hold.
        let one = vec![qseg(0, 100, 0.0, 1.0, Ease::Linear)];
        assert_eq!(eval_chain(&one, 50, 0.0, 2.0), 0.5);
        assert_eq!(eval_chain(&one, 100, 0.0, 2.0), 1.0);
        assert_eq!(eval_chain(&one, 101, 0.0, 2.0), 1.0);
        assert_eq!(eval_chain(&one, 200, 0.0, 2.0), 1.0);
        // A gap holds the latest-ended segment's `to`.
        let gap = vec![
            qseg(0, 10, 0.0, 1.0, Ease::Linear),
            qseg(20, 30, 0.0, 1.0, Ease::Linear),
        ];
        assert_eq!(eval_chain(&gap, 15, 0.0, 2.0), 1.0);
        assert_eq!(eval_chain(&gap, 25, 0.0, 2.0), 0.5);
        // Before everything starts the chain holds the opening `from`.
        let late = vec![qseg(100, 200, 0.25, 0.75, Ease::Linear)];
        assert_eq!(eval_chain(&late, 50, 0.0, 2.0), 0.25);
        // Overlapping segments resolve later-in-document-order wins.
        let over = vec![
            qseg(0, 10, 0.0, 1.0, Ease::Linear),
            qseg(0, 10, 0.0, 0.5, Ease::Linear),
        ];
        assert_eq!(eval_chain(&over, 5, 0.0, 2.0), 0.25);
    }

    #[test]
    fn eased_segments_follow_their_curve_and_clamp() {
        // In-quad at half progress is exactly one quarter.
        let seg = vec![qseg(0, 100, 0.0, 1.0, Ease::InQuad)];
        assert_eq!(eval_chain(&seg, 50, 0.0, 2.0), 0.25);
        // Hold keeps the opening value until the segment end, where it
        // lands on the next stop (every easing snaps p == 1 to 1).
        let hold = vec![qseg(0, 100, 0.4, 0.9, Ease::Hold)];
        assert_eq!(eval_chain(&hold, 1, 0.0, 2.0), 0.4);
        assert_eq!(eval_chain(&hold, 99, 0.0, 2.0), 0.4);
        assert_eq!(eval_chain(&hold, 100, 0.0, 2.0), 0.9);
        assert_eq!(eval_chain(&hold, 101, 0.0, 2.0), 0.9);
        // Out-of-range levels clamp to the property bounds (direct API
        // misuse; validation never produces these).
        let hot = vec![qseg(0, 10, 5.0, 5.0, Ease::Linear)];
        assert_eq!(eval_chain(&hot, 5, 0.0, 2.0), 2.0);
        assert_eq!(eval_chain(&hot, 5, -1.0, 1.0), 1.0);
    }

    #[test]
    fn pan_law_holds_center_and_silences_edges() {
        assert_eq!(pan_gains(0.0), (1.0, 1.0));
        assert_eq!(pan_gains(-1.0), (1.0, 0.0));
        assert_eq!(pan_gains(1.0), (0.0, 1.0));
        assert_eq!(pan_gains(-0.5), (1.0, 0.5));
        assert_eq!(pan_gains(0.5), (0.5, 1.0));
    }

    #[test]
    fn resolve_envelope_collects_directives_in_document_order() {
        // T001: tweens resolve missing endpoints to own values, sets
        // hold constants, keys expand stop-to-stop with the key easing
        // as fallback, and everything merges in document order.
        use notation::model::*;
        let audio = Clip::Audio(AudioClip {
            path: "s.wav".to_string(),
            start: 0.0,
            duration: 4.0,
            id: None,
            track: TrackKind::Audio,
            label: None,
            start_after: None,
            trim_start: 0.0,
            trim_end: None,
            rate: 1.0,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            crossfade: notation::model::Crossfade::default(),
            duck_under: None,
            fx: Default::default(),
            waveform: Default::default(),
            hidden: false,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
            line: 1,
            column: 1,
        });
        let comp = Composition {
            name: "t".to_string(),
            id: None,
            canvas: None,
            width: 64,
            height: 64,
            duration: 4.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: vec![audio],
            fonts: Vec::new(),
            tweens: vec![TweenDecl {
                target: 0,
                property: "volume".to_string(),
                kind: TweenKind::To,
                from: None,
                to: Some(0.5),
                from_color: None,
                to_color: None,
                start: 2.0,
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
                line: 10,
                column: 5,
            }],
            sets: vec![SetDecl {
                target: 0,
                property: "pan".to_string(),
                value: -0.5,
                start: 0.0,
                duration: 4.0,
                line: 3,
                column: 5,
            }],
            keys: vec![KeyDecl {
                target: 0,
                property: "volume".to_string(),
                ease: Some(EaseDecl::InQuad),
                stops: vec![
                    KeyStop {
                        at: 0.0,
                        value: 0.0,
                        value_color: None,
                        path: None,
                        ease: None,
                        line: 5,
                        column: 9,
                    },
                    KeyStop {
                        at: 1.0,
                        value: 1.0,
                        value_color: None,
                        path: None,
                        ease: Some(EaseDecl::Linear),
                        line: 6,
                        column: 9,
                    },
                ],
                stagger: 0.0,
                stagger_from: StaggerOrigin::Start,
                stagger_grid: None,
                repeat: 1,
                yoyo: false,
                repeat_delay: 0.0,
                orient: false,
                line: 5,
                column: 5,
            }],
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
        };
        let env = resolve_envelope(&comp, 0);
        // The key (lines 5-6) sorts before the tween (line 10).
        assert_eq!(env.vol.len(), 2);
        assert_eq!((env.vol[0].start, env.vol[0].end), (0.0, 1.0));
        assert_eq!((env.vol[0].from, env.vol[0].to), (0.0, 1.0));
        assert!(matches!(env.vol[0].ease, EaseDecl::InQuad));
        assert_eq!((env.vol[1].start, env.vol[1].end), (2.0, 3.0));
        // A `to` tween starts from unity volume.
        assert_eq!((env.vol[1].from, env.vol[1].to), (1.0, 0.5));
        assert_eq!(env.pan.len(), 1);
        assert_eq!((env.pan[0].from, env.pan[0].to), (-0.5, -0.5));
        // Other targets resolve empty.
        let other = resolve_envelope(&comp, 7);
        assert!(other.vol.is_empty() && other.pan.is_empty());
    }

    #[test]
    fn quantize_maps_times_to_sample_boundaries() {
        let segs = vec![SoundSeg {
            start: 1.0,
            end: 2.0,
            from: 0.0,
            to: 1.0,
            ease: EaseDecl::Linear,
            line: 1,
            col: 1,
        }];
        let q = quantize(&segs, 1.0);
        assert_eq!(q.len(), 1);
        assert_eq!((q[0].q0, q[0].q1), (0, 48_000));
        assert!(q[0].linear);
        // Collapsed stops drop instead of dividing by zero.
        let tiny = vec![SoundSeg {
            start: 1.0,
            end: 1.0 + 1e-9,
            from: 0.0,
            to: 1.0,
            ease: EaseDecl::Linear,
            line: 1,
            col: 1,
        }];
        assert!(quantize(&tiny, 1.0).is_empty());
    }

    /// The pre-envelope fade formula, kept as the behavior lock: the
    /// envelope-primitive reimplementation must reproduce it bit for
    /// bit (T002: pre-existing fade outputs stay byte-identical).
    fn legacy_fade_factor(i: usize, n: usize, fade_in_n: usize, fade_out_n: usize) -> f32 {
        let mut g = 1.0f32;
        if fade_in_n > 0 && i < fade_in_n {
            g *= (i + 1) as f32 / fade_in_n as f32;
        }
        if fade_out_n > 0 && i < n {
            let tail = n - 1 - i;
            if tail < fade_out_n {
                g *= tail as f32 / fade_out_n as f32;
            }
        }
        g
    }

    #[test]
    fn fade_sugar_reproduces_the_legacy_formula_exactly() {
        // Sweep window sizes, fade counts (including overlong fades
        // and empty windows), and every covered sample index.
        for n in [0usize, 1, 2, 3, 4, 5, 7, 33, 100, 480] {
            for (fi, fo) in [
                (0, 0),
                (1, 0),
                (0, 1),
                (2, 2),
                (5, 5),
                (7, 3),
                (100, 100),
                (1000, 1000),
            ] {
                for i in 0..n {
                    assert_eq!(
                        fade_factor(i, n, fi, fo),
                        legacy_fade_factor(i, n, fi, fo),
                        "i={i} n={n} fi={fi} fo={fo}"
                    );
                }
            }
        }
        // Full-length windows at spot samples (every sample would also
        // pass; spots keep the suite fast).
        for n in [4_800usize, 48_000, 96_000] {
            for (fi, fo) in [(0, 0), (1, 1), (12_000, 24_000), (n, n), (n + 7, n + 7)] {
                for i in [0, 1, 2, n / 3, n / 2, n - 2, n - 1] {
                    if i < n {
                        assert_eq!(
                            fade_factor(i, n, fi, fo),
                            legacy_fade_factor(i, n, fi, fo),
                            "i={i} n={n} fi={fi} fo={fo}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn explicit_envelopes_null_against_fade_sugar() {
        // T002: a `key` spelling of each fade quantizes to the same
        // segments the sugar evaluates, sample for sample.
        let n = 4_800usize;
        let m = 1_200usize;
        // A 0.1 s clip with `fade-in=0.1`: the explicit key spells the
        // same segment the sugar evaluates.
        let fade_in = quantize(
            &[SoundSeg {
                start: 0.0,
                end: 0.1,
                from: 0.0,
                to: 1.0,
                ease: EaseDecl::Linear,
                line: 1,
                col: 1,
            }],
            0.0,
        );
        assert_eq!(fade_in.len(), 1);
        assert_eq!((fade_in[0].q0, fade_in[0].q1), (0, 4_800));
        for i in [0, 1, 2_399, 4_799] {
            let b = i as i128 + 1;
            assert_eq!(
                eval_chain(&fade_in, b, 0.0, 2.0),
                fade_factor(i, n, 4_800.min(n), 0),
                "fade-in sample {i}"
            );
        }
        // Fade-out anchors to the placed window end, like the sugar.
        let end = n as f64 / 48_000.0;
        let fade_out = quantize(
            &[SoundSeg {
                start: end - 0.025,
                end,
                from: 1.0,
                to: 0.0,
                ease: EaseDecl::Linear,
                line: 1,
                col: 1,
            }],
            0.0,
        );
        assert_eq!(fade_out.len(), 1);
        assert_eq!((fade_out[0].q0, fade_out[0].q1), (3_600, 4_800));
        for i in [3_599, 3_600, 4_199, 4_799] {
            let b = i as i128 + 1;
            assert_eq!(
                eval_chain(&fade_out, b, 0.0, 2.0),
                fade_factor(i, n, 0, m.min(n)),
                "fade-out sample {i}"
            );
        }
    }

    fn xseg(q0: i128, q1: i128, shape: notation::model::Crossfade, outgoing: bool) -> XSeg {
        XSeg {
            q0,
            q1,
            shape,
            outgoing,
        }
    }

    #[test]
    fn xfade_gains_hit_hand_computed_samples() {
        use notation::model::Crossfade;
        // Linear pairs run the shared linear primitive: hand values at
        // exact binary points, exact endpoints by construction.
        let lin_out = xseg(0, 4, Crossfade::Linear, true);
        assert_eq!(xfade_gain(&lin_out, 1), 0.75);
        assert_eq!(xfade_gain(&lin_out, 4), 0.0);
        let lin_in = xseg(0, 4, Crossfade::Linear, false);
        assert_eq!(xfade_gain(&lin_in, 1), 0.25);
        assert_eq!(xfade_gain(&lin_in, 4), 1.0);
        // Constant-power pairs hold exact endpoints and meet at
        // equal-power in the middle (each ~0.7071, power ~1.0).
        let cp_out = xseg(0, 4_800, Crossfade::ConstantPower, true);
        let cp_in = xseg(0, 4_800, Crossfade::ConstantPower, false);
        assert_eq!(xfade_gain(&cp_out, 4_800), 0.0);
        assert_eq!(xfade_gain(&cp_in, 4_800), 1.0);
        let (mid_out, mid_in) = (xfade_gain(&cp_out, 2_400), xfade_gain(&cp_in, 2_400));
        assert_eq!(mid_out, mid_in);
        assert!(
            (mid_out - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-7,
            "midpoint equal-power: {mid_out}"
        );
        assert!(
            ((mid_out * mid_out + mid_in * mid_in) - 1.0).abs() < 1e-6,
            "midpoint power sums to one"
        );
        // A hard cut is exact digital silence on the outgoing side.
        let cut = xseg(0, 100, Crossfade::None, true);
        for b in [1, 50, 100] {
            assert_eq!(xfade_gain(&cut, b), 0.0);
        }
    }

    /// One composition holding exactly the given audio clips, for
    /// crossfade/duck plan tests (no media, no directives).
    fn plan_comp(clips: Vec<notation::model::Clip>) -> Composition {
        Composition {
            name: "t".to_string(),
            id: None,
            canvas: None,
            width: 64,
            height: 64,
            duration: 4.0,
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

    fn plan_audio(
        start: f64,
        duration: f64,
        id: Option<&str>,
        crossfade: notation::model::Crossfade,
        duck: Option<notation::model::DuckUnder>,
    ) -> notation::model::Clip {
        notation::model::Clip::Audio(notation::model::AudioClip {
            path: "s.wav".to_string(),
            start,
            duration,
            id: id.map(str::to_string),
            track: notation::model::TrackKind::Audio,
            label: None,
            start_after: None,
            trim_start: 0.0,
            trim_end: None,
            rate: 1.0,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            crossfade,
            duck_under: duck,
            fx: Default::default(),
            waveform: Default::default(),
            hidden: false,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
            line: 1,
            column: 1,
        })
    }

    #[test]
    fn plan_xfade_quantizes_handoffs_per_clip() {
        use notation::model::Crossfade;
        // A [0,2) yields to B [1,3): the outgoing tail quantizes
        // against A's start, the incoming head against B's.
        let comp = plan_comp(vec![
            plan_audio(0.0, 2.0, None, Crossfade::ConstantPower, None),
            plan_audio(1.0, 2.0, None, Crossfade::ConstantPower, None),
        ]);
        let plan = plan_xfade(&comp);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].len(), 1);
        assert_eq!((plan[0][0].q0, plan[0][0].q1), (48_000, 96_000));
        assert!(plan[0][0].outgoing);
        assert_eq!(plan[0][0].shape, Crossfade::ConstantPower);
        assert_eq!(plan[1].len(), 1);
        assert_eq!((plan[1][0].q0, plan[1][0].q1), (0, 48_000));
        assert!(!plan[1][0].outgoing);
        // A `none` pair cuts the outgoing clip and carries nothing
        // for the incoming one (the newcomer plays full).
        let comp = plan_comp(vec![
            plan_audio(0.0, 2.0, None, Crossfade::None, None),
            plan_audio(1.0, 2.0, None, Crossfade::Linear, None),
        ]);
        let plan = plan_xfade(&comp);
        assert_eq!(plan[0].len(), 1);
        assert_eq!(plan[0][0].shape, Crossfade::None);
        assert!(plan[0][0].outgoing);
        assert_eq!((plan[0][0].q0, plan[0][0].q1), (48_000, 96_000));
        assert!(plan[1].is_empty(), "a cut pair plays the newcomer full");
        // A stacked pair (shared start) plans nothing and sums.
        let comp = plan_comp(vec![
            plan_audio(0.0, 2.0, None, Crossfade::ConstantPower, None),
            plan_audio(0.0, 3.0, None, Crossfade::ConstantPower, None),
        ]);
        let plan = plan_xfade(&comp);
        assert!(plan[0].is_empty() && plan[1].is_empty());
    }

    #[test]
    fn plan_duck_gates_voice_samples() {
        use notation::model::{Crossfade, DuckUnder};
        let comp = plan_comp(vec![
            plan_audio(
                0.0,
                4.0,
                Some("bed"),
                Crossfade::ConstantPower,
                Some(DuckUnder {
                    target: "vox".to_string(),
                    amount: 0.25,
                    line: 1,
                    column: 1,
                }),
            ),
            plan_audio(1.0, 2.0, Some("vox"), Crossfade::ConstantPower, None),
        ]);
        let gate = plan_duck(&comp, 0).expect("bed ducks under vox");
        assert_eq!((gate.v0, gate.v1), (48_000, 144_000));
        assert_eq!(gate.amount, 0.25f32);
        assert!(plan_duck(&comp, 1).is_none());
    }

    #[test]
    fn delay_taps_land_on_exact_multiples_with_decaying_gains() {
        // T005: an impulse through delay 0.1 s / feedback 0.5 / mix 0.8
        // reads back dry at unity, then taps of 0.8, 0.4, 0.2, ...
        // at exact multiples of 4800 samples, with silence between.
        let d = 4_800usize;
        let dry = 4 * d;
        let mut signal = vec![0.0f32; dry + 3 * d];
        signal[0] = 1.0;
        apply_delay(&mut signal, dry, 0.1, 0.5, 0.8);
        assert_eq!(signal[0], 1.0);
        assert_eq!(signal[d], 0.8);
        assert_eq!(signal[2 * d], 0.4);
        assert_eq!(signal[3 * d], 0.2);
        assert_eq!(signal[d + 1], 0.0);
        assert_eq!(signal[2 * d - 1], 0.0);
        // The dry body passes through untouched.
        signal[1] = 0.5;
        let mut body = vec![0.0f32; dry + d];
        body[0] = 1.0;
        body[1] = 0.5;
        apply_delay(&mut body, dry, 0.1, 0.5, 0.8);
        assert_eq!(body[1], 0.5);
        assert_eq!(body[d + 1], 0.4);
        // Zero spacing or zero mix bypasses bit-identically.
        let mut untouched = vec![0.25f32; 100];
        let snapshot = untouched.clone();
        apply_delay(&mut untouched, 100, 0.0, 0.5, 0.8);
        assert_eq!(untouched, snapshot);
        apply_delay(&mut untouched, 100, 0.1, 0.5, 0.0);
        assert_eq!(untouched, snapshot);
    }

    #[test]
    fn reverb_blooms_a_deterministic_room_tail() {
        // T005: an impulse through reverb 0.8 reads back dry at unity,
        // then room taps of 0.8, 0.32, ... at exact multiples of the
        // 30 ms comb, and the same input runs twice bit-identically.
        let d = notation::model::REVERB_COMB_SAMPLES;
        let dry = 4 * d;
        let mut signal = vec![0.0f32; dry + 3 * d];
        signal[0] = 1.0;
        apply_reverb(&mut signal, dry, 0.8);
        assert_eq!(signal[0], 1.0);
        assert_eq!(signal[d], 0.8);
        assert!((signal[2 * d] - 0.32).abs() < 1e-7, "{}", signal[2 * d]);
        assert_eq!(signal[d + 1], 0.0);
        let rerun = {
            let mut again = vec![0.0f32; dry + 3 * d];
            again[0] = 1.0;
            apply_reverb(&mut again, dry, 0.8);
            again
        };
        assert_eq!(signal, rerun, "room runs deterministically");
        // Zero reverb bypasses bit-identically.
        let mut untouched = vec![0.25f32; 100];
        let snapshot = untouched.clone();
        apply_reverb(&mut untouched, 100, 0.0);
        assert_eq!(untouched, snapshot);
    }
}
