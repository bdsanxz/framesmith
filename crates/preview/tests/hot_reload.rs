//! Hot-reload regressions (Sprint 14, T005 / CAP-005, STATE-04,
//! REC-002): every acceptance criterion of
//! `T005-hot-reload-edits-fast.md`, including refusal variants.

use preview::{CacheSource, PreviewOptions, PreviewSession, RELOAD_BUDGET_MS};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-reload-{}-{}-{}",
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
                let key = path.strip_prefix(folder).unwrap().to_string_lossy().into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn ffmpeg() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH is set");
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("ffmpeg");
        if cand.is_file() {
            return cand;
        }
    }
    panic!("tests need ffmpeg on PATH");
}

/// Independent oracle: validate, prepare, and draw `index` straight
/// through the render pipeline — no session, no cache — and encode
/// with the same PNG encoder.
fn oracle_png(folder: &Path, backend: render::render::Backend, index: u64) -> Vec<u8> {
    let verdict = validate::validate::validate_folder_with_variables_strict_themed(
        folder,
        &validate::validate::VariableOverrides::new(),
        false,
        None,
    );
    assert!(matches!(
        verdict.status,
        validate::validate::Status::Valid
    ));
    let comp = verdict.composition.clone().unwrap();
    let prep =
        render::render::prepare_frames(&comp, &verdict.compositions, folder, &ffmpeg()).expect("prepares");
    let drawn = render::render::draw_batch_parallel(&prep, backend, &[index]).expect("draws");
    let (_, rgb) = drawn.iter().find(|(k, _)| *k == index).unwrap();
    render::outputs::encode_png_rgb8(320, 180, rgb).unwrap()
}

#[test]
fn valid_edit_applies_within_500ms_with_the_new_look() {
    let root = tmp("valid");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let backend = session.backend();
    let before = session.frame_png(0).expect("frame serves").png;
    // Frame 0 is hot now: a repeat must come from memory.
    assert_eq!(
        session.frame_png(0).expect("repeat serves").served_from,
        CacheSource::Memory
    );

    // The maker saves a new look mid-playback.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#ff0000\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let report = session.reload();
    assert!(report.applied, "valid edit applies: {report}");
    assert!(report.diagnostics.is_empty());
    assert!(
        report.within_budget(),
        "reload must meet the 500 ms bar, took {}ms",
        report.elapsed_ms()
    );
    assert!(
        report.elapsed_ms() < u128::from(RELOAD_BUDGET_MS),
        "reload timing is honestly under the bar"
    );
    assert_eq!(report.frame_count, 10);
    assert_eq!(session.frame_count(), 10);

    // The next request draws the new look fresh — never a stale cache
    // hit — and the bytes equal an independent full-pipeline draw.
    let fresh = session.frame_png(0).expect("new look serves");
    assert_eq!(fresh.served_from, CacheSource::Rendered);
    assert_ne!(fresh.png, before, "the new look must differ from the old");
    assert_eq!(
        fresh.png,
        oracle_png(&folder, backend, 0),
        "reloaded frame equals the full-render frame byte-identically"
    );
}

#[test]
fn playhead_is_kept_across_a_valid_reload() {
    let root = tmp("kept");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let mut transport = session.transport();
    transport.scrub_to(7).expect("scrub acts");
    transport.play().expect("play acts");
    assert!(session.frame_png(7).is_ok());

    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#00ff00\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let report = session.reload_keep_playhead(&mut transport);
    assert!(report.applied, "valid edit applies: {report}");
    assert_eq!(report.playhead, Some(7), "the playhead is kept");
    assert_eq!(transport.position(), 7);
    assert!(transport.is_playing(), "reload never pauses playback");
    let readout = transport.readout();
    assert_eq!(readout.frame, 7);
    assert_eq!(readout.frame_count, 10);
    // The kept playhead serves the new look.
    let frame = session.frame_png(transport.position()).expect("frame serves");
    assert_eq!(frame.served_from, CacheSource::Rendered);
}

#[test]
fn shortening_edit_clamps_the_playhead_without_resetting_state() {
    let root = tmp("shorten");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let mut transport = session.transport();
    transport.scrub_to(7).expect("scrub acts");
    transport.play().expect("play acts");
    transport.set_loop(true);
    transport.set_speed(2.0).expect("speed acts");

    // The edit shortens the timeline to 2 frames: the playhead clamps
    // onto the last frame instead of resetting to zero.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=0.2 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=0.2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let report = session.reload_keep_playhead(&mut transport);
    assert!(report.applied, "valid edit applies: {report}");
    assert_eq!(report.frame_count, 2);
    assert_eq!(report.playhead, Some(1));
    assert_eq!(transport.position(), 1, "clamped, never reset to zero");
    assert!(transport.is_playing(), "reload never pauses playback");
    assert!(transport.loop_enabled(), "reload keeps loop state");
    assert_eq!(transport.speed(), 2.0, "reload keeps speed");
}

#[test]
fn broken_edit_lists_diagnostics_with_last_good_kept() {
    let root = tmp("broken");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let good = session.frame_png(3).expect("frame serves").png;
    assert_eq!(session.last_good().unwrap().0, 3);

    write(&folder, "composition.kdl", "this is not kdl {{{{\n");
    let mut transport = session.transport();
    transport.scrub_to(7).expect("scrub acts");
    let report = session.reload_keep_playhead(&mut transport);

    assert!(!report.applied, "broken edit must not apply");
    assert!(!report.diagnostics.is_empty(), "diagnostics are listed");
    let first = &report.diagnostics[0];
    assert!(first.file.ends_with("composition.kdl"), "{first:?}");
    assert!(!first.message.is_empty() && !first.fix.is_empty(), "{first:?}");
    assert!(report.message.contains("composition.kdl"), "{report}");
    assert!(report.message.contains("last good"), "{report}");
    // The timeline facts are untouched.
    assert_eq!(report.frame_count, 10);
    assert_eq!(session.frame_count(), 10);
    // Last good frames keep showing: the cached frame serves with
    // identical bytes, and last_good is intact.
    let cached = session.frame_png(3).expect("last good keeps serving");
    assert_eq!(cached.served_from, CacheSource::Memory);
    assert_eq!(cached.png, good);
    let (index, bytes) = session.last_good().expect("last good kept");
    assert_eq!(index, 3);
    assert_eq!(bytes, good.as_slice());
}

#[test]
fn playhead_never_resets_on_a_failed_edit() {
    let root = tmp("no-reset");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let mut transport = session.transport();
    transport.scrub_to(7).expect("scrub acts");
    transport.play().expect("play acts");
    assert!(session.frame_png(7).is_ok());

    write(&folder, "composition.kdl", "this is not kdl {{{{\n");
    let report = session.reload_keep_playhead(&mut transport);
    assert!(!report.applied);
    assert_eq!(report.playhead, Some(7), "the failed edit keeps the playhead");
    assert_eq!(transport.position(), 7, "never reset to zero");
    assert!(transport.is_playing(), "a failed edit never pauses either");
}

#[test]
fn plain_reload_moves_no_playhead_by_itself() {
    let root = tmp("plain");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let mut transport = session.transport();
    transport.scrub_to(5).expect("scrub acts");

    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#0000ff\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let report = session.reload();
    assert!(report.applied, "valid edit applies: {report}");
    assert_eq!(report.playhead, None, "plain reload names no playhead");
    assert_eq!(transport.position(), 5, "a transport the caller kept back is untouched");
}

#[test]
fn broken_edit_after_a_valid_reload_keeps_the_new_look() {
    let root = tmp("new-look-kept");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#ff0000\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    assert!(session.reload().applied);
    let new_look = session.frame_png(2).expect("new look serves").png;

    // A follow-up broken edit keeps the new look, not the older one.
    write(&folder, "composition.kdl", "this is not kdl {{{{\n");
    let report = session.reload();
    assert!(!report.applied);
    assert!(!report.diagnostics.is_empty());
    let cached = session.frame_png(2).expect("new look keeps serving");
    assert_eq!(cached.png, new_look);
}

#[test]
fn renamed_composition_refuses_keeping_the_last_good_look() {
    let root = tmp("renamed");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = PreviewSession::open(
        &folder,
        PreviewOptions {
            composition: Some("t".to_string()),
            ..PreviewOptions::default()
        },
    )
    .expect("named composition opens");
    let good = session.frame_png(1).expect("frame serves").png;

    // The edit renames the composition away: the named preview refuses
    // the reload naming what the project holds, keeping the old look.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"u\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let report = session.reload();
    assert!(!report.applied);
    assert!(report.message.contains("\"u\""), "{report}");
    assert!(report.message.contains("last good"), "{report}");
    let cached = session.frame_png(1).expect("old look keeps serving");
    assert_eq!(cached.png, good);
    let (index, bytes) = session.last_good().expect("last good kept");
    assert_eq!(index, 1);
    assert_eq!(bytes, good.as_slice());
}

#[test]
fn reload_never_writes_into_the_project_directory() {
    let root = tmp("purity");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let before = tree_bytes(&folder);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#ff0000\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let after_valid = tree_bytes(&folder);
    assert!(session.reload().applied);
    let _ = session.frame_png(0).expect("frame serves");
    write(&folder, "composition.kdl", "this is not kdl {{{{\n");
    assert!(!session.reload().applied);
    // Only the maker's own saves changed the tree: reloads add nothing
    // (no logs, no caches, no sidecars).
    let mut expected = after_valid.clone();
    expected.insert(
        "composition.kdl".to_string(),
        std::fs::read(folder.join("composition.kdl")).unwrap(),
    );
    assert_eq!(tree_bytes(&folder), expected);
    assert_eq!(before.len(), expected.len());
    assert!(!folder.join("logs").exists());
}
