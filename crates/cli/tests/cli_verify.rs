//! Sprint 26 T004: prove equivalence and provenance.
//!
//! `verify` judges a file against the single-pass oracle inside
//! the codec noise band and reports the measurement; a chunked
//! render carries the verdict in its run output and names
//! chunking in the file's provenance stamp. A tampered output
//! fails naming the disagreeing frames, and the checker judges
//! single-pass outputs unchanged.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-verify-{}-{}-{}",
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

const COMP: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#112233\" version=1 {\n    box x=10 y=10 width=60 height=40 fill=\"#ff0000\" radius=4 border-width=2 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n    text value=\"Hi\" start=0 duration=1 x=70 y=20 size=20 color=\"#ffffff\"\n}\n";

fn valid_proj(root: &Path, tag: &str) -> PathBuf {
    let folder = root.join(tag);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), COMP).unwrap();
    folder
}

/// Invert one frame of the decoded video and re-encode, so the
/// tampered file genuinely disagrees with every oracle.
fn tamper_frame(src: &Path, dst: &Path, frame: usize) {
    let out = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &src.display().to_string(), "-f",
            "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success());
    let frame_bytes = 160usize * 120 * 3;
    let mut raw = out.stdout;
    assert!(raw.len() >= (frame + 1) * frame_bytes, "has frame {frame}");
    for b in &mut raw[frame * frame_bytes..(frame + 1) * frame_bytes] {
        *b = 255 - *b;
    }
    let mut child = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s",
            "160x120", "-r", "30", "-i", "-", "-f", "lavfi", "-i",
            "anullsrc=r=48000:cl=stereo", "-t", "1", "-c:v", "libx264",
            "-pix_fmt", "yuv420p", "-c:a", "aac", "-ar", "48000", "-ac",
            "2", "-shortest",
        ])
        .arg(dst)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn tamper encode");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(&raw)
        .expect("write raw");
    let status = child.wait().expect("tamper encode waits");
    assert!(status.success(), "tamper encode failed: {status}");
}

#[test]
fn chunked_render_verdict_and_verify_pass_chunked() {
    let root = tmp("pass");
    valid_proj(&root, "proj");
    let (code, out) = run(
        &["render", "proj", "-o", "single.mp4", "--backend", "cpu"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render", "proj", "-o", "chunked.mp4", "--backend", "cpu",
            "--chunks", "2",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    // The chunked run reports the measured probe verdict.
    assert!(out.contains("probe video:"), "{out}");
    assert!(out.contains("probe audio:"), "{out}");
    assert!(out.contains("probe duration:"), "{out}");
    assert!(
        out.contains("chunked reassembly, not single-pass byte identity"),
        "{out}"
    );
    assert!(out.contains("probe=within-noise"), "{out}");

    // The standalone checker passes the reassembled output and
    // confirms the chunking provenance.
    let (code, out) = run(
        &[
            "verify", "chunked.mp4", "--oracle", "single.mp4", "--chunked",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("probe verdict: within-noise"), "{out}");
    assert!(out.contains("render=chunked chunks=2 frames=30"), "{out}");
    assert!(out.contains("names chunking"), "{out}");
    assert!(out.contains("frames 30/30"), "{out}");

    // The checker judges single-pass outputs too: exact.
    let (code, out) = run(&["verify", "single.mp4", "--oracle", "single.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("probe verdict: within-noise"), "{out}");
    assert!(out.contains("does not name chunking"), "{out}");

    // JSON carries the same verdict and measurements.
    let (code, out) = run(
        &[
            "verify", "chunked.mp4", "--oracle", "single.mp4", "--chunked",
            "--json",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"within_noise\":true"), "{out}");
    assert!(out.contains("\"provenance_chunked\":true"), "{out}");
    assert!(out.contains("\"frames_candidate\":30"), "{out}");
}

#[test]
fn tampered_output_is_refused_naming_the_frame() {
    let root = tmp("tamper");
    valid_proj(&root, "proj");
    let (code, out) = run(
        &["render", "proj", "-o", "single.mp4", "--backend", "cpu"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render", "proj", "-o", "chunked.mp4", "--backend", "cpu",
            "--chunks", "2",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");

    tamper_frame(&root.join("chunked.mp4"), &root.join("tampered.mp4"), 7);
    let (code, out) = run(
        &[
            "verify", "tampered.mp4", "--oracle", "single.mp4", "--chunked",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("disagreeing frames [7]"), "{out}");
    assert!(out.contains("file withheld"), "{out}");
    assert!(!out.contains("probe verdict: within-noise"), "{out}");

    // The tampered file must not carry a passing verdict in JSON
    // either.
    let (code, out) = run(
        &[
            "verify", "tampered.mp4", "--oracle", "single.mp4",
            "--json",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("\"within_noise\":false"), "{out}");
}

#[test]
fn verify_chunked_flag_refuses_unstamped_single_pass() {
    let root = tmp("stamp");
    valid_proj(&root, "proj");
    let (code, out) = run(
        &["render", "proj", "-o", "single.mp4", "--backend", "cpu"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    // A single-pass file never names chunking: `--chunked` refuses
    // even though the pixels agree with themselves.
    let (code, out) = run(
        &["verify", "single.mp4", "--oracle", "single.mp4", "--chunked"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("does not name chunking"), "{out}");
    // Usage errors exit 2.
    let (code, out) = run(&["verify", "single.mp4"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--oracle"), "{out}");
}
