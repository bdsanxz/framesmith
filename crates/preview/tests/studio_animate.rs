//! Animation regressions (Sprint 17, T008 / CAP-008, CAP-009, J-05,
//! INT-001, INT-003, REC-001, REC-004): tween cards write real
//! tweens and sets with start, length, and speed; unroll exposes
//! computed values; the bezier editor shapes supported curves with
//! per-segment ease and apply-to-all while unsupported families list
//! as unavailable; motion paths preview live with editable points;
//! gesture captures land as keyframes in one turn while empty
//! captures discard with a note; unfollowable paths and unrunnable
//! easings refuse with reasons; every receipt echoes the shared
//! canvas/timeline selection.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use preview::{studio, studio_animate};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-animate-{}-{}-{}",
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
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=360 duration=6 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hello\" start=0 duration=6 x=10 y=40 size=24 color=\"#ffffff\"\n    box id=\"card\" x=0 y=0 width=200 height=100 fill=\"#ff0000\" start=0 duration=6\n}\n",
    );
}

fn shown(folder: &Path) -> notation::model::Composition {
    studio::effective_composition_for_session(folder, Some("t")).expect("effective comp")
}

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .expect("history lists")
        .turns
        .len()
}

fn card(
    kind: &str,
    target: &str,
    property: &str,
    endpoints: &str,
    window: &str,
) -> studio_animate::CardRequest {
    let target_json = if target.starts_with('#') {
        format!("{{\"index\":{}}}", &target[1..])
    } else {
        format!("{{\"id\":\"{target}\"}}")
    };
    let body = format!(
        "{{\"actor\":\"maker\",\"kind\":\"{kind}\",\"target\":{target_json},\"property\":\"{property}\"{endpoints},\"selection\":\"title,card\"{window}}}"
    );
    studio_animate::parse_card_body(&body).expect("card parses")
}

fn apply_card(folder: &Path, req: &studio_animate::CardRequest) -> studio_animate::CardReceipt {
    studio_animate::apply_card_edit(folder, Some("t"), req).expect("card applies")
}

/// Each card kind writes a real tween or set with its window: to,
/// from, from-to, and set land as one named turn each, with
/// `length / speed` as the stored duration.
#[test]
fn each_card_kind_writes_a_real_tween_or_set() {
    let dir = tmp("cards");
    project(&dir);
    let before = turn_count(&dir);
    let to = apply_card(
        &dir,
        &card(
            "to",
            "title",
            "x",
            ",\"to\":\"120\"",
            ",\"start\":1,\"length\":2,\"speed\":2",
        ),
    );
    assert_eq!(to.turn, before as u64, "first turn is seq 0");
    assert_eq!(to.applied, 1);
    assert_eq!(to.kept, vec!["title".to_string(), "card".to_string()]);
    assert!(to.dropped.is_empty(), "known keys keep");
    let from = apply_card(
        &dir,
        &card(
            "from",
            "title",
            "opacity",
            ",\"from\":\"0\"",
            ",\"start\":0,\"length\":2",
        ),
    );
    assert_eq!(from.turn, to.turn + 1);
    let both = apply_card(
        &dir,
        &card(
            "from-to",
            "card",
            "y",
            ",\"from\":\"10\",\"to\":\"60\"",
            ",\"start\":0,\"length\":4,\"speed\":2",
        ),
    );
    assert_eq!(both.turn, from.turn + 1);
    let set = apply_card(
        &dir,
        &card(
            "set",
            "card",
            "opacity",
            ",\"value\":\"0.5\"",
            ",\"start\":1,\"length\":3",
        ),
    );
    assert_eq!(set.turn, both.turn + 1);
    assert_eq!(turn_count(&dir), before + 4, "one turn per card");
    let comp = shown(&dir);
    assert_eq!(comp.tweens.len(), 3, "three tweens");
    assert_eq!(comp.sets.len(), 1, "one set");
    assert_eq!(comp.tweens[0].duration, 1.0, "length 2 at speed 2 runs 1");
    assert_eq!(comp.tweens[2].duration, 2.0, "length 4 at speed 2 runs 2");
    assert_eq!(comp.sets[0].value, 0.5);
    assert_eq!(comp.sets[0].duration, 3.0);
    assert_eq!(comp.tweens[0].property, "x");
}

/// Card refusals name their cause and stage nothing: an empty
/// target, a missing endpoint, a bad window, an unrunnable ease,
/// and a color set all refuse with no turn recorded.
#[test]
fn card_refusals_name_cause_and_stage_nothing() {
    let dir = tmp("card-refuse");
    project(&dir);
    let before = turn_count(&dir);
    let bad_bodies = [
        r#"{"actor":"maker","kind":"to","property":"x","to":"1","start":0,"length":1}"#,
        r#"{"actor":"maker","kind":"spin","target":{"id":"title"},"property":"x","to":"1","start":0,"length":1}"#,
        r#"{"actor":"maker","kind":"to","target":{"id":"title"},"property":"x","start":0,"length":1}"#,
        r#"{"actor":"","kind":"to","target":{"id":"title"},"property":"x","to":"1","start":0,"length":1}"#,
        r#"{"actor":"maker","kind":"to","target":{"id":"title"},"property":"x","to":"1","start":0,"length":0}"#,
        r#"{"actor":"maker","kind":"to","target":{"id":"title"},"property":"x","to":"1","start":0,"length":1,"speed":-1}"#,
        r#"{"actor":"maker","kind":"set","target":{"id":"title"},"property":"opacity","start":0,"length":1}"#,
        r#"{"actor":"maker","kind":"to","target":{"id":"title"},"property":"x","value":"3","to":"1","start":0,"length":1}"#,
    ];
    for body in bad_bodies {
        assert!(
            studio_animate::parse_card_body(body).is_err(),
            "refuses: {body}"
        );
    }
    assert_eq!(turn_count(&dir), before, "parse refusals stage nothing");
    // Unrunnable easings and color sets refuse at apply time.
    let unrunnable = card(
        "to",
        "title",
        "x",
        ",\"to\":\"120\"",
        ",\"start\":0,\"length\":1,\"ease\":\"cubic-bezier(2,0,0,1)\"",
    );
    let err = studio_animate::apply_card_edit(&dir, Some("t"), &unrunnable)
        .expect_err("bad curve refuses");
    assert!(err.contains("cubic-bezier"), "names the ease: {err}");
    let css_keyword = r#"{"actor":"maker","kind":"to","target":{"id":"title"},"property":"x","to":"1","start":0,"length":1,"ease":"ease-in"}"#;
    let err =
        studio_animate::parse_card_body(css_keyword).expect_err("CSS keyword lists unavailable");
    assert!(err.contains("unavailable"), "never faked: {err}");
    let color_set = card(
        "set",
        "card",
        "fill",
        ",\"value\":\"1\"",
        ",\"start\":0,\"length\":1",
    );
    let err = studio_animate::apply_card_edit(&dir, Some("t"), &color_set)
        .expect_err("color set refuses");
    assert!(
        err.contains("tween") || err.contains("key"),
        "names the instead: {err}"
    );
    let ghost = card(
        "to",
        "ghost",
        "x",
        ",\"to\":\"1\"",
        ",\"start\":0,\"length\":1",
    );
    let err = studio_animate::apply_card_edit(&dir, Some("t"), &ghost)
        .expect_err("unknown target refuses");
    assert!(err.contains("ghost"), "names the clip: {err}");
    assert_eq!(turn_count(&dir), before, "apply refusals record no turn");
    assert!(shown(&dir).tweens.is_empty());
    assert!(shown(&dir).sets.is_empty());
}

fn unroll(folder: &Path, query: &str) -> serde_json::Value {
    let (tween, key, samples) = {
        let mut t = None;
        let mut k = None;
        let mut s = None;
        for part in query.split('&') {
            let mut kv = part.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some("tween"), Some(v)) => t = Some(v),
                (Some("key"), Some(v)) => k = Some(v),
                (Some("samples"), Some(v)) => s = Some(v),
                _ => {}
            }
        }
        (t, k, s)
    };
    let req = studio_animate::parse_unroll_params(tween, key, samples).expect("unroll parses");
    let out = studio_animate::unroll_json(&shown(folder), &req).expect("unroll computes");
    serde_json::from_str(&out).expect("unroll is JSON")
}

/// Unroll exposes computed values for editing: eased tween windows
/// sample the midpoint honestly, keys echo stops with per-segment
/// values, and unknown directives refuse with no computation.
#[test]
fn unroll_exposes_computed_values() {
    let dir = tmp("unroll");
    project(&dir);
    apply_card(
        &dir,
        &card(
            "to",
            "title",
            "x",
            ",\"to\":\"120\"",
            ",\"start\":1,\"length\":2",
        ),
    );
    let tween = unroll(&dir, "tween=0&samples=5");
    assert_eq!(tween["kind"], "tween");
    assert_eq!(tween["ease"], "linear");
    let samples = tween["samples"].as_array().expect("samples array");
    assert_eq!(samples.len(), 5);
    assert_eq!(samples[0]["t"], 1.0);
    assert_eq!(samples[0]["value"], 10.0, "from the rest value");
    assert_eq!(samples[4]["t"], 3.0);
    assert_eq!(samples[4]["value"], 120.0);
    assert_eq!(samples[2]["value"], 65.0, "linear midpoint");
    // Keys unroll with stops echoed for editing.
    let mut session = sdk::SdkSession::open(
        &dir,
        sdk::OpenOptions {
            composition: Some("t".to_string()),
            ..sdk::OpenOptions::default()
        },
    )
    .expect("valid open");
    session
        .apply(sdk::EditOp::AddKey {
            target: sdk::ClipRef::by_id("title"),
            property: "x".to_string(),
            stops: vec![
                sdk::KeyStopInput {
                    at: 0.0,
                    value: "0".to_string(),
                    ease: None,
                },
                sdk::KeyStopInput {
                    at: 2.0,
                    value: "100".to_string(),
                    ease: None,
                },
            ],
            ease: None,
        })
        .expect("key stages");
    session.persist_turn("maker").expect("key persists");
    let key = unroll(&dir, "key=0&samples=3");
    assert_eq!(key["kind"], "key");
    assert_eq!(key["stops"].as_array().expect("stops").len(), 2);
    let ksamples = key["samples"].as_array().expect("key samples");
    assert_eq!(ksamples.len(), 3);
    assert_eq!(ksamples[1]["value"], 50.0, "segment midpoint");
    // Unknown directives refuse.
    let err = studio_animate::unroll_json(
        &shown(&dir),
        &studio_animate::parse_unroll_params(Some("9"), None, None).unwrap(),
    )
    .expect_err("unknown tween refuses");
    assert!(err.contains("names nothing"), "names the index: {err}");
    assert!(studio_animate::parse_unroll_params(None, None, None).is_err());
}

/// The bezier editor shapes supported curves with per-segment ease
/// and apply-to-all; unsupported families list as unavailable.
#[test]
fn bezier_shapes_curves_per_segment_and_all() {
    let dir = tmp("bezier");
    project(&dir);
    let before = turn_count(&dir);
    apply_card(
        &dir,
        &card(
            "to",
            "title",
            "x",
            ",\"to\":\"120\"",
            ",\"start\":0,\"length\":2",
        ),
    );
    let bezier = |body: &str| {
        let req = studio_animate::parse_bezier_body(body).expect("bezier parses");
        studio_animate::apply_bezier_edit(&dir, Some("t"), &req).expect("bezier applies")
    };
    let receipt = bezier(r#"{"actor":"maker","ease":"in-out-quad","tween":0,"selection":"title"}"#);
    assert_eq!(receipt.applied, 1);
    assert_eq!(receipt.target, "tween 0");
    assert_eq!(receipt.kept, vec!["title".to_string()]);
    assert_eq!(shown(&dir).tweens[0].ease.name(), "in-out-quad");
    // Per-segment ease writes one stop.
    let mut session = sdk::SdkSession::open(
        &dir,
        sdk::OpenOptions {
            composition: Some("t".to_string()),
            ..sdk::OpenOptions::default()
        },
    )
    .expect("valid open");
    session
        .apply(sdk::EditOp::AddKey {
            target: sdk::ClipRef::by_id("title"),
            property: "y".to_string(),
            stops: vec![
                sdk::KeyStopInput {
                    at: 0.0,
                    value: "0".to_string(),
                    ease: None,
                },
                sdk::KeyStopInput {
                    at: 1.0,
                    value: "10".to_string(),
                    ease: None,
                },
                sdk::KeyStopInput {
                    at: 2.0,
                    value: "20".to_string(),
                    ease: None,
                },
            ],
            ease: None,
        })
        .expect("key stages");
    session.persist_turn("maker").expect("key persists");
    let one = bezier(r#"{"actor":"maker","ease":"out-cubic","key":0,"stop":1}"#);
    assert_eq!(one.target, "key 0 stop 1");
    let comp = shown(&dir);
    assert_eq!(
        comp.keys[0].stops[1].ease.as_ref().map(|e| e.name()),
        Some("out-cubic".to_string())
    );
    assert!(
        comp.keys[0].stops[0].ease.is_none(),
        "other stops untouched"
    );
    // Apply-to-all fans one curve across the fallback and every stop.
    let all = bezier(r#"{"actor":"maker","ease":"bounce","key":0,"all":true}"#);
    assert_eq!(all.applied, 4, "fallback plus three stops");
    let comp = shown(&dir);
    assert_eq!(
        comp.keys[0].ease.as_ref().map(|e| e.name()),
        Some("bounce".to_string())
    );
    assert!(comp.keys[0]
        .stops
        .iter()
        .all(|s| s.ease.as_ref().map(|e| e.name()) == Some("bounce".to_string())));
    assert_eq!(
        turn_count(&dir),
        before + 5,
        "card, key, and three curve turns"
    );
    // Unsupported and unknown families refuse; tween/segment mixups refuse.
    for (body, needle) in [
        (
            r#"{"actor":"maker","ease":"ease-out","tween":0}"#,
            "unavailable",
        ),
        (r#"{"actor":"maker","ease":"wat","key":0}"#, "write one of"),
        (
            r#"{"actor":"maker","ease":"linear","tween":0,"stop":0}"#,
            "ride keys",
        ),
        (r#"{"actor":"maker","ease":"linear"}"#, "no target"),
        (
            r#"{"actor":"maker","ease":"linear","tween":0,"key":0}"#,
            "never both",
        ),
    ] {
        let err = studio_animate::parse_bezier_body(body).expect_err("bezier refuses");
        assert!(err.contains(needle), "names the rule ({needle}): {err}");
    }
    let err = studio_animate::apply_bezier_edit(
        &dir,
        Some("t"),
        &studio_animate::parse_bezier_body(r#"{"actor":"maker","ease":"linear","tween":7}"#)
            .unwrap(),
    )
    .expect_err("unknown tween refuses");
    assert!(err.contains("tween index 7"), "names the index: {err}");
    assert_eq!(turn_count(&dir), before + 5, "refusals record nothing");
}

/// Motion paths preview live with editable points: preview samples
/// the travel line, a dragged point commits through retarget, and an
/// unfollowable path or a mismatched morph refuses with its reason.
#[test]
fn paths_preview_edit_and_refuse() {
    let dir = tmp("paths");
    project(&dir);
    let before = turn_count(&dir);
    let preview = studio_animate::preview_path("M 0,0 L 100,0", 5).expect("preview renders");
    let value: serde_json::Value = serde_json::from_str(&preview).expect("preview is JSON");
    assert_eq!(value["points"].as_array().expect("points").len(), 5);
    assert_eq!(value["points"][4]["x"], 100.0, "travel reaches the end");
    assert_eq!(value["total"], 100.0);
    assert!(
        studio_animate::preview_path("M 0,0 Q 1", 5).is_err(),
        "broken path refuses"
    );
    // Add one path tween, then drag its endpoint and retarget.
    let add = studio_animate::parse_path_body(
        r#"{"actor":"maker","action":"add","target":{"id":"title"},"path":"M 0,0 L 100,0","start":0,"length":2,"ease":"linear"}"#,
    )
    .expect("path add parses");
    let receipt = studio_animate::apply_path_edit(&dir, Some("t"), &add).expect("path add applies");
    assert_eq!(receipt.applied, 1);
    assert_eq!(turn_count(&dir), before + 1);
    assert!(shown(&dir).tweens[0].path.is_some());
    let moved =
        studio_animate::move_path_point("M 0,0 L 100,0", 1, 100.0, 60.0).expect("endpoint drags");
    let retarget = studio_animate::parse_path_body(&format!(
        "{{\"actor\":\"maker\",\"action\":\"retarget\",\"index\":0,\"path\":{moved:?}}}"
    ))
    .expect("retarget parses");
    studio_animate::apply_path_edit(&dir, Some("t"), &retarget).expect("retarget applies");
    let comp = shown(&dir);
    assert!(
        comp.tweens[0].path.as_deref().unwrap().contains("60"),
        "edited point lands"
    );
    assert_eq!(turn_count(&dir), before + 2, "one turn per path edit");
    // Refusals: broken travel, mismatched morph, numeric retarget.
    for body in [
        r#"{"actor":"maker","action":"add","target":{"id":"title"},"path":"M 0,0 Q 1","start":0,"length":2}"#,
        r#"{"actor":"maker","action":"add","target":{"id":"title"},"path":"M 0,0 L 1,1","path-to":"M 0,0 C 1,1 2,2 3,3","start":0,"length":2}"#,
        r#"{"actor":"maker","action":"add","path":"M 0,0 L 1,1","start":0,"length":2}"#,
    ] {
        assert!(
            studio_animate::parse_path_body(body).is_err(),
            "refuses: {body}"
        );
    }
    apply_card(
        &dir,
        &card(
            "to",
            "title",
            "x",
            ",\"to\":\"5\"",
            ",\"start\":0,\"length\":1",
        ),
    );
    let numeric = studio_animate::parse_path_body(
        r#"{"actor":"maker","action":"retarget","index":1,"path":"M 0,0 L 1,1"}"#,
    )
    .expect("retarget parses");
    let err = studio_animate::apply_path_edit(&dir, Some("t"), &numeric)
        .expect_err("numeric retarget refuses");
    assert!(err.contains("no path"), "names the rule: {err}");
    assert_eq!(turn_count(&dir), before + 3, "refusals record nothing");
}

/// Gesture captures land as keyframes in one turn with smoothing
/// and simplification measured; an empty capture discards itself
/// with a note and no turn.
#[test]
fn capture_lands_keyframes_and_empties_discard() {
    let dir = tmp("capture");
    project(&dir);
    let before = turn_count(&dir);
    let mut samples = String::new();
    for i in 0..10 {
        if !samples.is_empty() {
            samples.push(',');
        }
        samples.push_str(&format!(
            "{{\"t\":{},\"x\":{},\"y\":5}}",
            i as f64 * 0.1,
            i as f64 * 12.0
        ));
    }
    let body = format!(
        "{{\"actor\":\"maker\",\"target\":{{\"id\":\"title\"}},\"samples\":[{samples}],\"selection\":\"title\"}}"
    );
    let req = studio_animate::parse_capture_body(&body).expect("capture parses");
    let outcome = studio_animate::apply_capture(&dir, Some("t"), &req).expect("capture applies");
    let receipt = match outcome {
        studio_animate::CaptureOutcome::Applied(receipt) => receipt,
        studio_animate::CaptureOutcome::Discarded { note, .. } => panic!("capture lands: {note}"),
    };
    assert_eq!(receipt.applied, 2, "x plus y in one turn");
    assert_eq!(receipt.recorded, 10);
    assert!(receipt.stops < receipt.recorded, "simplification measured");
    assert_eq!(receipt.kept, vec!["title".to_string()]);
    assert_eq!(turn_count(&dir), before + 1, "one turn for the gesture");
    let comp = shown(&dir);
    assert_eq!(comp.keys.len(), 2);
    assert!(comp.keys.iter().any(|k| k.property == "x"));
    assert!(comp.keys.iter().any(|k| k.property == "y"));
    // An empty capture discards itself with a note, staging nothing.
    let empty = studio_animate::parse_capture_body(
        r#"{"actor":"maker","target":{"id":"title"},"samples":[]}"#,
    )
    .expect("empty parses");
    let outcome = studio_animate::apply_capture(&dir, Some("t"), &empty).expect("discard answers");
    match outcome {
        studio_animate::CaptureOutcome::Discarded { note, .. } => {
            assert!(
                note.contains("no movement was recorded"),
                "note names it: {note}"
            );
            let json =
                studio_animate::capture_outcome_json(&studio_animate::CaptureOutcome::Discarded {
                    note: note.clone(),
                    target: "clip id \"title\"".to_string(),
                    kept: vec![],
                    dropped: vec![],
                });
            assert!(json.contains("\"saved\":false"), "discard JSON: {json}");
        }
        studio_animate::CaptureOutcome::Applied(_) => panic!("empty capture must discard"),
    }
    let still = studio_animate::parse_capture_body(
        r#"{"actor":"maker","target":{"id":"title"},"samples":[{"t":0,"x":7,"y":7},{"t":0.5,"x":7,"y":7}]}"#,
    )
    .expect("held pointer parses");
    let outcome = studio_animate::apply_capture(&dir, Some("t"), &still).expect("held answers");
    assert!(
        matches!(outcome, studio_animate::CaptureOutcome::Discarded { .. }),
        "held pointer is no movement"
    );
    assert_eq!(turn_count(&dir), before + 1, "discards record no turn");
}

/// Animation edits undo through the shared history machinery: one
/// undo walks the card turn back exactly.
#[test]
fn animation_turns_undo_exactly() {
    let dir = tmp("undo");
    project(&dir);
    apply_card(
        &dir,
        &card(
            "set",
            "card",
            "opacity",
            ",\"value\":\"0.5\"",
            ",\"start\":1,\"length\":3",
        ),
    );
    assert_eq!(shown(&dir).sets.len(), 1);
    let mut session = sdk::SdkSession::open(
        &dir,
        sdk::OpenOptions {
            composition: Some("t".to_string()),
            ..sdk::OpenOptions::default()
        },
    )
    .expect("valid open");
    session
        .apply(sdk::EditOp::SetTweenEase {
            index: 0,
            ease: "bounce".to_string(),
        })
        .expect_err("no tween yet to ease");
    let mut session = sdk::SdkSession::open(
        &dir,
        sdk::OpenOptions {
            composition: Some("t".to_string()),
            history: false,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("valid open");
    session
        .apply(sdk::EditOp::AddPathTween {
            target: sdk::ClipRef::by_id("title"),
            path: "M 0,0 L 50,0".to_string(),
            path_to: None,
            orient: false,
            start: 0.0,
            duration: 2.0,
            ease: "linear".to_string(),
        })
        .expect("path tween stages");
    assert_eq!(session.effective_composition().tweens.len(), 1);
    session.undo().expect("path tween undoes");
    assert!(session.effective_composition().tweens.is_empty());
    session
        .apply(sdk::EditOp::AddSet {
            target: sdk::ClipRef::by_id("card"),
            property: "opacity".to_string(),
            value: "0.25".to_string(),
            start: 0.0,
            duration: 1.0,
        })
        .expect("set stages");
    assert_eq!(session.effective_composition().sets.len(), 2);
    session.undo().expect("set undoes");
    assert_eq!(session.effective_composition().sets.len(), 1);
    session
        .apply(sdk::EditOp::SetTweenPath {
            index: 0,
            path: "M 0,0 L 9,9".to_string(),
            path_to: None,
        })
        .expect_err("no staged tween to retarget");
}

fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("server accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("timeout sets");
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
    project(folder);
}

/// The animation APIs answer over HTTP with the selection sync:
/// catalog lists supported and unavailable curves, cards apply,
/// unroll computes, paths preview and commit, captures land and
/// discard, refusals name their rule, and default frames render
/// byte-identical before and after the turns.
#[test]
fn animation_routes_answer_over_http() {
    with_server("http", http_project, |_folder, _reg, _entry, port| {
        let (code, out) = body_str(
            port,
            "GET",
            "/api/studio/animation?selection=title,ghost",
            None,
        );
        assert_eq!(code, 200, "catalog lists: {out}");
        assert!(out.contains("\"ease-families\""), "families: {out}");
        assert!(out.contains("in-out-quad"), "supported curve: {out}");
        assert!(out.contains("\"unavailable\""), "unavailable list: {out}");
        assert!(
            out.contains("ease-in"),
            "CSS keyword unavailable, never faked: {out}"
        );
        assert!(out.contains("\"kept\":[\"title\"]"), "kept echo: {out}");
        assert!(
            out.contains("\"dropped\":[\"ghost\"]"),
            "dropped echo: {out}"
        );
        // Default frames first: animation turns must not move them.
        let (frame_code, frame_before) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(frame_code, 200, "frame serves");
        // One card per kind over HTTP.
        for (kind, endpoints) in [
            ("to", r#","to":"120""#),
            ("from", r#","from":"0""#),
            ("from-to", r#","from":"10","to":"60""#),
        ] {
            let (code, out) = json(
                port,
                "POST",
                "/api/studio/card",
                Some(&format!(
                    "{{\"actor\":\"maker\",\"kind\":\"{kind}\",\"target\":{{\"id\":\"title\"}},\"property\":\"x\"{endpoints},\"start\":0,\"length\":2,\"speed\":1,\"selection\":\"title\"}}"
                )),
            );
            assert_eq!(code, 200, "{kind} card: {out}");
            assert_eq!(out["saved"], true);
            assert_eq!(out["selection"], serde_json::json!(["title"]));
        }
        let (code, out) = json(
            port,
            "POST",
            "/api/studio/card",
            Some(
                r#"{"actor":"maker","kind":"set","target":{"id":"card"},"property":"opacity","value":"0.5","start":1,"length":2,"selection":"card"}"#,
            ),
        );
        assert_eq!(code, 200, "set card: {out}");
        // Unroll the first tween.
        let (code, out) = json(port, "GET", "/api/studio/unroll?tween=0&samples=5", None);
        assert_eq!(code, 200, "unroll: {out}");
        assert_eq!(out["samples"].as_array().expect("samples").len(), 5);
        let (code, out) = body_str(port, "GET", "/api/studio/unroll?tween=40", None);
        assert_eq!(code, 404, "unknown tween: {out}");
        let (code, _) = body_str(port, "GET", "/api/studio/unroll", None);
        assert_eq!(code, 400, "missing directive refuses");
        // Bezier: supported applies, unsupported and unknown refuse.
        let (code, out) = json(
            port,
            "POST",
            "/api/studio/bezier",
            Some(r#"{"actor":"maker","ease":"out-back(1.7)","tween":0,"selection":"title"}"#),
        );
        assert_eq!(code, 200, "bezier: {out}");
        assert_eq!(out["target"], "tween 0");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/bezier",
            Some(r#"{"actor":"maker","ease":"ease-in","tween":0}"#),
        );
        assert_eq!(code, 400, "unavailable refuses: {out}");
        assert!(out.contains("unavailable"), "hint, never faked: {out}");
        let (code, _) = body_str(
            port,
            "POST",
            "/api/studio/bezier",
            Some(r#"{"actor":"maker","ease":"wat","tween":0}"#),
        );
        assert_eq!(code, 400, "unknown refuses");
        // Paths: preview, add, retarget.
        let (code, out) = json(
            port,
            "GET",
            "/api/studio/path-preview?d=M%200%2C0%20L%20100%2C0&samples=5",
            None,
        );
        assert_eq!(code, 200, "path preview: {out}");
        assert_eq!(out["points"].as_array().expect("points").len(), 5);
        let (code, out) = body_str(
            port,
            "GET",
            "/api/studio/path-preview?d=M%200%2C0%20Q%201",
            None,
        );
        assert_eq!(code, 400, "unfollowable refuses: {out}");
        let (code, out) = json(
            port,
            "POST",
            "/api/studio/path",
            Some(
                r#"{"actor":"maker","action":"add","target":{"id":"title"},"path":"M 0,0 L 100,0","start":0,"length":2,"selection":"title"}"#,
            ),
        );
        assert_eq!(code, 200, "path add: {out}");
        let (code, out) = json(
            port,
            "POST",
            "/api/studio/path",
            Some(r#"{"actor":"maker","action":"retarget","index":3,"path":"M 0,0 L 100,60"}"#),
        );
        assert_eq!(code, 200, "path retarget: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/path",
            Some(
                r#"{"actor":"maker","action":"add","target":{"id":"title"},"path":"M 0,0 Q 1","start":0,"length":2}"#,
            ),
        );
        assert_eq!(code, 400, "unfollowable add refuses: {out}");
        // Captures: movement lands, emptiness discards with a note.
        let (code, out) = json(
            port,
            "POST",
            "/api/studio/capture",
            Some(
                r#"{"actor":"maker","target":{"id":"card"},"samples":[{"t":0,"x":0,"y":0},{"t":0.5,"x":40,"y":10},{"t":1,"x":80,"y":0}],"selection":"card"}"#,
            ),
        );
        assert_eq!(code, 200, "capture: {out}");
        assert_eq!(out["saved"], true);
        assert_eq!(out["applied"], 2);
        let (code, out) = json(
            port,
            "POST",
            "/api/studio/capture",
            Some(r#"{"actor":"maker","target":{"id":"card"},"samples":[]}"#),
        );
        assert_eq!(code, 200, "empty discards: {out}");
        assert_eq!(out["saved"], false);
        assert!(
            out["note"]
                .as_str()
                .unwrap()
                .contains("no movement was recorded"),
            "note: {out}"
        );
        // Card refusal over HTTP: empty target.
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/card",
            Some(r#"{"actor":"maker","kind":"to","property":"x","to":"1","start":0,"length":1}"#),
        );
        assert_eq!(code, 400, "empty target refuses: {out}");
        // Frames render byte-identical after every animation turn.
        let (frame_code, frame_after) = http(port, "GET", "/api/frame?i=0&action=step", None);
        assert_eq!(frame_code, 200, "frame still serves");
        assert_eq!(
            frame_before, frame_after,
            "default renders stay byte-identical"
        );
        // The Studio page hosts the animation panel.
        let (code, page) = body_str(port, "GET", "/studio", None);
        assert_eq!(code, 200, "studio page serves");
        assert!(page.contains("id=\"animation-panel\""), "panel ships");
        assert!(page.contains("background-color"), "hyphenated styles ship");
    });
}
