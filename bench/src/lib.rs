//! Measurements of Framesmith, made by running the `framesmith` command.
//! Development only; the product does not depend on this crate.

pub mod seek;

use std::path::PathBuf;

pub const EXIT_PASS: i32 = 0;
pub const EXIT_FAIL: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_NOT_RUN: i32 = 3;

const USAGE: &str = "\
bench: measurements of Framesmith (development only)

  bench seek <project> [--framesmith PATH] [--work DIR] [--json]
      the real time the page takes to put itself at each frame, measured inside the page
      while the project is rendered; passes when the largest time is under 2 milliseconds

Exit codes: 0 pass, 1 fail, 2 the tool was called wrongly, 3 the measurement could not be run.
";

/// The `framesmith` program beside this one, where a build puts both.
fn beside(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Runs the tool and returns what it prints and its exit code.
pub fn run(args: &[String]) -> (String, i32) {
    let usage = |why: &str| (format!("bench: {why}\n\n{USAGE}"), EXIT_USAGE);
    let Some((command, rest)) = args.split_first() else {
        return (USAGE.to_string(), EXIT_USAGE);
    };
    match command.as_str() {
        "help" | "--help" | "-h" => (USAGE.to_string(), EXIT_PASS),
        "seek" => {
            let (mut project, mut framesmith, mut work, mut json) = (None, None, None, false);
            let mut rest = rest.iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--json" => json = true,
                    "--framesmith" | "--work" => {
                        let Some(value) = rest.next() else {
                            return usage(&format!("{arg} needs a value"));
                        };
                        if arg == "--work" {
                            work = Some(PathBuf::from(value));
                        } else {
                            framesmith = Some(PathBuf::from(value));
                        }
                    }
                    other if other.starts_with("--") => return usage(&format!("{other} is not an option of this command")),
                    _ if project.is_some() => return usage("give one project folder"),
                    other => project = Some(PathBuf::from(other)),
                }
            }
            let Some(project) = project else {
                return usage("give the project folder to measure");
            };
            let framesmith = framesmith.unwrap_or_else(|| beside("framesmith"));
            let work = work.unwrap_or_else(|| beside("bench-work"));
            match seek::measure(&framesmith, &project, &work) {
                Ok(costs) => {
                    let text = if json { seek::json(&costs) } else { seek::report(&costs) };
                    let pass = seek::summarise(&costs).is_some_and(|summary| summary.pass);
                    (text, if pass { EXIT_PASS } else { EXIT_FAIL })
                }
                Err(why) => (format!("seek measurement: not run\n  {why}\nresult: not run\n"), EXIT_NOT_RUN),
            }
        }
        other => usage(&format!("'{other}' is not a command of this tool")),
    }
}
