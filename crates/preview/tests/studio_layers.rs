//! Full Layers panel regressions (Sprint 18, T004 / CAP-004, J-04,
//! STATE-02, INT-001, INT-002): the tree lists elements by z-order
//! with kind icons; drag reorder changes draw order visibly; group
//! collects, ungroup restores, and enter scopes selection to the
//! group; every structure change lands as a named history turn;
//! selections match across Layers, canvas, and timeline; empty
//! grouping and structure breaks refuse by name. Only validated turns
//! write the project (plus hidden studio state files); queue outputs
//! land outside sources like CLI renders; default renders stay
//! byte-identical.

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
        "framesmith-studio-layers-{}-{}-{}",
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

/// Flat paint band: three overlapping boxes (`a` white, `b` red, `c`
/// green, all `order` 0) under a text (`title`). Document order is
/// draw order; point (100, 50) reads the topmost cover.
fn layers_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    box id=\"a\" x=10 y=20 width=200 height=100 fill=\"#ffffff\" order=0 start=0 duration=2\n    box id=\"b\" x=50 y=30 width=200 height=100 fill=\"#ff0000\" order=0 start=0 duration=2\n    box id=\"c\" x=90 y=40 width=200 height=100 fill=\"#00ff00\" order=0 start=0 duration=2\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=150 size=24 color=\"#ffffff\"\n}\n",
    );
}

/// Z-index bands: `card` (order 2) over `panel` (order 1) over `title`
/// and `ghost` (order 0), like the canvas fixture.
fn banded_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" order=2 start=0 duration=2\n    box id=\"panel\" x=50 y=30 width=80 height=40 fill=\"#ff0000\" order=1 start=0 duration=2\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=150 size=24 color=\"#ffffff\"\n    box id=\"ghost\" x=10 y=20 width=20 height=20 fill=\"#00ff00\" order=0 hidden=true start=0 duration=2\n}\n",
    );
}

/// Motion-anchored: a tween drives clip index 1 (`b`), so any
/// document move shuffling index 1 refuses instead of detaching it.
fn motion_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    box id=\"a\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" order=0 start=0 duration=2\n    box id=\"b\" x=50 y=30 width=80 height=40 fill=\"#ff0000\" order=0 start=0 duration=2\n    tween target=1 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"linear\"\n}\n",
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
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"));
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

/// Every file under the project folder with its bytes, so a refused
/// action can prove it staged nothing anywhere.
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

fn layer_keys(value: &serde_json::Value) -> Vec<String> {
    value
        .get("layers")
        .and_then(|l| l.as_array())
        .expect("layers array")
        .iter()
        .map(|l| l.get("key").unwrap().as_str().unwrap().to_string())
        .collect()
}

fn frame(port: u16) -> Vec<u8> {
    let (code, png) = http(port, "GET", "/api/frame?i=0&action=step", None);
    assert_eq!(code, 200, "frame serves");
    png
}

fn ffmpeg() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH is set");
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("ffmpeg");
        if cand.is_file() {
            return cand;
        }
        #[cfg(windows)]
        {
            let cand = dir.join("ffmpeg.exe");
            if cand.is_file() {
                return cand;
            }
        }
    }
    panic!("tests need ffmpeg on PATH");
}

/// The effective composition the Studio readouts serve: on-disk
/// sources with the persisted SDK delta replayed, like the Layers
/// tree, canvas selection, and timeline rows.
fn effective(folder: &Path) -> notation::model::Composition {
    sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("valid open")
    .effective_composition()
    .clone()
}

/// One RGB pixel of frame 0 of `comp`, drawn straight through the
/// render pipeline with no session and no cache — the independent
/// oracle proving what the paint order really draws.
fn pixel_of(folder: &Path, comp: &notation::model::Composition, x: usize, y: usize) -> [u8; 3] {
    let prep = render::render::prepare_frames(comp, &[], folder, &ffmpeg()).expect("prepares");
    let drawn = render::render::draw_batch_parallel(&prep, render::render::Backend::Cpu, &[0])
        .expect("draws");
    let (_, rgb) = drawn.iter().find(|(k, _)| *k == 0).unwrap();
    let at = (y * 320 + x) * 3;
    [rgb[at], rgb[at + 1], rgb[at + 2]]
}

#[test]
fn layers_tree_lists_z_order_with_icons_and_syncs_selection() {
    with_server("tree", layers_project, |folder, _reg, _entry, port| {
        let sources = kdl_bytes(folder);
        let (code, j) = json(port, "GET", "/api/studio/layers?selection=a,ghost", None);
        assert_eq!(code, 200, "layers read: {j}");
        // Flat band: document order is draw order.
        assert_eq!(layer_keys(&j), vec!["a", "b", "c", "title"]);
        let layers = j.get("layers").unwrap().as_array().unwrap();
        let icons: Vec<&str> = layers
            .iter()
            .map(|l| l.get("icon").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(icons, vec!["■", "■", "■", "T"], "kind icons: {icons:?}");
        assert!(
            layers.iter().all(|l| l.get("group").unwrap().is_null()),
            "nothing grouped yet"
        );
        assert!(
            layers.iter().all(|l| l.get("in-scope").unwrap() == true),
            "no scope entered"
        );
        assert_eq!(j.get("groups").unwrap().as_array().unwrap().len(), 0);
        assert_eq!(j.get("scope").unwrap(), &serde_json::Value::Null);
        let line = j.get("status-line").unwrap().as_str().unwrap();
        assert!(line.contains("4 element(s)"), "{line}");
        assert!(line.contains("0 group(s)"), "{line}");
        // The shared selection echoes through, like canvas and timeline.
        assert_eq!(
            j.get("selection").unwrap().get("kept").unwrap(),
            &serde_json::json!(["a"])
        );
        assert_eq!(
            j.get("selection").unwrap().get("dropped").unwrap(),
            &serde_json::json!(["ghost"])
        );
        // Canvas resolves the same key the layers tree marks.
        let (code, selected) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","key":"a"}"#),
        );
        assert_eq!(code, 200, "canvas selects: {selected}");
        assert_eq!(
            selected.get("keys").unwrap(),
            &serde_json::json!(["a"]),
            "canvas and layers name the same key"
        );
        // Timeline rows address the same key too.
        let (code, rows) = body_str(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200, "{rows}");
        assert!(
            rows.contains("\"id\":\"a\""),
            "timeline names the key: {rows}"
        );
        assert_eq!(kdl_bytes(folder), sources, "reads write nothing");
    });
}

#[test]
fn layers_reorder_moves_draw_order_as_a_named_turn() {
    with_server("reorder", layers_project, |folder, _reg, _entry, port| {
        let sources = kdl_bytes(folder);
        // The covered point reads the topmost cover through the real
        // renderer: green while `c` paints last.
        assert_eq!(pixel_of(folder, &effective(folder), 100, 50), [0, 255, 0]);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"reorder","key":"c","to":0}"#),
        );
        assert_eq!(code, 200, "reorder commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        let summaries = j.get("summaries").unwrap().as_array().unwrap();
        assert_eq!(summaries.len(), 1, "{summaries:?}");
        assert_eq!(
            summaries[0].as_str().unwrap(),
            "move clip id \"c\" to index 0"
        );
        assert!(
            j.get("detail")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("layers reorder"),
            "named detail: {j}"
        );
        // Document order moves, so the flat-band draw order follows:
        // the readout is the paint order the tree and renderer share.
        let (code, j) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200, "{j}");
        assert_eq!(layer_keys(&j), vec!["c", "a", "b", "title"]);
        // The covered point now reads `b`: the reorder visibly changed
        // draw order through the real renderer.
        assert_eq!(pixel_of(folder, &effective(folder), 100, 50), [255, 0, 0]);
        assert_eq!(kdl_bytes(folder), sources, "sources stay byte-identical");
        // One named turn in the shared history.
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 1);
        assert_eq!(listing.turns[0].actor, "maker");
        assert!(
            listing.turns[0]
                .summary
                .contains("move clip id \"c\" to index 0"),
            "turn named: {}",
            listing.turns[0].summary
        );
    });
}

#[test]
fn layers_reorder_crosses_bands_with_order_assist() {
    with_server("bands", banded_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200, "{j}");
        assert_eq!(layer_keys(&j), vec!["title", "ghost", "panel", "card"]);
        // Drag `card` below `panel`: the drop row's order lands it
        // (before `panel` in document order with its paint order).
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"reorder","key":"card","to":0,"order":1}"#),
        );
        assert_eq!(code, 200, "cross-band reorder commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        let summaries: Vec<&str> = j
            .get("summaries")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        assert_eq!(summaries.len(), 2, "{summaries:?}");
        assert!(summaries[0].contains("z-index"), "{summaries:?}");
        assert!(
            summaries[1].contains("move clip id \"card\""),
            "{summaries:?}"
        );
        let (code, j) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200, "{j}");
        assert_eq!(layer_keys(&j), vec!["title", "ghost", "card", "panel"]);
        let card = j
            .get("layers")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l.get("key").unwrap() == "card")
            .unwrap();
        assert_eq!(card.get("order").unwrap(), &serde_json::json!(1));
    });
}

#[test]
fn layers_reorder_refuses_when_motion_breaks() {
    with_server("guard", motion_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"reorder","key":"a","to":1}"#),
        );
        assert_eq!(code, 409, "motion-breaking reorder refuses: {text}");
        assert!(text.contains("re-pointing"), "{text}");
        assert!(text.contains("tween 0"), "{text}");
        assert_eq!(snapshot(folder), before, "refusals stage nothing anywhere");
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 0, "no turn recorded");
        // Unknown keys refuse as missing targets.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"reorder","key":"ghost","to":0}"#),
        );
        assert_eq!(code, 404, "unknown key refuses: {text}");
        assert!(text.contains("names nothing"), "{text}");
    });
}

#[test]
fn layers_group_collects_ungroup_restores_as_named_turns() {
    with_server("group", layers_project, |folder, _reg, _entry, port| {
        let sources = kdl_bytes(folder);
        let before_png = frame(port);
        // Group notes ride render-neutrally: the covered point reads
        // green before, during, and after the group.
        let green = pixel_of(folder, &effective(folder), 100, 50);
        assert_eq!(green, [0, 255, 0]);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"group","keys":["a","b"],"name":"credits"}"#),
        );
        assert_eq!(code, 200, "group commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        assert_eq!(
            j.get("summaries").unwrap(),
            &serde_json::json!(["group clip id \"a\", clip id \"b\" as \"credits\""])
        );
        assert!(
            j.get("detail")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("turn 0"),
            "{j}"
        );
        let (code, j) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200, "{j}");
        assert_eq!(
            j.get("groups").unwrap(),
            &serde_json::json!([{"name":"credits","members":["a","b"]}])
        );
        let layers = j.get("layers").unwrap().as_array().unwrap();
        for key in ["a", "b"] {
            let row = layers
                .iter()
                .find(|l| l.get("key").unwrap() == key)
                .unwrap();
            assert_eq!(row.get("group").unwrap(), &serde_json::json!("credits"));
        }
        let line = j.get("status-line").unwrap().as_str().unwrap();
        assert!(line.contains("1 group(s)"), "{line}");
        // Group notes ride render-neutral: the covered point still
        // reads green through the real renderer.
        assert_eq!(pixel_of(folder, &effective(folder), 100, 50), green);
        assert_eq!(frame(port), before_png, "served frames stay stable");
        assert_eq!(kdl_bytes(folder), sources, "sources stay byte-identical");
        // Unlisting restores the members as its own named turn.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"ungroup","name":"credits"}"#),
        );
        assert_eq!(code, 200, "ungroup commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(1));
        assert_eq!(
            j.get("summaries").unwrap(),
            &serde_json::json!(["ungroup \"credits\""])
        );
        let (code, j) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200, "{j}");
        assert_eq!(j.get("groups").unwrap().as_array().unwrap().len(), 0);
        assert_eq!(pixel_of(folder, &effective(folder), 100, 50), green);
        assert_eq!(frame(port), before_png, "served frames stay stable");
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 2);
        assert!(listing.turns[0].summary.contains("credits"));
        assert!(listing.turns[1].summary.contains("ungroup"));
    });
}

#[test]
fn layers_group_empty_selection_noops_without_a_turn() {
    with_server("empty", layers_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"group","keys":[],"name":"credits"}"#),
        );
        assert_eq!(code, 200, "empty grouping reports: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::Value::Null);
        assert_eq!(j.get("summaries").unwrap().as_array().unwrap().len(), 0);
        assert!(
            j.get("detail")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("empty selection does nothing"),
            "{j}"
        );
        assert_eq!(
            snapshot(folder),
            before,
            "the no-op stages nothing anywhere"
        );
        let listing = sdk::history::list_turns(folder).expect("history lists");
        assert_eq!(listing.turns.len(), 0, "no turn recorded");
    });
}

#[test]
fn layers_group_refusals_name_causes() {
    with_server("refusals", layers_project, |folder, _reg, _entry, port| {
        // Unknown members refuse as missing targets.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"group","keys":["a","ghost"],"name":"credits"}"#),
        );
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("names no clip"), "{text}");
        // Collecting works once; collecting a member again refuses.
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"group","keys":["a"],"name":"credits"}"#),
        );
        assert_eq!(code, 200);
        let before = snapshot(folder);
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"group","keys":["a","b"],"name":"other"}"#),
        );
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("already grouped in"), "{text}");
        assert!(text.contains("\"credits\""), "{text}");
        assert_eq!(snapshot(folder), before, "refusals stage nothing anywhere");
        // Unknown groups refuse naming the holdings on release and entry.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"ungroup","name":"nope"}"#),
        );
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("names no grouped members"), "{text}");
        assert!(text.contains("\"credits\""), "{text}");
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"action":"enter","name":"nope"}"#),
        );
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("names no layer group"), "{text}");
        let (code, text) = body_str(port, "GET", "/api/studio/layers?scope=nope", None);
        assert_eq!(code, 404, "{text}");
        assert!(text.contains("names no layer group"), "{text}");
    });
}

#[test]
fn layers_enter_scopes_selection_across_views() {
    with_server("scope", layers_project, |_folder, _reg, _entry, port| {
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"group","keys":["a","b"],"name":"g"}"#),
        );
        assert_eq!(code, 200);
        // Entering echoes the scope without recording a turn.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"action":"enter","name":"g"}"#),
        );
        assert_eq!(code, 200, "enter validates: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::Value::Null);
        assert_eq!(j.get("scope").unwrap(), &serde_json::json!("g"));
        assert!(j
            .get("detail")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("2 member(s)"));
        // The scoped readout dims outsiders.
        let (code, j) = json(port, "GET", "/api/studio/layers?scope=g", None);
        assert_eq!(code, 200, "{j}");
        assert_eq!(j.get("scope").unwrap(), &serde_json::json!("g"));
        let layers = j.get("layers").unwrap().as_array().unwrap();
        for key in ["a", "b"] {
            let row = layers
                .iter()
                .find(|l| l.get("key").unwrap() == key)
                .unwrap();
            assert_eq!(row.get("in-scope").unwrap(), &serde_json::Value::Bool(true));
        }
        for key in ["c", "title"] {
            let row = layers
                .iter()
                .find(|l| l.get("key").unwrap() == key)
                .unwrap();
            assert_eq!(
                row.get("in-scope").unwrap(),
                &serde_json::Value::Bool(false)
            );
        }
        // Key selection outside the entered group refuses naming it.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","key":"c","scope":"g"}"#),
        );
        assert_eq!(code, 409, "{text}");
        assert!(text.contains("sits outside entered group"), "{text}");
        // Members still select, and the scoped layers echo agrees.
        let (code, selected) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","key":"a","scope":"g"}"#),
        );
        assert_eq!(code, 200, "{selected}");
        assert_eq!(selected.get("keys").unwrap(), &serde_json::json!(["a"]));
        let (code, j) = json(port, "GET", "/api/studio/layers?selection=a&scope=g", None);
        assert_eq!(code, 200, "{j}");
        let row = j
            .get("layers")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l.get("key").unwrap() == "a")
            .unwrap();
        assert_eq!(row.get("selected").unwrap(), &serde_json::Value::Bool(true));
        assert_eq!(row.get("in-scope").unwrap(), &serde_json::Value::Bool(true));
        // Exiting clears the scope with nothing recorded.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"action":"exit"}"#),
        );
        assert_eq!(code, 200, "{j}");
        assert_eq!(j.get("scope").unwrap(), &serde_json::Value::Null);
        assert_eq!(j.get("turn").unwrap(), &serde_json::Value::Null);
    });
}

#[test]
fn layers_group_undo_releases_through_shared_history() {
    with_server("undo", layers_project, |folder, _reg, _entry, port| {
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/layers",
            Some(r#"{"actor":"maker","action":"group","keys":["a","b"],"name":"g"}"#),
        );
        assert_eq!(code, 200, "{j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        // History undo is the one undo system: the group releases whole.
        sdk::history::undo_turn(folder, 0).expect("undo reverses the turn");
        let (code, j) = json(port, "GET", "/api/studio/layers", None);
        assert_eq!(code, 200, "{j}");
        assert_eq!(j.get("groups").unwrap().as_array().unwrap().len(), 0);
    });
}

#[test]
fn layers_parse_refusals_are_bad_requests() {
    with_server("parse", layers_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        for (body, needle) in [
            (
                r#"{"actor":"maker","action":"fly"}"#,
                "unknown layers action",
            ),
            (r#"{"action":"reorder","key":"a","to":0}"#, "names no actor"),
            (
                r#"{"actor":"maker","action":"reorder","key":"a","to":"x"}"#,
                "not a document index",
            ),
            (
                r#"{"actor":"maker","action":"group","keys":["a"]}"#,
                "names no `name`",
            ),
        ] {
            let (code, text) = body_str(port, "POST", "/api/studio/layers", Some(body));
            assert_eq!(code, 400, "{body}: {text}");
            assert!(text.contains(needle), "{body}: {text}");
        }
        assert_eq!(snapshot(folder), before, "parse errors stage nothing");
    });
}
