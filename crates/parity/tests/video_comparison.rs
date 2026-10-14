//! Two videos compared: the four structural checks and the picture check, on videos written with
//! frx7 (`measure::write_video`) and read back with it. No outside program is started.

mod common;

use common::folder;
use common::pictures::{moving, with_block, HEIGHT, WIDTH};
use parity::compare::{compare, Options};
use parity::picture::{parse_excuse, parse_scene};
use parity::report::Verdict;
use parity::Report;
use std::path::{Path, PathBuf};

fn write(dir: &Path, name: &str, rate: (u32, u32), frames: &[measure::Frame]) -> PathBuf {
    let path = dir.join(name);
    measure::write_video(&path, rate, frames).unwrap();
    path
}

fn line<'a>(report: &'a Report, name: &str) -> &'a parity::Line {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name} in\n{}", report.text()))
}

const STRUCTURE: [&str; 4] = ["frame count", "frame rate", "size", "length"];

#[test]
fn two_videos_of_one_count_rate_size_and_length_pass_all_four_structural_checks() {
    let dir = folder("video_comparison_same_structure");
    let ours = write(&dir, "ours.mp4", (30, 1), &moving(24, WIDTH, HEIGHT));
    let theirs = write(&dir, "theirs.mp4", (30, 1), &with_block(&moving(24, WIDTH, HEIGHT), 5, 40));
    let report = compare(&ours, &theirs, &Options::default()).unwrap();
    for name in STRUCTURE {
        assert_eq!(line(&report, name).verdict, Verdict::Pass, "{name}\n{}", report.text());
    }
    assert_eq!(line(&report, "frame count").detail, "ours 24, reference 24");
    assert_eq!(line(&report, "frame rate").detail, "ours 30/1, reference 30/1");
    assert_eq!(line(&report, "size").detail, "ours 160x90, reference 160x90");
    assert!(
        line(&report, "length").detail.starts_with("ours 0.800000 s"),
        "{}",
        line(&report, "length").detail
    );
}

#[test]
fn a_different_size_fails_the_size_alone_and_shows_both_sizes() {
    let dir = folder("video_comparison_other_size");
    let ours = write(&dir, "ours.mp4", (30, 1), &moving(12, WIDTH, HEIGHT));
    let theirs = write(&dir, "theirs.mp4", (30, 1), &moving(12, 128, 72));
    let report = compare(&ours, &theirs, &Options::default()).unwrap();
    assert_eq!(line(&report, "size").verdict, Verdict::Fail);
    assert_eq!(line(&report, "size").detail, "ours 160x90, reference 128x72");
    for name in ["frame count", "frame rate", "length"] {
        assert_eq!(line(&report, name).verdict, Verdict::Pass, "{name}");
    }
    assert_eq!(report.overall(), Verdict::Fail);
    // Pictures of two sizes cannot be paired, and nothing is measured.
    let picture = line(&report, "picture");
    assert_eq!(picture.verdict, Verdict::Fail);
    assert_eq!(picture.detail, "frames cannot be paired: ours is 160x90, reference is 128x72");
}

#[test]
fn a_missing_frame_fails_the_count_and_the_length_with_both_values_and_the_frames_cannot_be_paired() {
    let dir = folder("video_comparison_missing_frame");
    let all = moving(24, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", (30, 1), &all);
    let theirs = write(&dir, "theirs.mp4", (30, 1), &all[..23]);
    let report = compare(&ours, &theirs, &Options::default()).unwrap();
    assert_eq!(line(&report, "frame count").verdict, Verdict::Fail);
    assert_eq!(line(&report, "frame count").detail, "ours 24, reference 23");
    assert_eq!(line(&report, "length").verdict, Verdict::Fail);
    assert!(
        line(&report, "length").detail.contains("ours 0.800000 s"),
        "{}",
        line(&report, "length").detail
    );
    assert!(
        line(&report, "length").detail.contains("reference 0.766667 s"),
        "{}",
        line(&report, "length").detail
    );
    for name in ["frame rate", "size"] {
        assert_eq!(line(&report, name).verdict, Verdict::Pass, "{name}");
    }
    assert_eq!(
        line(&report, "picture").detail,
        "frames cannot be paired: ours has 24 frames, reference has 23"
    );
    assert_eq!(report.overall(), Verdict::Fail);
    assert_eq!(report.exit_code(), 1);
    assert!(!report.text().contains("result: passed"));
}

#[test]
fn a_different_frame_rate_fails_the_rate_with_both_rates_when_the_counts_are_equal() {
    let dir = folder("video_comparison_other_rate");
    let all = moving(24, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", (30, 1), &all);
    let theirs = write(&dir, "theirs.mp4", (24, 1), &all);
    let report = compare(&ours, &theirs, &Options::default()).unwrap();
    assert_eq!(line(&report, "frame rate").verdict, Verdict::Fail);
    assert_eq!(line(&report, "frame rate").detail, "ours 30/1, reference 24/1");
    assert_eq!(line(&report, "frame count").verdict, Verdict::Pass);
    assert_eq!(line(&report, "size").verdict, Verdict::Pass);
    // 24 pictures last 0.8 s at 30 a second and 1 s at 24 a second.
    assert_eq!(line(&report, "length").verdict, Verdict::Fail);
    assert!(
        line(&report, "length").detail.contains("reference 1.000000 s"),
        "{}",
        line(&report, "length").detail
    );
    // The pictures are the same pictures, so the check of them still runs and passes.
    assert_eq!(line(&report, "picture").verdict, Verdict::Pass);
}

#[test]
fn videos_with_equal_frames_pass_the_picture_check_with_100_percent_on_every_frame() {
    let dir = folder("video_comparison_equal_frames");
    let ours = write(&dir, "ours.mp4", (30, 1), &moving(24, WIDTH, HEIGHT));
    let theirs = dir.join("theirs.mp4");
    std::fs::copy(&ours, &theirs).unwrap();
    let options = Options {
        per_frame: true,
        ..Options::default()
    };
    let report = compare(&ours, &theirs, &options).unwrap();
    assert_eq!(line(&report, "picture").verdict, Verdict::Pass, "{}", report.text());
    assert_eq!(
        line(&report, "picture").detail,
        "average 100.000 % over 24 frames; lowest frame 0 at 100.000 %; no frame under 99 %"
    );
    let text = report.text();
    for number in 0..24 {
        assert!(text.contains(&format!("frame {number}: 100.000 %")), "frame {number}\n{text}");
    }
    assert_eq!(report.overall(), Verdict::Pass);
    assert_eq!(report.exit_code(), 0);
    assert!(text.ends_with("result: passed\n"), "{text}");
    let frames = &report.json()["picture"]["frame_scores"];
    assert_eq!(frames.as_array().unwrap().len(), 24);
    assert!(frames.as_array().unwrap().iter().all(|f| f["score"] == 100.0 && f["state"] == "ok"));
}

#[test]
fn a_frame_that_differs_in_more_than_one_percent_is_listed_with_its_number_and_score_and_fails_its_scene() {
    let dir = folder("video_comparison_one_frame_differs");
    let base = moving(24, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", (30, 1), &base);
    let theirs = write(&dir, "theirs.mp4", (30, 1), &with_block(&base, 17, 30));
    let options = Options {
        scenes: vec![parse_scene("first=0-11").unwrap(), parse_scene("second=12-23").unwrap()],
        ..Options::default()
    };
    let report = compare(&ours, &theirs, &options).unwrap();
    let text = report.text();
    assert_eq!(line(&report, "scene first (frames 0-11)").verdict, Verdict::Pass, "{text}");
    let second = line(&report, "scene second (frames 12-23)");
    assert_eq!(second.verdict, Verdict::Fail, "{text}");
    assert!(second.detail.contains("lowest frame 17 at "), "{}", second.detail);
    assert_eq!(line(&report, "picture").verdict, Verdict::Fail);
    // The list names the frame and its score: 30 % of the rows were repainted, less the pixels that
    // already had that colour or were within the threshold of it.
    let listed: Vec<&String> = report
        .sections
        .iter()
        .find(|(heading, _)| heading == "frames under 99 %")
        .map(|(_, lines)| lines.iter().collect())
        .unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert!(listed[0].starts_with("frame 17: "), "{}", listed[0]);
    assert!(listed[0].ends_with("%  fail"), "{}", listed[0]);
    let score: f64 = listed[0]["frame 17: ".len()..].split(' ').next().unwrap().parse().unwrap();
    assert!((60.0..75.0).contains(&score), "{score}");
    let frames = &report.json()["picture"]["frame_scores"];
    assert_eq!(frames[17]["state"], "fail");
    assert_eq!(frames[16]["state"], "ok");
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn a_change_in_under_one_percent_of_a_frames_pixels_does_not_make_the_frame_low() {
    let dir = folder("video_comparison_small_change");
    let base = moving(12, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", (30, 1), &base);
    // One row of 90 is 1.1 %; build a change of a few pixels instead.
    let mut small = base.clone();
    for x in 100..110 {
        let at = (80 * WIDTH + x) as usize * 3;
        small[3].rgb[at..at + 3].copy_from_slice(&[0xff, 0xff, 0xff]);
    }
    let theirs = write(&dir, "theirs.mp4", (30, 1), &small);
    let report = compare(&ours, &theirs, &Options::default()).unwrap();
    assert_eq!(line(&report, "picture").verdict, Verdict::Pass, "{}", report.text());
    assert!(
        line(&report, "picture").detail.ends_with("no frame under 99 %"),
        "{}",
        line(&report, "picture").detail
    );
}

#[test]
fn the_average_and_every_scene_are_reported_and_a_low_scene_fails_the_check_though_the_average_is_high() {
    let dir = folder("video_comparison_scenes");
    let base = moving(30, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", (30, 1), &base);
    let theirs = write(&dir, "theirs.mp4", (30, 1), &with_block(&base, 2, 20));
    // One frame in thirty at about 80 %: the average over all is above 99 %, the scene of the frame is not.
    let options = Options {
        scenes: vec![parse_scene("short=0-2").unwrap(), parse_scene("long=3-29").unwrap()],
        ..Options::default()
    };
    let report = compare(&ours, &theirs, &options).unwrap();
    let picture = line(&report, "picture");
    assert!(picture.detail.starts_with("average 99."), "{}", picture.detail);
    assert_eq!(picture.verdict, Verdict::Fail);
    assert_eq!(line(&report, "scene short (frames 0-2)").verdict, Verdict::Fail);
    assert_eq!(line(&report, "scene long (frames 3-29)").verdict, Verdict::Pass);
    assert!(line(&report, "scene short (frames 0-2)").detail.starts_with("average "));
    let json = &report.json()["picture"];
    assert_eq!(json["scenes"].as_array().unwrap().len(), 2);
    assert_eq!(json["passes"], false);
}

#[test]
fn a_low_frame_with_a_named_cause_is_shown_as_excused_and_never_as_passed() {
    let dir = folder("video_comparison_excused");
    let base = moving(24, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", (30, 1), &base);
    let theirs = write(&dir, "theirs.mp4", (30, 1), &with_block(&base, 9, 30));
    let without = compare(&ours, &theirs, &Options::default()).unwrap();
    assert_eq!(without.exit_code(), 1);
    let options = Options {
        excuses: vec![parse_excuse("9=the reference holds a different still on this frame").unwrap()],
        ..Options::default()
    };
    let report = compare(&ours, &theirs, &options).unwrap();
    let text = report.text();
    assert!(
        text.contains("excused (not passed): the reference holds a different still on this frame"),
        "{text}"
    );
    assert!(!text.contains("frame 9: 100"), "{text}");
    let listed = report.sections.iter().find(|(h, _)| h == "frames under 99 %").unwrap();
    assert_eq!(listed.1.len(), 1);
    assert!(listed.1[0].starts_with("frame 9: "), "{:?}", listed.1);
    // The score is the one measured, and the average still counts it.
    let picture = line(&report, "picture");
    assert!(picture.detail.contains("lowest frame 9 at "), "{}", picture.detail);
    assert!(picture.detail.ends_with("1 frame under 99 % (1 excused)"), "{}", picture.detail);
    let state = &report.json()["picture"]["frame_scores"][9];
    assert_eq!(state["state"], "excused");
    assert_eq!(state["cause"], "the reference holds a different still on this frame");
    assert!(state["score"].as_f64().unwrap() < 99.0);
}

#[test]
fn a_cause_for_a_frame_that_is_not_low_is_not_used_and_the_report_says_so() {
    let dir = folder("video_comparison_unused_cause");
    let base = moving(12, WIDTH, HEIGHT);
    let ours = write(&dir, "ours.mp4", (30, 1), &base);
    let theirs = dir.join("theirs.mp4");
    std::fs::copy(&ours, &theirs).unwrap();
    let options = Options {
        excuses: vec![parse_excuse("3-4=a cause nobody needs").unwrap()],
        ..Options::default()
    };
    let report = compare(&ours, &theirs, &options).unwrap();
    assert_eq!(report.overall(), Verdict::Pass);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("'a cause nobody needs' for frames 3-4 was not used")),
        "{:?}",
        report.notes
    );
}

#[test]
fn scenes_that_leave_a_frame_out_or_run_past_the_last_are_refused_with_the_frame_named() {
    let dir = folder("video_comparison_bad_scenes");
    let ours = write(&dir, "ours.mp4", (30, 1), &moving(12, WIDTH, HEIGHT));
    let theirs = dir.join("theirs.mp4");
    std::fs::copy(&ours, &theirs).unwrap();
    let with = |scenes: &[&str]| Options {
        scenes: scenes.iter().map(|s| parse_scene(s).unwrap()).collect(),
        ..Options::default()
    };
    let why = compare(&ours, &theirs, &with(&["a=0-4", "b=6-11"])).unwrap_err();
    assert_eq!(why, "frame 5 is in no scene; the scenes must cover every frame");
    let why = compare(&ours, &theirs, &with(&["a=0-6", "b=6-11"])).unwrap_err();
    assert_eq!(why, "frame 6 is in the scenes 'a' and 'b'");
    let why = compare(&ours, &theirs, &with(&["a=0-12"])).unwrap_err();
    assert!(
        why.starts_with("scene 'a' ends at frame 12, and the videos have 12 frames"),
        "{why}"
    );
}

#[test]
fn a_file_that_is_not_a_video_is_an_error_that_names_it_and_no_report_is_made() {
    let dir = folder("video_comparison_not_a_video");
    let ours = write(&dir, "ours.mp4", (30, 1), &moving(6, WIDTH, HEIGHT));
    let junk = dir.join("junk.mp4");
    std::fs::write(&junk, b"this is not a video").unwrap();
    let why = compare(&ours, &junk, &Options::default()).unwrap_err();
    assert!(!why.is_empty());
    let missing = dir.join("missing.mp4");
    let why = compare(&missing, &ours, &Options::default()).unwrap_err();
    assert!(why.contains("missing.mp4"), "{why}");
}
