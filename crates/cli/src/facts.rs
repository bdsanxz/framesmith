//! The facts of a project that passed the check, worked out once. The text of `check` and `info` and the
//! JSON document of `info --json` are all made from these values, so the forms cannot differ.

use crate::text;
use model::{CheckReport, Diagnostic, Manifest};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use timebase::{frame_count, Time};

/// One clip and how long it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipDuration {
    pub label: String,
    pub duration: Time,
    /// `true` when the duration was not written and was worked out.
    pub filled_in: bool,
}

/// One clip whose start is relative to another clip: the start it resolved to, and how it was written.
#[derive(Clone, Debug, PartialEq)]
pub struct RelativeStart {
    pub label: String,
    pub start: Time,
    /// The start as the author wrote it (`after:intro`).
    pub written: String,
}

/// What `check` knows about the composition.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub fps_filled_in: bool,
    pub length: Time,
    pub length_filled_in: bool,
    pub frames: u64,
    pub clips: usize,
    pub tweens: usize,
    pub clip_durations: Vec<ClipDuration>,
    /// The clips whose start is relative, in document order.
    pub relative_starts: Vec<RelativeStart>,
}

impl Facts {
    pub fn of(manifest: &Manifest) -> Facts {
        Facts {
            name: manifest.name.clone(),
            width: manifest.width,
            height: manifest.height,
            fps: manifest.fps,
            fps_filled_in: !manifest.fps_written,
            length: manifest.length,
            length_filled_in: !manifest.length_written,
            frames: frame_count(manifest.length, manifest.fps),
            clips: manifest.clips.len(),
            tweens: manifest.tweens.len(),
            clip_durations: manifest
                .clips
                .iter()
                .map(|clip| ClipDuration {
                    label: clip.label.clone(),
                    duration: clip.duration,
                    filled_in: !clip.duration_written,
                })
                .collect(),
            relative_starts: manifest
                .clips
                .iter()
                .filter_map(|clip| {
                    Some(RelativeStart {
                        label: clip.label.clone(),
                        start: clip.start,
                        written: clip.relative_start.clone()?,
                    })
                })
                .collect(),
        }
    }
}

/// How many compositions the project holds, the root included. A project has one composition today;
/// when it can hold several, this is the one place that counts them.
fn composition_count(_manifest: &Manifest) -> usize {
    1
}

/// The path a render with no `--output` writes to, made absolute the way `render` makes it.
fn render_output(project: &Path) -> PathBuf {
    let path = model::default_output(project);
    std::path::absolute(&path).unwrap_or(path)
}

/// Everything `info` knows about a project that passed: the facts of `check`, and three more.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub project: String,
    pub composition: Facts,
    pub compositions: usize,
    pub output: String,
}

impl Info {
    pub fn of(manifest: &Manifest, project: &Path) -> Info {
        Info {
            project: std::path::absolute(project)
                .unwrap_or_else(|_| project.to_path_buf())
                .display()
                .to_string(),
            composition: Facts::of(manifest),
            compositions: composition_count(manifest),
            output: render_output(project).display().to_string(),
        }
    }
}

/// Whether the text form writes the time exactly: its denominator divides a thousand million, so it is a
/// decimal fraction of at most nine decimals. Every time a project can hold is one (the reader refuses a
/// written time with more decimals, and a sum or difference of such times has no more).
fn is_decimal(time: Time) -> bool {
    time.to_exact()
        .split_once('/')
        .and_then(|(_, den)| den.parse::<i128>().ok())
        .is_some_and(|den| den > 0 && 1_000_000_000 % den == 0)
}

/// A length in seconds as a JSON number: a whole number of seconds has no decimal point. The one
/// conversion of `render --json`, `info --json` and `timeline --json`. A time the text form writes
/// exactly is read back from that text, so the number is the one a program gets by reading the printed
/// text as a number, to the last digit a 64-bit number holds.
pub fn seconds(time: Time) -> Value {
    let seconds = if is_decimal(time) {
        time.to_string().parse::<f64>().unwrap_or_else(|_| time.to_f64())
    } else {
        time.to_f64()
    };
    if seconds.fract() == 0.0 {
        json!(seconds as u64)
    } else {
        json!(seconds)
    }
}

/// The diagnostics as the `diagnostics` of a JSON document: the one structure of `render --json` and
/// `info --json`.
pub fn diagnostics_value(diagnostics: &[Diagnostic]) -> Value {
    json!(diagnostics)
}

/// The outcome word of an exit code of the check.
pub fn outcome_word(code: i32) -> &'static str {
    match code {
        0 => "passed",
        2 => "refused",
        _ => "failed",
    }
}

/// The passed report's manifest: present only for a project a render would proceed with.
fn passed_manifest(report: &CheckReport) -> Option<&Manifest> {
    match &report.manifest {
        Some(manifest) if report.exit_code() == 0 => Some(manifest),
        _ => None,
    }
}

/// The exit code and the document of a report that did not pass: the outcome, the run id and the
/// diagnostics of `check`, in the order `check` prints them. The same for every command with `--json`.
fn failure_document(report: &CheckReport, run_id: &str) -> (i32, Value) {
    // Never 0 here: a report with no manifest did not pass.
    let code = match report.exit_code() {
        0 => 1,
        code => code,
    };
    let document = json!({
        "outcome": outcome_word(code),
        "run_id": run_id,
        "diagnostics": diagnostics_value(&report.diagnostics),
    });
    (code, document)
}

/// The facts the `--json` documents of `info` and `timeline` share, under the names they share.
fn insert_header_facts(document: &mut Map<String, Value>, facts: &Facts) {
    document.insert("composition".into(), json!(facts.name));
    document.insert("width".into(), json!(facts.width));
    document.insert("height".into(), json!(facts.height));
    document.insert("fps".into(), json!(facts.fps));
    document.insert("fps_filled_in".into(), json!(facts.fps_filled_in));
    document.insert("duration".into(), seconds(facts.length));
    document.insert("duration_filled_in".into(), json!(facts.length_filled_in));
    document.insert("frames".into(), json!(facts.frames));
}

/// The exit code and the `info --json` document of a report.
pub fn info_document(report: &CheckReport, project: &Path, run_id: &str) -> (i32, Value) {
    let Some(manifest) = passed_manifest(report) else {
        return failure_document(report, run_id);
    };
    let info = Info::of(manifest, project);
    let facts = &info.composition;
    let clip_durations: Vec<Value> = facts
        .clip_durations
        .iter()
        .map(|clip| {
            json!({
                "label": clip.label,
                "duration": seconds(clip.duration),
                "duration_filled_in": clip.filled_in,
            })
        })
        .collect();
    let mut document = Map::new();
    document.insert("outcome".into(), json!(outcome_word(0)));
    document.insert("run_id".into(), json!(run_id));
    document.insert("project".into(), json!(info.project));
    insert_header_facts(&mut document, facts);
    document.insert("compositions".into(), json!(info.compositions));
    document.insert("clips".into(), json!(facts.clips));
    document.insert("tweens".into(), json!(facts.tweens));
    document.insert("output".into(), json!(info.output));
    document.insert("clip_durations".into(), json!(clip_durations));
    document.insert("diagnostics".into(), diagnostics_value(&report.diagnostics));
    (0, Value::Object(document))
}

/// One clip of the timeline: its name and the window the project resolved. The text form and the JSON
/// document of `timeline` are both written from these values, so a time is taken once. A later ticket
/// adds a value to a clip here, then in `text::timeline_text` and in `clip_value`.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineClip {
    /// How a message names the clip (`#card`).
    pub name: String,
    pub start: Time,
    pub end: Time,
    pub duration: Time,
    /// `true` when the duration was not written and was worked out.
    pub duration_filled_in: bool,
    /// The clip's lane: the number written, or 0.
    pub lane: i64,
    /// `true` when no lane was written and 0 was filled in.
    pub lane_filled_in: bool,
}

/// What `timeline` shows of a project that passed: the facts of the composition, and its clips in
/// document order.
#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    pub facts: Facts,
    pub clips: Vec<TimelineClip>,
}

impl Timeline {
    pub fn of(manifest: &Manifest) -> Timeline {
        Timeline {
            facts: Facts::of(manifest),
            clips: manifest
                .clips
                .iter()
                .map(|clip| TimelineClip {
                    name: clip.label.clone(),
                    start: clip.start,
                    end: clip.end(),
                    duration: clip.duration,
                    duration_filled_in: !clip.duration_written,
                    lane: clip.lane(),
                    lane_filled_in: clip.lane_filled_in(),
                })
                .collect(),
        }
    }

    /// The lanes in number order, each with the indices (into `clips`) of its clips in document order.
    /// A lane that holds no clip is not in the list.
    pub fn lanes(&self) -> Vec<(i64, Vec<usize>)> {
        let mut lanes: std::collections::BTreeMap<i64, Vec<usize>> = std::collections::BTreeMap::new();
        for (index, clip) in self.clips.iter().enumerate() {
            lanes.entry(clip.lane).or_default().push(index);
        }
        lanes.into_iter().collect()
    }
}

/// One clip as an entry of `clips` in the `timeline --json` document.
fn clip_value(clip: &TimelineClip) -> Value {
    let mut entry = Map::new();
    entry.insert("name".into(), json!(clip.name));
    entry.insert("start".into(), seconds(clip.start));
    entry.insert("end".into(), seconds(clip.end));
    entry.insert("duration".into(), seconds(clip.duration));
    entry.insert("duration_filled_in".into(), json!(clip.duration_filled_in));
    entry.insert("lane".into(), json!(clip.lane));
    entry.insert("lane_filled_in".into(), json!(clip.lane_filled_in));
    Value::Object(entry)
}

/// The exit code and the `timeline --json` document of a report.
pub fn timeline_document(report: &CheckReport, run_id: &str) -> (i32, Value) {
    let Some(manifest) = passed_manifest(report) else {
        return failure_document(report, run_id);
    };
    let timeline = Timeline::of(manifest);
    let mut document = Map::new();
    document.insert("outcome".into(), json!(outcome_word(0)));
    document.insert("run_id".into(), json!(run_id));
    insert_header_facts(&mut document, &timeline.facts);
    document.insert("clips".into(), Value::Array(timeline.clips.iter().map(clip_value).collect()));
    document.insert("diagnostics".into(), diagnostics_value(&report.diagnostics));
    (0, Value::Object(document))
}

/// Everything `info` prints in its text form for a report.
pub fn info_text(report: &CheckReport, project: &Path) -> String {
    let manifest = match &report.manifest {
        Some(manifest) if report.exit_code() == 0 => manifest,
        // A project `check` would not pass: exactly what `check` prints.
        _ => return text::check_text(report),
    };
    let info = Info::of(manifest, project);
    let mut out = text::warnings_before(report);
    for (label, value) in text::info_rows(&info) {
        out.push_str(&text::fact_line("", label, &value));
    }
    out.push_str(&text::filled_clip_lines(&info.composition, ""));
    out.push_str(&text::relative_start_lines(&info.composition, ""));
    out
}

/// Everything `timeline` prints in its text form for a report: for a project `check` would not pass,
/// exactly what `check` prints; otherwise the warnings, then the timeline of the manifest.
pub fn timeline_text(report: &CheckReport) -> String {
    match &report.manifest {
        Some(manifest) if report.exit_code() == 0 => {
            let mut out = text::warnings_before(report);
            out.push_str(&text::timeline_text(&Timeline::of(manifest)));
            out
        }
        _ => text::check_text(report),
    }
}
