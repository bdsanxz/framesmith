//! Reading the command line.

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Init { folder: PathBuf },
    Check { project: PathBuf },
    Info { project: PathBuf, json: bool },
    Timeline { project: PathBuf, json: bool },
    Render(RenderOptions),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderOptions {
    pub project: PathBuf,
    pub output: Option<PathBuf>,
    /// As written; `render` decides whether it is acceptable.
    pub fps: Option<String>,
    pub json: bool,
}

/// A command line that cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub message: String,
    /// `render --json` was asked for, so the answer is a JSON document.
    pub json: bool,
}

fn usage(message: impl Into<String>) -> Usage {
    Usage {
        message: message.into(),
        json: false,
    }
}

/// The one path a command takes, and nothing else.
fn one_path(command: &str, what: &str, rest: &[OsString]) -> Result<PathBuf, Usage> {
    match rest {
        [] => Err(usage(format!("`framesmith {command}` needs {what}."))),
        [path] if !path.to_string_lossy().starts_with("--") => Ok(PathBuf::from(path)),
        [option] => Err(usage(format!("`framesmith {command}` has no option {}.", option.to_string_lossy()))),
        [_, extra, ..] => Err(usage(format!(
            "`framesmith {command}` takes only {what}; {} is one word too many.",
            extra.to_string_lossy()
        ))),
    }
}

fn render(rest: &[OsString]) -> Result<Command, Usage> {
    let mut options = RenderOptions::default();
    let mut project: Option<PathBuf> = None;
    let mut problem: Option<String> = None;
    let mut words = rest.iter();
    while let Some(word) = words.next() {
        let text = word.to_string_lossy();
        let (name, attached) = match text.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name.to_string(), Some(value.to_string())),
            _ => (text.to_string(), None),
        };
        match name.as_str() {
            "--json" if attached.is_none() => options.json = true,
            // The next word is the value whatever it looks like, so `--fps -5` reads as a value.
            "--fps" | "--output" => {
                let value = attached.map(OsString::from).or_else(|| words.next().cloned());
                match value {
                    None => problem = problem.or(Some(format!("{name} needs a value."))),
                    Some(value) if name == "--fps" => options.fps = Some(value.to_string_lossy().into_owned()),
                    Some(value) => options.output = Some(PathBuf::from(value)),
                }
            }
            _ if text.starts_with("--") => {
                problem = problem.or(Some(format!("`framesmith render` has no option {text}.")));
            }
            _ if project.is_none() => project = Some(PathBuf::from(word)),
            _ => {
                problem = problem.or(Some(format!(
                    "`framesmith render` takes one project folder; {text} is one word too many."
                )));
            }
        }
    }
    if problem.is_none() && project.is_none() {
        problem = Some("`framesmith render` needs the path of a project folder.".to_string());
    }
    match (problem, project) {
        (None, Some(project)) => Ok(Command::Render(RenderOptions { project, ..options })),
        (problem, _) => Err(Usage {
            message: problem.unwrap_or_default(),
            json: options.json,
        }),
    }
}

/// `info`: one path, and `--json` before or after it. Only the exact word `--json` is the option; every
/// other word is read as `check` reads it, so an unknown option is refused in the same words.
fn info(rest: &[OsString]) -> Result<Command, Usage> {
    let is_json = |word: &OsString| word.to_string_lossy() == "--json";
    let json = rest.iter().any(is_json);
    let others: Vec<OsString> = rest.iter().filter(|word| !is_json(word)).cloned().collect();
    match one_path("info", "the path of a project folder", &others) {
        Ok(project) => Ok(Command::Info { project, json }),
        Err(usage) => Err(Usage { json, ..usage }),
    }
}

/// `timeline`: the project folder, and `--json` before or after it. `timeline` only prints, so a word
/// after the folder that is not an option (an edit word such as `move`) is refused in words that say so;
/// any other option is refused as `info` refuses one it does not have. With `--json` present the refusal
/// is answered as a document.
fn timeline(rest: &[OsString]) -> Result<Command, Usage> {
    let is_json = |word: &OsString| word.to_string_lossy() == "--json";
    let json = rest.iter().any(is_json);
    let others: Vec<OsString> = rest.iter().filter(|word| !is_json(word)).cloned().collect();
    if let [_, word, ..] = others.as_slice() {
        let word = word.to_string_lossy();
        if !word.starts_with("--") {
            return Err(Usage {
                json,
                ..usage(format!(
                    "`framesmith timeline` only prints the timeline and has no edit commands; {word} is not one of its words."
                ))
            });
        }
    }
    match one_path("timeline", "the path of a project folder", &others) {
        Ok(project) => Ok(Command::Timeline { project, json }),
        Err(usage) => Err(Usage { json, ..usage }),
    }
}

/// Reads the words after the program name.
pub fn parse(args: &[OsString]) -> Result<Command, Usage> {
    let Some((first, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    match first.to_string_lossy().as_ref() {
        "--help" | "-h" | "help" => Ok(Command::Help),
        "--version" | "-V" => Ok(Command::Version),
        "init" => Ok(Command::Init {
            folder: one_path("init", "the name of a folder to create", rest)?,
        }),
        "check" => Ok(Command::Check {
            project: one_path("check", "the path of a project folder", rest)?,
        }),
        "info" => info(rest),
        "timeline" => timeline(rest),
        "render" => render(rest),
        other => Err(usage(format!("framesmith has no command {other}."))),
    }
}
