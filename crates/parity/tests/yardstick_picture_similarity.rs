//! Similarity at sampled frames and the side-by-side sheet. Yardstick tests: they make their videos
//! with ffmpeg and read them with ffprobe, started from outside (`docs/dev/gates.md`, section 5).

mod common;
mod yardstick_support;

use common::folder;
use parity::pictures::{self, Options};
use parity::report::Verdict;
use parity::sheet;
use yardstick_support::{pair, tools, Spec};

fn spec_shifted(range: (u32, u32), dx: i32) -> Spec {
    Spec {
        box_shift: Some((range, dx)),
        ..Spec::default()
    }
}

fn yardstick_every_sampled_frame_gets_a_value_and_it_passes_only_at_or_above_the_threshold() {
    let dir = folder("pictures_equal");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    let report = pictures::compare(&tools(), &a, &b, &Options::default()).unwrap();
    let frames: Vec<&parity::Line> = report.checks.iter().filter(|c| c.name.starts_with("frame ")).collect();
    assert_eq!(frames.len(), 8, "{}", report.text());
    for line in &frames {
        assert!(line.detail.starts_with("ssim "), "{}", line.detail);
        assert!(line.detail.contains("threshold 0.97"), "{}", line.detail);
        assert_eq!(line.verdict, Verdict::Pass);
    }
    assert_eq!(report.overall(), Verdict::Pass);
    assert_eq!(report.exit_code(), 0);
}

fn yardstick_a_threshold_above_the_value_fails_the_check() {
    let dir = folder("pictures_threshold");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    // Identical pictures give 1.0 at best, so a threshold above 1 can never be met.
    let options = Options {
        threshold: 1.01,
        ..Options::default()
    };
    let report = pictures::compare(&tools(), &a, &b, &options).unwrap();
    assert_eq!(report.overall(), Verdict::Fail);
}

fn yardstick_a_shifted_element_fails_and_names_the_lowest_scoring_frame() {
    let dir = folder("pictures_shift");
    // The still box is moved 20 pixels, but only on frames 40 to 48: of the sampled frames only 42 is inside.
    let (a, b) = pair(&dir, &Spec::default(), &spec_shifted((40, 48), 20));
    let report = pictures::compare(&tools(), &a, &b, &Options::default()).unwrap();
    assert_eq!(report.overall(), Verdict::Fail, "{}", report.text());
    let summary = &report.checks.last().unwrap();
    assert_eq!(summary.name, "pictures");
    assert_eq!(summary.verdict, Verdict::Fail);
    assert!(summary.detail.contains("lowest-scoring frame is 42"), "{}", summary.detail);
    let failing: Vec<&str> = report
        .checks
        .iter()
        .filter(|c| c.name.starts_with("frame ") && c.verdict == Verdict::Fail)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(failing, vec!["frame 42"]);
    assert_eq!(report.exit_code(), 1);
}

fn yardstick_different_frame_counts_cannot_be_paired_and_never_pass() {
    let dir = folder("pictures_counts");
    let (a, b) = pair(
        &dir,
        &Spec::default(),
        &Spec {
            frames: 45,
            mover: (10, 40, 20, 180),
            ..Spec::default()
        },
    );
    let report = pictures::compare(&tools(), &a, &b, &Options::default()).unwrap();
    let text = report.text();
    assert!(
        text.contains("frames cannot be paired: ours has 60 frames, reference has 45"),
        "{text}"
    );
    assert_ne!(report.overall(), Verdict::Pass);
    assert!(!text.contains("result: passed"));
    assert_eq!(report.exit_code(), 1);
}

fn yardstick_different_sizes_cannot_be_paired_either() {
    let dir = folder("pictures_sizes");
    let (a, _) = pair(&dir, &Spec::default(), &Spec::default());
    let small = dir.join("small.mp4");
    let out = std::process::Command::new(&tools().ffmpeg)
        .args(["-v", "error", "-y", "-i"])
        .arg(&a)
        .args(["-vf", "scale=160:90", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&small)
        .output()
        .unwrap();
    assert!(out.status.success());
    let report = pictures::compare(&tools(), &a, &small, &Options::default()).unwrap();
    assert!(report.text().contains("frames cannot be paired"));
    assert_eq!(report.overall(), Verdict::Fail);
}

fn yardstick_one_sheet_shows_every_sampled_frame_from_both_videos_with_its_number() {
    let dir = folder("pictures_sheet");
    let (a, b) = pair(&dir, &Spec::default(), &spec_shifted((40, 48), 20));
    let path = dir.join("nested").join("sheet.png");
    let options = Options {
        samples: 6,
        sheet: Some(path.clone()),
        ..Options::default()
    };
    let report = pictures::compare(&tools(), &a, &b, &options).unwrap();
    assert!(report.notes.iter().any(|n| n.contains("sheet.png")));
    let bytes = std::fs::read(&path).expect("the sheet exists, in a folder that was made for it");
    let (width, height) = measure::png_size(&bytes).expect("the sheet is a PNG file");
    // Six pairs in two columns: three rows of cells under a legend row.
    let thumb_height = (180 * sheet::THUMB_WIDTH).div_ceil(320);
    let expected = sheet::layout(6, thumb_height);
    let (spots, (expected_width, expected_height)) = (expected.spots, expected.size);
    assert_eq!((width, height), (expected_width, expected_height));
    assert_eq!(spots.len(), 6);
    let sheets: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
    assert_eq!(sheets.len(), 1, "exactly one sheet is written");
}

measure::yardstick_tests!(
    measure::yardstick::Needs::ffmpeg();
    yardstick_every_sampled_frame_gets_a_value_and_it_passes_only_at_or_above_the_threshold,
    yardstick_a_threshold_above_the_value_fails_the_check,
    yardstick_a_shifted_element_fails_and_names_the_lowest_scoring_frame,
    yardstick_different_frame_counts_cannot_be_paired_and_never_pass,
    yardstick_different_sizes_cannot_be_paired_either,
    yardstick_one_sheet_shows_every_sampled_frame_from_both_videos_with_its_number,
);
