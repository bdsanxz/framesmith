//! Sprint 9 T004: strict variable validation on demand, through the
//! [`validate_folder_with_variables_strict`] entry.
//!
//! Covers every acceptance criterion of
//! `T004-validate-strictly-on-demand.md`: default-mode warnings that
//! never block (unknown override keys, declared-but-never-bound
//! variables), strict-mode refusals naming the variable or key and the
//! fix (unknown key, unused declaration, default-less variable without
//! a value), and a strict run with everything supplied rendering clean.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{
    parse_variable_overrides, validate_folder_with_variables_strict, Diagnostic, Status,
    VariableOverrides,
};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n09t004-{}-{}-{}",
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

fn overrides(json: &str) -> VariableOverrides {
    parse_variable_overrides(json).expect("test payload must parse")
}

fn check(body: &str, json: &str, strict: bool) -> validate::validate::Verdict {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder_with_variables_strict(&dir, &overrides(json), strict)
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn messages(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.message.to_string()).collect()
}

fn fixes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.fix.to_string()).collect()
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

fn first_text_value(body: &str, json: &str, strict: bool) -> String {
    let v = check(body, json, strict);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => t.value.clone(),
        other => panic!("expected first clip to be text, got {other:?}"),
    }
}

// --- default mode warns without blocking -------------------------------------

#[test]
fn default_mode_renders_with_defaults_while_warning_on_unknown_key() {
    let v = check(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
        r##"{"title": "Sale", "ghost": "x"}"##,
        false,
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.variable_warnings.len(), 1, "{:?}", v.variable_warnings);
    let w = &v.variable_warnings[0];
    assert_eq!(w.key, "ghost");
    assert!(w.message.contains("--ghost"), "{}", w.message);
    assert!(!w.fix.is_empty(), "warning must name the fix");
    // The known override still applies; the unknown value is ignored.
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.value, "Sale"),
        other => panic!("expected text, got {other:?}"),
    }
    let line = format!("{}", v.variable_warnings[0]);
    assert!(line.starts_with("warning:"), "{line}");
    assert!(line.contains("--ghost"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn default_mode_renders_while_warning_on_unused_declaration() {
    let v = check(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"stale\" type=\"number\" default=1\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
        "{}",
        false,
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.variable_warnings.len(), 1, "{:?}", v.variable_warnings);
    let w = &v.variable_warnings[0];
    assert_eq!(w.key, "stale");
    assert!(w.message.contains("--stale"), "{}", w.message);
    assert!(!w.fix.is_empty(), "warning must name the fix");
    let line = format!("{w}");
    assert!(line.starts_with("warning:"), "{line}");
    assert!(line.contains("--stale"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

#[test]
fn default_mode_warnings_never_block_and_name_each_key_or_variable() {
    let got = first_text_value(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"stale\" type=\"string\" default=\"old\"\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
        r##"{"ghost": "x"}"##,
        false,
    );
    assert_eq!(got, "News");
    let v = check(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"stale\" type=\"string\" default=\"old\"\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
        r##"{"ghost": "x"}"##,
        false,
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.variable_warnings.len(), 2, "{:?}", v.variable_warnings);
    let keys: Vec<&str> = v.variable_warnings.iter().map(|w| w.key.as_str()).collect();
    assert!(keys.contains(&"ghost"), "{keys:?}");
    assert!(keys.contains(&"stale"), "{keys:?}");
    // The render proceeds: no diagnostics, overrides applied, file kept.
    assert!(v.diagnostics.is_empty(), "{:?}", v.diagnostics);
}

#[test]
fn default_mode_bound_with_fallback_resolves_quietly_without_warnings() {
    // A default-less variable that is bound (via an inline fallback) is
    // neither unknown nor unused, so default mode stays silent.
    let got = first_text_value(
        &body_with(
            "variable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late, friend)\" start=0 duration=2\n",
        ),
        "{}",
        false,
    );
    assert_eq!(got, "Hi friend");
    let v = check(
        &body_with(
            "variable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late, friend)\" start=0 duration=2\n",
        ),
        "{}",
        false,
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.variable_warnings.is_empty(), "{:?}", v.variable_warnings);
}

#[test]
fn default_mode_keeps_hard_binding_errors_free_of_strict_wording() {
    // A default-less variable used without a fallback or value still
    // refuses through the binding pass in both modes (T002 behavior);
    // default mode must not dress it in strict wording.
    let v = check(
        &body_with(
            "variable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late)\" start=0 duration=2\n",
        ),
        "{}",
        false,
    );
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--late"]);
    assert!(
        !messages(&v.diagnostics)
            .iter()
            .any(|m| m.contains("--strict-variables")),
        "{:?}",
        v.diagnostics
    );
}

// --- strict mode refuses naming the key or variable and the fix --------------

#[test]
fn strict_mode_fails_unknown_override_key_naming_key_and_fix() {
    let v = check(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
        r##"{"title": "Sale", "ghost": "x"}"##,
        true,
    );
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--ghost"]);
    assert!(v.variable_warnings.is_empty(), "{:?}", v.variable_warnings);
    let all_fixes = fixes(&v.diagnostics).join("\n");
    assert!(all_fixes.contains("remove the key"), "{all_fixes}");
    assert!(all_fixes.contains("--strict-variables"), "{all_fixes}");
}

#[test]
fn strict_mode_fails_unused_declaration_naming_variable_and_fix() {
    let v = check(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"stale\" type=\"number\" default=1\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
        "{}",
        true,
    );
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--stale"]);
    assert!(v.variable_warnings.is_empty(), "{:?}", v.variable_warnings);
    let all_fixes = fixes(&v.diagnostics).join("\n");
    assert!(all_fixes.contains("bind the variable"), "{all_fixes}");
    assert!(all_fixes.contains("--strict-variables"), "{all_fixes}");
}

#[test]
fn strict_mode_fails_default_less_variable_without_value() {
    // Bound only through an inline fallback, so the unused rule stays
    // quiet and the single diagnostic is the missing-value refusal.
    let v = check(
        &body_with(
            "variable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late, friend)\" start=0 duration=2\n",
        ),
        "{}",
        true,
    );
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--late"]);
    let all_fixes = fixes(&v.diagnostics).join("\n");
    assert!(all_fixes.contains("supply the value"), "{all_fixes}");
    assert!(all_fixes.contains("--strict-variables"), "{all_fixes}");
}

#[test]
fn strict_mode_supplied_but_unbound_variable_still_fails_as_unused() {
    // A caller value alone does not bind the variable: strict still
    // requires every declaration to be referenced by `var(--id)`.
    let v = check(
        &body_with("variable id=\"stale\" type=\"string\" default=\"old\"\n"),
        r##"{"stale": "new"}"##,
        true,
    );
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--stale"]);
}

#[test]
fn strict_mode_reports_every_problem_together() {
    // One unknown key, one unused declaration, one default-less
    // variable without a value: all three refuse before frame one.
    let v = check(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"stale\" type=\"string\" default=\"old\"\nvariable id=\"late\" type=\"string\"\ntext value=\"var(--title) var(--late, friend)\" start=0 duration=2\n",
        ),
        r##"{"ghost": "x"}"##,
        true,
    );
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 3, "{:?}", v.diagnostics);
    for d in &v.diagnostics {
        assert_eq!(d.code, "E_BAD_VARIABLE", "{d:?}");
    }
    let ms = messages(&v.diagnostics).join("\n");
    assert!(ms.contains("--ghost"), "{ms}");
    assert!(ms.contains("--stale"), "{ms}");
    assert!(ms.contains("--late"), "{ms}");
    // Every strict refusal names its fix.
    let all_fixes = fixes(&v.diagnostics).join("\n");
    assert!(all_fixes.contains("remove the key"), "{all_fixes}");
    assert!(all_fixes.contains("bind the variable"), "{all_fixes}");
    assert!(all_fixes.contains("supply the value"), "{all_fixes}");
    assert!(v.variable_warnings.is_empty(), "{:?}", v.variable_warnings);
}

#[test]
fn strict_mode_mistyped_override_keeps_its_single_type_refusal() {
    // A default-less variable with a mistyped override is already
    // refused at the declaration; strict must not add a second
    // missing-value diagnostic for the same variable.
    let v = check(
        &body_with(
            "variable id=\"n\" type=\"number\"\ntext value=\"n=var(--n, 0)\" start=0 duration=2\n",
        ),
        r##"{"n": "abc"}"##,
        true,
    );
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--n", "--variables"]);
}

// --- strict mode with everything supplied renders clean ----------------------

#[test]
fn strict_mode_with_all_values_supplied_and_every_variable_bound_renders_clean() {
    let got = first_text_value(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"late\" type=\"string\"\ntext value=\"var(--title) var(--late)\" start=0 duration=2\n",
        ),
        r##"{"title": "Sale", "late": "there"}"##,
        true,
    );
    assert_eq!(got, "Sale there");
    let v = check(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"late\" type=\"string\"\ntext value=\"var(--title) var(--late)\" start=0 duration=2\n",
        ),
        r##"{"title": "Sale", "late": "there"}"##,
        true,
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.diagnostics.is_empty(), "{:?}", v.diagnostics);
    assert!(v.variable_warnings.is_empty(), "{:?}", v.variable_warnings);
}
