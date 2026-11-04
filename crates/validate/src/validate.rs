//! Folder + composition validation (T003, T004, T005).
//!
//! Every problem is reported as a [`Diagnostic`] carrying file, line,
//! column and a stable code. The verdict mapping is:
//!
//! * no diagnostics → `Valid` (check exit 0)
//! * any `E_*` invalid diagnostic → `Invalid` (check exit 1)
//! * only `E_UNSUPPORTED` diagnostics → `Unsupported` (check exit 2,
//!   render refuses before the first frame and writes no MP4)

use notation::model::{
    parse_color, resolve_waveform_range, AudioClip, BlendMode, BoxClip, BoxShadow, Clip,
    Composition, Crossfade, DuckUnder, EaseDecl, FillPaint, FillRule, Fit, FlexAlign, FlexChild,
    FlexClip, FlexDirection, FlexJustify, FontDecl, Fps, GradientStop, ImageClip, InputDecl,
    KeyDecl, KeyStop, MarkDecl, MaskRef, NestClip, Normalize, Overflow, PathClip, Placement,
    ReplicateGroup, Rgb, SetDecl, SoundFx, SplitMode, StaggerGrid, StaggerOrigin, StartAfter,
    StrokePosition, SvgClip, TextClip, TextDirection, TextSpan, TrackKind, Turn3D, TweenDecl,
    TweenKind, VariableDecl, VariableDefault, VariableType, VideoClip, WaveformDraw,
    WaveformRangeError, CONFIG_FILE, MIN_DURATION_SECS, REPEAT_MAX, REPEAT_MIN,
};
use notation::parse::{parse_document, RawNode, RawValue, ValueKind};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One located, coded problem (or, for unsupported use, one
/// valid-but-not-yet-usable marking).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub code: &'static str,
    pub message: String,
    pub fix: String,
}

impl Diagnostic {
    fn new(
        file: &str,
        line: usize,
        column: usize,
        code: &'static str,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            code,
            message: message.into(),
            fix: fix.into(),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "error[{}] {}:{}:{}: {} (fix: {})",
            self.code, self.file, self.line, self.column, self.message, self.fix
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Valid,
    Invalid,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub status: Status,
    /// The selected composition: the root by default (Sprint 6 T006).
    /// `None` when the project is invalid.
    pub composition: Option<Composition>,
    /// Every validated composition in document order (Sprint 6 T006).
    /// Empty when the project is invalid; otherwise `composition` is
    /// the `root_name` entry of this list.
    pub compositions: Vec<Composition>,
    /// Name of the render root (the lone composition, or the one
    /// marked `root=true`). `None` when the project is invalid.
    pub root_name: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    /// One entry per `--variables`/host-map key naming no declared
    /// variable, plus one per declared-but-never-bound variable
    /// (Sprint 9 variables, CAP-003/CAP-004): warned visibly in
    /// default mode without blocking the render. Never blocks on its
    /// own; under `--strict-variables` both report as `E_BAD_VARIABLE`
    /// diagnostics instead and this list stays empty.
    pub variable_warnings: Vec<VariableWarning>,
    /// One entry per staggered `tween`/`key` whose pieces run past
    /// the target clip window (Sprint 7 motion, CAP-002): the cut is
    /// reviewable here with the numbers, never a diagnostic, so
    /// clamping stays a valid verdict.
    pub stagger_clamps: Vec<StaggerClamp>,
    /// Optional `framesmith.toml` setting, default true. Later sprints
    /// (T009/T010) own download behavior; the foundation records it so
    /// refusal wording can point at the right fix.
    pub ffmpeg_auto_download: bool,
    /// One entry per `video` clip whose trim/rate declaration validated
    /// (Sprint 5 T001): the effective source window and effective
    /// rendered length for `check` output review. Clamp reports live
    /// here, never as diagnostics, so clamping stays a valid verdict.
    pub footage: Vec<VideoFootage>,
    /// One entry per resolved length inferred from an omitted `duration`
    /// (Sprint 6 T003): audio clips (source window at rate, like video),
    /// stills (text, box, image run until the composition end), and the
    /// composition itself (until the latest finite clip end). `check`
    /// shows every entry for review before rendering; video inference
    /// keeps its own [`VideoFootage`] echo and never duplicates here.
    pub inferred: Vec<InferredLength>,
    /// One entry per same-track overlap handoff whose window clamped
    /// to the audible samples (Sprint 10 sound, CAP-003). `check`
    /// shows every entry for review; rendering blends the clamped
    /// window.
    pub crossfade_clamps: Vec<CrossfadeClamp>,
    /// One entry per delay/reverb tail truncated at the composition
    /// end (Sprint 10 sound, CAP-005). `check` shows every entry for
    /// review; rendering plays the tail to the end and cuts the rest.
    pub effect_tails: Vec<EffectTail>,
    /// One entry per `detect-silence` clip (Sprint 10 sound, CAP-006):
    /// the silent ranges `check` lists. Detection never alters audio,
    /// so these entries never block a valid render.
    pub silences: Vec<SilenceReport>,
    /// One entry per `waveform` clip whose drawn range clamped to the
    /// clip window (Sprint 10 sound, CAP-010). `check` shows every
    /// entry for review; rendering draws the clamped range.
    pub waveform_clamps: Vec<WaveformClamp>,
}

/// One reviewable inferred length (Sprint 6 T003): the resolved value
/// of an omitted `duration`, with the facts it came from. `kind` is
/// `audio`, `text`, `box`, `image`, or `composition`; `label` carries
/// the media path (audio, image) or the composition name, and is none
/// for text and box clips; `detail` states the rule that resolved it.
#[derive(Debug, Clone)]
pub struct InferredLength {
    pub kind: &'static str,
    pub label: Option<String>,
    pub line: usize,
    pub column: usize,
    pub duration: f64,
    pub detail: String,
}

/// One reviewable effect-tail truncation (Sprint 10 sound, CAP-005):
/// a delay/reverb tail that runs past the composition end, so the mix
/// renders it to the end and cuts the rest. The cut is reported here
/// with the numbers, never as a diagnostic, so truncation stays a
/// valid verdict.
#[derive(Debug, Clone)]
pub struct EffectTail {
    /// Document index of the effected clip.
    pub clip: usize,
    /// The clip's `id`, when declared.
    pub id: Option<String>,
    /// Natural delay tail in composition seconds.
    pub delay_tail: f64,
    /// Natural reverb tail in composition seconds.
    pub reverb_tail: f64,
    /// Natural effect end in composition seconds (clip end plus both
    /// tails, before clamping).
    pub raw_end: f64,
    /// Rendered effect end in composition seconds (the composition
    /// end, where the tail cuts).
    pub eff_end: f64,
    /// Clip location the entry sorts by (the clip's own node).
    pub line: usize,
    pub column: usize,
}

/// One reported pause map (Sprint 10 sound, CAP-006): the silent ranges
/// of one `detect-silence` clip, measured from the decoded source at
/// or below `threshold_db` dBFS for at least `min_seconds` each.
/// Detection never alters audio; an empty `ranges` means the clip has
/// no qualifying pause.
#[derive(Debug, Clone)]
pub struct SilenceReport {
    /// Document index of the detected clip.
    pub clip: usize,
    /// The clip's `id`, when declared.
    pub id: Option<String>,
    /// Element name (`audio` or `video`), for the human line.
    pub kind: &'static str,
    /// Ceiling the scan ran with, in dBFS.
    pub threshold_db: f64,
    /// Shortest reported pause, in composition seconds.
    pub min_seconds: f64,
    /// Silent ranges in composition seconds, in time order.
    pub ranges: Vec<(f64, f64)>,
    /// Clip location the entry sorts by (the clip's own node).
    pub line: usize,
    pub column: usize,
}

/// One reviewable waveform-range clamp (Sprint 10 sound, CAP-010):
/// a `waveform` clip whose drawn range runs past its clip window, so
/// the drawing clamps to the audible samples. The cut is reported
/// here with the numbers, never as a diagnostic, so clamping stays a
/// valid verdict.
#[derive(Debug, Clone)]
pub struct WaveformClamp {
    /// Document index of the drawing clip.
    pub clip: usize,
    /// The clip's `id`, when declared.
    pub id: Option<String>,
    /// Element name (`audio` or `video`), for the human line.
    pub kind: &'static str,
    /// Written range start/end in composition seconds.
    pub raw_from: f64,
    pub raw_to: f64,
    /// Clamped (drawn) range start/end in composition seconds.
    pub eff_from: f64,
    pub eff_to: f64,
    /// Clip location the entry sorts by (the clip's own node).
    pub line: usize,
    pub column: usize,
}

/// One reviewable crossfade clamp (Sprint 10 sound, CAP-003): a
/// same-track overlap handoff whose raw overlap runs past the
/// composition end, so the blend clamps to the audible samples. The
/// cut is reported here with the numbers, never as a diagnostic, so
/// clamping stays a valid verdict.
#[derive(Debug, Clone)]
pub struct CrossfadeClamp {
    /// Document index of the earlier (outgoing) clip.
    pub early: usize,
    /// Document index of the later (incoming) clip.
    pub late: usize,
    /// Resolved pair shape (`constant-power`, `linear`, or `none`).
    pub shape: &'static str,
    /// Raw overlap start/end in composition seconds.
    pub raw_start: f64,
    pub raw_end: f64,
    /// Clamped (audible) overlap start/end in composition seconds.
    pub eff_start: f64,
    pub eff_end: f64,
    /// Clip location both entries sort by (the later clip's node).
    pub line: usize,
    pub column: usize,
}

/// One reviewable stagger clamp (Sprint 7 motion, CAP-002): a `tween`
/// or `key` whose staggered pieces run past the target clip window.
/// Split pieces inherit the clip window, so a delayed piece never
/// extends it silently: the motion past the window end is cut, and
/// the cut is reported here with the numbers, never as a diagnostic,
/// so clamping stays a valid verdict.
#[derive(Debug, Clone)]
pub struct StaggerClamp {
    /// `tween` or `key`, for matching the entry to its directive.
    pub kind: &'static str,
    /// Document index of the target clip.
    pub target: usize,
    pub line: usize,
    pub column: usize,
    /// Animated pieces of the target (split pieces of a split text
    /// clip, else one).
    pub pieces: usize,
    /// Declared per-piece delay in seconds.
    pub stagger: f64,
    /// Declared origin (`start`, `center`, or `end`).
    pub origin: &'static str,
    /// Declared grid columns, when a `stagger-grid` is written.
    pub grid_cols: Option<usize>,
    /// Declared grid rows (`colsxrows` form), if any.
    pub grid_rows: Option<usize>,
    /// Largest piece delay in seconds.
    pub max_delay: f64,
    /// Latest animated instant in seconds (repeat passes and the
    /// largest piece delay included).
    pub motion_end: f64,
    /// Target clip window end in seconds.
    pub clip_end: f64,
    /// `motion_end` past `clip_end` in seconds.
    pub overflow: f64,
}

/// The reviewable footage window of one valid `video` clip (Sprint 5
/// T001/T009): declared trims plus rate resolved against the probed
/// source end into the effective window the renderer consumes, the
/// effective length it renders (`window ÷ rate`), and the resolved
/// clip length (written, or inferred when `duration` is omitted).
#[derive(Debug, Clone)]
pub struct VideoFootage {
    /// Declared `path`, for matching the entry to its clip.
    pub path: String,
    pub line: usize,
    pub column: usize,
    /// Declared `trim-start` (default 0).
    pub trim_start: f64,
    /// Declared `trim-end` (none means the source end).
    pub trim_end: Option<f64>,
    /// Declared `rate` (default 1).
    pub rate: f64,
    /// Declared `loop` (default false): the window repeats for the
    /// whole clip window instead of holding the last frame.
    pub loop_clip: bool,
    /// Probed source length in seconds; none when the probe could not
    /// report one (no FFmpeg on PATH, an unreadable file, or a still
    /// source such as HEIC with no timeline).
    pub source_duration: Option<f64>,
    /// Effective window actually consumed, in source seconds. The end
    /// is none only when both the declared end and the probe are
    /// missing; the window is then unreviewable until render probes.
    pub effective_start: f64,
    pub effective_end: Option<f64>,
    /// `(effective_end − effective_start) ÷ rate`, when the end is known.
    pub effective_duration: Option<f64>,
    /// Resolved clip length in composition seconds: the written
    /// `duration`, or the inferred `(trim_end − trim_start) ÷ rate`
    /// (Sprint 5 T009).
    pub duration: f64,
    /// True when `duration` was inferred (no `duration` key written),
    /// so `check` reviewers can tell it apart from an explicit length.
    pub inferred: bool,
    /// True when a past-end `trim-end` was clamped to the source end.
    pub clamped: bool,
}

pub const COMPOSITION_FILE: &str = "composition.kdl";

/// Later-sprint elements: well-formed but never rendered yet.
/// Each maps to the sprint that enables it. Boxes and images landed in
/// Sprint 2 (T002); tweens and sets validate since Sprint 2 (T003);
/// `audio` validates since Sprint 3 (T007), so only later sound-shaping
/// elements (Sprint 10), hardware encode (Sprint 25), and similar still
/// wait here — later motion properties are marked at the
/// key level inside `motion_property`.
pub fn unsupported_sprint(element: &str) -> Option<u32> {
    match element {
        // Sprint 10 sound proof: effects chains, automation, groups,
        // ducking, and the analysis/normalize verbs.
        "effect" | "effects" | "automation" | "group" | "groups" | "ducking" | "mixer"
        | "equalizer" | "normalize-audio" | "loudness" => Some(10),
        // Sprint 25: hardware encoding.
        "hardware" | "videotoolbox" | "nvenc" | "vaapi" | "amf" | "qsv" => Some(25),
        _ => None,
    }
}

/// Audio keys that wait for Sprint 10 sound proof (effects chains,
/// lane automation, groups, ducking, panning, per-sample curves, and
/// speed/pitch behavior). Well-formed but refused with the enabling
/// sprint, never mixed partially.
const LATER_AUDIO_KEYS: &[&str] = &[
    "automation",
    "compressor",
    "curve",
    "ducking",
    "echo",
    "effect",
    "effects",
    "eq",
    "equalizer",
    "fade-shape",
    "fade_shape",
    "group",
    "groups",
    "limiter",
    "loudness",
    "normalize",
    "pan",
    "panning",
    "pitch",
    "speed",
    "tempo",
];

/// Audio source extensions Sprint 3 mixes (decoded through the
/// pinned-or-host FFmpeg). Anything else is refused as
/// not-yet-supported, never guessed into the mix.
fn audio_format_supported(path: &str) -> bool {
    const KNOWN: &[&str] = &[
        "wav", "mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "aiff", "aif", "wma", "mka",
        "ac3", "mp2", "amr",
    ];
    let lower = path.to_lowercase();
    KNOWN.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// Parse a Sprint 3 `volume` value in dB (shared by `audio` tracks and
/// video sound keys): a finite number at most +12 dB, or `-inf` for
/// exact digital silence. Anything else is refused, never guessed.
fn parse_volume_db(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    match find(node, "volume") {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(db) if db == f64::NEG_INFINITY => Some(db),
            Some(db) if db.is_finite() && db <= 12.0 => Some(db),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!("{what} `volume` must be silence (`-inf`) or at most +12 dB"),
                    "set `volume=<dB at most +12>` (for example `volume=0`, `volume=6`, or `volume=\"-inf\"` for silence)",
                );
                None
            }
        },
    }
}

/// Parse one linear fade duration (Sprint 3 `fade-in`/`fade-out`, either
/// spelling): absent means 0, present must be a finite non-negative
/// number within the clip window. A fade longer than its clip is
/// refused, never clamped silently.
fn parse_fade(
    ctx: &mut Ctx,
    node: &RawNode,
    hyphen: &str,
    underscore: &str,
    duration: f64,
    what: &str,
) -> Option<f64> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 && x <= duration => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `{hyphen}` must run 0 through the clip duration ({duration}s); a fade longer than its clip is refused"
                    ),
                    format!("set `{hyphen}=<seconds from 0 to {duration}>` or remove the key"),
                );
                None
            }
        },
    }
}

/// Parse one `crossfade` shape (Sprint 10 sound, CAP-003): absent
/// means the default constant-power blend; present must name exactly
/// one of `constant-power`, `linear`, or `none`. Anything else is
/// refused naming the accepted set, never guessed into a blend.
fn parse_crossfade(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<Crossfade> {
    match find(node, "crossfade") {
        None => Some(Crossfade::default()),
        Some(v) => match as_str(v).and_then(Crossfade::parse) {
            Some(shape) => Some(shape),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `crossfade` {} is unknown; accepted crossfades are: {}",
                        raw_text(v),
                        Crossfade::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "set `crossfade=\"constant-power\"`, `\"linear\"`, or `\"none\"`, or remove the key for the default blend",
                );
                None
            }
        },
    }
}

/// Parse one `duck-under` plus `duck-amount` pair (Sprint 10 sound,
/// CAP-004): absent means full gain throughout; present needs both
/// keys together — a quoted voice clip id in the same composition
/// plus a gain from 0 to 1. A missing partner, a non-string voice, or
/// an out-of-range amount is refused naming the bound, never guessed.
/// The voice existence check runs project-wide with the other id
/// references, so one bad value still yields exactly one diagnostic.
fn parse_duck_under(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<Option<DuckUnder>> {
    let under = find_either(node, "duck-under", "duck_under");
    let amount = find_either(node, "duck-amount", "duck_amount");
    match (under, amount) {
        (None, None) => Some(None),
        (Some(u), None) => {
            ctx.push(
                u.line,
                u.column,
                "E_BAD_AUDIO",
                format!("{what} declares `duck-under` without `duck-amount`; a duck needs a voice id plus a gain"),
                "add `duck-amount=<0 to 1>` naming the gain while the voice sounds, or remove `duck-under`",
            );
            None
        }
        (None, Some(a)) => {
            ctx.push(
                a.line,
                a.column,
                "E_BAD_AUDIO",
                format!("{what} declares `duck-amount` without `duck-under`; a gain needs a voice to duck under"),
                "add `duck-under=\"<voice clip id>\"` naming the voice clip, or remove `duck-amount`",
            );
            None
        }
        (Some(u), Some(a)) => {
            let target = match as_str(u) {
                Some(s) => s.to_string(),
                None => {
                    ctx.push(
                        u.line,
                        u.column,
                        "E_UNKNOWN_REF",
                        format!(
                            "{what} `duck-under` must be a quoted id naming a clip in the same composition; found {}",
                            raw_text(u)
                        ),
                        "set `duck-under=\"<voice clip id>\"` naming the voice clip's `id`",
                    );
                    return None;
                }
            };
            let gain = match as_f64(a) {
                Some(x) if x.is_finite() && (0.0..=1.0).contains(&x) => x,
                _ => {
                    ctx.push(
                        a.line,
                        a.column,
                        "E_BAD_AUDIO",
                        format!(
                            "{what} `duck-amount` {} is outside the accepted range 0 to 1",
                            raw_text(a)
                        ),
                        "set `duck-amount=<0 to 1>` (for example `duck-amount=0.25` for a quarter gain under voice), or remove the ducking keys",
                    );
                    return None;
                }
            };
            ctx.refs.push(RefEntry {
                kind: "duck-under",
                target: target.clone(),
                line: u.line,
                column: u.column,
                owner_id: None,
                owner_line: node.name_line,
                needs_composition: false,
                comp: ctx.current_comp,
            });
            Some(Some(DuckUnder {
                target,
                amount: gain,
                line: u.line,
                column: u.column,
            }))
        }
    }
}

/// Parse one ranged effect number (Sprint 10 sound, CAP-005): absent
/// means the dry default; present must be finite within `lo..=hi`. An
/// unparsable or out-of-range value is refused naming the bound, never
/// clamped silently into the mix.
fn parse_fx_range(
    ctx: &mut Ctx,
    node: &RawNode,
    what: &str,
    hyphen: &str,
    underscore: &str,
    lo: f64,
    hi: f64,
    unit: &str,
) -> Option<f64> {
    let found = find_either(node, hyphen, underscore);
    match found {
        None => Some(match hyphen {
            "delay-feedback" => SoundFx::default().delay_feedback,
            "delay-mix" => SoundFx::default().delay_mix,
            "reverb" => SoundFx::default().reverb,
            _ => 0.0,
        }),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= lo && x <= hi => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `{hyphen}` {} is outside the accepted range {lo} to {hi}{unit}",
                        raw_text(v)
                    ),
                    format!(
                        "set `{hyphen}=<{lo} to {hi}{unit}>` or remove the key for the dry default"
                    ),
                );
                None
            }
        },
    }
}

/// Parse one `detect-silence` switch (Sprint 10 sound, CAP-006):
/// absent means no detection; present must be `true` or `false` (a
/// quoted `"true"`/`"false"` reads the same). Anything else is refused
/// naming the accepted form, never guessed off.
fn parse_detect_silence(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<bool> {
    match find_either(node, "detect-silence", "detect_silence") {
        None => Some(false),
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            ValueKind::Str(s)
                if s.trim().eq_ignore_ascii_case("true")
                    || s.trim().eq_ignore_ascii_case("false") =>
            {
                Some(s.trim().eq_ignore_ascii_case("true"))
            }
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `detect-silence` {} is not a silence switch; accepted values are `true` and `false`",
                        raw_text(v)
                    ),
                    format!(
                        "set `detect-silence=true` to list pauses for the {what}, or remove the key"
                    ),
                );
                None
            }
        },
    }
}

/// Parse one silence threshold in dBFS (Sprint 10 sound, CAP-006):
/// absent means −40 dBFS; present must be a plain dB number (at or
/// below counts as silent). An unparsable value is refused naming the
/// accepted form, never guessed into a ceiling.
fn parse_silence_threshold(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    match find_either(node, "silence-threshold", "silence_threshold") {
        None => Some(notation::model::SILENCE_THRESHOLD_DEFAULT_DB),
        // Finite dBFS numbers stand, and so does `-inf` (only exact
        // digital zero counts); NaN, +inf, and unparsable text name the
        // accepted form instead of guessing a ceiling.
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() || x == f64::NEG_INFINITY => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `silence-threshold` {} is not a plain dB number; accepted values are numbers in dBFS (for example `silence-threshold=-40`)",
                        raw_text(v)
                    ),
                    "set `silence-threshold=<dBFS number>` (for example `silence-threshold=-40`), or remove the key for the -40 dBFS default",
                );
                None
            }
        },
    }
}

/// Parse one `silence-min` in seconds (Sprint 10 sound, CAP-006):
/// absent means 0.5 s; present must be finite seconds from 0. Anything
/// else is refused naming the bound, never guessed into a duration.
fn parse_silence_min(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    match find_either(node, "silence-min", "silence_min") {
        None => Some(notation::model::SILENCE_MIN_DEFAULT_SECS),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `silence-min` {} is outside the accepted range 0 seconds and up",
                        raw_text(v)
                    ),
                    "set `silence-min=<seconds from 0>` (for example `silence-min=0.5`), or remove the key for the 0.5 s default",
                );
                None
            }
        },
    }
}

/// Parse the echo/room/detection block of one audio-bearing clip
/// (Sprint 10 sound, CAP-005/CAP-006): `delay` (0..2 s) with
/// `delay-feedback` (0..0.95) and `delay-mix` (0..1), `reverb` (0..1),
/// plus the opt-in `detect-silence` switch with `silence-threshold`
/// (plain dBFS) and `silence-min` (seconds from 0). Every bad value
/// fails with its bound; absent keys render dry and detect nothing.
fn parse_sound_fx(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<SoundFx> {
    let delay = parse_fx_range(ctx, node, what, "delay", "delay", 0.0, 2.0, " seconds")?;
    let delay_feedback = parse_fx_range(
        ctx,
        node,
        what,
        "delay-feedback",
        "delay_feedback",
        0.0,
        0.95,
        "",
    )?;
    let delay_mix = parse_fx_range(ctx, node, what, "delay-mix", "delay_mix", 0.0, 1.0, "")?;
    let reverb = parse_fx_range(ctx, node, what, "reverb", "reverb", 0.0, 1.0, "")?;
    let detect_silence = parse_detect_silence(ctx, node, what)?;
    let silence_threshold_db = parse_silence_threshold(ctx, node, what)?;
    let silence_min = parse_silence_min(ctx, node, what)?;
    Some(SoundFx {
        delay,
        delay_feedback,
        delay_mix,
        reverb,
        detect_silence,
        silence_threshold_db,
        silence_min,
    })
}

/// Parse one `waveform` drawing block (Sprint 10 sound, CAP-010):
/// absent means the clip draws no sound shape; present needs the
/// `waveform` switch (`true`/`false`, quoted or bare) plus optional
/// `waveform-from`/`waveform-to` bounds in composition seconds. A bad
/// switch, an unparsable bound, or bounds without the switch fail
/// naming the accepted form, never guessed into a drawing.
fn parse_waveform(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<WaveformDraw> {
    let switch = find(node, "waveform");
    let from = find_either(node, "waveform-from", "waveform_from");
    let to = find_either(node, "waveform-to", "waveform_to");
    let enabled = match switch {
        None => false,
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => *b,
            ValueKind::Str(s)
                if s.trim().eq_ignore_ascii_case("true")
                    || s.trim().eq_ignore_ascii_case("false") =>
            {
                s.trim().eq_ignore_ascii_case("true")
            }
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `waveform` {} is not a drawing switch; accepted values are `true` and `false`",
                        raw_text(v)
                    ),
                    "set `waveform=true` to draw the clip sound on the canvas, or remove the key"
                        .to_string(),
                );
                return None;
            }
        },
    };
    let mut bound = |key: &str, v: &RawValue| -> Option<f64> {
        match as_f64(v) {
            Some(x) if x.is_finite() => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `{key}` {} is not a time in composition seconds",
                        raw_text(v)
                    ),
                    format!("set `{key}=<seconds from 0>` bounding the drawn range, or remove the key for the clip window edge"),
                );
                None
            }
        }
    };
    let from_val = match from {
        None => None,
        Some(v) => Some(bound("waveform-from", v)?),
    };
    let to_val = match to {
        None => None,
        Some(v) => Some(bound("waveform-to", v)?),
    };
    if !enabled && (from.is_some() || to.is_some()) {
        let v = from.or(to).unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_AUDIO",
            format!("{what} draws no sound shape (`waveform` is off) but names a drawn range; a range needs the drawing"),
            "set `waveform=true` to draw within the range, or remove the `waveform-from`/`waveform-to` keys",
        );
        return None;
    }
    let (line, column) = match switch {
        Some(v) => (v.line, v.column),
        None => (node.name_line, node.name_column),
    };
    Some(WaveformDraw {
        enabled,
        from: from_val,
        to: to_val,
        line,
        column,
        from_loc: from.map(|v| (v.line, v.column)),
        to_loc: to.map(|v| (v.line, v.column)),
    })
}

fn as_f64(v: &RawValue) -> Option<f64> {
    match v.kind {
        ValueKind::Int(n) => Some(n as f64),
        ValueKind::Float(x) => Some(x),
        ValueKind::Str(ref s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn as_str(v: &RawValue) -> Option<&str> {
    match v.kind {
        ValueKind::Str(ref s) => Some(s.as_str()),
        _ => None,
    }
}

fn as_int(v: &RawValue) -> Option<i64> {
    match v.kind {
        ValueKind::Int(n) => Some(n),
        _ => None,
    }
}

/// One declared id, collected for project-wide uniqueness checking.
struct IdEntry {
    value: String,
    line: usize,
    column: usize,
    /// True for the composition name/id, false for a clip id or mark.
    is_composition: bool,
    /// True for a `mark` name (Sprint 7 motion, CAP-003): marks share
    /// the project id namespace so a `start-after` target resolves
    /// unambiguously, and a mark colliding with a clip or composition
    /// id fails as a duplicate.
    is_mark: bool,
    /// Index of the owning composition in document order (Sprint 6
    /// T006): clips register their composition's index so a
    /// `start-after` naming a clip in another composition can refuse
    /// instead of timing against a timeline it never renders with.
    comp: usize,
}

/// One id reference, collected for existence checking.
struct RefEntry {
    /// `start-after` or `nest`.
    kind: &'static str,
    target: String,
    line: usize,
    column: usize,
    /// The referring clip's own id, when it declares one.
    owner_id: Option<String>,
    owner_line: usize,
    /// True when the target must name a composition (`nest`); false
    /// when any id in the project namespace answers (`start-after`).
    needs_composition: bool,
    /// Index of the referring composition in document order (Sprint 6
    /// T006), so cross-composition `start-after` targets refuse.
    comp: usize,
}

struct Ctx<'a> {
    file: String,
    folder: &'a Path,
    diags: Vec<Diagnostic>,
    invalid: bool,
    footage: Vec<VideoFootage>,
    inferred: Vec<InferredLength>,
    stagger_clamps: Vec<StaggerClamp>,
    ids: Vec<IdEntry>,
    refs: Vec<RefEntry>,
    /// Every substituted `var()` use site (Sprint 9 variables, CAP-002):
    /// the attribute location plus the variable ids it drew on. The
    /// end-of-validation amendment pass uses these to name the variable
    /// behind any coercion failure at the same location.
    bindings: Vec<BindingSite>,
    crossfade_clamps: Vec<CrossfadeClamp>,
    effect_tails: Vec<EffectTail>,
    silences: Vec<SilenceReport>,
    waveform_clamps: Vec<WaveformClamp>,
    /// Index of the composition currently validating, in document
    /// order (Sprint 6 T006). Id and reference entries stamp this so
    /// cross-composition `start-after` targets can refuse.
    current_comp: usize,
    /// Per-render caller values for this verdict (Sprint 9 variables,
    /// CAP-003): every declared default they name is swapped for the
    /// coerced override in memory only. The project on disk is never
    /// written.
    overrides: VariableOverrides,
    /// Default-mode variable warnings collected for the verdict
    /// (Sprint 9 variables, CAP-003/CAP-004): unknown override keys and
    /// declared-but-never-bound variables. Warned visibly without
    /// blocking; empty under `--strict-variables` (both refuse there).
    override_warnings: Vec<VariableWarning>,
    /// Strict variable validation (Sprint 9 variables, CAP-004): when
    /// true, unknown override keys and declared-but-never-bound
    /// variables refuse as `E_BAD_VARIABLE` instead of warning, and
    /// every default-less variable without a supplied value refuses.
    strict: bool,
}

impl<'a> Ctx<'a> {
    fn push(
        &mut self,
        line: usize,
        column: usize,
        code: &'static str,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) {
        if code != "E_UNSUPPORTED" {
            self.invalid = true;
        }
        // One diagnostic per timing error (CAP-005): the inference loop
        // re-runs relative-start resolution as the composition end goes
        // from unknown to known, and a re-run must never double-report
        // the cycle or before-zero it already named. An identical
        // diagnostic (same location, code, and message) reports the
        // same error, so it is kept exactly once.
        let message = message.into();
        let duplicate = self.diags.iter().any(|d| {
            d.file == self.file
                && d.line == line
                && d.column == column
                && d.code == code
                && d.message == message
        });
        if duplicate {
            return;
        }
        self.diags.push(Diagnostic::new(
            &self.file.clone(),
            line,
            column,
            code,
            message,
            fix,
        ));
    }
}

fn find<'b>(node: &'b RawNode, key: &str) -> Option<&'b RawValue> {
    node.props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Find a hyphenated key under either spelling (`start-after` or
/// `start_after`): CSS-like hyphenated attributes accept underscore
/// aliases everywhere.
fn find_either<'b>(node: &'b RawNode, hyphen: &str, underscore: &str) -> Option<&'b RawValue> {
    find(node, hyphen).or_else(|| find(node, underscore))
}

/// Sprint 6 id syntax (T001): 1–64 characters of ASCII letters,
/// digits, dashes, and underscores. Anything else is refused, never
/// guessed into a referenceable name.
fn valid_id_syntax(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parse one clip `id` (T001): absent means anonymous; present must be
/// a quoted string with valid id syntax. Returns `None` after
/// reporting `E_BAD_ID` when the value is unusable, `Some(None)` when
/// absent, `Some(Some(id))` when declared. The caller registers a
/// declared id for project-wide uniqueness checking.
fn parse_clip_id(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<Option<String>> {
    match find(node, "id") {
        None => Some(None),
        Some(v) => match as_str(v) {
            Some(s) if valid_id_syntax(s) => Some(Some(s.to_string())),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ID",
                    format!(
                        "{what} `id` must be 1-64 letters, digits, `-` or `_`; found {}",
                        raw_text(v)
                    ),
                    format!("set `id=\"<letters-digits-dashes-underscores>\"` on the {what}"),
                );
                None
            }
        },
    }
}

/// Parse one clip `track` (T004): absent means the kind default (video
/// to video, audio to audio, everything else to graphics; `caption`
/// only when written). An unknown name fails with `E_BAD_TRACK`
/// naming the accepted set. One bad value yields exactly this one
/// diagnostic.
fn parse_track(ctx: &mut Ctx, node: &RawNode, element: &str, what: &str) -> Option<TrackKind> {
    match find(node, "track") {
        None => Some(TrackKind::default_for_clip(element)),
        Some(v) => match as_str(v).and_then(TrackKind::parse) {
            Some(t) => Some(t),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TRACK",
                    format!(
                        "{what} `track` {} is unknown; accepted tracks are: {}",
                        raw_text(v),
                        TrackKind::ACCEPTED.join(", ")
                    ),
                    "set `track=` to one of `video`, `audio`, `caption`, or `graphics`, or remove the key for the kind default",
                );
                None
            }
        },
    }
}

/// Parse one clip `label` (T004): absent means no note; present must be
/// a quoted string and rides along with no render effect. A non-string
/// label fails with `E_BAD_LABEL`, never guessed into text.
fn parse_label(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<Option<String>> {
    match find(node, "label") {
        None => Some(None),
        Some(v) => match as_str(v) {
            Some(s) => Some(Some(s.to_string())),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_LABEL",
                    format!(
                        "{what} `label` must be a quoted editor note; found {}",
                        raw_text(v)
                    ),
                    format!("set `label=\"...\"` on the {what}, or remove the key"),
                );
                None
            }
        },
    }
}

/// Parse one clip `hidden` switch (T009): absent means visible;
/// present must be `true` or `false`. A non-boolean value fails with
/// `E_BAD_HIDDEN`, never guessed. Hidden clips still validate fully:
/// every other key on the clip reports its own diagnostics, and the
/// id stays referenceable.
fn parse_hidden(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<bool> {
    match find(node, "hidden") {
        None => Some(false),
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_HIDDEN",
                    format!(
                        "{what} `hidden` must be `true` or `false`; found {}",
                        raw_text(v)
                    ),
                    format!(
                        "set `hidden=true` to hide the {what}, or `hidden=false` (or remove the key) to show it"
                    ),
                );
                None
            }
        },
    }
}

/// Parse one id reference (`start-after` or the `nest` target): absent
/// means none; present must be a quoted string. The existence check
/// runs project-wide after every clip validates, so one bad value
/// still yields exactly one diagnostic.
fn parse_ref(
    ctx: &mut Ctx,
    node: &RawNode,
    key: &str,
    what: &str,
) -> Option<Option<(String, usize, usize)>> {
    let found = if key == "start-after" {
        find_either(node, "start-after", "start_after")
    } else {
        find(node, key)
    };
    match found {
        None => Some(None),
        Some(v) => match as_str(v) {
            Some(s) => Some(Some((s.to_string(), v.line, v.column))),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_UNKNOWN_REF",
                    format!(
                        "{what} `{key}` must be a quoted id naming another clip or composition; found {}",
                        raw_text(v)
                    ),
                    format!("set `{key}=\"<id>\"` naming a declared `id` (or the composition name)"),
                );
                None
            }
        },
    }
}

/// Canvas preset table (T005): preset name to canvas pixels. Aliases
/// (`fullhd` for `1080p`, `uhd` for `4k`) resolve identically.
fn canvas_preset_dims(name: &str) -> Option<(u32, u32)> {
    match name {
        "1080p" | "fullhd" => Some((1920, 1080)),
        "720p" => Some((1280, 720)),
        "square" => Some((1080, 1080)),
        "portrait" => Some((1080, 1920)),
        "4k" | "uhd" => Some((3840, 2160)),
        _ => None,
    }
}

/// Canonical preset name for a valid `canvas` value: aliases resolve to
/// their preset (`fullhd` to `1080p`, `uhd` to `4k`).
fn canvas_canonical(name: &str) -> Option<&'static str> {
    match name {
        "1080p" | "fullhd" => Some("1080p"),
        "720p" => Some("720p"),
        "square" => Some("square"),
        "portrait" => Some("portrait"),
        "4k" | "uhd" => Some("4k"),
        _ => None,
    }
}

/// Accepted canvas spellings for diagnostics, aliases shown with their preset.
fn accepted_canvases() -> &'static str {
    "`1080p` (alias `fullhd`), `720p`, `square`, `portrait`, `4k` (alias `uhd`)"
}

/// Project-wide id and reference check (T001): every duplicate id
/// (one diagnostic naming every line), every unknown reference
/// (naming the referring line and the missing id), and every
/// self-reference. Runs after all clips validate so every problem in
/// the composition reports together.
fn check_ids_and_refs(ctx: &mut Ctx) {
    use std::collections::BTreeMap;
    let mut by_value: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in ctx.ids.iter().enumerate() {
        by_value.entry(e.value.clone()).or_default().push(i);
    }
    for (value, idxs) in &by_value {
        if idxs.len() < 2 {
            continue;
        }
        let lines: Vec<String> = idxs.iter().map(|&i| ctx.ids[i].line.to_string()).collect();
        let at = &ctx.ids[idxs[idxs.len() - 1]];
        ctx.push(
            at.line,
            at.column,
            "E_DUP_ID",
            format!(
                "duplicate id {value:?} declared at lines {}; ids must be unique across the whole project",
                lines.join(", ")
            ),
            format!("rename all but one `id={value:?}` to unique ids"),
        );
    }
    for r in std::mem::take(&mut ctx.refs) {
        // A clip that follows itself is refused like a bad reference:
        // its own id answers, but the relation is meaningless.
        if let Some(owner) = &r.owner_id {
            if owner == &r.target {
                ctx.push(
                    r.line,
                    r.column,
                    "E_BAD_ID",
                    format!(
                        "`{}` {:?} names the clip's own id (line {}); a clip cannot follow itself",
                        r.kind, r.target, r.owner_line,
                    ),
                    format!(
                        "point `{}=` at a different clip or composition id, or remove the key",
                        r.kind
                    ),
                );
                continue;
            }
        }
        let found = ctx.ids.iter().find(|e| e.value == r.target);
        match found {
            None => {
                ctx.push(
                    r.line,
                    r.column,
                    "E_UNKNOWN_REF",
                    format!(
                        "`{}` {:?} names no declared id, mark, or composition in this project",
                        r.kind, r.target
                    ),
                    format!(
                        "declare `id={:?}` on the intended clip or composition, declare a `mark` named {:?}, or fix the spelling",
                        r.target, r.target
                    ),
                );
            }
            Some(hit) if r.needs_composition && !hit.is_composition => {
                ctx.push(
                    r.line,
                    r.column,
                    "E_UNKNOWN_REF",
                    format!(
                        "`{}` {:?} names a clip id, but a nest target must be a composition",
                        r.kind, r.target
                    ),
                    "point `nest=` at the composition `name` (or its `id`), not a clip id",
                );
            }
            Some(_) => {}
        }
    }
}

/// Display name for a clip inside a `start-after` chain diagnostic:
/// its `id` when declared, else its document position.
fn chain_name(clips: &[Clip], index: usize) -> String {
    match clips[index].id() {
        Some(id) => format!("{id:?}"),
        None => format!("<clip {}>", index + 1),
    }
}

/// Resolve one clip's effective start through its `start-after` chain
/// (T002): an absolute clip resolves as written; a relative clip
/// resolves at its target's effective end plus its offset. Returns
/// `None` after recording exactly one diagnostic (cycle, before-zero)
/// or silently when the target already failed (unknown, self, dropped,
/// or upstream failure) so one bad value never cascades.
#[allow(clippy::too_many_arguments)]
fn resolve_one(
    ctx: &mut Ctx,
    clips: &[Clip],
    by_id: &std::collections::HashMap<&str, usize>,
    comp_name: &str,
    comp_id: Option<&str>,
    comp_end: Option<f64>,
    marks: &[MarkDecl],
    index: usize,
    resolved: &mut [Option<f64>],
    failed: &mut [bool],
    visiting: &mut [bool],
    stack: &mut Vec<usize>,
) -> Option<f64> {
    if let Some(s) = resolved[index] {
        return Some(s);
    }
    if failed[index] {
        return None;
    }
    let (target, offset, line, column) = match clips[index].start_after() {
        None => return Some(clips[index].start()),
        Some(sa) => (sa.target.clone(), sa.offset, sa.line, sa.column),
    };
    if visiting[index] {
        let pos = stack.iter().position(|&j| j == index).unwrap_or(0);
        let mut chain: Vec<String> = stack[pos..].iter().map(|&j| chain_name(clips, j)).collect();
        chain.push(chain_name(clips, index));
        ctx.push(
            line,
            column,
            "E_BAD_TIMING",
            format!(
                "`start-after` chain cycles: {}; a clip cannot follow itself through other clips",
                chain.join(" -> ")
            ),
            "break the cycle so every `start-after` chain ends at an absolute `start`",
        );
        for &j in &stack[pos..] {
            failed[j] = true;
        }
        failed[index] = true;
        return None;
    }
    // A self-reference was already refused (`E_BAD_ID`): never resolve.
    if clips[index].id() == Some(target.as_str()) {
        failed[index] = true;
        return None;
    }
    visiting[index] = true;
    stack.push(index);
    let target_end = if let Some(&j) = by_id.get(target.as_str()) {
        resolve_one(
            ctx, clips, by_id, comp_name, comp_id, comp_end, marks, j, resolved, failed, visiting,
            stack,
        )
        // A target whose own duration is still pending (a still with
        // an omitted `duration` waiting for the composition end)
        // has no end yet: the follower waits silently for a later
        // inference pass instead of resolving against `NAN`.
        .map(|s| s + clips[j].duration())
        .filter(|e| e.is_finite())
    } else if let Some(m) = marks.iter().find(|m| m.name == target) {
        // A `mark` target (Sprint 7 motion, CAP-003): the clip starts
        // exactly at the marked instant plus its offset. Marks need no
        // inference pass — the instant is written, never pending.
        Some(m.at)
    } else if Some(target.as_str()) == comp_id || target.as_str() == comp_name {
        comp_end
    } else {
        // Unknown, or a dropped target whose own error already reported:
        // stay silent so the one bad value yields one diagnostic.
        None
    };
    stack.pop();
    visiting[index] = false;
    match target_end {
        None => {
            failed[index] = true;
            None
        }
        Some(end) => {
            let effective = end + offset;
            if effective < 0.0 {
                ctx.push(
                    line,
                    column,
                    "E_BAD_TIMING",
                    format!(
                        "`start-after` {target:?} ends at {end}s with `start-offset` {offset}s, so this clip would start at {effective}s before zero"
                    ),
                    "lengthen the referenced window, shrink the negative offset, or give this clip an absolute `start=`",
                );
                failed[index] = true;
                None
            } else {
                resolved[index] = Some(effective);
                Some(effective)
            }
        }
    }
}

/// Resolve every `start-after` window project-wide (T002): each
/// relative clip starts exactly at its referenced effective end plus
/// its offset (negative overlaps are kept as authored); composition
/// names answer as the composition end. Absolute clips are untouched.
fn resolve_relative_starts(
    ctx: &mut Ctx,
    clips: &mut [Clip],
    comp_name: &str,
    comp_id: Option<&str>,
    comp_duration: Option<f64>,
    marks: &[MarkDecl],
) {
    use std::collections::HashMap;
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for (i, c) in clips.iter().enumerate() {
        if let Some(id) = c.id() {
            by_id.entry(id).or_insert(i);
        }
    }
    let comp_end = comp_duration.filter(|d| d.is_finite() && *d > 0.0);
    let mut resolved: Vec<Option<f64>> = vec![None; clips.len()];
    let mut failed: Vec<bool> = vec![false; clips.len()];
    let mut visiting: Vec<bool> = vec![false; clips.len()];
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..clips.len() {
        if clips[i].start_after().is_none() || resolved[i].is_some() || failed[i] {
            continue;
        }
        resolve_one(
            ctx,
            clips,
            &by_id,
            comp_name,
            comp_id,
            comp_end,
            marks,
            i,
            &mut resolved,
            &mut failed,
            &mut visiting,
            &mut stack,
        );
    }
    for (i, c) in clips.iter_mut().enumerate() {
        if let Some(s) = resolved[i] {
            c.set_start(s);
        }
    }
}

/// Validate an optional `framesmith.toml` config. Only `key = value`
/// lines are accepted; unknown keys are invalid (never ignored).
fn validate_config(folder: &Path, ctx_file: &str) -> (bool, Vec<Diagnostic>) {
    let path = folder.join(CONFIG_FILE);
    if !path.exists() {
        return (true, Vec::new());
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            return (
                false,
                vec![Diagnostic::new(
                    ctx_file,
                    1,
                    1,
                    "E_UNREADABLE",
                    format!("cannot read {CONFIG_FILE}: {e}"),
                    "make the config file readable or remove it",
                )],
            );
        }
    };
    let mut auto_download = true;
    let mut diags = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = match line.split_once('=') {
            Some(p) => (p.0.trim(), p.1.trim()),
            None => {
                diags.push(Diagnostic::new(
                    ctx_file,
                    line_no,
                    1,
                    "E_PARSE",
                    format!("malformed config line: {raw:?}"),
                    "use `key = value` lines",
                ));
                continue;
            }
        };
        match k {
            "ffmpeg_auto_download" => match v {
                "true" => auto_download = true,
                "false" => auto_download = false,
                _ => {
                    diags.push(Diagnostic::new(
                        ctx_file,
                        line_no,
                        1,
                        "E_BAD_CONFIG",
                        format!("{k} must be true or false, found {v:?}"),
                        "write `ffmpeg_auto_download = true` or `false`",
                    ));
                }
            },
            _ => {
                diags.push(Diagnostic::new(
                    ctx_file,
                    line_no,
                    1,
                    "E_UNKNOWN_KEY",
                    format!("unknown config key {k:?}"),
                    "remove the key or check the spelling",
                ));
            }
        }
    }
    (auto_download, diags)
}

/// Parse and validate one project folder. Never writes anything.
/// One per-render caller value (Sprint 9 variables, CAP-003): the
/// JSON scalar behind one `--variables` key or host-map entry.
/// Objects, arrays, and null are never variable values; the parser
/// below refuses them naming the flag and the expected shape.
#[derive(Debug, Clone, PartialEq)]
pub enum OverrideValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

/// Per-render caller values keyed by variable id (Sprint 9 variables,
/// CAP-003): the host entry passes this map directly, the CLI builds
/// it with [`parse_variable_overrides`]. Sorted so resolution and
/// unknown-key warnings stay deterministic across runs.
pub type VariableOverrides = BTreeMap<String, OverrideValue>;

/// One unknown override key (Sprint 9 variables, CAP-003): warned
/// visibly in default mode without blocking the render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableWarning {
    /// The override key naming no declared variable.
    pub key: String,
    pub message: String,
    pub fix: String,
}

impl std::fmt::Display for VariableWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "warning: {} (fix: {})", self.message, self.fix)
    }
}

/// Render one caller value for diagnostics (Sprint 9 variables,
/// CAP-003): the value the caller supplied, so refusals can name it.
fn override_text(value: &OverrideValue) -> String {
    match value {
        OverrideValue::Str(s) => format!("{s:?}"),
        OverrideValue::Num(x) => x.to_string(),
        OverrideValue::Bool(b) => b.to_string(),
    }
}

/// Shorten a `--variables` payload for diagnostics (Sprint 9 variables,
/// CAP-003): the head plus an ellipsis marker, so a huge payload never
/// floods the refusal.
fn short_payload(raw: &str) -> String {
    const MAX: usize = 160;
    if raw.len() <= MAX {
        format!("{raw:?}")
    } else {
        format!("{:?}...", &raw[..MAX])
    }
}

/// Parse one `--variables` payload (Sprint 9 variables, CAP-003): a
/// JSON object of string keys to scalar values (string, number, or
/// boolean), for example `--variables '{"title":"Sale"}'`. Every
/// failure names the flag and the expected shape: unparseable JSON, a
/// non-object payload, or an object, array, or null value.
pub fn parse_variable_overrides(raw: &str) -> Result<VariableOverrides, String> {
    const SHAPE: &str = "a JSON object of string keys to scalar values (for example `--variables '{\"title\":\"Sale\"}'`)";
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        format!(
            "--variables {} is not valid JSON ({e}); expected {SHAPE}",
            short_payload(raw)
        )
    })?;
    let obj = match value {
        serde_json::Value::Object(map) => map,
        _ => {
            return Err(format!(
                "--variables {} is not a JSON object; expected {SHAPE}",
                short_payload(raw)
            ));
        }
    };
    let mut out = VariableOverrides::new();
    for (key, val) in obj {
        let parsed = match val {
            serde_json::Value::String(s) => OverrideValue::Str(s),
            serde_json::Value::Number(n) => match n.as_f64() {
                Some(x) if x.is_finite() => OverrideValue::Num(x),
                _ => {
                    return Err(format!(
                        "--variables key {key:?} holds a non-finite number; expected a scalar value (a string, finite number, or boolean)"
                    ));
                }
            },
            serde_json::Value::Bool(b) => OverrideValue::Bool(b),
            serde_json::Value::Array(_) => {
                return Err(format!(
                    "--variables key {key:?} holds an array; expected a scalar value (a string, number, or boolean) — arrays are not variable values"
                ));
            }
            serde_json::Value::Object(_) => {
                return Err(format!(
                    "--variables key {key:?} holds an object; expected a scalar value (a string, number, or boolean) — objects are not variable values"
                ));
            }
            serde_json::Value::Null => {
                return Err(format!(
                    "--variables key {key:?} holds null; expected a scalar value (a string, number, or boolean) — null is not a variable value"
                ));
            }
        };
        out.insert(key, parsed);
    }
    Ok(out)
}

pub fn validate_folder(folder: &Path) -> Verdict {
    validate_folder_with_variables(folder, &VariableOverrides::new())
}

/// Validate one project folder in default variable mode (Sprint 9
/// variables, CAP-003): unknown override keys and declared-but-never-
/// bound variables warn visibly without blocking; every default-less
/// variable used without a fallback still refuses through the binding
/// pass. `check` and `render` without `--strict-variables` run here.
pub fn validate_folder_with_variables(folder: &Path, overrides: &VariableOverrides) -> Verdict {
    validate_folder_with_variables_strict(folder, overrides, false)
}

/// Validate one project folder with per-render caller values and an
/// explicit strict switch (Sprint 9 variables, CAP-004): the host
/// entry for overrides plus strict mode. Supplied values replace
/// matching defaults for this verdict only; nothing on disk changes,
/// and the returned compositions carry the resolved values. Default
/// mode (`strict` false) warns on unknown override keys and on
/// declared-but-never-bound variables without blocking; strict mode
/// (`--strict-variables`) promotes both to `E_BAD_VARIABLE` errors and
/// additionally requires every default-less variable to receive a
/// caller value. Unknown override values outside their variable's
/// type refuse before frame one in both modes, naming the variable
/// and the expected shape.
pub fn validate_folder_with_variables_strict(
    folder: &Path,
    overrides: &VariableOverrides,
    strict: bool,
) -> Verdict {
    validate_folder_with_variables_strict_themed(folder, overrides, strict, None)
}

/// Validate one project folder with per-render caller values, an
/// explicit strict switch, and a selected theme (Sprint 9 variables,
/// CAP-005): the host entry for overrides plus strict mode plus theme
/// selection. `theme` is the CLI `--theme` name (or the host's theme
/// choice); `None` selects no theme from the caller side, so each
/// composition falls back to its own `theme` attribute. The CLI theme
/// beats the composition attribute; an unknown name on either side
/// refuses before frame one naming the available themes. Theme tokens
/// resolve through the fixed precedence (caller override, then theme
/// token, then variable default, then binding fallback) with element
/// attributes written literally always winning outright.
pub fn validate_folder_with_variables_strict_themed(
    folder: &Path,
    overrides: &VariableOverrides,
    strict: bool,
    theme: Option<&str>,
) -> Verdict {
    let file = format!("{}/{COMPOSITION_FILE}", folder.display());
    let config_file = format!("{}/{CONFIG_FILE}", folder.display());

    if !folder.exists() {
        return Verdict {
            status: Status::Invalid,
            composition: None,
            compositions: Vec::new(),
            root_name: None,
            diagnostics: vec![Diagnostic::new(
                &file,
                1,
                1,
                "E_MISSING_DIR",
                format!("project folder {} does not exist", folder.display()),
                "point check at an existing folder or create one with `framesmith init`",
            )],
            variable_warnings: Vec::new(),
            ffmpeg_auto_download: true,
            footage: Vec::new(),
            inferred: Vec::new(),
            stagger_clamps: Vec::new(),
            crossfade_clamps: Vec::new(),
            effect_tails: Vec::new(),
            silences: Vec::new(),
            waveform_clamps: Vec::new(),
        };
    }

    let (ffmpeg_auto_download, config_diags) = validate_config(folder, &config_file);
    let config_invalid = config_diags.iter().any(|d| d.code != "E_UNSUPPORTED");

    let text = match std::fs::read_to_string(folder.join(COMPOSITION_FILE)) {
        Ok(t) => t,
        Err(e) => {
            let mut diags = config_diags;
            diags.push(Diagnostic::new(
                &file,
                1,
                1,
                if folder.join(COMPOSITION_FILE).exists() {
                    "E_UNREADABLE"
                } else {
                    "E_MISSING_COMPOSITION"
                },
                format!("cannot read {COMPOSITION_FILE}: {e}"),
                "create the composition with `framesmith init` or restore the file",
            ));
            return Verdict {
                status: Status::Invalid,
                composition: None,
                compositions: Vec::new(),
                root_name: None,
                diagnostics: diags,
                variable_warnings: Vec::new(),
                ffmpeg_auto_download,
                footage: Vec::new(),
                inferred: Vec::new(),
                stagger_clamps: Vec::new(),
                crossfade_clamps: Vec::new(),
                effect_tails: Vec::new(),
                silences: Vec::new(),
                waveform_clamps: Vec::new(),
            };
        }
    };

    let mut roots = match parse_document(&text) {
        Ok(r) => r,
        Err(e) => {
            let mut diags = config_diags;
            diags.push(Diagnostic::new(
                &file,
                e.line,
                e.column,
                "E_PARSE",
                format!("malformed KDL: {}", e.message),
                "fix the notation syntax at the reported location",
            ));
            return Verdict {
                status: Status::Invalid,
                composition: None,
                compositions: Vec::new(),
                root_name: None,
                diagnostics: diags,
                variable_warnings: Vec::new(),
                ffmpeg_auto_download,
                footage: Vec::new(),
                inferred: Vec::new(),
                stagger_clamps: Vec::new(),
                crossfade_clamps: Vec::new(),
                effect_tails: Vec::new(),
                silences: Vec::new(),
                waveform_clamps: Vec::new(),
            };
        }
    };

    let mut ctx = Ctx {
        file: file.clone(),
        folder,
        diags: config_diags,
        invalid: config_invalid,
        footage: Vec::new(),
        inferred: Vec::new(),
        stagger_clamps: Vec::new(),
        crossfade_clamps: Vec::new(),
        effect_tails: Vec::new(),
        silences: Vec::new(),
        waveform_clamps: Vec::new(),
        ids: Vec::new(),
        refs: Vec::new(),
        bindings: Vec::new(),
        current_comp: 0,
        overrides: overrides.clone(),
        override_warnings: Vec::new(),
        strict,
    };

    // Several uniquely named `composition` blocks validate together
    // (Sprint 6 T006); anything else at the top level is refused.
    let comp_positions: Vec<usize> = roots
        .iter()
        .enumerate()
        .filter(|(_, n)| n.name == "composition")
        .map(|(i, _)| i)
        .collect();
    if comp_positions.is_empty() {
        let (line, col) = roots
            .first()
            .map(|n| (n.name_line, n.name_column))
            .unwrap_or((1, 1));
        ctx.push(
            line,
            col,
            "E_MISSING_FIELD",
            "missing `composition` root node",
            "declare `composition name=... width=... ... { ... }`",
        );
        // Any other top-level node is still located for the author.
        // `theme` blocks are legal here (Sprint 9 variables, CAP-005)
        // but cannot save a project with no composition to render.
        for n in &roots {
            if n.name == "theme" {
                continue;
            }
            ctx.push(
                n.name_line,
                n.name_column,
                "E_UNKNOWN_ELEMENT",
                format!("unexpected top-level node {:?}", n.name),
                "only `composition` and `theme` blocks are allowed at the top level",
            );
        }
        return finish(ctx, None, Vec::new(), None, ffmpeg_auto_download);
    }
    for n in roots
        .iter()
        .filter(|n| n.name != "composition" && n.name != "theme")
    {
        ctx.push(
            n.name_line,
            n.name_column,
            "E_UNKNOWN_ELEMENT",
            format!("unexpected top-level node {:?}", n.name),
            "keep only `composition` and `theme` blocks at the top level",
        );
    }

    // Exactly one render root (Sprint 6 T006, REC-002): a lone
    // composition is the root implicitly and any `root` mark on it is
    // ignored; several compositions require exactly one `root=true`.
    let marks: Vec<Option<bool>> = comp_positions
        .iter()
        .map(|&i| parse_root_mark(&mut ctx, &roots[i]))
        .collect();
    let root_pos: usize = if comp_positions.len() == 1 {
        0
    } else {
        let marked: Vec<usize> = marks
            .iter()
            .enumerate()
            .filter(|(_, m)| **m == Some(true))
            .map(|(i, _)| i)
            .collect();
        if marked.len() == 1 {
            marked[0]
        } else {
            if marked.is_empty() {
                let first = &roots[comp_positions[0]];
                ctx.push(
                    first.name_line,
                    first.name_column,
                    "E_AMBIGUOUS_ROOT",
                    format!(
                        "project has {} compositions but none is marked `root=true`; exactly one composition must be the render root",
                        comp_positions.len()
                    ),
                    "mark exactly one composition `root=true` (`render` uses the root by default and `render --composition <name>` selects another)",
                );
            } else {
                let at = &roots[comp_positions[marked[marked.len() - 1]]];
                let (line, col) = find(at, "root")
                    .map(|v| (v.line, v.column))
                    .unwrap_or((at.name_line, at.name_column));
                let marked_names: Vec<String> = marked
                    .iter()
                    .map(|&i| {
                        let n = &roots[comp_positions[i]];
                        let label = find(n, "name").and_then(as_str).unwrap_or("?");
                        format!("{label:?} (line {})", n.name_line)
                    })
                    .collect();
                ctx.push(
                    line,
                    col,
                    "E_AMBIGUOUS_ROOT",
                    format!(
                        "project marks {} compositions as `root=true` ({}); exactly one composition must be the render root",
                        marked.len(),
                        marked_names.join(", ")
                    ),
                    "keep a single `root=true` and remove (or set `root=false` on) the other marks",
                );
            }
            0
        }
    };

    // --- per-composition validation --------------------------------------
    // Headers, children, and timing resolve per composition; ids stay
    // project-wide so duplicates and unknown references report
    // together (T001). Single-composition projects flow through the
    // same path, so their diagnostics keep their exact order.
    // Sprint 9 variables, CAP-005: named `theme` blocks validate once
    // project-wide (every composition shares the token collections),
    // then declarations validate and bindings substitute per
    // composition first (forward references resolve whatever the
    // document order), then headers and clips validate against the
    // substituted tree. Two loops keep the mutable substitution pass
    // apart from the shared validation borrows.
    let themes = collect_themes(&mut ctx, &roots);
    let mut all_variables: Vec<Vec<VariableDecl>> = Vec::new();
    for (ci, &ri) in comp_positions.iter().enumerate() {
        ctx.current_comp = ci;
        all_variables.push(collect_variables_and_substitute(
            &mut ctx,
            &mut roots[ri],
            &themes,
            theme,
        ));
    }
    // Sprint 9 variables, CAP-003: every override key naming no declared
    // variable in any composition warns visibly without blocking (default
    // mode). Sorted by key so repeated runs warn in the same order.
    // Sprint 9 variables, CAP-004: under `--strict-variables` the same
    // keys refuse as `E_BAD_VARIABLE` instead of warning.
    {
        let mut declared: BTreeSet<String> = BTreeSet::new();
        for vars in &all_variables {
            for d in vars {
                declared.insert(d.id.clone());
            }
        }
        let keys: Vec<String> = ctx.overrides.keys().cloned().collect();
        for key in &keys {
            if !declared.contains(key) {
                if ctx.strict {
                    ctx.push(
                        1,
                        1,
                        "E_BAD_VARIABLE",
                        format!(
                            "`--strict-variables` refuses unknown override key `--{key}`; no `variable` in the project declares `--{key}`, so the value matches nothing"
                        ),
                        format!(
                            "remove the key `--{key}` from `--variables`, declare `variable id=\"{key}\" type=\"...\"` in the composition, or drop `--strict-variables` for warning-only mode"
                        ),
                    );
                } else {
                    ctx.override_warnings.push(VariableWarning {
                        key: key.clone(),
                        message: format!(
                            "`--variables` names unknown variable `--{key}`; no `variable` in the project declares `--{key}`, so the value was ignored"
                        ),
                        fix: format!(
                            "remove the key from `--variables`, or declare `variable id=\"{key}\" type=\"...\"` in the composition"
                        ),
                    });
                }
            }
        }
    }
    // Sprint 9 variables, CAP-004: declared-but-never-bound variables
    // warn visibly without blocking in default mode and refuse as
    // `E_BAD_VARIABLE` under `--strict-variables`; strict mode
    // additionally requires every default-less variable to receive a
    // caller value. A variable counts as bound when any `var(--id)`
    // call names it in any composition (successful substitutions and
    // parseable-but-failed bindings alike); a caller value counts when
    // the override map carries the key, so a mistyped value (already
    // refused at the declaration) is never double-reported as missing.
    // Declarations report in document order so repeated runs agree.
    {
        use std::collections::BTreeSet as IdSet;
        let mut bound: IdSet<String> = IdSet::new();
        for site in &ctx.bindings {
            for id in &site.vars {
                bound.insert(id.clone());
            }
        }
        for vars in &all_variables {
            for d in vars {
                let used = bound.contains(&d.id);
                // The override pass above swaps a good caller value into
                // the in-memory default, so a missing default here with
                // no override key means no value from any source.
                let supplied = ctx.overrides.contains_key(&d.id);
                let has_value = d.default.is_some();
                if !used {
                    if ctx.strict {
                        ctx.push(
                            d.line,
                            d.column,
                            "E_BAD_VARIABLE",
                            format!(
                                "`--strict-variables` refuses unused variable `--{}`; it is declared but never bound by `var(--{})`, so the declaration has no effect",
                                d.id, d.id
                            ),
                            format!(
                                "bind the variable with `var(--{0})` in text, media, or an ordinary attribute, remove the `variable` declaration, or drop `--strict-variables` for warning-only mode",
                                d.id
                            ),
                        );
                    } else {
                        ctx.override_warnings.push(VariableWarning {
                            key: d.id.clone(),
                            message: format!(
                                "variable `--{0}` is declared but never bound by `var(--{0})`; the declaration has no effect",
                                d.id
                            ),
                            fix: format!(
                                "bind it with `var(--{0})` in text, media, or an ordinary attribute, or remove the `variable` declaration",
                                d.id
                            ),
                        });
                    }
                }
                if ctx.strict && !has_value && !supplied {
                    ctx.push(
                        d.line,
                        d.column,
                        "E_BAD_VARIABLE",
                        format!(
                            "`--strict-variables` refuses default-less variable `--{}` with no value; the declaration carries no `default` and no `--variables` value was supplied",
                            d.id
                        ),
                        format!(
                            "supply the value in `--variables` (for example `--variables '{{\"{}\": ...}}'` matching the variable's type), add `default=<value>` to the declaration, or drop `--strict-variables` for warning-only mode",
                            d.id
                        ),
                    );
                }
            }
        }
    }
    let mut partials: Vec<PartialComposition> = Vec::new();
    for (ci, &ri) in comp_positions.iter().enumerate() {
        ctx.current_comp = ci;
        let variables = all_variables[ci].clone();
        partials.push(validate_composition_parts(&mut ctx, &roots[ri], variables));
    }
    check_cross_composition_refs(&mut ctx, &partials);
    // --- project-wide ids and references (T001) ------------------------
    // Every duplicate id, unknown reference, and self-reference reports
    // here, together with the per-clip problems above.
    check_ids_and_refs(&mut ctx);
    let mut compositions: Vec<Composition> = Vec::new();
    for (ci, partial) in partials.into_iter().enumerate() {
        ctx.current_comp = ci;
        compositions.push(finish_composition(&mut ctx, partial));
    }
    // Sprint 6 T007: nest uses resolve against the finished composition
    // ends — past-end trims, circular chains, and over-deep chains
    // refuse here, before frame one, with the chain and the limit named.
    check_nest_graph(&mut ctx, &compositions);
    // Sprint 8 look (T004): mask and clip-path references resolve
    // against the finished compositions — dangling names, non-box
    // targets, and cycles refuse here, before frame one.
    check_mask_graph(&mut ctx, &compositions);
    // Sprint 10 sound (T003/T004): duck voices resolve against the
    // finished windows and overlong crossfade windows report their
    // clamp, before frame one.
    check_sound_windows(&mut ctx, &compositions);
    // Sprint 9 variables, CAP-002: name the variable behind every
    // substituted binding whose resolved value still failed to coerce,
    // so the refusal names the attribute and the variable together.
    amend_binding_diagnostics(&mut ctx);
    let (selected, all, root_name) = if !ctx.invalid {
        let selected = compositions[root_pos].clone();
        let name = compositions[root_pos].name.clone();
        (Some(selected), compositions, Some(name))
    } else {
        (None, Vec::new(), None)
    };
    finish(ctx, selected, all, root_name, ffmpeg_auto_download)
}

/// One composition's validated header plus children (Sprint 6 T006):
/// everything `finish_composition` needs to resolve windows, timing,
/// and motion for exactly this composition.
struct PartialComposition<'n> {
    root: &'n RawNode,
    name: String,
    comp_id: Option<String>,
    canvas_name: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    duration_omitted: bool,
    duration_written: Option<f64>,
    fps: Fps,
    background: notation::model::Rgb,
    version: u64,
    autoplay: bool,
    normalize: Normalize,
    normalize_loc: Option<(usize, usize)>,
    clips: Vec<Clip>,
    fonts: Vec<FontDecl>,
    pending_stills: Vec<usize>,
    motion_nodes: Vec<(bool, &'n RawNode)>,
    key_nodes: Vec<&'n RawNode>,
    marks: Vec<MarkDecl>,
    inputs: Vec<InputDecl>,
    variables: Vec<VariableDecl>,
    replicates: Vec<ReplicateGroup>,
}

/// Validate a `mark` element (Sprint 7 motion, CAP-003): a required
/// quoted `name` (id syntax, registered in the project namespace so a
/// duplicate fails as `E_DUP_ID`) and a required `at` time in finite
/// seconds >= 0. Marks carry no children. Whether the instant falls
/// inside composition time is checked in `finish_composition`, once the
/// (possibly inferred) composition end is known.
fn validate_mark(ctx: &mut Ctx, node: &RawNode) -> Option<MarkDecl> {
    const KEYS: &[&str] = &["name", "at"];
    let mut ok = true;
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown `mark` attribute {k:?}; a `mark` holds `name` and `at`"),
                "remove the key or check the spelling",
            );
            ok = false;
        }
    }
    if !node.children.is_empty() {
        let child = &node.children[0];
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_TIMING",
            "mark carries no children; a `mark` is one `name` with one `at` time",
            "remove the child elements from the mark",
        );
        ok = false;
    }
    let name = match find(node, "name").and_then(as_str) {
        Some(s) if valid_id_syntax(s) => Some(s.to_string()),
        _ => {
            match find(node, "name") {
                Some(v) => ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ID",
                    format!(
                        "mark `name` must be 1-64 letters, digits, `-` or `_`; found {}",
                        raw_text(v)
                    ),
                    "set `name=\"<letters-digits-dashes-underscores>\"` on the mark",
                ),
                None => ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_ID",
                    "mark is missing required `name`; every mark names one instant",
                    "add `name=\"<letters-digits-dashes-underscores>\"` to the mark",
                ),
            }
            ok = false;
            None
        }
    };
    let at = match find(node, "at").and_then(as_f64) {
        Some(x) if x.is_finite() && x >= 0.0 => Some(x),
        Some(x) => {
            let v = find(node, "at").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_TIMING",
                format!("mark `at` {x} is invalid; marks sit at seconds >= 0"),
                "set `at=<seconds from 0>` on the mark",
            );
            ok = false;
            None
        }
        None => {
            match find(node, "at") {
                Some(v) => ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "mark `at` is not a readable time in seconds",
                    "set `at=<seconds from 0>` on the mark",
                ),
                None => ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_TIMING",
                    "mark is missing required `at`; every mark names one instant in seconds",
                    "add `at=<seconds from 0>` to the mark",
                ),
            }
            ok = false;
            None
        }
    };
    if !ok {
        return None;
    }
    let (name, at) = (name.unwrap(), at.unwrap());
    ctx.ids.push(IdEntry {
        value: name.clone(),
        line: node.name_line,
        column: node.name_column,
        is_composition: false,
        is_mark: true,
        comp: ctx.current_comp,
    });
    Some(MarkDecl {
        name,
        at,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Largest `replicate` clone count (Sprint 7 motion, CAP-010): like
/// `repeat`, counts above 1000 are refused as unbounded-like, naming
/// the bound.
pub const REPLICATE_MAX: usize = 1000;

/// Validate an `input` element (Sprint 7 motion, CAP-010): a required
/// quoted `name` in expression identifier syntax (unique per
/// composition, never a reserved `t`/`i`/constant/function name) and
/// a required finite numeric `value`. `expr` reads these by name;
/// anything else is refused before frame one.
fn validate_input(ctx: &mut Ctx, node: &RawNode, declared: &[InputDecl]) -> Option<InputDecl> {
    const KEYS: &[&str] = &["name", "value"];
    let mut ok = true;
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown `input` attribute {k:?}; an `input` holds `name` and `value`"),
                "remove the key or check the spelling",
            );
            ok = false;
        }
    }
    if !node.children.is_empty() {
        let child = &node.children[0];
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_MOTION",
            "input carries no children; an `input` is one `name` with one `value`",
            "remove the child elements from the input",
        );
        ok = false;
    }
    let name = match find(node, "name").and_then(as_str) {
        Some(s) if motion::expr::valid_name(s) => Some(s.to_string()),
        Some(_) => {
            let v = find(node, "name").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_ID",
                format!(
                    "input `name` must be 1-64 letters, digits, or underscores, not starting with a digit; found {}",
                    raw_text(v)
                ),
                "set `name=\"<letters-digits-underscores>\"` on the input",
            );
            ok = false;
            None
        }
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_ID",
                "input is missing required `name`; every input names one number",
                "add `name=\"<letters-digits-underscores>\"` to the input",
            );
            ok = false;
            None
        }
    };
    if let Some(ref n) = name {
        if motion::expr::reserved(n) {
            let v = find(node, "name").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("input `name` {n:?} is reserved; `t`, `i`, constants, and function names cannot be inputs"),
                "rename the input to a non-reserved identifier",
            );
            ok = false;
        }
        if declared.iter().any(|d| &d.name == n) {
            let v = find(node, "name").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_DUP_ID",
                format!("duplicate input {n:?}; every input name is unique per composition"),
                "rename the input or remove the duplicate",
            );
            ok = false;
        }
    }
    let value = match find(node, "value").and_then(as_f64) {
        Some(x) if x.is_finite() => Some(x),
        Some(x) => {
            let v = find(node, "value").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("input `value` {x} is invalid; inputs hold one finite number"),
                "set `value=<number>` on the input",
            );
            ok = false;
            None
        }
        None => {
            match find(node, "value") {
                Some(v) => ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    "input `value` is not a readable number",
                    "set `value=<number>` on the input",
                ),
                None => ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_MOTION",
                    "input is missing required `value`; every input names the number it holds",
                    "add `value=<number>` to the input",
                ),
            }
            ok = false;
            None
        }
    };
    if !ok {
        return None;
    }
    Some(InputDecl {
        name: name.unwrap(),
        value: value.unwrap(),
        line: node.name_line,
        column: node.name_column,
    })
}

/// Validate a `replicate` element (Sprint 7 motion, CAP-010): a
/// required whole-number `count` of 1 to 1000 and exactly one
/// template clip child (`text`, `box`, `image`, or `video`). The
/// template expands to `count` clones inline (clone `k` exposes
/// piece index `i = k`), registered in `replicates`. Non-positive
/// counts fail; the template carries no `id` (clones would collide
/// in the project namespace).
fn validate_replicate(
    ctx: &mut Ctx,
    node: &RawNode,
    clips: &mut Vec<Clip>,
    pending_stills: &mut Vec<usize>,
    replicates: &mut Vec<ReplicateGroup>,
    styles: &[StyleDecl],
) {
    const KEYS: &[&str] = &["count"];
    let mut ok = true;
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown `replicate` attribute {k:?}; a `replicate` holds `count` and one template clip"),
                "remove the key or check the spelling",
            );
            ok = false;
        }
    }
    let count = match find(node, "count").and_then(as_f64) {
        Some(x) if x.is_finite() && x.fract() == 0.0 && x >= 1.0 && x <= REPLICATE_MAX as f64 => {
            Some(x as usize)
        }
        Some(x) => {
            let v = find(node, "count").unwrap();
            let why = if !x.is_finite() || x.fract() != 0.0 {
                "a whole number of clones"
            } else if x < 1.0 {
                "at least 1 clone"
            } else {
                &format!("at most {REPLICATE_MAX} clones")
            };
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("replicate `count` {x} is invalid; a replicate clones {why}"),
                format!("set `count=<whole number from 1 to {REPLICATE_MAX}>` on the replicate"),
            );
            ok = false;
            None
        }
        None => {
            match find(node, "count") {
                Some(v) => ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    "replicate `count` is not a readable whole number",
                    "set `count=<whole number>` on the replicate",
                ),
                None => ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_MOTION",
                    "replicate is missing required `count`; every replicate names its clone count",
                    "add `count=<whole number>` to the replicate",
                ),
            }
            ok = false;
            None
        }
    };
    if node.children.is_empty() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_MOTION",
            "replicate holds no template clip; a `replicate` clones one `text`, `box`, `image`, or `video` child",
            "add one template clip inside the replicate",
        );
        ok = false;
    } else if node.children.len() > 1 {
        let extra = &node.children[1];
        ctx.push(
            extra.name_line,
            extra.name_column,
            "E_BAD_MOTION",
            format!(
                "replicate holds {} template clips; a `replicate` clones exactly one child",
                node.children.len()
            ),
            "keep one template clip inside the replicate",
        );
        ok = false;
    }
    if !ok {
        return;
    }
    let template = &node.children[0];
    if find(template, "id").is_some() {
        let v = find(template, "id").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            "replicate template carries `id`; every clone would share it in the project namespace",
            "remove `id` from the template clip; clones expose `i` instead",
        );
        return;
    }
    let clip = match template.name.as_str() {
        "text" => validate_text(ctx, template, styles).map(Clip::Text),
        "box" => validate_box(ctx, template, styles).map(Clip::Box),
        "image" => validate_image(ctx, template, styles).map(Clip::Image),
        "path" => validate_path(ctx, template, styles).map(Clip::Path),
        "svg" => validate_svg(ctx, template, styles).map(Clip::Svg),
        "video" => validate_video(ctx, template).map(Clip::Video),
        "audio" | "nest" | "font" | "flex" => {
            ctx.push(
                template.name_line,
                template.name_column,
                "E_BAD_MOTION",
                format!(
                    "replicate template is `{}`; a `replicate` clones one picture clip (`text`, `box`, `image`, `path`, `svg`, or `video`)",
                    template.name
                ),
                "template a picture clip inside the replicate",
            );
            return;
        }
        other => {
            ctx.push(
                template.name_line,
                template.name_column,
                "E_BAD_MOTION",
                format!(
                    "replicate template is `{other}`; a `replicate` clones one `text`, `box`, `image`, `path`, `svg`, or `video` child, never motion or another replicate"
                ),
                "template one picture clip inside the replicate",
            );
            return;
        }
    };
    let Some(clip) = clip else {
        return;
    };
    let count = count.unwrap();
    let first = clips.len();
    let still_omitted = clip.duration().is_nan();
    for _ in 0..count {
        clips.push(clip.clone());
        if still_omitted {
            pending_stills.push(clips.len() - 1);
        }
    }
    replicates.push(ReplicateGroup { first, count });
}

/// Parse one composition `root` mark (Sprint 6 T006): absent means
/// "not marked"; present must be a bare `true` or `false`.
fn parse_root_mark(ctx: &mut Ctx, root: &RawNode) -> Option<bool> {
    match find(root, "root") {
        None => None,
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_AMBIGUOUS_ROOT",
                    format!(
                        "composition `root` must be `true` or `false`, found {}",
                        raw_text(v)
                    ),
                    "set `root=true` on the one default composition, or remove the key",
                );
                None
            }
        },
    }
}

/// Parse one composition `autoplay` switch (Sprint 10 sound, CAP-007):
/// absent means the default still start; present must be a bare
/// `true`/`false` (a quoted `"true"`/`"false"` reads the same).
/// Anything else fails with `E_BAD_FLAG`, never guessed, so `info`
/// only ever carries a switch the maker wrote.
fn parse_autoplay(ctx: &mut Ctx, root: &RawNode) -> Option<bool> {
    match find(root, "autoplay") {
        None => Some(false),
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            ValueKind::Str(s)
                if s.trim().eq_ignore_ascii_case("true")
                    || s.trim().eq_ignore_ascii_case("false") =>
            {
                Some(s.trim().eq_ignore_ascii_case("true"))
            }
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLAG",
                    format!(
                        "composition `autoplay` must be `true` or `false`; found {}",
                        raw_text(v)
                    ),
                    "set `autoplay=true` to hint auto-playback, or `autoplay=false` (or remove the key) for the default still start",
                );
                None
            }
        },
    }
}

/// Parse one composition `normalize` target (Sprint 10 sound,
/// CAP-009): absent means authored levels; present must be the quoted
/// `"loud"` landing integrated loudness at −16 LUFS with true-peak
/// guarding. Anything else fails with `E_BAD_AUDIO`, never guessed,
/// so the mixer only ever applies a target the maker wrote.
fn parse_normalize(ctx: &mut Ctx, root: &RawNode) -> Option<(Normalize, Option<(usize, usize)>)> {
    match find(root, "normalize") {
        None => Some((Normalize::None, None)),
        Some(v) => match as_str(v).and_then(Normalize::parse) {
            Some(target) => Some((target, Some((v.line, v.column)))),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "composition `normalize` {} is unknown; accepted targets are: {}",
                        raw_text(v),
                        Normalize::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "set `normalize=\"loud\"` for −16 LUFS one-pass loudness, or remove the key for authored levels",
                );
                None
            }
        },
    }
}

/// Refuse `start-after` targets that live in another composition
/// (Sprint 6 T006): each composition renders its own timeline, so a
/// clip can only follow clips in its own composition or name the
/// composition itself (which resolves to its end). `nest` targets
/// stay project-wide; nest execution is T007.
fn check_cross_composition_refs(ctx: &mut Ctx, partials: &[PartialComposition]) {
    for i in 0..ctx.refs.len() {
        let (target, line, column, owner) = {
            let r = &ctx.refs[i];
            if r.needs_composition {
                continue;
            }
            // Sprint 10 sound (T004): `duck-under` voices resolve in
            // `check_sound_windows` with a voice-specific diagnostic;
            // this walk names `start-after` relations only.
            if r.kind != "start-after" {
                continue;
            }
            (r.target.clone(), r.line, r.column, r.comp)
        };
        let hits: Vec<(bool, bool, usize)> = ctx
            .ids
            .iter()
            .filter(|e| e.value == target)
            .map(|e| (e.is_composition, e.is_mark, e.comp))
            .collect();
        if hits.len() != 1 {
            // Unknown targets report in `check_ids_and_refs`; duplicated
            // targets report as `E_DUP_ID` there. Neither needs a second
            // diagnostic here.
            continue;
        }
        let (hit_is_comp, hit_is_mark, hit_comp) = hits[0];
        if hit_comp == owner {
            continue;
        }
        let own = partials[owner].name.clone();
        let other = partials[hit_comp].name.clone();
        if hit_is_comp {
            ctx.push(
                line,
                column,
                "E_UNKNOWN_REF",
                format!(
                    "`start-after` {target:?} names composition {other:?}, but this clip is in composition {own:?}; a clip can only follow clips in its own composition or the composition itself"
                ),
                format!("point `start-after=` at a clip id in {own:?}, or at {own:?} itself"),
            );
        } else if hit_is_mark {
            ctx.push(
                line,
                column,
                "E_UNKNOWN_REF",
                format!(
                    "`start-after` {target:?} names a mark in composition {other:?}, but this clip is in composition {own:?}; a clip can only follow clips or marks in its own composition"
                ),
                format!("point `start-after=` at a clip id or mark in {own:?}"),
            );
        } else {
            ctx.push(
                line,
                column,
                "E_UNKNOWN_REF",
                format!(
                    "`start-after` {target:?} names a clip in composition {other:?}, but this clip is in composition {own:?}; a clip can only follow clips in its own composition"
                ),
                format!("point `start-after=` at a clip id in {own:?}"),
            );
        }
    }
}

/// One named shared look (Sprint 8 look, CAP-006/T006): a `style`
/// composition child carrying scalar properties by name. Elements
/// name it with `style="look"` (space-separated for several).
/// Validation bakes the resolved values into each referring element
/// before its own validator runs, so a styled element shares one
/// code path — and one refusal vocabulary — with direct values.
struct StyleDecl {
    name: String,
    props: Vec<(String, RawValue)>,
}

/// Every scalar key a `style` block may declare (Sprint 8 look,
/// CAP-006/T006): the union of the scalar attribute vocabularies of
/// the styleable elements (`text`, `box`, `flex` containers and child
/// boxes, `image`, `path`, `svg`), in both hyphenated and underscore
/// spellings where the element accepts both. Whether a property fits
/// a particular element stays the element validator's call — a style
/// value routes through the exact same check as the directly written
/// value — so this set only guards the block itself against typos.
const STYLE_KEYS: &[&str] = &[
    "start",
    "duration",
    "id",
    "track",
    "label",
    "hidden",
    "start-after",
    "start_after",
    "start-offset",
    "start_offset",
    "x",
    "y",
    "width",
    "height",
    "fill",
    "radius",
    "border-width",
    "border_width",
    "border-color",
    "border_color",
    "stroke-width",
    "stroke_width",
    "stroke-color",
    "stroke_color",
    "stroke-position",
    "stroke_position",
    "box-shadow",
    "box_shadow",
    "mix-blend-mode",
    "mix_blend_mode",
    "backdrop-blur",
    "backdrop_blur",
    "backdrop-brightness",
    "backdrop_brightness",
    "overflow",
    "mask",
    "clip-path",
    "clip_path",
    "opacity",
    "order",
    "rotation-x",
    "rotation_x",
    "rotate-x",
    "rotate_x",
    "rotation-y",
    "rotation_y",
    "rotate-y",
    "rotate_y",
    "perspective",
    "origin",
    "filter",
    "value",
    "size",
    "color",
    "position",
    "align",
    "wrap",
    "font-style",
    "font-family",
    "font_family",
    "direction",
    "line-height",
    "line_height",
    "letter-spacing",
    "letter_spacing",
    "split",
    "typewriter",
    "count-from",
    "count_from",
    "count-to",
    "count_to",
    "decimals",
    "display",
    "flex-direction",
    "flex_direction",
    "flex-grow",
    "flex_grow",
    "gap",
    "padding",
    "margin",
    "align-items",
    "align_items",
    "justify-content",
    "justify_content",
    "path",
    "fit",
    "placement",
    "d",
    "fill-rule",
    "fill_rule",
];

/// Canonical (hyphenated) spellings of the shared-look vocabulary,
/// for refusal messages that name the valid set.
const STYLE_CANONICAL: &[&str] = &[
    "start",
    "duration",
    "id",
    "track",
    "label",
    "hidden",
    "start-after",
    "start-offset",
    "x",
    "y",
    "width",
    "height",
    "fill",
    "radius",
    "border-width",
    "border-color",
    "stroke-width",
    "stroke-color",
    "stroke-position",
    "box-shadow",
    "mix-blend-mode",
    "backdrop-blur",
    "backdrop-brightness",
    "overflow",
    "mask",
    "clip-path",
    "opacity",
    "order",
    "rotation-x",
    "rotation-y",
    "perspective",
    "origin",
    "filter",
    "value",
    "size",
    "color",
    "position",
    "align",
    "wrap",
    "font-style",
    "font-family",
    "direction",
    "line-height",
    "letter-spacing",
    "split",
    "typewriter",
    "count-from",
    "count-to",
    "decimals",
    "display",
    "flex-direction",
    "flex-grow",
    "gap",
    "padding",
    "margin",
    "align-items",
    "justify-content",
    "path",
    "fit",
    "placement",
    "d",
    "fill-rule",
];

/// Human list of the valid style properties, for diagnostics.
fn style_valid_set() -> String {
    STYLE_CANONICAL
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Human list of the declared style names, for diagnostics.
fn style_available_names(styles: &[StyleDecl]) -> String {
    if styles.is_empty() {
        "(no styles are declared yet)".to_string()
    } else {
        styles
            .iter()
            .map(|s| format!("`{}`", s.name))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Validate one `style` composition child (Sprint 8 look, CAP-006):
/// a quoted `name` with id syntax, no duplicate block name, no
/// `style` key of its own (styles never nest), and only vocabulary
/// keys. Every bad property reports with the valid set; the block is
/// registered only when fully usable.
fn validate_style_block(
    ctx: &mut Ctx,
    node: &RawNode,
    styles: &[StyleDecl],
) -> Option<StyleDecl> {
    let name = match find(node, "name").and_then(as_str) {
        Some(s) if valid_id_syntax(s) => s.to_string(),
        _ => {
            let (line, column) = find(node, "name")
                .map(|v| (v.line, v.column))
                .unwrap_or((node.name_line, node.name_column));
            ctx.push(
                line,
                column,
                "E_BAD_STYLE",
                "style block is missing required `name` (1-64 letters, digits, `-` or `_`)",
                "add `name=\"<look-name>\"` to the style block",
            );
            return None;
        }
    };
    if let Some(_dup) = styles.iter().find(|s| s.name == name) {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_STYLE",
            format!(
                "style {name:?} is declared more than once; available styles are: {}",
                style_available_names(styles)
            ),
            format!("rename this block to a unique name, or remove it and use the declared `{name}`"),
        );
        return None;
    }
    let mut ok = true;
    let mut props = Vec::new();
    for (k, v) in &node.props {
        if k == "name" || k == "end" {
            continue;
        }
        if k == "style" {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_STYLE",
                format!(
                    "style {name:?} carries `style=`; styles never reference other styles"
                ),
                "remove the `style` key from the style block, or move the properties onto the element",
            );
            ok = false;
            continue;
        }
        if !STYLE_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_STYLE",
                format!(
                    "style {name:?} declares unknown property {k:?}; valid style properties are: {}",
                    style_valid_set()
                ),
                "remove the property or check the spelling against the valid set",
            );
            ok = false;
            continue;
        }
        props.push((k.clone(), v.clone()));
    }
    if !ok {
        return None;
    }
    Some(StyleDecl { name, props })
}

/// Resolve the `style` references of one element (Sprint 8 look,
/// CAP-006): every `style` attribute contributes its space-separated
/// names in document order, later names winning over earlier ones on
/// shared properties, and element-written attributes winning over
/// every style. The merged node keeps the element's own props first
/// (so `find` keeps hitting them) with style values appended after,
/// at style-block locations — the element validator below then reads
/// styled values through its exact direct-value code path. Unknown
/// names fail with the available styles; a missing or unreadable
/// `style` value fails where it is written.
fn styled_node<'a>(
    ctx: &mut Ctx,
    node: &'a RawNode,
    what: &str,
    styles: &[StyleDecl],
) -> Option<Cow<'a, RawNode>> {
    let refs: Vec<&RawValue> = node
        .props
        .iter()
        .filter(|(k, _)| *k == "style")
        .map(|(_, v)| v)
        .collect();
    if refs.is_empty() {
        return Some(Cow::Borrowed(node));
    }
    let mut names: Vec<(&str, usize, usize)> = Vec::new();
    for v in refs {
        match as_str(v) {
            Some(s) => {
                let parts: Vec<&str> = s.split_whitespace().collect();
                if parts.is_empty() {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_STYLE",
                        format!(
                            "{what} `style` names no block; available styles are: {}",
                            style_available_names(styles)
                        ),
                        "point `style=` at a declared style name, or remove the key",
                    );
                    return None;
                }
                for part in parts {
                    names.push((part, v.line, v.column));
                }
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_STYLE",
                    format!(
                        "{what} `style` must be a quoted style name; found {}",
                        raw_text(v)
                    ),
                    "set `style=\"<look-name>\"` on the element",
                );
                return None;
            }
        }
    }
    let mut merged: Vec<(String, RawValue)> = node
        .props
        .iter()
        .filter(|(k, _)| *k != "style")
        .cloned()
        .collect();
    // Element-written props stay first and untouchable: only appended
    // style entries (at or past `owned_len`) may be overwritten by a
    // later-declared style naming the same property.
    let owned_len = merged.len();
    for (name, line, column) in names {
        let style = match styles.iter().find(|s| s.name == name) {
            Some(s) => s,
            None => {
                ctx.push(
                    line,
                    column,
                    "E_BAD_STYLE",
                    format!(
                        "{what} style {name:?} names no declared style; available styles are: {}",
                        style_available_names(styles)
                    ),
                    format!(
                        "declare `style name={name:?} ...` in the composition, or point `style=` at one of: {}",
                        style_available_names(styles)
                    ),
                );
                return None;
            }
        };
        for (k, v) in &style.props {
            match merged.iter().position(|(ek, _)| ek == k) {
                Some(i) if i >= owned_len => {
                    merged[i].1 = v.clone();
                }
                Some(_) => {}
                None => merged.push((k.clone(), v.clone())),
            }
        }
    }
    Some(Cow::Owned(RawNode {
        name: node.name.clone(),
        name_line: node.name_line,
        name_column: node.name_column,
        props: merged,
        children: node.children.clone(),
    }))
}

fn validate_composition_parts<'n>(
    mut ctx: &mut Ctx,
    root: &'n RawNode,
    variables: Vec<VariableDecl>,
) -> PartialComposition<'n> {
    // --- root properties -------------------------------------------------
    let name = match find(root, "name").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                root.name_line,
                root.name_column,
                "E_MISSING_FIELD",
                "composition is missing required `name`",
                "add `name=\"...\"` to the composition root",
            );
            String::new()
        }
    };
    // Unknown root keys are invalid, never ignored (T003). `theme`
    // selects one named `theme` block (Sprint 9 variables, CAP-005);
    // its value stays literal through substitution and resolves in
    // the pre-pass, so it never reaches the readers here.
    const ROOT_KEYS: &[&str] = &[
        "name",
        "id",
        "root",
        "width",
        "height",
        "canvas",
        "duration",
        "fps",
        "background",
        "version",
        "autoplay",
        "normalize",
        "theme",
    ];
    for (k, v) in &root.props {
        if !ROOT_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown composition key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }

    // Composition `id` (T001): an optional stable reference name with
    // clip-id syntax, registered in the project namespace for
    // uniqueness and reference checks.
    let comp_id = match find(root, "id") {
        None => None,
        Some(v) => match as_str(v) {
            Some(s) if valid_id_syntax(s) => Some(s.to_string()),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ID",
                    format!(
                        "composition `id` must be 1-64 letters, digits, `-` or `_`; found {}",
                        raw_text(v)
                    ),
                    "set `id=\"<letters-digits-dashes-underscores>\"` on the composition root",
                );
                None
            }
        },
    };
    // The composition name answers id references too, so a `start-after`
    // or `nest` can name the composition by `name` or by `id`.
    if !name.is_empty() {
        ctx.ids.push(IdEntry {
            value: name.clone(),
            line: root.name_line,
            column: root.name_column,
            is_composition: true,
            is_mark: false,
            comp: ctx.current_comp,
        });
    }
    if let Some(id) = &comp_id {
        if id != &name {
            let v = find(root, "id").unwrap();
            ctx.ids.push(IdEntry {
                value: id.clone(),
                line: v.line,
                column: v.column,
                is_composition: true,
                is_mark: false,
                comp: ctx.current_comp,
            });
        }
    }

    // Canvas preset (T005): `canvas` names a preset sizing the canvas;
    // explicit `width`/`height` override it per dimension. An unknown
    // preset fails with the accepted names. Without `canvas` the
    // explicit dimensions stay required, exactly as before.
    let (canvas_dim, canvas_name) = match find(root, "canvas") {
        None => (None, None),
        Some(v) => match as_str(v).and_then(|s| {
            canvas_preset_dims(s).map(|d| (d, canvas_canonical(s).unwrap().to_string()))
        }) {
            Some(((w, h), canonical)) => (Some((w as f64, h as f64)), Some(canonical)),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_CANVAS",
                    format!(
                        "unknown canvas preset {}; accepted presets are: {}",
                        raw_text(v),
                        accepted_canvases()
                    ),
                    "set `canvas=` to one of the accepted presets, or remove it and use explicit `width`/`height`",
                );
                (None, None)
            }
        },
    };

    let mut need = |key: &str, what: &str, preset: Option<f64>| -> Option<f64> {
        match find(root, key).and_then(as_f64) {
            Some(x) => Some(x),
            None => {
                if find(root, key).is_none() {
                    if let Some(p) = preset {
                        return Some(p);
                    }
                }
                let (line, col) = find(root, key)
                    .map(|v| (v.line, v.column))
                    .unwrap_or((root.name_line, root.name_column));
                ctx.push(
                    line,
                    col,
                    if find(root, key).is_none() {
                        "E_MISSING_FIELD"
                    } else {
                        "E_BAD_DURATION"
                    },
                    format!("composition {what} must be a positive number"),
                    format!("set `{key}=<positive number>` on the composition root"),
                );
                None
            }
        }
    };
    let width = need("width", "width", canvas_dim.map(|(w, _)| w));
    let height = need("height", "height", canvas_dim.map(|(_, h)| h));
    // Composition `duration` (Sprint 6 T003): a written value must be a
    // positive number of seconds; an omitted key resolves to the latest
    // finite clip end once relative starts and clip inference land.
    // `duration_omitted` tells the inference loop an omission apart
    // from a refused value (a refused value must never infer).
    let duration_omitted = find(root, "duration").is_none();
    let duration_written: Option<f64> = match find(root, "duration") {
        None => None,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x > 0.0 => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_DURATION",
                    format!(
                        "composition duration must be a positive number, found {}",
                        raw_text(v)
                    ),
                    "set `duration=<positive seconds>` on the composition root",
                );
                None
            }
        },
    };
    for (key, val) in [("width", width), ("height", height)] {
        if let Some(x) = val {
            if x <= 0.0 || x.fract() != 0.0 {
                let (line, col) = find(root, key).map(|v| (v.line, v.column)).unwrap();
                ctx.push(
                    line,
                    col,
                    "E_BAD_CANVAS",
                    format!("composition {key} must be positive, found {x}"),
                    format!("set `{key}` to a positive value"),
                );
            }
        }
    }

    // fps: integer or exact rational in 1..=240, default 30.
    // A decimal value is rejected (never silently rounded).
    let fps = match find(root, "fps") {
        None => Fps::from_int(30),
        Some(v) => match &v.kind {
            ValueKind::Int(n) => {
                if *n < 1 || *n > 240 {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FPS",
                        format!("fps {n} is outside the accepted range 1 to 240"),
                        "use an integer or exact rational fps within 1 to 240",
                    );
                    Fps::from_int(30)
                } else {
                    Fps::from_int(*n as u64)
                }
            }
            ValueKind::Ratio(num, den) => {
                if *den == 0 {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FPS",
                        "fps rational has a zero denominator",
                        "use `<num>/<den>` with a non-zero denominator",
                    );
                    Fps::from_int(30)
                } else {
                    let value = *num as f64 / *den as f64;
                    if value < 1.0 || value > 240.0 {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_BAD_FPS",
                            format!("fps {num}/{den} is outside the accepted range 1 to 240"),
                            "use an integer or exact rational fps within 1 to 240",
                        );
                        Fps::from_int(30)
                    } else {
                        Fps {
                            num: *num,
                            den: *den,
                        }
                    }
                }
            }
            ValueKind::Str(s) if s.contains('/') => {
                match s
                    .split_once('/')
                    .map(|(a, b)| (a.parse::<u64>(), b.parse::<u64>()))
                {
                    Some((Ok(num), Ok(den))) if den != 0 => {
                        let value = num as f64 / den as f64;
                        if value < 1.0 || value > 240.0 {
                            ctx.push(
                                v.line,
                                v.column,
                                "E_BAD_FPS",
                                format!("fps {s:?} is outside the accepted range 1 to 240"),
                                "use an integer or exact rational fps within 1 to 240",
                            );
                            Fps::from_int(30)
                        } else {
                            Fps { num, den }
                        }
                    }
                    _ => {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_BAD_FPS",
                            format!("fps {s:?} is not an exact rational"),
                            "use an integer like `fps=30` or a rational like `fps=\"30000/1001\"`",
                        );
                        Fps::from_int(30)
                    }
                }
            }
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FPS",
                    "fps must be an integer or an exact rational; decimal fps is rejected",
                    "use an integer like `fps=30` or a rational like `fps=\"30000/1001\"`",
                );
                Fps::from_int(30)
            }
        },
    };

    let background = match find(root, "background").and_then(as_str) {
        Some(s) => match parse_color(s) {
            Some(c) => c,
            None => {
                let v = find(root, "background").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    format!("background {s:?} is not a `#rgb` or `#rrggbb` color"),
                    "use a hex color like `background=\"#1a1a2e\"`",
                );
                notation::model::Rgb { r: 0, g: 0, b: 0 }
            }
        },
        None => {
            ctx.push(
                root.name_line,
                root.name_column,
                "E_MISSING_FIELD",
                "composition is missing required `background`",
                "add `background=\"#rrggbb\"` to the composition root",
            );
            notation::model::Rgb { r: 0, g: 0, b: 0 }
        }
    };

    let version = match find(root, "version").and_then(as_int) {
        Some(n) if n >= 1 => n as u64,
        Some(n) => {
            let v = find(root, "version").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_VERSION",
                format!("notation version {n} is invalid; versions start at 1"),
                "set `version=1` on the composition root",
            );
            1
        }
        None => {
            let is_present = find(root, "version").is_some();
            let (line, col) = find(root, "version")
                .map(|v| (v.line, v.column))
                .unwrap_or((root.name_line, root.name_column));
            ctx.push(
                line,
                col,
                if is_present {
                    "E_BAD_VERSION"
                } else {
                    "E_MISSING_FIELD"
                },
                "composition is missing required integer `version`",
                "add `version=1` to the composition root",
            );
            1
        }
    };

    // Playback hint (Sprint 10 sound, CAP-007): metadata only, so a
    // bad value fails here while the mix stays untouched.
    let autoplay = parse_autoplay(&mut ctx, root);
    // One-pass loudness target (Sprint 10 sound, CAP-009): absent
    // means authored levels; only the quoted `"loud"` target mixes.
    let (normalize, normalize_loc) =
        parse_normalize(&mut ctx, root).unwrap_or((Normalize::None, None));

    // --- child elements --------------------------------------------------
    let mut clips: Vec<Clip> = Vec::new();
    let mut fonts: Vec<FontDecl> = Vec::new();
    // Document indexes into `clips` of stills (text, box, image) whose
    // omitted `duration` still waits for the composition end. Kept
    // per composition (T006) so indexes stay valid for this `clips`.
    let mut pending_stills: Vec<usize> = Vec::new();
    // Motion directives validate after the clip pass so `target`
    // indexes resolve against the declared clip count (T003).
    let mut motion_nodes: Vec<(bool, &RawNode)> = Vec::new();
    let mut key_nodes: Vec<&RawNode> = Vec::new();
    // Named instants (Sprint 7 motion, CAP-003): `start-after` accepts
    // them once the composition end is known in `finish_composition`.
    let mut marks: Vec<MarkDecl> = Vec::new();
    // Declared expression inputs (Sprint 7 motion, CAP-010): `expr`
    // resolves bare names against these once motion validates in
    // `finish_composition`.
    let mut inputs: Vec<InputDecl> = Vec::new();
    // Declared render variables (Sprint 9 variables, CAP-001/002):
    // validated in the pre-pass by `collect_variables_and_substitute`
    // (so forward references resolve) and passed in here. Unused
    // declarations stay valid here; unused reporting runs project-wide
    // after substitution (a warning in default mode, an error under
    // `--strict-variables`).
    // `replicate` expansions (Sprint 7 motion, CAP-010): template
    // clips expand to indexed clones inline below, so `target`
    // indexes resolve against the final clip count like any clip.
    let mut replicates: Vec<ReplicateGroup> = Vec::new();
    // Shared looks (Sprint 8 look, CAP-006/T006): style blocks
    // validate in a pre-pass so referring elements resolve them
    // whatever the declaration order (forward references allowed).
    let mut styles: Vec<StyleDecl> = Vec::new();
    for child in &root.children {
        if child.name == "style" {
            if let Some(s) = validate_style_block(&mut ctx, child, &styles) {
                styles.push(s);
            }
        }
    }
    for child in &root.children {
        match child.name.as_str() {
            // Stills (text, box, image) may omit `duration`: the
            // validator stores `NAN` for the omission and the index
            // joins `pending_stills` for the inference loop below.
            "text" => {
                if let Some(c) = validate_text(&mut ctx, child, &styles) {
                    if c.duration.is_nan() {
                        pending_stills.push(clips.len());
                    }
                    clips.push(Clip::Text(c));
                }
            }
            "video" => {
                if let Some(c) = validate_video(&mut ctx, child) {
                    clips.push(Clip::Video(c));
                }
            }
            "font" => {
                if let Some(f) = validate_font(&mut ctx, child) {
                    fonts.push(f);
                }
            }
            "style" => {}
            "box" => {
                if let Some(c) = validate_box(&mut ctx, child, &styles) {
                    if c.duration.is_nan() {
                        pending_stills.push(clips.len());
                    }
                    clips.push(Clip::Box(c));
                }
            }
            "flex" => {
                if let Some(c) = validate_flex(&mut ctx, child, &styles) {
                    if c.duration.is_nan() {
                        pending_stills.push(clips.len());
                    }
                    clips.push(Clip::Flex(c));
                }
            }
            "image" => {
                if let Some(c) = validate_image(&mut ctx, child, &styles) {
                    if c.duration.is_nan() {
                        pending_stills.push(clips.len());
                    }
                    clips.push(Clip::Image(c));
                }
            }
            "path" => {
                if let Some(c) = validate_path(&mut ctx, child, &styles) {
                    if c.duration.is_nan() {
                        pending_stills.push(clips.len());
                    }
                    clips.push(Clip::Path(c));
                }
            }
            "svg" => {
                if let Some(c) = validate_svg(&mut ctx, child, &styles) {
                    if c.duration.is_nan() {
                        pending_stills.push(clips.len());
                    }
                    clips.push(Clip::Svg(c));
                }
            }
            "audio" => {
                if let Some(c) = validate_audio(&mut ctx, child) {
                    clips.push(Clip::Audio(c));
                }
            }
            "nest" => {
                if let Some(c) = validate_nest(&mut ctx, child) {
                    clips.push(Clip::Nest(c));
                }
            }
            "tween" => motion_nodes.push((true, child)),
            "set" => motion_nodes.push((false, child)),
            "key" => key_nodes.push(child),
            "mark" => {
                if let Some(m) = validate_mark(&mut ctx, child) {
                    marks.push(m);
                }
            }
            "input" => {
                if let Some(d) = validate_input(&mut ctx, child, &inputs) {
                    inputs.push(d);
                }
            }
            // `variable` declarations validated in the pre-pass
            // (`collect_variables_and_substitute`); skipping them here
            // keeps every declaration validated exactly once.
            "variable" => {}
            "replicate" => {
                validate_replicate(
                    &mut ctx,
                    child,
                    &mut clips,
                    &mut pending_stills,
                    &mut replicates,
                    &styles,
                );
            }
            other if unsupported_sprint(other).is_some() => {
                let sprint = unsupported_sprint(other).unwrap();
                ctx.diags.push(Diagnostic::new(
                    &ctx.file.clone(),
                    child.name_line,
                    child.name_column,
                    "E_UNSUPPORTED",
                    format!("element `{other}` is valid notation but needs Sprint {sprint}; it cannot render yet"),
                    format!("remove `{other}` or wait for Sprint {sprint}"),
                ));
            }
            other => {
                ctx.push(
                    child.name_line,
                    child.name_column,
                    "E_UNKNOWN_ELEMENT",
                    format!("unknown element `{other}`"),
                    "use `text`, `font`, `video`, `box`, `flex`, `image`, `path`, `svg`, `audio`, `style`, `tween`, `key`, `set`, `mark`, `input`, `variable`, or `replicate`, or remove the element",
                );
            }
        }
    }

    PartialComposition {
        root,
        name,
        comp_id,
        canvas_name,
        width,
        height,
        duration_omitted,
        duration_written,
        fps,
        background,
        version,
        clips,
        fonts,
        pending_stills,
        motion_nodes,
        key_nodes,
        marks,
        inputs,
        variables,
        replicates,
        autoplay: autoplay.unwrap_or(false),
        normalize,
        normalize_loc,
    }
}

/// Resolve one composition's windows, timing, and motion (Sprint 6
/// T006): relative starts, duration inference, tiny-length refusals,
/// inferred-length echoes, and motion directives for exactly the clips
/// in `partial`.
fn finish_composition(mut ctx: &mut Ctx, partial: PartialComposition) -> Composition {
    let PartialComposition {
        root,
        name,
        comp_id,
        canvas_name,
        width,
        height,
        duration_omitted,
        duration_written,
        fps,
        background,
        version,
        mut clips,
        fonts,
        pending_stills,
        motion_nodes,
        key_nodes,
        marks,
        inputs,
        variables,
        replicates,
        autoplay,
        normalize,
        normalize_loc,
    } = partial;

    // --- relative starts plus duration inference (T002 + T003) -------
    // Each `start-after` clip lands exactly at its referenced effective
    // end plus its offset; cycles and before-zero results refuse here.
    // Unknown, self, and dropped targets stay silent (already reported
    // above) so one bad value yields one diagnostic, never a cascade.
    // An omitted composition `duration` resolves to the latest finite
    // clip end (relative chains included); an omitted still `duration`
    // resolves to the composition end minus its start. The three feed
    // each other — a resolved start moves a finite end, which moves the
    // composition end, which stretches a still that another clip may
    // follow — so resolution runs as a small fixpoint. Every round can
    // only move a window from unknown to known (a default start to a
    // resolved one, a `NAN` still to a finite length, an unknown
    // composition end to the latest finite end), so values converge and
    // the loop stops when a round changes nothing (cap 8 for safety).
    // Re-runs never double-report thanks to the identical-diagnostic
    // guard in `Ctx::push`.
    let mut comp_end: Option<f64> = duration_written;
    for _ in 0..8 {
        // The composition end joins the convergence state: inferring it
        // is itself progress a later round builds on (a `start-after`
        // naming the composition resolves only once it is known).
        let before = (
            comp_end.map(f64::to_bits),
            clips
                .iter()
                .map(|c| (c.start().to_bits(), c.duration().to_bits()))
                .collect::<Vec<_>>(),
        );
        resolve_relative_starts(
            &mut ctx,
            &mut clips,
            &name,
            comp_id.as_deref(),
            comp_end,
            &marks,
        );
        if duration_omitted && comp_end.is_none() {
            let mut best: Option<f64> = None;
            for c in &clips {
                let d = c.duration();
                if d.is_finite() {
                    let e = c.start() + d;
                    if e.is_finite() {
                        best = Some(best.map_or(e, |b: f64| b.max(e)));
                    }
                }
            }
            match best {
                Some(e) if e > 0.0 => comp_end = Some(e),
                _ => {
                    ctx.push(
                        root.name_line,
                        root.name_column,
                        "E_BAD_DURATION",
                        "composition omits `duration` but no finite clip end bounds it, so no length can be inferred",
                        "add `duration=<seconds>` to the composition, or write `duration=` on the clips so the composition can end at the latest clip end",
                    );
                }
            }
        }
        if let Some(end) = comp_end {
            for c in clips.iter_mut() {
                if c.duration().is_nan() {
                    c.set_duration(end - c.start());
                }
            }
        }
        let settled = (
            comp_end.map(f64::to_bits),
            clips
                .iter()
                .map(|c| (c.start().to_bits(), c.duration().to_bits()))
                .collect::<Vec<_>>(),
        ) == before;
        if settled {
            break;
        }
    }

    // --- mark range refusals (Sprint 7 motion, CAP-003) ---------------
    // A mark past the composition end names no instant a `start-after`
    // can follow into visible time, so it fails naming the instant and
    // the bound. Without a known composition end the composition
    // diagnostic above already speaks (no cascade).
    if let Some(end) = comp_end {
        for m in &marks {
            if m.at > end {
                ctx.push(
                    m.line,
                    m.column,
                    "E_BAD_TIMING",
                    format!(
                        "mark {:?} at {}s is outside composition time (composition runs 0 to {}s); a `start-after` naming it would start past the end",
                        m.name, m.at, end
                    ),
                    "move the mark inside the composition (`at=<seconds up to the duration>`), or lengthen the composition",
                );
            }
        }
    }

    // --- tiny-length refusals (T003, REC-005) --------------------------
    // Any written or resolved length below 1/60s fails naming the value
    // and the bound. Pending stills with no composition end to extend
    // to stay silent here: the composition diagnostic above already
    // names the one missing bound (no cascade).
    for c in &clips {
        let d = c.duration();
        if d.is_finite() && d < MIN_DURATION_SECS {
            let (line, col) = c.line_col();
            ctx.push(
                line,
                col,
                "E_BAD_TIMING",
                format!(
                    "{} duration {d}s is below the 1/60s minimum length (about {MIN_DURATION_SECS:.4}s); lengths this short have no frame to show",
                    c.what(),
                ),
                "lengthen the clip to at least 1/60s, or remove it",
            );
        }
    }
    if let Some(e) = comp_end {
        if e < MIN_DURATION_SECS {
            let (line, col) = find(root, "duration")
                .map(|v| (v.line, v.column))
                .unwrap_or((root.name_line, root.name_column));
            ctx.push(
                line,
                col,
                "E_BAD_DURATION",
                format!(
                    "composition duration {e}s is below the 1/60s minimum length (about {MIN_DURATION_SECS:.4}s); lengths this short have no frame to show"
                ),
                "lengthen the composition to at least 1/60s",
            );
        }
    }

    // --- reviewable inferred lengths (T003) ----------------------------
    // Every resolved omission lands in `check` output: stills that ran
    // until the composition end, and the composition when it ended at
    // the latest finite clip end. (Audio and video inference echo from
    // their own validators; pending stills with no composition end
    // echo nothing — the refusal above speaks for them.)
    for &i in &pending_stills {
        let d = clips[i].duration();
        if d.is_finite() {
            let (line, col) = clips[i].line_col();
            let kind = match &clips[i] {
                Clip::Text(_) => "text",
                Clip::Box(_) => "box",
                Clip::Flex(_) => "flex",
                Clip::Image(_) => "image",
                Clip::Path(_) => "path",
                Clip::Svg(_) => "svg",
                _ => "still",
            };
            let end = comp_end.unwrap_or(d + clips[i].start());
            ctx.inferred.push(InferredLength {
                kind,
                label: None,
                line,
                column: col,
                duration: d,
                detail: format!("until composition end {end:.3}s"),
            });
        }
    }
    if duration_omitted {
        if let Some(e) = comp_end {
            ctx.inferred.push(InferredLength {
                kind: "composition",
                label: Some(name.clone()),
                line: root.name_line,
                column: root.name_column,
                duration: e,
                detail: "until the latest finite clip end".to_string(),
            });
        }
    }

    // --- motion directives (T003 plus Sprint 7 keys and expressions) ---
    let input_names: Vec<&str> = inputs.iter().map(|d| d.name.as_str()).collect();
    let input_values: Vec<f64> = inputs.iter().map(|d| d.value).collect();
    let mut tweens: Vec<TweenDecl> = Vec::new();
    let mut sets: Vec<SetDecl> = Vec::new();
    for (is_tween, node) in motion_nodes {
        if is_tween {
            if let Some(t) = validate_tween(
                &mut ctx,
                node,
                &clips,
                &input_names,
                &input_values,
                fps,
                &replicates,
            ) {
                tweens.push(t);
            }
        } else if let Some(s) = validate_set(&mut ctx, node, &clips) {
            sets.push(s);
        }
    }
    let mut keys: Vec<KeyDecl> = Vec::new();
    for node in key_nodes {
        if let Some(k) = validate_key(&mut ctx, node, &clips) {
            keys.push(k);
        }
    }
    // --- stagger overflow (Sprint 7 motion, CAP-002) -------------------
    // Split pieces inherit the target clip window: a staggered piece
    // running past the window end is cut, and the cut is reported with
    // the numbers, never as a diagnostic.
    for t in &tweens {
        stagger_overflow(
            &mut ctx,
            &clips,
            "tween",
            t.target,
            t.line,
            t.column,
            t.stagger,
            t.stagger_from,
            t.stagger_grid,
            t.start,
            t.duration,
            t.repeat,
            t.repeat_delay,
        );
    }
    for k in &keys {
        let first = k.stops.first().map(|s| s.at).unwrap_or(0.0);
        let last = k.stops.last().map(|s| s.at).unwrap_or(0.0);
        stagger_overflow(
            &mut ctx,
            &clips,
            "key",
            k.target,
            k.line,
            k.column,
            k.stagger,
            k.stagger_from,
            k.stagger_grid,
            first,
            (last - first).max(0.0),
            k.repeat,
            k.repeat_delay,
        );
    }

    Composition {
        name,
        id: comp_id,
        canvas: canvas_name,
        width: width.unwrap_or(0.0) as u32,
        height: height.unwrap_or(0.0) as u32,
        duration: comp_end.unwrap_or(0.0),
        fps,
        background,
        version,
        clips,
        fonts,
        tweens,
        sets,
        keys,
        marks,
        inputs,
        variables,
        replicates,
        autoplay,
        normalize,
        normalize_loc,
    }
}

fn finish(
    ctx: Ctx<'_>,
    composition: Option<Composition>,
    compositions: Vec<Composition>,
    root_name: Option<String>,
    ffmpeg_auto_download: bool,
) -> Verdict {
    let status = if ctx.invalid {
        Status::Invalid
    } else if ctx.diags.iter().any(|d| d.code == "E_UNSUPPORTED") {
        Status::Unsupported
    } else {
        Status::Valid
    };
    Verdict {
        status,
        composition: if status == Status::Invalid {
            None
        } else {
            composition
        },
        compositions: if status == Status::Invalid {
            Vec::new()
        } else {
            compositions
        },
        root_name: if status == Status::Invalid {
            None
        } else {
            root_name
        },
        diagnostics: ctx.diags,
        variable_warnings: ctx.override_warnings,
        ffmpeg_auto_download,
        footage: ctx.footage,
        inferred: ctx.inferred,
        stagger_clamps: ctx.stagger_clamps,
        crossfade_clamps: ctx.crossfade_clamps,
        effect_tails: ctx.effect_tails,
        silences: ctx.silences,
        waveform_clamps: ctx.waveform_clamps,
    }
}

/// Shared absolute start/duration check (T005). Returns the window on
/// success; every missing/negative/zero/relative shape is a diagnostic.
/// Motion directives keep this strict form; clips resolve starts and
/// durations through the relaxed helpers below.
fn clip_window(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<(f64, f64)> {
    clip_window_inner(ctx, node, what)
}

fn clip_window_inner(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<(f64, f64)> {
    let mut bad = false;
    // `end=`-derived durations stay invalid in Sprint 1.
    if let Some(v) = find(node, "end") {
        ctx.push(
            v.line,
            v.column,
            "E_BAD_TIMING",
            format!("{what} uses `end=`; end-derived durations are not part of Sprint 1"),
            "declare an absolute `start=` and a written `duration=` instead",
        );
        bad = true;
    }
    let start = match find(node, "start") {
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => x,
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} start {x} is invalid; starts are absolute seconds >= 0"),
                    "set `start=` to an absolute non-negative time in seconds",
                );
                bad = true;
                0.0
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} start is not an absolute time; relative starts are not part of Sprint 1"),
                    "set `start=` to an absolute non-negative time in seconds",
                );
                bad = true;
                0.0
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_TIMING",
                format!("{what} is missing required `start`; every clip needs an absolute start"),
                "add `start=<seconds>` to the clip",
            );
            bad = true;
            0.0
        }
    };
    let duration = match find(node, "duration") {
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x > 0.0 => x,
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} duration {x} is invalid; durations must be positive"),
                    "set `duration=` to a positive time in seconds",
                );
                bad = true;
                0.0
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} duration is not a plain duration in seconds"),
                    "set `duration=` to a positive time in seconds",
                );
                bad = true;
                0.0
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_TIMING",
                format!(
                    "{what} is missing required `duration`; every clip needs a written duration"
                ),
                "add `duration=<seconds>` to the clip",
            );
            bad = true;
            0.0
        }
    };
    if bad {
        return None;
    }
    Some((start, duration))
}

/// Shared absolute start check (Sprint 5 T009): the `start` half of
/// [`clip_window`] for clips whose `duration` is optional. Returns the
/// start on success; every missing/negative/relative shape is a
/// diagnostic.
fn clip_start(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    if let Some(v) = find(node, "end") {
        ctx.push(
            v.line,
            v.column,
            "E_BAD_TIMING",
            format!("{what} uses `end=`; end-derived durations are not part of Sprint 1"),
            "declare an absolute `start=` and a written `duration=` instead",
        );
        return None;
    }
    match find(node, "start") {
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(x),
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} start {x} is invalid; starts are absolute seconds >= 0"),
                    "set `start=` to an absolute non-negative time in seconds",
                );
                None
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} start is not an absolute time; relative starts are not part of Sprint 1"),
                    "set `start=` to an absolute non-negative time in seconds",
                );
                None
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_TIMING",
                format!("{what} is missing required `start`; every clip needs an absolute start"),
                "add `start=<seconds>` to the clip",
            );
            None
        }
    }
}

/// Absolute start with Sprint 6 relative-start tolerance (T001): a
/// clip declaring `start-after` (either spelling) may omit `start`;
/// the start defaults to 0 until T002 resolves the real relative
/// window. `end=` stays refused.
fn clip_start_relaxed(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    if find_either(node, "start-after", "start_after").is_some() && find(node, "start").is_none() {
        if let Some(v) = find(node, "end") {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_TIMING",
                format!("{what} uses `end=`; end-derived durations are not part of Sprint 1"),
                "declare an absolute `start=` and a written `duration=` instead",
            );
            return None;
        }
        return Some(0.0);
    }
    clip_start(ctx, node, what)
}

/// Optional clip `duration` (Sprint 5 T009 for `video`, Sprint 6 T003
/// for every other clip): a written value must be a positive number of
/// seconds; an omitted key means the length is inferred later (footage
/// window for video and audio, composition end for stills). Returns
/// None on a parse failure after reporting it, `Some(None)` when
/// omitted, `Some(Some(d))` when written. A present `end=` key is
/// refused here so it keeps the [`clip_window`] wording.
fn clip_optional_duration(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<Option<f64>> {
    if let Some(v) = find(node, "end") {
        ctx.push(
            v.line,
            v.column,
            "E_BAD_TIMING",
            format!("{what} uses `end=`; end-derived durations are not part of Sprint 1"),
            "declare an absolute `start=` and a written `duration=` instead",
        );
        return None;
    }
    match find(node, "duration") {
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x > 0.0 => Some(Some(x)),
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} duration {x} is invalid; durations must be positive"),
                    "set `duration=` to a positive time in seconds",
                );
                None
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} duration is not a plain duration in seconds"),
                    "set `duration=` to a positive time in seconds",
                );
                None
            }
        },
        None => Some(None),
    }
}

/// Explicit paint order: a bare integer (negative allowed); anything
/// else is a validation error, never a guessed arrangement.
fn parse_order(ctx: &mut Ctx, node: &RawNode) -> Option<i64> {
    match find(node, "order") {
        None => Some(0),
        Some(v) => match &v.kind {
            ValueKind::Int(n) => Some(*n),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ORDER",
                    "paint `order` must be a whole number; ties fall back to document order",
                    "set `order=<integer>` (for example `order=1`), or remove the key",
                );
                None
            }
        },
    }
}

/// A declared spacing value (`line-height` or `letter-spacing`, hyphen
/// or underscore spelling): absent means the font default, present must
/// be a non-negative finite number. Zero letter spacing is default
/// advance. Anything else is a validation error.
fn parse_spacing(
    ctx: &mut Ctx,
    node: &RawNode,
    hyphen: &str,
    underscore: &str,
) -> Option<Option<f64>> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(None),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(Some(x)),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_SPACING",
                    format!("text `{hyphen}` must be a non-negative number of canvas pixels"),
                    format!("set `{hyphen}=<pixels >= 0>` or remove the key"),
                );
                None
            }
        },
    }
}

/// Validated Sprint 6 clip identity (T001/T004) plus relative start
/// (T002): the `id`, `track`, and `label` of one clip, with the `id`
/// registered in the project namespace, and the `start-after` plus
/// `start-offset` reference recorded for the project-wide existence
/// check and window resolution.
struct ClipIdentity {
    id: Option<String>,
    track: TrackKind,
    label: Option<String>,
    start_after: Option<StartAfter>,
    hidden: bool,
}

/// Parse one `start-offset`/`start_offset` value (T002): absent means
/// `0`; present must be a finite number of seconds (negative overlaps
/// earlier content). A non-numeric offset fails with `E_BAD_TIMING`,
/// never guessed to zero.
fn parse_start_offset(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    match find_either(node, "start-offset", "start_offset") {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!(
                        "{what} `start-offset` must be a number of seconds (negative overlaps earlier content); found {}",
                        raw_text(v)
                    ),
                    "set `start-offset=<seconds>` (for example `start-offset=0.5` or `start-offset=-1`), or remove the key",
                );
                None
            }
        },
    }
}

/// Shared Sprint 6 clip identity parse (T001/T002/T004): `id`,
/// `track`, and `label` validate independently, so each bad value
/// reports its own diagnostic; any failure drops the clip with nothing
/// registered. On success a declared id joins the project namespace
/// and a `start-after` reference joins the project-wide existence
/// check. An absolute `start` together with `start-after` is refused
/// as ambiguous (exactly one diagnostic, no reference recorded, so
/// nothing cascades); a lone `start-offset` without `start-after` is
/// refused the same way.
fn parse_clip_identity(
    ctx: &mut Ctx,
    node: &RawNode,
    element: &str,
    what: &str,
) -> Option<ClipIdentity> {
    let id = parse_clip_id(ctx, node, what);
    let track = parse_track(ctx, node, element, what);
    let label = parse_label(ctx, node, what);
    let hidden = parse_hidden(ctx, node, what);
    let start_after_raw = parse_ref(ctx, node, "start-after", what);
    let (id, track, label, hidden, start_after_raw) =
        match (id, track, label, hidden, start_after_raw) {
            (Some(id), Some(track), Some(label), Some(hidden), Some(start_after)) => {
                (id, track, label, hidden, start_after)
            }
            _ => return None,
        };
    let has_relative = start_after_raw.is_some();
    let has_absolute = find(node, "start").is_some();
    let has_offset = find_either(node, "start-offset", "start_offset").is_some();
    if has_relative && has_absolute {
        let (target, line, column) = start_after_raw.unwrap();
        let _ = target;
        ctx.push(
            line,
            column,
            "E_BAD_TIMING",
            format!(
                "{what} declares both an absolute `start` and a relative `start-after`; one clip cannot use both"
            ),
            "remove `start=` to follow the referenced end, or remove `start-after=` (and `start-offset=`) to keep the absolute time",
        );
        return None;
    }
    if !has_relative && has_offset {
        let v = find_either(node, "start-offset", "start_offset").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_TIMING",
            format!("{what} declares `start-offset` without `start-after`; an offset needs a referenced end"),
            "add `start-after=\"<id>\"` naming the clip to follow, or remove the `start-offset` key",
        );
        return None;
    }
    let offset = parse_start_offset(ctx, node, what)?;
    if let Some(id) = &id {
        ctx.ids.push(IdEntry {
            value: id.clone(),
            line: node.name_line,
            column: node.name_column,
            is_composition: false,
            is_mark: false,
            comp: ctx.current_comp,
        });
    }
    let start_after = match start_after_raw {
        None => None,
        Some((target, line, column)) => {
            ctx.refs.push(RefEntry {
                kind: "start-after",
                target: target.clone(),
                line,
                column,
                owner_id: id.clone(),
                owner_line: node.name_line,
                needs_composition: false,
                comp: ctx.current_comp,
            });
            Some(StartAfter {
                target,
                offset,
                line,
                column,
            })
        }
    };
    Some(ClipIdentity {
        id,
        track,
        label,
        start_after,
        hidden,
    })
}

/// Text-effect keys (Sprint 7 motion, CAP-009/REC-004) that only
/// `text` clips may carry. Every other clip kind lists these as known
/// keys and refuses them with `E_BAD_MOTION` naming the input, so a
/// split on non-text fails loudly instead of as an unknown key.
const TEXT_FX_KEYS: &[&str] = &[
    "split",
    "typewriter",
    "count-from",
    "count_from",
    "count-to",
    "count_to",
    "decimals",
];

/// Refuse text-effect keys on a non-text clip (Sprint 7 motion,
/// REC-004): `split`, `typewriter`, and the count keys apply to text
/// clips only. Returns true when at least one is present (each naming
/// its own diagnostic); the caller drops the clip.
fn reject_text_fx(ctx: &mut Ctx, node: &RawNode, what: &str) -> bool {
    let mut found = false;
    for key in TEXT_FX_KEYS {
        if let Some(v) = find(node, key) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("{what} carries `{key}`, but split, typewriter, and count effects apply to text clips only"),
                "move the effect to a `text` clip, or remove the key",
            );
            found = true;
        }
    }
    found
}

/// Parsed Sprint 7 text effects of one `text` clip (CAP-009).
struct TextFx {
    split: Option<notation::model::SplitMode>,
    typewriter: bool,
    count_from: Option<f64>,
    count_to: Option<f64>,
    decimals: u32,
}

/// Parse `split` / `typewriter` / `count-from` / `count-to` /
/// `decimals` of a `text` clip (Sprint 7 motion, CAP-009/REC-004):
/// each split mode addresses pieces individually; `typewriter`
/// reveals them across the window; the count range renders the
/// interpolated number. Splits with unknown modes, a typewriter
/// without a split, half-written or contradictory counts, and
/// `decimals` without a count all fail with `E_BAD_MOTION`.
fn parse_text_fx(ctx: &mut Ctx, node: &RawNode) -> Option<TextFx> {
    let mut ok = true;
    let split = match find(node, "split") {
        None => None,
        Some(v) => match as_str(v).and_then(notation::model::SplitMode::parse) {
            Some(m) => Some(m),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "text `split` {} is not a split mode; accepted modes are: {}",
                        raw_text(v),
                        notation::model::SplitMode::ACCEPTED.join(", ")
                    ),
                    "set `split=\"chars\"`, `split=\"words\"`, or `split=\"lines\"`, or remove the key",
                );
                ok = false;
                None
            }
        },
    };
    let typewriter = match find(node, "typewriter") {
        None => false,
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => *b,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("text `typewriter` must be `true` or `false`; found {}", raw_text(v)),
                    "set `typewriter=true` to reveal split pieces across the window, or remove the key",
                );
                ok = false;
                false
            }
        },
    };
    let mut count_opt = |hyphen: &str, underscore: &str| -> Option<Option<f64>> {
        match find(node, hyphen).or_else(|| find(node, underscore)) {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => Some(Some(x)),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_MOTION",
                        format!(
                            "text `{hyphen}` must be a finite number; found {}",
                            raw_text(v)
                        ),
                        format!("set `{hyphen}=<number>` or remove the key"),
                    );
                    ok = false;
                    Some(None)
                }
            },
        }
    };
    let count_from = count_opt("count-from", "count_from")?;
    let count_to = count_opt("count-to", "count_to")?;
    let decimals = match find(node, "decimals") {
        None => 0,
        Some(v) => match as_int(v) {
            Some(n) if n >= 0 => n as u32,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "text `decimals` must be a whole number of decimal places >= 0; found {}",
                        raw_text(v)
                    ),
                    "set `decimals=<places from 0>` or remove the key",
                );
                ok = false;
                0
            }
        },
    };
    // Contradictory modes (REC-004): a typewriter reveals split
    // pieces, so it needs a split; counting numbers contradicts
    // splitting pieces and revealing them; `decimals` alone counts
    // nothing.
    if typewriter && split.is_none() {
        let v = find(node, "typewriter").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            "text `typewriter=true` needs a `split`; the reveal shows split pieces across the window",
            "add `split=\"chars\"`, `split=\"words\"`, or `split=\"lines\"`, or set `typewriter=false`",
        );
        ok = false;
    }
    let counting = count_from.is_some() || count_to.is_some();
    if count_from.is_some() != count_to.is_some() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_MOTION",
            "text count needs both `count-from` and `count-to`; a half-written range counts nothing",
            "add the missing end of the range (`count-from=<number>` and `count-to=<number>`), or remove both keys",
        );
        ok = false;
    }
    if counting && split.is_some() {
        let v = find(node, "split").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            "text cannot both `split` into pieces and count from `count-from` to `count-to`; the modes contradict",
            "keep `split` (with optional `typewriter`) for piece reveals, or keep the count range for numbers",
        );
        ok = false;
    }
    if counting && typewriter {
        let v = find(node, "typewriter").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            "text cannot both `typewriter` pieces and count from `count-from` to `count-to`; the modes contradict",
            "keep `typewriter` with a `split` for piece reveals, or keep the count range for numbers",
        );
        ok = false;
    }
    if !counting && find(node, "decimals").is_some() {
        let v = find(node, "decimals").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            "text `decimals` needs a count range; decimal places format the counted number",
            "add `count-from=<number>` and `count-to=<number>`, or remove `decimals`",
        );
        ok = false;
    }
    if !ok {
        return None;
    }
    Some(TextFx {
        split,
        typewriter,
        count_from,
        count_to,
        decimals,
    })
}

/// Refuse `span` children on a non-text clip (Sprint 8 look,
/// CAP-002/REC-002): styled runs live in `text` clips only. Returns
/// true when at least one is present; the caller drops the clip.
fn reject_spans(ctx: &mut Ctx, node: &RawNode, what: &str) -> bool {
    let mut found = false;
    for child in &node.children {
        if child.name == "span" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_TEXT",
                format!("{what} holds a `span`; styled runs live in `text` clips only"),
                "move the `span` into a `text` clip, or remove it",
            );
            found = true;
        }
    }
    found
}

/// Span attributes (Sprint 8 look, CAP-002/T002): the `value` text
/// plus every per-run style override, in both hyphen and underscore
/// spellings. `direction` is deliberately absent: the clip's base
/// direction orders every run, so a span direction would contradict it.
const SPAN_KEYS: &[&str] = &[
    "value",
    "font-family",
    "font_family",
    "font-size",
    "font_size",
    "font-weight",
    "font_weight",
    "italic",
    "color",
    "line-height",
    "line_height",
    "letter-spacing",
    "letter_spacing",
];

/// One span `font-family` list (Sprint 8 look, CAP-002/T002): the same
/// comma try-order syntax as the clip key. An empty or unquoted list
/// fails with `E_BAD_FONT` naming the span.
fn span_font_family(
    ctx: &mut Ctx,
    node: &RawNode,
    label: &str,
) -> Option<Option<Vec<String>>> {
    match find(node, "font-family").or_else(|| find(node, "font_family")) {
        None => Some(None),
        Some(v) => match as_str(v) {
            Some(s) => {
                let names: Vec<String> = s
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect();
                if names.is_empty() {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FONT",
                        format!("text {label} `font-family` names no family"),
                        format!(
                            "set {label} `font-family=\"Family Name\"` or `font-family=\"First, Fallback\"`, or remove the key to inherit the enclosing style"
                        ),
                    );
                    None
                } else {
                    Some(Some(names))
                }
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FONT",
                    format!("text {label} `font-family` must be a quoted family list"),
                    format!(
                        "set {label} `font-family=\"Family Name\"` or `font-family=\"First, Fallback\"`, or remove the key to inherit the enclosing style"
                    ),
                );
                None
            }
        },
    }
}

/// One span spacing value (`line-height` or `letter-spacing`):
/// absent inherits the enclosing style, present must be a
/// non-negative finite number. Anything else fails with
/// `E_BAD_SPACING` naming the span.
fn span_spacing(
    ctx: &mut Ctx,
    node: &RawNode,
    hyphen: &str,
    underscore: &str,
    label: &str,
) -> Option<Option<f64>> {
    match find(node, hyphen).or_else(|| find(node, underscore)) {
        None => Some(None),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(Some(x)),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_SPACING",
                    format!(
                        "text {label} `{hyphen}` must be a non-negative number of canvas pixels"
                    ),
                    format!(
                        "set {label} `{hyphen}=<pixels >= 0>` or remove the key to inherit the enclosing style"
                    ),
                );
                None
            }
        },
    }
}

/// One `span` child of a `text` clip (Sprint 8 look, CAP-002/T002):
/// the run text with its style overrides plus nested `span`
/// children. A span with neither text nor nested spans fails naming
/// the span; a non-`span` child of a span fails the same way. Every
/// refusal names the span and the property with a concrete fix.
fn validate_text_span(ctx: &mut Ctx, node: &RawNode, label: &str) -> Option<TextSpan> {
    for (k, v) in &node.props {
        if !SPAN_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("text {label} carries unknown key {k:?}"),
                format!(
                    "remove the key from text {label} or check the spelling against `value`, `font-family`, `font-size`, `font-weight`, `italic`, `color`, `line-height`, and `letter-spacing`"
                ),
            );
        }
    }
    let value = match find(node, "value") {
        None => String::new(),
        Some(v) => match as_str(v) {
            Some(s) => s.to_string(),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TEXT",
                    format!("text {label} `value` must be quoted text"),
                    format!("set {label} `value=\"...\"` or remove the key"),
                );
                return None;
            }
        },
    };
    let font_family = span_font_family(ctx, node, label)?;
    let font_size = match find(node, "font-size").or_else(|| find(node, "font_size")) {
        None => None,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x > 0.0 => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TEXT",
                    format!(
                        "text {label} `font-size` must be a positive number of canvas pixels; found {}",
                        raw_text(v)
                    ),
                    format!("set {label} `font-size=<positive pixels>` or remove the key to inherit the enclosing size"),
                );
                return None;
            }
        },
    };
    let font_weight = match find(node, "font-weight").or_else(|| find(node, "font_weight")) {
        None => None,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && (100.0..=900.0).contains(&x) => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TEXT",
                    format!(
                        "text {label} `font-weight` {} is outside the accepted range 100 to 900",
                        raw_text(v)
                    ),
                    format!(
                        "set {label} `font-weight=<100 to 900>` (400 is regular, 700 is bold) or remove the key to inherit the enclosing weight"
                    ),
                );
                return None;
            }
        },
    };
    let italic = match find(node, "italic") {
        None => None,
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TEXT",
                    format!(
                        "text {label} `italic` must be `true` or `false`; found {}",
                        raw_text(v)
                    ),
                    format!("set {label} `italic=true` for italics, `italic=false` (or remove the key) for upright"),
                );
                return None;
            }
        },
    };
    let color = match find(node, "color") {
        None => None,
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => Some(c),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    format!("text {label} `color` is not a `#rgb` or `#rrggbb` color"),
                    format!("set {label} `color=\"#ffffff\"` or remove the key to inherit the enclosing color"),
                );
                return None;
            }
        },
    };
    let line_height = span_spacing(ctx, node, "line-height", "line_height", label)?;
    let letter_spacing = span_spacing(ctx, node, "letter-spacing", "letter_spacing", label)?;
    let mut children = Vec::with_capacity(node.children.len());
    let mut ok = true;
    for (i, child) in node.children.iter().enumerate() {
        if child.name != "span" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_TEXT",
                format!(
                    "text {label} holds `{}`; spans hold text and nested `span` runs only",
                    child.name
                ),
                format!("use `span value=\"...\"` children inside {label}, or remove the element"),
            );
            ok = false;
            continue;
        }
        match validate_text_span(ctx, child, &format!("{label}.{}", i + 1)) {
            Some(span) => children.push(span),
            None => ok = false,
        }
    }
    if !ok {
        return None;
    }
    if value.is_empty() && children.is_empty() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_TEXT",
            format!("text {label} carries no text; a span paints its `value`, its nested `span` children, or both"),
            format!("add `value=\"...\"` to {label} or nest `span` children inside it"),
        );
        return None;
    }
    Some(TextSpan {
        value,
        font_family,
        font_size,
        font_weight,
        italic,
        color,
        letter_spacing,
        line_height,
        children,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Every `span` child of a `text` clip (Sprint 8 look, CAP-002/T002):
/// `span` runs in document order. A non-`span` child fails with
/// `E_BAD_TEXT` naming the styled runs; each span validates
/// independently so every span problem reports at once.
fn parse_text_spans(ctx: &mut Ctx, node: &RawNode) -> Option<Vec<TextSpan>> {
    let mut spans = Vec::with_capacity(node.children.len());
    let mut ok = true;
    for (i, child) in node.children.iter().enumerate() {
        if child.name != "span" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_TEXT",
                format!(
                    "text clip holds `{}`; styled runs are `span` children",
                    child.name
                ),
                "write `span value=\"...\"` children inside the `text` clip, or remove the element",
            );
            ok = false;
            continue;
        }
        match validate_text_span(ctx, child, &format!("span {}", i + 1)) {
            Some(span) => spans.push(span),
            None => ok = false,
        }
    }
    if ok { Some(spans) } else { None }
}

fn validate_text(ctx: &mut Ctx, node: &RawNode, styles: &[StyleDecl]) -> Option<TextClip> {
    let merged = styled_node(ctx, node, "text clip", styles)?;
    let node = merged.as_ref();
    const KEYS: &[&str] = &[
        "value",
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "x",
        "y",
        "size",
        "color",
        "position",
        "align",
        "wrap",
        "font-style",
        "font-family",
        "font_family",
        "direction",
        "order",
        "line-height",
        "line_height",
        "letter-spacing",
        "letter_spacing",
        "split",
        "typewriter",
        "count-from",
        "count_from",
        "count-to",
        "count_to",
        "decimals",
        "rotation-x",
        "rotation_x",
        "rotate-x",
        "rotate_x",
        "rotation-y",
        "rotation_y",
        "rotate-y",
        "rotate_y",
        "perspective",
        "origin",
        "filter",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown text key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let identity = parse_clip_identity(ctx, node, "text", "text clip")?;
    // Sprint 6 T003: `start` resolves like every clip (absolute, or
    // `start-after` until T002 lands it), but `duration` is optional —
    // an omitted one runs until the composition end. The `NAN`
    // sentinel marks the omission for the inference loop; it never
    // survives validation (a valid verdict carries finite lengths).
    // Start and duration report independently (like the old shared
    // window check): a clip missing both names both problems at once.
    let start = clip_start_relaxed(ctx, node, "text clip");
    let duration_parsed = clip_optional_duration(ctx, node, "text clip");
    let (start, duration) = match (start, duration_parsed) {
        (Some(s), Some(Some(d))) => (s, d),
        (Some(s), Some(None)) => (s, f64::NAN),
        _ => return None,
    };
    // Styled runs (Sprint 8 look, CAP-002/T002): `span` children
    // paint in order after the clip `value` (which stays a base-style
    // run when both are written). A clip with spans may omit `value`;
    // a clip without spans keeps the required `value`, exactly as
    // before this task.
    let spans = parse_text_spans(ctx, node)?;
    let value = match find(node, "value") {
        Some(v) => match as_str(v) {
            Some(s) => s.to_string(),
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_MISSING_FIELD",
                    "text clip is missing required `value`",
                    "add `value=\"...\"` to the text clip",
                );
                return None;
            }
        },
        None => {
            if spans.is_empty() {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_MISSING_FIELD",
                    "text clip is missing required `value`",
                    "add `value=\"...\"` to the text clip",
                );
                return None;
            }
            String::new()
        }
    };
    // Plain helper (not a closure) so `ctx` stays freely reborrowable.
    fn num_prop(ctx: &mut Ctx, node: &RawNode, key: &str, default: f64) -> f64 {
        match find(node, key) {
            None => default,
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => x,
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `{key}` must be a number"),
                        format!("set `{key}=<number>`"),
                    );
                    default
                }
            },
        }
    }
    let (x, y, size) = (
        num_prop(ctx, node, "x", 0.0),
        num_prop(ctx, node, "y", 0.0),
        num_prop(ctx, node, "size", 48.0),
    );
    if !size.is_finite() || size <= 0.0 {
        let (line, col) = find(node, "size")
            .map(|v| (v.line, v.column))
            .unwrap_or((node.name_line, node.name_column));
        ctx.push(
            line,
            col,
            "E_BAD_TIMING",
            format!("text `size` {size} is invalid; sizes must be positive"),
            "set `size=<positive number>` on the text clip",
        );
        return None;
    }
    let color = match find(node, "color") {
        None => notation::model::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    "text color is not a `#rgb` or `#rrggbb` color",
                    "use a hex color like `color=\"#ffffff\"`",
                );
                return None;
            }
        },
    };
    // Optional placement sanity: negative placement is suspicious input.
    for key in ["x", "y"] {
        if let Some(v) = find(node, key).and_then(as_f64) {
            if v < 0.0 {
                let loc = find(node, key).unwrap();
                ctx.push(
                    loc.line,
                    loc.column,
                    "E_BAD_TIMING",
                    format!("text `{key}` {v} places content off-canvas"),
                    format!("set `{key}` to a non-negative canvas position"),
                );
            }
        }
    }
    // Alignment: exactly `left`, `center`, or `right`.
    let align = match find(node, "align") {
        None => notation::model::Align::Left,
        Some(v) => match as_str(v).and_then(notation::model::Align::parse) {
            Some(a) => a,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `align` must be exactly `left`, `center`, or `right`",
                    "set `align=\"left\"`, `align=\"center\"`, or `align=\"right\"`",
                );
                return None;
            }
        },
    };
    // Base direction (Sprint 8 look, CAP-002/T002): exactly `ltr`,
    // `rtl`, or `auto`; absent means left to right.
    let direction = match find(node, "direction") {
        None => TextDirection::Ltr,
        Some(v) => match as_str(v).and_then(TextDirection::parse) {
            Some(d) => d,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TEXT",
                    format!(
                        "text `direction` {} is unknown; accepted directions are: {}",
                        raw_text(v),
                        TextDirection::ACCEPTED.join(", ")
                    ),
                    "set `direction=\"ltr\"`, `direction=\"rtl\"`, or `direction=\"auto\"`, or remove the key for left-to-right",
                );
                return None;
            }
        },
    };
    // Font style: exactly `regular`, `bold`, `italic`, or `bold-italic`.
    let style = match find(node, "font-style") {
        None => notation::model::FontStyle::Regular,
        Some(v) => match as_str(v).and_then(notation::model::FontStyle::parse) {
            Some(s) => s,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `font-style` must be exactly `regular`, `bold`, `italic`, or `bold-italic`",
                    "set `font-style=\"bold\"`, `font-style=\"italic\"`, or `font-style=\"bold-italic\"`",
                );
                return None;
            }
        },
    };
    // Wrapping: `off`, or a positive wrap width in canvas pixels.
    let wrap = match find(node, "wrap") {
        None => notation::model::Wrap::Off,
        Some(v) => {
            if let Some(s) = as_str(v) {
                if s == "off" {
                    notation::model::Wrap::Off
                } else if let Ok(w) = s.parse::<f64>() {
                    if w.is_finite() && w > 0.0 {
                        notation::model::Wrap::Width(w)
                    } else {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_BAD_TIMING",
                            format!("text `wrap` {s:?} is invalid; use `\"off\"` or a positive width in pixels"),
                            "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                        );
                        return None;
                    }
                } else {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `wrap` {s:?} is invalid; use `\"off\"` or a positive width in pixels"),
                        "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                    );
                    return None;
                }
            } else if let Some(w) = as_f64(v) {
                if w.is_finite() && w > 0.0 {
                    notation::model::Wrap::Width(w)
                } else {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `wrap` {w} is invalid; widths must be positive"),
                        "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                    );
                    return None;
                }
            } else {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `wrap` must be `\"off\"` or a positive width in pixels",
                    "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                );
                return None;
            }
        }
    };
    // Position: an `"X,Y"` pair feeding the `x`/`y` defaults; explicit
    // `x`/`y` win when both are present.
    if let Some(v) = find(node, "position") {
        match as_str(v) {
            Some(s) => {
                let bad = |ctx: &mut Ctx| {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `position` {s:?} is invalid; use `\"X,Y\"` canvas pixels"),
                        "set `position=\"80,320\"` or use `x=` and `y=`",
                    );
                };
                match s.split_once(',') {
                    Some((a, b)) => match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                        (Ok(px), Ok(py)) if px.is_finite() && py.is_finite() => {
                            // Accepted shape; explicit x/y override below.
                            let _ = (px, py);
                        }
                        _ => {
                            bad(ctx);
                            return None;
                        }
                    },
                    None => {
                        bad(ctx);
                        return None;
                    }
                }
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `position` must be an `\"X,Y\"` pair in canvas pixels",
                    "set `position=\"80,320\"` or use `x=` and `y=`",
                );
                return None;
            }
        }
    }
    // Resolve position defaults (explicit x/y already won above).
    let (mut px, mut py) = (x, y);
    if find(node, "x").is_none() || find(node, "y").is_none() {
        if let Some(s) = find(node, "position").and_then(as_str) {
            if let Some((a, b)) = s.split_once(',') {
                if let (Ok(vx), Ok(vy)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                    if find(node, "x").is_none() {
                        px = vx;
                    }
                    if find(node, "y").is_none() {
                        py = vy;
                    }
                }
            }
        }
    }
    let order = parse_order(ctx, node)?;
    let line_height = parse_spacing(ctx, node, "line-height", "line_height")?;
    let letter_spacing = parse_spacing(ctx, node, "letter-spacing", "letter_spacing")?;
    // Font family: CSS-like comma-separated list in try order. Only
    // syntax is validated here (a machine family may serve a name the
    // project does not declare); render refuses a list nothing resolves.
    let font_family = match find(node, "font-family").or_else(|| find(node, "font_family")) {
        None => None,
        Some(v) => match as_str(v) {
            Some(s) => {
                let names: Vec<String> = s
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect();
                if names.is_empty() {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FONT",
                        "text `font-family` names no family",
                        "set `font-family=\"Family Name\"` or `font-family=\"First, Fallback\"`, or remove the key for the project font",
                    );
                    return None;
                }
                Some(names)
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FONT",
                    "text `font-family` must be a quoted family list",
                    "set `font-family=\"Family Name\"` or `font-family=\"First, Fallback\"`, or remove the key for the project font",
                );
                return None;
            }
        },
    };
    let fx = parse_text_fx(ctx, node)?;
    // Split, typewriter, and count effects address the plain `value`
    // (Sprint 7 motion, CAP-009); combined with `span` runs the
    // addressable text is ambiguous, so the combination is refused
    // naming the runs instead of animating the wrong text.
    if !spans.is_empty()
        && (fx.split.is_some()
            || fx.typewriter
            || fx.count_from.is_some()
            || fx.count_to.is_some())
    {
        let (line, column) = find(node, "split")
            .or_else(|| find(node, "typewriter"))
            .or_else(|| find(node, "count-from"))
            .or_else(|| find(node, "count_from"))
            .or_else(|| find(node, "count-to"))
            .or_else(|| find(node, "count_to"))
            .map(|v| (v.line, v.column))
            .unwrap_or((node.name_line, node.name_column));
        ctx.push(
            line,
            column,
            "E_BAD_TEXT",
            "text clip with `span` runs carries a split, typewriter, or count effect; the effects address the plain `value`, not styled runs",
            "remove the effect keys to paint the styled runs, or remove the `span` children to animate the plain `value`",
        );
        return None;
    }
    let turn = clip_turn(ctx, node, "text clip")?;
    let filter_blur = clip_filter(ctx, node, "text clip")?;
    Some(TextClip {
        value,
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        x: px,
        y: py,
        size,
        color,
        align,
        wrap,
        style,
        order,
        line_height,
        letter_spacing,
        font_family,
        direction,
        spans,
        split: fx.split,
        typewriter: fx.typewriter,
        count_from: fx.count_from,
        count_to: fx.count_to,
        decimals: fx.decimals,
        turn,
        filter_blur,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Render one authored scalar for diagnostics (Sprint 5 T001): the
/// value the maker wrote, so refusals can name it.
fn raw_text(v: &RawValue) -> String {
    match &v.kind {
        ValueKind::Str(s) => format!("{s:?}"),
        ValueKind::Int(n) => n.to_string(),
        ValueKind::Float(x) => x.to_string(),
        ValueKind::Ratio(n, d) => format!("{n}/{d}"),
        ValueKind::Bool(b) => b.to_string(),
    }
}

/// Find one executable on PATH without shelling out (Sprint 5 T001):
/// the probe below needs `ffprobe` or `ffmpeg`, whichever the machine
/// offers. None means the source-end checks are skipped, never guessed.
fn tool_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Probe one video source length in seconds (Sprint 5 T001): `ffprobe`
/// when it is on PATH, else the `Duration:` line of `ffmpeg -i` output
/// (the same facts render probes). Anything unusable — no FFmpeg on
/// PATH, an unreadable file, an unparseable reply — is None, and the
/// caller skips only the source-end checks: declaration checks
/// (backward, zero-length, rate range) never need the probe.
fn probe_source_duration(abs: &Path) -> Option<f64> {
    if let Some(ffprobe) = tool_on_path("ffprobe") {
        if let Ok(out) = std::process::Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(abs)
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(d) = text.parse::<f64>() {
                    if d.is_finite() && d > 0.0 {
                        return Some(d);
                    }
                }
            }
        }
    }
    if let Some(ffmpeg) = tool_on_path("ffmpeg") {
        if let Ok(out) = std::process::Command::new(ffmpeg)
            .args(["-hide_banner", "-i"])
            .arg(abs)
            .output()
        {
            let text = String::from_utf8_lossy(&out.stderr);
            for line in text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("Duration:") {
                    let ts = rest.split(',').next().unwrap_or("").trim();
                    let mut parts = ts.split(':');
                    if let (Some(h), Some(m), Some(s)) = (
                        parts.next().and_then(|p| p.parse::<f64>().ok()),
                        parts.next().and_then(|p| p.parse::<f64>().ok()),
                        parts.next().and_then(|p| p.parse::<f64>().ok()),
                    ) {
                        let d = h * 3600.0 + m * 60.0 + s;
                        if d.is_finite() && d > 0.0 {
                            return Some(d);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse one Sprint 5 footage trim (`trim-start`/`trim-end`, either
/// spelling): absent means the default (0 for the start, the source
/// end for the end); present must be finite seconds from 0. Returns
/// None on a parse failure after reporting it, so callers can keep
/// collecting the clip's remaining footage problems.
fn footage_trim(
    ctx: &mut Ctx,
    node: &RawNode,
    hyphen: &str,
    underscore: &str,
) -> Option<Option<f64>> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(None),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(Some(x)),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FOOTAGE",
                    format!(
                        "video `{hyphen}` {} must be seconds into the source from 0",
                        raw_text(v)
                    ),
                    format!("set `{hyphen}=<seconds >= 0>` or remove the key"),
                );
                None
            }
        },
    }
}

/// Parse the Sprint 5 footage `rate`: absent means unity; present must
/// be a finite number from 0.1 to 10. Returns None on failure after
/// reporting it.
fn footage_rate(ctx: &mut Ctx, node: &RawNode) -> Option<f64> {
    match find(node, "rate") {
        None => Some(1.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && (0.1..=10.0).contains(&x) => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FOOTAGE",
                    format!(
                        "video `rate` {} is outside the accepted range 0.1 to 10",
                        raw_text(v)
                    ),
                    "set `rate=<0.1 to 10>` (for example `rate=2` for double speed, `rate=0.5` for half speed), or remove the key for unity rate",
                );
                None
            }
        },
    }
}

/// Parse the Sprint 5 footage `loop` flag (T003): absent means no
/// repeat (an overrunning window holds the last frame); present must
/// be a `true`/`false` literal. Returns None on failure after
/// reporting it.
fn footage_loop(ctx: &mut Ctx, node: &RawNode) -> Option<bool> {
    match find(node, "loop") {
        None => Some(false),
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FOOTAGE",
                    format!(
                        "video `loop` {} must be `true` or `false`; the window repeats only when `loop=true`",
                        raw_text(v)
                    ),
                    "set `loop=true` to repeat the window, or `loop=false` (or remove the key) to hold the last frame",
                );
                None
            }
        },
    }
}

fn validate_video(ctx: &mut Ctx, node: &RawNode) -> Option<VideoClip> {
    const KEYS: &[&str] = &[
        "path",
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "fit",
        "order",
        "volume",
        "fade-in",
        "fade_in",
        "fade-out",
        "fade_out",
        "crossfade",
        "duck-under",
        "duck_under",
        "duck-amount",
        "duck_amount",
        "delay",
        "delay-feedback",
        "delay_feedback",
        "delay-mix",
        "delay_mix",
        "reverb",
        "detect-silence",
        "detect_silence",
        "silence-threshold",
        "silence_threshold",
        "silence-min",
        "silence_min",
        "waveform",
        "waveform-from",
        "waveform_from",
        "waveform-to",
        "waveform_to",
        "trim-start",
        "trim_start",
        "trim-end",
        "trim_end",
        "rate",
        "loop",
        "rotation-x",
        "rotation_x",
        "rotate-x",
        "rotate_x",
        "rotation-y",
        "rotation_y",
        "rotate-y",
        "rotate_y",
        "perspective",
        "origin",
        "filter",
    ];
    for (k, v) in &node.props {
        if LATER_AUDIO_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("video clip `{k}` needs Sprint 10; video sound mixes volume, linear fades, crossfades, ducking, delay/reverb, `detect-silence`, `volume`/`pan` key/tween/set envelopes, and `waveform` drawing only"),
                "use `volume` with `fade-in`/`fade-out`, ride `volume`/`pan` with `key`/`tween`/`set`, or wait for Sprint 10 sound proof",
            ));
            return None;
        }
        if !KEYS.contains(&k.as_str()) && !TEXT_FX_KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown video key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    if reject_text_fx(ctx, node, "video clip") {
        return None;
    }
    if reject_spans(ctx, node, "video clip") {
        return None;
    }
    // Sprint 5 footage loop flag (T003), parsed before the clip window:
    // a looping clip without an explicit `duration` would be unbounded
    // (REC-005), so it is refused here asking for one — before the
    // inference below could misname the problem.
    let loop_opt = footage_loop(ctx, node);
    if loop_opt == Some(true) && find(node, "duration").is_none() {
        let (line, col) = find(node, "loop")
            .map(|v| (v.line, v.column))
            .unwrap_or((node.name_line, node.name_column));
        ctx.push(
            line,
            col,
            "E_BAD_FOOTAGE",
            "looping video clip has no explicit `duration`; a loop without one would be unbounded",
            "add `duration=<seconds>` to the looping video clip",
        );
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "video", "video clip")?;
    // Sprint 5 T009: `start` stays mandatory, but `duration` is now
    // optional — an omitted one is inferred from the footage window
    // once the source end is known. Sprint 6 T001: a `start-after`
    // reference lets `start` wait for T002's relative resolution.
    let start = clip_start_relaxed(ctx, node, "video clip")?;
    let duration_opt = clip_optional_duration(ctx, node, "video clip")?;
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "video clip is missing required `path`",
                "add `path=\"media/clip.mp4\"` to the video clip",
            );
            return None;
        }
    };
    // Referenced media must exist and be readable (T003, T008): a
    // missing file names the file, and so does a file the process
    // cannot open (permission-denied or disappearing mid-check) —
    // never a blank picture downstream.
    let resolved = ctx.folder.join(&path);
    if !resolved.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("video source {path:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `path`",
        );
        return None;
    }
    if std::fs::File::open(&resolved).is_err() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("video source {path:?} cannot be opened (permission denied or the file disappeared)"),
            "make the file readable (check its permissions) or point `path` at a readable copy",
        );
        return None;
    }
    let fit = match find(node, "fit") {
        None => Fit::Contain,
        Some(v) => match as_str(v).and_then(Fit::parse) {
            Some(f) => f,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FIT",
                    "video `fit` must be exactly one of `contain`, `cover`, `fill`",
                    "set `fit=\"contain\"`, `\"cover\"`, or `\"fill\"`",
                );
                return None;
            }
        },
    };
    // Sprint 5 T007: an infinitely looping GIF without an explicit
    // `duration` is unbounded (REC-005), like a `loop=true` clip — the
    // refusal asks for one before any window math. Content-sniffed, so
    // a renamed file is refused the same way; the head read is bounded
    // and a missed sniff simply skips this check (the probe below
    // still gates the source). Only duration-less clips need it: a
    // written `duration` bounds the animation explicitly.
    if duration_opt.is_none() {
        if let Some(head) = read_head(&resolved, 262_144) {
            if let Some(gif) = gif_info(&head, head.len()) {
                if gif.infinite {
                    ctx.push(
                        node.name_line,
                        node.name_column,
                        "E_BAD_FOOTAGE",
                        format!("video source {path:?} is an infinitely looping GIF with no explicit `duration`; its length would be unbounded"),
                        "add `duration=<seconds>` to the clip (with `loop=true` to repeat the animation, or without it to play once and hold the last frame)",
                    );
                    return None;
                }
            }
        }
    }
    let order = parse_order(ctx, node)?;
    // Sprint 3 video sound keys (T007): volume shapes the clip's sound
    // in the mix; timing validation for fades runs below against the
    // resolved (written or inferred) clip length. Mixing itself belongs
    // to T010.
    let volume_db = parse_volume_db(ctx, node, "video clip")?;
    // Sprint 5 footage trim and rate (T001): the declared source window
    // plus rate, resolved against the probed source end. Every footage
    // problem is collected (never just the first), so one bad clip can
    // name a bad rate, a backward window, and a past-end start together.
    let ts = footage_trim(ctx, node, "trim-start", "trim_start");
    let te = footage_trim(ctx, node, "trim-end", "trim_end");
    let rate_opt = footage_rate(ctx, node);
    let mut footage_bad = ts.is_none() || te.is_none() || rate_opt.is_none() || loop_opt.is_none();
    let trim_start = ts.flatten().unwrap_or(0.0);
    let trim_end = te.flatten();
    let rate = rate_opt.unwrap_or(1.0);
    let loop_clip = loop_opt.unwrap_or(false);
    if ts.is_some() && te.is_some() {
        if let Some(end) = trim_end {
            if end < trim_start {
                let v = find(node, "trim-end")
                    .or_else(|| find(node, "trim_end"))
                    .unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FOOTAGE",
                    format!(
                        "video trim window {trim_start}..{end} runs backward; trims are seconds into the source"
                    ),
                    "set `trim-end` after `trim-start` for a forward window",
                );
                footage_bad = true;
            } else if end == trim_start {
                let v = find(node, "trim-end")
                    .or_else(|| find(node, "trim_end"))
                    .unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FOOTAGE",
                    format!(
                        "video trim window {trim_start}..{end} is empty; unlike audio silence, video has no frame to show"
                    ),
                    "set `trim-end` after `trim-start` for a positive window",
                );
                footage_bad = true;
            }
        }
    }
    // Source-end checks need probe facts; without them (no FFmpeg on
    // PATH, or an unreadable file) only the declaration checks above
    // apply, and the effective end stays unknown until render probes.
    let source_duration = probe_source_duration(&resolved);
    let mut effective_end = trim_end;
    let mut clamped = false;
    if !footage_bad {
        if let Some(src) = source_duration {
            if trim_start >= src {
                let (line, col) = find(node, "trim-start")
                    .or_else(|| find(node, "trim_start"))
                    .map(|v| (v.line, v.column))
                    .unwrap_or((node.name_line, node.name_column));
                ctx.push(
                    line,
                    col,
                    "E_BAD_FOOTAGE",
                    format!(
                        "video `trim-start` {trim_start} is at or past the source end {src} of {path:?}; the window would be empty"
                    ),
                    format!(
                        "set `trim-start` below {src}, or point `path` at a longer source"
                    ),
                );
                footage_bad = true;
            } else if let Some(end) = effective_end {
                if end > src {
                    effective_end = Some(src);
                    clamped = true;
                }
            } else {
                effective_end = Some(src);
            }
        }
    }
    if footage_bad {
        return None;
    }
    // Sprint 5 T009: resolve the clip length — the written `duration`,
    // or the inferred `(trim_end − trim_start) ÷ rate` using the probed
    // source end for an omitted `trim-end`. An omitted `duration` with
    // no knowable end (a still with no declared `trim-end`, or an
    // unprobable source) has no finite length: refuse asking for one
    // (REC-005), never guess.
    let (duration, inferred) = match duration_opt {
        Some(d) => (d, false),
        None => match effective_end {
            Some(e) => ((e - trim_start) / rate, true),
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_FOOTAGE",
                    format!("video clip omits `duration` but the source end of {path:?} is unknown, so no length can be inferred"),
                    "add `duration=<seconds>` to the clip (or declare `trim-end=<seconds>` to bound the window)",
                );
                return None;
            }
        },
    };
    let effective_duration = effective_end.map(|e| (e - trim_start) / rate);
    let fade_in = parse_fade(ctx, node, "fade-in", "fade_in", duration, "video clip")?;
    let fade_out = parse_fade(ctx, node, "fade-out", "fade_out", duration, "video clip")?;
    let crossfade = parse_crossfade(ctx, node, "video clip")?;
    let duck_under = parse_duck_under(ctx, node, "video clip")?;
    let fx = parse_sound_fx(ctx, node, "video clip")?;
    let waveform = parse_waveform(ctx, node, "video clip")?;
    let turn = clip_turn(ctx, node, "video clip")?;
    let filter_blur = clip_filter(ctx, node, "video clip")?;
    ctx.footage.push(VideoFootage {
        path: path.clone(),
        line: node.name_line,
        column: node.name_column,
        trim_start,
        trim_end,
        rate,
        loop_clip,
        source_duration,
        effective_start: trim_start,
        effective_end,
        effective_duration,
        duration,
        inferred,
        clamped,
    });
    Some(VideoClip {
        path,
        start,
        duration,
        duration_inferred: inferred,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        fit,
        order,
        volume_db,
        fade_in,
        fade_out,
        crossfade,
        duck_under,
        fx,
        waveform,
        trim_start,
        trim_end,
        rate,
        loop_clip,
        turn,
        filter_blur,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Validate one Sprint 3 `audio` track (T007) with Sprint 6 T003
/// duration rules: a readable sound source in a decodable format, an
/// absolute half-open window, ordered trim seconds into the source, a
/// `rate` from 0.1 to 10 (default 1, pitch follows speed like video),
/// a volume of silence through +12 dB, and linear fades within the
/// clip window. An omitted `duration` resolves to
/// `(trim_end − trim_start) ÷ rate` using the probed source end,
/// exactly like video inference, and echoes in `check` output for
/// review. Every problem is a located diagnostic; mixing and
/// render-time decode refusal belong to T008.
fn validate_audio(ctx: &mut Ctx, node: &RawNode) -> Option<AudioClip> {
    const KEYS: &[&str] = &[
        "path",
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "trim-start",
        "trim_start",
        "trim-end",
        "trim_end",
        "rate",
        "volume",
        "fade-in",
        "fade_in",
        "fade-out",
        "fade_out",
        "crossfade",
        "duck-under",
        "duck_under",
        "duck-amount",
        "duck_amount",
        "delay",
        "delay-feedback",
        "delay_feedback",
        "delay-mix",
        "delay_mix",
        "reverb",
        "detect-silence",
        "detect_silence",
        "silence-threshold",
        "silence_threshold",
        "silence-min",
        "silence_min",
        "waveform",
        "waveform-from",
        "waveform_from",
        "waveform-to",
        "waveform_to",
    ];
    for (k, v) in &node.props {
        if LATER_AUDIO_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("audio track `{k}` needs Sprint 10; audio mixes trim, rate, volume, linear fades, crossfades, ducking, delay/reverb, `detect-silence`, `volume`/`pan` key/tween/set envelopes, and `waveform` drawing only"),
                "remove the key, ride `volume`/`pan` with `key`/`tween`/`set`, or wait for Sprint 10 sound proof",
            ));
            return None;
        }
        if !KEYS.contains(&k.as_str()) && !TEXT_FX_KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown audio key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    if reject_text_fx(ctx, node, "audio track") {
        return None;
    }
    if reject_spans(ctx, node, "audio track") {
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "audio", "audio track")?;
    // Start and duration report independently (like the old shared
    // window check): a clip missing both names both problems at once.
    let start = clip_start_relaxed(ctx, node, "audio track");
    let duration_opt = clip_optional_duration(ctx, node, "audio track");
    let (start, duration_opt) = match (start, duration_opt) {
        (Some(s), Some(d)) => (s, d),
        _ => return None,
    };
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "audio track is missing required `path`",
                "add `path=\"audio/music.wav\"` to the audio track",
            );
            return None;
        }
    };
    // Presence and readability come before format: a missing file names
    // the file, a format outside the Sprint 3 set names the file and
    // waits for a later sprint.
    if !ctx.folder.join(&path).is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("audio source {path:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `path`",
        );
        return None;
    }
    if !audio_format_supported(&path) {
        ctx.diags.push(Diagnostic::new(
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "E_UNSUPPORTED",
            format!("audio source {path:?} needs a later sprint: Sprint 3 decodes WAV, MP3, FLAC, Ogg Opus, M4A/AAC, AIFF, WMA, and other FFmpeg-decodable audio only"),
            "point `path` at a supported audio file, or wait for the later audio-format sprint",
        ));
        return None;
    }
    // Sprint 6 T003 `rate`: absent means unity; present must be finite
    // from 0.1 to 10, exactly like video. Anything else is refused,
    // never guessed to unity.
    let rate = match find(node, "rate") {
        None => 1.0,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && (0.1..=10.0).contains(&x) => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "audio track `rate` {} is outside the accepted range 0.1 to 10",
                        raw_text(v)
                    ),
                    "set `rate=<0.1 to 10>` (for example `rate=2` for double speed, `rate=0.5` for half speed), or remove the key for unity rate",
                );
                return None;
            }
        },
    };
    let trim_of = |ctx: &mut Ctx, hyphen: &str, underscore: &str| -> Option<Option<f64>> {
        let found = find(node, hyphen).or_else(|| find(node, underscore));
        match found {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() && x >= 0.0 => Some(Some(x)),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_AUDIO",
                        format!("audio `{hyphen}` must be seconds into the source from 0"),
                        format!("set `{hyphen}=<seconds >= 0>` or remove the key"),
                    );
                    None
                }
            },
        }
    };
    let trim_start = trim_of(ctx, "trim-start", "trim_start")?.unwrap_or(0.0);
    let trim_end = trim_of(ctx, "trim-end", "trim_end")?;
    if let Some(end) = trim_end {
        // An empty window (end == start) is valid silence; a backward
        // window is refused, never swapped silently.
        if end < trim_start {
            let v = find(node, "trim-end")
                .or_else(|| find(node, "trim_end"))
                .unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_AUDIO",
                format!("audio trim window {trim_start}..{end} runs backward; trims are seconds into the source"),
                "set `trim-end` after `trim-start`, or equal for an empty (silent) window",
            );
            return None;
        }
    }
    // Source-end facts (same probe the video path uses): without them
    // only the declaration checks above apply. A past-end `trim-end`
    // clamps to the probed source end and the clamp echoes for review;
    // a `trim-start` at or past the source end leaves an empty window.
    let source_duration = probe_source_duration(&ctx.folder.join(&path));
    let mut effective_end = trim_end;
    let mut clamped: Option<(f64, f64)> = None;
    if let Some(src) = source_duration {
        if trim_start >= src {
            let (line, col) = find(node, "trim-start")
                .or_else(|| find(node, "trim_start"))
                .map(|v| (v.line, v.column))
                .unwrap_or((node.name_line, node.name_column));
            ctx.push(
                line,
                col,
                "E_BAD_AUDIO",
                format!(
                    "audio `trim-start` {trim_start} is at or past the source end {src} of {path:?}; the window would be empty"
                ),
                format!("set `trim-start` below {src}, or point `path` at a longer source"),
            );
            return None;
        } else if let Some(end) = effective_end {
            if end > src {
                effective_end = Some(src);
                clamped = Some((end, src));
            }
        } else {
            effective_end = Some(src);
        }
    }
    // Resolve the clip length — the written `duration`, or the inferred
    // `(trim_end − trim_start) ÷ rate` using the probed source end for
    // an omitted `trim-end`. An omitted `duration` with no knowable end
    // has no finite length: refuse asking for one (REC-005), never guess.
    let (duration, inferred) = match duration_opt {
        Some(d) => (d, false),
        None => match effective_end {
            Some(e) => ((e - trim_start) / rate, true),
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_AUDIO",
                    format!("audio track omits `duration` but the source end of {path:?} is unknown, so no length can be inferred"),
                    "add `duration=<seconds>` to the audio track (or declare `trim-end=<seconds>` to bound the window)",
                );
                return None;
            }
        },
    };
    let volume_db = parse_volume_db(ctx, node, "audio track")?;
    let fade_in = parse_fade(ctx, node, "fade-in", "fade_in", duration, "audio track")?;
    let fade_out = parse_fade(ctx, node, "fade-out", "fade_out", duration, "audio track")?;
    let crossfade = parse_crossfade(ctx, node, "audio track")?;
    let duck_under = parse_duck_under(ctx, node, "audio track")?;
    let fx = parse_sound_fx(ctx, node, "audio track")?;
    let waveform = parse_waveform(ctx, node, "audio track")?;
    if inferred {
        let end = effective_end.unwrap_or(trim_start + duration * rate);
        let mut detail = format!("from source window {trim_start:.3}s..{end:.3}s at rate {rate}x");
        if let Some((declared, src)) = clamped {
            detail.push_str(&format!(
                "; trim-end {declared:.3}s clamped to source end {src:.3}s"
            ));
        }
        ctx.inferred.push(InferredLength {
            kind: "audio",
            label: Some(path.clone()),
            line: node.name_line,
            column: node.name_column,
            duration,
            detail,
        });
    }
    Some(AudioClip {
        path,
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        trim_start,
        trim_end,
        rate,
        volume_db,
        fade_in,
        fade_out,
        crossfade,
        duck_under,
        fx,
        waveform,
        line: node.name_line,
        column: node.name_column,
    })
}

/// A finite box number with a default; non-numbers fail as `E_BAD_BOX`.
fn box_num(ctx: &mut Ctx, node: &RawNode, key: &str, default: f64) -> Option<f64> {
    match find(node, key) {
        None => Some(default),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_BOX",
                    format!("box `{key}` must be a number"),
                    format!("set `{key}=<number>` on the box clip"),
                );
                None
            }
        },
    }
}

/// Element tag for paint-stack diagnostics (Sprint 8 look, T004):
/// the clip kind plus its `id` when declared, so every refusal names
/// the offending element and property.
fn paint_element(identity_id: &Option<String>) -> String {
    match identity_id {
        Some(id) => format!("box clip {id:?}"),
        None => "box clip".to_string(),
    }
}

/// One `stroke-width` or `border-width` value in canvas pixels
/// (Sprint 8 look, T004): `stroke-width` (or `stroke_width`) wins
/// when written, else the historical `border-width`. A negative
/// `stroke-width` fails with `E_BAD_PAINT` naming the element and the
/// property; a negative `border-width` keeps the historical `E_BAD_BOX`
/// refusal byte-identically (Sprint 2 boxes keep validating as before).
fn box_stroke_width(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<f64> {
    let (key, stroke) = if find(node, "stroke-width").is_some() {
        ("stroke-width", true)
    } else if find(node, "stroke_width").is_some() {
        ("stroke_width", true)
    } else if find(node, "border-width").is_some() {
        ("border-width", false)
    } else {
        ("border_width", false)
    };
    let width = box_num(ctx, node, key, 0.0)?;
    if width < 0.0 {
        let v = find(node, key).unwrap();
        if stroke {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_PAINT",
                format!("{el} `{key}` {width} is invalid; stroke widths cannot be negative"),
                format!("set `{key}=<pixels >= 0>` on {el}"),
            );
        } else {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_BOX",
                format!("box `{key}` {width} is invalid; border widths cannot be negative"),
                format!("set `{key}=<pixels >= 0>` on the box clip"),
            );
        }
        return None;
    }
    Some(width)
}

/// One `stroke-color` or `border-color` value (Sprint 8 look, T004):
/// `stroke-color` (or `stroke_color`) wins when written, else the
/// historical `border-color`. Anything but `#rgb`/`#rrggbb` fails with
/// `E_BAD_COLOR` naming the written property.
fn box_stroke_color(ctx: &mut Ctx, node: &RawNode) -> Option<Rgb> {
    let key = if find(node, "stroke-color").is_some() {
        "stroke-color"
    } else if find(node, "stroke_color").is_some() {
        "stroke_color"
    } else if find(node, "border-color").is_some() {
        "border-color"
    } else {
        "border_color"
    };
    match find(node, key) {
        None => Some(Rgb { r: 0, g: 0, b: 0 }),
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => Some(c),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    format!("box `{key}` is not a `#rgb` or `#rrggbb` color"),
                    format!("use a hex color like `{key}=\"#000000\"`"),
                );
                None
            }
        },
    }
}

/// One `stroke-position` (Sprint 8 look, T004): absent means `inside`
/// (the historical border); present must read `center`, `inside`, or
/// `outside`, naming the accepted set otherwise.
fn box_stroke_position(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<StrokePosition> {
    let found = find(node, "stroke-position").or_else(|| find(node, "stroke_position"));
    match found {
        None => Some(StrokePosition::Inside),
        Some(v) => match as_str(v).and_then(StrokePosition::parse) {
            Some(p) => Some(p),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "{el} `stroke-position` {} is unknown; accepted positions are: {}",
                        raw_text(v),
                        StrokePosition::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    format!(
                        "set `stroke-position=\"center\"`, `\"inside\"`, or `\"outside\"` on {el}"
                    ),
                );
                None
            }
        },
    }
}

/// Parse one `box-shadow` value (Sprint 8 look, T004): `none` (or an
/// absent key) paints no shadow, else `"<offset-x> <offset-y> <blur>
/// <spread> <#color>"` with finite offsets, non-negative blur and
/// spread, and a `#rgb`/`#rrggbb` color. Anything else fails with
/// `E_BAD_PAINT` naming the element and the property with the syntax.
fn parse_box_shadow_value(s: &str) -> Option<BoxShadow> {
    let t: Vec<&str> = s.split_whitespace().collect();
    if t.len() != 5 {
        return None;
    }
    let num = |i: usize| t[i].parse::<f64>().ok().filter(|x| x.is_finite());
    let offset_x = num(0)?;
    let offset_y = num(1)?;
    let blur = num(2).filter(|x| *x >= 0.0)?;
    let spread = num(3).filter(|x| *x >= 0.0)?;
    let color = parse_color(t[4])?;
    Some(BoxShadow {
        offset_x,
        offset_y,
        blur,
        spread,
        color,
    })
}

fn box_shadow_opt(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<Option<BoxShadow>> {
    let found = find(node, "box-shadow").or_else(|| find(node, "box_shadow"));
    match found {
        None => Some(None),
        Some(v) => match as_str(v) {
            Some("none") => Some(None),
            Some(s) => match parse_box_shadow_value(s) {
                Some(sh) => Some(Some(sh)),
                None => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_PAINT",
                        format!(
                            "{el} `box-shadow` {s:?} is unusable; a shadow reads `\"<offset-x> <offset-y> <blur> <spread> <#color>\"` with blur and spread at 0 and up"
                        ),
                        format!(
                            "set `box-shadow=\"8 8 12 0 \\\"#000000\\\"\"` on {el}, or `box-shadow=\"none\"` to paint no shadow"
                        ),
                    );
                    None
                }
            },
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "{el} `box-shadow` must be a quoted shadow; found {}",
                        raw_text(v)
                    ),
                    format!(
                        "set `box-shadow=\"8 8 12 0 \\\"#000000\\\"\"` on {el}, or `box-shadow=\"none\"` to paint no shadow"
                    ),
                );
                None
            }
        },
    }
}

/// One `mix-blend-mode` (Sprint 8 look, T004): absent means `normal`
/// (source-over); present must read `normal`, `multiply`, `screen`,
/// or `overlay`, naming the accepted set otherwise.
fn box_blend_mode(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<BlendMode> {
    let found = find(node, "mix-blend-mode").or_else(|| find(node, "mix_blend_mode"));
    match found {
        None => Some(BlendMode::Normal),
        Some(v) => match as_str(v).and_then(BlendMode::parse) {
            Some(m) => Some(m),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "{el} `mix-blend-mode` {} is unknown; accepted modes are: {}",
                        raw_text(v),
                        BlendMode::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    format!(
                        "set `mix-blend-mode=\"normal\"`, `\"multiply\"`, `\"screen\"`, or `\"overlay\"` on {el}"
                    ),
                );
                None
            }
        },
    }
}

/// The backdrop pair (Sprint 8 look, T004): `backdrop-blur` samples
/// the flattened layers beneath through a blur sigma (absent means 0,
/// negative refuses), and `backdrop-brightness` scales them (absent
/// means 1, negative refuses). Both refuse with `E_BAD_PAINT` naming
/// the element and the property.
fn box_backdrop(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<(f64, f64)> {
    let blur = match find(node, "backdrop-blur").or_else(|| find(node, "backdrop_blur")) {
        None => 0.0,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "{el} `backdrop-blur` {} is invalid; backdrop blur runs 0 and up",
                        raw_text(v)
                    ),
                    format!(
                        "set `backdrop-blur=<pixels >= 0>` on {el}, or remove the key for a sharp backdrop"
                    ),
                );
                return None;
            }
        },
    };
    let brightness = match find(node, "backdrop-brightness")
        .or_else(|| find(node, "backdrop_brightness"))
    {
        None => 1.0,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "{el} `backdrop-brightness` {} is invalid; backdrop brightness scales 0 and up (1 keeps the layers)",
                        raw_text(v)
                    ),
                    format!(
                        "set `backdrop-brightness=<factor >= 0>` on {el}, or remove the key to keep the layers"
                    ),
                );
                return None;
            }
        },
    };
    Some((blur, brightness))
}

/// One `overflow` (Sprint 8 look, T004): absent means `visible`;
/// present must read `visible`, `hidden`, or `clip`, naming the
/// accepted set otherwise.
fn box_overflow(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<Overflow> {
    match find(node, "overflow") {
        None => Some(Overflow::Visible),
        Some(v) => match as_str(v).and_then(Overflow::parse) {
            Some(o) => Some(o),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "{el} `overflow` {} is unknown; accepted values are: {}",
                        raw_text(v),
                        Overflow::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    format!("set `overflow=\"visible\"`, `\"hidden\"`, or `\"clip\"` on {el}"),
                );
                None
            }
        },
    }
}

/// One `mask` / `clip-path` reference (Sprint 8 look, T004): absent
/// means unmasked; present must be a quoted id naming a `box` clip in
/// the same composition. Dangling references, non-box targets, and
/// cycles refuse in [`check_mask_graph`], once every clip is known —
/// forward references stay valid here.
fn box_mask_ref(
    ctx: &mut Ctx,
    node: &RawNode,
    hyphen: &str,
    underscore: &str,
    el: &str,
) -> Option<Option<MaskRef>> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(None),
        Some(v) => match as_str(v) {
            Some(s) if !s.is_empty() => Some(Some(MaskRef {
                target: s.to_string(),
                line: v.line,
                column: v.column,
            })),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "{el} `{hyphen}` must be a quoted id naming a `box` clip in the same composition; found {}",
                        raw_text(v)
                    ),
                    format!(
                        "set `{hyphen}=\"<box clip id>\"` on {el}, or remove the key to paint unmasked"
                    ),
                );
                None
            }
        },
    }
}

/// Paint-stack keys that belong on `box` clips (Sprint 8 look,
/// T004): the positioned stroke, the shadow, the blend, the backdrop,
/// overflow, and mask references. `stroke-width`/`stroke-color` alias
/// the historical border keys; the rest are new.
const PAINT_STACK_KEYS: &[&str] = &[
    "stroke-width",
    "stroke_width",
    "stroke-color",
    "stroke_color",
    "stroke-position",
    "stroke_position",
    "box-shadow",
    "box_shadow",
    "mix-blend-mode",
    "mix_blend_mode",
    "backdrop-blur",
    "backdrop_blur",
    "backdrop-brightness",
    "backdrop_brightness",
    "overflow",
    "mask",
    "clip-path",
    "clip_path",
];

/// Paint-stack keys a flex child must not carry (Sprint 8 look,
/// T004): laid-out children paint one fill with one stroke, while
/// shadows, blends, backdrops, overflow, and masks apply to `box`
/// clips. `stroke-position` rides on [`FLEX_CHILD_KEYS`]; the stroke
/// width and color alias the child's border keys.
const FLEX_CHILD_PAINT_KEYS: &[&str] = &[
    "box-shadow",
    "box_shadow",
    "mix-blend-mode",
    "mix_blend_mode",
    "backdrop-blur",
    "backdrop_blur",
    "backdrop-brightness",
    "backdrop_brightness",
    "overflow",
    "mask",
    "clip-path",
    "clip_path",
];

/// Box keys that name Sprint 8 looks: per-side borders and border
/// images. Well-formed but refused with the enabling sprint (T004),
/// never rendered partially. Gradient and image fills arrived in T003
/// as child elements, so a fill-shaped attribute redirects there.
const RICH_BOX_KEYS: &[&str] = &[
    "border-top-width",
    "border_top_width",
    "border-right-width",
    "border_right_width",
    "border-bottom-width",
    "border_bottom_width",
    "border-left-width",
    "border_left_width",
    "border-image",
    "border_image",
];

/// Fill-shaped box attributes from before T003 (Sprint 8 look): fills
/// are child elements (`linear-gradient`, `radial-gradient`,
/// `image-fill`), never attributes. A fill-shaped attribute fails with
/// `E_BAD_FILL` naming the element to write instead.
const LEGACY_FILL_KEYS: &[&str] = &["gradient", "fill-image", "fill_image"];

fn validate_box(ctx: &mut Ctx, node: &RawNode, styles: &[StyleDecl]) -> Option<BoxClip> {
    let merged = styled_node(ctx, node, "box clip", styles)?;
    let node = merged.as_ref();
    const KEYS: &[&str] = &[
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "x",
        "y",
        "width",
        "height",
        "fill",
        "radius",
        "border-width",
        "border_width",
        "border-color",
        "border_color",
        "stroke-width",
        "stroke_width",
        "stroke-color",
        "stroke_color",
        "stroke-position",
        "stroke_position",
        "box-shadow",
        "box_shadow",
        "mix-blend-mode",
        "mix_blend_mode",
        "backdrop-blur",
        "backdrop_blur",
        "backdrop-brightness",
        "backdrop_brightness",
        "overflow",
        "mask",
        "clip-path",
        "clip_path",
        "opacity",
        "order",
        "rotation-x",
        "rotation_x",
        "rotate-x",
        "rotate_x",
        "rotation-y",
        "rotation_y",
        "rotate-y",
        "rotate_y",
        "perspective",
        "origin",
        "filter",
    ];
    for (k, v) in &node.props {
        if LEGACY_FILL_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_FILL",
                format!("box `{k}` is not a box key; gradient and image fills are child elements, not attributes"),
                "write a `linear-gradient`, `radial-gradient`, or `image-fill` child element with its `stop` children or `src` on the box instead",
            );
            return None;
        }
        if RICH_BOX_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("box `{k}` needs Sprint 8; Sprint 2 boxes use one fill color, one corner-radius set, one border width with one border color, and one opacity"),
                "use a single `fill`, `radius`, `border-width` with one `border-color`, and `opacity`, or wait for Sprint 8 layout-and-look proof",
            ));
            return None;
        }
        if !KEYS.contains(&k.as_str()) && !TEXT_FX_KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown box key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    if reject_text_fx(ctx, node, "box clip") {
        return None;
    }
    if reject_spans(ctx, node, "box clip") {
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "box", "box clip")?;
    // Sprint 6 T003: `duration` is optional — an omitted one runs until
    // the composition end (`NAN` marks the omission for inference).
    // Start and duration report independently (like the old shared
    // window check): a clip missing both names both problems at once.
    let start = clip_start_relaxed(ctx, node, "box clip");
    let duration_parsed = clip_optional_duration(ctx, node, "box clip");
    let (start, duration) = match (start, duration_parsed) {
        (Some(s), Some(Some(d))) => (s, d),
        (Some(s), Some(None)) => (s, f64::NAN),
        _ => return None,
    };
    let x = box_num(ctx, node, "x", 0.0)?;
    let y = box_num(ctx, node, "y", 0.0)?;
    // Size is required and must be positive: a missing, negative, or
    // zero size is refused, never drawn as a guessed rectangle.
    let mut size = |key: &str| -> Option<f64> {
        match find(node, key) {
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_BOX",
                    format!("box clip is missing required `{key}`"),
                    format!("add `{key}=<positive pixels>` to the box clip"),
                );
                None
            }
            Some(v) => match as_f64(v) {
                Some(val) if val.is_finite() && val > 0.0 => Some(val),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_BOX",
                        format!("box `{key}` must be a positive number of canvas pixels"),
                        format!("set `{key}=<positive pixels>` on the box clip"),
                    );
                    None
                }
            },
        }
    };
    let width = size("width")?;
    let height = size("height")?;
    let fill = match find(node, "fill") {
        None => notation::model::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    "box `fill` is not a `#rgb` or `#rrggbb` color",
                    "use a hex color like `fill=\"#ff0000\"`",
                );
                return None;
            }
        },
    };
    let radius = box_num(ctx, node, "radius", 0.0)?;
    if radius < 0.0 {
        let v = find(node, "radius").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_BOX",
            format!("box `radius` {radius} is invalid; corner radii cannot be negative"),
            "set `radius=<pixels >= 0>` on the box clip",
        );
        return None;
    }
    let el = paint_element(&identity.id);
    let border_width = box_stroke_width(ctx, node, &el)?;
    let border_color = box_stroke_color(ctx, node)?;
    let stroke_position = box_stroke_position(ctx, node, &el)?;
    let box_shadow = box_shadow_opt(ctx, node, &el)?;
    let blend_mode = box_blend_mode(ctx, node, &el)?;
    let (backdrop_blur, backdrop_brightness) = box_backdrop(ctx, node, &el)?;
    let overflow = box_overflow(ctx, node, &el)?;
    let mask = box_mask_ref(ctx, node, "mask", "mask", &el)?;
    let clip_path = box_mask_ref(ctx, node, "clip-path", "clip_path", &el)?;
    let opacity = box_num(ctx, node, "opacity", 1.0)?;
    if !(0.0..=1.0).contains(&opacity) {
        let (line, col) = find(node, "opacity")
            .map(|v| (v.line, v.column))
            .unwrap_or((node.name_line, node.name_column));
        ctx.push(
            line,
            col,
            "E_BAD_BOX",
            format!(
                "box `opacity` {opacity} is invalid; opacity runs 0 (transparent) to 1 (opaque)"
            ),
            "set `opacity=<number from 0 to 1>` on the box clip",
        );
        return None;
    }
    let order = parse_order(ctx, node)?;
    let turn = clip_turn(ctx, node, "box clip")?;
    let filter_blur = clip_filter(ctx, node, "box clip")?;
    let fill_paint = validate_box_fill(ctx, node)?;
    Some(BoxClip {
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        x,
        y,
        width,
        height,
        fill,
        fill_paint,
        radius,
        border_width,
        border_color,
        stroke_position,
        box_shadow,
        blend_mode,
        backdrop_blur,
        backdrop_brightness,
        overflow,
        mask,
        clip_path,
        opacity,
        order,
        turn,
        filter_blur,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Validate the fill children of a `box` clip (Sprint 8 look,
/// CAP-003/T003): at most one `linear-gradient`, `radial-gradient`,
/// or `image-fill` child element (either hyphenated or underscored
/// spelling). No fill child keeps the authored `fill` color. Every
/// fill problem reports together, each naming its stop or fill and
/// the rule it breaks, before frame one.
fn validate_box_fill(ctx: &mut Ctx, node: &RawNode) -> Option<FillPaint> {
    let mut fills: Vec<&RawNode> = Vec::new();
    let mut ok = true;
    for child in &node.children {
        match child.name.as_str() {
            "linear-gradient" | "linear_gradient" | "radial-gradient" | "radial_gradient"
            | "image-fill" | "image_fill" => fills.push(child),
            "stop" => {
                ctx.push(
                    child.name_line,
                    child.name_column,
                    "E_BAD_FILL",
                    "box clip holds a `stop` outside a gradient; stops live inside a `linear-gradient` or `radial-gradient` element",
                    "move the `stop` inside a gradient element, or remove it",
                );
                ok = false;
            }
            _ => {
                ctx.push(
                    child.name_line,
                    child.name_column,
                    "E_BAD_FILL",
                    format!(
                        "box clip holds `{}`; a box holds at most one `linear-gradient`, `radial-gradient`, or `image-fill` child",
                        child.name
                    ),
                    "keep a single gradient or image-fill child on the box, or remove the child",
                );
                ok = false;
            }
        }
    }
    if fills.is_empty() {
        return ok.then_some(FillPaint::Solid);
    }
    let mut paints: Vec<FillPaint> = Vec::new();
    for fill in &fills {
        match fill.name.as_str() {
            "linear-gradient" | "linear_gradient" => {
                if let Some((angle, stops)) = validate_linear_gradient(ctx, fill) {
                    paints.push(FillPaint::Linear { angle, stops });
                } else {
                    ok = false;
                }
            }
            "radial-gradient" | "radial_gradient" => {
                if let Some(paint) = validate_radial_gradient(ctx, fill) {
                    paints.push(paint);
                } else {
                    ok = false;
                }
            }
            _ => {
                if let Some(paint) = validate_image_fill(ctx, fill) {
                    paints.push(paint);
                } else {
                    ok = false;
                }
            }
        }
    }
    if fills.len() > 1 {
        let names: Vec<String> = fills.iter().map(|f| format!("`{}`", f.name)).collect();
        ctx.push(
            fills[1].name_line,
            fills[1].name_column,
            "E_BAD_FILL",
            format!(
                "box clip carries {} fills ({}); an element paints exactly one fill",
                fills.len(),
                names.join(" and ")
            ),
            "keep a single `linear-gradient`, `radial-gradient`, or `image-fill` child on the box",
        );
        return None;
    }
    if !ok {
        return None;
    }
    paints.into_iter().next()
}

/// Validate one `stop` child of a gradient (Sprint 8 look, T003): a
/// required `offset` from 0 to 1 and a required `#rgb`/`#rrggbb`
/// `color`. `index` numbers the stop from 1 in document order so every
/// refusal names its stop.
fn validate_gradient_stop(
    ctx: &mut Ctx,
    kind: &str,
    node: &RawNode,
    index: usize,
) -> Option<GradientStop> {
    const KEYS: &[&str] = &["offset", "color"];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown {kind} stop key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let what = format!("{kind} stop {index}");
    let offset = match find(node, "offset") {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_FILL",
                format!("{what} is missing required `offset`; every stop names its position from 0 to 1"),
                format!("add `offset=<number from 0 to 1>` to stop {index} of the {kind}"),
            );
            return None;
        }
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && (0.0..=1.0).contains(&x) => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FILL",
                    format!("{what} `offset` is outside 0 to 1; stop {index} names its position inside the gradient"),
                    format!("set `offset=<number from 0 to 1>` on stop {index} of the {kind}"),
                );
                return None;
            }
        },
    };
    let color = match find(node, "color").and_then(as_str) {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_FILL",
                format!("{what} is missing required `color`; every stop names its color"),
                format!("add `color=\"#rrggbb\"` to stop {index} of the {kind}"),
            );
            return None;
        }
        Some(s) => match parse_color(s) {
            Some(c) => c,
            None => {
                let v = find(node, "color").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FILL",
                    format!("{what} `color` is not a `#rgb` or `#rrggbb` color"),
                    format!("set `color=\"#rrggbb\"` on stop {index} of the {kind}"),
                );
                return None;
            }
        },
    };
    Some(GradientStop {
        offset,
        color,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Check one parsed stop list (Sprint 8 look, T003): empty lists are
/// refused, a single stop paints flat, and two or more stops must run
/// from offset 0 to offset 1 in strictly rising order with no repeats.
/// Every refusal names its stop and the rule. `node` is the gradient
/// element, located only when no stop exists to name.
fn check_stop_order(
    ctx: &mut Ctx,
    kind: &str,
    node: &RawNode,
    stops: &[GradientStop],
) -> bool {
    if stops.is_empty() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_FILL",
            format!("{kind} carries no `stop` children; a gradient needs at least one stop"),
            format!("add at least one `stop offset=<0 to 1> color=\"#rrggbb\"` to the {kind}"),
        );
        return false;
    }
    if stops.len() == 1 {
        // One stop paints flat in its own color at any offset.
        return true;
    }
    let mut ok = true;
    if stops[0].offset != 0.0 {
        ctx.push(
            stops[0].line,
            stops[0].column,
            "E_BAD_FILL",
            format!(
                "{kind} stop 1 has offset {}; the first stop must sit at offset 0",
                stops[0].offset
            ),
            format!("set stop 1 `offset=0` on the {kind}"),
        );
        ok = false;
    }
    let last = stops.len() - 1;
    if stops[last].offset != 1.0 {
        ctx.push(
            stops[last].line,
            stops[last].column,
            "E_BAD_FILL",
            format!(
                "{kind} stop {} has offset {}; the last stop must sit at offset 1",
                last + 1,
                stops[last].offset
            ),
            format!("set stop {} `offset=1` on the {kind}", last + 1),
        );
        ok = false;
    }
    for (i, pair) in stops.windows(2).enumerate() {
        let (prev, cur) = (&pair[0], &pair[1]);
        if cur.offset == prev.offset {
            ctx.push(
                cur.line,
                cur.column,
                "E_BAD_FILL",
                format!(
                    "{kind} stop {} duplicates stop {} offset {}; stop offsets must be unique",
                    i + 2,
                    i + 1,
                    cur.offset
                ),
                format!(
                    "give stop {} an `offset` above {} and below the next stop",
                    i + 2,
                    prev.offset
                ),
            );
            ok = false;
        } else if cur.offset < prev.offset {
            ctx.push(
                cur.line,
                cur.column,
                "E_BAD_FILL",
                format!(
                    "{kind} stop {} offset {} runs before stop {} offset {}; stops must run in rising offset order",
                    i + 2,
                    cur.offset,
                    i + 1,
                    prev.offset
                ),
                format!("order the stops of the {kind} from offset 0 to offset 1"),
            );
            ok = false;
        }
    }
    ok
}

/// Collect the `stop` children of one gradient element, refusing
/// non-`stop` children. Returns `None` when any stop is unusable; the
/// order rules in [`check_stop_order`] run only over cleanly parsed
/// stops so one bad value never cascades into ordering noise.
fn collect_gradient_stops(
    ctx: &mut Ctx,
    kind: &str,
    node: &RawNode,
) -> Option<Vec<GradientStop>> {
    let mut stops: Vec<GradientStop> = Vec::new();
    let mut ok = true;
    for (i, child) in node
        .children
        .iter()
        .enumerate()
        .map(|(i, c)| (i + 1, c))
    {
        if child.name != "stop" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_FILL",
                format!(
                    "{kind} holds `{}`; a gradient holds `stop` children only",
                    child.name
                ),
                format!("use `stop offset=<0 to 1> color=\"#rrggbb\"` inside the {kind}, or remove the child"),
            );
            ok = false;
            continue;
        }
        match validate_gradient_stop(ctx, kind, child, i) {
            Some(stop) => stops.push(stop),
            None => ok = false,
        }
    }
    if !ok {
        return None;
    }
    if !check_stop_order(ctx, kind, node, &stops) {
        return None;
    }
    Some(stops)
}

/// Validate one `linear-gradient` fill child (Sprint 8 look, T003): an
/// optional `angle` in CSS degrees (0 points up, 90 points right) and
/// ordered `stop` children interpolating in sRGB.
fn validate_linear_gradient(ctx: &mut Ctx, node: &RawNode) -> Option<(f64, Vec<GradientStop>)> {
    const KIND: &str = "linear-gradient";
    for (k, v) in &node.props {
        if k != "angle" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown {KIND} key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let angle = match find(node, "angle") {
        None => 90.0,
        Some(v) => match as_f64(v) {
            Some(a) if a.is_finite() => a,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FILL",
                    format!("{KIND} `angle` must be a number of CSS degrees (0 points up, 90 points right)"),
                    format!("set `{KIND} angle=<degrees>`, or remove the key for left-to-right"),
                );
                return None;
            }
        },
    };
    let stops = collect_gradient_stops(ctx, KIND, node)?;
    Some((angle, stops))
}

/// Validate one `radial-gradient` fill child (Sprint 8 look, T003):
/// concentric rings from (`cx`, `cy`) in box-local pixels (defaulting
/// to the box center) out to `radius` pixels (defaulting to the
/// farthest corner), honoring the stop offsets.
fn validate_radial_gradient(ctx: &mut Ctx, node: &RawNode) -> Option<FillPaint> {
    const KIND: &str = "radial-gradient";
    for (k, v) in &node.props {
        if !["cx", "cy", "radius"].contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown {KIND} key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let mut center = |key: &str| -> Option<Option<f64>> {
        match find(node, key) {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => Some(Some(x)),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FILL",
                        format!("{KIND} `{key}` must be a number of box-local pixels"),
                        format!("set `{key}=<pixels>` on the {KIND}, or remove the key for the box center"),
                    );
                    None
                }
            },
        }
    };
    let cx = center("cx")?;
    let cy = center("cy")?;
    let radius = match find(node, "radius") {
        None => Some(None),
        Some(v) => match as_f64(v) {
            Some(r) if r.is_finite() && r > 0.0 => Some(Some(r)),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FILL",
                    format!("{KIND} `radius` must be a positive number of pixels"),
                    format!("set `radius=<positive pixels>` on the {KIND}, or remove the key to reach the farthest corner"),
                );
                return None;
            }
        },
    }?;
    let stops = collect_gradient_stops(ctx, KIND, node)?;
    Some(FillPaint::Radial {
        cx,
        cy,
        radius,
        stops,
    })
}

/// Validate one `image-fill` child (Sprint 8 look, T003): a required
/// `src` naming a readable still picture, painted clipped to the
/// element box per `object-fit` and `object-position`. Presence and
/// readability mirror the `image` clip so a missing file names the
/// fill before frame one.
fn validate_image_fill(ctx: &mut Ctx, node: &RawNode) -> Option<FillPaint> {
    const KIND: &str = "image-fill";
    for (k, v) in &node.props {
        if ![
            "src",
            "path",
            "object-fit",
            "object_fit",
            "object-position",
            "object_position",
        ]
        .contains(&k.as_str())
        {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown {KIND} key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    for child in &node.children {
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_FILL",
            format!("{KIND} holds `{}`; an image-fill carries no children", child.name),
            format!("remove the child from the {KIND}; the picture comes from `src`"),
        );
    }
    if !node.children.is_empty() {
        return None;
    }
    let src = match find(node, "src").or_else(|| find(node, "path")) {
        Some(v) => match as_str(v) {
            Some(s) => s.to_string(),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FILL",
                    format!("{KIND} `src` must be a quoted project folder path to a picture"),
                    format!("set `{KIND} src=\"media/photo.png\"`"),
                );
                return None;
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                format!("{KIND} is missing required `src`; the fill names the picture it paints"),
                format!("add `src=\"media/photo.png\"` to the {KIND}"),
            );
            return None;
        }
    };
    // Presence and readability come before format, exactly like the
    // `image` clip: a missing file names the fill, an unsupported
    // format names the fill and its sprint.
    let abs = ctx.folder.join(&src);
    if !abs.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("image-fill source {src:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `src`",
        );
        return None;
    }
    // SVG pictures never fill boxes (Sprint 8 look, CAP-005/T005):
    // they rasterize through `image` and `svg` clips only. A vector
    // source here names the fill and the clips that take it.
    if svg_format_supported(&src) {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_FILL",
            format!("{KIND} source {src:?} is an SVG picture; image fills paint PNG, JPEG, still GIF, or HEIC stills"),
            "rasterize the SVG to a `.png` first, or point an `image` or `svg` clip at the SVG source",
        );
        return None;
    }
    if !image_format_supported(&src) {
        ctx.diags.push(Diagnostic::new(
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "E_UNSUPPORTED",
            format!("image-fill source {src:?} needs a later sprint: Framesmith decodes PNG, JPEG, still GIF, and HEIC still images only"),
            "point `src` at a `.png`, `.jpg`, `.jpeg`, `.gif` (still), `.heic`, or `.heif` file, or wait for a later footage sprint",
        ));
        return None;
    }
    match std::fs::read(&abs) {
        Ok(bytes) => {
            if let Some(gif) = gif_info(&bytes, bytes.len()) {
                if gif.frames > 1 {
                    ctx.push(
                        node.name_line,
                        node.name_column,
                        "E_BAD_IMAGE",
                        format!("image-fill source {src:?} is an animated GIF; animations play in a `video` clip, not an image-fill"),
                        format!("use `video path={src:?} ...` to play the animation, or point the image-fill at a still picture"),
                    );
                    return None;
                }
                if gif.frames == 0 || !gif.complete {
                    ctx.push(
                        node.name_line,
                        node.name_column,
                        "E_BAD_IMAGE",
                        format!("image-fill source {src:?} is corrupt or not a readable GIF picture"),
                        "restore an undamaged still GIF file at the referenced path",
                    );
                    return None;
                }
            } else if !image_magic_ok(&bytes) && !heic_magic_ok(&bytes) {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_IMAGE",
                    format!("image-fill source {src:?} is corrupt or not a PNG/JPEG/GIF-still/HEIC picture"),
                    "restore an undamaged PNG, JPEG, still GIF, or HEIC file at the referenced path",
                );
                return None;
            }
        }
        Err(e) => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_UNREADABLE",
                format!("image-fill source {src:?} cannot be read: {e}"),
                "make the image file readable or fix the `src`",
            );
            return None;
        }
    }
    let fit = match find_either(node, "object-fit", "object_fit") {
        None => Fit::Fill,
        Some(v) => match as_str(v).and_then(Fit::parse) {
            Some(f) => f,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FILL",
                    "image-fill `object-fit` must be exactly one of `cover`, `contain`, or `fill`",
                    "set `object-fit=\"cover\"`, `\"contain\"`, or `\"fill\"`",
                );
                return None;
            }
        },
    };
    let placement = match find_either(node, "object-position", "object_position") {
        None => Placement::center(),
        Some(v) => match as_str(v).and_then(Placement::parse) {
            Some(p) => p,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FILL",
                    "image-fill `object-position` must be a horizontal and vertical placement like `\"center\"`, `\"top-left\"`, or `\"left top\"`",
                    "set `object-position=\"center\"` or a pair such as `object-position=\"top-left\"`",
                );
                return None;
            }
        },
    };
    Some(FillPaint::Image {
        src,
        fit,
        placement,
    })
}

/// `flex` container keys (Sprint 8 layout, T001): layout attributes in
/// either hyphenated or underscored spelling, plus the shared clip
/// identity, window, and paint-order keys. Anything else is
/// `STYLE-NAME-UNKNOWN` (or the owning task's refusal below).
const FLEX_KEYS: &[&str] = &[
    "start",
    "duration",
    "id",
    "track",
    "label",
    "hidden",
    "start-after",
    "start_after",
    "start-offset",
    "start_offset",
    "x",
    "y",
    "width",
    "height",
    "display",
    "flex-direction",
    "flex_direction",
    "gap",
    "padding",
    "margin",
    "align-items",
    "align_items",
    "justify-content",
    "justify_content",
    "order",
];

/// Paint, turn, and filter keys that belong on laid-out boxes (or on
/// no flex node at all): valid names in the wrong place fail with
/// `E_BAD_FLEX` naming the move, never as `STYLE-NAME-UNKNOWN`.
const FLEX_MISPLACED_KEYS: &[&str] = &[
    "fill",
    "radius",
    "border-width",
    "border_width",
    "border-color",
    "border_color",
    "opacity",
    "rotation-x",
    "rotation_x",
    "rotate-x",
    "rotate_x",
    "rotation-y",
    "rotation_y",
    "rotate-y",
    "rotate_y",
    "perspective",
    "origin",
    "filter",
];

/// `flex` child `box` keys (Sprint 8 layout, T001): base size, grow
/// share, box paint, and paint order. Positions and windows come from
/// the container, so timing and `x`/`y` live on the forbidden list.
const FLEX_CHILD_KEYS: &[&str] = &[
    "width",
    "height",
    "flex-grow",
    "flex_grow",
    "fill",
    "radius",
    "border-width",
    "border_width",
    "border-color",
    "border_color",
    "stroke-width",
    "stroke_width",
    "stroke-color",
    "stroke_color",
    "stroke-position",
    "stroke_position",
    "opacity",
    "order",
];

/// Container timing, identity, and position keys a flex child must not
/// carry: children inherit the container window, identity, and layout
/// position.
const FLEX_CHILD_FORBIDDEN_KEYS: &[&str] = &[
    "start",
    "duration",
    "id",
    "track",
    "label",
    "hidden",
    "start-after",
    "start_after",
    "start-offset",
    "start_offset",
    "x",
    "y",
    "end",
];

/// Valid layout vocabulary for unknown-name suggestions on the
/// container: canonical hyphenated spellings first, so the suggestion
/// reads canonically.
const FLEX_SUGGEST_KEYS: &[&str] = &[
    "display",
    "flex-direction",
    "gap",
    "padding",
    "margin",
    "align-items",
    "justify-content",
    "flex-grow",
    "width",
    "height",
];

/// Valid layout vocabulary for unknown-name suggestions on a child.
const FLEX_CHILD_SUGGEST_KEYS: &[&str] = &[
    "width",
    "height",
    "flex-grow",
    "fill",
    "radius",
    "border-width",
    "border-color",
    "opacity",
    "order",
];

/// Edit distance between two key spellings (Sprint 8 layout, T001):
/// plain Levenshtein over characters, so `STYLE-NAME-UNKNOWN` can
/// name the closest valid property deterministically.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Closest valid layout property to a misspelled key: the smallest
/// edit distance wins, ties breaking toward the earlier candidate so
/// the suggestion is deterministic.
fn closest_key<'a>(key: &str, candidates: &'a [&'a str]) -> &'a str {
    let mut best = candidates[0];
    let mut best_dist = usize::MAX;
    for c in candidates {
        let d = edit_distance(key, c);
        if d < best_dist {
            best_dist = d;
            best = c;
        }
    }
    best
}

/// One `STYLE-NAME-UNKNOWN` diagnostic for an unknown layout key:
/// names the offending property, the closest valid property, and the
/// full valid set, with a concrete fix.
fn flex_unknown_key(ctx: &mut Ctx, v: &RawValue, key: &str, what: &str, candidates: &[&str]) {
    let near = closest_key(key, candidates);
    ctx.push(
        v.line,
        v.column,
        "STYLE-NAME-UNKNOWN",
        format!("{what} has unknown layout property `{key}`; the closest valid property is `{near}`"),
        format!(
            "remove `{key}`, use `{near}`, or pick one of: {}",
            candidates
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
}

/// One authored `flex` container position (`x`/`y`): absent means
/// `default`; present must be a finite number. Anything else fails
/// with `E_BAD_FLEX` naming the property.
fn flex_num(ctx: &mut Ctx, node: &RawNode, key: &str, default: f64) -> Option<f64> {
    match find(node, key) {
        None => Some(default),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!(
                        "flex container `{key}` must be a number of canvas pixels; found {}",
                        raw_text(v)
                    ),
                    format!("set `{key}=<number>` on the flex container"),
                );
                None
            }
        },
    }
}

/// One non-negative `flex` spacing (`gap`, `padding`, or `margin`):
/// absent means 0; present must be a finite number >= 0. Negative and
/// non-numeric values fail with `E_BAD_FLEX` naming the number rule.
fn flex_spacing(ctx: &mut Ctx, node: &RawNode, hyphen: &str, underscore: &str) -> Option<f64> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(x),
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!("flex container `{hyphen}` {x} is invalid; spacings run 0 and up"),
                    format!("set `{hyphen}=<pixels >= 0>` on the flex container"),
                );
                None
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!(
                        "flex container `{hyphen}` must be a number of canvas pixels; found {}",
                        raw_text(v)
                    ),
                    format!("set `{hyphen}=<pixels >= 0>` on the flex container"),
                );
                None
            }
        },
    }
}

/// The required `flex-direction` of one container (Sprint 8 layout,
/// T001): absent fails naming the property; present must read `row`
/// or `column`, naming the accepted set otherwise.
fn flex_direction(ctx: &mut Ctx, node: &RawNode) -> Option<FlexDirection> {
    let found = find(node, "flex-direction").or_else(|| find(node, "flex_direction"));
    match found {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_FLEX",
                "flex container is missing required `flex-direction`; the container cannot flow without a main axis",
                "add `flex-direction=\"row\"` or `flex-direction=\"column\"` to the flex container",
            );
            None
        }
        Some(v) => match as_str(v).and_then(FlexDirection::parse) {
            Some(d) => Some(d),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!(
                        "flex container `flex-direction` {} is unknown; accepted directions are: {}",
                        raw_text(v),
                        FlexDirection::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "set `flex-direction=\"row\"` or `flex-direction=\"column\"` on the flex container",
                );
                None
            }
        },
    }
}

/// The `align-items` of one container (Sprint 8 layout, T001): absent
/// means `start`; present must read `start`, `center`, `end`, or
/// `stretch`, naming the accepted set otherwise.
fn flex_align(ctx: &mut Ctx, node: &RawNode) -> Option<FlexAlign> {
    let found = find(node, "align-items").or_else(|| find(node, "align_items"));
    match found {
        None => Some(FlexAlign::Start),
        Some(v) => match as_str(v).and_then(FlexAlign::parse) {
            Some(a) => Some(a),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!(
                        "flex container `align-items` {} is unknown; accepted alignments are: {}",
                        raw_text(v),
                        FlexAlign::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "set `align-items=\"start\"`, `\"center\"`, `\"end\"`, or `\"stretch\"` on the flex container",
                );
                None
            }
        },
    }
}

/// The `justify-content` of one container (Sprint 8 layout, T001):
/// absent means `start`; present must read `start`, `center`, or
/// `end`, naming the accepted set otherwise.
fn flex_justify(ctx: &mut Ctx, node: &RawNode) -> Option<FlexJustify> {
    let found = find(node, "justify-content").or_else(|| find(node, "justify_content"));
    match found {
        None => Some(FlexJustify::Start),
        Some(v) => match as_str(v).and_then(FlexJustify::parse) {
            Some(j) => Some(j),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!(
                        "flex container `justify-content` {} is unknown; accepted distributions are: {}",
                        raw_text(v),
                        FlexJustify::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "set `justify-content=\"start\"`, `\"center\"`, or `\"end\"` on the flex container",
                );
                None
            }
        },
    }
}

/// The `flex-grow` share of one child (Sprint 8 layout, T001): absent
/// means 0 (the base size stands); present must be a finite number
/// >= 0. Negative and non-numeric shares fail naming the number rule.
fn flex_grow(ctx: &mut Ctx, node: &RawNode) -> Option<f64> {
    let found = find(node, "flex-grow").or_else(|| find(node, "flex_grow"));
    match found {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(x),
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!("flex child `flex-grow` {x} is invalid; grow shares run 0 and up"),
                    "set `flex-grow=<number >= 0>` on the child box, or remove the key to keep the base size",
                );
                None
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!(
                        "flex child `flex-grow` must be a number; found {}",
                        raw_text(v)
                    ),
                    "set `flex-grow=<number >= 0>` on the child box, or remove the key to keep the base size",
                );
                None
            }
        },
    }
}

/// One laid-out child `box` of a `flex` container (Sprint 8 layout,
/// T001): base size, grow share, box paint, and paint order. Unknown
/// names fail with `STYLE-NAME-UNKNOWN` naming the closest valid
/// property; timing, identity, and position keys fail as inherited
/// from the container; text-effect keys fail as text-only.
fn validate_flex_child(
    ctx: &mut Ctx,
    node: &RawNode,
    styles: &[StyleDecl],
) -> Option<FlexChild> {
    let merged = styled_node(ctx, node, "flex child box", styles)?;
    let node = merged.as_ref();
    // T003 fills apply to `box` clips: a laid-out child box paints one
    // `fill` color, never a gradient or image fill of its own.
    if let Some(child) = node.children.first() {
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_FILL",
            format!(
                "flex child box holds `{}`; gradient and image fills apply to `box` clips, and flex child boxes paint one `fill` color",
                child.name
            ),
            "move the fill to a `box` clip, or remove the child",
        );
        return None;
    }
    for (k, v) in &node.props {
        if FLEX_CHILD_KEYS.contains(&k.as_str()) {
            continue;
        }
        if FLEX_CHILD_FORBIDDEN_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_FLEX",
                format!("flex child box carries `{k}`, but children inherit the container window and layout position"),
                format!("remove `{k}` from the child box; the flex container sets it"),
            );
            continue;
        }
        if FLEX_CHILD_PAINT_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_PAINT",
                format!("flex child box carries `{k}`, but shadows, blends, backdrops, overflow, and masks apply to `box` clips; laid-out children paint one fill with one stroke"),
                format!("move `{k}` onto a `box` clip, or remove the key"),
            );
            continue;
        }
        if TEXT_FX_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("flex child box carries `{k}`, but split, typewriter, and count effects apply to text clips only"),
                "move the effect to a `text` clip, or remove the key",
            );
            continue;
        }
        if RICH_BOX_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("flex child box `{k}` needs T003 fills; T001 child boxes use one fill color, one corner-radius set, one border width with one border color, and one opacity"),
                "use a single `fill`, `radius`, `border-width` with one `border-color`, and `opacity` on the child box",
            ));
            return None;
        }
        if FLEX_MISPLACED_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_FLEX",
                format!("flex child box carries `{k}`, but laid-out boxes carry no 3D turn or filter in T001"),
                format!("remove `{k}` from the child box"),
            );
            continue;
        }
        flex_unknown_key(ctx, v, k, "flex child box", FLEX_CHILD_SUGGEST_KEYS);
    }
    let mut size = |key: &str| -> Option<f64> {
        match find(node, key) {
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_FLEX",
                    format!("flex child box is missing required `{key}`"),
                    format!("add `{key}=<positive pixels>` to the child box"),
                );
                None
            }
            Some(v) => match as_f64(v) {
                Some(val) if val.is_finite() && val > 0.0 => Some(val),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FLEX",
                        format!("flex child box `{key}` must be a positive number of canvas pixels"),
                        format!("set `{key}=<positive pixels>` on the child box"),
                    );
                    None
                }
            },
        }
    };
    let width = size("width")?;
    let height = size("height")?;
    let grow = flex_grow(ctx, node)?;
    let fill = match find(node, "fill") {
        None => Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    "flex child box `fill` is not a `#rgb` or `#rrggbb` color",
                    "use a hex color like `fill=\"#ff0000\"`",
                );
                return None;
            }
        },
    };
    let radius = match find(node, "radius") {
        None => 0.0,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_BOX",
                    "flex child box `radius` must be a non-negative number of canvas pixels",
                    "set `radius=<pixels >= 0>` on the child box",
                );
                return None;
            }
        },
    };
    let border_key = if find(node, "stroke-width").is_some() {
        "stroke-width"
    } else if find(node, "stroke_width").is_some() {
        "stroke_width"
    } else if find(node, "border-width").is_some() {
        "border-width"
    } else {
        "border_width"
    };
    let border_width = match find(node, border_key) {
        None => 0.0,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_BOX",
                    format!(
                        "flex child box `{border_key}` must be a non-negative number of canvas pixels"
                    ),
                    format!("set `{border_key}=<pixels >= 0>` on the child box"),
                );
                return None;
            }
        },
    };
    let color_key = if find(node, "stroke-color").is_some() {
        "stroke-color"
    } else if find(node, "stroke_color").is_some() {
        "stroke_color"
    } else if find(node, "border-color").is_some() {
        "border-color"
    } else {
        "border_color"
    };
    let border_color = match find(node, color_key) {
        None => Rgb { r: 0, g: 0, b: 0 },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    format!(
                        "flex child box `{color_key}` is not a `#rgb` or `#rrggbb` color"
                    ),
                    format!("use a hex color like `{color_key}=\"#000000\"`"),
                );
                return None;
            }
        },
    };
    let stroke_position = match find(node, "stroke-position")
        .or_else(|| find(node, "stroke_position"))
    {
        None => StrokePosition::Inside,
        Some(v) => match as_str(v).and_then(StrokePosition::parse) {
            Some(p) => p,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PAINT",
                    format!(
                        "flex child box `stroke-position` {} is unknown; accepted positions are: {}",
                        raw_text(v),
                        StrokePosition::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    "set `stroke-position=\"center\"`, `\"inside\"`, or `\"outside\"` on the child box",
                );
                return None;
            }
        },
    };
    let opacity = match find(node, "opacity") {
        None => 1.0,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && (0.0..=1.0).contains(&x) => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_BOX",
                    "flex child box `opacity` must run 0 (transparent) to 1 (opaque)",
                    "set `opacity=<number from 0 to 1>` on the child box",
                );
                return None;
            }
        },
    };
    let order = parse_order(ctx, node)?;
    Some(FlexChild {
        width,
        height,
        flex_grow: grow,
        fill,
        radius,
        border_width,
        border_color,
        stroke_position,
        opacity,
        order,
        line: node.name_line,
        column: node.name_column,
    })
}

/// One `flex` container (Sprint 8 layout, CAP-001/T001): the row or
/// column of `box` children laid out by the CSS flex algorithm.
/// Unknown layout names fail with `STYLE-NAME-UNKNOWN` naming the
/// closest valid property; every other layout problem fails with
/// `E_BAD_FLEX` naming the property and its fix. An empty container
/// lays out cleanly; children inherit the container window.
fn validate_flex(ctx: &mut Ctx, node: &RawNode, styles: &[StyleDecl]) -> Option<FlexClip> {
    let merged = styled_node(ctx, node, "flex container", styles)?;
    let node = merged.as_ref();
    for (k, v) in &node.props {
        if FLEX_KEYS.contains(&k.as_str()) {
            continue;
        }
        if RICH_BOX_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("flex container `{k}` needs T003 fills; T001 containers lay out children and carry no paint of their own"),
                "move paint onto a child `box`, or remove the key",
            ));
            return None;
        }
        if FLEX_MISPLACED_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_FLEX",
                format!("flex container carries `{k}`, but containers lay out children and carry no paint, turn, or filter of their own"),
                format!("move `{k}` onto a child `box`, or remove it"),
            );
            continue;
        }
        if PAINT_STACK_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_PAINT",
                format!("flex container carries `{k}`, but containers lay out children and carry no paint; strokes, shadows, blends, backdrops, overflow, and masks apply to `box` clips"),
                format!("move `{k}` onto a `box` clip, or remove the key"),
            );
            continue;
        }
        if TEXT_FX_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("flex container carries `{k}`, but split, typewriter, and count effects apply to text clips only"),
                "move the effect to a `text` clip, or remove the key",
            );
            continue;
        }
        flex_unknown_key(ctx, v, k, "flex container", FLEX_SUGGEST_KEYS);
    }
    if reject_text_fx(ctx, node, "flex container") {
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "flex", "flex container")?;
    // Like stills (text, box, image): `duration` is optional — an
    // omitted one runs until the composition end (`NAN` marks the
    // omission for inference). Children inherit this window.
    let start = clip_start_relaxed(ctx, node, "flex container");
    let duration_parsed = clip_optional_duration(ctx, node, "flex container");
    let (start, duration) = match (start, duration_parsed) {
        (Some(s), Some(Some(d))) => (s, d),
        (Some(s), Some(None)) => (s, f64::NAN),
        _ => return None,
    };
    let x = flex_num(ctx, node, "x", 0.0)?;
    let y = flex_num(ctx, node, "y", 0.0)?;
    // Container size is required and must be positive: a missing,
    // negative, or zero size is refused, never guessed.
    let mut size = |key: &str| -> Option<f64> {
        match find(node, key) {
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_FLEX",
                    format!("flex container is missing required `{key}`"),
                    format!("add `{key}=<positive pixels>` to the flex container"),
                );
                None
            }
            Some(v) => match as_f64(v) {
                Some(val) if val.is_finite() && val > 0.0 => Some(val),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FLEX",
                        format!("flex container `{key}` must be a positive number of canvas pixels"),
                        format!("set `{key}=<positive pixels>` on the flex container"),
                    );
                    None
                }
            },
        }
    };
    let width = size("width")?;
    let height = size("height")?;
    match find(node, "display") {
        None => {}
        Some(v) => match as_str(v) {
            Some("flex") => {}
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FLEX",
                    format!(
                        "flex container `display` must read `\"flex\"`; found {}",
                        raw_text(v)
                    ),
                    "set `display=\"flex\"` on the flex container, or remove the key",
                );
                return None;
            }
        },
    }
    let direction = flex_direction(ctx, node)?;
    let gap = flex_spacing(ctx, node, "gap", "gap")?;
    let padding = flex_spacing(ctx, node, "padding", "padding")?;
    let margin = flex_spacing(ctx, node, "margin", "margin")?;
    let align_items = flex_align(ctx, node)?;
    let justify_content = flex_justify(ctx, node)?;
    let order = parse_order(ctx, node)?;
    let mut children = Vec::with_capacity(node.children.len());
    for child in &node.children {
        if child.name == "flex" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_FLEX",
                "flex container holds a nested `flex` container; T001 lays out one level of `box` children",
                "lay out `box` children inside the flex container, or lift the inner container out",
            );
            return None;
        }
        if child.name != "box" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_FLEX",
                format!(
                    "flex container holds `{}`; T001 lays out `box` children",
                    child.name
                ),
                "lay out `box` children inside the flex container",
            );
            return None;
        }
        children.push(validate_flex_child(ctx, child, styles)?);
    }
    Some(FlexClip {
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        x,
        y,
        width,
        height,
        direction,
        gap,
        padding,
        margin,
        align_items,
        justify_content,
        order,
        children,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Image file extensions the renderer decodes (Sprint 2 PNG/JPEG plus
/// Sprint 5 still GIF and HEIC/HEIF). Anything else (SVG and every
/// other format) is refused as not-yet-supported with its enabling
/// sprint, never rendered guessed. An animated GIF keeps its extension
/// here so it reaches the animated-content refusal pointing at `video`.
fn image_format_supported(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".heic")
        || lower.ends_with(".heif")
}

/// True when `path` names an SVG picture (Sprint 8 look, CAP-005/T005):
/// the `.svg` extension, case-insensitive. SVG sources rasterize at
/// the declared size through the same `image` and `svg` clips as
/// raster pictures.
fn svg_format_supported(path: &str) -> bool {
    path.to_lowercase().ends_with(".svg")
}

/// Parse SVG bytes once (Sprint 8 look, CAP-005/T005): UTF-8 text
/// holding an SVG tree. The `Err` carries the human cause for the
/// diagnostic; text in the file without system fonts still parses —
/// only glyphs need fonts, never the tree itself.
fn parse_svg_source(bytes: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("it is not UTF-8 text ({e})"))?;
    usvg::Tree::from_str(text, &usvg::Options::default())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn image_magic_ok(bytes: &[u8]) -> bool {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    // PNG signature, or a JPEG start-of-image marker.
    (bytes.len() >= 8 && &bytes[..8] == PNG)
        || (bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8)
}

/// True when `bytes` look like a HEIC/HEIF still (Sprint 5 T007/T008):
/// an ISO-BMFF `ftyp` box whose major brand or one of its compatible
/// brands is a HEIF brand. The decoder below still gates the pixels;
/// this only decides the file is worth handing to it.
fn heic_magic_ok(bytes: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 10] = [
        b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"msf1",
    ];
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    if BRANDS.iter().any(|b| b[..] == bytes[8..12]) {
        return true;
    }
    let mut pos = 16;
    while pos + 4 <= bytes.len() {
        if BRANDS.iter().any(|b| b[..] == bytes[pos..pos + 4]) {
            return true;
        }
        pos += 4;
        // Brands live at the head of the box; a long scan means these
        // are pixels, not brands.
        if pos > 64 {
            break;
        }
    }
    false
}

/// What a byte sniff of a GIF source reports (Sprint 5 T007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GifInfo {
    /// Image frames seen, capped at 2: still vs animated is all the
    /// footage contract needs.
    frames: u32,
    /// True when a NETSCAPE application extension declares loop count 0
    /// (play forever). An absent or nonzero loop count means one
    /// playthrough (or a finite repeat): a finite native length.
    infinite: bool,
    /// True when the walk reached the trailer: the whole input was seen.
    complete: bool,
}

/// Sniff GIF bytes (Sprint 5 T007): header, frame count (still vs
/// animated), and the NETSCAPE infinite-loop flag. Walks block
/// boundaries — never a substring search, since compressed pixels can
/// contain any byte value. Stops counting after the second frame but
/// keeps walking for the loop flag until the trailer. Reads at most
/// `limit` bytes, so huge movie files only pay for their head.
/// Returns None when the bytes are not GIF at all.
fn gif_info(bytes: &[u8], limit: usize) -> Option<GifInfo> {
    if bytes.len() < 6 || (&bytes[..6] != b"GIF87a" && &bytes[..6] != b"GIF89a") {
        return None;
    }
    let mut info = GifInfo {
        frames: 0,
        infinite: false,
        complete: false,
    };
    let end = bytes.len().min(limit);
    let mut pos = 6;
    // Logical screen descriptor: 7 bytes, then an optional global
    // color table.
    if pos + 7 > end {
        return Some(info);
    }
    let packed = bytes[pos + 4];
    pos += 7;
    if packed & 0x80 != 0 {
        let gct = 3usize.saturating_mul(1usize << ((packed & 0x07) + 1));
        if pos + gct > end {
            return Some(info);
        }
        pos += gct;
    }
    loop {
        if pos >= end {
            break;
        }
        match bytes[pos] {
            0x3B => {
                info.complete = true;
                break;
            }
            0x21 => {
                // Extension: `21 label` plus sub-blocks to the zero
                // terminator. Only the NETSCAPE application extension
                // matters here; everything else is skipped.
                if pos + 2 > end {
                    break;
                }
                let label = bytes[pos + 1];
                if label == 0xFF {
                    // `21 FF 0B "NETSCAPE2.0" 03 01 lo hi`: loop count 0
                    // plays forever.
                    let p = pos + 2;
                    if p < end {
                        let size = bytes[p] as usize;
                        if size == 11 && p + 12 <= end && &bytes[p + 1..p + 12] == b"NETSCAPE2.0" {
                            let q = p + 12;
                            if q + 4 <= end && bytes[q] == 3 && bytes[q + 1] == 1 {
                                let loops = (bytes[q + 2] as u16) | ((bytes[q + 3] as u16) << 8);
                                if loops == 0 {
                                    info.infinite = true;
                                }
                            }
                        }
                    }
                }
                let mut p = pos + 2;
                loop {
                    if p >= end {
                        break;
                    }
                    let size = bytes[p] as usize;
                    p += 1;
                    if size == 0 {
                        break;
                    }
                    p += size;
                    if p > end {
                        p = end;
                        break;
                    }
                }
                pos = p;
            }
            0x2C => {
                // Image descriptor: `2C` plus 9 bytes, an optional local
                // color table, LZW minimum code size, and image-data
                // sub-blocks to the zero terminator.
                info.frames = (info.frames + 1).min(2);
                if pos + 10 > end {
                    break;
                }
                let packed = bytes[pos + 9];
                pos += 10;
                if packed & 0x80 != 0 {
                    let lct = 3usize.saturating_mul(1usize << ((packed & 0x07) + 1));
                    if pos + lct > end {
                        break;
                    }
                    pos += lct;
                }
                if pos + 1 > end {
                    break;
                }
                pos += 1;
                loop {
                    if pos >= end {
                        break;
                    }
                    let size = bytes[pos] as usize;
                    pos += 1;
                    if size == 0 {
                        break;
                    }
                    pos += size;
                    if pos > end {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
    Some(info)
}

/// Read at most `limit` head bytes of a media file for content sniffing
/// (Sprint 5 T007): enough for the GIF header, color tables, and the
/// NETSCAPE loop flag, without paging a whole movie into memory.
/// Returns None when the file cannot be read here; the probe below
/// still gates the source, so a missed sniff never renders a guess.
fn read_head(abs: &std::path::Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(abs).ok()?;
    let mut buf = Vec::new();
    file.take(limit as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn validate_image(ctx: &mut Ctx, node: &RawNode, styles: &[StyleDecl]) -> Option<ImageClip> {
    let merged = styled_node(ctx, node, "image clip", styles)?;
    let node = merged.as_ref();
    const KEYS: &[&str] = &[
        "path",
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "x",
        "y",
        "width",
        "height",
        "fit",
        "placement",
        "order",
        "rotation-x",
        "rotation_x",
        "rotate-x",
        "rotate_x",
        "rotation-y",
        "rotation_y",
        "rotate-y",
        "rotate_y",
        "perspective",
        "origin",
        "filter",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && !TEXT_FX_KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown image key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    if reject_text_fx(ctx, node, "image clip") {
        return None;
    }
    if reject_spans(ctx, node, "image clip") {
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "image", "image clip")?;
    // Sprint 6 T003: `duration` is optional — an omitted one runs until
    // the composition end (`NAN` marks the omission for inference).
    // Start and duration report independently (like the old shared
    // window check): a clip missing both names both problems at once.
    let start = clip_start_relaxed(ctx, node, "image clip");
    let duration_parsed = clip_optional_duration(ctx, node, "image clip");
    let (start, duration) = match (start, duration_parsed) {
        (Some(s), Some(Some(d))) => (s, d),
        (Some(s), Some(None)) => (s, f64::NAN),
        _ => return None,
    };
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "image clip is missing required `path`",
                "add `path=\"media/photo.png\"` to the image clip",
            );
            return None;
        }
    };
    // Presence and readability come before format: a missing file names
    // the file, an unsupported format names the file and its sprint.
    let abs = ctx.folder.join(&path);
    if !abs.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("image source {path:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `path`",
        );
        return None;
    }
    // SVG pictures ride the same clip (Sprint 8 look, CAP-005/T005):
    // they rasterize at the declared size instead of decoding.
    let is_svg = svg_format_supported(&path);
    if !is_svg && !image_format_supported(&path) {
        ctx.diags.push(Diagnostic::new(
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "E_UNSUPPORTED",
            format!("image source {path:?} needs a later sprint: Framesmith decodes PNG, JPEG, still GIF, HEIC still images, and SVG pictures only"),
            "point `path` at a `.png`, `.jpg`, `.jpeg`, `.gif` (still), `.heic`, `.heif`, or `.svg` file, or wait for a later footage sprint",
        ));
        return None;
    }
    match std::fs::read(&abs) {
        Ok(bytes) => {
            if is_svg {
                if let Err(detail) = parse_svg_source(&bytes) {
                    ctx.push(
                        node.name_line,
                        node.name_column,
                        "E_BAD_IMAGE",
                        format!("image source {path:?} is corrupt or not a readable SVG picture ({detail})"),
                        "restore a readable SVG picture at the referenced path",
                    );
                    return None;
                }
            // An animated GIF never belongs in `image` (Sprint 5 T007):
            // it plays in a `video` clip. Content-based, so a renamed
            // file is refused the same way.
            } else if let Some(gif) = gif_info(&bytes, bytes.len()) {
                if gif.frames > 1 {
                    ctx.push(
                        node.name_line,
                        node.name_column,
                        "E_BAD_IMAGE",
                        format!("image source {path:?} is an animated GIF; animations play in a `video` clip, not `image`"),
                        format!("use `video path={path:?} ...` to play the animation, or point `image` at a still picture"),
                    );
                    return None;
                }
                if gif.frames == 0 || !gif.complete {
                    ctx.push(
                        node.name_line,
                        node.name_column,
                        "E_BAD_IMAGE",
                        format!("image source {path:?} is corrupt or not a readable GIF picture"),
                        "restore an undamaged still GIF file at the referenced path",
                    );
                    return None;
                }
            } else if !image_magic_ok(&bytes) && !heic_magic_ok(&bytes) {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_IMAGE",
                    format!("image source {path:?} is corrupt or not a PNG/JPEG/GIF-still/HEIC picture"),
                    "restore an undamaged PNG, JPEG, still GIF, or HEIC file at the referenced path",
                );
                return None;
            }
        }
        Err(e) => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_UNREADABLE",
                format!("image source {path:?} cannot be read: {e}"),
                "make the image file readable or fix the `path`",
            );
            return None;
        }
    }
    let mut pos = |key: &str, default: f64| -> Option<f64> {
        match find(node, key) {
            None => Some(default),
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => Some(x),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_IMAGE",
                        format!("image `{key}` must be a number of canvas pixels"),
                        format!("set `{key}=<number>` on the image clip"),
                    );
                    None
                }
            },
        }
    };
    let x = pos("x", 0.0)?;
    let y = pos("y", 0.0)?;
    let mut extent = |key: &str| -> Option<Option<f64>> {
        match find(node, key) {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(val) if val.is_finite() && val > 0.0 => Some(Some(val)),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_IMAGE",
                        format!("image `{key}` must be a positive number of canvas pixels"),
                        format!(
                            "set `{key}=<positive pixels>` on the image clip or remove the key"
                        ),
                    );
                    None
                }
            },
        }
    };
    let width = extent("width")?;
    let height = extent("height")?;
    let fit = match find(node, "fit") {
        None => Fit::Contain,
        Some(v) => match as_str(v).and_then(Fit::parse) {
            Some(f) => f,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FIT",
                    "image `fit` must be exactly one of `contain`, `cover`, `fill`",
                    "set `fit=\"contain\"`, `\"cover\"`, or `\"fill\"`",
                );
                return None;
            }
        },
    };
    let placement = match find(node, "placement") {
        None => Placement::center(),
        Some(v) => match as_str(v).and_then(Placement::parse) {
            Some(p) => p,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_IMAGE",
                    "image `placement` must be a horizontal and vertical placement like `\"center\"`, `\"top-left\"`, or `\"left top\"`",
                    "set `placement=\"center\"` or a pair such as `placement=\"top-left\"`",
                );
                return None;
            }
        },
    };
    let order = parse_order(ctx, node)?;
    let turn = clip_turn(ctx, node, "image clip")?;
    let filter_blur = clip_filter(ctx, node, "image clip")?;
    Some(ImageClip {
        path,
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        x,
        y,
        width,
        height,
        fit,
        placement,
        order,
        turn,
        filter_blur,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Display name of a `path` clip for paint diagnostics (Sprint 8 look,
/// T005): the clip kind plus its `id` when declared, so every refusal
/// names the offending element and property.
fn path_element(identity_id: &Option<String>) -> String {
    match identity_id {
        Some(id) => format!("path clip {id:?}"),
        None => "path clip".to_string(),
    }
}

/// One `path` number with a default (Sprint 8 look, T005): present
/// must be finite, else the diagnostic names the element and the key.
fn path_num(ctx: &mut Ctx, node: &RawNode, el: &str, key: &str, default: f64) -> Option<f64> {
    match find(node, key) {
        None => Some(default),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PATH",
                    format!("{el} `{key}` must be a number of canvas pixels"),
                    format!("set `{key}=<number>` on {el}"),
                );
                None
            }
        },
    }
}

/// One `stroke-width` value of a `path` clip (Sprint 8 look, T005):
/// absent means no stroke; present must be finite and non-negative.
/// A negative width fails with `E_BAD_PAINT` naming the element and
/// the property, like boxes.
fn path_stroke_width(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<f64> {
    let key = if find(node, "stroke-width").is_some() {
        "stroke-width"
    } else {
        "stroke_width"
    };
    let width = path_num(ctx, node, el, key, 0.0)?;
    if width < 0.0 {
        let v = find(node, key).unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_PAINT",
            format!("{el} `{key}` {width} is invalid; stroke widths cannot be negative"),
            format!("set `{key}=<pixels >= 0>` on {el}"),
        );
        return None;
    }
    Some(width)
}

/// One `stroke-color` value of a `path` clip (Sprint 8 look, T005):
/// absent means black; present must read `#rgb`/`#rrggbb`.
fn path_stroke_color(ctx: &mut Ctx, node: &RawNode, el: &str) -> Option<Rgb> {
    let key = if find(node, "stroke-color").is_some() {
        "stroke-color"
    } else {
        "stroke_color"
    };
    match find(node, key) {
        None => Some(Rgb { r: 0, g: 0, b: 0 }),
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => Some(c),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    format!("{el} `{key}` is not a `#rgb` or `#rrggbb` color"),
                    format!("use a hex color like `{key}=\"#000000\"`"),
                );
                None
            }
        },
    }
}

/// Validate one `path` clip (Sprint 8 look, CAP-005/T005): an
/// SVG-subset vector shape with a fill rule and the shared stroke
/// set, routed through the shared paint stack. Unparsable `d` fails
/// naming the 1-based failing command index; every paint problem
/// names its element and property before frame one.
fn validate_path(ctx: &mut Ctx, node: &RawNode, styles: &[StyleDecl]) -> Option<PathClip> {
    let merged = styled_node(ctx, node, "path clip", styles)?;
    let node = merged.as_ref();
    const KEYS: &[&str] = &[
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "x",
        "y",
        "d",
        "fill",
        "fill-rule",
        "fill_rule",
        "stroke-width",
        "stroke_width",
        "stroke-color",
        "stroke_color",
        "stroke-position",
        "stroke_position",
        "box-shadow",
        "box_shadow",
        "mix-blend-mode",
        "mix_blend_mode",
        "backdrop-blur",
        "backdrop_blur",
        "backdrop-brightness",
        "backdrop_brightness",
        "overflow",
        "mask",
        "clip-path",
        "clip_path",
        "opacity",
        "order",
        "rotation-x",
        "rotation_x",
        "rotate-x",
        "rotate_x",
        "rotation-y",
        "rotation_y",
        "rotate-y",
        "rotate_y",
        "perspective",
        "origin",
        "filter",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && !TEXT_FX_KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown path key {k:?}"),
                "remove the key or check the spelling against `d`, `fill`, `fill-rule`, `stroke-width`, `stroke-color`, and `stroke-position`",
            );
        }
    }
    if reject_text_fx(ctx, node, "path clip") {
        return None;
    }
    if reject_spans(ctx, node, "path clip") {
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "path", "path clip")?;
    let start = clip_start_relaxed(ctx, node, "path clip");
    let duration_parsed = clip_optional_duration(ctx, node, "path clip");
    let (start, duration) = match (start, duration_parsed) {
        (Some(s), Some(Some(d))) => (s, d),
        (Some(s), Some(None)) => (s, f64::NAN),
        _ => return None,
    };
    let el = path_element(&identity.id);
    let x = path_num(ctx, node, &el, "x", 0.0)?;
    let y = path_num(ctx, node, &el, "y", 0.0)?;
    let d = match find(node, "d") {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "path clip is missing required `d`; the shape draws its path data",
                "add `d=\"M<x>,<y> ...\"` to the path clip",
            );
            return None;
        }
        Some(v) => match as_str(v) {
            Some(s) => s.to_string(),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PATH",
                    format!("{el} `d` is not readable path data; paths are quoted strings like `\"M10,10 L90,10\"`"),
                    format!("set `d=\"M<x>,<y> ...\"` on {el}"),
                );
                return None;
            }
        },
    };
    if let Err(e) = motion::path::parse_indexed(&d) {
        let v = find(node, "d").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_PATH",
            format!("{el} `d` {e}"),
            format!("fix the `d` path data on {el}"),
        );
        return None;
    }
    let fill = match find(node, "fill") {
        None => Rgb { r: 0, g: 0, b: 0 },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    format!("{el} `fill` is not a `#rgb` or `#rrggbb` color"),
                    format!("use a hex color like `fill=\"#000000\"` on {el}"),
                );
                return None;
            }
        },
    };
    let fill_rule = match find(node, "fill-rule").or_else(|| find(node, "fill_rule")) {
        None => FillRule::NonZero,
        Some(v) => match as_str(v).and_then(FillRule::parse) {
            Some(r) => r,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PATH",
                    format!(
                        "{el} `fill-rule` {} is unknown; accepted rules are: {}",
                        raw_text(v),
                        FillRule::ACCEPTED
                            .iter()
                            .map(|s| format!("`{s}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    format!("set `fill-rule=\"nonzero\"` or `\"evenodd\"` on {el}"),
                );
                return None;
            }
        },
    };
    let border_width = path_stroke_width(ctx, node, &el)?;
    let border_color = path_stroke_color(ctx, node, &el)?;
    let stroke_position = box_stroke_position(ctx, node, &el)?;
    let box_shadow = box_shadow_opt(ctx, node, &el)?;
    let blend_mode = box_blend_mode(ctx, node, &el)?;
    let (backdrop_blur, backdrop_brightness) = box_backdrop(ctx, node, &el)?;
    let overflow = box_overflow(ctx, node, &el)?;
    let mask = box_mask_ref(ctx, node, "mask", "mask", &el)?;
    let clip_path = box_mask_ref(ctx, node, "clip-path", "clip_path", &el)?;
    let opacity = match find(node, "opacity") {
        None => 1.0,
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() => x,
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_PATH",
                    format!("{el} `opacity` must be a number"),
                    format!("set `opacity=<number from 0 to 1>` on {el}"),
                );
                return None;
            }
        },
    };
    if !(0.0..=1.0).contains(&opacity) {
        let (line, col) = find(node, "opacity")
            .map(|v| (v.line, v.column))
            .unwrap_or((node.name_line, node.name_column));
        ctx.push(
            line,
            col,
            "E_BAD_PATH",
            format!("{el} `opacity` {opacity} is invalid; opacity runs 0 (transparent) to 1 (opaque)"),
            format!("set `opacity=<number from 0 to 1>` on {el}"),
        );
        return None;
    }
    for child in &node.children {
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_PATH",
            format!("path clip holds `{}`; a path carries no children", child.name),
            "remove the child; the shape draws its `d` data",
        );
    }
    if !node.children.is_empty() {
        return None;
    }
    let order = parse_order(ctx, node)?;
    let turn = clip_turn(ctx, node, "path clip")?;
    let filter_blur = clip_filter(ctx, node, "path clip")?;
    Some(PathClip {
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        x,
        y,
        d,
        fill,
        fill_rule,
        border_width,
        border_color,
        stroke_position,
        box_shadow,
        blend_mode,
        backdrop_blur,
        backdrop_brightness,
        overflow,
        mask,
        clip_path,
        opacity,
        order,
        turn,
        filter_blur,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Display name of an `svg` clip for diagnostics (Sprint 8 look,
/// T005): the clip kind plus its `id` when declared.
fn svg_element(identity_id: &Option<String>) -> String {
    match identity_id {
        Some(id) => format!("svg clip {id:?}"),
        None => "svg clip".to_string(),
    }
}

/// Validate one `svg` clip (Sprint 8 look, CAP-005/T005): a standalone
/// `.svg` file rasterized at its declared `width`/`height`, composed
/// with `fit` and `placement` like the `image` clip. A missing file
/// names the file; an unreadable file names the file and the cause;
/// a corrupt file names the file and the parse cause; a missing,
/// zero, or negative extent names the rule and the fix — all before
/// frame one.
fn validate_svg(ctx: &mut Ctx, node: &RawNode, styles: &[StyleDecl]) -> Option<SvgClip> {
    let merged = styled_node(ctx, node, "svg clip", styles)?;
    let node = merged.as_ref();
    const KEYS: &[&str] = &[
        "path",
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "x",
        "y",
        "width",
        "height",
        "fit",
        "placement",
        "order",
        "rotation-x",
        "rotation_x",
        "rotate-x",
        "rotate_x",
        "rotation-y",
        "rotation_y",
        "rotate-y",
        "rotate_y",
        "perspective",
        "origin",
        "filter",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && !TEXT_FX_KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown svg key {k:?}"),
                "remove the key or check the spelling against `path`, `width`, `height`, `fit`, and `placement`",
            );
        }
    }
    if reject_text_fx(ctx, node, "svg clip") {
        return None;
    }
    if reject_spans(ctx, node, "svg clip") {
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "svg", "svg clip")?;
    let start = clip_start_relaxed(ctx, node, "svg clip");
    let duration_parsed = clip_optional_duration(ctx, node, "svg clip");
    let (start, duration) = match (start, duration_parsed) {
        (Some(s), Some(Some(d))) => (s, d),
        (Some(s), Some(None)) => (s, f64::NAN),
        _ => return None,
    };
    let el = svg_element(&identity.id);
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "svg clip is missing required `path`",
                "add `path=\"media/logo.svg\"` to the svg clip",
            );
            return None;
        }
    };
    if !svg_format_supported(&path) {
        let v = find(node, "path").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_IMAGE",
            format!("{el} source {path:?} must name a `.svg` file; an svg clip rasterizes a standalone SVG picture"),
            format!("point `path` at a `.svg` file on {el}, or use an `image` clip for raster pictures"),
        );
        return None;
    }
    let abs = ctx.folder.join(&path);
    if !abs.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("svg source {path:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `path`",
        );
        return None;
    }
    match std::fs::read(&abs) {
        Ok(bytes) => {
            if let Err(detail) = parse_svg_source(&bytes) {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_IMAGE",
                    format!("svg source {path:?} is corrupt or not a readable SVG picture ({detail})"),
                    "restore a readable SVG picture at the referenced path",
                );
                return None;
            }
        }
        Err(e) => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_UNREADABLE",
                format!("svg source {path:?} cannot be read: {e}"),
                "make the SVG file readable or fix the `path`",
            );
            return None;
        }
    }
    let mut pos = |key: &str| -> Option<f64> {
        match find(node, key) {
            None => Some(0.0),
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => Some(x),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_IMAGE",
                        format!("{el} `{key}` must be a number of canvas pixels"),
                        format!("set `{key}=<number>` on {el}"),
                    );
                    None
                }
            },
        }
    };
    let x = pos("x")?;
    let y = pos("y")?;
    let mut extent = |key: &str| -> Option<f64> {
        match find(node, key) {
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_IMAGE",
                    format!("{el} is missing required `{key}`; an svg rasterizes at its declared size"),
                    format!("add `{key}=<positive pixels>` to {el}"),
                );
                None
            }
            Some(v) => match as_f64(v) {
                Some(val) if val.is_finite() && val > 0.0 => Some(val),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_IMAGE",
                        format!("{el} `{key}` must be a positive number of canvas pixels"),
                        format!("set `{key}=<positive pixels>` on {el}"),
                    );
                    None
                }
            },
        }
    };
    let width = extent("width")?;
    let height = extent("height")?;
    let fit = match find(node, "fit") {
        None => Fit::Contain,
        Some(v) => match as_str(v).and_then(Fit::parse) {
            Some(f) => f,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FIT",
                    format!("{el} `fit` must be exactly one of `contain`, `cover`, `fill`"),
                    format!("set `fit=\"contain\"`, `\"cover\"`, or `\"fill\"` on {el}"),
                );
                return None;
            }
        },
    };
    let placement = match find(node, "placement") {
        None => Placement::center(),
        Some(v) => match as_str(v).and_then(Placement::parse) {
            Some(p) => p,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_IMAGE",
                    format!("{el} `placement` must be a horizontal and vertical placement like `\"center\"`, `\"top-left\"`, or `\"left top\"`"),
                    format!("set `placement=\"center\"` on {el}"),
                );
                return None;
            }
        },
    };
    for child in &node.children {
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_IMAGE",
            format!("svg clip holds `{}`; an svg carries no children", child.name),
            "remove the child; the picture comes from `path`",
        );
    }
    if !node.children.is_empty() {
        return None;
    }
    let order = parse_order(ctx, node)?;
    let turn = clip_turn(ctx, node, "svg clip")?;
    let filter_blur = clip_filter(ctx, node, "svg clip")?;
    Some(SvgClip {
        path,
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        x,
        y,
        width,
        height,
        fit,
        placement,
        order,
        turn,
        filter_blur,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Parse one Sprint 6 `nest` trim (`trim-start`/`trim-end`, either
/// spelling): absent means the default (0 for the start, the child end
/// for the end); present must be finite seconds from 0. Returns None
/// on a parse failure after reporting it, so callers keep collecting
/// the clip's remaining timing problems.
fn nest_trim(ctx: &mut Ctx, node: &RawNode, hyphen: &str, underscore: &str) -> Option<Option<f64>> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(None),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(Some(x)),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!(
                        "nest `{hyphen}` {} must be seconds into the child timeline from 0",
                        raw_text(v)
                    ),
                    format!("set `{hyphen}=<seconds >= 0>` or remove the key"),
                );
                None
            }
        },
    }
}

/// Parse the Sprint 6 `nest` `rate`: absent means unity; present must
/// be a finite number from 0.1 to 10, like footage. Returns None on
/// failure after reporting it.
fn nest_rate(ctx: &mut Ctx, node: &RawNode) -> Option<f64> {
    match find(node, "rate") {
        None => Some(1.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && (0.1..=10.0).contains(&x) => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!(
                        "nest `rate` {} is outside the accepted range 0.1 to 10",
                        raw_text(v)
                    ),
                    "set `rate=<0.1 to 10>` (for example `rate=2` for double speed, `rate=0.5` for half speed), or remove the key for unity rate",
                );
                None
            }
        },
    }
}

/// Validate one Sprint 6 `nest` clip (T001/T007): a named composition
/// reuse with an absolute half-open window plus `id`/`track`/`label`
/// and per-use `trim-start`, `trim-end`, `rate`, and `order`. The
/// `nest` property must name a composition; existence and kind resolve
/// project-wide after every clip validates, so one bad value still
/// yields exactly one diagnostic. Child-relative checks (trim past the
/// child end), cycles, and depth run in [`check_nest_graph`] once every
/// composition end is known.
fn validate_nest(ctx: &mut Ctx, node: &RawNode) -> Option<NestClip> {
    const KEYS: &[&str] = &[
        "nest",
        "start",
        "duration",
        "id",
        "track",
        "label",
        "hidden",
        "start-after",
        "start_after",
        "start-offset",
        "start_offset",
        "trim-start",
        "trim_start",
        "trim-end",
        "trim_end",
        "rate",
        "order",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && !TEXT_FX_KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown nest key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    if reject_text_fx(ctx, node, "nest clip") {
        return None;
    }
    if reject_spans(ctx, node, "nest clip") {
        return None;
    }
    let identity = parse_clip_identity(ctx, node, "nest", "nest clip")?;
    // Start and duration report independently (like the old shared
    // window check): a clip missing both names both problems at once.
    let start = clip_start_relaxed(ctx, node, "nest clip");
    // Sprint 6 T003: a nest without a written `duration` is unbounded,
    // so it is refused until `duration` is written — never guessed from
    // the child.
    let duration_parsed = clip_optional_duration(ctx, node, "nest clip");
    let (start, duration_parsed) = match (start, duration_parsed) {
        (Some(s), Some(d)) => (s, d),
        _ => return None,
    };
    let duration = match duration_parsed {
        Some(d) => d,
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_TIMING",
                "nest clip omits `duration`; an unbounded nest has no finite length until `duration` is written",
                "add `duration=<seconds>` to the nest clip",
            );
            return None;
        }
    };
    // Per-use timing (T007): the child window, rate, and order
    // validate here with one diagnostic per bad value; the child end
    // itself is only known after every composition resolves, so
    // past-end trims wait for `check_nest_graph`.
    let ts = nest_trim(ctx, node, "trim-start", "trim_start");
    let te = nest_trim(ctx, node, "trim-end", "trim_end");
    let rate_opt = nest_rate(ctx, node);
    let order_opt = parse_order(ctx, node);
    let timing_bad = ts.is_none() || te.is_none() || rate_opt.is_none() || order_opt.is_none();
    let trim_start = ts.flatten().unwrap_or(0.0);
    let trim_end = te.flatten();
    let rate = rate_opt.unwrap_or(1.0);
    let order = order_opt.unwrap_or(0);
    if ts.is_some() && te.is_some() {
        if let Some(end) = trim_end {
            if end < trim_start {
                let v = find(node, "trim-end")
                    .or_else(|| find(node, "trim_end"))
                    .unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!(
                        "nest trim window {trim_start}..{end} runs backward; trims are seconds into the child timeline"
                    ),
                    "set `trim-end` after `trim-start` for a forward window",
                );
                return None;
            } else if end == trim_start {
                let v = find(node, "trim-end")
                    .or_else(|| find(node, "trim_end"))
                    .unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!(
                        "nest trim window {trim_start}..{end} is empty; the use would show no content"
                    ),
                    "set `trim-end` after `trim-start` for a positive window",
                );
                return None;
            }
        }
    }
    if timing_bad {
        return None;
    }
    let (target, line, column) = match parse_ref(ctx, node, "nest", "nest clip")? {
        Some(t) => t,
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "nest clip is missing required `nest`; every nest names one composition",
                "add `nest=\"<composition name or id>\"` to the nest clip",
            );
            return None;
        }
    };
    ctx.refs.push(RefEntry {
        kind: "nest",
        target: target.clone(),
        line,
        column,
        owner_id: identity.id.clone(),
        owner_line: node.name_line,
        needs_composition: true,
        comp: ctx.current_comp,
    });
    Some(NestClip {
        target,
        start,
        duration,
        id: identity.id,
        track: identity.track,
        start_after: identity.start_after,
        label: identity.label,
        hidden: identity.hidden,
        trim_start,
        trim_end,
        rate,
        order,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Check every `duck-under` voice, every overlap handoff, every effect
/// tail, and every pause map once all composition windows are known
/// (Sprint 10 sound, CAP-003 through CAP-006): a voice naming a mark,
/// a composition, or another composition's clip refuses as
/// `E_UNKNOWN_REF` (unknown ids already refused by the project-wide
/// reference check, so this stays silent on them instead of
/// cascading); a handoff whose overlap runs past the composition end
/// reports its clamp for review, never as a diagnostic, so clamping
/// stays a valid verdict; a delay/reverb tail running past the end
/// reports its truncation the same way; and each `detect-silence`
/// clip reports its silent ranges, which never block. Clips that
/// failed validation are absent here, so a missing voice on an already
/// invalid project stays silent (its own diagnostic already covers
/// it). A duplicated voice id also stays silent here (already refused
/// as `E_DUP_ID`).
fn check_sound_windows(ctx: &mut Ctx, compositions: &[Composition]) {
    for (ci, comp) in compositions.iter().enumerate() {
        for clip in &comp.clips {
            let duck = match clip {
                Clip::Audio(a) => a.duck_under.as_ref(),
                Clip::Video(v) => v.duck_under.as_ref(),
                _ => None,
            };
            let Some(duck) = duck else { continue };
            // Every namespace entry answering this voice, in document order.
            let hits: Vec<&IdEntry> = ctx.ids.iter().filter(|e| e.value == duck.target).collect();
            if hits.is_empty() {
                continue;
            }
            if hits.len() > 1 {
                continue;
            }
            let hit = hits[0];
            let what = match clip {
                Clip::Audio(_) => "audio track",
                _ => "video clip",
            };
            if hit.is_mark || hit.is_composition {
                ctx.push(
                    duck.line,
                    duck.column,
                    "E_UNKNOWN_REF",
                    format!(
                        "{what} `duck-under` {:?} names {}, but a voice must be a clip in the same composition",
                        duck.target,
                        if hit.is_mark {
                            "a mark"
                        } else {
                            "a composition"
                        },
                    ),
                    format!(
                        "point `duck-under=` at a clip `id` in {:?}, not {:?}",
                        comp.name, duck.target
                    ),
                );
                continue;
            }
            // The entry stamps its owning composition: a voice elsewhere
            // never renders with this bed. A same-composition voice
            // missing from the finished clips failed validation itself,
            // so this stays silent (its own diagnostic already covers
            // it) instead of misnaming the problem.
            if hit.comp != ci {
                ctx.push(
                    duck.line,
                    duck.column,
                    "E_UNKNOWN_REF",
                    format!(
                        "{what} `duck-under` {:?} names a clip in another composition, but this clip is in composition {:?}; a voice must share the bed's timeline",
                        duck.target, comp.name
                    ),
                    format!(
                        "point `duck-under=` at a clip `id` in {:?}, or move the voice there",
                        comp.name
                    ),
                );
            }
        }
        for over in notation::model::sound_overlaps(&comp.clips, comp.duration) {
            if !over.clamped {
                continue;
            }
            let late = &comp.clips[over.late];
            let (line, column) = late.line_col();
            ctx.crossfade_clamps.push(CrossfadeClamp {
                early: over.early,
                late: over.late,
                shape: over.shape.name(),
                raw_start: over.raw_start,
                raw_end: over.raw_end,
                eff_start: over.eff_start,
                eff_end: over.eff_end,
                line,
                column,
            });
        }
        check_effect_tails(ctx, comp);
        check_silences(ctx.folder, ctx, comp);
        check_waveform_windows(ctx, comp);
    }
}

/// Review every on-canvas sound range (Sprint 10 sound, CAP-010,
/// REC-002): empty or backward written ranges fail naming the bounds,
/// ranges past the clip window clamp and report, and a range fully
/// outside the window fails with nothing to draw. Hidden clips render
/// nothing and report nothing.
fn check_waveform_windows(ctx: &mut Ctx, comp: &Composition) {
    if !comp.duration.is_finite() || comp.duration <= 0.0 {
        return;
    }
    for (idx, clip) in comp.clips.iter().enumerate() {
        if clip.hidden() {
            continue;
        }
        let (draw, kind) = match clip {
            Clip::Audio(a) if a.waveform.enabled => (&a.waveform, "audio"),
            Clip::Video(v) if v.waveform.enabled => (&v.waveform, "video"),
            _ => continue,
        };
        let what = match kind {
            "audio" => "audio track",
            _ => "video clip",
        };
        match resolve_waveform_range(
            clip.start(),
            clip.duration(),
            comp.duration,
            draw.from,
            draw.to,
        ) {
            Ok(range) => {
                if !range.clamped {
                    continue;
                }
                let (line, column) = clip.line_col();
                ctx.waveform_clamps.push(WaveformClamp {
                    clip: idx,
                    id: clip.id().map(str::to_string),
                    kind,
                    raw_from: range.raw_from,
                    raw_to: range.raw_to,
                    eff_from: range.eff_from,
                    eff_to: range.eff_to,
                    line,
                    column,
                });
            }
            Err(WaveformRangeError::EmptyWindow) => continue,
            Err(WaveformRangeError::EmptyOrBackward) => {
                let (line, column) = draw
                    .to_loc
                    .or(draw.from_loc)
                    .unwrap_or((draw.line, draw.column));
                let from_text = draw
                    .from
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "clip start".to_string());
                let to_text = draw
                    .to
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "clip end".to_string());
                ctx.push(
                    line,
                    column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} drawn range {from_text}s..{to_text}s is empty or backward; the drawing needs `waveform-from` below `waveform-to`"
                    ),
                    "set `waveform-from=<seconds>` below `waveform-to=<seconds>`, or remove both keys to draw the whole clip window",
                );
            }
            Err(WaveformRangeError::OutsideWindow) => {
                let end = (clip.start() + clip.duration()).min(comp.duration);
                ctx.push(
                    draw.line,
                    draw.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} drawn range {}s..{}s lies fully outside the clip window {:.3}s..{:.3}s; nothing would draw",
                        draw.from.unwrap_or(clip.start()),
                        draw.to.unwrap_or(end),
                        clip.start().max(0.0),
                        end,
                    ),
                    "move `waveform-from`/`waveform-to` inside the clip window, or remove both keys to draw the whole clip window",
                );
            }
        }
    }
}

/// Report every delay/reverb tail truncated at the composition end
/// (Sprint 10 sound, CAP-005, REC-003): the natural tail (echo taps at
/// exact delay multiples, then the fixed 30 ms room comb) renders to
/// the composition end and cuts the rest. Tails that fit inside the
/// composition stay silent; hidden clips render nothing and report
/// nothing.
fn check_effect_tails(ctx: &mut Ctx, comp: &Composition) {
    use notation::model::{delay_tail_frames, reverb_tail_frames, EFFECT_RATE};
    if !comp.duration.is_finite() {
        return;
    }
    for (idx, clip) in comp.clips.iter().enumerate() {
        if clip.hidden() {
            continue;
        }
        let fx = match notation::model::clip_fx(clip) {
            Some(f) => f,
            None => continue,
        };
        let delay_tail =
            delay_tail_frames(fx.delay, fx.delay_feedback, fx.delay_mix) as f64 / EFFECT_RATE;
        let reverb_tail = reverb_tail_frames(fx.reverb) as f64 / EFFECT_RATE;
        if delay_tail + reverb_tail <= 0.0 {
            continue;
        }
        let start = clip.start();
        let end = (start + clip.duration()).min(comp.duration);
        if !end.is_finite() || end <= start || start >= comp.duration {
            continue;
        }
        let raw_end = end + delay_tail + reverb_tail;
        if raw_end > comp.duration {
            let (line, column) = clip.line_col();
            ctx.effect_tails.push(EffectTail {
                clip: idx,
                id: clip.id().map(str::to_string),
                delay_tail,
                reverb_tail,
                raw_end,
                eff_end: comp.duration,
                line,
                column,
            });
        }
    }
}

/// Report one pause map per `detect-silence` clip (Sprint 10 sound,
/// CAP-006): silent ranges at or below the clip's dBFS ceiling for at
/// least its minimum seconds, measured from the decoded source window
/// (trimmed and re-timed exactly like the mix, before gains). Detection
/// reads only — the mix never sees these numbers. A clip FFmpeg cannot
/// decode (or no FFmpeg on PATH) reports nothing instead of blocking:
/// the render decodes the same window and names a real failure there.
fn check_silences(folder: &Path, ctx: &mut Ctx, comp: &Composition) {
    if !comp.duration.is_finite() || comp.duration <= 0.0 {
        return;
    }
    for (idx, clip) in comp.clips.iter().enumerate() {
        let fx = match notation::model::clip_fx(clip) {
            Some(f) if f.detect_silence => f,
            _ => continue,
        };
        let (path, trim_start, trim_end, rate, kind) = match clip {
            Clip::Audio(a) => (a.path.as_str(), a.trim_start, a.trim_end, a.rate, "audio"),
            Clip::Video(v) => (v.path.as_str(), v.trim_start, v.trim_end, v.rate, "video"),
            _ => continue,
        };
        let start = clip.start();
        let end = (start + clip.duration()).min(comp.duration);
        if !end.is_finite() || end <= start || start >= comp.duration {
            continue;
        }
        let mut take = end - start;
        if let Some(e) = trim_end {
            take = take.min(((e - trim_start).max(0.0)) / rate);
        }
        if take <= 0.0 {
            continue;
        }
        let loop_unit = match clip {
            Clip::Video(v) => v.loop_clip,
            _ => false,
        };
        let Some(pcm) = decode_detect_source(folder, path, trim_start, take * rate) else {
            continue;
        };
        let mut pcm = resample_detect(&pcm, rate);
        if loop_unit && !pcm.is_empty() {
            let want = (take * DETECT_RATE).round().max(1.0) as usize;
            pcm = tile_detect(&pcm, want);
        }
        let want = ((take * DETECT_RATE).round().max(1.0) as usize).min(pcm.len() / 2);
        let ranges = scan_silence(
            &pcm[..want * 2],
            start,
            fx.silence_threshold_db,
            fx.silence_min,
        );
        let (line, column) = clip.line_col();
        ctx.silences.push(SilenceReport {
            clip: idx,
            id: clip.id().map(str::to_string),
            kind,
            threshold_db: fx.silence_threshold_db,
            min_seconds: fx.silence_min,
            ranges,
            line,
            column,
        });
    }
}

/// Sample rate of pause detection (Sprint 10 sound, CAP-006): the mix
/// rate, so detected ranges name composition seconds directly.
const DETECT_RATE: f64 = 48_000.0;

/// Decode one detection window to 48 kHz stereo s16 (Sprint 10 sound,
/// CAP-006): the same FFmpeg shape the mix decode uses (`-ss` after
/// `-i`, exact seconds), so ranges measure the samples the mix would
/// play. Anything unusable — no FFmpeg on PATH, an unreadable file, a
/// sourceless video, a failed decode — is `None`, and the caller skips
/// the clip instead of blocking a valid project.
fn decode_detect_source(folder: &Path, rel: &str, trim_start: f64, take: f64) -> Option<Vec<i16>> {
    if !take.is_finite() || take <= 0.0 || !trim_start.is_finite() || trim_start < 0.0 {
        return None;
    }
    let abs = folder.join(rel);
    if !abs.is_file() {
        return None;
    }
    let ffmpeg = tool_on_path("ffmpeg")?;
    let out = std::process::Command::new(ffmpeg)
        .args([
            "-xerror",
            "-v",
            "error",
            "-i",
            &abs.display().to_string(),
            "-ss",
            &format!("{trim_start:.6}"),
            "-t",
            &format!("{take:.6}"),
            "-ar",
            "48000",
            "-ac",
            "2",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let (pairs, _) = out.stdout.as_chunks::<2>();
    if pairs.is_empty() {
        return None;
    }
    Some(pairs.iter().map(|b| i16::from_le_bytes(*b)).collect())
}

/// Re-time one detection window by `rate` (Sprint 10 sound, CAP-006):
/// the same linear re-timing the mix uses, so pause edges land on the
/// composition seconds the mix plays. Unity rate returns the input
/// unchanged.
fn resample_detect(pcm: &[i16], rate: f64) -> Vec<i16> {
    if !rate.is_finite() || rate <= 0.0 {
        return Vec::new();
    }
    if (rate - 1.0).abs() < 1e-12 {
        return pcm.to_vec();
    }
    const CHANNELS: usize = 2;
    let frames = pcm.len() / CHANNELS;
    if frames == 0 {
        return Vec::new();
    }
    let out_frames = ((frames as f64) / rate).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_frames * CHANNELS);
    for j in 0..out_frames {
        let pos = (j as f64) * rate;
        let i0 = (pos.floor() as usize).min(frames - 1);
        let i1 = (i0 + 1).min(frames - 1);
        let frac = (pos - pos.floor()) as f32;
        for c in 0..CHANNELS {
            let a = pcm[i0 * CHANNELS + c] as f32;
            let b = pcm[i1 * CHANNELS + c] as f32;
            out.push(
                (a + (b - a) * frac)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16,
            );
        }
    }
    out
}

/// Tile one detection window unit to `frames` composition frames
/// (Sprint 10 sound, CAP-006): looping clips repeat their source
/// window in the mix, so pauses scan the same tiling.
fn tile_detect(unit: &[i16], frames: usize) -> Vec<i16> {
    const CHANNELS: usize = 2;
    let unit_frames = unit.len() / CHANNELS;
    if unit_frames == 0 || frames == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(frames * CHANNELS);
    for j in 0..frames {
        let s = (j % unit_frames) * CHANNELS;
        out.extend_from_slice(&unit[s..s + CHANNELS]);
    }
    out
}

/// Scan stereo s16 frames for pauses (Sprint 10 sound, CAP-006): a
/// frame is silent when both channels sit at or below the dBFS ceiling
/// (`-inf` counts only exact digital zero). Runs of at least
/// `min_seconds` report as composition-second ranges in time order.
/// Pure sample math, so reruns agree exactly.
fn scan_silence(pcm: &[i16], clip_start: f64, threshold_db: f64, min_secs: f64) -> Vec<(f64, f64)> {
    const CHANNELS: usize = 2;
    let ceiling = if threshold_db == f64::NEG_INFINITY {
        0.0
    } else {
        10f64.powf(threshold_db / 20.0)
    };
    let frames = pcm.len() / CHANNELS;
    let need = ((min_secs * DETECT_RATE).round() as usize).max(1);
    let mut out = Vec::new();
    let mut run: Option<usize> = None;
    for i in 0..frames {
        let peak = (pcm[i * CHANNELS] as i32)
            .abs()
            .max((pcm[i * CHANNELS + 1] as i32).abs()) as f64
            / 32768.0;
        if peak <= ceiling {
            if run.is_none() {
                run = Some(i);
            }
        } else if let Some(s) = run.take() {
            if i - s >= need {
                out.push((
                    clip_start + s as f64 / DETECT_RATE,
                    clip_start + i as f64 / DETECT_RATE,
                ));
            }
        }
    }
    if let Some(s) = run {
        if frames - s >= need {
            out.push((
                clip_start + s as f64 / DETECT_RATE,
                clip_start + frames as f64 / DETECT_RATE,
            ));
        }
    }
    out
}

/// Deepest allowed `nest` chain (Sprint 6 T007, REC-004), shared with
/// the renderer through the notation model.
pub use notation::model::MAX_NEST_DEPTH;

/// Resolve a `nest` target to a composition index by `name` or `id`.
/// Returns `None` for unknown names and clip ids (both already refused
/// by the project-wide reference check), so the graph walk stays
/// silent on them instead of cascading.
fn nest_target_index(compositions: &[Composition], target: &str) -> Option<usize> {
    compositions
        .iter()
        .position(|c| c.name == target || c.id.as_deref() == Some(target))
}

/// Display name for a composition inside a `nest` chain diagnostic:
/// its `name`, which answers references.
fn nest_chain_name(compositions: &[Composition], index: usize) -> String {
    format!("{:?}", compositions[index].name)
}

/// Check every `nest` use once all composition ends are known (Sprint 6
/// T007): a `trim-start` at or past the child end refuses, a circular
/// chain refuses naming the chain, and a use chain deeper than
/// [`MAX_NEST_DEPTH`] refuses naming the chain and the limit. Unknown
/// targets stay silent (already refused); compositions with no knowable
/// end skip the trim check (their own diagnostic already covers it).
fn check_nest_graph(ctx: &mut Ctx, compositions: &[Composition]) {
    // Trim against the now-known child end.
    for comp in compositions {
        for clip in &comp.clips {
            let Clip::Nest(n) = clip else { continue };
            let Some(ti) = nest_target_index(compositions, &n.target) else {
                continue;
            };
            let child_end = compositions[ti].duration;
            if !child_end.is_finite() || child_end <= 0.0 {
                continue;
            }
            if n.trim_start >= child_end {
                ctx.push(
                    n.line,
                    n.column,
                    "E_BAD_TIMING",
                    format!(
                        "nest {:?} `trim-start` {} is at or past the child composition {:?} end {child_end}; the use would show no content",
                        n.target, n.trim_start, compositions[ti].name,
                    ),
                    format!(
                        "set `trim-start` below {child_end}, or point `nest` at a longer composition"
                    ),
                );
            }
        }
    }
    // Cycles and depth: one DFS over the composition use-graph in
    // document order. A back edge names its cycle; a path longer than
    // the limit names the chain and the limit. Memoized depths keep the
    // DAG walk linear; cyclic nodes stay unmemoized so a second path
    // through them still names its own cycle.
    let n = compositions.len();
    let mut adj: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); n];
    for (ci, comp) in compositions.iter().enumerate() {
        for clip in &comp.clips {
            let Clip::Nest(nc) = clip else { continue };
            if let Some(ti) = nest_target_index(compositions, &nc.target) {
                adj[ci].push((ti, nc.line, nc.column));
            }
        }
    }
    // Memoized depths keep the DAG walk linear. The memo also carries
    // the deepest witness chain from each node so an over-deep use
    // names its full chain, not just the top edge. Cyclic nodes return
    // a zero depth without memoizing, so a second path through them
    // still names its own cycle.
    let mut depth_memo: Vec<Option<(usize, Vec<usize>)>> = vec![None; n];
    let mut reported_cycles: Vec<Vec<usize>> = Vec::new();
    let mut reported_depth = false;
    #[allow(clippy::too_many_arguments)]
    fn dfs(
        ctx: &mut Ctx,
        compositions: &[Composition],
        adj: &[Vec<(usize, usize, usize)>],
        depth_memo: &mut [Option<(usize, Vec<usize>)>],
        reported_cycles: &mut Vec<Vec<usize>>,
        reported_depth: &mut bool,
        node: usize,
        stack: &mut Vec<usize>,
        in_stack: &mut [bool],
    ) -> (usize, Vec<usize>) {
        if let Some((d, chain)) = depth_memo[node].clone() {
            return (d, chain);
        }
        if in_stack[node] {
            return (0, vec![node]);
        }
        in_stack[node] = true;
        stack.push(node);
        let mut best: (usize, Vec<usize>) = (0, vec![node]);
        for (ti, line, column) in adj[node].clone() {
            if in_stack[ti] {
                let pos = stack.iter().position(|&j| j == ti).unwrap_or(0);
                let mut chain: Vec<String> = stack[pos..]
                    .iter()
                    .map(|&j| nest_chain_name(compositions, j))
                    .collect();
                chain.push(nest_chain_name(compositions, ti));
                let cycle: Vec<usize> = stack[pos..]
                    .iter()
                    .copied()
                    .chain(std::iter::once(ti))
                    .collect();
                if !reported_cycles.contains(&cycle) {
                    reported_cycles.push(cycle);
                    ctx.push(
                        line,
                        column,
                        "E_BAD_TIMING",
                        format!(
                            "`nest` chain cycles: {}; a composition cannot nest itself through other compositions",
                            chain.join(" -> ")
                        ),
                        "break the cycle so every `nest` chain ends at a composition that nests nothing",
                    );
                }
                continue;
            }
            let (child_depth, child_chain) = dfs(
                ctx,
                compositions,
                adj,
                depth_memo,
                reported_cycles,
                reported_depth,
                ti,
                stack,
                in_stack,
            );
            let via = child_depth + 1;
            if via > best.0 {
                let mut witness = vec![node];
                witness.extend(child_chain.iter().copied());
                best = (via, witness);
            }
            if via > MAX_NEST_DEPTH && !*reported_depth {
                *reported_depth = true;
                let mut witness = vec![node];
                witness.extend(child_chain);
                let chain: Vec<String> = witness
                    .iter()
                    .map(|&j| nest_chain_name(compositions, j))
                    .collect();
                ctx.push(
                    line,
                    column,
                    "E_BAD_TIMING",
                    format!(
                        "`nest` chain {} nests {via} deep, past the {MAX_NEST_DEPTH}-level limit; nesting deeper than {MAX_NEST_DEPTH} levels refuses",
                        chain.join(" -> "),
                    ),
                    format!(
                        "flatten the nesting to at most {MAX_NEST_DEPTH} levels (reuse the composition fewer times deep, or split the chain)"
                    ),
                );
            }
        }
        stack.pop();
        in_stack[node] = false;
        depth_memo[node] = Some(best.clone());
        best
    }
    for root in 0..n {
        let mut stack: Vec<usize> = Vec::new();
        let mut in_stack = vec![false; n];
        dfs(
            ctx,
            compositions,
            &adj,
            &mut depth_memo,
            &mut reported_cycles,
            &mut reported_depth,
            root,
            &mut stack,
            &mut in_stack,
        );
    }
}

/// Display name of a clip for mask diagnostics (Sprint 8 look, T004):
/// its quoted `id`, or `box #<index>` for an anonymous box.
fn mask_clip_name(comp: &Composition, index: usize) -> String {
    match &comp.clips[index] {
        Clip::Box(b) => match &b.id {
            Some(id) => format!("{id:?}"),
            None => format!("box #{index}"),
        },
        _ => format!("clip #{index}"),
    }
}

/// Kind name of a clip for mask-target diagnostics (Sprint 8 look, T004).
fn mask_clip_kind(clip: &Clip) -> &'static str {
    match clip {
        Clip::Text(_) => "text",
        Clip::Video(_) => "video",
        Clip::Box(_) => "box",
        Clip::Image(_) => "image",
        Clip::Path(_) => "path",
        Clip::Svg(_) => "svg",
        Clip::Flex(_) => "flex",
        Clip::Audio(_) => "audio",
        Clip::Nest(_) => "nest",
    }
}

/// Check every `mask` / `clip-path` reference once all clips are known
/// (Sprint 8 look, T004): a reference naming no clip in the same
/// composition dangles, a reference naming a non-`box` clip misses the
/// rounded-rect shape, and a reference cycle refuses naming the cycle
/// path through every reference in the chain. Forward references
/// resolve here, so declaration order never matters. Unknown targets
/// stay silent when the name is duplicated project-wide (already
/// refused as `E_DUP_ID`).
fn check_mask_graph(ctx: &mut Ctx, compositions: &[Composition]) {
    use std::collections::{HashMap, HashSet};
    // Project-wide duplicate ids, for cascade suppression (the
    // duplicate itself is already refused).
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for comp in compositions {
        for clip in &comp.clips {
            if let Some(id) = clip.id() {
                *counts.entry(id).or_insert(0) += 1;
            }
        }
        if let Some(id) = comp.id.as_deref() {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    for comp in compositions {
        // First box clip per id wins, matching render resolution;
        // duplicates are already refused above.
        let mut by_id: HashMap<&str, usize> = HashMap::new();
        for (i, clip) in comp.clips.iter().enumerate() {
            if let Some(id) = clip.id() {
                by_id.entry(id).or_insert(i);
            }
        }
        // Dangling and non-box targets refuse at the reference site.
        // Edges resolve only between box clips for the cycle walk;
        // `path` clips join the reference check (their targets must be
        // boxes too) but never take part in a cycle, since no edge
        // points at a non-box clip.
        let mut adj: Vec<Vec<(usize, usize, usize, String)>> = vec![Vec::new(); comp.clips.len()];
        for (i, clip) in comp.clips.iter().enumerate() {
            let (el, mask, clip_path) = match clip {
                Clip::Box(b) => (paint_element(&b.id), &b.mask, &b.clip_path),
                Clip::Path(p) => (path_element(&p.id), &p.mask, &p.clip_path),
                _ => continue,
            };
            for (key, r) in [("mask", mask), ("clip-path", clip_path)] {
                let Some(r) = r else { continue };
                if counts.get(r.target.as_str()).copied().unwrap_or(0) > 1 {
                    continue;
                }
                match by_id.get(r.target.as_str()) {
                    None => {
                        ctx.push(
                            r.line,
                            r.column,
                            "E_BAD_PAINT",
                            format!(
                                "{el} `{key}` names {:?}; no clip in this composition is called {:?}",
                                r.target, r.target
                            ),
                            format!(
                                "point `{key}` at the `id` of a `box` clip in the same composition, or remove the key"
                            ),
                        );
                    }
                    Some(&ti) if !matches!(comp.clips[ti], Clip::Box(_)) => {
                        ctx.push(
                            r.line,
                            r.column,
                            "E_BAD_PAINT",
                            format!(
                                "{el} `{key}` names {:?}, which is a `{}` clip; masks reference the rounded rect of a `box` clip",
                                r.target,
                                mask_clip_kind(&comp.clips[ti])
                            ),
                            format!(
                                "point `{key}` at the `id` of a `box` clip in the same composition, or remove the key"
                            ),
                        );
                    }
                    Some(&ti) => {
                        adj[i].push((ti, r.line, r.column, key.to_string()));
                    }
                }
            }
        }
        // Cycles: one DFS over the mask edges in document order. A
        // back edge names its cycle through every reference in the
        // chain, reported at the edge that closes it.
        let mut reported: HashSet<Vec<usize>> = HashSet::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut in_stack = vec![false; comp.clips.len()];
        fn dfs(
            ctx: &mut Ctx,
            comp: &Composition,
            adj: &[Vec<(usize, usize, usize, String)>],
            reported: &mut HashSet<Vec<usize>>,
            stack: &mut Vec<usize>,
            in_stack: &mut [bool],
            node: usize,
        ) {
            in_stack[node] = true;
            stack.push(node);
            for (ti, line, column, _key) in adj[node].clone() {
                if in_stack[ti] {
                    let pos = stack.iter().position(|&j| j == ti).unwrap_or(0);
                    let cycle: Vec<usize> = stack[pos..]
                        .iter()
                        .copied()
                        .chain(std::iter::once(ti))
                        .collect();
                    if reported.insert(cycle.clone()) {
                        let chain: Vec<String> = cycle
                            .iter()
                            .map(|&j| mask_clip_name(comp, j))
                            .collect();
                        ctx.push(
                            line,
                            column,
                            "E_BAD_PAINT",
                            format!(
                                "`mask`/`clip-path` cycle: {}; masked content renders self-first, so a cycle has no first layer",
                                chain.join(" -> ")
                            ),
                            "break the cycle so every `mask`/`clip-path` chain ends at an unmasked `box` clip",
                        );
                    }
                    continue;
                }
                dfs(ctx, comp, adj, reported, stack, in_stack, ti);
            }
            stack.pop();
            in_stack[node] = false;
        }
        for root in 0..comp.clips.len() {
            if matches!(comp.clips[root], Clip::Box(_)) {
                dfs(ctx, comp, &adj, &mut reported, &mut stack, &mut in_stack, root);
            }
        }
    }
}

/// Parse a `tween`/`key`/`frame` easing value (T003 plus the Sprint 7
/// motion set): the nineteen named curves, parameterized `out-back(n)`,
/// and the full curve set (`cubic-bezier`, `steps`, `spring`, `bounce`,
/// `elastic`, `wiggle`, `hold`). Ease names accept underscore aliases
/// (`in_quad`, `out_back(1.7)`, `cubic_bezier(...)`); parameters must be
/// numeric. Anything else is a validation error, never a guessed curve.
enum EaseParse {
    Ok(EaseDecl),
    /// Unknown name or malformed parameters.
    Bad(String),
}

/// Split an easing value into its dash-normalized name and optional
/// parenthesized parameters. `None` means unbalanced parentheses.
fn split_ease_call(t: &str) -> Option<(String, Option<String>)> {
    match t.find('(') {
        None => Some((t.replace('_', "-"), None)),
        Some(i) => {
            let name = t[..i].trim().replace('_', "-");
            let inner = t[i + 1..].trim().strip_suffix(')')?;
            Some((name, Some(inner.to_string())))
        }
    }
}

/// One finite number parameter, with surrounding whitespace allowed.
fn ease_number(text: &str) -> Option<f64> {
    let x: f64 = text.trim().parse().ok()?;
    x.is_finite().then_some(x)
}

/// One positive whole-count parameter (`steps(n)`, `wiggle(n)`).
fn ease_count(text: &str) -> Option<u32> {
    let x: f64 = text.trim().parse().ok()?;
    if x.is_finite() && x.fract() == 0.0 && x >= 1.0 && x <= u32::MAX as f64 {
        Some(x as u32)
    } else {
        None
    }
}

fn parse_ease_value(s: &str) -> EaseParse {
    let t = s.trim();
    let (name, params) = match split_ease_call(t) {
        Some(split) => split,
        None => {
            return EaseParse::Bad(format!("easing {t:?} has unbalanced parentheses"));
        }
    };
    // Bare curves take no parameters: `linear(...)` is malformed, never
    // a different curve.
    macro_rules! bare {
        ($decl:expr) => {
            match params {
                None => EaseParse::Ok($decl),
                Some(_) => EaseParse::Bad(format!(
                    "easing {t:?} takes no parameters; write `ease=\"{name}\"`"
                )),
            }
        };
    }
    match name.as_str() {
        "linear" => bare!(EaseDecl::Linear),
        "in-quad" => bare!(EaseDecl::InQuad),
        "out-quad" => bare!(EaseDecl::OutQuad),
        "in-out-quad" => bare!(EaseDecl::InOutQuad),
        "in-cubic" => bare!(EaseDecl::InCubic),
        "out-cubic" => bare!(EaseDecl::OutCubic),
        "in-out-cubic" => bare!(EaseDecl::InOutCubic),
        "in-quart" => bare!(EaseDecl::InQuart),
        "out-quart" => bare!(EaseDecl::OutQuart),
        "in-out-quart" => bare!(EaseDecl::InOutQuart),
        "in-quint" => bare!(EaseDecl::InQuint),
        "out-quint" => bare!(EaseDecl::OutQuint),
        "in-out-quint" => bare!(EaseDecl::InOutQuint),
        "in-sine" => bare!(EaseDecl::InSine),
        "out-sine" => bare!(EaseDecl::OutSine),
        "in-out-sine" => bare!(EaseDecl::InOutSine),
        "in-expo" => bare!(EaseDecl::InExpo),
        "out-expo" => bare!(EaseDecl::OutExpo),
        "in-out-expo" => bare!(EaseDecl::InOutExpo),
        "bounce" => bare!(EaseDecl::Bounce),
        "elastic" => bare!(EaseDecl::Elastic),
        "hold" => bare!(EaseDecl::Hold),
        "out-back" => match params {
            // `out-back` without a parameter, or a malformed one, is a
            // validation error, never a guessed overshoot.
            Some(inner) => match ease_number(&inner) {
                Some(n) if n >= 0.0 => EaseParse::Ok(EaseDecl::OutBack(n)),
                _ => EaseParse::Bad(format!(
                    "easing {t:?} needs a numeric overshoot, e.g. `ease=\"out-back(1.7)\"`"
                )),
            },
            None => EaseParse::Bad(format!(
                "easing {t:?} needs a numeric overshoot, e.g. `ease=\"out-back(1.7)\"`"
            )),
        },
        "cubic-bezier" => match params {
            Some(inner) => {
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() != 4 {
                    EaseParse::Bad(format!(
                        "easing {t:?} needs four numbers, e.g. `ease=\"cubic-bezier(0.25,0.1,0.25,1)\"`"
                    ))
                } else {
                    match (
                        ease_number(parts[0]),
                        ease_number(parts[1]),
                        ease_number(parts[2]),
                        ease_number(parts[3]),
                    ) {
                        (Some(x1), Some(y1), Some(x2), Some(y2))
                            if (0.0..=1.0).contains(&x1) && (0.0..=1.0).contains(&x2) =>
                        {
                            EaseParse::Ok(EaseDecl::CubicBezier(x1, y1, x2, y2))
                        }
                        _ => EaseParse::Bad(format!(
                            "easing {t:?} needs four numbers `cubic-bezier(x1,y1,x2,y2)` with x1 and x2 from 0 to 1, e.g. `ease=\"cubic-bezier(0.25,0.1,0.25,1)\"`"
                        )),
                    }
                }
            }
            None => EaseParse::Bad(format!(
                "easing {t:?} needs four numbers, e.g. `ease=\"cubic-bezier(0.25,0.1,0.25,1)\"`"
            )),
        },
        "steps" => match params {
            Some(inner) => match ease_count(&inner) {
                Some(n) => EaseParse::Ok(EaseDecl::Steps(n)),
                _ => EaseParse::Bad(format!(
                    "easing {t:?} needs a positive whole step count, e.g. `ease=\"steps(4)\"`"
                )),
            },
            None => EaseParse::Bad(format!(
                "easing {t:?} needs a positive whole step count, e.g. `ease=\"steps(4)\"`"
            )),
        },
        "spring" => match params {
            None => EaseParse::Ok(EaseDecl::Spring(
                motion::SPRING_DEFAULT_STIFFNESS,
                motion::SPRING_DEFAULT_DAMPING,
            )),
            Some(inner) => {
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() != 2 {
                    EaseParse::Bad(format!(
                        "easing {t:?} needs `spring` or two numbers, e.g. `ease=\"spring\"` or `ease=\"spring(100,5)\"`"
                    ))
                } else {
                    match (ease_number(parts[0]), ease_number(parts[1])) {
                        (Some(s), Some(d)) if s > 0.0 && d >= 0.0 => {
                            EaseParse::Ok(EaseDecl::Spring(s, d))
                        }
                        _ => EaseParse::Bad(format!(
                            "easing {t:?} needs `spring` or `spring(stiffness,damping)` with a positive stiffness and a non-negative damping, e.g. `ease=\"spring(100,5)\"`"
                        )),
                    }
                }
            }
        },
        "wiggle" => match params {
            None => EaseParse::Ok(EaseDecl::Wiggle(motion::WIGGLE_DEFAULT_COUNT)),
            Some(inner) => match ease_count(&inner) {
                Some(n) => EaseParse::Ok(EaseDecl::Wiggle(n)),
                _ => EaseParse::Bad(format!(
                    "easing {t:?} needs `wiggle` or a positive whole wiggle count, e.g. `ease=\"wiggle(3)\"`"
                )),
            },
        },
        _ => EaseParse::Bad(format!("unknown easing {t:?}")),
    }
}

/// Shared `ease` value check for `tween`, `key`, and `frame`: parses the
/// full curve set, reporting `E_BAD_EASING` naming the accepted forms on
/// anything else. Returns `None` after reporting (the caller marks its
/// own failure and falls back), `Some(None)` when no `ease` is written,
/// and `Some(Some(curve))` when one parses.
fn motion_ease_opt(ctx: &mut Ctx, node: &RawNode) -> Option<Option<EaseDecl>> {
    match find(node, "ease").and_then(as_str) {
        None => Some(None),
        Some(s) => match parse_ease_value(s) {
            EaseParse::Ok(e) => Some(Some(e)),
            EaseParse::Bad(detail) => {
                let v = find(node, "ease").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_EASING",
                    format!("{detail}; accepted easings are: {}", accepted_easings()),
                    "set `ease=` to a named curve or `ease=\"out-back(1.7)\"`",
                );
                None
            }
        },
    }
}

/// Comma-joined accepted easing spellings for diagnostics.
fn accepted_easings() -> String {
    let mut names: Vec<&str> = notation::model::EaseDecl::NAMED.to_vec();
    names.push("out-back(n)");
    names.push("cubic-bezier(x1,y1,x2,y2)");
    names.push("steps(n)");
    names.push("spring");
    names.push("spring(stiffness,damping)");
    names.push("bounce");
    names.push("elastic");
    names.push("wiggle");
    names.push("wiggle(n)");
    names.push("hold");
    names.join(", ")
}

/// Shared motion target check (T003): a required bare integer naming a
/// clip by document order. Missing, non-integer, negative, or
/// out-of-range targets are validation errors, never guessed clips.
fn motion_target(ctx: &mut Ctx, node: &RawNode, what: &str, clip_count: usize) -> Option<usize> {
    match find(node, "target") {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("{what} is missing required `target`; every directive names one clip"),
                "add `target=<clip index>` (0 names the first clip in the document)",
            );
            None
        }
        Some(v) => match &v.kind {
            ValueKind::Int(n) if *n >= 0 && (*n as usize) < clip_count => Some(*n as usize),
            ValueKind::Int(n) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "{what} target {n} names no clip; the composition declares {clip_count} clip(s), indexed from 0"
                    ),
                    "set `target=` to a clip index from 0 to the last clip",
                );
                None
            }
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `target` must be a whole clip index, indexed from 0"),
                    "set `target=<clip index>` (0 names the first clip in the document)",
                );
                None
            }
        },
    }
}

/// Canonical motion property name (Sprint 7 motion, CAP-005/006, plus
/// Sprint 10 sound envelopes): the hyphenated spelling with
/// underscores normalized, aliases resolved (`rotate-x` to
/// `rotation-x`, `size` to `font-size`, `fill-color` to `fill`,
/// `border-radius` to `radius`). Returns the canonical
/// `MOTION_PROPERTIES` entry, or the `SOUND_PROPERTIES` entry
/// (`volume`, `pan`) for sound envelopes, or `None` when the spelling
/// names no animatable property.
fn canonical_property(s: &str) -> Option<&'static str> {
    let norm = s.replace('_', "-");
    if let Some(found) = notation::model::MOTION_PROPERTIES
        .iter()
        .find(|p| **p == norm)
    {
        return Some(found);
    }
    if let Some(found) = notation::model::SOUND_PROPERTIES
        .iter()
        .find(|p| **p == norm)
    {
        return Some(found);
    }
    notation::model::MOTION_ALIASES
        .iter()
        .find(|(alias, _)| *alias == norm)
        .map(|(_, canonical)| *canonical)
}

/// Whether a canonical property rides a sound envelope (Sprint 10
/// sound, CAP-001): `volume` and `pan` animate per sample on
/// sound-bearing clips, never through the visual engine.
fn property_is_sound(property: &str) -> bool {
    notation::model::SOUND_PROPERTIES.contains(&property)
}

/// Refuse visual-only directive keys on sound envelopes (Sprint 10
/// sound, CAP-001): `volume`/`pan` ride one plain window evaluated per
/// sample — no cascade (`stagger`), no replay (`repeat`, `yoyo`,
/// `repeat-delay`), and no per-frame expression (`expr`). Every
/// present key reports with its fix, never silently ignored; `set`
/// needs no guard (its keys are all envelope-meaningful).
fn sound_directive_guards(ctx: &mut Ctx, node: &RawNode, what: &str, property: &str) -> bool {
    if !property_is_sound(property) {
        return true;
    }
    let mut ok = true;
    for key in [
        "expr",
        "stagger",
        "stagger-from",
        "stagger_from",
        "stagger-grid",
        "stagger_grid",
        "repeat",
        "yoyo",
        "repeat-delay",
        "repeat_delay",
    ] {
        if let Some(v) = find(node, key) {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("{what} `{key}` rides visual motion; a `{property}` sound envelope plays one plain window per sample, with no cascade, replay, or expression"),
                format!(
                    "remove `{key}` from the {what}, or animate a visual property to cascade it"
                ),
            );
            ok = false;
        }
    }
    ok
}

/// Whether a canonical motion property exists on a target clip kind
/// (Sprint 7 motion, CAP-005/006, plus Sprint 10 sound envelopes): the
/// geometric properties, the 3D turns, and blur ride every clip like
/// before, but attribute properties need the clip carrying them —
/// sizes on boxes and images, fill/radius/border on boxes, font
/// metrics on text, and sound envelopes (`volume`, `pan`) on
/// sound-bearing clips (`audio` tracks and `video` clips). Anything
/// else is a refusal naming the clip kind, never a silent no-op.
fn property_kind_ok(property: &str, clip: &Clip) -> bool {
    match property {
        "width" | "height" => {
            matches!(clip, Clip::Box(_) | Clip::Image(_) | Clip::Flex(_) | Clip::Svg(_))
        }
        "fill" | "border-width" | "border-color" => {
            matches!(clip, Clip::Box(_) | Clip::Path(_))
        }
        "radius" => matches!(clip, Clip::Box(_)),
        "font-size" | "letter-spacing" => matches!(clip, Clip::Text(_)),
        "volume" | "pan" => matches!(clip, Clip::Audio(_) | Clip::Video(_)),
        _ => true,
    }
}

/// Whether a canonical motion property holds colors rather than
/// numbers (Sprint 7 motion, CAP-006): `fill` and `border-color`
/// interpolate straight RGBA from hex colors.
fn property_is_color(property: &str) -> bool {
    matches!(property, "fill" | "border-color")
}

/// Human list of the animatable properties, for diagnostics.
fn animatable_list() -> String {
    "`x`, `y`, `scale`, `rotation`, `opacity`, `rotation-x`, `rotation-y`, `width`, `height`, `fill`, `radius`, `border-width`, `border-color`, `blur`, `font-size`, `letter-spacing`, and (on audio and video clips) `volume` and `pan`".to_string()
}

/// Shared motion property check (T003 plus Sprint 7 motion,
/// CAP-005/006): exactly one animatable property, stored canonical.
/// Later properties are marked with Sprint 7, beyond-blur filters with
/// Sprint 11, `clip-shape` names the numeric shape fields (the clip's
/// shape animates through `width`, `height`, and `radius`), and an
/// attribute property on a clip lacking it fails naming the clip kind.
/// `target` is the resolved clip index for the kind check; a missing
/// target skips only that check.
fn motion_property(
    ctx: &mut Ctx,
    node: &RawNode,
    what: &str,
    clips: &[Clip],
    target: Option<usize>,
) -> Option<String> {
    let raw = match find(node, "property").and_then(as_str) {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!(
                    "{what} is missing required `property`; every directive names what it moves"
                ),
                format!("add `property=` to one of {}", animatable_list()),
            );
            return None;
        }
        Some(s) => s.to_string(),
    };
    if let Some(canonical) = canonical_property(&raw) {
        if let Some(idx) = target {
            if let Some(clip) = clips.get(idx) {
                if !property_kind_ok(canonical, clip) {
                    let loc = find(node, "property").unwrap();
                    ctx.push(
                        loc.line,
                        loc.column,
                        "E_BAD_MOTION",
                        format!(
                            "{what} property {raw:?} needs {} clip; target {idx} is {}",
                            match canonical {
                                "width" | "height" => "a box, image, flex, or svg",
                                "fill" | "border-width" | "border-color" => "a box or path",
                                "radius" => "a box",
                                "volume" | "pan" => "an audio or video",
                                _ => "a text",
                            },
                            clip.what()
                        ),
                        format!(
                            "animate {raw:?} on a clip carrying it, or pick one of {}",
                            animatable_list()
                        ),
                    );
                    return None;
                }
            }
        }
        return Some(canonical.to_string());
    }
    let norm = raw.replace('_', "-");
    if notation::model::LATER_PROPERTIES.contains(&norm.as_str()) {
        ctx.diags.push(Diagnostic::new(
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "E_UNSUPPORTED",
            format!(
                "{what} property {raw:?} needs Sprint 7; animatable today are {}",
                animatable_list()
            ),
            "move one of the animatable properties, or wait for the named sprint",
        ));
        return None;
    }
    if notation::model::FILTER_PROPERTIES.contains(&norm.as_str()) {
        ctx.diags.push(Diagnostic::new(
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "E_UNSUPPORTED",
            format!(
                "{what} property {raw:?} is a filter beyond blur and needs Sprint 11; only `blur` animates today"
            ),
            "animate `blur` for softness, or wait for Sprint 11 picture treatments",
        ));
        return None;
    }
    if norm == "clip-shape" {
        let loc = find(node, "property").unwrap();
        ctx.push(
            loc.line,
            loc.column,
            "E_BAD_MOTION",
            format!(
                "{what} property {raw:?} names no single value; the clip's shape animates through its numeric fields `width`, `height`, `radius`, and `border-width`"
            ),
            "animate `width`, `height`, `radius`, or `border-width` on the clip instead",
        );
        return None;
    }
    ctx.push(
        find(node, "property")
            .map(|v| v.line)
            .unwrap_or(node.name_line),
        find(node, "property")
            .map(|v| v.column)
            .unwrap_or(node.name_column),
        "E_BAD_MOTION",
        format!(
            "{what} property {raw:?} is not animatable; animatable today are {}",
            animatable_list()
        ),
        format!("set `property=` to one of {}", animatable_list()),
    );
    None
}

/// Per-property value range check (T003 plus Sprint 7 motion,
/// CAP-005/006, plus Sprint 10 sound envelopes): opacity runs 0 to 1,
/// sizes run positive, radii and spacing and blur run 0 and up,
/// envelope volume runs 0 to 2, pan runs −1 to 1, and every other
/// numeric value must simply be finite. Out-of-range values are
/// refused, never clamped silently (eased overshoot still clamps at
/// draw time, like blur always did; per-sample sound evaluation clamps
/// the same way).
fn motion_value_ok(
    ctx: &mut Ctx,
    node: &RawNode,
    what: &str,
    property: &str,
    key: &str,
    v: f64,
) -> bool {
    if !v.is_finite() {
        let loc = find(node, key).unwrap();
        ctx.push(
            loc.line,
            loc.column,
            "E_BAD_MOTION",
            format!("{what} `{key}` is not a readable number"),
            format!("set `{key}=<number>` on the directive"),
        );
        return false;
    }
    let range: Option<(&str, &str)> = match property {
        "opacity" => {
            if !(0.0..=1.0).contains(&v) {
                Some(("0 (transparent) to 1 (opaque)", "0 to 1"))
            } else {
                None
            }
        }
        "volume" => {
            if !(0.0..=2.0).contains(&v) {
                Some(("0 (silence) to 2 (twice unity)", "0 to 2"))
            } else {
                None
            }
        }
        "pan" => {
            if !(-1.0..=1.0).contains(&v) {
                Some(("-1 (left) to 1 (right)", "-1 to 1"))
            } else {
                None
            }
        }
        "width" | "height" | "font-size" => {
            if v <= 0.0 {
                Some(("a positive number of pixels", "positive pixels"))
            } else {
                None
            }
        }
        "radius" | "letter-spacing" | "border-width" | "blur" => {
            if v < 0.0 {
                Some(("0 pixels and up", "0 and up"))
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some((runs, fix_range)) = range {
        let loc = find(node, key).unwrap();
        ctx.push(
            loc.line,
            loc.column,
            "E_BAD_MOTION",
            format!("{what} `{key}` {v} is invalid; {property} runs {runs}"),
            format!("set `{key}=<number {fix_range}>` on the directive"),
        );
        return false;
    }
    true
}

/// Find the first present spelling among hyphenated, underscored, and
/// legacy aliases: canonical hyphen first, so a clip writing both the
/// canonical key and an alias keeps the canonical value.
fn find_any<'b>(node: &'b RawNode, keys: &[&str]) -> Option<&'b RawValue> {
    keys.iter().find_map(|k| find(node, k))
}

/// Shared 3D-turn clip attributes (Sprint 7 motion, CAP-005):
/// `rotation-x`/`rotation-y` (plus `rotate-x`/`rotate-y`, either
/// underscore form) as finite degrees defaulting to 0, `perspective`
/// as finite positive pixels (absent renders flat), and `origin` as a
/// placement defaulting to `center`. A non-positive perspective and an
/// unknown origin keyword fail with `E_BAD_MOTION`, the origin refusal
/// listing the accepted values.
fn clip_turn(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<Turn3D> {
    const RX: &[&str] = &["rotation-x", "rotation_x", "rotate-x", "rotate_x"];
    const RY: &[&str] = &["rotation-y", "rotation_y", "rotate-y", "rotate_y"];
    let mut ok = true;
    let mut degrees = |keys: &[&str], canonical: &str| -> f64 {
        match find_any(node, keys) {
            None => 0.0,
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => x,
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_MOTION",
                        format!("{what} `{canonical}` must be a number of degrees"),
                        format!("set `{canonical}=<degrees>` on the {what}"),
                    );
                    ok = false;
                    0.0
                }
            },
        }
    };
    let rotation_x = degrees(RX, "rotation-x");
    let rotation_y = degrees(RY, "rotation-y");
    let perspective = match find(node, "perspective") {
        None => None,
        Some(v) => match as_f64(v) {
            Some(p) if p.is_finite() && p > 0.0 => Some(p),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "{what} `perspective` {} is invalid; perspective runs positive pixels (flat when omitted)",
                        raw_text(v)
                    ),
                    format!("set `perspective=<pixels above 0>` on the {what}, or remove the key for a flat turn"),
                );
                ok = false;
                None
            }
        },
    };
    let origin = match find(node, "origin").and_then(as_str) {
        None => Placement::center(),
        Some(s) => match Placement::parse(s) {
            Some(p) => p,
            None => {
                let v = find(node, "origin").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "{what} `origin` {s:?} is not an origin; use `center`, `top`, `bottom`, `left`, `right`, `top-left`, `top-right`, `bottom-left`, `bottom-right`, or a pair like `\"left top\"`"
                    ),
                    format!("set `origin=\"center\"` (or another placement) on the {what}"),
                );
                ok = false;
                Placement::center()
            }
        },
    };
    if !ok {
        return None;
    }
    Some(Turn3D {
        rotation_x,
        rotation_y,
        perspective,
        origin,
    })
}

/// Shared static `filter` clip key (Sprint 7 motion, CAP-006): absent
/// or `none` draws sharp (sigma 0); `blur(N)`/`blur(Npx)` draws a
/// static Gaussian blur of N canvas pixels. Any other filter shape is
/// still Sprint 11 and fails as unsupported naming the sprint; a
/// malformed `blur(...)` fails with `E_BAD_MOTION` naming the form.
fn clip_filter(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    let found = match find(node, "filter") {
        None => return Some(0.0),
        Some(v) => v,
    };
    let raw = match as_str(found) {
        Some(s) => s.trim().to_string(),
        None => {
            ctx.push(
                found.line,
                found.column,
                "E_BAD_MOTION",
                format!(
                    "{what} `filter` {} is not a filter; use `\"none\"` or `\"blur(<pixels >= 0>)\"`",
                    raw_text(found)
                ),
                format!("set `filter=\"none\"` or `filter=\"blur(2)\"` on the {what}"),
            );
            return None;
        }
    };
    if raw.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    if let Some(inner) = raw
        .strip_prefix("blur(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        // A parenthesized remainder is a filter list (or a second
        // function): beyond one blur, so Sprint 11, not malformed.
        if inner.contains('(') || inner.contains(')') {
            let v = find(node, "filter").unwrap();
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("{what} `filter` {raw:?} needs Sprint 11; only `none` and `blur(N)` exist today"),
                "use `filter=\"none\"` or `filter=\"blur(2)\"`, animate `blur` with a tween or key, or wait for Sprint 11 picture treatments",
            ));
            return None;
        }
        let num = inner
            .trim()
            .strip_suffix("px")
            .unwrap_or(inner.trim())
            .trim();
        match num.parse::<f64>() {
            Ok(sigma) if sigma.is_finite() && sigma >= 0.0 => return Some(sigma),
            _ => {
                let v = find(node, "filter").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `filter` {raw:?} is not a blur; use `filter=\"blur(<pixels >= 0>)\"` or `\"none\"`"),
                    format!("set `filter=\"blur(2)\"` on the {what}, or remove the key"),
                );
                return None;
            }
        }
    }
    let v = find(node, "filter").unwrap();
    ctx.diags.push(Diagnostic::new(
        &ctx.file.clone(),
        v.line,
        v.column,
        "E_UNSUPPORTED",
        format!("{what} `filter` {raw:?} needs Sprint 11; only `none` and `blur(N)` exist today"),
        "use `filter=\"none\"` or `filter=\"blur(2)\"`, animate `blur` with a tween or key, or wait for Sprint 11 picture treatments",
    ));
    None
}

/// Shared `stagger` delay check (Sprint 7 motion, CAP-002): absent
/// means 0 (no cascade); present must be a finite number of seconds
/// >= 0. A negative stagger fails with `E_BAD_MOTION` naming the
/// bound, never shifts pieces backward silently.
fn motion_stagger(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    match find(node, "stagger") {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(x),
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `stagger` {x} is invalid; stagger delays run 0 seconds and up"),
                    format!("set `stagger=<seconds from 0>` on the {what}"),
                );
                None
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `stagger` is not a readable number of seconds"),
                    format!("set `stagger=<seconds from 0>` on the {what}"),
                );
                None
            }
        },
    }
}

/// Shared `stagger-from` origin check (Sprint 7 motion, CAP-002):
/// absent means `start`; present must name `start`, `center`, or
/// `end` (either hyphenated or underscored spelling).
fn motion_stagger_origin(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<StaggerOrigin> {
    match find_either(node, "stagger-from", "stagger_from").and_then(as_str) {
        None => Some(StaggerOrigin::Start),
        Some(s) => match s {
            "start" => Some(StaggerOrigin::Start),
            "center" => Some(StaggerOrigin::Center),
            "end" => Some(StaggerOrigin::End),
            _ => {
                let v = find_either(node, "stagger-from", "stagger_from").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "{what} `stagger-from` {s:?} is not a stagger origin; use `start`, `center`, or `end`"
                    ),
                    format!("set `stagger-from=\"start\"`, `\"center\"`, or `\"end\"` on the {what}"),
                );
                None
            }
        },
    }
}

/// Shared `stagger-grid` check (Sprint 7 motion, CAP-002): absent
/// means the 1D piece order; present must be a positive whole column
/// count (`stagger-grid=4`) or `<columns>x<rows>` (`stagger-grid="4x3"`).
fn motion_stagger_grid(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<Option<StaggerGrid>> {
    let raw = match find_either(node, "stagger-grid", "stagger_grid") {
        None => return Some(None),
        Some(v) => v,
    };
    let parse_count = |s: &str| -> Option<usize> {
        let n: i64 = s.trim().parse().ok()?;
        (n >= 1).then_some(n as usize)
    };
    let grid = match &raw.kind {
        ValueKind::Int(n) if *n >= 1 => Some(StaggerGrid {
            cols: *n as usize,
            rows: None,
        }),
        ValueKind::Str(s) => {
            let parts: Vec<&str> = s.split('x').collect();
            match parts.as_slice() {
                [c] => parse_count(c).map(|cols| StaggerGrid { cols, rows: None }),
                [c, r] => match (parse_count(c), parse_count(r)) {
                    (Some(cols), Some(rows)) => Some(StaggerGrid {
                        cols,
                        rows: Some(rows),
                    }),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    };
    match grid {
        Some(g) => Some(Some(g)),
        None => {
            ctx.push(
                raw.line,
                raw.column,
                "E_BAD_MOTION",
                format!(
                    "{what} `stagger-grid` is not a grid; use a positive whole column count or `<columns>x<rows>`"
                ),
                format!("set `stagger-grid=<columns>` or `stagger-grid=\"<columns>x<rows>\"` on the {what}"),
            );
            None
        }
    }
}

/// Shared `repeat` count check (Sprint 7 motion, CAP-004): absent
/// means 1 (run once); present must be a whole number of passes from
/// 1 to 1000. A non-positive count names an endless repeat and fails;
/// an over-limit count and an unbounded marker (`infinite`,
/// `endless`, `forever`, `unbounded`) fail the same way, all naming
/// the accepted range. Counts arrive validated, never guessed.
fn motion_repeat(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<u32> {
    const BOUND: &str = "repeat counts run 1 (once) to 1000 passes";
    let raw = match find(node, "repeat") {
        None => return Some(REPEAT_MIN),
        Some(v) => v,
    };
    let bad = |ctx: &mut Ctx, detail: String| {
        ctx.push(
            raw.line,
            raw.column,
            "E_BAD_MOTION",
            format!("{what} `repeat` {detail}; {BOUND}"),
            format!("set `repeat=<whole passes from 1 to 1000>` on the {what}"),
        );
    };
    match &raw.kind {
        ValueKind::Int(n) => {
            if *n < REPEAT_MIN as i64 {
                bad(ctx, format!("{n} names an endless repeat"));
                None
            } else if *n > REPEAT_MAX as i64 {
                bad(ctx, format!("{n} is past the 1000-pass bound"));
                None
            } else {
                Some(*n as u32)
            }
        }
        ValueKind::Float(x) => {
            if x.is_finite()
                && x.fract() == 0.0
                && *x >= REPEAT_MIN as f64
                && *x <= REPEAT_MAX as f64
            {
                Some(*x as u32)
            } else if x.is_finite() && x.fract() != 0.0 {
                bad(ctx, format!("{x} is not a whole number of passes"));
                None
            } else if *x < REPEAT_MIN as f64 {
                bad(ctx, format!("{x} names an endless repeat"));
                None
            } else {
                bad(ctx, format!("{x} is past the 1000-pass bound"));
                None
            }
        }
        ValueKind::Str(s) => {
            let t = s.trim().to_lowercase();
            if ["infinite", "endless", "forever", "unbounded", "loop"].contains(&t.as_str()) {
                bad(ctx, format!("{s:?} names an endless repeat"));
                return None;
            }
            match s.trim().parse::<f64>() {
                Ok(x)
                    if x.is_finite()
                        && x.fract() == 0.0
                        && x >= REPEAT_MIN as f64
                        && x <= REPEAT_MAX as f64 =>
                {
                    Some(x as u32)
                }
                Ok(x) if x.is_finite() && x.fract() != 0.0 => {
                    bad(ctx, format!("{s:?} is not a whole number of passes"));
                    None
                }
                Ok(x) if x < REPEAT_MIN as f64 => {
                    bad(ctx, format!("{s:?} names an endless repeat"));
                    None
                }
                Ok(_) => {
                    bad(ctx, format!("{s:?} is past the 1000-pass bound"));
                    None
                }
                Err(_) => {
                    bad(ctx, format!("{s:?} is not a readable pass count"));
                    None
                }
            }
        }
        _ => {
            bad(ctx, "is not a readable pass count".to_string());
            None
        }
    }
}

/// Shared `yoyo` switch check (Sprint 7 motion, CAP-004): absent means
/// `false`; present must be `true` or `false`, like `typewriter`.
fn motion_yoyo(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<bool> {
    match find(node, "yoyo") {
        None => Some(false),
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `yoyo` must be `true` or `false`; alternate passes reverse only when `yoyo=true`"),
                    format!("set `yoyo=true` or `yoyo=false` on the {what}"),
                );
                None
            }
        },
    }
}

/// Shared `repeat-delay` gap check (Sprint 7 motion, CAP-004): absent
/// means 0 (no gap); present must be a finite number of seconds >= 0
/// holding end values between passes. A negative gap fails naming
/// the bound, never overlaps passes silently.
fn motion_repeat_delay(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    match find_either(node, "repeat-delay", "repeat_delay") {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(x),
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `repeat-delay` {x} is invalid; gap seconds run 0 and up"),
                    format!("set `repeat-delay=<seconds from 0>` on the {what}"),
                );
                None
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `repeat-delay` is not a readable number of seconds"),
                    format!("set `repeat-delay=<seconds from 0>` on the {what}"),
                );
                None
            }
        },
    }
}

/// Shared `orient` check (Sprint 7 motion, CAP-008): absent means
/// false (no tangent rotation); present must be `true` or `false`.
/// Anything else fails with `E_BAD_MOTION`, never guessed.
fn motion_orient(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<bool> {
    match find(node, "orient") {
        None => Some(false),
        Some(v) => match &v.kind {
            ValueKind::Bool(b) => Some(*b),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `orient` must be `true` or `false`; the clip follows the path tangent only when `orient=true`"),
                    format!("set `orient=true` or `orient=false` on the {what}"),
                );
                None
            }
        },
    }
}

/// Shared path-data check (Sprint 7 motion, CAP-008): the raw value of
/// `key` must be a string parsing as SVG-subset path data (`M`, `L`,
/// `C`, `Q`, `A`, `Z`). Unparseable data fails with `E_BAD_MOTION`
/// naming the command, never guessed. Returns the raw data on
/// success; `Ok(None)` when the key is absent.
fn motion_path_data(
    ctx: &mut Ctx,
    what: &str,
    key: &str,
    value: Option<&RawValue>,
) -> Result<Option<String>, ()> {
    let Some(v) = value else {
        return Ok(None);
    };
    let raw = match as_str(v) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("{what} `{key}` is not readable path data; paths are quoted strings like `\"M10,10 L110,10\"`"),
                format!("set `{key}=\"M<x>,<y> ...\"` on the {what}"),
            );
            return Err(());
        }
    };
    if let Err(e) = motion::path::parse(&raw) {
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            format!("{what} `{key}` {e}; use `M`, `L`, `C`, `Q`, `A`, or `Z` commands with plain numbers"),
            format!("fix the `{key}` path data on the {what}"),
        );
        return Err(());
    }
    Ok(Some(raw))
}

/// Shared morph-structure check (Sprint 7 motion, CAP-008): two parsed
/// paths interpolate only across the same command structure. Anything
/// else fails with `E_BAD_MOTION` naming the first differing command.
fn motion_path_morph(
    ctx: &mut Ctx,
    what: &str,
    detail: &str,
    line: usize,
    column: usize,
    from: &motion::path::Path,
    to: &motion::path::Path,
) -> bool {
    match motion::path::morph(from, to, 0.0) {
        Ok(_) => true,
        Err(e) => {
            ctx.push(
                line,
                column,
                "E_BAD_MOTION",
                format!("{what} {detail} morphs across structures: {e}"),
                format!("give both paths the same command structure, or travel one path"),
            );
            false
        }
    }
}

/// Animated pieces of a target clip (Sprint 7 motion, CAP-002): the
/// split pieces of a split text clip, else the single clip itself. An
/// empty split value animates no pieces.
fn stagger_pieces(clip: &Clip) -> usize {
    match clip {
        Clip::Text(tc) => match tc.split {
            Some(SplitMode::Chars) => tc.value.chars().count(),
            Some(SplitMode::Words) => tc.value.split_whitespace().count(),
            Some(SplitMode::Lines) => tc.value.split('\n').count(),
            None => 1,
        },
        _ => 1,
    }
}

/// Record one stagger clamp (Sprint 7 motion, CAP-002): when the
/// staggered, repeated motion of `kind` on `target` runs past the
/// target clip window, the motion past the window end is cut, and the
/// cut lands in the reviewable clamp list with the numbers. A single
/// piece, an unstaggered directive, and a motion window fitting the
/// clip record nothing, so fitting motion stays silent.
#[allow(clippy::too_many_arguments)]
fn stagger_overflow(
    ctx: &mut Ctx,
    clips: &[Clip],
    kind: &'static str,
    target: usize,
    line: usize,
    column: usize,
    stagger: f64,
    origin: StaggerOrigin,
    grid: Option<StaggerGrid>,
    window_start: f64,
    window_len: f64,
    repeat: u32,
    gap: f64,
) {
    if !(stagger > 0.0) || !stagger.is_finite() {
        return;
    }
    let Some(clip) = clips.get(target) else {
        return;
    };
    let pieces = stagger_pieces(clip);
    if pieces <= 1 {
        return;
    }
    let motion_origin = match origin {
        StaggerOrigin::Start => motion::StaggerOrigin::Start,
        StaggerOrigin::Center => motion::StaggerOrigin::Center,
        StaggerOrigin::End => motion::StaggerOrigin::End,
    };
    let motion_grid = grid.map(|g| {
        (
            g.cols,
            g.rows.unwrap_or(motion::stagger_grid_rows(pieces, g.cols)),
        )
    });
    let max_delay = motion::stagger_max_delay_grid(pieces, stagger, motion_origin, motion_grid);
    let motion_end = motion::repeat_motion_end(window_start, window_len, repeat, gap) + max_delay;
    let clip_end = clip.start() + clip.duration();
    let overflow = motion_end - clip_end;
    if overflow > 1e-9 {
        ctx.stagger_clamps.push(StaggerClamp {
            kind,
            target,
            line,
            column,
            pieces,
            stagger,
            origin: origin.name(),
            grid_cols: grid.map(|g| g.cols),
            grid_rows: grid.and_then(|g| g.rows),
            max_delay,
            motion_end,
            clip_end,
            overflow,
        });
    }
}

/// Shared stagger-plus-repeat block of a `tween` or `key` (Sprint 7
/// motion, CAP-002/CAP-004): the delay distribution and the replay
/// shape. Every bad key reports (never just the first), and the
/// caller marks its own failure; returns `None` after reporting, else
/// the six values in declaration order.
#[allow(clippy::type_complexity)]
fn motion_stagger_repeat(
    ctx: &mut Ctx,
    node: &RawNode,
    what: &str,
) -> Option<(f64, StaggerOrigin, Option<StaggerGrid>, u32, bool, f64)> {
    let stagger = motion_stagger(ctx, node, what);
    let origin = motion_stagger_origin(ctx, node, what);
    let grid = motion_stagger_grid(ctx, node, what);
    let repeat = motion_repeat(ctx, node, what);
    let yoyo = motion_yoyo(ctx, node, what);
    let gap = motion_repeat_delay(ctx, node, what);
    Some((stagger?, origin?, grid?, repeat?, yoyo?, gap?))
}

/// Largest sampled `i` values when range-checking an expression
/// (Sprint 7 motion, CAP-010): sampling covers every replicate clone
/// and every split piece of the target, so a per-piece violation is
/// refused before frame one.
fn expr_sample_is(target: usize, clips: &[Clip], replicates: &[ReplicateGroup]) -> Vec<f64> {
    for group in replicates {
        if target >= group.first && target < group.first + group.count {
            return (0..group.count).map(|k| k as f64).collect();
        }
    }
    if let Some(Clip::Text(tc)) = clips.get(target) {
        if let Some(mode) = tc.split {
            let n = match mode {
                SplitMode::Chars => tc.value.chars().count(),
                SplitMode::Words => tc.value.split_whitespace().count(),
                SplitMode::Lines => tc.value.split('\n').count(),
            };
            if n > 0 {
                return (0..n).map(|k| k as f64).collect();
            }
        }
    }
    vec![0.0]
}

/// Range rule of one numeric motion property (Sprint 7 motion,
/// CAP-010): mirrors `motion_value_ok` so sampled expression values
/// refuse exactly where written endpoint values would. Returns the
/// human range when `v` violates it.
fn expr_range_violation(property: &str, v: f64) -> Option<&'static str> {
    if !v.is_finite() {
        return Some("a finite number");
    }
    match property {
        "opacity" if !(0.0..=1.0).contains(&v) => Some("0 (transparent) to 1 (opaque)"),
        "width" | "height" | "font-size" if v <= 0.0 => Some("a positive number of pixels"),
        "radius" | "letter-spacing" | "border-width" | "blur" if v < 0.0 => Some("0 pixels and up"),
        "volume" if !(0.0..=2.0).contains(&v) => Some("0 (silence) to 2 (twice unity)"),
        "pan" if !(-1.0..=1.0).contains(&v) => Some("-1 (left) to 1 (right)"),
        _ => None,
    }
}

/// Validate one `expr` tween value (Sprint 7 motion, CAP-010): the
/// source parses as a pure expression over `t`, `i`, and the
/// declared inputs, and every sampled instant (each frame of the
/// window at `fps`, plus the window end, over every sampled `i`)
/// evaluates finite and in the property's range. The first violation
/// reports with its instant; the maker fixes the function, not each
/// instant.
#[allow(clippy::too_many_arguments)]
fn motion_expr(
    ctx: &mut Ctx,
    what: &str,
    property: Option<&str>,
    source: &str,
    value_line: usize,
    value_column: usize,
    input_names: &[&str],
    input_values: &[f64],
    target: Option<usize>,
    clips: &[Clip],
    replicates: &[ReplicateGroup],
    window: Option<(f64, f64)>,
    fps: Fps,
) -> Option<()> {
    let parsed = match motion::expr::parse(source, input_names) {
        Ok(e) => e,
        Err(err) => {
            ctx.push(
                value_line,
                // The expression starts after the opening quote, so the
                // byte offset lands on the offending character.
                value_column + 1 + err.offset,
                "E_BAD_MOTION",
                format!("{what} `expr` is not pure: {}", err.message),
                "write a pure expression over `t`, `i`, declared `input` names, and seeded `random(seed[,min,max])`",
            );
            return None;
        }
    };
    // Sampled instants: every frame time of the window plus its end,
    // so the check replays exactly what drawing evaluates. Sampling
    // needs the property, target, and window; purity above never
    // waits for them, so one bad key never hides an impure
    // expression.
    let (Some(property), Some(target), Some((start, duration))) = (property, target, window) else {
        // Pure, but nothing to sample against: the missing key
        // already reported, so this is not a second failure.
        return Some(());
    };
    let rate = fps.value().max(1.0);
    let frames = (duration * rate).ceil().max(1.0) as u64;
    // Long windows sample evenly (at most 4096 instants): the check
    // stays proportional to the window, never to its frame count.
    let stride = frames.div_ceil(4096).max(1);
    let mut instants: Vec<f64> = (0..frames)
        .step_by(stride as usize)
        .map(|k| start + k as f64 / rate)
        .take_while(|t| *t < start + duration)
        .collect();
    instants.push(start + duration);
    for &i in &expr_sample_is(target, clips, replicates) {
        for &t in &instants {
            let v = parsed.eval(t, i, input_values);
            if let Some(range) = expr_range_violation(property, v) {
                ctx.push(
                    value_line,
                    value_column,
                    "E_BAD_MOTION",
                    format!(
                        "{what} `expr` evaluates to {v} at t={t:.6}s i={i} (outside {range} for {property}); motion never shows an out-of-range value"
                    ),
                    format!("keep `{property}` expressions within {range} over the whole window"),
                );
                return None;
            }
        }
    }
    Some(())
}

fn validate_tween(
    ctx: &mut Ctx,
    node: &RawNode,
    clips: &[Clip],
    input_names: &[&str],
    input_values: &[f64],
    fps: Fps,
    replicates: &[ReplicateGroup],
) -> Option<TweenDecl> {
    const KEYS: &[&str] = &[
        "target",
        "property",
        "kind",
        "start",
        "duration",
        "ease",
        "from",
        "to",
        "expr",
        "path",
        "path-to",
        "path_to",
        "orient",
        "stagger",
        "stagger-from",
        "stagger_from",
        "stagger-grid",
        "stagger_grid",
        "repeat",
        "yoyo",
        "repeat-delay",
        "repeat_delay",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown tween key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let mut ok = true;
    let target = motion_target(ctx, node, "tween", clips.len());
    ok &= target.is_some();
    // Path travel (Sprint 7 motion, CAP-008): `path` replaces the
    // numeric shape — a path tween carries no `property`, `kind`,
    // `from`, or `to`. `path-to` morphs toward a second path of the
    // same structure; `orient` follows the tangent.
    let path_value = find(node, "path");
    let path_to_value = find_either(node, "path-to", "path_to");
    let is_path = path_value.is_some() || path_to_value.is_some();
    let orient = motion_orient(ctx, node, "tween");
    ok &= orient.is_some();
    if is_path {
        for key in ["property", "kind", "from", "to", "expr"] {
            if let Some(v) = find(node, key) {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("tween `{key}` conflicts with `path`; a path tween places the clip with `path` (`path-to` morphs, `orient` turns), not a named property with numbers or an expression"),
                    "remove the numeric key, or remove `path` to animate one property",
                );
                ok = false;
            }
        }
        if path_value.is_none() {
            let v = path_to_value.unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                "tween `path-to` needs `path`; a morph interpolates from the travel path toward `path-to`",
                "add `path=\"M<x>,<y> ...\"` to the tween, or remove `path-to`",
            );
            ok = false;
        }
    } else if orient.is_some() && find(node, "orient").is_some() {
        let v = find(node, "orient").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            "tween `orient` rides path motion; a numeric tween carries no `path`, so `orient` has no tangent to follow",
            "add `path=\"M<x>,<y> ...\"` to the tween, or remove `orient`",
        );
        ok = false;
    }
    let (path, path_to) = if is_path {
        let path = match motion_path_data(ctx, "tween", "path", path_value) {
            Ok(p) => p,
            Err(()) => {
                ok = false;
                None
            }
        };
        let path_to = match motion_path_data(ctx, "tween", "path-to", path_to_value) {
            Ok(p) => p,
            Err(()) => {
                ok = false;
                None
            }
        };
        match (&path, &path_to) {
            (Some(a), Some(b)) => {
                let pa = motion::path::parse(a).expect("path data parses after validation");
                let pb = motion::path::parse(b).expect("path data parses after validation");
                let v = path_to_value.unwrap();
                ok &= motion_path_morph(ctx, "tween", "`path-to`", v.line, v.column, &pa, &pb);
            }
            _ => {}
        }
        (path, path_to)
    } else {
        (None, None)
    };
    if is_path {
        let window = clip_window(ctx, node, "tween");
        ok &= window.is_some();
        let ease = match motion_ease_opt(ctx, node) {
            Some(e) => e.unwrap_or(EaseDecl::Linear),
            None => {
                ok = false;
                EaseDecl::Linear
            }
        };
        let stagger_repeat = motion_stagger_repeat(ctx, node, "tween");
        ok &= stagger_repeat.is_some();
        if !ok {
            return None;
        }
        let (start, duration) = window.unwrap();
        let (stagger, stagger_from, stagger_grid, repeat, yoyo, repeat_delay) =
            stagger_repeat.unwrap();
        return Some(TweenDecl {
            target: target.unwrap(),
            property: String::new(),
            kind: TweenKind::To,
            from: None,
            to: None,
            from_color: None,
            to_color: None,
            start,
            duration,
            ease,
            stagger,
            stagger_from,
            stagger_grid,
            repeat,
            yoyo,
            repeat_delay,
            path,
            path_to,
            orient: orient.unwrap_or(false),
            expr: None,
            line: node.name_line,
            column: node.name_column,
        });
    }
    let property = motion_property(ctx, node, "tween", clips, target);
    ok &= property.is_some();
    // Sound envelopes (Sprint 10, CAP-001) ride one plain window: the
    // visual-only keys refuse here, before the expression and
    // stagger/repeat machinery below could misread them.
    if let Some(p) = property.as_deref() {
        ok &= sound_directive_guards(ctx, node, "tween", p);
    }
    let kind = match find(node, "kind").and_then(as_str) {
        None => TweenKind::To,
        Some(s) => {
            match TweenKind::parse(s) {
                Some(k) => k,
                None => {
                    let v = find(node, "kind").unwrap();
                    ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("tween `kind` {s:?} is not a tween form; use `to`, `from`, or `from-to`"),
                    "set `kind=\"to\"`, `kind=\"from\"`, or `kind=\"from-to\"` on the tween",
                );
                    ok = false;
                    TweenKind::To
                }
            }
        }
    };
    // Window: absolute start plus positive duration, like clips.
    let window = clip_window(ctx, node, "tween");
    ok &= window.is_some();
    // Easing: the nineteen names, `out-back(n)`, and the Sprint 7 curve
    // set; anything else names the accepted forms.
    let ease = match motion_ease_opt(ctx, node) {
        Some(e) => e.unwrap_or(EaseDecl::Linear),
        None => {
            ok = false;
            EaseDecl::Linear
        }
    };
    // Expression (Sprint 7 motion, CAP-010): `expr` replaces the
    // endpoint numbers with a pure per-frame value function of `t`,
    // `i`, and declared inputs. It conflicts with `from`/`to`, with
    // path travel, with color properties (one number cannot paint a
    // color), with non-default `kind`/`ease` (nothing to shape
    // between), and with stagger/repeat windows (those belong to
    // segment motion).
    let expr_entry: Option<(String, usize, usize)> = match find(node, "expr") {
        None => None,
        Some(v) => match as_str(v) {
            Some(s) => Some((s.to_string(), v.line, v.column)),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    "tween `expr` must be a quoted string holding the expression",
                    "set `expr=\"t*30\"` on the tween",
                );
                ok = false;
                None
            }
        },
    };
    let is_expr = expr_entry.is_some();
    if is_expr {
        for key in ["from", "to"] {
            if let Some(v) = find(node, key) {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("tween `{key}` conflicts with `expr`; an expression tween evaluates `expr` per frame over the window, not endpoint numbers"),
                    "remove the endpoint key, or remove `expr` to move between two given values",
                );
                ok = false;
            }
        }
        if let Some(v) = find(node, "kind") {
            if !matches!(as_str(v), Some("to")) {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    "tween `kind` conflicts with `expr`; an expression tween evaluates per frame with no from/to form",
                    "remove `kind`, or remove `expr` to move between two given values",
                );
                ok = false;
            }
        }
        if find(node, "ease").is_some() && ease != EaseDecl::Linear {
            let v = find(node, "ease").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                "tween `ease` conflicts with `expr`; an expression evaluates directly per frame, with no segment to shape",
                "remove `ease`, or remove `expr` to ease between two given values",
            );
            ok = false;
        }
        if let Some(p) = property.as_deref() {
            if property_is_color(p) {
                let v = find(node, "expr").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("tween `expr` on {p:?} holds a color; an expression yields one number, so colors animate with `from`/`to`"),
                    "animate the color with `from=\"#rrggbb\"`/`to=\"#rrggbb\"` instead of `expr`",
                );
                ok = false;
            }
        }
    }
    // Values: `to` needs `to`, `from` needs `from`, `from-to` needs
    // both; a value under the wrong form is refused, never reinterpreted.
    // An expression tween carries neither endpoint.
    let need_from = matches!(kind, TweenKind::From | TweenKind::FromTo) && !is_expr;
    let need_to = matches!(kind, TweenKind::To | TweenKind::FromTo) && !is_expr;
    let mut read_value = |key: &str, needed: bool| -> Option<Option<f64>> {
        let present = find(node, key);
        if !needed {
            if let Some(v) = present {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "tween `{key}` needs kind `{}`; a `{}` tween carries no `{key}`",
                        kind.name(),
                        kind.name()
                    ),
                    format!(
                        "remove `{key}`, or set `kind=\"from-to\"` to move between two given values"
                    ),
                );
                ok = false;
                return Some(None);
            }
        }
        if needed && present.is_none() {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("tween kind `{}` is missing required `{key}`", kind.name()),
                format!("add `{key}=<number>` to the tween"),
            );
            ok = false;
            return Some(None);
        }
        match present {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(x) => {
                    if let Some(p) = property.as_deref() {
                        if !motion_value_ok(ctx, node, "tween", p, key, x) {
                            ok = false;
                            return Some(None);
                        }
                    }
                    Some(Some(x))
                }
                None => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_MOTION",
                        format!("tween `{key}` is not a readable number"),
                        format!("set `{key}=<number>` on the tween"),
                    );
                    ok = false;
                    Some(None)
                }
            },
        }
    };
    // Colors (Sprint 7 motion, CAP-006): `fill` and `border-color`
    // endpoints are hex colors interpolated in straight RGBA, never
    // numbers. Numeric properties carry no colors.
    let is_color = property.as_deref().is_some_and(property_is_color);
    let (from, to) = if is_color || is_expr {
        (None, None)
    } else {
        (read_value("from", need_from)?, read_value("to", need_to)?)
    };
    let mut read_color = |key: &str, needed: bool| -> Option<Option<Rgb>> {
        let present = find(node, key);
        if !needed {
            if let Some(v) = present {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "tween `{key}` needs kind `{}`; a `{}` tween carries no `{key}`",
                        kind.name(),
                        kind.name()
                    ),
                    format!(
                        "remove `{key}`, or set `kind=\"from-to\"` to move between two given colors"
                    ),
                );
                ok = false;
                return Some(None);
            }
        }
        if needed && present.is_none() {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("tween kind `{}` is missing required `{key}`", kind.name()),
                format!("add `{key}=\"#rrggbb\"` to the tween"),
            );
            ok = false;
            return Some(None);
        }
        match present {
            None => Some(None),
            Some(v) => match as_str(v).and_then(parse_color) {
                Some(c) => Some(Some(c)),
                None => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_MOTION",
                        format!(
                            "tween `{key}` {} is not a `#rgb` or `#rrggbb` color",
                            raw_text(v)
                        ),
                        format!("set `{key}=\"#rrggbb\"` on the tween"),
                    );
                    ok = false;
                    Some(None)
                }
            },
        }
    };
    let (from_color, to_color) = if is_color {
        (read_color("from", need_from)?, read_color("to", need_to)?)
    } else {
        (None, None)
    };
    // Stagger plus repeat (Sprint 7 motion, CAP-002/CAP-004): the delay
    // distribution and the replay shape; every bad key reports.
    let stagger_repeat = motion_stagger_repeat(ctx, node, "tween");
    ok &= stagger_repeat.is_some();
    // Expression cascades and replays (Sprint 7 motion, CAP-010): an
    // expression evaluates per frame over one window, so a cascade,
    // a replay, or a gap has no segment to distribute.
    if is_expr {
        if let Some((stagger, _, _, repeat, yoyo, gap)) = stagger_repeat {
            let mut conflict = |key: &str, message: String, fix: &str| {
                let v = find(node, key).unwrap();
                ctx.push(v.line, v.column, "E_BAD_MOTION", message, fix);
                ok = false;
            };
            if stagger != 0.0 {
                conflict(
                    "stagger",
                    "tween `stagger` conflicts with `expr`; an expression evaluates per frame over one window, with no cascade to distribute".to_string(),
                    "remove `stagger`, or remove `expr` to cascade segment motion",
                );
            }
            if repeat != 1 {
                conflict(
                    "repeat",
                    "tween `repeat` conflicts with `expr`; an expression evaluates per frame over one window, with no replay".to_string(),
                    "remove `repeat`, or remove `expr` to replay segment motion",
                );
            }
            if yoyo {
                conflict(
                    "yoyo",
                    "tween `yoyo` conflicts with `expr`; an expression evaluates per frame over one window, with no pass to reverse".to_string(),
                    "remove `yoyo`, or remove `expr` to reverse segment motion",
                );
            }
            if gap != 0.0 {
                let key = if find(node, "repeat-delay").is_some() {
                    "repeat-delay"
                } else {
                    "repeat_delay"
                };
                conflict(
                    key,
                    "tween `repeat-delay` conflicts with `expr`; an expression evaluates per frame over one window, with no gap to hold".to_string(),
                    "remove `repeat-delay`, or remove `expr` to gap segment motion",
                );
            }
        }
    }
    // Purity plus range (Sprint 7 motion, CAP-010): the expression
    // parses pure and samples in-range at every drawn instant, or the
    // tween reports with the instant named.
    if let Some(entry) = expr_entry.as_ref() {
        ok &= motion_expr(
            ctx,
            "tween",
            property.as_deref(),
            &entry.0,
            entry.1,
            entry.2,
            input_names,
            input_values,
            target,
            clips,
            replicates,
            window,
            fps,
        )
        .is_some();
    }
    if !ok {
        return None;
    }
    let (start, duration) = window.unwrap();
    let (stagger, stagger_from, stagger_grid, repeat, yoyo, repeat_delay) = stagger_repeat.unwrap();
    Some(TweenDecl {
        target: target.unwrap(),
        property: property.unwrap(),
        kind,
        from,
        to,
        from_color,
        to_color,
        start,
        duration,
        ease,
        stagger,
        stagger_from,
        stagger_grid,
        repeat,
        yoyo,
        repeat_delay,
        path: None,
        path_to: None,
        orient: false,
        expr: expr_entry.map(|entry| entry.0),
        line: node.name_line,
        column: node.name_column,
    })
}

fn validate_set(ctx: &mut Ctx, node: &RawNode, clips: &[Clip]) -> Option<SetDecl> {
    const KEYS: &[&str] = &["target", "property", "start", "duration", "value"];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown set key {k:?}; a `set` holds one value with no easing"),
                "remove the key or check the spelling",
            );
        }
    }
    let mut ok = true;
    let target = motion_target(ctx, node, "set", clips.len());
    ok &= target.is_some();
    let property = motion_property(ctx, node, "set", clips, target);
    ok &= property.is_some();
    let window = clip_window(ctx, node, "set");
    ok &= window.is_some();
    // Colors hold no single number (Sprint 7 motion, CAP-006): a set on
    // `fill` or `border-color` names the tween or key that interpolates
    // them instead.
    if property.as_deref().is_some_and(property_is_color) {
        let (line, column) = match find(node, "value") {
            Some(v) => (v.line, v.column),
            None => (node.name_line, node.name_column),
        };
        ctx.push(
            line,
            column,
            "E_BAD_MOTION",
            format!(
                "set property {:?} holds a color; a `set` holds one number, so colors animate with `tween` or `key`",
                property.as_deref().unwrap_or("")
            ),
            "animate the color with a `tween` or `key` instead of a `set`",
        );
        ok = false;
    }
    let value = match find(node, "value") {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                "set is missing required `value`; every hold names the value it holds",
                "add `value=<number>` to the set directive",
            );
            ok = false;
            0.0
        }
        Some(v) => match as_f64(v) {
            Some(x) => {
                if let Some(p) = property.as_deref() {
                    if !motion_value_ok(ctx, node, "set", p, "value", x) {
                        ok = false;
                    }
                }
                x
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    "set `value` is not a readable number",
                    "set `value=<number>` on the set directive",
                );
                ok = false;
                0.0
            }
        },
    };
    if !ok {
        return None;
    }
    let (start, duration) = window.unwrap();
    Some(SetDecl {
        target: target.unwrap(),
        property: property.unwrap(),
        value,
        start,
        duration,
        line: node.name_line,
        column: node.name_column,
    })
}

/// One parsed `frame` stop plus whether its `at` parsed: order checks
/// skip stops with unreadable times (already reported) so one bad value
/// never cascades into a false unordered diagnostic. `is_path` marks
/// stops that travel a `path` instead of holding a `value` (even when
/// the path itself failed); `has_value` marks stops that carry a
/// `value` key, for the mixed-shape check.
struct RawStop {
    at: f64,
    at_ok: bool,
    is_path: bool,
    has_value: bool,
    stop: Option<KeyStop>,
    line: usize,
    column: usize,
}

/// Validate one `frame` stop of a `key` (Sprint 7 motion): a required
/// `at` time in absolute seconds >= 0, a required numeric `value`
/// (range-checked like tween values) or — for path keys — a required
/// `path` of SVG-subset data, and an optional `ease` for the segment
/// running to the next stop. `index` numbers the stop from 1 in
/// document order so every diagnostic names it.
fn validate_frame(
    ctx: &mut Ctx,
    node: &RawNode,
    label: &str,
    index: usize,
    property: Option<&str>,
) -> RawStop {
    const KEYS: &[&str] = &["at", "value", "path", "ease"];
    let mut ok = true;
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown `frame` attribute {k:?}; a `frame` holds `at`, `value` (or `path`), and `ease`"),
                "remove the key or check the spelling",
            );
            ok = false;
        }
    }
    if !node.children.is_empty() {
        let child = &node.children[0];
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_MOTION",
            format!("key {label} stop {index} carries no children; a `frame` is one `at` time with one `value`"),
            "remove the child elements from the stop",
        );
        ok = false;
    }
    let (at, at_ok) = match find(node, "at") {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("key {label} stop {index} is missing required `at`; every stop names its time in seconds"),
                format!("add `at=<seconds>` to stop {index} of the key"),
            );
            ok = false;
            (0.0, false)
        }
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => (x, true),
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("key {label} stop {index} `at` {x} is invalid; stops sit at absolute seconds >= 0"),
                    format!("set `at=<seconds from 0>` on stop {index} of the key"),
                );
                ok = false;
                (0.0, false)
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("key {label} stop {index} `at` is not a readable number"),
                    format!("set `at=<seconds>` on stop {index} of the key"),
                );
                ok = false;
                (0.0, false)
            }
        },
    };
    // Colors (Sprint 7 motion, CAP-006): a `fill` or `border-color`
    // stop holds a hex color interpolated in straight RGBA; every other
    // stop holds a number range-checked like tween values.
    let stop_what = format!("key {label} stop {index}");
    let is_color = property.is_some_and(property_is_color);
    let mut value_color: Option<Rgb> = None;
    // Path stops (Sprint 7 motion, CAP-008): `path` replaces `value` —
    // the segment leaving the stop travels it, morphing toward the
    // next stop's path. `value` and `path` never share one stop.
    let is_path = find(node, "path").is_some();
    let has_value = find(node, "value").is_some();
    let mut stop_path: Option<String> = None;
    if is_path && has_value {
        let v = find(node, "value").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            format!("key {label} stop {index} carries both `value` and `path`; a path stop travels `path`, not a number"),
            format!("remove `value` from stop {index} of the key, or remove `path` to hold a number"),
        );
        ok = false;
    }
    if is_path {
        match motion_path_data(ctx, &stop_what, "path", find(node, "path")) {
            Ok(p) => stop_path = p,
            Err(()) => ok = false,
        }
    }
    let value = if is_path {
        0.0
    } else {
        match find(node, "value") {
            None => {
                ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("key {label} stop {index} is missing required `value`; every stop names the value it holds"),
                format!(
                    "add `value=<{}>` to stop {index} of the key",
                    if is_color { "color like \"#rrggbb\"" } else { "number" }
                ),
            );
                ok = false;
                0.0
            }
            Some(v) if is_color => match as_str(v).and_then(parse_color) {
                Some(c) => {
                    value_color = Some(c);
                    0.0
                }
                None => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_MOTION",
                        format!(
                        "key {label} stop {index} `value` {} is not a `#rgb` or `#rrggbb` color",
                        raw_text(v)
                    ),
                        format!("set `value=\"#rrggbb\"` on stop {index} of the key"),
                    );
                    ok = false;
                    0.0
                }
            },
            Some(v) => match as_f64(v) {
                // Range-checked exactly like tween values
                // (`motion_value_ok`), but naming the stop: every keyframe
                // refusal names its stop.
                Some(x) => {
                    if !motion_value_ok(ctx, node, &stop_what, property.unwrap_or(""), "value", x) {
                        ok = false;
                    }
                    x
                }
                None => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_MOTION",
                        format!("key {label} stop {index} `value` is not a readable number"),
                        format!("set `value=<number>` on stop {index} of the key"),
                    );
                    ok = false;
                    0.0
                }
            },
        }
    };
    let ease = match motion_ease_opt(ctx, node) {
        Some(e) => e,
        None => {
            ok = false;
            None
        }
    };
    let stop = ok.then(|| KeyStop {
        at,
        value,
        value_color,
        path: stop_path,
        ease,
        line: node.name_line,
        column: node.name_column,
    });
    RawStop {
        at,
        at_ok,
        is_path,
        has_value,
        stop,
        line: node.name_line,
        column: node.name_column,
    }
}

/// Validate a `key` element (Sprint 7 motion, CAP-001): a `target` clip,
/// an animatable `property`, an optional key-level `ease` fallback, and
/// at least two `frame` stops in strictly rising time order. Every stop
/// problem reports together, each naming its stop; unordered, duplicate,
/// unknown-property, and wrong-shape stops all fail before frame one.
fn validate_key(ctx: &mut Ctx, node: &RawNode, clips: &[Clip]) -> Option<KeyDecl> {
    const KEYS: &[&str] = &[
        "target",
        "property",
        "ease",
        "path",
        "path-to",
        "path_to",
        "orient",
        "stagger",
        "stagger-from",
        "stagger_from",
        "stagger-grid",
        "stagger_grid",
        "repeat",
        "yoyo",
        "repeat-delay",
        "repeat_delay",
    ];
    let mut ok = true;
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!(
                    "unknown `key` attribute {k:?}; a `key` holds `target`, `property`, and `ease`, with `frame` stops inside"
                ),
                "remove the key or check the spelling",
            );
            ok = false;
        }
    }
    let target = motion_target(ctx, node, "key", clips.len());
    ok &= target.is_some();
    // Path stops (Sprint 7 motion, CAP-008): a `path` rides the `frame`
    // stops, never the `key` element — the segment leaving each stop
    // travels that stop's path, morphing toward the next stop's.
    if let Some(v) = find(node, "path").or_else(|| find_either(node, "path-to", "path_to")) {
        let key = if find(node, "path").is_some() {
            "path"
        } else {
            "path-to"
        };
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            format!("key `{key}` rides `frame` stops, not the `key` element; every stop of a path key carries its own `path`"),
            "move the path data into `frame at=<seconds> path=\"M<x>,<y> ...\"` stops",
        );
        ok = false;
    }
    let orient = motion_orient(ctx, node, "key");
    ok &= orient.is_some();
    let ease = match motion_ease_opt(ctx, node) {
        Some(e) => e,
        None => {
            ok = false;
            None
        }
    };
    // The property peek stays diagnostic-free: numeric keys require
    // exactly one animatable property below, while path keys refuse
    // it. Labels read the canonical spelling, like before.
    let property_raw = find(node, "property").and_then(as_str).map(str::to_string);
    let label = match (
        &target,
        property_raw.as_deref().and_then(canonical_property),
    ) {
        (Some(t), Some(p)) => format!("target={t} property={p:?}"),
        _ => format!("at {}:{}", node.name_line, node.name_column),
    };
    let mut stops: Vec<RawStop> = Vec::new();
    for child in &node.children {
        if child.name != "frame" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_MOTION",
                format!(
                    "key {label} holds only `frame` stops; found `{}`",
                    child.name
                ),
                "use `frame at=<seconds> value=<number>` stops inside the key, or move the element out",
            );
            ok = false;
            continue;
        }
        stops.push(validate_frame(
            ctx,
            child,
            &label,
            stops.len() + 1,
            property_raw.as_deref().and_then(canonical_property),
        ));
        // A failed stop fails its key: the assembly below unwraps every
        // stop, so validity must be tracked here, not just reported.
        ok &= stops.last().is_some_and(|s| s.stop.is_some());
    }
    if stops.len() < 2 {
        let held = match stops.len() {
            0 => "no `frame` stops".to_string(),
            _ => "a single `frame` stop".to_string(),
        };
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_MOTION",
            format!("key {label} holds {held}; every key chains at least two stops"),
            "add at least two `frame at=<seconds> value=<number>` stops in time order inside the key",
        );
        ok = false;
    }
    // Stops run in time order: an earlier-or-equal time names its stop.
    // Stops with unreadable times stay out of the comparison (already
    // reported) so one bad value never cascades. Every offending stop
    // reports, not just the first.
    for i in 1..stops.len() {
        if !stops[i].at_ok || !stops[i - 1].at_ok {
            continue;
        }
        if stops[i].at < stops[i - 1].at {
            ctx.push(
                stops[i].line,
                stops[i].column,
                "E_BAD_MOTION",
                format!(
                    "key {label} stop {} at {}s runs before stop {} at {}s; stops run in time order",
                    i + 1,
                    stops[i].at,
                    i,
                    stops[i - 1].at
                ),
                "reorder the `frame` stops of the key so `at` times rise",
            );
            ok = false;
        } else if stops[i].at == stops[i - 1].at {
            ctx.push(
                stops[i].line,
                stops[i].column,
                "E_BAD_MOTION",
                format!(
                    "key {label} stops {} and {} share `at` {}s; zero-length segments are refused as duplicate times",
                    i,
                    i + 1,
                    stops[i].at
                ),
                "give each stop its own `at` time; instantaneous jumps use `hold` plus `set`",
            );
            ok = false;
        }
    }
    // Path keys (Sprint 7 motion, CAP-008): any path stop makes the
    // key a path key — `property` must be absent (the stops travel
    // their own paths) and every other stop must travel too. Morph
    // compatibility is checked per consecutive parsed pair, naming
    // the later stop; numeric keys keep the required-property check.
    let has_path = stops.iter().any(|s| s.is_path);
    let property: Option<String> = if has_path {
        if let Some(raw) = &property_raw {
            let v = find(node, "property").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_MOTION",
                format!("key property {raw:?} conflicts with path stops; a path key carries no `property` — every stop travels its own `path`"),
                "remove `property` from the key, or remove the stop `path` keys to animate one property",
            );
            ok = false;
        }
        for (i, stop) in stops.iter().enumerate() {
            if !stop.is_path && stop.has_value {
                ctx.push(
                    stop.line,
                    stop.column,
                    "E_BAD_MOTION",
                    format!("key {label} stop {} carries `value`; every stop of a path key carries `path`", i + 1),
                    format!("replace `value=<number>` with `path=\"M<x>,<y> ...\"` on stop {} of the key", i + 1),
                );
                ok = false;
            }
        }
        for (i, pair) in stops.windows(2).enumerate() {
            let (a, b) = (&pair[0], &pair[1]);
            let (Some(sa), Some(sb)) = (a.stop.as_ref(), b.stop.as_ref()) else {
                continue;
            };
            let (Some(pa), Some(pb)) = (sa.path.as_deref(), sb.path.as_deref()) else {
                continue;
            };
            // Both stops parsed (their data validated above), so these
            // re-parses cannot fail.
            let a_path = motion::path::parse(pa).expect("stop path parses after validation");
            let b_path = motion::path::parse(pb).expect("stop path parses after validation");
            ok &= motion_path_morph(
                ctx,
                &format!("key {label} stop {}", i + 2),
                "`path`",
                b.line,
                b.column,
                &a_path,
                &b_path,
            );
        }
        None
    } else {
        let property = motion_property(ctx, node, "key", clips, target);
        ok &= property.is_some();
        // Sound envelopes (Sprint 10, CAP-001) ride one plain chain of
        // stops: the visual-only keys refuse here, like tweens above.
        if let Some(p) = property.as_deref() {
            ok &= sound_directive_guards(ctx, node, "key", p);
        }
        property
    };
    if !has_path && orient.is_some() && find(node, "orient").is_some() {
        let v = find(node, "orient").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_MOTION",
            "key `orient` rides path motion; a numeric key carries no path stops, so `orient` has no tangent to follow",
            "add `path=\"M<x>,<y> ...\"` to the `frame` stops of the key, or remove `orient`",
        );
        ok = false;
    }
    // Stagger plus repeat (Sprint 7 motion, CAP-002/CAP-004): the delay
    // distribution and the replay shape; every bad key reports.
    let stagger_repeat = motion_stagger_repeat(ctx, node, "key");
    ok &= stagger_repeat.is_some();
    if !ok {
        return None;
    }
    let (stagger, stagger_from, stagger_grid, repeat, yoyo, repeat_delay) = stagger_repeat.unwrap();
    Some(KeyDecl {
        target: target.unwrap(),
        // Path keys travel their stops' paths, not a named property.
        property: property.unwrap_or_default(),
        ease,
        stops: stops.into_iter().map(|s| s.stop.unwrap()).collect(),
        stagger,
        stagger_from,
        stagger_grid,
        repeat,
        yoyo,
        repeat_delay,
        orient: orient.unwrap_or(false),
        line: node.name_line,
        column: node.name_column,
    })
}

fn validate_font(ctx: &mut Ctx, node: &RawNode) -> Option<FontDecl> {
    for (k, v) in &node.props {
        if k != "path" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown font key {k:?}"),
                "a font declares only `path`",
            );
        }
    }
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "font is missing required `path`",
                "add `path=\"fonts/Body.ttf\"` to the font",
            );
            return None;
        }
    };
    let abs = ctx.folder.join(&path);
    if !abs.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("font file {path:?} does not exist or is not readable"),
            "provide the font file at the referenced path or fix the `path`",
        );
        return None;
    }
    // A present font must be a format the CPU renderer can draw: no
    // silent substitution of a different font ever happens.
    match std::fs::read(&abs) {
        Ok(bytes) => {
            if let Err(reason) = glyph::font::check_supported(&bytes) {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_FONT",
                    format!("font file {path:?} is unsupported: {reason}"),
                    "point `path` at a TrueType (.ttf) or OpenType (.otf) font with TrueType outlines",
                );
                return None;
            }
        }
        Err(e) => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_UNREADABLE",
                format!("font file {path:?} cannot be read: {e}"),
                "make the font file readable or fix the `path`",
            );
            return None;
        }
    }
    Some(FontDecl {
        path,
        line: node.name_line,
        column: node.name_column,
    })
}

/// One named theme (Sprint 9 variables, CAP-005): a top-level `theme`
/// block collecting `token` children into a token-id to raw-value map.
/// Tokens are untyped text (colors, fonts, radius, spacing, durations,
/// easings); binding sites coerce the resolved text exactly like a
/// literal, so theme durations and easings animate identically to
/// literal values. Theme names share the variable-id shape so a
/// composition `theme` attribute and the CLI `--theme` flag name them
/// unambiguously.
struct Theme {
    name: String,
    line: usize,
    column: usize,
    tokens: BTreeMap<String, String>,
}

/// Validate one `token` element inside a `theme` block (Sprint 9
/// variables, CAP-005): a required `id` (variable-id shape, so it
/// reads inside `var(--id)`) and a required `value` kept as raw text.
/// Quoted strings substitute verbatim; bare numbers and booleans
/// substitute in their written form. A missing `id` or `value` is
/// refused naming the token; a duplicate token id keeps the first
/// value and refuses at the repeat.
fn validate_token(
    ctx: &mut Ctx,
    node: &RawNode,
    theme_name: &str,
    seen: &mut BTreeMap<String, (usize, usize)>,
) -> Option<(String, String)> {
    const KEYS: &[&str] = &["id", "value"];
    let mut ok = true;
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!(
                    "unknown `token` attribute {k:?} in theme {theme_name:?}; a `token` holds `id` and `value`"
                ),
                "remove the key or check the spelling",
            );
            ok = false;
        }
    }
    if !node.children.is_empty() {
        let child = &node.children[0];
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_VARIABLE",
            format!(
                "token carries no children in theme {theme_name:?}; a `token` is one `id` with one `value`"
            ),
            "remove the child elements from the token",
        );
        ok = false;
    }
    let id = match find(node, "id") {
        Some(v) => match as_str(v) {
            Some(s) if valid_variable_id(s) => Some(s.to_string()),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ID",
                    format!(
                        "token `id` in theme {theme_name:?} must start with a letter, then hold letters, digits, `-` or `_`; found {}",
                        raw_text(v)
                    ),
                    "set `id=\"<letters-digits-dashes-underscores starting with a letter>\"` (for example `id=\"accent\"`) on the token",
                );
                ok = false;
                None
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                format!(
                    "token in theme {theme_name:?} is missing required `id`; every token names one value"
                ),
                "add `id=\"<name starting with a letter>\"` to the token",
            );
            ok = false;
            None
        }
    };
    let value = match find(node, "value") {
        Some(v) => match &v.kind {
            ValueKind::Str(s) => Some(s.clone()),
            ValueKind::Int(n) => Some(n.to_string()),
            ValueKind::Float(x) => Some(x.to_string()),
            ValueKind::Bool(b) => Some(b.to_string()),
            ValueKind::Ratio(n, d) => Some(format!("{n}/{d}")),
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                match &id {
                    Some(token_id) => format!(
                        "token `--{token_id}` in theme {theme_name:?} is missing required `value`; every token holds one value"
                    ),
                    None => format!(
                        "token in theme {theme_name:?} is missing required `value`; every token holds one value"
                    ),
                },
                "add `value=\"...\"` to the token (a color, font list, radius, spacing, duration, or easing)",
            );
            ok = false;
            None
        }
    };
    if !ok {
        return None;
    }
    let (id, value) = (id.unwrap(), value.unwrap());
    if let Some((first_line, _)) = seen.get(&id) {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_VARIABLE",
            format!(
                "duplicate token `--{id}` in theme {theme_name:?} (first declared at line {first_line}); token ids must be unique per theme"
            ),
            format!("rename the repeat `token id=\"{id}\"` or remove it; the first value wins"),
        );
        return None;
    }
    seen.insert(id.clone(), (node.name_line, node.name_column));
    Some((id, value))
}

/// Validate one top-level `theme` block (Sprint 9 variables, CAP-005):
/// a required `name` plus only `token` children. A bad child, a bad
/// name, or a bad token refuses naming the theme; valid tokens still
/// collect so one bad token never hides the theme's good values behind
/// a second refusal.
fn validate_theme(ctx: &mut Ctx, node: &RawNode) -> Option<Theme> {
    const KEYS: &[&str] = &["name"];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!(
                    "unknown `theme` attribute {k:?}; a `theme` holds `name` with `token` children"
                ),
                "remove the key or check the spelling",
            );
        }
    }
    let name = match find(node, "name") {
        Some(v) => match as_str(v) {
            Some(s) if valid_variable_id(s) => Some(s.to_string()),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ID",
                    format!(
                        "theme `name` must start with a letter, then hold letters, digits, `-` or `_`; found {}",
                        raw_text(v)
                    ),
                    "set `name=\"<letters-digits-dashes-underscores starting with a letter>\"` (for example `name=\"brand\"`) on the theme",
                );
                None
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "theme is missing required `name`; every theme names one token collection",
                "add `name=\"<name starting with a letter>\"` to the theme",
            );
            None
        }
    };
    let who = name.clone().unwrap_or_else(|| "theme".to_string());
    let mut tokens = BTreeMap::new();
    let mut seen: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for child in &node.children {
        if child.name != "token" {
            ctx.push(
                child.name_line,
                child.name_column,
                "E_BAD_VARIABLE",
                format!(
                    "unexpected `{}` inside theme {who:?}; a `theme` holds only `token` children",
                    child.name
                ),
                "remove the element, or declare the value as `token id=\"...\" value=\"...\"`",
            );
            continue;
        }
        if let Some((id, value)) = validate_token(ctx, child, &who, &mut seen) {
            tokens.insert(id, value);
        }
    }
    // A missing name drops the block (returned `None` below); valid
    // tokens in a named-but-flawed block still resolve, so bindings
    // behind one bad token keep working instead of cascading.
    let name = match name {
        Some(n) => n,
        None => return None,
    };
    Some(Theme {
        name,
        line: node.name_line,
        column: node.name_column,
        tokens,
    })
}

/// Collect every top-level `theme` block project-wide (Sprint 9
/// variables, CAP-005), in document order. A repeated theme name
/// refuses at the repeat and merges its valid tokens into the first
/// block without overwriting, so every valid token still resolves.
fn collect_themes(ctx: &mut Ctx, roots: &[RawNode]) -> Vec<Theme> {
    let mut themes: Vec<Theme> = Vec::new();
    for node in roots {
        if node.name != "theme" {
            continue;
        }
        let Some(found) = validate_theme(ctx, node) else {
            continue;
        };
        if let Some(first) = themes.iter_mut().find(|t| t.name == found.name) {
            ctx.push(
                found.line,
                found.column,
                "E_BAD_VARIABLE",
                format!(
                    "duplicate theme {:?} (first declared at line {}); theme names must be unique",
                    found.name, first.line
                ),
                format!(
                    "rename the repeat `theme name={:?}` or remove it; the first block's tokens win",
                    found.name
                ),
            );
            for (id, value) in found.tokens {
                first.tokens.entry(id).or_insert(value);
            }
        } else {
            themes.push(found);
        }
    }
    themes
}

/// Available theme names for refusal messages (Sprint 9 variables,
/// CAP-005): quoted names in document order, or `none` when no
/// `theme` block is declared.
fn theme_available_list(themes: &[Theme]) -> String {
    if themes.is_empty() {
        "none".to_string()
    } else {
        themes
            .iter()
            .map(|t| format!("{:?}", t.name))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Resolve one composition's effective theme tokens (Sprint 9
/// variables, CAP-005): the caller theme (CLI `--theme`) beats the
/// composition `theme` attribute. An unknown name on either side
/// refuses naming the available themes; the attribute naming two
/// themes refuses naming the at-most-one rule. Returns the winning
/// token map (empty when no theme is selected or no selection
/// survived), so bindings resolve through the fixed precedence
/// (caller override, then theme token, then variable default, then
/// binding fallback) with literally written element attributes always
/// winning outright.
fn composition_theme_tokens(
    ctx: &mut Ctx,
    root: &RawNode,
    themes: &[Theme],
    cli_theme: Option<&str>,
) -> BTreeMap<String, String> {
    let comp_label = match find(root, "name").and_then(as_str) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => "(unnamed composition)".to_string(),
    };
    // Every `theme` attribute on the root, split on commas and
    // whitespace so `theme="a b"` reads as two selections.
    let mut attr_names: Vec<(String, usize, usize)> = Vec::new();
    let mut attr_loc: Option<(usize, usize)> = None;
    let mut attr_bad = false;
    for (k, v) in &root.props {
        if k != "theme" {
            continue;
        }
        attr_loc = Some((v.line, v.column));
        match as_str(v) {
            Some(raw) => {
                for part in raw
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|p| !p.is_empty())
                {
                    attr_names.push((part.to_string(), v.line, v.column));
                }
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_VARIABLE",
                    format!(
                        "composition {comp_label:?} `theme` must be a quoted theme name; found {}",
                        raw_text(v)
                    ),
                    "set `theme=\"<name>\"` to one declared `theme` block, or remove the key to render without a theme",
                );
                attr_bad = true;
            }
        }
    }
    let mut distinct: Vec<String> = attr_names.iter().map(|(n, _, _)| n.clone()).collect();
    distinct.sort();
    distinct.dedup();
    let mut attr_selected: Option<String> = None;
    if distinct.len() > 1 {
        let (line, column) = attr_loc.unwrap_or((root.name_line, root.name_column));
        let picked: Vec<String> = distinct.iter().map(|n| format!("{n:?}")).collect();
        ctx.push(
            line,
            column,
            "E_BAD_VARIABLE",
            format!(
                "composition {comp_label:?} selects {} themes ({}); a composition selects at most one theme",
                distinct.len(),
                picked.join(", ")
            ),
            "set `theme=\"<one name>\"` to a single declared `theme` block, or remove the key to render without a theme",
        );
        attr_bad = true;
    } else if distinct.len() == 1 {
        let wanted = distinct.into_iter().next().unwrap();
        if themes.iter().any(|t| t.name == wanted) {
            attr_selected = Some(wanted);
        } else {
            let (line, column) = attr_loc.unwrap_or((root.name_line, root.name_column));
            ctx.push(
                line,
                column,
                "E_BAD_VARIABLE",
                format!(
                    "unknown theme {wanted:?} selected by composition {comp_label:?}; available themes: {}",
                    theme_available_list(themes)
                ),
                format!(
                    "set `theme=` to one of the available themes, or declare `theme name={wanted:?} {{ ... }}` with its `token` children"
                ),
            );
            attr_bad = true;
        }
    } else if attr_loc.is_some() && !attr_bad {
        let (line, column) = attr_loc.unwrap();
        ctx.push(
            line,
            column,
            "E_BAD_VARIABLE",
            format!(
                "composition {comp_label:?} `theme` names no theme; available themes: {}",
                theme_available_list(themes)
            ),
            "set `theme=\"<name>\"` to one declared `theme` block, or remove the key to render without a theme",
        );
        attr_bad = true;
    }
    // The caller theme beats the composition attribute; an unknown
    // caller theme refuses the same way, located where flags report.
    let mut cli_selected: Option<String> = None;
    if let Some(wanted) = cli_theme {
        if themes.iter().any(|t| t.name == wanted) {
            cli_selected = Some(wanted.to_string());
        } else {
            ctx.push(
                1,
                1,
                "E_BAD_VARIABLE",
                format!(
                    "unknown `--theme` {wanted:?}; available themes: {}",
                    theme_available_list(themes)
                ),
                format!(
                    "pass `--theme <name>` with one of the available themes, or declare `theme name={wanted:?} {{ ... }}` with its `token` children"
                ),
            );
        }
    }
    let effective = cli_selected.or(if attr_bad { None } else { attr_selected });
    match effective {
        Some(name) => themes
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.tokens.clone())
            .unwrap_or_default(),
        None => BTreeMap::new(),
    }
}

/// Variable `id` syntax (Sprint 9 variables, CAP-001): starts with an
/// ASCII letter, then ASCII letters, digits, dashes, or underscores.
/// Stricter than clip ids (which allow a leading digit) so a variable
/// name always reads as a name inside `var(--id)`.
fn valid_variable_id(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Split one quoted comma-separated list (Sprint 9 variables,
/// CAP-001): entries trim whitespace and empties drop, exactly like
/// `font-family` lists.
fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Validate a `variable` element (Sprint 9 variables, CAP-001): a
/// required `id` (letter-led, unique across the project), a required
/// `type` (one of the seven declared types), an optional `label`,
/// an optional `default` type-checked against the declaration, and
/// for `enum` a required `options` list of at least two entries the
/// default is drawn from. An unused declaration stays valid here
/// (unused reporting runs project-wide after substitution: a warning
/// in default mode, an error under `--strict-variables`); every bad
/// declaration is refused before frame one naming the declaration
/// and the fix.
fn validate_variable(ctx: &mut Ctx, node: &RawNode) -> Option<VariableDecl> {
    const KEYS: &[&str] = &["id", "type", "default", "label", "options"];
    let mut ok = true;
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!(
                    "unknown `variable` attribute {k:?}; a `variable` holds `id`, `type`, `default`, `label`, and (`enum` only) `options`"
                ),
                "remove the key or check the spelling",
            );
            ok = false;
        }
    }
    if !node.children.is_empty() {
        let child = &node.children[0];
        ctx.push(
            child.name_line,
            child.name_column,
            "E_BAD_VARIABLE",
            "variable carries no children; a `variable` is one `id` with one `type`, one `default`, and (`enum` only) one `options` list",
            "remove the child elements from the variable",
        );
        ok = false;
    }
    let id = match find(node, "id") {
        Some(v) => match as_str(v) {
            Some(s) if valid_variable_id(s) => Some(s.to_string()),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ID",
                    format!(
                        "variable `id` must start with a letter, then hold letters, digits, `-` or `_`; found {}",
                        raw_text(v)
                    ),
                    "set `id=\"<letters-digits-dashes-underscores starting with a letter>\"` (for example `id=\"title\"`) on the variable",
                );
                ok = false;
                None
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "variable is missing required `id`; every declaration names one variable",
                "add `id=\"<name starting with a letter>\"` to the variable",
            );
            ok = false;
            None
        }
    };
    // The declaration name used inside diagnostics below: the id when
    // declared, else the bare element.
    let who = id.clone().unwrap_or_else(|| "variable".to_string());
    let var_type = match find(node, "type") {
        Some(v) => match as_str(v).and_then(VariableType::parse) {
            Some(t) => Some(t),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_VARIABLE",
                    format!(
                        "variable {who:?} has unknown type {}; accepted types are: {}",
                        raw_text(v),
                        VariableType::ACCEPTED.join(", ")
                    ),
                    "set `type=` to one of `string`, `number`, `color`, `boolean`, `enum`, `font`, or `image`",
                );
                ok = false;
                None
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                format!("variable {who:?} is missing required `type`; every declaration names one of the seven types"),
                "add `type=\"string\"` (or `number`, `color`, `boolean`, `enum`, `font`, `image`) to the variable",
            );
            ok = false;
            None
        }
    };
    let label = match find(node, "label") {
        None => None,
        Some(v) => match as_str(v) {
            Some(s) => Some(s.to_string()),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_LABEL",
                    format!(
                        "variable {who:?} `label` must be a quoted editor note; found {}",
                        raw_text(v)
                    ),
                    "set `label=\"...\"` to a quoted editor note, or remove the key; labels never change rendered output",
                );
                ok = false;
                None
            }
        },
    };
    // `options` belongs to `enum` only: any other type carrying it is
    // refused, and an `enum` without it (or with fewer than two
    // entries) is refused naming the rule.
    let options = match var_type {
        Some(VariableType::Enum) => match find(node, "options").and_then(as_str) {
            Some(s) => {
                let entries = split_list(s);
                if entries.len() < 2 {
                    let v = find(node, "options").unwrap();
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_VARIABLE",
                        format!(
                            "variable {who:?} `enum` needs at least two `options`; found {} ({})",
                            entries.len(),
                            raw_text(v)
                        ),
                        "declare `options=\"<first>, <second>\"` with at least two comma-separated entries",
                    );
                    ok = false;
                    None
                } else {
                    Some(entries)
                }
            }
            None => {
                let present = find(node, "options");
                match present {
                    Some(v) => {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_BAD_VARIABLE",
                            format!(
                                "variable {who:?} `options` must be a quoted comma-separated list; found {}",
                                raw_text(v)
                            ),
                            "declare `options=\"<first>, <second>\"` with at least two comma-separated entries",
                        );
                        ok = false;
                        None
                    }
                    None => {
                        ctx.push(
                            node.name_line,
                            node.name_column,
                            "E_MISSING_FIELD",
                            format!(
                                "variable {who:?} of type `enum` is missing required `options`; every `enum` lists at least two entries"
                            ),
                            "add `options=\"<first>, <second>\"` with at least two comma-separated entries to the variable",
                        );
                        ok = false;
                        None
                    }
                }
            }
        },
        _ => match find(node, "options") {
            Some(v) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_VARIABLE",
                    format!(
                        "variable {who:?} carries `options`, but only `enum` declarations hold `options`"
                    ),
                    "remove `options`, or set `type=\"enum\"` with at least two entries",
                );
                ok = false;
                None
            }
            None => None,
        },
    };
    // The declared `default`, type-checked against the declaration.
    // Absent means default-less (strict validation requires a caller
    // value for it); present must fit the type.
    let default = match (find(node, "default"), var_type) {
        (None, _) => None,
        (Some(_), None) => {
            // The type already failed above; the default stays silent
            // so one bad value yields one diagnostic, never a cascade.
            ok = false;
            None
        }
        (Some(v), Some(t)) => match variable_default(ctx, v, &who, t, options.as_deref()) {
            Some(d) => Some(d),
            None => {
                ok = false;
                None
            }
        },
    };
    if !ok {
        return None;
    }
    let (id, var_type) = (id.unwrap(), var_type.unwrap());
    ctx.ids.push(IdEntry {
        value: id.clone(),
        line: node.name_line,
        column: node.name_column,
        is_composition: false,
        is_mark: false,
        comp: ctx.current_comp,
    });
    Some(VariableDecl {
        id,
        var_type,
        default,
        label,
        options,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Type-check one variable `default` (Sprint 9 variables, CAP-001):
/// the raw value coerced to the declaration's type. A mismatch is
/// refused naming the declaration and the expected shape; an `enum`
/// default must match one `options` entry exactly and
/// case-sensitively. An `image` path must exist and read (missing
/// reads as missing, denied reads as unreadable); a `font` list
/// follows the same syntax check as `font-family` entries (resolution
/// happens at bind time, where missing families skip per entry and
/// an all-unknown list fails closed).
fn variable_default(
    ctx: &mut Ctx,
    v: &RawValue,
    who: &str,
    var_type: VariableType,
    options: Option<&[String]>,
) -> Option<VariableDefault> {
    match var_type {
        VariableType::String => {
            // Kept verbatim: quoted text as written, other scalars in
            // their written text form (a number-looking value never
            // auto-converts a `string` variable).
            let s = match &v.kind {
                ValueKind::Str(text) => text.clone(),
                _ => raw_text(v),
            };
            Some(VariableDefault::Str(s))
        }
        VariableType::Number => match as_f64(v) {
            Some(x) if x.is_finite() => Some(VariableDefault::Num(x)),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_VARIABLE",
                    format!(
                        "variable {who:?} default {} is not a number; a `number` default holds decimals like `default=1.5`",
                        raw_text(v)
                    ),
                    "set `default=<number>` (for example `default=1.5`) on the variable, or remove the key for a default-less declaration",
                );
                None
            }
        },
        VariableType::Color => match as_str(v).and_then(parse_color) {
            Some(c) => Some(VariableDefault::Color(c)),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_VARIABLE",
                    format!(
                        "variable {who:?} default {} is not a `#rgb` or `#rrggbb` color",
                        raw_text(v)
                    ),
                    "set `default=\"#rrggbb\"` (for example `default=\"#ff3366\"`) on the variable, or remove the key for a default-less declaration",
                );
                None
            }
        },
        VariableType::Boolean => {
            let b = match &v.kind {
                ValueKind::Bool(flag) => Some(*flag),
                ValueKind::Str(s) if s == "true" => Some(true),
                ValueKind::Str(s) if s == "false" => Some(false),
                _ => None,
            };
            match b {
                Some(flag) => Some(VariableDefault::Bool(flag)),
                None => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_VARIABLE",
                        format!(
                            "variable {who:?} default {} is not a boolean; accepted spellings are `true` and `false`",
                            raw_text(v)
                        ),
                        "set `default=true` or `default=false` (bare or quoted) on the variable, or remove the key for a default-less declaration",
                    );
                    None
                }
            }
        }
        VariableType::Enum => {
            let s = match &v.kind {
                ValueKind::Str(text) => text.clone(),
                _ => raw_text(v),
            };
            match options {
                // `options` already failed above; stay silent here so
                // one bad declaration yields one diagnostic.
                None => None,
                Some(entries) if entries.iter().any(|e| e == &s) => Some(VariableDefault::Enum(s)),
                Some(entries) => {
                    let list = entries
                        .iter()
                        .map(|e| format!("{e:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_VARIABLE",
                        format!(
                            "variable {who:?} default {s:?} is not one of its `options`: {list}",
                        ),
                        "set `default=` to one of the declared `options`, or add the value to `options`",
                    );
                    None
                }
            }
        }
        VariableType::Font => match as_str(v) {
            Some(s) => {
                let names = split_list(s);
                if names.is_empty() {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_VARIABLE",
                        format!("variable {who:?} default font list names no family"),
                        "set `default=\"Family Name\"` or `default=\"First, Fallback\"`, or remove the key for a default-less declaration",
                    );
                    None
                } else {
                    Some(VariableDefault::Font(names))
                }
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_VARIABLE",
                    format!(
                        "variable {who:?} default must be a quoted font family list; found {}",
                        raw_text(v)
                    ),
                    "set `default=\"Family Name\"` or `default=\"First, Fallback\"`, or remove the key for a default-less declaration",
                );
                None
            }
        },
        VariableType::Image => match as_str(v) {
            Some(s) => {
                let abs = ctx.folder.join(s);
                if !abs.is_file() {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_MISSING_FILE",
                        format!(
                            "variable {who:?} default image path {s:?} does not exist or is not readable"
                        ),
                        "provide the file at the referenced path relative to the project folder, or fix the `default`",
                    );
                    None
                } else {
                    let readable = std::fs::File::open(&abs)
                        .and_then(|mut f| {
                            use std::io::Read;
                            f.read(&mut [0u8; 1]).map(|_| ())
                        })
                        .is_ok();
                    if !readable {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_UNREADABLE",
                            format!("variable {who:?} default image path {s:?} cannot be read"),
                            "make the file readable, or fix the `default`",
                        );
                        None
                    } else {
                        Some(VariableDefault::Image(s.to_string()))
                    }
                }
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_VARIABLE",
                    format!(
                        "variable {who:?} default must be a quoted project-relative image path; found {}",
                        raw_text(v)
                    ),
                    "set `default=\"media/photo.png\"` to a quoted path relative to the project folder, or remove the key for a default-less declaration",
                );
                None
            }
        },
    }
}

/// One substituted `var()` use site (Sprint 9 variables, CAP-002): the
/// attribute location plus the variable ids the value drew on. Recorded
/// for every successfully parsed binding (even one whose own resolution
/// failed), so the end-of-validation amendment can name the variable
/// behind any coercion failure at the same location.
#[derive(Debug, Clone)]
struct BindingSite {
    line: usize,
    column: usize,
    node: String,
    key: String,
    vars: Vec<String>,
    /// File diagnostics (`E_MISSING_FILE` and friends) sit on the
    /// enclosing node, not the value: a site with `needs_text` only
    /// amends a diagnostic at its location when the message also
    /// quotes the resolved text. `None` for value-location sites,
    /// which match by location alone.
    needs_text: Option<String>,
}

/// The declared default of one variable as bindable text (Sprint 9
/// variables, CAP-002): the literal the binding substitutes before the
/// attribute coerces it. Strings substitute verbatim (a number-looking
/// value never auto-converts); every other type renders its canonical
/// spelling.
fn variable_binding_text(default: &VariableDefault) -> String {
    match default {
        VariableDefault::Str(s) => s.clone(),
        VariableDefault::Num(x) => x.to_string(),
        VariableDefault::Color(c) => format!("#{}", c.to_hex()),
        VariableDefault::Bool(b) => b.to_string(),
        VariableDefault::Enum(s) => s.clone(),
        VariableDefault::Font(names) => names.join(", "),
        VariableDefault::Image(path) => path.clone(),
    }
}

/// One parsed `var(--id)` or `var(--id, fallback)` call: byte span in
/// the attribute value plus the referenced id and optional fallback.
struct VarCall {
    id: String,
    fallback: Option<String>,
    start: usize,
    end: usize,
}

/// Advance one Unicode scalar value from byte offset `j`.
fn adv_char(raw: &str, j: usize) -> usize {
    j + raw[j..].chars().next().map(|c| c.len_utf8()).unwrap_or(1)
}

/// Skip ASCII whitespace from byte offset `j`.
fn skip_ws(raw: &str, mut j: usize) -> usize {
    while j < raw.len() {
        let b = raw.as_bytes()[j];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            j += 1;
        } else {
            break;
        }
    }
    j
}

/// Parse the single `var()` call opening at byte offset `abs` (the
/// `v` of `var(`). Returns the call plus the offset past its closing
/// paren, or a short syntax detail for the diagnostic.
fn parse_one_var_call(raw: &str, abs: usize) -> Result<(VarCall, usize), String> {
    let mut j = skip_ws(raw, abs + 4);
    if !raw[j..].starts_with("--") {
        return Err("expected `--<id>` after `var(`".to_string());
    }
    j += 2;
    let start = j;
    while j < raw.len() {
        let b = raw.as_bytes()[j];
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            j += 1;
        } else {
            break;
        }
    }
    let name = &raw[start..j];
    if !valid_variable_id(name) {
        return Err(format!(
            "expected a variable name like `--title` after `var(`, found `--{name}`"
        ));
    }
    j = skip_ws(raw, j);
    if j >= raw.len() {
        return Err(format!("unclosed `var(--{name})`; expected `)`"));
    }
    if raw.as_bytes()[j] == b')' {
        return Ok((
            VarCall {
                id: name.to_string(),
                fallback: None,
                start: abs,
                end: j + 1,
            },
            j + 1,
        ));
    }
    if raw.as_bytes()[j] != b',' {
        return Err(format!("expected `,` or `)` after `var(--{name}`"));
    }
    j = skip_ws(raw, j + 1);
    // The fallback runs to the matching close paren so nested calls
    // (`var(--a, var(--b))`) resolve; the depth count keeps parens
    // inside the fallback (which resolve recursively) balanced.
    let fb_start = j;
    let mut depth = 0usize;
    while j < raw.len() {
        match raw.as_bytes()[j] {
            b'(' => {
                depth += 1;
                j += 1;
            }
            b')' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                j += 1;
            }
            _ => {
                j = adv_char(raw, j);
            }
        }
    }
    if j >= raw.len() {
        return Err(format!("unclosed `var(--{name}, ...)`; expected `)`"));
    }
    let fallback = raw[fb_start..j].trim().to_string();
    if fallback.is_empty() {
        return Err(format!(
            "`var(--{name},)` needs a fallback value after the comma"
        ));
    }
    Ok((
        VarCall {
            id: name.to_string(),
            fallback: Some(fallback),
            start: abs,
            end: j + 1,
        },
        j + 1,
    ))
}

/// Parse every `var()` call in an attribute value, in order. A `var(`
/// glued to a longer identifier (letters, digits, `_`, `-`, `.`
/// before it) is literal text, never a call; any other `var(` that
/// fails to parse is bad syntax.
fn parse_var_calls(raw: &str) -> Result<Vec<VarCall>, String> {
    let mut calls = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let rel = match raw[i..].find("var(") {
            Some(p) => p,
            None => break,
        };
        let abs = i + rel;
        if abs > 0 {
            let prev = raw.as_bytes()[abs - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' || prev == b'.' {
                i = abs + 4;
                continue;
            }
        }
        match parse_one_var_call(raw, abs) {
            Ok((call, next)) => {
                calls.push(call);
                i = next;
            }
            Err(detail) => return Err(detail),
        }
    }
    Ok(calls)
}

/// Why one fallback failed to resolve (Sprint 9 variables, CAP-002):
/// carried outward so the diagnostic names the outer attribute.
enum FallbackError {
    /// A nested call names an undeclared variable with no fallback.
    Unknown(String),
    /// A nested call names a default-less variable with no fallback.
    NoValue(String),
    /// A nested fallback is malformed or nested too deep.
    Syntax(String),
}

/// Resolve nested `var()` calls inside one fallback (Sprint 9
/// variables, CAP-002/CAP-005): plain text passes through, calls
/// resolve like any binding through the fixed precedence (caller
/// override already swapped into the default, then theme token, then
/// variable default, then the nested fallback). Depth-capped so
/// fallbacks cannot nest forever. Silent: the caller diagnoses the
/// failure against the outer attribute.
fn resolve_var_text(
    vars: &[VariableDecl],
    theme: &BTreeMap<String, String>,
    overridden: &BTreeSet<String>,
    raw: &str,
    depth: usize,
) -> Result<Option<String>, FallbackError> {
    if depth > 8 {
        return Err(FallbackError::Syntax("nested too deep".to_string()));
    }
    let calls = match parse_var_calls(raw) {
        Ok(c) => c,
        Err(detail) => return Err(FallbackError::Syntax(detail)),
    };
    if calls.is_empty() {
        return Ok(Some(raw.to_string()));
    }
    let mut out = String::new();
    let mut last = 0;
    for call in &calls {
        out.push_str(&raw[last..call.start]);
        // Fixed precedence inside nested fallbacks too: a caller
        // override wins outright, then the theme token, then the
        // variable default, then the nested fallback.
        let hit: Option<String> = match vars.iter().find(|d| d.id == call.id) {
            Some(d) => match &d.default {
                Some(def) if overridden.contains(&d.id) => Some(variable_binding_text(def)),
                _ => theme
                    .get(&call.id)
                    .cloned()
                    .or_else(|| d.default.as_ref().map(variable_binding_text)),
            },
            None => theme.get(&call.id).cloned(),
        };
        match hit {
            Some(text) => out.push_str(&text),
            None => match &call.fallback {
                Some(fb) => match resolve_var_text(vars, theme, overridden, fb, depth + 1) {
                    Ok(rep) => {
                        if let Some(rep) = rep {
                            out.push_str(&rep);
                        } else {
                            return Ok(None);
                        }
                    }
                    Err(e) => return Err(e),
                },
                None => {
                    return Err(if vars.iter().any(|d| d.id == call.id) {
                        FallbackError::NoValue(call.id.clone())
                    } else {
                        FallbackError::Unknown(call.id.clone())
                    });
                }
            },
        }
        last = call.end;
    }
    out.push_str(&raw[last..]);
    Ok(Some(out))
}

/// Resolve one parsed call through the fixed precedence (Sprint 9
/// variables, CAP-002/CAP-005): the caller override (already swapped
/// into the default by the override pass), then the theme token, then
/// the variable default, then the fallback (itself resolved for nested
/// calls). A call resolving through nothing refuses here (REC-004)
/// naming the attribute, the token, and where to define it.
fn resolve_one_var_call(
    ctx: &mut Ctx,
    vars: &[VariableDecl],
    theme: &BTreeMap<String, String>,
    overridden: &BTreeSet<String>,
    element: &str,
    key: &str,
    line: usize,
    column: usize,
    call: &VarCall,
    depth: usize,
) -> Option<String> {
    // Diagnose a failed fallback against this attribute: the message
    // names the attribute plus the variable or syntax behind it.
    let bad_fallback = |ctx: &mut Ctx, err: FallbackError| {
        let (message, fix) = match err {
            FallbackError::Unknown(id) => (
                format!(
                    "`{element}` `{key}` fallback for `--{}` binds unknown variable `--{id}`; no `variable` in this composition declares `--{id}` and no theme selected for this render provides token `--{id}`",
                    call.id
                ),
                format!(
                    "declare `variable id=\"{id}\"` in the composition, add `token id=\"{id}\" value=\"...\"` to the selected theme, or use plain text in the fallback"
                ),
            ),
            FallbackError::NoValue(id) => (
                format!(
                    "`{element}` `{key}` fallback for `--{}` uses variable `--{id}` with no value; the declaration carries no `default` and no theme selected for this render provides token `--{id}`",
                    call.id
                ),
                format!("add `default=<value>` to `variable id=\"{id}\"`, add `token id=\"{id}\" value=\"...\"` to the selected theme, or use plain text in the fallback"),
            ),
            FallbackError::Syntax(detail) => (
                format!(
                    "`{element}` `{key}` fallback for `--{}` holds bad `var()` syntax ({detail}); expected `var(--id)` or `var(--id, fallback)`",
                    call.id
                ),
                format!(
                    "write `var(--{}, <fallback>)` with plain text or nested `var()` fallbacks in `{key}`",
                    call.id
                ),
            ),
        };
        ctx.push(line, column, "E_BAD_VARIABLE", message, fix);
    };
    // Fixed precedence: a caller override (already swapped into the
    // default, tracked in `overridden`) wins outright, then the theme
    // token, then the variable default, then the binding fallback.
    let hit: Option<String> = match vars.iter().find(|d| d.id == call.id) {
        Some(d) => match &d.default {
            Some(def) if overridden.contains(&d.id) => Some(variable_binding_text(def)),
            _ => theme
                .get(&call.id)
                .cloned()
                .or_else(|| d.default.as_ref().map(variable_binding_text)),
        },
        None => theme.get(&call.id).cloned(),
    };
    match hit {
        Some(text) => Some(text),
        None => {
            let declared = vars.iter().any(|d| d.id == call.id);
            match &call.fallback {
                Some(fb) => match resolve_var_text(vars, theme, overridden, fb, depth + 1) {
                    Ok(rep) => rep,
                    Err(e) => {
                        bad_fallback(ctx, e);
                        None
                    }
                },
                None => {
                    if declared {
                        ctx.push(
                            line,
                            column,
                            "E_BAD_VARIABLE",
                            format!(
                                "`{element}` `{key}` uses variable `--{}` with no value; the declaration carries no `default`, no `--variables` value was supplied, no theme selected for this render provides token `--{}`, and the binding carries no fallback",
                                call.id, call.id
                            ),
                            format!(
                                "add `default=<value>` to `variable id=\"{0}\"`, add `token id=\"{0}\" value=\"...\"` to the selected theme, pass `--variables '{{\"{0}\": ...}}'` for this render, or write `var(--{0}, <fallback>)` in `{key}`",
                                call.id
                            ),
                        );
                    } else {
                        ctx.push(
                            line,
                            column,
                            "E_BAD_VARIABLE",
                            format!(
                                "`{element}` `{key}` binds unknown variable `--{}`; no `variable` in this composition declares `--{}`, no theme selected for this render provides token `--{}`, and the binding carries no fallback",
                                call.id, call.id, call.id
                            ),
                            format!(
                                "declare `variable id=\"{0}\" type=\"...\"` in the composition, add `token id=\"{0}\" value=\"...\"` to the selected theme, pass `--variables '{{\"{0}\": ...}}'` for this render, or write `var(--{0}, <fallback>)` in `{key}`",
                                call.id
                            ),
                        );
                    }
                    None
                }
            }
        }
    }
}

/// Resolve every `var()` call in one attribute value (Sprint 9
/// variables, CAP-002/CAP-005). Returns the substituted text, the
/// referenced variable ids, and whether the whole value is one bare
/// call. Refusals name the attribute (and the variable, when there is
/// one).
fn resolve_binding_value(
    ctx: &mut Ctx,
    vars: &[VariableDecl],
    theme: &BTreeMap<String, String>,
    overridden: &BTreeSet<String>,
    element: &str,
    key: &str,
    raw: &str,
    line: usize,
    column: usize,
) -> Option<(String, Vec<String>, bool)> {
    let calls = match parse_var_calls(raw) {
        Ok(c) => c,
        Err(detail) => {
            ctx.push(
                line,
                column,
                "E_BAD_VARIABLE",
                format!(
                    "`{element}` `{key}` holds bad `var()` syntax ({detail}); expected `var(--id)` or `var(--id, fallback)`"
                ),
                format!(
                    "write `var(--<name starting with a letter>)` in `{key}` or remove the binding"
                ),
            );
            return None;
        }
    };
    if calls.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut last = 0;
    let mut ids = Vec::new();
    for call in &calls {
        out.push_str(&raw[last..call.start]);
        match resolve_one_var_call(
            ctx, vars, theme, overridden, element, key, line, column, call, 0,
        ) {
            Some(rep) => out.push_str(&rep),
            None => return None,
        }
        if !ids.contains(&call.id) {
            ids.push(call.id.clone());
        }
        last = call.end;
    }
    out.push_str(&raw[last..]);
    let whole = calls.len() == 1
        && raw[..calls[0].start].trim().is_empty()
        && raw[calls[0].end..].trim().is_empty();
    Some((out, ids, whole))
}

/// True when `var()` is refused in this attribute (Sprint 9 variables,
/// CAP-002): structural references (`id`, `track`, `start-after`
/// targets on any element) and the composition timing foundation
/// (`name`, `width`, `height`, `duration`, `fps`) stay literal so
/// references and timing never vary per render.
fn var_binding_structural(element: &str, key: &str, is_composition: bool) -> bool {
    let _ = element;
    if key == "id" || key == "track" || key == "start-after" || key == "start_after" {
        return true;
    }
    is_composition && matches!(key, "name" | "width" | "height" | "duration" | "fps")
}

/// Attribute keys whose readers take only bare integers (Sprint 9
/// variables, CAP-002): a whole-value binding resolving to whole
/// digits converts kind so it reads exactly like the literal.
const VAR_INT_KEYS: &[&str] = &["order", "decimals", "target", "version", "repeat"];

/// Attribute keys whose readers take only bare booleans (Sprint 9
/// variables, CAP-002): a whole-value binding resolving to exactly
/// `true`/`false` converts kind so it reads exactly like the literal.
const VAR_BOOL_KEYS: &[&str] = &[
    "hidden",
    "typewriter",
    "loop",
    "yoyo",
    "italic",
    "orient",
    "autoplay",
    "waveform",
    "detect-silence",
    "detect_silence",
];

/// Substitute every `var(--id)` binding in the composition tree
/// (Sprint 9 variables, CAP-002/CAP-005): allowed attributes resolve
/// through the fixed precedence (caller override already swapped into
/// the default, then theme token, then variable default, then inline
/// fallback) and rewrite in place, so every existing reader coerces
/// the resolved text exactly like a literal. Structural references and
/// timing keys refuse naming the location and reset to a benign
/// placeholder, so one bad value yields exactly the one binding
/// diagnostic. `variable` and `theme` subtrees stay literal (defaults
/// and token values are the resolution source, never bindings), and
/// the composition `root` mark and `theme` selection stay literal
/// (render-root and theme selection are not per-render values).
fn substitute_var_bindings(
    ctx: &mut Ctx,
    node: &mut RawNode,
    is_composition: bool,
    vars: &[VariableDecl],
    theme: &BTreeMap<String, String>,
    overridden: &BTreeSet<String>,
    comp_name: &str,
) {
    if node.name == "variable" || node.name == "theme" {
        return;
    }
    let element = node.name.clone();
    for (key, value) in node.props.iter_mut() {
        if is_composition && (key == "root" || key == "theme") {
            continue;
        }
        let raw = match &value.kind {
            ValueKind::Str(s) => s.clone(),
            _ => continue,
        };
        if !raw.contains("var(") {
            continue;
        }
        let (line, column) = (value.line, value.column);
        if var_binding_structural(&element, key, is_composition) {
            let loc = if is_composition {
                format!("composition `{key}`")
            } else {
                format!("`{element}` `{key}`")
            };
            let id_part = match parse_var_calls(&raw) {
                Ok(calls) if !calls.is_empty() => format!("`var(--{})`", calls[0].id),
                _ => "`var()`".to_string(),
            };
            ctx.push(
                line,
                column,
                "E_BAD_VARIABLE",
                format!(
                    "{loc} must not contain {id_part}; structural references and the composition timing foundation stay literal so references and timing never vary per render"
                ),
                format!("remove the `var()` from {loc} and write the literal value"),
            );
            // Reset to a benign placeholder so the refusal above stays
            // the single diagnostic for this value: a unique id for
            // `id` keys, the graphics lane for `track`, the composition
            // itself for `start-after` targets, and plain valid numbers
            // for timing keys. The verdict is already invalid, so the
            // placeholder never renders.
            let placeholder: Option<ValueKind> = if key == "id" {
                Some(ValueKind::Str(format!("__var_refused_{line}_{column}")))
            } else if key == "track" {
                Some(ValueKind::Str("graphics".to_string()))
            } else if (key == "start-after" || key == "start_after")
                && !comp_name.is_empty()
                && !comp_name.contains("var(")
            {
                Some(ValueKind::Str(comp_name.to_string()))
            } else if is_composition && (key == "width" || key == "height") {
                Some(ValueKind::Str("64".to_string()))
            } else if is_composition && key == "duration" {
                Some(ValueKind::Str("1".to_string()))
            } else if is_composition && key == "fps" {
                Some(ValueKind::Int(30))
            } else {
                None
            };
            if let Some(kind) = placeholder {
                value.kind = kind;
            }
            continue;
        }
        match resolve_binding_value(
            ctx, vars, theme, overridden, &element, key, &raw, line, column,
        ) {
            Some((out, ids, whole)) => {
                let converted = if whole && VAR_BOOL_KEYS.contains(&key.as_str()) {
                    match out.as_str() {
                        "true" => Some(ValueKind::Bool(true)),
                        "false" => Some(ValueKind::Bool(false)),
                        _ => None,
                    }
                } else if whole && VAR_INT_KEYS.contains(&key.as_str()) {
                    out.parse::<i64>().ok().map(ValueKind::Int)
                } else {
                    None
                };
                value.kind = converted.unwrap_or(ValueKind::Str(out.clone()));
                ctx.bindings.push(BindingSite {
                    line,
                    column,
                    node: element.clone(),
                    key: key.clone(),
                    vars: ids.clone(),
                    needs_text: None,
                });
                // Media checks diagnose at the node: a second site lets
                // the amendment name the variable there too, but only
                // when the message quotes this resolved text.
                if (key == "path" || key == "src") && out.len() >= 2 {
                    ctx.bindings.push(BindingSite {
                        line: node.name_line,
                        column: node.name_column,
                        node: element.clone(),
                        key: key.clone(),
                        vars: ids,
                        needs_text: Some(out),
                    });
                }
            }
            None => {
                // The binding already diagnosed itself above; record the
                // parsed ids (when parseable) so a same-location shape
                // diagnostic still names the variable.
                if let Ok(calls) = parse_var_calls(&raw) {
                    if !calls.is_empty() {
                        ctx.bindings.push(BindingSite {
                            line,
                            column,
                            node: element.clone(),
                            key: key.clone(),
                            vars: calls.into_iter().map(|c| c.id).collect(),
                            needs_text: None,
                        });
                    }
                }
            }
        }
    }
    for child in node.children.iter_mut() {
        substitute_var_bindings(ctx, child, false, vars, theme, overridden, comp_name);
    }
}

/// Coerce one caller override to its declaration's type (Sprint 9
/// variables, CAP-003): the JSON scalar becomes the typed default the
/// binding pass substitutes, mirroring `variable_default` but against
/// the override value instead of KDL. A value outside the type refuses
/// at the declaration, naming the variable and the expected shape; an
/// `image` path must exist and read, like a declared default.
fn coerce_override(
    ctx: &mut Ctx,
    decl: &VariableDecl,
    value: &OverrideValue,
) -> Option<VariableDefault> {
    let who = decl.id.as_str();
    let expected = match decl.var_type {
        VariableType::String => "",
        VariableType::Number => "a finite decimal number like `1.5`",
        VariableType::Color => "a `#rgb` or `#rrggbb` color like `\"#ff3366\"`",
        VariableType::Boolean => "`true` or `false`",
        VariableType::Enum => "",
        VariableType::Font => "a quoted font family list like `\"Family Name\"`",
        VariableType::Image => "a quoted project-relative image path like `\"media/photo.png\"`",
    };
    let bad = |ctx: &mut Ctx, expected: String| {
        ctx.push(
            decl.line,
            decl.column,
            "E_BAD_VARIABLE",
            format!(
                "`--variables` value for variable `--{who}` of type `{}` holds {}; expected {expected}",
                decl.var_type.name(),
                override_text(value),
            ),
            format!(
                "pass {expected} for `--{who}` in `--variables`, or remove the key to keep the declared default"
            ),
        );
    };
    match decl.var_type {
        VariableType::String => Some(match value {
            OverrideValue::Str(s) => VariableDefault::Str(s.clone()),
            OverrideValue::Num(x) => VariableDefault::Str(x.to_string()),
            OverrideValue::Bool(b) => VariableDefault::Str(b.to_string()),
        }),
        VariableType::Number => match value {
            OverrideValue::Num(x) if x.is_finite() => Some(VariableDefault::Num(*x)),
            OverrideValue::Str(s) => match s.parse::<f64>() {
                Ok(x) if x.is_finite() => Some(VariableDefault::Num(x)),
                _ => {
                    bad(ctx, expected.to_string());
                    None
                }
            },
            _ => {
                bad(ctx, expected.to_string());
                None
            }
        },
        VariableType::Color => match value {
            OverrideValue::Str(s) => match parse_color(s) {
                Some(c) => Some(VariableDefault::Color(c)),
                None => {
                    bad(ctx, expected.to_string());
                    None
                }
            },
            _ => {
                bad(ctx, expected.to_string());
                None
            }
        },
        VariableType::Boolean => {
            let flag = match value {
                OverrideValue::Bool(b) => Some(*b),
                OverrideValue::Str(s) if s == "true" => Some(true),
                OverrideValue::Str(s) if s == "false" => Some(false),
                _ => None,
            };
            match flag {
                Some(b) => Some(VariableDefault::Bool(b)),
                None => {
                    bad(ctx, expected.to_string());
                    None
                }
            }
        }
        VariableType::Enum => match value {
            OverrideValue::Str(s) => {
                let options = decl.options.as_deref().unwrap_or(&[]);
                if options.iter().any(|e| e == s) {
                    Some(VariableDefault::Enum(s.clone()))
                } else {
                    let list = options
                        .iter()
                        .map(|e| format!("{e:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    bad(ctx, format!("one of its `options`: {list}"));
                    None
                }
            }
            _ => {
                bad(
                    ctx,
                    match decl.options.as_deref() {
                        Some(options) => {
                            let list = options
                                .iter()
                                .map(|e| format!("{e:?}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("one of its `options`: {list}")
                        }
                        None => "one of its `options`".to_string(),
                    },
                );
                None
            }
        },
        VariableType::Font => match value {
            OverrideValue::Str(s) => {
                let names = split_list(s);
                if names.is_empty() {
                    bad(ctx, expected.to_string());
                    None
                } else {
                    Some(VariableDefault::Font(names))
                }
            }
            _ => {
                bad(ctx, expected.to_string());
                None
            }
        },
        VariableType::Image => match value {
            OverrideValue::Str(s) => {
                let abs = ctx.folder.join(s);
                if !abs.is_file() {
                    ctx.push(
                        decl.line,
                        decl.column,
                        "E_MISSING_FILE",
                        format!(
                            "`--variables` value for variable `--{who}` image path {s:?} does not exist or is not readable"
                        ),
                        format!(
                            "provide the file at the referenced path relative to the project folder, or remove the key to keep the declared default"
                        ),
                    );
                    None
                } else {
                    let readable = std::fs::File::open(&abs)
                        .and_then(|mut f| {
                            use std::io::Read;
                            f.read(&mut [0u8; 1]).map(|_| ())
                        })
                        .is_ok();
                    if !readable {
                        ctx.push(
                            decl.line,
                            decl.column,
                            "E_UNREADABLE",
                            format!(
                                "`--variables` value for variable `--{who}` image path {s:?} cannot be read"
                            ),
                            "make the file readable, or remove the key to keep the declared default".to_string(),
                        );
                        None
                    } else {
                        Some(VariableDefault::Image(s.clone()))
                    }
                }
            }
            _ => {
                bad(ctx, expected.to_string());
                None
            }
        },
    }
}

/// Validate the `variable` declarations of one composition, then
/// substitute every `var()` binding across it (Sprint 9 variables,
/// CAP-002/CAP-005). Declarations validate before any clip so
/// bindings resolve whatever the document order; the caller theme
/// (or the composition `theme` attribute) selects the token map, and
/// substitution rewrites bound attributes in place so existing readers
/// coerce resolved text like literals.
fn collect_variables_and_substitute(
    ctx: &mut Ctx,
    root: &mut RawNode,
    themes: &[Theme],
    cli_theme: Option<&str>,
) -> Vec<VariableDecl> {
    let mut variables = Vec::new();
    for child in root.children.iter() {
        if child.name == "variable" {
            if let Some(d) = validate_variable(ctx, child) {
                variables.push(d);
            }
        }
    }
    // Sprint 9 variables, CAP-003: caller overrides replace matching
    // defaults for this render only. Only the in-memory default swaps —
    // id, type, and options stay declared — so the binding pass below
    // substitutes the override exactly like a default. A value outside
    // the type already refused above at the declaration; the project on
    // disk is never written.
    for decl in variables.iter_mut() {
        let value = ctx.overrides.get(&decl.id).cloned();
        if let Some(value) = value {
            if let Some(coerced) = coerce_override(ctx, decl, &value) {
                decl.default = Some(coerced);
            }
        }
    }
    // The composition's theme selection resolves before any binding
    // (Sprint 9 variables, CAP-005): the caller theme beats the
    // composition `theme` attribute, and either side naming an unknown
    // theme (or the attribute naming two) refuses naming the rule.
    let theme_tokens = composition_theme_tokens(ctx, root, themes, cli_theme);
    // Caller-supplied ids resolve first everywhere (Sprint 9 variables,
    // CAP-005): the override pass above swapped them into the
    // in-memory defaults, and this set tells the binding pass to keep
    // them ahead of theme tokens.
    let overridden: BTreeSet<String> = ctx.overrides.keys().cloned().collect();
    // The composition name answers `start-after` references, so a
    // refused `start-after` binding falls back to it as a benign
    // placeholder. A name carrying its own `var()` stays raw (it
    // refuses separately below), never a placeholder.
    let comp_name = match root.props.iter().find(|(k, _)| k == "name") {
        Some((_, v)) => match &v.kind {
            ValueKind::Str(s) => s.clone(),
            _ => String::new(),
        },
        None => String::new(),
    };
    substitute_var_bindings(
        ctx,
        root,
        true,
        &variables,
        &theme_tokens,
        &overridden,
        &comp_name,
    );
    variables
}

/// Name the variable behind every coerced binding (Sprint 9 variables,
/// CAP-002): any diagnostic sitting exactly on a substituted `var()`
/// value gains the attribute and variable names, so a substitution
/// that does not coerce to the expected shape fails naming both.
fn amend_binding_diagnostics(ctx: &mut Ctx) {
    for site in &ctx.bindings {
        for diag in ctx.diags.iter_mut() {
            if diag.file != ctx.file || diag.line != site.line || diag.column != site.column {
                continue;
            }
            // Node-location file sites only amend diagnostics quoting
            // the resolved text, so an unrelated error sharing the
            // node never borrows the variable's name.
            if let Some(text) = &site.needs_text {
                if !diag.message.contains(text) {
                    continue;
                }
            }
            for id in &site.vars {
                if !diag.message.contains(&format!("--{id}")) {
                    diag.message.push_str(&format!(
                        " (binding: `{}` `{}` uses variable `--{id}`)",
                        site.node, site.key
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal single-frame GIF: header, GCT, one image, trailer.
    fn still_gif() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"GIF89a");
        b.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]); // 1x1, GCT flag, 2 colors
        b.extend_from_slice(&[0, 0, 0, 255, 255, 255]); // GCT
        b.extend_from_slice(&[0x21, 0xF9, 0x04, 0, 0, 0, 0, 0]); // GCE, no delay
        b.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0]); // image
        b.extend_from_slice(&[2, 2, 0x44, 1, 0]); // LZW + data + terminator
        b.extend_from_slice(&[0x3B]);
        b
    }

    /// Two-frame GIF with a NETSCAPE loop count appended: 0 means
    /// forever, nonzero means a finite repeat.
    fn two_frame_gif(loops: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"GIF89a");
        b.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
        b.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        b.extend_from_slice(b"\x21\xFF\x0BNETSCAPE2.0");
        b.extend_from_slice(&[3, 1, (loops & 0xFF) as u8, (loops >> 8) as u8, 0]);
        for _ in 0..2 {
            b.extend_from_slice(&[0x21, 0xF9, 0x04, 10, 0, 0, 0, 0]); // GCE, 10cs
            b.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
            b.extend_from_slice(&[2, 2, 0x44, 1, 0]);
        }
        b.extend_from_slice(&[0x3B]);
        b
    }

    #[test]
    fn gif_still_is_finite_and_complete() {
        let bytes = still_gif();
        let info = gif_info(&bytes, bytes.len()).expect("still GIF parses");
        assert_eq!(info.frames, 1);
        assert!(!info.infinite);
        assert!(info.complete);
    }

    #[test]
    fn gif_two_frames_are_animated() {
        let bytes = two_frame_gif(1);
        let info = gif_info(&bytes, bytes.len()).expect("animated GIF parses");
        assert_eq!(info.frames, 2);
        assert!(!info.infinite, "a nonzero loop count is finite");
        assert!(info.complete);
    }

    #[test]
    fn gif_netscape_zero_loop_is_infinite() {
        let bytes = two_frame_gif(0);
        let info = gif_info(&bytes, bytes.len()).expect("looping GIF parses");
        assert_eq!(info.frames, 2);
        assert!(info.infinite);
    }

    #[test]
    fn gif_no_netscape_is_finite() {
        let mut bytes = still_gif();
        // A second frame without any loop extension still plays once.
        let frame: Vec<u8> = vec![
            0x21, 0xF9, 0x04, 10, 0, 0, 0, 0, 0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0,
        ];
        bytes.splice(bytes.len() - 1..bytes.len() - 1, frame);
        let info = gif_info(&bytes, bytes.len()).expect("plays-once GIF parses");
        assert_eq!(info.frames, 2);
        assert!(!info.infinite);
        assert!(info.complete);
    }

    #[test]
    fn gif_non_gif_bytes_are_none() {
        assert!(gif_info(b"fake", 1024).is_none());
        assert!(gif_info(b"\x89PNG\r\n\x1a\n", 1024).is_none());
        assert!(
            gif_info(b"GIF89a", 1024).is_some(),
            "a bare header still sniffs as GIF"
        );
    }

    #[test]
    fn gif_compressed_payload_bytes_cannot_fake_a_frame() {
        // A 0x2C hiding inside image data is not a frame: the walker
        // only counts descriptors at block boundaries, skipping data
        // by its sub-block sizes.
        let mut bytes = still_gif();
        let at = bytes.iter().position(|&b| b == 0x44).expect("data byte");
        bytes[at] = 0x2C;
        let info = gif_info(&bytes, bytes.len()).expect("parses");
        assert_eq!(info.frames, 1, "data bytes are skipped by size");
        assert!(info.complete);
    }

    #[test]
    fn heic_ftyp_brands_match() {
        let mut heic = vec![0u8; 4];
        heic.extend_from_slice(b"ftypheic");
        heic.extend_from_slice(&[0, 0, 0, 0]);
        heic.extend_from_slice(b"mif1");
        assert!(heic_magic_ok(&heic));
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&[0u8; 8]);
        assert!(!heic_magic_ok(&png));
        assert!(!heic_magic_ok(b"fake"));
    }
}

#[cfg(test)]
mod sound_fx_tests {
    use super::*;

    /// Stereo s16 fixture: `loud` frames, then `quiet` frames, then
    /// `loud` frames again, so pause edges are hand-known.
    fn gap_pcm(loud: usize, quiet: usize) -> Vec<i16> {
        let mut pcm = Vec::with_capacity((2 * loud + quiet) * 2);
        for _ in 0..loud {
            pcm.extend_from_slice(&[10000, -10000]);
        }
        for _ in 0..quiet {
            pcm.extend_from_slice(&[0, 0]);
        }
        for _ in 0..loud {
            pcm.extend_from_slice(&[10000, -10000]);
        }
        pcm
    }

    #[test]
    fn pauses_report_exact_sample_edges() {
        // T006: a 1 s exact-digital gap between two 0.5 s tones at
        // −40 dBFS reports exactly 0.5 s..1.5 s.
        let pcm = gap_pcm(24_000, 48_000);
        let ranges = scan_silence(&pcm, 0.0, -40.0, 0.5);
        assert_eq!(ranges, vec![(0.5, 1.5)]);
        // A minimum longer than the gap reports nothing.
        assert!(scan_silence(&pcm, 0.0, -40.0, 1.5).is_empty());
        // A hotter ceiling still catches exact zero.
        let ranges = scan_silence(&pcm, 2.0, -60.0, 0.5);
        assert_eq!(ranges, vec![(2.5, 3.5)]);
    }

    #[test]
    fn ceiling_and_floor_behave() {
        // −40 dBFS is 1% of full scale: ±327 counts as silent, ±328
        // does not.
        let mut pcm = vec![0i16; 48_000 * 2];
        for (i, v) in [327, 328].iter().enumerate() {
            pcm[2 * (1000 + i * 100)] = *v;
            pcm[2 * (1000 + i * 100) + 1] = -*v;
        }
        // The ±328 blip splits the second into two pauses (±327 stays
        // silent under the ceiling) when the minimum fits.
        let halves = scan_silence(&pcm, 0.0, -40.0, 0.01);
        assert_eq!(halves.len(), 2);
        // `-inf` counts only exact digital zero.
        let mut near = vec![1i16; 48_000 * 2];
        near[0] = 0;
        near[1] = 0;
        assert!(scan_silence(&near, 0.0, f64::NEG_INFINITY, 0.5).is_empty());
        let mut exact = vec![0i16; 48_000 * 2];
        exact[0] = 1;
        let ranges = scan_silence(&exact, 0.0, f64::NEG_INFINITY, 0.5);
        assert_eq!(ranges.len(), 1);
        assert!((ranges[0].0 - 1.0 / 48_000.0).abs() < 1e-12);
    }

    #[test]
    fn detect_helpers_mirror_the_mix() {
        // Unity rate returns the input unchanged; double rate halves
        // the frame count; tiling repeats the unit exactly.
        let pcm: Vec<i16> = (0..20).collect();
        assert_eq!(resample_detect(&pcm, 1.0), pcm);
        let half = resample_detect(&pcm, 2.0);
        assert_eq!(half.len(), 5 * 2);
        assert_eq!(half[0], 0);
        let tiled = tile_detect(&pcm, 15);
        assert_eq!(tiled.len(), 15 * 2);
        assert_eq!(&tiled[..20], &pcm[..]);
        assert_eq!(&tiled[20..30], &pcm[..10]);
    }
}
