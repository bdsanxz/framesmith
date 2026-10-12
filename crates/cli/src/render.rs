//! `framesmith render`: the steps before the stages, then the stages, then
//! the result as text or as one JSON document.

use crate::args::{RenderOptions, Usage};
use crate::facts;
use crate::progress::Progress;
use crate::{signal, text};
use browser::{FindError, Places};
use model::{Diagnostic, Manifest};
use render::{Debug, Failure, Setup, Stage, Staging, Stopped, Summary};
use serde_json::{json, Value};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use timebase::{parse_fps, FpsError, MAX_FPS, MIN_FPS};

/// How a render command ends.
enum End {
    Completed(Summary),
    /// The check stopped it; the exit code follows from the diagnostics.
    Blocked(Vec<Diagnostic>),
    /// One failure, or both missing prerequisites.
    Failed(Vec<Failure>),
    Cancelled(Option<Stage>),
}

fn stderr(text: &str) {
    let _ = std::io::stderr().write_all(text.as_bytes());
}

fn fps_option(written: &str) -> Result<u32, Failure> {
    parse_fps(written).map_err(|error| match error {
        FpsError::Ratio => Failure::new(
            "FS-U004",
            format!("--fps {written} is a ratio; ratio frame rates are not supported in this version."),
            format!("Give --fps a whole number from {MIN_FPS} to {MAX_FPS}."),
        ),
        FpsError::Invalid => Failure::new(
            "FS-R004",
            format!("--fps {written} is not acceptable: the frame rate must be a whole number from {MIN_FPS} to {MAX_FPS}."),
            format!("Give --fps a whole number from {MIN_FPS} to {MAX_FPS}, for example --fps 24."),
        ),
    })
}

fn missing_browser(error: FindError) -> Failure {
    let failure = match error {
        FindError::NotFound => Failure::new(
            "FS-R001",
            "No Chromium-based browser was found.",
            "Install a Chromium-based browser, for example Google Chrome or Chromium, or set FRAMESMITH_BROWSER to the path of one.",
        ),
        FindError::Unusable { path, reason } => Failure::new(
            "FS-R002",
            format!("FRAMESMITH_BROWSER names {}, but {reason}.", path.display()),
            "Set FRAMESMITH_BROWSER to the path of a Chromium-based browser program, or unset it so Framesmith looks for one.",
        ),
    };
    Failure {
        program: Some("browser"),
        ..failure
    }
}

fn missing_ffmpeg() -> Failure {
    Failure {
        program: Some("ffmpeg"),
        ..Failure::new(
            "FS-R003",
            "ffmpeg was not found on the search path.",
            "Install ffmpeg and make sure the folder that holds it is on PATH.",
        )
    }
}

/// Runs the steps in order and stops at the first that fails.
fn attempt(options: &RenderOptions, run_id: &str, cancel: &Arc<AtomicBool>, progress: &mut Progress<Box<dyn Write>>) -> End {
    let cancelled = || cancel.load(Ordering::SeqCst);

    let fps_asked = match options.fps.as_deref().map(fps_option) {
        Some(Err(failure)) => return End::Failed(vec![failure]),
        Some(Ok(fps)) => Some(fps),
        None => None,
    };

    let report = model::check(&options.project);
    let manifest: Manifest = match report.manifest {
        Some(manifest) if report.exit_code() == 0 => manifest,
        _ => return End::Blocked(report.diagnostics),
    };
    for warning in &report.diagnostics {
        stderr(&format!("{}\n", text::diagnostic_block(warning)));
    }

    let places = Places::from_process();
    let found_browser = browser::find(&places).map_err(missing_browser);
    let found_ffmpeg = media::find_ffmpeg(&places.path).ok_or_else(missing_ffmpeg);
    let (browser, ffmpeg) = match (found_browser, found_ffmpeg) {
        (Ok(browser), Ok(ffmpeg)) => (browser, ffmpeg),
        (browser, ffmpeg) => return End::Failed(browser.err().into_iter().chain(ffmpeg.err()).collect()),
    };

    if cancelled() {
        return End::Cancelled(None);
    }
    let output = options.output.clone().unwrap_or_else(|| model::default_output(&options.project));
    let located = std::path::absolute(&output).map_err(|error| {
        Failure::new(
            "FS-R005",
            format!("The output cannot be written at {}: {error}.", output.display()),
            "Choose an output path in a folder you can write to, with --output <path>.",
        )
    });
    let mut staging = match located.and_then(|output| Staging::create(&output, run_id)) {
        Ok(staging) => staging,
        Err(failure) => return End::Failed(vec![failure]),
    };
    let project = std::fs::canonicalize(&options.project).unwrap_or_else(|_| options.project.clone());
    let setup = Setup {
        run_id,
        project: &project,
        manifest: &manifest,
        fps: fps_asked.unwrap_or(manifest.fps),
        browser: &browser,
        ffmpeg: &ffmpeg,
        cancel: Arc::clone(cancel),
        debug: Debug::from_env(),
    };
    match render::run(&setup, &mut staging, progress) {
        Ok(summary) => End::Completed(summary),
        Err(Stopped::Failed(failure)) => End::Failed(vec![failure]),
        Err(Stopped::Cancelled(stage)) => End::Cancelled(stage),
    }
}

fn failure_json(failure: &Failure) -> Value {
    let mut error = json!({
        "code": failure.code,
        "description": failure.description,
        "remedy": failure.remedy,
    });
    if let Some(program) = failure.program {
        error["program"] = json!(program);
    }
    if let Some(stage) = failure.stage {
        error["stage"] = json!(stage.name());
    }
    if let Some(frame) = failure.frame {
        error["frame"] = json!(frame);
    }
    error
}

fn outcome_word(code: i32) -> &'static str {
    match code {
        0 => "completed",
        2 => "refused",
        130 => "cancelled",
        _ => "failed",
    }
}

/// The exit code and the JSON document of an ending.
fn document(end: &End, run_id: &str) -> (i32, Value) {
    match end {
        End::Completed(summary) => {
            let duration = facts::seconds(summary.length);
            let document = json!({
                "outcome": "completed",
                "run_id": run_id,
                "output": summary.output.to_string_lossy(),
                "width": summary.width,
                "height": summary.height,
                "fps": summary.fps,
                "frames": summary.frames,
                "duration": duration,
            });
            (0, document)
        }
        End::Blocked(diagnostics) => {
            let code = match model::exit_code(diagnostics) {
                0 => 1,
                code => code,
            };
            (
                code,
                json!({ "outcome": outcome_word(code), "run_id": run_id, "diagnostics": facts::diagnostics_value(diagnostics) }),
            )
        }
        End::Failed(failures) => {
            let code = if failures.iter().all(|f| f.exit_code() == 2) { 2 } else { 1 };
            let mut document = json!({ "outcome": outcome_word(code), "run_id": run_id });
            if let Some(first) = failures.first() {
                document["error"] = failure_json(first);
            }
            if failures.len() > 1 {
                document["errors"] = failures.iter().map(failure_json).collect();
            }
            (code, document)
        }
        End::Cancelled(stage) => {
            let mut document = json!({ "outcome": "cancelled", "run_id": run_id });
            if let Some(stage) = stage {
                document["stage"] = json!(stage.name());
            }
            (130, document)
        }
    }
}

/// Says how the command ended, in words, and records it in the log.
fn tell(end: &End, code: i32, progress: &mut Progress<Box<dyn Write>>) {
    progress.end_line();
    match end {
        End::Completed(summary) => {
            progress.say(&format!(
                "Rendered {} frames, {}x{}, {} fps, in {:.1} s",
                summary.frames,
                summary.width,
                summary.height,
                summary.fps,
                summary.elapsed.as_secs_f64()
            ));
            tracing::info!(target: "cli", command = "render", exit = code, outcome = "completed", output = %summary.output.display(), frames = summary.frames, "command finished");
        }
        End::Blocked(diagnostics) => {
            for diagnostic in diagnostics {
                stderr(&format!("{}\n", text::diagnostic_block(diagnostic)));
            }
            stderr(&format!("{}\n", text::result_line(diagnostics)));
            let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
            tracing::error!(target: "cli", command = "render", exit = code, outcome = outcome_word(code), codes = %codes.join(","), "command finished: the project did not pass the check");
        }
        End::Failed(failures) => {
            for failure in failures {
                stderr(&text::failure_block(failure));
            }
            if let Some((failure, stage)) = failures.first().and_then(|f| Some((f, f.stage?))) {
                progress.say(&format!("render stopped in {}: {}", stage.name(), failure.description));
            }
            let codes: Vec<&str> = failures.iter().map(|f| f.code.as_str()).collect();
            let stage = failures.first().and_then(|f| f.stage).map_or("none", Stage::name);
            tracing::error!(target: "cli", command = "render", exit = code, outcome = outcome_word(code), codes = %codes.join(","), stage, "command finished");
        }
        End::Cancelled(stage) => {
            match stage {
                Some(stage) => progress.say(&format!("Render cancelled (in {})", stage.name())),
                None => progress.say("Render cancelled"),
            }
            tracing::warn!(target: "cli", command = "render", exit = code, outcome = "cancelled", stage = stage.map_or("none", Stage::name), "command finished");
        }
    }
}

fn progress_for(json: bool, output: &Path) -> Progress<Box<dyn Write>> {
    let shown = output.display().to_string();
    if json {
        let stream = std::io::stderr();
        let live = stream.is_terminal();
        Progress::new(Box::new(stream), live, shown)
    } else {
        let stream = std::io::stdout();
        let live = stream.is_terminal();
        Progress::new(Box::new(stream), live, shown)
    }
}

fn finish(end: &End, json: bool, run_id: &str, progress: &mut Progress<Box<dyn Write>>) -> i32 {
    let (code, document) = document(end, run_id);
    tell(end, code, progress);
    if json {
        let _ = writeln!(std::io::stdout(), "{document}");
    }
    code
}

pub fn run(options: &RenderOptions, run_id: &str) -> i32 {
    let cancel = signal::watch();
    let output: PathBuf = options.output.clone().unwrap_or_else(|| model::default_output(&options.project));
    tracing::info!(
        target: "cli",
        command = "render",
        project = %options.project.display(),
        output = %output.display(),
        fps = options.fps.as_deref().unwrap_or("as written"),
        json = options.json,
        "command started"
    );
    let mut progress = progress_for(options.json, &output);
    let end = attempt(options, run_id, &cancel, &mut progress);
    finish(&end, options.json, run_id, &mut progress)
}

/// A command line that could not be read: FS-R004, exit 1.
pub fn usage_error(usage: &Usage, run_id: &str) -> i32 {
    tracing::info!(target: "cli", command = "unreadable", "command started");
    let failure = Failure::new(
        "FS-R004",
        usage.message.clone(),
        "Run `framesmith --help` to see the commands and their options.",
    );
    let mut progress = progress_for(usage.json, Path::new(""));
    finish(&End::Failed(vec![failure]), usage.json, run_id, &mut progress)
}
