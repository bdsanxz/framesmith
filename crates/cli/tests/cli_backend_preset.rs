//! Sprint 3 T001 (render backend flag) + T002 (quality presets).
//!
//! Backend choice names the renderer without changing validity;
//! presets trade size and fidelity with announced settings, and every
//! refusal precedes the first frame with no MP4 written.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-pipeline-{}-{}-{}",
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
    run_env(args, cwd, &[])
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const COMP: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#112233\" version=1 {\n    box x=10 y=10 width=60 height=40 fill=\"#ff0000\" radius=4 border-width=2 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n    text value=\"Hi\" start=0 duration=1 x=70 y=20 size=20 color=\"#ffffff\"\n}\n";

const BAD_COMP: &str = "composition name=\"t\" width=160 height=120 duration=1 background=\"#112233\" version=1 {\n    text value=\"Hi\" duration=0\n}\n";

fn valid_proj(root: &Path, tag: &str) -> PathBuf {
    let folder = root.join(tag);
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", COMP);
    folder
}

fn ffprobe_csv(mp4: &Path) -> String {
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

fn mean_abs(a: &[u8], b: &[u8]) -> (u8, f64) {
    assert_eq!(a.len(), b.len());
    let mut max = 0u8;
    let mut total = 0u64;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = x.abs_diff(*y);
        max = max.max(d);
        total += d as u64;
    }
    (max, total as f64 / a.len() as f64)
}

// --- T001: backend selection ---------------------------------------------

#[test]
fn backend_cpu_names_cpu_in_output_and_log() {
    let root = tmp("backend-cpu");
    let folder = valid_proj(&root, "proj");
    let (code, out) = run(&["render", "proj", "--backend", "cpu"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("backend=cpu"), "{out}");
    assert!(folder.join("out.mp4").is_file());
    let log = std::fs::read_to_string(folder.join("logs").join("framesmith.log")).unwrap();
    assert!(log.contains("backend=cpu"), "{log}");
}

#[test]
fn backend_auto_matches_no_flag_on_this_machine() {
    // No GPU renderer ships here, so `auto` and no flag both mean `cpu`
    // and render byte-identical MP4s.
    let root = tmp("backend-auto");
    valid_proj(&root, "proj");
    let (code, a) = run(
        &["render", "proj", "-o", "a.mp4", "--backend", "auto"],
        &root,
    );
    assert_eq!(code, 0, "{a}");
    let (code, b) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{b}");
    assert!(a.contains("backend=cpu"), "{a}");
    assert!(b.contains("backend=cpu"), "{b}");
    assert_eq!(
        std::fs::read(root.join("a.mp4")).unwrap(),
        std::fs::read(root.join("b.mp4")).unwrap(),
        "auto and default renders agree byte-identically"
    );
}

#[test]
fn backend_gpu_refuses_without_gpu_before_first_frame() {
    let root = tmp("backend-gpu-refuse");
    let folder = valid_proj(&root, "proj");
    let (code, out) = run(&["render", "proj", "--backend", "gpu"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no GPU renderer is available"), "{out}");
    assert!(out.contains("--backend cpu"), "{out}");
    assert!(out.contains("--backend auto"), "{out}");
    assert!(!folder.join("out.mp4").exists(), "no MP4 may be written");
    assert!(
        !folder.join("out.mp4.framesmith-part").exists(),
        "no staged file may survive"
    );
    // The `--json` form carries the same refusal machine-readably.
    let (code, out) = run(&["render", "proj", "--backend", "gpu", "--json"], &root);
    assert_eq!(code, 1, "{out}");
    let json = out
        .lines()
        .find(|l| l.starts_with("{\"tool\""))
        .unwrap_or("");
    assert!(json.contains("\"backend\":\"gpu\""), "{json}");
    assert!(json.contains("no GPU renderer is available"), "{json}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn unknown_backend_is_a_validation_error_with_no_mp4() {
    let root = tmp("backend-unknown");
    let folder = valid_proj(&root, "proj");
    let (code, out) = run(&["render", "proj", "--backend", "cuda"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("\"cuda\""), "{out}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn validity_never_depends_on_the_backend_flag() {
    let root = tmp("backend-validity");
    let good = valid_proj(&root, "good");
    write(&good, "composition.kdl", COMP);
    let bad = root.join("bad");
    std::fs::create_dir_all(&bad).unwrap();
    write(&bad, "composition.kdl", BAD_COMP);
    for backend in ["cpu", "gpu", "auto"] {
        let (code, out) = run(&["check", "--backend", backend, "good"], &root);
        assert_eq!(code, 0, "backend={backend}: {out}");
        let (code, out) = run(&["check", "--backend", backend, "bad"], &root);
        assert_eq!(code, 1, "backend={backend}: {out}");
        assert!(out.contains("E_BAD_TIMING"), "backend={backend}: {out}");
    }
    // Unknown backend values are validation errors on `check` too.
    let (code, out) = run(&["check", "--backend", "cuda", "good"], &root);
    assert_eq!(code, 1, "{out}");
}

#[test]
fn gpu_backend_draws_when_the_probe_reports_one() {
    // FRAMESMITH_GPU_AVAILABLE=1 opts into the portable GPU picture
    // path; the render names `gpu` and stays within the Sprint 2
    // tolerance of the CPU picture of the same frame.
    let root = tmp("backend-gpu");
    valid_proj(&root, "proj");
    let env = [("FRAMESMITH_GPU_AVAILABLE", "1")];
    let (code, out) = run_env(
        &["render", "proj", "-o", "gpu.mp4", "--backend", "gpu"],
        &root,
        &env,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("backend=gpu"), "{out}");
    let (code, out) = run_env(
        &["render", "proj", "-o", "cpu.mp4", "--backend", "cpu"],
        &root,
        &env,
    );
    assert_eq!(code, 0, "{out}");
    for mp4 in ["gpu.mp4", "cpu.mp4"] {
        let streams = ffprobe_csv(&root.join(mp4));
        for want in ["codec_name=h264", "pix_fmt=yuv420p", "codec_name=aac"] {
            assert!(streams.contains(want), "{mp4}: {streams}");
        }
    }
    let (max, mean) = mean_abs(
        &decode_raw(&root.join("cpu.mp4"), 1),
        &decode_raw(&root.join("gpu.mp4"), 1),
    );
    println!("backend-gap: decoded cpu-vs-gpu max_abs={max} mean_abs={mean:.4}");
    // Post-encode comparison: generous bounds that still reject a
    // missing element or a shifted picture (tens of levels).
    assert!(max <= 32, "backend gap too large: max={max}");
    assert!(mean < 8.0, "backend gap too large: mean={mean:.4}");
    // `auto` follows the probe too.
    let (code, out) = run_env(
        &["render", "proj", "-o", "auto.mp4", "--backend", "auto"],
        &root,
        &env,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("backend=gpu"), "{out}");
}

// --- T002: presets and overrides ------------------------------------------

#[test]
fn presets_announce_settings_and_trade_size() {
    let root = tmp("presets");
    valid_proj(&root, "proj");
    let mut sizes = Vec::new();
    for (preset, crf, speed) in [
        ("draft", "crf=30", "x264-preset=veryfast"),
        ("standard", "crf=23", "x264-preset=default"),
        ("high", "crf=18", "x264-preset=slow"),
    ] {
        let (code, out) = run(
            &[
                "render",
                "proj",
                "-o",
                &format!("{preset}.mp4"),
                "--preset",
                preset,
            ],
            &root,
        );
        assert_eq!(code, 0, "{preset}: {out}");
        assert!(out.contains(&format!("preset={preset}")), "{preset}: {out}");
        assert!(out.contains(crf), "{preset}: {out}");
        assert!(out.contains(speed), "{preset}: {out}");
        let streams = ffprobe_csv(&root.join(format!("{preset}.mp4")));
        for want in [
            "codec_name=h264",
            "width=160",
            "pix_fmt=yuv420p",
            "codec_name=aac",
        ] {
            assert!(streams.contains(want), "{preset}: {streams}");
        }
        sizes.push(
            std::fs::metadata(root.join(format!("{preset}.mp4")))
                .unwrap()
                .len(),
        );
    }
    println!(
        "preset-sizes: draft={} standard={} high={}",
        sizes[0], sizes[1], sizes[2]
    );
    assert!(
        sizes[0] < sizes[1] && sizes[1] < sizes[2],
        "draft < standard < high in file size: {sizes:?}"
    );
}

#[test]
fn no_preset_behaves_exactly_like_standard() {
    let root = tmp("preset-default");
    valid_proj(&root, "proj");
    let (code, a) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{a}");
    let (code, b) = run(
        &["render", "proj", "-o", "b.mp4", "--preset", "standard"],
        &root,
    );
    assert_eq!(code, 0, "{b}");
    assert!(a.contains("preset=standard"), "{a}");
    assert_eq!(
        std::fs::read(root.join("a.mp4")).unwrap(),
        std::fs::read(root.join("b.mp4")).unwrap(),
        "default and standard renders are byte-identical"
    );
}

#[test]
fn crf_override_touches_only_its_own_axis() {
    let root = tmp("preset-crf");
    valid_proj(&root, "proj");
    let (code, out) = run(&["render", "proj", "-o", "c.mp4", "--crf", "30"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("preset=standard"), "{out}");
    assert!(out.contains("crf=30"), "{out}");
    let (code, out) = run(&["render", "proj", "-o", "s.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert_ne!(
        std::fs::read(root.join("c.mp4")).unwrap(),
        std::fs::read(root.join("s.mp4")).unwrap(),
        "an override must change the bytes"
    );
}

#[test]
fn video_bitrate_override_touches_only_its_own_axis() {
    let root = tmp("preset-bitrate");
    valid_proj(&root, "proj");
    let (code, out) = run(
        &["render", "proj", "-o", "b.mp4", "--video-bitrate", "500k"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("video-bitrate=500k"), "{out}");
    assert!(out.contains("crf=23"), "{out}");
    assert!(root.join("b.mp4").is_file());
}

#[test]
fn preset_refusals_write_no_mp4() {
    let root = tmp("preset-refuse");
    let folder = valid_proj(&root, "proj");
    // Both overrides together.
    let (code, out) = run(
        &["render", "proj", "--crf", "20", "--video-bitrate", "2M"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("--crf") && out.contains("--video-bitrate"),
        "{out}"
    );
    // Unknown preset name.
    let (code, out) = run(&["render", "proj", "--preset", "ultra"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("\"ultra\""), "{out}");
    // Non-numeric overrides.
    let (code, out) = run(&["render", "proj", "--crf", "banana"], &root);
    assert_eq!(code, 1, "{out}");
    let (code, out) = run(&["render", "proj", "--video-bitrate", "banana"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!folder.join("out.mp4").exists(), "no MP4 on refusal");
    assert!(!folder.join("out.mp4.framesmith-part").exists());
}
