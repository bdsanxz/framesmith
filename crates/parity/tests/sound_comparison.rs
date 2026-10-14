//! The sound checks of `parity compare` on MP4 files whose sound is written as AAC through the
//! system's encoder (`measure::write_video_with_sound`) and read back with frx7
//! (`docs/dev/comparison.md`, section 12).

mod common;

use common::folder;
use measure::{made_sound, Frame, Sound};
use parity::compare::{compare, Options};
use parity::report::{Line, Verdict};
use parity::Report;
use std::path::{Path, PathBuf};

const RATE: u32 = 48_000;

fn pictures() -> Vec<Frame> {
    measure::moving_pictures(30, 160, 90)
}

fn gained(sound: &Sound, db: f32) -> Sound {
    Sound {
        samples: sound.samples.iter().map(|v| v * 10f32.powf(db / 20.0)).collect(),
        ..sound.clone()
    }
}

fn written(dir: &Path, name: &str, sound: Option<&Sound>) -> PathBuf {
    let path = dir.join(format!("{name}.mp4"));
    measure::write_video_with_sound(&path, (30, 1), &pictures(), sound).unwrap();
    path
}

fn compared(ours: &Path, theirs: &Path) -> Report {
    compare(ours, theirs, &Options::default()).unwrap()
}

fn line<'a>(report: &'a Report, name: &str) -> &'a Line {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no line {name}:\n{}", report.text()))
}

#[test]
fn the_sound_written_to_a_file_is_read_back_to_the_sample() {
    let dir = folder("sound-read-back");
    for frames in [48_000, 47_999] {
        let sound = made_sound(RATE, 2, frames, 0.5, 0);
        let read = measure::read_sound(&written(&dir, &format!("f{frames}"), Some(&sound)))
            .unwrap()
            .unwrap();
        assert_eq!((read.rate, read.channels, read.frames()), (RATE, 2, frames as u64));
    }
    assert_eq!(measure::read_sound(&written(&dir, "none", None)).unwrap(), None);
}

#[test]
fn a_render_whose_sound_is_one_sample_short_fails_the_length_check_and_a_whole_one_passes() {
    let dir = folder("sound-length");
    let whole = written(&dir, "whole", Some(&made_sound(RATE, 2, 48_000, 0.5, 0)));
    let short = written(&dir, "short", Some(&made_sound(RATE, 2, 47_999, 0.5, 0)));
    let report = compared(&whole, &whole);
    assert_eq!(line(&report, "sound length").verdict, Verdict::Pass, "{}", report.text());
    assert_eq!(report.exit_code(), 0, "{}", report.text());
    let report = compared(&short, &whole);
    let length = line(&report, "sound length");
    assert_eq!(length.verdict, Verdict::Fail, "{}", report.text());
    assert_eq!(
        length.detail,
        "ours: sound 47999 sample frames, picture 48000 sample frames (1.000000 s at 48000 Hz)"
    );
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn files_0_4_lu_and_db_apart_pass_and_0_6_apart_fail() {
    let dir = folder("sound-levels");
    let base = made_sound(RATE, 2, 48_000, 0.5, 0);
    let reference = written(&dir, "reference", Some(&base));
    let near = written(&dir, "near", Some(&gained(&base, -0.4)));
    let far = written(&dir, "far", Some(&gained(&base, 0.6)));
    let report = compared(&near, &reference);
    for name in ["loudness", "peak"] {
        assert_eq!(line(&report, name).verdict, Verdict::Pass, "{}", report.text());
    }
    assert!(line(&report, "loudness").detail.contains("0.40 LU apart"), "{}", report.text());
    assert!(line(&report, "peak").detail.contains("0.40 dB apart"), "{}", report.text());
    let report = compared(&far, &reference);
    for name in ["loudness", "peak"] {
        assert_eq!(line(&report, name).verdict, Verdict::Fail, "{}", report.text());
    }
    assert!(line(&report, "loudness").detail.contains("0.60 LU apart"), "{}", report.text());
    assert!(line(&report, "peak").detail.contains("0.60 dB apart"), "{}", report.text());
}

#[test]
fn files_shifted_half_a_millisecond_pass_and_two_milliseconds_fail_with_the_shift() {
    let dir = folder("sound-shift");
    let reference = written(&dir, "reference", Some(&made_sound(RATE, 2, 48_000, 0.5, 0)));
    let near = written(&dir, "near", Some(&made_sound(RATE, 2, 48_000, 0.5, 24)));
    let far = written(&dir, "far", Some(&made_sound(RATE, 2, 48_000, 0.5, 96)));
    let report = compared(&near, &reference);
    let alignment = line(&report, "alignment");
    assert_eq!(alignment.verdict, Verdict::Pass, "{}", report.text());
    assert!(
        alignment.detail.starts_with("measured shift +0.500 ms (+24 sample frames"),
        "{}",
        alignment.detail
    );
    let report = compared(&far, &reference);
    let alignment = line(&report, "alignment");
    assert_eq!(alignment.verdict, Verdict::Fail, "{}", report.text());
    assert!(
        alignment.detail.starts_with("measured shift +2.000 ms (+96 sample frames"),
        "{}",
        alignment.detail
    );
}

#[test]
fn files_with_no_sound_report_the_sound_checks_as_not_applicable_and_still_check_the_length() {
    let dir = folder("sound-none");
    let a = written(&dir, "a", None);
    let b = written(&dir, "b", None);
    let report = compared(&a, &b);
    for name in ["loudness", "peak", "alignment", "sound length"] {
        assert_eq!(line(&report, name).verdict, Verdict::NotApplicable, "{}", report.text());
    }
    assert_eq!(line(&report, "length").verdict, Verdict::Pass);
    assert_eq!(report.exit_code(), 0, "{}", report.text());
    // The length is still judged: a render one frame shorter fails it.
    let shorter = dir.join("shorter.mp4");
    measure::write_video(&shorter, (30, 1), &pictures()[..29]).unwrap();
    let report = compared(&shorter, &b);
    assert_eq!(line(&report, "length").verdict, Verdict::Fail, "{}", report.text());
    assert_eq!(line(&report, "loudness").verdict, Verdict::NotApplicable);
}

#[test]
fn files_with_silent_tracks_are_no_sound_and_a_silent_track_one_sample_short_fails_the_length() {
    let dir = folder("sound-silent-tracks");
    let silent = |frames: usize| Sound {
        rate: RATE,
        channels: 2,
        samples: vec![0.0; 2 * frames],
    };
    let a = written(&dir, "a", Some(&silent(48_000)));
    let b = written(&dir, "b", Some(&silent(48_000)));
    let short = written(&dir, "short", Some(&silent(47_999)));
    let report = compared(&a, &b);
    for name in ["loudness", "peak", "alignment"] {
        assert_eq!(line(&report, name).verdict, Verdict::NotApplicable, "{}", report.text());
    }
    assert_eq!(line(&report, "sound length").verdict, Verdict::Pass, "{}", report.text());
    assert_eq!(line(&report, "length").verdict, Verdict::Pass);
    assert_eq!(report.exit_code(), 0, "{}", report.text());
    let report = compared(&short, &b);
    let length = line(&report, "sound length");
    assert_eq!(length.verdict, Verdict::Fail, "{}", report.text());
    assert_eq!(
        length.detail,
        "ours: sound 47999 sample frames, picture 48000 sample frames (1.000000 s at 48000 Hz)"
    );
    assert_eq!(line(&report, "loudness").verdict, Verdict::NotApplicable);
    assert_eq!(report.exit_code(), 1);
}
