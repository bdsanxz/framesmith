//! The records of passed gates (`docs/dev/gates.md`, section 14). A gate that passed for a tree is taken from its
//! record on the same tree; a change to a tracked file runs it again; uncommitted changes run every gate and are
//! not recorded; a hosted run takes nothing from the records; a failed gate is recorded nowhere.
//!
//! Each case is a small git repository of its own, with a copy of `scripts/gates.sh` and of
//! `scripts/gates/records.sh`, and a fake `cargo` and `rustc` in front of the search path. The fake `cargo` writes
//! down each call, so the number of formatter runs is what the script really did.

mod common;

use common::{manifest, Repo, Run, PIN_A};
use std::fs;
use std::path::{Path, PathBuf};

/// Writes its subcommand to the call log, then behaves as the gates need: `fmt` exits with `FAKE_FMT_EXIT`.
const FAKE_CARGO: &str = r#"#!/bin/bash
echo "$1" >>"$FAKE_CALLS"
case "$1" in
    --version) echo "cargo 0.0.0 (fixture)" ;;
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

    /// Runs `scripts/gates.sh` with `args`; the formatter exits with `fmt_exit`.
    fn run(&self, args: &[&str], fmt_exit: u8) -> Run {
        let path = format!(
            "{}:{}",
            self.repo.dir.join("target/fake-bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = self.repo.command("bash");
        command
            .arg("scripts/gates.sh")
            .args(args)
            .env("PATH", path)
            .env("FAKE_CALLS", self.calls())
            .env("FAKE_FMT_EXIT", fmt_exit.to_string());
        let output = command.output().expect("the script is run");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Run {
            code: output.status.code().unwrap_or(-1),
            text,
        }
    }

    fn calls(&self) -> PathBuf {
        self.repo.dir.join("target/calls.txt")
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
        self.repo.dir.join("target/gate-records/format")
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
