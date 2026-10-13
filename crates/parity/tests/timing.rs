//! When a clip appears and disappears, and where a moving element is on every frame.

mod common;

use common::{folder, pair, tools, Spec, BACKGROUND, CLIP, MOVER};
use parity::report::Verdict;
use parity::timing::{self, Options};

fn options() -> Options {
    Options {
        clip: Some(CLIP),
        mover: Some(MOVER),
        background: BACKGROUND,
        ..Options::default()
    }
}

fn line<'a>(report: &'a parity::Report, name: &str) -> &'a parity::Line {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name}: {}", report.text()))
}

fn late(by: u32) -> Spec {
    Spec {
        clip: (20 + by, 40 + by),
        ..Spec::default()
    }
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_the_appearance_and_disappearance_frames_are_reported_for_both_videos() {
    let dir = folder("timing_equal");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    let appears = line(&report, "clip appears");
    assert_eq!(appears.verdict, Verdict::Pass);
    assert!(appears.detail.contains("ours frame 20, reference frame 20"), "{}", appears.detail);
    let gone = line(&report, "clip disappears");
    assert_eq!(gone.verdict, Verdict::Pass);
    assert!(gone.detail.contains("ours frame 40, reference frame 40"), "{}", gone.detail);
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_one_frame_apart_passes_and_two_frames_apart_fails() {
    let dir = folder("timing_limit");
    let (a, b) = pair(&dir, &late(1), &Spec::default());
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    assert_eq!(line(&report, "clip appears").verdict, Verdict::Pass, "{}", report.text());
    assert_eq!(line(&report, "clip disappears").verdict, Verdict::Pass);
    assert!(line(&report, "clip appears").detail.contains("1 frame(s) apart"));
    let dir = folder("timing_limit_two");
    let (a, b) = pair(&dir, &late(2), &Spec::default());
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    assert_eq!(line(&report, "clip appears").verdict, Verdict::Fail);
    assert_eq!(line(&report, "clip disappears").verdict, Verdict::Fail);
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_a_clip_three_frames_late_fails_and_names_the_event_and_both_frames() {
    let dir = folder("timing_late");
    // Ours starts three frames late and ends where it should.
    let ours = Spec {
        clip: (23, 40),
        ..Spec::default()
    };
    let (a, b) = pair(&dir, &ours, &Spec::default());
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    let appears = line(&report, "clip appears");
    assert_eq!(appears.verdict, Verdict::Fail);
    assert!(appears.detail.contains("ours frame 23, reference frame 20"), "{}", appears.detail);
    assert!(appears.detail.contains("3 frame(s) apart (limit 1)"), "{}", appears.detail);
    assert_eq!(line(&report, "clip disappears").verdict, Verdict::Pass);
    assert_eq!(report.overall(), Verdict::Fail);
    assert_eq!(report.exit_code(), 1);
    assert!(report.text().contains("clip appears") && report.text().contains("result: failed"));
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_the_tracked_position_passes_within_one_pixel_and_reports_the_measured_value() {
    let dir = folder("timing_position");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    let position = line(&report, "position");
    assert_eq!(position.verdict, Verdict::Pass, "{}", position.detail);
    assert!(position.detail.contains("measured: largest difference 0.0"), "{}", position.detail);
    assert!(position.detail.contains("limit 1 px"), "{}", position.detail);
    assert!(position.detail.contains("60 frames compared"), "{}", position.detail);
    assert_eq!(report.exit_code(), 0);
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_an_element_five_pixels_off_at_the_end_fails_and_names_the_frame() {
    let dir = folder("timing_off");
    let ours = Spec {
        mover: (10, 50, 20, 185),
        ..Spec::default()
    };
    let (a, b) = pair(&dir, &ours, &Spec::default());
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    let position = line(&report, "position");
    assert_eq!(position.verdict, Verdict::Fail, "{}", position.detail);
    // The gap grows during the move (frames 10 to 50) and stays at five pixels once it ends; the frames
    // from 46 on all show a gap of five pixels, whole-pixel positions being what the test video holds.
    let named: usize = position
        .detail
        .split("at frame ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!((46..60).contains(&named), "{}", position.detail);
    let measured: f64 = position
        .detail
        .split("largest difference ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!((4.8..5.3).contains(&measured), "{}", position.detail);
    assert!(position.detail.contains("limit 1 px"), "{}", position.detail);
    // The timing of the clip is unaffected.
    assert_eq!(line(&report, "clip appears").verdict, Verdict::Pass);
    assert_eq!(report.exit_code(), 1);
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_a_failure_gives_the_measured_value_and_the_threshold() {
    let dir = folder("timing_report");
    let ours = Spec {
        clip: (23, 40),
        mover: (10, 50, 20, 185),
        ..Spec::default()
    };
    let (a, b) = pair(&dir, &ours, &Spec::default());
    let text = timing::compare_files(&tools(), &a, &b, &options()).unwrap().text();
    assert!(text.contains("3 frame(s) apart (limit 1)"), "{text}");
    assert!(
        text.contains("measured: largest difference") && text.contains("(limit 1 px)"),
        "{text}"
    );
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_a_clip_missing_from_one_video_fails() {
    let dir = folder("timing_missing");
    let nothing = Spec {
        clip: (1000, 1001),
        ..Spec::default()
    };
    let (a, b) = pair(&dir, &nothing, &Spec::default());
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    let appears = line(&report, "clip appears");
    assert_eq!(appears.verdict, Verdict::Fail);
    assert!(appears.detail.contains("ours never, reference frame 20"), "{}", appears.detail);
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_a_clip_that_stays_to_the_end_in_both_videos_has_no_disappearance_to_compare() {
    let dir = folder("timing_stays");
    let stays = Spec {
        clip: (20, 60),
        ..Spec::default()
    };
    let (a, b) = pair(&dir, &stays, &stays);
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    assert_eq!(line(&report, "clip disappears").verdict, Verdict::Pass);
    assert!(line(&report, "clip disappears").detail.contains("still there on the last frame"));
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_different_frame_counts_are_noted_and_only_shared_frames_are_compared() {
    let dir = folder("timing_counts");
    let shorter = Spec {
        frames: 45,
        mover: (10, 40, 20, 180),
        ..Spec::default()
    };
    let (a, b) = pair(&dir, &Spec::default(), &shorter);
    let report = timing::compare_files(&tools(), &a, &b, &options()).unwrap();
    assert!(report.notes.iter().any(|n| n.contains("frame counts differ")), "{}", report.text());
}

#[test]
fn the_centre_of_a_flat_element_is_found_to_a_fraction_of_a_pixel() {
    let mut frame = measure::Frame {
        width: 100,
        height: 100,
        rgb: BACKGROUND.iter().copied().cycle().take(100 * 100 * 3).collect(),
    };
    let mut paint = |x: u32, y: u32, colour: [u8; 3]| {
        let at = ((y * 100 + x) * 3) as usize;
        frame.rgb[at..at + 3].copy_from_slice(&colour);
    };
    for y in 40..60 {
        for x in 10..30 {
            paint(x, y, MOVER);
        }
    }
    let found = timing::centre(&frame, BACKGROUND, MOVER, 24, 100).unwrap();
    assert!((found.0 - 20.0).abs() < 1e-9 && (found.1 - 50.0).abs() < 1e-9, "{found:?}");
    assert!(
        timing::centre(&frame, BACKGROUND, CLIP, 24, 100).is_none(),
        "another colour is not found"
    );
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_asking_for_nothing_to_measure_is_a_mistake() {
    let dir = folder("timing_nothing");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    let asked = Options {
        background: BACKGROUND,
        ..Options::default()
    };
    assert!(timing::compare_files(&tools(), &a, &b, &asked).is_err());
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_a_region_limits_where_the_element_is_looked_for_and_positions_stay_those_of_the_whole_picture() {
    let dir = folder("timing_region");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    let whole = Options {
        mover: Some(MOVER),
        background: BACKGROUND,
        ..Options::default()
    };
    // The mover's row only: the same centres as with no region.
    let row = Options {
        region: Some((0, 110, 320, 50)),
        ..whole.clone()
    };
    let tools = tools();
    let everywhere = timing::track(&tools, &a, &whole).unwrap();
    let in_row = timing::track(&tools, &a, &row).unwrap();
    assert_eq!(everywhere.centres.len(), in_row.centres.len());
    for (frame, (whole, part)) in everywhere.centres.iter().zip(&in_row.centres).enumerate() {
        let (whole, part) = (whole.unwrap(), part.unwrap());
        assert!(
            (whole.0 - part.0).abs() < 1e-6 && (whole.1 - part.1).abs() < 1e-6,
            "frame {frame}: {whole:?} {part:?}"
        );
    }
    assert!(
        in_row.centres[0].is_some_and(|(_, y)| (y - 132.0).abs() < 0.5),
        "{:?}",
        in_row.centres[0]
    );
    let report = timing::compare_files(&tools, &a, &b, &row).unwrap();
    assert_eq!(line(&report, "position").verdict, Verdict::Pass, "{}", report.text());
    // A region the mover is never in: nothing is found.
    let above = Options {
        region: Some((0, 0, 320, 100)),
        ..whole.clone()
    };
    let report = timing::compare_files(&tools, &a, &b, &above).unwrap();
    let position = line(&report, "position");
    assert_eq!(position.verdict, Verdict::Fail);
    assert!(position.detail.contains("not found on any frame"), "{}", position.detail);
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_in_a_region_every_colour_may_count_and_a_five_pixel_gap_is_still_measured() {
    let dir = folder("timing_region_wide");
    let ours = Spec {
        mover: (10, 50, 20, 185),
        ..Spec::default()
    };
    let (a, b) = pair(&dir, &ours, &Spec::default());
    // The widest tolerance makes the whole region the box that is weighed.
    let asked = Options {
        mover: Some(MOVER),
        background: BACKGROUND,
        tolerance: 255,
        region: Some((0, 110, 320, 50)),
        ..Options::default()
    };
    let report = timing::compare_files(&tools(), &a, &b, &asked).unwrap();
    let position = line(&report, "position");
    assert_eq!(position.verdict, Verdict::Fail, "{}", position.detail);
    let measured: f64 = position
        .detail
        .split("largest difference ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!((4.8..5.3).contains(&measured), "{}", position.detail);
}

#[test]
#[ignore = "yardstick: needs ffmpeg and ffprobe, which it starts from outside"]
fn yardstick_a_region_outside_the_picture_is_a_mistake() {
    let dir = folder("timing_region_outside");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    let asked = Options {
        mover: Some(MOVER),
        background: BACKGROUND,
        region: Some((300, 0, 40, 40)),
        ..Options::default()
    };
    let why = timing::compare_files(&tools(), &a, &b, &asked).unwrap_err();
    assert!(why.contains("is not inside the 320x180 picture"), "{why}");
}
