//! `framesmith timeline`: the composition's clips with the start, end and duration the project resolved,
//! as text or as one JSON document. It reads the project through the same check as every other command and prints; it edits
//! nothing and starts no other program.

use crate::facts;
use model::Severity;
use std::io::Write;
use std::path::Path;

pub fn run(project: &Path, json: bool, run_id: &str) -> i32 {
    tracing::info!(target: "cli", command = "timeline", project = %project.display(), json, "command started");
    let report = model::check(project);
    let (code, printed) = if json {
        let (code, document) = facts::timeline_document(&report, run_id);
        (code, format!("{document}\n"))
    } else {
        (report.exit_code(), facts::timeline_text(&report))
    };
    // A closed pipe is not worth a crash: the exit code still tells the result.
    let _ = std::io::stdout().write_all(printed.as_bytes());
    let count = |kind: Severity| report.diagnostics.iter().filter(|d| d.kind == kind).count();
    tracing::info!(
        target: "cli",
        command = "timeline",
        exit = code,
        errors = count(Severity::Error),
        unsupported = count(Severity::Unsupported),
        unknown = count(Severity::Unknown),
        warnings = count(Severity::Warning),
        clips = report.manifest.as_ref().map_or(0, |manifest| manifest.clips.len()),
        "command finished"
    );
    code
}
