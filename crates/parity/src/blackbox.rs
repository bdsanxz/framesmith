//! Running the reference as a black box, only through its command line.
//!
//! It is installed outside the repository, in the folder `tools/blackbox/` of the workspace root
//! (decision A3). The folder is a setting, not a place written into the code: `--blackbox DIR`
//! names it, and when that is not given [`default_folder`] takes the nearest `tools/blackbox`
//! folder at or above the folder the command is run from. When the reference is not installed
//! there, nothing is run and the outcome is "not run": never "passed".

use crate::report::{EXIT_FAIL, EXIT_NOT_RUN, EXIT_PASS};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The command the reference installs.
const COMMAND: &str = "hyperframes";
/// The quality preset used when nothing else is asked.
pub const QUALITY: &str = "standard";
/// The frame rate used when nothing else is asked.
pub const FPS: u32 = 30;

/// One run of the reference.
#[derive(Clone, Debug)]
pub struct Job {
    /// The folder the reference is installed in (by default the workspace's `tools/blackbox`).
    pub blackbox: PathBuf,
    /// The folder with the counterpart composition.
    pub counterpart: PathBuf,
    pub output: PathBuf,
    /// `None` leaves the frame rate out, so the reference fills in its own.
    pub fps: Option<u32>,
    pub quality: String,
    /// A browser the reference should use instead of fetching its own, if the maintainer names one.
    pub browser: Option<PathBuf>,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The reference is not installed, so nothing was run.
    NotRun(String),
    /// The reference ran and its video exists.
    Rendered { version: String },
    /// The reference ran and failed.
    Failed(String),
}

impl Outcome {
    pub fn exit_code(&self) -> i32 {
        match self {
            Outcome::NotRun(_) => EXIT_NOT_RUN,
            Outcome::Rendered { .. } => EXIT_PASS,
            Outcome::Failed(_) => EXIT_FAIL,
        }
    }

    /// The words printed for the maintainer. Never says "passed".
    pub fn text(&self) -> String {
        match self {
            Outcome::NotRun(why) => format!("reference: {why}\nresult: not run\n"),
            Outcome::Rendered { version } => format!("reference version {version}\nresult: rendered\n"),
            Outcome::Failed(why) => format!("reference: {why}\nresult: failed\n"),
        }
    }
}

/// The folder the reference is installed in when none is named: the nearest `tools/blackbox`
/// folder at or above `from`. When there is none, the relative path `tools/blackbox` is returned,
/// which is not installed, so the run says "not run".
pub fn default_folder(from: &Path) -> PathBuf {
    from.ancestors()
        .map(|folder| folder.join("tools").join("blackbox"))
        .find(|candidate| candidate.is_dir())
        .unwrap_or_else(|| PathBuf::from("tools/blackbox"))
}

/// The reference's command inside the folder, when it is installed.
pub fn installed(blackbox: &Path) -> Option<PathBuf> {
    let command = blackbox.join("node_modules").join(".bin").join(COMMAND);
    command.is_file().then_some(command)
}

/// The environment the reference runs in: usage data, update checks, automatic installs and
/// skill installs are off, and everything it writes stays inside the folder.
pub fn environment(blackbox: &Path, browser: Option<&Path>) -> Vec<(String, OsString)> {
    let state = blackbox.join("state");
    let mut env: Vec<(String, OsString)> = vec![
        ("HOME".into(), state.join("home").into()),
        ("HYPERFRAMES_NO_TELEMETRY".into(), "1".into()),
        ("DO_NOT_TRACK".into(), "1".into()),
        ("HYPERFRAMES_NO_UPDATE_CHECK".into(), "1".into()),
        ("HYPERFRAMES_NO_AUTO_INSTALL".into(), "1".into()),
        ("HYPERFRAMES_SKIP_SKILLS".into(), "1".into()),
        ("HYPERFRAMES_EXTRACT_CACHE_DIR".into(), state.join("frames-cache").into()),
    ];
    if let Some(browser) = browser {
        env.push(("HYPERFRAMES_BROWSER_PATH".into(), browser.into()));
    }
    env
}

/// The arguments of the render: quality and a single worker are always explicit, and so is the
/// frame rate unless the maintainer asks for the reference's own.
pub fn arguments(job: &Job) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "render".into(),
        job.counterpart.clone().into(),
        "--output".into(),
        job.output.clone().into(),
    ];
    if let Some(fps) = job.fps {
        args.extend(["--fps".into(), fps.to_string().into()]);
    }
    args.extend(["--quality".into(), job.quality.clone().into(), "--workers".into(), "1".into()]);
    args
}

fn command(program: &Path, job: &Job) -> Command {
    let mut command = Command::new(program);
    command.current_dir(&job.blackbox).stdin(Stdio::null());
    for (name, value) in environment(&job.blackbox, job.browser.as_deref()) {
        command.env(name, value);
    }
    command
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Runs the reference on the counterpart, or says that it cannot.
pub fn run(job: &Job) -> Outcome {
    let blackbox = absolute(&job.blackbox);
    let Some(program) = installed(&blackbox) else {
        return Outcome::NotRun(format!("not installed under {} (nothing was run)", job.blackbox.display()));
    };
    let job = Job {
        blackbox: blackbox.clone(),
        counterpart: absolute(&job.counterpart),
        output: absolute(&job.output),
        ..job.clone()
    };
    if !job.counterpart.join("index.html").is_file() {
        return Outcome::Failed(format!("{} has no index.html", job.counterpart.display()));
    }
    if let Some(parent) = job.output.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Outcome::Failed(format!("{} could not be made ({e})", parent.display()));
        }
    }
    let _ = std::fs::remove_file(&job.output);
    let version = match command(&program, &job).arg("--version").output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Ok(out) => return Outcome::Failed(format!("its version could not be read (exit {:?})", out.status.code())),
        Err(e) => return Outcome::Failed(format!("it could not be started ({e})")),
    };
    match command(&program, &job).args(arguments(&job)).output() {
        Ok(out) if out.status.success() && job.output.is_file() => Outcome::Rendered { version },
        Ok(out) => Outcome::Failed(format!(
            "it exited with {:?} and {}: {}",
            out.status.code(),
            if job.output.is_file() { "left a video" } else { "made no video" },
            String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("").trim()
        )),
        Err(e) => Outcome::Failed(format!("it could not be started ({e})")),
    }
}
