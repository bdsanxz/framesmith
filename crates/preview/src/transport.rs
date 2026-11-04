//! Transport with display (Sprint 14, T003 / CAP-003).
//!
//! Page controls for play, pause, scrub, step, loop, and speed plus a
//! time/frame readout, acting on the timeline immediately. [`Transport`]
//! is the whole timeline state: the page maps each control onto one
//! method and renders [`Readout`] after every gesture, so the readout
//! is always the position the controls just produced.
//!
//! Wiring against the frame slice (T001) and the latency bars (T002):
//! after moving the playhead with [`Transport::scrub_to`],
//! [`Transport::step_forward`], [`Transport::step_back`], or
//! [`Transport::tick`], the page serves the frame at
//! [`Transport::position`] through
//! [`PreviewSession::frame_png_timed`](crate::PreviewSession::frame_png_timed)
//! with the matching [`TransportAction`](crate::TransportAction)
//! (`Step` for one-frame steps, `Scrub` for drags, `Jump` for arbitrary
//! seeks), and reads [`Readout`] for the time/frame display. Audio (T004)
//! owns the playback clock; `tick` here only advances the playhead by
//! wall-clock time scaled by speed. The daemon (T006) owns the server;
//! transport is page-only.
//!
//! Failure rule (CAP-003): a transport that cannot act — an empty
//! timeline — is visibly disabled with the reason. Every acting control
//! then refuses naming that reason instead of moving a playhead that
//! has nowhere to point; [`Transport::readout`] shows the same reason.

use std::time::Duration;

/// Frames per second behind a transport, in exact rational form.
pub use notation::model::Fps;

/// Which timeline gesture a transport control performs (Sprint 14,
/// T003): one method per control, each acting immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportControl {
    /// Start advancing the playhead.
    Play,
    /// Move the playhead to an arbitrary frame.
    Scrub,
    /// Move one frame forward.
    StepForward,
    /// Move one frame back.
    StepBack,
    /// Change the advance rate.
    SetSpeed,
}

impl std::fmt::Display for TransportControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Play => write!(f, "play"),
            Self::Scrub => write!(f, "scrub"),
            Self::StepForward => write!(f, "step forward"),
            Self::StepBack => write!(f, "step back"),
            Self::SetSpeed => write!(f, "speed"),
        }
    }
}

/// Why a transport control refused to act (Sprint 14, T003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// The timeline is empty: there is no frame to point at.
    Disabled,
    /// The scrub target sits outside `0..frame_count`.
    OutOfRange,
    /// The requested speed is not a finite rate above zero.
    BadSpeed,
}

/// A refused transport control (Sprint 14, T003): the kind plus a
/// message naming the control, the cause, and the fix — the same
/// shape the frame slice refuses with, so the page shows it as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    /// Why the control refused.
    pub kind: TransportErrorKind,
    /// Which control refused.
    pub control: TransportControl,
    /// Human message naming the control, the cause, and the fix.
    pub message: String,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TransportError {}

/// Reason a transport is disabled, shared by every refusal and the
/// readout so the page shows one consistent sentence.
fn disabled_message(frames: u64) -> String {
    format!(
        "transport disabled: the timeline is empty ({frames} frames); add composition time before playing"
    )
}

/// Timeline playhead with transport controls and a readout (Sprint 14,
/// T003 / CAP-003, STATE-03). Every acting method applies
/// synchronously — the page reads [`Transport::readout`] right after —
/// so controls and display can never disagree.
#[derive(Debug, Clone)]
pub struct Transport {
    frames: u64,
    fps: Fps,
    duration_secs: f64,
    position: u64,
    playing: bool,
    loop_enabled: bool,
    speed: f64,
    /// Sub-frame advance below one frame, kept in `[0, 1)`: consecutive
    /// ticks accumulate exactly instead of dropping fractions.
    carry: f64,
}

impl Transport {
    /// Timeline state over `frame_count` frames at `fps`, lasting
    /// `duration_secs` seconds, parked at frame 0, paused, unlooped, at
    /// 1x speed. A zero `frame_count` is the empty timeline: enabled
    /// controls refuse with the reason and the readout shows it.
    pub fn new(frame_count: u64, fps: Fps, duration_secs: f64) -> Self {
        Self {
            frames: frame_count,
            fps,
            duration_secs,
            position: 0,
            playing: false,
            loop_enabled: false,
            speed: 1.0,
            carry: 0.0,
        }
    }

    /// Output frame count of this timeline.
    pub fn frame_count(&self) -> u64 {
        self.frames
    }

    /// Frames per second behind the time readout.
    pub fn fps(&self) -> Fps {
        self.fps
    }

    /// Composition length in seconds behind the readout total.
    pub fn duration_secs(&self) -> f64 {
        self.duration_secs
    }

    /// Current frame index.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Whether the playhead advances on [`Transport::tick`].
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether advancing past the end wraps instead of stopping.
    pub fn loop_enabled(&self) -> bool {
        self.loop_enabled
    }

    /// Advance-rate multiplier applied by [`Transport::tick`].
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Whether any control can act: false only for an empty timeline.
    pub fn is_enabled(&self) -> bool {
        self.frames > 0
    }

    /// Why the transport is disabled, or `None` when it can act.
    pub fn disabled_reason(&self) -> Option<String> {
        if self.is_enabled() {
            None
        } else {
            Some(disabled_message(self.frames))
        }
    }

    /// Refuse when the timeline is empty, naming the control and the
    /// reason.
    fn ensure_enabled(&self, control: TransportControl) -> Result<(), TransportError> {
        if self.is_enabled() {
            Ok(())
        } else {
            Err(TransportError {
                kind: TransportErrorKind::Disabled,
                control,
                message: format!(
                    "transport refused: {control} needs a non-empty timeline; {}. No playhead was moved.",
                    disabled_message(self.frames)
                ),
            })
        }
    }

    /// Start advancing the playhead. At the last frame this restarts
    /// from frame 0 first, so play always produces motion; without loop
    /// the playhead stops again at the end, with loop it keeps
    /// wrapping.
    pub fn play(&mut self) -> Result<(), TransportError> {
        self.ensure_enabled(TransportControl::Play)?;
        if self.position + 1 >= self.frames {
            self.position = 0;
            self.carry = 0.0;
        }
        self.playing = true;
        Ok(())
    }

    /// Stop advancing the playhead. Always safe — pausing an empty or
    /// already-paused timeline changes nothing and never refuses.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Move the playhead to `index` immediately, keeping the
    /// playing/paused state. Refuses naming the valid window when
    /// `index` sits outside `0..frame_count`.
    pub fn scrub_to(&mut self, index: u64) -> Result<(), TransportError> {
        self.ensure_enabled(TransportControl::Scrub)?;
        if index >= self.frames {
            return Err(TransportError {
                kind: TransportErrorKind::OutOfRange,
                control: TransportControl::Scrub,
                message: format!(
                    "transport refused: scrub to frame {index} is outside the timeline ({} frame(s)): pass 0..{}. No playhead was moved.",
                    self.frames, self.frames
                ),
            });
        }
        self.position = index;
        self.carry = 0.0;
        Ok(())
    }

    /// Move one frame forward immediately, clamping at the last frame.
    pub fn step_forward(&mut self) -> Result<(), TransportError> {
        self.ensure_enabled(TransportControl::StepForward)?;
        if self.position + 1 < self.frames {
            self.position += 1;
        }
        self.carry = 0.0;
        Ok(())
    }

    /// Move one frame back immediately, clamping at frame 0.
    pub fn step_back(&mut self) -> Result<(), TransportError> {
        self.ensure_enabled(TransportControl::StepBack)?;
        self.position = self.position.saturating_sub(1);
        self.carry = 0.0;
        Ok(())
    }

    /// Arm or disarm looping: while armed, [`Transport::tick`] wraps
    /// past the end back to frame 0 without stopping. Always safe —
    /// toggling loop on an empty timeline changes nothing and never
    /// refuses.
    pub fn set_loop(&mut self, enabled: bool) {
        self.loop_enabled = enabled;
    }

    /// Change the advance rate: [`Transport::tick`] scales wall-clock
    /// time by this multiplier. Only finite rates above zero act; zero,
    /// negative, infinite, and NaN rates refuse naming the cause.
    pub fn set_speed(&mut self, speed: f64) -> Result<(), TransportError> {
        self.ensure_enabled(TransportControl::SetSpeed)?;
        if !speed.is_finite() || speed <= 0.0 {
            return Err(TransportError {
                kind: TransportErrorKind::BadSpeed,
                control: TransportControl::SetSpeed,
                message: format!(
                    "transport refused: speed {speed} is not a usable rate; pass a finite rate above zero (for example 0.5, 1, or 2). The rate stays {}x.",
                    self.speed
                ),
            });
        }
        self.speed = speed;
        Ok(())
    }

    /// Advance the playhead by `elapsed` of wall-clock time at the
    /// current speed. A no-op while paused or on an empty timeline, so
    /// the page can call it unconditionally per animation frame. With
    /// loop armed the playhead wraps past the end and keeps playing;
    /// without loop it parks on the last frame and stops.
    pub fn tick(&mut self, elapsed: Duration) {
        if !self.playing || self.frames == 0 {
            return;
        }
        let rate = self.fps.value();
        if !rate.is_finite() || rate <= 0.0 {
            return;
        }
        let delta = elapsed.as_secs_f64() * rate * self.speed + self.carry;
        if !delta.is_finite() || delta <= 0.0 {
            self.carry = delta.max(0.0);
            return;
        }
        let whole = delta.floor() as u64;
        self.carry = delta - whole as f64;
        if whole == 0 {
            return;
        }
        if self.loop_enabled {
            self.position = (self.position + whole) % self.frames;
        } else {
            let last = self.frames - 1;
            match self.position.saturating_add(whole) {
                next if next >= last => {
                    self.position = last;
                    self.playing = false;
                    self.carry = 0.0;
                }
                next => {
                    self.position = next;
                }
            }
        }
    }

    /// Rebase onto a reloaded timeline (Sprint 14, T005 / CAP-005):
    /// point the same transport at `frame_count` frames at `fps`
    /// lasting `duration_secs` seconds while keeping the playhead —
    /// the same position, clamped onto the last frame when the edit
    /// shortened the timeline past it. Playing/paused, loop, and speed
    /// stay exactly as they were: a reload never restarts playback
    /// from zero and never pauses it. Sub-frame carry resets, so the
    /// next tick advances whole from the kept position. Returns the
    /// kept position. A no-op for position 0 on an empty timeline.
    pub fn rebase(&mut self, frame_count: u64, fps: Fps, duration_secs: f64) -> u64 {
        self.frames = frame_count;
        self.fps = fps;
        self.duration_secs = duration_secs;
        self.carry = 0.0;
        if frame_count == 0 {
            self.position = 0;
        } else {
            self.position = self.position.min(frame_count - 1);
        }
        self.position
    }

    /// Follow the mix clock (Sprint 14, T004 / CAP-004): point the
    /// playhead at the frame sounding at `mix_secs` seconds of mix
    /// time. The page calls this on each audio timeupdate while
    /// sounding the preview mix, so playback position follows the mix
    /// instead of wall clock. Clamps to the timeline — times past the
    /// end park on the last frame — keeps the playing/paused state
    /// (the audio element owns start/stop), and is a no-op on an
    /// empty timeline, so the page can call it unconditionally while
    /// sound plays.
    pub fn sync_to_mix_time(&mut self, mix_secs: f64) {
        if self.frames == 0 {
            return;
        }
        self.position = crate::MixInfo::frame_for_mix_time(self.fps, self.frames, mix_secs);
        self.carry = 0.0;
    }

    /// Current time/frame readout: the frame number, the time of the
    /// current frame at this fps, the totals, and the control state —
    /// everything the page display shows after each gesture.
    pub fn readout(&self) -> Readout {
        let rate = self.fps.value();
        Readout {
            frame: self.position,
            frame_count: self.frames,
            time_secs: if rate > 0.0 {
                self.position as f64 / rate
            } else {
                0.0
            },
            duration_secs: self.duration_secs,
            fps: self.fps,
            playing: self.playing,
            loop_enabled: self.loop_enabled,
            speed: self.speed,
            enabled: self.is_enabled(),
        }
    }
}

/// Time/frame readout after one transport gesture (Sprint 14, T003 /
/// CAP-003): the current frame number plus its time in seconds, so the
/// display is always the position the controls just produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Readout {
    /// Current frame index.
    pub frame: u64,
    /// Output frame count of the timeline.
    pub frame_count: u64,
    /// Time of the current frame in seconds (`frame / fps`).
    pub time_secs: f64,
    /// Composition length in seconds.
    pub duration_secs: f64,
    /// Frames per second behind [`Readout::time_secs`].
    pub fps: Fps,
    /// Whether the playhead is advancing.
    pub playing: bool,
    /// Whether the end wraps instead of stopping.
    pub loop_enabled: bool,
    /// Advance-rate multiplier.
    pub speed: f64,
    /// False only for an empty timeline, whose display shows the
    /// disabling reason instead of a position.
    pub enabled: bool,
}

impl std::fmt::Display for Readout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.enabled {
            return write!(f, "{}; frame 0/0", disabled_message(self.frame_count));
        }
        write!(
            f,
            "frame {}/{} {:.3}s/{:.3}s {} loop {} {}x",
            self.frame,
            self.frame_count,
            self.time_secs,
            self.duration_secs,
            if self.playing { "playing" } else { "paused" },
            if self.loop_enabled { "on" } else { "off" },
            self.speed
        )
    }
}
