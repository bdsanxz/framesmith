//! `framesmith check`: reads the project and says what it found.

use crate::text;
use model::Severity;
use std::io::Write;
use std::path::Path;

pub fn run(project: &Path) -> i32 {
    tracing::info!(target: "cli", command = "check", project = %project.display(), "command started");
    let report = model::check(project);
    let code = report.exit_code();
    // A closed pipe is not worth a crash: the exit code still tells the result.
    let _ = std::io::stdout().write_all(text::check_text(&report).as_bytes());
    let count = |kind: Severity| report.diagnostics.iter().filter(|d| d.kind == kind).count();
    tracing::info!(
        target: "cli",
        command = "check",
        exit = code,
        errors = count(Severity::Error),
        unsupported = count(Severity::Unsupported),
        unknown = count(Severity::Unknown),
        warnings = count(Severity::Warning),
        "command finished"
    );
    code
}
