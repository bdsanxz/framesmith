//! The `framesmith` command.
//!
//! Behaviour is specified in `docs/spec/check-command.md` and `docs/spec/render-command.md`. This
//! version has two commands, `check` and `render`.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use draw::Renderer;
use draw_cpu::CpuRenderer;
use project::{Code, Kind, OpenError, Options, Project, Report, Settings};
use render::Scene;

pub mod runlog;

/// The exit code of a command that found nothing wrong.
pub const EXIT_OK: u8 = 0;
/// The exit code of a command line that cannot be read, an invalid project, or no project.
pub const EXIT_FAILED: u8 = 1;
/// The exit code of a project that is valid, except that it uses what this version does not support.
pub const EXIT_UNSUPPORTED: u8 = 2;

const CHECK_USAGE: &str = "usage: framesmith check <path>";
const RENDER_USAGE: &str = "usage: framesmith render <path>";

/// Runs the command with the arguments after its name. Results go to `out`, a command line that
/// cannot be read to `err`. Returns the exit code.
///
/// The run is logged under one run id (`docs/spec/run-log.md`); a log that cannot be written adds
/// one line to `err` and changes nothing else.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let log = runlog::Log::start();
    let code = log.scope(|| {
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            args = %runlog::word_list(args),
            "command started"
        );
        log.note_unreadable_level();
        command(args, out, err)
    });
    log.finish(err);
    code
}

/// Reads the command line, runs the command, prints its reply and logs its result.
fn command(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let reply = match args.split_first() {
        None => Reply::Unreadable("no command is given".to_string(), BOTH_USAGES),
        Some((command, rest)) if command == "check" => with_path("check", &[CHECK_USAGE], rest, check),
        Some((command, rest)) if command == "render" => match render_line(rest) {
            Ok((path, options)) => render(path, &options),
            Err(why) => Reply::Unreadable(why, &[RENDER_USAGE]),
        },
        Some((command, _)) => Reply::Unreadable(format!("`{}` is not a command", command.to_string_lossy()), BOTH_USAGES),
    };
    let written = match &reply {
        Reply::Printed(lines, ..) => lines.iter().try_for_each(|line| writeln!(out, "{line}")),
        Reply::Unreadable(why, usages) => writeln!(err, "framesmith: {}", Code::CommandLineNotRead.statement(why))
            .and_then(|()| usages.iter().try_for_each(|usage| writeln!(err, "{usage}"))),
    };
    let (code, outcome) = match (written, reply) {
        (Ok(()), Reply::Printed(_, code, outcome)) => (code, outcome),
        (Err(_), Reply::Printed(_, _, outcome)) => (EXIT_FAILED, outcome),
        (_, Reply::Unreadable(..)) => (EXIT_FAILED, Outcome::NotRead),
    };
    outcome.log(code);
    code
}

/// The usage lines of a command line that names no command of the program.
const BOTH_USAGES: &[&str] = &[CHECK_USAGE, RENDER_USAGE];

/// A command that takes the path of one project and no option: reads the words after its name and
/// runs it.
fn with_path(name: &str, usages: &'static [&'static str], rest: &[OsString], run: fn(PathBuf) -> Reply) -> Reply {
    if let Some(option) = rest.iter().find(|word| word.to_string_lossy().starts_with('-')) {
        return Reply::Unreadable(format!("`{name}` takes no option, and {} is one", option.to_string_lossy()), usages);
    }
    match rest {
        [] => Reply::Unreadable(format!("`{name}` needs the path of a project"), usages),
        [path] => run(PathBuf::from(path)),
        [_, extra, ..] => Reply::Unreadable(
            format!(
                "`{name}` takes the path of one project, and {} is one word too many",
                extra.to_string_lossy()
            ),
            usages,
        ),
    }
}

/// Reads the words after `render`: the path of one project and the options, in any order
/// (`docs/spec/render-command.md` section 1). Gives the sentence that says what is wrong otherwise.
fn render_line(rest: &[OsString]) -> Result<(PathBuf, Options), String> {
    let mut options = Options::default();
    let mut path: Option<&OsString> = None;
    let mut words = rest.iter();
    while let Some(word) = words.next() {
        let text = word.to_string_lossy();
        if !text.starts_with('-') {
            if path.is_some() {
                return Err(format!("`render` takes the path of one project, and {text} is one word too many"));
            }
            path = Some(word);
            continue;
        }
        let (name, written) = match text.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (text.to_string(), None),
        };
        if !Options::is_option(&name) {
            return Err(Options::unknown(&name));
        }
        let value = match written {
            Some(value) => value,
            None => match words.next() {
                Some(value) => value.to_string_lossy().into_owned(),
                None => return Err(Options::no_value(&name)),
            },
        };
        options.set(&name, &value)?;
    }
    match path {
        Some(path) => Ok((PathBuf::from(path), options)),
        None => Err("`render` needs the path of a project".to_string()),
    }
}

/// How a command ended, for the log (`docs/spec/run-log.md` section 4).
enum Outcome {
    Ready,
    Invalid {
        problems: usize,
    },
    Unsupported {
        problems: usize,
    },
    /// No composition to read; `problems` counts the reports of the project file before it.
    NoProject {
        code: Code,
        problems: usize,
    },
    NotRead,
    /// `render` wrote the film.
    Rendered {
        frames: u64,
    },
    /// `render` could not make the film.
    VideoNotMade {
        code: Code,
    },
}

impl Outcome {
    /// Writes the last line of the run: the level follows the outcome.
    fn log(&self, exit: u8) {
        match self {
            Outcome::Ready => tracing::info!(outcome = "ready", exit, "command finished"),
            Outcome::Invalid { problems } => {
                tracing::warn!(outcome = "invalid", exit, problems = *problems, "command finished")
            }
            Outcome::Unsupported { problems } => {
                tracing::warn!(outcome = "unsupported", exit, problems = *problems, "command finished")
            }
            Outcome::NoProject { code, problems: 0 } if code.kind() == Kind::Failure => {
                tracing::error!(outcome = "no project", exit, code = code.as_str(), "command finished")
            }
            Outcome::NoProject { code, problems: 0 } => {
                tracing::warn!(outcome = "no project", exit, code = code.as_str(), "command finished")
            }
            Outcome::NoProject { code, problems } if code.kind() == Kind::Failure => {
                tracing::error!(
                    outcome = "no project",
                    exit,
                    code = code.as_str(),
                    problems = *problems,
                    "command finished"
                )
            }
            Outcome::NoProject { code, problems } => {
                tracing::warn!(
                    outcome = "no project",
                    exit,
                    code = code.as_str(),
                    problems = *problems,
                    "command finished"
                )
            }
            Outcome::NotRead => tracing::warn!(outcome = "command line not read", exit, "command finished"),
            Outcome::Rendered { frames } => tracing::info!(outcome = "rendered", exit, frames = *frames, "command finished"),
            Outcome::VideoNotMade { code } => {
                tracing::error!(outcome = "video not made", exit, code = code.as_str(), "command finished")
            }
        }
    }
}

/// What a command has to say, and how it ends.
enum Reply {
    /// Lines for standard output, the exit code, and how it ended.
    Printed(Vec<String>, u8, Outcome),
    /// A command line that cannot be read, with what is wrong with it and the usage lines to print.
    Unreadable(String, &'static [&'static str]),
}

/// `framesmith check <path>`: opens the project that the path names (a folder, a project file or a
/// composition file) and says whether its project file and its composition are valid, or every
/// problem they have.
fn check(path: PathBuf) -> Reply {
    match Project::open(&path) {
        Ok(project) => {
            let root = project.root();
            let frames = if root.frames == 1 {
                "1 frame".to_string()
            } else {
                format!("{} frames", root.frames)
            };
            Reply::Printed(
                vec![format!(
                    "Ready: the project is valid, {frames}, {}x{}, {} fps",
                    root.width, root.height, root.fps
                )],
                EXIT_OK,
                Outcome::Ready,
            )
        }
        Err(error) => not_opened(error),
    }
}

/// `framesmith render <path>`: opens the project as `check` does, makes the scene this version
/// draws, draws every frame on the processor and writes the film to the project's output path. A
/// project that does not open, or that uses what this version does not draw, is told as `check` tells
/// it and nothing is written (`docs/spec/render-command.md`).
fn render(path: PathBuf, options: &Options) -> Reply {
    let mut project = match Project::open(&path) {
        Ok(project) => project,
        Err(error) => return not_opened(error),
    };
    project.apply(options);
    tracing::debug!(options = options.count(), "settings chosen");
    let scene = match Scene::from_project(&project) {
        Ok(scene) => scene,
        Err(reports) => return reports_reply(&reports),
    };
    let written_output = project.output();
    let output = project.resolve(&written_output);
    let settings = project.settings();
    let mut renderer = CpuRenderer;
    match render::write_film(&scene, &mut renderer, &output, settings.audio_bitrate()) {
        Ok(written) => {
            let mut lines = vec![
                format!("Rendered {}", output.display()),
                format!("frames: {}", written.frames),
                format!("size: {}x{}", scene.width(), scene.height()),
                format!("frame rate: {} fps", scene.fps()),
                format!("length: {:.3} s", written.frames as f64 / f64::from(scene.fps())),
                format!("renderer: {}", renderer.name()),
            ];
            lines.extend(settings_used(settings, &written_output));
            lines.push(format!("elapsed: {:.3} s", written.seconds));
            Reply::Printed(lines, EXIT_OK, Outcome::Rendered { frames: written.frames })
        }
        Err(failure) => {
            let code = Code::VideoNotMade;
            Reply::Printed(
                vec![
                    code.statement(&format!("the video could not be made: {failure}")),
                    "Failed: the video could not be made".to_string(),
                ],
                EXIT_FAILED,
                Outcome::VideoNotMade { code },
            )
        }
    }
}

/// The lines of the summary that name each setting used, with where its value came from
/// (`docs/spec/render-command.md` section 7).
fn settings_used(settings: &Settings, output: &str) -> Vec<String> {
    let line = |label: &str, value: String, key: &str| format!("{label}: {value} ({})", settings.source(key));
    vec![
        line("output", output.to_string(), "render.output"),
        line("backend", settings.backend().to_string(), "render.backend"),
        line("quality", settings.quality().to_string(), "render.quality"),
        line("encoder", settings.encoder().to_string(), "render.encoder"),
        line("codec", settings.codec().to_string(), "render.codec"),
        line("workers", settings.workers().to_string(), "render.workers"),
        line(
            "audio bitrate",
            format!("{} kbit/s", settings.audio_bitrate()),
            "render.audio-bitrate",
        ),
    ]
}

/// What a command prints when the project did not open: the same for `check` and `render`.
fn not_opened(error: OpenError) -> Reply {
    match error {
        OpenError::NoProject(no_project) => {
            let code = no_project.code();
            Reply::Printed(
                vec![OpenError::NoProject(no_project).to_string()],
                EXIT_FAILED,
                Outcome::NoProject { code, problems: 0 },
            )
        }
        OpenError::InvalidAndNoProject(reports, no_project) => {
            log_reports(&reports);
            let code = no_project.code();
            let problems = reports.len();
            let mut lines: Vec<String> = reports.iter().map(ToString::to_string).collect();
            lines.push(OpenError::NoProject(no_project).to_string());
            Reply::Printed(lines, EXIT_FAILED, Outcome::NoProject { code, problems })
        }
        OpenError::Invalid(reports) => reports_reply(&reports),
    }
}

/// The reports of a project that has some, and the closing line: `Unsupported` and the exit code 2
/// when every report is of the kind unsupported, `Invalid` and 1 otherwise.
fn reports_reply(reports: &[Report]) -> Reply {
    log_reports(reports);
    let files = files_of(reports);
    let mut lines: Vec<String> = reports.iter().map(ToString::to_string).collect();
    // Only what is not supported yet is wrong: the rest of the project is valid.
    if reports.iter().all(|report| report.problem().kind() == Kind::Unsupported) {
        lines.push(format!(
            "Unsupported: the project uses what this version does not support ({files})"
        ));
        Reply::Printed(lines, EXIT_UNSUPPORTED, Outcome::Unsupported { problems: reports.len() })
    } else {
        lines.push(format!("Invalid: the project is not valid ({files})"));
        Reply::Printed(lines, EXIT_FAILED, Outcome::Invalid { problems: reports.len() })
    }
}

/// Logs each report of a run at the most detailed level: where it is and its code, never its words.
fn log_reports(reports: &[Report]) {
    for report in reports {
        tracing::trace!(
            file = report.file(),
            code = report.problem().code().as_str(),
            line = report.problem().line(),
            "report"
        );
    }
}

/// The files that have a report, each once, in the order the reports are printed.
fn files_of(reports: &[Report]) -> String {
    let mut files: Vec<&str> = Vec::new();
    for report in reports {
        if !files.contains(&report.file()) {
            files.push(report.file());
        }
    }
    files.join(", ")
}
