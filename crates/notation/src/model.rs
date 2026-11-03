//! Shared composition model for the Framesmith Sprint 1 foundation CLI.
//!
//! Covers the data needed by T003 (folder + composition root), T004
//! (supported vs later-sprint elements) and T005 (absolute half-open
//! clip timing). Drawing, decoding and encoding arrive in later sprints.

use std::fmt;

/// Frames per second: an integer or an exact rational (e.g. `30000/1001`).
/// Decimal fps values are rejected by validation, never stored here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fps {
    pub num: u64,
    pub den: u64,
}

impl Fps {
    pub fn from_int(n: u64) -> Self {
        Self { num: n, den: 1 }
    }

    /// Exact rational value as f64 (used for render timing math only).
    pub fn value(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Total frame count for a duration in seconds, rounded up so the
    /// last partial frame still covers the tail of the composition.
    pub fn frame_count(&self, duration_secs: f64) -> u64 {
        (duration_secs * self.value()).ceil() as u64
    }
}

impl fmt::Display for Fps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

/// 8-bit RGB color parsed from `#rgb` / `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Lowercase `rrggbb` hex without a leading `#`, for FFmpeg filters.
    pub fn to_hex(&self) -> String {
        format!("{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Parse `#rgb` or `#rrggbb` (case-insensitive). Anything else is rejected.
pub fn parse_color(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#')?;
    let expand = |c: u8| -> Option<u8> {
        let d = (c as char).to_digit(16)? as u8;
        Some(d * 16 + d)
    };
    let digit = |c: u8| -> Option<u8> { Some((c as char).to_digit(16)? as u8) };
    match h.len() {
        3 => {
            let b = h.as_bytes();
            Some(Rgb {
                r: expand(b[0])?,
                g: expand(b[1])?,
                b: expand(b[2])?,
            })
        }
        6 => {
            let b = h.as_bytes();
            Some(Rgb {
                r: digit(b[0])? * 16 + digit(b[1])?,
                g: digit(b[2])? * 16 + digit(b[3])?,
                b: digit(b[4])? * 16 + digit(b[5])?,
            })
        }
        _ => None,
    }
}

/// Sprint 1 video placement modes. Unknown spellings are invalid (T003/T012
/// boundary: never stretch silently on a misspelled fit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Contain,
    Cover,
    Fill,
}

impl Fit {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "contain" => Some(Fit::Contain),
            "cover" => Some(Fit::Cover),
            "fill" => Some(Fit::Fill),
            _ => None,
        }
    }
}

/// Track kind of a clip (Sprint 6 `track` key): the display vocabulary
/// timelines use. `caption` only ever comes from a written `track`;
/// undeclared tracks default by element kind (video to video, audio
/// to audio, everything else to graphics). Unknown spellings are
/// refused, never guessed into a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Caption,
    Graphics,
}

impl TrackKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "video" => Some(TrackKind::Video),
            "audio" => Some(TrackKind::Audio),
            "caption" => Some(TrackKind::Caption),
            "graphics" => Some(TrackKind::Graphics),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TrackKind::Video => "video",
            TrackKind::Audio => "audio",
            TrackKind::Caption => "caption",
            TrackKind::Graphics => "graphics",
        }
    }

    /// Default track for an undeclared `track` key, by element name:
    /// `video` clips ride the video lane, `audio` clips the audio lane,
    /// every other element (text, box, image, nest) the graphics lane.
    pub fn default_for_clip(element: &str) -> Self {
        match element {
            "video" => TrackKind::Video,
            "audio" => TrackKind::Audio,
            _ => TrackKind::Graphics,
        }
    }

    /// Accepted spellings for diagnostics, in spec order.
    pub const ACCEPTED: [&'static str; 4] = ["video", "audio", "caption", "graphics"];
}

/// Horizontal alignment of wrapped text lines (Sprint 1 `align` key).
/// Only these three spellings are valid; anything else is a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Align::Left),
            "center" => Some(Align::Center),
            "right" => Some(Align::Right),
            _ => None,
        }
    }
}

/// Font style (Sprint 1 `font-style` key): regular, bold, italic, or
/// bold-italic. System faces resolve to the matching variant file;
/// project and bitmap faces synthesize the style (faux bold/italic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "regular" => Some(FontStyle::Regular),
            "bold" => Some(FontStyle::Bold),
            "italic" => Some(FontStyle::Italic),
            "bold-italic" => Some(FontStyle::BoldItalic),
            _ => None,
        }
    }
}

/// Word-wrap mode (Sprint 1 `wrap` key): off, or a positive wrap width
/// in canvas pixels. Lines break at spaces; words longer than the
/// width overflow rather than splitting mid-word.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wrap {
    Off,
    Width(f64),
}

/// A relative start (Sprint 6 `start-after` plus optional
/// `start-offset`): the clip begins exactly at the referenced clip's
/// effective end plus the offset (negative overlaps earlier content).
/// Resolution (T002) turns this into an absolute `start`; validation
/// refuses self-reference, cycles, unknown ids, before-zero results,
/// and absolute-plus-relative ambiguity.
#[derive(Debug, Clone)]
pub struct StartAfter {
    /// Referenced clip or composition id, as written.
    pub target: String,
    /// Seconds after the referenced end (may be negative).
    pub offset: f64,
    /// Location of the `start-after` value, for diagnostics.
    pub line: usize,
    pub column: usize,
}

/// How a `text` clip splits into individually addressable pieces
/// (Sprint 7 motion, CAP-009): `chars` splits per Unicode scalar value,
/// `words` splits on whitespace runs (joined with single spaces),
/// `lines` splits on newlines. Only `text` clips carry a split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitMode {
    Chars,
    Words,
    Lines,
}

impl SplitMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "chars" => Some(SplitMode::Chars),
            "words" => Some(SplitMode::Words),
            "lines" => Some(SplitMode::Lines),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SplitMode::Chars => "chars",
            SplitMode::Words => "words",
            SplitMode::Lines => "lines",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["chars", "words", "lines"];
}

/// A named composition instant (Sprint 7 motion, CAP-003): a `mark`
/// element names one time in seconds that `start-after` accepts as a
/// target, resolving to that instant plus any `start-offset`.
#[derive(Debug, Clone)]
pub struct MarkDecl {
    /// Mark name, unique across the project like an `id`.
    pub name: String,
    /// Named instant in composition seconds.
    pub at: f64,
    pub line: usize,
    pub column: usize,
}

/// The 3D turn of a visual clip (Sprint 7 motion, CAP-005):
/// `rotation-x`/`rotation-y` in degrees about the horizontal and
/// vertical axes through the `origin` point, foreshortened by
/// `perspective` (finite positive pixels; `None` renders flat with
/// orthographic foreshortening). The identity (both angles zero) draws
/// exactly as before, with or without a perspective.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn3D {
    pub rotation_x: f64,
    pub rotation_y: f64,
    pub perspective: Option<f64>,
    pub origin: Placement,
}

impl Turn3D {
    /// No 3D turn: zero angles, flat projection, centered origin.
    pub fn flat() -> Self {
        Self {
            rotation_x: 0.0,
            rotation_y: 0.0,
            perspective: None,
            origin: Placement::center(),
        }
    }

    /// True when both angles are zero (or non-finite, drawn flat like
    /// a zero 2D rotation): the element draws through the old fast
    /// path byte-identically, whatever the perspective or origin.
    pub fn is_identity(&self) -> bool {
        (self.rotation_x == 0.0 || !self.rotation_x.is_finite())
            && (self.rotation_y == 0.0 || !self.rotation_y.is_finite())
    }

    /// Origin as fractions of the element rect: left/center/right to
    /// 0/0.5/1 horizontally, top/center/bottom to 0/0.5/1 vertically.
    pub fn origin_fractions(&self) -> (f64, f64) {
        let fx = match self.origin.x {
            PlaceH::Left => 0.0,
            PlaceH::Center => 0.5,
            PlaceH::Right => 1.0,
        };
        let fy = match self.origin.y {
            PlaceV::Top => 0.0,
            PlaceV::Center => 0.5,
            PlaceV::Bottom => 1.0,
        };
        (fx, fy)
    }
}

/// A `text` clip (Sprint 1 usable element, plus Sprint 2 paint order,
/// line height and letter spacing, plus Sprint 6 id/track/label, plus
/// Sprint 7 split/typewriter/count text effects).
#[derive(Debug, Clone)]
pub struct TextClip {
    pub value: String,
    pub start: f64,
    pub duration: f64,
    /// Stable reference name (`id`), unique across the project when written.
    pub id: Option<String>,
    /// Display lane (`track`); defaults to graphics when undeclared.
    pub track: TrackKind,
    /// Editor display note (`label`); rides along with no render effect.
    pub label: Option<String>,
    /// Relative start, resolved to `start` by validation (T002); none
    /// means the absolute `start` stands as written.
    pub start_after: Option<StartAfter>,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub color: Rgb,
    pub align: Align,
    pub wrap: Wrap,
    pub style: FontStyle,
    /// Explicit paint order (default 0); ties fall back to document order.
    pub order: i64,
    /// Line separation in canvas pixels when declared; none means the
    /// font default. Only non-negative values are valid.
    pub line_height: Option<f64>,
    /// Extra advance per glyph in canvas pixels when declared; none or
    /// zero means default advance. Only non-negative values are valid.
    pub letter_spacing: Option<f64>,
    /// Explicit `font-family` list in try order (CSS-like, comma
    /// separated); none means the project font, else the machine style
    /// fallback. At least one name must resolve at render time.
    pub font_family: Option<Vec<String>>,
    /// Split mode (Sprint 7 motion, CAP-009): none means the whole
    /// value shows; a split addresses each piece for motion while the
    /// full value still shows unless `typewriter` reveals it.
    pub split: Option<SplitMode>,
    /// Typewriter reveal (Sprint 7 motion, CAP-009): `true` shows
    /// split pieces progressively across the clip window (needs
    /// `split`); `false` (default) shows the whole value.
    pub typewriter: bool,
    /// Count range (Sprint 7 motion, CAP-009): `count-from` and
    /// `count-to` render the interpolated number with `decimals`
    /// decimal places (default 0). Counting contradicts `split` and
    /// `typewriter`; `decimals` needs a count range.
    pub count_from: Option<f64>,
    pub count_to: Option<f64>,
    pub decimals: u32,
    /// 3D turn of the clip (Sprint 7 motion, CAP-005): rotation about
    /// the horizontal/vertical axes through `origin`, foreshortened by
    /// `perspective` (flat when omitted). Identity by default.
    pub turn: Turn3D,
    /// Static Gaussian blur sigma in canvas pixels (Sprint 7 motion,
    /// CAP-006): 0 draws sharp. Only `blur(N)`/`none` exist; any other
    /// `filter` is a Sprint 11 refusal at validation.
    pub filter_blur: f64,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// A basic `video` clip (Sprint 1 usable element, plus Sprint 2 order,
/// plus Sprint 3 sound keys, plus Sprint 5 footage trim, rate, loop,
/// and GIF/HEIC sources): the clip consumes the source window between
/// `trim_start` and `trim_end` (seconds into the source; none means the
/// true source end) at `rate` source seconds per composition second,
/// for both picture and sound. Without declared keys the whole source
/// plays at unity rate. With `loop_clip` the window repeats for the
/// whole clip window; without it a window that outruns the source holds
/// the last decoded frame as a still while its sound goes silent for
/// the remainder. An animated GIF plays with its native frame timings;
/// a still GIF or HEIC still holds one frame for the clip window. The
/// clip's sound enters the 48 kHz stereo mix with the declared
/// `volume_db` (dB, `-inf` is exact silence) and linear
/// `fade_in`/`fade_out` (seconds) applied, sample-aligned with
/// the picture. Without declared keys the sound mixes at unity gain.
#[derive(Debug, Clone)]
pub struct VideoClip {
    pub path: String,
    pub start: f64,
    /// Stable reference name (`id`), unique across the project when written.
    pub id: Option<String>,
    /// Display lane (`track`); defaults to video when undeclared.
    pub track: TrackKind,
    /// Editor display note (`label`); rides along with no render effect.
    pub label: Option<String>,
    /// Clip length in composition seconds: the written `duration`, or
    /// the inferred `(trim_end − trim_start) ÷ rate` when `duration`
    /// is omitted (Sprint 5 T009). `duration_inferred` marks the
    /// inferred case so `check` can show it for review; rendering is
    /// identical for written and inferred lengths.
    pub duration: f64,
    /// True when `duration` above was inferred from the footage window
    /// (no `duration` key written); false for an explicit `duration`.
    pub duration_inferred: bool,
    /// Relative start, resolved to `start` by validation (T002); none
    /// means the absolute `start` stands as written.
    pub start_after: Option<StartAfter>,
    pub fit: Fit,
    pub order: i64,
    /// Sound gain in dB (`f64::NEG_INFINITY` is exact silence); default 0.
    pub volume_db: f64,
    /// Linear fade in, in seconds; default 0.
    pub fade_in: f64,
    /// Linear fade out, in seconds; default 0.
    pub fade_out: f64,
    /// Source seconds where the window opens (`trim-start`); default 0.
    pub trim_start: f64,
    /// Source seconds where the window closes (`trim-end`); none means
    /// the true source end as reported by the probe.
    pub trim_end: Option<f64>,
    /// Source seconds consumed per composition second (`rate`, 0.1–10);
    /// default 1.
    pub rate: f64,
    /// Repeat the source window for the whole clip window (`loop`,
    /// default false). A looping clip without an explicit `duration`
    /// is refused as unbounded.
    pub loop_clip: bool,
    /// 3D turn of the clip (Sprint 7 motion, CAP-005). Identity default.
    pub turn: Turn3D,
    /// Static Gaussian blur sigma in canvas pixels (Sprint 7 motion,
    /// CAP-006): 0 draws sharp.
    pub filter_blur: f64,
    /// Overlap blend with the next same-track clip (Sprint 10 sound,
    /// CAP-003); default `constant-power`.
    pub crossfade: Crossfade,
    /// Voice ducking (Sprint 10 sound, CAP-004); none means full gain
    /// throughout.
    pub duck_under: Option<DuckUnder>,
    /// Echo, room, and pause detection (Sprint 10 sound, CAP-005/006);
    /// the default renders dry and detects nothing.
    pub fx: SoundFx,
    /// On-canvas sound drawing (Sprint 10 sound, CAP-010); disabled
    /// means the clip draws no sound shape.
    pub waveform: WaveformDraw,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// Overlap blend of an audio-bearing clip (Sprint 10 sound, CAP-003):
/// `constant-power` keeps the summed power flat across the handoff
/// (equal-power cosine/sine pair), `linear` runs the straight 1−p/p
/// pair, and `none` hard-cuts (the outgoing clip stops where the
/// incoming starts). Written `crossfade`; default `constant-power`.
/// Unknown spellings are refused, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Crossfade {
    #[default]
    ConstantPower,
    Linear,
    None,
}

/// One-pass loudness target of a composition (Sprint 10 sound,
/// CAP-009): `loud` lands integrated loudness at −16 LUFS with true
/// peak guarding, applied as one overall gain. Absent means the mix
/// renders at its authored levels. Unknown spellings are refused,
/// never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Normalize {
    #[default]
    None,
    Loud,
}

impl Normalize {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "loud" => Some(Normalize::Loud),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Normalize::None => "none",
            Normalize::Loud => "loud",
        }
    }

    /// Whether one overall loudness gain applies to the mix.
    pub fn active(self) -> bool {
        matches!(self, Normalize::Loud)
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 1] = ["loud"];
}

impl Crossfade {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "constant-power" => Some(Crossfade::ConstantPower),
            "linear" => Some(Crossfade::Linear),
            "none" => Some(Crossfade::None),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Crossfade::ConstantPower => "constant-power",
            Crossfade::Linear => "linear",
            Crossfade::None => "none",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["constant-power", "linear", "none"];
}

/// A `duck-under` plus `duck-amount` declaration (Sprint 10 sound,
/// CAP-004): while the named voice clip's timeline window sounds, this
/// clip scales to `amount` (0..1); outside the window it plays full.
/// Voice activity is read from timeline windows only, never from
/// signal detection, so ducking is deterministic by construction.
#[derive(Debug, Clone)]
pub struct DuckUnder {
    /// Voice clip id, as written.
    pub target: String,
    /// Gain while the voice sounds (0..1, inclusive).
    pub amount: f64,
    /// Location of the `duck-under` value, for diagnostics.
    pub line: usize,
    pub column: usize,
}

/// Echo and room plus pause detection of one audio-bearing clip
/// (Sprint 10 sound, CAP-005/CAP-006): `delay` seconds with
/// `delay-feedback` and `delay-mix` render a feedback echo inside the
/// mixer, `reverb` a deterministic room tail, and `detect-silence`
/// with its thresholds reports pauses in `check` without altering
/// audio. Defaults render dry and detect nothing.
#[derive(Debug, Clone)]
pub struct SoundFx {
    /// Echo spacing in composition seconds (0..2); 0 bypasses delay.
    pub delay: f64,
    /// Echo recirculation per tap (0..0.95); 0 keeps one tap.
    pub delay_feedback: f64,
    /// Wet level of every echo tap (0..1); 0 bypasses delay.
    pub delay_mix: f64,
    /// Room amount (0..1); 0 bypasses reverb.
    pub reverb: f64,
    /// Whether `check` lists this clip's silent ranges.
    pub detect_silence: bool,
    /// Silence ceiling in dBFS (at or below counts as silent).
    pub silence_threshold_db: f64,
    /// Shortest reported pause in composition seconds.
    pub silence_min: f64,
}

/// Default silence ceiling in dBFS (Sprint 10 sound, CAP-006).
pub const SILENCE_THRESHOLD_DEFAULT_DB: f64 = -40.0;

/// Default shortest reported pause in seconds (Sprint 10 sound,
/// CAP-006).
pub const SILENCE_MIN_DEFAULT_SECS: f64 = 0.5;

/// Longest echo spacing in seconds (Sprint 10 sound, CAP-005).
pub const DELAY_MAX_SECS: f64 = 2.0;

/// Highest echo recirculation (Sprint 10 sound, CAP-005): below unity
/// so every tail decays to silence instead of ringing forever.
pub const DELAY_FEEDBACK_MAX: f64 = 0.95;

/// Tap audibility floor (Sprint 10 sound, CAP-005/CAP-006): taps below
/// −60 dBFS end the natural tail; the mixer renders to the
/// composition end and `check` notes the truncation.
pub const EFFECT_TAIL_FLOOR: f64 = 0.001;

/// Room comb spacing in samples at 48 kHz (Sprint 10 sound, CAP-005):
/// a fixed 30 ms so the room tail is deterministic by construction.
pub const REVERB_COMB_SAMPLES: usize = 1440;

/// Mix sample rate for effect-tail sample math (Sprint 10 sound).
pub const EFFECT_RATE: f64 = 48_000.0;

/// Largest tap count one effect ever plans (Sprint 10 sound, CAP-005):
/// validation bounds keep real tails far below this; direct API misuse
/// with unity-or-higher feedback still terminates.
pub const EFFECT_MAX_TAPS: usize = 10_000;

impl Default for SoundFx {
    fn default() -> Self {
        Self {
            delay: 0.0,
            delay_feedback: 0.0,
            delay_mix: 0.0,
            reverb: 0.0,
            detect_silence: false,
            silence_threshold_db: SILENCE_THRESHOLD_DEFAULT_DB,
            silence_min: SILENCE_MIN_DEFAULT_SECS,
        }
    }
}

/// Audible echo taps of one delay setting (Sprint 10 sound, CAP-005):
/// tap `k >= 1` lands at `k * delay` with gain
/// `delay-mix * delay-feedback^(k-1)`, while taps at or above the
/// [`EFFECT_TAIL_FLOOR`] count. Zero delay or zero mix plans no taps.
pub fn delay_taps(delay_secs: f64, feedback: f64, mix: f64) -> usize {
    if !delay_secs.is_finite() || delay_secs <= 0.0 || !mix.is_finite() || mix <= 0.0 {
        return 0;
    }
    let spaced = (delay_secs * EFFECT_RATE).round();
    if spaced < 1.0 {
        return 0;
    }
    if mix < EFFECT_TAIL_FLOOR {
        return 0;
    }
    if !feedback.is_finite() || feedback <= 0.0 {
        return 1;
    }
    if feedback >= 1.0 {
        return EFFECT_MAX_TAPS;
    }
    let taps = 1.0 + (EFFECT_TAIL_FLOOR / mix).ln() / feedback.ln();
    (taps.floor() as usize).clamp(1, EFFECT_MAX_TAPS)
}

/// Natural echo tail in samples (Sprint 10 sound, CAP-005): audible
/// taps times the tap spacing. The mixer renders at most the room to
/// the composition end and `check` notes the cut.
pub fn delay_tail_frames(delay_secs: f64, feedback: f64, mix: f64) -> usize {
    if !delay_secs.is_finite() || delay_secs <= 0.0 {
        return 0;
    }
    let spaced = (delay_secs * EFFECT_RATE).round();
    if spaced < 1.0 {
        return 0;
    }
    delay_taps(delay_secs, feedback, mix).saturating_mul(spaced as usize)
}

/// Audible room taps of one reverb amount (Sprint 10 sound, CAP-005):
/// tap `k >= 1` lands at `k * 30 ms` with gain
/// `reverb * (reverb / 2)^(k-1)`. Zero reverb plans no taps.
pub fn reverb_taps(reverb: f64) -> usize {
    if !reverb.is_finite() || reverb <= 0.0 {
        return 0;
    }
    if reverb < EFFECT_TAIL_FLOOR {
        return 0;
    }
    let feedback = reverb / 2.0;
    if feedback <= 0.0 {
        return 1;
    }
    let taps = 1.0 + (EFFECT_TAIL_FLOOR / reverb).ln() / feedback.ln();
    (taps.floor() as usize).clamp(1, EFFECT_MAX_TAPS)
}

/// Natural room tail in samples (Sprint 10 sound, CAP-005): audible
/// taps times the fixed 30 ms comb spacing.
pub fn reverb_tail_frames(reverb: f64) -> usize {
    reverb_taps(reverb).saturating_mul(REVERB_COMB_SAMPLES)
}

/// Natural effect tail in samples with delay chained into reverb
/// (Sprint 10 sound, CAP-005): the echo tail first, then the room tail
/// blooming off it. Zero means the clip renders dry.
pub fn effect_tail_frames(delay_secs: f64, feedback: f64, mix: f64, reverb: f64) -> usize {
    delay_tail_frames(delay_secs, feedback, mix).saturating_add(reverb_tail_frames(reverb))
}

impl SoundFx {
    /// True when delay and reverb both bypass, so the clip mixes down
    /// the historical dry path bit-identically.
    pub fn dry(&self) -> bool {
        effect_tail_frames(self.delay, self.delay_feedback, self.delay_mix, self.reverb) == 0
    }
}

/// On-canvas sound drawing of one audio-bearing clip (Sprint 10
/// sound, CAP-010): `waveform="true"` draws the clip's sound shape
/// within `waveform-from`/`waveform-to` (composition seconds,
/// defaulting to the clip window). An empty or backward drawn range
/// is refused; a range past the clip window clamps to it and reports.
#[derive(Debug, Clone)]
pub struct WaveformDraw {
    /// Whether the clip's sound shape draws on the canvas.
    pub enabled: bool,
    /// Drawn range start in composition seconds; none means the clip
    /// window start.
    pub from: Option<f64>,
    /// Drawn range end in composition seconds; none means the clip
    /// window end.
    pub to: Option<f64>,
    /// Location of the `waveform` key (or the clip node when absent),
    /// for diagnostics.
    pub line: usize,
    pub column: usize,
    /// Location of the `waveform-from` key, when written.
    pub from_loc: Option<(usize, usize)>,
    /// Location of the `waveform-to` key, when written.
    pub to_loc: Option<(usize, usize)>,
}

impl Default for WaveformDraw {
    fn default() -> Self {
        Self {
            enabled: false,
            from: None,
            to: None,
            line: 1,
            column: 1,
            from_loc: None,
            to_loc: None,
        }
    }
}

/// A resolved on-canvas sound range (Sprint 10 sound, CAP-010): the
/// written range plus its clamp to the audible clip window. The
/// drawing spans `[eff_from, eff_to)` mapped across the canvas width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformRange {
    /// Written range start in composition seconds.
    pub raw_from: f64,
    /// Written range end in composition seconds.
    pub raw_to: f64,
    /// Clamped range start in composition seconds.
    pub eff_from: f64,
    /// Clamped range end in composition seconds.
    pub eff_to: f64,
    /// True when clamping cut the written range.
    pub clamped: bool,
}

/// Why one drawn range cannot render (Sprint 10 sound, CAP-010,
/// REC-002): the clip has no audible window, the written range is
/// empty or backward, or it lies fully outside the clip window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveformRangeError {
    EmptyWindow,
    EmptyOrBackward,
    OutsideWindow,
}

/// Resolve one drawn range against the audible clip window (Sprint 10
/// sound, CAP-010): the window is the clip span clamped to
/// composition time; absent bounds default to its ends. Empty or
/// backward written ranges fail before clamping; a range past the
/// window clamps to it and reports; a range fully outside it fails
/// with nothing to draw.
pub fn resolve_waveform_range(
    clip_start: f64,
    clip_duration: f64,
    comp_duration: f64,
    from: Option<f64>,
    to: Option<f64>,
) -> Result<WaveformRange, WaveformRangeError> {
    if !clip_start.is_finite()
        || !clip_duration.is_finite()
        || clip_duration <= 0.0
        || !comp_duration.is_finite()
        || comp_duration <= 0.0
    {
        return Err(WaveformRangeError::EmptyWindow);
    }
    let wstart = clip_start.max(0.0);
    let wend = (clip_start + clip_duration).min(comp_duration);
    if !wstart.is_finite() || !wend.is_finite() || wend <= wstart || wstart >= comp_duration {
        return Err(WaveformRangeError::EmptyWindow);
    }
    let raw_from = from.unwrap_or(wstart);
    let raw_to = to.unwrap_or(wend);
    if !raw_from.is_finite() || !raw_to.is_finite() || raw_to <= raw_from {
        return Err(WaveformRangeError::EmptyOrBackward);
    }
    let eff_from = raw_from.clamp(wstart, wend);
    let eff_to = raw_to.clamp(wstart, wend);
    if eff_to <= eff_from {
        return Err(WaveformRangeError::OutsideWindow);
    }
    Ok(WaveformRange {
        raw_from,
        raw_to,
        eff_from,
        eff_to,
        clamped: eff_from != raw_from || eff_to != raw_to,
    })
}

/// Smallest resolvable clip or composition length in seconds (Sprint 6
/// T003): any written or inferred length below one frame at 60 fps is
/// refused naming the value and the bound, never rendered as a sliver.
pub const MIN_DURATION_SECS: f64 = 1.0 / 60.0;

/// Deepest allowed `nest` chain in edges (Sprint 6 T007, REC-004): a
/// use chain longer than 8 edges refuses naming the chain and the
/// limit, before the first frame.
pub const MAX_NEST_DEPTH: usize = 8;

/// An `audio` track (Sprint 3 element, plus Sprint 5 footage trim and
/// Sprint 6 rate/inference): a sound source placed in a half-open
/// window with trim, rate, volume, and linear fades. `trim_start` and
/// `trim_end` select the source window in seconds (`trim_end` none
/// means the source end); `rate` consumes that many source seconds per
/// composition second (0.1–10, default 1, pitch follows speed like
/// video); `volume_db` is the gain in dB (`-inf` is exact silence,
/// values above +12 are refused); `fade_in`/`fade_out` are linear
/// amplitude ramps in seconds over the clip window. An omitted
/// `duration` resolves to `(trim_end − trim_start) ÷ rate` using the
/// probed source end, exactly like video inference.
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub path: String,
    pub start: f64,
    pub duration: f64,
    /// Stable reference name (`id`), unique across the project when written.
    pub id: Option<String>,
    /// Display lane (`track`); defaults to audio when undeclared.
    pub track: TrackKind,
    /// Editor display note (`label`); rides along with no render effect.
    pub label: Option<String>,
    /// Relative start, resolved to `start` by validation (T002); none
    /// means the absolute `start` stands as written.
    pub start_after: Option<StartAfter>,
    pub trim_start: f64,
    pub trim_end: Option<f64>,
    /// Source seconds consumed per composition second (`rate`, 0.1–10);
    /// default 1. Pitch follows speed, exactly like video.
    pub rate: f64,
    pub volume_db: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    /// Overlap blend with the next same-track clip (Sprint 10 sound,
    /// CAP-003); default `constant-power`.
    pub crossfade: Crossfade,
    /// Voice ducking (Sprint 10 sound, CAP-004); none means full gain
    /// throughout.
    pub duck_under: Option<DuckUnder>,
    /// Echo, room, and pause detection (Sprint 10 sound, CAP-005/006);
    /// the default renders dry and detects nothing.
    pub fx: SoundFx,
    /// On-canvas sound drawing (Sprint 10 sound, CAP-010); disabled
    /// means the clip draws no sound shape.
    pub waveform: WaveformDraw,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// Placement of an image (or video) inside its target rect, equivalent
/// to `object-position`: horizontal plus vertical alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub x: PlaceH,
    pub y: PlaceV,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceH {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceV {
    Top,
    Center,
    Bottom,
}

impl Placement {
    pub fn center() -> Self {
        Self {
            x: PlaceH::Center,
            y: PlaceV::Center,
        }
    }

    /// Parse `"center"`, `"left"`, `"top-right"`, `"left top"` and other
    /// one- or two-token horizontal/vertical placements. Anything else is
    /// rejected so a misspelled placement never places silently.
    pub fn parse(s: &str) -> Option<Self> {
        let norm = s.trim().to_lowercase().replace(['-', '_'], " ");
        let toks: Vec<&str> = norm.split_whitespace().collect();
        let mut h: Option<PlaceH> = None;
        let mut v: Option<PlaceV> = None;
        if toks.is_empty() || toks.len() > 2 {
            return None;
        }
        for tok in toks {
            match tok {
                "left" => {
                    if h.is_some() {
                        return None;
                    }
                    h = Some(PlaceH::Left);
                }
                "right" => {
                    if h.is_some() {
                        return None;
                    }
                    h = Some(PlaceH::Right);
                }
                "top" => {
                    if v.is_some() {
                        return None;
                    }
                    v = Some(PlaceV::Top);
                }
                "bottom" => {
                    if v.is_some() {
                        return None;
                    }
                    v = Some(PlaceV::Bottom);
                }
                "center" | "middle" => {
                    if h.is_none() {
                        h = Some(PlaceH::Center);
                    } else if v.is_none() {
                        v = Some(PlaceV::Center);
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        Some(Self {
            x: h.unwrap_or(PlaceH::Center),
            y: v.unwrap_or(PlaceV::Center),
        })
    }
}

/// A `box` clip (Sprint 2 element): position, size, fill color, corner
/// radius, border, and opacity.
#[derive(Debug, Clone)]
pub struct BoxClip {
    pub start: f64,
    pub duration: f64,
    /// Stable reference name (`id`), unique across the project when written.
    pub id: Option<String>,
    /// Display lane (`track`); defaults to graphics when undeclared.
    pub track: TrackKind,
    /// Editor display note (`label`); rides along with no render effect.
    pub label: Option<String>,
    /// Relative start, resolved to `start` by validation (T002); none
    /// means the absolute `start` stands as written.
    pub start_after: Option<StartAfter>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: Rgb,
    pub radius: f64,
    pub border_width: f64,
    pub border_color: Rgb,
    /// 0 transparent to 1 opaque.
    pub opacity: f64,
    pub order: i64,
    /// 3D turn of the clip (Sprint 7 motion, CAP-005). Identity default.
    pub turn: Turn3D,
    /// Static Gaussian blur sigma in canvas pixels (Sprint 7 motion,
    /// CAP-006): 0 draws sharp.
    pub filter_blur: f64,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// An `image` clip (Sprint 2 element, plus Sprint 5 HEIC and still-GIF
/// sources): a PNG, JPEG, HEIC/HEIF, or still-GIF source placed with
/// a fit and a placement inside its target rect. Pointing `image` at
/// an animated GIF is refused; animations play in a `video` clip.
#[derive(Debug, Clone)]
pub struct ImageClip {
    pub path: String,
    pub start: f64,
    pub duration: f64,
    /// Stable reference name (`id`), unique across the project when written.
    pub id: Option<String>,
    /// Display lane (`track`); defaults to graphics when undeclared.
    pub track: TrackKind,
    /// Editor display note (`label`); rides along with no render effect.
    pub label: Option<String>,
    /// Relative start, resolved to `start` by validation (T002); none
    /// means the absolute `start` stands as written.
    pub start_after: Option<StartAfter>,
    pub x: f64,
    pub y: f64,
    /// Target rect size; defaults to the composition canvas.
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fit: Fit,
    pub placement: Placement,
    pub order: i64,
    /// 3D turn of the clip (Sprint 7 motion, CAP-005). Identity default.
    pub turn: Turn3D,
    /// Static Gaussian blur sigma in canvas pixels (Sprint 7 motion,
    /// CAP-006): 0 draws sharp.
    pub filter_blur: f64,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// A project `font` declaration. The file must exist when declared.
#[derive(Debug, Clone)]
pub struct FontDecl {
    pub path: String,
    pub line: usize,
    pub column: usize,
}

/// The form of a `tween` (T003): `to` moves from the authored value to
/// the given value, `from` moves from the given value to the authored
/// value, and `from-to` moves between the two given values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweenKind {
    To,
    From,
    FromTo,
}

impl TweenKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "to" => Some(TweenKind::To),
            "from" => Some(TweenKind::From),
            "from-to" => Some(TweenKind::FromTo),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TweenKind::To => "to",
            TweenKind::From => "from",
            TweenKind::FromTo => "from-to",
        }
    }
}

/// One easing curve of a `tween` or `key` (T003 plus the Sprint 7 motion
/// set): the nineteen named curves of `CAP-009`, parameterized
/// `out-back(n)`, and the full curve set (`cubic-bezier`, `steps`,
/// `spring`, `bounce`, `elastic`, `wiggle`, `hold`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EaseDecl {
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InExpo,
    OutExpo,
    InOutExpo,
    OutBack(f64),
    CubicBezier(f64, f64, f64, f64),
    Steps(u32),
    Spring(f64, f64),
    Bounce,
    Elastic,
    Wiggle(u32),
    Hold,
}

impl EaseDecl {
    /// The nineteen named curves, in spec order.
    pub const NAMED: [&'static str; 19] = [
        "linear",
        "in-quad",
        "out-quad",
        "in-out-quad",
        "in-cubic",
        "out-cubic",
        "in-out-cubic",
        "in-quart",
        "out-quart",
        "in-out-quart",
        "in-quint",
        "out-quint",
        "in-out-quint",
        "in-sine",
        "out-sine",
        "in-out-sine",
        "in-expo",
        "out-expo",
        "in-out-expo",
    ];

    pub fn name(self) -> String {
        match self {
            EaseDecl::Linear => "linear".into(),
            EaseDecl::InQuad => "in-quad".into(),
            EaseDecl::OutQuad => "out-quad".into(),
            EaseDecl::InOutQuad => "in-out-quad".into(),
            EaseDecl::InCubic => "in-cubic".into(),
            EaseDecl::OutCubic => "out-cubic".into(),
            EaseDecl::InOutCubic => "in-out-cubic".into(),
            EaseDecl::InQuart => "in-quart".into(),
            EaseDecl::OutQuart => "out-quart".into(),
            EaseDecl::InOutQuart => "in-out-quart".into(),
            EaseDecl::InQuint => "in-quint".into(),
            EaseDecl::OutQuint => "out-quint".into(),
            EaseDecl::InOutQuint => "in-out-quint".into(),
            EaseDecl::InSine => "in-sine".into(),
            EaseDecl::OutSine => "out-sine".into(),
            EaseDecl::InOutSine => "in-out-sine".into(),
            EaseDecl::InExpo => "in-expo".into(),
            EaseDecl::OutExpo => "out-expo".into(),
            EaseDecl::InOutExpo => "in-out-expo".into(),
            EaseDecl::OutBack(n) => format!("out-back({n})"),
            EaseDecl::CubicBezier(x1, y1, x2, y2) => {
                format!("cubic-bezier({x1},{y1},{x2},{y2})")
            }
            EaseDecl::Steps(n) => format!("steps({n})"),
            EaseDecl::Spring(s, d) => format!("spring({s},{d})"),
            EaseDecl::Bounce => "bounce".into(),
            EaseDecl::Elastic => "elastic".into(),
            EaseDecl::Wiggle(n) => format!("wiggle({n})"),
            EaseDecl::Hold => "hold".into(),
        }
    }
}

/// Where staggered piece delays are measured from (Sprint 7 motion,
/// CAP-002): the first piece, the middle, or the last piece.
/// Written `stagger-from` (or `stagger_from`); default `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaggerOrigin {
    Start,
    Center,
    End,
}

impl StaggerOrigin {
    pub fn name(self) -> &'static str {
        match self {
            StaggerOrigin::Start => "start",
            StaggerOrigin::Center => "center",
            StaggerOrigin::End => "end",
        }
    }
}

/// A 2D stagger grid (Sprint 7 motion, CAP-002): pieces lay row-major
/// in `cols` columns (rows derived from the piece count, at least the
/// declared `rows` when written `colsxrows`). Written `stagger-grid`
/// (or `stagger_grid`) as `<columns>` or `<columns>x<rows>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaggerGrid {
    pub cols: usize,
    pub rows: Option<usize>,
}

/// Repeat bounds (Sprint 7 motion, CAP-004): `repeat` replays a motion
/// 1 to 1000 times; anything else (non-positive, over-limit, or an
/// unbounded marker) is refused naming the bound.
pub const REPEAT_MIN: u32 = 1;
pub const REPEAT_MAX: u32 = 1000;

/// A validated `tween` directive (T003): smooth motion of one property
/// of one clip over a half-open window. Drawing (T009/T010/T012)
/// evaluates these; validation only guarantees they are well-defined.
#[derive(Debug, Clone)]
pub struct TweenDecl {
    /// Document index of the target clip.
    pub target: usize,
    /// Canonical animatable property name (aliases normalized at
    /// validation): one of `MOTION_PROPERTIES`.
    pub property: String,
    pub kind: TweenKind,
    pub from: Option<f64>,
    pub to: Option<f64>,
    /// Color endpoints for `fill`/`border-color` tweens (Sprint 7
    /// motion, CAP-006): straight RGBA interpolation runs over these,
    /// channel by channel. `None` for every numeric property.
    pub from_color: Option<Rgb>,
    pub to_color: Option<Rgb>,
    pub start: f64,
    pub duration: f64,
    pub ease: EaseDecl,
    /// Per-piece delay in seconds (Sprint 7 motion, CAP-002): each
    /// animated piece starts `stagger` times its order index from
    /// `stagger_from` later. Default 0 (no cascade).
    pub stagger: f64,
    /// Stagger origin (Sprint 7 motion, CAP-002). Default `start`.
    pub stagger_from: StaggerOrigin,
    /// Stagger grid (Sprint 7 motion, CAP-002); none means the 1D
    /// piece order.
    pub stagger_grid: Option<StaggerGrid>,
    /// Replay count (Sprint 7 motion, CAP-004): 1 runs once, more
    /// replays the motion. Always 1 to 1000 after validation.
    pub repeat: u32,
    /// Whether alternate passes run backward (Sprint 7 motion,
    /// CAP-004). Default false.
    pub yoyo: bool,
    /// Held end values between passes in seconds (Sprint 7 motion,
    /// CAP-004). Default 0.
    pub repeat_delay: f64,
    /// Path travel (Sprint 7 motion, CAP-008): the SVG-subset path
    /// the clip follows by eased distance along true curve length.
    /// Present means a path tween: `property`, `kind`, `from`, and
    /// `to` must be absent. None means a numeric tween.
    pub path: Option<String>,
    /// Path morph destination (Sprint 7 motion, CAP-008): the path
    /// the travel interpolates toward, same structure required.
    /// Written `path-to` (or `path_to`).
    pub path_to: Option<String>,
    /// Whether the clip rotates to the path tangent (Sprint 7 motion,
    /// CAP-008): the tangent heading adds to the tweened rotation.
    /// Default false. Refused without `path`.
    pub orient: bool,
    /// Pure seeded expression (Sprint 7 motion, CAP-010): the value
    /// function of `t`, `i`, and declared inputs evaluated per frame
    /// over the tween window. Present means an expression tween:
    /// `from`, `to`, and path keys must be absent. None means a
    /// numeric or path tween.
    pub expr: Option<String>,
    pub line: usize,
    pub column: usize,
}

/// A validated `set` directive (T003): a stepwise hold of one property
/// of one clip over a half-open window, with no easing and no blending.
/// Drawing (T011) evaluates these; validation only guarantees they are
/// well-defined.
#[derive(Debug, Clone)]
pub struct SetDecl {
    /// Document index of the target clip.
    pub target: usize,
    /// Canonical animatable property name (one of `MOTION_PROPERTIES`).
    pub property: String,
    pub value: f64,
    pub start: f64,
    pub duration: f64,
    pub line: usize,
    pub column: usize,
}

/// One validated `frame` stop inside a `key` (Sprint 7 motion): the value
/// at `at` seconds, with the easing of the segment running to the next
/// stop (none means the key easing, then linear). The last stop's easing
/// is accepted and ignored: no segment leaves it. A path stop carries
/// `path` instead of `value` (Sprint 7 motion, CAP-008).
#[derive(Debug, Clone)]
pub struct KeyStop {
    pub at: f64,
    pub value: f64,
    /// Color value for `fill`/`border-color` stops (Sprint 7 motion,
    /// CAP-006): the interpolated color endpoint. `None` for every
    /// numeric property (`value` then holds the number).
    pub value_color: Option<Rgb>,
    /// Path travel of a path stop (Sprint 7 motion, CAP-008): the
    /// segment leaving this stop morphs toward the next stop's path.
    /// `None` for numeric stops.
    pub path: Option<String>,
    pub ease: Option<EaseDecl>,
    pub line: usize,
    pub column: usize,
}

/// A validated `key` directive (Sprint 7 motion): chained motion of one
/// property of one clip through at least two `frame` stops in time
/// order. Drawing evaluates each stop-to-stop segment with its own
/// easing; validation only guarantees the stops are well-defined.
#[derive(Debug, Clone)]
pub struct KeyDecl {
    /// Document index of the target clip.
    pub target: usize,
    /// Canonical animatable property name (one of `MOTION_PROPERTIES`).
    pub property: String,
    /// Key-level easing: the fallback for stops carrying no `ease`.
    pub ease: Option<EaseDecl>,
    /// Stops in document order with strictly rising `at` times.
    pub stops: Vec<KeyStop>,
    /// Per-piece delay in seconds (Sprint 7 motion, CAP-002): each
    /// animated piece starts `stagger` times its order index from
    /// `stagger_from` later. Default 0 (no cascade).
    pub stagger: f64,
    /// Stagger origin (Sprint 7 motion, CAP-002). Default `start`.
    pub stagger_from: StaggerOrigin,
    /// Stagger grid (Sprint 7 motion, CAP-002); none means the 1D
    /// piece order.
    pub stagger_grid: Option<StaggerGrid>,
    /// Replay count (Sprint 7 motion, CAP-004): 1 runs once, more
    /// replays the whole stop chain. Always 1 to 1000 after
    /// validation.
    pub repeat: u32,
    /// Whether alternate passes run backward (Sprint 7 motion,
    /// CAP-004). Default false.
    pub yoyo: bool,
    /// Held end values between passes in seconds (Sprint 7 motion,
    /// CAP-004). Default 0.
    pub repeat_delay: f64,
    /// Whether the clip rotates to the path tangent (Sprint 7 motion,
    /// CAP-008): the tangent heading adds to the keyed rotation.
    /// Default false. Refused on keys without path stops.
    pub orient: bool,
    pub line: usize,
    pub column: usize,
}

/// Properties tweenable and settable (Sprint 2 five, plus the Sprint 7
/// motion set): the five geometric ones, the 3D turns, the visual
/// properties, and blur. Anything else is refused as not-yet-supported
/// with its enabling sprint. Canonical hyphenated spellings; validation
/// also accepts the aliases below.
pub const MOTION_PROPERTIES: [&str; 16] = [
    "x",
    "y",
    "scale",
    "rotation",
    "opacity",
    "rotation-x",
    "rotation-y",
    "width",
    "height",
    "fill",
    "radius",
    "border-width",
    "border-color",
    "blur",
    "font-size",
    "letter-spacing",
];

/// Aliased motion property spellings, each mapping to its canonical
/// `MOTION_PROPERTIES` name: `rotate-x`/`rotate-y` predate the task
/// text, `size` is the text clip's own key, `fill-color` and
/// `border-radius` are the CSS-vocabulary names. Every hyphenated name
/// also accepts its underscore form.
pub const MOTION_ALIASES: [(&str, &str); 5] = [
    ("rotate-x", "rotation-x"),
    ("rotate-y", "rotation-y"),
    ("size", "font-size"),
    ("fill-color", "fill"),
    ("border-radius", "radius"),
];

/// Sound properties ridden by `key`/`tween`/`set` envelopes (Sprint 10
/// sound, CAP-001): `volume` is a linear level multiplier from 0
/// (silence) to 2 (twice unity), defaulting to 1; `pan` is an
/// equal-gain balance from −1 (left) to 1 (right), defaulting to 0.
/// They animate only on sound-bearing clips (`audio` tracks and `video`
/// clips); they are not visual motion properties, so the visual engine
/// never evaluates them. Every hyphenated name also accepts its
/// underscore form (here identical).
pub const SOUND_PROPERTIES: [&str; 2] = ["volume", "pan"];

/// The envelope level a sound property holds with no directive
/// (Sprint 10 sound, CAP-001): `volume` rides at unity gain, `pan` at
/// center. Tween endpoints a form omits resolve to these, exactly like
/// visual `to`/`from` tweens resolve to the clip's own values.
pub fn sound_own_value(property: &str) -> f64 {
    match property {
        "pan" => 0.0,
        _ => 1.0,
    }
}

/// Well-formed motion properties that still wait for later proof
/// (Sprint 7 paths/expressions and beyond).
pub const LATER_PROPERTIES: [&str; 7] = [
    "scale-x", "scale-y", "skew", "skew-x", "skew-y", "count", "typed",
];

/// Filter properties beyond blur (Sprint 7 motion, CAP-006): valid
/// shapes, but still Sprint 11 — refused as unsupported naming Sprint
/// 11, never drawn. `brightness` moved here from the Sprint 7 set: it
/// is a filter, not a Sprint 7 motion property.
pub const FILTER_PROPERTIES: [&str; 11] = [
    "brightness",
    "filter",
    "contrast",
    "saturate",
    "saturation",
    "grayscale",
    "greyscale",
    "hue-rotate",
    "sepia",
    "invert",
    "drop-shadow",
];

/// The clip's shape animates through its numeric fields (Sprint 7
/// motion, CAP-006 "shapes by numeric fields"): `width`, `height`, and
/// `radius` (plus `border-width`) interpolate the drawn geometry, so no
/// separate `clip-shape` property exists. `clip-shape` names nothing
/// animatable and is refused with this list.
pub const SHAPE_NUMERIC_PROPERTIES: [&str; 4] = ["width", "height", "radius", "border-width"];

/// A `nest` clip (Sprint 6 structural element): plays another
/// composition's local timeline inside this one with per-use
/// `trim-start`, `trim-end`, and `rate` (T007). At parent time `p` the
/// child shows its local time `trim_start + (p - start) * rate`; past
/// the content end the picture holds the child's last state while its
/// sound goes silent. The child's paint orders ride shifted by the
/// nest's own `order`, so the use stacks as one unit.
#[derive(Debug, Clone)]
pub struct NestClip {
    /// Name of the nested composition, as written.
    pub target: String,
    pub start: f64,
    pub duration: f64,
    /// Stable reference name (`id`), unique across the project when written.
    pub id: Option<String>,
    /// Display lane (`track`); defaults to graphics when undeclared.
    pub track: TrackKind,
    /// Editor display note (`label`); rides along with no render effect.
    pub label: Option<String>,
    /// Relative start, resolved to `start` by validation (T002); none
    /// means the absolute `start` stands as written.
    pub start_after: Option<StartAfter>,
    /// Child seconds where the use opens (`trim-start`); default 0.
    pub trim_start: f64,
    /// Child seconds where the use closes (`trim-end`); none means the
    /// child composition end. Past-end values clamp to the child end.
    pub trim_end: Option<f64>,
    /// Child seconds consumed per parent second (`rate`, 0.1–10);
    /// default 1. Pitch follows speed, like footage.
    pub rate: f64,
    /// Explicit paint order (default 0); the nested items stack shifted
    /// by this value, ties falling back to document order.
    pub order: i64,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub enum Clip {
    Text(TextClip),
    Video(VideoClip),
    Box(BoxClip),
    Image(ImageClip),
    Audio(AudioClip),
    Nest(NestClip),
}

impl Clip {
    pub fn start(&self) -> f64 {
        match self {
            Clip::Text(c) => c.start,
            Clip::Video(c) => c.start,
            Clip::Box(c) => c.start,
            Clip::Image(c) => c.start,
            Clip::Audio(c) => c.start,
            Clip::Nest(c) => c.start,
        }
    }

    /// Overwrite the absolute start (T002 relative resolution writes the
    /// resolved effective window back here, so every consumer renders it).
    pub fn set_start(&mut self, start: f64) {
        match self {
            Clip::Text(c) => c.start = start,
            Clip::Video(c) => c.start = start,
            Clip::Box(c) => c.start = start,
            Clip::Image(c) => c.start = start,
            Clip::Audio(c) => c.start = start,
            Clip::Nest(c) => c.start = start,
        }
    }

    /// The clip's own `id`, when declared.
    pub fn id(&self) -> Option<&str> {
        match self {
            Clip::Text(c) => c.id.as_deref(),
            Clip::Video(c) => c.id.as_deref(),
            Clip::Box(c) => c.id.as_deref(),
            Clip::Image(c) => c.id.as_deref(),
            Clip::Audio(c) => c.id.as_deref(),
            Clip::Nest(c) => c.id.as_deref(),
        }
    }

    /// The declared relative start, when `start-after` was written.
    pub fn start_after(&self) -> Option<&StartAfter> {
        match self {
            Clip::Text(c) => c.start_after.as_ref(),
            Clip::Video(c) => c.start_after.as_ref(),
            Clip::Box(c) => c.start_after.as_ref(),
            Clip::Image(c) => c.start_after.as_ref(),
            Clip::Audio(c) => c.start_after.as_ref(),
            Clip::Nest(c) => c.start_after.as_ref(),
        }
    }

    pub fn duration(&self) -> f64 {
        match self {
            Clip::Text(c) => c.duration,
            Clip::Video(c) => c.duration,
            Clip::Box(c) => c.duration,
            Clip::Image(c) => c.duration,
            Clip::Audio(c) => c.duration,
            Clip::Nest(c) => c.duration,
        }
    }

    /// Overwrite the resolved duration (T003 inference writes the
    /// inferred still length back here, so every consumer renders it).
    /// A `NAN` duration marks a still whose omitted length is still
    /// pending against the composition end; it never survives
    /// validation (a valid verdict carries only finite lengths).
    pub fn set_duration(&mut self, duration: f64) {
        match self {
            Clip::Text(c) => c.duration = duration,
            Clip::Video(c) => c.duration = duration,
            Clip::Box(c) => c.duration = duration,
            Clip::Image(c) => c.duration = duration,
            Clip::Audio(c) => c.duration = duration,
            Clip::Nest(c) => c.duration = duration,
        }
    }

    /// Diagnostic display name of the clip kind (`text clip`, ...).
    pub fn what(&self) -> &'static str {
        match self {
            Clip::Text(_) => "text clip",
            Clip::Video(_) => "video clip",
            Clip::Box(_) => "box clip",
            Clip::Image(_) => "image clip",
            Clip::Audio(_) => "audio track",
            Clip::Nest(_) => "nest clip",
        }
    }

    /// Source location of the clip node, for located diagnostics.
    pub fn line_col(&self) -> (usize, usize) {
        match self {
            Clip::Text(c) => (c.line, c.column),
            Clip::Video(c) => (c.line, c.column),
            Clip::Box(c) => (c.line, c.column),
            Clip::Image(c) => (c.line, c.column),
            Clip::Audio(c) => (c.line, c.column),
            Clip::Nest(c) => (c.line, c.column),
        }
    }

    /// Explicit paint order (default 0); ties fall back to document order.
    /// Audio tracks draw no picture, so they carry order 0 and never
    /// take part in paint sorting. A nest clip's own order shifts its
    /// nested items, which stack as one unit.
    pub fn order(&self) -> i64 {
        match self {
            Clip::Text(c) => c.order,
            Clip::Video(c) => c.order,
            Clip::Box(c) => c.order,
            Clip::Image(c) => c.order,
            Clip::Audio(_) => 0,
            Clip::Nest(c) => c.order,
        }
    }

    /// Visibility switch (Sprint 6 T009): `true` means the clip
    /// contributes no picture and no sound, as if absent, while its id
    /// stays referenceable and its windows keep validating.
    pub fn hidden(&self) -> bool {
        match self {
            Clip::Text(c) => c.hidden,
            Clip::Video(c) => c.hidden,
            Clip::Box(c) => c.hidden,
            Clip::Image(c) => c.hidden,
            Clip::Audio(c) => c.hidden,
            Clip::Nest(c) => c.hidden,
        }
    }

    /// Display lane of the clip (`track` key, else the kind default).
    pub fn track(&self) -> TrackKind {
        match self {
            Clip::Text(c) => c.track,
            Clip::Video(c) => c.track,
            Clip::Box(c) => c.track,
            Clip::Image(c) => c.track,
            Clip::Audio(c) => c.track,
            Clip::Nest(c) => c.track,
        }
    }

    /// Editor display note of the clip (`label` key), when declared.
    pub fn label(&self) -> Option<&str> {
        match self {
            Clip::Text(c) => c.label.as_deref(),
            Clip::Video(c) => c.label.as_deref(),
            Clip::Box(c) => c.label.as_deref(),
            Clip::Image(c) => c.label.as_deref(),
            Clip::Audio(c) => c.label.as_deref(),
            Clip::Nest(c) => c.label.as_deref(),
        }
    }

    /// Element name of the clip (`text`, `video`, `box`, `image`,
    /// `audio`, `nest`): the `kind` timelines and info report per row.
    pub fn kind(&self) -> &'static str {
        match self {
            Clip::Text(_) => "text",
            Clip::Video(_) => "video",
            Clip::Box(_) => "box",
            Clip::Image(_) => "image",
            Clip::Audio(_) => "audio",
            Clip::Nest(_) => "nest",
        }
    }
}

/// Optional project configuration file name, read from the folder root.
pub const CONFIG_FILE: &str = "framesmith.toml";

/// One declared expression input (Sprint 7 motion, CAP-010): an
/// `input` element names one finite number an `expr` may read by
/// name. Names use expression identifier syntax (letters, digits,
/// underscores, no leading digit), are unique per composition, and
/// never collide with `t`, `i`, constants, or function names.
#[derive(Debug, Clone)]
pub struct InputDecl {
    /// Declared name, as written.
    pub name: String,
    /// Declared value: always finite after validation.
    pub value: f64,
    pub line: usize,
    pub column: usize,
}

/// One `replicate` expansion (Sprint 7 motion, CAP-010): `count`
/// clones of one template clip, in document order starting at clip
/// `first`. Clone `k` exposes piece index `i = k` to expressions
/// (and to stagger distributions).
#[derive(Debug, Clone, Copy)]
pub struct ReplicateGroup {
    /// Document index of the first clone.
    pub first: usize,
    /// Number of clones (always positive after validation).
    pub count: usize,
}

/// Validated composition root (CAP-002 / T003).
#[derive(Debug, Clone)]
pub struct Composition {
    pub name: String,
    /// Stable reference name (`id`), unique across the project when written.
    pub id: Option<String>,
    /// Canvas preset name in canonical form (`1080p`, `720p`, `square`,
    /// `portrait`, `4k`); none when sized by explicit `width`/`height`.
    pub canvas: Option<String>,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: Fps,
    pub background: Rgb,
    pub version: u64,
    pub clips: Vec<Clip>,
    pub fonts: Vec<FontDecl>,
    /// Validated `tween` directives, in written order (T003). Still
    /// clips simply carry none; drawing (T009/T010/T012) evaluates them.
    pub tweens: Vec<TweenDecl>,
    /// Validated `set` directives, in written order (T003). Drawing
    /// (T011) evaluates them; overlapping sets resolve later-wins there.
    pub sets: Vec<SetDecl>,
    /// Validated `key` directives, in written order (Sprint 7 motion).
    /// Drawing expands each into stop-to-stop tweens below the sets.
    pub keys: Vec<KeyDecl>,
    /// Validated `mark` elements, in written order (Sprint 7 motion,
    /// CAP-003). `start-after` accepts a mark name, resolving to the
    /// marked instant; marks outside composition time are refused.
    pub marks: Vec<MarkDecl>,
    /// Declared expression inputs, in written order (Sprint 7 motion,
    /// CAP-010). Every `expr` in this composition resolves bare names
    /// against these; anything else is refused as unknown.
    pub inputs: Vec<InputDecl>,
    /// `replicate` expansions, in document order (Sprint 7 motion,
    /// CAP-010). Empty for replicate-free compositions.
    pub replicates: Vec<ReplicateGroup>,
    /// Playback-hint flag (Sprint 10 sound, CAP-007): carried into
    /// `info` only; never alters the mix or the picture, so toggling
    /// it re-renders byte-identically.
    pub autoplay: bool,
    /// One-pass loudness target (Sprint 10 sound, CAP-009): `loud`
    /// lands integrated loudness at −16 LUFS with true-peak guarding.
    /// Absent means the mix renders at its authored levels.
    pub normalize: Normalize,
    /// Location of the `normalize` key, when written; none means the
    /// default still levels.
    pub normalize_loc: Option<(usize, usize)>,
}

impl Composition {
    /// The replicate piece index of clip `clip_idx`: the clone offset
    /// inside its group, or 0 for ordinary clips. This is the `i` an
    /// expression reads for the clip.
    pub fn replicate_index(&self, clip_idx: usize) -> usize {
        for group in &self.replicates {
            if clip_idx >= group.first && clip_idx < group.first + group.count {
                return clip_idx - group.first;
            }
        }
        0
    }

    /// Declared input values in declaration order, for expression
    /// evaluation: entry `k` answers variable `k`.
    pub fn input_values(&self) -> Vec<f64> {
        self.inputs.iter().map(|d| d.value).collect()
    }

    /// Declared input names in declaration order, matching
    /// [`Composition::input_values`].
    pub fn input_names(&self) -> Vec<&str> {
        self.inputs.iter().map(|d| d.name.as_str()).collect()
    }
}

/// Half-open visibility rule (T005 / CAP-004): a clip with start `S` and
/// duration `D` is visible exactly for `S <= T < S + D`. The frame at the
/// exact end instant does not show the clip.
pub fn clip_visible(start: f64, duration: f64, t: f64) -> bool {
    t >= start && t < start + duration
}

/// Clip window clamped to the composition duration: overrunning clips are
/// cut at the composition end rather than extending the output (T005).
/// Returns `None` when nothing of the clip falls inside the composition.
pub fn clamped_window(start: f64, duration: f64, composition_duration: f64) -> Option<(f64, f64)> {
    let end = (start + duration).min(composition_duration);
    if end <= start || start >= composition_duration {
        return None;
    }
    Some((start.max(0.0), end))
}

/// One same-track overlap handoff (Sprint 10 sound, CAP-003): the
/// earlier clip (`early`) hands off to the later clip (`late`) across
/// their overlap. The late clip always fades in across the overlap;
/// the early clip fades out across it only when its tail lies inside
/// (`early_fades`); a continuing early clip is a bed and plays through
/// untouched. The effective overlap clamps the raw overlap to
/// composition time; `clamped` marks a partial cut, reported in
/// `check` for review.
#[derive(Debug, Clone)]
pub struct SoundOverlap {
    /// Document index of the earlier-starting clip.
    pub early: usize,
    /// Document index of the later-starting clip.
    pub late: usize,
    /// Resolved pair shape (see [`resolve_crossfade`]).
    pub shape: Crossfade,
    /// Overlap of the raw clip windows, in composition seconds.
    pub raw_start: f64,
    pub raw_end: f64,
    /// Overlap clamped to composition time, in composition seconds.
    pub eff_start: f64,
    pub eff_end: f64,
    /// True when the early clip's tail lies in the overlap (it fades
    /// out); false when it continues past (bed, plays through).
    pub early_fades: bool,
    /// True when the effective overlap is a partial cut of the raw
    /// overlap (an overlong window clamped to what renders).
    pub clamped: bool,
}

/// Resolve one pair's crossfade shape (Sprint 10 sound, CAP-003): a
/// `none` on either side hard-cuts, else a `linear` on either side
/// runs linear, else the default constant-power blend.
pub fn resolve_crossfade(a: Crossfade, b: Crossfade) -> Crossfade {
    if a == Crossfade::None || b == Crossfade::None {
        Crossfade::None
    } else if a == Crossfade::Linear || b == Crossfade::Linear {
        Crossfade::Linear
    } else {
        Crossfade::ConstantPower
    }
}

/// The `crossfade` declared on an audio-bearing clip; `None` for every
/// other clip kind (they never take part in overlap handoffs).
fn clip_crossfade(clip: &Clip) -> Option<Crossfade> {
    match clip {
        Clip::Audio(a) => Some(a.crossfade),
        Clip::Video(v) => Some(v.crossfade),
        _ => None,
    }
}

/// The echo/room/detection settings declared on an audio-bearing clip
/// (Sprint 10 sound, CAP-005/006); `None` for every other clip kind.
pub fn clip_fx(clip: &Clip) -> Option<&SoundFx> {
    match clip {
        Clip::Audio(a) => Some(&a.fx),
        Clip::Video(v) => Some(&v.fx),
        _ => None,
    }
}

/// The on-canvas sound drawing declared on an audio-bearing clip
/// (Sprint 10 sound, CAP-010); `None` for every other clip kind or
/// when the clip draws no sound shape.
pub fn clip_waveform(clip: &Clip) -> Option<&WaveformDraw> {
    match clip {
        Clip::Audio(a) if a.waveform.enabled => Some(&a.waveform),
        Clip::Video(v) if v.waveform.enabled => Some(&v.waveform),
        _ => None,
    }
}

/// Plan every same-track overlap handoff (Sprint 10 sound, CAP-003):
/// audio-bearing clips (`audio` tracks and `video` clips) sharing one
/// track lane, in start order, where the later clip starts strictly
/// inside the earlier one. Clips sharing a start stack as layers and
/// sum (no handoff); merely touching windows never overlap; hidden
/// clips are absent; pairs whose overlap leaves no audible sample are
/// dropped. Deterministic in document order, so every render plans the
/// same gains and re-renders stay byte-identical.
pub fn sound_overlaps(clips: &[Clip], composition_duration: f64) -> Vec<SoundOverlap> {
    let mut order: Vec<usize> = clips
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.hidden() && clip_crossfade(c).is_some())
        .map(|(i, _)| i)
        .collect();
    order.sort_by(|&a, &b| {
        clips[a]
            .start()
            .total_cmp(&clips[b].start())
            .then_with(|| clips[a].duration().total_cmp(&clips[b].duration()))
            .then_with(|| a.cmp(&b))
    });
    let mut out = Vec::new();
    for pair in order.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if clips[a].track() != clips[b].track() {
            continue;
        }
        let (sa, ea) = (clips[a].start(), clips[a].start() + clips[a].duration());
        let (sb, eb) = (clips[b].start(), clips[b].start() + clips[b].duration());
        // A handoff needs the newcomer to arrive strictly inside the
        // earlier window: shared starts stack as layers and sum.
        if !(sb > sa && sb < ea) {
            continue;
        }
        let raw_start = sb;
        let raw_end = ea.min(eb);
        debug_assert!(raw_end > raw_start);
        let eff_start = raw_start.max(0.0);
        let eff_end = raw_end.min(composition_duration);
        if eff_end <= eff_start {
            continue;
        }
        let shape = resolve_crossfade(
            clip_crossfade(&clips[a]).unwrap_or_default(),
            clip_crossfade(&clips[b]).unwrap_or_default(),
        );
        out.push(SoundOverlap {
            early: a,
            late: b,
            shape,
            raw_start,
            raw_end,
            eff_start,
            eff_end,
            early_fades: ea <= eb,
            clamped: eff_start != raw_start || eff_end != raw_end,
        });
    }
    out
}

/// The voice window plus duck amount driving one bed clip (Sprint 10
/// sound, CAP-004): the named voice clip's timeline window and the
/// bed's declared gain while it sounds. A hidden voice never sounds,
/// so it ducks nothing; an unknown or non-clip target resolves to
/// `None` here (validation refuses those before the first frame).
/// The amount converts to `f32` once, so the mix multiplies exactly
/// one gain per sample.
pub fn duck_target_window(clips: &[Clip], bed: usize) -> Option<(f64, f64, f32)> {
    let duck = match &clips[bed] {
        Clip::Audio(a) => a.duck_under.as_ref(),
        Clip::Video(v) => v.duck_under.as_ref(),
        _ => None,
    }?;
    let voice = clips
        .iter()
        .find(|c| c.id() == Some(duck.target.as_str()))?;
    if voice.hidden() {
        return None;
    }
    Some((
        voice.start(),
        voice.start() + voice.duration(),
        duck.amount as f32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_open_boundaries() {
        // Clip [2.0, 5.0): visible at start, inside, and just before end.
        assert!(clip_visible(2.0, 3.0, 2.0));
        assert!(clip_visible(2.0, 3.0, 3.5));
        assert!(clip_visible(2.0, 3.0, 4.999));
        // Invisible at the exact end instant and outside the window.
        assert!(!clip_visible(2.0, 3.0, 5.0));
        assert!(!clip_visible(2.0, 3.0, 5.001));
        assert!(!clip_visible(2.0, 3.0, 1.999));
        assert!(!clip_visible(2.0, 3.0, 0.0));
    }

    #[test]
    fn zero_start_clip() {
        assert!(clip_visible(0.0, 3.0, 0.0));
        assert!(!clip_visible(0.0, 3.0, 3.0));
    }

    #[test]
    fn overrun_is_clamped_not_extended() {
        assert_eq!(clamped_window(8.0, 5.0, 10.0), Some((8.0, 10.0)));
        assert_eq!(clamped_window(0.0, 3.0, 10.0), Some((0.0, 3.0)));
        assert_eq!(clamped_window(10.0, 2.0, 10.0), None);
        assert_eq!(clamped_window(11.0, 2.0, 10.0), None);
    }

    #[test]
    fn color_forms() {
        assert_eq!(
            parse_color("#ffffff"),
            Some(Rgb {
                r: 255,
                g: 255,
                b: 255
            })
        );
        assert_eq!(
            parse_color("#1A1A2e").map(|c| c.to_hex()),
            Some("1a1a2e".into())
        );
        assert_eq!(
            parse_color("#abc").map(|c| c.to_hex()),
            Some("aabbcc".into())
        );
        assert!(parse_color("ffffff").is_none());
        assert!(parse_color("#ffff").is_none());
        assert!(parse_color("#gggggg").is_none());
    }

    #[test]
    fn fps_math() {
        let f = Fps::from_int(30);
        assert_eq!(f.value(), 30.0);
        assert_eq!(f.frame_count(3.0), 90);
        let ntsc = Fps {
            num: 30000,
            den: 1001,
        };
        assert!((ntsc.value() - 29.97002997).abs() < 1e-6);
        assert_eq!(format!("{ntsc}"), "30000/1001");
    }

    #[test]
    fn fit_spellings_are_exact() {
        assert_eq!(Fit::parse("contain"), Some(Fit::Contain));
        assert_eq!(Fit::parse("Contain"), None);
        assert_eq!(Fit::parse("fit"), None);
    }

    #[test]
    fn placement_forms() {
        use super::{PlaceH, PlaceV, Placement};
        assert_eq!(Placement::parse("center"), Some(Placement::center()));
        let tl = Placement::parse("top-left").unwrap();
        assert_eq!((tl.x, tl.y), (PlaceH::Left, PlaceV::Top));
        let br = Placement::parse("right bottom").unwrap();
        assert_eq!((br.x, br.y), (PlaceH::Right, PlaceV::Bottom));
        assert_eq!(Placement::parse("left").unwrap().x, PlaceH::Left);
        assert_eq!(Placement::parse("bottom").unwrap().y, PlaceV::Bottom);
        assert!(Placement::parse("north").is_none());
        assert!(Placement::parse("left right").is_none());
        assert!(Placement::parse("").is_none());
        assert!(Placement::parse("center center center").is_none());
    }

    /// Sprint 10 sound (T003/T004): crossfade spellings parse exactly,
    /// pair shapes resolve none-over-linear-over-default, overlaps plan
    /// handoffs only, and duck windows follow voice timelines.
    fn sound_audio(
        start: f64,
        duration: f64,
        id: Option<&str>,
        crossfade: super::Crossfade,
        duck: Option<super::DuckUnder>,
        hidden: bool,
    ) -> super::Clip {
        super::Clip::Audio(super::AudioClip {
            path: "s.wav".to_string(),
            start,
            duration,
            id: id.map(str::to_string),
            track: super::TrackKind::Audio,
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
            fx: super::SoundFx::default(),
            waveform: super::WaveformDraw::default(),
            hidden,
            line: 1,
            column: 1,
        })
    }

    #[test]
    fn effect_tails_match_hand_math() {
        use super::*;
        // No spacing or no wet level plans no tail: dry by construction.
        assert_eq!(delay_taps(0.0, 0.5, 0.8), 0);
        assert_eq!(delay_taps(0.3, 0.5, 0.0), 0);
        assert_eq!(delay_tail_frames(0.0, 0.5, 0.8), 0);
        assert!(SoundFx::default().dry());
        // A lone tap (zero feedback) runs exactly one spacing.
        assert_eq!(delay_taps(0.3, 0.0, 0.8), 1);
        assert_eq!(delay_tail_frames(0.3, 0.0, 0.8), 14_400);
        // mix 0.8 with feedback 0.5: taps gain 0.8, 0.4, ...; the
        // tenth (0.8/512 = 0.0015625) still counts, the eleventh
        // (0.8/1024) falls below the floor: 10 taps of 14400 samples.
        assert_eq!(delay_taps(0.3, 0.5, 0.8), 10);
        assert_eq!(delay_tail_frames(0.3, 0.5, 0.8), 144_000);
        // A wet level below the floor plans nothing at all.
        assert_eq!(delay_taps(0.3, 0.5, 0.0005), 0);
        // Reverb 0.8 recirculates at 0.4 per 30 ms comb: taps gain
        // 0.8, 0.32, ...; the eighth (0.8*0.4^7 = 0.00013) is the last
        // above the floor: 8 taps of 1440 samples.
        assert_eq!(reverb_taps(0.0), 0);
        assert_eq!(reverb_taps(0.8), 8);
        assert_eq!(reverb_tail_frames(0.8), 8 * 1_440);
        // Chained tails add: echo first, then the room off it.
        assert_eq!(effect_tail_frames(0.3, 0.5, 0.8, 0.8), 144_000 + 8 * 1_440);
        assert_eq!(effect_tail_frames(0.0, 0.0, 0.0, 0.0), 0);
    }

    #[test]
    fn crossfade_spellings_parse_exactly() {
        use super::Crossfade;
        assert_eq!(
            Crossfade::parse("constant-power"),
            Some(Crossfade::ConstantPower)
        );
        assert_eq!(Crossfade::parse("linear"), Some(Crossfade::Linear));
        assert_eq!(Crossfade::parse("none"), Some(Crossfade::None));
        assert_eq!(Crossfade::parse("constant_power"), None);
        assert_eq!(Crossfade::parse("Linear"), None);
        assert_eq!(Crossfade::parse(""), None);
        assert_eq!(Crossfade::default(), Crossfade::ConstantPower);
        assert_eq!(Crossfade::ACCEPTED, ["constant-power", "linear", "none"]);
    }

    #[test]
    fn normalize_spellings_parse_exactly() {
        use super::Normalize;
        assert_eq!(Normalize::parse("loud"), Some(Normalize::Loud));
        assert_eq!(Normalize::parse("none"), None);
        assert_eq!(Normalize::parse("LOUD"), None);
        assert_eq!(Normalize::parse(""), None);
        assert_eq!(Normalize::default(), Normalize::None);
        assert!(!Normalize::None.active());
        assert!(Normalize::Loud.active());
        assert_eq!(Normalize::ACCEPTED, ["loud"]);
    }

    #[test]
    fn waveform_ranges_resolve_clamp_and_refuse() {
        use super::{resolve_waveform_range, WaveformRangeError};
        // Defaults span the clip window (clamped to the composition).
        let r = resolve_waveform_range(1.0, 2.0, 10.0, None, None).unwrap();
        assert_eq!((r.raw_from, r.raw_to), (1.0, 3.0));
        assert_eq!((r.eff_from, r.eff_to), (1.0, 3.0));
        assert!(!r.clamped);
        // An overrunning clip window clamps to the composition end.
        let r = resolve_waveform_range(9.0, 5.0, 10.0, None, None).unwrap();
        assert_eq!((r.eff_from, r.eff_to), (9.0, 10.0));
        assert!(!r.clamped);
        // A written range past the window clamps and reports.
        let r = resolve_waveform_range(1.0, 2.0, 10.0, Some(0.0), Some(5.0)).unwrap();
        assert_eq!((r.raw_from, r.raw_to), (0.0, 5.0));
        assert_eq!((r.eff_from, r.eff_to), (1.0, 3.0));
        assert!(r.clamped);
        // An interior written range passes through unclamped.
        let r = resolve_waveform_range(1.0, 2.0, 10.0, Some(1.5), Some(2.5)).unwrap();
        assert!(!r.clamped);
        assert_eq!((r.eff_from, r.eff_to), (1.5, 2.5));
        // Empty and backward written ranges fail before clamping.
        assert_eq!(
            resolve_waveform_range(1.0, 2.0, 10.0, Some(2.0), Some(2.0)),
            Err(WaveformRangeError::EmptyOrBackward)
        );
        assert_eq!(
            resolve_waveform_range(1.0, 2.0, 10.0, Some(2.5), Some(1.5)),
            Err(WaveformRangeError::EmptyOrBackward)
        );
        // A range fully outside the window fails with nothing to draw.
        assert_eq!(
            resolve_waveform_range(1.0, 2.0, 10.0, Some(5.0), Some(6.0)),
            Err(WaveformRangeError::OutsideWindow)
        );
        assert_eq!(
            resolve_waveform_range(1.0, 2.0, 10.0, Some(-2.0), Some(-1.0)),
            Err(WaveformRangeError::OutsideWindow)
        );
        // A clip with no audible window resolves nothing.
        assert_eq!(
            resolve_waveform_range(11.0, 2.0, 10.0, None, None),
            Err(WaveformRangeError::EmptyWindow)
        );
        assert_eq!(
            resolve_waveform_range(1.0, 0.0, 10.0, None, None),
            Err(WaveformRangeError::EmptyWindow)
        );
    }

    #[test]
    fn pair_shapes_resolve_none_over_linear_over_default() {
        use super::{resolve_crossfade, Crossfade};
        use Crossfade::*;
        assert_eq!(
            resolve_crossfade(ConstantPower, ConstantPower),
            ConstantPower
        );
        assert_eq!(resolve_crossfade(ConstantPower, Linear), Linear);
        assert_eq!(resolve_crossfade(Linear, ConstantPower), Linear);
        assert_eq!(resolve_crossfade(Linear, Linear), Linear);
        assert_eq!(resolve_crossfade(None, Linear), None);
        assert_eq!(resolve_crossfade(Linear, None), None);
        assert_eq!(resolve_crossfade(None, ConstantPower), None);
    }

    #[test]
    fn consecutive_overlaps_plan_one_handoff() {
        use super::{sound_overlaps, Crossfade};
        // Classic handoff: A [0,2) yields to B [1,3) across [1,2), and
        // A ends there so it fades out while B fades in.
        let clips = vec![
            sound_audio(0.0, 2.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(1.0, 2.0, None, Crossfade::ConstantPower, None, false),
        ];
        let plan = sound_overlaps(&clips, 3.0);
        assert_eq!(plan.len(), 1);
        assert_eq!((plan[0].early, plan[0].late), (0, 1));
        assert_eq!(plan[0].shape, Crossfade::ConstantPower);
        assert_eq!((plan[0].raw_start, plan[0].raw_end), (1.0, 2.0));
        assert_eq!((plan[0].eff_start, plan[0].eff_end), (1.0, 2.0));
        assert!(plan[0].early_fades);
        assert!(!plan[0].clamped);
    }

    #[test]
    fn layers_touching_lanes_and_hidden_never_hand_off() {
        use super::{sound_overlaps, Crossfade, TrackKind};
        // Shared starts stack as layers and sum (the overlap-sum lock);
        // touching windows never overlap.
        let stacked = vec![
            sound_audio(0.0, 1.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(0.0, 1.0, None, Crossfade::ConstantPower, None, false),
        ];
        assert!(sound_overlaps(&stacked, 1.0).is_empty());
        let touching = vec![
            sound_audio(0.0, 1.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(1.0, 1.0, None, Crossfade::ConstantPower, None, false),
        ];
        assert!(sound_overlaps(&touching, 2.0).is_empty());
        // A hidden clip is absent from the plan.
        let hidden = vec![
            sound_audio(0.0, 2.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(1.0, 2.0, None, Crossfade::ConstantPower, None, true),
        ];
        assert!(sound_overlaps(&hidden, 3.0).is_empty());
        // Different lanes sum, never blend: an audio bed under a video
        // voice keeps both full.
        let mut lanes = vec![
            sound_audio(0.0, 2.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(1.0, 2.0, None, Crossfade::ConstantPower, None, false),
        ];
        if let super::Clip::Audio(b) = &mut lanes[1] {
            b.track = TrackKind::Video;
        }
        assert!(sound_overlaps(&lanes, 3.0).is_empty());
    }

    #[test]
    fn continuing_early_clip_is_a_bed_and_overruns_clamp() {
        use super::{sound_overlaps, Crossfade};
        // B [1,2) sits inside A [0,4): B fades in, A plays through.
        let clips = vec![
            sound_audio(0.0, 4.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(1.0, 1.0, None, Crossfade::Linear, None, false),
        ];
        let plan = sound_overlaps(&clips, 4.0);
        assert_eq!(plan.len(), 1);
        assert!(!plan[0].early_fades, "a continuing bed plays through");
        assert_eq!(plan[0].shape, Crossfade::Linear);
        // An overlap running past the composition end clamps to what
        // renders and reports the cut.
        let over = vec![
            sound_audio(0.5, 2.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(1.5, 1.5, None, Crossfade::ConstantPower, None, false),
        ];
        let plan = sound_overlaps(&over, 2.0);
        assert_eq!(plan.len(), 1);
        assert_eq!((plan[0].raw_start, plan[0].raw_end), (1.5, 2.5));
        assert_eq!((plan[0].eff_start, plan[0].eff_end), (1.5, 2.0));
        assert!(plan[0].clamped);
        // An overlap entirely past the end leaves no audible sample.
        let past = vec![
            sound_audio(3.0, 1.0, None, Crossfade::ConstantPower, None, false),
            sound_audio(3.5, 1.0, None, Crossfade::ConstantPower, None, false),
        ];
        assert!(sound_overlaps(&past, 2.0).is_empty());
    }

    #[test]
    fn duck_windows_follow_voice_timelines() {
        use super::{duck_target_window, Crossfade, DuckUnder};
        let duck = |target: &str, amount: f64| DuckUnder {
            target: target.to_string(),
            amount,
            line: 1,
            column: 1,
        };
        let clips = vec![
            sound_audio(
                0.0,
                4.0,
                Some("bed"),
                Crossfade::ConstantPower,
                Some(duck("vox", 0.25)),
                false,
            ),
            sound_audio(1.0, 2.0, Some("vox"), Crossfade::ConstantPower, None, false),
        ];
        assert_eq!(duck_target_window(&clips, 0), Some((1.0, 3.0, 0.25f32)));
        assert_eq!(duck_target_window(&clips, 1), None);
        // A hidden voice never sounds, so it ducks nothing.
        let hidden = vec![
            sound_audio(
                0.0,
                4.0,
                Some("bed"),
                Crossfade::ConstantPower,
                Some(duck("vox", 0.25)),
                false,
            ),
            sound_audio(1.0, 2.0, Some("vox"), Crossfade::ConstantPower, None, true),
        ];
        assert_eq!(duck_target_window(&hidden, 0), None);
        // An unknown voice resolves to nothing here; validation
        // refuses it before the first frame.
        let unknown = vec![sound_audio(
            0.0,
            4.0,
            Some("bed"),
            Crossfade::ConstantPower,
            Some(duck("gone", 0.25)),
            false,
        )];
        assert_eq!(duck_target_window(&unknown, 0), None);
    }
}
