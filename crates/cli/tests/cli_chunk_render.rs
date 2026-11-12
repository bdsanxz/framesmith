//! Sprint 26 T001: chunk and reassemble in process.
//!
//! A chunked render draws and encodes each chunk independently, then
//! reassembles in order; its decoded pictures, audio statistics, and
//! duration match the single-pass oracle inside the encoder's noise
//! band. Single-pass renders stay byte-identical to themselves, an
//! unplannable split refuses before the first frame, and the plan
//! plus per-chunk receipts print one line per chunk.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-chunk-{}-{}-{}",
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

const COMP: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#112233\" version=1 {\n    box x=10 y=10 width=60 height=40 fill=\"#ff0000\" radius=4 border-width=2 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n    text value=\"Hi\" start=0 duration=1 x=70 y=20 size=20 color=\"#ffffff\"\n}\n";

fn valid_proj(root: &Path, tag: &str) -> PathBuf {
    let folder = root.join(tag);
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", COMP);
    folder
}

fn decode_video(mp4: &Path) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &mp4.display().to_string(), "-f",
            "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success());
    out.stdout
}

fn decode_audio(mp4: &Path) -> Vec<i16> {
    let out = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &mp4.display().to_string(), "-f",
            "s16le", "-ac", "2", "-ar", "48000", "-",
        ])
        .output()
        .expect("decode audio");
    assert!(out.status.success());
    out.stdout
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

fn ffprobe_duration(mp4: &Path) -> f64 {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error", "-show_entries", "format=duration", "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(mp4)
        .output()
        .expect("ffprobe duration");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap()
}

fn quality(a: &[u8], b: &[u8]) -> (u8, f64, f64) {
    // (max abs diff, mean abs diff, PSNR dB): the encoder's own noise
    // band. Chunk encodes look ahead over fewer frames than the
    // single-pass stream, so decoded pictures move by codec noise,
    // never by misalignment.
    assert_eq!(a.len(), b.len(), "decoded streams must align");
    let mut max = 0u8;
    let mut sum = 0u64;
    let mut sq = 0u64;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = x.abs_diff(*y);
        max = max.max(d);
        sum += d as u64;
        sq += d as u64 * d as u64;
    }
    let n = a.len() as f64;
    let mean = sum as f64 / n;
    let mse = sq as f64 / n;
    let psnr = if mse == 0.0 {
        99.0
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    };
    (max, mean, psnr)
}

#[test]
fn chunked_render_matches_single_pass_decoded() {
    let root = tmp("equiv");
    valid_proj(&root, "proj");
    // Two single-pass renders: byte-identical to the baseline.
    let (code, out) = run(
        &["render", "proj", "-o", "single-a.mp4", "--backend", "cpu"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &["render", "proj", "-o", "single-b.mp4", "--backend", "cpu"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("single-a.mp4")).unwrap();
    let b = std::fs::read(root.join("single-b.mp4")).unwrap();
    assert_eq!(a, b, "single-pass renders must stay byte-identical");

    // Chunked render: plan visible before draw, one line per chunk.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "chunked.mp4", "--backend", "cpu",
            "--chunks", "2",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stage chunks: plan"), "{out}");
    assert!(out.contains("chunk 1/2") && out.contains("chunk 2/2"), "{out}");
    assert!(out.contains("chunks=2"), "{out}");

    let single = root.join("single-a.mp4");
    let chunked = root.join("chunked.mp4");
    // Decoded pictures: same frame count, same length, within noise.
    let sv = decode_video(&single);
    let cv = decode_video(&chunked);
    assert_eq!(sv.len(), cv.len(), "chunked must carry every frame");
    let frame_bytes = 160usize * 120 * 3;
    assert_eq!(sv.len() / frame_bytes, 30, "one second at 30 fps");
    assert_eq!(cv.len() / frame_bytes, 30);
    let (vmax, vmean, vpsnr) = quality(&sv, &cv);
    assert!(
        vmax <= 64 && vmean <= 2.0 && vpsnr >= 35.0,
        "decoded divergence max={vmax} mean={vmean:.3} psnr={vpsnr:.1}dB exceeds the noise band"
    );
    // Decoded audio statistics: silent track in both, aligned.
    let sa = decode_audio(&single);
    let ca = decode_audio(&chunked);
    assert!((sa.len() as i64 - ca.len() as i64).abs() <= 1152, "audio lengths drift");
    let peak_a = sa.iter().map(|x| x.abs()).max().unwrap_or(0);
    let peak_c = ca.iter().map(|x| x.abs()).max().unwrap_or(0);
    assert!(peak_a <= 2 && peak_c <= 2, "silent comp must decode silent ({peak_a} vs {peak_c})");
    // Duration: same length the probe sees.
    let ds = ffprobe_duration(&single);
    let dc = ffprobe_duration(&chunked);
    assert!((ds - dc).abs() <= 0.05, "durations {ds}s vs {dc}s");
}

#[test]
fn unplannable_split_refuses_before_first_frame() {
    let root = tmp("refuse");
    valid_proj(&root, "proj");
    // 30 frames: 31 chunks strands an empty chunk.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "never.mp4", "--backend", "cpu",
            "--chunks", "31",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--chunks 31"), "{out}");
    assert!(!root.join("never.mp4").exists(), "no output may land");
    // Both targets together refuse as ambiguous.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "never.mp4", "--backend", "cpu",
            "--chunks", "2", "--chunk-size", "15",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--chunks") && out.contains("--chunk-size"), "{out}");
    assert!(!root.join("never.mp4").exists(), "no output may land");
    // A chunk size above the length still renders one chunk.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "one.mp4", "--backend", "cpu",
            "--chunk-size", "60",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("chunk 1/1"), "{out}");
    assert!(root.join("one.mp4").is_file());
}

#[test]
fn chunked_sequence_format_refuses_before_first_frame() {
    let root = tmp("fmt");
    valid_proj(&root, "proj");
    let (code, out) = run(
        &[
            "render", "proj", "--backend", "cpu", "--chunks", "2",
            "--format", "pngseq",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("pngseq"), "{out}");
    assert!(!root.join("frames").exists(), "no output may land");
}
