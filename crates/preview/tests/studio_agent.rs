//! Studio agent tools regressions (Sprint 20, T006 / CAP-007, J-03,
//! STATE-05, INT-002, REC-004): every acceptance criterion of
//! `T006-stage-edits-with-studio-tools.md`, including refusal variants.
//!
//! Reads return current truth, every write stages one edit each with a
//! staged receipt naming what will apply, applying records a shared
//! history turn, no tool renders or writes silently, and missing
//! targets refuse by name. Composition sources stay byte-identical
//! throughout: agent edits land in the SDK delta plus history only.

use preview::studio_agent::{dispatch_agent_tool, parse_agent_body};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-agent-{}-{}-{}",
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

/// One text clip (`title`, 0..2s) plus one box (`card`, 2..4s), 10
/// fps over 4 seconds.
fn reference_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    box id=\"card\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" start=2 duration=2\n}\n",
    );
}

fn project(tag: &str) -> PathBuf {
    let root = tmp(tag);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    reference_project(&folder);
    folder
}

/// Dispatch one JSON tool body against the folder; panics on refusal
/// with the message so failures read.
fn run(folder: &Path, body: &str) -> String {
    let call = parse_agent_body(body).expect("body parses");
    dispatch_agent_tool(folder, &call).expect("tool applies")
}

/// Dispatch one JSON tool body expecting refusal; returns the message.
fn refuse(folder: &Path, body: &str) -> String {
    let call = parse_agent_body(body).expect("body parses");
    dispatch_agent_tool(folder, &call).expect_err("tool must refuse")
}

fn history_turns(folder: &Path) -> Vec<(u64, String, String)> {
    sdk::history::list_turns(folder)
        .expect("history lists")
        .turns
        .into_iter()
        .map(|t| (t.seq, t.actor, t.summary))
        .collect()
}

#[test]
fn agent_body_rejects_unknown_tool_empty_actor_and_empty_body() {
    let err = parse_agent_body("{\"tool\":\"dance\"}").expect_err("unknown tool refuses");
    assert!(err.contains("\"dance\""), "names the tool: {err}");
    assert!(err.contains("look"), "lists the tools: {err}");
    let err =
        parse_agent_body("{\"tool\":\"look\",\"actor\":\"  \"}").expect_err("empty actor refuses");
    assert!(err.contains("actor"), "names the actor: {err}");
    let err = parse_agent_body("   ").expect_err("empty body refuses");
    assert!(err.contains("JSON tool body"), "names the body: {err}");
}

#[test]
fn agent_look_returns_current_composition_truth() {
    let folder = project("look");
    let out = run(&folder, "{\"tool\":\"look\",\"actor\":\"agent\"}");
    assert!(out.contains("\"composition\":\"t\""), "names root: {out}");
    assert!(out.contains("\"clips\":2"), "counts clips: {out}");
    assert!(out.contains("\"duration\":4"), "window: {out}");
    assert!(out.contains("\"canvas\":[320,180]"), "canvas: {out}");
    assert!(history_turns(&folder).is_empty(), "reads record no turn");
}

#[test]
fn agent_inspect_returns_clip_truth_and_names_missing_targets() {
    let folder = project("inspect");
    let out = run(
        &folder,
        "{\"tool\":\"inspect\",\"actor\":\"agent\",\"target-id\":\"title\"}",
    );
    assert!(out.contains("\"key\":\"title\""), "key: {out}");
    assert!(out.contains("\"kind\":\"text clip\""), "kind: {out}");
    assert!(out.contains("\"start\":0"), "window: {out}");
    let out = run(
        &folder,
        "{\"tool\":\"inspect\",\"actor\":\"agent\",\"target-index\":1}",
    );
    assert!(out.contains("\"key\":\"card\""), "index resolves: {out}");
    let err = refuse(
        &folder,
        "{\"tool\":\"inspect\",\"actor\":\"agent\",\"target-id\":\"ghost\"}",
    );
    assert!(err.contains("\"ghost\""), "names the id: {err}");
    let err = refuse(
        &folder,
        "{\"tool\":\"inspect\",\"actor\":\"agent\",\"target-index\":7}",
    );
    assert!(err.contains('7'), "names the index: {err}");
    let err = refuse(&folder, "{\"tool\":\"inspect\",\"actor\":\"agent\"}");
    assert!(err.contains("target-id"), "names the missing target: {err}");
    assert!(history_turns(&folder).is_empty(), "reads record no turn");
}

#[test]
fn agent_frame_reports_active_clips_and_rejects_negative_instants() {
    let folder = project("frame");
    let out = run(&folder, "{\"tool\":\"frame\",\"actor\":\"agent\",\"at\":1}");
    assert!(out.contains("\"title\""), "title active: {out}");
    assert!(!out.contains("\"card\""), "card inactive: {out}");
    let out = run(&folder, "{\"tool\":\"frame\",\"actor\":\"agent\",\"at\":3}");
    assert!(out.contains("\"card\""), "card active: {out}");
    assert!(!out.contains("\"title\""), "title done: {out}");
    let err = refuse(
        &folder,
        "{\"tool\":\"frame\",\"actor\":\"agent\",\"at\":-1}",
    );
    assert!(err.contains("-1"), "names the bound: {err}");
}

#[test]
fn agent_select_picks_clips_with_receipt_and_names_missing() {
    let folder = project("select");
    let out = run(
        &folder,
        "{\"tool\":\"select\",\"actor\":\"agent\",\"targets\":[\"title\",\"#1\"]}",
    );
    assert!(out.contains("\"selected\":true"), "receipt: {out}");
    assert!(
        out.contains("\"title\"") && out.contains("\"card\""),
        "picks: {out}"
    );
    assert!(out.contains("\"count\":2"), "count: {out}");
    let err = refuse(
        &folder,
        "{\"tool\":\"select\",\"actor\":\"agent\",\"targets\":[\"ghost\"]}",
    );
    assert!(err.contains("\"ghost\""), "names the target: {err}");
    let err = refuse(&folder, "{\"tool\":\"select\",\"actor\":\"agent\"}");
    assert!(err.contains("targets"), "names the missing list: {err}");
    assert!(
        history_turns(&folder).is_empty(),
        "view writes record no turn"
    );
}

#[test]
fn agent_seek_stages_playhead_with_receipt_and_frame_math() {
    let folder = project("seek");
    let out = run(
        &folder,
        "{\"tool\":\"seek\",\"actor\":\"agent\",\"at\":1.5}",
    );
    assert!(out.contains("\"at\":1.5"), "instant: {out}");
    assert!(out.contains("\"frame\":15"), "frame at 10fps: {out}");
    let out = run(
        &folder,
        "{\"tool\":\"seek\",\"actor\":\"agent\",\"frame\":20}",
    );
    assert!(out.contains("\"at\":2"), "frame converts: {out}");
    let err = refuse(&folder, "{\"tool\":\"seek\",\"actor\":\"agent\"}");
    assert!(err.contains("`at`"), "names the missing position: {err}");
    let err = refuse(&folder, "{\"tool\":\"seek\",\"actor\":\"agent\",\"at\":-2}");
    assert!(err.contains("-2"), "names the bound: {err}");
    assert!(
        history_turns(&folder).is_empty(),
        "view writes record no turn"
    );
}

#[test]
fn agent_set_text_stages_one_edit_with_turn_and_refuses_clean_state() {
    let folder = project("set-text");
    let before = history_turns(&folder).len();
    let out = run(
        &folder,
        "{\"tool\":\"set-text\",\"actor\":\"agent\",\"target-id\":\"title\",\"value\":\"Hello\"}",
    );
    assert!(out.contains("\"staged\":true"), "staged: {out}");
    assert!(out.contains("\"set-text\""), "tool: {out}");
    assert!(out.contains("\"turn\":",), "turn: {out}");
    let turns = history_turns(&folder);
    assert_eq!(turns.len(), before + 1, "one turn recorded");
    assert_eq!(turns.last().unwrap().1, "agent", "actor bracket");
    let err = refuse(
        &folder,
        "{\"tool\":\"set-text\",\"actor\":\"agent\",\"target-id\":\"title\",\"value\":\"Hello\"}",
    );
    assert!(err.contains("already"), "clean state refuses: {err}");
    assert_eq!(history_turns(&folder).len(), before + 1, "no empty turn");
    let err = refuse(
        &folder,
        "{\"tool\":\"set-text\",\"actor\":\"agent\",\"target-id\":\"card\",\"value\":\"Hi\"}",
    );
    assert!(err.contains("box clip"), "names the kind: {err}");
    let err = refuse(
        &folder,
        "{\"tool\":\"set-text\",\"actor\":\"agent\",\"target-id\":\"ghost\",\"value\":\"Hi\"}",
    );
    assert!(err.contains("\"ghost\""), "names the target: {err}");
}

#[test]
fn agent_set_style_stages_one_row_and_surfaces_sdk_refusals() {
    let folder = project("set-style");
    let before = history_turns(&folder).len();
    let out = run(
        &folder,
        "{\"tool\":\"set-style\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"color\",\"value\":\"#ff0000\"}",
    );
    assert!(out.contains("\"set-style\""), "tool: {out}");
    assert!(out.contains("color"), "names the row: {out}");
    assert_eq!(
        history_turns(&folder).len(),
        before + 1,
        "one turn recorded"
    );
    let err = refuse(
        &folder,
        "{\"tool\":\"set-style\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"nope-row\",\"value\":\"1\"}",
    );
    assert!(err.contains("nope-row"), "SDK reason carried: {err}");
    assert_eq!(
        history_turns(&folder).len(),
        before + 1,
        "refusal records nothing"
    );
    let err = refuse(
        &folder,
        "{\"tool\":\"set-style\",\"actor\":\"agent\",\"target-id\":\"ghost\",\"property\":\"color\",\"value\":\"#ff0000\"}",
    );
    assert!(err.contains("\"ghost\""), "names the target: {err}");
}

#[test]
fn agent_transform_stages_row_ops_in_field_order_and_needs_a_field() {
    let folder = project("transform");
    let out = run(
        &folder,
        "{\"tool\":\"transform\",\"actor\":\"agent\",\"target-id\":\"card\",\"x\":30,\"y\":40}",
    );
    assert!(out.contains("\"transform\""), "tool: {out}");
    assert!(out.contains("\"edits\":[\"set row `x`"), "x staged: {out}");
    let turns = history_turns(&folder);
    assert_eq!(turns.len(), 1, "one shared turn for the gesture");
    let err = refuse(
        &folder,
        "{\"tool\":\"transform\",\"actor\":\"agent\",\"target-id\":\"card\"}",
    );
    assert!(err.contains("`x`"), "names the fields: {err}");
    assert_eq!(history_turns(&folder).len(), 1, "refusal records nothing");
}

#[test]
fn agent_add_animation_stages_one_tween_and_validates_kind() {
    let folder = project("add-animation");
    let out = run(
        &folder,
        "{\"tool\":\"add-animation\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"to\":\"50\",\"start\":0,\"duration\":1}",
    );
    assert!(out.contains("\"add-animation\""), "tool: {out}");
    assert!(out.contains("tween"), "names the tween: {out}");
    assert_eq!(history_turns(&folder).len(), 1, "one turn recorded");
    let err = refuse(
        &folder,
        "{\"tool\":\"add-animation\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"kind\":\"wiggle\",\"to\":\"50\",\"start\":0,\"duration\":1}",
    );
    assert!(err.contains("\"wiggle\""), "names the kind: {err}");
    let err = refuse(
        &folder,
        "{\"tool\":\"add-animation\",\"actor\":\"agent\",\"target-id\":\"ghost\",\"property\":\"x\",\"to\":\"50\",\"start\":0,\"duration\":1}",
    );
    assert!(err.contains("\"ghost\""), "names the target: {err}");
}

#[test]
fn agent_update_animation_retargets_ease_and_names_missing_tween() {
    let folder = project("update-animation");
    run(
        &folder,
        "{\"tool\":\"add-animation\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"to\":\"50\",\"start\":0,\"duration\":1}",
    );
    let out = run(
        &folder,
        "{\"tool\":\"update-animation\",\"actor\":\"agent\",\"index\":0,\"ease\":\"in-out-quad\"}",
    );
    assert!(out.contains("\"update-animation\""), "tool: {out}");
    assert!(out.contains("in-out-quad"), "names the ease: {out}");
    let err = refuse(
        &folder,
        "{\"tool\":\"update-animation\",\"actor\":\"agent\",\"index\":4,\"ease\":\"in-out-quad\"}",
    );
    assert!(err.contains('4'), "names the index: {err}");
    let err = refuse(
        &folder,
        "{\"tool\":\"update-animation\",\"actor\":\"agent\",\"index\":0}",
    );
    assert!(err.contains("`ease`"), "names the fields: {err}");
}

#[test]
fn agent_add_keyframe_stages_one_key_and_needs_two_stops() {
    let folder = project("add-keyframe");
    let out = run(
        &folder,
        "{\"tool\":\"add-keyframe\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"stops\":[{\"at\":0,\"value\":\"10\"},{\"at\":1,\"value\":\"50\"}]}",
    );
    assert!(out.contains("\"add-keyframe\""), "tool: {out}");
    assert!(out.contains("key"), "names the key: {out}");
    assert_eq!(history_turns(&folder).len(), 1, "one turn recorded");
    let err = refuse(
        &folder,
        "{\"tool\":\"add-keyframe\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"stops\":[{\"at\":0,\"value\":\"10\"}]}",
    );
    assert!(err.contains("two stops"), "names the bound: {err}");
}

#[test]
fn agent_delete_animation_removes_by_kind_and_names_missing() {
    let folder = project("delete-animation");
    run(
        &folder,
        "{\"tool\":\"add-animation\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"to\":\"50\",\"start\":0,\"duration\":1}",
    );
    let out = run(
        &folder,
        "{\"tool\":\"delete-animation\",\"actor\":\"agent\",\"kind\":\"tween\",\"index\":0}",
    );
    assert!(out.contains("\"delete-animation\""), "tool: {out}");
    assert!(out.contains("remove tween 0"), "names the removal: {out}");
    let err = refuse(
        &folder,
        "{\"tool\":\"delete-animation\",\"actor\":\"agent\",\"kind\":\"tween\",\"index\":0}",
    );
    assert!(err.contains('0'), "names the missing index: {err}");
    let err = refuse(
        &folder,
        "{\"tool\":\"delete-animation\",\"actor\":\"agent\",\"kind\":\"zap\",\"index\":0}",
    );
    assert!(err.contains("\"zap\""), "names the kind: {err}");
}

#[test]
fn agent_tools_refuse_missing_composition_by_name() {
    let folder = project("composition");
    let err = refuse(
        &folder,
        "{\"tool\":\"look\",\"actor\":\"agent\",\"composition\":\"nope\"}",
    );
    assert!(err.contains("\"nope\""), "names the composition: {err}");
    assert!(err.contains("\"t\""), "lists available: {err}");
}

#[test]
fn agent_writes_keep_composition_sources_byte_identical() {
    let folder = project("identical");
    let sources_before = std::fs::read(folder.join("composition.kdl")).unwrap();
    for body in [
        "{\"tool\":\"set-text\",\"actor\":\"agent\",\"target-id\":\"title\",\"value\":\"One\"}",
        "{\"tool\":\"set-style\",\"actor\":\"agent\",\"target-id\":\"card\",\"property\":\"background-color\",\"value\":\"#00ff00\"}",
        "{\"tool\":\"transform\",\"actor\":\"agent\",\"target-id\":\"card\",\"x\":30,\"y\":40,\"width\":120,\"height\":60,\"start\":1,\"duration\":2}",
        "{\"tool\":\"add-animation\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"to\":\"50\",\"start\":0,\"duration\":1}",
        "{\"tool\":\"add-keyframe\",\"actor\":\"agent\",\"target-id\":\"title\",\"property\":\"x\",\"stops\":[{\"at\":0,\"value\":\"10\"},{\"at\":1,\"value\":\"50\"}]}",
        "{\"tool\":\"update-animation\",\"actor\":\"agent\",\"index\":0,\"ease\":\"in-out-quad\"}",
        "{\"tool\":\"delete-animation\",\"actor\":\"agent\",\"kind\":\"key\",\"index\":0}",
    ] {
        run(&folder, body);
    }
    let sources_after = std::fs::read(folder.join("composition.kdl")).unwrap();
    assert_eq!(sources_before, sources_after, "sources stay byte-identical");
    assert_eq!(history_turns(&folder).len(), 7, "seven turns recorded");
}

#[test]
fn agent_maker_can_undo_an_agent_turn_from_shared_history() {
    let folder = project("undo");
    run(
        &folder,
        "{\"tool\":\"set-text\",\"actor\":\"agent\",\"target-id\":\"title\",\"value\":\"Changed\"}",
    );
    let undone = sdk::history::undo_turn(&folder, history_turns(&folder).last().unwrap().0)
        .expect("maker undoes the agent turn");
    assert_eq!(undone.actor, "agent", "undo names the agent turn");
    let session = sdk::SdkSession::open(
        &folder,
        sdk::OpenOptions {
            preview: true,
            ..sdk::OpenOptions::default()
        },
    )
    .expect("project reopens");
    let value = match &session.effective_composition().clips[0] {
        notation::model::Clip::Text(c) => c.value.clone(),
        other => panic!("expected text, found {}", other.what()),
    };
    assert_eq!(value, "Hi", "undo restores the prior text");
}
