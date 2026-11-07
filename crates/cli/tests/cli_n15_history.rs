//! History CLI verbs end to end (Sprint 15, T008 / CAP-007, CAP-008,
//! CAP-009): `history list|show|undo|restore|peek|pin|begin|end` over
//! the SDK history module, `show --diff` naming changed files,
//! `undo --who` reversing a named actor's last turn still in effect,
//! `--exclusive`/`--who`/`--if-open` guards refusing with the holder,
//! listing while writes are unavailable, and missing target/backup
//! refusals naming which one is missing.
//!
//! Turns are recorded through the SDK (the CLI never records turns
//! itself); every behavior is then driven through the `framesmith
//! history` binary surface, human and `--json` alike.

use sdk::{EditOp, NewElement, OpenOptions, SdkSession};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-cli-history-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn text_project(folder: &Path) {
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    text value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
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

/// One recorded turn by `actor` carrying one added text element.
fn record_turn(folder: &Path, actor: &str) {
    let mut session = open(folder);
    let n = session.patches().len();
    add_text(&mut session, &format!("by-{actor}-{n}"));
    let receipt = session.persist_turn(actor).expect("persist turn");
    assert!(receipt.turn.is_some(), "turn must be recorded");
}

/// One CLI invocation; stdout and stderr stay separate so `--json`
/// purity (stdout parses) and refusal routing (stderr) are both
/// observable.
fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(bin())
        .args(args)
        .output()
        .expect("run framesmith binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn proj(root: &Path) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    text_project(&folder);
    folder
}

#[test]
fn history_list_shows_named_turns_in_order() {
    let root = tmp("list");
    let folder = proj(&root);
    record_turn(&folder, "maker");
    record_turn(&folder, "agent");

    let (code, out, _) = run(&["history", "list", folder.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let maker_at = out.find("maker").expect("maker listed: {out}");
    let agent_at = out.find("agent").expect("agent listed: {out}");
    assert!(maker_at < agent_at, "oldest first:\n{out}");
    assert!(out.contains("#0"), "{out}");
    assert!(out.contains("#1"), "{out}");
    assert!(out.contains("writes available"), "{out}");

    // The machine-readable report carries the same turns.
    let (code, out, _) = run(&["history", "list", "--json", folder.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["verb"], "list");
    assert_eq!(parsed["turns"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["turns"][0]["actor"], "maker");
    assert_eq!(parsed["turns"][1]["actor"], "agent");
    assert_eq!(parsed["writes_available"], true);
}

#[test]
fn history_show_diff_names_changed_files() {
    let root = tmp("show");
    let folder = proj(&root);
    record_turn(&folder, "ana");

    let (code, out, _) = run(&["history", "show", folder.to_str().unwrap(), "0"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("actor: ana"), "{out}");
    assert!(out.contains("pass --diff to name them"), "{out}");

    let (code, out, _) = run(&["history", "show", "--diff", folder.to_str().unwrap(), "0"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("changed files:"), "{out}");
    assert!(out.contains("sdk-overrides.json"), "{out}");

    // A missing turn refuses naming which target is missing.
    let (code, _, err) = run(&["history", "show", folder.to_str().unwrap(), "99"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("turn 99"), "{err}");
    assert!(err.contains("no recorded turn"), "{err}");
}

#[test]
fn history_peek_pin_begin_end_round_trip() {
    let root = tmp("roundtrip");
    let folder = proj(&root);
    record_turn(&folder, "ana");
    record_turn(&folder, "bob");
    let f = folder.to_str().unwrap();

    let (code, out, _) = run(&["history", "peek", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("newest turn #1 by bob"), "{out}");

    let (code, out, _) = run(&["history", "pin", f, "0"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("pinned turn #0"), "{out}");

    let (code, out, _) = run(&["history", "show", f, "0"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("pinned: yes"), "{out}");

    let (code, out, _) = run(&["history", "pin", "--unpin", f, "0"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("unpinned turn #0"), "{out}");

    let (code, out, _) = run(&["history", "begin", "--who", "cara", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("began shared turn"), "{out}");

    let (code, out, _) = run(&["history", "end", "--who", "cara", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ended the open turn"), "{out}");

    // Ending a name with no open turn refuses naming it.
    let (code, _, err) = run(&["history", "end", "--who", "ghost", f]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("ghost"), "{err}");
    assert!(err.contains("no open turn"), "{err}");
}

#[test]
fn history_undo_reverses_turn_from_backup_and_restore_brings_it_back() {
    let root = tmp("undo");
    let folder = proj(&root);
    record_turn(&folder, "ana");
    record_turn(&folder, "bob");
    let f = folder.to_str().unwrap();
    assert_eq!(open(&folder).patches().len(), 2);

    let (code, out, _) = run(&["history", "undo", f, "1"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("undone turn #1 by bob"), "{out}");
    assert!(out.contains("backup"), "{out}");
    assert!(out.contains("restored"), "{out}");
    assert_eq!(open(&folder).patches().len(), 1);

    // The undone turn shows its flag in the listing.
    let (code, out, _) = run(&["history", "list", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("[undone]"), "{out}");

    let (code, out, _) = run(&["history", "restore", f, "1"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("restored turn #1 by bob"), "{out}");
    assert!(out.contains("again"), "{out}");
    assert_eq!(open(&folder).patches().len(), 2);
}

#[test]
fn history_undo_who_reverses_named_actor_or_reports_none() {
    let root = tmp("who");
    let folder = proj(&root);
    for actor in ["ana", "bob", "ana"] {
        record_turn(&folder, actor);
    }
    let f = folder.to_str().unwrap();

    let (code, out, _) = run(&["history", "undo", "--who", "ana", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("undone turn #2 by ana"), "{out}");
    assert_eq!(open(&folder).patches().len(), 2);

    let (code, out, _) = run(&["history", "undo", "--who", "bob", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("undone turn #1 by bob"), "{out}");

    // Nobody by that name: the refusal reports it and changes nothing.
    let before = open(&folder).patches().len();
    let (code, _, err) = run(&["history", "undo", "--who", "ghost", f]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("ghost"), "{err}");
    assert!(err.contains("no turn still in effect"), "{err}");
    assert_eq!(open(&folder).patches().len(), before);
}

#[test]
fn history_bare_undo_reverses_newest_and_empty_history_reports_none() {
    let root = tmp("bare");
    let folder = proj(&root);
    let f = folder.to_str().unwrap();

    // Empty history: nothing to reverse, refused with the reason.
    let (code, _, err) = run(&["history", "undo", f]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("E_SDK_NO_TURN"), "{err}");

    record_turn(&folder, "ana");
    record_turn(&folder, "bob");
    let (code, out, _) = run(&["history", "undo", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("undone turn #1 by bob"), "{out}");
    assert_eq!(open(&folder).patches().len(), 1);
}

#[test]
fn history_exclusive_guard_refuses_naming_holder_and_preserves() {
    let root = tmp("guard");
    let folder = proj(&root);
    record_turn(&folder, "ana");
    let f = folder.to_str().unwrap();

    let (code, out, _) = run(&["history", "begin", "--who", "bob", "--exclusive", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("began exclusive turn"), "{out}");

    // A conflicting writer is refused by holder name with the age.
    let (code, _, err) = run(&["history", "begin", "--who", "ana", "--exclusive", f]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("bob"), "{err}");
    assert!(err.contains("open for"), "{err}");
    assert!(err.contains("preserved unapplied"), "{err}");

    // The refused begin changed nothing: turns intact, holder still bob.
    let (code, out, _) = run(&["history", "list", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 turn(s)"), "{out}");
    let (code, _, err) = run(&["history", "begin", "--who", "cara", "--exclusive", f]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("bob"), "{err}");

    // After the holder ends, the next writer proceeds.
    let (code, _, _) = run(&["history", "end", "--who", "bob", f]);
    assert_eq!(code, 0);
    let (code, out, _) = run(&["history", "begin", "--who", "ana", "--exclusive", f]);
    assert_eq!(code, 0, "{out}");
    let (code, _, _) = run(&["history", "end", "--who", "ana", f]);
    assert_eq!(code, 0);
}

#[test]
fn history_if_open_attaches_instead_of_starting_second() {
    let root = tmp("ifopen");
    let folder = proj(&root);
    let f = folder.to_str().unwrap();

    let (code, _, _) = run(&["history", "begin", "--who", "ana", f]);
    assert_eq!(code, 0);
    let (code, out, _) = run(&["history", "begin", "--who", "cara", "--if-open", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("attached"), "{out}");
    assert!(out.contains("ana"), "{out}");

    // JSON reports the attach without a new holder.
    let (code, out, _) = run(&["history", "begin", "--who", "x", "--if-open", "--json", f]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed["attached"], true);
    assert_eq!(parsed["actor"], "ana");

    let (code, _, _) = run(&["history", "end", "--who", "ana", f]);
    assert_eq!(code, 0);
}

/// Restores history-directory permissions on the way out, even on failure.
struct PermGuard {
    dir: PathBuf,
}

impl PermGuard {
    fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Drop for PermGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o755));
        }
    }
}

#[test]
fn history_listing_works_when_writes_are_unavailable() {
    let root = tmp("unwritable");
    let folder = proj(&root);
    record_turn(&folder, "ana");
    let f = folder.to_str().unwrap();

    let history_dir = folder.join(".framesmith").join("history");
    let _guard = PermGuard::new(history_dir.clone());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&history_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    // Listing still works, saying writes are unavailable with the cause.
    let (code, out, _) = run(&["history", "list", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("#0 ana"), "{out}");
    assert!(out.contains("writes are unavailable"), "{out}");
    assert!(out.contains("not writable"), "{out}");

    // Reads keep working too.
    let (code, _, _) = run(&["history", "show", "--diff", f, "0"]);
    assert_eq!(code, 0);
    let (code, _, _) = run(&["history", "peek", f]);
    assert_eq!(code, 0);
}

#[test]
fn history_undo_and_restore_refuse_naming_missing_target_or_backup() {
    let root = tmp("missing");
    let folder = proj(&root);
    record_turn(&folder, "ana");
    record_turn(&folder, "bob");
    let f = folder.to_str().unwrap();

    // Missing target: the turn was never recorded.
    let (code, _, err) = run(&["history", "undo", f, "99"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("turn 99"), "{err}");
    assert!(err.contains("target is missing"), "{err}");

    // Missing backup: the turn exists but its snapshot is gone.
    std::fs::remove_dir_all(
        folder
            .join(".framesmith")
            .join("history")
            .join("backups")
            .join("1"),
    )
    .unwrap();
    let (code, _, err) = run(&["history", "undo", f, "1"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("backup"), "{err}");

    // Restore names the same split.
    let (code, _, err) = run(&["history", "restore", f, "99"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("target is missing"), "{err}");
    let (code, _, err) = run(&["history", "restore", f, "1"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("backup"), "{err}");
}

#[test]
fn history_misuse_exits_2_with_usage() {
    let root = tmp("misuse");
    let folder = proj(&root);
    let f = folder.to_str().unwrap();

    // Unknown verb.
    let (code, _, err) = run(&["history", "bogus", f]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("unknown verb"), "{err}");

    // Missing verb and folder.
    let (code, _, err) = run(&["history"]);
    assert_eq!(code, 2, "{err}");

    // Verbs needing a sequence.
    for verb in ["show", "pin", "restore"] {
        let (code, _, err) = run(&["history", verb, f]);
        assert_eq!(code, 2, "{err}");
        assert!(err.contains("<seq>"), "{err}");
    }

    // Guards needing an actor.
    for verb in ["begin", "end"] {
        let (code, _, err) = run(&["history", verb, f]);
        assert_eq!(code, 2, "{err}");
        assert!(err.contains("--who"), "{err}");
    }

    // Flags on the wrong verb.
    let (code, _, err) = run(&["history", "list", "--diff", f]);
    assert_eq!(code, 2, "{err}");
    let (code, _, err) = run(&["history", "undo", "--who", "ana", f, "0"]);
    assert_eq!(code, 2, "{err}");

    // A missing project refuses (exit 1), naming history unavailable.
    let missing = root.join("no-such-proj");
    let (code, _, err) = run(&["history", "list", missing.to_str().unwrap()]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("history is unavailable"), "{err}");
}

#[test]
fn history_json_refusal_carries_code_and_message() {
    let root = tmp("jsonref");
    let folder = proj(&root);
    let f = folder.to_str().unwrap();

    let (code, out, _) = run(&["history", "show", "--json", f, "7"]);
    assert_eq!(code, 1, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed["status"], "refused");
    assert_eq!(parsed["code"], "E_SDK_MISSING_TARGET");
    assert!(parsed["message"].as_str().unwrap().contains("turn 7"));
}
