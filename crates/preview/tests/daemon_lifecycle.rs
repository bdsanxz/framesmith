//! Daemon lifecycle regressions (Sprint 14, T006 / CAP-006, J-03,
//! J-04, STATE-01, INT-002, REC-001, REC-003): every acceptance
//! criterion of `T006-manage-daemon-servers.md`, including refusal
//! variants.
//!
//! Liveness here is a real loopback TCP connect, and "live servers"
//! are real bound listeners held by stub threads that answer the
//! same `/api/shutdown` route the real server serves — so stop and
//! stop-all end them exactly like production. The full binary path
//! (spawn, readiness, fall-forward retry) is covered again in
//! `cli_preview_daemon.rs` through the real `framesmith` binary.

use preview::daemon::*;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-daemon-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text_project(folder: &Path) {
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

fn broken_project(folder: &Path) {
    std::fs::write(folder.join("composition.kdl"), "composition {{{ not kdl\n").unwrap();
}

/// A stub "server": holds its port open and ends itself on the same
/// `POST /api/shutdown` route the real server answers, so lifecycle
/// primitives behave exactly as in production.
fn stub_server() -> (u16, std::thread::JoinHandle<()>) {
    let listener = test_bind(0).expect("stub binds");
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).into_owned();
            if head.starts_with("POST /api/shutdown") {
                let _ = s.write_all(
                    b"HTTP/1.0 200 OK\r\nContent-Length: 18\r\nConnection: close\r\n\r\npreview shutting down",
                );
                break;
            }
            let _ =
                s.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        }
    });
    (port, handle)
}

fn entry_for(dir: &str, port: u16) -> ServerEntry {
    ServerEntry::new(
        dir.to_string(),
        "proj".to_string(),
        "t".to_string(),
        port,
        12345,
    )
}

#[test]
fn registry_round_trips_nasty_paths() {
    let root = tmp("json");
    let reg = root.join("servers.json");
    let entries = vec![
        ServerEntry::new(
            "C:\\we\"ird\npath".to_string(),
            "na\tme".to_string(),
            "comp\"1".to_string(),
            8130,
            42,
        ),
        ServerEntry::new(
            "/tmp/projet-é-☃".to_string(),
            "ünïcode".to_string(),
            "t".to_string(),
            8131,
            43,
        ),
    ];
    save_registry(&reg, &entries).unwrap();
    let back = load_registry(&reg).unwrap();
    assert_eq!(back, entries);
    // URLs survive the round trip too.
    assert_eq!(back[0].url, "http://127.0.0.1:8130/");
    // Missing file is an empty registry, not an error.
    let missing = load_registry(&root.join("absent.json")).unwrap();
    assert!(missing.is_empty());
    // Malformed bodies name the file.
    std::fs::write(&reg, "[{oops").unwrap();
    let err = load_registry(&reg).unwrap_err();
    assert!(err.contains("servers.json"), "{err}");
}

#[test]
fn start_plans_a_free_port_and_prints_its_url() {
    let root = tmp("fresh");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let reg = root.join("servers.json");
    match plan_start(&reg, &folder, None).expect("valid project plans") {
        StartPlan::ToStart {
            port,
            url,
            name,
            composition,
            dir,
        } => {
            assert_eq!(url, url_for_port(port));
            assert_eq!(url, format!("http://127.0.0.1:{port}/"));
            assert_eq!(name, "proj");
            assert_eq!(composition, "t");
            assert!(dir.ends_with("proj"), "{dir}");
            // The planned port is genuinely free right now.
            let held = std::net::TcpListener::bind(("127.0.0.1", port));
            assert!(held.is_ok(), "planned port must be free: {port}");
        }
        StartPlan::AlreadyRunning(_) => panic!("no server is running yet"),
    }
    // Planning writes nothing: no server starts from a plan alone.
    assert!(!reg.exists(), "a plan must not create the registry");
}

#[test]
fn second_start_for_one_directory_reuses_the_existing_url() {
    let root = tmp("dedupe");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let reg = root.join("servers.json");
    let (port, handle) = stub_server();
    let dir = canonical_dir(&folder).unwrap();
    let live = entry_for(&dir, port);
    add_entry(&reg, &live).unwrap();
    match plan_start(&reg, &folder, None).expect("plans") {
        StartPlan::AlreadyRunning(existing) => {
            assert_eq!(existing.port, port);
            assert_eq!(existing.url, live.url);
            let report = msg_existing(&existing);
            assert!(report.contains(&live.url), "{report}");
            assert!(report.contains("already"), "{report}");
        }
        StartPlan::ToStart { .. } => panic!("must reuse the live server"),
    }
    stop_server(&reg, &dir).unwrap();
    handle.join().unwrap();
}

#[test]
fn busy_preferred_port_falls_forward_with_the_new_url() {
    let root = tmp("busy");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let reg = root.join("servers.json");
    // Occupy the preferred port: it is busy for the whole test.
    let busy = test_bind(0).unwrap();
    let busy_port = busy.local_addr().unwrap().port();
    assert!(is_live(busy_port));
    assert_eq!(pick_port(Some(busy_port)).unwrap(), {
        let p = pick_port(Some(busy_port)).unwrap();
        assert_ne!(p, busy_port, "must fall forward past the busy port");
        assert!(!is_live(p), "fallen-forward port must be free");
        p
    });
    match plan_start(&reg, &folder, Some(busy_port)).expect("plans") {
        StartPlan::ToStart { port, url, .. } => {
            assert_ne!(port, busy_port);
            assert_eq!(url, url_for_port(port));
            assert!(!is_live(port));
        }
        StartPlan::AlreadyRunning(_) => panic!("no server is running"),
    }
}

#[test]
fn broken_project_refuses_before_any_server_starts() {
    let root = tmp("broken");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    broken_project(&folder);
    let reg = root.join("servers.json");
    let refusal = plan_start(&reg, &folder, None).unwrap_err();
    assert!(
        refusal.message.contains("preview refused"),
        "{}",
        refusal.message
    );
    assert!(
        refusal.message.contains("composition.kdl"),
        "{}",
        refusal.message
    );
    assert!(refusal.message.contains("fix"), "{}", refusal.message);
    assert!(
        refusal.message.contains("No preview was started"),
        "{}",
        refusal.message
    );
    assert!(!reg.exists(), "a refusal must record nothing");
}

#[test]
fn missing_directory_refuses_before_any_server_starts() {
    let root = tmp("missing");
    let reg = root.join("servers.json");
    let refusal = plan_start(&reg, &root.join("absent"), None).unwrap_err();
    assert!(
        refusal.message.contains("no project directory"),
        "{}",
        refusal.message
    );
    assert!(
        refusal.message.contains("No preview was started"),
        "{}",
        refusal.message
    );
    assert!(!reg.exists());
}

#[test]
fn stopping_a_missing_server_reports_already_stopped() {
    let root = tmp("stop-missing");
    let reg = root.join("servers.json");
    match stop_server(&reg, "/nowhere/proj").expect("stops") {
        StopOutcome::AlreadyStopped { dir } => {
            let report = msg_already_stopped(&dir);
            assert!(report.contains("already stopped"), "{report}");
        }
        StopOutcome::Stopped(_) => panic!("nothing was running"),
    }
}

#[test]
fn stopping_a_dead_server_reports_already_stopped_and_prunes() {
    let root = tmp("stop-dead");
    let reg = root.join("servers.json");
    // A free port with no listener is a dead server.
    let dead_port = test_bind(0).unwrap().local_addr().unwrap().port();
    assert!(!is_live(dead_port));
    add_entry(&reg, &entry_for("/dead/proj", dead_port)).unwrap();
    match stop_server(&reg, "/dead/proj").expect("stops") {
        StopOutcome::AlreadyStopped { .. } => {}
        StopOutcome::Stopped(_) => panic!("a dead server cannot stop"),
    }
    let entries = load_registry(&reg).unwrap();
    assert!(entries.is_empty(), "dead entries prune: {entries:?}");
}

#[test]
fn stop_ends_exactly_that_server() {
    let root = tmp("stop-one");
    let reg = root.join("servers.json");
    let (port_a, handle_a) = stub_server();
    let (port_b, _handle_b) = stub_server();
    let dir_a = "/proj/a".to_string();
    let dir_b = "/proj/b".to_string();
    add_entry(&reg, &entry_for(&dir_a, port_a)).unwrap();
    add_entry(&reg, &entry_for(&dir_b, port_b)).unwrap();
    match stop_server(&reg, &dir_a).expect("stops") {
        StopOutcome::Stopped(entry) => {
            assert_eq!(entry.port, port_a);
            let report = msg_stopped(&entry);
            assert!(report.contains(&entry.url), "{report}");
            assert!(report.contains("stopped"), "{report}");
        }
        StopOutcome::AlreadyStopped { .. } => panic!("server A is live"),
    }
    handle_a.join().unwrap();
    assert!(!is_live(port_a), "stopped server is quiet");
    assert!(is_live(port_b), "the other server is untouched");
    let entries = load_registry(&reg).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].dir, dir_b);
    // The survivor still stops cleanly.
    stop_server(&reg, &dir_b).unwrap();
}

#[test]
fn stop_all_ends_all_and_reports_the_count() {
    let root = tmp("stop-all");
    let reg = root.join("servers.json");
    let (port_a, handle_a) = stub_server();
    let (port_b, handle_b) = stub_server();
    let dead_port = test_bind(0).unwrap().local_addr().unwrap().port();
    add_entry(&reg, &entry_for("/proj/a", port_a)).unwrap();
    add_entry(&reg, &entry_for("/proj/b", port_b)).unwrap();
    add_entry(&reg, &entry_for("/proj/dead", dead_port)).unwrap();
    let outcome = stop_all_servers(&reg).expect("stops all");
    assert_eq!(outcome.stopped.len(), 2);
    assert_eq!(outcome.already, vec!["/proj/dead".to_string()]);
    assert_eq!(msg_stop_all(&outcome), "stopped 2 preview server(s).");
    handle_a.join().unwrap();
    handle_b.join().unwrap();
    assert!(!is_live(port_a) && !is_live(port_b));
    assert!(load_registry(&reg).unwrap().is_empty());
}

#[test]
fn empty_stop_all_reports_zero() {
    let root = tmp("stop-all-empty");
    let reg = root.join("servers.json");
    let outcome = stop_all_servers(&reg).expect("empty stop-all succeeds");
    assert!(outcome.stopped.is_empty());
    assert!(outcome.already.is_empty());
    assert_eq!(msg_stop_all(&outcome), "stopped 0 preview server(s).");
}

#[test]
fn list_shows_every_live_server_and_prunes_dead() {
    let root = tmp("list");
    let reg = root.join("servers.json");
    let (port_a, _handle_a) = stub_server();
    let dead_port = test_bind(0).unwrap().local_addr().unwrap().port();
    add_entry(&reg, &entry_for("/proj/a", port_a)).unwrap();
    add_entry(&reg, &entry_for("/proj/dead", dead_port)).unwrap();
    let live = list_servers(&reg).expect("lists");
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].port, port_a);
    let report = msg_list(&live);
    assert!(report.contains("1"), "{report}");
    assert!(report.contains(&live[0].url), "{report}");
    assert_eq!(msg_list(&[]), "no preview servers running.");
    stop_server(&reg, "/proj/a").unwrap();
}

#[test]
fn status_reports_name_the_project_and_port_or_none() {
    let live = entry_for("/proj/demo", 8123);
    let some = msg_status_some(&live);
    assert!(some.contains("proj"), "{some}");
    assert!(some.contains("8123"), "{some}");
    assert!(some.contains("http://127.0.0.1:8123/"), "{some}");
    let none = msg_status_none("/proj/demo");
    assert!(none.contains("no preview server"), "{none}");
    assert!(none.contains("/proj/demo"), "{none}");
}
