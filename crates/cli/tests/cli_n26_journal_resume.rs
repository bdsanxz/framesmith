//! Sprint 26 T003: journal and resume (CLI).
//!
//! A chunked run persists its journal under the project's hidden
//! state with atomic writes. Resuming an interrupted run renders
//! exactly the unfinished chunks; resuming a finished run is a
//! no-op; a corrupt journal refuses naming the file; finished
//! chunks are never resurrected; and no partial output is promoted.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n26journal-{}-{}-{}",
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

const COMP: &str = "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#112233\" version=1 {\n    box x=10 y=10 width=60 height=40 fill=\"#ff0000\" radius=4 border-width=2 border-color=\"#ffffff\" opacity=1 order=1 start=0 duration=2\n    text value=\"Hi\" start=0 duration=2 x=70 y=20 size=20 color=\"#ffffff\"\n}\n";

fn valid_proj(root: &Path, tag: &str) -> PathBuf {
    let folder = root.join(tag);
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", COMP);
    folder
}

fn journal_path(project: &Path, output_name: &str) -> PathBuf {
    // Mirror render::journal::journal_path_for: <proj>/.framesmith/render-journal/<file>.journal.json
    // `project` is the absolute project dir; callers pass the
    // absolute path or an absolute alias. The helper keeps the
    // historical leaf name so two outputs with the same stem but
    // different extensions stay distinct.
    let leaf = format!("{output_name}.journal.json");
    project
        .join(".framesmith")
        .join("render-journal")
        .join(leaf)
}

fn journal_leaf(output_name: &str) -> String {
    format!("{output_name}.journal.json")
}

fn decode_video(mp4: &Path) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &mp4.display().to_string(), "-f",
            "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success(), "ffmpeg decode: {}", String::from_utf8_lossy(&out.stderr));
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
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

fn quality(a: &[u8], b: &[u8]) -> (u8, f64, f64) {
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
    let psnr = if mse == 0.0 { 99.0 } else { 10.0 * (255.0 * 255.0 / mse).log10() };
    (max, mean, psnr)
}

#[test]
fn fresh_chunked_render_persists_journal_and_single_pass_stays_untouched() {
    let root = tmp("fresh");
    let proj = valid_proj(&root, "proj");
    // Single-pass baseline stays byte-identical (T001 pin).
    let (code, out) = run(&["render", "proj", "-o", "single.mp4", "--backend", "cpu"], &root);
    assert_eq!(code, 0, "{out}");
    let single_bytes = std::fs::read(root.join("single.mp4")).unwrap();
    let (code, out) = run(&["render", "proj", "-o", "single2.mp4", "--backend", "cpu"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(std::fs::read(root.join("single2.mp4")).unwrap(), single_bytes);

    // Chunked render creates the journal atomically under hidden state.
    let (code, out) = run(
        &["render", "proj", "-o", "chunked.mp4", "--backend", "cpu", "--chunks", "3"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stage chunks: plan"), "{out}");
    assert!(out.contains("chunks=3"), "{out}");
    let jpath = journal_path(&proj, "chunked.mp4");
    assert!(jpath.is_file(), "journal lands under hidden state: {}", jpath.display());
    let body = std::fs::read_to_string(&jpath).unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["version"], 1, "{body}");
    assert_eq!(v["total_frames"], 60, "2s at 30fps = 60 frames: {body}");
    assert!(v["chunks"].is_array());
    assert_eq!(v["chunks"].as_array().unwrap().len(), 3);
    for c in v["chunks"].as_array().unwrap() {
        assert_eq!(c["status"], "finished", "{body}");
    }
    assert_eq!(v["status"], "finished", "{body}");
    // Atomic writes leave no temp file beside the journal.
    let dir = jpath.parent().unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!leftovers.iter().any(|n| n.contains(".tmp")), "{leftovers:?} must not contain temp files");
}

#[test]
fn resume_of_finished_is_noop() {
    let root = tmp("noop");
    valid_proj(&root, "proj");
    let (code, out) = run(
        &["render", "proj", "-o", "chunked.mp4", "--backend", "cpu", "--chunks", "2"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let before = std::fs::read(root.join("chunked.mp4")).unwrap();
    // Resume of the finished run reports finished without re-rendering.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "chunked.mp4", "--backend", "cpu",
            "--chunks", "2", "--resume",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already finished") || out.contains("finished"), "{out}");
    assert!(out.contains("no chunks re-rendered"), "{out}");
    assert_eq!(std::fs::read(root.join("chunked.mp4")).unwrap(), before, "no-op must not change bytes");
    // The journal is not rewritten into a fresh plan on a no-op.
    let jpath = journal_path(&root.join("proj"), "chunked.mp4");
    let body = std::fs::read_to_string(jpath).unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["status"], "finished", "{body}");
}

#[test]
fn corrupt_journal_refuses_resume_naming_the_file() {
    let root = tmp("corrupt");
    let proj = valid_proj(&root, "proj");
    // First a clean run so a journal exists.
    let (code, out) = run(
        &["render", "proj", "-o", "chunked.mp4", "--backend", "cpu", "--chunks", "2"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let jpath = journal_path(&proj, "chunked.mp4");
    std::fs::write(&jpath, "{ not json").unwrap();
    // Resume must refuse naming the unreadable file.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "chunked.mp4", "--backend", "cpu",
            "--chunks", "2", "--resume",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains(&journal_leaf("chunked.mp4")), "{out}");
    // Separator-agnostic: the refusal names the journal path with
    // the OS separator.
    let out = out.replace('\\', "/");
    assert!(out.contains(".framesmith/render-journal"), "{out}");
    assert!(out.contains("repair the file or remove it") || out.contains("repair or remove"), "{out}");
    // The corrupt file is left untouched and the final output is not overwritten.
    assert_eq!(std::fs::read_to_string(&jpath).unwrap(), "{ not json");
}

#[test]
fn interrupt_then_resume_renders_only_unfinished() {
    let root = tmp("resume");
    let proj = valid_proj(&root, "proj");
    // Uninterrupted oracle.
    let (code, out) = run(
        &["render", "proj", "-o", "oracle.mp4", "--backend", "cpu", "--chunks", "3"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let oracle = root.join("oracle.mp4");
    let oracle_video_before = decode_video(&oracle);

    // Prepare a second output whose journal we will interrupt.
    let (code, out) = run(
        &["render", "proj", "-o", "resumed.mp4", "--backend", "cpu", "--chunks", "3"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let resumed = root.join("resumed.mp4");
    let jpath = journal_path(&proj, "resumed.mp4");
    // Simulate a crash after chunk 1 finished, chunk 2 started
    // (died mid-chunk), chunk 3 never started: edit the journal
    // in place like a kill -9 would leave it (started without a
    // following finished), delete the final file, and keep the
    // finished artifacts intact.
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&jpath).unwrap()).unwrap();
    v["status"] = serde_json::Value::String("inprogress".to_string());
    v["finished_at"] = serde_json::Value::Null;
    // Chunk 1 (index 0) stays finished; chunk 2 (index 1) is
    // started (crashed mid-chunk); chunk 3 (index 2) is planned.
    v["chunks"][1]["status"] = serde_json::Value::String("started".to_string());
    v["chunks"][2]["status"] = serde_json::Value::String("planned".to_string());
    std::fs::write(&jpath, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    std::fs::remove_file(&resumed).unwrap();
    assert!(!resumed.exists());

    // Resume: only the unfinished chunks re-render.
    let (code, out) = run(
        &[
            "render", "proj", "-o", "resumed.mp4", "--backend", "cpu",
            "--chunks", "3", "--resume",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stage chunks:"), "{out}");
    assert!(resumed.is_file(), "resumed output lands: {out}");
    // The resumed output equals the uninterrupted chunked output
    // inside the codec's own noise band.
    let resumed_video = decode_video(&resumed);
    assert_eq!(oracle_video_before.len(), resumed_video.len(), "same frames and alignment");
    let (vmax, vmean, vpsnr) = quality(&oracle_video_before, &resumed_video);
    assert!(
        vmax <= 64 && vmean <= 2.0 && vpsnr >= 35.0,
        "resumed vs oracle decoded divergence max={vmax} mean={vmean:.3} psnr={vpsnr:.1}dB exceeds the noise band"
    );
    let dur_o = ffprobe_duration(&oracle);
    let dur_r = ffprobe_duration(&resumed);
    assert!((dur_o - dur_r).abs() <= 0.05, "durations {dur_o}s vs {dur_r}s");
    // Audio statistics track as well (silent composition).
    let a_o = decode_audio(&oracle);
    let a_r = decode_audio(&resumed);
    assert!((a_o.len() as i64 - a_r.len() as i64).abs() <= 1152, "audio drift");
    let peak_o = a_o.iter().map(|x| x.abs()).max().unwrap_or(0);
    let peak_r = a_r.iter().map(|x| x.abs()).max().unwrap_or(0);
    assert!(peak_o <= 2 && peak_r <= 2, "silent peaks {peak_o} vs {peak_r}");
    // The journal records both the interruption and the resume.
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&jpath).unwrap()).unwrap();
    assert_eq!(v["status"], "finished", "{v}");
    let interruptions = v["interruptions"].as_array().unwrap();
    assert!(!interruptions.is_empty(), "interruptions recorded: {v}");
    assert!(interruptions.iter().any(|i| i["kind"] == "inferred" || i["kind"] == "cancel" || i["detail"].as_str().unwrap_or("").contains("chunk")), "{v}");
    assert!(interruptions.iter().any(|i| i["kind"] == "resume"), "{v}");
}

#[test]
fn resume_without_explicit_chunk_flags_reads_plan_from_journal() {
    let root = tmp("resume2");
    let proj = valid_proj(&root, "proj");
    let (code, out) = run(
        &["render", "proj", "-o", "chunked.mp4", "--backend", "cpu", "--chunks", "2"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    // Tamper the journal into an interrupted state with a single
    // finished chunk, remove the final, and resume without passing
    // chunk flags: the plan comes from the journal.
    let jpath = journal_path(&proj, "chunked.mp4");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&jpath).unwrap()).unwrap();
    v["status"] = serde_json::Value::String("inprogress".to_string());
    v["finished_at"] = serde_json::Value::Null;
    v["chunks"][1]["status"] = serde_json::Value::String("planned".to_string());
    std::fs::write(&jpath, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    std::fs::remove_file(root.join("chunked.mp4")).unwrap();
    let (code, out) = run(
        &["render", "proj", "-o", "chunked.mp4", "--backend", "cpu", "--resume"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("resuming"), "{out}");
    assert!(root.join("chunked.mp4").is_file());
}

#[test]
fn missing_chunk_artifact_refuses_resume_naming_the_chunk() {
    let root = tmp("missing");
    let proj = valid_proj(&root, "proj");
    let (code, out) = run(
        &["render", "proj", "-o", "chunked.mp4", "--backend", "cpu", "--chunks", "3"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let jpath = journal_path(&proj, "chunked.mp4");
    // Delete a finished chunk's artifact; the journal still says it
    // finished, so resume must refuse naming the missing chunk
    // rather than silently reviving it.
    let chunk_dir = root.join("chunked.chunks");
    let victim = chunk_dir.join("chunk-0.mp4");
    if victim.exists() {
        std::fs::remove_file(&victim).unwrap();
    }
    std::fs::remove_file(root.join("chunked.mp4")).unwrap();
    // Mark the journal inprogress so resume re-enters the render.
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&jpath).unwrap()).unwrap();
    v["status"] = serde_json::Value::String("inprogress".to_string());
    v["finished_at"] = serde_json::Value::Null;
    std::fs::write(&jpath, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let (code, out) = run(
        &[
            "render", "proj", "-o", "chunked.mp4", "--backend", "cpu",
            "--chunks", "3", "--resume",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("chunk 1/3"), "{out}");
    assert!(out.contains(&journal_leaf("chunked.mp4")), "{out}");
    assert!(!root.join("chunked.mp4").exists(), "no partial promotion");
}
