//! Inspector row regressions (Sprint 17, T007 / CAP-007, J-04,
//! INT-002, REC-001): every group exposes its rows with live
//! values, each row edit writes a validated patch turn, per-row
//! reset restores the default as an undoable turn, unsupported rows
//! mark unavailable with the milestone named, and unsupportable
//! values refuse with rule and location keeping the prior value.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use preview::{studio, studio_inspector};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-inspector-{}-{}-{}",
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

fn project(folder: &Path) {
    std::fs::create_dir_all(folder.join("media")).unwrap();
    std::fs::write(folder.join("media/clip.mp4"), b"not-a-real-video").unwrap();
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=360 duration=6 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hello\" start=0 duration=6 x=10 y=40 size=24 color=\"#ffffff\"\n    box id=\"card\" x=0 y=0 width=200 height=100 fill=\"#ff0000\" start=0 duration=6 {\n        effect preset=\"grayscale\"\n    }\n    video id=\"clip\" path=\"media/clip.mp4\" start=0 duration=4\n}\n",
    );
}

fn shown(folder: &Path) -> notation::model::Composition {
    studio::effective_composition_for_session(folder, Some("t")).expect("effective comp")
}

fn target_id(id: &str) -> sdk::ClipRef {
    sdk::ClipRef::by_id(id)
}

fn edit(
    actor: &str,
    target: sdk::ClipRef,
    property: &str,
    value: Option<&str>,
    reset: bool,
) -> studio_inspector::InspectorEdit {
    studio_inspector::InspectorEdit {
        actor: actor.to_string(),
        target,
        property: property.to_string(),
        value: value.map(str::to_string),
        reset,
        selection: vec!["title".to_string(), "card".to_string()],
    }
}

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .expect("history lists")
        .turns
        .len()
}

/// Every group exposes its rows with live values: ten groups, each
/// row carrying its kind, live value, and reset default.
#[test]
fn every_group_exposes_live_rows() {
    let dir = tmp("groups");
    project(&dir);
    let comp = shown(&dir);
    let out = studio_inspector::inspector_json(&comp, &target_id("title"), &[])
        .expect("text rows render");
    for group in [
        "Text", "Style", "Layout", "Motion", "Grade", "Effects", "Overlays", "Media", "Audio FX",
        "Timing",
    ] {
        assert!(
            out.contains(&format!("\"group\":\"{group}\"")),
            "group {group}"
        );
    }
    assert!(
        out.contains("\"property\":\"value\",\"kind\":\"text\",\"value\":\"Hello\""),
        "text live: {out}"
    );
    assert!(
        out.contains(
            "\"property\":\"font-size\",\"kind\":\"number\",\"value\":\"24\",\"default\":\"48\""
        ),
        "font-size live+default: {out}"
    );
    assert!(
        out.contains(
            "\"property\":\"start\",\"kind\":\"number\",\"value\":\"0\",\"default\":\"0\""
        ),
        "timing live: {out}"
    );
    assert!(
        out.contains(
            "\"property\":\"duration\",\"kind\":\"number\",\"value\":\"6\",\"default\":\"6\""
        ),
        "duration reset default: {out}"
    );
    let card = studio_inspector::inspector_json(&comp, &target_id("card"), &[]).expect("box rows");
    assert!(card.contains("\"property\":\"background-color\",\"kind\":\"color\",\"value\":\"#ff0000\",\"default\":\"#ffffff\""), "style live: {card}");
    assert!(
        card.contains(
            "\"property\":\"effect-amount\",\"kind\":\"number\",\"value\":\"1\",\"default\":\"1\""
        ),
        "effects live: {card}"
    );
    let clip =
        studio_inspector::inspector_json(&comp, &target_id("clip"), &[]).expect("video rows");
    assert!(
        clip.contains("\"property\":\"object-fit\",\"kind\":\"select\",\"value\":\"contain\""),
        "layout live: {clip}"
    );
    assert!(
        clip.contains("\"property\":\"volume\",\"kind\":\"number\",\"value\":\"0\""),
        "audio fx live: {clip}"
    );
    assert!(
        clip.contains("\"property\":\"trim-start\",\"kind\":\"number\",\"value\":\"0\""),
        "timing trim live: {clip}"
    );
}

/// Kinds the clip cannot support are skipped, never faked: a text
/// clip shows no Style rows, a box shows no Text rows.
#[test]
fn inapplicable_rows_are_skipped() {
    let dir = tmp("skip");
    project(&dir);
    let comp = shown(&dir);
    let text =
        studio_inspector::inspector_json(&comp, &target_id("title"), &[]).expect("text rows");
    assert!(
        !text.contains("\"property\":\"background-color\""),
        "no style leak: {text}"
    );
    assert!(
        !text.contains("\"property\":\"volume\""),
        "no audio leak: {text}"
    );
    let card = studio_inspector::inspector_json(&comp, &target_id("card"), &[]).expect("box rows");
    assert!(
        !card.contains("\"property\":\"font-size\""),
        "no text leak: {card}"
    );
    assert!(
        !card.contains("\"property\":\"volume\""),
        "no audio leak: {card}"
    );
}

/// Render-deferred rows mark unavailable with the milestone named.
#[test]
fn deferred_rows_mark_unavailable_with_milestone() {
    let dir = tmp("deferred");
    project(&dir);
    let comp = shown(&dir);
    let out = studio_inspector::inspector_json(&comp, &target_id("card"), &[]).expect("rows");
    assert!(
        out.contains("\"property\":\"filter\",\"kind\":\"text\",\"value\":\"\",\"default\":\"\",\"editable\":false,\"milestone\":\"filter milestone\""),
        "filter unavailable: {out}"
    );
    assert!(
        out.contains("\"property\":\"transform-3d\"")
            && out.contains("\"milestone\":\"3D milestone\""),
        "3d unavailable: {out}"
    );
    assert!(
        out.contains("\"property\":\"rotate-x\""),
        "rotate-x listed: {out}"
    );
    assert!(
        out.contains("\"property\":\"perspective\""),
        "perspective listed: {out}"
    );
    assert!(
        out.contains("\"property\":\"origin\""),
        "origin listed: {out}"
    );
}

/// One row edit per group writes a validated patch turn: edit, turn
/// recorded under the actor, live value updated.
#[test]
fn one_row_edit_per_group_records_a_turn() {
    let dir = tmp("edits");
    project(&dir);
    let before = turn_count(&dir);
    let edits = [
        ("title", "font-size", "32"),
        ("card", "background-color", "#00ff00"),
        ("card", "x", "12"),
        ("card", "filter-blur", "2"),
        ("card", "grade-exposure", "1"),
        ("card", "effect-amount", "0.5"),
        ("card", "visible", "false"),
        ("clip", "rate", "2"),
        ("clip", "volume", "-6"),
        ("title", "start", "1"),
        ("title", "duration", "4"),
    ];
    for (id, property, value) in edits {
        let receipt = studio_inspector::apply_inspector_edit(
            &dir,
            Some("t"),
            &edit("maker", target_id(id), property, Some(value), false),
        )
        .unwrap_or_else(|e| panic!("row {property} refused: {e}"));
        assert_eq!(receipt.actor, "maker");
        assert!(!receipt.summaries.is_empty(), "receipt names the turn");
        assert_eq!(
            receipt.selection,
            vec!["title".to_string(), "card".to_string()],
            "selection echoes"
        );
    }
    assert_eq!(
        turn_count(&dir),
        before + edits.len(),
        "one validated turn per row"
    );
    let comp = shown(&dir);
    assert_eq!(
        sdk::row_live_value(&comp.clips[0], "font-size").as_deref(),
        Some("32")
    );
    assert_eq!(
        sdk::row_live_value(&comp.clips[1], "background-color").as_deref(),
        Some("#00ff00")
    );
    assert_eq!(comp.clips[0].start(), 1.0);
    assert_eq!(comp.clips[0].duration(), 4.0);
    assert!(comp.clips[1].hidden());
}

/// Per-row reset restores the default as an undoable turn: reset,
/// then undo, and the prior value returns.
#[test]
fn per_row_reset_restores_default_as_undoable_turn() {
    let dir = tmp("reset");
    project(&dir);
    let receipt = studio_inspector::apply_inspector_edit(
        &dir,
        Some("t"),
        &edit("maker", target_id("title"), "font-size", Some("64"), false),
    )
    .expect("edit applies");
    assert_eq!(receipt.turn, 0, "first validated turn is seq 0");
    let reset = studio_inspector::apply_inspector_edit(
        &dir,
        Some("t"),
        &edit("maker", target_id("title"), "font-size", None, true),
    )
    .expect("reset applies");
    assert_eq!(reset.turn, receipt.turn + 1, "reset is its own turn");
    assert_eq!(
        sdk::row_live_value(&shown(&dir).clips[0], "font-size").as_deref(),
        Some("48"),
        "default restored"
    );
    // Reset is an undoable turn: walking undo backward restores the
    // edited value, then the original. A clean project keeps the
    // persisted delta out of the staged base.
    let undo_dir = tmp("reset-undo");
    project(&undo_dir);
    let mut session = sdk::SdkSession::open(
        &undo_dir,
        sdk::OpenOptions {
            history: false,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("valid open");
    session
        .apply(sdk::EditOp::SetRow {
            target: target_id("title"),
            property: "font-size".to_string(),
            value: "64".to_string(),
        })
        .expect("edit stages");
    session
        .apply(sdk::EditOp::SetRow {
            target: target_id("title"),
            property: "font-size".to_string(),
            value: "48".to_string(),
        })
        .expect("reset stages");
    session.undo().expect("reset undoes");
    assert_eq!(
        sdk::row_live_value(&session.effective_composition().clips[0], "font-size").as_deref(),
        Some("64"),
        "undo of reset restores the edit"
    );
    session.undo().expect("edit undoes");
    assert_eq!(
        sdk::row_live_value(&session.effective_composition().clips[0], "font-size").as_deref(),
        Some("24"),
        "undo of edit restores the original"
    );
    // Resetting a default-less row refuses with rule + location.
    let err = studio_inspector::apply_inspector_edit(
        &dir,
        Some("t"),
        &edit("maker", target_id("card"), "width", None, true),
    )
    .expect_err("default-less reset refuses");
    assert!(
        err.contains("required") && err.contains("width"),
        "rule+location: {err}"
    );
}

/// An unsupportable value refuses with rule and location keeping the
/// prior value: one bad value per family.
#[test]
fn bad_row_values_refuse_located_keeping_prior() {
    let dir = tmp("refuse");
    project(&dir);
    let cases = [
        ("title", "font-size", "-1"),
        ("title", "color", "red"),
        ("title", "text-align", "justify"),
        ("title", "font-family", "Alpha,, Beta"),
        ("title", "value", ""),
        ("card", "opacity", "2"),
        ("card", "border-width", "-1"),
        ("clip", "volume", "13"),
        ("clip", "fade-in", "99"),
        ("clip", "rate", "42"),
        ("clip", "object-fit", "stretch"),
        ("title", "start", "-1"),
        ("title", "duration", "0"),
        ("title", "visible", "maybe"),
        ("music-missing", "font-size", "10"),
    ];
    for (id, property, value) in cases {
        let before = sdk::row_live_value(&shown(&dir).clips[0], "font-size").expect("live reads");
        let err = studio_inspector::apply_inspector_edit(
            &dir,
            Some("t"),
            &edit("maker", target_id(id), property, Some(value), false),
        )
        .expect_err("refuses");
        assert!(
            err.contains(property) || err.contains(id),
            "rule+location for {property}: {err}"
        );
        assert_eq!(
            sdk::row_live_value(&shown(&dir).clips[0], "font-size").as_deref(),
            Some(before.as_str()),
            "prior kept for {property}={value}"
        );
    }
}

/// Malformed inspector bodies refuse before anything stages: empty
/// body, missing actor, missing target, unknown row, deferred row,
/// and value-less edits.
#[test]
fn malformed_bodies_refuse() {
    for body in [
        "",
        "{}",
        r#"{"actor":"","target":{"id":"title"},"property":"font-size","value":"32"}"#,
        r#"{"actor":"m","property":"font-size","value":"32"}"#,
        r#"{"actor":"m","target":{},"property":"font-size","value":"32"}"#,
        r#"{"actor":"m","target":{"id":"title"},"value":"32"}"#,
        r#"{"actor":"m","target":{"id":"title"},"property":"font-weight","value":"700"}"#,
        r#"{"actor":"m","target":{"id":"title"},"property":"filter","value":"blur(2)"}"#,
        r#"{"actor":"m","target":{"id":"title"},"property":"font-size"}"#,
        r#"{"actor":"m","target":{"id":"nope"},"property":"font-size","value":"32"}"#,
    ] {
        let parsed = studio_inspector::parse_inspector_body(body);
        let err = match parsed {
            Ok(edit) => {
                let dir = tmp("malformed");
                project(&dir);
                studio_inspector::apply_inspector_edit(&dir, Some("t"), &edit)
                    .expect_err("applies nothing")
            }
            Err(message) => message,
        };
        assert!(
            err.contains("Nothing was staged") || err.contains("Nothing was applied"),
            "refusal: {err}"
        );
    }
    // An unknown target refuses even when parsed.
    let dir = tmp("unknown-target");
    project(&dir);
    let err = studio_inspector::apply_inspector_edit(
        &dir,
        Some("t"),
        &edit("maker", target_id("ghost"), "font-size", Some("32"), false),
    )
    .expect_err("unknown target refuses");
    assert!(err.contains("ghost"), "location: {err}");
}

/// Inspector edits and rows observe the same undo system: the SDK
/// delta replays under both, and the receipt target echoes so
/// canvas, timeline, and inspector show one selection.
#[test]
fn inspector_shares_the_undo_system_and_selection() {
    let dir = tmp("sync");
    project(&dir);
    let receipt = studio_inspector::apply_inspector_edit(
        &dir,
        Some("t"),
        &edit("maker", target_id("card"), "x", Some("25"), false),
    )
    .expect("row applies");
    assert!(
        receipt.target.contains("card"),
        "target echoes: {}",
        receipt.target
    );
    let mut session = sdk::SdkSession::open(
        &dir,
        sdk::OpenOptions {
            history: false,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("valid open");
    assert_eq!(
        sdk::row_live_value(&session.effective_composition().clips[1], "x").as_deref(),
        Some("25"),
        "delta replays for headless sessions"
    );
    session.undo().expect("in-memory undo walks the row turn");
}

/// Minimal HTTP exchange over loopback with an optional body.
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
    let code = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    (code, raw)
}

fn body_str(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, String) {
    let (code, raw) = http(port, method, target, body);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

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

fn http_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=360 duration=6 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hello\" start=0 duration=6 x=10 y=40 size=24 color=\"#ffffff\"\n    box id=\"card\" x=0 y=0 width=200 height=100 fill=\"#ff0000\" start=0 duration=6\n}\n",
    );
}

/// The row API answers over HTTP: rows list, row edit applies, a
/// bad value refuses keeping the prior value.
#[test]
fn inspector_routes_answer_over_http() {
    with_server("http", http_project, |_folder, _reg, _entry, port| {
        let (code, out) = body_str(port, "GET", "/api/studio/inspector?target=title", None);
        assert_eq!(code, 200, "rows list: {out}");
        assert!(out.contains("\"group\":\"Text\""), "groups: {out}");
        assert!(out.contains("\"property\":\"font-size\""), "rows: {out}");
        assert!(out.contains("filter milestone"), "milestone: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/inspector",
            Some(
                r#"{"actor":"maker","target":{"id":"title"},"property":"font-size","value":"36"}"#,
            ),
        );
        assert_eq!(code, 200, "row edit: {out}");
        assert!(out.contains("\"saved\":true"), "receipt: {out}");
        let (_, out) = body_str(port, "GET", "/api/studio/inspector?target=title", None);
        assert!(out.contains("\"value\":\"36\""), "applied value: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/inspector",
            Some(
                r#"{"actor":"maker","target":{"id":"title"},"property":"font-size","value":"huge"}"#,
            ),
        );
        assert_eq!(code, 409, "bad value refuses: {out}");
        assert!(out.contains("font-size"), "rule+location: {out}");
        let (_, out) = body_str(port, "GET", "/api/studio/inspector?target=title", None);
        assert!(out.contains("\"value\":\"36\""), "prior kept: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/inspector",
            Some(
                r#"{"actor":"maker","target":{"id":"title"},"property":"font-size","reset":true}"#,
            ),
        );
        assert_eq!(code, 200, "reset: {out}");
        let (_, out) = body_str(port, "GET", "/api/studio/inspector?target=title", None);
        assert!(out.contains("\"value\":\"48\""), "default restored: {out}");
        let (code, _) = body_str(port, "GET", "/api/studio/inspector", None);
        assert_eq!(code, 400, "missing target refuses");
    });
}
