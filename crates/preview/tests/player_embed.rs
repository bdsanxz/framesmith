//! Embed-and-play regressions (Sprint 15, T005 / CAP-001, J-01,
//! STATE-02..03, REC-005): every acceptance criterion — embedding loads
//! frames and mix, each method takes observable effect, events fire in
//! canonical order, loading ends in ready or a named failure within 8
//! seconds unless no-load-timeout is set, unloadable compositions report
//! the cause with no guessed frame, and no height shows the film at its
//! own shape — plus refusal paths and the hyphenated-attribute,
//! fail-closed-font, and never-write-into-the-project gates.

use preview::{
    embed_snippet, film_style, player_page_html, Fps, LoadFailureKind, LoadedFilm, Player,
    PlayerAttributes, PlayerPageModel, PlayerState, CANONICAL_ORDER, LOAD_DEADLINE_SECS,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-player-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn film() -> LoadedFilm {
    LoadedFilm {
        width: 320,
        height: 180,
        frame_count: 10,
        fps: Fps::from_int(10),
        duration_secs: 1.0,
        has_audio: true,
        font_try_order: vec!["Alpha".to_string(), "Beta".to_string()],
    }
}

fn ready_player() -> Player {
    let mut p = Player::begin(PlayerAttributes::default());
    assert_eq!(p.state(), PlayerState::Loading);
    p.load(Duration::from_millis(120), Ok(film()));
    assert_eq!(p.state(), PlayerState::Ready);
    p
}

fn fast() -> Duration {
    Duration::from_millis(120)
}

#[test]
fn attributes_parse_hyphenated_names_and_round_trip() {
    let attrs = PlayerAttributes::from_query(
        "autoplay=1&loop=on&muted=true&controls=0&click-to-play=off&no-load-timeout=1",
    );
    assert!(attrs.autoplay);
    assert!(attrs.loop_enabled);
    assert!(attrs.muted);
    assert!(!attrs.controls);
    assert!(!attrs.click_to_play);
    assert!(attrs.no_load_timeout);
    // Round-trip keeps the hyphenated wire names.
    let query = attrs.to_query();
    assert!(query.contains("click-to-play=0"), "{query}");
    assert!(query.contains("no-load-timeout=1"), "{query}");
    assert!(PlayerAttributes::from_query(&query) == attrs);
    // Unknown keys and junk values never enable a behavior.
    let odd = PlayerAttributes::from_query("autoplay=yes&bogus=1&muted=%zz");
    assert!(!odd.autoplay && !odd.muted);
}

#[test]
fn load_ready_reports_film_first_frame_and_controls() {
    let p = ready_player();
    let f = p.film().expect("film loads");
    assert_eq!((f.width, f.height), (320, 180));
    assert_eq!(p.frame_count(), 10);
    assert!((p.duration() - 1.0).abs() < 1e-9);
    assert!((p.time() - 0.0).abs() < 1e-9);
    assert!(p.controls_enabled());
    assert!(p.disabled_reason().is_none());
    assert!(p.failure().is_none());
    // First frame is painted on load: the film address is observable.
    assert_eq!(p.painted_frame(), Some(0));
}

#[test]
fn every_method_takes_observable_effect() {
    let mut p = ready_player();
    // play / pause.
    p.play().expect("play acts");
    assert_eq!(p.state(), PlayerState::Playing);
    p.pause();
    assert_eq!(p.state(), PlayerState::Paused);
    assert_eq!(p.position(), 0, "pausing keeps the frame");
    // seek by frame and by time (0.5s at 10fps is frame 5).
    p.seek_to_frame(3).expect("seek acts");
    assert_eq!(p.position(), 3);
    assert!((p.time() - 0.3).abs() < 1e-9);
    p.seek_to_time(0.5).expect("time seek acts");
    assert_eq!(p.position(), 5);
    // rate persists across seeks.
    p.set_rate(2.0).expect("rate acts");
    assert!((p.rate() - 2.0).abs() < 1e-9);
    p.seek_to_frame(1).expect("seek acts");
    assert!((p.rate() - 2.0).abs() < 1e-9, "rate persists across seeks");
    // volume and mute.
    p.set_volume(0.25).expect("volume acts");
    assert!((p.volume() - 0.25).abs() < 1e-9);
    p.mute();
    assert!(p.is_muted());
    assert_eq!(p.audible_volume(), 0.0);
    assert!((p.volume() - 0.25).abs() < 1e-9, "mute keeps the level");
    p.unmute();
    assert!(!p.is_muted());
    assert!((p.audible_volume() - 0.25).abs() < 1e-9);
    // duration reports the composition length.
    assert!((p.duration() - 1.0).abs() < 1e-9);
    // range playback seeks to the window start and plays.
    p.play_range(2, 4).expect("range acts");
    assert_eq!(p.range(), Some((2, 4)));
    assert_eq!(p.position(), 2);
    assert_eq!(p.state(), PlayerState::Playing);
    p.clear_range();
    assert_eq!(p.range(), None);
}

#[test]
fn events_fire_in_canonical_order() {
    let mut p = Player::begin(PlayerAttributes::default());
    p.load(fast(), Ok(film()));
    p.play().expect("play");
    p.seek_to_frame(4).expect("seek");
    p.tick(Duration::from_millis(100));
    let names = p.event_names();
    assert_eq!(
        names,
        vec![
            "ready",
            "state",
            "timeline",
            "assets-ready",
            "painted",
            "state",
            "playback",
            "timeline",
            "painted",
            "playback",
        ],
        "load then play then seek then tick: {names:?}"
    );
    // The load prefix matches the canonical order, and kinds appear
    // in canonical order overall.
    assert_eq!(&names[..CANONICAL_ORDER.len() - 1], &CANONICAL_ORDER[..5]);
    let mut first_seen: Vec<&str> = Vec::new();
    for name in &names {
        if !first_seen.contains(name) {
            first_seen.push(name);
        }
    }
    assert_eq!(first_seen, CANONICAL_ORDER);
}

#[test]
fn autoplay_starts_playing_with_events_in_order() {
    let mut p = Player::begin(PlayerAttributes {
        autoplay: true,
        ..PlayerAttributes::default()
    });
    p.load(fast(), Ok(film()));
    assert_eq!(p.state(), PlayerState::Playing);
    assert_eq!(
        p.event_names(),
        vec![
            "ready",
            "state",
            "timeline",
            "assets-ready",
            "painted",
            "state",
            "playback",
        ]
    );
}

#[test]
fn slow_load_fails_with_named_timeout() {
    let mut p = Player::begin(PlayerAttributes::default());
    p.load(Duration::from_secs(9), Ok(film()));
    assert_eq!(p.state(), PlayerState::Failed);
    let failure = p.failure().expect("named failure").clone();
    assert_eq!(failure.kind, LoadFailureKind::Timeout);
    assert_eq!(failure.kind.name(), "load-timeout");
    assert!(failure.message.contains("load-timeout"), "{failure:?}");
    assert!(
        failure.message.contains(&LOAD_DEADLINE_SECS.to_string()),
        "{failure:?}"
    );
    assert!(failure.message.contains("no-load-timeout"), "{failure:?}");
    assert!(p.film().is_none(), "no guessed film");
    assert_eq!(p.painted_frame(), None, "no guessed frame");
    assert!(!p.controls_enabled(), "controls stay disabled");
}

#[test]
fn no_load_timeout_disables_the_deadline() {
    let mut p = Player::begin(PlayerAttributes {
        no_load_timeout: true,
        ..PlayerAttributes::default()
    });
    p.load(Duration::from_secs(30), Ok(film()));
    assert_eq!(p.state(), PlayerState::Ready);
    assert!(p.controls_enabled());
    assert_eq!(p.painted_frame(), Some(0));
}

#[test]
fn broken_load_reports_cause_with_no_guessed_frame() {
    let mut p = Player::begin(PlayerAttributes::default());
    p.load(
        fast(),
        Err("E_PARSE composition.kdl:1:1 broken".to_string()),
    );
    assert_eq!(p.state(), PlayerState::Failed);
    let failure = p.failure().expect("named failure").clone();
    assert_eq!(failure.kind, LoadFailureKind::Unloadable);
    assert_eq!(failure.kind.name(), "load-failed");
    assert!(failure.message.contains("E_PARSE"), "{failure:?}");
    assert!(
        failure.message.contains("no frame was guessed"),
        "{failure:?}"
    );
    assert!(p.film().is_none());
    assert_eq!(p.painted_frame(), None, "shows no guessed frame");
    assert!(!p.controls_enabled(), "controls stay disabled");
    assert!(p.disabled_reason().is_some());
    // Methods refuse against the named cause and move nothing.
    let refused = p.play().expect_err("play must refuse");
    assert!(refused.message.contains("E_PARSE"), "{refused:?}");
    assert_eq!(p.position(), 0);
    assert_eq!(p.event_names(), vec!["ready", "state"]);
}

#[test]
fn method_refusals_name_cause_and_move_nothing() {
    let mut p = ready_player();
    let events_before = p.events().len();
    let seek = p
        .seek_to_frame(10)
        .expect_err("frame 10 of 10 is out of range");
    assert!(seek.message.contains("outside the timeline"), "{seek:?}");
    assert!(seek.message.contains("0..10"), "{seek:?}");
    let time = p.seek_to_time(2.0).expect_err("past the duration");
    assert!(time.message.contains("0..=1.000s"), "{time:?}");
    let rate = p.set_rate(0.0).expect_err("zero rate");
    assert!(rate.message.contains("above zero"), "{rate:?}");
    assert!(rate.message.contains("stays 1x"), "{rate:?}");
    let volume = p.set_volume(1.5).expect_err("volume above full");
    assert!(volume.message.contains("0..=1"), "{volume:?}");
    assert!(volume.message.contains("stays 1"), "{volume:?}");
    let inverted = p.play_range(5, 2).expect_err("inverted window");
    assert!(
        inverted.message.contains("starts past its end"),
        "{inverted:?}"
    );
    let wide = p.play_range(0, 10).expect_err("window leaves the timeline");
    assert!(wide.message.contains("0..10"), "{wide:?}");
    assert_eq!(p.position(), 0, "refusals move nothing");
    assert_eq!(p.range(), None, "refused range leaves no receipt");
    assert_eq!(p.events().len(), events_before, "refusals fire no events");
}

#[test]
fn empty_timeline_disables_with_reason_while_serving_state() {
    let mut p = Player::begin(PlayerAttributes::default());
    p.load(
        fast(),
        Ok(LoadedFilm {
            width: 320,
            height: 180,
            frame_count: 0,
            fps: Fps::from_int(10),
            duration_secs: 0.0,
            has_audio: false,
            font_try_order: Vec::new(),
        }),
    );
    assert_eq!(p.state(), PlayerState::Ready);
    assert!(!p.controls_enabled());
    let reason = p.disabled_reason().expect("reason shown");
    assert!(reason.contains("empty"), "{reason:?}");
    let refused = p.play().expect_err("empty timeline refuses");
    assert!(refused.message.contains("empty"), "{refused:?}");
    // No frame exists, so nothing is ever painted — but the state still
    // reports instead of guessing.
    assert_eq!(p.painted_frame(), None);
    assert_eq!(
        p.event_names(),
        vec!["ready", "state", "timeline", "assets-ready"]
    );
}

#[test]
fn controls_toggleable_at_any_time_and_failed_stays_disabled() {
    let mut p = ready_player();
    p.play().expect("play");
    p.set_controls(false);
    assert!(!p.controls_on());
    assert!(!p.controls_enabled());
    assert!(p
        .disabled_reason()
        .expect("reason")
        .contains("controls are off"));
    // Toggling works mid-playback without stopping it.
    p.set_controls(true);
    assert!(p.controls_enabled());
    assert_eq!(p.state(), PlayerState::Playing);
    // On a failed load the toggle records but controls stay disabled.
    let mut bad = Player::begin(PlayerAttributes::default());
    bad.load(fast(), Err("broken".to_string()));
    bad.set_controls(true);
    assert!(bad.controls_on());
    assert!(!bad.controls_enabled(), "failed loads stay disabled");
}

#[test]
fn click_to_play_disable() {
    let mut p = ready_player();
    assert!(p.click_film(), "click plays when enabled");
    assert_eq!(p.state(), PlayerState::Playing);
    assert!(p.click_film(), "click pauses when enabled");
    assert_eq!(p.state(), PlayerState::Paused);
    p.set_click_to_play(false);
    assert!(!p.click_film(), "disabled clicks never act");
    assert_eq!(p.state(), PlayerState::Paused);
    // A failed load never acts on click either.
    let mut bad = Player::begin(PlayerAttributes::default());
    bad.load(fast(), Err("broken".to_string()));
    assert!(!bad.click_film());
}

#[test]
fn range_playback_loops_its_window() {
    let mut p = ready_player();
    p.play_range(2, 3).expect("range");
    // Two frames at 10fps need 200ms to lap the 2-frame window.
    p.tick(Duration::from_millis(200));
    assert_eq!(p.position(), 2, "range loops inside its window");
    assert_eq!(p.state(), PlayerState::Playing, "range keeps playing");
    // Without a range the end parks and pauses.
    p.clear_range();
    p.seek_to_frame(8).expect("seek");
    p.play().expect("play");
    p.tick(Duration::from_secs(1));
    assert_eq!(p.position(), 9);
    assert_eq!(p.state(), PlayerState::Paused, "the end parks and pauses");
}

#[test]
fn pause_keeps_frame_and_event_order_truthful() {
    let mut p = ready_player();
    p.play().expect("play");
    p.tick(Duration::from_millis(200));
    assert_eq!(p.position(), 2);
    p.pause();
    assert_eq!(p.position(), 2, "pausing keeps the frame");
    p.tick(Duration::from_secs(1));
    assert_eq!(p.position(), 2, "paused ticks never advance");
    let names = p.event_names();
    assert_eq!(names.iter().filter(|n| **n == "playback").count(), 2);
    assert_eq!(names.last(), Some(&"state"));
}

#[test]
fn font_try_order_preserved_fail_closed() {
    let p = ready_player();
    let f = p.film().expect("film loads");
    assert_eq!(
        f.font_try_order,
        vec!["Alpha".to_string(), "Beta".to_string()]
    );
}

#[test]
fn no_height_shows_film_at_own_shape() {
    let style = film_style(320, 180, None);
    assert!(style.contains("aspect-ratio: 320 / 180"), "{style}");
    assert!(!style.contains("height: 180"), "{style}");
    let fixed = film_style(320, 180, Some(240));
    assert!(fixed.contains("height: 240px"), "{fixed}");
    assert!(!fixed.contains("aspect-ratio"), "{fixed}");
}

#[test]
fn embed_snippet_uses_hyphenated_attributes_and_film_shape() {
    let snippet = embed_snippet(
        "http://127.0.0.1:8130/player",
        "t",
        &PlayerAttributes {
            autoplay: true,
            click_to_play: false,
            no_load_timeout: true,
            ..PlayerAttributes::default()
        },
        320,
        180,
        None,
    );
    assert!(snippet.contains("<iframe"), "{snippet}");
    assert!(snippet.contains("/player?composition=t"), "{snippet}");
    assert!(snippet.contains("autoplay=1"), "{snippet}");
    assert!(snippet.contains("click-to-play=0"), "{snippet}");
    assert!(snippet.contains("no-load-timeout=1"), "{snippet}");
    assert!(snippet.contains("aspect-ratio: 320 / 180"), "{snippet}");
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// Minimal PCM WAV (48 kHz stereo s16) written by hand so sounding
/// player tests need no encoder.
fn write_wav(dir: &Path, name: &str, secs: f64) {
    let rate: u32 = 48000;
    let frames = (secs * rate as f64).round() as u32;
    let data_len = frames * 2 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames * 2 {
        let s =
            ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / rate as f64).sin() * 10000.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(dir.join(name), out).unwrap();
}

fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn sounding_project(folder: &Path) {
    write_wav(folder, "s.wav", 1.0);
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n    audio path=\"s.wav\" start=0 duration=1\n}\n",
    );
}

/// Minimal HTTP exchange over loopback: returns (status code, body).
fn http(port: u16, method: &str, target: &str) -> (u16, Vec<u8>) {
    use std::io::{Read, Write};
    let addr = format!("127.0.0.1:{port}");
    let mut stream = std::net::TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        std::time::Duration::from_secs(5),
    )
    .expect("connect");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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

fn body_str(port: u16, method: &str, target: &str) -> (u16, String) {
    let (code, body) = http(port, method, target);
    (code, String::from_utf8_lossy(&body).into_owned())
}

/// Run one real server for this test: `setup` writes the project,
/// `f` drives it, then the server shuts down and the thread joins.
fn with_server(
    tag: &str,
    setup: impl FnOnce(&Path),
    f: impl FnOnce(&Path, &Path, preview::daemon::ServerEntry, u16),
) {
    use preview::daemon;
    use preview::server::{run_serve, ServeOptions};
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
    let start = std::time::Instant::now();
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
            start.elapsed() < std::time::Duration::from_secs(60),
            "server never became ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    f(&folder, &reg, entry.clone(), entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(entry.port), "port is quiet after shutdown");
}

#[test]
fn player_page_loads_frames_and_mix() {
    with_server(
        "page-mix",
        sounding_project,
        |_folder, _reg, _entry, port| {
            let (code, page) = body_str(port, "GET", "/player");
            assert_eq!(code, 200);
            // Embedding with a source loads the composition frames and mix.
            assert!(page.contains("— t</"), "{page}");
            assert!(page.contains("/api/frame?i="), "{page}");
            assert!(page.contains("/api/mix.wav"), "{page}");
            assert!(page.contains("framesmithPlayer"), "{page}");
            // Methods and ordered events are on the page.
            for method in [
                "play",
                "pause",
                "seek",
                "setRate",
                "setVolume",
                "setMuted",
                "setRange",
            ] {
                assert!(page.contains(method), "method {method} on the page");
            }
            for event in CANONICAL_ORDER {
                assert!(
                    page.contains(&format!("'{event}'")),
                    "event {event} on the page"
                );
            }
            // The 8-second deadline with the no-load-timeout escape.
            assert!(page.contains("8000"), "{page}");
            assert!(page.contains("noLoadTimeout"), "{page}");
            assert!(page.contains("load-timeout"), "{page}");
            assert!(page.contains("load-failed"), "{page}");
            // The mix serves real bytes: a sounding project is no 204.
            let (code, wav) = http(port, "GET", "/api/mix.wav");
            assert_eq!(code, 200);
            assert!(!wav.is_empty());
            // Player frames are pipeline bytes: repeats are identical PNGs.
            let (code_a, a) = http(port, "GET", "/api/frame?i=2");
            assert_eq!(code_a, 200);
            let (code_b, b) = http(port, "GET", "/api/frame?i=2");
            assert_eq!(code_b, 200);
            assert_eq!(a, b, "repeats serve identical bytes");
            assert_eq!(
                &a[..8],
                &[137, 80, 78, 71, 13, 10, 26, 10],
                "exact PNG bytes"
            );
        },
    );
}

#[test]
fn silent_player_page_serves_background_without_fake_track() {
    with_server(
        "page-silent",
        text_project,
        |_folder, _reg, _entry, port| {
            let (code, page) = body_str(port, "GET", "/player");
            assert_eq!(code, 200);
            assert!(page.contains("silent project"), "{page}");
            let (code, wav) = http(port, "GET", "/api/mix.wav");
            assert_eq!(code, 204, "silent mix answers 204 with no fake track");
            assert!(wav.is_empty());
            // Frames still load: the background film shows.
            let (code, frame) = http(port, "GET", "/api/frame?i=0");
            assert_eq!(code, 200);
            assert_eq!(&frame[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        },
    );
}

#[test]
fn embed_route_serves_snippet_with_hyphenated_attributes_and_film_shape() {
    with_server("embed", text_project, |_folder, _reg, _entry, port| {
        let (code, snippet) = body_str(
            port,
            "GET",
            "/embed?autoplay=1&click-to-play=0&no-load-timeout=1",
        );
        assert_eq!(code, 200, "{snippet}");
        assert!(snippet.contains("<iframe"), "{snippet}");
        assert!(snippet.contains("/player?composition=t"), "{snippet}");
        assert!(snippet.contains("autoplay=1"), "{snippet}");
        assert!(snippet.contains("click-to-play=0"), "{snippet}");
        assert!(snippet.contains("no-load-timeout=1"), "{snippet}");
        // No height given: the film shows at its own shape.
        assert!(snippet.contains("aspect-ratio: 320 / 180"), "{snippet}");
    });
}

#[test]
fn player_serving_writes_nothing_into_project() {
    with_server(
        "page-clean",
        sounding_project,
        |folder, _reg, _entry, port| {
            let before = tree_bytes(folder);
            for target in [
                "/player",
                "/player?autoplay=1&loop=1&muted=1",
                "/embed",
                "/api/status",
                "/api/frame?i=3",
                "/api/mix.wav",
            ] {
                let (code, _) = http(port, "GET", target);
                assert_eq!(code, 200, "{target}");
            }
            assert_eq!(
                tree_bytes(folder),
                before,
                "player caches never land in the project"
            );
        },
    );
}

#[test]
fn player_page_model_renders_film_shape_without_height() {
    let html = player_page_html(&PlayerPageModel {
        project: "proj".to_string(),
        composition: "t".to_string(),
        frame: 0,
        frame_count: 10,
        duration_secs: 1.0,
        fps_value: 10.0,
        width: 320,
        height: 180,
        has_audio: false,
        attrs: PlayerAttributes::default(),
    });
    assert!(html.contains("aspect-ratio: 320 / 180"), "{html}");
    assert!(html.contains("silent project"), "{html}");
}
