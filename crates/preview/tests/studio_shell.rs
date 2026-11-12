//! Studio shell regressions (Sprint 16, T001 / CAP-001, J-01,
//! STATE-02, REC-003): the `/studio` page grown from the preview and
//! player pages hosts dockable preview and timeline panels, saves the
//! arrangement per project, restores it on reopen, reopens deep links,
//! and falls back to the default arrangement and the project root with
//! banners.

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
        "framesmith-studio-{}-{}-{}",
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

fn layout_body(dock_preview: &str, dock_timeline: &str) -> String {
    format!(
        "{{\"version\":1,\"panels\":{{\"preview\":{{\"dock\":\"{dock_preview}\"}},\"timeline\":{{\"dock\":\"{dock_timeline}\"}}}}}}"
    )
}

#[test]
fn studio_opens_with_preview_and_timeline_panels() {
    with_server("open", text_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200, "{page}");
        assert!(page.contains("id=\"panel-preview\""), "{page}");
        assert!(page.contains("id=\"panel-timeline\""), "{page}");
        assert!(page.contains("<img id=\"frame\""), "{page}");
        assert!(page.contains("id=\"timeline-rows\""), "{page}");
        assert!(page.contains("id=\"banner\" hidden"), "{page}");
        assert!(page.contains("readout"), "{page}");
        assert!(page.contains("dock main"), "{page}");
        assert!(page.contains("reset layout"), "{page}");
    });
}

#[test]
fn studio_panels_dock_float_group_maximise_close_and_reset() {
    with_server("panels", text_project, |folder, _reg, _entry, port| {
        // Dock each panel somewhere else and persist.
        let (code, saved) = body_str(
            port,
            "POST",
            "/api/studio/layout",
            Some(&layout_body("left", "right")),
        );
        assert_eq!(code, 200, "{saved}");
        assert!(saved.contains("\"saved\":true"), "{saved}");
        let (code, got) = body_str(port, "GET", "/api/studio/layout", None);
        assert_eq!(code, 200, "{got}");
        assert!(got.contains("\"restored\":true"), "{got}");
        assert!(got.contains("\"dock\":\"left\""), "{got}");
        assert!(got.contains("\"dock\":\"right\""), "{got}");
        // The page paints the saved arrangement.
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("layout restored"), "{page}");

        // Float with a rectangle, group, maximise, close.
        let floating = "{\"version\":1,\"panels\":{\"preview\":{\"dock\":\"float\",\"float_rect\":[40,40,480,320]},\"timeline\":{\"dock\":\"main\",\"group\":\"preview\",\"maximized\":true}},\"order\":[\"timeline\",\"preview\"]}";
        let (code, saved) = body_str(port, "POST", "/api/studio/layout", Some(floating));
        assert_eq!(code, 200, "{saved}");
        let (code, got) = body_str(port, "GET", "/api/studio/layout", None);
        assert_eq!(code, 200, "{got}");
        assert!(got.contains("\"dock\":\"float\""), "{got}");
        assert!(got.contains("\"group\":\"preview\""), "{got}");
        assert!(got.contains("\"maximized\":true"), "{got}");

        // Close the timeline; the page hides it.
        let closed = "{\"version\":1,\"panels\":{\"preview\":{\"dock\":\"main\"},\"timeline\":{\"dock\":\"bottom\",\"visible\":false}},\"order\":[\"preview\",\"timeline\"]}";
        let (code, _) = body_str(port, "POST", "/api/studio/layout", Some(closed));
        assert_eq!(code, 200);
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(
            page.contains("data-hidden=\"1\"") || page.contains("\"visible\":false"),
            "{page}"
        );

        // Reset reopens the default arrangement.
        let (code, reset) = body_str(port, "POST", "/api/studio/reset", None);
        assert_eq!(code, 200, "{reset}");
        assert!(reset.contains("\"restored\":false"), "{reset}");
        assert!(reset.contains("\"dock\":\"main\""), "{reset}");
        assert!(reset.contains("\"dock\":\"bottom\""), "{reset}");
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("layout default"), "{page}");
        // The reset removed the file from the hidden history dir.
        assert!(
            !studio::studio_layout_path_for(folder).exists(),
            "reset forgets the saved file"
        );
    });
}

#[test]
fn studio_layout_saves_per_project_and_restores_on_reopen() {
    let root = tmp("per-project");
    for name in ["one", "two"] {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        text_project(&folder);
    }
    // Arrange project one differently.
    let one = root.join("one");
    let load = studio::load_layout(&one);
    assert!(!load.restored);
    let saved = studio::save_layout(&one, &layout_body("left", "right")).unwrap();
    assert_eq!(saved.panels["preview"].dock, "left");
    // Project two still opens the default.
    let two = root.join("two");
    let load = studio::load_layout(&two);
    assert!(!load.restored);
    assert_eq!(load.layout, studio::default_layout());
    // Project one restores its own arrangement.
    let load = studio::load_layout(&one);
    assert!(load.restored);
    assert!(load.notice.is_none());
    assert_eq!(load.layout.panels["preview"].dock, "left");
    assert_eq!(load.layout.panels["timeline"].dock, "right");
    // The layout file lives in the hidden history dir, never beside
    // the sources.
    assert_eq!(
        studio::studio_layout_path_for(&one),
        one.join(".framesmith").join("studio-layout.json")
    );
    assert!(one.join("composition.kdl").is_file());
}

#[test]
fn studio_deep_link_reopens_composition_time_and_selection() {
    with_server("deeplink", text_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(
            port,
            "GET",
            "/studio?composition=t&time=0.5&selection=title",
            None,
        );
        assert_eq!(code, 200, "{page}");
        assert!(page.contains("— t</h1>"), "{page}");
        assert!(page.contains("id=\"banner\" hidden"), "{page}");
        assert!(page.contains("/api/frame?i=5"), "{page}");
        assert!(page.contains("selection: title"), "{page}");
        // A frame deep link parks on the same frame.
        let (code, by_frame) = body_str(port, "GET", "/studio?composition=t&frame=5", None);
        assert_eq!(code, 200);
        assert!(by_frame.contains("/api/frame?i=5"), "{by_frame}");
        // The timeline endpoint lists the validated rows read-only.
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline?composition=t", None);
        assert_eq!(code, 200, "{rows}");
        assert!(rows.contains("\"composition\":\"t\""), "{rows}");
        assert!(rows.contains("\"kind\":\"text\""), "{rows}");
        assert!(rows.contains("\"id\":\"title\""), "{rows}");
        assert!(rows.contains("\"media_start\""), "{rows}");
    });
}

#[test]
fn studio_corrupt_layout_falls_back_to_default_with_banner() {
    with_server("corrupt", text_project, |folder, _reg, _entry, port| {
        let path = studio::studio_layout_path_for(folder);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not a layout").unwrap();
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("id=\"banner\" role=\"alert\""), "{page}");
        assert!(page.contains("default arrangement"), "{page}");
        assert!(page.contains("layout default"), "{page}");
        // Painting the fallback never deletes the saved file.
        assert!(path.is_file(), "fallback keeps the saved file intact");
        // A layout naming no hosted panel falls back the same way.
        std::fs::write(&path, "{\"version\":1,\"panels\":{},\"order\":[]}").unwrap();
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        assert!(page.contains("default arrangement"), "{page}");
    });
}

#[test]
fn studio_missing_composition_lands_on_project_root_with_banner() {
    with_server(
        "missing-comp",
        text_project,
        |_folder, _reg, _entry, port| {
            let (code, page) = body_str(port, "GET", "/studio?composition=nope", None);
            assert_eq!(code, 200, "{page}");
            assert!(page.contains("— t</h1>"), "{page}");
            assert!(page.contains("id=\"banner\" role=\"alert\""), "{page}");
            assert!(page.contains("unknown composition"), "{page}");
            assert!(page.contains("project root"), "{page}");
            // The timeline API refuses the same unknown name instead of
            // guessing rows.
            let (code, missing) =
                body_str(port, "GET", "/api/studio/timeline?composition=nope", None);
            assert_eq!(code, 404, "{missing}");
            assert!(missing.contains("unknown composition"), "{missing}");
            assert!(missing.contains("project root"), "{missing}");
        },
    );
}

#[test]
fn studio_layout_refusals_name_the_cause_and_save_nothing() {
    with_server("refuse", text_project, |folder, _reg, _entry, port| {
        for (label, bad) in [
            ("empty", ""),
            ("garbage", "{ nope"),
            ("version", "{\"version\":9,\"panels\":{\"preview\":{\"dock\":\"main\"},\"timeline\":{\"dock\":\"bottom\"}},\"order\":[]}"),
            ("panel", "{\"version\":1,\"panels\":{\"preview\":{\"dock\":\"main\"},\"nope\":{\"dock\":\"main\"}},\"order\":[]}"),
            ("dock", "{\"version\":1,\"panels\":{\"preview\":{\"dock\":\"up\"},\"timeline\":{\"dock\":\"bottom\"}},\"order\":[]}"),
            ("group", "{\"version\":1,\"panels\":{\"preview\":{\"dock\":\"main\",\"group\":\"nope\"},\"timeline\":{\"dock\":\"bottom\"}},\"order\":[]}"),
        ] {
            let (code, refused) = body_str(port, "POST", "/api/studio/layout", Some(bad));
            assert_eq!(code, 400, "{label}: {refused}");
            assert!(refused.contains("studio refused"), "{label}: {refused}");
        }
        // Every refusal left the previous arrangement intact: no file.
        assert!(
            !studio::studio_layout_path_for(folder).exists(),
            "refused saves write nothing"
        );
        // An out-of-range deep-link time clamps with a banner.
        let (code, page) = body_str(port, "GET", "/studio?composition=t&time=99", None);
        assert_eq!(code, 200);
        assert!(page.contains("id=\"banner\" role=\"alert\""), "{page}");
        assert!(page.contains("/api/frame?i=19"), "{page}");
        // An unknown selection entry drops with a banner.
        let (code, page) = body_str(port, "GET", "/studio?composition=t&selection=nope", None);
        assert_eq!(code, 200);
        assert!(page.contains("dropping it"), "{page}");
    });
}
