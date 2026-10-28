//! Every film holds one sound track: AAC at 48 kHz in two channels, at 192 kbit/s, as long as the
//! picture to the sample, and silent for a composition with no sound
//! (`docs/spec/render-command.md` section 5.2). The file is read with frx7 alone.

mod common;

use codec::{Hardware, Pictures};
use common::{project_with, scratch};
use draw_cpu::CpuRenderer;
use project::Project;
use render::{write_film, Scene};
use std::path::{Path, PathBuf};

fn formats() -> format::Registry {
    let mut formats = format::Registry::new();
    formats.register(mp4::FORMAT);
    formats
}

fn codecs() -> codec::Registry {
    let mut codecs = codec::Registry::new();
    codecs.register(backend_os::BACKEND);
    codecs
}

/// Renders a composition with no sound, `duration` seconds at `fps`, and gives the file.
fn rendered(name: &str, fps: u32, duration: &str) -> PathBuf {
    let composition = format!(
        "<composition version=\"1\" width=\"96\" height=\"64\" fps=\"{fps}\" duration=\"{duration}\" background=\"#204060\">\n\
         <box left=\"16\" top=\"16\" width=\"32\" height=\"32\" background=\"#ffffff\"/>\n\
         </composition>\n"
    );
    let folder = project_with("silent-sound", &format!("{name}-in"), &composition);
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let output = scratch("silent-sound", &format!("{name}-out")).join("film.mp4");
    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    output
}

fn sound_tracks(info: &probe::Info) -> Vec<&probe::Found> {
    info.tracks.iter().filter(|found| found.track.audio().is_some()).collect()
}

/// Every sample of the file's sound, read back through frx7's sound reader: the sample frames and
/// the loudest sample.
fn read_back(film: &Path) -> (u64, u16, u32, f32) {
    let mut source = codecs().audio_source(film).unwrap();
    let channels = source.channels();
    let rate = source.sample_rate();
    let mut piece = vec![1.0_f32; 4096 * usize::from(channels)];
    let mut frames = 0_u64;
    let mut loudest = 0.0_f32;
    loop {
        let read = source.read(&mut piece).unwrap();
        if read == 0 {
            break;
        }
        for sample in &piece[..read * usize::from(channels)] {
            loudest = loudest.max(sample.abs());
        }
        frames += read as u64;
    }
    (frames, channels, rate, loudest)
}

#[test]
fn a_film_with_no_sound_has_one_aac_sound_track_at_48_khz_in_two_channels() {
    let film = rendered("form", 30, "1");
    let info = probe::look(&film, &formats()).unwrap();
    assert_eq!(info.tracks.len(), 2, "one picture track and one sound track");
    let sound = sound_tracks(&info);
    assert_eq!(sound.len(), 1);
    assert_eq!(sound[0].track.codec, base::Codec::Aac);
    let audio = sound[0].track.audio().unwrap();
    assert_eq!(audio.sample_rate, 48_000);
    assert_eq!(audio.channels, 2);
    assert_eq!(sound[0].entry, Some(*b"mp4a"));
}

fn the_sound_lasts_the_picture_to_the_sample(fps: u32, duration: &str, seconds: u64) {
    let film = rendered(&format!("length-{fps}"), fps, duration);
    let info = probe::look(&film, &formats()).unwrap();
    let sound = sound_tracks(&info)[0];
    // The track's own length, in its ticks of 1/48,000 s, after the lead-in its edit list skips.
    assert_eq!(sound.track.timescale, 48_000);
    assert_eq!(sound.track.duration, seconds * 48_000, "the track's length in samples");
    let (frames, channels, rate, _) = read_back(&film);
    assert_eq!((channels, rate), (2, 48_000));
    assert_eq!(frames, seconds * 48_000, "the samples read back");
}

#[test]
fn at_24_frames_a_second_the_sound_lasts_the_picture_to_the_sample() {
    the_sound_lasts_the_picture_to_the_sample(24, "3", 3);
}

#[test]
fn at_30_frames_a_second_the_sound_lasts_the_picture_to_the_sample() {
    the_sound_lasts_the_picture_to_the_sample(30, "3", 3);
}

#[test]
fn at_60_frames_a_second_the_sound_lasts_the_picture_to_the_sample() {
    the_sound_lasts_the_picture_to_the_sample(60, "3", 3);
}

#[test]
fn a_length_of_an_odd_number_of_frames_gives_its_samples_exactly() {
    // 31 frames at 30 a second: 1.0333... s, 49,600 sample frames.
    let film = rendered("odd", 30, "1.01");
    let (frames, _, _, _) = read_back(&film);
    assert_eq!(frames, 49_600);
}

#[test]
fn every_sample_read_back_is_silence() {
    let film = rendered("silence", 30, "2");
    let (frames, _, _, loudest) = read_back(&film);
    assert_eq!(frames, 96_000);
    assert_eq!(loudest, 0.0, "the loudest sample read back");
}

#[test]
fn the_bit_rate_of_the_sound_track_is_within_ten_percent_of_192_kbit_s() {
    for (name, duration) in [("rate-1s", "1"), ("rate-10s", "10")] {
        let film = rendered(name, 30, duration);
        let info = probe::look(&film, &formats()).unwrap();
        let rate = sound_tracks(&info)[0].bit_rate();
        assert!(
            (rate / 192_000.0 - 1.0).abs() <= 0.10,
            "{name}: {rate:.0} bits a second against 192,000"
        );
    }
}

#[test]
fn every_picture_decodes_to_the_end_and_the_sound_track_is_read_to_its_end_by_the_system() {
    let film = rendered("plays", 24, "2");
    let mut pictures = Pictures::open(&film, &formats(), &codecs(), Hardware::Any).unwrap();
    assert_eq!(pictures.count(), 48);
    let mut decoded = 0;
    while pictures.next_picture().unwrap().is_some() {
        decoded += 1;
    }
    assert_eq!(decoded, 48);
    let (frames, channels, rate, _) = read_back(&film);
    assert_eq!((frames, channels, rate), (96_000, 2, 48_000));
}

#[test]
fn a_sound_bit_rate_of_128_kbit_s_gives_a_track_within_ten_percent_of_128_kbit_s() {
    let composition = "<composition version=\"1\" width=\"96\" height=\"64\" fps=\"30\" duration=\"2\" background=\"#204060\"/>\n";
    let folder = project_with("silent-sound", "rate-128-in", composition);
    std::fs::write(folder.join("framesmith.yaml"), "render:\n  audio-bitrate: 128\n").unwrap();
    let project = Project::open(&folder).unwrap();
    assert_eq!(project.settings().audio_bitrate(), 128);
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let output = scratch("silent-sound", "rate-128-out").join("film.mp4");
    write_film(&scene, &mut CpuRenderer, &output, project.settings().audio_bitrate()).unwrap();
    let info = probe::look(&output, &formats()).unwrap();
    let rate = sound_tracks(&info)[0].bit_rate();
    assert!((rate / 128_000.0 - 1.0).abs() <= 0.10, "{rate:.0} bits a second against 128,000");
}
