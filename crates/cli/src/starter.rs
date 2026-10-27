//! The starter project that `framesmith init` writes (`docs/spec/init-command.md`). Its composition and its project file are
//! text that ships with the program (`crates/cli/starter/`); its sound is a tone the program computes, so no file of the
//! starter is made by anyone else.

use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const COMPOSITION: &str = include_str!("../starter/composition.xml");
const PROJECT_FILE: &str = include_str!("../starter/framesmith.yaml");

/// The sample rate of the starter's sound, in sample frames a second.
const RATE: u32 = 48_000;
/// The length of the starter's sound, in sample frames: three seconds, as long as the film.
const TONE_FRAMES: u32 = 144_000;
/// The length of the fade in and out of the starter's sound, in sample frames: 0.1 second.
const FADE_FRAMES: u32 = 4_800;
/// The frequency of the starter's sound, in hertz.
const FREQUENCY: f64 = 440.0;
/// The level of the starter's sound, as a fraction of full scale.
const LEVEL: f64 = 0.15;

/// Why the starter was not written: the folder is not empty, or a path could not be created or written.
pub enum Refusal {
    NotEmpty,
    NotCreated { path: PathBuf, reason: String },
}

/// Writes the starter into `folder`, which is created when it does not exist and must be empty when it does. The
/// folder is named as the user wrote it, so every path in a refusal is the path as the user would type it.
pub fn write(folder: &Path) -> Result<(), Refusal> {
    match fs::read_dir(folder) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(Refusal::NotEmpty);
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir(folder).map_err(|error| not_created(folder, &error))?;
        }
        Err(error) => return Err(not_created(folder, &error)),
    }
    let audio = folder.join("audio");
    fs::create_dir(&audio).map_err(|error| not_created(&audio, &error))?;
    let files: [(PathBuf, Vec<u8>); 3] = [
        (folder.join("composition.xml"), COMPOSITION.as_bytes().to_vec()),
        (folder.join("framesmith.yaml"), PROJECT_FILE.as_bytes().to_vec()),
        (audio.join("tone.wav"), tone_wav()),
    ];
    for (path, bytes) in files {
        fs::write(&path, bytes).map_err(|error| not_created(&path, &error))?;
    }
    Ok(())
}

/// The lines `init` prints after it has written the starter: the folder, and the two commands that come next.
pub fn printed(folder: &Path) -> Vec<String> {
    let folder = folder.display();
    vec![
        format!("Created the starter project in {folder}."),
        "Next, run:".to_string(),
        format!("  framesmith check {folder}"),
        format!("  framesmith render {folder}"),
    ]
}

fn not_created(path: &Path, error: &io::Error) -> Refusal {
    Refusal::NotCreated {
        path: path.to_path_buf(),
        reason: reason(error),
    }
}

/// The reason the system gives, without the number of the error.
fn reason(error: &io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

/// The sound of the starter: a 440 Hz tone of 0.15 of full scale, in one channel of 16-bit samples, with a fade in and
/// out at each end (`docs/spec/init-command.md`, section 3.3).
fn tone_wav() -> Vec<u8> {
    let samples: Vec<i16> = (0..TONE_FRAMES).map(sample).collect();
    wav(RATE, &samples)
}

fn sample(frame: u32) -> i16 {
    let last = TONE_FRAMES - 1;
    let fade = (f64::from(frame) / f64::from(FADE_FRAMES))
        .min(f64::from(last - frame) / f64::from(FADE_FRAMES))
        .min(1.0);
    let phase = 2.0 * std::f64::consts::PI * FREQUENCY * f64::from(frame) / f64::from(RATE);
    (f64::from(i16::MAX) * LEVEL * fade * phase.sin()).round() as i16
}

/// A WAV file of one channel of 16-bit linear PCM: its RIFF header, a `fmt ` chunk of 16 bytes and the `data` chunk.
fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
    let data_length = samples.len() as u32 * 2;
    let mut bytes = Vec::with_capacity(44 + data_length as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_length).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_length.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}
