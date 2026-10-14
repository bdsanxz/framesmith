//! The event check and the tracking check of `parity compare`, on pairs of videos written with frx7
//! (`measure::write_video`) with a known shift in time or in place, and read back with it.

mod common;

use common::{folder, BACKGROUND, CLIP, MOVER};
use measure::Frame;
use parity::compare::{compare, Options};
use parity::report::Verdict;
use parity::tracking::{parse_element, Marks};
use parity::Report;
use std::path::{Path, PathBuf};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 90;
const FRAMES: usize = 30;

/// A still square of `CLIP` there from frame `appears` (20 x 20 at 110, 50), and a square of `MOVER`
/// (20 x 20) whose left edge is at `left(frame)`, top at 20.
fn pictures(appears: usize, left: impl Fn(usize) -> u32) -> Vec<Frame> {
    (0..FRAMES)
        .map(|at| {
            let mut frame = Frame {
                width: WIDTH,
                height: HEIGHT,
                rgb: BACKGROUND.iter().copied().cycle().take((WIDTH * HEIGHT * 3) as usize).collect(),
            };
            let mut paint = |x0: u32, y0: u32, colour: [u8; 3]| {
                for y in y0..y0 + 20 {
                    for x in x0..x0 + 20 {
                        let i = ((y * WIDTH + x) * 3) as usize;
                        frame.rgb[i..i + 3].copy_from_slice(&colour);
                    }
                }
            };
            if at >= appears {
                paint(110, 50, CLIP);
            }
            paint(left(at), 20, MOVER);
            frame
        })
        .collect()
}

/// The mover goes right 2 pixels a frame from frame 5 to frame 20, and stands still before and after.
fn path(at: usize) -> u32 {
    10 + 2 * (at.clamp(5, 20) as u32 - 5)
}

fn write(dir: &Path, name: &str, frames: &[Frame]) -> PathBuf {
    let file = dir.join(name);
    measure::write_video(&file, (30, 1), frames).unwrap();
    file
}

fn marks(tracked: bool) -> Marks {
    Marks {
        background: Some(BACKGROUND),
        elements: vec![parse_element("clip=ff5a36").unwrap()],
        tracked: tracked.then(|| parse_element("mover=2ecc71").unwrap()),
    }
}

fn run(dir: &str, ours: &[Frame], theirs: &[Frame], tracked: bool) -> Report {
    let dir = folder(dir);
    let a = write(&dir, "ours.mp4", ours);
    let b = write(&dir, "theirs.mp4", theirs);
    let options = Options {
        marks: marks(tracked),
        ..Options::default()
    };
    compare(&a, &b, &options).unwrap()
}

fn line<'a>(report: &'a Report, name: &str) -> &'a parity::Line {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name} in\n{}", report.text()))
}

fn section<'a>(report: &'a Report, heading: &str) -> &'a [String] {
    &report
        .sections
        .iter()
        .find(|(h, _)| h.starts_with(heading))
        .unwrap_or_else(|| panic!("no section {heading} in\n{}", report.text()))
        .1
}

#[test]
fn an_element_that_appears_on_the_same_frame_in_both_passes_the_event_check() {
    let report = run("events_same_frame", &pictures(10, path), &pictures(10, path), false);
    let events = line(&report, "events");
    assert_eq!(events.verdict, Verdict::Pass, "{}", report.text());
    let listed = section(&report, "events");
    assert!(
        listed.contains(&"clip appears: ours frame 10, reference frame 10, 0 frame(s) apart  pass".to_string()),
        "{}",
        report.text()
    );
}

#[test]
fn an_element_that_appears_two_frames_later_fails_the_event_check_naming_the_event_and_both_frames() {
    let report = run("events_two_frames_late", &pictures(12, path), &pictures(10, path), false);
    let events = line(&report, "events");
    assert_eq!(events.verdict, Verdict::Fail, "{}", report.text());
    assert!(
        events
            .detail
            .contains("clip appears: ours frame 12, reference frame 10, 2 frame(s) apart"),
        "{}",
        events.detail
    );
    assert_eq!(report.overall(), Verdict::Fail);
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn an_element_that_appears_one_frame_later_passes_the_event_check() {
    let report = run("events_one_frame_late", &pictures(11, path), &pictures(10, path), false);
    assert_eq!(line(&report, "events").verdict, Verdict::Pass, "{}", report.text());
    assert!(section(&report, "events").contains(&"clip appears: ours frame 11, reference frame 10, 1 frame(s) apart  pass".to_string()));
}

#[test]
fn starting_and_stopping_to_move_are_events() {
    let late = |at: usize| path(at.saturating_sub(2));
    let report = run("events_motion_late", &pictures(10, late), &pictures(10, path), true);
    let listed = section(&report, "events");
    assert!(
        listed.contains(&"mover starts moving: ours frame 8, reference frame 6, 2 frame(s) apart  fail".to_string()),
        "{}",
        report.text()
    );
    assert!(
        listed.contains(&"mover stops moving: ours frame 23, reference frame 21, 2 frame(s) apart  fail".to_string()),
        "{}",
        report.text()
    );
    assert_eq!(line(&report, "events").verdict, Verdict::Fail);
}

#[test]
fn a_tracked_element_in_the_same_place_on_every_frame_passes_the_tracking_check() {
    let report = run("tracking_same_place", &pictures(10, path), &pictures(10, path), true);
    let tracking = line(&report, "tracking");
    assert_eq!(tracking.verdict, Verdict::Pass, "{}", report.text());
    assert!(tracking.detail.contains("on all 30 frame(s)"), "{}", tracking.detail);
    assert_eq!(section(&report, "tracked frames over"), ["none".to_string()]);
}

#[test]
fn a_tracked_element_two_pixels_off_on_one_frame_fails_naming_the_frame_and_the_distance() {
    let off = |at: usize| path(at) + if at == 14 { 2 } else { 0 };
    let report = run("tracking_two_pixels_off", &pictures(10, off), &pictures(10, path), true);
    let tracking = line(&report, "tracking");
    assert_eq!(tracking.verdict, Verdict::Fail, "{}", report.text());
    assert!(
        tracking.detail.contains("1 of 30 frame(s) over the limit") && tracking.detail.contains("frame 14: 2.0 px apart"),
        "{}",
        tracking.detail
    );
    assert_eq!(section(&report, "tracked frames over").len(), 1, "{}", report.text());
}

#[test]
fn a_tracked_element_one_pixel_off_passes_the_tracking_check() {
    let off = |at: usize| path(at) + 1;
    let report = run("tracking_one_pixel_off", &pictures(10, off), &pictures(10, path), true);
    let tracking = line(&report, "tracking");
    assert_eq!(tracking.verdict, Verdict::Pass, "{}", report.text());
    assert!(tracking.detail.contains("largest 1.0 px"), "{}", tracking.detail);
}

#[test]
fn a_fixture_with_no_tracked_element_reports_tracking_as_not_applicable_not_passed() {
    let report = run("tracking_not_marked", &pictures(10, path), &pictures(10, path), false);
    let tracking = line(&report, "tracking");
    assert_eq!(tracking.verdict, Verdict::NotApplicable, "{}", report.text());
    assert!(report.text().contains("tracking"));
    let text = report.text();
    let row = text.lines().find(|l| l.trim_start().starts_with("tracking")).unwrap();
    assert!(row.contains("not applicable") && !row.contains("pass"), "{row}");
    let json = report.json();
    let row = json["checks"].as_array().unwrap().iter().find(|c| c["name"] == "tracking").unwrap();
    assert_eq!(row["verdict"], "not applicable", "{json}");
    // The other checks still decide the result.
    assert_eq!(report.overall(), Verdict::Pass);
}

#[test]
fn marks_without_a_background_are_refused() {
    let dir = folder("marks_without_background");
    let a = write(&dir, "ours.mp4", &pictures(10, path)[..2]);
    let options = Options {
        marks: Marks {
            background: None,
            ..marks(true)
        },
        ..Options::default()
    };
    let why = compare(&a, &a, &options).unwrap_err();
    assert!(why.contains("--background"), "{why}");
}
