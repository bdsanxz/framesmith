//! Sprint 2, ticket T003: the frames of the eight Sprint 1 fixtures have not changed.
//!
//! Notes for maintainers
//! ---------------------
//! * The record of the hashes lives in `tests/records/sprint1-frames-<kind>.txt`, one file for a
//!   full browser build (`full`) and one for a headless-only build (`headless`). The kind of the
//!   browser in use is told by its program name (see `common/sprint1.rs`). It was made once, with
//!   the Sprint 1 release build, before any change to the render path.
//! * **A hash that differs is a defect.** Report it; find and fix the cause in the product. Do not
//!   replace or edit the record to make a test pass. The record changes only with the product
//!   owner's approval.
//! * The ordinary tests only read the record. The only way to write it is the ignored test
//!   `s2_t003_make_record`, which needs a release build and refuses to replace a file that exists:
//!
//!   `cargo test --release -p cli --test s2_t003_sprint1_frames -- --ignored s2_t003_make_record --nocapture`
//!
//!   (run once with each kind of browser, chosen with `FRAMESMITH_BROWSER`).
//! * With no record for the browser in use, the comparing test fails with a message that says so; it
//!   never passes and never skips.
//! * The hashes are of the pixels the browser painted (`measure::Frame::hash`), not of the encoded
//!   video.

mod common;

use common::sprint1::*;
use common::*;
use std::path::Path;
use std::process::Command;

/// A record that holds every fixture, with frames, for the browser in use.
fn record() -> Hashes {
    let (kind, version) = browser_in_use();
    load(kind, &version).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn s2_t003_c1_the_record_holds_every_frame_of_the_eight_fixtures_for_both_kinds_of_browser() {
    for kind in KINDS {
        let file = record_file(kind);
        let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        for needed in ["commit:", "date:", "browser:", "made with:", "product owner's approval", "defect"] {
            assert!(
                text.lines().any(|l| l.starts_with('#') && l.contains(needed)),
                "{kind}: the header has no {needed:?}"
            );
        }
        let hashes = parse(&text).unwrap();
        assert_eq!(hashes.keys().map(String::as_str).collect::<Vec<_>>(), {
            let mut names = FIXTURES.to_vec();
            names.sort_unstable();
            names
        });
        for (name, frames) in &hashes {
            assert!(!frames.is_empty(), "{kind}: {name} has no frames");
        }
    }
}

#[test]
fn s2_t003_c2_every_frame_of_every_sprint_1_fixture_equals_the_record() {
    let record = record();
    let (kind, version) = browser_in_use();
    let mut failures = Vec::new();
    for (name, rendered) in render_all("s2_t003_c2") {
        match rendered.and_then(|hashes| compare(&record, name, &hashes).map(|()| hashes.len())) {
            Ok(frames) => println!("{kind} build, {name}: {frames} frames, every hash equals the record"),
            Err(message) => failures.push(message),
        }
    }
    println!("browser in use: {version}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn s2_t003_c2_a_browser_with_no_record_fails_with_a_message_that_says_so() {
    let message = load("no-such-build", "Example 1.0").unwrap_err();
    assert!(message.contains("no record of Sprint 1 frames"), "{message}");
    assert!(message.contains("no-such-build") && message.contains("Example 1.0"), "{message}");
}

#[test]
fn s2_t003_c3_a_changed_colour_fails_and_names_the_fixture_and_the_first_frame_that_differs() {
    let record = record();
    // The pair-move fixture shows a red square from 1 s to 2 s: frames 30 to 59 at 30 frames per second.
    let project = fixture("s2_t003_c3", "pair-move");
    let page = project.join("index.html");
    let original = std::fs::read_to_string(&page).unwrap();
    let altered = original.replace("background: #ff5a36;", "background: #36a5ff;");
    assert_ne!(original, altered, "the colour is in the page");
    std::fs::write(&page, altered).unwrap();
    // The unchanged copy equals the record, so the colour is the only difference.
    let message = compare_project(&record, "pair-move", &project).unwrap_err();
    println!("{message}");
    assert!(message.contains("fixture pair-move"), "{message}");
    assert!(message.contains("frame 30 is the first that differs"), "{message}");
}

#[test]
fn s2_t003_c3_a_frame_count_that_differs_fails_and_so_does_a_single_hash() {
    let record = record();
    let recorded = &record["empty"];
    assert_eq!(compare(&record, "empty", recorded), Ok(()));
    let mut shorter = recorded.clone();
    shorter.pop();
    let message = compare(&record, "empty", &shorter).unwrap_err();
    assert!(
        message.contains("fixture empty") && message.contains("frames were rendered"),
        "{message}"
    );
    let mut changed = recorded.clone();
    changed[7] ^= 1;
    let message = compare(&record, "empty", &changed).unwrap_err();
    assert!(
        message.contains("fixture empty") && message.contains("frame 7 is the first that differs"),
        "{message}"
    );
}

#[test]
fn s2_t003_c4_recording_refuses_to_replace_an_existing_record() {
    let dir = scratch("s2_t003_c4");
    let file = dir.join("sprint1-frames-full.txt");
    write_new(&file, "first\n").unwrap();
    let message = write_new(&file, "second\n").unwrap_err();
    assert!(
        message.contains("already exists") && message.contains("product owner's approval"),
        "{message}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "first\n", "the record is as it was");
}

#[test]
fn s2_t003_c4_the_records_in_the_repository_say_that_a_differing_hash_is_a_defect() {
    for kind in KINDS {
        let text = std::fs::read_to_string(record_file(kind)).unwrap().to_lowercase();
        assert!(text.contains("changes only with the product owner's approval"), "{kind}");
        assert!(text.contains("a differing hash is a defect"), "{kind}");
    }
}

fn git(args: &[&str]) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ran = measure::run(Path::new("/usr/bin/git"), args, dir, &[]);
    assert_eq!(ran.code, 0, "git {args:?}: {}", ran.all());
    ran.stdout.trim_end().to_string()
}

/// The maintainer's way to make a record. Not run by the ordinary tests.
#[test]
#[ignore = "makes the record of Sprint 1 frames; run by hand, once for each kind of browser"]
fn s2_t003_make_record() {
    if cfg!(debug_assertions) {
        panic!("the record is made with a release build: use `cargo test --release`");
    }
    let (kind, version) = browser_in_use();
    let file = record_file(kind);
    assert!(
        !file.exists(),
        "{} already exists and is not replaced: the record changes only with the product owner's approval",
        file.display()
    );

    let commit = git(&["rev-parse", "HEAD"]);
    let changed = git(&["status", "--porcelain", "--untracked-files=all"]);
    assert!(
        changed
            .lines()
            .all(|line| line[3..].starts_with("docs/") || line[3..].starts_with("crates/cli/tests/")),
        "product code differs from the commit (only docs and the tests of this ticket may), so this is not the Sprint 1 build:\n{changed}"
    );
    let date = Command::new("/bin/date").arg("+%Y-%m-%d").output().unwrap();
    let platform = Command::new("/usr/bin/uname").arg("-sm").output().unwrap();
    let mut header = vec![
        format!("commit: {commit}"),
        format!("date: {}", String::from_utf8_lossy(&date.stdout).trim()),
        format!("platform: {}", String::from_utf8_lossy(&platform.stdout).trim()),
        format!("browser: {version}"),
        format!(
            "made with: the Sprint 1 release build (cargo test --release -p cli --test s2_t003_sprint1_frames -- --ignored s2_t003_make_record); \
             FRAMESMITH_BROWSER={}",
            std::env::var("FRAMESMITH_BROWSER").unwrap_or_else(|_| "(not set: the default browser)".to_string())
        ),
        "uncommitted changes when it was made (none of them product code, which equals the commit):".to_string(),
    ];
    header.extend(changed.lines().map(|line| format!("  {line}")));
    if changed.is_empty() {
        header.push("  (none)".to_string());
    }
    header.push(
        "How it is used: tests/s2_t003_sprint1_frames.rs renders the eight fixtures with the current build and compares every frame hash with this file.".to_string(),
    );
    header.push(
        "A differing hash is a defect to be reported. This record is not replaced or edited to make a test pass; it changes only with the product owner's approval.".to_string(),
    );

    let mut hashes = Vec::new();
    for (name, rendered) in render_all("s2_t003_make_record") {
        let rendered = rendered.unwrap_or_else(|e| panic!("fixture {name}: {e}"));
        println!("{kind} build, {name}: {} frames recorded", rendered.len());
        hashes.push((name, rendered));
    }
    write_new(&file, &record_text(kind, &header, &hashes)).unwrap();
    println!("written: {}", file.display());
}
