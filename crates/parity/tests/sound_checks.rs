//! The four sound checks on made sounds with a known length, gain and shift
//! (`docs/dev/comparison.md`, section 12).

use measure::{made_sound, Sound, VideoFacts};
use parity::report::{Line, Verdict};
use parity::sound::judge;
use parity::Report;

const RATE: u32 = 48_000;

/// The facts of a picture track of one second: 30 pictures at 30 a second.
fn one_second() -> VideoFacts {
    VideoFacts {
        container: "mp4".to_string(),
        codec: "h264".to_string(),
        width: 160,
        height: 90,
        frames: 30,
        rate: (30, 1),
        ticks: 30_000,
        timescale: 30_000,
    }
}

fn gained(sound: &Sound, db: f32) -> Sound {
    Sound {
        samples: sound.samples.iter().map(|v| v * 10f32.powf(db / 20.0)).collect(),
        ..sound.clone()
    }
}

fn judged(ours: Option<&Sound>, theirs: Option<&Sound>) -> Report {
    let mut report = Report::new("sound");
    judge(&mut report, ours, theirs, &one_second());
    report
}

fn line<'a>(report: &'a Report, name: &str) -> &'a Line {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no line {name}:\n{}", report.text()))
}

#[test]
fn a_sound_as_long_as_its_picture_passes_the_length_check() {
    let sound = made_sound(RATE, 2, 48_000, 0.5, 0);
    let report = judged(Some(&sound), Some(&sound));
    let length = line(&report, "sound length");
    assert_eq!(length.verdict, Verdict::Pass, "{}", report.text());
    assert_eq!(
        length.detail,
        "ours: sound 48000 sample frames, picture 48000 sample frames (1.000000 s at 48000 Hz)"
    );
}

#[test]
fn a_sound_one_sample_short_fails_the_length_check_with_both_lengths() {
    let short = made_sound(RATE, 2, 47_999, 0.5, 0);
    let reference = made_sound(RATE, 2, 48_000, 0.5, 0);
    let report = judged(Some(&short), Some(&reference));
    let length = line(&report, "sound length");
    assert_eq!(length.verdict, Verdict::Fail, "{}", report.text());
    assert_eq!(
        length.detail,
        "ours: sound 47999 sample frames, picture 48000 sample frames (1.000000 s at 48000 Hz)"
    );
    // One sample long fails as well.
    let long = made_sound(RATE, 2, 48_001, 0.5, 0);
    assert_eq!(line(&judged(Some(&long), Some(&reference)), "sound length").verdict, Verdict::Fail);
}

#[test]
fn a_picture_that_is_not_a_whole_number_of_samples_long_fails_the_length_check() {
    let sound = made_sound(44_100, 2, 44_100, 0.5, 0);
    let mut picture = one_second();
    // 1001/30000 s at 44100 Hz is 1471.47 sample frames.
    picture.ticks = 1001;
    let mut report = Report::new("sound");
    judge(&mut report, Some(&sound), Some(&sound), &picture);
    let length = line(&report, "sound length");
    assert_eq!(length.verdict, Verdict::Fail);
    assert!(length.detail.contains("picture 1471.470 sample frames"), "{}", length.detail);
}

#[test]
fn loudness_0_4_lu_apart_passes_and_0_6_lu_apart_fails_with_both_values() {
    let reference = made_sound(RATE, 2, 96_000, 0.5, 0);
    let near = judged(Some(&gained(&reference, -0.4)), Some(&reference));
    let loudness = line(&near, "loudness");
    assert_eq!(loudness.verdict, Verdict::Pass, "{}", near.text());
    assert!(loudness.detail.ends_with("; 0.40 LU apart (limit 0.5 LU)"), "{}", loudness.detail);
    let far = judged(Some(&gained(&reference, -0.6)), Some(&reference));
    let loudness = line(&far, "loudness");
    assert_eq!(loudness.verdict, Verdict::Fail, "{}", far.text());
    let theirs = parity::sound::loudness(&reference).unwrap();
    let ours = parity::sound::loudness(&gained(&reference, -0.6)).unwrap();
    assert_eq!(
        loudness.detail,
        format!("ours {ours:.2} LUFS, reference {theirs:.2} LUFS; 0.60 LU apart (limit 0.5 LU)")
    );
}

/// The made sound at a low level with one sample raised to `spike`: the peak is the spike, and the
/// loudness hardly moves.
fn spiked(spike: f32) -> Sound {
    let mut sound = made_sound(RATE, 2, 48_000, 0.2, 0);
    sound.samples[2 * 20_000] = spike;
    sound
}

#[test]
fn peaks_0_4_db_apart_pass_and_0_6_db_apart_fail_with_both_values() {
    let reference = spiked(0.5);
    let near = judged(Some(&spiked(0.5 * 10f32.powf(0.4 / 20.0))), Some(&reference));
    let peak = line(&near, "peak");
    assert_eq!(peak.verdict, Verdict::Pass, "{}", near.text());
    assert_eq!(peak.detail, "ours -5.62 dBFS, reference -6.02 dBFS; 0.40 dB apart (limit 0.5 dB)");
    assert_eq!(line(&near, "loudness").verdict, Verdict::Pass);
    let far = judged(Some(&spiked(0.5 * 10f32.powf(0.6 / 20.0))), Some(&reference));
    let peak = line(&far, "peak");
    assert_eq!(peak.verdict, Verdict::Fail, "{}", far.text());
    assert_eq!(peak.detail, "ours -5.42 dBFS, reference -6.02 dBFS; 0.60 dB apart (limit 0.5 dB)");
}

#[test]
fn a_shift_of_half_a_millisecond_passes_and_two_milliseconds_fail_with_the_measured_shift() {
    let reference = made_sound(RATE, 2, 48_000, 0.5, 0);
    // 24 sample frames at 48 kHz are 0.5 ms; 96 are 2 ms.
    let near = judged(Some(&made_sound(RATE, 2, 48_000, 0.5, 24)), Some(&reference));
    let alignment = line(&near, "alignment");
    assert_eq!(alignment.verdict, Verdict::Pass, "{}", near.text());
    assert_eq!(
        alignment.detail,
        "measured shift +0.500 ms (+24 sample frames at 48000 Hz): ours is later than the reference; limit 1 ms"
    );
    let far = judged(Some(&made_sound(RATE, 2, 48_000, 0.5, 96)), Some(&reference));
    let alignment = line(&far, "alignment");
    assert_eq!(alignment.verdict, Verdict::Fail, "{}", far.text());
    assert_eq!(
        alignment.detail,
        "measured shift +2.000 ms (+96 sample frames at 48000 Hz): ours is later than the reference; limit 1 ms"
    );
    // The other way round: the reference is the later one.
    let earlier = judged(Some(&reference), Some(&made_sound(RATE, 2, 48_000, 0.5, 96)));
    assert!(line(&earlier, "alignment")
        .detail
        .starts_with("measured shift -2.000 ms (-96 sample frames"));
    let same = judged(Some(&reference), Some(&reference));
    assert!(line(&same, "alignment")
        .detail
        .starts_with("measured shift +0.000 ms (+0 sample frames"));
    assert_eq!(line(&same, "alignment").verdict, Verdict::Pass);
}

#[test]
fn a_shift_at_the_edge_of_the_search_fails_and_says_it_may_be_larger() {
    let reference = made_sound(RATE, 2, 48_000, 0.5, 0);
    // 960 sample frames are the 20 ms of the search.
    let report = judged(Some(&made_sound(RATE, 2, 48_000, 0.5, 960)), Some(&reference));
    let alignment = line(&report, "alignment");
    assert_eq!(alignment.verdict, Verdict::Fail);
    assert!(alignment.detail.contains("the shift may be larger"), "{}", alignment.detail);
}

#[test]
fn renders_with_no_sound_report_the_level_and_alignment_checks_as_not_applicable() {
    let report = judged(None, None);
    for name in ["loudness", "peak", "alignment", "sound length"] {
        assert_eq!(line(&report, name).verdict, Verdict::NotApplicable, "{}", report.text());
    }
    assert_eq!(line(&report, "loudness").detail, "neither render has sound");
    assert_eq!(
        report.overall(),
        Verdict::NotRun,
        "no judged check here: the structural ones are added by compare"
    );
}

#[test]
fn sound_in_one_render_only_fails_the_level_and_alignment_checks() {
    let sound = made_sound(RATE, 2, 48_000, 0.5, 0);
    let report = judged(None, Some(&sound));
    for name in ["sound length", "loudness", "peak", "alignment"] {
        assert_eq!(line(&report, name).verdict, Verdict::Fail, "{}", report.text());
    }
    let report = judged(Some(&sound), None);
    assert_eq!(line(&report, "sound length").verdict, Verdict::Pass);
    assert_eq!(line(&report, "loudness").verdict, Verdict::Fail);
}

#[test]
fn two_silent_sounds_pass_the_levels_and_give_nothing_to_align() {
    let silent = Sound {
        rate: RATE,
        channels: 2,
        samples: vec![0.0; 96_000],
    };
    let report = judged(Some(&silent), Some(&silent));
    assert_eq!(line(&report, "loudness").verdict, Verdict::Pass);
    assert_eq!(line(&report, "peak").verdict, Verdict::Pass);
    assert_eq!(line(&report, "alignment").verdict, Verdict::NotRun);
}
