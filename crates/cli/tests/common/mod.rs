//! What the tests of the run log share: scratch folders, running `framesmith`, reading the log.

#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const VALID: &str = r#"<composition version="1" id="demo" width="1280" height="720" duration="5"/>"#;
/// Two problems: a size that is odd and a duration that is not a time.
pub const INVALID: &str = r#"<composition version="1" width="641" height="720" duration="soon"/>"#;

/// A new, empty folder `name` for one test.
pub fn scratch(group: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(group).join(name);
    if dir.exists() {
        make_writable(&dir);
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Gives every folder under `dir` back its owner's permissions, so that it can be removed.
fn make_writable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                make_writable(&entry.path());
            }
        }
    }
}

/// A folder `demo` holding a `composition.xml` with `text`, and an empty working folder `cwd`,
/// both new, side by side. Returns `(project, cwd)`.
pub fn project(group: &str, name: &str, text: &str) -> (PathBuf, PathBuf) {
    let base = scratch(group, name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    fs::write(project.join("composition.xml"), text).unwrap();
    (project, cwd)
}

/// Runs `framesmith` with `args` from `cwd`, with `FRAMESMITH_LOG` unset and `env` added.
pub fn framesmith<I, S>(cwd: &Path, args: I, env: &[(&str, &str)]) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_framesmith"));
    command.args(args).current_dir(cwd).env_remove("FRAMESMITH_LOG");
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().unwrap()
}

/// The most blocks of 512 bytes that one file may grow to under `framesmith_with_file_limit`: 32 KB. The run log of a
/// short command is smaller; the film of a few seconds is larger.
pub const FILE_LIMIT_BLOCKS: u32 = 64;

/// Runs `framesmith` with `args` from `cwd` through `/bin/sh`, with each file limited to `FILE_LIMIT_BLOCKS` and SIGXFSZ
/// ignored, so that a write past the limit fails with an ordinary error and the command goes on with its own report.
pub fn framesmith_with_file_limit<I, S>(cwd: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let script = format!("trap '' XFSZ; ulimit -f {FILE_LIMIT_BLOCKS}; exec \"$0\" \"$@\"");
    Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg(env!("CARGO_BIN_EXE_framesmith"))
        .args(args)
        .current_dir(cwd)
        .env_remove("FRAMESMITH_LOG")
        .output()
        .unwrap()
}

pub fn out(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

pub fn err(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// The log file of a run from `cwd`.
pub fn log_path(cwd: &Path) -> PathBuf {
    cwd.join("logs").join("framesmith.log")
}

/// The whole text of the log of `cwd`, or nothing when there is none.
pub fn log_text(cwd: &Path) -> String {
    fs::read_to_string(log_path(cwd)).unwrap_or_default()
}

/// One line of the log, taken apart (`docs/spec/run-log.md` section 3).
#[derive(Debug, Clone)]
pub struct Line {
    pub time: String,
    pub level: String,
    pub component: String,
    pub run_id: String,
    /// The message and the fields, as written.
    pub text: String,
    pub raw: String,
}

pub fn parse(text: &str) -> Vec<Line> {
    text.lines()
        .map(|raw| {
            let mut rest = raw;
            let mut next = || {
                let trimmed = rest.trim_start_matches(' ');
                let (word, after) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
                rest = after;
                word.to_string()
            };
            let time = next();
            let level = next();
            let component = next();
            let run = next();
            Line {
                time,
                level,
                component,
                run_id: run.strip_prefix("run=").unwrap_or("").to_string(),
                text: rest.to_string(),
                raw: raw.to_string(),
            }
        })
        .collect()
}

/// The messages and fields of every line of the log, one after the other, without the time, the
/// level, the component and the run id. A word that must be absent from the log is looked for here:
/// the time and the run id are digits that differ in every run, and a word made of digits or of the
/// letters a to f can be in them by chance.
pub fn after_run_id(text: &str) -> String {
    parse(text).iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
}

pub fn lines(cwd: &Path) -> Vec<Line> {
    parse(&log_text(cwd))
}
