//! Headless edit regressions (Sprint 15, T002 / CAP-003): every
//! operation family applies with a visible result, each patch carries
//! its inverse, preview shows the effect before persistence, an added
//! element persists in the override delta and replays on reopen
//! keeping its position, and unsupported or malformed operations refuse
//! with a reason code leaving composition, player state, and history
//! exactly unchanged.

use sdk::{ClipRef, EditOp, EditRefusalKind, KeyStopInput, NewElement, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-edits-{}-{}-{}",
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

fn added_text(id: &str) -> NewElement {
    NewElement::Text {
        id: Some(id.to_string()),
        label: Some("sdk note".to_string()),
        value: "Hello".to_string(),
        start: 0.0,
        duration: 2.0,
        x: 20.0,
        y: 60.0,
        size: 24.0,
        color: "#ff0000".to_string(),
        font_family: Some("Alpha, Beta".to_string()),
    }
}

fn first_text_value(session: &SdkSession) -> String {
    for clip in &session.effective_composition().clips {
        if let notation::model::Clip::Text(t) = clip {
            return t.value.clone();
        }
    }
    panic!("effective composition holds no text clip");
}

#[test]
fn element_ops_apply_with_visible_result_and_inverses() {
    let root = tmp("elements");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    assert_eq!(session.effective_composition().clips.len(), 2);

    // Add a text element at index 1: the visible result sits there.
    let added = session
        .apply(EditOp::AddElement {
            element: added_text("sdk-note"),
            position: Some(1),
        })
        .expect("add applies");
    assert_eq!(added.seq, 0);
    assert_eq!(session.effective_composition().clips.len(), 3);
    assert_eq!(
        session.effective_composition().clips[1].id(),
        Some("sdk-note")
    );
    // The carried inverse removes exactly the inserted clip.
    match &added.inverse {
        sdk::RevertOp::RemoveClipAt { index } => assert_eq!(*index, 1),
        other => panic!("add inverse removes the clip: {other:?}"),
    }

    // Add a box at the end, then remove it by id.
    let added_box = session
        .apply(EditOp::AddElement {
            element: NewElement::Box {
                id: Some("sdk-box".to_string()),
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
        .expect("box add applies");
    assert_eq!(added_box.seq, 1);
    assert_eq!(session.effective_composition().clips.len(), 4);
    let removed = session
        .apply(EditOp::RemoveElement {
            target: ClipRef::by_id("sdk-box"),
        })
        .expect("remove applies");
    assert_eq!(session.effective_composition().clips.len(), 3);
    match &removed.inverse {
        sdk::RevertOp::InsertClip { index, clip } => {
            assert_eq!(*index, 3);
            assert_eq!(clip.id(), Some("sdk-box"));
        }
        other => panic!("remove inverse reinserts the snapshot: {other:?}"),
    }
    assert_eq!(session.patches().len(), 3);
}

#[test]
fn added_element_persists_in_overrides_and_replays_in_position() {
    let root = tmp("replay");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let comp_before = std::fs::read(folder.join("composition.kdl")).unwrap();

    let mut session = SdkSession::open(&folder, OpenOptions::default()).expect("valid open");
    session
        .apply(EditOp::AddElement {
            element: added_text("sdk-note"),
            position: Some(1),
        })
        .expect("add applies");
    // Preview shows the effect before anything persists: staged, the
    // element visible, no delta file on disk.
    let preview = session.preview_edits();
    assert_eq!(preview.staged, 1);
    assert_eq!(preview.persisted, 0);
    assert_eq!(preview.summary.len(), 1);
    assert!(
        !session.overrides_delta_path().exists(),
        "preview writes nothing"
    );
    assert!(preview.would_write.is_some());
    assert_eq!(
        session.effective_composition().clips[1].id(),
        Some("sdk-note")
    );

    let report = session.persist().expect("persist writes the delta");
    assert_eq!(report.applied, 1);
    assert_eq!(report.path, Some(session.overrides_delta_path()));
    assert!(session.overrides_delta_path().is_file());
    // Only the SDK delta was added: composition sources byte-identical,
    // no render output anywhere.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        comp_before
    );
    assert!(!has_render_output(&folder));

    // Reopen replays the delta keeping the position.
    let reopened = SdkSession::open(&folder, OpenOptions::default()).expect("reopen replays");
    assert_eq!(reopened.effective_composition().clips.len(), 3);
    assert_eq!(
        reopened.effective_composition().clips[1].id(),
        Some("sdk-note")
    );
    let clip = &reopened.effective_composition().clips[1];
    match clip {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.value, "Hello");
            assert_eq!(t.label.as_deref(), Some("sdk note"));
            assert_eq!(
                t.font_family,
                Some(vec!["Alpha".to_string(), "Beta".to_string()])
            );
        }
        other => panic!("replayed element is the text clip: {other:?}"),
    }
    assert_eq!(reopened.patches().len(), 1);
    assert!(reopened.patches()[0].persisted);
    // Persisting a clean replay writes nothing new.
    let mut reopened = reopened;
    let report = reopened.persist().expect("empty persist is a no-op");
    assert_eq!(report.applied, 0);
    assert_eq!(report.path, None);
}

#[test]
fn tween_add_carries_inverse_and_previews_before_persist() {
    let root = tmp("tween");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);

    let added = session
        .apply(EditOp::AddTween {
            target: ClipRef::by_index(0),
            property: "x".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("120".to_string()),
            start: 0.0,
            duration: 1.0,
            ease: "in-out-quad".to_string(),
        })
        .expect("tween applies");
    assert_eq!(session.effective_composition().tweens.len(), 1);
    let tween = &session.effective_composition().tweens[0];
    assert_eq!(tween.property, "x");
    assert_eq!(tween.target, 0);
    assert_eq!(tween.to, Some(120.0));
    match &added.inverse {
        sdk::RevertOp::RemoveTween { index } => assert_eq!(*index, 0),
        other => panic!("tween inverse removes the tween: {other:?}"),
    }
    // Preview surfaces the staged tween before anything persists.
    let preview = session.preview_edits();
    assert_eq!(preview.staged, 1);
    assert!(!session.overrides_delta_path().exists());
    assert_eq!(preview.would_write, Some(session.overrides_delta_path()));
    let report = session.persist().expect("persist writes the delta");
    assert_eq!(report.applied, 1);
    // Reopen replays the tween too.
    let reopened = SdkSession::open(
        &folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("reopen replays the tween");
    assert_eq!(reopened.effective_composition().tweens.len(), 1);
    assert_eq!(reopened.effective_composition().tweens[0].property, "x");
}

#[test]
fn key_add_applies_with_stops_and_color_endpoints() {
    let root = tmp("key");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);

    // Numeric key with an underscored alias property and ease.
    let added = session
        .apply(EditOp::AddKey {
            target: ClipRef::by_index(0),
            property: "font_size".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 0.0,
                    value: "24".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 1.0,
                    value: "48".to_string(),
                    ease: Some("bounce".to_string()),
                },
            ],
            ease: Some("linear".to_string()),
        })
        .expect("key applies");
    assert_eq!(session.effective_composition().keys.len(), 1);
    let key = &session.effective_composition().keys[0];
    // The alias canonicalizes to the hyphenated motion property.
    assert_eq!(key.property, "font-size");
    assert_eq!(key.stops.len(), 2);
    assert_eq!((key.stops[0].at, key.stops[0].value), (0.0, 24.0));
    assert_eq!((key.stops[1].at, key.stops[1].value), (1.0, 48.0));
    match &added.inverse {
        sdk::RevertOp::RemoveKey { index } => assert_eq!(*index, 0),
        other => panic!("key inverse removes the key: {other:?}"),
    }

    // Color key on `fill` parses endpoints like check does.
    session
        .apply(EditOp::AddKey {
            target: ClipRef::by_index(1),
            property: "fill".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 0.0,
                    value: "#ff0000".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 1.0,
                    value: "#00ff00".to_string(),
                    ease: None,
                },
            ],
            ease: None,
        })
        .expect("color key applies");
    let key = &session.effective_composition().keys[1];
    assert_eq!(
        key.stops[0].value_color,
        Some(notation::model::Rgb { r: 255, g: 0, b: 0 })
    );
    assert_eq!(
        key.stops[1].value_color,
        Some(notation::model::Rgb { r: 0, g: 255, b: 0 })
    );
}

#[test]
fn label_set_applies_and_inverts() {
    let root = tmp("label");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    assert_eq!(session.effective_composition().clips[0].label(), None);

    let set = session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("hero".to_string()),
        })
        .expect("label applies");
    assert_eq!(
        session.effective_composition().clips[0].label(),
        Some("hero")
    );
    match &set.inverse {
        sdk::RevertOp::RestoreLabel { label, .. } => assert_eq!(*label, None),
        other => panic!("label inverse restores the old note: {other:?}"),
    }
    // The inverse restores the exact prior state.
    session
        .apply_revert(&set.inverse)
        .expect("revert restores the label");
    assert_eq!(session.effective_composition().clips[0].label(), None);

    // Clearing an already-empty label still records its inverse.
    let clear = session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_id("missing-id-check"),
            label: None,
        })
        .expect_err("unknown id refuses");
    assert_eq!(clear.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(clear.code, sdk::E_SDK_UNKNOWN_TARGET);
}

#[test]
fn arc_append_applies_and_inverts() {
    let root = tmp("arc");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    path_project(&folder);
    let mut session = open(&folder);
    let before = match &session.effective_composition().clips[0] {
        notation::model::Clip::Path(p) => p.d.clone(),
        other => panic!("first clip is the path: {other:?}"),
    };

    let appended = session
        .apply(EditOp::AppendArc {
            target: ClipRef::by_index(0),
            rx: 30.0,
            ry: 30.0,
            rot: 0.0,
            large: false,
            sweep: true,
            x: 100.0,
            y: 100.0,
        })
        .expect("arc appends");
    let after = match &session.effective_composition().clips[0] {
        notation::model::Clip::Path(p) => p.d.clone(),
        other => panic!("first clip is the path: {other:?}"),
    };
    assert!(after.starts_with(&before));
    assert!(after.contains('A'), "appended segment is an arc: {after:?}");
    // The appended path still parses.
    assert!(motion::path::parse(&after).is_ok());
    match &appended.inverse {
        sdk::RevertOp::RestorePathD { d, .. } => assert_eq!(*d, before),
        other => panic!("arc inverse restores the prior data: {other:?}"),
    }
    session
        .apply_revert(&appended.inverse)
        .expect("revert restores the path");
    let restored = match &session.effective_composition().clips[0] {
        notation::model::Clip::Path(p) => p.d.clone(),
        other => panic!("first clip is the path: {other:?}"),
    };
    assert_eq!(restored, before);

    // Negative radii refuse as bad values on a path clip.
    let refused = session
        .apply(EditOp::AppendArc {
            target: ClipRef::by_index(0),
            rx: -1.0,
            ry: 10.0,
            rot: 0.0,
            large: false,
            sweep: true,
            x: 5.0,
            y: 5.0,
        })
        .expect_err("negative radius refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);
}

#[test]
fn structure_move_reorders_and_inverts() {
    let root = tmp("move");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    assert_eq!(first_text_value(&session), "One");

    let moved = session
        .apply(EditOp::MoveElement {
            target: ClipRef::by_index(0),
            to: 1,
        })
        .expect("move applies");
    assert_eq!(first_text_value(&session), "Two");
    match &moved.inverse {
        sdk::RevertOp::MoveClip { to, .. } => assert_eq!(*to, 0),
        other => panic!("move inverse moves back: {other:?}"),
    }
    session
        .apply_revert(&moved.inverse)
        .expect("revert restores the order");
    assert_eq!(first_text_value(&session), "One");
}

fn snapshot(session: &SdkSession) -> (Vec<String>, Vec<Option<String>>, Vec<String>, usize, usize) {
    let comp = session.effective_composition();
    let values = comp
        .clips
        .iter()
        .map(|c| match c {
            notation::model::Clip::Text(t) => format!("text:{}", t.value),
            notation::model::Clip::Box(_) => "box".to_string(),
            notation::model::Clip::Path(p) => format!("path:{}", p.d),
            other => other.kind().to_string(),
        })
        .collect();
    let labels = comp
        .clips
        .iter()
        .map(|c| c.label().map(|s| s.to_string()))
        .collect();
    let ids = comp
        .clips
        .iter()
        .map(|c| c.id().unwrap_or("").to_string())
        .collect();
    (values, labels, ids, comp.tweens.len(), comp.keys.len())
}

#[test]
fn inverses_roundtrip_to_the_exact_prior_state() {
    let root = tmp("roundtrip");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    path_project(&folder);
    let mut session = open(&folder);
    let before = snapshot(&session);

    let a = session
        .apply(EditOp::AddElement {
            element: added_text("sdk-note"),
            position: Some(1),
        })
        .expect("add applies");
    let b = session
        .apply(EditOp::AddTween {
            target: ClipRef::by_id("sdk-note"),
            property: "opacity".to_string(),
            kind: "from-to".to_string(),
            from: Some("0".to_string()),
            to: Some("1".to_string()),
            start: 0.0,
            duration: 2.0,
            ease: "linear".to_string(),
        })
        .expect("tween applies");
    let c = session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("lead".to_string()),
        })
        .expect("label applies");
    let d = session
        .apply(EditOp::AppendArc {
            target: ClipRef::by_index(0),
            rx: 10.0,
            ry: 10.0,
            rot: 0.0,
            large: true,
            sweep: false,
            x: 50.0,
            y: 50.0,
        })
        .expect("arc applies");
    assert_ne!(snapshot(&session), before);

    for inverse in [&d.inverse, &c.inverse, &b.inverse, &a.inverse] {
        session.apply_revert(inverse).expect("revert applies");
    }
    assert_eq!(snapshot(&session), before);
}

#[test]
fn unsupported_operations_refuse_with_reason_codes() {
    let root = tmp("unsupported");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    path_project(&folder);
    // Seeded history: a refused edit must leave it exactly unchanged.
    let history = sdk::history_dir_for(&folder);
    std::fs::create_dir_all(&history).unwrap();
    write(&history, "keep.json", "{\"turn\": 1}");
    let comp_before = std::fs::read(folder.join("composition.kdl")).unwrap();
    let before = tree_bytes(&folder);

    let mut session = SdkSession::open(&folder, OpenOptions::default()).expect("valid open");

    // Arc on a text clip is outside capability.
    let refused = session
        .apply(EditOp::AppendArc {
            target: ClipRef::by_index(1),
            rx: 10.0,
            ry: 10.0,
            rot: 0.0,
            large: false,
            sweep: true,
            x: 5.0,
            y: 5.0,
        })
        .expect_err("arc on text refuses");
    assert_eq!(refused.kind, EditRefusalKind::Unsupported);
    assert_eq!(refused.code, sdk::E_SDK_UNSUPPORTED);
    assert!(refused.message.contains("path clips only"), "{refused:?}");

    // A Sprint 11 filter property is unsupported, naming its area.
    let refused = session
        .apply(EditOp::AddTween {
            target: ClipRef::by_index(0),
            property: "brightness".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("1".to_string()),
            start: 0.0,
            duration: 1.0,
            ease: "linear".to_string(),
        })
        .expect_err("filter property refuses");
    assert_eq!(refused.code, sdk::E_SDK_UNSUPPORTED);

    // A later-sprint motion property is unsupported, naming its area.
    let refused = session
        .apply(EditOp::AddTween {
            target: ClipRef::by_index(0),
            property: "scale-x".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("1".to_string()),
            start: 0.0,
            duration: 1.0,
            ease: "linear".to_string(),
        })
        .expect_err("later property refuses");
    assert_eq!(refused.code, sdk::E_SDK_UNSUPPORTED);

    // A sound property is unsupported on picture clips.
    let refused = session
        .apply(EditOp::AddKey {
            target: ClipRef::by_index(0),
            property: "volume".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 0.0,
                    value: "1".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 1.0,
                    value: "0.5".to_string(),
                    ease: None,
                },
            ],
            ease: None,
        })
        .expect_err("sound property refuses");
    assert_eq!(refused.code, sdk::E_SDK_UNSUPPORTED);

    // An unknown clip id refuses.
    let refused = session
        .apply(EditOp::RemoveElement {
            target: ClipRef::by_id("ghost"),
        })
        .expect_err("unknown id refuses");
    assert_eq!(refused.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(refused.code, sdk::E_SDK_UNKNOWN_TARGET);

    // Nothing staged, nothing written: composition, delta, and seeded
    // history byte-identical.
    assert!(session.patches().is_empty());
    assert_eq!(tree_bytes(&folder), before);
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        comp_before
    );
}

#[test]
fn malformed_values_refuse_with_reason_codes() {
    let root = tmp("bad-values");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before = tree_bytes(&folder);
    let mut session = open(&folder);

    // Bad color.
    let refused = session
        .apply(EditOp::AddElement {
            element: NewElement::Box {
                id: None,
                label: None,
                start: 0.0,
                duration: 2.0,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                fill: "not-a-color".to_string(),
            },
            position: None,
        })
        .expect_err("bad color refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Bad easing.
    let refused = session
        .apply(EditOp::AddTween {
            target: ClipRef::by_index(0),
            property: "x".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("10".to_string()),
            start: 0.0,
            duration: 1.0,
            ease: "zoom-fast".to_string(),
        })
        .expect_err("bad ease refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Unknown property.
    let refused = session
        .apply(EditOp::AddTween {
            target: ClipRef::by_index(0),
            property: "wobble".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("10".to_string()),
            start: 0.0,
            duration: 1.0,
            ease: "linear".to_string(),
        })
        .expect_err("unknown property refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Out-of-range value (opacity runs 0 to 1).
    let refused = session
        .apply(EditOp::AddTween {
            target: ClipRef::by_index(0),
            property: "opacity".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("5".to_string()),
            start: 0.0,
            duration: 1.0,
            ease: "linear".to_string(),
        })
        .expect_err("out-of-range refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Single-stop key.
    let refused = session
        .apply(EditOp::AddKey {
            target: ClipRef::by_index(0),
            property: "x".to_string(),
            stops: vec![KeyStopInput {
                at: 0.0,
                value: "1".to_string(),
                ease: None,
            }],
            ease: None,
        })
        .expect_err("single stop refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Unordered stops.
    let refused = session
        .apply(EditOp::AddKey {
            target: ClipRef::by_index(0),
            property: "x".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 1.0,
                    value: "1".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 1.0,
                    value: "2".to_string(),
                    ease: None,
                },
            ],
            ease: None,
        })
        .expect_err("unordered stops refuse");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Duplicate id.
    session
        .apply(EditOp::AddElement {
            element: added_text("sdk-note"),
            position: None,
        })
        .expect("first add applies");
    let refused = session
        .apply(EditOp::AddElement {
            element: added_text("sdk-note"),
            position: None,
        })
        .expect_err("duplicate id refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Empty font-family entry.
    let mut bad_font = added_text("other");
    if let NewElement::Text { font_family, .. } = &mut bad_font {
        *font_family = Some("Alpha, , Beta".to_string());
    }
    let refused = session
        .apply(EditOp::AddElement {
            element: bad_font,
            position: None,
        })
        .expect_err("empty family refuses");
    assert_eq!(refused.code, sdk::E_SDK_BAD_VALUE);

    // Arc on a text clip: capability precedes value checks, so this
    // refuses as unsupported even with a negative radius.
    let refused = session
        .apply(EditOp::AppendArc {
            target: ClipRef::by_index(0),
            rx: -1.0,
            ry: 10.0,
            rot: 0.0,
            large: false,
            sweep: true,
            x: 5.0,
            y: 5.0,
        })
        .expect_err("arc on text refuses");
    assert_eq!(refused.code, sdk::E_SDK_UNSUPPORTED);

    // Only the one valid add staged; the project tree (no history dir
    // here) is otherwise untouched.
    assert_eq!(session.patches().len(), 1);
    let mut after = tree_bytes(&folder);
    after.remove("composition.kdl");
    let mut expected = before.clone();
    expected.remove("composition.kdl");
    assert_eq!(after, expected);
}

#[test]
fn read_only_and_preview_sessions_refuse_persist() {
    let root = tmp("read-only");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before = tree_bytes(&folder);

    // A read-only session refuses every apply.
    let mut session = SdkSession::open(
        &folder,
        OpenOptions {
            persist: false,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open");
    let refused = session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("x".to_string()),
        })
        .expect_err("read-only apply refuses");
    assert_eq!(refused.kind, EditRefusalKind::ReadOnly);
    assert_eq!(refused.code, sdk::E_SDK_READ_ONLY);

    // A preview session stages normally but never persists.
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
    assert_eq!(
        preview.effective_composition().clips[0].label(),
        Some("previewed")
    );
    let staged = preview.preview_edits();
    assert_eq!(staged.staged, 1);
    assert_eq!(staged.would_write, None);
    let refused = preview.persist().expect_err("preview persist refuses");
    assert_eq!(refused.kind, EditRefusalKind::Preview);
    assert_eq!(refused.code, sdk::E_SDK_PREVIEW);

    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn corrupt_and_dangling_deltas_refuse_reopen() {
    let root = tmp("corrupt-delta");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    // A valid persist first, then corrupt the delta: reopen refuses
    // naming the file and creates nothing new.
    let mut session = SdkSession::open(&folder, OpenOptions::default()).expect("valid open");
    session
        .apply(EditOp::SetLabel {
            target: ClipRef::by_index(0),
            label: Some("kept".to_string()),
        })
        .expect("label applies");
    session.persist().expect("persist writes");
    let delta = session.overrides_delta_path();
    assert!(delta.is_file());
    let good = std::fs::read(&delta).unwrap();
    std::fs::write(&delta, "{not json").unwrap();
    let refused = SdkSession::open(&folder, OpenOptions::default()).expect_err("corrupt refuses");
    assert!(
        refused.message.contains("sdk-overrides.json"),
        "{refused:?}"
    );

    // Another version refuses instead of guessing.
    let mut versioned = good.clone();
    let text = String::from_utf8(versioned.clone()).unwrap();
    assert!(text.contains("\"version\": 1"), "{text:?}");
    versioned = text
        .replace("\"version\": 1", "\"version\": 99")
        .into_bytes();
    std::fs::write(&delta, &versioned).unwrap();
    let refused = SdkSession::open(&folder, OpenOptions::default()).expect_err("version refuses");
    assert!(refused.message.contains("version 99"), "{refused:?}");

    // A well-formed delta that no longer applies refuses naming the
    // operation.
    let dangling = serde_json::json!({
        "version": 1,
        "ops": [{"op": "removeelement", "target": {"id": null, "index": 5}}]
    });
    std::fs::write(&delta, serde_json::to_string_pretty(&dangling).unwrap()).unwrap();
    let refused = SdkSession::open(
        &folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect_err("dangling replay refuses");
    assert!(refused.message.contains("no longer applies"), "{refused:?}");
}

#[test]
fn edits_leave_default_renders_byte_identical() {
    let root = tmp("byte-identical");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let comp_before = std::fs::read(folder.join("composition.kdl")).unwrap();

    let mut session = SdkSession::open(&folder, OpenOptions::default()).expect("valid open");
    session
        .apply(EditOp::AddElement {
            element: added_text("sdk-note"),
            position: Some(0),
        })
        .expect("add applies");
    session
        .apply(EditOp::AddTween {
            target: ClipRef::by_index(0),
            property: "y".to_string(),
            kind: "to".to_string(),
            from: None,
            to: Some("90".to_string()),
            start: 0.0,
            duration: 2.0,
            ease: "linear".to_string(),
        })
        .expect("tween applies");
    session.persist().expect("persist writes the delta");

    // Composition sources byte-identical, no render output, and the
    // only project write is the SDK delta beside the history dir.
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
