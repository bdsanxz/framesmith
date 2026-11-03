//! Sprint 6 T006: several compositions with one root.
//!
//! Validates through the public [`validate_folder`] API only: several
//! uniquely named blocks validate together, the lone composition is
//! the implicit root, several require exactly one `root=true`, and
//! cross-composition `start-after` targets refuse while `nest` targets
//! stay project-wide.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n06t006-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &Path, body: &str) {
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
}

fn codes(dir: &Path) -> Vec<String> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

// --- root rules ----------------------------------------------------------

#[test]
fn two_compositions_with_one_root_validate_and_select_root() {
    let dir = tmp("marked-root");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=1 duration=1\n\
         }\n\
         composition name=\"cutdown\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.compositions.len(), 2);
    assert_eq!(v.compositions[0].name, "main");
    assert_eq!(v.compositions[1].name, "cutdown");
    assert_eq!(v.root_name.as_deref(), Some("cutdown"));
    let comp = v.composition.expect("valid verdict carries the root");
    assert_eq!(comp.name, "cutdown");
    assert_eq!(comp.clips.len(), 1);
}

#[test]
fn three_compositions_with_middle_root_select_it() {
    let dir = tmp("three");
    write_comp(
        &dir,
        "composition name=\"a\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n\
         composition name=\"b\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=0 duration=1\n\
         }\n\
         composition name=\"c\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.root_name.as_deref(), Some("b"));
    assert_eq!(v.composition.expect("root").name, "b");
}

#[test]
fn single_composition_without_mark_is_the_implicit_root() {
    let dir = tmp("implicit");
    write_comp(
        &dir,
        "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.compositions.len(), 1);
    assert_eq!(v.root_name.as_deref(), Some("solo"));
    assert_eq!(v.composition.expect("root").name, "solo");
}

#[test]
fn single_composition_with_root_true_stays_valid() {
    let dir = tmp("solo-marked");
    write_comp(
        &dir,
        "composition name=\"solo\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.root_name.as_deref(), Some("solo"));
}

#[test]
fn single_composition_with_root_false_stays_the_implicit_root() {
    let dir = tmp("solo-unmarked");
    write_comp(
        &dir,
        "composition name=\"solo\" root=false width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.root_name.as_deref(), Some("solo"));
}

#[test]
fn several_without_root_mark_fail_ambiguous() {
    let dir = tmp("no-root");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n\
         composition name=\"cutdown\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_AMBIGUOUS_ROOT");
    assert_eq!((d.line, d.column), (1, 1));
    assert!(d.message.contains("2 compositions"), "{}", d.message);
    assert!(!d.fix.is_empty());
    assert!(v.composition.is_none());
    assert!(v.compositions.is_empty());
    assert!(v.root_name.is_none());
}

#[test]
fn several_root_marks_fail_ambiguous_naming_both() {
    let dir = tmp("two-roots");
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n\
         composition name=\"cutdown\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_AMBIGUOUS_ROOT");
    assert!(d.message.contains("\"main\""), "{}", d.message);
    assert!(d.message.contains("\"cutdown\""), "{}", d.message);
    assert!(!d.fix.is_empty());
}

#[test]
fn non_boolean_root_mark_fails() {
    let dir = tmp("root-shape");
    write_comp(
        &dir,
        "composition name=\"main\" root=\"yes\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n\
         composition name=\"cutdown\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    // A malformed mark is not a mark: the project reports the bad
    // shape plus the resulting ambiguity together.
    assert_eq!(codes(&dir), vec!["E_AMBIGUOUS_ROOT", "E_AMBIGUOUS_ROOT"]);
}

#[test]
fn duplicate_composition_names_fail() {
    let dir = tmp("dup-name");
    write_comp(
        &dir,
        "composition name=\"same\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n\
         composition name=\"same\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert!(codes(&dir).contains(&"E_DUP_ID".to_string()));
}

#[test]
fn stray_top_level_nodes_still_refused_with_several_compositions() {
    let dir = tmp("stray");
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n\
         subtitle value=\"Hi\" start=0 duration=1\n\
         composition name=\"cutdown\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert!(codes(&dir).contains(&"E_UNKNOWN_ELEMENT".to_string()));
}

// --- timing across compositions ------------------------------------------

#[test]
fn start_after_own_composition_name_resolves_to_own_end() {
    let dir = tmp("own-follow");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
         }\n\
         composition name=\"alt\" root=true width=64 height=64 duration=4 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" duration=1 start-after=\"alt\"\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.compositions.len(), 2);
    assert_eq!(v.compositions[1].clips[0].start(), 4.0);
    // The other composition keeps its own independent windows.
    assert_eq!(v.compositions[0].clips[0].start(), 0.0);
}

#[test]
fn start_after_clip_in_another_composition_refuses() {
    let dir = tmp("cross-clip");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         box id=\"lead\" x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
         }\n\
         composition name=\"alt\" root=true width=64 height=64 duration=4 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" duration=1 start-after=\"lead\"\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_UNKNOWN_REF");
    assert!(d.message.contains("\"lead\""), "{}", d.message);
    assert!(d.message.contains("\"main\""), "{}", d.message);
    assert!(!d.fix.is_empty());
}

#[test]
fn start_after_other_composition_name_refuses() {
    let dir = tmp("cross-comp");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
         }\n\
         composition name=\"alt\" root=true width=64 height=64 duration=4 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" duration=1 start-after=\"main\"\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_REF"]);
}

#[test]
fn nest_naming_another_composition_validates() {
    let dir = tmp("nest-cross");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"bumper\" start=0 duration=1\n\
         }\n\
         composition name=\"bumper\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.compositions.len(), 2);
    assert_eq!(v.root_name.as_deref(), Some("bumper"));
}
