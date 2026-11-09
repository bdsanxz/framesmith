//! Audio-rack lane regressions (Sprint 18, T007 / CAP-007): range
//! edits (ramp, swell, dip, simplify, copy, paste) rewrite `volume`
//! and `pan` key stops exactly while stops outside the window keep
//! their exact instant, value, and easing; silent targets and empty
//! ranges refuse by name; undo and redo reverse whole rewrites; the
//! persisted delta replays lane ops on reopen.

use sdk::{ClipRef, EditOp, EditRefusalKind, KeyStopInput, LaneEdit, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-rack-{tag}-{}-{}",
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

/// Project with two audio lanes, one video lane, and a silent text
/// clip: the rack's working set.
fn lane_project(folder: &Path) {
    write(folder, "s.wav", "not-a-real-sound");
    write(folder, "v.mp4", "not-a-real-video");
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=4 fps=10 background=\"#112233\" version=1 {\n    audio id=\"music\" path=\"s.wav\" start=0 duration=4\n    audio id=\"voice\" path=\"s.wav\" start=0 duration=4\n    video id=\"clip\" path=\"v.mp4\" start=0 duration=4\n    text id=\"title\" value=\"Hi\" start=0 duration=4 x=10 y=40 size=24 color=\"#ffffff\"\n    key target=0 property=\"volume\" {\n        frame at=0 value=1\n        frame at=1 value=0.8\n        frame at=2 value=0.6\n        frame at=3 value=0.8\n    }\n    key target=1 property=\"pan\" {\n        frame at=0 value=-1\n        frame at=4 value=1 ease=\"in-quad\"\n    }\n    key target=2 property=\"volume\" {\n        frame at=0 value=1\n        frame at=2 value=1\n        frame at=4 value=1\n    }\n}\n",
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
                out.insert(
                    path.strip_prefix(folder)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    out
}

/// `(at, value)` pairs of one key.
fn stops_of(session: &SdkSession, key: usize) -> Vec<(f64, f64)> {
    session.effective_composition().keys[key]
        .stops
        .iter()
        .map(|s| (s.at, s.value))
        .collect()
}

/// `(at, value)` pairs of every key on one lane, in key order.
fn lane_stops(session: &SdkSession, target: usize, property: &str) -> Vec<Vec<(f64, f64)>> {
    session
        .effective_composition()
        .keys
        .iter()
        .filter(|k| k.target == target && k.property == property)
        .map(|k| k.stops.iter().map(|s| (s.at, s.value)).collect())
        .collect()
}

#[test]
fn ramp_rewrites_window_and_keeps_outside_exact() {
    let dir = tmp("ramp");
    lane_project(&dir);
    let mut session = open(&dir);
    // Lane reads: at 1 the lane rides 0.8, at 3 it rides 0.8.
    let applied = session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "volume".to_string(),
            start: 1.0,
            end: 3.0,
            edit: LaneEdit::Ramp {
                from: None,
                to: None,
            },
        })
        .expect("ramp applies");
    assert_eq!(
        applied.op.summary(),
        "ramp lane `volume` on clip id \"music\" across 1..3"
    );
    // The old key kept its outside stop (0,1) plus the window-edge
    // stop (1,0.8) so its side still renders exactly; the fresh key
    // rides the window.
    assert_eq!(
        lane_stops(&session, 0, "volume"),
        vec![vec![(0.0, 1.0), (1.0, 0.8)], vec![(1.0, 0.8), (3.0, 0.8)]]
    );
    // Undo restores the exact prior key list.
    session.undo().expect("undo reverses the ramp");
    assert_eq!(
        stops_of(&session, 0),
        vec![(0.0, 1.0), (1.0, 0.8), (2.0, 0.6), (3.0, 0.8)]
    );
    session.redo().expect("redo re-applies the ramp");
    assert_eq!(
        lane_stops(&session, 0, "volume"),
        vec![vec![(0.0, 1.0), (1.0, 0.8)], vec![(1.0, 0.8), (3.0, 0.8)]]
    );
}

#[test]
fn ramp_with_explicit_endpoints_sets_levels() {
    let dir = tmp("ramp-explicit");
    lane_project(&dir);
    let mut session = open(&dir);
    session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "volume".to_string(),
            start: 0.0,
            end: 2.0,
            edit: LaneEdit::Ramp {
                from: Some("0".to_string()),
                to: Some("2".to_string()),
            },
        })
        .expect("explicit ramp applies");
    let lanes = lane_stops(&session, 0, "volume");
    let fresh = lanes.last().cloned().unwrap();
    assert_eq!(fresh, vec![(0.0, 0.0), (2.0, 2.0)]);
    // The old key kept its outside stops plus the edge rescue.
    assert!(lanes.iter().any(|k| k.contains(&(3.0, 0.8))), "{lanes:?}");
}

#[test]
fn swell_and_dip_shape_the_window() {
    let dir = tmp("swell-dip");
    lane_project(&dir);
    let mut session = open(&dir);
    session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "volume".to_string(),
            start: 0.0,
            end: 2.0,
            edit: LaneEdit::Swell {
                peak: "1.5".to_string(),
            },
        })
        .expect("swell applies");
    let lanes = lane_stops(&session, 0, "volume");
    let fresh = lanes.last().cloned().unwrap();
    assert_eq!(fresh.len(), 3);
    assert_eq!((fresh[1].0, fresh[1].1), (1.0, 1.5));
    assert_eq!((fresh[0].0, fresh[0].1), (0.0, 1.0));
    assert!((fresh[2].1 - 0.6).abs() < 1e-12, "{fresh:?}");

    let dir = tmp("dip");
    lane_project(&dir);
    let mut session = open(&dir);
    session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("voice"),
            property: "pan".to_string(),
            start: 0.0,
            end: 4.0,
            edit: LaneEdit::Dip {
                trough: "0".to_string(),
            },
        })
        .expect("dip applies");
    let lanes = lane_stops(&session, 1, "pan");
    let fresh = lanes.last().cloned().unwrap();
    assert_eq!(fresh, vec![(0.0, -1.0), (2.0, 0.0), (4.0, 1.0)]);
}

#[test]
fn simplify_drops_predicted_stops_and_refuses_straight() {
    let dir = tmp("simplify");
    lane_project(&dir);
    let mut session = open(&dir);
    // Stops (0,1),(1,0.8),(2,0.6),(3,0.8): the straight 0..3 line
    // predicts stop 1 within 0.25, but stop 2 deviates 0.267, so only
    // stop 1 drops.
    session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "volume".to_string(),
            start: 0.0,
            end: 3.0,
            edit: LaneEdit::Simplify { tolerance: 0.25 },
        })
        .expect("simplify applies");
    let lanes = lane_stops(&session, 0, "volume");
    let fresh = lanes.last().cloned().unwrap();
    assert_eq!(fresh, vec![(0.0, 1.0), (2.0, 0.6), (3.0, 0.8)], "{lanes:?}");

    // A lane with nothing to drop refuses instead of staging a
    // phantom patch: at tolerance 0 every music stop deviates past
    // straight, so simplify keeps them all.
    let dir = tmp("simplify-keep");
    lane_project(&dir);
    let mut session = open(&dir);
    let before = tree_bytes(&dir);
    let err = session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "volume".to_string(),
            start: 0.0,
            end: 3.0,
            edit: LaneEdit::Simplify { tolerance: 0.0 },
        })
        .expect_err("detailed lane refuses simplify");
    assert_eq!(err.kind, EditRefusalKind::BadValue);
    assert!(err.message.contains("keeps every stop"), "{err:?}");
    assert!(session.patches().is_empty());
    assert_eq!(tree_bytes(&dir), before);

    // Fewer than three stops refuses naming the count.
    let err = session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("voice"),
            property: "pan".to_string(),
            start: 0.0,
            end: 4.0,
            edit: LaneEdit::Simplify { tolerance: 0.5 },
        })
        .expect_err("two-stop lane refuses simplify");
    assert!(err.message.contains("at least three stops"), "{err:?}");
}

#[test]
fn copy_paste_round_trips_points_exactly() {
    let dir = tmp("copy-paste");
    lane_project(&dir);
    let session = open(&dir);
    let comp = session.effective_composition().clone();
    let copied = sdk::edits::copy_lane_range(&comp, &ClipRef::by_id("music"), "volume", 1.0, 3.0)
        .expect("copy reads stops");
    assert_eq!(copied.len(), 3);
    assert_eq!(
        copied.iter().map(|s| s.at).collect::<Vec<_>>(),
        vec![1.0, 2.0, 3.0]
    );

    // Paste the copied instants onto the pan lane with pan-range
    // values: the pasted key carries the same instants exactly.
    let mut session = open(&dir);
    session
        .apply(EditOp::PasteLane {
            target: ClipRef::by_id("voice"),
            property: "pan".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 1.0,
                    value: "-0.5".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 2.0,
                    value: "0".to_string(),
                    ease: Some("in-quad".to_string()),
                },
                KeyStopInput {
                    at: 3.0,
                    value: "0.5".to_string(),
                    ease: None,
                },
            ],
        })
        .expect("paste applies");
    let lanes = lane_stops(&session, 1, "pan");
    let fresh = lanes.last().cloned().unwrap();
    assert_eq!(fresh, vec![(1.0, -0.5), (2.0, 0.0), (3.0, 0.5)]);
    // The pasted easing rides along: stop 2 keeps `in-quad`.
    let key = session
        .effective_composition()
        .keys
        .iter()
        .find(|k| {
            k.target == 1
                && k.property == "pan"
                && k.stops.iter().any(|s| s.at == 2.0 && s.value == 0.0)
                && k.stops.len() == 3
        })
        .expect("pasted key found");
    assert_eq!(
        key.stops[1].ease.map(|e| e.name()),
        Some("in-quad".to_string())
    );
    // The old pan key kept its outside stop plus the edge rescue.
    assert!(lanes.iter().any(|k| k.contains(&(0.0, -1.0))), "{lanes:?}");
    // Undo reverses the whole paste at once.
    session.undo().expect("undo reverses the paste");
    assert_eq!(stops_of(&session, 1), vec![(0.0, -1.0), (4.0, 1.0)]);
}

#[test]
fn silent_targets_and_empty_ranges_refuse_by_name() {
    let dir = tmp("refusals");
    lane_project(&dir);
    let mut session = open(&dir);
    let before = tree_bytes(&dir);

    // A lane edit over silence (a text clip) refuses naming its kind.
    let err = session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("title"),
            property: "volume".to_string(),
            start: 0.0,
            end: 2.0,
            edit: LaneEdit::Ramp {
                from: None,
                to: None,
            },
        })
        .expect_err("text lane refuses");
    assert_eq!(err.kind, EditRefusalKind::Unsupported);
    assert_eq!(err.code, sdk::E_SDK_UNSUPPORTED);
    assert!(err.message.contains("text clip"), "{err:?}");
    assert!(err.message.contains("silence"), "{err:?}");

    // A window holding no stops reports the empty range.
    let err = session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "pan".to_string(),
            start: 0.0,
            end: 4.0,
            edit: LaneEdit::Ramp {
                from: None,
                to: None,
            },
        })
        .expect_err("keyless lane refuses");
    assert_eq!(err.kind, EditRefusalKind::BadValue);
    assert!(err.message.contains("the range is empty"), "{err:?}");
    assert!(err.message.contains("0..4"), "{err:?}");

    // Copying an empty range refuses the same way and stages nothing.
    let comp = session.effective_composition().clone();
    let err = sdk::edits::copy_lane_range(&comp, &ClipRef::by_id("music"), "pan", 0.0, 4.0)
        .expect_err("empty copy refuses");
    assert!(err.message.contains("the range is empty"), "{err:?}");

    // A video clip carries sound: its lane rewrites fine.
    session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("clip"),
            property: "volume".to_string(),
            start: 0.0,
            end: 4.0,
            edit: LaneEdit::Ramp {
                from: Some("1".to_string()),
                to: Some("0.5".to_string()),
            },
        })
        .expect("video lane rewrites");
    assert_eq!(
        lane_stops(&session, 2, "volume").last().cloned().unwrap(),
        vec![(0.0, 1.0), (4.0, 0.5)]
    );

    // The two refusals staged nothing: the only patch is the video ramp.
    assert_eq!(session.patches().len(), 1);
    let fresh = open(&dir);
    assert!(fresh.patches().is_empty());
    assert_eq!(tree_bytes(&dir), before);
}

#[test]
fn malformed_lane_values_refuse_with_ranges() {
    let dir = tmp("bad-values");
    lane_project(&dir);
    let mut session = open(&dir);
    let before = tree_bytes(&dir);
    let music = ClipRef::by_id("music");

    // Volume past twice unity refuses naming the range.
    let err = session
        .apply(EditOp::RewriteLane {
            target: music.clone(),
            property: "volume".to_string(),
            start: 0.0,
            end: 1.0,
            edit: LaneEdit::Swell {
                peak: "3".to_string(),
            },
        })
        .expect_err("hot peak refuses");
    assert_eq!(err.code, sdk::E_SDK_BAD_VALUE);
    assert!(
        err.message.contains("0 (silence) to 2 (twice unity)"),
        "{err:?}"
    );

    // Pan past full right refuses naming the range (the voice lane
    // carries pan stops, so the value check runs).
    let err = session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("voice"),
            property: "pan".to_string(),
            start: 0.0,
            end: 1.0,
            edit: LaneEdit::Dip {
                trough: "-2".to_string(),
            },
        })
        .expect_err("wide trough refuses");
    assert!(err.message.contains("-1 (left) to 1 (right)"), "{err:?}");

    // A backward window refuses.
    let err = session
        .apply(EditOp::RewriteLane {
            target: music.clone(),
            property: "volume".to_string(),
            start: 2.0,
            end: 2.0,
            edit: LaneEdit::Ramp {
                from: None,
                to: None,
            },
        })
        .expect_err("backward window refuses");
    assert!(err.message.contains("2..2"), "{err:?}");

    // No lane property by that name exists.
    let err = session
        .apply(EditOp::RewriteLane {
            target: music.clone(),
            property: "gain".to_string(),
            start: 0.0,
            end: 1.0,
            edit: LaneEdit::Ramp {
                from: None,
                to: None,
            },
        })
        .expect_err("unknown lane refuses");
    assert!(err.message.contains("`volume` or `pan`"), "{err:?}");

    // A single-stop paste cannot stand as a key.
    let err = session
        .apply(EditOp::PasteLane {
            target: music.clone(),
            property: "volume".to_string(),
            stops: vec![KeyStopInput {
                at: 1.0,
                value: "1".to_string(),
                ease: None,
            }],
        })
        .expect_err("single-stop paste refuses");
    assert!(err.message.contains("at least two stops"), "{err:?}");

    // Unordered paste stops refuse.
    let err = session
        .apply(EditOp::PasteLane {
            target: music.clone(),
            property: "volume".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 2.0,
                    value: "1".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 1.0,
                    value: "1".to_string(),
                    ease: None,
                },
            ],
        })
        .expect_err("unordered paste refuses");
    assert!(err.message.contains("rising time order"), "{err:?}");

    // An unknown clip refuses.
    let err = session
        .apply(EditOp::PasteLane {
            target: ClipRef::by_id("ghost"),
            property: "volume".to_string(),
            stops: vec![
                KeyStopInput {
                    at: 1.0,
                    value: "1".to_string(),
                    ease: None,
                },
                KeyStopInput {
                    at: 2.0,
                    value: "1".to_string(),
                    ease: None,
                },
            ],
        })
        .expect_err("ghost target refuses");
    assert_eq!(err.code, sdk::E_SDK_UNKNOWN_TARGET);

    assert!(session.patches().is_empty());
    assert_eq!(tree_bytes(&dir), before);
}

#[test]
fn lane_ops_persist_and_replay_on_reopen() {
    let root = tmp("persist");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    lane_project(&folder);
    let mut session = open(&folder);
    session
        .apply(EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "volume".to_string(),
            start: 1.0,
            end: 3.0,
            edit: LaneEdit::Ramp {
                from: None,
                to: None,
            },
        })
        .expect("ramp applies");
    let report = session.persist().expect("persist writes the delta");
    assert_eq!(report.applied, 1);

    // Reopening replays the lane rewrite onto the validated
    // composition at the same points.
    let session = open(&folder);
    assert_eq!(
        lane_stops(&session, 0, "volume"),
        vec![vec![(0.0, 1.0), (1.0, 0.8)], vec![(1.0, 0.8), (3.0, 0.8)]]
    );
}

#[test]
fn apply_to_copy_previews_without_touching_base() {
    let dir = tmp("copy-apply");
    lane_project(&dir);
    let session = open(&dir);
    let base = session.effective_composition().clone();
    let modified = sdk::edits::apply_ops_to_composition(
        &base,
        &[EditOp::RewriteLane {
            target: ClipRef::by_id("music"),
            property: "volume".to_string(),
            start: 1.0,
            end: 3.0,
            edit: LaneEdit::Ramp {
                from: Some("0".to_string()),
                to: Some("0".to_string()),
            },
        }],
    )
    .expect("copy applies");
    // The copy carries the fresh key; the base keeps its truth.
    assert!(modified
        .keys
        .iter()
        .any(|k| k.target == 0 && k.property == "volume" && k.stops.len() == 2));
    assert_eq!(base.keys.len(), modified.keys.len() - 1);
    // A refusing op returns the error with no copy at all.
    let err = sdk::edits::apply_ops_to_composition(
        &base,
        &[EditOp::RewriteLane {
            target: ClipRef::by_id("title"),
            property: "volume".to_string(),
            start: 0.0,
            end: 1.0,
            edit: LaneEdit::Ramp {
                from: None,
                to: None,
            },
        }],
    )
    .expect_err("silent copy refuses");
    assert!(err.message.contains("silence"), "{err:?}");
}
