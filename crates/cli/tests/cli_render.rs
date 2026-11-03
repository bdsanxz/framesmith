//! T011–T014 end-to-end: CPU text over color and video, exact timing,
//! sound passthrough, standard MP4 output, and failure handling.
//!
//! All fixtures use the host `ffmpeg`/`ffprobe` (like `cli_foundation`)
//! and H.264/AAC only — the encoder pair every suitable FFmpeg provides.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-render-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

fn ffprobe_streams(mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,height,pix_fmt,sample_rate,channels",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn decode_raw(mp4: &Path, frames: usize) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-frames:v",
            &frames.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode mp4");
    assert!(out.status.success());
    out.stdout
}

/// Mean channel values of one raw RGB24 frame.
fn mean_rgb(frame: &[u8]) -> (f64, f64, f64) {
    let n = frame.len() / 3;
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for i in 0..n {
        r += frame[3 * i] as u64;
        g += frame[3 * i + 1] as u64;
        b += frame[3 * i + 2] as u64;
    }
    (r as f64 / n as f64, g as f64 / n as f64, b as f64 / n as f64)
}

/// Pixels far from `bg` (text/video marks survive yuv420p roundtrips).
fn marked_pixels(frame: &[u8], bg: (u8, u8, u8)) -> usize {
    frame
        .chunks_exact(3)
        .filter(|p| {
            (p[0] as i32 - bg.0 as i32).abs()
                + (p[1] as i32 - bg.1 as i32).abs()
                + (p[2] as i32 - bg.2 as i32)
                > 100
        })
        .count()
}

fn mean_volume_db(wav: &Path) -> f64 {
    let out = Command::new("ffmpeg")
        .args([
            "-v",
            "info",
            "-i",
            &wav.display().to_string(),
            "-af",
            "volumedetect",
            "-f",
            "null",
            "-",
        ])
        .output()
        .expect("volumedetect");
    let text = String::from_utf8_lossy(&out.stderr);
    for line in text.lines() {
        if let Some(pos) = line.find("mean_volume:") {
            let num: String = line[pos + "mean_volume:".len()..]
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            if let Ok(v) = num.parse::<f64>() {
                return v;
            }
        }
    }
    panic!("no mean_volume in volumedetect output: {text}");
}

fn to_wav(mp4: &Path, wav: &Path) {
    ffmpeg(&[
        "-i",
        &mp4.display().to_string(),
        "-map",
        "0:a",
        &wav.display().to_string(),
    ]);
}

#[test]
fn text_on_color_renders_standard_mp4() {
    let root = tmp("color");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    assert!(mp4.is_file());
    // T014: one standard MP4 — H.264 yuv420p video + AAC stereo 48 kHz.
    let streams = ffprobe_streams(&mp4);
    for want in [
        "codec_name=h264",
        "width=160",
        "height=120",
        "pix_fmt=yuv420p",
        "codec_name=aac",
        "sample_rate=48000",
        "channels=2",
    ] {
        assert!(streams.contains(want), "{streams}");
    }
    // T011: the text mark is actually drawn over the background.
    let raw = decode_raw(&mp4, 1);
    assert!(marked_pixels(&raw, (0x11, 0x22, 0x33)) > 50, "text must be drawn");
}

#[test]
fn reruns_decode_byte_identical() {
    let root = tmp("determinism");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=1 fps=30 background=\"#001100\" version=1 {\n    text value=\"AB\" start=0 duration=0.5 x=5 y=5 size=16 color=\"#ffffff\"\n    text value=\"CD\" start=0.25 duration=0.75 x=5 y=30 size=16 color=\"#ff0000\" align=\"center\" wrap=80\n}\n",
    );
    assert_eq!(run(&["render", "proj", "-o", "a.mp4"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "b.mp4"], &root).0, 0);
    // T010/CAP-010: decoded frames are byte-identical across runs.
    let a = decode_raw(&root.join("a.mp4"), 30);
    let b = decode_raw(&root.join("b.mp4"), 30);
    assert_eq!(a.len(), b.len());
    assert_eq!(a, b);
}

#[test]
fn overlapping_text_paints_in_document_order() {
    let root = tmp("paintorder");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Same spot, same window: the later document entry wins the overlap.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    text value=\"AA\" start=0 duration=1 x=4 y=4 size=16 color=\"#ffffff\"\n    text value=\"AA\" start=0 duration=1 x=4 y=4 size=16 color=\"#ff0000\"\n}\n",
    );
    assert_eq!(run(&["render", "proj"], &root).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 1);
    let mut bright = 0;
    for p in raw.chunks_exact(3) {
        let dist = p[0] as i32 + p[1] as i32 + p[2] as i32;
        if dist > 100 {
            bright += 1;
            // White undercoat would leave green/blue behind; red wins.
            assert!(p[1] < 150 && p[2] < 150, "overpaint must be red: {p:?}");
        }
    }
    assert!(bright > 20, "red text must be drawn");
}

#[test]
fn clip_windows_gate_text_visibility() {
    let root = tmp("windows");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    text value=\"AA\" start=0 duration=0.5 x=4 y=4 size=16 color=\"#ffffff\"\n}\n",
    );
    assert_eq!(run(&["render", "proj"], &root).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 30);
    let fs = 96 * 64 * 3;
    assert_eq!(raw.len(), 30 * fs);
    // Half-open [0, 0.5): frames 0..15 show text, 15..30 do not.
    for k in [0, 7, 14] {
        assert!(marked_pixels(&raw[k * fs..(k + 1) * fs], (0, 0, 0)) > 20, "frame {k}");
    }
    for k in [15, 20, 29] {
        assert_eq!(marked_pixels(&raw[k * fs..(k + 1) * fs], (0, 0, 0)), 0, "frame {k}");
    }
}

/// Solid-color source: frame `j` is `cols[j]` at `src_fps`.
fn solid_source(path: &Path, cols: &[(u8, u8, u8)], w: u32, h: u32, fps: u32) {
    let mut raw = Vec::with_capacity(cols.len() * w as usize * h as usize * 3);
    for c in cols {
        raw.extend(std::iter::repeat(*c).take(w as usize * h as usize).flat_map(|p| [p.0, p.1, p.2]));
    }
    let raw_path = path.with_extension("raw");
    std::fs::write(&raw_path, &raw).unwrap();
    ffmpeg(&[
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        &format!("{w}x{h}"),
        "-r",
        &fps.to_string(),
        "-i",
        &raw_path.display().to_string(),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-r",
        &fps.to_string(),
        &path.display().to_string(),
    ]);
}

#[test]
fn video_fits_place_picture() {
    let root = tmp("fits");
    // Solid red source; blue canvas with a different aspect ratio.
    let src = root.join("red.mp4");
    solid_source(&src, &[(255, 0, 0); 30], 160, 120, 30);
    for (fit, corner_red) in [("contain", false), ("cover", true), ("fill", true)] {
        let folder = root.join(format!("fit-{fit}"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&src, folder.join("clip.mp4")).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=320 height=200 duration=1 fps=30 background=\"#0000ff\" version=1 {{\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"{fit}\"\n}}\n"
            ),
        );
        let (code, out) = run(&["render", "."], &folder);
        assert_eq!(code, 0, "{out}");
        let raw = decode_raw(&folder.join("out.mp4"), 1);
        // Corner pixel: letterbox blue for contain, red picture otherwise.
        let (r, g, b) = (raw[0] as i32, raw[1] as i32, raw[2] as i32);
        if corner_red {
            assert!(r > 150 && g < 150 && b < 150, "fit={fit}: corner {r},{g},{b}");
        } else {
            assert!(b > 150 && r < 150, "fit={fit}: corner {r},{g},{b}");
        }
    }
    // A misspelled fit never stretches silently.
    let folder = root.join("badfit");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(&src, folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"zoom\"\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_FIT"), "{out}");
}

#[test]
fn differing_fps_has_no_drift() {
    let root = tmp("fpsdrift");
    // 15 distinct colors at 15fps; 30fps output must show floor(k/2) exactly.
    let cols: Vec<(u8, u8, u8)> = (0..15)
        .map(|j| ((j * 37 % 256) as u8, (j * 91 % 256) as u8, (j * 53 % 256) as u8))
        .collect();
    let src = root.join("cycle.mp4");
    solid_source(&src, &cols, 64, 48, 15);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(&src, folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"fill\"\n}\n",
    );
    assert_eq!(run(&["render", "proj"], &root).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 30);
    let fs = 64 * 48 * 3;
    for k in 0..30usize {
        let (r, g, b) = mean_rgb(&raw[k * fs..(k + 1) * fs]);
        let (er, eg, eb) = cols[k / 2];
        let d = (r - er as f64).abs() + (g - eg as f64).abs() + (b - eb as f64).abs();
        assert!(d < 60.0, "frame {k}: got ({r:.0},{g:.0},{b:.0}) want {er},{eg},{eb}");
    }
    // Same-folder rerun decodes identically.
    assert_eq!(run(&["render", "proj", "-o", "b.mp4"], &root).0, 0);
    assert_eq!(raw, decode_raw(&root.join("b.mp4"), 30));
}

#[test]
fn higher_fps_source_downsamples_exactly() {
    let root = tmp("fpsdown");
    // Alternating red/green at 60fps; 30fps output lands on even frames.
    let cols: Vec<(u8, u8, u8)> = (0..60).map(|j| if j % 2 == 0 { (255, 0, 0) } else { (0, 255, 0) }).collect();
    let src = root.join("alt.mp4");
    solid_source(&src, &cols, 64, 48, 60);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(&src, folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"fill\"\n}\n",
    );
    assert_eq!(run(&["render", "proj"], &root).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 30);
    let fs = 64 * 48 * 3;
    for k in 0..30usize {
        let (r, g, _b) = mean_rgb(&raw[k * fs..(k + 1) * fs]);
        assert!(r > 150.0 && g < 150.0, "frame {k}: got ({r:.0},{g:.0})");
    }
}

#[test]
fn audio_passes_through_unchanged() {
    let root = tmp("audio");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x48:rate=30:duration=1",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-shortest",
        &root.join("src.mp4").display().to_string(),
    ]);
    to_wav(&root.join("src.mp4"), &root.join("src.wav"));
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(root.join("src.mp4"), folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"fill\"\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    // T013: the output carries sound for the rendered range, unmixed —
    // same loudness as the source, no volume filters in the pipeline.
    to_wav(&mp4, &root.join("out.wav"));
    let src_db = mean_volume_db(&root.join("src.wav"));
    let out_db = mean_volume_db(&root.join("out.wav"));
    assert!((src_db - out_db).abs() < 3.0, "src {src_db} vs out {out_db}");
}

#[test]
fn silent_source_renders_with_default_audio() {
    let root = tmp("silent");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x48:rate=30:duration=1",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-an",
        &root.join("mute.mp4").display().to_string(),
    ]);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(root.join("mute.mp4"), folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"fill\"\n}\n",
    );
    // T013: no audible input renders with the encoder default, not a failure.
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(ffprobe_streams(&folder.join("out.mp4")).contains("codec_name=aac"));
}

#[test]
fn corrupt_source_refuses_naming_file() {
    let root = tmp("corrupt");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("clip.mp4"), b"this is not a video file").unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"contain\"\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("clip.mp4"), "{out}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn short_source_refuses_naming_file() {
    let root = tmp("short");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x48:rate=30:duration=0.5",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &root.join("half.mp4").display().to_string(),
    ]);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(root.join("half.mp4"), folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"cover\"\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("clip.mp4"), "{out}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn unsupported_codec_refuses_naming_file() {
    let root = tmp("codec");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x48:rate=30:duration=1",
        "-c:v",
        "mpeg4",
        &root.join("old.mp4").display().to_string(),
    ]);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(root.join("old.mp4"), folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"contain\"\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("clip.mp4"), "{out}");
    assert!(out.contains("mpeg4"), "{out}");
    assert!(!folder.join("out.mp4").exists());
}

/// Corrupt the AAC decoder config (`esds` tag 5 payload) of `mp4` in
/// place: the container still probes both streams, pictures still decode,
/// but the audio stream cannot be opened.
fn damage_aac_config(mp4: &Path) {
    let mut buf = std::fs::read(mp4).unwrap();
    let e = buf
        .windows(4)
        .position(|w| w == b"esds")
        .expect("mixed fixture must carry an esds box")
        + 8;
    // Read one descriptor tag + continuation-coded length.
    fn tag_len(buf: &[u8], mut o: usize) -> (u8, usize, usize) {
        let t = buf[o];
        o += 1;
        let mut ln = 0usize;
        loop {
            let b = buf[o];
            o += 1;
            ln = (ln << 7) | ((b & 0x7f) as usize);
            if b & 0x80 == 0 {
                break;
            }
        }
        (t, ln, o)
    }
    let (_, _, o) = tag_len(&buf, e);
    let (_, _, o2) = tag_len(&buf, o + 3);
    let (t, ln, o3) = tag_len(&buf, o2 + 13);
    assert_eq!(t, 5, "expected the AAC DecoderSpecificInfo tag");
    for b in buf.iter_mut().skip(o3).take(ln) {
        *b = 0xFF;
    }
    std::fs::write(mp4, &buf).unwrap();
}

#[test]
fn undecodable_audio_refuses_without_silent_track() {
    let root = tmp("badaudio");
    // Valid pictures with a damaged audio decoder config.
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x48:rate=30:duration=1",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-shortest",
        &root.join("mix.mp4").display().to_string(),
    ]);
    damage_aac_config(&root.join("mix.mp4"));
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(root.join("mix.mp4"), folder.join("clip.mp4")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 fit=\"contain\"\n}\n",
    );
    // T013: undecodable audio refuses naming the file; no silent track.
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("clip.mp4"), "{out}");
    assert!(out.contains("audio"), "{out}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn encode_failure_reports_stage_and_writes_no_mp4() {
    let root = tmp("encfail");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    text value=\"Hi\" start=0 duration=2\n}\n",
    );
    // Cap the output file size so the encoder dies mid-write.
    let out = Command::new("sh")
        .args([
            "-c",
            &format!("ulimit -f 4; exec {} render proj -o out.mp4", bin().display()),
        ])
        .current_dir(&root)
        .output()
        .expect("run framesmith under a file-size limit");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code().unwrap_or(-1), 1, "{text}");
    // T014: the failure reports the FFmpeg stage and leaves no MP4.
    assert!(text.contains("encode stage"), "{text}");
    assert!(!folder.join("out.mp4").exists(), "{text}");
    assert!(
        !folder.join("out.mp4.framesmith-part").exists(),
        "no truncated temp file may survive: {text}"
    );
}

/// A host TrueType font for the project-font render path, or `None`
/// when the machine carries none of the well-known candidates.
fn system_ttf() -> Option<PathBuf> {
    for cand in [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Verdana.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        let p = PathBuf::from(cand);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[test]
fn project_font_renders_with_attributes() {
    let Some(ttf) = system_ttf() else {
        // No host TrueType font to borrow: nothing to prove here.
        return;
    };
    let root = tmp("projfont");
    let folder = root.join("proj");
    std::fs::create_dir_all(folder.join("fonts")).unwrap();
    std::fs::copy(&ttf, folder.join("fonts").join("Body.ttf")).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=192 height=128 duration=1 fps=30 background=\"#101010\" version=1 {\n    font path=\"fonts/Body.ttf\"\n    text value=\"Hello there maker\" start=0 duration=1 x=8 y=8 size=28 color=\"#ffff00\" align=\"center\" wrap=176\n    text value=\"Second\" start=0.5 duration=0.5 x=8 y=90 size=20 color=\"#00ffff\" position=\"8,90\"\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("built-in default face"), "{out}");
    let raw = decode_raw(&folder.join("out.mp4"), 1);
    assert!(marked_pixels(&raw, (0x10, 0x10, 0x10)) > 100, "project font must draw");
}

/// Two project fonts of different families, or skip when the machine
/// carries no such pair.
fn two_family_ttfs() -> Option<((PathBuf, &'static str), (PathBuf, &'static str))> {
    for pair in [
        (
            (
                "/System/Library/Fonts/Supplemental/Arial.ttf",
                "Arial",
            ),
            (
                "/System/Library/Fonts/Supplemental/Andale Mono.ttf",
                "Andale Mono",
            ),
        ),
        (
            (
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "DejaVu Sans",
            ),
            (
                "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
                "DejaVu Serif",
            ),
        ),
    ] {
        let ((pa, fa), (pb, fb)) = pair;
        if PathBuf::from(pa).is_file() && PathBuf::from(pb).is_file() {
            return Some(((PathBuf::from(pa), fa), (PathBuf::from(pb), fb)));
        }
    }
    None
}

/// `font-family` selects the named project font: the same text renders
/// different pixels under two families, and `check` stays green.
#[test]
fn font_family_selects_project_family() {
    let Some(((pa, _fa), (pb, fb))) = two_family_ttfs() else {
        return;
    };
    let root = tmp("fontfam");
    for (tag, family_attr) in [
        ("default", String::new()),
        ("named", format!(" font-family=\"{fb}\"")),
    ] {
        let folder = root.join(tag);
        std::fs::create_dir_all(folder.join("fonts")).unwrap();
        std::fs::copy(&pa, folder.join("fonts").join("A.ttf")).unwrap();
        std::fs::copy(&pb, folder.join("fonts").join("B.ttf")).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=192 height=64 duration=1 fps=30 background=\"#101010\" version=1 {{\n    font path=\"fonts/A.ttf\"\n    font path=\"fonts/B.ttf\"\n    text value=\"Hello maker\" start=0 duration=1 x=8 y=8 size=28 color=\"#ffff00\"{family_attr}\n}}\n"
            ),
        );
        let (code, out) = run(&["check", tag], &root);
        assert_eq!(code, 0, "{out}");
        let (code, out) = run(&["render", tag], &root);
        assert_eq!(code, 0, "{out}");
    }
    let raw_default = decode_raw(&root.join("default").join("out.mp4"), 1);
    let raw_named = decode_raw(&root.join("named").join("out.mp4"), 1);
    assert_ne!(
        raw_default, raw_named,
        "two families must draw different pixels"
    );
    assert!(
        marked_pixels(&raw_named, (0x10, 0x10, 0x10)) > 100,
        "named family must draw"
    );
}

/// An explicit `font-family` nothing resolves refuses before the first
/// frame and writes no MP4.
#[test]
fn unknown_font_family_refuses() {
    let root = tmp("fontfamunknown");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=1 font-family=\"NoSuchFamilyXYZ, AlsoMissing\"\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("NoSuchFamilyXYZ"), "{out}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn missing_font_and_bad_font_refuse() {
    let root = tmp("fonts");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 background=\"#000\" version=1 {\n    font path=\"fonts/gone.ttf\"\n    text value=\"Hi\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_MISSING_FILE"), "{out}");
    // WOFF is a real font wrapper the CPU renderer cannot draw.
    std::fs::create_dir_all(folder.join("fonts")).unwrap();
    std::fs::write(folder.join("fonts").join("web.woff"), b"wOFFjunkjunk").unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 background=\"#000\" version=1 {\n    font path=\"fonts/web.woff\"\n    text value=\"Hi\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_FONT"), "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!folder.join("out.mp4").exists());
}

fn run_env(args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

const SYSFONT_COMP: &str = "composition name=\"t\" width=640 height=360 duration=1 fps=30 background=\"#1a1a2e\" version=1 {\n    text value=\"Hello from Framesmith\" start=0 duration=1 x=70 y=140 size=48 color=\"#ffffff\"\n}\n";

/// No declared `font`: render must name the face origin and draw text.
#[test]
fn default_face_names_its_origin_and_draws() {
    let root = tmp("sysfont");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", SYSFONT_COMP);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("using system font ") || out.contains("no system font found"),
        "render must announce the default face origin: {out}"
    );
    let raw = decode_raw(&folder.join("out.mp4"), 1);
    assert!(
        marked_pixels(&raw, (0x1a, 0x1a, 0x2e)) > 100,
        "default face must draw readable text"
    );
}

/// `font-style` accepts only the four known spellings.
#[test]
fn font_style_must_be_known() {
    let root = tmp("fontstyle");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=30 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=1 font-style=\"black\"\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    assert!(out.contains("font-style"), "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!folder.join("out.mp4").exists());
}


/// Skip style tests on machines without a bold system variant.
fn has_bold_variant() -> bool {
    [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .any(|p| p.is_file())
}

fn style_comp(style: &str) -> String {
    format!(
        "composition name=\"t\" width=640 height=360 duration=1 fps=30 background=\"#1a1a2e\" version=1 {{\n    text value=\"Hello from Framesmith\" start=0 duration=1 x=70 y=140 size=48 color=\"#ffffff\" font-style=\"{style}\"\n}}\n"
    )
}

fn render_style(root: &Path, tag: &str, style: &str) -> (Vec<u8>, String) {
    let folder = root.join(tag);
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", &style_comp(style));
    let (code, out) = run(&["render", tag], root);
    assert_eq!(code, 0, "{out}");
    (decode_raw(&folder.join("out.mp4"), 1), out)
}

/// System `bold` resolves the real variant file and draws heavier.
#[test]
fn font_style_bold_uses_variant_and_adds_ink() {
    if !has_bold_variant() {
        return;
    }
    let root = tmp("fstylebold");
    let (raw_regular, _) = render_style(&root, "regular", "regular");
    let (raw_bold, out_bold) = render_style(&root, "bold", "bold");
    assert!(out_bold.contains("Arial Bold.ttf"), "{out_bold}");
    let ink = |r: &[u8]| r.iter().map(|b| *b as u64).sum::<u64>();
    assert!(
        ink(&raw_bold) > ink(&raw_regular),
        "bold must put down more ink than regular"
    );
}

/// System `italic` slants the text instead of drawing upright.
#[test]
fn font_style_italic_slants() {
    if !has_bold_variant() {
        return;
    }
    let root = tmp("fstyleitalic");
    let (raw_regular, _) = render_style(&root, "regular", "regular");
    let (raw_italic, out_italic) = render_style(&root, "italic", "italic");
    assert!(out_italic.contains("Arial Italic.ttf"), "{out_italic}");
    assert_ne!(raw_italic, raw_regular, "italic must differ from upright");
}

/// `bold-italic` combines both variant selection and synthesis.
#[test]
fn font_style_bold_italic_combines() {
    if !has_bold_variant() {
        return;
    }
    let root = tmp("fstylebi");
    let (raw_regular, _) = render_style(&root, "regular", "regular");
    let (raw_bi, out_bi) = render_style(&root, "bi", "bold-italic");
    assert!(out_bi.contains("Arial Bold Italic.ttf"), "{out_bi}");
    assert_ne!(raw_bi, raw_regular, "bold-italic must differ from upright");
}

/// A declared project font has no variant file to pick, so `bold`
/// is synthesized (faux bold) and still draws heavier.
#[test]
fn font_style_project_font_faux_bold() {
    let src = [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file());
    let Some(src) = src else {
        return;
    };
    let root = tmp("fstyleproj");
    std::fs::create_dir_all(root.join("proj").join("fonts")).unwrap();
    std::fs::copy(&src, root.join("proj").join("fonts").join("Body.ttf")).unwrap();
    let mk = |style: &str| {
        format!(
            "composition name=\"t\" width=320 height=180 duration=1 fps=30 background=\"#101010\" version=1 {{\n    font path=\"fonts/Body.ttf\"\n    text value=\"Heavy\" start=0 duration=1 x=8 y=8 size=48 color=\"#ffff00\" font-style=\"{style}\"\n}}\n"
        )
    };
    for (tag, style) in [("regular", "regular"), ("bold", "bold")] {
        let folder = root.join(tag);
        std::fs::create_dir_all(folder.join("fonts")).unwrap();
        std::fs::copy(&src, folder.join("fonts").join("Body.ttf")).unwrap();
        write(&folder, "composition.kdl", &mk(style));
        let (code, out) = run(&["render", tag], &root);
        assert_eq!(code, 0, "{out}");
        assert!(!out.contains("system font"), "{out}");
    }
    let ink = |tag: &str| {
        let raw = decode_raw(&root.join(tag).join("out.mp4"), 1);
        raw.iter().map(|b| *b as u64).sum::<u64>()
    };
    assert!(ink("bold") > ink("regular"), "faux bold must add ink");
}

/// `FRAMESMITH_SYSTEM_FONT` overrides the host candidate list.
#[test]
fn system_font_override_is_honored() {
    let src = [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file());
    let Some(src) = src else {
        return;
    };
    let root = tmp("sysfontoverride");
    std::fs::copy(&src, root.join("Custom.ttf")).unwrap();
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", SYSFONT_COMP);
    let custom = root.join("Custom.ttf").display().to_string();
    let (code, out) = run_env(&["render", "proj"], &root, &[("FRAMESMITH_SYSTEM_FONT", &custom)]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("using system font Custom.ttf"), "{out}");
    let raw = decode_raw(&folder.join("out.mp4"), 1);
    assert!(
        marked_pixels(&raw, (0x1a, 0x1a, 0x2e)) > 100,
        "override face must draw readable text"
    );
}

/// Sprint 2 still life (T004/T005): a rounded bordered box behind a
/// `contain` PNG and a `cover` JPEG renders to a standard MP4 whose
/// first frame carries the box fill, the border, and both pictures.
#[test]
fn sprint2_box_and_images_render_to_standard_mp4() {
    let root = tmp("stilllife");
    let folder = root.join("proj");
    std::fs::create_dir_all(folder.join("media")).unwrap();
    // Deterministic still sources: a red/green split PNG and a blue JPEG.
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc=size=64x48:rate=1:duration=1",
        "-frames:v", "1",
        &folder.join("media/photo.png").display().to_string(),
    ]);
    ffmpeg(&[
        "-f", "lavfi", "-i", "color=c=blue:size=48x64:rate=1:duration=1",
        "-frames:v", "1",
        &folder.join("media/photo.jpg").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=1 background=\"#101010\" version=1 {\n    box x=20 y=20 width=280 height=200 fill=\"#ff0000\" radius=12 border-width=4 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n    image path=\"media/photo.png\" x=40 y=40 width=120 height=120 fit=\"contain\" placement=\"top-left\" order=2 start=0 duration=1\n    image path=\"media/photo.jpg\" x=160 y=40 width=120 height=120 fit=\"cover\" placement=\"center\" order=3 start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    assert!(mp4.is_file());
    let streams = ffprobe_streams(&mp4);
    assert!(streams.contains("h264"), "{streams}");
    assert!(streams.contains("yuv420p"), "{streams}");
    assert!(streams.contains("aac"), "{streams}");
    let raw = decode_raw(&mp4, 1);
    assert_eq!(raw.len(), 320 * 240 * 3);
    // Box fill dominates the frame: strongly red on average.
    let (r, g, b) = mean_rgb(&raw);
    assert!(r > 100.0 && r > g + 30.0 && r > b + 30.0, "red box must dominate: {r},{g},{b}");
    // The border ring leaves near-white marks; the pictures add variety.
    assert!(marked_pixels(&raw, (0x10, 0x10, 0x10)) > 1000, "box, border and pictures must mark the frame");
}

/// A PNG-signature file with a corrupt body passes the magic check but
/// refuses at render time, naming the file with no MP4 (REC-005).
#[test]
fn corrupt_body_image_refuses_at_render() {
    let root = tmp("corruptimg");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    let mut bad = b"\x89PNG\r\n\x1a\n".to_vec();
    bad.extend_from_slice(b"this is not png body data at all, just trailing junk");
    std::fs::write(folder.join("bad.png"), &bad).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=1 background=\"#000\" version=1 {\n    image path=\"bad.png\" start=0 duration=1\n}\n",
    );
    // Magic bytes pass validation; the decode refuses at render.
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("bad.png"), "refusal names the file: {out}");
    assert!(!folder.join("out.mp4").exists());
    assert!(!folder.join("out.mp4.framesmith-part").exists());
}

/// T004: a successful render produces one complete MP4 at the target
/// with only the MP4 plus the run log record remaining — no staged
/// (`.framesmith-part`) or probe (`.framesmith-write-probe`) files.
#[test]
fn success_writes_one_complete_mp4_without_leftovers() {
    let root = tmp("atomicsuccess");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    assert!(mp4.is_file());
    assert!(mp4.metadata().unwrap().len() > 0, "MP4 must be complete");
    let streams = ffprobe_streams(&mp4);
    assert!(streams.contains("codec_name=h264"), "{streams}");
    let mut leftovers = Vec::new();
    for entry in std::fs::read_dir(&folder).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".framesmith-part") || name == ".framesmith-write-probe" {
            leftovers.push(name);
        }
    }
    assert!(leftovers.is_empty(), "staged/work files remain: {leftovers:?}");
    assert!(folder.join("logs").join("framesmith.log").is_file(), "run log is kept");
}

/// T004: re-rendering a folder replaces the previous MP4 atomically —
/// the old file is never visible half-written and no staged file
/// survives either render.
#[test]
fn rerender_replaces_mp4_atomically_without_leftovers() {
    let root = tmp("atomicrerender");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#445566\" version=1 {\n    text value=\"Again\" start=0 duration=1\n}\n",
    );
    assert_eq!(run(&["render", "proj", "-o", "a.mp4"], &root).0, 0);
    let first = std::fs::read(root.join("a.mp4")).unwrap();
    assert!(!first.is_empty());
    assert_eq!(run(&["render", "proj", "-o", "a.mp4"], &root).0, 0);
    let second = std::fs::read(root.join("a.mp4")).unwrap();
    assert!(!second.is_empty(), "re-render must leave one complete MP4");
    assert_eq!(first, second, "same input re-renders deterministically");
    assert!(!root.join("a.mp4.framesmith-part").exists());
    assert!(!root.join(".framesmith-write-probe").exists());
    assert!(!folder.join(".framesmith-write-probe").exists());
}

/// T004: an output directory that cannot be created (a regular file
/// blocks the path) refuses before the first frame with the path
/// named, leaving no partial MP4 or staged file.
#[test]
fn uncreatable_output_dir_refuses_with_path_named() {
    let root = tmp("atomicmkdir");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n",
    );
    std::fs::write(root.join("blocker"), b"not a directory").unwrap();
    let (code, out) = run(&["render", "proj", "-o", "blocker/out.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("blocker"), "refusal names the path: {out}");
    assert!(out.contains("output stage"), "refusal names the stage: {out}");
    assert!(!root.join("blocker").join("out.mp4").exists());
}

/// T004: an unwritable output directory refuses before the first frame
/// with the path named (output stage), leaving no partial MP4.
#[test]
fn unwritable_output_dir_refuses_with_path_named() {
    if is_root() {
        return;
    }
    let root = tmp("atomicnowrite");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n",
    );
    let outdir = root.join("locked");
    std::fs::create_dir_all(&outdir).unwrap();
    let mut perms = std::fs::metadata(&outdir).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&outdir, perms).unwrap();
    let (code, out) = run(&["render", "proj", "-o", "locked/out.mp4"], &root);
    // Restore so the temp dir can be cleaned up.
    let mut perms = std::fs::metadata(&outdir).unwrap().permissions();
    perms.set_readonly(false);
    std::fs::set_permissions(&outdir, perms).unwrap();
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("locked"), "refusal names the path: {out}");
    assert!(out.contains("output stage"), "refusal names the stage: {out}");
    assert!(!outdir.join("out.mp4").exists());
    assert!(!outdir.join("out.mp4.framesmith-part").exists());
}

/// True when running as uid 0 (permission bits do not apply).
fn is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}
