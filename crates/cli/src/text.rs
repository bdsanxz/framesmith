//! The words the commands print. Nothing here depends on colour.

use model::{CheckReport, Diagnostic, Manifest, Severity};
use render::Failure;
use timebase::frame_count;

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

fn filled(written: bool) -> &'static str {
    if written {
        ""
    } else {
        " (filled in)"
    }
}

/// The facts of a composition, one per line, then each worked-out clip duration.
pub fn facts(manifest: &Manifest) -> String {
    let mut text = String::new();
    let mut fact = |name: &str, value: String| text.push_str(&format!("  {name:<13}{value}\n"));
    fact("composition", manifest.name.clone());
    fact("size", format!("{}x{}", manifest.width, manifest.height));
    fact("rate", format!("{} fps{}", manifest.fps, filled(manifest.fps_written)));
    fact("length", format!("{} s{}", manifest.length, filled(manifest.length_written)));
    fact("frames", frame_count(manifest.length, manifest.fps).to_string());
    fact("clips", manifest.clips.len().to_string());
    fact("tweens", manifest.tweens.len().to_string());
    for clip in manifest.clips.iter().filter(|clip| !clip.duration_written) {
        text.push_str(&format!("  clip {}  duration {} s (filled in)\n", clip.label, clip.duration));
    }
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
            text.push_str(&facts(manifest));
        }
    }
    text
}
