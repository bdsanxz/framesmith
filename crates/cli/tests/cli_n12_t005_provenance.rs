//! Sprint 12 T005 end-to-end: stamp provenance.
//!
//! Every render stamps the tool name and version plus a digest of the
//! composition inputs: file containers (MP4, HEVC, WebM, ProRes) carry
//! the stamp as a `comment` tag readable through the standard probe,
//! PNG sequences write a sidecar JSON with the same fields, and HLS
//! playlists carry the stamp in a comment. GIF carries neither and
//! warns naming the format without blocking pixels. Stamped outputs
//! render pixel-identical to unstamped equivalents, and any input
//! change re-stamps.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n12t005-{tag}-{}-{}",
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

fn gen_tone(path: &Path, freq: u32) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency={freq}:duration=1"),
        "-ar",
        "48000",
        "-ac",
        "2",
        &path.display().to_string(),
    ]);
}

/// Human composition used by every provenance render: text over color
/// plus an audible tone, so media bytes join the digest.
fn project(root: &Path, text: &str) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {{\n    text value=\"{text}\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n    audio path=\"tone.wav\" start=0 duration=1\n}}\n"
        ),
    );
    folder
}

/// The `comment` tag of one container, empty when the muxer dropped it.
fn container_comment(path: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=comment",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .expect("run host ffprobe");
    assert!(
        out.status.success(),
        "ffprobe failed for {}",
        path.display()
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The `inputs=` digest inside one stamp string.
fn digest_of(stamp: &str) -> String {
    let (_, digest) = stamp
        .split_once("inputs=")
        .unwrap_or_else(|| panic!("stamp carries no digest: {stamp}"));
    assert!(
        digest.len() == 16 && digest.chars().all(|c| c.is_ascii_hexdigit()),
        "digest is 16 hex chars: {stamp}"
    );
    digest.to_string()
}

/// Tool version, exactly as the binary reports it.
fn tool_version() -> String {
    let out = Command::new(bin())
        .args(["--version"])
        .output()
        .expect("run framesmith --version");
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let version = text
        .trim()
        .split_whitespace()
        .last()
        .unwrap_or_else(|| panic!("no version in {text:?}"))
        .to_string();
    assert!(!version.is_empty(), "version parses: {text:?}");
    version
}

/// Decoded RGB24 frames of one video file.
fn decode_rgb(path: &Path, frames: usize, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            "-frames:v",
            &frames.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success(), "decode failed for {}", path.display());
    assert_eq!(out.stdout.len(), frames * w as usize * h as usize * 3);
    out.stdout
}

/// Mean per-channel absolute difference between two raw frames.
fn mean_abs_diff(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let total: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64)
        .sum();
    total as f64 / a.len() as f64
}

#[test]
fn file_formats_probe_to_tool_version_and_digest() {
    let root = tmp("containers");
    project(&root, "Hi");
    let version = tool_version();
    let cases: &[(&str, &[&str], &str)] = &[
        ("mp4", &[], "out.mp4"),
        ("hevc", &[], "out-hevc.mp4"),
        ("webm", &[], "out.webm"),
        ("prores", &["--prores-profile", "hq"], "out.mov"),
    ];
    let mut digests = Vec::new();
    for (format, extra, name) in cases {
        let mut args: Vec<&str> = vec!["render", "proj", "--format", format];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["-o", name]);
        let (code, out) = run(&args, &root);
        assert_eq!(code, 0, "render --format {format}: {out}");
        let stamp = container_comment(&root.join(name));
        assert!(
            stamp.starts_with(&format!("framesmith {version} inputs=")),
            "format {format}: {stamp:?}"
        );
        digests.push(digest_of(&stamp));
    }
    // One composition means one digest on every destination.
    for d in &digests {
        assert_eq!(d, &digests[0], "same inputs stamp one digest");
    }
    // The machine-readable report still names the destination.
    let out = Command::new(bin())
        .args([
            "render", "proj", "--format", "hevc", "-o", "j.mp4", "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("run framesmith binary");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.contains("\"format\":\"hevc\""), "{stdout}");
    assert_eq!(
        digest_of(&container_comment(&root.join("j.mp4"))),
        digests[0]
    );
}

#[test]
fn pngseq_writes_a_sidecar_with_the_same_fields() {
    let root = tmp("sidecar");
    project(&root, "Hi");
    let version = tool_version();
    let (code, out) = run(&["render", "proj", "-o", "s.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let digest = digest_of(&container_comment(&root.join("s.mp4")));
    let (code, out) = run(
        &["render", "proj", "--format", "pngseq", "-o", "stills"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let dir = root.join("stills");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    // The frames directory keeps exactly the numbered frames.
    let expected: Vec<String> = (0..15).map(|i| format!("frame-{i:04}.png")).collect();
    assert_eq!(names, expected);
    // The sidecar rides beside the sequence directory with the same
    // fields the containers carry as tags.
    let sidecar_path = root.join("stills.provenance.json");
    let sidecar = std::fs::read_to_string(&sidecar_path).unwrap();
    assert!(sidecar.contains("\"tool\":\"framesmith\""), "{sidecar}");
    assert!(
        sidecar.contains(&format!("\"version\":\"{version}\"")),
        "{sidecar}"
    );
    assert!(
        sidecar.contains(&format!("\"digest\":\"{digest}\"")),
        "{sidecar}"
    );
    // The sidecar is deterministic: a re-render writes the same bytes.
    let (code, out) = run(
        &["render", "proj", "--format", "pngseq", "-o", "again"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(&sidecar_path).unwrap(),
        std::fs::read(root.join("again.provenance.json")).unwrap()
    );
}

#[test]
fn hls_playlist_carries_the_stamp_in_a_comment() {
    let root = tmp("hls");
    project(&root, "Hi");
    let version = tool_version();
    let (code, out) = run(&["render", "proj", "-o", "s.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let digest = digest_of(&container_comment(&root.join("s.mp4")));
    let (code, out) = run(&["render", "proj", "--format", "hls", "-o", "h1"], &root);
    assert_eq!(code, 0, "{out}");
    let playlist = root.join("h1/index.m3u8");
    let body = std::fs::read_to_string(&playlist).unwrap();
    let stamp_line = body
        .lines()
        .find(|l| l.starts_with("# framesmith "))
        .unwrap_or_else(|| panic!("playlist carries a stamp comment: {body}"));
    assert!(
        stamp_line.contains(&version) && stamp_line.contains(&digest),
        "{stamp_line}"
    );
    // The playlist still verifies: a header, named segments on disk,
    // and decodable pictures.
    assert!(body.contains("#EXTM3U"), "{body}");
    let segs: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    assert!(!segs.is_empty(), "{body}");
    for seg in &segs {
        let seg_path = root.join("h1").join(seg);
        assert!(seg_path.is_file(), "segment written: {body}");
        let raw = decode_rgb(&seg_path, 1, 160, 120);
        assert!(!raw.iter().all(|&b| b == 0), "segment decodes: {seg}");
    }
    // The stamped playlist re-renders byte-identically (software path).
    let (code, out) = run(&["render", "proj", "--format", "hls", "-o", "h2"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(&playlist).unwrap(),
        std::fs::read(root.join("h2/index.m3u8")).unwrap(),
        "stamped playlist re-renders byte-identically"
    );
}

#[test]
fn gif_warns_naming_the_format_without_blocking_pixels() {
    let root = tmp("gif");
    project(&root, "Hi");
    let (code, out) = run(&["render", "proj", "--format", "gif", "-o", "a.gif"], &root);
    assert_eq!(code, 0, "gif still renders: {out}");
    assert!(
        out.contains("--format gif"),
        "warning names the format: {out}"
    );
    assert!(out.contains("unstamped"), "warning says unstamped: {out}");
    assert!(out.contains("provenance"), "warning says provenance: {out}");
    // The animation still covers the full composition.
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &root.join("a.gif").display().to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode gif");
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), 15 * 160 * 120 * 3);
    // GIF re-renders byte-identically (nothing to stamp, nothing drifts).
    let (code, out) = run(&["render", "proj", "--format", "gif", "-o", "b.gif"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("a.gif")).unwrap(),
        std::fs::read(root.join("b.gif")).unwrap()
    );
}

#[test]
fn stamped_outputs_render_pixel_identical_to_unstamped() {
    let root = tmp("pixels");
    project(&root, "Hi");
    let (code, out) = run(&["render", "proj", "-o", "stamped.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(container_comment(&root.join("stamped.mp4")).contains("framesmith"));
    // Strip every container tag without re-encoding: the pictures must
    // not move at all.
    ffmpeg(&[
        "-i",
        &root.join("stamped.mp4").display().to_string(),
        "-c",
        "copy",
        "-map_metadata",
        "-1",
        &root.join("bare.mp4").display().to_string(),
    ]);
    assert!(
        container_comment(&root.join("bare.mp4")).is_empty(),
        "stripped copy carries no stamp"
    );
    let (a, b) = (
        decode_rgb(&root.join("stamped.mp4"), 15, 160, 120),
        decode_rgb(&root.join("bare.mp4"), 15, 160, 120),
    );
    assert_eq!(
        mean_abs_diff(&a, &b),
        0.0,
        "provenance never changes pixels"
    );
}

#[test]
fn digest_changes_with_composition_and_media_inputs() {
    let root = tmp("sensitivity");
    project(&root, "Hi");
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let base = digest_of(&container_comment(&root.join("a.mp4")));
    // A composition edit re-stamps.
    project(&root, "Yo");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let edited = digest_of(&container_comment(&root.join("b.mp4")));
    assert_ne!(edited, base, "composition change re-stamps");
    // A media edit re-stamps even when the composition text is back.
    project(&root, "Hi");
    gen_tone(&root.join("proj/tone.wav"), 880);
    let (code, out) = run(&["render", "proj", "-o", "c.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let swapped = digest_of(&container_comment(&root.join("c.mp4")));
    assert_ne!(swapped, base, "media change re-stamps");
}

#[test]
fn software_rerenders_stamp_byte_identically() {
    let root = tmp("determinism");
    project(&root, "Hi");
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("a.mp4")).unwrap(),
        std::fs::read(root.join("b.mp4")).unwrap(),
        "stamped software re-render is byte-identical"
    );
}
