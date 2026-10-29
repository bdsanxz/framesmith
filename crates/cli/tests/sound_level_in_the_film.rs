//! The level of a sound in the film `framesmith render` writes (`docs/spec/sound.md`, section 3a): a sound's sound track
//! is read back through frx7, so a level is compared within the encoder's loss. The exact values of the level are held by
//! the mix's own tests; here the film must play the level it was given, and a sound longer than the film is cut at its end.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, project};
use testkit::sound_files::{write_wav, Pcm};

const RATE: u32 = 48_000;

/// Renders a film of `seconds` seconds at 30 fps of `audio` (the `<audio>` elements, from line 2) with the WAV file
/// `tone.wav` of a tone at `level` in the project folder. Returns the film's sound as frames of two channels.
fn render(name: &str, seconds: u32, audio: &str, level: f32) -> measure::Sound {
    let text = format!(
        "<composition version=\"1\" width=\"64\" height=\"64\" fps=\"30\" duration=\"{seconds}\" background=\"#000000\">\n{audio}\n</composition>\n"
    );
    let (project, cwd) = project("sound-level-film", name, &text);
    let samples: Vec<f32> = (0..seconds as usize * RATE as usize)
        .flat_map(|frame| {
            let value = level * (2.0 * std::f32::consts::PI * 1_000.0 * frame as f32 / RATE as f32).sin();
            [value, value]
        })
        .collect();
    write_wav(&project.join("tone.wav"), RATE, 2, Pcm::Int(16), &samples).unwrap();
    let output = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    let sound = measure::read_sound(&film(&project)).unwrap().expect("the film has a sound track");
    assert_eq!((sound.rate, sound.channels), (RATE, 2));
    sound
}

fn film(project: &Path) -> PathBuf {
    fs::read_dir(project.join("renders"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "mp4"))
        .expect("a film was written")
}

/// The largest level of `sound` from frame `from` up to `to`, in its left channel.
fn peak(sound: &measure::Sound, from: usize, to: usize) -> f32 {
    sound.samples[from * 2..to * 2]
        .chunks(2)
        .map(|pair| pair[0].abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_volume_of_one_half_plays_the_level_at_half_the_amplitude() {
    let plain = render("plain", 3, "<audio src=\"tone.wav\" start=\"0\" duration=\"3\"/>", 0.5);
    let halved = render(
        "halved",
        3,
        "<audio src=\"tone.wav\" start=\"0\" duration=\"3\" volume=\"0.5\"/>",
        0.5,
    );
    // Inside the sound, away from the encoder's spread at its edges. The tone's own peak is 0.5, half of it is 0.25.
    let (full, half) = (peak(&plain, 24_000, 120_000), peak(&halved, 24_000, 120_000));
    assert!((full - 0.5).abs() < 0.03, "no volume: {full}");
    assert!((half - 0.25).abs() < 0.03, "volume 0.5: {half}");
}

#[test]
fn points_at_one_and_two_seconds_hold_the_level_then_fall_to_silence() {
    let sound = render(
        "points",
        3,
        "<audio src=\"tone.wav\" start=\"0\" duration=\"3\">\n<volume at=\"1\" value=\"1\"/>\n<volume at=\"2\" value=\"0\"/>\n</audio>",
        0.5,
    );
    // Full amplitude until 1 second; the level 0.5 at 1.5 seconds (a tone of 0.5 at half is 0.25); silence from 2 seconds.
    assert!(
        (peak(&sound, 4_000, 40_000) - 0.5).abs() < 0.03,
        "until 1 s: {}",
        peak(&sound, 4_000, 40_000)
    );
    assert!(
        (peak(&sound, 70_000, 74_000) - 0.25).abs() < 0.03,
        "at 1.5 s: {}",
        peak(&sound, 70_000, 74_000)
    );
    assert!(
        peak(&sound, 100_000, 140_000) < 0.01,
        "after 2 s: {}",
        peak(&sound, 100_000, 140_000)
    );
}

#[test]
fn a_sound_longer_than_the_film_is_cut_at_the_last_sample_frame_of_the_film() {
    // A film of one second is 48,000 sample frames; the sound of three seconds is cut there, and the track is as long.
    let sound = render("cut", 1, "<audio src=\"tone.wav\" start=\"0\" duration=\"3\"/>", 0.5);
    assert_eq!(sound.samples.len() / 2, RATE as usize, "the sound track is as long as the film");
}
