//! The sound of a timeline, mixed in stretches and encoded, read back from the finished file.
//! These tests need `ffmpeg`, which makes their sources and reads the finished file back;
//! `poc` does not start it. The file is AAC, so levels are compared with a little room.

use poc::audio::{self, CEILING, RATE, STRETCH};
use poc::model::Audio;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sound-render").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

/// Writes a tone of our own making: `seconds` of 440 Hz at `height`, at `rate` samples a
/// second, the same on `channels` channels.
fn tone_of(folder: &Path, name: &str, height: f32, seconds: f64, rate: u32, channels: usize) {
    let one = format!("{height}*sin(2*PI*440*t)");
    let all = vec![one; channels].join("|");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("aevalsrc={all}:s={rate}:d={seconds}"))
        .args(["-c:a", "pcm_f32le"])
        .arg(folder.join(name))
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg made the tone");
}

/// A tone on two channels.
fn tone(folder: &Path, name: &str, height: f32, seconds: f64, rate: u32) {
    tone_of(folder, name, height, seconds, rate, 2);
}

fn sound(name: &str, start: f64, duration: f64, volume: f64) -> Audio {
    Audio {
        id: name.to_string(),
        source: PathBuf::from(name),
        start,
        duration,
        media_start: 0.0,
        volume,
        automation: Vec::new(),
        line: 1,
    }
}

/// The left channel of a finished sound file.
fn left(file: &Path) -> Vec<f32> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(file)
        .args(["-af", "pan=mono|c0=c0", "-ar", &RATE.to_string(), "-f", "f32le", "pipe:1"])
        .output()
        .expect("ffmpeg runs");
    assert!(output.status.success(), "the sound file is read");
    let samples = output.stdout.as_chunks::<4>().0.iter();
    samples.map(|bytes| f32::from_le_bytes(*bytes)).collect()
}

/// The highest sample between two times.
fn height(samples: &[f32], from: f64, to: f64) -> f32 {
    let at = |seconds: f64| (seconds * f64::from(RATE)) as usize;
    let part = &samples[at(from)..at(to).min(samples.len())];
    part.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

#[test]
fn a_sound_plays_in_its_window_at_its_volume_and_the_rest_is_silent() {
    let folder = folder("window");
    tone(&folder, "tone.wav", 0.5, 3.0, 44_100);
    // One second of a three-second source, from 1.0 s, at half volume.
    let sounds = [sound("tone.wav", 1.0, 1.0, 0.5)];
    let output = folder.join("out.m4a");
    let mixed = audio::render(&folder, &sounds, 3.0, &output, 192).expect("the sound is made");
    assert!((mixed.peak - 0.25).abs() < 0.01 && mixed.lowered == 0.0, "peak {}", mixed.peak);
    let samples = left(&output);
    assert!(
        (samples.len() as f64 / f64::from(RATE) - 3.0).abs() < 0.03,
        "three seconds of sound"
    );
    assert!(height(&samples, 0.0, 0.95) < 0.01, "silent before the sound");
    let playing = height(&samples, 1.05, 1.95);
    assert!((playing - 0.25).abs() < 0.02, "the tone at half its height: {playing}");
    assert!(height(&samples, 2.05, 2.95) < 0.01, "silent after the sound");
    assert!(!output.with_extension("mix.f32").exists(), "the mix as made is removed");
}

#[test]
fn a_sound_longer_than_a_stretch_has_no_gap_where_stretches_meet() {
    let folder = folder("stretches");
    let stretch = STRETCH as f64 / f64::from(RATE);
    let seconds = stretch * 2.5;
    tone(&folder, "tone.wav", 0.5, seconds, 48_000);
    let output = folder.join("out.m4a");
    audio::render(&folder, &[sound("tone.wav", 0.0, seconds, 1.0)], seconds, &output, 192).expect("the sound is made");
    let samples = left(&output);
    // Every twentieth of a second around the two joins holds the tone at its height.
    for join in [stretch, stretch * 2.0] {
        for step in -4..4 {
            let from = join + f64::from(step) * 0.05;
            let here = height(&samples, from, from + 0.05);
            assert!((here - 0.5).abs() < 0.03, "at {from:.2} s the tone is at {here}");
        }
    }
}

#[test]
fn two_sounds_that_add_up_over_the_ceiling_are_lowered_together() {
    let folder = folder("ceiling");
    tone(&folder, "one.wav", 0.8, 2.0, 48_000);
    tone(&folder, "two.wav", 0.8, 2.0, 48_000);
    let output = folder.join("out.m4a");
    let sounds = [sound("one.wav", 0.0, 2.0, 1.0), sound("two.wav", 0.0, 2.0, 1.0)];
    let mixed = audio::render(&folder, &sounds, 2.0, &output, 192).expect("the sound is made");
    assert!((mixed.peak - 1.6).abs() < 0.01, "the two tones add up to {}", mixed.peak);
    let expected = 20.0 * (mixed.peak / CEILING).log10();
    assert!((mixed.lowered - expected).abs() < 1e-3, "lowered by {} dB", mixed.lowered);
    let playing = height(&left(&output), 0.5, 1.5);
    assert!((playing - CEILING).abs() < 0.03, "the file's tone is at the ceiling: {playing}");
}

#[test]
fn a_source_that_ends_before_its_clip_is_silent_from_there() {
    let folder = folder("short");
    tone(&folder, "tone.wav", 0.5, 1.0, 48_000);
    let output = folder.join("out.m4a");
    audio::render(&folder, &[sound("tone.wav", 0.0, 3.0, 1.0)], 3.0, &output, 192).expect("the sound is made");
    let samples = left(&output);
    assert!((height(&samples, 0.1, 0.9) - 0.5).abs() < 0.03);
    assert!(height(&samples, 1.1, 2.9) < 0.01, "silent after the source's end");
}

#[test]
fn a_missing_source_and_one_that_is_no_sound_are_named() {
    let folder = folder("faults");
    let output = folder.join("out.m4a");
    let missing = audio::render(&folder, &[sound("nowhere.wav", 0.0, 1.0, 1.0)], 1.0, &output, 192);
    let said = missing.err().expect("a missing source is an error");
    assert!(said.contains("nowhere.wav") && said.contains("was not found"), "{said}");
    std::fs::write(folder.join("words.wav"), "these are words, not sound").expect("the file is written");
    let broken = audio::render(&folder, &[sound("words.wav", 0.0, 1.0, 1.0)], 1.0, &output, 192);
    let said = broken.err().expect("a source that cannot be decoded is an error");
    assert!(said.contains("words.wav") && said.contains("could not be read"), "{said}");
    assert!(
        !output.with_extension("mix.f32").exists(),
        "the mix as made is removed after a failure too"
    );
}

#[test]
fn a_source_of_one_channel_plays_on_both_sides_three_decibels_lower() {
    let folder = folder("one-channel");
    tone_of(&folder, "tone.wav", 0.5, 1.0, 48_000, 1);
    let output = folder.join("out.m4a");
    let mixed = audio::render(&folder, &[sound("tone.wav", 0.0, 1.0, 1.0)], 1.0, &output, 192).expect("the sound is made");
    let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    assert!((mixed.peak - expected).abs() < 0.01, "the peak of the mix: {}", mixed.peak);
}

#[test]
fn a_sound_from_a_file_with_a_lead_in_plays_on_time() {
    // AAC starts with a lead-in of about 23 ms that a decoder passes over. A click half a
    // second into such a file, played from the file's beginning, is half a second in.
    let folder = folder("lead-in");
    let click = "if(between(t\\,0.5\\,0.51)\\,0.8*sin(2*PI*1000*(t-0.5))\\,0)";
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("aevalsrc={click}|{click}:s=44100:d=2"))
        .args(["-c:a", "aac", "-b:a", "192k"])
        .arg(folder.join("click.m4a"))
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg made the click");
    let output = folder.join("out.m4a");
    audio::render(&folder, &[sound("click.m4a", 1.0, 2.0, 1.0)], 3.0, &output, 192).expect("the sound is made");
    let samples = left(&output);
    let first = samples.iter().position(|sample| sample.abs() > 0.1).expect("the click is there");
    let at = first as f64 / f64::from(RATE);
    assert!((at - 1.5).abs() < 0.002, "the click begins at {at} s");
}
