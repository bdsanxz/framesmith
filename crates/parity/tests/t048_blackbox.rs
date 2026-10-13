//! T048: the reference is a black box in a git-ignored folder; nothing the product builds or tests needs it.

mod common;

use common::folder;
use parity::blackbox::{self, Job, Outcome};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn counterpart(dir: &Path) -> PathBuf {
    let c = dir.join("counterpart");
    std::fs::create_dir_all(&c).unwrap();
    std::fs::write(c.join("index.html"), "<!doctype html><title>x</title>").unwrap();
    c
}

fn job(dir: &Path, blackbox: &Path) -> Job {
    Job {
        blackbox: blackbox.to_path_buf(),
        counterpart: counterpart(dir),
        output: dir.join("out").join("reference.mp4"),
        fps: Some(30),
        quality: "standard".into(),
        browser: None,
    }
}

/// A stand-in for the reference's command that records how it was called and writes a file.
fn fake_reference(blackbox: &Path, exit_code: i32) {
    let bin = blackbox.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 0.0.0-fake; exit 0; fi\n\
         env > \"{0}/recorded-env.txt\"\n\
         echo \"$@\" > \"{0}/recorded-args.txt\"\n\
         pwd > \"{0}/recorded-cwd.txt\"\n\
         while [ $# -gt 0 ]; do if [ \"$1\" = \"--output\" ]; then out=$2; fi; shift; done\n\
         [ {1} -eq 0 ] && echo fake > \"$out\"\n\
         exit {1}\n",
        blackbox.display(),
        exit_code
    );
    let path = bin.join("hyperframes");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn t048_c5_with_the_reference_absent_the_tool_says_not_run_and_never_passed() {
    let dir = folder("t048_c5_absent");
    let empty = dir.join("blackbox");
    std::fs::create_dir_all(&empty).unwrap();
    let outcome = blackbox::run(&job(&dir, &empty));
    assert!(matches!(outcome, Outcome::NotRun(_)), "{outcome:?}");
    let text = outcome.text();
    assert!(text.contains("not run"), "{text}");
    assert!(text.contains("not installed"), "{text}");
    assert!(!text.to_lowercase().contains("passed"), "{text}");
    assert_ne!(outcome.exit_code(), 0);
    assert_eq!(outcome.exit_code(), 3, "not run has its own exit code");
    assert!(!dir.join("out").join("reference.mp4").exists());
}

#[test]
fn t048_c5_the_binary_prints_not_run_and_exits_with_the_distinct_code() {
    let dir = folder("t048_c5_binary");
    let c = counterpart(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_parity"))
        .args(["reference"])
        .arg(&c)
        .arg(dir.join("o.mp4"))
        .args(["--blackbox"])
        .arg(dir.join("nothing-here"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("not run"), "{stdout}");
    assert!(!stdout.to_lowercase().contains("passed"), "{stdout}");
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn t048_c1_the_reference_is_run_with_the_rate_quality_and_one_worker_set_explicitly() {
    let dir = folder("t048_c1_args");
    let blackbox = dir.join("blackbox");
    fake_reference(&blackbox, 0);
    let mut j = job(&dir, &blackbox);
    j.fps = Some(24);
    j.quality = "draft".into();
    let outcome = blackbox::run(&j);
    assert_eq!(
        outcome,
        Outcome::Rendered {
            version: "0.0.0-fake".into()
        }
    );
    assert!(outcome.text().contains("rendered") && !outcome.text().contains("passed"));
    assert_eq!(outcome.exit_code(), 0);
    let args = std::fs::read_to_string(blackbox.join("recorded-args.txt")).unwrap();
    assert!(args.starts_with("render "), "{args}");
    assert!(args.contains("--fps 24"), "{args}");
    assert!(args.contains("--quality draft"), "{args}");
    assert!(args.contains("--workers 1"), "{args}");
    assert!(
        args.contains(&format!("--output {}", dir.join("out").join("reference.mp4").display())),
        "{args}"
    );
    assert!(dir.join("out").join("reference.mp4").is_file());
}

#[test]
fn t048_c3_usage_data_and_update_checks_are_off_and_its_state_stays_inside_the_folder() {
    let dir = folder("t048_c3_env");
    let blackbox = dir.join("blackbox");
    fake_reference(&blackbox, 0);
    let mut j = job(&dir, &blackbox);
    j.browser = Some(PathBuf::from("/somewhere/browser"));
    assert_eq!(
        blackbox::run(&j),
        Outcome::Rendered {
            version: "0.0.0-fake".into()
        }
    );
    let env = std::fs::read_to_string(blackbox.join("recorded-env.txt")).unwrap();
    for wanted in [
        "HYPERFRAMES_NO_TELEMETRY=1",
        "DO_NOT_TRACK=1",
        "HYPERFRAMES_NO_UPDATE_CHECK=1",
        "HYPERFRAMES_NO_AUTO_INSTALL=1",
        "HYPERFRAMES_SKIP_SKILLS=1",
        "HYPERFRAMES_BROWSER_PATH=/somewhere/browser",
    ] {
        assert!(env.lines().any(|l| l == wanted), "{wanted} missing from the environment:\n{env}");
    }
    let home = env.lines().find_map(|l| l.strip_prefix("HOME=")).unwrap();
    assert!(
        Path::new(home).starts_with(&blackbox),
        "HOME={home} is outside {}",
        blackbox.display()
    );
    let cache = env.lines().find_map(|l| l.strip_prefix("HYPERFRAMES_EXTRACT_CACHE_DIR=")).unwrap();
    assert!(Path::new(cache).starts_with(&blackbox), "{cache}");
    let cwd = std::fs::read_to_string(blackbox.join("recorded-cwd.txt")).unwrap();
    assert!(Path::new(cwd.trim()).ends_with("blackbox"), "{cwd}");
}

#[test]
fn t048_a_reference_that_fails_is_reported_as_failed_not_as_not_run_or_passed() {
    let dir = folder("t048_failed");
    let blackbox = dir.join("blackbox");
    fake_reference(&blackbox, 1);
    let outcome = blackbox::run(&job(&dir, &blackbox));
    assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
    assert!(outcome.text().contains("result: failed"));
    assert_eq!(outcome.exit_code(), 1);
}

#[test]
fn t048_a_counterpart_without_an_entry_page_is_refused_before_anything_runs() {
    let dir = folder("t048_no_entry");
    let blackbox = dir.join("blackbox");
    fake_reference(&blackbox, 0);
    let mut j = job(&dir, &blackbox);
    j.counterpart = dir.join("empty-folder");
    std::fs::create_dir_all(&j.counterpart).unwrap();
    let outcome = blackbox::run(&j);
    assert!(
        matches!(&outcome, Outcome::Failed(why) if why.contains("index.html")),
        "{outcome:?}"
    );
    assert!(!blackbox.join("recorded-args.txt").exists());
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[test]
fn t048_c2_the_reference_folder_defaults_to_the_nearest_tools_blackbox_at_or_above_the_current_folder() {
    let dir = folder("t048_c2_default_folder");
    let workspace = dir.join("workspace");
    let outer = workspace.join("tools").join("blackbox");
    let deep = workspace.join("repository").join("crates").join("parity");
    std::fs::create_dir_all(&outer).unwrap();
    std::fs::create_dir_all(&deep).unwrap();
    // From the workspace root, from a folder of the repository, and from a worktree deeper down.
    assert_eq!(blackbox::default_folder(&workspace), outer);
    assert_eq!(blackbox::default_folder(&workspace.join("repository")), outer);
    assert_eq!(blackbox::default_folder(&deep), outer);
    // A nearer `tools/blackbox` wins over a farther one.
    let nearer = workspace.join("repository").join("tools").join("blackbox");
    std::fs::create_dir_all(&nearer).unwrap();
    assert_eq!(blackbox::default_folder(&deep), nearer);
    assert_eq!(blackbox::default_folder(&workspace), outer);
}

#[test]
fn t048_c2_the_default_folder_is_found_by_the_binary_when_the_flag_is_left_out() {
    let dir = folder("t048_c2_binary_default");
    let workspace = dir.join("workspace");
    let work = workspace.join("repository");
    std::fs::create_dir_all(&work).unwrap();
    // The reference is installed in the workspace's folder, outside the folder the command runs in.
    fake_reference(&workspace.join("tools").join("blackbox"), 0);
    let out = Command::new(env!("CARGO_BIN_EXE_parity"))
        .current_dir(&work)
        .args(["reference"])
        .arg(counterpart(&dir))
        .arg(dir.join("o.mp4"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("result: rendered"), "{stdout}");
    assert_eq!(out.status.code(), Some(0));
}

fn files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target" || n == "node_modules") {
                continue;
            }
            files_under(&path, found);
        } else if path
            .extension()
            .is_some_and(|e| e == "rs" || e == "toml" || e == "sh" || e == "yml")
        {
            found.push(path);
        }
    }
}

#[test]
fn t048_c6_nothing_in_the_products_gates_depends_on_the_reference() {
    let crates = root().join("crates");
    let mut offences = Vec::new();
    for entry in std::fs::read_dir(&crates).unwrap().flatten() {
        if entry.file_name() == "parity" {
            continue;
        }
        let mut files = Vec::new();
        files_under(&entry.path(), &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            // No other crate may depend on this one, mention the folder of the reference, or name its command.
            if text.contains("blackbox") || text.contains("hyperframes") || text.lines().any(|l| l.trim_start().starts_with("parity")) {
                offences.push(file.display().to_string());
            }
        }
    }
    let mut gates = Vec::new();
    files_under(&root().join("scripts"), &mut gates);
    files_under(&root().join(".github"), &mut gates);
    for file in gates {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        if text.contains("blackbox") || text.contains("hyperframes") || text.contains("parity") {
            offences.push(file.display().to_string());
        }
    }
    assert!(
        offences.is_empty(),
        "these files depend on the comparison tool or the reference: {offences:?}"
    );
}

#[test]
fn t048_c6_the_comparison_tools_own_tests_never_read_the_reference_folder() {
    let mut files = Vec::new();
    files_under(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"), &mut files);
    for file in files {
        if file.file_name().is_some_and(|n| n == "t048_blackbox.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("tools/blackbox"), "{} reads the reference's folder", file.display());
    }
    // And with the default folder absent, the run says so (it is not required to exist).
    let dir = folder("t048_c6_default");
    let mut j = job(&dir, Path::new("tools/blackbox-that-is-not-here"));
    j.blackbox = dir.join("never-installed");
    assert!(matches!(blackbox::run(&j), Outcome::NotRun(_)));
}

#[test]
fn t048_no_frame_rate_is_passed_when_the_maintainer_asks_for_the_reference_s_own() {
    let dir = folder("t048_own_rate");
    let blackbox = dir.join("blackbox");
    fake_reference(&blackbox, 0);
    let mut j = job(&dir, &blackbox);
    j.fps = None;
    assert!(matches!(blackbox::run(&j), Outcome::Rendered { .. }));
    let args = std::fs::read_to_string(blackbox.join("recorded-args.txt")).unwrap();
    assert!(!args.contains("--fps"), "{args}");
    assert!(args.contains("--quality standard") && args.contains("--workers 1"), "{args}");
}
