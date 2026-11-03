//! `init`, `check`, `render` and `errors` command implementations.
//!
//! Sprint 1 scope (T001–T010 except drawing, sound and the full run log):
//!
//! * `init` creates a minimal valid project with no network and no FFmpeg.
//! * `init` refuses unsafe targets and changes nothing on refusal.
//! * `check [--json]` validates without rendering (exits 0 / 1 / 2) as
//!   human text or machine-readable JSON carrying the same diagnostics.
//! * `errors [--json] [CODE]` prints the error-code catalog.
//! * `render` revalidates, repeats the same diagnostics on failure, reports
//!   the FFmpeg finding, self-downloads the pinned build when needed and
//!   allowed (announced in output and the run log), refuses before the
//!   first frame when downloading is disabled or offline, and — for fully
//!   valid folders with a suitable or self-provided FFmpeg — draws every
//!   frame on the CPU (background, exact-time video, document-order text),
//!   pipes raw frames to FFmpeg with source sound passed through, and
//!   writes one standard MP4 (H.264 yuv420p + AAC stereo 48 kHz).

use ffmpeg::bootstrap;
use validate::catalog::{self, json_escape};
use ffmpeg::ffmpeg as ffmpeg_mod;
use log::log;
use validate::validate as validate_mod;
use validate_mod::{Status, Verdict, validate_folder};
use std::path::{Path, PathBuf};

/// Project-name rule (reported verbatim in refusal messages): 1–64
/// characters, ASCII letters/digits/`-`/`_`, starting with a letter
/// or digit. No path separators, so the name can never escape the
/// parent directory.
pub fn name_error(name: &str) -> Option<String> {
    if name.is_empty() || name.len() > 64 {
        return Some("name must be 1 to 64 characters long".to_string());
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() {
        return Some("name must start with a letter or digit".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some(
            "name may only contain letters, digits, `-` and `_`".to_string(),
        );
    }
    if name == "." || name == ".." {
        return Some("name must not be `.` or `..`".to_string());
    }
    None
}

/// Minimal valid scaffold: only Sprint 1 elements, passes `check`
/// untouched, needs no network and no FFmpeg (T001).
pub fn scaffold_composition(name: &str) -> String {
    format!(
        "// Framesmith project `{name}` — minimal Sprint 1 composition.\n\
         // Validates with `framesmith check` and renders without changes.\n\
         composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#1a1a2e\" version=1 {{\n\
         \x20   text value=\"Hello from Framesmith\" start=0 duration=3 x=80 y=320 size=64 color=\"#ffffff\"\n\
         }}\n"
    )
}

fn dir_is_empty(dir: &Path) -> std::io::Result<bool> {
    Ok(std::fs::read_dir(dir)?.next().is_none())
}

/// `init`: create the scaffold. Returns the process exit code.
pub fn cmd_init(name: &str, parent: &Path) -> i32 {
    if let Some(rule) = name_error(name) {
        eprintln!(
            "framesmith init refused: invalid project name {name:?}: {rule}.\n\
             fix: choose a name of 1-64 letters, digits, `-` or `_`, starting with a letter or digit."
        );
        return 1;
    }
    let target = parent.join(name);
    if target.exists() {
        match dir_is_empty(&target) {
            Ok(true) => {} // An explicitly empty folder may be used.
            Ok(false) => {
                eprintln!(
                    "framesmith init refused: target {} already exists and is not empty.\n\
                     fix: choose another name or an empty folder; the existing content was left untouched.",
                    target.display()
                );
                return 1;
            }
            Err(e) => {
                eprintln!(
                    "framesmith init refused: cannot inspect target {}: {e}.\n\
                     fix: choose a readable, writable location.",
                    target.display()
                );
                return 1;
            }
        }
    } else if let Err(e) = std::fs::create_dir_all(&target) {
        eprintln!(
            "framesmith init refused: cannot create target {}: {e}.\n\
             fix: choose a writable location.",
            target.display()
        );
        return 1;
    }
    let created_dir = !target.exists() || dir_is_empty(&target).unwrap_or(false);
    let _ = created_dir;
    let comp_path = target.join(validate_mod::COMPOSITION_FILE);
    if let Err(e) = std::fs::write(&comp_path, scaffold_composition(name)) {
        // No partial folder on failure: remove what this run created.
        let _ = std::fs::remove_file(&comp_path);
        // Only remove the directory when this run created it and it is
        // still empty (never touch pre-existing content).
        if dir_is_empty(&target).unwrap_or(false) {
            let _ = std::fs::remove_dir(&target);
        }
        eprintln!(
            "framesmith init refused: cannot write {}: {e}.\n\
             fix: choose a writable location; nothing was left behind.",
            comp_path.display()
        );
        return 1;
    }
    println!("created project {} ({}).", target.display(), validate_mod::COMPOSITION_FILE);
    println!("next: run `framesmith check {}` to validate it.", target.display());
    0
}

/// One footage window as human text (Sprint 5 T001/T009): the
/// effective source window, the resolved clip length (marked when
/// inferred from an omitted `duration`), and the effective rendered
/// length of one valid `video` clip, plus a clamp note when a
/// past-end `trim-end` was cut back to the probed source end.
fn format_footage_human(f: &validate_mod::VideoFootage) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let window = match f.effective_end {
        Some(e) => format!("{:.3}s..{:.3}s", f.effective_start, e),
        None => format!("{:.3}s..? (source end unknown)", f.effective_start),
    };
    let length = match f.effective_duration {
        Some(d) => format!("{d:.3}s"),
        None => "unknown (source end unknown)".to_string(),
    };
    let duration = if f.inferred {
        format!("clip duration {:.3}s (inferred)", f.duration)
    } else {
        format!("clip duration {:.3}s", f.duration)
    };
    let _ = writeln!(
        out,
        "video {:?} ({}:{}): source window {window}, rate {}x, {duration}, effective length {length}, loop {}.",
        f.path,
        f.line,
        f.column,
        f.rate,
        if f.loop_clip { "on" } else { "off" }
    );
    if f.clamped {
        if let (Some(declared), Some(src)) = (f.trim_end, f.source_duration) {
            let _ = writeln!(
                out,
                "note: video {:?} ({}:{}): trim-end {declared:.3}s is past source end {src:.3}s; clamped to {src:.3}s.",
                f.path, f.line, f.column
            );
        }
    }
    out
}

/// Human-readable check report. Pure function so tests can compare it
/// against the `--json` form and the render refusal text (T006/T007).
pub fn format_check_human(folder: &Path, verdict: &Verdict) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for d in &verdict.diagnostics {
        let _ = writeln!(out, "{d}");
    }
    for f in &verdict.footage {
        out.push_str(&format_footage_human(f));
    }
    match verdict.status {
        Status::Valid => {
            let c = verdict.composition.as_ref().unwrap();
            let _ = writeln!(
                out,
                "ok: {} is valid ({}x{}, {}s, {}fps, {} clip(s)).",
                folder.display(),
                c.width,
                c.height,
                c.duration,
                c.fps,
                c.clips.len()
            );
        }
        Status::Invalid => {
            let _ = writeln!(
                out,
                "invalid: {} has {} problem(s); no output written.",
                folder.display(),
                verdict.diagnostics.len()
            );
        }
        Status::Unsupported => {
            let _ = writeln!(
                out,
                "unsupported: {} is well-formed but needs a later sprint; render would refuse before the first frame.",
                folder.display()
            );
        }
    }
    out
}

/// Machine-readable check report (T007): exactly the same diagnostic set
/// as the human text, one line of JSON carrying file, line, column and
/// code per item. Pure function for testability.
pub fn format_check_json(folder: &Path, verdict: &Verdict) -> String {
    use std::fmt::Write;
    let (status, exit) = match verdict.status {
        Status::Valid => ("valid", 0),
        Status::Invalid => ("invalid", 1),
        Status::Unsupported => ("unsupported", 2),
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"check\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"diagnostics\":[",
        json_escape(&folder.display().to_string())
    );
    for (i, d) in verdict.diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"file\":\"{}\",\"line\":{},\"column\":{},\"code\":\"{}\",\"message\":\"{}\",\"fix\":\"{}\"}}",
            json_escape(&d.file),
            d.line,
            d.column,
            json_escape(d.code),
            json_escape(&d.message),
            json_escape(&d.fix)
        );
    }
    // Sprint 5 T001/T009: the same effective windows as the human echo,
    // plus the resolved clip length and its inferred marking, so
    // inference and clamping are reviewable machine-readably.
    out.push_str("],\"videos\":[");
    let num = |v: Option<f64>| match v {
        Some(x) => format!("{x}"),
        None => "null".to_string(),
    };
    for (i, f) in verdict.footage.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"path\":\"{}\",\"line\":{},\"column\":{},\"trim_start\":{},\"trim_end\":{},\"rate\":{},\"loop\":{},\"source_duration\":{},\"effective_start\":{},\"effective_end\":{},\"effective_duration\":{},\"duration\":{},\"inferred\":{},\"clamped\":{}}}",
            json_escape(&f.path),
            f.line,
            f.column,
            num(Some(f.trim_start)),
            num(f.trim_end),
            num(Some(f.rate)),
            if f.loop_clip { "true" } else { "false" },
            num(f.source_duration),
            num(Some(f.effective_start)),
            num(f.effective_end),
            num(f.effective_duration),
            num(Some(f.duration)),
            if f.inferred { "true" } else { "false" },
            if f.clamped { "true" } else { "false" },
        );
    }
    out.push_str("]}");
    out
}

/// Append one run-log line. A failure is stated on the terminal with the
/// missing log spelled out, while the run still follows its own verdict
/// (T015/CAP-019: the log is append-only observability and never alters
/// decisions). With `json_stdout`, stdout must stay pure machine-readable
/// JSON, so the warning goes to stderr instead.
fn note_log(folder: &Path, level: &str, component: &str, run: &str, msg: &str, json_stdout: bool) {
    // No log location exists when the folder itself is missing: state the
    // gap without creating the queried folder as a side effect.
    if !folder.is_dir() {
        let line = format!(
            "warning: run log unavailable: {} has no logs/framesmith.log location (folder does not exist); run continues with its own verdict.",
            folder.display()
        );
        if json_stdout {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
        return;
    }
    if let Err(e) = log::append(folder, level, component, run, msg) {
        let line =
            format!("warning: run log unavailable: {e}; run continues with its own verdict.");
        if json_stdout {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

/// `check`: validate without rendering. Exits 0 (valid), 1 (invalid),
/// 2 (valid but unsupported). Never writes an MP4 and never needs FFmpeg.
/// With `json`, prints the machine-readable report instead of human text.
/// Every validation run appends its notable events to the run log (T015).
/// The optional `--backend` flag is accepted for render-command parity
/// but never changes the verdict (T001): the same folder is valid or
/// invalid under every backend. An unknown backend value is a validation
/// error (exit 1).
pub fn cmd_check(folder: &Path, json: bool, backend: Option<&str>) -> i32 {
    if let Some(raw) = backend {
        if render::render::Backend::parse(raw).is_none() {
            let msg = format!(
                "framesmith check: unknown --backend {raw:?}: expected cpu, gpu, or auto."
            );
            eprintln!("{msg}");
            return 1;
        }
    }
    let run = log::run_id();
    let verdict = validate_folder(folder);
    if json {
        println!("{}", format_check_json(folder, &verdict));
    } else {
        print!("{}", format_check_human(folder, &verdict));
    }
    let code = match verdict.status {
        Status::Valid => 0,
        Status::Invalid => 1,
        Status::Unsupported => 2,
    };
    // T015: every validation run appends its notable events to the run
    // log (`note_log` states the gap when there is nowhere to write).
    note_log(
        folder,
        "info",
        "check",
        &run,
        &format!("check start folder={}", folder.display()),
        json,
    );
    let (level, what) = match verdict.status {
        Status::Valid => ("info", "check valid"),
        Status::Invalid => ("warn", "check invalid"),
        Status::Unsupported => ("warn", "check unsupported"),
    };
    note_log(
        folder,
        level,
        "check",
        &run,
        &format!(
            "{what}: {} diagnostic(s), exit {code}",
            verdict.diagnostics.len()
        ),
        json,
    );
    code
}

/// Render a fully valid folder to a standard MP4 using one explicit
/// Flags for one `render` run (Sprint 3 T001/T002/T006): the backend
/// choice, the quality preset with exclusive encoder overrides, and
/// machine-readable output.
#[derive(Debug, Clone, Default)]
pub struct RenderRequest {
    pub backend: Option<String>,
    pub preset: Option<String>,
    pub crf: Option<String>,
    pub video_bitrate: Option<String>,
    pub json: bool,
}

/// Flag choices resolved before the first frame (T001/T002): the
/// requested backend, the active preset (`standard` when none is named),
/// and at most one encoder override.
struct ResolvedRender {
    backend: render::render::Backend,
    preset: render::render::Preset,
    crf: Option<u8>,
    video_bitrate: Option<String>,
}

/// Validate one `--video-bitrate` value: a positive number with an
/// optional `k`/`K`/`m`/`M` suffix (for example `800k` or `2M`).
fn parse_video_bitrate(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    let num = match t.strip_suffix(['k', 'K', 'm', 'M']) {
        Some(n) => n,
        None => t,
    };
    match num.parse::<f64>() {
        Ok(x) if x.is_finite() && x > 0.0 => Ok(t.to_string()),
        _ => Err(format!(
            "render refused: --video-bitrate {raw:?} is not numeric (for example `800k` or `2M`) (validation stage); no MP4 was written"
        )),
    }
}

fn resolve_render_request(req: &RenderRequest) -> Result<ResolvedRender, String> {
    let backend_raw = req.backend.as_deref().unwrap_or("auto");
    let backend = render::render::Backend::parse(backend_raw).ok_or_else(|| {
        format!(
            "render refused: unknown --backend {backend_raw:?}: expected cpu, gpu, or auto (validation stage); no MP4 was written"
        )
    })?;
    let preset_raw = req.preset.as_deref().unwrap_or("standard");
    let preset = render::render::Preset::parse(preset_raw).ok_or_else(|| {
        format!(
            "render refused: unknown --preset {preset_raw:?}: expected draft, standard, or high (validation stage); no MP4 was written"
        )
    })?;
    if req.crf.is_some() && req.video_bitrate.is_some() {
        return Err(
            "render refused: --crf and --video-bitrate together are a validation error: pass only one override (validation stage); no MP4 was written"
                .to_string(),
        );
    }
    let crf = match &req.crf {
        None => None,
        Some(raw) => match raw.trim().parse::<u8>() {
            Ok(n) if n <= 51 => Some(n),
            _ => {
                return Err(format!(
                    "render refused: --crf {raw:?} is not a number from 0 to 51 (validation stage); no MP4 was written"
                ));
            }
        },
    };
    let video_bitrate = match &req.video_bitrate {
        None => None,
        Some(raw) => Some(parse_video_bitrate(raw)?),
    };
    Ok(ResolvedRender {
        backend,
        preset,
        crf,
        video_bitrate,
    })
}

/// One progress line: stdout for human renders, stderr for `--json`
/// renders so stdout stays pure machine-readable JSON.
fn stage_line(json: bool, line: &str) {
    if json {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

/// Logical worker count for the frames-stage announcement, plus the
/// invalid-setting note the spec requires in the output and the log.
fn worker_note() -> (usize, Option<String>) {
    let n = render::render::worker_count();
    match std::env::var("FRAMESMITH_WORKERS") {
        Ok(raw) => match raw.trim().parse::<usize>() {
            Ok(w) if (1..=64).contains(&w) => (w, None),
            _ => {
                let plural = if n == 1 { "worker" } else { "workers" };
                (
                    n,
                    Some(format!(
                        "workers: invalid FRAMESMITH_WORKERS={raw:?}, using {n} {plural}"
                    )),
                )
            }
        },
        Err(_) => (n, None),
    }
}

/// `render` with default flags (CPU-effective auto backend, standard
/// preset, no overrides, human output): preserved for existing callers.
pub fn cmd_render(folder: &Path, output: Option<&Path>) -> i32 {
    cmd_render_full(folder, output, &RenderRequest::default())
}

/// Machine-readable render report (T006): the same stages, outcomes,
/// and measured durations the human progress lines carry.
struct RenderJson<'a> {
    folder: &'a Path,
    status: &'a str,
    exit: i32,
    backend: &'a str,
    preset: &'a str,
    enc: &'a render::render::ResolvedEncoder,
    output: &'a str,
    message: &'a str,
    diagnostics: &'a [validate_mod::Diagnostic],
    stages: &'a [render::render::StageTiming],
    total_ms: u64,
    /// Footage frame-cache report (Sprint 5 T006): one lookup per decoded
    /// clip window. Zero on runs that never reach the frames stage.
    cache_hits: u64,
    cache_misses: u64,
}

fn format_render_json(ctx: &RenderJson) -> String {
    use std::fmt::Write;
    let RenderJson {
        status,
        exit,
        backend,
        preset,
        total_ms,
        cache_hits,
        cache_misses,
        ..
    } = *ctx;
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"render\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"backend\":\"{backend}\",\"preset\":\"{preset}\",\"crf\":{},\"x264_preset\":",
        json_escape(&ctx.folder.display().to_string()),
        ctx.enc.crf,
    );
    match ctx.enc.x264_preset {
        Some(p) => {
            let _ = write!(out, "\"{p}\"");
        }
        None => out.push_str("null"),
    }
    let _ = write!(out, ",\"video_bitrate\":");
    match &ctx.enc.video_bitrate {
        Some(b) => {
            let _ = write!(out, "\"{}\"", json_escape(b));
        }
        None => out.push_str("null"),
    }
    let _ = write!(out, ",\"cache\":{{\"hits\":{cache_hits},\"misses\":{cache_misses}}}");
    let _ = write!(
        out,
        ",\"output\":\"{}\",\"message\":\"{}\",\"diagnostics\":[",
        json_escape(ctx.output),
        json_escape(ctx.message),
    );
    for (i, d) in ctx.diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"file\":\"{}\",\"line\":{},\"column\":{},\"code\":\"{}\",\"message\":\"{}\",\"fix\":\"{}\"}}",
            json_escape(&d.file),
            d.line,
            d.column,
            json_escape(d.code),
            json_escape(&d.message),
            json_escape(&d.fix)
        );
    }
    out.push_str("],\"stages\":[");
    for (i, s) in ctx.stages.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"name\":\"{}\",\"status\":\"{}\",\"duration_ms\":{},\"detail\":\"{}\"}}",
            s.name,
            s.status,
            s.duration_ms,
            json_escape(&s.detail)
        );
    }
    let _ = write!(out, "],\"timing_ms\":{{\"total\":{total_ms}}}}}");
    out
}

/// `render`: revalidate, select the backend, report FFmpeg state, build
/// the 48 kHz mix, draw frames (possibly in parallel), encode atomically,
/// and report staged progress with measured per-stage timings (T001,
/// T002, T003, T006, T008). Every run appends its notable events to the
/// run log. Refusals are logged too: no verdict ever skips the log.
pub fn cmd_render_full(folder: &Path, output: Option<&Path>, req: &RenderRequest) -> i32 {
    use std::time::Instant;
    let json = req.json;
    let run = log::run_id();
    let total_start = Instant::now();
    let mut stages: Vec<render::render::StageTiming> = Vec::new();
    // Ctrl-C handling for this render (Sprint 3 T005): one SIGINT sets
    // a flag the stages poll; every path below translates it to exit
    // 130 with staged and partial files removed.
    render::cancel::install();

    // Flags resolve before anything else: an unknown backend or preset,
    // a non-numeric override, or both overrides together refuses before
    // the first frame with no MP4 written.
    let resolved = match resolve_render_request(req) {
        Ok(r) => r,
        Err(e) => {
            stage_line(json, &e);
            note_log(folder, "warn", "render", &run, &e, json);
            if json {
                let enc = render::render::RenderOptions::default().encoder();
                println!(
                    "{}",
                    format_render_json(&RenderJson { folder, status: "invalid", exit: 1, backend: "unknown", preset: "unknown", enc: &enc, output: "", message: &e, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &[], total_ms: 0 })
                );
            }
            return 1;
        }
    };
    let mut opts = render::render::RenderOptions {
        backend: resolved.backend,
        preset: resolved.preset,
        crf_override: resolved.crf,
        video_bitrate: resolved.video_bitrate.clone(),
    };
    // `auto` resolves through the capability probe (T001): the GPU
    // renderer when one is available, else the CPU renderer. The chosen
    // backend is named in the output and the run log.
    opts.backend = render::render::resolve_backend(resolved.backend);

    stage_line(json, "stage validate: running");
    let validate_start = Instant::now();
    let verdict = validate_folder(folder);
    let validate_ms = validate_start.elapsed().as_millis() as u64;
    if !json {
        for d in &verdict.diagnostics {
            println!("{d}");
        }
    }
    let (vlevel, vwhat) = match verdict.status {
        Status::Valid => ("info", "render validation: valid"),
        Status::Invalid => ("warn", "render validation: invalid"),
        Status::Unsupported => ("warn", "render validation: unsupported"),
    };
    note_log(
        folder,
        vlevel,
        "check",
        &run,
        &format!(
            "{vwhat}: {} diagnostic(s)",
            verdict.diagnostics.len()
        ),
        json,
    );
    match verdict.status {
        Status::Invalid => {
            let msg = format!(
                "render refused: {} is invalid (validation stage); fix the problems above and re-run. No MP4 was written.",
                folder.display()
            );
            push_stage(&mut stages, render::render::StageTiming::failed(
                "validate",
                validate_ms,
                format!("{} problem(s)", verdict.diagnostics.len()),
            ));
            stage_line(json, &format!("stage validate: failed ({validate_ms}ms): {msg}"));
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "warn", "render", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson { folder, status: "invalid", exit: 1, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &msg, cache_hits: 0, cache_misses: 0, diagnostics: &verdict.diagnostics, stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
                );
            } else {
                println!("{msg}");
            }
            return 1;
        }
        Status::Unsupported => {
            let names: Vec<String> = verdict
                .diagnostics
                .iter()
                .map(|d| d.message.clone())
                .collect();
            let msg = format!(
                "render refused before the first frame: {} uses later-sprint element(s): {}",
                folder.display(),
                names.join("; ")
            );
            push_stage(&mut stages, render::render::StageTiming::failed(
                "validate",
                validate_ms,
                format!("{} later-sprint use(s)", verdict.diagnostics.len()),
            ));
            stage_line(json, &format!("stage validate: failed ({validate_ms}ms): {msg}"));
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "warn", "render", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson { folder, status: "unsupported", exit: 2, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &msg, cache_hits: 0, cache_misses: 0, diagnostics: &verdict.diagnostics, stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
                );
            } else {
                println!(
                    "render refused before the first frame: {} uses later-sprint element(s):",
                    folder.display()
                );
                for n in &names {
                    println!("  - {n}");
                }
                println!("No MP4 or partial output was written.");
            }
            return 2;
        }
        Status::Valid => {
            push_stage(&mut stages, render::render::StageTiming::ok(
                "validate",
                validate_ms,
                format!("{} diagnostic(s)", verdict.diagnostics.len()),
            ));
            stage_line(json, &format!("stage validate: ok ({validate_ms}ms)"));
        }
    }
    // A `gpu` request on a machine without a GPU renderer refuses before
    // the first frame, naming the missing capability and suggesting the
    // working choices; no MP4 is written.
    if resolved.backend == render::render::Backend::Gpu && !render::gpu::gpu_available() {
        let msg = "render refused: no GPU renderer is available on this machine (capability probe found none); use `--backend cpu` or `--backend auto` instead (validation stage). No MP4 was written.".to_string();
        stage_line(json, &format!("stage validate: note: {msg}"));
        print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
        note_log(folder, "warn", "render", &run, &msg, json);
        if json {
            println!(
                "{}",
                format_render_json(&RenderJson { folder, status: "invalid", exit: 1, backend: "gpu", preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &msg, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
            );
        } else {
            println!("{msg}");
        }
        return 1;
    }
    // Sprint 5 T002/T003/T009: the render summary carries the effective
    // footage windows the render consumes (INT-003), including the
    // probe-wins clamp report and inferred lengths. Only clips with
    // declared trim/rate/loop keys, an inferred duration, or a clamp
    // announce; plain clips render exactly as before with no new lines.
    for f in &verdict.footage {
        if f.trim_start != 0.0 || f.trim_end.is_some() || f.rate != 1.0 || f.loop_clip || f.clamped || f.inferred {
            for line in format_footage_human(f).lines() {
                let note = format!("stage validate: note: {line}");
                stage_line(json, &note);
                note_log(folder, "info", "render", &run, &note, json);
            }
        }
    }
    let comp = verdict.composition.unwrap();
    if render::cancel::is_cancelled() {
        return cmd_cancelled(folder, &run, json, opts.backend.name(), opts.preset.name(), &opts.encoder(), &folder.join("out.mp4"), "validate", &mut stages, total_start, 0, "");
    }

    // Detection only reads host state; the folder is never modified (T008).
    let finding = ffmpeg_mod::detect();
    stage_line(json, &finding.to_string());
    note_log(folder, "info", "bootstrap", &run, &finding.to_string(), json);
    // Bootstrap: reuse the host binary, self-download the pinned build,
    // or refuse before the first frame (T009/T010). The composition is
    // never touched here; only the tool cache may gain the pinned build.
    let cfg = bootstrap::BootstrapConfig::from_env(verdict.ffmpeg_auto_download);
    let ffmpeg_bin = match bootstrap::ensure_ffmpeg(&finding, &cfg) {
        bootstrap::Bootstrap::Ready {
            ffmpeg_path,
            downloaded: _,
            announcements,
        } => {
            for a in &announcements {
                stage_line(json, a);
                note_log(folder, "info", "bootstrap", &run, a, json);
            }
            ffmpeg_path
        }
        bootstrap::Bootstrap::Refused { code, message } => {
            stage_line(json, &message);
            if !json {
                println!("(code {code} — see `framesmith errors {code}` for meaning and fix.)");
            }
            note_log(folder, "warn", "bootstrap", &run, &message, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson { folder, status: "failed", exit: 1, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &message, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
                );
            }
            return 1;
        }
    };

    let default_out: PathBuf = folder.join("out.mp4");
    let out = output.map(Path::to_path_buf).unwrap_or(default_out);
    // T004 output preflight (output stage, before the first frame): the
    // target directory must exist and accept the staged write beside the
    // target, or the render refuses with the path named and writes
    // nothing — never a partial MP4.
    let probe_dir: PathBuf = match out.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    if let Err(e) = std::fs::create_dir_all(&probe_dir) {
        let msg = format!(
            "render refused: cannot create output directory {}: {e} (output stage). No MP4 was written.",
            probe_dir.display()
        );
        stage_line(json, &format!("stage output: failed: {msg}"));
        note_log(folder, "error", "render", &run, &msg, json);
        if json {
            println!(
                "{}",
                format_render_json(&RenderJson { folder, status: "failed", exit: 1, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &msg, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
            );
        } else {
            println!("{msg}");
        }
        return 1;
    }
    let probe = probe_dir.join(".framesmith-write-probe");
    match std::fs::write(&probe, b"framesmith writability probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
        }
        Err(e) => {
            let msg = format!(
                "render refused: output directory {} is not writable: {e} (output stage). No MP4 was written.",
                probe_dir.display()
            );
            stage_line(json, &format!("stage output: failed: {msg}"));
            note_log(folder, "error", "render", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson { folder, status: "failed", exit: 1, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &msg, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
                );
            } else {
                println!("{msg}");
            }
            return 1;
        }
    }

    let (workers, workers_note) = worker_note();
    if let Some(note) = &workers_note {
        stage_line(json, note);
        note_log(folder, "warn", "render", &run, note, json);
    }

    // Frames stage, first half: prepare every picture input before the
    // first frame (fonts, video probes and decodes, images).
    let frames_start = Instant::now();
    stage_line(
        json,
        &format!(
            "stage frames: running (backend={}, {}, workers={workers})",
            opts.backend.name(),
            opts.announce()
        ),
    );
    let prep = match render::render::prepare_frames(&comp, folder, &ffmpeg_bin) {
        Ok(prep) => prep,
        Err(e) => {
            if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                let ms = frames_start.elapsed().as_millis() as u64;
                let detail = without_cancel_prefix(&e).unwrap_or("").to_string();
                return cmd_cancelled(folder, &run, json, opts.backend.name(), opts.preset.name(), &opts.encoder(), &out, "frames", &mut stages, total_start, ms, &detail);
            }
            let ms = frames_start.elapsed().as_millis() as u64;
            push_stage(&mut stages, render::render::StageTiming::failed(
                "frames",
                ms,
                "prepare failed".to_string(),
            ));
            stage_line(json, &format!("stage frames: failed ({ms}ms): {e}"));
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "error", "render", &run, &e, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson { folder, status: "failed", exit: 1, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &e, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
                );
            } else if e.starts_with("render refused:") || e.starts_with("render failed:") {
                println!("{e}");
            } else {
                println!("render failed: {e}");
            }
            return 1;
        }
    };
    if render::cancel::is_cancelled() {
        let ms = frames_start.elapsed().as_millis() as u64;
        return cmd_cancelled(folder, &run, json, opts.backend.name(), opts.preset.name(), &opts.encoder(), &out, "frames", &mut stages, total_start, ms, "");
    }
    let prepared_note = format!("prepared {} frame(s)", prep.frames());
    // Audio stage: smoke-decode video sound and build the Framesmith
    // 48 kHz stereo mix of the `audio` tracks (T008).
    let audio_start = Instant::now();
    stage_line(json, "stage audio: running");
    let mix_path = render::render::mix_staging_path(&out);
    let plan = match render::render::build_audio_plan(&comp, folder, &ffmpeg_bin, &prep, &mix_path)
    {
        Ok(plan) => plan,
        Err(e) => {
            if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                let audio_ms = audio_start.elapsed().as_millis() as u64;
                let detail = without_cancel_prefix(&e).unwrap_or("").to_string();
                return cmd_cancelled(folder, &run, json, opts.backend.name(), opts.preset.name(), &opts.encoder(), &out, "audio", &mut stages, total_start, audio_ms, &detail);
            }
            let audio_ms = audio_start.elapsed().as_millis() as u64;
            let frames_ms = frames_start.elapsed().as_millis() as u64;
            push_stage(&mut stages, render::render::StageTiming::ok(
                "frames",
                frames_ms,
                prepared_note.clone(),
            ));
            push_stage(&mut stages, render::render::StageTiming::failed(
                "audio",
                audio_ms,
                "mix build failed".to_string(),
            ));
            stage_line(json, &format!("stage frames: ok ({frames_ms}ms): {prepared_note}"));
            stage_line(json, &format!("stage audio: failed ({audio_ms}ms): {e}"));
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "error", "render", &run, &e, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson { folder, status: "failed", exit: 1, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &e, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
                );
            } else if e.starts_with("render refused:") || e.starts_with("render failed:") {
                println!("{e}");
            } else {
                println!("render failed: {e}");
            }
            return 1;
        }
    };
    if render::cancel::is_cancelled() {
        let ms = audio_start.elapsed().as_millis() as u64;
        return cmd_cancelled(folder, &run, json, opts.backend.name(), opts.preset.name(), &opts.encoder(), &out, "audio", &mut stages, total_start, ms, "");
    }
    let audio_ms = audio_start.elapsed().as_millis() as u64;
    let audio_note = match &plan.mix_pcm {
        Some(_) => "mix: audible sources mixed at 48 kHz stereo".to_string(),
        None => "mix: no audible sound; silent track".to_string(),
    };
    stage_line(json, &format!("stage audio: ok ({audio_ms}ms): {audio_note}"));
    push_stage(&mut stages, render::render::StageTiming::ok(
        "audio",
        audio_ms,
        audio_note.clone(),
    ));

    // Frames stage, second half, plus encode: draw in parallel in bounded
    // batches, re-emit in frame order, and rename the MP4 into place.
    let (done_msg, draw_ms, encode_ms) =
        match render::render::render_staged(&prep, &plan, &opts, &ffmpeg_bin, &out) {
            Ok(outcome) => outcome,
            Err(e) => {
                if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                    let frames_ms = frames_start.elapsed().as_millis() as u64;
                    let detail = without_cancel_prefix(&e).unwrap_or("").to_string();
                    let stage: &'static str = if e.contains("encode") { "encode" } else { "frames" };
                    return cmd_cancelled(folder, &run, json, opts.backend.name(), opts.preset.name(), &opts.encoder(), &out, stage, &mut stages, total_start, frames_ms, &detail);
                }
                let frames_ms = frames_start.elapsed().as_millis() as u64;
                push_stage(&mut stages, render::render::StageTiming::failed(
                    "frames",
                    frames_ms,
                    prepared_note.clone(),
                ));
                stage_line(json, &format!("stage frames: failed ({frames_ms}ms): {e}"));
                print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
                note_log(folder, "error", "render", &run, &e, json);
                if json {
                    println!(
                        "{}",
                        format_render_json(&RenderJson { folder, status: "failed", exit: 1, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: "", message: &e, cache_hits: 0, cache_misses: 0, diagnostics: &[], stages: &stages, total_ms: total_start.elapsed().as_millis() as u64 })
                    );
                } else if e.starts_with("render refused:") || e.starts_with("render failed:") {
                    println!("{e}");
                } else {
                    println!("render failed: {e}");
                }
                return 1;
            }
        };
    let frames_ms = frames_start.elapsed().as_millis() as u64;
    push_stage(&mut stages, render::render::StageTiming::ok(
        "frames",
        frames_ms,
        format!("{prepared_note}; drew {draw_ms}ms"),
    ));
    push_stage(&mut stages, render::render::StageTiming::ok(
        "encode",
        encode_ms,
        "ffmpeg encode and atomic rename".to_string(),
    ));
    stage_line(
        json,
        &format!("stage frames: ok ({frames_ms}ms): {prepared_note}"),
    );
    // Sprint 5 T006: the footage frame-cache report (INT-003) — hits on
    // an unchanged re-render, misses on first or bypassed renders. The
    // same counts ride in the `--json` document below.
    let (cache_hits, cache_misses) = (prep.cache_stats.hits, prep.cache_stats.misses);
    let cache_line = format!("cache: {cache_hits} hit(s), {cache_misses} miss(es)");
    stage_line(json, &cache_line);
    note_log(folder, "info", "render", &run, &cache_line, json);
    stage_line(json, &format!("stage encode: ok ({encode_ms}ms)"));
    stage_line(json, &done_msg);
    note_log(folder, "info", "render", &run, &done_msg, json);
    let total_ms = total_start.elapsed().as_millis() as u64;
    push_stage(&mut stages, render::render::StageTiming::ok(
        "done",
        total_ms,
        done_msg.clone(),
    ));
    stage_line(json, "stage done: ok");
    print_timing_line(json, &stages, total_ms);
    note_log(
        folder,
        "info",
        "render",
        &run,
        &format!(
            "timing: {}",
            stages
                .iter()
                .map(|s| format!("{}={}ms", s.name, s.duration_ms))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        json,
    );
    if json {
        println!(
            "{}",
            format_render_json(&RenderJson { folder, status: "valid", exit: 0, backend: opts.backend.name(), preset: opts.preset.name(), enc: &opts.encoder(), output: &out.display().to_string(), message: &done_msg, cache_hits, cache_misses, diagnostics: &[], stages: &stages, total_ms })
        );
    }
    0
}

/// Canonical stage order for the timing summary and the `--json`
/// stages array (T006): validate, frames, audio, encode, done.
/// Progress lines print in execution order; this keeps the recorded
/// entries in the documented order whatever path the run took.
fn push_stage(stages: &mut Vec<render::render::StageTiming>, entry: render::render::StageTiming) {
    const ORDER: &[&str] = &["validate", "frames", "audio", "encode", "done"];
    let rank = |name: &str| ORDER.iter().position(|n| *n == name).unwrap_or(usize::MAX);
    let at = stages
        .iter()
        .position(|e| rank(e.name) > rank(entry.name))
        .unwrap_or(stages.len());
    stages.insert(at, entry);
}

/// Strip the cancellation sentinel prefix for display; `None` when the
/// message is not a cancellation travelling up the stack.
fn without_cancel_prefix(e: &str) -> Option<&str> {
    e.strip_prefix(render::cancel::PREFIX)
        .map(|s| s.trim_start_matches([':', ' ']))
}

/// Cancelled render (Sprint 3 T005): remove staged and partial files
/// beside the target, name every leftover path that could not be
/// removed, and exit 130. A cancelled run is not resumable; re-running
/// the same command starts over.
#[allow(clippy::too_many_arguments)]
fn cmd_cancelled(
    folder: &Path,
    run: &str,
    json: bool,
    backend: &str,
    preset: &str,
    enc: &render::render::ResolvedEncoder,
    out: &Path,
    stage: &'static str,
    stages: &mut Vec<render::render::StageTiming>,
    total_start: std::time::Instant,
    stage_ms: u64,
    detail: &str,
) -> i32 {
    let mix = render::render::mix_staging_path(out);
    let leftovers = render::cancel::remove_staged(out, &[mix]);
    let mut msg = if detail.is_empty() {
        format!("render cancelled: Ctrl-C during the {stage} stage; workers stopped, FFmpeg child terminated, staged and partial files removed.")
    } else {
        detail.to_string()
    };
    if !leftovers.is_empty() && !msg.contains("leftover") {
        let names: Vec<String> = leftovers
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        msg.push_str(&format!(
            " leftover path(s) that could not be removed: {}.",
            names.join(", ")
        ));
    }
    msg.push_str(" exit 130; re-run the same command to start over.");
    push_stage(
        stages,
        render::render::StageTiming::failed(stage, stage_ms, "cancelled".to_string()),
    );
    let total_ms = total_start.elapsed().as_millis() as u64;
    stage_line(
        json,
        &format!("stage {stage}: cancelled ({stage_ms}ms): {msg}"),
    );
    print_timing_line(json, stages, total_ms);
    note_log(folder, "warn", "render", run, &msg, json);
    if json {
        println!(
            "{}",
            format_render_json(&RenderJson {
                folder,
                status: "cancelled",
                exit: 130,
                backend,
                preset,
                enc,
                output: &out.display().to_string(),
                message: &msg,
                diagnostics: &[],
                stages,
                total_ms,
                // A cancelled run never reports cache counts: the frames
                // stage may not have finished, so hits/misses would lie.
                cache_hits: 0,
                cache_misses: 0,
            })
        );
    } else {
        println!("{msg}");
    }
    render::cancel::EXIT_CODE
}

/// Per-stage timing summary (T006): measured durations for every stage
/// the run reached, in both human and `--json` forms. Never an estimate.
fn print_timing_line(json: bool, stages: &[render::render::StageTiming], total_ms: u64) {
    let mut parts: Vec<String> = stages
        .iter()
        .map(|s| format!("{}={}ms", s.name, s.duration_ms))
        .collect();
    parts.push(format!("total={total_ms}ms"));
    stage_line(json, &format!("timing: {}", parts.join(" ")));
}

/// `errors`: print the error-code catalog (T007). With a code, prints the
/// single entry; with `--json`, prints machine-readable JSON instead of
/// human text. Returns the process exit code.
pub fn cmd_errors(code: Option<&str>, json: bool) -> i32 {
    match code {
        None => {
            if json {
                use std::fmt::Write;
                let mut out = String::from("{\"tool\":\"framesmith\",\"command\":\"errors\",\"codes\":[");
                for (i, e) in catalog::CATALOG.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(
                        out,
                        "{{\"code\":\"{}\",\"meaning\":\"{}\",\"example\":\"{}\",\"fix\":\"{}\"}}",
                        json_escape(e.code),
                        json_escape(e.meaning),
                        json_escape(e.example),
                        json_escape(e.fix)
                    );
                }
                out.push_str("]}");
                println!("{out}");
            } else {
                for e in catalog::CATALOG {
                    println!("{}", catalog::render_entry(e));
                }
            }
            0
        }
        Some(c) => match catalog::lookup(c) {
            Some(e) => {
                if json {
                    println!(
                        "{{\"tool\":\"framesmith\",\"command\":\"errors\",\"code\":\"{}\",\"meaning\":\"{}\",\"example\":\"{}\",\"fix\":\"{}\"}}",
                        json_escape(e.code),
                        json_escape(e.meaning),
                        json_escape(e.example),
                        json_escape(e.fix)
                    );
                } else {
                    println!("{}", catalog::render_entry(e));
                }
                0
            }
            None => {
                eprintln!("framesmith errors: unknown error code {c:?}.");
                eprintln!("valid codes:");
                for e in catalog::CATALOG {
                    eprintln!("  {}", e.code);
                }
                eprintln!("usage: framesmith errors [--json] [CODE]");
                2
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    pub fn tmp_dir(tag: &str) -> PathBuf {
        let n = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "framesmith-foundation-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(folder: &Path, name: &str, content: &str) {
        std::fs::write(folder.join(name), content).unwrap();
    }

    const VALID: &str = "composition name=\"t\" width=640 height=480 duration=2 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=2\n}\n";

    #[test]
    fn scaffold_passes_check_untouched() {
        let root = tmp_dir("scaffold");
        assert_eq!(cmd_init("demo", &root), 0);
        let verdict = validate_folder(&root.join("demo"));
        assert_eq!(verdict.status, Status::Valid, "{:?}", verdict.diagnostics);
    }

    #[test]
    fn init_refuses_nonempty_and_changes_nothing() {
        let root = tmp_dir("nonempty");
        let target = root.join("proj");
        std::fs::create_dir_all(&target).unwrap();
        write(&target, "keep.txt", "precious");
        write(&target, validate_mod::COMPOSITION_FILE, VALID);
        let before: Vec<(PathBuf, Vec<u8>)> = std::fs::read_dir(&target)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (p.clone(), std::fs::read(&p).unwrap())
            })
            .collect();
        assert_eq!(cmd_init("proj", &root), 1);
        let after: Vec<(PathBuf, Vec<u8>)> = std::fs::read_dir(&target)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (p.clone(), std::fs::read(&p).unwrap())
            })
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn init_uses_empty_existing_folder() {
        let root = tmp_dir("empty");
        std::fs::create_dir_all(root.join("proj")).unwrap();
        assert_eq!(cmd_init("proj", &root), 0);
        assert_eq!(
            validate_folder(&root.join("proj")).status,
            Status::Valid
        );
    }

    #[test]
    fn init_rejects_bad_names() {
        let root = tmp_dir("names");
        let bad65 = "x".repeat(65);
        for bad in ["", "has space", "a/b", "..", ".", "-x!", "é", bad65.as_str()] {
            assert_eq!(cmd_init(bad, &root), 1, "name {bad:?}");
        }
        // Nothing may be created for bad names: only entries allowed are
        // none at all (every refusal happens before touching the fs).
        let entries: Vec<_> = std::fs::read_dir(&root).unwrap().collect();
        assert!(entries.is_empty(), "{entries:?}");
        // Boundary: 64 chars is accepted.
        let ok64 = "a".repeat(64);
        assert_eq!(cmd_init(&ok64, &root), 0);
    }

    #[test]
    fn init_rejects_unwritable_location() {
        let root = tmp_dir("nowrite");
        let ro = root.join("ro");
        std::fs::create_dir_all(&ro).unwrap();
        let mut perms = std::fs::metadata(&ro).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&ro, perms).unwrap();
        let code = cmd_init("proj", &ro);
        // Restore so the temp dir can be cleaned up.
        let mut perms = std::fs::metadata(&ro).unwrap().permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&ro, perms).unwrap();
        if is_root() {
            // Running as root bypasses permission bits; skip the assertion.
            let _ = code;
            return;
        }
        assert_eq!(code, 1);
        assert!(!ro.join("proj").exists(), "no partial folder may remain");
    }

    /// True when running as uid 0 (permission bits do not apply).
    /// Shells out to `id -u` to avoid external crates.
    fn is_root() -> bool {
        #[cfg(unix)]
        {
            std::process::Command::new("id")
                .arg("-u")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
                .unwrap_or(false)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    #[test]
    fn fps_shapes() {
        let case = |fps_src: &str| {
            let folder = tmp_dir("fpscase");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=1 fps={fps_src} background=\"#000000\" version=1 {{\n}}\n"
                ),
            );
            validate_folder(&folder).status
        };
        assert_eq!(case("30"), Status::Valid);
        assert_eq!(case("30000/1001"), Status::Valid);
        assert_eq!(case("\"30000/1001\""), Status::Valid);
        assert_eq!(case("29.97"), Status::Invalid, "decimal fps is rejected");
        assert_eq!(case("0"), Status::Invalid);
        assert_eq!(case("241"), Status::Invalid);
        assert_eq!(case("240"), Status::Valid);
        assert_eq!(case("1"), Status::Valid);
    }

    #[test]
    fn unknown_keys_are_invalid() {
        let folder = tmp_dir("unknownkey");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 bogus=1 background=\"#000\" version=1 {\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_UNKNOWN_KEY"));
    }

    #[test]
    fn unknown_config_keys_are_invalid() {
        let folder = tmp_dir("unknowncfg");
        write(&folder, validate_mod::COMPOSITION_FILE, VALID);
        write(
            &folder,
            notation::model::CONFIG_FILE,
            "bogus_key = true\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_UNKNOWN_KEY"));
    }

    #[test]
    fn later_sprint_elements_are_unsupported_not_invalid() {
        // Boxes, images, tweens, sets, and audio validate since Sprints
        // 2 and 3 (T002/T003/T007); effects and automation still wait
        // for Sprint 10 sound proof.
        let folder = tmp_dir("unsupported");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    effect preset=\"hall\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Unsupported, "{:?}", v.diagnostics);
        assert!(
            v.diagnostics
                .iter()
                .any(|d| d.code == "E_UNSUPPORTED" && d.message.contains("Sprint 10")),
            "{:?}",
            v.diagnostics
        );
    }

    #[test]
    fn only_sprint1_elements_stay_renderable() {
        let folder = tmp_dir("s1only");
        write(&folder, validate_mod::COMPOSITION_FILE, VALID);
        assert_eq!(validate_folder(&folder).status, Status::Valid);
    }

    #[test]
    fn line_height_and_letter_spacing_are_supported() {
        // Sprint 2 (T002): both spellings validate; negative values fail.
        for attr in ["line-height", "letter_spacing"] {
            let folder = tmp_dir("textattr");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2 {attr}=2\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Valid, "{attr}: {:?}", v.diagnostics);
        }
        let folder = tmp_dir("textattrbad");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=2 letter-spacing=-2\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_SPACING"));
    }

    #[test]
    fn font_family_is_supported() {
        // Sprint 2 follow-up: CSS-like `font-family` lists validate in
        // both spellings; an empty list fails with E_BAD_FONT.
        for attr in ["font-family", "font_family"] {
            let folder = tmp_dir("fontfam");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2 {attr}=\"DejaVu Sans, sans-serif\"\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Valid, "{attr}: {:?}", v.diagnostics);
        }
        let folder = tmp_dir("fontfambad");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=2 font-family=\" , \"\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_FONT"));
    }

    #[test]
    fn valid_still_elements_pass_validation() {
        // T002: a box, an ordered text clip, and spacing validate with no
        // diagnostics and stay render candidates.
        let folder = tmp_dir("stillvalid");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=1 order=1 line-height=28 letter-spacing=2\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" radius=2 border-width=1 border-color=\"#000000\" opacity=0.5 order=2 start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        assert!(v.diagnostics.is_empty());
        let comp = v.composition.unwrap();
        assert_eq!(comp.clips.len(), 2);
        assert_eq!(comp.clips[0].order(), 1);
        assert_eq!(comp.clips[1].order(), 2);
    }

    #[test]
    fn still_element_problems_list_every_code() {
        // T002: one run reports every problem with file, line, column,
        // and code; render refuses before the first frame the same way.
        // One clip per problem: each clip reports its own first defect.
        let folder = tmp_dir("stillbad");
        // Passes the magic check so fit and placement are reached.
        std::fs::write(folder.join("ok.png"), b"\x89PNG\r\n\x1a\npadding").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    box x=0 y=0 width=0 height=4 fill=\"#ff0000\" start=0 duration=1\n    box x=0 y=0 width=4 height=4 fill=\"red\" start=0 duration=1\n    box x=0 y=0 width=4 height=4 fill=\"#ff0000\" opacity=2 start=0 duration=1\n    text value=\"Hi\" start=0 duration=1 order=\"front\"\n    text value=\"Hi\" start=0 duration=1 letter-spacing=-1\n    image path=\"gone.png\" start=0 duration=1\n    image path=\"ok.png\" start=0 duration=1 fit=\"zoom\"\n    image path=\"ok.png\" start=0 duration=1 placement=\"north\"\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        let codes: Vec<&str> = v.diagnostics.iter().map(|d| d.code).collect();
        for want in ["E_BAD_BOX", "E_BAD_COLOR", "E_BAD_ORDER", "E_BAD_SPACING", "E_MISSING_FILE", "E_BAD_FIT", "E_BAD_IMAGE"] {
            assert!(codes.contains(&want), "missing {want} in {codes:?}");
        }
        for d in &v.diagnostics {
            assert!(!d.file.is_empty() && d.line >= 1 && d.column >= 1, "{d:?}");
            assert!(!d.fix.is_empty(), "{d:?}");
        }
        let human = format_check_human(&folder, &v);
        let json = format_check_json(&folder, &v);
        for want in ["E_BAD_BOX", "E_BAD_ORDER", "E_BAD_SPACING", "E_MISSING_FILE"] {
            assert!(human.contains(want), "{human}");
            assert!(json.contains(want), "{json}");
        }
    }

    #[test]
    fn unsupported_image_formats_name_file_and_sprint() {
        // T002/Sprint 5 T007: beyond PNG/JPEG/still-GIF/HEIC stays
        // not-yet-supported (exit 2), never a guessed render.
        let folder = tmp_dir("imgfmt");
        write(&folder, "photo.webp", "not a webp at all");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    image path=\"photo.webp\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Unsupported, "{:?}", v.diagnostics);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_UNSUPPORTED" && d.message.contains("photo.webp")));
    }

    /// Sprint 5 T007: an animated GIF in `image` is invalid (exit 1)
    /// with a diagnostic pointing at `video`, while a still GIF and a
    /// HEIC still validate like PNG/JPEG.
    #[test]
    fn animated_gif_in_image_refuses_pointing_at_video() {
        let folder = tmp_dir("imganim");
        // Two-frame GIF: header, color table, two GCE+image pairs,
        // trailer (plays once — still refused, animations live in
        // `video` regardless of loop count).
        let mut gif = Vec::new();
        gif.extend_from_slice(b"GIF89a");
        gif.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
        gif.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        for _ in 0..2 {
            gif.extend_from_slice(&[0x21, 0xF9, 0x04, 10, 0, 0, 0, 0]);
            gif.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
            gif.extend_from_slice(&[2, 2, 0x44, 1, 0]);
        }
        gif.extend_from_slice(&[0x3B]);
        std::fs::write(folder.join("anim.gif"), &gif).unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    image path=\"anim.gif\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert!(
            v.diagnostics.iter().any(|d| d.code == "E_BAD_IMAGE"
                && d.message.contains("anim.gif")
                && d.message.contains("`video`")),
            "{:?}",
            v.diagnostics
        );
        assert_eq!(cmd_check(&folder, false, None), 1);
        assert!(!folder.join("out.mp4").exists());
        // A single frame of the same bytes is a still GIF: valid.
        let mut still = Vec::new();
        still.extend_from_slice(b"GIF89a");
        still.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
        still.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        still.extend_from_slice(&[0x21, 0xF9, 0x04, 0, 0, 0, 0, 0]);
        still.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
        still.extend_from_slice(&[2, 2, 0x44, 1, 0]);
        still.extend_from_slice(&[0x3B]);
        std::fs::write(folder.join("still.gif"), &still).unwrap();
        // A minimal HEIC head: `ftyp` box with a HEIF brand. Check-only:
        // the decoder below gates the pixels at render time.
        let mut heic = vec![0u8; 4];
        heic.extend_from_slice(b"ftypheic");
        heic.extend_from_slice(&[0, 0, 0, 0]);
        heic.extend_from_slice(b"mif1");
        std::fs::write(folder.join("photo.heic"), &heic).unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    image path=\"still.gif\" start=0 duration=1\n    image path=\"photo.heic\" start=1 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    }

    #[test]
    fn corrupt_images_are_invalid_not_unsupported() {
        let folder = tmp_dir("imgbad");
        write(&folder, "bad.png", "not a png at all");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    image path=\"bad.png\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_IMAGE"));
    }

    #[test]
    fn render_refuses_unsupported_before_first_frame() {
        let folder = tmp_dir("refuse-render");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    automation lane=\"volume\" start=0 duration=1\n}\n",
        );
        assert_eq!(cmd_render(&folder, None), 2);
        assert!(!folder.join("out.mp4").exists());
    }

    #[test]
    fn render_refuses_invalid_stills_before_first_frame() {
        // T002: render revalidates and stops with the same diagnostics.
        let folder = tmp_dir("refuse-still");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    box width=10 height=10 fill=\"#ff0000\" start=0 duration=1 order=1.5\n}\n",
        );
        let before = validate_folder(&folder);
        assert_eq!(before.status, Status::Invalid);
        assert_eq!(cmd_render(&folder, None), 1);
        let after = validate_folder(&folder);
        assert_eq!(
            before.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            after.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            "render must refuse with the same diagnostics check reports"
        );
        assert!(!folder.join("out.mp4").exists());
    }

    #[test]
    fn render_refuses_invalid_without_mp4() {
        let folder = tmp_dir("refuse-invalid");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=0 height=64 duration=1 background=\"#000\" version=1 {\n}\n",
        );
        assert_eq!(cmd_render(&folder, None), 1);
        assert!(!folder.join("out.mp4").exists());
    }

    #[test]
    fn timing_shapes() {
        let folder = tmp_dir("timing");
        // Missing start.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    text value=\"Hi\" duration=2\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_TIMING"));
        // Zero duration.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=0\n}\n",
        );
        assert_eq!(validate_folder(&folder).status, Status::Invalid);
        // Negative start.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    text value=\"Hi\" start=-1 duration=2\n}\n",
        );
        assert_eq!(validate_folder(&folder).status, Status::Invalid);
        // Relative start string.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    text value=\"Hi\" start=\"after:intro\" duration=2\n}\n",
        );
        assert_eq!(validate_folder(&folder).status, Status::Invalid);
        // End-derived duration.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 end=2\n}\n",
        );
        assert_eq!(validate_folder(&folder).status, Status::Invalid);
        // Overrun is clipped, not extended: still valid.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    text value=\"Hi\" start=3 duration=5\n}\n",
        );
        assert_eq!(validate_folder(&folder).status, Status::Valid);
    }

    #[test]
    fn check_reports_every_problem_in_one_run() {
        // T006: notation, reference and timing problems all surface together.
        let folder = tmp_dir("allproblems");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 bogus=1 background=\"#000\" version=1 {\n\
             \x20   text value=\"Hi\" duration=0\n\
             \x20   video path=\"media/gone.mp4\" start=0 duration=1\n\
             }\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        let codes: Vec<&str> = v.diagnostics.iter().map(|d| d.code).collect();
        assert!(codes.contains(&"E_UNKNOWN_KEY"), "{codes:?}");
        assert!(codes.contains(&"E_BAD_TIMING"), "{codes:?}");
        assert!(codes.contains(&"E_MISSING_FILE"), "{codes:?}");
        assert!(codes.len() >= 3, "{codes:?}");
        // Every diagnostic is located with file, line, column and code.
        for d in &v.diagnostics {
            assert!(!d.file.is_empty());
            assert!(d.line >= 1);
            assert!(d.column >= 1);
            assert!(!d.code.is_empty());
            assert!(!d.fix.is_empty());
        }
    }

    #[test]
    fn human_and_json_reports_carry_the_same_problems() {
        // T007: the two forms report exactly the same set of problems.
        let folder = tmp_dir("parity");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 bogus=1 background=\"#000\" version=1 {\n\
             \x20   text value=\"Hi\" duration=0\n\
             \x20   video path=\"media/gone.mp4\" start=0 duration=1\n\
             }\n",
        );
        let v = validate_folder(&folder);
        let human = format_check_human(&folder, &v);
        let json = format_check_json(&folder, &v);
        for d in &v.diagnostics {
            assert!(human.contains(&format!("error[{}]", d.code)), "{human}");
            assert!(json.contains(&format!("\"code\":\"{}\"", d.code)), "{json}");
            assert!(json.contains(&format!("\"line\":{}", d.line)), "{json}");
            assert!(json.contains(&format!("\"column\":{}", d.column)), "{json}");
        }
        // Code counts agree between the two forms.
        for code in ["E_UNKNOWN_KEY", "E_BAD_TIMING", "E_MISSING_FILE"] {
            let in_human = human.matches(&format!("error[{code}]")).count();
            let in_json = json.matches(&format!("\"code\":\"{code}\"")).count();
            assert!(in_human >= 1, "{human}");
            assert_eq!(in_human, in_json, "human and JSON disagree on {code}");
        }
        assert!(json.starts_with("{\"tool\":\"framesmith\""), "{json}");
    }

    #[test]
    fn json_reports_valid_and_unsupported_verdicts() {
        let folder = tmp_dir("jsonverdicts");
        write(&folder, validate_mod::COMPOSITION_FILE, VALID);
        let v = validate_folder(&folder);
        let json = format_check_json(&folder, &v);
        assert!(json.contains("\"status\":\"valid\""), "{json}");
        assert!(json.contains("\"exit\":0"), "{json}");
        assert!(json.contains("\"diagnostics\":[]"), "{json}");

        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    automation lane=\"volume\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(cmd_check(&folder, false, None), 2);
        assert_eq!(cmd_check(&folder, true, None), 2);
        let json = format_check_json(&folder, &v);
        assert!(json.contains("\"status\":\"unsupported\""), "{json}");
        assert!(json.contains("\"exit\":2"), "{json}");
        assert!(json.contains("E_UNSUPPORTED"), "{json}");
    }

    #[test]
    fn catalog_explains_every_reported_code() {
        // T007: no reported code goes without meaning and fix.
        let folder = tmp_dir("catexplain");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition width=0 height=64 duration=-1 fps=29.97 background=\"red\" version=0 {\n\
             \x20   text start=-1 duration=0\n\
             \x20   video path=\"gone.mp4\" start=0 duration=1 fit=\"zoom\"\n\
             }\n",
        );
        let v = validate_folder(&folder);
        assert!(!v.diagnostics.is_empty());
        for d in &v.diagnostics {
            let entry =
                catalog::lookup(d.code).unwrap_or_else(|| panic!("{} has no catalog entry", d.code));
            assert!(!entry.meaning.is_empty());
            assert!(!entry.example.is_empty());
            assert!(!entry.fix.is_empty());
        }
    }

    #[test]
    fn text_attributes_validate() {
        // size must be positive.
        for size in ["0", "-5", "\"huge\""] {
            let folder = tmp_dir("textsize");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2 size={size}\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Invalid, "size={size}: {:?}", v.diagnostics);
            assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_TIMING"));
        }
        // align is exactly left/center/right.
        let folder = tmp_dir("badalign");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=2 align=\"diagonal\"\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_TIMING"));
        // wrap is "off" or a positive width.
        for wrap in ["\"banana\"", "-3", "0"] {
            let folder = tmp_dir("badwrap");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2 wrap={wrap}\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Invalid, "wrap={wrap}: {:?}", v.diagnostics);
        }
        // position is an "X,Y" pair.
        let folder = tmp_dir("badpos");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=2 position=\"nowhere\"\n}\n",
        );
        assert_eq!(validate_folder(&folder).status, Status::Invalid);
        // Valid attributes stay renderable.
        let folder = tmp_dir("goodattrs");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    text value=\"Hi there\" start=0 duration=2 x=4 y=6 size=12 color=\"#fff\" align=\"center\" wrap=40 position=\"4,6\"\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        let clip = match &v.composition.unwrap().clips[0] {
            notation::model::Clip::Text(t) => t.clone(),
            c => panic!("expected text, got {c:?}"),
        };
        assert_eq!((clip.x, clip.y), (4.0, 6.0));
        assert!(matches!(clip.align, notation::model::Align::Center));
        assert!(matches!(clip.wrap, notation::model::Wrap::Width(w) if w == 40.0));
        // position feeds defaults; explicit x wins.
        let folder = tmp_dir("posoverride");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=2 x=9 position=\"4,6\"\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        match &v.composition.unwrap().clips[0] {
            notation::model::Clip::Text(t) => {
                assert_eq!((t.x, t.y), (9.0, 6.0));
            }
            c => panic!("expected text, got {c:?}"),
        }
    }

    #[test]
    fn font_files_validate_format() {
        // Declared but absent: E_MISSING_FILE (never silent substitution).
        let folder = tmp_dir("nofont");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    font path=\"fonts/gone.ttf\"\n    text value=\"Hi\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_MISSING_FILE"));
        // Present but not a supported font: E_BAD_FONT.
        for (name, bytes) in [
            ("fonts/web.woff", b"wOFFjunkjunkjunk".as_slice()),
            ("fonts/blob.ttf", b"definitely not a font".as_slice()),
        ] {
            let folder = tmp_dir("badfont");
            std::fs::create_dir_all(folder.join("fonts")).unwrap();
            write(&folder, validate_mod::COMPOSITION_FILE, &format!(
                "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {{\n    font path=\"{name}\"\n    text value=\"Hi\" start=0 duration=1\n}}\n"
            ));
            std::fs::write(folder.join(name), bytes).unwrap();
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Invalid, "{name}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics.iter().any(|d| d.code == "E_BAD_FONT"),
                "{name}: {:?}",
                v.diagnostics
            );
            // The catalog explains the new code.
            assert!(validate::catalog::lookup("E_BAD_FONT").is_some());
        }
    }

    #[test]
    fn valid_motion_directives_pass_validation() {
        // T003: tweens in all three forms, set holds, and easing names
        // including `out-back(n)` validate with no diagnostics and land
        // in the composition for later drawing phases.
        let folder = tmp_dir("motionvalid");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=4\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=4\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"linear\"\n    tween target=1 property=\"opacity\" kind=\"from\" start=1 duration=1 from=0 ease=\"in-out-sine\"\n    tween target=0 property=\"y\" kind=\"from-to\" start=2 duration=1 from=4 to=20 ease=\"out-back(1.7)\"\n    set target=1 property=\"scale\" start=0 duration=2 value=1.5\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        assert!(v.diagnostics.is_empty());
        let comp = v.composition.unwrap();
        assert_eq!(comp.tweens.len(), 3);
        assert_eq!(comp.sets.len(), 1);
        assert_eq!(comp.tweens[0].property, "x");
        assert!(matches!(
            comp.tweens[0].kind,
            notation::model::TweenKind::To
        ));
        assert!(matches!(
            comp.tweens[2].ease,
            notation::model::EaseDecl::OutBack(_)
        ));
        assert_eq!(comp.sets[0].value, 1.5);
        // Valid motion still renders (statically until T009/T011 draw it).
        assert_eq!(cmd_check(&folder, false, None), 0);
        assert_eq!(cmd_check(&folder, true, None), 0);
    }

    #[test]
    fn motion_problems_list_every_code() {
        // T003: one run reports every motion problem with file, line,
        // column, and code, identically in human and machine forms; the
        // catalog covers the new codes; render refuses with exit 1.
        let folder = tmp_dir("motionbad");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=2\n    tween property=\"x\" kind=\"to\" start=0 duration=1 to=10\n    tween target=9 property=\"x\" kind=\"to\" start=0 duration=1 to=10\n    tween target=0 property=\"wobble\" kind=\"to\" start=0 duration=1 to=10\n    tween target=0 property=\"x\" kind=\"sideways\" start=0 duration=1 to=10\n    tween target=0 property=\"x\" kind=\"to\" start=-1 duration=1 to=10\n    tween target=0 property=\"x\" kind=\"from\" start=0 duration=1 to=10\n    tween target=0 property=\"opacity\" kind=\"to\" start=0 duration=1 to=2\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"fast\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"out-back\"\n    set target=0 property=\"y\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        let codes: Vec<&str> = v.diagnostics.iter().map(|d| d.code).collect();
        for want in ["E_BAD_MOTION", "E_BAD_TIMING", "E_BAD_EASING"] {
            assert!(codes.contains(&want), "missing {want} in {codes:?}");
        }
        // Ten bad directives, each located with a fix direction.
        assert!(v.diagnostics.len() >= 10, "{:?}", v.diagnostics);
        for d in &v.diagnostics {
            assert!(!d.file.is_empty() && d.line >= 1 && d.column >= 1, "{d:?}");
            assert!(!d.fix.is_empty(), "{d:?}");
        }
        // The bad easing names the accepted set.
        assert!(
            v.diagnostics.iter().any(|d| d.code == "E_BAD_EASING"
                && d.message.contains("in-out-sine")),
            "{:?}",
            v.diagnostics
        );
        let human = format_check_human(&folder, &v);
        let json = format_check_json(&folder, &v);
        for want in ["E_BAD_MOTION", "E_BAD_EASING"] {
            assert!(human.contains(want), "{human}");
            assert!(json.contains(want), "{json}");
        }
        for d in &v.diagnostics {
            assert!(json.contains(d.code), "{json}");
            assert!(json.contains(&d.fix.replace('"', "\\\"")), "{json}");
        }
        assert!(validate::catalog::lookup("E_BAD_MOTION").is_some());
        assert!(validate::catalog::lookup("E_BAD_EASING").is_some());
        // Render revalidates and refuses before the first frame.
        assert_eq!(cmd_render(&folder, None), 1);
        assert!(!folder.join("out.mp4").exists());
        assert!(!folder.join("out.mp4.framesmith-part").exists());
    }

    #[test]
    fn later_motion_properties_and_easings_wait_for_sprint_7() {
        // T003: well-formed use of beyond-Sprint-2 properties and easings
        // exits 2 naming the property/easing with its enabling sprint.
        for prop in ["scale-x", "blur", "rotate-x", "typed"] {
            let folder = tmp_dir("laterprop");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!("composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2\n    tween target=0 property=\"{prop}\" kind=\"to\" start=0 duration=1 to=1\n}}\n"),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Unsupported, "{prop}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics.iter().any(|d| d.code == "E_UNSUPPORTED"
                    && d.message.contains(prop)
                    && d.message.contains("Sprint 7")),
                "{prop}: {:?}",
                v.diagnostics
            );
            assert_eq!(cmd_check(&folder, false, None), 2);
            assert_eq!(cmd_check(&folder, true, None), 2);
            assert_eq!(cmd_render(&folder, None), 2);
            assert!(!folder.join("out.mp4").exists());
        }
        for ease in ["spring", "bounce", "cubic-bezier(0.25,0.1,0.25,1)"] {
            let folder = tmp_dir("laterease");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!("composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"{ease}\"\n}}\n"),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Unsupported, "{ease}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics.iter().any(|d| d.code == "E_UNSUPPORTED"
                    && d.message.contains("Sprint 7")),
                "{ease}: {:?}",
                v.diagnostics
            );
        }
    }

    #[test]
    fn rich_box_looks_wait_for_sprint_8() {
        // T004: per-side borders, border images, and non-solid fills are
        // refused as not-yet-supported with Sprint 8, never guessed.
        for key in ["border-top-width", "border-image", "gradient"] {
            let folder = tmp_dir("richbox");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!("composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {{\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" {key}=1 start=0 duration=1\n}}\n"),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Unsupported, "{key}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics.iter().any(|d| d.code == "E_UNSUPPORTED"
                    && d.message.contains(key)
                    && d.message.contains("Sprint 8")),
                "{key}: {:?}",
                v.diagnostics
            );
            assert_eq!(cmd_render(&folder, None), 2);
            assert!(!folder.join("out.mp4").exists());
        }
    }

    #[test]
    fn missing_files_are_located() {
        let folder = tmp_dir("missing");
        assert_eq!(validate_folder(&folder.join("nope")).status, Status::Invalid);
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    video path=\"media/gone.mp4\" start=0 duration=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_MISSING_FILE"));
        // Unknown video fit is invalid, never silent.
        std::fs::create_dir_all(folder.join("media")).unwrap();
        std::fs::write(folder.join("media/clip.mp4"), b"fake").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    video path=\"media/clip.mp4\" start=0 duration=1 fit=\"zoom\"\n}\n",
        );
        let v = validate_folder(&folder);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_FIT"));
    }

    /// T007: a valid folder with `audio` tracks (trim, volume, fades)
    /// and video sound keys checks with exit code 0 and zero
    /// diagnostics, carrying the authored values into the model.
    #[test]
    fn audio_tracks_validate_with_trim_volume_and_fades() {
        let folder = tmp_dir("audiovalid");
        std::fs::write(folder.join("s.wav"), b"RIFF....").unwrap();
        std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    audio path=\"s.wav\" start=0 duration=2 trim-start=0.5 trim-end=1.5 volume=6 fade-in=0.25 fade-out=0.5\n    audio path=\"s.wav\" start=2 duration=2 trim_start=0 volume=\"-inf\" fade_in=0 fade_out=0\n    video path=\"clip.mp4\" start=0 duration=1 volume=-3 fade-in=0.2 fade-out=0.2\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        assert!(v.diagnostics.is_empty());
        let comp = v.composition.unwrap();
        assert_eq!(comp.clips.len(), 3);
        match &comp.clips[0] {
            notation::model::Clip::Audio(a) => {
                assert_eq!((a.start, a.duration), (0.0, 2.0));
                assert_eq!((a.trim_start, a.trim_end), (0.5, Some(1.5)));
                assert_eq!(a.volume_db, 6.0);
                assert_eq!((a.fade_in, a.fade_out), (0.25, 0.5));
            }
            c => panic!("expected audio, got {c:?}"),
        }
        match &comp.clips[1] {
            notation::model::Clip::Audio(a) => {
                assert_eq!(a.volume_db, f64::NEG_INFINITY, "silence stays exact");
                assert_eq!((a.trim_start, a.trim_end), (0.0, None));
            }
            c => panic!("expected audio, got {c:?}"),
        }
        match &comp.clips[2] {
            notation::model::Clip::Video(vd) => {
                assert_eq!(vd.volume_db, -3.0);
                assert_eq!((vd.fade_in, vd.fade_out), (0.2, 0.2));
            }
            c => panic!("expected video, got {c:?}"),
        }
        // Audio draws no picture, so it never takes part in paint order.
        assert_eq!(comp.clips[0].order(), 0);
    }

    /// T007: one run lists every audio problem — missing and
    /// unsupported files, out-of-range volume, overlong fades, bad
    /// trims, and unknown keys — identically in human and `--json` form.
    #[test]
    fn audio_problems_list_every_code_in_one_run() {
        let folder = tmp_dir("audiobad");
        std::fs::write(folder.join("s.wav"), b"RIFF....").unwrap();
        std::fs::write(folder.join("s.xyz"), b"mystery").unwrap();
        std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    audio path=\"gone.wav\" start=0 duration=1\n    audio path=\"s.wav\" start=0 duration=1 volume=20\n    audio path=\"s.wav\" start=0 duration=1 fade-in=5\n    audio path=\"s.wav\" start=0 duration=1 trim-start=-1\n    audio path=\"s.wav\" start=0 duration=1 trim-start=2 trim-end=1\n    audio path=\"s.wav\" start=0 duration=1 bogus=1\n    audio path=\"s.xyz\" start=0 duration=1\n    video path=\"clip.mp4\" start=0 duration=1 volume=13\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        let codes: Vec<&str> = v.diagnostics.iter().map(|d| d.code).collect();
        for want in [
            "E_MISSING_FILE",
            "E_BAD_AUDIO",
            "E_UNKNOWN_KEY",
            "E_UNSUPPORTED",
        ] {
            assert!(codes.contains(&want), "missing {want} in {codes:?}");
        }
        assert_eq!(
            codes.iter().filter(|c| **c == "E_BAD_AUDIO").count(),
            5,
            "volume, fade, negative trim, backward trim, video volume: {codes:?}"
        );
        for d in &v.diagnostics {
            assert!(!d.file.is_empty() && d.line >= 1 && d.column >= 1, "{d:?}");
            assert!(!d.fix.is_empty(), "{d:?}");
            assert!(catalog::lookup(d.code).is_some(), "{} has no catalog entry", d.code);
        }
        let human = format_check_human(&folder, &v);
        let json = format_check_json(&folder, &v);
        for want in ["E_MISSING_FILE", "E_BAD_AUDIO", "E_UNKNOWN_KEY", "E_UNSUPPORTED"] {
            let in_human = human.matches(&format!("error[{want}]")).count();
            let in_json = json.matches(&format!("\"code\":\"{want}\"")).count();
            assert!(in_human >= 1, "{human}");
            assert_eq!(in_human, in_json, "human and JSON disagree on {want}");
        }
    }

    /// T007: video sound keys validate on their own — over-ceiling
    /// volume, overlong fades, and unreadable volumes are invalid while
    /// unity defaults stay valid.
    #[test]
    fn video_sound_keys_validate() {
        // Unity defaults (no sound keys) stay valid.
        let folder = tmp_dir("vidsnd");
        std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        // Each sound misuse is its own invalid verdict.
        for (tag, line) in [
            ("loud", "video path=\"clip.mp4\" start=0 duration=2 volume=12.5"),
            ("fade", "video path=\"clip.mp4\" start=0 duration=2 fade-out=9"),
            ("shape", "video path=\"clip.mp4\" start=0 duration=2 volume=\"loud\""),
        ] {
            let folder = tmp_dir("vidsndbad");
            std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    {line}\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics.iter().any(|d| d.code == "E_BAD_AUDIO"),
                "{tag}: {:?}",
                v.diagnostics
            );
        }
        // Boundary: exactly +12 dB is valid, anything above is not.
        for (want, volume) in [(Status::Valid, "12"), (Status::Invalid, "12.5")] {
            let folder = tmp_dir("vidsndedge");
            std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    video path=\"clip.mp4\" start=0 duration=2 volume={volume}\n}}\n"
                ),
            );
            assert_eq!(validate_folder(&folder).status, want, "volume={volume}");
        }
    }

    /// Sprint 5 T001: trim/rate declaration checks that need no probe
    /// (fake bytes keep the probe silent): a backward window, a
    /// zero-length window, and rates below 0.1, above 10, zero, or
    /// negative each fail with a located `E_BAD_FOOTAGE` diagnostic
    /// naming the accepted rule.
    #[test]
    fn video_trim_and_rate_declarations_validate_without_probe() {
        for (tag, line) in [
            ("backward", "video path=\"clip.mp4\" start=0 duration=2 trim-start=3 trim-end=1"),
            ("zero", "video path=\"clip.mp4\" start=0 duration=2 trim-start=2 trim-end=2"),
            ("low", "video path=\"clip.mp4\" start=0 duration=2 rate=0.05"),
            ("high", "video path=\"clip.mp4\" start=0 duration=2 rate=20"),
            ("zero-rate", "video path=\"clip.mp4\" start=0 duration=2 rate=0"),
            ("negative-rate", "video path=\"clip.mp4\" start=0 duration=2 rate=-1"),
            ("negative-trim", "video path=\"clip.mp4\" start=0 duration=2 trim-start=-1"),
        ] {
            let folder = tmp_dir("vidfootage");
            std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    {line}\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics.iter().all(|d| d.code == "E_BAD_FOOTAGE"),
                "{tag}: {:?}",
                v.diagnostics
            );
            for d in &v.diagnostics {
                assert!(!d.file.is_empty() && d.line >= 1 && d.column >= 1, "{tag}: {d:?}");
                assert!(!d.fix.is_empty(), "{tag}: {d:?}");
                assert!(validate::catalog::lookup(d.code).is_some());
            }
            // Boundary values stay in the message: rates name 0.1–10,
            // windows name both ends.
            let human = format_check_human(&folder, &v);
            let json = format_check_json(&folder, &v);
            assert!(human.contains("error[E_BAD_FOOTAGE]"), "{tag}: {human}");
            assert!(json.contains("\"code\":\"E_BAD_FOOTAGE\""), "{tag}: {json}");
            if line.contains("rate") {
                assert!(human.contains("0.1 to 10"), "{tag}: {human}");
            }
        }
        // Boundary: exactly 0.1 and 10 are valid rates.
        for rate in ["0.1", "10"] {
            let folder = tmp_dir("vidrateedge");
            std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    video path=\"clip.mp4\" start=0 duration=2 rate={rate}\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Valid, "rate={rate}: {:?}", v.diagnostics);
        }
    }

    /// Sprint 5 T001: several footage problems in one composition are
    /// all reported together (one clip can name a bad rate and a
    /// backward window at once), and exit codes stay 0/1.
    #[test]
    fn video_footage_problems_report_together() {
        let folder = tmp_dir("vidfootmulti");
        std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2 trim-start=3 trim-end=1 rate=20\n    video path=\"clip.mp4\" start=2 duration=2 rate=0\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert_eq!(v.diagnostics.len(), 3, "{:?}", v.diagnostics);
        assert!(
            v.diagnostics.iter().all(|d| d.code == "E_BAD_FOOTAGE"),
            "{:?}",
            v.diagnostics
        );
        assert_eq!(cmd_check(&folder, false, None), 1);
        assert_eq!(cmd_check(&folder, true, None), 1);
        let human = format_check_human(&folder, &v);
        let json = format_check_json(&folder, &v);
        assert_eq!(
            human.matches("error[E_BAD_FOOTAGE]").count(),
            3,
            "{human}"
        );
        assert_eq!(
            json.matches("\"code\":\"E_BAD_FOOTAGE\"").count(),
            3,
            "{json}"
        );
    }

    /// Sprint 5 T001: underscore aliases validate identically to
    /// hyphenated spellings, and clips without footage keys keep their
    /// defaults with no behavior change.
    #[test]
    fn video_trim_underscore_aliases_match_hyphenated() {
        let folder = tmp_dir("vidfootalias");
        std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2 trim_start=0.5 trim_end=1.5\n    video path=\"clip.mp4\" start=2 duration=2\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        assert!(v.diagnostics.is_empty());
        let comp = v.composition.unwrap();
        match &comp.clips[0] {
            notation::model::Clip::Video(vd) => {
                assert_eq!((vd.trim_start, vd.trim_end), (0.5, Some(1.5)));
                assert_eq!(vd.rate, 1.0);
            }
            c => panic!("expected video, got {c:?}"),
        }
        match &comp.clips[1] {
            notation::model::Clip::Video(vd) => {
                assert_eq!((vd.trim_start, vd.trim_end, vd.rate), (0.0, None, 1.0));
            }
            c => panic!("expected video, got {c:?}"),
        }
        // A backward window through aliases fails exactly like the
        // hyphenated spelling.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2 trim_start=3 trim_end=1\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert!(
            v.diagnostics.iter().any(|d| d.code == "E_BAD_FOOTAGE"
                && d.message.contains("backward")),
            "{:?}",
            v.diagnostics
        );
    }

    /// Sprint 5 T003: a looping clip validates with its window echoed
    /// in both forms, while a looping clip without `duration` and a
    /// non-boolean `loop` fail `check` with located `E_BAD_FOOTAGE`
    /// diagnostics asking for the fix.
    #[test]
    fn video_loop_validates_and_unbounded_loop_refuses() {
        let folder = tmp_dir("vidfootloop");
        std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=3 trim-start=0.5 trim-end=1.5 rate=2 loop=true\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        assert_eq!(v.footage.len(), 1);
        assert!(v.footage[0].loop_clip);
        match &v.composition.as_ref().unwrap().clips[0] {
            notation::model::Clip::Video(vd) => assert!(vd.loop_clip),
            c => panic!("expected video, got {c:?}"),
        }
        let human = format_check_human(&folder, &v);
        let json = format_check_json(&folder, &v);
        assert!(human.contains("loop on"), "{human}");
        assert!(json.contains("\"loop\":true"), "{json}");
        // A looping clip without `duration` asks for one explicitly.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 loop=true\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert!(
            v.diagnostics.iter().any(|d| d.code == "E_BAD_FOOTAGE"
                && d.message.contains("explicit `duration`")),
            "{:?}",
            v.diagnostics
        );
        let human = format_check_human(&folder, &v);
        assert!(human.contains("error[E_BAD_FOOTAGE]"), "{human}");
        assert_eq!(cmd_check(&folder, false, None), 1);
        assert!(!folder.join("out.mp4").exists());
        // A non-boolean `loop` names the accepted spellings.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2 loop=\"yes\"\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
        assert!(
            v.diagnostics.iter().any(|d| d.code == "E_BAD_FOOTAGE"
                && d.message.contains("`true` or `false`")),
            "{:?}",
            v.diagnostics
        );
        // `loop=false` validates exactly like an omitted `loop`.
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2 loop=false\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        assert!(!v.footage[0].loop_clip);
        assert!(format_check_human(&folder, &v).contains("loop off"));
    }

    /// T007: later-sprint audio use — effects, automation, groups, and
    /// ducking keys plus the matching elements — checks with exit code
    /// 2 naming the element and its enabling sprint, in both forms, and
    /// `render` refuses before the first frame with no MP4.
    #[test]
    fn later_sprint_audio_use_names_enabling_sprint() {
        for (tag, body) in [
            ("key", "audio path=\"s.wav\" start=0 duration=1 effect=\"reverb\""),
            ("vkey", "video path=\"clip.mp4\" start=0 duration=1 ducking=0.5"),
            ("elem", "automation lane=\"volume\" start=0 duration=1"),
            ("group", "group name=\"drums\""),
        ] {
            let folder = tmp_dir("audiolater");
            std::fs::write(folder.join("s.wav"), b"RIFF....").unwrap();
            std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    {body}\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Unsupported, "{tag}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics
                    .iter()
                    .any(|d| d.code == "E_UNSUPPORTED" && d.message.contains("Sprint 10")),
                "{tag}: {:?}",
                v.diagnostics
            );
            let human = format_check_human(&folder, &v);
            let json = format_check_json(&folder, &v);
            assert!(human.contains("Sprint 10"), "{tag}: {human}");
            assert!(json.contains("Sprint 10"), "{tag}: {json}");
            assert!(json.contains("E_UNSUPPORTED"), "{tag}: {json}");
            assert_eq!(cmd_render(&folder, None), 2);
            assert!(!folder.join("out.mp4").exists(), "{tag}");
            assert!(!folder.join("out.mp4.framesmith-part").exists(), "{tag}");
        }
    }

    /// T007: `render` refuses invalid audio with the same diagnostics
    /// `check` reports, and writes no MP4.
    #[test]
    fn render_refuses_invalid_audio_with_same_diagnostics() {
        let folder = tmp_dir("refuseaudio");
        std::fs::write(folder.join("s.wav"), b"RIFF....").unwrap();
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    audio path=\"s.wav\" start=0 duration=2 volume=20\n    audio path=\"gone.wav\" start=0 duration=1\n}\n",
        );
        let before = validate_folder(&folder);
        assert_eq!(before.status, Status::Invalid);
        assert_eq!(cmd_render(&folder, None), 1);
        let after = validate_folder(&folder);
        assert_eq!(
            before.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            after.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
            "render must refuse with the same diagnostics check reports"
        );
        assert!(!folder.join("out.mp4").exists());
        assert!(!folder.join("out.mp4.framesmith-part").exists());
    }

    /// T007: the catalog explains the new audio code with meaning,
    /// example, and fix.
    #[test]
    fn catalog_covers_audio_codes() {
        let entry = catalog::lookup("E_BAD_AUDIO").expect("E_BAD_AUDIO must be cataloged");
        assert!(!entry.meaning.is_empty());
        assert!(!entry.example.is_empty());
        assert!(!entry.fix.is_empty());
        assert_eq!(cmd_errors(Some("E_BAD_AUDIO"), false), 0);
        assert_eq!(cmd_errors(Some("E_BAD_AUDIO"), true), 0);
    }
}
