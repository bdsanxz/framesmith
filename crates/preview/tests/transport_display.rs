//! Transport with display regressions (Sprint 14, T003 / CAP-003):
//! every acceptance criterion of `T003-transport-with-display.md`,
//! including refusal variants.

use preview::{
    Fps, PreviewOptions, PreviewSession, Transport, TransportControl, TransportErrorKind,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-preview-transport-{}-{}-{}",
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

fn ten_frames() -> Transport {
    Transport::new(10, Fps::from_int(10), 1.0)
}

#[test]
fn each_control_acts_on_the_timeline_immediately() {
    let mut t = ten_frames();
    assert_eq!((t.position(), t.is_playing()), (0, false));

    t.play().expect("play acts");
    assert!(t.is_playing());

    t.scrub_to(4).expect("scrub acts");
    assert_eq!(t.position(), 4);
    assert!(t.is_playing(), "scrub keeps the playing state");

    t.step_forward().expect("step forward acts");
    assert_eq!(t.position(), 5);
    t.step_back().expect("step back acts");
    assert_eq!(t.position(), 4);

    t.set_loop(true);
    assert!(t.loop_enabled());
    t.set_loop(false);
    assert!(!t.loop_enabled());

    t.set_speed(2.0).expect("speed acts");
    assert_eq!(t.speed(), 2.0);

    t.pause();
    assert!(!t.is_playing());
    assert_eq!(t.position(), 4, "pause holds the playhead");
}

#[test]
fn readout_shows_the_current_time_and_frame_number() {
    let mut t = ten_frames();
    t.scrub_to(3).expect("scrub acts");
    let readout = t.readout();
    assert_eq!(readout.frame, 3);
    assert_eq!(readout.frame_count, 10);
    assert!(
        (readout.time_secs - 0.3).abs() < 1e-9,
        "frame 3 at 10fps is 0.3s"
    );
    assert!((readout.duration_secs - 1.0).abs() < 1e-12);
    let text = readout.to_string();
    assert!(text.contains("3/10"), "{text}");
    assert!(text.contains("0.300"), "{text}");
    assert!(text.contains("1.000"), "{text}");

    // Frame 0 reads zero time; the last frame reads the tail.
    t.scrub_to(0).expect("scrub acts");
    assert_eq!(t.readout().time_secs, 0.0);
    t.scrub_to(9).expect("scrub acts");
    assert!((t.readout().time_secs - 0.9).abs() < 1e-9);
}

#[test]
fn loop_repeats_the_timeline_without_stopping() {
    let mut t = ten_frames();
    t.set_loop(true);
    t.scrub_to(8).expect("scrub acts");
    t.play().expect("play acts");
    // 0.5s at 10fps and 1x is 5 frames: 8 -> 9 -> 0 -> 1 -> 2 -> 3.
    t.tick(Duration::from_millis(500));
    assert_eq!(t.position(), 3);
    assert!(t.is_playing(), "looping never stops at the end");
    let text = t.readout().to_string();
    assert!(text.contains("playing"), "{text}");
    assert!(text.contains("loop on"), "{text}");
}

#[test]
fn without_loop_advance_parks_on_the_last_frame_and_stops() {
    let mut t = ten_frames();
    t.scrub_to(8).expect("scrub acts");
    t.play().expect("play acts");
    t.tick(Duration::from_millis(500));
    assert_eq!(t.position(), 9, "parks on the last frame");
    assert!(!t.is_playing(), "stops when the mix ends");
}

#[test]
fn speed_changes_the_advance_rate() {
    // 1x: 0.5s at 10fps advances 5 frames.
    let mut normal = Transport::new(100, Fps::from_int(10), 10.0);
    normal.play().expect("play acts");
    normal.tick(Duration::from_millis(500));
    assert_eq!(normal.position(), 5);

    // 2x covers twice the timeline in the same wall-clock time.
    let mut fast = Transport::new(100, Fps::from_int(10), 10.0);
    fast.set_speed(2.0).expect("speed acts");
    fast.play().expect("play acts");
    fast.tick(Duration::from_millis(500));
    assert_eq!(fast.position(), 10);

    // 0.5x covers half.
    let mut slow = Transport::new(100, Fps::from_int(10), 10.0);
    slow.set_speed(0.5).expect("speed acts");
    slow.play().expect("play acts");
    slow.tick(Duration::from_millis(500));
    assert_eq!(
        slow.position(),
        2,
        "0.5x over 0.5s is 2.5 frames -> frame 2"
    );

    // Sub-frame fractions accumulate instead of dropping: two 0.05s
    // ticks at 10fps are half a frame each, landing one frame on.
    let mut frac = Transport::new(100, Fps::from_int(10), 10.0);
    frac.play().expect("play acts");
    frac.tick(Duration::from_millis(50));
    assert_eq!(frac.position(), 0, "half a frame does not move yet");
    frac.tick(Duration::from_millis(50));
    assert_eq!(frac.position(), 1, "two halves make a frame");
}

#[test]
fn steps_clamp_at_the_ends_and_play_restarts_from_the_end() {
    let mut t = ten_frames();
    t.step_back().expect("step back at 0 clamps");
    assert_eq!(t.position(), 0);

    t.scrub_to(9).expect("scrub acts");
    t.step_forward().expect("step forward at the end clamps");
    assert_eq!(t.position(), 9);

    // Play at the last frame restarts from 0 so play always moves.
    t.play().expect("play acts");
    assert_eq!(t.position(), 0);
    assert!(t.is_playing());
}

#[test]
fn tick_is_a_noop_while_paused() {
    let mut t = ten_frames();
    t.scrub_to(4).expect("scrub acts");
    t.tick(Duration::from_secs(10));
    assert_eq!(t.position(), 4, "a paused playhead never advances");
}

#[test]
fn scrub_outside_the_window_refuses_naming_it() {
    let mut t = ten_frames();
    let err = t.scrub_to(10).expect_err("frame 10 of 10 refuses");
    assert_eq!(err.kind, TransportErrorKind::OutOfRange);
    assert_eq!(err.control, TransportControl::Scrub);
    assert!(err.message.contains("10"), "{}", err.message);
    assert!(err.message.contains("0..10"), "{}", err.message);
    assert_eq!(t.position(), 0, "a refused scrub moves nothing");
    assert_eq!(t.readout().frame, 0);
}

#[test]
fn unusable_speeds_refuse_and_keep_the_old_rate() {
    let mut t = ten_frames();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let err = t.set_speed(bad).expect_err("bad speed refuses");
        assert_eq!(err.kind, TransportErrorKind::BadSpeed, "{bad}");
        assert_eq!(err.control, TransportControl::SetSpeed);
        assert!(
            err.message.contains("finite rate above zero"),
            "{}",
            err.message
        );
    }
    assert_eq!(t.speed(), 1.0, "refused speeds keep the old rate");
    t.set_speed(0.25).expect("fractional speeds act");
    assert_eq!(t.speed(), 0.25);
}

#[test]
fn an_empty_timeline_disables_transport_visibly_with_the_reason() {
    let mut t = Transport::new(0, Fps::from_int(10), 0.0);
    assert!(!t.is_enabled());
    let reason = t.disabled_reason().expect("empty names its reason");
    assert!(reason.contains("empty"), "{reason}");

    // Every acting control refuses naming the reason; nothing moves.
    for err in [
        t.play().expect_err("play refuses"),
        t.scrub_to(0).expect_err("scrub refuses"),
        t.step_forward().expect_err("step forward refuses"),
        t.step_back().expect_err("step back refuses"),
        t.set_speed(2.0).expect_err("speed refuses"),
    ] {
        assert_eq!(err.kind, TransportErrorKind::Disabled, "{err}");
        assert!(err.message.contains("empty"), "{err}");
    }
    assert_eq!(t.position(), 0);

    // The safe controls never refuse: pause holds, loop toggles, and
    // the clock tick passes through without moving anything.
    t.pause();
    t.set_loop(true);
    t.play().expect_err("still disabled after safe controls");
    let mut ticking = Transport::new(0, Fps::from_int(10), 0.0);
    ticking.tick(Duration::from_secs(60));
    assert_eq!(ticking.position(), 0);

    // The readout shows the reason instead of a position.
    let readout = t.readout();
    assert!(!readout.enabled);
    let text = readout.to_string();
    assert!(text.contains("disabled"), "{text}");
    assert!(text.contains("empty"), "{text}");

    // A non-empty timeline is enabled with no reason to show.
    let live = ten_frames();
    assert!(live.is_enabled());
    assert_eq!(live.disabled_reason(), None);
    assert!(live.readout().enabled);
}

#[test]
fn session_transport_matches_the_served_timeline() {
    let root = tmp("session");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let session = PreviewSession::open(&folder, PreviewOptions::default()).expect("opens");
    let mut t = session.transport();
    assert_eq!(t.frame_count(), session.frame_count());
    assert_eq!(t.frame_count(), 10);
    assert_eq!(t.fps(), Fps::from_int(10));
    assert!((t.duration_secs() - 1.0).abs() < 1e-12);
    assert!(t.is_enabled(), "a served timeline enables transport");
    assert!(!t.is_playing());
    assert_eq!(t.position(), 0);

    // The transport position drives frame serving: the readout frame
    // is the frame the page requests.
    t.scrub_to(7).expect("scrub acts");
    assert_eq!(t.readout().frame, 7);
    let mut session = session;
    let frame = session.frame_png(t.position()).expect("frame serves");
    assert_eq!(frame.index, 7);
}

#[test]
fn readout_text_names_state_loop_and_speed() {
    let mut t = ten_frames();
    let paused = t.readout().to_string();
    assert!(paused.contains("paused"), "{paused}");
    assert!(paused.contains("loop off"), "{paused}");
    assert!(paused.contains("1x"), "{paused}");

    t.set_loop(true);
    t.set_speed(2.0).expect("speed acts");
    t.play().expect("play acts");
    let playing = t.readout().to_string();
    assert!(playing.contains("playing"), "{playing}");
    assert!(playing.contains("loop on"), "{playing}");
    assert!(playing.contains("2x"), "{playing}");
}
