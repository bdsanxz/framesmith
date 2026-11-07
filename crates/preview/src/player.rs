//! Embeddable player (Sprint 15, T005 / CAP-001, J-01, STATE-02..03,
//! REC-005).
//!
//! A host embeds the player with a composition source plus attributes
//! (autoplay, loop, muted, controls toggleable at any time,
//! click-to-play disable, no-load-timeout); the player loads and plays
//! the composition's frames and mix, exposes methods (play, pause,
//! seek, time, duration, playback rate, volume, mute, range playback),
//! and fires events (ready, state, timeline, assets-ready, painted,
//! playback) in that canonical order. Loading ends in ready or a named
//! failure within 8 seconds unless no-load-timeout disables the
//! deadline; an unloadable composition reports its cause and shows no
//! guessed frame. With no height given the film shows at its own shape
//! (an `aspect-ratio` style from the canvas).
//!
//! Attribute names are hyphenated CSS-like words (`click-to-play`,
//! `no-load-timeout`), matching the Sprint 2 notation gate. The player
//! shows the same frames the render pipeline computes (it serves
//! through [`PreviewSession`](crate::PreviewSession), never redraws),
//! writes no cache into the project directory, and never resolves
//! `font-family`: try-order lists pass through end to end and unknown
//! families stay fail-closed at render time.
//!
//! Built-in controls live in T006; this slice is embedding, loading,
//! methods, and events, plus the `play` command surface serving the
//! same player page.

use std::time::Duration;

use notation::model::Fps;

/// Load deadline (Sprint 15, T005): loading ends in ready or a named
/// failure within 8 seconds unless no-load-timeout disables it.
pub const LOAD_DEADLINE_SECS: u64 = 8;

/// [`LOAD_DEADLINE_SECS`] as a [`Duration`].
pub const LOAD_DEADLINE: Duration = Duration::from_secs(LOAD_DEADLINE_SECS);

/// Canonical event order (Sprint 15, T005): ready, state, timeline,
/// assets-ready, painted, playback. Every load-then-play sequence emits
/// a prefix of this order; later method calls append further state,
/// timeline, painted, and playback events in the same relative order.
pub const CANONICAL_ORDER: [&str; 6] = [
    "ready",
    "state",
    "timeline",
    "assets-ready",
    "painted",
    "playback",
];

/// Host embed attributes (Sprint 15, T005 / CAP-001). Hyphenated
/// CSS-like names on the wire (`click-to-play`, `no-load-timeout`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerAttributes {
    /// Start playing as soon as the load is ready.
    pub autoplay: bool,
    /// Wrap past the end instead of stopping.
    pub loop_enabled: bool,
    /// Start silent (volume is kept, output is muted).
    pub muted: bool,
    /// Playback controls are enabled. Toggleable at any time through
    /// [`Player::set_controls`], even mid-playback; on a failed load
    /// controls stay disabled regardless.
    pub controls: bool,
    /// Clicking the film toggles play/pause. When false, clicks never
    /// act ([`Player::click_film`] reports ignored).
    pub click_to_play: bool,
    /// Disable the 8-second load deadline. When false (the default) a
    /// slow load fails with a named timeout instead of hanging.
    pub no_load_timeout: bool,
}

impl Default for PlayerAttributes {
    fn default() -> Self {
        Self {
            autoplay: false,
            loop_enabled: false,
            muted: false,
            controls: true,
            click_to_play: true,
            no_load_timeout: false,
        }
    }
}

impl PlayerAttributes {
    /// Parse embed attributes from a URL query string (`?autoplay=1&…`).
    /// Known keys are `autoplay`, `loop`, `muted`, `controls`,
    /// `click-to-play`, and `no-load-timeout`; `1`/`true`/`on` enable,
    /// `0`/`false`/`off` disable, anything else (and unknown keys) is
    /// ignored so a host typo never silently enables a behavior.
    pub fn from_query(query: &str) -> Self {
        let mut attrs = Self::default();
        for (key, value) in split_query(query) {
            let on = match value.as_str() {
                "1" | "true" | "on" => Some(true),
                "0" | "false" | "off" => Some(false),
                _ => None,
            };
            let Some(on) = on else { continue };
            match key.as_str() {
                "autoplay" => attrs.autoplay = on,
                "loop" => attrs.loop_enabled = on,
                "muted" => attrs.muted = on,
                "controls" => attrs.controls = on,
                "click-to-play" => attrs.click_to_play = on,
                "no-load-timeout" => attrs.no_load_timeout = on,
                _ => {}
            }
        }
        attrs
    }

    /// Render back to a query string holding only non-default values,
    /// with the hyphenated names (`click-to-play`, `no-load-timeout`).
    pub fn to_query(&self) -> String {
        let mut parts = Vec::new();
        if self.autoplay {
            parts.push("autoplay=1");
        }
        if self.loop_enabled {
            parts.push("loop=1");
        }
        if self.muted {
            parts.push("muted=1");
        }
        if !self.controls {
            parts.push("controls=0");
        }
        if !self.click_to_play {
            parts.push("click-to-play=0");
        }
        if self.no_load_timeout {
            parts.push("no-load-timeout=1");
        }
        parts.join("&")
    }
}

/// Split a query string into decoded pairs (`+` is a space, `%XX`
/// unescapes; malformed escapes stay literal).
fn split_query(query: &str) -> Vec<(String, String)> {
    fn decode(s: &str) -> String {
        let mut out = String::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => {
                    out.push(' ');
                    i += 1;
                }
                b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(b) => {
                        out.push(b as char);
                        i += 3;
                    }
                    Err(_) => {
                        out.push('%');
                        i += 1;
                    }
                },
                _ => {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
        }
        out
    }
    query
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (decode(k), decode(v)),
            None => (decode(p), "1".to_string()),
        })
        .collect()
}

/// One player event kind (Sprint 15, T005): ready, state, timeline,
/// assets-ready, painted, or playback, in [`CANONICAL_ORDER`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerEventKind {
    /// The load finished: the film (or the named failure) is known.
    Ready,
    /// Playing/paused, rate, volume, mute, or controls changed.
    State,
    /// Duration, frame count, or range changed.
    Timeline,
    /// Frames and mix are available (or known silent).
    AssetsReady,
    /// A frame was shown; carries the frame index.
    Painted,
    /// The playhead advanced; carries the new frame index.
    Playback,
}

impl PlayerEventKind {
    /// Wire name of the event, in canonical order.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::State => "state",
            Self::Timeline => "timeline",
            Self::AssetsReady => "assets-ready",
            Self::Painted => "painted",
            Self::Playback => "playback",
        }
    }
}

/// One fired player event: its kind plus a human detail (frame index,
/// state word, or cause).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEvent {
    /// Which event fired.
    pub kind: PlayerEventKind,
    /// Human detail, e.g. `frame 3` or `paused`.
    pub detail: String,
}

/// A loadable film: canvas, timeline, mix presence, and the preserved
/// `font-family` try-order (kept as authored, never resolved or
/// reordered; unknown families stay fail-closed at render time).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedFilm {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Output frame count.
    pub frame_count: u64,
    /// Frames per second behind time conversions.
    pub fps: Fps,
    /// Composition length in seconds.
    pub duration_secs: f64,
    /// Whether a mix exists (false plays silently, never a fake track).
    pub has_audio: bool,
    /// Authored `font-family` try-order lists, comma order preserved.
    pub font_try_order: Vec<String>,
}

/// Why a load failed (Sprint 15, T005 / REC-005): a named timeout past
/// the 8-second deadline, or an unloadable composition naming its
/// cause. Either way no frame is guessed and controls stay disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadFailureKind {
    /// The load ran past the deadline without no-load-timeout.
    Timeout,
    /// The composition cannot load (broken sources, unknown name…).
    Unloadable,
}

impl LoadFailureKind {
    /// Stable failure name reported to the host.
    pub fn name(self) -> &'static str {
        match self {
            Self::Timeout => "load-timeout",
            Self::Unloadable => "load-failed",
        }
    }
}

/// A failed player load: the kind plus a message naming the cause with
/// its fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerFailure {
    /// Why the load failed.
    pub kind: LoadFailureKind,
    /// Human message naming the cause with its fix.
    pub message: String,
}

/// A refused player method (Sprint 15, T005): the method plus a message
/// naming the cause with its fix; nothing moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerError {
    /// Which method refused.
    pub method: &'static str,
    /// Human message naming the cause with its fix.
    pub message: String,
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PlayerError {}

/// Player load state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    /// Loading (only observable before [`Player::load`] returns).
    Loading,
    /// Loaded and paused at the playhead.
    Ready,
    /// The playhead advances on [`Player::tick`].
    Playing,
    /// Paused with the frame kept.
    Paused,
    /// The load failed; [`Player::failure`] names the cause.
    Failed,
}

/// Reason the player transport is disabled, shared by every refusal so
/// the host shows one consistent sentence.
fn disabled_message(film: Option<&LoadedFilm>) -> String {
    match film {
        None => {
            "player disabled: the composition failed to load; fix the cause and reload".to_string()
        }
        Some(f) if f.frame_count == 0 => {
            "player disabled: the timeline is empty (0 frames); add composition time before playing"
                .to_string()
        }
        _ => "player disabled: controls are off; enable controls to play".to_string(),
    }
}

/// The embeddable player (Sprint 15, T005): attributes, methods, and an
/// ordered event log over one loaded film. Owns no renderer and no
/// cache; frames and mix stay served by the preview session behind the
/// same `/api/frame` and `/api/mix.wav` endpoints, so shown bytes equal
/// the pipeline's bytes exactly.
#[derive(Debug, Clone)]
pub struct Player {
    attrs: PlayerAttributes,
    state: PlayerState,
    film: Option<LoadedFilm>,
    failure: Option<PlayerFailure>,
    position: u64,
    rate: f64,
    volume: f64,
    muted: bool,
    range: Option<(u64, u64)>,
    controls: bool,
    click_to_play: bool,
    carry: f64,
    painted: Option<u64>,
    events: Vec<PlayerEvent>,
}

impl Player {
    /// Start loading: records the attributes and reports [`PlayerState::Loading`].
    /// [`Player::load`] finishes the load.
    pub fn begin(attrs: PlayerAttributes) -> Self {
        Self {
            controls: attrs.controls,
            click_to_play: attrs.click_to_play,
            muted: attrs.muted,
            attrs,
            state: PlayerState::Loading,
            film: None,
            failure: None,
            position: 0,
            rate: 1.0,
            volume: 1.0,
            range: None,
            carry: 0.0,
            painted: None,
            events: Vec::new(),
        }
    }

    /// Finish loading (Sprint 15, T005): `elapsed` is the measured load
    /// time and `outcome` the composition result. Past the 8-second
    /// deadline without no-load-timeout the load fails with the named
    /// `load-timeout` failure even when the composition itself is fine;
    /// a broken composition fails naming its cause with no guessed
    /// frame. Success fires ready, state, timeline, assets-ready, and
    /// painted (for a non-empty timeline) in canonical order, then
    /// starts playing with state and playback when autoplay is set.
    pub fn load(&mut self, elapsed: Duration, outcome: Result<LoadedFilm, String>) {
        let film = match outcome {
            Ok(f) => f,
            Err(cause) => {
                self.fail(
                    LoadFailureKind::Unloadable,
                    format!(
                        "player load-failed: {cause}; no frame was guessed and controls stay disabled. Fix the cause and reload."
                    ),
                );
                return;
            }
        };
        if elapsed > LOAD_DEADLINE && !self.attrs.no_load_timeout {
            self.fail(
                LoadFailureKind::Timeout,
                format!(
                    "player load-timeout: loading took {:.1}s, past the {}s deadline; no frame was guessed and controls stay disabled. Fix the slow source and reload, or embed with no-load-timeout to disable the deadline.",
                    elapsed.as_secs_f64(),
                    LOAD_DEADLINE_SECS
                ),
            );
            return;
        }
        self.film = Some(film);
        self.state = PlayerState::Ready;
        self.push(PlayerEventKind::Ready, "ready");
        self.push(PlayerEventKind::State, self.state_word());
        let (frames, duration) = self.timeline_numbers();
        self.push(
            PlayerEventKind::Timeline,
            format!("timeline {frames} frames {duration:.3}s"),
        );
        self.push(
            PlayerEventKind::AssetsReady,
            if self.film_has_audio() {
                "assets-ready: frames and mix"
            } else {
                "assets-ready: frames, silent (no audio track)"
            }
            .to_string(),
        );
        if self.frame_count() > 0 {
            self.painted = Some(0);
            self.push(PlayerEventKind::Painted, "painted frame 0".to_string());
        }
        if self.attrs.autoplay {
            let _ = self.play();
        }
    }

    fn fail(&mut self, kind: LoadFailureKind, message: String) {
        self.state = PlayerState::Failed;
        self.film = None;
        self.failure = Some(PlayerFailure { kind, message });
        self.push(PlayerEventKind::Ready, kind.name());
        self.push(PlayerEventKind::State, self.state_word());
    }

    fn push(&mut self, kind: PlayerEventKind, detail: impl Into<String>) {
        self.events.push(PlayerEvent {
            kind,
            detail: detail.into(),
        });
    }

    fn state_word(&self) -> String {
        match self.state {
            PlayerState::Loading => "loading".to_string(),
            PlayerState::Ready => "ready paused".to_string(),
            PlayerState::Playing => "playing".to_string(),
            PlayerState::Paused => "paused".to_string(),
            PlayerState::Failed => "failed".to_string(),
        }
    }

    fn timeline_numbers(&self) -> (u64, f64) {
        match &self.film {
            Some(f) => (f.frame_count, f.duration_secs),
            None => (0, 0.0),
        }
    }

    fn film_has_audio(&self) -> bool {
        self.film.as_ref().is_some_and(|f| f.has_audio)
    }

    /// Load state of the player.
    pub fn state(&self) -> PlayerState {
        self.state
    }

    /// The load failure, when the player failed to load.
    pub fn failure(&self) -> Option<&PlayerFailure> {
        self.failure.as_ref()
    }

    /// The loaded film, when the load succeeded.
    pub fn film(&self) -> Option<&LoadedFilm> {
        self.film.as_ref()
    }

    /// Embed attributes this player started with.
    pub fn attributes(&self) -> &PlayerAttributes {
        &self.attrs
    }

    /// Whether playback controls can act: a loaded film with controls
    /// on. A failed load stays disabled however controls toggle.
    pub fn controls_enabled(&self) -> bool {
        self.film.is_some() && self.controls && self.frame_count() > 0
    }

    /// Why controls cannot act, or `None` when they can.
    pub fn disabled_reason(&self) -> Option<String> {
        if self.controls_enabled() {
            None
        } else if self.film.is_none() {
            Some(disabled_message(None))
        } else if self.frame_count() == 0 {
            Some(disabled_message(self.film.as_ref()))
        } else {
            Some("player disabled: controls are off; enable controls to play".to_string())
        }
    }

    fn ensure_actable(&self, method: &'static str) -> Result<&LoadedFilm, PlayerError> {
        match &self.film {
            None => Err(PlayerError {
                method,
                message: format!(
                    "player refused: {method} needs a loaded composition; {}. Nothing moved.",
                    self.failure
                        .as_ref()
                        .map(|f| f.message.clone())
                        .unwrap_or_else(|| "the composition failed to load".to_string())
                ),
            }),
            Some(f) if f.frame_count == 0 => Err(PlayerError {
                method,
                message: format!(
                    "player refused: {method} needs a non-empty timeline; {}. Nothing moved.",
                    disabled_message(self.film.as_ref())
                ),
            }),
            Some(f) => Ok(f),
        }
    }

    /// Output frame count (0 before a successful load).
    pub fn frame_count(&self) -> u64 {
        self.film.as_ref().map(|f| f.frame_count).unwrap_or(0)
    }

    /// Composition length in seconds (0.0 before a successful load).
    pub fn duration(&self) -> f64 {
        self.film.as_ref().map(|f| f.duration_secs).unwrap_or(0.0)
    }

    /// Current playhead time in seconds (`position / fps`).
    pub fn time(&self) -> f64 {
        match &self.film {
            Some(f) if f.fps.value() > 0.0 => self.position as f64 / f.fps.value(),
            _ => 0.0,
        }
    }

    /// Current frame index.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Playback rate multiplier.
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Volume in `0..=1` (kept while muted).
    pub fn volume(&self) -> f64 {
        self.volume
    }

    /// Whether output is muted.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Audible volume: 0 while muted, else [`Player::volume`].
    pub fn audible_volume(&self) -> f64 {
        if self.muted {
            0.0
        } else {
            self.volume
        }
    }

    /// Active range playback window, when set.
    pub fn range(&self) -> Option<(u64, u64)> {
        self.range
    }

    /// Whether controls are toggled on (failed loads still report
    /// disabled through [`Player::controls_enabled`]).
    pub fn controls_on(&self) -> bool {
        self.controls
    }

    /// Last shown frame index: `None` until the first paint, proving an
    /// unloadable composition shows no guessed frame.
    pub fn painted_frame(&self) -> Option<u64> {
        self.painted
    }

    /// Ordered event log.
    pub fn events(&self) -> &[PlayerEvent] {
        &self.events
    }

    /// Event wire names in firing order.
    pub fn event_names(&self) -> Vec<&'static str> {
        self.events.iter().map(|e| e.kind.name()).collect()
    }

    /// Start advancing the playhead. At the last frame (or the range
    /// end) this restarts from the range start (or frame 0) first, so
    /// play always produces motion. Fires state and playback.
    pub fn play(&mut self) -> Result<(), PlayerError> {
        let (frames, _) = self.timeline_numbers();
        self.ensure_actable("play")?;
        let (start, end) = self.play_window(frames);
        if self.position >= end || self.position + 1 >= frames {
            self.position = start;
            self.carry = 0.0;
        }
        self.state = PlayerState::Playing;
        self.push(PlayerEventKind::State, self.state_word());
        self.push(
            PlayerEventKind::Playback,
            format!("playback frame {}", self.position),
        );
        Ok(())
    }

    /// Stop advancing, keeping the frame: pausing is always safe and a
    /// no-op (no event) unless playback was running. Fires state.
    pub fn pause(&mut self) {
        if self.state == PlayerState::Playing {
            self.state = if self.film.is_some() {
                PlayerState::Paused
            } else {
                PlayerState::Failed
            };
            self.push(PlayerEventKind::State, self.state_word());
        }
    }

    /// Move the playhead to frame `index`, keeping the playing/paused
    /// state. Refuses outside `0..frame_count`. Fires timeline and
    /// painted.
    pub fn seek_to_frame(&mut self, index: u64) -> Result<(), PlayerError> {
        let frames = self.ensure_actable("seek")?.frame_count;
        if index >= frames {
            return Err(PlayerError {
                method: "seek",
                message: format!(
                    "player refused: seek to frame {index} is outside the timeline ({frames} frame(s)): pass 0..{frames}. No playhead was moved."
                ),
            });
        }
        self.position = index;
        self.carry = 0.0;
        self.painted = Some(index);
        self.push(
            PlayerEventKind::Timeline,
            format!("timeline seek frame {index}"),
        );
        self.push(PlayerEventKind::Painted, format!("painted frame {index}"));
        Ok(())
    }

    /// Move the playhead to `secs` seconds. Refuses negative times and
    /// times past the duration; the exact duration parks on the last
    /// frame. Fires timeline and painted.
    pub fn seek_to_time(&mut self, secs: f64) -> Result<(), PlayerError> {
        let (frames, duration, rate) = {
            let f = self.ensure_actable("seek")?;
            (f.frame_count, f.duration_secs, f.fps.value())
        };
        if !secs.is_finite() || secs < 0.0 || secs > duration {
            return Err(PlayerError {
                method: "seek",
                message: format!(
                    "player refused: seek to {secs}s is outside the timeline (0..={duration:.3}s): pass a time inside the composition. No playhead was moved."
                ),
            });
        }
        let index = ((secs * rate).floor() as u64).min(frames.saturating_sub(1));
        self.seek_to_frame(index)
    }

    /// Change the advance rate: only finite rates above zero act.
    /// Persists across seeks inside the session. Fires state.
    pub fn set_rate(&mut self, rate: f64) -> Result<(), PlayerError> {
        self.ensure_actable("rate")?;
        if !rate.is_finite() || rate <= 0.0 {
            return Err(PlayerError {
                method: "rate",
                message: format!(
                    "player refused: rate {rate} is not a usable playback rate; pass a finite rate above zero (for example 0.5, 1, or 2). The rate stays {}x.",
                    self.rate
                ),
            });
        }
        self.rate = rate;
        self.push(PlayerEventKind::State, format!("rate {rate}x"));
        Ok(())
    }

    /// Change the volume in `0..=1`. Refuses non-finite and out-of-band
    /// values without touching the current level. Fires state.
    pub fn set_volume(&mut self, volume: f64) -> Result<(), PlayerError> {
        self.ensure_actable("volume")?;
        if !volume.is_finite() || volume < 0.0 || volume > 1.0 {
            return Err(PlayerError {
                method: "volume",
                message: format!(
                    "player refused: volume {volume} is outside 0..=1; pass 0 (silent) to 1 (full). The volume stays {}.",
                    self.volume
                ),
            });
        }
        self.volume = volume;
        self.push(PlayerEventKind::State, format!("volume {volume}"));
        Ok(())
    }

    /// Mute (`true`) or unmute (`false`); the volume level is kept.
    /// Fires state on change, and is a no-op (no event) otherwise.
    pub fn set_muted(&mut self, muted: bool) {
        if self.muted != muted {
            self.muted = muted;
            self.push(
                PlayerEventKind::State,
                if muted { "muted" } else { "unmuted" }.to_string(),
            );
        }
    }

    /// Mute, keeping the volume level. Fires state on change.
    pub fn mute(&mut self) {
        self.set_muted(true);
    }

    /// Unmute, restoring the kept volume level. Fires state on change.
    pub fn unmute(&mut self) {
        self.set_muted(false);
    }

    /// Play only frames `start..=end`, looping the window for review.
    /// Seeks to `start` and starts playing. Refuses an inverted or
    /// out-of-range window before anything moves. Fires timeline, state,
    /// and playback.
    pub fn play_range(&mut self, start: u64, end: u64) -> Result<(), PlayerError> {
        let frames = self.ensure_actable("range")?.frame_count;
        if start > end {
            return Err(PlayerError {
                method: "range",
                message: format!(
                    "player refused: range {start}..={end} starts past its end; pass a start at or before the end. No range was set."
                ),
            });
        }
        if end >= frames {
            return Err(PlayerError {
                method: "range",
                message: format!(
                    "player refused: range {start}..={end} leaves the timeline ({frames} frame(s)): pass 0..{frames}. No range was set."
                ),
            });
        }
        self.range = Some((start, end));
        self.position = start;
        self.carry = 0.0;
        self.painted = Some(start);
        self.state = PlayerState::Playing;
        self.push(
            PlayerEventKind::Timeline,
            format!("timeline range {start}..={end}"),
        );
        self.push(PlayerEventKind::State, self.state_word());
        self.push(
            PlayerEventKind::Playback,
            format!("playback frame {}", self.position),
        );
        Ok(())
    }

    /// Clear the range window; the full timeline plays again. Fires
    /// timeline.
    pub fn clear_range(&mut self) {
        self.range = None;
        self.push(
            PlayerEventKind::Timeline,
            "timeline range cleared".to_string(),
        );
    }

    /// Toggle controls at any time (even mid-playback or before the
    /// load finishes). On a failed load controls still report disabled.
    /// Fires state.
    pub fn set_controls(&mut self, on: bool) {
        self.controls = on;
        self.push(
            PlayerEventKind::State,
            if on { "controls on" } else { "controls off" }.to_string(),
        );
    }

    /// Toggle click-to-play at any time. Fires state.
    pub fn set_click_to_play(&mut self, on: bool) {
        self.click_to_play = on;
        self.push(
            PlayerEventKind::State,
            if on {
                "click-to-play on"
            } else {
                "click-to-play off"
            }
            .to_string(),
        );
    }

    /// A click on the film: toggles play/pause when click-to-play is on
    /// and the player can act, and reports whether it acted. A disabled
    /// click or a failed load never acts and fires no event.
    pub fn click_film(&mut self) -> bool {
        if !self.click_to_play || self.film.is_none() || self.frame_count() == 0 {
            return false;
        }
        if self.state == PlayerState::Playing {
            self.pause();
        } else if self.play().is_err() {
            return false;
        }
        true
    }

    /// Advance the playhead by `elapsed` of wall-clock time at the
    /// current rate. A no-op while paused, failed, or empty. A set range
    /// loops inside its window (review loops); without a range the end
    /// parks on the last frame and pauses. Fires playback when the frame
    /// changes (plus state when playback stops at the end).
    pub fn tick(&mut self, elapsed: Duration) -> u64 {
        if self.state != PlayerState::Playing {
            return self.position;
        }
        let Some(film) = &self.film else {
            return self.position;
        };
        let rate = film.fps.value();
        if !rate.is_finite() || rate <= 0.0 {
            return self.position;
        }
        let delta = elapsed.as_secs_f64() * rate * self.rate + self.carry;
        if !delta.is_finite() || delta <= 0.0 {
            self.carry = delta.max(0.0);
            return self.position;
        }
        let whole = delta.floor() as u64;
        self.carry = delta - whole as f64;
        if whole == 0 {
            return self.position;
        }
        let frames = film.frame_count;
        let (start, end) = self.play_window(frames);
        let window = end - start + 1;
        let rel = self.position.saturating_sub(start);
        if self.range.is_some() || self.attrs.loop_enabled {
            self.position = start + (rel + whole) % window;
        } else {
            let last = frames - 1;
            match self.position.saturating_add(whole) {
                next if next >= last => {
                    self.position = last;
                    self.state = PlayerState::Paused;
                    self.carry = 0.0;
                    self.painted = Some(last);
                    self.push(PlayerEventKind::Playback, format!("playback frame {last}"));
                    self.push(PlayerEventKind::State, self.state_word());
                    return self.position;
                }
                next => {
                    self.position = next;
                }
            }
        }
        self.painted = Some(self.position);
        self.push(
            PlayerEventKind::Playback,
            format!("playback frame {}", self.position),
        );
        self.position
    }

    /// Current play window: the active range, else the full timeline.
    fn play_window(&self, frames: u64) -> (u64, u64) {
        self.range.unwrap_or((0, frames.saturating_sub(1)))
    }
}

/// Film style for the embed (Sprint 15, T005): with no height given the
/// film shows at its own shape through `aspect-ratio`; an explicit
/// height pins the film height instead.
pub fn film_style(width: u32, height: u32, height_attr: Option<u32>) -> String {
    match height_attr {
        None => format!("width:100%;height:auto;aspect-ratio: {width} / {height};"),
        Some(h) => format!("width:100%;height: {h}px;"),
    }
}

/// Host embed snippet (Sprint 15, T005 / INT-001): an iframe pointing at
/// the served player page with the hyphenated attributes
/// (`click-to-play`, `no-load-timeout`) and the film shape style.
pub fn embed_snippet(
    player_url: &str,
    composition: &str,
    attrs: &PlayerAttributes,
    width: u32,
    height: u32,
    height_attr: Option<u32>,
) -> String {
    let query = attrs.to_query();
    let src = if query.is_empty() {
        format!("{player_url}?composition={composition}")
    } else {
        format!("{player_url}?composition={composition}&{query}")
    };
    format!(
        "<iframe src=\"{src}\" style=\"{}border:0\" title=\"Framesmith player — {composition}\" allow=\"autoplay\"></iframe>",
        film_style(width, height, height_attr)
    )
}

/// Built-in speed menu stops (Sprint 15, T006 / CAP-002): the page
/// `<select>` offers exactly these rates, from 0.25x to 4x. The
/// control path accepts any rate inside the menu range; the host
/// method [`Player::set_rate`] keeps accepting any finite positive
/// rate.
pub const SPEED_MENU: [f64; 7] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0];

/// Slowest built-in menu rate (0.25x).
pub const SPEED_MENU_MIN: f64 = 0.25;

/// Fastest built-in menu rate (4x).
pub const SPEED_MENU_MAX: f64 = 4.0;

/// Whether `rate` is one of the built-in menu stops.
pub fn is_menu_rate(rate: f64) -> bool {
    SPEED_MENU.contains(&rate)
}

/// Time readout after one control gesture (Sprint 15, T006 /
/// CAP-002): the current frame number plus its time in seconds beside
/// the totals, so the display is always the position the control just
/// produced. Speed, volume, and mute ride along so every control
/// answers with the whole visible state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerReadout {
    /// Current frame index.
    pub frame: u64,
    /// Output frame count of the timeline.
    pub frame_count: u64,
    /// Time of the current frame in seconds (`frame / fps`).
    pub time_secs: f64,
    /// Composition length in seconds.
    pub duration_secs: f64,
    /// Playback rate multiplier.
    pub rate: f64,
    /// Volume in `0..=1` (kept while muted).
    pub volume: f64,
    /// Whether output is muted.
    pub muted: bool,
    /// Whether the playhead is advancing.
    pub playing: bool,
    /// False only for an empty timeline, whose page shows the
    /// disabling reason instead of a position.
    pub enabled: bool,
}

impl std::fmt::Display for PlayerReadout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "frame {}/{} {:.3}s/{:.3}s {} {}x volume {}{}",
            self.frame,
            self.frame_count,
            self.time_secs,
            self.duration_secs,
            if self.playing { "playing" } else { "paused" },
            self.rate,
            self.volume,
            if self.muted { " muted" } else { "" }
        )
    }
}

/// One built-in control gesture (Sprint 15, T006 / CAP-002): every
/// control acts on the playing composition immediately and answers
/// with the fresh readout, so the display never lags the gesture.
/// Seeks never touch the rate, so speed persists across seeks inside
/// the session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlAction {
    /// Start advancing the playhead.
    Play,
    /// Stop advancing, keeping the frame. Always safe, even on an
    /// empty timeline or a failed load.
    Pause,
    /// Move the playhead to a frame index immediately.
    ScrubFrame(u64),
    /// Move the playhead to a time in seconds immediately.
    ScrubTime(f64),
    /// Change the advance rate within the menu range (0.25x to 4x).
    Rate(f64),
    /// Change the volume within `0..=1`.
    Volume(f64),
    /// Mute, keeping the volume level. Always safe.
    Mute,
    /// Unmute, restoring the kept level. Always safe.
    Unmute,
}

impl Player {
    /// Current time/frame readout: everything the page display shows
    /// after one control gesture.
    pub fn readout(&self) -> PlayerReadout {
        PlayerReadout {
            frame: self.position,
            frame_count: self.frame_count(),
            time_secs: self.time(),
            duration_secs: self.duration(),
            rate: self.rate,
            volume: self.volume,
            muted: self.muted,
            playing: self.state == PlayerState::Playing,
            enabled: self.controls_enabled(),
        }
    }

    /// Background frame still served when transport is disabled
    /// (Sprint 15, T006): an empty timeline refuses every transport
    /// gesture with the reason, but the loaded film — canvas and all —
    /// stays available, so the page shows the background box instead
    /// of a guessed frame. `None` only when the load itself failed.
    pub fn background_frame(&self) -> Option<(u32, u32)> {
        self.film.as_ref().map(|f| (f.width, f.height))
    }

    /// Run one built-in control gesture immediately and answer with
    /// the fresh readout. Refusals name the method, the cause, and the
    /// fix, and move nothing: an empty timeline refuses transport with
    /// the disabling reason, a rate outside 0.25x to 4x refuses naming
    /// the menu bound, and a failed load refuses naming its cause.
    pub fn apply_control(&mut self, action: ControlAction) -> Result<PlayerReadout, PlayerError> {
        match action {
            ControlAction::Play => self.play().map(|()| self.readout()),
            ControlAction::Pause => {
                self.pause();
                Ok(self.readout())
            }
            ControlAction::ScrubFrame(index) => self.seek_to_frame(index).map(|()| self.readout()),
            ControlAction::ScrubTime(secs) => self.seek_to_time(secs).map(|()| self.readout()),
            ControlAction::Rate(rate) => self.set_control_rate(rate).map(|()| self.readout()),
            ControlAction::Volume(volume) => self.set_volume(volume).map(|()| self.readout()),
            ControlAction::Mute => {
                self.mute();
                Ok(self.readout())
            }
            ControlAction::Unmute => {
                self.unmute();
                Ok(self.readout())
            }
        }
    }

    /// Change the advance rate from the built-in controls: only the
    /// menu range (0.25x to 4x) acts. Persists across seeks inside the
    /// session. Fires state.
    fn set_control_rate(&mut self, rate: f64) -> Result<(), PlayerError> {
        self.ensure_actable("rate")?;
        if !rate.is_finite() || rate < SPEED_MENU_MIN || rate > SPEED_MENU_MAX {
            return Err(PlayerError {
                method: "rate",
                message: format!(
                    "player refused: rate {rate} is outside the built-in speed menu (0.25x to 4x); pick a menu rate from 0.25 to 4. The rate stays {}x.",
                    self.rate
                ),
            });
        }
        self.rate = rate;
        self.push(PlayerEventKind::State, format!("rate {rate}x"));
        Ok(())
    }
}

/// Model behind the served player page: everything the page needs
/// without touching server internals, so tests build it directly.
#[derive(Debug, Clone)]
pub struct PlayerPageModel {
    /// Project directory key shown in the title.
    pub project: String,
    /// Composition name.
    pub composition: String,
    /// Current frame index.
    pub frame: u64,
    /// Output frame count.
    pub frame_count: u64,
    /// Composition length in seconds.
    pub duration_secs: f64,
    /// Fps value behind the clock.
    pub fps_value: f64,
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Whether a mix exists (false renders the silent note, never a
    /// fake track).
    pub has_audio: bool,
    /// Embed attributes from the query string.
    pub attrs: PlayerAttributes,
}

/// The embeddable player page (Sprint 15, T005): the film at its own
/// shape when no height is given, the mix as the clock, and a
/// `window.framesmithPlayer` object implementing the method set while
/// firing `framesmith:<kind>` events in canonical order. Loading ends in
/// ready or a named failure within 8 seconds unless no-load-timeout is
/// set; a failure shows its cause with controls disabled and no guessed
/// frame. Built-in transport controls (Sprint 15, T006 / CAP-002)
/// live on this page: the play control, scrubber, time readout, speed
/// menu (0.25x to 4x), mute, and volume act immediately with the
/// readout showing current time and duration; an empty timeline
/// renders the controls disabled with the reason while still serving
/// the background frame.
pub fn player_page_html(model: &PlayerPageModel) -> String {
    player_page_html_with_background(model, "#000")
}

/// Label for one menu stop (`1x`, `0.25x`): whole rates print without
/// a decimal point.
fn menu_label(rate: f64) -> String {
    if rate.fract() == 0.0 {
        format!("{rate:.0}x")
    } else {
        format!("{rate}x")
    }
}

/// The embeddable player page with built-in controls (Sprint 15, T006
/// / CAP-002): the same film, mix, method set, and canonical events
/// as [`player_page_html`], plus the play control, scrubber, time
/// readout, speed menu (0.25x to 4x), mute, and volume in the fixed
/// high-contrast treatment. Every control acts on the playing
/// composition immediately and refreshes the time readout (current
/// time and duration); speed persists across seeks. An empty timeline
/// renders the controls disabled with the reason while showing the
/// background frame box in `background_css`; a silent project keeps
/// the silent note with the controls enabled. Style attributes use
/// hyphenated CSS vocabulary (`background-color`, `border-radius`,
/// `font-variant-numeric`). `background_css` is a `#rrggbb` color the
/// server reads from the composition; the model-only page defaults it
/// to black.
pub fn player_page_html_with_background(model: &PlayerPageModel, background_css: &str) -> String {
    let audio_block = if model.has_audio {
        "<audio id=\"mix\" src=\"/api/mix.wav\" preload=\"auto\"></audio>".to_string()
    } else {
        "<p id=\"audio-note\">silent project: video plays with no audio track.</p>".to_string()
    };
    let attrs_json = format!(
        "{{\"autoplay\":{},\"loop\":{},\"muted\":{},\"controls\":{},\"clickToPlay\":{},\"noLoadTimeout\":{}}}",
        model.attrs.autoplay,
        model.attrs.loop_enabled,
        model.attrs.muted,
        model.attrs.controls,
        model.attrs.click_to_play,
        model.attrs.no_load_timeout
    );
    let empty = model.frame_count == 0;
    let controls_off = !model.attrs.controls;
    let disabled = empty || controls_off;
    let reason = if empty {
        "player disabled: the timeline is empty (0 frames); add composition time before playing"
    } else if controls_off {
        "player disabled: controls are off; enable controls to play"
    } else {
        ""
    };
    let disabled_attr = if disabled { " disabled" } else { "" };
    let reason_hidden = if disabled { "" } else { " hidden" };
    let background_hidden = if empty { "" } else { " hidden" };
    let status_text = if empty { reason } else { "loading…" };
    let start_secs = if model.fps_value > 0.0 {
        model.frame as f64 / model.fps_value
    } else {
        0.0
    };
    let readout_text = format!(
        "{:.3}s/{:.3}s frame {}/{}",
        start_secs, model.duration_secs, model.frame, model.frame_count
    );
    let mut speed_options = String::new();
    for rate in SPEED_MENU {
        let selected = if (rate - 1.0).abs() < f64::EPSILON {
            " selected"
        } else {
            ""
        };
        speed_options.push_str(&format!(
            "<option value=\"{rate}\"{selected}>{}</option>",
            menu_label(rate)
        ));
    }
    let mute_label = if model.attrs.muted { "unmute" } else { "mute" };
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Framesmith player — {project} — {composition}</title>\n\
        <style>body{{background:#111;color:#eee;font-family:sans-serif;max-width:900px;margin:2em auto}}#film{{max-width:100%;background:#000;display:block}}{style}#film-background{{max-width:100%;display:block}}#status{{font-variant-numeric:tabular-nums}}#controls{{background-color:#000;color:#fff;border:1px solid #fff;border-radius:4px;padding:8px;margin-top:8px}}#controls button,#controls select,#controls input{{margin:2px}}#time-readout{{font-variant-numeric:tabular-nums}}#controls-reason{{font-weight:bold}}</style>\n\
        </head>\n<body>\n<h1>player: {project} — {composition}</h1>\n\
        <p id=\"status\">{status}</p>\n\
        <img id=\"film\" src=\"/api/frame?i={frame}\" alt=\"player film\" style=\"{style}\" hidden>\n\
        <div id=\"film-background\"{background_hidden} style=\"{style}background-color:{background};\">background frame</div>\n\
        <div id=\"controls\" style=\"background-color:#000;color:#fff;border:1px solid #fff;border-radius:4px;padding:8px;margin-top:8px\">\n\
        <button id=\"ctl-play\" type=\"button\"{disabled_attr}>play</button>\n\
        <label for=\"ctl-scrub\">scrub</label>\n\
        <input id=\"ctl-scrub\" type=\"range\" min=\"0\" max=\"{scrub_max}\" value=\"{frame}\"{disabled_attr}>\n\
        <p id=\"time-readout\" style=\"font-variant-numeric:tabular-nums\">{readout}</p>\n\
        <label for=\"ctl-speed\">speed</label>\n\
        <select id=\"ctl-speed\"{disabled_attr}>{speed_options}</select>\n\
        <button id=\"ctl-mute\" type=\"button\"{disabled_attr}>{mute_label}</button>\n\
        <label for=\"ctl-volume\">volume</label>\n\
        <input id=\"ctl-volume\" type=\"range\" min=\"0\" max=\"1\" step=\"0.01\" value=\"1\"{disabled_attr}>\n\
        <p id=\"controls-reason\"{reason_hidden}>{reason}</p>\n\
        </div>\n\
        <div id=\"failure\" hidden></div>\n{audio}\n\
        <script>\nwindow.framesmithPlayer = (function() {{\n  const attrs = {attrs};\n  const frameCount = {frame_count};\n  const durationSecs = {duration};\n  const fps = {fps};\n  const LOAD_DEADLINE_MS = {deadline_ms};\n  const ORDER = ['ready', 'state', 'timeline', 'assets-ready', 'painted', 'playback'];\n  let position = {frame};\n  let playing = false;\n  let rate = 1;\n  let volume = 1;\n  let muted = !!attrs.muted;\n  let controls = !!attrs.controls;\n  let clickToPlay = !!attrs.clickToPlay;\n  let range = null;\n  function fire(kind, detail) {{\n    document.dispatchEvent(new CustomEvent('framesmith:' + kind, {{detail: {{kind, detail}}}}));\n  }}\n  function fail(kind, cause) {{\n    document.getElementById('film').hidden = true;\n    const box = document.getElementById('failure');\n    box.hidden = false;\n    box.textContent = 'player ' + kind + ': ' + cause + '; controls stay disabled.';\n    document.getElementById('status').textContent = 'player ' + kind + ': ' + cause;\n    syncControlsEnabled(false, 'player ' + kind + ': ' + cause + '; controls stay disabled.');\n    fire('ready', kind);\n    fire('state', 'failed');\n  }}\n  async function load() {{\n    const deadline = attrs.noLoadTimeout ? null : LOAD_DEADLINE_MS;\n    const ctrl = new AbortController();\n    let timer = null;\n    if (deadline !== null) timer = setTimeout(() => ctrl.abort(new Error('load-timeout')), deadline);\n    try {{\n      const s = await (await fetch('/api/status', {{signal: ctrl.signal}})).json();\n      if (timer) clearTimeout(timer);\n      fire('ready', 'ready');\n      fire('state', 'ready paused');\n      fire('timeline', 'timeline ' + s.frame_count + ' frames ' + s.duration_secs + 's');\n      fire('assets-ready', s.has_audio === false ? 'assets-ready: frames, silent (no audio track)' : 'assets-ready: frames and mix');\n      if (frameCount === 0) {{\n        document.getElementById('film').hidden = true;\n        const bg = document.getElementById('film-background');\n        if (bg) bg.hidden = false;\n      }} else {{\n        await show(position);\n      }}\n      syncControlsEnabled(frameCount > 0 && controls, null);\n      if (attrs.autoplay && frameCount > 0) play();\n    }} catch (e) {{\n      if (timer) clearTimeout(timer);\n      const aborted = e && e.message === 'load-timeout';\n      fail(aborted ? 'load-timeout' : 'load-failed', aborted ? ('loading took past the ' + (LOAD_DEADLINE_MS / 1000) + 's deadline') : String((e && e.message) || e));\n    }}\n  }}\n  async function show(i) {{\n    const r = await fetch('/api/frame?i=' + i);\n    if (!r.ok) {{ fail('load-failed', await r.text()); return; }}\n    const b = await r.blob();\n    const img = document.getElementById('film');\n    img.src = URL.createObjectURL(b);\n    img.hidden = false;\n    position = i;\n    document.getElementById('status').textContent = 'frame ' + i + '/' + frameCount;\n    refreshReadout();\n    fire('painted', 'painted frame ' + i);\n  }}\n  function play() {{ playing = true; fire('state', 'playing'); fire('playback', 'playback frame ' + position); }}\n  function pause() {{ if (!playing) return; playing = false; fire('state', 'paused'); }}\n  async function seek(i) {{ if (frameCount === 0) return; await show(i); fire('timeline', 'timeline seek frame ' + i); }}\n  function time() {{ return fps > 0 ? position / fps : 0; }}\n  function duration() {{ return durationSecs; }}\n  function setRate(r) {{ rate = r; fire('state', 'rate ' + r + 'x'); }}\n  function setVolume(v) {{ volume = v; fire('state', 'volume ' + v); }}\n  function setMuted(m) {{ muted = !!m; fire('state', muted ? 'muted' : 'unmuted'); }}\n  function isMuted() {{ return muted; }}\n  function setRange(a, b) {{ range = [a, b]; fire('timeline', 'timeline range ' + a + '..=' + b); }}\n  function setControls(on) {{ controls = !!on; syncControlsEnabled(frameCount > 0 && controls, 'player disabled: controls are off; enable controls to play'); fire('state', controls ? 'controls on' : 'controls off'); }}\n  function refreshReadout() {{\n    const r = document.getElementById('time-readout');\n    if (r) r.textContent = time().toFixed(3) + 's/' + duration().toFixed(3) + 's frame ' + position + '/' + frameCount;\n    const s = document.getElementById('ctl-scrub');\n    if (s) s.value = position;\n  }}\n  function syncControlsEnabled(ok, why) {{\n    ['ctl-play', 'ctl-scrub', 'ctl-speed', 'ctl-mute', 'ctl-volume'].forEach(id => {{ const el = document.getElementById(id); if (el) el.disabled = !ok; }});\n    const box = document.getElementById('controls-reason');\n    if (box) {{ box.hidden = !!ok; if (!ok && why) box.textContent = why; }}\n  }}\n  function wireControls() {{\n    const playCtl = document.getElementById('ctl-play');\n    if (playCtl) playCtl.addEventListener('click', () => {{ if (frameCount === 0 || !controls) return; playing ? pause() : play(); refreshReadout(); }});\n    const scrub = document.getElementById('ctl-scrub');\n    if (scrub) scrub.addEventListener('input', async (e) => {{ if (frameCount === 0 || !controls) return; await seek(Number(e.target.value)); refreshReadout(); }});\n    const speed = document.getElementById('ctl-speed');\n    if (speed) speed.addEventListener('change', (e) => {{ if (!controls) return; setRate(Number(e.target.value)); refreshReadout(); }});\n    const mute = document.getElementById('ctl-mute');\n    if (mute) mute.addEventListener('click', () => {{ setMuted(!muted); mute.textContent = muted ? 'unmute' : 'mute'; refreshReadout(); }});\n    const vol = document.getElementById('ctl-volume');\n    if (vol) vol.addEventListener('input', (e) => {{ setVolume(Number(e.target.value)); refreshReadout(); }});\n  }}\n  document.getElementById('film').addEventListener('click', () => {{ if (!clickToPlay) return; playing ? pause() : play(); }});\n  wireControls();\n  refreshReadout();\n  load();\n  return {{play, pause, seek, time, duration, setRate, setVolume, setMuted, setRange, setControls, isMuted,\n    get rate() {{ return rate; }}, get volume() {{ return volume; }},\n    get position() {{ return position; }}, get playing() {{ return playing; }},\n    get ORDER() {{ return ORDER; }}}};\n}})();\n</script>\n</body>\n</html>\n",
        project = model.project,
        composition = model.composition,
        style = film_style(model.width, model.height, None),
        frame = model.frame,
        attrs = attrs_json,
        frame_count = model.frame_count,
        duration = model.duration_secs,
        fps = model.fps_value,
        audio = audio_block,
        deadline_ms = LOAD_DEADLINE_SECS * 1000,
        status = status_text,
        background_hidden = background_hidden,
        background = background_css,
        disabled_attr = disabled_attr,
        scrub_max = model.frame_count.saturating_sub(1),
        readout = readout_text,
        speed_options = speed_options,
        mute_label = mute_label,
        reason_hidden = reason_hidden,
        reason = reason,
    )
}
