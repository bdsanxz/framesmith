//! The `parity compare` command as the maintainer runs it: what it prints, and its exit code. It is
//! run with a search path that holds no program but the system's own, to show that it reads both
//! videos with frx7 alone (`docs/dev/comparison.md`, section 2).

mod common;

use common::folder;
use common::pictures::{moving, with_block, HEIGHT, WIDTH};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn write(dir: &Path, name: &str, frames: &[measure::Frame]) -> PathBuf {
    let path = dir.join(name);
    measure::write_video(&path, (30, 1), frames).unwrap();
    path
}

/// Runs the command with a search path of one empty folder: nothing can be found on it.
fn run(dir: &Path, args: &[&std::ffi::OsStr]) -> Output {
    let empty = dir.join("empty-search-path");
    std::fs::create_dir_all(&empty).unwrap();
    run_on(dir, &empty, args)
}

fn run_on(dir: &Path, search_path: &Path, args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parity"))
        .args(args)
        .env("PATH", search_path)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn os(text: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(text)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn equal_videos_pass_and_the_command_exits_with_0_with_nothing_on_the_search_path() {
    let dir = folder("comparison_command_pass");
    let ours = write(&dir, "ours.mp4", &moving(12, WIDTH, HEIGHT));
    let theirs = dir.join("theirs.mp4");
    std::fs::copy(&ours, &theirs).unwrap();
    let out = run(&dir, &[os("compare"), ours.as_os_str(), theirs.as_os_str()]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}\n{}", String::from_utf8_lossy(&out.stderr));
    for name in ["frame count", "frame rate", "size", "length", "picture"] {
        let found = text
            .lines()
            .find(|l| l.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("{name}\n{text}"));
        assert!(found.contains(" pass "), "{found}");
    }
    assert!(text.ends_with("result: passed\n"), "{text}");
}

#[test]
fn a_frame_that_differs_fails_the_command_with_exit_code_1_and_lists_the_frame() {
    let dir = folder("comparison_command_fail");
    let base = moving(12, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", &base);
    let theirs = write(&dir, "theirs.mp4", &with_block(&base, 7, 30));
    let out = run(
        &dir,
        &[
            os("compare"),
            ours.as_os_str(),
            theirs.as_os_str(),
            os("--scene"),
            os("a=0-5"),
            os("--scene"),
            os("b=6-11"),
        ],
    );
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("frames under 99 %:\n  frame 7: "), "{text}");
    let b = text.lines().find(|l| l.contains("scene b (frames 6-11)")).unwrap();
    assert!(b.contains(" fail "), "{b}");
    let a = text.lines().find(|l| l.contains("scene a (frames 0-5)")).unwrap();
    assert!(a.contains(" pass "), "{a}");
    assert!(text.ends_with("result: failed\n"), "{text}");
}

#[test]
fn a_cause_given_on_the_command_line_shows_the_frame_as_excused() {
    let dir = folder("comparison_command_excused");
    let base = moving(40, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", &base);
    // One frame in forty at about 80 %: the average stays above 99 %.
    let theirs = write(&dir, "theirs.mp4", &with_block(&base, 7, 20));
    let out = run(
        &dir,
        &[
            os("compare"),
            ours.as_os_str(),
            theirs.as_os_str(),
            os("--excuse"),
            os("7=a different still"),
        ],
    );
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("excused (not passed): a different still"), "{text}");
    assert!(text.contains("1 frame under 99 % (1 excused)"), "{text}");
}

#[test]
fn the_json_report_has_the_checks_the_scenes_and_every_frame() {
    let dir = folder("comparison_command_json");
    let base = moving(12, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", &base);
    let theirs = write(&dir, "theirs.mp4", &with_block(&base, 7, 30));
    let out = run(&dir, &[os("compare"), ours.as_os_str(), theirs.as_os_str(), os("--json")]);
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["result"], "failed");
    let names: Vec<&str> = json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "frame count",
            "frame rate",
            "size",
            "length",
            "picture",
            "scene all (frames 0-11)",
            "events",
            "tracking",
            "sound length",
            "loudness",
            "peak",
            "alignment"
        ]
    );
    // No element is marked: neither line may read as a pass.
    assert_eq!(json["checks"][6]["verdict"], "not applicable");
    assert_eq!(json["checks"][7]["verdict"], "not applicable");
    assert_eq!(json["picture"]["frame_scores"].as_array().unwrap().len(), 12);
    assert_eq!(json["picture"]["frame_scores"][7]["state"], "fail");
    assert_eq!(json["picture"]["scenes"][0]["name"], "all");
}

#[test]
fn the_sheet_is_written_where_asked_and_names_the_low_frames() {
    let dir = folder("comparison_command_sheet");
    let base = moving(12, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", &base);
    let theirs = write(&dir, "theirs.mp4", &with_block(&base, 7, 30));
    let sheet = dir.join("out").join("low.png");
    // The sheet's place is looked at with git (the folder is inside this repository's ignored `target`),
    // so the system's folders are on the search path for this one.
    let out = run_on(
        &dir,
        Path::new("/usr/bin:/bin"),
        &[
            os("compare"),
            ours.as_os_str(),
            theirs.as_os_str(),
            os("--sheet"),
            sheet.as_os_str(),
        ],
    );
    assert!(stdout(&out).contains("sheet: "), "{}", stdout(&out));
    let (width, height) = measure::png_size(&std::fs::read(&sheet).unwrap()).unwrap();
    assert!(width > 700 && height > 100, "{width}x{height}");
}

#[test]
fn a_wrong_call_exits_with_2_and_makes_no_report() {
    let dir = folder("comparison_command_usage");
    let ours = write(&dir, "ours.mp4", &moving(6, WIDTH, HEIGHT));
    let junk = dir.join("junk.mp4");
    std::fs::write(&junk, b"not a video").unwrap();
    let missing = dir.join("missing.mp4");
    for args in [
        vec![os("compare"), ours.as_os_str()],
        vec![os("compare"), ours.as_os_str(), missing.as_os_str()],
        vec![os("compare"), ours.as_os_str(), junk.as_os_str()],
        vec![os("compare"), ours.as_os_str(), ours.as_os_str(), os("--scene"), os("a=0-2")],
        vec![os("compare"), ours.as_os_str(), ours.as_os_str(), os("--excuse"), os("3=")],
        vec![os("compare"), ours.as_os_str(), ours.as_os_str(), os("--threshold"), os("0.5")],
        vec![os("nothing")],
    ] {
        let out = run(&dir, &args);
        let text = stdout(&out);
        assert_eq!(out.status.code(), Some(2), "{args:?}\n{text}");
        assert!(text.starts_with("parity: "), "{text}");
        assert!(!text.contains("result:"), "no report is made: {text}");
    }
}
