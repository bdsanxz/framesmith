//! Sprint 9 T001: typed `variable` declarations, through the public
//! [`validate_folder`] API — the same verdict `check` reports, with
//! the resolved model asserted on the composition.
//!
//! Covers every acceptance criterion of
//! `T001-declare-typed-variables.md`: one declaration per type with a
//! valid default, `enum` options plus a drawn-from-them default,
//! hyphenated/underscored spellings, unused-stays-valid, and every
//! declaration-level refusal variant naming the declaration.

use notation::model::{VariableDefault, VariableType};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n09t001-{}-{}-{}",
        tag,
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

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n";
const CLIP: &str = "text value=\"Hi\" start=0 duration=2\n";

fn body_with(vars: &str) -> String {
    format!("{OPEN}{vars}{CLIP}}}\n")
}

fn check(body: &str) -> validate::validate::Verdict {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir)
}

fn check_with_files(body: &str, files: &[(&str, &[u8])]) -> validate::validate::Verdict {
    let dir = tmp("case-files");
    write_comp(&dir, body);
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    validate_folder(&dir)
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn messages(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.message.clone()).collect()
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code} in {:?}",
        codes(ds)
    );
    let ms = messages(ds);
    for needle in needles {
        assert!(
            ms.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {ms:?}"
        );
    }
}

fn valid_vars(body: &str) -> Vec<notation::model::VariableDecl> {
    let v = check(body);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.diagnostics.is_empty(), "{:?}", v.diagnostics);
    v.composition.unwrap().variables
}

fn var_of(vars: &[notation::model::VariableDecl], id: &str) -> notation::model::VariableDecl {
    vars.iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("expected variable {id:?} in {vars:?}"))
        .clone()
}

// --- happy paths -------------------------------------------------------------

const ALL_SEVEN: &str = concat!(
    "variable id=\"title\" type=\"string\" default=\"Hello\" label=\"Heading\"\n",
    "variable id=\"count\" type=\"number\" default=1.5\n",
    "variable id=\"accent\" type=\"color\" default=\"#ff3366\"\n",
    "variable id=\"show\" type=\"boolean\" default=true\n",
    "variable id=\"size\" type=\"enum\" options=\"small, large\" default=\"large\"\n",
    "variable id=\"brand\" type=\"font\" default=\"Family, Fallback\"\n",
    "variable id=\"hero\" type=\"image\" default=\"photo.png\"\n",
);

#[test]
fn all_seven_types_with_valid_defaults_are_accepted() {
    let dir = tmp("seven");
    write_comp(&dir, &body_with(ALL_SEVEN));
    std::fs::write(dir.join("photo.png"), [0x89, b'P', b'N', b'G']).unwrap();
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let vars = v.composition.unwrap().variables;
    assert_eq!(vars.len(), 7);

    assert_eq!(var_of(&vars, "title").var_type, VariableType::String);
    assert_eq!(
        var_of(&vars, "title").default,
        Some(VariableDefault::Str("Hello".to_string()))
    );
    assert_eq!(var_of(&vars, "title").label.as_deref(), Some("Heading"));

    assert_eq!(
        var_of(&vars, "count").default,
        Some(VariableDefault::Num(1.5))
    );
    assert_eq!(
        var_of(&vars, "accent").default,
        Some(VariableDefault::Color(notation::model::Rgb {
            r: 0xff,
            g: 0x33,
            b: 0x66
        }))
    );
    assert_eq!(
        var_of(&vars, "show").default,
        Some(VariableDefault::Bool(true))
    );
    assert_eq!(var_of(&vars, "size").var_type, VariableType::Enum);
    assert_eq!(
        var_of(&vars, "size").options,
        Some(vec!["small".to_string(), "large".to_string()])
    );
    assert_eq!(
        var_of(&vars, "size").default,
        Some(VariableDefault::Enum("large".to_string()))
    );
    assert_eq!(
        var_of(&vars, "brand").default,
        Some(VariableDefault::Font(vec![
            "Family".to_string(),
            "Fallback".to_string()
        ]))
    );
    assert_eq!(
        var_of(&vars, "hero").default,
        Some(VariableDefault::Image("photo.png".to_string()))
    );
    for id in ["title", "count", "accent", "show", "size", "brand", "hero"] {
        assert!(var_of(&vars, id).label.is_none() || id == "title");
    }
}

#[test]
fn declaration_attributes_accept_both_spellings() {
    // Every `variable` attribute is a single word, so the hyphenated
    // and underscored spellings coincide: one declaration carrying
    // every attribute validates.
    let vars = valid_vars(&body_with(
        "variable id=\"a\" type=\"enum\" options=\"p, q\" default=\"p\" label=\"n\"\n",
    ));
    assert_eq!(vars.len(), 1);
    assert_eq!(var_of(&vars, "a").label.as_deref(), Some("n"));
}

#[test]
fn unused_declarations_leave_the_composition_valid() {
    // Bound nowhere, warned nowhere (strict reporting is a later
    // slice): the composition stays valid and silent.
    let vars = valid_vars(&body_with(
        "variable id=\"unused\" type=\"string\" default=\"kept\"\n",
    ));
    assert_eq!(vars.len(), 1);
}

#[test]
fn default_less_declaration_is_valid_for_later_strict_slices() {
    let vars = valid_vars(&body_with("variable id=\"late\" type=\"number\"\n"));
    assert_eq!(vars.len(), 1);
    assert_eq!(var_of(&vars, "late").default, None);
}

#[test]
fn string_default_keeps_unicode_spaces_and_punctuation_verbatim() {
    let vars = valid_vars(&body_with(
        "variable id=\"line\" type=\"string\" default=\"Héllo, wörld! 😀 — 100% (x)\"\n",
    ));
    assert_eq!(
        var_of(&vars, "line").default,
        Some(VariableDefault::Str(
            "Héllo, wörld! 😀 — 100% (x)".to_string()
        ))
    );
}

#[test]
fn bare_scalars_for_string_stay_text_never_numbers() {
    let vars = valid_vars(&body_with("variable id=\"n\" type=\"string\" default=5\n"));
    assert_eq!(
        var_of(&vars, "n").default,
        Some(VariableDefault::Str("5".to_string()))
    );
}

#[test]
fn quoted_boolean_spellings_are_accepted() {
    let vars = valid_vars(&body_with(
        "variable id=\"a\" type=\"boolean\" default=\"true\"\nvariable id=\"b\" type=\"boolean\" default=\"false\"\n",
    ));
    assert_eq!(
        var_of(&vars, "a").default,
        Some(VariableDefault::Bool(true))
    );
    assert_eq!(
        var_of(&vars, "b").default,
        Some(VariableDefault::Bool(false))
    );
}

#[test]
fn composition_without_variables_is_unchanged() {
    let v = check(&body_with(""));
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.composition.unwrap().variables.is_empty());
}

// --- declaration-level refusals ------------------------------------------------

#[test]
fn duplicate_variable_id_fails_naming_the_declaration() {
    let v = check(&body_with(
        "variable id=\"dup\" type=\"string\" default=\"a\"\nvariable id=\"dup\" type=\"number\" default=1\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_DUP_ID", &["\"dup\""]);
}

#[test]
fn variable_id_colliding_with_a_clip_id_fails() {
    let v = check(&format!(
        "{OPEN}variable id=\"t1\" type=\"string\" default=\"a\"\ntext value=\"Hi\" start=0 duration=2 id=\"t1\"\n}}\n"
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_DUP_ID", &["\"t1\""]);
}

#[test]
fn unknown_type_fails_naming_the_accepted_set() {
    let v = check(&body_with(
        "variable id=\"bad\" type=\"strnig\" default=\"x\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &[
            "\"bad\"",
            "strnig",
            "accepted types are",
            "string",
            "number",
            "color",
            "boolean",
            "enum",
            "font",
            "image",
        ],
    );
}

#[test]
fn variable_id_must_start_with_a_letter() {
    for bad in ["9lives", "has space", ""] {
        let v = check(&body_with(&format!(
            "variable id=\"{bad}\" type=\"string\" default=\"x\"\n"
        )));
        assert_eq!(v.status, Status::Invalid, "id {bad:?}");
        assert_refusal(&v.diagnostics, "E_BAD_ID", &["must start with a letter"]);
    }
}

#[test]
fn number_default_outside_its_type_fails() {
    let v = check(&body_with(
        "variable id=\"n\" type=\"number\" default=\"abc\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["\"n\"", "not a number"]);
}

#[test]
fn color_default_outside_its_type_fails() {
    let v = check(&body_with(
        "variable id=\"c\" type=\"color\" default=\"red\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["\"c\"", "#rgb", "#rrggbb"],
    );
}

#[test]
fn boolean_default_outside_its_type_fails() {
    let v = check(&body_with(
        "variable id=\"b\" type=\"boolean\" default=\"yes\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["\"b\"", "not a boolean", "`true`", "`false`"],
    );
}

#[test]
fn font_default_with_no_family_fails() {
    let v = check(&body_with(
        "variable id=\"f\" type=\"font\" default=\" , \"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["\"f\"", "names no family"],
    );
}

#[test]
fn enum_with_fewer_than_two_options_fails_naming_the_rule() {
    let v = check(&body_with(
        "variable id=\"e\" type=\"enum\" options=\"only\" default=\"only\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["\"e\"", "at least two `options`"],
    );
}

#[test]
fn enum_without_options_fails() {
    let v = check(&body_with(
        "variable id=\"e\" type=\"enum\" default=\"a\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FIELD", &["\"e\"", "`options`"]);
}

#[test]
fn default_outside_options_fails_naming_the_declaration() {
    let v = check(&body_with(
        "variable id=\"e\" type=\"enum\" options=\"small, large\" default=\"huge\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["\"e\"", "\"huge\"", "\"small\"", "\"large\""],
    );
}

#[test]
fn enum_matching_is_exact_and_case_sensitive() {
    let v = check(&body_with(
        "variable id=\"e\" type=\"enum\" options=\"small, large\" default=\"Large\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["\"e\"", "\"Large\""]);
}

#[test]
fn options_on_a_non_enum_type_fails() {
    let v = check(&body_with(
        "variable id=\"s\" type=\"string\" default=\"x\" options=\"a, b\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["\"s\"", "only `enum`"]);
}

#[test]
fn image_default_pointing_at_a_missing_file_fails_naming_the_path() {
    let v = check(&body_with(
        "variable id=\"hero\" type=\"image\" default=\"gone.png\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FILE", &["\"hero\"", "gone.png"]);
}

#[test]
fn image_default_with_a_present_file_is_accepted() {
    let v = check_with_files(
        &body_with("variable id=\"hero\" type=\"image\" default=\"photo.png\"\n"),
        &[("photo.png", &[0x89, b'P', b'N', b'G'])],
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[cfg(unix)]
#[test]
fn image_default_denied_by_permissions_is_unreadable_not_missing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmp("unreadable");
    let target = dir.join("photo.png");
    std::fs::write(&target, [0x89, b'P', b'N', b'G']).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
    // A privileged runner reads through 0o000; only assert the refusal
    // shape when the denial actually holds.
    if std::fs::File::open(&target).is_ok() {
        return;
    }
    write_comp(
        &dir,
        &body_with("variable id=\"hero\" type=\"image\" default=\"photo.png\"\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_UNREADABLE", &["\"hero\"", "photo.png"]);
}

#[test]
fn missing_id_and_type_are_refused() {
    let v = check(&body_with("variable type=\"string\" default=\"x\"\n"));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FIELD", &["`id`"]);

    let v = check(&body_with("variable id=\"x\" default=\"y\"\n"));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FIELD", &["\"x\"", "`type`"]);
}

#[test]
fn unquoted_label_is_refused() {
    let v = check(&body_with(
        "variable id=\"x\" type=\"string\" default=\"y\" label=5\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_LABEL", &["\"x\""]);
}

#[test]
fn unknown_key_and_children_are_refused() {
    let v = check(&body_with(
        "variable id=\"x\" type=\"string\" default=\"y\" bogus=1\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_UNKNOWN_KEY", &["\"bogus\""]);

    let v = check(&format!(
        "{OPEN}variable id=\"x\" type=\"string\" default=\"y\" {{\ntext value=\"Hi\" start=0 duration=1\n}}\n{CLIP}}}\n"
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["no children"]);
}
