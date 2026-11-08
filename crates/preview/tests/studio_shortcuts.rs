//! Remappable shortcut regressions (Sprint 16, T008 / CAP-009, J-04,
//! INT-003, REC-004): the full default map drives playback, the work
//! area, tools, edit, and panels; a project remap overrides single keys
//! and chords alike with immediate effect and the defaults visible
//! underneath; describe-a-change copies a ready agent request from the
//! selection; an unparseable remap falls back to defaults naming the bad
//! entry; a colliding chord keeps the old binding and reports the
//! collision. Every visual timeline edit a shortcut triggers lands as a
//! named history turn through the SDK patch and history machinery.

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
        "framesmith-studio-shortcuts-{}-{}-{}",
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

/// Two text clips on the main track: `title` (0..2s) and `bye` (2..4s),
/// 10 fps, 40 frames.
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
    let value: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|_| panic!("not JSON from {method} {target}: {text}"));
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

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .map(|l| l.turns.len())
        .unwrap_or(usize::MAX)
}

fn rows_clip_count(body: &serde_json::Value) -> usize {
    body.get("effect")
        .and_then(|e| e.get("rows"))
        .and_then(|r| r.get("clips"))
        .and_then(|c| c.as_array())
        .map(|a| a.len())
        .expect("effect.rows.clips")
}

#[test]
fn default_map_covers_all_five_areas() {
    use preview::{default_shortcuts, normalize_chord, ShortcutAction};
    let map = default_shortcuts();
    // Every action has exactly one default chord.
    assert_eq!(map.len(), ShortcutAction::ALL_ACTIONS.len());
    let mut actions: Vec<&str> = map.iter().map(|b| b.action.as_str()).collect();
    actions.sort();
    let mut want: Vec<&str> = ShortcutAction::ALL_ACTIONS.to_vec();
    want.sort();
    assert_eq!(actions, want);
    // Every area is covered.
    for area in ["playback", "work-area", "tools", "edit", "panels"] {
        assert!(
            map.iter().any(|b| b.area().as_str() == area),
            "area {area} has no default shortcut"
        );
    }
    // No two chords alike, and every default chord is already canonical.
    let mut chords: Vec<&str> = map.iter().map(|b| b.chord.as_str()).collect();
    chords.sort();
    chords.dedup();
    assert_eq!(chords.len(), map.len(), "default chords collide");
    for b in &map {
        assert_eq!(normalize_chord(&b.chord).unwrap(), b.chord);
    }
}

#[test]
fn chord_normalization_orders_modifiers_and_rejects_junk() {
    use preview::normalize_chord;
    assert_eq!(normalize_chord("space").unwrap(), "space");
    assert_eq!(normalize_chord("  SPACE ").unwrap(), "space");
    assert_eq!(normalize_chord("Shift+CTRL+x").unwrap(), "ctrl+shift+x");
    assert_eq!(normalize_chord("ctrl+k").unwrap(), "ctrl+k");
    assert_eq!(normalize_chord("g g").unwrap(), "g g");
    assert!(normalize_chord("").is_err());
    assert!(normalize_chord("   ").is_err());
    assert!(normalize_chord("ctrl").is_err());
    assert!(normalize_chord("ctrl++x").is_err());
    assert!(normalize_chord("hyper+z").is_err());
}

fn dispatch(port: u16, payload: &str) -> (u16, serde_json::Value) {
    json(port, "POST", "/api/studio/shortcut", Some(payload))
}

#[test]
fn every_default_shortcut_acts_in_its_area() {
    with_server(
        "dispatch-all",
        text_project,
        |_folder, _reg, _entry, port| {
            // Playback: play toggles on, steps move, ends park, loop and
            // speed flip through the readout.
            let (code, out) = dispatch(port, r#"{"chord":"space"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["action"], "play-toggle");
            assert_eq!(out["area"], "playback");
            assert_eq!(out["effect"]["playing"], true);
            let (code, out) = dispatch(port, r#"{"action":"play-toggle"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["playing"], false);

            let (code, out) = dispatch(port, r#"{"chord":"arrow-right"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["readout"]["frame"], 1);
            let (code, out) = dispatch(port, r#"{"chord":"arrow-left"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["readout"]["frame"], 0);

            let (code, out) = dispatch(port, r#"{"chord":"end"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["readout"]["frame"], 39);
            let (code, out) = dispatch(port, r#"{"chord":"home"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["readout"]["frame"], 0);

            let (code, out) = dispatch(port, r#"{"chord":"l"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["loop"], true);
            let (code, out) = dispatch(port, r#"{"chord":"l"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["loop"], false);

            let (code, out) = dispatch(port, r#"{"chord":"="}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["speed"], 1.25);
            let (code, out) = dispatch(port, r#"{"chord":"-"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["speed"], 1.0);

            // Work area: in and out land at the playhead, clear empties.
            let (code, _) = body_str(port, "POST", "/api/scrub?i=10", None);
            assert_eq!(code, 200);
            let (code, out) = dispatch(port, r#"{"chord":"i"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["work_area"]["in"], 1.0);
            let (code, _) = body_str(port, "POST", "/api/scrub?i=30", None);
            assert_eq!(code, 200);
            let (code, out) = dispatch(port, r#"{"chord":"o"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["work_area"]["out"], 3.0);
            let (code, out) = dispatch(port, r#"{"chord":"shift+o"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["work_area"]["in"], serde_json::Value::Null);

            // Tools: mode switches answer as panel state; split, picks run
            // as named history turns through the SDK machinery.
            let (code, out) = dispatch(port, r#"{"chord":"v"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["panel_tool"], "select");
            let (code, out) = dispatch(port, r#"{"chord":"b"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["panel_tool"], "razor");

            let (code, _) = body_str(port, "POST", "/api/scrub?i=10", None);
            assert_eq!(code, 200);
            let (code, out) = dispatch(port, r#"{"chord":"s"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(rows_clip_count(&out), 3);
            assert!(out["effect"]["turn"].as_u64().is_some());

            let (code, out) = dispatch(port, r#"{"chord":"shift+arrow-left"}"#);
            assert_eq!(code, 200, "{out}");
            assert!(!out["effect"]["selection"].as_array().unwrap().is_empty());
            let (code, out) = dispatch(port, r#"{"chord":"shift+arrow-right"}"#);
            assert_eq!(code, 200, "{out}");
            assert!(!out["effect"]["selection"].as_array().unwrap().is_empty());

            // Edit: the handoff copies from the selection, undo reverses.
            let (code, out) =
                dispatch(port, r##"{"action":"describe-change","selection":["#0"]}"##);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["area"], "edit");
            assert!(out["effect"]["request"]
                .as_str()
                .unwrap()
                .contains("history turn"));

            // Panels: theme flips, layout resets, focus acks.
            let (code, out) = dispatch(port, r#"{"chord":"t"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["theme"], "dark");
            let (code, out) = dispatch(port, r#"{"chord":"t"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["theme"], "light");
            let (code, out) = dispatch(port, r#"{"chord":"ctrl+0"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["reset"], true);
            let (code, out) = dispatch(port, r#"{"chord":"1"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["focus"], "preview");
            let (code, out) = dispatch(port, r#"{"chord":"2"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["effect"]["focus"], "timeline");

            // Unknown chords and actions refuse naming the map.
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/shortcut",
                Some(r#"{"chord":"ctrl+q"}"#),
            );
            assert_eq!(code, 400, "{text}");
            assert!(text.contains("drives nothing"), "{text}");
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/shortcut",
                Some(r#"{"action":"nope"}"#),
            );
            assert_eq!(code, 400, "{text}");
            assert!(text.contains("unknown shortcut action"), "{text}");
        },
    );
}

#[test]
fn project_remap_overrides_key_and_chord_immediately() {
    with_server("remap", text_project, |folder, _reg, _entry, port| {
        // Baseline: the defaults drive.
        let (code, out) = dispatch(port, r#"{"chord":"b"}"#);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["action"], "tool-razor");

        // A project remap rebinds one single key and one chord alike.
        let (code, out) = json(
            port,
            "POST",
            "/api/studio/shortcuts",
            Some(r#"{"version":1,"bindings":{"tool-razor":"x","split-at-playhead":"ctrl+k"}}"#),
        );
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["restored"], true);

        // The new bindings act at once, case-insensitively.
        let (code, out) = dispatch(port, r#"{"chord":"X"}"#);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["action"], "tool-razor");
        let (code, _) = body_str(port, "POST", "/api/scrub?i=10", None);
        assert_eq!(code, 200);
        let (code, out) = dispatch(port, r#"{"chord":"ctrl+k"}"#);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["action"], "split-at-playhead");
        assert_eq!(rows_clip_count(&out), 3);
        // The old chord is freed: `s` now drives nothing.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/shortcut",
            Some(r#"{"chord":"s"}"#),
        );
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("drives nothing"), "{text}");

        // The default map stays visible underneath the remap.
        let (code, out) = json(port, "GET", "/api/studio/shortcuts", None);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["restored"], true);
        let defaults = out["defaults"].as_array().unwrap();
        assert_eq!(defaults.len(), 22);
        assert!(defaults
            .iter()
            .any(|b| b["chord"] == "b" && b["action"] == "tool-razor"));
        let bindings = out["bindings"].as_array().unwrap();
        assert!(bindings
            .iter()
            .any(|b| b["chord"] == "x" && b["action"] == "tool-razor"));
        assert!(bindings
            .iter()
            .any(|b| b["chord"] == "ctrl+k" && b["action"] == "split-at-playhead"));

        // The remap lives in the hidden history directory, never beside
        // the sources.
        let remap =
            std::fs::read_to_string(folder.join(".framesmith").join("studio-shortcuts.json"))
                .expect("remap file");
        assert!(remap.contains("tool-razor"), "{remap}");
    });
}

#[test]
fn unparseable_remap_falls_back_naming_the_bad_entry() {
    with_server("remap-bad", text_project, |folder, _reg, _entry, port| {
        // Garbage JSON falls back to defaults naming the failure.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/shortcuts",
            Some("this is not json"),
        );
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("not JSON"), "{text}");

        // An unknown action names the bad entry.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/shortcuts",
            Some(r#"{"version":1,"bindings":{"playyy":"x"}}"#),
        );
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("bindings.playyy"), "{text}");
        assert!(text.contains("unknown action"), "{text}");

        // A bad chord names the bad entry.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/shortcuts",
            Some(r#"{"version":1,"bindings":{"tool-razor":"ctrl"}}"#),
        );
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("bindings.tool-razor"), "{text}");

        // Nothing was saved: the defaults still drive and the file is
        // untouched (missing).
        let (code, out) = dispatch(port, r#"{"chord":"b"}"#);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["action"], "tool-razor");
        assert!(
            !folder
                .join(".framesmith")
                .join("studio-shortcuts.json")
                .exists(),
            "refused remap wrote a file"
        );
        assert_eq!(turn_count(folder), 0);

        // A hand-written broken file falls back the same way, naming
        // the bad entry on reads.
        std::fs::create_dir_all(folder.join(".framesmith")).unwrap();
        write(
            folder,
            ".framesmith/studio-shortcuts.json",
            r#"{"version":1,"bindings":{"tool-razor":"ctrl"}}"#,
        );
        let (code, out) = json(port, "GET", "/api/studio/shortcuts", None);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["restored"], false);
        assert!(out["notice"]
            .as_str()
            .unwrap()
            .contains("bindings.tool-razor"));
        let (code, out) = dispatch(port, r#"{"chord":"b"}"#);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["action"], "tool-razor");
    });
}

#[test]
fn colliding_chord_keeps_the_old_binding_and_reports() {
    with_server(
        "remap-collide",
        text_project,
        |folder, _reg, _entry, port| {
            // Stealing `v` (tool-select) for the razor collides: the POST
            // refuses, naming the collision and the kept binding.
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/shortcuts",
                Some(r#"{"version":1,"bindings":{"tool-razor":"v"}}"#),
            );
            assert_eq!(code, 409, "{text}");
            assert!(text.contains("\"v\""), "{text}");
            assert!(text.contains("tool-select"), "{text}");
            assert!(text.contains("Nothing was saved"), "{text}");

            // The old binding still drives; the razor keeps its default.
            let (code, out) = dispatch(port, r#"{"chord":"v"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["action"], "tool-select");
            let (code, out) = dispatch(port, r#"{"chord":"b"}"#);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["action"], "tool-razor");
            assert!(
                !folder
                    .join(".framesmith")
                    .join("studio-shortcuts.json")
                    .exists(),
                "colliding remap wrote a file"
            );

            // A hand-written colliding file resolves the same way: the old
            // binding holds and the collision is reported.
            std::fs::create_dir_all(folder.join(".framesmith")).unwrap();
            write(
                folder,
                ".framesmith/studio-shortcuts.json",
                r#"{"version":1,"bindings":{"tool-select":"x","tool-razor":"x"}}"#,
            );
            let (code, out) = json(port, "GET", "/api/studio/shortcuts", None);
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["restored"], true);
            assert_eq!(out["collisions"].as_array().unwrap().len(), 1);
            assert!(out["notice"].as_str().unwrap().contains("\"x\""));
            // Exactly one contender wins `x`; the loser keeps its default
            // chord, so both actions stay drivable with no silent steal.
            let (code, out) = dispatch(port, r#"{"chord":"x"}"#);
            assert_eq!(code, 200, "{out}");
            let winner = out["action"].as_str().unwrap().to_string();
            assert!(
                winner == "tool-select" || winner == "tool-razor",
                "{winner}"
            );
            let loser_default = if winner == "tool-select" { "b" } else { "v" };
            let loser = if winner == "tool-select" {
                "tool-razor"
            } else {
                "tool-select"
            };
            let (code, out) = dispatch(port, &format!(r#"{{"chord":"{loser_default}"}}"#));
            assert_eq!(code, 200, "{out}");
            assert_eq!(out["action"], loser);
        },
    );
}

#[test]
fn describe_copies_a_ready_agent_request_from_the_selection() {
    with_server("describe", text_project, |_folder, _reg, _entry, port| {
        // The handoff names the composition, playhead, and windows, and
        // ends ready for the SDK with a turn receipt.
        let (code, out) = json(
            port,
            "GET",
            "/api/studio/describe?selection=title%2C%231",
            None,
        );
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["composition"], "t");
        let request = out["request"].as_str().unwrap();
        assert!(request.contains("composition \"t\""), "{request}");
        assert!(request.contains("title"), "{request}");
        assert!(request.contains("bye"), "{request}");
        assert!(request.contains("0.000s..2.000s"), "{request}");
        assert!(request.contains("2.000s..4.000s"), "{request}");
        assert!(request.contains("named history turn"), "{request}");

        // No selection refuses; unknown entries refuse naming the keys.
        let (code, text) = body_str(port, "GET", "/api/studio/describe", None);
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("no selection"), "{text}");
        let (code, text) = body_str(port, "GET", "/api/studio/describe?selection=nope", None);
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("\"nope\""), "{text}");
        assert!(text.contains("timeline holds"), "{text}");
    });
}

#[test]
fn undo_reverses_a_shortcut_edit_with_byte_identical_frames() {
    with_server("undo-frames", text_project, |folder, _reg, _entry, port| {
        let (_, before) = http(port, "GET", "/api/frame?i=10&action=step", None);
        assert!(!before.is_empty());

        // Split through the shortcut, then undo through the shortcut:
        // one turn recorded, then reversed through shared history.
        let (code, _) = body_str(port, "POST", "/api/scrub?i=10", None);
        assert_eq!(code, 200);
        let (code, out) = dispatch(port, r#"{"chord":"s"}"#);
        assert_eq!(code, 200, "{out}");
        assert_eq!(rows_clip_count(&out), 3);
        assert_eq!(turn_count(folder), 1);

        let (code, out) = dispatch(port, r#"{"chord":"ctrl+z"}"#);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["action"], "undo");
        assert_eq!(out["effect"]["rows"]["clips"].as_array().unwrap().len(), 2);

        // The timeline and the frame bytes match the pre-edit state.
        let (code, out) = json(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["clips"].as_array().unwrap().len(), 2);
        let (_, after) = http(port, "GET", "/api/frame?i=10&action=step", None);
        assert_eq!(before, after);

        // Undo with no turn in effect refuses with the count.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/shortcut",
            Some(r#"{"chord":"ctrl+z"}"#),
        );
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("no turn still in effect"), "{text}");
    });
}

#[test]
fn shortcut_traffic_keeps_default_renders_and_project_hygiene() {
    with_server("hygiene", text_project, |folder, _reg, _entry, port| {
        let (_, before) = http(port, "GET", "/api/frame?i=5&action=step", None);
        assert!(!before.is_empty());

        // Remap saves, mode switches, handoffs, and focus moves change
        // no composition: frames stay byte-identical.
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/shortcuts",
            Some(r#"{"version":1,"bindings":{"tool-razor":"x"}}"#),
        );
        assert_eq!(code, 200);
        for payload in [
            r#"{"chord":"x"}"#,
            r#"{"chord":"l"}"#,
            r#"{"chord":"l"}"#,
            r#"{"action":"describe-change","selection":["title"]}"#,
            r#"{"chord":"1"}"#,
            r#"{"chord":"2"}"#,
        ] {
            let (code, _) = dispatch(port, payload);
            assert_eq!(code, 200, "{payload}");
        }
        let (_, after) = http(port, "GET", "/api/frame?i=5&action=step", None);
        assert_eq!(before, after);

        // Only validated project writes exist: the composition plus the
        // hidden history directory (hidden SDK deltas, turns, remaps).
        // Render scratch and caches stay outside the project.
        let mut stray = Vec::new();
        let mut stack = vec![folder.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    if dir == folder && path.file_name().unwrap() != ".framesmith" {
                        stray.push(path);
                    } else {
                        stack.push(path);
                    }
                } else if dir == folder && path.file_name().unwrap() != "composition.kdl" {
                    stray.push(path);
                }
            }
        }
        assert!(stray.is_empty(), "stray project files: {stray:?}");
    });
}

#[test]
fn studio_page_shows_the_shortcut_surface() {
    with_server("page", text_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        for needle in [
            r#"id="shortcuts""#,
            r#"id="describe-copy""#,
            r#"id="shortcut-status""#,
            "/api/studio/shortcuts",
            "/api/studio/shortcut",
            "/api/studio/describe",
            "paintShortcuts",
            "copy agent request",
        ] {
            assert!(page.contains(needle), "page misses {needle}");
        }
    });
}
