//! Sounds placed in the film by `framesmith render` (`docs/spec/sound.md`): each sound file is written by the test from
//! numbers of our own, the film is rendered on the processor, and its sound track is read back through frx7. The film's
//! sound is AAC, so a level is compared within the encoder's loss, and a place is found from where the sound is loud.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, project};

const RATE: u32 = 48_000;

/// Renders a 3-second, 30 fps film of `audio` (the `<audio>` elements, one a line from line 2) with the WAV files
/// `files` (a name and its stereo or mono samples, with their channel count) in the project folder. Returns the film's
/// sound as frames of two channels.
fn render(name: &str, audio: &str, files: &[(&str, u16, Vec<f32>)]) -> (PathBuf, measure::Sound) {
    let text = format!(
        "<composition version=\"1\" width=\"64\" height=\"64\" fps=\"30\" duration=\"3\" background=\"#000000\">\n{audio}\n</composition>\n"
    );
    let (project, cwd) = project("sound-placed", name, &text);
    for (file, channels, samples) in files {
        measure::write_wav(&project.join(file), RATE, *channels, samples).unwrap();
    }
    let output = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    let film = film(&project);
    let sound = measure::read_sound(&film).unwrap().expect("the film has a sound track");
    assert_eq!((sound.rate, sound.channels), (RATE, 2));
    (film, sound)
}

fn film(project: &Path) -> PathBuf {
    fs::read_dir(project.join("renders"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "mp4"))
        .expect("a film was written")
}

/// The loudest sample frame of `sound` (the larger of its two channels).
fn loudest(sound: &measure::Sound) -> (usize, f32) {
    let mut best = (0, 0.0f32);
    for (frame, pair) in sound.samples.chunks(2).enumerate() {
        let level = pair[0].abs().max(pair[1].abs());
        if level > best.1 {
            best = (frame, level);
        }
    }
    best
}

/// The largest level of `sound` from frame `from` up to `to`, in one channel (`channel`).
fn peak(sound: &measure::Sound, channel: usize, from: usize, to: usize) -> f32 {
    sound.samples[from * 2..to * 2]
        .chunks(2)
        .map(|pair| pair[channel].abs())
        .fold(0.0, f32::max)
}

/// A tone of 1 kHz at `level`, `frames` long, in `channels` channels.
fn tone(frames: usize, channels: u16, level: f32) -> Vec<f32> {
    (0..frames)
        .flat_map(|frame| {
            let value = level * (2.0 * std::f32::consts::PI * 1_000.0 * frame as f32 / RATE as f32).sin();
            std::iter::repeat_n(value, usize::from(channels))
        })
        .collect()
}

/// A click: one sample frame at `level` at frame `at` of `frames` of silence, in two channels.
fn click(frames: usize, at: usize, level: f32) -> Vec<f32> {
    let mut samples = vec![0.0; frames * 2];
    samples[at * 2] = level;
    samples[at * 2 + 1] = level;
    samples
}

#[test]
fn a_click_at_1_0001_seconds_is_heard_at_sample_48005_not_at_a_frame_edge() {
    let (_, sound) = render(
        "click",
        "<audio src=\"click.wav\" start=\"1.0001\" duration=\"0.5\"/>",
        &[("click.wav", 2, click(4_800, 0, 0.9))],
    );
    let (frame, level) = loudest(&sound);
    assert_eq!(frame, 48_005, "the click is loudest at its sample, level {level}");
    assert!(level > 0.3, "the click is heard: {level}");
}

#[test]
fn a_sound_of_two_seconds_is_heard_for_96000_samples_and_silence_around_it() {
    let (_, sound) = render(
        "duration",
        "<audio src=\"tone.wav\" start=\"0.5\" duration=\"2\"/>",
        &[("tone.wav", 2, tone(3 * RATE as usize, 2, 0.5))],
    );
    // 24,000 to 120,000: loud inside, silent outside, away from the encoder's spread at the two edges.
    assert!(peak(&sound, 0, 0, 23_000) < 0.01, "before: {}", peak(&sound, 0, 0, 23_000));
    assert!(
        peak(&sound, 0, 25_000, 119_000) > 0.45,
        "inside: {}",
        peak(&sound, 0, 25_000, 119_000)
    );
    assert!(
        peak(&sound, 0, 121_000, 144_000) < 0.01,
        "after: {}",
        peak(&sound, 0, 121_000, 144_000)
    );
}

#[test]
fn media_start_makes_the_first_sample_heard_the_source_sample_at_half_a_second() {
    // The source is silent but for a click at its frame 24,000, which is 0.5 s; heard from 0.5 s at start 1, it is
    // the first sample of the sound, at frame 48,000 of the film.
    let (_, sound) = render(
        "media-start",
        "<audio src=\"marked.wav\" start=\"1\" duration=\"1\" media-start=\"0.5\"/>",
        &[("marked.wav", 2, click(RATE as usize, 24_000, 0.9))],
    );
    assert_eq!(loudest(&sound).0, 48_000);
}

#[test]
fn a_one_channel_source_is_heard_on_both_sides_3_db_lower() {
    let (_, sound) = render(
        "one-channel",
        "<audio src=\"mono.wav\" start=\"0\" duration=\"3\"/>",
        &[("mono.wav", 1, tone(3 * RATE as usize, 1, 0.5))],
    );
    let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    for channel in [0, 1] {
        let level = peak(&sound, channel, 24_000, 120_000);
        assert!((level - expected).abs() < 0.01, "channel {channel}: {level}, not {expected}");
    }
}

#[test]
fn a_source_of_one_second_in_a_clip_of_three_is_heard_for_one_second_then_silence() {
    let (_, sound) = render(
        "short-source",
        "<audio src=\"short.wav\" start=\"0\" duration=\"3\"/>",
        &[("short.wav", 2, tone(RATE as usize, 2, 0.5))],
    );
    assert!(peak(&sound, 0, 1_000, 47_000) > 0.45);
    assert!(
        peak(&sound, 0, 49_000, 144_000) < 0.01,
        "after the source: {}",
        peak(&sound, 0, 49_000, 144_000)
    );
}

#[test]
fn a_film_with_a_sound_has_a_sound_track_as_long_as_its_picture_to_the_sample() {
    // A sound that would go on past the film's end.
    let (_, sound) = render(
        "length",
        "<audio src=\"tone.wav\" start=\"2\" duration=\"5\"/>",
        &[("tone.wav", 2, tone(5 * RATE as usize, 2, 0.5))],
    );
    // 90 frames at 30 a second are 3 seconds: 144,000 sample frames.
    assert_eq!(sound.frames(), 144_000);
}
