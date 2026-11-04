//! Transport latency bars regressions (Sprint 14, T002 / CAP-002,
//! REC-004): every acceptance criterion of
//! `T002-meet-transport-latency-bars.md`, including refusal variants.

use preview::{
    CacheSource, FrameErrorKind, LatencyBars, LatencyTracker, PreviewOptions, PreviewSession,
    TransportAction, Warmth, JUMP_BAR_MS, SCRUB_BAR_MS, STEP_BAR_MS,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-preview-latency-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text_project(folder: &Path) {
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

fn open(folder: &Path) -> PreviewSession {
    PreviewSession::open(folder, PreviewOptions::default()).expect("valid project opens")
}

fn drive(session: &mut PreviewSession, action: TransportAction, seq: &[u64]) {
    for index in seq {
        session
            .frame_png_timed(action, *index)
            .expect("frame serves");
    }
}

#[test]
fn specification_bars_are_50_50_and_100_ms() {
    assert_eq!(STEP_BAR_MS, 50.0);
    assert_eq!(SCRUB_BAR_MS, 50.0);
    assert_eq!(JUMP_BAR_MS, 100.0);
    let bars = LatencyBars::default();
    assert_eq!(
        (bars.step_ms, bars.scrub_ms, bars.jump_ms),
        (50.0, 50.0, 100.0)
    );
}

#[test]
fn warm_step_scrub_and_jump_sequences_meet_their_bars() {
    let root = tmp("bars");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    // Step: one frame at a time; scrub: a drag back and forth; jump:
    // arbitrary seeks. Each action runs in its own session so its cold
    // pass draws fresh; each sequence runs twice, and the second pass
    // serves warm from the exact-frame cache.
    let sequences = [
        (TransportAction::Step, vec![0u64, 1, 2, 3, 4, 5]),
        (TransportAction::Scrub, vec![1u64, 2, 3, 4, 5, 6]),
        (TransportAction::Jump, vec![9u64, 0, 7, 2, 8, 1]),
    ];
    let mut reports = Vec::new();
    for (action, seq) in &sequences {
        let mut session = open(&folder);
        drive(&mut session, *action, seq);
        drive(&mut session, *action, seq);
        reports.push(session.latency_report());
    }
    assert_eq!(reports.len(), 3);
    for ((action, _), report) in sequences.iter().zip(&reports) {
        let row = report.action(*action).expect("one row per action");
        assert_eq!(row.warm_samples, 6, "{action} warm count");
        assert_eq!(row.cold_samples, 6, "{action} cold count");
        let warm = row.warm_median_ms.expect("warm median exists");
        assert!(
            warm <= action.bar_ms(&LatencyBars::default()),
            "{action} warm median {warm:.3}ms must sit at most the bar"
        );
        assert_eq!(row.meets_bar, Some(true), "{action} meets its bar");
        assert!(
            row.cold_median_ms.is_some(),
            "{action} cold median is labeled too"
        );
        assert_eq!(report.all_bars_met(), Some(true));
        let text = report.to_string();
        assert!(text.contains("warm"), "{text}");
        assert!(text.contains("cold"), "{text}");
        assert!(text.contains("all bars met"), "{text}");
    }
}

#[test]
fn disk_hits_file_warm_and_rendered_files_cold() {
    let root = tmp("warmth");
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
    // Cold draw, then evict from the one-slot memory cache so the
    // repeat comes from disk: both frame 1 and the disk repeat of
    // frame 0 must still file their samples.
    let (first, _) = session
        .frame_png_timed(TransportAction::Step, 0)
        .expect("frame 0 serves");
    assert_eq!(first.served_from, CacheSource::Rendered);
    session
        .frame_png_timed(TransportAction::Step, 1)
        .expect("frame 1 serves");
    let (again, _) = session
        .frame_png_timed(TransportAction::Step, 0)
        .expect("frame 0 repeats");
    assert_eq!(again.served_from, CacheSource::Disk);
    let samples = session.latency_samples();
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].warmth, Warmth::Cold);
    assert_eq!(samples[1].warmth, Warmth::Cold);
    assert_eq!(samples[2].warmth, Warmth::Warm);
    let report = session.latency_report();
    let row = report.action(TransportAction::Step).unwrap();
    assert_eq!(row.cold_samples, 2);
    assert_eq!(row.warm_samples, 1);
    // The verdict follows the warm bucket only.
    assert_eq!(row.meets_bar, Some(true));
}

#[test]
fn cold_samples_never_count_toward_the_bar() {
    let mut tracker = LatencyTracker::new();
    for _ in 0..3 {
        tracker.record(
            TransportAction::Step,
            Warmth::Cold,
            Duration::from_millis(500),
        );
    }
    for _ in 0..3 {
        tracker.record(TransportAction::Step, Warmth::Warm, Duration::from_millis(5));
    }
    let report = tracker.report();
    let row = report.action(TransportAction::Step).unwrap();
    assert_eq!(row.cold_median_ms, Some(500.0));
    assert_eq!(row.warm_median_ms, Some(5.0));
    // Slow cold draws are labeled, not judged: the bar verdict follows
    // the fast warm median.
    assert_eq!(row.meets_bar, Some(true));
    assert_eq!(report.all_bars_met(), Some(true));
    let text = report.to_string();
    assert!(text.contains("labeled cold, excluded from bar"), "{text}");
}

#[test]
fn missed_bar_reports_the_measured_median_against_the_bar() {
    let mut tracker = LatencyTracker::new();
    for _ in 0..5 {
        tracker.record(
            TransportAction::Step,
            Warmth::Warm,
            Duration::from_millis(120),
        );
    }
    let report = tracker.report();
    let row = report.action(TransportAction::Step).unwrap();
    assert_eq!(row.warm_median_ms, Some(120.0));
    assert_eq!(row.bar_ms, 50.0);
    assert_eq!(row.meets_bar, Some(false));
    assert_eq!(report.all_bars_met(), Some(false));
    // Honest miss: the report names the measured median against the
    // bar instead of substituting anything.
    let text = report.to_string();
    assert!(text.contains("MISS"), "{text}");
    assert!(text.contains("120"), "{text}");
    assert!(text.contains("50"), "{text}");
    assert!(text.contains("BAR MISSED"), "{text}");
    // The samples behind the miss are intact for review.
    assert_eq!(tracker.count(TransportAction::Step, Warmth::Warm), 5);
}

#[test]
fn jump_bar_is_100ms_while_step_and_scrub_are_50ms() {
    let mut tracker = LatencyTracker::new();
    for _ in 0..3 {
        tracker.record(TransportAction::Jump, Warmth::Warm, Duration::from_millis(80));
        tracker.record(TransportAction::Step, Warmth::Warm, Duration::from_millis(80));
    }
    let report = tracker.report();
    assert_eq!(
        report.action(TransportAction::Jump).unwrap().meets_bar,
        Some(true),
        "80ms warm jump median meets the 100ms jump bar"
    );
    assert_eq!(
        report.action(TransportAction::Step).unwrap().meets_bar,
        Some(false),
        "80ms warm step median misses the 50ms step bar"
    );
    assert_eq!(report.all_bars_met(), Some(false));
}

#[test]
fn empty_session_reports_no_claim_not_a_pass() {
    let root = tmp("empty");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let session = open(&folder);
    assert!(session.latency_samples().is_empty());
    let report = session.latency_report();
    for action in [
        TransportAction::Step,
        TransportAction::Scrub,
        TransportAction::Jump,
    ] {
        let row = report.action(action).unwrap();
        assert_eq!(row.warm_median_ms, None);
        assert_eq!(row.cold_median_ms, None);
        assert_eq!(row.meets_bar, None);
    }
    assert_eq!(report.all_bars_met(), None);
    let text = report.to_string();
    assert!(text.contains("no claim"), "{text}");
}

#[test]
fn refused_frames_file_no_latency_sample() {
    let root = tmp("refused");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    let err = session
        .frame_png_timed(TransportAction::Jump, 10)
        .expect_err("frame 10 is out of range");
    assert_eq!(err.kind, FrameErrorKind::OutOfRange);
    assert!(
        session.latency_samples().is_empty(),
        "refusals must not pollute the medians"
    );
    let report = session.latency_report();
    assert_eq!(
        report.action(TransportAction::Jump).unwrap().meets_bar,
        None
    );
    assert_eq!(report.all_bars_met(), None);
    // The session still serves afterwards, and only served frames file.
    session
        .frame_png_timed(TransportAction::Jump, 9)
        .expect("frame 9 serves");
    assert_eq!(session.latency_samples().len(), 1);
}
