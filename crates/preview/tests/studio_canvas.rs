//! Canvas view regressions (Sprint 17, T001 / CAP-001, J-01,
//! STATE-01, REC-003): the canvas panel shows the film with zoom 25
//! to 400 percent anchored at the cursor or centre, free pan, fit,
//! fullscreen, rulers, a minimap, and 90/70 percent safe margins on
//! demand; view state never touches sources; out-of-bounds zoom
//! clamps showing the value; a missing film shows the empty canvas
//! with the reason. View changes stage no patch and record no turn,
//! so default renders stay byte-identical.

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
        "framesmith-studio-canvas-{}-{}-{}",
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
/// 40 frames, 4 film seconds, 320x180.
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

fn canvas_file(folder: &Path) -> Option<Vec<u8>> {
    std::fs::read(folder.join(".framesmith").join("studio-canvas.json")).ok()
}

fn kdl_bytes(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("composition.kdl")).expect("sources stay readable")
}

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .map(|l| l.turns.len())
        .unwrap_or(0)
}

#[test]
fn canvas_defaults_to_fit_with_every_overlay_off() {
    with_server("defaults", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(port, "GET", "/api/studio/canvas", None);
        assert_eq!(code, 200, "canvas state answers: {state}");
        assert_eq!(state["zoom-percent"], 100.0);
        assert_eq!(state["anchor"], "center");
        assert_eq!(state["pan-x"], 0.0);
        assert_eq!(state["pan-y"], 0.0);
        assert_eq!(state["fullscreen"], false);
        assert_eq!(state["show-rulers"], false);
        assert_eq!(state["show-minimap"], false);
        assert_eq!(state["show-safe-90"], false);
        assert_eq!(state["show-safe-70"], false);
        assert_eq!(state["empty"], false);
        assert_eq!(state["reason"], serde_json::Value::Null);
        assert_eq!(state["transport-enabled"], true);
        assert_eq!(state["film"]["width"], 320);
        assert_eq!(state["film"]["height"], 180);
        assert_eq!(state["film"]["frame-count"], 40);
        assert_eq!(
            state["safe-90"],
            serde_json::json!([16.0, 9.0, 288.0, 162.0])
        );
        assert_eq!(
            state["safe-70"],
            serde_json::json!([48.0, 27.0, 224.0, 126.0])
        );
        assert!(state["ruler-step"].as_f64().unwrap_or(0.0) > 0.0);
        assert_eq!(state["restored"], false);
        assert!(state["status-line"]
            .as_str()
            .unwrap_or("")
            .contains("zoom 100%"));
    });
}

#[test]
fn canvas_zoom_runs_25_to_400_at_both_anchors() {
    with_server("zoom-range", text_project, |_folder, _reg, _entry, port| {
        for (percent, anchor) in [
            (25.0, "cursor"),
            (100.0, "center"),
            (400.0, "cursor"),
            (250.0, "center"),
            (25.0, "center"),
            (400.0, "center"),
        ] {
            let (code, state) = json(
                port,
                "POST",
                "/api/studio/canvas",
                Some(&format!(
                    "{{\"zoom-percent\":{percent},\"anchor\":{anchor:?}}}"
                )),
            );
            assert_eq!(code, 200, "zoom {percent}% at {anchor} applies: {state}");
            assert_eq!(state["zoom-percent"], percent);
            assert_eq!(state["anchor"], anchor);
            assert_eq!(
                state["clamped"],
                serde_json::Value::Null,
                "no clamp flag leaks"
            );
            let line = format!("{} {}", state["detail"], state["status-line"]);
            assert!(
                line.contains(&format!("{percent:.0}%")),
                "shows the value: {line}"
            );
            assert!(line.contains(anchor), "names the anchor: {line}");
            assert!(!state["clamped-request"].is_boolean(), "no stray clamp key");
        }
        // The last zoom persists for the next paint.
        let (code, state) = json(port, "GET", "/api/studio/canvas", None);
        assert_eq!(code, 200);
        assert_eq!(state["zoom-percent"], 400.0);
        assert_eq!(state["anchor"], "center");
        assert_eq!(state["restored"], true);
    });
}

#[test]
fn canvas_out_of_bounds_zoom_clamps_showing_the_clamped_value() {
    with_server("zoom-clamp", text_project, |_folder, _reg, _entry, port| {
        let (code, low) = json(
            port,
            "POST",
            "/api/studio/canvas",
            Some("{\"zoom-percent\":10,\"anchor\":\"cursor\"}"),
        );
        assert_eq!(code, 200, "below the floor still answers: {low}");
        assert_eq!(low["zoom-percent"], 25.0);
        let detail = low["detail"].as_str().unwrap_or("").to_string();
        assert!(detail.contains("25"), "shows the clamped value: {detail}");
        assert!(detail.contains("25% floor"), "names the bound: {detail}");

        let (code, high) = json(
            port,
            "POST",
            "/api/studio/canvas",
            Some("{\"zoom-percent\":500,\"anchor\":\"center\"}"),
        );
        assert_eq!(code, 200, "past the ceiling still answers: {high}");
        assert_eq!(high["zoom-percent"], 400.0);
        let detail = high["detail"].as_str().unwrap_or("").to_string();
        assert!(detail.contains("400"), "shows the clamped value: {detail}");
        assert!(detail.contains("400% ceiling"), "names the bound: {detail}");

        let (code, state) = json(port, "GET", "/api/studio/canvas", None);
        assert_eq!(code, 200);
        assert_eq!(state["zoom-percent"], 400.0, "the clamped value persists");
    });
}

#[test]
fn canvas_pan_moves_freely_and_fit_frames_the_film() {
    with_server("pan-fit", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/canvas",
            Some("{\"zoom-percent\":250,\"anchor\":\"cursor\",\"pan-x\":120,\"pan-y\":-45}"),
        );
        assert_eq!(code, 200, "pan applies: {state}");
        assert_eq!(state["zoom-percent"], 250.0);
        assert_eq!(state["pan-x"], 120.0);
        assert_eq!(state["pan-y"], -45.0);

        let (code, state) = json(port, "POST", "/api/studio/canvas", Some("{\"fit\":true}"));
        assert_eq!(code, 200, "fit applies: {state}");
        assert_eq!(state["zoom-percent"], 100.0);
        assert_eq!(state["anchor"], "center");
        assert_eq!(state["pan-x"], 0.0);
        assert_eq!(state["pan-y"], 0.0);
        let detail = state["detail"].as_str().unwrap_or("").to_string();
        assert!(detail.contains("fit"), "fit says so: {detail}");
        assert!(detail.contains("100%"), "fit shows the value: {detail}");
    });
}

#[test]
fn canvas_overlays_toggle_on_demand() {
    with_server("overlays", text_project, |_folder, _reg, _entry, port| {
        for key in [
            "show-rulers",
            "show-minimap",
            "show-safe-90",
            "show-safe-70",
        ] {
            let (code, state) = json(
                port,
                "POST",
                "/api/studio/canvas",
                Some(&format!("{{\"{key}\":true}}")),
            );
            assert_eq!(code, 200, "{key} turns on: {state}");
            assert_eq!(state[key], true);
        }
        let (code, state) = json(port, "GET", "/api/studio/canvas", None);
        assert_eq!(code, 200);
        for key in [
            "show-rulers",
            "show-minimap",
            "show-safe-90",
            "show-safe-70",
        ] {
            assert_eq!(state[key], true, "{key} stays on: {state}");
        }
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/canvas",
            Some("{\"show-safe-90\":false,\"show-minimap\":false}"),
        );
        assert_eq!(code, 200, "toggles turn off: {state}");
        assert_eq!(state["show-safe-90"], false);
        assert_eq!(state["show-minimap"], false);
        assert_eq!(state["show-safe-70"], true, "untouched overlays stay on");
        assert_eq!(state["show-rulers"], true, "untouched overlays stay on");
        // Fullscreen is view state too.
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/canvas",
            Some("{\"fullscreen\":true}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["fullscreen"], true);
    });
}

#[test]
fn canvas_view_never_touches_sources() {
    with_server("no-touch", text_project, |folder, _reg, _entry, port| {
        let before = kdl_bytes(folder);
        for body in [
            "{\"zoom-percent\":250,\"anchor\":\"cursor\"}",
            "{\"pan-x\":60,\"pan-y\":30}",
            "{\"fit\":true}",
            "{\"show-rulers\":true,\"show-minimap\":true,\"show-safe-90\":true,\"show-safe-70\":true}",
            "{\"fullscreen\":true}",
            "{\"zoom-percent\":500}",
        ] {
            let (code, _) = json(port, "POST", "/api/studio/canvas", Some(body));
            assert_eq!(code, 200, "view update applies: {body}");
        }
        assert_eq!(kdl_bytes(folder), before, "sources stay byte-identical");
        assert_eq!(turn_count(folder), 0, "no history turn records a view move");
        assert!(
            std::fs::read(folder.join(".framesmith").join("sdk-overrides.json")).is_err(),
            "no SDK delta stages a view move"
        );
        assert!(
            canvas_file(folder).is_some(),
            "only the hidden canvas file lands"
        );
    });
}

#[test]
fn canvas_refusals_name_the_cause_and_save_nothing() {
    with_server("refusals", text_project, |folder, _reg, _entry, port| {
        for body in [
            "",
            "not json",
            "[1,2]",
            "{}",
            "{\"zoom\":150}",
            "{\"zoom-percent\":\"far\"}",
            "{\"zoom-percent\":null}",
            "{\"anchor\":\"corner\"}",
            "{\"anchor\":42}",
            "{\"pan-x\":null}",
            "{\"fit\":\"yes\"}",
            "{\"fullscreen\":\"yes\"}",
            "{\"show-rulers\":1}",
            "{\"selection\":[42]}",
            "{\"selection\":[\"\"]}",
            "{\"composition\":42}",
        ] {
            let (code, text) = body_str(port, "POST", "/api/studio/canvas", Some(body));
            assert_eq!(code, 400, "refuses {body:?}: {text}");
            assert!(
                text.starts_with("studio refused: "),
                "refusal prefix: {text}"
            );
            assert!(text.contains("Nothing was saved"), "nothing saved: {text}");
        }
        assert!(
            canvas_file(folder).is_none(),
            "no refusal writes the hidden file"
        );
        assert!(!kdl_bytes(folder).is_empty());
        assert_eq!(turn_count(folder), 0, "no refusal records a turn");
    });
}

#[test]
fn canvas_missing_film_shows_the_empty_canvas_with_the_reason() {
    with_server("empty", text_project, |_folder, _reg, _entry, port| {
        let (code, text) = body_str(port, "GET", "/api/studio/canvas?composition=nope", None);
        assert_eq!(code, 404, "unknown composition refuses: {text}");
        assert!(text.contains("nope"), "names the missing film: {text}");

        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/canvas",
            Some("{\"zoom-percent\":150,\"composition\":\"nope\"}"),
        );
        assert_eq!(code, 404, "a view on a missing film refuses: {text}");
        assert!(text.contains("nope"), "names the missing film: {text}");
    });
}

#[test]
fn canvas_selection_stays_in_sync_with_the_timeline() {
    with_server("selection", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(port, "GET", "/api/studio/canvas?selection=title,%231", None);
        assert_eq!(code, 200, "known keys sync: {state}");
        assert_eq!(
            state["selection"]["kept"],
            serde_json::json!(["title", "#1"])
        );
        assert_eq!(state["selection"]["dropped"], serde_json::json!([]));

        let (code, state) = json(
            port,
            "GET",
            "/api/studio/canvas?selection=title,nope,%2399",
            None,
        );
        assert_eq!(code, 200, "unknown keys drop with a note: {state}");
        assert_eq!(state["selection"]["kept"], serde_json::json!(["title"]));
        assert_eq!(
            state["selection"]["dropped"],
            serde_json::json!(["nope", "#99"])
        );

        let (code, state) = json(
            port,
            "POST",
            "/api/studio/canvas",
            Some("{\"zoom-percent\":150,\"selection\":[\"bye\",\"ghost\"]}"),
        );
        assert_eq!(code, 200, "echoed selection syncs: {state}");
        assert_eq!(state["selection"]["kept"], serde_json::json!(["bye"]));
        assert_eq!(state["selection"]["dropped"], serde_json::json!(["ghost"]));
        assert_eq!(state["zoom-percent"], 150.0, "the view still applies");
    });
}

#[test]
fn canvas_page_hosts_the_view_panel_with_hyphenated_styles() {
    with_server("page", text_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        for id in [
            "canvas-view-bar",
            "canvas-zoom",
            "canvas-anchor",
            "canvas-zoom-apply",
            "canvas-fit",
            "canvas-fullscreen",
            "canvas-pan-apply",
            "canvas-rulers",
            "canvas-minimap-toggle",
            "canvas-safe-90",
            "canvas-safe-70",
            "canvas-status",
            "canvas-stage",
            "canvas-empty",
            "canvas-minimap",
            "canvas-selection-note",
        ] {
            assert!(page.contains(id), "the page hosts {id}");
        }
        for style in ["background-color", "border-radius", "object-fit", "z-index"] {
            assert!(
                page.contains(style),
                "styles stay CSS-like hyphenated: {style}"
            );
        }
        assert!(
            !page.contains("zoom_percent"),
            "no snake_case view keys leak"
        );
        assert!(
            !page.contains("show_rulers"),
            "no snake_case overlay keys leak"
        );
    });
}

#[test]
fn canvas_moves_keep_default_renders_byte_identical() {
    with_server(
        "byte-identical",
        text_project,
        |_folder, _reg, _entry, port| {
            let (code, before) = http(port, "GET", "/api/frame?i=0&action=step", None);
            assert_eq!(code, 200);
            assert!(!before.is_empty());
            for body in [
            "{\"zoom-percent\":400,\"anchor\":\"cursor\",\"pan-x\":90,\"pan-y\":60}",
            "{\"show-rulers\":true,\"show-minimap\":true,\"show-safe-90\":true,\"show-safe-70\":true,\"fullscreen\":true}",
            "{\"fit\":true}",
        ] {
            let (code, _) = json(port, "POST", "/api/studio/canvas", Some(body));
            assert_eq!(code, 200);
        }
            let (code, after) = http(port, "GET", "/api/frame?i=0&action=step", None);
            assert_eq!(code, 200);
            assert_eq!(before, after, "view state never feeds the render pipeline");
        },
    );
}

#[test]
fn canvas_font_try_order_passes_through_untouched() {
    let root = tmp("fonts");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"f\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\" font-family=\"Nope, Fallback\"\n}\n",
    );
    // Read-only open: validates without persisting, so the probe
    // writes nothing — not the history directory, not the delta.
    let session = sdk::SdkSession::open(
        &folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("font project validates");
    let lists = preview::studio_canvas::canvas_font_lists_of(session.effective_composition());
    assert_eq!(
        lists,
        vec!["Nope, Fallback".to_string()],
        "try-order passes through in comma order, never resolved"
    );
    assert!(
        !folder.join(".framesmith").exists(),
        "the read-only font probe writes nothing"
    );
}
