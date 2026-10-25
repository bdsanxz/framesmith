//! What the tests of the frx7 pin share: a small git repository made in a folder of its own, laid
//! out like Framesmith's (a root manifest that pins frx7, a lock file, the record of the pin), and
//! a way to run `scripts/gates/frx7.sh` in it.
#![allow(dead_code)]

pub mod scratch;

use std::path::{Path, PathBuf};
use std::process::Command;

pub const REMOTE: &str = "https://github.com/bdsanxz/frx7";
pub const PIN_A: &str = "1111111111111111111111111111111111111111";
pub const PIN_B: &str = "2222222222222222222222222222222222222222";

/// What a hosted runner gives every step: the marker of the runner and the files a step may write to
/// (its job summary, its outputs, the environment and the search path of later steps).
pub const RUNNER_VARIABLES: [&str; 5] = [
    "GITHUB_ACTIONS",
    "GITHUB_STEP_SUMMARY",
    "GITHUB_OUTPUT",
    "GITHUB_ENV",
    "GITHUB_PATH",
];

/// Takes the runner's variables out of a child's environment, so that a script a test starts, one that
/// is made to fail on purpose, reports to no runner and writes no file of one (`docs/dev/gates.md`,
/// section 12). Every helper that starts a script or a program of the repository calls it.
pub fn isolate_from_runner(command: &mut Command) -> &mut Command {
    for name in RUNNER_VARIABLES {
        command.env_remove(name);
    }
    command
}

/// The script under test, in this repository.
pub fn script_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates/frx7.sh")
}

/// The root manifest of a repository that pins frx7 at `rev`.
pub fn manifest(rev: &str) -> String {
    format!(
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/a\"]\n\n[workspace.dependencies]\na = {{ path = \"crates/a\" }}\n\
         base = {{ git = \"{REMOTE}\", rev = \"{rev}\" }}\nmp4 = {{ git = \"{REMOTE}\", rev = \"{rev}\" }}\n"
    )
}

/// A lock file whose frx7 packages are taken from the pin `rev`.
pub fn lock(rev: &str) -> String {
    let source = format!("git+{REMOTE}?rev={rev}#{rev}");
    format!(
        "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"0.1.0\"\ndependencies = [\n \"base\",\n]\n\n\
         [[package]]\nname = \"base\"\nversion = \"0.1.0\"\nsource = \"{source}\"\n\n\
         [[package]]\nname = \"mp4\"\nversion = \"0.1.0\"\nsource = \"{source}\"\ndependencies = [\n \"base\",\n]\n"
    )
}

/// The record of the pin: a document whose history holds `rows` (old, new, reason).
pub fn record(rows: &[(&str, &str, &str)]) -> String {
    let mut text = String::from("# How Framesmith depends on frx7\n\n## 5. Moving the pin\n\n### The pin's history\n\n| Date | Old revision | New revision | Reason |\n|---|---|---|---|\n");
    for (old, new, reason) in rows {
        text.push_str(&format!("| 2026-10-14 | `{old}` | `{new}` | {reason} |\n"));
    }
    text.push_str("\n## 6. The gate\n\nText after the history.\n");
    text
}

/// A git repository in a folder of its own.
pub struct Repo {
    pub dir: PathBuf,
}

/// What a command printed and how it ended.
pub struct Run {
    pub code: i32,
    pub text: String,
}

impl Repo {
    /// An empty repository, made new in a folder named `name` under the tests' scratch folder.
    pub fn empty(name: &str) -> Repo {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("frx7-gates").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        // The scripts name folders by their full, resolved path; cargo gives this one as it was given
        // the target folder, which may hold `..` or a link.
        let dir = std::fs::canonicalize(&dir).expect("the folder has a full path");
        let repo = Repo { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.write(
            "scripts/gates/frx7.sh",
            &std::fs::read_to_string(script_source()).expect("the script is read"),
        );
        repo
    }

    /// A repository whose first commit pins frx7 at `PIN_A`, with its lock file and its record.
    pub fn pinned(name: &str) -> Repo {
        let repo = Repo::empty(name);
        repo.write("Cargo.toml", &manifest(PIN_A));
        repo.write("Cargo.lock", &lock(PIN_A));
        repo.write(
            "crates/a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n[dependencies]\nbase = { workspace = true }\n",
        );
        repo.write("docs/dev/frx7.md", &record(&[("none", PIN_A, "First pin.")]));
        repo.commit("Pin frx7");
        repo
    }

    pub fn write(&self, relative: &str, content: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the folder is made");
        std::fs::write(&path, content).expect("the file is written");
    }

    pub fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.dir.join(relative)).expect("the file is read")
    }

    pub fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        isolate_from_runner(&mut command);
        command
            .current_dir(&self.dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid");
        command
    }

    /// Runs git in the repository and gives what it printed; a failure stops the test.
    pub fn git(&self, args: &[&str]) -> String {
        let output = self.command("git").args(args).output().expect("git is run");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Commits every change, new files included.
    pub fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// Runs `scripts/gates/frx7.sh` of the repository with `args`.
    pub fn script(&self, args: &[&str]) -> Run {
        self.script_with(args, &[])
    }

    pub fn script_with(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = self.command("bash");
        command.arg("scripts/gates/frx7.sh").args(args);
        for (name, value) in env {
            command.env(name, value);
        }
        let output = command.output().expect("the script is run");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Run {
            code: output.status.code().unwrap_or(-1),
            text,
        }
    }
}
