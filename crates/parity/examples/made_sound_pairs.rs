//! Development only: writes pairs of videos whose sound has a known length, gain and shift, for
//! trying the sound checks of `parity compare` by hand (`docs/dev/comparison.md`, section 12).
//!
//! `cargo run -p parity --example made_sound_pairs -- <new folder>` writes, each with the same 30
//! pictures (160 x 90, 30 a second, one second) and a made sound (`measure::made_sound`: four tones
//! that swell and fade, 48 kHz, two channels, peak 0.5) as AAC through the system's encoder:
//!
//! - `base.mp4` and `same.mp4`: the sound 48,000 sample frames long, written separately;
//! - `short-1.mp4`: the sound 47,999 sample frames long, one sample short of the picture;
//! - `quieter-0.4.mp4` and `quieter-0.6.mp4`: the sound 0.4 or 0.6 dB lower;
//! - `louder-0.4.mp4` and `louder-0.6.mp4`: the sound 0.4 or 0.6 dB higher;
//! - `later-0.5ms.mp4` and `later-2ms.mp4`: the sound 24 or 96 sample frames later;
//! - `silent-a.mp4` and `silent-b.mp4`: the pictures and no sound track.

use measure::Sound;
use std::path::Path;

fn gained(sound: &Sound, db: f32) -> Sound {
    Sound {
        samples: sound.samples.iter().map(|v| v * 10f32.powf(db / 20.0)).collect(),
        ..sound.clone()
    }
}

fn main() {
    let Some(folder) = std::env::args().nth(1) else {
        eprintln!("usage: made_sound_pairs <new folder>");
        std::process::exit(2);
    };
    let folder = Path::new(&folder);
    if folder.exists() {
        eprintln!("{} exists; give a new folder", folder.display());
        std::process::exit(2);
    }
    if let Err(error) = std::fs::create_dir_all(folder) {
        eprintln!("{}: {error}", folder.display());
        std::process::exit(2);
    }
    let pictures = measure::moving_pictures(30, 160, 90);
    let sound = |frames: usize, delay: usize| measure::made_sound(48_000, 2, frames, 0.5, delay);
    let base = sound(48_000, 0);
    let files: Vec<(&str, Option<Sound>)> = vec![
        ("base", Some(base.clone())),
        ("same", Some(base.clone())),
        ("short-1", Some(sound(47_999, 0))),
        ("quieter-0.4", Some(gained(&base, -0.4))),
        ("quieter-0.6", Some(gained(&base, -0.6))),
        ("louder-0.4", Some(gained(&base, 0.4))),
        ("louder-0.6", Some(gained(&base, 0.6))),
        ("later-0.5ms", Some(sound(48_000, 24))),
        ("later-2ms", Some(sound(48_000, 96))),
        ("silent-a", None),
        ("silent-b", None),
    ];
    for (name, made) in files {
        let path = folder.join(format!("{name}.mp4"));
        if let Err(error) = measure::write_video_with_sound(&path, (30, 1), &pictures, made.as_ref()) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        println!("{}", path.display());
    }
}
