//! `framesmith info`: what Framesmith understands a project to be, as text or as one JSON document.
//! Both are made from the facts of `facts.rs`, which `check` prints from too, so the commands and the
//! two forms cannot differ.

use crate::facts;
use model::Severity;
use std::io::Write;
use std::path::Path;

pub fn run(project: &Path, json: bool, run_id: &str) -> i32 {
    tracing::info!(target: "cli", command = "info", project = %project.display(), json, "command started");
    let report = model::check(project);
    let (code, printed) = if json {
        let (code, document) = facts::info_document(&report, project, run_id);
        (code, format!("{document}\n"))
    } else {
        (report.exit_code(), facts::info_text(&report, project))
    };
    // A closed pipe is not worth a crash: the exit code still tells the result.
    let _ = std::io::stdout().write_all(printed.as_bytes());
    let count = |kind: Severity| report.diagnostics.iter().filter(|d| d.kind == kind).count();
    tracing::info!(
        target: "cli",
        command = "info",
        exit = code,
        errors = count(Severity::Error),
        unsupported = count(Severity::Unsupported),
        unknown = count(Severity::Unknown),
        warnings = count(Severity::Warning),
        "command finished"
    );
    code
}
