//! Running a program and keeping what it printed.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What a finished program left behind.
#[derive(Clone, Debug)]
pub struct Ran {
    /// The exit code; 128 plus the signal number when a signal ended it.
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Ran {
    /// Standard output followed by standard error.
    pub fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Runs `program` in `dir` with the given arguments and extra environment.
pub fn run<S: AsRef<OsStr>>(program: &Path, args: &[S], dir: &Path, env: &[(&str, &OsStr)]) -> Ran {
    use std::os::unix::process::ExitStatusExt;
    let mut command = Command::new(program);
    command.args(args).current_dir(dir).stdin(Stdio::null());
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("{} could not be run: {e}", program.display()));
    Ran {
        code: output
            .status
            .code()
            .or_else(|| output.status.signal().map(|s| 128 + s))
            .unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The first program of that name on the search path.
pub fn tool(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|folder| folder.join(name))
        .find(|candidate| candidate.is_file())
}
