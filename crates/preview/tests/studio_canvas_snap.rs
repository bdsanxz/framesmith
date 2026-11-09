//! Canvas snap regressions (Sprint 17, T003 / CAP-003, J-01,
//! INT-003): drags snap to element edges and centres, frame edges and
//! centre, grid, and equidistance guides with guide lines; rulers
//! reflect the snapped position; conflicting targets resolve by the
//! documented priority showing which one won; with snapping off drags
//! move freely and the status line says so. Resolving stages no patch
//! and records no turn, while default renders stay byte-identical.

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
        "framesmith-studio-canvas-snap-{}-{}-{}",
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

/// Two boxes (`card` order 2, `panel` order 1), one text (`title`, no
/// authored extents), and one hidden box (`ghost`): 10 fps, 40 frames,
/// 320x180.
fn snap_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" order=2 start=0 duration=4\n    box id=\"panel\" x=50 y=30 width=80 height=40 fill=\"#ff0000\" order=1 start=0 duration=4\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=150 size=24 color=\"#ffffff\"\n    box id=\"ghost\" x=10 y=20 width=20 height=20 fill=\"#00ff00\" hidden=true start=0 duration=4\n}\n",
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
            start.elapsed() < Duration::from_secs(20),
            "server never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    f(&folder, &reg, entry.clone(), entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(entry.port), "port is quiet after shutdown");
}

fn kdl_bytes(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("composition.kdl")).expect("sources read")
}

#[test]
fn element_edges_and_centres_snap_with_guide_lines() {
    with_server("element", snap_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[11,44]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["did-snap"], true);
        assert_eq!(j["snapped"], serde_json::json!([10, 45]));
        assert_eq!(j["x"]["target"], "card left");
        assert_eq!(j["x"]["family"], "element");
        assert_eq!(j["x"]["priority"], 0);
        assert_eq!(j["y"]["target"], "card center-y");
        assert_eq!(j["y"]["family"], "element");
        let guides = j["guide-lines"].as_array().expect("guides list");
        assert_eq!(guides.len(), 2, "one guide line per snapped axis");
        assert!(guides.iter().any(|g| g["axis"] == "x" && g["at"] == 10));
        assert!(guides.iter().any(|g| g["axis"] == "y" && g["at"] == 45));
        let winner = j["winner"].as_str().expect("winner text");
        assert!(winner.contains("card left"), "winner names x: {winner}");
        assert!(winner.contains("card center-y"), "winner names y: {winner}");
        let status = j["status-line"].as_str().expect("status line");
        assert!(
            status.contains("guide line"),
            "status shows guides: {status}"
        );
        assert!(
            status.contains("beating"),
            "status shows beaten rivals: {status}"
        );
        assert_eq!(
            j["priority-order"],
            serde_json::json!(["element", "equidistance", "frame", "grid"])
        );
        assert!(
            !j.to_string().contains("\"turn\""),
            "resolve records no turn"
        );
    });
}

#[test]
fn frame_edges_centre_and_grid_snap() {
    with_server("frame-grid", snap_project, |_folder, _reg, _entry, port| {
        // Frame centre on y with a grid-only landing on x (the dragged
        // boxes stay out of the resolve through `exclude`).
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[300,89],"exclude":["card","panel"]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["snapped"], serde_json::json!([300, 90]));
        assert_eq!(j["y"]["target"], "frame center-y");
        assert_eq!(j["y"]["family"], "frame");
        assert_eq!(j["x"]["family"], "grid");
        // A grid-only landing with both dragged boxes excluded.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[205,103],"exclude":["card","panel"]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["snapped"], serde_json::json!([200, 100]));
        assert_eq!(j["x"]["family"], "grid");
        assert_eq!(j["y"]["family"], "grid");
        // A frame edge beats the grid line holding the same position.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[321,200],"exclude":["card","panel"]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["x"]["target"], "frame right");
        assert_eq!(j["x"]["family"], "frame");
    });
}

#[test]
fn equidistance_guide_wins_with_documented_priority() {
    with_server(
        "equidistance",
        snap_project,
        |_folder, _reg, _entry, port| {
            // x = 30 completes the equal spacing between the left edges of
            // `card` (10) and `panel` (50); the grid line at 30 loses by
            // priority even at the same distance.
            let (code, j) = json(
                port,
                "POST",
                "/api/studio/canvas-snap",
                Some(r#"{"want":[31,200]}"#),
            );
            assert_eq!(code, 200);
            assert_eq!(j["x"]["snapped"], 30);
            assert_eq!(j["x"]["family"], "equidistance");
            assert!(
                j["x"]["target"].as_str().unwrap().contains("between"),
                "guide names both edges: {}",
                j["x"]["target"]
            );
            assert!(!j["y"]["did-snap"].as_bool().unwrap());
            let status = j["status-line"].as_str().expect("status line");
            assert!(
                status.contains("equidistance"),
                "status names family: {status}"
            );
            assert!(
                status.contains("beating"),
                "status shows beaten rivals: {status}"
            );
        },
    );
}

#[test]
fn conflicting_targets_resolve_by_priority_showing_the_winner() {
    with_server("conflict", snap_project, |_folder, _reg, _entry, port| {
        // x = 159 sits 1 px from `frame center-x` (160) yet 6 px from
        // the equidistance midpoint 165: priority crowns equidistance,
        // and the answer says so.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[159,200]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["snapped"][0], 165);
        assert_eq!(j["x"]["family"], "equidistance");
        let winner = j["winner"].as_str().expect("winner text");
        assert!(
            winner.contains("equidistance"),
            "winner shows family: {winner}"
        );
        let status = j["status-line"].as_str().expect("status line");
        assert!(
            status.contains("beating 1 rival"),
            "winner beats frame: {status}"
        );
        // The same-position conflict on x = 11 crowns `card left`
        // over the grid line at 10.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[11,200]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["x"]["target"], "card left");
        assert_eq!(j["x"]["family"], "element");
        assert_eq!(j["x"]["rivals"], 1);
    });
}

#[test]
fn rulers_reflect_the_snapped_position() {
    with_server("rulers", snap_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[11,44]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["ruler-x"], 10, "ruler-x follows the snap");
        assert_eq!(j["ruler-y"], 45, "ruler-y follows the snap");
        assert_eq!(j["ruler-x"], j["snapped"][0]);
        assert_eq!(j["ruler-y"], j["snapped"][1]);
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        for needle in [
            "canvas-snap-panel",
            "canvas-snap-status",
            "canvas-snap-priority",
            "snap priority: element, then equidistance, then frame, then grid",
            "background-color",
        ] {
            assert!(page.contains(needle), "studio page shows {needle}");
        }
    });
}

#[test]
fn snap_off_drags_freely_with_status_line() {
    with_server("snap-off", snap_project, |_folder, _reg, _entry, port| {
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/snap",
            Some(r#"{"enabled":false}"#),
        );
        assert_eq!(code, 200);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[11,44]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["did-snap"], false);
        assert_eq!(j["snapped"], serde_json::json!([11, 44]));
        assert_eq!(j["guide-lines"], serde_json::json!([]));
        assert_eq!(j["status-line"], "snapping is off: drags move freely.");
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/snap",
            Some(r#"{"enabled":true}"#),
        );
        assert_eq!(code, 200);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[11,44]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["did-snap"], true, "re-enabling restores snapping");
    });
}

#[test]
fn canvas_snap_refuses_bad_bodies_and_missing_projects() {
    with_server("refusals", snap_project, |_folder, _reg, _entry, port| {
        for body in [
            "",
            "{}",
            r#"{"want":[1]}"#,
            r#"{"want":["a",2]}"#,
            r#"{"want":[1,2],"scale":0}"#,
            r#"{"want":[1,2],"grid":-5}"#,
            r#"{"want":[1,2],"grid":999}"#,
        ] {
            let (code, text) = body_str(port, "POST", "/api/studio/canvas-snap", Some(body));
            assert_eq!(code, 400, "refusal for {body:?}");
            assert!(
                text.contains("Nothing was resolved"),
                "refusal names stasis: {text}"
            );
        }
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/canvas-snap?composition=nope",
            Some(r#"{"want":[11,44]}"#),
        );
        assert_eq!(code, 404);
        assert!(text.contains("nope"), "missing composition named: {text}");
    });
}

#[test]
fn canvas_snap_unreachable_project_refuses() {
    with_server("unreachable", snap_project, |folder, _reg, _entry, port| {
        std::fs::remove_dir_all(folder).expect("project removed");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[11,44]}"#),
        );
        assert_eq!(code, 409);
        assert!(text.contains("unreachable"), "unreachable named: {text}");
    });
}

#[test]
fn canvas_snap_stages_nothing_and_frames_stay_identical() {
    with_server("no-stage", snap_project, |folder, _reg, _entry, port| {
        let sources = kdl_bytes(folder);
        let (before_code, before_png) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(before_code, 200, "frame serves before the resolve");
        for body in [
            r#"{"want":[11,44]}"#,
            r#"{"want":[31,200]}"#,
            r#"{"want":[159,200],"exclude":["card"]}"#,
            r#"{"want":[205,103],"grid":5}"#,
        ] {
            let (code, text) = body_str(port, "POST", "/api/studio/canvas-snap", Some(body));
            assert_eq!(code, 200, "resolve for {body}");
            assert!(
                !text.contains("\"turn\""),
                "resolve records no turn: {body}"
            );
        }
        assert_eq!(kdl_bytes(folder), sources, "sources stay byte-identical");
        let (after_code, after_png) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(after_code, 200, "frame serves after the resolve");
        assert_eq!(after_png, before_png, "default frame stays byte-identical");
    });
}

#[test]
fn canvas_snap_keeps_canvas_and_timeline_selection_in_sync() {
    with_server("selection", snap_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/canvas-snap",
            Some(r#"{"want":[11,44],"selection":["card","nope"]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(j["selection"]["kept"], serde_json::json!(["card"]));
        assert_eq!(j["selection"]["dropped"], serde_json::json!(["nope"]));
    });
}
