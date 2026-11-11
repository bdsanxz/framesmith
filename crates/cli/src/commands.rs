//! `init`, `check`, `lint`, `render` and `errors` command implementations.
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
use sdk::history as sdk_history;
use sdk::{ClipRef, OpenOptions, SdkSession};
use std::path::{Path, PathBuf};
use validate::catalog::{self, json_escape};
use validate::lint;
use validate::validate as validate_mod;
use validate_mod::{
    validate_folder, validate_folder_with_variables_strict_themed, Status, Verdict,
};

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
///
/// Every project also gains both agent guide files (`AGENTS.md` and
/// `CLAUDE.md`); pass `example` to start from a bundled starter
/// instead of the minimal scaffold (Sprint 19, T008; guides deepened
/// by the agents sprint, T002). Refusals change nothing.
pub fn cmd_init(name: &str, parent: &Path) -> i32 {
    cmd_init_with_example(name, parent, None)
}

/// `init` with an optional starter example (Sprint 19, T008 / CAP-008):
/// gate order is name, then example, then target occupancy, and nothing
/// is written until all three pass. An unknown example refuses listing
/// every available starter; a non-empty target refuses leaving its
/// content byte-identical. Returns the process exit code.
pub fn cmd_init_with_example(name: &str, parent: &Path, example: Option<&str>) -> i32 {
    if let Some(rule) = name_error(name) {
        eprintln!(
            "framesmith init refused: invalid project name {name:?}: {rule}.\n\
             fix: choose a name of 1-64 letters, digits, `-` or `_`, starting with a letter or digit."
        );
        return 1;
    }
    // The example gate runs before touching the filesystem: an unknown
    // name refuses here, so no directory is ever created for it.
    let starter = match example {
        None => None,
        Some(want) => match crate::starters::find_starter(want) {
            Some(s) => Some(s),
            None => {
                eprintln!(
                    "framesmith init refused: unknown starter example {want:?}.\n\
                     available starters: {}.\n\
                     fix: pass one of those names to `--example`; nothing was created.",
                    crate::starters::starter_list_line()
                );
                return 1;
            }
        },
    };
    let composition = match starter {
        Some(s) => crate::starters::starter_composition(s, name),
        None => scaffold_composition(name),
    };
    let guide = crate::starters::agent_guide(name, starter);
    let claude_guide = crate::starters::claude_guide(name, starter);
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
    let guide_path = target.join(crate::starters::AGENT_GUIDE_FILE);
    let claude_path = target.join(crate::starters::CLAUDE_GUIDE_FILE);
    if let Err(e) = std::fs::write(&comp_path, &composition)
        .and_then(|()| std::fs::write(&guide_path, &guide))
        .and_then(|()| std::fs::write(&claude_path, &claude_guide))
    {
        // No partial folder on failure: remove what this run created.
        let _ = std::fs::remove_file(&comp_path);
        let _ = std::fs::remove_file(&guide_path);
        let _ = std::fs::remove_file(&claude_path);
        // Only remove the directory when this run created it and it is
        // still empty (never touch pre-existing content).
        if dir_is_empty(&target).unwrap_or(false) {
            let _ = std::fs::remove_dir(&target);
        }
        eprintln!(
            "framesmith init refused: cannot write project files: {e}.\n\
             fix: choose a writable location; nothing was left behind."
        );
        return 1;
    }
    match starter {
        Some(s) => println!(
            "created project {} from starter example {:?} ({}).",
            target.display(),
            s.name,
            validate_mod::COMPOSITION_FILE
        ),
        None => println!(
            "created project {} ({}).",
            target.display(),
            validate_mod::COMPOSITION_FILE
        ),
    }
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

/// One dropped file marker as human text (Sprint 21 adapters,
/// T001): the lottie clip, the marker name and range, and why it
/// names no range, so the drop is reviewable while the verdict
/// stays valid.
fn format_marker_drop_human(f: &validate_mod::MarkerDrop) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let name = match &f.id {
        Some(id) => format!("{id:?}"),
        None => format!("#{}", f.clip),
    };
    let _ = writeln!(
        out,
        "note: lottie {:?} {} ({}:{}): marker {:?} {:.3}s..{:.3}s dropped ({}).",
        f.path, name, f.line, f.column, f.name, f.start, f.end, f.detail
    );
    out
}

/// One adapter source readiness as human text (Sprint 21 adapters,
/// T007 / CAP-007): the source kind, name, state, and — for failed
/// sources — the cause validation refused with, so `check` lists the
/// same readiness the render path gates on: per source, never per
/// frame.
fn format_readiness_human(f: &validate::readiness::AdapterReadiness) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "{f}");
    out
}

/// Per-source adapter readiness as human text (Sprint 21 adapters,
/// T007 / CAP-007): every entry plus the ready/loading/failed tally,
/// so a mixed film lists each source once. Empty for projects with
/// no adapter clips, so default output is byte-identical.
fn format_readiness_section_human(verdict: &Verdict) -> String {
    use std::fmt::Write;
    if verdict.readiness.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for f in &verdict.readiness {
        out.push_str(&format_readiness_human(f));
    }
    let mut ready = 0;
    let mut loading = 0;
    let mut failed = 0;
    for f in &verdict.readiness {
        match f.state {
            validate::readiness::AdapterState::Ready => ready += 1,
            validate::readiness::AdapterState::Loading => loading += 1,
            validate::readiness::AdapterState::Failed => failed += 1,
        }
    }
    let _ = writeln!(
        out,
        "readiness: {} source(s): {ready} ready, {loading} loading, {failed} failed.",
        verdict.readiness.len()
    );
    out
}

/// Every in-window file marker as human text (Sprint 21 adapters,
/// T001): the marker name plus its named range on the owning lottie
/// clip, so marker ranges are reviewable before rendering.
fn format_lottie_markers_human(comp: &Composition) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for clip in &comp.clips {
        if let notation::model::Clip::Lottie(lo) = clip {
            for m in &lo.markers {
                let _ = writeln!(
                    out,
                    "marker {:?} on lottie {:?} ({}:{}): range {:.3}s..{:.3}s, loop {}.",
                    m.name,
                    lo.path,
                    lo.line,
                    lo.column,
                    m.start,
                    m.end,
                    lo.loop_mode.name()
                );
            }
        }
    }
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

/// Human-readable lint report (Sprint 13 check, T001 / CAP-001): the
/// batched findings plus unrunnable-rule notes, then the explicit
/// zero-or-count summary. Pure function shared by `check` and `lint`
/// so both commands print the same findings for the same project.
pub fn format_lint_human(folder: &Path, verdict: &Verdict) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    // Sprint 21 adapters (T007 / CAP-007): `lint` lists the same
    // per-source readiness `check` lists, so adapter projects read
    // identically under both commands.
    out.push_str(&format_readiness_section_human(verdict));
    for f in &verdict.lint_findings {
        let _ = writeln!(out, "{f}");
    }
    for n in &verdict.lint_notes {
        let _ = writeln!(out, "{n}");
    }
    if verdict.lint_findings.is_empty() && verdict.lint_notes.is_empty() {
        let _ = writeln!(
            out,
            "lint: 0 findings across {} rules in {}.",
            lint::RULE_IDS.len(),
            folder.display()
        );
    } else {
        let _ = writeln!(
            out,
            "lint: {} finding(s) and {} note(s) across {} rules in {} (warnings do not block).",
            verdict.lint_findings.len(),
            verdict.lint_notes.len(),
            lint::RULE_IDS.len(),
            folder.display()
        );
    }
    out
}

/// One validation diagnostic as JSON (Sprint 1 T007): file, line,
/// column, code, message, and fix. Shared by the `check` and `lint`
/// machine-readable reports so both name the same problems.
fn push_diagnostics_json(out: &mut String, verdict: &Verdict) {
    use std::fmt::Write;
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
}

/// Lint findings and notes as JSON arrays (Sprint 13 check, T001 /
/// CAP-001): `"lint":[...]` plus `"lint_notes":[...]`, every finding
/// carrying its rule, severity, source, file, line, column, message,
/// and fix. Shared by `check` and `lint` so both commands report the
/// same findings machine-readably.
fn push_lint_json(out: &mut String, verdict: &Verdict) {
    use std::fmt::Write;
    out.push_str("\"lint\":[");
    for (i, f) in verdict.lint_findings.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"rule\":\"{}\",\"severity\":\"{}\",\"source\":\"{}\",\"file\":\"{}\",\"line\":{},\"column\":{},\"message\":\"{}\",\"fix\":\"{}\"}}",
            json_escape(f.rule),
            json_escape(f.severity.as_str()),
            json_escape(f.source.as_str()),
            json_escape(&f.file),
            f.line,
            f.column,
            json_escape(&f.message),
            json_escape(&f.fix)
        );
    }
    out.push_str("],\"lint_notes\":[");
    for (i, n) in verdict.lint_notes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"rule\":\"{}\",\"file\":\"{}\",\"line\":{},\"column\":{},\"cause\":\"{}\",\"workaround\":\"{}\"}}",
            json_escape(n.rule),
            json_escape(&n.file),
            n.line,
            n.column,
            json_escape(&n.cause),
            json_escape(&n.workaround)
        );
    }
    out.push(']');
}

/// Per-source adapter readiness as a JSON array (Sprint 21 adapters,
/// T007 / CAP-007): the same entries the human `check` lists, so
/// `check --json` and `lint --json` agree with the human listing.
/// Empty for adapter-free projects. `cause` is null unless failed.
fn push_readiness_json(out: &mut String, verdict: &Verdict) {
    use std::fmt::Write;
    out.push_str("\"readiness\":[");
    for (i, f) in verdict.readiness.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let id = match &f.id {
            Some(v) => format!("\"{}\"", json_escape(v)),
            None => "null".to_string(),
        };
        let cause = match &f.cause {
            Some(v) => format!("\"{}\"", json_escape(v)),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"clip\":{},\"id\":{},\"kind\":\"{}\",\"name\":\"{}\",\"source\":\"{}\",\"state\":\"{}\",\"cause\":{},\"line\":{},\"column\":{}}}",
            f.clip,
            id,
            json_escape(f.kind),
            json_escape(&f.display_name()),
            json_escape(&f.source),
            f.state.as_str(),
            cause,
            f.line,
            f.column,
        );
    }
    out.push(']');
}

/// Apply release-gate strictness to a verdict's lint batch (Sprint 13
/// check, T007 / CAP-007): elevate every matching finding from
/// warning to error and return the elevated count. The finding and
/// note counts never change; callers compare counts across modes to
/// prove strictness invents nothing.
pub fn apply_strict_to_verdict(verdict: &mut Verdict, mode: lint::StrictMode) -> usize {
    let mut elevated = 0;
    for finding in &mut verdict.lint_findings {
        if lint::strict_elevates(finding.rule, mode)
            && finding.severity != lint::LintSeverity::Error
        {
            finding.severity = lint::LintSeverity::Error;
            elevated += 1;
        }
    }
    elevated
}

/// Sorted unique rule ids currently at error severity in a verdict
/// that strictness elevated (Sprint 13 check, T007): the named set
/// for gate messages, so a stopped run says which rules blocked it.
pub fn elevated_rule_names(verdict: &Verdict) -> Vec<&'static str> {
    let mut rules: Vec<&'static str> = verdict
        .lint_findings
        .iter()
        .filter(|f| f.severity == lint::LintSeverity::Error)
        .map(|f| f.rule)
        .collect();
    rules.sort_unstable();
    rules.dedup();
    rules
}

/// Human-readable strict-gate stop line for `check` and `lint`
/// (Sprint 13 check, T007 / REC-004): the mode, the elevated count,
/// and the fix. The elevated findings themselves print just above
/// through the normal lint batch, each with its rule and fix; this
/// line states the gate so the stop is explicit on a valid project.
pub fn format_strict_gate_human(mode: lint::StrictMode, elevated: usize) -> String {
    format!(
        "strict: {elevated} finding(s) elevated to errors by {} on a valid project; fix the findings above or re-run without {} to keep warnings as warnings. Nothing was written.",
        mode.flag(),
        mode.flag()
    )
}

/// Human-readable lint summary with the strict footer swapped in
/// (Sprint 13 check, T007): the normal human batch, except the
/// `(warnings do not block)` tail reads as the elevation it became
/// when `mode` elevated at least one finding. Pure function shared
/// by the `check` and `lint` strict paths so both stay consistent.
pub fn format_lint_human_gated(
    folder: &Path,
    verdict: &Verdict,
    mode: lint::StrictMode,
    elevated: usize,
) -> String {
    let out = format_lint_human(folder, verdict);
    if elevated == 0 || mode == lint::StrictMode::Off || verdict.status != Status::Valid {
        return out;
    }
    let mut out = out.replacen(
        "(warnings do not block)",
        &format!("({elevated} elevated to errors by {})", mode.flag()),
        1,
    );
    out.push_str(&format_strict_gate_human(mode, elevated));
    out.push('\n');
    out
}

/// Fold a strict gate into a verdict JSON report (Sprint 13 check,
/// T007): the normal report, except a gated valid run exits 1 (the
/// REC-004 gate shares the invalid exit) and carries the mode plus
/// the elevated count. Only valid reports gate: invalid and
/// unsupported reports keep their own exits. Pure function shared by
/// the `check` and `lint` strict paths.
fn patch_gated_json(report: String, mode: lint::StrictMode, elevated: usize) -> String {
    let mut out = report.replacen("\"exit\":0", "\"exit\":1", 1);
    out.pop();
    use std::fmt::Write;
    let _ = write!(
        out,
        ",\"strict\":\"{}\",\"elevated\":{}}}",
        mode.as_str(),
        elevated
    );
    out
}

/// Machine-readable lint report with the strict gate folded in
/// (Sprint 13 check, T007 / CAP-001): like [`format_lint_json`],
/// except a gated valid run exits 1 and names its mode. Pure
/// function for testability.
pub fn format_lint_json_gated(
    folder: &Path,
    verdict: &Verdict,
    mode: lint::StrictMode,
    elevated: usize,
) -> String {
    let report = format_lint_json(folder, verdict);
    if elevated == 0 || mode == lint::StrictMode::Off || verdict.status != Status::Valid {
        report
    } else {
        patch_gated_json(report, mode, elevated)
    }
}

/// Machine-readable check report with the strict gate folded in
/// (Sprint 13 check, T007): like [`format_check_json`], except a
/// gated valid run exits 1 and names its mode. Pure function for
/// testability.
pub fn format_check_json_gated(
    folder: &Path,
    verdict: &Verdict,
    master: Option<u32>,
    mode: lint::StrictMode,
    elevated: usize,
) -> String {
    let report = format_check_json(folder, verdict, master);
    if elevated == 0 || mode == lint::StrictMode::Off || verdict.status != Status::Valid {
        report
    } else {
        patch_gated_json(report, mode, elevated)
    }
}

/// Machine-readable lint report (Sprint 13 check, T001 / CAP-001): the
/// same verdict status, exit, and diagnostics as `check` plus the same
/// lint arrays, under the `lint` command name. Pure function for
/// testability.
pub fn format_lint_json(folder: &Path, verdict: &Verdict) -> String {
    use std::fmt::Write;
    let (status, exit) = match verdict.status {
        Status::Valid => ("valid", 0),
        Status::Invalid => ("invalid", 1),
        Status::Unsupported => ("unsupported", 2),
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"lint\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"diagnostics\":[",
        json_escape(&folder.display().to_string())
    );
    push_diagnostics_json(&mut out, verdict);
    out.push_str("],");
    push_lint_json(&mut out, verdict);
    // Sprint 21 adapters (T007 / CAP-007): per-source readiness rides
    // after `lint_rules`, so the long-pinned `lint_notes`-to-`lint_rules`
    // adjacency the agreement tests slice between stays intact.
    let _ = write!(out, ",\"lint_rules\":{}", lint::RULE_IDS.len());
    out.push_str(",");
    push_readiness_json(&mut out, verdict);
    out.push('}');
    out
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
    // Sprint 9 variables (CAP-003): unknown `--variables` keys warn
    // visibly in default mode without blocking; every warning names the
    // key and the fix, like a diagnostic but with a zero exit code.
    for w in &verdict.variable_warnings {
        let _ = writeln!(out, "{w}");
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
    for comp in &verdict.compositions {
        out.push_str(&format_lottie_markers_human(comp));
    }
    for f in &verdict.marker_drops {
        out.push_str(&format_marker_drop_human(f));
    }
    // Sprint 21 adapters (T007 / CAP-007): per-source readiness
    // across adapter kinds, with the same values the render path
    // gates on. Empty for adapter-free projects.
    out.push_str(&format_readiness_section_human(verdict));
    // Sprint 10 sound (CAP-011): the master peak is always reported
    // when measurable; a hot master additionally warns naming the
    // peak, without changing the exit code.
    if let Some(p) = master {
        let _ = writeln!(out, "master peak: {}.", render::peaks::format_dbfs(p));
        if let Some(w) = hot_master_warning(p) {
            let _ = writeln!(out, "{w}");
        }
    }
    // Sprint 13 check, T001 / CAP-001: `check` runs the lint rules
    // together with validation and reports everything at once — the
    // same batch `lint` prints, so both commands agree.
    out.push_str(&format_lint_human(folder, verdict));
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

/// Human-readable check report with the strict gate folded in
/// (Sprint 13 check, T007): like [`format_check_human`], except a
/// gated valid run swaps the warnings footer for the elevation and
/// appends the strict stop line. Pure function for testability.
pub fn format_check_human_gated(
    folder: &Path,
    verdict: &Verdict,
    master: Option<u32>,
    mode: lint::StrictMode,
    elevated: usize,
) -> String {
    let out = format_check_human(folder, verdict, master);
    if elevated == 0 || mode == lint::StrictMode::Off || verdict.status != Status::Valid {
        return out;
    }
    let mut out = out.replacen(
        "(warnings do not block)",
        &format!("({elevated} elevated to errors by {})", mode.flag()),
        1,
    );
    out.push_str(&format_strict_gate_human(mode, elevated));
    out.push('\n');
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
    push_diagnostics_json(&mut out, verdict);
    // Sprint 13 check, T001 / CAP-001: the same lint batch the `lint`
    // command prints, so `check --json` and `lint --json` agree.
    out.push_str("],");
    push_lint_json(&mut out, verdict);
    // Sprint 5 T001/T009: the same effective windows as the human echo,
    // plus the resolved clip length and its inferred marking, so
    // inference and clamping are reviewable machine-readably.
    // Sprint 9 variables (CAP-003): unknown `--variables` keys ride
    // along machine-readably with the same key, message, and fix the
    // human warning carries; empty for override-free runs.
    out.push_str(",\"variable_warnings\":[");
    for (i, w) in verdict.variable_warnings.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"key\":\"{}\",\"message\":\"{}\",\"fix\":\"{}\"}}",
            json_escape(&w.key),
            json_escape(&w.message),
            json_escape(&w.fix)
        );
    }
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
    // Sprint 21 adapters (T001): every dropped file marker as
    // reviewable numbers, so marker drops are machine-readable like
    // waveform-range cuts.
    out.push_str("],\"marker_drops\":[");
    for (i, f) in verdict.marker_drops.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let id = match &f.id {
            Some(v) => format!("\"{}\"", json_escape(v)),
            None => "null".to_string(),
        };
        let _ = write!(
            out,
            "{{\"clip\":{},\"id\":{},\"path\":\"{}\",\"name\":\"{}\",\"start\":{},\"end\":{},\"detail\":\"{}\",\"line\":{},\"column\":{}}}",
            f.clip,
            id,
            json_escape(&f.path),
            json_escape(&f.name),
            num(Some(f.start)),
            num(Some(f.end)),
            json_escape(&f.detail),
            f.line,
            f.column,
        );
    }
    // Sprint 21 adapters (T007 / CAP-007): per-source readiness as
    // reviewable values, so the machine listing agrees with the
    // human one and with the render gate.
    out.push_str("],");
    push_readiness_json(&mut out, verdict);
    // Sprint 10 sound (CAP-011): the preview master peak plus the hot
    // warning, in the same numbers the human lines carry; nulls when
    // no peak was measurable, so refusal shapes stay identical.
    out.push_str(",\"master_peak\":");
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
    cmd_check_full(folder, json, backend, None)
}

/// `check` with per-render caller values and no strict gate (Sprint 9
/// variables, CAP-003): preserved for existing callers; strict runs go
/// through [`cmd_check_full_strict`].
pub fn cmd_check_full(
    folder: &Path,
    json: bool,
    backend: Option<&str>,
    variables: Option<&str>,
) -> i32 {
    cmd_check_full_strict(
        folder,
        json,
        backend,
        variables,
        false,
        lint::StrictMode::Off,
    )
}

/// `check` with per-render caller values and an explicit strict switch
/// (Sprint 9 variables, CAP-003/CAP-004): the host entry for overrides
/// plus strict mode. `variables` is the raw `--variables` JSON payload;
/// `None` validates every binding at its declared default. A misshapen
/// payload fails naming the flag and the expected shape (exit 1);
/// otherwise the overridden composition validates exactly like an
/// unoverridden one, with unknown keys and unused declarations warned
/// visibly in default mode and refused under `strict`
/// (`--strict-variables`), which additionally requires every
/// default-less variable to receive a caller value.
pub fn cmd_check_full_strict(
    folder: &Path,
    json: bool,
    backend: Option<&str>,
    variables: Option<&str>,
    strict: bool,
    strict_mode: lint::StrictMode,
) -> i32 {
    cmd_check_full_strict_themed(folder, json, backend, variables, strict, None, strict_mode)
}

/// `check` with per-render caller values, strict mode, and a theme
/// selection (Sprint 9 variables, CAP-005): `theme` is the raw
/// `--theme` name; `None` selects no caller theme, so each composition
/// falls back to its own `theme` attribute. The caller theme beats the
/// composition attribute; an unknown name on either side refuses before
/// frame one naming the available themes. `strict_mode` is the release
/// gate (Sprint 13 check, T007 / CAP-007): `--strict` elevates the
/// named warnings to errors, `--strict-all` elevates every warning;
/// a gated valid run exits 1 listing each elevated finding.
pub fn cmd_check_full_strict_themed(
    folder: &Path,
    json: bool,
    backend: Option<&str>,
    variables: Option<&str>,
    strict: bool,
    theme: Option<&str>,
    strict_mode: lint::StrictMode,
) -> i32 {
    if let Some(raw) = backend {
        if render::render::Backend::parse(raw).is_none() {
            let msg =
                format!("framesmith check: unknown --backend {raw:?}: expected cpu, gpu, or auto.");
            eprintln!("{msg}");
            return 1;
        }
    }
    let overrides = match variables {
        None => validate_mod::VariableOverrides::new(),
        Some(raw) => match validate_mod::parse_variable_overrides(raw) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("framesmith check: {e}");
                return 1;
            }
        },
    };
    let run = log::run_id();
    let mut verdict =
        validate_folder_with_variables_strict_themed(folder, &overrides, strict, theme);
    // Sprint 10 sound (CAP-009): silent normalization fails closed in
    // `check` too, before any peak is previewed.
    apply_normalize_silence_gate(folder, &mut verdict);
    // Sprint 13 check (T007 / CAP-007): elevate after every other
    // verdict mutation so the finding count matches the default run
    // exactly; only a valid project with elevated findings gates.
    let elevated = apply_strict_to_verdict(&mut verdict, strict_mode);
    let gated = verdict.status == Status::Valid && elevated > 0;
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
        println!(
            "{}",
            format_check_json_gated(folder, &verdict, master, strict_mode, elevated)
        );
    } else {
        print!(
            "{}",
            format_check_human_gated(folder, &verdict, master, strict_mode, elevated)
        );
    }
    let code = if gated {
        1
    } else {
        match verdict.status {
            Status::Valid => 0,
            Status::Invalid => 1,
            Status::Unsupported => 2,
        }
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
    let (level, what) = if gated {
        (
            "warn",
            format!(
                "check gated by {} ({} elevated)",
                strict_mode.flag(),
                elevated
            ),
        )
    } else {
        match verdict.status {
            Status::Valid => ("info", "check valid".to_string()),
            Status::Invalid => ("warn", "check invalid".to_string()),
            Status::Unsupported => ("warn", "check unsupported".to_string()),
        }
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

/// `lint`: report the named-rule findings without rendering (Sprint 13
/// check, T001 / CAP-001). Runs the same validation as `check` and
/// prints the same batched lint report `check` embeds — human text or
/// machine-readable JSON — and exits with the same codes (0 valid,
/// 1 invalid, 2 unsupported). Findings are warnings and never change
/// the exit on their own; on invalid or unsupported projects the
/// validation diagnostics print first, then the lint batch, so one run
/// still reports everything at once. Never writes an MP4 and never
/// shells to FFmpeg: the T001 packs are static analysis.
pub fn cmd_lint(folder: &Path, json: bool, backend: Option<&str>) -> i32 {
    cmd_lint_full_strict_themed(
        folder,
        json,
        backend,
        None,
        false,
        None,
        lint::StrictMode::Off,
    )
}

/// Resolve the release-gate strictness from the `--strict` and
/// `--strict-all` flags (Sprint 13 check, T007 / CAP-007):
/// `--strict-all` wins when both are set, though the CLI refuses the
/// combination with exit 2 before it can reach here.
pub fn resolve_strict_mode(strict: bool, strict_all: bool) -> lint::StrictMode {
    if strict_all {
        lint::StrictMode::All
    } else if strict {
        lint::StrictMode::Named
    } else {
        lint::StrictMode::Off
    }
}

/// `lint` with per-render caller values, strict variable mode, and a
/// theme selection (Sprint 13 check, T001): the same knobs as
/// [`cmd_check_full_strict_themed`], the same validation underneath,
/// only the report differs. `strict_mode` is the release gate
/// (Sprint 13 check, T007 / CAP-007): a gated valid run exits 1
/// listing each elevated finding with its rule and fix.
pub fn cmd_lint_full_strict_themed(
    folder: &Path,
    json: bool,
    backend: Option<&str>,
    variables: Option<&str>,
    strict: bool,
    theme: Option<&str>,
    strict_mode: lint::StrictMode,
) -> i32 {
    if let Some(raw) = backend {
        if render::render::Backend::parse(raw).is_none() {
            let msg =
                format!("framesmith lint: unknown --backend {raw:?}: expected cpu, gpu, or auto.");
            eprintln!("{msg}");
            return 1;
        }
    }
    let overrides = match variables {
        None => validate_mod::VariableOverrides::new(),
        Some(raw) => match validate_mod::parse_variable_overrides(raw) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("framesmith lint: {e}");
                return 1;
            }
        },
    };
    let run = log::run_id();
    let mut verdict =
        validate_folder_with_variables_strict_themed(folder, &overrides, strict, theme);
    // Sprint 13 check (T007 / CAP-007): elevate after validation so
    // the finding count matches the default run exactly; only a valid
    // project with elevated findings gates.
    let elevated = apply_strict_to_verdict(&mut verdict, strict_mode);
    let gated = verdict.status == Status::Valid && elevated > 0;
    if json {
        println!(
            "{}",
            format_lint_json_gated(folder, &verdict, strict_mode, elevated)
        );
    } else {
        for d in &verdict.diagnostics {
            println!("{d}");
        }
        for w in &verdict.variable_warnings {
            println!("{w}");
        }
        print!(
            "{}",
            format_lint_human_gated(folder, &verdict, strict_mode, elevated)
        );
    }
    let code = if gated {
        1
    } else {
        match verdict.status {
            Status::Valid => 0,
            Status::Invalid => 1,
            Status::Unsupported => 2,
        }
    };
    let (level, what) = if gated {
        (
            "warn",
            format!(
                "lint gated by {} ({} elevated)",
                strict_mode.flag(),
                elevated
            ),
        )
    } else {
        match verdict.status {
            Status::Valid => ("info", "lint valid".to_string()),
            Status::Invalid => ("warn", "lint invalid".to_string()),
            Status::Unsupported => ("warn", "lint unsupported".to_string()),
        }
    };
    note_log(
        folder,
        level,
        "lint",
        &run,
        &format!(
            "{what}: {} diagnostic(s), {} finding(s), exit {code}",
            verdict.diagnostics.len(),
            verdict.lint_findings.len()
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

/// Words after `timeline` that name an edit verb (Sprint 15, T007):
/// anything else keeps the Sprint 6 listing parse. A project folder
/// literally named like a verb is unreachable as a listing; prefix
/// it with `./` — the verb still wins, by design.
pub const TIMELINE_VERBS: [&str; 10] = [
    "move",
    "trim",
    "split",
    "delete",
    "set",
    "duplicate",
    "ids",
    "plan",
    "apply",
    "undo",
];

/// True when the word after `timeline` names an edit verb rather than
/// a project folder for the Sprint 6 listing.
pub fn is_timeline_verb(word: &str) -> bool {
    TIMELINE_VERBS.contains(&word)
}

/// Content verbs: the six verbs `plan` and `apply` nest over. `ids`
/// and `undo` stand alone; `plan`/`apply` never nest further.
fn is_content_verb(word: &str) -> bool {
    matches!(
        word,
        "move" | "trim" | "split" | "delete" | "set" | "duplicate"
    )
}

/// CLI-owned undo state for `timeline undo` (Sprint 15, T007): the
/// whole-delta bytes before the last verb application, so one CLI
/// `undo` restores the exact prior effective sources whatever the
/// verb staged (one patch or many). Lives in the hidden project
/// directory next to the SDK delta; render, check, and validation
/// never read it. Single level, exactly like the SDK session's
/// last-verb tracking: a second apply overwrites it, an empty apply
/// clears it, and a successful undo consumes it so a double undo
/// refuses.
const TIMELINE_UNDO_FILE: &str = "timeline-undo.json";

/// Version of the CLI-owned timeline undo record. Another version is
/// unreadable, never guessed: `undo` refuses like a missing record.
const TIMELINE_UNDO_VERSION: u32 = 1;

/// Hidden undo-record path for one project folder.
fn timeline_undo_path_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(TIMELINE_UNDO_FILE)
}

/// Usage error for the timeline verbs (exit 2): malformed flags, not
/// a refused edit. Prints to stderr; the project tree is untouched.
fn timeline_verb_usage(msg: &str) -> i32 {
    eprintln!("framesmith timeline: {msg}.");
    eprintln!(
        "usage:\n\
         \x20 framesmith timeline [--json] [--composition <name>] <folder> [<name>]  (Sprint 6 listing, unchanged)\n\
         \x20 framesmith timeline move --clip <ref> --to <expr> [--composition <name>] [--plan] <folder>\n\
         \x20 framesmith timeline trim --clip <ref> [--start <expr>] [--end <expr>] [--composition <name>] [--plan] <folder>\n\
         \x20 framesmith timeline split --clip <ref> --at <expr> [--composition <name>] [--plan] <folder>\n\
         \x20 framesmith timeline delete [--clip <ref>] [--start <expr> --end <expr>] [--snap <secs>] [--composition <name>] [--plan] <folder>\n\
         \x20 framesmith timeline set --clip <ref> --property label|hidden|start|duration --value <v> [--composition <name>] [--plan] <folder>\n\
         \x20 framesmith timeline duplicate --clip <ref> [--at <expr>] [--new-id <id>] [--composition <name>] [--plan] <folder>\n\
         \x20 framesmith timeline ids [--composition <name>] <folder>\n\
         \x20 framesmith timeline plan <move|trim|split|delete|set|duplicate> <same flags> <folder>\n\
         \x20 framesmith timeline apply <move|trim|split|delete|set|duplicate> <same flags> <folder>\n\
         \x20 framesmith timeline undo <folder>\n\
         \x20 <ref> is a clip `id`, or a document index when fully numeric; <expr> is `+N` seconds, `Nf` frames, `end`, or `after:<ref>`"
    );
    2
}

/// One refused timeline verb (exit 1): the reason text with its
/// reason code in `Display` form (`[E_SDK_*] ...`), on stdout like the
/// listing refusals. SDK refusals arrive pre-coded through their own
/// `Display`; hand-written ones below carry the same `[CODE]` prefix.
/// A refused apply stages nothing, persists nothing, and leaves no
/// receipt; the pending undo record (if any) stays exactly as it was.
fn refuse_timeline_verb(line: &str) -> i32 {
    println!("{line}");
    1
}

/// One `--flag <value>` step: takes the next token as the value. A
/// missing value, or one starting with `-` (another flag), is a usage
/// error. Clip refs starting with `-` are therefore unreachable;
/// `id` values with a leading dash are pathological and unsupported.
fn take_flag_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, i32> {
    *i += 1;
    match args.get(*i) {
        Some(v) if !v.starts_with('-') => Ok(v.clone()),
        _ => Err(timeline_verb_usage(&format!("{flag} needs a value"))),
    }
}

/// Raw content-verb arguments: every flag boxed, presence-tracked so
/// an empty `--value ""` (clearing a label) still counts as given.
#[derive(Debug, Default)]
struct ContentArgs {
    clip: Option<String>,
    to: Option<String>,
    at: Option<String>,
    start: Option<String>,
    end: Option<String>,
    snap: Option<String>,
    property: Option<String>,
    value: Option<String>,
    new_id: Option<String>,
    composition: Option<String>,
    plan: bool,
    folder: Option<String>,
}

/// Parse the flags of one content verb (Sprint 15, T007). `arg0` is
/// the verb word, used only to reject flags the verb does not take.
/// On success the struct still needs per-verb required-flag checks.
fn parse_content_args(verb: &str, args: &[String]) -> Result<ContentArgs, i32> {
    let mut out = ContentArgs::default();
    let set_flag = |slot: &mut Option<String>, flag: &str, v: String| -> Result<(), i32> {
        if slot.is_some() {
            return Err(timeline_verb_usage(&format!("duplicate {flag} flag")));
        }
        *slot = Some(v);
        Ok(())
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--clip" => {
                let v = take_flag_value(args, &mut i, "--clip")?;
                set_flag(&mut out.clip, "--clip", v)?;
            }
            "--to" => {
                let v = take_flag_value(args, &mut i, "--to")?;
                set_flag(&mut out.to, "--to", v)?;
            }
            "--at" => {
                let v = take_flag_value(args, &mut i, "--at")?;
                set_flag(&mut out.at, "--at", v)?;
            }
            "--start" => {
                let v = take_flag_value(args, &mut i, "--start")?;
                set_flag(&mut out.start, "--start", v)?;
            }
            "--end" => {
                let v = take_flag_value(args, &mut i, "--end")?;
                set_flag(&mut out.end, "--end", v)?;
            }
            "--snap" => {
                let v = take_flag_value(args, &mut i, "--snap")?;
                set_flag(&mut out.snap, "--snap", v)?;
            }
            "--property" => {
                let v = take_flag_value(args, &mut i, "--property")?;
                set_flag(&mut out.property, "--property", v)?;
            }
            "--value" => {
                let v = take_flag_value(args, &mut i, "--value")?;
                set_flag(&mut out.value, "--value", v)?;
            }
            "--new-id" => {
                let v = take_flag_value(args, &mut i, "--new-id")?;
                set_flag(&mut out.new_id, "--new-id", v)?;
            }
            "--composition" => {
                let v = take_flag_value(args, &mut i, "--composition")?;
                set_flag(&mut out.composition, "--composition", v)?;
            }
            "--plan" => {
                if out.plan {
                    return Err(timeline_verb_usage("duplicate --plan flag"));
                }
                out.plan = true;
            }
            other if other.starts_with('-') => {
                return Err(timeline_verb_usage(&format!("unknown flag {other:?}")));
            }
            other => {
                if out.folder.is_some() {
                    return Err(timeline_verb_usage(&format!(
                        "expected exactly one <folder>, found extra {other:?}"
                    )));
                }
                out.folder = Some(other.to_string());
            }
        }
        i += 1;
    }
    // Flags the verb does not take are usage errors, catching typos
    // (`move --at` meant `--to`) before anything resolves.
    let reject = |present: bool, flag: &str| -> Result<(), i32> {
        if present {
            return Err(timeline_verb_usage(&format!("{verb} takes no {flag} flag")));
        }
        Ok(())
    };
    match verb {
        "move" => {
            reject(out.at.is_some(), "--at")?;
            reject(out.start.is_some(), "--start")?;
            reject(out.end.is_some(), "--end")?;
            reject(out.snap.is_some(), "--snap")?;
            reject(out.property.is_some(), "--property")?;
            reject(out.value.is_some(), "--value")?;
            reject(out.new_id.is_some(), "--new-id")?;
        }
        "trim" => {
            reject(out.to.is_some(), "--to")?;
            reject(out.at.is_some(), "--at")?;
            reject(out.snap.is_some(), "--snap")?;
            reject(out.property.is_some(), "--property")?;
            reject(out.value.is_some(), "--value")?;
            reject(out.new_id.is_some(), "--new-id")?;
        }
        "split" => {
            reject(out.to.is_some(), "--to")?;
            reject(out.start.is_some(), "--start")?;
            reject(out.end.is_some(), "--end")?;
            reject(out.snap.is_some(), "--snap")?;
            reject(out.property.is_some(), "--property")?;
            reject(out.value.is_some(), "--value")?;
            reject(out.new_id.is_some(), "--new-id")?;
        }
        "delete" => {
            reject(out.to.is_some(), "--to")?;
            reject(out.at.is_some(), "--at")?;
            reject(out.property.is_some(), "--property")?;
            reject(out.value.is_some(), "--value")?;
            reject(out.new_id.is_some(), "--new-id")?;
        }
        "set" => {
            reject(out.to.is_some(), "--to")?;
            reject(out.at.is_some(), "--at")?;
            reject(out.start.is_some(), "--start")?;
            reject(out.end.is_some(), "--end")?;
            reject(out.snap.is_some(), "--snap")?;
            reject(out.new_id.is_some(), "--new-id")?;
        }
        "duplicate" => {
            reject(out.to.is_some(), "--to")?;
            reject(out.start.is_some(), "--start")?;
            reject(out.end.is_some(), "--end")?;
            reject(out.snap.is_some(), "--snap")?;
            reject(out.property.is_some(), "--property")?;
            reject(out.value.is_some(), "--value")?;
        }
        _ => return Err(timeline_verb_usage(&format!("unknown verb {verb:?}"))),
    }
    Ok(out)
}

/// One `--clip` value as an SDK clip reference: fully numeric reads
/// as a document index, anything else as a stable `id` — the same
/// rule the SDK uses for `after:<ref>`.
fn parse_clip_ref(raw: &str) -> ClipRef {
    match raw.parse::<usize>() {
        Ok(index) => ClipRef::by_index(index),
        Err(_) => ClipRef::by_id(raw),
    }
}

/// One parsed time-expression flag: raw text goes to the SDK parser
/// so the refusal (with its reason code) comes from the module that
/// owns the four forms. Returns the exit code on refusal.
fn parse_time_flag(raw: &str) -> Result<sdk::TimeExpr, i32> {
    sdk::parse_time_expr(raw).map_err(|e| refuse_timeline_verb(&e.to_string()))
}

/// Build one content verb from parsed flags: required-flag checks are
/// usage errors (exit 2); unresolvable expressions and incapable
/// properties are SDK refusals (exit 1, no receipt, nothing written).
fn build_content_verb(verb: &str, a: &ContentArgs) -> Result<sdk::TimelineVerb, i32> {
    let need = |slot: &Option<String>, flag: &str| -> Result<String, i32> {
        slot.clone()
            .ok_or_else(|| timeline_verb_usage(&format!("{verb} needs {flag}")))
    };
    match verb {
        "move" => Ok(sdk::TimelineVerb::Move {
            target: parse_clip_ref(&need(&a.clip, "--clip")?),
            to: parse_time_flag(&need(&a.to, "--to")?)?,
        }),
        "trim" => {
            if a.start.is_none() && a.end.is_none() {
                return Err(timeline_verb_usage(
                    "trim needs --start and/or --end (at least one edge)",
                ));
            }
            Ok(sdk::TimelineVerb::Trim {
                target: parse_clip_ref(&need(&a.clip, "--clip")?),
                start: a.start.as_deref().map(parse_time_flag).transpose()?,
                end: a.end.as_deref().map(parse_time_flag).transpose()?,
            })
        }
        "split" => Ok(sdk::TimelineVerb::Split {
            target: parse_clip_ref(&need(&a.clip, "--clip")?),
            at: parse_time_flag(&need(&a.at, "--at")?)?,
        }),
        "delete" => {
            let snap = a
                .snap
                .as_deref()
                .map(|raw| {
                    raw.parse::<f64>().map_err(|_| {
                        timeline_verb_usage("--snap needs a value (tolerance seconds from 0)")
                    })
                })
                .transpose()?;
            let target = a.clip.as_deref().map(parse_clip_ref);
            Ok(sdk::TimelineVerb::Delete {
                target,
                start: a.start.as_deref().map(parse_time_flag).transpose()?,
                end: a.end.as_deref().map(parse_time_flag).transpose()?,
                snap,
            })
        }
        "set" => {
            let property = sdk::parse_set_property(&need(&a.property, "--property")?)
                .map_err(|e| refuse_timeline_verb(&e.to_string()))?;
            let value = need(&a.value, "--value")?;
            Ok(sdk::TimelineVerb::Set {
                target: parse_clip_ref(&need(&a.clip, "--clip")?),
                property,
                value,
            })
        }
        "duplicate" => Ok(sdk::TimelineVerb::Duplicate {
            target: parse_clip_ref(&need(&a.clip, "--clip")?),
            at: a.at.as_deref().map(parse_time_flag).transpose()?,
            new_id: a.new_id.clone(),
        }),
        _ => Err(timeline_verb_usage(&format!("unknown verb {verb:?}"))),
    }
}

/// Open one composition through the SDK for a verb: read-only for
/// plan and ids (no directory is created, nothing is written), writable
/// for apply. The persisted SDK delta replays in both cases, so verbs
/// see every earlier application. Refusals print the SDK message.
fn open_for_verb(
    folder: &Path,
    composition: Option<&str>,
    writable: bool,
) -> Result<SdkSession, i32> {
    SdkSession::open(
        folder,
        OpenOptions {
            persist: writable,
            history: false,
            composition: composition.map(|s| s.to_string()),
            ..OpenOptions::default()
        },
    )
    .map_err(|e| {
        println!("{e}");
        1
    })
}

/// `timeline ids`: list every clip identifier with its window and the
/// `mediaStart` file-to-film mapping. Pure read: opens read-only and
/// writes nothing anywhere.
fn run_timeline_ids(composition: Option<&str>, folder: &Path) -> i32 {
    let session = match open_for_verb(folder, composition, false) {
        Ok(s) => s,
        Err(code) => return code,
    };
    print!("{}", session.timeline_rows_human());
    0
}

/// Plan one content verb (Sprint 15, T007): print only the count
/// (`timeline plan: N clip(s) would change`) and change nothing — the
/// session opens read-only and this path performs no write of any
/// kind: no delta, no undo record, no directory, no log.
fn run_content_plan(verb: sdk::TimelineVerb, composition: Option<&str>, folder: &Path) -> i32 {
    let session = match open_for_verb(folder, composition, false) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match session.timeline_plan(&verb) {
        Ok(plan) => {
            println!("{plan}");
            0
        }
        Err(e) => refuse_timeline_verb(&e.to_string()),
    }
}

/// Read the raw persisted SDK delta bytes, if the file exists. Any
/// other read failure refuses the apply: the pre-verb state could not
/// be snapshotted, so staging must not start.
fn snapshot_delta(folder: &Path) -> Result<Option<String>, i32> {
    let path = folder
        .join(sdk::HISTORY_DIR_NAME)
        .join(sdk::SDK_OVERRIDES_FILE);
    match std::fs::read_to_string(&path) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(refuse_timeline_verb(
            &format!(
                "[E_SDK_UNWRITABLE] timeline refused: the SDK override delta {} cannot be read ({e}); the pre-verb state is unsnapshottable. Nothing was staged, and no receipt was left",
                path.display()
            ),
        )),
    }
}

/// Write the CLI undo record after a successful apply plus persist:
/// the pre-verb delta bytes with the applied count and summary. A
/// second apply overwrites it (single level, like the SDK session).
/// Warns without failing the apply when the record itself is
/// unwritable — the verb already landed; only a later `undo` suffers.
fn store_undo_record(folder: &Path, prev: Option<String>, applied: usize, summary: &str) {
    let path = timeline_undo_path_for(folder);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!(
                "warning: the timeline undo record {} cannot be prepared ({e}); `timeline undo` will be unavailable for this turn.",
                path.display()
            );
            return;
        }
    }
    let record = serde_json::json!({
        "version": TIMELINE_UNDO_VERSION,
        "prev_delta": prev,
        "applied": applied,
        "verb_summary": summary,
    });
    let raw = serde_json::to_string_pretty(&record).unwrap_or_else(|_| "{}".to_string());
    if let Err(e) = std::fs::write(&path, raw) {
        eprintln!(
            "warning: the timeline undo record {} cannot be written ({e}); `timeline undo` will be unavailable for this turn.",
            path.display()
        );
    }
}

/// Drop the pending CLI undo record, if any: an empty apply (nothing
/// staged, like the SDK's zero-length run) leaves no verb to undo, so
/// a later `undo` refuses exactly like the SDK's double undo.
fn clear_undo_record(folder: &Path) {
    let path = timeline_undo_path_for(folder);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!(
            "warning: the timeline undo record {} cannot be cleared ({e}).",
            path.display()
        ),
    }
}

/// Apply one content verb (Sprint 15, T007): stage through the SDK
/// patch machinery, persist the hidden SDK delta (composition sources
/// stay byte-identical), record the pre-verb delta for `undo`, then
/// print the undo receipt and the resulting `mediaStart` rows. A
/// refused apply stages nothing and leaves no receipt; an empty apply
/// (a range touching nothing) clears the pending undo and still
/// reports. Returns the process exit code.
fn run_content_apply(verb: sdk::TimelineVerb, composition: Option<&str>, folder: &Path) -> i32 {
    let mut session = match open_for_verb(folder, composition, true) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let receipt = match session.timeline_apply(verb) {
        Ok(r) => r,
        Err(e) => return refuse_timeline_verb(&e.to_string()),
    };
    if receipt.applied == 0 {
        clear_undo_record(folder);
        println!("{receipt}");
        return 0;
    }
    let prev = match snapshot_delta(folder) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let summary = receipt.verb_summary.clone();
    let applied = receipt.applied;
    if let Err(e) = session.persist() {
        return refuse_timeline_verb(&e.to_string());
    }
    store_undo_record(folder, prev, applied, &summary);
    println!("{receipt}");
    print!("{}", session.timeline_rows_human());
    0
}

/// Read the pending CLI undo record: `None` when no verb application
/// is pending (nothing applied yet, the last run was already undone,
/// or the record is corrupt) — all refuse like the SDK's empty undo.
fn read_undo_record(folder: &Path) -> Option<(Option<String>, usize, String)> {
    let raw = std::fs::read_to_string(timeline_undo_path_for(folder)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if value.get("version")?.as_u64()? != u64::from(TIMELINE_UNDO_VERSION) {
        return None;
    }
    let prev = match value.get("prev_delta")? {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        _ => return None,
    };
    let applied = value.get("applied")?.as_u64()? as usize;
    let summary = value.get("verb_summary")?.as_str()?.to_string();
    Some((prev, applied, summary))
}

/// `timeline undo`: restore the exact pre-verb delta (byte-identical
/// bytes, or a removed delta when none existed) and consume the
/// record, so the effective sources match the pre-verb state and a
/// second undo refuses. With no pending record, refuses with
/// `E_SDK_NOTHING_TO_UNDO` changing nothing.
fn run_timeline_undo(folder: &Path) -> i32 {
    let (prev, applied, summary) = match read_undo_record(folder) {
        Some(r) => r,
        None => {
            return refuse_timeline_verb(
                "[E_SDK_NOTHING_TO_UNDO] timeline refused: undo names no verb application: no mutating verb ran since the last undo. Nothing was reversed",
            );
        }
    };
    let delta_path = folder
        .join(sdk::HISTORY_DIR_NAME)
        .join(sdk::SDK_OVERRIDES_FILE);
    match prev {
        Some(bytes) => {
            if let Some(parent) = delta_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return refuse_timeline_verb(
                        &format!(
                            "[E_SDK_UNWRITABLE] timeline refused: the hidden SDK directory {} cannot be prepared ({e}); the prior sources stay staged. Nothing was reversed",
                            parent.display()
                        ),
                    );
                }
            }
            if let Err(e) = std::fs::write(&delta_path, bytes) {
                return refuse_timeline_verb(
                    &format!(
                        "[E_SDK_UNWRITABLE] timeline refused: the SDK override delta {} cannot be restored ({e}). Nothing was reversed",
                        delta_path.display()
                    ),
                );
            }
        }
        None => match std::fs::remove_file(&delta_path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return refuse_timeline_verb(
                        &format!(
                            "[E_SDK_UNWRITABLE] timeline refused: the SDK override delta {} cannot be removed ({e}). Nothing was reversed",
                            delta_path.display()
                        ),
                    );
            }
        },
    }
    clear_undo_record(folder);
    println!(
        "timeline undo: restored {applied} patch(es): {summary}; sources match the pre-verb state"
    );
    0
}

/// `timeline` edit verbs (Sprint 15, T007 / CAP-006): the human CLI
/// surface over the SDK timeline module. Content verbs apply by
/// default (persisting the hidden SDK delta plus the CLI undo record,
/// printing the undo receipt and the resulting `mediaStart` rows);
/// `--plan` (or the `plan` verb) prints only the count and changes
/// nothing; `ids` lists identifiers; `undo` restores the exact prior
/// sources. `args[0]` is the verb word. Returns the process exit code:
/// 0 applied, 1 refused with a reason code, 2 malformed invocation.
///
/// Write boundary: the only project writes are the hidden SDK delta
/// and the hidden CLI undo record, both under `.framesmith/` and only
/// on explicit persist (apply/undo). Plan, ids, listing, and every
/// refusal write nothing — not even the run log.
pub fn cmd_timeline_verb(args: &[String]) -> i32 {
    let verb = match args.first() {
        Some(v) => v.clone(),
        None => return timeline_verb_usage("expected a verb"),
    };
    match verb.as_str() {
        "ids" => {
            let mut composition: Option<String> = None;
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--composition" => {
                        let v = match take_flag_value(args, &mut i, "--composition") {
                            Ok(v) => v,
                            Err(code) => return code,
                        };
                        if composition.is_some() {
                            return timeline_verb_usage("duplicate --composition flag");
                        }
                        composition = Some(v);
                    }
                    other if other.starts_with('-') => {
                        return timeline_verb_usage(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            return timeline_verb_usage(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => run_timeline_ids(composition.as_deref(), Path::new(&f)),
                None => timeline_verb_usage("expected exactly one <folder>"),
            }
        }
        "undo" => {
            let mut folder: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    other if other.starts_with('-') => {
                        return timeline_verb_usage(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            return timeline_verb_usage(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(other.to_string());
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => run_timeline_undo(Path::new(&f)),
                None => timeline_verb_usage("expected exactly one <folder>"),
            }
        }
        "plan" | "apply" => {
            let nested = match args.get(1) {
                Some(n) => n.clone(),
                None => {
                    return timeline_verb_usage(&format!(
                        "{verb} needs a content verb (one of move, trim, split, delete, set, duplicate)"
                    ));
                }
            };
            if !is_content_verb(&nested) {
                return timeline_verb_usage(&format!(
                    "{verb} needs a content verb (one of move, trim, split, delete, set, duplicate), found {nested:?}"
                ));
            }
            let parsed = match parse_content_args(&nested, &args[2..]) {
                Ok(p) => p,
                Err(code) => return code,
            };
            if parsed.plan {
                return timeline_verb_usage(&format!(
                    "--plan contradicts `{verb}`; use `timeline plan {nested} ...` to preview"
                ));
            }
            let folder = match parsed.folder.clone() {
                Some(f) => f,
                None => return timeline_verb_usage("expected exactly one <folder>"),
            };
            let content = match build_content_verb(&nested, &parsed) {
                Ok(c) => c,
                Err(code) => return code,
            };
            if verb == "plan" {
                run_content_plan(content, parsed.composition.as_deref(), Path::new(&folder))
            } else {
                run_content_apply(content, parsed.composition.as_deref(), Path::new(&folder))
            }
        }
        word if is_content_verb(word) => {
            let parsed = match parse_content_args(word, &args[1..]) {
                Ok(p) => p,
                Err(code) => return code,
            };
            let folder = match parsed.folder.clone() {
                Some(f) => f,
                None => return timeline_verb_usage("expected exactly one <folder>"),
            };
            let content = match build_content_verb(word, &parsed) {
                Ok(c) => c,
                Err(code) => return code,
            };
            if parsed.plan {
                run_content_plan(content, parsed.composition.as_deref(), Path::new(&folder))
            } else {
                run_content_apply(content, parsed.composition.as_deref(), Path::new(&folder))
            }
        }
        other => timeline_verb_usage(&format!("unknown verb {other:?}")),
    }
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

/// Bake the composition-wide grade chain to an equivalent `.cube`
/// (Sprint 11 grade, CAP-002/T003): resolves the selected
/// composition's `grade` (composition level only: lane and clip
/// grades are per-element looks and ride along with their clips,
/// never with the baked file), samples the position-independent
/// color pipeline, and writes a size-33-or-smaller `.cube` that
/// renders pixel-identical (sole-LUT chains bit-exactly) when
/// imported with `lut ... amount=1`. Prints exactly one report line
/// with the grid size and the source chain. An identity chain, a
/// spatial chain (vignette/grain no 3D LUT can store), or an
/// unwritable output refuses non-zero with nothing written.
pub fn cmd_grade_bake(folder: &Path, output: &Path, composition: Option<&str>) -> i32 {
    let verdict = validate_folder(folder);
    if !matches!(verdict.status, Status::Valid) {
        for d in &verdict.diagnostics {
            println!("{d}");
        }
        match verdict.status {
            Status::Invalid => {
                println!(
                    "grade-bake refused: {} is invalid; fix the problems above and re-run. Nothing was written.",
                    folder.display()
                );
                return 1;
            }
            _ => {
                println!(
                    "grade-bake refused: {} uses later-sprint element(s); bake the grade once the project checks green. Nothing was written.",
                    folder.display()
                );
                return 2;
            }
        }
    }
    let comp = match select_composition(&verdict, composition) {
        Ok(c) => c,
        Err(msg) => {
            // `select_composition` words its refusal for render; bake
            // rewords the stage but keeps the missing name and the
            // available names.
            let msg = msg
                .replacen("render refused:", "grade-bake refused:", 1)
                .replacen(
                    "(validation stage); no MP4 was written",
                    "(nothing was written)",
                    1,
                );
            println!("{msg}");
            return 1;
        }
    };
    let mut resolved = notation::model::ResolvedGrade::identity().layer(&comp.grade);
    if resolved.is_empty() {
        println!(
            "grade-bake refused: composition {:?} has no grade content to bake (an identity chain writes no pass-through file); tune a `grade` preset, adjust, detail, or `lut` on the composition and re-run. Nothing was written.",
            comp.name
        );
        return 1;
    }
    if !resolved.vignette.is_empty() || !resolved.grain.is_empty() {
        println!(
            "grade-bake refused: composition {:?} carries vignette or grain, which a `.cube` cannot store (they vary per pixel, not per color); bake a color-only chain instead. Nothing was written.",
            comp.name
        );
        return 1;
    }
    // Materialize the authored LUT, if any: a file that vanished
    // between `check` and this bake refuses here naming the path.
    if resolved.lut.src.is_some() && resolved.lut.amount != 0.0 {
        let src = resolved.lut.src.clone().unwrap_or_default();
        let abs = folder.join(&src);
        if !abs.is_file() {
            println!(
                "grade-bake refused: lut source {src:?} does not exist or is not readable; nothing was written."
            );
            return 1;
        }
        match std::fs::read(&abs) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => match notation::model::parse_cube(&text) {
                    Ok(table) => {
                        resolved.lut.grid = Some(table);
                    }
                    Err(problem) => {
                        println!(
                            "grade-bake refused: lut source {src:?} is unusable: {problem}; nothing was written."
                        );
                        return 1;
                    }
                },
                Err(_) => {
                    println!(
                        "grade-bake refused: lut source {src:?} is not UTF-8 text; nothing was written."
                    );
                    return 1;
                }
            },
            Err(e) => {
                println!(
                    "grade-bake refused: lut source {src:?} cannot be read: {e}; nothing was written."
                );
                return 1;
            }
        }
    }
    // The baked grid: a sole LUT import re-samples exactly on its own
    // lattice (bit-exact on reimport, with a verbatim copy when the
    // blend is full); any tuned chain samples at the 33 maximum for
    // the finest fidelity the format allows.
    let sole_lut = resolved.preset == notation::model::GradePreset::None
        && resolved.exposure == 0.0
        && resolved.contrast == 0.0
        && resolved.saturation == 0.0
        && resolved.temperature == 0.0
        && resolved.tint == 0.0
        && resolved.wheel.is_empty()
        && resolved.curve.is_empty()
        && resolved.hsl.is_empty()
        && resolved.lut.src.is_some()
        && resolved.lut.amount != 0.0
        && resolved.lut.grid.is_some();
    let (size, body): (usize, Option<Vec<u8>>) = if sole_lut && resolved.lut.amount == 1.0 {
        // Verbatim copy: the resolved mapping IS this file's mapping,
        // so re-emitting its bytes is the most faithful bake.
        let src = resolved.lut.src.clone().unwrap_or_default();
        let size = resolved.lut.grid.as_ref().map(|t| t.size).unwrap_or(33);
        match std::fs::read(folder.join(&src)) {
            Ok(bytes) => (size, Some(bytes)),
            Err(e) => {
                println!(
                    "grade-bake refused: lut source {src:?} cannot be read: {e}; nothing was written."
                );
                return 1;
            }
        }
    } else {
        let size = if sole_lut {
            resolved.lut.grid.as_ref().map(|t| t.size).unwrap_or(33)
        } else {
            notation::model::LUT_MAX_GRID_SIZE
        };
        (size, None)
    };
    let bytes: Vec<u8> = match body {
        Some(raw) => raw,
        None => {
            let chain = grade_chain_text(&resolved);
            let mut out = format!(
                "TITLE \"Framesmith grade-bake of composition {:?} ({chain})\"\nLUT_3D_SIZE {size}\nDOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 1.0 1.0 1.0\n",
                comp.name
            );
            let denom = (size - 1) as f64;
            for b in 0..size {
                for g in 0..size {
                    for r in 0..size {
                        let (rr, gg, bb) = resolved.apply(
                            r as f64 * 255.0 / denom,
                            g as f64 * 255.0 / denom,
                            b as f64 * 255.0 / denom,
                        );
                        out.push_str(&format!(
                            "{:.9} {:.9} {:.9}\n",
                            rr as f32 / 255.0,
                            gg as f32 / 255.0,
                            bb as f32 / 255.0
                        ));
                    }
                }
            }
            out.into_bytes()
        }
    };
    // Atomic write: a temp file beside the target, renamed over it,
    // so a failed bake never leaves a partial `.cube` behind.
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                println!(
                    "grade-bake refused: cannot create output directory {}: {e}; nothing was written.",
                    parent.display()
                );
                return 1;
            }
        }
    }
    let tmp = output.with_extension(format!("tmp-cube-{}", std::process::id()));
    let write_out = std::fs::write(&tmp, &bytes).and_then(|()| std::fs::rename(&tmp, output));
    if let Err(e) = write_out {
        // The temp file holds any partial bytes; the target itself is
        // untouched (rename is atomic), so only the temp goes.
        let _ = std::fs::remove_file(&tmp);
        println!(
            "grade-bake refused: cannot write {}: {e}; nothing was written.",
            output.display()
        );
        return 1;
    }
    let chain = grade_chain_text(&resolved);
    println!(
        "baked {size}-point LUT from composition {:?} ({chain}) -> {}.",
        comp.name,
        output.display()
    );
    0
}

/// One-line source-chain summary for the bake report: every
/// non-identity stage of the resolved composition grade.
fn grade_chain_text(resolved: &notation::model::ResolvedGrade) -> String {
    let mut parts: Vec<String> = Vec::new();
    if resolved.preset != notation::model::GradePreset::None {
        parts.push(format!("preset={}", resolved.preset.name()));
    }
    for (key, v) in [
        ("exposure", resolved.exposure),
        ("contrast", resolved.contrast),
        ("saturation", resolved.saturation),
        ("temperature", resolved.temperature),
        ("tint", resolved.tint),
    ] {
        if v != 0.0 {
            parts.push(format!("{key}={v}"));
        }
    }
    if !resolved.wheel.is_empty() {
        parts.push(format!(
            "wheel={:.3},{:.3},{:.3}",
            resolved.wheel.shadows, resolved.wheel.midtones, resolved.wheel.highlights
        ));
    }
    if !resolved.curve.is_empty() {
        let mut chans = Vec::new();
        for (name, pts) in [
            ("red", &resolved.curve.red),
            ("green", &resolved.curve.green),
            ("blue", &resolved.curve.blue),
            ("master", &resolved.curve.master),
        ] {
            if !pts.is_empty() {
                chans.push(format!("{name}:{}pts", pts.len()));
            }
        }
        parts.push(format!("curve={}", chans.join("+")));
    }
    if !resolved.hsl.is_empty() {
        parts.push(format!(
            "hsl={:.1}..{:.1}@{:.3}",
            resolved.hsl.hue_min, resolved.hsl.hue_max, resolved.hsl.lightness
        ));
    }
    if let Some(src) = &resolved.lut.src {
        if resolved.lut.amount != 0.0 {
            parts.push(format!("lut={src:?}@{}", resolved.lut.amount));
        }
    }
    if parts.is_empty() {
        parts.push("identity".to_string());
    }
    parts.join(", ")
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
    /// Per-render variable values (Sprint 9 variables, CAP-003): the
    /// raw `--variables` JSON payload, for example
    /// `'{"title":"Sale"}'`. None renders every binding at its
    /// declared default.
    pub variables: Option<String>,
    /// Strict variable validation (Sprint 9 variables, CAP-004): the
    /// `--strict-variables` flag. Unknown override keys and
    /// declared-but-never-bound variables refuse instead of warning,
    /// and every default-less variable must receive a caller value.
    pub strict_variables: bool,
    /// Release-gate strictness (Sprint 13 check, T007 / CAP-007): the
    /// `--strict` flag. The named warnings become errors and stop the
    /// render before frame one.
    pub strict: bool,
    /// Release-gate strictness (Sprint 13 check, T007 / CAP-007): the
    /// `--strict-all` flag. Every warning becomes an error and stops
    /// the render before frame one.
    pub strict_all: bool,
    /// Theme selection (Sprint 9 variables, CAP-005): the raw
    /// `--theme` name. None selects no caller theme, so each
    /// composition falls back to its own `theme` attribute; the caller
    /// theme beats the composition attribute.
    pub theme: Option<String>,
    /// Batch manifest (Sprint 9 variables, CAP-006): the raw
    /// `--batch` rows-file path. None renders one variant; some
    /// renders one MP4 per rows-file row (see [`cmd_render_batch`]).
    pub batch: Option<String>,
    /// Batch output naming (Sprint 9 variables, CAP-006): the raw
    /// `--name-template` text, for example `"clip-{title}-{index}"`.
    /// None (with `--batch`) names rows `clip-{index}`.
    pub name_template: Option<String>,
    /// Batch job control (Sprint 9 variables, CAP-006): the raw
    /// `--jobs` flag text. None (with `--batch`) renders rows one at
    /// a time, in order.
    pub jobs: Option<String>,
    /// Stop the batch after the first failing row (Sprint 9 variables,
    /// CAP-006): the `--fail-fast` flag. Without it every row is
    /// attempted and the summary reports each row's pass or fail.
    pub fail_fast: bool,
    /// Output destination (Sprint 12 outputs, T001): the raw `--format`
    /// flag text (`mp4`, `hevc`, `webm`, `prores`, `gif`, `pngseq`,
    /// `hls`). None renders the historical H.264 MP4 exactly as before.
    pub format: Option<String>,
    /// ProRes profile (Sprint 12 outputs, T001): the raw
    /// `--prores-profile` flag text (`proxy`, `lt`, `standard`, `hq`,
    /// `4444`). None means `standard`; only `--format prores` honors it.
    pub prores_profile: Option<String>,
    /// Output resolution (Sprint 12 outputs, T002): the raw
    /// `--resolution` flag text (`source`, `1080p`, `720p`, `480p`,
    /// `square-1080`). None means `source` (the canvas renders as
    /// authored, byte-identical).
    pub resolution: Option<String>,
    /// Supersample multiple (Sprint 12 outputs, T002): the raw
    /// `--supersample` flag text (a whole number from 1 to 4). None
    /// means 1 (the identity path with no scaling work).
    pub supersample: Option<String>,
    /// Encoder choice (Sprint 12 outputs, T003): the raw `--encoder`
    /// flag text (`auto`, `software`, `videotoolbox`). None means
    /// `software` (the historical CPU path, byte-identical).
    pub encoder: Option<String>,
    /// Keyframe interval (Sprint 12 outputs, T003): the raw
    /// `--keyint` flag text (a positive whole number of frames).
    /// None emits no `-g`, keeping default bytes identical.
    pub keyint: Option<String>,
    /// Peak bitrate cap (Sprint 12 outputs, T003): the raw
    /// `--maxrate` flag text (for example `8M`). None emits no
    /// `-maxrate`/`-bufsize`, keeping default bytes identical.
    pub maxrate: Option<String>,
    /// HDR mode (Sprint 12 outputs, T004): the raw `--hdr` flag text
    /// (`off`, `pq`, `hlg`, `auto`). None means `auto` (HDR in leads
    /// to HDR out, SDR in to SDR out, decided from source probes).
    pub hdr: Option<String>,
}

/// Flag choices resolved before the first frame (T001/T002, Sprint 12
/// T001): the requested backend, the active preset (`standard` when none
/// is named), at most one encoder override, and the output destination
/// (`mp4` plus the `standard` ProRes profile when none is named).
struct ResolvedRender {
    backend: render::render::Backend,
    preset: render::render::Preset,
    crf: Option<u8>,
    video_bitrate: Option<String>,
    format: render::outputs::OutputFormat,
    prores_profile: render::outputs::ProresProfile,
    resolution: render::scale::Resolution,
    supersample: u8,
    /// Encoder choice (Sprint 12 outputs, T003): `software` when no
    /// `--encoder` flag was passed.
    encoder: render::outputs::EncoderChoice,
    /// Keyframe interval (Sprint 12 outputs, T003): None when no
    /// `--keyint` flag was passed.
    keyint: Option<u32>,
    /// Peak bitrate cap (Sprint 12 outputs, T003): None when no
    /// `--maxrate` flag was passed.
    maxrate: Option<String>,
    /// HDR mode (Sprint 12 outputs, T004): `auto` when no `--hdr`
    /// flag was passed.
    hdr: render::outputs::HdrMode,
    /// Parsed per-render variable values (Sprint 9 variables, CAP-003):
    /// empty when no `--variables` flag was passed.
    variables: validate_mod::VariableOverrides,
    /// Strict variable validation (Sprint 9 variables, CAP-004): true
    /// when `--strict-variables` was passed.
    strict_variables: bool,
    /// Theme selection (Sprint 9 variables, CAP-005): the raw
    /// `--theme` name, or none when the flag was absent.
    theme: Option<String>,
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

/// Resolve the `--format` / `--prores-profile` / `--resolution` /
/// `--supersample` / `--encoder` / `--keyint` / `--maxrate` /
/// `--hdr` destination flags (Sprint 12 outputs, T001/T002/T003/T004):
/// defaults are `mp4` plus the `standard` ProRes profile, the `source`
/// resolution, 1x supersampling (the identity path), the `software`
/// encoder, no rate-control pacing, and the `auto` HDR mode. Every
/// problem is reported at once, each naming the flag and the accepted
/// values; nothing is written on refusal. Shared by the single render
/// and the batch driver (row names need the format's extension before
/// any row starts).
fn resolve_destination(
    req: &RenderRequest,
) -> Result<
    (
        render::outputs::OutputFormat,
        render::outputs::ProresProfile,
        render::scale::Resolution,
        u8,
        render::outputs::EncoderChoice,
        Option<u32>,
        Option<String>,
        render::outputs::HdrMode,
    ),
    String,
> {
    let format = match req.format.as_deref() {
        None => Some(render::outputs::OutputFormat::Mp4),
        Some(raw) => render::outputs::OutputFormat::parse(raw),
    };
    let prores_profile = match req.prores_profile.as_deref() {
        None => Some(render::outputs::ProresProfile::Standard),
        Some(raw) => render::outputs::ProresProfile::parse(raw),
    };
    let resolution = match req.resolution.as_deref() {
        None => Some(render::scale::Resolution::Source),
        Some(raw) => render::scale::Resolution::parse(raw),
    };
    let supersample = match req.supersample.as_deref() {
        None => Some(1u8),
        Some(raw) => render::scale::parse_supersample(raw),
    };
    let encoder = match req.encoder.as_deref() {
        None => Some(render::outputs::EncoderChoice::Software),
        Some(raw) => render::outputs::EncoderChoice::parse(raw),
    };
    let keyint = match req.keyint.as_deref() {
        None => Some(None),
        Some(raw) => render::outputs::parse_keyint(raw).map(Some),
    };
    let maxrate = match req.maxrate.as_deref() {
        None => Some(None),
        Some(raw) => render::outputs::parse_maxrate(raw).map(Some),
    };
    let hdr = match req.hdr.as_deref() {
        None => Some(render::outputs::HdrMode::Auto),
        Some(raw) => render::outputs::HdrMode::parse(raw),
    };
    let mut dest_problems: Vec<String> = Vec::new();
    if format.is_none() {
        dest_problems.push(format!(
            "unknown --format {:?}: expected {}",
            req.format.as_deref().unwrap_or(""),
            render::outputs::OutputFormat::accepted()
        ));
    }
    if prores_profile.is_none() {
        dest_problems.push(format!(
            "unknown --prores-profile {:?}: expected {}",
            req.prores_profile.as_deref().unwrap_or(""),
            render::outputs::ProresProfile::accepted()
        ));
    }
    if resolution.is_none() {
        dest_problems.push(format!(
            "unknown --resolution {:?}: expected {}; pass one of those values to `--resolution`",
            req.resolution.as_deref().unwrap_or(""),
            render::scale::Resolution::accepted()
        ));
    }
    if supersample.is_none() {
        dest_problems.push(format!(
            "invalid --supersample {:?}: expected {}; pass `--supersample 1` (or 2, 3, 4)",
            req.supersample.as_deref().unwrap_or(""),
            render::scale::supersample_accepted()
        ));
    }
    if encoder.is_none() {
        dest_problems.push(format!(
            "unknown --encoder {:?}: expected {}",
            req.encoder.as_deref().unwrap_or(""),
            render::outputs::EncoderChoice::accepted()
        ));
    }
    if keyint.is_none() {
        dest_problems.push(format!(
            "invalid --keyint {:?}: expected {}; pass `--keyint 60` (or another whole number of 1 or more)",
            req.keyint.as_deref().unwrap_or(""),
            render::outputs::keyint_accepted()
        ));
    }
    if maxrate.is_none() {
        dest_problems.push(format!(
            "invalid --maxrate {:?}: expected {}; pass `--maxrate 8M`",
            req.maxrate.as_deref().unwrap_or(""),
            render::outputs::maxrate_accepted()
        ));
    }
    if hdr.is_none() {
        dest_problems.push(format!(
            "unknown --hdr {:?}: expected {}",
            req.hdr.as_deref().unwrap_or(""),
            render::outputs::HdrMode::accepted()
        ));
    }
    if let (Some(f), Some(_)) = (format, prores_profile) {
        if req.prores_profile.is_some() && f != render::outputs::OutputFormat::Prores {
            dest_problems.push(format!(
                "--prores-profile {:?} needs --format prores (got --format {:?}); pass --format prores or drop --prores-profile",
                req.prores_profile.as_deref().unwrap_or(""),
                f.name()
            ));
        }
    }
    if !dest_problems.is_empty() {
        return Err(format!(
            "render refused: {} (validation stage); no output was written",
            dest_problems.join("; ")
        ));
    }
    Ok((
        format.unwrap_or(render::outputs::OutputFormat::Mp4),
        prores_profile.unwrap_or(render::outputs::ProresProfile::Standard),
        resolution.unwrap_or(render::scale::Resolution::Source),
        supersample.unwrap_or(1),
        encoder.unwrap_or(render::outputs::EncoderChoice::Software),
        keyint.unwrap_or(None),
        maxrate.unwrap_or(None),
        hdr.unwrap_or(render::outputs::HdrMode::Auto),
    ))
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
    // Sprint 9 variables (CAP-003): the `--variables` payload parses to
    // the per-render map here, before the first frame. A misshapen
    // payload refuses naming the flag and the expected shape, with no
    // MP4 written.
    let variables = match &req.variables {
        None => validate_mod::VariableOverrides::new(),
        Some(raw) => validate_mod::parse_variable_overrides(raw)
            .map_err(|e| format!("render refused: {e} (validation stage); no MP4 was written"))?,
    };
    // Sprint 12 outputs (T001/T002/T003/T004): the destination
    // flags validate together before the first frame and report every
    // problem at once. Each problem names the flag and the accepted
    // values; nothing is written on refusal.
    let (format, prores_profile, resolution, supersample, encoder, keyint, maxrate, hdr) =
        resolve_destination(req)?;
    Ok(ResolvedRender {
        backend,
        preset,
        crf,
        video_bitrate,
        format,
        prores_profile,
        resolution,
        supersample,
        encoder,
        keyint,
        maxrate,
        hdr,
        variables,
        strict_variables: req.strict_variables,
        theme: req.theme.clone(),
    })
}

/// Short display names of probed HDR sources (Sprint 12 outputs,
/// T004): file names in document order, so refusal and report lines
/// name the cause without leaking absolute work paths.
fn hdr_source_names(
    sources: &[(PathBuf, render::outputs::HdrTransfer)],
    want: Option<render::outputs::HdrTransfer>,
) -> String {
    sources
        .iter()
        .filter(|(_, t)| want.is_none_or(|w| *t == w))
        .map(|(p, t)| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string());
            format!("{name:?} ({})", t.probe_token())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolve the `--hdr` mode against the probed sources (Sprint 12
/// outputs, T004): `auto` renders HEVC Main10 BT.2020 when a source
/// carries PQ/HLG and SDR otherwise; explicit `pq`/`hlg` demand that
/// transfer; HDR sources rendering to SDR-only destinations tone-map
/// under `auto`. Returns the effective transfer, whether the SDR
/// pictures are a tone map, and the report note naming the decision —
/// or a refusal naming the cause with a concrete fix, before frame
/// one with nothing written.
fn resolve_hdr_decision(
    mode: render::outputs::HdrMode,
    format: render::outputs::OutputFormat,
    prep: &render::render::Prepared,
    hdr_flag_passed: bool,
) -> Result<(render::outputs::HdrTransfer, bool, Option<String>), String> {
    use render::outputs::{HdrMode, HdrTransfer};
    let sources = prep.hdr_sources();
    let detected = prep.composition_transfer();
    // An explicit demand names its transfer and the probe token, so
    // the fix is concrete even for makers who never saw the probe.
    let demand_refusal = |want: HdrTransfer| {
        let cause = if sources.is_empty() {
            "the composition's video sources are all SDR (no source carries PQ or HLG)".to_string()
        } else {
            format!(
                "the composition's HDR source(s) carry {} only ({})",
                if want == HdrTransfer::Pq { "HLG" } else { "PQ" },
                hdr_source_names(&sources, None)
            )
        };
        format!(
            "render refused: --hdr {} demands a {} ({}) source, but {cause}; add a {} source or re-run with --hdr auto (HDR in leads to HDR out, SDR in to SDR out) or --hdr off for SDR (validation stage); no output was written",
            want.name(),
            if want == HdrTransfer::Pq {
                "PQ"
            } else {
                "HLG"
            },
            want.probe_token(),
            want.name(),
        )
    };
    let format_refusal = || {
        format!(
            "render refused: --hdr {} needs a format that carries HEVC Main10 BT.2020 (mp4, hevc, or hls), but --format {} cannot carry HDR; re-run with --format hevc for HEVC Main10 BT.2020 (or --hdr off for SDR) (validation stage); no output was written",
            mode.name(),
            format.name(),
        )
    };
    match mode {
        HdrMode::Off => {
            if sources.is_empty() {
                Ok((HdrTransfer::Sdr, false, None))
            } else {
                Ok((
                    HdrTransfer::Sdr,
                    false,
                    Some(format!(
                        "stage output: note: --hdr off forced SDR; {} HDR source(s) ({}) render as SDR.",
                        sources.len(),
                        hdr_source_names(&sources, None)
                    )),
                ))
            }
        }
        HdrMode::Pq | HdrMode::Hlg => {
            let want = if mode == HdrMode::Pq {
                HdrTransfer::Pq
            } else {
                HdrTransfer::Hlg
            };
            if !sources.iter().any(|(_, t)| *t == want) {
                return Err(demand_refusal(want));
            }
            if !render::outputs::format_carries_hdr(format) {
                return Err(format_refusal());
            }
            Ok((
                want,
                false,
                Some(format!(
                    "stage output: note: --hdr {} matched {} in {}; rendering HEVC Main10 BT.2020 ({}).",
                    mode.name(),
                    if want == HdrTransfer::Pq { "PQ" } else { "HLG" },
                    hdr_source_names(&sources, Some(want)),
                    want.name(),
                )),
            ))
        }
        HdrMode::Auto => {
            if detected == HdrTransfer::Sdr {
                // A bare `auto` resolving to SDR stays silent, so the
                // pinned lines read exactly as before; an explicit
                // `--hdr auto` still reports the decision it drove.
                let note = hdr_flag_passed.then(|| {
                    "stage output: note: --hdr auto found SDR-only sources; rendering SDR."
                        .to_string()
                });
                Ok((HdrTransfer::Sdr, false, note))
            } else if render::outputs::format_carries_hdr(format) {
                Ok((
                    detected,
                    false,
                    Some(format!(
                        "stage output: note: --hdr auto detected {} ({}) in {}; rendering HEVC Main10 BT.2020 ({}).",
                        if detected == HdrTransfer::Pq {
                            "PQ"
                        } else {
                            "HLG"
                        },
                        detected.probe_token(),
                        hdr_source_names(&sources, Some(detected)),
                        detected.name(),
                    )),
                ))
            } else if render::outputs::format_tone_maps_hdr(format) {
                // The drawn SDR pictures are the tone map: the decoder
                // and the CPU blitter already composite HDR video into
                // the SDR canvas, so the GIF/PNG animation is SDR by
                // nature and the report names the mapping.
                Ok((
                    HdrTransfer::Sdr,
                    true,
                    Some(format!(
                        "stage output: note: --hdr auto detected {} ({}) in {}, but --format {} is SDR-only; rendering a tone-mapped SDR {} animation.",
                        if detected == HdrTransfer::Pq {
                            "PQ"
                        } else {
                            "HLG"
                        },
                        detected.probe_token(),
                        hdr_source_names(&sources, Some(detected)),
                        format.name(),
                        format.name(),
                    )),
                ))
            } else {
                Err(format!(
                    "render refused: --hdr auto detected {} ({}) in {}, but --format {} cannot carry HDR; re-run with --format hevc for HEVC Main10 BT.2020 (or --hdr off for SDR) (validation stage); no output was written",
                    if detected == HdrTransfer::Pq {
                        "PQ"
                    } else {
                        "HLG"
                    },
                    detected.probe_token(),
                    hdr_source_names(&sources, Some(detected)),
                    format.name(),
                ))
            }
        }
    }
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
    /// Output destination (Sprint 12 outputs, T001): the `--format`
    /// value (`mp4` for historical renders).
    format: &'a str,
    /// Output scaling (Sprint 12 outputs, T002): the `--resolution`
    /// value (`source` for canvas-size renders) and the
    /// `--supersample` multiple (1 on the identity path).
    resolution: &'a str,
    supersample: u8,
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
        "{{\"tool\":\"framesmith\",\"command\":\"render\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"backend\":\"{backend}\",\"preset\":\"{preset}\",\"format\":\"{}\",\"resolution\":\"{}\",\"supersample\":{},\"crf\":{},\"x264_preset\":",
        json_escape(&ctx.folder.display().to_string()),
        json_escape(ctx.format),
        json_escape(ctx.resolution),
        ctx.supersample,
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
    // Sprint 12 outputs (T003): the requested encoder, the effective
    // hardware use, and the rate-control pacing of the render.
    let _ = write!(
        out,
        ",\"encoder\":\"{}\",\"hardware\":{}",
        ctx.enc.encoder_choice.name(),
        ctx.enc.hardware
    );
    let _ = write!(out, ",\"keyint\":");
    match ctx.enc.keyint {
        Some(g) => {
            let _ = write!(out, "{g}");
        }
        None => out.push_str("null"),
    }
    let _ = write!(out, ",\"maxrate\":");
    match &ctx.enc.maxrate {
        Some(m) => {
            let _ = write!(out, "\"{}\"", json_escape(m));
        }
        None => out.push_str("null"),
    }
    // Sprint 12 outputs (T004): the requested HDR mode and the
    // effective transfer the probe drove (`sdr` on the default path).
    let _ = write!(
        out,
        ",\"hdr\":\"{}\",\"transfer\":\"{}\"",
        ctx.enc.hdr.name(),
        ctx.enc.hdr_transfer.name()
    );
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

/// Staged mix path for one destination (Sprint 12 outputs, T001):
/// files stage beside the target as before; sequences stage beside
/// their temp directory (not inside it: `render_staged` clears the
/// temp directory before encoding, which would delete the mix). The
/// final destination stays untouched until the finalize step moves
/// verified results into place.
fn mix_path_for_dest(dest: &render::outputs::ResolvedOutput) -> PathBuf {
    match dest {
        render::outputs::ResolvedOutput::File { path } => render::render::mix_staging_path(path),
        render::outputs::ResolvedOutput::PngSeq { dir }
        | render::outputs::ResolvedOutput::Hls { dir, .. } => {
            let mut mix = render::outputs::staging_dir(dir).into_os_string();
            mix.push(".mix.pcm");
            PathBuf::from(mix)
        }
    }
}

/// Nearest existing ancestor of `path` (Sprint 12 outputs, T001): the
/// path itself when it exists, else the closest parent that does, else
/// the working directory. Sequence preflight probes writability here
/// without creating the final destination early.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut cur: &Path = path;
    loop {
        if cur.exists() {
            return cur.to_path_buf();
        }
        match cur.parent() {
            Some(p) if !p.as_os_str().is_empty() => cur = p,
            _ => return PathBuf::from("."),
        }
    }
}

/// Output preflight for one destination (Sprint 12 outputs, T001):
/// files keep the historical create-plus-probe discipline verbatim;
/// sequences probe the nearest existing ancestor without creating the
/// final directory, refusing before frame one naming the path with no
/// partial output left behind.
fn preflight_dest(dest: &render::outputs::ResolvedOutput) -> Result<(), String> {
    match dest {
        render::outputs::ResolvedOutput::File { path } => {
            let probe_dir: PathBuf = match path.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                _ => PathBuf::from("."),
            };
            if let Err(e) = std::fs::create_dir_all(&probe_dir) {
                return Err(format!(
                    "render refused: cannot create output directory {}: {e} (output stage). No MP4 was written.",
                    probe_dir.display()
                ));
            }
            let probe = probe_dir.join(".framesmith-write-probe");
            match std::fs::write(&probe, b"framesmith writability probe") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&probe);
                    Ok(())
                }
                Err(e) => Err(format!(
                    "render refused: output directory {} is not writable: {e} (output stage). No MP4 was written.",
                    probe_dir.display()
                )),
            }
        }
        render::outputs::ResolvedOutput::PngSeq { dir }
        | render::outputs::ResolvedOutput::Hls { dir, .. } => {
            let anchor = nearest_existing_ancestor(dir);
            let probe = anchor.join(".framesmith-write-probe");
            match std::fs::write(&probe, b"framesmith writability probe") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&probe);
                    Ok(())
                }
                Err(e) => Err(format!(
                    "render refused: output directory {} is not writable: {e} (output stage). No output was written.",
                    dir.display()
                )),
            }
        }
    }
}

/// Report an output-stage refusal (Sprint 12 outputs, T001): the staged
/// line, the run log, and the machine-readable document share the same
/// message; nothing was written.
#[allow(clippy::too_many_arguments)]
fn output_stage_refusal(
    folder: &Path,
    json: bool,
    run: &str,
    backend: &str,
    preset: &str,
    enc: &render::render::ResolvedEncoder,
    format: &str,
    resolution: &str,
    supersample: u8,
    stages: &[render::render::StageTiming],
    total_start: std::time::Instant,
    msg: &str,
) -> i32 {
    stage_line(json, &format!("stage output: failed: {msg}"));
    note_log(folder, "error", "render", run, msg, json);
    if json {
        println!(
            "{}",
            format_render_json(&RenderJson {
                folder,
                status: "failed",
                exit: 1,
                backend,
                preset,
                format,
                resolution,
                supersample,
                enc,
                output: "",
                message: msg,
                cache_hits: 0,
                cache_misses: 0,
                peaks: None,
                normalize: None,
                diagnostics: &[],
                stages,
                total_ms: total_start.elapsed().as_millis() as u64
            })
        );
    } else {
        println!("{msg}");
    }
    1
}

/// `render`: revalidate, select the backend, report FFmpeg state, build
/// the 48 kHz mix, draw frames (possibly in parallel), encode atomically,
/// and report staged progress with measured per-stage timings (T001,
/// T002, T003, T006, T008). Every run appends its notable events to the
/// run log. Refusals are logged too: no verdict ever skips the log.
pub fn cmd_render_full(folder: &Path, output: Option<&Path>, req: &RenderRequest) -> i32 {
    // Storyboard approval gate (Sprint 20, T004 / CAP-004, REC-002):
    // while a presented board awaits maker approval, the build is
    // refused pointing at the pending gate — before batch routing,
    // validation, or any frame renders, and with no MP4 written.
    // Projects without a board behave exactly as before.
    if let Some(refusal) = preview::studio_stills::gate_refusal(folder, "render") {
        let run = log::run_id();
        stage_line(req.json, &refusal);
        note_log(folder, "warn", "render", &run, &refusal, req.json);
        if req.json {
            let enc = render::render::RenderOptions::default().encoder();
            println!(
                "{}",
                format_render_json(&RenderJson {
                    folder,
                    status: "invalid",
                    exit: 1,
                    backend: "unknown",
                    preset: "unknown",
                    format: "unknown",
                    resolution: "unknown",
                    supersample: 0,
                    enc: &enc,
                    output: "",
                    message: &refusal,
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
    // Sprint 9 variables (CAP-006): a rows manifest renders one MP4
    // per row through the batch driver below; every other flag rides
    // along into each row render.
    if req.batch.is_some() {
        return cmd_render_batch(folder, output, req);
    }
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
                        format: "unknown",
                        resolution: "unknown",
                        supersample: 0,
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
        format: resolved.format,
        prores_profile: resolved.prores_profile,
        resolution: resolved.resolution,
        supersample: resolved.supersample,
        encoder_choice: resolved.encoder,
        hardware: false,
        hw_codec: None,
        keyint: resolved.keyint,
        maxrate: resolved.maxrate.clone(),
        hdr: resolved.hdr,
        hdr_transfer: render::outputs::HdrTransfer::Sdr,
        hdr_tone_mapped: false,
        provenance: None,
        gif_loop: 0,
    };
    // `auto` resolves through the capability probe (T001): the GPU
    // renderer when one is available, else the CPU renderer. The chosen
    // backend is named in the output and the run log.
    opts.backend = render::render::resolve_backend(resolved.backend);
    // Sprint 12 outputs (T001): the destination resolves from
    // `--format` and `-o` before anything renders. Files keep the
    // historical `-o`-as-given semantics; sequences resolve to a
    // directory (PNG) or a playlist-plus-segments directory (HLS).
    let dest = render::outputs::resolve_output(resolved.format, folder, output);

    stage_line(json, "stage validate: running");
    let validate_start = Instant::now();
    // Sprint 9 variables (CAP-003/CAP-004/CAP-005): caller overrides
    // resolve in memory for this render only; the project on disk is
    // never written. Under `--strict-variables` unknown keys, unused
    // declarations, and valueless default-less variables refuse here.
    // The caller theme (`--theme`) beats each composition `theme`
    // attribute; unknown names on either side refuse here.
    let mut verdict = validate_folder_with_variables_strict_themed(
        folder,
        &resolved.variables,
        resolved.strict_variables,
        resolved.theme.as_deref(),
    );
    let validate_ms = validate_start.elapsed().as_millis() as u64;
    // Sprint 13 check (T007 / CAP-007): the release gate. Strictness
    // never invents findings; it only elevates the validated batch.
    // `--strict-all` wins when both flags reach this layer, though
    // the CLI refuses the combination with exit 2 before it can.
    let strict_mode = if req.strict_all {
        lint::StrictMode::All
    } else if req.strict {
        lint::StrictMode::Named
    } else {
        lint::StrictMode::Off
    };
    let elevated = apply_strict_to_verdict(&mut verdict, strict_mode);
    let gated = verdict.status == Status::Valid && elevated > 0;
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
                        format: opts.format.name(),
                        resolution: opts.resolution.name(),
                        supersample: opts.supersample,
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
                        format: opts.format.name(),
                        resolution: opts.resolution.name(),
                        supersample: opts.supersample,
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
            // Sprint 13 check (T007 / REC-004): the strict gate stops
            // before frame one, listing each elevated finding with
            // its rule and fix; nothing is rendered and no output is
            // written.
            if gated {
                let rules = elevated_rule_names(&verdict).join(", ");
                let msg = format!(
                    "render refused before the first frame: {} elevated {elevated} finding(s) ({rules}) to errors on a valid project; fix the findings above or re-run without {} (warnings do not block). Nothing was rendered and no output was written.",
                    strict_mode.flag(),
                    strict_mode.flag()
                );
                push_stage(
                    &mut stages,
                    render::render::StageTiming::failed(
                        "validate",
                        validate_ms,
                        format!("{elevated} strict finding(s)"),
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
                            format: opts.format.name(),
                            resolution: opts.resolution.name(),
                            supersample: opts.supersample,
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
                    for f in &verdict.lint_findings {
                        if f.severity == lint::LintSeverity::Error {
                            println!("{f}");
                        }
                    }
                    println!("{msg}");
                }
                return 1;
            }
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
                    format: opts.format.name(),
                    resolution: opts.resolution.name(),
                    supersample: opts.supersample,
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
    // Sprint 9 variables (CAP-003/CAP-004): unknown `--variables` keys
    // and unused declarations warn visibly without blocking, in both
    // human and `--json` runs (the JSON report carries the same
    // warnings machine-readably).
    for w in &verdict.variable_warnings {
        let note = format!("stage validate: note: {w}");
        stage_line(json, &note);
        note_log(folder, "info", "render", &run, &note, json);
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
    // Sprint 24 slideshow (T003 / CAP-004): rendering the deck
    // renders only the first slide — with the film's validation
    // gates applied and byte parity to a direct render of that
    // slide's composition. No linear deck export exists.
    // When the user names `--composition` explicitly, that wins;
    // otherwise a deck renders its first slide.
    let comp = match req.composition.as_deref() {
        None if verdict.slideshow.is_some() => {
            let deck = verdict.slideshow.as_ref().unwrap();
            let first = deck.first_slide().expect("validated deck has a slide");
            match verdict
                .compositions
                .iter()
                .find(|c| c.name == first.composition || c.id.as_deref() == Some(&first.composition))
            {
                Some(c) => c.clone(),
                None => {
                    let msg = format!(
                        "render refused: slide {:?} references unknown composition {:?}; no MP4 was written",
                        first.id, first.composition
                    );
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
                                format: opts.format.name(),
                                resolution: opts.resolution.name(),
                                supersample: opts.supersample,
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
            }
        }
        _ => match select_composition(&verdict, req.composition.as_deref()) {
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
                                format: opts.format.name(),
                                resolution: opts.resolution.name(),
                                supersample: opts.supersample,
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
            },
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
                        format: opts.format.name(),
                        resolution: opts.resolution.name(),
                        supersample: opts.supersample,
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
        // Sprint 12 outputs (T001): the historical MP4 note stays
        // byte-identical; other destinations name their own output.
        let note = match resolved.format {
            render::outputs::OutputFormat::Mp4 => format!(
                "stage validate: note: rendering sub-range {:.3}s..{:.3}s of composition {:?} as its own MP4.",
                r.start, r.end, comp.name
            ),
            f => format!(
                "stage validate: note: rendering sub-range {:.3}s..{:.3}s of composition {:?} as its own {} output.",
                r.start,
                r.end,
                comp.name,
                f.name()
            ),
        };
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
            &dest,
            opts.format.name(),
            opts.resolution.name(),
            opts.supersample,
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
                        format: opts.format.name(),
                        resolution: opts.resolution.name(),
                        supersample: opts.supersample,
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

    // Sprint 12 outputs (T001): the host FFmpeg must provide the
    // destination's video encoder beyond the Sprint 1 pair, or the
    // render refuses before frame one naming the format, the missing
    // encoder, and the fix.
    if let Err(msg) = render::outputs::check_format_encoders(&ffmpeg_bin, resolved.format) {
        return output_stage_refusal(
            folder,
            json,
            &run,
            opts.backend.name(),
            opts.preset.name(),
            &opts.encoder(),
            opts.format.name(),
            opts.resolution.name(),
            opts.supersample,
            &stages,
            total_start,
            &msg,
        );
    }
    // Sprint 12 outputs (T003; Sprint 25, T002): resolve the
    // `--encoder` choice against the host FFmpeg before the first
    // frame. `auto` takes the first hardware encoder for this OS
    // the host provides and software otherwise without failing
    // (the fallback fact named); an explicit hardware choice
    // refuses naming the choice and codec, and nothing is written
    // on refusal.
    match resolved.encoder {
        render::outputs::EncoderChoice::Software => {}
        render::outputs::EncoderChoice::Auto => {
            match render::outputs::resolve_auto(&ffmpeg_bin, resolved.format) {
                Some((codec, _choice)) => {
                    opts.hardware = true;
                    opts.hw_codec = Some(codec);
                    let note = format!(
                        "stage output: note: --encoder auto selected hardware encoding ({codec}); software encodes stay byte-identical with --encoder software."
                    );
                    stage_line(json, &note);
                    note_log(folder, "info", "render", &run, &note, json);
                }
                None => {
                    let note = "stage output: note: --encoder auto selected software encoding (no hardware encoder on this machine).";
                    stage_line(json, note);
                    note_log(folder, "info", "render", &run, note, json);
                }
            }
        }
        choice => {
            match render::outputs::check_explicit_encoder(
                &ffmpeg_bin,
                resolved.format,
                choice,
            ) {
                Err(msg) => {
                    return output_stage_refusal(
                        folder,
                        json,
                        &run,
                        opts.backend.name(),
                        opts.preset.name(),
                        &opts.encoder(),
                        opts.format.name(),
                        opts.resolution.name(),
                        opts.supersample,
                        &stages,
                        total_start,
                        &msg,
                    );
                }
                Ok(codec) => {
                    opts.hardware = true;
                    opts.hw_codec = Some(codec);
                    let note = format!(
                        "stage output: note: encoding with {codec} (hardware); decodability is verified, not byte-identity."
                    );
                    stage_line(json, &note);
                    note_log(folder, "info", "render", &run, &note, json);
                }
            }
        }
    }
    // T004 output preflight (output stage, before the first frame),
    // extended to every destination (Sprint 12 outputs, T001): files
    // keep the historical create-plus-probe discipline; sequences
    // probe writability without creating the final directory. Any
    // failure refuses with the path named and writes nothing — never a
    // partial file, playlist, or frame.
    if let Err(msg) = preflight_dest(&dest) {
        return output_stage_refusal(
            folder,
            json,
            &run,
            opts.backend.name(),
            opts.preset.name(),
            &opts.encoder(),
            opts.format.name(),
            opts.resolution.name(),
            opts.supersample,
            &stages,
            total_start,
            &msg,
        );
    }

    let (workers, workers_note) = worker_note();
    if let Some(note) = &workers_note {
        stage_line(json, note);
        note_log(folder, "warn", "render", &run, note, json);
    }

    // Sprint 12 outputs (T002): the scale plan derives from the
    // selected composition's canvas and the `--resolution` /
    // `--supersample` flags. `source` at 1x is the identity path (the
    // composition renders untouched, byte-identical); anything else
    // scales the composition to the internal size once, so every
    // downstream step — motion, shaping, describe, drawing — runs at
    // the higher resolution unchanged.
    let scale = opts.scale_plan(comp.width, comp.height);
    let scaled_comp = render::scale::scale_composition(&comp, scale);
    let scaled_all: Vec<Composition> = verdict
        .compositions
        .iter()
        .map(|c| render::scale::scale_composition(c, scale))
        .collect();
    if !scale.identity {
        let note = format!(
            "stage validate: note: resolution {} supersample {}x: rendering {}x{} internally, delivering {}x{}.",
            resolved.resolution.name(),
            resolved.supersample,
            scale.internal_w,
            scale.internal_h,
            scale.out_w,
            scale.out_h
        );
        stage_line(json, &note);
        note_log(folder, "info", "render", &run, &note, json);
    }

    // Frames stage, first half: prepare every picture input before the
    // first frame (fonts, video probes and decodes, images).
    let frames_start = Instant::now();
    // Sprint 12 outputs (T003): the encoder and rate-control tail
    // rides the frames-stage line only when paced or off-default, so
    // default lines stay exactly as before.
    let frames_extra = opts
        .announce_extra()
        .map(|x| format!(", {x}"))
        .unwrap_or_default();
    stage_line(
        json,
        &format!(
            "stage frames: running (backend={}, {}{}, workers={workers})",
            opts.backend.name(),
            opts.announce(),
            frames_extra
        ),
    );
    // Sprint 6 T008: a sub-range prepares exactly its own window (its
    // frame `k` is the range start plus `k` frame intervals); a full
    // render prepares the whole composition exactly as before.
    // Sprint 12 outputs (T002): preparation consumes the scaled
    // composition (the identity path scales nothing, so the bytes
    // match the unscaled render exactly).
    let mut prep = match render::render::prepare_frames_ranged(
        &scaled_comp,
        &scaled_all,
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
                    &dest,
                    opts.format.name(),
                    opts.resolution.name(),
                    opts.supersample,
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
                        format: opts.format.name(),
                        resolution: opts.resolution.name(),
                        supersample: opts.supersample,
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
            &dest,
            opts.format.name(),
            opts.resolution.name(),
            opts.supersample,
            "frames",
            &mut stages,
            total_start,
            ms,
            "",
        );
    }
    // Sprint 12 outputs (T002): expression-driven geometry evaluates
    // per frame, so its pixel-valued results ride the internal
    // multiplier from here (1.0 on the identity path: a no-op).
    prep.scale_expr_results(scale.expr_scale());
    // Sprint 12 outputs (T004): the HDR decision resolves from the
    // requested mode plus the probed source transfers, before the
    // first frame and before any output exists. A refusal here names
    // the cause with a concrete fix and writes nothing — the staged
    // mix and the encode target are both created later.
    match resolve_hdr_decision(resolved.hdr, resolved.format, &prep, req.hdr.is_some()) {
        Ok((transfer, tone_mapped, note)) => {
            opts.hdr_transfer = transfer;
            opts.hdr_tone_mapped = tone_mapped;
            // HDR always encodes software (HEVC Main10): the hardware
            // encoders tune SDR, so an `auto` hardware pick steps
            // aside with a note instead of failing the render.
            if transfer != render::outputs::HdrTransfer::Sdr && opts.hardware {
                opts.hardware = false;
                opts.hw_codec = None;
                let hw_note = "stage output: note: HDR renders on software (HEVC Main10 BT.2020); the hardware encoder steps aside for this render.";
                stage_line(json, hw_note);
                note_log(folder, "info", "render", &run, hw_note, json);
            }
            if let Some(note) = note {
                stage_line(json, &note);
                note_log(folder, "info", "render", &run, &note, json);
            }
        }
        Err(msg) => {
            return output_stage_refusal(
                folder,
                json,
                &run,
                opts.backend.name(),
                opts.preset.name(),
                &opts.encoder(),
                opts.format.name(),
                opts.resolution.name(),
                opts.supersample,
                &stages,
                total_start,
                &msg,
            );
        }
    }
    // Sprint 12 outputs (T005): every render stamps the tool name, the
    // version, and the digest of the composition inputs. File
    // containers embed the stamp as metadata (already in the encode
    // argv via `opts.encoder()`); the PNG sidecar, the HLS playlist
    // comment, and the GIF warning land after the encode succeeds, so
    // provenance never blocks pixels.
    let digest = render::outputs::composition_inputs_digest(&verdict.compositions, folder);
    opts.provenance = Some(render::outputs::Provenance {
        version: env!("CARGO_PKG_VERSION").to_string(),
        digest,
    });
    let prepared_note = format!("prepared {} frame(s)", prep.frames());
    // Audio stage: smoke-decode video sound and build the Framesmith
    // 48 kHz stereo mix of the `audio` tracks (T008).
    let audio_start = Instant::now();
    stage_line(json, "stage audio: running");
    let mix_path = mix_path_for_dest(&dest);
    let plan =
        match render::render::build_audio_plan(&scaled_comp, folder, &ffmpeg_bin, &prep, &mix_path)
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
                        &dest,
                        opts.format.name(),
                        opts.resolution.name(),
                        opts.supersample,
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
                            format: opts.format.name(),
                            resolution: opts.resolution.name(),
                            supersample: opts.supersample,
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
            &dest,
            opts.format.name(),
            opts.resolution.name(),
            opts.supersample,
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
                        format: opts.format.name(),
                        resolution: opts.resolution.name(),
                        supersample: opts.supersample,
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
                            format: opts.format.name(),
                            resolution: opts.resolution.name(),
                            supersample: opts.supersample,
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
        Some(mix) => summarize_peaks(mix, &scaled_comp, &verdict.silences, start_offset),
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
        match render::render::render_staged(&prep, &plan, &opts, &ffmpeg_bin, &dest, &scale) {
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
                        &dest,
                        opts.format.name(),
                        opts.resolution.name(),
                        opts.supersample,
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
                            format: opts.format.name(),
                            resolution: opts.resolution.name(),
                            supersample: opts.supersample,
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
    // Sprint 12 outputs (T005): the stamp lands after the encode
    // succeeds. PNG sequences gain a sibling sidecar JSON file with
    // the same fields (the frames directory itself keeps exactly the
    // numbered frames), HLS gains a playlist comment, and GIF — which
    // carries neither metadata nor sidecar — warns naming the format.
    // None of this touches pixels: a landing failure warns and still
    // exits 0.
    if let Some(prov) = &opts.provenance {
        match &dest {
            render::outputs::ResolvedOutput::PngSeq { dir } => {
                let sidecar = render::outputs::provenance_sidecar_for_seq(dir);
                if let Err(e) = std::fs::write(&sidecar, prov.sidecar_json()) {
                    let note = format!(
                        "stage output: note: provenance: sidecar {} could not be written: {e}; pictures are unchanged.",
                        sidecar.display()
                    );
                    stage_line(json, &note);
                    note_log(folder, "warn", "render", &run, &note, json);
                }
            }
            render::outputs::ResolvedOutput::Hls { .. } => {
                let playlist = dest.display_path();
                let patched = std::fs::read_to_string(playlist).map(|body| {
                    let comment = render::outputs::provenance_playlist_comment(&prov.stamp());
                    match body.lines().next() {
                        // The stamp rides right under the header, still a
                        // `#` comment, so segment verification and players
                        // pass over it.
                        Some(first) if first.trim() == "#EXTM3U" => {
                            let mut out = String::with_capacity(body.len() + comment.len() + 1);
                            out.push_str(first);
                            out.push('\n');
                            out.push_str(&comment);
                            out.push('\n');
                            for line in body.lines().skip(1) {
                                out.push_str(line);
                                out.push('\n');
                            }
                            out
                        }
                        _ => {
                            let mut out = String::with_capacity(body.len() + comment.len() + 2);
                            out.push_str(body.trim_end());
                            out.push('\n');
                            out.push_str(&comment);
                            out.push('\n');
                            out
                        }
                    }
                });
                match patched {
                    Ok(body) => {
                        if let Err(e) = std::fs::write(playlist, body) {
                            let note = format!(
                                "stage output: note: provenance: playlist comment {} could not be written: {e}; pictures are unchanged.",
                                playlist.display()
                            );
                            stage_line(json, &note);
                            note_log(folder, "warn", "render", &run, &note, json);
                        }
                    }
                    Err(e) => {
                        let note = format!(
                            "stage output: note: provenance: playlist {} could not be stamped: {e}; pictures are unchanged.",
                            playlist.display()
                        );
                        stage_line(json, &note);
                        note_log(folder, "warn", "render", &run, &note, json);
                    }
                }
            }
            _ => {
                if !render::outputs::format_embeds_provenance(opts.format) {
                    let note = format!(
                        "stage output: note: provenance: --format {} carries no container metadata or sidecar, so this output is unstamped (pictures unchanged).",
                        opts.format.name()
                    );
                    stage_line(json, &note);
                    note_log(folder, "warn", "render", &run, &note, json);
                }
            }
        }
    }
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
                format: opts.format.name(),
                resolution: opts.resolution.name(),
                supersample: opts.supersample,
                enc: &opts.encoder(),
                output: &dest.display_path().display().to_string(),
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

/// Batch row limit (Sprint 9 variables, CAP-006): a rows file above
/// this many rows is refused naming the limit instead of starting,
/// so a runaway manifest never schedules unbounded renders.
pub const MAX_BATCH_ROWS: usize = 10_000;

/// Default batch name template (Sprint 9 variables, CAP-006): row
/// indexes count from 0, matching the `timeline` row numbering.
pub const DEFAULT_BATCH_TEMPLATE: &str = "clip-{index}";

/// One parsed batch row (Sprint 9 variables, CAP-006): the per-row
/// variable values, an optional explicit output name (winning over
/// `--name-template` for that row), and an optional per-row theme
/// (winning over the global `--theme` for that row).
#[derive(Debug, Clone, Default)]
pub struct BatchRow {
    pub variables: validate_mod::VariableOverrides,
    pub name: Option<String>,
    pub theme: Option<String>,
}

/// Parse one batch rows file (Sprint 9 variables, CAP-006): a JSON
/// array of row objects, each holding an optional `variables` object
/// of string keys to scalar values plus optional `name` and `theme`
/// strings. Every failure names the file and the row problem, before
/// any render starts.
pub fn parse_batch_rows(path: &Path) -> Result<Vec<BatchRow>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "render refused: cannot read batch rows file {}: {e} (validation stage); no MP4 was written",
            path.display()
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "render refused: batch rows file {} is not valid JSON ({e}); expected a JSON array of row objects like [{{\"variables\": {{\"title\": \"Sale\"}}}}] (validation stage); no MP4 was written",
            path.display()
        )
    })?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        other => {
            return Err(format!(
                "render refused: batch rows file {} holds {}; expected a JSON array of row objects like [{{\"variables\": {{\"title\": \"Sale\"}}}}] (validation stage); no MP4 was written",
                path.display(),
                json_kind(&other),
            ));
        }
    };
    let mut rows = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        rows.push(parse_batch_row(path, index, item)?);
    }
    Ok(rows)
}

/// JSON value kind for batch row-shape refusals (Sprint 9 variables,
/// CAP-006): the short word refusals name instead of echoing content.
fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// Parse one batch row value (Sprint 9 variables, CAP-006): an object
/// with optional `variables` (an object of string keys to scalar
/// values, like `--variables`), `name` (a string output name), and
/// `theme` (a string theme name). Anything else names the file, the
/// row index, and the expected shape.
fn parse_batch_row(path: &Path, index: usize, item: serde_json::Value) -> Result<BatchRow, String> {
    let refused = |detail: String| {
        format!(
            "render refused: batch rows file {} row {index} is a bad row shape: {detail}; expected each row to be an object like {{\"variables\": {{\"title\": \"Sale\"}}, \"name\": \"clip-a\", \"theme\": \"brand\"}} with `variables` an object of string keys to scalar values (validation stage); no MP4 was written",
            path.display(),
        )
    };
    let obj = match item {
        serde_json::Value::Object(map) => map,
        other => {
            return Err(refused(format!(
                "the row is {} instead of an object",
                json_kind(&other)
            )));
        }
    };
    let mut row = BatchRow::default();
    for (key, val) in obj {
        match key.as_str() {
            "variables" => {
                let vars = match val {
                    serde_json::Value::Object(map) => map,
                    other => {
                        return Err(refused(format!(
                            "key \"variables\" holds {} instead of an object",
                            json_kind(&other)
                        )));
                    }
                };
                for (name, scalar) in vars {
                    let parsed = match scalar {
                        serde_json::Value::String(s) => validate_mod::OverrideValue::Str(s),
                        serde_json::Value::Number(n) => match n.as_f64() {
                            Some(x) if x.is_finite() => validate_mod::OverrideValue::Num(x),
                            _ => {
                                return Err(refused(format!(
                                    "variable {name:?} holds a non-finite number; expected a scalar value (a string, finite number, or boolean)"
                                )));
                            }
                        },
                        serde_json::Value::Bool(b) => validate_mod::OverrideValue::Bool(b),
                        other => {
                            return Err(refused(format!(
                                "variable {name:?} holds {}; expected a scalar value (a string, number, or boolean)",
                                json_kind(&other)
                            )));
                        }
                    };
                    row.variables.insert(name, parsed);
                }
            }
            "name" => match val {
                serde_json::Value::String(s) => row.name = Some(s),
                other => {
                    return Err(refused(format!(
                        "key \"name\" holds {} instead of a string output name",
                        json_kind(&other)
                    )));
                }
            },
            "theme" => match val {
                serde_json::Value::String(s) => row.theme = Some(s),
                other => {
                    return Err(refused(format!(
                        "key \"theme\" holds {} instead of a string theme name",
                        json_kind(&other)
                    )));
                }
            },
            other => {
                return Err(refused(format!(
                    "unknown key {other:?}; expected only \"variables\", \"name\", and \"theme\""
                )));
            }
        }
    }
    Ok(row)
}

/// Collect the `{placeholder}` names of one name template in order
/// (Sprint 9 variables, CAP-006): an unclosed `{` refuses naming the
/// template, before any render starts.
fn batch_template_placeholders(template: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(format!(
                "render refused: --name-template {template:?} has an unclosed '{{'; close every placeholder like {{index}} or {{title}} (validation stage); no MP4 was written"
            ));
        };
        out.push(after[..close].to_string());
        rest = &after[close + 1..];
    }
    Ok(out)
}

/// Render one caller value for a batch output name (Sprint 9
/// variables, CAP-006): strings substitute verbatim, numbers and
/// booleans use their JSON scalar text.
fn batch_scalar_text(value: &validate_mod::OverrideValue) -> String {
    match value {
        validate_mod::OverrideValue::Str(s) => s.clone(),
        validate_mod::OverrideValue::Num(x) => x.to_string(),
        validate_mod::OverrideValue::Bool(b) => b.to_string(),
    }
}

/// Resolve one row's output file stem (Sprint 9 variables, CAP-006):
/// `{index}` fills from the row position (counting from 0) and every
/// other `{placeholder}` fills from that row's `variables` map, with
/// an empty substitution when the row carries no such key. A
/// placeholder that is neither `index`, a declared variable id, nor a
/// key of any row's `variables` map refuses naming the placeholder,
/// before any render starts.
fn resolve_batch_name(
    template: &str,
    index: usize,
    vars: &validate_mod::VariableOverrides,
    declared: &std::collections::BTreeSet<String>,
    row_keys: &std::collections::BTreeSet<String>,
) -> Result<String, String> {
    for ph in batch_template_placeholders(template)? {
        if ph == "index" || declared.contains(&ph) || row_keys.contains(&ph) {
            continue;
        }
        return Err(format!(
            "render refused: --name-template {template:?} names unknown placeholder {{{ph}}}; expected {{index}} or a variable id bound in the rows (for example {{title}}) (validation stage); no MP4 was written"
        ));
    }
    let mut out = String::with_capacity(template.len() + 8);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        // Checked above: every `{` closes, so this always succeeds.
        let close = after.find('}').unwrap_or(after.len());
        let key = &after[..close];
        if key == "index" {
            out.push_str(&index.to_string());
        } else if let Some(value) = vars.get(key) {
            out.push_str(&batch_scalar_text(value));
        }
        rest = &after[close.min(after.len())..];
        rest = rest.strip_prefix('}').unwrap_or(rest);
    }
    out.push_str(rest);
    Ok(out)
}

/// Resolve one batch output path (Sprint 9 variables, CAP-006): the
/// templated stem gains `.mp4` unless it already carries that suffix,
/// and relative names resolve against the project folder so row MP4s
/// sit beside the default `out.mp4`.
/// Batch row output path for one destination (Sprint 12 outputs,
/// T001): file destinations append the format extension exactly like
/// the historical `.mp4` rule; sequence destinations resolve the stem
/// to a directory (the PNG directory, or the HLS directory holding
/// `index.m3u8`).
fn batch_output_path(folder: &Path, stem: &str, format: render::outputs::OutputFormat) -> PathBuf {
    let path = if format.is_sequence() {
        PathBuf::from(stem)
    } else {
        let ext = format.default_extension();
        let file = if stem.ends_with(&format!(".{ext}")) {
            stem.to_string()
        } else {
            format!("{stem}.{ext}")
        };
        PathBuf::from(&file)
    };
    if path.is_absolute() {
        path
    } else {
        folder.join(path)
    }
}

/// Serialize one row's variables for the per-row render request
/// (Sprint 9 variables, CAP-006): rows parse from JSON values, so
/// they serialize back losslessly through the same scalar shapes
/// `--variables` accepts.
fn batch_overrides_json(vars: &validate_mod::VariableOverrides) -> String {
    let mut map = serde_json::Map::with_capacity(vars.len());
    for (key, value) in vars {
        let json = match value {
            validate_mod::OverrideValue::Str(s) => serde_json::Value::String(s.clone()),
            validate_mod::OverrideValue::Num(x) => serde_json::Number::from_f64(*x)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            validate_mod::OverrideValue::Bool(b) => serde_json::Value::Bool(*b),
        };
        map.insert(key.clone(), json);
    }
    serde_json::Value::Object(map).to_string()
}

/// One batch row outcome (Sprint 9 variables, CAP-006): pass or fail
/// with the row's output path and the reason (empty on success).
struct BatchOutcome {
    index: usize,
    ok: bool,
    output: String,
    message: String,
}

/// Render one batch row (Sprint 9 variables, CAP-006): pre-validate
/// the row's variables (plus its theme pick) so a bad row refuses
/// before frame one with the diagnostic named, otherwise render to
/// the row's templated output through the normal single-render path
/// (which stages, encodes, and reports exactly like `render -o`).
fn run_batch_row(
    folder: &Path,
    req: &RenderRequest,
    row: &BatchRow,
    output: &Path,
    index: usize,
) -> BatchOutcome {
    let theme = row.theme.as_deref().or(req.theme.as_deref());
    let verdict = validate_folder_with_variables_strict_themed(
        folder,
        &row.variables,
        req.strict_variables,
        theme,
    );
    if verdict.status != validate_mod::Status::Valid {
        let detail = verdict
            .diagnostics
            .first()
            .map(|d| format!("{d}"))
            .unwrap_or_else(|| "the row is invalid".to_string());
        return BatchOutcome {
            index,
            ok: false,
            output: output.display().to_string(),
            message: format!("row {index} invalid: {detail}"),
        };
    }
    let mut row_req = req.clone();
    row_req.variables = Some(batch_overrides_json(&row.variables));
    row_req.theme = theme.map(str::to_string);
    row_req.batch = None;
    row_req.name_template = None;
    row_req.jobs = None;
    row_req.fail_fast = false;
    let code = cmd_render_full(folder, Some(output), &row_req);
    if code == 0 {
        BatchOutcome {
            index,
            ok: true,
            output: output.display().to_string(),
            message: String::new(),
        }
    } else {
        BatchOutcome {
            index,
            ok: false,
            output: output.display().to_string(),
            message: format!(
                "row {index} render failed with exit {code}; see the row output above"
            ),
        }
    }
}

/// Machine-readable batch summary (Sprint 9 variables, CAP-006): the
/// same per-row pass/fail with output paths the human lines carry.
fn format_batch_json(
    folder: &Path,
    rows_file: &str,
    template: &str,
    jobs: usize,
    fail_fast: bool,
    outcomes: &[BatchOutcome],
) -> String {
    use std::fmt::Write;
    let passed = outcomes.iter().filter(|o| o.ok).count();
    let failed = outcomes
        .iter()
        .filter(|o| !o.ok && !o.message.starts_with("skipped"))
        .count();
    let skipped = outcomes.len() - passed - failed;
    let (status, exit) = if failed == 0 {
        ("ok", 0)
    } else {
        ("failed", 1)
    };
    let mut out = String::new();
    let fail_flag = if fail_fast { "true" } else { "false" };
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"render\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"batch\":{{\"rows_file\":\"{}\",\"template\":\"{}\",\"jobs\":{jobs},\"fail_fast\":{fail_flag},\"passed\":{passed},\"failed\":{failed},\"skipped\":{skipped}}},\"rows\":[",
        json_escape(&folder.display().to_string()),
        json_escape(rows_file),
        json_escape(template),
    );
    for (i, o) in outcomes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let row_status = if o.ok {
            "ok"
        } else if o.message.starts_with("skipped") {
            "skipped"
        } else {
            "failed"
        };
        let _ = write!(
            out,
            "{{\"index\":{},\"status\":\"{row_status}\",\"output\":\"{}\",\"message\":\"{}\"}}",
            o.index,
            json_escape(&o.output),
            json_escape(&o.message),
        );
    }
    out.push_str("]}");
    out
}

/// `render --batch` (Sprint 9 variables, CAP-006): render every rows
/// file row with its `variables` map to its templated output name,
/// then report per-row pass or fail with output paths. Concurrency is
/// bounded by `--jobs` (default 1, sequential); `--fail-fast` stops
/// scheduling after the first failing row while earlier rows' MP4s
/// stay in place. The batch exit is non-zero when any row fails and
/// zero when all pass (an empty rows file reports zero rows and
/// exits 0). Manifest, template, and duplicate-name problems refuse
/// before any render starts; a failing row never removes another
/// row's MP4.
pub fn cmd_render_batch(folder: &Path, output: Option<&Path>, req: &RenderRequest) -> i32 {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let json = req.json;
    let run = log::run_id();
    // Storyboard approval gate (Sprint 20, T004 / CAP-004, REC-002):
    // batch rows render MP4s, so the same pending gate refuses them
    // before any row starts, with no row rendered.
    if let Some(refusal) = preview::studio_stills::gate_refusal(folder, "render") {
        stage_line(json, &refusal);
        note_log(folder, "warn", "render", &run, &refusal, json);
        return 1;
    }
    let batch_file = req.batch.as_deref().unwrap_or("rows.json");
    let batch_path = PathBuf::from(batch_file);
    if output.is_some() {
        eprintln!(
            "framesmith render: --output cannot be combined with --batch; output names come from --name-template."
        );
        return 2;
    }
    if req.variables.is_some() {
        eprintln!(
            "framesmith render: --variables cannot be combined with --batch; each row carries its own variables map."
        );
        return 2;
    }
    // Sprint 12 outputs (T001): row output names carry the destination
    // extension, so the format resolves before any row starts; a bad
    // combination refuses here with no row rendered.
    let (batch_format, _, _, _, _, _, _, _) = match resolve_destination(req) {
        Ok(v) => v,
        Err(msg) => {
            stage_line(json, &msg);
            note_log(folder, "warn", "render", &run, &msg, json);
            return 1;
        }
    };
    let jobs: usize = match &req.jobs {
        None => 1,
        Some(raw) => match raw.trim().parse::<usize>() {
            Ok(n) if (1..=64).contains(&n) => n,
            _ => {
                let msg = format!(
                    "render refused: --jobs {raw:?} is not an integer from 1 to 64 (validation stage); no MP4 was written"
                );
                stage_line(json, &msg);
                note_log(folder, "warn", "render", &run, &msg, json);
                return 1;
            }
        },
    };
    let fail_fast = req.fail_fast;
    let template = req
        .name_template
        .as_deref()
        .unwrap_or(DEFAULT_BATCH_TEMPLATE);
    note_log(
        folder,
        "info",
        "render",
        &run,
        &format!(
            "batch start folder={} rows_file={batch_file:?} template={template:?} jobs={jobs} fail_fast={fail_fast}",
            folder.display()
        ),
        json,
    );

    let rows = match parse_batch_rows(&batch_path) {
        Ok(rows) => rows,
        Err(e) => {
            stage_line(json, &e);
            note_log(folder, "warn", "render", &run, &e, json);
            return 1;
        }
    };
    if rows.len() > MAX_BATCH_ROWS {
        let msg = format!(
            "render refused: batch rows file {} holds {} rows, above the limit of {} rows; split the file into smaller batches and re-run (validation stage); no MP4 was written",
            batch_path.display(),
            rows.len(),
            MAX_BATCH_ROWS
        );
        stage_line(json, &msg);
        note_log(folder, "warn", "render", &run, &msg, json);
        return 1;
    }
    // Declared variable ids (best effort: an invalid project yields
    // none, and per-row validation still names the real problems).
    let declared: std::collections::BTreeSet<String> = validate_folder(folder)
        .compositions
        .iter()
        .flat_map(|c| c.variables.iter().map(|v| v.id.clone()))
        .collect();
    let row_keys: std::collections::BTreeSet<String> = rows
        .iter()
        .flat_map(|r| r.variables.keys().cloned())
        .collect();
    // Resolve every output name before any render: an unknown
    // placeholder, an empty name, or two rows sharing one name refuses
    // here with both rows named.
    let mut outputs: Vec<PathBuf> = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let stem = match &row.name {
            Some(explicit) => explicit.clone(),
            None => match resolve_batch_name(template, index, &row.variables, &declared, &row_keys)
            {
                Ok(stem) => stem,
                Err(e) => {
                    stage_line(json, &e);
                    note_log(folder, "warn", "render", &run, &e, json);
                    return 1;
                }
            },
        };
        if stem.is_empty() {
            let msg = format!(
                "render refused: batch rows file {} row {index} resolves to an empty output name under --name-template {template:?}; give the row a \"name\" or a placeholder value (validation stage); no MP4 was written",
                batch_path.display()
            );
            stage_line(json, &msg);
            note_log(folder, "warn", "render", &run, &msg, json);
            return 1;
        }
        outputs.push(batch_output_path(folder, &stem, batch_format));
    }
    {
        let mut seen: std::collections::BTreeMap<PathBuf, usize> =
            std::collections::BTreeMap::new();
        for (index, out) in outputs.iter().enumerate() {
            if let Some(first) = seen.insert(out.clone(), index) {
                let msg = format!(
                    "render refused: batch rows {first} and {index} resolve to the same output name {}; make --name-template unique per row (for example include {{index}}) or give the rows distinct \"name\" values (validation stage); no MP4 was written",
                    out.display()
                );
                stage_line(json, &msg);
                note_log(folder, "warn", "render", &run, &msg, json);
                return 1;
            }
        }
    }

    if rows.is_empty() {
        let line = "batch summary: 0 row(s): 0 passed, 0 failed, 0 skipped.".to_string();
        stage_line(json, &line);
        note_log(folder, "info", "render", &run, &line, json);
        if json {
            println!(
                "{}",
                format_batch_json(folder, batch_file, template, jobs, fail_fast, &[])
            );
        }
        return 0;
    }

    // Execute rows: sequential in order by default, bounded parallel
    // workers under `--jobs`. Outcomes stay in row order whatever the
    // schedule; `--fail-fast` stops scheduling new rows after the
    // first failure while rows already running finish.
    let mut outcomes: Vec<Option<BatchOutcome>> = Vec::with_capacity(rows.len());
    outcomes.resize_with(rows.len(), || None);
    if jobs == 1 {
        let mut stop_at: Option<usize> = None;
        for (index, (row, out)) in rows.iter().zip(outputs.iter()).enumerate() {
            if stop_at.is_some() {
                outcomes[index] = Some(BatchOutcome {
                    index,
                    ok: false,
                    output: out.display().to_string(),
                    message: format!(
                        "skipped: fail-fast stopped the batch at row {}",
                        stop_at.unwrap_or(0)
                    ),
                });
                continue;
            }
            let outcome = run_batch_row(folder, req, row, out, index);
            if !outcome.ok {
                note_log(
                    folder,
                    "warn",
                    "render",
                    &run,
                    &format!("batch row {index} failed: {}", outcome.message),
                    json,
                );
                if fail_fast {
                    stop_at = Some(index);
                }
            }
            outcomes[index] = Some(outcome);
        }
    } else {
        let next = AtomicUsize::new(0);
        let failed_flag = AtomicBool::new(false);
        let failed_at = AtomicUsize::new(usize::MAX);
        let slots: Vec<std::sync::Mutex<Option<BatchOutcome>>> = (0..rows.len())
            .map(|_| std::sync::Mutex::new(None))
            .collect();
        std::thread::scope(|s| {
            for _ in 0..jobs.min(rows.len()) {
                s.spawn(|| loop {
                    if fail_fast && failed_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    if index >= rows.len() {
                        break;
                    }
                    let outcome = run_batch_row(folder, req, &rows[index], &outputs[index], index);
                    if !outcome.ok {
                        failed_flag.store(true, Ordering::SeqCst);
                        failed_at.fetch_min(index, Ordering::SeqCst);
                    }
                    *slots[index].lock().unwrap() = Some(outcome);
                });
            }
        });
        let stopped = failed_flag.load(Ordering::SeqCst) && fail_fast;
        let stop_row = failed_at.load(Ordering::SeqCst);
        for (index, slot) in slots.into_iter().enumerate() {
            let taken = slot.into_inner().unwrap();
            outcomes[index] = match taken {
                Some(o) => {
                    if !o.ok {
                        note_log(
                            folder,
                            "warn",
                            "render",
                            &run,
                            &format!("batch row {index} failed: {}", o.message),
                            json,
                        );
                    }
                    Some(o)
                }
                None => {
                    debug_assert!(stopped);
                    Some(BatchOutcome {
                        index,
                        ok: false,
                        output: outputs[index].display().to_string(),
                        message: format!("skipped: fail-fast stopped the batch at row {stop_row}"),
                    })
                }
            };
        }
        let _ = stopped;
    }
    let outcomes: Vec<BatchOutcome> = outcomes.into_iter().map(|o| o.unwrap()).collect();

    for o in &outcomes {
        if o.ok {
            stage_line(json, &format!("batch: row {}: ok {}", o.index, o.output));
        } else if o.message.starts_with("skipped") {
            stage_line(
                json,
                &format!("batch: row {}: {} {}", o.index, o.message, o.output),
            );
        } else {
            stage_line(
                json,
                &format!("batch: row {}: failed {}: {}", o.index, o.output, o.message),
            );
        }
    }
    let passed = outcomes.iter().filter(|o| o.ok).count();
    let failed = outcomes
        .iter()
        .filter(|o| !o.ok && !o.message.starts_with("skipped"))
        .count();
    let skipped = outcomes.len() - passed - failed;
    let line = format!(
        "batch summary: {} row(s): {passed} passed, {failed} failed, {skipped} skipped.",
        outcomes.len()
    );
    stage_line(json, &line);
    note_log(folder, "info", "render", &run, &line, json);
    if json {
        println!(
            "{}",
            format_batch_json(folder, batch_file, template, jobs, fail_fast, &outcomes)
        );
    }
    if failed == 0 {
        0
    } else {
        1
    }
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
    dest: &render::outputs::ResolvedOutput,
    format: &str,
    resolution: &str,
    supersample: u8,
    stage: &'static str,
    stages: &mut Vec<render::render::StageTiming>,
    total_start: std::time::Instant,
    stage_ms: u64,
    detail: &str,
) -> i32 {
    // Sprint 12 outputs (T001): files remove the staged temp file plus
    // the mix as before; sequences remove the staging directory plus
    // the staged mix, so no partial directory survives.
    let mix = mix_path_for_dest(dest);
    let leftovers: Vec<PathBuf> = match dest {
        render::outputs::ResolvedOutput::File { path } => {
            render::cancel::remove_staged(path, &[mix])
        }
        render::outputs::ResolvedOutput::PngSeq { dir }
        | render::outputs::ResolvedOutput::Hls { dir, .. } => {
            let stage_dir = render::outputs::staging_dir(dir);
            let mut leftovers = Vec::new();
            match std::fs::remove_dir_all(&stage_dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => leftovers.push(stage_dir),
            }
            match std::fs::remove_file(&mix) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => leftovers.push(mix),
            }
            leftovers
        }
    };
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
                format,
                resolution,
                supersample,
                enc,
                output: &dest.display_path().display().to_string(),
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

/// `snapshot` request (Sprint 13 check, T005 / CAP-005): the raw flag
/// texts for one snapshot run. `at` holds the comma-separated `--at`
/// times, `every` the `--every` interval, `from`/`to` the `--from` /
/// `--to` range bounds, and `out` the `--out` directory; exactly one
/// of the three time selectors must be present. `composition`,
/// `backend`, `variables`, `strict_variables`, and `theme` ride along
/// from the render flags so the snapshots show the same picture the
/// render would.
#[derive(Debug, Clone, Default)]
pub struct SnapshotRequest {
    pub json: bool,
    pub composition: Option<String>,
    pub backend: Option<String>,
    pub variables: Option<String>,
    pub strict_variables: bool,
    pub theme: Option<String>,
    pub at: Option<String>,
    pub every: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub out: Option<String>,
}

/// One resolved snapshot frame (Sprint 13 check, T005 / CAP-005): the
/// requested composition time, the output frame drawn for it, and the
/// exact time of that frame. `requested` and `time` differ only when
/// the requested time fell between frames and snapped.
struct SnapshotPick {
    requested: f64,
    frame: u64,
    time: f64,
}

/// Parse one raw `--at` / `--every` / `--from` / `--to` value (Sprint
/// 13 check, T005 / CAP-005): a finite number of seconds. Anything
/// else refuses naming the flag with the expected shape, before any
/// PNG is written.
fn parse_snapshot_seconds(raw: &str, flag: &str) -> Result<f64, String> {
    match raw.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(format!(
            "snapshot refused: {flag} {raw:?} is not a number of seconds: pass {flag}=<seconds> (for example {flag}=0.5); no PNG was written"
        )),
    }
}

/// Canonical snapshot time text (Sprint 13 check, T005 / CAP-005):
/// up to six decimals with trailing zeros stripped (`0`, `0.5`,
/// `0.033333`), so `snap-<time>s.png` names the exact frame time
/// without ambiguity.
fn format_snap_time(t: f64) -> String {
    if t == 0.0 {
        return "0".to_string();
    }
    let s = format!("{t:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// File name of one snapshot PNG (Sprint 13 check, T005 / CAP-005):
/// `snap-<time>s.png` at the exact (possibly snapped) frame time.
fn snapshot_file_name(time: f64) -> String {
    format!("snap-{}s.png", format_snap_time(time))
}

/// Restate a render-pipeline error for the snapshot command (Sprint 13
/// check, T005 / CAP-005): the same cause, but naming `snapshot` and
/// PNGs instead of `render` and MP4s.
fn snapshotize(mut e: String) -> String {
    if let Some(rest) = e.strip_prefix("render refused:") {
        e = format!("snapshot refused:{rest}");
    } else if let Some(rest) = e.strip_prefix("render failed:") {
        e = format!("snapshot failed:{rest}");
    }
    e = e.replace("no MP4 was written", "no PNG was written");
    e
}

/// Resolve the requested times to output frames (Sprint 13 check, T005
/// / CAP-005): exactly one of `--at` (comma times), `--every`
/// (interval), or `--from`/`--to` (range). Between-frame times snap to
/// the nearest frame (ties round up); the caller reports every snap.
/// Every refusal names the flag and the fix, before any PNG is written.
fn resolve_snapshot_picks(
    req: &SnapshotRequest,
    comp: &Composition,
) -> Result<Vec<SnapshotPick>, String> {
    let rate = comp.fps.value();
    let duration = comp.duration;
    let frames_total = comp.fps.frame_count(duration);
    if frames_total == 0 {
        return Err(
            "snapshot refused: the composition has no frames; no PNG was written".to_string(),
        );
    }
    let snap = |t: f64| -> SnapshotPick {
        let frame = ((t * rate).round() as u64).min(frames_total - 1);
        let time = frame as f64 / rate;
        SnapshotPick {
            requested: t,
            frame,
            time,
        }
    };
    if let Some(raw) = req.at.as_deref() {
        let mut picks: Vec<SnapshotPick> = Vec::new();
        let mut seen: Vec<u64> = Vec::new();
        let mut any = false;
        for part in raw.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            any = true;
            let t = parse_snapshot_seconds(part, "--at")?;
            if t < 0.0 || t > duration {
                return Err(format!(
                    "snapshot refused: --at {t}s is outside composition {:?} (duration {duration:.3}s): pass a time from 0 to {duration:.3}s; no PNG was written",
                    comp.name
                ));
            }
            let pick = snap(t);
            if !seen.contains(&pick.frame) {
                seen.push(pick.frame);
                picks.push(pick);
            }
        }
        if !any {
            return Err(
                "snapshot refused: --at needs at least one time in seconds (for example --at=0,0.5,1); no PNG was written"
                    .to_string(),
            );
        }
        picks.sort_by_key(|p| p.frame);
        return Ok(picks);
    }
    if let Some(raw) = req.every.as_deref() {
        let interval = parse_snapshot_seconds(raw, "--every")?;
        if interval <= 0.0 {
            return Err(format!(
                "snapshot refused: --every {raw:?} is not a positive number of seconds: pass --every=<seconds> (for example --every=0.5); no PNG was written"
            ));
        }
        let mut picks: Vec<SnapshotPick> = Vec::new();
        let mut k: u64 = 0;
        loop {
            let t = k as f64 * interval;
            if t >= duration {
                break;
            }
            picks.push(snap(t));
            k += 1;
        }
        if picks.is_empty() {
            return Err(format!(
                "snapshot refused: --every {raw:?} lands no frame inside composition {:?} (duration {duration:.3}s); no PNG was written",
                comp.name
            ));
        }
        return Ok(picks);
    }
    // Reachable only when the caller passed `--from` and/or `--to`
    // (the shape check in `main` guarantees the pair travels together).
    let (from_raw, to_raw) = match (req.from.as_deref(), req.to.as_deref()) {
        (Some(f), Some(t)) => (f, t),
        _ => {
            return Err(
                "snapshot refused: pass --from=<seconds> together with --to=<seconds> (for example --from=0 --to=1); no PNG was written"
                    .to_string(),
            );
        }
    };
    let from = parse_snapshot_seconds(from_raw, "--from")?;
    let to = parse_snapshot_seconds(to_raw, "--to")?;
    if from < 0.0 || to > duration {
        return Err(format!(
            "snapshot refused: --from/--to {from:.3}s..{to:.3}s reaches outside composition {:?} (duration {duration:.3}s): pass 0 <= --from < --to <= {duration:.3}s; no PNG was written",
            comp.name
        ));
    }
    if from > to {
        return Err(format!(
            "snapshot refused: --from/--to {from:.3}s..{to:.3}s runs backward in composition {:?}: pass 0 <= --from < --to <= {duration:.3}s; no PNG was written",
            comp.name
        ));
    }
    if from == to {
        return Err(format!(
            "snapshot refused: --from/--to {from:.3}s..{to:.3}s is an empty range in composition {:?}: pass 0 <= --from < --to <= {duration:.3}s; no PNG was written",
            comp.name
        ));
    }
    let first = ((from * rate) - 1e-9).ceil() as i64;
    let last = ((to * rate) - 1e-9).ceil() as i64 - 1;
    if first > last {
        return Err(format!(
            "snapshot refused: --from/--to {from:.3}s..{to:.3}s holds no output frame at {} fps; no PNG was written",
            comp.fps
        ));
    }
    Ok((first..=last)
        .map(|k| {
            let time = k as f64 / rate;
            SnapshotPick {
                requested: time,
                frame: k as u64,
                time,
            }
        })
        .collect())
}

/// Machine-readable snapshot report (Sprint 13 check, T005 / CAP-005):
/// the same composition, frames, and files the human lines carry.
#[allow(clippy::too_many_arguments)]
fn format_snapshot_json(
    folder: &Path,
    status: &str,
    exit: i32,
    composition: Option<&str>,
    backend: &str,
    output: &str,
    message: Option<&str>,
    picks: &[SnapshotPick],
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"snapshot\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"composition\":",
        json_escape(&folder.display().to_string())
    );
    match composition {
        Some(c) => {
            let _ = write!(out, "\"{}\"", json_escape(c));
        }
        None => out.push_str("null"),
    }
    let _ = write!(
        out,
        ",\"backend\":\"{}\",\"output\":\"{}\",\"message\":",
        json_escape(backend),
        json_escape(output)
    );
    match message {
        Some(m) => {
            let _ = write!(out, "\"{}\"", json_escape(m));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"frames\":[");
    for (i, p) in picks.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"requested\":{},\"frame\":{},\"time\":{},\"file\":\"{}\"}}",
            p.requested,
            p.frame,
            p.time,
            json_escape(&snapshot_file_name(p.time))
        );
    }
    out.push_str("],\"files\":[");
    for (i, p) in picks.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\"", json_escape(&snapshot_file_name(p.time)));
    }
    out.push_str("]}");
    out
}

/// `snapshot`: draw the requested frames and write them as
/// `snap-<time>s.png` into `--out` (Sprint 13 check, T005 / CAP-005).
/// Frames draw through the same pipeline as a full render, so each
/// PNG decodes pixel-identical to the same full-render frame; no MP4
/// is ever written. Every refusal names the flag and the fix, and no
/// PNG is written before all refusals have passed. Exits 0 on success,
/// 1 on refusal or failure, 2 when the project is valid but uses
/// later-sprint elements.
pub fn cmd_snapshot_full(folder: &Path, req: &SnapshotRequest) -> i32 {
    let json = req.json;
    let run = log::run_id();
    // The `--out` directory travels as given (like render `-o`:
    // relative to the caller's working directory). Its absence is a
    // usage error, like a missing `<folder>`.
    let out_dir = match req.out.as_deref() {
        Some(raw) if !raw.trim().is_empty() => PathBuf::from(raw),
        _ => {
            eprintln!("framesmith snapshot: missing --out <dir>.");
            eprintln!("usage: framesmith snapshot [--json] [--composition <name>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] (--at <t,..> | --every <secs> | --from <s> --to <e>) --out <dir> <folder>");
            return 2;
        }
    };
    // Flags resolve before validation: an unknown backend refuses
    // naming the flag with no PNG written.
    let backend_raw = req.backend.as_deref().unwrap_or("auto");
    let backend = match render::render::Backend::parse(backend_raw) {
        Some(b) => b,
        None => {
            let msg = format!(
                "snapshot refused: unknown --backend {backend_raw:?}: expected cpu, gpu, or auto; no PNG was written"
            );
            stage_line(json, &msg);
            note_log(folder, "warn", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "invalid",
                        1,
                        None,
                        "unknown",
                        "",
                        Some(&msg),
                        &[]
                    )
                );
            }
            return 1;
        }
    };
    let overrides = match req.variables.as_deref() {
        None => validate_mod::VariableOverrides::new(),
        Some(raw) => match validate_mod::parse_variable_overrides(raw) {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("snapshot refused: {e}; no PNG was written");
                stage_line(json, &msg);
                note_log(folder, "warn", "snapshot", &run, &msg, json);
                if json {
                    println!(
                        "{}",
                        format_snapshot_json(
                            folder,
                            "invalid",
                            1,
                            None,
                            backend.name(),
                            "",
                            Some(&msg),
                            &[]
                        )
                    );
                }
                return 1;
            }
        },
    };
    stage_line(json, "stage validate: running");
    let verdict = validate_folder_with_variables_strict_themed(
        folder,
        &overrides,
        req.strict_variables,
        req.theme.as_deref(),
    );
    match verdict.status {
        Status::Invalid => {
            let msg = format!(
                "snapshot refused: {} is invalid; fix the problems `check` reports and re-run. No PNG was written.",
                folder.display()
            );
            stage_line(json, &format!("stage validate: failed: {msg}"));
            note_log(folder, "warn", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "invalid",
                        1,
                        None,
                        backend.name(),
                        "",
                        Some(&msg),
                        &[]
                    )
                );
            } else {
                println!("{msg}");
                for d in &verdict.diagnostics {
                    println!("{d}");
                }
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
                "snapshot refused: {} uses later-sprint element(s): {}. No PNG was written.",
                folder.display(),
                names.join("; ")
            );
            stage_line(json, &format!("stage validate: failed: {msg}"));
            note_log(folder, "warn", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "unsupported",
                        2,
                        None,
                        backend.name(),
                        "",
                        Some(&msg),
                        &[]
                    )
                );
            } else {
                println!("{msg}");
            }
            return 2;
        }
        Status::Valid => {
            stage_line(json, "stage validate: ok");
        }
    }
    for w in &verdict.variable_warnings {
        let note = format!("stage validate: note: {w}");
        stage_line(json, &note);
        note_log(folder, "info", "snapshot", &run, &note, json);
    }
    if backend == render::render::Backend::Gpu && !render::gpu::gpu_available() {
        let msg = "snapshot refused: no GPU renderer is available on this machine (capability probe found none); use `--backend cpu` or `--backend auto` instead. No PNG was written.".to_string();
        stage_line(json, &msg);
        note_log(folder, "warn", "snapshot", &run, &msg, json);
        if json {
            println!(
                "{}",
                format_snapshot_json(
                    folder,
                    "invalid",
                    1,
                    None,
                    backend.name(),
                    "",
                    Some(&msg),
                    &[]
                )
            );
        }
        return 1;
    }
    // The selected composition draws; an unknown name refuses with the
    // available names and no PNG written.
    let comp = match select_composition(&verdict, req.composition.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            let msg = snapshotize(e);
            stage_line(json, &msg);
            note_log(folder, "warn", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "invalid",
                        1,
                        None,
                        backend.name(),
                        "",
                        Some(&msg),
                        &[]
                    )
                );
            }
            return 1;
        }
    };
    // Times resolve against the selected composition: a bad time, an
    // empty range, or an outside time refuses naming the flag before
    // any PNG is written.
    let picks = match resolve_snapshot_picks(req, &comp) {
        Ok(p) => p,
        Err(msg) => {
            stage_line(json, &msg);
            note_log(folder, "warn", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "invalid",
                        1,
                        Some(&comp.name),
                        backend.name(),
                        "",
                        Some(&msg),
                        &[]
                    )
                );
            }
            return 1;
        }
    };
    // Detection only reads host state; the folder is never modified.
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
    // Bootstrap: the frame preparation decodes video and image sources
    // through the host FFmpeg, so a missing binary refuses here with
    // the fix, before any PNG is written.
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
            let msg = snapshotize(message);
            stage_line(json, &msg);
            if !json {
                println!("(code {code} — see `framesmith errors {code}` for meaning and fix.)");
            }
            note_log(folder, "warn", "bootstrap", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "failed",
                        1,
                        Some(&comp.name),
                        backend.name(),
                        "",
                        Some(&msg),
                        &[]
                    )
                );
            }
            return 1;
        }
    };
    if render::cancel::is_cancelled() {
        let msg = format!(
            "{} snapshot cancelled: Ctrl-C before the first frame; no PNG was written",
            render::cancel::PREFIX
        );
        stage_line(json, &msg);
        note_log(folder, "warn", "snapshot", &run, &msg, json);
        if json {
            println!(
                "{}",
                format_snapshot_json(
                    folder,
                    "failed",
                    130,
                    Some(&comp.name),
                    backend.name(),
                    "",
                    Some(&msg),
                    &[]
                )
            );
        }
        return render::cancel::EXIT_CODE;
    }
    // The output directory must accept files before the first frame is
    // drawn: probe the nearest existing ancestor without creating the
    // final directory, so an unwritable `--out` refuses with no PNG
    // written.
    {
        let anchor = nearest_existing_ancestor(&out_dir);
        let probe = anchor.join(".framesmith-write-probe");
        if let Err(e) = std::fs::write(&probe, b"framesmith writability probe") {
            let msg = format!(
                "snapshot refused: output directory {} is not writable: {e} (pass a writable path to `--out`); no PNG was written",
                out_dir.display()
            );
            stage_line(json, &msg);
            note_log(folder, "warn", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "invalid",
                        1,
                        Some(&comp.name),
                        backend.name(),
                        &out_dir.display().to_string(),
                        Some(&msg),
                        &[]
                    )
                );
            }
            return 1;
        } else {
            let _ = std::fs::remove_file(&probe);
        }
    }
    // Frames stage: prepare every picture input (fonts, video probes
    // and decodes, images), exactly like a full render, so snapshots
    // show the same pictures the render encodes.
    stage_line(json, "stage frames: running");
    let effective_backend = render::render::resolve_backend(backend);
    let prep =
        match render::render::prepare_frames(&comp, &verdict.compositions, folder, &ffmpeg_bin) {
            Ok(prep) => prep,
            Err(e) => {
                let msg = snapshotize(e);
                stage_line(json, &format!("stage frames: failed: {msg}"));
                note_log(folder, "error", "snapshot", &run, &msg, json);
                if json {
                    println!(
                        "{}",
                        format_snapshot_json(
                            folder,
                            "failed",
                            1,
                            Some(&comp.name),
                            backend.name(),
                            "",
                            Some(&msg),
                            &[]
                        )
                    );
                } else {
                    println!("{msg}");
                }
                return 1;
            }
        };
    let (canvas_w, canvas_h) = prep.canvas();
    let frames: Vec<u64> = picks.iter().map(|p| p.frame).collect();
    // Draw every requested frame through the same pipeline (and the
    // same effective backend) as the full render: frame N alone equals
    // frame N of the full render exactly.
    let drawn = match render::render::draw_batch_parallel(&prep, effective_backend, &frames) {
        Ok(d) => d,
        Err(e) => {
            let msg = snapshotize(e);
            stage_line(json, &format!("stage frames: failed: {msg}"));
            note_log(folder, "error", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "failed",
                        1,
                        Some(&comp.name),
                        backend.name(),
                        "",
                        Some(&msg),
                        &[]
                    )
                );
            } else {
                println!("{msg}");
            }
            return 1;
        }
    };
    // Encode every PNG before writing any file, so an encoding problem
    // still leaves no partial snapshot set behind.
    let mut encoded: Vec<(String, Vec<u8>)> = Vec::with_capacity(drawn.len());
    for (frame, rgb) in &drawn {
        let pick = picks
            .iter()
            .find(|p| p.frame == *frame)
            .expect("drawn frame was requested");
        match render::outputs::encode_png_rgb8(canvas_w, canvas_h, rgb) {
            Ok(bytes) => encoded.push((snapshot_file_name(pick.time), bytes)),
            Err(msg) => {
                stage_line(json, &format!("stage frames: failed: {msg}"));
                note_log(folder, "error", "snapshot", &run, &msg, json);
                if json {
                    println!(
                        "{}",
                        format_snapshot_json(
                            folder,
                            "failed",
                            1,
                            Some(&comp.name),
                            backend.name(),
                            "",
                            Some(&msg),
                            &[]
                        )
                    );
                } else {
                    println!("{msg}");
                }
                return 1;
            }
        }
    }
    // Output stage: create the directory, prove it accepts a file, then
    // write every PNG. A write failure removes this run's files so no
    // partial set is presented as complete.
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        let msg = format!(
            "snapshot refused: cannot create output directory {}: {e}; no PNG was written",
            out_dir.display()
        );
        stage_line(json, &msg);
        note_log(folder, "warn", "snapshot", &run, &msg, json);
        if json {
            println!(
                "{}",
                format_snapshot_json(
                    folder,
                    "invalid",
                    1,
                    Some(&comp.name),
                    backend.name(),
                    &out_dir.display().to_string(),
                    Some(&msg),
                    &[]
                )
            );
        }
        return 1;
    }
    let probe = out_dir.join(".framesmith-write-probe");
    match std::fs::write(&probe, b"framesmith writability probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
        }
        Err(e) => {
            let msg = format!(
                "snapshot refused: output directory {} is not writable: {e} (pass a writable path to `--out`); no PNG was written",
                out_dir.display()
            );
            stage_line(json, &msg);
            note_log(folder, "warn", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "invalid",
                        1,
                        Some(&comp.name),
                        backend.name(),
                        &out_dir.display().to_string(),
                        Some(&msg),
                        &[]
                    )
                );
            }
            return 1;
        }
    }
    let mut written: Vec<String> = Vec::with_capacity(encoded.len());
    for (name, bytes) in &encoded {
        let path = out_dir.join(name);
        if let Err(e) = std::fs::write(&path, bytes) {
            for done in &written {
                let _ = std::fs::remove_file(out_dir.join(done));
            }
            let msg = format!(
                "snapshot failed: cannot write {}: {e}; no complete snapshot set was written",
                path.display()
            );
            stage_line(json, &msg);
            note_log(folder, "error", "snapshot", &run, &msg, json);
            if json {
                println!(
                    "{}",
                    format_snapshot_json(
                        folder,
                        "failed",
                        1,
                        Some(&comp.name),
                        backend.name(),
                        &out_dir.display().to_string(),
                        Some(&msg),
                        &[]
                    )
                );
            } else {
                println!("{msg}");
            }
            return 1;
        }
        written.push(name.clone());
    }
    let out_text = out_dir.display().to_string();
    for pick in &picks {
        if pick.requested != pick.time {
            stage_line(
                json,
                &format!(
                    "snapshot: note: time {}s is between frames; using frame {} at {}s.",
                    pick.requested,
                    pick.frame,
                    format_snap_time(pick.time)
                ),
            );
        }
    }
    stage_line(
        json,
        &format!(
            "snapshot: wrote {} PNG(s) to {} (composition {:?}, backend {}).",
            written.len(),
            out_text,
            comp.name,
            effective_backend.name()
        ),
    );
    for pick in &picks {
        stage_line(
            json,
            &format!(
                "snapshot: wrote {} (frame {} at {}s).",
                snapshot_file_name(pick.time),
                pick.frame,
                format_snap_time(pick.time)
            ),
        );
    }
    note_log(
        folder,
        "info",
        "snapshot",
        &run,
        &format!("snapshot ok: {} PNG(s) in {}", written.len(), out_text),
        json,
    );
    if json {
        println!(
            "{}",
            format_snapshot_json(
                folder,
                "ok",
                0,
                Some(&comp.name),
                effective_backend.name(),
                &out_text,
                None,
                &picks
            )
        );
    }
    0
}

// Inspection commands (Sprint 13 check, T006 / CAP-006): `keyframes`
// lists computed per-frame motion values, `compare` joins two renders
// side by side with an SSIM score, `grade-compare` splits one
// composition into ungraded and graded halves, `doctor` reports
// environment and project health, and `benchmark` times bundled
// references. Every command is read-only toward the project: sources
// and media are never modified, and only the command's own outputs
// (`-o`/`--out` files) are written. The run log and the footage cache
// keep their usual operational trails beside the project, as with
// every other command.

/// Restate a render-pipeline error for an inspection command.
/// (Sprint
/// 13 check, T006 / CAP-006): the cause stays the render's exact
/// words, while the command name and the no-output tail name this
/// inspection — there is no MP4 to leave behind, only the command's
/// own listing or file unwritten.
fn inspectize(cmd: &str, tail: &str, e: String) -> String {
    let restated = if let Some(rest) = e.strip_prefix("render refused:") {
        format!("{cmd} refused:{rest}")
    } else if let Some(rest) = e.strip_prefix("render failed:") {
        format!("{cmd} failed:{rest}")
    } else {
        format!("{cmd} refused: {e}")
    };
    restated
        .replace("no MP4 was written", tail)
        .replace("No MP4 was written", tail)
}

/// Parse one raw `--at` value for an inspection command (Sprint 13
/// check, T006 / CAP-006): a finite number of seconds, refused naming
/// the flag before anything is drawn or written.
fn parse_inspect_seconds(cmd: &str, tail: &str, raw: &str) -> Result<f64, String> {
    match raw.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(format!(
            "{cmd} refused: --at {raw:?} is not a number of seconds: pass --at=<seconds> (for example --at=0.5); {tail}"
        )),
    }
}

/// Global SSIM of two same-size RGB pictures over luma (Sprint 13
/// check, T006 / CAP-006): 1.0 for byte-identical inputs, lower the
/// more the pictures differ. The whole frame is one window, which is
/// exact for the identical-input bar check the command promises.
fn ssim_luma(a: &[u8], b: &[u8], pixels: usize) -> f64 {
    if a == b {
        return 1.0;
    }
    debug_assert_eq!(a.len(), pixels * 3);
    debug_assert_eq!(b.len(), pixels * 3);
    let mut sum_a = 0.0f64;
    let mut sum_b = 0.0f64;
    for i in 0..pixels {
        let o = i * 3;
        sum_a +=
            0.299 * f64::from(a[o]) + 0.587 * f64::from(a[o + 1]) + 0.114 * f64::from(a[o + 2]);
        sum_b +=
            0.299 * f64::from(b[o]) + 0.587 * f64::from(b[o + 1]) + 0.114 * f64::from(b[o + 2]);
    }
    let n = pixels as f64;
    let mean_a = sum_a / n;
    let mean_b = sum_b / n;
    let mut var_a = 0.0f64;
    let mut var_b = 0.0f64;
    let mut cov = 0.0f64;
    for i in 0..pixels {
        let o = i * 3;
        let la =
            0.299 * f64::from(a[o]) + 0.587 * f64::from(a[o + 1]) + 0.114 * f64::from(a[o + 2]);
        let lb =
            0.299 * f64::from(b[o]) + 0.587 * f64::from(b[o + 1]) + 0.114 * f64::from(b[o + 2]);
        var_a += (la - mean_a) * (la - mean_a);
        var_b += (lb - mean_b) * (lb - mean_b);
        cov += (la - mean_a) * (lb - mean_b);
    }
    var_a /= n;
    var_b /= n;
    cov /= n;
    // Luminance dynamic range with the standard stability constants.
    let c1 = 6.5025f64;
    let c2 = 58.5225f64;
    let num = (2.0 * mean_a * mean_b + c1) * (2.0 * cov + c2);
    let den = (mean_a * mean_a + mean_b * mean_b + c1) * (var_a + var_b + c2);
    if den <= 0.0 {
        return 1.0;
    }
    (num / den).clamp(0.0, 1.0)
}

/// The similarity bar `compare` reports every score against (Sprint 13
/// check, T006 / CAP-006): SSIM All at least 0.96, the same bar the
/// specification names.
pub const COMPARE_SSIM_BAR: f64 = 0.96;

/// Join two same-size RGB pictures side by side (Sprint 13 check, T006
/// / CAP-006): `left` on the left half, `right` on the right half, one
/// row at a time.
fn join_side_by_side(left: &[u8], right: &[u8], w: u32, h: u32) -> Vec<u8> {
    let w = w as usize;
    let row = w * 3;
    let mut out = vec![0u8; w * 2 * h as usize * 3];
    for y in 0..h as usize {
        out[y * row * 2..y * row * 2 + row].copy_from_slice(&left[y * row..(y + 1) * row]);
        out[y * row * 2 + row..(y + 1) * row * 2].copy_from_slice(&right[y * row..(y + 1) * row]);
    }
    out
}

/// One frame drawn for an inspection command (Sprint 13 check, T006 /
/// CAP-006): the selected composition, its canvas, the snapped frame
/// and its exact time, the straight RGB8 picture, and the effective
/// backend that drew it.
struct InspectedDraw {
    comp: Composition,
    width: u32,
    height: u32,
    frame: u64,
    time: f64,
    rgb: Vec<u8>,
    backend: String,
}

/// Validate, select, and draw one snapped frame through the same
/// pipeline (and the same effective backend) as a full render, for an
/// inspection command (Sprint 13 check, T006 / CAP-006). Every refusal
/// names the command, the flag or project at fault, and the fix, and
/// carries the command's no-output tail; nothing is drawn or written
/// before every check has passed. `strip_grades` draws the
/// composition with its composition and lane grades removed (the
/// `grade-compare` ungraded half); per-clip grades ride with their
/// clips either way.
#[allow(clippy::too_many_arguments)]
fn draw_inspect_frame(
    cmd: &str,
    tail: &str,
    folder: &Path,
    composition: Option<&str>,
    backend_raw: &str,
    overrides: &validate_mod::VariableOverrides,
    strict_variables: bool,
    theme: Option<&str>,
    at: f64,
    strip_grades: bool,
) -> Result<InspectedDraw, String> {
    let backend = match render::render::Backend::parse(backend_raw) {
        Some(b) => b,
        None => {
            return Err(format!(
                "{cmd} refused: unknown --backend {backend_raw:?}: expected cpu, gpu, or auto; {tail}"
            ));
        }
    };
    let verdict =
        validate_folder_with_variables_strict_themed(folder, overrides, strict_variables, theme);
    match verdict.status {
        Status::Invalid => {
            return Err(format!(
                "{cmd} refused: {} is invalid; fix the problems `check` reports and re-run. {tail}.",
                folder.display()
            ));
        }
        Status::Unsupported => {
            return Err(format!(
                "{cmd} refused: {} uses later-sprint element(s); re-run once the project checks green. {tail}.",
                folder.display()
            ));
        }
        Status::Valid => {}
    }
    let comp = match select_composition(&verdict, composition) {
        Ok(c) => c,
        Err(e) => return Err(inspectize(cmd, tail, e)),
    };
    if !at.is_finite() || at < 0.0 || at > comp.duration {
        return Err(format!(
            "{cmd} refused: --at {at}s is outside composition {:?} (duration {:.3}s): pass a time from 0 to {:.3}s; {tail}",
            comp.name, comp.duration, comp.duration
        ));
    }
    if backend == render::render::Backend::Gpu && !render::gpu::gpu_available() {
        return Err(format!(
            "{cmd} refused: no GPU renderer is available on this machine (capability probe found none); use `--backend cpu` or `--backend auto` instead. {tail}."
        ));
    }
    let rate = comp.fps.value();
    let frames_total = comp.fps.frame_count(comp.duration);
    if frames_total == 0 {
        return Err(format!(
            "{cmd} refused: composition {:?} has no frames; {tail}",
            comp.name
        ));
    }
    let frame = ((at * rate).round() as u64).min(frames_total - 1);
    let time = frame as f64 / rate;
    let finding = ffmpeg_mod::detect();
    let cfg = bootstrap::BootstrapConfig::from_env(verdict.ffmpeg_auto_download);
    let ffmpeg_bin = match bootstrap::ensure_ffmpeg(&finding, &cfg) {
        bootstrap::Bootstrap::Ready { ffmpeg_path, .. } => ffmpeg_path,
        bootstrap::Bootstrap::Refused { message, .. } => {
            return Err(inspectize(cmd, tail, message));
        }
    };
    if render::cancel::is_cancelled() {
        return Err(format!(
            "{} {cmd} cancelled: Ctrl-C before the first frame; {tail}",
            render::cancel::PREFIX
        ));
    }
    let mut use_comp = comp.clone();
    if strip_grades {
        use_comp.grade = notation::model::Grade::default();
        use_comp.lane_grades.clear();
    }
    let prep =
        match render::render::prepare_frames(&use_comp, &verdict.compositions, folder, &ffmpeg_bin)
        {
            Ok(prep) => prep,
            Err(e) => return Err(inspectize(cmd, tail, e)),
        };
    let (canvas_w, canvas_h) = prep.canvas();
    let effective = render::render::resolve_backend(backend);
    let drawn = match render::render::draw_batch_parallel(&prep, effective, &[frame]) {
        Ok(d) => d,
        Err(e) => return Err(inspectize(cmd, tail, e)),
    };
    let rgb = drawn
        .into_iter()
        .next()
        .map(|(_, rgb)| rgb)
        .ok_or_else(|| format!("{cmd} failed: the frame draw returned no picture; {tail}"))?;
    Ok(InspectedDraw {
        comp,
        width: canvas_w,
        height: canvas_h,
        frame,
        time,
        rgb,
        backend: effective.name().to_string(),
    })
}

/// `keyframes` request flags (Sprint 13 check, T006 / CAP-006): the
/// report shape plus the target clip and property. `clip` names a clip
/// `id` (or a zero-based clip index); `property` names a motion
/// property such as `x` or `opacity`.
pub struct KeyframesRequest {
    pub json: bool,
    pub composition: Option<String>,
    pub backend: Option<String>,
    pub variables: Option<String>,
    pub strict_variables: bool,
    pub theme: Option<String>,
    pub clip: Option<String>,
    pub property: Option<String>,
}

/// Normalize one raw `--property` value to its canonical motion name
/// (Sprint 13 check, T006 / CAP-006): case-insensitive, underscores
/// read as hyphens, and the model's aliases (`rotate-x`, `size`,
/// `fill-color`, `border-radius`) resolve to their canonical names.
/// Anything outside the sixteen motion properties refuses.
fn normalize_keyframes_property(raw: &str) -> Option<&'static str> {
    let lower = raw.trim().to_lowercase().replace('_', "-");
    for (alias, canon) in notation::model::MOTION_ALIASES {
        if lower == alias {
            return Some(canon);
        }
    }
    notation::model::MOTION_PROPERTIES
        .iter()
        .find(|p| **p == lower)
        .copied()
}

/// One computed keyframe value (Sprint 13 check, T006 / CAP-006): a
/// scalar number, or a hex color for the two paint properties.
enum KeyframeValue {
    Num(f64),
    Hex(String),
}

/// Format one RGB channel triple as lowercase `#rrggbb` (Sprint 13
/// check, T006 / CAP-006).
fn keyframe_hex(channels: [f64; 3]) -> String {
    let mut hex = String::with_capacity(7);
    hex.push('#');
    for c in channels {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", c.round().clamp(0.0, 255.0) as u8);
    }
    hex
}

/// Read one canonical property out of an evaluated motion state
/// (Sprint 13 check, T006 / CAP-006): the same per-frame values the
/// draw pass evaluates, so the listing matches rendered motion.
fn keyframe_value(property: &str, ev: &render::motion_plan::Evaluated) -> KeyframeValue {
    match property {
        "x" => KeyframeValue::Num(ev.x),
        "y" => KeyframeValue::Num(ev.y),
        "scale" => KeyframeValue::Num(ev.scale),
        "rotation" => KeyframeValue::Num(ev.rotation),
        "opacity" => KeyframeValue::Num(ev.opacity),
        "rotation-x" => KeyframeValue::Num(ev.rotate_x),
        "rotation-y" => KeyframeValue::Num(ev.rotate_y),
        "width" => KeyframeValue::Num(ev.width),
        "height" => KeyframeValue::Num(ev.height),
        "fill" => KeyframeValue::Hex(keyframe_hex(ev.fill)),
        "radius" => KeyframeValue::Num(ev.radius),
        "border-width" => KeyframeValue::Num(ev.border_width),
        "border-color" => KeyframeValue::Hex(keyframe_hex(ev.border)),
        "blur" => KeyframeValue::Num(ev.blur),
        "font-size" => KeyframeValue::Num(ev.font_size),
        "letter-spacing" => KeyframeValue::Num(ev.letter_spacing),
        _ => KeyframeValue::Num(f64::NAN),
    }
}

/// Human and JSON text of one keyframe value (Sprint 13 check, T006 /
/// CAP-006): scalars print trimmed like snapshot times, colors print
/// as `#rrggbb`.
fn keyframe_text(value: &KeyframeValue) -> String {
    match value {
        KeyframeValue::Num(v) => format_snap_time(*v),
        KeyframeValue::Hex(h) => h.clone(),
    }
}

/// Resolve one `--clip` value to a clip index (Sprint 13 check, T006 /
/// CAP-006): a clip `id` first, then a zero-based clip index.
/// Anything else refuses naming the available ids.
fn resolve_keyframes_clip(comp: &Composition, raw: &str) -> Result<(usize, String), String> {
    for (i, clip) in comp.clips.iter().enumerate() {
        if clip.id() == Some(raw) {
            return Ok((i, raw.to_string()));
        }
    }
    if let Ok(i) = raw.trim().parse::<usize>() {
        if i < comp.clips.len() {
            let label = comp.clips[i]
                .id()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("clip {i}"));
            return Ok((i, label));
        }
    }
    let mut ids: Vec<String> = comp
        .clips
        .iter()
        .filter_map(|c| c.id().map(|s| format!("{s:?}")))
        .collect();
    ids.sort();
    Err(format!(
        "keyframes refused: unknown --clip {raw:?}: the composition holds {}; pass one of those ids or a clip index from 0 to {}; nothing was listed",
        if ids.is_empty() {
            "no clip ids".to_string()
        } else {
            ids.join(", ")
        },
        comp.clips.len().saturating_sub(1)
    ))
}

/// `keyframes`: list computed per-frame values of one motion property
/// on one clip (Sprint 13 check, T006 / CAP-006). Values evaluate the
/// same motion plan the draw pass runs, so the listing matches
/// rendered motion. Prints one line per frame (human) or the same
/// rows as JSON. Exits 0 on success, 1 on refusal, 2 when the project
/// is valid but uses later-sprint elements. Never renders and never
/// writes a file.
pub fn cmd_keyframes_full(folder: &Path, req: &KeyframesRequest) -> i32 {
    use std::fmt::Write;
    let json = req.json;
    let run = log::run_id();
    const TAIL: &str = "nothing was listed";
    let fail = |msg: &str| -> i32 {
        stage_line(json, msg);
        note_log(folder, "warn", "keyframes", &run, msg, json);
        if json {
            println!(
                "{{\"tool\":\"framesmith\",\"command\":\"keyframes\",\"folder\":\"{}\",\"status\":\"invalid\",\"exit\":1,\"message\":\"{}\"}}",
                json_escape(&folder.display().to_string()),
                json_escape(msg)
            );
        }
        1
    };
    let clip_raw = match req.clip.as_deref() {
        Some(c) => c,
        None => {
            eprintln!("framesmith keyframes: missing --clip <id>.");
            eprintln!("usage: framesmith keyframes [--json] [--composition <name>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] --clip <id> --property <name> <folder>");
            return 2;
        }
    };
    let property_raw = match req.property.as_deref() {
        Some(p) => p,
        None => {
            eprintln!("framesmith keyframes: missing --property <name>.");
            eprintln!("usage: framesmith keyframes [--json] [--composition <name>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] --clip <id> --property <name> <folder>");
            return 2;
        }
    };
    let property = match normalize_keyframes_property(property_raw) {
        Some(p) => p,
        None => {
            let mut accepted: Vec<&str> = notation::model::MOTION_PROPERTIES.to_vec();
            accepted.sort();
            return fail(&format!(
                "keyframes refused: unknown --property {property_raw:?}: pass one of {}; {TAIL}",
                accepted.join(", ")
            ));
        }
    };
    if let Some(raw) = req.backend.as_deref() {
        if render::render::Backend::parse(raw).is_none() {
            return fail(&format!(
                "keyframes refused: unknown --backend {raw:?}: expected cpu, gpu, or auto; {TAIL}"
            ));
        }
    }
    let overrides = match req.variables.as_deref() {
        None => validate_mod::VariableOverrides::new(),
        Some(raw) => match validate_mod::parse_variable_overrides(raw) {
            Ok(m) => m,
            Err(e) => return fail(&format!("keyframes refused: {e}; {TAIL}")),
        },
    };
    stage_line(json, "stage validate: running");
    let verdict = validate_folder_with_variables_strict_themed(
        folder,
        &overrides,
        req.strict_variables,
        req.theme.as_deref(),
    );
    match verdict.status {
        Status::Invalid => {
            return fail(&format!(
                "keyframes refused: {} is invalid; fix the problems `check` reports and re-run. {TAIL}.",
                folder.display()
            ));
        }
        Status::Unsupported => {
            let msg = format!(
                "keyframes refused: {} uses later-sprint element(s); re-run once the project checks green. {TAIL}.",
                folder.display()
            );
            stage_line(json, &msg);
            note_log(folder, "warn", "keyframes", &run, &msg, json);
            if json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"keyframes\",\"folder\":\"{}\",\"status\":\"unsupported\",\"exit\":2,\"message\":\"{}\"}}",
                    json_escape(&folder.display().to_string()),
                    json_escape(&msg)
                );
            }
            return 2;
        }
        Status::Valid => {
            stage_line(json, "stage validate: ok");
        }
    }
    let comp = match select_composition(&verdict, req.composition.as_deref()) {
        Ok(c) => c,
        Err(e) => return fail(&inspectize("keyframes", TAIL, e)),
    };
    let (clip_idx, clip_label) = match resolve_keyframes_clip(&comp, clip_raw) {
        Ok(v) => v,
        Err(msg) => return fail(&msg),
    };
    let motions =
        render::motion_plan::plan_motions(&comp, render::motion_plan::snap_fps(&comp.fps));
    let frames_total = comp.fps.frame_count(comp.duration);
    if !json {
        println!(
            "keyframes for clip {:?} property {:?} in composition {:?} ({} frame(s) at {} fps).",
            clip_label, property, comp.name, frames_total, comp.fps
        );
    }
    let mut rows: Vec<(u64, f64, String, bool)> = Vec::new();
    for k in 0..frames_total {
        let t = render::motion_plan::frame_time(&comp.fps, k);
        let time = k as f64 / comp.fps.value();
        let ev = motions[clip_idx].values_at(t);
        let value = keyframe_value(property, &ev);
        let text = keyframe_text(&value);
        let is_num = matches!(value, KeyframeValue::Num(_));
        if !json {
            println!(
                "frame {k} at {}s: {} {}={}",
                format_snap_time(time),
                clip_label,
                property,
                text
            );
        }
        rows.push((k, time, text, is_num));
    }
    note_log(
        folder,
        "info",
        "keyframes",
        &run,
        &format!(
            "keyframes ok: clip {clip_label:?} property {property:?}, {frames_total} frame(s)"
        ),
        json,
    );
    if json {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"tool\":\"framesmith\",\"command\":\"keyframes\",\"folder\":\"{}\",\"status\":\"valid\",\"exit\":0,\"composition\":\"{}\",\"clip\":\"{}\",\"clip_index\":{},\"property\":\"{}\",\"frames\":[",
            json_escape(&folder.display().to_string()),
            json_escape(&comp.name),
            json_escape(&clip_label),
            clip_idx,
            json_escape(property)
        );
        for (i, (k, time, text, is_num)) in rows.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if *is_num {
                let _ = write!(out, "{{\"frame\":{k},\"time\":{time},\"value\":{text}}}");
            } else {
                let _ = write!(
                    out,
                    "{{\"frame\":{k},\"time\":{time},\"value\":\"{}\"}}",
                    json_escape(text)
                );
            }
        }
        out.push_str("]}");
        println!("{out}");
    }
    0
}

/// `compare` request flags (Sprint 13 check, T006 / CAP-006): the two
/// projects travel as function arguments; the request carries the
/// report shape, the shared composition name and time, and the output
/// file. `--at` defaults to the first frame.
pub struct CompareRequest {
    pub json: bool,
    pub composition: Option<String>,
    pub at: Option<String>,
    pub backend: Option<String>,
    pub variables: Option<String>,
    pub strict_variables: bool,
    pub theme: Option<String>,
    pub out: Option<String>,
}

/// Machine-readable `compare` report (Sprint 13 check, T006 / CAP-006):
/// the two projects, the drawn frames, the SSIM score against its bar,
/// and the written file.
#[allow(clippy::too_many_arguments)]
fn format_compare_json(
    folder_a: &Path,
    folder_b: &Path,
    status: &str,
    exit: i32,
    composition: Option<&str>,
    frame_a: Option<u64>,
    time_a: Option<f64>,
    frame_b: Option<u64>,
    time_b: Option<f64>,
    ssim: Option<f64>,
    pass: Option<bool>,
    file: &str,
    message: Option<&str>,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"tool\":\"framesmith\",\"command\":\"compare\",\"folder_a\":\"{}\",\"folder_b\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"composition\":",
        json_escape(&folder_a.display().to_string()),
        json_escape(&folder_b.display().to_string())
    );
    match composition {
        Some(c) => {
            let _ = write!(out, "\"{}\"", json_escape(c));
        }
        None => out.push_str("null"),
    }
    let opt = |v: Option<u64>| {
        v.map(|n| n.to_string())
            .unwrap_or_else(|| "null".to_string())
    };
    let optf = |v: Option<f64>| {
        v.map(|n| n.to_string())
            .unwrap_or_else(|| "null".to_string())
    };
    let _ = write!(
        out,
        ",\"frame_a\":{},\"time_a\":{},\"frame_b\":{},\"time_b\":{}",
        opt(frame_a),
        optf(time_a),
        opt(frame_b),
        optf(time_b)
    );
    match ssim {
        Some(s) => {
            let _ = write!(out, ",\"ssim\":{s},\"bar\":{COMPARE_SSIM_BAR}");
        }
        None => out.push_str(",\"ssim\":null,\"bar\":null"),
    }
    match pass {
        Some(p) => {
            let _ = write!(out, ",\"pass\":{p}");
        }
        None => out.push_str(",\"pass\":null"),
    }
    let _ = write!(out, ",\"file\":\"{}\"", json_escape(file));
    match message {
        Some(m) => {
            let _ = write!(out, ",\"message\":\"{}\"", json_escape(m));
        }
        None => out.push_str(",\"message\":null"),
    }
    out.push('}');
    out
}

/// `compare`: render one frame of two projects, join them side by side
/// (left is the first project, right is the second), and score the
/// pair with SSIM against the 0.96 bar (Sprint 13 check, T006 /
/// CAP-006). Identical projects score exactly 1.0 and the file is
/// still written. Both canvases must match; anything else refuses
/// naming the sizes with nothing written. Exits 0 whenever the file
/// and the score are produced (a below-bar score is a reviewable
/// result, not a failure), 1 on refusal, 2 on later-sprint elements.
/// Never writes an MP4; the only file written is `-o`.
pub fn cmd_compare_full(folder_a: &Path, folder_b: &Path, req: &CompareRequest) -> i32 {
    let json = req.json;
    let run = log::run_id();
    const TAIL: &str = "no file was written";
    let out_path = match req.out.as_deref() {
        Some(raw) if !raw.trim().is_empty() => PathBuf::from(raw),
        _ => {
            eprintln!("framesmith compare: missing -o <out.png>.");
            eprintln!("usage: framesmith compare [--json] [--composition <name>] [--at <secs>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] -o <out.png> <folderA> <folderB>");
            return 2;
        }
    };
    let fail = |msg: &str, status: &str, exit: i32| -> i32 {
        stage_line(json, msg);
        note_log(folder_a, "warn", "compare", &run, msg, json);
        if json {
            println!(
                "{}",
                format_compare_json(
                    folder_a,
                    folder_b,
                    status,
                    exit,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    "",
                    Some(msg)
                )
            );
        }
        exit
    };
    for (role, folder) in [("first", folder_a), ("second", folder_b)] {
        if !folder.is_dir() {
            return fail(
                &format!(
                    "compare refused: the {role} project {} has no project directory; pass two existing project folders. {TAIL}.",
                    folder.display()
                ),
                "invalid",
                1,
            );
        }
    }
    let at = match req.at.as_deref() {
        None => 0.0,
        Some(raw) => match parse_inspect_seconds("compare", TAIL, raw) {
            Ok(v) => v,
            Err(msg) => return fail(&msg, "invalid", 1),
        },
    };
    let backend_raw = req.backend.as_deref().unwrap_or("auto");
    let overrides = match req.variables.as_deref() {
        None => validate_mod::VariableOverrides::new(),
        Some(raw) => match validate_mod::parse_variable_overrides(raw) {
            Ok(m) => m,
            Err(e) => return fail(&format!("compare refused: {e}; {TAIL}"), "invalid", 1),
        },
    };
    stage_line(json, "stage validate: running");
    let left = match draw_inspect_frame(
        "compare",
        TAIL,
        folder_a,
        req.composition.as_deref(),
        backend_raw,
        &overrides,
        req.strict_variables,
        req.theme.as_deref(),
        at,
        false,
    ) {
        Ok(d) => d,
        Err(msg) => {
            let status = if msg.contains("later-sprint") {
                "unsupported"
            } else {
                "invalid"
            };
            let exit = if status == "unsupported" { 2 } else { 1 };
            return fail(&msg, status, exit);
        }
    };
    let right = match draw_inspect_frame(
        "compare",
        TAIL,
        folder_b,
        req.composition.as_deref(),
        backend_raw,
        &overrides,
        req.strict_variables,
        req.theme.as_deref(),
        at,
        false,
    ) {
        Ok(d) => d,
        Err(msg) => {
            let status = if msg.contains("later-sprint") {
                "unsupported"
            } else {
                "invalid"
            };
            let exit = if status == "unsupported" { 2 } else { 1 };
            return fail(&msg, status, exit);
        }
    };
    if left.width != right.width || left.height != right.height {
        return fail(
            &format!(
                "compare refused: canvas sizes differ (first project {}x{}, second project {}x{}): compare projects with the same canvas. {TAIL}.",
                left.width, left.height, right.width, right.height
            ),
            "invalid",
            1,
        );
    }
    let pixels = left.width as usize * left.height as usize;
    let score = ssim_luma(&left.rgb, &right.rgb, pixels);
    let pass = score + 1e-9 >= COMPARE_SSIM_BAR;
    let joined = join_side_by_side(&left.rgb, &right.rgb, left.width, left.height);
    let bytes = match render::outputs::encode_png_rgb8(left.width * 2, left.height, &joined) {
        Ok(b) => b,
        Err(msg) => {
            return fail(
                &format!(
                    "compare failed: cannot encode {}: {msg}; {TAIL}",
                    out_path.display()
                ),
                "failed",
                1,
            );
        }
    };
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return fail(
                    &format!(
                        "compare refused: cannot create output directory {}: {e}; {TAIL}",
                        parent.display()
                    ),
                    "invalid",
                    1,
                );
            }
        }
    }
    if let Err(e) = std::fs::write(&out_path, &bytes) {
        return fail(
            &format!(
                "compare failed: cannot write {}: {e}; no complete comparison was written",
                out_path.display()
            ),
            "failed",
            1,
        );
    }
    let verdict = if pass { "pass" } else { "below bar" };
    stage_line(
        json,
        &format!(
            "compare: drew frame {} at {}s from {} and frame {} at {}s from {} (composition {:?}, backend {}).",
            left.frame,
            format_snap_time(left.time),
            folder_a.display(),
            right.frame,
            format_snap_time(right.time),
            folder_b.display(),
            left.comp.name,
            left.backend
        ),
    );
    stage_line(
        json,
        &format!(
            "compare: wrote {} (left {}, right {}, each {}x{}, side-by-side {}x{}).",
            out_path.display(),
            folder_a.display(),
            folder_b.display(),
            left.width,
            left.height,
            left.width * 2,
            left.height
        ),
    );
    stage_line(
        json,
        &format!("compare: ssim {score} against bar {COMPARE_SSIM_BAR}: {verdict}."),
    );
    note_log(
        folder_a,
        "info",
        "compare",
        &run,
        &format!(
            "compare ok: ssim {score} ({verdict}) in {}",
            out_path.display()
        ),
        json,
    );
    if json {
        println!(
            "{}",
            format_compare_json(
                folder_a,
                folder_b,
                "ok",
                0,
                Some(&left.comp.name),
                Some(left.frame),
                Some(left.time),
                Some(right.frame),
                Some(right.time),
                Some(score),
                Some(pass),
                &out_path.display().to_string(),
                None,
            )
        );
    }
    0
}

/// `grade-compare` request flags (Sprint 13 check, T006 / CAP-006):
/// the report shape, the selected composition and time, and the
/// output file. `--at` defaults to the first frame.
pub struct GradeCompareRequest {
    pub json: bool,
    pub composition: Option<String>,
    pub at: Option<String>,
    pub backend: Option<String>,
    pub variables: Option<String>,
    pub strict_variables: bool,
    pub theme: Option<String>,
    pub out: Option<String>,
}

/// `grade-compare`: render one frame of a composition twice — with
/// its grades and without its composition and lane grades — and join
/// the halves split screen (left ungraded, right graded) into `-o`
/// (Sprint 13 check, T006 / CAP-006). A composition with no
/// composition or lane grade refuses naming the gap, since both halves
/// would be the same picture. Exits 0 on success, 1 on refusal, 2 on
/// later-sprint elements. Never writes an MP4; the only file written
/// is `-o`.
pub fn cmd_grade_compare_full(folder: &Path, req: &GradeCompareRequest) -> i32 {
    use std::fmt::Write;
    let json = req.json;
    let run = log::run_id();
    const TAIL: &str = "no file was written";
    let out_path = match req.out.as_deref() {
        Some(raw) if !raw.trim().is_empty() => PathBuf::from(raw),
        _ => {
            eprintln!("framesmith grade-compare: missing -o <out.png>.");
            eprintln!("usage: framesmith grade-compare [--json] [--composition <name>] [--at <secs>] [--backend cpu|gpu|auto] [--variables JSON] [--strict-variables] [--theme NAME] -o <out.png> <folder>");
            return 2;
        }
    };
    let fail = |msg: &str, status: &str, exit: i32| -> i32 {
        stage_line(json, msg);
        note_log(folder, "warn", "grade-compare", &run, msg, json);
        if json {
            let mut out = String::new();
            let _ = write!(
                out,
                "{{\"tool\":\"framesmith\",\"command\":\"grade-compare\",\"folder\":\"{}\",\"status\":\"{status}\",\"exit\":{exit},\"message\":\"{}\"}}",
                json_escape(&folder.display().to_string()),
                json_escape(msg)
            );
            println!("{out}");
        }
        exit
    };
    if !folder.is_dir() {
        return fail(
            &format!(
                "grade-compare refused: {} has no project directory; pass an existing project folder. {TAIL}.",
                folder.display()
            ),
            "invalid",
            1,
        );
    }
    let at = match req.at.as_deref() {
        None => 0.0,
        Some(raw) => match parse_inspect_seconds("grade-compare", TAIL, raw) {
            Ok(v) => v,
            Err(msg) => return fail(&msg, "invalid", 1),
        },
    };
    let backend_raw = req.backend.as_deref().unwrap_or("auto");
    let overrides = match req.variables.as_deref() {
        None => validate_mod::VariableOverrides::new(),
        Some(raw) => match validate_mod::parse_variable_overrides(raw) {
            Ok(m) => m,
            Err(e) => return fail(&format!("grade-compare refused: {e}; {TAIL}"), "invalid", 1),
        },
    };
    stage_line(json, "stage validate: running");
    // The grade check runs on the selected composition before any
    // draw: an identity chain would print two identical halves, so it
    // refuses naming the composition instead.
    let graded = match draw_inspect_frame(
        "grade-compare",
        TAIL,
        folder,
        req.composition.as_deref(),
        backend_raw,
        &overrides,
        req.strict_variables,
        req.theme.as_deref(),
        at,
        false,
    ) {
        Ok(d) => d,
        Err(msg) => {
            let status = if msg.contains("later-sprint") {
                "unsupported"
            } else {
                "invalid"
            };
            let exit = if status == "unsupported" { 2 } else { 1 };
            return fail(&msg, status, exit);
        }
    };
    let has_grade = {
        let resolved = notation::model::ResolvedGrade::identity().layer(&graded.comp.grade);
        !resolved.is_empty()
            || graded.comp.lane_grades.iter().any(|lane| {
                !notation::model::ResolvedGrade::identity()
                    .layer(&lane.grade)
                    .is_empty()
            })
    };
    if !has_grade {
        return fail(
            &format!(
                "grade-compare refused: composition {:?} carries no composition or lane grade to compare (an identity chain shows two identical halves); tune a `grade` on the composition and re-run. {TAIL}.",
                graded.comp.name
            ),
            "invalid",
            1,
        );
    }
    let plain = match draw_inspect_frame(
        "grade-compare",
        TAIL,
        folder,
        req.composition.as_deref(),
        backend_raw,
        &overrides,
        req.strict_variables,
        req.theme.as_deref(),
        at,
        true,
    ) {
        Ok(d) => d,
        Err(msg) => {
            let status = if msg.contains("later-sprint") {
                "unsupported"
            } else {
                "invalid"
            };
            let exit = if status == "unsupported" { 2 } else { 1 };
            return fail(&msg, status, exit);
        }
    };
    let joined = join_side_by_side(&plain.rgb, &graded.rgb, graded.width, graded.height);
    let bytes = match render::outputs::encode_png_rgb8(graded.width * 2, graded.height, &joined) {
        Ok(b) => b,
        Err(msg) => {
            return fail(
                &format!(
                    "grade-compare failed: cannot encode {}: {msg}; {TAIL}",
                    out_path.display()
                ),
                "failed",
                1,
            );
        }
    };
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return fail(
                    &format!(
                        "grade-compare refused: cannot create output directory {}: {e}; {TAIL}",
                        parent.display()
                    ),
                    "invalid",
                    1,
                );
            }
        }
    }
    if let Err(e) = std::fs::write(&out_path, &bytes) {
        return fail(
            &format!(
                "grade-compare failed: cannot write {}: {e}; no complete comparison was written",
                out_path.display()
            ),
            "failed",
            1,
        );
    }
    stage_line(
        json,
        &format!(
            "grade-compare: drew frame {} at {}s from {} (composition {:?}, backend {}).",
            graded.frame,
            format_snap_time(graded.time),
            folder.display(),
            graded.comp.name,
            graded.backend
        ),
    );
    stage_line(
        json,
        &format!(
            "grade-compare: wrote {} (left ungraded without composition or lane grades, right graded, each {}x{}, split {}x{}).",
            out_path.display(),
            graded.width,
            graded.height,
            graded.width * 2,
            graded.height
        ),
    );
    note_log(
        folder,
        "info",
        "grade-compare",
        &run,
        &format!("grade-compare ok: {}", out_path.display()),
        json,
    );
    if json {
        println!(
            "{{\"tool\":\"framesmith\",\"command\":\"grade-compare\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"composition\":\"{}\",\"frame\":{},\"time\":{},\"file\":\"{}\",\"message\":null}}",
            json_escape(&folder.display().to_string()),
            json_escape(&graded.comp.name),
            graded.frame,
            graded.time,
            json_escape(&out_path.display().to_string())
        );
    }
    0
}

/// Today's UTC date as `YYYY-MM-DD` (Sprint 13 check, T006 / CAP-006):
/// the civil-date conversion over Unix days, so `benchmark` needs no
/// date dependency to stamp its machine-specific timings.
fn utc_today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let z = secs as i64 / 86_400 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    y += if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// `doctor`: report environment and project health facts (Sprint 13
/// check, T006 / CAP-006). Without a folder, reports the machine only
/// (Framesmith version, FFmpeg finding, GPU probe, text-shaping face,
/// OS and architecture). With a folder, validates the project and
/// adds its status, compositions, and canvas facts; a missing folder
/// or an invalid project refuses naming the cause. Read-only: prints
/// the report and writes nothing. Exits 0 when the report is
/// complete, 1 when the project cannot be reported, 2 on usage errors
/// or later-sprint elements.
pub fn cmd_doctor(folder: Option<&Path>, json: bool) -> i32 {
    use std::fmt::Write;
    let run = log::run_id();
    let framesmith = env!("CARGO_PKG_VERSION");
    let finding = ffmpeg_mod::detect();
    let ffmpeg_text = finding.to_string();
    let gpu = render::gpu::gpu_available();
    let mut fonts = layout::Fonts::new();
    let default_face = fonts.resolve_system_style(false, false);
    let registered_faces = fonts.machine_faces().len();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let mut out = String::new();
    let _ = writeln!(out, "doctor: framesmith {framesmith}.");
    let _ = writeln!(out, "doctor: {ffmpeg_text}.");
    let _ = writeln!(
        out,
        "doctor: gpu renderer {}.",
        if gpu {
            "available"
        } else {
            "unavailable (cpu draws every frame)"
        }
    );
    match &default_face {
        Some(family) => {
            let _ = writeln!(
                out,
                "doctor: text shaping available (default face {family:?}, {registered_faces} machine face(s) registered)."
            );
        }
        None => {
            let _ = writeln!(
                out,
                "doctor: text shaping unavailable (no system face resolved; set FRAMESMITH_SYSTEM_FONT to a readable font file)."
            );
        }
    }
    let _ = writeln!(out, "doctor: host {os}/{arch}.");
    // Sprint 25 hardware encoders (T002 / INT-003): the same
    // resolution the render path uses, probed from the host FFmpeg
    // listing, so doctor and renders never drift. Read-only: doctor
    // never downloads anything.
    let hw_encoders: Vec<&str> = finding
        .ffmpeg_path
        .as_deref()
        .map(render::outputs::probe_hardware_encoders)
        .unwrap_or_default();
    let auto_codec = finding
        .ffmpeg_path
        .as_deref()
        .and_then(|bin| render::outputs::resolve_auto(bin, render::outputs::OutputFormat::Mp4))
        .map(|(codec, _choice)| codec);
    let hw_text = if hw_encoders.is_empty() {
        "none (software encodes only)".to_string()
    } else {
        hw_encoders.join(", ")
    };
    let auto_text = match auto_codec {
        Some(codec) => format!(
            "{codec} (from --encoder auto on {os}/{arch}; software stays the default)"
        ),
        None => format!("software fallback (no hardware encoder for {os}/{arch} H.264)"),
    };
    let _ = writeln!(out, "doctor: hardware encoders: {hw_text}.");
    let _ = writeln!(out, "doctor: auto encoder: {auto_text}.");
    let hw_json = format!(
        ",\"hardware_encoders\":[{}],\"auto_encoder\":{}",
        hw_encoders
            .iter()
            .map(|e| format!("\"{}\"", json_escape(e)))
            .collect::<Vec<_>>()
            .join(","),
        match auto_codec {
            Some(codec) => format!("\"{}\"", json_escape(codec)),
            None => "\"software\"".to_string(),
        }
    );
    // Sprint 23 ML health (T001 / INT-003): the same resolution the
    // runners use, so health output and runner behavior agree.
    let ml_cfg = ml::MlConfig::from_env();
    let ml_lines = ml::doctor::doctor_lines(&ml_cfg);
    for line in &ml_lines {
        let _ = writeln!(out, "{line}");
    }
    let mut ml_json = String::from(",\"ml_engines\":[");
    let mut first = true;
    for (name, kind, found) in ml::engines::engine_health() {
        if !first {
            ml_json.push(',');
        }
        first = false;
        match found {
            Some(where_) => {
                let _ = write!(
                    ml_json,
                    "{{\"name\":\"{}\",\"kind\":\"{}\",\"present\":true,\"path\":\"{}\"}}",
                    json_escape(&name),
                    json_escape(&kind),
                    json_escape(&where_)
                );
            }
            None => {
                let _ = write!(
                    ml_json,
                    "{{\"name\":\"{}\",\"kind\":\"{}\",\"present\":false,\"path\":null}}",
                    json_escape(&name),
                    json_escape(&kind)
                );
            }
        }
    }
    ml_json.push_str("],\"ml_models\":[");
    first = true;
    for m in ml::KNOWN_MODELS {
        if !first {
            ml_json.push(',');
        }
        first = false;
        let state = match ml::model_status(&ml_cfg, m) {
            ml::ModelState::Ready => "ready",
            ml::ModelState::Missing => "missing",
        };
        let _ = write!(
            ml_json,
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"engine\":\"{}\",\"state\":\"{state}\"}}",
            json_escape(m.name),
            json_escape(m.kind),
            json_escape(m.engine)
        );
    }
    ml_json.push(']');
    if let Some(dir) = folder {
        if !dir.is_dir() {
            let msg = format!(
                "doctor refused: {} has no project directory; pass an existing project folder or run without one for machine facts only.",
                dir.display()
            );
            if json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"doctor\",\"folder\":\"{}\",\"status\":\"invalid\",\"exit\":1,\"message\":\"{}\"}}",
                    json_escape(&dir.display().to_string()),
                    json_escape(&msg)
                );
            } else {
                print!("{out}");
                println!("{msg}");
            }
            note_log(dir, "warn", "doctor", &run, &msg, json);
            return 1;
        }
        let verdict = validate_folder(dir);
        let status_text = match verdict.status {
            Status::Valid => "valid",
            Status::Invalid => "invalid",
            Status::Unsupported => "unsupported",
        };
        let _ = writeln!(
            out,
            "doctor: project {} is {status_text} ({} composition(s)).",
            dir.display(),
            verdict.compositions.len()
        );
        for comp in &verdict.compositions {
            let _ = writeln!(
                out,
                "doctor: composition {:?}: {}x{} {} fps duration {:.3}s, {} clip(s).",
                comp.name,
                comp.width,
                comp.height,
                comp.fps,
                comp.duration,
                comp.clips.len()
            );
        }
        if json {
            let mut job = String::new();
            let _ = write!(
                job,
                "{{\"tool\":\"framesmith\",\"command\":\"doctor\",\"folder\":\"{}\",\"status\":\"{status_text}\",\"exit\":{},\"framesmith\":\"{framesmith}\",\"ffmpeg_suitable\":{},\"ffmpeg\":\"{}\",\"gpu_available\":{gpu},\"text_shaping\":",
                json_escape(&dir.display().to_string()),
                if verdict.status == Status::Valid { 0 } else if verdict.status == Status::Invalid { 1 } else { 2 },
                finding.suitable,
                json_escape(&ffmpeg_text)
            );
            match &default_face {
                Some(family) => {
                    let _ = write!(job, "\"{}\"", json_escape(family));
                }
                None => job.push_str("null"),
            }
            let _ = write!(
                job,
                ",\"os\":\"{os}\",\"arch\":\"{arch}\"{hw_json}{ml_json},\"compositions\":["
            );
            for (i, comp) in verdict.compositions.iter().enumerate() {
                if i > 0 {
                    job.push(',');
                }
                let _ = write!(
                    job,
                    "{{\"name\":\"{}\",\"width\":{},\"height\":{},\"duration\":{},\"fps\":\"{}\",\"clips\":{}}}",
                    json_escape(&comp.name),
                    comp.width,
                    comp.height,
                    comp.duration,
                    comp.fps,
                    comp.clips.len()
                );
            }
            job.push_str("],\"diagnostics\":[");
            for (i, d) in verdict.diagnostics.iter().enumerate() {
                if i > 0 {
                    job.push(',');
                }
                let _ = write!(job, "\"{}\"", json_escape(&d.to_string()));
            }
            job.push_str("]}");
            println!("{job}");
        } else {
            print!("{out}");
            for d in &verdict.diagnostics {
                println!("doctor: {d}");
            }
        }
        let code = match verdict.status {
            Status::Valid => 0,
            Status::Invalid => 1,
            Status::Unsupported => 2,
        };
        note_log(
            dir,
            if code == 0 { "info" } else { "warn" },
            "doctor",
            &run,
            &format!(
                "doctor {status_text}: {} composition(s)",
                verdict.compositions.len()
            ),
            json,
        );
        return code;
    }
    if json {
        let mut job = String::new();
        let _ = write!(
            job,
            "{{\"tool\":\"framesmith\",\"command\":\"doctor\",\"folder\":null,\"status\":\"ok\",\"exit\":0,\"framesmith\":\"{framesmith}\",\"ffmpeg_suitable\":{},\"ffmpeg\":\"{}\",\"gpu_available\":{gpu},\"text_shaping\":",
            finding.suitable,
            json_escape(&ffmpeg_text)
        );
        match &default_face {
            Some(family) => {
                let _ = write!(job, "\"{}\"", json_escape(family));
            }
            None => job.push_str("null"),
        }
        let _ = write!(job, ",\"os\":\"{os}\",\"arch\":\"{arch}\"{hw_json}{ml_json}}}");
        println!("{job}");
    } else {
        print!("{out}");
    }
    0
}

/// `benchmark` request flags (Sprint 13 check, T006 / CAP-006):
/// `--out` optionally keeps one PNG per bundled reference.
pub struct BenchmarkRequest {
    pub json: bool,
    pub out: Option<String>,
}

/// One bundled benchmark reference (Sprint 13 check, T006 /
/// CAP-006): a small KDL composition rendered in full through the
/// normal pipeline, so the timing measures real drawing.
struct BenchmarkRef {
    name: &'static str,
    kdl: &'static str,
}

/// The bundled references (Sprint 13 check, T006 / CAP-006): a static
/// two-box card plus a moving box over a text line, both tiny so the
/// command stays fast on any machine.
const BENCHMARK_REFS: [BenchmarkRef; 2] = [
    BenchmarkRef {
        name: "still",
        kdl: "composition name=\"still\" width=96 height=96 duration=1 fps=10 background=\"#101820\" version=1 {\n box id=\"a\" start=0 duration=1 x=8 y=8 width=80 height=36 fill=\"#e0a100\"\n box id=\"b\" start=0 duration=1 x=8 y=52 width=80 height=36 fill=\"#2e6fb0\"\n}\n",
    },
    BenchmarkRef {
        name: "slide",
        kdl: "composition name=\"slide\" width=128 height=128 duration=1 fps=10 background=\"#0c0c0c\" version=1 {\n text id=\"t\" value=\"bench\" start=0 duration=1 x=8 y=8 size=20 color=\"#ffffff\"\n box id=\"m\" start=0 duration=1 x=0 y=80 width=24 height=24 fill=\"#ff4040\"\n tween target=1 property=\"x\" kind=\"to\" start=0 duration=1 to=104 ease=\"linear\"\n}\n",
    },
];

/// One timed reference render (Sprint 13 check, T006 / CAP-006): the
/// frame count, the canvas, both pass timings, whether the two passes
/// drew byte-identical pictures, and the first frame's picture.
struct BenchmarkResult {
    name: &'static str,
    frames: u64,
    width: u32,
    height: u32,
    ms_first: u128,
    ms_second: u128,
    deterministic: bool,
    first_rgb: Vec<u8>,
}

/// `benchmark`: render the bundled references twice each, report
/// per-reference timings with versions and date, and label the
/// numbers machine-specific (Sprint 13 check, T006 / CAP-006). With
/// `--out`, keeps one PNG per reference. An unrunnable reference
/// (refused validation or a failed draw) fails naming the reference
/// and the cause with nothing written — not even a partial PNG set.
/// Never writes an MP4. Exits 0 on success, 1 on refusal or failure,
/// 2 on usage errors.
pub fn cmd_benchmark(req: &BenchmarkRequest) -> i32 {
    use std::fmt::Write;
    let json = req.json;
    let run = log::run_id();
    const TAIL: &str = "nothing was written";
    static BENCH_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let fail = |msg: &str| -> i32 {
        stage_line(json, msg);
        if json {
            println!(
                "{{\"tool\":\"framesmith\",\"command\":\"benchmark\",\"status\":\"failed\",\"exit\":1,\"message\":\"{}\"}}",
                json_escape(msg)
            );
        }
        1
    };
    let out_dir = match req.out.as_deref() {
        None => None,
        Some(raw) if !raw.trim().is_empty() => Some(PathBuf::from(raw)),
        Some(_) => {
            eprintln!("framesmith benchmark: --out needs a value (an output directory).");
            eprintln!("usage: framesmith benchmark [--json] [--out <dir>]");
            return 2;
        }
    };
    let finding = ffmpeg_mod::detect();
    let ffmpeg_text = finding.to_string();
    let ffmpeg_version = finding
        .version
        .clone()
        .unwrap_or_else(|| "not found".to_string());
    let ffmpeg_bin = finding
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let base = std::env::temp_dir().join(format!(
        "framesmith-benchmark-{}-{}",
        std::process::id(),
        BENCH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&base);
    let mut results: Vec<BenchmarkResult> = Vec::new();
    let mut outcome: Result<(), String> = Ok(());
    for reference in &BENCHMARK_REFS {
        if outcome.is_err() {
            break;
        }
        let dir = base.join(reference.name);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            outcome = Err(format!(
                "benchmark refused: bundled reference {:?} has no scratch space: {e}; {TAIL}",
                reference.name
            ));
            break;
        }
        if let Err(e) = std::fs::write(dir.join("composition.kdl"), reference.kdl) {
            outcome = Err(format!(
                "benchmark refused: bundled reference {:?} cannot stage its composition: {e}; {TAIL}",
                reference.name
            ));
            break;
        }
        let verdict = validate_folder(&dir);
        if verdict.status != Status::Valid {
            let problems: Vec<String> = verdict.diagnostics.iter().map(|d| d.to_string()).collect();
            outcome = Err(format!(
                "benchmark refused: bundled reference {:?} is unrunnable: {}; {TAIL}",
                reference.name,
                if problems.is_empty() {
                    "validation failed".to_string()
                } else {
                    problems.join("; ")
                }
            ));
            break;
        }
        let comp = match select_composition(&verdict, None) {
            Ok(c) => c,
            Err(e) => {
                outcome = Err(inspectize("benchmark", TAIL, e));
                break;
            }
        };
        let prep =
            match render::render::prepare_frames(&comp, &verdict.compositions, &dir, &ffmpeg_bin) {
                Ok(prep) => prep,
                Err(e) => {
                    outcome = Err(inspectize("benchmark", TAIL, e));
                    break;
                }
            };
        let (w, h) = prep.canvas();
        let frames_total = comp.fps.frame_count(comp.duration);
        let frames: Vec<u64> = (0..frames_total).collect();
        let backend = render::render::resolve_backend(render::render::Backend::Auto);
        let start = std::time::Instant::now();
        let first = match render::render::draw_batch_parallel(&prep, backend, &frames) {
            Ok(d) => d,
            Err(e) => {
                outcome = Err(inspectize("benchmark", TAIL, e));
                break;
            }
        };
        let ms_first = start.elapsed().as_millis();
        let start = std::time::Instant::now();
        let second = match render::render::draw_batch_parallel(&prep, backend, &frames) {
            Ok(d) => d,
            Err(e) => {
                outcome = Err(inspectize("benchmark", TAIL, e));
                break;
            }
        };
        let ms_second = start.elapsed().as_millis();
        let deterministic = first == second;
        let first_rgb = first
            .into_iter()
            .next()
            .map(|(_, rgb)| rgb)
            .unwrap_or_default();
        results.push(BenchmarkResult {
            name: reference.name,
            frames: frames_total,
            width: w,
            height: h,
            ms_first,
            ms_second,
            deterministic,
            first_rgb,
        });
    }
    if let Err(msg) = &outcome {
        let _ = std::fs::remove_dir_all(&base);
        return fail(msg);
    }
    if !results.iter().all(|r| r.deterministic) {
        let names: Vec<&str> = results
            .iter()
            .filter(|r| !r.deterministic)
            .map(|r| r.name)
            .collect();
        let _ = std::fs::remove_dir_all(&base);
        return fail(&format!(
            "benchmark failed: reference(s) {} drew different bytes across passes; {TAIL}",
            names.join(", ")
        ));
    }
    // Every PNG encodes before any file is written, so an encoding
    // problem still leaves no partial benchmark set behind.
    let mut encoded: Vec<(String, Vec<u8>)> = Vec::new();
    for r in &results {
        match render::outputs::encode_png_rgb8(r.width, r.height, &r.first_rgb) {
            Ok(bytes) => encoded.push((format!("benchmark-{}.png", r.name), bytes)),
            Err(msg) => {
                let _ = std::fs::remove_dir_all(&base);
                return fail(&format!(
                    "benchmark failed: cannot encode reference {:?}: {msg}; {TAIL}",
                    r.name
                ));
            }
        }
    }
    if let Some(dir) = &out_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            let _ = std::fs::remove_dir_all(&base);
            return fail(&format!(
                "benchmark refused: cannot create output directory {}: {e}; {TAIL}",
                dir.display()
            ));
        }
        let probe = dir.join(".framesmith-write-probe");
        match std::fs::write(&probe, b"framesmith writability probe") {
            Ok(()) => {
                let _ = std::fs::remove_file(&probe);
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&base);
                return fail(&format!(
                    "benchmark refused: output directory {} is not writable: {e}; {TAIL}",
                    dir.display()
                ));
            }
        }
        let mut written: Vec<String> = Vec::new();
        for (name, bytes) in &encoded {
            let path = dir.join(name);
            if let Err(e) = std::fs::write(&path, bytes) {
                for done in &written {
                    let _ = std::fs::remove_file(dir.join(done));
                }
                let _ = std::fs::remove_dir_all(&base);
                return fail(&format!(
                    "benchmark failed: cannot write {}: {e}; no complete benchmark set was written",
                    path.display()
                ));
            }
            written.push(name.clone());
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    let framesmith = env!("CARGO_PKG_VERSION");
    let date = utc_today();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let total_ms: u128 = results.iter().map(|r| r.ms_first + r.ms_second).sum();
    if !json {
        println!("benchmark: framesmith {framesmith}, ffmpeg {ffmpeg_version}, {os}/{arch}, date {date}.");
        for r in &results {
            println!(
                "benchmark: reference {:?}: {} frame(s) {}x{} in {}ms then {}ms (deterministic yes).",
                r.name, r.frames, r.width, r.height, r.ms_first, r.ms_second
            );
        }
        if let Some(dir) = &out_dir {
            println!(
                "benchmark: wrote {} PNG(s) to {}.",
                encoded.len(),
                dir.display()
            );
        }
        println!(
            "benchmark: {total_ms}ms total; timings are machine-specific: measured on this machine only, never a guarantee or a cross-machine comparison."
        );
        let _ = ffmpeg_text;
        let _ = run;
    } else {
        let mut job = String::new();
        let _ = write!(
            job,
            "{{\"tool\":\"framesmith\",\"command\":\"benchmark\",\"status\":\"ok\",\"exit\":0,\"framesmith\":\"{framesmith}\",\"ffmpeg\":\"{}\",\"os\":\"{os}\",\"arch\":\"{arch}\",\"date\":\"{date}\",\"machine_specific\":true,\"references\":[",
            json_escape(&ffmpeg_version)
        );
        for (i, r) in results.iter().enumerate() {
            if i > 0 {
                job.push(',');
            }
            let _ = write!(
                job,
                "{{\"name\":\"{}\",\"frames\":{},\"width\":{},\"height\":{},\"ms_first\":{},\"ms_second\":{},\"deterministic\":true}}",
                json_escape(r.name),
                r.frames,
                r.width,
                r.height,
                r.ms_first,
                r.ms_second
            );
        }
        job.push_str("],\"files\":[");
        for (i, (name, _)) in encoded.iter().enumerate() {
            if i > 0 {
                job.push(',');
            }
            let _ = write!(job, "\"{}\"", json_escape(name));
        }
        let _ = write!(job, "],\"message\":null}}");
        println!("{job}");
    }
    0
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

/// Preview daemon commands (Sprint 14, T006 / CAP-006, J-03, J-04,
/// STATE-01, INT-002, REC-001, REC-003).
///
/// `preview` starts one background server per project directory on a
/// free loopback port and prints its URL; `preview-status` names the
/// project and port or reports none; `preview-stop` ends exactly that
/// server; `preview-list` shows every live server; `preview-stop-all`
/// ends all and reports the count. A second start for one directory
/// prints the existing URL; a busy preferred port falls forward with
/// the new URL printed; stopping a dead server reports
/// already-stopped; an empty stop-all reports zero and exits 0; a
/// broken project refuses before any server starts.
#[derive(Debug, Default)]
pub struct PreviewStartRequest {
    /// Raw `--port` flag text. None picks a free port; a busy value
    /// falls forward to the next free one.
    pub port: Option<String>,
    /// Named composition to preview. None previews the root.
    pub composition: Option<String>,
    /// Run the server in this process instead of the background.
    pub foreground: bool,
}

/// Parse raw `--port` text into a preferred port (Sprint 14, T006).
fn parse_preview_port(raw: Option<&str>) -> Result<Option<u16>, String> {
    match raw {
        None => Ok(None),
        Some(text) => text.parse::<u16>().map(Some).map_err(|_| {
            format!("--port needs a value (a port number, for example 8130), found {text:?}")
        }),
    }
}

/// Wait for a spawned `preview-serve` child to record a live entry.
enum StartWait {
    Ready(preview::daemon::ServerEntry),
    ChildExited(Option<i32>),
    Timeout,
}

fn wait_for_server(
    registry: &Path,
    dir: &str,
    port: u16,
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> StartWait {
    let start = std::time::Instant::now();
    loop {
        if let Some(entry) = preview::daemon::find_ready(registry, dir, port) {
            return StartWait::Ready(entry);
        }
        match child.try_wait() {
            Ok(Some(status)) => return StartWait::ChildExited(status.code()),
            Ok(None) => {}
            Err(_) => return StartWait::Timeout,
        }
        if start.elapsed() >= timeout {
            return StartWait::Timeout;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Server log for one spawned child (Sprint 14, T006): the daemon
/// must not inherit the parent's stderr pipe — a caller capturing the
/// parent's output would never see EOF while the server lives — so
/// the child logs to a file and the parent prints its tail when the
/// start fails.
fn serve_log_path(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("framesmith-preview-serve-{port}.log"))
}

/// Last lines of a failed server's log for the start-failure report.
fn serve_log_tail(port: u16) -> String {
    let body = std::fs::read_to_string(serve_log_path(port)).unwrap_or_default();
    let lines: Vec<&str> = body.lines().collect();
    let tail = lines
        .iter()
        .rev()
        .take(20)
        .rev()
        .cloned()
        .collect::<Vec<_>>();
    tail.join("\n")
}

fn spawn_serve_child(
    port: u16,
    registry: &Path,
    folder: &Path,
    composition: Option<&str>,
) -> std::io::Result<std::process::Child> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    // Detach into a new session (Unix): the server must outlive the
    // `preview` parent — and the terminal — so a plain orphan is not
    // enough. `setsid` cannot fail here: a freshly spawned child is
    // never a process-group leader.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let log = std::fs::File::create(serve_log_path(port))?;
    cmd.arg("preview-serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--registry")
        .arg(registry)
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log);
    if let Some(name) = composition {
        cmd.arg("--composition").arg(name);
    }
    cmd.spawn()
}

/// `preview`: start (or reuse) one background server and print its
/// URL. Returns the process exit code.
pub fn cmd_preview(folder: &Path, req: &PreviewStartRequest) -> i32 {
    cmd_start(
        "preview",
        folder,
        req.port.as_deref(),
        req.composition.clone(),
        req.foreground,
    )
}

/// `play` (Sprint 15, T005): serve the same server as `preview` and
/// print its embeddable player page URL (`<server>/player`, served with
/// the hyphenated embed attributes and the film at its own shape).
/// A refused project fails before any server starts, exactly like
/// `preview`. Returns the process exit code.
pub fn cmd_play(folder: &Path, req: &PreviewStartRequest) -> i32 {
    cmd_start(
        "play",
        folder,
        req.port.as_deref(),
        req.composition.clone(),
        req.foreground,
    )
}

/// `present` (Sprint 24, T002): serve presenter and audience views on
/// its own port (default 3004), reusing the preview server shape.
/// Missing manifest refuses; vanished targets hold.
pub fn cmd_present(folder: &Path, req: &PreviewStartRequest) -> i32 {
    // Default port 3004 for present (preview/play use 3003/picked).
    let port = req.port.as_deref().map(|s| s.to_string()).unwrap_or_else(|| "3004".to_string());
    let preferred = match parse_preview_port(Some(&port)) {
        Ok(p) => p,
        Err(m) => {
            eprintln!("framesmith present: {m}.");
            return 2;
        }
    };
    // Validate manifest exists before starting server.
    let verdict = validate_folder(folder);
    match verdict.status {
        Status::Invalid => {
            eprintln!("framesmith present refused: {} is invalid. No present was started.", folder.display());
            for d in &verdict.diagnostics {
                eprintln!("{}:{}:{} {} {} (fix: {})", d.file, d.line, d.column, d.code, d.message, d.fix);
            }
            return 1;
        }
        Status::Unsupported => {
            eprintln!("framesmith present refused: {} uses later-sprint elements. No present was started.", folder.display());
            return 1;
        }
        Status::Valid => {}
    }
    if verdict.slideshow.is_none() {
        let file = format!("{}/{}", folder.display(), validate_mod::COMPOSITION_FILE);
        eprintln!("present refused: {} declares no slideshow manifest (no `slideshow` node in {}); add one `slideshow {{ slide ... }}` block. No present was started.", folder.display(), file);
        return 1;
    }
    // Reuse the preview daemon registry / fall-forward but bind the
    // present server (which serves /presenter and /audience).
    let registry = preview::daemon::registry_path();
    let mut candidate = match preview::daemon::plan_start(&registry, folder, preferred) {
        Ok(preview::daemon::StartPlan::AlreadyRunning(entry)) => {
            println!("{}", preview::daemon::msg_existing(&entry));
            println!("presenter: {}presenter audience: {}audience", entry.url, entry.url);
            return 0;
        }
        Ok(preview::daemon::StartPlan::ToStart { port, .. }) => port,
        Err(refusal) => {
            eprintln!("{refusal}");
            return 1;
        }
    };
    let dir = match preview::daemon::validate_project(folder) {
        Ok(p) => p.dir,
        Err(refusal) => {
            eprintln!("{refusal}");
            return 1;
        }
    };
    if req.foreground {
        eprintln!(
            "framesmith present: serving presenter at {}presenter audience at {}audience; press Ctrl-C to stop.",
            preview::daemon::url_for_port(candidate),
            preview::daemon::url_for_port(candidate)
        );
        return preview::present::run_present_serve(
            &preview::present::PresentServeOptions {
                folder: folder.to_path_buf(),
                port: candidate,
            },
            &registry,
        );
    }
    for _ in 0..10 {
        let mut child = match spawn_present_child(candidate, &registry, folder) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("framesmith present: cannot start the present server: {e}.");
                return 1;
            }
        };
        match wait_for_server(
            &registry,
            &dir,
            candidate,
            &mut child,
            std::time::Duration::from_secs(20),
        ) {
            StartWait::Ready(entry) => {
                println!("{}", preview::daemon::msg_started(&entry));
                println!("presenter: {}presenter audience: {}audience", entry.url, entry.url);
                return 0;
            }
            StartWait::ChildExited(Some(preview::server::PORT_BUSY_EXIT)) => {
                if let Some(entry) = preview::daemon::find_live_for(&registry, &dir) {
                    println!("{}", preview::daemon::msg_existing(&entry));
                    println!("presenter: {}presenter audience: {}audience", entry.url, entry.url);
                    return 0;
                }
                match preview::daemon::pick_port(Some(candidate.saturating_add(1))) {
                    Ok(next) => candidate = next,
                    Err(m) => {
                        eprintln!("{m}; no present was started");
                        return 1;
                    }
                }
                continue;
            }
            StartWait::ChildExited(code) => {
                let tail = serve_log_tail(candidate);
                eprintln!(
                    "framesmith present: the present server exited before serving (exit {}); fix the cause and re-run. No present was started.\nserver log {}:\n{tail}",
                    code.map(|c| c.to_string()).as_deref().unwrap_or("?"),
                    serve_log_path(candidate).display()
                );
                return 1;
            }
            StartWait::Timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let tail = serve_log_tail(candidate);
                eprintln!(
                    "framesmith present: the present server did not answer within 20s; fix the cause and re-run. No present was started.\nserver log {}:\n{tail}",
                    serve_log_path(candidate).display()
                );
                return 1;
            }
        }
    }
    eprintln!("framesmith present: the preferred ports stayed busy; free one and re-run. No present was started.");
    1
}

fn spawn_present_child(
    port: u16,
    registry: &Path,
    folder: &Path,
) -> std::io::Result<std::process::Child> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let log = std::fs::File::create(serve_log_path(port))?;
    cmd.arg("present-serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--registry")
        .arg(registry)
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log);
    cmd.spawn()
}

pub fn cmd_present_serve(folder: &Path, port: u16, registry: &Path) -> i32 {
    preview::present::run_present_serve(
        &preview::present::PresentServeOptions {
            folder: folder.to_path_buf(),
            port,
        },
        registry,
    )
}

/// Player page URL behind one server entry (Sprint 15, T005).
pub fn player_url_for(entry: &preview::daemon::ServerEntry) -> String {
    format!("{}player", entry.url)
}

/// Shared `preview` / `play` start: validate, then run in this process
/// (`foreground`) or spawn one background server per project directory
/// with busy-port fall-forward. `cmd` names the command in reports;
/// `play` additionally prints the player page URL.
fn cmd_start(
    cmd: &str,
    folder: &Path,
    port: Option<&str>,
    composition: Option<String>,
    foreground: bool,
) -> i32 {
    let preferred = match parse_preview_port(port) {
        Ok(p) => p,
        Err(message) => {
            eprintln!("framesmith {cmd}: {message}.");
            return 2;
        }
    };
    let registry = preview::daemon::registry_path();
    // `play` reuses the one server per directory: the daemon entry is
    // shared, and only the printed URL differs (the player page).
    let announce = |entry: &preview::daemon::ServerEntry| {
        println!("{}", preview::daemon::msg_existing(entry));
        if cmd == "play" {
            println!("open {} in a browser to play.", player_url_for(entry));
        }
    };
    let mut candidate = match preview::daemon::plan_start(&registry, folder, preferred) {
        Ok(preview::daemon::StartPlan::AlreadyRunning(entry)) => {
            if cmd == "play" {
                announce(&entry);
            } else {
                println!("{}", preview::daemon::msg_existing(&entry));
            }
            return 0;
        }
        Ok(preview::daemon::StartPlan::ToStart { port, .. }) => port,
        Err(refusal) => {
            eprintln!("{refusal}");
            return 1;
        }
    };
    // The plan borrows nothing: re-derive the directory key once for
    // the readiness probes below.
    let dir = match preview::daemon::validate_project(folder) {
        Ok(p) => p.dir,
        Err(refusal) => {
            eprintln!("{refusal}");
            return 1;
        }
    };
    if foreground {
        if cmd == "play" {
            eprintln!(
                "framesmith play: serving the player at {} (append /player); press Ctrl-C to stop.",
                preview::daemon::url_for_port(candidate)
            );
        }
        return preview::server::run_serve(&preview::server::ServeOptions {
            folder: folder.to_path_buf(),
            port: candidate,
            registry,
            composition,
        });
    }
    for _ in 0..10 {
        let mut child =
            match spawn_serve_child(candidate, &registry, folder, composition.as_deref()) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("framesmith {cmd}: cannot start the preview server: {e}.");
                    return 1;
                }
            };
        match wait_for_server(
            &registry,
            &dir,
            candidate,
            &mut child,
            std::time::Duration::from_secs(20),
        ) {
            StartWait::Ready(entry) => {
                println!("{}", preview::daemon::msg_started(&entry));
                if cmd == "play" {
                    println!("open {} in a browser to play.", player_url_for(&entry));
                } else {
                    println!("open {} in a browser to review.", entry.url);
                }
                return 0;
            }
            StartWait::ChildExited(Some(preview::server::PORT_BUSY_EXIT)) => {
                // The port raced us: fall forward and retry, but first
                // re-check the registry — a sibling start may have won
                // this directory while we spawned.
                if let Some(entry) = preview::daemon::find_live_for(&registry, &dir) {
                    if cmd == "play" {
                        announce(&entry);
                    } else {
                        println!("{}", preview::daemon::msg_existing(&entry));
                    }
                    return 0;
                }
                match preview::daemon::pick_port(Some(candidate.saturating_add(1))) {
                    Ok(next) => candidate = next,
                    Err(message) => {
                        eprintln!("{message}; no {cmd} was started");
                        return 1;
                    }
                }
                continue;
            }
            StartWait::ChildExited(code) => {
                let tail = serve_log_tail(candidate);
                eprintln!(
                    "framesmith {cmd}: the preview server exited before serving (exit {}); fix the cause and re-run. No {cmd} was started.\nserver log {}:\n{tail}",
                    code.map(|c| c.to_string()).as_deref().unwrap_or("?"),
                    serve_log_path(candidate).display()
                );
                return 1;
            }
            StartWait::Timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let tail = serve_log_tail(candidate);
                eprintln!(
                    "framesmith {cmd}: the preview server did not answer within 20s; fix the cause and re-run. No {cmd} was started.\nserver log {}:\n{tail}",
                    serve_log_path(candidate).display()
                );
                return 1;
            }
        }
    }
    eprintln!("framesmith {cmd}: the preferred ports stayed busy; free one and re-run. No {cmd} was started.");
    1
}

/// `preview-status`: name the project's server and port, or none.
/// With no folder, list every live server instead.
pub fn cmd_preview_status(folder: Option<&Path>) -> i32 {
    let registry = preview::daemon::registry_path();
    match folder {
        Some(f) => {
            let dir = preview::daemon::canonical_dir(f)
                .unwrap_or_else(|_| f.to_string_lossy().into_owned());
            match preview::daemon::list_servers(&registry) {
                Ok(entries) => {
                    match entries.iter().find(|e| e.dir == dir) {
                        Some(entry) => println!("{}", preview::daemon::msg_status_some(entry)),
                        None => println!("{}", preview::daemon::msg_status_none(&dir)),
                    }
                    0
                }
                Err(message) => {
                    eprintln!("{message}");
                    1
                }
            }
        }
        None => cmd_preview_list(),
    }
}

/// `preview-stop`: end exactly that server, or report already-stopped
/// when the entry is missing or dead.
pub fn cmd_preview_stop(folder: &Path) -> i32 {
    let registry = preview::daemon::registry_path();
    let dir = preview::daemon::canonical_dir(folder)
        .unwrap_or_else(|_| folder.to_string_lossy().into_owned());
    match preview::daemon::stop_server(&registry, &dir) {
        Ok(preview::daemon::StopOutcome::Stopped(entry)) => {
            println!("{}", preview::daemon::msg_stopped(&entry));
            0
        }
        Ok(preview::daemon::StopOutcome::AlreadyStopped { dir }) => {
            println!("{}", preview::daemon::msg_already_stopped(&dir));
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// `preview-list`: show every live server.
pub fn cmd_preview_list() -> i32 {
    let registry = preview::daemon::registry_path();
    match preview::daemon::list_servers(&registry) {
        Ok(entries) => {
            println!("{}", preview::daemon::msg_list(&entries));
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// `preview-stop-all`: end all servers and report the count; an empty
/// registry reports zero and still exits 0.
pub fn cmd_preview_stop_all() -> i32 {
    let registry = preview::daemon::registry_path();
    match preview::daemon::stop_all_servers(&registry) {
        Ok(outcome) => {
            for entry in &outcome.stopped {
                println!("{}", preview::daemon::msg_stopped(entry));
            }
            for dir in &outcome.already {
                println!("{}", preview::daemon::msg_already_stopped(dir));
            }
            println!("{}", preview::daemon::msg_stop_all(&outcome));
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// `preview-serve` (internal): the background child `preview` spawns.
/// Validates and opens the session before binding, records the
/// registry entry, and serves until `/api/shutdown`.
pub fn cmd_preview_serve(
    folder: &Path,
    port: u16,
    registry: &Path,
    composition: Option<&str>,
) -> i32 {
    preview::server::run_serve(&preview::server::ServeOptions {
        folder: folder.to_path_buf(),
        port,
        registry: registry.to_path_buf(),
        composition: composition.map(|s| s.to_string()),
    })
}

/// `history` verbs (Sprint 15, T008 / CAP-007, CAP-008, CAP-009, J-04,
/// STATE-05, INT-003, REC-003, REC-004): one shared, guarded record
/// for humans and agents inside the project's hidden history directory.
///
/// * `list` names every turn oldest first and always works, even when
///   writes are unavailable (then it says so with the cause).
/// * `show` prints one turn; `--diff` names its changed files.
/// * `peek` prints the newest turn without consuming anything.
/// * `pin` (or `pin --unpin`) keeps a turn named for `restore`.
/// * `begin --who <name> [--exclusive] [--if-open]` opens a guarded
///   turn; `--if-open` joins the open turn instead of starting one.
/// * `end --who <name>` closes that name's open turn.
/// * `undo [<seq>] [--who <name>]` reverses one turn from its backup;
///   `--who` reverses that name's last turn still in effect; a bare
///   `undo` reverses the newest turn.
/// * `restore <seq>` brings a turn's `after` state back.
///
/// Refusals exit 1 naming the holder, the missing target or backup,
/// or the empty `--who`, and change nothing; usage errors exit 2.
/// Render, check, and validation never read the history directory, so
/// these verbs never change default renders. Returns the exit code.
#[derive(Debug, Default)]
pub struct HistoryRequest {
    /// Verb: `list`, `show`, `undo`, `restore`, `peek`, `pin`,
    /// `begin`, or `end`.
    pub verb: String,
    /// Project folder owning the hidden history directory.
    pub folder: PathBuf,
    /// Raw turn sequence for `show`, `pin`, `undo`, and `restore`.
    pub seq: Option<String>,
    /// Actor name for `begin`, `end`, and `undo --who`.
    pub who: Option<String>,
    /// `show --diff`: name the turn's changed files.
    pub diff: bool,
    /// `begin --exclusive`: block other writers until `end`.
    pub exclusive: bool,
    /// `begin --if-open`: join the open turn instead of starting one.
    pub if_open: bool,
    /// `pin --unpin`: clear the pin instead of setting it.
    pub unpin: bool,
    /// Machine-readable JSON report instead of human text.
    pub json: bool,
}

/// Parse raw `<seq>` text into a turn number (Sprint 15 history): a
/// non-numeric sequence is a usage error naming the expected shape.
fn parse_history_seq(raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|_| format!("expected a turn number <seq>, found {raw:?}"))
}

/// One refused `history` verb (Sprint 15, T008): the SDK refusal on
/// stderr as human text (or as the machine-readable report with
/// `--json`, keeping stdout pure JSON), the run log warned, exit 1.
/// Nothing was recorded, reversed, or guarded on refusal.
fn refuse_history(req: &HistoryRequest, refusal: &sdk_history::HistoryRefusal) -> i32 {
    let run = log::run_id();
    if req.json {
        println!(
            "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"{}\",\"folder\":\"{}\",\"status\":\"refused\",\"exit\":1,\"code\":\"{}\",\"message\":\"{}\"}}",
            json_escape(&req.verb),
            json_escape(&req.folder.display().to_string()),
            json_escape(refusal.code),
            json_escape(&refusal.message)
        );
    } else {
        eprintln!("framesmith history {} refused: {refusal}", req.verb);
    }
    note_log(
        &req.folder,
        "warn",
        "history",
        &run,
        &format!("history {} refused: {refusal}", req.verb),
        req.json,
    );
    1
}

/// One misused `history` verb (Sprint 15, T008): the usage gap on
/// stderr with the verb usage, exit 2. Nothing ran, so nothing changed.
fn misuse_history(req: &HistoryRequest, msg: &str) -> i32 {
    eprintln!("framesmith history {}: {msg}.", req.verb);
    eprintln!("{HISTORY_USAGE}");
    2
}

/// Usage line for every `history` verb (shared by the dispatch and
/// the misuse report so both name the same shape).
const HISTORY_USAGE: &str = "usage: framesmith history (list|show|undo|restore|peek|pin|begin|end) [--json] [--diff] [--who <name>] [--exclusive] [--if-open] [--unpin] <folder> [<seq>]";

/// `history list`: name every recorded turn oldest first, then say
/// whether the current run may write history. Listing never refuses
/// over writability: an unwritable history directory still lists,
/// saying so with the cause. Returns the process exit code.
fn cmd_history_list(req: &HistoryRequest) -> i32 {
    let run = log::run_id();
    match sdk_history::list_turns(&req.folder) {
        Ok(listing) => {
            if req.json {
                use std::fmt::Write;
                let mut out = format!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"list\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"turns\":[",
                    json_escape(&req.folder.display().to_string())
                );
                for (i, t) in listing.turns.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(
                        out,
                        "{{\"seq\":{},\"actor\":\"{}\",\"summary\":\"{}\",\"pinned\":{},\"undone\":{},\"created_unix\":{}}}",
                        t.seq,
                        json_escape(&t.actor),
                        json_escape(&t.summary),
                        t.pinned,
                        t.undone,
                        t.created_unix
                    );
                }
                let _ = write!(
                    out,
                    "],\"writes_available\":{},\"write_cause\":{}",
                    listing.writes_available,
                    match &listing.write_cause {
                        Some(c) => format!("\"{}\"", json_escape(c)),
                        None => "null".to_string(),
                    }
                );
                out.push('}');
                println!("{out}");
            } else {
                println!(
                    "history: {} turn(s) in {}.",
                    listing.turns.len(),
                    req.folder.display()
                );
                for t in &listing.turns {
                    let mut flags = String::new();
                    if t.pinned {
                        flags.push_str(" [pinned]");
                    }
                    if t.undone {
                        flags.push_str(" [undone]");
                    }
                    println!("#{} {} {:?}{flags}", t.seq, t.actor, t.summary);
                }
                if listing.writes_available {
                    println!("writes available.");
                } else {
                    println!(
                        "warning: writes are unavailable: {}.",
                        listing.write_cause.as_deref().unwrap_or("unknown cause")
                    );
                }
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!("history list: {} turn(s)", listing.turns.len()),
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// `history show`: print one recorded turn; `--diff` names its
/// changed files (the `show --diff` names). A missing turn refuses
/// naming which target is missing. Returns the exit code.
fn cmd_history_show(req: &HistoryRequest, seq: u64) -> i32 {
    let run = log::run_id();
    match sdk_history::show_turn(&req.folder, seq) {
        Ok(detail) => {
            let r = &detail.record;
            if req.json {
                use std::fmt::Write;
                let mut out = format!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"show\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"seq\":{},\"actor\":\"{}\",\"summary\":\"{}\",\"pinned\":{},\"undone\":{},\"created_unix\":{},\"targets\":[",
                    json_escape(&req.folder.display().to_string()),
                    r.seq,
                    json_escape(&r.actor),
                    json_escape(&r.summary),
                    r.pinned,
                    r.undone,
                    r.created_unix
                );
                for (i, t) in r.targets.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{}\"", json_escape(t));
                }
                out.push_str("],\"artifacts_cleaned\":[");
                for (i, a) in r.artifacts_cleaned.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{}\"", json_escape(a));
                }
                out.push_str("],\"changed_files\":[");
                for (i, f) in detail.diff.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{}\"", json_escape(f));
                }
                out.push_str("]}");
                println!("{out}");
            } else {
                println!("turn #{} in {}.", r.seq, req.folder.display());
                println!("actor: {}", r.actor);
                println!("summary: {:?}", r.summary);
                println!("targets:");
                for t in &r.targets {
                    println!("  - {t}");
                }
                println!("artifacts cleaned:");
                for a in &r.artifacts_cleaned {
                    println!("  - {a}");
                }
                println!("pinned: {}.", if r.pinned { "yes" } else { "no" });
                println!("undone: {}.", if r.undone { "yes" } else { "no" });
                if req.diff {
                    println!("changed files:");
                    for f in &detail.diff {
                        println!("  - {f}");
                    }
                } else {
                    println!(
                        "changed files: {} file(s) (pass --diff to name them).",
                        detail.diff.len()
                    );
                }
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!("history show: turn {seq}"),
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// `history peek`: print the newest recorded turn without consuming
/// anything; an empty history reports that and still exits 0.
/// Returns the exit code.
fn cmd_history_peek(req: &HistoryRequest) -> i32 {
    let run = log::run_id();
    match sdk_history::peek_turn(&req.folder) {
        Ok(Some(t)) => {
            if req.json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"peek\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"seq\":{},\"actor\":\"{}\",\"summary\":\"{}\",\"pinned\":{},\"undone\":{},\"created_unix\":{}}}",
                    json_escape(&req.folder.display().to_string()),
                    t.seq,
                    json_escape(&t.actor),
                    json_escape(&t.summary),
                    t.pinned,
                    t.undone,
                    t.created_unix
                );
            } else {
                println!(
                    "newest turn #{} by {} {:?} in {}.",
                    t.seq,
                    t.actor,
                    t.summary,
                    req.folder.display()
                );
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!("history peek: turn {}", t.seq),
                req.json,
            );
            0
        }
        Ok(None) => {
            if req.json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"peek\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"seq\":null,\"message\":\"no turns recorded\"}}",
                    json_escape(&req.folder.display().to_string())
                );
            } else {
                println!("no turns recorded in {}.", req.folder.display());
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                "history peek: no turns recorded",
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// `history pin`: keep (`pin`) or release (`pin --unpin`) a named
/// turn so `restore` can bring it back. A missing turn refuses naming
/// which target is missing. Returns the exit code.
fn cmd_history_pin(req: &HistoryRequest, seq: u64) -> i32 {
    let run = log::run_id();
    match sdk_history::pin_turn(&req.folder, seq, !req.unpin) {
        Ok(record) => {
            if req.json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"pin\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"seq\":{},\"pinned\":{}}}",
                    json_escape(&req.folder.display().to_string()),
                    record.seq,
                    record.pinned
                );
            } else if record.pinned {
                println!("pinned turn #{}.", record.seq);
            } else {
                println!("unpinned turn #{}.", record.seq);
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!("history pin: turn {} pinned={}", record.seq, record.pinned),
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// `history begin`: open a guarded turn for `--who <name>`;
/// `--exclusive` blocks other writers until `end`, `--if-open`
/// joins the already-open turn instead of starting a second. A
/// conflicting turn refuses naming the holder and the turn's age,
/// preserving the refused work unapplied. Returns the exit code.
fn cmd_history_begin(req: &HistoryRequest, who: &str) -> i32 {
    let run = log::run_id();
    let opts = sdk_history::BeginOptions {
        exclusive: req.exclusive,
        if_open: req.if_open,
    };
    match sdk_history::begin_turn(&req.folder, who, &opts) {
        Ok(handle) => {
            if req.json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"begin\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"actor\":\"{}\",\"exclusive\":{},\"attached\":{},\"started_unix\":{}}}",
                    json_escape(&req.folder.display().to_string()),
                    json_escape(&handle.actor),
                    handle.exclusive,
                    handle.attached,
                    handle.started_unix
                );
            } else if handle.attached {
                println!(
                    "attached {who:?} to the open turn held by {:?} (--if-open).",
                    handle.actor
                );
            } else if handle.exclusive {
                println!(
                    "began exclusive turn for {who:?} in {}.",
                    req.folder.display()
                );
            } else {
                println!("began shared turn for {who:?} in {}.", req.folder.display());
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!(
                    "history begin: actor={} exclusive={} attached={}",
                    handle.actor, handle.exclusive, handle.attached
                ),
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// `history end`: close `--who <name>`'s open turn. Ending a name
/// with no open turn refuses naming it and changes nothing. Returns
/// the exit code.
fn cmd_history_end(req: &HistoryRequest, who: &str) -> i32 {
    let run = log::run_id();
    match sdk_history::end_turn(&req.folder, who) {
        Ok(handle) => {
            if req.json {
                println!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"end\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"actor\":\"{}\"}}",
                    json_escape(&req.folder.display().to_string()),
                    json_escape(&handle.actor)
                );
            } else {
                println!("ended the open turn held by {:?}.", handle.actor);
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!("history end: actor={}", handle.actor),
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// One reversed turn as human text (Sprint 15, T008): the verb, the
/// turn, and the restored state the tree now matches, so the maker
/// reads back what changed on disk.
fn format_history_reversed_human(
    verb: &str,
    seq: u64,
    actor: &str,
    summary: &str,
    restored_state: &str,
    restored_files: &[String],
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{verb} turn #{seq} by {actor} {summary:?}; {restored_state}."
    );
    let _ = writeln!(out, "restored {} file(s):", restored_files.len());
    for f in restored_files {
        let _ = writeln!(out, "  - {f}");
    }
    out
}

/// `history undo`: reverse one recorded turn from its backup and
/// report the restored state. `--who <name>` reverses that name's
/// last turn still in effect (reporting when none is); a bare `undo`
/// reverses the newest turn. A missing target or backup refuses naming
/// which one is missing, reversing nothing. Returns the exit code.
fn cmd_history_undo(req: &HistoryRequest, seq: Option<u64>, who: Option<&str>) -> i32 {
    let run = log::run_id();
    let outcome = match (seq, who) {
        (_, Some(name)) => sdk_history::undo_who(&req.folder, name),
        (Some(n), None) => sdk_history::undo_turn(&req.folder, n),
        (None, None) => match sdk_history::list_turns(&req.folder) {
            Err(refusal) => Err(refusal),
            Ok(listing) => match listing.turns.iter().max_by_key(|t| t.seq) {
                Some(newest) => sdk_history::undo_turn(&req.folder, newest.seq),
                None => {
                    return refuse_history(
                        req,
                        &sdk_history::HistoryRefusal {
                            kind: sdk_history::HistoryRefusalKind::NoTurn,
                            code: sdk_history::E_SDK_NO_TURN,
                            message: format!(
                                "history refused: no recorded turn answers the request: the history holds no turns. Record a turn first; nothing was reversed."
                            ),
                        },
                    );
                }
            },
        },
    };
    match outcome {
        Ok(receipt) => {
            if req.json {
                use std::fmt::Write;
                let mut out = format!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"undo\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"seq\":{},\"actor\":\"{}\",\"summary\":\"{}\",\"restored_state\":\"{}\",\"restored_files\":[",
                    json_escape(&req.folder.display().to_string()),
                    receipt.seq,
                    json_escape(&receipt.actor),
                    json_escape(&receipt.summary),
                    json_escape(&receipt.restored_state)
                );
                for (i, f) in receipt.restored_files.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{}\"", json_escape(f));
                }
                out.push_str("]}");
                println!("{out}");
            } else {
                print!(
                    "{}",
                    format_history_reversed_human(
                        "undone",
                        receipt.seq,
                        &receipt.actor,
                        &receipt.summary,
                        &receipt.restored_state,
                        &receipt.restored_files
                    )
                );
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!("history undo: turn {}", receipt.seq),
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// `history restore`: bring a turn's `after` state back and report
/// the restored state. A missing target or backup refuses naming
/// which one is missing, restoring nothing. Returns the exit code.
fn cmd_history_restore(req: &HistoryRequest, seq: u64) -> i32 {
    let run = log::run_id();
    match sdk_history::restore_turn(&req.folder, seq) {
        Ok(receipt) => {
            if req.json {
                use std::fmt::Write;
                let mut out = format!(
                    "{{\"tool\":\"framesmith\",\"command\":\"history\",\"verb\":\"restore\",\"folder\":\"{}\",\"status\":\"ok\",\"exit\":0,\"seq\":{},\"actor\":\"{}\",\"summary\":\"{}\",\"restored_state\":\"{}\",\"restored_files\":[",
                    json_escape(&req.folder.display().to_string()),
                    receipt.seq,
                    json_escape(&receipt.actor),
                    json_escape(&receipt.summary),
                    json_escape(&receipt.restored_state)
                );
                for (i, f) in receipt.restored_files.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{}\"", json_escape(f));
                }
                out.push_str("]}");
                println!("{out}");
            } else {
                print!(
                    "{}",
                    format_history_reversed_human(
                        "restored",
                        receipt.seq,
                        &receipt.actor,
                        &receipt.summary,
                        &receipt.restored_state,
                        &receipt.restored_files
                    )
                );
            }
            note_log(
                &req.folder,
                "info",
                "history",
                &run,
                &format!("history restore: turn {}", receipt.seq),
                req.json,
            );
            0
        }
        Err(refusal) => refuse_history(req, &refusal),
    }
}

/// `history`: dispatch one guarded-history verb (Sprint 15, T008).
/// Returns the process exit code (0 ok, 1 refused, 2 misused).
pub fn cmd_history(req: &HistoryRequest) -> i32 {
    match req.verb.as_str() {
        "list" => cmd_history_list(req),
        "peek" => cmd_history_peek(req),
        "show" => match &req.seq {
            Some(raw) => match parse_history_seq(raw) {
                Ok(seq) => cmd_history_show(req, seq),
                Err(msg) => misuse_history(req, &msg),
            },
            None => misuse_history(req, "show needs a turn number <seq>"),
        },
        "pin" => match &req.seq {
            Some(raw) => match parse_history_seq(raw) {
                Ok(seq) => cmd_history_pin(req, seq),
                Err(msg) => misuse_history(req, &msg),
            },
            None => misuse_history(req, "pin needs a turn number <seq>"),
        },
        "begin" => match &req.who {
            Some(who) => cmd_history_begin(req, who),
            None => misuse_history(req, "begin needs --who <name> (an actor name)"),
        },
        "end" => match &req.who {
            Some(who) => cmd_history_end(req, who),
            None => misuse_history(req, "end needs --who <name> (an actor name)"),
        },
        "undo" => {
            let seq = match &req.seq {
                Some(raw) => match parse_history_seq(raw) {
                    Ok(n) => Some(n),
                    Err(msg) => return misuse_history(req, &msg),
                },
                None => None,
            };
            cmd_history_undo(req, seq, req.who.as_deref())
        }
        "restore" => match &req.seq {
            Some(raw) => match parse_history_seq(raw) {
                Ok(seq) => cmd_history_restore(req, seq),
                Err(msg) => misuse_history(req, &msg),
            },
            None => misuse_history(req, "restore needs a turn number <seq>"),
        },
        other => misuse_history(req, &format!("unknown verb {other:?}; expected list, show, undo, restore, peek, pin, begin, or end")),
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
        #[cfg(windows)]
        {
            // Readonly bits on Windows directories do not stop
            // creation; the unwritable case is covered by the
            // file-as-parent variant below on every OS.
            let _ = code;
            let file_parent = root.join("file");
            std::fs::write(&file_parent, "x").unwrap();
            let code = cmd_init("proj2", &file_parent);
            assert_eq!(code, 1);
            assert!(!file_parent.join("proj2").exists(), "no partial folder may remain");
            return;
        }
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
    fn init_starter_project_validates_with_guide() {
        let root = tmp_dir("starter-unit");
        assert_eq!(cmd_init_with_example("card", &root, Some("title-card")), 0);
        let target = root.join("card");
        assert!(target.join(validate_mod::COMPOSITION_FILE).is_file());
        assert!(target.join(crate::starters::AGENT_GUIDE_FILE).is_file());
        assert_eq!(
            validate_folder(&target).status,
            Status::Valid,
            "{:?}",
            validate_folder(&target).diagnostics
        );
    }

    #[test]
    fn init_unknown_example_creates_nothing() {
        let root = tmp_dir("starter-unknown");
        assert_eq!(cmd_init_with_example("ghost", &root, Some("nope")), 1);
        assert!(!root.join("ghost").exists());
        // The empty-name gate runs first: bad names never reach lookup.
        assert_eq!(cmd_init_with_example("", &root, Some("nope")), 1);
    }

    #[test]
    fn init_starter_refuses_nonempty_target_untouched() {
        let root = tmp_dir("starter-occupied");
        let target = root.join("taken");
        std::fs::create_dir_all(&target).unwrap();
        write(&target, "keep.txt", "precious");
        assert_eq!(cmd_init_with_example("taken", &root, Some("color-grid")), 1);
        assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"precious");
        assert!(!target.join(validate_mod::COMPOSITION_FILE).exists());
        assert!(!target.join(crate::starters::AGENT_GUIDE_FILE).exists());
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
        // Beyond-blur motion filter properties still refuse (Sprint 11
        // effects live on clip `effect` elements; only `blur` animates).
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
                v.diagnostics
                    .iter()
                    .any(|d| d.code == "E_BAD_FILL" && d.message.contains("child elements")),
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

    // Sprint 9 T006: batch manifest parsing, name templates, and the
    // batch driver's pre-render refusals (no FFmpeg needed here; real
    // row renders are covered by the CLI e2e batch tests).
    const BATCH_VALID: &str = "composition name=\"t\" width=64 height=64 duration=1 fps=30 background=\"#000\" version=1 {\nvariable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=1\n}\n";

    fn batch_req(batch: &str) -> RenderRequest {
        RenderRequest {
            batch: Some(batch.to_string()),
            name_template: Some("clip-{title}-{index}".to_string()),
            ..RenderRequest::default()
        }
    }

    #[test]
    fn batch_parses_rows_with_variables_name_and_theme() {
        let root = tmp_dir("batch-parse");
        let path = root.join("rows.json");
        write(
            &root,
            "rows.json",
            r#"[{"variables": {"title": "Sale", "n": 9.5, "b": true}, "name": "a", "theme": "brand"}, {"variables": {}}]"#,
        );
        let rows = parse_batch_rows(&path).expect("rows parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].variables.get("title"),
            Some(&validate_mod::OverrideValue::Str("Sale".to_string()))
        );
        assert_eq!(
            rows[0].variables.get("n"),
            Some(&validate_mod::OverrideValue::Num(9.5))
        );
        assert_eq!(
            rows[0].variables.get("b"),
            Some(&validate_mod::OverrideValue::Bool(true))
        );
        assert_eq!(rows[0].name.as_deref(), Some("a"));
        assert_eq!(rows[0].theme.as_deref(), Some("brand"));
        assert!(rows[1].variables.is_empty());
        assert!(rows[1].name.is_none());
    }

    #[test]
    fn batch_refuses_unreadable_rows_file_naming_the_file() {
        let root = tmp_dir("batch-missing");
        let missing = root.join("nope.json");
        let err = parse_batch_rows(&missing).unwrap_err();
        assert!(err.contains("cannot read batch rows file"), "{err}");
        assert!(err.contains("nope.json"), "{err}");
    }

    #[test]
    fn batch_refuses_non_array_and_bad_row_shapes() {
        let root = tmp_dir("batch-shapes");
        for (tag, body, want) in [
            ("obj", r#"{"variables": {}}"#, "holds an object"),
            ("row-str", r#"["nope"]"#, "row 0 is a bad row shape"),
            (
                "vars-str",
                r#"[{"variables": "x"}]"#,
                "key \"variables\" holds a string",
            ),
            (
                "arr-value",
                r#"[{"variables": {"title": [1]}}]"#,
                "variable \"title\" holds an array",
            ),
            (
                "null-value",
                r#"[{"variables": {"title": null}}]"#,
                "variable \"title\" holds null",
            ),
            (
                "unknown-key",
                r#"[{"variables": {}, "zzz": 1}]"#,
                "unknown key \"zzz\"",
            ),
            (
                "name-num",
                r#"[{"name": 3}]"#,
                "key \"name\" holds a number",
            ),
        ] {
            let path = root.join(format!("{tag}.json"));
            std::fs::write(&path, body).unwrap();
            let err = parse_batch_rows(&path).unwrap_err();
            assert!(err.contains(want), "{tag}: {err}");
            assert!(err.contains(&format!("{tag}.json")), "{tag}: {err}");
        }
        let bad = root.join("bad.json");
        std::fs::write(&bad, "[1,").unwrap();
        let err = parse_batch_rows(&bad).unwrap_err();
        assert!(err.contains("is not valid JSON"), "{err}");
    }

    #[test]
    fn batch_template_fills_index_and_variables() {
        let mut vars = validate_mod::VariableOverrides::new();
        vars.insert(
            "title".to_string(),
            validate_mod::OverrideValue::Str("Sale".to_string()),
        );
        vars.insert("n".to_string(), validate_mod::OverrideValue::Num(9.0));
        let declared: std::collections::BTreeSet<String> =
            ["title".to_string()].into_iter().collect();
        let keys: std::collections::BTreeSet<String> =
            ["title".to_string(), "n".to_string()].into_iter().collect();
        assert_eq!(
            resolve_batch_name("clip-{title}-{index}", 2, &vars, &declared, &keys).unwrap(),
            "clip-Sale-2"
        );
        assert_eq!(
            resolve_batch_name("n{n}", 0, &vars, &declared, &keys).unwrap(),
            "n9"
        );
        // A declared id missing from this row substitutes empty.
        assert_eq!(
            resolve_batch_name(
                "{title}x",
                0,
                &validate_mod::VariableOverrides::new(),
                &declared,
                &keys
            )
            .unwrap(),
            "x"
        );
    }

    #[test]
    fn batch_template_refuses_unknown_placeholder_naming_it() {
        let vars = validate_mod::VariableOverrides::new();
        let empty: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let err = resolve_batch_name("clip-{bogus}-{index}", 0, &vars, &empty, &empty).unwrap_err();
        assert!(err.contains("unknown placeholder {bogus}"), "{err}");
        assert!(err.contains("--name-template"), "{err}");
        let err = resolve_batch_name("clip-{title", 0, &vars, &empty, &empty).unwrap_err();
        assert!(err.contains("unclosed"), "{err}");
    }

    #[test]
    fn batch_output_path_appends_mp4_and_joins_folder() {
        use render::outputs::OutputFormat;
        let folder = PathBuf::from("proj");
        assert_eq!(
            batch_output_path(&folder, "clip-a", OutputFormat::Mp4),
            PathBuf::from("proj/clip-a.mp4")
        );
        assert_eq!(
            batch_output_path(&folder, "clip-a.mp4", OutputFormat::Mp4),
            PathBuf::from("proj/clip-a.mp4")
        );
    }

    #[test]
    fn batch_output_path_follows_the_destination_format() {
        use render::outputs::OutputFormat;
        let folder = PathBuf::from("proj");
        assert_eq!(
            batch_output_path(&folder, "clip-a", OutputFormat::Webm),
            PathBuf::from("proj/clip-a.webm")
        );
        assert_eq!(
            batch_output_path(&folder, "clip-a", OutputFormat::Prores),
            PathBuf::from("proj/clip-a.mov")
        );
        assert_eq!(
            batch_output_path(&folder, "clip-a", OutputFormat::Hevc),
            PathBuf::from("proj/clip-a.mp4")
        );
        assert_eq!(
            batch_output_path(&folder, "clip-a", OutputFormat::Gif),
            PathBuf::from("proj/clip-a.gif")
        );
        // Sequence destinations resolve the stem to a directory.
        assert_eq!(
            batch_output_path(&folder, "stills", OutputFormat::PngSeq),
            PathBuf::from("proj/stills")
        );
        assert_eq!(
            batch_output_path(&folder, "stream", OutputFormat::Hls),
            PathBuf::from("proj/stream")
        );
    }

    #[test]
    fn destination_flags_resolve_together_before_frame_one() {
        use render::outputs::{OutputFormat, ProresProfile};
        use render::scale::Resolution;
        let ok = RenderRequest {
            format: Some("prores".to_string()),
            prores_profile: Some("hq".to_string()),
            ..RenderRequest::default()
        };
        assert_eq!(
            resolve_destination(&ok).unwrap(),
            (
                OutputFormat::Prores,
                ProresProfile::Hq,
                Resolution::Source,
                1,
                render::outputs::EncoderChoice::Software,
                None,
                None,
                render::outputs::HdrMode::Auto,
            )
        );
        let plain = RenderRequest::default();
        assert_eq!(
            resolve_destination(&plain).unwrap(),
            (
                OutputFormat::Mp4,
                ProresProfile::Standard,
                Resolution::Source,
                1,
                render::outputs::EncoderChoice::Software,
                None,
                None,
                render::outputs::HdrMode::Auto,
            )
        );
        let scaled = RenderRequest {
            resolution: Some("720p".to_string()),
            supersample: Some("3".to_string()),
            ..RenderRequest::default()
        };
        assert_eq!(
            resolve_destination(&scaled).unwrap(),
            (
                OutputFormat::Mp4,
                ProresProfile::Standard,
                Resolution::P720,
                3,
                render::outputs::EncoderChoice::Software,
                None,
                None,
                render::outputs::HdrMode::Auto,
            )
        );
        let bad_format = RenderRequest {
            format: Some("avi".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&bad_format).unwrap_err();
        assert!(err.contains("--format"), "{err}");
        assert!(
            err.contains("mp4, hevc, webm, mov, prores, gif, pngseq, or hls"),
            "{err}"
        );
        let bad_combo = RenderRequest {
            format: Some("webm".to_string()),
            prores_profile: Some("hq".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&bad_combo).unwrap_err();
        assert!(err.contains("--prores-profile"), "{err}");
        assert!(err.contains("--format prores"), "{err}");
        let bad_profile = RenderRequest {
            format: Some("prores".to_string()),
            prores_profile: Some("ultra".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&bad_profile).unwrap_err();
        assert!(err.contains("--prores-profile"), "{err}");
        assert!(err.contains("proxy, lt, standard, hq, or 4444"), "{err}");
        // Every problem reports at once.
        let both_bad = RenderRequest {
            format: Some("avi".to_string()),
            prores_profile: Some("ultra".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&both_bad).unwrap_err();
        assert!(err.contains("--format"), "{err}");
        assert!(err.contains("--prores-profile"), "{err}");
    }

    #[test]
    fn scale_flags_refuse_unknown_preset_and_range_with_fixes() {
        // An unknown preset names the flag and the accepted values.
        let bad_preset = RenderRequest {
            resolution: Some("8k".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&bad_preset).unwrap_err();
        assert!(err.contains("--resolution"), "{err}");
        assert!(
            err.contains("source, 1080p, 720p, 480p, square-1080, or 4k"),
            "{err}"
        );
        assert!(err.contains("no output was written"), "{err}");
        // A supersample outside 1-4 names the flag and the range.
        for bad in ["0", "5", "2.5", "abc", ""] {
            let req = RenderRequest {
                supersample: Some(bad.to_string()),
                ..RenderRequest::default()
            };
            let err = resolve_destination(&req).unwrap_err();
            assert!(err.contains("--supersample"), "{err}");
            assert!(err.contains("from 1 to 4"), "{err}");
            assert!(err.contains("no output was written"), "{err}");
        }
        // Every scale problem reports at once, alongside format ones.
        let all_bad = RenderRequest {
            format: Some("avi".to_string()),
            resolution: Some("8k".to_string()),
            supersample: Some("9".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&all_bad).unwrap_err();
        assert!(err.contains("--format"), "{err}");
        assert!(err.contains("--resolution"), "{err}");
        assert!(err.contains("--supersample"), "{err}");
    }

    #[test]
    fn encoder_flags_resolve_with_shapes_and_fixes() {
        use render::outputs::EncoderChoice;
        // Named encoder, interval, and cap resolve together.
        let paced = RenderRequest {
            encoder: Some("auto".to_string()),
            keyint: Some("60".to_string()),
            maxrate: Some("8M".to_string()),
            ..RenderRequest::default()
        };
        let (_, _, _, _, encoder, keyint, maxrate, hdr) = resolve_destination(&paced).unwrap();
        assert_eq!(hdr, render::outputs::HdrMode::Auto);
        assert_eq!(encoder, EncoderChoice::Auto);
        assert_eq!(keyint, Some(60));
        assert_eq!(maxrate.as_deref(), Some("8M"));
        for raw in ["software", "videotoolbox"] {
            let req = RenderRequest {
                encoder: Some(raw.to_string()),
                ..RenderRequest::default()
            };
            let (_, _, _, _, encoder, _, _, _) = resolve_destination(&req).unwrap();
            assert_eq!(encoder.name(), raw);
        }
        // An unknown encoder names the flag and the accepted values.
        let bad_encoder = RenderRequest {
            encoder: Some("gpu".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&bad_encoder).unwrap_err();
        assert!(err.contains("--encoder"), "{err}");
        assert!(err.contains("auto, software, cpu, videotoolbox, nvenc, vaapi, qsv, or amf"), "{err}");
        assert!(err.contains("no output was written"), "{err}");
        // A non-positive keyint names the flag and the accepted shape.
        for bad in ["0", "-4", "1.5", "many", ""] {
            let req = RenderRequest {
                keyint: Some(bad.to_string()),
                ..RenderRequest::default()
            };
            let err = resolve_destination(&req).unwrap_err();
            assert!(err.contains("--keyint"), "{bad}: {err}");
            assert!(err.contains("positive whole number"), "{bad}: {err}");
            assert!(err.contains("no output was written"), "{bad}: {err}");
        }
        // A malformed maxrate names the flag and the accepted shape.
        for bad in ["fast", "-5M", "0", ""] {
            let req = RenderRequest {
                maxrate: Some(bad.to_string()),
                ..RenderRequest::default()
            };
            let err = resolve_destination(&req).unwrap_err();
            assert!(err.contains("--maxrate"), "{bad}: {err}");
            assert!(err.contains("8M"), "{bad}: {err}");
            assert!(err.contains("no output was written"), "{bad}: {err}");
        }
        // Every encoder problem reports at once, alongside format ones.
        let all_bad = RenderRequest {
            format: Some("avi".to_string()),
            encoder: Some("cuda".to_string()),
            keyint: Some("0".to_string()),
            maxrate: Some("fast".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&all_bad).unwrap_err();
        assert!(err.contains("--format"), "{err}");
        assert!(err.contains("--encoder"), "{err}");
        assert!(err.contains("--keyint"), "{err}");
        assert!(err.contains("--maxrate"), "{err}");
    }

    #[test]
    fn hdr_flags_resolve_with_shapes_and_fixes() {
        use render::outputs::HdrMode;
        // Each named mode resolves.
        for raw in ["off", "pq", "hlg", "auto"] {
            let req = RenderRequest {
                hdr: Some(raw.to_string()),
                ..RenderRequest::default()
            };
            let (_, _, _, _, _, _, _, hdr) = resolve_destination(&req).unwrap();
            assert_eq!(hdr.name(), raw);
        }
        // Absent means `auto`, so default renders decide from probes.
        let (_, _, _, _, _, _, _, hdr) = resolve_destination(&RenderRequest::default()).unwrap();
        assert_eq!(hdr, HdrMode::Auto);
        // An unknown mode names the flag and the accepted values.
        let bad_hdr = RenderRequest {
            hdr: Some("dolby".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&bad_hdr).unwrap_err();
        assert!(err.contains("--hdr"), "{err}");
        assert!(err.contains("off, pq, hlg, or auto"), "{err}");
        assert!(err.contains("no output was written"), "{err}");
        // Every destination problem reports at once, alongside HDR ones.
        let all_bad = RenderRequest {
            format: Some("avi".to_string()),
            hdr: Some("dolby".to_string()),
            ..RenderRequest::default()
        };
        let err = resolve_destination(&all_bad).unwrap_err();
        assert!(err.contains("--format"), "{err}");
        assert!(err.contains("--hdr"), "{err}");
    }

    #[test]
    fn batch_overrides_json_round_trips_through_parse() {
        let mut vars = validate_mod::VariableOverrides::new();
        vars.insert(
            "title".to_string(),
            validate_mod::OverrideValue::Str("S\"ale".to_string()),
        );
        vars.insert("n".to_string(), validate_mod::OverrideValue::Num(2.5));
        vars.insert("b".to_string(), validate_mod::OverrideValue::Bool(false));
        let raw = batch_overrides_json(&vars);
        let back = validate_mod::parse_variable_overrides(&raw).expect("round trip");
        assert_eq!(back, vars);
    }

    fn batch_folder(tag: &str, rows_body: &str) -> (PathBuf, RenderRequest) {
        let root = tmp_dir(tag);
        let folder = root.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        write(&folder, validate_mod::COMPOSITION_FILE, BATCH_VALID);
        write(&root, "rows.json", rows_body);
        let req = batch_req(&root.join("rows.json").display().to_string());
        (folder, req)
    }

    #[test]
    fn batch_driver_refuses_duplicate_names_naming_both_rows() {
        let (folder, req) = batch_folder(
            "batch-dup",
            r#"[{"variables": {"title": "A"}}, {"variables": {"title": "B"}}]"#,
        );
        let mut dup_req = req;
        dup_req.name_template = Some("same".to_string());
        assert_eq!(cmd_render_batch(&folder, None, &dup_req), 1);
        // No render started: no MP4 beside the project.
        assert!(!folder.join("same.mp4").exists());
        assert!(!folder.join("out.mp4").exists());
    }

    #[test]
    fn batch_driver_refuses_unknown_placeholder_before_any_render() {
        let (folder, mut req) = batch_folder(
            "batch-ph",
            r#"[{"variables": {"title": "A"}}, {"variables": {"title": "B"}}]"#,
        );
        req.name_template = Some("clip-{bogus}-{index}".to_string());
        assert_eq!(cmd_render_batch(&folder, None, &req), 1);
        assert!(!folder.join("clip-A-0.mp4").exists());
    }

    #[test]
    fn batch_driver_refuses_rows_above_the_limit_naming_it() {
        let root = tmp_dir("batch-limit");
        let folder = root.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        write(&folder, validate_mod::COMPOSITION_FILE, BATCH_VALID);
        let mut body = String::from("[");
        for i in 0..=MAX_BATCH_ROWS {
            if i > 0 {
                body.push(',');
            }
            body.push_str(r#"{"variables": {}}"#);
        }
        body.push(']');
        write(&root, "rows.json", &body);
        let req = batch_req(&root.join("rows.json").display().to_string());
        assert_eq!(cmd_render_batch(&folder, None, &req), 1);
        assert!(!folder.join("out.mp4").exists());
    }

    #[test]
    fn batch_driver_reports_zero_rows_and_exits_zero() {
        let (folder, req) = batch_folder("batch-empty", r#"[]"#);
        assert_eq!(cmd_render_batch(&folder, None, &req), 0);
    }

    #[test]
    fn batch_driver_refuses_bad_jobs_values() {
        let (folder, mut req) = batch_folder("batch-jobs", r#"[]"#);
        for bad in ["0", "99", "many", "-2"] {
            req.jobs = Some(bad.to_string());
            assert_eq!(cmd_render_batch(&folder, None, &req), 1, "{bad}");
        }
    }

    #[test]
    fn batch_json_summary_marks_failed_and_skipped() {
        let outcomes = vec![
            BatchOutcome {
                index: 0,
                ok: true,
                output: "proj/clip-0.mp4".to_string(),
                message: String::new(),
            },
            BatchOutcome {
                index: 1,
                ok: false,
                output: "proj/clip-1.mp4".to_string(),
                message: "row 1 invalid: boom".to_string(),
            },
            BatchOutcome {
                index: 2,
                ok: false,
                output: "proj/clip-2.mp4".to_string(),
                message: "skipped: fail-fast stopped the batch at row 1".to_string(),
            },
        ];
        let text = format_batch_json(
            Path::new("proj"),
            "rows.json",
            "clip-{index}",
            1,
            true,
            &outcomes,
        );
        assert!(text.contains(r#""status":"failed""#), "{text}");
        assert!(
            text.contains(r#""passed":1,"failed":1,"skipped":1"#),
            "{text}"
        );
        assert!(text.contains(r#""index":2,"status":"skipped""#), "{text}");
        // Machine-readable JSON again parses.
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["rows"].as_array().unwrap().len(), 3);
    }
}
