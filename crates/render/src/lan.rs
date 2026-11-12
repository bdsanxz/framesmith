//! Local-network chunk distribution (Sprint 26, T002 / CAP-005,
//! CAP-006, CAP-002, CAP-008): a stateless worker contract over TCP.
//! A coordinator dispatches chunk specs to explicitly invited
//! workers; each worker renders exactly its spec (range, source,
//! quality, format) and returns its chunk artifact plus a receipt,
//! requiring no other chunk's state. The coordinator reassembles
//! returned artifacts through the T001 path
//! ([`crate::chunk::assemble_chunks`]) and reports which chunk ran
//! where. Unreachable workers are reported and their in-flight
//! chunks redistribute to the remaining workers; a worker that is
//! never invited never joins.
//!
//! Wire framing: one TCP connection carries one request and one
//! reply. Requests are `[u32 header length][JSON ChunkSpec][raw
//! staged-PCM mix slice]`, replies are `[u32 length][JSON
//! WorkerResponse][raw chunk artifact]`, all big-endian length
//! prefixes. Each side resolves its own FFmpeg from the pinned
//! build registry (the caller injects the resolved path), so
//! Windows path joins and `.exe` binaries behave per host.

use crate::chunk::{ChunkRange, ChunkReceipt, assemble_chunks, chunk_dir_for, slice_mix_file};
use crate::outputs::OutputFormat;
use crate::render::{AudioPlan, Prepared, RenderOptions, prepare_frames_ranged, render_staged};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar};
use std::time::{Duration, Instant};

/// Wire contract version sent in every [`ChunkSpec`]. A mismatch is
/// refused by the worker naming the version, never rendered.
pub const LAN_WIRE_VERSION: &str = "1";

/// Stall hook for tests only (Sprint 26, T002): when this
/// environment variable names milliseconds, a worker sleeps that
/// long after reading a request and before rendering it, so a
/// coordinator test can kill the worker mid-chunk. Unset in real
/// runs (zero delay).
const STALL_ENV: &str = "FRAMESMITH_WORKER_STALL_MS";

/// Default per-chunk dispatch timeout (milliseconds). The
/// coordinator never waits on a worker longer than this per chunk
/// attempt; `FRAMESMITH_WORKER_TIMEOUT_MS` overrides it for slow
/// machines (for example a first worker still downloading its
/// pinned FFmpeg build — the worker resolves FFmpeg per request).
pub const WORKER_TIMEOUT_MS_ENV: &str = "FRAMESMITH_WORKER_TIMEOUT_MS";
pub const DEFAULT_TIMEOUT_MS: u64 = 600_000;
/// Connect timeout for an invited worker address. A refused or
/// silent address is reported unreachable instead of waited on.
pub const CONNECT_TIMEOUT_MS: u64 = 5_000;
/// Largest accepted wire header (specs carry a source file, never
/// media bytes — media rides the raw slice/artifact frames).
const MAX_HEADER: usize = 32 * 1024 * 1024;

/// One chunk of work for a worker (Sprint 26, T002 / CAP-005): the
/// complete, self-contained spec a worker renders from alone.
/// The worker needs no other chunk's state; everything it draws
/// with is here. The staged-mix slice covers exactly this range's
/// samples, sample-aligned like the in-process path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSpec {
    pub version: String,
    /// Invitation secret. The worker refuses any request whose
    /// token does not match the one it was started with, so an
    /// uninvited machine never renders.
    pub token: String,
    /// Zero-based position of this chunk inside the plan.
    pub index: usize,
    /// Plan size, for receipt text (`chunk 2/5 frames 30..60`).
    pub total: usize,
    /// First frame this chunk renders (inclusive).
    pub start_frame: u64,
    /// First frame after this chunk (exclusive).
    pub end_frame: u64,
    /// Total prepared frames, for worker sanity checks.
    pub total_frames: usize,
    /// Raw `composition.kdl` source of the project. The worker
    /// validates its own copy before drawing anything.
    pub composition_source: String,
    /// Selected composition name (multi-composition projects).
    pub composition_name: String,
    /// Raw `--variables` payload (`None` when absent).
    pub variables_json: Option<String>,
    pub strict_variables: bool,
    pub theme: Option<String>,
    /// `--start`/`--end` composition-time window the preparation
    /// covers (`None`/`None` renders the whole composition).
    pub range_start: Option<f64>,
    pub range_end: Option<f64>,
    /// Effective render choices, as names the worker parses back:
    /// backend (`cpu`; `gpu`/`auto` refuse on workers), preset,
    /// format, resolution, encoder choice, HDR mode and transfer.
    pub backend: String,
    pub preset: String,
    pub crf_override: Option<u8>,
    pub video_bitrate: Option<String>,
    pub format: String,
    pub prores_profile: String,
    pub resolution: String,
    pub supersample: u8,
    pub encoder_choice: String,
    pub keyint: Option<u32>,
    pub maxrate: Option<String>,
    pub hdr: String,
    pub hdr_transfer: String,
    pub hdr_tone_mapped: bool,
    /// Provenance stamp (tool version plus the composition-inputs
    /// digest) the chunk video encode carries, as the coordinator
    /// resolved it.
    pub provenance_version: String,
    pub provenance_digest: String,
    /// Length in bytes of the raw PCM mix slice that follows this
    /// header on the wire (`0` for a silent composition).
    pub mix_slice_len: usize,
}

/// Worker reply for one chunk spec: `ok` with a receipt and the
/// artifact frame following on the wire, or a refusal naming the
/// cause (uninvited, an undeliverable range, a render failure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResponse {
    pub ok: bool,
    pub index: usize,
    /// Human refusal/cause text when `ok` is false.
    pub message: String,
    /// Frames drawn and encoded (`0` on refusal).
    pub frames: u64,
    /// Artifact bytes following this header (`0` on refusal).
    pub artifact_len: usize,
}

/// Parse the `--workers` invitation list (Sprint 26, T002 /
/// CAP-006): a comma-separated `host:port` list (hostnames and LAN
/// IPs are fine; IPv6 literals must be bracketed, `[::1]:port`).
/// Empty entries, malformed addresses, and duplicates refuse naming
/// the value, before any worker is contacted. Without the maker's
/// explicit invitation no machine ever joins — this function is
/// the only door into the invited set.
pub fn parse_workers(raw: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let addr = part.trim();
        if addr.is_empty() {
            return Err(format!(
                "render refused: --workers {:?} holds an empty worker address: pass a comma-separated `host:port` list (validation stage); no MP4 was written",
                raw
            ));
        }
        // `host:port` with a bracketed IPv6 literal, or a single
        // colon for IPv4/hostname.
        let valid = if let Some(rest) = addr.strip_prefix('[') {
            match rest.split_once(']') {
                Some((host, tail)) => {
                    !host.is_empty()
                        && tail.strip_prefix(':').is_some_and(|port| {
                            !port.is_empty() && port.parse::<u16>().map(|p| p > 0).unwrap_or(false)
                        })
                }
                None => false,
            }
        } else {
            addr.matches(':').count() == 1 && {
                let (host, port) = addr.split_once(':').unwrap();
                !host.is_empty()
                    && !port.is_empty()
                    && port.parse::<u16>().map(|p| p > 0).unwrap_or(false)
            }
        };
        if !valid {
            return Err(format!(
                "render refused: --workers address {addr:?} is not a `host:port` pair with a port from 1 to 65535 (bracket IPv6 literals as `[::1]:port`) (validation stage); no MP4 was written"
            ));
        }
        if out.iter().any(|w| w == addr) {
            return Err(format!(
                "render refused: --workers lists worker {addr:?} twice: each invited machine must appear once (validation stage); no MP4 was written"
            ));
        }
        out.push(addr.to_string());
    }
    if out.is_empty() {
        return Err(
            "render refused: LAN rendering needs at least one invited worker: pass --workers host:port,... (validation stage); no MP4 was written"
                .to_string(),
        );
    }
    Ok(out)
}

/// Check a request's invitation token against the token the
/// worker was started with (Sprint 26, T002 / CAP-006): a mismatch
/// refuses naming the invitation, never renders. Kept as a free
/// function so the CLI can validate the pair before connecting.
pub fn check_invitation(got: &str, want: &str) -> Result<(), String> {
    if got.trim() != want.trim() {
        return Err(
            "worker refused: this machine is not invited to render this chunk (invitation token mismatch); re-invite it with the maker's --worker-token".to_string(),
        );
    }
    Ok(())
}

/// Outcome of a worker rendering one chunk (Sprint 26, T002 /
/// CAP-002): the artifact bytes plus the measured receipt. The
/// bytes are carried over the wire; the receipt's `frames` and
/// `bytes` are measured from the artifact, never estimated.
#[derive(Debug)]
pub struct WorkerChunk {
    /// Frames drawn and encoded.
    pub frames: u64,
    /// Artifact bytes.
    pub artifact: Vec<u8>,
    pub draw_ms: u64,
    pub encode_ms: u64,
}

/// Render exactly the chunk in `spec` from the spec alone
/// (Sprint 26, T002 / CAP-005): the worker validates its own copy
/// of the source, prepares the full window the coordinator
/// described, draws this chunk's frames at their absolute
/// instants (byte-identical to single-pass, exactly like the
/// in-process path), encodes them with this chunk's mix slice,
/// and returns the artifact. It never touches another chunk's
/// range and never reassigns itself anything else. `want_token`
/// is the invitation token this worker was started with; an
/// empty one means this machine was never invited and renders
/// nothing.
pub fn render_worker_chunk(
    spec: &ChunkSpec,
    want_token: &str,
    ffmpeg_bin: &Path,
    mix_slice: &[u8],
    work_root: &Path,
) -> Result<WorkerChunk, String> {
    if want_token.trim().is_empty() {
        return Err(
            "worker refused: this machine is not invited to render chunk (no --worker-token); re-invite it with the maker's token"
                .to_string(),
        );
    }
    check_invitation(&spec.token, want_token)?;
    if spec.version != LAN_WIRE_VERSION {
        return Err(format!(
            "worker refused: chunk spec version {:?} is not the worker contract {:?}; update Framesmith on both machines",
            spec.version, LAN_WIRE_VERSION
        ));
    }
    if spec.total == 0 || spec.index >= spec.total {
        return Err(format!(
            "worker refused: chunk spec index {}/{} is outside the plan; no frames were drawn",
            spec.index + 1,
            spec.total
        ));
    }
    if spec.start_frame >= spec.end_frame {
        return Err(format!(
            "worker refused: chunk {} frames {}..{} owns no frames; no frames were drawn",
            spec.index + 1,
            spec.start_frame,
            spec.end_frame
        ));
    }
    if spec.total_frames == 0 || spec.end_frame > spec.total_frames as u64 {
        return Err(format!(
            "worker refused: chunk {} frames {}..{} runs past the {} prepared frame(s); no frames were drawn",
            spec.index + 1,
            spec.start_frame,
            spec.end_frame,
            spec.total_frames
        ));
    }
    if mix_slice.len() != spec.mix_slice_len {
        return Err(format!(
            "worker refused: chunk {} mix slice carries {} byte(s) but the spec names {}; no frames were drawn",
            spec.index + 1,
            mix_slice.len(),
            spec.mix_slice_len
        ));
    }
    if spec.composition_source.trim().is_empty() {
        return Err(format!(
            "worker refused: chunk {} spec carries no composition source; no frames were drawn",
            spec.index + 1
        ));
    }
    let format = OutputFormat::parse(&spec.format).ok_or_else(|| {
        format!(
            "worker refused: chunk {} spec format {:?} is unknown; no frames were drawn",
            spec.index + 1,
            spec.format
        )
    })?;
    crate::chunk::check_chunked_format(format)?;
    if spec.backend != "cpu" {
        return Err(format!(
            "worker refused: chunk {} asks for backend {:?}: workers render on cpu only in this sprint; re-run single-pass or chunk in-process for a gpu backend",
            spec.index + 1,
            spec.backend
        ));
    }

    // Monotonic per-chunk stamp (a process-relative clock reads 0
    // here and previously collided across parallel workers and tests).
    static STAMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = STAMP.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = work_root.join(format!(
        "lan-chunk-{}-{}-{}",
        spec.index,
        std::process::id(),
        stamp
    ));
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!("worker failed: cannot create work directory {}: {e}", dir.display())
    })?;
    let guard = DirGuard(dir.clone());
    let result = render_chunk_inner(spec, ffmpeg_bin, mix_slice, &dir);
    drop(guard);
    result
}

struct DirGuard(PathBuf);
impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn render_chunk_inner(
    spec: &ChunkSpec,
    ffmpeg_bin: &Path,
    mix_slice: &[u8],
    dir: &Path,
) -> Result<WorkerChunk, String> {
    // The worker validates its own copy of the source before any
    // frame is drawn, with the same caller values the coordinator
    // validated with (variables, strictness, theme).
    std::fs::write(dir.join("composition.kdl"), &spec.composition_source).map_err(|e| {
        format!(
            "worker failed: cannot stage the composition source: {e} [chunk {}/{}]",
            spec.index + 1,
            spec.total
        )
    })?;
    let overrides = match &spec.variables_json {
        None => validate::validate::VariableOverrides::new(),
        Some(raw) => validate::validate::parse_variable_overrides(raw)
            .map_err(|e| format!("worker refused: {e} [chunk {}/{}]", spec.index + 1, spec.total))?,
    };
    let verdict = validate::validate::validate_folder_with_variables_strict_themed(
        dir,
        &overrides,
        spec.strict_variables,
        spec.theme.as_deref(),
    );
    if verdict.status != validate::validate::Status::Valid || verdict.composition.is_none() {
        let why = verdict
            .diagnostics
            .first()
            .map(|d| d.to_string())
            .unwrap_or_else(|| "no valid composition".to_string());
        return Err(format!(
            "worker refused: staged source is invalid: {why} [chunk {}/{}]",
            spec.index + 1,
            spec.total
        ));
    }
    let comp = verdict
        .compositions
        .iter()
        .find(|c| c.name == spec.composition_name)
        .cloned()
        .or_else(|| verdict.composition.clone())
        .ok_or_else(|| {
            format!(
                "worker refused: composition {:?} is not in the staged source [chunk {}/{}]",
                spec.composition_name,
                spec.index + 1,
                spec.total
            )
        })?;
    let all = verdict.compositions.clone();
    // The scale plan derives from the spec's own flags, exactly
    // like the coordinator derived its own. The worker scales
    // the composition first and prepares the scaled tree, so a
    // non-identity run draws at the internal size.
    let scale = crate::scale::ScalePlan::from_flags(
        crate::scale::Resolution::parse(&spec.resolution).ok_or_else(|| {
            format!(
                "worker refused: unknown resolution {:?} in the spec [chunk {}/{}]",
                spec.resolution,
                spec.index + 1,
                spec.total
            )
        })?,
        spec.supersample,
        comp.width,
        comp.height,
    );
    let scaled_comp = crate::scale::scale_composition(&comp, scale);
    let scaled_all: Vec<notation::model::Composition> = all
        .iter()
        .map(|c| crate::scale::scale_composition(c, scale))
        .collect();
    // The window the coordinator prepared; the worker prepares the
    // same window so frame N draws at the same absolute instant.
    let range = crate::render::resolve_range(
        &scaled_comp.name,
        scaled_comp.duration,
        &scaled_comp.fps,
        spec.range_start,
        spec.range_end,
    )
    .map_err(|e| format!("{e} [chunk {}/{}]", spec.index + 1, spec.total))?;
    let mut prep = prepare_frames_ranged(&scaled_comp, &scaled_all, dir, ffmpeg_bin, range)
        .map_err(|e| format!("{e} [chunk {}/{}]", spec.index + 1, spec.total))?;
    prep.scale_expr_results(scale.expr_scale());
    // Targets the worker encodes with: the coordinator's resolved
    // choices, verbatim, so the chunk's encode matches its own.
    let opts = worker_options(spec)?;
    let frames_want = (spec.end_frame - spec.start_frame) as usize;
    if prep.frames() != spec.total_frames {
        return Err(format!(
            "worker refused: staged source prepares {} frame(s) but the spec names {}; the source disagrees with the coordinator [chunk {}/{}]",
            prep.frames(),
            spec.total_frames,
            spec.index + 1,
            spec.total
        ));
    }
    let fps = prep.comp.fps;
    let range_start = prep.range_start;
    let frames = frames_want;
    let chunk_secs = frames as f64 / fps.value();
    // Shift the window to this chunk's first frame: frame N draws
    // at its absolute instant, byte-identical to single-pass.
    prep.frames = frames;
    prep.range_start = range_start + crate::motion_plan::frame_time(&fps, spec.start_frame);
    prep.out_duration = chunk_secs;
    let mix_path = if spec.mix_slice_len > 0 {
        let p = dir.join("chunk.mix.pcm");
        std::fs::write(&p, mix_slice).map_err(|e| {
            format!("worker failed: cannot stage the chunk mix slice: {e} [chunk {}/{}]", spec.index + 1, spec.total)
        })?;
        Some(p)
    } else {
        None
    };
    let plan = AudioPlan {
        video_segs: Vec::new(),
        mix_pcm: mix_path,
        normalize: None,
        waveforms: Vec::new(),
    };
    let ext = opts.format.default_extension();
    let chunk_path: PathBuf = dir.join(format!("chunk.{ext}"));
    let dest = crate::outputs::ResolvedOutput::File {
        path: chunk_path.clone(),
    };
    let (_, draw_ms, encode_ms) =
        render_staged(&prep, &plan, &opts, ffmpeg_bin, &dest, &scale)
            .map_err(|e| format!("{e} [chunk {}/{}]", spec.index + 1, spec.total))?;
    let artifact = std::fs::read(&chunk_path).map_err(|e| {
        format!(
            "worker failed: cannot read the chunk artifact: {e} [chunk {}/{}]",
            spec.index + 1,
            spec.total
        )
    })?;
    if artifact.is_empty() {
        return Err(format!(
            "worker failed: chunk {}/{} produced no data",
            spec.index + 1,
            spec.total
        ));
    }
    Ok(WorkerChunk {
        frames: frames as u64,
        artifact,
        draw_ms,
        encode_ms,
    })
}

/// Reconstruct the coordinator's [`RenderOptions`] from the spec
/// (Sprint 26, T002): every worker-side choice is named in the
/// spec, so the chunk encodes exactly like the in-process path.
fn worker_options(spec: &ChunkSpec) -> Result<RenderOptions, String> {
    let at = format!(" [chunk {}/{}]", spec.index + 1, spec.total);
    let format = OutputFormat::parse(&spec.format).ok_or_else(|| {
        format!("worker refused: unknown format {:?} in the spec{at}", spec.format)
    })?;
    let prores_profile = crate::outputs::ProresProfile::parse(&spec.prores_profile)
        .ok_or_else(|| format!("worker refused: unknown prores-profile {:?} in the spec{at}", spec.prores_profile))?;
    let resolution = crate::scale::Resolution::parse(&spec.resolution)
        .ok_or_else(|| format!("worker refused: unknown resolution {:?} in the spec{at}", spec.resolution))?;
    let encoder_choice = crate::outputs::EncoderChoice::parse(&spec.encoder_choice)
        .ok_or_else(|| format!("worker refused: unknown encoder {:?} in the spec{at}", spec.encoder_choice))?;
    let hdr = crate::outputs::HdrMode::parse(&spec.hdr)
        .ok_or_else(|| format!("worker refused: unknown hdr {:?} in the spec{at}", spec.hdr))?;
    let hdr_transfer = match spec.hdr_transfer.as_str() {
        "sdr" => crate::outputs::HdrTransfer::Sdr,
        "pq" => crate::outputs::HdrTransfer::Pq,
        "hlg" => crate::outputs::HdrTransfer::Hlg,
        other => return Err(format!("worker refused: unknown hdr transfer {other:?} in the spec{at}")),
    };
    let backend = crate::render::Backend::parse(&spec.backend)
        .ok_or_else(|| format!("worker refused: unknown backend {:?} in the spec{at}", spec.backend))?;
    let preset = crate::render::Preset::parse(&spec.preset)
        .ok_or_else(|| format!("worker refused: unknown preset {:?} in the spec{at}", spec.preset))?;
    Ok(RenderOptions {
        backend,
        preset,
        crf_override: spec.crf_override,
        video_bitrate: spec.video_bitrate.clone(),
        format,
        prores_profile,
        resolution,
        supersample: spec.supersample,
        encoder_choice,
        hardware: false,
        hw_codec: None,
        keyint: spec.keyint,
        maxrate: spec.maxrate.clone(),
        hdr,
        hdr_transfer,
        hdr_tone_mapped: spec.hdr_tone_mapped,
        provenance: Some(crate::outputs::Provenance {
            version: spec.provenance_version.clone(),
            digest: spec.provenance_digest.clone(),
        }),
        gif_loop: 0,
    })
}

/// Per-attempt dispatch timeout: `FRAMESMITH_WORKER_TIMEOUT_MS`
/// when set to a positive number, else [`DEFAULT_TIMEOUT_MS`].
/// A worker that never answers is reported after this long,
/// never waited on forever.
pub fn worker_chunk_timeout() -> Duration {
    let ms = std::env::var(WORKER_TIMEOUT_MS_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|m| *m > 0)
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    Duration::from_millis(ms)
}

pub(crate) fn stall_ms() -> u64 {
    std::env::var(STALL_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

/// One dispatched chunk attempt against one invited worker.
/// Returns the raw artifact bytes, or a [`DispatchError`]: a
/// [`DispatchError::Dead`] worker (unreachable, killed, or a
/// connection that died mid-chunk) whose in-flight chunk must
/// redistribute, or a [`DispatchError::Refused`] refusal (the
/// worker answered with a cause — retrying elsewhere would
/// refuse identically).
pub enum DispatchError {
    Dead(String),
    Refused(String),
}

impl DispatchError {
    pub fn cause(self) -> String {
        match self {
            DispatchError::Dead(c) => c,
            DispatchError::Refused(c) => c,
        }
    }
    pub fn is_refused(&self) -> bool {
        matches!(self, DispatchError::Refused(_))
    }
}

/// Dispatch one chunk to one invited worker address (Sprint 26,
/// T002 / CAP-006): opens the wire connection, sends the spec
/// plus the already-sliced mix range, and reads the artifact plus
/// receipt. The timeout bounds the whole attempt, connect through
/// last artifact byte.
pub fn dispatch_one(
    addr: &str,
    spec: &ChunkSpec,
    mix_slice: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, DispatchError> {
    use std::net::ToSocketAddrs;
    let socket: Vec<std::net::SocketAddr> = addr
        .to_socket_addrs()
        .map_err(|e| DispatchError::Dead(format!("worker {addr} unreachable: {e}")))?
        .collect();
    let first = socket.first().cloned().ok_or_else(|| {
        DispatchError::Dead(format!("worker {addr} unreachable: cannot resolve"))
    })?;
    let mut stream = TcpStream::connect_timeout(&first, Duration::from_millis(CONNECT_TIMEOUT_MS))
        .map_err(|e| DispatchError::Dead(format!("worker {addr} unreachable: {e}")))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| DispatchError::Dead(format!("worker {addr} unreachable: {e}")))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| DispatchError::Dead(format!("worker {addr} unreachable: {e}")))?;
    let header = serde_json::to_vec(spec)
        .map_err(|e| DispatchError::Refused(format!("chunk {} spec encode failed: {e}", spec.index + 1)))?;
    write_frame(&mut stream, &header).map_err(|e| {
        DispatchError::Dead(format!("worker {addr} connection died sending chunk {}: {e}", spec.index + 1))
    })?;
    stream
        .write_all(mix_slice)
        .map_err(|e| DispatchError::Dead(format!("worker {addr} connection died sending chunk {}: {e}", spec.index + 1)))?;
    let resp_bytes = read_frame(&mut stream).map_err(|e| {
        DispatchError::Dead(format!(
            "worker {addr} died mid-chunk {}: {e}",
            spec.index + 1
        ))
    })?;
    let resp: WorkerResponse = serde_json::from_slice(&resp_bytes).map_err(|e| {
        DispatchError::Dead(format!(
            "worker {addr} replied to chunk {} with corrupt data: {e}",
            spec.index + 1
        ))
    })?;
    if !resp.ok {
        return Err(DispatchError::Refused(format!(
            "worker {addr} refused chunk {}: {}",
            spec.index + 1,
            resp.message
        )));
    }
    if resp.index != spec.index {
        return Err(DispatchError::Dead(format!(
            "worker {addr} replied to chunk {} with receipt for chunk {}; the artifact is discarded",
            spec.index + 1,
            resp.index + 1
        )));
    }
    if resp.artifact_len == 0 || resp.frames == 0 {
        return Err(DispatchError::Dead(format!(
            "worker {addr} returned an empty artifact for chunk {}; the artifact is discarded",
            spec.index + 1
        )));
    }
    let mut artifact = vec![0u8; resp.artifact_len];
    stream.read_exact(&mut artifact).map_err(|e| {
        DispatchError::Dead(format!(
            "worker {addr} died sending the chunk {} artifact: {e}",
            spec.index + 1
        ))
    })?;
    Ok(artifact)
}

pub(crate) fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> std::io::Result<()> {
    stream.write_all(&(payload.len() as u32).to_be_bytes())?;
    stream.write_all(payload)?;
    stream.flush()
}

pub(crate) fn read_frame(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let n = u32::from_be_bytes(len) as usize;
    if n > MAX_HEADER {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("wire frame {n} byte(s) exceeds the {MAX_HEADER} cap"),
        ));
    }
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// Serve chunk requests on `bind` (Sprint 26, T002 / CAP-006): the
/// invited worker process. Every accepted connection is one wire
/// request answered with exactly that chunk's artifact plus
/// receipt, or a refusal. A worker never serves without its
/// invitation `token` (the caller refuses to start without one).
/// `resolve_ffmpeg` resolves this worker's own FFmpeg from the
/// pinned registry per request (latest build, never a PATH guess
/// and never the coordinator's binary). `on_log` observes one
/// pure-text line per request and per refusal.
pub fn serve_worker(
    bind: &str,
    token: &str,
    resolve_ffmpeg: &(dyn Fn() -> Result<PathBuf, String> + Send + Sync),
    on_log: &(dyn Fn(&str) + Send + Sync),
) -> Result<(), String> {
    // Ensure a library caller gets the same invitation gate as
    // the CLI even without the serve env var: the token is the
    // authority, compared per request.
    if token.trim().is_empty() {
        return Err(
            "worker refused: a worker never serves without the maker's --worker-token: refusing to listen uninvited"
                .to_string(),
        );
    }
    let listener = TcpListener::bind(if bind.trim().is_empty() { "0.0.0.0" } else { bind })
        .map_err(|e| format!("worker refused: cannot bind {bind:?}: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("worker refused: cannot read the bind address: {e}"))?;
    on_log(&format!("stage workers: serving on {local} (invited via --worker-token)"));
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Err(e) = handle_connection(&mut stream, token, resolve_ffmpeg, on_log) {
            on_log(&format!("stage workers: {e}"));
        }
    }
    Ok(())
}

/// Handle one wire connection: read the request fully (the token
/// is checked after the read, so a refusal is always a parseable
/// reply and never a half-open close), then either render the
/// chunk or refuse naming the cause.
pub fn handle_connection(
    stream: &mut TcpStream,
    token: &str,
    resolve_ffmpeg: &(dyn Fn() -> Result<PathBuf, String> + Send + Sync),
    on_log: &(dyn Fn(&str) + Send + Sync),
) -> Result<(), String> {
    let header_bytes = read_frame(stream)
        .map_err(|e| format!("worker dropped a connection before its request: {e}"))?;
    let spec: ChunkSpec = serde_json::from_slice(&header_bytes)
        .map_err(|e| {
            let line = format!("worker refused: undecodable chunk spec: {e}");
            let resp = WorkerResponse {
                ok: false,
                index: 0,
                message: line.clone(),
                frames: 0,
                artifact_len: 0,
            };
            let _ = write_reply(stream, &resp, &[]);
            line
        })?;
    if spec.mix_slice_len > 512 * 1024 * 1024 {
        let line = format!(
            "worker refused: chunk {}/{} mix slice {} byte(s) exceeds the worker cap",
            spec.index + 1,
            spec.total,
            spec.mix_slice_len
        );
        let resp = WorkerResponse {
            ok: false,
            index: spec.index,
            message: line.clone(),
            frames: 0,
            artifact_len: 0,
        };
        let _ = write_reply(stream, &resp, &[]);
        return Err(line);
    }
    let mut mix = vec![0u8; spec.mix_slice_len];
    let _ = stream.read_exact(&mut mix);
    check_invitation(&spec.token, token).map_err(|e| {
        on_log(&format!(
            "stage workers: refused chunk {}/{}: uninvited machine",
            spec.index + 1,
            spec.total
        ));
        let line = format!("{e} [chunk {}/{}]", spec.index + 1, spec.total);
        let resp = WorkerResponse {
            ok: false,
            index: spec.index,
            message: line,
            frames: 0,
            artifact_len: 0,
        };
        let _ = write_reply(stream, &resp, &[]);
        e
    })?;
    let stall = stall_ms();
    if stall > 0 {
        std::thread::sleep(Duration::from_millis(stall));
    }
    let label = format!("chunk {}/{}", spec.index + 1, spec.total);
    on_log(&format!(
        "stage workers: {label} frames {}..{} received, rendering",
        spec.start_frame, spec.end_frame
    ));
    let ffmpeg_bin = resolve_ffmpeg()?;
    let work_root = std::env::temp_dir().join("framesmith-lan-worker");
    let _ = std::fs::create_dir_all(&work_root);
    match render_worker_chunk(&spec, token, &ffmpeg_bin, &mix, &work_root) {
        Ok(out) => {
            let resp = WorkerResponse {
                ok: true,
                index: spec.index,
                message: String::new(),
                frames: out.frames,
                artifact_len: out.artifact.len(),
            };
            write_reply(stream, &resp, &out.artifact)
                .map_err(|e| format!("worker failed: cannot reply for {label}: {e}"))?;
            on_log(&format!(
                "stage workers: {label} ok (frames {}, bytes {})",
                out.frames,
                out.artifact.len()
            ));
            Ok(())
        }
        Err(e) => {
            let resp = WorkerResponse {
                ok: false,
                index: spec.index,
                message: e.clone(),
                frames: 0,
                artifact_len: 0,
            };
            let _ = write_reply(stream, &resp, &[]);
            on_log(&format!("stage workers: {label} refused: {e}"));
            Err(e)
        }
    }
}

pub(crate) fn write_reply(
    stream: &mut TcpStream,
    resp: &WorkerResponse,
    artifact: &[u8],
) -> std::io::Result<()> {
    let header = serde_json::to_vec(resp).expect("WorkerResponse serializes");
    write_frame(stream, &header)?;
    stream.write_all(artifact)?;
    stream.flush()
}

/// Outcome of a distributed render (Sprint 26, T002 / CAP-006):
/// the T001 reassembly result plus the worker each chunk ran on,
/// in plan order.
#[derive(Debug)]
pub struct LanOutcome {
    /// Human success line naming the chunked output and its plan.
    pub message: String,
    pub draw_ms: u64,
    pub encode_ms: u64,
    pub receipts: Vec<ChunkReceipt>,
    pub chunk_dir: PathBuf,
    /// `worker` beside each receipt: the invited address that
    /// rendered it, in plan order.
    pub workers: Vec<String>,
}

/// The coordinator's view of a full distributed run: everything
/// [`crate::chunk::render_chunked`] does except the drawing —
/// specs are built from the coordinator's preparation, dispatched
/// to the invited workers with dead-worker redistribution, and
/// reassembled through the untouched T001 path.
pub struct LanInputs<'a> {
    pub prep: &'a mut Prepared,
    pub full_plan: &'a AudioPlan,
    pub opts: &'a RenderOptions,
    pub ffmpeg_bin: &'a Path,
    pub final_path: &'a Path,
    pub scale: &'a crate::scale::ScalePlan,
    pub chunks: &'a [ChunkRange],
    /// Raw `composition.kdl` of the `folder` (the whole source
    /// bundle this sprint: single-file compositions).
    pub composition_source: &'a str,
    pub composition_name: &'a str,
    pub variables_json: Option<&'a str>,
    pub strict_variables: bool,
    pub theme: Option<&'a str>,
    pub range_start: Option<f64>,
    pub range_end: Option<f64>,
    pub provenance_version: &'a str,
    pub provenance_digest: &'a str,
    pub workers: &'a [String],
    pub token: &'a str,
    pub timeout: Duration,
    pub on_progress: Option<&'a (dyn Fn(&str) + Send + Sync)>,
}

/// Run a distributed chunked render (Sprint 26, T002 /
/// CAP-005, CAP-006, CAP-008): one dispatch thread per invited
/// worker claims chunks from a shared queue; a worker that dies
/// mid-chunk loses only its in-flight chunk, which redistributes
/// to a remaining worker and re-renders exactly once. Every
/// returned artifact is written into the T001 chunk directory and
/// the run reassembles through [`assemble_chunks`], so the final
/// file and its provenance are the T001 path's. A worker refusal
/// (uninvited, an undeliverable range, a render failure) aborts
/// the run naming the chunk; a missing chunk is never skipped.
pub fn render_distributed(
    inputs: LanInputs<'_>,
) -> Result<LanOutcome, String> {
    let chunks = inputs.chunks;
    if chunks.is_empty() {
        return Err(
            "render refused: chunked rendering needs at least one chunk; no MP4 was written"
                .to_string(),
        );
    }
    crate::chunk::check_chunked_format(inputs.opts.format)?;
    if inputs.workers.is_empty() {
        return Err(
            "render refused: LAN rendering needs at least one invited worker: pass --workers host:port,... (validation stage); no MP4 was written"
                .to_string(),
        );
    }
    if inputs.token.trim().is_empty() {
        return Err(
            "render refused: LAN rendering needs the maker's invitation: pass --worker-token alongside --workers (validation stage); no MP4 was written"
                .to_string(),
        );
    }
    if inputs.opts.hardware || inputs.opts.hw_codec.is_some() {
        return Err(format!(
            "render refused: LAN chunk rendering needs a software encode (the workers cannot promise hardware encoders): re-run with --encoder software (validation stage); no MP4 was written"
        ));
    }
    if inputs.opts.backend != crate::render::Backend::Cpu {
        return Err(format!(
            "render refused: LAN chunk workers render on cpu only in this sprint (got {}); re-run single-pass or chunk in-process for a gpu backend (validation stage); no MP4 was written",
            inputs.opts.backend.name()
        ));
    }
    let total = chunks.len();
    let fps = inputs.prep.comp.fps;
    let total_frames = inputs.prep.frames();
    let out_duration = inputs.prep.out_duration;
    let full_mix = inputs.full_plan.mix_pcm.as_deref();
    let timeout = inputs.timeout;
    let workers: Vec<String> = inputs.workers.to_vec();
    // Stage every chunk's audio slice up front: the worker encodes
    // exactly its own samples, sample-aligned like the in-process
    // path, and the reassembly rebuilds audio from the full mix.
    let chunk_dir = chunk_dir_for(inputs.final_path);
    std::fs::create_dir_all(&chunk_dir).map_err(|e| {
        format!(
            "render refused: cannot create chunk directory {}: {e}; no MP4 was written",
            chunk_dir.display()
        )
    })?;
    let width = chunk_name_width(total);
    let ext = inputs.opts.format.default_extension();
    let mut slices: Vec<Vec<u8>> = Vec::with_capacity(total);
    for chunk in chunks {
        let start_secs = chunk.start_frame as f64 / fps.value();
        let end_secs = chunk.end_frame as f64 / fps.value();
        match full_mix {
            Some(mix) => {
                let mix_path = chunk_dir.join(format!("chunk-{:0width$}.mix.pcm", chunk.index));
                slice_mix_file(mix, start_secs, end_secs, &mix_path)
                    .map_err(|e| format!("{e} [chunk {}]", chunk.label(total)))?;
                let bytes = std::fs::read(&mix_path).map_err(|e| {
                    format!(
                        "render failed: cannot read the staged chunk mix {}: {e} (audio stage) [chunk {}]",
                        mix_path.display(),
                        chunk.label(total)
                    )
                })?;
                slices.push(bytes);
            }
            None => slices.push(Vec::new()),
        }
    }
    let spec_for = |chunk: &ChunkRange, mix_len: usize| -> ChunkSpec {
        ChunkSpec {
            version: LAN_WIRE_VERSION.to_string(),
            token: inputs.token.to_string(),
            index: chunk.index,
            total,
            start_frame: chunk.start_frame,
            end_frame: chunk.end_frame,
            total_frames,
            composition_source: inputs.composition_source.to_string(),
            composition_name: inputs.composition_name.to_string(),
            variables_json: inputs.variables_json.map(str::to_string),
            strict_variables: inputs.strict_variables,
            theme: inputs.theme.map(str::to_string),
            range_start: inputs.range_start,
            range_end: inputs.range_end,
            backend: inputs.opts.backend.name().to_string(),
            preset: inputs.opts.preset.name().to_string(),
            crf_override: inputs.opts.crf_override,
            video_bitrate: inputs.opts.video_bitrate.clone(),
            format: inputs.opts.format.name().to_string(),
            prores_profile: inputs.opts.prores_profile.name().to_string(),
            resolution: inputs.opts.resolution.name().to_string(),
            supersample: inputs.opts.supersample,
            encoder_choice: inputs.opts.encoder_choice.name().to_string(),
            keyint: inputs.opts.keyint,
            maxrate: inputs.opts.maxrate.clone(),
            hdr: inputs.opts.hdr.name().to_string(),
            hdr_transfer: inputs.opts.hdr_transfer.name().to_string(),
            hdr_tone_mapped: inputs.opts.hdr_tone_mapped,
            provenance_version: inputs.provenance_version.to_string(),
            provenance_digest: inputs.provenance_digest.to_string(),
            mix_slice_len: mix_len,
        }
    };
    let specs: Vec<ChunkSpec> = chunks
        .iter()
        .zip(slices.iter())
        .map(|(c, s)| spec_for(c, s.len()))
        .collect();

    let progress = |line: &str| {
        if let Some(hook) = inputs.on_progress {
            hook(line);
        }
    };
    let dispatch_start = Instant::now();
    let head = format!(
        "stage workers: dispatching {total} chunk(s) to {} invited worker(s)",
        workers.len()
    );
    progress(&head);

    // One thread per invited worker claims chunks from the queue
    // (Sprint 26, T002 / CAP-006, REC-007): a thread holds exactly
    // its worker's address, so a worker never carries two of this
    // run's chunks at once. Completing threads wait for peers
    // that are still in flight, so a redistributed chunk always
    // has a live worker thread to claim it.
    struct Shared {
        pending: std::collections::VecDeque<usize>,
        remaining: usize,
        dead: Vec<String>,
        results: Vec<Option<(String, Vec<u8>)>>,
        fatal: Option<String>,
        settled: bool,
    }
    let shared = Arc::new((std::sync::Mutex::new(Shared {
        pending: (0..total).collect(),
        remaining: total,
        dead: Vec::new(),
        results: vec![None; total],
        fatal: None,
        settled: false,
    }), Condvar::new()));
    let workers_arc = Arc::new(workers.clone());
    let specs_ref = Arc::new(specs);
    let slices_ref = Arc::new(slices);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker_idx in 0..workers_arc.len() {
            let shared = shared.clone();
            let workers_arc = workers_arc.clone();
            let specs_ref = specs_ref.clone();
            let slices_ref = slices_ref.clone();
            handles.push(scope.spawn(move || {
                let addr = workers_arc[worker_idx].clone();
                loop {
                    let claim = {
                        let mut state = shared.0.lock().unwrap();
                        loop {
                            if state.settled || state.fatal.is_some() {
                                break None;
                            }
                            if let Some(idx) = state.pending.pop_front() {
                                break Some(idx);
                            }
                            if state.remaining == 0 {
                                break None;
                            }
                            state = shared.1.wait(state).unwrap();
                        }
                    };
                    let idx = match claim {
                        Some(i) => i,
                        None => return,
                    };
                    let chunk = chunks[idx];
                    let label = chunk.label(total);
                    let line = format!("stage workers: {label} dispatched to {addr}");
                    if let Some(hook) = inputs.on_progress {
                        hook(&line);
                    }
                    match dispatch_one(&addr, &specs_ref[idx], &slices_ref[idx], timeout) {
                        Ok(artifact) => {
                            let mut state = shared.0.lock().unwrap();
                            if state.fatal.is_none() && state.results[idx].is_none() {
                                state.results[idx] = Some((addr.clone(), artifact));
                                state.remaining -= 1;
                                if state.remaining == 0 {
                                    state.settled = true;
                                }
                                shared.1.notify_all();
                            }
                            // The receipt line for this chunk
                            // lands after every worker settles, in
                            // plan order.
                        }
                        Err(DispatchError::Dead(cause)) => {
                            let mut state = shared.0.lock().unwrap();
                            if !state.dead.iter().any(|d| d == &addr) {
                                state.dead.push(addr.clone());
                            }
                            if state.dead.len() >= workers_arc.len() {
                                // No live worker left: the run fails
                                // naming the in-flight chunk.
                                if state.fatal.is_none() {
                                    state.fatal = Some(format!(
                                        "render failed: every invited worker is unreachable; the in-flight chunk is not lost: {label}; dead workers {}; re-invite a reachable worker or re-run; no MP4 was written",
                                        state.dead.join(", ")
                                    ));
                                    state.remaining = 0;
                                    state.settled = true;
                                    shared.1.notify_all();
                                }
                                return;
                            }
                            // The dead worker loses only this chunk:
                            // it redistributes to a remaining worker
                            // and re-renders exactly once.
                            if state.results[idx].is_none() && state.fatal.is_none() {
                                state.pending.push_back(idx);
                                shared.1.notify_all();
                            }
                            let line = format!(
                                "stage workers: {cause}: redistributing {label} to a remaining worker"
                            );
                            if let Some(hook) = inputs.on_progress {
                                hook(&line);
                            }
                            return;
                        }
                        Err(DispatchError::Refused(refusal)) => {
                            let mut state = shared.0.lock().unwrap();
                            if state.fatal.is_none() {
                                state.fatal = Some(format!(
                                    "render failed: {refusal}; finished chunks stay on disk and no MP4 was written"
                                ));
                                state.remaining = 0;
                                state.settled = true;
                                shared.1.notify_all();
                            }
                            return;
                        }
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }
    });
    let state = shared.0.lock().unwrap();
    if let Some(fatal) = &state.fatal {
        return Err(fatal.clone());
    }
    if state.remaining != 0 {
        let missing: Vec<String> = state
            .results
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.is_none().then(|| chunks[i].label(total)))
            .collect();
        return Err(format!(
            "render refused: distributed reassembly is missing {} chunk(s): {}; re-run the distributed render to produce them; no MP4 was written",
            missing.len(),
            missing.join(", ")
        ));
    }
    let mut results: Vec<(String, Vec<u8>)> = Vec::with_capacity(total);
    for r in &state.results {
        results.push(r.clone().expect("every chunk resolved"));
    }
    let dead = state.dead.clone();
    drop(state);
    // The dispatch wall is the drawn+encoded time of a LAN run:
    // the workers did this work, overlapped across machines.
    let draw_ms = dispatch_start.elapsed().as_millis() as u64;
    let encode_ms: u64;

    // Reassemble through the untouched T001 path, from the
    // artifacts the workers returned.
    let mut receipts: Vec<ChunkReceipt> = Vec::with_capacity(total);
    for (i, (worker, artifact)) in results.iter().enumerate() {
        let chunk = chunks[i];
        let frames = chunk.frame_count();
        let path = chunk_dir.join(format!("chunk-{:0width$}.{ext}", chunk.index));
        std::fs::write(&path, artifact).map_err(|e| {
            format!(
                "render failed: cannot stage the returned chunk artifact {}: {e} (encode stage)",
                path.display()
            )
        })?;
        let bytes = artifact.len() as u64;
        let line = format!(
            "stage workers: chunk {} ok on {worker} (frames {frames}, bytes {bytes})",
            chunk.index + 1,
        );
        // Progress names every chunk's worker; the receipt line
        // is the CAP-008 artifact.
        progress(&line);
        receipts.push(ChunkReceipt {
            index: chunk.index,
            start_frame: chunk.start_frame,
            end_frame: chunk.end_frame,
            frames,
            path,
            bytes,
        });
    }
    let artifacts: Vec<crate::chunk::ChunkArtifact> = receipts
        .iter()
        .map(|r| crate::chunk::ChunkArtifact {
            range: ChunkRange {
                index: r.index,
                start_frame: r.start_frame,
                end_frame: r.end_frame,
            },
            path: r.path.clone(),
        })
        .collect();
    let assemble_start = Instant::now();
    let comment = inputs
        .opts
        .provenance
        .as_ref()
        .map(|p| p.chunked_stamp(total, inputs.prep.frames as u64));
    assemble_chunks(
        inputs.ffmpeg_bin,
        &artifacts,
        inputs.final_path,
        inputs.opts.format,
        full_mix,
        out_duration,
        comment.as_deref(),
    )?;
    encode_ms = assemble_start.elapsed().as_millis() as u64;
    let pace = format!(
        "stage workers: {} chunk(s) rendered on {} invited worker(s){}",
        total,
        workers.len(),
        if dead.is_empty() {
            String::new()
        } else {
            format!(" (dead workers redistributed: {})", dead.join(", "))
        }
    );
    progress(&pace);
    let shown = inputs.final_path.display().to_string();
    let scale = inputs.scale;
    let text_drawn = inputs.prep.layouts.iter().any(|laid| laid.is_some())
        || inputs.prep.bitmap_fallback;
    let origin = crate::render::origin_note(
        inputs.prep.project_family.is_some(),
        &inputs.prep.fonts,
        inputs.prep.bitmap_fallback,
        text_drawn,
    );
    let extra = inputs
        .opts
        .announce_extra()
        .map(|x| format!(", {x}"))
        .unwrap_or_default();
    let worker_list = workers.join(",");
    let message = if inputs.opts.format == OutputFormat::Mp4 {
        format!(
            "rendered {} ({}x{}, {}s){} [backend={}, {}, chunks={}, workers={}{}].",
            shown,
            scale.out_w,
            scale.out_h,
            inputs.prep.out_duration,
            origin,
            inputs.opts.backend.name(),
            inputs.opts.announce(),
            total,
            worker_list,
            extra
        )
    } else {
        format!(
            "rendered {} ({}x{}, {}s){} [format={}, backend={}, {}, chunks={}, workers={}{}].",
            shown,
            scale.out_w,
            scale.out_h,
            inputs.prep.out_duration,
            origin,
            inputs.opts.format.name(),
            inputs.opts.backend.name(),
            inputs.opts.announce(),
            total,
            worker_list,
            extra
        )
    };
    Ok(LanOutcome {
        message,
        draw_ms,
        encode_ms,
        receipts,
        chunk_dir,
        workers: results.into_iter().map(|(w, _)| w).collect(),
    })
}

fn chunk_name_width(total: usize) -> usize {
    let mut w = 1;
    let mut x = total;
    while x >= 10 {
        x /= 10;
        w += 1;
    }
    w
}

