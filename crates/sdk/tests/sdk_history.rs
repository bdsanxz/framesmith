//! Shared guarded history regressions (Sprint 15, T008 / CAP-007,
//! CAP-008, CAP-009): named turns with edit targets and artifact
//! cleanup, list/show/peek/pin/begin/end with `show --diff`, whole-turn
//! undo from backups, named-actor undo, exclusive/who/if-open guards
//! naming the holder, listing when unwritable, and missing
//! target/backup refusals.

use sdk::{
    BeginOptions, EditOp, NewElement, OpenOptions, SdkSession, E_SDK_BAD_VALUE, E_SDK_GUARD_HELD,
    E_SDK_HISTORY_UNAVAILABLE, E_SDK_HISTORY_UNWRITABLE, E_SDK_MISSING_BACKUP,
    E_SDK_MISSING_TARGET, E_SDK_NO_TURN, E_SDK_NO_TURN_IN_EFFECT, E_SDK_PREVIEW, E_SDK_READ_ONLY,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-history-{}-{}-{}",
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
    SdkSession::open(folder, OpenOptions::default()).expect("valid open")
}

fn add_text(session: &mut SdkSession, value: &str) {
    session
        .apply(EditOp::AddElement {
            element: NewElement::Text {
                id: None,
                label: None,
                value: value.to_string(),
                start: 0.0,
                duration: 1.0,
                x: 10.0,
                y: 40.0,
                size: 24.0,
                color: "#ffffff".to_string(),
                font_family: None,
            },
            position: None,
        })
        .expect("apply");
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

fn history_files(tree: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    // Separator-agnostic: Windows tree keys carry backslashes.
    tree.keys()
        .map(|k| k.replace('\\', "/"))
        .filter(|k| k.starts_with(".framesmith/"))
        .collect()
}

#[test]
fn persist_turn_records_actor_targets_and_cleanup() {
    let root = tmp("record");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before_kdl = std::fs::read(folder.join("composition.kdl")).unwrap();
    // A stray artifact the turn must clean and name.
    std::fs::create_dir_all(folder.join(".framesmith")).unwrap();
    write(&folder, ".framesmith/stale-render.tmp", "junk");

    let mut session = open(&folder);
    add_text(&mut session, "Two");
    let receipt = session.persist_turn("ana").expect("persist turn");
    assert_eq!(receipt.applied, 1);
    assert_eq!(receipt.turn, Some(0));
    assert_eq!(receipt.actor.as_deref(), Some("ana"));

    let listing = sdk::history::list_turns(&folder).expect("list");
    assert_eq!(listing.turns.len(), 1);
    let turn = &listing.turns[0];
    assert_eq!(turn.actor, "ana");
    assert!(turn.summary.contains("ana"), "{}", turn.summary);
    assert!(!turn.undone);

    let detail = sdk::history::show_turn(&folder, 0).expect("show");
    assert_eq!(detail.record.actor, "ana");
    assert!(
        detail.record.targets.iter().any(|t| t.contains("add text")),
        "{:?}",
        detail.record.targets
    );
    assert_eq!(
        detail.record.artifacts_cleaned,
        vec![".framesmith/stale-render.tmp".to_string()]
    );
    assert!(!folder.join(".framesmith/stale-render.tmp").exists());
    // `show --diff` names the changed delta file.
    assert!(
        detail
            .diff
            .iter()
            .any(|f| f.ends_with("sdk-overrides.json")),
        "{:?}",
        detail.diff
    );
    // Composition sources stay byte-identical; everything new lives
    // under the hidden history directory; no render output appears.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        before_kdl
    );
    assert!(!history_files(&tree_bytes(&folder)).is_empty());
    for key in tree_bytes(&folder).keys() {
        assert!(
            !key.ends_with(".mp4") && !key.ends_with(".png"),
            "render output leaked: {key}"
        );
    }
    assert!(!has_render_output(&folder));
}

#[test]
fn interleaved_human_and_agent_turns_list_in_order() {
    let root = tmp("interleave");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    let mut human = open(&folder);
    add_text(&mut human, "Human");
    human.persist_turn("maker").expect("human turn");
    drop(human);
    // Reopen replays the delta, then the agent appends its own turn.
    let mut agent = open(&folder);
    assert_eq!(agent.patches().len(), 1);
    add_text(&mut agent, "Agent");
    agent.persist_turn("agent").expect("agent turn");

    let listing = sdk::history::list_turns(&folder).expect("list");
    assert_eq!(listing.turns.len(), 2);
    assert_eq!(listing.turns[0].actor, "maker");
    assert_eq!(listing.turns[1].actor, "agent");
    assert_eq!(listing.turns[0].seq, 0);
    assert_eq!(listing.turns[1].seq, 1);
    assert!(listing.writes_available);
    assert_eq!(listing.write_cause, None);

    // Peek sees the newest turn without consuming anything.
    let peek = sdk::history::peek_turn(&folder)
        .expect("peek")
        .expect("a turn");
    assert_eq!(peek.seq, 1);
    assert_eq!(peek.actor, "agent");
    assert_eq!(sdk::history::list_turns(&folder).unwrap().turns.len(), 2);

    // Pin and unpin round-trip on the named turn.
    let pinned = sdk::history::pin_turn(&folder, 0, true).expect("pin");
    assert!(pinned.pinned);
    assert!(sdk::history::show_turn(&folder, 0).unwrap().record.pinned);
    let unpinned = sdk::history::pin_turn(&folder, 0, false).expect("unpin");
    assert!(!unpinned.pinned);
}

#[test]
fn undo_turn_reverses_whole_turn_from_backup() {
    let root = tmp("undo");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    let mut session = open(&folder);
    add_text(&mut session, "A");
    session.persist_turn("ana").expect("turn 0");
    drop(session);
    let mut session = open(&folder);
    add_text(&mut session, "B");
    session.persist_turn("ana").expect("turn 1");
    drop(session);
    assert_eq!(open(&folder).patches().len(), 2);

    let receipt = sdk::history::undo_turn(&folder, 1).expect("undo turn 1");
    assert_eq!(receipt.seq, 1);
    assert_eq!(receipt.actor, "ana");
    assert!(
        receipt.restored_state.contains("backup"),
        "{}",
        receipt.restored_state
    );
    assert!(
        receipt
            .restored_files
            .iter()
            .any(|f| f.ends_with("sdk-overrides.json")),
        "{:?}",
        receipt.restored_files
    );
    // The tree matches turn 1's backup: reopening replays one op.
    assert_eq!(open(&folder).patches().len(), 1);
    assert!(sdk::history::show_turn(&folder, 1).unwrap().record.undone);

    let receipt = sdk::history::undo_turn(&folder, 0).expect("undo turn 0");
    assert_eq!(receipt.seq, 0);
    assert_eq!(open(&folder).patches().len(), 0);
    // Other turns stay intact and listed.
    assert_eq!(sdk::history::list_turns(&folder).unwrap().turns.len(), 2);
}

#[test]
fn undo_who_reverses_last_turn_still_in_effect() {
    let root = tmp("who");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    for (actor, value) in [("ana", "A"), ("bob", "B"), ("ana", "C")] {
        let mut session = open(&folder);
        add_text(&mut session, value);
        session.persist_turn(actor).expect("turn");
    }

    let receipt = sdk::history::undo_who(&folder, "ana").expect("undo-who ana");
    assert_eq!(receipt.seq, 2);
    assert_eq!(open(&folder).patches().len(), 2);
    // Bob's turn is still in effect and undoes from its own backup.
    let receipt = sdk::history::undo_who(&folder, "bob").expect("undo-who bob");
    assert_eq!(receipt.seq, 1);
    assert_eq!(open(&folder).patches().len(), 1);
    // Turn 2 is spent, so ana's next undo names the older turn, whose
    // backup predates every edit.
    let receipt = sdk::history::undo_who(&folder, "ana").expect("undo-who ana again");
    assert_eq!(receipt.seq, 0);
    assert_eq!(open(&folder).patches().len(), 0);

    // Nobody by that name: the refusal reports it and changes nothing.
    let tree = tree_bytes(&folder);
    let err = sdk::history::undo_who(&folder, "ghost").expect_err("no turn in effect");
    assert_eq!(err.code, E_SDK_NO_TURN_IN_EFFECT);
    assert!(err.message.contains("ghost"), "{}", err.message);
    assert!(
        err.message.contains("no turn still in effect"),
        "{}",
        err.message
    );
    assert_eq!(tree_bytes(&folder), tree);
}

#[test]
fn exclusive_guard_refuses_naming_holder_and_preserves_work() {
    let root = tmp("guard");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    let _holder = sdk::history::begin_turn(
        &folder,
        "bob",
        &BeginOptions {
            exclusive: true,
            if_open: false,
        },
    )
    .expect("begin exclusive");

    let mut session = open(&folder);
    add_text(&mut session, "Blocked");
    let tree = tree_bytes(&folder);
    let err = session
        .persist_turn("ana")
        .expect_err("guarded persist refuses");
    assert_eq!(err.code, E_SDK_GUARD_HELD);
    assert!(err.message.contains("bob"), "{}", err.message);
    assert!(err.message.contains("open for"), "{}", err.message);
    // The refused writer's work is preserved unapplied, never discarded.
    assert_eq!(tree_bytes(&folder), tree);
    assert_eq!(session.preview_edits().staged, 1);

    // After the holder ends the turn, the same staged work persists.
    sdk::history::end_turn(&folder, "bob").expect("end");
    let receipt = session.persist_turn("ana").expect("persist after end");
    assert_eq!(receipt.turn, Some(0));
    assert_eq!(open(&folder).patches().len(), 1);
}

#[test]
fn shared_begins_coexist_and_if_open_attaches() {
    let root = tmp("shared");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let _ = open(&folder);

    let ana = sdk::history::begin_turn(
        &folder,
        "ana",
        &BeginOptions {
            exclusive: false,
            if_open: false,
        },
    )
    .expect("ana shared begin");
    assert!(!ana.attached);
    // A second shared writer coexists.
    let bob = sdk::history::begin_turn(
        &folder,
        "bob",
        &BeginOptions {
            exclusive: false,
            if_open: false,
        },
    )
    .expect("bob shared begin");
    assert!(!bob.attached);
    // An exclusive request conflicts while shared turns are open,
    // naming the holder.
    let err = sdk::history::begin_turn(
        &folder,
        "cara",
        &BeginOptions {
            exclusive: true,
            if_open: false,
        },
    )
    .expect_err("exclusive conflicts");
    assert_eq!(err.code, E_SDK_GUARD_HELD);
    assert!(err.message.contains("ana"), "{}", err.message);
    // `--if-open` attaches to the already-open turn instead.
    let attached = sdk::history::begin_turn(
        &folder,
        "cara",
        &BeginOptions {
            exclusive: false,
            if_open: true,
        },
    )
    .expect("attach");
    assert!(attached.attached);
    assert_eq!(attached.actor, "ana");
    // Ending a name with no open turn reports that and changes nothing.
    let err = sdk::history::end_turn(&folder, "ghost").expect_err("no open turn");
    assert_eq!(err.code, E_SDK_NO_TURN);
    assert!(err.message.contains("ghost"), "{}", err.message);
    sdk::history::end_turn(&folder, "ana").expect("end ana");
    sdk::history::end_turn(&folder, "bob").expect("end bob");
    // With the shared turns ended, exclusivity succeeds.
    let exclusive = sdk::history::begin_turn(
        &folder,
        "cara",
        &BeginOptions {
            exclusive: true,
            if_open: false,
        },
    )
    .expect("exclusive after ends");
    assert!(exclusive.exclusive);
    sdk::history::end_turn(&folder, "cara").expect("end cara");
}

// Unavailability is forced with Unix directory permission bits,
// which Windows does not honor the same way.
#[test]
#[cfg(unix)]
fn listing_works_when_writes_are_unavailable() {
    let root = tmp("unwritable");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    let mut session = open(&folder);
    add_text(&mut session, "A");
    session.persist_turn("ana").expect("turn");
    drop(session);

    let history_dir = folder.join(".framesmith").join("history");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&history_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }
    // Listing still works, saying writes are unavailable with the cause.
    let listing = sdk::history::list_turns(&folder).expect("list when unwritable");
    assert_eq!(listing.turns.len(), 1);
    assert!(!listing.writes_available);
    let cause = listing.write_cause.expect("cause");
    assert!(cause.contains("not writable"), "{cause}");
    // Reads keep working too.
    assert!(sdk::history::show_turn(&folder, 0).is_ok());
    assert!(sdk::history::peek_turn(&folder).unwrap().is_some());
    // Writes refuse with the cause instead of guessing. Rewriting an
    // existing file needs file permission, so the index itself goes
    // read-only too; the directory bit is what the listing reports.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            history_dir.join("turns.json"),
            std::fs::Permissions::from_mode(0o444),
        )
        .unwrap();
    }
    let err = sdk::history::pin_turn(&folder, 0, true).expect_err("pin refuses");
    assert_eq!(err.code, E_SDK_HISTORY_UNWRITABLE);
    assert!(err.message.contains("turns.json"), "{}", err.message);
    let err = sdk::history::begin_turn(
        &folder,
        "bob",
        &BeginOptions {
            exclusive: false,
            if_open: false,
        },
    )
    .expect_err("begin refuses");
    assert_eq!(err.code, E_SDK_HISTORY_UNWRITABLE);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&history_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(
            history_dir.join("turns.json"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
    assert!(sdk::history::pin_turn(&folder, 0, true).is_ok());
}

#[test]
fn undo_with_missing_target_or_backup_refuses_naming_which() {
    let root = tmp("missing");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    let mut session = open(&folder);
    add_text(&mut session, "A");
    session.persist_turn("ana").expect("turn 0");
    drop(session);

    // Missing target: the turn was never recorded.
    let tree = tree_bytes(&folder);
    let err = sdk::history::undo_turn(&folder, 99).expect_err("missing target");
    assert_eq!(err.code, E_SDK_MISSING_TARGET);
    assert!(err.message.contains("turn 99"), "{}", err.message);
    assert!(err.message.contains("target is missing"), "{}", err.message);
    assert_eq!(tree_bytes(&folder), tree);

    // Missing backup: the turn exists but its snapshot is gone.
    let mut session = open(&folder);
    add_text(&mut session, "B");
    session.persist_turn("ana").expect("turn 1");
    drop(session);
    std::fs::remove_dir_all(
        folder
            .join(".framesmith")
            .join("history")
            .join("backups")
            .join("1"),
    )
    .unwrap();
    let tree = tree_bytes(&folder);
    let err = sdk::history::undo_turn(&folder, 1).expect_err("missing backup");
    assert_eq!(err.code, E_SDK_MISSING_BACKUP);
    assert!(err.message.contains("backup"), "{}", err.message);
    assert!(err.message.contains('1'), "{}", err.message);
    assert_eq!(tree_bytes(&folder), tree);
    // Restore names the same split: missing target vs missing backup.
    let err = sdk::history::restore_turn(&folder, 99).expect_err("restore missing target");
    assert_eq!(err.code, E_SDK_MISSING_TARGET);
}

#[test]
fn corrupt_turn_index_refuses_naming_the_file() {
    let root = tmp("corrupt");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let _ = open(&folder);
    write(&folder, ".framesmith/history/turns.json", "{ not json");

    let err = sdk::history::list_turns(&folder).expect_err("corrupt index");
    assert_eq!(err.code, E_SDK_HISTORY_UNAVAILABLE);
    assert!(err.message.contains("turns.json"), "{}", err.message);
    let err = sdk::history::peek_turn(&folder).expect_err("corrupt index peek");
    assert_eq!(err.code, E_SDK_HISTORY_UNAVAILABLE);
}

#[test]
fn restore_brings_back_a_pinned_turn() {
    let root = tmp("restore");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    let mut session = open(&folder);
    add_text(&mut session, "A");
    session.persist_turn("ana").expect("turn 0");
    drop(session);
    let mut session = open(&folder);
    add_text(&mut session, "B");
    session.persist_turn("bob").expect("turn 1");
    drop(session);
    sdk::history::pin_turn(&folder, 1, true).expect("pin");

    sdk::history::undo_turn(&folder, 1).expect("undo");
    assert_eq!(open(&folder).patches().len(), 1);

    let receipt = sdk::history::restore_turn(&folder, 1).expect("restore");
    assert_eq!(receipt.seq, 1);
    assert_eq!(receipt.actor, "bob");
    assert!(
        receipt.restored_state.contains("again"),
        "{}",
        receipt.restored_state
    );
    assert_eq!(open(&folder).patches().len(), 2);
    let detail = sdk::history::show_turn(&folder, 1).expect("show");
    assert!(!detail.record.undone);
    assert!(detail.record.pinned);
}

#[test]
fn render_check_and_validation_never_read_history() {
    let root = tmp("boundary");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let before_kdl = std::fs::read(folder.join("composition.kdl")).unwrap();

    let mut session = open(&folder);
    add_text(&mut session, "A");
    session.persist_turn("ana").expect("turn");
    drop(session);

    // Opening validates exactly like `check` with the history present:
    // the record never feeds validation.
    let reopened = open(&folder);
    assert_eq!(reopened.patches().len(), 1);
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        before_kdl
    );
    // Every history byte lives under the hidden directory; composition
    // sources carry no trace of the record.
    for key in tree_bytes(&folder).keys() {
        // Separator-agnostic: Windows tree keys carry backslashes.
        let key = key.replace('\\', "/");
        assert!(
            key.starts_with(".framesmith/") || key == "composition.kdl",
            "unexpected file: {key}"
        );
    }
    assert!(!has_render_output(&folder));
    // Deleting the whole record still opens the same composition;
    // the file on disk wins for current state.
    std::fs::remove_dir_all(folder.join(".framesmith")).unwrap();
    let reopened = SdkSession::open(
        &folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("open without history");
    assert_eq!(reopened.effective_composition().clips.len(), 1);
}

#[test]
fn hyphenated_font_try_order_survives_the_turn_fail_closed() {
    let root = tmp("font");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);

    let mut session = open(&folder);
    session
        .apply(EditOp::AddElement {
            element: NewElement::Text {
                id: None,
                label: None,
                value: "Styled".to_string(),
                start: 0.0,
                duration: 1.0,
                x: 10.0,
                y: 40.0,
                size: 24.0,
                color: "#ffffff".to_string(),
                font_family: Some("NoSuchFace, serif".to_string()),
            },
            position: None,
        })
        .expect("apply");
    session.persist_turn("ana").expect("turn");
    drop(session);

    // Unknown families stay fail-closed: the open keeps the authored
    // try-order untouched instead of resolving or substituting.
    let reopened = open(&folder);
    let families = reopened
        .effective_composition()
        .clips
        .iter()
        .filter_map(|c| match c {
            notation::model::Clip::Text(t) => t.font_family.clone(),
            _ => None,
        })
        .next_back()
        .expect("added text keeps its families");
    assert_eq!(
        families,
        vec!["NoSuchFace".to_string(), "serif".to_string()]
    );
    let detail = sdk::history::show_turn(&folder, 0).expect("show");
    assert!(detail.record.targets.iter().any(|t| t.contains("add text")));
}

#[test]
fn read_only_preview_and_empty_actor_refuse_before_anything_changes() {
    let root = tmp("refusals");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    let mut session = open(&folder);
    add_text(&mut session, "A");
    session.persist_turn("ana").expect("turn 0");
    drop(session);
    let tree = tree_bytes(&folder);

    let mut read_only = SdkSession::open(
        &folder,
        OpenOptions {
            persist: false,
            ..OpenOptions::default()
        },
    )
    .expect("read-only open");
    let err = read_only
        .persist_turn("ana")
        .expect_err("read-only refuses");
    assert_eq!(err.code, E_SDK_READ_ONLY);

    let mut preview = SdkSession::open(
        &folder,
        OpenOptions {
            preview: true,
            ..OpenOptions::default()
        },
    )
    .expect("preview open");
    let err = preview.persist_turn("ana").expect_err("preview refuses");
    assert_eq!(err.code, E_SDK_PREVIEW);

    let mut session = open(&folder);
    let err = session
        .persist_turn("   ")
        .expect_err("empty actor refuses");
    assert_eq!(err.code, E_SDK_BAD_VALUE);
    assert_eq!(tree_bytes(&folder), tree);

    // Sessions without history persist the delta but record no turn.
    let mut no_history = SdkSession::open(
        &folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("no-history open");
    add_text(&mut no_history, "B");
    let receipt = no_history
        .persist_turn("ana")
        .expect("persist without record");
    assert_eq!(receipt.turn, None);
    assert_eq!(receipt.actor, None);
    // No new turn was recorded: the index still holds turn 0 only.
    assert_eq!(sdk::history::list_turns(&folder).unwrap().turns.len(), 1);
}
