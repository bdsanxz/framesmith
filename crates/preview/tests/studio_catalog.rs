//! Sprint 19 T005: the Studio catalog panel over the shared
//! offline word index.
//!
//! `GET /studio/catalog` serves the human panel page (fully
//! offline: no CDN, no model); `GET
//! /api/studio/catalog/search?q=WORDS` ranks through the same
//! merged [`catalog::SearchIndex`] the CLI picker uses, so the
//! panel and the CLI agree on order. A missing query refuses with
//! 400; an empty registry reports itself; a miss carries the
//! search-miss offer.

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
        "framesmith-studio-catalog-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_manifest(
    dir: &Path,
    name: &str,
    kind: &str,
    title: &str,
    description: &str,
    tags: &[&str],
) {
    let tags_json = tags
        .iter()
        .map(|t| format!("{t:?}"))
        .collect::<Vec<_>>()
        .join(",");
    let dims = if kind == "component" {
        String::new()
    } else {
        r#","width": 1280, "height": 720, "duration": 4"#.to_string()
    };
    let text = format!(
        r#"{{"name": {name:?}, "type": {kind:?}, "title": {title:?}, "description": {description:?},
            "files": [{{"src": "composition.kdl", "target": "composition.kdl"}}]{dims}, "tags": [{tags_json}]}}"#
    );
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), text).unwrap();
}

/// Fixture registry shared by every server test in this file (one
/// fixed path per process, identical bytes): `sunset` matches in
/// name, title, tags, and description in that order — the same
/// order the CLI picker prints.
fn fixture_registry() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-catalog-fixture-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    write_manifest(
        &dir,
        "sunset-glow",
        "block",
        "Harbor Evening",
        "An original amber composition over still water.",
        &["evenings"],
    );
    write_manifest(
        &dir,
        "dawn-chorus",
        "component",
        "Sunset Accents",
        "A short snippet with soft edges.",
        &["accents"],
    );
    write_manifest(
        &dir,
        "meadow-walk",
        "block",
        "Meadow Walk",
        "A gentle stroll past painted fences.",
        &["sunset"],
    );
    write_manifest(
        &dir,
        "pebble-sort",
        "component",
        "Pebble Sort",
        "A quiet sunset shimmer for backgrounds.",
        &["shimmer"],
    );
    std::env::set_var("FRAMESMITH_REGISTRY", &dir);
    dir
}

fn text_project(folder: &Path) {
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

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

fn with_server(f: impl FnOnce(u16)) {
    let _registry = fixture_registry();
    let root = tmp("server");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
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
    f(entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
}

#[test]
fn panel_page_serves_offline_search_box() {
    with_server(|port| {
        let (code, body) = body_str(port, "GET", "/studio/catalog");
        assert_eq!(code, 200, "{body}");
        assert!(body.contains("catalog-search"), "{body}");
        assert!(body.contains("/api/studio/catalog/search"), "{body}");
        assert!(
            !body.contains("http://") && !body.contains("https://"),
            "{body}"
        );
    });
}

#[test]
fn panel_search_matches_cli_picker_order() {
    with_server(|port| {
        let (code, body) = body_str(port, "GET", "/api/studio/catalog/search?q=sunset");
        assert_eq!(code, 200, "{body}");
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        let order: Vec<String> = value["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            order,
            vec!["sunset-glow", "dawn-chorus", "meadow-walk", "pebble-sort"],
            "panel shares the CLI ranking: {body}"
        );
        assert_eq!(value["empty"], false);
    });
}

#[test]
fn panel_search_refuses_missing_query_and_offers_misses() {
    with_server(|port| {
        let (code, body) = body_str(port, "GET", "/api/studio/catalog/search");
        assert_eq!(code, 400, "{body}");
        assert!(body.contains("?q=WORDS"), "{body}");
        let (code, body) = body_str(port, "GET", "/api/studio/catalog/search?q=xylophone");
        assert_eq!(code, 200, "{body}");
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["hits"].as_array().unwrap().len(), 0);
        assert!(
            value["report_offer"]
                .as_str()
                .unwrap()
                .contains("report-miss"),
            "{body}"
        );
    });
}
