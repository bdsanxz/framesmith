//! Audio as the clock (Sprint 14, T004 / CAP-004, STATE-03).
//!
//! Pressing play sounds the render mix and playback position follows
//! the mix clock. [`PreviewSession`](crate::PreviewSession) serves the
//! mix through the same computation a full render mixes
//! ([`render::render::build_mix_preview`]), so heard bytes equal the
//! rendered mix byte-identically; the page plays those bytes in its
//! audio element and advances the playhead from the element's time.
//!
//! Wiring against the frame slice (T001) and transport (T003): the
//! page checks [`PreviewSession::play_with_audio`](crate::PreviewSession::play_with_audio)
//! before starting sound — a soundless project plays video silently
//! with no error ([`AudioPlay::Silent`]), an undecodable mix blocks
//! play naming the file ([`MixError`]) — then serves the frame at
//! [`Transport::position`](crate::Transport::position) per audio
//! timeupdate after moving it with
//! [`Transport::sync_to_mix_time`](crate::Transport::sync_to_mix_time).
//! Mute and volume are page state ([`PlaybackGain`]): they scale the
//! served mix at playback time and never touch the project.
//!
//! Failure rule (CAP-004): a project with no audio plays video
//! silently with no error and no fake track; an undecodable mix
//! blocks play with the cause named.

use notation::model::Fps;

/// Mix sample rate: the render mix is 48 kHz stereo, and the preview
/// mix is the same bytes, so the page clock runs at this rate.
pub const MIX_RATE: u32 = render::render::MIX_RATE;
/// Mix channels: stereo, interleaved left/right.
pub const MIX_CHANNELS: u32 = render::render::MIX_CHANNELS;

/// Facts about one served mix the page clocks from (Sprint 14, T004):
/// the rate, the channel count, the sample-frame count, and the
/// composition length the mix covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixInfo {
    /// Sample rate in Hz (always [`MIX_RATE`]).
    pub sample_rate: u32,
    /// Channel count (always [`MIX_CHANNELS`], interleaved).
    pub channels: u32,
    /// Sample frames (one frame is `channels` samples).
    pub sample_frames: usize,
    /// Composition length in seconds the mix covers.
    pub duration_secs: f64,
}

impl MixInfo {
    /// Facts for a mix of `sample_frames` sample frames covering
    /// `duration_secs` seconds.
    pub fn new(sample_frames: usize, duration_secs: f64) -> Self {
        Self {
            sample_rate: MIX_RATE,
            channels: MIX_CHANNELS,
            sample_frames,
            duration_secs,
        }
    }

    /// Facts for interleaved stereo s16 `pcm` covering `duration_secs`
    /// seconds.
    pub fn for_pcm(pcm_len: usize, duration_secs: f64) -> Self {
        Self::new(pcm_len / MIX_CHANNELS as usize, duration_secs)
    }

    /// Length of the mix in seconds (`sample_frames / rate`).
    pub fn mix_secs(&self) -> f64 {
        self.sample_frames as f64 / self.sample_rate as f64
    }

    /// Sample-frame index sounding at `secs` seconds into the mix,
    /// clamped to the last frame; non-positive or non-finite times
    /// point at frame 0.
    pub fn sample_for_time(&self, secs: f64) -> usize {
        if !secs.is_finite() || secs <= 0.0 || self.sample_frames == 0 {
            return 0;
        }
        ((secs * self.sample_rate as f64).floor() as usize).min(self.sample_frames - 1)
    }

    /// Time in seconds of sample-frame `sample`, clamped to the mix
    /// length.
    pub fn time_for_sample(&self, sample: usize) -> f64 {
        sample.min(self.sample_frames.saturating_sub(1)) as f64 / self.sample_rate as f64
    }

    /// Timeline frame showing at `mix_secs` seconds of mix time: the
    /// mix clock owns the playhead, so the page maps each audio
    /// timeupdate through here instead of advancing by wall clock.
    /// Clamped to `0..frame_count`; non-positive or non-finite times
    /// point at frame 0, times past the end park on the last frame.
    pub fn frame_for_mix_time(fps: Fps, frame_count: u64, mix_secs: f64) -> u64 {
        if frame_count == 0 {
            return 0;
        }
        let rate = fps.value();
        if !rate.is_finite() || rate <= 0.0 || !mix_secs.is_finite() || mix_secs <= 0.0 {
            return 0;
        }
        ((mix_secs * rate).floor() as u64).min(frame_count - 1)
    }

    /// Mix time in seconds of timeline `frame` at `fps`
    /// (`frame / fps`).
    pub fn mix_time_for_frame(fps: Fps, frame: u64) -> f64 {
        let rate = fps.value();
        if !rate.is_finite() || rate <= 0.0 {
            return 0.0;
        }
        frame as f64 / rate
    }
}

impl std::fmt::Display for MixInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mix {} frames ({} channels at {} Hz, {:.3}s)",
            self.sample_frames, self.channels, self.sample_rate, self.duration_secs
        )
    }
}

/// What pressing play sounds (Sprint 14, T004): either the render mix
/// or intentional silence — never a fake track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioPlay {
    /// The project carries no audible sound: the page plays video
    /// silently with no error.
    Silent,
    /// The project mixes sound: the page plays the served mix and
    /// clocks the playhead from it.
    Sound(MixInfo),
}

impl AudioPlay {
    /// Mix facts when this play sounds the mix, `None` for silence.
    pub fn info(&self) -> Option<MixInfo> {
        match *self {
            AudioPlay::Silent => None,
            AudioPlay::Sound(info) => Some(info),
        }
    }

    /// Whether pressing play starts audible sound.
    pub fn has_sound(&self) -> bool {
        matches!(*self, AudioPlay::Sound(_))
    }
}

/// Why a preview mix refused to serve (Sprint 14, T004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixErrorKind {
    /// A mix source cannot be decoded: names the file.
    Undecodable,
    /// The mix computation itself refused (for example a loudness
    /// normalization with no audible mix to measure): names the cause.
    Refused,
}

/// A blocked preview mix (Sprint 14, T004): the kind plus a message
/// naming the cause with its fix — the same shape the frame slice
/// refuses with, so the page shows it as-is. Nothing plays on this
/// error: play stays blocked until the cause is fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixError {
    /// Why the mix blocked.
    pub kind: MixErrorKind,
    /// Human message naming the cause and the fix.
    pub message: String,
}

impl MixError {
    /// Restate a render mix-preview failure as a preview refusal: the
    /// same cause, but naming `preview` and the blocked play instead
    /// of the render output.
    pub(crate) fn from_preview_error(e: render::render::PreviewError) -> Self {
        let message = crate::previewize(e.message().to_string());
        // Decode failures name the file; anything else (silent
        // normalization) is a mix refusal with the render-stage cause.
        let kind = if message.contains("cannot be decoded") {
            MixErrorKind::Undecodable
        } else {
            MixErrorKind::Refused
        };
        Self { kind, message }
    }
}

impl std::fmt::Display for MixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MixError {}

/// Why pressing play with sound refused (Sprint 14, T004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayAudioErrorKind {
    /// The mix cannot be decoded or computed: play is blocked.
    MixBlocked,
    /// The mix is fine but the transport refused (an empty timeline):
    /// the transport names its reason.
    TransportRefused,
}

/// A refused press of play with sound (Sprint 14, T004): the kind
/// plus a message naming the control, the cause, and the fix. The
/// playhead never moves on this error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayAudioError {
    /// Why play refused.
    pub kind: PlayAudioErrorKind,
    /// Human message naming the cause and the fix.
    pub message: String,
}

impl PlayAudioError {
    /// Play blocked by the mix: the mix message already names the
    /// file or cause with its fix.
    pub fn mix(e: MixError) -> Self {
        Self {
            kind: PlayAudioErrorKind::MixBlocked,
            message: format!("preview refused: play is blocked: {e}. No sound plays."),
        }
    }

    /// Play refused by the transport: the transport message already
    /// names the control, the cause, and the fix.
    pub fn transport(e: crate::TransportError) -> Self {
        Self {
            kind: PlayAudioErrorKind::TransportRefused,
            message: e.message.clone(),
        }
    }
}

impl std::fmt::Display for PlayAudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PlayAudioError {}

/// Page-side mute and volume (Sprint 14, T004 / CAP-004): scaling the
/// served mix at playback time. Page-owned — construction and every
/// setter take no project path, so riding mute or volume can never
/// write into the project.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackGain {
    muted: bool,
    volume: f64,
}

/// Why a volume change refused (Sprint 14, T004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainErrorKind {
    /// The requested level is not a usable page volume.
    BadVolume,
}

/// A refused volume change (Sprint 14, T004): the message names the
/// cause and the accepted range, and the level stays where it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GainError {
    /// Why the change refused.
    pub kind: GainErrorKind,
    /// Human message naming the cause and the accepted range.
    pub message: String,
}

impl std::fmt::Display for GainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GainError {}

impl PlaybackGain {
    /// Full volume, unmuted: play sounds the served mix untouched.
    pub fn new() -> Self {
        Self {
            muted: false,
            volume: 1.0,
        }
    }

    /// Whether playback is muted.
    pub fn muted(&self) -> bool {
        self.muted
    }

    /// Page volume level in `0..=1`.
    pub fn volume(&self) -> f64 {
        self.volume
    }

    /// Mute or unmute playback. Always acts — muting an already-muted
    /// page changes nothing and never refuses.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Set the page volume level: only finite levels in `0..=1` act;
    /// anything else refuses naming the accepted range and keeps the
    /// current level.
    pub fn set_volume(&mut self, volume: f64) -> Result<(), GainError> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(GainError {
                kind: GainErrorKind::BadVolume,
                message: format!(
                    "preview refused: volume {volume} is not a usable page level; pass a finite level from 0 (silent) to 1 (full). The level stays {}.",
                    self.volume
                ),
            });
        }
        self.volume = volume;
        Ok(())
    }

    /// Linear gain sounding right now: exact zero when muted, else the
    /// page volume.
    pub fn effective_gain(&self) -> f64 {
        if self.muted {
            0.0
        } else {
            self.volume
        }
    }

    /// Scale interleaved s16 `pcm` by the effective gain for playback:
    /// muting maps every sample to digital silence, a level below full
    /// scales amplitude linearly. The served mix is never rewritten —
    /// this returns fresh playback bytes.
    pub fn apply(&self, pcm: &[i16]) -> Vec<i16> {
        let g = self.effective_gain() as f32;
        if (g - 1.0).abs() < f32::EPSILON {
            return pcm.to_vec();
        }
        pcm.iter()
            .map(|&s| {
                (s as f32 * g)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
            .collect()
    }
}

impl Default for PlaybackGain {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PlaybackGain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.muted {
            write!(f, "muted (page volume {}x)", self.volume)
        } else {
            write!(f, "page volume {}x", self.volume)
        }
    }
}

/// Encode interleaved stereo s16 `pcm` at [`MIX_RATE`] as a WAV the
/// page audio element plays directly: a 44-byte PCM header plus the
/// little-endian samples, no extra chunks.
pub fn encode_wav(pcm: &[i16]) -> Vec<u8> {
    let data_len = pcm.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(MIX_CHANNELS as u16).to_le_bytes());
    out.extend_from_slice(&MIX_RATE.to_le_bytes());
    out.extend_from_slice(&(MIX_RATE * MIX_CHANNELS * 2).to_le_bytes());
    out.extend_from_slice(&((MIX_CHANNELS * 2) as u16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
