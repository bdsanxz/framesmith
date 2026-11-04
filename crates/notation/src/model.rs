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

/// Base text direction of a `text` clip (Sprint 8 look, CAP-002):
/// `ltr` lays runs left to right, `rtl` right to left, and `auto`
/// infers the direction from the first strong character. Only these
/// three spellings are valid; anything else is a diagnostic naming
/// this set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
    Auto,
}

impl TextDirection {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ltr" => Some(TextDirection::Ltr),
            "rtl" => Some(TextDirection::Rtl),
            "auto" => Some(TextDirection::Auto),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TextDirection::Ltr => "ltr",
            TextDirection::Rtl => "rtl",
            TextDirection::Auto => "auto",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["ltr", "rtl", "auto"];
}

/// One styled run of a `text` clip (Sprint 8 look, CAP-002/T002): a
/// `span` child overriding the clip style for its own text. Every
/// attribute is optional and falls back to the enclosing style (the
/// parent span, else the clip): `font-family` in comma try order,
/// `font-size` in canvas pixels, `font-weight` from 100 to 900,
/// `italic` as a boolean, `color` as `#rgb`/`#rrggbb`,
/// `letter-spacing` and `line-height` in canvas pixels. A span carries
/// its text in `value`, in nested `span` children (which inherit the
/// span's own overrides), or both (the value paints first, then the
/// children in order); a span with neither is refused.
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub value: String,
    pub font_family: Option<Vec<String>>,
    pub font_size: Option<f64>,
    pub font_weight: Option<f64>,
    pub italic: Option<bool>,
    pub color: Option<Rgb>,
    pub letter_spacing: Option<f64>,
    pub line_height: Option<f64>,
    pub children: Vec<TextSpan>,
    pub line: usize,
    pub column: usize,
}

/// Motion assertions of one visual clip (Sprint 13 check, T003 /
/// CAP-003): the authored `assert-appear`, `assert-inside`, and
/// `assert-moving` children in KDL, validated with the clip and
/// checked by lint after validation. `None` means the assertion was
/// not written; an unwritten assertion never fires. Audio clips carry
/// no picture, so they carry no assertions (an assertion child on an
/// audio clip is refused at validation).
#[derive(Debug, Clone, Default)]
pub struct MotionAsserts {
    /// `assert-appear by=<seconds>`: the clip must first show no later
    /// than `by` seconds. `None` means unwritten.
    pub appear: Option<AssertAppear>,
    /// `assert-inside`: the clip must stay fully inside the frame
    /// while visible (authored geometry; motion is not re-evaluated,
    /// like the T002 layout rules). `None` means unwritten.
    pub inside: Option<AssertSite>,
    /// `assert-moving`: the clip's picture must change across its
    /// window (a visual tween/key/set, a typewriter reveal, or a count
    /// range). `None` means unwritten.
    pub moving: Option<AssertSite>,
}

impl MotionAsserts {
    /// True when the clip carries no assertion at all.
    pub fn is_empty(&self) -> bool {
        self.appear.is_none() && self.inside.is_none() && self.moving.is_none()
    }
}

/// One validated `assert-appear` child: the deadline in composition
/// seconds plus the assertion site, so the finding names the element
/// and the measured appearance time.
#[derive(Debug, Clone)]
pub struct AssertAppear {
    /// The `by` deadline in composition seconds (finite, >= 0).
    pub by: f64,
    /// Location of the `assert-appear` child node.
    pub line: usize,
    pub column: usize,
}

/// One validated `assert-inside` or `assert-moving` child: a flag
/// assertion carrying only its site, so the finding names the element
/// and the measured bounds or stillness.
#[derive(Debug, Clone)]
pub struct AssertSite {
    /// Location of the assertion child node.
    pub line: usize,
    pub column: usize,
}

/// A `text` clip (Sprint 1 usable element, plus Sprint 2 paint order,
/// line height and letter spacing, plus Sprint 6 id/track/label, plus
/// Sprint 7 split/typewriter/count text effects, plus Sprint 8 styled
/// `span` runs with a base `direction`).
#[derive(Debug, Clone)]
pub struct TextClip {
    /// Motion assertions of the clip (Sprint 13 check, T003): the
    /// validated `assert-appear`, `assert-inside`, and `assert-moving`
    /// children. Empty when unwritten; lint checks them after
    /// validation.
    pub asserts: MotionAsserts,
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
    /// Base text direction (Sprint 8 look, CAP-002/T002): the bidi
    /// base the runs reorder under. Default `ltr`.
    pub direction: TextDirection,
    /// Styled runs (Sprint 8 look, CAP-002/T002): `span` children in
    /// document order. Empty means the whole `value` paints in the
    /// clip style, exactly as before this task.
    pub spans: Vec<TextSpan>,
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
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
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
    pub asserts: MotionAsserts,
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
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
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
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before. Audio clips draw no
    /// picture, so their grade rests inert; flex and nest grades
    /// resolve onto the items they emit.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
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

/// Main-axis direction of a `flex` container (Sprint 8 layout,
/// CAP-001): `row` flows children left to right, `column` top to
/// bottom. Only these two spellings are valid; anything else is a
/// diagnostic naming this set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

impl FlexDirection {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "row" => Some(FlexDirection::Row),
            "column" => Some(FlexDirection::Column),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FlexDirection::Row => "row",
            FlexDirection::Column => "column",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 2] = ["row", "column"];
}

/// Cross-axis alignment of a `flex` container (Sprint 8 layout,
/// CAP-001): `start` packs children at the cross start, `center`
/// centers each child across the content box, `end` packs them at the
/// cross end, and `stretch` grows each child across the content box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexAlign {
    Start,
    Center,
    End,
    Stretch,
}

impl FlexAlign {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "start" => Some(FlexAlign::Start),
            "center" => Some(FlexAlign::Center),
            "end" => Some(FlexAlign::End),
            "stretch" => Some(FlexAlign::Stretch),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FlexAlign::Start => "start",
            FlexAlign::Center => "center",
            FlexAlign::End => "end",
            FlexAlign::Stretch => "stretch",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 4] = ["start", "center", "end", "stretch"];
}

/// Main-axis distribution of a `flex` container (Sprint 8 layout,
/// CAP-001): `start` packs children at the main start, `center`
/// centers the row or column in the leftover space, and `end` packs
/// it at the main end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexJustify {
    Start,
    Center,
    End,
}

impl FlexJustify {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "start" => Some(FlexJustify::Start),
            "center" => Some(FlexJustify::Center),
            "end" => Some(FlexJustify::End),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FlexJustify::Start => "start",
            FlexJustify::Center => "center",
            FlexJustify::End => "end",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["start", "center", "end"];
}

/// One gradient color stop (Sprint 8 look, CAP-003/T003): a `stop`
/// child of a `linear-gradient` or `radial-gradient` fill. `offset`
/// runs 0 (gradient start) to 1 (gradient end); `color` is the stop
/// color. Neighboring stops interpolate in sRGB, channel by channel.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: Rgb,
    pub line: usize,
    pub column: usize,
}

/// Non-solid fill of a `box` clip (Sprint 8 look, CAP-003/T003): one
/// `linear-gradient`, `radial-gradient`, or `image-fill` child element.
/// `Solid` keeps the authored `fill` color, exactly as before this task.
#[derive(Debug, Clone, PartialEq)]
pub enum FillPaint {
    Solid,
    /// Linear gradient across the element box: `angle` in CSS degrees
    /// (0 points up, 90 points right; 90 is left-to-right), `stops` in
    /// rising offset order from 0 to 1.
    Linear {
        angle: f64,
        stops: Vec<GradientStop>,
    },
    /// Radial gradient of concentric rings: `cx`/`cy` name the center
    /// in box-local pixels (`None` centers on the box), `radius` names
    /// the ring radius in pixels (`None` reaches the farthest corner),
    /// `stops` run from the center (offset 0) outward (offset 1).
    Radial {
        cx: Option<f64>,
        cy: Option<f64>,
        radius: Option<f64>,
        stops: Vec<GradientStop>,
    },
    /// Image painted clipped to the element box: `src` is the project
    /// folder path, `fit` and `placement` behave like the `image`
    /// clip's `fit` and `placement` (`object-fit`/`object-position`).
    Image {
        src: String,
        fit: Fit,
        placement: Placement,
    },
}

impl FillPaint {
    /// True for the plain authored `fill` color (no fill child).
    pub fn is_solid(&self) -> bool {
        matches!(self, FillPaint::Solid)
    }
}

/// Where a `box` stroke paints relative to the fill edge (Sprint 8
/// look, CAP-004/T004): `inside` rings the inner edge (the historical
/// border behavior, and the default, so existing boxes render
/// byte-identically), `center` straddles the edge, and `outside` rings
/// the outer edge. Only these three spellings are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokePosition {
    Center,
    #[default]
    Inside,
    Outside,
}

impl StrokePosition {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "center" => Some(StrokePosition::Center),
            "inside" => Some(StrokePosition::Inside),
            "outside" => Some(StrokePosition::Outside),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            StrokePosition::Center => "center",
            StrokePosition::Inside => "inside",
            StrokePosition::Outside => "outside",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["center", "inside", "outside"];
}

/// How a `box` composites with the layers beneath (Sprint 8 look,
/// CAP-004/T004): `normal` paints source-over, `multiply`, `screen`,
/// and `overlay` blend channel by channel before the element alpha
/// composites. Only these four spellings are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
}

impl BlendMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "normal" => Some(BlendMode::Normal),
            "multiply" => Some(BlendMode::Multiply),
            "screen" => Some(BlendMode::Screen),
            "overlay" => Some(BlendMode::Overlay),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 4] = ["normal", "multiply", "screen", "overlay"];
}

/// Clipping of a `box` body at its own edge (Sprint 8 look,
/// CAP-004/T004): `visible` paints the whole stroke even past the
/// edge, `hidden` and `clip` cut fill and stroke at the rounded-rect
/// box (the shadow still paints outside). Only these three spellings
/// are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Clip,
}

impl Overflow {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "visible" => Some(Overflow::Visible),
            "hidden" => Some(Overflow::Hidden),
            "clip" => Some(Overflow::Clip),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Overflow::Visible => "visible",
            Overflow::Hidden => "hidden",
            Overflow::Clip => "clip",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["visible", "hidden", "clip"];
}

/// How a `path` shape fills its interior (Sprint 8 look,
/// CAP-005/T005): `nonzero` fills contours by nonzero winding (the
/// SVG default), `evenodd` alternates fill across nested contours.
/// Only these two spellings are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl FillRule {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "nonzero" => Some(FillRule::NonZero),
            "evenodd" => Some(FillRule::EvenOdd),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FillRule::NonZero => "nonzero",
            FillRule::EvenOdd => "evenodd",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 2] = ["nonzero", "evenodd"];
}

/// A `box-shadow` declaration (Sprint 8 look, CAP-004/T004): the
/// shadow paints below the element (the background stays transparent
/// so it shows through) offset by (`offset_x`, `offset_y`) canvas
/// pixels, blurred by `blur` sigma pixels, grown by `spread` pixels,
/// in `color`. Written as one attribute:
/// `box-shadow="8 8 12 0 #000000"`, or `box-shadow="none"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f64,
    pub offset_y: f64,
    pub blur: f64,
    pub spread: f64,
    pub color: Rgb,
}

/// A `mask` / `clip-path` reference (Sprint 8 look, CAP-004/T004): the
/// `id` of a `box` clip in the same composition whose rounded rect
/// masks this element. Forward references are allowed; dangling
/// references and cycles refuse before frame one. The referenced box
/// still paints normally; it additionally lends its shape as a mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskRef {
    pub target: String,
    pub line: usize,
    pub column: usize,
}

/// The resolved rounded-rect shape of a `mask` / `clip-path` target
/// (Sprint 8 look, CAP-004/T004): the referenced box's authored box in
/// canvas pixels. The masked element renders self-first, then its
/// alpha multiplies by this shape's coverage exactly once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaskShape {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

/// A `box` clip (Sprint 2 element, plus Sprint 8 gradient and image
/// fills, plus the Sprint 8 paint stack): position, size, fill color,
/// corner radius, stroke, shadow, blend, backdrop, overflow, and mask,
/// and at most one gradient or image fill child painting clipped to
/// the box. A fill child takes precedence over the `fill` color. The
/// paint stack renders in one layout-then-paint pass: backdrop samples
/// the flattened layers beneath, the shadow paints below the element,
/// the stroke paints above the fill at its position, the blend
/// composites with the layers beneath, overflow clips the body at the
/// box, and the mask applies once over self-first content.
#[derive(Debug, Clone)]
pub struct BoxClip {
    pub asserts: MotionAsserts,
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
    /// Gradient or image fill (Sprint 8 look, CAP-003/T003): `Solid`
    /// keeps the `fill` color; any other variant paints clipped to the
    /// box instead of it.
    pub fill_paint: FillPaint,
    pub radius: f64,
    pub border_width: f64,
    pub border_color: Rgb,
    /// Where the stroke paints relative to the fill edge (Sprint 8
    /// look, CAP-004/T004). Default `inside` (the historical border).
    pub stroke_position: StrokePosition,
    /// Drop shadow below the element (Sprint 8 look, CAP-004/T004);
    /// none paints no shadow.
    pub box_shadow: Option<BoxShadow>,
    /// Blend with the layers beneath (Sprint 8 look, CAP-004/T004).
    /// Default `normal` (source-over).
    pub blend_mode: BlendMode,
    /// Backdrop blur sigma in canvas pixels (Sprint 8 look,
    /// CAP-004/T004): samples the flattened layers beneath before the
    /// element paints. Default 0 (sharp).
    pub backdrop_blur: f64,
    /// Backdrop brightness factor (Sprint 8 look, CAP-004/T004):
    /// scales the sampled layers beneath (1 keeps them). Default 1.
    pub backdrop_brightness: f64,
    /// Body clipping at the box edge (Sprint 8 look, CAP-004/T004).
    /// Default `visible`.
    pub overflow: Overflow,
    /// Rounded-rect mask reference (Sprint 8 look, CAP-004/T004); none
    /// paints unmasked.
    pub mask: Option<MaskRef>,
    /// Rounded-rect clip-path reference (Sprint 8 look, CAP-004/T004);
    /// none paints unclipped.
    pub clip_path: Option<MaskRef>,
    /// 0 transparent to 1 opaque.
    pub opacity: f64,
    pub order: i64,
    /// 3D turn of the clip (Sprint 7 motion, CAP-005). Identity default.
    pub turn: Turn3D,
    /// Static Gaussian blur sigma in canvas pixels (Sprint 7 motion,
    /// CAP-006): 0 draws sharp.
    pub filter_blur: f64,
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// One laid-out child of a `flex` container (Sprint 8 layout, T001):
/// a box-sized item with its own paint. The position comes from the
/// container layout, never from authored `x`/`y`; the window comes
/// from the container, never from authored timing keys.
#[derive(Debug, Clone)]
pub struct FlexChild {
    /// Base main-axis size in canvas pixels (width in a row, height
    /// in a column) before `flex-grow` shares the free space.
    pub width: f64,
    pub height: f64,
    /// Share of leftover main-axis space; 0 keeps the base size.
    pub flex_grow: f64,
    pub fill: Rgb,
    pub radius: f64,
    pub border_width: f64,
    pub border_color: Rgb,
    /// Where the stroke paints relative to the fill edge (Sprint 8
    /// look, CAP-004/T004). Default `inside` (the historical border).
    pub stroke_position: StrokePosition,
    /// 0 transparent to 1 opaque.
    pub opacity: f64,
    /// Paint order inside the container; the container order shifts it.
    pub order: i64,
    pub line: usize,
    pub column: usize,
}

/// A `flex` container (Sprint 8 layout, CAP-001/T001): a row or column
/// of `box` children laid out by the CSS flex algorithm. `display`
/// must read `"flex"` when written; `flex-direction` names the main
/// axis; `gap` separates neighbors; `padding` insets the content box
/// on every side; `margin` offsets the container origin;
/// `align-items` places children across the content box and
/// `justify-content` distributes leftover space along it. Children
/// inherit the container window and are positioned by the layout.
#[derive(Debug, Clone)]
pub struct FlexClip {
    pub asserts: MotionAsserts,
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
    /// Container origin in canvas pixels, before `margin`.
    pub x: f64,
    pub y: f64,
    /// Container outer size in canvas pixels; both must be positive.
    pub width: f64,
    pub height: f64,
    pub direction: FlexDirection,
    /// Space between neighbors in canvas pixels; 0 packs them tight.
    pub gap: f64,
    /// Inset of the content box on every side, in canvas pixels.
    pub padding: f64,
    /// Offset of the container origin, in canvas pixels.
    pub margin: f64,
    pub align_items: FlexAlign,
    pub justify_content: FlexJustify,
    /// Explicit paint order (default 0); children ride shifted by this.
    pub order: i64,
    pub children: Vec<FlexChild>,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before. Audio clips draw no
    /// picture, so their grade rests inert; flex and nest grades
    /// resolve onto the items they emit.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
    pub line: usize,
    pub column: usize,
}

/// A `path` clip (Sprint 8 look, CAP-005/T005): an SVG-subset vector
/// shape (`d` with `M`, `L`, `C`, `Q`, `A`, `Z` commands) filled by
/// `fill-rule` in `fill` and stroked with the shared stroke set,
/// routed through the shared paint stack like a `box`. The `d`
/// coordinates are absolute canvas pixels; `x`/`y` translate the
/// whole shape (both default 0). Motion `scale` grows the shape
/// about its own top-left and `rotation` turns it about its own
/// center, exactly like a box about its box. Shadow offsets, blur,
/// and spread stay in canvas pixels and never scale.
#[derive(Debug, Clone)]
pub struct PathClip {
    pub asserts: MotionAsserts,
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
    /// Canvas-pixel translation of the whole shape; default 0.
    pub x: f64,
    pub y: f64,
    /// SVG-subset path data, as written.
    pub d: String,
    /// Fill color of the shape interior; default black (SVG canvas).
    pub fill: Rgb,
    /// Interior rule (`nonzero` default, `evenodd` alternate).
    pub fill_rule: FillRule,
    pub border_width: f64,
    pub border_color: Rgb,
    /// Where the stroke paints relative to the shape edge (Sprint 8
    /// look, CAP-004/T004). Default `inside`, like boxes.
    pub stroke_position: StrokePosition,
    /// Drop shadow below the element; none paints no shadow.
    pub box_shadow: Option<BoxShadow>,
    /// Blend with the layers beneath. Default `normal` (source-over).
    pub blend_mode: BlendMode,
    /// Backdrop blur sigma in canvas pixels; default 0 (sharp).
    pub backdrop_blur: f64,
    /// Backdrop brightness factor; default 1 (keeps beneath).
    pub backdrop_brightness: f64,
    /// Body clipping at the shape edge. Default `visible`.
    pub overflow: Overflow,
    /// Rounded-rect mask reference; none paints unmasked. Targets
    /// are `box` clips only, like every other element.
    pub mask: Option<MaskRef>,
    /// Rounded-rect clip-path reference; none paints unclipped.
    pub clip_path: Option<MaskRef>,
    /// 0 transparent to 1 opaque.
    pub opacity: f64,
    pub order: i64,
    /// 3D turn of the clip (Sprint 7 motion, CAP-005). Identity default.
    pub turn: Turn3D,
    /// Static Gaussian blur sigma in canvas pixels (Sprint 7 motion,
    /// CAP-006): 0 draws sharp.
    pub filter_blur: f64,
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// An `svg` clip (Sprint 8 look, CAP-005/T005): a standalone `.svg`
/// file rasterized at its declared `width`/`height` (both required
/// and positive, even below 64px) with `fit` and `placement`
/// composing it into the target rect exactly like the `image` clip,
/// which also accepts `.svg` sources. Pointing `svg` at a missing,
/// unreadable, or corrupt file is refused; animations inside the SVG
/// (SMIL, script) never play — the first frame's still picture
/// renders.
#[derive(Debug, Clone)]
pub struct SvgClip {
    pub asserts: MotionAsserts,
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
    /// Declared raster size in canvas pixels; both required positive.
    pub width: f64,
    pub height: f64,
    pub fit: Fit,
    pub placement: Placement,
    pub order: i64,
    /// 3D turn of the clip (Sprint 7 motion, CAP-005). Identity default.
    pub turn: Turn3D,
    /// Static Gaussian blur sigma in canvas pixels (Sprint 7 motion,
    /// CAP-006): 0 draws sharp.
    pub filter_blur: f64,
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
    /// Visibility switch (Sprint 6 T009): `true` skips picture and
    /// sound as if absent, while the id stays referenceable.
    pub hidden: bool,
    pub line: usize,
    pub column: usize,
}

/// An `image` clip (Sprint 2 element, plus Sprint 5 HEIC and still-GIF
/// sources, plus Sprint 8 SVG sources): a PNG, JPEG, HEIC/HEIF,
/// still-GIF, or SVG source placed with a fit and a placement inside
/// its target rect. Pointing `image` at an animated GIF is refused;
/// animations play in a `video` clip.
#[derive(Debug, Clone)]
pub struct ImageClip {
    pub asserts: MotionAsserts,
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
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
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
    pub asserts: MotionAsserts,
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
    /// Color grade of the clip (Sprint 11 grade, CAP-001/T001): the
    /// authored `grade` child, resolved with the lane and
    /// composition grades at frame time. Empty by default, so
    /// ungraded clips render exactly as before. Audio clips draw no
    /// picture, so their grade rests inert; flex and nest grades
    /// resolve onto the items they emit.
    pub grade: Grade,
    /// Named media effects of the clip (Sprint 11 effects, CAP-003/T004):
    /// authored `effect` children in document order, applied after the
    /// grade chain in that same order. Empty by default, so clips
    /// without effects render exactly as before.
    pub effects: Vec<Effect>,
    /// Ordered VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the authored `vfx` child, applied after the effects stack in
    /// the fixed order noise, warp, displacement, luma matte. Empty
    /// by default, so clips without VFX render exactly as before.
    pub vfx: VfxChain,
    pub line: usize,
    pub column: usize,
}

/// A color-grade preset (Sprint 11 grade, CAP-001/T001): the named
/// base look a `grade` applies before its adjust attributes. Only
/// these six spellings are valid; anything else is refused, never
/// guessed into a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GradePreset {
    /// No preset look; the adjust attributes (if any) shape the pixels.
    #[default]
    None,
    /// Filmic contrast curve with a touch less saturation.
    Filmic,
    /// High-contrast black and white.
    Noir,
    /// Cool shadows toward teal, warm highlights toward orange.
    TealOrange,
    /// Lifted blacks and softened contrast with muted saturation.
    Pastel,
    /// Bright, contrasty, strongly desaturated (bleach bypass).
    Bleach,
}

impl GradePreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(GradePreset::None),
            "filmic" => Some(GradePreset::Filmic),
            "noir" => Some(GradePreset::Noir),
            "teal-orange" => Some(GradePreset::TealOrange),
            "pastel" => Some(GradePreset::Pastel),
            "bleach" => Some(GradePreset::Bleach),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            GradePreset::None => "none",
            GradePreset::Filmic => "filmic",
            GradePreset::Noir => "noir",
            GradePreset::TealOrange => "teal-orange",
            GradePreset::Pastel => "pastel",
            GradePreset::Bleach => "bleach",
        }
    }

    /// Accepted spellings, in spec order.
    pub const ACCEPTED: [&'static str; 6] = [
        "none",
        "filmic",
        "noir",
        "teal-orange",
        "pastel",
        "bleach",
    ];
}

/// Accepted `grade` adjust ranges (Sprint 11 grade, CAP-001/T001):
/// values outside these fail validation naming the attribute and the
/// range, never clamp silently.
pub const GRADE_EXPOSURE_RANGE: (f64, f64) = (-4.0, 4.0);
pub const GRADE_CONTRAST_RANGE: (f64, f64) = (-1.0, 1.0);
pub const GRADE_SATURATION_RANGE: (f64, f64) = (-1.0, 2.0);
pub const GRADE_TEMPERATURE_RANGE: (f64, f64) = (-1.0, 1.0);
pub const GRADE_TINT_RANGE: (f64, f64) = (-1.0, 1.0);

/// Accepted `wheel` band range (Sprint 11 grade, CAP-001/T002): each of
/// `shadows`, `midtones`, `highlights` lifts its tonal zone by this
/// much; values outside fail validation, never clamp silently.
pub const GRADE_WHEEL_BAND_RANGE: (f64, f64) = (-1.0, 1.0);
/// Accepted `hsl` ranges (Sprint 11 grade, CAP-001/T002): the hue window
/// in degrees, the saturation selection floor, and the lightness shift.
pub const GRADE_HSL_HUE_RANGE: (f64, f64) = (0.0, 360.0);
pub const GRADE_HSL_SATURATION_RANGE: (f64, f64) = (0.0, 1.0);
pub const GRADE_HSL_LIGHTNESS_RANGE: (f64, f64) = (-1.0, 1.0);
/// Accepted `vignette` ranges (Sprint 11 grade, CAP-001/T002): corner
/// darkening strength and edge falloff, both unit.
pub const GRADE_VIGNETTE_AMOUNT_RANGE: (f64, f64) = (0.0, 1.0);
pub const GRADE_VIGNETTE_SOFTNESS_RANGE: (f64, f64) = (0.0, 1.0);
/// Accepted `grain` ranges (Sprint 11 grade, CAP-001/T002): visibility
/// amount (unit) and cell size in pixels.
pub const GRADE_GRAIN_AMOUNT_RANGE: (f64, f64) = (0.0, 1.0);
pub const GRADE_GRAIN_SIZE_RANGE: (i64, i64) = (1, 64);
/// Seed used when a `grain` child omits `seed` (Sprint 11 grade,
/// CAP-001/T002): fixed, never wall-clock randomness, so seedless
/// grain still re-renders byte-identically.
pub const GRADE_DEFAULT_GRAIN_SEED: i64 = 0;
/// Falloff used when a `vignette` child omits `softness`.
pub const GRADE_DEFAULT_VIGNETTE_SOFTNESS: f64 = 0.5;
/// Cell size used when a `grain` child omits `size`: per-pixel grain.
pub const GRADE_DEFAULT_GRAIN_SIZE: u32 = 1;
/// Peak channel swing of full-amount grain in 0..255 levels.
pub const GRADE_GRAIN_PEAK: f64 = 48.0;

/// One authored `wheel` stage (Sprint 11 grade, CAP-001/T002): the
/// `wheel` child of a `grade`. Each band lifts its tonal zone (`shadows`
/// below middle gray, `highlights` above, `midtones` as a bell
/// between); every band is optional and a missing band inherits the
/// outer level, never zeroes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WheelGrade {
    /// Lift of dark tones; none means inherit.
    pub shadows: Option<f64>,
    /// Lift of middle tones; none means inherit.
    pub midtones: Option<f64>,
    /// Lift of bright tones; none means inherit.
    pub highlights: Option<f64>,
}

impl WheelGrade {
    /// True when no band is authored.
    pub fn is_empty(&self) -> bool {
        self.shadows.is_none() && self.midtones.is_none() && self.highlights.is_none()
    }
}

/// One authored `curve` stage (Sprint 11 grade, CAP-001/T002): the
/// `curve` child of a `grade`. Each key holds a point list
/// (`"0,0 0.5,0.6 1,1"`: whitespace-separated `x,y` pairs) remapping
/// that channel; `master` remaps every channel before the per-channel
/// lists. A missing channel inherits the outer level.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CurveGrade {
    /// Red-channel points; none means inherit.
    pub red: Option<Vec<(f64, f64)>>,
    /// Green-channel points; none means inherit.
    pub green: Option<Vec<(f64, f64)>>,
    /// Blue-channel points; none means inherit.
    pub blue: Option<Vec<(f64, f64)>>,
    /// All-channel points, applied first; none means inherit.
    pub master: Option<Vec<(f64, f64)>>,
}

impl CurveGrade {
    /// Channel keys in validation order.
    pub const CHANNELS: [&'static str; 4] = ["red", "green", "blue", "master"];

    /// True when no channel list is authored.
    pub fn is_empty(&self) -> bool {
        self.red.is_none() && self.green.is_none() && self.blue.is_none() && self.master.is_none()
    }
}

/// Parse one curve point list (`"0,0 0.5,0.6 1,1"`) into `(x, y)`
/// pairs. Needs at least two pairs, every number finite in 0..1, and
/// strictly rising `x`; anything else is `None` (the caller refuses it
/// naming the element and the expected shape).
pub fn parse_curve_points(s: &str) -> Option<Vec<(f64, f64)>> {
    let mut pts = Vec::new();
    for tok in s.split_whitespace() {
        let (xs, ys) = tok.split_once(',')?;
        if xs.is_empty() || ys.is_empty() {
            return None;
        }
        // Exactly one comma per pair: `0,0,` or `0,,0` never parses.
        if xs.contains(',') || ys.contains(',') {
            return None;
        }
        let x: f64 = xs.parse().ok()?;
        let y: f64 = ys.parse().ok()?;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }
        if let Some((px, _)) = pts.last() {
            if x <= *px {
                return None;
            }
        }
        pts.push((x, y));
    }
    if pts.len() < 2 {
        return None;
    }
    Some(pts)
}

/// One authored `hsl` stage (Sprint 11 grade, CAP-001/T002): the `hsl`
/// child of a `grade`. Colors whose hue falls in `hue-min`..`hue-max`
/// (degrees, wrapping past 360 when min exceeds max) and whose
/// saturation reaches the `saturation` floor take the `lightness`
/// shift; every other pixel passes through. Every key is optional and
/// a missing key inherits the outer level.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HslGrade {
    /// Hue window start in degrees; none means inherit.
    pub hue_min: Option<f64>,
    /// Hue window end in degrees; none means inherit.
    pub hue_max: Option<f64>,
    /// Saturation selection floor; none means inherit.
    pub saturation: Option<f64>,
    /// Lightness shift; none means inherit.
    pub lightness: Option<f64>,
}

impl HslGrade {
    /// True when no selection or shift is authored.
    pub fn is_empty(&self) -> bool {
        self.hue_min.is_none()
            && self.hue_max.is_none()
            && self.saturation.is_none()
            && self.lightness.is_none()
    }
}

/// One authored `vignette` stage (Sprint 11 grade, CAP-001/T002): the
/// `vignette` child of a `grade`. `amount` darkens frame corners toward
/// black; `softness` widens the falloff from the corners toward the
/// center (the center pixel itself never moves). Both keys are optional
/// and a missing key inherits the outer level.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VignetteGrade {
    /// Corner darkening strength; none means inherit.
    pub amount: Option<f64>,
    /// Edge falloff width; none means inherit.
    pub softness: Option<f64>,
}

impl VignetteGrade {
    /// True when neither knob is authored.
    pub fn is_empty(&self) -> bool {
        self.amount.is_none() && self.softness.is_none()
    }
}

/// One authored `grain` stage (Sprint 11 grade, CAP-001/T002): the
/// `grain` child of a `grade`. `amount` sets visibility, `size` groups
/// pixels into square cells sharing one value, and `seed` picks the
/// deterministic pattern (omitted seeds use [`GRADE_DEFAULT_GRAIN_SEED`]).
/// Every key is optional and a missing key inherits the outer level.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GrainGrade {
    /// Visibility amount; none means inherit.
    pub amount: Option<f64>,
    /// Cell size in pixels; none means inherit.
    pub size: Option<u32>,
    /// Pattern seed; none means inherit (resolving to the default seed).
    pub seed: Option<i64>,
}

impl GrainGrade {
    /// True when no grain knob is authored.
    pub fn is_empty(&self) -> bool {
        self.amount.is_none() && self.size.is_none() && self.seed.is_none()
    }
}

/// Accepted `lut` amount range (Sprint 11 grade, CAP-002/T003): 0
/// leaves the pipeline untouched (pixel-identical to the `lut`
/// removed), 1 applies the grid fully; anything else fails
/// validation naming this range, never clamps silently.
pub const GRADE_LUT_AMOUNT_RANGE: (f64, f64) = (0.0, 1.0);

/// Largest `.cube` grid edge Framesmith imports or bakes (Sprint 11
/// grade, CAP-002/T003): grids run 2 to 33 points per axis. A larger
/// `LUT_3D_SIZE` fails naming this limit; a baked file never exceeds
/// it either.
pub const LUT_MAX_GRID_SIZE: usize = 33;

/// One authored `lut` stage (Sprint 11 grade, CAP-002/T003): the
/// `lut` child of a `grade`. `src` names a project-folder-relative
/// `.cube` file applied at this chain position; `amount` blends the
/// grid (0 passes through, 1 applies fully). Both keys are optional
/// and a missing key inherits the outer level, like every other
/// stage; a bare `lut` is the identity.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LutGrade {
    /// `.cube` file path; none means inherit.
    pub src: Option<String>,
    /// Blend amount; none means inherit (resolving to full).
    pub amount: Option<f64>,
}

impl LutGrade {
    /// True when no LUT knob is authored.
    pub fn is_empty(&self) -> bool {
        self.src.is_none() && self.amount.is_none()
    }
}

/// One parsed 3D `.cube` lookup table (Sprint 11 grade, CAP-002/T003):
/// `size` grid points per axis over the declared domain (unit by
/// default), `values` in R-fastest order (`(b * size + g) * size + r`),
/// absolute output triplets in the same domain.
#[derive(Debug, Clone, PartialEq)]
pub struct LutTable {
    /// Grid points per axis (2 to [`LUT_MAX_GRID_SIZE`]).
    pub size: usize,
    /// `size * size * size` absolute output triplets, R fastest.
    pub values: Vec<[f32; 3]>,
    /// Declared title, if the file names one.
    pub title: Option<String>,
    /// Declared input domain lower corner (unit by default).
    pub domain_min: [f64; 3],
    /// Declared input domain upper corner (unit by default).
    pub domain_max: [f64; 3],
}

impl LutTable {
    /// Trilinear lookup of one normalized (0..1) pixel: the query is
    /// scaled into the declared domain, interpolated over the eight
    /// surrounding lattice points, and scaled back to normalized
    /// units. Queries within float dust of a lattice plane snap to
    /// it, so sampling a grid exactly on its own lattice returns the
    /// stored triplets bit-exactly (a same-size bake reimports
    /// pixel-identically).
    pub fn lookup(&self, r: f64, g: f64, b: f64) -> [f64; 3] {
        let n = self.size;
        // Absolute domain coordinates of the normalized query: the
        // lattice spans the declared domain, not the unit cube.
        let mut t = [0.0f64; 3];
        for (i, q) in [r, g, b].iter().enumerate() {
            let span = self.domain_max[i] - self.domain_min[i];
            t[i] = if span > 0.0 {
                self.domain_min[i] + q.clamp(0.0, 1.0) * span
            } else {
                self.domain_min[i]
            };
        }
        // Lattice coordinates with dust snapping.
        let mut i0 = [0usize; 3];
        let mut fr = [0.0f64; 3];
        for (k, x) in t.iter().enumerate() {
            let span = self.domain_max[k] - self.domain_min[k];
            let unit = if span > 0.0 {
                ((x - self.domain_min[k]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let f = (unit * (n as f64 - 1.0)).clamp(0.0, n as f64 - 1.0);
            let mut base = f.floor() as usize;
            let mut frac = f - base as f64;
            if frac < 1e-9 {
                frac = 0.0;
            } else if frac > 1.0 - 1e-9 {
                frac = 0.0;
                base = base.saturating_add(1);
            }
            if base >= n {
                base = n - 1;
                frac = 0.0;
            }
            i0[k] = base;
            fr[k] = frac;
        }
        let at = |r: usize, g: usize, b: usize| -> [f64; 3] {
            let v = self.values[(b * n + g) * n + r];
            // Denormalize from absolute domain units back to 0..1.
            let mut out = [0.0f64; 3];
            for i in 0..3 {
                let span = self.domain_max[i] - self.domain_min[i];
                out[i] = if span > 0.0 {
                    (f64::from(v[i]) - self.domain_min[i]) / span
                } else {
                    f64::from(v[i])
                };
            }
            out
        };
        let r1 = (i0[0] + 1).min(n - 1);
        let g1 = (i0[1] + 1).min(n - 1);
        let b1 = (i0[2] + 1).min(n - 1);
        let (fx, fy, fz) = (fr[0], fr[1], fr[2]);
        let c000 = at(i0[0], i0[1], i0[2]);
        let c100 = at(r1, i0[1], i0[2]);
        let c010 = at(i0[0], g1, i0[2]);
        let c110 = at(r1, g1, i0[2]);
        let c001 = at(i0[0], i0[1], b1);
        let c101 = at(r1, i0[1], b1);
        let c011 = at(i0[0], g1, b1);
        let c111 = at(r1, g1, b1);
        let mut out = [0.0f64; 3];
        for i in 0..3 {
            let x00 = c000[i] + (c100[i] - c000[i]) * fx;
            let x10 = c010[i] + (c110[i] - c010[i]) * fx;
            let x01 = c001[i] + (c101[i] - c001[i]) * fx;
            let x11 = c011[i] + (c111[i] - c011[i]) * fx;
            let y0 = x00 + (x10 - x00) * fy;
            let y1 = x01 + (x11 - x01) * fy;
            out[i] = y0 + (y1 - y0) * fz;
        }
        out
    }
}

/// Parse one `.cube` file body (Sprint 11 grade, CAP-002/T003) into
/// its grid. Accepts `TITLE`, `DOMAIN_MIN`/`DOMAIN_MAX` (unit by
/// default), and the required `LUT_3D_SIZE` 2 to
/// [`LUT_MAX_GRID_SIZE`], then exactly `size^3` finite data triplets
/// in R-fastest order; `#` starts a comment. Anything else is an
/// error naming the file content and the fix (a 1D LUT, a missing
/// size, an oversized grid, a bad triplet, or a short/long body).
pub fn parse_cube(text: &str) -> Result<LutTable, String> {
    let mut title: Option<String> = None;
    let mut size: Option<usize> = None;
    let mut domain_min = [0.0f64; 3];
    let mut domain_max = [1.0f64; 3];
    let mut values: Vec<[f32; 3]> = Vec::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line_no = lineno + 1;
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // A `TITLE` may carry `#` inside its quotes, so it is
        // matched before comment stripping.
        let keyword = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if keyword == "TITLE" {
            let rest = line["TITLE".len()..].trim();
            let unquoted = rest
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(rest);
            title = Some(unquoted.to_string());
            continue;
        }
        if let Some((head, _)) = line.split_once('#') {
            line = head.trim();
            if line.is_empty() {
                continue;
            }
        }
        let keyword = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let rest = line
            .split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ");
        match keyword.as_str() {
            "LUT_3D_SIZE" => {
                match rest.trim().parse::<usize>() {
                    Ok(n) if n > LUT_MAX_GRID_SIZE => {
                        return Err(format!(
                            "`LUT_3D_SIZE {n}` on line {line_no} exceeds the {max} grid limit; a `.cube` grid is {max} or smaller per axis",
                            max = LUT_MAX_GRID_SIZE
                        ));
                    }
                    Ok(n) if n >= 2 => {
                        if size.is_some() {
                            return Err(format!(
                                "line {line_no} repeats `LUT_3D_SIZE`; a `.cube` names its grid size once"
                            ));
                        }
                        size = Some(n);
                    }
                    _ => {
                        return Err(format!(
                            "line {line_no} sets `LUT_3D_SIZE` to {rest:?}; a `.cube` grid runs 2 to {max} points per axis",
                            max = LUT_MAX_GRID_SIZE
                        ));
                    }
                }
            }
            "LUT_1D_SIZE" => {
                return Err(format!(
                    "line {line_no} declares a 1D LUT (`LUT_1D_SIZE`); Framesmith imports 3D `.cube` files only (`LUT_3D_SIZE` 2 to {max})",
                    max = LUT_MAX_GRID_SIZE
                ));
            }
            "DOMAIN_MIN" | "DOMAIN_MAX" => {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() != 3 {
                    return Err(format!(
                        "line {line_no} sets `{keyword}` to {rest:?}; a domain corner lists exactly three numbers like `DOMAIN_MIN 0.0 0.0 0.0`"
                    ));
                }
                let mut corner = [0.0f64; 3];
                for (i, p) in parts.iter().enumerate() {
                    match p.parse::<f64>() {
                        Ok(x) if x.is_finite() => corner[i] = x,
                        _ => {
                            return Err(format!(
                                "line {line_no} sets `{keyword}` to {rest:?}; every domain number is finite like `DOMAIN_MIN 0.0 0.0 0.0`"
                            ));
                        }
                    }
                }
                if keyword == "DOMAIN_MIN" {
                    domain_min = corner;
                } else {
                    domain_max = corner;
                }
            }
            _ => {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let triplet = parts.len() == 3
                    && parts
                        .iter()
                        .all(|p| p.parse::<f64>().map(|x| x.is_finite()).unwrap_or(false));
                if !triplet {
                    return Err(format!(
                        "line {line_no} reads {line:?}; a `.cube` body holds `LUT_3D_SIZE`, domain corners, and one `r g b` triplet per line"
                    ));
                }
                if size.is_none() {
                    return Err(format!(
                        "line {line_no} holds data before any `LUT_3D_SIZE`; a `.cube` names its grid size before its triplets"
                    ));
                }
                let v: Vec<f64> =
                    parts.iter().map(|p| p.parse::<f64>().unwrap_or(0.0)).collect();
                values.push([v[0] as f32, v[1] as f32, v[2] as f32]);
            }
        }
    }
    let size = match size {
        Some(n) => n,
        None => {
            return Err(
                "missing `LUT_3D_SIZE`; a `.cube` names its grid size, e.g. `LUT_3D_SIZE 33`"
                    .to_string(),
            );
        }
    };
    let want = size * size * size;
    if values.len() != want {
        return Err(format!(
            "found {} data triplets for `LUT_3D_SIZE {size}`; a `.cube` body holds exactly {want} `r g b` lines",
            values.len()
        ));
    }
    for i in 0..3 {
        if !(domain_max[i] > domain_min[i]) {
            return Err(format!(
                "`DOMAIN_MAX` {domain_max:?} must sit above `DOMAIN_MIN` {domain_min:?} on every axis"
            ));
        }
    }
    Ok(LutTable {
        size,
        values,
        title,
        domain_min,
        domain_max,
    })
}

/// One authored `grade` level (Sprint 11 grade, CAP-001/T001+T002): a
/// `grade` child on a clip, a lane group, or the composition. Every
/// field is optional: a missing piece is skipped (inherited from the
/// outer level), never zeroed. An explicit `preset="none"` clears the
/// preset stage for that level inward; an omitted `preset` inherits it.
/// Detail stages (`wheel`, `curve`, `hsl`, `vignette`, `grain`,
/// `lut`) layer the same way, piece by piece.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Grade {
    pub preset: Option<GradePreset>,
    /// Exposure shift in EV (×2^exposure); none means inherit.
    pub exposure: Option<f64>,
    /// Linear contrast around middle gray (0 is identity, -1 is flat
    /// gray); none means inherit.
    pub contrast: Option<f64>,
    /// Saturation shift (0 is identity, -1 is grayscale); none means
    /// inherit.
    pub saturation: Option<f64>,
    /// Warmth shift (positive is warmer); none means inherit.
    pub temperature: Option<f64>,
    /// Green-magenta shift (positive is magenta); none means inherit.
    pub tint: Option<f64>,
    /// Color-wheel lifts per tonal zone; missing bands inherit.
    pub wheel: WheelGrade,
    /// Per-channel curve remaps; missing channels inherit.
    pub curve: CurveGrade,
    /// HSL-qualified lightness shift; missing keys inherit.
    pub hsl: HslGrade,
    /// Corner darkening; missing knobs inherit.
    pub vignette: VignetteGrade,
    /// Seeded grain; missing knobs inherit.
    pub grain: GrainGrade,
    /// Imported `.cube` file and blend; missing knobs inherit.
    pub lut: LutGrade,
}

impl Grade {
    /// A grade level carrying nothing: every piece inherits outward,
    /// so layering it changes nothing.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when no piece is authored at this level.
    pub fn is_empty(&self) -> bool {
        self.preset.is_none()
            && self.exposure.is_none()
            && self.contrast.is_none()
            && self.saturation.is_none()
            && self.temperature.is_none()
            && self.tint.is_none()
            && self.wheel.is_empty()
            && self.curve.is_empty()
            && self.hsl.is_empty()
            && self.vignette.is_empty()
            && self.grain.is_empty()
            && self.lut.is_empty()
    }
}

/// One lane-group grade (Sprint 11 grade, CAP-001/T001): a `lane`
/// element naming one track whose clips all inherit `grade` between
/// the composition grade (outer) and their own clip grade (inner).
#[derive(Debug, Clone, PartialEq)]
pub struct LaneGrade {
    pub track: TrackKind,
    pub grade: Grade,
}

/// One resolved `wheel` stage (Sprint 11 grade, CAP-001/T002): the
/// inward-out merge of every level's `wheel` bands. All zeros is the
/// identity: no tonal lift anywhere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedWheel {
    /// Lift of dark tones.
    pub shadows: f64,
    /// Lift of middle tones.
    pub midtones: f64,
    /// Lift of bright tones.
    pub highlights: f64,
}

impl ResolvedWheel {
    /// The identity stage: no lift.
    pub fn identity() -> Self {
        Self {
            shadows: 0.0,
            midtones: 0.0,
            highlights: 0.0,
        }
    }

    /// True when no band lifts.
    pub fn is_empty(&self) -> bool {
        self.shadows == 0.0 && self.midtones == 0.0 && self.highlights == 0.0
    }

    /// Layer one authored stage: every band the level authors wins;
    /// every missing band keeps the base value.
    pub fn layer(&self, wheel: &WheelGrade) -> Self {
        Self {
            shadows: wheel.shadows.unwrap_or(self.shadows),
            midtones: wheel.midtones.unwrap_or(self.midtones),
            highlights: wheel.highlights.unwrap_or(self.highlights),
        }
    }
}

/// One resolved `curve` stage (Sprint 11 grade, CAP-001/T002): the
/// inward-out merge of every level's `curve` channels. An inner level
/// replaces a whole channel list; a missing channel inherits it. Empty
/// lists everywhere are the identity: channels pass through.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCurve {
    /// Red-channel points.
    pub red: Vec<(f64, f64)>,
    /// Green-channel points.
    pub green: Vec<(f64, f64)>,
    /// Blue-channel points.
    pub blue: Vec<(f64, f64)>,
    /// All-channel points, applied first.
    pub master: Vec<(f64, f64)>,
}

impl ResolvedCurve {
    /// The identity stage: no remap.
    pub fn identity() -> Self {
        Self {
            red: Vec::new(),
            green: Vec::new(),
            blue: Vec::new(),
            master: Vec::new(),
        }
    }

    /// True when no channel remaps.
    pub fn is_empty(&self) -> bool {
        self.red.is_empty() && self.green.is_empty() && self.blue.is_empty() && self.master.is_empty()
    }

    /// Layer one authored stage: every channel list the level authors
    /// replaces the base list; every missing channel keeps it.
    pub fn layer(&self, curve: &CurveGrade) -> Self {
        Self {
            red: curve.red.clone().unwrap_or_else(|| self.red.clone()),
            green: curve.green.clone().unwrap_or_else(|| self.green.clone()),
            blue: curve.blue.clone().unwrap_or_else(|| self.blue.clone()),
            master: curve.master.clone().unwrap_or_else(|| self.master.clone()),
        }
    }
}

/// One resolved `hsl` stage (Sprint 11 grade, CAP-001/T002): the
/// inward-out merge of every level's `hsl` keys. A zero `lightness` is
/// the identity whatever the selection holds: nothing shifts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedHsl {
    /// Hue window start in degrees.
    pub hue_min: f64,
    /// Hue window end in degrees.
    pub hue_max: f64,
    /// Saturation selection floor.
    pub saturation: f64,
    /// Lightness shift.
    pub lightness: f64,
}

impl ResolvedHsl {
    /// The identity stage: the whole wheel qualifies, nothing shifts.
    pub fn identity() -> Self {
        Self {
            hue_min: 0.0,
            hue_max: 360.0,
            saturation: 0.0,
            lightness: 0.0,
        }
    }

    /// True when no lightness shifts.
    pub fn is_empty(&self) -> bool {
        self.lightness == 0.0
    }

    /// Layer one authored stage: every key the level authors wins;
    /// every missing key keeps the base value.
    pub fn layer(&self, hsl: &HslGrade) -> Self {
        Self {
            hue_min: hsl.hue_min.unwrap_or(self.hue_min),
            hue_max: hsl.hue_max.unwrap_or(self.hue_max),
            saturation: hsl.saturation.unwrap_or(self.saturation),
            lightness: hsl.lightness.unwrap_or(self.lightness),
        }
    }
}

/// One resolved `vignette` stage (Sprint 11 grade, CAP-001/T002): the
/// inward-out merge of every level's `vignette` knobs. A zero `amount`
/// is the identity whatever `softness` holds: corners keep their light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedVignette {
    /// Corner darkening strength.
    pub amount: f64,
    /// Edge falloff width.
    pub softness: f64,
}

impl ResolvedVignette {
    /// The identity stage: no darkening.
    pub fn identity() -> Self {
        Self {
            amount: 0.0,
            softness: GRADE_DEFAULT_VIGNETTE_SOFTNESS,
        }
    }

    /// True when corners keep their light.
    pub fn is_empty(&self) -> bool {
        self.amount == 0.0
    }

    /// Layer one authored stage: every knob the level authors wins;
    /// every missing knob keeps the base value.
    pub fn layer(&self, vignette: &VignetteGrade) -> Self {
        Self {
            amount: vignette.amount.unwrap_or(self.amount),
            softness: vignette.softness.unwrap_or(self.softness),
        }
    }
}

/// One resolved `grain` stage (Sprint 11 grade, CAP-001/T002): the
/// inward-out merge of every level's `grain` knobs. A zero `amount` is
/// the identity whatever `size` or `seed` hold: pixels stay clean.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedGrain {
    /// Visibility amount.
    pub amount: f64,
    /// Cell size in pixels.
    pub size: u32,
    /// Pattern seed.
    pub seed: i64,
}

impl ResolvedGrain {
    /// The identity stage: no grain (default seed and cell size stand
    /// by for the first authored amount).
    pub fn identity() -> Self {
        Self {
            amount: 0.0,
            size: GRADE_DEFAULT_GRAIN_SIZE,
            seed: GRADE_DEFAULT_GRAIN_SEED,
        }
    }

    /// True when pixels stay clean.
    pub fn is_empty(&self) -> bool {
        self.amount == 0.0
    }

    /// Layer one authored stage: every knob the level authors wins;
    /// every missing knob keeps the base value.
    pub fn layer(&self, grain: &GrainGrade) -> Self {
        Self {
            amount: grain.amount.unwrap_or(self.amount),
            size: grain.size.unwrap_or(self.size),
            seed: grain.seed.unwrap_or(self.seed),
        }
    }
}

/// One resolved `lut` stage (Sprint 11 grade, CAP-002/T003): the
/// inward-out merge of every level's `lut` keys. A missing `src`
/// (or a zero `amount`) is the identity whatever the other knob
/// holds: the pipeline passes through. The parsed `grid`
/// materializes at frame time from the resolved `src`; an unloaded
/// grid (`None`) also passes through, so only the render resolve
/// step (which loads the grid before drawing) applies the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLut {
    /// `.cube` file path; none means no import.
    pub src: Option<String>,
    /// Blend amount (full by default).
    pub amount: f64,
    /// Parsed grid; loaded at frame time from `src`.
    pub grid: Option<LutTable>,
}

impl ResolvedLut {
    /// The identity stage: no import (full amount stands by for the
    /// first authored `src`).
    pub fn identity() -> Self {
        Self {
            src: None,
            amount: 1.0,
            grid: None,
        }
    }

    /// True when the pipeline passes through (no file, or a zero
    /// blend: pixel-identical to the `lut` removed).
    pub fn is_empty(&self) -> bool {
        self.src.is_none() || self.amount == 0.0
    }

    /// Layer one authored stage: every key the level authors wins; a
    /// missing key keeps the base value. The grid never layers: it
    /// always materializes fresh from the resolved `src` at frame
    /// time.
    pub fn layer(&self, lut: &LutGrade) -> Self {
        Self {
            src: lut.src.clone().or_else(|| self.src.clone()),
            amount: lut.amount.unwrap_or(self.amount),
            grid: None,
        }
    }
}

/// One fully resolved per-clip look (Sprint 11 grade,
/// CAP-001/CAP-002, T001+T002+T003): the inward-out merge
/// (composition, then lane, then clip) with every missing piece
/// inherited. `preset=None` plus all-zero adjusts plus empty detail
/// stages is the identity: pixels pass through untouched. Detail
/// stages run after the T001 chain in the fixed spec order: wheels,
/// curves, HSL, LUT, vignette, grain.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGrade {
    pub preset: GradePreset,
    pub exposure: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub temperature: f64,
    pub tint: f64,
    /// Color-wheel lifts per tonal zone.
    pub wheel: ResolvedWheel,
    /// Per-channel curve remaps.
    pub curve: ResolvedCurve,
    /// HSL-qualified lightness shift.
    pub hsl: ResolvedHsl,
    /// Imported `.cube` file and blend.
    pub lut: ResolvedLut,
    /// Corner darkening.
    pub vignette: ResolvedVignette,
    /// Seeded grain.
    pub grain: ResolvedGrain,
}

impl ResolvedGrade {
    /// The identity look: no preset, no shifts, no detail.
    pub fn identity() -> Self {
        Self {
            preset: GradePreset::None,
            exposure: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature: 0.0,
            tint: 0.0,
            wheel: ResolvedWheel::identity(),
            curve: ResolvedCurve::identity(),
            hsl: ResolvedHsl::identity(),
            lut: ResolvedLut::identity(),
            vignette: ResolvedVignette::identity(),
            grain: ResolvedGrain::identity(),
        }
    }

    /// True when the look passes pixels through untouched.
    pub fn is_empty(&self) -> bool {
        self.preset == GradePreset::None
            && self.exposure == 0.0
            && self.contrast == 0.0
            && self.saturation == 0.0
            && self.temperature == 0.0
            && self.tint == 0.0
            && self.wheel.is_empty()
            && self.curve.is_empty()
            && self.hsl.is_empty()
            && self.lut.is_empty()
            && self.vignette.is_empty()
            && self.grain.is_empty()
    }

    /// True when the look varies across the frame (corner darkening or
    /// grain): the background cannot be a flat fill and must paint
    /// per pixel.
    pub fn is_spatial(&self) -> bool {
        !self.vignette.is_empty() || !self.grain.is_empty()
    }

    /// Layer one authored level over this resolved base: every piece
    /// the level authors wins (including an explicit `preset="none"`,
    /// which clears the preset stage, and any authored detail piece);
    /// every missing piece keeps the base value.
    pub fn layer(&self, grade: &Grade) -> Self {
        Self {
            preset: grade.preset.unwrap_or(self.preset),
            exposure: grade.exposure.unwrap_or(self.exposure),
            contrast: grade.contrast.unwrap_or(self.contrast),
            saturation: grade.saturation.unwrap_or(self.saturation),
            temperature: grade.temperature.unwrap_or(self.temperature),
            tint: grade.tint.unwrap_or(self.tint),
            wheel: self.wheel.layer(&grade.wheel),
            curve: self.curve.layer(&grade.curve),
            hsl: self.hsl.layer(&grade.hsl),
            lut: self.lut.layer(&grade.lut),
            vignette: self.vignette.layer(&grade.vignette),
            grain: self.grain.layer(&grade.grain),
        }
    }

    /// [`ResolvedGrade::layer`] with an optional level (a missing lane
    /// grade, for example): `None` keeps the base unchanged.
    pub fn layer_opt(&self, grade: Option<&Grade>) -> Self {
        match grade {
            Some(g) => self.layer(g),
            None => self.clone(),
        }
    }

    /// Compact fingerprint of the look for frame descriptions.
    pub fn fingerprint(&self) -> String {
        fn curve_fp(pts: &[(f64, f64)]) -> String {
            pts.iter()
                .map(|(x, y)| format!("{x:.6},{y:.6}"))
                .collect::<Vec<_>>()
                .join(";")
        }
        format!(
            "{}:{:.6}:{:.6}:{:.6}:{:.6}:{:.6}|w:{:.6}:{:.6}:{:.6}|c:{}:{}:{}:{}|h:{:.6}:{:.6}:{:.6}:{:.6}|l:{}:{:.6}|v:{:.6}:{:.6}|g:{:.6}:{}:{}",
            self.preset.name(),
            self.exposure,
            self.contrast,
            self.saturation,
            self.temperature,
            self.tint,
            self.wheel.shadows,
            self.wheel.midtones,
            self.wheel.highlights,
            curve_fp(&self.curve.red),
            curve_fp(&self.curve.green),
            curve_fp(&self.curve.blue),
            curve_fp(&self.curve.master),
            self.hsl.hue_min,
            self.hsl.hue_max,
            self.hsl.saturation,
            self.hsl.lightness,
            self.lut.src.as_deref().unwrap_or(""),
            self.lut.amount,
            self.vignette.amount,
            self.vignette.softness,
            self.grain.amount,
            self.grain.size,
            self.grain.seed,
        )
    }

    /// Apply the resolved look to one straight-RGB pixel with channels
    /// in 0..255, returning the graded channels in 0..255. This is the
    /// position-independent chain (preset, exposure, contrast,
    /// saturation, temperature, tint, wheels, curves, HSL, LUT);
    /// render paths use [`ResolvedGrade::apply_at`] so the spatial
    /// stages (vignette, grain) see the pixel position. Pure
    /// closed-form math plus table lookup, no RNG, so identical
    /// inputs grade byte-identically. A LUT stage without a loaded
    /// grid passes through; the render resolve step loads the grid
    /// before drawing.
    pub fn apply(&self, r: f64, g: f64, b: f64) -> (f64, f64, f64) {
        // Work normalized; clamp once at the end.
        let mut rgb = self.apply_base([r / 255.0, g / 255.0, b / 255.0]);
        rgb = self.apply_wheels(rgb);
        rgb = self.apply_curves(rgb);
        rgb = self.apply_hsl(rgb);
        rgb = self.apply_lut(rgb);
        (
            (rgb[0] * 255.0).clamp(0.0, 255.0),
            (rgb[1] * 255.0).clamp(0.0, 255.0),
            (rgb[2] * 255.0).clamp(0.0, 255.0),
        )
    }

    /// Apply the full resolved look to one straight-RGB pixel with
    /// channels in 0..255 at frame position (`x`, `y`) on a `w` by `h`
    /// canvas, returning the graded channels in 0..255. Stages run in
    /// the fixed spec order: preset, exposure, contrast, saturation,
    /// temperature, tint, wheels, curves, HSL, LUT, vignette, grain.
    /// Pure closed-form math plus table lookup and an integer grain
    /// hash, no RNG state, so identical inputs grade
    /// byte-identically.
    pub fn apply_at(
        &self,
        r: f64,
        g: f64,
        b: f64,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> (f64, f64, f64) {
        // Work normalized; clamp once at the end.
        let mut rgb = self.apply_base([r / 255.0, g / 255.0, b / 255.0]);
        rgb = self.apply_wheels(rgb);
        rgb = self.apply_curves(rgb);
        rgb = self.apply_hsl(rgb);
        rgb = self.apply_lut(rgb);
        rgb = self.apply_vignette(rgb, x, y, w, h);
        rgb = self.apply_grain(rgb, x, y, w, h);
        (
            (rgb[0] * 255.0).clamp(0.0, 255.0),
            (rgb[1] * 255.0).clamp(0.0, 255.0),
            (rgb[2] * 255.0).clamp(0.0, 255.0),
        )
    }

    /// LUT import on a normalized pixel: the grid remaps the pixel
    /// through trilinear lookup, then `amount` blends between the
    /// unmapped pipeline output (amount 0) and the fully mapped one
    /// (amount 1). A missing file, a zero amount, or an unloaded grid
    /// passes through untouched.
    fn apply_lut(&self, rgb: [f64; 3]) -> [f64; 3] {
        let amount = self.lut.amount;
        if amount == 0.0 {
            return rgb;
        }
        let Some(grid) = &self.lut.grid else {
            return rgb;
        };
        let hit = grid.lookup(rgb[0], rgb[1], rgb[2]);
        [
            rgb[0] + (hit[0] - rgb[0]) * amount,
            rgb[1] + (hit[1] - rgb[1]) * amount,
            rgb[2] + (hit[2] - rgb[2]) * amount,
        ]
    }

    /// The position-independent T001 prefix on a normalized (0..1)
    /// pixel: preset, exposure, contrast, saturation, temperature,
    /// tint. Both [`ResolvedGrade::apply`] and
    /// [`ResolvedGrade::apply_at`] run this first with the same op
    /// order, so ungraded detail keeps T001 pixels bit-exact.
    fn apply_base(&self, mut rgb: [f64; 3]) -> [f64; 3] {
        rgb = Self::apply_preset(self.preset, rgb);
        if self.exposure != 0.0 {
            let m = 2f64.powf(self.exposure);
            rgb = [rgb[0] * m, rgb[1] * m, rgb[2] * m];
        }
        if self.contrast != 0.0 {
            let m = 1.0 + self.contrast;
            rgb = [
                (rgb[0] - 0.5) * m + 0.5,
                (rgb[1] - 0.5) * m + 0.5,
                (rgb[2] - 0.5) * m + 0.5,
            ];
        }
        if self.saturation != 0.0 {
            let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            let m = 1.0 + self.saturation;
            rgb = [
                l + (rgb[0] - l) * m,
                l + (rgb[1] - l) * m,
                l + (rgb[2] - l) * m,
            ];
        }
        if self.temperature != 0.0 {
            let t = self.temperature;
            rgb = [rgb[0] * (1.0 + 0.20 * t), rgb[1], rgb[2] * (1.0 - 0.20 * t)];
        }
        if self.tint != 0.0 {
            let n = self.tint;
            rgb = [
                rgb[0] * (1.0 + 0.08 * n),
                rgb[1] * (1.0 - 0.10 * n),
                rgb[2] * (1.0 + 0.08 * n),
            ];
        }
        rgb
    }

    /// Wheels on a normalized pixel: each band lifts all three channels
    /// by its value weighted for its zone. Shadows weigh `(1-2L)^2`
    /// below middle gray (exactly zero at and above it), highlights
    /// weigh `(2L-1)^2` above middle gray (exactly zero at and below
    /// it), midtones take the bell remainder (peaking at middle gray,
    /// exactly zero at pure black and white), so a band never moves
    /// the far zones.
    fn apply_wheels(&self, rgb: [f64; 3]) -> [f64; 3] {
        let (s, m, h) = (
            self.wheel.shadows,
            self.wheel.midtones,
            self.wheel.highlights,
        );
        if s == 0.0 && m == 0.0 && h == 0.0 {
            return rgb;
        }
        let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        let ws = (1.0 - l / 0.5).clamp(0.0, 1.0);
        let ws = ws * ws;
        let wh = ((l - 0.5) / 0.5).clamp(0.0, 1.0);
        let wh = wh * wh;
        let wm = (1.0 - ws - wh).max(0.0);
        let lift = s * ws + m * wm + h * wh;
        [rgb[0] + lift, rgb[1] + lift, rgb[2] + lift]
    }

    /// Curves on a normalized pixel: `master` remaps every channel
    /// first, then each per-channel list remaps its own channel through
    /// piecewise-linear segments. Empty lists pass through.
    fn apply_curves(&self, rgb: [f64; 3]) -> [f64; 3] {
        if self.curve.is_empty() {
            return rgb;
        }
        let mut out = rgb;
        if !self.curve.master.is_empty() {
            out = [
                Self::remap_channel(&self.curve.master, out[0]),
                Self::remap_channel(&self.curve.master, out[1]),
                Self::remap_channel(&self.curve.master, out[2]),
            ];
        }
        if !self.curve.red.is_empty() {
            out[0] = Self::remap_channel(&self.curve.red, out[0]);
        }
        if !self.curve.green.is_empty() {
            out[1] = Self::remap_channel(&self.curve.green, out[1]);
        }
        if !self.curve.blue.is_empty() {
            out[2] = Self::remap_channel(&self.curve.blue, out[2]);
        }
        out
    }

    /// One piecewise-linear remap: inputs below the first point take
    /// its output, inputs above the last take its output, and inputs
    /// between take the lerped segment. Points arrive validated
    /// (rising `x`, every number in 0..1).
    fn remap_channel(pts: &[(f64, f64)], x: f64) -> f64 {
        if x <= pts[0].0 {
            return pts[0].1;
        }
        let last = pts.len() - 1;
        if x >= pts[last].0 {
            return pts[last].1;
        }
        // First segment whose end sits past `x`.
        let mut lo = 0;
        let mut hi = last;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if pts[mid].0 <= x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (x0, y0) = pts[lo];
        let (x1, y1) = pts[hi];
        y0 + (x - x0) * (y1 - y0) / (x1 - x0)
    }

    /// HSL selection on a normalized pixel: colors inside the hue
    /// window (wrapping past 360 when min exceeds max) whose saturation
    /// reaches the floor take the lightness shift in HSL space; every
    /// other pixel passes through exactly.
    fn apply_hsl(&self, rgb: [f64; 3]) -> [f64; 3] {
        if self.hsl.is_empty() {
            return rgb;
        }
        let (h, s, l) = Self::rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
        let in_hue = if self.hsl.hue_min <= self.hsl.hue_max {
            h >= self.hsl.hue_min && h <= self.hsl.hue_max
        } else {
            h >= self.hsl.hue_min || h <= self.hsl.hue_max
        };
        if !in_hue || s < self.hsl.saturation {
            return rgb;
        }
        let (r, g, b) = Self::hsl_to_rgb(h, s, (l + self.hsl.lightness).clamp(0.0, 1.0));
        [r, g, b]
    }

    /// RGB (0..1) to HSL (hue degrees 0..360, saturation and lightness
    /// 0..1). Achromatic pixels report hue 0 deterministically.
    fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if max == min {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let mut h = if max == r {
            ((g - b) / d) % 6.0
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
        if h < 0.0 {
            h += 360.0;
        }
        (h, s, l)
    }

    /// HSL back to RGB (0..1).
    fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
        if s == 0.0 {
            return (l, l, l);
        }
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        (r + m, g + m, b + m)
    }

    /// Vignette on a normalized pixel: corners darken toward black by
    /// `amount` through a smoothstep falloff whose inner edge sits at
    /// `1 - softness` of the center-to-corner distance. The falloff
    /// reaches exactly zero at and inside the inner edge, so the center
    /// never moves; a zero amount skips untouched.
    fn apply_vignette(&self, rgb: [f64; 3], x: f64, y: f64, w: f64, h: f64) -> [f64; 3] {
        let amount = self.vignette.amount;
        if amount == 0.0 {
            return rgb;
        }
        if !(w > 0.0) || !(h > 0.0) || !x.is_finite() || !y.is_finite() {
            return rgb;
        }
        let (cx, cy) = (w / 2.0, h / 2.0);
        let max_d = cx.hypot(cy);
        if !(max_d > 0.0) {
            return rgb;
        }
        let d = (x + 0.5 - cx).hypot(y + 0.5 - cy) / max_d;
        let e0 = 1.0 - self.vignette.softness.clamp(0.0, 1.0);
        let t = if e0 >= 1.0 {
            if d >= 1.0 { 1.0 } else { 0.0 }
        } else {
            let t = ((d - e0) / (1.0 - e0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let f = 1.0 - amount * t;
        [rgb[0] * f, rgb[1] * f, rgb[2] * f]
    }

    /// Seeded grain on a normalized pixel: one deterministic FNV-1a
    /// hash of (`seed`, grain cell) picks a centered value scaled by
    /// `amount`, added achromatically to every channel. Pure integer
    /// hashing, no RNG state, so the same seed re-renders
    /// byte-identically and different seeds pattern differently. A zero
    /// amount skips untouched.
    fn apply_grain(&self, rgb: [f64; 3], x: f64, y: f64, w: f64, h: f64) -> [f64; 3] {
        let amount = self.grain.amount;
        if amount == 0.0 {
            return rgb;
        }
        if !x.is_finite() || !y.is_finite() || !(w > 0.0) || !(h > 0.0) {
            return rgb;
        }
        let size = self.grain.size.max(1) as f64;
        let cx = (x / size).floor() as i64;
        let cy = (y / size).floor() as i64;
        let n = Self::grain_value(self.grain.seed, cx, cy);
        let d = amount * (GRADE_GRAIN_PEAK / 255.0) * n;
        [rgb[0] + d, rgb[1] + d, rgb[2] + d]
    }

    /// One deterministic grain value in [-1, 1) for a (`seed`, cell)
    /// pair: FNV-1a over the little-endian bytes, top 53 bits scaled to
    /// [0, 1), then centered. Wrapping arithmetic throughout, so debug
    /// and release builds agree bit-exactly.
    fn grain_value(seed: i64, cx: i64, cy: i64) -> f64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for word in [seed as u64, cx as u64, cy as u64] {
            for byte in word.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        let f = ((h >> 11) as f64) * (1.0 / 9007199254740992.0);
        f * 2.0 - 1.0
    }

    /// One preset's base look on a normalized (0..1) pixel.
    fn apply_preset(preset: GradePreset, rgb: [f64; 3]) -> [f64; 3] {
        // Smooth S-curve through (0,0) and (1,1) for filmic looks.
        fn s_curve(x: f64) -> f64 {
            let t = x.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        }
        fn luma(rgb: [f64; 3]) -> f64 {
            0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
        }
        fn desat(rgb: [f64; 3], keep: f64) -> [f64; 3] {
            let l = luma(rgb);
            [
                l + (rgb[0] - l) * keep,
                l + (rgb[1] - l) * keep,
                l + (rgb[2] - l) * keep,
            ]
        }
        match preset {
            GradePreset::None => rgb,
            GradePreset::Filmic => {
                let d = desat(rgb, 0.9);
                [
                    d[0] + (s_curve(d[0]) - d[0]) * 0.30,
                    d[1] + (s_curve(d[1]) - d[1]) * 0.30,
                    d[2] + (s_curve(d[2]) - d[2]) * 0.30,
                ]
            }
            GradePreset::Noir => {
                let l = luma(rgb);
                let s = l + (s_curve(l) - l) * 0.5;
                [s, s, s]
            }
            GradePreset::TealOrange => {
                let l = luma(rgb).clamp(0.0, 1.0);
                if l < 0.5 {
                    let d = 0.5 - l;
                    [rgb[0] - d * 0.24, rgb[1] + d * 0.10, rgb[2] + d * 0.16]
                } else {
                    let d = l - 0.5;
                    [rgb[0] + d * 0.22, rgb[1] + d * 0.10, rgb[2] - d * 0.18]
                }
            }
            GradePreset::Pastel => {
                let lifted = [rgb[0] * 0.82 + 0.10, rgb[1] * 0.82 + 0.10, rgb[2] * 0.82 + 0.10];
                desat(lifted, 0.7)
            }
            GradePreset::Bleach => {
                let bright = [
                    (rgb[0] - 0.5) * 1.3 + 0.56,
                    (rgb[1] - 0.5) * 1.3 + 0.56,
                    (rgb[2] - 0.5) * 1.3 + 0.56,
                ];
                desat(bright, 0.45)
            }
        }
    }
}

/// One named media effect preset (Sprint 11 effects, CAP-003/T004):
/// the stylistic vocabulary (`effect preset="..."`) applied after the
/// grade chain in document order. Only these eighteen spellings are
/// valid; anything else is refused, never guessed into a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectPreset {
    Grayscale,
    Sepia,
    Invert,
    Brightness,
    Contrast,
    HueRotate,
    Saturate,
    Blur,
    Sharpen,
    Vignette,
    Grain,
    Noise,
    Pixelate,
    Posterize,
    Threshold,
    EdgeDetect,
    Emboss,
    ChromaticAberration,
}

impl EffectPreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grayscale" => Some(EffectPreset::Grayscale),
            "sepia" => Some(EffectPreset::Sepia),
            "invert" => Some(EffectPreset::Invert),
            "brightness" => Some(EffectPreset::Brightness),
            "contrast" => Some(EffectPreset::Contrast),
            "hue-rotate" => Some(EffectPreset::HueRotate),
            "saturate" => Some(EffectPreset::Saturate),
            "blur" => Some(EffectPreset::Blur),
            "sharpen" => Some(EffectPreset::Sharpen),
            "vignette" => Some(EffectPreset::Vignette),
            "grain" => Some(EffectPreset::Grain),
            "noise" => Some(EffectPreset::Noise),
            "pixelate" => Some(EffectPreset::Pixelate),
            "posterize" => Some(EffectPreset::Posterize),
            "threshold" => Some(EffectPreset::Threshold),
            "edge-detect" => Some(EffectPreset::EdgeDetect),
            "emboss" => Some(EffectPreset::Emboss),
            "chromatic-aberration" => Some(EffectPreset::ChromaticAberration),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EffectPreset::Grayscale => "grayscale",
            EffectPreset::Sepia => "sepia",
            EffectPreset::Invert => "invert",
            EffectPreset::Brightness => "brightness",
            EffectPreset::Contrast => "contrast",
            EffectPreset::HueRotate => "hue-rotate",
            EffectPreset::Saturate => "saturate",
            EffectPreset::Blur => "blur",
            EffectPreset::Sharpen => "sharpen",
            EffectPreset::Vignette => "vignette",
            EffectPreset::Grain => "grain",
            EffectPreset::Noise => "noise",
            EffectPreset::Pixelate => "pixelate",
            EffectPreset::Posterize => "posterize",
            EffectPreset::Threshold => "threshold",
            EffectPreset::EdgeDetect => "edge-detect",
            EffectPreset::Emboss => "emboss",
            EffectPreset::ChromaticAberration => "chromatic-aberration",
        }
    }

    /// Accepted spellings, in spec order.
    pub const ACCEPTED: [&'static str; 18] = [
        "grayscale",
        "sepia",
        "invert",
        "brightness",
        "contrast",
        "hue-rotate",
        "saturate",
        "blur",
        "sharpen",
        "vignette",
        "grain",
        "noise",
        "pixelate",
        "posterize",
        "threshold",
        "edge-detect",
        "emboss",
        "chromatic-aberration",
    ];

    /// Keys this preset reads besides `preset` and `amount` (Sprint 11
    /// effects, CAP-003/T004): every other known effect key on this
    /// preset is refused naming the preset and the expected shape.
    pub fn allowed_keys(self) -> &'static [&'static str] {
        match self {
            EffectPreset::Grayscale | EffectPreset::Sepia | EffectPreset::Invert => &[],
            EffectPreset::Brightness => &["brightness"],
            EffectPreset::Contrast => &["contrast"],
            EffectPreset::Saturate => &["saturate"],
            EffectPreset::HueRotate => &["angle"],
            EffectPreset::Blur => &["radius"],
            EffectPreset::Sharpen => &["strength"],
            EffectPreset::Vignette => &["softness"],
            EffectPreset::Grain => &["size", "seed"],
            EffectPreset::Noise => &["seed"],
            EffectPreset::Pixelate => &["size"],
            EffectPreset::Posterize => &["levels"],
            EffectPreset::Threshold => &["threshold"],
            EffectPreset::EdgeDetect => &["strength"],
            EffectPreset::Emboss => &["strength"],
            EffectPreset::ChromaticAberration => &["distance"],
        }
    }
}

/// Accepted `effect` ranges (Sprint 11 effects, CAP-003/T004): values
/// outside these fail validation naming the preset and the range,
/// never clamp silently. Key names follow the CSS vocabulary
/// (`brightness`, `contrast`, `saturate`, `angle` for `hue-rotate`,
/// `radius` for `blur`, `softness`, `size`, `seed`, `levels`,
/// `threshold`, `distance`).
pub const EFFECT_AMOUNT_RANGE: (f64, f64) = (0.0, 1.0);
pub const EFFECT_BRIGHTNESS_RANGE: (f64, f64) = (0.0, 4.0);
pub const EFFECT_CONTRAST_RANGE: (f64, f64) = (0.0, 4.0);
pub const EFFECT_SATURATE_RANGE: (f64, f64) = (0.0, 4.0);
pub const EFFECT_HUE_ANGLE_RANGE: (f64, f64) = (0.0, 360.0);
pub const EFFECT_BLUR_RADIUS_RANGE: (f64, f64) = (0.0, 64.0);
pub const EFFECT_STRENGTH_RANGE: (f64, f64) = (0.0, 4.0);
pub const EFFECT_VIGNETTE_SOFTNESS_RANGE: (f64, f64) = (0.0, 1.0);
pub const EFFECT_PIXEL_SIZE_RANGE: (i64, i64) = (1, 64);
pub const EFFECT_SEED_DEFAULT: i64 = 0;
pub const EFFECT_POSTERIZE_LEVELS_RANGE: (i64, i64) = (2, 16);
pub const EFFECT_THRESHOLD_RANGE: (f64, f64) = (0.0, 1.0);
pub const EFFECT_CHROMA_DISTANCE_RANGE: (f64, f64) = (0.0, 32.0);

/// Documented defaults for unmentioned preset parameters (Sprint 11
/// effects, CAP-003/T004).
pub const EFFECT_DEFAULT_AMOUNT: f64 = 1.0;
pub const EFFECT_DEFAULT_BRIGHTNESS: f64 = 1.3;
pub const EFFECT_DEFAULT_CONTRAST: f64 = 1.3;
pub const EFFECT_DEFAULT_SATURATE: f64 = 2.0;
pub const EFFECT_DEFAULT_HUE_ANGLE: f64 = 90.0;
pub const EFFECT_DEFAULT_BLUR_RADIUS: f64 = 4.0;
pub const EFFECT_DEFAULT_STRENGTH: f64 = 1.0;
pub const EFFECT_DEFAULT_VIGNETTE_SOFTNESS: f64 = GRADE_DEFAULT_VIGNETTE_SOFTNESS;
pub const EFFECT_DEFAULT_GRAIN_SIZE: u32 = GRADE_DEFAULT_GRAIN_SIZE;
pub const EFFECT_DEFAULT_PIXEL_SIZE: u32 = 8;
pub const EFFECT_DEFAULT_POSTERIZE_LEVELS: u32 = 4;
pub const EFFECT_DEFAULT_THRESHOLD: f64 = 0.5;
pub const EFFECT_DEFAULT_CHROMA_DISTANCE: f64 = 2.0;

/// Peak channel swing of full-amount effect grain and noise in 0..255
/// levels (Sprint 11 effects, CAP-003/T004): the same swing as grade
/// grain, so the two match at full visibility.
pub const EFFECT_GRAIN_PEAK: f64 = GRADE_GRAIN_PEAK;

/// Every known `effect` key (Sprint 11 effects, CAP-003/T004): `preset`
/// plus `amount` plus each preset's parameters. Anything else on an
/// `effect` is refused as an unknown key, never guessed.
pub const EFFECT_KEYS: [&str; 14] = [
    "preset",
    "amount",
    "brightness",
    "contrast",
    "saturate",
    "angle",
    "radius",
    "strength",
    "softness",
    "size",
    "seed",
    "levels",
    "threshold",
    "distance",
];

/// Preset-specific parameters of one `effect` (Sprint 11 effects,
/// CAP-003/T004): every field is optional and a missing field renders
/// at its documented default. Only the authoring preset's own keys
/// are ever set; any other key on the preset is refused at check
/// time, so render never reads a foreign field.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectParams {
    /// `brightness` preset factor; none means the default.
    pub brightness: Option<f64>,
    /// `contrast` preset factor; none means the default.
    pub contrast: Option<f64>,
    /// `saturate` preset factor; none means the default.
    pub saturate: Option<f64>,
    /// `hue-rotate` preset angle in degrees; none means the default.
    pub angle: Option<f64>,
    /// `blur` preset radius in canvas pixels; none means the default.
    pub radius: Option<f64>,
    /// `sharpen`, `edge-detect`, and `emboss` preset strength; none
    /// means the default.
    pub strength: Option<f64>,
    /// `vignette` preset edge falloff; none means the default.
    pub softness: Option<f64>,
    /// `grain` and `pixelate` preset cell size in pixels; none means
    /// the default.
    pub size: Option<u32>,
    /// `grain` and `noise` preset pattern seed; none means the
    /// default seed (fixed, never wall-clock randomness).
    pub seed: Option<i64>,
    /// `posterize` preset levels per channel; none means the default.
    pub levels: Option<u32>,
    /// `threshold` preset luminance cutoff; none means the default.
    pub threshold: Option<f64>,
    /// `chromatic-aberration` preset channel shift in canvas pixels;
    /// none means the default.
    pub distance: Option<f64>,
}

impl EffectParams {
    /// Factor of the `brightness` preset at full amount.
    pub fn brightness(&self) -> f64 {
        self.brightness.unwrap_or(EFFECT_DEFAULT_BRIGHTNESS)
    }
    /// Factor of the `contrast` preset at full amount.
    pub fn contrast(&self) -> f64 {
        self.contrast.unwrap_or(EFFECT_DEFAULT_CONTRAST)
    }
    /// Factor of the `saturate` preset at full amount.
    pub fn saturate(&self) -> f64 {
        self.saturate.unwrap_or(EFFECT_DEFAULT_SATURATE)
    }
    /// Angle of the `hue-rotate` preset at full amount, in degrees.
    pub fn angle(&self) -> f64 {
        self.angle.unwrap_or(EFFECT_DEFAULT_HUE_ANGLE)
    }
    /// Radius of the `blur` preset at full amount, in canvas pixels.
    pub fn radius(&self) -> f64 {
        self.radius.unwrap_or(EFFECT_DEFAULT_BLUR_RADIUS)
    }
    /// Strength of the `sharpen`, `edge-detect`, and `emboss`
    /// presets at full amount.
    pub fn strength(&self) -> f64 {
        self.strength.unwrap_or(EFFECT_DEFAULT_STRENGTH)
    }
    /// Edge falloff of the `vignette` preset at full amount.
    pub fn softness(&self) -> f64 {
        self.softness.unwrap_or(EFFECT_DEFAULT_VIGNETTE_SOFTNESS)
    }
    /// Cell size of the `grain` preset at full amount, in pixels.
    pub fn grain_size(&self) -> u32 {
        self.size.unwrap_or(EFFECT_DEFAULT_GRAIN_SIZE)
    }
    /// Pattern seed of the `grain` and `noise` presets.
    pub fn seed(&self) -> i64 {
        self.seed.unwrap_or(EFFECT_SEED_DEFAULT)
    }
    /// Block size of the `pixelate` preset at full amount, in pixels.
    pub fn pixel_size(&self) -> u32 {
        self.size.unwrap_or(EFFECT_DEFAULT_PIXEL_SIZE)
    }
    /// Levels per channel of the `posterize` preset at full amount.
    pub fn levels(&self) -> u32 {
        self.levels.unwrap_or(EFFECT_DEFAULT_POSTERIZE_LEVELS)
    }
    /// Luminance cutoff of the `threshold` preset at full amount.
    pub fn threshold(&self) -> f64 {
        self.threshold.unwrap_or(EFFECT_DEFAULT_THRESHOLD)
    }
    /// Channel shift of the `chromatic-aberration` preset at full
    /// amount, in canvas pixels.
    pub fn distance(&self) -> f64 {
        self.distance.unwrap_or(EFFECT_DEFAULT_CHROMA_DISTANCE)
    }
}

/// One authored `effect` child (Sprint 11 effects, CAP-003/T004): a
/// clip carries its effects in document order and they apply after
/// the grade chain in that same order. `amount` blends the
/// full-strength look (0 passes through pixel-identical to the effect
/// removed, 1 applies it fully); unmentioned parameters render at
/// their documented defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub preset: EffectPreset,
    /// Blend amount from 0 (pass-through) to 1 (full look).
    pub amount: f64,
    /// Preset-specific parameters (only the preset's own keys set).
    pub params: EffectParams,
}

impl Effect {
    /// True when the effect passes pixels through untouched: render
    /// skips it, so disabling an effect renders pixel-identical to
    /// the chain written without it.
    pub fn is_empty(&self) -> bool {
        self.amount == 0.0
    }

    /// Compact fingerprint of the effect for look descriptions.
    pub fn fingerprint(&self) -> String {
        fn num(x: Option<f64>) -> String {
            x.map(|v| format!("{v:.6}")).unwrap_or_default()
        }
        fn int(x: Option<i64>) -> String {
            x.map(|v| format!("{v}")).unwrap_or_default()
        }
        fn uint(x: Option<u32>) -> String {
            x.map(|v| format!("{v}")).unwrap_or_default()
        }
        format!(
            "{}:{:.6}:{}/{}/{}/{}/{}/{}/{}/{}/{}/{}/{}/{}",
            self.preset.name(),
            self.amount,
            num(self.params.brightness),
            num(self.params.contrast),
            num(self.params.saturate),
            num(self.params.angle),
            num(self.params.radius),
            num(self.params.strength),
            num(self.params.softness),
            uint(self.params.size),
            int(self.params.seed),
            uint(self.params.levels),
            num(self.params.threshold),
            num(self.params.distance),
        )
    }
}

/// Accepted `vfx` stage ranges (Sprint 11 VFX, CAP-004/T005): values
/// outside these fail validation naming the stage and the range,
/// never clamp silently. Key names follow the task vocabulary
/// (`amount`, `seed`, `grid`, `strength`, `map`, `scale`, `src`).
pub const VFX_AMOUNT_RANGE: (f64, f64) = (0.0, 1.0);
pub const VFX_WARP_GRID_RANGE: (i64, i64) = (1, 64);
pub const VFX_WARP_STRENGTH_RANGE: (f64, f64) = (0.0, 64.0);
pub const VFX_DISPLACEMENT_SCALE_RANGE: (f64, f64) = (0.0, 64.0);
pub const VFX_SEED_DEFAULT: i64 = 0;

/// Documented defaults for unmentioned VFX stage parameters
/// (Sprint 11 VFX, CAP-004/T005).
pub const VFX_DEFAULT_AMOUNT: f64 = 1.0;
pub const VFX_DEFAULT_WARP_GRID: u32 = 16;
pub const VFX_DEFAULT_WARP_STRENGTH: f64 = 8.0;
pub const VFX_DEFAULT_DISPLACEMENT_SCALE: f64 = 8.0;

/// Peak channel swing of full-amount VFX noise in 0..255 levels
/// (Sprint 11 VFX, CAP-004/T005): the same swing as effect grain
/// and noise, so the three match at full visibility.
pub const VFX_NOISE_PEAK: f64 = EFFECT_GRAIN_PEAK;

/// Accepted `vfx` stage names, in application order.
pub const VFX_STAGES: [&str; 4] = ["noise", "warp", "displacement", "luma-matte"];

/// One authored `noise` VFX stage (Sprint 11 VFX, CAP-004/T005):
/// achromatic per-pixel noise scaled by `amount`, patterned by
/// `seed` (absent means the fixed default seed, never wall-clock
/// randomness).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VfxNoise {
    pub amount: f64,
    pub seed: Option<i64>,
}

impl VfxNoise {
    pub fn seed(&self) -> i64 {
        self.seed.unwrap_or(VFX_SEED_DEFAULT)
    }

    /// True when the stage passes pixels through untouched: render
    /// skips it, so disabling the stage renders pixel-identical to
    /// the chain written without it.
    pub fn is_empty(&self) -> bool {
        self.amount == 0.0
    }
}

/// One authored `warp` VFX stage (Sprint 11 VFX, CAP-004/T005): a
/// smooth deterministic warp field over `grid`-pixel cells peaking at
/// `strength` canvas pixels of offset, blended by `amount`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VfxWarp {
    pub amount: f64,
    pub grid: Option<u32>,
    pub strength: Option<f64>,
}

impl VfxWarp {
    pub fn grid(&self) -> u32 {
        self.grid.unwrap_or(VFX_DEFAULT_WARP_GRID)
    }
    pub fn strength(&self) -> f64 {
        self.strength.unwrap_or(VFX_DEFAULT_WARP_STRENGTH)
    }

    /// True when the stage passes pixels through untouched: render
    /// skips it, so disabling the stage renders pixel-identical to
    /// the chain written without it.
    pub fn is_empty(&self) -> bool {
        self.amount == 0.0 || self.strength() == 0.0
    }
}

/// One authored `displacement` VFX stage (Sprint 11 VFX, CAP-004/T005):
/// the `map` picture's channels steer the offset (red steers x, green
/// steers y) peaking at `scale` canvas pixels, blended by `amount`.
/// An omitted `map` steers by the layer's own luma on both axes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VfxDisplacement {
    pub amount: f64,
    pub map: Option<String>,
    pub scale: Option<f64>,
}

impl VfxDisplacement {
    pub fn scale(&self) -> f64 {
        self.scale.unwrap_or(VFX_DEFAULT_DISPLACEMENT_SCALE)
    }

    /// True when the stage passes pixels through untouched: render
    /// skips it, so disabling the stage renders pixel-identical to
    /// the chain written without it.
    pub fn is_empty(&self) -> bool {
        self.amount == 0.0 || self.scale() == 0.0
    }
}

/// One authored `luma-matte` VFX stage (Sprint 11 VFX, CAP-004/T005):
/// the `src` picture's luma scales the layer alpha toward a cutout,
/// blended by `amount` (0 passes through pixel-identical to the
/// stage removed, 1 cuts fully to the matte).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VfxLumaMatte {
    pub amount: f64,
    pub src: String,
}

impl VfxLumaMatte {
    /// True when the stage passes pixels through untouched: render
    /// skips it, so disabling the stage renders pixel-identical to
    /// the chain written without it.
    pub fn is_empty(&self) -> bool {
        self.amount == 0.0
    }
}

/// One authored `vfx` child (Sprint 11 VFX, CAP-004/T005): a clip
/// carries at most one, holding each of the four stages at most
/// once. Stages apply after the effects stack in the fixed order
/// noise, warp, displacement, luma matte, regardless of document
/// order. A missing stage is skipped, never zeroed; an inert stage
/// (`amount` 0, or a zero `strength`/`scale`) renders
/// pixel-identical to the chain written without it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VfxChain {
    pub noise: Option<VfxNoise>,
    pub warp: Option<VfxWarp>,
    pub displacement: Option<VfxDisplacement>,
    pub luma_matte: Option<VfxLumaMatte>,
}

impl VfxChain {
    /// The empty chain: no stages, renders untouched.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when the chain passes pixels through untouched: render
    /// takes the fast path, so an inert chain renders pixel-identical
    /// to no chain at all.
    pub fn is_empty(&self) -> bool {
        let noise_empty = self.noise.as_ref().is_none_or(|s| s.is_empty());
        let warp_empty = self.warp.as_ref().is_none_or(|s| s.is_empty());
        let displacement_empty = self
            .displacement
            .as_ref()
            .is_none_or(|s| s.is_empty());
        let matte_empty = self.luma_matte.as_ref().is_none_or(|s| s.is_empty());
        noise_empty && warp_empty && displacement_empty && matte_empty
    }

    /// Merge an outer (nest-use) chain under this one (Sprint 11 VFX,
    /// CAP-004/T005): each stage the inner chain already defines wins,
    /// each missing stage inherits the outer one, so a merged chain
    /// still holds every stage at most once.
    pub fn layer_outer(&self, outer: &VfxChain) -> VfxChain {
        VfxChain {
            noise: self.noise.clone().or_else(|| outer.noise.clone()),
            warp: self.warp.clone().or_else(|| outer.warp.clone()),
            displacement: self
                .displacement
                .clone()
                .or_else(|| outer.displacement.clone()),
            luma_matte: self
                .luma_matte
                .clone()
                .or_else(|| outer.luma_matte.clone()),
        }
    }

    /// Compact fingerprint of the chain for look descriptions.
    pub fn fingerprint(&self) -> String {
        fn num(x: Option<f64>) -> String {
            x.map(|v| format!("{v:.6}")).unwrap_or_default()
        }
        fn int(x: Option<i64>) -> String {
            x.map(|v| format!("{v}")).unwrap_or_default()
        }
        fn uint(x: Option<u32>) -> String {
            x.map(|v| format!("{v}")).unwrap_or_default()
        }
        let noise = self
            .noise
            .as_ref()
            .map(|s| format!("n:{:.6}:{}", s.amount, int(s.seed)))
            .unwrap_or_default();
        let warp = self
            .warp
            .as_ref()
            .map(|s| {
                format!(
                    "w:{:.6}:{}:{}",
                    s.amount,
                    uint(s.grid),
                    num(s.strength)
                )
            })
            .unwrap_or_default();
        let displacement = self
            .displacement
            .as_ref()
            .map(|s| {
                format!(
                    "d:{:.6}:{}:{}",
                    s.amount,
                    s.map.as_deref().unwrap_or(""),
                    num(s.scale)
                )
            })
            .unwrap_or_default();
        let matte = self
            .luma_matte
            .as_ref()
            .map(|s| format!("m:{:.6}:{}", s.amount, s.src))
            .unwrap_or_default();
        format!("{noise}|{warp}|{displacement}|{matte}")
    }
}

/// One bridge between two clips (Sprint 11 transitions, CAP-005/T006):
/// a composition-level `transition` element naming the outgoing clip
/// in `from` and the incoming clip in `to`, the bridge length in
/// `duration` seconds, and the visible motion in `kind`. The bridge
/// replays the tail of `from` against the head of `to` over the first
/// `duration` seconds of `to` (the window `[to.start, to.start +
/// duration)`), borrowing at most the whole of each clip: `duration`
/// must fit inside both clips' ranges. Neither clip's range moves and
/// the timeline length is unchanged; downstream start times render
/// exactly as without the transition.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// Outgoing clip id, as written.
    pub from: String,
    /// Incoming clip id, as written.
    pub to: String,
    /// Bridge length in composition seconds (always positive).
    pub duration: f64,
    /// Visible motion of the bridge.
    pub kind: TransitionKind,
    pub line: usize,
    pub column: usize,
}

/// The visible motion of a `transition` bridge (Sprint 11 transitions,
/// CAP-005/T006): fourteen natives plus the six-piece CSS gallery. The
/// natives replace the outgoing picture with the incoming one along a
/// moving shape (or a full-frame blend for the dissolve family); the
/// `css-` gallery composites the incoming picture over the outgoing
/// one while fading it in (opacity plus rise, sink, zoom, or slide).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    Fade,
    Dissolve,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    Slide,
    Push,
    Cover,
    CircleOpen,
    Iris,
    DipBlack,
    DipWhite,
    BlurDissolve,
    CssFadeUp,
    CssFadeDown,
    CssZoomIn,
    CssZoomOut,
    CssSlideLeft,
    CssSlideRight,
}

impl TransitionKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fade" => Some(TransitionKind::Fade),
            "dissolve" => Some(TransitionKind::Dissolve),
            "wipe-left" => Some(TransitionKind::WipeLeft),
            "wipe-right" => Some(TransitionKind::WipeRight),
            "wipe-up" => Some(TransitionKind::WipeUp),
            "wipe-down" => Some(TransitionKind::WipeDown),
            "slide" => Some(TransitionKind::Slide),
            "push" => Some(TransitionKind::Push),
            "cover" => Some(TransitionKind::Cover),
            "circle-open" => Some(TransitionKind::CircleOpen),
            "iris" => Some(TransitionKind::Iris),
            "dip-black" => Some(TransitionKind::DipBlack),
            "dip-white" => Some(TransitionKind::DipWhite),
            "blur-dissolve" => Some(TransitionKind::BlurDissolve),
            "css-fade-up" => Some(TransitionKind::CssFadeUp),
            "css-fade-down" => Some(TransitionKind::CssFadeDown),
            "css-zoom-in" => Some(TransitionKind::CssZoomIn),
            "css-zoom-out" => Some(TransitionKind::CssZoomOut),
            "css-slide-left" => Some(TransitionKind::CssSlideLeft),
            "css-slide-right" => Some(TransitionKind::CssSlideRight),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Fade => "fade",
            TransitionKind::Dissolve => "dissolve",
            TransitionKind::WipeLeft => "wipe-left",
            TransitionKind::WipeRight => "wipe-right",
            TransitionKind::WipeUp => "wipe-up",
            TransitionKind::WipeDown => "wipe-down",
            TransitionKind::Slide => "slide",
            TransitionKind::Push => "push",
            TransitionKind::Cover => "cover",
            TransitionKind::CircleOpen => "circle-open",
            TransitionKind::Iris => "iris",
            TransitionKind::DipBlack => "dip-black",
            TransitionKind::DipWhite => "dip-white",
            TransitionKind::BlurDissolve => "blur-dissolve",
            TransitionKind::CssFadeUp => "css-fade-up",
            TransitionKind::CssFadeDown => "css-fade-down",
            TransitionKind::CssZoomIn => "css-zoom-in",
            TransitionKind::CssZoomOut => "css-zoom-out",
            TransitionKind::CssSlideLeft => "css-slide-left",
            TransitionKind::CssSlideRight => "css-slide-right",
        }
    }

    /// Accepted spellings, in spec order (natives, then CSS gallery).
    pub const ACCEPTED: [&'static str; 20] = [
        "fade",
        "dissolve",
        "wipe-left",
        "wipe-right",
        "wipe-up",
        "wipe-down",
        "slide",
        "push",
        "cover",
        "circle-open",
        "iris",
        "dip-black",
        "dip-white",
        "blur-dissolve",
        "css-fade-up",
        "css-fade-down",
        "css-zoom-in",
        "css-zoom-out",
        "css-slide-left",
        "css-slide-right",
    ];
}

/// The sampling tier of a composition `motion-blur` element (Sprint 11
/// motion blur, CAP-006/T007): how the written `samples` budget turns
/// into the effective per-frame sample count. `auto` measures the
/// shutter window's own motion (fraction of pixels differing between
/// the window endpoints) and scales the count to it, so a still window
/// draws exactly once; the named tiers cap the count outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionBlurTier {
    Auto,
    High,
    Medium,
    Low,
}

impl MotionBlurTier {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(MotionBlurTier::Auto),
            "high" => Some(MotionBlurTier::High),
            "medium" => Some(MotionBlurTier::Medium),
            "low" => Some(MotionBlurTier::Low),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            MotionBlurTier::Auto => "auto",
            MotionBlurTier::High => "high",
            MotionBlurTier::Medium => "medium",
            MotionBlurTier::Low => "low",
        }
    }

    /// Accepted spellings, cheapest first.
    pub const ACCEPTED: [&'static str; 4] = ["auto", "high", "medium", "low"];
}

/// Shutter blur for moving content (Sprint 11 motion blur,
/// CAP-006/T007): a composition-level `motion-blur` element with a
/// `shutter` angle in degrees (0–360), a `samples` budget (1–32), and
/// a `tier`. The frame averages evenly spaced sub-frame pictures
/// across the shutter-open window (`shutter`/360 of a frame interval,
/// ending at frame time), so content moving along its motion vectors
/// smears in its direction of travel. `None` on the composition means
/// blur off: frames draw through the exact pre-blur path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionBlur {
    /// Shutter angle in degrees, 0–360. 0 opens no window, so the
    /// frame draws sharp exactly like blur off.
    pub shutter: f64,
    /// Authored sample budget, 1–32. The tier caps it per frame.
    pub samples: u32,
    /// Sampling tier (see [`MotionBlurTier`]).
    pub tier: MotionBlurTier,
    pub line: usize,
    pub column: usize,
}

impl MotionBlur {
    /// Outright cap of the written `samples` budget for one tier
    /// (Sprint 11 motion blur, CAP-006/T007): `high` draws the full
    /// budget, `medium` at most 8, `low` at most 4. `auto` has no
    /// outright cap — it scales by measured motion instead (see
    /// [`MotionBlur::effective_samples`]).
    pub fn tier_cap(tier: MotionBlurTier, samples: u32) -> usize {
        let samples = samples.max(1) as usize;
        match tier {
            MotionBlurTier::High => samples,
            MotionBlurTier::Medium => samples.min(8),
            MotionBlurTier::Low => samples.min(4),
            MotionBlurTier::Auto => samples,
        }
    }

    /// Effective per-frame sample count (Sprint 11 motion blur,
    /// CAP-006/T007): the named tiers return [`MotionBlur::tier_cap`];
    /// `auto` scales by `motion`, the fraction of pixels differing
    /// between the shutter window's endpoints (0–1, saturated): a
    /// still window (`motion` 0) draws exactly once — pixel-identical
    /// to blur off — and fuller motion earns up to the full budget.
    pub fn effective_samples(&self, motion: f64) -> usize {
        match self.tier {
            MotionBlurTier::Auto => {
                if !(motion > 0.0) {
                    return 1;
                }
                let scaled = (motion.min(1.0) * self.samples.max(1) as f64).ceil();
                (scaled as usize).clamp(1, self.samples.max(1) as usize)
            }
            tier => Self::tier_cap(tier, self.samples),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Clip {
    Text(TextClip),
    Video(VideoClip),
    Box(BoxClip),
    Flex(FlexClip),
    Image(ImageClip),
    Path(PathClip),
    Svg(SvgClip),
    Audio(AudioClip),
    Nest(NestClip),
}

impl Clip {
    pub fn start(&self) -> f64 {
        match self {
            Clip::Text(c) => c.start,
            Clip::Video(c) => c.start,
            Clip::Box(c) => c.start,
            Clip::Flex(c) => c.start,
            Clip::Image(c) => c.start,
            Clip::Path(c) => c.start,
            Clip::Svg(c) => c.start,
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
            Clip::Flex(c) => c.start = start,
            Clip::Image(c) => c.start = start,
            Clip::Path(c) => c.start = start,
            Clip::Svg(c) => c.start = start,
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
            Clip::Flex(c) => c.id.as_deref(),
            Clip::Image(c) => c.id.as_deref(),
            Clip::Path(c) => c.id.as_deref(),
            Clip::Svg(c) => c.id.as_deref(),
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
            Clip::Flex(c) => c.start_after.as_ref(),
            Clip::Image(c) => c.start_after.as_ref(),
            Clip::Path(c) => c.start_after.as_ref(),
            Clip::Svg(c) => c.start_after.as_ref(),
            Clip::Audio(c) => c.start_after.as_ref(),
            Clip::Nest(c) => c.start_after.as_ref(),
        }
    }

    pub fn duration(&self) -> f64 {
        match self {
            Clip::Text(c) => c.duration,
            Clip::Video(c) => c.duration,
            Clip::Box(c) => c.duration,
            Clip::Flex(c) => c.duration,
            Clip::Image(c) => c.duration,
            Clip::Path(c) => c.duration,
            Clip::Svg(c) => c.duration,
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
            Clip::Flex(c) => c.duration = duration,
            Clip::Image(c) => c.duration = duration,
            Clip::Path(c) => c.duration = duration,
            Clip::Svg(c) => c.duration = duration,
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
            Clip::Flex(_) => "flex container",
            Clip::Image(_) => "image clip",
            Clip::Path(_) => "path clip",
            Clip::Svg(_) => "svg clip",
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
            Clip::Flex(c) => (c.line, c.column),
            Clip::Image(c) => (c.line, c.column),
            Clip::Path(c) => (c.line, c.column),
            Clip::Svg(c) => (c.line, c.column),
            Clip::Audio(c) => (c.line, c.column),
            Clip::Nest(c) => (c.line, c.column),
        }
    }

    /// Explicit paint order (default 0); ties fall back to document order.
    /// Audio tracks draw no picture, so they carry order 0 and never
    /// take part in paint sorting. A nest clip's own order shifts its
    /// nested items, which stack as one unit; a flex container's own
    /// order shifts its laid-out children the same way.
    pub fn order(&self) -> i64 {
        match self {
            Clip::Text(c) => c.order,
            Clip::Video(c) => c.order,
            Clip::Box(c) => c.order,
            Clip::Flex(c) => c.order,
            Clip::Image(c) => c.order,
            Clip::Path(c) => c.order,
            Clip::Svg(c) => c.order,
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
            Clip::Flex(c) => c.hidden,
            Clip::Image(c) => c.hidden,
            Clip::Path(c) => c.hidden,
            Clip::Svg(c) => c.hidden,
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
            Clip::Flex(c) => c.track,
            Clip::Image(c) => c.track,
            Clip::Path(c) => c.track,
            Clip::Svg(c) => c.track,
            Clip::Audio(c) => c.track,
            Clip::Nest(c) => c.track,
        }
    }

    /// Authored named media effects of the clip (Sprint 11 effects,
    /// CAP-003/T004): the `effect` children in document order, applied
    /// after the grade chain. Audio clips draw no picture, so their
    /// effects rest inert; flex and nest effects resolve onto the
    /// items they emit.
    pub fn effects(&self) -> &[Effect] {
        match self {
            Clip::Text(c) => &c.effects,
            Clip::Video(c) => &c.effects,
            Clip::Box(c) => &c.effects,
            Clip::Flex(c) => &c.effects,
            Clip::Image(c) => &c.effects,
            Clip::Path(c) => &c.effects,
            Clip::Svg(c) => &c.effects,
            Clip::Audio(c) => &c.effects,
            Clip::Nest(c) => &c.effects,
        }
    }

    /// Authored VFX chain of the clip (Sprint 11 VFX, CAP-004/T005):
    /// the `vfx` child, applied after the effects stack in the fixed
    /// order noise, warp, displacement, luma matte. Audio clips draw
    /// no picture, so their chain rests inert; flex and nest chains
    /// resolve onto the items they emit.
    pub fn vfx(&self) -> &VfxChain {
        match self {
            Clip::Text(c) => &c.vfx,
            Clip::Video(c) => &c.vfx,
            Clip::Box(c) => &c.vfx,
            Clip::Flex(c) => &c.vfx,
            Clip::Image(c) => &c.vfx,
            Clip::Path(c) => &c.vfx,
            Clip::Svg(c) => &c.vfx,
            Clip::Audio(c) => &c.vfx,
            Clip::Nest(c) => &c.vfx,
        }
    }

    /// Authored color grade of the clip (Sprint 11 grade, CAP-001/T001):
    /// the `grade` child, resolved with the lane and composition grades
    /// at frame time. Audio clips draw no picture, so their grade rests
    /// inert; flex and nest grades resolve onto the items they emit.
    pub fn grade(&self) -> &Grade {
        match self {
            Clip::Text(c) => &c.grade,
            Clip::Video(c) => &c.grade,
            Clip::Box(c) => &c.grade,
            Clip::Flex(c) => &c.grade,
            Clip::Image(c) => &c.grade,
            Clip::Path(c) => &c.grade,
            Clip::Svg(c) => &c.grade,
            Clip::Audio(c) => &c.grade,
            Clip::Nest(c) => &c.grade,
        }
    }

    /// Editor display note of the clip (`label` key), when declared.
    pub fn label(&self) -> Option<&str> {
        match self {
            Clip::Text(c) => c.label.as_deref(),
            Clip::Video(c) => c.label.as_deref(),
            Clip::Box(c) => c.label.as_deref(),
            Clip::Flex(c) => c.label.as_deref(),
            Clip::Image(c) => c.label.as_deref(),
            Clip::Path(c) => c.label.as_deref(),
            Clip::Svg(c) => c.label.as_deref(),
            Clip::Audio(c) => c.label.as_deref(),
            Clip::Nest(c) => c.label.as_deref(),
        }
    }

    /// Element name of the clip (`text`, `video`, `box`, `image`,
    /// `audio`, `nest`, `flex`): the `kind` timelines and info report
    /// per row.
    pub fn kind(&self) -> &'static str {
        match self {
            Clip::Text(_) => "text",
            Clip::Video(_) => "video",
            Clip::Box(_) => "box",
            Clip::Flex(_) => "flex",
            Clip::Image(_) => "image",
            Clip::Path(_) => "path",
            Clip::Svg(_) => "svg",
            Clip::Audio(_) => "audio",
            Clip::Nest(_) => "nest",
        }
    }

    /// Motion assertions of the clip (Sprint 13 check, T003): the
    /// validated `assert-appear`, `assert-inside`, and `assert-moving`
    /// children. `None` for audio clips, which carry no picture and
    /// refuse assertion children at validation.
    pub fn asserts(&self) -> Option<&MotionAsserts> {
        match self {
            Clip::Text(c) => Some(&c.asserts),
            Clip::Video(c) => Some(&c.asserts),
            Clip::Box(c) => Some(&c.asserts),
            Clip::Flex(c) => Some(&c.asserts),
            Clip::Image(c) => Some(&c.asserts),
            Clip::Path(c) => Some(&c.asserts),
            Clip::Svg(c) => Some(&c.asserts),
            Clip::Audio(_) => None,
            Clip::Nest(c) => Some(&c.asserts),
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

/// One declared render variable (Sprint 9 variables, CAP-001): a
/// `variable` element names one per-render constant with a type and
/// an optional default. Bindings (`var(--id)`, T002), caller
/// overrides (T003), strict reporting (T004), themes (T005), and
/// batch rows (T006) compose on top of this vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableType {
    String,
    Number,
    Color,
    Boolean,
    Enum,
    Font,
    Image,
}

impl VariableType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "string" => Some(VariableType::String),
            "number" => Some(VariableType::Number),
            "color" => Some(VariableType::Color),
            "boolean" => Some(VariableType::Boolean),
            "enum" => Some(VariableType::Enum),
            "font" => Some(VariableType::Font),
            "image" => Some(VariableType::Image),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VariableType::String => "string",
            VariableType::Number => "number",
            VariableType::Color => "color",
            VariableType::Boolean => "boolean",
            VariableType::Enum => "enum",
            VariableType::Font => "font",
            VariableType::Image => "image",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 7] = [
        "string", "number", "color", "boolean", "enum", "font", "image",
    ];
}

/// One validated variable default (Sprint 9 variables, CAP-001):
/// the declared `default` coerced to the declaration's type.
/// `None` on the declaration means default-less (a later slice
/// requires a caller value for it under strict validation).
#[derive(Debug, Clone, PartialEq)]
pub enum VariableDefault {
    Str(String),
    Num(f64),
    Color(Rgb),
    Bool(bool),
    Enum(String),
    /// Font family list in try order, like `font-family`: missing
    /// families are skipped per entry at bind time, and a list
    /// nothing resolves fails closed there.
    Font(Vec<String>),
    /// Project-relative media path, checked present at declaration.
    Image(String),
}

/// One `variable` declaration (Sprint 9 variables, CAP-001).
#[derive(Debug, Clone)]
pub struct VariableDecl {
    /// Declared id, unique across the project.
    pub id: String,
    pub var_type: VariableType,
    /// Typed default; none means the declaration carries no default
    /// (strict validation in a later slice requires a caller value).
    pub default: Option<VariableDefault>,
    /// Editor display note (`label`); rides along with no render effect.
    pub label: Option<String>,
    /// Declared `options` for `enum` (at least two entries); none for
    /// every other type.
    pub options: Option<Vec<String>>,
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
    /// Declared render variables, in written order (Sprint 9
    /// variables, CAP-001). Empty for variable-free compositions,
    /// which validate and render exactly as before.
    pub variables: Vec<VariableDecl>,
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
    /// Composition-wide grade (Sprint 11 grade, CAP-001/T001): the
    /// outer look every clip layers under (lane grade, then clip
    /// grade), and the look the background carries alone.
    pub grade: Grade,
    /// Per-lane grades (Sprint 11 grade, CAP-001/T001): at most one
    /// `grade` per track, layered between the composition grade and
    /// each clip's own grade.
    pub lane_grades: Vec<LaneGrade>,
    /// Clip bridges (Sprint 11 transitions, CAP-005/T006): validated
    /// `transition` elements in written order. Empty for
    /// transition-free compositions, which render exactly as before.
    pub transitions: Vec<Transition>,
    /// Shutter blur (Sprint 11 motion blur, CAP-006/T007): the
    /// validated `motion-blur` element, or `None` when the composition
    /// carries none. `None` renders exactly as before; only the render
    /// root's element blurs (nested compositions validate the same
    /// rules but render honors the root, like transitions).
    pub motion_blur: Option<MotionBlur>,
    /// Source location of the `composition` node (Sprint 13 check,
    /// T003): timeline-wide lint findings (`frozen-timeline`) name
    /// this site. Render output is unchanged.
    pub line: usize,
    pub column: usize,
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
    fn grade_presets_parse_exactly() {
        assert_eq!(GradePreset::parse("none"), Some(GradePreset::None));
        assert_eq!(GradePreset::parse("filmic"), Some(GradePreset::Filmic));
        assert_eq!(GradePreset::parse("noir"), Some(GradePreset::Noir));
        assert_eq!(
            GradePreset::parse("teal-orange"),
            Some(GradePreset::TealOrange)
        );
        assert_eq!(GradePreset::parse("pastel"), Some(GradePreset::Pastel));
        assert_eq!(GradePreset::parse("bleach"), Some(GradePreset::Bleach));
        assert_eq!(GradePreset::parse("Filmic"), None);
        assert_eq!(GradePreset::parse("vintage"), None);
        assert_eq!(GradePreset::parse("teal_orange"), None);
        assert_eq!(GradePreset::ACCEPTED.len(), 6);
        assert_eq!(GradePreset::default(), GradePreset::None);
        assert_eq!(GradePreset::Filmic.name(), "filmic");
    }

    #[test]
    fn motion_blur_tiers_parse_exactly() {
        assert_eq!(MotionBlurTier::ACCEPTED.len(), 4);
        for name in MotionBlurTier::ACCEPTED {
            let tier = MotionBlurTier::parse(name).expect("accepted tier parses");
            assert_eq!(tier.name(), name);
        }
        assert_eq!(MotionBlurTier::parse("Auto"), None);
        assert_eq!(MotionBlurTier::parse("ultra"), None);
        assert_eq!(MotionBlurTier::parse(""), None);
        assert_eq!(MotionBlurTier::parse("medium_quality"), None);
    }

    fn blur(shutter: f64, samples: u32, tier: MotionBlurTier) -> MotionBlur {
        MotionBlur {
            shutter,
            samples,
            tier,
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn motion_blur_tiers_cap_samples_per_their_rule() {
        // high draws the full budget; medium caps at 8; low at 4.
        assert_eq!(
            blur(180.0, 16, MotionBlurTier::High).effective_samples(0.0),
            16
        );
        assert_eq!(
            blur(180.0, 16, MotionBlurTier::Medium).effective_samples(0.0),
            8
        );
        assert_eq!(
            blur(180.0, 16, MotionBlurTier::Low).effective_samples(0.0),
            4
        );
        // Budgets under the cap pass through untouched.
        assert_eq!(
            blur(180.0, 5, MotionBlurTier::Medium).effective_samples(0.0),
            5
        );
        assert_eq!(
            blur(180.0, 3, MotionBlurTier::Low).effective_samples(0.0),
            3
        );
        assert_eq!(
            blur(180.0, 1, MotionBlurTier::Low).effective_samples(0.0),
            1
        );
        // Named tiers ignore the motion reading entirely.
        assert_eq!(
            blur(180.0, 16, MotionBlurTier::High).effective_samples(1.0),
            16
        );
        assert_eq!(
            blur(180.0, 16, MotionBlurTier::Low).effective_samples(1.0),
            4
        );
    }

    #[test]
    fn motion_blur_auto_scales_with_motion_and_rests_sharp() {
        let auto = |samples: u32| blur(180.0, samples, MotionBlurTier::Auto);
        // Still content draws exactly once: pixel-identical to off.
        assert_eq!(auto(16).effective_samples(0.0), 1);
        assert_eq!(auto(1).effective_samples(0.0), 1);
        // Full-window motion earns the full budget; half earns half.
        assert_eq!(auto(16).effective_samples(1.0), 16);
        assert_eq!(auto(16).effective_samples(0.5), 8);
        // Motion below one sample-step still rests sharp; the budget
        // scales up linearly from there.
        assert_eq!(auto(16).effective_samples(0.001), 1);
        assert_eq!(auto(16).effective_samples(0.0625), 1);
        assert_eq!(auto(16).effective_samples(0.0626), 2);
        // A budget of one stays one whatever the motion.
        assert_eq!(auto(1).effective_samples(1.0), 1);
        // Oversaturated motion readings saturate at the budget.
        assert_eq!(auto(16).effective_samples(2.5), 16);
        // NaN motion (no reading) rests sharp, never panics.
        assert_eq!(auto(16).effective_samples(f64::NAN), 1);
    }

    fn effect_presets_parse_exactly() {
        assert_eq!(EffectPreset::ACCEPTED.len(), 18);
        for name in EffectPreset::ACCEPTED {
            let preset = EffectPreset::parse(name).expect("accepted preset parses");
            assert_eq!(preset.name(), name);
        }
        assert_eq!(EffectPreset::parse("Grayscale"), None);
        assert_eq!(EffectPreset::parse("vintage"), None);
        assert_eq!(EffectPreset::parse("hue_rotate"), None);
        assert_eq!(EffectPreset::parse(""), None);
        // Every preset documents the keys it reads; `preset` and
        // `amount` ride outside this list.
        assert!(EffectPreset::Grayscale.allowed_keys().is_empty());
        assert_eq!(EffectPreset::Grain.allowed_keys(), &["size", "seed"]);
        assert_eq!(
            EffectPreset::ChromaticAberration.allowed_keys(),
            &["distance"]
        );
        // Every known key belongs to at least the universal pair or
        // one preset; no key is silently unread.
        for key in EFFECT_KEYS.iter() {
            assert!(
                *key == "preset"
                    || *key == "amount"
                    || EffectPreset::ACCEPTED.iter().any(|n| EffectPreset::parse(n)
                        .expect("accepted preset parses")
                        .allowed_keys()
                        .contains(key)),
                "key {key} belongs to no preset"
            );
        }
    }

    #[test]
    fn effect_defaults_and_identity() {
        let params = EffectParams::default();
        assert_eq!(params.brightness(), EFFECT_DEFAULT_BRIGHTNESS);
        assert_eq!(params.contrast(), EFFECT_DEFAULT_CONTRAST);
        assert_eq!(params.saturate(), EFFECT_DEFAULT_SATURATE);
        assert_eq!(params.angle(), EFFECT_DEFAULT_HUE_ANGLE);
        assert_eq!(params.radius(), EFFECT_DEFAULT_BLUR_RADIUS);
        assert_eq!(params.strength(), EFFECT_DEFAULT_STRENGTH);
        assert_eq!(params.softness(), EFFECT_DEFAULT_VIGNETTE_SOFTNESS);
        assert_eq!(params.grain_size(), EFFECT_DEFAULT_GRAIN_SIZE);
        assert_eq!(params.seed(), EFFECT_SEED_DEFAULT);
        assert_eq!(params.pixel_size(), EFFECT_DEFAULT_PIXEL_SIZE);
        assert_eq!(params.levels(), EFFECT_DEFAULT_POSTERIZE_LEVELS);
        assert_eq!(params.threshold(), EFFECT_DEFAULT_THRESHOLD);
        assert_eq!(params.distance(), EFFECT_DEFAULT_CHROMA_DISTANCE);
        let full = Effect {
            preset: EffectPreset::Grayscale,
            amount: 1.0,
            params: EffectParams::default(),
        };
        assert!(!full.is_empty());
        let off = Effect {
            amount: 0.0,
            ..full.clone()
        };
        assert!(off.is_empty());
        // Distinct looks fingerprint distinctly.
        assert_ne!(full.fingerprint(), off.fingerprint());
    }

    #[test]
    fn vfx_stages_default_and_identity() {
        assert_eq!(VFX_STAGES, ["noise", "warp", "displacement", "luma-matte"]);
        let warp = VfxWarp::default();
        assert_eq!(warp.grid(), VFX_DEFAULT_WARP_GRID);
        assert_eq!(warp.strength(), VFX_DEFAULT_WARP_STRENGTH);
        let displacement = VfxDisplacement::default();
        assert_eq!(displacement.scale(), VFX_DEFAULT_DISPLACEMENT_SCALE);
        assert_eq!(VfxNoise::default().seed(), VFX_SEED_DEFAULT);
        // The empty chain renders untouched.
        assert!(VfxChain::empty().is_empty());
        assert!(VfxChain::default().is_empty());
        // Every stage at full strength keeps the chain active.
        let full = VfxChain {
            noise: Some(VfxNoise { amount: 1.0, seed: None }),
            warp: Some(VfxWarp { amount: 1.0, grid: None, strength: None }),
            displacement: Some(VfxDisplacement { amount: 1.0, map: None, scale: None }),
            luma_matte: Some(VfxLumaMatte { amount: 1.0, src: "m.png".to_string() }),
        };
        assert!(!full.is_empty());
        // An inert stage matches the chain written without it.
        let quiet = VfxChain {
            noise: Some(VfxNoise { amount: 0.0, seed: None }),
            ..VfxChain::empty()
        };
        assert!(quiet.is_empty());
        let flat_warp = VfxChain {
            warp: Some(VfxWarp { amount: 1.0, grid: None, strength: Some(0.0) }),
            ..VfxChain::empty()
        };
        assert!(flat_warp.is_empty());
        let flat_displacement = VfxChain {
            displacement: Some(VfxDisplacement { amount: 1.0, map: None, scale: Some(0.0) }),
            ..VfxChain::empty()
        };
        assert!(flat_displacement.is_empty());
        // Distinct chains fingerprint distinctly.
        assert_ne!(full.fingerprint(), VfxChain::empty().fingerprint());
        assert_ne!(full.fingerprint(), quiet.fingerprint());
    }

    #[test]
    fn vfx_chain_layers_inner_wins_per_stage() {
        let inner = VfxChain {
            warp: Some(VfxWarp { amount: 0.5, grid: Some(8), strength: None }),
            ..VfxChain::empty()
        };
        let outer = VfxChain {
            warp: Some(VfxWarp { amount: 1.0, grid: None, strength: None }),
            noise: Some(VfxNoise { amount: 1.0, seed: Some(3) }),
            ..VfxChain::empty()
        };
        let merged = inner.layer_outer(&outer);
        // The inner warp wins; the missing noise inherits the outer one.
        assert_eq!(merged.warp.as_ref().unwrap().amount, 0.5);
        assert_eq!(merged.warp.as_ref().unwrap().grid, Some(8));
        assert_eq!(merged.noise.as_ref().unwrap().seed, Some(3));
        assert!(merged.displacement.is_none());
        assert!(merged.luma_matte.is_none());
    }

    #[test]
    fn grade_identity_passes_pixels_through() {
        let id = ResolvedGrade::identity();
        assert!(id.is_empty());
        assert_eq!(id.apply(0.0, 0.0, 0.0), (0.0, 0.0, 0.0));
        assert_eq!(id.apply(255.0, 255.0, 255.0), (255.0, 255.0, 255.0));
        assert_eq!(id.apply(10.0, 128.0, 250.0), (10.0, 128.0, 250.0));
        assert_eq!(Grade::empty().is_empty(), true);
        assert_eq!(Grade::default().is_empty(), true);
    }

    #[test]
    fn grade_levels_compose_clip_wins_missing_skipped() {
        // Outer composition grade: preset plus exposure.
        let comp = Grade {
            preset: Some(GradePreset::Filmic),
            exposure: Some(1.0),
            contrast: Some(0.2),
            saturation: None,
            temperature: None,
            tint: None,
            ..Default::default()
        };
        // Lane grade overrides exposure only; the preset inherits.
        let lane = Grade {
            preset: None,
            exposure: Some(-1.0),
            contrast: None,
            saturation: Some(0.5),
            temperature: None,
            tint: None,
            ..Default::default()
        };
        // Clip grade overrides the preset explicitly and inherits the rest.
        let clip = Grade {
            preset: Some(GradePreset::None),
            exposure: None,
            contrast: None,
            saturation: None,
            temperature: Some(0.25),
            tint: None,
            ..Default::default()
        };
        let resolved = ResolvedGrade::identity()
            .layer(&comp)
            .layer_opt(Some(&lane))
            .layer(&clip);
        assert_eq!(resolved.preset, GradePreset::None);
        assert_eq!(resolved.exposure, -1.0);
        assert_eq!(resolved.contrast, 0.2);
        assert_eq!(resolved.saturation, 0.5);
        assert_eq!(resolved.temperature, 0.25);
        assert_eq!(resolved.tint, 0.0);
        // A missing lane keeps the composition value, never zeroed.
        let no_lane = ResolvedGrade::identity().layer(&comp).layer_opt(None);
        assert_eq!(no_lane.exposure, 1.0);
        assert_eq!(no_lane.preset, GradePreset::Filmic);
    }

    #[test]
    fn grade_adjusts_shift_pixels_in_their_direction() {
        let mid = (128.0, 128.0, 128.0);
        // Exposure +1 EV doubles the channel (clamped).
        let up = ResolvedGrade {
            exposure: 1.0,
            ..ResolvedGrade::identity()
        };
        let (r, g, b) = up.apply(mid.0, mid.1, mid.2);
        assert!((r - 255.0).abs() < 1.0 && (g - 255.0).abs() < 1.0, "{r},{g},{b}");
        let down = ResolvedGrade {
            exposure: -1.0,
            ..ResolvedGrade::identity()
        };
        let (r, _, _) = down.apply(mid.0, mid.1, mid.2);
        assert!((r - 64.0).abs() < 1.0, "{r}");
        // Full desaturation yields gray; full saturation boost spreads channels.
        let gray = ResolvedGrade {
            saturation: -1.0,
            ..ResolvedGrade::identity()
        };
        let (r, g, b) = gray.apply(200.0, 100.0, 50.0);
        assert!((r - g).abs() < 1e-6 && (g - b).abs() < 1e-6, "{r},{g},{b}");
        let vivid = ResolvedGrade {
            saturation: 1.0,
            ..ResolvedGrade::identity()
        };
        let (r0, _, _) = vivid.apply(200.0, 100.0, 50.0);
        let (r1, _, _) = gray.apply(200.0, 100.0, 50.0);
        assert!(r0 > r1, "{r0} vs {r1}");
        // Positive contrast pushes brights up and darks down.
        let punch = ResolvedGrade {
            contrast: 0.5,
            ..ResolvedGrade::identity()
        };
        let (hi, _, _) = punch.apply(200.0, 200.0, 200.0);
        let (lo, _, _) = punch.apply(60.0, 60.0, 60.0);
        assert!(hi > 200.0 && lo < 60.0, "{hi},{lo}");
        // Temperature warms (red up, blue down); tint magentas (green down).
        let warm = ResolvedGrade {
            temperature: 1.0,
            ..ResolvedGrade::identity()
        };
        let (r, g, b) = warm.apply(128.0, 128.0, 128.0);
        assert!(r > 128.0 && b < 128.0 && g == 128.0, "{r},{g},{b}");
        let magenta = ResolvedGrade {
            tint: 1.0,
            ..ResolvedGrade::identity()
        };
        let (r, g, b) = magenta.apply(128.0, 128.0, 128.0);
        assert!(r > 128.0 && b > 128.0 && g < 128.0, "{r},{g},{b}");
    }

    #[test]
    fn grade_presets_render_distinct_looks() {
        let px = (90.0, 120.0, 150.0);
        let looks: Vec<(GradePreset, (f64, f64, f64))> = GradePreset::ACCEPTED
            .iter()
            .map(|name| {
                let preset = GradePreset::parse(name).unwrap();
                let g = ResolvedGrade {
                    preset,
                    ..ResolvedGrade::identity()
                };
                (preset, g.apply(px.0, px.1, px.2))
            })
            .collect();
        // Every preset except `none` visibly changes the pixel.
        for (preset, out) in &looks {
            let (r, g, b) = *out;
            if *preset == GradePreset::None {
                assert_eq!((r, g, b), px);
            } else {
                let dr = (r - px.0).abs() + (g - px.1).abs() + (b - px.2).abs();
                assert!(dr > 1.0, "{preset:?} barely moves {px:?}: {out:?}");
            }
        }
        // Noir is gray, pastel lifts shadows, bleach brightens the mean.
        let noir = ResolvedGrade {
            preset: GradePreset::Noir,
            ..ResolvedGrade::identity()
        };
        let (r, g, b) = noir.apply(px.0, px.1, px.2);
        assert!((r - g).abs() < 1e-6 && (g - b).abs() < 1e-6, "{r},{g},{b}");
        let pastel = ResolvedGrade {
            preset: GradePreset::Pastel,
            ..ResolvedGrade::identity()
        };
        let (r, _, _) = pastel.apply(10.0, 10.0, 10.0);
        assert!(r > 10.0, "{r}");
        let bleach = ResolvedGrade {
            preset: GradePreset::Bleach,
            ..ResolvedGrade::identity()
        };
        let (r, g, b) = bleach.apply(px.0, px.1, px.2);
        assert!((r + g + b) / 3.0 > (px.0 + px.1 + px.2) / 3.0, "{r},{g},{b}");
    }

    // --- Sprint 11 grade detail stages (T002) ---

    /// One detail-only resolved look: every T001 piece identity, one
    /// stage set. Positions read on a 64x64 canvas.
    fn detail() -> ResolvedGrade {
        ResolvedGrade::identity()
    }

    fn at(grade: &ResolvedGrade, v: f64) -> (f64, f64, f64) {
        grade.apply_at(v, v, v, 0.0, 0.0, 64.0, 64.0)
    }

    #[test]
    fn grade_curve_points_parse_the_documented_shape() {
        assert_eq!(
            parse_curve_points("0,0 0.5,0.6 1,1"),
            Some(vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)])
        );
        assert_eq!(
            parse_curve_points("0,1 1,0"),
            Some(vec![(0.0, 1.0), (1.0, 0.0)])
        );
        // Reject: a lone point, a broken pair, non-numbers, out-of-range
        // numbers, flat or falling `x`, trailing commas, empties.
        assert_eq!(parse_curve_points("0,0"), None);
        assert_eq!(parse_curve_points("0,0 0.5"), None);
        assert_eq!(parse_curve_points("a,b 1,1"), None);
        assert_eq!(parse_curve_points("0,0 2,1"), None);
        assert_eq!(parse_curve_points("0,0 0,-0.5 1,1"), None);
        assert_eq!(parse_curve_points("0,0 0.5,0.5 0.5,0.9 1,1"), None);
        assert_eq!(parse_curve_points("0,0 0.7,0.5 0.4,0.9 1,1"), None);
        assert_eq!(parse_curve_points("0,0, 1,1"), None);
        assert_eq!(parse_curve_points(""), None);
        assert_eq!(parse_curve_points("0,0 NaN,1 1,1"), None);
    }

    #[test]
    fn grade_wheels_shift_only_their_zone() {
        // Zone-center grays: 127.5 sits exactly on middle gray so the
        // far-zone weights read exactly zero.
        let (dark, mid, bright) = (25.0, 127.5, 180.0);
        let wheel = |s: f64, m: f64, h: f64| {
            let mut g = detail();
            g.wheel.shadows = s;
            g.wheel.midtones = m;
            g.wheel.highlights = h;
            g
        };
        // Shadows lift the dark pixel and leave mid and bright exact.
        let g = wheel(0.6, 0.0, 0.0);
        let (r, gg, b) = at(&g, dark);
        assert!(r > dark + 50.0 && gg == r && b == r, "{r},{gg},{b}");
        assert_eq!(at(&g, mid), (mid, mid, mid));
        assert_eq!(at(&g, bright), (bright, bright, bright));
        // Highlights lift the bright pixel and leave dark and mid exact.
        let g = wheel(0.0, 0.0, 0.6);
        let (r, _, _) = at(&g, bright);
        assert!(r > bright + 10.0, "{r}");
        assert_eq!(at(&g, dark), (dark, dark, dark));
        assert_eq!(at(&g, mid), (mid, mid, mid));
        // Midtones lift middle gray and leave pure black and white exact.
        let g = wheel(0.0, 0.5, 0.0);
        let (r, _, _) = at(&g, mid);
        assert!((r - (mid + 127.5)).abs() < 1.0, "{r}");
        assert_eq!(at(&g, 0.0), (0.0, 0.0, 0.0));
        assert_eq!(at(&g, 255.0), (255.0, 255.0, 255.0));
        // A zeroed wheel is the identity look.
        assert!(wheel(0.0, 0.0, 0.0).is_empty());
        assert!(!g.is_empty());
    }

    #[test]
    fn grade_curve_remaps_its_channel_through_the_points() {
        let mut g = detail();
        g.curve.red = vec![(0.0, 0.0), (0.5, 1.0), (1.0, 1.0)];
        // Mid red clips to the top while green and blue pass through.
        let (r, gg, b) = g.apply_at(128.0, 100.0, 50.0, 0.0, 0.0, 64.0, 64.0);
        assert_eq!((r, gg, b), (255.0, 100.0, 50.0));
        // Below the first point takes its output; above the last takes
        // its output; between lerps the segment.
        let mut g = detail();
        g.curve.master = vec![(0.0, 0.0), (1.0, 0.5)];
        let (r, gg, b) = g.apply_at(200.0, 100.0, 50.0, 0.0, 0.0, 64.0, 64.0);
        assert!((r - 100.0).abs() < 0.5, "{r}");
        assert!((gg - 50.0).abs() < 0.5, "{gg}");
        assert!((b - 25.0).abs() < 0.5, "{b}");
        // Master runs before the per-channel list: master halves, then
        // red lifts back through its own points.
        let mut g = detail();
        g.curve.master = vec![(0.0, 0.0), (1.0, 0.5)];
        g.curve.red = vec![(0.0, 0.0), (0.5, 1.0), (1.0, 1.0)];
        let (r, gg, _) = g.apply_at(200.0, 100.0, 50.0, 0.0, 0.0, 64.0, 64.0);
        assert!((r - 200.0).abs() < 1.0, "{r}");
        assert!((gg - 50.0).abs() < 0.5, "{gg}");
        // An empty curve stage is the identity look.
        assert!(detail().is_empty());
        assert!(ResolvedCurve::identity().is_empty());
    }

    #[test]
    fn grade_hsl_shifts_only_colors_inside_the_range() {
        let mut g = detail();
        g.hsl.hue_min = 300.0;
        g.hsl.hue_max = 60.0;
        g.hsl.saturation = 0.2;
        g.hsl.lightness = 0.2;
        assert!(!g.is_empty());
        // Red sits inside the wrapping window: lightness lifts it pink.
        let (r, gg, b) = g.apply_at(255.0, 0.0, 0.0, 0.0, 0.0, 64.0, 64.0);
        assert!((r - 255.0).abs() < 1.0, "{r}");
        assert!(gg > 90.0 && gg < 115.0 && (gg - b).abs() < 1e-6, "{r},{gg},{b}");
        // Green sits outside the window: byte-identical.
        assert_eq!(g.apply_at(0.0, 255.0, 0.0, 0.0, 0.0, 64.0, 64.0), (0.0, 255.0, 0.0));
        // Gray fails the saturation floor: byte-identical.
        assert_eq!(
            g.apply_at(128.0, 128.0, 128.0, 0.0, 0.0, 64.0, 64.0),
            (128.0, 128.0, 128.0)
        );
        // A non-wrapping window excludes red but keeps green.
        let mut g = detail();
        g.hsl.hue_min = 90.0;
        g.hsl.hue_max = 270.0;
        g.hsl.lightness = 0.2;
        assert_eq!(g.apply_at(255.0, 0.0, 0.0, 0.0, 0.0, 64.0, 64.0), (255.0, 0.0, 0.0));
        let (r, gg, b) = g.apply_at(0.0, 255.0, 0.0, 0.0, 0.0, 64.0, 64.0);
        assert!((gg - 255.0).abs() < 1.0 && r > 90.0, "{r},{gg},{b}");
        // A zero shift is the identity look whatever the window holds.
        assert!(!g.hsl.is_empty());
        let mut g = detail();
        g.hsl.hue_min = 90.0;
        assert!(g.is_empty());
    }

    #[test]
    fn grade_vignette_darkens_corners_not_the_center() {
        let mut g = detail();
        g.vignette.amount = 0.8;
        g.vignette.softness = 0.5;
        assert!(!g.is_empty());
        assert!(g.is_spatial());
        // The center pixel never moves, whatever the knobs hold.
        assert_eq!(g.apply_at(200.0, 200.0, 200.0, 31.0, 31.0, 64.0, 64.0), (200.0, 200.0, 200.0));
        let mut full = detail();
        full.vignette.amount = 1.0;
        full.vignette.softness = 1.0;
        // An odd canvas has a pixel dead on the frame center: distance
        // zero reads exactly zero falloff even at full softness.
        assert_eq!(
            full.apply_at(200.0, 200.0, 200.0, 32.0, 32.0, 65.0, 65.0),
            (200.0, 200.0, 200.0)
        );
        // The corner loses most of its light.
        let (r, gg, b) = g.apply_at(200.0, 200.0, 200.0, 0.0, 0.0, 64.0, 64.0);
        assert!(r < 60.0 && gg == r && b == r, "{r},{gg},{b}");
        // Full amount all but blacks the corner pixel out (pixel centers
        // never sit exactly at distance one).
        let (r, _, _) = full.apply_at(200.0, 200.0, 200.0, 0.0, 0.0, 64.0, 64.0);
        assert!(r < 1.0, "{r}");
        // Zero amount is the identity look even with softness authored.
        let mut g = detail();
        g.vignette.softness = 0.9;
        assert!(g.is_empty());
        assert!(!g.is_spatial());
    }

    #[test]
    fn grade_grain_is_deterministic_and_seed_sensitive() {
        let grain = |seed: i64| {
            let mut g = detail();
            g.grain.amount = 1.0;
            g.grain.size = 1;
            g.grain.seed = seed;
            g
        };
        // The same seed re-renders byte-identically.
        let a = grain(7);
        let first = a.apply_at(128.0, 128.0, 128.0, 5.0, 9.0, 64.0, 64.0);
        let second = a.apply_at(128.0, 128.0, 128.0, 5.0, 9.0, 64.0, 64.0);
        assert_eq!(first, second);
        assert_ne!(first, (128.0, 128.0, 128.0), "grain must show");
        // Different seeds pattern differently.
        let b = grain(8);
        let other = b.apply_at(128.0, 128.0, 128.0, 5.0, 9.0, 64.0, 64.0);
        assert_ne!(first, other);
        // Grain cells share one value across `size` pixels.
        let mut big = detail();
        big.grain.amount = 1.0;
        big.grain.size = 2;
        big.grain.seed = 7;
        let p00 = big.apply_at(128.0, 128.0, 128.0, 4.0, 6.0, 64.0, 64.0);
        let p10 = big.apply_at(128.0, 128.0, 128.0, 5.0, 6.0, 64.0, 64.0);
        assert_eq!(p00, p10);
        // Zero amount is the identity look even with a seed authored.
        let mut g = detail();
        g.grain.seed = 99;
        g.grain.size = 4;
        assert!(g.is_empty());
        assert!(!g.is_spatial());
        assert_eq!(g.apply_at(10.0, 128.0, 250.0, 3.0, 3.0, 64.0, 64.0), (10.0, 128.0, 250.0));
    }

    #[test]
    fn grade_detail_layers_piece_by_piece_inward() {
        // The lane authors a wheel band and a curve channel; the clip
        // overrides that band and adds its own vignette: every authored
        // piece wins, every missing piece inherits.
        let mut lane = Grade::empty();
        lane.wheel.shadows = Some(0.2);
        lane.curve.red = Some(vec![(0.0, 0.0), (1.0, 1.0)]);
        lane.vignette.softness = Some(0.9);
        let mut clip = Grade::empty();
        clip.wheel.shadows = Some(-0.4);
        clip.wheel.highlights = Some(0.3);
        clip.grain.seed = Some(11);
        let resolved = ResolvedGrade::identity().layer(&lane).layer(&clip);
        assert_eq!(resolved.wheel.shadows, -0.4);
        assert_eq!(resolved.wheel.highlights, 0.3);
        assert_eq!(resolved.wheel.midtones, 0.0);
        assert_eq!(resolved.curve.red, vec![(0.0, 0.0), (1.0, 1.0)]);
        assert!(resolved.curve.green.is_empty());
        assert_eq!(resolved.vignette.softness, 0.9);
        assert_eq!(resolved.vignette.amount, 0.0);
        assert_eq!(resolved.grain.seed, 11);
        assert_eq!(resolved.grain.amount, 0.0);
        // An inner channel list replaces the outer one wholesale.
        let mut outer = Grade::empty();
        outer.curve.red = Some(vec![(0.0, 0.0), (1.0, 0.5)]);
        let mut inner = Grade::empty();
        inner.curve.red = Some(vec![(0.0, 0.0), (1.0, 1.0)]);
        let resolved = ResolvedGrade::identity().layer(&outer).layer(&inner);
        assert_eq!(resolved.curve.red, vec![(0.0, 0.0), (1.0, 1.0)]);
        assert!(!resolved.is_empty());
    }

    #[test]
    fn grade_detail_rides_the_fingerprint() {
        let base = detail().fingerprint();
        let mut wheel = detail();
        wheel.wheel.midtones = 0.1;
        assert_ne!(wheel.fingerprint(), base);
        let mut curve = detail();
        curve.curve.green = vec![(0.0, 0.0), (1.0, 1.0)];
        assert_ne!(curve.fingerprint(), base);
        assert_ne!(curve.fingerprint(), wheel.fingerprint());
        let mut hsl = detail();
        hsl.hsl.lightness = -0.1;
        assert_ne!(hsl.fingerprint(), base);
        let mut vig = detail();
        vig.vignette.amount = 0.4;
        assert_ne!(vig.fingerprint(), base);
        let mut grain = detail();
        grain.grain.amount = 0.4;
        grain.grain.seed = 3;
        assert_ne!(grain.fingerprint(), base);
        // The T001 prefix still fingerprints first: a preset-only look
        // keeps its historic head.
        let mut preset = detail();
        preset.preset = GradePreset::Filmic;
        assert!(preset.fingerprint().starts_with("filmic:"), "{}", preset.fingerprint());
    }

    #[test]
    fn grade_apply_prefix_matches_apply_at_without_detail() {
        // Without detail stages the full chain degrades exactly to the
        // T001 prefix at any position: existing pixels stay bit-exact.
        let mut g = Grade::empty();
        g.preset = Some(GradePreset::TealOrange);
        g.exposure = Some(0.5);
        g.contrast = Some(0.2);
        g.saturation = Some(-0.25);
        g.temperature = Some(0.25);
        g.tint = Some(-0.5);
        let resolved = ResolvedGrade::identity().layer(&g);
        for (px, x, y) in [(10.0, 3.0, 7.0), (200.0, 0.0, 0.0), (128.0, 63.0, 31.0)] {
            assert_eq!(
                resolved.apply(px, 100.0, 50.0),
                resolved.apply_at(px, 100.0, 50.0, x, y, 64.0, 64.0)
            );
        }
    }

    /// A size-2 `.cube` swapping red and blue, R-fastest.
    fn swap_cube_text() -> String {
        let mut s = String::from("TITLE \"swap r and b\"\n# a comment line\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    s.push_str(&format!("{b} {g} {r}\n"));
                }
            }
        }
        s
    }

    fn swap_grid() -> LutTable {
        parse_cube(&swap_cube_text()).expect("swap cube parses")
    }

    #[test]
    fn cube_parses_identity_and_trilinear_midpoints() {
        // Identity grid: lattice points pass through, midpoints
        // interpolate linearly.
        let mut s = String::from("LUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    s.push_str(&format!("{r} {g} {b}\n"));
                }
            }
        }
        let t = parse_cube(&s).expect("identity cube parses");
        assert_eq!(t.size, 2);
        assert_eq!(t.values.len(), 8);
        assert_eq!(t.title, None);
        assert_eq!(t.lookup(0.0, 0.0, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(t.lookup(1.0, 1.0, 1.0), [1.0, 1.0, 1.0]);
        let mid = t.lookup(0.5, 0.25, 0.75);
        assert!((mid[0] - 0.5).abs() < 1e-9, "{mid:?}");
        assert!((mid[1] - 0.25).abs() < 1e-9, "{mid:?}");
        assert!((mid[2] - 0.75).abs() < 1e-9, "{mid:?}");
        // Sampling exactly on the lattice returns stored triplets
        // bit-exactly (same-size bake reimports pixel-identically).
        let t2 = swap_grid();
        assert_eq!(t2.title, Some("swap r and b".to_string()));
        assert_eq!(t2.lookup(1.0, 0.0, 0.0), [0.0, 0.0, 1.0]);
        assert_eq!(t2.lookup(0.0, 0.0, 1.0), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn cube_honors_explicit_domain() {
        let mut s = String::from(
            "TITLE test\nDOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 2.0 2.0 2.0\nLUT_3D_SIZE 2\n",
        );
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    s.push_str(&format!("{} {} {}\n", 2 * r, 2 * g, 2 * b));
                }
            }
        }
        let t = parse_cube(&s).expect("domain cube parses");
        assert_eq!(t.domain_max, [2.0, 2.0, 2.0]);
        // Identity over [0, 2]: normalized corners map back exactly.
        assert_eq!(t.lookup(0.0, 0.0, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(t.lookup(1.0, 1.0, 1.0), [1.0, 1.0, 1.0]);
        let mid = t.lookup(0.5, 0.5, 0.5);
        assert!((mid[0] - 0.5).abs() < 1e-9, "{mid:?}");
    }

    #[test]
    fn cube_rejects_bad_files_naming_the_problem() {
        // Missing size.
        let err = parse_cube("0 0 0\n1 1 1\n").unwrap_err();
        assert!(err.contains("LUT_3D_SIZE"), "{err}");
        // Oversized grid names the 33 limit.
        let err = parse_cube("LUT_3D_SIZE 64\n").unwrap_err();
        assert!(err.contains("64"), "{err}");
        assert!(err.contains("33"), "{err}");
        // Size 1 is below the 2 minimum.
        let err = parse_cube("LUT_3D_SIZE 1\n0 0 0\n").unwrap_err();
        assert!(err.contains("2"), "{err}");
        // 1D LUTs are refused, not guessed.
        let err = parse_cube("LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap_err();
        assert!(err.contains("1D"), "{err}");
        // A non-triplet line names the line.
        let err = parse_cube("LUT_3D_SIZE 2\n0 0\n").unwrap_err();
        assert!(err.contains("line 2"), "{err}");
        // A short body names the shortfall.
        let mut s = String::from("LUT_3D_SIZE 2\n");
        s.push_str("0 0 0\n");
        let err = parse_cube(&s).unwrap_err();
        assert!(err.contains('8'), "{err}");
        // A non-finite triplet is not data.
        let err = parse_cube("LUT_3D_SIZE 2\nNaN 0 0\n").unwrap_err();
        assert!(err.contains("line 2"), "{err}");
        // Data before the size names the order.
        let err = parse_cube("0 0 0\nLUT_3D_SIZE 2\n").unwrap_err();
        assert!(err.contains("LUT_3D_SIZE"), "{err}");
        // A collapsed domain is refused.
        let bad_domain = "DOMAIN_MIN 0 0 0\nDOMAIN_MAX 0 0 0\nLUT_3D_SIZE 2\n".to_string()
            + &"0 0 0\n".repeat(8);
        let err = parse_cube(&bad_domain).unwrap_err();
        assert!(err.contains("DOMAIN_MAX"), "{err}");
    }

    #[test]
    fn lut_layers_piece_by_piece_inward() {
        // Outer authors src and amount; inner amount-only inherits
        // the file, inner src-only inherits the blend.
        let mut outer = Grade::empty();
        outer.lut.src = Some("look.cube".to_string());
        outer.lut.amount = Some(0.5);
        let mut inner = Grade::empty();
        inner.lut.amount = Some(0.8);
        let resolved = ResolvedGrade::identity().layer(&outer).layer(&inner);
        assert_eq!(resolved.lut.src, Some("look.cube".to_string()));
        assert_eq!(resolved.lut.amount, 0.8);
        assert!(resolved.lut.grid.is_none());
        assert!(!resolved.is_empty());
        let mut inner_src = Grade::empty();
        inner_src.lut.src = Some("other.cube".to_string());
        let resolved = ResolvedGrade::identity().layer(&outer).layer(&inner_src);
        assert_eq!(resolved.lut.src, Some("other.cube".to_string()));
        assert_eq!(resolved.lut.amount, 0.5);
        // Bare levels inherit everything; a bare chain stays identity.
        let resolved = ResolvedGrade::identity().layer(&Grade::empty());
        assert!(resolved.lut.is_empty());
        assert!(resolved.is_empty());
        // A zero amount is the identity whatever src holds.
        let mut zero = Grade::empty();
        zero.lut.src = Some("look.cube".to_string());
        zero.lut.amount = Some(0.0);
        let resolved = ResolvedGrade::identity().layer(&zero);
        assert!(resolved.lut.is_empty());
        assert!(resolved.is_empty());
    }

    #[test]
    fn lut_applies_at_its_amount() {
        let grid = swap_grid();
        // Full amount swaps red and blue exactly.
        let mut full = ResolvedGrade::identity();
        full.lut.src = Some("swap.cube".to_string());
        full.lut.amount = 1.0;
        full.lut.grid = Some(grid.clone());
        assert!(!full.is_empty());
        let (r, g, b) = full.apply(255.0, 0.0, 0.0);
        assert!((r - 0.0).abs() < 1e-6 && (g - 0.0).abs() < 1e-6 && (b - 255.0).abs() < 1e-6, "{r},{g},{b}");
        // Zero amount passes through (pixel-identical to no LUT).
        let mut zero = full.clone();
        zero.lut.amount = 0.0;
        assert!(zero.is_empty());
        assert_eq!(zero.apply(200.0, 100.0, 50.0), (200.0, 100.0, 50.0));
        // Half amount lands halfway between unmapped and mapped.
        let mut half = full.clone();
        half.lut.amount = 0.5;
        let plain = ResolvedGrade::identity().apply(200.0, 100.0, 50.0);
        let mapped = full.apply(200.0, 100.0, 50.0);
        let blended = half.apply(200.0, 100.0, 50.0);
        for i in 0..3 {
            let (p, m, h) = (
                [plain.0, plain.1, plain.2][i],
                [mapped.0, mapped.1, mapped.2][i],
                [blended.0, blended.1, blended.2][i],
            );
            assert!((h - (p + m) / 2.0).abs() < 1e-6, "{p},{m},{h}");
        }
        // The LUT rides the position-independent apply and the
        // positional apply_at identically (it is not spatial).
        let at = half.apply_at(200.0, 100.0, 50.0, 3.0, 5.0, 64.0, 64.0);
        assert_eq!((blended.0, blended.1, blended.2), (at.0, at.1, at.2));
        assert!(!half.is_spatial());
        // An unloaded grid passes through until the resolve step
        // loads it.
        let mut unloaded = ResolvedGrade::identity();
        unloaded.lut.src = Some("swap.cube".to_string());
        assert_eq!(unloaded.apply(255.0, 0.0, 0.0), (255.0, 0.0, 0.0));
    }

    #[test]
    fn lut_rides_the_fingerprint() {
        let base = ResolvedGrade::identity().fingerprint();
        let mut src = ResolvedGrade::identity();
        src.lut.src = Some("a.cube".to_string());
        assert_ne!(src.fingerprint(), base);
        let mut other = ResolvedGrade::identity();
        other.lut.src = Some("b.cube".to_string());
        assert_ne!(other.fingerprint(), src.fingerprint());
        let mut half = src.clone();
        half.lut.amount = 0.5;
        assert_ne!(half.fingerprint(), src.fingerprint());
    }

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
    fn fill_rule_spellings_are_exact() {
        use super::FillRule;
        assert_eq!(FillRule::parse("nonzero"), Some(FillRule::NonZero));
        assert_eq!(FillRule::parse("evenodd"), Some(FillRule::EvenOdd));
        assert_eq!(FillRule::parse("NonZero"), None);
        assert_eq!(FillRule::parse("winding"), None);
        assert_eq!(FillRule::ACCEPTED, ["nonzero", "evenodd"]);
        assert_eq!(FillRule::default(), FillRule::NonZero);
        assert_eq!(FillRule::NonZero.name(), "nonzero");
        assert_eq!(FillRule::EvenOdd.name(), "evenodd");
    }

    #[test]
    fn paint_stack_spellings_are_exact() {
        use super::{BlendMode, Overflow, StrokePosition};
        assert_eq!(
            StrokePosition::parse("center"),
            Some(StrokePosition::Center)
        );
        assert_eq!(
            StrokePosition::parse("inside"),
            Some(StrokePosition::Inside)
        );
        assert_eq!(
            StrokePosition::parse("outside"),
            Some(StrokePosition::Outside)
        );
        assert_eq!(StrokePosition::parse("middle"), None);
        assert_eq!(StrokePosition::parse("Center"), None);
        assert_eq!(
            StrokePosition::ACCEPTED,
            ["center", "inside", "outside"]
        );
        assert_eq!(StrokePosition::default(), StrokePosition::Inside);
        assert_eq!(BlendMode::parse("multiply"), Some(BlendMode::Multiply));
        assert_eq!(BlendMode::parse("screen"), Some(BlendMode::Screen));
        assert_eq!(BlendMode::parse("overlay"), Some(BlendMode::Overlay));
        assert_eq!(BlendMode::parse("normal"), Some(BlendMode::Normal));
        assert_eq!(BlendMode::parse("darken"), None);
        assert_eq!(BlendMode::parse("Multiply"), None);
        assert_eq!(BlendMode::ACCEPTED, ["normal", "multiply", "screen", "overlay"]);
        assert_eq!(BlendMode::default(), BlendMode::Normal);
        assert_eq!(Overflow::parse("hidden"), Some(Overflow::Hidden));
        assert_eq!(Overflow::parse("clip"), Some(Overflow::Clip));
        assert_eq!(Overflow::parse("visible"), Some(Overflow::Visible));
        assert_eq!(Overflow::parse("scroll"), None);
        assert_eq!(Overflow::parse("Hidden"), None);
        assert_eq!(Overflow::ACCEPTED, ["visible", "hidden", "clip"]);
        assert_eq!(Overflow::default(), Overflow::Visible);
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
            grade: super::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
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
