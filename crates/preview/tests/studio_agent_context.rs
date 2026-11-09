//! Selection context and agent handoff regressions (Sprint 20, T007 /
//! CAP-008, J-04, INT-003): preview and Studio context readouts name
//! the composition, the time, and the selection; ask-agent and
//! copy-to-agent hand the maker a ready request from the current
//! selection; agent edits land bracketed by agent name in shared
//! history and the maker undoes them like any turn; no selection reads
//! explicitly empty; every readout is read-only and default renders
//! stay byte-identical.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use preview::studio_agent::dispatch_agent_tool;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-agent-context-{}-{}-{}",
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

/// One text clip (`title`, 0..2s) plus one box (`card`, 2..4s), 10 fps
/// over 4 seconds, for the library-level history tests.
fn mixed_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" start=2 duration=2\n}\n",
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

#[test]
fn context_names_composition_time_and_selection() {
    with_server("context", text_project, |_folder, _reg, _entry, port| {
        // Park the preview playhead at frame 10 (1s at 10fps) so the
        // readout names real preview time.
        let (code, _) = body_str(port, "POST", "/api/scrub?i=10", None);
        assert_eq!(code, 200);
        let (code, out) = json(port, "GET", "/api/studio/context?selection=title", None);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["context"], true);
        assert_eq!(out["composition"], "t");
        assert_eq!(out["frame"], 10);
        assert_eq!(out["time-secs"], 1.0);
        assert_eq!(out["duration-secs"], 4.0);
        assert_eq!(out["fps"], 10.0);
        assert_eq!(out["selection"], serde_json::json!(["title"]));
        let clips = out["clips"].as_array().unwrap();
        assert_eq!(clips.len(), 1, "{out}");
        assert_eq!(clips[0]["key"], "title");
        assert_eq!(clips[0]["id"], "title");
        assert_eq!(clips[0]["window"], serde_json::json!([0.0, 2.0]));
        let note = out["selection-note"].as_str().unwrap();
        assert!(note.contains("title"), "names the pick: {note}");

        // A `#index` entry resolves against document order too.
        let (code, out) = json(port, "GET", "/api/studio/context?selection=%231", None);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["selection"], serde_json::json!(["#1"]));
        assert_eq!(out["clips"].as_array().unwrap()[0]["key"], "#1");
    });
}

#[test]
fn context_reads_empty_selection_explicitly() {
    with_server("empty", text_project, |_folder, _reg, _entry, port| {
        let (code, out) = json(port, "GET", "/api/studio/context", None);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["composition"], "t");
        assert_eq!(out["selection"], serde_json::json!([]));
        assert_eq!(out["clips"], serde_json::json!([]));
        let note = out["selection-note"].as_str().unwrap();
        assert!(note.contains("empty"), "explicit empty: {note}");
        // The empty readout never expands to the whole timeline.
        let text = serde_json::to_string(&out).unwrap();
        assert!(!text.contains("\"title\""), "no clip leaks in: {text}");
        assert!(!text.contains("\"bye\""), "no clip leaks in: {text}");
    });
}

#[test]
fn context_refuses_unknown_composition_and_selection() {
    with_server("refusals", text_project, |_folder, _reg, _entry, port| {
        let (code, text) = body_str(port, "GET", "/api/studio/context?composition=nope", None);
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("\"nope\""), "names the composition: {text}");
        assert!(text.contains("\"t\""), "lists the holdings: {text}");
        let (code, text) = body_str(port, "GET", "/api/studio/context?selection=ghost", None);
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("\"ghost\""), "names the entry: {text}");
        assert!(text.contains("title"), "names the holdings: {text}");
    });
}

#[test]
fn ask_agent_copies_a_ready_request_from_the_selection() {
    with_server("ask", text_project, |_folder, _reg, _entry, port| {
        let (code, out) = json(
            port,
            "GET",
            "/api/studio/ask-agent?selection=title%2C%231",
            None,
        );
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["handoff"], true);
        assert_eq!(out["kind"], "ask-agent");
        assert_eq!(out["composition"], "t");
        assert_eq!(out["selection"], serde_json::json!(["title", "#1"]));
        let request = out["request"].as_str().unwrap();
        assert!(request.contains("Ask the Studio agent"), "{request}");
        assert!(request.contains("composition \"t\""), "{request}");
        assert!(request.contains("title"), "{request}");
        assert!(request.contains("bye"), "{request}");
        assert!(request.contains("0.000s..2.000s"), "{request}");
        assert!(request.contains("2.000s..4.000s"), "{request}");
        assert!(request.contains("named history turn"), "{request}");

        // No selection refuses; unknown entries refuse naming the keys.
        let (code, text) = body_str(port, "GET", "/api/studio/ask-agent", None);
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("no selection"), "{text}");
        assert!(text.contains("ask-agent"), "{text}");
        let (code, text) = body_str(port, "GET", "/api/studio/ask-agent?selection=nope", None);
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("\"nope\""), "{text}");
    });
}

#[test]
fn copy_agent_hands_a_clipboard_handoff_from_the_selection() {
    with_server("copy", text_project, |_folder, _reg, _entry, port| {
        let (code, out) = json(port, "GET", "/api/studio/copy-agent?selection=bye", None);
        assert_eq!(code, 200, "{out}");
        assert_eq!(out["handoff"], true);
        assert_eq!(out["kind"], "copy-to-agent");
        assert_eq!(out["composition"], "t");
        assert_eq!(out["selection"], serde_json::json!(["bye"]));
        let request = out["request"].as_str().unwrap();
        assert!(request.contains("Studio agent handoff"), "{request}");
        assert!(request.contains("composition \"t\""), "{request}");
        assert!(request.contains("bye"), "{request}");
        assert!(request.contains("2.000s..4.000s"), "{request}");
        assert!(request.contains("named history turn"), "{request}");

        let (code, text) = body_str(port, "GET", "/api/studio/copy-agent", None);
        assert_eq!(code, 400, "{text}");
        assert!(text.contains("no selection"), "{text}");
        assert!(text.contains("copy-to-agent"), "{text}");
        let (code, text) = body_str(port, "GET", "/api/studio/copy-agent?selection=ghost", None);
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("\"ghost\""), "{text}");
    });
}

#[test]
fn ask_and_copy_handoffs_voice_the_same_selection_differently() {
    with_server("voices", text_project, |_folder, _reg, _entry, port| {
        let (code, ask) = json(port, "GET", "/api/studio/ask-agent?selection=title", None);
        assert_eq!(code, 200, "{ask}");
        let (code, copy) = json(port, "GET", "/api/studio/copy-agent?selection=title", None);
        assert_eq!(code, 200, "{copy}");
        let ask_text = ask["request"].as_str().unwrap();
        let copy_text = copy["request"].as_str().unwrap();
        assert_ne!(ask_text, copy_text, "two handoff voices");
        assert!(ask_text.contains("title") && copy_text.contains("title"));
    });
}

#[test]
fn context_and_handoffs_stage_nothing_and_frames_stay_identical() {
    with_server("readonly", text_project, |folder, _reg, _entry, port| {
        let sources_before = std::fs::read(folder.join("composition.kdl")).unwrap();
        let turns_before = turn_count(folder);
        let (_, frame_before) = http(port, "GET", "/api/frame?i=10&action=step", None);
        assert!(!frame_before.is_empty());

        // Every read: context with and without selection, both
        // handoffs, plus the refusal paths.
        for target in [
            "/api/studio/context",
            "/api/studio/context?selection=title%2Cbye",
            "/api/studio/ask-agent?selection=title",
            "/api/studio/copy-agent?selection=bye",
            "/api/studio/ask-agent",
            "/api/studio/copy-agent?selection=ghost",
            "/api/studio/context?composition=nope",
        ] {
            let _ = http(port, "GET", target, None);
        }

        assert_eq!(
            std::fs::read(folder.join("composition.kdl")).unwrap(),
            sources_before,
            "sources stay byte-identical"
        );
        assert_eq!(turn_count(folder), turns_before, "no turn recorded");
        let (_, frame_after) = http(port, "GET", "/api/frame?i=10&action=step", None);
        assert_eq!(frame_before, frame_after, "frames stay byte-identical");
    });
}

/// Dispatch one JSON tool body against the folder; panics on refusal
/// with the message so failures read.
fn run(folder: &Path, body: &str) -> String {
    let call = preview::studio_agent::parse_agent_body(body).expect("body parses");
    dispatch_agent_tool(folder, &call).expect("tool applies")
}

fn history_turns(folder: &Path) -> Vec<(u64, String, String, bool)> {
    sdk::history::list_turns(folder)
        .expect("history lists")
        .turns
        .into_iter()
        .map(|t| (t.seq, t.actor, t.summary, t.undone))
        .collect()
}

#[test]
fn agent_turns_bracket_by_agent_name_and_the_maker_undoes_them() {
    let root = tmp("brackets");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    mixed_project(&folder);

    // The maker restyles the card first, then the agent (under its own
    // name) rewords the title: two turns from two writers.
    run(
        &folder,
        "{\"tool\":\"set-style\",\"actor\":\"maker\",\"target-id\":\"card\",\"property\":\"background-color\",\"value\":\"#ff0000\"}",
    );
    run(
        &folder,
        "{\"tool\":\"set-text\",\"actor\":\"agent-iris\",\"target-id\":\"title\",\"value\":\"Changed\"}",
    );
    let turns = history_turns(&folder);
    assert_eq!(turns.len(), 2, "both writers recorded");
    assert_eq!(turns[0].1, "maker", "human turn keeps its name");
    assert_eq!(turns[1].1, "agent-iris", "agent turn bracketed by name");
    assert!(turns.iter().all(|t| !t.3), "both still in effect");

    // The maker undoes the agent turn by agent name from history: the
    // title restores while the maker turn stays in effect.
    let undone =
        sdk::history::undo_who(&folder, "agent-iris").expect("maker undoes the agent turn");
    assert_eq!(undone.actor, "agent-iris", "undo names the agent turn");
    let turns = history_turns(&folder);
    assert!(!turns[0].3, "maker turn still in effect");
    assert!(turns[1].3, "agent turn undone");
    let session = sdk::SdkSession::open(
        &folder,
        sdk::OpenOptions {
            preview: true,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("project reopens");
    let value = match &session.effective_composition().clips[0] {
        notation::model::Clip::Text(c) => c.value.clone(),
        other => panic!("expected text, found {}", other.what()),
    };
    assert_eq!(value, "Hi", "undo restores the prior text");

    // With no agent turn still in effect the named undo refuses, and
    // every turn stays listed: human turns are never hidden.
    let err = sdk::history::undo_who(&folder, "agent-iris").expect_err("nothing left to undo");
    assert!(err.to_string().contains("agent-iris"), "names the agent");
    assert_eq!(history_turns(&folder).len(), 2, "turns stay listed");
}
