//! Snap and zoom regressions (Sprint 16, T005 / CAP-005, J-02,
//! STATE-03, INT-001): drags snap to the playhead, clip edges, beats,
//! and the frame grid within the visible threshold with a guide line;
//! the threshold shows and changes only through the visible project
//! setting; with snapping off drags move freely and the status line
//! says so; zoom fits and goes manual from 10 percent to frame level
//! anchored at the cursor or playhead, with zoom to selection;
//! out-of-bounds zoom clamps showing the clamped value. Snap and zoom
//! are view state: they stage no patch and record no turn, while the
//! snapped drag itself lands as a named history turn through the SDK
//! patch and history machinery.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-snap-zoom-{}-{}-{}",
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

/// Two text clips: `title` (0..2s) and `bye` (2..4s), 10 fps,
/// 40 frames, 4 film seconds.
fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"bye\" value=\"Bye\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Minimal HTTP exchange over loopback with an optional body.
/// Returns (status code, body).
fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let payload = body.unwrap_or("");
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
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

/// Run one real server for this test: `setup` writes the project,
/// `f` drives it, then the server shuts down and the thread joins.
fn with_server(
    tag: &str,
    setup: impl FnOnce(&Path),
    f: impl FnOnce(&Path, &Path, daemon::ServerEntry, u16),
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
            start.elapsed() < Duration::from_secs(60),
            "server never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    f(&folder, &reg, entry.clone(), entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(entry.port), "port is quiet after shutdown");
}

fn delta_bytes(folder: &Path) -> Option<Vec<u8>> {
    std::fs::read(folder.join(".framesmith").join("sdk-overrides.json")).ok()
}

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .map(|l| l.turns.len())
        .unwrap_or(usize::MAX)
}

fn snap_file(folder: &Path) -> Option<Vec<u8>> {
    std::fs::read(folder.join(".framesmith").join("studio-snap.json")).ok()
}

fn zoom_file(folder: &Path) -> Option<Vec<u8>> {
    std::fs::read(folder.join(".framesmith").join("studio-zoom.json")).ok()
}

#[test]
fn studio_snap_resolves_every_target_with_a_guide_line() {
    with_server(
        "snap-targets",
        text_project,
        |_folder, _reg, _entry, port| {
            // The playhead parks at frame 0 (0.0s): a drag 0.05s out snaps
            // back to it within 8 px at 100 px/s (0.08s of reach).
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve",
                Some("{\"want\":0.05,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"did_snap\":true"), "{out}");
            assert!(out.contains("playhead"), "{out}");
            assert!(out.contains("\"snapped\":0.000"), "{out}");
            assert!(out.contains("\"guide_at\":0.000"), "{out}");
            assert!(out.contains("guide shown"), "{out}");
            // A drag 0.05s past the shared 2.0s edge ties the grid line at
            // 2.0s (frame 20) on distance; the clip edge wins the tie by
            // priority and the guide paints the edge.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve",
                Some("{\"want\":2.05,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"did_snap\":true"), "{out}");
            assert!(out.contains("clip edge"), "{out}");
            assert!(out.contains("\"snapped\":2.000"), "{out}");
            assert!(out.contains("\"guide_at\":2.000"), "{out}");
            // A beat at 3.0s ties the grid line at 3.0s on distance; the
            // beat wins the tie and the guide paints the beat.
            let (code, beat) = body_str(
                port,
                "POST",
                "/api/studio/beats",
                Some("{\"action\":\"add\",\"name\":\"down\",\"at\":3.0}"),
            );
            assert_eq!(code, 200, "{beat}");
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve",
                Some("{\"want\":3.04,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"did_snap\":true"), "{out}");
            assert!(out.contains("beat down"), "{out}");
            assert!(out.contains("\"snapped\":3.000"), "{out}");
            assert!(out.contains("\"guide_at\":3.000"), "{out}");
            // Mid-clip with no edge, beat, or playhead near, the frame
            // grid still catches the drag: frame 15 at 1.5s.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve",
                Some("{\"want\":1.53,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"did_snap\":true"), "{out}");
            assert!(out.contains("grid frame 15"), "{out}");
            assert!(out.contains("\"snapped\":1.500"), "{out}");
            assert!(out.contains("\"guide_at\":1.500"), "{out}");
        },
    );
}

#[test]
fn studio_snap_misses_outside_the_threshold() {
    with_server("snap-miss", text_project, |_folder, _reg, _entry, port| {
        // At 200 px/s the 8 px threshold reaches 0.04s: a drag 0.05s
        // off the 1.2s/1.3s grid lines moves freely with no guide.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/snap-resolve",
            Some("{\"want\":1.25,\"px-per-sec\":200}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"did_snap\":false"), "{out}");
        assert!(out.contains("\"snapped\":1.250"), "{out}");
        assert!(out.contains("\"guide_at\":null"), "{out}");
        assert!(out.contains("moves freely"), "{out}");
        // No target anywhere near: at 200 px/s the 8 px reach is
        // 0.04s, under the 0.05s grid half-step, so a mid-cell drag
        // moves freely with no guide even with the playhead parked
        // far away.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/snap-resolve",
            Some("{\"want\":0.55,\"playhead\":9.0,\"px-per-sec\":200}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"did_snap\":false"), "{out}");
        assert!(out.contains("\"guide_at\":null"), "{out}");
    });
}

#[test]
fn studio_snap_threshold_shows_and_changes_only_through_the_setting() {
    with_server(
        "snap-setting",
        text_project,
        |folder, _reg, _entry, port| {
            // The default setting shows: on, within 8 px.
            let (code, out) = body_str(port, "GET", "/api/studio/snap", None);
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"enabled\":true"), "{out}");
            assert!(out.contains("\"threshold-px\":8"), "{out}");
            assert!(out.contains("\"restored\":false"), "{out}");
            assert!(out.contains("8 px"), "{out}");
            // The visible setting narrows the magnet to 4 px: the
            // 0.05s-off edge drag that snapped at 8 px now misses.
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/snap",
                Some("{\"threshold-px\":4}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"threshold-px\":4"), "{out}");
            assert!(out.contains("4 px"), "{out}");
            let (code, get) = body_str(port, "GET", "/api/studio/snap", None);
            assert_eq!(code, 200, "{get}");
            assert!(get.contains("\"threshold-px\":4"), "{get}");
            assert!(get.contains("\"restored\":true"), "{get}");
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve",
                Some("{\"want\":2.05,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"did_snap\":false"), "{out}");
            // Refused bodies save nothing: the file keeps the 4 px
            // setting byte-identical.
            let before = snap_file(folder).expect("setting saved");
            for (label, bad) in [
                ("empty", ""),
                ("garbage", "{ nope"),
                ("negative", "{\"threshold-px\":-1}"),
                ("zero", "{\"threshold-px\":0}"),
                ("too-wide", "{\"threshold-px\":65}"),
                ("not-a-number", "{\"threshold-px\":\"wide\"}"),
                ("bad-switch", "{\"enabled\":\"yes\"}"),
                ("no-keys", "{\"anchor\":\"cursor\"}"),
            ] {
                let (code, refused) = body_str(port, "POST", "/api/studio/snap", Some(bad));
                assert_eq!(code, 400, "{label}: {refused}");
                assert!(refused.contains("studio refused"), "{label}: {refused}");
            }
            assert_eq!(snap_file(folder), Some(before), "refusals save nothing");
            // A corrupt snap file falls back to 8 px and says so.
            let path = folder.join(".framesmith").join("studio-snap.json");
            std::fs::write(&path, "{ not a snap setting").unwrap();
            let (code, out) = body_str(port, "GET", "/api/studio/snap", None);
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"threshold-px\":8"), "{out}");
            assert!(out.contains("snapping within 8"), "{out}");
            assert!(out.contains("\"restored\":false"), "{out}");
        },
    );
}

#[test]
fn studio_snap_off_moves_freely_and_says_so() {
    with_server("snap-off", text_project, |_folder, _reg, _entry, port| {
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/snap",
            Some("{\"enabled\":false}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"enabled\":false"), "{out}");
        assert!(out.contains("snapping is off"), "{out}");
        // Even a drag dead on the 2.0s edge moves freely now.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/snap-resolve",
            Some("{\"want\":2.0,\"px-per-sec\":100}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"did_snap\":false"), "{out}");
        assert!(out.contains("\"snapped\":2.000"), "{out}");
        assert!(out.contains("\"guide_at\":null"), "{out}");
        assert!(out.contains("snapping is off: drags move freely."), "{out}");
        // The project setting (not a silent switch) reports the same.
        let (code, out) = body_str(port, "GET", "/api/studio/snap", None);
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("snapping is off: drags move freely."), "{out}");
    });
}

#[test]
fn studio_zoom_fits_goes_manual_and_frames_a_selection() {
    with_server("zoom-modes", text_project, |_folder, _reg, _entry, port| {
        // Fit: the 4s timeline fills 800 px at 200 px/s, 100 percent.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/zoom",
            Some("{\"mode\":\"fit\",\"viewport-px\":800}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"px-per-sec\":200.000"), "{out}");
        assert!(out.contains("\"percent\":100.000"), "{out}");
        assert!(out.contains("\"clamped\":false"), "{out}");
        assert!(out.contains("zoom fit"), "{out}");
        // Manual 150 percent anchored at the cursor holds 2.5s at
        // 750 content px.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/zoom",
            Some("{\"mode\":\"manual\",\"percent\":150,\"anchor\":\"cursor\",\"anchor-at\":2.5,\"viewport-px\":800}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"px-per-sec\":300.000"), "{out}");
        assert!(out.contains("\"percent\":150.000"), "{out}");
        assert!(out.contains("\"clamped\":false"), "{out}");
        assert!(out.contains("\"anchor\":\"cursor\""), "{out}");
        assert!(out.contains("\"anchor-at\":2.500"), "{out}");
        assert!(out.contains("\"anchor-px\":750.000"), "{out}");
        // The playhead anchor is the default: no `anchor` key still
        // anchors at the playhead (frame 0, 0.0s here).
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/zoom",
            Some("{\"mode\":\"manual\",\"percent\":50,\"viewport-px\":800}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"px-per-sec\":100.000"), "{out}");
        assert!(out.contains("\"anchor\":\"playhead\""), "{out}");
        assert!(out.contains("\"anchor-at\":0.000"), "{out}");
        // Zoom to selection frames `title` (0..2s): 800 px over 2s.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/zoom",
            Some("{\"mode\":\"selection\",\"selection\":[\"title\"],\"viewport-px\":800}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"px-per-sec\":400.000"), "{out}");
        assert!(out.contains("\"span\":[0.000,2.000]"), "{out}");
        assert!(out.contains("selection"), "{out}");
        assert!(out.contains("\"clamped\":false"), "{out}");
        // The preference reads back what the panel last applied.
        let (code, out) = body_str(port, "GET", "/api/studio/zoom", None);
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"mode\":\"selection\""), "{out}");
        assert!(out.contains("\"restored\":true"), "{out}");
        assert!(out.contains("selection"), "{out}");
    });
}

#[test]
fn studio_zoom_clamps_out_of_bounds_and_shows_the_value() {
    with_server("zoom-clamp", text_project, |folder, _reg, _entry, port| {
        // Below the 10 percent floor: clamps to 10 and says so.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/zoom",
            Some("{\"mode\":\"manual\",\"percent\":5,\"viewport-px\":800}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"clamped\":true"), "{out}");
        assert!(out.contains("\"percent\":10.000"), "{out}");
        assert!(out.contains("clamped"), "{out}");
        assert!(out.contains("10"), "{out}");
        let (code, get) = body_str(port, "GET", "/api/studio/zoom", None);
        assert_eq!(code, 200, "{get}");
        assert!(get.contains("\"percent\":10"), "{get}");
        // Past frame level (10 fps * 64 px = 640 px/s, 320 percent of
        // the 200 px/s fit): clamps to frame level and says so.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/zoom",
            Some("{\"mode\":\"manual\",\"percent\":1000,\"viewport-px\":800}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"clamped\":true"), "{out}");
        assert!(out.contains("\"px-per-sec\":640.000"), "{out}");
        assert!(out.contains("\"percent\":320.000"), "{out}");
        assert!(out.contains("frame level"), "{out}");
        // A selection in a huge viewport clamps the same way:
        // framing `title` (0..2s) in 4000 px wants 2000 px/s, past
        // frame level, so it clamps to 640 px/s (64 percent of the
        // 1000 px/s fit) and shows the clamped value.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/zoom",
            Some("{\"mode\":\"selection\",\"selection\":[\"title\"],\"viewport-px\":4000}"),
        );
        assert_eq!(code, 200, "{out}");
        assert!(out.contains("\"clamped\":true"), "{out}");
        assert!(out.contains("\"px-per-sec\":640.000"), "{out}");
        assert!(out.contains("\"percent\":64.000"), "{out}");
        assert!(out.contains("frame level"), "{out}");
        assert!(out.contains("clamped to 64%"), "{out}");
        // The clamped value is what the project keeps.
        let raw = zoom_file(folder).expect("zoom saved");
        let text = String::from_utf8_lossy(&raw).into_owned();
        assert!(text.contains("\"percent\": 64"), "{text}");
    });
}

#[test]
fn studio_snap_and_zoom_refusals_stage_nothing() {
    with_server(
        "snap-zoom-no",
        text_project,
        |folder, _reg, _entry, port| {
            let before_delta = delta_bytes(folder);
            let before_turns = turn_count(folder);
            // Snap-resolve refusals name the cause.
            for (label, bad) in [
                ("empty", ""),
                ("garbage", "{ nope"),
                ("no-want", "{\"px-per-sec\":100}"),
                ("no-scale", "{\"want\":1.0}"),
                ("zero-scale", "{\"want\":1.0,\"px-per-sec\":0}"),
                (
                    "bad-playhead",
                    "{\"want\":1.0,\"px-per-sec\":100,\"playhead\":\"soon\"}",
                ),
            ] {
                let (code, refused) = body_str(port, "POST", "/api/studio/snap-resolve", Some(bad));
                assert_eq!(code, 400, "{label}: {refused}");
                assert!(refused.contains("studio refused"), "{label}: {refused}");
            }
            // Unknown compositions 404 on both routes.
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve?composition=nope",
                Some("{\"want\":1.0,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 404, "{refused}");
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/zoom",
                Some("{\"mode\":\"fit\",\"viewport-px\":800,\"composition\":\"nope\"}"),
            );
            assert_eq!(code, 404, "{refused}");
            // Zoom refusals: malformed bodies 400, semantic misses 409.
            for (label, bad) in [
                ("empty", ""),
                ("garbage", "{ nope"),
                ("unknown-mode", "{\"mode\":\"dolly\",\"viewport-px\":800}"),
                (
                    "manual-no-percent",
                    "{\"mode\":\"manual\",\"viewport-px\":800}",
                ),
                (
                    "bad-anchor",
                    "{\"mode\":\"fit\",\"viewport-px\":800,\"anchor\":\"nose\"}",
                ),
                (
                    "selection-empty",
                    "{\"mode\":\"selection\",\"viewport-px\":800}",
                ),
            ] {
                let (code, refused) = body_str(port, "POST", "/api/studio/zoom", Some(bad));
                assert_eq!(code, 400, "{label}: {refused}");
                assert!(refused.contains("studio refused"), "{label}: {refused}");
            }
            for (label, bad, needle) in [
                (
                    "unknown-clip",
                    "{\"mode\":\"selection\",\"selection\":[\"nope\"],\"viewport-px\":800}",
                    "names no clip",
                ),
                (
                    "past-end-clip",
                    "{\"mode\":\"selection\",\"selection\":[\"#9\"],\"viewport-px\":800}",
                    "past the open composition",
                ),
                (
                    "no-viewport",
                    "{\"mode\":\"fit\"}",
                    "names no `viewport-px`",
                ),
            ] {
                let (code, refused) = body_str(port, "POST", "/api/studio/zoom", Some(bad));
                assert_eq!(code, 409, "{label}: {refused}");
                assert!(refused.contains("studio refused"), "{label}: {refused}");
                assert!(refused.contains(needle), "{label}: {refused}");
            }
            // Every refusal left the delta, history, and zoom
            // preference exactly as they were (no zoom file yet: the
            // only zoom attempts refused).
            assert_eq!(delta_bytes(folder), before_delta, "refusals stage nothing");
            assert_eq!(turn_count(folder), before_turns, "refusals record nothing");
            assert_eq!(zoom_file(folder), None, "refused zooms persist nothing");
            assert_eq!(snap_file(folder), None, "snap reads persist nothing");
        },
    );
}

#[test]
fn studio_snapped_drag_records_one_undoable_turn() {
    with_server(
        "snap-drag-turn",
        text_project,
        |folder, _reg, _entry, port| {
            let before_turns = turn_count(folder);
            // The drag wants 2.05s; the setting snaps it to the 2.0s
            // edge, and the tool applies the snapped instant.
            let (code, found) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve",
                Some("{\"want\":2.05,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 200, "{found}");
            assert!(found.contains("\"snapped\":2.000"), "{found}");
            let (code, out) = body_str(
                port,
                "POST",
                "/api/studio/tool",
                Some("{\"tool\":\"select\",\"clip\":\"title\",\"to\":2.0}"),
            );
            assert_eq!(code, 200, "{out}");
            assert!(out.contains("\"applied\":true"), "{out}");
            assert!(out.contains("[2.000,4.000]"), "{out}");
            let listing = sdk::history::list_turns(folder).expect("history lists");
            assert_eq!(listing.turns.len(), before_turns + 1, "one turn recorded");
            assert_eq!(listing.turns.last().expect("a turn").actor, "studio");
            // Undo through the shared history restores the rows: the
            // snapped drag unwinds like any headless edit.
            let seq = listing.turns.last().expect("a turn").seq;
            sdk::history::undo_turn(folder, seq).expect("undo applies");
            let (code, _) = body_str(port, "POST", "/api/reload", None);
            assert_eq!(code, 200);
            let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
            assert_eq!(code, 200, "{rows}");
            assert!(rows.contains("[0.000,2.000]"), "{rows}");
        },
    );
}

#[test]
fn studio_snap_zoom_keeps_default_renders_byte_identical() {
    with_server(
        "snap-zoom-frames",
        text_project,
        |folder, _reg, _entry, port| {
            let (code, before) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            // Snap and zoom churn touches only hidden view state.
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/snap",
                Some("{\"threshold-px\":4}"),
            );
            assert_eq!(code, 200);
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/snap-resolve",
                Some("{\"want\":2.05,\"px-per-sec\":100}"),
            );
            assert_eq!(code, 200);
            let (code, _) = body_str(
                port,
                "POST",
                "/api/studio/zoom",
                Some("{\"mode\":\"manual\",\"percent\":150,\"viewport-px\":800}"),
            );
            assert_eq!(code, 200);
            let (code, after) = http(port, "GET", "/api/frame?i=5&action=step", None);
            assert_eq!(code, 200);
            assert_eq!(after, before, "view state never re-renders frames");
            // The composition sources never changed: only hidden files
            // (the SDK history dir) carry the new setting.
            let kdl = std::fs::read(folder.join("composition.kdl")).unwrap();
            assert!(
                std::str::from_utf8(&kdl)
                    .unwrap()
                    .contains("start=0 duration=2"),
                "sources stay byte-identical"
            );
            let mut visible: Vec<String> = std::fs::read_dir(folder)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            visible.sort();
            assert_eq!(
                visible,
                vec![".framesmith", "composition.kdl"],
                "{visible:?}"
            );
        },
    );
}

#[test]
fn studio_shell_serves_the_snap_zoom_surface() {
    with_server(
        "snap-zoom-page",
        text_project,
        |_folder, _reg, _entry, port| {
            let (code, page) = body_str(port, "GET", "/studio", None);
            assert_eq!(code, 200);
            // The visible project setting: switch plus threshold.
            assert!(page.contains("id=\"snap-enabled\""), "snap switch");
            assert!(page.contains("id=\"snap-threshold\""), "threshold input");
            assert!(page.contains("id=\"snap-save\""), "snap save");
            assert!(page.contains("id=\"snap-status\""), "snap status line");
            // Zoom: fit, manual percent with cursor/playhead anchor, and
            // zoom to selection, with its status line.
            assert!(page.contains("id=\"zoom-fit\""), "zoom fit");
            assert!(page.contains("id=\"zoom-percent\""), "manual percent");
            assert!(page.contains("id=\"zoom-anchor\""), "anchor select");
            assert!(page.contains("playhead"), "playhead anchor");
            assert!(page.contains("cursor"), "cursor anchor");
            assert!(page.contains("id=\"zoom-apply\""), "apply zoom");
            assert!(page.contains("id=\"zoom-selection\""), "zoom to selection");
            assert!(page.contains("id=\"zoom-status\""), "zoom status line");
            // The guide line element the panel paints at snapped instants.
            assert!(page.contains("id=\"snap-guide\""), "guide line");
            // Hyphenated request keys end to end, never camelCase.
            assert!(page.contains("threshold-px"), "hyphenated threshold key");
            assert!(page.contains("px-per-sec"), "hyphenated scale key");
            assert!(page.contains("anchor-at"), "hyphenated anchor key");
            assert!(page.contains("viewport-px"), "hyphenated viewport key");
            // The surface talks to the new endpoints.
            assert!(page.contains("/api/studio/snap"), "snap endpoints");
            assert!(
                page.contains("/api/studio/snap-resolve"),
                "snap-resolve endpoint"
            );
            assert!(page.contains("/api/studio/zoom"), "zoom endpoint");
        },
    );
}
