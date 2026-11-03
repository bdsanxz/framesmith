//! Picture decoding through the host FFmpeg pair (no frx7 media stack).

use ffmpeg::picture::{decode_image, decode_memory, png_size};
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-ffmpeg-picture-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn host_pair() -> Option<(PathBuf, PathBuf)> {
    let ffmpeg = which_ffmpeg()?;
    let probe = ffmpeg.with_file_name("ffprobe");
    if probe.is_file() {
        return Some((ffmpeg, probe));
    }
    let system = PathBuf::from("/opt/homebrew/bin/ffprobe");
    if system.is_file() {
        return Some((ffmpeg, system));
    }
    None
}

fn which_ffmpeg() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("ffmpeg");
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn make_png(dir: &std::path::Path, name: &str, w: u32, h: u32) -> PathBuf {
    let out = dir.join(name);
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size={w}x{h}:rate=1:duration=1"),
            "-frames:v",
            "1",
            &out.display().to_string(),
        ])
        .status()
        .expect("run host ffmpeg");
    assert!(status.success(), "host ffmpeg must write a test png");
    out
}

/// A generated PNG decodes to exact dimensions with non-blank ink.
#[test]
fn png_decodes_to_sized_rgb_with_ink() {
    let Some((ffmpeg_bin, ffprobe_bin)) = host_pair() else {
        return;
    };
    let dir = tmp("png");
    let png = make_png(&dir, "in.png", 64, 48);
    let pic = decode_image(&ffmpeg_bin, &ffprobe_bin, &png).expect("png must decode");
    assert_eq!((pic.width, pic.height), (64, 48));
    assert_eq!(pic.bytes.len(), 64 * 48 * 3);
    assert!(
        pic.bytes.iter().any(|b| *b > 0),
        "test pattern must carry ink"
    );
}

/// A missing file refuses naming the path, never a blank picture.
#[test]
fn missing_picture_refuses_naming_the_file() {
    let dir = tmp("missing");
    let missing = dir.join("gone.png");
    let fake_bin = PathBuf::from("ffmpeg");
    let fake_probe = PathBuf::from("ffprobe");
    let err = decode_image(&fake_bin, &fake_probe, &missing).unwrap_err();
    assert!(err.contains("gone.png"), "{err}");
    assert!(err.contains("does not exist"), "{err}");
}

/// `decode_memory` (the frx7 replacement) decodes in-memory PNG bytes —
/// what an `sbix` strike hands over — to sized straight RGBA.
#[test]
fn memory_png_decodes_to_sized_rgba() {
    let Some((ffmpeg_bin, _)) = host_pair() else {
        return;
    };
    let dir = tmp("mem");
    let png = make_png(&dir, "mem.png", 32, 24);
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(png_size(&bytes), Some((32, 24)));
    let pic = decode_memory(&ffmpeg_bin, &bytes, "U+0041 test").expect("memory png must decode");
    assert_eq!((pic.width, pic.height), (32, 24));
    assert_eq!(pic.bytes.len(), 32 * 24 * 4);
    assert!(
        pic.bytes.iter().any(|b| *b > 0),
        "test pattern must carry ink"
    );
}

/// Non-PNG bytes refuse before any subprocess runs, naming the label.
#[test]
fn memory_garbage_refuses_naming_the_label() {
    let fake_bin = PathBuf::from("ffmpeg");
    let err = decode_memory(&fake_bin, b"not a picture at all", "U+1F600 emoji").unwrap_err();
    assert!(err.contains("U+1F600 emoji"), "{err}");
    assert!(err.contains("not a PNG"), "{err}");
}

/// A corrupt file refuses naming the path, never a blank picture.
#[test]
fn corrupt_picture_refuses_naming_the_file() {
    let Some((ffmpeg_bin, ffprobe_bin)) = host_pair() else {
        return;
    };
    let dir = tmp("corrupt");
    let bad = dir.join("bad.png");
    std::fs::write(&bad, b"not a picture at all").unwrap();
    let err = decode_image(&ffmpeg_bin, &ffprobe_bin, &bad).unwrap_err();
    assert!(err.contains("bad.png"), "{err}");
}
