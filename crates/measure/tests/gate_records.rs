//! The records of passed gates (`docs/dev/gates.md`, section 14). A gate that passed for a tree is taken from its
//! record on the same tree; a change to a tracked file runs it again; uncommitted changes, untracked files included,
//! run every gate and are not recorded; a changed toolchain line runs it again; a hosted run takes nothing from the
//! records; a failed gate is recorded nowhere; and a run never writes into a build folder of the caller's.
//!
//! Each case is a small git repository of its own, with a copy of `scripts/gates.sh` and of
//! `scripts/gates/records.sh`, and a fake `cargo` and `rustc` in front of the search path. The fake `cargo` writes
//! down each call, so the number of formatter runs is what the script really did.

mod common;

use common::{manifest, Repo, Run, PIN_A};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Writes its subcommand to the call log, then behaves as the gates need: `--version` prints `FAKE_VERSION`, and
/// `fmt` exits with `FAKE_FMT_EXIT`.
const FAKE_CARGO: &str = r#"#!/bin/bash
echo "$1" >>"$FAKE_CALLS"
case "$1" in
    --version) echo "cargo ${FAKE_VERSION:-0.0.0} (fixture)" ;;
    deny) echo "cargo-deny 0.0.0 (fixture)" ;;
    fmt) exit "${FAKE_FMT_EXIT:-0}" ;;
    clippy) exit 0 ;;
    *) echo "fixture cargo: not expected: $*" >&2; exit 9 ;;
esac
"#;

const FAKE_RUSTC: &str = "#!/bin/bash\necho \"rustc 0.0.0 (fixture)\"\n";

fn executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).expect("the file is written");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("the file is made runnable");
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Fixture {
    repo: Repo,
}

impl Fixture {
    /// A repository committed with the gate script, its record script, a manifest that pins frx7, and fake tools.
    fn new(name: &str) -> Fixture {
        let repo = Repo::empty(name);
        repo.write("Cargo.toml", &manifest(PIN_A));
        repo.write(".gitignore", "/target/\n");
        repo.write("scripts/gates.sh", &fs::read_to_string(root().join("scripts/gates.sh")).unwrap());
        repo.write(
            "scripts/gates/records.sh",
            &fs::read_to_string(root().join("scripts/gates/records.sh")).unwrap(),
        );
        repo.write("README.md", "A fixture repository.\n");
        repo.commit("Start the fixture");
        let bin = repo.dir.join("target/fake-bin");
        fs::create_dir_all(&bin).unwrap();
        executable(&bin.join("cargo"), FAKE_CARGO);
        executable(&bin.join("rustc"), FAKE_RUSTC);
        Fixture { repo }
    }

    /// The build folder of the fixture: its records, its logs and its fake tools live here, and nowhere else.
    fn target(&self) -> PathBuf {
        self.repo.dir.join("target")
    }

    /// The command that runs `scripts/gates.sh` with `args`, the formatter exiting with `fmt_exit` and the fake cargo
    /// printing `version`. `caller_target`, when given, is the build folder a caller had set in the environment: the
    /// run must not write into it, so the fixture's own build folder is set after it, and the last value wins.
    fn command(&self, args: &[&str], fmt_exit: u8, version: &str, caller_target: Option<&Path>) -> Command {
        let path = format!(
            "{}:{}",
            self.target().join("fake-bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = self.repo.command("bash");
        if let Some(caller) = caller_target {
            command.env("CARGO_TARGET_DIR", caller);
        }
        command
            .arg("scripts/gates.sh")
            .args(args)
            .env("PATH", path)
            .env("FAKE_CALLS", self.calls())
            .env("FAKE_FMT_EXIT", fmt_exit.to_string())
            .env("FAKE_VERSION", version)
            .env("CARGO_TARGET_DIR", self.target());
        command
    }

    /// Runs `command` and gives what it printed and how it ended.
    fn execute(&self, mut command: Command) -> Run {
        let output = command.output().expect("the script is run");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Run {
            code: output.status.code().unwrap_or(-1),
            text,
        }
    }

    /// Runs `scripts/gates.sh` with `args`; the formatter exits with `fmt_exit`.
    fn run(&self, args: &[&str], fmt_exit: u8) -> Run {
        self.run_with_version(args, fmt_exit, "0.0.0")
    }

    /// Runs `scripts/gates.sh` with `args`, the fake cargo printing `version` as its version.
    fn run_with_version(&self, args: &[&str], fmt_exit: u8, version: &str) -> Run {
        self.execute(self.command(args, fmt_exit, version, None))
    }

    fn calls(&self) -> PathBuf {
        self.target().join("calls.txt")
    }

    /// How many times the script ran the formatter, in all the runs so far.
    fn formatter_runs(&self) -> usize {
        fs::read_to_string(self.calls())
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "fmt")
            .count()
    }

    /// How many times the script ran the static analysis, in all the runs so far.
    fn analysis_runs(&self) -> usize {
        fs::read_to_string(self.calls())
            .unwrap_or_default()
            .lines()
            .filter(|line| *line == "clippy")
            .count()
    }

    /// The record of the formatting gate.
    fn format_record(&self) -> PathBuf {
        self.target().join("gate-records/format")
    }
}

#[test]
fn a_gate_that_passed_for_a_tree_is_not_run_again_on_the_same_tree() {
    let fixture = Fixture::new("records-same-tree");

    let first = fixture.run(&["--gate", "format"], 0);
    assert_eq!(first.code, 3, "{}", first.text);
    assert_eq!(
        fixture.formatter_runs(),
        1,
        "the first run ran the formatter {} time(s)",
        fixture.formatter_runs()
    );
    assert!(
        first.text.contains("ran: no pass of this gate is recorded for this tree"),
        "{}",
        first.text
    );
    assert!(fixture.format_record().is_file(), "the pass was not recorded: {}", first.text);

    let second = fixture.run(&["--gate", "format"], 0);
    assert_eq!(second.code, 3, "{}", second.text);
    assert_eq!(
        fixture.formatter_runs(),
        1,
        "the recorded gate ran again on the same tree: {}",
        second.text
    );
    assert!(
        second
            .text
            .contains("taken from its record: a pass of this gate is recorded for this tree"),
        "{}",
        second.text
    );
    assert!(second.text.contains("PASS formatting (from its record)"), "{}", second.text);
}

#[test]
fn a_change_to_a_tracked_file_runs_the_recorded_gate_again() {
    let fixture = Fixture::new("records-changed-file");
    fixture.run(&["--gate", "format"], 0);
    assert_eq!(fixture.formatter_runs(), 1);

    fixture.repo.write("README.md", "A changed fixture.\n");
    fixture.repo.commit("Change the readme");
    let run = fixture.run(&["--gate", "format"], 0);
    assert_eq!(run.code, 3, "{}", run.text);
    assert_eq!(
        fixture.formatter_runs(),
        2,
        "a change to a tracked file did not run the gate again: {}",
        run.text
    );
    assert!(
        run.text.contains("ran: no pass of this gate is recorded for this tree"),
        "{}",
        run.text
    );
}

#[test]
fn uncommitted_changes_run_every_gate_and_record_nothing_even_on_a_recorded_tree() {
    let fixture = Fixture::new("records-uncommitted");
    let clean = fixture.run(&["--gate", "format", "--gate", "lint"], 0);
    assert_eq!(clean.code, 3, "{}", clean.text);
    assert_eq!((fixture.formatter_runs(), fixture.analysis_runs()), (1, 1));

    fixture.repo.write("README.md", "Edited, not committed.\n");
    let dirty = fixture.run(&["--gate", "format", "--gate", "lint"], 0);
    assert_eq!(dirty.code, 3, "{}", dirty.text);
    assert_eq!(
        (fixture.formatter_runs(), fixture.analysis_runs()),
        (2, 2),
        "the gates of a tree with uncommitted changes were not both run: {}",
        dirty.text
    );
    assert_eq!(
        dirty.text.matches("ran: the working tree has uncommitted changes").count(),
        2,
        "{}",
        dirty.text
    );

    fixture.repo.git(&["checkout", "--", "README.md"]);
    let back = fixture.run(&["--gate", "format", "--gate", "lint"], 0);
    assert_eq!(back.code, 3, "{}", back.text);
    assert_eq!(
        (fixture.formatter_runs(), fixture.analysis_runs()),
        (2, 2),
        "the committed tree is recorded, so it is taken again: {}",
        back.text
    );
}

#[test]
fn an_untracked_file_runs_every_gate_as_uncommitted_changes_do() {
    let fixture = Fixture::new("records-untracked");
    let clean = fixture.run(&["--gate", "format"], 0);
    assert_eq!(clean.code, 3, "{}", clean.text);
    assert_eq!(fixture.formatter_runs(), 1);

    fixture.repo.write("notes.txt", "Not committed, not tracked.\n");
    let untracked = fixture.run(&["--gate", "format"], 0);
    assert_eq!(untracked.code, 3, "{}", untracked.text);
    assert_eq!(
        fixture.formatter_runs(),
        2,
        "an untracked file did not run the gate: {}",
        untracked.text
    );
    assert!(
        untracked.text.contains("ran: the working tree has uncommitted changes"),
        "{}",
        untracked.text
    );
}

#[test]
fn a_changed_toolchain_line_runs_the_recorded_gate_again() {
    let fixture = Fixture::new("records-toolchain");
    fixture.run(&["--gate", "format"], 0);
    assert_eq!(fixture.formatter_runs(), 1);

    let newer = fixture.run_with_version(&["--gate", "format"], 0, "9.9.9");
    assert_eq!(newer.code, 3, "{}", newer.text);
    assert_eq!(
        fixture.formatter_runs(),
        2,
        "a changed toolchain line did not run the recorded gate again: {}",
        newer.text
    );
    assert!(
        newer.text.contains("ran: no pass of this gate is recorded for this tree"),
        "{}",
        newer.text
    );
}

#[test]
fn a_run_never_writes_into_a_build_folder_the_caller_set() {
    let fixture = Fixture::new("records-caller-target");
    let caller = fixture.repo.dir.join("caller-target");
    fs::create_dir_all(&caller).unwrap();

    let run = fixture.execute(fixture.command(&["--gate", "format"], 0, "0.0.0", Some(&caller)));
    assert_eq!(run.code, 3, "{}", run.text);
    assert!(
        !caller.join("gate-records").exists(),
        "a run wrote records into the build folder of the caller: {}",
        run.text
    );
    assert!(
        !caller.join("gates").exists(),
        "a run wrote logs into the build folder of the caller: {}",
        run.text
    );
    assert!(
        fixture.format_record().is_file(),
        "the pass was not recorded in the fixture's own build folder: {}",
        run.text
    );
}

#[test]
fn a_hosted_run_takes_nothing_from_its_records() {
    let fixture = Fixture::new("records-hosted");
    fixture.run(&["--gate", "format"], 0);
    assert_eq!(fixture.formatter_runs(), 1);

    let hosted = fixture.run(&["--hosted", "--gate", "format"], 0);
    assert_eq!(hosted.code, 0, "{}", hosted.text);
    assert_eq!(
        fixture.formatter_runs(),
        2,
        "a hosted run took the gate from its record: {}",
        hosted.text
    );
    assert!(
        hosted.text.contains("ran: a hosted run takes nothing from its records"),
        "{}",
        hosted.text
    );
}

#[test]
fn a_failed_gate_is_not_recorded() {
    let fixture = Fixture::new("records-failed");

    let failed = fixture.run(&["--gate", "format"], 1);
    assert_eq!(failed.code, 1, "{}", failed.text);
    assert!(!fixture.format_record().exists(), "a failed gate was recorded: {}", failed.text);

    let passed = fixture.run(&["--gate", "format"], 0);
    assert_eq!(passed.code, 3, "{}", passed.text);
    assert_eq!(
        fixture.formatter_runs(),
        2,
        "the failed gate was taken from a record: {}",
        passed.text
    );
    assert!(fixture.format_record().is_file(), "{}", passed.text);
}

#[test]
fn every_gate_of_the_default_run_has_its_title_in_the_one_table() {
    // The summary lines of a gate taken from its record name it by this title, so a gate without a line here prints
    // nothing useful: the table of `scripts/gates.sh` must name each gate of the default run.
    let script = fs::read_to_string(root().join("scripts/gates.sh")).unwrap();
    for gate in ["format", "lint", "test", "core", "licence", "pin"] {
        let line = format!("GATE_TITLE_{gate}=\"");
        assert!(script.contains(&line), "the gate {gate} has no title line in scripts/gates.sh");
    }
}
