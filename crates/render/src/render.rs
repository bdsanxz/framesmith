//! CPU composition renderer (T011–T014 / CAP-008–CAP-014, CAP-018).
//!
//! For a validated composition the renderer:
//!
//! * resolves the text face per clip (first declared project `font`,
//!   else the host system face matching the clip `font-style`, else
//!   the built-in bitmap fallback);
//! * probes every referenced video through the resolved FFmpeg binary
//!   (codec, dimensions, duration, audio presence) and smoke-decodes
//!   audio/video before the first frame, refusing with a diagnostic
//!   naming the file when the source is corrupt, unsupported, too short,
//!   or has undecodable audio;
//! * draws each output frame on the CPU — background fill, video planes
//!   (`contain` / `cover` / `fill`, exact rational-time picture, text in
//!   document order only inside each clip's half-open window) — and pipes
//!   raw RGB24 frames to FFmpeg, which writes one MP4 with H.264 yuv420p
//!   video and AAC stereo 48 kHz audio;
//! * mixes every audible source — each `audio` track with its trim,
//!   volume, and linear fades, plus each `video` clip's sound stream
//!   with the clip's volume and fades — sample-accurately into one
//!   48 kHz stereo mix; a composition with no audible input renders
//!   with the encoder's silent default;
//! * draws picture exactly as before: video sound enters the mix, never
//!   the blitter, so picture bytes are unchanged by the mix;
//! * encodes to a temporary file and renames on success, so an encode or
//!   decode failure stops the render, reports the FFmpeg stage, and never
//!   presents a wrong or truncated MP4 as success.

use glyph::font::Face;
use crate::frame::{FrameDescription, FrameItem, describe};
use crate::motion_plan::{ClipMotion, plan_motions, snap_fps};
use notation::model::{Clip, Composition, Fit};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A probed video source: the facts render needs after validation.
#[derive(Debug, Clone)]
pub(crate) struct ProbedVideo {
    /// Index into `composition` video clips (document order).
    pub(crate) clip_idx: usize,
    pub(crate) path: PathBuf,
    /// Probed codec name, kept for refusal and trace wording.
    #[allow(dead_code)]
    pub(crate) codec: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Source duration in seconds when the container reports one.
    pub(crate) duration: Option<f64>,
    pub(crate) has_audio: bool,
}

/// One clip window decoded to raw pictures with presentation timestamps.
pub(crate) struct DecodedRange {
    /// Index into the `videos` probe list.
    pub(crate) video_idx: usize,
    /// Source dimensions (validates the byte layout).
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Concatenated RGB24 pictures, `framesize` bytes each.
    pub(crate) frames: Vec<u8>,
    /// `pts_time` per picture, in presentation order.
    pub(crate) pts: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Verification surface (T013–T015): prepared media, single frames, workers.
// ---------------------------------------------------------------------------

/// Worker batch bound: one scheduling batch never holds more than this
/// many decoded frames, so worker partitioning cannot grow memory with
/// the composition length.
const WORKER_BATCH: usize = 64;

/// Everything one render needs before the first frame (T013–T015): the
/// validated composition, its motion evaluators, shaped text, probed and
/// decoded media, and the CPU raster cache. Both renderers draw from the
/// same [`Prepared`]: the CPU blitter through [`Prepared::cpu_frame`],
/// the GPU picture path through `gpu::render_frame`.
pub struct Prepared {
    pub(crate) comp: Composition,
    pub(crate) motions: Vec<ClipMotion>,
    pub(crate) frames: usize,
    pub(crate) fonts: layout::Fonts,
    pub(crate) project_family: Option<String>,
    pub(crate) layouts: Vec<Option<layout::Block>>,
    pub(crate) bitmap_fallback: bool,
    pub(crate) videos: Vec<ProbedVideo>,
    pub(crate) ranges: Vec<DecodedRange>,
    pub(crate) images: Vec<DecodedImage>,
    pub(crate) raster: RasterCache,
}

impl Prepared {
    /// Borrow the validated composition (GPU traversal input).
    pub(crate) fn composition(&self) -> &Composition {
        &self.comp
    }

    /// Borrow the per-clip motion evaluators.
    pub(crate) fn motion_ref(&self) -> &Vec<ClipMotion> {
        &self.motions
    }

    /// Output frame count of the prepared composition.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Canvas dimensions `(width, height)` of the prepared composition.
    pub fn canvas(&self) -> (u32, u32) {
        (self.comp.width, self.comp.height)
    }

    /// Draw one output frame alone (T015): the same description and the
    /// same blitter as the full render, so frame N alone equals frame N
    /// from a full render exactly on the CPU renderer. The frame is
    /// returned as raw RGB24 bytes (`width * height * 3`).
    pub fn cpu_frame(&mut self, frame: u64) -> Result<Vec<u8>, String> {
        let desc = describe(&self.comp, &self.motions, frame);
        let mut out = vec![0u8; self.comp.width as usize * self.comp.height as usize * 3];
        draw_description(
            &desc,
            &mut self.raster,
            &self.layouts,
            self.bitmap_fallback,
            &self.videos,
            &self.ranges,
            &self.images,
            &mut out,
        )?;
        Ok(out)
    }
}

/// Default worker count for frame scheduling (T004): the machine's own
/// parallelism, clamped to the 1-through-64 scheduling range (1 when the
/// count cannot be read). Drawing is embarrassingly parallel — frame N
/// depends only on N — so idle cores only ever cost wall time.
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 64)
}

/// Logical worker count for frame scheduling (T015/T004):
/// `FRAMESMITH_WORKERS` wins when it parses as 1 through 64, else the
/// machine default. Workers never change the picture — they only change
/// the order frames are computed in.
pub fn worker_count() -> usize {
    std::env::var("FRAMESMITH_WORKERS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&w| w >= 1 && w <= 64)
        .unwrap_or_else(default_workers)
}

/// Per-thread CPU raster-face cache (Sprint 3 T003): every parallel
/// worker owns one, so workers share only immutable prepared media and
/// still draw byte-identical pictures at any worker count.
pub type RasterCache = HashMap<(usize, u32), Face>;

/// Render backend requested or resolved for one render (Sprint 3 T001).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Gpu,
    Auto,
}

impl Backend {
    /// Exactly the three accepted spellings; anything else is refused
    /// before the first frame with no MP4 written.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "cpu" => Some(Backend::Cpu),
            "gpu" => Some(Backend::Gpu),
            "auto" => Some(Backend::Auto),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Backend::Cpu => "cpu",
            Backend::Gpu => "gpu",
            Backend::Auto => "auto",
        }
    }
}

/// Resolve the `--backend` flag to the renderer that draws: `auto` (the
/// default) uses the GPU renderer when the capability probe reports one
/// available, else the CPU renderer. The returned name is the effective
/// backend the output and the run log name.
pub fn resolve_backend(flag: Backend) -> Backend {
    match flag {
        Backend::Auto => {
            if crate::gpu::gpu_available() {
                Backend::Gpu
            } else {
                Backend::Cpu
            }
        }
        other => other,
    }
}

/// Quality preset for one render (Sprint 3 T002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Draft,
    Standard,
    High,
}

impl Preset {
    /// Exactly the three accepted spellings; anything else is refused
    /// before the first frame with no MP4 written.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "draft" => Some(Preset::Draft),
            "standard" => Some(Preset::Standard),
            "high" => Some(Preset::High),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Preset::Draft => "draft",
            Preset::Standard => "standard",
            Preset::High => "high",
        }
    }

    /// Exact encoder numbers behind the preset, recorded next to the
    /// fixtures (`crates/cli/fixtures/PIPELINE.md`). `standard` carries
    /// no explicit flags so its bytes match the Sprint 2 output exactly;
    /// `draft` trades fidelity for size and speed, `high` spends size
    /// for strictly more fidelity.
    pub fn encoder(self) -> EncoderSettings {
        match self {
            // CRF 30 + fastest trade-off: visibly lighter, faster encode.
            Preset::Draft => EncoderSettings {
                crf: 30,
                x264_preset: Some("veryfast"),
            },
            // Sprint 2 output quality: libx264 defaults (CRF 23,
            // `medium`), passed through with no explicit flags.
            Preset::Standard => EncoderSettings {
                crf: 23,
                x264_preset: None,
            },
            // CRF 18 + slow: strictly more picture fidelity, larger file.
            Preset::High => EncoderSettings {
                crf: 18,
                x264_preset: Some("slow"),
            },
        }
    }
}

/// libx264 encoder numbers resolved for one render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncoderSettings {
    /// Constant-rate factor 0 (lossless) through 51 (worst).
    pub crf: u8,
    /// x264 speed/fidelity trade-off, or `None` for the encoder default.
    pub x264_preset: Option<&'static str>,
}

/// Per-render choices from the CLI flags (Sprint 3 T001/T002).
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Effective drawing backend (already resolved through the probe).
    pub backend: Backend,
    /// Active quality preset (`standard` when no preset is named).
    pub preset: Preset,
    /// `--crf` override of the preset's CRF axis, if given.
    pub crf_override: Option<u8>,
    /// `--video-bitrate` override of the bitrate axis, if given.
    pub video_bitrate: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            backend: Backend::Cpu,
            preset: Preset::Standard,
            crf_override: None,
            video_bitrate: None,
        }
    }
}

impl RenderOptions {
    /// Effective encoder numbers: each override touches only its own
    /// axis of the active preset; `--video-bitrate` adds a bitrate
    /// ceiling alongside the preset's CRF.
    pub fn encoder(&self) -> ResolvedEncoder {
        let base = self.preset.encoder();
        ResolvedEncoder {
            crf: self.crf_override.unwrap_or(base.crf),
            x264_preset: base.x264_preset,
            video_bitrate: self.video_bitrate.clone(),
            from_preset: self.preset,
        }
    }

    /// Human announcement of the active preset and effective settings,
    /// printed in the render output and the run log (T002).
    pub fn announce(&self) -> String {
        let enc = self.encoder();
        let mut parts = vec![format!("preset={}", self.preset.name())];
        parts.push(format!("crf={}", enc.crf));
        match enc.x264_preset {
            Some(p) => parts.push(format!("x264-preset={p}")),
            None => parts.push("x264-preset=default (medium)".to_string()),
        }
        match &enc.video_bitrate {
            Some(b) => parts.push(format!("video-bitrate={b}")),
            None => parts.push("video-bitrate=preset default".to_string()),
        }
        parts.join(", ")
    }
}

/// Encoder numbers resolved for one render (preset plus overrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEncoder {
    pub crf: u8,
    pub x264_preset: Option<&'static str>,
    pub video_bitrate: Option<String>,
    pub from_preset: Preset,
}

/// One staged progress entry (Sprint 3 T006): the stage name, its
/// outcome, and its measured wall-clock duration. Durations are
/// measured with a monotonic clock, never modeled; no entry carries an
/// estimated completion time.
#[derive(Debug, Clone)]
pub struct StageTiming {
    pub name: &'static str,
    pub status: &'static str,
    pub duration_ms: u64,
    pub detail: String,
}

impl StageTiming {
    pub fn ok(name: &'static str, duration_ms: u64, detail: String) -> Self {
        Self {
            name,
            status: "ok",
            duration_ms,
            detail,
        }
    }

    pub fn failed(name: &'static str, duration_ms: u64, detail: String) -> Self {
        Self {
            name,
            status: "failed",
            duration_ms,
            detail,
        }
    }
}

/// Visit order for `total` positions across `workers` (T015): round-robin
/// strides, so worker `w` owns positions `w, w + workers, ...`. Every
/// position keeps its own frame number; only the visit order changes.
/// The encoder consumes consecutive batches, so this distributes the
/// positions *within* one consecutive batch — batches themselves always
/// cover consecutive frames, keeping every output frame at its own time.
pub fn worker_order(total: usize, workers: usize) -> Vec<u64> {
    let workers = workers.max(1);
    let mut order = Vec::with_capacity(total);
    for w in 0..workers {
        let mut k = w;
        while k < total {
            order.push(k as u64);
            k += workers;
        }
    }
    order
}

/// Run the picture pre-first-frame steps of [`render_composition`] —
/// fonts, motion evaluators, text layouts, video probes with video
/// smoke decodes, clip-window decodes, and image decodes — and return
/// the shared [`Prepared`] both renderers draw from. Audio-stream
/// smoke decodes and the 48 kHz mix (tracks plus video sound) live in
/// [`build_audio_plan`] (the audio stage, Sprint 3 T008/T010), which
/// runs after this step. Any failure names its file or stage and writes
/// no MP4, exactly as the full render would.
pub fn prepare_frames(
    comp: &Composition,
    folder: &Path,
    ffmpeg_bin: &Path,
) -> Result<Prepared, String> {
    // Text faces: first declared project font, else the host system
    // Text engine: parley shaping over the project font plus the
    // machine's fonts, with per-character fallback and FFmpeg-decoded
    // emoji. The project font (first declared `font`) names its own
    // family; without one the generic families resolve on the machine.
    // Origins are always named in the success line.
    let mut fonts = layout::Fonts::new();
    let mut project_family: Option<String> = None;
    // Every declared project font registers under its own family so an
    // explicit `font-family` can select any of them; the first declared
    // font stays the default family.
    for (index, decl) in comp.fonts.iter().enumerate() {
        let bytes = std::fs::read(folder.join(&decl.path)).map_err(|e| {
            format!(
                "render refused: font file {:?} cannot be read: {e}; no MP4 was written",
                decl.path
            )
        })?;
        let family = glyph::tables::family_name(&bytes, 0).unwrap_or_else(|| decl.path.clone());
        fonts.register(bytes, Some(&family)).map_err(|e| {
            let reason = match e {
                layout::FontError::Unsupported(kind) => format!("{kind} is not supported; use a TrueType (.ttf) font with TrueType outlines"),
                layout::FontError::NotAFont => "the file is not a recognized font".to_string(),
                layout::FontError::Collection(reason) => reason,
                layout::FontError::Damaged(reason) => reason,
            };
            format!("render refused: font file {:?} {reason}; no MP4 was written", decl.path)
        })?;
        if index == 0 {
            project_family = Some(family);
        }
    }
    // Motion: exact-time clock plus one evaluator per clip. Validated
    // `tween`/`set` directives (T003) ride in the composition; every
    // clip still holds its own values until tween/set evaluation lands
    // (T009/T010/T011) — the evaluator already runs on every frame.
    let fps_snap = snap_fps(&comp.fps);
    let motions = plan_motions(comp, fps_snap);
    // Text layouts: one parley block per text clip, shaped once up
    // front (text is static within a clip). A layout the machine cannot
    // serve refuses before the first frame; with no font anywhere the
    // announced bitmap fallback draws instead of a blank.
    let raster: RasterCache = HashMap::new();
    let mut layouts: Vec<Option<layout::Block>> = Vec::with_capacity(comp.clips.len());
    let mut bitmap_fallback = false;
    for clip in &comp.clips {
        let Clip::Text(tc) = clip else {
            layouts.push(None);
            continue;
        };
        // The style family: the project family when declared, else the
        // system variant file for the clip style (`Arial Bold`...), else
        // the generic fallback inside layout. Names the same basenames
        // as the old per-style candidate files.
        let bold = matches!(tc.style, notation::model::FontStyle::Bold | notation::model::FontStyle::BoldItalic);
        let italic = matches!(tc.style, notation::model::FontStyle::Italic | notation::model::FontStyle::BoldItalic);
        // Explicit `font-family` wins in written try order (project
        // families, then machine families); a list nothing resolves
        // refuses here instead of silently falling back.
        let families = match &tc.font_family {
            Some(names) => {
                if !fonts.resolves_any(names) {
                    return Err(format!(
                        "render refused: text {:?} names unknown font-family {:?}; no MP4 was written",
                        tc.value,
                        names.join(", ")
                    ));
                }
                names.clone()
            }
            None => match &project_family {
                Some(family) => vec![family.clone()],
                None => fonts.resolve_system_style(bold, italic).into_iter().collect(),
            },
        };
        let style = layout::Style {
            families,
            size: tc.size as f32,
            weight: if bold { 700.0 } else { 400.0 },
            italic,
            align: match tc.align {
                notation::model::Align::Left => layout::Align::Left,
                notation::model::Align::Center => layout::Align::Center,
                notation::model::Align::Right => layout::Align::Right,
            },
            colour: [tc.color.r, tc.color.g, tc.color.b, 255],
            line_height: tc.line_height.map(|v| v as f32),
            letter_spacing: tc.letter_spacing.map(|v| v as f32),
        };
        let wrap = match tc.wrap {
            notation::model::Wrap::Off => None,
            notation::model::Wrap::Width(w) => Some(w as f32),
        };
        match layout::layout(&mut fonts, &tc.value, &style, wrap, Some(ffmpeg_bin)) {
            Ok(block) => layouts.push(Some(block)),
            Err(layout::TextError::NoFamily) if project_family.is_none() => {
                bitmap_fallback = true;
                layouts.push(None);
            }
            Err(e) => {
                return Err(format!("render refused: text {:?} cannot be laid out ({e}); no MP4 was written", tc.value));
            }
        }
    }

    // Probe every referenced video before the first frame.
    let mut videos: Vec<ProbedVideo> = Vec::new();
    for (idx, clip) in comp.clips.iter().enumerate() {
        if let Clip::Video(v) = clip {
            let probed = probe_video(ffmpeg_bin, &folder.join(&v.path), &v.path)?;
            videos.push(ProbedVideo { clip_idx: idx, ..probed });
        }
    }
    // Smoke-decode each video stream so a corrupt file or an
    // undecodable picture refuses before the first frame, naming the
    // file. Audio streams are smoke-decoded in the audio stage
    // ([`build_audio_plan`]), which names the same file on failure.
    for pv in &videos {
        smoke_decode(ffmpeg_bin, pv, false)?;
        // The source must cover the whole visible clip window.
        if let Some(clip) = comp.clips.get(pv.clip_idx) {
            let (s, d) = (clip.start(), clip.duration());
            let end = (s + d).min(comp.duration);
            if let Some(src_dur) = pv.duration {
                if end - s > src_dur + 1e-6 {
                    return Err(format!(
                        "render refused: video source {:?} is {src_dur:.3}s but the clip window needs {:.3}s of picture; no MP4 was written",
                        rel_name(folder, &pv.path),
                        end - s
                    ));
                }
            }
        }
    }
    // Decode each visible clip window once, with timestamps. Output
    // frames then select pictures by exact time: no drift, no per-frame
    // processes, and no stale-frame holding past the source end.
    let mut ranges: Vec<DecodedRange> = Vec::with_capacity(videos.len());
    for (vi, pv) in videos.iter().enumerate() {
        let win = match comp.clips.get(pv.clip_idx) {
            Some(Clip::Video(v)) => clip_window_len(comp, v),
            _ => 0.0,
        };
        if win <= 0.0 {
            // Never visible (fully past the composition end): keep a
            // placeholder so `ranges` stays aligned with `videos`.
            ranges.push(DecodedRange {
                video_idx: vi,
                width: pv.width,
                height: pv.height,
                frames: Vec::new(),
                pts: Vec::new(),
            });
            continue;
        }
        let mut range = decode_range(ffmpeg_bin, pv, win)?;
        range.video_idx = vi;
        ranges.push(range);
    }
    // Decode every referenced image once (T005): a PNG or JPEG still
    // decoded to straight RGBA through FFmpeg. A file validation
    // accepted but FFmpeg cannot decode refuses before the first frame,
    // naming the file — never a partial MP4.
    let mut images: Vec<DecodedImage> = Vec::new();
    for clip in &comp.clips {
        if let Clip::Image(im) = clip {
            if images.iter().any(|d: &DecodedImage| d.path == folder.join(&im.path)) {
                continue;
            }
            images.push(decode_image(ffmpeg_bin, folder, &im.path)?);
        }
    }

    let frames = comp.fps.frame_count(comp.duration) as usize;
    if frames == 0 {
        return Err("render refused: the composition has no frames; no MP4 was written".to_string());
    }

    Ok(Prepared {
        comp: comp.clone(),
        motions,
        frames,
        fonts,
        project_family,
        layouts,
        bitmap_fallback,
        videos,
        ranges,
        images,
        raster,
    })
}

/// Historical entry point preserved for existing callers: every
/// pre-first-frame step of [`prepare_frames`] plus audio-stream smoke
/// decodes. The CLI render path uses [`prepare_frames`] plus
/// [`build_audio_plan`] so the frames stage and the audio stage time
/// separately (Sprint 3 T006).
pub fn prepare(
    comp: &Composition,
    folder: &Path,
    ffmpeg_bin: &Path,
) -> Result<Prepared, String> {
    let prep = prepare_frames(comp, folder, ffmpeg_bin)?;
    smoke_audio_streams(ffmpeg_bin, &prep.videos)?;
    Ok(prep)
}

/// Smoke-decode every audible video stream so a corrupt file or an
/// undecodable audio stream refuses before the first frame, naming the
/// file. Runs inside the audio stage ([`build_audio_plan`]); kept as a
/// separate step so [`prepare`] preserves its historical behavior.
pub(crate) fn smoke_audio_streams(ffmpeg_bin: &Path, videos: &[ProbedVideo]) -> Result<(), String> {
    for pv in videos.iter().filter(|v| v.has_audio) {
        smoke_decode(ffmpeg_bin, pv, true)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Framesmith 48 kHz stereo mix (Sprint 3 T008 / CAP-009 + CAP-010).
// ---------------------------------------------------------------------------

/// Framesmith mix sample rate: every audible sound lands in one 48 kHz
/// stereo mix, and a soundless composition still carries a full-duration
/// 48 kHz stereo track of digital silence.
pub const MIX_RATE: u32 = 48_000;
/// Mix channels: stereo.
pub const MIX_CHANNELS: u32 = 2;

/// Audio inputs for the encoder: FFmpeg-handled video-sound segments
/// (always empty since Sprint 3 T010 moved video sound into the mix;
/// retained for the encoder wiring) plus the Framesmith PCM mix of
/// every audible `audio` track and `video` sound stream.
pub struct AudioPlan {
    pub video_segs: Vec<AudioSeg>,
    /// Staged raw s16le 48 kHz stereo mix, or `None` when the
    /// composition carries no audible sound at all.
    pub mix_pcm: Option<PathBuf>,
}

/// Staged mix path beside the final MP4 (same filesystem, removed after
/// the encode whatever the outcome).
pub fn mix_staging_path(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_owned();
    name.push(".framesmith-mix.pcm");
    PathBuf::from(name)
}

/// Convert a validated `volume` in dB to a linear amplitude gain:
/// silence (`-inf`) is exact zero, values at most +12 dB become their
/// gain. Anything above +12 dB is refused, never silently limited, so
/// the mix bus never clips behind the author's back.
pub fn db_to_gain(db: f64) -> Result<f32, String> {
    if db == f64::NEG_INFINITY {
        return Ok(0.0);
    }
    if !db.is_finite() || db > 12.0 {
        return Err(format!(
            "render refused: volume {db} dB is above the +12 dB ceiling; gain is never limited silently, no MP4 was written"
        ));
    }
    Ok(10f64.powf(db / 20.0) as f32)
}

/// Linear fade factor of sample `i` of `n` placed samples with a fade-in
/// over the first `fade_in_n` samples and a fade-out over the last
/// `fade_out_n` samples. Both fades are linear in amplitude; a zero fade
/// leaves that edge at unity.
pub fn fade_factor(i: usize, n: usize, fade_in_n: usize, fade_out_n: usize) -> f32 {
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

/// Add one decoded stereo s16 track into the f32 mix accumulator at
/// `offset` samples with `gain` and linear fades: overlapping tracks
/// sum sample by sample (no limiter, no normalization), so the mix is
/// deterministic — the same inputs always add to the same samples.
pub fn mix_into(
    acc: &mut [f32],
    pcm: &[i16],
    offset: usize,
    gain: f32,
    fade_in_n: usize,
    fade_out_n: usize,
) {
    let channels = MIX_CHANNELS as usize;
    let frames = pcm.len() / channels;
    let capacity = acc.len() / channels;
    let n = frames.min(capacity.saturating_sub(offset.min(capacity)));
    for i in 0..n {
        let g = gain * fade_factor(i, n, fade_in_n.min(n), fade_out_n.min(n));
        for c in 0..channels {
            acc[(offset + i) * channels + c] += pcm[i * channels + c] as f32 / 32768.0 * g;
        }
    }
}

/// Decode `take` seconds of `abs` from `trim_start` to raw s16le 48 kHz
/// stereo. A source that ends early decodes short and the caller pads
/// silence; a source FFmpeg cannot open or decode refuses naming the
/// file with the audio stage.
fn decode_audio_source(
    ffmpeg_bin: &Path,
    abs: &Path,
    rel: &str,
    trim_start: f64,
    take: f64,
) -> Result<Vec<i16>, String> {
    let out = Command::new(ffmpeg_bin)
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
        .map_err(|e| {
            format!("render refused: audio source {rel:?} cannot be decoded ({e}) (audio stage); no MP4 was written")
        })?;
    if !out.status.success() {
        let tail = stderr_tail(&out.stderr, 3);
        return Err(format!(
            "render refused: audio source {rel:?} cannot be decoded{} (audio stage); the system never renders a silent replacement; no MP4 was written",
            if tail.is_empty() { String::new() } else { format!(" ({tail})") }
        ));
    }
    let bytes = out.stdout;
    let (pairs, _) = bytes.as_chunks::<2>();
    let mut pcm = Vec::with_capacity(pairs.len());
    for pair in pairs {
        pcm.push(i16::from_le_bytes(*pair));
    }
    Ok(pcm)
}

/// Decode one sound source for the mix, translating a decode failure
/// during cancellation into the cancellation itself so a Ctrl-C that
/// kills the decoder reports exit 130 instead of a decode failure.
fn decode_mix_source(
    ffmpeg_bin: &Path,
    abs: &Path,
    rel: &str,
    trim_start: f64,
    take: f64,
) -> Result<Vec<i16>, String> {
    match decode_audio_source(ffmpeg_bin, abs, rel, trim_start, take) {
        Ok(pcm) => Ok(pcm),
        Err(e) => {
            if crate::cancel::is_cancelled() {
                Err(crate::cancel::error("audio"))
            } else {
                Err(e)
            }
        }
    }
}

/// Place decoded stereo frames into the mix at `start` seconds with
/// `gain` and linear fades, clamping to the composition length.
#[allow(clippy::too_many_arguments)]
fn place_into_mix(
    acc: &mut [f32],
    pcm: &[i16],
    total: usize,
    start: f64,
    take: f64,
    gain: f32,
    fade_in: f64,
    fade_out: f64,
) {
    let offset = (start * MIX_RATE as f64).round().max(0.0) as usize;
    let n_frames = pcm.len() / MIX_CHANNELS as usize;
    let placed = n_frames.min(total.saturating_sub(offset.min(total)));
    let placed = placed.min((take * MIX_RATE as f64).ceil() as usize);
    let fade_in_n = ((fade_in * MIX_RATE as f64).round() as usize).min(placed);
    let fade_out_n = ((fade_out * MIX_RATE as f64).round() as usize).min(placed);
    mix_into(
        acc,
        &pcm[..placed * MIX_CHANNELS as usize],
        offset.min(total),
        gain,
        fade_in_n,
        fade_out_n,
    );
}

/// Build the audio plan (audio stage, Sprint 3 T008/T010): smoke-decode
/// every audible video stream, then decode each audible source — every
/// `audio` track with its trim, plus every `video` clip's sound stream
/// with the clip's volume and fades — to 48 kHz stereo PCM, place each
/// sample-accurately at its written start, and sum all sources in
/// composition document order into one deterministic staged mix file.
///
/// Every audible sound starts within 1 ms of its written time: offsets
/// round to the nearest 48 kHz sample (at most half a sample of error).
/// The mix is byte-identical across reruns and worker counts (one fixed
/// summation order, no randomness). Video without a sound stream
/// contributes nothing and is not an error; without declared volume or
/// fades the video sound mixes at unity gain.
pub fn build_audio_plan(
    comp: &Composition,
    folder: &Path,
    ffmpeg_bin: &Path,
    prep: &Prepared,
    mix_path: &Path,
) -> Result<AudioPlan, String> {
    if crate::cancel::is_cancelled() {
        return Err(crate::cancel::error("audio"));
    }
    let videos = &prep.videos;
    smoke_audio_streams(ffmpeg_bin, videos)?;
    if crate::cancel::is_cancelled() {
        return Err(crate::cancel::error("audio"));
    }

    // Audibility scan: an `audio` track with a non-empty window clamped
    // to the composition, or a `video` clip with a sound stream and a
    // non-empty window. Anything else contributes nothing.
    let mut audible = false;
    for (idx, clip) in comp.clips.iter().enumerate() {
        match clip {
            Clip::Audio(a) => {
                let end = (a.start + a.duration).min(comp.duration);
                if end > a.start && a.start < comp.duration {
                    audible = true;
                    break;
                }
            }
            Clip::Video(v) => {
                let end = (v.start + v.duration).min(comp.duration);
                if end > v.start
                    && v.start < comp.duration
                    && videos.iter().any(|p| p.clip_idx == idx && p.has_audio)
                {
                    audible = true;
                    break;
                }
            }
            _ => {}
        }
    }
    if !audible {
        return Ok(AudioPlan {
            video_segs: Vec::new(),
            mix_pcm: None,
        });
    }

    let total = (comp.duration * MIX_RATE as f64).ceil().max(1.0) as usize;
    let mut acc = vec![0.0f32; total * MIX_CHANNELS as usize];
    for (idx, clip) in comp.clips.iter().enumerate() {
        if crate::cancel::is_cancelled() {
            return Err(crate::cancel::error("audio"));
        }
        match clip {
            Clip::Audio(a) => {
                let end = (a.start + a.duration).min(comp.duration);
                if end <= a.start || a.start >= comp.duration {
                    continue;
                }
                let window = (end - a.start).max(0.0);
                let avail = a
                    .trim_end
                    .map(|e| (e - a.trim_start).max(0.0))
                    .unwrap_or(window);
                let take = window.min(avail);
                if take <= 0.0 {
                    continue;
                }
                let pcm = decode_mix_source(
                    ffmpeg_bin,
                    &folder.join(&a.path),
                    &a.path,
                    a.trim_start,
                    take,
                )?;
                let gain = db_to_gain(a.volume_db)?;
                place_into_mix(&mut acc, &pcm, total, a.start, take, gain, a.fade_in, a.fade_out);
            }
            Clip::Video(v) => {
                // Soundless video contributes nothing and is not an error.
                let pv = match videos.iter().find(|p| p.clip_idx == idx && p.has_audio) {
                    Some(pv) => pv,
                    None => continue,
                };
                let end = (v.start + v.duration).min(comp.duration);
                if end <= v.start || v.start >= comp.duration {
                    continue;
                }
                let take = (end - v.start).max(0.0);
                if take <= 0.0 {
                    continue;
                }
                let rel = rel_name(folder, &pv.path);
                let pcm = decode_mix_source(ffmpeg_bin, &pv.path, &rel, 0.0, take)?;
                let gain = db_to_gain(v.volume_db)?;
                place_into_mix(&mut acc, &pcm, total, v.start, take, gain, v.fade_in, v.fade_out);
            }
            _ => {}
        }
    }
    let mut bytes = Vec::with_capacity(acc.len() * 2);
    for &x in &acc {
        let v = (x.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    if let Some(parent) = mix_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("render refused: cannot create output directory {}: {e} (audio stage).", parent.display())
            })?;
        }
    }
    std::fs::write(mix_path, &bytes).map_err(|e| {
        format!("render failed: cannot stage the audio mix {}: {e} (audio stage)", mix_path.display())
    })?;
    Ok(AudioPlan {
        video_segs: Vec::new(),
        mix_pcm: Some(mix_path.to_path_buf()),
    })
}

// ---------------------------------------------------------------------------
// Parallel frame drawing (Sprint 3 T003).
// ---------------------------------------------------------------------------

/// Shared immutable draw inputs for parallel workers (Sprint 3 T003).
/// Every field is `Sync` — unlike [`Prepared`], which also carries the
/// thread-local raster-face cache whose glyph faces are not `Sync` — so
/// `&DrawView` crosses threads while each worker owns its raster cache.
pub(crate) struct DrawView<'a> {
    comp: &'a Composition,
    motions: &'a Vec<ClipMotion>,
    videos: &'a [ProbedVideo],
    ranges: &'a [DecodedRange],
    images: &'a [DecodedImage],
    layouts: &'a [Option<layout::Block>],
    bitmap_fallback: bool,
}

impl Prepared {
    /// Borrow the thread-shareable draw inputs of this preparation.
    pub(crate) fn draw_view(&self) -> DrawView<'_> {
        DrawView {
            comp: &self.comp,
            motions: &self.motions,
            videos: &self.videos,
            ranges: &self.ranges,
            images: &self.images,
            layouts: &self.layouts,
            bitmap_fallback: self.bitmap_fallback,
        }
    }
}

/// Draw one output frame on the effective backend from shared draw
/// inputs with a caller-owned raster cache. Frame N depends only on N,
/// so parallel workers drawing different frames still emit identical
/// bytes to a serial run.
pub(crate) fn draw_one_frame_view(
    view: &DrawView,
    raster: &mut RasterCache,
    backend: Backend,
    frame: u64,
) -> Result<Vec<u8>, String> {
    let desc = describe(view.comp, view.motions, frame);
    match backend {
        Backend::Gpu => crate::gpu::render_description_split(
            view.videos,
            view.ranges,
            view.images,
            view.layouts,
            view.bitmap_fallback,
            raster,
            &desc,
        ),
        Backend::Cpu | Backend::Auto => {
            let mut out = vec![0u8; desc.width as usize * desc.height as usize * 3];
            draw_description(
                &desc,
                raster,
                view.layouts,
                view.bitmap_fallback,
                view.videos,
                view.ranges,
                view.images,
                &mut out,
            )?;
            Ok(out)
        }
    }
}

/// Draw one output frame on the effective backend from shared prepared
/// media with a caller-owned raster cache (serial entry point; the
/// parallel path uses [`draw_batch_parallel`]).
pub fn draw_one_frame(
    prep: &Prepared,
    raster: &mut RasterCache,
    backend: Backend,
    frame: u64,
) -> Result<Vec<u8>, String> {
    draw_one_frame_view(&prep.draw_view(), raster, backend, frame)
}

/// Draw `frames` in parallel across [`worker_count`] workers with
/// thread-local raster caches and return them in frame order. Any worker
/// count draws byte-identical pictures; a worker failure aborts the
/// whole batch with the first underlying reason named.
pub fn draw_batch_parallel(
    prep: &Prepared,
    backend: Backend,
    frames: &[u64],
) -> Result<Vec<(u64, Vec<u8>)>, String> {
    draw_batch_parallel_with_workers(prep, backend, frames, worker_count())
}

/// [`draw_batch_parallel`] with an explicit worker count (unit-testable
/// without touching the process environment; the CLI always passes
/// [`worker_count`]). Positions within the batch visit round-robin
/// ([`worker_order`]); frame numbers travel with their pixels and the
/// batch is re-sorted, so any worker count emits the batch's frames in
/// ascending order with identical bytes.
pub(crate) fn draw_batch_parallel_with_workers(
    prep: &Prepared,
    backend: Backend,
    frames: &[u64],
    workers: usize,
) -> Result<Vec<(u64, Vec<u8>)>, String> {
    let view = prep.draw_view();
    let workers = workers.max(1).min(frames.len().max(1));
    if workers <= 1 || frames.len() <= 1 {
        let mut raster = RasterCache::new();
        let mut out = Vec::with_capacity(frames.len());
        for &k in frames {
            out.push((k, draw_one_frame_view(&view, &mut raster, backend, k)?));
        }
        return Ok(out);
    }
    let visit = worker_order(frames.len(), workers);
    let chunk = visit.len().div_ceil(workers);
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        let shared: &DrawView = &view;
        for piece in visit.chunks(chunk) {
            let piece: Vec<u64> = piece.iter().map(|&p| frames[p as usize]).collect();
            handles.push(s.spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut raster = RasterCache::new();
                    let mut got = Vec::with_capacity(piece.len());
                    for k in piece {
                        got.push((k, draw_one_frame_view(shared, &mut raster, backend, k)?));
                    }
                    Ok::<Vec<(u64, Vec<u8>)>, String>(got)
                }));
                match outcome {
                    Ok(inner) => inner,
                    Err(_) => Err(
                        "worker failed: a frame worker panicked; no MP4 was written".to_string(),
                    ),
                }
            }));
        }
        let mut slots: Vec<(u64, Vec<u8>)> = Vec::with_capacity(frames.len());
        let mut first_err: Option<String> = None;
        for h in handles {
            match h.join() {
                Ok(Ok(mut got)) => slots.append(&mut got),
                Ok(Err(e)) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
                Err(_) => {
                    if first_err.is_none() {
                        first_err = Some(
                            "worker failed: a frame worker did not finish; no MP4 was written"
                                .to_string(),
                        );
                    }
                }
            }
        }
        if let Some(e) = first_err {
            return Err(e);
        }
        slots.sort_by_key(|(k, _)| *k);
        Ok(slots)
    })
}

/// Remove the staged work files beside `out` (the encode temp file and
/// the staged audio mix, when present). Success and failure paths both
/// call this so no partial or work file survives at the target.
fn remove_work_files(out: &Path, plan: &AudioPlan) {
    let extra: Vec<PathBuf> = plan.mix_pcm.clone().into_iter().collect();
    let _ = crate::cancel::remove_staged(out, &extra);
}

/// Staged files that must go on cancellation: the encode temp file and
/// the staged mix, when present.
fn staged_extras(plan: &AudioPlan) -> Vec<PathBuf> {
    plan.mix_pcm.clone().into_iter().collect()
}

/// Cancel the in-progress encode: stop the FFmpeg child, remove staged
/// and partial files, and report the interruption with any leftover
/// paths that could not be removed.
fn cancel_encode(child: &mut std::process::Child, out: &Path, plan: &AudioPlan, stage: &str) -> String {
    let _ = child.kill();
    let _ = child.wait();
    let leftovers = crate::cancel::remove_staged(out, &staged_extras(plan));
    cancel_report(stage, &leftovers)
}

/// Cancellation error for `stage`, naming every leftover path that
/// could not be removed (empty when cleanup succeeded).
fn cancel_report(stage: &str, leftovers: &[PathBuf]) -> String {
    if leftovers.is_empty() {
        return crate::cancel::error(stage);
    }
    let names: Vec<String> = leftovers.iter().map(|p| p.display().to_string()).collect();
    format!(
        "{} render {stage} interrupted by Ctrl-C (SIGINT); cleanup incomplete, leftover path(s): {}; no MP4 was written",
        crate::cancel::PREFIX,
        names.join(", ")
    )
}

/// Draw every frame and encode the MP4 (frames + encode stages, Sprint 3
/// T003/T006/T008): frames are drawn in parallel in bounded batches and
/// re-emitted in frame order, the staged mix (when present) joins the
/// encoder inputs, and the finished MP4 is renamed into place.
/// Returns the human success line plus the measured frames and encode
/// durations in milliseconds.
pub fn render_staged(
    prep: &Prepared,
    plan: &AudioPlan,
    opts: &RenderOptions,
    ffmpeg_bin: &Path,
    out: &Path,
) -> Result<(String, u64, u64), String> {
    use std::io::Write;
    use std::time::Instant;
    let comp = prep.composition();
    let frames = prep.frames;

    // Encode to a temp file first; rename only on success.
    let tmp = tmp_output(out);
    if let Some(parent) = tmp.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                remove_work_files(out, plan);
                format!("render refused: cannot create output directory {}: {e}.", parent.display())
            })?;
        }
    }
    if crate::cancel::is_cancelled() {
        let leftovers = crate::cancel::remove_staged(out, &staged_extras(plan));
        return Err(cancel_report("frames/encode", &leftovers));
    }
    let enc = opts.encoder();
    let args = encode_args(comp, &plan.video_segs, plan.mix_pcm.as_deref(), &tmp, &enc);
    let frames_start = Instant::now();
    let mut child = Command::new(ffmpeg_bin)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if crate::cancel::is_cancelled() {
                let leftovers = crate::cancel::remove_staged(out, &staged_extras(plan));
                return cancel_report("frames/encode", &leftovers);
            }
            remove_work_files(out, plan);
            format!("render failed: could not launch ffmpeg (encode stage): {e}")
        })?;
    let stdin = child.stdin.take();
    let mut stdin = stdin.ok_or_else(|| {
        remove_work_files(out, plan);
        "render failed: no ffmpeg stdin (encode stage)".to_string()
    })?;

    // Stream frames. A write error means the encoder went away early;
    // keep going to collect its status and stderr (the stage report).
    // Frames are drawn in parallel in bounded consecutive batches and
    // emitted in frame order, so any worker count draws the same bytes.
    // Batches must be consecutive ranges: the encoder consumes one
    // frame stream, and only in-order emission keeps every output frame
    // at its own time (T004: strided batches once scrambled slow comps
    // at more than one worker).
    let mut write_failed = false;
    let mut start = 0u64;
    let total = frames as u64;
    while start < total {
        let end = (start + WORKER_BATCH as u64).min(total);
        let batch: Vec<u64> = (start..end).collect();
        let batch = batch.as_slice();
        // Ctrl-C stops workers between batches: the scope has joined
        // its threads before this check, so no worker outlives the
        // cancellation.
        if crate::cancel::is_cancelled() {
            drop(stdin);
            return Err(cancel_encode(&mut child, out, plan, "frames"));
        }
        let slots = match draw_batch_parallel(prep, opts.backend, batch) {
            Ok(slots) => slots,
            Err(e) => {
                if crate::cancel::is_cancelled() {
                    drop(stdin);
                    return Err(cancel_encode(&mut child, out, plan, "frames"));
                }
                drop(stdin);
                let _ = child.wait();
                remove_work_files(out, plan);
                return Err(format!("{e}; no MP4 was written"));
            }
        };
        for (_, bytes) in slots {
            if !write_failed {
                if let Err(e) = stdin.write_all(&bytes) {
                    write_failed = true;
                    let _ = e;
                }
            }
        }
        if crate::cancel::is_cancelled() {
            drop(stdin);
            return Err(cancel_encode(&mut child, out, plan, "frames"));
        }
        start = end;
    }
    drop(stdin);
    let frames_ms = frames_start.elapsed().as_millis() as u64;
    let encode_start = Instant::now();
    let output = child.wait_with_output().map_err(|e| {
        if crate::cancel::is_cancelled() {
            let leftovers = crate::cancel::remove_staged(out, &staged_extras(plan));
            return cancel_report("encode", &leftovers);
        }
        remove_work_files(out, plan);
        format!("render failed: could not read ffmpeg status (encode stage): {e}")
    })?;
    // A SIGINT that kills the encoder looks like an encode failure;
    // the flag tells the two apart so cancellation exits 130.
    if crate::cancel::is_cancelled() {
        let _ = std::fs::remove_file(&tmp);
        let leftovers = crate::cancel::remove_staged(out, &staged_extras(plan));
        return Err(cancel_report("encode", &leftovers));
    }
    if !output.status.success() {
        remove_work_files(out, plan);
        let tail = stderr_tail(&output.stderr, 5);
        return Err(format!(
            "render failed: ffmpeg encode stage exited with status {}{}; no MP4 was written",
            output.status,
            if tail.is_empty() { String::new() } else { format!(": {tail}") }
        ));
    }
    // The encoder must have produced output; an empty file is a failure.
    let produced = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if produced == 0 {
        remove_work_files(out, plan);
        return Err("render failed: ffmpeg encode stage wrote no data; no MP4 was written".to_string());
    }
    std::fs::rename(&tmp, out).map_err(|e| {
        remove_work_files(out, plan);
        format!("render failed: cannot move the encoded MP4 into place: {e} (output stage)")
    })?;
    // The staged mix is a work file: only the MP4 plus the run log
    // record remain after a render.
    if let Some(mix) = &plan.mix_pcm {
        let _ = std::fs::remove_file(mix);
    }
    let encode_ms = encode_start.elapsed().as_millis() as u64;
    let text_drawn = prep.layouts.iter().any(|laid| laid.is_some()) || prep.bitmap_fallback;
    Ok((
        format!(
            "rendered {} ({}x{}, {}s){} [backend={}, {}].",
            out.display(),
            comp.width,
            comp.height,
            comp.duration,
            origin_note(
                prep.project_family.is_some(),
                &prep.fonts,
                prep.bitmap_fallback,
                text_drawn
            ),
            opts.backend.name(),
            opts.announce()
        ),
        frames_ms,
        encode_ms,
    ))
}

/// Render one validated composition to `out` using `ffmpeg_bin`.
/// Returns the human success line on success, or a refusal/failure
/// message (the caller decides the exit code). Never leaves a partial
/// MP4 at `out` on failure.
///
/// Frames are computed in bounded consecutive batches (round-robin
/// within each batch, see [`worker_order`]) and emitted to the encoder
/// in frame order, so any worker count draws byte-identical pictures.
pub fn render_composition(
    comp: &Composition,
    folder: &Path,
    ffmpeg_bin: &Path,
    out: &Path,
) -> Result<String, String> {
    // Default path (CPU backend, standard preset, no overrides) used by
    // non-CLI callers: the staged pipeline with measured stages that
    // the CLI would print one by one.
    let opts = RenderOptions::default();
    render_with_options(comp, folder, ffmpeg_bin, out, &opts).map(|(msg, _, _)| msg)
}

/// Render one validated composition with explicit Sprint 3 options.
/// Returns the human success line plus the measured frames and encode
/// stage durations. Staged mix and encode work files are always removed.
pub fn render_with_options(
    comp: &Composition,
    folder: &Path,
    ffmpeg_bin: &Path,
    out: &Path,
    opts: &RenderOptions,
) -> Result<(String, u64, u64), String> {
    let prep = prepare_frames(comp, folder, ffmpeg_bin)?;
    let mix_path = mix_staging_path(out);
    let plan = match build_audio_plan(comp, folder, ffmpeg_bin, &prep, &mix_path) {
        Ok(plan) => plan,
        Err(e) => {
            // The staged mix may exist already: never leave it behind.
            let _ = std::fs::remove_file(&mix_path);
            return Err(e);
        }
    };
    render_staged(&prep, &plan, opts, ffmpeg_bin, out)
}

/// Success-line suffix naming the text origins, never silent: the
/// project font needs no note; otherwise the used system files join in
/// registration order, or the bitmap last resort is announced. A
/// composition drawing no text carries no note, as before.
fn origin_note(project: bool, fonts: &layout::Fonts, bitmap: bool, text_drawn: bool) -> String {
    if project || !text_drawn {
        return String::new();
    }
    let mut names: Vec<String> = Vec::new();
    for face in fonts.machine_faces() {
        let name = face
            .path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| face.path.to_string_lossy().into_owned());
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() && !bitmap {
        return String::new();
    }
    let mut parts = Vec::new();
    if !names.is_empty() {
        parts.push(format!("using system font {}", names.join(", ")));
    }
    if bitmap {
        parts.push("using the built-in default face (no system font found)".to_string());
    }
    format!(" (no `font` declared; {})", parts.join("; "))
}

/// Text face set for one render: the project font when declared,
/// else per-style system faces with a bitmap last resort.
/// System faces are resolved once per style and reused across frames.
/// CPU raster faces for one render, keyed by parley font bytes and
/// face index: shaping resolves families, this cache parses each used
/// face once and reuses it across frames.
pub(crate) fn raster_face<'a>(raster: &'a mut RasterCache, font: &std::sync::Arc<Vec<u8>>, index: u32) -> Result<&'a Face, String> {
    let key = (std::sync::Arc::as_ptr(font) as usize, index);
    if !raster.contains_key(&key) {
        let face = Face::load_bytes(font).map_err(|e| format!("render failed: shaped font cannot rasterize ({e})"))?;
        raster.insert(key, face);
    }
    Ok(&raster[&key])
}

pub struct AudioSeg {
    pub(crate) path: PathBuf,
    pub(crate) trim_start: f64,
    pub(crate) trim_len: f64,
    pub(crate) delay_ms: u64,
}

fn rel_name(folder: &Path, path: &Path) -> String {
    path.strip_prefix(folder)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// Temporary encode target next to the final MP4.
fn tmp_output(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_owned();
    name.push(".framesmith-part");
    PathBuf::from(name)
}

/// Last `n` non-empty stderr lines, joined for the stage report.
fn stderr_tail(stderr: &[u8], n: usize) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join(" | ").chars().take(600).collect()
}

/// Build the ffmpeg encode arguments. Raw RGB24 frames arrive on stdin
/// as input 0; each audible video source adds an input with trim/delay
/// filters, and the staged Framesmith mix (when present) arrives as raw
/// s16le 48 kHz stereo. The `standard` preset with no overrides emits no
/// explicit encoder flags, so its bytes match the Sprint 2 output
/// exactly; every other preset or override sets only its own axis.
fn encode_args(
    comp: &Composition,
    audio: &[AudioSeg],
    mix: Option<&Path>,
    tmp: &Path,
    enc: &ResolvedEncoder,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-s".into(),
        format!("{}x{}", comp.width, comp.height),
        "-r".into(),
        comp.fps.to_string(),
        "-i".into(),
        "-".into(),
    ];
    for seg in audio {
        args.push("-i".into());
        args.push(seg.path.display().to_string());
    }
    let mix_input: Option<usize> = if let Some(mix_path) = mix {
        let idx = 1 + audio.len();
        args.extend([
            "-f".into(),
            "s16le".into(),
            "-ar".into(),
            "48000".into(),
            "-ac".into(),
            "2".into(),
            "-i".into(),
            mix_path.display().to_string(),
        ]);
        Some(idx)
    } else {
        None
    };
    if audio.is_empty() && mix_input.is_none() {
        // T013: no audible input renders with the silent encoder default.
        args.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "anullsrc=r=48000:cl=stereo".into(),
        ]);
    } else {
        // Trim each video source to its clip window, delay to the clip
        // start, pad to the composition length. (Since T010 `audio` is
        // always empty here — video sound mixes in the staged PCM
        // instead of passing through — this arm only serves the
        // encoder wiring.)
        let mut fc = String::new();
        for (i, seg) in audio.iter().enumerate() {
            fc.push_str(&format!(
                "[{inp}:a]atrim=start={ts:.6}:duration={len:.6},asetpts=PTS-STARTPTS,adelay={ms}:all=1[s{i}];",
                ts = seg.trim_start,
                len = seg.trim_len,
                ms = seg.delay_ms,
                inp = i + 1,
                i = i
            ));
        }
        // The staged mix already spans the full composition at 48 kHz
        // stereo; pad and trim only guard the exact container length.
        if let Some(m) = mix_input {
            fc.push_str(&format!(
                "[{m}:a]apad,atrim=duration={:.6}[mix];",
                comp.duration
            ));
        }
        let n_inputs = audio.len() + if mix_input.is_some() { 1 } else { 0 };
        if n_inputs == 1 {
            if mix_input.is_some() {
                fc.push_str(&format!("[mix]apad,atrim=duration={:.6}[outa]", comp.duration));
            } else {
                fc.push_str(&format!("[s0]apad,atrim=duration={:.6}[outa]", comp.duration));
            }
        } else {
            for i in 0..audio.len() {
                fc.push_str(&format!("[s{i}]"));
            }
            if mix_input.is_some() {
                fc.push_str("[mix]");
            }
            fc.push_str(&format!(
                "amix=inputs={n_inputs}:normalize=0:duration=longest,apad,atrim=duration={:.6}[outa]",
                comp.duration
            ));
        }
        args.push("-filter_complex".into());
        args.push(fc);
        args.push("-map".into());
        args.push("0:v".into());
        args.push("-map".into());
        args.push("[outa]".into());
    }
    if audio.is_empty() && mix_input.is_none() {
        args.extend([
            "-map".into(),
            "0:v".into(),
            "-map".into(),
            "1:a".into(),
        ]);
    }
    // Preset and overrides: each flag sets only its own axis.
    let standard_quiet = enc.from_preset == Preset::Standard
        && enc.video_bitrate.is_none()
        && Some(enc.crf) == Some(Preset::Standard.encoder().crf)
        && enc.x264_preset.is_none();
    if !standard_quiet {
        args.push("-crf".into());
        args.push(enc.crf.to_string());
        if let Some(p) = enc.x264_preset {
            args.push("-preset".into());
            args.push(p.into());
        }
    }
    if let Some(b) = &enc.video_bitrate {
        args.push("-b:v".into());
        args.push(b.clone());
    }
    args.extend([
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-ac".into(),
        "2".into(),
        "-ar".into(),
        "48000".into(),
        "-t".into(),
        format!("{}", comp.duration),
        "-movflags".into(),
        "+faststart".into(),
        // Pinned: the temp file carries a non-standard extension, so the
        // container must not be guessed from its name.
        "-f".into(),
        "mp4".into(),
        tmp.display().to_string(),
    ]);
    args
}

/// Probe one video source through the resolved FFmpeg binary:
/// codec, dimensions, duration, and audio presence.
/// Anything unreadable or outside the Sprint 1 container/codec promise
/// refuses with a diagnostic naming the file.
fn probe_video(ffmpeg_bin: &Path, abs: &Path, rel: &str) -> Result<ProbedVideo, String> {
    let out = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-i", &abs.display().to_string()])
        .output()
        .map_err(|e| {
            format!(
                "render refused: cannot probe video source {rel:?} ({e}); no MP4 was written"
            )
        })?;
    // ffmpeg prints input facts to stderr and exits non-zero without an
    // output file; the facts are still usable when present.
    let text = String::from_utf8_lossy(&out.stderr);
    if !text.contains("Input #0") {
        return Err(format!(
            "render refused: video source {rel:?} is unreadable (ffmpeg reports no input stream); no MP4 was written"
        ));
    }
    let duration = parse_duration(&text);
    let mut video: Option<(String, u32, u32)> = None;
    let mut has_audio = false;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("Stream #") {
            continue;
        }
        if let Some(v) = parse_video_stream(line) {
            if video.is_none() {
                video = Some(v);
            }
        }
        if line.contains("Audio:") {
            has_audio = true;
        }
    }
    let (codec, width, height) = video.ok_or_else(|| {
        format!("render refused: video source {rel:?} has no decodable video stream; no MP4 was written")
    })?;
    if codec != "h264" && codec != "hevc" {
        return Err(format!(
            "render refused: video source {rel:?} uses unsupported codec `{codec}` (Sprint 1 supports H.264 and HEVC); no MP4 was written"
        ));
    }
    if width == 0 || height == 0 {
        return Err(format!(
            "render refused: video source {rel:?} reports no picture dimensions; no MP4 was written"
        ));
    }
    Ok(ProbedVideo {
        clip_idx: 0,
        path: abs.to_path_buf(),
        codec,
        width,
        height,
        duration,
        has_audio,
    })
}

/// `Duration: 00:00:03.00` → seconds.
fn parse_duration(text: &str) -> Option<f64> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Duration:") {
            let ts = rest.split(',').next()?.trim();
            let mut parts = ts.split(':');
            let h: f64 = parts.next()?.parse().ok()?;
            let m: f64 = parts.next()?.parse().ok()?;
            let s: f64 = parts.next()?.parse().ok()?;
            if h < 0.0 || m < 0.0 || s < 0.0 {
                return None;
            }
            return Some(h * 3600.0 + m * 60.0 + s);
        }
    }
    None
}

/// `Stream #0:0...: Video: h264 ..., 640x480 ...` → (codec, w, h).
fn parse_video_stream(line: &str) -> Option<(String, u32, u32)> {
    let after = line.split("Video:").nth(1)?.trim();
    let codec = after.split(|c: char| c == ' ' || c == '(' || c == ',').next()?;
    if codec.is_empty() {
        return None;
    }
    // Dimensions are the first `WxH` token after the codec name.
    for tok in after.split(|c: char| c == ' ' || c == ',' || c == '(' || c == ')') {
        if let Some((w, h)) = tok.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                if w > 0 && h > 0 {
                    return Some((codec.to_string(), w, h));
                }
            }
        }
    }
    None
}

/// Decode a whole stream to null: catches corrupt files and undecodable
/// audio before the first frame. Names the file on failure.
fn smoke_decode(ffmpeg_bin: &Path, pv: &ProbedVideo, audio: bool) -> Result<(), String> {
    let stream = if audio { "0:a:0" } else { "0:v:0" };
    let kind = if audio { "audio" } else { "video" };
    let out = Command::new(ffmpeg_bin)
        .args([
            // `-xerror`: a corrupt packet fails the run instead of being
            // concealed; the render then refuses naming the file.
            "-xerror",
            "-v",
            "error",
            "-i",
            &pv.path.display().to_string(),
            "-map",
            stream,
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| {
            format!(
                "render refused: cannot decode {} of {:?} ({e}); no MP4 was written",
                kind,
                pv.path.display()
            )
        })?;
    if !out.status.success() {
        let tail = stderr_tail(&out.stderr, 3);
        return Err(format!(
            "render refused: {} stream of {:?} is undecodable{}; the system never renders a silent or still replacement; no MP4 was written",
            kind,
            pv.path.display(),
            if tail.is_empty() { String::new() } else { format!(" ({tail})") }
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Frame drawing (CPU).
// ---------------------------------------------------------------------------

/// One decoded image source (T005): straight RGBA rows decoded once
/// through FFmpeg, shared by every clip pointing at the same path.
#[derive(Debug, Clone)]
pub(crate) struct DecodedImage {
    pub(crate) path: PathBuf,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
    /// Memoized resampled dest-rect bitmaps (T004): each entry is a pure
    /// function of these source pixels and its sampling geometry, so
    /// frames and workers share entries and every hit is byte-identical
    /// to recomputing. The cache lives on the prepared image, dying with
    /// the render, so a changed file can never read stale.
    pub(crate) scaled: std::sync::Arc<std::sync::Mutex<HashMap<ImageScaleKey, std::sync::Arc<CachedScale>>>>,
}

/// Sampling geometry behind one cached resampled dest rect (T004): the
/// source dimensions plus every float that feeds the sampling grid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ImageScaleKey {
    pub(crate) sw: u32,
    pub(crate) sh: u32,
    pub(crate) dw: u64,
    pub(crate) dh: u64,
    pub(crate) dx: u64,
    pub(crate) dy: u64,
    pub(crate) i0: u32,
    pub(crate) j0: u32,
    pub(crate) i1: u32,
    pub(crate) j1: u32,
    pub(crate) scale: u64,
    pub(crate) fit: u8,
    pub(crate) place_x: u8,
    pub(crate) place_y: u8,
}

/// One resampled dest rect (T004): per-pixel rounded RGB plus the lerped
/// texel alpha as f64 bits, blended over the canvas with the frame's own
/// opacity exactly as the direct sampler would. `opaque` is set when
/// every lerped alpha is exactly 1.0, unlocking a row-copy blend.
#[derive(Debug, Clone)]
pub(crate) struct CachedScale {
    pub(crate) w: u32,
    pub(crate) rgb: Vec<u8>,
    pub(crate) alpha: Vec<u64>,
    pub(crate) opaque: bool,
}

/// Decode one image source to straight RGBA (T005). Dimensions come
/// from the container's own stream facts; the byte count must match
/// them exactly. Any failure names the file and writes no MP4.
fn decode_image(ffmpeg_bin: &Path, folder: &Path, rel: &str) -> Result<DecodedImage, String> {
    let abs = folder.join(rel);
    // Probe: the container's own stream facts (codec, dimensions).
    let probe = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-i", &abs.display().to_string()])
        .output()
        .map_err(|e| {
            format!("render refused: cannot probe image source {rel:?} ({e}); no MP4 was written")
        })?;
    let text = String::from_utf8_lossy(&probe.stderr);
    let mut dims: Option<(u32, u32)> = None;
    for line in text.lines() {
        if line.trim().starts_with("Stream #") && line.contains("Video:") {
            if let Some(v) = parse_video_stream(line.trim()) {
                dims = Some((v.1, v.2));
                break;
            }
        }
    }
    let (width, height) = dims.ok_or_else(|| {
        format!("render refused: image source {rel:?} reports no decodable picture stream; no MP4 was written")
    })?;
    if width == 0 || height == 0 {
        return Err(format!(
            "render refused: image source {rel:?} reports no picture dimensions; no MP4 was written"
        ));
    }
    let out = Command::new(ffmpeg_bin)
        .args([
            "-xerror",
            "-v",
            "error",
            "-i",
            &abs.display().to_string(),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .output()
        .map_err(|e| {
            format!("render refused: cannot decode image source {rel:?} ({e}); no MP4 was written")
        })?;
    if !out.status.success() {
        let tail = stderr_tail(&out.stderr, 3);
        return Err(format!(
            "render refused: image source {rel:?} cannot be decoded{}; the system never presents a partial MP4 as success; no MP4 was written",
            if tail.is_empty() {
                " (ffmpeg decode stage)".to_string()
            } else {
                format!(" (ffmpeg decode stage: {tail})")
            }
        ));
    }
    let want = width as usize * height as usize * 4;
    if out.stdout.len() != want {
        return Err(format!(
            "render refused: image source {rel:?} decoded to a partial picture ({} bytes, want {want}); no MP4 was written",
            out.stdout.len()
        ));
    }
    Ok(DecodedImage {
        path: abs,
        width,
        height,
        rgba: out.stdout,
        scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
    })
}

/// Is `(px, py)` inside the rounded rect `[x0,x1]x[y0,y1]` with corner
/// radius `r` (T004)? The middle bands accept directly; the corners
/// measure against their centers. A zero radius is a plain rectangle.
fn rr_contains(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> bool {
    if r <= 0.0 {
        return px >= x0 && px <= x1 && py >= y0 && py <= y1;
    }
    if px >= x0 + r && px <= x1 - r && py >= y0 && py <= y1 {
        return true;
    }
    if px >= x0 && px <= x1 && py >= y0 + r && py <= y1 - r {
        return true;
    }
    let cx = if px < x0 + r { x0 + r } else { x1 - r };
    let cy = if py < y0 + r { y0 + r } else { y1 - r };
    (px - cx) * (px - cx) + (py - cy) * (py - cy) <= r * r
}

/// Coverage of the rounded rect over one canvas pixel, by 4x4
/// supersampling (T004): edges and corners feather over one pixel.
fn rr_coverage(i: i64, j: i64, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> f64 {
    rr_coverage_mapped(i, j, &|px, py| (px, py), x0, y0, x1, y1, r)
}

/// Coverage of the rounded rect over one canvas pixel, with each
/// sub-sample mapped through `map` first (T010 rotation: the
/// inverse-rotated sample is tested against the unrotated rect).
fn rr_coverage_mapped(
    i: i64,
    j: i64,
    map: &impl Fn(f64, f64) -> (f64, f64),
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    r: f64,
) -> f64 {
    if x1 <= x0 || y1 <= y0 {
        return 0.0;
    }
    let mut hits = 0u32;
    for sy in 0..4 {
        for sx in 0..4 {
            let px = i as f64 + (sx as f64 + 0.5) / 4.0;
            let py = j as f64 + (sy as f64 + 0.5) / 4.0;
            let (qx, qy) = map(px, py);
            if rr_contains(qx, qy, x0, y0, x1, y1, r) {
                hits += 1;
            }
        }
    }
    hits as f64 / 16.0
}

/// Composite one RGB color over the canvas pixel at `(i, j)` with the
/// given source alpha (0 transparent to 1 opaque), source-over.
#[allow(clippy::too_many_arguments)]
fn blend_over(frame: &mut [u8], cw: u32, chh: u32, i: i64, j: i64, r: u8, g: u8, b: u8, alpha: f64) {
    if alpha <= 0.0 || i < 0 || j < 0 || i >= cw as i64 || j >= chh as i64 {
        return;
    }
    let o = (j as u32 * cw + i as u32) as usize * 3;
    if o + 2 >= frame.len() {
        return;
    }
    if alpha >= 1.0 {
        frame[o] = r;
        frame[o + 1] = g;
        frame[o + 2] = b;
        return;
    }
    let inv = 1.0 - alpha;
    frame[o] = (r as f64 * alpha + frame[o] as f64 * inv + 0.5) as u8;
    frame[o + 1] = (g as f64 * alpha + frame[o + 1] as f64 * inv + 0.5) as u8;
    frame[o + 2] = (b as f64 * alpha + frame[o + 2] as f64 * inv + 0.5) as u8;
}

/// Draw one box (T004/T010): a filled rounded rect with an inner border
/// ring, both under the element opacity. A fully transparent box draws
/// nothing; a fully opaque box hides what is beneath it. The border
/// ring lives inside the box edge; the fill covers the inner rect.
/// Uniform `scale` grows the box (size, radius, border) about its
/// top-left; `rotation` (degrees, positive turns clockwise on screen)
/// turns it about its scaled center. A non-positive scale draws nothing.
pub(crate) fn draw_box(frame: &mut [u8], cw: u32, chh: u32, b: &crate::frame::ResolvedBox) {
    if b.opacity <= 0.0 || b.width <= 0.0 || b.height <= 0.0 {
        return;
    }
    if b.scale <= 0.0 || !b.scale.is_finite() {
        return;
    }
    let alpha = b.opacity.clamp(0.0, 1.0);
    let s = b.scale;
    let sw = b.width * s;
    let sh = b.height * s;
    let (x0, y0) = (b.x, b.y);
    let (x1, y1) = (b.x + sw, b.y + sh);
    let r = (b.radius * s).max(0.0).min(sw / 2.0).min(sh / 2.0);
    let bw = (b.border_width * s).max(0.0);
    let r_in = (r - bw).max(0.0);
    let (ix0, iy0, ix1, iy1) = (x0 + bw, y0 + bw, x1 - bw, y1 - bw);
    let paint = |frame: &mut [u8], i: i64, j: i64, outer: f64, inner: f64| {
        if outer <= 0.0 {
            return;
        }
        if bw > 0.0 {
            let ring = (outer - inner).max(0.0);
            if ring > 0.0 {
                blend_over(
                    frame, cw, chh, i, j,
                    b.border_color.r, b.border_color.g, b.border_color.b,
                    ring * alpha,
                );
            }
            if inner > 0.0 {
                blend_over(
                    frame, cw, chh, i, j,
                    b.fill.r, b.fill.g, b.fill.b,
                    inner * alpha,
                );
            }
        } else {
            blend_over(frame, cw, chh, i, j, b.fill.r, b.fill.g, b.fill.b, outer * alpha);
        }
    };
    if b.rotation == 0.0 || !b.rotation.is_finite() {
        let i0 = (x0.floor() as i64).max(0);
        let i1 = (x1.ceil() as i64).min(cw as i64);
        let j0 = (y0.floor() as i64).max(0);
        let j1 = (y1.ceil() as i64).min(chh as i64);
        // Sharp integer rect (T004 speed): with square corners on integer
        // edges every visited pixel's 16 sub-samples fall strictly inside,
        // so coverage is exactly 1.0 without supersampling. Finite edges
        // only; anything else takes the slow path.
        let sharp = r <= 0.0
            && x0.is_finite()
            && y0.is_finite()
            && x1.is_finite()
            && y1.is_finite()
            && x0.fract() == 0.0
            && y0.fract() == 0.0
            && x1.fract() == 0.0
            && y1.fract() == 0.0;
        for j in j0..j1 {
            for i in i0..i1 {
                let outer = if sharp {
                    1.0
                } else {
                    rr_coverage(i, j, x0, y0, x1, y1, r)
                };
                let inner = if bw > 0.0 {
                    rr_coverage(i, j, ix0, iy0, ix1, iy1, r_in)
                } else {
                    0.0
                };
                paint(frame, i, j, outer, inner);
            }
        }
        return;
    }
    // Rotated path: every sub-sample is inverse-rotated into the
    // unrotated box frame before the rounded-rect test.
    let theta = b.rotation.to_radians();
    let (sin, cos) = theta.sin_cos();
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let hw = sw / 2.0;
    let hh = sh / 2.0;
    let ex = hw * cos.abs() + hh * sin.abs();
    let ey = hw * sin.abs() + hh * cos.abs();
    let i0 = ((cx - ex).floor() as i64).max(0);
    let i1 = ((cx + ex).ceil() as i64).min(cw as i64);
    let j0 = ((cy - ey).floor() as i64).max(0);
    let j1 = ((cy + ey).ceil() as i64).min(chh as i64);
    let map = |px: f64, py: f64| -> (f64, f64) {
        let dx = px - cx;
        let dy = py - cy;
        (cx + dx * cos + dy * sin, cy - dx * sin + dy * cos)
    };
    for j in j0..j1 {
        for i in i0..i1 {
            let outer = rr_coverage_mapped(i, j, &map, x0, y0, x1, y1, r);
            let inner = if bw > 0.0 {
                rr_coverage_mapped(i, j, &map, ix0, iy0, ix1, iy1, r_in)
            } else {
                0.0
            };
            paint(frame, i, j, outer, inner);
        }
    }
}

/// Fitted picture size for an image target rect (T005): `fill`
/// stretches, `contain` letterboxes, `cover` bleeds.
fn image_fit_size(tw: f64, th: f64, sw: f64, sh: f64, fit: Fit) -> (f64, f64) {
    match fit {
        Fit::Fill => (tw, th),
        Fit::Contain => {
            let s = (tw / sw).min(th / sh);
            (sw * s, sh * s)
        }
        Fit::Cover => {
            let s = (tw / sw).max(th / sh);
            (sw * s, sh * s)
        }
    }
}

/// Draw one image (T005/T010): the decoded still placed with the
/// requested fit and placement inside its target rect, clipped to that
/// rect. Bilinear sampling keeps scaled pictures smooth; the source
/// alpha scaled by the element opacity composites over the canvas, so
/// transparent PNG pixels show through. Uniform `scale` grows the
/// target rect about its top-left; `rotation` (degrees, positive turns
/// clockwise on screen) turns the placed picture about its own center.
/// A non-positive scale draws nothing.
pub(crate) fn draw_image(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    im: &crate::frame::ResolvedImage,
    decoded: &DecodedImage,
) {
    use notation::model::{PlaceH, PlaceV};
    let sw = decoded.width as f64;
    let sh = decoded.height as f64;
    if sw <= 0.0 || sh <= 0.0 {
        return;
    }
    if im.scale <= 0.0 || !im.scale.is_finite() {
        return;
    }
    if im.opacity <= 0.0 {
        return;
    }
    let alpha = im.opacity.clamp(0.0, 1.0);
    let tw = im.width.unwrap_or(cw as f64) * im.scale;
    let th = im.height.unwrap_or(chh as f64) * im.scale;
    if tw <= 0.0 || th <= 0.0 {
        return;
    }
    let (dw, dh) = image_fit_size(tw, th, sw, sh, im.fit);
    if dw <= 0.0 || dh <= 0.0 {
        return;
    }
    // Placement inside the target rect, equivalent to
    // `object-position`: start, center, or end along each axis. A
    // `fill` picture exactly covers the target, so placement changes
    // nothing for it.
    let ox = match im.placement.x {
        PlaceH::Left => 0.0,
        PlaceH::Center => (tw - dw) / 2.0,
        PlaceH::Right => tw - dw,
    };
    let oy = match im.placement.y {
        PlaceV::Top => 0.0,
        PlaceV::Center => (th - dh) / 2.0,
        PlaceV::Bottom => th - dh,
    };
    let (dx, dy) = (im.x + ox, im.y + oy);
    // Canvas pixels whose centers fall inside both the picture and its
    // target rect (cover bleed is cropped to the target, never beyond).
    let (tx0, ty0, tx1, ty1) = (im.x, im.y, im.x + tw, im.y + th);
    if im.rotation == 0.0 || !im.rotation.is_finite() {
        let j0 = (dy - 0.5).ceil().max(0.0).max((ty0 - 0.5).ceil().max(0.0)) as i64;
        let j1 = (dy + dh - 0.5).floor().min(chh as f64 - 1.0).min((ty1 - 0.5).floor()).max(-1.0) as i64;
        let i0 = (dx - 0.5).ceil().max(0.0).max((tx0 - 0.5).ceil().max(0.0)) as i64;
        let i1 = (dx + dw - 0.5).floor().min(cw as f64 - 1.0).min((tx1 - 0.5).floor()).max(-1.0) as i64;
        if j1 < j0 || i1 < i0 {
            return;
        }
        // Separable sampling tables (T004 speed): each column/row maps
        // through the old formula once, then pixels only look up. The
        // conversions are exact, so every expression evaluates as before.
        let swm1 = decoded.width - 1;
        let shm1 = decoded.height - 1;
        let mut cols: Vec<(u32, u32, f64)> = Vec::with_capacity((i1 - i0 + 1) as usize);
        for i in i0..=i1 {
            let su = (((i as f64 + 0.5 - dx) * sw / dw) - 0.5).clamp(0.0, sw - 1.0);
            let x0 = su.floor() as u32;
            cols.push((x0, (x0 + 1).min(swm1), su - su.floor()));
        }
        let mut rows: Vec<(u32, u32, f64)> = Vec::with_capacity((j1 - j0 + 1) as usize);
        for j in j0..=j1 {
            let sv = (((j as f64 + 0.5 - dy) * sh / dh) - 0.5).clamp(0.0, sh - 1.0);
            let y0 = sv.floor() as u32;
            rows.push((y0, (y0 + 1).min(shm1), sv - sv.floor()));
        }
        // Resampled dest rect (T004): a pure function of the source
        // pixels and this geometry, memoized on the prepared image so
        // static pictures resample once per geometry, not once per frame.
        let key = ImageScaleKey {
            sw: decoded.width,
            sh: decoded.height,
            dw: dw.to_bits(),
            dh: dh.to_bits(),
            dx: dx.to_bits(),
            dy: dy.to_bits(),
            i0: i0 as u32,
            j0: j0 as u32,
            i1: i1 as u32,
            j1: j1 as u32,
            scale: im.scale.to_bits(),
            fit: im.fit as u8,
            place_x: im.placement.x as u8,
            place_y: im.placement.y as u8,
        };
        let hit = decoded
            .scaled
            .lock()
            .ok()
            .and_then(|m| m.get(&key).cloned());
        let entry: std::sync::Arc<CachedScale> = match hit {
            Some(e) => e,
            None => {
                let w = (i1 - i0 + 1) as usize;
                let h = (j1 - j0 + 1) as usize;
                let mut rgb = vec![0u8; w * h * 3];
                let mut calpha = vec![0u64; w * h];
                let mut all_opaque = true;
                for (rj, &(y0, y1, fy)) in rows.iter().enumerate() {
                    let r0 = ((y0 * decoded.width) as usize) * 4;
                    let r1 = ((y1 * decoded.width) as usize) * 4;
                    for (ci, &(x0, x1, fx)) in cols.iter().enumerate() {
                        let s00 = r0 + (x0 as usize) * 4;
                        let s10 = r0 + (x1 as usize) * 4;
                        let s01 = r1 + (x0 as usize) * 4;
                        let s11 = r1 + (x1 as usize) * 4;
                        let k = rj * w + ci;
                        for c in 0..4 {
                            // The old `texel`: RGB raw, alpha divided before
                            // the lerp — the same values, in the same order.
                            let f = |s: usize| -> f64 {
                                if c < 3 {
                                    decoded.rgba[s + c] as f64
                                } else {
                                    decoded.rgba[s + c] as f64 / 255.0
                                }
                            };
                            let top = f(s00) * (1.0 - fx) + f(s10) * fx;
                            let bot = f(s01) * (1.0 - fx) + f(s11) * fx;
                            let v = top * (1.0 - fy) + bot * fy;
                            if c < 3 {
                                rgb[k * 3 + c] = v.round() as u8;
                            } else {
                                if v != 1.0 {
                                    all_opaque = false;
                                }
                                calpha[k] = v.to_bits();
                            }
                        }
                    }
                }
                let e = std::sync::Arc::new(CachedScale {
                    w: w as u32,
                    rgb,
                    alpha: calpha,
                    opaque: all_opaque,
                });
                if let Ok(mut m) = decoded.scaled.lock() {
                    if m.len() >= 8 {
                        m.clear();
                    }
                    m.insert(key, e.clone());
                }
                e
            }
        };
        // Blend the rect with this frame's own opacity — exactly the old
        // `paint` tail (`blend_over` over the rounded RGB and the lerped
        // alpha scaled by the frame opacity), inlined: every visited
        // pixel is inside the canvas by construction, so the clipping
        // checks would never fire. A fully opaque rect at full frame
        // opacity copies row by row.
        let w = entry.w as usize;
        if alpha >= 1.0 && entry.opaque {
            let n = w * 3;
            for r in 0..entry.alpha.len() / w {
                let o = (((j0 + r as i64) as u32 * cw + i0 as u32) as usize) * 3;
                let s = r * n;
                frame[o..o + n].copy_from_slice(&entry.rgb[s..s + n]);
            }
            return;
        }
        for (k, &bits) in entry.alpha.iter().enumerate() {
            let a_tex = f64::from_bits(bits);
            let ae = (a_tex * alpha).clamp(0.0, 1.0);
            if ae <= 0.0 {
                continue;
            }
            let o = (((j0 + (k / w) as i64) as u32 * cw + (i0 + (k % w) as i64) as u32) as usize) * 3;
            if ae >= 1.0 {
                frame[o] = entry.rgb[k * 3];
                frame[o + 1] = entry.rgb[k * 3 + 1];
                frame[o + 2] = entry.rgb[k * 3 + 2];
                continue;
            }
            let inv = 1.0 - ae;
            frame[o] = (entry.rgb[k * 3] as f64 * ae + frame[o] as f64 * inv + 0.5) as u8;
            frame[o + 1] = (entry.rgb[k * 3 + 1] as f64 * ae + frame[o + 1] as f64 * inv + 0.5) as u8;
            frame[o + 2] = (entry.rgb[k * 3 + 2] as f64 * ae + frame[o + 2] as f64 * inv + 0.5) as u8;
        }
        return;
    }
    let sample_at = |su: f64, sv: f64| -> [f64; 4] {
        let su = su.clamp(0.0, sw - 1.0);
        let sv = sv.clamp(0.0, sh - 1.0);
        let x0 = su.floor() as u32;
        let x1 = (x0 + 1).min(decoded.width - 1);
        let fx = su - su.floor();
        let y0 = sv.floor() as u32;
        let y1 = (y0 + 1).min(decoded.height - 1);
        let fy = sv - sv.floor();
        let texel = |xx: u32, yy: u32| -> [f64; 4] {
            let s = ((yy * decoded.width + xx) as usize) * 4;
            [
                decoded.rgba[s] as f64,
                decoded.rgba[s + 1] as f64,
                decoded.rgba[s + 2] as f64,
                decoded.rgba[s + 3] as f64 / 255.0,
            ]
        };
        let p00 = texel(x0, y0);
        let p10 = texel(x1, y0);
        let p01 = texel(x0, y1);
        let p11 = texel(x1, y1);
        let mut rgba = [0.0f64; 4];
        for c in 0..4 {
            let top = p00[c] * (1.0 - fx) + p10[c] * fx;
            let bot = p01[c] * (1.0 - fx) + p11[c] * fx;
            rgba[c] = top * (1.0 - fy) + bot * fy;
        }
        rgba
    };
    let paint = |frame: &mut [u8], i: i64, j: i64, su: f64, sv: f64| {
        let rgba = sample_at(su, sv);
        blend_over(
            frame, cw, chh, i, j,
            rgba[0].round() as u8,
            rgba[1].round() as u8,
            rgba[2].round() as u8,
            (rgba[3] * alpha).clamp(0.0, 1.0),
        );
    };
    // Rotated path: the unrotated picture is sampled through the
    // inverse rotation about the target center; the target-rect clip
    // lifts so turned corners keep their picture.
    let theta = im.rotation.to_radians();
    let (sin, cos) = theta.sin_cos();
    let hw = dw / 2.0;
    let hh = dh / 2.0;
    let ex = hw * cos.abs() + hh * sin.abs();
    let ey = hw * sin.abs() + hh * cos.abs();
    let (pcx, pcy) = (dx + dw / 2.0, dy + dh / 2.0);
    let j0 = (pcy - ey - 0.5).ceil().max(0.0) as i64;
    let j1 = (pcy + ey - 0.5).floor().min(chh as f64 - 1.0) as i64;
    let i0 = (pcx - ex - 0.5).ceil().max(0.0) as i64;
    let i1 = (pcx + ex - 0.5).floor().min(cw as f64 - 1.0) as i64;
    if j1 < j0 || i1 < i0 {
        return;
    }
    for j in j0..=j1 {
        if j < 0 || j >= chh as i64 {
            continue;
        }
        for i in i0..=i1 {
            if i < 0 || i >= cw as i64 {
                continue;
            }
            // Into the unrotated picture frame, then into source texels.
            let qx = i as f64 + 0.5 - pcx;
            let qy = j as f64 + 0.5 - pcy;
            let ux = qx * cos + qy * sin + dw / 2.0;
            let uy = -qx * sin + qy * cos + dh / 2.0;
            if ux < 0.0 || uy < 0.0 || ux > dw || uy > dh {
                continue;
            }
            let su = (ux * sw / dw) - 0.5;
            let sv = (uy * sh / dh) - 0.5;
            paint(frame, i, j, su, sv);
        }
    }
}

/// Fill an RGB24 canvas with one color (T004): the first triplet is
/// written once, then doubling `copy_within` spreads it. Byte-identical
/// to the per-triplet loop (proven by
/// [`tests::canvas_fill_matches_the_per_pixel_loop`]) at memset speed.
pub(crate) fn fill_canvas(frame: &mut [u8], r: u8, g: u8, b: u8) {
    if frame.len() < 3 {
        return;
    }
    frame[0] = r;
    frame[1] = g;
    frame[2] = b;
    let mut n = 3usize;
    while n < frame.len() {
        let m = (2 * n).min(frame.len());
        frame.copy_within(..(m - n), n);
        n = m;
    }
}

/// Draw one frame description into `frame`. The description is the only
/// composition input: visibility, paint order, and evaluated motion are
/// already resolved in it, so the same frame number always draws the
/// same picture. `layouts[doc]` holds the parley block of text clip
/// `doc` (or nothing for the bitmap last resort); `ranges[i]` is the
/// decoded window for `videos[i]`; `raster` caches parsed faces.
/// `images` holds each decoded still (T005).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_description(
    desc: &FrameDescription,
    raster: &mut RasterCache,
    layouts: &[Option<layout::Block>],
    bitmap_fallback: bool,
    videos: &[ProbedVideo],
    ranges: &[DecodedRange],
    images: &[DecodedImage],
    frame: &mut [u8],
) -> Result<(), String> {
    // Background fill.
    fill_canvas(frame, desc.background.r, desc.background.g, desc.background.b);
    let cw = desc.width;
    let chh = desc.height;
    for item in &desc.items {
        match item {
            FrameItem::Video(v) => {
                let vi = videos
                    .iter()
                    .position(|p| p.clip_idx == v.doc)
                    .ok_or_else(|| {
                        format!("render failed: video clip #{} was not probed (decode stage)", v.doc)
                    })?;
                let range = &ranges[vi];
                let j = pick_picture(range, v.src_t).ok_or_else(|| {
                    format!(
                        "render refused: video source {:?} yields no picture at {:.3}s (past its timestamps); the system never holds a stale frame; no MP4 was written",
                        videos[vi].path.display(),
                        v.src_t
                    )
                })?;
                let fs = range.width as usize * range.height as usize * 3;
                let pic = &range.frames[j * fs..(j + 1) * fs];
                blit_fit(
                    frame, cw, chh, pic, range.width, range.height, v.fit,
                    v.x, v.y, v.scale, v.rotation, v.opacity,
                );
            }
            FrameItem::Text(tc) => {
                match layouts.get(tc.doc).and_then(|laid| laid.as_ref()) {
                    Some(block) => draw_text_block(frame, cw, chh, raster, block, tc)?,
                    None if bitmap_fallback => {
                        draw_bitmap_text_item(frame, cw, chh, tc);
                    }
                    None => {
                        return Err("render failed: text layout is missing and no bitmap fallback applies".to_string());
                    }
                }
            }
            FrameItem::Box(b) => {
                draw_box(frame, cw, chh, b);
            }
            FrameItem::Image(im) => {
                let decoded = images
                    .iter()
                    .find(|d| d.path.ends_with(&im.path))
                    .ok_or_else(|| {
                        format!("render failed: image source {:?} was not decoded (decode stage)", im.path)
                    })?;
                draw_image(frame, cw, chh, im, decoded);
            }
        }
    }
    Ok(())
}

/// Visible clip window clamped to the composition, as source seconds.
/// Validation guarantees `start >= 0`, so the source always plays from 0.
fn clip_window_len(comp: &Composition, v: &notation::model::VideoClip) -> f64 {
    (v.start + v.duration).min(comp.duration) - v.start
}

/// Decode one clip window `[0, win_len)` to raw pictures with their
/// presentation timestamps (`showinfo`). One sequential pass per clip:
/// output frames then select pictures by exact timestamp, so differing
/// source fps never drifts and no per-frame process is needed.
fn decode_range(
    ffmpeg_bin: &Path,
    pv: &ProbedVideo,
    win_len: f64,
) -> Result<DecodedRange, String> {
    let out = Command::new(ffmpeg_bin)
        .args([
            "-xerror",
            "-v",
            "info",
            "-i",
            &pv.path.display().to_string(),
            "-t",
            &format!("{:.6}", win_len.max(0.0)),
            "-vf",
            "showinfo",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .map_err(|e| {
            format!(
                "render failed: cannot decode video source {:?} ({e})",
                pv.path.display()
            )
        })?;
    if !out.status.success() {
        let tail = stderr_tail(&out.stderr, 3);
        return Err(format!(
            "render failed: cannot decode video source {:?} (decode stage){}; no MP4 is presented as success",
            pv.path.display(),
            if tail.is_empty() { String::new() } else { format!(": {tail}") }
        ));
    }
    let framesize = pv.width as usize * pv.height as usize * 3;
    if framesize == 0 || out.stdout.len() % framesize != 0 {
        return Err(format!(
            "render refused: video source {:?} decoded to a partial picture ({} bytes); no MP4 was written",
            pv.path.display(),
            out.stdout.len()
        ));
    }
    let text = String::from_utf8_lossy(&out.stderr);
    let mut pts: Vec<f64> = Vec::new();
    for line in text.lines() {
        if let Some(idx) = line.find("pts_time:") {
            let rest = line[idx + "pts_time:".len()..].trim();
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect();
            if let Ok(t) = num.parse::<f64>() {
                if t.is_finite() {
                    pts.push(t);
                }
            }
        }
    }
    let count = out.stdout.len() / framesize;
    // The `-t` window cut can log one extra `showinfo` line for the
    // boundary frame it drops (a clip window into a longer source):
    // that frame has no picture, so it carries no mapping. Anything
    // fewer than the picture count is still a refusal, never a guess.
    if pts.len() > count {
        pts.truncate(count);
    }
    if count == 0 || pts.len() != count {
        return Err(format!(
            "render refused: video source {:?} yields no mappable timestamps ({} pictures, {} timestamps); no MP4 was written",
            pv.path.display(),
            count,
            pts.len()
        ));
    }
    Ok(DecodedRange {
        video_idx: 0,
        width: pv.width,
        height: pv.height,
        frames: out.stdout,
        pts,
    })
}

/// Picture index covering `src_t`: the last picture at or before it.
/// Before the first timestamp the first picture applies (sub-frame
/// container offsets); past the last picture plus one typical gap the
/// source is exhausted and the render refuses instead of holding.
pub(crate) fn pick_picture(range: &DecodedRange, src_t: f64) -> Option<usize> {
    let eps = 1e-6;
    let mut idx: Option<usize> = None;
    for (i, &t) in range.pts.iter().enumerate() {
        if t <= src_t + eps {
            idx = Some(i);
        } else {
            break;
        }
    }
    match idx {
        Some(i) => {
            // Past-the-end detection: the source ended mid-window.
            if i + 1 == range.pts.len() {
                let gap = typical_gap(&range.pts);
                if src_t > range.pts[i] + gap * 1.5 + eps {
                    return None;
                }
            }
            Some(i)
        }
        None => Some(0),
    }
}

/// Median gap between consecutive timestamps (fallback: whole span).
fn typical_gap(pts: &[f64]) -> f64 {
    if pts.len() < 2 {
        return f64::INFINITY;
    }
    let mut gaps: Vec<f64> = pts.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect();
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    gaps[gaps.len() / 2]
}

/// Where the source picture lands on the canvas for `fit`.
/// Returns (dx, dy, dw, dh): destination rect in canvas pixels.
fn fit_rect(
    cw: u32,
    chh: u32,
    sw: u32,
    sh: u32,
    fit: Fit,
) -> (f64, f64, f64, f64) {
    let (cw, chh, sw, sh) = (cw as f64, chh as f64, sw as f64, sh as f64);
    match fit {
        Fit::Fill => (0.0, 0.0, cw, chh),
        Fit::Contain => {
            let s = (cw / sw).min(chh / sh);
            let dw = sw * s;
            let dh = sh * s;
            ((cw - dw) / 2.0, (chh - dh) / 2.0, dw, dh)
        }
        Fit::Cover => {
            let s = (cw / sw).max(chh / sh);
            let dw = sw * s;
            let dh = sh * s;
            ((cw - dw) / 2.0, (chh - dh) / 2.0, dw, dh)
        }
    }
}

/// Scale `src` (RGB24, `sw`x`sh`) into the fit rect with bilinear
/// sampling, compositing over the canvas (T010 motion: position offset,
/// uniform scale and rotation in degrees about the dest-rect center,
/// opacity 0 transparent to 1 opaque). The identity transform paints
/// opaquely exactly as before; a non-positive scale paints nothing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blit_fit(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    src: &[u8],
    sw: u32,
    sh: u32,
    fit: Fit,
    ox: f64,
    oy: f64,
    scale: f64,
    rotation: f64,
    opacity: f64,
) {
    if opacity <= 0.0 {
        return;
    }
    if scale <= 0.0 || !scale.is_finite() {
        return;
    }
    let alpha = opacity.clamp(0.0, 1.0);
    let (fx0, fy0, fw, fh) = fit_rect(cw, chh, sw, sh, fit);
    // Scale the dest rect about its own center, then shift it.
    let (ccx, ccy) = (fx0 + fw / 2.0, fy0 + fh / 2.0);
    let (dw, dh) = (fw * scale, fh * scale);
    if dw < 1.0 || dh < 1.0 {
        return;
    }
    let (dx, dy) = (ccx - dw / 2.0 + ox, ccy - dh / 2.0 + oy);
    // Hoisted sampling facts (T004 speed): the conversions are exact, so
    // every per-pixel expression below evaluates exactly as before.
    let swf = sw as f64;
    let shf = sh as f64;
    let swm1 = sw - 1;
    let shm1 = sh - 1;
    let opaque = alpha >= 1.0;
    let inva = 1.0 - alpha;
    // Blend one resampled texel over the canvas (the old `paint` tail).
    // Every float expression is the old one, in the old order.
    let blend = |frame: &mut [u8], o: usize, p00: usize, p10: usize, p01: usize, p11: usize, fx: f64, fy: f64| {
        if p11 + 2 >= src.len() {
            return;
        }
        if o + 2 >= frame.len() {
            return;
        }
        for c in 0..3 {
            let top = src[p00 + c] as f64 * (1.0 - fx) + src[p10 + c] as f64 * fx;
            let bot = src[p01 + c] as f64 * (1.0 - fx) + src[p11 + c] as f64 * fx;
            let v = (top * (1.0 - fy) + bot * fy + 0.5) as u8;
            if opaque {
                frame[o + c] = v;
            } else {
                frame[o + c] =
                    (v as f64 * alpha + frame[o + c] as f64 * inva + 0.5) as u8;
            }
        }
    };
    if rotation == 0.0 || !rotation.is_finite() {
        // Canvas rows/columns whose centers fall inside the dest rect; the
        // source sample is edge-clamped so the rect is fully covered.
        let j0 = ((dy - 0.5).ceil().max(0.0)) as u32;
        let j1 = ((dy + dh - 0.5).floor().min(chh as f64 - 1.0)) as u32;
        let i0 = ((dx - 0.5).ceil().max(0.0)) as u32;
        let i1 = ((dx + dw - 0.5).floor().min(cw as f64 - 1.0)) as u32;
        if j1 < j0 || i1 < i0 {
            return;
        }
        // Identity sampling (T004 speed): when the dest rect matches the
        // source texel-for-texel on integer offsets, every sample lands
        // exactly on its texel (`su`/`sv` stay integral, both weights 0),
        // so the whole bilinear reduces to a copy — or a copy blended
        // with the frame opacity. Same bytes, no resampling.
        if swf == dw
            && shf == dh
            && dx.is_finite()
            && dy.is_finite()
            && dx.fract() == 0.0
            && dy.fract() == 0.0
        {
            let (ox, oy) = (dx as i64, dy as i64);
            // The loop bounds already prove every index below inside its
            // buffer (integer offsets stay in range exactly as the
            // clamp/floor analysis shows), so rows copy with one check.
            let sww = sw as usize;
            let oxu = ((i0 as i64 - ox) as usize) * 3;
            let orow0 = (i0 as usize) * 3;
            let n = (i1 - i0 + 1) as usize * 3;
            for j in j0..=j1 {
                let srow = (((j as i64 - oy) as usize) * sww) * 3 + oxu;
                let orow = (j * cw) as usize * 3 + orow0;
                if opaque {
                    frame[orow..orow + n].copy_from_slice(&src[srow..srow + n]);
                } else {
                    for k in 0..n {
                        frame[orow + k] =
                            (src[srow + k] as f64 * alpha + frame[orow + k] as f64 * inva
                                + 0.5) as u8;
                    }
                }
            }
            return;
        }
        // Separable sampling tables (T004 speed): each column/row maps
        // through the old formula once, then pixels only look up.
        let mut cols: Vec<(u32, u32, f64)> = Vec::with_capacity((i1 - i0 + 1) as usize);
        for i in i0..=i1 {
            let su = (((i as f64 + 0.5 - dx) * swf / dw) - 0.5).clamp(0.0, swf - 1.0);
            let x0 = su.floor() as u32;
            cols.push((x0, (x0 + 1).min(swm1), su - su.floor()));
        }
        let mut rows: Vec<(u32, u32, f64)> = Vec::with_capacity((j1 - j0 + 1) as usize);
        for j in j0..=j1 {
            let sv = (((j as f64 + 0.5 - dy) * shf / dh) - 0.5).clamp(0.0, shf - 1.0);
            let y0 = sv.floor() as u32;
            rows.push((y0, (y0 + 1).min(shm1), sv - sv.floor()));
        }
        for (rj, &(y0, y1, fy)) in rows.iter().enumerate() {
            let j = j0 + rj as u32;
            let row_f = (j * cw) as usize * 3;
            let row0 = (y0 * sw) as usize * 3;
            let row1 = (y1 * sw) as usize * 3;
            for (ci, &(x0, x1, fx)) in cols.iter().enumerate() {
                let i = i0 + ci as u32;
                blend(
                    frame,
                    row_f + (i as usize) * 3,
                    row0 + (x0 as usize) * 3,
                    row0 + (x1 as usize) * 3,
                    row1 + (x0 as usize) * 3,
                    row1 + (x1 as usize) * 3,
                    fx,
                    fy,
                );
            }
        }
        return;
    }
    // Rotated path: the dest rect turns about its own center; pixels
    // outside it stay untouched.
    let theta = rotation.to_radians();
    let (sin, cos) = theta.sin_cos();
    let (pcx, pcy) = (dx + dw / 2.0, dy + dh / 2.0);
    let ex = (dw / 2.0) * cos.abs() + (dh / 2.0) * sin.abs();
    let ey = (dw / 2.0) * sin.abs() + (dh / 2.0) * cos.abs();
    let j0 = ((pcy - ey - 0.5).ceil().max(0.0)) as u32;
    let j1 = ((pcy + ey - 0.5).floor().min(chh as f64 - 1.0)) as u32;
    let i0 = ((pcx - ex - 0.5).ceil().max(0.0)) as u32;
    let i1 = ((pcx + ex - 0.5).floor().min(cw as f64 - 1.0)) as u32;
    if j1 < j0 || i1 < i0 {
        return;
    }
    for j in j0..=j1 {
        for i in i0..=i1 {
            let qx = i as f64 + 0.5 - pcx;
            let qy = j as f64 + 0.5 - pcy;
            let ux = qx * cos + qy * sin + dw / 2.0;
            let uy = -qx * sin + qy * cos + dh / 2.0;
            if ux < 0.0 || uy < 0.0 || ux > dw || uy > dh {
                continue;
            }
            let su = ((ux * swf / dw) - 0.5).clamp(0.0, swf - 1.0);
            let x0 = su.floor() as u32;
            let x1 = (x0 + 1).min(swm1);
            let fx = su - su.floor();
            let sv = ((uy * shf / dh) - 0.5).clamp(0.0, shf - 1.0);
            let y0 = sv.floor() as u32;
            let y1 = (y0 + 1).min(shm1);
            let fy = sv - sv.floor();
            blend(
                frame,
                ((j * cw + i) as usize) * 3,
                ((y0 * sw + x0) as usize) * 3,
                ((y0 * sw + x1) as usize) * 3,
                ((y1 * sw + x0) as usize) * 3,
                ((y1 * sw + x1) as usize) * 3,
                fx,
                fy,
            );
        }
    }
}

/// Draw one shaped text block scaled by `s` about `(x, y)` (block
/// top-left, motion offsets applied): parley outline runs through the
/// cached CPU faces, emoji pictures composited with source-over. Glyph
/// pens and sizes scale, so the ink grows exactly by the factor.
/// Parley already chose the faces and the synthesis; the effective
/// style replays its verdict (embolden → faux bold, skew → faux
/// oblique) through the same blitter the old path used, so weight and
/// style keep their ink.
fn draw_block_scaled(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    block: &layout::Block,
    x: f64,
    y: f64,
    s: f64,
) -> Result<(), String> {
    use notation::model::FontStyle as S;
    for run in &block.runs {
        let face = raster_face(raster, &run.font, run.index)?;
        let eff = match (run.embolden, run.skew_degrees != 0.0) {
            (false, false) => S::Regular,
            (true, false) => S::Bold,
            (false, true) => S::Italic,
            (true, true) => S::BoldItalic,
        };
        let color = (run.colour[0], run.colour[1], run.colour[2]);
        for g in &run.glyphs {
            face.blit_gid(frame, cw, chh, g.id, g.ch, run.size as f64 * s, x + g.x as f64 * s, y + g.y as f64 * s, color, eff);
        }
    }
    for pic in &block.pictures {
        blend_rgba(frame, cw, chh, &pic.rgba, pic.width, pic.height, (x + pic.x as f64 * s).round() as i64, (y + pic.y as f64 * s).round() as i64);
    }
    Ok(())
}

/// Draw one shaped text block into an RGBA `layer` instead of the
/// frame, for the rotation/fade path: same pens and faces as
/// [`draw_block_scaled`], carrying `global_alpha` in the layer alpha.
fn draw_block_layer(
    layer: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    block: &layout::Block,
    x: f64,
    y: f64,
    s: f64,
    global_alpha: f64,
) -> Result<(), String> {
    use notation::model::FontStyle as S;
    for run in &block.runs {
        let face = raster_face(raster, &run.font, run.index)?;
        let eff = match (run.embolden, run.skew_degrees != 0.0) {
            (false, false) => S::Regular,
            (true, false) => S::Bold,
            (false, true) => S::Italic,
            (true, true) => S::BoldItalic,
        };
        let color = (run.colour[0], run.colour[1], run.colour[2]);
        for g in &run.glyphs {
            face.blit_gid_layer(layer, cw, chh, g.id, g.ch, run.size as f64 * s, x + g.x as f64 * s, y + g.y as f64 * s, color, eff, global_alpha);
        }
    }
    for pic in &block.pictures {
        blend_rgba_layer(layer, cw, chh, &pic.rgba, pic.width, pic.height, (x + pic.x as f64 * s).round() as i64, (y + pic.y as f64 * s).round() as i64, global_alpha);
    }
    Ok(())
}

/// Draw one shaped text block faded straight onto the canvas (T004):
/// like [`draw_block_scaled`] but every glyph carries `global_alpha`,
/// so an unrotated fade without emoji pictures needs no scratch layer.
/// Edge texels may differ from the layer route by a rounding step.
fn draw_block_scaled_faded(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    block: &layout::Block,
    x: f64,
    y: f64,
    s: f64,
    global_alpha: f64,
) -> Result<(), String> {
    use notation::model::FontStyle as S;
    for run in &block.runs {
        let face = raster_face(raster, &run.font, run.index)?;
        let eff = match (run.embolden, run.skew_degrees != 0.0) {
            (false, false) => S::Regular,
            (true, false) => S::Bold,
            (false, true) => S::Italic,
            (true, true) => S::BoldItalic,
        };
        let color = (run.colour[0], run.colour[1], run.colour[2]);
        for g in &run.glyphs {
            face.blit_gid_faded(frame, cw, chh, g.id, g.ch, run.size as f64 * s, x + g.x as f64 * s, y + g.y as f64 * s, color, eff, global_alpha);
        }
    }
    debug_assert!(block.pictures.is_empty(), "faded direct path takes no emoji pictures");
    Ok(())
}

/// Draw one text item (T010): uniform `scale` reshapes the ink about
/// the top-left, `rotation` (degrees, clockwise positive) turns it
/// about the top-left, `opacity` fades it. The still transform
/// (scale 1, rotation 0, opacity 1) draws directly exactly as before;
/// an unrotated fade without emoji pictures also draws directly with
/// the fade baked per glyph (T004 speed); any other transform renders
/// into a scratch layer and composites only its ink box through the
/// rotation with the fade baked in.
pub(crate) fn draw_text_block(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    block: &layout::Block,
    tc: &crate::frame::ResolvedText,
) -> Result<(), String> {
    if tc.scale <= 0.0 || !tc.scale.is_finite() || tc.opacity <= 0.0 {
        return Ok(());
    }
    if tc.rotation == 0.0 || !tc.rotation.is_finite() {
        if tc.opacity >= 1.0 {
            return draw_block_scaled(frame, cw, chh, raster, block, tc.x, tc.y, tc.scale);
        }
        if block.pictures.is_empty() {
            return draw_block_scaled_faded(frame, cw, chh, raster, block, tc.x, tc.y, tc.scale, tc.opacity.clamp(0.0, 1.0));
        }
        let mut layer = vec![0u8; cw as usize * chh as usize * 4];
        draw_block_layer(&mut layer, cw, chh, raster, block, tc.x, tc.y, tc.scale, tc.opacity.clamp(0.0, 1.0))?;
        if let Some(bbox) = layer_ink_bbox(&layer, cw, chh) {
            composite_layer_bbox(frame, cw, chh, &layer, bbox, tc.x, tc.y, 0.0);
        }
        return Ok(());
    }
    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
    draw_block_layer(&mut layer, cw, chh, raster, block, tc.x, tc.y, tc.scale, tc.opacity.clamp(0.0, 1.0))?;
    if let Some(bbox) = layer_ink_bbox(&layer, cw, chh) {
        composite_layer_bbox(frame, cw, chh, &layer, bbox, tc.x, tc.y, tc.rotation);
    }
    Ok(())
}

/// Bitmap last resort: the composition declares no `font` and the
/// machine offers none. Draws from the resolved description with the
/// built-in 5x7 face (unknown cells draw hollow, never blank),
/// announced in the note. Bitmap text scaled by `s` about `(x, y)`.
fn draw_bitmap_text_scaled(frame: &mut [u8], cw: u32, chh: u32, tc: &crate::frame::ResolvedText, x: f64, y: f64, s: f64) {
    let face = Face::fallback();
    let laid = face.layout(&tc.value, tc.size, None);
    // The declared separation wins; zero spacing means default advance.
    let advance = tc.line_height.unwrap_or(laid.line_height);
    let spacing = tc.letter_spacing.unwrap_or(0.0).max(0.0);
    let color = (tc.color.r, tc.color.g, tc.color.b);
    for (li, line) in laid.lines.iter().enumerate() {
        let baseline = y + laid.ascent * s + li as f64 * advance * s;
        let mut dx = 0.0;
        for g in &line.glyphs {
            face.blit(frame, cw, chh, g.ch, tc.size * s, x + g.x * s + dx * s, baseline, color, tc.style);
            dx += spacing;
        }
    }
}

/// Bitmap text item with motion (T010): still transforms draw directly;
/// rotated or faded text renders into a scratch layer and composites only
/// its ink box (T004 speed).
pub(crate) fn draw_bitmap_text_item(frame: &mut [u8], cw: u32, chh: u32, tc: &crate::frame::ResolvedText) {
    if tc.scale <= 0.0 || !tc.scale.is_finite() || tc.opacity <= 0.0 {
        return;
    }
    if (tc.rotation == 0.0 || !tc.rotation.is_finite()) && tc.opacity >= 1.0 {
        draw_bitmap_text_scaled(frame, cw, chh, tc, tc.x, tc.y, tc.scale);
        return;
    }
    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
    draw_bitmap_text_layer(&mut layer, cw, chh, tc, tc.scale, tc.opacity.clamp(0.0, 1.0));
    if let Some(bbox) = layer_ink_bbox(&layer, cw, chh) {
        composite_layer_bbox(frame, cw, chh, &layer, bbox, tc.x, tc.y, if tc.rotation.is_finite() { tc.rotation } else { 0.0 });
    }
}

/// Bitmap text into an RGBA `layer`, carrying `global_alpha`.
fn draw_bitmap_text_layer(layer: &mut [u8], cw: u32, chh: u32, tc: &crate::frame::ResolvedText, s: f64, global_alpha: f64) {
    let face = Face::fallback();
    let laid = face.layout(&tc.value, tc.size, None);
    let advance = tc.line_height.unwrap_or(laid.line_height);
    let spacing = tc.letter_spacing.unwrap_or(0.0).max(0.0);
    let color = (tc.color.r, tc.color.g, tc.color.b);
    for (li, line) in laid.lines.iter().enumerate() {
        let baseline = tc.y + laid.ascent * s + li as f64 * advance * s;
        let mut dx = 0.0;
        for g in &line.glyphs {
            face.blit_gid_layer(layer, cw, chh, 0, g.ch, tc.size * s, tc.x + g.x * s + dx * s, baseline, color, tc.style, global_alpha);
            dx += spacing;
        }
    }
}

/// Composite one straight-RGBA layer over the RGB24 canvas through a
/// rotation (degrees, clockwise positive) about `(ax, ay)`, source-over
/// with the layer alpha (T010 text turns and fades). The layer is full
/// canvas size; pixels outside its rotated extent keep the canvas.
/// A zero rotation samples texels exactly, never resampling stills.
/// Production paths use the ink-box variant below; this full walk stays
/// as the test oracle for it.
#[cfg(test)]
fn composite_layer(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    layer: &[u8],
    ax: f64,
    ay: f64,
    rotation: f64,
) {
    let theta = rotation.to_radians();
    let (sin, cos) = theta.sin_cos();
    // Rotated-canvas bounds about the anchor: the farthest corner sets
    // the radius, so every turned pixel is visited.
    let corners = [
        (0.0, 0.0),
        (cw as f64, 0.0),
        (0.0, chh as f64),
        (cw as f64, chh as f64),
    ];
    let radius = corners
        .iter()
        .map(|(x, y)| ((x - ax).powi(2) + (y - ay).powi(2)).sqrt())
        .fold(0.0f64, f64::max)
        .ceil() as i64;
    let i0 = ((ax - radius as f64).floor() as i64).max(0);
    let i1 = ((ax + radius as f64).ceil() as i64).min(cw as i64 - 1);
    let j0 = ((ay - radius as f64).floor() as i64).max(0);
    let j1 = ((ay + radius as f64).ceil() as i64).min(chh as i64 - 1);
    for j in j0..=j1 {
        for i in i0..=i1 {
            // Into layer space through the inverse rotation.
            let qx = i as f64 + 0.5 - ax;
            let qy = j as f64 + 0.5 - ay;
            let lx = qx * cos + qy * sin + ax - 0.5;
            let ly = -qx * sin + qy * cos + ay - 0.5;
            let x0 = lx.floor() as i64;
            let y0 = ly.floor() as i64;
            if x0 < -1 || y0 < -1 || x0 > cw as i64 || y0 > chh as i64 {
                continue;
            }
            let (fx, fy) = (lx - x0 as f64, ly - y0 as f64);
            // Outside the layer the texel is transparent.
            let texel = |xx: i64, yy: i64| -> [f64; 4] {
                if xx < 0 || yy < 0 || xx >= cw as i64 || yy >= chh as i64 {
                    return [0.0; 4];
                }
                let s = ((yy as u32 * cw + xx as u32) as usize) * 4;
                [
                    layer[s] as f64,
                    layer[s + 1] as f64,
                    layer[s + 2] as f64,
                    layer[s + 3] as f64 / 255.0,
                ]
            };
            let mut rgba = [0.0f64; 4];
            for c in 0..4 {
                let top = texel(x0, y0)[c] * (1.0 - fx) + texel(x0 + 1, y0)[c] * fx;
                let bot = texel(x0, y0 + 1)[c] * (1.0 - fx) + texel(x0 + 1, y0 + 1)[c] * fx;
                rgba[c] = top * (1.0 - fy) + bot * fy;
            }
            let a = rgba[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let o = (j as u32 * cw + i as u32) as usize * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            let inv = 1.0 - a;
            frame[o] = (rgba[0] * a + frame[o] as f64 * inv + 0.5) as u8;
            frame[o + 1] = (rgba[1] * a + frame[o + 1] as f64 * inv + 0.5) as u8;
            frame[o + 2] = (rgba[2] * a + frame[o + 2] as f64 * inv + 0.5) as u8;
        }
    }
}

/// Ink bounds of a full-canvas straight-RGBA layer (T004): the inclusive
/// `(x0, y0, x1, y1)` texel rect covering every texel with nonzero
/// alpha, or `None` when the layer is fully transparent. One linear scan
/// of the alpha channel — far cheaper than compositing the whole canvas
/// for a small ink area.
fn layer_ink_bbox(layer: &[u8], cw: u32, chh: u32) -> Option<(i64, i64, i64, i64)> {
    if cw == 0 || chh == 0 {
        return None;
    }
    let total = cw as usize * chh as usize;
    let cwu = cw as usize;
    let (mut x0, mut y0) = (cw as i64, chh as i64);
    let (mut x1, mut y1) = (-1i64, -1i64);
    let mut touch = |t: usize| {
        let i = (t % cwu) as i64;
        let j = (t / cwu) as i64;
        if i < x0 {
            x0 = i;
        }
        if i > x1 {
            x1 = i;
        }
        if j < y0 {
            y0 = j;
        }
        if j > y1 {
            y1 = j;
        }
    };
    // Four texels per step (T004 speed): the alpha bytes sit at offsets
    // 3, 7, 11 and 15, so one test skips all four when transparent —
    // the common case for text ink on an empty layer.
    for (k, w) in layer.chunks_exact(16).take(total / 4).enumerate() {
        if w[3] == 0 && w[7] == 0 && w[11] == 0 && w[15] == 0 {
            continue;
        }
        let t = k * 4;
        if w[3] != 0 {
            touch(t);
        }
        if w[7] != 0 {
            touch(t + 1);
        }
        if w[11] != 0 {
            touch(t + 2);
        }
        if w[15] != 0 {
            touch(t + 3);
        }
    }
    // Up to three trailing texels fall outside the full words.
    for t in (total / 4 * 4)..total {
        let s = t * 4;
        if s + 3 < layer.len() && layer[s + 3] != 0 {
            touch(t);
        }
    }
    if x1 < 0 {
        None
    } else {
        Some((x0, y0, x1, y1))
    }
}

/// Composite a full-canvas straight-RGBA layer over the RGB24 canvas,
/// visiting only the ink bounding box (T004 speed): byte-identical to
/// the full-canvas walk. The zero-rotation path copies texels exactly —
/// the full walk samples integers there, so its bilinear weights are
/// exactly 1 and 0. The rotated path runs the same bilinear math over
/// the rotated extent of the box grown by one texel plus a two-pixel
/// guard; every canvas pixel outside that extent can only sample
/// transparent texels, so skipping it changes no byte.
fn composite_layer_bbox(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    layer: &[u8],
    bbox: (i64, i64, i64, i64),
    ax: f64,
    ay: f64,
    rotation: f64,
) {
    let (bx0, by0, bx1, by1) = bbox;
    if rotation == 0.0 || !rotation.is_finite() {
        let i0 = bx0.max(0);
        let i1 = (bx1 + 1).min(cw as i64);
        let j0 = by0.max(0);
        let j1 = (by1 + 1).min(chh as i64);
        for j in j0..j1 {
            let row_l = (j as u32 * cw) as usize * 4;
            let row_f = (j as u32 * cw) as usize * 3;
            for i in i0..i1 {
                let s = row_l + (i as usize) * 4;
                if s + 3 >= layer.len() {
                    continue;
                }
                let a = layer[s + 3] as f64 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let o = row_f + (i as usize) * 3;
                if o + 2 >= frame.len() {
                    continue;
                }
                if a >= 1.0 {
                    frame[o] = layer[s];
                    frame[o + 1] = layer[s + 1];
                    frame[o + 2] = layer[s + 2];
                    continue;
                }
                let inv = 1.0 - a;
                frame[o] = (layer[s] as f64 * a + frame[o] as f64 * inv + 0.5) as u8;
                frame[o + 1] = (layer[s + 1] as f64 * a + frame[o + 1] as f64 * inv + 0.5) as u8;
                frame[o + 2] = (layer[s + 2] as f64 * a + frame[o + 2] as f64 * inv + 0.5) as u8;
            }
        }
        return;
    }
    let theta = rotation.to_radians();
    let (sin, cos) = theta.sin_cos();
    // Forward-rotate the grown-box corners about the anchor: the inverse
    // walk samples `(lx, ly)` through `qx * cos + qy * sin` /
    // `-qx * sin + qy * cos`, so the forward map is the transpose.
    // The two-pixel guard absorbs float rounding at the edges.
    let gx = [bx0 as f64 - 1.0, bx1 as f64 + 1.0];
    let gy = [by0 as f64 - 1.0, by1 as f64 + 1.0];
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &cx in &gx {
        for &cy in &gy {
            let qx = cx - ax + 0.5;
            let qy = cy - ay + 0.5;
            let px = ax + (qx * cos - qy * sin) - 0.5;
            let py = ay + (qx * sin + qy * cos) - 0.5;
            lo_x = lo_x.min(px);
            lo_y = lo_y.min(py);
            hi_x = hi_x.max(px);
            hi_y = hi_y.max(py);
        }
    }
    let i0 = (lo_x.floor() as i64 - 2).max(0);
    let i1 = (hi_x.ceil() as i64 + 2).min(cw as i64 - 1);
    let j0 = (lo_y.floor() as i64 - 2).max(0);
    let j1 = (hi_y.ceil() as i64 + 2).min(chh as i64 - 1);
    for j in j0..=j1 {
        for i in i0..=i1 {
            // Into layer space through the inverse rotation.
            let qx = i as f64 + 0.5 - ax;
            let qy = j as f64 + 0.5 - ay;
            let lx = qx * cos + qy * sin + ax - 0.5;
            let ly = -qx * sin + qy * cos + ay - 0.5;
            let x0 = lx.floor() as i64;
            let y0 = ly.floor() as i64;
            if x0 < -1 || y0 < -1 || x0 > cw as i64 || y0 > chh as i64 {
                continue;
            }
            let (fx, fy) = (lx - x0 as f64, ly - y0 as f64);
            // Outside the layer the texel is transparent.
            let texel = |xx: i64, yy: i64| -> [f64; 4] {
                if xx < 0 || yy < 0 || xx >= cw as i64 || yy >= chh as i64 {
                    return [0.0; 4];
                }
                let s = ((yy as u32 * cw + xx as u32) as usize) * 4;
                [
                    layer[s] as f64,
                    layer[s + 1] as f64,
                    layer[s + 2] as f64,
                    layer[s + 3] as f64 / 255.0,
                ]
            };
            let mut rgba = [0.0f64; 4];
            for c in 0..4 {
                let top = texel(x0, y0)[c] * (1.0 - fx) + texel(x0 + 1, y0)[c] * fx;
                let bot = texel(x0, y0 + 1)[c] * (1.0 - fx) + texel(x0 + 1, y0 + 1)[c] * fx;
                rgba[c] = top * (1.0 - fy) + bot * fy;
            }
            let a = rgba[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let o = (j as u32 * cw + i as u32) as usize * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            let inv = 1.0 - a;
            frame[o] = (rgba[0] * a + frame[o] as f64 * inv + 0.5) as u8;
            frame[o + 1] = (rgba[1] * a + frame[o + 1] as f64 * inv + 0.5) as u8;
            frame[o + 2] = (rgba[2] * a + frame[o + 2] as f64 * inv + 0.5) as u8;
        }
    }
}

/// Blend straight-RGBA source pixels into a straight-RGBA layer with
/// source-over, scaling the source alpha by `global_alpha` (emoji
/// pictures on the text fade path).
fn blend_rgba_layer(
    layer: &mut [u8],
    lw: u32,
    lh: u32,
    rgba: &[u8],
    w: u32,
    h: u32,
    x0: i64,
    y0: i64,
    global_alpha: f64,
) {
    for j in 0..h {
        let y = y0 + j as i64;
        if y < 0 || y >= lh as i64 {
            continue;
        }
        for i in 0..w {
            let x = x0 + i as i64;
            if x < 0 || x >= lw as i64 {
                continue;
            }
            let s = ((j * w + i) as usize) * 4;
            if s + 3 >= rgba.len() {
                continue;
            }
            let a = (rgba[s + 3] as f64 / 255.0 * global_alpha).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let o = (y as u32 * lw + x as u32) as usize * 4;
            if o + 3 >= layer.len() {
                continue;
            }
            let dst_a = layer[o + 3] as f64 / 255.0;
            let out_a = a + dst_a * (1.0 - a);
            if out_a <= 0.0 {
                continue;
            }
            layer[o] = ((rgba[s] as f64 * a + layer[o] as f64 * dst_a * (1.0 - a)) / out_a + 0.5) as u8;
            layer[o + 1] = ((rgba[s + 1] as f64 * a + layer[o + 1] as f64 * dst_a * (1.0 - a)) / out_a + 0.5) as u8;
            layer[o + 2] = ((rgba[s + 2] as f64 * a + layer[o + 2] as f64 * dst_a * (1.0 - a)) / out_a + 0.5) as u8;
            layer[o + 3] = (out_a * 255.0 + 0.5) as u8;
        }
    }
}

/// Composite straight-RGBA pixels over the RGB24 canvas with
/// source-over, clipped to the canvas. Emoji pictures keep their own
/// colors; only their alpha blends with the canvas.
fn blend_rgba(frame: &mut [u8], cw: u32, chh: u32, rgba: &[u8], w: u32, h: u32, x0: i64, y0: i64) {
    for j in 0..h {
        let y = y0 + j as i64;
        if y < 0 || y >= chh as i64 {
            continue;
        }
        for i in 0..w {
            let x = x0 + i as i64;
            if x < 0 || x >= cw as i64 {
                continue;
            }
            let s = ((j * w + i) as usize) * 4;
            if s + 3 >= rgba.len() {
                continue;
            }
            let a = rgba[s + 3] as u32;
            if a == 0 {
                continue;
            }
            let o = (y as u32 * cw + x as u32) as usize * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            if a == 255 {
                frame[o] = rgba[s];
                frame[o + 1] = rgba[s + 1];
                frame[o + 2] = rgba[s + 2];
                continue;
            }
            let inv = 255 - a;
            frame[o] = ((rgba[s] as u32 * a + frame[o] as u32 * inv + 127) / 255) as u8;
            frame[o + 1] = ((rgba[s + 1] as u32 * a + frame[o + 1] as u32 * inv + 127) / 255) as u8;
            frame[o + 2] = ((rgba[s + 2] as u32 * a + frame[o + 2] as u32 * inv + 127) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{ResolvedBox, ResolvedImage};
    use notation::model::{Fit, PlaceH, PlaceV, Placement, Rgb};
    use std::path::PathBuf;

    fn test_box(x: f64, y: f64, w: f64, h: f64, fill: Rgb) -> ResolvedBox {
        ResolvedBox {
            doc: 0,
            order: 0,
            x,
            y,
            width: w,
            height: h,
            fill,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
        }
    }

    fn pixel(frame: &[u8], cw: u32, x: u32, y: u32) -> (u8, u8, u8) {
        let o = (y * cw + x) as usize * 3;
        (frame[o], frame[o + 1], frame[o + 2])
    }

    #[test]
    fn image_scale_cache_reuses_static_geometry_byte_identically() {
        // T004: a second draw at the same geometry hits the memoized
        // rect (one entry), draws byte-identical pixels, and a moved
        // picture keys a second entry.
        let decoded = solid_image(8, 8, 200, 100, 50, 255);
        let mut im = test_image(Fit::Contain, Placement::center());
        im.width = Some(16.0);
        im.height = Some(16.0);
        let mut a = vec![3u8; 32 * 32 * 3];
        let mut b = vec![3u8; 32 * 32 * 3];
        draw_image(&mut a, 32, 32, &im, &decoded);
        assert_eq!(decoded.scaled.lock().unwrap().len(), 1, "miss inserts one entry");
        draw_image(&mut b, 32, 32, &im, &decoded);
        assert_eq!(decoded.scaled.lock().unwrap().len(), 1, "same geometry hits");
        assert_eq!(a, b, "hit draws byte-identical pixels");
        assert_ne!(a, vec![3u8; 32 * 32 * 3], "the picture actually painted");
        let mut im2 = im.clone();
        im2.x = 4.0;
        let mut c = vec![3u8; 32 * 32 * 3];
        draw_image(&mut c, 32, 32, &im2, &decoded);
        assert_eq!(decoded.scaled.lock().unwrap().len(), 2, "moved picture keys again");
        assert_ne!(b, c, "moved picture paints differently");
        // A faded draw at cached geometry blends the same rect: compare
        // against a fresh (uncached) source, which must match exactly.
        let fresh = solid_image(8, 8, 200, 100, 50, 255);
        let mut faded_hit = vec![9u8; 32 * 32 * 3];
        let mut faded_fresh = vec![9u8; 32 * 32 * 3];
        let mut im3 = im.clone();
        im3.opacity = 0.5;
        draw_image(&mut faded_hit, 32, 32, &im3, &decoded);
        draw_image(&mut faded_fresh, 32, 32, &im3, &fresh);
        assert_eq!(faded_hit, faded_fresh, "cached rect fades exactly like a fresh sample");
    }

    #[test]
    fn opaque_box_hides_what_is_beneath() {
        // T004: a fully opaque box paints its fill exactly; outside the
        // box the background shows through.
        let mut frame = vec![10u8; 32 * 32 * 3];
        let b = test_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        draw_box(&mut frame, 32, 32, &b);
        assert_eq!(pixel(&frame, 32, 16, 16), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 9, 9), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 0, 0), (10, 10, 10));
        assert_eq!(pixel(&frame, 32, 31, 31), (10, 10, 10));
    }

    #[test]
    fn transparent_box_draws_nothing() {
        // T004: opacity 0 leaves the canvas untouched.
        let mut frame = vec![10u8; 32 * 32 * 3];
        let mut b = test_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        b.opacity = 0.0;
        draw_box(&mut frame, 32, 32, &b);
        assert!(frame.iter().all(|&v| v == 10));
    }

    #[test]
    fn half_opacity_blends_with_the_canvas() {
        // T004: opacity 0.5 blends fill and background evenly.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let mut b = test_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 200, g: 100, b: 0 });
        b.opacity = 0.5;
        draw_box(&mut frame, 32, 32, &b);
        assert_eq!(pixel(&frame, 32, 16, 16), (100, 50, 0));
    }

    #[test]
    fn border_ring_paints_inside_with_fill_within() {
        // T004: a bordered box paints the border ring at its edge and
        // the fill inside it.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let mut b = test_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        b.border_width = 2.0;
        b.border_color = Rgb { r: 0, g: 0, b: 255 };
        draw_box(&mut frame, 32, 32, &b);
        // Edge pixel: border color. Deep interior: fill color.
        assert_eq!(pixel(&frame, 32, 8, 16), (0, 0, 255));
        assert_eq!(pixel(&frame, 32, 16, 16), (255, 0, 0));
        // Just outside the box: untouched.
        assert_eq!(pixel(&frame, 32, 7, 16), (0, 0, 0));
    }

    #[test]
    fn rounded_corners_cut_the_shape() {
        // T004: a large radius rounds the corners away; the middle of
        // each edge still paints.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let mut b = test_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 0, g: 255, b: 0 });
        b.radius = 8.0;
        draw_box(&mut frame, 32, 32, &b);
        assert_eq!(pixel(&frame, 32, 8, 8), (0, 0, 0), "corner is cut");
        assert_eq!(pixel(&frame, 32, 16, 8), (0, 255, 0), "edge middle paints");
        assert_eq!(pixel(&frame, 32, 16, 16), (0, 255, 0));
    }

    #[test]
    fn image_fit_sizes_match_object_fit() {
        // T005: `fill` stretches, `contain` letterboxes, `cover` bleeds.
        assert_eq!(image_fit_size(100.0, 100.0, 200.0, 50.0, Fit::Fill), (100.0, 100.0));
        assert_eq!(image_fit_size(100.0, 100.0, 200.0, 50.0, Fit::Contain), (100.0, 25.0));
        assert_eq!(image_fit_size(100.0, 100.0, 200.0, 50.0, Fit::Cover), (400.0, 100.0));
    }

    fn solid_image(w: u32, h: u32, r: u8, g: u8, b: u8, a: u8) -> DecodedImage {
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for _ in 0..w * h {
            rgba.extend_from_slice(&[r, g, b, a]);
        }
        DecodedImage {
            path: PathBuf::from("media/photo.png"),
            width: w,
            height: h,
            rgba,
            scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    fn test_image(fit: Fit, placement: Placement) -> ResolvedImage {
        ResolvedImage {
            doc: 0,
            order: 0,
            path: "media/photo.png".to_string(),
            x: 0.0,
            y: 0.0,
            width: Some(32.0),
            height: Some(32.0),
            fit,
            placement,
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
        }
    }

    #[test]
    fn fill_image_covers_the_target_rect() {
        // T005: a `fill` picture stretches over the whole target.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let decoded = solid_image(4, 4, 255, 0, 0, 255);
        draw_image(&mut frame, 32, 32, &test_image(Fit::Fill, Placement::center()), &decoded);
        assert_eq!(pixel(&frame, 32, 0, 0), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 31, 31), (255, 0, 0));
    }

    #[test]
    fn contain_placement_moves_the_picture() {
        // T005: a `contain` picture letterboxes; left and right
        // placements put the picture at opposite edges.
        let decoded = solid_image(16, 32, 0, 255, 0, 255);
        let mut left = vec![9u8; 32 * 32 * 3];
        let mut right = vec![9u8; 32 * 32 * 3];
        draw_image(
            &mut left,
            32,
            32,
            &test_image(Fit::Contain, Placement { x: PlaceH::Left, y: PlaceV::Center }),
            &decoded,
        );
        draw_image(
            &mut right,
            32,
            32,
            &test_image(Fit::Contain, Placement { x: PlaceH::Right, y: PlaceV::Center }),
            &decoded,
        );
        assert_ne!(left, right, "placements must differ");
        assert_eq!(pixel(&left, 32, 0, 16), (0, 255, 0));
        assert_eq!(pixel(&left, 32, 31, 16), (9, 9, 9));
        assert_eq!(pixel(&right, 32, 31, 16), (0, 255, 0));
        assert_eq!(pixel(&right, 32, 0, 16), (9, 9, 9));
    }

    #[test]
    fn transparent_png_pixels_show_through() {
        // T005: source alpha composites over the canvas.
        let mut frame = vec![77u8; 32 * 32 * 3];
        let decoded = solid_image(4, 4, 255, 0, 0, 0);
        draw_image(&mut frame, 32, 32, &test_image(Fit::Fill, Placement::center()), &decoded);
        assert!(frame.iter().all(|&v| v == 77));
    }

    // --- Sprint 2 motion drawing (T006, T009–T012) ---------------------

    fn motion_box(x: f64, y: f64, w: f64, h: f64, fill: Rgb) -> ResolvedBox {
        ResolvedBox {
            doc: 0,
            order: 0,
            x,
            y,
            width: w,
            height: h,
            fill,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
        }
    }

    #[test]
    fn scaled_box_doubles_its_footprint() {
        // T010: scale 2 about the top-left grows 16x16 to 32x32.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let mut b = motion_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        b.scale = 2.0;
        draw_box(&mut frame, 32, 32, &b);
        assert_eq!(pixel(&frame, 32, 8, 8), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 23, 23), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 31, 31), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 7, 16), (0, 0, 0), "top-left anchors the growth");
        // Half scale shrinks to 8x8.
        let mut half = vec![0u8; 32 * 32 * 3];
        let mut h = motion_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        h.scale = 0.5;
        draw_box(&mut half, 32, 32, &h);
        assert_eq!(pixel(&half, 32, 8, 8), (255, 0, 0));
        assert_eq!(pixel(&half, 32, 15, 15), (255, 0, 0));
        assert_eq!(pixel(&half, 32, 16, 16), (0, 0, 0));
    }

    #[test]
    fn quarter_turn_swaps_the_box_footprint() {
        // T010: a 16x8 box turned 90 degrees covers 8x16 about its center.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let mut b = motion_box(8.0, 4.0, 16.0, 8.0, Rgb { r: 0, g: 255, b: 0 });
        b.rotation = 90.0;
        draw_box(&mut frame, 32, 32, &b);
        assert_eq!(pixel(&frame, 32, 16, 8), (0, 255, 0), "center stays covered");
        assert_eq!(pixel(&frame, 32, 16, 0), (0, 255, 0), "turned box reaches the top middle");
        assert_eq!(pixel(&frame, 32, 16, 15), (0, 255, 0), "and the bottom middle");
        assert_eq!(pixel(&frame, 32, 8, 4), (0, 0, 0), "old corner is now empty");
        assert_eq!(pixel(&frame, 32, 23, 4), (0, 0, 0), "old corner is now empty");
    }

    #[test]
    fn image_opacity_fades_to_background() {
        // T010: opacity 0 draws nothing, 0.5 blends evenly.
        let decoded = solid_image(4, 4, 200, 100, 0, 255);
        let mut gone = vec![7u8; 32 * 32 * 3];
        let mut clear = test_image(Fit::Fill, Placement::center());
        clear.opacity = 0.0;
        draw_image(&mut gone, 32, 32, &clear, &decoded);
        assert!(gone.iter().all(|&v| v == 7));
        let mut half = vec![0u8; 32 * 32 * 3];
        let mut mid = test_image(Fit::Fill, Placement::center());
        mid.opacity = 0.5;
        draw_image(&mut half, 32, 32, &mid, &decoded);
        assert_eq!(pixel(&half, 32, 16, 16), (100, 50, 0));
    }

    #[test]
    fn video_offset_scale_and_opacity_move_the_picture() {
        // T009/T010: the fitted video picture shifts, grows, and fades.
        let sw = 4u32;
        let sh = 4u32;
        let mut src = vec![0u8; sw as usize * sh as usize * 3];
        for px in src.chunks_exact_mut(3) {
            px[0] = 255;
        }
        // Still identity: full-canvas red.
        let mut still = vec![9u8; 32 * 32 * 3];
        blit_fit(&mut still, 32, 32, &src, sw, sh, Fit::Fill, 0.0, 0.0, 1.0, 0.0, 1.0);
        assert_eq!(pixel(&still, 32, 0, 0), (255, 0, 0));
        assert_eq!(pixel(&still, 32, 31, 31), (255, 0, 0));
        // Shifted right by 8: left strip keeps the canvas.
        let mut moved = vec![9u8; 32 * 32 * 3];
        blit_fit(&mut moved, 32, 32, &src, sw, sh, Fit::Fill, 8.0, 0.0, 1.0, 0.0, 1.0);
        assert_eq!(pixel(&moved, 32, 0, 16), (9, 9, 9));
        assert_eq!(pixel(&moved, 32, 8, 16), (255, 0, 0));
        // Opacity 0 paints nothing.
        let mut gone = vec![9u8; 32 * 32 * 3];
        blit_fit(&mut gone, 32, 32, &src, sw, sh, Fit::Fill, 0.0, 0.0, 1.0, 0.0, 0.0);
        assert!(gone.iter().all(|&v| v == 9));
        // Half opacity blends evenly.
        let mut half = vec![0u8; 32 * 32 * 3];
        blit_fit(&mut half, 32, 32, &src, sw, sh, Fit::Fill, 0.0, 0.0, 1.0, 0.0, 0.5);
        assert_eq!(pixel(&half, 32, 16, 16), (128, 0, 0));
    }

    fn test_text(value: &str, scale: f64, rotation: f64, opacity: f64) -> crate::frame::ResolvedText {
        crate::frame::ResolvedText {
            doc: 0,
            order: 0,
            value: value.to_string(),
            x: 4.0,
            y: 4.0,
            size: 14.0,
            color: Rgb { r: 255, g: 255, b: 255 },
            style: notation::model::FontStyle::Regular,
            line_height: None,
            letter_spacing: None,
            scale,
            rotation,
            opacity,
        }
    }

    fn ink_extent(frame: &[u8], cw: u32) -> (u32, u32, u32, usize) {
        let (mut x0, mut x1, mut y1) = (cw, 0u32, 0u32);
        let mut n = 0usize;
        for (i, px) in frame.chunks_exact(3).enumerate() {
            if px[0] > 20 || px[1] > 20 || px[2] > 20 {
                let (x, y) = (i as u32 % cw, i as u32 / cw);
                x0 = x0.min(x);
                x1 = x1.max(x);
                y1 = y1.max(y);
                n += 1;
            }
        }
        (x0, x1, y1, n)
    }

    #[test]
    fn scaled_bitmap_text_grows_about_its_top_left() {
        // T010: the fallback face draws without project fonts, so text
        // scale is pixel-verifiable: doubling the scale doubles the ink.
        let mut one = vec![0u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut one, 64, 32, &test_text("HI", 1.0, 0.0, 1.0));
        let mut two = vec![0u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut two, 64, 32, &test_text("HI", 2.0, 0.0, 1.0));
        let (ax0, ax1, ay1, an) = ink_extent(&one, 64);
        let (bx0, bx1, by1, bn) = ink_extent(&two, 64);
        assert!(an > 10, "fallback ink must draw, got {an} pixels");
        assert_eq!(ax0, bx0, "scale anchors at the top-left");
        assert!((bx1 - bx0) >= (ax1 - ax0) * 2 - 2, "width doubles: {ax0}..{ax1} vs {bx0}..{bx1}");
        assert!(bn > an * 2, "ink area grows: {an} vs {bn}");
        let _ = ay1;
        let _ = by1;
    }

    #[test]
    fn faded_bitmap_text_blends_with_the_canvas() {
        // T010: opacity 0 draws nothing; between 0 and 1 the ink blends.
        let mut gone = vec![9u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut gone, 64, 32, &test_text("HI", 1.0, 0.0, 0.0));
        assert!(gone.iter().all(|&v| v == 9));
        let mut full = vec![9u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut full, 64, 32, &test_text("HI", 1.0, 0.0, 1.0));
        let mut half = vec![9u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut half, 64, 32, &test_text("HI", 1.0, 0.0, 0.5));
        let peak = |f: &[u8]| f.chunks_exact(3).map(|p| p[0]).max().unwrap_or(0);
        assert!(peak(&full) > 200, "opaque ink covers the canvas");
        assert!(peak(&half) > 20 && peak(&half) < peak(&full), "faded ink blends");
    }

    #[test]
    fn rotated_bitmap_text_turns_about_its_top_left() {
        // T010: a quarter turn moves the ink off its row and the old
        // seat keeps the canvas.
        let mut flat = vec![10u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut flat, 64, 32, &test_text("HI", 1.0, 0.0, 1.0));
        let mut turned = vec![10u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut turned, 64, 32, &test_text("HI", 1.0, 90.0, 1.0));
        assert_ne!(flat, turned, "rotation must move the ink");
        let (_, _, _, n) = ink_extent(&turned, 64);
        assert!(n > 10, "turned ink still draws, got {n} pixels");
    }

    #[test]
    fn rotated_video_keeps_its_center() {
        // T010: turning the fitted picture about its own center leaves
        // the middle pixel covered.
        let mut src = vec![0u8; 8 * 4 * 3];
        for px in src.chunks_exact_mut(3) {
            px[1] = 255;
        }
        let mut turned = vec![9u8; 32 * 32 * 3];
        blit_fit(&mut turned, 32, 32, &src, 8, 4, Fit::Fill, 0.0, 0.0, 1.0, 90.0, 1.0);
        assert_eq!(pixel(&turned, 32, 16, 16), (0, 255, 0));
    }

    #[test]
    fn faded_text_without_emoji_draws_directly() {
        // T004: an unrotated fade with no emoji pictures takes the
        // direct path — an empty block draws nothing and returns Ok,
        // exercising the branch without needing a shaped font.
        let block = layout::Block {
            width: 0.0,
            height: 0.0,
            runs: Vec::new(),
            pictures: Vec::new(),
            lines: Vec::new(),
        };
        let mut frame = vec![7u8; 32 * 32 * 3];
        let tc = test_text("Hi", 1.0, 0.0, 0.5);
        let mut raster = RasterCache::new();
        draw_text_block(&mut frame, 32, 32, &mut raster, &block, &tc).unwrap();
        assert!(frame.iter().all(|&v| v == 7), "empty faded text draws nothing");
    }

    #[test]
    fn bbox_composite_matches_the_full_canvas_walk() {
        // T004: the ink-box composite writes exactly what the full-canvas
        // walk writes, still and rotated, on sparse and dense ink.
        let (cw, chh) = (64u32, 48u32);
        let mut layer = vec![0u8; cw as usize * chh as usize * 4];
        for k in 0..300 {
            let x = (k * 37) % 64;
            let y = (k * 53) % 48;
            let s = (y * 64 + x) * 4;
            layer[s] = (k * 11 % 256) as u8;
            layer[s + 1] = (k * 29 % 256) as u8;
            layer[s + 2] = (k * 53 % 256) as u8;
            layer[s + 3] = (k * 97 % 256) as u8;
        }
        // A dense translucent block crossing the canvas edge.
        for y in 40..48 {
            for x in 50..64 {
                let s = (y * 64 + x) * 4;
                layer[s] = 200;
                layer[s + 1] = 100;
                layer[s + 2] = 50;
                layer[s + 3] = 128;
            }
        }
        let bbox = layer_ink_bbox(&layer, cw, chh).expect("ink is present");
        for rotation in [0.0, 33.0, 90.0, -45.0, 180.0, 7.5] {
            let mut a = vec![7u8; cw as usize * chh as usize * 3];
            let mut b = a.clone();
            composite_layer(&mut a, cw, chh, &layer, 10.0, 12.0, rotation);
            composite_layer_bbox(&mut b, cw, chh, &layer, bbox, 10.0, 12.0, rotation);
            assert_eq!(a, b, "rotation {rotation}");
        }
        // An empty layer reports no box; callers then draw nothing.
        let empty = vec![0u8; cw as usize * chh as usize * 4];
        assert_eq!(layer_ink_bbox(&empty, cw, chh), None);
        assert_eq!(layer_ink_bbox(&[], 0, 0), None);
        // Odd canvas areas still bound the last texel exactly.
        let (ow, oh) = (5u32, 3u32);
        let mut odd = vec![0u8; ow as usize * oh as usize * 4];
        odd[(14 * 4) + 3] = 200;
        assert_eq!(layer_ink_bbox(&odd, ow, oh), Some((4, 2, 4, 2)));
    }

    #[test]
    fn composite_layer_copies_exactly_without_rotation() {
        // The fade path with no turn resamples nothing: texels land exactly.
        let mut layer = vec![0u8; 32 * 32 * 4];
        let o = (16 * 32 + 5) as usize * 4;
        layer[o] = 255;
        layer[o + 3] = 255;
        let mut frame = vec![10u8; 32 * 32 * 3];
        composite_layer(&mut frame, 32, 32, &layer, 0.0, 0.0, 0.0);
        assert_eq!(pixel(&frame, 32, 5, 16), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 5, 15), (10, 10, 10));
        assert_eq!(pixel(&frame, 32, 6, 16), (10, 10, 10));
    }

    #[test]
    fn composite_layer_turns_pixels_about_the_anchor() {
        // A red texel right of the anchor lands below it after a
        // clockwise quarter turn (texel centers rotate, so the landing
        // pixel is the rotated center), and its old seat keeps the canvas.
        let mut layer = vec![0u8; 32 * 32 * 4];
        let o = (16 * 32 + 20) as usize * 4;
        layer[o] = 255;
        layer[o + 3] = 255;
        let mut frame = vec![10u8; 32 * 32 * 3];
        composite_layer(&mut frame, 32, 32, &layer, 16.0, 16.0, 90.0);
        assert_eq!(pixel(&frame, 32, 20, 16), (10, 10, 10));
        let (r, g, b) = pixel(&frame, 32, 15, 20);
        assert!(r > 200 && g < 55 && b < 55, "turned pixel lands below, got ({r}, {g}, {b})");
    }

    #[test]
    fn drawn_frames_follow_paint_order_only() {
        // T006 at the blitter: swapping document positions while fixed
        // written orders hold leaves the picture unchanged; equal
        // orders follow document order.
        use crate::frame::{describe, FrameDescription};
        use crate::motion_plan::plan_motions;
        use notation::model::{BoxClip, Composition, Fps, Rgb};
        fn comp(order_first: bool, tie: bool) -> Composition {
            let (red_order, blue_order) = if tie { (0, 0) } else { (1, 0) };
            let mut clips = vec![
                BoxClip {
                    start: 0.0, duration: 1.0, x: 4.0, y: 4.0,
                    width: 16.0, height: 16.0,
                    fill: Rgb { r: 255, g: 0, b: 0 },
                    radius: 0.0, border_width: 0.0,
                    border_color: Rgb { r: 0, g: 0, b: 0 },
                    opacity: 1.0, order: red_order, line: 1, column: 1,
                },
                BoxClip {
                    start: 0.0, duration: 1.0, x: 8.0, y: 8.0,
                    width: 16.0, height: 16.0,
                    fill: Rgb { r: 0, g: 0, b: 255 },
                    radius: 0.0, border_width: 0.0,
                    border_color: Rgb { r: 0, g: 0, b: 0 },
                    opacity: 1.0, order: blue_order, line: 1, column: 1,
                },
            ];
            if !order_first {
                clips.swap(0, 1);
            }
            Composition {
                name: "t".to_string(), width: 32, height: 32, duration: 1.0,
                fps: Fps::from_int(30), background: Rgb { r: 0, g: 0, b: 0 },
                version: 1,
                clips: clips.into_iter().map(notation::model::Clip::Box).collect(),
                fonts: Vec::new(), tweens: Vec::new(), sets: Vec::new(),
            }
        }
        fn draw(comp: &Composition) -> Vec<u8> {
            let motions = plan_motions(comp, 30);
            let desc: FrameDescription = describe(comp, &motions, 0);
            let mut raster = std::collections::HashMap::new();
            let mut frame = vec![0u8; 32 * 32 * 3];
            draw_description(&desc, &mut raster, &[], false, &[], &[], &[], &mut frame).unwrap();
            frame
        }
        // Distinct written orders: document swap changes nothing; the
        // higher order (red) covers the overlap either way.
        let a = draw(&comp(true, false));
        let b = draw(&comp(false, false));
        assert_eq!(a, b, "picture follows written order, not document order");
        let o = (12 * 32 + 12) as usize * 3;
        assert_eq!((a[o], a[o + 1], a[o + 2]), (255, 0, 0));
        // Equal orders: document order decides, so the swap shows.
        let c = draw(&comp(true, true));
        let d = draw(&comp(false, true));
        assert_ne!(c, d, "ties must follow document order");
        assert_eq!((c[o], c[o + 1], c[o + 2]), (0, 0, 255));
        assert_eq!((d[o], d[o + 1], d[o + 2]), (255, 0, 0));
    }

    #[test]
    fn durations_parse() {
        assert!((parse_duration("  Duration: 00:00:03.00, start: 0.0").unwrap() - 3.0).abs() < 1e-9);
        assert!((parse_duration("Duration: 01:02:03.50, x").unwrap() - 3723.5).abs() < 1e-9);
        assert!(parse_duration("no duration here").is_none());
    }

    #[test]
    fn video_stream_lines_parse() {
        let line = "Stream #0:0[0x1](und): Video: h264 (High), yuv420p, 160x120 [SAR 1:1], 15 fps";
        assert_eq!(
            parse_video_stream(line),
            Some(("h264".to_string(), 160, 120))
        );
        let hevc = "Stream #0:0: Video: hevc (Main), yuv420p, 64x48, 60 fps, 60 tbr";
        assert_eq!(parse_video_stream(hevc), Some(("hevc".to_string(), 64, 48)));
        assert!(parse_video_stream("Stream #0:1: Audio: aac (LC)").is_none());
        assert!(parse_video_stream("garbage").is_none());
    }

    #[test]
    fn fit_rects_center_and_cover() {
        // Contain: pillarbox the narrow source, centered.
        let (dx, dy, dw, dh) = fit_rect(320, 200, 160, 120, Fit::Contain);
        assert!((dw - 266.666).abs() < 0.01);
        assert!((dh - 200.0).abs() < 1e-9);
        assert!((dx - (320.0 - dw) / 2.0).abs() < 1e-9);
        assert!((dy).abs() < 1e-9);
        // Cover: full bleed, centered crop.
        let (dx, dy, dw, dh) = fit_rect(320, 200, 160, 120, Fit::Cover);
        assert!((dw - 320.0).abs() < 1e-9);
        assert!((dh - 240.0).abs() < 1e-9);
        assert!((dx).abs() < 1e-9);
        assert!((dy + 20.0).abs() < 1e-9);
        // Fill: exact canvas, no centering.
        assert_eq!(fit_rect(320, 200, 160, 120, Fit::Fill), (0.0, 0.0, 320.0, 200.0));
    }

    #[test]
    fn picture_selection_is_last_at_or_before() {
        let range = DecodedRange {
            video_idx: 0,
            width: 2,
            height: 2,
            frames: vec![0; 3 * 12],
            pts: vec![0.0, 0.0667, 0.1333],
        };
        assert_eq!(pick_picture(&range, 0.0), Some(0));
        assert_eq!(pick_picture(&range, 0.0667), Some(1));
        assert_eq!(pick_picture(&range, 0.1), Some(1));
        assert_eq!(pick_picture(&range, 0.1333), Some(2));
        // Past the end by more than a gap: refuse (None), never hold.
        assert_eq!(pick_picture(&range, 1.0), None);
        // Before the first timestamp: first picture (sub-frame offsets).
        assert_eq!(pick_picture(&range, -0.5), Some(0));
    }

    #[test]
    fn encode_args_pin_the_standard_mp4() {
        use notation::model::{Composition, Fps, Rgb};
        let comp = Composition {
            name: "t".to_string(),
            width: 64,
            height: 48,
            duration: 1.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: Vec::new(),
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
        };
        let tmp = PathBuf::from("/tmp/x.framesmith-part");
        let standard = RenderOptions::default().encoder();
        let no_audio = encode_args(&comp, &[], None, &tmp, &standard);
        let joined = no_audio.join(" ");
        for flag in ["libx264", "yuv420p", "aac", "48000", "rawvideo", "-f", "mp4"] {
            assert!(joined.contains(flag), "{joined}");
        }
        // Sprint 3 T002: `standard` with no overrides emits no explicit
        // encoder flags, so its bytes match the Sprint 2 output exactly.
        assert!(!joined.contains("-crf"), "{joined}");
        assert!(!joined.contains("-preset"), "{joined}");
        assert!(!joined.contains("-b:v"), "{joined}");
        // The output path is the last argument (fake-FFmpeg compatible).
        assert_eq!(no_audio.last().unwrap(), &"/tmp/x.framesmith-part".to_string());
        // With audio: filter trims and delays, still standard codecs.
        let seg = AudioSeg {
            path: PathBuf::from("/tmp/a.mp4"),
            trim_start: 0.0,
            trim_len: 1.0,
            delay_ms: 500,
        };
        let with_audio = encode_args(&comp, &[seg], None, &tmp, &standard).join(" ");
        assert!(with_audio.contains("adelay=500"), "{with_audio}");
        assert!(with_audio.contains("atrim"), "{with_audio}");
        assert!(!with_audio.contains("volume"), "{with_audio}");
        assert!(!with_audio.contains("afade"), "{with_audio}");
    }

    #[test]
    fn backend_spellings_resolve_through_the_probe() {
        // Sprint 3 T001: exactly three spellings; `auto` follows the
        // capability probe, explicit choices pass through.
        assert_eq!(Backend::parse("cpu"), Some(Backend::Cpu));
        assert_eq!(Backend::parse("gpu"), Some(Backend::Gpu));
        assert_eq!(Backend::parse("auto"), Some(Backend::Auto));
        for bad in ["", "CPU", "cuda", "metal", "cpu,gpu", " auto "] {
            assert_eq!(Backend::parse(bad), None, "{bad:?}");
        }
        assert_eq!(resolve_backend(Backend::Cpu), Backend::Cpu);
        assert_eq!(resolve_backend(Backend::Gpu), Backend::Gpu);
        // The probe decides `auto`: with no override env the machine
        // reports no GPU renderer, so `auto` means the CPU renderer.
        let saved = std::env::var("FRAMESMITH_GPU_AVAILABLE").ok();
        std::env::remove_var("FRAMESMITH_GPU_AVAILABLE");
        assert!(!crate::gpu::gpu_available());
        assert_eq!(resolve_backend(Backend::Auto), Backend::Cpu);
        std::env::set_var("FRAMESMITH_GPU_AVAILABLE", "1");
        assert!(crate::gpu::gpu_available());
        assert_eq!(resolve_backend(Backend::Auto), Backend::Gpu);
        std::env::set_var("FRAMESMITH_GPU_AVAILABLE", "0");
        assert!(!crate::gpu::gpu_available());
        match saved {
            Some(v) => std::env::set_var("FRAMESMITH_GPU_AVAILABLE", v),
            None => std::env::remove_var("FRAMESMITH_GPU_AVAILABLE"),
        }
    }

    #[test]
    fn preset_numbers_and_overrides_set_only_their_own_axis() {
        // Sprint 3 T002: exact numbers recorded next to the fixtures.
        let draft = Preset::Draft.encoder();
        assert_eq!((draft.crf, draft.x264_preset), (30, Some("veryfast")));
        let standard = Preset::Standard.encoder();
        assert_eq!((standard.crf, standard.x264_preset), (23, None));
        let high = Preset::High.encoder();
        assert_eq!((high.crf, high.x264_preset), (18, Some("slow")));
        for bad in ["", "STANDARD", "ultra", "4k"] {
            assert_eq!(Preset::parse(bad), None, "{bad:?}");
        }
        // No preset named behaves exactly like `standard` and announces it.
        let named = RenderOptions {
            preset: Preset::Standard,
            ..RenderOptions::default()
        };
        assert_eq!(RenderOptions::default().encoder(), named.encoder());
        assert!(RenderOptions::default().announce().contains("preset=standard"));
        // `--crf` overrides only the CRF axis of the active preset.
        let over = RenderOptions {
            preset: Preset::Draft,
            crf_override: Some(20),
            ..RenderOptions::default()
        };
        let enc = over.encoder();
        assert_eq!((enc.crf, enc.x264_preset), (20, Some("veryfast")));
        assert!(over.announce().contains("crf=20"));
        // `--video-bitrate` adds its own axis alongside the preset CRF.
        let over = RenderOptions {
            video_bitrate: Some("2M".to_string()),
            ..RenderOptions::default()
        };
        let enc = over.encoder();
        assert_eq!((enc.crf, enc.video_bitrate.as_deref()), (23, Some("2M")));
    }

    #[test]
    fn encoder_flags_follow_presets_and_overrides() {
        use notation::model::{Composition, Fps, Rgb};
        let comp = Composition {
            name: "t".to_string(),
            width: 64,
            height: 48,
            duration: 1.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: Vec::new(),
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
        };
        let tmp = PathBuf::from("/tmp/x.framesmith-part");
        let draft = RenderOptions {
            preset: Preset::Draft,
            ..RenderOptions::default()
        }
        .encoder();
        let joined = encode_args(&comp, &[], None, &tmp, &draft).join(" ");
        assert!(joined.contains("-crf 30"), "{joined}");
        assert!(joined.contains("-preset veryfast"), "{joined}");
        let high = RenderOptions {
            preset: Preset::High,
            ..RenderOptions::default()
        }
        .encoder();
        let joined = encode_args(&comp, &[], None, &tmp, &high).join(" ");
        assert!(joined.contains("-crf 18"), "{joined}");
        assert!(joined.contains("-preset slow"), "{joined}");
        // A staged mix joins the encoder as a raw 48 kHz stereo input.
        let mix = PathBuf::from("/tmp/x.framesmith-mix.pcm");
        let joined = encode_args(&comp, &[], Some(&mix), &tmp, &high).join(" ");
        assert!(joined.contains("s16le"), "{joined}");
        assert!(joined.contains("48000"), "{joined}");
        assert!(joined.contains("[mix]"), "{joined}");
    }

    #[test]
    fn mix_gain_math_clips_nothing_silently() {
        // Sprint 3 T008: silence is exact zero, +6 dB doubles amplitude,
        // and anything above +12 dB is refused, never limited.
        assert_eq!(db_to_gain(f64::NEG_INFINITY).unwrap(), 0.0);
        assert_eq!(db_to_gain(0.0).unwrap(), 1.0);
        let plus6 = db_to_gain(6.0).unwrap();
        assert!((plus6 - 1.9952622).abs() < 1e-4, "{plus6}");
        assert!((db_to_gain(12.0).unwrap() - 3.9810717).abs() < 1e-6);
        for bad in [12.0001, 20.0, f64::INFINITY, f64::NAN] {
            assert!(db_to_gain(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn mix_fades_are_linear_in_amplitude() {
        // A 4-sample note with a 2-sample fade-in and 2-sample fade-out.
        assert_eq!(fade_factor(0, 4, 0, 0), 1.0);
        assert_eq!(fade_factor(0, 4, 2, 0), 0.5);
        assert_eq!(fade_factor(1, 4, 2, 0), 1.0);
        assert_eq!(fade_factor(3, 4, 0, 2), 0.0);
        assert_eq!(fade_factor(2, 4, 0, 2), 0.5);
        // Edges clamp to the placed window, never divide by zero.
        assert_eq!(fade_factor(0, 1, 5, 5), 0.0);
    }

    #[test]
    fn mix_sums_overlaps_deterministically() {
        // Two identical tones sum sample by sample (no limiter, no
        // normalization); the same inputs always add to the same mix.
        let tone: Vec<i16> = vec![1000, -1000, 2000, -2000];
        let mut a = vec![0.0f32; 4];
        mix_into(&mut a, &tone, 0, 1.0, 0, 0);
        let mut b = vec![0.0f32; 4];
        mix_into(&mut b, &tone, 0, 1.0, 0, 0);
        mix_into(&mut b, &tone, 0, 1.0, 0, 0);
        assert_eq!(a.len(), b.len());
        for i in 0..a.len() {
            assert!((b[i] - 2.0 * a[i]).abs() < 1e-9, "{b:?} vs {a:?}");
        }
        // Placement offsets shift, fades shape, gains scale.
        let mut c = vec![0.0f32; 8];
        mix_into(&mut c, &tone, 2, 0.5, 0, 0);
        assert_eq!(&c[..4], &[0.0, 0.0, 0.0, 0.0]);
        assert!((c[4] - 1000.0 / 32768.0 * 0.5).abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn worker_order_covers_every_frame_exactly_once() {
        // T015: any worker count visits every frame once; one worker
        // keeps canonical order; two workers stride round-robin.
        for (total, workers) in [(0usize, 1), (1, 1), (5, 1), (5, 2), (5, 3), (60, 8), (10, 64), (7, 100)] {
            let mut got = worker_order(total, workers);
            got.sort_unstable();
            assert_eq!(got, (0..total as u64).collect::<Vec<_>>(), "total={total} workers={workers}");
        }
        assert_eq!(worker_order(6, 1), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(worker_order(6, 2), vec![0, 2, 4, 1, 3, 5]);
        assert_eq!(worker_order(0, 4), Vec::<u64>::new());
    }

    /// Box-only preparation with no media inputs (no FFmpeg needed).
    fn box_prep(clips: Vec<notation::model::Clip>) -> Prepared {
        use crate::motion_plan::plan_motions;
        use notation::model::{Composition, Fps, Rgb};
        let comp = Composition {
            name: "t".to_string(),
            width: 64,
            height: 48,
            duration: 1.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips,
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
        };
        let motions = plan_motions(&comp, 30);
        let frames = comp.fps.frame_count(comp.duration) as usize;
        let layouts = vec![None; comp.clips.len()];
        Prepared {
            comp,
            motions,
            frames,
            fonts: layout::Fonts::new(),
            project_family: None,
            layouts,
            bitmap_fallback: true,
            videos: Vec::new(),
            ranges: Vec::new(),
            images: Vec::new(),
            raster: RasterCache::new(),
        }
    }

    fn red_box() -> notation::model::Clip {
        use notation::model::{BoxClip, Rgb};
        notation::model::Clip::Box(BoxClip {
            start: 0.0,
            duration: 1.0,
            x: 4.0,
            y: 4.0,
            width: 20.0,
            height: 20.0,
            fill: Rgb { r: 255, g: 0, b: 0 },
            radius: 2.0,
            border_width: 1.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            opacity: 1.0,
            order: 1,
            line: 1,
            column: 1,
        })
    }

    #[test]
    fn parallel_batches_match_serial_bytes_exactly() {
        // Sprint 3 T003: any worker count draws byte-identical pictures,
        // and a solo frame equals the same frame from a full batch —
        // exactly on CPU, deterministically on the GPU path too.
        let prep = box_prep(vec![red_box()]);
        let frames: Vec<u64> = (0..12).collect();
        let serial = draw_batch_parallel_with_workers(&prep, Backend::Cpu, &frames, 1).unwrap();
        assert_eq!(serial.len(), 12);
        for w in [2, 3, 4, 8, 64] {
            let got = draw_batch_parallel_with_workers(&prep, Backend::Cpu, &frames, w).unwrap();
            assert_eq!(got, serial, "worker count {w} must draw identical bytes");
        }
        // Solo frame equality through the same parallel path.
        let solo = draw_batch_parallel_with_workers(&prep, Backend::Cpu, &[7], 4).unwrap();
        let full7 = serial.iter().find(|(k, _)| *k == 7).unwrap();
        assert_eq!(solo[0].1, full7.1, "solo frame 7 must equal full-render frame 7");
        // Frame order is ascending whatever the visit order.
        let order: Vec<u64> = serial.iter().map(|(k, _)| *k).collect();
        assert_eq!(order, frames);
        // The GPU traversal is deterministic across worker counts too.
        let g1 = draw_batch_parallel_with_workers(&prep, Backend::Gpu, &frames, 1).unwrap();
        let g4 = draw_batch_parallel_with_workers(&prep, Backend::Gpu, &frames, 4).unwrap();
        assert_eq!(g1, g4, "gpu batches must be deterministic");
    }

    #[test]
    fn parallel_batches_name_worker_failures() {
        // Sprint 3 T003: a worker failure aborts the batch with the
        // underlying reason named. A video clip with no probed source
        // fails every frame that draws it, serially or in parallel.
        use notation::model::{Clip, Fit, VideoClip};
        let prep = box_prep(vec![Clip::Video(VideoClip {
            path: "clip.mp4".to_string(),
            start: 0.0,
            duration: 1.0,
            fit: Fit::Contain,
            order: 0,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            line: 1,
            column: 1,
        })]);
        for w in [1, 2, 4] {
            let err =
                draw_batch_parallel_with_workers(&prep, Backend::Cpu, &[0, 1, 2], w).expect_err(
                    "a frame no probe covers must fail at worker count {w}",
                );
            assert!(err.contains("was not probed"), "{err}");
        }
    }

    #[test]
    fn worker_count_reads_its_env_and_clamps() {
        // T015/T004: FRAMESMITH_WORKERS 1..=64 wins, anything else is the
        // machine default. One test owns the variable so parallel tests
        // cannot race it.
        let saved = std::env::var("FRAMESMITH_WORKERS").ok();
        let fallback = default_workers();
        assert!((1..=64).contains(&fallback), "{fallback}");
        std::env::remove_var("FRAMESMITH_WORKERS");
        assert_eq!(worker_count(), fallback, "unset means the machine default");
        for good in ["1", "2", "8", "64", " 4 "] {
            std::env::set_var("FRAMESMITH_WORKERS", good);
            assert_eq!(worker_count(), good.trim().parse::<usize>().unwrap(), "{good:?} wins");
        }
        for bad in ["0", "65", "100", "abc", "", "-3", "2.5"] {
            std::env::set_var("FRAMESMITH_WORKERS", bad);
            assert_eq!(worker_count(), fallback, "{bad:?} falls back to the machine default");
        }
        match saved {
            Some(v) => std::env::set_var("FRAMESMITH_WORKERS", v),
            None => std::env::remove_var("FRAMESMITH_WORKERS"),
        }
    }

    #[test]
    fn canvas_fill_matches_the_per_pixel_loop() {
        // T004: the doubling fill writes exactly what the old per-triplet
        // loop wrote, on even, odd, and tiny canvases.
        for (w, h) in [(32u32, 32u32), (1920, 1080), (7, 5), (1, 1)] {
            for (r, g, b) in [(0u8, 0, 0), (255, 128, 7), (10, 20, 30)] {
                let mut fast = vec![0xA5u8; w as usize * h as usize * 3];
                fill_canvas(&mut fast, r, g, b);
                let mut slow = vec![0xA5u8; w as usize * h as usize * 3];
                for px in slow.chunks_exact_mut(3) {
                    px[0] = r;
                    px[1] = g;
                    px[2] = b;
                }
                assert_eq!(fast, slow, "{w}x{h} rgb({r},{g},{b})");
            }
        }
        let mut empty: Vec<u8> = Vec::new();
        fill_canvas(&mut empty, 1, 2, 3);
        assert!(empty.is_empty());
    }

    #[test]
    fn cancel_flag_roundtrip_and_error_shape() {
        // Sprint 3 T005: install clears a stale request, request sets the
        // flag, the error carries the sentinel prefix, and the exit code
        // is 130. One test owns the process-global flag throughout.
        crate::cancel::install();
        assert!(!crate::cancel::is_cancelled());
        crate::cancel::request();
        assert!(crate::cancel::is_cancelled());
        let e = crate::cancel::error("frames");
        assert!(crate::cancel::is_cancel_error(&e), "{e}");
        assert!(e.contains("frames"), "{e}");
        assert_eq!(crate::cancel::EXIT_CODE, 130);
        assert!(!crate::cancel::is_cancel_error("render failed: boom"));
        crate::cancel::clear();
        assert!(!crate::cancel::is_cancelled());
    }

    #[test]
    fn remove_staged_cleans_and_names_leftovers() {
        // Sprint 3 T005: staged temp plus extras go; missing files are
        // fine; a path that cannot be removed is reported, never hidden.
        let dir = std::env::temp_dir().join(format!(
            "framesmith-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.mp4");
        let mut tmp_name = out.as_os_str().to_owned();
        tmp_name.push(".framesmith-part");
        let tmp = PathBuf::from(tmp_name);
        std::fs::write(&tmp, b"partial").unwrap();
        let extra = dir.join("mix.pcm");
        std::fs::write(&extra, b"mix").unwrap();
        let left = crate::cancel::remove_staged(&out, std::slice::from_ref(&extra));
        assert!(left.is_empty(), "{left:?}");
        assert!(!tmp.exists());
        assert!(!extra.exists());
        // Missing files are not leftovers.
        let left = crate::cancel::remove_staged(&out, std::slice::from_ref(&extra));
        assert!(left.is_empty(), "{left:?}");
        // A directory cannot be removed with remove_file: it is named.
        let undel = dir.join("locked");
        std::fs::create_dir_all(&undel).unwrap();
        let left = crate::cancel::remove_staged(&out, std::slice::from_ref(&undel));
        assert_eq!(left, vec![undel.clone()], "{left:?}");
        let report = cancel_report("frames", &left);
        assert!(crate::cancel::is_cancel_error(&report), "{report}");
        assert!(report.contains(&undel.display().to_string()), "{report}");
        assert!(!report.contains("exit 130"), "{report}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn db_gain_ceiling_and_silence() {
        // Sprint 3 T009/T010: silence is exact zero, +12 dB is the
        // ceiling, anything above is refused, never limited silently.
        assert_eq!(db_to_gain(f64::NEG_INFINITY).unwrap(), 0.0);
        assert!((db_to_gain(0.0).unwrap() - 1.0).abs() < 1e-6);
        assert!((db_to_gain(12.0).unwrap() - 3.9810717).abs() < 1e-4);
        assert!((db_to_gain(-6.0).unwrap() - 0.5011872).abs() < 1e-4);
        for bad in [12.5, 13.0, 20.0, f64::INFINITY, f64::NAN] {
            assert!(db_to_gain(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn placed_mix_applies_gain_and_fades() {
        // Sprint 3 T009/T010: one constant tone placed with unity gain
        // keeps its level, silence gain zeroes it, and a full fade-in
        // over the window starts near silence and reaches unity.
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(&mut acc, &tone, 4800, 0.0, 0.1, 1.0, 0.0, 0.0);
        assert!((acc[2 * 2400] - 10000.0 / 32768.0).abs() < 1e-6);
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(&mut acc, &tone, 4800, 0.0, 0.1, 0.0, 0.0, 0.0);
        assert!(acc.iter().all(|&x| x == 0.0));
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(&mut acc, &tone, 4800, 0.0, 0.1, 1.0, 0.1, 0.0);
        assert!(acc[0].abs() < acc[2 * 4799].abs() * 0.05, "{} vs {}", acc[0], acc[2 * 4799]);
        assert!((acc[2 * 4799] - 10000.0 / 32768.0).abs() < 1e-6);
    }
}
