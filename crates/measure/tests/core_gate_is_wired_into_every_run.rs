//! Where the core gate is run, and which test files may name the comparison programs: the pieces of
//! the repository that make the core suite a gate of every change (`docs/dev/gates.md`, sections 5
//! and 10). The gate itself is proved by running it, with a control of offending tests, in
//! `scripts/gates/core-selftest.sh`.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn text(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative)).unwrap_or_else(|why| panic!("{relative}: {why}"))
}

#[test]
fn the_hosted_workflow_runs_the_core_gate_on_main_and_on_pull_requests() {
    let workflow = text(".github/workflows/gates.yml");
    assert!(
        workflow.contains("on:\n  push:\n    branches: [main]\n  pull_request:\n"),
        "the triggers of the workflow changed"
    );
    let job = workflow
        .split("\n  core-suite:\n")
        .nth(1)
        .expect("the workflow has no job core-suite");
    let job = job.split("\n\n").next().unwrap();
    assert!(
        job.lines().any(|line| line.trim() == "run: scripts/gates.sh --hosted --gate core"),
        "the job does not run the core gate: {job}"
    );
}

#[test]
fn the_local_run_before_a_push_runs_the_core_gate_with_the_others() {
    let gates = text("scripts/gates.sh");
    assert!(
        gates.contains("WANTED=\" format lint test core licence pin free\""),
        "the default run does not hold the core gate"
    );
    let hook = text(".githooks/pre-push");
    assert!(
        hook.contains("\"$root/scripts/gates.sh\""),
        "the hook does not run the gates script"
    );
    assert!(!hook.contains("--gate"), "the hook narrows the run");
}

fn files_under(folder: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn only_yardstick_files_name_the_comparison_programs() {
    let programs = [format!("{}{}", "ff", "mpeg"), format!("{}{}", "ff", "probe")];
    let mut checked = 0;
    let mut offenders = Vec::new();
    for krate in std::fs::read_dir(root().join("crates")).unwrap().flatten() {
        let tests = krate.path().join("tests");
        if !tests.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        files_under(&tests, &mut files);
        for file in files {
            let relative = file.strip_prefix(&tests).unwrap().to_path_buf();
            let yardstick = relative
                .components()
                .any(|part| part.as_os_str().to_string_lossy().starts_with("yardstick"));
            if yardstick {
                continue;
            }
            checked += 1;
            let content = std::fs::read_to_string(&file).unwrap_or_default();
            if programs.iter().any(|name| content.contains(name.as_str())) {
                offenders.push(file.display().to_string());
            }
        }
    }
    assert!(checked > 10, "only {checked} core test files were looked at");
    assert!(
        offenders.is_empty(),
        "core test files that name a comparison program: {offenders:?}"
    );
}
