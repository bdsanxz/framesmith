//! Staged variable edit, remove, and warning-carrying persist
//! (Sprint 18, T005): edits re-validate exactly like declares,
//! removals refuse while referenced (listing every site) and land
//! on disk, and the lenient persist blocks hard offenders writing
//! nothing while unused declarations persist as warnings.

use sdk::{
    OpenOptions, SdkSession, VariableInput, E_SDK_BAD_VARIABLE, E_SDK_INVALID_VARIABLES,
    E_SDK_UNKNOWN_VARIABLE,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-vars-edit-{}-{}-{}",
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

fn project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"accent\" type=\"color\" default=\"#ff3366\"\n    text id=\"title\" value=\"Hello\" start=0 duration=1 x=10 y=40 size=24 color=\"var(--accent, #ffffff)\"\n}\n",
    );
}

fn input(id: &str, var_type: &str, default: Option<&str>) -> VariableInput {
    VariableInput {
        id: id.to_string(),
        var_type: var_type.to_string(),
        default: default.map(|s| s.to_string()),
        label: None,
        options: None,
    }
}

/// Edit rewrites the staged record and the open composition entry:
/// the new default reads back and replays after persist.
#[test]
fn edit_rewrites_staged_declaration() {
    let dir = tmp("edit-ok");
    project(&dir);
    let mut session = open(&dir);
    session
        .declare_variable(input("stage", "string", Some("A")))
        .expect("declare");
    let back = session
        .edit_variable("stage", input("stage", "string", Some("B")))
        .expect("edit");
    assert_eq!(back.id, "stage");
    assert_eq!(back.var_type, "string");
    assert_eq!(back.default.as_deref(), Some("B"));
    let outcome = session
        .persist_variables_with_warnings()
        .expect("lenient persist");
    assert_eq!(outcome.report.declarations, 1);
    let again = open(&dir);
    let decl = again
        .variable_declarations()
        .iter()
        .find(|d| d.id == "stage")
        .expect("replayed declaration");
    assert_eq!(
        format!("{:?}", decl.default),
        format!(
            "{:?}",
            Some(notation::model::VariableDefault::Str("B".to_string()))
        )
    );
}

/// Edits validate exactly like declares: bad types, bad defaults,
/// and duplicate renames refuse staging nothing.
#[test]
fn edit_validates_like_declare() {
    let dir = tmp("edit-bad");
    project(&dir);
    let mut session = open(&dir);
    session
        .declare_variable(input("stage", "string", Some("A")))
        .expect("declare");
    session
        .declare_variable(input("other", "number", Some("1.5")))
        .expect("declare other");
    let bad = session
        .edit_variable("stage", input("stage", "nope", Some("A")))
        .expect_err("unknown type refuses");
    assert_eq!(bad.code, E_SDK_BAD_VARIABLE);
    let bad = session
        .edit_variable("other", input("other", "number", Some("abc")))
        .expect_err("mistyped default refuses");
    assert_eq!(bad.code, E_SDK_BAD_VARIABLE);
    let dup = session
        .edit_variable("stage", input("other", "string", Some("A")))
        .expect_err("rename onto a taken id refuses");
    assert_eq!(dup.code, E_SDK_BAD_VARIABLE);
    // Nothing staged: the read-back still shows the originals.
    let ids: Vec<String> = session
        .staged_variable_records()
        .iter()
        .map(|r| r.id.clone())
        .collect();
    assert!(ids.contains(&"stage".to_string()), "staged kept: {ids:?}");
    let stage = session
        .variable_declarations()
        .iter()
        .find(|d| d.id == "stage")
        .expect("stage kept");
    assert_eq!(stage.var_type, notation::model::VariableType::String);
}

/// Edits of unknown and on-disk ids refuse: unknown ids name
/// `E_SDK_UNKNOWN_VARIABLE`, on-disk declarations live in their
/// KDL source.
#[test]
fn edit_unknown_and_disk_ids_refuse() {
    let dir = tmp("edit-ids");
    project(&dir);
    let mut session = open(&dir);
    let unknown = session
        .edit_variable("ghost", input("ghost", "string", Some("A")))
        .expect_err("unknown id refuses");
    assert_eq!(unknown.code, E_SDK_UNKNOWN_VARIABLE);
    assert!(unknown.message.contains("--ghost"), "names it: {unknown}");
    let disk = session
        .edit_variable("accent", input("accent", "color", Some("#000000")))
        .expect_err("on-disk id refuses");
    assert_eq!(disk.code, E_SDK_BAD_VARIABLE);
    assert!(
        disk.message.contains("project sources"),
        "routes to source: {disk}"
    );
}

/// Removal drops an unreferenced staged declaration and the delta
/// rewrite lands it: reopening shows no record.
#[test]
fn remove_drops_unreferenced_declaration() {
    let dir = tmp("remove-ok");
    project(&dir);
    let mut session = open(&dir);
    session
        .declare_variable(input("temp", "string", Some("T")))
        .expect("declare");
    session
        .persist_variables_with_warnings()
        .expect("persist declare");
    let removed = session.remove_variable("temp").expect("remove");
    assert_eq!(removed.id, "temp");
    let outcome = session
        .persist_variables_with_warnings()
        .expect("persist removal");
    assert!(
        outcome
            .report
            .paths
            .iter()
            .any(|p| p.ends_with("sdk-variables.json")),
        "removal rewrites the delta: {:?}",
        outcome.report.paths
    );
    let again = open(&dir);
    assert!(
        again.variable_declarations().iter().all(|d| d.id != "temp"),
        "removal replayed"
    );
    assert!(again.staged_variable_records().is_empty(), "stage clean");
}

/// Removal while `var(--id)` references stand refuses listing
/// every site, removing nothing — including references hand-added
/// to the sources after the declare staged.
#[test]
fn remove_refuses_while_referenced() {
    let dir = tmp("remove-used");
    project(&dir);
    let mut session = open(&dir);
    session
        .declare_variable(input("stage", "string", Some("A")))
        .expect("declare");
    write(
        &dir,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"accent\" type=\"color\" default=\"#ff3366\"\n    text id=\"title\" value=\"Hello var(--stage)\" start=0 duration=1 x=10 y=40 size=24 color=\"var(--accent, #ffffff)\"\n}\n",
    );
    let refused = session
        .remove_variable("stage")
        .expect_err("in-use refuses");
    assert_eq!(refused.code, E_SDK_BAD_VARIABLE);
    assert!(refused.message.contains("--stage"), "names it: {refused}");
    assert!(
        refused.message.contains("composition.kdl"),
        "lists the site: {refused}"
    );
    // Nothing dropped: the record stages on.
    assert_eq!(session.staged_variable_records().len(), 1);
}

/// Removal of unknown and on-disk ids refuses without touching
/// the stage.
#[test]
fn remove_unknown_and_disk_ids_refuse() {
    let dir = tmp("remove-ids");
    project(&dir);
    let mut session = open(&dir);
    let unknown = session
        .remove_variable("ghost")
        .expect_err("unknown refuses");
    assert_eq!(unknown.code, E_SDK_UNKNOWN_VARIABLE);
    let disk = session.remove_variable("accent").expect_err("disk refuses");
    assert_eq!(disk.code, E_SDK_BAD_VARIABLE);
    assert!(
        disk.message.contains("project sources"),
        "routes to source: {disk}"
    );
}

/// The lenient persist writes unused declarations and returns
/// the advisories as warnings instead of blocking.
#[test]
fn lenient_persist_warns_unused() {
    let dir = tmp("lenient");
    project(&dir);
    let mut session = open(&dir);
    session
        .declare_variable(input("fresh", "string", Some("Hi")))
        .expect("declare");
    let outcome = session
        .persist_variables_with_warnings()
        .expect("unused persists with warnings");
    assert_eq!(outcome.report.declarations, 1);
    assert_eq!(
        outcome.warnings.len(),
        1,
        "one advisory: {:?}",
        outcome.warnings
    );
    assert!(
        outcome.warnings[0].message.contains("--fresh")
            && outcome.warnings[0].message.contains("never bound"),
        "never-bound warning: {:?}",
        outcome.warnings
    );
    // The strict path still blocks on the same stage.
    let mut strict = open(&dir);
    strict
        .declare_variable(input("fresh2", "string", Some("Hi")))
        .expect("declare");
    let blocked = strict.persist_variables().expect_err("strict blocks");
    assert_eq!(blocked.code, E_SDK_INVALID_VARIABLES);
}

/// The lenient persist blocks hard offenders writing nothing: an
/// image declaration whose file vanishes after staging fails its
/// rule at persist time, stopping the save with the delta
/// unwritten.
#[test]
fn lenient_persist_blocks_hard_writing_nothing() {
    let dir = tmp("lenient-hard");
    project(&dir);
    std::fs::create_dir_all(dir.join("media")).unwrap();
    write(&dir, "media/photo.png", "\u{89}PNG\r\n\u{1a}\n");
    let mut session = open(&dir);
    session
        .declare_variable(input("hero", "image", Some("media/photo.png")))
        .expect("declare");
    std::fs::remove_file(dir.join("media/photo.png")).unwrap();
    let blocked = session
        .persist_variables_with_warnings()
        .expect_err("vanished image blocks");
    assert_eq!(blocked.code, E_SDK_INVALID_VARIABLES);
    assert!(
        blocked.message.contains("\"hero\""),
        "names the offender: {blocked}"
    );
    assert!(
        !dir.join(".framesmith/sdk-variables.json").exists(),
        "nothing saved"
    );
}
