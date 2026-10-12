//! `framesmith preview`: shows a project in a browser. It reads the project, starts the preview server on
//! the loopback address, prints the address and keeps running. Every option of the command is read here,
//! and its help is written here (`HELP`); see `docs/spec/preview.md`.

use crate::args::Usage;
use crate::facts::Facts;
use crate::{render as command, text};
use model::{Diagnostic, Severity};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

/// The help lines of the command, placed among the commands by `lib.rs`.
pub const HELP: &str = "  framesmith preview <project>  Show the project in a browser, at an address on this machine.\n";

fn usage(message: String) -> Usage {
    Usage { message, json: false }
}

/// The words after `preview`: one project folder, and nothing else yet.
fn project_of(words: &[OsString]) -> Result<PathBuf, Usage> {
    let mut project: Option<PathBuf> = None;
    for word in words {
        let shown = word.to_string_lossy();
        if shown.starts_with("--") {
            return Err(usage(format!("`framesmith preview` has no option {shown}.")));
        }
        if project.is_some() {
            return Err(usage(format!(
                "`framesmith preview` takes one project folder; {shown} is one word too many."
            )));
        }
        project = Some(PathBuf::from(word));
    }
    project.ok_or_else(|| usage("`framesmith preview` needs the path of a project folder.".to_string()))
}

fn stderr(text: &str) {
    let _ = std::io::stderr().write_all(text.as_bytes());
}

/// A project that does not pass the check is not previewed (until ticket T074): the diagnostics and the
/// result line are printed as `render` prints them, and nothing listens.
fn blocked(diagnostics: &[Diagnostic]) -> i32 {
    let code = match model::exit_code(diagnostics) {
        0 => 1,
        code => code,
    };
    for diagnostic in diagnostics {
        stderr(&format!("{}\n", text::diagnostic_block(diagnostic)));
    }
    stderr(&format!("{}\n", text::result_line(diagnostics)));
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    let count = |kind: Severity| diagnostics.iter().filter(|d| d.kind == kind).count();
    tracing::error!(
        target: "cli",
        command = "preview",
        exit = code,
        errors = count(Severity::Error),
        unsupported = count(Severity::Unsupported),
        unknown = count(Severity::Unknown),
        codes = %codes.join(","),
        "command finished: the project did not pass the check, nothing was started"
    );
    code
}

pub fn run(words: &[OsString], run_id: &str) -> i32 {
    let project = match project_of(words) {
        Ok(project) => project,
        Err(usage) => return command::usage_error(&usage, run_id),
    };
    tracing::info!(target: "cli", command = "preview", project = %project.display(), "command started");
    let report = model::check(&project);
    let manifest = match report.manifest {
        Some(manifest) if report.exit_code() == 0 => manifest,
        _ => return blocked(&report.diagnostics),
    };
    for warning in &report.diagnostics {
        stderr(&format!("{}\n", text::diagnostic_block(warning)));
    }
    let preview = match server::Preview::start(&project) {
        Ok(preview) => preview,
        Err(error) => {
            let failure = ::render::Failure::new(
                "FS-P001",
                format!("The preview cannot listen on {}: {error}.", server::ADDRESS_HOST),
                "Check that this machine lets programs listen on 127.0.0.1 (a firewall, a sandbox or a denied network does not), then run the command again.",
            );
            stderr(&text::failure_block(&failure));
            tracing::error!(target: "cli", command = "preview", exit = 1, code = "FS-P001", reason = %error, "command finished: the preview could not listen");
            return 1;
        }
    };
    let facts = Facts::of(&manifest);
    let line = format!(
        "Preview at {}  {}  {}x{}  {} fps  {} s\n",
        preview.address(),
        facts.name,
        facts.width,
        facts.height,
        facts.fps,
        facts.length
    );
    // A closed pipe is not worth a crash.
    let mut out = std::io::stdout();
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();
    tracing::info!(
        target: "cli",
        command = "preview",
        address = %preview.address(),
        composition = %facts.name,
        width = facts.width,
        height = facts.height,
        fps = facts.fps,
        "preview running"
    );
    preview.wait();
    0
}
