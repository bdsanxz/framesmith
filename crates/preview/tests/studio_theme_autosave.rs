//! Studio theme, autosave, and banners (Sprint 16, T002 / CAP-002,
//! J-01, STATE-02, REC-003): light and dark themes paint shell and
//! panels together per project; visual edits save immediately and code
//! saves debounced with no save button, every save landing as a
//! validated turn through the SDK patch and history machinery; a
//! paused queue banners and keeps every edit staged; an unreachable
//! project opens the shell read-only.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use preview::studio;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-t002-{}-{}-{}",
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

fn text_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text value=\"Bye\" start=1 duration=1 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
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
        .expect("history lists")
        .turns
        .len()
}

fn label_edit(actor: &str, source: &str, label: &str) -> String {
    format!("{{\"actor\":\"{actor}\",\"source\":\"{source}\",\"edits\":[{{\"op\":\"setlabel\",\"target\":{{\"id\":\"title\",\"index\":null}},\"label\":\"{label}\"}}]}}")
}

fn move_edit(actor: &str, source: &str) -> String {
    format!("{{\"actor\":\"{actor}\",\"source\":\"{source}\",\"edits\":[{{\"op\":\"moveelement\",\"target\":{{\"id\":\"title\",\"index\":null}},\"to\":1}}]}}")
}

#[test]
fn studio_theme_defaults_light_and_switches_per_project() {
    with_server("theme", text_project, |folder, _reg, _entry, port| {
        let (code, state) = body_str(port, "GET", "/api/studio/theme", None);
        assert_eq!(code, 200, "{state}");
        assert!(state.contains("\"theme\":\"light\""), "{state}");
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("data-theme=\"light\""), "{page}");
        assert!(page.contains("id=\"theme-toggle\""), "{page}");
        // One hyphenated vocabulary paints shell and panels together.
        assert!(page.contains("--background-color"), "{page}");
        assert!(page.contains("var(--panel-color)"), "{page}");
        assert!(page.contains("theme: light"), "{page}");
        // An unknown theme refuses naming the two values, keeping light.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/theme",
            Some("{\"theme\":\"sepia\"}"),
        );
        assert_eq!(code, 400, "{refused}");
        assert!(refused.contains("unknown theme"), "{refused}");
        // Switching to dark repaints the shell and both panels at once.
        let (code, saved) = body_str(
            port,
            "POST",
            "/api/studio/theme",
            Some("{\"theme\":\"dark\"}"),
        );
        assert_eq!(code, 200, "{saved}");
        assert!(saved.contains("\"theme\":\"dark\""), "{saved}");
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("data-theme=\"dark\""), "{page}");
        assert!(page.contains("theme: dark"), "{page}");
        // The theme file lives in the hidden history dir, never beside
        // the sources.
        assert_eq!(
            studio::studio_theme_path_for(folder),
            folder.join(".framesmith").join("studio-theme.json")
        );
        assert!(studio::studio_theme_path_for(folder).is_file());
        // Per-project: a second project still opens light.
        let other = tmp("theme-other");
        text_project(&other);
        assert!(matches!(
            studio::load_theme(&other).theme,
            studio::StudioTheme::Light
        ));
    });
}

#[test]
fn studio_visual_edit_saves_immediately_as_validated_turn() {
    with_server("autosave", text_project, |folder, _reg, _entry, port| {
        let before = std::fs::read(folder.join("composition.kdl")).unwrap();
        let (code, saved) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "visual", "Hello")),
        );
        assert_eq!(code, 200, "{saved}");
        assert!(saved.contains("\"saved\":true"), "{saved}");
        assert!(saved.contains("\"applied\":1"), "{saved}");
        assert!(saved.contains("\"turn\":"), "{saved}");
        assert!(saved.contains("label"), "{saved}");
        // The save is visible in the shared history under the actor.
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 1);
        assert_eq!(listing.turns[0].actor, "maker");
        // Sources stay byte-identical; the hidden delta records the turn.
        assert_eq!(
            std::fs::read(folder.join("composition.kdl")).unwrap(),
            before
        );
        assert!(folder
            .join(".framesmith")
            .join("sdk-overrides.json")
            .is_file());
        // The timeline readout replays the persisted delta.
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"label\":\"Hello\""), "{rows}");
        // Only validated turns write the project: sources plus the
        // hidden history dir, nothing else.
        let mut top: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        top.sort();
        assert_eq!(
            top,
            vec![".framesmith".to_string(), "composition.kdl".to_string()]
        );
    });
}

#[test]
fn studio_code_edit_saves_through_the_same_validated_turn() {
    with_server("code", text_project, |folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        // No save button anywhere: saves fire automatically.
        assert!(!page.contains("type=\"submit\""), "{page}");
        assert!(!page.contains("save-button"), "{page}");
        // Visual saves call immediately, code saves ride the debounce.
        assert!(page.contains("function autosaveVisual"), "{page}");
        assert!(page.contains("function autosaveCode"), "{page}");
        assert!(page.contains("setTimeout"), "{page}");
        // A code-sourced save lands as the same validated turn.
        let (code, saved) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&move_edit("agent", "code")),
        );
        assert_eq!(code, 200, "{saved}");
        assert!(saved.contains("\"saved\":true"), "{saved}");
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 1);
        assert_eq!(listing.turns[0].actor, "agent");
        // Title moved from index 0 to index 1 in the readout.
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(
            rows.contains("\"index\":1,\"kind\":\"text\",\"id\":\"title\""),
            "{rows}"
        );
    });
}

#[test]
fn studio_paused_queue_banners_and_keeps_every_edit_staged() {
    with_server("queue", text_project, |folder, _reg, _entry, port| {
        let (code, state) = body_str(port, "POST", "/api/studio/queue", Some("{\"paused\":true}"));
        assert_eq!(code, 200, "{state}");
        assert!(state.contains("\"paused\":true"), "{state}");
        // A visual save while paused stages without applying.
        let (code, staged) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "visual", "Staged")),
        );
        assert_eq!(code, 200, "{staged}");
        assert!(staged.contains("\"saved\":false"), "{staged}");
        assert!(staged.contains("\"staged\":1"), "{staged}");
        // Nothing applied: no turn, no delta, rows unchanged.
        assert_eq!(turn_count(folder), 0);
        assert!(!folder
            .join(".framesmith")
            .join("sdk-overrides.json")
            .exists());
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200);
        assert!(!rows.contains("Staged"), "{rows}");
        // The shell banners the paused queue with the staged depth.
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(
            page.contains("id=\"queue-banner\" role=\"alert\""),
            "{page}"
        );
        assert!(page.contains("save queue paused"), "{page}");
        let (code, state) = body_str(port, "GET", "/api/studio/save-state", None);
        assert_eq!(code, 200, "{state}");
        assert!(state.contains("\"paused\":true"), "{state}");
        assert!(state.contains("\"staged\":1"), "{state}");
        // Resuming drains through the SDK machinery as a validated turn.
        let (code, resumed) = body_str(
            port,
            "POST",
            "/api/studio/queue",
            Some("{\"paused\":false}"),
        );
        assert_eq!(code, 200, "{resumed}");
        assert!(resumed.contains("\"saved\":true"), "{resumed}");
        assert!(resumed.contains("\"turn\":"), "{resumed}");
        assert_eq!(turn_count(folder), 1);
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200);
        assert!(rows.contains("\"label\":\"Staged\""), "{rows}");
    });
}

#[test]
fn studio_failed_drain_keeps_every_edit_staged() {
    with_server("drainfail", text_project, |folder, _reg, _entry, port| {
        let good = std::fs::read(folder.join("composition.kdl")).unwrap();
        let (code, _) = body_str(port, "POST", "/api/studio/queue", Some("{\"paused\":true}"));
        assert_eq!(code, 200);
        let (code, staged) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "visual", "Held")),
        );
        assert_eq!(code, 200, "{staged}");
        // Break the project on disk: the drain must refuse with the
        // staged edit kept, not half-applied.
        std::fs::write(folder.join("composition.kdl"), "composition { broken (((\n").unwrap();
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/queue",
            Some("{\"paused\":false}"),
        );
        assert_eq!(code, 409, "{refused}");
        let (code, state) = body_str(port, "GET", "/api/studio/save-state", None);
        assert_eq!(code, 200, "{state}");
        assert!(state.contains("\"staged\":1"), "{state}");
        assert_eq!(turn_count(folder), 0);
        // Restore the sources: the same staged edit replays as a turn.
        std::fs::write(folder.join("composition.kdl"), good).unwrap();
        let (code, resumed) = body_str(
            port,
            "POST",
            "/api/studio/queue",
            Some("{\"paused\":false}"),
        );
        assert_eq!(code, 200, "{resumed}");
        assert!(resumed.contains("\"saved\":true"), "{resumed}");
        assert_eq!(turn_count(folder), 1);
    });
}

#[test]
fn studio_unreachable_project_opens_read_only_and_keeps_staged() {
    with_server("gone", text_project, |folder, _reg, _entry, port| {
        let away = folder.parent().unwrap().join("proj-away");
        std::fs::rename(folder, &away).unwrap();
        // The shell opens read-only with a banner, never refusing.
        let (code, page) = body_str(port, "GET", "/studio?composition=t", None);
        assert_eq!(code, 200, "{page}");
        assert!(page.contains("data-read-only=\"1\""), "{page}");
        assert!(page.contains("read-only"), "{page}");
        assert!(page.contains("unreachable"), "{page}");
        // Readouts name the unreachable project instead of rows.
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 404, "{rows}");
        assert!(rows.contains("unreachable"), "{rows}");
        // Saves stage read-only: nothing applies, nothing is lost.
        let (code, staged) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "visual", "Held")),
        );
        assert_eq!(code, 409, "{staged}");
        assert!(staged.contains("\"staged\":1"), "{staged}");
        assert!(staged.contains("read-only"), "{staged}");
        // Theme saves refuse without recreating the project on disk.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/theme",
            Some("{\"theme\":\"dark\"}"),
        );
        assert_eq!(code, 409, "{refused}");
        // The refused theme save recreated nothing on disk.
        assert!(!folder.exists());
        // Restore the folder: the staged edit replays as a validated turn.
        std::fs::rename(&away, folder).unwrap();
        let (code, resumed) = body_str(
            port,
            "POST",
            "/api/studio/queue",
            Some("{\"paused\":false}"),
        );
        assert_eq!(code, 200, "{resumed}");
        assert!(resumed.contains("\"saved\":true"), "{resumed}");
        assert_eq!(turn_count(folder), 1);
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("data-read-only=\"0\""), "{page}");
    });
}

#[test]
fn studio_edit_refusals_name_the_cause_and_change_nothing() {
    with_server("refuse-edit", text_project, |folder, _reg, _entry, port| {
        let (_, before_rows) = http(port, "GET", "/api/studio/timeline", None);
        let before_src = std::fs::read(folder.join("composition.kdl")).unwrap();
        let cases: &[(&str, Option<&str>, u16, &str)] = &[
            ("empty", None, 400, "needs a JSON edit body"),
            ("garbage", Some("{ nope"), 400, "not JSON"),
            (
                "actor",
                Some("{\"edits\":[]}"),
                400,
                "names no actor",
            ),
            (
                "empty-edits",
                Some("{\"actor\":\"m\",\"edits\":[]}"),
                400,
                "empty `edits`",
            ),
            (
                "source",
                Some("{\"actor\":\"m\",\"source\":\"pigeon\",\"edits\":[{\"op\":\"setlabel\",\"target\":{\"id\":\"title\",\"index\":null},\"label\":\"x\"}]}"),
                400,
                "unknown save source",
            ),
            (
                "op",
                Some("{\"actor\":\"m\",\"edits\":[{\"op\":\"frobnicate\"}]}"),
                400,
                "not patch operations",
            ),
            (
                "target",
                Some("{\"actor\":\"m\",\"edits\":[{\"op\":\"removeelement\",\"target\":{\"id\":\"nope\",\"index\":null}}]}"),
                409,
                "E_SDK_UNKNOWN_TARGET",
            ),
            (
                "value",
                Some("{\"actor\":\"m\",\"edits\":[{\"op\":\"retiming\",\"target\":{\"id\":\"title\",\"index\":null},\"start\":null,\"duration\":null,\"trim_start\":0.5}]}"),
                409,
                "E_SDK_UNSUPPORTED",
            ),
        ];
        for (label, body, code, needle) in cases {
            let (got, msg) = body_str(port, "POST", "/api/studio/edit", *body);
            assert_eq!(got, *code, "{label}: {msg}");
            assert!(msg.contains(needle), "{label}: {msg}");
        }
        // The queue switch refuses a body without the pause switch.
        let (got, msg) = body_str(port, "POST", "/api/studio/queue", Some("{}"));
        assert_eq!(got, 400, "{msg}");
        assert!(msg.contains("queue switch needs"), "{msg}");
        // Nothing changed: no turns, same rows, same sources, no delta.
        assert_eq!(turn_count(folder), 0);
        assert!(!folder
            .join(".framesmith")
            .join("sdk-overrides.json")
            .exists());
        let (_, after_rows) = http(port, "GET", "/api/studio/timeline", None);
        assert_eq!(before_rows, after_rows);
        assert_eq!(
            std::fs::read(folder.join("composition.kdl")).unwrap(),
            before_src
        );
    });
}

#[test]
fn studio_default_renders_stay_byte_identical() {
    with_server("bytes", text_project, |folder, _reg, _entry, port| {
        let (code, before) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(code, 200);
        assert!(!before.is_empty());
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/theme",
            Some("{\"theme\":\"dark\"}"),
        );
        assert_eq!(code, 200);
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/layout",
            Some("{\"version\":1,\"panels\":{\"preview\":{\"dock\":\"left\"},\"timeline\":{\"dock\":\"right\"}}}"),
        );
        assert_eq!(code, 200);
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "visual", "Same")),
        );
        assert_eq!(code, 200);
        // Theme, layout, and a label-only turn never touch pixels.
        let (code, after) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(code, 200);
        assert_eq!(before, after);
        let _ = folder;
    });
}
