//! Inline text editing session regressions (Sprint 17, T006 /
//! CAP-006, J-03, STATE-03, REC-001, REC-002): double-click opens the
//! live session with bold, italic, underline, and colour; typing
//! previews without staging; Enter commits as exactly one patch
//! turn and Escape cancels with nothing staged; an invalid value
//! (for example an unknown font family) refuses with its reason
//! keeping the prior text; offered fonts always resolve. Only
//! validated turns write the project (plus hidden studio state
//! files); default renders stay byte-identical.

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
        "framesmith-studio-text-{}-{}-{}",
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

/// One box, two texts (`title` regular white, `note` bold red), and
/// one hidden text (`ghost`): 10 fps, 40 frames, 320x180.
fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" order=2 start=0 duration=4\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=150 size=24 color=\"#ffffff\"\n    text id=\"note\" value=\"Yo\" font-style=\"bold\" color=\"#ff0000\" font-family=\"serif\" start=0 duration=4 x=10 y=100 size=16\n    text id=\"ghost\" value=\"Boo\" hidden=true start=0 duration=4 x=10 y=50 size=12\n}\n",
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
    std::fs::read(folder.join("composition.kdl")).expect("sources stay readable")
}

/// Every file under the project folder with its bytes, so a
/// refused, previewed, or cancelled session can prove it staged
/// nothing anywhere.
fn snapshot(folder: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                let rel = path.strip_prefix(folder).unwrap().to_path_buf();
                out.push((rel, bytes));
            }
        }
    }
    out.sort();
    out
}

fn live_value(folder: &Path, id: &str) -> (String, String, String) {
    let shown =
        preview::studio::effective_composition_for_session(folder, None).expect("effective opens");
    let clip = shown
        .clips
        .iter()
        .find(|c| c.id() == Some(id))
        .expect("clip present");
    match clip {
        notation::model::Clip::Text(c) => (
            c.value.clone(),
            match c.style {
                notation::model::FontStyle::Regular => "regular".to_string(),
                notation::model::FontStyle::Bold => "bold".to_string(),
                notation::model::FontStyle::Italic => "italic".to_string(),
                notation::model::FontStyle::BoldItalic => "bold-italic".to_string(),
            },
            format!("#{:02x}{:02x}{:02x}", c.color.r, c.color.g, c.color.b),
        ),
        _ => panic!("text expected"),
    }
}

#[test]
fn text_open_previews_and_commit_records_one_turn() {
    with_server("session", text_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        // Double-click opens the live session with current styles.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"open","target":{"id":"title"},"selection":"title"}"##),
        );
        assert_eq!(code, 200, "open reads the session: {j}");
        assert_eq!(j.get("action").unwrap(), "open");
        assert_eq!(j.get("value").unwrap(), "Hi");
        assert_eq!(j.get("bold").unwrap(), &serde_json::Value::Bool(false));
        assert_eq!(j.get("font-style").unwrap(), "regular");
        assert_eq!(j.get("color").unwrap(), "#ffffff");
        assert_eq!(j.get("underline").unwrap(), &serde_json::Value::Bool(false));
        assert!(
            j.get("offered-fonts").unwrap().is_array(),
            "offers fonts: {j}"
        );
        assert_eq!(
            j.get("selection").unwrap().get("kept").unwrap(),
            &serde_json::json!(["title"]),
            "selection stays in sync: {j}"
        );
        // Typing with every style previews live without staging.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(
                r##"{"action":"preview","target":{"id":"title"},"value":"Hello","bold":true,"italic":true,"underline":true,"color":"#ff0000"}"##,
            ),
        );
        assert_eq!(code, 200, "preview echoes the keystrokes: {j}");
        assert_eq!(j.get("value").unwrap(), "Hello");
        assert_eq!(j.get("font-style").unwrap(), "bold-italic");
        assert_eq!(j.get("color").unwrap(), "#ff0000");
        assert_eq!(
            j.get("underline").unwrap(),
            &serde_json::Value::Bool(true),
            "underline applies in-session: {j}"
        );
        assert_eq!(j.get("applied").unwrap(), &serde_json::json!(3));
        let notes = j.get("notes").unwrap().as_array().unwrap();
        assert_eq!(notes.len(), 1, "underline previews with a note: {j}");
        assert_eq!(snapshot(folder), before, "previews stage nothing anywhere");
        // Enter commits exactly one patch turn.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(
                r##"{"action":"commit","actor":"maker","target":{"id":"title"},"value":"Hello","bold":true,"italic":true,"underline":true,"color":"#ff0000"}"##,
            ),
        );
        assert_eq!(code, 200, "commit records: {j}");
        assert_eq!(j.get("saved").unwrap(), &serde_json::Value::Bool(true));
        assert_eq!(j.get("committed").unwrap(), &serde_json::Value::Bool(true));
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        assert_eq!(j.get("applied").unwrap(), &serde_json::json!(3));
        let summaries = j.get("summaries").unwrap().as_array().unwrap();
        assert_eq!(summaries.len(), 3, "one row op per change: {summaries:?}");
        let detail = j.get("detail").unwrap().as_str().unwrap();
        assert!(detail.contains("turn 0"), "turn named: {detail}");
        // The effective composition carries the committed text.
        assert_eq!(
            live_value(folder, "title"),
            (
                "Hello".to_string(),
                "bold-italic".to_string(),
                "#ff0000".to_string()
            ),
            "commit lands through the shared machinery"
        );
        // A second commit records the next turn.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(
                r##"{"action":"commit","actor":"maker","target":{"id":"title"},"color":"#00ff00"}"##,
            ),
        );
        assert_eq!(code, 200, "recolor commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(1));
        assert_eq!(live_value(folder, "title").2, "#00ff00".to_string());
    });
}

#[test]
fn text_cancel_discards_the_keystrokes_with_nothing_staged() {
    with_server("cancel", text_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"preview","target":{"id":"title"},"value":"Draft","bold":true}"##),
        );
        assert_eq!(code, 200, "preview stages the echo: {j}");
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"cancel","target":{"id":"title"}}"##),
        );
        assert_eq!(code, 200, "Escape cancels: {j}");
        assert_eq!(j.get("cancelled").unwrap(), &serde_json::Value::Bool(true));
        assert_eq!(j.get("committed").unwrap(), &serde_json::Value::Bool(false));
        assert_eq!(snapshot(folder), before, "cancel stages nothing anywhere");
        assert_eq!(
            live_value(folder, "title").0,
            "Hi".to_string(),
            "prior text stays"
        );
        // No turn recorded: the next commit is turn 0.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"commit","actor":"maker","target":{"id":"title"},"value":"Kept"}"##),
        );
        assert_eq!(code, 200, "commit after cancel: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
    });
}

#[test]
fn text_unknown_font_family_refuses_keeping_prior_text() {
    with_server("fonts", text_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        // The session offers fonts; recording one must succeed, so
        // the refusal below is about the unknown family only.
        let (code, open) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"open","target":{"id":"title"}}"##),
        );
        assert_eq!(code, 200, "open offers fonts: {open}");
        let offered = open
            .get("offered-fonts")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>();
        assert!(
            offered.iter().any(|f| f == "serif"),
            "the authored resolving family is offered: {offered:?}"
        );
        // An unknown family refuses on preview and on commit.
        for action in ["preview", "commit"] {
            let (code, text) = body_str(
                port,
                "POST",
                "/api/studio/text",
                Some(&format!(
                    r##"{{"action":"{action}","actor":"maker","target":{{"id":"title"}},"font-family":"Nope, Missing Too"}}"##
                )),
            );
            assert_eq!(code, 409, "{action} refuses the unknown family: {text}");
            assert!(
                text.contains("resolves to no known family"),
                "the refusal names the reason: {text}"
            );
            assert!(
                text.contains("Nothing was staged"),
                "refusals stage nothing: {text}"
            );
        }
        assert_eq!(snapshot(folder), before, "refusals stage nothing anywhere");
        assert_eq!(
            live_value(folder, "title").0,
            "Hi".to_string(),
            "prior text stays"
        );
        // Every offered family resolves: committing one succeeds.
        if let Some(first) = offered.first() {
            let (code, j) = json(
                port,
                "POST",
                "/api/studio/text",
                Some(&format!(
                    r##"{{"action":"commit","actor":"maker","target":{{"id":"title"}},"font-family":{first:?}}}"##
                )),
            );
            assert_eq!(code, 200, "offered family {first:?} commits: {j}");
            // Try-order passes through untouched: an unknown head
            // with a resolvable tail commits in written order.
            let (code, j) = json(
                port,
                "POST",
                "/api/studio/text",
                Some(&format!(
                    r##"{{"action":"commit","actor":"maker","target":{{"id":"title"}},"font-family":"Nope, {first}"}}"##
                )),
            );
            assert_eq!(code, 200, "mixed try-order passes through: {j}");
            let shown = preview::studio::effective_composition_for_session(folder, None)
                .expect("effective opens");
            assert_eq!(
                preview::studio_canvas::canvas_font_lists_of(&shown)
                    .iter()
                    .find(|l| l.contains("Nope"))
                    .expect("mixed list rides along"),
                &format!("Nope, {first}"),
                "try-order commits in written comma order"
            );
        }
    });
}

#[test]
fn text_invalid_values_refuse_with_reasons() {
    with_server("refusals", text_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        // A colour outside #rgb/#rrggbb refuses.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"preview","target":{"id":"title"},"color":"red"}"##),
        );
        assert_eq!(code, 409, "bad colour refuses: {text}");
        assert!(text.contains("`color`"), "names the row: {text}");
        // An empty value on a span-less clip refuses.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"preview","target":{"id":"title"},"value":""}"##),
        );
        assert_eq!(code, 409, "empty value refuses: {text}");
        assert!(text.contains("`value`"), "names the row: {text}");
        // Sessions open on text clips only.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"open","target":{"id":"card"}}"##),
        );
        assert_eq!(code, 409, "box target refuses: {text}");
        assert!(
            text.contains("text clips only"),
            "names the kind rule: {text}"
        );
        // Unknown targets name the holdings.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"open","target":{"id":"nobody"}}"##),
        );
        assert_eq!(code, 404, "unknown id refuses: {text}");
        assert!(text.contains("\"nobody\""), "names the key: {text}");
        // Hidden text shows no session.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"open","target":{"id":"ghost"}}"##),
        );
        assert_eq!(code, 409, "hidden text refuses: {text}");
        assert!(text.contains("`visible`"), "names the row back: {text}");
        // An unchanged session records no turn.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"commit","actor":"maker","target":{"id":"title"}}"##),
        );
        assert_eq!(code, 409, "empty commit refuses: {text}");
        assert!(
            text.contains("changed nothing"),
            "names the clean state: {text}"
        );
        // Commits are named: no actor, no turn.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"commit","target":{"id":"title"},"value":"X"}"##),
        );
        assert_eq!(code, 400, "actorless commit refuses: {text}");
        assert!(text.contains("no actor"), "names the actor: {text}");
        // Unknown actions and mistyped toggles refuse up front.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"type","target":{"id":"title"}}"##),
        );
        assert_eq!(code, 400, "unknown action refuses: {text}");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"preview","target":{"id":"title"},"bold":"yes"}"##),
        );
        assert_eq!(code, 400, "mistyped toggle refuses: {text}");
        assert!(text.contains("`bold`"), "names the toggle: {text}");
        assert_eq!(snapshot(folder), before, "refusals stage nothing anywhere");
        assert_eq!(
            live_value(folder, "title").0,
            "Hi".to_string(),
            "prior text stays through every refusal"
        );
    });
}

#[test]
fn text_page_hosts_the_session_panel_with_hyphenated_styles() {
    with_server("page", text_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200, "studio serves");
        for needle in [
            "canvas-text-session",
            "canvas-text-edit",
            "canvas-text-bold",
            "canvas-text-underline",
            "canvas-text-status",
            "text-decoration",
            "background-color",
            "font-weight",
        ] {
            assert!(page.contains(needle), "page carries {needle}");
        }
        assert!(
            !page.contains("font_family") || page.contains("font-family"),
            "style attributes stay hyphenated"
        );
    });
}

#[test]
fn text_sessions_keep_default_renders_byte_identical() {
    with_server("frames", text_project, |folder, _reg, _entry, port| {
        let sources = kdl_bytes(folder);
        let (before_code, before_png) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(before_code, 200, "frame serves before the session");
        assert!(!before_png.is_empty());
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"open","target":{"id":"title"}}"##),
        );
        assert_eq!(code, 200);
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(
                r##"{"action":"preview","target":{"id":"title"},"value":"Draft","bold":true,"underline":true,"color":"#00ff00"}"##,
            ),
        );
        assert_eq!(code, 200);
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/text",
            Some(r##"{"action":"cancel","target":{"id":"title"}}"##),
        );
        assert_eq!(code, 200);
        assert_eq!(kdl_bytes(folder), sources, "sources stay byte-identical");
        let (after_code, after_png) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(after_code, 200, "frame serves after the session");
        assert_eq!(
            before_png, after_png,
            "open, preview, and cancel change no frame bytes"
        );
    });
}
