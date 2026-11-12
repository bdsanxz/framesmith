//! Undo and redo regressions (Sprint 15, T003 / CAP-004): undo walks
//! applied patches restoring exact prior sources, redo re-applies
//! them symmetrically, the 101st entry drops the oldest with the
//! listing reporting the drop, capability checks refuse with reason
//! codes before anything changes, a patch failing validation blocks
//! persistence naming offenders while staged state is kept, and undo
//! with a missing target or backup refuses naming which one is
//! missing.

use sdk::{ClipRef, EditOp, EditRefusalKind, KeyStopInput, NewElement, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-undo-{}-{}-{}",
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
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n    text value=\"Two\" start=0 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn path_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    path d=\"M 0 0 L 100 0 L 100 100\" start=0 duration=2 x=0 y=0\n    text value=\"Hi\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
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

/// Exact-source oracle: the debug rendering of the effective
/// composition captures every staged field, so equality means the
/// sources restored exactly, never approximately.
fn snap(session: &SdkSession) -> String {
    format!("{:#?}", session.effective_composition())
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

fn has_render_output(folder: &Path) -> bool {
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "mp4" | "png" | "mov" | "mkv" | "webm") {
                    return true;
                }
            }
        }
    }
    false
}

#[test]
fn undo_redo_restore_exact_sources_symmetrically() {
    let root = tmp("roundtrip");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    path_project(&folder);
    let mut session = open(&folder);
    let base = snap(&session);
    assert_eq!(session.undo_listing().depth, 0);
    assert_eq!(session.undo_listing().drop_report, None);

    // One patch per family: element add, motion tween (underscored
    // alias canonicalizes to the hyphenated property), key, and label.
    session
        .apply(EditOp::AddElement {
            element: NewElement::Text {
                id: Some("sdk-note".to_string()),
                label: Some("sdk note".to_string()),
                value: "Hello".to_string(),
                start: 0.0,
                duration: 2.0,
                x: 20.0,
                y: 60.0,
                size: 24.0,
                color: "#ff0000".to_string(),
                font_family: Some("Alpha, Beta".to_string()),
            },
            position: Some(1),
        })
        .expect("add applies");
    session
        .apply(EditOp::AddTween {
            target: ClipRef::by_id("sdk-note"),
            property: "font_size".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("48".to_string()),
            start: 0.0,
            duration: 2.0,
            ease: "linear".to_string(),
        })
        .expect("tween applies");
    session
        .apply(EditOp::AddKey {
            target: ClipRef::by_index(0),
            property: "x".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 0.0,
                    value: "0".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 1.0,
                    value: "100".to_string(),
                    ease: None,
                },
            ],
            ease: None,
        })
        .expect("key applies");
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("lead".to_string()),
        })
        .expect("label applies");
    let full = snap(&session);
    assert_ne!(full, base);
    assert_eq!(session.patches().len(), 4);
    assert_eq!(session.undo_listing().depth, 4);
    assert_eq!(session.undo_listing().redo_depth, 0);

    // The hyphenated CSS-like surface survives staging: the
    // comma-separated font try-order passes through unresolved and
    // the alias canonicalizes to the hyphenated motion property.
    let comp = session.effective_composition();
    match &comp.clips[1] {
        notation::model::Clip::Text(t) => assert_eq!(
            t.font_family,
            Some(vec!["Alpha".to_string(), "Beta".to_string()]),
            "font-family try-order passes through, never substituted"
        ),
        other => panic!("added clip is text: {other:?}"),
    }
    assert_eq!(comp.tweens[0].property, "font-size");

    // Undo walks newest-first with receipts; each step restores.
    let r3 = session.undo().expect("undo label");
    assert_eq!(r3.seq, 3);
    assert!(r3.redo_available);
    assert!(r3.summary.contains("label"), "{r3:?}");
    let r2 = session.undo().expect("undo key");
    assert_eq!(r2.seq, 2);
    let r1 = session.undo().expect("undo tween");
    assert_eq!(r1.seq, 1);
    assert!(r1.inverse_summary.contains("tween"), "{r1:?}");
    let r0 = session.undo().expect("undo add");
    assert_eq!(r0.seq, 0);
    assert_eq!(snap(&session), base, "undo restores exact prior sources");
    let listing = session.undo_listing();
    assert_eq!(listing.depth, 0);
    assert_eq!(listing.redo_depth, 4);

    // Redo re-applies symmetrically with the same sequence numbers.
    for want in 0..4u64 {
        let receipt = session.redo().expect("redo applies");
        assert_eq!(receipt.seq, want);
        assert!(receipt.undo_available);
    }
    assert_eq!(
        snap(&session),
        full,
        "redo restores the exact staged sources"
    );
    // Fail-closed fonts and hyphenated properties round-trip too.
    let comp = session.effective_composition();
    match &comp.clips[1] {
        notation::model::Clip::Text(t) => assert_eq!(
            t.font_family,
            Some(vec!["Alpha".to_string(), "Beta".to_string()])
        ),
        other => panic!("added clip is text: {other:?}"),
    }
    assert_eq!(comp.tweens[0].property, "font-size");
    let listing = session.undo_listing();
    assert_eq!(listing.depth, 4);
    assert_eq!(listing.redo_depth, 0);
    assert_eq!(listing.entries.len(), 4);
    assert_eq!(listing.entries[0].seq, 0);
}

#[test]
fn overflow_drops_oldest_and_listing_reports() {
    let root = tmp("bound");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);

    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("v0".to_string()),
        })
        .expect("first applies");
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("v1".to_string()),
        })
        .expect("second applies");
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("v2".to_string()),
        })
        .expect("third applies");
    let after_third = snap(&session);

    // 103 applies total: the 101st drops the oldest, and two more
    // drop two more.
    for i in 3..103u32 {
        session
            .apply(EditOp::SetLabel {
                target: ClipRef::by_index(0),
                label: Some(format!("v{i}")),
            })
            .expect("label applies");
    }
    assert_eq!(session.patches().len(), 100);
    assert_eq!(session.patches()[0].seq, 3);
    assert_eq!(session.patches()[99].seq, 102);
    let listing = session.undo_listing();
    assert_eq!(listing.depth, 100);
    assert_eq!(listing.capacity, 100);
    assert_eq!(listing.dropped, 3);
    let report = listing.drop_report.expect("listing reports the drop");
    assert!(report.contains("3"), "{report:?}");
    assert!(report.contains("100-entry bound"), "{report:?}");
    assert_eq!(listing.entries.len(), 100);
    assert_eq!(listing.entries[0].seq, 3);

    // Undoing all 100 restores the state the third patch left: the
    // dropped oldest two are no longer walkable, the third is.
    for _ in 0..100 {
        session.undo().expect("undo walks");
    }
    assert_eq!(snap(&session), after_third);
    assert_eq!(session.undo_listing().depth, 0);
    assert_eq!(session.undo_listing().redo_depth, 100);
    // Nothing left to undo refuses with its reason code.
    let refused = session.undo().expect_err("empty undo refuses");
    assert_eq!(refused.kind, EditRefusalKind::NothingToUndo);
    assert_eq!(refused.code, sdk::E_SDK_NOTHING_TO_UNDO);
    assert!(refused.message.contains("no patch target"), "{refused:?}");

    // Redoing all 100 restores the newest state symmetrically.
    for _ in 0..100 {
        session.redo().expect("redo walks");
    }
    assert_eq!(
        session.effective_composition().clips[0].label(),
        Some("v102")
    );
    assert_eq!(session.undo_listing().depth, 100);
}

#[test]
fn capability_refusals_change_nothing() {
    let root = tmp("caps");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before = tree_bytes(&folder);

    // An empty undo stack refuses with its reason code.
    let mut session = open(&folder);
    let refused = session.undo().expect_err("empty undo refuses");
    assert_eq!(refused.kind, EditRefusalKind::NothingToUndo);
    assert_eq!(refused.code, sdk::E_SDK_NOTHING_TO_UNDO);
    let refused = session.redo().expect_err("empty redo refuses");
    assert_eq!(refused.kind, EditRefusalKind::NothingToRedo);
    assert_eq!(refused.code, sdk::E_SDK_NOTHING_TO_REDO);
    assert!(session.patches().is_empty());
    assert_eq!(tree_bytes(&folder), before);

    // A read-only session refuses undo and redo before anything changes.
    let mut read_only = SdkSession::open(
        &folder,
        OpenOptions {
            persist: false,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid read-only open");
    let refused = read_only.undo().expect_err("read-only undo refuses");
    assert_eq!(refused.kind, EditRefusalKind::ReadOnly);
    assert_eq!(refused.code, sdk::E_SDK_READ_ONLY);
    let refused = read_only.redo().expect_err("read-only redo refuses");
    assert_eq!(refused.kind, EditRefusalKind::ReadOnly);
    assert_eq!(refused.code, sdk::E_SDK_READ_ONLY);
    assert_eq!(tree_bytes(&folder), before);

    // A new apply after undo branches: the undone tail no longer
    // re-applies, so redo refuses naming the empty stack.
    let mut session = open(&folder);
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("a".to_string()),
        })
        .expect("first applies");
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("b".to_string()),
        })
        .expect("second applies");
    session.undo().expect("undo second");
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("c".to_string()),
        })
        .expect("branch applies");
    let refused = session.redo().expect_err("branched redo refuses");
    assert_eq!(refused.code, sdk::E_SDK_NOTHING_TO_REDO);
    assert_eq!(session.effective_composition().clips[0].label(), Some("c"));
    assert_eq!(tree_bytes(&folder), before);

    // Preview sessions stage in memory: undo and redo work there, and
    // only persistence refuses.
    let mut preview = SdkSession::open(
        &folder,
        OpenOptions {
            preview: true,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid preview open");
    preview
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("previewed".to_string()),
        })
        .expect("preview stages");
    preview.undo().expect("preview undo works in memory");
    assert_eq!(preview.effective_composition().clips[0].label(), None);
    preview.redo().expect("preview redo works in memory");
    assert_eq!(
        preview.effective_composition().clips[0].label(),
        Some("previewed")
    );
    let refused = preview.persist().expect_err("preview persist refuses");
    assert_eq!(refused.code, sdk::E_SDK_PREVIEW);
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn broken_turn_blocks_persist_naming_offenders() {
    let root = tmp("broken");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let comp_before = std::fs::read(folder.join("composition.kdl")).unwrap();

    // A project that no longer validates blocks with its offender list.
    let mut session = open(&folder);
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("kept".to_string()),
        })
        .expect("label applies");
    write(&folder, "composition.kdl", "{ this is not kdl\n");
    let refused = session.persist().expect_err("broken project blocks");
    assert_eq!(refused.kind, EditRefusalKind::BrokenTurn);
    assert_eq!(refused.code, sdk::E_SDK_BROKEN_TURN);
    assert!(refused.message.contains("composition.kdl"), "{refused:?}");
    assert!(
        refused.message.contains("Nothing was persisted"),
        "{refused:?}"
    );
    // Staged pre-edit state is kept: patch, effect, and no delta.
    assert_eq!(session.patches().len(), 1);
    assert_eq!(
        session.effective_composition().clips[0].label(),
        Some("kept")
    );
    assert!(!session.overrides_delta_path().exists());
    // Fixing the cause lets the same staged patch persist.
    write(
        &folder,
        "composition.kdl",
        &String::from_utf8(comp_before.clone()).unwrap(),
    );
    let report = session.persist().expect("fixed project persists");
    assert_eq!(report.applied, 1);
    assert!(session.overrides_delta_path().is_file());

    // A patch that no longer applies blocks naming the offender patch.
    let root = tmp("stale");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let comp_before = std::fs::read(folder.join("composition.kdl")).unwrap();
    let mut session = open(&folder);
    session
        .apply(EditOp::RemoveElement {
            target: ClipRef::by_index(1),
        })
        .expect("remove applies");
    // Externally drop the second clip: the project stays valid but the
    // staged remove dangles.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let refused = session.persist().expect_err("stale patch blocks");
    assert_eq!(refused.code, sdk::E_SDK_BROKEN_TURN);
    assert!(refused.message.contains("patch 0"), "{refused:?}");
    assert!(
        refused.message.contains(sdk::E_SDK_UNKNOWN_TARGET),
        "{refused:?}"
    );
    assert!(
        refused.message.contains("Nothing was persisted"),
        "{refused:?}"
    );
    // Staged state kept: the patch and its staged effect survive.
    assert_eq!(session.patches().len(), 1);
    assert_eq!(session.effective_composition().clips.len(), 1);
    assert!(!session.overrides_delta_path().exists());
    // Restoring the composition lets the same staged patch persist.
    write(
        &folder,
        "composition.kdl",
        &String::from_utf8(comp_before).unwrap(),
    );
    let report = session.persist().expect("restored project persists");
    assert_eq!(report.applied, 1);
}

#[test]
fn undo_missing_target_or_backup_names_which() {
    // A missing target: the inverse addresses a clip index that no
    // longer exists.
    let root = tmp("missing-target");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    path_project(&folder);
    let before = tree_bytes(&folder);
    let mut session = open(&folder);
    session
        .apply(EditOp::AddElement {
            element: NewElement::Box {
                id: None,
                label: None,
                start: 0.0,
                duration: 2.0,
                x: 5.0,
                y: 5.0,
                width: 40.0,
                height: 30.0,
                fill: "#00ff00".to_string(),
            },
            position: None,
        })
        .expect("add applies");
    // Disturb the staged composition outside the undo stack: two raw
    // reverts shrink past what the recorded inverse addresses.
    let raw = sdk::RevertOp::RemoveClipAt { index: 0 };
    session.apply_revert(&raw).expect("raw revert removes");
    session
        .apply_revert(&raw)
        .expect("raw revert removes again");
    assert_eq!(session.effective_composition().clips.len(), 1);
    let refused = session
        .undo()
        .expect_err("undo with missing target refuses");
    assert_eq!(refused.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(refused.code, sdk::E_SDK_UNKNOWN_TARGET);
    assert!(
        refused.message.contains("its target is missing"),
        "{refused:?}"
    );
    assert!(refused.message.contains("patch 0"), "{refused:?}");
    // Nothing reversed: the patch stays parked and the tree is untouched
    // apart from the pre-existing staged composition.
    assert_eq!(session.patches().len(), 1);
    assert_eq!(session.undo_listing().redo_depth, 0);
    assert_eq!(session.effective_composition().clips.len(), 1);
    let mut changed = tree_bytes(&folder);
    changed.remove("composition.kdl");
    let mut expected = before.clone();
    expected.remove("composition.kdl");
    assert_eq!(changed, expected);

    // A missing backup: the reinsert snapshot collides with a live `id`.
    let root = tmp("missing-backup");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    session
        .apply(EditOp::AddElement {
            element: NewElement::Text {
                id: Some("sdk-note".to_string()),
                label: None,
                value: "Hello".to_string(),
                start: 0.0,
                duration: 2.0,
                x: 0.0,
                y: 0.0,
                size: 24.0,
                color: "#ff0000".to_string(),
                font_family: None,
            },
            position: None,
        })
        .expect("add applies");
    session
        .apply(EditOp::RemoveElement {
            target: ClipRef::by_id("sdk-note"),
        })
        .expect("remove applies");
    // Reinsert the backup outside the undo stack, so the recorded undo
    // finds the `id` already taken.
    let backup = session.patches()[1].inverse.clone();
    session.apply_revert(&backup).expect("raw backup reinsert");
    let refused = session.undo().expect_err("undo with taken backup refuses");
    assert_eq!(refused.kind, EditRefusalKind::BadValue);
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);
    assert!(
        refused.message.contains("its backup cannot restore"),
        "{refused:?}"
    );
    assert!(refused.message.contains("patch 1"), "{refused:?}");
    assert_eq!(session.patches().len(), 2);
    assert_eq!(session.undo_listing().redo_depth, 0);
}

#[test]
fn persist_undo_redo_roundtrip_rewrites_delta() {
    let root = tmp("persist-roundtrip");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let comp_before = std::fs::read(folder.join("composition.kdl")).unwrap();

    let mut session = open(&folder);
    session
        .apply(EditOp::AddElement {
            element: NewElement::Text {
                id: Some("sdk-note".to_string()),
                label: None,
                value: "Hello".to_string(),
                start: 0.0,
                duration: 2.0,
                x: 20.0,
                y: 60.0,
                size: 24.0,
                color: "#ff0000".to_string(),
                font_family: Some("Alpha, Beta".to_string()),
            },
            position: Some(1),
        })
        .expect("add applies");
    let report = session.persist().expect("persist writes");
    assert_eq!(report.applied, 1);
    let full = snap(&session);

    // Undo after persist diverges in memory; persisting rewrites the
    // delta without the undone patch (applied counts only newly
    // staged edits) and reopening sees the undone state.
    session.undo().expect("undo after persist");
    assert_eq!(session.effective_composition().clips.len(), 2);
    let report = session.persist().expect("persist rewrites the undo");
    assert_eq!(report.applied, 0);
    assert_eq!(report.path, Some(session.overrides_delta_path()));
    let reopened = open(&folder);
    assert_eq!(reopened.effective_composition().clips.len(), 2);
    assert_eq!(snap(&reopened), snap(&session));

    // Redo re-applies symmetrically and persists the return.
    session.redo().expect("redo after persist");
    assert_eq!(snap(&session), full);
    let report = session.persist().expect("persist rewrites the redo");
    assert_eq!(report.path, Some(session.overrides_delta_path()));
    let reopened = open(&folder);
    assert_eq!(reopened.effective_composition().clips.len(), 3);
    assert_eq!(snap(&reopened), full);

    // Composition sources stay byte-identical, nothing rendered, and
    // the only project write is the SDK delta beside history.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        comp_before
    );
    assert!(!has_render_output(&folder));
    // Separator-agnostic: Windows tree keys carry backslashes, so
    // normalize before comparing to the slash forms.
    let mut names: Vec<String> = tree_bytes(&folder)
        .keys()
        .map(|n| n.replace('\\', "/"))
        .collect();
    names.sort();
    assert!(
        names
            .iter()
            .all(|n| n == "composition.kdl" || n.starts_with(".framesmith/")),
        "{names:?}"
    );
    assert!(
        names.contains(&".framesmith/sdk-overrides.json".to_string()),
        "{names:?}"
    );
}
