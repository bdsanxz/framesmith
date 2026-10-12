//! Sprint 2, ticket T101: the second half of the relative-timing pair is rendered by the program of the
//! comparison, through its own command line, at the frame rate the Framesmith half is written with; its
//! counterpart and its output stay outside the repository; and with that program absent the result is
//! "not run", never "passed".
//!
//! Notes for maintainers
//! ---------------------
//! * Nothing here needs the real program of the comparison: a stand-in that notes how it was called takes
//!   its place, as in `t048_blackbox.rs`. The real renders of the pair are made by the evidence script of the
//!   ticket and judged in ticket T102.
//! * The Framesmith half is the fixture `pair-relative-timing`; its counterpart is not in the repository,
//!   so the tests write a small counterpart of their own where one is needed.

mod common;

use common::folder;
use parity::blackbox::{self, Job, Outcome};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn pair_fixture() -> PathBuf {
    root().join("fixtures").join("pair-relative-timing")
}

/// A counterpart folder with an entry page, made in the test's own folder.
fn counterpart(dir: &Path) -> PathBuf {
    let c = dir.join("counterpart");
    std::fs::create_dir_all(&c).unwrap();
    std::fs::write(c.join("index.html"), "<!doctype html><title>x</title>").unwrap();
    c
}

/// A stand-in for the program of the comparison: it records how it was called and writes a file.
fn stand_in(blackbox: &Path) {
    let bin = blackbox.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 0.0.0-stand-in; exit 0; fi\n\
         echo \"$@\" > \"{0}/recorded-args.txt\"\n\
         while [ $# -gt 0 ]; do if [ \"$1\" = \"--output\" ]; then out=$2; fi; shift; done\n\
         echo video > \"$out\"\n",
        blackbox.display()
    );
    let program = bin.join(blackbox::COMMAND);
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The number written in `data-fs-fps` by the Framesmith half of the pair.
fn written_rate() -> u32 {
    let page = std::fs::read_to_string(pair_fixture().join("index.html")).unwrap();
    let at = page.find("data-fs-fps=\"").expect("the fixture writes its frame rate") + "data-fs-fps=\"".len();
    page[at..].split('"').next().unwrap().parse().expect("the frame rate is a number")
}

#[test]
fn s2_t101_c3_the_other_half_is_asked_for_the_rate_our_half_is_written_with() {
    let dir = folder("s2_t101_c3");
    let blackbox = dir.join("blackbox");
    stand_in(&blackbox);
    let counterpart = counterpart(&dir);
    let output = dir.join("out").join("relative-timing.reference.mp4");

    // The job the maintainer gets by asking for nothing but the counterpart and the output: the rate is
    // the tool's default, and it is the rate the fixture of the pair is written with.
    let job = Job {
        blackbox: blackbox.clone(),
        counterpart: counterpart.clone(),
        output: output.clone(),
        fps: Some(blackbox::FPS),
        quality: blackbox::QUALITY.to_string(),
        browser: None,
    };
    let outcome = blackbox::run(&job);
    assert!(matches!(outcome, Outcome::Rendered { .. }), "{outcome:?}");
    assert!(output.is_file(), "no video at {}", output.display());

    let args = std::fs::read_to_string(blackbox.join("recorded-args.txt")).unwrap();
    assert!(args.starts_with(&format!("render {}", counterpart.display())), "{args}");
    assert!(args.contains(&format!("--fps {}", written_rate())), "{args}");
    assert_eq!(written_rate(), 30);
    assert_eq!(blackbox::FPS, written_rate(), "both halves of the pair are asked for the same rate");
    assert!(args.contains("--workers 1"), "{args}");
}

#[test]
fn s2_t101_c4_the_pair_fixture_holds_only_our_own_text_and_no_counterpart_or_video_is_in_the_repository() {
    // The folder of the pair holds one page, and the page is Framesmith's own format: every data attribute
    // starts with `data-fs-`, nothing is loaded from elsewhere, and no script is a file or a link.
    let names: Vec<String> = std::fs::read_dir(pair_fixture())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["index.html".to_string()], "{names:?}");
    let page = std::fs::read_to_string(pair_fixture().join("index.html")).unwrap();
    for (at, _) in page.match_indices("data-") {
        assert!(
            page[at..].starts_with("data-fs-"),
            "an attribute of another format: {}",
            &page[at..at + 24]
        );
    }
    for marker in ["http:", "https:", "<script src", "<link", "@import", "url("] {
        assert!(!page.contains(marker), "the page loads something from elsewhere: {marker}");
    }

    // Nothing of the other program is kept in the repository: no folder of counterparts (the folder name
    // the comparison tool's runner is given for them) and no rendered video, whatever its name.
    let mut offences = Vec::new();
    let mut folders = vec![root()];
    while let Some(dir) = folders.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if matches!(name.as_str(), "pairs" | "counterparts" | "blackbox" | "node_modules") {
                    offences.push(path.display().to_string());
                } else if !matches!(name.as_str(), "target" | ".git") {
                    folders.push(path);
                }
            } else if matches!(path.extension().and_then(|e| e.to_str()), Some("mp4" | "webm" | "mov" | "mkv")) {
                offences.push(path.display().to_string());
            }
        }
    }
    assert!(
        offences.is_empty(),
        "things kept in the repository that belong outside it: {offences:?}"
    );
}

#[test]
fn s2_t101_c5_with_the_reference_absent_the_pair_is_reported_not_run_and_never_passed() {
    let dir = folder("s2_t101_c5");
    let counterpart = counterpart(&dir);
    let absent = dir.join("nothing-installed-here");
    let output = dir.join("out").join("relative-timing.reference.mp4");
    let call = |blackbox: &Path| {
        Command::new(env!("CARGO_BIN_EXE_parity"))
            .arg("reference")
            .arg(&counterpart)
            .arg(&output)
            .args(["--fps", "30", "--blackbox"])
            .arg(blackbox)
            .output()
            .unwrap()
    };

    // Absent: "not run", the distinct exit code, no video, and the word "passed" nowhere.
    let ran = call(&absent);
    let said = String::from_utf8_lossy(&ran.stdout).to_string();
    assert!(said.contains("result: not run"), "{said}");
    assert!(said.contains("not installed"), "{said}");
    assert!(!said.to_lowercase().contains("passed"), "{said}");
    assert_eq!(ran.status.code(), Some(3), "{said}");
    assert!(!output.exists());

    // Control: with the program there the same call renders, and says so in other words.
    let present = dir.join("installed");
    stand_in(&present);
    let ran = call(&present);
    let said = String::from_utf8_lossy(&ran.stdout).to_string();
    assert!(said.contains("result: rendered"), "{said}");
    assert!(!said.contains("not run"), "{said}");
    assert_eq!(ran.status.code(), Some(0), "{said}");
    assert!(output.is_file());
}
