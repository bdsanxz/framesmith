//! Canvas selection and transform regressions (Sprint 17, T002 /
//! CAP-002, J-01, J-06, INT-001, INT-003, REC-001..003): click,
//! Shift-add, and marquee select the intended elements; resize and
//! rotate handles snap at 15 degrees with Shift; axis lock drops
//! the other delta; group moves keep offsets; arrows nudge 1 px
//! (10 with Shift); off-canvas elements show indicators; the layers
//! readout lists elements by z-order and selects from it; canvas
//! and timeline selections match; every applied change lands as a
//! named history turn with a receipt; refused, cancelled, or
//! clamped gestures behave per the specification with causes
//! shown. Only validated turns write the project (plus hidden
//! studio state files); default renders stay byte-identical.

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
        "framesmith-studio-select-{}-{}-{}",
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

/// Two boxes (`card` order 2, `panel` order 1, overlapping), one
/// text (`title`, no authored extents), and one hidden box
/// (`ghost` over `card`): 10 fps, 40 frames, 320x180.
fn select_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" order=2 start=0 duration=4\n    box id=\"panel\" x=50 y=30 width=80 height=40 fill=\"#ff0000\" order=1 start=0 duration=4\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=150 size=24 color=\"#ffffff\"\n    box id=\"ghost\" x=10 y=20 width=20 height=20 fill=\"#00ff00\" hidden=true start=0 duration=4\n}\n",
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

fn kdl_bytes(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("composition.kdl")).expect("sources stay readable")
}

/// Every file under the project folder with its bytes, so a
/// refused, previewed, or cancelled gesture can prove it staged
/// nothing anywhere.
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

fn keys_of(value: &serde_json::Value) -> Vec<String> {
    value
        .get("keys")
        .and_then(|k| k.as_array())
        .expect("keys array")
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect()
}
#[test]
fn select_click_shift_add_marquee_and_clear() {
    with_server("gestures", select_project, |_folder, _reg, _entry, port| {
        // Click the overlap: the topmost box wins (card, order 2).
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","at":[60,40]}"#),
        );
        assert_eq!(code, 200, "click selects: {j}");
        assert_eq!(keys_of(&j), vec!["card".to_string()]);
        // The hidden ghost over card shows no handles: card still wins.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","at":[15,25]}"#),
        );
        assert_eq!(code, 200, "hidden skipped: {j}");
        assert_eq!(keys_of(&j), vec!["card".to_string()]);
        // Click the panel-only corner.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","at":[120,60]}"#),
        );
        assert_eq!(code, 200, "panel corner selects: {j}");
        assert_eq!(keys_of(&j), vec!["panel".to_string()]);
        // Shift-add over the current selection unions both boxes.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"add","keys":["card"],"current":["panel"]}"#),
        );
        assert_eq!(code, 200, "shift-add unions: {j}");
        assert_eq!(keys_of(&j), vec!["panel".to_string(), "card".to_string()]);
        // Toggle flips membership: card drops, title (by key) joins.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"toggle","keys":["card","title"],"current":["panel","card"]}"#),
        );
        assert_eq!(code, 200, "toggle flips: {j}");
        assert_eq!(keys_of(&j), vec!["panel".to_string(), "title".to_string()]);
        // Marquee over the whole film takes boxed elements in paint
        // rank: panel (order 1) then card (order 2). Text carries no
        // authored extents and the hidden ghost shows no handles.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"marquee","rect":[0,0,320,180]}"#),
        );
        assert_eq!(code, 200, "marquee takes boxes: {j}");
        assert_eq!(keys_of(&j), vec!["panel".to_string(), "card".to_string()]);
        // Clear empties the selection.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"clear"}"#),
        );
        assert_eq!(code, 200, "clear empties: {j}");
        assert!(keys_of(&j).is_empty());
    });
}

#[test]
fn select_click_miss_and_unknown_keys_refuse_with_causes() {
    with_server("miss", select_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","at":[310,170]}"#),
        );
        assert_eq!(code, 404, "empty film refuses: {text}");
        assert!(
            text.contains("(310, 170)"),
            "the miss names the point: {text}"
        );
        assert!(
            text.contains("Nothing was staged"),
            "refusal stages nothing: {text}"
        );
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","key":"nobody"}"#),
        );
        assert_eq!(code, 404, "unknown key refuses: {text}");
        assert!(
            text.contains("\"nobody\""),
            "the miss names the key: {text}"
        );
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"toggle","keys":["card","nobody"],"current":[]}"#),
        );
        assert_eq!(code, 404, "unknown toggle key refuses: {text}");
        assert!(text.contains("\"nobody\""), "names the missing key: {text}");
        assert_eq!(snapshot(folder), before, "refusals stage nothing anywhere");
    });
}

#[test]
fn select_layers_lists_z_order_and_selects_from_the_readout() {
    with_server("layers", select_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(port, "GET", "/api/studio/layers?selection=card", None);
        assert_eq!(code, 200, "layers read: {j}");
        let layers = j.get("layers").and_then(|l| l.as_array()).expect("layers");
        let order: Vec<String> = layers
            .iter()
            .map(|l| l.get("key").unwrap().as_str().unwrap().to_string())
            .collect();
        // Paint rank: order 0 in document order, then order 1, 2.
        assert_eq!(
            order,
            vec!["title", "ghost", "panel", "card"],
            "z-order: {order:?}"
        );
        let card = layers
            .iter()
            .find(|l| l.get("key").unwrap() == "card")
            .unwrap();
        assert_eq!(
            card.get("selected").unwrap(),
            &serde_json::Value::Bool(true)
        );
        assert_eq!(
            card.get("box").unwrap().as_array().unwrap().len(),
            4,
            "boxed elements carry boxes"
        );
        let title = layers
            .iter()
            .find(|l| l.get("key").unwrap() == "title")
            .unwrap();
        assert!(title.get("box").unwrap().is_null(), "text carries no box");
        let ghost = layers
            .iter()
            .find(|l| l.get("key").unwrap() == "ghost")
            .unwrap();
        assert_eq!(ghost.get("hidden").unwrap(), &serde_json::Value::Bool(true));
        // Selecting from the readout resolves by key.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"click","key":"title"}"#),
        );
        assert_eq!(code, 200, "readout key selects: {j}");
        assert_eq!(keys_of(&j), vec!["title".to_string()]);
        assert!(j.get("detail").unwrap().as_str().unwrap().contains("title"));
    });
}

#[test]
fn select_canvas_and_timeline_stay_in_sync() {
    with_server("sync", select_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/select",
            Some(r#"{"action":"add","keys":["card","panel"],"current":[]}"#),
        );
        assert_eq!(code, 200);
        assert_eq!(keys_of(&j), vec!["card".to_string(), "panel".to_string()]);
        // The same selection string echoes identically from the
        // canvas view and the layers readout.
        let (code, canvas) = json(port, "GET", "/api/studio/canvas?selection=card,panel", None);
        assert_eq!(code, 200);
        let (code, layers) = json(port, "GET", "/api/studio/layers?selection=card,panel", None);
        assert_eq!(code, 200);
        assert_eq!(
            canvas.get("selection").unwrap(),
            layers.get("selection").unwrap(),
            "canvas and layers echo the same sync"
        );
        assert_eq!(
            canvas.get("selection").unwrap().get("kept").unwrap(),
            &serde_json::json!(["card", "panel"])
        );
        // The timeline resolves the same keys over the same clips.
        let (code, rows) = json(port, "GET", "/api/studio/timeline", None);
        assert_eq!(code, 200);
        let ids: Vec<String> = rows
            .get("clips")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                c.get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("#?")
                    .to_string()
            })
            .collect();
        assert!(
            ids.contains(&"card".to_string()),
            "timeline holds card: {ids:?}"
        );
        assert!(
            ids.contains(&"panel".to_string()),
            "timeline holds panel: {ids:?}"
        );
        // Unknown echo entries drop with a note, never refuse.
        let (code, canvas) = json(
            port,
            "GET",
            "/api/studio/canvas?selection=card,nobody",
            None,
        );
        assert_eq!(code, 200);
        assert_eq!(
            canvas.get("selection").unwrap().get("kept").unwrap(),
            &serde_json::json!(["card"])
        );
        assert_eq!(
            canvas.get("selection").unwrap().get("dropped").unwrap(),
            &serde_json::json!(["nobody"])
        );
    });
}
#[test]
fn transform_move_commits_a_named_turn() {
    with_server("move", select_project, |folder, _reg, _entry, port| {
        let sources = kdl_bytes(folder);
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[10,-5],"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "move commits: {j}");
        assert_eq!(j.get("saved").unwrap(), &serde_json::Value::Bool(true));
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(0));
        let summaries = j.get("summaries").unwrap().as_array().unwrap();
        assert_eq!(summaries.len(), 2, "one row op per axis: {summaries:?}");
        assert!(summaries
            .iter()
            .all(|s| s.as_str().unwrap().contains("card")));
        let detail = j.get("detail").unwrap().as_str().unwrap();
        assert!(
            detail.contains("canvas move card by (10, -5)px"),
            "named detail: {detail}"
        );
        assert!(detail.contains("turn 0"), "turn named: {detail}");
        // The effective composition shows the move; sources stay bytes.
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
        assert_eq!(
            card.get("box").unwrap().as_array().unwrap()[0..2],
            [serde_json::json!(20.0), serde_json::json!(15.0)],
            "card moved: {}",
            card.get("box").unwrap()
        );
        assert_eq!(kdl_bytes(folder), sources, "sources stay byte-identical");
        // A second commit records turn 2.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[0,5],"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "second move commits: {j}");
        assert_eq!(j.get("turn").unwrap(), &serde_json::json!(1));
        assert_eq!(kdl_bytes(folder), sources, "sources still identical");
    });
}

#[test]
fn transform_group_move_keeps_offsets_with_axis_lock() {
    with_server("group", select_project, |_folder, _reg, _entry, port| {
        // Card (10,20) and panel (50,30): offset (40,10) survives +5/+5.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"},{"id":"panel"}],"action":"move","delta":[5,5],"commit":true,"selection":"card,panel"}"#,
            ),
        );
        assert_eq!(code, 200, "group move commits: {j}");
        assert_eq!(j.get("applied").unwrap(), &serde_json::json!(4));
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        let at = |key: &str| {
            values
                .iter()
                .find(|v| v.get("target").unwrap() == key)
                .unwrap()
                .clone()
        };
        let card = at("card");
        let panel = at("panel");
        assert_eq!(
            (card["x"].as_f64().unwrap(), card["y"].as_f64().unwrap()),
            (15.0, 25.0)
        );
        assert_eq!(
            (panel["x"].as_f64().unwrap(), panel["y"].as_f64().unwrap()),
            (55.0, 35.0)
        );
        assert_eq!(
            j.get("selection").unwrap().get("kept").unwrap(),
            &serde_json::json!(["card", "panel"]),
            "selection echoes through the turn"
        );
        // Axis lock drops the vertical delta with a note.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[3,7],"axis":"x","commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "locked move commits: {j}");
        let detail = j.get("detail").unwrap().as_str().unwrap();
        assert!(detail.contains("axis lock"), "the lock shows: {detail}");
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        assert_eq!(values[0]["x"].as_f64().unwrap(), 18.0);
        assert_eq!(values[0]["y"].as_f64().unwrap(), 25.0, "vertical dropped");
    });
}

#[test]
fn transform_resize_handles_commit_and_clamp() {
    with_server("resize", select_project, |_folder, _reg, _entry, port| {
        // Bottom-right grows freely.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"resize","handle":"bottom-right","size-delta":[20,10],"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "resize commits: {j}");
        assert_eq!(j.get("clamped").unwrap(), &serde_json::Value::Bool(false));
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        assert_eq!(values[0]["width"].as_f64().unwrap(), 120.0);
        assert_eq!(values[0]["height"].as_f64().unwrap(), 60.0);
        assert_eq!(values[0]["x"].as_f64().unwrap(), 10.0);
        // Left edge drag keeps the right edge: x follows, w shrinks.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"resize","handle":"left","size-delta":[20,0],"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "left resize commits: {j}");
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        assert_eq!(values[0]["width"].as_f64().unwrap(), 100.0);
        assert_eq!(values[0]["x"].as_f64().unwrap(), 30.0);
        // Past zero clamps to the 1px floor and the turn records it.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"resize","handle":"left","size-delta":[500,0],"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "clamped resize commits: {j}");
        assert_eq!(j.get("clamped").unwrap(), &serde_json::Value::Bool(true));
        let detail = j.get("detail").unwrap().as_str().unwrap();
        assert!(detail.contains("1px floor"), "the bound shows: {detail}");
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        assert_eq!(values[0]["width"].as_f64().unwrap(), 1.0);
    });
}

#[test]
fn transform_nudge_moves_1px_and_10px_with_shift() {
    with_server("nudge", select_project, |_folder, _reg, _entry, port| {
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"panel"}],"action":"nudge","nudge":[1,0],"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "nudge commits: {j}");
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        assert_eq!(values[0]["x"].as_f64().unwrap(), 51.0, "1 px without Shift");
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"panel"}],"action":"nudge","nudge":[-1,0],"shift":true,"commit":true}"#,
            ),
        );
        assert_eq!(code, 200, "shift nudge commits: {j}");
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        assert_eq!(values[0]["x"].as_f64().unwrap(), 41.0, "10 px with Shift");
        assert!(j
            .get("detail")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("Shift x10"));
    });
}
#[test]
fn transform_rotate_previews_the_snap_but_commit_refuses() {
    with_server("rotate", select_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        // Preview: Shift snaps 22 degrees to the 15-degree grid.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"rotate","degrees":22,"shift":true}"#,
            ),
        );
        assert_eq!(code, 200, "rotate previews: {j}");
        let applied = j.get("applied-values").unwrap();
        assert_eq!(applied.get("angle").unwrap().as_f64().unwrap(), 15.0);
        assert_eq!(
            applied.get("snapped").unwrap(),
            &serde_json::Value::Bool(true)
        );
        assert_eq!(j.get("turn").unwrap(), &serde_json::Value::Null);
        // Unsnaped preview keeps the authored angle.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(r#"{"actor":"maker","targets":[{"id":"card"}],"action":"rotate","degrees":22}"#),
        );
        assert_eq!(code, 200, "plain rotate previews: {j}");
        assert_eq!(
            j.get("applied-values")
                .unwrap()
                .get("angle")
                .unwrap()
                .as_f64()
                .unwrap(),
            22.0
        );
        // Commit refuses: static rotation has no render-supported
        // row, so nothing stages and no turn records.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"rotate","degrees":22,"shift":true,"commit":true}"#,
            ),
        );
        assert_eq!(code, 409, "rotate commit refuses: {text}");
        assert!(
            text.contains("no render-supported row"),
            "names the rule: {text}"
        );
        assert!(text.contains("motion-driven"), "names the layer: {text}");
        assert!(
            text.contains("Nothing was applied or recorded"),
            "untouched: {text}"
        );
        assert_eq!(snapshot(folder), before, "rotate stages nothing anywhere");
    });
}

#[test]
fn transform_preview_and_cancel_stage_nothing() {
    with_server("staged", select_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        // Preview computes the move without staging or recording.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[30,30]}"#),
        );
        assert_eq!(code, 200, "preview answers: {j}");
        assert_eq!(j.get("committed").unwrap(), &serde_json::Value::Bool(false));
        assert_eq!(j.get("staged").unwrap(), &serde_json::Value::Bool(false));
        let values = j.get("applied-values").unwrap().as_array().unwrap();
        assert_eq!(values[0]["x"].as_f64().unwrap(), 40.0);
        assert!(j
            .get("detail")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("nothing was staged"));
        assert_eq!(snapshot(folder), before, "preview writes nothing");
        // Cancel discards with a note.
        let (code, j) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[30,30],"cancelled":true}"#,
            ),
        );
        assert_eq!(code, 200, "cancel answers: {j}");
        assert_eq!(j.get("cancelled").unwrap(), &serde_json::Value::Bool(true));
        assert!(j
            .get("detail")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("cancelled"));
        assert_eq!(snapshot(folder), before, "cancel writes nothing");
        // The effective composition still shows the authored origin.
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
        assert_eq!(
            card.get("box").unwrap().as_array().unwrap()[0],
            serde_json::json!(10.0),
            "no staged value leaked"
        );
    });
}

#[test]
fn transform_refusals_name_the_cause_and_stage_nothing() {
    with_server("refuse", select_project, |folder, _reg, _entry, port| {
        let before = snapshot(folder);
        // Unknown target: a drag with no valid target.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"nobody"}],"action":"move","delta":[1,0],"commit":true}"#,
            ),
        );
        assert_eq!(code, 404, "unknown target: {text}");
        assert!(
            text.contains("\"nobody\""),
            "names the missing target: {text}"
        );
        // Text has no size rows: resize refuses with the supported kinds.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"title"}],"action":"resize","handle":"right","size-delta":[5,0],"commit":true}"#,
            ),
        );
        assert_eq!(code, 409, "text resize refuses: {text}");
        assert!(
            text.contains("sized clips"),
            "names the supported kinds: {text}"
        );
        assert!(text.contains("keeps its prior size"), "prior kept: {text}");
        // Hidden clips show no handles: the locked canvas relationship.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"ghost"}],"action":"move","delta":[1,0],"commit":true}"#,
            ),
        );
        assert_eq!(code, 409, "hidden move refuses: {text}");
        assert!(text.contains("no handles"), "names the lock: {text}");
        // Resize grabs exactly one element.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"},{"id":"panel"}],"action":"resize","handle":"right","size-delta":[5,0],"commit":true}"#,
            ),
        );
        assert_eq!(code, 400, "group resize refuses: {text}");
        assert!(text.contains("one element"), "names the rule: {text}");
        // Commits are named: no actor, no turn.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transform",
            Some(r#"{"targets":[{"id":"card"}],"action":"move","delta":[1,0],"commit":true}"#),
        );
        assert_eq!(code, 400, "anonymous commit refuses: {text}");
        assert!(text.contains("actor"), "names the actor: {text}");
        // Non-finite geometry refuses.
        let (code, text) = body_str(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[1,0],"axis":"sideways","commit":true}"#,
            ),
        );
        assert_eq!(code, 400, "bad axis refuses: {text}");
        assert!(text.contains("axis"), "names the key: {text}");
        assert_eq!(snapshot(folder), before, "refusals stage nothing anywhere");
    });
}

#[test]
fn transform_off_canvas_moves_show_indicators_pointing_out() {
    with_server(
        "offcanvas",
        select_project,
        |_folder, _reg, _entry, port| {
            // Push the card fully past the right film edge: allowed, with
            // a turn recording the off-canvas result.
            let (code, j) = json(
                port,
                "POST",
                "/api/studio/transform",
                Some(
                    r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[500,0],"commit":true}"#,
                ),
            );
            assert_eq!(code, 200, "off-canvas move commits: {j}");
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
            assert_eq!(
                card.get("off-canvas").unwrap(),
                &serde_json::json!(["right"]),
                "indicator points right: {card}"
            );
            let indicators = layers.get("indicators").unwrap().as_object().unwrap();
            assert!(indicators.contains_key("card"), "indicators map names card");
        },
    );
}

#[test]
fn studio_page_hosts_the_selection_panel_with_hyphenated_styles() {
    with_server("page", select_project, |_folder, _reg, _entry, port| {
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200);
        for id in [
            "canvas-layers-bar",
            "canvas-layers",
            "canvas-nudge",
            "canvas-nudge-left",
            "canvas-nudge-right",
            "canvas-nudge-up",
            "canvas-nudge-down",
            "canvas-transform-status",
        ] {
            assert!(page.contains(id), "the page hosts {id}");
        }
        for style in [
            "background-color",
            "border-radius",
            "z-index",
            "font-variant-numeric",
        ] {
            assert!(
                page.contains(style),
                "styles stay CSS-like hyphenated: {style}"
            );
        }
        // The pre-existing canvas view panel stays hosted.
        for id in ["canvas-view-bar", "canvas-stage", "canvas-selection-note"] {
            assert!(page.contains(id), "the canvas panel stays hosted: {id}");
        }
    });
}

#[test]
fn transform_leaves_font_family_try_order_untouched() {
    let root = tmp("fonts");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=40 size=24 color=\"#ffffff\" font-family=\"Nope, Fallback\"\n    box id=\"card\" x=10 y=100 width=60 height=30 fill=\"#ffffff\" start=0 duration=4\n}\n",
    );
    // Read-only probe: the authored try-order list rides through in
    // written comma order, never resolved or substituted.
    let session = sdk::SdkSession::open(
        &folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("font project validates");
    assert_eq!(
        preview::studio_canvas::canvas_font_lists_of(session.effective_composition()),
        vec!["Nope, Fallback".to_string()],
    );
    // A committed canvas move through the shared machinery keeps the
    // list intact while landing its own turn.
    let shown =
        preview::studio::effective_composition_for_session(&folder, None).expect("effective opens");
    let req = preview::studio_select::parse_transform_body(
        r#"{"actor":"maker","targets":[{"id":"card"}],"action":"move","delta":[5,5],"commit":true}"#,
    )
    .expect("move parses");
    let (proposed, clamped, _) =
        preview::studio_select::propose_transform(&shown, &req, 320.0, 180.0)
            .expect("move proposes");
    assert!(!clamped);
    let receipt = preview::studio_select::apply_transform(&folder, None, &req, &proposed)
        .expect("move applies");
    assert_eq!(receipt.turn, 0, "the move lands as a named turn");
    let session = sdk::SdkSession::open(
        &folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("project reopens");
    assert_eq!(
        preview::studio_canvas::canvas_font_lists_of(session.effective_composition()),
        vec!["Nope, Fallback".to_string()],
        "the turn keeps try-order untouched"
    );
}

#[test]
fn transform_keeps_default_renders_served_and_sources_identical() {
    with_server("frames", select_project, |folder, _reg, _entry, port| {
        let sources = kdl_bytes(folder);
        let (before_code, before_png) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(before_code, 200, "frame serves before the turn");
        // Commit a nudge, then serve the same default frame: the
        // preview pipeline still answers over the same sources.
        let (code, _) = json(
            port,
            "POST",
            "/api/studio/transform",
            Some(
                r#"{"actor":"maker","targets":[{"id":"card"}],"action":"nudge","nudge":[0,1],"commit":true}"#,
            ),
        );
        assert_eq!(code, 200);
        assert_eq!(kdl_bytes(folder), sources, "sources stay byte-identical");
        let (after_code, _) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(after_code, 200, "frame serves after the turn");
        assert!(!before_png.is_empty());
    });
}
//__APPEND__
