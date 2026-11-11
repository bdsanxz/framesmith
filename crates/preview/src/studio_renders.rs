//! Renders panel and queue (Sprint 18, T006 / CAP-006, CAP-009, J-05,
//! STATE-03, STATE-04, INT-003, REC-002, REC-003, REC-005).
//!
//! The maker queues renders with format, quality, frame rate,
//! resolution, and GPU encoding settings; jobs run in queue order with
//! measured progress and an ETA labeled an estimate; cancel stops the
//! run and removes the partial output reporting the stage reached;
//! download delivers the exact rendered file; delete removes the
//! output saying what was removed (refusing running jobs);
//! hide-finished clears completed rows only; a missing FFmpeg shows a
//! notice instead of queueing; a failed job names its stage while the
//! queue advances; and GIF export joins the queue at up to 30 fps with
//! no audio, the chosen loop count in the file, refusing over-cap
//! timelines by name.
//!
//! Read boundary: the queue file is the one truth (written by the
//! Compositions panel's render-one and by this panel, executed only
//! here). Listing reads the file with live progress overlaid; a
//! corrupt or version-wrong file refuses naming the file, never
//! guessed or clobbered. Renders replay the validated project through
//! the same pipeline as the headless `render` verb with the job's
//! settings, so queue outputs match their headless counterparts and
//! default renders stay byte-identical.
//!
//! Write boundary: the only project writes are the hidden queue file
//! plus the hidden renders directory beside it (the queued outputs,
//! outside the project sources exactly as CLI renders land outside
//! them) — never the composition sources, never outside the project
//! except the outside-project preview caches the preview sessions
//! already own. Queue rows are append-only records; execution only
//! updates states.
//!
//! Refusal boundary: a bad body, an unknown composition, an invalid
//! project, a missing FFmpeg, an over-cap GIF timeline, a delete of a
//! running job, and an unknown job all refuse naming the cause; every
//! refusal stages nothing, queues nothing, and records nothing.
//!
//! Style boundary: panel CSS uses hyphenated CSS vocabulary
//! (`background-color`, `border-radius`, `object-fit`,
//! `font-variant-numeric`) and carries no `font-family`, so the
//! fail-closed font try-order passes through untouched.
//!
//! Concurrency: the server stays single-threaded; one worker thread
//! per server runs queued jobs in order while the handlers answer
//! reads. The pipeline polls one process-wide cancel flag, so the
//! queue runs one job at a time per process and clears the flag before
//! and after every job.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::studio::json_escape;
use crate::studio_compositions::{
    render_queue_path_for, renders_dir_for, RenderJob, RENDER_QUEUE_VERSION,
};

/// Frame rate the queue accepts (Sprint 18, T006): the panel offers
/// exactly these three; anything else refuses naming them.
pub const QUEUE_FPS: [u32; 3] = [24, 30, 60];

/// GIF frame-rate cap (Sprint 18, T006 / CAP-009): a GIF job encodes
/// at up to 30 fps, clamping 60 down with a note.
pub const GIF_MAX_FPS: u32 = 30;

/// GIF frame cap (Sprint 18, T006 / CAP-009): 30 fps for 30 seconds.
/// A longer timeline refuses naming the cap and the measured length.
pub const GIF_MAX_FRAMES: u64 = 900;

/// Longest loop count a GIF job records (`0` means forever).
pub const GIF_MAX_LOOP: u32 = 65535;

/// Terminal job states (Sprint 18, T006): hide-finished clears exactly
/// these rows, leaving queued and running jobs untouched.
const TERMINAL_STATES: [&str; 3] = ["succeeded", "failed", "cancelled"];

/// Process-wide queue-file guard (Sprint 18, T006): every queue read
/// and write below holds it, so the worker thread and the request
/// handlers never interleave a read-modify-write. Renders themselves
/// run unlocked, so progress polls never block on a running job.
static QUEUE_LOCK: Mutex<()> = Mutex::new(());

/// Why a renders action refused.
fn refused_renders(message: String) -> String {
    format!("studio refused: {message}")
}

/// Epoch milliseconds now (Sprint 18, T006): ETA math rides this
/// clock; a clock failure reads as zero, never panics.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// One enqueue request (Sprint 18, T006): who queues, what renders,
/// and how. `composition` names by `name` or `id`; `None` renders the
/// render root. `fps` overrides the composition rate; `None` keeps
/// it. `loop_count` rides GIF jobs only (`0` means forever).
#[derive(Debug, Clone)]
pub struct EnqueueRequest {
    /// Actor that queues the job.
    pub actor: String,
    /// Composition to render, by `name` or `id`; the root when omitted.
    pub composition: Option<String>,
    /// Queue format (`mp4`, `mov`, `webm`, or `gif`).
    pub format: String,
    /// Quality preset (`draft`, `standard`, or `high`).
    pub quality: String,
    /// Frame rate override (24, 30, or 60); the composition rate when omitted.
    pub fps: Option<u32>,
    /// Resolution preset (`auto`, `1080p`, or `4k`).
    pub resolution: String,
    /// GPU encoding toggle (hardware when the host provides it,
    /// software with a note otherwise).
    pub gpu: bool,
    /// GIF loop count (`0` means forever).
    pub loop_count: u32,
}

/// Receipt of one queued job (Sprint 18, T006).
#[derive(Debug, Clone)]
pub struct EnqueueReceipt {
    /// Job sequence in the queue.
    pub seq: u64,
    /// Composition the job renders.
    pub composition: String,
    /// Effective queue format.
    pub format: String,
    /// Effective frame rate (GIF-clamped where it applied).
    pub fps: u32,
    /// Planned output path, under the hidden renders directory.
    pub output: String,
}

/// Parse one enqueue body (Sprint 18, T006): every setting validates
/// before anything queues — unknown formats, qualities, frame rates,
/// and resolutions refuse naming the accepted values; a non-GIF loop
/// count refuses naming the GIF-only rule; a bad loop count refuses
/// naming the range. A refused body queues nothing.
pub fn parse_enqueue_body(body: &str) -> Result<EnqueueRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_renders(
            "queueing needs a JSON body (`{\"actor\":\"maker\"}` plus settings); pass the renders form. Nothing was queued."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_renders(format!(
            "the queue body is not JSON ({e}). Pass `{{\"actor\":\"maker\"}}` plus settings. Nothing was queued."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_renders(
            "queueing names no actor; queued jobs record who queued them. Pass a non-empty `actor`. Nothing was queued.".to_string(),
        ));
    }
    let composition = value
        .get("composition")
        .and_then(|s| s.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let format = value
        .get("format")
        .and_then(|s| s.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "mp4".to_string());
    if render::outputs::OutputFormat::parse(&format).is_none()
        || !matches!(format.as_str(), "mp4" | "mov" | "webm" | "gif")
    {
        return Err(refused_renders(format!(
            "unknown queue format {format:?}: expected mp4, mov, webm, or gif; pass one of those to `format`. Nothing was queued."
        )));
    }
    let quality = value
        .get("quality")
        .and_then(|s| s.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "standard".to_string());
    if render::render::Preset::parse(&quality).is_none() {
        return Err(refused_renders(format!(
            "unknown queue quality {quality:?}: expected draft, standard, or high; pass one of those to `quality`. Nothing was queued."
        )));
    }
    let fps = match value.get("fps") {
        None => None,
        Some(v) if v.is_null() => None,
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| {
                refused_renders(format!(
                    "the queue `fps` is not a frame rate ({v}); pass 24, 30, or 60, or omit it for the composition rate. Nothing was queued."
                ))
            })?;
            if !QUEUE_FPS.contains(&(n as u32)) {
                return Err(refused_renders(format!(
                    "unknown queue frame rate {n}: expected 24, 30, or 60; pass one of those to `fps`, or omit it for the composition rate. Nothing was queued."
                )));
            }
            Some(n as u32)
        }
    };
    let resolution = value
        .get("resolution")
        .and_then(|s| s.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "auto".to_string());
    if !matches!(resolution.as_str(), "auto" | "1080p" | "4k") {
        return Err(refused_renders(format!(
            "unknown queue resolution {resolution:?}: expected auto, 1080p, or 4k; pass one of those to `resolution`. Nothing was queued."
        )));
    }
    let gpu = value.get("gpu").and_then(|g| g.as_bool()).unwrap_or(false);
    let loop_count = match value.get("loop_count") {
        None => 0,
        Some(v) if v.is_null() => 0,
        Some(v) => v.as_u64().ok_or_else(|| {
            refused_renders(format!(
                "the queue `loop_count` is not a loop count ({v}); pass 0 (forever) through {GIF_MAX_LOOP}, GIF jobs only. Nothing was queued."
            ))
        })? as u32,
    };
    if loop_count > GIF_MAX_LOOP {
        return Err(refused_renders(format!(
            "the queue `loop_count` {loop_count} is past the GIF loop cap of {GIF_MAX_LOOP}; pass 0 (forever) through {GIF_MAX_LOOP}. Nothing was queued."
        )));
    }
    if loop_count > 0 && format != "gif" {
        return Err(refused_renders(format!(
            "the queue `loop_count` {loop_count} rides GIF jobs only (format {format:?} carries no loop count); drop `loop_count` or queue a GIF. Nothing was queued."
        )));
    }
    Ok(EnqueueRequest {
        actor,
        composition,
        format,
        quality,
        fps,
        resolution,
        gpu,
        loop_count,
    })
}

/// Read the persisted render queue (guard held by the caller):
/// missing file means an empty queue; a corrupt or version-wrong
/// file refuses naming the file, never guessed or clobbered.
fn read_jobs(folder: &Path) -> Result<Vec<RenderJob>, String> {
    let path = render_queue_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(refused_renders(format!(
                "the render queue {} cannot be read ({e}); fix or remove it and retry. Nothing changed.",
                path.display()
            )));
        }
        Ok(raw) => raw,
    };
    let file: RenderQueueFile = serde_json::from_str(&raw).map_err(|e| {
        refused_renders(format!(
            "the render queue {} is not a render queue ({e}); fix or remove it and retry. Nothing changed.",
            path.display()
        ))
    })?;
    if file.version != RENDER_QUEUE_VERSION {
        return Err(refused_renders(format!(
            "the render queue {} carries version {}, this Studio reads version {}; fix or remove it and retry. Nothing changed.",
            path.display(),
            file.version,
            RENDER_QUEUE_VERSION
        )));
    }
    Ok(file.jobs)
}

/// The persisted render queue file (Sprint 18, T001/T006): versioned,
/// one entry per job in queue order.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RenderQueueFile {
    version: u32,
    #[serde(default)]
    jobs: Vec<RenderJob>,
}

/// Write the queue back under the guard: creating the hidden
/// directory when missing, so a refused write leaves the previous
/// queue intact.
fn write_jobs(folder: &Path, jobs: &[RenderJob]) -> Result<(), String> {
    let path = render_queue_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_renders(format!(
                "the hidden queue directory {} cannot be prepared: {e}. Nothing changed.",
                parent.display()
            ))
        })?;
    }
    let raw = serde_json::to_string_pretty(&RenderQueueFile {
        version: RENDER_QUEUE_VERSION,
        jobs: jobs.to_vec(),
    })
    .map_err(|e| {
        refused_renders(format!(
            "the render queue cannot be encoded: {e}. Nothing changed."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_renders(format!(
            "the render queue {} cannot be written: {e}. Nothing changed.",
            path.display()
        ))
    })?;
    Ok(())
}

/// Update one job in place, persisting under the guard (Sprint 18,
/// T006): no-ops when the row is gone, so a delete racing a progress
/// tick never resurrects the row.
fn update_job(
    folder: &Path,
    seq: u64,
    update: impl FnOnce(&mut RenderJob),
) -> Result<bool, String> {
    let _guard = QUEUE_LOCK.lock().map_err(|_| {
        refused_renders("the render queue is busy; retry the update. Nothing changed.".to_string())
    })?;
    let mut jobs = read_jobs(folder)?;
    let Some(job) = jobs.iter_mut().find(|j| j.seq == seq) else {
        return Ok(false);
    };
    update(job);
    write_jobs(folder, &jobs)?;
    Ok(true)
}

/// Resolve the FFmpeg binary one queue job encodes with (Sprint 18,
/// T006): the host binary when it is suitable, else a refusal naming
/// the cause — the queue never self-downloads and never queues blind.
/// `path_value` overrides `PATH` (tests simulate a missing FFmpeg
/// without touching process state).
pub fn resolve_queue_ffmpeg(path_value: Option<OsString>) -> Result<PathBuf, String> {
    let finding = match path_value {
        Some(p) => ffmpeg::ffmpeg::detect_with_path(Some(p)),
        None => ffmpeg::ffmpeg::detect(),
    };
    if !finding.suitable {
        return Err(refused_renders(format!(
            "FFmpeg is missing or unsuitable ({}); install {} or newer with the libx264 and aac encoders and re-queue. Nothing was queued or rendered.",
            finding.reason,
            ffmpeg::ffmpeg::MIN_FFMPEG_MAJOR,
        )));
    }
    finding.ffmpeg_path.ok_or_else(|| {
        refused_renders(
            "FFmpeg reported suitable but named no binary; reinstall FFmpeg and re-queue. Nothing was queued or rendered."
                .to_string(),
        )
    })
}

/// FFmpeg readiness for the renders readout (Sprint 18, T006): the
/// same gate [`resolve_queue_ffmpeg`] enforces, reported without
/// queueing anything.
pub fn ffmpeg_status(path_value: Option<OsString>) -> (bool, String) {
    match resolve_queue_ffmpeg(path_value) {
        Ok(bin) => (true, format!("ffmpeg: suitable ({})", bin.display())),
        Err(message) => (false, message),
    }
}

/// Queue one render with settings (Sprint 18, T006): guards run in
/// order — settings (in [`parse_enqueue_body`]), an FFmpeg that can
/// encode, a valid project, a known composition, the GIF frame cap —
/// and the appended job plans its output under the hidden renders
/// directory. Every refusal queues nothing and renders nothing.
pub fn enqueue_job(
    folder: &Path,
    req: &EnqueueRequest,
    path_value: Option<OsString>,
) -> Result<EnqueueReceipt, String> {
    // Storyboard approval gate (Sprint 20, T004 / CAP-004, REC-002):
    // while a presented board awaits maker approval, queueing a
    // render refuses pointing at the pending gate. Nothing is queued
    // and nothing renders.
    if let Some(refusal) = crate::studio_stills::gate_refusal(folder, "studio") {
        return Err(refusal);
    }
    let ffmpeg_bin = resolve_queue_ffmpeg(path_value)?;
    let verdict = validate::validate::validate_folder(folder);
    if verdict.status != validate::validate::Status::Valid {
        let mut reason = format!(
            "the project {} is invalid; fix the problems `check` reports and re-queue. Nothing was queued or rendered.",
            folder.display()
        );
        for d in verdict.diagnostics.iter().take(3) {
            reason.push_str(&format!(
                " {}:{}:{} {} {}",
                d.file, d.line, d.column, d.code, d.message
            ));
        }
        if verdict.diagnostics.len() > 3 {
            reason.push_str(&format!(" (and {} more)", verdict.diagnostics.len() - 3));
        }
        return Err(refused_renders(reason));
    }
    let comp = select_queue_composition(&verdict, req.composition.as_deref())?;
    let format = render::outputs::OutputFormat::parse(&req.format).ok_or_else(|| {
        refused_renders(format!(
            "unknown queue format {:?}: expected mp4, mov, webm, or gif. Nothing was queued.",
            req.format
        ))
    })?;
    let mut fps = match req.fps {
        Some(f) => f,
        None => comp.fps.value().round() as u32,
    };
    fps = fps.max(1);
    let mut notes = Vec::new();
    if format == render::outputs::OutputFormat::Gif && fps > GIF_MAX_FPS {
        notes.push(format!(
            "frame rate capped at {GIF_MAX_FPS} fps for GIF (requested {fps})."
        ));
        fps = GIF_MAX_FPS;
    }
    if format == render::outputs::OutputFormat::Gif {
        let frames = notation::model::Fps::from_int(fps as u64).frame_count(comp.duration);
        if frames > GIF_MAX_FRAMES {
            return Err(refused_renders(format!(
                "composition {:?} renders {frames} frames at {fps} fps, over the GIF cap of {GIF_MAX_FRAMES} frames; shorten the timeline, lower `fps`, or queue a video instead. Nothing was queued.",
                comp.name
            )));
        }
    }
    if let Err(message) = render::outputs::check_format_encoders(&ffmpeg_bin, format) {
        return Err(refused_renders(format!("{message} Nothing was queued.")));
    }
    let _guard = QUEUE_LOCK.lock().map_err(|_| {
        refused_renders(
            "the render queue is busy; retry the queue. Nothing was queued.".to_string(),
        )
    })?;
    let mut jobs = read_jobs(folder)?;
    let seq = jobs.iter().map(|j| j.seq).max().unwrap_or(0) + 1;
    let output = renders_dir_for(folder)
        .join(format!(
            "{}-{}.{}",
            comp.name,
            seq,
            format.default_extension()
        ))
        .display()
        .to_string();
    jobs.push(RenderJob {
        seq,
        composition: comp.name.clone(),
        status: "queued".to_string(),
        output: output.clone(),
        actor: req.actor.clone(),
        lint: 0,
        format: format.name().to_string(),
        quality: req.quality.clone(),
        fps: req.fps,
        resolution: req.resolution.clone(),
        gpu: req.gpu,
        kind: if format == render::outputs::OutputFormat::Gif {
            "gif".to_string()
        } else {
            "video".to_string()
        },
        loop_count: req.loop_count,
        stage: "queued".to_string(),
        done_frames: 0,
        total_frames: 0,
        started_ms: 0,
        elapsed_ms: 0,
        error: None,
        notes,
        output_bytes: None,
    });
    write_jobs(folder, &jobs)?;
    Ok(EnqueueReceipt {
        seq,
        composition: comp.name.clone(),
        format: format.name().to_string(),
        fps,
        output,
    })
}

/// Pick the job's composition from a validated verdict (Sprint 18,
/// T006): by `name` or `id`, else the render root. An unknown name
/// refuses listing what the project holds.
fn select_queue_composition(
    verdict: &validate::validate::Verdict,
    want: Option<&str>,
) -> Result<notation::model::Composition, String> {
    match want {
        None => verdict
            .composition
            .clone()
            .or_else(|| verdict.compositions.first().cloned())
            .ok_or_else(|| {
                refused_renders(
                    "the project holds no composition to queue; add one and re-queue. Nothing was queued."
                        .to_string(),
                )
            }),
        Some(name) => {
            let name = name.trim();
            verdict
                .compositions
                .iter()
                .find(|c| c.name == name || c.id.as_deref() == Some(name))
                .cloned()
                .ok_or_else(|| {
                    let mut available: Vec<String> = verdict
                        .compositions
                        .iter()
                        .map(|c| format!("{:?}", c.name))
                        .collect();
                    available.sort();
                    refused_renders(format!(
                        "unknown composition {name:?}: the project holds {}; queue one of those. Nothing was queued.",
                        available.join(", ")
                    ))
                })
        }
    }
}

/// Render one render-one receipt as JSON for the renders API.
pub fn enqueue_receipt_json(receipt: &EnqueueReceipt) -> String {
    format!(
        "{{\"queued\":true,\"seq\":{},\"composition\":\"{}\",\"format\":\"{}\",\"fps\":{},\"output\":\"{}\"}}",
        receipt.seq,
        json_escape(&receipt.composition),
        json_escape(&receipt.format),
        receipt.fps,
        json_escape(&receipt.output)
    )
}

/// Hooks driving one queue run (Sprint 18, T006): `path_value`
/// overrides `PATH` for FFmpeg detection, and `on_progress` observes
/// every piped batch (tests cancel mid-run through it).
pub struct RunHooks<'a> {
    /// `PATH` override for FFmpeg detection; the real `PATH` when `None`.
    pub path_value: Option<OsString>,
    /// Batch observer `(frames_done, frames_total)`, called on the
    /// rendering thread after every emitted batch.
    pub on_progress: Option<&'a dyn Fn(u64, u64)>,
}

impl Default for RunHooks<'_> {
    fn default() -> Self {
        Self {
            path_value: None,
            on_progress: None,
        }
    }
}

/// Outcome of one executed queue job (Sprint 18, T006).
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Job sequence that ran.
    pub seq: u64,
    /// Terminal status (`succeeded`, `failed`, or `cancelled`).
    pub status: String,
    /// Success line or failure report.
    pub detail: String,
}

/// Run the first queued job to completion (Sprint 18, T006): jobs run
/// in sequence order, and the queue advances past failures — a failed
/// job names its stage while the next queued job still runs when its
/// turn comes. Returns `None` while nothing is queued. A corrupt
/// queue file also returns `None`; the readout names the file.
pub fn run_next_queued(folder: &Path, hooks: &RunHooks) -> Option<RunResult> {
    render::cancel::clear();
    let seq = {
        let _guard = QUEUE_LOCK.lock().ok()?;
        let jobs = read_jobs(folder).ok()?;
        jobs.iter()
            .filter(|j| j.status == "queued")
            .min_by_key(|j| j.seq)
            .map(|j| j.seq)
    }?;
    let started = Instant::now();
    let started_ms = now_ms();
    // Claim the row only while it is still queued: a cancel or delete
    // racing the worker wins, and this run stands down.
    let _ = update_job(folder, seq, |job| {
        if job.status == "queued" {
            job.status = "running".to_string();
            job.stage = "validate".to_string();
            job.started_ms = started_ms;
            job.done_frames = 0;
            job.total_frames = 0;
            job.error = None;
            job.elapsed_ms = 0;
            job.output_bytes = None;
        }
    });
    let claimed = {
        let _guard = QUEUE_LOCK.lock().ok()?;
        read_jobs(folder)
            .ok()?
            .into_iter()
            .find(|j| j.seq == seq)
            .is_some_and(|j| j.status == "running" && j.started_ms == started_ms)
    };
    if !claimed {
        return None;
    }
    let outcome = execute_job(folder, seq, hooks);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let (status, stage, error, output_bytes, detail) = match outcome {
        JobOutcome::Succeeded {
            stage,
            output_bytes,
            detail,
        } => ("succeeded", stage, None, Some(output_bytes), detail),
        JobOutcome::Failed { stage, message } => {
            let detail = message.clone();
            ("failed", stage, Some(message), None, detail)
        }
        JobOutcome::Cancelled { stage, message } => {
            let detail = message.clone();
            ("cancelled", stage, Some(message), None, detail)
        }
    };
    let _ = update_job(folder, seq, |job| {
        job.status = status.to_string();
        job.stage = stage.to_string();
        job.error = error.clone();
        job.elapsed_ms = elapsed_ms;
        if output_bytes.is_some() {
            job.output_bytes = output_bytes;
            job.done_frames = job.total_frames;
        }
    });
    render::cancel::clear();
    Some(RunResult {
        seq,
        status: status.to_string(),
        detail,
    })
}

/// One executed job's terminal shape (Sprint 18, T006).
enum JobOutcome {
    Succeeded {
        stage: String,
        output_bytes: u64,
        detail: String,
    },
    Failed {
        stage: String,
        message: String,
    },
    Cancelled {
        stage: String,
        message: String,
    },
}

/// Render one queued job through the pipeline (Sprint 18, T006): the
/// same stages as the headless `render` verb — validate, prepare,
/// audio, frames, encode, output — with the job's settings honored
/// (format, quality, frame rate, resolution, GPU toggle, GIF loop).
/// Anything failing names its stage; cancellation removes the partial
/// output and reports the stage reached.
fn execute_job(folder: &Path, seq: u64, hooks: &RunHooks) -> JobOutcome {
    let set_stage = |stage: &str| {
        let _ = update_job(folder, seq, |job| {
            job.stage = stage.to_string();
        });
    };
    let note = |text: &str| {
        let text = text.to_string();
        let _ = update_job(folder, seq, |job| {
            if !job.notes.contains(&text) {
                job.notes.push(text.clone());
            }
        });
    };
    let failed = |stage: &str, message: String| JobOutcome::Failed {
        stage: stage.to_string(),
        message,
    };
    let ffmpeg_bin = match resolve_queue_ffmpeg(hooks.path_value.clone()) {
        Ok(bin) => bin,
        Err(message) => return failed("ffmpeg", message),
    };
    let verdict = validate::validate::validate_folder(folder);
    if verdict.status != validate::validate::Status::Valid {
        let mut reason = format!(
            "render failed: the project {} is invalid (validate stage); fix the problems `check` reports and re-queue.",
            folder.display()
        );
        for d in verdict.diagnostics.iter().take(3) {
            reason.push_str(&format!(
                " {}:{}:{} {} {}",
                d.file, d.line, d.column, d.code, d.message
            ));
        }
        if verdict.diagnostics.len() > 3 {
            reason.push_str(&format!(" (and {} more)", verdict.diagnostics.len() - 3));
        }
        return failed("validate", reason);
    }
    let tracked = {
        let _guard = match QUEUE_LOCK.lock() {
            Ok(g) => g,
            Err(_) => {
                return failed(
                    "validate",
                    "render failed: the render queue is busy (validate stage); re-queue the job."
                        .to_string(),
                );
            }
        };
        match read_jobs(folder) {
            Ok(jobs) => jobs.into_iter().find(|j| j.seq == seq).map(|j| {
                (
                    j.composition.clone(),
                    j.format.clone(),
                    j.quality.clone(),
                    j.fps,
                    j.resolution.clone(),
                    j.gpu,
                    j.loop_count,
                )
            }),
            Err(e) => return failed("validate", format!("render failed: {e}")),
        }
    };
    let Some((
        composition_name,
        format_name,
        quality_name,
        fps_want,
        resolution_name,
        gpu,
        loop_count,
    )) = tracked
    else {
        return failed(
            "validate",
            format!("render failed: job {seq} left the queue before it ran (validate stage)."),
        );
    };
    let base = match select_queue_composition(&verdict, Some(&composition_name)) {
        Ok(comp) => comp,
        Err(message) => {
            return failed(
                "validate",
                message.replace("Nothing was queued.", "Nothing was rendered."),
            );
        }
    };
    let format = match render::outputs::OutputFormat::parse(&format_name) {
        Some(f)
            if matches!(
                f,
                render::outputs::OutputFormat::Mp4
                    | render::outputs::OutputFormat::Mov
                    | render::outputs::OutputFormat::Webm
                    | render::outputs::OutputFormat::Gif
            ) =>
        {
            f
        }
        _ => {
            return failed(
                "validate",
                format!(
                    "render failed: job {seq} carries unknown queue format {format_name:?} (validate stage); re-queue with mp4, mov, webm, or gif."
                ),
            );
        }
    };
    let preset = match render::render::Preset::parse(&quality_name) {
        Some(p) => p,
        None => {
            return failed(
                "validate",
                format!(
                    "render failed: job {seq} carries unknown queue quality {quality_name:?} (validate stage); re-queue with draft, standard, or high."
                ),
            );
        }
    };
    let resolution = match resolution_name.as_str() {
        "auto" => render::scale::Resolution::Source,
        "1080p" => render::scale::Resolution::P1080,
        "4k" => render::scale::Resolution::P2160,
        _ => {
            return failed(
                "validate",
                format!(
                    "render failed: job {seq} carries unknown queue resolution {resolution_name:?} (validate stage); re-queue with auto, 1080p, or 4k."
                ),
            );
        }
    };
    let mut fps = fps_want
        .unwrap_or_else(|| base.fps.value().round() as u32)
        .max(1);
    if format == render::outputs::OutputFormat::Gif && fps > GIF_MAX_FPS {
        note(&format!(
            "frame rate capped at {GIF_MAX_FPS} fps for GIF (requested {fps})."
        ));
        fps = GIF_MAX_FPS;
    }
    if format == render::outputs::OutputFormat::Gif {
        let frames = notation::model::Fps::from_int(fps as u64).frame_count(base.duration);
        if frames > GIF_MAX_FRAMES {
            return failed(
                "validate",
                format!(
                    "render failed: composition {:?} renders {frames} frames at {fps} fps, over the GIF cap of {GIF_MAX_FRAMES} frames (validate stage); shorten the timeline, lower `fps`, or queue a video instead. Nothing was written.",
                    base.name
                ),
            );
        }
    }
    if let Err(message) = render::outputs::check_format_encoders(&ffmpeg_bin, format) {
        return failed("validate", message);
    }
    let mut opts = render::render::RenderOptions {
        preset,
        format,
        resolution,
        gif_loop: loop_count,
        ..render::render::RenderOptions::default()
    };
    if gpu {
        opts.encoder_choice = render::outputs::EncoderChoice::Auto;
        // Sprint 25 T002: `auto` resolves per OS to the first hardware
        // encoder the host listing provides, else software.
        if let Some((codec, _choice)) = render::outputs::resolve_auto(&ffmpeg_bin, format) {
            opts.hardware = true;
            opts.hw_codec = Some(codec);
            note(&format!("GPU encoding on the hardware encoder ({codec})."));
        } else {
            note("GPU toggle on but the host FFmpeg provides no hardware encoder for this format; software encode.");
        }
    }
    let mut comp = base.clone();
    comp.fps = notation::model::Fps::from_int(fps as u64);
    let scale = opts.scale_plan(comp.width, comp.height);
    let scaled_comp = render::scale::scale_composition(&comp, scale);
    let fps_overridden = fps != base.fps.value().round() as u32;
    let scaled_all: Vec<notation::model::Composition> = verdict
        .compositions
        .iter()
        .map(|c| {
            let mut sized = render::scale::scale_composition(c, scale);
            if c.name == comp.name || c.id == comp.id {
                sized.fps = notation::model::Fps::from_int(fps as u64);
            }
            sized
        })
        .collect();
    set_stage("prepare");
    if render::cancel::is_cancelled() {
        return cancelled_before_frames(seq, "prepare");
    }
    let prep = match render::render::prepare_frames(&scaled_comp, &scaled_all, folder, &ffmpeg_bin)
    {
        Ok(prep) => prep,
        Err(e) => {
            if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                return cancelled_before_frames(seq, "prepare");
            }
            return failed("prepare", e);
        }
    };
    let total_frames = prep.frames() as u64;
    let _ = update_job(folder, seq, |job| {
        job.total_frames = total_frames;
        job.done_frames = 0;
    });
    let detected = prep.composition_transfer();
    let sources = prep.hdr_sources();
    match resolve_queue_hdr(format, detected, &sources) {
        Ok((transfer, tone_mapped, hdr_note)) => {
            opts.hdr_transfer = transfer;
            opts.hdr_tone_mapped = tone_mapped;
            if transfer != render::outputs::HdrTransfer::Sdr && opts.hardware {
                opts.hardware = false;
                note("HDR renders on software (HEVC Main10 BT.2020); the hardware encoder steps aside for this render.");
            }
            if let Some(hdr_note) = hdr_note {
                note(&hdr_note);
            }
        }
        Err(message) => return failed("validate", message),
    }
    let digest_all: Vec<notation::model::Composition> = if fps_overridden || !scale.identity {
        scaled_all
    } else {
        verdict.compositions.clone()
    };
    let digest = render::outputs::composition_inputs_digest(&digest_all, folder);
    opts.provenance = Some(render::outputs::Provenance {
        version: env!("CARGO_PKG_VERSION").to_string(),
        digest,
    });
    if format == render::outputs::OutputFormat::Gif {
        note("GIF carries no provenance stamp (pictures unchanged).");
    }
    set_stage("audio");
    if render::cancel::is_cancelled() {
        return cancelled_before_frames(seq, "audio");
    }
    let output_path = {
        let _guard = match QUEUE_LOCK.lock() {
            Ok(g) => g,
            Err(_) => {
                return failed(
                    "audio",
                    "render failed: the render queue is busy (audio stage); re-queue the job."
                        .to_string(),
                );
            }
        };
        match read_jobs(folder) {
            Ok(jobs) => jobs
                .into_iter()
                .find(|j| j.seq == seq)
                .map(|j| j.output.clone()),
            Err(e) => return failed("audio", format!("render failed: {e}")),
        }
    }
    .unwrap_or_default();
    let output = PathBuf::from(&output_path);
    let mix_path = render::render::mix_staging_path(&output);
    let plan =
        match render::render::build_audio_plan(&scaled_comp, folder, &ffmpeg_bin, &prep, &mix_path)
        {
            Ok(plan) => plan,
            Err(e) => {
                let _ = std::fs::remove_file(&mix_path);
                if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                    return cancelled_before_frames(seq, "audio");
                }
                return failed("audio", e);
            }
        };
    if render::cancel::is_cancelled() {
        let _ = std::fs::remove_file(&mix_path);
        return cancelled_before_frames(seq, "audio");
    }
    set_stage("frames");
    let dest = render::outputs::ResolvedOutput::File {
        path: output.clone(),
    };
    // Measured progress (Sprint 18, T006): every piped batch persists
    // `done_frames` beside the job, so the readout measures frames
    // instead of guessing; the test hook observes the same counts.
    let hook = |done: u64, total: u64| {
        let _ = update_job(folder, seq, |job| {
            job.done_frames = done;
            job.total_frames = total;
            job.stage = "frames".to_string();
        });
        if let Some(observe) = hooks.on_progress {
            observe(done, total);
        }
    };
    let encode_result = render::render::render_staged_with_progress(
        &prep,
        &plan,
        &opts,
        &ffmpeg_bin,
        &dest,
        &scale,
        Some(&hook),
    );
    let (done_msg, _frames_ms, _encode_ms) = match encode_result {
        Ok(outcome) => outcome,
        Err(e) => {
            if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                let stage = if e.contains("encode") {
                    "encode"
                } else {
                    "frames"
                };
                remove_partial_output(&output, Some(&mix_path));
                return JobOutcome::Cancelled {
                    stage: stage.to_string(),
                    message: format!(
                        "render cancelled: job {seq} stopped during {stage}; the partial output was removed. {}",
                        without_cancel_prefix(&e)
                    ),
                };
            }
            let stage = if e.contains("(encode stage)") {
                "encode"
            } else if e.contains("(output stage)") {
                "output"
            } else {
                "frames"
            };
            return failed(stage, e);
        }
    };
    set_stage("done");
    let output_bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    JobOutcome::Succeeded {
        stage: "done".to_string(),
        output_bytes,
        detail: done_msg,
    }
}

/// Cancel before any frame piped (Sprint 18, T006): nothing staged
/// yet, so the report names the reached stage with nothing to remove.
fn cancelled_before_frames(seq: u64, stage: &str) -> JobOutcome {
    JobOutcome::Cancelled {
        stage: stage.to_string(),
        message: format!(
            "render cancelled: job {seq} stopped during {stage} before any frame piped; no output was written."
        ),
    }
}

/// Strip the `cancelled:` sentinel for the row report (Sprint 18,
/// T006): the stage already rides `stage`, so the message stays short.
fn without_cancel_prefix(e: &str) -> String {
    match e.strip_prefix(render::cancel::PREFIX) {
        Some(rest) => rest.trim().to_string(),
        None => e.to_string(),
    }
}

/// Remove the partial output beside a cancelled job (Sprint 18,
/// T006): the staged temp plus any final file the finalize step may
/// have placed, plus the staged mix. Leftovers ride the report.
fn remove_partial_output(output: &Path, mix: Option<&PathBuf>) {
    let mut staged = output.as_os_str().to_owned();
    staged.push(".framesmith-part");
    let staged = PathBuf::from(staged);
    let _ = std::fs::remove_file(&staged);
    let _ = std::fs::remove_file(output);
    if let Some(mix) = mix {
        let _ = std::fs::remove_file(mix);
    }
}

/// Mirror the headless HDR `auto` decision for one queue job (Sprint
/// 18, T006): SDR stays silent; HDR rides HEVC Main10 where the
/// format carries it, tone-maps on the SDR-only picture formats, and
/// refuses naming the format limit otherwise.
fn resolve_queue_hdr(
    format: render::outputs::OutputFormat,
    detected: render::outputs::HdrTransfer,
    sources: &[(PathBuf, render::outputs::HdrTransfer)],
) -> Result<(render::outputs::HdrTransfer, bool, Option<String>), String> {
    use render::outputs::HdrTransfer;
    if detected == HdrTransfer::Sdr {
        return Ok((HdrTransfer::Sdr, false, None));
    }
    let names = sources
        .iter()
        .map(|(p, _)| {
            p.file_name()
                .map(|n| format!("{n:?}"))
                .unwrap_or_else(|| format!("{p:?}"))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");
    let what = if detected == HdrTransfer::Pq {
        "PQ"
    } else {
        "HLG"
    };
    if render::outputs::format_carries_hdr(format) {
        Ok((
            detected,
            false,
            Some(format!(
                "HDR {what} detected in {names}; rendering HEVC Main10 BT.2020."
            )),
        ))
    } else if render::outputs::format_tone_maps_hdr(format) {
        Ok((
            HdrTransfer::Sdr,
            true,
            Some(format!(
                "HDR {what} detected in {names}, but {} is SDR-only; rendering a tone-mapped SDR animation.",
                format.name()
            )),
        ))
    } else {
        Err(format!(
            "render refused: HDR {what} detected in {names}, but --format {} cannot carry HDR (validate stage); re-queue with mp4 for HEVC Main10 BT.2020. Nothing was written",
            format.name()
        ))
    }
}

/// Cancel one job (Sprint 18, T006 / REC-005): a queued job cancels
/// before it starts with nothing written; a running job stops through
/// the pipeline cancel flag and its row reports cancelled with the
/// stage reached once the encode stops. Terminal jobs refuse as
/// already finished.
pub fn cancel_job(folder: &Path, seq: u64) -> Result<String, String> {
    let _guard = QUEUE_LOCK.lock().map_err(|_| {
        refused_renders("the render queue is busy; retry the cancel. Nothing changed.".to_string())
    })?;
    let mut jobs = read_jobs(folder)?;
    let Some(job) = jobs.iter_mut().find(|j| j.seq == seq) else {
        return Err(refused_renders(format!(
            "unknown job {seq}: the queue holds {}; cancel one of those. Nothing changed.",
            job_seqs(&jobs)
        )));
    };
    if TERMINAL_STATES.contains(&job.status.as_str()) {
        return Err(refused_renders(format!(
            "job {seq} is already {}; finished rows clear through hide-finished. Nothing changed.",
            job.status
        )));
    }
    if job.status == "running" {
        render::cancel::request();
        return Ok(format!(
            "cancel requested for job {seq} ({}); the row reports cancelled with the stage reached once the encode stops.",
            job.composition
        ));
    }
    let composition = job.composition.clone();
    {
        let job = &mut jobs.iter_mut().find(|j| j.seq == seq).expect("row present");
        job.status = "cancelled".to_string();
        job.stage = "queued".to_string();
        job.error = Some(format!(
            "render cancelled: job {seq} cancelled while queued; no output was written."
        ));
        job.elapsed_ms = 0;
    }
    write_jobs(folder, &jobs)?;
    Ok(format!(
        "cancelled queued job {seq} ({composition}); no output was written."
    ))
}

/// Delete one job (Sprint 18, T006 / REC-003): terminal rows go with
/// their output files, queued rows go with nothing written, and a
/// running job refuses until it is cancelled. The report says what
/// was removed.
pub fn delete_job(folder: &Path, seq: u64) -> Result<String, String> {
    let _guard = QUEUE_LOCK.lock().map_err(|_| {
        refused_renders(
            "the render queue is busy; retry the delete. Nothing was deleted.".to_string(),
        )
    })?;
    let mut jobs = read_jobs(folder)?;
    let pos = jobs.iter().position(|j| j.seq == seq).ok_or_else(|| {
        refused_renders(format!(
            "unknown job {seq}: the queue holds {}; delete one of those. Nothing was deleted.",
            job_seqs(&jobs)
        ))
    })?;
    if jobs[pos].status == "running" {
        return Err(refused_renders(format!(
            "job {seq} is running; cancel it before deleting. Nothing was deleted."
        )));
    }
    let job = jobs.remove(pos);
    write_jobs(folder, &jobs)?;
    let output = PathBuf::from(&job.output);
    if job.status == "queued" || job.status == "cancelled" && job.output_bytes.is_none() {
        if output.exists() {
            let _ = std::fs::remove_file(&output);
        }
        return Ok(format!(
            "deleted job {seq} ({}); no output existed.",
            job.composition
        ));
    }
    match std::fs::remove_file(&output) {
        Ok(()) => Ok(format!(
            "deleted job {seq} ({}): removed {}.",
            job.composition,
            output.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(format!(
            "deleted job {seq} ({}): the row was cleared; no output file remained.",
            job.composition
        )),
        Err(e) => Ok(format!(
            "deleted job {seq} ({}): the row was cleared but {} could not be removed ({e}).",
            job.composition,
            output.display()
        )),
    }
}

/// Hide finished rows (Sprint 18, T006): succeeded, failed, and
/// cancelled rows clear with their output files; queued and running
/// jobs stay exactly as they are.
pub fn hide_finished(folder: &Path) -> Result<String, String> {
    let _guard = QUEUE_LOCK.lock().map_err(|_| {
        refused_renders(
            "the render queue is busy; retry hiding finished rows. Nothing changed.".to_string(),
        )
    })?;
    let mut jobs = read_jobs(folder)?;
    let finished: Vec<RenderJob> = jobs
        .iter()
        .filter(|j| TERMINAL_STATES.contains(&j.status.as_str()))
        .cloned()
        .collect();
    if finished.is_empty() {
        return Ok("no finished jobs to clear; queued and running jobs stay.".to_string());
    }
    for job in &finished {
        let _ = std::fs::remove_file(&job.output);
    }
    jobs.retain(|j| !TERMINAL_STATES.contains(&j.status.as_str()));
    write_jobs(folder, &jobs)?;
    let seqs = finished
        .iter()
        .map(|j| j.seq.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "cleared {} finished job(s) ({}); queued and running jobs stay.",
        finished.len(),
        seqs
    ))
}

/// Fail running rows left by a dead server (Sprint 18, T006): the
/// run can never report back, so the row names the interrupted stage
/// and the partial output goes, while queued rows wait their turn.
fn reset_interrupted_jobs(folder: &Path) {
    let _guard = match QUEUE_LOCK.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let mut jobs = match read_jobs(folder) {
        Ok(jobs) => jobs,
        Err(_) => return,
    };
    let mut touched = false;
    for job in jobs.iter_mut().filter(|j| j.status == "running") {
        let output = PathBuf::from(&job.output);
        remove_partial_output(&output, None);
        job.status = "failed".to_string();
        job.error = Some(format!(
            "render failed: the Studio server stopped while job {} ran during {} (interrupted stage); the partial output was removed.",
            job.seq, job.stage
        ));
        job.elapsed_ms = 0;
        touched = true;
    }
    if touched {
        let _ = write_jobs(folder, &jobs);
    }
}

/// Sequence list naming what the queue holds (Sprint 18, T006): used
/// by unknown-job refusals.
fn job_seqs(jobs: &[RenderJob]) -> String {
    if jobs.is_empty() {
        return "nothing".to_string();
    }
    let mut seqs: Vec<u64> = jobs.iter().map(|j| j.seq).collect();
    seqs.sort_unstable();
    seqs.iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Download one job's output bytes (Sprint 18, T006): the exact
/// rendered file, so the download byte-matches what the queue
/// delivered. A missing file before success refuses naming the
/// state; a missing file after success names the loss.
pub fn download_job(folder: &Path, seq: u64) -> Result<(String, Vec<u8>), String> {
    let _guard = QUEUE_LOCK.lock().map_err(|_| {
        refused_renders("the render queue is busy; retry the download.".to_string())
    })?;
    let jobs = read_jobs(folder)?;
    let Some(job) = jobs.iter().find(|j| j.seq == seq) else {
        return Err(refused_renders(format!(
            "unknown job {seq}: the queue holds {}; download one of those.",
            job_seqs(&jobs)
        )));
    };
    if job.status != "succeeded" {
        return Err(refused_renders(format!(
            "job {seq} is {}; only succeeded jobs download. Wait for the run or re-queue.",
            job.status
        )));
    }
    match std::fs::read(&job.output) {
        Ok(bytes) => Ok((job.output.clone(), bytes)),
        Err(e) => Err(refused_renders(format!(
            "job {seq} succeeded but {} cannot be read ({e}); the output is lost — re-queue the job.",
            job.output
        ))),
    }
}

/// Content type serving one queue download (Sprint 18, T006): the
/// container behind the queue format.
pub fn download_content_type(output: &str) -> &'static str {
    if output.ends_with(".webm") {
        "video/webm"
    } else if output.ends_with(".mov") {
        "video/quicktime"
    } else if output.ends_with(".gif") {
        "image/gif"
    } else {
        "video/mp4"
    }
}

/// Measured ETA seconds for one job (Sprint 18, T006): elapsed time
/// scaled by the unfinished frame share — measured, never guessed —
/// or `None` while nothing measurable has piped yet. The label is
/// always `estimate`, per the specification.
pub fn eta_secs(job: &RenderJob, now: u64) -> Option<f64> {
    if job.status != "running" {
        return None;
    }
    if job.total_frames == 0 || job.done_frames == 0 || job.done_frames >= job.total_frames {
        return None;
    }
    if now <= job.started_ms {
        return None;
    }
    let elapsed = (now - job.started_ms) as f64 / 1000.0;
    Some(elapsed * (job.total_frames - job.done_frames) as f64 / job.done_frames as f64)
}

/// Measured progress share for one job (Sprint 18, T006): `None`
/// while the total is unknown.
pub fn progress_share(job: &RenderJob) -> Option<f64> {
    if job.total_frames == 0 {
        return None;
    }
    Some((job.done_frames.min(job.total_frames) as f64) / (job.total_frames as f64))
}

/// Render the queue readout as JSON for the renders API (Sprint 18,
/// T006): FFmpeg readiness plus every job with measured progress and
/// the ETA labeled an estimate.
pub fn renders_queue_json(
    jobs: &[RenderJob],
    ffmpeg_available: bool,
    ffmpeg_detail: &str,
    now: u64,
) -> String {
    let mut out = String::new();
    for (i, job) in jobs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let mut row = crate::studio_compositions::render_job_json(job);
        row.pop();
        let progress = match progress_share(job) {
            Some(p) => format!("{p:.4}"),
            None => "null".to_string(),
        };
        let (eta_secs, eta_label) = match eta_secs(job, now) {
            Some(eta) => (format!("{eta:.1}"), "\"estimate\""),
            None => ("null".to_string(), "null"),
        };
        out.push_str(&format!(
            "{},\"progress\":{},\"eta_secs\":{},\"eta_label\":{}}}",
            row, progress, eta_secs, eta_label
        ));
    }
    format!(
        "{{\"ffmpeg\":{{\"available\":{},\"detail\":\"{}\"}},\"jobs\":[{}]}}",
        ffmpeg_available,
        json_escape(ffmpeg_detail),
        out
    )
}

/// Spawn the queue worker (Sprint 18, T006): one background thread
/// per server running queued jobs in order until `shutdown` flips.
/// Jobs keep advancing past failures; each pump iteration holds the
/// queue guard only around file reads and writes, never across a
/// render, so progress polls never block on a running job.
pub fn spawn_queue_worker(
    folder: PathBuf,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    // A previous server may have died mid-render: its running rows can
    // never complete, so they fail naming the interrupted stage before
    // this worker picks up anything queued.
    reset_interrupted_jobs(&folder);
    std::thread::spawn(move || {
        while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
            let hooks = RunHooks::default();
            match run_next_queued(&folder, &hooks) {
                Some(_) => {}
                None => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
    });
}

/// Extra CSS the renders panel needs (Sprint 18, T006): the queue
/// form, the job table with measured progress, and the status line.
/// Style attributes stay CSS-like hyphenated end to end.
pub fn renders_css() -> String {
    r#"        #panel-renders{margin-top:8px}
        #renders-form{display:flex;gap:8px;flex-wrap:wrap;align-items:flex-end;margin-top:8px}
        #renders-form label{display:flex;flex-direction:column;font-size:12px}
        #renders-form select,#renders-form input{margin-top:2px}
        #renders-form button,#renders-hide{border-radius:4px}
        #renders-table{width:100%;border-collapse:collapse;margin-top:8px}
        #renders-table td,#renders-table th{border:1px solid var(--line-color);padding:2px 6px;font-variant-numeric:tabular-nums}
        #renders-status{font-variant-numeric:tabular-nums}
        .renders-progress{min-width:120px}"#.to_string()
}

/// The renders control block (Sprint 18, T006): the queue form with
/// format, quality, frame rate, resolution, GPU toggle, and GIF loop
/// count, the job table the panel script paints from the renders API,
/// and a status line reporting receipts and refusals.
pub fn renders_html() -> String {
    r#"<section class="panel" id="panel-renders" aria-label="renders panel">
        <h2>renders</h2>
        <form id="renders-form">
        <label>composition <input id="renders-composition" type="text" placeholder="root"></label>
        <label>format <select id="renders-format"><option value="mp4">mp4</option><option value="mov">mov</option><option value="webm">webm</option><option value="gif">gif</option></select></label>
        <label>quality <select id="renders-quality"><option value="draft">draft</option><option value="standard" selected>standard</option><option value="high">high</option></select></label>
        <label>frame rate <select id="renders-fps"><option value="">composition</option><option value="24">24</option><option value="30">30</option><option value="60">60</option></select></label>
        <label>resolution <select id="renders-resolution"><option value="auto" selected>auto</option><option value="1080p">1080p</option><option value="4k">4k</option></select></label>
        <label>gpu <input id="renders-gpu" type="checkbox"></label>
        <label>loop count <input id="renders-loop" type="number" value="0" min="0" max="65535" step="1"></label>
        <button id="renders-queue" type="button">queue render</button>
        </form>
        <table id="renders-table"><thead><tr><th>job</th><th>composition</th><th>settings</th><th>status</th><th>progress</th><th>eta (estimate)</th><th>actions</th></tr></thead>
        <tbody id="renders-body"></tbody></table>
        <div><button id="renders-hide" type="button">hide finished</button></div>
        <p id="renders-status" role="status">renders ready</p>
        </section>"#
        .to_string()
}

/// The renders panel script (Sprint 18, T006): queues renders with
/// the form settings, paints the queue with measured progress and
/// the ETA estimate, cancels queued and running jobs, deletes rows
/// (refusing running ones), hides finished rows, and downloads the
/// exact rendered files.
pub fn renders_js() -> String {
    r#"        function rendersStatus(t) { const s = document.getElementById('renders-status'); if (s) s.textContent = t; }
        let rendersTimer = null;
        async function paintRenders() {
          const box = document.getElementById('renders-body'); if (!box) return;
          let j = null;
          try {
            const r = await fetch('/api/studio/renders');
            if (!r.ok) { rendersStatus(await r.text()); return; }
            j = await r.json();
          } catch (e) { rendersStatus('renders unavailable: ' + e); return; }
          if (!j.ffmpeg || !j.ffmpeg.available) {
            rendersStatus((j.ffmpeg && j.ffmpeg.detail) || 'FFmpeg is missing: install it before queueing.');
          }
          box.innerHTML = '';
          let active = false;
          for (const job of (j.jobs || [])) {
            const tr = document.createElement('tr');
            const seq = document.createElement('td'); seq.textContent = job.seq; tr.appendChild(seq);
            const comp = document.createElement('td'); comp.textContent = job.composition; tr.appendChild(comp);
            const settings = document.createElement('td');
            settings.textContent = job.format + ' ' + job.quality + ' ' + (job.fps === null ? 'composition' : job.fps) + 'fps ' + job.resolution + (job.gpu ? ' gpu' : '') + (job.kind === 'gif' ? (' loop ' + job.loop_count) : '');
            tr.appendChild(settings);
            const status = document.createElement('td');
            status.textContent = job.status + (job.stage && job.stage !== job.status ? (' (' + job.stage + ')') : '') + (job.error ? (': ' + job.error) : '');
            tr.appendChild(status);
            const progress = document.createElement('td'); progress.className = 'renders-progress';
            progress.textContent = (job.progress === null || job.progress === undefined) ? '—' : (Number(job.progress * 100).toFixed(1) + '%');
            tr.appendChild(progress);
            const eta = document.createElement('td');
            eta.textContent = (job.eta_secs === null || job.eta_secs === undefined) ? '—' : (Number(job.eta_secs).toFixed(1) + 's (estimate)');
            tr.appendChild(eta);
            const actions = document.createElement('td');
            const cancel = document.createElement('button'); cancel.type = 'button'; cancel.textContent = 'cancel';
            cancel.disabled = (job.status !== 'queued' && job.status !== 'running');
            cancel.addEventListener('click', () => postRendersCancel(job.seq));
            const del = document.createElement('button'); del.type = 'button'; del.textContent = 'delete';
            del.disabled = (job.status === 'running');
            del.addEventListener('click', () => postRendersDelete(job.seq));
            actions.append(cancel, del);
            if (job.status === 'succeeded') {
              const down = document.createElement('a'); down.textContent = 'download';
              down.href = '/api/studio/renders/' + job.seq + '/download';
              actions.appendChild(down);
            }
            tr.appendChild(actions);
            box.appendChild(tr);
            if (job.status === 'queued' || job.status === 'running') active = true;
          }
          if ((j.jobs || []).length === 0) rendersStatus('no queued renders');
          else if (active) rendersStatus(j.jobs.length + ' job(s) in the queue');
          if (rendersTimer) { clearTimeout(rendersTimer); rendersTimer = null; }
          if (active) rendersTimer = setTimeout(paintRenders, 2000);
        }
        async function postRendersEnqueue() {
          try {
            const fpsRaw = document.getElementById('renders-fps').value;
            const body = {
              actor: 'maker',
              composition: document.getElementById('renders-composition').value || null,
              format: document.getElementById('renders-format').value,
              quality: document.getElementById('renders-quality').value,
              fps: fpsRaw === '' ? null : Number(fpsRaw),
              resolution: document.getElementById('renders-resolution').value,
              gpu: document.getElementById('renders-gpu').checked,
              loop_count: Number(document.getElementById('renders-loop').value || 0)
            };
            const r = await fetch('/api/studio/renders', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
            const t = await r.text();
            if (!r.ok) { rendersStatus(t); banner(t); toast(t); return; }
            const j = JSON.parse(t);
            rendersStatus('queued ' + j.composition + ' as job ' + j.seq + ' (' + j.format + ')');
            toast('queued ' + j.composition + ' (job ' + j.seq + ')');
            paintRenders();
          } catch (e) { rendersStatus('queue failed: ' + e); }
        }
        async function postRendersCancel(seq) {
          try {
            const r = await fetch('/api/studio/renders/' + seq + '/cancel', {method: 'POST'});
            const t = await r.text();
            if (!r.ok) { rendersStatus(t); banner(t); toast(t); return; }
            rendersStatus(t); toast(t); paintRenders();
          } catch (e) { rendersStatus('cancel failed: ' + e); }
        }
        async function postRendersDelete(seq) {
          try {
            const r = await fetch('/api/studio/renders/' + seq, {method: 'DELETE'});
            const t = await r.text();
            if (!r.ok) { rendersStatus(t); banner(t); toast(t); return; }
            rendersStatus(t); toast(t); paintRenders();
          } catch (e) { rendersStatus('delete failed: ' + e); }
        }
        async function postRendersHide() {
          try {
            const r = await fetch('/api/studio/renders/hide-finished', {method: 'POST'});
            const t = await r.text();
            if (!r.ok) { rendersStatus(t); banner(t); return; }
            rendersStatus(t); paintRenders();
          } catch (e) { rendersStatus('hide failed: ' + e); }
        }
        function wireRenders() {
          const form = document.getElementById('renders-form');
          if (form && !form.dataset.wired) { form.dataset.wired = '1'; form.addEventListener('submit', (e) => { e.preventDefault(); postRendersEnqueue(); }); }
          const hide = document.getElementById('renders-hide');
          if (hide && !hide.dataset.wired) { hide.dataset.wired = '1'; hide.addEventListener('click', postRendersHide); }
        }
        wireRenders(); paintRenders();"#
        .to_string()
}
