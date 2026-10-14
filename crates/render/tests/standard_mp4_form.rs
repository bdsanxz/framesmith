//! The form of the finished file: H.264 of 8 bits in 4:2:0 marked BT.709, the tables before the
//! media, key pictures at most two seconds apart, pictures reordered, every picture decodable to the
//! end, and flat colours read back as the BT.709 conversion gives them
//! (`docs/spec/render-command.md` section 5.1). The file is read with frx7 alone.

mod common;

use base::{Color, PixelFormat};
use codec::{Hardware, Pictures};
use common::{project_with, scratch};
use draw_cpu::CpuRenderer;
use project::Project;
use render::{write_film, Scene};
use std::fs;
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

/// A grey, a white and a black box of 32 by 32 on a dark blue canvas of 192 by 108, each on the
/// 16-pixel grid of the encoder's blocks (section 5.1).
fn three_boxes(fps: u32, duration: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"192\" height=\"108\" fps=\"{fps}\" duration=\"{duration}\" background=\"#204060\">\n\
         <box left=\"16\" top=\"32\" width=\"32\" height=\"32\" background=\"#808080\"/>\n\
         <box left=\"80\" top=\"32\" width=\"32\" height=\"32\" background=\"#ffffff\"/>\n\
         <box left=\"144\" top=\"32\" width=\"32\" height=\"32\" background=\"#000000\"/>\n\
         </composition>\n"
    )
}

/// Renders the composition to a file of its own and gives the file's path.
fn rendered(name: &str, fps: u32, duration: &str) -> PathBuf {
    let folder = project_with("standard-mp4", &format!("{name}-in"), &three_boxes(fps, duration));
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let output = scratch("standard-mp4", &format!("{name}-out")).join("film.mp4");
    write_film(&scene, &mut CpuRenderer, &output).unwrap();
    output
}

fn picture_track(info: &probe::Info) -> &probe::Found {
    info.tracks
        .iter()
        .find(|found| found.track.video().is_some())
        .expect("a picture track")
}

#[test]
fn the_stream_is_h264_of_8_bits_in_4_2_0_marked_bt709_in_its_headers_and_its_track() {
    let film = rendered("marks", 30, "1");
    let info = probe::look(&film, &formats()).unwrap();
    // One picture track; the one sound track beside it is section 5.2's.
    assert_eq!(info.tracks.iter().filter(|found| found.track.video().is_some()).count(), 1);
    let found = picture_track(&info);
    assert_eq!(found.track.codec.name(), "h264");
    let facts = found.facts.expect("the headers of the stream are read");
    assert_eq!(facts.bits, 8);
    assert_eq!(format!("{:?}", facts.chroma), "Yuv420", "4:2:0");
    assert!(!facts.interlaced);
    assert_eq!(facts.color, Some(Color::BT709), "the colour description of the headers");
    assert_eq!(
        found.track.video().unwrap().color,
        Some(Color::BT709),
        "the colour marks of the track"
    );
    let text = info.text();
    assert!(text.contains("primaries 1, transfer 1, matrix 1, limited range"), "{text}");
    assert!(text.contains("headers: 8 bits, 4:2:0"), "{text}");
}

/// The four letters and the place of every top-level box of a file, in the order they lie.
fn top_level_boxes(path: &Path) -> Vec<(String, u64, u64)> {
    let bytes = fs::read(path).unwrap();
    let mut boxes = Vec::new();
    let mut at = 0usize;
    while at + 8 <= bytes.len() {
        let small = u64::from(u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()));
        let name = String::from_utf8_lossy(&bytes[at + 4..at + 8]).into_owned();
        let size = match small {
            1 => u64::from_be_bytes(bytes[at + 8..at + 16].try_into().unwrap()),
            0 => (bytes.len() - at) as u64,
            size => size,
        };
        assert!(size >= 8, "box `{name}` at {at} has a size of {size}");
        boxes.push((name, at as u64, size));
        at += size as usize;
    }
    assert_eq!(at, bytes.len(), "the boxes end where the file ends");
    boxes
}

#[test]
fn the_tables_lie_before_the_media() {
    let film = rendered("front", 30, "2");
    let boxes = top_level_boxes(&film);
    let names: Vec<&str> = boxes.iter().map(|(name, _, _)| name.as_str()).collect();
    assert_eq!(names.first(), Some(&"ftyp"), "{names:?}");
    assert_eq!(names.iter().filter(|name| **name == "moov").count(), 1, "{names:?}");
    assert_eq!(names.iter().filter(|name| **name == "mdat").count(), 1, "{names:?}");
    let moov = names.iter().position(|name| *name == "moov").unwrap();
    let mdat = names.iter().position(|name| *name == "mdat").unwrap();
    assert!(moov < mdat, "the tables come before the media: {names:?}");
    assert!(
        names[moov + 1..mdat].iter().all(|name| *name == "free"),
        "only free room lies between the tables and the media: {names:?}"
    );
}

/// The times, in seconds, at which the key pictures of a file are shown, in the order shown.
fn key_times(path: &Path) -> (Vec<f64>, u64, f64) {
    let info = probe::look_at_samples(path, &formats()).unwrap();
    let found = picture_track(&info);
    let scale = f64::from(found.track.timescale);
    let mut keys: Vec<f64> = found
        .samples
        .iter()
        .filter(|sample| sample.key)
        .map(|sample| sample.shown as f64 / scale)
        .collect();
    keys.sort_by(f64::total_cmp);
    (keys, found.track.samples, found.track.duration as f64 / scale)
}

fn key_pictures_at_most_two_seconds_apart(fps: u32) {
    let film = rendered(&format!("keys-{fps}"), fps, "10");
    let (keys, pictures, seconds) = key_times(&film);
    assert_eq!(pictures, u64::from(fps) * 10);
    assert!((seconds - 10.0).abs() < 1e-9, "{seconds}");
    assert_eq!(keys.first(), Some(&0.0), "the first picture is a key picture: {keys:?}");
    assert!(keys.len() >= 5, "ten seconds need at least five key pictures: {keys:?}");
    for pair in keys.windows(2) {
        assert!(
            pair[1] - pair[0] <= 2.0 + 1e-9,
            "at {fps} frames a second key pictures at {} s and {} s are {} s apart: {keys:?}",
            pair[0],
            pair[1],
            pair[1] - pair[0]
        );
    }
}

#[test]
fn in_ten_seconds_at_24_frames_a_second_no_two_key_pictures_are_further_apart_than_two_seconds() {
    key_pictures_at_most_two_seconds_apart(24);
}

#[test]
fn in_ten_seconds_at_30_frames_a_second_no_two_key_pictures_are_further_apart_than_two_seconds() {
    key_pictures_at_most_two_seconds_apart(30);
}

#[test]
fn in_ten_seconds_at_60_frames_a_second_no_two_key_pictures_are_further_apart_than_two_seconds() {
    key_pictures_at_most_two_seconds_apart(60);
}

#[test]
fn the_pictures_of_the_stream_are_reordered() {
    let film = rendered("reordered", 30, "2");
    let info = probe::look_at_samples(&film, &formats()).unwrap();
    let found = picture_track(&info);
    let reordering = found.reordering.expect("probe says whether pictures are reordered");
    assert!(reordering.reordered, "{reordering:?}");
    assert!(info.text().contains("pictures reordered"), "{}", info.text());
    // The same seen in the sample times directly: some picture is shown later than one decoded after it.
    let samples = &found.samples;
    let later = (0..samples.len()).any(|one| samples[one + 1..].iter().any(|after| after.shown < samples[one].shown));
    assert!(later, "a picture is shown later than one decoded after it");
}

#[test]
fn every_picture_decodes_to_the_end_in_the_order_shown() {
    let film = rendered("to-the-end", 30, "3");
    let mut pictures = Pictures::open(&film, &formats(), &codecs(), Hardware::Any).unwrap();
    assert_eq!(pictures.count(), 90);
    let mut numbers = Vec::new();
    let mut last_pts = i64::MIN;
    while let Some(numbered) = pictures.next_picture().unwrap() {
        assert!(numbered.frame.pts > last_pts, "times increase");
        last_pts = numbered.frame.pts;
        numbers.push(numbered.number);
    }
    assert_eq!(numbers, (0..90).collect::<Vec<u64>>());
}

/// Luma, Cb and Cr of a decoded picture at one place.
fn ycbcr(picture: &base::Picture, x: usize, y: usize) -> [u8; 3] {
    let luma = &picture.planes[0];
    let chroma = &picture.planes[1];
    let at = (y / 2) * chroma.stride + (x / 2) * 2;
    [luma.data[y * luma.stride + x], chroma.data[at], chroma.data[at + 1]]
}

/// The most every sample inside a box, 4 pixels in from its edges, differs from `want`, over every
/// picture of the file.
fn largest_differences(film: &Path) -> Vec<(&'static str, [u8; 3], u8)> {
    let boxes: [(&str, usize, [u8; 3]); 3] = [
        ("grey", 16, [126, 128, 128]),
        ("white", 80, [235, 128, 128]),
        ("black", 144, [16, 128, 128]),
    ];
    let mut most = [0u8; 3];
    let mut pictures = Pictures::open(film, &formats(), &codecs(), Hardware::Any).unwrap();
    let mut count = 0;
    while let Some(numbered) = pictures.next_picture().unwrap() {
        let read = numbered.frame.surface.read().unwrap();
        let planes = pixel::convert(&read, Some(Color::BT709), PixelFormat::Nv12, Some(Color::BT709)).unwrap();
        for (place, (_, left, want)) in boxes.iter().enumerate() {
            for y in 32 + 4..32 + 32 - 4 {
                for x in left + 4..left + 32 - 4 {
                    let found = ycbcr(&planes, x, y);
                    for channel in 0..3 {
                        most[place] = most[place].max(found[channel].abs_diff(want[channel]));
                    }
                }
            }
        }
        count += 1;
    }
    assert_eq!(count, 30);
    boxes.iter().zip(most).map(|((name, _, want), most)| (*name, *want, most)).collect()
}

#[test]
fn a_grey_a_white_and_a_black_box_read_back_with_the_values_of_the_bt709_conversion_within_two_levels() {
    let film = rendered("flat", 30, "1");
    for (name, want, most) in largest_differences(&film) {
        eprintln!("the {name} box: the most any sample lies from {want:?} is {most}");
        assert!(most <= 2, "the {name} box lies {most} levels from {want:?}, more than the stated 2");
    }
}
