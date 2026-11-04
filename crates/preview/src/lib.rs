//! Preview frame service (Sprint 14, T001 / CAP-001).
//!
//! A [`PreviewSession`] opens a project folder exactly like `check` and
//! `snapshot` validate it, prepares the render pipeline once, and then
//! serves any frame index on demand: each served frame draws through the
//! same pipeline as a full render, so served bytes equal the same
//! full-render frame byte-identically. Served frames cache as exact PNG
//! bytes — first in memory (bounded LRU), then spilled to a disk cache
//! that lives outside the project directory — so repeats never redraw.
//! A broken project refuses before anything starts, naming the cause
//! with file, line, and fix; an unrunnable frame mid-session reports
//! its cause while the last good frame stays available.
//!
//! Later preview slices layer on this core: latency bars (T002) read
//! [`CacheSource`], transport and the page (T003) drive [`Transport`]
//! and read [`PreviewFrame`] plus [`PreviewSession::last_good`], audio
//! (T004) and reload (T005) reuse the preparation, and the daemon
//! commands (T006) own the server around sessions.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod audio;
pub mod daemon;
mod latency;
mod reload;
pub mod server;
mod transport;

pub use audio::{
    AudioPlay, GainError, GainErrorKind, MixError, MixErrorKind, MixInfo, PlayAudioError,
    PlayAudioErrorKind, PlaybackGain, MIX_CHANNELS, MIX_RATE,
};
pub use latency::{
    ActionLatency, LatencyBars, LatencySample, LatencyTracker, SessionLatencyReport, TransportAction,
    Warmth, JUMP_BAR_MS, SCRUB_BAR_MS, STEP_BAR_MS,
};
pub use reload::{ReloadReport, RELOAD_BUDGET, RELOAD_BUDGET_MS};
pub use transport::{
    Fps, Readout, Transport, TransportControl, TransportError, TransportErrorKind,
};

/// Default bound on hot frames held in memory (Sprint 14, T001):
/// repeats hit memory, older frames fall through to the outside-project
/// disk cache, so long timelines never grow the session without bound.
pub const DEFAULT_MEMORY_FRAMES: usize = 64;

/// Where one served frame's bytes came from (Sprint 14, T001): freshly
/// drawn through the render pipeline, or served from the memory or disk
/// layer of the exact-frame cache. Latency reports (T002) label warm
/// versus cold from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheSource {
    /// Drawn this request through the render pipeline.
    Rendered,
    /// Served from the in-memory exact-byte cache.
    Memory,
    /// Served from the outside-project disk cache.
    Disk,
}

/// One served preview frame (Sprint 14, T001): the frame index, its
/// canvas size, the exact PNG bytes, and where the bytes came from.
#[derive(Debug, Clone)]
pub struct PreviewFrame {
    /// Output frame index (frame N of the composition timeline).
    pub index: u64,
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Exact PNG bytes (same encoder as `snapshot`, so the same frame
    /// of a full render encodes to identical bytes).
    pub png: Vec<u8>,
    /// Which layer served these bytes.
    pub served_from: CacheSource,
}

/// Cache counters for one session (Sprint 14, T001): every served frame
/// lands in exactly one bucket, so tests and latency reports can tell
/// warm hits from fresh draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Frames drawn through the render pipeline this session.
    pub rendered: u64,
    /// Frames served from the in-memory exact-byte cache.
    pub memory_hits: u64,
    /// Frames served from the outside-project disk cache.
    pub disk_hits: u64,
}

/// One validation diagnostic carried on an open refusal (Sprint 14,
/// T001): file, line, column, code, message, and fix — the same shape
/// `check` prints, so the maker fixes the same problem the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewDiagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub code: String,
    pub message: String,
    pub fix: String,
}

impl From<&validate::validate::Diagnostic> for PreviewDiagnostic {
    fn from(d: &validate::validate::Diagnostic) -> Self {
        Self {
            file: d.file.clone(),
            line: d.line,
            column: d.column,
            code: d.code.to_string(),
            message: d.message.clone(),
            fix: d.fix.clone(),
        }
    }
}

/// Why a preview session refused to open (Sprint 14, T001).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalKind {
    /// The project is invalid: the refusal carries the diagnostics.
    Invalid,
    /// The project uses later-sprint elements: names them.
    Unsupported,
    /// The named composition is not in the project.
    UnknownComposition,
    /// No FFmpeg binary is available to prepare media.
    NoFfmpeg,
    /// The project validated but preparation or the backend failed.
    PrepareFailed,
}

/// A refused preview open (Sprint 14, T001): the kind, a human message
/// naming the cause with its fix, and the diagnostics when the project
/// itself is at fault. Nothing starts on refusal: no cache directory,
/// no preparation, no server.
#[derive(Debug, Clone)]
pub struct PreviewRefusal {
    pub kind: RefusalKind,
    pub message: String,
    pub diagnostics: Vec<PreviewDiagnostic>,
}

impl std::fmt::Display for PreviewRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Why one frame request failed while the session stays open (Sprint
/// 14, T001): the cause is named and the last good frame stays
/// available through [`PreviewSession::last_good`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameErrorKind {
    /// The index is outside `0..frame_count`.
    OutOfRange,
    /// The frame would not draw (a source vanished mid-session, the
    /// PNG would not encode, ...): names the underlying cause.
    RenderFailed,
}

/// One failed frame request (Sprint 14, T001).
#[derive(Debug, Clone)]
pub struct PreviewFrameError {
    pub kind: FrameErrorKind,
    pub message: String,
}

impl std::fmt::Display for PreviewFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Options for opening a preview session (Sprint 14, T001).
#[derive(Debug, Clone)]
pub struct PreviewOptions {
    /// Composition to preview: the render root by default, looked up by
    /// name or id when set.
    pub composition: Option<String>,
    /// Drawing backend. `Auto` (the default) resolves through the GPU
    /// capability probe exactly like `snapshot`.
    pub backend: render::render::Backend,
    /// Root for the outside-project disk cache. Defaults to a fresh
    /// per-session directory under the OS temp dir; a set path must be
    /// outside the project directory (it is created when missing).
    pub cache_dir: Option<PathBuf>,
    /// Bound on hot frames held in memory (LRU). At least 1; older
    /// frames stay on disk, so lowering this only costs redraws, never
    /// correctness.
    pub memory_frames: usize,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            composition: None,
            backend: render::render::Backend::Auto,
            cache_dir: None,
            memory_frames: DEFAULT_MEMORY_FRAMES,
        }
    }
}

/// Serializes preparations process-wide (Sprint 14, T001): preparation
/// points the footage cache outside the project through a
/// process-global environment root, so two sessions preparing at once
/// must not interleave the switch. Drawing needs no lock — frame N
/// depends only on N at any worker count.
static PREPARE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Next per-session cache directory sequence (process-unique together
/// with the pid, so concurrent sessions never share a disk cache).
static SESSION_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// First `ffmpeg` binary on `PATH` (Sprint 14, T001): preparation
/// decodes media through the host FFmpeg, so a session without one
/// refuses before starting. Detection never downloads or installs.
fn tool_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Whole milliseconds in a reload timing (Sprint 14, T005): the
/// page shows this against the 500 ms bar.
fn elapsed_as_ms(elapsed: Duration) -> u128 {
    elapsed.as_millis()
}

/// Restate a render-pipeline error as a preview refusal (Sprint 14,
/// T001): the same cause, but naming `preview` and the served frame
/// instead of the render output.
pub(crate) fn previewize(mut e: String) -> String {
    for (from, to) in [
        ("render refused:", "preview refused:"),
        ("render failed:", "preview failed:"),
        ("snapshot refused:", "preview refused:"),
        ("snapshot failed:", "preview failed:"),
        ("; no MP4 was written", "; no preview frame was served"),
        ("; no PNG was written", "; no preview frame was served"),
    ] {
        e = e.replace(from, to);
    }
    e
}

/// An exact-frame preview session over one validated project (Sprint
/// 14, T001). Owns its preparation plus the two-layer exact-byte
/// cache; the disk layer lives outside the project directory and is
/// removed when the session drops.
pub struct PreviewSession {
    folder: PathBuf,
    /// Composition selector from [`PreviewOptions::composition`]: `None`
    /// follows the render root across reloads, `Some` keeps previewing
    /// the same named composition (Sprint 14, T005).
    wanted: Option<String>,
    composition_name: String,
    composition: notation::model::Composition,
    all_compositions: Vec<notation::model::Composition>,
    ffmpeg_bin: PathBuf,
    frames: u64,
    canvas: (u32, u32),
    fps: notation::model::Fps,
    duration_secs: f64,
    backend: render::render::Backend,
    prep: render::render::Prepared,
    mix: Option<Result<Option<Vec<i16>>, MixError>>,
    memory_cap: usize,
    memory: HashMap<u64, Vec<u8>>,
    memory_order: VecDeque<u64>,
    disk_dir: PathBuf,
    stats: CacheStats,
    last_good: Option<(u64, Vec<u8>)>,
    latency: LatencyTracker,
}

impl PreviewSession {
    /// Open a preview session (Sprint 14, T001): validate exactly like
    /// `check`, refuse broken projects before anything starts (naming
    /// file, line, and fix), then prepare the render pipeline once with
    /// the footage cache redirected outside the project. The project
    /// directory is never written to.
    pub fn open(folder: &Path, options: PreviewOptions) -> Result<Self, PreviewRefusal> {
        if !folder.is_dir() {
            return Err(PreviewRefusal {
                kind: RefusalKind::Invalid,
                message: format!(
                    "preview refused: {} has no project directory; pass an existing project folder",
                    folder.display()
                ),
                diagnostics: Vec::new(),
            });
        }
        let verdict = validate::validate::validate_folder_with_variables_strict_themed(
            folder,
            &validate::validate::VariableOverrides::new(),
            false,
            None,
        );
        match verdict.status {
            validate::validate::Status::Invalid => {
                let mut message = format!(
                    "preview refused: {} is invalid; fix the problems `check` reports and re-run. No preview was started.",
                    folder.display()
                );
                let diagnostics: Vec<PreviewDiagnostic> =
                    verdict.diagnostics.iter().map(PreviewDiagnostic::from).collect();
                for d in &diagnostics {
                    message.push_str(&format!("\n{}:{}:{} {} {}", d.file, d.line, d.column, d.code, d.message));
                }
                return Err(PreviewRefusal {
                    kind: RefusalKind::Invalid,
                    message,
                    diagnostics,
                });
            }
            validate::validate::Status::Unsupported => {
                let names: Vec<String> = verdict
                    .diagnostics
                    .iter()
                    .map(|d| d.message.clone())
                    .collect();
                return Err(PreviewRefusal {
                    kind: RefusalKind::Unsupported,
                    message: format!(
                        "preview refused: {} uses later-sprint element(s): {}. No preview was started.",
                        folder.display(),
                        names.join("; ")
                    ),
                    diagnostics: verdict.diagnostics.iter().map(PreviewDiagnostic::from).collect(),
                });
            }
            validate::validate::Status::Valid => {}
        }
        let comp = match &options.composition {
            None => verdict
                .composition
                .clone()
                .expect("valid verdict carries a composition"),
            Some(want) => verdict
                .compositions
                .iter()
                .find(|c| c.name == *want || c.id.as_deref() == Some(want))
                .cloned()
                .ok_or_else(|| {
                    let mut available: Vec<String> = verdict
                        .compositions
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    available.sort();
                    PreviewRefusal {
                        kind: RefusalKind::UnknownComposition,
                        message: format!(
                            "preview refused: unknown composition {want:?}: the project holds {}; pass one of those names. No preview was started.",
                            available
                                .iter()
                                .map(|n| format!("{n:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        diagnostics: Vec::new(),
                    }
                })?,
        };
        let ffmpeg_bin = tool_on_path("ffmpeg").ok_or_else(|| PreviewRefusal {
            kind: RefusalKind::NoFfmpeg,
            message: "preview refused: no FFmpeg binary was found on PATH; install FFmpeg and re-run. No preview was started.".to_string(),
            diagnostics: Vec::new(),
        })?;
        let effective_backend = render::render::resolve_backend(options.backend);
        if options.backend == render::render::Backend::Gpu && !render::gpu::gpu_available() {
            return Err(PreviewRefusal {
                kind: RefusalKind::PrepareFailed,
                message: "preview refused: no GPU renderer is available on this machine (capability probe found none); use the cpu backend or auto instead. No preview was started.".to_string(),
                diagnostics: Vec::new(),
            });
        }
        // The disk cache lives outside the project directory: a fresh
        // per-session directory under the OS temp dir by default, or
        // under the caller-provided root. Created only after validation
        // passed, so a broken project leaves nothing behind.
        let disk_dir = match &options.cache_dir {
            Some(root) => {
                let seq = SESSION_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                root.join(format!("preview-{}-{seq}", std::process::id()))
            }
            None => {
                let seq = SESSION_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::env::temp_dir().join(format!("framesmith-preview-{}-{seq}", std::process::id()))
            }
        };
        if let Some(root) = &options.cache_dir {
            if root == folder || root.starts_with(folder) {
                return Err(PreviewRefusal {
                    kind: RefusalKind::PrepareFailed,
                    message: format!(
                        "preview refused: the cache directory {} sits inside the project {}; preview caches live outside the project. No preview was started.",
                        root.display(),
                        folder.display()
                    ),
                    diagnostics: Vec::new(),
                });
            }
        }
        if let Err(e) = std::fs::create_dir_all(&disk_dir) {
            return Err(PreviewRefusal {
                kind: RefusalKind::PrepareFailed,
                message: format!(
                    "preview refused: the outside-project cache {} cannot be created: {e}. No preview was started.",
                    disk_dir.display()
                ),
                diagnostics: Vec::new(),
            });
        }
        // Prepare under the process-wide lock with the footage cache
        // redirected into the session disk dir, then restore the
        // environment: preparation is the only stage that touches the
        // footage cache, and the previous value (if any) belongs to the
        // caller.
        let prep = {
            let _guard = PREPARE_LOCK.lock().unwrap();
            let previous = std::env::var_os("FRAMESMITH_FOOTAGE_CACHE_DIR");
            std::env::set_var("FRAMESMITH_FOOTAGE_CACHE_DIR", &disk_dir);
            let prepared = render::render::prepare_frames(
                &comp,
                &verdict.compositions,
                folder,
                &ffmpeg_bin,
            );
            match previous {
                Some(v) => std::env::set_var("FRAMESMITH_FOOTAGE_CACHE_DIR", v),
                None => std::env::remove_var("FRAMESMITH_FOOTAGE_CACHE_DIR"),
            }
            prepared
        }
        .map_err(|e| PreviewRefusal {
            kind: RefusalKind::PrepareFailed,
            message: previewize(e),
            diagnostics: Vec::new(),
        })?;
        let frames = prep.frames() as u64;
        let canvas = prep.canvas();
        let fps = comp.fps;
        let duration_secs = comp.duration;
        Ok(Self {
            folder: folder.to_path_buf(),
            wanted: options.composition.clone(),
            composition_name: comp.name.clone(),
            composition: comp.clone(),
            all_compositions: verdict.compositions.clone(),
            ffmpeg_bin: ffmpeg_bin.clone(),
            frames,
            canvas,
            fps,
            duration_secs,
            backend: effective_backend,
            prep,
            mix: None,
            memory_cap: options.memory_frames.max(1),
            memory: HashMap::new(),
            memory_order: VecDeque::new(),
            disk_dir,
            stats: CacheStats::default(),
            last_good: None,
            latency: LatencyTracker::new(),
        })
    }

    /// Project folder this session previews.
    pub fn project_folder(&self) -> &Path {
        &self.folder
    }

    /// Name of the previewed composition.
    pub fn composition_name(&self) -> &str {
        &self.composition_name
    }

    /// Output frame count of the previewed composition.
    pub fn frame_count(&self) -> u64 {
        self.frames
    }

    /// Canvas dimensions `(width, height)` of the previewed composition.
    pub fn canvas(&self) -> (u32, u32) {
        self.canvas
    }

    /// Frames per second behind the transport time readout (Sprint 14,
    /// T003): the validated composition rate.
    pub fn fps(&self) -> notation::model::Fps {
        self.fps
    }

    /// Composition length in seconds behind the readout total.
    pub fn duration_secs(&self) -> f64 {
        self.duration_secs
    }

    /// Timeline transport over this session (Sprint 14, T003 / CAP-003):
    /// parked at frame 0, paused, unlooped, at 1x speed. The page moves
    /// it with the transport controls, serves the frame at
    /// [`Transport::position`] through
    /// [`PreviewSession::frame_png_timed`], and renders
    /// [`Transport::readout`] as the time/frame display.
    pub fn transport(&self) -> Transport {
        Transport::new(self.frames, self.fps, self.duration_secs)
    }

    /// Effective drawing backend (resolved through the capability probe).
    pub fn backend(&self) -> render::render::Backend {
        self.backend
    }

    /// Cache counters for this session.
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Outside-project disk cache directory of this session.
    pub fn cache_dir(&self) -> &Path {
        &self.disk_dir
    }

    /// Last successfully served frame, if any (Sprint 14, T001): a
    /// failed frame keeps this intact, so the page never shows a
    /// partial frame.
    pub fn last_good(&self) -> Option<(u64, &[u8])> {
        self.last_good.as_ref().map(|(i, b)| (*i, b.as_slice()))
    }

    /// Serve one frame for a transport gesture, timing the request
    /// into the session latency record (Sprint 14, T002): the elapsed
    /// time files under `action` as warm when the frame came from
    /// either cache layer and cold when it drew fresh. A refused frame
    /// (out of range, unrunnable) returns its error without filing a
    /// sample, so failures never pollute the medians.
    pub fn frame_png_timed(
        &mut self,
        action: TransportAction,
        index: u64,
    ) -> Result<(PreviewFrame, std::time::Duration), PreviewFrameError> {
        let start = Instant::now();
        let frame = self.frame_png(index)?;
        let elapsed = start.elapsed();
        self.latency.record_source(action, frame.served_from, elapsed);
        Ok((frame, elapsed))
    }

    /// Every latency sample filed so far, in filing order.
    pub fn latency_samples(&self) -> &[LatencySample] {
        self.latency.samples()
    }

    /// Per-session latency medians labeled warm versus cold under the
    /// specification bars (Sprint 14, T002 / CAP-002, REC-004).
    pub fn latency_report(&self) -> SessionLatencyReport {
        self.latency.report()
    }

    /// Per-session latency medians under custom bars (same samples and
    /// warm/cold labels, different thresholds).
    pub fn latency_report_with(&self, bars: &LatencyBars) -> SessionLatencyReport {
        self.latency.report_with(bars)
    }

    /// Serve the render mix as interleaved stereo s16 PCM at 48 kHz
    /// (Sprint 14, T004 / CAP-004): the same computation a full render
    /// mixes ([`render::render::build_mix_preview`]), so heard bytes
    /// equal the rendered mix byte-identically. Built once and cached;
    /// a soundless project serves `None` — video plays silently with
    /// no error and no fake track — while an undecodable source
    /// reports its cause naming the file and blocks play.
    pub fn mix_pcm(&mut self) -> Result<Option<&[i16]>, MixError> {
        if self.mix.is_none() {
            let built = render::render::build_mix_preview(
                &self.composition,
                &self.all_compositions,
                &self.folder,
                &self.ffmpeg_bin,
            )
            .map_err(MixError::from_preview_error);
            self.mix = Some(built);
        }
        match self.mix.as_ref().expect("mix was just built") {
            Ok(None) => Ok(None),
            Ok(Some(pcm)) => Ok(Some(pcm.as_slice())),
            Err(e) => Err(e.clone()),
        }
    }

    /// Facts about the served mix for the page clock (Sprint 14,
    /// T004): `None` when the project carries no audible sound, an
    /// error naming the cause when the mix cannot be decoded.
    pub fn mix_info(&mut self) -> Result<Option<MixInfo>, MixError> {
        let duration_secs = self.duration_secs;
        match self.mix_pcm()? {
            None => Ok(None),
            Some(pcm) => Ok(Some(MixInfo::for_pcm(pcm.len(), duration_secs))),
        }
    }

    /// Serve the render mix as WAV bytes the page audio element plays
    /// directly (Sprint 14, T004): `None` for a soundless project,
    /// naming the cause when the mix cannot be decoded.
    pub fn mix_wav(&mut self) -> Result<Option<Vec<u8>>, MixError> {
        match self.mix_pcm()? {
            None => Ok(None),
            Some(pcm) => Ok(Some(audio::encode_wav(pcm))),
        }
    }

    /// Press play with sound (Sprint 14, T004 / CAP-004, STATE-03):
    /// the mix is checked first, so an undecodable mix blocks play
    /// naming the cause and the playhead never moves; a soundless
    /// project plays video silently with no error. Only when the mix
    /// passes does the transport start advancing.
    pub fn play_with_audio(
        &mut self,
        transport: &mut Transport,
    ) -> Result<AudioPlay, PlayAudioError> {
        let duration_secs = self.duration_secs;
        let play = match self.mix_pcm() {
            Ok(None) => AudioPlay::Silent,
            Ok(Some(pcm)) => AudioPlay::Sound(MixInfo::for_pcm(pcm.len(), duration_secs)),
            Err(e) => return Err(PlayAudioError::mix(e)),
        };
        transport.play().map_err(PlayAudioError::transport)?;
        Ok(play)
    }

    /// Timeline frame showing at `mix_secs` seconds of mix time
    /// (Sprint 14, T004): the mix clock owns the playhead — the page
    /// maps each audio timeupdate through here. Clamped to the
    /// timeline; times past the end park on the last frame.
    pub fn frame_for_mix_time(&self, mix_secs: f64) -> u64 {
        MixInfo::frame_for_mix_time(self.fps, self.frames, mix_secs)
    }

    /// Drop every cached frame without touching the preparation (Sprint
    /// 14, T005): after invalidation the next request redraws through
    /// the pipeline, so a mid-reload request computes fresh instead of
    /// serving stale.
    pub fn invalidate(&mut self) {
        self.mix = None;
        self.memory.clear();
        self.memory_order.clear();
        if let Ok(entries) = std::fs::read_dir(&self.disk_dir) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Hot-reload the project folder (Sprint 14, T005 / CAP-005,
    /// STATE-04, REC-002): re-validate exactly like `check`, then
    /// re-prepare the render pipeline. A valid edit applies the new
    /// look — every cached frame (both layers), the cached mix, and the
    /// stale last-good frame are dropped, so the next request draws the
    /// new look fresh instead of serving stale — and the report carries
    /// the reload time against the 500 ms bar. A broken edit keeps
    /// everything (preparation, caches, last good, mix) and the report
    /// lists the diagnostics for the page. Either way nothing here
    /// moves any playhead: the session never owns one.
    pub fn reload(&mut self) -> ReloadReport {
        let start = Instant::now();
        let broken = |session: &Self,
                      diagnostics: Vec<PreviewDiagnostic>,
                      message: String|
         -> ReloadReport {
            ReloadReport {
                applied: false,
                elapsed: start.elapsed(),
                message,
                diagnostics,
                frame_count: session.frames,
                canvas: session.canvas,
                playhead: None,
            }
        };
        if !self.folder.is_dir() {
            return broken(
                self,
                Vec::new(),
                format!(
                    "preview reload kept the last good look: {} no longer holds a project directory; fix it and save again. Last good frames keep showing.",
                    self.folder.display()
                ),
            );
        }
        let verdict = validate::validate::validate_folder_with_variables_strict_themed(
            &self.folder,
            &validate::validate::VariableOverrides::new(),
            false,
            None,
        );
        match verdict.status {
            validate::validate::Status::Invalid => {
                let diagnostics: Vec<PreviewDiagnostic> =
                    verdict.diagnostics.iter().map(PreviewDiagnostic::from).collect();
                let mut message = format!(
                    "preview reload kept the last good look: {} is invalid; fix the problems and save again. Last good frames keep showing.",
                    self.folder.display()
                );
                for d in &diagnostics {
                    message.push_str(&format!(
                        "\n{}:{}:{} {} {} (fix: {})",
                        d.file, d.line, d.column, d.code, d.message, d.fix
                    ));
                }
                return broken(self, diagnostics, message);
            }
            validate::validate::Status::Unsupported => {
                let diagnostics: Vec<PreviewDiagnostic> =
                    verdict.diagnostics.iter().map(PreviewDiagnostic::from).collect();
                let names: Vec<String> =
                    diagnostics.iter().map(|d| d.message.clone()).collect();
                return broken(
                    self,
                    diagnostics,
                    format!(
                        "preview reload kept the last good look: {} uses later-sprint element(s): {}. Last good frames keep showing.",
                        self.folder.display(),
                        names.join("; ")
                    ),
                );
            }
            validate::validate::Status::Valid => {}
        }
        let comp = match &self.wanted {
            None => verdict.composition.clone().expect("valid verdict carries a composition"),
            Some(want) => match verdict
                .compositions
                .iter()
                .find(|c| c.name == *want || c.id.as_deref() == Some(want))
                .cloned()
            {
                Some(comp) => comp,
                None => {
                    let mut available: Vec<String> = verdict
                        .compositions
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    available.sort();
                    return broken(
                        self,
                        Vec::new(),
                        format!(
                            "preview reload kept the last good look: unknown composition {want:?}: the project holds {}. Last good frames keep showing.",
                            available
                                .iter()
                                .map(|n| format!("{n:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    );
                }
            },
        };
        // Re-prepare before dropping anything: a prepare failure keeps
        // the last good look exactly like a broken edit.
        let prep = {
            let _guard = PREPARE_LOCK.lock().unwrap();
            let previous = std::env::var_os("FRAMESMITH_FOOTAGE_CACHE_DIR");
            std::env::set_var("FRAMESMITH_FOOTAGE_CACHE_DIR", &self.disk_dir);
            let prepared = render::render::prepare_frames(
                &comp,
                &verdict.compositions,
                &self.folder,
                &self.ffmpeg_bin,
            );
            match previous {
                Some(v) => std::env::set_var("FRAMESMITH_FOOTAGE_CACHE_DIR", v),
                None => std::env::remove_var("FRAMESMITH_FOOTAGE_CACHE_DIR"),
            }
            prepared
        };
        let prep = match prep {
            Ok(prep) => prep,
            Err(e) => {
                return broken(
                    self,
                    Vec::new(),
                    format!(
                        "preview reload kept the last good look: {}; fix it and save again. Last good frames keep showing.",
                        previewize(e)
                    ),
                )
            }
        };
        // The new look is live: swap the preparation, then drop every
        // cached byte of the old look — memory, served-frame disk
        // entries (footage decoding entries stay: the new preparation
        // just refreshed them), the cached mix, the stale last-good
        // frame, and the cache counters — so the next request draws the
        // new look fresh, never stale.
        self.composition_name = comp.name.clone();
        self.composition = comp.clone();
        self.all_compositions = verdict.compositions.clone();
        self.prep = prep;
        self.frames = self.prep.frames() as u64;
        self.canvas = self.prep.canvas();
        self.fps = comp.fps;
        self.duration_secs = comp.duration;
        self.memory.clear();
        self.memory_order.clear();
        self.clear_frame_entries();
        self.mix = None;
        self.last_good = None;
        self.stats = CacheStats::default();
        let elapsed = start.elapsed();
        ReloadReport {
            applied: true,
            elapsed,
            message: format!(
                "preview reloaded composition {:?} in {}ms: the new look is live, playhead kept ({} frame(s), {}x{}).",
                self.composition_name,
                elapsed_as_ms(elapsed),
                self.frames,
                self.canvas.0,
                self.canvas.1
            ),
            diagnostics: Vec::new(),
            frame_count: self.frames,
            canvas: self.canvas,
            playhead: None,
        }
    }

    /// Hot-reload while keeping the page playhead (Sprint 14, T005 /
    /// CAP-005): [`PreviewSession::reload`] plus the playhead rule. On
    /// success the transport rebases onto the new timeline — same
    /// position, clamped into the new frame count when the edit
    /// shortened the timeline, same playing/loop/speed state — and the
    /// report carries the kept position. On failure the transport is
    /// left untouched (never reset) and the report carries the kept
    /// position alongside the diagnostics.
    pub fn reload_keep_playhead(&mut self, transport: &mut Transport) -> ReloadReport {
        let before = transport.position();
        let mut report = self.reload();
        if report.applied {
            report.playhead = Some(transport.rebase(self.frames, self.fps, self.duration_secs));
        } else {
            report.playhead = Some(before);
        }
        report
    }

    /// Drop served-frame disk entries only, keeping footage decoding
    /// entries (Sprint 14, T005): the new preparation just refreshed
    /// them, and the next draw must decode from them instead of
    /// re-fetching media.
    fn clear_frame_entries(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.disk_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.starts_with("frame-") && (name.ends_with(".png") || name.ends_with(".tmp"))
                {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    /// Disk entry for one frame index.
    fn disk_path(&self, index: u64) -> PathBuf {
        self.disk_dir.join(format!("frame-{index:06}.png"))
    }

    /// Record one memory entry under the LRU bound, evicting the
    /// oldest. Eviction only drops the hot copy: the disk layer keeps
    /// the exact bytes, so the frame stays a cache hit.
    fn remember(&mut self, index: u64, png: Vec<u8>) {
        if !self.memory.contains_key(&index) {
            self.memory_order.push_back(index);
            while self.memory_order.len() > self.memory_cap {
                if let Some(old) = self.memory_order.pop_front() {
                    self.memory.remove(&old);
                }
            }
        }
        self.memory.insert(index, png);
    }

    /// Serve one frame as exact PNG bytes (Sprint 14, T001): memory hit,
    /// else disk hit, else draw through the same pipeline as a full
    /// render and cache the exact bytes in both layers. An out-of-range
    /// index refuses naming the valid window; an unrunnable frame
    /// reports its cause and leaves the last good frame untouched.
    pub fn frame_png(&mut self, index: u64) -> Result<PreviewFrame, PreviewFrameError> {
        if index >= self.frames {
            return Err(PreviewFrameError {
                kind: FrameErrorKind::OutOfRange,
                message: format!(
                    "preview refused: frame {index} is outside composition {:?} ({} frame(s)): pass 0..{}. No preview frame was served.",
                    self.composition_name, self.frames, self.frames
                ),
            });
        }
        if let Some(bytes) = self.memory.get(&index).cloned() {
            self.stats.memory_hits += 1;
            return Ok(PreviewFrame {
                index,
                width: self.canvas.0,
                height: self.canvas.1,
                png: bytes,
                served_from: CacheSource::Memory,
            });
        }
        let disk_path = self.disk_path(index);
        if let Ok(bytes) = std::fs::read(&disk_path) {
            // A present entry is exact by construction: the directory
            // is fresh per session and only this session writes it.
            self.stats.disk_hits += 1;
            self.remember(index, bytes.clone());
            return Ok(PreviewFrame {
                index,
                width: self.canvas.0,
                height: self.canvas.1,
                png: bytes,
                served_from: CacheSource::Disk,
            });
        }
        let drawn = render::render::draw_batch_parallel(&self.prep, self.backend, &[index]).map_err(|e| {
            PreviewFrameError {
                kind: FrameErrorKind::RenderFailed,
                message: format!("{}; showing last good frame {}", previewize(e), self.last_good_desc()),
            }
        })?;
        let (_, rgb) = drawn
            .iter()
            .find(|(k, _)| *k == index)
            .expect("drawn frame was requested");
        let png = render::outputs::encode_png_rgb8(self.canvas.0, self.canvas.1, rgb).map_err(|e| {
            PreviewFrameError {
                kind: FrameErrorKind::RenderFailed,
                message: format!("{}; showing last good frame {}", previewize(e), self.last_good_desc()),
            }
        })?;
        // Atomic store: encode fully, then rename into place, so a
        // crash never leaves a partial entry behind.
        let tmp = disk_path.with_extension("png.tmp");
        if std::fs::write(&tmp, &png).is_ok() {
            let _ = std::fs::rename(&tmp, &disk_path);
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
        self.stats.rendered += 1;
        self.remember(index, png.clone());
        self.last_good = Some((index, png.clone()));
        Ok(PreviewFrame {
            index,
            width: self.canvas.0,
            height: self.canvas.1,
            png,
            served_from: CacheSource::Rendered,
        })
    }

    /// Human description of the last good frame for failure messages.
    fn last_good_desc(&self) -> String {
        match &self.last_good {
            Some((i, _)) => format!("frame {i}"),
            None => "none yet".to_string(),
        }
    }
}

impl std::fmt::Debug for PreviewSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewSession")
            .field("folder", &self.folder)
            .field("composition_name", &self.composition_name)
            .field("frames", &self.frames)
            .field("canvas", &self.canvas)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl Drop for PreviewSession {
    /// Best-effort cleanup of the outside-project disk cache (Sprint
    /// 14, T001): stopping the session removes its cache; the project
    /// directory was never written to, so there is nothing to undo
    /// there.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.disk_dir);
    }
}
