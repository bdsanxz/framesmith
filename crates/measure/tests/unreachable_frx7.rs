//! A pin Cargo cannot fetch: the build fails with a message that names the revision and the
//! remote, and the gates add one line that says the same. See `docs/dev/frx7.md`, section 7.
//!
//! The remote is made unreachable on this machine by telling git to read it from a place that does
//! not exist, so the test needs no network and fails the same way with the network up or down.

mod common;

use common::REMOTE;
use std::path::{Path, PathBuf};
use std::process::Command;

const MISSING: &str = "0123456789abcdef0123456789abcdef01234567";

/// A workspace of its own whose one crate takes `base` of frx7 at `MISSING`, with Cargo's home and
/// target folders inside it so that nothing outside is read or written.
fn workspace(name: &str) -> PathBuf {
    // The process number is in the name because one test runs this test program again (below), and
    // the two must not share a folder.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("unreachable-frx7")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("crates/user/src")).expect("the folders are made");
    std::fs::create_dir_all(dir.join("home")).expect("the folder is made");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/user\"]\n\n[workspace.dependencies]\nbase = {{ git = \"{REMOTE}\", rev = \"{MISSING}\" }}\n"
        ),
    )
    .expect("the manifest is written");
    std::fs::write(
        dir.join("crates/user/Cargo.toml"),
        "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nbase = { workspace = true }\n",
    )
    .expect("the manifest is written");
    std::fs::write(dir.join("crates/user/src/lib.rs"), "").expect("the source is written");
    dir
}

/// The command `program` in `dir`, with the remote read from a place that does not exist. The search
/// path of the child is the folder of the toolchain that runs these tests (the folder of the cargo in
/// `CARGO`) before the search path of the test: the test starts cargo, which needs `rustc`, and the
/// core suite runs with only the system's folders on its own search path (`docs/dev/gates.md`,
/// section 10). With `isolate`, the runner's variables are taken out of the child's environment
/// (section 12).
fn cut_off_command(dir: &Path, program: &str, args: &[&str], isolate: bool) -> Command {
    let nowhere = dir.join("no-such-remote");
    let mut command = Command::new(program);
    if isolate {
        common::isolate_from_runner(&mut command);
    }
    command
        .args(args)
        .current_dir(dir)
        .env("PATH", search_path_with_toolchain())
        .env("CARGO_HOME", dir.join("home"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
        .env("CARGO_NET_RETRY", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", format!("url.file://{}.insteadOf", nowhere.display()))
        .env("GIT_CONFIG_VALUE_0", REMOTE);
    command
}

fn search_path_with_toolchain() -> std::ffi::OsString {
    let mut folders = Vec::new();
    if let Some(cargo) = std::env::var_os("CARGO") {
        if let Some(folder) = Path::new(&cargo).parent() {
            folders.push(folder.to_path_buf());
        }
    }
    folders.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    std::env::join_paths(folders).expect("the search path is joined")
}

fn finish(mut command: Command) -> (i32, String) {
    let output = command.output().expect("the program is run");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), text)
}

/// Runs `program` in `dir` with the remote read from a place that does not exist.
fn run_cut_off(dir: &Path, program: &str, args: &[&str]) -> (i32, String) {
    finish(cut_off_command(dir, program, args, true))
}

#[test]
fn a_build_that_cannot_fetch_the_pin_fails_naming_the_revision_and_the_remote() {
    let dir = workspace("build");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let (code, text) = run_cut_off(&dir, &cargo, &["build"]);
    assert_ne!(code, 0, "{text}");
    assert!(text.contains(&format!("unable to update {REMOTE}?rev={MISSING}")), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_gate_that_cannot_fetch_the_pin_says_which_revision_and_which_remote() {
    let dir = workspace("gate");
    std::fs::create_dir_all(dir.join("scripts")).expect("the folder is made");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates.sh"),
        dir.join("scripts/gates.sh"),
    )
    .expect("the script is copied");
    let (code, text) = run_cut_off(&dir, "bash", &["scripts/gates.sh", "--gate", "lint"]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("FAIL static analysis")
            && text.contains(&format!(
                "frx7 revision {MISSING} could not be fetched from {REMOTE}; see docs/dev/frx7.md, section 7"
            )),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

const GATE_TEST: &str = "a_gate_that_cannot_fetch_the_pin_says_which_revision_and_which_remote";

/// The files a hosted runner lets a step write to, each with a line in it.
fn runner_files(dir: &Path) -> Vec<(&'static str, PathBuf)> {
    common::RUNNER_VARIABLES
        .iter()
        .filter(|name| **name != "GITHUB_ACTIONS")
        .map(|name| {
            let file = dir.join(format!("{}.txt", name.to_lowercase()));
            std::fs::write(&file, "as the runner left it\n").expect("the file is written");
            (*name, file)
        })
        .collect()
}

#[test]
fn a_gate_started_by_a_test_leaves_the_runners_summary_as_it_was() {
    let dir = workspace("runner-files");
    std::fs::create_dir_all(dir.join("scripts")).expect("the folder is made");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates.sh"),
        dir.join("scripts/gates.sh"),
    )
    .expect("the script is copied");
    let files = runner_files(&dir);
    let as_set_by_a_runner = |command: &mut Command| {
        command.env("GITHUB_ACTIONS", "true");
        for (name, file) in &files {
            command.env(name, file);
        }
    };
    let summary = files
        .iter()
        .find(|(name, _)| *name == "GITHUB_STEP_SUMMARY")
        .expect("a summary")
        .1
        .clone();

    // The control: a gate started with the runner's variables in its environment does write its
    // failure to the summary. Without this the check below could pass for a gate that never would.
    let mut unisolated = cut_off_command(&dir, "bash", &["scripts/gates.sh", "--gate", "lint"], false);
    as_set_by_a_runner(&mut unisolated);
    let (code, text) = finish(unisolated);
    assert_eq!(code, 1, "{text}");
    let written = std::fs::read_to_string(&summary).expect("the summary is read");
    assert!(
        written.contains("FAIL static analysis"),
        "the control wrote nothing to the summary: {written}"
    );
    for (_, file) in &files {
        std::fs::write(file, "as the runner left it\n").expect("the file is written");
    }

    // The test as it is started on a runner: the whole test is run again with the runner's variables
    // set, and the gate it starts must leave every one of the runner's files as it was.
    let mut again = Command::new(std::env::current_exe().expect("this test program"));
    again.args([GATE_TEST, "--exact", "--test-threads", "1"]);
    as_set_by_a_runner(&mut again);
    let (code, text) = finish(again);
    assert_eq!(code, 0, "the gate test failed when run with the runner's variables set: {text}");
    assert!(text.contains(&format!("test {GATE_TEST} ... ok")), "{text}");
    for (name, file) in &files {
        let after = std::fs::read_to_string(file).expect("the file is read");
        assert_eq!(after, "as the runner left it\n", "the gate wrote to {name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
