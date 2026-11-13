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
//!   naming the file when the source is corrupt, has no decodable
//!   video stream, or has undecodable audio; a clip window that
//!   outruns its source loops its window (or holds the last frame),
//!   never refuses;
//! * draws each output frame on the CPU — background fill, video planes
//!   (`contain` / `cover` / `fill`, exact rational-time picture, text in
//!   document order only inside each clip's half-open window) — and pipes
//!   raw RGB24 frames to FFmpeg, which writes one MP4 with H.264 yuv420p
//!   video and AAC stereo 48 kHz audio;
//! * mixes every audible source — each `audio` track with its trim,
//!   volume, and linear fades, plus each `video` clip's sound stream
//!   from its source window (`trim-start`/`trim-end`) at its `rate`
//!   (plainly resampled, pitch follows speed) with the clip's volume
//!   and fades — sample-accurately into one 48 kHz stereo mix; a
//!   composition with no audible input renders with the encoder's
//!   silent default;
//! * draws picture exactly as before: video sound enters the mix, never
//!   the blitter, so picture bytes are unchanged by the mix;
//! * encodes to a temporary file and renames on success, so an encode or
//!   decode failure stops the render, reports the FFmpeg stage, and never
//!   presents a wrong or truncated MP4 as success.

use crate::blur::Premult;
use crate::frame::{FrameDescription, FrameItem};
use crate::motion_plan::{plan_motions, snap_fps, ClipMotion};
use crate::turn::Turn;
use glyph::font::Face;
use notation::model::{
    AudioClip, Clip, Composition, FillPaint, Fit, GradientStop, LutTable, ResolvedGrade, Rgb,
    TransitionKind, VideoClip,
};
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
    /// Source transfer (Sprint 12 outputs, CAP-004): PQ when the
    /// container names SMPTE 2084, HLG for ARIB STD-B67, SDR
    /// otherwise. The HDR decision consumes this per source.
    pub(crate) transfer: crate::outputs::HdrTransfer,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Source duration in seconds when the container reports one.
    pub(crate) duration: Option<f64>,
    pub(crate) has_audio: bool,
    /// Loop the source window for the whole clip window (Sprint 5
    /// T003); without it an overrunning window holds the last frame.
    pub(crate) loop_clip: bool,
    /// Container rotation in degrees, normalized to [0, 360) (Sprint 5
    /// T005): 0 means render the stored pixels as-is; 90/180/270 mean
    /// the decoder auto-rotates and `width`/`height` below are already
    /// the oriented (player-visible) dimensions. Exotic angles stay 0
    /// here and render stored, exactly like the decoder default.
    pub(crate) rotation: f64,
    /// Effective source window opening, in source seconds.
    pub(crate) win_start: f64,
    /// Effective source window length, in source seconds; None only
    /// when neither a declared `trim-end` nor a probed duration is
    /// known (then looping degrades to hold).
    pub(crate) win_len: Option<f64>,
}

/// One clip window decoded to raw pictures with presentation timestamps.
///
/// `Clone` only serves the footage frame cache's tests; the render path
/// moves ranges without cloning.
#[derive(Debug, Clone)]
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

/// One decoded Lottie animation ready to rasterize (Sprint 21
/// adapters, T001): the file decoded once before the first frame
/// (fail-closed, naming the file), then rasterized per output frame
/// at its loop-mapped instant. Decoding once keeps reruns
/// byte-identical without re-reading the file per frame.
#[derive(Debug, Clone)]
pub(crate) struct PreparedLottie {
    /// Global document id of the owning clip.
    pub(crate) clip_idx: usize,
    /// The decoded animation: timing, markers, loop default, layers.
    pub(crate) doc: lottie::LottieDoc,
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
/// One transitively nested composition of a render (Sprint 6 T007):
/// its own clips and motions plus its global document base into the
/// unified prepared media (layouts, videos, ranges). The root
/// composition lives in [`Prepared`] proper; the tree holds only the
/// nested ones, in breadth-first document order.
pub(crate) struct TreeComp {
    pub comp: Composition,
    pub motions: Vec<ClipMotion>,
    pub base: usize,
}

/// Pre-shaped layouts for frame-varying text (Sprint 7 motion,
/// CAP-009 plus T006): every distinct `(string, font size,
/// letter-spacing)` triple a typewriter reveal, count range, or
/// animated metric shows at a drawn instant, shaped once at
/// preparation. Lookups are read-only during drawing, so any worker
/// count draws identical bytes, and re-renders shape the same strings
/// again.
#[derive(Debug, Default)]
pub(crate) struct DynLayouts {
    index: std::collections::HashMap<(usize, String, u64, u64), usize>,
    blocks: Vec<layout::Block>,
}

/// Size/spacing key of a dynamic triple: exact font size bits, and the
/// letter-spacing bits (`u64::MAX` sentinel when the clip leaves the
/// font default in place).
pub(crate) fn metrics_key(size: f64, spacing: Option<f64>) -> (u64, u64) {
    (
        size.to_bits(),
        spacing.map(f64::to_bits).unwrap_or(u64::MAX),
    )
}

impl DynLayouts {
    /// The pre-shaped block for global document id `doc` showing
    /// `value` at `size`/`spacing`, when the clip is frame-varying;
    /// `None` for still text (drawn from the static layouts) and for
    /// the empty string (nothing to draw).
    pub(crate) fn get(
        &self,
        doc: usize,
        value: &str,
        size: f64,
        spacing: Option<f64>,
    ) -> Option<&layout::Block> {
        if value.is_empty() {
            return None;
        }
        let (sb, lb) = metrics_key(size, spacing);
        self.index
            .get(&(doc, value.to_string(), sb, lb))
            .and_then(|&i| self.blocks.get(i))
    }

    fn insert(
        &mut self,
        doc: usize,
        value: String,
        size: f64,
        spacing: Option<f64>,
        block: layout::Block,
    ) {
        let (sb, lb) = metrics_key(size, spacing);
        let at = self.blocks.len();
        self.blocks.push(block);
        self.index.insert((doc, value, sb, lb), at);
    }

    fn contains(&self, doc: usize, value: &str, size: f64, spacing: Option<f64>) -> bool {
        if value.is_empty() {
            return true;
        }
        let (sb, lb) = metrics_key(size, spacing);
        self.index.contains_key(&(doc, value.to_string(), sb, lb))
    }
}

pub struct Prepared {
    pub(crate) comp: Composition,
    pub(crate) motions: Vec<ClipMotion>,
    pub(crate) frames: usize,
    pub(crate) fonts: layout::Fonts,
    pub(crate) project_family: Option<String>,
    pub(crate) layouts: Vec<Option<layout::Block>>,
    /// Pre-shaped blocks for frame-varying text (Sprint 7 motion):
    /// typewriter reveals, count ranges, and animated font metrics draw
    /// these, keyed by document id, string, font size, and letter
    /// spacing. Empty for still-only projects.
    pub(crate) dyn_layouts: DynLayouts,
    /// Pre-shaped caption word blocks (Sprint 22 captions, T001):
    /// every word of every `captions` clip in its resolved paint
    /// (track style plus live overrides), keyed by document id, cue,
    /// and word. Empty for caption-free projects, which prepare
    /// exactly as before.
    pub(crate) caption_layouts: crate::captions::CaptionLayouts,
    pub(crate) bitmap_fallback: bool,
    pub(crate) videos: Vec<ProbedVideo>,
    pub(crate) ranges: Vec<DecodedRange>,
    pub(crate) images: Vec<DecodedImage>,
    /// Decoded Lottie animations (Sprint 21 adapters, T001): one per
    /// `lottie` clip at its global document id, decoded before the
    /// first frame, rasterized once per drawn frame.
    pub(crate) lotties: Vec<PreparedLottie>,
    pub(crate) raster: RasterCache,
    /// Shared path-data parse cache (Sprint 8 look, T005): every frame
    /// reuses the cached contours and recomputes only numbers. Shared
    /// across workers through the draw view; interior locking keeps
    /// any worker count byte-identical.
    pub(crate) paths: crate::path_paint::PathCache,
    /// Nested compositions reachable from the root (Sprint 6 T007),
    /// in breadth-first document order. Empty for nest-free projects;
    /// `layouts`, `videos`, `ranges`, and `images` above already
    /// include every tree composition's media at the global document
    /// ids [`TreeComp::base`] records.
    pub(crate) tree: Vec<TreeComp>,
    /// Footage frame-cache hits and misses for this preparation (Sprint
    /// 5 T006): one lookup per decoded clip window. Read-only after
    /// [`prepare_frames`]; drawing never touches it, so it cannot
    /// change the picture at any worker count.
    pub cache_stats: crate::footage_cache::CacheStats,
    /// Composition time of output frame 0 (Sprint 6 T008): zero for a
    /// full render, the `--start` instant for a sub-range render. Every
    /// drawn frame adds its own frame interval exactly, so ranged frames
    /// equal the full render's frames at the same instants.
    pub(crate) range_start: timebase::Time,
    /// Output media length in seconds (Sprint 6 T008): the composition
    /// duration for a full render, `end - start` for a sub-range. The
    /// encoder and the success line use this, never the composition.
    pub(crate) out_duration: f64,
    /// Project folder the composition validated against (Sprint 8 look,
    /// T003): image fills stat their source here on every frame, so a
    /// file deleted mid-render refuses with its path named.
    pub(crate) folder: PathBuf,
}

impl Prepared {
    /// Borrow the validated composition (GPU traversal input).
    pub(crate) fn composition(&self) -> &Composition {
        &self.comp
    }

    /// The render tree view (Sprint 6 T007): the root plus every nested
    /// composition with per-composition motions and global bases.
    pub(crate) fn nest_view(&self) -> crate::frame::NestView<'_> {
        let mut comps = Vec::with_capacity(1 + self.tree.len());
        let mut motions = Vec::with_capacity(1 + self.tree.len());
        let mut bases = Vec::with_capacity(1 + self.tree.len());
        comps.push(&self.comp);
        motions.push(self.motions.as_slice());
        bases.push(0);
        for t in &self.tree {
            comps.push(&t.comp);
            motions.push(t.motions.as_slice());
            bases.push(t.base);
        }
        crate::frame::NestView {
            comps,
            motions,
            bases,
        }
    }

    /// HDR sources in this preparation (Sprint 12 outputs, CAP-004):
    /// every probed video's path with its transfer, in document
    /// order (root first, then nested). SDR-only preparations return
    /// an empty list. The HDR decision consumes this: the composition
    /// carries HDR when any rendered source does.
    pub fn hdr_sources(&self) -> Vec<(std::path::PathBuf, crate::outputs::HdrTransfer)> {
        self.videos
            .iter()
            .filter(|v| v.transfer != crate::outputs::HdrTransfer::Sdr)
            .map(|v| (v.path.clone(), v.transfer))
            .collect()
    }

    /// Effective transfer of this preparation (Sprint 12 outputs,
    /// CAP-004): PQ when any source carries PQ, else HLG when any
    /// source carries HLG, else SDR. PQ wins a mixed composition so
    /// the decision stays deterministic.
    pub fn composition_transfer(&self) -> crate::outputs::HdrTransfer {
        let mut seen_hlg = false;
        for v in &self.videos {
            match v.transfer {
                crate::outputs::HdrTransfer::Pq => return crate::outputs::HdrTransfer::Pq,
                crate::outputs::HdrTransfer::Hlg => seen_hlg = true,
                crate::outputs::HdrTransfer::Sdr => {}
            }
        }
        if seen_hlg {
            crate::outputs::HdrTransfer::Hlg
        } else {
            crate::outputs::HdrTransfer::Sdr
        }
    }

    /// Output frame count of the prepared composition.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Canvas dimensions `(width, height)` of the prepared composition.
    pub fn canvas(&self) -> (u32, u32) {
        (self.comp.width, self.comp.height)
    }

    /// Ride expression-driven geometry on the internal multiplier
    /// (Sprint 12 outputs, T002): expression strings cannot scale
    /// statically, so pixel-valued results scale at draw time. 1.0 on
    /// the identity path, where expression-free and expression-driven
    /// motion agree exactly. Covers the root and every nested
    /// composition.
    pub fn scale_expr_results(&mut self, k: f64) {
        if k == 1.0 {
            return;
        }
        for m in &mut self.motions {
            m.set_expr_scale(k);
        }
        for t in &mut self.tree {
            for m in &mut t.motions {
                m.set_expr_scale(k);
            }
        }
    }

    /// Draw one output frame alone (T015): the same description and the
    /// same blitter as the full render, so frame N alone equals frame N
    /// from a full render exactly on the CPU renderer. The frame is
    /// returned as raw RGB24 bytes (`width * height * 3`).
    pub fn cpu_frame(&mut self, frame: u64) -> Result<Vec<u8>, String> {
        let view = self.nest_view();
        let desc = crate::frame::describe_nested_at(
            &view,
            0,
            frame,
            self.range_start + crate::motion_plan::frame_time(&self.comp.fps, frame),
        );
        let mut out = vec![0u8; self.comp.width as usize * self.comp.height as usize * 3];
        draw_description(
            &desc,
            &mut self.raster,
            &self.layouts,
            &self.dyn_layouts,
            &self.caption_layouts,
            self.bitmap_fallback,
            &self.videos,
            &self.ranges,
            &self.images,
            &self.lotties,
            &self.paths,
            self.folder.as_path(),
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

/// Per-render choices from the CLI flags (Sprint 3 T001/T002, plus
/// the Sprint 12 output destination).
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
    /// `--format` destination (Sprint 12 outputs, T001): `mp4` when no
    /// format is named, rendering exactly as before.
    pub format: crate::outputs::OutputFormat,
    /// `--prores-profile` choice for ProRes MOVs (Sprint 12 outputs,
    /// T001): `standard` when no profile is named.
    pub prores_profile: crate::outputs::ProresProfile,
    /// `--resolution` preset (Sprint 12 outputs, T002): `source`
    /// when no resolution is named, keeping the canvas size.
    pub resolution: crate::scale::Resolution,
    /// `--supersample` multiple (Sprint 12 outputs, T002): 1 when
    /// none is named, the identity path with no scaling work.
    pub supersample: u8,
    /// `--encoder` choice (Sprint 12 outputs, T003): `software` when
    /// no encoder is named, so default renders encode exactly as
    /// before on every machine.
    pub encoder_choice: crate::outputs::EncoderChoice,
    /// Effective hardware use (Sprint 12 outputs, T003; Sprint 25,
    /// T002): true only when the caller resolved `auto` or an
    /// explicit hardware choice against a host FFmpeg that provides
    /// the destination's hardware encoder.
    pub hardware: bool,
    /// Resolved hardware codec (Sprint 25, T002): the FFmpeg encoder
    /// `auto`/explicit resolved to (for example `h264_nvenc`), set
    /// by the caller alongside [`Self::hardware`]. `None` keeps the
    /// legacy VideoToolbox mapping for library callers.
    pub hw_codec: Option<&'static str>,
    /// `--keyint` GOP length (Sprint 12 outputs, T003): None emits no
    /// `-g`, keeping default bytes identical.
    pub keyint: Option<u32>,
    /// `--maxrate` peak cap (Sprint 12 outputs, T003): None emits no
    /// `-maxrate`/`-bufsize`, keeping default bytes identical.
    pub maxrate: Option<String>,
    /// Requested `--hdr` mode (Sprint 12 outputs, T004): `auto` when
    /// no HDR flag is passed, so default renders decide from probes.
    pub hdr: crate::outputs::HdrMode,
    /// Effective transfer of this render (Sprint 12 outputs, T004):
    /// resolved from the mode plus the probed sources after
    /// preparation (`Sdr` until the caller resolves it). `Sdr`
    /// emits the historical argv exactly.
    pub hdr_transfer: crate::outputs::HdrTransfer,
    /// True when an HDR source renders to an SDR-only destination
    /// under `auto` (Sprint 12 outputs, T004): the drawn SDR pictures
    /// are the tone map, and the report notes the mapping.
    pub hdr_tone_mapped: bool,
    /// Provenance stamped on the delivered output (Sprint 12 outputs,
    /// T005): None keeps the historical argv exactly (library callers
    /// and unit fixtures); the CLI always sets it, so every CLI render
    /// carries the tool name, the version, and the inputs digest.
    pub provenance: Option<crate::outputs::Provenance>,
    /// GIF loop count (Sprint 18, T006): `0` means forever, the
    /// historical default every existing caller keeps.
    pub gif_loop: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            backend: Backend::Cpu,
            preset: Preset::Standard,
            crf_override: None,
            video_bitrate: None,
            format: crate::outputs::OutputFormat::Mp4,
            prores_profile: crate::outputs::ProresProfile::Standard,
            resolution: crate::scale::Resolution::Source,
            supersample: 1,
            encoder_choice: crate::outputs::EncoderChoice::Software,
            hardware: false,
            hw_codec: None,
            keyint: None,
            maxrate: None,
            hdr: crate::outputs::HdrMode::Auto,
            hdr_transfer: crate::outputs::HdrTransfer::Sdr,
            hdr_tone_mapped: false,
            provenance: None,
            gif_loop: 0,
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
            encoder_choice: self.encoder_choice,
            hardware: self.hardware,
            hw_codec: self.hw_codec,
            keyint: self.keyint,
            maxrate: self.maxrate.clone(),
            hdr: self.hdr,
            hdr_transfer: self.hdr_transfer,
            provenance: self.provenance.clone(),
            gif_loop: self.gif_loop,
        }
    }

    /// Scaling work of one render (Sprint 12 outputs, T002): the
    /// output size, the internal size, and the multipliers from the
    /// active `--resolution` / `--supersample` flags and the canvas.
    /// `source` at 1x is the identity plan (no scaling work).
    pub fn scale_plan(&self, canvas_w: u32, canvas_h: u32) -> crate::scale::ScalePlan {
        crate::scale::ScalePlan::from_flags(self.resolution, self.supersample, canvas_w, canvas_h)
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

    /// Encoder, rate-control, and HDR note for the frames-stage line
    /// and the success line (Sprint 12 outputs, T003/T004): `None` on
    /// the default path (no `--encoder`/`--keyint`/`--maxrate` flags,
    /// software encoding, and the `auto` HDR mode resolving to SDR),
    /// so the pinned lines stay byte-identical. Otherwise the
    /// requested choice, the effective hardware codec, the
    /// interval/cap that paced the stream, and the HDR decision the
    /// probe drove.
    pub fn announce_extra(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if self.encoder_choice != crate::outputs::EncoderChoice::Software {
            parts.push(format!("encoder={}", self.encoder_choice.name()));
        }
        if self.hardware {
            let codec = self
                .hw_codec
                .or_else(|| {
                    crate::outputs::hardware_encoder_for_choice(self.format, self.encoder_choice)
                })
                .or_else(|| crate::outputs::hardware_encoder_for(self.format))
                .unwrap_or("software");
            parts.push(format!("hw={codec}"));
        }
        if let Some(g) = self.keyint {
            parts.push(format!("keyint={g}"));
        }
        if let Some(m) = &self.maxrate {
            parts.push(format!("maxrate={m}"));
        }
        // HDR rides only when the invocation or the probe says so:
        // a bare `auto` resolving to SDR stays silent.
        if self.hdr != crate::outputs::HdrMode::Auto {
            parts.push(format!("hdr={}", self.hdr.name()));
        }
        if self.hdr_transfer != crate::outputs::HdrTransfer::Sdr {
            parts.push(format!(
                "transfer={} (HEVC Main10 BT.2020)",
                self.hdr_transfer.name()
            ));
        }
        if self.hdr_tone_mapped {
            parts.push("tone-mapped HDR to SDR".to_string());
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }
}

/// Encoder numbers resolved for one render (preset plus overrides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEncoder {
    pub crf: u8,
    pub x264_preset: Option<&'static str>,
    pub video_bitrate: Option<String>,
    pub from_preset: Preset,
    /// Requested `--encoder` choice (Sprint 12 outputs, T003).
    pub encoder_choice: crate::outputs::EncoderChoice,
    /// Effective hardware use (Sprint 12 outputs, T003; Sprint 25,
    /// T002): the caller resolved this against the host FFmpeg
    /// before the first frame.
    pub hardware: bool,
    /// Resolved hardware codec (Sprint 25, T002): `None` keeps the
    /// legacy VideoToolbox mapping for library callers.
    pub hw_codec: Option<&'static str>,
    /// `--keyint` GOP length, if named (Sprint 12 outputs, T003).
    pub keyint: Option<u32>,
    /// `--maxrate` peak cap, if named (Sprint 12 outputs, T003).
    pub maxrate: Option<String>,
    /// Requested `--hdr` mode (Sprint 12 outputs, T004).
    pub hdr: crate::outputs::HdrMode,
    /// Effective transfer of this render (Sprint 12 outputs, T004).
    pub hdr_transfer: crate::outputs::HdrTransfer,
    /// Provenance stamp for this render (Sprint 12 outputs, T005):
    /// None emits no metadata, keeping the historical argv exactly.
    pub provenance: Option<crate::outputs::Provenance>,
    /// GIF loop count (Sprint 18, T006): `0` means forever, the
    /// historical default every existing caller keeps.
    pub gif_loop: u32,
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
/// Resolve the render tree in breadth-first document order (Sprint 6
/// T007): the root first, then every transitively nested composition.
/// Targets resolve by composition `name` or `id` against `all` (plus
/// the root itself when it is not listed there). An unknown target or
/// a chain past 8 edges refuses before the first frame; valid projects
/// never hit either, because validation gates them first.
fn nest_render_order<'a>(
    comp: &'a Composition,
    all: &'a [Composition],
) -> Result<Vec<&'a Composition>, String> {
    fn find<'a>(
        comp: &'a Composition,
        all: &'a [Composition],
        target: &str,
    ) -> Option<&'a Composition> {
        if comp.name == target || comp.id.as_deref() == Some(target) {
            return Some(comp);
        }
        all.iter()
            .find(|c| c.name == target || c.id.as_deref() == Some(target))
    }
    let mut order: Vec<&Composition> = vec![comp];
    let mut depths: Vec<usize> = vec![0];
    let mut parents: Vec<usize> = vec![usize::MAX];
    let mut cursor = 0;
    while cursor < order.len() {
        let depth = depths[cursor];
        if depth >= notation::model::MAX_NEST_DEPTH {
            // Do not expand past the limit: a deeper chain is refused
            // below with the chain named, never rendered.
            cursor += 1;
            continue;
        }
        let clips: Vec<(String, usize, usize)> = order[cursor]
            .clips
            .iter()
            .filter_map(|c| match c {
                Clip::Nest(n) => Some((n.target.clone(), n.line, n.column)),
                _ => None,
            })
            .collect();
        for (target, line, column) in clips {
            let Some(hit) = find(comp, all, &target) else {
                return Err(format!(
                    "render refused: nest {target:?} names no composition in this project (validation stage); no MP4 was written (at {line}:{column})"
                ));
            };
            if order.iter().any(|c| c.name == hit.name) {
                continue;
            }
            // A use deeper than the limit refuses naming the chain and
            // the limit, before the first frame.
            if depth + 1 > notation::model::MAX_NEST_DEPTH {
                let mut chain: Vec<String> = Vec::new();
                let mut at = cursor;
                let mut rev: Vec<&str> = vec![hit.name.as_str()];
                loop {
                    rev.push(order[at].name.as_str());
                    if parents[at] == usize::MAX {
                        break;
                    }
                    at = parents[at];
                }
                rev.reverse();
                chain.extend(rev.iter().map(|s| format!("{s:?}")));
                return Err(format!(
                    "render refused: `nest` chain {} nests {} deep, past the {}-level limit; flatten the nesting to at most {} levels (validation stage); no MP4 was written",
                    chain.join(" -> "),
                    depth + 1,
                    notation::model::MAX_NEST_DEPTH,
                    notation::model::MAX_NEST_DEPTH,
                ));
            }
            parents.push(cursor);
            depths.push(depth + 1);
            order.push(hit);
        }
        cursor += 1;
    }
    Ok(order)
}

/// Register one composition's declared project fonts (Sprint 6 T007):
/// every declared font registers under its own family so an explicit
/// `font-family` can select any of them; the first declared font stays
/// the default family. The root registers into the preparation's font
/// set; nested compositions register into a throwaway set (their
/// shaped blocks carry their own face bytes).
fn register_comp_fonts(
    comp: &Composition,
    folder: &Path,
    fonts: &mut layout::Fonts,
    project_family: &mut Option<String>,
) -> Result<(), String> {
    // Text faces: first declared project font, else the host system
    // Text engine: parley shaping over the project font plus the
    // machine's fonts, with per-character fallback and FFmpeg-decoded
    // emoji. The project font (first declared `font`) names its own
    // family; without one the generic families resolve on the machine.
    // Origins are always named in the success line.
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
                layout::FontError::Unsupported(kind) => format!(
                    "{kind} is not supported; use a TrueType (.ttf) font with TrueType outlines"
                ),
                layout::FontError::NotAFont => "the file is not a recognized font".to_string(),
                layout::FontError::Collection(reason) => reason,
                layout::FontError::Damaged(reason) => reason,
            };
            format!(
                "render refused: font file {:?} {reason}; no MP4 was written",
                decl.path
            )
        })?;
        if index == 0 {
            *project_family = Some(family);
        }
    }
    Ok(())
}

/// Shape one composition's text layouts (Sprint 6 T007): one parley
/// block per text clip, shaped once up front (text is static within a
/// clip). A layout the machine cannot serve refuses before the first
/// frame; with no font anywhere the announced bitmap fallback draws
/// instead of a blank. Returns the per-clip blocks plus whether this
/// composition falls back to the bitmap face.
/// The parley style of one text clip (shared by the static layout pass
/// and the frame-varying text pass): explicit `font-family` wins in
/// written try order, else the project family, else the system style
/// faces. A list nothing resolves refuses here instead of silently
/// falling back.
fn text_layout_style(
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    tc: &notation::model::TextClip,
) -> Result<layout::Style, String> {
    text_layout_style_with(fonts, project_family, tc, tc.size, tc.letter_spacing)
}

/// Style of one text clip with overridden font metrics (Sprint 7
/// motion, T006): animated `font-size`/`letter-spacing` re-shape the
/// text at its evaluated metrics. `line_height` and wrapping stay
/// authored (layout reaction to animated sizes belongs to T011, so
/// they do not follow here).
fn text_layout_style_with(
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    tc: &notation::model::TextClip,
    size: f64,
    letter_spacing: Option<f64>,
) -> Result<layout::Style, String> {
    // The style family: the project family when declared, else the
    // system variant file for the clip style (`Arial Bold`...), else
    // the generic fallback inside layout. Names the same basenames
    // as the old per-style candidate files.
    let bold = matches!(
        tc.style,
        notation::model::FontStyle::Bold | notation::model::FontStyle::BoldItalic
    );
    let italic = matches!(
        tc.style,
        notation::model::FontStyle::Italic | notation::model::FontStyle::BoldItalic
    );
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
        None => match project_family {
            Some(family) => vec![family.clone()],
            None => fonts
                .resolve_system_style(bold, italic)
                .into_iter()
                .collect(),
        },
    };
    Ok(layout::Style {
        families,
        size: size as f32,
        weight: if bold { 700.0 } else { 400.0 },
        italic,
        align: match tc.align {
            notation::model::Align::Left => layout::Align::Left,
            notation::model::Align::Center => layout::Align::Center,
            notation::model::Align::Right => layout::Align::Right,
        },
        colour: [tc.color.r, tc.color.g, tc.color.b, 255],
        line_height: tc.line_height.map(|v| v as f32),
        letter_spacing: letter_spacing.map(|v| v as f32),
    })
}

/// Shape one string with one text clip's style (Sprint 7 motion,
/// CAP-009): the static pass shapes the authored value, the
/// frame-varying pass shapes each distinct reveal/count string. `None`
/// means the bitmap fallback (no family anywhere); anything else
/// unshapable refuses before the first frame.
fn shape_text_string(
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    tc: &notation::model::TextClip,
    text: &str,
    ffmpeg_bin: &Path,
) -> Result<Option<layout::Block>, String> {
    if !tc.spans.is_empty() {
        return shape_text_spans(
            fonts,
            project_family,
            tc,
            tc.size,
            tc.letter_spacing,
            ffmpeg_bin,
        );
    }
    let style = text_layout_style(fonts, project_family, tc)?;
    let wrap = match tc.wrap {
        notation::model::Wrap::Off => None,
        notation::model::Wrap::Width(w) => Some(w as f32),
    };
    match layout::layout(fonts, text, &style, wrap, Some(ffmpeg_bin)) {
        Ok(block) => Ok(Some(block)),
        Err(layout::TextError::NoFamily) if project_family.is_none() => Ok(None),
        Err(e) => Err(format!(
            "render refused: text {text:?} cannot be laid out ({e}); no MP4 was written"
        )),
    }
}

/// One flattened styled run of a text clip with `span` children
/// (Sprint 8 look, T002): the run text with its inherited style —
/// each `None` inherits the parent span, else the clip. The clip
/// `value` (when written) flattens first in the plain clip style.
struct FlatRun {
    text: String,
    families: Option<Vec<String>>,
    size: Option<f64>,
    weight: Option<f64>,
    italic: Option<bool>,
    color: Option<notation::model::Rgb>,
    letter_spacing: Option<f64>,
    line_height: Option<f64>,
}

/// Flatten one span subtree (Sprint 8 look, T002): the span's own
/// `value` paints first in its style, then nested spans in order,
/// each inheriting the unset attributes of its parent.
fn flatten_span(span: &notation::model::TextSpan, parent: &FlatRun, out: &mut Vec<FlatRun>) {
    let cur = FlatRun {
        text: String::new(),
        families: span.font_family.clone().or_else(|| parent.families.clone()),
        size: span.font_size.or(parent.size),
        weight: span.font_weight.or(parent.weight),
        italic: span.italic.or(parent.italic),
        color: span.color.or(parent.color),
        letter_spacing: span.letter_spacing.or(parent.letter_spacing),
        line_height: span.line_height.or(parent.line_height),
    };
    if !span.value.is_empty() {
        out.push(FlatRun {
            text: span.value.clone(),
            families: cur.families.clone(),
            size: cur.size,
            weight: cur.weight,
            italic: cur.italic,
            color: cur.color,
            letter_spacing: cur.letter_spacing,
            line_height: cur.line_height,
        });
    }
    for child in &span.children {
        flatten_span(child, &cur, out);
    }
}

/// Shape one text clip carrying `span` runs (Sprint 8 look, T002):
/// the clip `value` (when written) plus every span flatten into one
/// shaped paragraph, each run painted with its own resolved family
/// try-order, size, weight, style, color, and spacing under the
/// clip's base direction. `size`/`letter_spacing` override the clip
/// defaults (Sprint 7 motion, T006); runs with explicit metrics keep
/// theirs. `None` means the bitmap fallback (no family anywhere);
/// anything else unshapable refuses before the first frame naming the
/// span.
#[allow(clippy::too_many_arguments)]
fn shape_text_spans(
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    tc: &notation::model::TextClip,
    size: f64,
    letter_spacing: Option<f64>,
    ffmpeg_bin: &Path,
) -> Result<Option<layout::Block>, String> {
    let bold = matches!(
        tc.style,
        notation::model::FontStyle::Bold | notation::model::FontStyle::BoldItalic
    );
    let italic = matches!(
        tc.style,
        notation::model::FontStyle::Italic | notation::model::FontStyle::BoldItalic
    );
    // The default families every run without an explicit list paints
    // in: the project family when declared, else the system variant
    // faces for the clip style. Resolved once so the system file
    // registers a single time no matter how many runs inherit it.
    let base_families: Vec<String> = match project_family {
        Some(family) => vec![family.clone()],
        None => fonts
            .resolve_system_style(bold, italic)
            .into_iter()
            .collect(),
    };
    let base = FlatRun {
        text: String::new(),
        families: tc.font_family.clone(),
        size: Some(size),
        weight: Some(if bold { 700.0 } else { 400.0 }),
        italic: Some(italic),
        color: Some(tc.color),
        letter_spacing,
        line_height: tc.line_height,
    };
    let mut flat: Vec<FlatRun> = Vec::new();
    if !tc.value.is_empty() {
        flat.push(FlatRun {
            text: tc.value.clone(),
            families: base.families.clone(),
            size: base.size,
            weight: base.weight,
            italic: base.italic,
            color: base.color,
            letter_spacing: base.letter_spacing,
            line_height: base.line_height,
        });
    }
    for span in &tc.spans {
        flatten_span(span, &base, &mut flat);
    }
    // Resolve each run against its effective family list: an explicit
    // list (on the span chain or the clip) nothing serves refuses
    // naming the span instead of silently falling back; inherited
    // project/system families ride the layout fallback like plain text.
    let mut runs: Vec<layout::RunStyle> = Vec::with_capacity(flat.len());
    for (i, run) in flat.iter().enumerate() {
        let (families, explicit) = match &run.families {
            Some(names) => (names.clone(), true),
            None => (base_families.clone(), false),
        };
        if explicit && !fonts.resolves_any(&families) {
            return Err(format!(
                "render refused: text span {} names unknown font-family {:?}; no MP4 was written",
                i + 1,
                families.join(", ")
            ));
        }
        runs.push(layout::RunStyle {
            text: run.text.clone(),
            families,
            size: run.size.unwrap_or(size) as f32,
            weight: run.weight.unwrap_or(if bold { 700.0 } else { 400.0 }) as f32,
            italic: run.italic.unwrap_or(italic),
            colour: run
                .color
                .map(|c| [c.r, c.g, c.b, 255])
                .unwrap_or([tc.color.r, tc.color.g, tc.color.b, 255]),
            letter_spacing: run.letter_spacing.unwrap_or(0.0).max(0.0) as f32,
            line_height: run.line_height.map(|v| v as f32),
        });
    }
    let align = match tc.align {
        notation::model::Align::Left => layout::Align::Left,
        notation::model::Align::Center => layout::Align::Center,
        notation::model::Align::Right => layout::Align::Right,
    };
    let direction = match tc.direction {
        notation::model::TextDirection::Ltr => layout::Direction::Ltr,
        notation::model::TextDirection::Rtl => layout::Direction::Rtl,
        notation::model::TextDirection::Auto => layout::Direction::Auto,
    };
    let wrap = match tc.wrap {
        notation::model::Wrap::Off => None,
        notation::model::Wrap::Width(w) => Some(w as f32),
    };
    let any_explicit = flat.iter().any(|r| r.families.is_some());
    match layout::layout_runs(fonts, &runs, align, direction, wrap, Some(ffmpeg_bin)) {
        Ok(block) => Ok(Some(block)),
        Err(layout::TextError::NoFamily) | Err(layout::TextError::NoFamilyRun { .. })
            if project_family.is_none() && !any_explicit =>
        {
            Ok(None)
        }
        Err(e) => Err(format!(
            "render refused: text with spans cannot be laid out ({e}); no MP4 was written"
        )),
    }
}

/// True when a composition animates any text clip's font metrics
/// (Sprint 7 motion, T006): a `font-size` or `letter-spacing` tween,
/// key, or set on a text clip needs per-frame pre-shaped layouts at
/// the frame's evaluated metrics.
fn text_metrics_animated(comp: &Composition) -> bool {
    let animates = |target: usize, property: &str| {
        matches!(comp.clips.get(target), Some(Clip::Text(_)))
            && (property == "font-size" || property == "letter-spacing")
    };
    comp.tweens.iter().any(|d| animates(d.target, &d.property))
        || comp.keys.iter().any(|d| animates(d.target, &d.property))
        || comp.sets.iter().any(|d| animates(d.target, &d.property))
}

/// Shape one string with overridden font metrics (Sprint 7 motion,
/// T006): animated `font-size`/`letter-spacing` re-shape at their
/// evaluated values. Otherwise identical to [`shape_text_string`].
fn shape_text_string_with(
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    tc: &notation::model::TextClip,
    text: &str,
    size: f64,
    letter_spacing: Option<f64>,
    ffmpeg_bin: &Path,
) -> Result<Option<layout::Block>, String> {
    if !tc.spans.is_empty() {
        return shape_text_spans(fonts, project_family, tc, size, letter_spacing, ffmpeg_bin);
    }
    let style = text_layout_style_with(fonts, project_family, tc, size, letter_spacing)?;
    let wrap = match tc.wrap {
        notation::model::Wrap::Off => None,
        notation::model::Wrap::Width(w) => Some(w as f32),
    };
    match layout::layout(fonts, text, &style, wrap, Some(ffmpeg_bin)) {
        Ok(block) => Ok(Some(block)),
        Err(layout::TextError::NoFamily) if project_family.is_none() => Ok(None),
        Err(e) => Err(format!(
            "render refused: text {text:?} cannot be laid out ({e}); no MP4 was written"
        )),
    }
}

fn shape_comp_layouts(
    comp: &Composition,
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    ffmpeg_bin: &Path,
) -> Result<(Vec<Option<layout::Block>>, bool), String> {
    let mut layouts: Vec<Option<layout::Block>> = Vec::with_capacity(comp.clips.len());
    let mut bitmap_fallback = false;
    for clip in &comp.clips {
        let Clip::Text(tc) = clip else {
            layouts.push(None);
            continue;
        };
        match shape_text_string(fonts, project_family, tc, &tc.value, ffmpeg_bin)? {
            Some(block) => layouts.push(Some(block)),
            None => {
                bitmap_fallback = true;
                layouts.push(None);
            }
        }
    }
    Ok((layouts, bitmap_fallback))
}

/// Probe and decode one composition's videos (Sprint 6 T007): every
/// referenced video is probed before the first frame, picture streams
/// are smoke-decoded, and each visible clip window is decoded once
/// with timestamps. `clip_base` is the composition's global document
/// base and `video_base` its first index in the unified probe list, so
/// items address the unified tables. A clip window that outruns its
/// source is not a refusal (Sprint 5 T003): a looping clip repeats its
/// window, a non-looping one holds the last decoded frame with a
/// silent sound remainder.
fn prepare_comp_videos(
    comp: &Composition,
    clip_base: usize,
    video_base: usize,
    folder: &Path,
    ffmpeg_bin: &Path,
    footage_cache: &mut crate::footage_cache::FootageCache,
) -> Result<(Vec<ProbedVideo>, Vec<DecodedRange>), String> {
    // Probe every referenced video before the first frame.
    let mut videos: Vec<ProbedVideo> = Vec::new();
    for (idx, clip) in comp.clips.iter().enumerate() {
        if let Clip::Video(v) = clip {
            let probed = probe_video(ffmpeg_bin, &folder.join(&v.path), &v.path)?;
            let (eff_start, eff_end) = effective_source_window(v, probed.duration);
            let win_len = eff_end.map(|e| (e - eff_start).max(0.0));
            videos.push(ProbedVideo {
                clip_idx: clip_base + idx,
                loop_clip: v.loop_clip,
                win_start: eff_start,
                win_len,
                ..probed
            });
        }
    }
    // Smoke-decode each video stream so a corrupt file or an
    // undecodable picture refuses before the first frame, naming the
    // file. Audio streams are smoke-decoded in the audio stage
    // ([`build_audio_plan`]), which names the same file on failure.
    for pv in &videos {
        smoke_decode(ffmpeg_bin, pv, false)?;
    }
    // Decode each visible clip window once, with timestamps. Output
    // frames then select pictures by exact time: no drift, no per-frame
    // processes, and no stale-frame holding past the source end.
    //
    // Sprint 5 T006: each window decodes through the project-local
    // content-keyed frame cache, so an unchanged re-render reuses the
    // stored pictures verbatim (faster, byte-identical) while edited
    // media bytes always decode fresh.
    let mut ranges: Vec<DecodedRange> = Vec::with_capacity(videos.len());
    for (local_vi, pv) in videos.iter().enumerate() {
        let vi = video_base + local_vi;
        let local_idx = pv.clip_idx - clip_base;
        let (seek, src_len) = match comp.clips.get(local_idx) {
            Some(Clip::Video(v)) => {
                let win = clip_window_len(comp, v);
                let (_, eff_end) = effective_source_window(v, pv.duration);
                let want = (win * v.rate).max(0.0);
                let avail = eff_end.map(|e| (e - v.trim_start).max(0.0)).unwrap_or(want);
                (v.trim_start.max(0.0), want.min(avail))
            }
            _ => (0.0, 0.0),
        };
        let win = match comp.clips.get(local_idx) {
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
        let abs = pv.path.clone();
        let (width, height, rotation) = (pv.width, pv.height, pv.rotation);
        let range = footage_cache.decode_window(
            ffmpeg_bin,
            &abs,
            seek,
            src_len,
            width,
            height,
            rotation,
            vi,
            || decode_range(ffmpeg_bin, pv, seek, src_len),
        )?;
        ranges.push(range);
    }
    Ok((videos, ranges))
}

/// Decode one composition's images (Sprint 6 T007, plus Sprint 8 look
/// T005): a PNG or JPEG still decoded to straight RGBA through
/// FFmpeg, an SVG picture rasterized at its declared size through
/// `resvg`. A file validation accepted but the decoder cannot read
/// refuses before the first frame, naming the file — never a partial
/// MP4. Images accumulate into the shared list, deduplicated by
/// absolute path across the tree.
fn prepare_comp_images(
    comp: &Composition,
    folder: &Path,
    ffmpeg_bin: &Path,
    images: &mut Vec<DecodedImage>,
) -> Result<(), String> {
    for clip in &comp.clips {
        match clip {
            Clip::Image(im) => {
                if images
                    .iter()
                    .any(|d: &DecodedImage| d.path == folder.join(&im.path))
                {
                    continue;
                }
                if im.path.to_lowercase().ends_with(".svg") {
                    images.push(crate::path_paint::decode_svg_image(
                        folder,
                        &im.path,
                        im.width.unwrap_or(f64::from(comp.width)),
                        im.height.unwrap_or(f64::from(comp.height)),
                        im.fit,
                        im.placement,
                    )?);
                } else {
                    images.push(decode_image(ffmpeg_bin, folder, &im.path)?);
                }
            }
            // A `map` clip pre-renders its offline viewport (Sprint 21
            // adapters, T004): tiles composite at the authored center
            // and zoom with overlays, scale bar, and attribution baked
            // in, under the canonical key the frame stage resolves.
            // Local reads and PNG decodes only, never the network. A
            // vanished or replaced tile refuses before frame one,
            // naming the file, like any undecodable still.
            Clip::Map(m) => {
                let (vw_px, vh_px) = crate::map::viewport_px(m, (comp.width, comp.height));
                let key = crate::map::viewport_key(m, vw_px, vh_px);
                if images
                    .iter()
                    .any(|d: &DecodedImage| d.path == folder.join(&key))
                {
                    continue;
                }
                let (mw, mh, rgba) = crate::map::render_viewport(folder, m, vw_px, vh_px)?;
                images.push(DecodedImage {
                    path: folder.join(key),
                    width: mw,
                    height: mh,
                    rgba,
                    scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
                });
            }
            // A `chart` clip pre-renders its data and style (Sprint 21
            // adapters, CAP-002/T002): the validated rows baked to
            // straight RGBA at the authored size under the canonical
            // key the frame stage resolves. Pure pixels, no browser,
            // no network, no per-frame work — reruns share the entry
            // and byte-match. Infallible here: validation already
            // refused undecodable sources and malformed rows.
            Clip::Chart(ch) => {
                let (vw_px, vh_px) = crate::chart::viewport_px(ch, (comp.width, comp.height));
                let key = crate::chart::viewport_key(ch, vw_px, vh_px);
                if images
                    .iter()
                    .any(|d: &DecodedImage| d.path == folder.join(&key))
                {
                    continue;
                }
                let (mw, mh, rgba) = crate::chart::render_viewport(ch, vw_px, vh_px);
                images.push(DecodedImage {
                    path: folder.join(key),
                    width: mw,
                    height: mh,
                    rgba,
                    scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
                });
            }
            // An `svg` clip decodes its standalone file pre-composed
            // at the declared `width`/`height` (Sprint 8 look, T005),
            // so the shared blitter draws it with `fill` placement.
            Clip::Svg(sv) => {
                if images
                    .iter()
                    .any(|d: &DecodedImage| d.path == folder.join(&sv.path))
                {
                    continue;
                }
                images.push(crate::path_paint::decode_svg_image(
                    folder,
                    &sv.path,
                    sv.width,
                    sv.height,
                    sv.fit,
                    sv.placement,
                )?);
            }
            // Sprint 8 look (T003): box image fills decode with the
            // clip stills, so an unreadable fill refuses before frame
            // one even when `check` was skipped; the per-frame vanish
            // check still guards sources deleted after this decode.
            Clip::Box(b) => {
                if let FillPaint::Image { src, .. } = &b.fill_paint {
                    if images
                        .iter()
                        .any(|d: &DecodedImage| d.path == folder.join(src))
                    {
                        continue;
                    }
                    images.push(decode_image(ffmpeg_bin, folder, src)?);
                }
            }
            _ => {}
        }
        // Sprint 11 VFX (CAP-004/T005): active file-backed stages
        // decode with the clip stills, so an unreadable map or matte
        // refuses before frame one even when `check` was skipped; the
        // per-frame vanish check still guards sources deleted after
        // this decode. Audio chains rest inert and decode nothing; an
        // inert stage decodes nothing either, so disabling a stage
        // matches the chain written without it.
        if !matches!(clip, Clip::Audio(_)) {
            let vfx = clip.vfx();
            let mut stage_srcs = Vec::new();
            if let Some(stage) = vfx.displacement.as_ref().filter(|s| !s.is_empty()) {
                if let Some(map) = stage.map.as_deref() {
                    stage_srcs.push(map);
                }
            }
            if let Some(stage) = vfx.luma_matte.as_ref().filter(|s| !s.is_empty()) {
                stage_srcs.push(stage.src.as_str());
            }
            for src in stage_srcs {
                if images
                    .iter()
                    .any(|d: &DecodedImage| d.path == folder.join(src))
                {
                    continue;
                }
                images.push(decode_image(ffmpeg_bin, folder, src)?);
            }
        }
    }
    Ok(())
}

/// Prepare every picture input before the first frame (Sprint 6 T007):
/// the root composition plus every transitively nested composition —
/// fonts, motion evaluators, text layouts, video probes with video
/// smoke decodes, clip-window decodes, and image decodes — and return
/// the shared [`Prepared`] both renderers draw from. `all` carries
/// every validated composition so `nest` targets resolve; a project
/// that validates clean always resolves here. Audio-stream smoke
/// decodes and the 48 kHz mix (tracks plus video sound) live in
/// [`build_audio_plan`] (the audio stage, Sprint 3 T008/T010), which
/// runs after this step. Any failure names its file or stage and writes
/// no MP4, exactly as the full render would.
/// One validated sub-range of a composition (Sprint 6 T008): the
/// composition-time window `[start, end)` rendered as its own
/// standalone MP4 with picture and sound aligned as in the full
/// render. `start` defaults to 0 and `end` to the composition length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRange {
    pub start: f64,
    pub end: f64,
}

/// Validate one `render --start/--end` window against the selected
/// composition (Sprint 6 T008, REC-003): `None`/`None` renders the
/// whole composition. An empty, backward, or out-of-range window
/// refuses before frame one, naming the composition length and the
/// accepted window. `fps` is the selected composition's rate, so the
/// refusal can also name the frame count the window would yield.
pub fn resolve_range(
    comp_name: &str,
    duration: f64,
    fps: &notation::model::Fps,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<Option<RenderRange>, String> {
    if start.is_none() && end.is_none() {
        return Ok(None);
    }
    let s = start.unwrap_or(0.0);
    let e = end.unwrap_or(duration);
    let window = format!("{s:.3}s..{e:.3}s");
    let accepted = format!("0 <= start < end <= {duration:.3}s");
    let refuse = |why: String| {
        format!(
            "render refused: --start/--end {window} is not a usable sub-range of composition {comp_name:?} (duration {duration:.3}s): {why}; accepted window is {accepted} (validation stage); no MP4 was written"
        )
    };
    for (key, v) in [("--start", s), ("--end", e)] {
        if !v.is_finite() {
            return Err(refuse(format!("{key} must be a number of seconds")));
        }
    }
    if s > e {
        return Err(refuse(format!(
            "the window runs backward (start {s:.3}s is past end {e:.3}s)"
        )));
    }
    if s == e {
        return Err(refuse(format!(
            "the window is empty (start and end are both {s:.3}s)"
        )));
    }
    if s < 0.0 || e > duration || s >= duration {
        return Err(refuse(format!(
            "the window reaches outside the composition (composition spans 0..{duration:.3}s)"
        )));
    }
    let frames = fps.frame_count(e - s);
    if frames == 0 {
        return Err(refuse(format!(
            "the window holds no output frame at {fps} fps"
        )));
    }
    Ok(Some(RenderRange { start: s, end: e }))
}

pub fn prepare_frames(
    comp: &Composition,
    all: &[Composition],
    folder: &Path,
    ffmpeg_bin: &Path,
) -> Result<Prepared, String> {
    prepare_frames_ranged(comp, all, folder, ffmpeg_bin, None)
}

/// Prepare every picture input for one render (Sprint 6 T008): the
/// whole composition for a full render, or exactly the `range` window
/// for a sub-range render. A ranged preparation draws its frame `k`
/// at the range start plus `k` frame intervals and encodes `end -
/// start` seconds, so the standalone MP4 aligns with the full render.
pub fn prepare_frames_ranged(
    comp: &Composition,
    all: &[Composition],
    folder: &Path,
    ffmpeg_bin: &Path,
    range: Option<RenderRange>,
) -> Result<Prepared, String> {
    // The render tree: the root first, then nested compositions in
    // breadth-first document order. Nest-free projects prepare exactly
    // the root, as before.
    let order = nest_render_order(comp, all)?;
    // Adapter readiness (Sprint 21 adapters, T007 / CAP-007): every
    // file-backed adapter source re-resolves before the first frame,
    // so a source lost after validation refuses naming its file and
    // cause instead of decoding a guess or writing a partial MP4.
    crate::readiness::gate_adapter_sources(folder, &order)?;
    let mut footage_cache = crate::footage_cache::FootageCache::open(folder);
    let raster: RasterCache = HashMap::new();
    // The root keeps the preparation's font set (its origins are named
    // in the success line, as before); nested compositions shape with
    // throwaway sets.
    let mut fonts = layout::Fonts::new();
    let mut project_family: Option<String> = None;
    register_comp_fonts(order[0], folder, &mut fonts, &mut project_family)?;
    // Motion: exact-time clock plus one evaluator per clip. Validated
    // `tween`/`set` directives (T003) ride in the composition; every
    // clip still holds its own values until tween/set evaluation lands
    // (T009/T010/T011) — the evaluator already runs on every frame.
    let motions = plan_motions(order[0], snap_fps(&order[0].fps));
    let (mut layouts, root_fallback) =
        shape_comp_layouts(order[0], &mut fonts, &project_family, ffmpeg_bin)?;
    let mut bitmap_fallback = root_fallback;
    let (mut videos, mut ranges) =
        prepare_comp_videos(order[0], 0, 0, folder, ffmpeg_bin, &mut footage_cache)?;
    let mut images: Vec<DecodedImage> = Vec::new();
    prepare_comp_images(order[0], folder, ffmpeg_bin, &mut images)?;
    crate::captions::prepare_caption_mattes(order[0], folder, &mut images)?;
    let mut caption_layouts = crate::captions::CaptionLayouts::new();
    let captions_fallback = crate::captions::shape_caption_layouts(
        order[0],
        0,
        &mut fonts,
        &project_family,
        ffmpeg_bin,
        &mut caption_layouts,
    )?;
    bitmap_fallback |= captions_fallback;
    let mut lotties = prepare_comp_lotties(order[0], 0, folder)?;
    // Nested compositions append their media at their global document
    // base, so nested items address the unified tables.
    let mut tree: Vec<TreeComp> = Vec::new();
    let mut base = order[0].clips.len();
    // Nested font sets are retained for the frame-varying text pass
    // below; the root set lives in `fonts` until `Prepared` takes it.
    let mut nested_font_store: Vec<(layout::Fonts, Option<String>)> = Vec::new();
    for nested in order.iter().skip(1) {
        let mut nested_fonts = layout::Fonts::new();
        let mut nested_family: Option<String> = None;
        register_comp_fonts(nested, folder, &mut nested_fonts, &mut nested_family)?;
        let nested_motions = plan_motions(nested, snap_fps(&nested.fps));
        let (nested_layouts, nested_fallback) =
            shape_comp_layouts(nested, &mut nested_fonts, &nested_family, ffmpeg_bin)?;
        bitmap_fallback |= crate::captions::shape_caption_layouts(
            nested,
            base,
            &mut nested_fonts,
            &nested_family,
            ffmpeg_bin,
            &mut caption_layouts,
        )?;
        nested_font_store.push((nested_fonts, nested_family));
        let video_base = videos.len();
        let (mut nested_videos, nested_ranges) = prepare_comp_videos(
            nested,
            base,
            video_base,
            folder,
            ffmpeg_bin,
            &mut footage_cache,
        )?;
        prepare_comp_images(nested, folder, ffmpeg_bin, &mut images)?;
        crate::captions::prepare_caption_mattes(nested, folder, &mut images)?;
        lotties.extend(prepare_comp_lotties(nested, base, folder)?);
        layouts.extend(nested_layouts);
        bitmap_fallback |= nested_fallback;
        videos.append(&mut nested_videos);
        ranges.extend(nested_ranges);
        tree.push(TreeComp {
            comp: (*nested).clone(),
            motions: nested_motions,
            base,
        });
        base += nested.clips.len();
    }

    // Sprint 6 T008: a sub-range renders exactly `end - start` seconds
    // from the `--start` instant; a full render keeps the whole
    // composition from zero. `frame_count` rounds up, so a non-empty
    // validated window always holds at least one frame.
    let (range_start, out_duration) = match range {
        Some(r) => (crate::motion_plan::secs_to_time(r.start), r.end - r.start),
        None => (timebase::Time::ZERO, comp.duration),
    };
    let frames = comp.fps.frame_count(out_duration) as usize;
    if frames == 0 {
        return Err(
            "render refused: the composition has no frames; no MP4 was written".to_string(),
        );
    }

    // Sprint 7 text effects (CAP-009): pre-shape every distinct string
    // a typewriter reveal or count range shows at a drawn instant.
    // The enumeration replays the exact instants drawing replays
    // (range start plus frame intervals, through each nest `rate`),
    // so every drawn instant hits a pre-shaped block. Shaping runs
    // once here, read-only afterwards, so worker counts and
    // re-renders draw identical bytes. Still-only projects skip the
    // enumeration entirely.
    let mut dyn_layouts = DynLayouts::default();
    let any_dynamic = order.iter().any(|c| {
        c.clips.iter().any(|clip| match clip {
            Clip::Text(tc) => crate::textfx::is_dynamic(tc),
            _ => false,
        }) || text_metrics_animated(c)
    });
    if any_dynamic {
        let mut comp_fonts: Vec<(&mut layout::Fonts, &Option<String>)> =
            Vec::with_capacity(order.len());
        comp_fonts.push((&mut fonts, &project_family));
        for (f, fam) in nested_font_store.iter_mut() {
            comp_fonts.push((f, fam));
        }
        let mut comp_motions: Vec<&[ClipMotion]> = Vec::with_capacity(order.len());
        comp_motions.push(&motions);
        for t in &tree {
            comp_motions.push(&t.motions);
        }
        let mut bases: Vec<usize> = Vec::with_capacity(order.len());
        bases.push(0);
        for t in &tree {
            bases.push(t.base);
        }
        let view = crate::frame::NestView {
            comps: order.clone(),
            motions: comp_motions,
            bases: bases.clone(),
        };
        // Distinct dynamic strings (plus animated metric triples) in
        // first-seen (frame) order, so shaping order — and every byte
        // it yields — is deterministic.
        let mut pending: Vec<(usize, String, f64, Option<f64>)> = Vec::new();
        for k in 0..frames as u64 {
            let now = range_start + crate::motion_plan::frame_time(&comp.fps, k);
            let desc = crate::frame::describe_nested_at(&view, 0, k, now);
            for item in &desc.items {
                if let crate::frame::FrameItem::Text(tc) = item {
                    if tc.value.is_empty() {
                        continue;
                    }
                    if dyn_layouts.contains(tc.doc, &tc.value, tc.size, tc.letter_spacing) {
                        continue;
                    }
                    if pending.iter().any(|(d, s, z, l)| {
                        *d == tc.doc && *s == tc.value && *z == tc.size && *l == tc.letter_spacing
                    }) {
                        continue;
                    }
                    pending.push((tc.doc, tc.value.clone(), tc.size, tc.letter_spacing));
                }
            }
        }
        for (doc, text, size, spacing) in pending {
            // Owning composition and local clip of one global document id.
            let mut owner = 0usize;
            for (ci, b) in view.bases.iter().enumerate() {
                if doc >= *b && doc - *b < order[ci].clips.len() {
                    owner = ci;
                    break;
                }
            }
            let local = doc - view.bases[owner];
            let Clip::Text(tc) = &order[owner].clips[local] else {
                continue;
            };
            // Static layouts already hold authored-metric triples, so
            // only frame-varying strings and metric overrides shape here.
            if !crate::textfx::is_dynamic(tc) && size == tc.size && spacing == tc.letter_spacing {
                continue;
            }
            let (fonts_for, family_for) = &mut comp_fonts[owner];
            match shape_text_string_with(
                fonts_for, family_for, tc, &text, size, spacing, ffmpeg_bin,
            )? {
                Some(block) => dyn_layouts.insert(doc, text, size, spacing, block),
                // The bitmap fallback draws from the string itself, so
                // nothing is pre-shaped for it.
                None => {}
            }
        }
    }

    Ok(Prepared {
        comp: comp.clone(),
        motions,
        frames,
        fonts,
        project_family,
        layouts,
        dyn_layouts,
        caption_layouts,
        bitmap_fallback,
        videos,
        ranges,
        images,
        lotties,
        raster,
        paths: crate::path_paint::PathCache::new(),
        tree,
        cache_stats: footage_cache.stats(),
        range_start,
        out_duration,
        folder: folder.to_path_buf(),
    })
}

/// Historical entry point preserved for existing callers: every
/// pre-first-frame step of [`prepare_frames`] plus audio-stream smoke
/// decodes, for a standalone composition with no siblings to nest. The
/// CLI render path uses [`prepare_frames`] plus [`build_audio_plan`]
/// so the frames stage and the audio stage time separately (Sprint 3
/// T006).
pub fn prepare(comp: &Composition, folder: &Path, ffmpeg_bin: &Path) -> Result<Prepared, String> {
    let prep = prepare_frames(comp, std::slice::from_ref(comp), folder, ffmpeg_bin)?;
    let no_motions: &[ClipMotion] = &[];
    let view = crate::frame::NestView {
        comps: vec![comp],
        motions: vec![no_motions],
        bases: vec![0],
    };
    smoke_audio_streams(ffmpeg_bin, &prep.videos, &view)?;
    Ok(prep)
}

/// Smoke-decode every audible video stream so a corrupt file or an
/// undecodable audio stream refuses before the first frame, naming the
/// file. Runs inside the audio stage ([`build_audio_plan`]); kept as a
/// separate step so [`prepare`] preserves its historical behavior.
/// Hidden clips are absent from the mix (Sprint 6 T009): their sources
/// are never smoke-decoded, so a corrupt hidden source cannot fail a
/// render that hides it.
pub(crate) fn smoke_audio_streams(
    ffmpeg_bin: &Path,
    videos: &[ProbedVideo],
    view: &crate::frame::NestView,
) -> Result<(), String> {
    for pv in videos.iter().filter(|v| v.has_audio) {
        if view.is_hidden(pv.clip_idx) {
            continue;
        }
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
    /// Planned loudness gain (Sprint 10 sound, CAP-009): `Some` when
    /// the composition normalizes, already applied to the staged mix
    /// and reported in the render summary.
    pub normalize: Option<crate::loudness::NormalizeReport>,
    /// On-canvas sound shapes (Sprint 10 sound, CAP-010), drawn onto
    /// rendered frames in [`render_staged`].
    pub waveforms: Vec<crate::waveform::WaveformOverlay>,
}

/// Why a mix preview is unavailable (Sprint 10 sound): silent
/// normalization fails closed by construction; anything else names a
/// decode failure the render reports.
#[derive(Debug, Clone)]
pub enum PreviewError {
    /// `normalize="loud"` with a silent mix: the listing refuses
    /// instead of scaling silence, carrying the render-stage wording.
    Silent(String),
    /// A probe or decode failure naming the file.
    Failed(String),
}

impl PreviewError {
    /// The refusal or failure wording the caller reports.
    pub fn message(&self) -> &str {
        match self {
            PreviewError::Silent(m) | PreviewError::Failed(m) => m,
        }
    }
}

/// Refusal for a normalized composition with no audible mix (Sprint
/// 10 sound, CAP-009/REC-005): one overall gain cannot scale silence
/// to −16 LUFS, so the render stops before the first frame and
/// writes no MP4.
fn silent_normalize_refusal(comp_name: &str) -> String {
    format!(
        "render refused: composition {comp_name:?} sets `normalize=\"loud\"` but its mix carries no audible samples to measure; one overall gain cannot scale silence to −16 LUFS — add audible sound or remove `normalize`. No MP4 was written (audio stage)"
    )
}

/// Plan one overall loudness gain for a placed mix (Sprint 10 sound,
/// CAP-009): the gain landing integrated loudness at −16 LUFS with
/// true-peak guarding. Silent mixes fail closed with the render-stage
/// refusal instead of scaling silence.
fn normalize_gain_for_acc(
    comp: &Composition,
    acc: &[f32],
) -> Result<(f32, crate::loudness::NormalizeReport), String> {
    match crate::loudness::normalize_gain_for_mix(acc) {
        Ok(planned) => Ok(planned),
        Err(_) => Err(silent_normalize_refusal(&comp.name)),
    }
}

/// On-canvas sound shapes of one composition (Sprint 10 sound,
/// CAP-010): per `waveform` clip, the clip's placed contribution —
/// the same decode, resample, and per-sample gains the mix walk uses
/// (times the loudness `gain`) — sliced to the drawn range and peaked
/// per canvas column. Valid projects resolve every drawn range, so an
/// unresolvable range is skipped instead of misdrawn; a decode
/// failure refuses naming the file. Clips without the drawing flag
/// plan no decode at all.
fn build_waveform_overlays(
    comp: &Composition,
    videos: &[ProbedVideo],
    folder: &Path,
    ffmpeg_bin: &Path,
    total: usize,
    gain: f32,
) -> Result<Vec<crate::waveform::WaveformOverlay>, String> {
    use notation::model::{clip_waveform, resolve_waveform_range};
    if !comp.clips.iter().any(|c| clip_waveform(c).is_some()) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let xplan = crate::envelope::plan_xfade(comp);
    for (idx, clip) in comp.clips.iter().enumerate() {
        if clip.hidden() {
            continue;
        }
        let draw = match clip_waveform(clip) {
            Some(d) => d,
            None => continue,
        };
        let Ok(range) = resolve_waveform_range(
            clip.start(),
            clip.duration(),
            comp.duration,
            draw.from,
            draw.to,
        ) else {
            continue;
        };
        let channels = MIX_CHANNELS as usize;
        let mut scratch = vec![0.0f32; total * channels];
        match clip {
            Clip::Audio(a) => {
                let Some((pcm, start, take)) =
                    decode_audio_window(ffmpeg_bin, folder, a, comp.duration)?
                else {
                    continue;
                };
                place_into_mix(
                    &mut scratch,
                    &pcm,
                    total,
                    start,
                    take,
                    db_to_gain(a.volume_db)?,
                    a.fade_in,
                    a.fade_out,
                    &crate::envelope::resolve_envelope(comp, idx),
                    &xplan[idx],
                    crate::envelope::plan_duck(comp, idx),
                    &a.fx,
                );
            }
            Clip::Video(v) => {
                let Some((pcm, start, take)) =
                    decode_video_window(ffmpeg_bin, folder, videos, v, idx, total, comp.duration)?
                else {
                    continue;
                };
                place_into_mix(
                    &mut scratch,
                    &pcm,
                    total,
                    start,
                    take,
                    db_to_gain(v.volume_db)?,
                    v.fade_in,
                    v.fade_out,
                    &crate::envelope::resolve_envelope(comp, idx),
                    &xplan[idx],
                    crate::envelope::plan_duck(comp, idx),
                    &v.fx,
                );
            }
            _ => continue,
        }
        if gain != 1.0 {
            for s in scratch.iter_mut() {
                *s *= gain;
            }
        }
        let from_frame = (range.eff_from * MIX_RATE as f64).round().max(0.0) as usize;
        let to_frame = (range.eff_to * MIX_RATE as f64).round().max(0.0) as usize;
        let frames = scratch.len() / channels;
        let slice = &scratch[from_frame.min(frames) * channels..to_frame.min(frames) * channels];
        out.push(crate::waveform::WaveformOverlay {
            from: range.eff_from,
            to: range.eff_to,
            columns: crate::waveform::columns_for_contribution(slice, comp.width as usize),
        });
    }
    Ok(out)
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
///
/// Sprint 10 sound (CAP-002): fades are envelope sugar — each edge is
/// one linear envelope segment run through the shared sample-exact
/// primitive [`crate::envelope::eval_linear`], so the sugared and
/// explicit-envelope forms null-test equal and every pre-existing fade
/// output stays byte-identical.
pub fn fade_factor(i: usize, n: usize, fade_in_n: usize, fade_out_n: usize) -> f32 {
    use crate::envelope::eval_linear;
    let mut g = 1.0f32;
    if fade_in_n > 0 && i < fade_in_n {
        g *= eval_linear(0.0, 1.0, i as i128 + 1, 0, fade_in_n as i128);
    }
    if fade_out_n > 0 && i < n {
        let tail = n - 1 - i;
        if tail < fade_out_n {
            g *= eval_linear(
                1.0,
                0.0,
                i as i128 + 1,
                n as i128 - fade_out_n as i128,
                n as i128,
            );
        }
    }
    g
}

/// Plainly resample one stereo s16 source by `rate` (Sprint 5 T002):
/// playback consumes `rate` source seconds per composition second, so
/// `rate=2` halves the sample count (pitch doubles) and `rate=0.5`
/// doubles it (pitch halves) — pitch follows speed, never corrected.
/// Linear interpolation between source frames; the same input always
/// yields the same output. `rate == 1` returns the input unchanged so
/// unity mixes keep their historical bytes.
pub fn resample_plain(pcm: &[i16], rate: f64) -> Vec<i16> {
    debug_assert!(rate.is_finite() && rate > 0.0);
    if (rate - 1.0).abs() < 1e-12 {
        return pcm.to_vec();
    }
    let channels = MIX_CHANNELS as usize;
    let frames = pcm.len() / channels;
    if frames == 0 {
        return Vec::new();
    }
    let out_frames = ((frames as f64) / rate).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_frames * channels);
    for j in 0..out_frames {
        let pos = (j as f64) * rate;
        let i0 = (pos.floor() as usize).min(frames - 1);
        let i1 = (i0 + 1).min(frames - 1);
        let frac = (pos - pos.floor()) as f32;
        for c in 0..channels {
            let a = pcm[i0 * channels + c] as f32;
            let b = pcm[i1 * channels + c] as f32;
            out.push(
                (a + (b - a) * frac)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16,
            );
        }
    }
    out
}

/// Repeat one stereo s16 loop unit to exactly `frames` composition
/// frames (Sprint 5 T003): a looping clip repeats its source window —
/// picture and sound together — so the resampled window unit tiles
/// with no gap at each loop point. An empty unit or a zero fill stays
/// empty; the same unit always tiles to the same samples.
pub fn tile_loop_unit(unit: &[i16], frames: usize) -> Vec<i16> {
    let channels = MIX_CHANNELS as usize;
    let unit_frames = unit.len() / channels;
    if unit_frames == 0 || frames == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(frames * channels);
    for j in 0..frames {
        let s = (j % unit_frames) * channels;
        out.extend_from_slice(&unit[s..s + channels]);
    }
    out
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
    mix_into_full(
        acc,
        pcm,
        offset,
        gain,
        fade_in_n,
        fade_out_n,
        &[],
        &[],
        &[],
        None,
        offset,
    )
}

/// Add one decoded stereo s16 track into the f32 mix accumulator with
/// `gain`, linear fades, sound envelopes (Sprint 10, CAP-001/002),
/// overlap crossfades (CAP-003), and voice ducking (CAP-004):
/// explicit `volume` envelopes multiply the sugared fades (edge
/// smoothing survives a level ride), crossfade segments multiply next
/// (handoffs blend under the envelope), the duck gate multiplies last
/// (voice windows scale the whole bed), and `pan` envelopes balance
/// the channels equal-gain. Quantized `vol`/`pan` chains come from
/// [`crate::envelope::quantize`]; empty chains ride the static
/// behavior. Summation order matches [`mix_into`] exactly, so
/// envelope-free clips mix bit-identically to before.
#[allow(clippy::too_many_arguments)]
fn mix_into_full(
    acc: &mut [f32],
    pcm: &[i16],
    offset: usize,
    gain: f32,
    fade_in_n: usize,
    fade_out_n: usize,
    vol: &[crate::envelope::QSeg],
    pan: &[crate::envelope::QSeg],
    xfade: &[crate::envelope::XSeg],
    duck: Option<crate::envelope::DuckWin>,
    duck_base: usize,
) {
    use crate::envelope::{eval_chain, pan_gains, xfade_gain};
    let channels = MIX_CHANNELS as usize;
    let frames = pcm.len() / channels;
    let capacity = acc.len() / channels;
    let n = frames.min(capacity.saturating_sub(offset.min(capacity)));
    let fi = fade_in_n.min(n);
    let fo = fade_out_n.min(n);
    let has_vol = !vol.is_empty();
    let has_pan = !pan.is_empty();
    let has_xfade = !xfade.is_empty();
    for i in 0..n {
        // One gain primitive: the sugared fades evaluate exactly like
        // `fade_factor`, then the explicit volume envelope multiplies,
        // then every covering crossfade segment, then the duck gate.
        // The multiplication order is fixed, so re-renders agree bit
        // for bit at any worker count.
        let b = i as i128 + 1;
        let mut g = gain * fade_factor(i, n, fi, fo);
        if has_vol {
            g *= eval_chain(vol, b, 0.0, 2.0);
        }
        if has_xfade {
            let mut xg = 1.0f32;
            for s in xfade {
                if s.q0 < b && b <= s.q1 {
                    xg *= xfade_gain(s, b);
                }
            }
            g *= xg;
        }
        if let Some(d) = duck {
            let k = duck_base + i;
            if d.v0 <= k && k < d.v1 {
                g *= d.amount;
            }
        }
        if has_pan {
            debug_assert_eq!(channels, 2);
            let (pl, pr) = pan_gains(eval_chain(pan, b, -1.0, 1.0));
            acc[(offset + i) * channels] += pcm[i * channels] as f32 / 32768.0 * g * pl;
            acc[(offset + i) * channels + 1] += pcm[i * channels + 1] as f32 / 32768.0 * g * pr;
        } else {
            for c in 0..channels {
                acc[(offset + i) * channels + c] += pcm[i * channels + c] as f32 / 32768.0 * g;
            }
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
/// `gain`, linear fades, the clip's sound envelope (Sprint 10,
/// CAP-001/002), its crossfade segments (CAP-003), its duck gate
/// (CAP-004), and its echo/room tail (CAP-005), clamping to the
/// composition length. Fades desugar to the same envelope segments an
/// explicit `key` would carry; the explicit `volume` chain multiplies
/// them, crossfades blend handoffs underneath, ducking scales voice
/// windows last, `pan` balances the channels, and delay blooms echo
/// taps before reverb blooms the room off them. Dry clips mix down
/// the historical path bit-identically; effected tails truncate at the
/// composition end (reported by `check`, never refused).
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
    env: &crate::envelope::SoundEnvelope,
    xfade: &[crate::envelope::XSeg],
    duck: Option<crate::envelope::DuckWin>,
    fx: &notation::model::SoundFx,
) {
    let offset = (start * MIX_RATE as f64).round().max(0.0) as usize;
    let n_frames = pcm.len() / MIX_CHANNELS as usize;
    let placed = n_frames.min(total.saturating_sub(offset.min(total)));
    let placed = placed.min((take * MIX_RATE as f64).ceil() as usize);
    let fade_in_n = ((fade_in * MIX_RATE as f64).round() as usize).min(placed);
    let fade_out_n = ((fade_out * MIX_RATE as f64).round() as usize).min(placed);
    let vol = crate::envelope::quantize(&env.vol, start);
    let pan = crate::envelope::quantize(&env.pan, start);
    let tail =
        notation::model::effect_tail_frames(fx.delay, fx.delay_feedback, fx.delay_mix, fx.reverb);
    if tail == 0 {
        mix_into_full(
            acc,
            &pcm[..placed * MIX_CHANNELS as usize],
            offset.min(total),
            gain,
            fade_in_n,
            fade_out_n,
            &vol,
            &pan,
            xfade,
            duck,
            offset.min(total),
        );
        return;
    }
    // Effected clips render dry into scratch, bloom delay then reverb
    // per channel, and add the room back truncated to the composition
    // end. The per-sample gains (fades, envelopes, crossfades, duck)
    // run the same primitives over the same boundaries, so the dry
    // head nulls against the dry path exactly.
    use crate::envelope::{apply_delay, apply_reverb};
    use notation::model::delay_tail_frames;
    let delay_tail = delay_tail_frames(fx.delay, fx.delay_feedback, fx.delay_mix);
    let room = total.saturating_sub(offset.min(total));
    let keep = (placed + tail).min(room);
    let mut scratch = vec![0.0f32; (placed + tail) * MIX_CHANNELS as usize];
    mix_into_full(
        &mut scratch,
        &pcm[..placed * MIX_CHANNELS as usize],
        0,
        gain,
        fade_in_n,
        fade_out_n,
        &vol,
        &pan,
        xfade,
        duck,
        offset.min(total),
    );
    let channels = MIX_CHANNELS as usize;
    let mut left: Vec<f32> = scratch.iter().step_by(channels).copied().collect();
    let mut right: Vec<f32> = scratch.iter().skip(1).step_by(channels).copied().collect();
    for ch in [&mut left, &mut right] {
        apply_delay(ch, placed, fx.delay, fx.delay_feedback, fx.delay_mix);
        apply_reverb(ch, placed + delay_tail, fx.reverb);
    }
    let base = offset.min(total) * channels;
    for i in 0..keep {
        acc[base + i * channels] += left[i];
        acc[base + i * channels + 1] += right[i];
    }
}

/// Decoded window of one `audio` track (Sprint 6 T003 placement,
/// shared by the mix walk and waveform drawing): the composition-time
/// `take` clamped to the audible window, the trim window at the
/// clip's `rate`, and `range_end`, decoded from `trim_start` and
/// plainly resampled so pitch follows speed. Returns `None` when the
/// track contributes no samples — the same skips the mix applies.
/// The returned triple is the resampled PCM, the placement start in
/// composition seconds, and the composition-time take.
fn decode_audio_window(
    ffmpeg_bin: &Path,
    folder: &Path,
    a: &AudioClip,
    range_end: f64,
) -> Result<Option<(Vec<i16>, f64, f64)>, String> {
    let end = (a.start + a.duration).min(range_end);
    if end <= a.start || a.start >= range_end {
        return Ok(None);
    }
    // Sprint 6 T003: the audible window in composition seconds,
    // bounded by the source window at the clip's `rate` — `take * rate`
    // source seconds from `trim_start`, plainly resampled like video
    // sound so pitch follows speed. Unity rate decodes exactly the
    // historical window, so older mixes stay byte-identical.
    let window = (end - a.start).max(0.0);
    let avail = a
        .trim_end
        .map(|e| (e - a.trim_start).max(0.0) / a.rate)
        .unwrap_or(window);
    let take = window.min(avail);
    if take <= 0.0 {
        return Ok(None);
    }
    let decoded = decode_mix_source(
        ffmpeg_bin,
        &folder.join(&a.path),
        &a.path,
        a.trim_start,
        take * a.rate,
    )?;
    Ok(Some((resample_plain(&decoded, a.rate), a.start, take)))
}

/// Decoded window of one `video` clip's sound (Sprint 5 T002
/// placement, shared by the mix walk and waveform drawing): `take *
/// rate` source seconds from `trim_start`, plainly resampled to
/// composition seconds with loop tiling and silent padding exactly
/// like the mix. Returns `None` for soundless video or an empty
/// window. `clip_idx` is the clip's global document index into
/// `videos`; the triple matches [`decode_audio_window`].
fn decode_video_window(
    ffmpeg_bin: &Path,
    folder: &Path,
    videos: &[ProbedVideo],
    v: &VideoClip,
    clip_idx: usize,
    total: usize,
    range_end: f64,
) -> Result<Option<(Vec<i16>, f64, f64)>, String> {
    // Soundless video contributes nothing and is not an error.
    let pv = match videos
        .iter()
        .find(|p| p.clip_idx == clip_idx && p.has_audio)
    {
        Some(pv) => pv,
        None => return Ok(None),
    };
    let end = (v.start + v.duration).min(range_end);
    if end <= v.start || v.start >= range_end {
        return Ok(None);
    }
    let take = (end - v.start).max(0.0);
    if take <= 0.0 {
        return Ok(None);
    }
    // Sprint 5 T002: the clip's sound is its source window — `take *
    // rate` source seconds from `trim_start` — plainly resampled to
    // `take` composition seconds so sound stays aligned with the
    // re-timed picture and pitch follows speed. Unity rate with no
    // trim keeps the historical decode (offset 0, no resample) so
    // older mixes stay byte-identical. Sprint 5 T003: the decode never
    // runs past the effective window end (a looping clip tiles the one
    // window unit instead); a non-looping overrun decodes short and
    // the mix pads the silent remainder.
    let rel = rel_name(folder, &pv.path);
    let src_len = match pv.win_len {
        Some(w) => (take * v.rate).min(w),
        None => take * v.rate,
    };
    let decoded = decode_mix_source(ffmpeg_bin, &pv.path, &rel, v.trim_start, src_len)?;
    let unit = resample_plain(&decoded, v.rate);
    let want_frames = (take * MIX_RATE as f64).round().max(0.0) as usize;
    let unit_frames = unit.len() / MIX_CHANNELS as usize;
    let room = total.saturating_sub((v.start * MIX_RATE as f64).round().max(0.0) as usize);
    let short = unit_frames < want_frames.min(room);
    let pcm = if v.loop_clip && unit_frames > 0 && short {
        tile_loop_unit(&unit, want_frames)
    } else {
        unit
    };
    Ok(Some((pcm, v.start, take)))
}

/// Mix one tree composition's audible clips into `acc` (Sprint 6 T007
/// for the `nest` arm): every `audio` track with its trim, plus every
/// `video` clip's sound stream from its source window at its `rate`
/// (plainly resampled) with the clip's volume and fades, placed
/// sample-accurately and clamped to `range_end` — the composition
/// duration for the root, the scratch length for nested renders.
/// `base` is the composition's global document base into the unified
/// `videos` table. A `nest` clip mixes its whole target timeline once
/// into a scratch buffer, slices its content window, resamples it by
/// the use `rate`, and places it at the use start; past the content
/// the use contributes silence. Valid projects never hit the depth cap
/// or an unresolvable target (validation gates both); the guards keep
/// direct API misuse silent instead of recursing.
#[allow(clippy::too_many_arguments)]
fn place_comp_clips(
    view: &crate::frame::NestView<'_>,
    videos: &[ProbedVideo],
    folder: &Path,
    ffmpeg_bin: &Path,
    acc: &mut [f32],
    total: usize,
    range_end: f64,
    ci: usize,
    depth: u8,
    visited: &mut [bool],
) -> Result<(), String> {
    if crate::cancel::is_cancelled() {
        return Err(crate::cancel::error("audio"));
    }
    if depth > crate::frame::MAX_NEST_RENDER_DEPTH || visited[ci] {
        return Ok(());
    }
    visited[ci] = true;
    let comp = view.comps[ci];
    let base = view.bases[ci];
    // One crossfade plan per composition (Sprint 10, CAP-003): every
    // same-track handoff quantized once, then sliced per clip below.
    // The plan derives from the validated model alone, so every render
    // plans identical gains and re-renders stay byte-identical.
    let xplan = crate::envelope::plan_xfade(comp);
    for (idx, clip) in comp.clips.iter().enumerate() {
        if crate::cancel::is_cancelled() {
            visited[ci] = false;
            return Err(crate::cancel::error("audio"));
        }
        // Hidden clips contribute no sound (Sprint 6 T009): the mix
        // treats them as if absent. Picture exclusion rides in the
        // frame description; ids and windows keep validating.
        if clip.hidden() {
            continue;
        }
        match clip {
            Clip::Audio(a) => {
                let Some((pcm, _start, take)) =
                    decode_audio_window(ffmpeg_bin, folder, a, range_end)?
                else {
                    continue;
                };
                let gain = db_to_gain(a.volume_db)?;
                let env = crate::envelope::resolve_envelope(comp, idx);
                let duck = crate::envelope::plan_duck(comp, idx);
                place_into_mix(
                    &mut *acc,
                    &pcm,
                    total,
                    a.start,
                    take,
                    gain,
                    a.fade_in,
                    a.fade_out,
                    &env,
                    &xplan[idx],
                    duck,
                    &a.fx,
                );
            }
            Clip::Video(v) => {
                let Some((pcm, _start, take)) = decode_video_window(
                    ffmpeg_bin,
                    folder,
                    videos,
                    v,
                    base + idx,
                    total,
                    range_end,
                )?
                else {
                    continue;
                };
                let gain = db_to_gain(v.volume_db)?;
                let env = crate::envelope::resolve_envelope(comp, idx);
                let duck = crate::envelope::plan_duck(comp, idx);
                place_into_mix(
                    &mut *acc,
                    &pcm,
                    total,
                    v.start,
                    take,
                    gain,
                    v.fade_in,
                    v.fade_out,
                    &env,
                    &xplan[idx],
                    duck,
                    &v.fx,
                );
            }
            Clip::Nest(n) => {
                // Play the target's local timeline through this use:
                // mix the whole child once, slice the content window,
                // resample by the use rate, and place at the use start.
                let Some(ti) = view.resolve(&n.target) else {
                    continue;
                };
                let child = view.comps[ti];
                if !child.duration.is_finite() || child.duration <= 0.0 {
                    continue;
                }
                let content_end = n
                    .trim_end
                    .map(|e| e.min(child.duration))
                    .unwrap_or(child.duration);
                if content_end <= n.trim_start {
                    continue;
                }
                let content_len = (content_end - n.trim_start) / n.rate;
                let parent_len = n.duration.min(content_len);
                if parent_len <= 0.0 || n.start >= range_end {
                    continue;
                }
                let child_total = (child.duration * MIX_RATE as f64).ceil().max(1.0) as usize;
                let mut scratch = vec![0.0f32; child_total * MIX_CHANNELS as usize];
                place_comp_clips(
                    view,
                    videos,
                    folder,
                    ffmpeg_bin,
                    &mut scratch,
                    child_total,
                    child.duration,
                    ti,
                    depth + 1,
                    visited,
                )?;
                // Slice the use window out of the child mix, quantized
                // exactly like the staged mix so the slice is stable.
                let s0 =
                    ((n.trim_start * MIX_RATE as f64).round().max(0.0) as usize).min(child_total);
                let s1 = (s0 + (parent_len * n.rate * MIX_RATE as f64).round().max(0.0) as usize)
                    .min(child_total);
                if s1 <= s0 {
                    continue;
                }
                let channels = MIX_CHANNELS as usize;
                let slice: Vec<i16> = scratch[s0 * channels..s1 * channels]
                    .iter()
                    .map(|&x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16)
                    .collect();
                let pcm = resample_plain(&slice, n.rate);
                place_into_mix(
                    acc,
                    &pcm,
                    total,
                    n.start,
                    parent_len,
                    1.0,
                    0.0,
                    0.0,
                    &crate::envelope::SoundEnvelope::default(),
                    &[],
                    None,
                    &notation::model::SoundFx::default(),
                );
            }
            _ => {}
        }
    }
    visited[ci] = false;
    Ok(())
}

/// Whether one tree composition sounds within its clamped window
/// (Sprint 6 T007): an `audio` track with a non-empty window, a
/// `video` clip with a sound stream and a non-empty window, or a
/// `nest` use with a non-empty content window whose target sounds.
/// Mirrors [`place_comp_clips`] without decoding, so the scan never
/// misses a placement and never invents one the walk would skip.
fn subtree_audible(
    view: &crate::frame::NestView<'_>,
    videos: &[ProbedVideo],
    ci: usize,
    range_end: f64,
    depth: u8,
    visited: &mut Vec<bool>,
) -> bool {
    if depth > crate::frame::MAX_NEST_RENDER_DEPTH || visited[ci] {
        return false;
    }
    visited[ci] = true;
    let comp = view.comps[ci];
    let base = view.bases[ci];
    for (idx, clip) in comp.clips.iter().enumerate() {
        // Hidden clips are absent from the mix (Sprint 6 T009), so
        // they never count as audible — an all-hidden project renders
        // the silent track exactly like a soundless one.
        if clip.hidden() {
            continue;
        }
        let hit = match clip {
            Clip::Audio(a) => {
                let end = (a.start + a.duration).min(range_end);
                end > a.start && a.start < range_end
            }
            Clip::Video(v) => {
                let end = (v.start + v.duration).min(range_end);
                end > v.start
                    && v.start < range_end
                    && videos
                        .iter()
                        .any(|p| p.clip_idx == base + idx && p.has_audio)
            }
            Clip::Nest(n) => match view.resolve(&n.target) {
                None => false,
                Some(ti) => {
                    let child = view.comps[ti];
                    if !child.duration.is_finite() || child.duration <= 0.0 {
                        false
                    } else {
                        let content_end = n
                            .trim_end
                            .map(|e| e.min(child.duration))
                            .unwrap_or(child.duration);
                        if content_end <= n.trim_start {
                            false
                        } else {
                            let content_len = (content_end - n.trim_start) / n.rate;
                            let parent_len = n.duration.min(content_len);
                            parent_len > 0.0
                                && n.start < range_end
                                && subtree_audible(
                                    view,
                                    videos,
                                    ti,
                                    child.duration,
                                    depth + 1,
                                    visited,
                                )
                        }
                    }
                }
            },
            _ => false,
        };
        if hit {
            visited[ci] = false;
            return true;
        }
    }
    visited[ci] = false;
    false
}

/// Build the audio plan (audio stage, Sprint 3 T008/T010, windowed Sprint
/// 5 T002, nested Sprint 6 T007): smoke-decode every audible video
/// stream, then decode each audible source — every `audio` track with
/// its trim, every `video` clip's sound stream from its source window
/// at its `rate` (plainly resampled) with the clip's volume and fades,
/// plus every `nest` use's content window resampled at the use `rate`
/// (picture holds past the content while sound goes silent) — to 48 kHz
/// stereo PCM, place each sample-accurately at its written start, and
/// sum all sources in composition document order into one deterministic
/// staged mix file.
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
    let view = prep.nest_view();
    let videos = &prep.videos;
    smoke_audio_streams(ffmpeg_bin, videos, &view)?;
    if crate::cancel::is_cancelled() {
        return Err(crate::cancel::error("audio"));
    }

    // Audibility scan: an `audio` track with a non-empty window clamped
    // to the composition, a `video` clip with a sound stream and a
    // non-empty window, or a `nest` use sounding through its content.
    // Anything else contributes nothing.
    let mut scan_visited = vec![false; view.comps.len()];
    if !subtree_audible(&view, videos, 0, comp.duration, 0, &mut scan_visited) {
        // Fail closed when nothing audible can measure (Sprint 10
        // sound, CAP-009): a silent mix never scales to target.
        if comp.normalize.active() {
            return Err(silent_normalize_refusal(&comp.name));
        }
        return Ok(AudioPlan {
            video_segs: Vec::new(),
            mix_pcm: None,
            normalize: None,
            waveforms: Vec::new(),
        });
    }

    let total = (comp.duration * MIX_RATE as f64).ceil().max(1.0) as usize;
    let mut acc = vec![0.0f32; total * MIX_CHANNELS as usize];
    let mut visited = vec![false; view.comps.len()];
    place_comp_clips(
        &view,
        videos,
        folder,
        ffmpeg_bin,
        &mut acc,
        total,
        comp.duration,
        0,
        0,
        &mut visited,
    )?;
    // One overall loudness gain (Sprint 10 sound, CAP-009): planned
    // from the placed mix, applied before staging, so the staged file
    // the MP4 encodes already lands at −16 LUFS with true-peak
    // guarding. Silent mixes fail closed here with no staged file.
    let (gain, report) = if comp.normalize.active() {
        let (gain, report) = normalize_gain_for_acc(comp, &acc)?;
        for x in acc.iter_mut() {
            *x *= gain;
        }
        (gain, Some(report))
    } else {
        (1.0f32, None)
    };
    let waveforms = build_waveform_overlays(comp, videos, folder, ffmpeg_bin, total, gain)?;
    let mut bytes = Vec::with_capacity(acc.len() * 2);
    for &x in &acc {
        let v = (x.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    if let Some(parent) = mix_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "render refused: cannot create output directory {}: {e} (audio stage).",
                    parent.display()
                )
            })?;
        }
    }
    std::fs::write(mix_path, &bytes).map_err(|e| {
        format!(
            "render failed: cannot stage the audio mix {}: {e} (audio stage)",
            mix_path.display()
        )
    })?;
    Ok(AudioPlan {
        video_segs: Vec::new(),
        mix_pcm: Some(mix_path.to_path_buf()),
        normalize: report,
        waveforms,
    })
}

/// Probe only the sound facts of one video source (Sprint 10 sound,
/// CAP-008/CAP-011): whether an audio stream exists plus the container
/// duration, from the same `ffmpeg -i` facts the picture probe reads.
/// Picture streams are never required here (a soundless video still
/// probes: it simply reports no audio), and nothing is decoded, so
/// peak analysis stays cheap beside a full render.
fn probe_audio_source(
    ffmpeg_bin: &Path,
    abs: &Path,
    rel: &str,
) -> Result<(bool, Option<f64>), String> {
    let out = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-i", &abs.display().to_string()])
        .output()
        .map_err(|e| {
            format!("render refused: cannot probe video source {rel:?} ({e}); no MP4 was written")
        })?;
    let text = String::from_utf8_lossy(&out.stderr);
    if !text.contains("Input #0") {
        return Err(format!(
            "render refused: video source {rel:?} is unreadable (ffmpeg reports no input stream); no MP4 was written"
        ));
    }
    let mut has_audio = false;
    for line in text.lines() {
        if line.trim().starts_with("Stream #") && line.contains("Audio:") {
            has_audio = true;
            break;
        }
    }
    Ok((has_audio, parse_duration(&text)))
}

/// Build the 48 kHz stereo mix in memory for peak analysis (Sprint 10
/// sound, CAP-008/CAP-011): the same trims, rates, gains, fades,
/// envelopes, crossfades, ducking, and delay/reverb the staged render
/// mix uses — the same [`place_comp_clips`] walk, quantized the same
/// way — but with an audio-only probe table and no picture
/// preparation, so `check` and `waveform` can name exact peaks without
/// rendering. Normalizing compositions measure the post-gain mix, so
/// the preview peaks at exactly what the staged render encodes.
/// Returns `None` for a soundless composition. Silent normalization
/// fails closed ([`PreviewError::Silent`]); an undecodable source
/// fails naming the file ([`PreviewError::Failed`]); the caller
/// decides what each means (`check` skips the heat warning silently
/// and stays valid, `waveform` refuses the listing).
pub fn build_mix_preview(
    comp: &Composition,
    all: &[Composition],
    folder: &Path,
    ffmpeg_bin: &Path,
) -> Result<Option<Vec<i16>>, PreviewError> {
    let order = nest_render_order(comp, all).map_err(PreviewError::Failed)?;
    // Audio-only video table: presence plus the effective window from
    // the same source-window rule the picture path uses, with no
    // picture probe or decode.
    let mut videos: Vec<ProbedVideo> = Vec::new();
    let mut bases: Vec<usize> = Vec::new();
    let mut base = 0usize;
    for c in &order {
        bases.push(base);
        for (idx, clip) in c.clips.iter().enumerate() {
            if let Clip::Video(v) = clip {
                let abs = folder.join(&v.path);
                let rel = rel_name(folder, &abs);
                let (has_audio, duration) =
                    probe_audio_source(ffmpeg_bin, &abs, &rel).map_err(PreviewError::Failed)?;
                let (_, eff_end) = effective_source_window(v, duration);
                videos.push(ProbedVideo {
                    clip_idx: base + idx,
                    path: abs,
                    codec: String::new(),
                    transfer: crate::outputs::HdrTransfer::Sdr,
                    width: 0,
                    height: 0,
                    duration,
                    has_audio,
                    loop_clip: v.loop_clip,
                    rotation: 0.0,
                    win_start: v.trim_start,
                    win_len: eff_end.map(|e| (e - v.trim_start).max(0.0)),
                });
            }
        }
        base += c.clips.len();
    }
    let no_motions: &[ClipMotion] = &[];
    let motions: Vec<&[ClipMotion]> = order.iter().map(|_| no_motions).collect();
    let view = crate::frame::NestView {
        comps: order,
        motions,
        bases,
    };
    let mut scan_visited = vec![false; view.comps.len()];
    if !subtree_audible(&view, &videos, 0, comp.duration, 0, &mut scan_visited) {
        return Ok(None);
    }
    let total = (comp.duration * MIX_RATE as f64).ceil().max(1.0) as usize;
    let mut acc = vec![0.0f32; total * MIX_CHANNELS as usize];
    let mut visited = vec![false; view.comps.len()];
    place_comp_clips(
        &view,
        &videos,
        folder,
        ffmpeg_bin,
        &mut acc,
        total,
        comp.duration,
        0,
        0,
        &mut visited,
    )
    .map_err(PreviewError::Failed)?;
    // The preview carries the same overall gain the staged render
    // encodes, so every surface names the same peaks for the same
    // bytes; silent normalization fails closed here too.
    if comp.normalize.active() {
        match crate::loudness::normalize_gain_for_mix(&acc) {
            Ok((gain, _)) => {
                for x in acc.iter_mut() {
                    *x *= gain;
                }
            }
            Err(_) => return Err(PreviewError::Silent(silent_normalize_refusal(&comp.name))),
        }
    }
    let mut pcm = Vec::with_capacity(acc.len());
    for &x in &acc {
        pcm.push((x.clamp(-1.0, 1.0) * 32767.0).round() as i16);
    }
    Ok(Some(pcm))
}

/// Crop one staged 48 kHz stereo mix to a sub-range (Sprint 6 T008):
/// the ranged MP4's sound is exactly the full render's mix sliced to
/// `[start, end)`, sample-quantized the same way the mix places every
/// source. A silent composition stages no mix, so there is nothing to
/// crop. Rewrites the staged file in place; the caller reuses the same
/// audio plan with a ranged preparation.
pub fn crop_mix_to_range(mix_path: &Path, start: f64, end: f64) -> Result<(), String> {
    let bytes = std::fs::read(mix_path).map_err(|e| {
        format!(
            "render failed: cannot read the staged audio mix {}: {e} (audio stage)",
            mix_path.display()
        )
    })?;
    let channels = MIX_CHANNELS as usize;
    let total = bytes.len() / (2 * channels);
    let s0 = ((start * MIX_RATE as f64).round().max(0.0) as usize).min(total);
    let s1 = ((end * MIX_RATE as f64).round().max(0.0) as usize).min(total);
    if s1 <= s0 {
        return Err(format!(
            "render failed: sub-range {start:.3}s..{end:.3}s holds no staged mix samples (audio stage)"
        ));
    }
    let stride = 2 * channels;
    std::fs::write(&mix_path, &bytes[s0 * stride..s1 * stride]).map_err(|e| {
        format!(
            "render failed: cannot stage the ranged audio mix {}: {e} (audio stage)",
            mix_path.display()
        )
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Parallel frame drawing (Sprint 3 T003).
// ---------------------------------------------------------------------------

/// Shared immutable draw inputs for parallel workers (Sprint 3 T003).
/// Every field is `Sync` — unlike [`Prepared`], which also carries the
/// thread-local raster-face cache whose glyph faces are not `Sync` — so
/// `&DrawView` crosses threads while each worker owns its raster cache.
pub(crate) struct DrawView<'a> {
    videos: &'a [ProbedVideo],
    ranges: &'a [DecodedRange],
    images: &'a [DecodedImage],
    lotties: &'a [PreparedLottie],
    paths: &'a crate::path_paint::PathCache,
    layouts: &'a [Option<layout::Block>],
    dyn_layouts: &'a DynLayouts,
    captions: &'a crate::captions::CaptionLayouts,
    bitmap_fallback: bool,
    /// The render tree (Sprint 6 T007): the root plus every nested
    /// composition, so one frame description resolves every `nest`.
    nest: crate::frame::NestView<'a>,
    /// Composition time of output frame 0 (Sprint 6 T008): zero for a
    /// full render, the `--start` instant for a sub-range.
    time_offset: timebase::Time,
    /// Project folder (Sprint 8 look, T003): image fills stat their
    /// source here on every frame.
    folder: &'a Path,
}

impl Prepared {
    /// Borrow the thread-shareable draw inputs of this preparation.
    pub(crate) fn draw_view(&self) -> DrawView<'_> {
        DrawView {
            videos: &self.videos,
            ranges: &self.ranges,
            images: &self.images,
            lotties: &self.lotties,
            paths: &self.paths,
            layouts: &self.layouts,
            dyn_layouts: &self.dyn_layouts,
            captions: &self.caption_layouts,
            bitmap_fallback: self.bitmap_fallback,
            nest: self.nest_view(),
            time_offset: self.range_start,
            folder: self.folder.as_path(),
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
    let fps = view.nest.comps[0].fps;
    let end = view.time_offset + crate::motion_plan::frame_time(&fps, frame);
    match view.nest.comps[0].motion_blur {
        // Blur-free compositions draw through the exact pre-blur path.
        None => draw_sharp_frame_view(view, raster, backend, frame, end),
        Some(blur) => {
            let mut draw_at = |now| draw_sharp_frame_view(view, &mut *raster, backend, frame, now);
            crate::motion_blur::draw_blurred(&mut draw_at, end, &fps, &blur)
        }
    }
}

/// Draw one output frame at one exact composition instant (Sprint 11
/// motion blur, CAP-006/T007): the pre-blur path — the description at
/// `now` (plain, or transition-bridged when a bridge covers `now`),
/// drawn through the effective backend. Blur-free frames call this
/// with the frame-time instant directly, so they render byte-identical
/// to before.
fn draw_sharp_frame_view(
    view: &DrawView,
    raster: &mut RasterCache,
    backend: Backend,
    frame: u64,
    now: timebase::Time,
) -> Result<Vec<u8>, String> {
    let desc = crate::frame::describe_nested_at(&view.nest, 0, frame, now);
    let active = crate::transitions::active_transitions(view.nest.comps[0], desc.time);
    if active.is_empty() {
        return draw_plain_frame_view(view, raster, backend, &desc);
    }
    let planned = plan_transition_frame(&view.nest, 0, frame, &desc, &active);
    draw_planned_transition(
        view.videos,
        view.ranges,
        view.images,
        view.lotties,
        view.layouts,
        view.dyn_layouts,
        view.captions,
        view.bitmap_fallback,
        raster,
        view.paths,
        view.folder,
        backend,
        &planned,
    )
}

/// Draw one described frame with no bridge active (Sprint 11
/// transitions, CAP-005/T006): the exact pre-transition path, so
/// transition-free frames — and transition-free compositions — draw
/// byte-identical to before.
pub(crate) fn draw_plain_frame_view(
    view: &DrawView,
    raster: &mut RasterCache,
    backend: Backend,
    desc: &FrameDescription,
) -> Result<Vec<u8>, String> {
    match backend {
        Backend::Gpu => crate::gpu::render_description_split(
            view.videos,
            view.ranges,
            view.images,
            view.lotties,
            view.layouts,
            view.dyn_layouts,
            view.captions,
            view.bitmap_fallback,
            raster,
            view.paths,
            view.folder,
            desc,
        ),
        Backend::Cpu | Backend::Auto => {
            let mut out = vec![0u8; desc.width as usize * desc.height as usize * 3];
            draw_description(
                desc,
                raster,
                view.layouts,
                view.dyn_layouts,
                view.captions,
                view.bitmap_fallback,
                view.videos,
                view.ranges,
                view.images,
                view.lotties,
                view.paths,
                view.folder,
                &mut out,
            )?;
            Ok(out)
        }
    }
}

/// One bridge layer's isolated pictures: the outgoing side drawn at
/// its remapped tail instant and the incoming side at frame time,
/// each over flat black and flat white for coverage recovery.
pub(crate) struct PlannedTransitionLayer {
    pub kind: TransitionKind,
    pub progress: f64,
    pub a_black: FrameDescription,
    pub a_white: FrameDescription,
    pub b_black: FrameDescription,
    pub b_white: FrameDescription,
}

/// One bridged frame's pictures: the composition without the bridged
/// clips plus one isolated pair per active bridge.
pub(crate) struct PlannedTransitionFrame {
    pub base: FrameDescription,
    pub layers: Vec<PlannedTransitionLayer>,
}

/// Describe one bridged frame's pictures (Sprint 11 transitions,
/// CAP-005/T006): the base without any bridged clip, and per bridge
/// the outgoing tail replayed at its remapped instant plus the
/// incoming head at frame time. Descriptions are owned data, so the
/// caller draws them after this borrow ends.
pub(crate) fn plan_transition_frame(
    nest: &crate::frame::NestView,
    root: usize,
    frame: u64,
    desc: &FrameDescription,
    active: &[crate::transitions::ActiveTransition],
) -> PlannedTransitionFrame {
    fn flat(mut desc: FrameDescription, bg: Rgb) -> FrameDescription {
        // Flat ungraded backgrounds: coverage recovery compares
        // against the exact fill bytes, so the grade must not tint
        // the key colors.
        desc.background = bg;
        desc.composition_grade = ResolvedGrade::identity();
        desc
    }
    let black = Rgb { r: 0, g: 0, b: 0 };
    let white = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    let hidden: Vec<(usize, usize)> = active.iter().map(|a| (a.from, a.to)).collect();
    let base = crate::frame::describe_nested_at_filtered(
        nest,
        root,
        frame,
        desc.time,
        Some(&|i: usize| !hidden.iter().any(|(f, t)| *f == i || *t == i)),
    );
    let mut layers = Vec::with_capacity(active.len());
    for a in active {
        let from = a.from;
        let only_a = crate::frame::describe_nested_at_filtered(
            nest,
            root,
            frame,
            a.from_time,
            Some(&|i: usize| i == from),
        );
        let to = a.to;
        let only_b = crate::frame::describe_nested_at_filtered(
            nest,
            root,
            frame,
            desc.time,
            Some(&|i: usize| i == to),
        );
        layers.push(PlannedTransitionLayer {
            kind: a.kind,
            progress: a.progress,
            a_black: flat(only_a.clone(), black),
            a_white: flat(only_a, white),
            b_black: flat(only_b.clone(), black),
            b_white: flat(only_b, white),
        });
    }
    PlannedTransitionFrame { base, layers }
}

/// Draw one bridged frame's pictures and composite them (Sprint 11
/// transitions, CAP-005/T006): the base plus every layer's
/// black/white pairs draw through the same backend traversal as
/// ordinary frames, then the blend composites over the base. Drawing
/// needs no render tree, only the planned descriptions plus media.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_planned_transition(
    videos: &[ProbedVideo],
    ranges: &[DecodedRange],
    images: &[DecodedImage],
    lotties: &[PreparedLottie],
    layouts: &[Option<layout::Block>],
    dyn_layouts: &DynLayouts,
    captions: &crate::captions::CaptionLayouts,
    bitmap_fallback: bool,
    raster: &mut RasterCache,
    paths: &crate::path_paint::PathCache,
    folder: &Path,
    backend: Backend,
    planned: &PlannedTransitionFrame,
) -> Result<Vec<u8>, String> {
    let mut draw = |desc: &FrameDescription| -> Result<Vec<u8>, String> {
        match backend {
            Backend::Gpu => crate::gpu::render_description_split(
                videos,
                ranges,
                images,
                lotties,
                layouts,
                dyn_layouts,
                captions,
                bitmap_fallback,
                raster,
                paths,
                folder,
                desc,
            ),
            Backend::Cpu | Backend::Auto => {
                let mut out = vec![0u8; desc.width as usize * desc.height as usize * 3];
                draw_description(
                    desc,
                    raster,
                    layouts,
                    dyn_layouts,
                    captions,
                    bitmap_fallback,
                    videos,
                    ranges,
                    images,
                    lotties,
                    paths,
                    folder,
                    &mut out,
                )?;
                Ok(out)
            }
        }
    };
    let base = draw(&planned.base)?;
    let (w, h) = (planned.base.width as usize, planned.base.height as usize);
    // Owned pair buffers first (the draw above borrows the raster
    // mutably, so layers can only borrow the finished buffers).
    let mut bufs: Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
    for layer in &planned.layers {
        bufs.push((
            draw(&layer.a_black)?,
            draw(&layer.a_white)?,
            draw(&layer.b_black)?,
            draw(&layer.b_white)?,
        ));
    }
    let layers: Vec<crate::transitions::TransitionLayer<'_>> = planned
        .layers
        .iter()
        .zip(bufs.iter())
        .map(
            |(layer, (a_black, a_white, b_black, b_white))| crate::transitions::TransitionLayer {
                kind: layer.kind,
                progress: layer.progress,
                a_black,
                a_white,
                b_black,
                b_white,
            },
        )
        .collect();
    Ok(crate::transitions::composite_transition_frame(
        &base, w, h, &layers,
    ))
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
                    Err(_) => {
                        Err("worker failed: a frame worker panicked; no MP4 was written"
                            .to_string())
                    }
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

/// Remove staged sequence work (the temp directory plus the staged
/// audio mix, when present) after a failed sequence render, so no
/// partial directory survives at the target.
fn remove_staged_dir(stage_dir: &Path, plan: &AudioPlan) {
    let _ = std::fs::remove_dir_all(stage_dir);
    if let Some(mix) = &plan.mix_pcm {
        let _ = std::fs::remove_file(mix);
    }
}

/// Staged files that must go on cancellation: the encode temp file and
/// the staged mix, when present.
fn staged_extras(plan: &AudioPlan) -> Vec<PathBuf> {
    plan.mix_pcm.clone().into_iter().collect()
}

/// Cancel the in-progress encode: stop the FFmpeg child, remove staged
/// and partial files, and report the interruption with any leftover
/// paths that could not be removed.
fn cancel_encode(
    child: &mut std::process::Child,
    out: &Path,
    plan: &AudioPlan,
    stage: &str,
) -> String {
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

/// Encode-failure wording for one destination (Sprint 12 outputs,
/// T001): the historical `mp4` suffix stays byte-identical, every other
/// format names the same failure with `output` instead of `MP4`.
fn no_output_suffix(format: crate::outputs::OutputFormat) -> &'static str {
    match format {
        crate::outputs::OutputFormat::Mp4 => "; no MP4 was written",
        _ => "; no output was written",
    }
}

/// Attach the destination-aware failure suffix, replacing the
/// historical MP4 suffix when a drawn-frame error already carries it
/// (so non-MP4 renders never claim an MP4).
fn fail_suffix(mut e: String, format: crate::outputs::OutputFormat) -> String {
    if format != crate::outputs::OutputFormat::Mp4 {
        if let Some(base) = e.strip_suffix("; no MP4 was written") {
            e = base.to_string();
        }
    }
    e + no_output_suffix(format)
}

/// Cancellation report for one destination (Sprint 12 outputs, T001):
/// the historical MP4 wording for `mp4`, the output wording otherwise.
fn cancel_report_for(
    stage: &str,
    leftovers: &[PathBuf],
    format: crate::outputs::OutputFormat,
) -> String {
    if leftovers.is_empty() {
        return match format {
            crate::outputs::OutputFormat::Mp4 => crate::cancel::error(stage),
            _ => format!(
                "{} render {stage} interrupted by Ctrl-C (SIGINT); no output was written",
                crate::cancel::PREFIX
            ),
        };
    }
    let names: Vec<String> = leftovers.iter().map(|p| p.display().to_string()).collect();
    match format {
        crate::outputs::OutputFormat::Mp4 => cancel_report(stage, leftovers),
        _ => format!(
            "{} render {stage} interrupted by Ctrl-C (SIGINT); cleanup incomplete, leftover path(s): {}; no output was written",
            crate::cancel::PREFIX,
            names.join(", ")
        ),
    }
}

/// Cancel the in-progress sequence encode: stop the FFmpeg child and
/// remove the staging directory plus the staged mix, so no partial
/// sequence survives.
fn cancel_encode_seq(
    child: &mut std::process::Child,
    staged: &crate::outputs::StagedTarget,
    plan: &AudioPlan,
    stage: &str,
    format: crate::outputs::OutputFormat,
) -> String {
    let _ = child.kill();
    let _ = child.wait();
    let (stage_dir, extra) = match staged {
        crate::outputs::StagedTarget::File { .. } => {
            unreachable!("sequence cancel on a file target")
        }
        crate::outputs::StagedTarget::PngSeq { stage_dir, .. }
        | crate::outputs::StagedTarget::Hls { stage_dir, .. } => {
            (stage_dir.clone(), staged_extras(plan))
        }
    };
    let _ = std::fs::remove_dir_all(&stage_dir);
    let mut leftovers = Vec::new();
    for t in extra {
        match std::fs::remove_file(&t) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => leftovers.push(t),
        }
    }
    if std::fs::remove_dir_all(&stage_dir).is_ok() {
        // Already removed above; a reappearing directory is a leftover.
    }
    cancel_report_for(stage, &leftovers, format)
}

/// Remove sequence staging plus the staged mix after a failed encode,
/// so no partial directory survives at the target.
fn fail_sequence(staged: &crate::outputs::StagedTarget, plan: &AudioPlan) {
    match staged {
        crate::outputs::StagedTarget::File { .. } => {
            unreachable!("sequence cleanup on a file target")
        }
        crate::outputs::StagedTarget::PngSeq { stage_dir, .. }
        | crate::outputs::StagedTarget::Hls { stage_dir, .. } => {
            remove_staged_dir(stage_dir, plan);
        }
    }
}

/// Move one staged file into the final destination directory,
/// creating it when needed (Sprint 12 outputs, T001).
fn move_staged_file(stage_dir: &Path, dir: &Path, name: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| {
        format!(
            "render failed: cannot create output directory {}: {e} (output stage)",
            dir.display()
        )
    })?;
    std::fs::rename(stage_dir.join(name), dir.join(name)).map_err(|e| {
        format!("render failed: cannot move the encoded output into place: {e} (output stage)")
    })?;
    Ok(())
}

/// Verify staged PNG frames and move them into place (Sprint 12
/// outputs, T001): the frame count must equal the composition frame
/// count exactly, or the render fails with no partial directory left
/// behind.
fn finalize_png_seq(
    stage_dir: &Path,
    dir: &Path,
    frames: usize,
    plan: &AudioPlan,
    format: crate::outputs::OutputFormat,
) -> Result<(), String> {
    let mut names: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(stage_dir).map_err(|e| {
        fail_sequence(
            &crate::outputs::StagedTarget::PngSeq {
                stage_dir: stage_dir.to_path_buf(),
                dir: dir.to_path_buf(),
                frames,
            },
            plan,
        );
        format!("render failed: cannot list the staged PNG frames: {e} (encode stage)")
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            format!("render failed: cannot list the staged PNG frames: {e} (encode stage)")
        })?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    let expected: Vec<String> = (0..frames).map(|i| format!("frame-{i:04}.png")).collect();
    if names != expected {
        fail_sequence(
            &crate::outputs::StagedTarget::PngSeq {
                stage_dir: stage_dir.to_path_buf(),
                dir: dir.to_path_buf(),
                frames,
            },
            plan,
        );
        return Err(fail_suffix(
            format!(
                "render failed: the PNG sequence holds {} frame(s), expected {} (encode stage)",
                names.len(),
                frames
            ),
            format,
        ));
    }
    for name in &names {
        if let Err(e) = move_staged_file(stage_dir, dir, name) {
            fail_sequence(
                &crate::outputs::StagedTarget::PngSeq {
                    stage_dir: stage_dir.to_path_buf(),
                    dir: dir.to_path_buf(),
                    frames,
                },
                plan,
            );
            return Err(e);
        }
    }
    let _ = std::fs::remove_dir_all(stage_dir);
    if let Some(mix) = &plan.mix_pcm {
        let _ = std::fs::remove_file(mix);
    }
    Ok(())
}

/// Verify the staged HLS playlist plus segments and move them into
/// place (Sprint 12 outputs, T001): every segment the playlist names
/// must exist, or the render fails leaving no playlist behind.
fn finalize_hls(
    stage_dir: &Path,
    dir: &Path,
    playlist_name: &str,
    plan: &AudioPlan,
    format: crate::outputs::OutputFormat,
) -> Result<usize, String> {
    let staged_playlist = stage_dir.join(playlist_name);
    let body = std::fs::read_to_string(&staged_playlist).map_err(|e| {
        fail_sequence(
            &crate::outputs::StagedTarget::Hls {
                stage_dir: stage_dir.to_path_buf(),
                dir: dir.to_path_buf(),
                playlist_name: playlist_name.to_string(),
            },
            plan,
        );
        format!("render failed: the HLS playlist was not written: {e} (encode stage)")
    })?;
    let mut segments: Vec<String> = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        segments.push(line.to_string());
    }
    if segments.is_empty() {
        fail_sequence(
            &crate::outputs::StagedTarget::Hls {
                stage_dir: stage_dir.to_path_buf(),
                dir: dir.to_path_buf(),
                playlist_name: playlist_name.to_string(),
            },
            plan,
        );
        return Err(fail_suffix(
            "render failed: the HLS playlist names no segments (encode stage)".to_string(),
            format,
        ));
    }
    for seg in &segments {
        if !stage_dir.join(seg).is_file() {
            fail_sequence(
                &crate::outputs::StagedTarget::Hls {
                    stage_dir: stage_dir.to_path_buf(),
                    dir: dir.to_path_buf(),
                    playlist_name: playlist_name.to_string(),
                },
                plan,
            );
            return Err(fail_suffix(
                format!("render failed: the HLS playlist names {seg:?}, which was not written (encode stage)"),
                format,
            ));
        }
    }
    for seg in &segments {
        if let Err(e) = move_staged_file(stage_dir, dir, seg) {
            fail_sequence(
                &crate::outputs::StagedTarget::Hls {
                    stage_dir: stage_dir.to_path_buf(),
                    dir: dir.to_path_buf(),
                    playlist_name: playlist_name.to_string(),
                },
                plan,
            );
            return Err(e);
        }
    }
    if let Err(e) = move_staged_file(stage_dir, dir, playlist_name) {
        fail_sequence(
            &crate::outputs::StagedTarget::Hls {
                stage_dir: stage_dir.to_path_buf(),
                dir: dir.to_path_buf(),
                playlist_name: playlist_name.to_string(),
            },
            plan,
        );
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(stage_dir);
    if let Some(mix) = &plan.mix_pcm {
        let _ = std::fs::remove_file(mix);
    }
    Ok(segments.len())
}

/// Draw every frame and encode the destination (frames + encode
/// stages, Sprint 3 T003/T006/T008, Sprint 12 T001): frames are drawn
/// in parallel in bounded batches and re-emitted in frame order, the
/// staged mix (when present) joins the encoder inputs of destinations
/// that carry audio, and the finished file (or PNG frames, or HLS
/// playlist plus segments) moves into place only on success.
/// Returns the human success line plus the measured frames and encode
/// durations in milliseconds.
pub fn render_staged(
    prep: &Prepared,
    plan: &AudioPlan,
    opts: &RenderOptions,
    ffmpeg_bin: &Path,
    dest: &crate::outputs::ResolvedOutput,
    scale: &crate::scale::ScalePlan,
) -> Result<(String, u64, u64), String> {
    render_staged_with_progress(prep, plan, opts, ffmpeg_bin, dest, scale, None)
}

/// Draw and encode with measured frame progress (Sprint 18, T006):
/// after every emitted batch the hook observes `(frames_done,
/// frames_total)` on the calling thread, so the Renders panel reports
/// measured progress with an estimated remainder. `None` skips the
/// hook, so every existing caller renders exactly as before.
pub fn render_staged_with_progress(
    prep: &Prepared,
    plan: &AudioPlan,
    opts: &RenderOptions,
    ffmpeg_bin: &Path,
    dest: &crate::outputs::ResolvedOutput,
    scale: &crate::scale::ScalePlan,
    progress: Option<&dyn Fn(u64, u64)>,
) -> Result<(String, u64, u64), String> {
    use std::io::Write;
    use std::time::Instant;
    let comp = prep.composition();
    let frames = prep.frames;
    let format = opts.format;

    // Stage the encode target first: a temp file for single files, a
    // temp directory for sequences. Only the finalize step moves
    // staged results into place, so a failed render never leaves a
    // partial file, playlist, or frame behind.
    let staged = crate::outputs::StagedTarget::stage(dest, frames);
    let final_file: Option<PathBuf> = match &staged {
        crate::outputs::StagedTarget::File { final_path, .. } => Some(final_path.clone()),
        _ => None,
    };
    match &staged {
        crate::outputs::StagedTarget::File { tmp, .. } => {
            if let Some(parent) = tmp.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        if let Some(out) = &final_file {
                            remove_work_files(out, plan);
                        }
                        format!(
                            "render refused: cannot create output directory {}: {e}.",
                            parent.display()
                        )
                    })?;
                }
            }
        }
        crate::outputs::StagedTarget::PngSeq { stage_dir, .. }
        | crate::outputs::StagedTarget::Hls { stage_dir, .. } => {
            let _ = std::fs::remove_dir_all(stage_dir);
            std::fs::create_dir_all(stage_dir).map_err(|e| {
                fail_sequence(&staged, plan);
                format!(
                    "render refused: cannot create output directory {}: {e}.",
                    stage_dir.display()
                )
            })?;
        }
    }
    let cleanup_file = |plan: &AudioPlan| {
        if let Some(out) = &final_file {
            remove_work_files(out, plan);
        }
    };
    if crate::cancel::is_cancelled() {
        match &staged {
            crate::outputs::StagedTarget::File { .. } => {
                let leftovers = crate::cancel::remove_staged(
                    final_file.as_deref().unwrap(),
                    &staged_extras(plan),
                );
                return Err(cancel_report_for("frames/encode", &leftovers, format));
            }
            seq => {
                fail_sequence(seq, plan);
                return Err(cancel_report_for("frames/encode", &[], format));
            }
        }
    }
    let enc = opts.encoder();
    // Every destination pipes the same `rgb24` frames at the output
    // size: alpha composites at draw time, so pictures match the MP4
    // on every format (Sprint 12 outputs, T001/T002).
    let args = crate::outputs::encode_args_for(
        format,
        opts.prores_profile,
        comp,
        &plan.video_segs,
        plan.mix_pcm.as_deref(),
        &staged,
        &enc,
        prep.out_duration,
        (scale.out_w, scale.out_h),
    );
    let frames_start = Instant::now();
    let mut child = Command::new(ffmpeg_bin)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if crate::cancel::is_cancelled() {
                return match &staged {
                    crate::outputs::StagedTarget::File { .. } => {
                        let leftovers = crate::cancel::remove_staged(
                            final_file.as_deref().unwrap(),
                            &staged_extras(plan),
                        );
                        cancel_report_for("frames/encode", &leftovers, format)
                    }
                    seq => {
                        fail_sequence(seq, plan);
                        cancel_report_for("frames/encode", &[], format)
                    }
                };
            }
            match &staged {
                crate::outputs::StagedTarget::File { .. } => cleanup_file(plan),
                seq => fail_sequence(seq, plan),
            }
            format!("render failed: could not launch ffmpeg (encode stage): {e}")
        })?;
    let stdin = child.stdin.take();
    let mut stdin = stdin.ok_or_else(|| {
        match &staged {
            crate::outputs::StagedTarget::File { .. } => cleanup_file(plan),
            seq => fail_sequence(seq, plan),
        }
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
            match &staged {
                crate::outputs::StagedTarget::File { .. } => {
                    return Err(cancel_encode(
                        &mut child,
                        final_file.as_deref().unwrap(),
                        plan,
                        "frames",
                    ));
                }
                seq => return Err(cancel_encode_seq(&mut child, seq, plan, "frames", format)),
            }
        }
        let slots = match draw_batch_parallel(prep, opts.backend, batch) {
            Ok(slots) => slots,
            Err(e) => {
                if crate::cancel::is_cancelled() {
                    drop(stdin);
                    match &staged {
                        crate::outputs::StagedTarget::File { .. } => {
                            return Err(cancel_encode(
                                &mut child,
                                final_file.as_deref().unwrap(),
                                plan,
                                "frames",
                            ));
                        }
                        seq => {
                            return Err(cancel_encode_seq(&mut child, seq, plan, "frames", format));
                        }
                    }
                }
                drop(stdin);
                let _ = child.wait();
                match &staged {
                    crate::outputs::StagedTarget::File { .. } => cleanup_file(plan),
                    seq => fail_sequence(seq, plan),
                }
                return Err(fail_suffix(e, format));
            }
        };
        // On-canvas sound shapes (Sprint 10 sound, CAP-010): the
        // static drawing for the drawn range covers every rendered
        // frame at its own instant, on either backend, so ranged
        // renders match the full render's frames sample for sample.
        let draw_sound = !plan.waveforms.is_empty();
        for (f, bytes) in slots {
            let mut bytes = bytes;
            if draw_sound {
                let t = (prep.range_start + crate::motion_plan::frame_time(&comp.fps, f)).to_f64();
                crate::waveform::draw_waveform_overlay(
                    &mut bytes,
                    comp.width,
                    comp.height,
                    t,
                    &plan.waveforms,
                );
            }
            // Scaled renders (Sprint 12 outputs, T002) draw at the
            // internal size, then downscale to the output size before
            // piping: the encoder always sees output-size frames. The
            // identity path skips this entirely (no scaling work).
            let bytes = if scale.needs_downscale() {
                crate::scale::downscale_rgb24(
                    &bytes,
                    scale.internal_w,
                    scale.internal_h,
                    scale.out_w,
                    scale.out_h,
                )
            } else {
                bytes
            };
            if !write_failed {
                if let Err(e) = stdin.write_all(&bytes) {
                    write_failed = true;
                    let _ = e;
                }
            }
        }
        if crate::cancel::is_cancelled() {
            drop(stdin);
            match &staged {
                crate::outputs::StagedTarget::File { .. } => {
                    return Err(cancel_encode(
                        &mut child,
                        final_file.as_deref().unwrap(),
                        plan,
                        "frames",
                    ));
                }
                seq => return Err(cancel_encode_seq(&mut child, seq, plan, "frames", format)),
            }
        }
        start = end;
        if let Some(report) = progress {
            report(end, total);
        }
    }
    drop(stdin);
    let frames_ms = frames_start.elapsed().as_millis() as u64;
    let encode_start = Instant::now();
    let output = child.wait_with_output().map_err(|e| {
        if crate::cancel::is_cancelled() {
            return match &staged {
                crate::outputs::StagedTarget::File { .. } => {
                    let leftovers = crate::cancel::remove_staged(
                        final_file.as_deref().unwrap(),
                        &staged_extras(plan),
                    );
                    cancel_report_for("encode", &leftovers, format)
                }
                seq => {
                    fail_sequence(seq, plan);
                    cancel_report_for("encode", &[], format)
                }
            };
        }
        match &staged {
            crate::outputs::StagedTarget::File { .. } => cleanup_file(plan),
            seq => fail_sequence(seq, plan),
        }
        format!("render failed: could not read ffmpeg status (encode stage): {e}")
    })?;
    // A SIGINT that kills the encoder looks like an encode failure;
    // the flag tells the two apart so cancellation exits 130.
    if crate::cancel::is_cancelled() {
        return match &staged {
            crate::outputs::StagedTarget::File { .. } => {
                let leftovers = crate::cancel::remove_staged(
                    final_file.as_deref().unwrap(),
                    &staged_extras(plan),
                );
                Err(cancel_report_for("encode", &leftovers, format))
            }
            seq => {
                fail_sequence(seq, plan);
                Err(cancel_report_for("encode", &[], format))
            }
        };
    }
    if !output.status.success() {
        match &staged {
            crate::outputs::StagedTarget::File { .. } => cleanup_file(plan),
            seq => fail_sequence(seq, plan),
        }
        let tail = stderr_tail(&output.stderr, 5);
        return Err(fail_suffix(
            format!(
                "render failed: ffmpeg encode stage exited with status {}{}",
                output.status,
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(": {tail}")
                }
            ),
            format,
        ));
    }
    // The encoder must have produced output; an empty file is a failure.
    let produced: u64 = match &staged {
        crate::outputs::StagedTarget::File { tmp, .. } => {
            std::fs::metadata(tmp).map(|m| m.len()).unwrap_or(0)
        }
        crate::outputs::StagedTarget::PngSeq { stage_dir, .. } => std::fs::read_dir(stage_dir)
            .map(|entries| entries.count() as u64)
            .unwrap_or(0),
        crate::outputs::StagedTarget::Hls { stage_dir, .. } => std::fs::read_dir(stage_dir)
            .map(|entries| entries.count() as u64)
            .unwrap_or(0),
    };
    if produced == 0 {
        match &staged {
            crate::outputs::StagedTarget::File { .. } => cleanup_file(plan),
            seq => fail_sequence(seq, plan),
        }
        return Err(fail_suffix(
            "render failed: ffmpeg encode stage wrote no data".to_string(),
            format,
        ));
    }
    // Move the staged result into place only on success: the finished
    // file is renamed, PNG frames and HLS segments move file by file
    // after their counts and references verify.
    let detail: Option<String> = match &staged {
        crate::outputs::StagedTarget::File { tmp, .. } => {
            let out = final_file.as_deref().unwrap();
            if let Err(e) = std::fs::rename(tmp, out) {
                cleanup_file(plan);
                return Err(
                    match format {
                        crate::outputs::OutputFormat::Mp4 => format!(
                            "render failed: cannot move the encoded MP4 into place: {e} (output stage)"
                        ),
                        _ => format!(
                            "render failed: cannot move the encoded output into place: {e} (output stage)"
                        ),
                    },
                );
            }
            // The staged mix is a work file: only the destination plus
            // the run log record remain after a render.
            if let Some(mix) = &plan.mix_pcm {
                let _ = std::fs::remove_file(mix);
            }
            // ProRes names its profile in the success line so the
            // delivered file is reviewable against the request.
            match format {
                crate::outputs::OutputFormat::Prores => {
                    Some(format!("profile={}", opts.prores_profile.name()))
                }
                _ => None,
            }
        }
        crate::outputs::StagedTarget::PngSeq { stage_dir, dir, .. } => {
            finalize_png_seq(stage_dir, dir, frames, plan, format)?;
            Some(format!("{frames} frames"))
        }
        crate::outputs::StagedTarget::Hls {
            stage_dir,
            dir,
            playlist_name,
            ..
        } => {
            let n = finalize_hls(stage_dir, dir, playlist_name, plan, format)?;
            Some(format!("{n} segment{}", if n == 1 { "" } else { "s" }))
        }
    };
    let encode_ms = encode_start.elapsed().as_millis() as u64;
    let text_drawn = prep.layouts.iter().any(|laid| laid.is_some()) || prep.bitmap_fallback;
    let shown = dest.display_path().display().to_string();
    let origin = origin_note(
        prep.project_family.is_some(),
        &prep.fonts,
        prep.bitmap_fallback,
        text_drawn,
    );
    // The success line names the OUTPUT size (what the probe sees):
    // the canvas size on the identity path (the pinned line exactly),
    // the preset size on a scaled render.
    // Encoder and rate-control tail (Sprint 12 outputs, T003): empty
    // on the default path, so the pinned lines stay exact; otherwise
    // the choice and pacing that wrote the stream.
    let extra = opts
        .announce_extra()
        .map(|x| format!(", {x}"))
        .unwrap_or_default();
    let done_msg = match format {
        // Pinned: the historical success line exactly.
        crate::outputs::OutputFormat::Mp4 => format!(
            "rendered {} ({}x{}, {}s){} [backend={}, {}{}].",
            shown,
            scale.out_w,
            scale.out_h,
            prep.out_duration,
            origin,
            opts.backend.name(),
            opts.announce(),
            extra
        ),
        _ => format!(
            "rendered {} ({}x{}, {}s){} [{}format={}, backend={}, {}{}].",
            shown,
            scale.out_w,
            scale.out_h,
            prep.out_duration,
            origin,
            detail.map(|d| format!("{d}, ")).unwrap_or_default(),
            format.name(),
            opts.backend.name(),
            opts.announce(),
            extra
        ),
    };
    Ok((done_msg, frames_ms, encode_ms))
}

/// Render one validated composition to `out` using `ffmpeg_bin`.
/// Returns the human success line on success, or a refusal/failure
/// message (the caller decides the exit code). Never leaves a partial
/// MP4 at `out` on failure. `all` carries every validated composition
/// so `nest` targets resolve (Sprint 6 T007).
///
/// Frames are computed in bounded consecutive batches (round-robin
/// within each batch, see [`worker_order`]) and emitted to the encoder
/// in frame order, so any worker count draws byte-identical pictures.
pub fn render_composition(
    comp: &Composition,
    all: &[Composition],
    folder: &Path,
    ffmpeg_bin: &Path,
    out: &Path,
) -> Result<String, String> {
    // Default path (CPU backend, standard preset, no overrides) used by
    // non-CLI callers: the staged pipeline with measured stages that
    // the CLI would print one by one.
    let opts = RenderOptions::default();
    render_with_options(comp, all, folder, ffmpeg_bin, out, &opts).map(|(msg, _, _)| msg)
}

/// Render one validated composition with explicit Sprint 3 options.
/// Returns the human success line plus the measured frames and encode
/// stage durations. Staged mix and encode work files are always removed.
/// `all` carries every validated composition so `nest` targets resolve.
pub fn render_with_options(
    comp: &Composition,
    all: &[Composition],
    folder: &Path,
    ffmpeg_bin: &Path,
    out: &Path,
    opts: &RenderOptions,
) -> Result<(String, u64, u64), String> {
    render_with_options_progress(comp, all, folder, ffmpeg_bin, out, opts, None)
}

/// Render with measured frame progress (Sprint 18, T006): the hook
/// observes `(frames_done, frames_total)` after every emitted batch,
/// so the Renders panel reports measured progress. `None` renders
/// exactly as [`render_with_options`].
pub fn render_with_options_progress(
    comp: &Composition,
    all: &[Composition],
    folder: &Path,
    ffmpeg_bin: &Path,
    out: &Path,
    opts: &RenderOptions,
    progress: Option<&dyn Fn(u64, u64)>,
) -> Result<(String, u64, u64), String> {
    let prep = prepare_frames(comp, all, folder, ffmpeg_bin)?;
    let mix_path = mix_staging_path(out);
    let plan = match build_audio_plan(comp, folder, ffmpeg_bin, &prep, &mix_path) {
        Ok(plan) => plan,
        Err(e) => {
            // The staged mix may exist already: never leave it behind.
            let _ = std::fs::remove_file(&mix_path);
            return Err(e);
        }
    };
    // Non-CLI callers render the historical MP4 file exactly (the
    // identity scale plan: no scaling work, output is the canvas).
    let dest = crate::outputs::ResolvedOutput::File {
        path: out.to_path_buf(),
    };
    let scale = crate::scale::ScalePlan::identity(comp.width, comp.height);
    render_staged_with_progress(&prep, &plan, opts, ffmpeg_bin, &dest, &scale, progress)
}

/// Success-line suffix naming the text origins, never silent: the
/// project font needs no note; otherwise the used system files join in
/// registration order, or the bitmap last resort is announced. A
/// composition drawing no text carries no note, as before.
pub(crate) fn origin_note(
    project: bool,
    fonts: &layout::Fonts,
    bitmap: bool,
    text_drawn: bool,
) -> String {
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
pub(crate) fn raster_face<'a>(
    raster: &'a mut RasterCache,
    font: &std::sync::Arc<Vec<u8>>,
    index: u32,
) -> Result<&'a Face, String> {
    let key = (std::sync::Arc::as_ptr(font) as usize, index);
    if !raster.contains_key(&key) {
        let face = Face::load_bytes(font)
            .map_err(|e| format!("render failed: shaped font cannot rasterize ({e})"))?;
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

/// Last `n` non-empty stderr lines, joined for the stage report.
pub(crate) fn stderr_tail(stderr: &[u8], n: usize) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join(" | ").chars().take(600).collect()
}

/// Raw frame input arguments: piped `rawvideo` frames arrive on stdin
/// as input 0 at the output frame size and composition rate (Sprint 12
/// outputs, T001/T002). The output size is the canvas size on the
/// identity path and the preset size on a scaled render (vectors
/// re-rasterize at the internal size, then downscale to this size
/// before piping). Every destination pipes `rgb24`: alpha composites
/// at draw time, so pictures match the MP4 on every format.
pub(crate) fn encode_input_args(
    comp: &Composition,
    pix_fmt: &str,
    frame: (u32, u32),
) -> Vec<String> {
    vec![
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        pix_fmt.into(),
        "-s".into(),
        format!("{}x{}", frame.0, frame.1),
        "-r".into(),
        comp.fps.to_string(),
        "-i".into(),
        "-".into(),
    ]
}

/// Audio inputs plus the mix filtergraph (shared by every destination
/// that carries audio): each audible video source adds an input with
/// trim/delay filters, and the staged Framesmith mix (when present)
/// arrives as raw s16le 48 kHz stereo. `duration` is the output media
/// length in seconds: the composition duration for a full render, the
/// range length for a sub-range render (Sprint 6 T008). The staged mix
/// (when present) is already cropped to that length; the `atrim` and
/// `-t` guards only pin the exact container length.
pub(crate) fn push_audio_args(
    args: &mut Vec<String>,
    audio: &[AudioSeg],
    mix: Option<&Path>,
    duration: f64,
) {
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
            fc.push_str(&format!("[{m}:a]apad,atrim=duration={:.6}[mix];", duration));
        }
        let n_inputs = audio.len() + if mix_input.is_some() { 1 } else { 0 };
        if n_inputs == 1 {
            if mix_input.is_some() {
                fc.push_str(&format!("[mix]apad,atrim=duration={:.6}[outa]", duration));
            } else {
                fc.push_str(&format!("[s0]apad,atrim=duration={:.6}[outa]", duration));
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
                duration
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
        args.extend(["-map".into(), "0:v".into(), "-map".into(), "1:a".into()]);
    }
}

/// True when the active preset emits no explicit encoder flags: the
/// `standard` preset with no overrides, so default bytes match the
/// Sprint 2 output exactly. Every other preset or override sets only
/// its own axis.
pub(crate) fn standard_quiet(enc: &ResolvedEncoder) -> bool {
    enc.from_preset == Preset::Standard
        && enc.video_bitrate.is_none()
        && Some(enc.crf) == Some(Preset::Standard.encoder().crf)
        && enc.x264_preset.is_none()
}

/// Build the ffmpeg encode arguments for the standard MP4 (Sprint 12
/// outputs, T001: `--format mp4`, the default). The `standard` preset
/// with no overrides emits no explicit encoder flags, so its bytes
/// match the Sprint 2 output exactly; every other preset or override
/// sets only its own axis.
pub(crate) fn encode_args(
    comp: &Composition,
    audio: &[AudioSeg],
    mix: Option<&Path>,
    tmp: &Path,
    enc: &ResolvedEncoder,
    duration: f64,
    frame: (u32, u32),
) -> Vec<String> {
    let mut args = encode_input_args(comp, "rgb24", frame);
    push_audio_args(&mut args, audio, mix, duration);
    // The hardware path drops the x264 quality axes (`-crf`,
    // `-preset`): VideoToolbox tunes quality through bitrate flags
    // instead, and unknown encoder options fail the render.
    let hw = enc.hardware;
    // Preset and overrides: each flag sets only its own axis.
    if !hw && !standard_quiet(enc) {
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
    // Rate control (Sprint 12 outputs, CAP-003): absent flags emit
    // nothing, keeping default bytes identical.
    if let Some(g) = enc.keyint {
        args.push("-g".into());
        args.push(g.to_string());
    }
    if let Some(m) = &enc.maxrate {
        args.push("-maxrate".into());
        args.push(m.clone());
        args.push("-bufsize".into());
        args.push(crate::outputs::maxrate_bufsize(m));
    }
    let video_codec = if hw {
        enc.hw_codec
            .or_else(|| {
                crate::outputs::hardware_encoder_for_choice(
                    crate::outputs::OutputFormat::Mp4,
                    enc.encoder_choice,
                )
            })
            .unwrap_or("h264_videotoolbox")
    } else {
        "libx264"
    };
    args.extend([
        "-c:v".into(),
        video_codec.into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-ac".into(),
        "2".into(),
        "-ar".into(),
        "48000".into(),
    ]);
    // Provenance (Sprint 12 outputs, CAP-005): the stamp rides a
    // container tag, never the pictures. None keeps the pinned argv
    // exactly (library callers and unit fixtures).
    if let Some(prov) = &enc.provenance {
        args.extend(crate::outputs::provenance_metadata_args(&prov.stamp()));
    }
    args.extend([
        "-t".into(),
        format!("{duration}"),
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
/// Anything unreadable, or any file with no decodable video stream
/// (Sprint 5 T008: missing, corrupt, non-video, permission-denied, or
/// disappearing sources), refuses with a diagnostic naming the file.
/// Any container with a video stream the FFmpeg binary can decode
/// (MOV, MP4, MKV, WebM, and similar) renders like any other source:
/// the decoder below speaks rawvideo whatever the codec, so the codec
/// name is a fact for refusal text, never a gate.
fn probe_video(ffmpeg_bin: &Path, abs: &Path, rel: &str) -> Result<ProbedVideo, String> {
    let out = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-i", &abs.display().to_string()])
        .output()
        .map_err(|e| {
            format!("render refused: cannot probe video source {rel:?} ({e}); no MP4 was written")
        })?;
    // ffmpeg prints input facts to stderr and exits non-zero without an
    // output file; the facts are still usable when present.
    let text = String::from_utf8_lossy(&out.stderr);
    if !text.contains("Input #0") {
        return Err(format!(
            "render refused: video source {rel:?} is unreadable (ffmpeg reports no input stream); no MP4 was written"
        ));
    }
    let duration = parse_duration(&text).or_else(|| probe_format_duration(ffmpeg_bin, abs));
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
    if width == 0 || height == 0 {
        return Err(format!(
            "render refused: video source {rel:?} reports no picture dimensions; no MP4 was written"
        ));
    }
    // Sprint 5 T005: container rotation applies so the rendered picture
    // matches what players show. Phones store landscape pixels with a
    // 90-degree display matrix; the decoder below auto-rotates those to
    // portrait, so the probe reports the oriented dimensions and every
    // `fit`/placement step sees the player-visible picture. A source
    // without rotation metadata renders exactly as stored.
    let rotation = parse_container_rotation(&text);
    let (width, height) = oriented_dimensions(width, height, rotation);
    // Sprint 12 outputs (CAP-004): the source transfer rides the
    // probe, so `auto` and explicit `--hdr` modes decide from facts.
    let transfer = crate::outputs::detect_transfer(&text);
    Ok(ProbedVideo {
        clip_idx: 0,
        path: abs.to_path_buf(),
        codec,
        transfer,
        width,
        height,
        duration,
        has_audio,
        loop_clip: false,
        rotation,
        win_start: 0.0,
        win_len: None,
    })
}

/// Parse container rotation metadata from `ffmpeg -i` output (Sprint 5
/// T005): a `Display Matrix: rotation of N degrees` side-data line wins
/// (the demuxer-computed angle), else a legacy `rotate : N` metadata
/// tag. Absent or unparseable rotation means render-as-stored (0).
fn parse_container_rotation(text: &str) -> f64 {
    let mut tag: Option<f64> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(at) = line.find("rotation of") {
            let rest = line[at + "rotation of".len()..].trim();
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
                .collect();
            if let Ok(deg) = num.parse::<f64>() {
                if deg.is_finite() {
                    return normalize_rotation(deg);
                }
            }
        }
        if tag.is_none() {
            if let Some((_, value)) = line.split_once("rotate :") {
                let value = value.trim();
                if let Ok(deg) = value.parse::<f64>() {
                    if deg.is_finite() {
                        tag = Some(normalize_rotation(deg));
                    }
                }
            }
        }
    }
    tag.unwrap_or(0.0)
}

/// Normalize a rotation angle to [0, 360).
fn normalize_rotation(deg: f64) -> f64 {
    let mut a = deg % 360.0;
    if a < 0.0 {
        a += 360.0;
    }
    if a == 360.0 {
        0.0
    } else {
        a
    }
}

/// Oriented picture dimensions for a normalized container rotation
/// (Sprint 5 T005): quarter turns (90/270) swap the stored axes, half
/// turns and upright keep them. Only exact quarter/half turns orient;
/// exotic angles render stored (see [`needs_autorotate`]).
fn oriented_dimensions(width: u32, height: u32, rotation: f64) -> (u32, u32) {
    if is_quarter_turn(rotation) {
        (height, width)
    } else {
        (width, height)
    }
}

/// True for exact 90/270-degree rotations (within tolerance): the only
/// angles whose oriented picture swaps axes.
fn is_quarter_turn(rotation: f64) -> bool {
    (rotation - 90.0).abs() < 1e-6 || (rotation - 270.0).abs() < 1e-6
}

/// True when the decoder must auto-rotate (Sprint 5 T005): the exact
/// quarter/half turns FFmpeg's `autorotate` transpose path handles
/// (90/180/270). Upright needs no filter; exotic angles render stored,
/// exactly like the decoder default, so `fit` still sees real pixels.
fn needs_autorotate(rotation: f64) -> bool {
    (rotation - 90.0).abs() < 1e-6
        || (rotation - 180.0).abs() < 1e-6
        || (rotation - 270.0).abs() < 1e-6
}

/// The `transpose` filter chain for one probed orientation (Sprint
/// 25): FFmpeg's `transpose` `dir` semantics match the `autorotate`
/// orientation for every exact quarter/half turn, and the filter
/// works on every FFmpeg version (the `-autorotate` flag does not).
fn transpose_chain(rotation: f64) -> String {
    // Verified byte-for-byte against `-autorotate` (with
    // `-noautorotate` set so the filter is the only rotation) on the
    // rotation fixture: probed 90 -> transpose=2, 270 -> transpose=1.
    if (rotation - 90.0).abs() < 1e-6 {
        "transpose=2".to_string()
    } else if (rotation - 180.0).abs() < 1e-6 {
        "transpose=1,transpose=1".to_string()
    } else {
        "transpose=1".to_string()
    }
}

/// `Duration: 00:00:03.00` → seconds.
pub(crate) fn parse_duration(text: &str) -> Option<f64> {
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
pub(crate) fn parse_video_stream(line: &str) -> Option<(String, u32, u32)> {
    let after = line.split("Video:").nth(1)?.trim();
    let codec = after
        .split(|c: char| c == ' ' || c == '(' || c == ',')
        .next()?;
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
    pub(crate) scaled:
        std::sync::Arc<std::sync::Mutex<HashMap<ImageScaleKey, std::sync::Arc<CachedScale>>>>,
}

impl DecodedImage {
    /// Build one prepared still from already-decoded straight RGBA
    /// rows (Sprint 22 captions, T003): subject mattes decode through
    /// the `png` crate rather than FFmpeg, but share the table, the
    /// path key, and the empty scale cache with every other still.
    pub(crate) fn from_rgba(path: PathBuf, width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            path,
            width,
            height,
            rgba,
            scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
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
pub(crate) fn rr_coverage(i: i64, j: i64, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> f64 {
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
fn blend_over(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    i: i64,
    j: i64,
    r: u8,
    g: u8,
    b: u8,
    alpha: f64,
) {
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

/// Sample one gradient stop list at `t` (Sprint 8 look, T003): the
/// sRGB channel-by-channel lerp of the surrounding stops, clamped to
/// the end stops outside. The scan walks stops in order, so ten stops
/// cost about twice five — no per-stop cost leak beyond the linear
/// walk. Empty lists paint black; validation refuses them before
/// frame one.
pub(crate) fn sample_gradient(stops: &[GradientStop], t: f64) -> (u8, u8, u8) {
    let Some(first) = stops.first() else {
        return (0, 0, 0);
    };
    let t = t.clamp(0.0, 1.0);
    if t <= first.offset {
        return (first.color.r, first.color.g, first.color.b);
    }
    let mut prev = first;
    for stop in &stops[1..] {
        if t <= stop.offset {
            let span = stop.offset - prev.offset;
            let f = if span <= 0.0 {
                0.0
            } else {
                (t - prev.offset) / span
            };
            let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * f).round() as u8;
            return (
                mix(prev.color.r, stop.color.r),
                mix(prev.color.g, stop.color.g),
                mix(prev.color.b, stop.color.b),
            );
        }
        prev = stop;
    }
    (prev.color.r, prev.color.g, prev.color.b)
}

/// Linear-gradient position of a box-local point (Sprint 8 look,
/// T003): the `angle` in CSS degrees (0 points up, 90 points right)
/// projected onto the gradient axis, 0 at the start edge and 1 at the
/// end edge, clamped to the box. 90 degrees runs left-to-right.
pub(crate) fn linear_fill_t(angle_deg: f64, lx: f64, ly: f64, w: f64, h: f64) -> f64 {
    let rad = angle_deg.to_radians();
    // CSS direction: 0 deg points up, 90 deg points right; canvas y
    // runs down, so the y axis flips.
    let (dx, dy) = (rad.sin(), -rad.cos());
    // Axis through the box center, spanning the projected corners.
    let len = (w * dx.abs() + h * dy.abs()).max(f64::MIN_POSITIVE);
    let proj = (lx - w / 2.0) * dx + (ly - h / 2.0) * dy;
    (proj / len + 0.5).clamp(0.0, 1.0)
}

/// Bilinear sample of a decoded image at source texel coords (Sprint
/// 8 look, T003): the same texel lerp `draw_image` uses, as straight
/// RGBA with alpha 0 to 1.
fn sample_fill_image(decoded: &DecodedImage, su: f64, sv: f64) -> [f64; 4] {
    let sw = decoded.width.max(1) as f64;
    let sh = decoded.height.max(1) as f64;
    let su = su.clamp(0.0, sw - 1.0);
    let sv = sv.clamp(0.0, sh - 1.0);
    let x0 = su.floor() as u32;
    let x1 = (x0 + 1).min(decoded.width.max(1) - 1);
    let fx = su - su.floor();
    let y0 = sv.floor() as u32;
    let y1 = (y0 + 1).min(decoded.height.max(1) - 1);
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
}

/// Image texel for one box-local point of an image fill (Sprint 8
/// look, T003): the fitted picture sampled at the point, clipped to
/// the fill region (the full box, or the inner rect when a border
/// ring covers the edge). `None` means the point shows no picture —
/// the border ring or canvas below shows through. `sw`/`sh` are the
/// scaled box size, `bw` the scaled border width.
fn box_image_at(
    decoded: &DecodedImage,
    fit: Fit,
    placement: notation::model::Placement,
    lx: f64,
    ly: f64,
    sw: f64,
    sh: f64,
    fill_inset: f64,
) -> Option<(u8, u8, u8, f64)> {
    use notation::model::{PlaceH, PlaceV};
    let srcw = decoded.width.max(1) as f64;
    let srch = decoded.height.max(1) as f64;
    let (tx, ty, tw, th) = if fill_inset > 0.0 {
        (
            fill_inset,
            fill_inset,
            sw - 2.0 * fill_inset,
            sh - 2.0 * fill_inset,
        )
    } else {
        (0.0, 0.0, sw, sh)
    };
    if tw <= 0.0 || th <= 0.0 {
        return None;
    }
    let (dw, dh) = image_fit_size(tw, th, srcw, srch, fit);
    if dw <= 0.0 || dh <= 0.0 {
        return None;
    }
    let ox = match placement.x {
        PlaceH::Left => 0.0,
        PlaceH::Center => (tw - dw) / 2.0,
        PlaceH::Right => tw - dw,
    };
    let oy = match placement.y {
        PlaceV::Top => 0.0,
        PlaceV::Center => (th - dh) / 2.0,
        PlaceV::Bottom => th - dh,
    };
    let px = lx - (tx + ox);
    let py = ly - (ty + oy);
    if px < 0.0 || py < 0.0 || px > dw || py > dh {
        return None;
    }
    let rgba = sample_fill_image(decoded, px * srcw / dw - 0.5, py * srch / dh - 0.5);
    Some((
        rgba[0].round() as u8,
        rgba[1].round() as u8,
        rgba[2].round() as u8,
        rgba[3].clamp(0.0, 1.0),
    ))
}

/// Paint one box pixel through the generic `blend` sink (Sprint 8
/// look, T003/T004): the border ring keeps the border color, the fill
/// area takes the solid color, the gradient sample, or the image
/// texel, each under `coverage * alpha`. `(lx, ly)` is the box-local
/// point in scaled pixels, `(sw, sh)` the scaled box size, `bw` the
/// scaled border width, `fill_inset` the fill shrink the image fill
/// fits into (the positioned stroke width inside the box),
/// `outer`/`inner` the ring and fill coverages.
#[allow(clippy::too_many_arguments)]
fn paint_box_pixel(
    blend: &mut dyn FnMut(u8, u8, u8, f64),
    b: &crate::frame::ResolvedBox,
    decoded: Option<&DecodedImage>,
    lx: f64,
    ly: f64,
    sw: f64,
    sh: f64,
    bw: f64,
    fill_inset: f64,
    outer: f64,
    inner: f64,
    alpha: f64,
) {
    if outer <= 0.0 {
        return;
    }
    // The solid fast path paints exactly as before (T004): border ring
    // over fill, both under the element opacity.
    if b.fill_paint.is_solid() {
        if bw > 0.0 {
            let ring = (outer - inner).max(0.0);
            if ring > 0.0 {
                blend(
                    b.border_color.r,
                    b.border_color.g,
                    b.border_color.b,
                    ring * alpha,
                );
            }
            if inner > 0.0 {
                blend(b.fill.r, b.fill.g, b.fill.b, inner * alpha);
            }
        } else {
            blend(b.fill.r, b.fill.g, b.fill.b, outer * alpha);
        }
        return;
    }
    // Gradient and image fills paint the fill area only; the border
    // ring keeps the solid border color above them.
    let fill_ring = |blend: &mut dyn FnMut(u8, u8, u8, f64)| {
        if bw > 0.0 {
            let ring = (outer - inner).max(0.0);
            if ring > 0.0 {
                blend(
                    b.border_color.r,
                    b.border_color.g,
                    b.border_color.b,
                    ring * alpha,
                );
            }
        }
    };
    match &b.fill_paint {
        FillPaint::Solid => unreachable!("solid paints above"),
        FillPaint::Linear { angle, stops } => {
            fill_ring(blend);
            let area = if bw > 0.0 { inner } else { outer };
            if area > 0.0 {
                let (r, g, bl) = sample_gradient(stops, linear_fill_t(*angle, lx, ly, sw, sh));
                blend(r, g, bl, area * alpha);
            }
        }
        FillPaint::Radial {
            cx,
            cy,
            radius,
            stops,
        } => {
            fill_ring(blend);
            let area = if bw > 0.0 { inner } else { outer };
            if area > 0.0 {
                let (ccx, ccy) = (cx.unwrap_or(sw / 2.0), cy.unwrap_or(sh / 2.0));
                let r = radius.unwrap_or(sw.hypot(sh) / 2.0).max(f64::MIN_POSITIVE);
                let t = ((lx - ccx).hypot(ly - ccy) / r).clamp(0.0, 1.0);
                let (fr, fg, fb) = sample_gradient(stops, t);
                blend(fr, fg, fb, area * alpha);
            }
        }
        FillPaint::Image { fit, placement, .. } => {
            fill_ring(blend);
            let area = if bw > 0.0 { inner } else { outer };
            if area > 0.0 {
                if let Some(decoded) = decoded {
                    if let Some((r, g, bl, a)) =
                        box_image_at(decoded, *fit, *placement, lx, ly, sw, sh, fill_inset)
                    {
                        blend(r, g, bl, (a * area * alpha).clamp(0.0, 1.0));
                    }
                }
            }
        }
    }
}

/// Look up the decoded picture for one image fill (Sprint 8 look,
/// T003): the file is statted on every call, so a source that
/// vanishes mid-render refuses here with its path named and no
/// partial MP4, instead of painting stale decoded pixels.
/// Resolve one item's VFX stage pictures (Sprint 11 VFX, CAP-004/T005):
/// each active file-backed stage borrows its decoded picture from the
/// shared list. A source deleted after the decode stage refuses naming
/// its path with no partial MP4, exactly like the image-fill path. An
/// inert stage resolves to none, so disabling a stage matches the
/// chain written without it even when its file vanishes.
pub(crate) fn resolve_vfx_maps<'a>(
    images: &'a [DecodedImage],
    folder: &Path,
    vfx: &notation::model::VfxChain,
) -> Result<crate::vfx::VfxMaps<'a>, String> {
    fn one<'a>(
        images: &'a [DecodedImage],
        folder: &Path,
        stage: &str,
        src: &str,
    ) -> Result<crate::vfx::VfxMap<'a>, String> {
        if !folder.join(src).is_file() {
            return Err(format!(
                "render refused: {stage} source {src:?} vanished before its frame could draw; no MP4 was written"
            ));
        }
        let decoded = images.iter().find(|d| d.path.ends_with(src)).ok_or_else(|| {
            format!(
                "render refused: {stage} source {src:?} was not decoded (decode stage); no MP4 was written"
            )
        })?;
        Ok(crate::vfx::VfxMap {
            w: decoded.width as usize,
            h: decoded.height as usize,
            rgba: &decoded.rgba,
        })
    }
    let displacement = match vfx.displacement.as_ref().filter(|s| !s.is_empty()) {
        Some(stage) => match stage.map.as_deref() {
            Some(map) => Some(one(images, folder, "displacement", map)?),
            None => None,
        },
        None => None,
    };
    let luma_matte = match vfx.luma_matte.as_ref().filter(|s| !s.is_empty()) {
        Some(stage) => Some(one(images, folder, "luma-matte", &stage.src)?),
        None => None,
    };
    Ok(crate::vfx::VfxMaps {
        displacement,
        luma_matte,
    })
}

pub(crate) fn fill_image_decoded<'a>(
    images: &'a [DecodedImage],
    folder: &Path,
    src: &str,
) -> Result<&'a DecodedImage, String> {
    if !folder.join(src).is_file() {
        return Err(format!(
            "render refused: image-fill source {src:?} vanished before its frame could draw; no MP4 was written"
        ));
    }
    images.iter().find(|d| d.path.ends_with(src)).ok_or_else(|| {
        format!("render refused: image-fill source {src:?} was not decoded (decode stage); no MP4 was written")
    })
}

/// Draw one box (T004/T010, plus Sprint 8 gradient and image fills,
/// plus the Sprint 8 stroke positions): a filled rounded rect with a
/// positioned stroke ring, both under the element opacity. A fully
/// transparent box draws nothing; a fully opaque box hides what is
/// beneath it. The stroke paints above the fill at its position
/// (`inside` rings the inner edge like the historical border,
/// `center` straddles the edge, `outside` rings the outer edge);
/// overflow `hidden`/`clip` cuts body paint at the box edge. A
/// `linear-gradient` or `radial-gradient` fill paints its sRGB ramp
/// clipped to the fill area; an `image-fill` paints its fitted picture
/// there. Uniform `scale` grows the box (size, radius, border) about
/// its top-left; `rotation` (degrees, positive turns clockwise on
/// screen) turns it about its scaled center. A non-positive scale
/// draws nothing. An image fill whose source vanished refuses with
/// its path named.
pub(crate) fn draw_box(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    b: &crate::frame::ResolvedBox,
    images: &[DecodedImage],
    folder: &Path,
) -> Result<(), String> {
    if b.opacity <= 0.0 || b.width <= 0.0 || b.height <= 0.0 {
        return Ok(());
    }
    if b.scale <= 0.0 || !b.scale.is_finite() {
        return Ok(());
    }
    // Resolve the image fill once per box: the vanish check stats the
    // file, so a source deleted mid-render refuses here with its path
    // named and no partial MP4, instead of painting stale pixels.
    let decoded = match &b.fill_paint {
        FillPaint::Image { src, .. } => Some(fill_image_decoded(images, folder, src)?),
        _ => None,
    };
    let alpha = b.opacity.clamp(0.0, 1.0);
    let s = b.scale;
    let sw = b.width * s;
    let sh = b.height * s;
    let (x0, y0) = (b.x, b.y);
    let (x1, y1) = (b.x + sw, b.y + sh);
    let r = (b.radius * s).max(0.0).min(sw / 2.0).min(sh / 2.0);
    let bw = (b.border_width * s).max(0.0);
    // Stroke position (Sprint 8 look, T004): the outer paint rect and
    // the fill rect move with the stroke; `inside` reproduces the
    // historical border exactly.
    let ((ox0, oy0, ox1, oy1, ro), (fx0, fy0, fx1, fy1, rf), fill_inset) =
        crate::paint::stroke_geometry(x0, y0, x1, y1, r, bw, b.stroke_position);
    // Overflow `hidden`/`clip` cuts fill and stroke at the box edge;
    // `visible` paints the whole stroke. An inside stroke never leaves
    // the box, so clipping is a no-op for it.
    let clips = crate::paint::overflow_clips(b.overflow);
    if b.rotation == 0.0 || !b.rotation.is_finite() {
        let i0 = (ox0.floor() as i64).max(0);
        let i1 = (ox1.ceil() as i64).min(cw as i64);
        let j0 = (oy0.floor() as i64).max(0);
        let j1 = (oy1.ceil() as i64).min(chh as i64);
        // Sharp integer rect (T004 speed): with square corners on integer
        // edges every visited pixel's 16 sub-samples fall strictly inside,
        // so coverage is exactly 1.0 without supersampling. Finite edges
        // only; anything else takes the slow path. Positioned strokes
        // take the coverage path so the outset rounds identically.
        let sharp = b.stroke_position == notation::model::StrokePosition::Inside
            && r <= 0.0
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
                let mut outer = if sharp {
                    1.0
                } else {
                    rr_coverage(i, j, ox0, oy0, ox1, oy1, ro)
                };
                let mut inner = if bw > 0.0 {
                    rr_coverage(i, j, fx0, fy0, fx1, fy1, rf)
                } else {
                    0.0
                };
                if clips {
                    let box_cov = if sharp {
                        1.0
                    } else {
                        rr_coverage(i, j, x0, y0, x1, y1, r)
                    };
                    outer = outer.min(box_cov);
                    inner = inner.min(box_cov);
                }
                if outer <= 0.0 {
                    continue;
                }
                let (lx, ly) = (i as f64 + 0.5 - x0, j as f64 + 0.5 - y0);
                let mut blend = |rr: u8, gg: u8, bb: u8, a: f64| {
                    blend_over(frame, cw, chh, i, j, rr, gg, bb, a);
                };
                paint_box_pixel(
                    &mut blend, b, decoded, lx, ly, sw, sh, bw, fill_inset, outer, inner, alpha,
                );
            }
        }
        return Ok(());
    }
    // Rotated path: every sub-sample is inverse-rotated into the
    // unrotated box frame before the rounded-rect test; the fill
    // samples at the inverse-rotated pixel center in the same frame.
    // The walk spans the stroke outset, which rotates about the same
    // center (the outset is symmetric, so the center never moves).
    let theta = b.rotation.to_radians();
    let (sin, cos) = theta.sin_cos();
    let (cx, cy) = ((ox0 + ox1) / 2.0, (oy0 + oy1) / 2.0);
    let hw = (ox1 - ox0) / 2.0;
    let hh = (oy1 - oy0) / 2.0;
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
            let mut outer = rr_coverage_mapped(i, j, &map, ox0, oy0, ox1, oy1, ro);
            let mut inner = if bw > 0.0 {
                rr_coverage_mapped(i, j, &map, fx0, fy0, fx1, fy1, rf)
            } else {
                0.0
            };
            if clips {
                let box_cov = rr_coverage_mapped(i, j, &map, x0, y0, x1, y1, r);
                outer = outer.min(box_cov);
                inner = inner.min(box_cov);
            }
            if outer <= 0.0 {
                continue;
            }
            let (mx, my) = map(i as f64 + 0.5, j as f64 + 0.5);
            let (lx, ly) = (mx - x0, my - y0);
            let mut blend = |rr: u8, gg: u8, bb: u8, a: f64| {
                blend_over(frame, cw, chh, i, j, rr, gg, bb, a);
            };
            paint_box_pixel(
                &mut blend, b, decoded, lx, ly, sw, sh, bw, fill_inset, outer, inner, alpha,
            );
        }
    }
    Ok(())
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
        let j1 = (dy + dh - 0.5)
            .floor()
            .min(chh as f64 - 1.0)
            .min((ty1 - 0.5).floor())
            .max(-1.0) as i64;
        let i0 = (dx - 0.5).ceil().max(0.0).max((tx0 - 0.5).ceil().max(0.0)) as i64;
        let i1 = (dx + dw - 0.5)
            .floor()
            .min(cw as f64 - 1.0)
            .min((tx1 - 0.5).floor())
            .max(-1.0) as i64;
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
            let o =
                (((j0 + (k / w) as i64) as u32 * cw + (i0 + (k % w) as i64) as u32) as usize) * 3;
            if ae >= 1.0 {
                frame[o] = entry.rgb[k * 3];
                frame[o + 1] = entry.rgb[k * 3 + 1];
                frame[o + 2] = entry.rgb[k * 3 + 2];
                continue;
            }
            let inv = 1.0 - ae;
            frame[o] = (entry.rgb[k * 3] as f64 * ae + frame[o] as f64 * inv + 0.5) as u8;
            frame[o + 1] =
                (entry.rgb[k * 3 + 1] as f64 * ae + frame[o + 1] as f64 * inv + 0.5) as u8;
            frame[o + 2] =
                (entry.rgb[k * 3 + 2] as f64 * ae + frame[o + 2] as f64 * inv + 0.5) as u8;
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
            frame,
            cw,
            chh,
            i,
            j,
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

/// Per-frame LUT grid cache (Sprint 11 grade, CAP-002/T003): one
/// parsed grid per distinct `src`, loaded fresh on every frame. The
/// freshness is the point — like the image-fill vanish check, the
/// file is statted on every frame, so a source deleted mid-render
/// refuses here with its path named and no partial MP4, instead of
/// painting stale grids.
pub(crate) struct LutFrameCache {
    tables: HashMap<String, LutTable>,
}

impl LutFrameCache {
    pub(crate) fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    /// Resolve one look's LUT for drawing: clones the grade with its
    /// grid materialized from disk. A look without a file (or with a
    /// zero amount) resolves untouched; a vanished, unreadable, or
    /// unparsable file refuses naming the path, like the check-time
    /// refusals but raised mid-render with no partial MP4.
    pub(crate) fn grade(
        &mut self,
        folder: &Path,
        grade: &ResolvedGrade,
    ) -> Result<ResolvedGrade, String> {
        let (Some(src), amount) = (grade.lut.src.as_deref(), grade.lut.amount) else {
            return Ok(grade.clone());
        };
        if amount == 0.0 {
            return Ok(grade.clone());
        }
        // The vanish check stats the file on every frame (even a
        // cached one), exactly like the image-fill path: a source
        // deleted mid-render refuses here instead of painting the
        // stale cached grid.
        let abs = folder.join(src);
        if !abs.is_file() {
            self.tables.remove(src);
            return Err(format!(
                "render refused: lut source {src:?} vanished before its frame could draw; no MP4 was written"
            ));
        }
        if !self.tables.contains_key(src) {
            let bytes = std::fs::read(&abs).map_err(|e| {
                format!(
                    "render refused: lut source {src:?} cannot be read: {e}; no MP4 was written"
                )
            })?;
            let text = String::from_utf8(bytes).map_err(|_| {
                format!("render refused: lut source {src:?} is not UTF-8 text; no MP4 was written")
            })?;
            let table = notation::model::parse_cube(&text).map_err(|problem| {
                format!(
                    "render refused: lut source {src:?} is unusable: {problem}; no MP4 was written"
                )
            })?;
            self.tables.insert(src.to_string(), table);
        }
        let mut out = grade.clone();
        out.lut.grid = self.tables.get(src).cloned();
        Ok(out)
    }
}

/// Fill an RGB24 canvas with the graded background (Sprint 11 grade,
/// CAP-001/T001+T002): the composition-wide grade alone. An empty
/// grade keeps the authored color bit-exact; a position-independent
/// grade fills flat through the position-independent chain (LUT
/// included once the resolve step loads its grid); a spatial grade
/// (vignette or grain) paints per pixel so corners and grain land
/// where the frame is. Both backends share this, so CPU and GPU
/// backgrounds agree byte-exactly.
pub(crate) fn fill_graded_background(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    bg: notation::model::Rgb,
    grade: &ResolvedGrade,
) {
    if grade.is_empty() {
        fill_canvas(frame, bg.r, bg.g, bg.b);
        return;
    }
    if !grade.is_spatial() {
        let (r, g, b) = grade.apply(bg.r as f64, bg.g as f64, bg.b as f64);
        fill_canvas(frame, r.round() as u8, g.round() as u8, b.round() as u8);
        return;
    }
    let (w, h) = (cw as f64, chh as f64);
    for j in 0..chh {
        for i in 0..cw {
            let o = ((j as usize * cw as usize) + i as usize) * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            let (r, g, b) = grade.apply_at(
                bg.r as f64,
                bg.g as f64,
                bg.b as f64,
                i as f64,
                j as f64,
                w,
                h,
            );
            frame[o] = r.round() as u8;
            frame[o + 1] = g.round() as u8;
            frame[o + 2] = b.round() as u8;
        }
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
/// True when a blur sigma draws blurred (Sprint 7 motion, CAP-006):
/// positive and finite. Anything else draws sharp.
pub(crate) fn blur_on(sigma: f64) -> bool {
    sigma > 0.0 && sigma.is_finite()
}

/// Blur spread margin in canvas pixels: the kernel reaches
/// `ceil(3σ)`, plus one texel of guard.
pub(crate) fn blur_margin(sigma: f64) -> f64 {
    if !blur_on(sigma) {
        0.0
    } else {
        (3.0 * sigma).ceil() + 1.0
    }
}

/// Composite one straight-RGBA texel over a layer texel at byte offset
/// `o` (source-over in layer space, for `draw_box_layer`).
pub(crate) fn layer_blend(layer: &mut [u8], o: usize, r: u8, g: u8, b: u8, a: f64) {
    if !(a > 0.0) || o + 3 >= layer.len() {
        return;
    }
    let a = a.clamp(0.0, 1.0);
    let da = layer[o + 3] as f64 / 255.0;
    let na = a + da * (1.0 - a);
    if na <= 0.0 {
        layer[o + 3] = 0;
        return;
    }
    layer[o] = ((r as f64 * a + layer[o] as f64 * da * (1.0 - a)) / na + 0.5) as u8;
    layer[o + 1] = ((g as f64 * a + layer[o + 1] as f64 * da * (1.0 - a)) / na + 0.5) as u8;
    layer[o + 2] = ((b as f64 * a + layer[o + 2] as f64 * da * (1.0 - a)) / na + 0.5) as u8;
    layer[o + 3] = (na * 255.0 + 0.5) as u8;
}

/// Draw one box into a transparent straight-RGBA layer (Sprint 7
/// motion, T005/T006, plus Sprint 8 gradient and image fills): the
/// same unrotated geometry `draw_box` draws, but as layer texels the
/// blur/turn composite consumes. Rotation and projection run in the
/// composite, never here. The caller resolves the image fill once
/// (with its vanish check); the layer paints its texels, if any.
pub(crate) fn draw_box_layer(
    layer: &mut [u8],
    cw: u32,
    chh: u32,
    b: &crate::frame::ResolvedBox,
    decoded: Option<&DecodedImage>,
) {
    if b.opacity <= 0.0 || b.width <= 0.0 || b.height <= 0.0 {
        return;
    }
    if b.scale <= 0.0 || !b.scale.is_finite() {
        return;
    }
    let alpha = b.opacity.clamp(0.0, 1.0);
    let s = b.scale;
    let (x0, y0) = (b.x, b.y);
    let sw = b.width * s;
    let sh = b.height * s;
    let (x1, y1) = (b.x + sw, b.y + sh);
    let r = (b.radius * s).max(0.0).min(sw / 2.0).min(sh / 2.0);
    let bw = (b.border_width * s).max(0.0);
    // Stroke position (Sprint 8 look, T004): `inside` reproduces the
    // historical border exactly; positioned strokes move the outer
    // and fill rects before the layer paints.
    let ((ox0, oy0, ox1, oy1, ro), (fx0, fy0, fx1, fy1, rf), fill_inset) =
        crate::paint::stroke_geometry(x0, y0, x1, y1, r, bw, b.stroke_position);
    let clips = crate::paint::overflow_clips(b.overflow);
    let i0 = (ox0.floor() as i64).max(0);
    let i1 = (ox1.ceil() as i64).min(cw as i64);
    let j0 = (oy0.floor() as i64).max(0);
    let j1 = (oy1.ceil() as i64).min(chh as i64);
    let sharp = b.stroke_position == notation::model::StrokePosition::Inside
        && r <= 0.0
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
            if i < 0 || j < 0 || i >= cw as i64 || j >= chh as i64 {
                continue;
            }
            let mut outer = if sharp {
                1.0
            } else {
                rr_coverage(i, j, ox0, oy0, ox1, oy1, ro)
            };
            if outer <= 0.0 {
                continue;
            }
            let mut inner = if bw > 0.0 {
                rr_coverage(i, j, fx0, fy0, fx1, fy1, rf)
            } else {
                0.0
            };
            if clips {
                let box_cov = if sharp {
                    1.0
                } else {
                    rr_coverage(i, j, x0, y0, x1, y1, r)
                };
                outer = outer.min(box_cov);
                inner = inner.min(box_cov);
                if outer <= 0.0 {
                    continue;
                }
            }
            let o = (j as u32 * cw + i as u32) as usize * 4;
            let (lx, ly) = (i as f64 + 0.5 - x0, j as f64 + 0.5 - y0);
            // Per-pixel ring-then-fill over the layer (associative
            // with the frame-blit order, so the unblurred layer
            // composites identically).
            let mut blend = |rr: u8, gg: u8, bb: u8, a: f64| {
                layer_blend(layer, o, rr, gg, bb, a);
            };
            paint_box_pixel(
                &mut blend, b, decoded, lx, ly, sw, sh, bw, fill_inset, outer, inner, alpha,
            );
        }
    }
}

/// Draw one image into a transparent straight-RGBA layer (Sprint 7
/// motion, T005/T006): the same unrotated target-rect sampling
/// `draw_image` draws, but as layer texels. Returns the placed picture
/// rect `(dx, dy, dw, dh)` and the target rect `(tx0, ty0, tx1, ty1)`
/// the composite needs, or `None` when nothing draws (exactly the
/// `draw_image` early-outs).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_image_layer(
    layer: &mut [u8],
    cw: u32,
    chh: u32,
    im: &crate::frame::ResolvedImage,
    decoded: &DecodedImage,
) -> Option<(f64, f64, f64, f64, f64, f64, f64, f64)> {
    use notation::model::{PlaceH, PlaceV};
    let sw = decoded.width as f64;
    let sh = decoded.height as f64;
    if sw <= 0.0 || sh <= 0.0 {
        return None;
    }
    if im.scale <= 0.0 || !im.scale.is_finite() {
        return None;
    }
    if im.opacity <= 0.0 {
        return None;
    }
    let alpha = im.opacity.clamp(0.0, 1.0);
    let tw = im.width.unwrap_or(cw as f64) * im.scale;
    let th = im.height.unwrap_or(chh as f64) * im.scale;
    if tw <= 0.0 || th <= 0.0 {
        return None;
    }
    let (dw, dh) = image_fit_size(tw, th, sw, sh, im.fit);
    if dw <= 0.0 || dh <= 0.0 {
        return None;
    }
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
    let (tx0, ty0, tx1, ty1) = (im.x, im.y, im.x + tw, im.y + th);
    let swm1 = decoded.width - 1;
    let shm1 = decoded.height - 1;
    let sample = |su: f64, sv: f64| -> [f64; 4] {
        let su = su.clamp(0.0, sw - 1.0);
        let sv = sv.clamp(0.0, sh - 1.0);
        let x0 = su.floor() as u32;
        let x1 = (x0 + 1).min(swm1);
        let fx = su - su.floor();
        let y0 = sv.floor() as u32;
        let y1 = (y0 + 1).min(shm1);
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
    let j0 = (dy - 0.5).ceil().max(0.0).max((ty0 - 0.5).ceil().max(0.0)) as i64;
    let j1 = (dy + dh - 0.5)
        .floor()
        .min(chh as f64 - 1.0)
        .min((ty1 - 0.5).floor())
        .max(-1.0) as i64;
    let i0 = (dx - 0.5).ceil().max(0.0).max((tx0 - 0.5).ceil().max(0.0)) as i64;
    let i1 = (dx + dw - 0.5)
        .floor()
        .min(cw as f64 - 1.0)
        .min((tx1 - 0.5).floor())
        .max(-1.0) as i64;
    if j1 < j0 || i1 < i0 {
        return None;
    }
    for j in j0..=j1 {
        if j < 0 || j >= chh as i64 {
            continue;
        }
        for i in i0..=i1 {
            if i < 0 || i >= cw as i64 {
                continue;
            }
            let su = (((i as f64 + 0.5 - dx) * sw / dw) - 0.5).clamp(0.0, sw - 1.0);
            let sv = (((j as f64 + 0.5 - dy) * sh / dh) - 0.5).clamp(0.0, sh - 1.0);
            let rgba = sample(su, sv);
            let o = (j as u32 * cw + i as u32) as usize * 4;
            if o + 3 >= layer.len() {
                continue;
            }
            layer[o] = rgba[0].round().clamp(0.0, 255.0) as u8;
            layer[o + 1] = rgba[1].round().clamp(0.0, 255.0) as u8;
            layer[o + 2] = rgba[2].round().clamp(0.0, 255.0) as u8;
            layer[o + 3] = ((rgba[3] * alpha).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    Some((dx, dy, dw, dh, tx0, ty0, tx1, ty1))
}

/// Dest rect of a video blit (mirrors `blit_fit`'s geometry): `None`
/// when the rect is degenerate (draws nothing, exactly like `blit_fit`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn blit_dest(
    cw: u32,
    chh: u32,
    sw: u32,
    sh: u32,
    fit: Fit,
    scale: f64,
    ox: f64,
    oy: f64,
) -> Option<(f64, f64, f64, f64)> {
    if scale <= 0.0 || !scale.is_finite() {
        return None;
    }
    let (fx0, fy0, fw, fh) = fit_rect(cw, chh, sw, sh, fit);
    let (ccx, ccy) = (fx0 + fw / 2.0, fy0 + fh / 2.0);
    let (dw, dh) = (fw * scale, fh * scale);
    if dw < 1.0 || dh < 1.0 {
        return None;
    }
    Some((ccx - dw / 2.0 + ox, ccy - dh / 2.0 + oy, dw, dh))
}

/// Composite one premultiplied layer straight over the canvas
/// (Sprint 7 motion, T005/T006): no resampling, so blurred-but-still
/// elements land exactly. Only the grown element box is visited.
pub(crate) fn composite_premult_direct(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    pm: &crate::blur::Premult,
    bbox: (f64, f64, f64, f64),
) {
    let i0 = (bbox.0.floor() as i64).max(0);
    let i1 = (bbox.2.ceil() as i64).min(cw as i64);
    let j0 = (bbox.1.floor() as i64).max(0);
    let j1 = (bbox.3.ceil() as i64).min(chh as i64);
    for j in j0..j1 {
        for i in i0..i1 {
            let s = ((j as usize) * pm.w + i as usize) * 4;
            if s + 3 >= pm.px.len() {
                continue;
            }
            let a = pm.px[s + 3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let o = (j as u32 * cw + i as u32) as usize * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            if a >= 1.0 {
                frame[o] = pm.px[s].round().clamp(0.0, 255.0) as u8;
                frame[o + 1] = pm.px[s + 1].round().clamp(0.0, 255.0) as u8;
                frame[o + 2] = pm.px[s + 2].round().clamp(0.0, 255.0) as u8;
                continue;
            }
            let inv = 1.0 - a as f64;
            frame[o] = (pm.px[s] as f64 + frame[o] as f64 * inv + 0.5) as u8;
            frame[o + 1] = (pm.px[s + 1] as f64 + frame[o + 1] as f64 * inv + 0.5) as u8;
            frame[o + 2] = (pm.px[s + 2] as f64 + frame[o + 2] as f64 * inv + 0.5) as u8;
        }
    }
}

/// Screen bounds of a card turned through 2D rotation (about `anchor`)
/// after the 3D turn (Sprint 7 motion, CAP-005): the inverse walk
/// visits exactly this box, grown by one texel of guard.
pub(crate) fn warp_bounds(
    card: (f64, f64, f64, f64),
    anchor: (f64, f64),
    rotation_deg: f64,
    turn: Option<crate::turn::Turn>,
) -> (f64, f64, f64, f64) {
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let corners = [
        (card.0, card.1),
        (card.2, card.1),
        (card.2, card.3),
        (card.0, card.3),
    ];
    let mut bounds = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for (ux, uy) in corners {
        let (mut px, mut py) = match turn {
            Some(t) => t.forward(ux, uy),
            None => (ux, uy),
        };
        if rotation_deg != 0.0 && rotation_deg.is_finite() {
            let dx = px - anchor.0;
            let dy = py - anchor.1;
            px = anchor.0 + dx * cos - dy * sin;
            py = anchor.1 + dx * sin + dy * cos;
        }
        bounds.0 = bounds.0.min(px);
        bounds.1 = bounds.1.min(py);
        bounds.2 = bounds.2.max(px);
        bounds.3 = bounds.3.max(py);
    }
    (
        bounds.0 - 1.0,
        bounds.1 - 1.0,
        bounds.2 + 1.0,
        bounds.3 + 1.0,
    )
}

/// Composite one premultiplied layer through 2D rotation (about
/// `anchor`, degrees clockwise) and an optional 3D turn (Sprint 7
/// motion, CAP-005/006): every visited screen pixel pulls its source
/// texel through the inverse chain, so turned, rotated, and blurred
/// elements sample exactly one layer texel each. `bounds` is the
/// [`warp_bounds`] box of the card.
#[allow(clippy::too_many_arguments)]
pub(crate) fn composite_premult_warp(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    pm: &crate::blur::Premult,
    anchor: (f64, f64),
    rotation_deg: f64,
    turn: Option<crate::turn::Turn>,
    bounds: (f64, f64, f64, f64),
) {
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let rotated = rotation_deg != 0.0 && rotation_deg.is_finite();
    let i0 = (bounds.0.floor() as i64).max(0);
    let i1 = (bounds.2.ceil() as i64).min(cw as i64);
    let j0 = (bounds.1.floor() as i64).max(0);
    let j1 = (bounds.3.ceil() as i64).min(chh as i64);
    for j in j0..=j1 {
        if j < 0 || j >= chh as i64 {
            continue;
        }
        for i in i0..=i1 {
            if i < 0 || i >= cw as i64 {
                continue;
            }
            let qx = i as f64 + 0.5;
            let qy = j as f64 + 0.5;
            // Into the unrotated card frame through the inverse 2D
            // rotation about the anchor.
            let (mut rx, mut ry) = if rotated {
                let dx = qx - anchor.0;
                let dy = qy - anchor.1;
                (
                    anchor.0 + dx * cos + dy * sin,
                    anchor.1 - dx * sin + dy * cos,
                )
            } else {
                (qx, qy)
            };
            // Into layer space through the inverse 3D turn.
            if let Some(t) = turn {
                let Some((ux, uy)) = t.inverse(rx, ry) else {
                    continue;
                };
                rx = ux;
                ry = uy;
            }
            let rgba = pm.sample(rx - 0.5, ry - 0.5);
            let a = rgba[3].clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let o = (j as u32 * cw + i as u32) as usize * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            if a >= 1.0 {
                frame[o] = rgba[0].round().clamp(0.0, 255.0) as u8;
                frame[o + 1] = rgba[1].round().clamp(0.0, 255.0) as u8;
                frame[o + 2] = rgba[2].round().clamp(0.0, 255.0) as u8;
                continue;
            }
            let inv = 1.0 - a;
            frame[o] = (rgba[0] + frame[o] as f64 * inv + 0.5) as u8;
            frame[o + 1] = (rgba[1] + frame[o + 1] as f64 * inv + 0.5) as u8;
            frame[o + 2] = (rgba[2] + frame[o + 2] as f64 * inv + 0.5) as u8;
        }
    }
}

/// 3D turn of one resolved item about its card (Sprint 7 motion,
/// CAP-005): the evaluated angles with the clip's static
/// perspective/origin, or `None` for the identity (the old fast path
/// then draws byte-identically).
pub(crate) fn item_turn(
    rx: f64,
    ry: f64,
    turn: &notation::model::Turn3D,
    card: (f64, f64, f64, f64),
) -> Option<Turn> {
    let (fx, fy) = turn.origin_fractions();
    Turn::new(
        rx,
        ry,
        turn.perspective,
        card.0 + fx * (card.2 - card.0),
        card.1 + fy * (card.3 - card.1),
    )
}

/// Identity probe for the 3D turn (origin-independent): `None` means no
/// turn can apply, whatever the card; `Some` means the card decides.
pub(crate) fn turn_probe(rx: f64, ry: f64, perspective: Option<f64>) -> Option<Turn> {
    Turn::new(rx, ry, perspective, 0.0, 0.0)
}

/// Grow a card box by `margin` pixels on every side.
fn grown(card: (f64, f64, f64, f64), margin: f64) -> (f64, f64, f64, f64) {
    (
        card.0 - margin,
        card.1 - margin,
        card.2 + margin,
        card.3 + margin,
    )
}

/// Draw one box through blur and 3D turn (Sprint 7 motion, T005/T006):
/// layer, premultiply, blur, then composite directly (still) or through
/// the rotation/turn warp. Only runs when blur or a turn applies.
pub(crate) fn draw_box_advanced(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    b: &crate::frame::ResolvedBox,
    images: &[DecodedImage],
    folder: &Path,
    grade: &ResolvedGrade,
    effects: &[notation::model::Effect],
    vfx: &notation::model::VfxChain,
    maps: &crate::vfx::VfxMaps<'_>,
) -> Result<(), String> {
    if b.opacity <= 0.0 || b.width <= 0.0 || b.height <= 0.0 {
        return Ok(());
    }
    if b.scale <= 0.0 || !b.scale.is_finite() {
        return Ok(());
    }
    // Image fills resolve once per box, with the same vanish check as
    // the direct path: a source deleted mid-render refuses here with
    // its path named and no partial MP4.
    let decoded = match &b.fill_paint {
        FillPaint::Image { src, .. } => Some(fill_image_decoded(images, folder, src)?),
        _ => None,
    };
    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
    draw_box_layer(&mut layer, cw, chh, b, decoded);
    let mut pm = Premult::from_layer(&layer, cw as usize, chh as usize);
    if blur_on(b.blur) {
        pm.blur(b.blur);
    }
    // The look grades the layer's own pixels, then its effects
    // (Sprint 11 grade T001, effects T004).
    crate::blur::apply_grade_to_premult(&mut pm, grade);
    crate::effects::apply_effects_to_premult(&mut pm, effects);
    crate::vfx::apply_vfx_to_premult(&mut pm, vfx, maps);
    let s = b.scale;
    let card = (b.x, b.y, b.x + b.width * s, b.y + b.height * s);
    let anchor = ((card.0 + card.2) / 2.0, (card.1 + card.3) / 2.0);
    let turn = item_turn(b.rotate_x, b.rotate_y, &b.turn, card);
    let rotated = b.rotation != 0.0 && b.rotation.is_finite();
    match (turn, rotated) {
        (None, false) => {
            composite_premult_direct(frame, cw, chh, &pm, grown(card, blur_margin(b.blur)))
        }
        _ => composite_premult_warp(
            frame,
            cw,
            chh,
            &pm,
            anchor,
            b.rotation,
            turn,
            warp_bounds(card, anchor, b.rotation, turn),
        ),
    }
    Ok(())
}

/// Draw one image through blur and 3D turn (Sprint 7 motion, T005/T006):
/// layer, premultiply, blur, then composite directly (still) or through
/// the rotation/turn warp. Only runs when blur or a turn applies. The
/// 2D rotation anchors at the placed picture center, exactly like
/// `draw_image`; the turn origin is a fraction of the target rect.
pub(crate) fn draw_image_advanced(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    im: &crate::frame::ResolvedImage,
    decoded: &DecodedImage,
    grade: &ResolvedGrade,
    effects: &[notation::model::Effect],
    vfx: &notation::model::VfxChain,
    maps: &crate::vfx::VfxMaps<'_>,
) {
    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
    let Some((dx, dy, dw, dh, tx0, ty0, tx1, ty1)) =
        draw_image_layer(&mut layer, cw, chh, im, decoded)
    else {
        return;
    };
    let mut pm = Premult::from_layer(&layer, cw as usize, chh as usize);
    if blur_on(im.blur) {
        pm.blur(im.blur);
    }
    // The look grades the layer's own pixels, then its effects
    // (Sprint 11 grade T001, effects T004).
    crate::blur::apply_grade_to_premult(&mut pm, grade);
    crate::effects::apply_effects_to_premult(&mut pm, effects);
    crate::vfx::apply_vfx_to_premult(&mut pm, vfx, maps);
    let card = (tx0, ty0, tx1, ty1);
    let anchor = (dx + dw / 2.0, dy + dh / 2.0);
    let turn = item_turn(im.rotate_x, im.rotate_y, &im.turn, card);
    let rotated = im.rotation != 0.0 && im.rotation.is_finite();
    match (turn, rotated) {
        (None, false) => {
            composite_premult_direct(frame, cw, chh, &pm, grown(card, blur_margin(im.blur)))
        }
        _ => composite_premult_warp(
            frame,
            cw,
            chh,
            &pm,
            anchor,
            im.rotation,
            turn,
            warp_bounds(card, anchor, im.rotation, turn),
        ),
    }
}

/// Draw one video through blur and 3D turn (Sprint 7 motion, T005/T006):
/// the shared blitter paints the unrotated dest rect onto a black
/// scratch (premultiplied by construction), a binary dest mask carries
/// the element alpha, then blur and composite run directly (still) or
/// through the rotation/turn warp. Only runs when blur or a turn
/// applies. The 2D rotation anchors at the dest center, exactly like
/// `blit_fit`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_video_advanced(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    v: &crate::frame::ResolvedVideo,
    pic: &[u8],
    sw: u32,
    sh: u32,
    turn: Option<Turn>,
    grade: &ResolvedGrade,
    effects: &[notation::model::Effect],
    vfx: &notation::model::VfxChain,
    maps: &crate::vfx::VfxMaps<'_>,
) {
    if v.opacity <= 0.0 {
        return;
    }
    let Some((dx, dy, dw, dh)) = blit_dest(cw, chh, sw, sh, v.fit, v.scale, v.x, v.y) else {
        return;
    };
    let alpha = v.opacity.clamp(0.0, 1.0);
    let mut scratch = vec![0u8; cw as usize * chh as usize * 3];
    blit_fit(
        &mut scratch,
        cw,
        chh,
        pic,
        sw,
        sh,
        v.fit,
        v.x,
        v.y,
        v.scale,
        0.0,
        v.opacity,
    );
    let margin = blur_margin(v.blur);
    let (w, h) = (cw as usize, chh as usize);
    let mut pm = Premult {
        w,
        h,
        px: vec![0.0f32; w * h * 4],
    };
    let i0 = ((dx - margin).floor() as i64).max(0);
    let i1 = ((dx + dw + margin).ceil() as i64).min(cw as i64);
    let j0 = ((dy - margin).floor() as i64).max(0);
    let j1 = ((dy + dh + margin).ceil() as i64).min(chh as i64);
    for j in j0..j1 {
        for i in i0..i1 {
            let cx = i as f64 + 0.5;
            let cy = j as f64 + 0.5;
            if cx < dx || cx > dx + dw || cy < dy || cy > dy + dh {
                continue;
            }
            let s3 = ((j as u32 * cw + i as u32) as usize) * 3;
            let s4 = ((j as usize) * w + i as usize) * 4;
            if s3 + 2 >= scratch.len() || s4 + 3 >= pm.px.len() {
                continue;
            }
            pm.px[s4] = scratch[s3] as f32;
            pm.px[s4 + 1] = scratch[s3 + 1] as f32;
            pm.px[s4 + 2] = scratch[s3 + 2] as f32;
            pm.px[s4 + 3] = alpha as f32;
        }
    }
    if blur_on(v.blur) {
        pm.blur(v.blur);
    }
    // The look grades the layer's own pixels, then its effects
    // (Sprint 11 grade T001, effects T004).
    crate::blur::apply_grade_to_premult(&mut pm, grade);
    crate::effects::apply_effects_to_premult(&mut pm, effects);
    crate::vfx::apply_vfx_to_premult(&mut pm, vfx, maps);
    let card = (dx, dy, dx + dw, dy + dh);
    let anchor = ((card.0 + card.2) / 2.0, (card.1 + card.3) / 2.0);
    let rotated = v.rotation != 0.0 && v.rotation.is_finite();
    match (turn, rotated) {
        (None, false) => composite_premult_direct(frame, cw, chh, &pm, grown(card, margin)),
        _ => composite_premult_warp(
            frame,
            cw,
            chh,
            &pm,
            anchor,
            v.rotation,
            turn,
            warp_bounds(card, anchor, v.rotation, turn),
        ),
    }
}

/// Draw one shaped text through blur and 3D turn (Sprint 7 motion,
/// T005/T006): layer, premultiply, blur, then composite directly
/// (still) or through the rotation/turn warp. Only runs when blur or a
/// turn applies. The 2D rotation anchors at `(x, y)`, exactly like the
/// layer path of `draw_text_block`; the turn origin is a fraction of
/// the laid-out card.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text_advanced(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    block: &layout::Block,
    tc: &crate::frame::ResolvedText,
    turn: Option<Turn>,
    card: (f64, f64, f64, f64),
    grade: &ResolvedGrade,
    effects: &[notation::model::Effect],
    vfx: &notation::model::VfxChain,
    maps: &crate::vfx::VfxMaps<'_>,
) -> Result<(), String> {
    if tc.scale <= 0.0 || !tc.scale.is_finite() || tc.opacity <= 0.0 {
        return Ok(());
    }
    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
    draw_block_layer(
        &mut layer,
        cw,
        chh,
        raster,
        block,
        tc.x,
        tc.y,
        tc.scale,
        tc.opacity.clamp(0.0, 1.0),
    )?;
    let mut pm = Premult::from_layer(&layer, cw as usize, chh as usize);
    if blur_on(tc.blur) {
        pm.blur(tc.blur);
    }
    // The look grades the layer's own pixels, then its effects
    // (Sprint 11 grade T001, effects T004).
    crate::blur::apply_grade_to_premult(&mut pm, grade);
    crate::effects::apply_effects_to_premult(&mut pm, effects);
    crate::vfx::apply_vfx_to_premult(&mut pm, vfx, maps);
    let anchor = (tc.x, tc.y);
    let rotated = tc.rotation != 0.0 && tc.rotation.is_finite();
    match (turn, rotated) {
        (None, false) => {
            composite_premult_direct(frame, cw, chh, &pm, grown(card, blur_margin(tc.blur)))
        }
        _ => composite_premult_warp(
            frame,
            cw,
            chh,
            &pm,
            anchor,
            tc.rotation,
            turn,
            warp_bounds(card, anchor, tc.rotation, turn),
        ),
    }
    Ok(())
}

/// Composite one pre-drawn bitmap-fallback layer through blur and 3D
/// turn (Sprint 7 motion, T005/T006): premultiply, blur, then
/// composite directly (still) or through the warp. Shared by the CPU
/// and GPU picture paths; the caller draws the layer and measures the
/// ink-box card. The 2D rotation anchors at `(x, y)`, exactly like
/// `draw_bitmap_text_item`.
pub(crate) fn draw_bitmap_advanced(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    layer: &[u8],
    tc: &crate::frame::ResolvedText,
    turn: Option<Turn>,
    card: (f64, f64, f64, f64),
    grade: &ResolvedGrade,
    effects: &[notation::model::Effect],
    vfx: &notation::model::VfxChain,
    maps: &crate::vfx::VfxMaps<'_>,
) {
    let mut pm = Premult::from_layer(layer, cw as usize, chh as usize);
    if blur_on(tc.blur) {
        pm.blur(tc.blur);
    }
    // The look grades the layer's own pixels, then its effects
    // (Sprint 11 grade T001, effects T004).
    crate::blur::apply_grade_to_premult(&mut pm, grade);
    crate::effects::apply_effects_to_premult(&mut pm, effects);
    crate::vfx::apply_vfx_to_premult(&mut pm, vfx, maps);
    let anchor = (tc.x, tc.y);
    let rotated = tc.rotation != 0.0 && tc.rotation.is_finite();
    match (turn, rotated) {
        (None, false) => {
            composite_premult_direct(frame, cw, chh, &pm, grown(card, blur_margin(tc.blur)))
        }
        _ => composite_premult_warp(
            frame,
            cw,
            chh,
            &pm,
            anchor,
            tc.rotation,
            turn,
            warp_bounds(card, anchor, tc.rotation, turn),
        ),
    }
}

pub(crate) fn draw_description(
    desc: &FrameDescription,
    raster: &mut RasterCache,
    layouts: &[Option<layout::Block>],
    dyn_layouts: &DynLayouts,
    captions: &crate::captions::CaptionLayouts,
    bitmap_fallback: bool,
    videos: &[ProbedVideo],
    ranges: &[DecodedRange],
    images: &[DecodedImage],
    lotties: &[PreparedLottie],
    paths: &crate::path_paint::PathCache,
    folder: &Path,
    frame: &mut [u8],
) -> Result<(), String> {
    // LUT grids resolve once per frame from disk (Sprint 11 grade,
    // CAP-002/T003): the freshness is the mid-render vanish check.
    let mut luts = LutFrameCache::new();
    // Background fill, carrying the composition-wide grade alone
    // (Sprint 11 grade, T001+T002). Empty and position-independent
    // grades fill flat and bit-exact; spatial grades (vignette, grain)
    // paint per pixel.
    let comp_grade = luts.grade(folder, &desc.composition_grade)?;
    fill_graded_background(frame, desc.width, desc.height, desc.background, &comp_grade);
    let cw = desc.width;
    let chh = desc.height;
    for item in &desc.items {
        match item {
            FrameItem::Video(v) => {
                // Inward-out look for this clip (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(v.doc))?;
                let effects = desc.effects_for(v.doc);
                let vfx = desc.vfx_for(v.doc);
                let maps = resolve_vfx_maps(images, folder, vfx)?;
                let vi = videos
                    .iter()
                    .position(|p| p.clip_idx == v.doc)
                    .ok_or_else(|| {
                        format!(
                            "render failed: video clip #{} was not probed (decode stage)",
                            v.doc
                        )
                    })?;
                let pv = &videos[vi];
                let range = &ranges[vi];
                // Sprint 5 T003: a looping clip repeats its source
                // window; a non-looping clip that outruns its source
                // holds the last decoded frame as a still.
                let select_t = if pv.loop_clip {
                    wrap_loop_time(v.src_t, pv.win_start, pv.win_len)
                } else {
                    v.src_t
                };
                let j = pick_picture_or_hold(range, select_t).ok_or_else(|| {
                    format!(
                        "render refused: video source {:?} yields no picture at {:.3}s; no MP4 was written",
                        videos[vi].path.display(),
                        v.src_t
                    )
                })?;
                let fs = range.width as usize * range.height as usize * 3;
                let pic = &range.frames[j * fs..(j + 1) * fs];
                // Sprint 7 motion (T005/T006): blurred or turned video
                // draws through the layer path; everything else blits
                // exactly as before.
                let card = blit_dest(cw, chh, range.width, range.height, v.fit, v.scale, v.x, v.y)
                    .map(|(dx, dy, dw, dh)| (dx, dy, dx + dw, dy + dh));
                let turn = card.and_then(|c| item_turn(v.rotate_x, v.rotate_y, &v.turn, c));
                if blur_on(v.blur)
                    || turn.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_video_advanced(
                        frame,
                        cw,
                        chh,
                        v,
                        pic,
                        range.width,
                        range.height,
                        turn,
                        &grade,
                        effects,
                        vfx,
                        &maps,
                    );
                } else {
                    blit_fit(
                        frame,
                        cw,
                        chh,
                        pic,
                        range.width,
                        range.height,
                        v.fit,
                        v.x,
                        v.y,
                        v.scale,
                        v.rotation,
                        v.opacity,
                    );
                }
            }
            FrameItem::Text(tc) => {
                // Inward-out look for this clip (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(tc.doc))?;
                let effects = desc.effects_for(tc.doc);
                let vfx = desc.vfx_for(tc.doc);
                let maps = resolve_vfx_maps(images, folder, vfx)?;
                if tc.value.is_empty() && !tc.has_spans {
                    // A typewriter reveal at progress 0 shows no pieces:
                    // nothing to draw at this instant. A value-less span
                    // text still paints its shaped runs (Sprint 8, T002).
                    continue;
                }
                // Frame-varying text (typewriter, count, animated
                // metrics) draws its pre-shaped string; still text
                // draws its static layout.
                let block = dyn_layouts
                    .get(tc.doc, &tc.value, tc.size, tc.letter_spacing)
                    .or_else(|| layouts.get(tc.doc).and_then(|laid| laid.as_ref()));
                match block {
                    Some(block) => {
                        // Sprint 7 motion (T005/T006): blurred or turned
                        // text draws through the layer path; everything
                        // else draws exactly as before.
                        let card = (
                            tc.x,
                            tc.y,
                            tc.x + block.width as f64 * tc.scale,
                            tc.y + block.height as f64 * tc.scale,
                        );
                        let turn = item_turn(tc.rotate_x, tc.rotate_y, &tc.turn, card);
                        if blur_on(tc.blur)
                            || turn.is_some()
                            || !grade.is_empty()
                            || !effects.is_empty()
                            || !vfx.is_empty()
                        {
                            draw_text_advanced(
                                frame, cw, chh, raster, block, tc, turn, card, &grade, effects,
                                vfx, &maps,
                            )?;
                        } else {
                            draw_text_block(frame, cw, chh, raster, block, tc)?;
                        }
                    }
                    None if bitmap_fallback => {
                        // The bitmap fallback has no laid-out card: the
                        // probe runs origin-free, and only an active
                        // blur or turn pays for the ink-box card.
                        let probe = turn_probe(tc.rotate_x, tc.rotate_y, tc.turn.perspective);
                        if !(blur_on(tc.blur)
                            || probe.is_some()
                            || !grade.is_empty()
                            || !effects.is_empty()
                            || !vfx.is_empty())
                        {
                            draw_bitmap_text_item(frame, cw, chh, tc);
                        } else if tc.scale <= 0.0 || !tc.scale.is_finite() || tc.opacity <= 0.0 {
                            // Nothing draws, exactly like the legacy path.
                        } else {
                            let mut layer = vec![0u8; cw as usize * chh as usize * 4];
                            draw_bitmap_text_layer(
                                &mut layer,
                                cw,
                                chh,
                                tc,
                                tc.scale,
                                tc.opacity.clamp(0.0, 1.0),
                            );
                            match layer_ink_bbox(&layer, cw, chh) {
                                None => {}
                                Some((x0, y0, x1, y1)) => {
                                    let card =
                                        (x0 as f64, y0 as f64, x1 as f64 + 1.0, y1 as f64 + 1.0);
                                    let turn = item_turn(tc.rotate_x, tc.rotate_y, &tc.turn, card);
                                    draw_bitmap_advanced(
                                        frame, cw, chh, &layer, tc, turn, card, &grade, effects,
                                        vfx, &maps,
                                    );
                                }
                            }
                        }
                    }
                    None => {
                        return Err(
                            "render failed: text layout is missing and no bitmap fallback applies"
                                .to_string(),
                        );
                    }
                }
            }
            FrameItem::Box(b) => {
                // Inward-out look for this box (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(b.doc))?;
                let effects = desc.effects_for(b.doc);
                let vfx = desc.vfx_for(b.doc);
                let maps = resolve_vfx_maps(images, folder, vfx)?;
                // Sprint 8 look (T004): any paint-stack feature draws
                // through the paint stack (backdrop, shadow, blend,
                // mask) in one layout-then-paint pass. Sprint 7 motion
                // (T005/T006): blurred or turned boxes draw through the
                // layer path; everything else draws exactly as before.
                // A graded box always takes a layer path so the look
                // grades its own pixels before compositing.
                // Sprint 8 fills resolve through the same image list
                // with their vanish check.
                let probe = turn_probe(b.rotate_x, b.rotate_y, b.turn.perspective);
                if b.paint_stack_active() {
                    crate::paint::draw_box_paint(
                        frame, cw, chh, b, images, folder, &grade, effects, vfx, &maps,
                    )?;
                } else if blur_on(b.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_box_advanced(
                        frame, cw, chh, b, images, folder, &grade, effects, vfx, &maps,
                    )?;
                } else {
                    draw_box(frame, cw, chh, b, images, folder)?;
                }
            }
            // A vector path paints through the shared paint stack
            // (Sprint 8 look, T005): fill by its fill rule, the
            // positioned stroke above it, then shadow, blend, and
            // mask like boxes.
            FrameItem::Path(q) => {
                // Inward-out look for this clip (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(q.doc))?;
                let effects = desc.effects_for(q.doc);
                let vfx = desc.vfx_for(q.doc);
                let maps = resolve_vfx_maps(images, folder, vfx)?;
                crate::path_paint::draw_path(
                    frame, cw, chh, q, paths, &grade, effects, vfx, &maps,
                )?;
            }
            FrameItem::Image(im) => {
                // Inward-out look for this clip (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(im.doc))?;
                let effects = desc.effects_for(im.doc);
                let vfx = desc.vfx_for(im.doc);
                let maps = resolve_vfx_maps(images, folder, vfx)?;
                let decoded = images
                    .iter()
                    .find(|d| d.path.ends_with(&im.path))
                    .ok_or_else(|| {
                        format!(
                            "render failed: image source {:?} was not decoded (decode stage)",
                            im.path
                        )
                    })?;
                // Sprint 7 motion (T005/T006): blurred or turned images
                // draw through the layer path; everything else draws
                // exactly as before. A graded image takes the layer
                // path so the look grades its own pixels.
                let probe = turn_probe(im.rotate_x, im.rotate_y, im.turn.perspective);
                if blur_on(im.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_image_advanced(frame, cw, chh, im, decoded, &grade, effects, vfx, &maps);
                } else {
                    draw_image(frame, cw, chh, im, decoded);
                }
            }
            // A native Lottie animation (Sprint 21 adapters, T001):
            // the prepared doc rasterizes at the loop-mapped instant
            // pre-composed at the declared size, then rides the
            // shared image blitter with `fill` placement. A looping
            // animation repeats its window; a non-looping one holds
            // its edge frame past the window, exactly like footage.
            FrameItem::Lottie(lo) => {
                // Inward-out look for this clip (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(lo.doc))?;
                let effects = desc.effects_for(lo.doc);
                let vfx = desc.vfx_for(lo.doc);
                let maps = resolve_vfx_maps(images, folder, vfx)?;
                let prep = lotties
                    .iter()
                    .find(|d| d.clip_idx == lo.doc)
                    .ok_or_else(|| {
                        format!(
                            "render failed: lottie source {:?} was not decoded (decode stage)",
                            lo.path
                        )
                    })?;
                let select_t = lottie::sample_time(prep.doc.len_secs(), lo.loop_mode, lo.src_t);
                let decoded = crate::lottie::rasterize(
                    &prep.doc,
                    select_t,
                    lo.width,
                    lo.height,
                    lo.fit,
                    lo.placement,
                    &lo.path,
                )?;
                // The raster is pre-composed at the declared size
                // (like an `svg` clip), so the blitter draws it with
                // `fill` placement.
                let im = crate::frame::ResolvedImage {
                    doc: lo.doc,
                    order: lo.order,
                    path: lo.path.clone(),
                    x: lo.x,
                    y: lo.y,
                    width: Some(lo.width),
                    height: Some(lo.height),
                    fit: notation::model::Fit::Fill,
                    placement: notation::model::Placement::center(),
                    scale: lo.scale,
                    rotation: lo.rotation,
                    opacity: lo.opacity,
                    rotate_x: lo.rotate_x,
                    rotate_y: lo.rotate_y,
                    blur: lo.blur,
                    turn: lo.turn,
                };
                let probe = turn_probe(im.rotate_x, im.rotate_y, im.turn.perspective);
                if blur_on(im.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_image_advanced(frame, cw, chh, &im, &decoded, &grade, effects, vfx, &maps);
                } else {
                    draw_image(frame, cw, chh, &im, &decoded);
                }
            }
            FrameItem::Scene3d(s3) => {
                // Inward-out look for this scene (Sprint 11 grade T001, effects T004).
                let grade = luts.grade(folder, &desc.grade_for(s3.doc))?;
                let effects = desc.effects_for(s3.doc);
                let vfx = desc.vfx_for(s3.doc);
                let maps = resolve_vfx_maps(images, folder, vfx)?;
                let (rw, rh) = crate::scene3d::raster_px(s3.width, s3.height);
                let decoded = crate::scene3d::render_scene(
                    s3.doc,
                    &s3.objects,
                    &s3.camera,
                    &s3.lights,
                    rw,
                    rh,
                );
                // The raster is pre-composed at the evaluated size
                // (like an `svg` clip), so the blitter draws it with
                // `fill` placement.
                let im = crate::frame::ResolvedImage {
                    doc: s3.doc,
                    order: s3.order,
                    path: decoded.path.display().to_string(),
                    x: s3.x,
                    y: s3.y,
                    width: Some(s3.width),
                    height: Some(s3.height),
                    fit: notation::model::Fit::Fill,
                    placement: notation::model::Placement::center(),
                    scale: s3.scale,
                    rotation: s3.rotation,
                    opacity: s3.opacity,
                    rotate_x: s3.rotate_x,
                    rotate_y: s3.rotate_y,
                    blur: s3.blur,
                    turn: s3.turn,
                };
                let probe = turn_probe(im.rotate_x, im.rotate_y, im.turn.perspective);
                if blur_on(im.blur)
                    || probe.is_some()
                    || !grade.is_empty()
                    || !effects.is_empty()
                    || !vfx.is_empty()
                {
                    draw_image_advanced(frame, cw, chh, &im, &decoded, &grade, effects, vfx, &maps);
                } else {
                    draw_image(frame, cw, chh, &im, &decoded);
                }
            }
            FrameItem::Caption(rc) => {
                // Caption words paint through the caption painter
                // (Sprint 22 captions, T001/T002/T003): the track
                // style, overrides, and presets resolve the word
                // pixels, and the matte covers them behind the
                // subject. The look resolves here like every other
                // layer (a vanished LUT refuses naming the file
                // before any word paints); grade, effect, and VFX
                // chains ride the item for the fingerprint and for
                // later slices, and word paint in this slice draws
                // the resolved caption style directly.
                let _look = (
                    luts.grade(folder, &desc.grade_for(rc.doc))?,
                    desc.effects_for(rc.doc),
                    desc.vfx_for(rc.doc),
                );
                crate::captions::draw_caption(frame, cw, chh, raster, captions, images, rc)?;
            }
        }
    }
    Ok(())
}

/// Decode one composition's Lottie animations (Sprint 21 adapters,
/// T001): every `lottie` clip re-decodes its file here, so a file
/// validation accepted but the decoder cannot read refuses before
/// the first frame, naming the file — never a partial MP4. An empty
/// clip list decodes nothing. Docs accumulate in document order at
/// their global document ids; the decoded bytes stay in memory, so a
/// source deleted mid-render still draws the prepared picture.
fn prepare_comp_lotties(
    comp: &Composition,
    clip_base: usize,
    folder: &Path,
) -> Result<Vec<PreparedLottie>, String> {
    let mut out = Vec::new();
    for (idx, clip) in comp.clips.iter().enumerate() {
        let notation::model::Clip::Lottie(lo) = clip else {
            continue;
        };
        let abs = folder.join(&lo.path);
        let bytes = std::fs::read(&abs).map_err(|e| {
            format!(
                "render refused: lottie source {:?} cannot be read ({e}); no MP4 was written",
                lo.path
            )
        })?;
        let doc = lottie::decode_source(&lo.path, &bytes).map_err(|detail| {
            format!(
                "render refused: lottie source {:?} is undecodable ({detail}); no MP4 was written",
                lo.path
            )
        })?;
        out.push(PreparedLottie {
            clip_idx: clip_base + idx,
            doc,
        });
    }
    Ok(out)
}

/// Visible clip window clamped to the composition, as composition seconds.
/// `rate` maps these to source seconds; `trim_start` anchors the window.
fn clip_window_len(comp: &Composition, v: &notation::model::VideoClip) -> f64 {
    (v.start + v.duration).min(comp.duration) - v.start
}

/// Effective source window of one `video` clip against the probed
/// source (Sprint 5 T002): the window opens at `trim_start` and closes
/// at the declared `trim-end`, or at the probed source end when none is
/// declared. The probe wins — a past-end `trim-end` clamps to the true
/// end here (check output already reported it) instead of decoding a
/// guess. The end is `None` only when neither a `trim-end` nor a probed
/// duration is known.
pub(crate) fn effective_source_window(
    v: &notation::model::VideoClip,
    probed: Option<f64>,
) -> (f64, Option<f64>) {
    let end = match (v.trim_end, probed) {
        (Some(e), Some(src)) => Some(e.min(src)),
        (Some(e), None) => Some(e),
        (None, src) => src,
    };
    (v.trim_start, end)
}

/// Fallback duration from `ffprobe`'s format metadata (Sprint 25):
/// some builds report no `Duration` in `ffmpeg -i` stderr for
/// containers (notably GIF) whose format duration is exact. Only
/// consulted when the primary `ffmpeg -i` probe yields none.
pub(crate) fn probe_format_duration(ffmpeg_bin: &Path, abs: &Path) -> Option<f64> {
    let ffprobe = ffprobe_for(ffmpeg_bin)?;
    let out = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &abs.display().to_string(),
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let value: f64 = text.trim().parse().ok()?;
    if value.is_finite() && value > 0.0 {
        Some(value)
    } else {
        None
    }
}

/// Locate the `ffprobe` twin of the resolved `ffmpeg` binary (Sprint 5
/// T004): the sibling `ffprobe` beside it when present, else the first
/// `ffprobe` on `PATH`. `None` means packet timestamps cannot be probed
/// and the decode below falls back to a bounded backtrack.
pub(crate) fn ffprobe_for(ffmpeg_bin: &Path) -> Option<PathBuf> {
    if let Some(dir) = ffmpeg_bin.parent() {
        if !dir.as_os_str().is_empty() {
            #[cfg(windows)]
            let name = "ffprobe.exe";
            #[cfg(not(windows))]
            let name = "ffprobe";
            let sibling = dir.join(name);
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        #[cfg(windows)]
        let name = "ffprobe.exe";
        #[cfg(not(windows))]
        let name = "ffprobe";
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|cand| cand.is_file())
    })
}

/// Parse one `ffprobe -show_entries packet=pts_time` reply (Sprint 5
/// T004): every packet must carry a finite numeric presentation
/// timestamp. Any missing (`N/A`) or unparseable entry means the
/// source timing cannot be trusted, so the whole reply is unusable —
/// the caller refuses naming the file instead of guessing.
fn parse_packet_pts(text: &str) -> Result<Vec<f64>, ()> {
    let mut pts = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line.parse::<f64>() {
            Ok(t) if t.is_finite() => pts.push(t),
            _ => return Err(()),
        }
    }
    if pts.is_empty() {
        return Err(());
    }
    Ok(pts)
}

/// Probe one source's packet presentation timestamps, oldest to newest
/// in presentation order (Sprint 5 T004): `ffprobe` reads the container
/// packet index without decoding, so even long files probe fast. Packet
/// order is decode order (B-frames!), so the timestamps sort into the
/// presentation timeline output instants sample against.
///
/// The result is `Some` sorted timeline on success; `None` when the
/// container lists no video packet timestamps at all (for example a
/// file whose audio box is damaged while its video is intact — the
/// video decode below still gates timing, and the audio stage still
/// refuses naming the file). Any listed-but-unparseable entry (`N/A`
/// from a timing-less stream) refuses naming the file, with
/// re-encoding as the fix — never guessed timing.
fn probe_packet_pts(ffprobe_bin: &Path, pv: &ProbedVideo) -> Result<Option<Vec<f64>>, String> {
    // A single authoritative probe run: its stdout decides. The exit
    // status is deliberately ignored — a damaged audio box can fail the
    // run while the listed video timestamps stay exact.
    let stdout = match Command::new(ffprobe_bin)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "csv=p=0",
            &pv.path.display().to_string(),
        ])
        .output()
    {
        // The probe binary vanished mid-run: fall back to the bounded
        // decode below, which still gates decoder-level timing.
        Err(_) => return Ok(None),
        Ok(out) => out.stdout,
    };
    let text = String::from_utf8_lossy(&stdout);
    let listed: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if listed.is_empty() {
        return Ok(None);
    }
    let mut pts = parse_packet_pts(&text).map_err(|_| {
        format!(
            "render refused: video source {:?} has unparseable timing and cannot be sampled exactly; re-encode the source to fix its timestamps; no MP4 was written",
            pv.path.display()
        )
    })?;
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Some(pts))
}

/// Decode start for a clip window opening at `seek` (Sprint 5 T004):
/// the covering frame — the last presentation timestamp at or before
/// `seek` — so output-seeking never drops the frame whose interval
/// covers the window opening (the VFR gap case). The decode seeks to
/// 1 ms before that frame: the covering frame is always emitted, any
/// extra earlier frame is harmless (selection is by timestamp), and a
/// zero start keeps the historical no-`-ss` argument shape.
fn decode_start_for_seek(sorted_pts: &[f64], seek: f64) -> f64 {
    let mut start = 0.0;
    for &t in sorted_pts {
        if t <= seek + 1e-9 {
            start = t;
        } else {
            break;
        }
    }
    (start - 0.001).max(0.0)
}

/// Frame budget for a decode of `[target, end]` source seconds (Sprint
/// 5 T004): every packet timestamp in the window, plus two spare slots
/// for container fuzz (edit lists, rounding) — the `-frames:v` cap only
/// bounds the run, never the selection, which stays timestamp-exact.
fn decode_frame_budget(sorted_pts: &[f64], target: f64, end: f64) -> usize {
    let in_window = sorted_pts
        .iter()
        .filter(|t| **t >= target - 1e-9 && **t <= end + 1e-6)
        .count();
    in_window + 2
}

/// Decode one clip window `[seek, seek + src_len)` to raw pictures with
/// their true presentation timestamps (`showinfo`). One sequential pass
/// per clip: output frames then select pictures by exact timestamp, so
/// variable-frame-rate sources sample the frame whose interval covers
/// each output instant — no duplicated, dropped, or slipped frames
/// beyond what the source timestamps declare.
///
/// The decode runs with `passthrough` frame mode (the vsync CFR
/// duplicator stays off) and `copyts` absolute timestamps, seeking at
/// the demuxer to just before the covering frame, so the first picture
/// is the source frame at-or-before `seek` and every `pts_time` is
/// already source seconds: picture selection (`src_t`) needs no shift
/// arithmetic. A zero start keeps the historical no-`-ss` argument
/// shape so pre-footage decodes stay byte-identical. Container rotation
/// (`autorotate`) orients the pixels to the probed dimensions, so `fit`
/// and placement see the player-visible picture.
fn decode_range(
    ffmpeg_bin: &Path,
    pv: &ProbedVideo,
    seek: f64,
    src_len: f64,
) -> Result<DecodedRange, String> {
    // Sprint 5 T004: packet timestamps decide the decode start and
    // expose unparseable timing before a single picture renders. When
    // the container lists no video packet timestamps (or no ffprobe is
    // available at all), fall back to a bounded 0.5 s backtrack with a
    // generous frame cap: timing stays timestamp-exact (showinfo still
    // gates every picture); only a long-gap covering frame beyond the
    // backtrack could miss — an environmental degradation, never a
    // guess.
    //
    // Sprint 5 T007: GIF sources always decode from the start. The GIF
    // demuxer honors an input `-ss` by dropping the pictures before it
    // — including the covering frame the window needs — so a seeked
    // GIF decode would time-slip. GIFs are short; decoding from zero
    // with a packet-counted budget keeps every native timestamp
    // (including per-frame delays) exact.
    let fallback = || {
        (
            (seek - 0.5).max(0.0),
            ((src_len + 0.5) * 60.0).ceil() as usize + 8,
        )
    };
    let (target, budget) = if pv.codec == "gif" {
        match ffprobe_for(ffmpeg_bin) {
            Some(ffprobe) => match probe_packet_pts(&ffprobe, pv)? {
                Some(sorted) => {
                    let end = seek + src_len.max(0.0);
                    let count = sorted.iter().filter(|t| **t <= end + 1e-6).count();
                    (0.0, count + 2)
                }
                None => fallback(),
            },
            None => fallback(),
        }
    } else {
        match ffprobe_for(ffmpeg_bin) {
            Some(ffprobe) => match probe_packet_pts(&ffprobe, pv)? {
                Some(sorted) => {
                    let target = decode_start_for_seek(&sorted, seek);
                    let end = seek + src_len.max(0.0);
                    (target, decode_frame_budget(&sorted, target, end))
                }
                None => fallback(),
            },
            None => fallback(),
        }
    };
    let mut args: Vec<String> = vec!["-xerror".into(), "-v".into(), "info".into()];
    // Rotation applies through `transpose` (`-vf` below), not the
    // `-autorotate` flag: the flag's option placement is rejected by
    // older FFmpeg builds (e.g. 6.1), while `transpose` works on
    // every version. `-noautorotate` disables the default autorotate
    // first, so the filter is the only rotation applied.
    let transpose = if needs_autorotate(pv.rotation) {
        args.push("-noautorotate".into());
        Some(transpose_chain(pv.rotation))
    } else {
        None
    };
    if target > 0.0 {
        // Input seek: the demuxer jumps to the nearest seek point at or
        // before `target` and drops earlier frames by timestamp, so the
        // covering frame is emitted with its absolute timestamp intact
        // (`copyts` below). An output seek would instead renumber
        // timestamps from the seek point and drop the covering frame.
        args.push("-ss".into());
        args.push(format!("{target:.6}"));
    }
    args.extend([("-i".to_string()), (pv.path.display().to_string())]);
    if target > 0.0 {
        args.push("-copyts".into());
    }
    let vf = match &transpose {
        Some(t) => format!("{t},showinfo"),
        None => "showinfo".to_string(),
    };
    args.extend([
        "-fps_mode".into(),
        "passthrough".into(),
        "-frames:v".into(),
        format!("{}", budget.max(1)),
        "-vf".into(),
        vf,
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-".into(),
    ]);
    let out = Command::new(ffmpeg_bin).args(&args).output().map_err(|e| {
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
    // `copyts` keeps absolute presentation timestamps, so `pts_time`
    // already speaks source seconds — no shift arithmetic. A missing or
    // non-numeric timestamp line means decoder-level timing loss and
    // refuses exactly like the packet gate above.
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
    // The `-frames:v` cap can end the run right as one extra `showinfo`
    // line for a boundary frame logs: that frame has no picture, so it
    // carries no mapping. Anything fewer than the picture count is
    // still a refusal, never a guess.
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

/// Wrap one source time into a loop window (Sprint 5 T003): the
/// repeated unit is the trimmed window played at the declared rate, so
/// composition seconds past the clip start advance `rate` source
/// seconds modulo the window length — picture and sound repeat
/// together with no gap at the loop point (a remainder landing on the
/// window end shows the first frame again). A non-positive or unknown
/// window never wraps, so looping degrades to hold there.
pub(crate) fn wrap_loop_time(src_t: f64, start: f64, len: Option<f64>) -> f64 {
    match len {
        Some(w) if w.is_finite() && w > 0.0 => {
            let mut off = (src_t - start) % w;
            if off < 0.0 {
                off += w;
            }
            if (w - off) < 1e-9 {
                off = 0.0;
            }
            start + off
        }
        _ => src_t,
    }
}

/// Picture index covering `src_t`, holding the last picture past the
/// source end (Sprint 5 T003): a non-looping clip whose window outruns
/// its source holds the last decoded frame as a still while its sound
/// goes silent for the remainder. Before the first timestamp the first
/// picture applies; an empty range still yields none.
pub(crate) fn pick_picture_or_hold(range: &DecodedRange, src_t: f64) -> Option<usize> {
    match pick_picture(range, src_t) {
        Some(i) => Some(i),
        // [`pick_picture`] yields None only past the source end: hold
        // the last decoded picture instead of refusing.
        None => range.pts.len().checked_sub(1),
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
fn fit_rect(cw: u32, chh: u32, sw: u32, sh: u32, fit: Fit) -> (f64, f64, f64, f64) {
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
    let blend = |frame: &mut [u8],
                 o: usize,
                 p00: usize,
                 p10: usize,
                 p01: usize,
                 p11: usize,
                 fx: f64,
                 fy: f64| {
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
                frame[o + c] = (v as f64 * alpha + frame[o + c] as f64 * inva + 0.5) as u8;
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
                        frame[orow + k] = (src[srow + k] as f64 * alpha
                            + frame[orow + k] as f64 * inva
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
pub(crate) fn draw_block_scaled(
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
            face.blit_gid(
                frame,
                cw,
                chh,
                g.id,
                g.ch,
                run.size as f64 * s,
                x + g.x as f64 * s,
                y + g.y as f64 * s,
                color,
                eff,
            );
        }
    }
    for pic in &block.pictures {
        blend_rgba(
            frame,
            cw,
            chh,
            &pic.rgba,
            pic.width,
            pic.height,
            (x + pic.x as f64 * s).round() as i64,
            (y + pic.y as f64 * s).round() as i64,
        );
    }
    Ok(())
}

/// Draw one shaped text block into an RGBA `layer` instead of the
/// frame, for the rotation/fade path: same pens and faces as
/// [`draw_block_scaled`], carrying `global_alpha` in the layer alpha.
pub(crate) fn draw_block_layer(
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
            face.blit_gid_layer(
                layer,
                cw,
                chh,
                g.id,
                g.ch,
                run.size as f64 * s,
                x + g.x as f64 * s,
                y + g.y as f64 * s,
                color,
                eff,
                global_alpha,
            );
        }
    }
    for pic in &block.pictures {
        blend_rgba_layer(
            layer,
            cw,
            chh,
            &pic.rgba,
            pic.width,
            pic.height,
            (x + pic.x as f64 * s).round() as i64,
            (y + pic.y as f64 * s).round() as i64,
            global_alpha,
        );
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
            face.blit_gid_faded(
                frame,
                cw,
                chh,
                g.id,
                g.ch,
                run.size as f64 * s,
                x + g.x as f64 * s,
                y + g.y as f64 * s,
                color,
                eff,
                global_alpha,
            );
        }
    }
    debug_assert!(
        block.pictures.is_empty(),
        "faded direct path takes no emoji pictures"
    );
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
            return draw_block_scaled_faded(
                frame,
                cw,
                chh,
                raster,
                block,
                tc.x,
                tc.y,
                tc.scale,
                tc.opacity.clamp(0.0, 1.0),
            );
        }
        let mut layer = vec![0u8; cw as usize * chh as usize * 4];
        draw_block_layer(
            &mut layer,
            cw,
            chh,
            raster,
            block,
            tc.x,
            tc.y,
            tc.scale,
            tc.opacity.clamp(0.0, 1.0),
        )?;
        if let Some(bbox) = layer_ink_bbox(&layer, cw, chh) {
            composite_layer_bbox(frame, cw, chh, &layer, bbox, tc.x, tc.y, 0.0);
        }
        return Ok(());
    }
    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
    draw_block_layer(
        &mut layer,
        cw,
        chh,
        raster,
        block,
        tc.x,
        tc.y,
        tc.scale,
        tc.opacity.clamp(0.0, 1.0),
    )?;
    if let Some(bbox) = layer_ink_bbox(&layer, cw, chh) {
        composite_layer_bbox(frame, cw, chh, &layer, bbox, tc.x, tc.y, tc.rotation);
    }
    Ok(())
}

/// Bitmap last resort: the composition declares no `font` and the
/// machine offers none. Draws from the resolved description with the
/// built-in 5x7 face (unknown cells draw hollow, never blank),
/// announced in the note. Bitmap text scaled by `s` about `(x, y)`.
fn draw_bitmap_text_scaled(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    tc: &crate::frame::ResolvedText,
    x: f64,
    y: f64,
    s: f64,
) {
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
            face.blit(
                frame,
                cw,
                chh,
                g.ch,
                tc.size * s,
                x + g.x * s + dx * s,
                baseline,
                color,
                tc.style,
            );
            dx += spacing;
        }
    }
}

/// Bitmap text item with motion (T010): still transforms draw directly;
/// rotated or faded text renders into a scratch layer and composites only
/// its ink box (T004 speed).
pub(crate) fn draw_bitmap_text_item(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    tc: &crate::frame::ResolvedText,
) {
    if tc.scale <= 0.0 || !tc.scale.is_finite() || tc.opacity <= 0.0 {
        return;
    }
    if (tc.rotation == 0.0 || !tc.rotation.is_finite()) && tc.opacity >= 1.0 {
        draw_bitmap_text_scaled(frame, cw, chh, tc, tc.x, tc.y, tc.scale);
        return;
    }
    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
    draw_bitmap_text_layer(
        &mut layer,
        cw,
        chh,
        tc,
        tc.scale,
        tc.opacity.clamp(0.0, 1.0),
    );
    if let Some(bbox) = layer_ink_bbox(&layer, cw, chh) {
        composite_layer_bbox(
            frame,
            cw,
            chh,
            &layer,
            bbox,
            tc.x,
            tc.y,
            if tc.rotation.is_finite() {
                tc.rotation
            } else {
                0.0
            },
        );
    }
}

/// Bitmap text into an RGBA `layer`, carrying `global_alpha`.
pub(crate) fn draw_bitmap_text_layer(
    layer: &mut [u8],
    cw: u32,
    chh: u32,
    tc: &crate::frame::ResolvedText,
    s: f64,
    global_alpha: f64,
) {
    let face = Face::fallback();
    let laid = face.layout(&tc.value, tc.size, None);
    let advance = tc.line_height.unwrap_or(laid.line_height);
    let spacing = tc.letter_spacing.unwrap_or(0.0).max(0.0);
    let color = (tc.color.r, tc.color.g, tc.color.b);
    for (li, line) in laid.lines.iter().enumerate() {
        let baseline = tc.y + laid.ascent * s + li as f64 * advance * s;
        let mut dx = 0.0;
        for g in &line.glyphs {
            face.blit_gid_layer(
                layer,
                cw,
                chh,
                0,
                g.ch,
                tc.size * s,
                tc.x + g.x * s + dx * s,
                baseline,
                color,
                tc.style,
                global_alpha,
            );
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
pub(crate) fn layer_ink_bbox(layer: &[u8], cw: u32, chh: u32) -> Option<(i64, i64, i64, i64)> {
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
pub(crate) fn composite_layer_bbox(
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
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
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
            layer[o] =
                ((rgba[s] as f64 * a + layer[o] as f64 * dst_a * (1.0 - a)) / out_a + 0.5) as u8;
            layer[o + 1] = ((rgba[s + 1] as f64 * a + layer[o + 1] as f64 * dst_a * (1.0 - a))
                / out_a
                + 0.5) as u8;
            layer[o + 2] = ((rgba[s + 2] as f64 * a + layer[o + 2] as f64 * dst_a * (1.0 - a))
                / out_a
                + 0.5) as u8;
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
    use notation::model::{Fit, PlaceH, PlaceV, Placement, Rgb, TrackKind};
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
            fill_paint: FillPaint::Solid,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: notation::model::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: notation::model::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: notation::model::Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
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
        assert_eq!(
            decoded.scaled.lock().unwrap().len(),
            1,
            "miss inserts one entry"
        );
        draw_image(&mut b, 32, 32, &im, &decoded);
        assert_eq!(
            decoded.scaled.lock().unwrap().len(),
            1,
            "same geometry hits"
        );
        assert_eq!(a, b, "hit draws byte-identical pixels");
        assert_ne!(a, vec![3u8; 32 * 32 * 3], "the picture actually painted");
        let mut im2 = im.clone();
        im2.x = 4.0;
        let mut c = vec![3u8; 32 * 32 * 3];
        draw_image(&mut c, 32, 32, &im2, &decoded);
        assert_eq!(
            decoded.scaled.lock().unwrap().len(),
            2,
            "moved picture keys again"
        );
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
        assert_eq!(
            faded_hit, faded_fresh,
            "cached rect fades exactly like a fresh sample"
        );
    }

    #[test]
    fn opaque_box_hides_what_is_beneath() {
        // T004: a fully opaque box paints its fill exactly; outside the
        // box the background shows through.
        let mut frame = vec![10u8; 32 * 32 * 3];
        let b = test_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        draw_box(&mut frame, 32, 32, &b, &[], Path::new(".")).unwrap();
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
        draw_box(&mut frame, 32, 32, &b, &[], Path::new(".")).unwrap();
        assert!(frame.iter().all(|&v| v == 10));
    }

    #[test]
    fn half_opacity_blends_with_the_canvas() {
        // T004: opacity 0.5 blends fill and background evenly.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let mut b = test_box(
            8.0,
            8.0,
            16.0,
            16.0,
            Rgb {
                r: 200,
                g: 100,
                b: 0,
            },
        );
        b.opacity = 0.5;
        draw_box(&mut frame, 32, 32, &b, &[], Path::new(".")).unwrap();
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
        draw_box(&mut frame, 32, 32, &b, &[], Path::new(".")).unwrap();
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
        draw_box(&mut frame, 32, 32, &b, &[], Path::new(".")).unwrap();
        assert_eq!(pixel(&frame, 32, 8, 8), (0, 0, 0), "corner is cut");
        assert_eq!(pixel(&frame, 32, 16, 8), (0, 255, 0), "edge middle paints");
        assert_eq!(pixel(&frame, 32, 16, 16), (0, 255, 0));
    }

    #[test]
    fn image_fit_sizes_match_object_fit() {
        // T005: `fill` stretches, `contain` letterboxes, `cover` bleeds.
        assert_eq!(
            image_fit_size(100.0, 100.0, 200.0, 50.0, Fit::Fill),
            (100.0, 100.0)
        );
        assert_eq!(
            image_fit_size(100.0, 100.0, 200.0, 50.0, Fit::Contain),
            (100.0, 25.0)
        );
        assert_eq!(
            image_fit_size(100.0, 100.0, 200.0, 50.0, Fit::Cover),
            (400.0, 100.0)
        );
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
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
        }
    }

    #[test]
    fn fill_image_covers_the_target_rect() {
        // T005: a `fill` picture stretches over the whole target.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let decoded = solid_image(4, 4, 255, 0, 0, 255);
        draw_image(
            &mut frame,
            32,
            32,
            &test_image(Fit::Fill, Placement::center()),
            &decoded,
        );
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
            &test_image(
                Fit::Contain,
                Placement {
                    x: PlaceH::Left,
                    y: PlaceV::Center,
                },
            ),
            &decoded,
        );
        draw_image(
            &mut right,
            32,
            32,
            &test_image(
                Fit::Contain,
                Placement {
                    x: PlaceH::Right,
                    y: PlaceV::Center,
                },
            ),
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
        draw_image(
            &mut frame,
            32,
            32,
            &test_image(Fit::Fill, Placement::center()),
            &decoded,
        );
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
            fill_paint: FillPaint::Solid,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: notation::model::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: notation::model::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: notation::model::Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
        }
    }

    #[test]
    fn scaled_box_doubles_its_footprint() {
        // T010: scale 2 about the top-left grows 16x16 to 32x32.
        let mut frame = vec![0u8; 32 * 32 * 3];
        let mut b = motion_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        b.scale = 2.0;
        draw_box(&mut frame, 32, 32, &b, &[], Path::new(".")).unwrap();
        assert_eq!(pixel(&frame, 32, 8, 8), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 23, 23), (255, 0, 0));
        assert_eq!(pixel(&frame, 32, 31, 31), (255, 0, 0));
        assert_eq!(
            pixel(&frame, 32, 7, 16),
            (0, 0, 0),
            "top-left anchors the growth"
        );
        // Half scale shrinks to 8x8.
        let mut half = vec![0u8; 32 * 32 * 3];
        let mut h = motion_box(8.0, 8.0, 16.0, 16.0, Rgb { r: 255, g: 0, b: 0 });
        h.scale = 0.5;
        draw_box(&mut half, 32, 32, &h, &[], Path::new(".")).unwrap();
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
        draw_box(&mut frame, 32, 32, &b, &[], Path::new(".")).unwrap();
        assert_eq!(
            pixel(&frame, 32, 16, 8),
            (0, 255, 0),
            "center stays covered"
        );
        assert_eq!(
            pixel(&frame, 32, 16, 0),
            (0, 255, 0),
            "turned box reaches the top middle"
        );
        assert_eq!(
            pixel(&frame, 32, 16, 15),
            (0, 255, 0),
            "and the bottom middle"
        );
        assert_eq!(
            pixel(&frame, 32, 8, 4),
            (0, 0, 0),
            "old corner is now empty"
        );
        assert_eq!(
            pixel(&frame, 32, 23, 4),
            (0, 0, 0),
            "old corner is now empty"
        );
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
        blit_fit(
            &mut still,
            32,
            32,
            &src,
            sw,
            sh,
            Fit::Fill,
            0.0,
            0.0,
            1.0,
            0.0,
            1.0,
        );
        assert_eq!(pixel(&still, 32, 0, 0), (255, 0, 0));
        assert_eq!(pixel(&still, 32, 31, 31), (255, 0, 0));
        // Shifted right by 8: left strip keeps the canvas.
        let mut moved = vec![9u8; 32 * 32 * 3];
        blit_fit(
            &mut moved,
            32,
            32,
            &src,
            sw,
            sh,
            Fit::Fill,
            8.0,
            0.0,
            1.0,
            0.0,
            1.0,
        );
        assert_eq!(pixel(&moved, 32, 0, 16), (9, 9, 9));
        assert_eq!(pixel(&moved, 32, 8, 16), (255, 0, 0));
        // Opacity 0 paints nothing.
        let mut gone = vec![9u8; 32 * 32 * 3];
        blit_fit(
            &mut gone,
            32,
            32,
            &src,
            sw,
            sh,
            Fit::Fill,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
        );
        assert!(gone.iter().all(|&v| v == 9));
        // Half opacity blends evenly.
        let mut half = vec![0u8; 32 * 32 * 3];
        blit_fit(
            &mut half,
            32,
            32,
            &src,
            sw,
            sh,
            Fit::Fill,
            0.0,
            0.0,
            1.0,
            0.0,
            0.5,
        );
        assert_eq!(pixel(&half, 32, 16, 16), (128, 0, 0));
    }

    fn test_text(
        value: &str,
        scale: f64,
        rotation: f64,
        opacity: f64,
    ) -> crate::frame::ResolvedText {
        crate::frame::ResolvedText {
            doc: 0,
            order: 0,
            value: value.to_string(),
            has_spans: false,
            x: 4.0,
            y: 4.0,
            size: 14.0,
            color: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            style: notation::model::FontStyle::Regular,
            line_height: None,
            letter_spacing: None,
            scale,
            rotation,
            opacity,
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
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
        assert!(
            (bx1 - bx0) >= (ax1 - ax0) * 2 - 2,
            "width doubles: {ax0}..{ax1} vs {bx0}..{bx1}"
        );
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
        assert!(
            peak(&half) > 20 && peak(&half) < peak(&full),
            "faded ink blends"
        );
    }

    #[test]
    fn blurred_bitmap_text_draws_and_rerenders_identically() {
        // T006: blurred bitmap text draws non-empty through the
        // advanced path and re-renders byte-identically (no system
        // fonts needed: the fallback face shapes the ink).
        let mut tc = test_text("HI", 1.0, 0.0, 1.0);
        tc.blur = 1.5;
        let mut layer = vec![0u8; 64 * 32 * 4];
        draw_bitmap_text_layer(&mut layer, 64, 32, &tc, tc.scale, 1.0);
        let (x0, y0, x1, y1) = layer_ink_bbox(&layer, 64, 32).expect("fallback ink draws");
        let card = (x0 as f64, y0 as f64, x1 as f64 + 1.0, y1 as f64 + 1.0);
        let mut frame = vec![0u8; 64 * 32 * 3];
        draw_bitmap_advanced(
            &mut frame,
            64,
            32,
            &layer,
            &tc,
            None,
            card,
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        );
        let (_, _, _, n) = ink_extent(&frame, 64);
        assert!(n > 10, "blurred ink draws, got {n}");
        let mut again = vec![0u8; 64 * 32 * 3];
        draw_bitmap_advanced(
            &mut again,
            64,
            32,
            &layer,
            &tc,
            None,
            card,
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        );
        assert_eq!(frame, again, "blurred re-render is byte-identical");
        let mut sharp = vec![0u8; 64 * 32 * 3];
        draw_bitmap_text_item(&mut sharp, 64, 32, &test_text("HI", 1.0, 0.0, 1.0));
        assert_ne!(frame, sharp, "blur changes the ink");
    }

    #[test]
    fn dyn_layout_triples_key_size_and_spacing() {
        // T006: dynamic layouts key the full (doc, string, size,
        // spacing) triple, so animated metrics never alias the static
        // block or each other.
        let mut dyn_ = DynLayouts::default();
        let block = || layout::Block {
            width: 1.0,
            height: 2.0,
            runs: Vec::new(),
            pictures: Vec::new(),
            lines: Vec::new(),
        };
        assert!(dyn_.get(0, "Hi", 16.0, None).is_none());
        dyn_.insert(0, "Hi".to_string(), 16.0, None, block());
        dyn_.insert(0, "Hi".to_string(), 32.0, None, block());
        dyn_.insert(0, "Hi".to_string(), 16.0, Some(2.0), block());
        assert!(dyn_.get(0, "Hi", 16.0, None).is_some());
        assert!(dyn_.get(0, "Hi", 32.0, None).is_some());
        assert!(dyn_.get(0, "Hi", 16.0, Some(2.0)).is_some());
        assert!(dyn_.get(0, "Hi", 24.0, None).is_none(), "other sizes miss");
        assert!(
            dyn_.get(0, "Hi", 16.0, Some(1.0)).is_none(),
            "other spacing misses"
        );
        assert!(
            dyn_.get(0, "Bye", 16.0, None).is_none(),
            "other strings miss"
        );
        assert!(dyn_.contains(0, "Hi", 32.0, None));
        assert!(!dyn_.contains(0, "Hi", 33.0, None));
    }

    fn span_clip(spans: Vec<notation::model::TextSpan>) -> notation::model::TextClip {
        notation::model::TextClip {
            asserts: Default::default(),
            value: String::new(),
            start: 0.0,
            duration: 2.0,
            id: None,
            track: notation::model::TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            x: 0.0,
            y: 0.0,
            size: 24.0,
            color: notation::model::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            align: notation::model::Align::Left,
            wrap: notation::model::Wrap::Off,
            style: notation::model::FontStyle::Regular,
            order: 0,
            line_height: None,
            letter_spacing: None,
            font_family: None,
            direction: notation::model::TextDirection::Ltr,
            spans,
            split: None,
            typewriter: false,
            count_from: None,
            count_to: None,
            decimals: 0,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        }
    }

    fn span(value: &str) -> notation::model::TextSpan {
        notation::model::TextSpan {
            value: value.to_string(),
            font_family: None,
            font_size: None,
            font_weight: None,
            italic: None,
            color: None,
            letter_spacing: None,
            line_height: None,
            children: Vec::new(),
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn spans_shape_each_run_with_own_paint() {
        // T002: two runs with distinct sizes and colors shape into one
        // block carrying both paints, in document order.
        let mut red = span("Hi ");
        red.font_size = Some(24.0);
        red.color = Some(notation::model::Rgb { r: 255, g: 0, b: 0 });
        let mut green = span("there");
        green.font_size = Some(48.0);
        green.color = Some(notation::model::Rgb { r: 0, g: 255, b: 0 });
        let tc = span_clip(vec![red, green]);
        let mut fonts = layout::Fonts::new();
        let block = shape_text_string(&mut fonts, &None, &tc, "", Path::new("ffmpeg"))
            .expect("spans must shape")
            .expect("machine fonts must serve latin");
        let text: String = block
            .runs
            .iter()
            .flat_map(|r| r.glyphs.iter().map(|g| g.ch))
            .collect();
        assert_eq!(text, "Hi there");
        let sizes: Vec<f32> = block.runs.iter().map(|r| r.size).collect();
        assert!(sizes.contains(&24.0) && sizes.contains(&48.0), "{sizes:?}");
        let colours: Vec<[u8; 4]> = block.runs.iter().map(|r| r.colour).collect();
        assert!(colours.contains(&[255, 0, 0, 255]));
        assert!(colours.contains(&[0, 255, 0, 255]));
    }

    #[test]
    fn spans_unknown_family_refuses_naming_span_before_frame_one() {
        // T002: a run whose family list nothing resolves refuses before
        // frame one naming the span, never falling back silently.
        let mut bad = span("Hi");
        bad.font_family = Some(vec!["no-such-family-xyz".to_string()]);
        let tc = span_clip(vec![bad, span("there")]);
        let mut fonts = layout::Fonts::new();
        let err = shape_text_string(&mut fonts, &None, &tc, "", Path::new("ffmpeg"))
            .expect_err("unknown run family must refuse");
        assert!(err.contains("span 1"), "refusal names the span: {err}");
        assert!(
            err.contains("font-family"),
            "refusal names the property: {err}"
        );
        assert!(
            err.contains("no MP4 was written"),
            "refusal leaves no output: {err}"
        );
    }

    #[test]
    fn spans_empty_shaping_refuses_naming_span() {
        // T002: a run shaping to no glyphs (here a newline-only run)
        // refuses naming the span.
        let tc = span_clip(vec![span("Hi"), span("\n")]);
        let mut fonts = layout::Fonts::new();
        let err = shape_text_string(&mut fonts, &None, &tc, "", Path::new("ffmpeg"))
            .expect_err("empty shaping must refuse");
        assert!(err.contains("span 2"), "refusal names the span: {err}");
    }

    #[test]
    fn spans_nested_runs_inherit_enclosing_style() {
        // T002: a nested span without overrides paints in its parent's
        // size and color.
        let mut outer = span("");
        outer.font_size = Some(36.0);
        outer.color = Some(notation::model::Rgb { r: 0, g: 0, b: 255 });
        outer.children = vec![span("inner")];
        let tc = span_clip(vec![outer]);
        let mut fonts = layout::Fonts::new();
        let block = shape_text_string(&mut fonts, &None, &tc, "", Path::new("ffmpeg"))
            .expect("nested spans must shape")
            .expect("machine fonts must serve latin");
        assert_eq!(block.runs.len(), 1);
        assert_eq!(block.runs[0].size, 36.0);
        assert_eq!(block.runs[0].colour, [0, 0, 255, 255]);
    }

    #[test]
    fn turned_blurred_box_matches_gpu_within_tolerance() {
        // T005/T006: a turned, blurred box draws on both picture paths
        // within a few levels (the GPU quantizes motion values; blur
        // and the turn ride exact).
        use notation::model::Clip;
        let mut clip = red_box();
        if let Clip::Box(bc) = &mut clip {
            bc.x = 4.1;
            bc.turn.rotation_x = 30.0;
            bc.filter_blur = 1.0;
        }
        let prep = box_prep(vec![clip]);
        let cpu = draw_batch_parallel_with_workers(&prep, Backend::Cpu, &[0], 1).unwrap();
        let gpu = draw_batch_parallel_with_workers(&prep, Backend::Gpu, &[0], 1).unwrap();
        let (a, b) = (&cpu[0].1, &gpu[0].1);
        assert_eq!(a.len(), b.len());
        let max = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (*x as i16 - *y as i16).abs())
            .max()
            .unwrap_or(0);
        assert!(max <= 8, "cpu/gpu drift too far: {max}");
        // Identical motion inputs draw identical bytes on both paths.
        let mut clip = red_box();
        if let Clip::Box(bc) = &mut clip {
            bc.turn.rotation_y = -20.0;
            bc.turn.perspective = Some(300.0);
        }
        let prep = box_prep(vec![clip]);
        let cpu = draw_batch_parallel_with_workers(&prep, Backend::Cpu, &[3], 1).unwrap();
        let gpu = draw_batch_parallel_with_workers(&prep, Backend::Gpu, &[3], 1).unwrap();
        assert_eq!(cpu[0].1, gpu[0].1, "exact inputs draw identical bytes");
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
        blit_fit(
            &mut turned,
            32,
            32,
            &src,
            8,
            4,
            Fit::Fill,
            0.0,
            0.0,
            1.0,
            90.0,
            1.0,
        );
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
        assert!(
            frame.iter().all(|&v| v == 7),
            "empty faded text draws nothing"
        );
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
        assert!(
            r > 200 && g < 55 && b < 55,
            "turned pixel lands below, got ({r}, {g}, {b})"
        );
    }

    #[test]
    fn drawn_frames_follow_paint_order_only() {
        // T006 at the blitter: swapping document positions while fixed
        // written orders hold leaves the picture unchanged; equal
        // orders follow document order.
        use crate::frame::{describe, FrameDescription};
        use crate::motion_plan::plan_motions;
        use notation::model::{BoxClip, Composition, Fps, Rgb, TrackKind};
        fn comp(order_first: bool, tie: bool) -> Composition {
            let (red_order, blue_order) = if tie { (0, 0) } else { (1, 0) };
            let mut clips = vec![
                BoxClip {
                    asserts: Default::default(),
                    id: None,
                    track: TrackKind::Graphics,
                    label: None,
                    start_after: None,
                    hidden: false,
                    start: 0.0,
                    duration: 1.0,
                    x: 4.0,
                    y: 4.0,
                    width: 16.0,
                    height: 16.0,
                    fill: Rgb { r: 255, g: 0, b: 0 },
                    fill_paint: FillPaint::Solid,
                    radius: 0.0,
                    border_width: 0.0,
                    border_color: Rgb { r: 0, g: 0, b: 0 },
                    stroke_position: notation::model::StrokePosition::Inside,
                    box_shadow: None,
                    blend_mode: notation::model::BlendMode::Normal,
                    backdrop_blur: 0.0,
                    backdrop_brightness: 1.0,
                    overflow: notation::model::Overflow::Visible,
                    mask: None,
                    clip_path: None,
                    opacity: 1.0,
                    order: red_order,
                    line: 1,
                    column: 1,
                    turn: notation::model::Turn3D::flat(),
                    filter_blur: 0.0,
                    grade: notation::model::Grade::default(),
                    effects: Vec::new(),
                    vfx: Default::default(),
                },
                BoxClip {
                    asserts: Default::default(),
                    id: None,
                    track: TrackKind::Graphics,
                    label: None,
                    start_after: None,
                    hidden: false,
                    start: 0.0,
                    duration: 1.0,
                    x: 8.0,
                    y: 8.0,
                    width: 16.0,
                    height: 16.0,
                    fill: Rgb { r: 0, g: 0, b: 255 },
                    fill_paint: FillPaint::Solid,
                    radius: 0.0,
                    border_width: 0.0,
                    border_color: Rgb { r: 0, g: 0, b: 0 },
                    stroke_position: notation::model::StrokePosition::Inside,
                    box_shadow: None,
                    blend_mode: notation::model::BlendMode::Normal,
                    backdrop_blur: 0.0,
                    backdrop_brightness: 1.0,
                    overflow: notation::model::Overflow::Visible,
                    mask: None,
                    clip_path: None,
                    opacity: 1.0,
                    order: blue_order,
                    line: 1,
                    column: 1,
                    turn: notation::model::Turn3D::flat(),
                    filter_blur: 0.0,
                    grade: notation::model::Grade::default(),
                    effects: Vec::new(),
                    vfx: Default::default(),
                },
            ];
            if !order_first {
                clips.swap(0, 1);
            }
            Composition {
                line: 1,
                column: 1,
                id: None,
                canvas: None,
                name: "t".to_string(),
                width: 32,
                height: 32,
                duration: 1.0,
                fps: Fps::from_int(30),
                background: Rgb { r: 0, g: 0, b: 0 },
                version: 1,
                clips: clips.into_iter().map(notation::model::Clip::Box).collect(),
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
        fn draw(comp: &Composition) -> Vec<u8> {
            let motions = plan_motions(comp, 30);
            let desc: FrameDescription = describe(comp, &motions, 0);
            let mut raster = std::collections::HashMap::new();
            let mut frame = vec![0u8; 32 * 32 * 3];
            draw_description(
                &desc,
                &mut raster,
                &[],
                &DynLayouts::default(),
                &crate::captions::CaptionLayouts::new(),
                false,
                &[],
                &[],
                &[],
                &[],
                &crate::path_paint::PathCache::new(),
                std::path::Path::new("."),
                &mut frame,
            )
            .unwrap();
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

    // --- Sprint 11 grade (T001): preset and adjust pixels ---

    fn grade_box(
        fill: (u8, u8, u8),
        track: notation::model::TrackKind,
        grade: notation::model::Grade,
    ) -> notation::model::BoxClip {
        use notation::model as m;
        m::BoxClip {
            asserts: Default::default(),
            id: None,
            track,
            label: None,
            start_after: None,
            hidden: false,
            start: 0.0,
            duration: 1.0,
            x: 8.0,
            y: 8.0,
            width: 16.0,
            height: 16.0,
            fill: Rgb {
                r: fill.0,
                g: fill.1,
                b: fill.2,
            },
            fill_paint: m::FillPaint::Solid,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: m::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: m::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: m::Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            order: 0,
            line: 1,
            column: 1,
            turn: m::Turn3D::flat(),
            filter_blur: 0.0,
            grade,
            effects: Vec::new(),
            vfx: Default::default(),
        }
    }

    fn grade_comp(
        bg: (u8, u8, u8),
        boxes: Vec<notation::model::BoxClip>,
        comp_grade: notation::model::Grade,
        lanes: Vec<notation::model::LaneGrade>,
    ) -> Composition {
        Composition {
            line: 1,
            column: 1,
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 32,
            height: 32,
            duration: 1.0,
            fps: notation::model::Fps::from_int(30),
            background: Rgb {
                r: bg.0,
                g: bg.1,
                b: bg.2,
            },
            version: 1,
            clips: boxes.into_iter().map(notation::model::Clip::Box).collect(),
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
            grade: comp_grade,
            lane_grades: lanes,
            transitions: Vec::new(),
            motion_blur: None,
        }
    }

    fn draw32(comp: &Composition) -> Vec<u8> {
        draw32_in(comp, std::path::Path::new("."))
    }

    fn draw32_in(comp: &Composition, folder: &std::path::Path) -> Vec<u8> {
        use crate::frame::describe;
        let motions = plan_motions(comp, 30);
        let desc = describe(comp, &motions, 0);
        let mut raster = std::collections::HashMap::new();
        let mut frame = vec![0u8; 32 * 32 * 3];
        draw_description(
            &desc,
            &mut raster,
            &[],
            &DynLayouts::default(),
            &crate::captions::CaptionLayouts::new(),
            false,
            &[],
            &[],
            &[],
            &[],
            &crate::path_paint::PathCache::new(),
            folder,
            &mut frame,
        )
        .unwrap();
        frame
    }

    fn px(frame: &[u8], x: usize, y: usize) -> (u8, u8, u8) {
        let o = (y * 32 + x) * 3;
        (frame[o], frame[o + 1], frame[o + 2])
    }

    fn empty_grade() -> notation::model::Grade {
        notation::model::Grade::default()
    }

    fn preset_grade(preset: notation::model::GradePreset) -> notation::model::Grade {
        notation::model::Grade {
            preset: Some(preset),
            ..empty_grade()
        }
    }

    #[test]
    fn grade_presets_render_their_looks() {
        use notation::model::GradePreset;
        // A colored box so every preset has room to move.
        let plain = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                notation::model::TrackKind::Graphics,
                empty_grade(),
            )],
            empty_grade(),
            Vec::new(),
        ));
        assert_eq!(px(&plain, 16, 16), (200, 100, 50));
        // `none` renders exactly the authored pixels.
        let none = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                notation::model::TrackKind::Graphics,
                preset_grade(GradePreset::None),
            )],
            empty_grade(),
            Vec::new(),
        ));
        assert_eq!(none, plain);
        // Every other preset visibly moves the pixels.
        for preset in [
            GradePreset::Filmic,
            GradePreset::Noir,
            GradePreset::TealOrange,
            GradePreset::Pastel,
            GradePreset::Bleach,
        ] {
            let graded = draw32(&grade_comp(
                (0, 0, 0),
                vec![grade_box(
                    (200, 100, 50),
                    notation::model::TrackKind::Graphics,
                    preset_grade(preset),
                )],
                empty_grade(),
                Vec::new(),
            ));
            assert_ne!(graded, plain, "{preset:?} must move pixels");
        }
        // Noir is gray.
        let noir = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                notation::model::TrackKind::Graphics,
                preset_grade(GradePreset::Noir),
            )],
            empty_grade(),
            Vec::new(),
        ));
        let (r, g, b) = px(&noir, 16, 16);
        assert_eq!((r, g, b), (r, r, r), "noir renders gray");
        // Teal-orange cools these shadows: red down, blue up.
        let to = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                notation::model::TrackKind::Graphics,
                preset_grade(GradePreset::TealOrange),
            )],
            empty_grade(),
            Vec::new(),
        ));
        let (r, _, b) = px(&to, 16, 16);
        assert!(r < 200 && b > 50, "teal-orange cools shadows: {r},{b}");
        // Pastel lifts a dark box.
        let pastel = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (20, 20, 20),
                notation::model::TrackKind::Graphics,
                preset_grade(GradePreset::Pastel),
            )],
            empty_grade(),
            Vec::new(),
        ));
        let (r, _, _) = px(&pastel, 16, 16);
        assert!(r > 20, "pastel lifts shadows: {r}");
        // Bleach brightens the mean.
        let bleach = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                notation::model::TrackKind::Graphics,
                preset_grade(GradePreset::Bleach),
            )],
            empty_grade(),
            Vec::new(),
        ));
        let (r, g, b) = px(&bleach, 16, 16);
        assert!(
            (r as u32 + g as u32 + b as u32) > 350,
            "bleach brightens: {r},{g},{b}"
        );
    }

    #[test]
    fn grade_adjusts_shift_pixels_in_their_direction() {
        use notation::model::TrackKind;
        let graded = |grade: notation::model::Grade| {
            draw32(&grade_comp(
                (0, 0, 0),
                vec![grade_box((128, 128, 128), TrackKind::Graphics, grade)],
                empty_grade(),
                Vec::new(),
            ))
        };
        let one = |key: &str, v: f64| {
            let mut g = empty_grade();
            match key {
                "exposure" => g.exposure = Some(v),
                "contrast" => g.contrast = Some(v),
                "saturation" => g.saturation = Some(v),
                "temperature" => g.temperature = Some(v),
                "tint" => g.tint = Some(v),
                _ => unreachable!(),
            }
            px(&graded(g), 16, 16)
        };
        assert_eq!(one("exposure", 1.0), (255, 255, 255));
        assert_eq!(one("exposure", -1.0), (64, 64, 64));
        // Contrast spreads brights up and darks down around middle gray.
        let (r, _, _) = one("contrast", 0.5);
        assert_eq!((r, r, r), (128, 128, 128), "mid gray holds under contrast");
        // Saturation at -1 grays a colored box.
        let sat = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box((200, 100, 50), TrackKind::Graphics, {
                let mut g = empty_grade();
                g.saturation = Some(-1.0);
                g
            })],
            empty_grade(),
            Vec::new(),
        ));
        let (r, g, b) = px(&sat, 16, 16);
        assert!((r as i16 - g as i16).abs() <= 1 && (g as i16 - b as i16).abs() <= 1);
        // Temperature warms, tint magentas.
        let (r, g, b) = one("temperature", 1.0);
        assert!(r > 128 && b < 128 && g == 128, "{r},{g},{b}");
        let (r, g, b) = one("tint", 1.0);
        assert!(r > 128 && b > 128 && g < 128, "{r},{g},{b}");
    }

    #[test]
    fn grade_levels_compose_with_clip_winning() {
        use notation::model::{LaneGrade, TrackKind};
        // Composition +1 EV with a clip -1 EV resolves to the clip's
        // -1 EV (override), not the sum and not the composition value.
        let comp = grade_comp(
            (0, 0, 0),
            vec![grade_box((128, 128, 128), TrackKind::Video, {
                let mut g = empty_grade();
                g.exposure = Some(-1.0);
                g
            })],
            {
                let mut g = empty_grade();
                g.exposure = Some(1.0);
                g
            },
            Vec::new(),
        );
        assert_eq!(px(&draw32(&comp), 16, 16), (64, 64, 64));
        // A lane grade applies to every clip on that track and no other:
        // the video-track box doubles, the graphics-track box holds.
        let mut video_box = grade_box((64, 64, 64), TrackKind::Video, empty_grade());
        video_box.x = 2.0;
        video_box.width = 12.0;
        video_box.order = 0;
        let mut gfx_box = grade_box((64, 64, 64), TrackKind::Graphics, empty_grade());
        gfx_box.x = 18.0;
        gfx_box.width = 12.0;
        gfx_box.order = 1;
        let comp = grade_comp(
            (0, 0, 0),
            vec![video_box, gfx_box],
            empty_grade(),
            vec![LaneGrade {
                track: TrackKind::Video,
                grade: {
                    let mut g = empty_grade();
                    g.exposure = Some(1.0);
                    g
                },
            }],
        );
        let frame = draw32(&comp);
        assert_eq!(px(&frame, 8, 16), (128, 128, 128));
        assert_eq!(px(&frame, 24, 16), (64, 64, 64));
        // A missing piece at the clip level inherits the composition
        // value instead of zeroing it: composition saturation -1 with
        // a clip exposure-only tweak still renders gray.
        let comp = grade_comp(
            (0, 0, 0),
            vec![grade_box((200, 100, 50), TrackKind::Graphics, {
                let mut g = empty_grade();
                g.exposure = Some(0.5);
                g
            })],
            {
                let mut g = empty_grade();
                g.saturation = Some(-1.0);
                g
            },
            Vec::new(),
        );
        let (r, g, b) = px(&draw32(&comp), 16, 16);
        assert!((r as i16 - g as i16).abs() <= 2 && (g as i16 - b as i16).abs() <= 2);
    }

    #[test]
    fn grade_background_carries_the_composition_look() {
        // A gray background warms under a composition temperature.
        let comp = grade_comp(
            (128, 128, 128),
            Vec::new(),
            {
                let mut g = empty_grade();
                g.temperature = Some(1.0);
                g
            },
            Vec::new(),
        );
        let (r, g, b) = px(&draw32(&comp), 2, 2);
        assert!(r > 128 && b < 128 && g == 128, "{r},{g},{b}");
        // An ungraded background stays bit-exact.
        let plain = grade_comp((10, 20, 30), Vec::new(), empty_grade(), Vec::new());
        assert_eq!(px(&draw32(&plain), 2, 2), (10, 20, 30));
    }

    #[test]
    fn graded_renders_are_byte_identical() {
        // A graded frame re-renders byte-identically (no randomness in
        // the look pipeline).
        let comp = grade_comp(
            (10, 10, 10),
            vec![grade_box(
                (200, 100, 50),
                notation::model::TrackKind::Graphics,
                {
                    let mut g = preset_grade(notation::model::GradePreset::Filmic);
                    g.exposure = Some(0.5);
                    g
                },
            )],
            {
                let mut g = empty_grade();
                g.temperature = Some(-0.5);
                g
            },
            Vec::new(),
        );
        assert_eq!(draw32(&comp), draw32(&comp));
    }

    // --- Sprint 11 grade detail stages (T002) ---

    #[test]
    fn grade_wheel_bands_render_their_zones() {
        use notation::model::TrackKind;
        let graded = |fill: (u8, u8, u8), band: &str, v: f64| {
            let mut g = empty_grade();
            match band {
                "shadows" => g.wheel.shadows = Some(v),
                "midtones" => g.wheel.midtones = Some(v),
                "highlights" => g.wheel.highlights = Some(v),
                _ => unreachable!(),
            }
            draw32(&grade_comp(
                (0, 0, 0),
                vec![grade_box(fill, TrackKind::Graphics, g)],
                empty_grade(),
                Vec::new(),
            ))
        };
        // Shadows lift a dark box far, highlights leave it alone.
        let (r, _, _) = px(&graded((40, 40, 40), "shadows", 0.6), 16, 16);
        assert!((100..=125).contains(&r), "shadows lift dark: {r}");
        let (r, _, _) = px(&graded((40, 40, 40), "highlights", 0.6), 16, 16);
        assert!((39..=41).contains(&r), "highlights hold dark: {r}");
        // Highlights clip a bright box to white, shadows leave it alone.
        let (r, _, _) = px(&graded((220, 220, 220), "highlights", 0.6), 16, 16);
        assert_eq!(r, 255, "highlights clip bright: {r}");
        let (r, _, _) = px(&graded((220, 220, 220), "shadows", 0.6), 16, 16);
        assert!((219..=221).contains(&r), "shadows hold bright: {r}");
        // Midtones lift middle gray.
        let (r, _, _) = px(&graded((128, 128, 128), "midtones", 0.4), 16, 16);
        assert!((226..=234).contains(&r), "midtones lift mid: {r}");
    }

    #[test]
    fn grade_curve_renders_its_remap() {
        // Red dips through (0.5, 0) while green and blue hold.
        let mut g = empty_grade();
        g.curve.red = Some(vec![(0.0, 0.0), (0.5, 0.0), (1.0, 1.0)]);
        let frame = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                notation::model::TrackKind::Graphics,
                g,
            )],
            empty_grade(),
            Vec::new(),
        ));
        let (r, gg, b) = px(&frame, 16, 16);
        assert!((140..=150).contains(&r), "red dips: {r}");
        assert!((99..=101).contains(&gg), "green holds: {gg}");
        assert!((49..=51).contains(&b), "blue holds: {b}");
    }

    #[test]
    fn grade_hsl_renders_only_its_selection() {
        use notation::model::TrackKind;
        // One red box and one green box under the same red-window look.
        let mut red_box = grade_box((200, 60, 60), TrackKind::Graphics, {
            let mut g = empty_grade();
            g.hsl.hue_min = Some(330.0);
            g.hsl.hue_max = Some(30.0);
            g.hsl.saturation = Some(0.2);
            g.hsl.lightness = Some(0.3);
            g
        });
        red_box.x = 2.0;
        red_box.width = 12.0;
        red_box.order = 0;
        let mut green_box = grade_box((60, 200, 60), TrackKind::Graphics, {
            let mut g = empty_grade();
            g.hsl.hue_min = Some(330.0);
            g.hsl.hue_max = Some(30.0);
            g.hsl.saturation = Some(0.2);
            g.hsl.lightness = Some(0.3);
            g
        });
        green_box.x = 18.0;
        green_box.width = 12.0;
        green_box.order = 1;
        let frame = draw32(&grade_comp(
            (0, 0, 0),
            vec![red_box, green_box],
            empty_grade(),
            Vec::new(),
        ));
        let (r, gg, b) = px(&frame, 8, 16);
        assert!(
            r > 230 && gg > 130 && b > 130,
            "red lifts pink: {r},{gg},{b}"
        );
        let (r, gg, b) = px(&frame, 24, 16);
        assert!((59..=61).contains(&r), "green red holds: {r}");
        assert!((199..=201).contains(&gg), "green holds: {gg}");
        assert!((59..=61).contains(&b), "green blue holds: {b}");
    }

    #[test]
    fn grade_vignette_darkens_frame_corners_on_the_background() {
        // A composition vignette over a flat background: corners fall,
        // the center holds exactly.
        let mut comp_grade = empty_grade();
        comp_grade.vignette.amount = Some(1.0);
        comp_grade.vignette.softness = Some(0.5);
        let frame = draw32(&grade_comp(
            (150, 150, 150),
            Vec::new(),
            comp_grade,
            Vec::new(),
        ));
        let (r, _, _) = px(&frame, 0, 0);
        assert!(r < 20, "corner falls: {r}");
        assert_eq!(px(&frame, 16, 16), (150, 150, 150));
        // A position-independent composition grade still fills flat.
        let mut flat_grade = empty_grade();
        flat_grade.temperature = Some(1.0);
        let frame = draw32(&grade_comp(
            (128, 128, 128),
            Vec::new(),
            flat_grade,
            Vec::new(),
        ));
        assert_eq!(px(&frame, 0, 0), px(&frame, 16, 16));
    }

    #[test]
    fn grade_grain_renders_and_rerenders_identically() {
        use notation::model::TrackKind;
        let grained = |seed: i64| {
            let mut g = empty_grade();
            g.grain.amount = Some(1.0);
            g.grain.size = Some(1);
            g.grain.seed = Some(seed);
            draw32(&grade_comp(
                (0, 0, 0),
                vec![grade_box((128, 128, 128), TrackKind::Graphics, g)],
                empty_grade(),
                Vec::new(),
            ))
        };
        let plain = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (128, 128, 128),
                TrackKind::Graphics,
                empty_grade(),
            )],
            empty_grade(),
            Vec::new(),
        ));
        let a = grained(7);
        // Grain shows somewhere inside the box.
        let shows = (8..24).any(|y| {
            (8..24).any(|x| {
                let (r, _, _) = px(&a, x, y);
                let (pr, _, _) = px(&plain, x, y);
                (r as i16 - pr as i16).abs() > 1
            })
        });
        assert!(shows, "grain must visibly move box pixels");
        // The same seed re-renders byte-identically; another seed moves.
        assert_eq!(a, grained(7));
        assert_ne!(a, grained(8));
    }

    // --- Sprint 11 VFX chain (T005): stage pixels, purity, mattes ---

    fn vfx_box(fill: (u8, u8, u8), vfx: notation::model::VfxChain) -> notation::model::BoxClip {
        let mut b = grade_box(fill, notation::model::TrackKind::Graphics, empty_grade());
        b.vfx = vfx;
        b
    }

    fn vfx_noise(amount: f64, seed: i64) -> notation::model::VfxChain {
        notation::model::VfxChain {
            noise: Some(notation::model::VfxNoise {
                amount,
                seed: Some(seed),
            }),
            ..notation::model::VfxChain::empty()
        }
    }

    #[test]
    fn vfx_noise_on_box_moves_pixels_and_rerenders_identically() {
        use notation::model::TrackKind;
        let noised = |seed: i64| {
            draw32(&grade_comp(
                (0, 0, 0),
                vec![vfx_box((200, 100, 50), vfx_noise(1.0, seed))],
                empty_grade(),
                Vec::new(),
            ))
        };
        let plain = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                TrackKind::Graphics,
                empty_grade(),
            )],
            empty_grade(),
            Vec::new(),
        ));
        let a = noised(7);
        // Noise shows somewhere inside the box.
        let shows = (8..24).any(|y| {
            (8..24).any(|x| {
                let (r, _, _) = px(&a, x, y);
                let (pr, _, _) = px(&plain, x, y);
                (r as i16 - pr as i16).abs() > 1
            })
        });
        assert!(shows, "VFX noise must visibly move box pixels");
        // The same seed re-renders byte-identically; another seed moves.
        assert_eq!(a, noised(7));
        assert_ne!(a, noised(8));
    }

    #[test]
    fn vfx_warp_moves_box_pixels() {
        use notation::model::{TrackKind, VfxChain, VfxWarp};
        let warped = draw32(&grade_comp(
            (0, 0, 0),
            vec![vfx_box(
                (200, 100, 50),
                VfxChain {
                    warp: Some(VfxWarp {
                        amount: 1.0,
                        grid: Some(4),
                        strength: Some(6.0),
                    }),
                    ..VfxChain::empty()
                },
            )],
            empty_grade(),
            Vec::new(),
        ));
        let plain = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                TrackKind::Graphics,
                empty_grade(),
            )],
            empty_grade(),
            Vec::new(),
        ));
        assert_ne!(warped, plain, "VFX warp must move box pixels");
    }

    #[test]
    fn vfx_inert_chain_matches_chain_written_without_stage() {
        // Disabling a stage renders pixel-identical to the chain
        // written without it, through the full describe-and-draw path.
        use notation::model::{TrackKind, VfxChain, VfxNoise, VfxWarp};
        let full = VfxChain {
            noise: Some(VfxNoise {
                amount: 1.0,
                seed: Some(7),
            }),
            warp: Some(VfxWarp {
                amount: 1.0,
                grid: Some(4),
                strength: Some(3.0),
            }),
            ..VfxChain::empty()
        };
        let draw = |vfx: VfxChain| {
            draw32(&grade_comp(
                (0, 0, 0),
                vec![vfx_box((200, 100, 50), vfx)],
                empty_grade(),
                Vec::new(),
            ))
        };
        let without_noise = VfxChain {
            noise: None,
            ..full.clone()
        };
        let zeroed_noise = VfxChain {
            noise: Some(VfxNoise {
                amount: 0.0,
                seed: Some(7),
            }),
            ..full.clone()
        };
        assert_eq!(draw(zeroed_noise), draw(without_noise));
        let without_warp = VfxChain {
            warp: None,
            ..full.clone()
        };
        let zeroed_warp = VfxChain {
            warp: Some(VfxWarp {
                amount: 1.0,
                grid: Some(4),
                strength: Some(0.0),
            }),
            ..full.clone()
        };
        assert_eq!(draw(zeroed_warp), draw(without_warp));
        // The empty chain matches no chain at all.
        let plain = draw32(&grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                TrackKind::Graphics,
                empty_grade(),
            )],
            empty_grade(),
            Vec::new(),
        ));
        assert_eq!(draw(VfxChain::empty()), plain);
    }

    /// One 32x32 half-white, half-black matte as decoded bytes.
    fn split_matte(folder: &std::path::Path, name: &str) -> DecodedImage {
        let (w, h) = (32usize, 32usize);
        let mut rgba = vec![0u8; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let o = (j * w + i) * 4;
                let v = if i < w / 2 { 255 } else { 0 };
                rgba[o] = v;
                rgba[o + 1] = v;
                rgba[o + 2] = v;
                rgba[o + 3] = 255;
            }
        }
        // The resolve step stats the source: a marker file proves it
        // exists, while the bytes ride in on the shared image list.
        std::fs::write(folder.join(name), b"matte").expect("write matte marker");
        DecodedImage {
            path: folder.join(name),
            width: w as u32,
            height: h as u32,
            rgba,
            scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    fn draw32_media(
        comp: &Composition,
        folder: &std::path::Path,
        images: &[DecodedImage],
    ) -> Vec<u8> {
        use crate::frame::describe;
        let motions = plan_motions(comp, 30);
        let desc = describe(comp, &motions, 0);
        let mut raster = std::collections::HashMap::new();
        let mut frame = vec![0u8; 32 * 32 * 3];
        draw_description(
            &desc,
            &mut raster,
            &[],
            &DynLayouts::default(),
            &crate::captions::CaptionLayouts::new(),
            false,
            &[],
            &[],
            images,
            &[],
            &crate::path_paint::PathCache::new(),
            folder,
            &mut frame,
        )
        .unwrap();
        frame
    }

    fn vfx_tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-render-vfx-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("vfx tmp dir");
        dir
    }

    #[test]
    fn vfx_luma_matte_cuts_to_the_matte() {
        use notation::model::{VfxChain, VfxLumaMatte};
        let dir = vfx_tmp("matte");
        let matte = split_matte(&dir, "matte.png");
        let frame = draw32_media(
            &grade_comp(
                (0, 0, 0),
                vec![vfx_box(
                    (200, 100, 50),
                    VfxChain {
                        luma_matte: Some(VfxLumaMatte {
                            amount: 1.0,
                            src: "matte.png".to_string(),
                        }),
                        ..VfxChain::empty()
                    },
                )],
                empty_grade(),
                Vec::new(),
            ),
            &dir,
            std::slice::from_ref(&matte),
        );
        // The box spans x 8..24; the matte is white left of x=16.
        let (r, g, b) = px(&frame, 10, 16);
        assert!(
            (r as i16 - 200).abs() <= 2
                && (g as i16 - 100).abs() <= 2
                && (b as i16 - 50).abs() <= 2,
            "white matte keeps the box color, got {r},{g},{b}"
        );
        let (r, g, b) = px(&frame, 21, 16);
        assert!(
            r <= 2 && g <= 2 && b <= 2,
            "black matte cuts to the background, got {r},{g},{b}"
        );
        // Re-rendering cuts identically.
        let again = draw32_media(
            &grade_comp(
                (0, 0, 0),
                vec![vfx_box(
                    (200, 100, 50),
                    VfxChain {
                        luma_matte: Some(VfxLumaMatte {
                            amount: 1.0,
                            src: "matte.png".to_string(),
                        }),
                        ..VfxChain::empty()
                    },
                )],
                empty_grade(),
                Vec::new(),
            ),
            &dir,
            std::slice::from_ref(&matte),
        );
        assert_eq!(frame, again);
    }

    #[test]
    fn vfx_resolve_refuses_vanished_and_undecoded_sources() {
        use notation::model::{VfxChain, VfxLumaMatte};
        let dir = vfx_tmp("resolve");
        let chain = VfxChain {
            luma_matte: Some(VfxLumaMatte {
                amount: 1.0,
                src: "gone.png".to_string(),
            }),
            ..VfxChain::empty()
        };
        // No file on disk: the vanish refusal names the path.
        let err = resolve_vfx_maps(&[], &dir, &chain).expect_err("vanished matte must refuse");
        assert!(err.contains("gone.png"), "{err}");
        assert!(err.contains("no MP4 was written"), "{err}");
        // A marker file with no decoded entry: the decode refusal
        // names the path instead of painting a guess.
        std::fs::write(dir.join("gone.png"), b"matte").expect("write marker");
        let err = resolve_vfx_maps(&[], &dir, &chain).expect_err("undecoded matte must refuse");
        assert!(err.contains("gone.png"), "{err}");
        // An inert stage resolves to none without touching disk.
        let inert = VfxChain {
            luma_matte: Some(VfxLumaMatte {
                amount: 0.0,
                src: "never.png".to_string(),
            }),
            ..VfxChain::empty()
        };
        let maps = resolve_vfx_maps(&[], &dir, &inert).expect("inert stage resolves");
        assert!(maps.luma_matte.is_none());
        assert!(maps.displacement.is_none());
    }

    // --- Sprint 11 LUT import (T003): `.cube` pixels, blends, vanish ---

    /// One size-2 `.cube` swapping red and blue, written to `dir`.
    fn write_swap_cube(dir: &std::path::Path) -> std::path::PathBuf {
        let mut s = String::from("TITLE \"swap r and b\"\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    s.push_str(&format!("{b} {g} {r}\n"));
                }
            }
        }
        let path = dir.join("swap.cube");
        std::fs::write(&path, s).expect("write swap cube");
        path
    }

    fn lut_tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-render-lut-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("lut tmp dir");
        dir
    }

    fn lut_grade(src: &str, amount: f64) -> notation::model::Grade {
        let mut g = empty_grade();
        g.lut.src = Some(src.to_string());
        g.lut.amount = Some(amount);
        g
    }

    #[test]
    fn grade_lut_shifts_pixels_at_full_amount() {
        use notation::model::TrackKind;
        let dir = lut_tmp("full");
        write_swap_cube(&dir);
        let frame = draw32_in(
            &grade_comp(
                (0, 0, 0),
                vec![grade_box(
                    (200, 100, 50),
                    TrackKind::Graphics,
                    lut_grade("swap.cube", 1.0),
                )],
                empty_grade(),
                Vec::new(),
            ),
            &dir,
        );
        // Red and blue trade places through the grid (f32 layer path
        // rounds within a couple of LSB).
        let (r, g, b) = px(&frame, 16, 16);
        assert!(
            (r as i16 - 50).abs() <= 2
                && (g as i16 - 100).abs() <= 2
                && (b as i16 - 200).abs() <= 2,
            "swapped box pixel, got {r},{g},{b}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grade_lut_on_the_composition_grades_the_background() {
        let dir = lut_tmp("bg");
        write_swap_cube(&dir);
        let frame = draw32_in(
            &grade_comp(
                (10, 20, 30),
                Vec::new(),
                lut_grade("swap.cube", 1.0),
                Vec::new(),
            ),
            &dir,
        );
        // The flat background fill carries the loaded grid too.
        let (r, g, b) = px(&frame, 4, 4);
        assert!(
            (r as i16 - 30).abs() <= 2 && (g as i16 - 20).abs() <= 2 && (b as i16 - 10).abs() <= 2,
            "swapped background pixel, got {r},{g},{b}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grade_lut_zero_amount_matches_no_lut_exactly() {
        use notation::model::TrackKind;
        let dir = lut_tmp("zero");
        write_swap_cube(&dir);
        let boxed = |grade: notation::model::Grade| {
            grade_comp(
                (0, 0, 0),
                vec![grade_box((200, 100, 50), TrackKind::Graphics, grade)],
                empty_grade(),
                Vec::new(),
            )
        };
        let plain = draw32_in(&boxed(empty_grade()), &dir);
        let zero = draw32_in(&boxed(lut_grade("swap.cube", 0.0)), &dir);
        assert_eq!(zero, plain, "amount 0 is pixel-identical to no LUT");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grade_lut_half_amount_blends_halfway() {
        use notation::model::TrackKind;
        let dir = lut_tmp("half");
        write_swap_cube(&dir);
        let boxed = |grade: notation::model::Grade| {
            draw32_in(
                &grade_comp(
                    (0, 0, 0),
                    vec![grade_box((200, 100, 50), TrackKind::Graphics, grade)],
                    empty_grade(),
                    Vec::new(),
                ),
                &dir,
            )
        };
        let none = boxed(empty_grade());
        let full = boxed(lut_grade("swap.cube", 1.0));
        let half = boxed(lut_grade("swap.cube", 0.5));
        // The byte render rounds, so halfway holds within one LSB.
        for o in (0..none.len()).step_by(3) {
            for c in 0..3 {
                let (a, b, h) = (none[o + c] as i16, full[o + c] as i16, half[o + c] as i16);
                assert!(
                    (h - (a + b) / 2).abs() <= 1,
                    "half blend off at byte {o}+{c}: none={a} full={b} half={h}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Sprint 11 motion blur (T007): shutter integration ---

    fn blur_tween() -> notation::model::TweenDecl {
        // Box 0 slides right 112 canvas pixels over the 1s composition,
        // linear: frame 15 sits at exactly x=56.
        notation::model::TweenDecl {
            target: 0,
            property: "x".to_string(),
            kind: notation::model::TweenKind::FromTo,
            from: Some(0.0),
            to: Some(112.0),
            from_color: None,
            to_color: None,
            start: 0.0,
            duration: 1.0,
            ease: notation::model::EaseDecl::Linear,
            stagger: 0.0,
            stagger_from: notation::model::StaggerOrigin::Start,
            stagger_grid: None,
            repeat: 1,
            yoyo: false,
            repeat_delay: 0.0,
            path: None,
            path_to: None,
            orient: false,
            expr: None,
            line: 1,
            column: 1,
        }
    }

    fn blur_comp(
        tweens: Vec<notation::model::TweenDecl>,
        blur: Option<notation::model::MotionBlur>,
    ) -> Composition {
        let mut boxed = grade_box((255, 255, 255), TrackKind::Video, empty_grade());
        boxed.x = 0.0;
        boxed.y = 24.0;
        boxed.width = 16.0;
        boxed.height = 16.0;
        let mut comp = grade_comp((0, 0, 0), vec![boxed], empty_grade(), Vec::new());
        comp.width = 128;
        comp.height = 64;
        comp.tweens = tweens;
        comp.motion_blur = blur;
        comp
    }

    fn motion_blur(
        shutter: f64,
        samples: u32,
        tier: notation::model::MotionBlurTier,
    ) -> notation::model::MotionBlur {
        notation::model::MotionBlur {
            shutter,
            samples,
            tier,
            line: 1,
            column: 1,
        }
    }

    fn draw_blur_frame(comp: &Composition, frame: u64, backend: Backend) -> Vec<u8> {
        use crate::frame::NestView;
        let motions = plan_motions(comp, 30);
        let nest = NestView::single(comp, &motions);
        let paths = crate::path_paint::PathCache::new();
        let dyn_layouts = DynLayouts::default();
        let view = DrawView {
            videos: &[],
            ranges: &[],
            images: &[],
            lotties: &[],
            paths: &paths,
            layouts: &[],
            dyn_layouts: &dyn_layouts,
            captions: &crate::captions::CaptionLayouts::new(),
            bitmap_fallback: false,
            nest,
            time_offset: timebase::Time::ZERO,
            folder: std::path::Path::new("."),
        };
        let mut raster = RasterCache::new();
        draw_one_frame_view(&view, &mut raster, backend, frame).expect("blurred frame draws")
    }

    fn red_row(frame: &[u8], width: u32, y: u32) -> Vec<u8> {
        (0..width)
            .map(|x| frame[((y * width + x) as usize) * 3])
            .collect()
    }

    fn in_between_count(row: &[u8]) -> usize {
        row.iter().filter(|&&v| v > 0 && v < 255).count()
    }

    #[test]
    fn moving_object_blurs_along_its_motion_at_180_degrees() {
        use notation::model::MotionBlurTier;
        let sharp = blur_comp(vec![blur_tween()], None);
        let blurred = blur_comp(
            vec![blur_tween()],
            Some(motion_blur(180.0, 16, MotionBlurTier::High)),
        );
        // Frame 15: the box sits at exactly x=56, moving right.
        let a = draw_blur_frame(&sharp, 15, Backend::Cpu);
        let b = draw_blur_frame(&blurred, 15, Backend::Cpu);
        assert_ne!(a, b, "shutter blur changes the moving frame");
        let (ra, rb) = (red_row(&a, 128, 32), red_row(&b, 128, 32));
        // The sharp box covers [56, 72): crisp edges, no halo.
        assert_eq!(ra[55], 0, "sharp frame leaves pixel 55 dark");
        assert_eq!(ra[72], 0, "sharp frame leaves pixel 72 dark");
        assert_eq!(in_between_count(&ra), 0, "sharp edges are crisp");
        // The 180-degree window trails behind the motion: pixel 55
        // (left of the box) lights up while pixel 72 (ahead of the
        // box) stays dark — the smear follows the motion direction.
        assert!(rb[55] > 0, "blur trails behind the motion, got {}", rb[55]);
        assert_eq!(rb[72], 0, "no smear ahead of the motion");
        assert!(
            in_between_count(&rb) >= 2,
            "blur softens the edges, got {:?}",
            &rb[52..76]
        );
    }

    #[test]
    fn wider_shutter_blurs_more_than_narrower() {
        use notation::model::MotionBlurTier;
        let sharp = blur_comp(vec![blur_tween()], None);
        let narrow = blur_comp(
            vec![blur_tween()],
            Some(motion_blur(90.0, 16, MotionBlurTier::High)),
        );
        let wide = blur_comp(
            vec![blur_tween()],
            Some(motion_blur(360.0, 16, MotionBlurTier::High)),
        );
        let a = draw_blur_frame(&sharp, 15, Backend::Cpu);
        let n = draw_blur_frame(&narrow, 15, Backend::Cpu);
        let w = draw_blur_frame(&wide, 15, Backend::Cpu);
        assert_ne!(n, w, "shutter angle changes the smear");
        let (ca, cn, cw) = (
            in_between_count(&red_row(&a, 128, 32)),
            in_between_count(&red_row(&n, 128, 32)),
            in_between_count(&red_row(&w, 128, 32)),
        );
        assert!(cn > ca, "a 90-degree shutter softens edges ({cn} vs {ca})");
        assert!(cw > cn, "a 360-degree shutter softens more ({cw} vs {cn})");
    }

    #[test]
    fn still_content_under_auto_matches_blur_off() {
        use notation::model::MotionBlurTier;
        // No tweens: nothing moves, so `auto` must draw exactly once.
        let sharp = blur_comp(Vec::new(), None);
        let auto = blur_comp(
            Vec::new(),
            Some(motion_blur(180.0, 16, MotionBlurTier::Auto)),
        );
        let high = blur_comp(
            Vec::new(),
            Some(motion_blur(180.0, 16, MotionBlurTier::High)),
        );
        let a = draw_blur_frame(&sharp, 15, Backend::Cpu);
        assert_eq!(
            draw_blur_frame(&auto, 15, Backend::Cpu),
            a,
            "still auto renders pixel-identical to blur off"
        );
        assert_eq!(
            draw_blur_frame(&high, 15, Backend::Cpu),
            a,
            "averaging identical still pictures is exact"
        );
    }

    #[test]
    fn blurred_rerenders_are_byte_identical() {
        use notation::model::MotionBlurTier;
        for tier in [MotionBlurTier::Auto, MotionBlurTier::High] {
            let comp = blur_comp(vec![blur_tween()], Some(motion_blur(180.0, 8, tier)));
            let once = draw_blur_frame(&comp, 15, Backend::Cpu);
            let twice = draw_blur_frame(&comp, 15, Backend::Cpu);
            assert_eq!(
                once,
                twice,
                "tier {:?} re-renders byte-identically",
                tier.name()
            );
        }
    }

    #[test]
    fn cpu_and_gpu_agree_on_blurred_frames() {
        use notation::model::MotionBlurTier;
        let comp = blur_comp(
            vec![blur_tween()],
            Some(motion_blur(180.0, 8, MotionBlurTier::High)),
        );
        let cpu = draw_blur_frame(&comp, 15, Backend::Cpu);
        let gpu = draw_blur_frame(&comp, 15, Backend::Gpu);
        let cmp = crate::gpu::compare_frames(&cpu, &gpu).expect("frames compare");
        assert!(cmp.pass(), "{}", cmp.report("motion-blur", 15));
    }

    #[test]
    fn lut_frame_cache_resolves_and_refuses_a_vanish() {
        let dir = lut_tmp("vanish");
        write_swap_cube(&dir);
        let mut luts = LutFrameCache::new();
        let mut base = notation::model::ResolvedGrade::identity();
        base.lut.src = Some("swap.cube".to_string());
        base.lut.amount = 1.0;
        let resolved = luts.grade(&dir, &base).expect("first resolve loads");
        assert_eq!(resolved.lut.grid.as_ref().map(|t| t.size), Some(2));
        // A second frame reuses the file while it stands.
        let _ = luts.grade(&dir, &base).expect("cached resolve loads");
        // The source vanishes mid-render: the next frame refuses
        // naming the path, instead of painting the stale grid.
        std::fs::remove_file(dir.join("swap.cube")).expect("delete cube");
        let err = luts.grade(&dir, &base).expect_err("vanish must refuse");
        assert!(err.contains("swap.cube"), "{err}");
        assert!(err.contains("vanished"), "{err}");
        assert!(err.contains("no MP4 was written"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grade_lut_vanishing_between_describe_and_draw_refuses() {
        use crate::frame::describe;
        use notation::model::TrackKind;
        let dir = lut_tmp("midrender");
        write_swap_cube(&dir);
        let comp = grade_comp(
            (0, 0, 0),
            vec![grade_box(
                (200, 100, 50),
                TrackKind::Graphics,
                lut_grade("swap.cube", 1.0),
            )],
            empty_grade(),
            Vec::new(),
        );
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 0);
        // The file stands at describe time, then vanishes before the
        // frame draws: the draw stats the file for its frame and
        // refuses naming the path, instead of painting stale pixels.
        std::fs::remove_file(dir.join("swap.cube")).expect("delete cube");
        let mut raster = std::collections::HashMap::new();
        let mut frame = vec![0u8; 32 * 32 * 3];
        let err = draw_description(
            &desc,
            &mut raster,
            &[],
            &DynLayouts::default(),
            &crate::captions::CaptionLayouts::new(),
            false,
            &[],
            &[],
            &[],
            &[],
            &crate::path_paint::PathCache::new(),
            &dir,
            &mut frame,
        )
        .expect_err("vanished LUT must refuse the frame");
        assert!(err.contains("swap.cube"), "{err}");
        assert!(err.contains("vanished"), "{err}");
        assert!(err.contains("no MP4 was written"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lut_frame_cache_refuses_an_unparsable_grid() {
        let dir = lut_tmp("corrupt");
        std::fs::write(dir.join("bad.cube"), "LUT_3D_SIZE 2\n0 0\n").expect("write bad cube");
        let mut luts = LutFrameCache::new();
        let mut base = notation::model::ResolvedGrade::identity();
        base.lut.src = Some("bad.cube".to_string());
        let err = luts.grade(&dir, &base).expect_err("corrupt must refuse");
        assert!(err.contains("bad.cube"), "{err}");
        assert!(err.contains("unusable"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn durations_parse() {
        assert!(
            (parse_duration("  Duration: 00:00:03.00, start: 0.0").unwrap() - 3.0).abs() < 1e-9
        );
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
        assert_eq!(
            fit_rect(320, 200, 160, 120, Fit::Fill),
            (0.0, 0.0, 320.0, 200.0)
        );
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
            line: 1,
            column: 1,
            id: None,
            canvas: None,
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
        };
        let tmp = PathBuf::from("/tmp/x.framesmith-part");
        let standard = RenderOptions::default().encoder();
        let no_audio = encode_args(
            &comp,
            &[],
            None,
            &tmp,
            &standard,
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = no_audio.join(" ");
        for flag in [
            "libx264", "yuv420p", "aac", "48000", "rawvideo", "-f", "mp4",
        ] {
            assert!(joined.contains(flag), "{joined}");
        }
        // Sprint 3 T002: `standard` with no overrides emits no explicit
        // encoder flags, so its bytes match the Sprint 2 output exactly.
        assert!(!joined.contains("-crf"), "{joined}");
        assert!(!joined.contains("-preset"), "{joined}");
        assert!(!joined.contains("-b:v"), "{joined}");
        // The output path is the last argument (fake-FFmpeg compatible).
        assert_eq!(
            no_audio.last().unwrap(),
            &"/tmp/x.framesmith-part".to_string()
        );
        // With audio: filter trims and delays, still standard codecs.
        let seg = AudioSeg {
            path: PathBuf::from("/tmp/a.mp4"),
            trim_start: 0.0,
            trim_len: 1.0,
            delay_ms: 500,
        };
        let with_audio = encode_args(
            &comp,
            &[seg],
            None,
            &tmp,
            &standard,
            comp.duration,
            (comp.width, comp.height),
        )
        .join(" ");
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
        assert!(RenderOptions::default()
            .announce()
            .contains("preset=standard"));
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
            line: 1,
            column: 1,
            id: None,
            canvas: None,
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
        };
        let tmp = PathBuf::from("/tmp/x.framesmith-part");
        let draft = RenderOptions {
            preset: Preset::Draft,
            ..RenderOptions::default()
        }
        .encoder();
        let joined = encode_args(
            &comp,
            &[],
            None,
            &tmp,
            &draft,
            comp.duration,
            (comp.width, comp.height),
        )
        .join(" ");
        assert!(joined.contains("-crf 30"), "{joined}");
        assert!(joined.contains("-preset veryfast"), "{joined}");
        let high = RenderOptions {
            preset: Preset::High,
            ..RenderOptions::default()
        }
        .encoder();
        let joined = encode_args(
            &comp,
            &[],
            None,
            &tmp,
            &high,
            comp.duration,
            (comp.width, comp.height),
        )
        .join(" ");
        assert!(joined.contains("-crf 18"), "{joined}");
        assert!(joined.contains("-preset slow"), "{joined}");
        // A staged mix joins the encoder as a raw 48 kHz stereo input.
        let mix = PathBuf::from("/tmp/x.framesmith-mix.pcm");
        let joined = encode_args(
            &comp,
            &[],
            Some(&mix),
            &tmp,
            &high,
            comp.duration,
            (comp.width, comp.height),
        )
        .join(" ");
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
        for (total, workers) in [
            (0usize, 1),
            (1, 1),
            (5, 1),
            (5, 2),
            (5, 3),
            (60, 8),
            (10, 64),
            (7, 100),
        ] {
            let mut got = worker_order(total, workers);
            got.sort_unstable();
            assert_eq!(
                got,
                (0..total as u64).collect::<Vec<_>>(),
                "total={total} workers={workers}"
            );
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
            line: 1,
            column: 1,
            id: None,
            canvas: None,
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
            dyn_layouts: DynLayouts::default(),
            caption_layouts: crate::captions::CaptionLayouts::new(),
            bitmap_fallback: true,
            videos: Vec::new(),
            ranges: Vec::new(),
            images: Vec::new(),
            lotties: Vec::new(),
            raster: RasterCache::new(),
            paths: crate::path_paint::PathCache::new(),
            tree: Vec::new(),
            cache_stats: crate::footage_cache::CacheStats::default(),
            range_start: timebase::Time::ZERO,
            out_duration: 1.0,
            folder: std::path::PathBuf::from("."),
        }
    }

    fn red_box() -> notation::model::Clip {
        use notation::model::{BoxClip, Rgb, TrackKind};
        notation::model::Clip::Box(BoxClip {
            asserts: Default::default(),
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            start: 0.0,
            duration: 1.0,
            x: 4.0,
            y: 4.0,
            width: 20.0,
            height: 20.0,
            fill: Rgb { r: 255, g: 0, b: 0 },
            fill_paint: FillPaint::Solid,
            radius: 2.0,
            border_width: 1.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: notation::model::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: notation::model::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: notation::model::Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            order: 1,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })
    }

    // --- Sprint 21 adapters (T002): native charts in the stack ---

    fn chart_clip(
        kind: notation::model::ChartKind,
        data: Vec<notation::model::ChartPoint>,
    ) -> notation::model::Clip {
        use notation::model::{ChartClip, Rgb, TrackKind};
        notation::model::Clip::Chart(ChartClip {
            asserts: Default::default(),
            kind,
            title: Some("sales".to_string()),
            data,
            data_file: None,
            start: 0.0,
            duration: 1.0,
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            hidden: false,
            x: 0.0,
            y: 0.0,
            width: Some(64.0),
            height: Some(48.0),
            background: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            palette: vec![Rgb {
                r: 78,
                g: 121,
                b: 167,
            }],
            show_legend: true,
            show_gridlines: true,
            show_value_labels: true,
            min: None,
            max: None,
            order: 0,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
            line: 1,
            column: 1,
        })
    }

    fn chart_point(label: &str, value: f64) -> notation::model::ChartPoint {
        notation::model::ChartPoint {
            series: None,
            label: label.to_string(),
            value,
            x: None,
        }
    }

    /// Stage one chart raster exactly like `prepare_comp_images` does,
    /// so the frame below draws through the shared image blitter.
    fn stage_chart(prep: &mut Prepared, ch: &notation::model::ChartClip) {
        let (w, h) = crate::chart::viewport_px(ch, (prep.comp.width, prep.comp.height));
        let key = crate::chart::viewport_key(ch, w, h);
        let (mw, mh, rgba) = crate::chart::render_viewport(ch, w, h);
        prep.images.push(DecodedImage {
            path: std::path::PathBuf::from(".").join(key),
            width: mw,
            height: mh,
            rgba,
            scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        });
    }

    fn draw_cpu(prep: &Prepared, frame: u64) -> Vec<u8> {
        let mut raster = RasterCache::new();
        draw_one_frame(prep, &mut raster, Backend::Cpu, frame).expect("chart frame draws")
    }

    /// A chart clip enters the same composition stack as media: the
    /// pre-rendered raster composites over the background, and reruns
    /// (plus later frames, charts being static per film time) land
    /// byte-identical frames.
    #[test]
    fn chart_enters_the_composition_stack_deterministically() {
        for kind in [
            notation::model::ChartKind::Bar,
            notation::model::ChartKind::Pie,
            notation::model::ChartKind::Gauge,
        ] {
            let data = match kind {
                notation::model::ChartKind::Pie => {
                    vec![chart_point("a", 30.0), chart_point("b", 70.0)]
                }
                notation::model::ChartKind::Gauge => vec![chart_point("load", 62.0)],
                _ => vec![chart_point("Q1", 12.0), chart_point("Q2", 19.0)],
            };
            let clip = chart_clip(kind, data);
            let ch = match &clip {
                notation::model::Clip::Chart(c) => c.clone(),
                _ => unreachable!(),
            };
            let mut prep = box_prep(vec![clip]);
            stage_chart(&mut prep, &ch);
            let first = draw_cpu(&prep, 0);
            let rerun = draw_cpu(&prep, 0);
            assert_eq!(first, rerun, "{kind:?} reruns byte-identically");
            // Charts are static per film time: a later frame matches.
            let later = draw_cpu(&prep, 15);
            assert_eq!(first, later, "{kind:?} holds still across film time");
            // The white panel composites over the black background.
            assert!(
                first.iter().any(|&b| b > 0),
                "{kind:?} paints into the composition stack"
            );
        }
    }

    /// An empty chart draws its named axes into the stack: the frame
    /// is not the flat background, and reruns match.
    #[test]
    fn empty_chart_draws_named_axes_in_the_stack() {
        let clip = chart_clip(notation::model::ChartKind::Bar, Vec::new());
        let ch = match &clip {
            notation::model::Clip::Chart(c) => c.clone(),
            _ => unreachable!(),
        };
        let mut prep = box_prep(vec![clip]);
        stage_chart(&mut prep, &ch);
        let first = draw_cpu(&prep, 0);
        assert_eq!(first, draw_cpu(&prep, 0));
        assert!(
            first.iter().any(|&b| b > 0),
            "empty chart still names its axes"
        );
    }

    // --- Sprint 7 motion (T005/T006): 3D turns and visual blur ---

    fn sharp_box(x: f64, y: f64, w: f64, h: f64) -> ResolvedBox {
        ResolvedBox {
            doc: 0,
            order: 0,
            x,
            y,
            width: w,
            height: h,
            fill: Rgb { r: 255, g: 0, b: 0 },
            fill_paint: FillPaint::Solid,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: notation::model::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: notation::model::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: notation::model::Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
        }
    }

    fn turned_box(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> ResolvedBox {
        ResolvedBox {
            rotate_x: rx,
            rotate_y: ry,
            ..sharp_box(x, y, w, h)
        }
    }

    fn red_height_at(frame: &[u8], cw: u32, chh: u32, col: u32) -> u32 {
        (0..chh)
            .filter(|&j| pixel(frame, cw, col, j) != (0, 0, 0))
            .count() as u32
    }

    #[test]
    fn advanced_identity_matches_legacy_box_bytes() {
        // T005/T006: with no blur and an identity turn the layer path
        // reproduces the legacy blit byte-identically (sharp integer
        // box: every coverage is exactly 0 or 1).
        let b = sharp_box(4.0, 4.0, 20.0, 12.0);
        let mut legacy = vec![0u8; 64 * 48 * 3];
        draw_box(&mut legacy, 64, 48, &b, &[], Path::new(".")).unwrap();
        let mut advanced = vec![0u8; 64 * 48 * 3];
        draw_box_advanced(
            &mut advanced,
            64,
            48,
            &b,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert_eq!(advanced, legacy, "identity layer path matches the blit");
        // A rounded, bordered, translucent box matches within one
        // step per channel (the layer stores one rounded alpha byte).
        let mut soft = sharp_box(4.5, 5.5, 20.0, 12.0);
        soft.radius = 3.0;
        soft.opacity = 0.5;
        soft.border_width = 2.0;
        soft.border_color = Rgb { r: 0, g: 0, b: 255 };
        let mut legacy = vec![0u8; 64 * 48 * 3];
        draw_box(&mut legacy, 64, 48, &soft, &[], Path::new(".")).unwrap();
        let mut advanced = vec![0u8; 64 * 48 * 3];
        draw_box_advanced(
            &mut advanced,
            64,
            48,
            &soft,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert_eq!(legacy.len(), advanced.len());
        for (i, (a, l)) in advanced.iter().zip(legacy.iter()).enumerate() {
            assert!(
                (*a as i16 - *l as i16).abs() <= 1,
                "byte {i} drifts more than one step: {a} vs {l}"
            );
        }
    }

    #[test]
    fn flat_turn_foreshortens_through_the_origin() {
        // T005: rx=60 flat halves the height about the center line
        // (rows 20..39 for a 40-tall box at y=10); rx=90 edge-on
        // draws nothing.
        let b = turned_box(10.0, 10.0, 40.0, 40.0, 60.0, 0.0);
        let mut frame = vec![0u8; 64 * 64 * 3];
        draw_box_advanced(
            &mut frame,
            64,
            64,
            &b,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        for j in 20..40 {
            assert_eq!(pixel(&frame, 64, 30, j), (255, 0, 0), "row {j} fills");
        }
        assert_eq!(pixel(&frame, 64, 30, 19), (0, 0, 0), "above stays empty");
        assert_eq!(pixel(&frame, 64, 30, 40), (0, 0, 0), "below stays empty");
        assert_eq!(pixel(&frame, 64, 10, 30), (255, 0, 0), "full width keeps");
        assert_eq!(pixel(&frame, 64, 49, 30), (255, 0, 0), "full width keeps");
        let edge = turned_box(10.0, 10.0, 40.0, 40.0, 90.0, 0.0);
        let mut frame = vec![0u8; 64 * 64 * 3];
        draw_box_advanced(
            &mut frame,
            64,
            64,
            &edge,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert!(frame.iter().all(|&v| v == 0), "edge-on cards draw nothing");
    }

    #[test]
    fn origin_top_collapses_to_the_top_line() {
        // T005: the same rx=60 turn about the top edge pins the top
        // (rows 10..29) instead of the center (rows 20..39).
        use notation::model::{Placement, Turn3D};
        let mut top = turned_box(10.0, 10.0, 40.0, 40.0, 60.0, 0.0);
        top.turn = Turn3D {
            origin: Placement {
                x: notation::model::PlaceH::Left,
                y: notation::model::PlaceV::Top,
            },
            ..Turn3D::flat()
        };
        let mut frame = vec![0u8; 64 * 64 * 3];
        draw_box_advanced(
            &mut frame,
            64,
            64,
            &top,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert_eq!(pixel(&frame, 64, 30, 10), (255, 0, 0), "top pins");
        assert_eq!(
            pixel(&frame, 64, 30, 29),
            (255, 0, 0),
            "bottom halves to 29"
        );
        assert_eq!(
            pixel(&frame, 64, 30, 30),
            (0, 0, 0),
            "past the half stays empty"
        );
        assert_eq!(
            pixel(&frame, 64, 30, 35),
            (0, 0, 0),
            "center rows stay empty"
        );
    }

    #[test]
    fn perspective_turn_makes_a_trapezoid() {
        // T005: ry=45 flat foreshortens symmetrically (equal edge
        // heights); with p=200 the near (left) edge stands taller.
        let flat = turned_box(40.0, 40.0, 80.0, 40.0, 0.0, 45.0);
        let mut frame = vec![0u8; 160 * 120 * 3];
        draw_box_advanced(
            &mut frame,
            160,
            120,
            &flat,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        // Flat edges: find the first/last red columns, compare heights.
        let cols: Vec<u32> = (0..160)
            .filter(|&i| red_height_at(&frame, 160, 120, i) > 0)
            .collect();
        let (left, right) = (*cols.first().unwrap(), *cols.last().unwrap());
        let (lh, rh) = (
            red_height_at(&frame, 160, 120, left),
            red_height_at(&frame, 160, 120, right),
        );
        assert!(
            (lh as i32 - rh as i32).abs() <= 1,
            "flat edges match: {lh} vs {rh}"
        );
        // Perspective: the same card with p=200.
        let mut persp = turned_box(40.0, 40.0, 80.0, 40.0, 0.0, 45.0);
        persp.turn.perspective = Some(200.0);
        let mut frame = vec![0u8; 160 * 120 * 3];
        draw_box_advanced(
            &mut frame,
            160,
            120,
            &persp,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        let cols: Vec<u32> = (0..160)
            .filter(|&i| red_height_at(&frame, 160, 120, i) > 0)
            .collect();
        let (left, right) = (*cols.first().unwrap(), *cols.last().unwrap());
        let (lh, rh) = (
            red_height_at(&frame, 160, 120, left),
            red_height_at(&frame, 160, 120, right),
        );
        assert!(
            lh > rh + 5,
            "near edge {lh} stands taller than far edge {rh}"
        );
        // Both renders are deterministic.
        let mut again = vec![0u8; 160 * 120 * 3];
        draw_box_advanced(
            &mut again,
            160,
            120,
            &persp,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert_eq!(frame, again, "turned re-render is byte-identical");
    }

    #[test]
    fn pixel_box_blur_matches_the_gaussian() {
        // T006: a 1x1 red box blurred with σ=1 follows the hand
        // Gaussian independently: every drawn byte equals the rounded
        // separable kernel weight times 255 (tail bytes round to zero,
        // so the byte mass is the rounded mass, not 255).
        let mut b = sharp_box(10.0, 10.0, 1.0, 1.0);
        b.blur = 1.0;
        let mut frame = vec![0u8; 32 * 32 * 3];
        draw_box_advanced(
            &mut frame,
            32,
            32,
            &b,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        // Hand kernel, computed here from the Gaussian alone.
        let raw = [-3.0f64, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0].map(|x| (-x * x / 2.0).exp());
        let total: f64 = raw.iter().sum();
        let k: Vec<f64> = raw.iter().map(|v| v / total).collect();
        let expect = |di: i64, dj: i64| -> u8 {
            if di.abs() > 3 || dj.abs() > 3 {
                return 0;
            }
            (255.0 * k[(di + 3) as usize] * k[(dj + 3) as usize]).round() as u8
        };
        let (r, g, bl) = pixel(&frame, 32, 10, 10);
        assert_eq!((g, bl), (0, 0), "stays red");
        assert_eq!(r, expect(0, 0), "peak matches k0²·255");
        assert_eq!(expect(0, 0), 41, "hand peak is 41");
        for dj in -3..=3 {
            for di in -3..=3 {
                let (pr, pg, pb) = pixel(&frame, 32, (10 + di) as u32, (10 + dj) as u32);
                assert_eq!((pg, pb), (0, 0), "stays red at ({di},{dj})");
                assert_eq!(
                    pr,
                    expect(di, dj),
                    "tap ({di},{dj}) matches the hand kernel"
                );
            }
        }
        assert_eq!(pixel(&frame, 32, 0, 0), (0, 0, 0), "far corners stay empty");
        let mass: u64 = frame.iter().step_by(3).map(|&v| v as u64).sum();
        let hand: u64 = (-3..=3)
            .flat_map(|dj| (-3..=3).map(move |di| expect(di, dj) as u64))
            .sum();
        assert_eq!(mass, hand, "byte mass matches the rounded kernel");
        // Deterministic re-render.
        let mut again = vec![0u8; 32 * 32 * 3];
        draw_box_advanced(
            &mut again,
            32,
            32,
            &b,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert_eq!(frame, again, "blurred re-render is byte-identical");
    }

    #[test]
    fn static_filter_blur_softens_box_edges() {
        // T006: filter="blur(2.5)" (static own blur) softens the edge
        // while the interior stays solid, and differs from the sharp
        // draw.
        let mut b = sharp_box(10.0, 10.0, 20.0, 20.0);
        b.blur = 2.5;
        let mut blurred = vec![0u8; 64 * 64 * 3];
        draw_box_advanced(
            &mut blurred,
            64,
            64,
            &b,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert_eq!(
            pixel(&blurred, 64, 20, 20),
            (255, 0, 0),
            "interior stays solid"
        );
        let (er, _, _) = pixel(&blurred, 64, 10, 10);
        assert!(er > 0 && er < 255, "the corner softens, got {er}");
        let (or_, _, _) = pixel(&blurred, 64, 8, 8);
        assert!(or_ > 0, "softness spreads outside, got {or_}");
        b.blur = 0.0;
        let mut sharp = vec![0u8; 64 * 64 * 3];
        draw_box(&mut sharp, 64, 64, &b, &[], Path::new(".")).unwrap();
        assert_ne!(blurred, sharp, "blur changes the picture");
    }

    #[test]
    fn image_layer_identity_matches_legacy_blit() {
        // T006: with no blur and an identity turn the image layer path
        // reproduces the legacy blit byte-identically (opaque solid).
        let decoded = solid_image(4, 4, 0, 255, 0, 255);
        let im = test_image(Fit::Fill, Placement::center());
        let mut legacy = vec![0u8; 32 * 32 * 3];
        draw_image(&mut legacy, 32, 32, &im, &decoded);
        let mut frame = vec![0u8; 32 * 32 * 3];
        draw_image_advanced(
            &mut frame,
            32,
            32,
            &im,
            &decoded,
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        );
        assert_eq!(frame, legacy, "identity image layer matches the blit");
        assert_eq!(pixel(&frame, 32, 0, 0), (0, 255, 0));
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
        assert_eq!(
            solo[0].1, full7.1,
            "solo frame 7 must equal full-render frame 7"
        );
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
        use notation::model::{Clip, Fit, TrackKind, VideoClip};
        let prep = box_prep(vec![Clip::Video(VideoClip {
            asserts: Default::default(),
            id: None,
            track: TrackKind::Video,
            label: None,
            start_after: None,
            hidden: false,
            path: "clip.mp4".to_string(),
            start: 0.0,
            duration: 1.0,
            duration_inferred: false,
            fit: Fit::Contain,
            order: 0,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            crossfade: notation::model::Crossfade::default(),
            duck_under: None,
            fx: Default::default(),
            waveform: Default::default(),
            trim_start: 0.0,
            trim_end: None,
            rate: 1.0,
            loop_clip: false,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        })]);
        for w in [1, 2, 4] {
            let err = draw_batch_parallel_with_workers(&prep, Backend::Cpu, &[0, 1, 2], w)
                .expect_err("a frame no probe covers must fail at worker count {w}");
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
            assert_eq!(
                worker_count(),
                good.trim().parse::<usize>().unwrap(),
                "{good:?} wins"
            );
        }
        for bad in ["0", "65", "100", "abc", "", "-3", "2.5"] {
            std::env::set_var("FRAMESMITH_WORKERS", bad);
            assert_eq!(
                worker_count(),
                fallback,
                "{bad:?} falls back to the machine default"
            );
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
    fn plain_resample_doubles_and_halves_pitch() {
        // Sprint 5 T002: a 440 Hz tone over 1 s (48000 frames stereo)
        // at rate 2 keeps 24000 frames whose zero-crossing rate is
        // doubled (880 Hz, pitch follows speed); at rate 0.5 it keeps
        // 96000 frames at half the crossing rate (220 Hz).
        let tone: Vec<i16> = (0..48000)
            .flat_map(|j| {
                let v = (2.0 * std::f64::consts::PI * 440.0 * j as f64 / 48000.0).sin();
                let s = (v * 20000.0) as i16;
                [s, s]
            })
            .collect();
        let crossings = |pcm: &[i16]| -> usize {
            (1..pcm.len() / 2)
                .filter(|&j| {
                    (pcm[(j - 1) * 2] <= 0 && pcm[j * 2] > 0)
                        || (pcm[(j - 1) * 2] >= 0 && pcm[j * 2] < 0)
                })
                .count()
        };
        let base = crossings(&tone);
        assert!(
            (base as i32 - 880).abs() <= 4,
            "base tone crosses ~880, got {base}"
        );
        let fast = resample_plain(&tone, 2.0);
        assert_eq!(fast.len() / 2, 24000, "rate 2 halves the frame count");
        let cf = crossings(&fast);
        assert!(
            (cf as i32 - 880).abs() <= 6,
            "rate 2 keeps the source crossings in half the time (pitch doubles), got {cf}"
        );
        let slow = resample_plain(&tone, 0.5);
        assert_eq!(slow.len() / 2, 96000, "rate 0.5 doubles the frame count");
        let cs = crossings(&slow);
        assert!(
            (cs as i32 - 880).abs() <= 6,
            "rate 0.5 spreads the source crossings over double time (pitch halves), got {cs}"
        );
        // Unity returns the input unchanged, sample for sample.
        assert_eq!(resample_plain(&tone, 1.0), tone, "rate 1 is the identity");
        assert!(resample_plain(&[], 2.0).is_empty());
        // A constant level survives resampling without scaling.
        let flat: Vec<i16> = vec![8000; 2 * 4800];
        let flat_fast = resample_plain(&flat, 2.0);
        assert_eq!(flat_fast.len(), 2 * 2400);
        assert!(flat_fast.iter().all(|&x| x == 8000));
    }

    #[test]
    fn effective_window_clamps_past_end_to_the_probe() {
        // Sprint 5 T002: the probe wins — a past-end trim-end clamps to
        // the true source end, an omitted trim-end means the source
        // end, and an unknown source end stays unknown.
        let clip = |ts: f64, te: Option<f64>| notation::model::VideoClip {
            asserts: Default::default(),
            id: None,
            track: TrackKind::Video,
            label: None,
            start_after: None,
            hidden: false,
            path: "c.mp4".into(),
            start: 0.0,
            duration: 1.0,
            duration_inferred: false,
            fit: Fit::Contain,
            order: 0,
            volume_db: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            crossfade: notation::model::Crossfade::default(),
            duck_under: None,
            fx: Default::default(),
            waveform: Default::default(),
            trim_start: ts,
            trim_end: te,
            rate: 1.0,
            loop_clip: false,
            line: 1,
            column: 1,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: Default::default(),
        };
        assert_eq!(
            effective_source_window(&clip(1.0, Some(9.0)), Some(4.0)),
            (1.0, Some(4.0))
        );
        assert_eq!(
            effective_source_window(&clip(1.0, Some(3.0)), Some(4.0)),
            (1.0, Some(3.0))
        );
        assert_eq!(
            effective_source_window(&clip(0.0, None), Some(4.0)),
            (0.0, Some(4.0))
        );
        assert_eq!(effective_source_window(&clip(1.0, None), None), (1.0, None));
        assert_eq!(
            effective_source_window(&clip(1.0, Some(9.0)), None),
            (1.0, Some(9.0))
        );
    }

    #[test]
    fn loop_wrap_repeats_the_trimmed_window_at_rate() {
        // Sprint 5 T003: the repeated unit is the trimmed window played
        // at the declared rate — composition seconds map to source
        // seconds modulo the window length, with no gap at the loop
        // point (the window end shows the first frame again).
        let w = Some(2.0);
        assert!((wrap_loop_time(1.0, 1.0, w) - 1.0).abs() < 1e-9);
        assert!((wrap_loop_time(2.5, 1.0, w) - 2.5).abs() < 1e-9);
        // One full window later: back at the window opening.
        assert!((wrap_loop_time(3.0, 1.0, w) - 1.0).abs() < 1e-9);
        assert!((wrap_loop_time(4.5, 1.0, w) - 2.5).abs() < 1e-9);
        // Two loop points: 1 + 2k always reopens the window.
        assert!((wrap_loop_time(5.0, 1.0, w) - 1.0).abs() < 1e-9);
        assert!((wrap_loop_time(7.0, 1.0, w) - 1.0).abs() < 1e-9);
        // Unknown or degenerate windows never wrap (hold instead).
        assert!((wrap_loop_time(9.0, 1.0, None) - 9.0).abs() < 1e-9);
        assert!((wrap_loop_time(9.0, 1.0, Some(0.0)) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn hold_keeps_the_last_picture_past_the_source_end() {
        // Sprint 5 T003: a non-looping overrun holds the last decoded
        // frame; in-window times still pick by exact timestamp.
        let range = DecodedRange {
            video_idx: 0,
            width: 2,
            height: 1,
            frames: vec![10, 20, 30, 40, 50, 60, 70, 80, 90],
            pts: vec![1.0, 2.0, 3.0],
        };
        assert_eq!(pick_picture_or_hold(&range, 1.0), Some(0));
        assert_eq!(pick_picture_or_hold(&range, 2.5), Some(1));
        assert_eq!(pick_picture_or_hold(&range, 3.0), Some(2));
        // Far past the end: the last picture holds, never None.
        assert_eq!(pick_picture_or_hold(&range, 4.0), Some(2));
        assert_eq!(pick_picture_or_hold(&range, 99.0), Some(2));
        // Before the first timestamp: the first picture applies.
        assert_eq!(pick_picture_or_hold(&range, 0.0), Some(0));
        // Wrapping then holding covers the loop path too.
        assert_eq!(
            pick_picture_or_hold(&range, wrap_loop_time(5.0, 1.0, Some(2.0))),
            Some(0)
        );
        assert_eq!(
            pick_picture_or_hold(&range, wrap_loop_time(4.5, 1.0, Some(2.0))),
            Some(1)
        );
    }

    #[test]
    fn loop_unit_tiles_with_no_gap() {
        // Sprint 5 T003: the resampled window unit repeats sample for
        // sample, so sound loops with picture and no slip.
        let unit: Vec<i16> = vec![1, 2, 3, 4, 5, 6];
        assert_eq!(tile_loop_unit(&unit, 2), vec![1, 2, 3, 4]);
        assert_eq!(tile_loop_unit(&unit, 3), vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(tile_loop_unit(&unit, 5), vec![1, 2, 3, 4, 5, 6, 1, 2, 3, 4]);
        assert!(tile_loop_unit(&unit, 0).is_empty());
        assert!(tile_loop_unit(&[], 4).is_empty());
    }

    #[test]
    fn rotation_parses_display_matrix_and_tag_lines() {
        // Sprint 5 T005: the demuxer-computed display matrix wins, the
        // legacy `rotate` tag is the fallback, absence means stored.
        let matrix =
            "Stream #0:0: Video: h264, 320x240\n      Display Matrix: rotation of 90.00 degrees";
        assert!((parse_container_rotation(matrix) - 90.0).abs() < 1e-9);
        let neg =
            "Stream #0:0: Video: h264, 320x240\n      Display Matrix: rotation of -90.00 degrees";
        assert!((parse_container_rotation(neg) - 270.0).abs() < 1e-9);
        let half = "Display Matrix: rotation of 180.00 degrees";
        assert!((parse_container_rotation(half) - 180.0).abs() < 1e-9);
        let tag = "Stream #0:0: Video: h264, 320x240\n      rotate : 90";
        assert!((parse_container_rotation(tag) - 90.0).abs() < 1e-9);
        let full_turn = "Display Matrix: rotation of 360.00 degrees";
        assert!((parse_container_rotation(full_turn) - 0.0).abs() < 1e-9);
        let plain = "Stream #0:0: Video: h264, 320x240 [SAR 1:1 DAR 4:3]";
        assert!((parse_container_rotation(plain) - 0.0).abs() < 1e-9);
        let broken = "Display Matrix: rotation of sideways degrees";
        assert!((parse_container_rotation(broken) - 0.0).abs() < 1e-9);
        // Matrix wins over a conflicting tag.
        let both = "rotate : 180\nDisplay Matrix: rotation of 90.00 degrees";
        assert!((parse_container_rotation(both) - 90.0).abs() < 1e-9);
        assert!(!needs_autorotate(0.0));
        assert!(needs_autorotate(90.0));
        assert!(needs_autorotate(180.0));
        assert!(needs_autorotate(270.0));
        assert!(!needs_autorotate(45.0));
    }

    #[test]
    fn oriented_dimensions_swap_on_quarter_turns() {
        // Sprint 5 T005: portrait phone footage (stored landscape,
        // rotated 90) orients to portrait; half turns and upright keep
        // stored axes; exotic angles render stored.
        assert_eq!(oriented_dimensions(320, 240, 90.0), (240, 320));
        assert_eq!(oriented_dimensions(320, 240, 270.0), (240, 320));
        assert_eq!(oriented_dimensions(320, 240, 0.0), (320, 240));
        assert_eq!(oriented_dimensions(320, 240, 180.0), (320, 240));
        assert_eq!(oriented_dimensions(320, 240, 45.0), (320, 240));
        assert_eq!(
            oriented_dimensions(320, 240, normalize_rotation(-90.0)),
            (240, 320)
        );
    }

    #[test]
    fn packet_pts_parse_rejects_missing_timing() {
        // Sprint 5 T004: every packet timestamp must parse; one N/A
        // (raw streams carry no timing) poisons the whole reply, and an
        // empty reply maps nothing.
        assert_eq!(
            parse_packet_pts("0.000000\n0.033333\n0.066667\n").unwrap(),
            vec![0.0, 0.033333, 0.066667]
        );
        assert!(parse_packet_pts("0.000000\nN/A\n0.066667\n").is_err());
        assert!(parse_packet_pts("").is_err());
        assert!(parse_packet_pts("bogus\n").is_err());
        assert!(parse_packet_pts("nan\n").is_err());
        assert!(parse_packet_pts("inf\n").is_err());
        // Negative timestamps (edit lists) are valid timing.
        assert_eq!(
            parse_packet_pts("-0.040000\n0.000000\n").unwrap(),
            vec![-0.04, 0.0]
        );
    }

    #[test]
    fn decode_start_covers_gap_windows() {
        // Sprint 5 T004: the decode starts 1 ms before the frame whose
        // interval covers the window opening — including a VFR gap —
        // and at zero for windows opening before the first frame.
        let pts: Vec<f64> = [0.0, 0.033333, 0.966667, 2.0, 2.033333]
            .into_iter()
            .collect();
        let start = decode_start_for_seek(&pts, 1.5);
        assert!(
            (start - 0.965667).abs() < 1e-9,
            "backs up 1 ms before the covering 0.966667 frame, got {start}"
        );
        let exact = decode_start_for_seek(&pts, 2.0);
        assert!(
            (exact - 1.999).abs() < 1e-9,
            "an exact frame time still includes its own frame, got {exact}"
        );
        assert!(
            (decode_start_for_seek(&pts, 0.0) - 0.0).abs() < 1e-9,
            "zero stays zero (historical shape)"
        );
        assert!(
            (decode_start_for_seek(&pts, 0.01) - 0.0).abs() < 1e-9,
            "first-frame coverage clamps to zero"
        );
        // A seek before the first timestamp decodes from the start.
        let late: Vec<f64> = [0.5, 0.533333].into_iter().collect();
        assert!((decode_start_for_seek(&late, 0.1) - 0.0).abs() < 1e-9);
        // Budgets count the window plus two spare slots.
        assert_eq!(
            decode_frame_budget(&pts, 0.965667, 1.7),
            3,
            "covering frame only (+2 spare)"
        );
        assert_eq!(
            decode_frame_budget(&pts, 0.0, 2.1),
            7,
            "five window packets (+2 spare)"
        );
    }

    #[test]
    fn vfr_gap_selects_the_covering_frame() {
        // Sprint 5 T004: sampling by true presentation intervals shows
        // the covering frame across a VFR gap — the 0.966667 frame for
        // every instant in [0.966667, 2.0) — never the next frame early.
        let fs = 3usize;
        let range = DecodedRange {
            video_idx: 0,
            width: 1,
            height: 1,
            frames: vec![0u8; 5 * fs],
            pts: vec![0.9, 0.933333, 0.966667, 2.0, 2.033333],
        };
        assert_eq!(
            pick_picture(&range, 1.5),
            Some(2),
            "gap instant shows the covering frame"
        );
        assert_eq!(pick_picture(&range, 0.966667), Some(2));
        assert_eq!(pick_picture(&range, 1.999), Some(2));
        assert_eq!(
            pick_picture(&range, 2.0),
            Some(3),
            "the next frame starts exactly at its timestamp"
        );
        assert_eq!(pick_picture(&range, 0.95), Some(1));
        // Holding past the source end still applies past the last frame.
        assert_eq!(pick_picture_or_hold(&range, 99.0), Some(4));
    }

    #[test]
    fn placed_mix_applies_gain_and_fades() {
        // Sprint 3 T009/T010: one constant tone placed with unity gain
        // keeps its level, silence gain zeroes it, and a full fade-in
        // over the window starts near silence and reaches unity.
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let empty = crate::envelope::SoundEnvelope::default();
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut acc,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &[],
            None,
            &notation::model::SoundFx::default(),
        );
        assert!((acc[2 * 2400] - 10000.0 / 32768.0).abs() < 1e-6);
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut acc,
            &tone,
            4800,
            0.0,
            0.1,
            0.0,
            0.0,
            0.0,
            &empty,
            &[],
            None,
            &notation::model::SoundFx::default(),
        );
        assert!(acc.iter().all(|&x| x == 0.0));
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut acc,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.1,
            0.0,
            &empty,
            &[],
            None,
            &notation::model::SoundFx::default(),
        );
        assert!(
            acc[0].abs() < acc[2 * 4799].abs() * 0.05,
            "{} vs {}",
            acc[0],
            acc[2 * 4799]
        );
        assert!((acc[2 * 4799] - 10000.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn placed_mix_applies_volume_envelopes_per_sample() {
        // Sprint 10 T001: a volume envelope over a constant tone matches
        // hand-computed sample values exactly (constant tone, unity
        // static gain, no fades).
        use crate::envelope::{quantize, SoundEnvelope, SoundSeg};
        use notation::model::EaseDecl;
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let seg = SoundSeg {
            start: 0.0,
            end: 0.1,
            from: 0.0,
            to: 1.0,
            ease: EaseDecl::Linear,
            line: 1,
            col: 1,
        };
        let env = SoundEnvelope {
            vol: vec![seg],
            pan: Vec::new(),
        };
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut acc,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &env,
            &[],
            None,
            &notation::model::SoundFx::default(),
        );
        // Sample-exact ramp: sample i rides (i + 1) / 4800 through the
        // mixer's op order (the envelope value rounds once, the final
        // scale rounds once).
        let v0 = 1f32 / 4800.0;
        assert_eq!(acc[0], 10000.0 / 32768.0 * v0);
        assert_eq!(acc[2 * 2399], 10000.0 / 32768.0 * 0.5);
        assert_eq!(acc[2 * 4799], 10000.0 / 32768.0);
        // The quantized chain is what the mixer evaluated.
        let q = quantize(&env.vol, 0.0);
        assert_eq!(q.len(), 1);
        assert_eq!((q[0].q0, q[0].q1), (0, 4_800));
    }

    #[test]
    fn placed_mix_balances_pan_envelopes_per_sample() {
        // Sprint 10 T001: a hard-left pan keeps the left channel exact
        // and silences the right one, sample for sample.
        use crate::envelope::SoundEnvelope;
        let stereo: Vec<i16> = (0..4800).flat_map(|_| [20000, 10000]).collect();
        let env = SoundEnvelope {
            vol: Vec::new(),
            pan: vec![crate::envelope::SoundSeg {
                start: 0.0,
                end: 0.1,
                from: -1.0,
                to: -1.0,
                ease: notation::model::EaseDecl::Linear,
                line: 1,
                col: 1,
            }],
        };
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut acc,
            &stereo,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &env,
            &[],
            None,
            &notation::model::SoundFx::default(),
        );
        for i in 0..4800 {
            assert_eq!(acc[2 * i], 20000.0 / 32768.0);
            assert_eq!(acc[2 * i + 1], 0.0);
        }
    }

    #[test]
    fn placed_mix_nulls_sugar_against_explicit_envelopes() {
        // Sprint 10 T002: sugared fades and explicit-envelope twins
        // place bit-identical samples. The twin spells each fade as one
        // volume segment; fades multiply with volume envelopes while
        // volume segments resolve last-wins, so twins cover the
        // non-overlapping fade pairs an explicit key can spell.
        use crate::envelope::{SoundEnvelope, SoundSeg};
        use notation::model::EaseDecl;
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let empty = crate::envelope::SoundEnvelope::default();
        let mut sugared = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut sugared,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.05,
            0.025,
            &empty,
            &[],
            None,
            &notation::model::SoundFx::default(),
        );
        let twin = SoundEnvelope {
            vol: vec![
                SoundSeg {
                    start: 0.0,
                    end: 0.05,
                    from: 0.0,
                    to: 1.0,
                    ease: EaseDecl::Linear,
                    line: 1,
                    col: 1,
                },
                SoundSeg {
                    start: 0.075,
                    end: 0.1,
                    from: 1.0,
                    to: 0.0,
                    ease: EaseDecl::Linear,
                    line: 2,
                    col: 1,
                },
            ],
            pan: Vec::new(),
        };
        let mut explicit = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut explicit,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &twin,
            &[],
            None,
            &notation::model::SoundFx::default(),
        );
        assert_eq!(sugared, explicit);
    }

    #[test]
    fn placed_mix_shapes_crossfade_segments_exactly() {
        // Sprint 10 T003: a linear outgoing segment runs the shared
        // linear primitive through the mixer's op order, and a hard
        // cut holds exact silence across the whole overlap.
        use crate::envelope::{SoundEnvelope, XSeg};
        use notation::model::Crossfade;
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let empty = SoundEnvelope::default();
        let out = [XSeg {
            q0: 0,
            q1: 4_800,
            shape: Crossfade::Linear,
            outgoing: true,
        }];
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut acc,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &out,
            None,
            &notation::model::SoundFx::default(),
        );
        assert_eq!(acc[0], 10000.0 / 32768.0 * (4_799.0f32 / 4_800.0));
        assert_eq!(acc[2 * 2399], 10000.0 / 32768.0 * 0.5);
        assert_eq!(acc[2 * 4799], 0.0);
        // The cut twin silences every covered sample exactly.
        let cut = [XSeg {
            q0: 0,
            q1: 4_800,
            shape: Crossfade::None,
            outgoing: true,
        }];
        let mut silenced = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut silenced,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &cut,
            None,
            &notation::model::SoundFx::default(),
        );
        assert!(silenced.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn placed_mix_gates_duck_windows_exactly() {
        // Sprint 10 T004: voice-window samples scale by the declared
        // gain exactly; the samples around the gate play full.
        use crate::envelope::{DuckWin, SoundEnvelope};
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let empty = SoundEnvelope::default();
        let gate = DuckWin {
            v0: 100,
            v1: 200,
            amount: 0.25,
        };
        let mut acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut acc,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &[],
            Some(gate),
            &notation::model::SoundFx::default(),
        );
        let full = 10000.0 / 32768.0;
        assert_eq!(acc[2 * 99], full);
        assert_eq!(acc[2 * 100], full * 0.25);
        assert_eq!(acc[2 * 199], full * 0.25);
        assert_eq!(acc[2 * 200], full);
        assert_eq!(acc[2 * 4799], full);
    }

    #[test]
    fn effected_dry_head_nulls_against_the_dry_path() {
        // Sprint 10 T005: an effected clip renders its dry head through
        // the same gain primitives, so the head nulls exactly against
        // the dry mix; the echo tap lands one spacing later at the wet
        // level; and a tail with no room truncates to the dry mix.
        use crate::envelope::SoundEnvelope;
        use notation::model::SoundFx;
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let empty = SoundEnvelope::default();
        let dry_fx = SoundFx::default();
        let wet_fx = SoundFx {
            delay: 0.1,
            delay_feedback: 0.0,
            delay_mix: 1.0,
            ..Default::default()
        };
        let mut dry_acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut dry_acc,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &[],
            None,
            &dry_fx,
        );
        // Room for the dry head plus the one 4800-sample tap.
        let mut wet_acc = vec![0.0f32; 2 * 9600];
        place_into_mix(
            &mut wet_acc,
            &tone,
            9600,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &[],
            None,
            &wet_fx,
        );
        assert_eq!(
            &wet_acc[..2 * 4800],
            &dry_acc[..],
            "the dry head nulls exactly"
        );
        assert_eq!(
            wet_acc[2 * 4800],
            dry_acc[0],
            "the lone tap replays the dry head at unity wet"
        );
        // With no room past the head, the tail truncates to the dry mix.
        let mut cut_acc = vec![0.0f32; 2 * 4800];
        place_into_mix(
            &mut cut_acc,
            &tone,
            4800,
            0.0,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &[],
            None,
            &wet_fx,
        );
        assert_eq!(cut_acc, dry_acc, "a fully cut tail is the dry mix");
    }

    #[test]
    fn effected_duck_gates_follow_absolute_samples() {
        // Sprint 10 T005: the duck gate of an effected clip still reads
        // absolute mix samples (not scratch offsets), and the echo of a
        // ducked sample carries the ducked level.
        use crate::envelope::{DuckWin, SoundEnvelope};
        use notation::model::SoundFx;
        let tone: Vec<i16> = vec![10000; 2 * 4800];
        let empty = SoundEnvelope::default();
        let wet_fx = SoundFx {
            delay: 0.1,
            delay_feedback: 0.0,
            delay_mix: 1.0,
            ..Default::default()
        };
        // The clip starts at frame 4800; the gate covers its samples
        // 100..200 (absolute 4900..5000).
        let gate = DuckWin {
            v0: 4900,
            v1: 5000,
            amount: 0.25,
        };
        let mut acc = vec![0.0f32; 2 * 14400];
        place_into_mix(
            &mut acc,
            &tone,
            14400,
            0.1,
            0.1,
            1.0,
            0.0,
            0.0,
            &empty,
            &[],
            Some(gate),
            &wet_fx,
        );
        let full = 10000.0 / 32768.0;
        assert_eq!(acc[2 * 4899], full);
        assert_eq!(acc[2 * 4900], full * 0.25);
        assert_eq!(acc[2 * 4999], full * 0.25);
        assert_eq!(acc[2 * 5000], full);
        // The tap of gated sample 100 lands 4800 later, still gated.
        assert_eq!(acc[2 * 9700], full * 0.25);
        assert_eq!(acc[2 * 9699], full);
    }

    // --- Sprint 6 T008: sub-range validation ---------------------------------

    fn range_fps() -> notation::model::Fps {
        notation::model::Fps::from_int(30)
    }

    #[test]
    fn range_defaults_to_the_whole_composition() {
        assert_eq!(
            resolve_range("main", 4.0, &range_fps(), None, None),
            Ok(None)
        );
        assert_eq!(
            resolve_range("main", 4.0, &range_fps(), Some(0.0), None),
            Ok(Some(RenderRange {
                start: 0.0,
                end: 4.0
            }))
        );
        assert_eq!(
            resolve_range("main", 4.0, &range_fps(), None, Some(2.5)),
            Ok(Some(RenderRange {
                start: 0.0,
                end: 2.5
            }))
        );
        assert_eq!(
            resolve_range("main", 4.0, &range_fps(), Some(1.0), Some(3.0)),
            Ok(Some(RenderRange {
                start: 1.0,
                end: 3.0
            }))
        );
    }

    #[test]
    fn range_refusals_name_the_window_and_the_bounds() {
        // Empty: start and end coincide.
        let e = resolve_range("main", 4.0, &range_fps(), Some(2.0), Some(2.0)).unwrap_err();
        assert!(e.contains("empty"), "{e}");
        assert!(e.contains("4.000s"), "{e}");
        assert!(
            e.contains("accepted window is 0 <= start < end <= 4.000s"),
            "{e}"
        );
        assert!(e.contains("no MP4 was written"), "{e}");
        // Backward: start runs past end.
        let e = resolve_range("main", 4.0, &range_fps(), Some(3.0), Some(1.0)).unwrap_err();
        assert!(e.contains("backward"), "{e}");
        assert!(e.contains("\"main\""), "{e}");
        // Out of range: past the end, before zero, or starting at the end.
        for (s, e2) in [(0.0, 9.0), (-1.0, 2.0), (4.0, 5.0), (1.0, 4.0001)] {
            let e = resolve_range("main", 4.0, &range_fps(), Some(s), Some(e2)).unwrap_err();
            assert!(e.contains("outside the composition"), "{e}");
            assert!(e.contains("no MP4 was written"), "{e}");
        }
        // End exactly at the duration is the closed edge, not out of range.
        assert!(resolve_range("main", 4.0, &range_fps(), Some(1.0), Some(4.0)).is_ok());
        // Non-finite bounds never render.
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(resolve_range("main", 4.0, &range_fps(), Some(bad), Some(2.0)).is_err());
            assert!(resolve_range("main", 4.0, &range_fps(), Some(0.0), Some(bad)).is_err());
        }
    }

    #[test]
    fn range_crop_slices_the_full_mix_samples() {
        // 4 s of stereo silence with a marker second: the [1,3) crop is
        // exactly the full mix sliced to those sample frames.
        let dir =
            std::env::temp_dir().join(format!("framesmith-range-crop-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mix = dir.join("mix.pcm");
        let mut bytes = vec![0u8; 4 * 48000 * 2 * 2];
        for i in (48000 * 2)..(96000 * 2) {
            let v = (i as i16).to_le_bytes();
            bytes[i * 2] = v[0];
            bytes[i * 2 + 1] = v[1];
        }
        std::fs::write(&mix, &bytes).unwrap();
        crop_mix_to_range(&mix, 1.0, 3.0).unwrap();
        let got = std::fs::read(&mix).unwrap();
        assert_eq!(got.len(), 2 * 48000 * 2 * 2);
        assert_eq!(&got[..], &bytes[48000 * 2 * 2..144000 * 2 * 2]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Sprint 8 look (T003): gradient and image fills ---

    fn stops_rgb() -> Vec<GradientStop> {
        use notation::model::GradientStop;
        vec![
            GradientStop {
                offset: 0.0,
                color: Rgb { r: 255, g: 0, b: 0 },
                line: 1,
                column: 1,
            },
            GradientStop {
                offset: 0.5,
                color: Rgb { r: 0, g: 255, b: 0 },
                line: 2,
                column: 1,
            },
            GradientStop {
                offset: 1.0,
                color: Rgb { r: 0, g: 0, b: 255 },
                line: 3,
                column: 1,
            },
        ]
    }

    fn paint_box(paint: notation::model::FillPaint, x: f64, y: f64, w: f64, h: f64) -> ResolvedBox {
        ResolvedBox {
            doc: 0,
            order: 0,
            x,
            y,
            width: w,
            height: h,
            fill: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            fill_paint: paint,
            radius: 0.0,
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            stroke_position: notation::model::StrokePosition::Inside,
            box_shadow: None,
            blend_mode: notation::model::BlendMode::Normal,
            backdrop_blur: 0.0,
            backdrop_brightness: 1.0,
            overflow: notation::model::Overflow::Visible,
            mask: None,
            clip_path: None,
            opacity: 1.0,
            scale: 1.0,
            rotation: 0.0,
            rotate_x: 0.0,
            rotate_y: 0.0,
            blur: 0.0,
            turn: notation::model::Turn3D::flat(),
        }
    }

    #[test]
    fn gradient_samples_interpolate_in_srgb_with_end_clamps() {
        let stops = stops_rgb();
        assert_eq!(sample_gradient(&stops, 0.0), (255, 0, 0));
        assert_eq!(sample_gradient(&stops, 1.0), (0, 0, 255));
        // sRGB channel lerp: halfway red to green rounds half up.
        assert_eq!(sample_gradient(&stops, 0.25), (128, 128, 0));
        assert_eq!(sample_gradient(&stops, 0.5), (0, 255, 0));
        assert_eq!(sample_gradient(&stops, 0.75), (0, 128, 128));
        // Outside clamps to the end stops.
        assert_eq!(sample_gradient(&stops, -0.5), (255, 0, 0));
        assert_eq!(sample_gradient(&stops, 2.0), (0, 0, 255));
        // One stop paints flat at any t.
        let one = vec![stops[1].clone()];
        assert_eq!(sample_gradient(&one, 0.0), (0, 255, 0));
        assert_eq!(sample_gradient(&one, 0.9), (0, 255, 0));
    }

    #[test]
    fn linear_three_stop_ramp_clips_to_the_element_box() {
        use notation::model::FillPaint;
        // Half-pixel x offset so pixel centers land on exact stops:
        // lx is an integer, t hits 0.5 at lx 24.
        let b = paint_box(
            FillPaint::Linear {
                angle: 90.0,
                stops: stops_rgb(),
            },
            7.5,
            8.0,
            48.0,
            32.0,
        );
        let mut frame = vec![0u8; 64 * 48 * 3];
        draw_box(&mut frame, 64, 48, &b, &[], Path::new(".")).unwrap();
        // Exact stop at the gradient middle.
        assert_eq!(pixel(&frame, 64, 31, 24), (0, 255, 0));
        // Ramp ends approach the end stops.
        let (r, g, bl) = pixel(&frame, 64, 8, 24);
        assert!(
            r > 230 && g < 24 && bl == 0,
            "left should approach red, got ({r}, {g}, {bl})"
        );
        let (r, g, bl) = pixel(&frame, 64, 54, 24);
        assert!(
            bl > 230 && g < 24 && r == 0,
            "right should approach blue, got ({r}, {g}, {bl})"
        );
        // Outside the box the canvas shows through.
        assert_eq!(pixel(&frame, 64, 6, 24), (0, 0, 0));
        assert_eq!(pixel(&frame, 64, 0, 0), (0, 0, 0));
        // The ramp is constant down a column (horizontal gradient).
        assert_eq!(pixel(&frame, 64, 31, 8), pixel(&frame, 64, 31, 39));
    }

    #[test]
    fn linear_angle_zero_runs_bottom_to_top() {
        use notation::model::FillPaint;
        let b = paint_box(
            FillPaint::Linear {
                angle: 0.0,
                stops: vec![
                    notation::model::GradientStop {
                        offset: 0.0,
                        color: Rgb { r: 255, g: 0, b: 0 },
                        line: 1,
                        column: 1,
                    },
                    notation::model::GradientStop {
                        offset: 1.0,
                        color: Rgb { r: 0, g: 0, b: 255 },
                        line: 2,
                        column: 1,
                    },
                ],
            },
            0.0,
            0.0,
            10.0,
            10.0,
        );
        let mut frame = vec![0u8; 10 * 10 * 3];
        draw_box(&mut frame, 10, 10, &b, &[], Path::new(".")).unwrap();
        let (r, _, bl) = pixel(&frame, 10, 5, 0);
        assert!(bl > r, "top should approach blue, got ({r}, {bl})");
        let (r, _, bl) = pixel(&frame, 10, 5, 9);
        assert!(r > bl, "bottom should approach red, got ({r}, {bl})");
    }

    #[test]
    fn radial_gradient_renders_concentric_rings() {
        use notation::model::FillPaint;
        let b = paint_box(
            FillPaint::Radial {
                cx: None,
                cy: None,
                radius: None,
                stops: vec![
                    notation::model::GradientStop {
                        offset: 0.0,
                        color: Rgb {
                            r: 255,
                            g: 255,
                            b: 255,
                        },
                        line: 1,
                        column: 1,
                    },
                    notation::model::GradientStop {
                        offset: 1.0,
                        color: Rgb { r: 0, g: 0, b: 0 },
                        line: 2,
                        column: 1,
                    },
                ],
            },
            0.0,
            0.0,
            40.0,
            40.0,
        );
        let mut frame = vec![0u8; 40 * 40 * 3];
        draw_box(&mut frame, 40, 40, &b, &[], Path::new(".")).unwrap();
        let (r, g, bl) = pixel(&frame, 40, 20, 20);
        assert!(
            r > 240 && g > 240 && bl > 240,
            "center should approach white, got ({r}, {g}, {bl})"
        );
        let (r, g, bl) = pixel(&frame, 40, 39, 39);
        assert!(
            r < 16 && g < 16 && bl < 16,
            "corner should approach black, got ({r}, {g}, {bl})"
        );
        // Concentric: equal distances paint equal colors.
        assert_eq!(pixel(&frame, 40, 25, 20), pixel(&frame, 40, 20, 25));
    }

    #[test]
    fn radial_gradient_honors_declared_center_and_radius() {
        use notation::model::FillPaint;
        let b = paint_box(
            FillPaint::Radial {
                cx: Some(10.5),
                cy: Some(10.5),
                radius: Some(10.0),
                stops: vec![
                    notation::model::GradientStop {
                        offset: 0.0,
                        color: Rgb { r: 255, g: 0, b: 0 },
                        line: 1,
                        column: 1,
                    },
                    notation::model::GradientStop {
                        offset: 1.0,
                        color: Rgb { r: 0, g: 0, b: 255 },
                        line: 2,
                        column: 1,
                    },
                ],
            },
            0.0,
            0.0,
            40.0,
            40.0,
        );
        let mut frame = vec![0u8; 40 * 40 * 3];
        draw_box(&mut frame, 40, 40, &b, &[], Path::new(".")).unwrap();
        assert_eq!(pixel(&frame, 40, 10, 10), (255, 0, 0));
        assert_eq!(pixel(&frame, 40, 15, 10), (128, 0, 128));
        assert_eq!(pixel(&frame, 40, 20, 10), (0, 0, 255));
        // Past the radius the end stop holds.
        assert_eq!(pixel(&frame, 40, 30, 10), (0, 0, 255));
    }

    #[test]
    fn border_ring_stays_solid_over_a_gradient() {
        use notation::model::FillPaint;
        let mut b = paint_box(
            FillPaint::Linear {
                angle: 90.0,
                stops: stops_rgb(),
            },
            0.0,
            0.0,
            10.0,
            10.0,
        );
        b.border_width = 2.0;
        b.border_color = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let mut frame = vec![0u8; 10 * 10 * 3];
        draw_box(&mut frame, 10, 10, &b, &[], Path::new(".")).unwrap();
        assert_eq!(pixel(&frame, 10, 0, 5), (255, 255, 255));
        // Inside the ring the gradient paints, not the border.
        assert_ne!(pixel(&frame, 10, 5, 5), (255, 255, 255));
        // Three-stop ramp at lx 5.5: green-to-blue segment, mostly green.
        let (r, g, bl) = pixel(&frame, 10, 5, 5);
        assert!(
            r == 0 && g > 200 && bl < 64,
            "inner should be greenish, got ({r}, {g}, {bl})"
        );
    }

    fn fill_image_at(dir: &Path, name: &str, w: u32, h: u32, rgba: Vec<u8>) -> DecodedImage {
        std::fs::write(dir.join(name), b"stat-only").unwrap();
        DecodedImage {
            path: dir.join(name),
            width: w,
            height: h,
            rgba,
            scaled: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    fn image_paint(src: &str, fit: Fit, px: &str) -> notation::model::FillPaint {
        notation::model::FillPaint::Image {
            src: src.to_string(),
            fit,
            placement: Placement::parse(px).unwrap(),
        }
    }

    #[test]
    fn image_fill_cover_paints_the_whole_box() {
        let dir =
            std::env::temp_dir().join(format!("framesmith-fill-cover-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let decoded = fill_image_at(&dir, "red.png", 4, 4, vec![255, 0, 0, 255].repeat(16));
        let b = paint_box(
            image_paint("red.png", Fit::Cover, "center"),
            0.0,
            0.0,
            8.0,
            8.0,
        );
        let mut frame = vec![0u8; 8 * 8 * 3];
        draw_box(&mut frame, 8, 8, &b, &[decoded], dir.as_path()).unwrap();
        assert!(frame.chunks_exact(3).all(|p| p == [255, 0, 0]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_fill_contain_letterboxes_with_object_position() {
        let dir =
            std::env::temp_dir().join(format!("framesmith-fill-contain-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Centered: the 4-wide picture sits in the middle of the 8-wide box.
        let decoded = fill_image_at(&dir, "red.png", 4, 4, vec![255, 0, 0, 255].repeat(16));
        let b = paint_box(
            image_paint("red.png", Fit::Contain, "center"),
            0.0,
            0.0,
            8.0,
            4.0,
        );
        let mut frame = vec![0u8; 8 * 8 * 3];
        draw_box(&mut frame, 8, 8, &b, &[decoded], dir.as_path()).unwrap();
        assert_eq!(pixel(&frame, 8, 4, 2), (255, 0, 0));
        assert_eq!(pixel(&frame, 8, 2, 2), (255, 0, 0));
        assert_eq!(pixel(&frame, 8, 0, 2), (0, 0, 0));
        assert_eq!(pixel(&frame, 8, 7, 2), (0, 0, 0));
        // Top-left: the picture hugs the origin.
        let decoded = fill_image_at(&dir, "red.png", 4, 4, vec![255, 0, 0, 255].repeat(16));
        let b = paint_box(
            image_paint("red.png", Fit::Contain, "top-left"),
            0.0,
            0.0,
            8.0,
            4.0,
        );
        let mut frame = vec![0u8; 8 * 8 * 3];
        draw_box(&mut frame, 8, 8, &b, &[decoded], dir.as_path()).unwrap();
        assert_eq!(pixel(&frame, 8, 0, 2), (255, 0, 0));
        assert_eq!(pixel(&frame, 8, 5, 2), (0, 0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_fill_stretch_maps_source_corners_to_box_corners() {
        let dir =
            std::env::temp_dir().join(format!("framesmith-fill-stretch-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Two texels: red left, blue right.
        let decoded = fill_image_at(&dir, "rb.png", 2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]);
        let b = paint_box(
            image_paint("rb.png", Fit::Fill, "center"),
            0.0,
            0.0,
            4.0,
            2.0,
        );
        let mut frame = vec![0u8; 4 * 2 * 3];
        draw_box(&mut frame, 4, 2, &b, &[decoded], dir.as_path()).unwrap();
        assert_eq!(pixel(&frame, 4, 0, 0), (255, 0, 0));
        assert_eq!(pixel(&frame, 4, 3, 1), (0, 0, 255));
        assert_eq!(pixel(&frame, 4, 1, 0), (191, 0, 64));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vanished_image_fill_refuses_naming_the_path() {
        let dir = std::env::temp_dir().join(format!("framesmith-fill-gone-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // No file is written: the source is already gone at draw time.
        let b = paint_box(
            image_paint("gone.png", Fit::Cover, "center"),
            0.0,
            0.0,
            8.0,
            8.0,
        );
        let mut frame = vec![0u8; 8 * 8 * 3];
        let err = draw_box(&mut frame, 8, 8, &b, &[], dir.as_path()).unwrap_err();
        assert!(err.contains("gone.png"), "must name the path: {err}");
        assert!(err.contains("no MP4"), "must leave no partial MP4: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn advanced_layer_path_matches_direct_gradient_bytes() {
        use notation::model::FillPaint;
        let b = paint_box(
            FillPaint::Linear {
                angle: 90.0,
                stops: stops_rgb(),
            },
            4.0,
            4.0,
            20.0,
            12.0,
        );
        let mut direct = vec![0u8; 64 * 48 * 3];
        draw_box(&mut direct, 64, 48, &b, &[], Path::new(".")).unwrap();
        let mut layered = vec![0u8; 64 * 48 * 3];
        draw_box_advanced(
            &mut layered,
            64,
            48,
            &b,
            &[],
            Path::new("."),
            &ResolvedGrade::identity(),
            &[],
            &notation::model::VfxChain::empty(),
            &crate::vfx::VfxMaps::empty(),
        )
        .unwrap();
        assert_eq!(direct, layered);
    }

    #[test]
    fn stop_cost_scales_linearly_without_per_stop_leak() {
        // T003 perf: adding stops must add proportional scan work, with
        // no superlinear leak. Per-sample call/lerp/accumulate costs are
        // identical for every stop count, so raw 10-vs-5 ratios under-
        // read the slope; differencing two equal stop steps (5->10 and
        // 10->15) cancels every fixed cost, and the slope ratio must be
        // 1 for linear scaling (0.25 for a quadratic leak). Each round
        // times every size up and back down, so linear clock drift
        // cancels within the round; the median round sets the verdict,
        // so sibling-test CPU noise cannot tilt one size.
        use notation::model::GradientStop;
        let mk = |n: usize| {
            (0..n)
                .map(|i| GradientStop {
                    offset: i as f64 / (n - 1) as f64,
                    color: Rgb {
                        r: (i * 25 % 256) as u8,
                        g: (i * 57 % 256) as u8,
                        b: (i * 91 % 256) as u8,
                    },
                    line: 1,
                    column: 1,
                })
                .collect::<Vec<_>>()
        };
        let five = mk(5);
        let ten = mk(10);
        let fifteen = mk(15);
        // Long holds: with shared-CPU contention the per-size gap is
        // noise-dominated at small sample counts (medians spiked to
        // negative slopes on CI); 1M samples + 7 rounds keeps the
        // 0.7-1.4 band honest without widening it.
        const N: usize = 1_000_000;
        let ts: Vec<f64> = (0..N).map(|i| (i % 1000) as f64 / 1000.0).collect();
        // Process-CPU clock: shared-runner preemption inflates wall
        // time but never CPU time, so slope verdicts survive noisy
        // CI hosts. Windows has no std process-CPU clock, so it
        // keeps anchored wall (proven passing); the CI render step
        // already runs this package single-threaded.
        #[cfg(unix)]
        let cpu_now = || -> f64 {
            unsafe {
                let mut ts: libc::timespec = std::mem::zeroed();
                libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts);
                ts.tv_sec as f64 + ts.tv_nsec as f64 * 1e-9
            }
        };
        #[cfg(not(unix))]
        let cpu_now = {
            let anchor = std::time::Instant::now();
            move || anchor.elapsed().as_secs_f64()
        };
        let mut acc = 0u64;
        let mut time = |stops: &[GradientStop]| {
            acc = 0;
            let start = cpu_now();
            for &t in &ts {
                let (r, g, b) = sample_gradient(stops, t);
                acc += r as u64 + g as u64 + b as u64;
            }
            std::hint::black_box(acc);
            cpu_now() - start
        };
        let mut ratios = Vec::new();
        for _ in 0..7 {
            let (a5, a10, a15) = (time(&five), time(&ten), time(&fifteen));
            let (b15, b10, b5) = (time(&fifteen), time(&ten), time(&five));
            let slope_lo = ((a10 - a5) + (b10 - b5)) / 2.0;
            let slope_hi = ((a15 - a10) + (b15 - b10)) / 2.0;
            ratios.push(slope_hi / slope_lo);
        }
        ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let ratio = ratios[3];
        assert!(
            (0.7..=1.4).contains(&ratio),
            "stop cost must scale linearly (slope ratio ~1, quadratic would read ~0.25), got median {ratio:.3} from {ratios:.3?}"
        );
    }
}
