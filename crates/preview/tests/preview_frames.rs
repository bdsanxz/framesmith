//! Preview frame service regressions (Sprint 14, T001 / CAP-001):
//! every acceptance criterion of `T001-serve-any-frame-exactly.md`,
//! including refusal variants.

use preview::{CacheSource, FrameErrorKind, PreviewOptions, PreviewSession, RefusalKind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);
/// Serializes tests that mutate process-global render tuning
/// (`FRAMESMITH_WORKERS`): the picture never depends on it, but the
/// setting itself is process-global.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-preview-{}-{}-{}",
        tag,
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

fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn ffmpeg() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH is set");
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("ffmpeg");
        if cand.is_file() {
            return cand;
        }
        #[cfg(windows)]
        {
            let cand = dir.join("ffmpeg.exe");
            if cand.is_file() {
                return cand;
            }
        }
    }
    panic!("tests need ffmpeg on PATH");
}

/// Sorted map of every file under `folder` to its bytes: the
/// never-writes-into-the-project oracle.
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

#[test]
fn scattered_frames_equal_the_full_render_byte_identically() {
    let root = tmp("exact");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    assert_eq!(session.frame_count(), 10);
    assert_eq!(session.canvas(), (320, 180));
    // Scatter across the timeline, out of order.
    let mut served = Vec::new();
    for index in [7u64, 0, 9, 3, 5, 1] {
        let frame = session.frame_png(index).expect("frame serves");
        assert_eq!(frame.index, index);
        assert_eq!((frame.width, frame.height), (320, 180));
        assert_eq!(frame.served_from, CacheSource::Rendered);
        served.push((index, frame.png));
    }
    // Independent oracle: validate, prepare, and draw the same frames
    // straight through the render pipeline, then encode with the same
    // PNG encoder. No session, no cache — just the pipeline.
    let verdict = validate::validate::validate_folder_with_variables_strict_themed(
        &folder,
        &validate::validate::VariableOverrides::new(),
        false,
        None,
    );
    assert!(matches!(verdict.status, validate::validate::Status::Valid));
    let comp = verdict.composition.clone().unwrap();
    let prep = render::render::prepare_frames(&comp, &verdict.compositions, &folder, &ffmpeg())
        .expect("prepares");
    let frames: Vec<u64> = served.iter().map(|(i, _)| *i).collect();
    let drawn =
        render::render::draw_batch_parallel(&prep, session.backend(), &frames).expect("draws");
    for (index, bytes) in &served {
        let (_, rgb) = drawn.iter().find(|(k, _)| k == index).unwrap();
        let expected = render::outputs::encode_png_rgb8(320, 180, rgb).unwrap();
        assert_eq!(
            *bytes, expected,
            "served frame {index} must equal the full-render frame byte-identically"
        );
    }
}

#[test]
fn repeated_requests_serve_from_cache_with_identical_bytes() {
    let root = tmp("cache");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let first = session.frame_png(4).expect("frame serves");
    assert_eq!(first.served_from, CacheSource::Rendered);
    let second = session.frame_png(4).expect("repeat serves");
    assert_eq!(second.served_from, CacheSource::Memory);
    assert_eq!(
        first.png, second.png,
        "cache repeats must be identical bytes"
    );
    let stats = session.stats();
    assert_eq!(stats.rendered, 1);
    assert_eq!(stats.memory_hits, 1);
    assert_eq!(stats.disk_hits, 0);
}

#[test]
fn disk_cache_survives_memory_eviction_with_identical_bytes() {
    let root = tmp("disk");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = PreviewSession::open(
        &folder,
        PreviewOptions {
            memory_frames: 1,
            ..PreviewOptions::default()
        },
    )
    .expect("valid project opens");
    let outside = session.cache_dir().to_path_buf();
    assert!(
        !outside.starts_with(&folder),
        "disk cache must live outside the project: {outside:?}"
    );
    let a = session.frame_png(0).expect("frame 0 serves").png;
    let _ = session.frame_png(1).expect("frame 1 serves");
    // Frame 0 fell out of the one-slot memory cache: the repeat must
    // come from disk, with identical bytes and no redraw.
    let again = session.frame_png(0).expect("frame 0 repeats");
    assert_eq!(again.served_from, CacheSource::Disk);
    assert_eq!(a, again.png, "disk repeats must be identical bytes");
    let stats = session.stats();
    assert_eq!(stats.rendered, 2);
    assert_eq!(stats.disk_hits, 1);
}

#[test]
fn frames_render_in_any_order_with_worker_count_independence() {
    let _env = ENV_LOCK.lock().unwrap();
    let root = tmp("workers");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let order = [9u64, 0, 4, 7, 2, 8, 1, 3, 6, 5];
    let mut drawn_by_workers: Vec<Vec<Vec<u8>>> = Vec::new();
    for workers in ["1", "4"] {
        std::env::set_var("FRAMESMITH_WORKERS", workers);
        let mut session =
            PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
        let mut frames = Vec::new();
        for index in order {
            frames.push(session.frame_png(index).expect("frame serves").png);
        }
        drawn_by_workers.push(frames);
    }
    std::env::remove_var("FRAMESMITH_WORKERS");
    assert_eq!(
        drawn_by_workers[0], drawn_by_workers[1],
        "any worker count draws byte-identical frames in any order"
    );
}

#[test]
fn out_of_range_frame_refuses_naming_the_window() {
    let root = tmp("range");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    assert_eq!(session.frame_count(), 10);
    let err = session.frame_png(10).expect_err("frame 10 is out of range");
    assert_eq!(err.kind, FrameErrorKind::OutOfRange);
    assert!(err.message.contains("frame 10"), "{err}");
    assert!(err.message.contains("10 frame(s)"), "{err}");
    assert!(session.last_good().is_none(), "nothing served yet");
}

#[test]
fn unrunnable_frame_reports_its_cause_with_last_good_kept() {
    let root = tmp("unrunnable");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // An image project: deleting the source mid-session makes later
    // uncached frames unrunnable, naming the file.
    let out = Command::new(ffmpeg())
        .args([
            "-f",
            "lavfi",
            "-i",
            "nullsrc=size=32x32:rate=1",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgba",
            &folder.join("half.png").display().to_string(),
        ])
        .output()
        .expect("run ffmpeg");
    assert!(out.status.success(), "ffmpeg must paint half.png");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    box x=10 y=10 width=50 height=50 fill=\"#ff0000\" start=0 duration=1 order=1 {\n        image-fill src=\"half.png\"\n    }\n}\n",
    );
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let good = session.frame_png(0).expect("frame 0 serves");
    assert_eq!(session.last_good().unwrap().0, 0);
    std::fs::remove_file(folder.join("half.png")).unwrap();
    // Frame 0 stays cached (last good keeps showing); an uncached frame
    // must fail naming the vanished source.
    let cached = session.frame_png(0).expect("cached frame keeps serving");
    assert_eq!(cached.png, good.png);
    let err = session.frame_png(7).expect_err("frame 7 cannot draw now");
    assert_eq!(err.kind, FrameErrorKind::RenderFailed);
    assert!(err.message.contains("half.png"), "{err}");
    let (index, bytes) = session.last_good().expect("last good kept");
    assert_eq!(index, 0);
    assert_eq!(bytes, good.png.as_slice());
}

#[test]
fn broken_project_refuses_before_anything_starts_naming_the_cause() {
    let root = tmp("broken");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", "this is not kdl {{{{\n");
    let before = tree_bytes(&folder);
    let err = PreviewSession::open(&folder, PreviewOptions::default())
        .expect_err("broken project must refuse");
    assert_eq!(err.kind, RefusalKind::Invalid);
    assert!(err.message.contains("preview refused"), "{err}");
    assert!(err.message.contains("invalid"), "{err}");
    assert!(!err.diagnostics.is_empty(), "refusal names its diagnostics");
    let first = &err.diagnostics[0];
    assert!(first.file.ends_with("composition.kdl"), "{first:?}");
    assert!(
        !first.message.is_empty() && !first.fix.is_empty(),
        "{first:?}"
    );
    // Refusal starts nothing: the project is untouched and no outside
    // cache directory was even created for the attempt.
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn unknown_composition_refuses_listing_what_the_project_holds() {
    let root = tmp("unknown-comp");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let err = PreviewSession::open(
        &folder,
        PreviewOptions {
            composition: Some("nope".to_string()),
            ..PreviewOptions::default()
        },
    )
    .expect_err("unknown composition must refuse");
    assert_eq!(err.kind, RefusalKind::UnknownComposition);
    assert!(err.message.contains("\"t\""), "{err}");
}

#[test]
fn preview_never_writes_into_the_project_directory() {
    let root = tmp("purity");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before = tree_bytes(&folder);
    assert!(!folder.join("logs").exists());
    let cache_dir = {
        let mut session =
            PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
        for index in [0u64, 5, 9, 3] {
            session.frame_png(index).expect("frame serves");
        }
        let dir = session.cache_dir().to_path_buf();
        assert!(dir.is_dir(), "disk cache exists outside the project");
        dir
    };
    // Dropping the session cleans up its outside-project cache, and the
    // project itself never gained a file — not even a log dir shell.
    assert!(!cache_dir.exists(), "session cleanup removes its cache");
    assert_eq!(tree_bytes(&folder), before);
    assert!(!folder.join("logs").exists());
}

#[test]
fn cache_dir_inside_the_project_is_refused() {
    let root = tmp("cache-in-project");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before = tree_bytes(&folder);
    let err = PreviewSession::open(
        &folder,
        PreviewOptions {
            cache_dir: Some(folder.join("cache")),
            ..PreviewOptions::default()
        },
    )
    .expect_err("in-project cache must refuse");
    assert_eq!(err.kind, RefusalKind::PrepareFailed);
    assert_eq!(tree_bytes(&folder), before);
}
