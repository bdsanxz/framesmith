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

/// A `text` clip (Sprint 1 usable element, plus Sprint 2 paint order,
/// line height and letter spacing).
#[derive(Debug, Clone)]
pub struct TextClip {
    pub value: String,
    pub start: f64,
    pub duration: f64,
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
    pub line: usize,
    pub column: usize,
}

/// A basic `video` clip (Sprint 1 usable element, plus Sprint 2 order,
/// plus Sprint 3 sound keys): the clip's sound enters the 48 kHz stereo
/// mix with the declared `volume_db` (dB, `-inf` is exact silence) and
/// linear `fade_in`/`fade_out` (seconds) applied, sample-aligned with
/// the picture. Without declared keys the sound mixes at unity gain.
#[derive(Debug, Clone)]
pub struct VideoClip {
    pub path: String,
    pub start: f64,
    pub duration: f64,
    pub fit: Fit,
    pub order: i64,
    /// Sound gain in dB (`f64::NEG_INFINITY` is exact silence); default 0.
    pub volume_db: f64,
    /// Linear fade in, in seconds; default 0.
    pub fade_in: f64,
    /// Linear fade out, in seconds; default 0.
    pub fade_out: f64,
    pub line: usize,
    pub column: usize,
}

/// An `audio` track (Sprint 3 element): a sound source placed in a
/// half-open window with trim, volume, and linear fades. `trim_start`
/// and `trim_end` select the source window in seconds (`trim_end` none
/// means the source end); `volume_db` is the gain in dB (`-inf` is
/// exact silence, values above +12 are refused); `fade_in`/`fade_out`
/// are linear amplitude ramps in seconds over the clip window.
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub path: String,
    pub start: f64,
    pub duration: f64,
    pub trim_start: f64,
    pub trim_end: Option<f64>,
    pub volume_db: f64,
    pub fade_in: f64,
    pub fade_out: f64,
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
        Self { x: PlaceH::Center, y: PlaceV::Center }
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
    pub line: usize,
    pub column: usize,
}

/// An `image` clip (Sprint 2 element): a PNG or JPEG source placed with
/// a fit and a placement inside its target rect.
#[derive(Debug, Clone)]
pub struct ImageClip {
    pub path: String,
    pub start: f64,
    pub duration: f64,
    pub x: f64,
    pub y: f64,
    /// Target rect size; defaults to the composition canvas.
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fit: Fit,
    pub placement: Placement,
    pub order: i64,
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

/// One easing curve of a `tween` (T003): the nineteen named curves of
/// `CAP-009` plus parameterized `out-back(n)`. Later easings
/// (`cubic-bezier`, `steps`, `spring`, `bounce`, `elastic`, `wiggle`,
/// `hold`) are refused as not-yet-supported, never parsed here.
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

    /// Later easings refused as not-yet-supported (Sprint 7 motion proof).
    pub const LATER: [&'static str; 7] = [
        "cubic-bezier",
        "steps",
        "spring",
        "bounce",
        "elastic",
        "wiggle",
        "hold",
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
        }
    }
}

/// A validated `tween` directive (T003): smooth motion of one property
/// of one clip over a half-open window. Drawing (T009/T010/T012)
/// evaluates these; validation only guarantees they are well-defined.
#[derive(Debug, Clone)]
pub struct TweenDecl {
    /// Document index of the target clip.
    pub target: usize,
    /// One of `x`, `y`, `scale`, `rotation`, `opacity`.
    pub property: String,
    pub kind: TweenKind,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub start: f64,
    pub duration: f64,
    pub ease: EaseDecl,
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
    /// One of `x`, `y`, `scale`, `rotation`, `opacity`.
    pub property: String,
    pub value: f64,
    pub start: f64,
    pub duration: f64,
    pub line: usize,
    pub column: usize,
}

/// Properties tweenable and settable in Sprint 2. Anything else is
/// refused as not-yet-supported with its enabling sprint (T003).
pub const MOTION_PROPERTIES: [&str; 5] = ["x", "y", "scale", "rotation", "opacity"];

/// Well-formed motion properties that wait for Sprint 7 motion proof.
pub const LATER_PROPERTIES: [&str; 13] = [
    "scale-x",
    "scale-y",
    "skew",
    "skew-x",
    "skew-y",
    "rotate-x",
    "rotate-y",
    "rotation-x",
    "rotation-y",
    "blur",
    "brightness",
    "count",
    "typed",
];

#[derive(Debug, Clone)]
pub enum Clip {
    Text(TextClip),
    Video(VideoClip),
    Box(BoxClip),
    Image(ImageClip),
    Audio(AudioClip),
}

impl Clip {
    pub fn start(&self) -> f64 {
        match self {
            Clip::Text(c) => c.start,
            Clip::Video(c) => c.start,
            Clip::Box(c) => c.start,
            Clip::Image(c) => c.start,
            Clip::Audio(c) => c.start,
        }
    }

    pub fn duration(&self) -> f64 {
        match self {
            Clip::Text(c) => c.duration,
            Clip::Video(c) => c.duration,
            Clip::Box(c) => c.duration,
            Clip::Image(c) => c.duration,
            Clip::Audio(c) => c.duration,
        }
    }

    /// Explicit paint order (default 0); ties fall back to document order.
    /// Audio tracks draw no picture, so they carry order 0 and never
    /// take part in paint sorting.
    pub fn order(&self) -> i64 {
        match self {
            Clip::Text(c) => c.order,
            Clip::Video(c) => c.order,
            Clip::Box(c) => c.order,
            Clip::Image(c) => c.order,
            Clip::Audio(_) => 0,
        }
    }
}

/// Optional project configuration file name, read from the folder root.
pub const CONFIG_FILE: &str = "framesmith.toml";

/// Validated composition root (CAP-002 / T003).
#[derive(Debug, Clone)]
pub struct Composition {
    pub name: String,
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
            Some(Rgb { r: 255, g: 255, b: 255 })
        );
        assert_eq!(parse_color("#1A1A2e").map(|c| c.to_hex()), Some("1a1a2e".into()));
        assert_eq!(parse_color("#abc").map(|c| c.to_hex()), Some("aabbcc".into()));
        assert!(parse_color("ffffff").is_none());
        assert!(parse_color("#ffff").is_none());
        assert!(parse_color("#gggggg").is_none());
    }

    #[test]
    fn fps_math() {
        let f = Fps::from_int(30);
        assert_eq!(f.value(), 30.0);
        assert_eq!(f.frame_count(3.0), 90);
        let ntsc = Fps { num: 30000, den: 1001 };
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
}
