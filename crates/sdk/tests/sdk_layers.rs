//! Layers structure regressions (Sprint 18, T004 / CAP-004): group
//! collect and release land as named, undoable patch turns over the
//! render-neutral editor-note slot; drag reorder moves document
//! indices; a move shuffling motion-targeted indices refuses naming
//! the blockers; empty groups and unknown names refuse with the
//! holdings named. Only validated turns write the project (plus the
//! hidden SDK delta); default renders stay byte-identical.

use sdk::{ClipRef, EditOp, EditRefusalKind, KeyStopInput, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-layers-{}-{}-{}",
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

/// Three note-free boxes (`a`, `b`, `c`, all `order` 0) plus a noted
/// box (`d` carrying an editor note): 2 s, 10 fps, 320x180.
fn layers_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    box id=\"a\" x=10 y=20 width=100 height=50 fill=\"#ffffff\" order=0 start=0 duration=2\n    box id=\"b\" x=50 y=30 width=80 height=40 fill=\"#ff0000\" order=0 start=0 duration=2\n    box id=\"c\" x=90 y=40 width=60 height=30 fill=\"#00ff00\" order=0 start=0 duration=2\n    box id=\"d\" x=130 y=50 width=40 height=20 fill=\"#0000ff\" order=0 start=0 duration=2 label=\"keep me\"\n}\n",
    );
}

fn open(folder: &Path) -> SdkSession {
    SdkSession::open(
        folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open")
}

fn open_history(folder: &Path) -> SdkSession {
    SdkSession::open(folder, OpenOptions::default()).expect("valid open")
}

/// Sorted map of every file under `folder` to its bytes: the
/// refused-edit oracle (composition, history, and delta unchanged).
fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
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

fn labels(session: &SdkSession) -> Vec<(String, Option<String>)> {
    session
        .effective_composition()
        .clips
        .iter()
        .map(|c| {
            (
                c.id().unwrap_or("?").to_string(),
                c.label().map(|s| s.to_string()),
            )
        })
        .collect()
}

fn doc_ids(session: &SdkSession) -> Vec<String> {
    session
        .effective_composition()
        .clips
        .iter()
        .map(|c| c.id().unwrap_or("?").to_string())
        .collect()
}

#[test]
fn group_collect_sets_notes_and_inverts() {
    let root = tmp("collect");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    layers_project(&folder);
    let mut session = open(&folder);

    let applied = session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("a"), ClipRef::by_id("b")],
            name: "credits".to_string(),
        })
        .expect("group applies");
    assert_eq!(
        applied.op.summary(),
        "group clip id \"a\", clip id \"b\" as \"credits\""
    );
    assert_eq!(
        labels(&session),
        vec![
            ("a".to_string(), Some("group:credits".to_string())),
            ("b".to_string(), Some("group:credits".to_string())),
            ("c".to_string(), None),
            ("d".to_string(), Some("keep me".to_string())),
        ]
    );
    match &applied.inverse {
        sdk::RevertOp::RestoreGroupLabels { members } => {
            assert_eq!(members.len(), 2);
            assert!(members.iter().all(|m| m.label.is_none()));
        }
        other => panic!("group inverse restores group notes: {other:?}"),
    }
    session
        .apply_revert(&applied.inverse)
        .expect("revert restores the notes");
    assert_eq!(
        labels(&session),
        vec![
            ("a".to_string(), None),
            ("b".to_string(), None),
            ("c".to_string(), None),
            ("d".to_string(), Some("keep me".to_string())),
        ]
    );
}

#[test]
fn group_persists_a_named_turn_and_undo_restores() {
    let root = tmp("turn");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    layers_project(&folder);
    let before_kdl = std::fs::read(folder.join("composition.kdl")).unwrap();
    let mut session = open_history(&folder);
    session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("b"), ClipRef::by_id("c")],
            name: "credits".to_string(),
        })
        .expect("group applies");
    let receipt = session.persist_turn("maker").expect("turn persists");
    assert_eq!(receipt.turn, Some(0));
    assert_eq!(receipt.actor.as_deref(), Some("maker"));
    let listing = sdk::history::list_turns(&folder).expect("history lists");
    assert_eq!(listing.turns.len(), 1);
    assert_eq!(listing.turns[0].actor, "maker");
    assert!(
        listing.turns[0].summary.contains("group"),
        "turn named: {}",
        listing.turns[0].summary
    );
    assert!(
        listing.turns[0].summary.contains("credits"),
        "group named: {}",
        listing.turns[0].summary
    );
    // Only the hidden delta writes: sources stay byte-identical.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        before_kdl
    );
    drop(session);
    // History undo is the shared undo: the group releases whole.
    sdk::history::undo_turn(&folder, 0).expect("undo reverses the turn");
    let session = open_history(&folder);
    assert!(
        session
            .effective_composition()
            .clips
            .iter()
            .all(|c| c.label().is_none_or(|l| !l.starts_with("group:"))),
        "undo releases every member"
    );
}

#[test]
fn group_refusals_name_the_cause_and_stage_nothing() {
    let root = tmp("refusals");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    layers_project(&folder);
    let mut session = open(&folder);

    // An empty group target list refuses: grouping an empty selection
    // does nothing.
    let refused = session
        .apply(EditOp::GroupElements {
            targets: vec![],
            name: "credits".to_string(),
        })
        .expect_err("empty group refuses");
    assert_eq!(refused.kind, EditRefusalKind::BadValue);
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);
    assert!(refused.message.contains("names no clips"), "{refused}");
    assert!(refused.message.contains("empty selection"), "{refused}");

    // An empty group name refuses.
    let refused = session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("a")],
            name: "  ".to_string(),
        })
        .expect_err("empty name refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);
    assert!(
        refused.message.contains("non-empty group name"),
        "{refused}"
    );

    // An unknown member refuses.
    let refused = session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("ghost")],
            name: "credits".to_string(),
        })
        .expect_err("unknown member refuses");
    assert_eq!(refused.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(refused.code, sdk::E_SDK_UNKNOWN_TARGET);

    // A noted member refuses: grouping would overwrite the note.
    let refused = session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("a"), ClipRef::by_id("d")],
            name: "credits".to_string(),
        })
        .expect_err("noted member refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);
    assert!(refused.message.contains("\"d\""), "{refused}");
    assert!(refused.message.contains("editor note"), "{refused}");

    // Collecting works once; collecting again refuses as already
    // grouped, and collecting into another name refuses too.
    session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("a")],
            name: "credits".to_string(),
        })
        .expect("first collect applies");
    let refused = session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("a")],
            name: "credits".to_string(),
        })
        .expect_err("double collect refuses");
    assert!(refused.message.contains("already grouped in"), "{refused}");
    assert!(refused.message.contains("\"credits\""), "{refused}");
    let refused = session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("a")],
            name: "other".to_string(),
        })
        .expect_err("cross-group collect refuses");
    assert!(refused.message.contains("already grouped in"), "{refused}");
    assert!(refused.message.contains("\"credits\""), "{refused}");
    // The failed collects stage nothing: only the first collect holds.
    assert_eq!(
        labels(&session),
        vec![
            ("a".to_string(), Some("group:credits".to_string())),
            ("b".to_string(), None),
            ("c".to_string(), None),
            ("d".to_string(), Some("keep me".to_string())),
        ]
    );
}

#[test]
fn refused_groups_leave_the_tree_untouched() {
    let root = tmp("untouched");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    layers_project(&folder);
    let before = tree_bytes(&folder);
    let mut session = open(&folder);
    let _ = session
        .apply(EditOp::GroupElements {
            targets: vec![],
            name: "credits".to_string(),
        })
        .expect_err("empty group refuses");
    let _ = session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("d")],
            name: "credits".to_string(),
        })
        .expect_err("noted member refuses");
    drop(session);
    assert_eq!(tree_bytes(&folder), before);
    // No hidden history writes on refusal either.
    assert!(
        !folder.join(".framesmith").exists(),
        "refusals write nothing anywhere"
    );
}

#[test]
fn ungroup_releases_members_and_names_holdings() {
    let root = tmp("ungroup");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    layers_project(&folder);
    let mut session = open(&folder);
    session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("a"), ClipRef::by_id("c")],
            name: "credits".to_string(),
        })
        .expect("group applies");

    let applied = session
        .apply(EditOp::UngroupElements {
            name: "credits".to_string(),
        })
        .expect("ungroup applies");
    assert_eq!(applied.op.summary(), "ungroup \"credits\"");
    assert_eq!(
        labels(&session),
        vec![
            ("a".to_string(), None),
            ("b".to_string(), None),
            ("c".to_string(), None),
            ("d".to_string(), Some("keep me".to_string())),
        ],
        "ungroup restores the members"
    );
    session
        .apply_revert(&applied.inverse)
        .expect("revert re-collects the group");
    assert_eq!(
        labels(&session)[0].1,
        Some("group:credits".to_string()),
        "undo of ungroup re-collects"
    );
    assert_eq!(
        labels(&session)[2].1,
        Some("group:credits".to_string()),
        "undo of ungroup re-collects"
    );

    // An unknown name refuses naming the holdings.
    session
        .apply(EditOp::UngroupElements {
            name: "credits".to_string(),
        })
        .expect("release applies");
    session
        .apply(EditOp::GroupElements {
            targets: vec![ClipRef::by_id("b")],
            name: "flair".to_string(),
        })
        .expect("second group applies");
    let refused = session
        .apply(EditOp::UngroupElements {
            name: "nope".to_string(),
        })
        .expect_err("unknown ungroup refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);
    assert!(
        refused.message.contains("names no grouped members"),
        "{refused}"
    );
    assert!(refused.message.contains("\"flair\""), "{refused}");

    // An empty name refuses.
    let refused = session
        .apply(EditOp::UngroupElements {
            name: "  ".to_string(),
        })
        .expect_err("empty ungroup refuses");
    assert!(refused.message.contains("grouped name"), "{refused}");
}

fn add_tween_on(session: &mut SdkSession, target: ClipRef) {
    session
        .apply(EditOp::AddTween {
            target,
            property: "x".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("120".to_string()),
            start: 0.0,
            duration: 1.0,
            ease: "in-out-quad".to_string(),
        })
        .expect("tween applies");
}

#[test]
fn move_refuses_when_motion_sits_inside_the_range() {
    let root = tmp("guard");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    layers_project(&folder);
    let mut session = open(&folder);
    // Motion targets clip index 1 (`b`).
    add_tween_on(&mut session, ClipRef::by_id("b"));

    // Moving `a` from 0 to 2 shuffles 0..=2, re-pointing the tween.
    let before = doc_ids(&session);
    let refused = session
        .apply(EditOp::MoveElement {
            target: ClipRef::by_id("a"),
            to: 2,
        })
        .expect_err("motion-breaking move refuses");
    assert_eq!(refused.kind, EditRefusalKind::BadValue);
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);
    assert!(refused.message.contains("re-pointing"), "{refused}");
    assert!(
        refused.message.contains("tween 0 (clip index 1)"),
        "{refused}"
    );
    assert!(refused.message.contains("z-index"), "{refused}");
    assert_eq!(doc_ids(&session), before, "refused move stages nothing");
    assert_eq!(
        session.effective_composition().tweens.len(),
        1,
        "tween untouched"
    );

    // A move outside the motion range still applies: the tween sits at
    // index 0 after moving `c` nowhere near it.
    let mut session = open(&folder);
    add_tween_on(&mut session, ClipRef::by_id("a"));
    session
        .apply(EditOp::MoveElement {
            target: ClipRef::by_id("b"),
            to: 2,
        })
        .expect("motion-free range moves");
    assert_eq!(doc_ids(&session), vec!["a", "c", "b", "d"]);
}

#[test]
fn move_guard_covers_sets_and_keys() {
    let root = tmp("sets-keys");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    layers_project(&folder);
    let mut session = open(&folder);
    session
        .apply(EditOp::AddSet {
            target: ClipRef::by_index(2),
            property: "x".to_string(),
            value: "5".to_string(),
            start: 0.0,
            duration: 1.0,
        })
        .expect("set applies");
    session
        .apply(EditOp::AddKey {
            target: ClipRef::by_index(2),
            property: "x".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 0.0,
                    value: "0".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 1.0,
                    value: "10".to_string(),
                    ease: None,
                },
            ],
            ease: None,
        })
        .expect("key applies");
    let refused = session
        .apply(EditOp::MoveElement {
            target: ClipRef::by_index(0),
            to: 3,
        })
        .expect_err("set/key-breaking move refuses");
    assert!(
        refused.message.contains("set 0 (clip index 2)"),
        "{refused}"
    );
    assert!(
        refused.message.contains("key 0 (clip index 2)"),
        "{refused}"
    );
    assert_eq!(doc_ids(&session), vec!["a", "b", "c", "d"]);
}
