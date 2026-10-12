//! The words the commands print. Nothing here depends on colour.

use crate::facts::{Facts, Info, Timeline};
use model::{CheckReport, Diagnostic, Severity};
use render::Failure;

fn counted(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A diagnostic as three lines: kind, code and place; description; remedy.
pub fn diagnostic_block(diagnostic: &Diagnostic) -> String {
    format!(
        "{} {} {}\n  {}\n  fix: {}\n",
        diagnostic.kind.word(),
        diagnostic.code,
        diagnostic.location(),
        diagnostic.description,
        diagnostic.remedy
    )
}

/// An error that is not about a place in a file.
pub fn failure_block(failure: &Failure) -> String {
    let kind = if failure.code.starts_with("FS-U") { "unsupported" } else { "error" };
    format!("{kind} {}\n  {}\n  fix: {}\n", failure.code, failure.description, failure.remedy)
}

/// The last line of a check: the result in words, with the counts when it
/// did not pass.
pub fn result_line(diagnostics: &[Diagnostic]) -> String {
    let count = |kind: Severity| diagnostics.iter().filter(|d| d.kind == kind).count();
    let result = match model::exit_code(diagnostics) {
        0 => return "check passed".to_string(),
        1 => "check failed",
        _ => "check refused",
    };
    format!(
        "{result}: {}, {} unsupported, {} unknown, {}",
        counted(count(Severity::Error), "error", "errors"),
        count(Severity::Unsupported),
        count(Severity::Unknown),
        counted(count(Severity::Warning), "warning", "warnings")
    )
}

fn filled(filled_in: bool) -> &'static str {
    if filled_in {
        " (filled in)"
    } else {
        ""
    }
}

/// The facts of a composition as (label, value) rows, in the order `check` and `info` print them. The
/// one place that decides what a fact is called and how it is worded.
pub fn fact_rows(facts: &Facts) -> Vec<(&'static str, String)> {
    vec![
        ("composition", facts.name.clone()),
        ("size", format!("{}x{}", facts.width, facts.height)),
        ("rate", format!("{} fps{}", facts.fps, filled(facts.fps_filled_in))),
        ("length", format!("{} s{}", facts.length, filled(facts.length_filled_in))),
        ("frames", facts.frames.to_string()),
        ("clips", facts.clips.to_string()),
        ("tweens", facts.tweens.to_string()),
    ]
}

/// The rows `info` prints: the facts of `check`, framed by the ones only `info` knows.
pub fn info_rows(info: &Info) -> Vec<(&'static str, String)> {
    let mut rows = fact_rows(&info.composition);
    let at = rows.iter().position(|(label, _)| *label == "clips").unwrap_or(rows.len());
    rows.insert(at, ("compositions", info.compositions.to_string()));
    rows.insert(0, ("project", info.project.clone()));
    rows.push(("output", info.output.clone()));
    rows
}

/// One row as a line: the label padded to 13 columns, then the value.
pub fn fact_line(indent: &str, label: &str, value: &str) -> String {
    format!("{indent}{label:<13}{value}\n")
}

/// One line for each clip whose duration was worked out.
pub fn filled_clip_lines(facts: &Facts, indent: &str) -> String {
    facts
        .clip_durations
        .iter()
        .filter(|clip| clip.filled_in)
        .map(|clip| format!("{indent}clip {}  duration {} s (filled in)\n", clip.label, clip.duration))
        .collect()
}

/// One line for each clip whose start is relative to another clip: the start it resolved to, in seconds,
/// and the form the author wrote. A project with only written times has none.
pub fn relative_start_lines(facts: &Facts, indent: &str) -> String {
    facts
        .relative_starts
        .iter()
        .map(|clip| format!("{indent}clip {}  start {} s ({})\n", clip.label, clip.start, clip.written))
        .collect()
}

/// The facts of a composition, one per line, then each worked-out clip duration and each relative start.
pub fn facts(facts: &Facts) -> String {
    let mut text = String::new();
    for (label, value) in fact_rows(facts) {
        text.push_str(&fact_line("  ", label, &value));
    }
    text.push_str(&filled_clip_lines(facts, "  "));
    text.push_str(&relative_start_lines(facts, "  "));
    text
}

/// Everything `check` prints for a report.
pub fn check_text(report: &CheckReport) -> String {
    let mut text = String::new();
    for diagnostic in &report.diagnostics {
        text.push_str(&diagnostic_block(diagnostic));
        text.push('\n');
    }
    text.push_str(&result_line(&report.diagnostics));
    text.push('\n');
    if report.exit_code() == 0 {
        if let Some(manifest) = &report.manifest {
            text.push_str(&facts(&Facts::of(manifest)));
        }
    }
    text
}

/// The warnings of a project that passed, each followed by a blank line: what `info` and `timeline`
/// print before their own lines.
pub fn warnings_before(report: &CheckReport) -> String {
    let mut out = String::new();
    for warning in &report.diagnostics {
        out.push_str(&diagnostic_block(warning));
        out.push('\n');
    }
    out
}

/// The value of one fact row, worded exactly as `check` and `info` word it (marks included).
fn fact_value(facts: &Facts, label: &str) -> String {
    fact_rows(facts)
        .into_iter()
        .find(|(name, _)| *name == label)
        .map(|(_, value)| value)
        .unwrap_or_default()
}

/// The header line of a timeline: the composition's name, size, rate, length and frame count. The size,
/// rate and length are the values of the rows of `check` and `info`, so the marks are the same.
pub fn timeline_header(facts: &Facts) -> String {
    format!(
        "{}  {}  {}  {}  {}",
        facts.name,
        fact_value(facts, "size"),
        fact_value(facts, "rate"),
        fact_value(facts, "length"),
        counted(facts.frames as usize, "frame", "frames")
    )
}

/// A timeline: the header, then the lanes in number order, each a line of its own followed by one line for
/// each of its clips in document order, with the clip's name and the start, end and duration the project
/// resolved, as `Timeline::of` took them from the manifest. A clip whose lane was filled in carries a mark
/// at the end of its line. Each column is as wide as its widest cell over all the clips, so the lines read
/// as a table from one lane to the next; no line ends in a space.
pub fn timeline_text(timeline: &Timeline) -> String {
    let mut out = timeline_header(&timeline.facts);
    out.push('\n');
    let rows: Vec<[String; 4]> = timeline
        .clips
        .iter()
        .map(|clip| {
            [
                clip.name.clone(),
                format!("{} s", clip.start),
                format!("{} s", clip.end),
                format!(
                    "{} s{}{}",
                    clip.duration,
                    filled(clip.duration_filled_in),
                    if clip.lane_filled_in { " (lane filled in)" } else { "" }
                ),
            ]
        })
        .collect();
    let width = |column: usize| rows.iter().map(|row| row[column].chars().count()).max().unwrap_or(0);
    let (names, starts, ends) = (width(0), width(1), width(2));
    for (lane, members) in timeline.lanes() {
        out.push_str(&format!("lane {lane}\n"));
        for index in members {
            let [name, start, end, duration] = &rows[index];
            out.push_str(&format!("  {name:<names$}  {start:<starts$} to {end:<ends$}  {duration}\n"));
        }
    }
    out
}
