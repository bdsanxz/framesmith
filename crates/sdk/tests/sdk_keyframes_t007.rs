//! Keyframe edit regressions (Sprint 16, T007 / CAP-007): retiming a
//! stop, editing easing, removing a stop, and copying a key apply
//! through the patch machinery with exact inverses, while unknown
//! targets, order violations, and bad easings refuse with a reason
//! code leaving the composition and the undo stack exactly unchanged.

use sdk::{EditOp, EditRefusalKind, OpenOptions, SdkSession};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-keyframes-{tag}-{}-{}",
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

fn keyed_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"One\" start=0 duration=4 x=10 y=40 size=24 color=\"#ffffff\"\n    text id=\"b\" value=\"Two\" start=0 duration=4 x=10 y=80 size=24 color=\"#ffffff\"\n    key target=0 property=\"x\" ease=\"out-quad\" {\n        frame at=0 value=10\n        frame at=1 value=60 ease=\"in-quad\"\n        frame at=2 value=110\n    }\n    key target=1 property=\"opacity\" {\n        frame at=0 value=0\n        frame at=4 value=1\n    }\n}\n",
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

fn stop_ats(session: &SdkSession, key: usize) -> Vec<f64> {
    session.effective_composition().keys[key]
        .stops
        .iter()
        .map(|s| s.at)
        .collect()
}

#[test]
fn retime_stop_moves_and_undo_restores() {
    let dir = tmp("retime");
    keyed_project(&dir);
    let mut session = open(&dir);
    assert_eq!(stop_ats(&session, 0), vec![0.0, 1.0, 2.0]);
    let applied = session
        .apply(EditOp::RetimeKeyStop {
            key: 0,
            stop: 1,
            at: 1.5,
        })
        .expect("retime applies");
    assert_eq!(applied.op.summary(), "retime key 0 stop 1 to 1.5");
    assert_eq!(stop_ats(&session, 0), vec![0.0, 1.5, 2.0]);
    let undo = session.undo().expect("undo restores");
    assert!(undo.summary.contains("key 0 stop 1"), "{undo:?}");
    assert_eq!(stop_ats(&session, 0), vec![0.0, 1.0, 2.0]);
}

#[test]
fn retime_past_a_neighbor_refuses_and_changes_nothing() {
    let dir = tmp("retime-order");
    keyed_project(&dir);
    let mut session = open(&dir);
    let depth = session.patches().len();
    // Past the next stop: strictly rising order would break.
    let err = session
        .apply(EditOp::RetimeKeyStop {
            key: 0,
            stop: 0,
            at: 2.5,
        })
        .expect_err("order violation refuses");
    assert_eq!(err.kind, EditRefusalKind::BadValue);
    assert_eq!(err.code, sdk::E_SDK_BAD_VALUE);
    // Exactly onto the previous stop: still not strictly rising.
    let err = session
        .apply(EditOp::RetimeKeyStop {
            key: 0,
            stop: 2,
            at: 1.0,
        })
        .expect_err("landing on the neighbor refuses");
    assert_eq!(err.kind, EditRefusalKind::BadValue);
    // A negative or non-finite instant refuses too.
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        let err = session
            .apply(EditOp::RetimeKeyStop {
                key: 1,
                stop: 0,
                at: bad,
            })
            .expect_err("bad instant refuses");
        assert_eq!(err.kind, EditRefusalKind::BadValue, "at={bad:?}");
    }
    assert_eq!(stop_ats(&session, 0), vec![0.0, 1.0, 2.0]);
    assert_eq!(stop_ats(&session, 1), vec![0.0, 4.0]);
    assert_eq!(session.patches().len(), depth);
}

#[test]
fn retime_unknown_key_or_stop_refuses() {
    let dir = tmp("retime-unknown");
    keyed_project(&dir);
    let mut session = open(&dir);
    let depth = session.patches().len();
    let err = session
        .apply(EditOp::RetimeKeyStop {
            key: 9,
            stop: 0,
            at: 1.0,
        })
        .expect_err("unknown key refuses");
    assert_eq!(err.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(err.code, sdk::E_SDK_UNKNOWN_TARGET);
    let err = session
        .apply(EditOp::RetimeKeyStop {
            key: 0,
            stop: 9,
            at: 1.0,
        })
        .expect_err("unknown stop refuses");
    assert_eq!(err.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(session.patches().len(), depth);
}

#[test]
fn set_ease_key_level_and_stop_level_with_undo() {
    let dir = tmp("ease");
    keyed_project(&dir);
    let mut session = open(&dir);
    // Key-level fallback first.
    session
        .apply(EditOp::SetKeyEase {
            key: 1,
            stop: None,
            ease: "in-quad".to_string(),
        })
        .expect("key ease applies");
    assert_eq!(
        session.effective_composition().keys[1]
            .ease
            .as_ref()
            .map(|e| e.name()),
        Some("in-quad".to_string())
    );
    // Then one stop segment (underscores read as hyphens, like check).
    session
        .apply(EditOp::SetKeyEase {
            key: 0,
            stop: Some(2),
            ease: "in_quad".to_string(),
        })
        .expect("stop ease applies");
    assert_eq!(
        session.effective_composition().keys[0].stops[2]
            .ease
            .as_ref()
            .map(|e| e.name()),
        Some("in-quad".to_string())
    );
    // Undo walks back in reverse: the stop ease, then the key ease.
    session.undo().expect("undo restores stop ease");
    assert!(session.effective_composition().keys[0].stops[2]
        .ease
        .is_none());
    session.undo().expect("undo restores key ease");
    assert!(session.effective_composition().keys[1].ease.is_none());
}

#[test]
fn set_ease_unknown_curve_or_target_refuses() {
    let dir = tmp("ease-refused");
    keyed_project(&dir);
    let mut session = open(&dir);
    let depth = session.patches().len();
    let err = session
        .apply(EditOp::SetKeyEase {
            key: 0,
            stop: None,
            ease: "wobble".to_string(),
        })
        .expect_err("unknown curve refuses");
    assert_eq!(err.kind, EditRefusalKind::BadValue);
    assert!(err.message.contains("\"wobble\""), "{err}");
    let err = session
        .apply(EditOp::SetKeyEase {
            key: 7,
            stop: None,
            ease: "linear".to_string(),
        })
        .expect_err("unknown key refuses");
    assert_eq!(err.kind, EditRefusalKind::UnknownTarget);
    let err = session
        .apply(EditOp::SetKeyEase {
            key: 0,
            stop: Some(7),
            ease: "linear".to_string(),
        })
        .expect_err("unknown stop refuses");
    assert_eq!(err.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(session.patches().len(), depth);
    assert_eq!(
        session.effective_composition().keys[0]
            .ease
            .as_ref()
            .map(|e| e.name()),
        Some("out-quad".to_string())
    );
}

#[test]
fn remove_stop_shrinks_and_undo_reinserts_exactly() {
    let dir = tmp("remove-stop");
    keyed_project(&dir);
    let mut session = open(&dir);
    session
        .apply(EditOp::RemoveKeyStop { key: 0, stop: 1 })
        .expect("stop removal applies");
    assert_eq!(stop_ats(&session, 0), vec![0.0, 2.0]);
    // The surviving stops keep their values and easings.
    let key = &session.effective_composition().keys[0];
    assert_eq!(key.stops[0].value, 10.0);
    assert_eq!(key.stops[1].value, 110.0);
    session.undo().expect("undo reinserts the stop");
    assert_eq!(stop_ats(&session, 0), vec![0.0, 1.0, 2.0]);
    let key = &session.effective_composition().keys[0];
    assert_eq!(key.stops[1].value, 60.0);
    assert_eq!(
        key.stops[1].ease.as_ref().map(|e| e.name()),
        Some("in-quad".to_string())
    );
}

#[test]
fn remove_stop_from_a_two_stop_key_removes_the_whole_key() {
    let dir = tmp("remove-two-stop");
    keyed_project(&dir);
    let mut session = open(&dir);
    assert_eq!(session.effective_composition().keys.len(), 2);
    session
        .apply(EditOp::RemoveKeyStop { key: 1, stop: 0 })
        .expect("two-stop removal applies");
    assert_eq!(session.effective_composition().keys.len(), 1);
    assert_eq!(session.effective_composition().keys[0].property, "x");
    session.undo().expect("undo restores the whole key");
    assert_eq!(session.effective_composition().keys.len(), 2);
    assert_eq!(stop_ats(&session, 1), vec![0.0, 4.0]);
}

#[test]
fn remove_stop_unknown_target_refuses() {
    let dir = tmp("remove-unknown");
    keyed_project(&dir);
    let mut session = open(&dir);
    let depth = session.patches().len();
    let err = session
        .apply(EditOp::RemoveKeyStop { key: 5, stop: 0 })
        .expect_err("unknown key refuses");
    assert_eq!(err.kind, EditRefusalKind::UnknownTarget);
    let err = session
        .apply(EditOp::RemoveKeyStop { key: 0, stop: 5 })
        .expect_err("unknown stop refuses");
    assert_eq!(err.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(session.patches().len(), depth);
    assert_eq!(session.effective_composition().keys.len(), 2);
}

#[test]
fn duplicate_key_appends_a_copy_and_undo_removes_it() {
    let dir = tmp("duplicate");
    keyed_project(&dir);
    let mut session = open(&dir);
    session
        .apply(EditOp::DuplicateKey { key: 0 })
        .expect("copy applies");
    assert_eq!(session.effective_composition().keys.len(), 3);
    let (orig, copy) = {
        let keys = &session.effective_composition().keys;
        (keys[0].property.clone(), keys[2].property.clone())
    };
    assert_eq!(orig, copy);
    assert_eq!(stop_ats(&session, 2), vec![0.0, 1.0, 2.0]);
    session.undo().expect("undo removes the copy");
    assert_eq!(session.effective_composition().keys.len(), 2);
    let err = session
        .apply(EditOp::DuplicateKey { key: 6 })
        .expect_err("copying nothing refuses");
    assert_eq!(err.kind, EditRefusalKind::UnknownTarget);
    assert_eq!(err.code, sdk::E_SDK_UNKNOWN_TARGET);
}

#[test]
fn keyframe_edits_persist_and_replay_on_reopen() {
    let dir = tmp("replay");
    keyed_project(&dir);
    let mut session = SdkSession::open(&dir, OpenOptions::default()).expect("valid open");
    session
        .apply(EditOp::RetimeKeyStop {
            key: 0,
            stop: 2,
            at: 2.5,
        })
        .expect("retime applies");
    session
        .apply(EditOp::SetKeyEase {
            key: 0,
            stop: None,
            ease: "bounce".to_string(),
        })
        .expect("ease applies");
    session
        .apply(EditOp::DuplicateKey { key: 1 })
        .expect("copy applies");
    let report = session.persist().expect("persist writes the delta");
    assert_eq!(report.applied, 3);
    // Reopening replays the persisted delta onto the validated base.
    let fresh = SdkSession::open(&dir, OpenOptions::default()).expect("reopen replays");
    assert_eq!(stop_ats(&fresh, 0), vec![0.0, 1.0, 2.5]);
    assert_eq!(
        fresh.effective_composition().keys[0]
            .ease
            .as_ref()
            .map(|e| e.name()),
        Some("bounce".to_string())
    );
    assert_eq!(fresh.effective_composition().keys.len(), 3);
}
