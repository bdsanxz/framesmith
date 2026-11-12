//! External-change conflicts (Sprint 16, T003 / CAP-003, J-05,
//! STATE-04, REC-002): a project file changing on disk while Studio
//! holds unsaved work banners disk versus unsaved; the maker's pick
//! applies with the losing version kept in history; hot reload applies
//! automatically only while no unsaved work is held; a pick that cannot
//! apply reports why and keeps both versions.

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
        "framesmith-studio-t003-{}-{}-{}",
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
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"sub\" value=\"Bye\" start=1 duration=1 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Disk version of the project: the `sub` clip carries a `Disk` label,
/// so rows name the disk side of the conflict.
fn disk_version(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"sub\" label=\"Disk\" value=\"Bye\" start=1 duration=1 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Disk version that drops the `title` clip: staged edits against
/// `title` can no longer apply, so picks report why.
fn disk_without_title(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"sub\" value=\"Bye\" start=1 duration=1 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
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

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .expect("history lists")
        .turns
        .len()
}

fn label_edit(actor: &str, label: &str) -> String {
    format!("{{\"actor\":\"{actor}\",\"source\":\"visual\",\"edits\":[{{\"op\":\"setlabel\",\"target\":{{\"id\":\"title\",\"index\":null}},\"label\":\"{label}\"}}]}}")
}

fn pause(port: u16) {
    let (code, state) = body_str(port, "POST", "/api/studio/queue", Some("{\"paused\":true}"));
    assert_eq!(code, 200, "{state}");
}

/// First recorded turn sequence in the resolved receipt.
fn receipt_turn(receipt: &str) -> u64 {
    let key = "\"turn\":";
    let at = receipt.find(key).expect("receipt carries a turn");
    receipt[at + key.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .expect("turn parses")
}

#[test]
fn studio_clean_hot_reload_applies_with_no_unsaved_work() {
    with_server("clean", text_project, |folder, _reg, _entry, port| {
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(!rows.contains("\"label\":\"Disk\""), "{rows}");
        // An external rewrite with no staged work reloads silently: no
        // conflict, no turn, no delta file.
        disk_version(folder);
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"label\":\"Disk\""), "{rows}");
        let (code, conflict) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{conflict}");
        assert!(conflict.contains("\"conflict\":false"), "{conflict}");
        assert_eq!(turn_count(folder), 0);
        assert!(!folder
            .join(".framesmith")
            .join("sdk-overrides.json")
            .exists());
        // A clean reload writes nothing at all: the sources stand
        // alone with no hidden history dir, no scratch, no cache.
        let mut top: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        top.sort();
        assert_eq!(top, vec!["composition.kdl".to_string()]);
    });
}

#[test]
fn studio_external_change_under_unsaved_work_banners_both_versions() {
    with_server("banner", text_project, |folder, _reg, _entry, port| {
        pause(port);
        let (code, staged) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "Staged")),
        );
        assert_eq!(code, 200, "{staged}");
        assert!(staged.contains("\"saved\":false"), "{staged}");
        disk_version(folder);
        // The conflict names both versions: the changed disk file and
        // the unsaved staged edits.
        let (code, conflict) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{conflict}");
        assert!(conflict.contains("\"conflict\":true"), "{conflict}");
        assert!(conflict.contains("changed on disk"), "{conflict}");
        assert!(conflict.contains("composition.kdl"), "{conflict}");
        assert!(conflict.contains("Staged"), "{conflict}");
        assert!(conflict.contains("\"staged\":1"), "{conflict}");
        // The shell banners disk versus unsaved with the maker's picks.
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(
            page.contains("id=\"conflict-banner\" role=\"alert\""),
            "{page}"
        );
        assert!(page.contains("changed on disk"), "{page}");
        assert!(page.contains("Staged"), "{page}");
        assert!(page.contains("id=\"pick-disk\""), "{page}");
        assert!(page.contains("id=\"pick-unsaved\""), "{page}");
        assert!(page.contains("refreshConflict"), "{page}");
        assert!(page.contains("#conflict-banner"), "{page}");
        // Neither version wins silently: rows still show the pre-change
        // view — no disk label, no staged label.
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(!rows.contains("\"label\":\"Disk\""), "{rows}");
        assert!(!rows.contains("\"label\":\"Staged\""), "{rows}");
        // Editing pauses until the pick lands: edits, tools, and manual
        // reloads all refuse naming the pick.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "Later")),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("waiting for your pick"), "{refused}");
        let (code, refused) = body_str(port, "POST", "/api/reload", None);
        assert_eq!(code, 409, "{refused}");
        assert!(refused.contains("waiting for your pick"), "{refused}");
        // Nothing applied: no turn, no delta, staged edit intact.
        assert_eq!(turn_count(folder), 0);
        assert!(!folder
            .join(".framesmith")
            .join("sdk-overrides.json")
            .exists());
        let (code, state) = body_str(port, "GET", "/api/studio/save-state", None);
        assert_eq!(code, 200, "{state}");
        assert!(state.contains("\"staged\":1"), "{state}");
    });
}

#[test]
fn studio_pick_unsaved_applies_and_keeps_disk_in_history() {
    with_server("unsaved", text_project, |folder, _reg, _entry, port| {
        pause(port);
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "Staged")),
        );
        assert_eq!(code, 200);
        disk_version(folder);
        let (code, pending) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{pending}");
        assert!(pending.contains("\"conflict\":true"), "{pending}");
        // The maker keeps the unsaved version: one validated turn under
        // the pick actor, rows showing the disk base plus the staged
        // label.
        let (code, resolved) = body_str(
            port,
            "POST",
            "/api/studio/conflict",
            Some("{\"pick\":\"unsaved\",\"actor\":\"maker\"}"),
        );
        assert_eq!(code, 200, "{resolved}");
        assert!(resolved.contains("\"resolved\":true"), "{resolved}");
        assert!(resolved.contains("\"pick\":\"unsaved\""), "{resolved}");
        assert!(
            resolved.contains("disk version is kept in turn"),
            "{resolved}"
        );
        let seq = receipt_turn(&resolved);
        assert_eq!(turn_count(folder), 1);
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns[0].actor, "maker");
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"label\":\"Staged\""), "{rows}");
        assert!(rows.contains("\"label\":\"Disk\""), "{rows}");
        let (code, conflict) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{conflict}");
        assert!(conflict.contains("\"conflict\":false"), "{conflict}");
        // The losing disk version rests in the turn's backup: history
        // undo brings the disk version back, unsaved gone.
        sdk::history::undo_turn(folder, seq).expect("undo restores disk");
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"label\":\"Disk\""), "{rows}");
        assert!(!rows.contains("\"label\":\"Staged\""), "{rows}");
    });
}

#[test]
fn studio_pick_disk_applies_and_keeps_unsaved_in_history() {
    with_server("disk", text_project, |folder, _reg, _entry, port| {
        pause(port);
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "Staged")),
        );
        assert_eq!(code, 200);
        disk_version(folder);
        // The maker keeps the disk version: one validated turn, rows
        // showing the disk file with the staged label gone.
        let (code, resolved) = body_str(
            port,
            "POST",
            "/api/studio/conflict",
            Some("{\"pick\":\"disk\",\"actor\":\"agent\"}"),
        );
        assert_eq!(code, 200, "{resolved}");
        assert!(resolved.contains("\"resolved\":true"), "{resolved}");
        assert!(resolved.contains("\"pick\":\"disk\""), "{resolved}");
        assert!(
            resolved.contains("unsaved Studio version is kept in turn"),
            "{resolved}"
        );
        let seq = receipt_turn(&resolved);
        assert_eq!(turn_count(folder), 1);
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns[0].actor, "agent");
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"label\":\"Disk\""), "{rows}");
        assert!(!rows.contains("\"label\":\"Staged\""), "{rows}");
        let (code, state) = body_str(port, "GET", "/api/studio/save-state", None);
        assert_eq!(code, 200, "{state}");
        assert!(state.contains("\"staged\":0"), "{state}");
        // The losing unsaved version rests in the turn's backup:
        // history restore brings the staged label back.
        sdk::history::restore_turn(folder, seq).expect("restore brings back unsaved");
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"label\":\"Staged\""), "{rows}");
        assert!(rows.contains("\"label\":\"Disk\""), "{rows}");
    });
}

#[test]
fn studio_failing_pick_reports_why_and_keeps_both_versions() {
    with_server("failpick", text_project, |folder, _reg, _entry, port| {
        pause(port);
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "Staged")),
        );
        assert_eq!(code, 200);
        // The disk drops the `title` clip the staged edit names: no
        // pick can apply.
        disk_without_title(folder);
        let (code, pending) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{pending}");
        assert!(pending.contains("\"conflict\":true"), "{pending}");
        for pick in ["unsaved", "disk"] {
            let (code, refused) = body_str(
                port,
                "POST",
                "/api/studio/conflict",
                Some(&format!("{{\"pick\":\"{pick}\",\"actor\":\"maker\"}}")),
            );
            assert_eq!(code, 409, "{pick}: {refused}");
            assert!(refused.contains("cannot apply"), "{pick}: {refused}");
            assert!(
                refused.contains("Both versions are kept"),
                "{pick}: {refused}"
            );
        }
        // Both versions survive: the disk file holds its bytes, the
        // staged edit still waits, the conflict still pends, no turn.
        assert_eq!(turn_count(folder), 0);
        let (code, conflict) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{conflict}");
        assert!(conflict.contains("\"conflict\":true"), "{conflict}");
        let (code, state) = body_str(port, "GET", "/api/studio/save-state", None);
        assert_eq!(code, 200, "{state}");
        assert!(state.contains("\"staged\":1"), "{state}");
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(!rows.contains("\"label\":\"Staged\""), "{rows}");
        // Restoring the clip returns the known bytes, so the conflict
        // is moot and clears; the same staged edit then drains through
        // the normal resume path — the maker is never forced to drop
        // work.
        text_project(folder);
        let (code, conflict) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{conflict}");
        assert!(conflict.contains("\"conflict\":false"), "{conflict}");
        let (code, resumed) = body_str(
            port,
            "POST",
            "/api/studio/queue",
            Some("{\"paused\":false}"),
        );
        assert_eq!(code, 200, "{resumed}");
        assert!(resumed.contains("\"saved\":true"), "{resumed}");
        assert_eq!(turn_count(folder), 1);
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"label\":\"Staged\""), "{rows}");
    });
}

#[test]
fn studio_conflict_pick_refusals_change_nothing() {
    with_server("refuse-pick", text_project, |folder, _reg, _entry, port| {
        // No pending change: the pick refuses with nothing changed.
        let (code, refused) = body_str(
            port,
            "POST",
            "/api/studio/conflict",
            Some("{\"pick\":\"disk\",\"actor\":\"maker\"}"),
        );
        assert_eq!(code, 409, "{refused}");
        assert!(
            refused.contains("no external change is pending"),
            "{refused}"
        );
        let cases: &[(&str, Option<&str>, u16, &str)] = &[
            ("empty", None, 400, "needs a JSON pick body"),
            ("garbage", Some("{ nope"), 400, "not JSON"),
            (
                "pick",
                Some("{\"pick\":\"sideways\",\"actor\":\"maker\"}"),
                400,
                "unknown conflict pick",
            ),
            ("actor", Some("{\"pick\":\"disk\"}"), 400, "names no actor"),
        ];
        for (label, body, code, needle) in cases {
            let (got, msg) = body_str(port, "POST", "/api/studio/conflict", *body);
            assert_eq!(got, *code, "{label}: {msg}");
            assert!(msg.contains(needle), "{label}: {msg}");
        }
        assert_eq!(turn_count(folder), 0);
        assert!(!folder
            .join(".framesmith")
            .join("sdk-overrides.json")
            .exists());
        let (code, conflict) = body_str(port, "GET", "/api/studio/conflict", None);
        assert_eq!(code, 200, "{conflict}");
        assert!(conflict.contains("\"conflict\":false"), "{conflict}");
    });
}

#[test]
fn studio_conflict_cycle_keeps_default_renders_byte_identical() {
    with_server("bytes", text_project, |folder, _reg, _entry, port| {
        let (code, before) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(code, 200);
        assert!(!before.is_empty());
        // Label-only changes on both sides never touch pixels: the
        // frame after the full conflict cycle matches bit for bit.
        pause(port);
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/edit",
            Some(&label_edit("maker", "Staged")),
        );
        assert_eq!(code, 200);
        disk_version(folder);
        let (code, resolved) = body_str(
            port,
            "POST",
            "/api/studio/conflict",
            Some("{\"pick\":\"unsaved\",\"actor\":\"maker\"}"),
        );
        assert_eq!(code, 200, "{resolved}");
        let (code, after) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(code, 200);
        assert_eq!(before, after);
        let _ = folder;
    });
}
