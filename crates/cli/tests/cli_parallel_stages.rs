//! Sprint 3 T003 (parallel frames, deterministic) + T006 (staged
//! progress with timings).
//!
//! Any worker count draws byte-identical MP4s; progress names every
//! stage as it runs with measured durations in both human and `--json`
//! forms, and a failed stage is named with its reason.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-stages-{}-{}-{}",
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

/// Animated composition: a box tweened across the frame so every frame
/// differs and any scheduling change would show in the bytes.
fn animated_comp() -> String {
    "composition name=\"t\" width=128 height=96 duration=1 fps=30 background=\"#101010\" version=1 {\n    box x=4 y=20 width=24 height=24 fill=\"#ff0000\" radius=2 border-width=1 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=1\n    text value=\"Hi\" start=0 duration=1 x=60 y=30 size=18 color=\"#ffffff\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=100 ease=\"linear\"\n}\n"
    .to_string()
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

// --- T003: parallel determinism -------------------------------------------

#[test]
fn worker_counts_render_byte_identical_mp4s() {
    let root = tmp("workers");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), animated_comp()).unwrap();
    let (code, out) = run_env(&["render", "proj", "-o", "w1.mp4"], &root, &[("FRAMESMITH_WORKERS", "1")]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run_env(&["render", "proj", "-o", "w8.mp4"], &root, &[("FRAMESMITH_WORKERS", "8")]);
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("w1.mp4")).unwrap();
    let b = std::fs::read(root.join("w8.mp4")).unwrap();
    println!("worker-bytes: 1 worker = {} bytes, 8 workers = {} bytes", a.len(), b.len());
    assert_eq!(a, b, "1 worker and 8 workers must render byte-identical MP4s");
    assert_eq!(decode_raw(&root.join("w1.mp4"), 30), decode_raw(&root.join("w8.mp4"), 30));
}

#[test]
fn parallel_batches_emit_frames_in_time_order() {
    // T004: a multi-batch parallel render must emit consecutive frames
    // in time order — strided batches once scrambled every slow comp
    // past the first batch at more than one worker.
    let root = tmp("workers-order");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    let comp = "composition name=\"t\" width=128 height=96 duration=3 fps=30 background=\"#101010\" version=1 {\n    box x=4 y=20 width=24 height=24 fill=\"#ff0000\" radius=0 opacity=1 order=1 start=0 duration=3\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=3 to=100 ease=\"linear\"\n}\n";
    std::fs::write(folder.join("composition.kdl"), comp).unwrap();
    let (code, out) = run_env(&["render", "proj", "-o", "w8.mp4"], &root, &[("FRAMESMITH_WORKERS", "8")]);
    assert_eq!(code, 0, "{out}");
    let raw = decode_raw(&root.join("w8.mp4"), 90);
    assert_eq!(raw.len(), 128 * 96 * 3 * 90);
    // Left edge of the red box per output frame: advancing and on the
    // frame's own time (linear tween 4 -> 100 over 90 frames). The edge
    // detector stays loose (first clearly-red column) because x264
    // softens the box edge; scrambled emission still fails the
    // per-frame position check by tens of pixels.
    let mut prev = -1i32;
    let mut first = 0i32;
    let mut last = 0i32;
    for k in 0..90usize {
        let base = k * 128 * 96 * 3;
        let mut edge = 128i32;
        for x in 0..128usize {
            let o = base + (20 * 128 + x) * 3;
            if raw[o] > 120 {
                edge = x as i32;
                break;
            }
        }
        let expected = 4.0 + 96.0 * k as f64 / 90.0;
        assert!(edge >= prev, "frame {k}: box edge {edge} must not move back past {prev}");
        assert!(
            (edge as f64 - expected).abs() <= 3.0,
            "frame {k}: edge {edge} must sit on its own time (want {expected:.1})"
        );
        if k == 0 {
            first = edge;
        }
        last = edge;
        prev = edge;
    }
    assert!(last - first >= 80, "box must cross the frame ({first} -> {last})");
}

#[test]
fn invalid_worker_setting_falls_back_and_says_so() {
    let root = tmp("workers-invalid");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), animated_comp()).unwrap();
    let (code, out) = run_env(&["render", "proj"], &root, &[("FRAMESMITH_WORKERS", "bogus")]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("workers: invalid FRAMESMITH_WORKERS=\"bogus\""), "{out}");
    assert!(out.contains("using "), "{out}");
    assert!(folder.join("out.mp4").is_file());
}

#[test]
fn parallelism_changes_neither_verdict() {
    let root = tmp("workers-verdict");
    let bad = root.join("bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(
        bad.join("composition.kdl"),
        "composition name=\"t\" width=64 height=48 duration=1 background=\"#000\" version=1 {\n    text value=\"Hi\" duration=0\n}\n",
    )
    .unwrap();
    for workers in ["1", "8"] {
        let (code, out) = run_env(&["render", "bad"], &root, &[("FRAMESMITH_WORKERS", workers)]);
        assert_eq!(code, 1, "workers={workers}: {out}");
        assert!(out.contains("E_BAD_TIMING"), "workers={workers}: {out}");
        assert!(!bad.join("out.mp4").exists());
    }
}

// --- T006: staged progress -------------------------------------------------

const STAGE_NAMES: [&str; 5] = ["validate", "frames", "audio", "encode", "done"];

#[test]
fn human_progress_names_every_stage_with_measured_timings() {
    let root = tmp("stages-human");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), animated_comp()).unwrap();
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    for stage in ["stage validate:", "stage frames:", "stage audio:", "stage encode:", "stage done:"] {
        assert!(out.contains(stage), "missing {stage}: {out}");
    }
    assert!(out.contains("timing:"), "{out}");
    for stage in ["validate=", "frames=", "audio=", "encode=", "done=", "total="] {
        assert!(out.contains(stage), "timing misses {stage}: {out}");
    }
    assert!(out.contains("backend=cpu"), "{out}");
    assert!(out.contains("preset=standard"), "{out}");
    // Progress never claims an estimated completion time.
    for banned in ["ETA", "eta", "remaining", "estimated", "%", "complete in"] {
        assert!(!out.contains(banned), "progress must not estimate completion ({banned}): {out}");
    }
}

#[test]
fn json_progress_carries_the_same_stages_and_durations() {
    let root = tmp("stages-json");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), animated_comp()).unwrap();
    let (code, combined) = run(&["render", "proj", "--json"], &root);
    assert_eq!(code, 0, "{combined}");
    // Stdout stays pure JSON: exactly one document, and it parses as an
    // object with the render fields.
    let out = Command::new(bin())
        .args(["render", "proj", "--json"])
        .current_dir(&root)
        .output()
        .expect("run framesmith binary");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let doc = stdout.trim();
    assert!(doc.starts_with("{\"tool\":\"framesmith\""), "{doc}");
    assert!(doc.ends_with('}'), "{doc}");
    assert_eq!(doc.lines().count(), 1, "stdout must be one JSON document");
    for field in [
        "\"command\":\"render\"",
        "\"status\":\"valid\"",
        "\"exit\":0",
        "\"backend\":\"cpu\"",
        "\"preset\":\"standard\"",
        "\"crf\":23",
    ] {
        assert!(doc.contains(field), "missing {field}: {doc}");
    }
    // Same stages in the same canonical order, each with a measured
    // numeric duration.
    let mut last = 0;
    for name in STAGE_NAMES {
        let tag = format!("\"name\":\"{name}\",\"status\":\"ok\",\"duration_ms\":");
        let pos = doc.find(&tag).unwrap_or_else(|| panic!("missing stage {name}: {doc}"));
        assert!(pos > last, "stages out of order at {name}: {doc}");
        last = pos;
        let rest = &doc[pos + tag.len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        assert!(!digits.is_empty(), "stage {name} has no measured duration: {doc}");
    }
    // Human and JSON agree on the stage sequence.
    let (code, human) = run(&["render", "proj", "-o", "h.mp4"], &root);
    assert_eq!(code, 0, "{human}");
    for name in STAGE_NAMES {
        assert!(human.contains(&format!("stage {name}")), "human misses {name}: {human}");
    }
}

#[test]
fn failing_renders_name_the_failed_stage_in_both_forms() {
    let root = tmp("stages-fail");
    let folder = root.join("bad");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=64 height=48 duration=1 background=\"#000\" version=1 {\n    text value=\"Hi\" duration=0\n}\n",
    )
    .unwrap();
    let (code, out) = run(&["render", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("stage validate: failed"), "{out}");
    assert!(out.contains("(validation stage)"), "{out}");
    assert!(out.contains("timing: validate="), "timing covers reached stages: {out}");
    assert!(!folder.join("out.mp4").exists());
    // The `--json` form names the same failed stage machine-readably.
    let out = Command::new(bin())
        .args(["render", "bad", "--json"])
        .current_dir(&root)
        .output()
        .expect("run framesmith binary");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let doc = stdout.trim();
    assert!(doc.contains("\"status\":\"invalid\""), "{doc}");
    assert!(doc.contains("\"name\":\"validate\",\"status\":\"failed\""), "{doc}");
    assert!(doc.contains("\"timing_ms\""), "{doc}");
}
