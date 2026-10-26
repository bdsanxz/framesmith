//! A film whose picture is HEVC (`docs/spec/render-command.md` section 5.3): HEVC of 8 bits in 4:2:0, Main profile,
//! marked `hvc1` with `hvcC` and the brand `hvc1`, every picture and all the sound decodable to the end, and the same
//! frame marks as the H.264 film of the same scene. The expected values are those of ISO/IEC 14496-15 clause 8.3.3.1
//! and 8.4.1 and of ITU-T H.265 (Main is general profile 1), read from the file's bytes by this test's own walk.

mod common;

use base::Color;
use codec::{Hardware, Pictures};
use common::{project_with, scratch};
use draw_cpu::CpuRenderer;
use project::{Codec, Project};
use render::{write_film, write_film_marked, Encoding, Scene};
use std::fs;
use std::path::{Path, PathBuf};

const GROUP: &str = "hevc-film";

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

/// Boxes that move over two seconds at 30 frames a second, on a canvas of 192 by 108.
const MOVING: &str = "<composition version=\"1\" width=\"192\" height=\"108\" fps=\"30\" duration=\"2\" background=\"#204060\">\n\
     <box left=\"16\" top=\"32\" width=\"32\" height=\"32\" background=\"#ffffff\">\n\
     <tween at=\"0\" duration=\"2\" to-x=\"120\"/>\n\
     </box>\n\
     <box left=\"80\" top=\"64\" width=\"32\" height=\"32\" background=\"#808080\"/>\n\
     </composition>\n";

/// Renders `MOVING` in `codec` and gives the file and the marks of the frames drawn.
fn rendered(name: &str, codec: Codec) -> (PathBuf, Vec<u64>) {
    let folder = project_with(GROUP, &format!("{name}-in"), MOVING);
    let scene = Scene::from_project(&Project::open(&folder).unwrap(), &codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let output = scratch(GROUP, &format!("{name}-out")).join("film.mp4");
    let mut marks = Vec::new();
    write_film_marked(
        &scene,
        Vec::new(),
        &mut CpuRenderer,
        &output,
        192,
        Encoding { codec, ..Encoding::default() },
        Some(&mut marks),
    )
    .unwrap();
    (output, marks)
}

/// The boxes inside `range` of `bytes`, as (four letters, start of the payload, end).
fn boxes(bytes: &[u8], range: std::ops::Range<usize>) -> Vec<([u8; 4], usize, usize)> {
    let mut found = Vec::new();
    let mut at = range.start;
    while at + 8 <= range.end {
        // A size of 1 is followed by the size in 64 bits (ISO/IEC 14496-12, 4.2).
        let (size, head) = match u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) {
            1 => (u64::from_be_bytes(bytes[at + 8..at + 16].try_into().unwrap()) as usize, 16),
            size => (size as usize, 8),
        };
        assert!(size >= head && at + size <= range.end, "a box of {size} bytes at {at}");
        found.push((bytes[at + 4..at + 8].try_into().unwrap(), at + head, at + size));
        at += size;
    }
    found
}

fn child(bytes: &[u8], range: std::ops::Range<usize>, name: &[u8; 4]) -> std::ops::Range<usize> {
    boxes(bytes, range)
        .into_iter()
        .find(|(four, _, _)| four == name)
        .map(|(_, start, end)| start..end)
        .unwrap_or_else(|| panic!("no `{}` box", String::from_utf8_lossy(name)))
}

/// What the file's own bytes say: the compatible brands of `ftyp`, and for each track the four letters of its
/// first sample entry and, inside it, the payload of its configuration box (`hvcC` or `avcC`), when it has one.
struct Marks {
    brands: Vec<[u8; 4]>,
    entries: Vec<([u8; 4], Option<Vec<u8>>)>,
}

fn marks_of(path: &Path) -> Marks {
    let bytes = fs::read(path).unwrap();
    let ftyp = child(&bytes, 0..bytes.len(), b"ftyp");
    // Major brand, minor version, then the compatible brands (ISO/IEC 14496-12, 4.3).
    let brands = bytes[ftyp.start + 8..ftyp.end]
        .chunks(4)
        .map(|four| four.try_into().unwrap())
        .collect();
    let moov = child(&bytes, 0..bytes.len(), b"moov");
    let mut entries = Vec::new();
    for (name, start, end) in boxes(&bytes, moov) {
        if &name != b"trak" {
            continue;
        }
        let mdia = child(&bytes, start..end, b"mdia");
        let minf = child(&bytes, mdia, b"minf");
        let stbl = child(&bytes, minf, b"stbl");
        let stsd = child(&bytes, stbl, b"stsd");
        // Version and flags, then the count of entries; the first entry follows.
        let entry_at = stsd.start + 8;
        let size = u32::from_be_bytes(bytes[entry_at..entry_at + 4].try_into().unwrap()) as usize;
        let four: [u8; 4] = bytes[entry_at + 4..entry_at + 8].try_into().unwrap();
        // A visual sample entry holds 78 bytes of its own after its head (ISO/IEC 14496-12, 12.1.3), then its boxes.
        let config = if &four == b"hvc1" || &four == b"hev1" || &four == b"avc1" {
            let inner = entry_at + 8 + 78..entry_at + size;
            boxes(&bytes, inner)
                .into_iter()
                .find(|(name, _, _)| name == b"hvcC" || name == b"avcC")
                .map(|(_, start, end)| bytes[start..end].to_vec())
        } else {
            None
        };
        entries.push((four, config));
    }
    Marks { brands, entries }
}

#[test]
fn an_hevc_film_is_marked_hvc1_with_its_parameter_sets_in_hvcc_and_the_brand_hvc1() {
    let (film, _) = rendered("mark", Codec::Hevc);
    let marks = marks_of(&film);
    let pictures: Vec<_> = marks.entries.iter().filter(|(four, _)| four != b"mp4a").collect();
    assert_eq!(pictures.len(), 1, "one picture track");
    assert_eq!(&pictures[0].0, b"hvc1");
    assert!(marks.brands.contains(b"hvc1"), "the brand hvc1 is among the compatible brands");
    let hvcc = pictures[0].1.as_ref().expect("an hvcC box in the sample entry");
    assert_eq!(hvcc[0], 1, "configuration version 1");
    assert_eq!(hvcc[1] & 0x1f, 1, "general profile 1, Main");
    assert_eq!(hvcc[16] & 0x03, 1, "chroma format 1, 4:2:0");
    assert_eq!(hvcc[17] & 0x07, 0, "luma of 8 bits");
    assert_eq!(hvcc[18] & 0x07, 0, "chroma of 8 bits");
    // The arrays of parameter sets: video (32), sequence (33) and picture (34) are all there (8.4.1, `hvc1`).
    let mut at = 23;
    let mut types = Vec::new();
    for _ in 0..hvcc[22] {
        types.push(hvcc[at] & 0x3f);
        let count = u16::from_be_bytes([hvcc[at + 1], hvcc[at + 2]]);
        at += 3;
        for _ in 0..count {
            let length = u16::from_be_bytes([hvcc[at], hvcc[at + 1]]) as usize;
            at += 2 + length;
        }
    }
    assert_eq!(at, hvcc.len(), "the arrays fill the record");
    for wanted in [32, 33, 34] {
        assert!(types.contains(&wanted), "parameter sets of type {wanted} in {types:?}");
    }
}

#[test]
fn the_probe_of_an_hevc_film_shows_one_hevc_stream_of_8_bits_marked_bt709() {
    let (film, _) = rendered("probe", Codec::Hevc);
    let info = probe::look(&film, &formats()).unwrap();
    let pictures: Vec<_> = info.tracks.iter().filter(|found| found.track.video().is_some()).collect();
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].track.codec.name(), "hevc");
    assert_eq!(pictures[0].entry, Some(*b"hvc1"));
    let facts = pictures[0].facts.expect("the headers of the stream are read");
    assert_eq!(facts.bits, 8);
    assert_eq!(format!("{:?}", facts.chroma), "Yuv420");
    assert_eq!(facts.color, Some(Color::BT709));
    let text = info.text();
    assert!(text.contains("sample entry: hvc1"), "{text}");
    assert!(text.contains("headers: 8 bits, 4:2:0"), "{text}");
}

#[test]
fn every_picture_and_all_the_sound_of_an_hevc_film_decode_to_the_end() {
    let (film, _) = rendered("to-the-end", Codec::Hevc);
    let mut pictures = Pictures::open(&film, &formats(), &codecs(), Hardware::Any).unwrap();
    assert_eq!(pictures.count(), 60);
    let mut numbers = Vec::new();
    let mut last_pts = i64::MIN;
    while let Some(numbered) = pictures.next_picture().unwrap() {
        assert!(numbered.frame.pts > last_pts, "times increase");
        last_pts = numbered.frame.pts;
        numbers.push(numbered.number);
    }
    assert_eq!(numbers, (0..60).collect::<Vec<u64>>());
    let mut source = codecs().audio_source(&film).unwrap();
    assert_eq!((source.channels(), source.sample_rate()), (2, 48_000));
    let mut piece = vec![0.0_f32; 4096 * 2];
    let mut frames = 0_u64;
    loop {
        let read = source.read(&mut piece).unwrap();
        if read == 0 {
            break;
        }
        frames += read as u64;
    }
    assert_eq!(frames, 2 * 48_000, "two seconds of sound, to the sample");
}

#[test]
fn the_frame_marks_of_an_hevc_film_and_an_h264_film_of_one_scene_are_the_same() {
    let (_, hevc) = rendered("marks-hevc", Codec::Hevc);
    let (_, h264) = rendered("marks-h264", Codec::H264);
    assert_eq!(hevc.len(), 60);
    assert_eq!(hevc, h264);
    // The boxes move, so the marks are not all one: the comparison is of frames that differ.
    assert!(hevc.windows(2).any(|pair| pair[0] != pair[1]));
}

#[test]
fn a_film_written_with_no_codec_named_is_h264_marked_avc1() {
    let folder = project_with(GROUP, "default-in", MOVING);
    let scene = Scene::from_project(&Project::open(&folder).unwrap(), &codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let output = scratch(GROUP, "default-out").join("film.mp4");
    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    let marks = marks_of(&output);
    let entries: Vec<[u8; 4]> = marks.entries.iter().map(|(four, _)| *four).collect();
    assert_eq!(entries, [*b"avc1", *b"mp4a"]);
    assert!(!marks.brands.contains(b"hvc1"));
}
