//! T006 proof shape: the one-command gate and the benchmark script exist,
//! are runnable, report the missing HyperFrames tool as OPEN (never a
//! faked comparison), and the gate refuses folders that do not validate.
//!
//! The heavy renders and fixture matrices run in the other suites and
//! inside the gate itself; these tests pin the gate's contract fast:
//! script presence, the missing-tool OPEN path with no claimed ratio,
//! and the check exit-code contract the gate relies on.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tools_dir() -> PathBuf {
    let mut d = manifest_dir();
    d.pop();
    d.pop();
    d.join("tools")
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn gate_and_benchmark_scripts_are_present_and_runnable() {
    for name in ["v1-proof.sh", "benchmark.sh"] {
        let p = tools_dir().join(name);
        assert!(p.is_file(), "{name} must exist at {}", p.display());
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(
            text.starts_with("#!/bin/sh"),
            "{name} must be a shell script"
        );
    }
    // Executable bits keep `./v1-proof.sh` working from a fresh checkout.
    #[cfg(unix)]
    for name in ["v1-proof.sh", "benchmark.sh"] {
        let p = tools_dir().join(name);
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "{name} must be executable");
    }
}

#[test]
fn benchmark_reports_open_without_hyperframes_and_claims_no_ratio() {
    if command_missing_hyperframes() {
        let script = tools_dir().join("benchmark.sh");
        let out = Command::new("sh")
            .arg(&script)
            .arg("--check-only")
            .output()
            .expect("run benchmark.sh --check-only");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        let code = out.status.code().unwrap_or(-1);
        assert_eq!(
            code, 2,
            "--check-only with no HyperFrames must exit 2, got {code}: {text}"
        );
        assert!(
            text.contains("OPEN") && text.contains("hyperframes"),
            "must report OPEN with the missing tool named: {text}"
        );
        assert!(
            !text
                .lines()
                .any(|l| l.starts_with("ratio=") || l.starts_with("RESULT")),
            "no ratio or RESULT line may be claimed while the tool is missing: {text}"
        );
    }
}

#[test]
fn gate_names_all_outcomes_and_never_passes_a_missing_tool() {
    let text = std::fs::read_to_string(tools_dir().join("v1-proof.sh")).unwrap();
    let lower = text.to_lowercase();
    for marker in [
        "O1",
        "O2",
        "O3",
        "O4",
        "O5",
        "O6",
        "O7",
        "O8",
        "cargo test --workspace",
        "playback-proof",
        "GATE: PASS",
        "GATE: OPEN",
        "GATE: FAIL",
    ] {
        assert!(text.contains(marker), "gate must mention {marker}");
    }
    assert!(
        lower.contains("hyperframes"),
        "gate must mention HyperFrames"
    );
    assert!(
        text.contains("no ratio claimed") || text.contains("no ratio is claimed"),
        "gate must state that no ratio is claimed while HyperFrames is missing"
    );
}

/// Stage a fixture folder to a temp dir: `check` appends a run log to
/// the folder it validates, so checking the committed folder in place
/// would pollute the checkout.
fn stage_fixture(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "framesmith-v1-proof-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let src = manifest_dir().join("fixtures").join(name);
    let dst = root.join(name);
    std::fs::create_dir_all(&dst).unwrap();
    for e in std::fs::read_dir(&src).unwrap() {
        let e = e.unwrap();
        let fname = e.file_name().into_string().unwrap();
        if fname == "logs" || fname == "out.mp4" {
            continue;
        }
        std::fs::copy(e.path(), dst.join(&fname)).unwrap();
    }
    (root, dst)
}

#[test]
fn gate_contract_invalid_and_later_folders_are_refused() {
    // The gate stages these same fixtures and asserts these same codes;
    // pin the contract here so a behavior change fails fast.
    let (root, _) = stage_fixture("invalid-timing");
    let (code, out) = run(&["check", "invalid-timing"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    let (root, _) = stage_fixture("later-audio");
    let (code, out) = run(&["check", "later-audio"], &root);
    assert_eq!(code, 2, "{out}");
}

fn command_missing_hyperframes() -> bool {
    // The benchmark detects HyperFrames the same way: the pinned npm
    // package via npx, invoked through the shell (benchmark.sh does
    // `npx --yes hyperframes@0.9.6 --version` in sh). Keep this probe
    // in sync with benchmark.sh, or the two disagree by construction —
    // a direct spawn of npx fails on Windows (.cmd needs a shell).
    Command::new("sh")
        .args(["-c", "npx --yes hyperframes@0.9.6 --version >/dev/null 2>&1"])
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
}
