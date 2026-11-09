//! Crop mode regressions (Sprint 17, T004 / CAP-004, J-02,
//! STATE-02, INT-001, INT-003, REC-001..003): Free, 16:9, 9:16, 1:1,
//! and 4:5 presets constrain the crop frame with live preview;
//! committing writes exactly one patch turn and cancelling leaves
//! nothing staged; an un-fittable preset clamps showing the clamped
//! frame; crop changes only the visible window, never the media. Only
//! validated turns write the project (plus hidden studio state files);
//! default renders stay byte-identical.

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
        "framesmith-studio-crop-{}-{}-{}",
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

/// Two boxes, one text with a try-order font list, one hidden box,
/// and one unreferenced media file: 10 fps, 40 frames, 320x180.
fn crop_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" order=2 start=0 duration=4\n    box id=\"panel\" x=50 y=30 width=80 height=40 fill=\"#ff0000\" order=1 start=0 duration=4\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=150 size=24 color=\"#ffffff\"\n    box id=\"ghost\" x=10 y=20 width=20 height=20 fill=\"#00ff00\" hidden=true start=0 duration=4\n}\n",
    );
    std::fs::write(folder.join("media.bin"), vec![7u8; 4096]).unwrap();
}

/// Project with an unresolvable font-family list: headless SDK
/// sessions open it (fail-closed resolution stays at render time),
/// so the try-order pass-through check runs without a server.
fn font_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=40 size=24 color=\"#ffffff\" font-family=\"Nope, Fallback\"\n    box id=\"card\" x=10 y=100 width=60 height=30 fill=\"#ffffff\" start=0 duration=4\n}\n",
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
        if start.elapsed() > Duration::from_secs(15) {
            panic!("preview server never registered");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let port = entry.port;
    f(&folder, &reg, entry, port);
    let (code, _) = body_str(port, "POST", "/api/shutdown", None);
    assert_eq!(code, 200, "server shuts down");
    handle.join().expect("server thread");
}

fn kdl_bytes(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("composition.kdl")).expect("sources stay readable")
}

/// Every file under the project folder with its bytes, so a
/// refused, previewed, or cancelled crop can prove it staged nothing
/// anywhere.
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

fn frame_bytes(port: u16, i: u64) -> Vec<u8> {
    let (code, bytes) = http(port, "GET", &format!("/api/frame?i={i}&action=step"), None);
    assert_eq!(code, 200, "frame {i} serves");
    bytes
}

#[test]
fn crop_each_preset_constrains_frame_with_live_preview() {
    with_server("presets", crop_project, |folder, _reg, _entry, port| {
        let before_kdl = kdl_bytes(folder);
        let before_snap = snapshot(folder);
        let before_frame = frame_bytes(port, 0);
        // (preset, expected aspect or None for free)
        for (preset, aspect) in [
            ("free", None),
            ("16:9", Some(16.0 / 9.0)),
            ("9:16", Some(9.0 / 16.0)),
            ("1:1", Some(1.0)),
            ("4:5", Some(4.0 / 5.0)),
        ] {
            let (code, j) = json(
                port,
                "POST",
                "/api/studio/crop",
                Some(&format!(
                    r#"{{"actor":"maker","target":{{"id":"card"}},"preset":"{preset}"}}"#
                )),
            );
            assert_eq!(code, 200, "preset {preset} previews: {j}");
            assert_eq!(j.get("committed").unwrap(), &serde_json::json!(false));
            assert_eq!(j.get("turn").unwrap(), &serde_json::Value::Null);
            assert_eq!(
                j.get("summaries").unwrap(),
                &serde_json::json!([]),
                "preview records no turn"
            );
            let frame = j.get("applied-frame").expect("staged frame");
            let (w, h) = (
                frame.get("width").unwrap().as_f64().unwrap(),
                frame.get("height").unwrap().as_f64().unwrap(),
            );
            assert!(w > 0.0 && h > 0.0, "positive frame: {frame}");
            if let Some(a) = aspect {
                let got = w / h;
                assert!(
                    (got - a).abs() < 1e-6,
                    "preset {preset} constrains the frame: {w}x{h}"
                );
            } else {
                assert_eq!((w, h), (100.0, 50.0), "free keeps the source rect");
            }
            // The staged frame stays inside the card source rect.
            let (x, y) = (
                frame.get("x").unwrap().as_f64().unwrap(),
                frame.get("y").unwrap().as_f64().unwrap(),
            );
            assert!(
                x >= 10.0 - 1e-9 && y >= 20.0 - 1e-9,
                "frame origin: {frame}"
            );
            assert!(
                x + w <= 110.0 + 1e-9 && y + h <= 70.0 + 1e-9,
                "frame inside source: {frame}"
            );
        }
        assert_eq!(
            kdl_bytes(folder),
            before_kdl,
            "previews stage no source write"
        );
        assert_eq!(
            snapshot(folder),
            before_snap,
            "previews stage nothing anywhere"
        );
        assert_eq!(
            frame_bytes(port, 0),
            before_frame,
            "default renders stay byte-identical"
        );
    });
}

#[test]
fn crop_commit_writes_exactly_one_patch_turn() {
    with_server("commit", crop_project, |folder, _reg, _entry, port| {
        let before_kdl = kdl_bytes(folder);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"actor":"maker","target":{"id":"card"},"preset":"16:9","commit":true}"#),
        );
        assert_eq!(code, 200, "crop commits: {j}");
        assert_eq!(j.get("saved").unwrap(), &serde_json::Value::Bool(true));
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        assert_eq!(j.get("applied").unwrap(), &serde_json::json!(4));
        let summaries = j.get("summaries").unwrap().as_array().unwrap();
        assert_eq!(summaries.len(), 4, "one frame turn: {summaries:?}");
        for property in ["`x`", "`y`", "`width`", "`height`"] {
            assert!(
                summaries
                    .iter()
                    .any(|s| s.as_str().unwrap().contains(property)),
                "turn rewrites {property}: {summaries:?}"
            );
        }
        assert!(
            summaries
                .iter()
                .all(|s| s.as_str().unwrap().contains("card")),
            "turn names its clip: {summaries:?}"
        );
        let detail = j.get("detail").unwrap().as_str().unwrap();
        assert!(
            detail.contains("crop card to 16:9"),
            "named detail: {detail}"
        );
        assert!(detail.contains("turn 0"), "turn named: {detail}");
        // The effective composition shows the cropped window; sources
        // stay byte-identical.
        let (code, layers) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200);
        let card = layers
            .get("layers")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l.get("key").unwrap() == "card")
            .unwrap();
        let frame = j.get("applied-frame").unwrap();
        let card_box = card.get("box").unwrap().as_array().unwrap();
        for (i, key) in ["x", "y", "width", "height"].iter().enumerate() {
            let want = frame.get(*key).unwrap().as_f64().unwrap();
            let got = card_box[i].as_f64().unwrap();
            assert!(
                (got - want).abs() < 2e-3,
                "layers show the committed frame: {key} {got} vs {want}"
            );
        }
        assert_eq!(kdl_bytes(folder), before_kdl, "sources stay byte-identical");
        // A second commit records exactly one more turn.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"actor":"maker","target":{"id":"card"},"preset":"1:1","commit":true}"#),
        );
        assert_eq!(code, 200, "second crop commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(1));
        assert_eq!(kdl_bytes(folder), before_kdl, "sources still identical");
    });
}

#[test]
fn crop_cancel_stages_nothing() {
    with_server("cancel", crop_project, |folder, _reg, _entry, port| {
        let before_kdl = kdl_bytes(folder);
        let before_snap = snapshot(folder);
        let (code, layers_before) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/crop",
            Some(
                r#"{"actor":"maker","target":{"id":"card"},"preset":"9:16","frame":{"x":0,"y":0,"width":90,"height":40},"cancelled":true}"#,
            ),
        );
        assert_eq!(code, 200, "crop cancels: {j}");
        assert_eq!(j.get("cancelled").unwrap(), &serde_json::Value::Bool(true));
        assert_eq!(
            j.get("turn").unwrap_or(&serde_json::Value::Null),
            &serde_json::Value::Null
        );
        let detail = j.get("detail").unwrap().as_str().unwrap();
        assert!(
            detail.contains("nothing was recorded"),
            "cancel says so: {detail}"
        );
        let (code, layers_after) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200);
        assert_eq!(layers_after, layers_before, "geometry untouched");
        assert_eq!(kdl_bytes(folder), before_kdl, "sources untouched");
        assert_eq!(snapshot(folder), before_snap, "nothing staged anywhere");
    });
}

#[test]
fn crop_unfittable_preset_clamps_showing_clamped_frame() {
    with_server("clamp", crop_project, |folder, _reg, _entry, port| {
        let before_kdl = kdl_bytes(folder);
        // A 1000x1000 drag cannot fit the 100x50 card: the frame
        // clamps to the 9:16 frame the source holds.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/crop",
            Some(
                r#"{"actor":"maker","target":{"id":"card"},"preset":"9:16","frame":{"x":0,"y":0,"width":1000,"height":1000}}"#,
            ),
        );
        assert_eq!(code, 200, "un-fittable previews: {j}");
        assert_eq!(j.get("clamped").unwrap(), &serde_json::Value::Bool(true));
        let frame = j.get("applied-frame").unwrap();
        let (x, y, w, h) = (
            frame.get("x").unwrap().as_f64().unwrap(),
            frame.get("y").unwrap().as_f64().unwrap(),
            frame.get("width").unwrap().as_f64().unwrap(),
            frame.get("height").unwrap().as_f64().unwrap(),
        );
        assert!((w / h - 9.0 / 16.0).abs() < 1e-6, "aspect holds: {w}x{h}");
        assert!(
            x >= 10.0 - 1e-9 && y >= 20.0 - 1e-9,
            "origin inside: {frame}"
        );
        assert!(
            x + w <= 110.0 + 1e-9 && y + h <= 70.0 + 1e-9,
            "frame inside source: {frame}"
        );
        let notes = j
            .get("notes")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(notes.contains("clamped"), "the clamp shows: {notes}");
        // Committing records the clamped result as one turn.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/crop",
            Some(
                r#"{"actor":"maker","target":{"id":"card"},"preset":"9:16","frame":{"x":0,"y":0,"width":1000,"height":1000},"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "clamped crop commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        assert_eq!(j.get("clamped").unwrap(), &serde_json::Value::Bool(true));
        let (code, layers) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200);
        let card = layers
            .get("layers")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l.get("key").unwrap() == "card")
            .unwrap();
        let card_box = card.get("box").unwrap().as_array().unwrap();
        assert!((card_box[2].as_f64().unwrap() - w).abs() < 1e-6);
        assert!((card_box[3].as_f64().unwrap() - h).abs() < 1e-6);
        assert_eq!(kdl_bytes(folder), before_kdl, "sources stay byte-identical");
    });
}

#[test]
fn crop_changes_only_the_visible_window_never_media() {
    with_server("media", crop_project, |folder, _reg, _entry, port| {
        let media_before = std::fs::read(folder.join("media.bin")).unwrap();
        let outside_before: Vec<(PathBuf, Vec<u8>)> = snapshot(folder)
            .into_iter()
            .filter(|(rel, _)| !rel.starts_with(".framesmith"))
            .collect();
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"actor":"maker","target":{"id":"card"},"preset":"4:5","commit":true}"#),
        );
        assert_eq!(code, 200, "crop commits: {j}");
        // The turn rewrites geometry rows only: no source, fit, or
        // placement row rides along, and the media file is untouched.
        let summaries = j.get("summaries").unwrap().as_array().unwrap();
        for s in summaries {
            let text = s.as_str().unwrap();
            assert!(
                ["`x`", "`y`", "`width`", "`height`"]
                    .iter()
                    .any(|p| text.contains(p)),
                "geometry rows only: {text}"
            );
            assert!(
                !text.contains("source") && !text.contains("object-fit"),
                "media rows untouched: {text}"
            );
        }
        assert_eq!(
            std::fs::read(folder.join("media.bin")).unwrap(),
            media_before,
            "media bytes untouched"
        );
        // Only validated turns write: every file outside the
        // hidden studio state directory stays byte-identical.
        let outside_after: Vec<(PathBuf, Vec<u8>)> = snapshot(folder)
            .into_iter()
            .filter(|(rel, _)| !rel.starts_with(".framesmith"))
            .collect();
        assert_eq!(
            outside_after, outside_before,
            "project writes stay in validated turns plus hidden studio state"
        );
    });
}

#[test]
fn crop_keeps_font_family_try_order_untouched() {
    // Headless: the authored try-order list rides through the shared
    // machinery in written comma order, never resolved or
    // substituted, while the crop lands its own turn.
    let root = tmp("fonts");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    font_project(&folder);
    let shown =
        preview::studio::effective_composition_for_session(&folder, None).expect("effective opens");
    assert_eq!(
        preview::studio_canvas::canvas_font_lists_of(&shown),
        vec!["Nope, Fallback".to_string()],
    );
    let req = preview::studio_crop::parse_crop_body(
        r#"{"actor":"maker","target":{"id":"card"},"preset":"free","commit":true}"#,
    )
    .expect("crop parses");
    let (proposed, clamped, _) =
        preview::studio_crop::propose_crop(&shown, &req, 320.0, 180.0).expect("crop proposes");
    assert!(!clamped, "free keeps the whole source rect");
    let receipt =
        preview::studio_crop::apply_crop(&folder, None, &req, &proposed).expect("crop applies");
    assert_eq!(receipt.turn, 0, "the crop lands as a named turn");
    assert_eq!(receipt.applied, 4, "one frame turn");
    let shown =
        preview::studio::effective_composition_for_session(&folder, None).expect("effective opens");
    assert_eq!(
        preview::studio_canvas::canvas_font_lists_of(&shown),
        vec!["Nope, Fallback".to_string()],
        "the list keeps written order through the turn"
    );
}

#[test]
fn crop_refusals_name_the_cause_and_stage_nothing() {
    with_server("refusals", crop_project, |folder, _reg, _entry, port| {
        let before_snap = snapshot(folder);
        // Unknown preset: 400.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"actor":"maker","target":{"id":"card"},"preset":"21:9"}"#),
        );
        assert_eq!(code, 400, "unknown preset refuses: {text}");
        assert!(
            text.contains("unknown crop preset"),
            "names the rule: {text}"
        );
        // Unknown target: 404.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"actor":"maker","target":{"id":"nobody"},"preset":"free"}"#),
        );
        assert_eq!(code, 404, "unknown target refuses: {text}");
        assert!(text.contains("names nothing"), "names the target: {text}");
        // Text carries no crop window: 409 with rule and location.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"actor":"maker","target":{"id":"title"},"preset":"1:1"}"#),
        );
        assert_eq!(code, 409, "unsized kind refuses: {text}");
        assert!(text.contains("sized clips"), "names the rule: {text}");
        // Hidden clips show no crop frame: 409.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"actor":"maker","target":{"id":"ghost"},"preset":"1:1"}"#),
        );
        assert_eq!(code, 409, "hidden refuses: {text}");
        assert!(text.contains("no crop frame"), "names the lock: {text}");
        // Commits are named: no actor, no turn.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/crop",
            Some(r#"{"target":{"id":"card"},"preset":"free","commit":true}"#),
        );
        assert_eq!(code, 400, "anonymous commit refuses: {text}");
        assert!(text.contains("names no actor"), "names the actor: {text}");
        // Empty bodies refuse.
        let (code, text) = body_str(port, "POST", "/api/studio/crop", Some(""));
        assert_eq!(code, 400, "empty body refuses: {text}");
        assert_eq!(snapshot(folder), before_snap, "refusals stage nothing");
    });
}

#[test]
fn crop_selection_stays_in_sync() {
    with_server("sync", crop_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/crop",
            Some(
                r#"{"actor":"maker","target":{"id":"card"},"preset":"free","commit":true,"selection":"card,nobody"}"#,
            ),
        );
        assert_eq!(code, 200, "crop commits: {j}");
        assert_eq!(
            j.get("selection").unwrap().get("kept").unwrap(),
            &serde_json::json!(["card"]),
            "canvas keeps the cropped clip"
        );
        assert_eq!(
            j.get("selection").unwrap().get("dropped").unwrap(),
            &serde_json::json!(["nobody"]),
            "timeline drops the unknown key"
        );
    });
}

#[test]
fn crop_page_hosts_crop_bar_with_hyphenated_styles() {
    with_server("page", crop_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        for id in [
            "crop-bar",
            "crop-presets",
            "crop-commit",
            "crop-cancel",
            "crop-status",
        ] {
            assert!(page.contains(id), "the page hosts {id}");
        }
        for preset in ["free", "16:9", "9:16", "1:1", "4:5"] {
            assert!(
                page.contains(&format!("data-preset=\"{preset}\"")) || page.contains(preset),
                "the page offers {preset}"
            );
        }
        for style in ["background-color", "border-radius", "font-variant-numeric"] {
            assert!(
                page.contains(style),
                "styles stay CSS-like hyphenated: {style}"
            );
        }
    });
}
