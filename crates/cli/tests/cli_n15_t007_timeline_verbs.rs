//! Timeline edit verbs end-to-end (Sprint 15, T007 / CAP-006):
//! `timeline move|trim|split|delete|set|duplicate` rewrite the effective
//! composition with a visible result, every time-expression form
//! resolves to its film position, rows expose `mediaStart`, `plan`
//! prints only a count and changes nothing, `apply` prints an undo
//! receipt, `undo` restores the exact prior sources, ranged delete
//! removes only its range with snap, and unresolvable expressions or
//! incapable verbs refuse with a reason code leaving no receipt. The
//! bare Sprint 6 listing keeps working unchanged alongside the verbs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-cli-n15t007-{tag}-{}-{}",
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

/// One CLI invocation; stdout and stderr stay separate so plan output
/// (count only on stdout) and refusal reports can be asserted exactly.
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

fn text_project(folder: &Path) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=6 fps=10 background=\"#112233\" version=1 {\n    text id=\"a\" value=\"One\" start=0 duration=2 x=10 y=40 size=24 color=\"#ffffff\" font-family=\"Alpha, Beta\"\n    text id=\"b\" value=\"Two\" start=2 duration=2 x=10 y=80 size=24 color=\"#ffffff\"\n    text id=\"c\" value=\"Three\" start=4 duration=2 x=10 y=120 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

fn footage_project(folder: &Path) {
    std::fs::create_dir_all(folder.join("media")).unwrap();
    std::fs::write(folder.join("media/clip.mp4"), b"not-a-real-video").unwrap();
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=6 fps=10 background=\"#112233\" version=1 {\n    video id=\"v\" path=\"media/clip.mp4\" start=0 duration=4 trim-start=1 rate=1\n    text id=\"tail\" value=\"Tail\" start=4 duration=2 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

fn project(tag: &str) -> (PathBuf, PathBuf) {
    let root = tmp(tag);
    let folder = root.join("proj");
    text_project(&folder);
    (root, folder)
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

fn s(folder: &Path) -> String {
    folder.to_str().unwrap().to_string()
}

#[test]
fn each_verb_rewrites_with_a_visible_result() {
    // move shifts one clip in film time and prints a receipt plus rows.
    let (_root, folder) = project("verbs-move");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "move", "--clip", "b", "--to", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("window 3.000s..5.000s"), "{out}");

    // trim rewrites one window.
    let (_root, folder) = project("verbs-trim");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "trim", "--clip", "a", "--start", "+0.5", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("window 0.500s..2.000s"), "{out}");

    // split divides one clip into two touching halves.
    let (_root, folder) = project("verbs-split");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "split", "--clip", "a", "--at", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("window 0.000s..1.000s"), "{out}");
    assert!(out.contains("window 1.000s..2.000s"), "{out}");

    // delete removes the whole target clip.
    let (_root, folder) = project("verbs-delete");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "delete", "--clip", "b", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(!out.contains("id=\"b\""), "{out}");
    assert!(out.contains("(2 clip(s)"), "{out}");

    // set writes one property.
    let (_root, folder) = project("verbs-set");
    let f = s(&folder);
    let (code, out, err) = run(&[
        "timeline",
        "set",
        "--clip",
        "a",
        "--property",
        "hidden",
        "--value",
        "true",
        &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("hidden"), "{out}");

    // duplicate clones the clip right after itself.
    let (_root, folder) = project("verbs-dup");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "duplicate", "--clip", "a", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("(4 clip(s)"), "{out}");
    assert!(out.contains("window 2.000s..4.000s"), "{out}");
}

#[test]
fn each_time_expression_form_resolves_to_its_film_position() {
    // +N reads N seconds after the target clip's start (b starts at 2).
    let (_root, folder) = project("times-offset");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "move", "--clip", "b", "--to", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 3.000s..5.000s"), "{out}");

    // Nf reads film frame N at the 10fps rate (20f = 2s).
    let (_root, folder) = project("times-frames");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "move", "--clip", "a", "--to", "20f", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 2.000s..4.000s"), "{out}");

    // end reads the 6s composition end.
    let (_root, folder) = project("times-end");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "move", "--clip", "c", "--to", "end", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 6.000s..8.000s"), "{out}");

    // after:<id> reads the referenced clip's end (b ends at 4).
    let (_root, folder) = project("times-after-id");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "move", "--clip", "a", "--to", "after:b", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 4.000s..6.000s"), "{out}");

    // after:<index> reads the document index's end (index 1 ends at 4).
    let (_root, folder) = project("times-after-index");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "move", "--clip", "c", "--to", "after:1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 4.000s..6.000s"), "{out}");

    // A fully numeric --clip reads as a document index (1 is b).
    let (_root, folder) = project("times-clip-index");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "move", "--clip", "1", "--to", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 3.000s..5.000s"), "{out}");
}

#[test]
fn rows_expose_media_start_mapping_file_time_to_film_time() {
    let root = tmp("rows");
    let folder = root.join("proj");
    footage_project(&folder);
    let f = s(&folder);

    // The footage row maps file seconds: trim-start 1 opens the window.
    let (code, out, err) = run(&["timeline", "ids", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("mediaStart 1.000s"), "{out}");
    assert!(out.contains("mediaStart 0.000s"), "{out}");
    assert!(out.contains("[0] video"), "{out}");

    // Moving in film time keeps the file origin: same frames, new time.
    let (code, out, err) = run(&["timeline", "move", "--clip", "v", "--to", "40f", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 4.000s..8.000s"), "{out}");
    assert!(out.contains("mediaStart 1.000s"), "{out}");

    // Trimming the head advances the file origin past trimmed footage.
    let root = tmp("rows-trim");
    let folder = root.join("proj");
    footage_project(&folder);
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "trim", "--clip", "v", "--start", "+2", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 2.000s..4.000s"), "{out}");
    assert!(out.contains("mediaStart 3.000s"), "{out}");
}

#[test]
fn plan_mode_prints_only_a_count_and_changes_nothing() {
    let (_root, folder) = project("plan");
    let f = s(&folder);
    let before = tree_bytes(&folder);

    // `plan` nesting prints only the count line on stdout.
    let (code, out, err) = run(&["timeline", "plan", "move", "--clip", "b", "--to", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out, "timeline plan: 1 clip(s) would change\n", "{out:?}");
    assert!(err.is_empty(), "{err:?}");

    // The --plan flag on a direct verb plans identically.
    let (code, out, err) = run(&[
        "timeline", "move", "--clip", "b", "--to", "+1", "--plan", &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out, "timeline plan: 1 clip(s) would change\n", "{out:?}");

    // Split touches two resulting halves.
    let (code, out, err) = run(&[
        "timeline", "split", "--clip", "a", "--at", "+1", "--plan", &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out, "timeline plan: 2 clip(s) would change\n", "{out:?}");

    // Nothing written anywhere: no hidden directory at all.
    assert_eq!(tree_bytes(&folder), before);
    assert!(!folder.join(".framesmith").exists());
    assert!(!has_render_output(&folder));
}

#[test]
fn apply_prints_an_undo_receipt_and_undo_restores_exact_sources() {
    let (_root, folder) = project("receipt");
    let f = s(&folder);
    let (code, before, _) = run(&["timeline", "ids", &f]);
    assert_eq!(code, 0);

    let (code, out, err) = run(&["timeline", "move", "--clip", "b", "--to", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("window 3.000s..5.000s"), "{out}");

    // Composition sources stay byte-identical; only the hidden delta
    // records the turn, and default renders are untouched.
    let raw = std::fs::read_to_string(folder.join("composition.kdl")).unwrap();
    assert!(
        raw.contains("text id=\"b\" value=\"Two\" start=2 duration=2"),
        "{raw}"
    );
    assert!(folder
        .join(".framesmith")
        .join("sdk-overrides.json")
        .is_file());
    assert!(!has_render_output(&folder));

    // Undo restores the exact prior rows and removes the delta again.
    let (code, out, err) = run(&["timeline", "undo", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("restored 1 patch(es)"), "{out}");
    let (code, after, _) = run(&["timeline", "ids", &f]);
    assert_eq!(code, 0);
    assert_eq!(after, before);
    assert!(!folder
        .join(".framesmith")
        .join("sdk-overrides.json")
        .exists());

    // A second undo names no pending verb application and changes nothing.
    let (code, out, _) = run(&["timeline", "undo", &f]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_NOTHING_TO_UNDO"), "{out}");
    let (code, again, _) = run(&["timeline", "ids", &f]);
    assert_eq!(code, 0);
    assert_eq!(again, before);
}

#[test]
fn undo_restores_exact_sources_after_a_multi_patch_ranged_delete() {
    let (_root, folder) = project("receipt-range");
    let f = s(&folder);
    let (code, before, _) = run(&["timeline", "ids", &f]);
    assert_eq!(code, 0);

    // Range [1,5) stages three patches across three clips.
    let (code, out, err) = run(&["timeline", "delete", "--start", "+1", "--end", "+5", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("window 0.000s..1.000s"), "{out}");
    assert!(out.contains("window 5.000s..6.000s"), "{out}");

    // One undo reverses the whole verb, not one patch.
    let (code, out, err) = run(&["timeline", "undo", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("restored 3 patch(es)"), "{out}");
    let (code, after, _) = run(&["timeline", "ids", &f]);
    assert_eq!(code, 0);
    assert_eq!(after, before);
}

#[test]
fn ranged_delete_removes_only_its_range_with_snap_covering_delete() {
    // Range [1,5): head of a shortens, b drops whole, tail of c shortens.
    let (_root, folder) = project("ranged");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "delete", "--start", "+1", "--end", "+5", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("(2 clip(s)"), "{out}");
    assert!(out.contains("window 0.000s..1.000s"), "{out}");
    assert!(out.contains("window 5.000s..6.000s"), "{out}");

    // Strictly-inside ranges split one clip into two survivors.
    let (_root, folder) = project("ranged-inside");
    let f = s(&folder);
    let (code, out, err) = run(&[
        "timeline", "delete", "--clip", "a", "--start", "+0.5", "--end", "+1.5", &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("(4 clip(s)"), "{out}");
    assert!(out.contains("window 0.000s..0.500s"), "{out}");
    assert!(out.contains("window 1.500s..2.000s"), "{out}");

    // Snap pulls a near-miss edge onto the clip edge: [0.05,2] with a
    // 0.1 tolerance covers the whole [0,2) clip, removing it outright.
    let (_root, folder) = project("ranged-snap");
    let f = s(&folder);
    let (code, out, err) = run(&[
        "timeline", "delete", "--clip", "a", "--start", "+0.05", "--end", "+2", "--snap", "0.1", &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("(2 clip(s)"), "{out}");
    assert!(!out.contains("id=\"a\""), "{out}");

    // Without the snap the same range only shortens the head.
    let (_root, folder) = project("ranged-nosnap");
    let f = s(&folder);
    let (code, out, err) = run(&[
        "timeline", "delete", "--clip", "a", "--start", "+0.05", "--end", "+2", &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 0.000s..0.050s"), "{out}");
}

#[test]
fn set_covers_label_hidden_and_film_window() {
    let (_root, folder) = project("set");
    let f = s(&folder);

    let (code, out, err) = run(&[
        "timeline",
        "set",
        "--clip",
        "a",
        "--property",
        "label",
        "--value",
        "cover",
        &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("label=\"cover\""), "{out}");

    // Verbs stack: later invocations replay the persisted delta.
    let (code, out, err) = run(&[
        "timeline",
        "set",
        "--clip",
        "a",
        "--property",
        "start",
        "--value",
        "1",
        &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 1.000s..3.000s"), "{out}");
    assert!(out.contains("label=\"cover\""), "{out}");

    let (code, out, err) = run(&[
        "timeline",
        "set",
        "--clip",
        "a",
        "--property",
        "duration",
        "--value",
        "0.5",
        &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 1.000s..1.500s"), "{out}");

    // An empty value clears the label.
    let (code, out, err) = run(&[
        "timeline",
        "set",
        "--clip",
        "a",
        "--property",
        "label",
        "--value",
        "",
        &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(!out.contains("label=\"cover\""), "{out}");
}

#[test]
fn duplicate_drops_colliding_ids_unless_a_fresh_one_is_named() {
    // Without a new id the copy drops the id so ids stay unique.
    let (_root, folder) = project("dup");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "duplicate", "--clip", "a", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("id=\"a\""), "{out}");
    assert!(out.contains("id=-"), "{out}");

    // A fresh id is honored on the copy.
    let (_root, folder) = project("dup-named");
    let f = s(&folder);
    let (code, out, err) = run(&[
        "timeline",
        "duplicate",
        "--clip",
        "a",
        "--at",
        "30f",
        "--new-id",
        "a2",
        &f,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("id=\"a2\""), "{out}");
    assert!(out.contains("window 3.000s..5.000s"), "{out}");

    // A taken id refuses before anything stages, leaving no receipt.
    let (_root, folder) = project("dup-taken");
    let f = s(&folder);
    let before = tree_bytes(&folder);
    let (code, out, _) = run(&["timeline", "duplicate", "--clip", "a", "--new-id", "b", &f]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_BAD_VALUE"), "{out}");
    assert!(!out.contains("undo receipt"), "{out}");
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn unresolvable_expressions_and_incapable_verbs_refuse_with_no_receipt() {
    let (_root, folder) = project("refuse");
    let f = s(&folder);
    let before = tree_bytes(&folder);

    // A form outside the four time expressions refuses.
    let (code, out, _) = run(&["timeline", "move", "--clip", "a", "--to", "tomorrow", &f]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_BAD_VALUE"), "{out}");

    // plan mode refuses the same way, printing no count.
    let (code, out, _) = run(&[
        "timeline", "plan", "move", "--clip", "a", "--to", "tomorrow", &f,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_BAD_VALUE"), "{out}");
    assert!(!out.contains("would change"), "{out}");

    // after:<ref> naming nothing refuses with the unknown-target code.
    let (code, out, _) = run(&["timeline", "move", "--clip", "a", "--to", "after:ghost", &f]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_UNKNOWN_TARGET"), "{out}");

    // A split outside the clip window refuses.
    let (code, out, _) = run(&["timeline", "split", "--clip", "a", "--at", "+9", &f]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_BAD_VALUE"), "{out}");

    // A delete naming neither target nor range refuses.
    let (code, out, _) = run(&["timeline", "delete", &f]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_BAD_VALUE"), "{out}");

    // An unknown set property refuses as outside verb capability.
    let (code, out, _) = run(&[
        "timeline",
        "set",
        "--clip",
        "a",
        "--property",
        "opacity",
        "--value",
        "0.5",
        &f,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_UNSUPPORTED"), "{out}");

    // A bad hidden value refuses.
    let (code, out, _) = run(&[
        "timeline",
        "set",
        "--clip",
        "a",
        "--property",
        "hidden",
        "--value",
        "maybe",
        &f,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_BAD_VALUE"), "{out}");

    // Undo with no prior application refuses and changes nothing.
    let (code, out, _) = run(&["timeline", "undo", &f]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_SDK_NOTHING_TO_UNDO"), "{out}");

    // Nothing staged anywhere, nothing written, no receipt left behind.
    assert!(!out.contains("undo receipt"));
    assert_eq!(tree_bytes(&folder), before);
    assert!(!folder.join(".framesmith").exists());
    assert!(!has_render_output(&folder));
}

#[test]
fn malformed_invocations_are_usage_errors() {
    let (_root, folder) = project("usage");
    let f = s(&folder);

    // Missing required flags.
    let (code, _, err) = run(&["timeline", "move", "--clip", "a", &f]);
    assert_eq!(code, 2, "{err}");
    // Unknown flags.
    let (code, _, err) = run(&[
        "timeline", "move", "--clip", "a", "--to", "+1", "--frob", &f,
    ]);
    assert_eq!(code, 2, "{err}");
    // Flags the verb does not take.
    let (code, _, err) = run(&[
        "timeline", "move", "--clip", "a", "--to", "+1", "--at", "+2", &f,
    ]);
    assert_eq!(code, 2, "{err}");
    // plan/apply need a nested content verb.
    let (code, _, err) = run(&["timeline", "plan", &f]);
    assert_eq!(code, 2, "{err}");
    let (code, _, err) = run(&["timeline", "apply", "undo", &f]);
    assert_eq!(code, 2, "{err}");
    // --plan contradicts nested apply.
    let (code, _, err) = run(&[
        "timeline", "apply", "move", "--clip", "a", "--to", "+1", "--plan", &f,
    ]);
    assert_eq!(code, 2, "{err}");
    // Usage errors write nothing.
    assert!(!folder.join(".framesmith").exists());
}

#[test]
fn nested_plan_and_apply_match_the_direct_forms() {
    let (_root, folder) = project("nested-apply");
    let f = s(&folder);
    let (code, out, err) = run(&["timeline", "apply", "move", "--clip", "b", "--to", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("undo receipt"), "{out}");
    assert!(out.contains("window 3.000s..5.000s"), "{out}");

    let (_root, folder) = project("nested-plan");
    let f = s(&folder);
    let before = tree_bytes(&folder);
    let (code, out, err) = run(&["timeline", "plan", "split", "--clip", "a", "--at", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out, "timeline plan: 2 clip(s) would change\n", "{out:?}");
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn font_family_try_order_passes_through_verbs_untouched() {
    let (_root, folder) = project("fonts");
    let f = s(&folder);
    let kdl_before = std::fs::read(folder.join("composition.kdl")).unwrap();

    // Unknown families stay fail-closed: the verb still applies, and
    // nothing resolves, substitutes, or reorders the try-order list.
    let (code, out, err) = run(&["timeline", "move", "--clip", "a", "--to", "+1", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        kdl_before
    );
    let delta =
        std::fs::read_to_string(folder.join(".framesmith").join("sdk-overrides.json")).unwrap();
    assert!(!delta.contains("font"), "{delta}");

    let (code, out, err) = run(&["timeline", "undo", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        kdl_before
    );
}

#[test]
fn sprint6_listing_keeps_working_unchanged_beside_the_verbs() {
    let (_root, folder) = project("listing");
    let f = s(&folder);

    // Human listing: Sprint 6 rows with no mediaStart column.
    let (code, out, err) = run(&["timeline", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.starts_with("timeline of composition \"t\" in "),
        "{out}"
    );
    assert!(out.contains("[0] text id=\"a\""), "{out}");
    assert!(out.contains("window 0.000s..2.000s"), "{out}");
    assert!(!out.contains("mediaStart"), "{out}");

    // JSON listing: same rows, no mediaStart key.
    let (code, out, err) = run(&["timeline", "--json", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("\"command\":\"timeline\""), "{out}");
    assert!(!out.contains("mediaStart"), "{out}");

    // Verbs do not leak into the listing: after an apply the Sprint 6
    // rows still read the on-disk sources (the SDK delta is hidden).
    let (code, _, err) = run(&["timeline", "move", "--clip", "b", "--to", "+1", &f]);
    assert_eq!(code, 0, "{err}");
    let (code, out, err) = run(&["timeline", &f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("window 2.000s..4.000s"), "{out}");
    assert!(!out.contains("mediaStart"), "{out}");
}
