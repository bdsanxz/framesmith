//! `parity against`: the reference is run and its video is compared with ours; with the reference
//! absent every check says "not run" and the exit code is never 0; nothing of the reference is
//! written into a git repository (`docs/dev/comparison.md`, sections 9 and 10). The reference is a
//! stand-in script in a folder of the test's own.

mod common;

use common::folder;
use common::pictures::{moving, with_block, HEIGHT, WIDTH};
use parity::blackbox;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A stand-in for the reference: it writes a marker when it starts a render, and the video it makes
/// is a copy of `video`.
fn install(blackbox: &Path, video: &Path) {
    let bin = blackbox.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 0.0.0-stand-in; exit 0; fi\n\
         echo started > \"{0}/started\"\n\
         while [ $# -gt 0 ]; do if [ \"$1\" = \"--output\" ]; then out=$2; fi; shift; done\n\
         cp \"{1}\" \"$out\"\n",
        blackbox.display(),
        video.display()
    );
    let path = bin.join(blackbox::COMMAND);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Setup {
    dir: PathBuf,
    ours: PathBuf,
    counterpart: PathBuf,
    blackbox: PathBuf,
    /// The folder with a program called `node` and the system's own folders.
    search_path: std::ffi::OsString,
}

fn setup(name: &str) -> Setup {
    let dir = folder(name);
    let ours = dir.join("ours.mp4");
    measure::write_video(&ours, (30, 1), &moving(12, WIDTH, HEIGHT)).unwrap();
    let counterpart = dir.join("counterpart");
    std::fs::create_dir_all(&counterpart).unwrap();
    std::fs::write(counterpart.join("index.html"), "<!doctype html><title>x</title>").unwrap();
    let runtime = dir.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::write(runtime.join("node"), "").unwrap();
    let search_path = std::env::join_paths([runtime.as_path(), Path::new("/usr/bin"), Path::new("/bin")]).unwrap();
    Setup {
        blackbox: dir.join("blackbox"),
        dir,
        ours,
        counterpart,
        search_path,
    }
}

fn against(s: &Setup, reference_video: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parity"))
        .arg("against")
        .arg(&s.counterpart)
        .arg(&s.ours)
        .arg("--reference-video")
        .arg(reference_video)
        .arg("--blackbox")
        .arg(&s.blackbox)
        .args(extra)
        .env("PATH", &s.search_path)
        .current_dir(&s.dir)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.invalid"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A git repository of the test's own whose `.gitignore` covers the folder `ignored/`.
fn repository(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join(".gitignore"), "ignored/\n").unwrap();
    git(&repo, &["add", ".gitignore"]);
    git(&repo, &["commit", "-q", "-m", "ignore a folder"]);
    repo
}

#[test]
fn with_the_reference_not_installed_every_check_says_not_run_and_the_exit_code_is_3() {
    let s = setup("reference_comparison_not_installed");
    let out = against(&s, &s.dir.join("out").join("reference.mp4"), &[]);
    let said = text(&out);
    assert_eq!(out.status.code(), Some(3), "{said}");
    let line = said.lines().find(|l| l.trim_start().starts_with("reference render")).unwrap();
    assert!(
        line.contains("not run") && line.contains("the reference is not installed under "),
        "{line}"
    );
    for name in ["frame count", "frame rate", "size", "length", "picture"] {
        let line = said.lines().find(|l| l.trim_start().starts_with(name)).unwrap();
        assert!(line.contains("not run"), "{line}");
    }
    assert!(said.ends_with("result: not run\n"), "{said}");
    assert!(!said.to_lowercase().contains("pass"), "the word pass does not appear: {said}");
    assert!(!s.dir.join("out").exists(), "nothing was made");
}

#[test]
fn with_the_runtime_of_the_reference_missing_the_run_is_not_run_too_and_names_the_runtime() {
    let s = setup("reference_comparison_no_runtime");
    install(&s.blackbox, &s.ours);
    let out = Command::new(env!("CARGO_BIN_EXE_parity"))
        .arg("against")
        .arg(&s.counterpart)
        .arg(&s.ours)
        .arg("--reference-video")
        .arg(s.dir.join("reference.mp4"))
        .arg("--blackbox")
        .arg(&s.blackbox)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    let said = text(&out);
    assert_eq!(out.status.code(), Some(3), "{said}");
    assert!(
        said.contains("its runtime, node, was not found in any folder of the search path"),
        "{said}"
    );
    assert!(!s.blackbox.join("started").exists(), "the reference was not started");
}

#[test]
fn with_the_reference_installed_its_video_is_compared_with_ours() {
    let s = setup("reference_comparison_installed");
    install(&s.blackbox, &s.ours);
    let out = against(&s, &s.dir.join("out").join("reference.mp4"), &[]);
    let said = text(&out);
    assert_eq!(out.status.code(), Some(0), "{said}");
    let line = said.lines().find(|l| l.trim_start().starts_with("reference render")).unwrap();
    assert!(
        line.contains(" pass ") && line.contains("reference version 0.0.0-stand-in wrote "),
        "{line}"
    );
    assert!(said.contains("picture") && said.ends_with("result: passed\n"), "{said}");
    assert!(s.blackbox.join("started").exists());
}

#[test]
fn a_reference_video_that_differs_fails_the_run_with_exit_code_1() {
    let s = setup("reference_comparison_differs");
    let other = s.dir.join("other.mp4");
    measure::write_video(&other, (30, 1), &with_block(&moving(12, WIDTH, HEIGHT), 4, 30)).unwrap();
    install(&s.blackbox, &other);
    let out = against(&s, &s.dir.join("reference.mp4"), &[]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("frames under 99 %:\n  frame 4: "), "{}", text(&out));
}

#[test]
fn a_run_writes_the_reference_video_into_a_repository_only_where_git_ignores_it_and_git_status_stays_clean() {
    let s = setup("reference_comparison_ignored_place");
    install(&s.blackbox, &s.ours);
    let repo = repository(&s.dir);
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    let place = repo.join("ignored").join("reference.mp4");
    let out = against(&s, &place, &["--sheet", repo.join("ignored").join("sheet.png").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(place.is_file(), "the reference's video was written where git ignores it");
    assert_eq!(
        git(&repo, &["status", "--porcelain"]),
        "",
        "nothing of the run is in the repository's status"
    );
    assert_eq!(git(&repo, &["ls-files"]).trim(), ".gitignore");
}

#[test]
fn a_place_that_would_add_the_reference_video_to_a_repository_is_refused_before_the_reference_starts() {
    let s = setup("reference_comparison_refused_place");
    install(&s.blackbox, &s.ours);
    let repo = repository(&s.dir);
    let place = repo.join("fixtures").join("reference.mp4");
    let out = against(&s, &place, &[]);
    let said = text(&out);
    assert_eq!(out.status.code(), Some(2), "{said}");
    assert!(said.starts_with("parity: the reference's video "), "{said}");
    assert!(said.contains("reference.mp4 would be inside the git repository "), "{said}");
    assert!(said.contains("repo"), "{said}");
    assert!(!s.blackbox.join("started").exists(), "the reference was not started");
    assert!(!repo.join("fixtures").exists(), "nothing was made in the repository");
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    // The reference absent: the refusal still comes first, and says so with exit code 2.
    let absent = Setup {
        blackbox: s.dir.join("nowhere"),
        ..s
    };
    let out = against(&absent, &place, &[]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
}

#[test]
fn a_sheet_in_a_place_a_repository_does_not_ignore_is_refused_too() {
    let s = setup("reference_comparison_refused_sheet");
    install(&s.blackbox, &s.ours);
    let repo = repository(&s.dir);
    let out = against(
        &s,
        &s.dir.join("reference.mp4"),
        &["--sheet", repo.join("sheet.png").to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("the sheet "), "{}", text(&out));
    assert!(!s.blackbox.join("started").exists());
    let out = Command::new(env!("CARGO_BIN_EXE_parity"))
        .arg("compare")
        .arg(&s.ours)
        .arg(&s.ours)
        .arg("--sheet")
        .arg(repo.join("sheet.png"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(!repo.join("sheet.png").exists());
}
