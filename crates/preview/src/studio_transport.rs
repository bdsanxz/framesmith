//! Canvas transport extras for the Studio page (Sprint 17, T005 /
//! CAP-005, J-05, STATE-01, REC-005).
//!
//! The maker drives canvas review with J, K, and L shuttle at 1x, 2x,
//! and 4x (K pauses), arrows stepping exactly one frame, a speed menu
//! from 0.25x to 2x, and a seek bar under the canvas; playback health
//! reports which clock the film followed (the mix or the wall clock)
//! and how long the screen showed no moving picture. A film short of
//! data holds its last frame with the hold reported, never looping a
//! tail snippet.
//!
//! The transport reuses the Sprint 15 player clock: advances scale
//! wall-clock time by the composition fps and the active rate with a
//! sub-frame carry, exactly like [`crate::player::Player::tick`], and
//! the speed menu stops are player menu stops capped at 2x (every
//! canvas stop rides in [`crate::player::SPEED_MENU`]). Shuttle never
//! touches the mix: forward and reverse shuttle only move the
//! serving-session playhead, and the mix keeps owning the playhead
//! while it sounds (health then reports the `mix` clock).
//!
//! Transport moves no composition data: actions scrub, step, play,
//! pause, shuttle, and tick against the serving-session playhead,
//! persist only the menu speed in the hidden history directory next
//! to the layout file — never beside the sources and never outside
//! the project — and stage no patch and record no history turn, so
//! default renders replay to themselves byte-identically. Canvas and
//! timeline selections stay in sync: the transport echoes the same
//! stable `id` and `#index` keys the canvas view syncs.
//!
//! Request bodies use hyphenated keys (`set-speed`, `elapsed-ms`);
//! every style the strip paints uses CSS-like hyphenated vocabulary
//! (`background-color`, `border-radius`, `font-variant-numeric`); the
//! canvas carries no style attributes of its own, so the fail-closed
//! `font-family` comma try-order passes through untouched — authored
//! family lists ride along in written order and unknown families stay
//! fail-closed at render time, never resolved or substituted here.

use std::path::{Path, PathBuf};
use std::time::Instant;

/// File name of the persisted canvas transport settings inside the
/// hidden project directory (next to the layout and canvas files —
/// never beside the sources).
pub const STUDIO_TRANSPORT_FILE: &str = "studio-transport.json";

/// Version of the persisted canvas transport settings.
pub const STUDIO_TRANSPORT_VERSION: u32 = 1;

/// J/K/L shuttle stops: 1x, 2x, and 4x in the shuttled direction, K
/// pausing. Shuttle rates ride the player rate path directly; the
/// speed menu below owns the persisted menu speed.
pub const SHUTTLE_RATES: [f64; 3] = [1.0, 2.0, 4.0];

/// Canvas speed menu stops, 0.25x to 2x: the Sprint 15 player menu
/// stops capped at 2x, so every canvas stop is a player stop the
/// player clock already runs.
pub const TRANSPORT_SPEED_MENU: [f64; 6] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0];

/// Slowest canvas menu speed (0.25x).
pub const TRANSPORT_SPEED_MIN: f64 = 0.25;

/// Fastest canvas menu speed (2x).
pub const TRANSPORT_SPEED_MAX: f64 = 2.0;

/// Default canvas menu speed (1x).
pub const DEFAULT_TRANSPORT_SPEED: f64 = 1.0;

/// Hidden transport-settings path for one project folder.
pub fn transport_path_for(folder: &Path) -> PathBuf {
    folder
        .join(sdk::HISTORY_DIR_NAME)
        .join(STUDIO_TRANSPORT_FILE)
}

/// Prefix every canvas transport refusal carries; nothing moves,
/// stages, or saves on a refusal.
fn refused_transport(message: String) -> String {
    format!("studio refused: {message}")
}

/// One shuttle direction (Sprint 17, T005): forward or reverse while
/// shuttling, idle before the first J or L press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShuttleDirection {
    /// No shuttle press yet; plain play advances forward.
    #[default]
    Idle,
    /// J drives the playhead backward.
    Reverse,
    /// L drives the playhead forward.
    Forward,
}

impl ShuttleDirection {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ShuttleDirection::Idle => "idle",
            ShuttleDirection::Reverse => "reverse",
            ShuttleDirection::Forward => "forward",
        }
    }

    /// Parse one direction spelling; anything else refuses naming the
    /// three.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "idle" => Some(ShuttleDirection::Idle),
            "reverse" => Some(ShuttleDirection::Reverse),
            "forward" => Some(ShuttleDirection::Forward),
            _ => None,
        }
    }
}

/// Live shuttle memory (Sprint 17, T005): the direction, the shuttle
/// rate while J/L-driven, and whether the picture advances. K pauses
/// keeping direction and rate for the readout; the next J or L press
/// restarts its own direction at 1x.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShuttleState {
    /// Which way J/L drives the playhead.
    pub direction: ShuttleDirection,
    /// Shuttle rate while J/L-driven (`Some(1|2|4)`), `None` while a
    /// plain play or the speed menu owns the rate.
    pub shuttle_rate: Option<f64>,
    /// Whether the picture advances on tick.
    pub playing: bool,
}

impl Default for ShuttleState {
    fn default() -> Self {
        Self {
            direction: ShuttleDirection::Idle,
            shuttle_rate: None,
            playing: false,
        }
    }
}

impl ShuttleState {
    /// Press L: while playing forward under shuttle the rate cycles
    /// 1x to 2x to 4x and back to 1x; otherwise forward shuttle
    /// restarts at 1x.
    pub fn press_l(&mut self) {
        if self.playing
            && self.direction == ShuttleDirection::Forward
            && self.shuttle_rate.is_some()
        {
            let next = match self.shuttle_rate {
                Some(r) if (r - 1.0).abs() < f64::EPSILON => 2.0,
                Some(r) if (r - 2.0).abs() < f64::EPSILON => 4.0,
                _ => 1.0,
            };
            self.shuttle_rate = Some(next);
        } else {
            self.direction = ShuttleDirection::Forward;
            self.shuttle_rate = Some(1.0);
            self.playing = true;
        }
    }

    /// Press J: the reverse mirror of [`ShuttleState::press_l`].
    pub fn press_j(&mut self) {
        if self.playing
            && self.direction == ShuttleDirection::Reverse
            && self.shuttle_rate.is_some()
        {
            let next = match self.shuttle_rate {
                Some(r) if (r - 1.0).abs() < f64::EPSILON => 2.0,
                Some(r) if (r - 2.0).abs() < f64::EPSILON => 4.0,
                _ => 1.0,
            };
            self.shuttle_rate = Some(next);
        } else {
            self.direction = ShuttleDirection::Reverse;
            self.shuttle_rate = Some(1.0);
            self.playing = true;
        }
    }

    /// Press K: pause, keeping direction and rate for the readout.
    pub fn press_k(&mut self) {
        self.playing = false;
    }

    /// Plain pause from the strip: pause and hand the rate back to
    /// the speed menu.
    pub fn pause_plain(&mut self) {
        self.playing = false;
        self.shuttle_rate = None;
        if self.direction == ShuttleDirection::Idle {
            self.direction = ShuttleDirection::Forward;
        }
    }

    /// Plain play from the strip: advance forward at the menu speed
    /// (shuttle memory cleared), keeping a shuttled direction for the
    /// readout when one is set.
    pub fn play_plain(&mut self) {
        if self.direction == ShuttleDirection::Idle {
            self.direction = ShuttleDirection::Forward;
        }
        self.shuttle_rate = None;
        self.playing = true;
    }

    /// The rate one tick advances at: the shuttle rate while
    /// J/L-driven, else the menu `speed`.
    pub fn effective_rate(self, speed: f64) -> f64 {
        self.shuttle_rate.unwrap_or(speed)
    }
}

/// The persisted canvas transport settings (Sprint 17, T005): the
/// menu speed only. Shuttle rates stay live playback state — pausing,
/// seeking, or reopening the shell never replays a shuttle — so only
/// validated menu speeds land in the hidden file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportSettings {
    /// Menu speed in TRANSPORT_SPEED_MIN..=TRANSPORT_SPEED_MAX.
    pub speed: f64,
    /// Whether the timeline loop switch is armed (echoed; the canvas
    /// advance itself always holds the last frame instead of looping
    /// a tail snippet).
    pub loop_enabled: bool,
}

/// The default transport settings: 1x with the loop off.
pub fn default_transport_settings() -> TransportSettings {
    TransportSettings {
        speed: DEFAULT_TRANSPORT_SPEED,
        loop_enabled: false,
    }
}

/// What loading the transport settings reported: the settings to
/// honor, whether they came from the project file, and the fallback
/// banner when they did not.
#[derive(Debug, Clone)]
pub struct TransportLoad {
    /// The settings to honor (the default with no file, or on an
    /// unrestorable file).
    pub settings: TransportSettings,
    /// True when the settings came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file exists but
    /// cannot restore.
    pub notice: Option<String>,
}

/// Refuse a menu speed outside 0.25x to 2x, naming the menu bound;
/// the rate stays as it was and nothing saves.
pub fn check_menu_speed(speed: f64) -> Result<f64, String> {
    if !speed.is_finite() || speed < TRANSPORT_SPEED_MIN || speed > TRANSPORT_SPEED_MAX {
        return Err(refused_transport(format!(
            "the canvas `speed` value {speed} is outside the speed menu (0.25x to 2x); pick a menu speed from 0.25 to 2. The rate stays as it was. Nothing was saved."
        )));
    }
    Ok(speed)
}

/// Load the per-project transport settings: a missing file honors
/// the default with no banner; a corrupt, version-wrong, or invalid
/// file honors the default and says so.
pub fn load_transport(folder: &Path) -> TransportLoad {
    let fallback = TransportLoad {
        settings: default_transport_settings(),
        restored: false,
        notice: None,
    };
    let path = transport_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return fallback,
        Err(e) => {
            return TransportLoad {
                settings: default_transport_settings(),
                restored: false,
                notice: Some(format!(
                    "studio: the transport settings {} cannot be read ({e}); playing at 1x instead.",
                    path.display()
                )),
            };
        }
        Ok(raw) => raw,
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Err(e) => {
            return TransportLoad {
                settings: default_transport_settings(),
                restored: false,
                notice: Some(format!(
                    "studio: the transport settings {} are not transport settings ({e}); playing at 1x instead.",
                    path.display()
                )),
            };
        }
        Ok(v) => v,
    };
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version as u32 != STUDIO_TRANSPORT_VERSION {
        return TransportLoad {
            settings: default_transport_settings(),
            restored: false,
            notice: Some(format!(
                "studio: the transport settings {} carry version {version}, this Studio reads version {STUDIO_TRANSPORT_VERSION}; playing at 1x instead.",
                path.display()
            )),
        };
    }
    let speed = value
        .get("speed")
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite());
    let loop_enabled = value.get("loop-enabled").and_then(|v| v.as_bool());
    match (speed, loop_enabled) {
        (Some(speed), Some(loop_enabled)) => match check_menu_speed(speed) {
            Ok(speed) => TransportLoad {
                settings: TransportSettings {
                    speed,
                    loop_enabled,
                },
                restored: true,
                notice: None,
            },
            Err(why) => TransportLoad {
                settings: default_transport_settings(),
                restored: false,
                notice: Some(format!(
                    "studio: the transport settings {} carry an unusable speed: {why} Playing at 1x instead.",
                    path.display()
                )),
            },
        },
        _ => TransportLoad {
            settings: default_transport_settings(),
            restored: false,
            notice: Some(format!(
                "studio: the transport settings {} name no valid `speed` and `loop-enabled`; playing at 1x instead.",
                path.display()
            )),
        },
    }
}

/// Persist validated transport settings: writes only the hidden
/// transport file — creating the hidden directory when missing — so
/// a refused speed leaves the previous settings intact and sources
/// never move.
pub fn save_transport(folder: &Path, settings: &TransportSettings) -> Result<(), String> {
    check_menu_speed(settings.speed).map_err(|why| {
        format!(
            "studio refused: the transport settings cannot save an unusable speed: {why} Nothing was saved."
        )
    })?;
    let path = transport_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_transport(format!(
                "the hidden transport directory {} cannot be prepared: {e}. Nothing was saved.",
                parent.display()
            ))
        })?;
    }
    let raw = serde_json::to_string_pretty(&serde_json::json!({
        "version": STUDIO_TRANSPORT_VERSION,
        "speed": settings.speed,
        "loop-enabled": settings.loop_enabled,
    }))
    .map_err(|e| {
        refused_transport(format!(
            "the transport settings cannot be encoded: {e}. Nothing was saved."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_transport(format!(
            "the transport file {} cannot be written: {e}. Nothing was saved.",
            path.display()
        ))
    })?;
    Ok(())
}

/// One canvas transport request (Sprint 17, T005): every field rides
/// along only when the body names it. Structural parsing only; the
/// serving session validates the move against the timeline.
#[derive(Debug, Clone)]
pub enum TransportRequest {
    /// J/L shuttle press (cycles 1x to 2x to 4x) or K pause.
    ShuttleJ,
    ShuttleK,
    ShuttleL,
    /// Plain play at the menu speed / plain pause.
    Play,
    Pause,
    /// Move exactly one frame forward or back.
    StepForward,
    StepBack,
    /// Park the playhead at `frame`.
    Seek(u64),
    /// Set the menu speed (0.25x to 2x, persisted).
    SetSpeed(f64),
    /// Advance the playhead by wall-clock time at the active rate
    /// (the page calls this per animation frame while playing).
    Tick(f64),
}

/// Every served transport action spelling, for refusal messages.
pub const TRANSPORT_ACTIONS: [&str; 10] = [
    "shuttle-j",
    "shuttle-k",
    "shuttle-l",
    "play",
    "pause",
    "step-forward",
    "step-back",
    "seek",
    "set-speed",
    "tick",
];

/// Read one request number: present and finite, or a refusal naming
/// the key.
fn transport_number(value: &serde_json::Value, key: &str) -> Result<f64, String> {
    let raw = value.get(key).ok_or_else(|| {
        refused_transport(format!(
            "the canvas transport needs `{key}` for this action; pass the {key} the strip shows. Nothing moved."
        ))
    })?;
    let n = raw.as_f64().ok_or_else(|| {
        refused_transport(format!(
            "the canvas transport `{key}` value {raw} is not a number; pass a finite number. Nothing moved."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_transport(format!(
            "the canvas transport `{key}` value {n:?} is not finite; pass a finite number. Nothing moved."
        )));
    }
    Ok(n)
}

/// One parsed canvas transport body: the action with the echoed
/// selection and the composition the strip shows.
#[derive(Debug, Clone)]
pub struct ParsedTransportRequest {
    /// The strip action to drive.
    pub request: TransportRequest,
    /// Echoed selection, when the body names `selection`.
    pub selection: Option<Vec<String>>,
    /// Composition the strip shows, when the body names it.
    pub composition: Option<String>,
}

/// Parse one canvas transport body (`{"action":"shuttle-l"}`,
/// `{"action":"seek","frame":12}`, `{"action":"set-speed","speed":0.5}`,
/// `{"action":"tick","elapsed-ms":66.7}`): the action must name a
/// served spelling and carry its fields. A refused body names its
/// cause and moves nothing.
pub fn parse_transport_body(body: &str) -> Result<ParsedTransportRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_transport(
            "the canvas transport needs a JSON transport body (`{\"action\":\"shuttle-l\"}`); pass the strip action. Nothing moved."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_transport(format!(
            "the canvas transport body is not JSON ({e}). Pass `{{\"action\":\"shuttle-l\"}}` or another strip action. Nothing moved."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_transport(
            "the canvas transport body is not a transport action; pass a JSON object with an `action` key. Nothing moved."
                .to_string(),
        ));
    }
    let action = value
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            refused_transport(format!(
                "the canvas transport names no action; pass one of {}. Nothing moved.",
                TRANSPORT_ACTIONS.join(", ")
            ))
        })?;
    let request = match action {
        "shuttle-j" => TransportRequest::ShuttleJ,
        "shuttle-k" => TransportRequest::ShuttleK,
        "shuttle-l" => TransportRequest::ShuttleL,
        "play" => TransportRequest::Play,
        "pause" => TransportRequest::Pause,
        "step-forward" => TransportRequest::StepForward,
        "step-back" => TransportRequest::StepBack,
        "seek" => {
            let frame = transport_number(&value, "frame")?;
            if frame < 0.0 || frame.fract() != 0.0 {
                return Err(refused_transport(format!(
                    "the canvas transport `frame` value {frame} is not a frame index; pass a whole frame from 0 up. Nothing moved."
                )));
            }
            TransportRequest::Seek(frame as u64)
        }
        "set-speed" => {
            let speed = transport_number(&value, "speed")?;
            TransportRequest::SetSpeed(
                check_menu_speed(speed).map_err(|why| format!("{why} Nothing moved."))?,
            )
        }
        "tick" => {
            let elapsed_ms = transport_number(&value, "elapsed-ms")?;
            if elapsed_ms < 0.0 {
                return Err(refused_transport(format!(
                    "the canvas transport `elapsed-ms` value {elapsed_ms} is negative; pass 0 or more milliseconds of wall-clock time. Nothing moved."
                )));
            }
            TransportRequest::Tick(elapsed_ms / 1000.0)
        }
        _ => {
            return Err(refused_transport(format!(
                "unknown canvas transport action {action:?}: the strip drives {}. Pass one of those actions. Nothing moved.",
                TRANSPORT_ACTIONS.join(", ")
            )));
        }
    };
    let selection = match value.get("selection") {
        None => None,
        Some(v) => {
            let list = v.as_array().ok_or_else(|| {
                refused_transport(format!(
                    "the canvas transport `selection` value {v} is not a clip list; pass declared `id` values or `#index` entries. Nothing moved."
                ))
            })?;
            let mut out = Vec::new();
            for entry in list {
                let name = entry.as_str().ok_or_else(|| {
                    refused_transport(format!(
                        "the canvas transport `selection` entry {entry} is not a clip reference; pass a declared `id` or a `#index` entry. Nothing moved."
                    ))
                })?;
                if name.trim().is_empty() {
                    return Err(refused_transport(
                        "the canvas transport `selection` names an empty clip; pass a declared `id` or a `#index` entry. Nothing moved."
                            .to_string(),
                    ));
                }
                out.push(name.to_string());
            }
            Some(out)
        }
    };
    let composition = match value.get("composition") {
        None => None,
        Some(v) => {
            let name = v.as_str().ok_or_else(|| {
                refused_transport(format!(
                    "the canvas transport `composition` value {v} is not a composition name; pass the composition the strip shows. Nothing moved."
                ))
            })?;
            if name.trim().is_empty() {
                return Err(refused_transport(
                    "the canvas transport `composition` names an empty composition; pass the composition the strip shows. Nothing moved."
                        .to_string(),
                ));
            }
            Some(name.to_string())
        }
    };
    Ok(ParsedTransportRequest {
        request,
        selection,
        composition,
    })
}

/// Advance the playhead by wall-clock time at the active rate
/// (Sprint 17, T005): the Sprint 15 player clock — `elapsed_secs` of
/// wall time scaled by the composition fps and the rate, with the
/// sub-frame `carry` kept across ticks so consecutive ticks advance
/// exactly instead of dropping fractions. Forward parks on the last
/// frame (holding it, never looping a tail snippet); reverse clamps
/// on frame 0. Returns the new position, the kept carry, and whether
/// the advance parked at a bound.
pub fn advance_position(
    position: u64,
    frame_count: u64,
    fps_value: f64,
    elapsed_secs: f64,
    rate: f64,
    reverse: bool,
    carry: f64,
) -> (u64, f64, bool) {
    if frame_count == 0 || !fps_value.is_finite() || fps_value <= 0.0 {
        return (position, 0.0, false);
    }
    if !elapsed_secs.is_finite() || elapsed_secs < 0.0 || !rate.is_finite() || rate <= 0.0 {
        return (position, carry.max(0.0), false);
    }
    let delta = elapsed_secs * fps_value * rate + carry;
    if !delta.is_finite() || delta <= 0.0 {
        return (position, delta.max(0.0), false);
    }
    let whole = delta.floor() as u64;
    let kept = delta - whole as f64;
    if whole == 0 {
        return (position, kept, false);
    }
    let last = frame_count - 1;
    if reverse {
        match position.checked_sub(whole) {
            Some(next) => (next, kept, false),
            None => (0, 0.0, true),
        }
    } else {
        match position.saturating_add(whole) {
            next if next >= last => (last, 0.0, true),
            next => (next, kept, false),
        }
    }
}

/// Which clock the film followed (Sprint 17, T005): the mix while it
/// sounds and the picture advances, the wall clock while a silent
/// film advances, paused while the playhead rests, and disabled while
/// no film can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackClock {
    /// The mix owns the playhead (a sounding film advancing).
    Mix,
    /// Wall-clock ticks own the playhead (a silent film advancing).
    Wall,
    /// The playhead rests (paused or never started).
    Paused,
    /// No film can show (empty or missing timeline).
    Disabled,
}

impl PlaybackClock {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackClock::Mix => "mix",
            PlaybackClock::Wall => "wall",
            PlaybackClock::Paused => "paused",
            PlaybackClock::Disabled => "disabled",
        }
    }
}

/// Resolve which clock the film followed: disabled with no film,
/// paused while the playhead rests, the mix while a sounding film
/// advances, else the wall clock.
pub fn resolve_clock(frame_count: u64, playing: bool, has_audio: bool) -> PlaybackClock {
    if frame_count == 0 {
        PlaybackClock::Disabled
    } else if !playing {
        PlaybackClock::Paused
    } else if has_audio {
        PlaybackClock::Mix
    } else {
        PlaybackClock::Wall
    }
}

/// Why the canvas holds a bound frame (Sprint 17, T005 / REC-005):
/// the film is short of data past the bound, so the bound frame
/// holds with the hold reported instead of looping a snippet.
pub fn hold_last_message(last: u64, frame_count: u64) -> String {
    format!(
        "holding last frame {last} of {frame_count}: the film is short of data past frame {last}; the last frame holds instead of looping. Restore the media to play on."
    )
}

/// Why the canvas holds the first frame under reverse shuttle: the
/// film starts at frame 0, so frame 0 holds instead of wrapping.
pub fn hold_first_message(frame_count: u64) -> String {
    format!(
        "holding first frame 0 of {frame_count}: reverse shuttle reached the start of the film; frame 0 holds instead of wrapping."
    )
}

/// One-line playback health: which clock the film followed, how long
/// the screen showed no moving picture, and the hold while one
/// applies.
pub fn health_detail(clock: PlaybackClock, still_secs: f64, held: Option<&str>) -> String {
    let clock_word = match clock {
        PlaybackClock::Mix => "clock mix (the mix owns the playhead)",
        PlaybackClock::Wall => "clock wall (wall-clock ticks own the playhead)",
        PlaybackClock::Paused => "clock paused (the playhead rests)",
        PlaybackClock::Disabled => "clock disabled (no film can show)",
    };
    match held {
        Some(hold) => {
            format!("playback health: {clock_word}; still picture {still_secs:.1}s; {hold}")
        }
        None => format!("playback health: {clock_word}; still picture {still_secs:.1}s."),
    }
}

/// Live canvas transport state the server keeps per session (Sprint
/// 17, T005): shuttle memory, the sub-frame carry, and the
/// still-picture watch. Pure playback state: nothing here touches
/// composition sources or history.
#[derive(Debug, Clone)]
pub struct StudioTransportLive {
    /// Shuttle memory (direction, rate, playing).
    pub shuttle: ShuttleState,
    /// Sub-frame advance below one frame, kept in `[0, 1)`.
    pub carry: f64,
    /// Last shown frame index behind the still-picture watch.
    pub last_frame: u64,
    /// When `last_frame` started showing.
    pub last_change: Instant,
    /// Hold report while the canvas holds a bound frame.
    pub held: Option<String>,
}

impl StudioTransportLive {
    /// Fresh live state parked at `frame` from `now`.
    pub fn new(frame: u64, now: Instant) -> Self {
        Self {
            shuttle: ShuttleState::default(),
            carry: 0.0,
            last_frame: frame,
            last_change: now,
            held: None,
        }
    }

    /// Note the shown frame at `now`: a changed frame restarts the
    /// still-picture watch and clears any hold.
    pub fn note_frame(&mut self, frame: u64, now: Instant) {
        if frame != self.last_frame {
            self.last_frame = frame;
            self.last_change = now;
            self.held = None;
        }
        self.carry = 0.0;
    }

    /// How long the screen showed no moving picture, in seconds.
    pub fn still_secs(&self, now: Instant) -> f64 {
        now.duration_since(self.last_change).as_secs_f64().max(0.0)
    }
}

/// Render one canvas transport state as JSON: the playhead with its
/// readout, the shuttle, the 0.25x to 2x speed menu, the seek bar
/// under the canvas, playback health, the synced selection, and the
/// `font-family` pass-through. Every key is hyphenated; every style
/// the strip paints from this state uses CSS-like hyphenated
/// vocabulary.
#[allow(clippy::too_many_arguments)]
pub fn transport_state_json(
    settings: &TransportSettings,
    settings_restored: bool,
    settings_notice: Option<&str>,
    shuttle: &ShuttleState,
    frame: u64,
    frame_count: u64,
    time_secs: f64,
    duration_secs: f64,
    fps_value: f64,
    playing: bool,
    enabled: bool,
    empty_reason: Option<&str>,
    has_audio: bool,
    still_secs: f64,
    held: Option<&str>,
    detail: &str,
    minimap_frame: u64,
    kept: &[String],
    dropped: &[String],
    font_families: &[String],
) -> String {
    let clock = resolve_clock(frame_count, playing, has_audio);
    let held_json = match held {
        Some(h) => format!("\"{}\"", crate::studio::json_escape(h)),
        None => "null".to_string(),
    };
    let reason = match empty_reason {
        Some(r) => format!("\"{}\"", crate::studio::json_escape(r)),
        None => "null".to_string(),
    };
    let notice = match settings_notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    let kept_json = kept
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let dropped_json = dropped
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let fonts_json = font_families
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let menu_json = TRANSPORT_SPEED_MENU
        .iter()
        .map(|r| format!("{r}"))
        .collect::<Vec<_>>()
        .join(",");
    let shuttle_rate = match shuttle.shuttle_rate {
        Some(r) => format!("{r}"),
        None => "null".to_string(),
    };
    let health = health_detail(clock, still_secs, held);
    format!(
        "{{\"frame\":{frame},\"frame-count\":{frame_count},\"time-secs\":{time_secs:.3},\"duration-secs\":{duration_secs:.3},\"fps\":{fps_value},\"playing\":{playing},\"enabled\":{enabled},\"empty\":{},\"reason\":{reason},\"transport-enabled\":{enabled},\"speed\":{},\"speed-menu\":[{menu_json}],\"speed-restored\":{settings_restored},\"speed-notice\":{notice},\"loop-enabled\":{},\"shuttle\":{{\"direction\":\"{}\",\"rate\":{shuttle_rate},\"playing\":{}}},\"seek-bar\":{{\"min\":0,\"max\":{},\"value\":{frame}}},\"health\":{{\"clock\":\"{}\",\"still-secs\":{still_secs:.3},\"held\":{held_json},\"detail\":\"{}\"}},\"status-line\":\"{}\",\"detail\":\"{}\",\"minimap-frame\":{minimap_frame},\"selection\":{{\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]}},\"font-families\":[{fonts_json}]}}",
        frame_count == 0,
        settings.speed,
        settings.loop_enabled,
        shuttle.direction.as_str(),
        shuttle.playing,
        frame_count.saturating_sub(1),
        clock.as_str(),
        crate::studio::json_escape(&health),
        crate::studio::json_escape(&health),
        crate::studio::json_escape(detail),
    )
}

/// One-line transport summary for the strip status: the shuttle with
/// its rate, the frame, and the speed.
#[allow(clippy::too_many_arguments)]
pub fn transport_detail(
    shuttle: &ShuttleState,
    speed: f64,
    frame: u64,
    frame_count: u64,
    playing: bool,
) -> String {
    let drive = match (shuttle.direction, shuttle.shuttle_rate) {
        (ShuttleDirection::Forward, Some(r)) => format!("shuttle forward {r}x"),
        (ShuttleDirection::Reverse, Some(r)) => format!("shuttle reverse {r}x"),
        (ShuttleDirection::Forward, None) if playing => format!("play forward {speed}x"),
        (ShuttleDirection::Reverse, None) if playing => format!("play reverse {speed}x"),
        (dir, _) => format!("{} paused", dir.as_str()),
    };
    format!("canvas transport: {drive}; frame {frame}/{frame_count} at {speed}x.")
}

/// Extra CSS the canvas transport strip needs (Sprint 17, T005): the
/// J/K/L and step buttons, the speed menu, the seek bar under the
/// canvas, and the health and hold lines. Style attributes stay
/// CSS-like hyphenated end to end (`background-color`,
/// `border-radius`, `font-variant-numeric`).
pub fn transport_css() -> String {
    r#"        #canvas-transport-bar{margin-top:8px;display:flex;gap:8px;align-items:center;flex-wrap:wrap;background-color:var(--panel-color);border:1px solid var(--line-color);border-radius:4px;padding:8px 12px}
        #canvas-transport-bar button{margin-right:4px}
        #transport-health{font-variant-numeric:tabular-nums}
        #transport-seek{flex:1;min-width:160px}
        #transport-hold{border:1px dashed var(--line-color);border-radius:4px;padding:8px 12px;margin-top:8px}"#.to_string()
}

/// The canvas transport strip (Sprint 17, T005): J/K/L shuttle with K
/// pausing, one-frame step buttons, the 0.25x to 2x speed menu, the
/// seek bar under the canvas, and the playback health and hold
/// lines.
pub fn transport_html() -> String {
    let mut options = String::new();
    for rate in TRANSPORT_SPEED_MENU {
        let selected = if (rate - DEFAULT_TRANSPORT_SPEED).abs() < f64::EPSILON {
            " selected"
        } else {
            ""
        };
        let label = if rate.fract() == 0.0 {
            format!("{rate:.0}x")
        } else {
            format!("{rate}x")
        };
        options.push_str(&format!(
            "<option value=\"{rate}\"{selected}>{label}</option>"
        ));
    }
    format!(
        r#"<div class="dock-bar" id="canvas-transport-bar" tabindex="0" aria-label="canvas transport: J, K, L shuttle; arrows step one frame">
        <button id="transport-j" type="button" title="shuttle reverse (J)">J</button>
        <button id="transport-k" type="button" title="pause (K)">K</button>
        <button id="transport-l" type="button" title="shuttle forward (L)">L</button>
        <button id="transport-step-back" type="button" title="step one frame back">step back</button>
        <button id="transport-step-forward" type="button" title="step one frame forward">step forward</button>
        <button id="transport-play" type="button">play</button>
        <button id="transport-pause" type="button">pause</button>
        <label>speed <select id="transport-speed">{options}</select></label>
        <label>seek <input id="transport-seek" type="range" min="0" max="0" value="0"></label>
        <span id="transport-shuttle" role="status">idle paused</span>
        </div>
        <p id="transport-health" role="status">playback health: waiting for the canvas.</p>
        <p id="transport-hold" role="alert" hidden></p>"#
    )
}

/// The canvas transport strip script (Sprint 17, T005): paints the
/// shuttle, the 0.25x to 2x speed menu, the seek bar under the canvas
/// with the playhead, and playback health with its hold; posts strip
/// actions and keeps the strip selection on the shared selection the
/// canvas and timeline paint. J, K, and L drive the shuttle only
/// while the strip has focus, so the shell shortcuts keep working
/// everywhere else.
pub fn transport_js() -> String {
    r#"        let transportView = null;
        function transportStatusText(t) { const s = document.getElementById('transport-health'); if (s) s.textContent = t; }
        function paintTransportHold(held) {
          const box = document.getElementById('transport-hold');
          if (!box) return;
          if (!held) { box.hidden = true; box.textContent = ''; return; }
          box.hidden = false;
          box.textContent = held;
        }
        async function paintTransport() {
          try {
            const q = new URLSearchParams();
            q.set('composition', (rows && rows.composition) || boot.composition);
            if (selection.length) q.set('selection', selection.join(','));
            const j = await (await fetch('/api/studio/transport?' + q.toString())).json();
            transportView = j;
            const shuttle = document.getElementById('transport-shuttle');
            if (shuttle) shuttle.textContent = j.shuttle.direction + ' ' + (j.shuttle.rate === null ? j.speed + 'x' : j.shuttle.rate + 'x') + ' ' + (j.playing ? 'playing' : 'paused');
            const speed = document.getElementById('transport-speed');
            if (speed) speed.value = String(j.speed);
            const seek = document.getElementById('transport-seek');
            if (seek) { seek.max = String(j['seek-bar'].max); seek.value = String(j['seek-bar'].value); seek.disabled = !j.enabled; }
            for (const id of ['transport-j','transport-k','transport-l','transport-step-back','transport-step-forward','transport-play','transport-pause']) {
              const b = document.getElementById(id); if (b) b.disabled = !j.enabled && (id !== 'transport-k' && id !== 'transport-pause');
            }
            transportStatusText(j.health.detail);
            paintTransportHold(j.health.held);
            if (j.selection && j.selection.kept) {
              const dropped = j.selection.dropped || [];
              if (dropped.length) transportStatusText(j.health.detail + ' dropped unknown selection: ' + dropped.join(', '));
            }
          } catch (e) { transportStatusText('transport unavailable: ' + e); }
        }
        async function postTransport(payload) {
          try {
            const r = await fetch('/api/studio/transport', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(payload)});
            const t = await r.text();
            if (!r.ok) { transportStatusText(t); banner(t); return; }
            transportView = JSON.parse(t);
            const shuttle = document.getElementById('transport-shuttle');
            if (shuttle) shuttle.textContent = transportView.shuttle.direction + ' ' + (transportView.shuttle.rate === null ? transportView.speed + 'x' : transportView.shuttle.rate + 'x') + ' ' + (transportView.playing ? 'playing' : 'paused');
            const seek = document.getElementById('transport-seek');
            if (seek) { seek.max = String(transportView['seek-bar'].max); seek.value = String(transportView['seek-bar'].value); }
            transportStatusText(transportView.health.detail);
            paintTransportHold(transportView.health.held);
            if (typeof show === 'function') show(transportView.frame);
          } catch (e) { transportStatusText('transport failed: ' + e); }
        }
        function wireTransportButton(id, payload) {
          const el = document.getElementById(id);
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('click', () => postTransport(payload)); }
        }
        wireTransportButton('transport-j', {action: 'shuttle-j'});
        wireTransportButton('transport-k', {action: 'shuttle-k'});
        wireTransportButton('transport-l', {action: 'shuttle-l'});
        wireTransportButton('transport-step-back', {action: 'step-back'});
        wireTransportButton('transport-step-forward', {action: 'step-forward'});
        wireTransportButton('transport-play', {action: 'play'});
        wireTransportButton('transport-pause', {action: 'pause'});
        const transportSpeed = document.getElementById('transport-speed');
        if (transportSpeed && !transportSpeed.dataset.wired) { transportSpeed.dataset.wired = '1'; transportSpeed.addEventListener('change', () => postTransport({action: 'set-speed', speed: Number(transportSpeed.value)})); }
        const transportSeek = document.getElementById('transport-seek');
        if (transportSeek && !transportSeek.dataset.wired) { transportSeek.dataset.wired = '1'; transportSeek.addEventListener('change', () => postTransport({action: 'seek', frame: Number(transportSeek.value)})); }
        const transportBar = document.getElementById('canvas-transport-bar');
        if (transportBar && !transportBar.dataset.wiredKeys) {
          transportBar.dataset.wiredKeys = '1';
          transportBar.addEventListener('keydown', (e) => {
            const tag = (e.target && e.target.tagName) || '';
            if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
            const k = String(e.key || '').toLowerCase();
            if (k === 'j' || k === 'k' || k === 'l') { e.preventDefault(); e.stopPropagation(); postTransport({action: 'shuttle-' + k}); }
            else if (k === 'arrowright') { e.preventDefault(); e.stopPropagation(); postTransport({action: 'step-forward'}); }
            else if (k === 'arrowleft') { e.preventDefault(); e.stopPropagation(); postTransport({action: 'step-back'}); }
          });
        }
        paintTransport();"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuttle_l_cycles_1x_2x_4x_then_wraps() {
        let mut s = ShuttleState::default();
        s.press_l();
        assert_eq!(
            (s.direction, s.shuttle_rate, s.playing),
            (ShuttleDirection::Forward, Some(1.0), true)
        );
        s.press_l();
        assert_eq!(s.shuttle_rate, Some(2.0));
        s.press_l();
        assert_eq!(s.shuttle_rate, Some(4.0));
        s.press_l();
        assert_eq!(s.shuttle_rate, Some(1.0));
    }

    #[test]
    fn shuttle_j_mirrors_l_in_reverse() {
        let mut s = ShuttleState::default();
        s.press_j();
        assert_eq!(
            (s.direction, s.shuttle_rate, s.playing),
            (ShuttleDirection::Reverse, Some(1.0), true)
        );
        s.press_j();
        assert_eq!(s.shuttle_rate, Some(2.0));
        s.press_j();
        assert_eq!(s.shuttle_rate, Some(4.0));
        s.press_j();
        assert_eq!(s.shuttle_rate, Some(1.0));
    }

    #[test]
    fn shuttle_k_pauses_keeping_direction_and_rate() {
        let mut s = ShuttleState::default();
        s.press_l();
        s.press_l();
        s.press_k();
        assert!(!s.playing);
        assert_eq!(s.direction, ShuttleDirection::Forward);
        assert_eq!(s.shuttle_rate, Some(2.0));
        // The next L restarts forward at 1x.
        s.press_l();
        assert_eq!(
            (s.direction, s.shuttle_rate, s.playing),
            (ShuttleDirection::Forward, Some(1.0), true)
        );
    }

    #[test]
    fn shuttle_directions_restart_each_other_at_1x() {
        let mut s = ShuttleState::default();
        s.press_l();
        s.press_l();
        s.press_j();
        assert_eq!(
            (s.direction, s.shuttle_rate, s.playing),
            (ShuttleDirection::Reverse, Some(1.0), true)
        );
    }

    #[test]
    fn speed_menu_runs_0_25x_to_2x_inside_the_player_menu() {
        assert_eq!(TRANSPORT_SPEED_MENU[0], TRANSPORT_SPEED_MIN);
        assert_eq!(
            TRANSPORT_SPEED_MENU[TRANSPORT_SPEED_MENU.len() - 1],
            TRANSPORT_SPEED_MAX
        );
        for rate in TRANSPORT_SPEED_MENU {
            assert!(
                crate::player::is_menu_rate(rate),
                "canvas stop {rate}x rides the player clock menu"
            );
            assert!(check_menu_speed(rate).is_ok());
        }
        for bad in [0.1, 2.5, 4.0, 0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                check_menu_speed(bad).is_err(),
                "menu refuses {bad} naming the 0.25x to 2x bound"
            );
        }
    }

    #[test]
    fn advance_position_follows_the_player_clock_math() {
        // 10 fps, 40 frames, 1s at 1x advances exactly 10 frames.
        let (pos, carry, parked) = advance_position(0, 40, 10.0, 1.0, 1.0, false, 0.0);
        assert_eq!((pos, parked), (10, false));
        assert!(carry.abs() < 1e-9);
        // 2x doubles the advance; fractions carry instead of dropping.
        let (pos, carry, _) = advance_position(0, 40, 10.0, 0.15, 2.0, false, 0.0);
        assert_eq!(pos, 3);
        assert!((carry - 0.0).abs() < 1e-9);
        let (pos, carry, _) = advance_position(0, 40, 10.0, 0.11, 1.0, false, 0.0);
        assert_eq!(pos, 1);
        assert!((carry - 0.1).abs() < 1e-9);
        let (pos, _, _) = advance_position(pos, 40, 10.0, 0.11, 1.0, false, carry);
        assert_eq!(pos, 2, "the carried fraction lands next tick");
    }

    #[test]
    fn advance_position_parks_at_the_bounds_for_the_hold() {
        let (pos, _, parked) = advance_position(38, 40, 10.0, 1.0, 1.0, false, 0.0);
        assert_eq!((pos, parked), (39, true));
        let (pos, _, parked) = advance_position(2, 40, 10.0, 1.0, 1.0, true, 0.0);
        assert_eq!((pos, parked), (0, true));
        let (pos, _, parked) = advance_position(0, 40, 10.0, 1.0, 1.0, true, 0.0);
        assert_eq!((pos, parked), (0, true));
        let (pos, _, parked) = advance_position(5, 0, 10.0, 1.0, 1.0, false, 0.0);
        assert_eq!((pos, parked), (5, false));
    }

    #[test]
    fn transport_bodies_parse_every_action_and_refuse_every_gap() {
        let parsed = parse_transport_body("{\"action\":\"shuttle-l\"}").expect("shuttle-l");
        assert!(matches!(parsed.request, TransportRequest::ShuttleL));
        let parsed = parse_transport_body("{\"action\":\"seek\",\"frame\":12}").expect("seek");
        assert!(matches!(parsed.request, TransportRequest::Seek(12)));
        let parsed = parse_transport_body(
            "{\"action\":\"step-forward\",\"selection\":[\"a\",\"#1\"],\"composition\":\"t\"}",
        )
        .expect("echo");
        assert!(matches!(parsed.request, TransportRequest::StepForward));
        assert_eq!(
            parsed.selection,
            Some(vec!["a".to_string(), "#1".to_string()])
        );
        assert_eq!(parsed.composition, Some("t".to_string()));
        let parsed =
            parse_transport_body("{\"action\":\"set-speed\",\"speed\":0.5}").expect("speed");
        assert!(matches!(parsed.request, TransportRequest::SetSpeed(v) if (v - 0.5).abs() < 1e-9));
        let parsed =
            parse_transport_body("{\"action\":\"tick\",\"elapsed-ms\":100}").expect("tick");
        assert!(matches!(parsed.request, TransportRequest::Tick(v) if (v - 0.1).abs() < 1e-9));
        for body in [
            "",
            "not json",
            "[1,2]",
            "{}",
            "{\"action\":\"fly\"}",
            "{\"action\":\"seek\"}",
            "{\"action\":\"seek\",\"frame\":-1}",
            "{\"action\":\"seek\",\"frame\":1.5}",
            "{\"action\":\"seek\",\"frame\":\"far\"}",
            "{\"action\":\"set-speed\"}",
            "{\"action\":\"set-speed\",\"speed\":4}",
            "{\"action\":\"set-speed\",\"speed\":0.1}",
            "{\"action\":\"tick\"}",
            "{\"action\":\"tick\",\"elapsed-ms\":-5}",
            "{\"action\":\"play\",\"selection\":[42]}",
            "{\"action\":\"play\",\"selection\":[\"\"]}",
            "{\"action\":\"play\",\"composition\":42}",
        ] {
            let err = parse_transport_body(body).expect_err(&format!("refuses {body:?}"));
            assert!(err.starts_with("studio refused: "), "refusal prefix: {err}");
            assert!(err.contains("Nothing moved"), "nothing moved: {err}");
        }
    }

    #[test]
    fn health_names_the_clock_and_the_still_picture_with_the_hold() {
        assert_eq!(resolve_clock(0, true, true), PlaybackClock::Disabled);
        assert_eq!(resolve_clock(40, false, true), PlaybackClock::Paused);
        assert_eq!(resolve_clock(40, true, true), PlaybackClock::Mix);
        assert_eq!(resolve_clock(40, true, false), PlaybackClock::Wall);
        let hold = hold_last_message(39, 40);
        assert!(hold.contains("39 of 40"), "hold names the frame: {hold}");
        assert!(
            hold.contains("instead of looping"),
            "hold rules out loops: {hold}"
        );
        let detail = health_detail(PlaybackClock::Mix, 12.34, Some(&hold));
        assert!(detail.contains("clock mix"), "names the clock: {detail}");
        assert!(
            detail.contains("12.3s"),
            "names the still duration: {detail}"
        );
        assert!(detail.contains("39 of 40"), "reports the hold: {detail}");
        let moving = health_detail(PlaybackClock::Wall, 0.04, None);
        assert!(moving.contains("clock wall"), "wall clock: {moving}");
        assert!(moving.contains("0.0s"), "fresh picture: {moving}");
    }

    #[test]
    fn transport_state_keys_stay_hyphenated() {
        let shuttle = ShuttleState::default();
        let settings = default_transport_settings();
        let state = transport_state_json(
            &settings,
            false,
            None,
            &shuttle,
            3,
            40,
            0.3,
            4.0,
            10.0,
            false,
            true,
            None,
            false,
            1.2,
            None,
            "canvas transport: idle paused; frame 3/40 at 1x.",
            3,
            &[],
            &[],
            &["Nope, Fallback".to_string()],
        );
        for key in [
            "\"frame-count\"",
            "\"time-secs\"",
            "\"duration-secs\"",
            "\"transport-enabled\"",
            "\"speed-menu\"",
            "\"speed-restored\"",
            "\"speed-notice\"",
            "\"loop-enabled\"",
            "\"seek-bar\"",
            "\"still-secs\"",
            "\"status-line\"",
            "\"minimap-frame\"",
            "\"font-families\"",
        ] {
            assert!(
                state.contains(key),
                "hyphenated key {key} rides along: {state}"
            );
        }
        assert!(
            !state.contains("frame_count") && !state.contains("time_secs"),
            "no snake_case keys leak: {state}"
        );
        assert!(
            state.contains("Nope, Fallback"),
            "font try-order passes through in comma order: {state}"
        );
    }

    #[test]
    fn transport_settings_round_trip_through_the_hidden_file_only() {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-studio-transport-unit-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings = TransportSettings {
            speed: 0.5,
            loop_enabled: true,
        };
        save_transport(&dir, &settings).expect("valid speed saves");
        let load = load_transport(&dir);
        assert!(load.restored);
        assert_eq!(load.settings, settings);
        let top: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            top,
            vec![sdk::HISTORY_DIR_NAME.to_string()],
            "only the hidden history directory lands: {top:?}"
        );
        assert!(
            std::fs::read_to_string(transport_path_for(&dir))
                .unwrap()
                .contains("\"speed\": 0.5"),
            "the hidden file carries the hyphenated speed"
        );
        let bad = TransportSettings {
            speed: 4.0,
            loop_enabled: false,
        };
        assert!(save_transport(&dir, &bad).is_err(), "4x never persists");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
