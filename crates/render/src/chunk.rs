//! In-process chunking (Sprint 26, T001 / CAP-001, CAP-002, CAP-003,
//! CAP-008, CAP-009): a chunked render splits the prepared frame range
//! into contiguous ranges, draws and encodes each chunk independently
//! through the same worker scheduler single-pass renders use, and
//! reassembles the chunk files in order with a lossless
//! (stream-copy) concat. Single-pass renders never enter this path and
//! keep their historical bytes exactly.

use crate::outputs::OutputFormat;
use crate::render::{AudioPlan, Prepared, RenderOptions, origin_note, render_staged, stderr_tail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One contiguous chunk of a prepared render: frame indices
/// `[start_frame, end_frame)` in the preparation's local frame space
/// (frame 0 is the first prepared frame, whatever its composition
/// time is). A plan tiles `[0, total)` with no overlap or gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRange {
    /// Zero-based position of this chunk inside the plan.
    pub index: usize,
    /// First frame owned by this chunk (inclusive).
    pub start_frame: u64,
    /// First frame after this chunk (exclusive).
    pub end_frame: u64,
}

impl ChunkRange {
    /// Frames owned by this chunk (always at least one in a valid plan).
    pub fn frame_count(self) -> u64 {
        self.end_frame.saturating_sub(self.start_frame)
    }

    /// Short plan label, for refusal and receipt text:
    /// `chunk 2/5 frames 30..60`.
    pub fn label(self, total: usize) -> String {
        format!(
            "chunk {}/{} frames {}..{}",
            self.index + 1,
            total,
            self.start_frame,
            self.end_frame
        )
    }
}

/// Plan a chunk tiling of `total_frames` (Sprint 26, T001 / CAP-001):
/// exactly one of `target_chunks` (count) or `target_size` (frames per
/// chunk) must be given. Count plans spread the remainder over the
/// first chunks (10 frames over 3 chunks tiles as 4, 3, 3); size
/// plans cut greedily (10 frames at size 3 tiles as 3, 3, 3, 1).
/// Every chunk owns at least one frame, consecutive chunks meet
/// exactly, and the union is `[0, total_frames)`. An unplannable
/// request (no frames, a zero target, both targets at once, or more
/// chunks than frames — which would strand an empty chunk) refuses
/// naming the value, before any frame is drawn.
pub fn plan_chunks(
    total_frames: usize,
    target_chunks: Option<usize>,
    target_size: Option<usize>,
) -> Result<Vec<ChunkRange>, String> {
    if total_frames == 0 {
        return Err(
            "render refused: cannot plan chunks over a composition with no frames; no MP4 was written"
                .to_string(),
        );
    }
    match (target_chunks, target_size) {
        (Some(_), Some(_)) => Err(
            "render refused: --chunks and --chunk-size together are ambiguous: pass only one chunk target (validation stage); no MP4 was written"
                .to_string(),
        ),
        (None, None) => Err(
            "render refused: chunked rendering needs a chunk target: pass --chunks <count> or --chunk-size <frames> (validation stage); no MP4 was written"
                .to_string(),
        ),
        (Some(0), None) => Err(
            "render refused: --chunks 0 asks for no chunks: pass a count from 1 to {} (validation stage); no MP4 was written"
                .to_string(),
        ),
        (Some(n), None) if n > total_frames => Err(format!(
            "render refused: --chunks {n} strands frames over {} frame(s): more chunks than frames leaves an empty chunk; pass --chunks at most {total_frames} (validation stage); no MP4 was written",
            total_frames, total_frames = total_frames
        )),
        (Some(n), None) => {
            let base = (total_frames / n) as u64;
            let rem = (total_frames % n) as u64;
            let mut out = Vec::with_capacity(n);
            let mut cursor = 0u64;
            for i in 0..n {
                let len = base + u64::from((i as u64) < rem);
                out.push(ChunkRange {
                    index: i,
                    start_frame: cursor,
                    end_frame: cursor + len,
                });
                cursor += len;
            }
            Ok(out)
        }
        (None, Some(0)) => Err(
            "render refused: --chunk-size 0 asks for empty chunks: pass a size from 1 frame (validation stage); no MP4 was written"
                .to_string(),
        ),
        (None, Some(size)) => {
            let size = size as u64;
            let total = total_frames as u64;
            let mut out = Vec::new();
            let mut cursor = 0u64;
            let mut index = 0usize;
            while cursor < total {
                let end = (cursor + size).min(total);
                out.push(ChunkRange {
                    index,
                    start_frame: cursor,
                    end_frame: end,
                });
                cursor = end;
                index += 1;
            }
            Ok(out)
        }
    }
}

/// Formats a chunked render may reassemble (Sprint 26, T001): file
/// containers the concat demuxer joins with stream copy. GIF loops a
/// single raster and PNG sequences / HLS are directories, so none of
/// them has an ordered chunk reassembly; those refuse before the
/// first frame naming the format.
pub fn check_chunked_format(format: OutputFormat) -> Result<(), String> {
    match format {
        OutputFormat::Mp4 | OutputFormat::Hevc | OutputFormat::Webm | OutputFormat::Mov
        | OutputFormat::Prores => Ok(()),
        OutputFormat::Gif => Err(format!(
            "render refused: chunking with --format {} is not supported: a GIF is a single looping raster with no ordered reassembly; render single-pass for a GIF (validation stage); no output was written",
            format.name()
        )),
        OutputFormat::PngSeq | OutputFormat::Hls => Err(format!(
            "render refused: chunking with --format {} is not supported: a directory-style destination has no ordered chunk reassembly; render single-pass for --format {} (validation stage); no output was written",
            format.name(),
            format.name()
        )),
    }
}

/// One finished chunk artifact plus its planned range: the input the
/// ordered reassembly consumes, in plan order.
#[derive(Debug, Clone)]
pub struct ChunkArtifact {
    /// Planned range this artifact was drawn and encoded from.
    pub range: ChunkRange,
    /// Encoded chunk file on disk.
    pub path: PathBuf,
}

/// Completion receipt for one chunk (Sprint 26, T001 / CAP-002): the
/// range it rendered, its frame count, and the identity of its output
/// file. Receipts are measured from the chunk artifact, never
/// estimated.
#[derive(Debug, Clone)]
pub struct ChunkReceipt {
    /// Zero-based position of this chunk inside the plan.
    pub index: usize,
    /// First frame this chunk rendered (inclusive).
    pub start_frame: u64,
    /// First frame after this chunk (exclusive).
    pub end_frame: u64,
    /// Frames drawn and encoded for this chunk.
    pub frames: u64,
    /// Encoded chunk file path.
    pub path: PathBuf,
    /// Chunk file size in bytes (its output identity).
    pub bytes: u64,
}

/// Outcome of a chunked render (Sprint 26, T001): the final-file
/// success line plus measured stage durations, per-chunk receipts,
/// the chunk directory holding the artifacts, and the equivalence
/// probe verdict (Sprint 26, T004 / CAP-004): the reassembled
/// output matched the single-pass oracle inside the codec noise
/// band, or the run refused and no outcome exists.
pub struct ChunkedOutcome {
    /// Human success line naming the chunked output and its plan.
    pub message: String,
    /// Measured frame/draw wall time across all chunks plus the
    /// oracle.
    pub draw_ms: u64,
    /// Measured encode wall time across all chunks, the oracle,
    /// reassembly, and the probe.
    pub encode_ms: u64,
    /// One receipt per chunk, in plan order.
    pub receipts: Vec<ChunkReceipt>,
    /// Directory holding the chunk artifacts beside the final file.
    pub chunk_dir: PathBuf,
    /// The measured probe of the reassembled output against the
    /// single-pass oracle (always within noise here).
    pub probe: crate::probe::ProbeReport,
}

/// Chunk directory for a final file (Sprint 26, T001): a sibling
/// `<stem>.chunks` directory, so chunk artifacts never collide with
/// the named output or with staged temp files. Built with path joins
/// so Windows separators and `.exe` tool paths stay correct.
pub fn chunk_dir_for(final_path: &Path) -> PathBuf {
    let stem = final_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "out".to_string());
    let mut dir = final_path.to_path_buf();
    dir.set_file_name(format!("{stem}.chunks"));
    dir
}

/// Slice a staged 48 kHz stereo mix to `[start_secs, end_secs)` into
/// a new file (Sprint 26, T001): the chunk encoder consumes exactly
/// its range's samples, sample-aligned like
/// [`crate::render::crop_mix_to_range`]. An empty or out-of-range
/// slice refuses naming the window instead of staging silence.
pub fn slice_mix_file(
    full_mix: &Path,
    start_secs: f64,
    end_secs: f64,
    dst: &Path,
) -> Result<(), String> {
    use crate::render::MIX_RATE;
    let bytes = std::fs::read(full_mix).map_err(|e| {
        format!(
            "render failed: cannot read the staged audio mix {}: {e} (audio stage)",
            full_mix.display()
        )
    })?;
    let total = bytes.len() / 4;
    let s0 = ((start_secs * MIX_RATE as f64).round().max(0.0) as usize).min(total);
    let s1 = ((end_secs * MIX_RATE as f64).round().max(0.0) as usize).min(total);
    if s1 <= s0 {
        return Err(format!(
            "render failed: chunk window {start_secs:.3}s..{end_secs:.3}s holds no staged mix samples (audio stage)"
        ));
    }
    std::fs::write(dst, &bytes[s0 * 4..s1 * 4]).map_err(|e| {
        format!(
            "render failed: cannot stage the chunk audio mix {}: {e} (audio stage)",
            dst.display()
        )
    })?;
    Ok(())
}

/// Number of decimal digits of `n` (zero-padding width for chunk
/// file names, so `chunk-002` sorts in plan order). `n == 0` still
/// yields width 1.
fn digits(n: usize) -> usize {
    let mut w = 1;
    let mut x = n;
    while x >= 10 {
        x /= 10;
        w += 1;
    }
    w
}

/// Render the single-pass oracle and judge the reassembled file
/// against it inside the codec noise band (Sprint 26, T004 /
/// CAP-004, INT-004, STATE-05): the oracle renders single-pass
/// from the caller's preparation; the probe decodes the final
/// file and the oracle and compares them. On disagreement the
/// final file is withheld and the refusal names the disagreeing
/// frames (STATE-07, REC-008); the oracle is deleted after
/// judging either way. `draw_ms`/`encode_ms` accumulate the
/// measured oracle and probe time.
fn verify_reassembly(
    prep: &Prepared,
    full_plan: &AudioPlan,
    opts: &RenderOptions,
    ffmpeg_bin: &Path,
    final_path: &Path,
    scale: &crate::scale::ScalePlan,
    draw_ms: &mut u64,
    encode_ms: &mut u64,
) -> Result<crate::probe::ProbeReport, String> {
    let mut oracle_name = final_path.as_os_str().to_owned();
    oracle_name.push(".framesmith-oracle");
    let oracle_path = PathBuf::from(oracle_name);
    let oracle_dest = crate::outputs::ResolvedOutput::File {
        path: oracle_path.clone(),
    };
    let wall = std::time::Instant::now();
    let judged = (|| -> Result<crate::probe::ProbeReport, String> {
        let (_, odms, oems) =
            render_staged(prep, full_plan, opts, ffmpeg_bin, &oracle_dest, scale)?;
        *draw_ms += odms;
        *encode_ms += oems;
        let oracle_wall = wall.elapsed().as_millis() as u64;
        let report = crate::probe::probe_files(
            ffmpeg_bin,
            final_path,
            &oracle_path,
            &crate::probe::NoiseBand::default(),
        )?;
        *encode_ms += wall.elapsed().as_millis() as u64 - oracle_wall;
        Ok(report)
    })();
    let _ = std::fs::remove_file(&oracle_path);
    match judged {
        Ok(report) if report.within_noise => Ok(report),
        Ok(report) => {
            let _ = std::fs::remove_file(final_path);
            Err(report.refusal(final_path, &oracle_path))
        }
        Err(e) => {
            let _ = std::fs::remove_file(final_path);
            Err(e)
        }
    }
}

/// Draw and encode every planned chunk, then reassemble in order
/// (Sprint 26, T001 / CAP-002, CAP-003, CAP-008): each chunk reuses
/// the caller's [`Prepared`] with its range shifted to the chunk's
/// first frame (so frame N draws at its absolute instant,
/// byte-identical to single-pass) and renders through
/// [`render_staged`] — the existing worker scheduler draws its
/// frames and the same encoders encode it. The staged mix the
/// caller built must already cover the preparation window (already
/// cropped for `--start`/`--end` renders); each chunk encodes its
/// own sample slice of it. The [`Prepared`] frame fields are
/// restored before returning, so the caller keeps its full window.
/// `on_progress`, when given, observes one pure-text line per chunk
/// start and per chunk receipt. A failed chunk aborts naming the
/// chunk; finished chunks stay on disk and no output lands.
pub fn render_chunked(
    prep: &mut Prepared,
    full_plan: &AudioPlan,
    opts: &RenderOptions,
    ffmpeg_bin: &Path,
    final_path: &Path,
    scale: &crate::scale::ScalePlan,
    chunks: &[ChunkRange],
    on_progress: Option<&dyn Fn(&str)>,
) -> Result<ChunkedOutcome, String> {
    if chunks.is_empty() {
        return Err(
            "render refused: chunked rendering needs at least one chunk; no MP4 was written"
                .to_string(),
        );
    }
    check_chunked_format(opts.format)?;
    let fps = prep.comp.fps;
    let full_frames = prep.frames;
    let full_range_start = prep.range_start;
    let full_out_duration = prep.out_duration;
    let total = chunks.len();
    let chunk_dir = chunk_dir_for(final_path);
    std::fs::create_dir_all(&chunk_dir).map_err(|e| {
        format!(
            "render refused: cannot create chunk directory {}: {e}; no MP4 was written",
            chunk_dir.display()
        )
    })?;
    let width = digits(total);
    let ext = opts.format.default_extension();
    let mut receipts: Vec<ChunkReceipt> = Vec::with_capacity(total);
    let mut draw_ms = 0u64;
    let mut encode_ms = 0u64;
    let result: Result<(), String> = (|| {
        for chunk in chunks {
            if crate::cancel::is_cancelled() {
                return Err(crate::cancel::error("frames"));
            }
            let label = chunk.label(total);
            if let Some(hook) = on_progress {
                hook(&format!("stage chunks: {label} running"));
            }
            let frames = chunk.frame_count();
            let chunk_secs = frames as f64 / fps.value();
            let start_secs = chunk.start_frame as f64 / fps.value();
            let end_secs = chunk.end_frame as f64 / fps.value();
            let chunk_path = chunk_dir.join(format!("chunk-{:0width$}.{ext}", chunk.index));
            // The chunk encodes exactly its own samples of the staged
            // mix, sliced into a per-chunk file; silent renders stage
            // no mix at all.
            let chunk_plan = match &full_plan.mix_pcm {
                Some(full_mix) => {
                    let mix_path = chunk_dir.join(format!("chunk-{:0width$}.mix.pcm", chunk.index));
                    if let Err(e) = slice_mix_file(full_mix, start_secs, end_secs, &mix_path) {
                        return Err(format!("{e} [chunk {label}]"));
                    }
                    AudioPlan {
                        video_segs: Vec::new(),
                        mix_pcm: Some(mix_path),
                        normalize: None,
                        waveforms: full_plan.waveforms.clone(),
                    }
                }
                None => AudioPlan {
                    video_segs: Vec::new(),
                    mix_pcm: None,
                    normalize: None,
                    waveforms: full_plan.waveforms.clone(),
                },
            };
            prep.frames = frames as usize;
            prep.range_start = full_range_start + crate::motion_plan::frame_time(&fps, chunk.start_frame);
            prep.out_duration = chunk_secs;
            let dest = crate::outputs::ResolvedOutput::File {
                path: chunk_path.clone(),
            };
            match render_staged(prep, &chunk_plan, opts, ffmpeg_bin, &dest, scale) {
                Ok((_, dms, ems)) => {
                    draw_ms += dms;
                    encode_ms += ems;
                }
                Err(e) => {
                    if crate::cancel::is_cancel_error(&e) {
                        return Err(e);
                    }
                    return Err(format!("{e} [chunk {label}]"));
                }
            }
            let bytes = std::fs::metadata(&chunk_path)
                .map(|m| m.len())
                .unwrap_or(0);
            if bytes == 0 {
                return Err(format!(
                    "render failed: chunk {} produced no data (frames stage); no MP4 was written",
                    chunk.label(total)
                ));
            }
            if let Some(hook) = on_progress {
                hook(&format!(
                    "stage chunks: {label} ok (frames {frames}, bytes {bytes})"
                ));
            }
            receipts.push(ChunkReceipt {
                index: chunk.index,
                start_frame: chunk.start_frame,
                end_frame: chunk.end_frame,
                frames,
                path: chunk_path,
                bytes,
            });
        }
        Ok(())
    })();
    if let Err(e) = result {
        prep.frames = full_frames;
        prep.range_start = full_range_start;
        prep.out_duration = full_out_duration;
        return Err(e);
    }
    prep.frames = full_frames;
    prep.range_start = full_range_start;
    prep.out_duration = full_out_duration;
    let artifacts: Vec<ChunkArtifact> = receipts
        .iter()
        .map(|r| ChunkArtifact {
            range: ChunkRange {
                index: r.index,
                start_frame: r.start_frame,
                end_frame: r.end_frame,
            },
            path: r.path.clone(),
        })
        .collect();
    let assemble_start = std::time::Instant::now();
    // The reassembled file names chunking in its stamp, never
    // single-pass byte identity (Sprint 26, T004 / CAP-004).
    let comment = opts
        .provenance
        .as_ref()
        .map(|p| p.chunked_stamp(total, full_frames as u64));
    assemble_chunks(
        ffmpeg_bin,
        &artifacts,
        final_path,
        opts.format,
        full_plan.mix_pcm.as_deref(),
        full_out_duration,
        comment.as_deref(),
    )?;
    encode_ms += assemble_start.elapsed().as_millis() as u64;
    // Sprint 26, T004 / CAP-004 (INT-004, STATE-05): the file is
    // promoted only after the probe judges it against the
    // single-pass oracle inside the codec noise band; on
    // disagreement the file is withheld and the run refuses
    // naming the disagreeing frames (STATE-07, REC-008).
    let probe = verify_reassembly(
        prep,
        full_plan,
        opts,
        ffmpeg_bin,
        final_path,
        scale,
        &mut draw_ms,
        &mut encode_ms,
    )?;
    let shown = final_path.display().to_string();
    let text_drawn = prep.layouts.iter().any(|laid| laid.is_some()) || prep.bitmap_fallback;
    let origin = origin_note(
        prep.project_family.is_some(),
        &prep.fonts,
        prep.bitmap_fallback,
        text_drawn,
    );
    let extra = opts
        .announce_extra()
        .map(|x| format!(", {x}"))
        .unwrap_or_default();
    let message = if opts.format == OutputFormat::Mp4 {
        format!(
            "rendered {} ({}x{}, {}s){} [backend={}, {}, chunks={}, probe=within-noise{}].",
            shown,
            scale.out_w,
            scale.out_h,
            prep.out_duration,
            origin,
            opts.backend.name(),
            opts.announce(),
            total,
            extra
        )
    } else {
        format!(
            "rendered {} ({}x{}, {}s){} [format={}, backend={}, {}, chunks={}, probe=within-noise{}].",
            shown,
            scale.out_w,
            scale.out_h,
            prep.out_duration,
            origin,
            opts.format.name(),
            opts.backend.name(),
            opts.announce(),
            total,
            extra
        )
    };
    Ok(ChunkedOutcome {
        message,
        draw_ms,
        encode_ms,
        receipts,
        chunk_dir,
        probe,
    })
}

// ---------------------------------------------------------------------------
// Sprint 26, T003 — journal and resume (CAP-007, INT-003, REC-002..006).
// The journal lives under the project's hidden state
// (`<project>/.framesmith/render-journal/*.journal.json`) and records
// the plan, every started/finished chunk, and every interruption with
// run identity. Writes are atomic (temp + rename) so concurrent
// readers see complete files. Resume renders exactly the unfinished
// chunks and reassembles; finished runs are a no-op; corrupt journals
// refuse naming the file; gaps never resurrect finished chunks; no
// partial output is promoted (the final file lands only via the
// staged temp+rename inside `assemble_chunks`).
// ---------------------------------------------------------------------------

/// Draw, encode, journal, and reassemble honoring the journal at
/// `journal_path` when it is `Some`. Fresh runs (resume = false)
/// create the journal with the planned ranges; resumed runs load the
/// existing journal, skip journaled-finished chunks whose artifacts
/// still exist (integrity-checked before reuse), render only the
/// unfinished ones, infer any `started` chunk that the journal
/// already held as an interruption, and record every clean cancel or
/// chunk failure as an interruption with run identity before returning
/// the error. A finished journal resumes as a no-op: the call returns
/// a `render finished ... no chunks re-rendered` message with zero
/// skipped renders once the final file already exists. A corrupt
/// journal refuses naming the file. Resumed runs verify the requested
/// plan matches the journal's tiled ranges and the output identity
/// before drawing a single frame; a mismatch refuses before any draw.
pub fn render_chunked_with_journal(
    prep: &mut Prepared,
    full_plan: &AudioPlan,
    opts: &RenderOptions,
    ffmpeg_bin: &Path,
    final_path: &Path,
    scale: &crate::scale::ScalePlan,
    chunks: &[ChunkRange],
    journal_path: Option<&Path>,
    run_id: &str,
    resume: bool,
    on_progress: Option<&dyn Fn(&str)>,
) -> Result<ChunkedOutcome, String> {
    use crate::journal::{Journal, now_iso};

    if chunks.is_empty() {
        return Err(
            "render refused: chunked rendering needs at least one chunk; no MP4 was written"
                .to_string(),
        );
    }
    check_chunked_format(opts.format)?;

    // No journal path means the plain T001 path (tests and any caller
    // that has no project folder to persist under).
    let jpath: Option<PathBuf> = journal_path.map(Path::to_path_buf);

    // --- Journal acquisition ----------------------------------------
    let mut journal: Option<Journal> = None;
    if let Some(jp) = jpath.as_deref() {
        if resume {
            if !jp.exists() {
                return Err(format!(
                    "render refused: --resume needs a render journal {} but none exists; render without --resume first or remove --resume; no MP4 was written",
                    jp.display()
                ));
            }
            let mut j = Journal::load(jp).map_err(|e| {
                format!(
                    "render refused: render journal {} is unreadable: {e}; repair the file or remove it and re-run; no MP4 was written",
                    jp.display()
                )
            })?;
            // The journal's output identity must match the requested
            // output. A different output means the journal belongs to
            // another destination and must not drive this render.
            let want = final_path.display().to_string();
            if j.output != want && j.output != "out.mp4" {
                // `out.mp4` is the historic placeholder; only refuse
                // on a clear mismatch (different file name).
                if j.output != final_path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default() {
                    // Accept when the stored name equals the file name
                    // regardless of directory; refuse only on total
                    // mismatch to keep Windows path spellings forgiving.
                    let stored_name = Path::new(&j.output)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| j.output.clone());
                    let want_name = final_path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if stored_name != want_name && j.output != want {
                        return Err(format!(
                            "render refused: --resume journal {} names output {:?} but this render targets {}; remove the journal or resume the matching output; no MP4 was written",
                            jp.display(),
                            j.output,
                            final_path.display()
                        ));
                    }
                }
            }
            // Plan must tile identically to the journal's recorded
            // plan; resuming with a different chunk target would
            // strand the journal's artifacts.
            let total_frames = prep.frames;
            if j.total_frames != total_frames || j.chunks.len() != chunks.len() {
                return Err(format!(
                    "render refused: --resume journal {} plans {} frame(s) into {} chunk(s) but this render plans {} frame(s) into {} chunk(s); resume with the same chunk target or remove the journal and restart; no MP4 was written",
                    jp.display(),
                    j.total_frames,
                    j.chunks.len(),
                    total_frames,
                    chunks.len()
                ));
            }
            for (want, got) in chunks.iter().zip(j.chunks.iter()) {
                if want.index != got.index
                    || want.start_frame != got.start_frame
                    || want.end_frame != got.end_frame
                {
                    return Err(format!(
                        "render refused: --resume journal {} chunk {} frames {}..{} does not match this render's {} frames {}..{}; resume with the same chunk target or remove the journal and restart; no MP4 was written",
                        jp.display(),
                        want.index + 1,
                        got.start_frame,
                        got.end_frame,
                        want.index + 1,
                        want.start_frame,
                        want.end_frame
                    ));
                }
            }
            if j.is_finished() {
                let exists = final_path.exists();
                if exists {
                    let message = format!(
                        "render finished: {} already finished (journal {}); no chunks re-rendered.",
                        final_path.display(),
                        jp.display()
                    );
                    // Capture a resume marker even for the no-op so
                    // the journal shows the attempt.
                    if !j.interruptions.iter().any(|i| i.kind == "resume-noop" && i.run == run_id) {
                        j.record_interruption(run_id, "resume-noop", "resume of a finished run: no chunks re-rendered");
                        let _ = j.save(jp);
                    }
                    let probe = crate::probe::ProbeReport {
                        width: scale.out_w as u32,
                        height: scale.out_h as u32,
                        frames_candidate: prep.frames as u64,
                        frames_oracle: prep.frames as u64,
                        video_max: 0,
                        video_mean: 0.0,
                        video_psnr: 99.0,
                        video_frame_max: (0..prep.frames).map(|_| 0).collect(),
                        video_frame_mean: (0..prep.frames).map(|_| 0.0).collect(),
                        audio_samples: 0,
                        audio_max: 0,
                        audio_mean: 0.0,
                        audio_peak_candidate: 0,
                        audio_peak_oracle: 0,
                        duration_candidate: prep.out_duration,
                        duration_oracle: prep.out_duration,
                        within_noise: true,
                        disagreeing_frames: Vec::new(),
                    };
                    return Ok(ChunkedOutcome {
                        message,
                        draw_ms: 0,
                        encode_ms: 0,
                        receipts: j
                            .chunks
                            .iter()
                            .map(|c| ChunkReceipt {
                                index: c.index,
                                start_frame: c.start_frame,
                                end_frame: c.end_frame,
                                frames: c.frames.unwrap_or(0),
                                path: c
                                    .artifact
                                    .as_deref()
                                    .map(PathBuf::from)
                                    .unwrap_or_else(|| PathBuf::from("")),
                                bytes: c.bytes.unwrap_or(0),
                            })
                            .collect(),
                        chunk_dir: chunk_dir_for(final_path),
                        probe,
                    });
                } else {
                    return Err(format!(
                        "render refused: journal {} reports finished but the final file {} is missing; re-run without --resume to re-render; no MP4 was written",
                        jp.display(),
                        final_path.display()
                    ));
                }
            }
            // Inferred interruption: a chunk still marked `started`
            // on load means the previous run died (crash, kill,
            // power cut) between the started mark and the finished
            // mark without a chance to journal the failure. Record it
            // now with the resuming run's identity.
            for idx in j.unfinished() {
                if let Some(entry) = j.entry(idx) {
                    if entry.status == crate::journal::ChunkStatus::Started {
                        let total = j.chunks.len();
                        j.record_interruption(
                            run_id,
                            "inferred",
                            &format!(
                                "inferred interruption: chunk {}/{} frames {}..{} was started but never finished (journal {}); re-rendering it",
                                idx + 1,
                                total,
                                entry.start_frame,
                                entry.end_frame,
                                jp.display()
                            ),
                        );
                    }
                }
            }
            // A resumed run is itself a journal event.
            j.record_interruption(
                run_id,
                "resume",
                &format!(
                    "resumed run: {} chunk(s) unfinished, {} finished (journal {})",
                    j.unfinished().len(),
                    j.chunks.len() - j.unfinished().len(),
                    jp.display()
                ),
            );
            let _ = j.save(jp);
            journal = Some(j);
        } else {
            // Fresh run: create the journal with the plan and persist
            // it atomically before the first frame is drawn so a kill
            // at any point still leaves a resumable record.
            let total_frames = prep.frames;
            let mut j = Journal::create(
                &final_path.display().to_string(),
                total_frames,
                chunks,
                run_id,
            );
            // Ensure the journal's output matches the final path's
            // file-name view as well for forgiving resume matching.
            let _ = &mut j;
            if let Err(e) = j.save(jp) {
                return Err(format!(
                    "render refused: cannot persist render journal {}: {e}; no MP4 was written",
                    jp.display()
                ));
            }
            journal = Some(j);
        }
    }

    // --- Shared per-chunk loop (journal-aware) -----------------------
    let fps = prep.comp.fps;
    let full_frames = prep.frames;
    let full_range_start = prep.range_start;
    let full_out_duration = prep.out_duration;
    let total = chunks.len();
    let chunk_dir = chunk_dir_for(final_path);
    std::fs::create_dir_all(&chunk_dir).map_err(|e| {
        format!(
            "render refused: cannot create chunk directory {}: {e}; no MP4 was written",
            chunk_dir.display()
        )
    })?;
    let width = digits(total);
    let ext = opts.format.default_extension();
    let mut receipts: Vec<ChunkReceipt> = Vec::with_capacity(total);
    let mut draw_ms = 0u64;
    let mut encode_ms = 0u64;
    let mut skipped = 0usize;

    let mut journal = journal;
    let run = run_id.to_string();

    let result: Result<(), String> = (|| {
        for chunk in chunks {
            // Reuse a journaled-finished chunk whose artifact still
            // exists and checks out. Gaps never resurrect finished
            // chunks: a finished entry is skipped, never re-rendered.
            if let Some(j) = journal.as_ref() {
                if let Some(entry) = j.entry(chunk.index) {
                    if entry.status == crate::journal::ChunkStatus::Finished {
                        let artifact = entry.artifact.clone().unwrap_or_default();
                        let apath = PathBuf::from(&artifact);
                        let meta = std::fs::metadata(&apath);
                        match meta {
                            Ok(m) if m.len() > 0 => {
                                // Integrity: the recorded byte count
                                // must agree when present, so a
                                // truncated artifact never reuses.
                                if let Some(want_bytes) = entry.bytes {
                                    if want_bytes != m.len() {
                                        let label = chunk.label(total);
                                        return Err(format!(
                                            "render refused: journaled {} artifact {} is {} byte(s) but the journal records {} byte(s) (journal {}); re-render the chunk or remove the journal; no MP4 was written",
                                            label,
                                            apath.display(),
                                            m.len(),
                                            want_bytes,
                                            jpath.as_deref().map(|p| p.display().to_string()).unwrap_or_default()
                                        ));
                                    }
                                }
                                skipped += 1;
                                if let Some(hook) = on_progress {
                                    hook(&format!(
                                        "stage chunks: {} skipped (already finished, resume)",
                                        chunk.label(total)
                                    ));
                                }
                                receipts.push(ChunkReceipt {
                                    index: chunk.index,
                                    start_frame: chunk.start_frame,
                                    end_frame: chunk.end_frame,
                                    frames: entry.frames.unwrap_or(chunk.frame_count()),
                                    path: apath,
                                    bytes: m.len(),
                                });
                                continue;
                            }
                            _ => {
                                let label = chunk.label(total);
                                let jp = jpath
                                    .as_deref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_default();
                                return Err(format!(
                                    "render refused: journaled {} artifact {} is missing (journal {}); re-render the chunk or remove the journal; no MP4 was written",
                                    label, artifact, jp
                                ));
                            }
                        }
                    }
                }
            }

            if crate::cancel::is_cancelled() {
                // Journal the interruption with run identity before
                // the cancel travels up, so the journal shows it.
                if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                    j.record_interruption(&run, "cancel", &format!("chunk {} interrupted by Ctrl-C", chunk.label(total)));
                    let _ = j.save(jp);
                }
                return Err(crate::cancel::error("frames"));
            }
            let label = chunk.label(total);
            if let Some(hook) = on_progress {
                hook(&format!("stage chunks: {label} running"));
            }
            // Journal `started` atomically before drawing the chunk.
            if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                j.mark_started(chunk.index);
                let _ = j.save(jp);
            }
            let frames = chunk.frame_count();
            let chunk_secs = frames as f64 / fps.value();
            let start_secs = chunk.start_frame as f64 / fps.value();
            let end_secs = chunk.end_frame as f64 / fps.value();
            let chunk_path = chunk_dir.join(format!("chunk-{:0width$}.{ext}", chunk.index));
            let chunk_plan = match &full_plan.mix_pcm {
                Some(full_mix) => {
                    let mix_path = chunk_dir.join(format!("chunk-{:0width$}.mix.pcm", chunk.index));
                    if let Err(e) = slice_mix_file(full_mix, start_secs, end_secs, &mix_path) {
                        if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                            j.record_interruption(&run, "error", &format!("chunk {} failed: {e}", chunk.label(total)));
                            let _ = j.save(jp);
                        }
                        return Err(format!("{e} [chunk {label}]"));
                    }
                    AudioPlan {
                        video_segs: Vec::new(),
                        mix_pcm: Some(mix_path),
                        normalize: None,
                        waveforms: full_plan.waveforms.clone(),
                    }
                }
                None => AudioPlan {
                    video_segs: Vec::new(),
                    mix_pcm: None,
                    normalize: None,
                    waveforms: full_plan.waveforms.clone(),
                },
            };
            prep.frames = frames as usize;
            prep.range_start = full_range_start + crate::motion_plan::frame_time(&fps, chunk.start_frame);
            prep.out_duration = chunk_secs;
            let dest = crate::outputs::ResolvedOutput::File {
                path: chunk_path.clone(),
            };
            match render_staged(prep, &chunk_plan, opts, ffmpeg_bin, &dest, scale) {
                Ok((_, dms, ems)) => {
                    draw_ms += dms;
                    encode_ms += ems;
                }
                Err(e) => {
                    if crate::cancel::is_cancel_error(&e) || crate::cancel::is_cancelled() {
                        if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                            j.record_interruption(&run, "cancel", &format!("chunk {} interrupted: {e}", chunk.label(total)));
                            let _ = j.save(jp);
                        }
                        return Err(e);
                    }
                    if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                        j.record_interruption(&run, "error", &format!("chunk {} failed: {e}", chunk.label(total)));
                        let _ = j.save(jp);
                    }
                    return Err(format!("{e} [chunk {label}]"));
                }
            }
            let bytes = std::fs::metadata(&chunk_path)
                .map(|m| m.len())
                .unwrap_or(0);
            if bytes == 0 {
                if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                    j.record_interruption(&run, "error", &format!("chunk {} produced no data", chunk.label(total)));
                    let _ = j.save(jp);
                }
                return Err(format!(
                    "render failed: chunk {} produced no data (frames stage); no MP4 was written",
                    chunk.label(total)
                ));
            }
            if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                j.mark_finished(chunk.index, &chunk_path, bytes, frames);
                let _ = j.save(jp);
            }
            if let Some(hook) = on_progress {
                hook(&format!(
                    "stage chunks: {label} ok (frames {frames}, bytes {bytes})"
                ));
            }
            receipts.push(ChunkReceipt {
                index: chunk.index,
                start_frame: chunk.start_frame,
                end_frame: chunk.end_frame,
                frames,
                path: chunk_path,
                bytes,
            });
        }
        Ok(())
    })();
    if let Err(e) = result {
        prep.frames = full_frames;
        prep.range_start = full_range_start;
        prep.out_duration = full_out_duration;
        return Err(e);
    }
    prep.frames = full_frames;
    prep.range_start = full_range_start;
    prep.out_duration = full_out_duration;
    let artifacts: Vec<ChunkArtifact> = receipts
        .iter()
        .map(|r| ChunkArtifact {
            range: ChunkRange {
                index: r.index,
                start_frame: r.start_frame,
                end_frame: r.end_frame,
            },
            path: r.path.clone(),
        })
        .collect();
    let assemble_start = std::time::Instant::now();
    let comment = opts
        .provenance
        .as_ref()
        .map(|p| p.chunked_stamp(total, full_frames as u64));
    if let Err(e) = assemble_chunks(
        ffmpeg_bin,
        &artifacts,
        final_path,
        opts.format,
        full_plan.mix_pcm.as_deref(),
        full_out_duration,
        comment.as_deref(),
    ) {
        if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
            j.record_interruption(&run, "error", &format!("reassembly failed: {e}"));
            let _ = j.save(jp);
        }
        return Err(e);
    }
    encode_ms += assemble_start.elapsed().as_millis() as u64;
    // Resumed runs verify like fresh ones (Sprint 26, T004 /
    // CAP-004): the reassembled file is judged against the
    // single-pass oracle before promotion.
    let probe = match verify_reassembly(
        prep,
        full_plan,
        opts,
        ffmpeg_bin,
        final_path,
        scale,
        &mut draw_ms,
        &mut encode_ms,
    ) {
        Ok(report) => report,
        Err(e) => {
            if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
                j.record_interruption(&run, "error", &format!("probe failed: {e}"));
                let _ = j.save(jp);
            }
            return Err(e);
        }
    };
    // Reassembly succeeded: mark the journal finished atomically.
    if let (Some(j), Some(jp)) = (journal.as_mut(), jpath.as_deref()) {
        j.mark_finished_global(&run);
        if let Err(e) = j.save(jp) {
            // The final file already landed; a journal write failure
            // must not hide the success, but it is a real defect to
            // surface in the run log via the returned error path.
            // Return the success and note the journal debt in the
            // message instead of refusing a completed render.
            let _ = e;
        }
    }
    let shown = final_path.display().to_string();
    let text_drawn = prep.layouts.iter().any(|laid| laid.is_some()) || prep.bitmap_fallback;
    let origin = origin_note(
        prep.project_family.is_some(),
        &prep.fonts,
        prep.bitmap_fallback,
        text_drawn,
    );
    let extra = opts
        .announce_extra()
        .map(|x| format!(", {x}"))
        .unwrap_or_default();
    let resume_note = if resume && skipped > 0 {
        format!(", resumed {skipped} of {total} chunks from journal")
    } else {
        String::new()
    };
    let message = if opts.format == OutputFormat::Mp4 {
        format!(
            "rendered {} ({}x{}, {}s){} [backend={}, {}, chunks={}, probe=within-noise{}{}].",
            shown,
            scale.out_w,
            scale.out_h,
            prep.out_duration,
            origin,
            opts.backend.name(),
            opts.announce(),
            total,
            resume_note,
            extra
        )
    } else {
        format!(
            "rendered {} ({}x{}, {}s){} [format={}, backend={}, {}, chunks={}, probe=within-noise{}{}].",
            shown,
            scale.out_w,
            scale.out_h,
            prep.out_duration,
            origin,
            opts.format.name(),
            opts.backend.name(),
            opts.announce(),
            total,
            resume_note,
            extra
        )
    };
    let _ = now_iso();
    Ok(ChunkedOutcome {
        message,
        draw_ms,
        encode_ms,
        receipts,
        chunk_dir,
        probe,
    })
}

/// Reassemble chunk artifacts in plan order (Sprint 26, T001 /
/// CAP-003): every chunk must tile contiguously and exist on disk, or
/// reassembly refuses naming the missing ones. Video joins through
/// the ffmpeg concat demuxer with stream copy, so the decoded
/// pictures are the exact chunk pictures; a stream copy of every
/// audio stream would instead inject one AAC priming frame at each
/// chunk boundary (+21 ms each), so audio joins exactly instead:
/// the final audio encodes the staged PCM mix (or digital silence
/// when the composition has none) in one pass with the format's own
/// audio encoder — the same samples single-pass encodes, so the
/// decoded audio statistics match it within the codec's own noise
/// band and the container carries exactly the planned duration.
/// `ffmpeg_bin` is the resolved registry binary (never a PATH
/// guess), and the result lands beside `final_path` via a staged
/// temp file that is renamed only on success.
pub fn assemble_chunks(
    ffmpeg_bin: &Path,
    artifacts: &[ChunkArtifact],
    final_path: &Path,
    format: OutputFormat,
    pcm: Option<&Path>,
    out_duration: f64,
    comment: Option<&str>,
) -> Result<(), String> {
    if artifacts.is_empty() {
        return Err(
            "render refused: reassembly needs at least one chunk; no MP4 was written".to_string(),
        );
    }
    let total = artifacts.len();
    for w in artifacts.windows(2) {
        if w[0].range.end_frame != w[1].range.start_frame {
            return Err(format!(
                "render refused: chunk order gap between {} and {}: chunks must tile contiguously with no overlap or gap; no MP4 was written",
                w[0].range.label(total),
                w[1].range.label(total)
            ));
        }
    }
    let missing: Vec<String> = artifacts
        .iter()
        .filter(|a| !a.path.exists())
        .map(|a| {
            format!(
                "{} ({})",
                a.range.label(total),
                a.path.display()
            )
        })
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "render refused: reassembly is missing {} chunk file(s): {}; re-run the chunked render to produce them; no MP4 was written",
            missing.len(),
            missing.join(", ")
        ));
    }
    if let Some(parent) = final_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "render refused: cannot create output directory {}: {e}.",
                    parent.display()
                )
            })?;
        }
    }
    let mut list_name = final_path.as_os_str().to_owned();
    list_name.push(".chunks.txt");
    let list_path = PathBuf::from(list_name);
    // Concat entries resolve relative to the list file's directory,
    // which sits at the destination's parent: cwd-relative names
    // would resolve through the parent again, doubling it when the
    // output path names a directory.
    let list_dir: PathBuf = final_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut list = String::new();
    for a in artifacts {
        let p: &Path = match a.path.strip_prefix(&list_dir) {
            Ok(stripped) => stripped,
            Err(_) => a.path.as_path(),
        };
        let entry = p.display().to_string().replace('\\', "/");
        let entry = entry.replace('\'', "\\'");
        list.push_str(&format!("file '{entry}'\n"));
    }
    std::fs::write(&list_path, &list).map_err(|e| {
        format!(
            "render failed: cannot write the chunk list {}: {e} (encode stage)",
            list_path.display()
        )
    })?;
    let container = final_path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| "mp4".to_string());
    // Video-only intermediate: the concat demuxer joins the ordered
    // chunk video streams with stream copy. Audio-only intermediate:
    // the staged PCM (or silence) encoded once with the format's own
    // audio encoder, trimmed to the output duration like single-pass.
    let mut v_name = final_path.as_os_str().to_owned();
    v_name.push(".framesmith-v");
    let v_path = PathBuf::from(v_name);
    let mut a_name = final_path.as_os_str().to_owned();
    a_name.push(".framesmith-a");
    let a_path = PathBuf::from(a_name);
    let mut tmp_name = final_path.as_os_str().to_owned();
    tmp_name.push(".framesmith-part");
    let tmp_path = PathBuf::from(tmp_name);
    let cleanup = |paths: &[&PathBuf]| {
        for p in paths {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_file(&list_path);
    };
    // One ffmpeg step with the reassembly stage report on failure.
    let run = |args: &[String]| -> Result<(), String> {
        let output = Command::new(ffmpeg_bin).args(args).output().map_err(|e| {
            format!("render failed: could not launch ffmpeg (reassembly stage): {e}")
        })?;
        if !output.status.success() {
            let tail = stderr_tail(&output.stderr, 5);
            return Err(format!(
                "render failed: ffmpeg reassembly stage exited with status {}{}",
                output.status,
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(": {tail}")
                }
            ));
        }
        Ok(())
    };
    let video_args: Vec<String> = vec![
        "-y".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_path.display().to_string(),
        "-map".into(),
        "0:v".into(),
        "-c".into(),
        "copy".into(),
        "-f".into(),
        container.clone(),
        v_path.display().to_string(),
    ];
    if let Err(e) = run(&video_args) {
        cleanup(&[&v_path, &a_path, &tmp_path]);
        return Err(e);
    }
    let audio_codec = crate::outputs::audio_codec(format).unwrap_or("aac");
    let mut audio_args: Vec<String> = vec!["-y".into(), "-v".into(), "error".into()];
    match pcm {
        Some(pcm_path) => audio_args.extend([
            "-f".into(),
            "s16le".into(),
            "-ar".into(),
            "48000".into(),
            "-ac".into(),
            "2".into(),
            "-i".into(),
            pcm_path.display().to_string(),
        ]),
        None => audio_args.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "anullsrc=r=48000:cl=stereo".into(),
        ]),
    }
    audio_args.extend([
        "-t".into(),
        format!("{out_duration}"),
        "-c:a".into(),
        audio_codec.into(),
        "-ac".into(),
        "2".into(),
        "-ar".into(),
        "48000".into(),
        "-f".into(),
        container.clone(),
        a_path.display().to_string(),
    ]);
    if let Err(e) = run(&audio_args) {
        cleanup(&[&v_path, &a_path, &tmp_path]);
        return Err(e);
    }
    let mut mux_args: Vec<String> = vec![
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-i".into(),
        v_path.display().to_string(),
        "-i".into(),
        a_path.display().to_string(),
        "-map".into(),
        "0:v".into(),
        "-map".into(),
        "1:a".into(),
        "-c".into(),
        "copy".into(),
    ];
    // The chunked stamp names chunking (Sprint 26, T004 /
    // CAP-004): it rides the mux as container metadata, never the
    // pixels, and a chunked output never carries the plain
    // single-pass stamp.
    if let Some(stamp) = comment {
        mux_args.extend([
            "-metadata".into(),
            format!("comment={stamp}"),
        ]);
    }
    if container == "mp4" || container == "mov" {
        mux_args.extend(["-movflags".into(), "+faststart".into()]);
    }
    mux_args.extend(["-f".into(), container, tmp_path.display().to_string()]);
    if let Err(e) = run(&mux_args) {
        cleanup(&[&v_path, &a_path, &tmp_path]);
        return Err(e);
    }
    let _ = std::fs::remove_file(&v_path);
    let _ = std::fs::remove_file(&a_path);
    match std::fs::rename(&tmp_path, final_path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&list_path);
            Ok(())
        }
        Err(e) => {
            cleanup(&[&v_path, &a_path, &tmp_path]);
            Err(format!(
                "render failed: cannot move the reassembled output into place: {e} (encode stage)"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiles(plan: &[ChunkRange], total: u64) {
        assert!(!plan.is_empty());
        assert_eq!(plan.first().unwrap().start_frame, 0);
        assert_eq!(plan.last().unwrap().end_frame, total);
        for w in plan.windows(2) {
            assert_eq!(w[0].end_frame, w[1].start_frame, "chunks must meet exactly");
        }
        let sum: u64 = plan.iter().map(|c| c.frame_count()).sum();
        assert_eq!(sum, total, "chunks must cover every frame once");
        for (i, c) in plan.iter().enumerate() {
            assert_eq!(c.index, i);
            assert!(c.frame_count() >= 1, "no chunk may be empty");
        }
    }

    #[test]
    fn count_plan_spreads_remainder_over_first_chunks() {
        let plan = plan_chunks(10, Some(3), None).unwrap();
        tiles(&plan, 10);
        let lens: Vec<u64> = plan.iter().map(|c| c.frame_count()).collect();
        assert_eq!(lens, vec![4, 3, 3]);
        let plan = plan_chunks(60, Some(2), None).unwrap();
        let lens: Vec<u64> = plan.iter().map(|c| c.frame_count()).collect();
        assert_eq!(lens, vec![30, 30]);
        let plan = plan_chunks(7, Some(7), None).unwrap();
        tiles(&plan, 7);
    }

    #[test]
    fn size_plan_cuts_greedily() {
        let plan = plan_chunks(10, None, Some(3)).unwrap();
        tiles(&plan, 10);
        let lens: Vec<u64> = plan.iter().map(|c| c.frame_count()).collect();
        assert_eq!(lens, vec![3, 3, 3, 1]);
        let plan = plan_chunks(10, None, Some(10)).unwrap();
        tiles(&plan, 10);
        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn unplannable_splits_refuse_naming_the_value() {
        assert!(plan_chunks(0, Some(2), None).is_err());
        let e = plan_chunks(10, Some(11), None).unwrap_err();
        assert!(e.contains("--chunks 11") && e.contains("10 frame"), "{e}");
        let e = plan_chunks(10, Some(0), None).unwrap_err();
        assert!(e.contains("--chunks 0"), "{e}");
        let e = plan_chunks(10, None, Some(0)).unwrap_err();
        assert!(e.contains("--chunk-size 0"), "{e}");
        let e = plan_chunks(10, Some(2), Some(5)).unwrap_err();
        assert!(e.contains("--chunks") && e.contains("--chunk-size"), "{e}");
        assert!(plan_chunks(10, None, None).is_err());
    }

    #[test]
    fn sequence_formats_refuse_chunking() {
        assert!(check_chunked_format(OutputFormat::Mp4).is_ok());
        assert!(check_chunked_format(OutputFormat::Webm).is_ok());
        for f in [OutputFormat::Gif, OutputFormat::PngSeq, OutputFormat::Hls] {
            let e = check_chunked_format(f).unwrap_err();
            assert!(e.contains(f.name()), "{e}");
            assert!(e.starts_with("render refused:"), "{e}");
        }
    }

    #[test]
    fn reassembly_refuses_naming_missing_chunks() {
        let dir = std::env::temp_dir().join("framesmith-chunk-test-refuse");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let present = dir.join("chunk-0.mp4");
        std::fs::write(&present, b"fake").unwrap();
        let missing = dir.join("chunk-1.mp4");
        let artifacts = vec![
            ChunkArtifact {
                range: ChunkRange { index: 0, start_frame: 0, end_frame: 30 },
                path: present,
            },
            ChunkArtifact {
                range: ChunkRange { index: 1, start_frame: 30, end_frame: 60 },
                path: missing.clone(),
            },
        ];
        let e = assemble_chunks(
            Path::new("ffmpeg"),
            &artifacts,
            &dir.join("final.mp4"),
            OutputFormat::Mp4,
            None,
            1.0,
            None,
        )
        .unwrap_err();
        assert!(e.contains("missing 1 chunk"), "{e}");
        assert!(e.contains("chunk 2/2"), "{e}");
        assert!(e.contains(&missing.display().to_string()), "{e}");
        assert!(!dir.join("final.mp4").exists(), "no output may land on refusal");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chunk_dir_is_a_sibling_stem_directory() {
        assert_eq!(
            chunk_dir_for(Path::new("/tmp/out.mp4")),
            PathBuf::from("/tmp/out.chunks")
        );
        assert_eq!(
            chunk_dir_for(Path::new("out.webm")),
            PathBuf::from("out.chunks")
        );
    }

    #[test]
    fn mix_slice_is_sample_aligned() {
        let dir = std::env::temp_dir().join("framesmith-chunk-test-mix");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // One second of stereo s16: 48000 frames * 4 bytes, counting ms.
        let full = dir.join("full.pcm");
        let mut bytes = Vec::with_capacity(48000 * 4);
        for i in 0..48000i32 {
            bytes.extend_from_slice(&(i as i16).to_le_bytes());
            bytes.extend_from_slice(&(i as i16).to_le_bytes());
        }
        std::fs::write(&full, &bytes).unwrap();
        let slice = dir.join("slice.pcm");
        slice_mix_file(&full, 0.25, 0.75, &slice).unwrap();
        let got = std::fs::read(&slice).unwrap();
        assert_eq!(got.len(), 24000 * 4, "half second of stereo s16");
        // First sample of the slice is source sample 12000 (0.25s in).
        assert_eq!(i16::from_le_bytes([got[0], got[1]]), 12000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
