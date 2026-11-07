//! Built-in player controls (Sprint 15, T006 / CAP-002, J-01,
//! STATE-03): every control acts immediately with the readout showing
//! current time and duration, speed persists across seeks, an empty
//! timeline disables transport with the reason shown while still
//! serving the background frame, and a silent project plays with no
//! error and no fake track — plus refusal paths and the
//! hyphenated-attribute, fail-closed-font, and never-write-into-the
//! project gates.

use preview::{
    player_page_html, player_page_html_with_background, ControlAction, Fps, LoadedFilm, Player,
    PlayerAttributes, PlayerPageModel, PlayerState, SPEED_MENU, SPEED_MENU_MAX, SPEED_MENU_MIN,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-controls-{tag}-{}-{}",
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
        font_try_order: vec!["Alpha, Beta".to_string()],
    }
}

fn silent_film() -> LoadedFilm {
    LoadedFilm {
        has_audio: false,
        ..film()
    }
}

fn empty_film() -> LoadedFilm {
    LoadedFilm {
        frame_count: 0,
        duration_secs: 0.0,
        has_audio: false,
        font_try_order: Vec::new(),
        ..film()
    }
}

fn ready_player() -> Player {
    let mut p = Player::begin(PlayerAttributes::default());
    p.load(Duration::from_millis(120), Ok(film()));
    assert_eq!(p.state(), PlayerState::Ready);
    p
}

fn ready_silent() -> Player {
    let mut p = Player::begin(PlayerAttributes::default());
    p.load(Duration::from_millis(120), Ok(silent_film()));
    assert_eq!(p.state(), PlayerState::Ready);
    p
}

fn ready_empty() -> Player {
    let mut p = Player::begin(PlayerAttributes::default());
    p.load(Duration::from_millis(120), Ok(empty_film()));
    assert_eq!(p.state(), PlayerState::Ready);
    p
}

fn model() -> PlayerPageModel {
    PlayerPageModel {
        project: "proj".to_string(),
        composition: "t".to_string(),
        frame: 0,
        frame_count: 10,
        duration_secs: 1.0,
        fps_value: 10.0,
        width: 320,
        height: 180,
        has_audio: true,
        attrs: PlayerAttributes::default(),
    }
}

#[test]
fn every_control_acts_immediately_with_fresh_readout() {
    let mut p = ready_player();
    // Play: the state moves and the readout reports playing at 0.000s.
    let r = p.apply_control(ControlAction::Play).expect("play acts");
    assert_eq!(p.state(), PlayerState::Playing);
    assert!(r.playing);
    assert_eq!((r.frame, r.frame_count), (0, 10));
    // Scrub by frame: the playhead moves and the readout follows.
    let r = p
        .apply_control(ControlAction::ScrubFrame(3))
        .expect("scrub acts");
    assert_eq!(p.position(), 3);
    assert_eq!(r.frame, 3);
    assert!((r.time_secs - 0.3).abs() < 1e-9);
    // Scrub by time: 0.5s at 10fps is frame 5.
    let r = p
        .apply_control(ControlAction::ScrubTime(0.5))
        .expect("time scrub acts");
    assert_eq!(p.position(), 5);
    assert!((r.time_secs - 0.5).abs() < 1e-9);
    // Rate, volume, mute: each acts and the readout carries it.
    let r = p
        .apply_control(ControlAction::Rate(2.0))
        .expect("rate acts");
    assert!((r.rate - 2.0).abs() < 1e-9);
    let r = p
        .apply_control(ControlAction::Volume(0.25))
        .expect("volume acts");
    assert!((r.volume - 0.25).abs() < 1e-9);
    let r = p.apply_control(ControlAction::Mute).expect("mute acts");
    assert!(r.muted);
    assert!(p.is_muted());
    let r = p.apply_control(ControlAction::Unmute).expect("unmute acts");
    assert!(!r.muted);
    // Duration rides every readout.
    assert!((r.duration_secs - 1.0).abs() < 1e-9);
}

#[test]
fn readout_shows_current_time_and_duration_through_every_control() {
    let mut p = ready_player();
    p.apply_control(ControlAction::ScrubFrame(3))
        .expect("scrub acts");
    let text = p.readout().to_string();
    assert!(text.contains("0.300s/1.000s"), "{text}");
    assert!(text.contains("frame 3/10"), "{text}");
    // After another act the readout moved with the gesture.
    p.apply_control(ControlAction::ScrubTime(0.5))
        .expect("time scrub acts");
    let text = p.readout().to_string();
    assert!(text.contains("0.500s/1.000s"), "{text}");
    assert!(text.contains("frame 5/10"), "{text}");
    // Rate changes keep reporting time and duration, not just speed.
    p.apply_control(ControlAction::Rate(0.5))
        .expect("rate acts");
    let text = p.readout().to_string();
    assert!(text.contains("0.500s/1.000s"), "{text}");
    assert!(text.contains("0.5x"), "{text}");
}

#[test]
fn speed_persists_across_seeks() {
    let mut p = ready_player();
    p.apply_control(ControlAction::Rate(2.0))
        .expect("rate acts");
    p.apply_control(ControlAction::ScrubFrame(1))
        .expect("seek acts");
    assert!((p.rate() - 2.0).abs() < 1e-9, "rate survives a frame seek");
    p.apply_control(ControlAction::ScrubTime(0.8))
        .expect("time seek acts");
    assert!((p.rate() - 2.0).abs() < 1e-9, "rate survives a time seek");
    assert!((p.readout().rate - 2.0).abs() < 1e-9);
    // The host method rate persists too: seeks never touch the rate.
    p.set_rate(4.0).expect("method rate acts");
    p.seek_to_frame(2).expect("seek acts");
    assert!((p.rate() - 4.0).abs() < 1e-9);
}

#[test]
fn speed_menu_spans_quarter_to_four() {
    assert_eq!(SPEED_MENU, [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0]);
    assert!((SPEED_MENU_MIN - 0.25).abs() < 1e-12);
    assert!((SPEED_MENU_MAX - 4.0).abs() < 1e-12);
    assert!(preview::is_menu_rate(1.0));
    assert!(preview::is_menu_rate(4.0));
    assert!(
        !preview::is_menu_rate(3.0),
        "3.0 is in range but not a menu stop"
    );
    // Both endpoints act through the controls.
    let mut p = ready_player();
    p.apply_control(ControlAction::Rate(0.25))
        .expect("menu floor acts");
    assert!((p.rate() - 0.25).abs() < 1e-9);
    p.apply_control(ControlAction::Rate(4.0))
        .expect("menu ceiling acts");
    assert!((p.rate() - 4.0).abs() < 1e-9);
    // Outside the menu range refuses naming the bound; the rate stays.
    for bad in [8.0, 0.1, 0.0, -1.0, f64::NAN, f64::INFINITY] {
        let events_before = p.events().len();
        let refused = p
            .apply_control(ControlAction::Rate(bad))
            .expect_err("out-of-menu rate");
        assert!(refused.message.contains("0.25x to 4x"), "{refused:?}");
        assert!(
            (p.rate() - 4.0).abs() < 1e-9,
            "refused rate keeps the level"
        );
        assert_eq!(p.events().len(), events_before, "refusals fire no events");
    }
}

#[test]
fn control_refusals_name_cause_and_move_nothing() {
    let mut p = ready_player();
    let events_before = p.events().len();
    // Scrub outside the timeline.
    let bad_frame = p
        .apply_control(ControlAction::ScrubFrame(10))
        .expect_err("frame 10 of 10");
    assert!(
        bad_frame.message.contains("outside the timeline"),
        "{bad_frame:?}"
    );
    let bad_time = p
        .apply_control(ControlAction::ScrubTime(2.0))
        .expect_err("past duration");
    assert!(bad_time.message.contains("0..=1.000s"), "{bad_time:?}");
    let bad_neg = p
        .apply_control(ControlAction::ScrubTime(-0.1))
        .expect_err("negative time");
    assert!(
        bad_neg.message.contains("outside the timeline"),
        "{bad_neg:?}"
    );
    // Volume outside 0..=1 keeps the level.
    let bad_vol = p
        .apply_control(ControlAction::Volume(1.5))
        .expect_err("volume above full");
    assert!(bad_vol.message.contains("0..=1"), "{bad_vol:?}");
    assert!((p.volume() - 1.0).abs() < 1e-9);
    assert_eq!(p.position(), 0, "refusals move nothing");
    assert_eq!(p.events().len(), events_before, "refusals fire no events");
    // A failed load refuses through the controls naming its cause.
    let mut bad = Player::begin(PlayerAttributes::default());
    bad.load(
        Duration::from_millis(120),
        Err("E_PARSE composition.kdl:1:1 broken".to_string()),
    );
    let refused = bad
        .apply_control(ControlAction::Play)
        .expect_err("failed load refuses");
    assert!(refused.message.contains("E_PARSE"), "{refused:?}");
    let refused = bad
        .apply_control(ControlAction::Rate(1.5))
        .expect_err("failed load refuses rate");
    assert!(refused.message.contains("E_PARSE"), "{refused:?}");
}

#[test]
fn empty_timeline_disables_transport_but_serves_background() {
    let mut p = ready_empty();
    // An empty timeline is loaded, not failed: the film is known.
    assert_eq!(p.state(), PlayerState::Ready);
    assert!(p.film().is_some());
    assert!(!p.controls_enabled(), "transport is disabled");
    let reason = p.disabled_reason().expect("reason shown");
    assert!(reason.contains("empty"), "{reason:?}");
    // The background frame is still served: the canvas is available.
    assert_eq!(p.background_frame(), Some((320, 180)));
    // Transport gestures refuse with the reason and move nothing.
    for action in [
        ControlAction::Play,
        ControlAction::ScrubFrame(0),
        ControlAction::ScrubTime(0.0),
        ControlAction::Rate(1.5),
        ControlAction::Volume(0.5),
    ] {
        let refused = p
            .apply_control(action)
            .expect_err("empty timeline refuses transport");
        assert!(
            refused.message.contains("empty"),
            "{action:?} refused: {refused:?}"
        );
    }
    assert_eq!(p.position(), 0);
    // Pause and mute are always safe no-ops, even on an empty timeline.
    p.apply_control(ControlAction::Pause)
        .expect("pause stays safe");
    p.apply_control(ControlAction::Mute)
        .expect("mute stays safe");
    assert!(p.is_muted());
    p.apply_control(ControlAction::Unmute)
        .expect("unmute stays safe");
}

#[test]
fn silent_project_plays_without_error_or_fake_track() {
    let mut p = ready_silent();
    assert!(!p.film().expect("film loads").has_audio);
    // Every control acts: a silent project plays with no error.
    p.apply_control(ControlAction::Play).expect("play acts");
    assert_eq!(p.state(), PlayerState::Playing);
    p.apply_control(ControlAction::ScrubFrame(6))
        .expect("scrub acts");
    assert_eq!(p.position(), 6);
    p.apply_control(ControlAction::Rate(0.5))
        .expect("rate acts");
    p.apply_control(ControlAction::Volume(0.0))
        .expect("silence volume acts");
    p.apply_control(ControlAction::Mute).expect("mute acts");
    p.apply_control(ControlAction::Unmute).expect("unmute acts");
    // No fake track: silence is the absence of a mix, reported in events.
    assert!(p.event_names().contains(&"assets-ready"));
    let assets = p
        .events()
        .iter()
        .find(|e| e.kind == preview::PlayerEventKind::AssetsReady)
        .expect("assets-ready fires");
    assert!(assets.detail.contains("silent"), "{assets:?}");
}

#[test]
fn font_try_order_passes_through_controls_fail_closed() {
    let mut p = ready_player();
    p.apply_control(ControlAction::ScrubFrame(2))
        .expect("scrub acts");
    p.apply_control(ControlAction::Rate(2.0))
        .expect("rate acts");
    // Comma try-order is preserved verbatim and never resolved: unknown
    // families stay fail-closed at render time.
    assert_eq!(
        p.film().expect("film").font_try_order,
        vec!["Alpha, Beta".to_string()]
    );
}

#[test]
fn page_carries_all_controls_with_live_readout() {
    let html = player_page_html(&model());
    // Every built-in control is on the page with a stable hook.
    for id in [
        "ctl-play",
        "ctl-scrub",
        "time-readout",
        "ctl-speed",
        "ctl-mute",
        "ctl-volume",
    ] {
        assert!(
            html.contains(&format!("id=\"{id}\"")),
            "control {id} on the page"
        );
    }
    // The speed menu offers the 0.25x to 4x stops with 1x selected.
    assert!(html.contains("value=\"0.25\""), "{html}");
    assert!(html.contains(">4x</option>"), "{html}");
    assert!(
        html.contains("<option value=\"1\" selected>1x</option>"),
        "{html}"
    );
    // The readout shows current time and duration from the start.
    assert!(html.contains("0.000s/1.000s frame 0/10"), "{html}");
    // Controls are enabled: no disabled attribute, reason stays hidden.
    assert!(
        !html.contains(" disabled>"),
        "no disabled attribute: {html}"
    );
    assert!(
        html.contains("<p id=\"controls-reason\" hidden></p>"),
        "{html}"
    );
    // The wiring refreshes the readout after every control gesture.
    for hook in [
        "wireControls",
        "refreshReadout",
        "syncControlsEnabled",
        "isMuted",
    ] {
        assert!(html.contains(hook), "wiring {hook} on the page");
    }
    // Scrub bounds track the timeline: max is the last frame index.
    assert!(html.contains("max=\"9\""), "{html}");
}

#[test]
fn page_styles_use_hyphenated_css_vocabulary() {
    let html = player_page_html(&model());
    for prop in [
        "background-color:",
        "border-radius:",
        "font-variant-numeric:",
    ] {
        assert!(html.contains(prop), "hyphenated {prop} on the page");
    }
    assert!(
        !html.contains("backgroundColor"),
        "no camelCase style names"
    );
    assert!(!html.contains("borderRadius"), "no camelCase style names");
}

#[test]
fn empty_page_disables_controls_shows_reason_and_background() {
    let html = player_page_html_with_background(
        &PlayerPageModel {
            frame_count: 0,
            duration_secs: 0.0,
            has_audio: false,
            ..model()
        },
        "#112233",
    );
    // Transport is disabled with the reason shown.
    let reason = "player disabled: the timeline is empty (0 frames)";
    assert!(html.contains(reason), "{html}");
    assert!(html.contains("<p id=\"controls-reason\">"), "{html}");
    assert!(
        !html.contains("<p id=\"controls-reason\" hidden>"),
        "{html}"
    );
    for id in [
        "ctl-play",
        "ctl-scrub",
        "ctl-speed",
        "ctl-mute",
        "ctl-volume",
    ] {
        assert!(
            html.contains(&format!("id=\"{id}\""),),
            "{id} still rendered"
        );
    }
    assert!(
        html.contains("<button id=\"ctl-play\" type=\"button\" disabled>"),
        "{html}"
    );
    // The background frame is still served in the composition color.
    assert!(html.contains("id=\"film-background\""), "{html}");
    assert!(!html.contains("id=\"film-background\" hidden"), "{html}");
    assert!(html.contains("background-color:#112233;"), "{html}");
    assert!(html.contains("aspect-ratio: 320 / 180"), "{html}");
    // Nothing to scrub: the scrubber parks at the only bound.
    assert!(html.contains("max=\"0\""), "{html}");
}

#[test]
fn controls_off_page_disables_with_reason() {
    let html = player_page_html(&PlayerPageModel {
        attrs: PlayerAttributes {
            controls: false,
            ..PlayerAttributes::default()
        },
        ..model()
    });
    assert!(html.contains("player disabled: controls are off"), "{html}");
    assert!(
        html.contains("<button id=\"ctl-play\" type=\"button\" disabled>"),
        "{html}"
    );
}

#[test]
fn silent_page_keeps_note_and_enabled_controls() {
    let html = player_page_html(&PlayerPageModel {
        has_audio: false,
        ..model()
    });
    assert!(html.contains("silent project"), "{html}");
    assert!(!html.contains("<audio"), "no fake track element");
    assert!(
        !html.contains(" disabled>"),
        "silent project keeps enabled controls: {html}"
    );
    assert!(html.contains("0.000s/1.000s frame 0/10"), "{html}");
}

#[test]
fn muted_attr_starts_mute_button_unmuting() {
    let html = player_page_html(&PlayerPageModel {
        attrs: PlayerAttributes {
            muted: true,
            ..PlayerAttributes::default()
        },
        ..model()
    });
    assert!(html.contains(">unmute</button>"), "{html}");
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

fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn sounding_project(folder: &Path) {
    use std::io::Write as _;
    let rate: u32 = 48000;
    // Minimal PCM WAV (48 kHz stereo s16) written by hand.
    let data_len = rate * 2 * 2;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..rate * 2 {
        let s =
            ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / rate as f64).sin() * 10000.0) as i16;
        wav.write_all(&s.to_le_bytes()).unwrap();
    }
    std::fs::write(folder.join("s.wav"), wav).unwrap();
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
            start.elapsed() < std::time::Duration::from_secs(20),
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
fn served_controls_page_wires_to_pipeline() {
    with_server(
        "controls",
        sounding_project,
        |_folder, _reg, _entry, port| {
            let (code, page) = body_str(port, "GET", "/player");
            assert_eq!(code, 200);
            // Every built-in control is served with the composition film.
            for id in [
                "ctl-play",
                "ctl-scrub",
                "time-readout",
                "ctl-speed",
                "ctl-mute",
                "ctl-volume",
            ] {
                assert!(
                    page.contains(&format!("id=\"{id}\"")),
                    "control {id} served"
                );
            }
            // The speed menu spans 0.25x to 4x and the readout is live.
            assert!(page.contains("value=\"0.25\""), "{page}");
            assert!(page.contains(">4x</option>"), "{page}");
            assert!(page.contains("time-readout"), "{page}");
            // The background box carries the composition background color.
            assert!(page.contains("background-color:#112233;"), "{page}");
            // Hyphenated CSS vocabulary, no camelCase style names.
            assert!(page.contains("border-radius:"), "{page}");
            assert!(!page.contains("backgroundColor"), "{page}");
            // Frames are pipeline bytes: repeats are identical PNGs.
            let (code_a, a) = http(port, "GET", "/api/frame?i=2");
            assert_eq!(code_a, 200);
            let (code_b, b) = http(port, "GET", "/api/frame?i=2");
            assert_eq!(code_b, 200);
            assert_eq!(a, b, "repeats serve identical bytes");
        },
    );
}

#[test]
fn silent_served_project_keeps_enabled_controls_without_fake_track() {
    with_server(
        "controls-silent",
        text_project,
        |_folder, _reg, _entry, port| {
            let (code, page) = body_str(port, "GET", "/player");
            assert_eq!(code, 200);
            assert!(page.contains("silent project"), "{page}");
            // Controls stay enabled on a silent project: no disabled hook.
            assert!(!page.contains(" disabled>"), "{page}");
            assert!(
                page.contains("<p id=\"controls-reason\" hidden></p>"),
                "{page}"
            );
            // Silent mix answers 204 with no fake track; frames still show.
            let (code, wav) = http(port, "GET", "/api/mix.wav");
            assert_eq!(code, 204, "silent mix answers 204 with no fake track");
            assert!(wav.is_empty());
            let (code, frame) = http(port, "GET", "/api/frame?i=0");
            assert_eq!(code, 200);
            assert_eq!(&frame[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        },
    );
}

#[test]
fn serving_controls_writes_nothing_into_project() {
    with_server(
        "controls-clean",
        sounding_project,
        |folder, _reg, _entry, port| {
            let before = tree_bytes(folder);
            for target in [
                "/player",
                "/player?autoplay=1&loop=1&muted=1",
                "/player?controls=0",
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
                "controls pages never write into the project"
            );
        },
    );
}
