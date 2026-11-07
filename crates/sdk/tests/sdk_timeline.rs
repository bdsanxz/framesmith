//! Timeline verb regressions (Sprint 15, T007 / CAP-006): every verb
//! rewrites the staged composition with a visible result, each time
//! expression form resolves to its film position, rows expose
//! `mediaStart`, plan mode prints only a count while changing nothing,
//! apply prints an undo receipt, verb undo restores the exact prior
//! sources, ranged delete removes only its range with snap, and
//! unresolvable expressions or incapable verbs refuse with a reason
//! code leaving no receipt.

use sdk::{
    ClipRef, EditRefusalKind, OpenOptions, SdkSession, SetProperty, TimeExpr, TimelineVerb,
    E_SDK_BAD_VALUE, E_SDK_NOTHING_TO_UNDO, E_SDK_UNKNOWN_TARGET, E_SDK_UNSUPPORTED,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-timeline-{}-{}-{}",
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
        "composition name=\"t\" width=320 height=180 duration=6 fps=10 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\" font-family=\"Alpha, Beta\"\n    text id=\"b\" value=\"Two\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n    text id=\"c\" value=\"Three\" start=4 duration=2 x=10 y=120 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn footage_project(folder: &Path) {
    std::fs::create_dir_all(folder.join("media")).unwrap();
    std::fs::write(folder.join("media/clip.mp4"), b"not-a-real-video").unwrap();
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=6 fps=10 background=\"#112233\" version=1 {\n    video id=\"v\" path=\"media/clip.mp4\" start=0 duration=4 trim-start=1 rate=1\n    text id=\"tail\" value=\"Tail\" start=4 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn flex_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=480 duration=2 fps=10 background=\"#000000\" version=1 {\n    flex id=\"row\" display=\"flex\" flex-direction=\"row\" gap=10 x=10 y=20 width=340 height=100 start=0 duration=2 {\n        box width=100 height=50 fill=\"#ff0000\"\n    }\n}\n",
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
/// refused-edit and no-write oracle.
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
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
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

fn windows(session: &SdkSession) -> Vec<(f64, f64)> {
    session
        .timeline_rows()
        .iter()
        .map(|r| (r.start, r.end))
        .collect()
}

fn id_a() -> ClipRef {
    ClipRef::by_id("a")
}

#[test]
fn each_verb_rewrites_sources_with_visible_result() {
    let root = tmp("verbs");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    // move shifts one clip in film time.
    let mut session = open(&folder);
    let receipt = session
        .timeline_apply(TimelineVerb::Move {
            target: ClipRef::by_id("b"),
            to: TimeExpr::Offset(1.0),
        })
        .expect("move applies");
    assert_eq!(receipt.applied, 1);
    assert_eq!(windows(&session)[1], (3.0, 5.0));

    // trim rewrites one window.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Trim {
            target: id_a(),
            start: Some(TimeExpr::Offset(0.5)),
            end: None,
        })
        .expect("trim applies");
    assert_eq!(windows(&session)[0], (0.5, 2.0));

    // split divides one clip into two touching halves.
    let mut session = open(&folder);
    let receipt = session
        .timeline_apply(TimelineVerb::Split {
            target: id_a(),
            at: TimeExpr::Offset(1.0),
        })
        .expect("split applies");
    assert_eq!(receipt.applied, 1);
    assert_eq!(windows(&session).len(), 4);
    assert_eq!(windows(&session)[0], (0.0, 1.0));
    assert_eq!(windows(&session)[1], (1.0, 2.0));

    // delete removes the whole target clip.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Delete {
            target: Some(ClipRef::by_id("b")),
            start: None,
            end: None,
            snap: None,
        })
        .expect("delete applies");
    assert_eq!(session.timeline_rows().len(), 2);
    assert_eq!(windows(&session), vec![(0.0, 2.0), (4.0, 6.0)]);

    // set writes one property.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Set {
            target: id_a(),
            property: SetProperty::Hidden,
            value: "true".to_string(),
        })
        .expect("set applies");
    assert!(session.timeline_rows()[0].hidden);

    // duplicate clones the clip right after itself.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Duplicate {
            target: id_a(),
            at: None,
            new_id: None,
        })
        .expect("duplicate applies");
    assert_eq!(windows(&session).len(), 4);
    assert_eq!(windows(&session)[1], (2.0, 4.0));
    assert_eq!(session.timeline_rows()[1].id, None);

    // ids lists every identifier and mutates nothing.
    let mut session = open(&folder);
    let before = tree_bytes(&folder);
    let receipt = session
        .timeline_apply(TimelineVerb::Ids)
        .expect("ids applies");
    assert_eq!(receipt.applied, 0);
    assert_eq!(receipt.listing.len(), 3);
    assert!(receipt.listing[0].contains("\"a\""));
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn each_time_expression_form_resolves_to_its_film_position() {
    let root = tmp("times");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    // +N reads N seconds after the target clip's start (b starts at 2).
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Move {
            target: ClipRef::by_id("b"),
            to: TimeExpr::Offset(1.0),
        })
        .expect("offset move applies");
    assert_eq!(windows(&session)[1], (3.0, 5.0));

    // Nf reads film frame N at the 10fps composition rate (20f = 2s).
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Move {
            target: id_a(),
            to: sdk::parse_time_expr("20f").expect("frames parse"),
        })
        .expect("frame move applies");
    assert_eq!(windows(&session)[0], (2.0, 4.0));

    // end reads the 6s composition end.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Move {
            target: ClipRef::by_id("c"),
            to: sdk::parse_time_expr("end").expect("end parses"),
        })
        .expect("end move applies");
    assert_eq!(windows(&session)[2], (6.0, 8.0));

    // after:<id> reads the referenced clip's end (b ends at 4).
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Move {
            target: id_a(),
            to: sdk::parse_time_expr("after:b").expect("after parses"),
        })
        .expect("after-id move applies");
    assert_eq!(windows(&session)[0], (4.0, 6.0));

    // after:<index> reads the document index's end (index 1 ends at 4).
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Move {
            target: ClipRef::by_id("c"),
            to: sdk::parse_time_expr("after:1").expect("after-index parses"),
        })
        .expect("after-index move applies");
    assert_eq!(windows(&session)[2], (4.0, 6.0));
}

#[test]
fn rows_expose_media_start_mapping_file_time_to_film_time() {
    let root = tmp("rows");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    footage_project(&folder);
    let session = open(&folder);

    let rows = session.timeline_rows();
    assert_eq!(rows.len(), 2);
    // The footage row maps file seconds: trim-start 1 opens the window.
    assert_eq!(rows[0].kind, "video");
    assert_eq!(rows[0].media_start, 1.0);
    assert_eq!((rows[0].start, rows[0].end), (0.0, 4.0));
    // Plain clips carry no file window.
    assert_eq!(rows[1].kind, "text");
    assert_eq!(rows[1].media_start, 0.0);

    let human = session.timeline_rows_human();
    assert!(human.contains("mediaStart 1.000s"), "{human}");
    assert!(human.contains("mediaStart 0.000s"), "{human}");
    assert!(human.contains("[0] video"), "{human}");
}

#[test]
fn plan_mode_prints_only_a_count_and_changes_nothing() {
    let root = tmp("plan");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let session = open(&folder);
    let before_tree = tree_bytes(&folder);
    let before_rows = session.timeline_rows();

    let plan = session
        .timeline_plan(&TimelineVerb::Move {
            target: ClipRef::by_id("b"),
            to: TimeExpr::Offset(1.0),
        })
        .expect("move plans");
    assert_eq!(plan.affected, 1);
    assert_eq!(plan.to_string(), "timeline plan: 1 clip(s) would change");
    assert_eq!(plan.detail, vec!["move clip id \"b\" to 3.000s"]);

    // Split touches two resulting halves.
    let plan = session
        .timeline_plan(&TimelineVerb::Split {
            target: id_a(),
            at: TimeExpr::Offset(1.0),
        })
        .expect("split plans");
    assert_eq!(plan.to_string(), "timeline plan: 2 clip(s) would change");

    // Ranged delete counts every clip it would rewrite.
    let plan = session
        .timeline_plan(&TimelineVerb::Delete {
            target: None,
            start: Some(TimeExpr::Offset(1.0)),
            end: Some(TimeExpr::Offset(3.0)),
            snap: None,
        })
        .expect("range delete plans");
    assert_eq!(plan.affected, 2, "{:?}", plan.detail);

    // Nothing staged, nothing written.
    assert_eq!(session.timeline_rows(), before_rows);
    assert_eq!(session.undo_listing().depth, 0);
    assert_eq!(tree_bytes(&folder), before_tree);
    assert!(!has_render_output(&folder));
}

#[test]
fn apply_prints_an_undo_receipt_and_verb_undo_restores_exact_sources() {
    let root = tmp("receipt");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    footage_project(&folder);
    let mut session = open(&folder);
    let before = session.timeline_rows();

    let receipt = session
        .timeline_apply(TimelineVerb::Move {
            target: ClipRef::by_id("v"),
            to: TimeExpr::Frames(40),
        })
        .expect("move applies");
    assert_eq!(receipt.applied, 1);
    assert!(receipt.undo_available);
    let printed = receipt.to_string();
    assert!(printed.contains("undo receipt"), "{printed}");
    assert!(
        printed.contains(&format!("{:?}", receipt.seqs)),
        "{printed}"
    );
    // Moving in film time keeps the file origin: same frames, new time.
    let moved = session.timeline_rows();
    assert_eq!((moved[0].start, moved[0].end), (4.0, 8.0));
    assert_eq!(moved[0].media_start, 1.0);

    let undo = session.timeline_undo().expect("verb undo applies");
    assert_eq!(undo.undone, 1);
    let printed = undo.to_string();
    assert!(printed.contains("restored 1 patch(es)"), "{printed}");
    assert_eq!(session.timeline_rows(), before);

    // A second undo names no pending verb application.
    let refused = session.timeline_undo().expect_err("double undo refuses");
    assert_eq!(refused.code, E_SDK_NOTHING_TO_UNDO);
    assert_eq!(refused.kind, EditRefusalKind::NothingToUndo);
    assert_eq!(session.timeline_rows(), before);
}

#[test]
fn ranged_delete_removes_only_its_range_with_snap_covering_delete() {
    let root = tmp("ranged");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    // Range [1,5): head of a shortens, b drops whole, tail of c shortens.
    let mut session = open(&folder);
    let receipt = session
        .timeline_apply(TimelineVerb::Delete {
            target: None,
            start: Some(TimeExpr::Offset(1.0)),
            end: Some(TimeExpr::Offset(5.0)),
            snap: None,
        })
        .expect("ranged delete applies");
    assert_eq!(receipt.applied, 3);
    assert_eq!(
        windows(&session),
        vec![(0.0, 1.0), (5.0, 6.0)],
        "{}",
        receipt.verb_summary
    );

    // Strictly-inside ranges split one clip into two survivors.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Delete {
            target: Some(id_a()),
            start: Some(TimeExpr::Offset(0.5)),
            end: Some(TimeExpr::Offset(1.5)),
            snap: None,
        })
        .expect("inside delete applies");
    assert_eq!(windows(&session).len(), 4);
    assert_eq!(windows(&session)[0], (0.0, 0.5));
    assert_eq!(windows(&session)[1], (1.5, 2.0));

    // Snap pulls a near-miss edge onto the clip edge: [0.05,2] with a
    // 0.1 tolerance covers the whole [0,2) clip, removing it outright.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Delete {
            target: Some(id_a()),
            start: Some(TimeExpr::Offset(0.05)),
            end: Some(TimeExpr::Offset(2.0)),
            snap: Some(0.1),
        })
        .expect("snapped delete applies");
    assert_eq!(session.timeline_rows().len(), 2);

    // Without the snap the same range only shortens the head.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Delete {
            target: Some(id_a()),
            start: Some(TimeExpr::Offset(0.05)),
            end: Some(TimeExpr::Offset(2.0)),
            snap: None,
        })
        .expect("unsnapped delete applies");
    assert_eq!(windows(&session)[0], (0.0, 0.05));

    // A range touching nothing stages nothing but still reports.
    let mut session = open(&folder);
    let receipt = session
        .timeline_apply(TimelineVerb::Delete {
            target: None,
            start: Some(TimeExpr::Offset(20.0)),
            end: Some(TimeExpr::Offset(21.0)),
            snap: None,
        })
        .expect("empty delete applies");
    assert_eq!(receipt.applied, 0);
    assert_eq!(windows(&session).len(), 3);
    let refused = session
        .timeline_undo()
        .expect_err("undo of empty run refuses");
    assert_eq!(refused.code, E_SDK_NOTHING_TO_UNDO);
}

#[test]
fn trim_and_split_keep_footage_showing_the_same_frames() {
    let root = tmp("footage");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    footage_project(&folder);

    // Trimming the head advances the file origin past trimmed footage.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Trim {
            target: ClipRef::by_id("v"),
            start: Some(TimeExpr::Offset(2.0)),
            end: None,
        })
        .expect("trim applies");
    let rows = session.timeline_rows();
    assert_eq!((rows[0].start, rows[0].end), (2.0, 4.0));
    assert_eq!(rows[0].media_start, 3.0);

    // Splitting divides the file window so both halves match the past.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Split {
            target: ClipRef::by_id("v"),
            at: TimeExpr::Offset(1.0),
        })
        .expect("split applies");
    let rows = session.timeline_rows();
    assert_eq!(rows.len(), 3);
    assert_eq!((rows[0].start, rows[0].end), (0.0, 1.0));
    assert_eq!((rows[1].start, rows[1].end), (1.0, 4.0));
    assert_eq!(rows[0].media_start, 1.0);
    assert_eq!(rows[1].media_start, 2.0);
    // The first half anchors the stable id; the copy drops it.
    assert_eq!(rows[0].id.as_deref(), Some("v"));
    assert_eq!(rows[1].id, None);

    // Undo restores the exact prior window and file origin.
    let before = {
        let fresh = open(&folder);
        fresh.timeline_rows()
    };
    session.timeline_undo().expect("verb undo applies");
    assert_eq!(session.timeline_rows(), before);
}

#[test]
fn duplicate_drops_colliding_ids_unless_a_fresh_one_is_named() {
    let root = tmp("dup");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    // Without a new id the copy drops the id so ids stay unique.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Duplicate {
            target: id_a(),
            at: None,
            new_id: None,
        })
        .expect("duplicate applies");
    let rows = session.timeline_rows();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].id.as_deref(), Some("a"));
    assert_eq!(rows[1].id, None);

    // A fresh id is honored on the copy.
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Duplicate {
            target: id_a(),
            at: Some(TimeExpr::Frames(30)),
            new_id: Some("a2".to_string()),
        })
        .expect("named duplicate applies");
    let rows = session.timeline_rows();
    assert_eq!(rows[1].id.as_deref(), Some("a2"));
    assert_eq!((rows[1].start, rows[1].end), (3.0, 5.0));

    // A taken id refuses before anything stages.
    let mut session = open(&folder);
    let before = tree_bytes(&folder);
    let refused = session
        .timeline_apply(TimelineVerb::Duplicate {
            target: id_a(),
            at: None,
            new_id: Some("b".to_string()),
        })
        .expect_err("colliding duplicate refuses");
    assert_eq!(refused.code, E_SDK_BAD_VALUE);
    assert_eq!(session.undo_listing().depth, 0);
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn verb_applies_stage_in_memory_without_project_writes() {
    let root = tmp("nowrite");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    let before = tree_bytes(&folder);

    for verb in [
        TimelineVerb::Move {
            target: id_a(),
            to: TimeExpr::Offset(1.0),
        },
        TimelineVerb::Split {
            target: ClipRef::by_id("b"),
            at: TimeExpr::Offset(1.0),
        },
        TimelineVerb::Set {
            target: ClipRef::by_id("c"),
            property: SetProperty::Label,
            value: "note".to_string(),
        },
    ] {
        session.timeline_apply(verb).expect("verb applies");
    }
    // Staged patches, byte-identical tree, no render output anywhere.
    assert_eq!(session.undo_listing().depth, 3);
    assert_eq!(tree_bytes(&folder), before);
    assert!(!has_render_output(&folder));
    // The composition sources on disk never move: only .framesmith
    // history would ever appear, and this session disables it.
    assert!(!folder.join(".framesmith").exists());
}

#[test]
fn verb_patches_persist_and_replay_with_positions_kept() {
    let root = tmp("persist");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    session
        .timeline_apply(TimelineVerb::Move {
            target: ClipRef::by_id("b"),
            to: TimeExpr::Offset(2.0),
        })
        .expect("move applies");
    let staged = session.timeline_rows();
    let report = session.persist().expect("persist writes the delta");
    assert_eq!(report.applied, 1);

    // Composition sources stay byte-identical; only the hidden delta
    // records the turn.
    let raw = std::fs::read_to_string(folder.join("composition.kdl")).unwrap();
    assert!(
        raw.contains("text id=\"b\" value=\"Two\" start=2 duration=2"),
        "{raw}"
    );

    let reopened = open(&folder);
    assert_eq!(reopened.timeline_rows(), staged);
    assert_eq!(reopened.undo_listing().depth, 1);
    assert!(!has_render_output(&folder));
}

#[test]
fn unresolvable_expressions_and_incapable_verbs_refuse_with_no_receipt() {
    let root = tmp("refuse");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let flex_folder = root.join("flexproj");
    std::fs::create_dir_all(&flex_folder).unwrap();
    flex_project(&flex_folder);

    // A form outside the four time expressions refuses.
    assert!(sdk::parse_time_expr("tomorrow").is_err());
    assert!(sdk::parse_time_expr("").is_err());
    assert!(sdk::parse_time_expr("+").is_err());
    assert!(sdk::parse_time_expr("after:").is_err());
    assert!(sdk::parse_time_expr("1.5x").is_err());
    let bad = sdk::parse_time_expr("tomorrow").expect_err("bad form refuses");
    assert_eq!(bad.code, E_SDK_BAD_VALUE);

    let mut session = open(&folder);
    let before_tree = tree_bytes(&folder);
    let before_rows = session.timeline_rows();

    // after:<ref> naming nothing refuses with the unknown-target code.
    let refused = session
        .timeline_apply(TimelineVerb::Move {
            target: id_a(),
            to: sdk::parse_time_expr("after:ghost").expect("parses"),
        })
        .expect_err("dangling after refuses");
    assert_eq!(refused.code, E_SDK_UNKNOWN_TARGET);
    assert_eq!(refused.kind, EditRefusalKind::UnknownTarget);

    // A split outside the clip window refuses.
    let refused = session
        .timeline_apply(TimelineVerb::Split {
            target: id_a(),
            at: TimeExpr::Offset(9.0),
        })
        .expect_err("outside split refuses");
    assert_eq!(refused.code, E_SDK_BAD_VALUE);

    // A trim naming no edge refuses.
    let refused = session
        .timeline_apply(TimelineVerb::Trim {
            target: id_a(),
            start: None,
            end: None,
        })
        .expect_err("edgeless trim refuses");
    assert_eq!(refused.code, E_SDK_BAD_VALUE);

    // A delete naming neither target nor range refuses.
    let refused = session
        .timeline_apply(TimelineVerb::Delete {
            target: None,
            start: None,
            end: None,
            snap: None,
        })
        .expect_err("targetless delete refuses");
    assert_eq!(refused.code, E_SDK_BAD_VALUE);

    // A negative snap tolerance refuses.
    let refused = session
        .timeline_apply(TimelineVerb::Delete {
            target: Some(id_a()),
            start: Some(TimeExpr::Offset(0.5)),
            end: Some(TimeExpr::Offset(1.5)),
            snap: Some(-0.5),
        })
        .expect_err("negative snap refuses");
    assert_eq!(refused.code, E_SDK_BAD_VALUE);

    // An unknown set property refuses as outside capability before any
    // verb is even built.
    let bad_prop = sdk::parse_set_property("opacity").expect_err("opacity is no verb property");
    assert_eq!(bad_prop.code, E_SDK_UNSUPPORTED);
    assert_eq!(bad_prop.kind, EditRefusalKind::Unsupported);

    // A bad hidden value refuses.
    let refused = session
        .timeline_apply(TimelineVerb::Set {
            target: id_a(),
            property: SetProperty::Hidden,
            value: "maybe".to_string(),
        })
        .expect_err("bad hidden refuses");
    assert_eq!(refused.code, E_SDK_BAD_VALUE);

    // Splitting a flex container refuses as outside capability.
    let mut flex_session = open(&flex_folder);
    let refused = flex_session
        .timeline_apply(TimelineVerb::Split {
            target: ClipRef::by_id("row"),
            at: TimeExpr::Offset(1.0),
        })
        .expect_err("flex split refuses");
    assert_eq!(refused.code, E_SDK_UNSUPPORTED);
    assert_eq!(refused.kind, EditRefusalKind::Unsupported);

    // Nothing staged anywhere, nothing written, no receipt left behind.
    assert_eq!(session.timeline_rows(), before_rows);
    assert_eq!(session.undo_listing().depth, 0);
    assert_eq!(tree_bytes(&folder), before_tree);
    assert!(!has_render_output(&folder));
}

#[test]
fn set_covers_label_and_film_window_with_exact_undo() {
    let root = tmp("set");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    let before = session.timeline_rows();

    session
        .timeline_apply(TimelineVerb::Set {
            target: id_a(),
            property: SetProperty::Label,
            value: "cover".to_string(),
        })
        .expect("label set applies");
    assert_eq!(session.timeline_rows()[0].label.as_deref(), Some("cover"));

    session
        .timeline_apply(TimelineVerb::Set {
            target: id_a(),
            property: SetProperty::Start,
            value: "1".to_string(),
        })
        .expect("start set applies");
    assert_eq!(windows(&session)[0], (1.0, 3.0));

    session
        .timeline_apply(TimelineVerb::Set {
            target: id_a(),
            property: SetProperty::Duration,
            value: "0.5".to_string(),
        })
        .expect("duration set applies");
    assert_eq!(windows(&session)[0], (1.0, 1.5));

    // Undo reverses the last verb only: duration restores, label stays.
    session.timeline_undo().expect("verb undo applies");
    assert_eq!(windows(&session)[0], (1.0, 3.0));
    assert_eq!(session.timeline_rows()[0].label.as_deref(), Some("cover"));
    let _ = before;
}

#[test]
fn font_family_try_order_passes_through_verbs_untouched() {
    let root = tmp("fonts");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);

    fn families(session: &SdkSession) -> Vec<String> {
        session
            .effective_composition()
            .clips
            .iter()
            .find_map(|c| match c {
                notation::model::Clip::Text(t) => t.font_family.clone(),
                _ => None,
            })
            .expect("a text clip carries font-family")
    }
    assert_eq!(
        families(&session),
        vec!["Alpha".to_string(), "Beta".to_string()]
    );

    session
        .timeline_apply(TimelineVerb::Move {
            target: id_a(),
            to: TimeExpr::Offset(1.0),
        })
        .expect("move applies");
    assert_eq!(
        families(&session),
        vec!["Alpha".to_string(), "Beta".to_string()]
    );
    session.timeline_undo().expect("verb undo applies");
    assert_eq!(
        families(&session),
        vec!["Alpha".to_string(), "Beta".to_string()]
    );
}
