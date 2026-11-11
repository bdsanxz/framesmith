//! Sprint 24 T004: slideshow present session over loopback HTTP —
//! the two-view sync proof gate.
//!
//! The one-command proof (`tools/n24-slideshow-proof.sh`) stages
//! the reference to a workdir copy and points this test at it with
//! `FRAMESMITH_N24_PROOF_REF`, so the two-view sync runs on exactly
//! the staged fixture bytes. Without the variable the test falls
//! back to the checkout reference, so `cargo test` alone still
//! proves the same property on the same files.
//!
//! Drives the reference deck through every navigation input over
//! the real present server (fragment hold, sequential next, branch
//! resolution, prev/home/end, hotspot click, autoplay tick, and a
//! vanished-target hold), checks picker-cache frame bytes repeat
//! identical per slide and differ across slides, and asserts the
//! Studio slideshow panel checklist mirrors the validated manifest
//! with a Present launch hint.

use preview::daemon;
use preview::present::{run_present_serve, PresentServeOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-present-n24slideshow-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn reference_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FRAMESMITH_N24_PROOF_REF") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/slideshow-reference")
    }
}

/// Minimal HTTP exchange over loopback: returns (status code, body).
/// `body` is sent as the request body when non-empty (POST navigates).
fn http(port: u16, method: &str, target: &str, body: &str) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
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

fn body_str(port: u16, method: &str, target: &str, body: &str) -> (u16, String) {
    let (code, body) = http(port, method, target, body);
    (code, String::from_utf8_lossy(&body).into_owned())
}

fn navigate(port: u16, json: &str) -> String {
    let (code, body) = body_str(port, "POST", "/api/present/navigate", json);
    assert_eq!(code, 200, "{body}");
    body
}

fn current_id(port: u16) -> String {
    let (code, body) = body_str(port, "GET", "/api/present/status", "");
    assert_eq!(code, 200, "{body}");
    let key = "\"currentId\":\"";
    let start = body.find(key).expect("status carries currentId");
    let rest = &body[start + key.len()..];
    let end = rest.find('"').expect("currentId closes");
    rest[..end].to_string()
}

#[test]
fn present_session_proves_two_view_sync_through_the_state_machine() {
    let root = tmp("deck");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    let kdl = std::fs::read_to_string(reference_dir().join("composition.kdl"))
        .expect("reference composition.kdl");
    std::fs::write(folder.join("composition.kdl"), &kdl).unwrap();

    let reg = root.join("registry.json");
    let dir = daemon::canonical_dir(&folder).unwrap();
    let opts = PresentServeOptions {
        folder: folder.clone(),
        port: 0,
    };
    let reg_t = reg.clone();
    let handle = std::thread::spawn(move || run_present_serve(&opts, &reg_t));
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
            panic!("present server exited before serving");
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "present server never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let port = entry.port;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Both views serve on the same state machine from the first
        // frame: presenter shows current/next/notes/timer/press-P,
        // audience shows the fullscreen slide frame.
        let (code, presenter) = body_str(port, "GET", "/presenter", "");
        assert_eq!(code, 200, "{presenter}");
        assert!(presenter.contains("presenter: current \"s1\""), "{presenter}");
        assert!(presenter.contains("\"s2\""), "{presenter}");
        assert!(presenter.contains("notes: welcome"), "{presenter}");
        assert!(presenter.contains("timer:"), "{presenter}");
        assert!(presenter.contains("toggle fullscreen (P)"), "{presenter}");
        assert!(presenter.contains("ArrowRight"), "{presenter}");
        assert!(presenter.contains("ArrowLeft"), "{presenter}");
        assert!(presenter.contains("Home"), "{presenter}");
        assert!(presenter.contains("End"), "{presenter}");
        let (code, audience) = body_str(port, "GET", "/audience", "");
        assert_eq!(code, 200, "{audience}");
        assert!(audience.contains("/api/present/frame"), "{audience}");
        assert_eq!(current_id(port), "s1", "session opens on the first slide");

        // Arrow-next on s1: the fragment holds before advancing.
        let body = navigate(port, r#"{"action":"next"}"#);
        assert!(body.contains("\"ok\":false"), "{body}");
        assert!(body.contains("fragment"), "{body}");
        assert_eq!(current_id(port), "s1", "fragment holds on s1");
        assert!(body.contains("\"fragmentIndex\":1"), "{body}");

        // Second next advances to s2; slides agree with the
        // presenter page (two-view sync through the one machine).
        let body = navigate(port, r#"{"action":"next"}"#);
        assert!(body.contains("\"ok\":true"), "{body}");
        assert_eq!(current_id(port), "s2");
        let (_, page) = body_str(port, "GET", "/presenter", "");
        assert!(page.contains("presenter: current \"s2\""), "{page}");

        // Next on s2 follows the branch to s3 first.
        navigate(port, r#"{"action":"next"}"#);
        assert_eq!(current_id(port), "s3", "branch lands on its target");
        let (_, audience) = body_str(port, "GET", "/audience", "");
        assert!(audience.contains("/api/present/frame"), "{audience}");

        // Prev steps back; home/end jump to the deck ends.
        navigate(port, r#"{"action":"prev"}"#);
        assert_eq!(current_id(port), "s2");
        navigate(port, r#"{"action":"home"}"#);
        assert_eq!(current_id(port), "s1");
        navigate(port, r#"{"action":"end"}"#);
        assert_eq!(current_id(port), "s3");

        // Hotspot h1 on s1 jumps straight to s3.
        navigate(port, r#"{"action":"home"}"#);
        navigate(port, r#"{"action":"hotspot","hotspot":"h1"}"#);
        assert_eq!(current_id(port), "s3", "hotspot lands on its target");

        // A vanished target holds the last good slide and reports.
        let body = navigate(port, r#"{"action":"goto","target":"gone"}"#);
        assert!(body.contains("\"ok\":false"), "{body}");
        assert!(body.contains("vanished"), "{body}");
        assert_eq!(current_id(port), "s3", "vanished target holds last good");

        // Autoplay ticks past the fragment by duration: from s1 a
        // tick advances to s2 (fragments never hold autoplay).
        navigate(port, r#"{"action":"home"}"#);
        assert_eq!(current_id(port), "s1");
        let mut advanced = false;
        for _ in 0..3 {
            std::thread::sleep(Duration::from_millis(1200));
            navigate(port, r#"{"action":"tick"}"#);
            if current_id(port) == "s2" {
                advanced = true;
                break;
            }
        }
        assert!(advanced, "autoplay tick must advance s1 -> s2");

        // Picker-cache frames: repeats on one slide are
        // byte-identical; a new slide serves different bytes.
        let (_, frame_a) = http(port, "GET", "/api/present/frame", "");
        let (_, frame_b) = http(port, "GET", "/api/present/frame", "");
        assert_eq!(frame_a, frame_b, "same slide repeats identical bytes");
        assert_eq!(
            &frame_a[..8],
            &[137, 80, 78, 71, 13, 10, 26, 10],
            "exact PNG bytes"
        );
        let (_, frame_c) = {
            navigate(port, r#"{"action":"next"}"#);
            http(port, "GET", "/api/present/frame", "")
        };
        assert_ne!(frame_a, frame_c, "different slides differ");

        // The Studio slideshow panel mirrors the validated
        // manifest in order, with notes/fragments/branches/
        // hotspots and a Present launch hint.
        let (code, panel) = body_str(port, "GET", "/api/studio/slideshow", "");
        assert_eq!(code, 200, "{panel}");
        assert!(panel.contains("\"id\":\"s1\""), "{panel}");
        assert!(panel.contains("\"composition\":\"intro\""), "{panel}");
        assert!(panel.contains("\"notes\":\"welcome\""), "{panel}");
        assert!(panel.contains("\"fragments\":1"), "{panel}");
        assert!(panel.contains("\"hotspots\":[\"s3\"]"), "{panel}");
        assert!(panel.contains("\"id\":\"s2\""), "{panel}");
        assert!(panel.contains("\"composition\":\"agenda\""), "{panel}");
        assert!(panel.contains("\"notes\":\"topics\""), "{panel}");
        assert!(panel.contains("\"branches\":[\"s3\"]"), "{panel}");
        assert!(panel.contains("\"id\":\"s3\""), "{panel}");
        assert!(panel.contains("\"composition\":\"detail\""), "{panel}");
        assert!(panel.contains("\"notes\":\"wrap up\""), "{panel}");
        assert!(panel.contains("\"autoplay\":1"), "{panel}");
        let (code, launch) = body_str(port, "GET", "/api/studio/slideshow/present", "");
        assert_eq!(code, 200, "{launch}");
        assert!(launch.contains("framesmith present"), "{launch}");
    }));
    assert!(daemon::request_shutdown(port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(port), "port is quiet after shutdown");
    let entries = daemon::load_registry(&reg).unwrap();
    assert!(
        entries.iter().all(|e| e.dir != dir),
        "shutdown forgets the entry: {entries:?}"
    );
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn present_refuses_a_project_without_a_manifest() {
    let root = tmp("bare");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"solo\" root=true width=160 height=90 fps=10 duration=1 background=\"#101010\" version=1 {\n    text id=\"t\" value=\"Solo\" start=0 duration=1 x=10 y=30 size=16 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
    let reg = root.join("registry.json");
    let opts = PresentServeOptions {
        folder: folder.clone(),
        port: 0,
    };
    let code = run_present_serve(&opts, &reg);
    assert_ne!(code, 0, "missing manifest must refuse before serving");
    let _ = Path::new(&folder);
}
