//! The `framesmith` command. What each command prints and returns is
//! written down in `docs/spec/commands.md` and `docs/spec/render-stages.md`.

pub mod args;
mod check;
mod facts;
mod info;
mod init;
pub mod log;
mod preview;
pub mod progress;
mod render;
mod signal;
pub mod text;
mod timeline;

use args::Command;
use std::ffi::OsString;

const HELP_HEAD: &str = "\
framesmith renders an HTML composition to a video on this machine.

Usage:
  framesmith init <folder>      Create a starter project in a new folder.
  framesmith check <project>    Read a project and report what a render would trip over.
  framesmith info <project> [--json]
                                Print what Framesmith understands a project to be.
  framesmith timeline <project> [--json]
                                Print the clips of a project with their start, end and duration.
  framesmith render <project> [--output <path>] [--fps <n>] [--json]
                                Render the project to an MP4 file.
";

/// The lines about `preview` come from `preview.rs`, which reads its options, so that no later preview
/// option changes this file. They stand between `HELP_HEAD` and `HELP_TAIL`.
const HELP_TAIL: &str = "
Options for render:
  --output <path>   Where the video is written. Default: <project>/out/<folder name>.mp4
  --fps <n>         The frame rate of this run, a whole number from 1 to 240.
  --json            Print the result as one JSON document on standard output.

Option for info and timeline:
  --json            Print the facts (or the diagnostics) as one JSON document on standard output.

  framesmith --help             Show this text.
  framesmith --version          Show the version.
";

/// Runs one command and returns its exit code. `args` are the words after
/// the program name.
pub fn run(args: &[OsString]) -> i32 {
    let command = args::parse(args);
    if let Ok(Command::Help) = command {
        print!("{HELP_HEAD}{}{HELP_TAIL}", preview::HELP);
        return 0;
    }
    if let Ok(Command::Version) = command {
        println!("framesmith {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let run_id = log::new_run_id();
    log::install(&run_id);
    match command {
        Ok(Command::Init { folder }) => init::run(&folder),
        Ok(Command::Check { project }) => check::run(&project),
        Ok(Command::Info { project, json }) => info::run(&project, json, &run_id),
        Ok(Command::Timeline { project, json }) => timeline::run(&project, json, &run_id),
        Ok(Command::Render(options)) => render::run(&options, &run_id),
        Ok(Command::Preview { words }) => preview::run(&words, &run_id),
        Ok(Command::Help | Command::Version) => 0,
        Err(usage) => render::usage_error(&usage, &run_id),
    }
}
