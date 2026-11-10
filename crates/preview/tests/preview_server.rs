//! Serving-server regressions (Sprint 14, T006): the `preview-serve`
//! child validates before binding, serves true frames / the real mix
//! / transport / reload over loopback HTTP, and shuts down cleanly
//! forgetting its registry entry.

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
        "framesmith-server-{}-{}-{}",
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
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Minimal HTTP exchange over loopback: returns (status code, body).
fn http(port: u16, method: &str, target: &str) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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

fn body_str(port: u16, method: &str, target: &str) -> (u16, String) {
    let (code, body) = http(port, method, target);
    (code, String::from_utf8_lossy(&body).into_owned())
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
    let entries = daemon::load_registry(&reg).unwrap();
    assert!(
        entries.iter().all(|e| e.dir != dir),
        "shutdown forgets the entry: {entries:?}"
    );
}

#[test]
fn status_names_project_composition_and_port() {
    with_server("status", text_project, |folder, _reg, entry, port| {
        let dir = daemon::canonical_dir(folder).unwrap();
        let (code, body) = body_str(port, "GET", "/api/status");
        assert_eq!(code, 200, "{body}");
        assert!(body.contains(&dir), "{body}");
        assert!(body.contains("\"composition\":\"t\""), "{body}");
        assert!(body.contains(&format!("\"port\":{}", entry.port)), "{body}");
        assert!(body.contains(&entry.url), "{body}");
        assert!(body.contains("\"frame_count\":10"), "{body}");
    });
}

#[test]
fn frames_serve_stable_png_and_time_into_latency() {
    with_server("frames", text_project, |_folder, _reg, _entry, port| {
        let (code_a, a) = http(port, "GET", "/api/frame?i=2");
        assert_eq!(code_a, 200);
        let (code_b, b) = http(port, "GET", "/api/frame?i=2&action=step");
        assert_eq!(code_b, 200);
        assert_eq!(a, b, "repeats serve identical bytes");
        assert!(a.len() > 8);
        assert_eq!(
            &a[..8],
            &[137, 80, 78, 71, 13, 10, 26, 10],
            "exact PNG bytes"
        );
        // The repeat filed a warm sample under the step bar.
        let (code, status) = body_str(port, "GET", "/api/status");
        assert_eq!(code, 200);
        assert!(status.contains("\"action\":\"step\""), "{status}");
        assert!(status.contains("\"warm_samples\":1"), "{status}");
        // Out of range refuses naming the valid window.
        let (code, bad) = body_str(port, "GET", "/api/frame?i=9999");
        assert_eq!(code, 404, "{bad}");
        assert!(bad.contains("outside"), "{bad}");
        assert!(bad.contains("10"), "{bad}");
    });
}

#[test]
fn transport_mix_and_page_serve_the_session() {
    with_server("session", text_project, |_folder, _reg, _entry, port| {
        let (code, scrub) = body_str(port, "POST", "/api/scrub?i=5");
        assert_eq!(code, 200, "{scrub}");
        assert!(scrub.contains("\"frame\":5"), "{scrub}");
        let (code, step) = body_str(port, "POST", "/api/step?d=1");
        assert_eq!(code, 200, "{step}");
        assert!(step.contains("\"frame\":6"), "{step}");
        // The mix clock owns the playhead: 0.5s at 10fps is frame 5.
        let (code, sync) = body_str(port, "POST", "/api/sync?time=0.5");
        assert_eq!(code, 200, "{sync}");
        assert!(sync.contains("\"frame\":5"), "{sync}");
        let (code, _) = body_str(port, "POST", "/api/pause");
        assert_eq!(code, 200);
        // Soundless project: silent video, no error, no fake track.
        let (code, wav) = http(port, "GET", "/api/mix.wav");
        assert_eq!(code, 204, "silent mix answers 204");
        assert!(wav.is_empty());
        // The review page names the composition and shows controls.
        let (code, page) = body_str(port, "GET", "/");
        assert_eq!(code, 200);
        assert!(page.contains("— t</"), "{page}");
        // Earlier scrub/step/sync calls moved the playhead to frame 5.
        assert!(page.contains("frame 5/10"), "{page}");
        assert!(page.contains("<img"), "{page}");
        assert!(page.contains("silent project"), "{page}");
        // Unknown routes refuse instead of guessing.
        let (code, missing) = body_str(port, "GET", "/api/nope");
        assert_eq!(code, 404, "{missing}");
    });
}

#[test]
fn hot_reload_applies_the_new_look_then_keeps_last_good() {
    with_server("reload", text_project, |folder, _reg, _entry, port| {
        let (_, before) = http(port, "GET", "/api/frame?i=0");
        // A valid edit applies hot: new bytes, reload reported.
        write(
            folder,
            "composition.kdl",
            "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#aabbcc\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
        );
        let (code, status) = body_str(port, "GET", "/api/status");
        assert_eq!(code, 200);
        assert!(status.contains("reloaded"), "{status}");
        let (_, after) = http(port, "GET", "/api/frame?i=0");
        assert_ne!(before, after, "the new look serves fresh bytes");
        // An explicit reload with no changes still applies cleanly.
        let (code, reload) = body_str(port, "POST", "/api/reload");
        assert_eq!(code, 200, "{reload}");
        assert!(reload.contains("\"applied\":true"), "{reload}");
        // A broken edit keeps the last good look with diagnostics.
        write(folder, "composition.kdl", "composition {{{ not kdl\n");
        let (_, kept) = http(port, "GET", "/api/frame?i=0");
        assert_eq!(kept, after, "last good frames keep showing");
        let (code, status) = body_str(port, "GET", "/api/status");
        assert_eq!(code, 200);
        assert!(status.contains("last good look"), "{status}");
    });
}

#[test]
fn broken_project_refuses_before_any_server_starts() {
    let root = tmp("refuse");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("composition.kdl"), "composition {{{ not kdl\n").unwrap();
    let reg = root.join("registry.json");
    let code = run_serve(&ServeOptions {
        folder: folder.clone(),
        port: 0,
        registry: reg.clone(),
        composition: None,
    });
    assert_eq!(code, 1, "refusal exits 1");
    let entries = daemon::load_registry(&reg).unwrap_or_default();
    assert!(
        entries.is_empty(),
        "a refusal records no server: {entries:?}"
    );
}
