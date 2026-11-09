//! Renders panel and queue regressions (Sprint 18, T006 / CAP-006,
//! CAP-009, J-05, STATE-03, STATE-04, INT-003, REC-002, REC-003,
//! REC-005).
//!
//! Jobs queue in order with format, quality, frame-rate, resolution,
//! and GPU settings; progress is measured with the ETA labeled an
//! estimate; cancel stops the run and removes the partial output
//! reporting the stage reached; download byte-matches; delete and
//! hide-finished behave per the specification; a missing FFmpeg
//! refuses instead of queueing; failed jobs name their stage while
//! the queue advances; and GIF exports loop as chosen at the capped
//! rate with no audio.

use preview::daemon;
use preview::server::{ServeOptions, run_serve};
use preview::studio_compositions;
use preview::studio_renders;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

/// Renders and cancels share one process-wide pipeline flag, so every
/// test that executes a job or cancels holds this to completion.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-renders-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// One silent text composition: 320x180, 2s at 30 fps, 60 frames.
fn tiny_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=30 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Four silent seconds at 30 fps (120 frames): two batches of
/// measured progress.
fn mid_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=30 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Hi\" start=0 duration=4 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Ten silent seconds at 30 fps (300 frames): long enough that a
/// cancel requested on the first piped batch always lands mid-run.
fn long_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=10 fps=30 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Hello cancel\" start=0 duration=10 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"b\" value=\"second line here\" start=0 duration=10 x=10 y=90 size=24 color=\"#ffff00\"\n}\n",
    );
}

/// Twenty silent seconds at 30 fps (600 frames): the HTTP cancel
/// test watches this job go running before cancelling, so the abort
/// always lands mid-run with seconds of margin.
fn http_cancel_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=20 fps=30 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Hello cancel\" start=0 duration=20 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"b\" value=\"second line here\" start=0 duration=20 x=10 y=90 size=24 color=\"#ffff00\"\n}\n",
    );
}

/// Forty silent seconds at 30 fps (1200 frames): over the GIF cap.
fn overcap_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=40 fps=30 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Too long\" start=0 duration=40 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Two silent seconds at 15 fps (30 frames): a quick GIF source.
fn gif_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=15 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Loop\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn enqueue_req(format: &str) -> studio_renders::EnqueueRequest {
    studio_renders::EnqueueRequest {
        actor: "maker".to_string(),
        composition: None,
        format: format.to_string(),
        quality: "standard".to_string(),
        fps: None,
        resolution: "auto".to_string(),
        gpu: false,
        loop_count: 0,
    }
}

fn queue_of(folder: &Path) -> Vec<studio_compositions::RenderJob> {
    studio_compositions::list_render_queue(folder).expect("the queue lists")
}

/// A `PATH` value with no FFmpeg on it (Sprint 18, T006): the queue
/// refuses against this without touching process state.
fn empty_path() -> Option<OsString> {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-no-ffmpeg-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Some(dir.into_os_string())
}

fn ffprobe_show(path: &Path, entries: &str) -> String {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            entries,
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .output()
        .expect("run host ffprobe");
    assert!(
        out.status.success(),
        "ffprobe failed for {}",
        path.display()
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// GIF loop count from the file bytes (Sprint 18, T006): the
/// `NETSCAPE2.0` application extension carries the count as a
/// little-endian `u16` (`0` means forever).
fn gif_loop_count(bytes: &[u8]) -> Option<u16> {
    let marker = b"NETSCAPE2.0";
    let at = bytes
        .windows(marker.len())
        .position(|w| w == marker)?;
    let mut i = at + marker.len();
    while i + 2 < bytes.len() {
        // Sub-block: size byte, id byte, count LE u16.
        let size = bytes[i] as usize;
        if size >= 3 && bytes[i + 1] == 1 {
            return Some(u16::from_le_bytes([bytes[i + 2], bytes[i + 3]]));
        }
        if size == 0 {
            return None;
        }
        i += 1 + size;
    }
    None
}

// --- enqueue bodies and settings -------------------------------------------

#[test]
fn enqueue_body_refusals_name_the_cause_and_queue_nothing() {
    let folder = tmp("bodies");
    tiny_project(&folder);
    let before = tree_bytes(&folder);
    for (body, needle) in [
        ("", "needs a JSON body"),
        ("{nope", "is not JSON"),
        ("{\"actor\":\"\"}", "names no actor"),
        (
            "{\"actor\":\"maker\",\"format\":\"avi\"}",
            "unknown queue format",
        ),
        (
            "{\"actor\":\"maker\",\"quality\":\"ultra\"}",
            "unknown queue quality",
        ),
        (
            "{\"actor\":\"maker\",\"fps\":25}",
            "unknown queue frame rate",
        ),
        (
            "{\"actor\":\"maker\",\"fps\":\"fast\"}",
            "not a frame rate",
        ),
        (
            "{\"actor\":\"maker\",\"resolution\":\"8k\"}",
            "unknown queue resolution",
        ),
        (
            "{\"actor\":\"maker\",\"format\":\"mp4\",\"loop_count\":2}",
            "GIF jobs only",
        ),
        (
            "{\"actor\":\"maker\",\"format\":\"gif\",\"loop_count\":70000}",
            "loop cap",
        ),
        (
            "{\"actor\":\"maker\",\"format\":\"gif\",\"loop_count\":\"many\"}",
            "not a loop count",
        ),
    ] {
        let err = studio_renders::parse_enqueue_body(body).expect_err("bad body refuses");
        assert!(err.contains(needle), "body {body:?}: {err}");
    }
    assert_eq!(tree_bytes(&folder), before, "parsing writes nothing");
    let req = studio_renders::parse_enqueue_body(
        "{\"actor\":\"maker\",\"composition\":\"t\",\"format\":\"webm\",\"quality\":\"high\",\"fps\":24,\"resolution\":\"1080p\",\"gpu\":true,\"loop_count\":0}",
    )
    .expect("a full body parses");
    assert_eq!(req.composition.as_deref(), Some("t"));
    assert_eq!(req.format, "webm");
    assert_eq!(req.quality, "high");
    assert_eq!(req.fps, Some(24));
    assert_eq!(req.resolution, "1080p");
    assert!(req.gpu);
}

#[test]
fn enqueue_validates_settings_and_plans_hidden_output() {
    let _lock = serial();
    let folder = tmp("enqueue");
    tiny_project(&folder);
    let before_kdl = std::fs::read(folder.join("composition.kdl")).unwrap();
    let receipt = studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None)
        .expect("mp4 enqueues");
    assert_eq!(receipt.seq, 1);
    assert_eq!(receipt.composition, "t");
    assert_eq!(receipt.format, "mp4");
    assert!(receipt.output.contains(".framesmith"), "got {}", receipt.output);
    assert!(receipt.output.ends_with("t-1.mp4"), "got {}", receipt.output);
    let jobs = queue_of(&folder);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "queued");
    assert_eq!(jobs[0].stage, "queued");
    assert_eq!(jobs[0].format, "mp4");
    assert_eq!(jobs[0].quality, "standard");
    assert!(jobs[0].notes.is_empty(), "no clamps on a plain job");
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        before_kdl,
        "queueing never touches sources"
    );
    assert!(
        !Path::new(&receipt.output).exists(),
        "queueing writes plans, never pixels"
    );
}

#[test]
fn enqueue_honors_every_setting_combination() {
    let _lock = serial();
    let folder = tmp("settings");
    tiny_project(&folder);
    let mut req = enqueue_req("webm");
    req.quality = "high".to_string();
    req.fps = Some(24);
    req.resolution = "1080p".to_string();
    req.gpu = true;
    let receipt = studio_renders::enqueue_job(&folder, &req, None).expect("webm enqueues");
    assert_eq!(receipt.seq, 1);
    assert!(receipt.output.ends_with("t-1.webm"), "got {}", receipt.output);
    let mut gif = enqueue_req("gif");
    gif.fps = Some(60);
    gif.loop_count = 3;
    let gif_receipt = studio_renders::enqueue_job(&folder, &gif, None).expect("gif enqueues");
    assert_eq!(gif_receipt.seq, 2);
    assert_eq!(gif_receipt.fps, 30, "GIF clamps to 30 fps at enqueue");
    assert!(gif_receipt.output.ends_with("t-2.gif"), "got {}", gif_receipt.output);
    let jobs = queue_of(&folder);
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].format, "webm");
    assert_eq!(jobs[0].quality, "high");
    assert_eq!(jobs[0].fps, Some(24));
    assert_eq!(jobs[0].resolution, "1080p");
    assert!(jobs[0].gpu);
    assert_eq!(jobs[1].kind, "gif");
    assert_eq!(jobs[1].loop_count, 3);
    assert!(
        jobs[1].notes.iter().any(|n| n.contains("capped at 30")),
        "the clamp is recorded: {:?}",
        jobs[1].notes
    );
    let mut mov = enqueue_req("mov");
    mov.resolution = "4k".to_string();
    let mov_receipt = studio_renders::enqueue_job(&folder, &mov, None).expect("mov enqueues");
    assert!(mov_receipt.output.ends_with("t-3.mov"), "got {}", mov_receipt.output);
}

#[test]
fn enqueue_unknown_composition_refuses_listing_what_the_project_holds() {
    let _lock = serial();
    let folder = tmp("unknown-comp");
    tiny_project(&folder);
    let before = tree_bytes(&folder);
    let mut req = enqueue_req("mp4");
    req.composition = Some("nope".to_string());
    let err = studio_renders::enqueue_job(&folder, &req, None).expect_err("unknown refuses");
    assert!(err.contains("unknown composition"), "{err}");
    assert!(err.contains("\"t\""), "{err}");
    assert!(queue_of(&folder).is_empty());
    assert_eq!(tree_bytes(&folder), before, "nothing queued");
}

#[test]
fn enqueue_invalid_project_refuses_naming_the_problems() {
    let _lock = serial();
    let folder = tmp("invalid");
    tiny_project(&folder);
    write(&folder, "composition.kdl", "composition name=\n");
    let before = tree_bytes(&folder);
    let err = studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None)
        .expect_err("invalid refuses");
    assert!(err.contains("invalid"), "{err}");
    assert_eq!(tree_bytes(&folder), before, "nothing queued");
}

#[test]
fn missing_ffmpeg_refuses_instead_of_queueing() {
    let _lock = serial();
    let folder = tmp("no-ffmpeg");
    tiny_project(&folder);
    let before = tree_bytes(&folder);
    let err = studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), empty_path())
        .expect_err("missing FFmpeg refuses");
    assert!(err.contains("FFmpeg"), "{err}");
    assert!(queue_of(&folder).is_empty(), "nothing queued");
    assert_eq!(tree_bytes(&folder), before, "nothing written");
    let (available, detail) = studio_renders::ffmpeg_status(empty_path());
    assert!(!available);
    assert!(detail.contains("FFmpeg"), "{detail}");
    let (available, _) = studio_renders::ffmpeg_status(None);
    assert!(available, "the test host ships FFmpeg");
}

#[test]
fn gif_over_cap_refuses_naming_the_cap_and_the_measured_length() {
    let _lock = serial();
    let folder = tmp("overcap");
    overcap_project(&folder);
    let before = tree_bytes(&folder);
    let err = studio_renders::enqueue_job(&folder, &enqueue_req("gif"), None)
        .expect_err("over-cap GIF refuses");
    assert!(err.contains("900"), "{err}");
    assert!(err.contains("1200"), "{err}");
    assert!(queue_of(&folder).is_empty());
    assert_eq!(tree_bytes(&folder), before, "nothing queued or written");
}

// --- execution -------------------------------------------------------------

#[test]
fn queue_runs_jobs_in_order_with_measured_progress() {
    let _lock = serial();
    let folder = tmp("order");
    mid_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("job 1 queues");
    let mut second = enqueue_req("webm");
    second.fps = Some(24);
    studio_renders::enqueue_job(&folder, &second, None).expect("job 2 queues");
    // Progress scopes per job: the mp4 job pipes 120 frames, the 24
    // fps webm job pipes 96.
    let seen_first = RefCell::new(Vec::<(u64, u64)>::new());
    let hooks_first = studio_renders::RunHooks {
        path_value: None,
        on_progress: Some(&|done, total| {
            seen_first.borrow_mut().push((done, total));
        }),
    };
    let first = studio_renders::run_next_queued(&folder, &hooks_first).expect("a job runs");
    assert_eq!(first.seq, 1, "jobs run in queue order");
    assert_eq!(first.status, "succeeded", "{}", first.detail);
    let seen_second = RefCell::new(Vec::<(u64, u64)>::new());
    let hooks_second = studio_renders::RunHooks {
        path_value: None,
        on_progress: Some(&|done, total| {
            seen_second.borrow_mut().push((done, total));
        }),
    };
    let second_result = studio_renders::run_next_queued(&folder, &hooks_second).expect("the queue advances");
    assert_eq!(second_result.seq, 2);
    assert_eq!(second_result.status, "succeeded", "{}", second_result.detail);
    assert!(
        studio_renders::run_next_queued(&folder, &studio_renders::RunHooks::default()).is_none(),
        "no queued job remains"
    );
    let seen_first = seen_first.borrow();
    let seen_second = seen_second.borrow();
    for (seen, total) in [(&*seen_first, 120), (&*seen_second, 96)] {
        assert!(seen.len() >= 2, "progress measured at least twice: {seen:?}");
        assert!(
            seen.windows(2).all(|w| w[1].0 >= w[0].0),
            "frames done never go backward: {seen:?}"
        );
        assert!(
            seen.iter().all(|(_, t)| *t == total),
            "the total stays the measured frame count: {seen:?}"
        );
        assert_eq!(*seen.last().unwrap(), (total, total));
    }
    let jobs = queue_of(&folder);
    assert!(jobs.iter().all(|j| j.status == "succeeded"));
    assert_eq!(jobs[0].total_frames, 120);
    assert_eq!(jobs[1].total_frames, 96);
    assert!(
        jobs.iter().all(|j| Path::new(&j.output).exists()),
        "every job delivered its file"
    );
}

#[test]
fn default_queue_render_is_deterministic_with_headless_traits() {
    let _lock = serial();
    let folder = tmp("deterministic");
    tiny_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues");
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues again");
    let hooks = studio_renders::RunHooks::default();
    for seq in [1, 2] {
        let result = studio_renders::run_next_queued(&folder, &hooks).expect("runs");
        assert_eq!(result.seq, seq);
        assert_eq!(result.status, "succeeded", "{}", result.detail);
    }
    let jobs = queue_of(&folder);
    let first = std::fs::read(&jobs[0].output).unwrap();
    let second = std::fs::read(&jobs[1].output).unwrap();
    assert_eq!(first, second, "same inputs re-render byte-identically");
    let probe = ffprobe_show(
        Path::new(&jobs[0].output),
        "stream=codec_name,width,height,avg_frame_rate",
    );
    assert!(probe.contains("codec_name=h264"), "{probe}");
    assert!(probe.contains("width=320"), "{probe}");
    assert!(probe.contains("height=180"), "{probe}");
    assert!(probe.contains("avg_frame_rate=30/1"), "{probe}");
}

#[test]
fn cancel_mid_run_removes_partial_output_and_reports_the_stage() {
    let _lock = serial();
    let folder = tmp("cancel");
    long_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues");
    let hooks = studio_renders::RunHooks {
        path_value: None,
        on_progress: Some(&|done, _| {
            if done >= 1 {
                render::cancel::request();
            }
        }),
    };
    let result = studio_renders::run_next_queued(&folder, &hooks).expect("runs");
    assert_eq!(result.seq, 1);
    assert_eq!(result.status, "cancelled", "{}", result.detail);
    let jobs = queue_of(&folder);
    assert_eq!(jobs[0].status, "cancelled");
    assert!(
        jobs[0].stage == "frames" || jobs[0].stage == "encode",
        "the row reports the stage reached: {}",
        jobs[0].stage
    );
    let message = jobs[0].error.clone().unwrap_or_default();
    assert!(message.contains("cancelled"), "{message}");
    assert!(
        message.contains(&jobs[0].stage),
        "the report names the stage: {message}"
    );
    assert!(
        !Path::new(&jobs[0].output).exists(),
        "the partial output is removed"
    );
    let part = format!("{}.framesmith-part", jobs[0].output);
    assert!(!Path::new(&part).exists(), "no staged temp survives");
}

#[test]
fn failed_job_names_its_stage_while_the_queue_advances() {
    let _lock = serial();
    let folder = tmp("fail");
    tiny_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("job 1 queues");
    studio_renders::enqueue_job(&folder, &enqueue_req("mov"), None).expect("job 2 queues");
    write(&folder, "composition.kdl", "composition name=\n");
    let hooks = studio_renders::RunHooks::default();
    let failed = studio_renders::run_next_queued(&folder, &hooks).expect("job 1 runs");
    assert_eq!(failed.seq, 1);
    assert_eq!(failed.status, "failed", "{}", failed.detail);
    let jobs = queue_of(&folder);
    assert_eq!(jobs[0].status, "failed");
    assert_eq!(jobs[0].stage, "validate");
    assert!(jobs[0].error.clone().unwrap_or_default().contains("validate"));
    assert_eq!(jobs[1].status, "queued", "the failure never blocks the queue");
    tiny_project(&folder);
    let revived = studio_renders::run_next_queued(&folder, &hooks).expect("job 2 runs");
    assert_eq!(revived.seq, 2, "the queue advanced past the failure");
    assert_eq!(revived.status, "succeeded", "{}", revived.detail);
}

// --- job actions -----------------------------------------------------------

#[test]
fn cancel_queued_job_reports_no_output_written() {
    let _lock = serial();
    let folder = tmp("cancel-queued");
    tiny_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues");
    let report = studio_renders::cancel_job(&folder, 1).expect("queued cancels");
    assert!(report.contains("no output was written"), "{report}");
    let jobs = queue_of(&folder);
    assert_eq!(jobs[0].status, "cancelled");
    assert_eq!(jobs[0].stage, "queued");
    let again = studio_renders::cancel_job(&folder, 1).expect_err("terminal refuses");
    assert!(again.contains("already cancelled"), "{again}");
    let unknown = studio_renders::cancel_job(&folder, 9).expect_err("unknown refuses");
    assert!(unknown.contains("unknown job"), "{unknown}");
}

#[test]
fn delete_says_what_was_removed_and_refuses_running_jobs() {
    let _lock = serial();
    let folder = tmp("delete");
    tiny_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues");
    let report = studio_renders::delete_job(&folder, 1).expect("queued deletes");
    assert!(report.contains("no output existed"), "{report}");
    assert!(queue_of(&folder).is_empty());
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues again");
    studio_renders::run_next_queued(&folder, &studio_renders::RunHooks::default())
        .expect("runs");
    let output = queue_of(&folder)[0].output.clone();
    assert!(Path::new(&output).exists());
    let report = studio_renders::delete_job(&folder, 1).expect("finished deletes");
    assert!(report.contains(&output), "{report}");
    assert!(!Path::new(&output).exists(), "the output file goes with the row");
    assert!(queue_of(&folder).is_empty());
    let unknown = studio_renders::delete_job(&folder, 7).expect_err("unknown refuses");
    assert!(unknown.contains("unknown job"), "{unknown}");
    // A crafted running row refuses until cancelled.
    let raw = format!(
        "{{\"version\":1,\"jobs\":[{{\"seq\":3,\"composition\":\"t\",\"status\":\"running\",\"output\":\"{}/ghost.mp4\",\"actor\":\"maker\",\"lint\":0}}]}}",
        folder.join(".framesmith").join("studio-renders").display()
    );
    std::fs::create_dir_all(folder.join(".framesmith")).unwrap();
    std::fs::write(
        folder.join(".framesmith").join("studio-render-queue.json"),
        raw,
    )
    .unwrap();
    let err = studio_renders::delete_job(&folder, 3).expect_err("running refuses");
    assert!(err.contains("is running"), "{err}");
    assert!(err.contains("cancel it"), "{err}");
    assert_eq!(queue_of(&folder).len(), 1, "nothing deleted");
}

#[test]
fn hide_finished_clears_only_completed_rows_with_their_outputs() {
    let _lock = serial();
    let folder = tmp("hide");
    tiny_project(&folder);
    for _ in 0..2 {
        studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues");
    }
    let hooks = studio_renders::RunHooks::default();
    let done = studio_renders::run_next_queued(&folder, &hooks).expect("job 1 runs");
    assert_eq!(done.status, "succeeded");
    studio_renders::cancel_job(&folder, 2).expect("job 2 cancels while queued");
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("job 3 queues");
    let output = queue_of(&folder)[0].output.clone();
    assert!(Path::new(&output).exists());
    let report = studio_renders::hide_finished(&folder).expect("hides");
    assert!(report.contains('1') && report.contains('2'), "{report}");
    assert!(!Path::new(&output).exists(), "finished outputs go with their rows");
    let jobs = queue_of(&folder);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].seq, 3);
    assert_eq!(jobs[0].status, "queued");
    let empty = studio_renders::hide_finished(&folder).expect("hides again");
    assert!(empty.contains("no finished jobs"), "{empty}");
}

#[test]
fn download_byte_matches_the_rendered_file() {
    let _lock = serial();
    let folder = tmp("download");
    tiny_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues");
    let queued = studio_renders::download_job(&folder, 1).expect_err("queued never downloads");
    assert!(queued.contains("queued"), "{queued}");
    studio_renders::run_next_queued(&folder, &studio_renders::RunHooks::default())
        .expect("runs");
    let (output, bytes) = studio_renders::download_job(&folder, 1).expect("downloads");
    assert_eq!(bytes, std::fs::read(&output).unwrap());
    assert!(!bytes.is_empty());
    let unknown = studio_renders::download_job(&folder, 9).expect_err("unknown refuses");
    assert!(unknown.contains("unknown job"), "{unknown}");
}

// --- GIF exports -----------------------------------------------------------

#[test]
fn gif_exports_loop_as_chosen_at_the_capped_rate_with_no_audio() {
    let _lock = serial();
    let folder = tmp("gif");
    gif_project(&folder);
    let mut one = enqueue_req("gif");
    one.loop_count = 1;
    studio_renders::enqueue_job(&folder, &one, None).expect("loop-1 queues");
    let mut forever = enqueue_req("gif");
    forever.loop_count = 0;
    studio_renders::enqueue_job(&folder, &forever, None).expect("forever queues");
    let hooks = studio_renders::RunHooks::default();
    for seq in [1, 2] {
        let result = studio_renders::run_next_queued(&folder, &hooks).expect("runs");
        assert_eq!(result.seq, seq);
        assert_eq!(result.status, "succeeded", "{}", result.detail);
    }
    let jobs = queue_of(&folder);
    let first = std::fs::read(&jobs[0].output).unwrap();
    let second = std::fs::read(&jobs[1].output).unwrap();
    assert_eq!(gif_loop_count(&first), Some(1), "the chosen loop count lands in the file");
    assert_eq!(gif_loop_count(&second), Some(0), "loop count zero means forever");
    for (job, bytes) in [&jobs[0], &jobs[1]].iter().zip([&first, &second]) {
        assert_eq!(bytes[0..6], *b"GIF89a", "a real GIF encodes");
        // GIF timestamps quantize to centiseconds, so the container
        // rate wobbles around the 15 fps feed; the frame count and
        // the duration stay exact.
        let probe = ffprobe_show(
            Path::new(&job.output),
            "stream=codec_name,nb_frames,avg_frame_rate:format=duration",
        );
        assert!(probe.contains("codec_name=gif"), "{probe}");
        assert!(probe.contains("nb_frames=30"), "{probe}");
        let fps = parse_rate(&probe, "avg_frame_rate=");
        assert!((fps - 15.0).abs() <= 2.0, "near the 15 fps feed: {probe}");
        let duration: f64 = probe
            .lines()
            .find_map(|l| l.strip_prefix("duration="))
            .and_then(|v| v.parse().ok())
            .unwrap_or(-1.0);
        assert!((duration - 2.0).abs() < 0.3, "{probe}");
        let streams = ffprobe_show(Path::new(&job.output), "stream=index");
        assert_eq!(streams.lines().count(), 1, "GIF carries no audio: {streams}");
    }
    assert!(
        jobs.iter().all(|j| j.kind == "gif"),
        "GIF jobs ride the queue as GIF"
    );
}

#[test]
fn gif_over_fps_request_clamps_to_thirty_with_a_note() {
    let _lock = serial();
    let folder = tmp("gif-clamp");
    tiny_project(&folder);
    let mut req = enqueue_req("gif");
    req.fps = Some(60);
    req.loop_count = 2;
    let receipt = studio_renders::enqueue_job(&folder, &req, None).expect("queues");
    assert_eq!(receipt.fps, 30);
    studio_renders::run_next_queued(&folder, &studio_renders::RunHooks::default())
        .expect("runs");
    let jobs = queue_of(&folder);
    assert_eq!(jobs[0].status, "succeeded");
    assert!(
        jobs[0].notes.iter().any(|n| n.contains("capped at 30")),
        "the clamp is recorded: {:?}",
        jobs[0].notes
    );
    // The 60 fps request encodes at the 30 fps cap (60 frames over
    // two seconds); the container rate wobbles with centisecond
    // quantization.
    let probe = ffprobe_show(
        Path::new(&jobs[0].output),
        "stream=nb_frames,avg_frame_rate:format=duration",
    );
    assert!(probe.contains("nb_frames=60"), "{probe}");
    let fps = parse_rate(&probe, "avg_frame_rate=");
    assert!((fps - 30.0).abs() <= 4.0, "near the 30 fps cap: {probe}");
    let bytes = std::fs::read(&jobs[0].output).unwrap();
    assert_eq!(gif_loop_count(&bytes), Some(2));
}

/// Parse an `avg_frame_rate=A/B` probe line as a float.
fn parse_rate(probe: &str, prefix: &str) -> f64 {
    probe
        .lines()
        .find_map(|l| l.strip_prefix(prefix))
        .and_then(|v| {
            let (a, b) = v.split_once('/')?;
            Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?)
        })
        .unwrap_or(-1.0)
}

// --- formats, quality, resolution, GPU -------------------------------------

#[test]
fn mov_writes_a_quicktime_container_and_webm_stays_decodable() {
    let _lock = serial();
    let folder = tmp("mov-webm");
    tiny_project(&folder);
    studio_renders::enqueue_job(&folder, &enqueue_req("mov"), None).expect("mov queues");
    studio_renders::enqueue_job(&folder, &enqueue_req("webm"), None).expect("webm queues");
    let hooks = studio_renders::RunHooks::default();
    for seq in [1, 2] {
        let result = studio_renders::run_next_queued(&folder, &hooks).expect("runs");
        assert_eq!(result.seq, seq);
        assert_eq!(result.status, "succeeded", "{}", result.detail);
    }
    let jobs = queue_of(&folder);
    let mov_probe = ffprobe_show(
        Path::new(&jobs[0].output),
        "format=format_name:stream=codec_name",
    );
    assert!(mov_probe.contains("mov"), "{mov_probe}");
    assert!(mov_probe.contains("codec_name=h264"), "{mov_probe}");
    let webm_probe = ffprobe_show(
        Path::new(&jobs[1].output),
        "format=format_name:stream=codec_name",
    );
    assert!(webm_probe.contains("webm"), "{webm_probe}");
    assert!(webm_probe.contains("codec_name=vp9"), "{webm_probe}");
}

#[test]
fn quality_resolution_and_gpu_settings_land_in_the_output() {
    let _lock = serial();
    let folder = tmp("qr-gpu");
    tiny_project(&folder);
    let mut req = enqueue_req("mp4");
    req.quality = "draft".to_string();
    req.resolution = "1080p".to_string();
    req.gpu = true;
    studio_renders::enqueue_job(&folder, &req, None).expect("queues");
    let hooks = studio_renders::RunHooks::default();
    let result = studio_renders::run_next_queued(&folder, &hooks).expect("runs");
    assert_eq!(result.status, "succeeded", "{}", result.detail);
    let jobs = queue_of(&folder);
    assert_eq!(jobs[0].quality, "draft");
    assert_eq!(jobs[0].resolution, "1080p");
    assert!(jobs[0].gpu);
    assert!(
        !jobs[0].notes.is_empty(),
        "the GPU toggle reports what it did: {:?}",
        jobs[0].notes
    );
    let probe = ffprobe_show(Path::new(&jobs[0].output), "stream=width,height,codec_name");
    assert!(probe.contains("width=1920"), "{probe}");
    assert!(probe.contains("height=1080"), "{probe}");
    assert!(probe.contains("codec_name=h264"), "{probe}");
}

#[test]
fn resolution_4k_delivers_ultra_high_definition() {
    let _lock = serial();
    let folder = tmp("uhd");
    tiny_project(&folder);
    let mut req = enqueue_req("mp4");
    req.resolution = "4k".to_string();
    studio_renders::enqueue_job(&folder, &req, None).expect("queues");
    let result =
        studio_renders::run_next_queued(&folder, &studio_renders::RunHooks::default())
            .expect("runs");
    assert_eq!(result.status, "succeeded", "{}", result.detail);
    let jobs = queue_of(&folder);
    let probe = ffprobe_show(Path::new(&jobs[0].output), "stream=width,height");
    assert!(probe.contains("width=3840"), "{probe}");
    assert!(probe.contains("height=2160"), "{probe}");
}

// --- readout: progress and ETA ---------------------------------------------

#[test]
fn readout_measures_progress_and_labels_eta_an_estimate() {
    let folder = tmp("readout");
    tiny_project(&folder);
    let receipt = studio_renders::enqueue_job(&folder, &enqueue_req("mp4"), None).expect("queues");
    let jobs = queue_of(&folder);
    let raw = studio_renders::renders_queue_json(&jobs, true, "ffmpeg: suitable", 1_000);
    let value: serde_json::Value = serde_json::from_str(&raw).expect("queue renders as JSON");
    assert_eq!(value["ffmpeg"]["available"], true);
    assert_eq!(value["jobs"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(value["jobs"][0]["progress"], serde_json::Value::Null);
    assert_eq!(value["jobs"][0]["eta_secs"], serde_json::Value::Null);
    assert_eq!(value["jobs"][0]["eta_label"], serde_json::Value::Null);
    assert_eq!(value["jobs"][0]["format"], "mp4");
    assert_eq!(value["jobs"][0]["quality"], "standard");
    assert_eq!(value["jobs"][0]["resolution"], "auto");
    // A running job halfway through a ten-second run carries measured
    // progress with the ETA labeled an estimate, never a promise.
    let now = studio_renders::now_ms();
    let mut running = jobs[0].clone();
    running.status = "running".to_string();
    running.stage = "frames".to_string();
    running.started_ms = now - 10_000;
    running.total_frames = 100;
    running.done_frames = 25;
    assert_eq!(studio_renders::progress_share(&running), Some(0.25));
    assert_eq!(studio_renders::eta_secs(&running, now), Some(30.0));
    let raw = studio_renders::renders_queue_json(std::slice::from_ref(&running), true, "ok", now);
    let value: serde_json::Value = serde_json::from_str(&raw).expect("queue renders as JSON");
    assert_eq!(value["jobs"][0]["progress"], 0.25);
    assert_eq!(value["jobs"][0]["eta_secs"], 30.0);
    assert_eq!(value["jobs"][0]["eta_label"], "estimate");
    let _ = receipt;
}

// --- panel assets ------------------------------------------------------------

#[test]
fn renders_panel_controls_cover_every_setting() {
    let css = studio_renders::renders_css();
    assert!(css.contains("border-radius"), "hyphenated CSS vocabulary");
    assert!(css.contains("font-variant-numeric"));
    assert!(!css.contains("font-family"), "no font stack in panel CSS");
    let html = studio_renders::renders_html();
    for needle in [
        "panel-renders",
        "renders-form",
        "renders-format",
        "renders-quality",
        "renders-fps",
        "renders-resolution",
        "renders-gpu",
        "renders-loop",
        "renders-table",
        "renders-hide",
        "renders-status",
        "eta (estimate)",
    ] {
        assert!(html.contains(needle), "the panel shows {needle}");
    }
    for option in ["mp4", "mov", "webm", "gif", "draft", "standard", "high", "24", "30", "60", "auto", "1080p", "4k"] {
        assert!(html.contains(option), "the panel offers {option}");
    }
    let js = studio_renders::renders_js();
    for needle in [
        "/api/studio/renders",
        "paintRenders",
        "postRendersEnqueue",
        "postRendersCancel",
        "postRendersDelete",
        "postRendersHide",
        "(estimate)",
    ] {
        assert!(js.contains(needle), "the panel script paints {needle}");
    }
    assert!(!js.contains("font-family"), "no font stack in panel script");
}

// --- HTTP: routes and the queue worker ---------------------------------------

/// Minimal HTTP exchange over loopback with an optional body.
/// Returns (status code, body).
fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let payload = body.unwrap_or("");
    let extra = if method == "DELETE" {
        String::new()
    } else {
        format!("Content-Length: {}\r\n", payload.len())
    };
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n{extra}Connection: close\r\n\r\n{payload}"
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let code: u16 = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .expect("status line");
    let body = match raw.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(i) => raw[i + 4..].to_vec(),
        None => Vec::new(),
    };
    (code, body)
}

fn body_str(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, String) {
    let (code, raw) = http(port, method, target, body);
    (code, String::from_utf8_lossy(&raw).into_owned())
}

fn json(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, serde_json::Value) {
    let (code, text) = body_str(port, method, target, body);
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("not JSON: {text}"));
    (code, value)
}

/// Run one real server for this test: `setup` writes the project,
/// `f` drives it, then the server shuts down and the thread joins.
fn with_server(
    tag: &str,
    setup: impl FnOnce(&Path),
    f: impl FnOnce(&Path, u16),
) {
    let root = tmp(tag);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    setup(&folder);
    let reg = root.join("registry.json");
    let dir = daemon::canonical_dir(&folder).unwrap();
    let opts = ServeOptions {
        folder: folder.clone(),
        port: 0,
        registry: reg.clone(),
        composition: None,
    };
    let handle = std::thread::spawn(move || run_serve(&opts));
    let start = Instant::now();
    let entry = loop {
        let entries = daemon::load_registry(&reg).unwrap_or_default();
        if let Some(e) = entries
            .into_iter()
            .find(|e| e.dir == dir && daemon::is_live(e.port))
        {
            break e;
        }
        if handle.is_finished() {
            panic!("server exited before serving");
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "server never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    f(&folder, entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(entry.port), "port is quiet after shutdown");
}

fn wait_for_status(port: u16, seq: u64, timeout: Duration) -> serde_json::Value {
    let start = Instant::now();
    loop {
        let (code, state) = json(port, "GET", "/api/studio/renders", None);
        assert_eq!(code, 200, "the queue answers: {state}");
        let job = state["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["seq"] == seq)
            .unwrap_or_else(|| panic!("job {seq} lists: {state}"))
            .clone();
        if job["status"] == "queued" || job["status"] == "running" {
            assert!(
                start.elapsed() < timeout,
                "job {seq} never finished: {job}"
            );
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        return job;
    }
}

#[test]
fn restarted_server_fails_interrupted_rows_while_queued_rows_wait() {
    let _lock = serial();
    with_server(
        "interrupted",
        |folder| {
            tiny_project(folder);
            let renders = folder.join(".framesmith").join("studio-renders");
            std::fs::create_dir_all(&renders).unwrap();
            let partial = renders.join("t-1.mp4");
            std::fs::write(&partial, b"partial").unwrap();
            std::fs::write(
                folder.join(".framesmith").join("studio-render-queue.json"),
                format!(
                    "{{\"version\":1,\"jobs\":[{{\"seq\":1,\"composition\":\"t\",\"status\":\"running\",\"stage\":\"frames\",\"output\":\"{}\",\"actor\":\"maker\",\"lint\":0}},{{\"seq\":2,\"composition\":\"t\",\"status\":\"queued\",\"output\":\"{}\",\"actor\":\"maker\",\"lint\":0}}]}}",
                    partial.display(),
                    renders.join("t-2.mp4").display()
                ),
            )
            .unwrap();
        },
        |folder, port| {
            let (code, state) = json(port, "GET", "/api/studio/renders", None);
            assert_eq!(code, 200);
            let jobs = state["jobs"].as_array().unwrap();
            assert_eq!(jobs.len(), 2);
            assert_eq!(jobs[0]["status"], "failed");
            assert!(jobs[0]["error"]
                .as_str()
                .unwrap_or_default()
                .contains("interrupted"));
            assert!(
                jobs[1]["status"] == "queued" || jobs[1]["status"] == "running",
                "the queued row waits its turn or runs normally, never fails: {}",
                jobs[1]
            );
            assert!(
                !folder
                    .join(".framesmith")
                    .join("studio-renders")
                    .join("t-1.mp4")
                    .exists(),
                "the partial output goes with the interrupted row"
            );
        },
    );
}

#[test]
fn studio_page_serves_the_renders_panel() {
    let _lock = serial();
    with_server("page", tiny_project, |_folder, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        for needle in [
            "panel-renders",
            "renders-form",
            "renders-format",
            "renders-loop",
            "renders-table",
            "renders-status",
            "paintRenders",
        ] {
            assert!(page.contains(needle), "the shell paints {needle}");
        }
    });
}

#[test]
fn http_renders_flow_queues_runs_downloads_and_deletes() {
    let _lock = serial();
    with_server("flow", tiny_project, |folder, port| {
        let before_kdl = std::fs::read(folder.join("composition.kdl")).unwrap();
        let turns_before = sdk::history::list_turns(folder)
            .map(|l| l.turns.len())
            .unwrap_or(0);
        let (code, receipt) = json(
            port,
            "POST",
            "/api/studio/renders",
            Some("{\"actor\":\"maker\",\"format\":\"mp4\"}"),
        );
        assert_eq!(code, 200, "enqueue answers: {receipt}");
        assert_eq!(receipt["seq"], 1);
        let (code, state) = json(port, "GET", "/api/studio/renders", None);
        assert_eq!(code, 200);
        assert_eq!(state["ffmpeg"]["available"], true);
        assert_eq!(state["jobs"].as_array().map(|a| a.len()), Some(1));
        let job = wait_for_status(port, 1, Duration::from_secs(120));
        assert_eq!(job["status"], "succeeded", "{job}");
        assert_eq!(job["format"], "mp4");
        assert!(job["output_bytes"].as_u64().unwrap_or(0) > 0, "{job}");
        let output = job["output"].as_str().unwrap().to_string();
        let (code, bytes) = http(port, "GET", "/api/studio/renders/1/download", None);
        assert_eq!(code, 200, "download answers");
        assert_eq!(bytes, std::fs::read(&output).unwrap(), "download byte-matches");
        assert!(!bytes.is_empty());
        let (code, report) = body_str(port, "DELETE", "/api/studio/renders/1", None);
        assert_eq!(code, 200, "{report}");
        assert!(report.contains(&output), "{report}");
        assert!(!Path::new(&output).exists(), "delete removes the output");
        let (code, state) = json(port, "GET", "/api/studio/renders", None);
        assert_eq!(code, 200);
        assert_eq!(state["jobs"].as_array().map(|a| a.len()), Some(0));
        assert_eq!(
            std::fs::read(folder.join("composition.kdl")).unwrap(),
            before_kdl,
            "serving and rendering never touch sources"
        );
        assert!(
            folder.join(".framesmith").join("studio-render-queue.json").exists(),
            "the queue persists as hidden studio state"
        );
        assert_eq!(
            sdk::history::list_turns(folder)
                .map(|l| l.turns.len())
                .unwrap_or(0),
            turns_before,
            "queueing and rendering record no history turn: only hidden studio state changes"
        );
    });
}

#[test]
fn http_cancel_running_job_reports_cancelled_with_no_partial() {
    let _lock = serial();
    with_server("http-cancel", http_cancel_project, |_folder, port| {
        let (code, receipt) = json(
            port,
            "POST",
            "/api/studio/renders",
            Some("{\"actor\":\"maker\",\"format\":\"mp4\"}"),
        );
        assert_eq!(code, 200, "enqueue answers: {receipt}");
        // Wait until frames pipe with most of the job ahead, so the
        // cancel always lands mid-run instead of racing the queued
        // row or the final batch.
        let start = Instant::now();
        loop {
            let (code, state) = json(port, "GET", "/api/studio/renders", None);
            assert_eq!(code, 200);
            let job = state["jobs"][0].clone();
            let done = job["done_frames"].as_u64().unwrap_or(0);
            let total = job["total_frames"].as_u64().unwrap_or(0);
            if job["status"] == "running" && total > 0 && done > 0 && total - done > 64 {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "frames never piped with runway left: {job}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let (code, report) = body_str(port, "POST", "/api/studio/renders/1/cancel", None);
        assert_eq!(code, 200, "{report}");
        assert!(report.contains("cancel requested"), "{report}");
        let job = wait_for_status(port, 1, Duration::from_secs(120));
        assert_eq!(job["status"], "cancelled", "{job}");
        assert!(
            job["stage"] == "frames" || job["stage"] == "encode",
            "the row reports the stage reached: {job}"
        );
        let message = job["error"].as_str().unwrap_or_default();
        assert!(message.contains("cancelled"), "{job}");
        assert!(message.contains(job["stage"].as_str().unwrap()), "{job}");
        let output = job["output"].as_str().unwrap().to_string();
        assert!(!Path::new(&output).exists(), "no partial output survives");
        let part = format!("{output}.framesmith-part");
        assert!(!Path::new(&part).exists(), "no staged temp survives");
    });
}

#[test]
fn http_refusals_name_the_cause_with_the_right_status() {
    let _lock = serial();
    with_server("refusals", tiny_project, |_folder, port| {
        let (code, text) = body_str(port, "POST", "/api/studio/renders", Some(""));
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("JSON body"), "{text}");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/renders",
            Some("{\"actor\":\"maker\",\"composition\":\"nope\"}"),
        );
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("unknown composition"), "{text}");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/renders",
            Some("{\"actor\":\"maker\",\"format\":\"avi\"}"),
        );
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("unknown queue format"), "{text}");
        let (code, text) = body_str(port, "POST", "/api/studio/renders/9/cancel", None);
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("unknown job"), "{text}");
        let (code, text) = body_str(port, "DELETE", "/api/studio/renders/9", None);
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("unknown job"), "{text}");
        let (code, text) = body_str(port, "GET", "/api/studio/renders/9/download", None);
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("unknown job"), "{text}");
        let (code, job) = json(
            port,
            "POST",
            "/api/studio/renders",
            Some("{\"actor\":\"maker\",\"format\":\"mp4\"}"),
        );
        assert_eq!(code, 200);
        let (code, text) = body_str(port, "GET", "/api/studio/renders/1/download", None);
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("only succeeded jobs"), "{text}");
        let _ = job;
    });
}
