//! Nest-element regressions (Sprint 18, T001): the Studio
//! Compositions panel places compositions as `nest` clips through
//! [`sdk::EditOp::AddElement`], so the SDK learns one new element
//! kind. Applying stages a graphics-track nest with the named target
//! and window, persisting replays it on reopen at the same position,
//! undo reverses it, and an empty target, a bad window, or a taken id
//! refuses with a reason code leaving the composition, the delta, and
//! history exactly unchanged.

use sdk::{EditOp, EditRefusalKind, NewElement, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-nest-{}-{}-{}",
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
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
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

fn nest_op(target: &str, start: f64, duration: f64) -> EditOp {
    EditOp::AddElement {
        element: NewElement::Nest {
            id: None,
            label: None,
            target: target.to_string(),
            start,
            duration,
        },
        position: None,
    }
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

#[test]
fn nest_element_applies_with_target_window_and_graphics_track() {
    let folder = tmp("apply");
    text_project(&folder);
    let mut session = open(&folder);
    let before = session.effective_composition().clips.len();
    session
        .apply(nest_op("b-roll", 0.0, 2.0))
        .expect("nest applies");
    assert_eq!(
        session.effective_composition().clips.len(),
        before + 1,
        "the nest lands on the timeline"
    );
}

#[test]
fn nest_element_carries_target_and_defaults() {
    let folder = tmp("shape");
    text_project(&folder);
    let mut session = open(&folder);
    session
        .apply(nest_op("b-roll", 1.0, 2.0))
        .expect("nest applies");
    let comp = session.effective_composition().clone();
    let Some(notation::model::Clip::Nest(nest)) = comp.clips.last() else {
        panic!("the landed clip is a nest");
    };
    assert_eq!(nest.target, "b-roll");
    assert_eq!((nest.start, nest.duration), (1.0, 2.0));
    assert_eq!(nest.track, notation::model::TrackKind::Graphics);
    assert_eq!(nest.rate, 1.0);
    assert_eq!(nest.trim_start, 0.0);
    assert_eq!(nest.trim_end, None);
    assert!(!nest.hidden);
}

#[test]
fn nest_apply_summarizes_and_undo_reverses() {
    let folder = tmp("undo");
    text_project(&folder);
    let mut session = open(&folder);
    let applied = session
        .apply(nest_op("b-roll", 0.0, 2.0))
        .expect("nest applies");
    assert_eq!(applied.op.summary(), "add nest element at end");
    let receipt = session.undo().expect("undo reverses");
    assert!(
        receipt.summary.contains("nest"),
        "undo receipt names the nest, got {:?}",
        receipt.summary
    );
    assert!(
        session
            .effective_composition()
            .clips
            .iter()
            .all(|c| !matches!(c, notation::model::Clip::Nest(_))),
        "no nest remains after undo"
    );
}

#[test]
fn nest_persists_in_the_delta_and_replays_on_reopen() {
    let folder = tmp("persist");
    text_project(&folder);
    let mut session = open(&folder);
    session
        .apply(nest_op("b-roll", 0.5, 1.5))
        .expect("nest applies");
    session.persist().expect("delta persists");
    let reopened = open(&folder);
    let comp = reopened.effective_composition().clone();
    let Some(notation::model::Clip::Nest(nest)) = comp.clips.last() else {
        panic!("the nest replays on reopen");
    };
    assert_eq!(nest.target, "b-roll");
    assert_eq!((nest.start, nest.duration), (0.5, 1.5));
}

#[test]
fn nest_with_empty_target_refuses_and_changes_nothing() {
    let folder = tmp("empty-target");
    text_project(&folder);
    let before = tree_bytes(&folder);
    let mut session = open(&folder);
    let err = session
        .apply(nest_op("   ", 0.0, 1.0))
        .expect_err("empty target refuses");
    assert_eq!(err.kind, EditRefusalKind::BadValue);
    assert_eq!(err.code, sdk::E_SDK_BAD_VALUE);
    assert!(err.message.contains("names no composition"), "{err}");
    assert_eq!(tree_bytes(&folder), before, "nothing was written");
}

#[test]
fn nest_with_bad_window_refuses_and_changes_nothing() {
    let folder = tmp("bad-window");
    text_project(&folder);
    let before = tree_bytes(&folder);
    let mut session = open(&folder);
    for (start, duration) in [(-1.0, 1.0), (0.0, 0.0), (f64::NAN, 1.0)] {
        let err = session
            .apply(nest_op("b-roll", start, duration))
            .expect_err("bad window refuses");
        assert_eq!(err.kind, EditRefusalKind::BadValue, "at {start}/{duration}");
    }
    assert_eq!(tree_bytes(&folder), before, "nothing was written");
}

#[test]
fn nest_with_taken_id_refuses_and_changes_nothing() {
    let folder = tmp("taken-id");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text id=\"hero\" value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let before = tree_bytes(&folder);
    let mut session = open(&folder);
    let err = session
        .apply(EditOp::AddElement {
            element: NewElement::Nest {
                id: Some("hero".to_string()),
                label: None,
                target: "b-roll".to_string(),
                start: 0.0,
                duration: 1.0,
            },
            position: None,
        })
        .expect_err("taken id refuses");
    assert_eq!(err.kind, EditRefusalKind::BadValue);
    assert!(err.message.contains("already taken"), "{err}");
    assert_eq!(tree_bytes(&folder), before, "nothing was written");
}

#[test]
fn nest_element_reports_its_id_and_kind() {
    let element = NewElement::Nest {
        id: Some("n1".to_string()),
        label: None,
        target: "b-roll".to_string(),
        start: 0.0,
        duration: 1.0,
    };
    assert_eq!(element.id(), Some("n1"));
    assert_eq!(element.what(), "nest element");
    let raw = serde_json::to_value(&element).expect("nest serializes");
    assert_eq!(raw.get("kind").and_then(|k| k.as_str()), Some("nest"));
    let back: NewElement = serde_json::from_value(raw).expect("nest round-trips");
    assert_eq!(back.id(), Some("n1"));
    assert_eq!(back.what(), "nest element");
}
