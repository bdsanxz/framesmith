//! Canvas transport extras regressions (Sprint 17, T005 / CAP-005,
//! J-05, STATE-01, REC-005): J, K, and L shuttle at 1x, 2x, and 4x
//! with K pausing, arrows step exactly one frame, the speed menu runs
//! 0.25x to 2x, the seek bar scrubs under the canvas, playback health
//! reports the clock followed and the still-picture duration, and a
//! film short of data holds its last frame with the hold reported,
//! never looping a tail snippet. Transport moves no composition
//! data: only validated menu speeds land in the hidden history
//! directory, so default renders stay byte-identical with no history
//! turn recorded.

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
        "framesmith-studio-transport-{tag}-{}-{}",
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
/// 40 frames, 4 film seconds, 320x180, silent.
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

fn kdl_bytes(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("composition.kdl")).expect("sources stay readable")
}

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .map(|l| l.turns.len())
        .unwrap_or(0)
}

fn hidden_files(folder: &Path) -> Vec<String> {
    let dir = folder.join(".framesmith");
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn transport_defaults_report_paused_health_with_the_seek_bar() {
    with_server("defaults", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(port, "GET", "/api/studio/transport", None);
        assert_eq!(code, 200, "transport state answers: {state}");
        assert_eq!(state["frame"], 0);
        assert_eq!(state["frame-count"], 40);
        assert_eq!(state["playing"], false);
        assert_eq!(state["enabled"], true);
        assert_eq!(state["empty"], false);
        assert_eq!(state["reason"], serde_json::Value::Null);
        assert_eq!(state["transport-enabled"], true);
        assert_eq!(state["speed"], 1.0);
        assert_eq!(
            state["speed-menu"],
            serde_json::json!([0.25, 0.5, 0.75, 1, 1.5, 2])
        );
        assert_eq!(state["shuttle"]["direction"], "idle");
        assert_eq!(state["shuttle"]["rate"], serde_json::Value::Null);
        assert_eq!(state["shuttle"]["playing"], false);
        assert_eq!(state["seek-bar"]["min"], 0);
        assert_eq!(state["seek-bar"]["max"], 39);
        assert_eq!(state["seek-bar"]["value"], 0);
        assert_eq!(state["health"]["clock"], "paused");
        assert!(state["health"]["still-secs"].as_f64().unwrap_or(-1.0) >= 0.0);
        assert_eq!(state["health"]["held"], serde_json::Value::Null);
        let detail = state["health"]["detail"].as_str().unwrap_or("");
        assert!(detail.contains("clock paused"), "names the clock: {detail}");
        assert!(
            detail.contains("still picture"),
            "names the still duration: {detail}"
        );
        let line = state["detail"].as_str().unwrap_or("").to_string();
        assert!(
            line.contains("frame 0/40"),
            "detail shows the playhead: {line}"
        );
    });
}

#[test]
fn shuttle_l_cycles_1x_2x_4x_and_k_pauses() {
    with_server("shuttle-l", text_project, |_folder, _reg, _entry, port| {
        for (want, press) in [(1.0, "shuttle-l"), (2.0, "shuttle-l"), (4.0, "shuttle-l")] {
            let (code, state) = json(
                port,
                "POST",
                "/api/studio/transport",
                Some(&format!("{{\"action\":\"{press}\"}}")),
            );
            assert_eq!(code, 200, "shuttle {press} to {want}x applies: {state}");
            assert_eq!(state["playing"], true);
            assert_eq!(state["shuttle"]["direction"], "forward");
            assert_eq!(state["shuttle"]["rate"], want);
        }
        // The 4x shuttle parks the playhead motion but keeps driving.
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"shuttle-l\"}"),
        );
        assert_eq!(code, 200);
        assert_eq!(
            state["shuttle"]["rate"], 1.0,
            "shuttle wraps 4x back to 1x: {state}"
        );
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"shuttle-k\"}"),
        );
        assert_eq!(code, 200, "K pauses: {state}");
        assert_eq!(state["playing"], false);
        assert_eq!(state["shuttle"]["direction"], "forward");
        assert_eq!(
            state["shuttle"]["rate"], 1.0,
            "K keeps the rate for the readout: {state}"
        );
        assert_eq!(state["health"]["clock"], "paused");
    });
}

#[test]
fn shuttle_j_drives_reverse_and_directions_restart_at_1x() {
    with_server("shuttle-j", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"seek\",\"frame\":20}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["frame"], 20);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"shuttle-j\"}"),
        );
        assert_eq!(code, 200, "J shuttles reverse: {state}");
        assert_eq!(state["shuttle"]["direction"], "reverse");
        assert_eq!(state["shuttle"]["rate"], 1.0);
        assert_eq!(state["playing"], true);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"tick\",\"elapsed-ms\":500}"),
        );
        assert_eq!(code, 200, "reverse tick moves back: {state}");
        assert_eq!(state["frame"], 15, "500ms at 1x is 5 frames back: {state}");
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"shuttle-l\"}"),
        );
        assert_eq!(code, 200, "L restarts forward at 1x: {state}");
        assert_eq!(state["shuttle"]["direction"], "forward");
        assert_eq!(state["shuttle"]["rate"], 1.0);
    });
}

#[test]
fn arrows_step_exactly_one_frame() {
    with_server("steps", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"seek\",\"frame\":10}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["frame"], 10);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"step-forward\"}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["frame"], 11, "one frame forward: {state}");
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"step-back\"}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["frame"], 10, "one frame back: {state}");
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"seek\",\"frame\":0}"),
        );
        assert_eq!(code, 200);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"step-back\"}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["frame"], 0, "frame 0 clamps: {state}");
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"seek\",\"frame\":39}"),
        );
        assert_eq!(code, 200);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"step-forward\"}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["frame"], 39, "the last frame clamps: {state}");
    });
}

#[test]
fn speed_menu_runs_0_25x_to_2x_and_persists() {
    with_server("speed", text_project, |folder, _reg, _entry, port| {
        for speed in [0.25, 0.5, 0.75, 1.0, 1.5, 2.0] {
            let (code, state) = json(
                port,
                "POST",
                "/api/studio/transport",
                Some(&format!("{{\"action\":\"set-speed\",\"speed\":{speed}}}")),
            );
            assert_eq!(code, 200, "menu speed {speed}x applies: {state}");
            assert_eq!(state["speed"], speed);
        }
        let (code, state) = json(port, "GET", "/api/studio/transport", None);
        assert_eq!(code, 200);
        assert_eq!(state["speed"], 2.0, "the menu speed persists: {state}");
        assert_eq!(state["speed-restored"], true);
        let raw = std::fs::read(folder.join(".framesmith").join("studio-transport.json"))
            .expect("the hidden transport file lands");
        let saved: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&raw))
            .expect("hidden transport file is JSON");
        assert_eq!(saved["speed"], 2.0);
        assert_eq!(saved["version"], 1);
        for bad in [4.0, 0.1, 0.0, -1.0] {
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/transport",
                Some(&format!("{{\"action\":\"set-speed\",\"speed\":{bad}}}")),
            );
            assert_eq!(code, 400, "menu refuses {bad}x: {text}");
            assert!(
                text.starts_with("studio refused: "),
                "refusal prefix: {text}"
            );
            assert!(text.contains("0.25x to 2x"), "names the menu bound: {text}");
            assert!(text.contains("Nothing moved"), "nothing moved: {text}");
        }
    });
}

#[test]
fn seek_bar_scrubs_under_the_canvas() {
    with_server("seek", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"seek\",\"frame\":20}"),
        );
        assert_eq!(code, 200, "seek parks the playhead: {state}");
        assert_eq!(state["frame"], 20);
        assert_eq!(state["seek-bar"]["value"], 20);
        assert_eq!(state["seek-bar"]["max"], 39);
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"seek\",\"frame\":40}"),
        );
        assert_eq!(code, 409, "seek past the end refuses: {text}");
        assert!(
            text.contains("outside the timeline"),
            "names the window: {text}"
        );
        assert!(text.contains("0..40"), "names the valid frames: {text}");
    });
}

#[test]
fn playback_health_reports_the_clock_and_the_still_picture() {
    with_server("health", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"play\"}"),
        );
        assert_eq!(code, 200, "play starts: {state}");
        assert_eq!(state["playing"], true);
        // The text project carries no audio, so the wall clock owns
        // the silent picture.
        assert_eq!(state["health"]["clock"], "wall");
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"tick\",\"elapsed-ms\":300}"),
        );
        assert_eq!(code, 200, "tick advances 3 frames at 1x: {state}");
        assert_eq!(state["frame"], 3);
        assert!(state["health"]["still-secs"].as_f64().unwrap_or(99.0) < 5.0);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"pause\"}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["playing"], false);
        assert_eq!(state["health"]["clock"], "paused");
    });
}

#[test]
fn short_film_holds_its_last_frame_with_the_hold_reported() {
    with_server("hold", text_project, |_folder, _reg, _entry, port| {
        // Arm the timeline loop: the canvas hold still wins over it —
        // a short film never loops a tail snippet.
        let (code, _) = body_str(port, "POST", "/api/loop?on=1", None);
        assert_eq!(code, 200);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"seek\",\"frame\":38}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["loop-enabled"], true);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"shuttle-l\"}"),
        );
        assert_eq!(code, 200, "shuttle starts from 38: {state}");
        assert_eq!(state["playing"], true);
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"tick\",\"elapsed-ms\":2000}"),
        );
        assert_eq!(code, 200, "the tail parks instead of looping: {state}");
        assert_eq!(state["frame"], 39, "the last frame holds: {state}");
        assert_eq!(state["playing"], false, "the hold pauses: {state}");
        let held = state["health"]["held"].as_str().unwrap_or("").to_string();
        assert!(
            held.contains("39 of 40"),
            "the hold names the frame: {held}"
        );
        assert!(
            held.contains("instead of looping"),
            "the hold rules out loops: {held}"
        );
        let detail = state["health"]["detail"].as_str().unwrap_or("").to_string();
        assert!(
            detail.contains("39 of 40"),
            "health reports the hold: {detail}"
        );
        // A further tick while paused is a no-op: the hold stands.
        let (code, state) = json(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"tick\",\"elapsed-ms\":2000}"),
        );
        assert_eq!(code, 200);
        assert_eq!(state["frame"], 39, "the hold stands: {state}");
    });
}

#[test]
fn transport_refusals_name_the_cause_and_move_nothing() {
    with_server("refusals", text_project, |folder, _reg, _entry, port| {
        for body in [
            "",
            "not json",
            "[1,2]",
            "{}",
            "{\"action\":\"fly\"}",
            "{\"action\":\"shuttle-x\"}",
            "{\"action\":\"seek\"}",
            "{\"action\":\"seek\",\"frame\":\"far\"}",
            "{\"action\":\"seek\",\"frame\":-2}",
            "{\"action\":\"seek\",\"frame\":2.5}",
            "{\"action\":\"seek\",\"frame\":99}",
            "{\"action\":\"set-speed\"}",
            "{\"action\":\"set-speed\",\"speed\":\"fast\"}",
            "{\"action\":\"tick\",\"elapsed-ms\":\"long\"}",
            "{\"action\":\"tick\",\"elapsed-ms\":-1}",
            "{\"action\":\"play\",\"selection\":[42]}",
            "{\"action\":\"play\",\"selection\":[\"\"]}",
            "{\"action\":\"play\",\"composition\":42}",
        ] {
            let (code, text) = body_str(port, "POST", "/api/studio/transport", Some(body));
            assert!(
                code == 400 || code == 409,
                "refuses {body:?} with 400 or 409: {code} {text}"
            );
            assert!(text.contains("refused"), "names the cause: {text}");
        }
        let (code, text) = body_str(port, "GET", "/api/studio/transport?composition=nope", None);
        assert_eq!(code, 404, "a missing film refuses: {text}");
        assert!(text.contains("nope"), "names the missing film: {text}");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transport",
            Some("{\"action\":\"shuttle-l\",\"composition\":\"nope\"}"),
        );
        assert_eq!(code, 404, "a strip move on a missing film refuses: {text}");
        assert!(text.contains("nope"), "names the missing film: {text}");
        assert_eq!(turn_count(folder), 0, "no refusal records a turn");
    });
}

#[test]
fn transport_moves_neither_sources_nor_history() {
    with_server("no-touch", text_project, |folder, _reg, _entry, port| {
        let before = kdl_bytes(folder);
        for body in [
            "{\"action\":\"shuttle-l\"}",
            "{\"action\":\"tick\",\"elapsed-ms\":500}",
            "{\"action\":\"shuttle-k\"}",
            "{\"action\":\"step-forward\"}",
            "{\"action\":\"step-back\"}",
            "{\"action\":\"seek\",\"frame\":12}",
            "{\"action\":\"set-speed\",\"speed\":0.5}",
            "{\"action\":\"play\"}",
            "{\"action\":\"pause\"}",
            "{\"action\":\"shuttle-j\"}",
            "{\"action\":\"tick\",\"elapsed-ms\":250}",
            "{\"action\":\"shuttle-k\"}",
        ] {
            let (code, _) = json(port, "POST", "/api/studio/transport", Some(body));
            assert_eq!(code, 200, "strip action applies: {body}");
        }
        assert_eq!(kdl_bytes(folder), before, "sources stay byte-identical");
        assert_eq!(
            turn_count(folder),
            0,
            "no history turn records a strip move"
        );
        assert!(
            std::fs::read(folder.join(".framesmith").join("sdk-overrides.json")).is_err(),
            "no SDK delta stages a strip move"
        );
        for name in hidden_files(folder) {
            assert!(
                ["studio-transport.json", "studio-canvas.json"].contains(&name.as_str())
                    || name.starts_with("studio-"),
                "only hidden studio state lands: {name}"
            );
        }
    });
}

#[test]
fn transport_moves_keep_default_renders_byte_identical() {
    with_server(
        "byte-identical",
        text_project,
        |_folder, _reg, _entry, port| {
            let (code, before) = http(port, "GET", "/api/frame?i=0&action=step", None);
            assert_eq!(code, 200);
            assert!(!before.is_empty());
            for body in [
                "{\"action\":\"shuttle-l\"}",
                "{\"action\":\"tick\",\"elapsed-ms\":500}",
                "{\"action\":\"set-speed\",\"speed\":0.5}",
                "{\"action\":\"seek\",\"frame\":7}",
                "{\"action\":\"shuttle-k\"}",
            ] {
                let (code, _) = json(port, "POST", "/api/studio/transport", Some(body));
                assert_eq!(code, 200);
            }
            let (code, after) = http(port, "GET", "/api/frame?i=0&action=step", None);
            assert_eq!(code, 200);
            assert_eq!(before, after, "strip moves never feed the render pipeline");
        },
    );
}

#[test]
fn transport_selection_stays_in_sync_with_the_timeline() {
    with_server("selection", text_project, |_folder, _reg, _entry, port| {
        let (code, state) = json(
            port,
            "GET",
            "/api/studio/transport?selection=title,%231",
            None,
        );
        assert_eq!(code, 200, "known keys sync: {state}");
        assert_eq!(
            state["selection"]["kept"],
            serde_json::json!(["title", "#1"])
        );
        assert_eq!(state["selection"]["dropped"], serde_json::json!([]));

        let (code, state) = json(
            port,
            "GET",
            "/api/studio/transport?selection=title,nope,%2399",
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
            "/api/studio/transport",
            Some("{\"action\":\"step-forward\",\"selection\":[\"bye\",\"ghost\"]}"),
        );
        assert_eq!(code, 200, "echoed selection syncs: {state}");
        assert_eq!(state["selection"]["kept"], serde_json::json!(["bye"]));
        assert_eq!(state["selection"]["dropped"], serde_json::json!(["ghost"]));
        assert_eq!(state["frame"], 1, "the strip move still applies");
    });
}

#[test]
fn transport_page_hosts_the_strip_with_hyphenated_styles() {
    with_server("page", text_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        for id in [
            "canvas-transport-bar",
            "transport-j",
            "transport-k",
            "transport-l",
            "transport-step-back",
            "transport-step-forward",
            "transport-play",
            "transport-pause",
            "transport-speed",
            "transport-seek",
            "transport-shuttle",
            "transport-health",
            "transport-hold",
        ] {
            assert!(page.contains(id), "the page hosts {id}");
        }
        for style in ["background-color", "border-radius", "font-variant-numeric"] {
            assert!(
                page.contains(style),
                "styles stay CSS-like hyphenated: {style}"
            );
        }
        for rate in ["0.25", "0.5", "0.75", "1.5", "2"] {
            assert!(page.contains(rate), "the speed menu offers {rate}x");
        }
        assert!(
            !page.contains("transport_seek"),
            "no snake_case strip keys leak"
        );
        assert!(
            !page.contains("still_secs"),
            "no snake_case health keys leak"
        );
    });
}

#[test]
fn transport_font_try_order_passes_through_untouched() {
    // A live server cannot boot a project whose families stay
    // fail-closed at draw time, so the pass-through rides the same
    // read-only SDK probe the canvas view uses: the transport route
    // serves these exact lists without resolving or reordering them.
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

#[test]
fn transport_read_probe_writes_nothing() {
    // A GET-only server run leaves the project untouched: no hidden
    // directory, no turns, sources intact.
    with_server("read-only", text_project, |folder, _reg, _entry, port| {
        let (code, state) = json(port, "GET", "/api/studio/transport", None);
        assert_eq!(code, 200);
        assert_eq!(state["font-families"], serde_json::json!([]));
        assert!(
            hidden_files(folder).is_empty(),
            "a read-only transport probe writes nothing"
        );
        assert!(!kdl_bytes(folder).is_empty());
        assert_eq!(turn_count(folder), 0, "no probe records a turn");
    });
}
