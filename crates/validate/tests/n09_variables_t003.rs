//! Sprint 9 T003: override values per render, through the public
//! [`validate_folder_with_variables`] host entry plus the
//! [`parse_variable_overrides`] CLI payload parser.
//!
//! Covers every acceptance criterion of
//! `T003-override-values-per-render.md`: override substitution,
//! defaults kept for unmentioned bindings, disk unchanged,
//! determinism, unknown-key warnings, both shape refusals, and
//! overridden-composition validation.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{
    parse_variable_overrides, validate_folder, validate_folder_with_variables, Diagnostic, Status,
    VariableOverrides,
};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n09t003-{}-{}-{}",
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

fn check_with(dir: &Path, json: &str) -> validate::validate::Verdict {
    validate_folder_with_variables(dir, &overrides(json))
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

fn first_text_value(body: &str, json: &str) -> String {
    let dir = tmp("case");
    write_comp(&dir, body);
    let v = check_with(&dir, json);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => t.value.clone(),
        other => panic!("expected first clip to be text, got {other:?}"),
    }
}

// --- happy paths -------------------------------------------------------------

#[test]
fn override_replaces_the_bound_value_while_others_keep_defaults() {
    let got = first_text_value(
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"sub\" type=\"string\" default=\"keep\"\ntext value=\"var(--title) | var(--sub)\" start=0 duration=2\n",
        ),
        r##"{"title": "Sale"}"##,
    );
    assert_eq!(got, "Sale | keep");
}

#[test]
fn scalar_overrides_coerce_to_each_type() {
    let dir = tmp("coerce");
    write_comp(
        &dir,
        &format!(
            "{OPEN}variable id=\"n\" type=\"number\" default=1\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\nvariable id=\"flag\" type=\"boolean\" default=false\nvariable id=\"sz\" type=\"enum\" options=\"left, right\" default=\"left\"\ntext value=\"Hi\" start=0 duration=2 x=\"var(--n)\" color=\"var(--accent)\" hidden=\"var(--flag)\" align=\"var(--sz)\"\n}}\n"
        ),
    );
    let v = check_with(
        &dir,
        r##"{"n": 2.5, "accent": "#ff3366", "flag": true, "sz": "right"}"##,
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.variable_warnings.is_empty(), "{:?}", v.variable_warnings);
    let comp = v.composition.unwrap();
    match &comp.clips[0] {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.x, 2.5);
            assert_eq!(
                t.color,
                notation::model::Rgb {
                    r: 0xff,
                    g: 0x33,
                    b: 0x66
                }
            );
            assert!(t.hidden);
            assert_eq!(t.align, notation::model::Align::Right);
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn number_looking_string_overrides_stay_verbatim_for_string_variables() {
    let got = first_text_value(
        &body_with(
            "variable id=\"code\" type=\"string\" default=\"x\"\ntext value=\"n=var(--code)\" start=0 duration=2\n",
        ),
        r##"{"code": 5}"##,
    );
    assert_eq!(got, "n=5");
}

#[test]
fn quoted_spellings_match_kdl_defaults_for_number_and_boolean() {
    let got = first_text_value(
        &body_with(
            "variable id=\"n\" type=\"number\" default=1\ntext value=\"n=var(--n)\" start=0 duration=2 x=\"var(--n)\"\n",
        ),
        r##"{"n": "2.5"}"##,
    );
    assert_eq!(got, "n=2.5");
    let got = first_text_value(
        &body_with(
            "variable id=\"f\" type=\"boolean\" default=false\ntext value=\"v=var(--f)\" start=0 duration=2 hidden=\"var(--f)\"\n",
        ),
        r##"{"f": "true"}"##,
    );
    assert_eq!(got, "v=true");
}

#[test]
fn default_less_variable_is_supplied_by_the_override() {
    let got = first_text_value(
        &body_with(
            "variable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late)\" start=0 duration=2\n",
        ),
        r##"{"late": "there"}"##,
    );
    assert_eq!(got, "Hi there");
}

#[test]
fn image_override_plays_the_substituted_file() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\n";
    let dir = tmp("img-override");
    write_comp(
        &dir,
        &format!(
            "{OPEN}variable id=\"hero\" type=\"image\" default=\"other.png\"\nimage path=\"var(--hero)\" start=0 duration=1\n}}\n"
        ),
    );
    std::fs::write(dir.join("hero.png"), png).unwrap();
    std::fs::write(dir.join("other.png"), png).unwrap();
    let v = check_with(&dir, r##"{"hero": "hero.png"}"##);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Image(img) => assert_eq!(img.path, "hero.png"),
        other => panic!("expected image, got {other:?}"),
    }
}

#[test]
fn empty_object_keeps_every_default_with_no_warnings() {
    let dir = tmp("empty");
    // The declaration stays bound (Sprint 9 T004 warns on
    // declared-but-never-bound variables even with no overrides), so an
    // empty object is warning-free only when every variable is used.
    write_comp(
        &dir,
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
    );
    let v = check_with(&dir, "{}");
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.variable_warnings.is_empty());
    let plain = validate_folder(&dir);
    assert_eq!(plain.status, Status::Valid);
    assert_eq!(
        plain.composition.unwrap().clips.len(),
        v.composition.unwrap().clips.len()
    );
}

#[test]
fn same_overrides_twice_resolve_identically_and_touch_no_disk() {
    let body = body_with(
        "variable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2\n",
    );
    let dir = tmp("deterministic");
    write_comp(&dir, &body);
    let before = std::fs::read(dir.join("composition.kdl")).unwrap();
    let first = check_with(&dir, r##"{"title": "Sale"}"##);
    let second = check_with(&dir, r##"{"title": "Sale"}"##);
    assert_eq!(first.status, Status::Valid, "{:?}", first.diagnostics);
    assert_eq!(second.status, Status::Valid, "{:?}", second.diagnostics);
    let a = format!("{:?}", first.composition.unwrap().clips);
    let b = format!("{:?}", second.composition.unwrap().clips);
    assert_eq!(a, b, "same overrides must resolve identically");
    assert!(a.contains("Sale"), "{a}");
    let after = std::fs::read(dir.join("composition.kdl")).unwrap();
    assert_eq!(
        before, after,
        "overridden validation must not write the project"
    );
}

#[test]
fn different_overrides_resolve_differently() {
    let body = body_with(
        "variable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2\n",
    );
    let dir = tmp("variants");
    write_comp(&dir, &body);
    let dir2 = tmp("variants2");
    write_comp(&dir2, &body);
    let a = check_with(&dir, r##"{"title": "One"}"##);
    let b = check_with(&dir2, r##"{"title": "Two"}"##);
    let ta = format!("{:?}", a.composition.unwrap().clips);
    let tb = format!("{:?}", b.composition.unwrap().clips);
    assert!(ta.contains("One") && !ta.contains("Two"), "{ta}");
    assert!(tb.contains("Two") && !tb.contains("One"), "{tb}");
}

// --- unknown keys warn visibly without blocking --------------------------------

#[test]
fn unknown_override_key_warns_without_blocking() {
    let dir = tmp("unknown-key");
    write_comp(
        &dir,
        &body_with(
            "variable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2\n",
        ),
    );
    let v = check_with(&dir, r##"{"title": "Sale", "ghost": "x"}"##);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.variable_warnings.len(), 1, "{:?}", v.variable_warnings);
    let w = &v.variable_warnings[0];
    assert_eq!(w.key, "ghost");
    assert!(w.message.contains("--ghost"), "{}", w.message);
    assert!(!w.fix.is_empty(), "warning must name the fix");
    // The known override still applies.
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.value, "Sale"),
        other => panic!("expected text, got {other:?}"),
    }
    // The report line is visible: it starts with `warning:`, names the
    // key, and stays on the zero-exit-code path (`check` prints it via
    // `format_check_human`; the binary end-to-end test asserts that).
    let line = format!("{}", v.variable_warnings[0]);
    assert!(line.starts_with("warning:"), "{line}");
    assert!(line.contains("--ghost"), "{line}");
    assert!(line.contains("(fix:"), "{line}");
}

// --- shape refusals name the flag and the expected shape ------------------------

#[test]
fn non_object_payloads_fail_naming_the_flag_and_shape() {
    for raw in ["[1, 2]", "\"title\"", "42", "true", "null", "{bad json", ""] {
        let err = parse_variable_overrides(raw).expect_err(&format!("{raw:?} must refuse"));
        assert!(err.contains("--variables"), "{raw:?}: {err}");
        assert!(
            err.contains("JSON object") || err.contains("valid JSON"),
            "{raw:?}: {err}"
        );
    }
}

#[test]
fn non_scalar_values_fail_naming_the_flag_key_and_shape() {
    for raw in [
        r##"{"title": {"a": 1}}"##,
        r##"{"title": ["a"]}"##,
        r##"{"title": null}"##,
    ] {
        let err = parse_variable_overrides(raw).expect_err(&format!("{raw:?} must refuse"));
        assert!(err.contains("--variables"), "{raw:?}: {err}");
        assert!(err.contains("\"title\""), "{raw:?}: {err}");
        assert!(err.contains("scalar"), "{raw:?}: {err}");
    }
}

// --- overridden composition validates --------------------------------------------

#[test]
fn mistyped_override_refuses_naming_the_variable_and_shape() {
    // Number variable given a non-numeric string.
    let dir = tmp("bad-num");
    write_comp(
        &dir,
        &body_with(
            "variable id=\"n\" type=\"number\" default=1\ntext value=\"Hi\" start=0 duration=2 x=\"var(--n)\"\n",
        ),
    );
    let v = check_with(&dir, r##"{"n": "abc"}"##);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--n", "--variables"]);

    // Color variable given a non-color.
    let dir = tmp("bad-color");
    write_comp(
        &dir,
        &body_with(
            "variable id=\"accent\" type=\"color\" default=\"#000000\"\ntext value=\"Hi\" start=0 duration=2 color=\"var(--accent)\"\n",
        ),
    );
    let v = check_with(&dir, r##"{"accent": "red"}"##);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--accent", "#rrggbb"]);

    // Boolean variable given something outside true/false.
    let dir = tmp("bad-bool");
    write_comp(
        &dir,
        &body_with(
            "variable id=\"f\" type=\"boolean\" default=false\ntext value=\"Hi\" start=0 duration=2 hidden=\"var(--f)\"\n",
        ),
    );
    let v = check_with(&dir, r##"{"f": "yes"}"##);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["--f", "`true`", "`false`"],
    );

    // Enum variable given a value outside its options.
    let dir = tmp("bad-enum");
    write_comp(
        &dir,
        &body_with(
            "variable id=\"sz\" type=\"enum\" options=\"left, right\" default=\"left\"\ntext value=\"Hi\" start=0 duration=2 align=\"var(--sz)\"\n",
        ),
    );
    let v = check_with(&dir, r##"{"sz": "LEFT"}"##);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["--sz", "options"]);
}

#[test]
fn missing_image_override_is_still_a_missing_file_naming_the_variable() {
    let dir = tmp("bad-img");
    write_comp(
        &dir,
        &body_with(
            "variable id=\"hero\" type=\"image\" default=\"hero.png\"\nimage path=\"var(--hero)\" start=0 duration=1\n",
        ),
    );
    std::fs::write(dir.join("hero.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    let v = check_with(&dir, r##"{"hero": "gone.png"}"##);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FILE", &["--hero", "gone.png"]);
}
