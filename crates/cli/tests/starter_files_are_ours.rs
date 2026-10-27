//! Every file of the starter is written by us: the starter holds exactly its three files, and none of them is a font, a
//! picture or a sound of anyone else's making, or a script (`CLAUDE.md`, rule 11; `docs/spec/init-command.md`, section 3).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{framesmith, scratch};

/// Every file under `folder`, relative to it, in the order of their paths.
fn files_under(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path.strip_prefix(folder).unwrap().to_path_buf());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_starter_holds_exactly_its_three_files_and_each_is_ours() {
    let cwd = scratch("init", "files-are-ours");
    let output = framesmith(&cwd, ["init", "demo"], &[]);
    assert_eq!(output.status.code(), Some(0));
    let folder = cwd.join("demo");
    let files = files_under(&folder);
    let expected: Vec<PathBuf> = ["audio/tone.wav", "composition.xml", "framesmith.yaml"]
        .iter()
        .map(PathBuf::from)
        .collect();
    assert_eq!(files, expected, "the starter is these three files and no others");
    for file in &files {
        let name = file.to_string_lossy().to_lowercase();
        for banned in [
            ".ttf", ".otf", ".ttc", ".woff", ".woff2", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".mp3", ".m4a", ".aac",
            ".aif", ".aiff", ".caf", ".mp4", ".mov", ".js", ".ts", ".sh", ".py", ".rb", ".pl", ".html", ".htm", ".wasm", ".exe", ".dylib",
        ] {
            assert!(!name.ends_with(banned), "{name} is of a kind the starter must not hold");
        }
    }
}

#[test]
fn the_starter_tone_is_the_wav_file_the_spec_describes() {
    let cwd = scratch("init", "tone-is-ours");
    let output = framesmith(&cwd, ["init", "demo"], &[]);
    assert_eq!(output.status.code(), Some(0));
    let bytes = fs::read(cwd.join("demo/audio/tone.wav")).unwrap();
    // The RIFF header of a plain WAV file: one channel, 48,000 sample frames a second, 16 bits a sample (`init-command.md`,
    // section 3.3): 144,000 sample frames, so 288,000 bytes of data after the 44 bytes of header.
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36 + 288_000);
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 16);
    assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1, "linear PCM");
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1, "one channel");
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 48_000, "sample rate");
    assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 16, "bits a sample");
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 288_000);
    let sample = |frame: usize| i16::from_le_bytes(bytes[44 + frame * 2..46 + frame * 2].try_into().unwrap());
    // The fades hold the first and the last sample at 0, and the steady tone peaks at 0.15 of full scale: 4915.05, rounded.
    assert_eq!(sample(0), 0);
    assert_eq!(sample(143_999), 0);
    let peak = (4_800..139_200).map(|frame| sample(frame).abs()).max().unwrap();
    assert!((4_914..=4_916).contains(&peak), "steady peak {peak}");
}
