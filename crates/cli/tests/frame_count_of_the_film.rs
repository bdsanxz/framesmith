//! A rendered film holds exactly the frames its length and frame rate call for, at the frame rate
//! written on the root, and the count `check` prints, the count of the summary and the count in the
//! file are one number (`docs/spec/render-command.md` section 5, `docs/spec/notation.md` section 5.1).

mod common;

use std::path::Path;

use common::project as make;
use common::*;
use measure::{open_pictures, video_facts};

const GROUP: &str = "frame-count-of-the-film";

fn composition(fps: &str, duration: &str) -> String {
    format!(
        "<composition version=\"1\" id=\"count\" width=\"32\" height=\"18\" fps=\"{fps}\" duration=\"{duration}\" background=\"#204060\">\n\
         <box left=\"4\" top=\"4\" width=\"8\" height=\"6\" background=\"#e0a030\"/>\n\
         </composition>\n"
    )
}

/// Passes when the file at `film` holds `rule` pictures, by its track and by decoding every picture;
/// otherwise says both counts.
fn film_holds(film: &Path, rule: u64) -> Result<(), String> {
    let facts = video_facts(film)?;
    if facts.frames != rule {
        return Err(format!("the file holds {} frames; the frame-grid rule gives {rule}", facts.frames));
    }
    let mut reader = open_pictures(film)?;
    let mut decoded = 0u64;
    while reader.next_frame()?.is_some() {
        decoded += 1;
    }
    if decoded != rule {
        return Err(format!("the file decodes to {decoded} frames; the frame-grid rule gives {rule}"));
    }
    Ok(())
}

/// The number on the line of `printed` that starts with `start` and ends with `end`.
fn number_between(printed: &str, start: &str, end: &str) -> u64 {
    let rest = &printed[printed.find(start).unwrap_or_else(|| panic!("no `{start}` in {printed}")) + start.len()..];
    rest[..rest.find(end).unwrap_or_else(|| panic!("no `{end}` in {printed}"))]
        .parse()
        .unwrap()
}

struct Counts {
    check: u64,
    summary: u64,
    rate: (u32, u32),
    film: std::path::PathBuf,
}

/// Runs `check`, then `render`, on a composition and gives the counts each printed and the file.
fn check_and_render(name: &str, fps: &str, duration: &str) -> Counts {
    let (project, cwd) = make(GROUP, name, &composition(fps, duration));
    let checked = framesmith(&cwd, [std::ffi::OsStr::new("check"), project.as_os_str()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    let check = number_between(&out(&checked), "Ready: the project is valid, ", " frames");
    assert!(out(&checked).ends_with(&format!(", 32x18, {fps} fps\n")), "{}", out(&checked));

    let rendered = framesmith(&cwd, [std::ffi::OsStr::new("render"), project.as_os_str()], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    let summary = number_between(&out(&rendered), "frames: ", "\n");
    assert!(out(&rendered).contains(&format!("\nframe rate: {fps} fps\n")), "{}", out(&rendered));

    let film = std::path::PathBuf::from(out(&rendered).lines().next().unwrap().strip_prefix("Rendered ").unwrap());
    assert_eq!(film, project.join("renders/demo.mp4"));
    let rate = video_facts(&film).unwrap().rate;
    Counts {
        check,
        summary,
        rate,
        film,
    }
}

#[test]
fn five_seconds_give_120_frames_at_24_150_at_30_and_300_at_60_at_the_rate_written() {
    for (fps, rule) in [(24u32, 120u64), (30, 150), (60, 300)] {
        let counts = check_and_render(&format!("five-seconds-at-{fps}"), &fps.to_string(), "5");
        film_holds(&counts.film, rule).unwrap_or_else(|message| panic!("fps {fps}: {message}"));
        assert_eq!(counts.rate, (fps, 1), "fps {fps}: the rate of the file");
        let facts = video_facts(&counts.film).unwrap();
        assert!((facts.seconds() - 5.0).abs() < 1e-9, "fps {fps}: {} s", facts.seconds());
    }
}

#[test]
fn a_duration_of_1_01_at_30_gives_31_frames_and_1_00001_gives_30() {
    for (duration, rule) in [("1.01", 31u64), ("1.00001", 30)] {
        let counts = check_and_render(&format!("between-frames-{duration}"), "30", duration);
        film_holds(&counts.film, rule).unwrap_or_else(|message| panic!("duration {duration}: {message}"));
        assert_eq!(counts.rate, (30, 1), "duration {duration}: the rate of the file");
    }
}

#[test]
fn the_count_check_prints_the_count_of_the_summary_and_the_count_in_the_file_are_one_number() {
    for (fps, duration) in [("24", "5"), ("30", "1.01"), ("30", "1.00001"), ("60", "0.5")] {
        let counts = check_and_render(&format!("one-number-{fps}-{duration}"), fps, duration);
        let in_file = video_facts(&counts.film).unwrap().frames;
        assert_eq!(
            (counts.check, counts.summary),
            (in_file, in_file),
            "fps {fps}, duration {duration}: check, summary and file"
        );
    }
}

#[test]
fn a_file_whose_count_differs_from_the_rule_fails_and_names_both_counts() {
    // A composition of 1.01 s at 30 frames a second: the file holds 31 frames.
    let counts = check_and_render("differs", "30", "1.01");
    for rule in [30u64, 32] {
        let message = film_holds(&counts.film, rule).expect_err("a count that differs must fail");
        assert_eq!(message, format!("the file holds 31 frames; the frame-grid rule gives {rule}"));
    }
    // And the test of the count fails as a test, naming both.
    let failed = std::panic::catch_unwind(|| {
        film_holds(&counts.film, 30).unwrap_or_else(|message| panic!("{message}"));
    })
    .expect_err("the check panics");
    let text = failed.downcast_ref::<String>().unwrap();
    assert!(text.contains("31") && text.contains("30"), "{text}");
}
