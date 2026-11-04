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
use ffmpeg::ffmpeg as ffmpeg_mod;
use log::log;
use notation::model::Composition;
use std::path::{Path, PathBuf};
use validate::catalog::{self, json_escape};
use validate::validate as validate_mod;
use validate_mod::{validate_folder, Status, Verdict};

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
        return Some("name may only contain letters, digits, `-` and `_`".to_string());
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
    println!(
        "created project {} ({}).",
        target.display(),
        validate_mod::COMPOSITION_FILE
    );
    println!(
        "next: run `framesmith check {}` to validate it.",
        target.display()
    );
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

/// One stagger clamp as human text (Sprint 7 motion, CAP-002): the
/// staggered directive, its piece count and largest delay, and the
/// overflow cut at the target clip window, so clamping is reviewable
/// before rendering while the verdict stays valid.
fn format_stagger_clamp_human(f: &validate_mod::StaggerClamp) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let grid = match (f.grid_cols, f.grid_rows) {
        (Some(c), Some(r)) => format!(" grid {c}x{r}"),
        (Some(c), None) => format!(" grid {c} columns"),
        _ => String::new(),
    };
    let _ = writeln!(
        out,
        "note: {} target={} ({}:{}): stagger {:.3}s across {} pieces from {}{} delays the last piece by {:.3}s; motion ends at {:.3}s, past clip end {:.3}s; clamped to the clip window (overflow {:.3}s).",
        f.kind, f.target, f.line, f.column, f.stagger, f.pieces, f.origin, grid, f.max_delay,
        f.motion_end, f.clip_end, f.overflow
    );
    out
}

/// One crossfade clamp as human text (Sprint 10 sound, CAP-003): the
/// handoff pair, its resolved shape, and the overlap cut back to the
/// audible samples, so clamping is reviewable before rendering while
/// the verdict stays valid.
fn format_crossfade_clamp_human(f: &validate_mod::CrossfadeClamp) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "note: crossfade clips {}..{} ({}:{}): overlap {:.3}s..{:.3}s runs past the composition end; clamped to {:.3}s..{:.3}s (shape {}).",
        f.early, f.late, f.line, f.column, f.raw_start, f.raw_end, f.eff_start, f.eff_end, f.shape
    );
    out
}

/// One effect-tail truncation as human text (Sprint 10 sound, CAP-005):
/// the effected clip, its natural delay plus reverb tails, and the cut
/// back to the composition end, so truncation is reviewable before
/// rendering while the verdict stays valid.
fn format_effect_tail_human(f: &validate_mod::EffectTail) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let name = match &f.id {
        Some(id) => format!("{id:?}"),
        None => format!("#{}", f.clip),
    };
    let _ = writeln!(
        out,
        "note: effect tail clip {name} ({}:{}): delay tail {:.3}s plus reverb tail {:.3}s runs to {:.3}s past the composition end; truncated to {:.3}s.",
        f.line, f.column, f.delay_tail, f.reverb_tail, f.raw_end, f.eff_end
    );
    out
}

/// One waveform-range clamp as human text (Sprint 10 sound, CAP-010):
/// the drawing clip, its written range, and the cut back to the clip
/// window, so clamping is reviewable before rendering while the
/// verdict stays valid.
fn format_waveform_clamp_human(f: &validate_mod::WaveformClamp) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let name = match &f.id {
        Some(id) => format!("{id:?}"),
        None => format!("#{}", f.clip),
    };
    let _ = writeln!(
        out,
        "note: waveform {} {} ({}:{}): drawn range {:.3}s..{:.3}s runs past the clip window; clamped to {:.3}s..{:.3}s.",
        f.kind, name, f.line, f.column, f.raw_from, f.raw_to, f.eff_from, f.eff_to
    );
    out
}

/// One pause map as human text (Sprint 10 sound, CAP-006): the opted-in
/// clip with its ceiling and minimum plus every silent range in time
/// order, so pauses are reviewable without opening an audio editor.
fn format_silence_human(f: &validate_mod::SilenceReport) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let name = match &f.id {
        Some(id) => format!("{id:?}"),
        None => format!("#{}", f.clip),
    };
    if f.ranges.is_empty() {
        let _ = writeln!(
            out,
            "silence {} {} ({}:{}): no silent ranges at or below {} dBFS lasting at least {}s.",
            f.kind, name, f.line, f.column, f.threshold_db, f.min_seconds
        );
    } else {
        let parts: Vec<String> = f
            .ranges
            .iter()
            .map(|(s, e)| format!("{s:.3}s..{e:.3}s"))
            .collect();
        let _ = writeln!(
            out,
            "silence {} {} ({}:{}): {} range(s) at or below {} dBFS lasting at least {}s: {}.",
            f.kind,
            name,
            f.line,
            f.column,
            f.ranges.len(),
            f.threshold_db,
            f.min_seconds,
            parts.join(", ")
        );
    }
    out
}

/// One inferred length as human text (Sprint 6 T003): the resolved
/// value of an omitted `duration` with the rule that resolved it, so
/// inference is reviewable before rendering.
fn format_inferred_human(f: &validate_mod::InferredLength) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let length = if f.kind == "composition" {
        format!("duration {:.3}s", f.duration)
    } else {
        format!("clip duration {:.3}s", f.duration)
    };
    match &f.label {
        Some(label) => {
            let _ = writeln!(
                out,
                "inferred {} {label:?} ({}:{}): {length} (inferred {}).",
                f.kind, f.line, f.column, f.detail
            );
        }
        None => {
            let _ = writeln!(
                out,
                "inferred {} ({}:{}): {length} (inferred {}).",
                f.kind, f.line, f.column, f.detail
            );
        }
    }
    out
}

/// First binary of `name` on `PATH`, for peak preview only (Sprint 10
/// sound, CAP-011): `check` and `waveform` never bootstrap or download
/// FFmpeg, so a missing binary skips the preview (`check`) or refuses
/// the listing (`waveform`) instead of fetching anything.
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

/// Silent-normalization gate for `check` (Sprint 10 sound, CAP-009 /
/// REC-005): a normalizing composition whose post-gain preview mix
/// carries no audible samples fails closed here with the same
/// file+line+column+code+fix shape as every other sound refusal, so
/// `check` and `render` agree on silence. Undecodable sources skip
/// silently (the render names them), as does a missing FFmpeg — in
/// both cases `render` still fails closed before the first frame.
fn apply_normalize_silence_gate(folder: &Path, verdict: &mut Verdict) {
    use render::render::PreviewError;
    if verdict.status != Status::Valid {
        return;
    }
    let targets: Vec<Composition> = verdict
        .compositions
        .iter()
        .filter(|c| c.normalize.active())
        .cloned()
        .collect();
    if targets.is_empty() {
        return;
    }
    let Some(ffmpeg_bin) = tool_on_path("ffmpeg") else {
        return;
    };
    let mut refused = false;
    for comp in &targets {
        let silent = match render::render::build_mix_preview(
            comp,
            &verdict.compositions,
            folder,
            &ffmpeg_bin,
        ) {
            Err(PreviewError::Silent(_)) => true,
            Err(PreviewError::Failed(_)) => false,
            Ok(None) => true,
            Ok(Some(pcm)) => render::peaks::peak_of(&pcm) == 0,
        };
        if !silent {
            continue;
        }
        let (line, column) = comp.normalize_loc.unwrap_or((1, 1));
        verdict.diagnostics.push(validate_mod::Diagnostic {
            file: format!("{}/{}", folder.display(), validate_mod::COMPOSITION_FILE),
            line,
            column,
            code: "E_BAD_AUDIO",
            message: format!(
                "composition {:?} sets `normalize=\"loud\"` but its mix carries no audible samples to measure; one overall gain cannot scale silence to −16 LUFS",
                comp.name
            ),
            fix: "add audible sound or remove `normalize`".to_string(),
        });
        refused = true;
    }
    if refused {
        verdict.status = Status::Invalid;
        verdict.composition = None;
        verdict.compositions = Vec::new();
        verdict.root_name = None;
    }
}

/// Hot-master warning naming the peak (Sprint 10 sound, CAP-011 /
/// REC-005): `Some` exactly when the master exceeds −1 dBFS. Shared by
/// `check`, the render summary, and `waveform` so every surface names
/// the same peak for the same bytes; warnings never change an exit
/// code.
fn hot_master_warning(peak: u32) -> Option<String> {
    if render::peaks::is_hot(peak) {
        Some(format!(
            "warning: hot master: peak {} is above -1.0 dBFS; lower a level before release (valid projects still render).",
            render::peaks::format_dbfs(peak)
        ))
    } else {
        None
    }
}

/// Master peak of the render root without rendering (Sprint 10 sound,
/// CAP-011): the exact preview mix through the same placement the
/// staged render mix uses. `None` when there is no FFmpeg on PATH, a
/// source is undecodable (the render names it), or the verdict carries
/// no composition — in all three cases the caller reports no peak and
/// the verdict stands unchanged.
fn preview_master_peak(folder: &Path, verdict: &Verdict) -> Option<u32> {
    let comp = verdict.composition.as_ref()?;
    let ffmpeg_bin = tool_on_path("ffmpeg")?;
    match render::render::build_mix_preview(comp, &verdict.compositions, folder, &ffmpeg_bin) {
        Ok(Some(pcm)) => Some(render::peaks::peak_of(&pcm)),
        Ok(None) => Some(0),
        Err(_) => None,
    }
}

/// Human-readable check report. Pure function so tests can compare it
/// against the `--json` form and the render refusal text (T006/T007).
/// `master` carries the preview master peak when one was measurable
/// (Sprint 10 sound, CAP-011); `None` reports no peak and changes
/// nothing else.
pub fn format_check_human(folder: &Path, verdict: &Verdict, master: Option<u32>) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for d in &verdict.diagnostics {
        let _ = writeln!(out, "{d}");
    }
    for f in &verdict.footage {
        out.push_str(&format_footage_human(f));
    }
    for f in &verdict.inferred {
        out.push_str(&format_inferred_human(f));
    }
    for f in &verdict.stagger_clamps {
        out.push_str(&format_stagger_clamp_human(f));
    }
    for f in &verdict.crossfade_clamps {
        out.push_str(&format_crossfade_clamp_human(f));
    }
    for f in &verdict.effect_tails {
        out.push_str(&format_effect_tail_human(f));
    }
    for f in &verdict.silences {
        out.push_str(&format_silence_human(f));
    }
    for f in &verdict.waveform_clamps {
        out.push_str(&format_waveform_clamp_human(f));
    }
    // Sprint 10 sound (CAP-011): the master peak is always reported
    // when measurable; a hot master additionally warns naming the
    // peak, without changing the exit code.
    if let Some(p) = master {
        let _ = writeln!(out, "master peak: {}.", render::peaks::format_dbfs(p));
        if let Some(w) = hot_master_warning(p) {
            let _ = writeln!(out, "{w}");
        }
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
            // Sprint 6 T006: multi-composition projects name every
            // composition plus the root, so the default render target
            // is reviewable. Single-composition output is unchanged.
            if verdict.compositions.len() > 1 {
                let parts: Vec<String> = verdict
                    .compositions
                    .iter()
                    .map(|m| {
                        let mark = if Some(m.name.as_str()) == verdict.root_name.as_deref() {
                            " (root)"
                        } else {
                            ""
                        };
                        format!("{:?}{mark} ({} clip(s))", m.name, m.clips.len())
                    })
                    .collect();
                let _ = writeln!(out, "compositions: {}.", parts.join(", "));
            }
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
/// code per item. Pure function for testability. `master` carries the
/// preview master peak when one was measurable (Sprint 10 sound,
/// CAP-011); `None` reports null peaks and changes nothing else.
pub fn format_check_json(folder: &Path, verdict: &Verdict, master: Option<u32>) -> String {
    format_verdict_json("check", folder, verdict, master)
}

/// Machine-readable verdict report with a caller-chosen command name
/// (Sprint 6 T010): the `check` JSON shape with the same diagnostics,
/// footage, inferred lengths, compositions, and root, so every listing
/// command refuses exactly like `check` while naming itself.
pub fn format_verdict_json(
    cmd: &str,
    folder: &Path,
    verdict: &Verdict,
    master: Option<u32>,
) -> String {
    use std::fmt::Write;
    let (status, exit) = match verdict.status {
        Status::Valid => ("valid", 0),
        Status::Invalid => ("invalid", 1),
        Status::Unsupported => ("unsupported", 2),
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"{}\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"diagnostics\":[",
        json_escape(cmd),
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
    // Sprint 6 T003: the same inferred lengths as the human echo, so
    // omitted audio, still, and composition lengths are reviewable
    // machine-readably.
    out.push_str("],\"inferred\":[");
    for (i, f) in verdict.inferred.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let label = match &f.label {
            Some(l) => format!("\"{}\"", json_escape(l)),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"kind\":\"{}\",\"label\":{},\"line\":{},\"column\":{},\"duration\":{},\"detail\":\"{}\"}}",
            json_escape(f.kind),
            label,
            f.line,
            f.column,
            num(Some(f.duration)),
            json_escape(&f.detail),
        );
    }
    // Sprint 7 motion: every stagger clamp as reviewable numbers, so
    // overflow cuts are machine-readable like footage clamps.
    out.push_str("],\"stagger_clamps\":[");
    for (i, f) in verdict.stagger_clamps.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let grid_cols = match f.grid_cols {
            Some(c) => c.to_string(),
            None => "null".to_string(),
        };
        let grid_rows = match f.grid_rows {
            Some(r) => r.to_string(),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"kind\":\"{}\",\"target\":{},\"line\":{},\"column\":{},\"pieces\":{},\"stagger\":{},\"origin\":\"{}\",\"grid_cols\":{},\"grid_rows\":{},\"max_delay\":{},\"motion_end\":{},\"clip_end\":{},\"overflow\":{}}}",
            json_escape(f.kind),
            f.target,
            f.line,
            f.column,
            f.pieces,
            num(Some(f.stagger)),
            json_escape(f.origin),
            grid_cols,
            grid_rows,
            num(Some(f.max_delay)),
            num(Some(f.motion_end)),
            num(Some(f.clip_end)),
            num(Some(f.overflow)),
        );
    }
    // Sprint 10 sound: every crossfade clamp as reviewable numbers,
    // so overlong overlap cuts are machine-readable like stagger
    // clamps.
    out.push_str("],\"crossfade_clamps\":[");
    for (i, f) in verdict.crossfade_clamps.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"early\":{},\"late\":{},\"shape\":\"{}\",\"raw_start\":{},\"raw_end\":{},\"eff_start\":{},\"eff_end\":{},\"line\":{},\"column\":{}}}",
            f.early,
            f.late,
            json_escape(f.shape),
            num(Some(f.raw_start)),
            num(Some(f.raw_end)),
            num(Some(f.eff_start)),
            num(Some(f.eff_end)),
            f.line,
            f.column,
        );
    }
    // Sprint 10 sound: every effect-tail truncation as reviewable
    // numbers, so tail cuts are machine-readable like crossfade
    // clamps.
    out.push_str("],\"effect_tails\":[");
    for (i, f) in verdict.effect_tails.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let id = match &f.id {
            Some(v) => format!("\"{}\"", json_escape(v)),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"clip\":{},\"id\":{},\"delay_tail\":{},\"reverb_tail\":{},\"raw_end\":{},\"eff_end\":{},\"line\":{},\"column\":{}}}",
            f.clip,
            id,
            num(Some(f.delay_tail)),
            num(Some(f.reverb_tail)),
            num(Some(f.raw_end)),
            num(Some(f.eff_end)),
            f.line,
            f.column,
        );
    }
    // Sprint 10 sound: every pause map as reviewable numbers, so
    // detected speech pauses are machine-readable like the human
    // listing.
    out.push_str("],\"silences\":[");
    for (i, f) in verdict.silences.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let id = match &f.id {
            Some(v) => format!("\"{}\"", json_escape(v)),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"clip\":{},\"id\":{},\"kind\":\"{}\",\"threshold_db\":{},\"min_seconds\":{},\"line\":{},\"column\":{},\"ranges\":[",
            f.clip,
            id,
            json_escape(f.kind),
            num(Some(f.threshold_db)),
            num(Some(f.min_seconds)),
            f.line,
            f.column,
        );
        for (j, (s, e)) in f.ranges.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"start\":{},\"end\":{}}}",
                num(Some(*s)),
                num(Some(*e)),
            );
        }
        out.push_str("]}");
    }
    // Sprint 10 sound: every waveform-range clamp as reviewable
    // numbers, so drawn-range cuts are machine-readable like effect
    // tails.
    out.push_str("],\"waveform_clamps\":[");
    for (i, f) in verdict.waveform_clamps.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let id = match &f.id {
            Some(v) => format!("\"{}\"", json_escape(v)),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"clip\":{},\"id\":{},\"kind\":\"{}\",\"raw_from\":{},\"raw_to\":{},\"eff_from\":{},\"eff_to\":{},\"line\":{},\"column\":{}}}",
            f.clip,
            id,
            json_escape(f.kind),
            num(Some(f.raw_from)),
            num(Some(f.raw_to)),
            num(Some(f.eff_from)),
            num(Some(f.eff_to)),
            f.line,
            f.column,
        );
    }
    // Sprint 10 sound (CAP-011): the preview master peak plus the hot
    // warning, in the same numbers the human lines carry; nulls when
    // no peak was measurable, so refusal shapes stay identical.
    out.push_str("],\"master_peak\":");
    match master {
        Some(p) => {
            let db = match render::peaks::dbfs_of(p) {
                Some(db) => db.to_string(),
                None => "null".to_string(),
            };
            let _ = write!(
                out,
                "{},\"master_peak_dbfs\":{},\"peak_warning\":",
                render::peaks::linear_of(p),
                db
            );
            match hot_master_warning(p) {
                Some(w) => {
                    let _ = write!(out, "\"{}\"", json_escape(&w));
                }
                None => out.push_str("null"),
            }
        }
        None => out.push_str("null,\"master_peak_dbfs\":null,\"peak_warning\":null"),
    }
    // Sprint 6 T006: every composition name plus the render root, so
    // the default render target is reviewable machine-readably.
    out.push_str(",\"compositions\":[");
    for (i, c) in verdict.compositions.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\"", json_escape(&c.name));
    }
    out.push_str("],\"root\":");
    match &verdict.root_name {
        Some(r) => {
            let _ = write!(out, "\"{}\"", json_escape(r));
        }
        None => out.push_str("null"),
    }
    out.push('}');
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
/// 2 (valid but unsupported). Never writes an MP4; it shells to FFmpeg
/// only to measure `detect-silence` pauses, and a project without
/// FFmpeg still validates (pauses report nothing instead of blocking).
/// With `json`, prints the machine-readable report instead of human text.
/// Every validation run appends its notable events to the run log (T015).
/// The optional `--backend` flag is accepted for render-command parity
/// but never changes the verdict (T001): the same folder is valid or
/// invalid under every backend. An unknown backend value is a validation
/// error (exit 1).
pub fn cmd_check(folder: &Path, json: bool, backend: Option<&str>) -> i32 {
    if let Some(raw) = backend {
        if render::render::Backend::parse(raw).is_none() {
            let msg =
                format!("framesmith check: unknown --backend {raw:?}: expected cpu, gpu, or auto.");
            eprintln!("{msg}");
            return 1;
        }
    }
    let run = log::run_id();
    let mut verdict = validate_folder(folder);
    // Sprint 10 sound (CAP-009): silent normalization fails closed in
    // `check` too, before any peak is previewed.
    apply_normalize_silence_gate(folder, &mut verdict);
    // Sprint 10 sound (CAP-011): a valid project additionally reports
    // its preview master peak (and warns when hot); anything else —
    // no FFmpeg, an undecodable source, an invalid project — reports
    // no peak and the verdict stands unchanged.
    let master = if verdict.status == Status::Valid {
        preview_master_peak(folder, &verdict)
    } else {
        None
    };
    if json {
        println!("{}", format_check_json(folder, &verdict, master));
    } else {
        print!("{}", format_check_human(folder, &verdict, master));
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

/// One composition row as human text (Sprint 6 T010): its name, the
/// root mark when it is the default render target, and its clip count.
fn format_composition_row(c: &Composition, is_root: bool) -> String {
    if is_root {
        format!("{:?} (root, {} clip(s))", c.name, c.clips.len())
    } else {
        format!("{:?} ({} clip(s))", c.name, c.clips.len())
    }
}

/// Human-readable `compositions` report (Sprint 6 T010): every
/// composition name in document order plus the root mark. Pure function
/// so tests can compare it against the `--json` form.
pub fn format_compositions_human(folder: &Path, verdict: &Verdict) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "compositions in {}:", folder.display());
    for c in &verdict.compositions {
        let is_root = Some(c.name.as_str()) == verdict.root_name.as_deref();
        let _ = writeln!(out, "  {}.", format_composition_row(c, is_root));
    }
    out
}

/// Machine-readable `compositions` report (Sprint 6 T010): the same
/// names, clip counts, and root as the human text. Pure function.
pub fn format_compositions_json(folder: &Path, verdict: &Verdict) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"compositions\",\"folder\":\"{}\",\"status\":\"valid\",\"exit\":0,\"compositions\":[",
        json_escape(&folder.display().to_string())
    );
    for (i, c) in verdict.compositions.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let is_root = Some(c.name.as_str()) == verdict.root_name.as_deref();
        let _ = write!(
            out,
            "{{\"name\":\"{}\",\"clips\":{},\"root\":{}}}",
            json_escape(&c.name),
            c.clips.len(),
            if is_root { "true" } else { "false" }
        );
    }
    out.push_str("],\"root\":");
    match &verdict.root_name {
        Some(r) => {
            let _ = write!(out, "\"{}\"", json_escape(r));
        }
        None => out.push_str("null"),
    }
    out.push('}');
    out
}

/// One timeline row as human text (Sprint 6 T010): the clip kind, id,
/// track, label, hidden state, and effective window — the same window
/// `render` draws, since both read the resolved verdict composition.
fn format_timeline_row(index: usize, clip: &notation::model::Clip) -> String {
    let id = match clip.id() {
        Some(id) => format!("{id:?}"),
        None => "-".to_string(),
    };
    let label = match clip.label() {
        Some(label) => format!("{label:?}"),
        None => "-".to_string(),
    };
    let visibility = if clip.hidden() { "hidden" } else { "visible" };
    format!(
        "  [{index}] {} id={id} track={} label={label} {visibility} window {:.3}s..{:.3}s",
        clip.kind(),
        clip.track().name(),
        clip.start(),
        clip.start() + clip.duration()
    )
}

/// Human-readable `timeline` report (Sprint 6 T010): one row per clip
/// in document order with id, track, label, hidden state, and the
/// effective window. Pure function for testability.
pub fn format_timeline_human(folder: &Path, comp: &Composition) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "timeline of composition {:?} in {} ({} clip(s), {}x{}, {:.3}s, {}fps):",
        comp.name,
        folder.display(),
        comp.clips.len(),
        comp.width,
        comp.height,
        comp.duration,
        comp.fps
    );
    if comp.clips.is_empty() {
        let _ = writeln!(out, "  (no clips)");
    }
    for (i, clip) in comp.clips.iter().enumerate() {
        let _ = writeln!(out, "{}.", format_timeline_row(i, clip));
    }
    out
}

/// Machine-readable `timeline` report (Sprint 6 T010): the same rows as
/// the human text, one object per clip. Pure function.
pub fn format_timeline_json(folder: &Path, verdict: &Verdict, comp: &Composition) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"timeline\",\"folder\":\"{}\",\"status\":\"valid\",\"exit\":0,\"composition\":\"{}\",\"root\":",
        json_escape(&folder.display().to_string()),
        json_escape(&comp.name)
    );
    match &verdict.root_name {
        Some(r) => {
            let _ = write!(out, "\"{}\"", json_escape(r));
        }
        None => out.push_str("null"),
    }
    let _ = write!(
        out,
        ",\"canvas\":{{\"width\":{},\"height\":{},\"preset\":",
        comp.width, comp.height
    );
    match &comp.canvas {
        Some(p) => {
            let _ = write!(out, "\"{}\"", json_escape(p));
        }
        None => out.push_str("null"),
    }
    let _ = write!(
        out,
        "}},\"duration\":{},\"fps\":\"{}\",\"frames\":{},\"clips\":[",
        comp.duration,
        comp.fps,
        comp.fps.frame_count(comp.duration)
    );
    for (i, clip) in comp.clips.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let id = match clip.id() {
            Some(id) => format!("\"{}\"", json_escape(id)),
            None => "null".to_string(),
        };
        let label = match clip.label() {
            Some(label) => format!("\"{}\"", json_escape(label)),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"index\":{i},\"kind\":\"{}\",\"id\":{id},\"track\":\"{}\",\"label\":{label},\"hidden\":{},\"start\":{},\"end\":{}}}",
            clip.kind(),
            clip.track().name(),
            if clip.hidden() { "true" } else { "false" },
            clip.start(),
            clip.start() + clip.duration()
        );
    }
    out.push_str("]}");
    out
}

/// Per-kind clip counts of one composition, in element order
/// (text, video, box, image, audio, nest).
fn count_clips_by_kind(comp: &Composition) -> [usize; 6] {
    let mut counts = [0usize; 6];
    for clip in &comp.clips {
        let at = match clip.kind() {
            "text" => 0,
            "video" => 1,
            "box" => 2,
            "image" => 3,
            "audio" => 4,
            _ => 5,
        };
        counts[at] += 1;
    }
    counts
}

/// Human-readable `info` report (Sprint 6 T010): the canvas, duration,
/// and clip counts of one composition. Pure function for testability.
pub fn format_info_human(folder: &Path, comp: &Composition) -> String {
    use std::fmt::Write;
    const KINDS: [&str; 6] = ["text", "video", "box", "image", "audio", "nest"];
    let mut out = String::new();
    let _ = writeln!(
        out,
        "info for composition {:?} in {}:",
        comp.name,
        folder.display()
    );
    match &comp.canvas {
        Some(preset) => {
            let _ = writeln!(
                out,
                "  canvas: {}x{} (preset {preset:?}).",
                comp.width, comp.height
            );
        }
        None => {
            let _ = writeln!(out, "  canvas: {}x{}.", comp.width, comp.height);
        }
    }
    let _ = writeln!(
        out,
        "  duration: {:.3}s ({} frames at {}fps).",
        comp.duration,
        comp.fps.frame_count(comp.duration),
        comp.fps
    );
    let counts = count_clips_by_kind(comp);
    let hidden = comp.clips.iter().filter(|c| c.hidden()).count();
    let kinds: Vec<String> = KINDS
        .iter()
        .zip(counts.iter())
        .map(|(k, n)| format!("{k}={n}"))
        .collect();
    let _ = writeln!(
        out,
        "  clips: {} total ({}; hidden={}).",
        comp.clips.len(),
        kinds.join(", "),
        hidden
    );
    // Sprint 10 sound (CAP-007): the playback hint rides along with no
    // render effect, so preview tooling can read it here.
    let _ = writeln!(out, "  autoplay: {}.", comp.autoplay);
    out
}

/// Machine-readable `info` report (Sprint 6 T010): the same canvas,
/// duration, and clip counts as the human text. Pure function.
pub fn format_info_json(folder: &Path, comp: &Composition) -> String {
    use std::fmt::Write;
    const KINDS: [&str; 6] = ["text", "video", "box", "image", "audio", "nest"];
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"info\",\"folder\":\"{}\",\"status\":\"valid\",\"exit\":0,\"composition\":\"{}\",\"canvas\":{{\"width\":{},\"height\":{},\"preset\":",
        json_escape(&folder.display().to_string()),
        json_escape(&comp.name),
        comp.width,
        comp.height
    );
    match &comp.canvas {
        Some(p) => {
            let _ = write!(out, "\"{}\"", json_escape(p));
        }
        None => out.push_str("null"),
    }
    let counts = count_clips_by_kind(comp);
    let hidden = comp.clips.iter().filter(|c| c.hidden()).count();
    // Sprint 10 sound (CAP-007): the playback hint rides at the
    // composition level, matching the human line below; `clips`
    // keeps its exact historical shape and tail.
    let _ = write!(
        out,
        "}},\"duration\":{},\"fps\":\"{}\",\"frames\":{},\"autoplay\":{},\"clips\":{{\"total\":{},\"by_kind\":{{",
        comp.duration,
        comp.fps,
        comp.fps.frame_count(comp.duration),
        if comp.autoplay { "true" } else { "false" },
        comp.clips.len()
    );
    for (i, (k, n)) in KINDS.iter().zip(counts.iter()).enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{k}\":{n}");
    }
    let _ = write!(out, "}},\"hidden\":{hidden}");
    out.push_str("}}");
    out
}

/// Refuse a listing exactly like `check` (Sprint 6 T010 §9): the same
/// diagnostics in the requested form, the same exit code (1 invalid,
/// 2 unsupported), and no listing output.
fn refuse_listing(cmd: &str, folder: &Path, verdict: &Verdict, json: bool) -> i32 {
    let run = log::run_id();
    if json {
        println!("{}", format_verdict_json(cmd, folder, verdict, None));
    } else {
        print!("{}", format_check_human(folder, verdict, None));
    }
    let code = match verdict.status {
        Status::Valid => 0,
        Status::Invalid => 1,
        Status::Unsupported => 2,
    };
    let (level, what) = match verdict.status {
        Status::Valid => ("info", format!("{cmd} valid")),
        Status::Invalid => ("warn", format!("{cmd} invalid")),
        Status::Unsupported => ("warn", format!("{cmd} unsupported")),
    };
    note_log(
        folder,
        "info",
        cmd,
        &run,
        &format!("{cmd} start folder={}", folder.display()),
        json,
    );
    note_log(
        folder,
        level,
        cmd,
        &run,
        &format!(
            "{what}: {} diagnostic(s), exit {code}",
            verdict.diagnostics.len()
        ),
        json,
    );
    code
}

/// Pick the composition one listing names (Sprint 6 T010): the root by
/// default, or the `--composition` name (a composition `name` or `id`).
/// An unknown name refuses with the available names; the caller lists
/// nothing on refusal. Only call with a valid verdict — invalid
/// projects refuse through [`refuse_listing`] before selection.
fn select_listing(cmd: &str, verdict: &Verdict, name: Option<&str>) -> Result<Composition, String> {
    match name {
        None => Ok(verdict
            .composition
            .clone()
            .expect("valid verdict carries a composition")),
        Some(want) => verdict
            .compositions
            .iter()
            .find(|c| c.name == want || c.id.as_deref() == Some(want))
            .cloned()
            .ok_or_else(|| {
                let mut available: Vec<String> =
                    verdict.compositions.iter().map(|c| c.name.clone()).collect();
                available.sort();
                let list = available
                    .iter()
                    .map(|n| format!("{n:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{cmd} refused: unknown composition {want:?}: the project holds {list}; pass one of those names (validation stage); nothing was listed"
                )
            }),
    }
}

/// `compositions`: list every composition name plus the root, in human
/// text or `--json` with identical content. Invalid projects refuse
/// like `check`. Returns the process exit code.
pub fn cmd_compositions(folder: &Path, json: bool) -> i32 {
    let run = log::run_id();
    let verdict = validate_folder(folder);
    if verdict.status != Status::Valid {
        return refuse_listing("compositions", folder, &verdict, json);
    }
    if json {
        println!("{}", format_compositions_json(folder, &verdict));
    } else {
        print!("{}", format_compositions_human(folder, &verdict));
    }
    note_log(
        folder,
        "info",
        "compositions",
        &run,
        &format!(
            "compositions: {} composition(s), root={:?}",
            verdict.compositions.len(),
            verdict.root_name.as_deref().unwrap_or("-")
        ),
        json,
    );
    0
}

/// `timeline`: list every clip of one composition with id, track,
/// label, hidden state, and effective window — the windows `render`
/// draws. Names the root composition by default. Invalid projects
/// refuse like `check`; an unknown name refuses with no listing.
/// Returns the process exit code.
pub fn cmd_timeline(folder: &Path, name: Option<&str>, json: bool) -> i32 {
    let run = log::run_id();
    let verdict = validate_folder(folder);
    if verdict.status != Status::Valid {
        return refuse_listing("timeline", folder, &verdict, json);
    }
    let comp = match select_listing("timeline", &verdict, name) {
        Ok(c) => c,
        Err(msg) => {
            if json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"timeline\",\"folder\":\"{}\",\"status\":\"invalid\",\"exit\":1,\"message\":\"{}\"}}",
                    json_escape(&folder.display().to_string()),
                    json_escape(&msg)
                );
            } else {
                println!("{msg}");
            }
            note_log(folder, "warn", "timeline", &run, &msg, json);
            return 1;
        }
    };
    if json {
        println!("{}", format_timeline_json(folder, &verdict, &comp));
    } else {
        print!("{}", format_timeline_human(folder, &comp));
    }
    note_log(
        folder,
        "info",
        "timeline",
        &run,
        &format!(
            "timeline: composition {:?}, {} clip(s)",
            comp.name,
            comp.clips.len()
        ),
        json,
    );
    0
}

/// `info`: summarize one composition's canvas, duration, and clip
/// counts. Names the root composition by default. Invalid projects
/// refuse like `check`; an unknown name refuses with no listing.
/// Returns the process exit code.
pub fn cmd_info(folder: &Path, name: Option<&str>, json: bool) -> i32 {
    let run = log::run_id();
    let verdict = validate_folder(folder);
    if verdict.status != Status::Valid {
        return refuse_listing("info", folder, &verdict, json);
    }
    let comp = match select_listing("info", &verdict, name) {
        Ok(c) => c,
        Err(msg) => {
            if json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"info\",\"folder\":\"{}\",\"status\":\"invalid\",\"exit\":1,\"message\":\"{}\"}}",
                    json_escape(&folder.display().to_string()),
                    json_escape(&msg)
                );
            } else {
                println!("{msg}");
            }
            note_log(folder, "warn", "info", &run, &msg, json);
            return 1;
        }
    };
    if json {
        println!("{}", format_info_json(folder, &comp));
    } else {
        print!("{}", format_info_human(folder, &comp));
    }
    note_log(
        folder,
        "info",
        "info",
        &run,
        &format!(
            "info: composition {:?}, {} clip(s)",
            comp.name,
            comp.clips.len()
        ),
        json,
    );
    0
}

/// Flags for one `waveform` run (Sprint 10 sound, CAP-008): the
/// composition to measure (resolved name, like `timeline`/`info`) plus
/// the raw `--buckets` flag text, validated before the first decode.
pub struct WaveformRequest {
    pub composition: Option<String>,
    pub buckets: Option<String>,
    pub json: bool,
}

/// Default `waveform` bucket count (Sprint 10 sound, CAP-008):
/// sixteen buckets shape a short mix readably; `--buckets N` names
/// any other positive count.
pub const WAVEFORM_DEFAULT_BUCKETS: usize = 16;

/// Restate an audio-stage refusal for `waveform` (Sprint 10 sound,
/// CAP-008): the cause and the file stay the render's exact words,
/// while the command name and the no-output tail name this listing —
/// there is no MP4 to leave behind, only peaks never listed.
fn waveform_error(e: &str) -> String {
    let restated = if let Some(rest) = e.strip_prefix("render refused: ") {
        format!("waveform refused: {rest}")
    } else if let Some(rest) = e.strip_prefix("render failed: ") {
        format!("waveform failed: {rest}")
    } else {
        format!("waveform refused: {e}")
    };
    restated
        .replace("no MP4 was written", "nothing was listed")
        .replace("No MP4 was written", "Nothing was listed")
}

/// `waveform`: report per-bucket peaks of one composition's mix plus
/// its master peak, in human text or `--json` with identical numbers.
/// Names the render root by default; invalid projects refuse like
/// `check`; bucket counts below 1 fail; a missing FFmpeg or an
/// undecodable source refuses with the cause named and no listing.
/// A hot master warns naming the peak, without changing the exit
/// code. Returns the process exit code.
pub fn cmd_waveform(folder: &Path, req: &WaveformRequest) -> i32 {
    use std::fmt::Write;
    let run = log::run_id();
    // Bucket counts resolve before validation: below 1 (or not a
    // positive whole number at all) refuses before the first decode
    // with the accepted form named and nothing listed.
    let buckets = match &req.buckets {
        None => WAVEFORM_DEFAULT_BUCKETS,
        Some(raw) => match raw.trim().parse::<usize>().ok().filter(|&n| n >= 1) {
            Some(n) => n,
            None => {
                let why = match raw.trim().parse::<i64>() {
                    Ok(n) => format!("--buckets {n} is below 1"),
                    Err(_) => format!("--buckets {raw:?} is not a positive whole number"),
                };
                let msg = format!(
                    "waveform refused: {why}: pass --buckets=<buckets of 1 or more> (for example --buckets=16), or omit the flag (validation stage); nothing was listed"
                );
                if req.json {
                    println!(
                        "{{\"tool\":\"framesmith\",\"command\":\"waveform\",\"folder\":\"{}\",\"status\":\"invalid\",\"exit\":1,\"message\":\"{}\"}}",
                        json_escape(&folder.display().to_string()),
                        json_escape(&msg)
                    );
                } else {
                    println!("{msg}");
                }
                note_log(folder, "warn", "waveform", &run, &msg, req.json);
                return 1;
            }
        },
    };
    let verdict = validate_folder(folder);
    if verdict.status != Status::Valid {
        return refuse_listing("waveform", folder, &verdict, req.json);
    }
    let comp = match select_listing("waveform", &verdict, req.composition.as_deref()) {
        Ok(c) => c,
        Err(msg) => {
            if req.json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"waveform\",\"folder\":\"{}\",\"status\":\"invalid\",\"exit\":1,\"message\":\"{}\"}}",
                    json_escape(&folder.display().to_string()),
                    json_escape(&msg)
                );
            } else {
                println!("{msg}");
            }
            note_log(folder, "warn", "waveform", &run, &msg, req.json);
            return 1;
        }
    };
    // Peak analysis decodes like the render but never downloads: no
    // FFmpeg on PATH, or an undecodable source, refuses with the cause
    // named and no listing.
    let fail = |msg: &str| -> i32 {
        if req.json {
            println!(
                "{{\"tool\":\"framesmith\",\"command\":\"waveform\",\"folder\":\"{}\",\"status\":\"invalid\",\"exit\":1,\"message\":\"{}\"}}",
                json_escape(&folder.display().to_string()),
                json_escape(msg)
            );
        } else {
            println!("{msg}");
        }
        note_log(folder, "warn", "waveform", &run, msg, req.json);
        1
    };
    let Some(ffmpeg_bin) = tool_on_path("ffmpeg") else {
        return fail("waveform refused: FFmpeg was not found on PATH: install FFmpeg to list waveform peaks (nothing was listed)");
    };
    let mix = match render::render::build_mix_preview(
        &comp,
        &verdict.compositions,
        folder,
        &ffmpeg_bin,
    ) {
        Ok(m) => m,
        Err(e) => return fail(&waveform_error(e.message())),
    };
    let frames = mix.as_ref().map(|m| m.len() / 2).unwrap_or(0);
    let peaks = match &mix {
        Some(m) => render::peaks::bucket_peaks(m, buckets),
        None => vec![0; buckets],
    };
    let master = mix.as_ref().map(|m| render::peaks::peak_of(m)).unwrap_or(0);
    if req.json {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"tool\":\"framesmith\",\"command\":\"waveform\",\"folder\":\"{}\",\"status\":\"valid\",\"exit\":0,\"composition\":\"{}\",\"buckets\":[",
            json_escape(&folder.display().to_string()),
            json_escape(&comp.name),
        );
        for (i, p) in peaks.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let (s, e) = render::peaks::bucket_window(frames, buckets, i);
            let db = match render::peaks::dbfs_of(*p) {
                Some(db) => db.to_string(),
                None => "null".to_string(),
            };
            let _ = write!(
                out,
                "{{\"start\":{s},\"end\":{e},\"peak\":{},\"peak_dbfs\":{db}}}",
                render::peaks::linear_of(*p),
            );
        }
        let master_db = match render::peaks::dbfs_of(master) {
            Some(db) => db.to_string(),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "],\"master_peak\":{},\"master_peak_dbfs\":{},\"peak_warning\":",
            render::peaks::linear_of(master),
            master_db
        );
        match hot_master_warning(master) {
            Some(w) => {
                let _ = write!(out, "\"{}\"", json_escape(&w));
            }
            None => out.push_str("null"),
        }
        out.push('}');
        println!("{out}");
    } else {
        println!(
            "waveform for composition {:?} in {}: {buckets} bucket(s) over {frames} frame(s) ({:.3}s).",
            comp.name,
            folder.display(),
            frames as f64 / render::peaks::PEAK_RATE
        );
        for (i, p) in peaks.iter().enumerate() {
            let (s, e) = render::peaks::bucket_window(frames, buckets, i);
            println!(
                "bucket {}/{buckets}: {s:.3}s..{e:.3}s peak {}.",
                i + 1,
                render::peaks::format_dbfs(*p)
            );
        }
        println!("master peak {}.", render::peaks::format_dbfs(master));
        if let Some(w) = hot_master_warning(master) {
            println!("{w}");
        }
    }
    note_log(
        folder,
        "info",
        "waveform",
        &run,
        &format!(
            "waveform: composition {:?}, {buckets} bucket(s), {} frame(s)",
            comp.name, frames
        ),
        req.json,
    );
    0
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
    /// Named composition to render (Sprint 6 T006): a composition
    /// `name` (or `id`). None renders the root.
    pub composition: Option<String>,
    /// Sub-range start in composition seconds (Sprint 6 T008): the raw
    /// `--start` flag text, parsed and validated after composition
    /// selection. None renders from zero.
    pub start: Option<String>,
    /// Sub-range end in composition seconds (Sprint 6 T008): the raw
    /// `--end` flag text. None renders to the composition end.
    pub end: Option<String>,
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

/// One reported section peak (Sprint 10 sound, CAP-008): the label and
/// window of an audible section plus the final-mix peak inside it.
/// Windows read in the staged mix: for a sub-range render they shift
/// by the range start, so the report always covers the MP4 written.
struct PeakSection {
    label: String,
    start: f64,
    end: f64,
    peak: u32,
}

/// Render-summary peak report (Sprint 10 sound, CAP-008/CAP-011): the
/// master peak, one peak per audible section window, and the loudest
/// silent section (the `check` pause maps measured on this mix).
struct PeakSummary {
    master: u32,
    sections: Vec<PeakSection>,
    loudest_silence: Option<(f64, f64, u32)>,
}

/// Measure one render-summary peak report (Sprint 10 sound,
/// CAP-008/CAP-011): the master peak over the whole staged mix, one
/// peak per audible section window, and the loudest `check` pause
/// range on this mix. `start_offset` shifts section windows for
/// ranged renders (zero renders the whole composition). Pure sample
/// math, so the summary names the same peaks a re-render measures.
fn summarize_peaks(
    mix: &[i16],
    comp: &Composition,
    silences: &[validate_mod::SilenceReport],
    start_offset: f64,
) -> PeakSummary {
    use render::peaks;
    let master = peaks::peak_of(mix);
    let mut sections = Vec::new();
    for s in peaks::mix_sections(comp) {
        let (start, end) = (s.start - start_offset, s.end - start_offset);
        if !end.is_finite() || end <= 0.0 {
            continue;
        }
        let (start, end) = (start.max(0.0), end);
        sections.push(PeakSection {
            label: s.label(),
            start,
            end,
            peak: peaks::peak_in_seconds(mix, start, end),
        });
    }
    let mut loudest: Option<(f64, f64, u32)> = None;
    for report in silences {
        for (s, e) in &report.ranges {
            let (start, end) = ((s - start_offset).max(0.0), e - start_offset);
            if !end.is_finite() || end <= 0.0 {
                continue;
            }
            let peak = peaks::peak_in_seconds(mix, start, end);
            let louder = match loudest {
                Some((_, _, p)) => peak > p,
                None => true,
            };
            if louder {
                loudest = Some((start, end, peak));
            }
        }
    }
    PeakSummary {
        master,
        sections,
        loudest_silence: loudest,
    }
}

/// One summary peak line for the human report (Sprint 10 sound,
/// CAP-008/CAP-011): the master, one line per section, and the
/// loudest silent section (or its absence). The hot-master warning
/// follows the same wording as `check`.
fn format_peak_human(summary: &PeakSummary) -> Vec<String> {
    use render::peaks;
    let mut lines = Vec::new();
    lines.push(format!(
        "stage audio: note: master peak {}.",
        peaks::format_dbfs(summary.master)
    ));
    for s in &summary.sections {
        lines.push(format!(
            "stage audio: note: section {} {:.3}s..{:.3}s peak {}.",
            s.label,
            s.start,
            s.end,
            peaks::format_dbfs(s.peak)
        ));
    }
    match summary.loudest_silence {
        Some((s, e, p)) => lines.push(format!(
            "stage audio: note: loudest silent section {s:.3}s..{e:.3}s peak {}.",
            peaks::format_dbfs(p)
        )),
        None => lines.push("stage audio: note: no silent sections.".to_string()),
    }
    if let Some(w) = hot_master_warning(summary.master) {
        lines.push(format!("stage audio: {w}"));
    }
    lines
}

/// Machine-readable summary peaks (Sprint 10 sound, CAP-008/CAP-011):
/// the same master, sections, loudest silence, and hot warning the
/// human lines carry. Silent levels report `"peak_dbfs":null` with
/// their linear `0.0`, since JSON carries no infinity.
fn format_peaks_json(summary: &PeakSummary) -> String {
    use render::peaks;
    use std::fmt::Write;
    let mut out = String::new();
    let db = |p: u32| match peaks::dbfs_of(p) {
        Some(db) => db.to_string(),
        None => "null".to_string(),
    };
    let _ = write!(
        out,
        "{{\"master_peak\":{},\"master_peak_dbfs\":{},\"peak_warning\":",
        peaks::linear_of(summary.master),
        db(summary.master)
    );
    match hot_master_warning(summary.master) {
        Some(w) => {
            let _ = write!(out, "\"{}\"", json_escape(&w));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"sections\":[");
    for (i, s) in summary.sections.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"label\":\"{}\",\"start\":{},\"end\":{},\"peak\":{},\"peak_dbfs\":{}}}",
            json_escape(&s.label),
            s.start,
            s.end,
            peaks::linear_of(s.peak),
            db(s.peak)
        );
    }
    out.push_str("],\"loudest_silence\":");
    match summary.loudest_silence {
        Some((s, e, p)) => {
            let _ = write!(
                out,
                "{{\"start\":{s},\"end\":{e},\"peak\":{},\"peak_dbfs\":{}}}",
                peaks::linear_of(p),
                db(p)
            );
        }
        None => out.push_str("null"),
    }
    out.push('}');
    out
}

/// One loudness report as human lines (Sprint 10 sound, CAP-009): the
/// measured integrated loudness, the applied overall gain, and the
/// true-peak facts the guard used.
fn format_normalize_human(report: &render::loudness::NormalizeReport) -> Vec<String> {
    let dbtp = 20.0 * report.true_peak_after.log10();
    let mut line = format!(
        "stage audio: note: normalize: loud: measured {:.1} LUFS, applied gain {:+.1} dB ({:.4}x), true peak {:.1} dBTP",
        report.measured_lufs, report.gain_db, report.gain, dbtp,
    );
    if report.guarded {
        line.push_str(" (guarded to the -1.0 dBTP ceiling).");
    } else {
        line.push('.');
    }
    vec![line]
}

/// Machine-readable loudness report (Sprint 10 sound, CAP-009): the
/// same target, measurement, gain, and guard the human line carries.
/// Levels are finite by construction (silent mixes fail closed before
/// any report), so plain numbers serialize directly.
fn format_normalize_json(report: &render::loudness::NormalizeReport) -> String {
    format!(
        "{{\"target\":\"loud\",\"measured_lufs\":{},\"gain_db\":{},\"gain\":{},\"true_peak_dbtp\":{},\"guarded\":{}}}",
        report.measured_lufs,
        report.gain_db,
        report.gain,
        20.0 * report.true_peak_after.log10(),
        if report.guarded { "true" } else { "false" },
    )
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
    /// Preformatted summary peaks object (Sprint 10 sound, CAP-008 /
    /// CAP-011): `Some` on runs that reached the audio stage, `None`
    /// on every earlier refusal (renders as `"peaks":null`).
    peaks: Option<String>,
    /// Preformatted loudness report (Sprint 10 sound, CAP-009): `Some`
    /// on normalizing runs that reached the audio stage, `None`
    /// otherwise (renders as `"normalize":null`).
    normalize: Option<String>,
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
    let _ = write!(
        out,
        ",\"cache\":{{\"hits\":{cache_hits},\"misses\":{cache_misses}}}"
    );
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
    out.push_str("],\"peaks\":");
    match &ctx.peaks {
        Some(p) => out.push_str(p),
        None => out.push_str("null"),
    }
    out.push_str(",\"normalize\":");
    match &ctx.normalize {
        Some(n) => out.push_str(n),
        None => out.push_str("null"),
    }
    let _ = write!(out, ",\"timing_ms\":{{\"total\":{total_ms}}}}}");
    out
}

/// Parse one raw `--start`/`--end` flag value (Sprint 6 T008): absent
/// means the composition edge (0 for `--start`, the duration for
/// `--end`); present must be a finite number of seconds. Anything else
/// refuses with the flag named, before the first frame.
fn parse_range_bound(raw: Option<&str>, flag: &str) -> Result<Option<f64>, String> {
    match raw {
        None => Ok(None),
        Some(text) => match text.trim().parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(Some(v)),
            _ => Err(format!(
                "render refused: {flag} {text:?} is not a number of seconds: pass {flag}=<seconds> (for example {flag}=1.5), or omit the flag (validation stage); no MP4 was written"
            )),
        },
    }
}

/// Pick the composition one `render` run encodes (Sprint 6 T006):
/// the root by default, or the `--composition` name (a composition
/// `name` or `id`). An unknown name refuses with the available names;
/// the caller writes no output on refusal. Only call with a valid or
/// supported verdict (invalid projects refuse before selection).
fn select_composition(verdict: &Verdict, name: Option<&str>) -> Result<Composition, String> {
    match name {
        None => Ok(verdict
            .composition
            .clone()
            .expect("valid verdict carries a composition")),
        Some(want) => verdict
            .compositions
            .iter()
            .find(|c| c.name == want || c.id.as_deref() == Some(want))
            .cloned()
            .ok_or_else(|| {
                let mut available: Vec<String> =
                    verdict.compositions.iter().map(|c| c.name.clone()).collect();
                available.sort();
                let list = available
                    .iter()
                    .map(|n| format!("{n:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "render refused: unknown composition {want:?}: the project holds {list}; pass one of those names to `--composition` (validation stage); no MP4 was written"
                )
            }),
    }
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
                    format_render_json(&RenderJson {
                        folder,
                        status: "invalid",
                        exit: 1,
                        backend: "unknown",
                        preset: "unknown",
                        enc: &enc,
                        output: "",
                        message: &e,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &[],
                        total_ms: 0
                    })
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
        &format!("{vwhat}: {} diagnostic(s)", verdict.diagnostics.len()),
        json,
    );
    match verdict.status {
        Status::Invalid => {
            let msg = format!(
                "render refused: {} is invalid (validation stage); fix the problems above and re-run. No MP4 was written.",
                folder.display()
            );
            push_stage(
                &mut stages,
                render::render::StageTiming::failed(
                    "validate",
                    validate_ms,
                    format!("{} problem(s)", verdict.diagnostics.len()),
                ),
            );
            stage_line(
                json,
                &format!("stage validate: failed ({validate_ms}ms): {msg}"),
            );
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "warn", "render", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson {
                        folder,
                        status: "invalid",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &opts.encoder(),
                        output: "",
                        message: &msg,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &verdict.diagnostics,
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
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
            push_stage(
                &mut stages,
                render::render::StageTiming::failed(
                    "validate",
                    validate_ms,
                    format!("{} later-sprint use(s)", verdict.diagnostics.len()),
                ),
            );
            stage_line(
                json,
                &format!("stage validate: failed ({validate_ms}ms): {msg}"),
            );
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "warn", "render", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson {
                        folder,
                        status: "unsupported",
                        exit: 2,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &opts.encoder(),
                        output: "",
                        message: &msg,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &verdict.diagnostics,
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
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
            push_stage(
                &mut stages,
                render::render::StageTiming::ok(
                    "validate",
                    validate_ms,
                    format!("{} diagnostic(s)", verdict.diagnostics.len()),
                ),
            );
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
                format_render_json(&RenderJson {
                    folder,
                    status: "invalid",
                    exit: 1,
                    backend: "gpu",
                    preset: opts.preset.name(),
                    enc: &opts.encoder(),
                    output: "",
                    message: &msg,
                    cache_hits: 0,
                    cache_misses: 0,
                    peaks: None,
                    normalize: None,
                    diagnostics: &[],
                    stages: &stages,
                    total_ms: total_start.elapsed().as_millis() as u64
                })
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
        if f.trim_start != 0.0
            || f.trim_end.is_some()
            || f.rate != 1.0
            || f.loop_clip
            || f.clamped
            || f.inferred
        {
            for line in format_footage_human(f).lines() {
                let note = format!("stage validate: note: {line}");
                stage_line(json, &note);
                note_log(folder, "info", "render", &run, &note, json);
            }
        }
    }
    // Sprint 6 T003: every other inferred length (audio, stills, the
    // composition) announces the same way, so the render summary shows
    // what `check` reviewed.
    for f in &verdict.inferred {
        for line in format_inferred_human(f).lines() {
            let note = format!("stage validate: note: {line}");
            stage_line(json, &note);
            note_log(folder, "info", "render", &run, &note, json);
        }
    }
    // Sprint 6 T006: the root renders by default; `--composition`
    // selects another composition by name (or `id`). An unknown name
    // refuses before the first frame with no MP4 written.
    let comp = match select_composition(&verdict, req.composition.as_deref()) {
        Ok(c) => c,
        Err(msg) => {
            stage_line(json, &msg);
            note_log(folder, "warn", "render", &run, &msg, json);
            if json {
                let enc = render::render::RenderOptions::default().encoder();
                println!(
                    "{}",
                    format_render_json(&RenderJson {
                        folder,
                        status: "invalid",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &enc,
                        output: "",
                        message: &msg,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
                );
            }
            return 1;
        }
    };
    // Sprint 6 T008: `--start`/`--end` renders exactly that
    // composition-time window as a standalone MP4 on the selected
    // composition. Empty, backward, and out-of-range windows refuse
    // here — before bootstrap, output preflight, and frame one — with
    // the composition length and the accepted window named, and no MP4
    // written. Two absent flags render the whole composition.
    let range = match (|| -> Result<Option<render::render::RenderRange>, String> {
        let s = parse_range_bound(req.start.as_deref(), "--start")?;
        let e = parse_range_bound(req.end.as_deref(), "--end")?;
        render::render::resolve_range(&comp.name, comp.duration, &comp.fps, s, e)
    })() {
        Ok(r) => r,
        Err(msg) => {
            stage_line(json, &msg);
            note_log(folder, "warn", "render", &run, &msg, json);
            if json {
                let enc = render::render::RenderOptions::default().encoder();
                println!(
                    "{}",
                    format_render_json(&RenderJson {
                        folder,
                        status: "invalid",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &enc,
                        output: "",
                        message: &msg,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
                );
            }
            return 1;
        }
    };
    if let Some(r) = range {
        let note = format!(
            "stage validate: note: rendering sub-range {:.3}s..{:.3}s of composition {:?} as its own MP4.",
            r.start, r.end, comp.name
        );
        stage_line(json, &note);
        note_log(folder, "info", "render", &run, &note, json);
    }
    if render::cancel::is_cancelled() {
        return cmd_cancelled(
            folder,
            &run,
            json,
            opts.backend.name(),
            opts.preset.name(),
            &opts.encoder(),
            &folder.join("out.mp4"),
            "validate",
            &mut stages,
            total_start,
            0,
            "",
        );
    }

    // Detection only reads host state; the folder is never modified (T008).
    let finding = ffmpeg_mod::detect();
    stage_line(json, &finding.to_string());
    note_log(
        folder,
        "info",
        "bootstrap",
        &run,
        &finding.to_string(),
        json,
    );
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
                    format_render_json(&RenderJson {
                        folder,
                        status: "failed",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &opts.encoder(),
                        output: "",
                        message: &message,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
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
                format_render_json(&RenderJson {
                    folder,
                    status: "failed",
                    exit: 1,
                    backend: opts.backend.name(),
                    preset: opts.preset.name(),
                    enc: &opts.encoder(),
                    output: "",
                    message: &msg,
                    cache_hits: 0,
                    cache_misses: 0,
                    peaks: None,
                    normalize: None,
                    diagnostics: &[],
                    stages: &stages,
                    total_ms: total_start.elapsed().as_millis() as u64
                })
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
                    format_render_json(&RenderJson {
                        folder,
                        status: "failed",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &opts.encoder(),
                        output: "",
                        message: &msg,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
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
    // Sprint 6 T008: a sub-range prepares exactly its own window (its
    // frame `k` is the range start plus `k` frame intervals); a full
    // render prepares the whole composition exactly as before.
    let prep = match render::render::prepare_frames_ranged(
        &comp,
        &verdict.compositions,
        folder,
        &ffmpeg_bin,
        range,
    ) {
        Ok(prep) => prep,
        Err(e) => {
            if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                let ms = frames_start.elapsed().as_millis() as u64;
                let detail = without_cancel_prefix(&e).unwrap_or("").to_string();
                return cmd_cancelled(
                    folder,
                    &run,
                    json,
                    opts.backend.name(),
                    opts.preset.name(),
                    &opts.encoder(),
                    &out,
                    "frames",
                    &mut stages,
                    total_start,
                    ms,
                    &detail,
                );
            }
            let ms = frames_start.elapsed().as_millis() as u64;
            push_stage(
                &mut stages,
                render::render::StageTiming::failed("frames", ms, "prepare failed".to_string()),
            );
            stage_line(json, &format!("stage frames: failed ({ms}ms): {e}"));
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "error", "render", &run, &e, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson {
                        folder,
                        status: "failed",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &opts.encoder(),
                        output: "",
                        message: &e,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
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
        return cmd_cancelled(
            folder,
            &run,
            json,
            opts.backend.name(),
            opts.preset.name(),
            &opts.encoder(),
            &out,
            "frames",
            &mut stages,
            total_start,
            ms,
            "",
        );
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
                return cmd_cancelled(
                    folder,
                    &run,
                    json,
                    opts.backend.name(),
                    opts.preset.name(),
                    &opts.encoder(),
                    &out,
                    "audio",
                    &mut stages,
                    total_start,
                    audio_ms,
                    &detail,
                );
            }
            let audio_ms = audio_start.elapsed().as_millis() as u64;
            let frames_ms = frames_start.elapsed().as_millis() as u64;
            push_stage(
                &mut stages,
                render::render::StageTiming::ok("frames", frames_ms, prepared_note.clone()),
            );
            push_stage(
                &mut stages,
                render::render::StageTiming::failed(
                    "audio",
                    audio_ms,
                    "mix build failed".to_string(),
                ),
            );
            stage_line(
                json,
                &format!("stage frames: ok ({frames_ms}ms): {prepared_note}"),
            );
            stage_line(json, &format!("stage audio: failed ({audio_ms}ms): {e}"));
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "error", "render", &run, &e, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson {
                        folder,
                        status: "failed",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &opts.encoder(),
                        output: "",
                        message: &e,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
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
        return cmd_cancelled(
            folder,
            &run,
            json,
            opts.backend.name(),
            opts.preset.name(),
            &opts.encoder(),
            &out,
            "audio",
            &mut stages,
            total_start,
            ms,
            "",
        );
    }
    // Sprint 6 T008: the staged mix spans the whole composition, so a
    // sub-range crops it to exactly its own window — sample for sample
    // the full render's mix sliced to `[start, end)`. A silent
    // composition stages no mix, so there is nothing to crop.
    if let (Some(r), Some(mix)) = (range, plan.mix_pcm.as_deref()) {
        if let Err(e) = render::render::crop_mix_to_range(mix, r.start, r.end) {
            let audio_ms = audio_start.elapsed().as_millis() as u64;
            let frames_ms = frames_start.elapsed().as_millis() as u64;
            push_stage(
                &mut stages,
                render::render::StageTiming::ok("frames", frames_ms, prepared_note.clone()),
            );
            push_stage(
                &mut stages,
                render::render::StageTiming::failed(
                    "audio",
                    audio_ms,
                    "range crop failed".to_string(),
                ),
            );
            stage_line(
                json,
                &format!("stage frames: ok ({frames_ms}ms): {prepared_note}"),
            );
            stage_line(json, &format!("stage audio: failed ({audio_ms}ms): {e}"));
            print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
            note_log(folder, "error", "render", &run, &e, json);
            if json {
                println!(
                    "{}",
                    format_render_json(&RenderJson {
                        folder,
                        status: "failed",
                        exit: 1,
                        backend: opts.backend.name(),
                        preset: opts.preset.name(),
                        enc: &opts.encoder(),
                        output: "",
                        message: &e,
                        cache_hits: 0,
                        cache_misses: 0,
                        peaks: None,
                        normalize: None,
                        diagnostics: &[],
                        stages: &stages,
                        total_ms: total_start.elapsed().as_millis() as u64
                    })
                );
            } else if e.starts_with("render refused:") || e.starts_with("render failed:") {
                println!("{e}");
            } else {
                println!("render failed: {e}");
            }
            // The staged mix must not survive a failed range crop.
            let _ = std::fs::remove_file(mix);
            return 1;
        }
    }
    // Sprint 10 sound (CAP-008/CAP-011): the summary peaks — master,
    // per-section, and loudest silent section — measured on the staged
    // mix the MP4 encodes (already cropped for sub-ranges, so the
    // report always covers the file written). A soundless composition
    // reports digital silence with no sections.
    let start_offset = range.map(|r| r.start).unwrap_or(0.0);
    let peak_mix: Option<Vec<i16>> = match &plan.mix_pcm {
        Some(mix_path) => match std::fs::read(mix_path) {
            Ok(bytes) => {
                let (pairs, _) = bytes.as_chunks::<2>();
                Some(pairs.iter().map(|b| i16::from_le_bytes(*b)).collect())
            }
            Err(e) => {
                let msg = format!(
                    "render failed: cannot read the staged audio mix {}: {e} (audio stage)",
                    mix_path.display()
                );
                let audio_ms = audio_start.elapsed().as_millis() as u64;
                let frames_ms = frames_start.elapsed().as_millis() as u64;
                push_stage(
                    &mut stages,
                    render::render::StageTiming::ok("frames", frames_ms, prepared_note.clone()),
                );
                push_stage(
                    &mut stages,
                    render::render::StageTiming::failed(
                        "audio",
                        audio_ms,
                        "mix read failed".to_string(),
                    ),
                );
                stage_line(
                    json,
                    &format!("stage frames: ok ({frames_ms}ms): {prepared_note}"),
                );
                stage_line(json, &format!("stage audio: failed ({audio_ms}ms): {msg}"));
                print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
                note_log(folder, "error", "render", &run, &msg, json);
                if json {
                    println!(
                        "{}",
                        format_render_json(&RenderJson {
                            folder,
                            status: "failed",
                            exit: 1,
                            backend: opts.backend.name(),
                            preset: opts.preset.name(),
                            enc: &opts.encoder(),
                            output: "",
                            message: &msg,
                            cache_hits: 0,
                            cache_misses: 0,
                            peaks: None,
                            normalize: None,
                            diagnostics: &[],
                            stages: &stages,
                            total_ms: total_start.elapsed().as_millis() as u64
                        })
                    );
                } else {
                    println!("{msg}");
                }
                let _ = std::fs::remove_file(mix_path);
                return 1;
            }
        },
        None => None,
    };
    let peak_summary = match &peak_mix {
        Some(mix) => summarize_peaks(mix, &comp, &verdict.silences, start_offset),
        None => PeakSummary {
            master: 0,
            sections: Vec::new(),
            loudest_silence: None,
        },
    };
    let peaks_json = format_peaks_json(&peak_summary);
    let normalize_json = plan.normalize.as_ref().map(format_normalize_json);
    let audio_ms = audio_start.elapsed().as_millis() as u64;
    let audio_note = match &plan.mix_pcm {
        Some(_) => "mix: audible sources mixed at 48 kHz stereo".to_string(),
        None => "mix: no audible sound; silent track".to_string(),
    };
    stage_line(
        json,
        &format!("stage audio: ok ({audio_ms}ms): {audio_note}"),
    );
    for line in format_peak_human(&peak_summary) {
        stage_line(json, &line);
        note_log(folder, "info", "render", &run, &line, json);
    }
    // Sprint 10 sound (CAP-009): a normalizing run reports the one
    // overall gain the mix encoded, in both forms.
    if let Some(report) = &plan.normalize {
        for line in format_normalize_human(report) {
            stage_line(json, &line);
            note_log(folder, "info", "render", &run, &line, json);
        }
    }
    push_stage(
        &mut stages,
        render::render::StageTiming::ok("audio", audio_ms, audio_note.clone()),
    );

    // Frames stage, second half, plus encode: draw in parallel in bounded
    // batches, re-emit in frame order, and rename the MP4 into place.
    let (done_msg, draw_ms, encode_ms) =
        match render::render::render_staged(&prep, &plan, &opts, &ffmpeg_bin, &out) {
            Ok(outcome) => outcome,
            Err(e) => {
                if render::cancel::is_cancel_error(&e) || render::cancel::is_cancelled() {
                    let frames_ms = frames_start.elapsed().as_millis() as u64;
                    let detail = without_cancel_prefix(&e).unwrap_or("").to_string();
                    let stage: &'static str = if e.contains("encode") {
                        "encode"
                    } else {
                        "frames"
                    };
                    return cmd_cancelled(
                        folder,
                        &run,
                        json,
                        opts.backend.name(),
                        opts.preset.name(),
                        &opts.encoder(),
                        &out,
                        stage,
                        &mut stages,
                        total_start,
                        frames_ms,
                        &detail,
                    );
                }
                let frames_ms = frames_start.elapsed().as_millis() as u64;
                push_stage(
                    &mut stages,
                    render::render::StageTiming::failed("frames", frames_ms, prepared_note.clone()),
                );
                stage_line(json, &format!("stage frames: failed ({frames_ms}ms): {e}"));
                print_timing_line(json, &stages, total_start.elapsed().as_millis() as u64);
                note_log(folder, "error", "render", &run, &e, json);
                if json {
                    println!(
                        "{}",
                        format_render_json(&RenderJson {
                            folder,
                            status: "failed",
                            exit: 1,
                            backend: opts.backend.name(),
                            preset: opts.preset.name(),
                            enc: &opts.encoder(),
                            output: "",
                            message: &e,
                            cache_hits: 0,
                            cache_misses: 0,
                            peaks: None,
                            normalize: None,
                            diagnostics: &[],
                            stages: &stages,
                            total_ms: total_start.elapsed().as_millis() as u64
                        })
                    );
                } else if e.starts_with("render refused:") || e.starts_with("render failed:") {
                    println!("{e}");
                } else {
                    println!("render failed: {e}");
                }
                return 1;
            }
        };
    // Sprint 6 T008: the success line names the sub-range, so both the
    // human progress line and the `--json` message say which window the
    // standalone MP4 holds. A full render reads exactly as before.
    let done_msg = match range {
        Some(r) => format!("{done_msg} [range {:.3}s..{:.3}s]", r.start, r.end),
        None => done_msg,
    };
    let frames_ms = frames_start.elapsed().as_millis() as u64;
    push_stage(
        &mut stages,
        render::render::StageTiming::ok(
            "frames",
            frames_ms,
            format!("{prepared_note}; drew {draw_ms}ms"),
        ),
    );
    push_stage(
        &mut stages,
        render::render::StageTiming::ok(
            "encode",
            encode_ms,
            "ffmpeg encode and atomic rename".to_string(),
        ),
    );
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
    push_stage(
        &mut stages,
        render::render::StageTiming::ok("done", total_ms, done_msg.clone()),
    );
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
            format_render_json(&RenderJson {
                folder,
                status: "valid",
                exit: 0,
                backend: opts.backend.name(),
                preset: opts.preset.name(),
                enc: &opts.encoder(),
                output: &out.display().to_string(),
                message: &done_msg,
                cache_hits,
                cache_misses,
                peaks: Some(peaks_json),
                normalize: normalize_json,
                diagnostics: &[],
                stages: &stages,
                total_ms
            })
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
        let names: Vec<String> = leftovers.iter().map(|p| p.display().to_string()).collect();
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
                peaks: None,
                normalize: None,
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
                let mut out =
                    String::from("{\"tool\":\"framesmith\",\"command\":\"errors\",\"codes\":[");
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
        assert_eq!(validate_folder(&root.join("proj")).status, Status::Valid);
    }

    #[test]
    fn init_rejects_bad_names() {
        let root = tmp_dir("names");
        let bad65 = "x".repeat(65);
        for bad in [
            "",
            "has space",
            "a/b",
            "..",
            ".",
            "-x!",
            "é",
            bad65.as_str(),
        ] {
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
        write(&folder, notation::model::CONFIG_FILE, "bogus_key = true\n");
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
        for want in [
            "E_BAD_BOX",
            "E_BAD_COLOR",
            "E_BAD_ORDER",
            "E_BAD_SPACING",
            "E_MISSING_FILE",
            "E_BAD_FIT",
            "E_BAD_IMAGE",
        ] {
            assert!(codes.contains(&want), "missing {want} in {codes:?}");
        }
        for d in &v.diagnostics {
            assert!(!d.file.is_empty() && d.line >= 1 && d.column >= 1, "{d:?}");
            assert!(!d.fix.is_empty(), "{d:?}");
        }
        let human = format_check_human(&folder, &v, None);
        let json = format_check_json(&folder, &v, None);
        for want in [
            "E_BAD_BOX",
            "E_BAD_ORDER",
            "E_BAD_SPACING",
            "E_MISSING_FILE",
        ] {
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
        assert!(v
            .diagnostics
            .iter()
            .any(|d| d.code == "E_UNSUPPORTED" && d.message.contains("photo.webp")));
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
            before
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
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
        let human = format_check_human(&folder, &v, None);
        let json = format_check_json(&folder, &v, None);
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
        let json = format_check_json(&folder, &v, None);
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
        let json = format_check_json(&folder, &v, None);
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
            let entry = catalog::lookup(d.code)
                .unwrap_or_else(|| panic!("{} has no catalog entry", d.code));
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
            assert_eq!(
                v.status,
                Status::Invalid,
                "size={size}: {:?}",
                v.diagnostics
            );
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
            assert_eq!(
                v.status,
                Status::Invalid,
                "wrap={wrap}: {:?}",
                v.diagnostics
            );
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
            v.diagnostics
                .iter()
                .any(|d| d.code == "E_BAD_EASING" && d.message.contains("in-out-sine")),
            "{:?}",
            v.diagnostics
        );
        let human = format_check_human(&folder, &v, None);
        let json = format_check_json(&folder, &v, None);
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
        // T003 plus Sprint 7 motion (T005/T006): well-formed use of
        // beyond-today properties exits 2 naming the property with its
        // enabling sprint. `blur` and `rotate-x` animate since T005/T006
        // (asserted valid below); `scale-x` and `typed` still wait for
        // Sprint 7, and `brightness` moved to the Sprint 11 filter set.
        // Sprint 7 motion (T007) enables the full easing curve set:
        // those easings validate cleanly and land in the composition.
        for prop in ["scale-x", "typed"] {
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
        // Beyond-blur filters wait for Sprint 11, even well-formed.
        for prop in ["brightness", "filter", "contrast"] {
            let folder = tmp_dir("laterfilter");
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
                    && d.message.contains("Sprint 11")),
                "{prop}: {:?}",
                v.diagnostics
            );
            assert_eq!(cmd_check(&folder, false, None), 2);
            assert_eq!(cmd_check(&folder, true, None), 2);
            assert_eq!(cmd_render(&folder, None), 2);
            assert!(!folder.join("out.mp4").exists());
        }
        // Sprint 7 motion properties validate clean and check green.
        for (prop, to) in [("blur", "2"), ("rotate-x", "45"), ("rotation-y", "30")] {
            let folder = tmp_dir("nowprop");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!("composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2\n    tween target=0 property=\"{prop}\" kind=\"to\" start=0 duration=1 to={to}\n}}\n"),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Valid, "{prop}: {:?}", v.diagnostics);
            assert!(v.diagnostics.is_empty(), "{prop}: {:?}", v.diagnostics);
            assert_eq!(cmd_check(&folder, false, None), 0);
        }
        for ease in [
            "spring",
            "spring(100,5)",
            "bounce",
            "elastic",
            "wiggle",
            "wiggle(2)",
            "hold",
            "steps(4)",
            "cubic-bezier(0.25,0.1,0.25,1)",
        ] {
            let folder = tmp_dir("laterease");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!("composition name=\"t\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n    text value=\"Hi\" start=0 duration=2\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"{ease}\"\n}}\n"),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Valid, "{ease}: {:?}", v.diagnostics);
            assert!(v.diagnostics.is_empty(), "{ease}: {:?}", v.diagnostics);
            assert_eq!(cmd_check(&folder, false, None), 0);
        }
    }

    #[test]
    fn rich_box_looks_wait_for_sprint_8() {
        // T004: per-side borders and border images are refused as
        // not-yet-supported with Sprint 8, never guessed.
        for key in ["border-top-width", "border-image"] {
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
        // T003: gradient and image fills arrived as child elements, so a
        // fill-shaped attribute is refused as invalid with the element to
        // write instead of not-yet-supported.
        for key in ["gradient", "fill-image"] {
            let folder = tmp_dir("richboxfill");
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!("composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {{\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" {key}=1 start=0 duration=1\n}}\n"),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Invalid, "{key}: {:?}", v.diagnostics);
            assert!(
                v.diagnostics.iter().any(|d| d.code == "E_BAD_FILL"
                    && d.message.contains("child elements")),
                "{key}: {:?}",
                v.diagnostics
            );
            assert_eq!(cmd_render(&folder, None), 1);
            assert!(!folder.join("out.mp4").exists());
        }
    }

    #[test]
    fn missing_files_are_located() {
        let folder = tmp_dir("missing");
        assert_eq!(
            validate_folder(&folder.join("nope")).status,
            Status::Invalid
        );
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
            assert!(
                catalog::lookup(d.code).is_some(),
                "{} has no catalog entry",
                d.code
            );
        }
        let human = format_check_human(&folder, &v, None);
        let json = format_check_json(&folder, &v, None);
        for want in [
            "E_MISSING_FILE",
            "E_BAD_AUDIO",
            "E_UNKNOWN_KEY",
            "E_UNSUPPORTED",
        ] {
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
            (
                "loud",
                "video path=\"clip.mp4\" start=0 duration=2 volume=12.5",
            ),
            (
                "fade",
                "video path=\"clip.mp4\" start=0 duration=2 fade-out=9",
            ),
            (
                "shape",
                "video path=\"clip.mp4\" start=0 duration=2 volume=\"loud\"",
            ),
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
            (
                "backward",
                "video path=\"clip.mp4\" start=0 duration=2 trim-start=3 trim-end=1",
            ),
            (
                "zero",
                "video path=\"clip.mp4\" start=0 duration=2 trim-start=2 trim-end=2",
            ),
            (
                "low",
                "video path=\"clip.mp4\" start=0 duration=2 rate=0.05",
            ),
            ("high", "video path=\"clip.mp4\" start=0 duration=2 rate=20"),
            (
                "zero-rate",
                "video path=\"clip.mp4\" start=0 duration=2 rate=0",
            ),
            (
                "negative-rate",
                "video path=\"clip.mp4\" start=0 duration=2 rate=-1",
            ),
            (
                "negative-trim",
                "video path=\"clip.mp4\" start=0 duration=2 trim-start=-1",
            ),
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
                assert!(
                    !d.file.is_empty() && d.line >= 1 && d.column >= 1,
                    "{tag}: {d:?}"
                );
                assert!(!d.fix.is_empty(), "{tag}: {d:?}");
                assert!(validate::catalog::lookup(d.code).is_some());
            }
            // Boundary values stay in the message: rates name 0.1–10,
            // windows name both ends.
            let human = format_check_human(&folder, &v, None);
            let json = format_check_json(&folder, &v, None);
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
        let human = format_check_human(&folder, &v, None);
        let json = format_check_json(&folder, &v, None);
        assert_eq!(human.matches("error[E_BAD_FOOTAGE]").count(), 3, "{human}");
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
            v.diagnostics
                .iter()
                .any(|d| d.code == "E_BAD_FOOTAGE" && d.message.contains("backward")),
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
        let human = format_check_human(&folder, &v, None);
        let json = format_check_json(&folder, &v, None);
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
            v.diagnostics
                .iter()
                .any(|d| d.code == "E_BAD_FOOTAGE" && d.message.contains("explicit `duration`")),
            "{:?}",
            v.diagnostics
        );
        let human = format_check_human(&folder, &v, None);
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
            v.diagnostics
                .iter()
                .any(|d| d.code == "E_BAD_FOOTAGE" && d.message.contains("`true` or `false`")),
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
        assert!(format_check_human(&folder, &v, None).contains("loop off"));
    }

    /// T007: later-sprint audio use — effects, automation, groups, and
    /// ducking keys plus the matching elements — checks with exit code
    /// 2 naming the element and its enabling sprint, in both forms, and
    /// `render` refuses before the first frame with no MP4.
    #[test]
    fn later_sprint_audio_use_names_enabling_sprint() {
        for (tag, body) in [
            (
                "key",
                "audio path=\"s.wav\" start=0 duration=1 effect=\"reverb\"",
            ),
            (
                "vkey",
                "video path=\"clip.mp4\" start=0 duration=1 ducking=0.5",
            ),
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
            let human = format_check_human(&folder, &v, None);
            let json = format_check_json(&folder, &v, None);
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
            before
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
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

    const T010_TWO_COMP: &str = "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000\" version=1 root=true {\n    box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0.5 duration=1 id=\"red\" track=\"video\" label=\"cover\"\n    box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1.5 duration=0.5 hidden=true\n}\ncomposition name=\"cut\" canvas=\"square\" duration=1 fps=30 background=\"#112233\" version=1 {\n    text value=\"Cut\" x=0 y=0 size=8 color=\"#fff\" start=0 duration=1 id=\"ct\" track=\"caption\" label=\"cap\"\n}\n";

    /// Sprint 6 T010: the `compositions` human and JSON forms carry the
    /// same names, clip counts, and root.
    #[test]
    fn compositions_forms_carry_the_same_names_and_root() {
        let folder = tmp_dir("t010unit");
        write(&folder, validate_mod::COMPOSITION_FILE, T010_TWO_COMP);
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        let human = format_compositions_human(&folder, &v);
        let json = format_compositions_json(&folder, &v);
        for name in ["main", "cut"] {
            assert!(human.contains(&format!("{name:?}")), "{human}");
            assert!(json.contains(&format!("\"name\":\"{name}\"")), "{json}");
        }
        assert!(human.contains("\"main\" (root, 2 clip(s))"), "{human}");
        assert!(
            json.contains("\"name\":\"main\",\"clips\":2,\"root\":true"),
            "{json}"
        );
        assert!(
            json.contains("\"name\":\"cut\",\"clips\":1,\"root\":false"),
            "{json}"
        );
        assert!(json.contains("\"root\":\"main\""), "{json}");
        assert_eq!(cmd_compositions(&folder, false), 0);
        assert_eq!(cmd_compositions(&folder, true), 0);
    }

    /// Sprint 6 T010: the `timeline` human and JSON forms carry the
    /// same rows — id, track, label, hidden state, and window.
    #[test]
    fn timeline_forms_carry_the_same_rows() {
        let folder = tmp_dir("t010unit");
        write(&folder, validate_mod::COMPOSITION_FILE, T010_TWO_COMP);
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        let comp = v.composition.as_ref().unwrap();
        let human = format_timeline_human(&folder, comp);
        let json = format_timeline_json(&folder, &v, comp);
        assert!(
            human.contains(
                "[0] box id=\"red\" track=video label=\"cover\" visible window 0.500s..1.500s."
            ),
            "{human}"
        );
        assert!(
            human.contains("[1] box id=- track=graphics label=- hidden window 1.500s..2.000s."),
            "{human}"
        );
        assert!(json.contains("\"id\":\"red\",\"track\":\"video\",\"label\":\"cover\",\"hidden\":false,\"start\":0.5,\"end\":1.5"), "{json}");
        assert!(json.contains("\"id\":null,\"track\":\"graphics\",\"label\":null,\"hidden\":true,\"start\":1.5,\"end\":2"), "{json}");
        assert_eq!(cmd_timeline(&folder, None, false), 0);
        assert_eq!(cmd_timeline(&folder, Some("cut"), true), 0);
        assert_eq!(cmd_timeline(&folder, Some("ghost"), false), 1);
        assert_eq!(cmd_timeline(&folder, Some("ghost"), true), 1);
    }

    /// Sprint 6 T010: the `info` human and JSON forms carry the same
    /// canvas, duration, and clip counts.
    #[test]
    fn info_forms_carry_the_same_summary() {
        let folder = tmp_dir("t010unit");
        write(&folder, validate_mod::COMPOSITION_FILE, T010_TWO_COMP);
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        let comp = v.composition.as_ref().unwrap();
        let human = format_info_human(&folder, comp);
        let json = format_info_json(&folder, comp);
        assert!(human.contains("canvas: 64x64."), "{human}");
        assert!(
            human.contains("duration: 2.000s (60 frames at 30fps)."),
            "{human}"
        );
        assert!(
            human.contains(
                "clips: 2 total (text=0, video=0, box=2, image=0, audio=0, nest=0; hidden=1)."
            ),
            "{human}"
        );
        assert!(
            json.contains("\"canvas\":{\"width\":64,\"height\":64,\"preset\":null}"),
            "{json}"
        );
        assert!(
            json.contains("\"duration\":2,\"fps\":\"30\",\"frames\":60"),
            "{json}"
        );
        assert!(json.contains("\"total\":2"), "{json}");
        assert!(json.contains("\"hidden\":1"), "{json}");
        let cut = v.compositions.iter().find(|c| c.name == "cut").unwrap();
        let cut_human = format_info_human(&folder, cut);
        assert!(
            cut_human.contains("canvas: 1080x1080 (preset \"square\")."),
            "{cut_human}"
        );
        assert_eq!(cmd_info(&folder, None, false), 0);
        assert_eq!(cmd_info(&folder, Some("cut"), true), 0);
        assert_eq!(cmd_info(&folder, Some("ghost"), false), 1);
        assert_eq!(cmd_info(&folder, Some("ghost"), true), 1);
    }

    /// Sprint 6 T010: listing refusals carry the same diagnostics as
    /// `check`, only under the listing command name.
    #[test]
    fn listing_refusals_match_check_diagnostics() {
        let folder = tmp_dir("t010unit");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 bogus=1 background=\"#000\" version=1 {\n}\n",
        );
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Invalid);
        let check = format_check_json(&folder, &v, None);
        let timeline = format_verdict_json("timeline", &folder, &v, None);
        assert!(timeline.contains("\"command\":\"timeline\""), "{timeline}");
        // Same diagnostic payload, only the command name differs.
        let strip =
            |s: &str, cmd: &str| s.replace(&format!("\"command\":\"{cmd}\""), "\"command\":\"?\"");
        assert_eq!(strip(&timeline, "timeline"), strip(&check, "check"));
        assert_eq!(cmd_compositions(&folder, false), 1);
        assert_eq!(cmd_compositions(&folder, true), 1);
        assert_eq!(cmd_timeline(&folder, None, false), 1);
        assert_eq!(cmd_timeline(&folder, None, true), 1);
        assert_eq!(cmd_info(&folder, None, false), 1);
        assert_eq!(cmd_info(&folder, None, true), 1);
    }

    /// Sprint 10 sound (CAP-007): `info` carries the playback hint in
    /// both forms, defaulting to the still start when undeclared.
    #[test]
    fn info_carries_autoplay_in_both_forms() {
        for (tag, attrs, human_want, json_want) in [
            (
                "flagged",
                " autoplay=true",
                "autoplay: true.",
                "\"autoplay\":true",
            ),
            (
                "quoted",
                " autoplay=\"false\"",
                "autoplay: false.",
                "\"autoplay\":false",
            ),
            ("absent", "", "autoplay: false.", "\"autoplay\":false"),
        ] {
            let folder = tmp_dir(tag);
            write(
                &folder,
                validate_mod::COMPOSITION_FILE,
                &format!(
                    "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1{attrs} {{\n    text value=\"Hi\" start=0 duration=1\n}}\n"
                ),
            );
            let v = validate_folder(&folder);
            assert_eq!(v.status, Status::Valid, "{tag}: {:?}", v.diagnostics);
            let comp = v.composition.as_ref().unwrap();
            let human = format_info_human(&folder, comp);
            let json = format_info_json(&folder, comp);
            assert!(human.contains(human_want), "{tag}: {human}");
            assert!(json.contains(json_want), "{tag}: {json}");
            // The JSON stays balanced with the new field in place.
            assert_eq!(
                json.matches('{').count(),
                json.matches('}').count(),
                "{json}"
            );
            assert_eq!(cmd_info(&folder, None, false), 0);
            assert_eq!(cmd_info(&folder, None, true), 0);
        }
    }

    /// Sprint 10 sound (CAP-011): `check` reports the preview master
    /// peak in both forms whenever one is measurable, and warns naming
    /// the peak exactly when it exceeds −1 dBFS — all without changing
    /// the verdict shape when no peak is measurable.
    #[test]
    fn check_reports_master_peak_and_hot_warning_in_both_forms() {
        let folder = tmp_dir("peakforms");
        write(&folder, validate_mod::COMPOSITION_FILE, VALID);
        let v = validate_folder(&folder);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        // Full scale warns naming the peak, in both forms.
        let human = format_check_human(&folder, &v, Some(32767));
        assert!(human.contains("master peak: -0.0 dBFS."), "{human}");
        assert!(
            human.contains("warning: hot master: peak -0.0 dBFS is above -1.0 dBFS"),
            "{human}"
        );
        let json = format_check_json(&folder, &v, Some(32767));
        assert!(json.contains("\"master_peak\":"), "{json}");
        assert!(json.contains("\"master_peak_dbfs\":-0."), "{json}");
        assert!(
            json.contains("\"peak_warning\":\"warning: hot master: peak -0.0 dBFS"),
            "{json}"
        );
        // A quiet master reports with no warning; silence reports -inf.
        let human = format_check_human(&folder, &v, Some(1000));
        assert!(human.contains("master peak: -"), "{human}");
        assert!(!human.contains("hot master"), "{human}");
        let json = format_check_json(&folder, &v, Some(1000));
        assert!(json.contains("\"peak_warning\":null"), "{json}");
        let human = format_check_human(&folder, &v, Some(0));
        assert!(
            human.contains("master peak: -inf dBFS (digital silence)."),
            "{human}"
        );
        assert!(!human.contains("hot master"), "{human}");
        // No measurable peak changes nothing.
        let human = format_check_human(&folder, &v, None);
        assert!(!human.contains("master peak"), "{human}");
        let json = format_check_json(&folder, &v, None);
        assert!(json.contains("\"master_peak\":null"), "{json}");
        assert!(json.contains("\"peak_warning\":null"), "{json}");
    }

    /// Sprint 10 sound (CAP-008): `waveform` refuses bad bucket counts
    /// and invalid projects before any decode, with no listing.
    #[test]
    fn waveform_refusals_need_no_decoder() {
        let missing = tmp_dir("wavemissing").join("nope");
        for buckets in ["0", "-3", "lots"] {
            let req = WaveformRequest {
                composition: None,
                buckets: Some(buckets.to_string()),
                json: false,
            };
            assert_eq!(cmd_waveform(&missing, &req), 1);
            let req = WaveformRequest {
                composition: None,
                buckets: Some(buckets.to_string()),
                json: true,
            };
            assert_eq!(cmd_waveform(&missing, &req), 1);
        }
        let folder = tmp_dir("waveinvalid");
        write(
            &folder,
            validate_mod::COMPOSITION_FILE,
            "composition name=\"t\" width=64 height=64 duration=1 bogus=1 background=\"#000\" version=1 {\n}\n",
        );
        let req = WaveformRequest {
            composition: None,
            buckets: None,
            json: false,
        };
        assert_eq!(cmd_waveform(&folder, &req), 1);
        let req = WaveformRequest {
            composition: None,
            buckets: None,
            json: true,
        };
        assert_eq!(cmd_waveform(&folder, &req), 1);
    }
}
