//! A mix that is too loud is lowered as a whole, and the summary says by how much (`docs/spec/sound.md`,
//! section 4; `docs/spec/render-command.md`, section 7). The tones are made by frx7's test-only writer and the
//! film's sound is read back through frx7, so peaks are compared within the encoder's loss; the exact lowered
//! values are held by the mix's own tests. The meter that reads the peak back is the product's own, held to
//! hand-worked values by the mix tests independently of any film.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, project};
use testkit::sound_files::{write_wav, Pcm};

const RATE: u32 = 48_000;

/// A cosine tone of `seconds` seconds at `freq`, amplitude `amp`, on both channels.
fn tone(seconds: usize, freq: f64, amp: f32) -> Vec<f32> {
    (0..seconds * RATE as usize)
        .flat_map(|frame| {
            let value = amp * (2.0 * std::f64::consts::PI * freq * frame as f64 / RATE as f64).cos() as f32;
            [value, value]
        })
        .collect()
}

/// Renders a film of `seconds` seconds at 30 fps of `audio` (the `<audio>` elements, from line 2) over the WAV
/// files `wavs` (name and samples) in the project folder. Returns the printed summary and the film's sound.
fn render(name: &str, seconds: u32, audio: &str, wavs: &[(&str, Vec<f32>)]) -> (String, measure::Sound) {
    let text = format!(
        "<composition version=\"1\" width=\"64\" height=\"64\" fps=\"30\" duration=\"{seconds}\" background=\"#000000\">\n{audio}\n</composition>\n"
    );
    let (project, cwd) = project("sound-lowered-film", name, &text);
    for (file, samples) in wavs {
        write_wav(&project.join(file), RATE, 2, Pcm::Float, samples).unwrap();
    }
    let output = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    let printed = out(&output);
    let sound = measure::read_sound(&film(&project)).unwrap().expect("the film has a sound track");
    assert_eq!((sound.rate, sound.channels), (RATE, 2));
    (printed, sound)
}

fn film(project: &Path) -> PathBuf {
    fs::read_dir(project.join("renders"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "mp4"))
        .expect("a film was written")
}

/// The true peak of read-back samples in decibels under full scale, through the meter the mix tests hold.
fn db_under(samples: &[f32]) -> f64 {
    let mut meter = mix::peak::Meter::new();
    let frames: Vec<[f32; 2]> = samples.as_chunks::<2>().0.iter().map(|pair| [pair[0], pair[1]]).collect();
    let last = *frames.last().unwrap();
    meter.push(&frames);
    -20.0 * meter.peak(last).log10()
}

#[test]
fn two_full_tones_lower_to_the_limit_and_the_summary_says_how_much() {
    // Two full-level cosines (440 and 880 Hz) meet crest to crest at 2.0: the film's mix is lowered
    // by 20·log10(2 / 0.891250938) = 7.02 dB, the render ends 0, and what is read back peaks a
    // decibel under full scale, within the encoder's loss.
    let printed = render(
        "two-tones",
        1,
        "<audio src=\"loud.wav\" start=\"0\" duration=\"1\"/>\n<audio src=\"louder.wav\" start=\"0\" duration=\"1\"/>",
        &[("loud.wav", tone(1, 440.0, 1.0)), ("louder.wav", tone(1, 880.0, 1.0))],
    )
    .0;
    assert!(printed.contains("lowered: 7.02 dB"), "{printed}");
}

#[test]
fn the_lowered_film_keeps_headroom_through_the_encoder() {
    // The mix itself peaks a decibel under full scale (the mix tests hold that to the bit); the
    // encoder adds its own loss, about 0.7 dB on these tones on this machine.
    // What the film must keep is headroom: read back, the true peak is still under full scale, and
    // still within a decibel of where the mix put it. Without the lowering it would read about 6 dB
    // over full scale.
    let (_, sound) = render(
        "two-tones-peak",
        1,
        "<audio src=\"loud.wav\" start=\"0\" duration=\"1\"/>\n<audio src=\"louder.wav\" start=\"0\" duration=\"1\"/>",
        &[("loud.wav", tone(1, 440.0, 1.0)), ("louder.wav", tone(1, 880.0, 1.0))],
    );
    let under = db_under(&sound.samples);
    assert!(under > 0.0 && under <= 0.8, "{under}");
}

#[test]
fn a_quiet_mix_has_no_lowering_line() {
    // One 440 Hz cosine at 0.25: under the limit, so the summary ends with the elapsed line right
    // after the audio bit rate line, with no lowering in between.
    let (printed, _) = render(
        "quiet",
        1,
        "<audio src=\"soft.wav\" start=\"0\" duration=\"1\"/>",
        &[("soft.wav", tone(1, 440.0, 0.25))],
    );
    assert!(!printed.contains("lowered:"), "{printed}");
}
