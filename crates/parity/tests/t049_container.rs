//! T049: container facts compared with the reference, by stream type.

mod common;

use common::{folder, make, pair, tools, Audio, Spec};
use parity::container;
use parity::report::Verdict;

fn verdict(report: &parity::Report, name: &str) -> Verdict {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name}"))
        .verdict
}

fn detail(report: &parity::Report, name: &str) -> String {
    report.checks.iter().find(|c| c.name == name).unwrap().detail.clone()
}

const FIVE: [&str; 5] = ["frame count", "frame rate", "size", "codec", "pixel format"];

#[test]
fn t049_c1_the_five_facts_are_reported_separately_and_pass_on_equal_videos() {
    let dir = folder("t049_c1_equal");
    let (a, b) = pair(&dir, &Spec::default(), &Spec::default());
    let report = container::compare_files(&tools().ffprobe, &a, &b).unwrap();
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, FIVE);
    for name in FIVE {
        assert_eq!(verdict(&report, name), Verdict::Pass, "{name}");
    }
    assert_eq!(report.overall(), Verdict::Pass);
    assert_eq!(report.exit_code(), 0);
    assert!(report.text().contains("result: passed"));
}

#[test]
fn t049_c1_each_fact_fails_on_its_own_and_names_both_values() {
    let dir = folder("t049_c1_each");
    let (a, b) = (dir.join("a.mp4"), dir.join("b.mp4"));
    make(&a, &Spec::default());
    // A different number of frames at the same rate fails only the frame count.
    make(
        &b,
        &Spec {
            frames: 45,
            clip: (20, 40),
            mover: (10, 40, 20, 180),
            ..Spec::default()
        },
    );
    let report = container::compare_files(&tools().ffprobe, &a, &b).unwrap();
    assert_eq!(verdict(&report, "frame count"), Verdict::Fail);
    assert_eq!(detail(&report, "frame count"), "ours 60, reference 45");
    for name in ["frame rate", "size", "codec", "pixel format"] {
        assert_eq!(verdict(&report, name), Verdict::Pass, "{name}");
    }
    assert_eq!(report.overall(), Verdict::Fail);
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn t049_c1_codec_size_and_pixel_format_failures_are_separate() {
    let dir = folder("t049_c1_codec");
    let a = dir.join("a.mp4");
    make(&a, &Spec::default());
    // The same picture re-encoded: other codec, other pixel format and another size.
    let other = |name: &str, args: &[&str]| {
        let path = dir.join(name);
        let out = std::process::Command::new(&tools().ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(&a)
            .args(args)
            .arg(&path)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        path
    };
    let codec = other("codec.mp4", &["-c:v", "mpeg4", "-pix_fmt", "yuv420p"]);
    let r = container::compare_files(&tools().ffprobe, &a, &codec).unwrap();
    assert_eq!(verdict(&r, "codec"), Verdict::Fail);
    assert!(detail(&r, "codec").contains("h264") && detail(&r, "codec").contains("mpeg4"));
    assert_eq!(verdict(&r, "pixel format"), Verdict::Pass);
    let pix = other("pix.mp4", &["-c:v", "libx264", "-pix_fmt", "yuv444p"]);
    let r = container::compare_files(&tools().ffprobe, &a, &pix).unwrap();
    assert_eq!(verdict(&r, "pixel format"), Verdict::Fail);
    assert!(detail(&r, "pixel format").contains("yuv420p") && detail(&r, "pixel format").contains("yuv444p"));
    assert_eq!(verdict(&r, "codec"), Verdict::Pass);
    let size = other("size.mp4", &["-vf", "scale=160:90", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
    let r = container::compare_files(&tools().ffprobe, &a, &size).unwrap();
    assert_eq!(verdict(&r, "size"), Verdict::Fail);
    assert_eq!(detail(&r, "size"), "ours 320x180, reference 160x90");
    assert_eq!(verdict(&r, "codec"), Verdict::Pass);
}

#[test]
fn t049_c2_an_audio_stream_on_one_side_does_not_change_the_video_checks() {
    let dir = folder("t049_c2_audio");
    let (a, b) = pair(
        &dir,
        &Spec::default(),
        &Spec {
            audio: Audio::AfterVideo,
            ..Spec::default()
        },
    );
    let report = container::compare_files(&tools().ffprobe, &a, &b).unwrap();
    for name in FIVE {
        assert_eq!(verdict(&report, name), Verdict::Pass, "{name}");
    }
    assert_eq!(report.overall(), Verdict::Pass);
    let listed = report.not_compared.join("\n");
    assert!(listed.contains("not compared in this sprint"), "{listed}");
    assert!(
        listed.contains("ours has 0 audio") && listed.contains("reference has 1"),
        "{listed}"
    );
    assert!(report.text().contains("not compared in this sprint"));
}

#[test]
fn t049_c2_the_video_stream_is_found_by_type_not_by_position() {
    let dir = folder("t049_c2_position");
    // Audio comes first in the second file: stream 0 is audio, stream 1 is video.
    let (a, b) = pair(
        &dir,
        &Spec::default(),
        &Spec {
            audio: Audio::BeforeVideo,
            ..Spec::default()
        },
    );
    let probe = std::process::Command::new(&tools().ffprobe)
        .args(["-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0"])
        .arg(&b)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&probe.stdout).lines().next(),
        Some("audio"),
        "the test file must have audio first"
    );
    let report = container::compare_files(&tools().ffprobe, &a, &b).unwrap();
    for name in FIVE {
        assert_eq!(verdict(&report, name), Verdict::Pass, "{name}: {}", report.text());
    }
}

#[test]
fn t049_c3_a_colour_tag_difference_is_information_and_does_not_fail() {
    let dir = folder("t049_c3_tags");
    let (a, b) = pair(
        &dir,
        &Spec::default(),
        &Spec {
            other_tags: true,
            ..Spec::default()
        },
    );
    let report = container::compare_files(&tools().ffprobe, &a, &b).unwrap();
    for name in FIVE {
        assert_eq!(verdict(&report, name), Verdict::Pass, "{name}");
    }
    assert_eq!(report.overall(), Verdict::Pass);
    assert_eq!(report.exit_code(), 0);
    let info = report.info.join("\n");
    assert!(info.contains("color_space: ours bt709, reference bt470bg"), "{info}");
    let text = report.text();
    assert!(text.contains("information only (never fails the comparison)"));
    assert!(text.contains("result: passed"));
}

#[test]
fn t049_c4_a_different_frame_rate_fails_and_names_both_values() {
    let dir = folder("t049_c4_rate");
    // 2.5 seconds at 24 and 30 frames per second would differ in count; keep 60 frames in both
    // so that only the rate differs.
    let (a, b) = pair(
        &dir,
        &Spec::default(),
        &Spec {
            fps: 24,
            frames: 60,
            clip: (20, 40),
            mover: (10, 50, 20, 180),
            ..Spec::default()
        },
    );
    let report = container::compare_files(&tools().ffprobe, &a, &b).unwrap();
    assert_eq!(verdict(&report, "frame rate"), Verdict::Fail);
    let said = detail(&report, "frame rate");
    assert!(said.contains("30/1") && said.contains("24/1"), "{said}");
    assert_eq!(verdict(&report, "frame count"), Verdict::Pass);
    assert_eq!(report.overall(), Verdict::Fail);
    assert_eq!(report.exit_code(), 1);
    assert!(report.text().contains("result: failed"));
}

#[test]
fn t049_equal_rates_written_differently_are_the_same_rate() {
    assert!(container::same_rate("30/1", "30000/1000"));
    assert!(container::same_rate("30", "30/1"));
    assert!(!container::same_rate("30000/1001", "30/1"));
    assert!(!container::same_rate("30/1", "24/1"));
}
