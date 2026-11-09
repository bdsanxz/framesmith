//! Storyboard approval gate regressions (Sprint 20, T004 / CAP-004,
//! J-02, STATE-04, REC-002): every acceptance criterion of
//! `T004-hold-the-storyboard-gate.md`, including refusal variants.
//!
//! One board of stills arms the gate; the film stays byte-identical
//! until the maker approves; the Studio stills toggle approves stills
//! before motion; locked frames persist for resume; the board page
//! never plays posed stills as a slideshow; a missing approval state
//! reads as not approved; early builds refuse pointing at the gate.

use preview::studio_stills::{
    approval_state, gate_refusal, load_storyboard, pending_gate, present_board, render_board_page,
    set_approval, stills_status_json, storyboard_path_for, LockedFrame,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-stills-{}-{}-{}",
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

fn frames() -> Vec<LockedFrame> {
    vec![
        LockedFrame {
            at: 0.0,
            frame: 0,
            label: "t at 0.000s".to_string(),
            still: "still-0000.png".to_string(),
        },
        LockedFrame {
            at: 2.0,
            frame: 20,
            label: "t at 2.000s".to_string(),
            still: "still-0020.png".to_string(),
        },
    ]
}

/// Sorted map of every composition source under `folder`: the
/// never-touches-the-film oracle.
fn film_bytes(folder: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some(".framesmith") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("kdl") {
                let key = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
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
fn missing_state_reads_as_not_approved_with_an_idle_gate() {
    let folder = project("missing");
    assert!(!approval_state(&folder), "missing approval is not approved");
    assert!(
        gate_refusal(&folder, "agent").is_none(),
        "no board means the gate stays idle"
    );
    assert!(
        gate_refusal(&folder, "render").is_none(),
        "renders run without a board"
    );
    assert!(pending_gate(&folder).is_none(), "nothing pends");
    assert!(!storyboard_path_for(&folder).exists(), "no state written");
    let out = stills_status_json(&folder);
    assert!(out.contains("\"presented\":false"), "{out}");
    assert!(out.contains("\"approved\":false"), "{out}");
    assert!(out.contains("\"frames\":0"), "{out}");
}

#[test]
fn board_presents_locks_and_arms_the_gate() {
    let folder = project("board");
    let before = film_bytes(&folder);
    let receipt = present_board(&folder, "agent-iris", "t", "Launch board", frames())
        .expect("board presents");
    assert_eq!(receipt.frames.len(), 2, "two locked stills");
    assert_eq!(receipt.actor, "agent-iris", "actor recorded");
    assert!(storyboard_path_for(&folder).is_file(), "state persists");
    // The film is byte-identical before approval.
    assert_eq!(film_bytes(&folder), before, "board reads the film only");
    assert!(!approval_state(&folder), "a fresh board is not approved");
    let pending = pending_gate(&folder).expect("gate is armed");
    assert_eq!(pending.board.len(), 2, "both stills lock");
    assert_eq!(pending.title, "Launch board", "title persists");
    assert_eq!(pending.composition, "t", "composition persists");
    let refusal = gate_refusal(&folder, "agent").expect("builds refuse");
    assert!(refusal.contains("approve"), "points at the gate: {refusal}");
    let render_refusal = gate_refusal(&folder, "render").expect("renders refuse");
    assert!(
        render_refusal.contains("No MP4 was written"),
        "{render_refusal}"
    );
    // The status read names the board and its locks.
    let out = stills_status_json(&folder);
    assert!(out.contains("\"presented\":true"), "{out}");
    assert!(out.contains("\"approved\":false"), "{out}");
    assert!(out.contains("\"locked-frames\""), "{out}");
    assert!(out.contains("still-0000.png"), "{out}");
    assert!(out.contains("still-0020.png"), "{out}");
    // The presentation records a shared history turn.
    let turns = history_turns(&folder);
    assert_eq!(turns.len(), 1, "one board turn: {turns:?}");
    assert_eq!(turns[0].1, "agent-iris", "actor bracketed");
    assert!(turns[0].2.contains("storyboard"), "summary names the gate");
}

#[test]
fn agent_writes_refuse_while_gate_holds_but_reads_stay_open() {
    let folder = project("agent-gate");
    let before = film_bytes(&folder);
    present_board(&folder, "agent-iris", "t", "Launch board", frames()).expect("board presents");
    // Composition writes refuse pointing at the pending gate.
    for tool in [
        "set-text",
        "set-style",
        "transform",
        "add-animation",
        "add-keyframe",
    ] {
        let body = match tool {
            "set-text" => {
                "{\"tool\":\"set-text\",\"actor\":\"agent-iris\",\"target-id\":\"title\",\"value\":\"Later\"}"
            }
            "set-style" => {
                "{\"tool\":\"set-style\",\"actor\":\"agent-iris\",\"target-id\":\"card\",\"property\":\"opacity\",\"value\":\"0.5\"}"
            }
            "transform" => {
                "{\"tool\":\"transform\",\"actor\":\"agent-iris\",\"target-id\":\"card\",\"x\":4}"
            }
            "add-animation" => {
                "{\"tool\":\"add-animation\",\"actor\":\"agent-iris\",\"target-id\":\"card\",\"property\":\"x\",\"to\":\"50\",\"start\":2,\"duration\":1}"
            }
            _ => {
                "{\"tool\":\"add-keyframe\",\"actor\":\"agent-iris\",\"target-id\":\"card\",\"property\":\"x\",\"stops\":[{\"at\":2.0,\"value\":\"10\"},{\"at\":3.0,\"value\":\"30\"}]}"
            }
        };
        let call = preview::studio_agent::parse_agent_body(body).expect("body parses");
        let err = preview::studio_agent::dispatch_agent_tool(&folder, &call)
            .expect_err(&format!("{tool} must refuse before approval"));
        assert!(err.contains("approve"), "{tool} points at the gate: {err}");
    }
    // Reads stay open: nothing may animate, but everything may be seen.
    for body in [
        "{\"tool\":\"look\",\"actor\":\"agent-iris\"}",
        "{\"tool\":\"inspect\",\"actor\":\"agent-iris\",\"target-id\":\"title\"}",
        "{\"tool\":\"frame\",\"actor\":\"agent-iris\",\"at\":1.0}",
    ] {
        let call = preview::studio_agent::parse_agent_body(body).expect("body parses");
        let out =
            preview::studio_agent::dispatch_agent_tool(&folder, &call).expect("reads stay open");
        assert!(!out.is_empty(), "read answers: {body}");
    }
    assert_eq!(film_bytes(&folder), before, "refusals stage nothing");
    // Only the board turn is recorded: refused writes record no turn.
    assert_eq!(history_turns(&folder).len(), 1, "no turn for refusals");
}

#[test]
fn approval_lands_the_gate_and_records_a_turn() {
    let folder = project("approve");
    let before = film_bytes(&folder);
    present_board(&folder, "agent-iris", "t", "Launch board", frames()).expect("board presents");
    let receipt = set_approval(&folder, "maker", true).expect("approval records");
    assert!(receipt.approved, "toggle lands approved");
    assert_eq!(receipt.frames, 2, "covers both locked stills");
    assert_eq!(receipt.actor, "maker", "maker decision recorded");
    assert!(approval_state(&folder), "reads as approved");
    assert!(
        gate_refusal(&folder, "agent").is_none(),
        "motion may proceed"
    );
    assert!(
        gate_refusal(&folder, "render").is_none(),
        "render may proceed"
    );
    assert_eq!(film_bytes(&folder), before, "approval changes no film file");
    let turns = history_turns(&folder);
    assert_eq!(turns.len(), 2, "board turn plus approval turn");
    assert_eq!(turns[1].1, "maker", "approval bracketed by the maker");
    assert!(
        turns[1].2.contains("approved"),
        "summary names the decision"
    );
    // Motion applies again after approval.
    let call = preview::studio_agent::parse_agent_body(
        "{\"tool\":\"set-text\",\"actor\":\"agent-iris\",\"target-id\":\"title\",\"value\":\"Later\"}",
    )
    .expect("body parses");
    let out = preview::studio_agent::dispatch_agent_tool(&folder, &call).expect("applies");
    assert!(out.contains("\"staged\":true"), "staged receipt: {out}");
}

#[test]
fn toggle_reopens_the_gate() {
    let folder = project("reopen");
    present_board(&folder, "agent-iris", "t", "Launch board", frames()).expect("board presents");
    set_approval(&folder, "maker", true).expect("approval records");
    assert!(gate_refusal(&folder, "agent").is_none(), "gate landed");
    let receipt = set_approval(&folder, "maker", false).expect("re-open records");
    assert!(!receipt.approved, "toggle re-opens");
    assert!(!approval_state(&folder), "reads as not approved again");
    let refusal = gate_refusal(&folder, "studio").expect("gate re-armed");
    assert!(refusal.contains("approve"), "points at the gate: {refusal}");
    let call = preview::studio_agent::parse_agent_body(
        "{\"tool\":\"set-text\",\"actor\":\"agent-iris\",\"target-id\":\"title\",\"value\":\"Later\"}",
    )
    .expect("body parses");
    preview::studio_agent::dispatch_agent_tool(&folder, &call).expect_err("refuses again");
}

#[test]
fn approve_without_a_board_refuses() {
    let folder = project("no-board");
    let err = set_approval(&folder, "maker", true).expect_err("nothing to approve");
    assert!(
        err.contains("present a board first"),
        "names the fix: {err}"
    );
    assert!(!storyboard_path_for(&folder).exists(), "nothing persisted");
    assert!(history_turns(&folder).is_empty(), "nothing recorded");
}

#[test]
fn corrupt_state_reads_not_approved_and_keeps_the_file() {
    let folder = project("corrupt");
    let path = storyboard_path_for(&folder);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ not a gate state").unwrap();
    assert!(!approval_state(&folder), "corrupt reads as not approved");
    let refusal = gate_refusal(&folder, "render").expect("corrupt file arms the gate");
    assert!(refusal.contains("fresh board"), "names the fix: {refusal}");
    assert!(path.is_file(), "corrupt file kept for inspection");
    let out = stills_status_json(&folder);
    assert!(out.contains("\"approved\":false"), "{out}");
    assert!(out.contains("fresh board"), "notice names the fix: {out}");
    // A fresh board replaces the corrupt state.
    present_board(&folder, "agent-iris", "t", "Fresh board", frames()).expect("re-board works");
    assert!(!approval_state(&folder), "fresh board starts unapproved");
    assert!(
        pending_gate(&folder).is_some(),
        "gate armed on the fresh board"
    );
}

#[test]
fn missing_approval_field_reads_as_not_approved() {
    let folder = project("no-field");
    let path = storyboard_path_for(&folder);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "{\"version\": 1, \"composition\": \"t\", \"title\": \"Board\", \"board\": [{\"at\": 0.0, \"frame\": 0, \"label\": \"t at 0.000s\", \"still\": \"still-0000.png\"}]}",
    )
    .unwrap();
    assert!(!approval_state(&folder), "missing field is not approved");
    assert!(gate_refusal(&folder, "agent").is_some(), "gate holds");
}

#[test]
fn empty_board_and_bad_paths_refuse() {
    let folder = project("empty");
    let err = present_board(&folder, "agent-iris", "t", "Empty", Vec::new())
        .expect_err("an empty board is no board");
    assert!(err.contains("no locked stills"), "names the cause: {err}");
    let bad = vec![LockedFrame {
        at: 0.0,
        frame: 0,
        label: "t at 0.000s".to_string(),
        still: "../escape.png".to_string(),
    }];
    let err = present_board(&folder, "agent-iris", "t", "Escape", bad)
        .expect_err("still paths stay inside the board");
    assert!(err.contains("not a path"), "stills stay inside: {err}");
    assert!(!storyboard_path_for(&folder).exists(), "nothing persisted");
}

#[test]
fn board_page_is_stills_only() {
    let page = render_board_page("Launch board", "t", &frames(), false);
    assert!(page.contains("<!DOCTYPE html>"), "one HTML page");
    assert!(page.contains("Launch board"), "title shown");
    assert_eq!(page.matches("<img").count(), 2, "one still per lock");
    assert!(page.contains("still-0000.png"), "stills referenced");
    assert!(page.contains("0.000s"), "film times captioned");
    assert!(page.contains("frame 20"), "frames captioned");
    assert!(
        page.contains("Awaiting maker approval"),
        "gate position shown"
    );
    for banned in ["<video", "autoplay", "setInterval", "requestAnimationFrame"] {
        assert!(!page.contains(banned), "posed stills never play: {banned}");
    }
    let approved = render_board_page("Launch board", "t", &frames(), true);
    assert!(approved.contains("may proceed"), "approved position shown");
    assert!(
        !approved.contains("<video"),
        "still no motion when approved"
    );
}

#[test]
fn queued_renders_refuse_while_gate_holds() {
    let folder = project("queue-gate");
    present_board(&folder, "agent-iris", "t", "Launch board", frames()).expect("board presents");
    let req = preview::studio_renders::EnqueueRequest {
        actor: "maker".to_string(),
        composition: None,
        format: "mp4".to_string(),
        quality: "draft".to_string(),
        fps: None,
        resolution: "auto".to_string(),
        gpu: false,
        loop_count: 0,
    };
    let err = preview::studio_renders::enqueue_job(&folder, &req, None)
        .expect_err("queued builds refuse before approval");
    assert!(err.contains("approve"), "points at the gate: {err}");
    // The approval lands the queue too.
    set_approval(&folder, "maker", true).expect("approval records");
    assert!(
        preview::studio_stills::gate_refusal(&folder, "studio").is_none(),
        "queue may proceed after approval"
    );
}

#[test]
fn locked_frames_persist_for_resume() {
    let folder = project("persist");
    present_board(&folder, "agent-iris", "t", "Launch board", frames()).expect("board presents");
    set_approval(&folder, "maker", true).expect("approval records");
    // Resume reads the same state back from disk (no memory of the
    // presenting process needed).
    let load = load_storyboard(&folder);
    assert!(load.restored, "state file found");
    let state = load.state.expect("state parses");
    assert!(state.approved, "approval persisted");
    assert_eq!(state.board.len(), 2, "locks persisted");
    assert_eq!(
        state.board[0].still, "still-0000.png",
        "still names persisted"
    );
    assert!((state.board[1].at - 2.0).abs() < 1e-9, "instants persisted");
}
