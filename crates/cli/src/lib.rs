//! The `framesmith` command.
//!
//! Behaviour is specified in `docs/spec/check-command.md`. This version has one command, `check`.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use project::{Code, Kind, OpenError, Project, COMPOSITION_FILE};

pub mod runlog;

/// The exit code of a command that found nothing wrong.
pub const EXIT_OK: u8 = 0;
/// The exit code of a command line that cannot be read, an invalid project, or no project.
pub const EXIT_FAILED: u8 = 1;

const USAGE: &str = "usage: framesmith check <folder>";

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
        None => Reply::Unreadable("no command is given".to_string()),
        Some((command, rest)) if command == "check" => {
            if let Some(option) = rest.iter().find(|word| word.to_string_lossy().starts_with('-')) {
                Reply::Unreadable(format!("`check` takes no option, and {} is one", option.to_string_lossy()))
            } else {
                match rest {
                    [] => Reply::Unreadable("`check` needs the folder of a project".to_string()),
                    [folder] => check(PathBuf::from(folder)),
                    [_, extra, ..] => Reply::Unreadable(format!(
                        "`check` takes the folder of one project, and {} is one word too many",
                        extra.to_string_lossy()
                    )),
                }
            }
        }
        Some((command, _)) => Reply::Unreadable(format!("`{}` is not a command", command.to_string_lossy())),
    };
    let written = match &reply {
        Reply::Printed(lines, ..) => lines.iter().try_for_each(|line| writeln!(out, "{line}")),
        Reply::Unreadable(why) => {
            writeln!(err, "framesmith: {}", Code::CommandLineNotRead.statement(why)).and_then(|()| writeln!(err, "{USAGE}"))
        }
    };
    let (code, outcome) = match (written, reply) {
        (Ok(()), Reply::Printed(_, code, outcome)) => (code, outcome),
        (Err(_), Reply::Printed(_, _, outcome)) => (EXIT_FAILED, outcome),
        (_, Reply::Unreadable(_)) => (EXIT_FAILED, Outcome::NotRead),
    };
    outcome.log(code);
    code
}

/// How a command ended, for the log (`docs/spec/run-log.md` section 4).
enum Outcome {
    Ready,
    Invalid { problems: usize },
    NoProject(Code),
    NotRead,
}

impl Outcome {
    /// Writes the last line of the run: the level follows the outcome.
    fn log(&self, exit: u8) {
        match self {
            Outcome::Ready => tracing::info!(outcome = "ready", exit, "command finished"),
            Outcome::Invalid { problems } => {
                tracing::warn!(outcome = "invalid", exit, problems = *problems, "command finished")
            }
            Outcome::NoProject(code) if code.kind() == Kind::Failure => {
                tracing::error!(outcome = "no project", exit, code = code.as_str(), "command finished")
            }
            Outcome::NoProject(code) => {
                tracing::warn!(outcome = "no project", exit, code = code.as_str(), "command finished")
            }
            Outcome::NotRead => tracing::warn!(outcome = "command line not read", exit, "command finished"),
        }
    }
}

/// What a command has to say, and how it ends.
enum Reply {
    /// Lines for standard output, the exit code, and how it ended.
    Printed(Vec<String>, u8, Outcome),
    /// A command line that cannot be read, with what is wrong with it.
    Unreadable(String),
}

/// `framesmith check <folder>`: opens the project and says whether its root is valid, or every
/// problem it has.
fn check(folder: PathBuf) -> Reply {
    match Project::open(&folder) {
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
        Err(OpenError::NoProject(no_project)) => {
            let code = no_project.code();
            Reply::Printed(
                vec![OpenError::NoProject(no_project).to_string()],
                EXIT_FAILED,
                Outcome::NoProject(code),
            )
        }
        Err(OpenError::Invalid(reports)) => {
            for report in &reports {
                tracing::trace!(
                    file = report.file(),
                    code = report.problem().code().as_str(),
                    line = report.problem().line(),
                    "report"
                );
            }
            let mut lines: Vec<String> = reports.iter().map(ToString::to_string).collect();
            lines.push(format!("Invalid: the project is not valid ({COMPOSITION_FILE})"));
            Reply::Printed(lines, EXIT_FAILED, Outcome::Invalid { problems: reports.len() })
        }
    }
}
