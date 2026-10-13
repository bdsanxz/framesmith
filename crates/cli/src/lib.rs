//! The `framesmith` command.
//!
//! Behaviour is specified in `docs/spec/check-command.md`. This version has one command, `check`.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use project::{OpenError, Project, COMPOSITION_FILE};

/// The exit code of a command that found nothing wrong.
pub const EXIT_OK: u8 = 0;
/// The exit code of a command line that cannot be read, an invalid project, or no project.
pub const EXIT_FAILED: u8 = 1;

const USAGE: &str = "usage: framesmith check <folder>";

/// Runs the command with the arguments after its name. Results go to `out`, a command line that
/// cannot be read to `err`. Returns the exit code.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
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
        Reply::Printed(lines, _) => lines.iter().try_for_each(|line| writeln!(out, "{line}")),
        Reply::Unreadable(why) => writeln!(err, "framesmith: {why}").and_then(|()| writeln!(err, "{USAGE}")),
    };
    match (written, reply) {
        (Ok(()), Reply::Printed(_, code)) => code,
        _ => EXIT_FAILED,
    }
}

/// What a command has to say, and how it ends.
enum Reply {
    /// Lines for standard output, and the exit code.
    Printed(Vec<String>, u8),
    /// A command line that cannot be read, with what is wrong with it.
    Unreadable(String),
}

/// `framesmith check <folder>`: opens the project and says whether its root is valid.
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
            )
        }
        Err(error @ OpenError::NoProject(_)) => Reply::Printed(vec![error.to_string()], EXIT_FAILED),
        Err(error @ OpenError::Invalid(_)) => Reply::Printed(
            vec![error.to_string(), format!("Invalid: the project is not valid ({COMPOSITION_FILE})")],
            EXIT_FAILED,
        ),
    }
}
