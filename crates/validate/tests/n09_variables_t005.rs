//! Sprint 9 T005: reskin with named themes, through the public
//! [`validate_folder_with_variables_strict_themed`] host entry.
//!
//! Covers every acceptance criterion of
//! `T005-reskin-with-named-themes.md`: one selection re-skinning every
//! binding, CLI beating the composition attribute, override beating
//! theme, literal attributes beating both, defaults and fallbacks
//! filling silent gaps, two-theme and unknown-theme refusals,
//! through-nothing refusals, and theme durations/easings animating
//! exactly like literals.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{
    parse_variable_overrides, validate_folder_with_variables_strict_themed, Diagnostic, Status,
    VariableOverrides,
};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n09t005-{}-{}-{}",
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

fn overrides(json: &str) -> VariableOverrides {
    parse_variable_overrides(json).expect("test payload must parse")
}

fn empty_overrides() -> VariableOverrides {
    VariableOverrides::new()
}

fn check_themed(
    dir: &Path,
    overrides: &VariableOverrides,
    theme: Option<&str>,
) -> validate::validate::Verdict {
    validate_folder_with_variables_strict_themed(dir, overrides, false, theme)
}

fn check_themed_strict(
    dir: &Path,
    overrides: &VariableOverrides,
    theme: Option<&str>,
) -> validate::validate::Verdict {
    validate_folder_with_variables_strict_themed(dir, overrides, true, theme)
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

fn text_colors(comp: &notation::model::Composition) -> Vec<notation::model::Rgb> {
    comp.clips
        .iter()
        .map(|c| match c {
            notation::model::Clip::Text(t) => t.color,
            other => panic!("expected text clips, got {other:?}"),
        })
        .collect()
}

// --- happy paths ---------------------------------------------------------------

#[test]
fn one_theme_selection_reskins_every_binding() {
    let dir = tmp("reskin");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\ntext value=\"A\" start=0 duration=2 color=\"var(--accent)\"\ntext value=\"B\" start=0 duration=2 color=\"var(--accent)\"\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.unwrap();
    assert_eq!(
        text_colors(&comp),
        vec![
            notation::model::Rgb {
                r: 0xff,
                g: 0x33,
                b: 0x66
            },
            notation::model::Rgb {
                r: 0xff,
                g: 0x33,
                b: 0x66
            },
        ]
    );
}

#[test]
fn caller_theme_beats_the_composition_theme_attribute() {
    let dir = tmp("cli-beats-attr");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ntheme name=\"bold\" {\ntoken id=\"accent\" value=\"#00ff00\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\ntext value=\"A\" start=0 duration=2 color=\"var(--accent)\"\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), Some("bold"));
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.unwrap();
    assert_eq!(
        text_colors(&comp),
        vec![notation::model::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0x00
        }]
    );
}

#[test]
fn caller_override_beats_the_theme_token() {
    let dir = tmp("override-beats-theme");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\ntext value=\"A\" start=0 duration=2 color=\"var(--accent)\"\n}\n",
    );
    let v = check_themed(&dir, &overrides(r##"{"accent": "#00ff00"}"##), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.unwrap();
    assert_eq!(
        text_colors(&comp),
        vec![notation::model::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0x00
        }]
    );
}

#[test]
fn literal_element_attribute_beats_override_and_theme() {
    let dir = tmp("literal-wins");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\nvariable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2 color=\"var(--accent)\"\ntext value=\"Fixed\" start=0 duration=2 color=\"#123456\"\n}\n",
    );
    let v = check_themed(
        &dir,
        &overrides(r##"{"title": "Sale", "accent": "#00ff00"}"##),
        None,
    );
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.unwrap();
    match &comp.clips[0] {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.value, "Sale");
            assert_eq!(
                t.color,
                notation::model::Rgb {
                    r: 0x00,
                    g: 0xff,
                    b: 0x00
                }
            );
        }
        other => panic!("expected first clip to be text, got {other:?}"),
    }
    match &comp.clips[1] {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.value, "Fixed");
            assert_eq!(
                t.color,
                notation::model::Rgb {
                    r: 0x12,
                    g: 0x34,
                    b: 0x56
                }
            );
        }
        other => panic!("expected second clip to be text, got {other:?}"),
    }
}

#[test]
fn variable_default_fills_where_theme_and_caller_are_silent() {
    let body = "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\nvariable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2 color=\"var(--accent)\"\n}\n";
    // The theme is selected but holds no `title` token: the default wins.
    let dir = tmp("default-silent-theme");
    write_comp(&dir, body);
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.value, "News"),
        other => panic!("expected text, got {other:?}"),
    }
    // No theme at all: the default still wins (backward compatible).
    let dir = tmp("default-no-theme");
    write_comp(
        &dir,
        "composition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 {\nvariable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=2\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.value, "News"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn binding_fallback_fills_where_everything_else_is_silent() {
    let dir = tmp("fallback-silent");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late, FB)\" start=0 duration=2\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.value, "Hi FB"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn theme_token_resolves_with_no_variable_declaration() {
    let dir = tmp("token-only");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"greeting\" value=\"Hello\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"say var(--greeting)\" start=0 duration=2\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.value, "say Hello"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn theme_token_inside_a_nested_fallback_resolves() {
    let dir = tmp("nested-fallback-theme");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"tok\" value=\"T\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"Hi var(--a, var(--tok))\" start=0 duration=2\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.value, "Hi T"),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn theme_durations_and_easings_animate_like_literals() {
    let themed = "theme name=\"brand\" {\ntoken id=\"len\" value=1\ntoken id=\"amp\" value=\"1.5\"\ntoken id=\"ease\" value=\"in-out-quad\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"Hi\" start=0 duration=2\ntween target=0 property=\"x\" kind=\"to\" start=0 duration=\"var(--len)\" to=\"var(--amp)\" ease=\"var(--ease)\"\n}\n";
    let literal = "composition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 {\ntext value=\"Hi\" start=0 duration=2\ntween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=1.5 ease=\"in-out-quad\"\n}\n";
    let dir = tmp("motion-themed");
    write_comp(&dir, themed);
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let themed_comp = v.composition.unwrap();
    assert_eq!(themed_comp.tweens.len(), 1);
    assert_eq!(themed_comp.tweens[0].duration, 1.0);
    assert_eq!(themed_comp.tweens[0].to, Some(1.5));
    assert_eq!(
        themed_comp.tweens[0].ease,
        notation::model::EaseDecl::InOutQuad
    );
    let dir = tmp("motion-literal");
    write_comp(&dir, literal);
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let literal_comp = v.composition.unwrap();
    assert_eq!(
        themed_comp.tweens[0].duration,
        literal_comp.tweens[0].duration
    );
    assert_eq!(themed_comp.tweens[0].to, literal_comp.tweens[0].to);
    assert_eq!(themed_comp.tweens[0].ease, literal_comp.tweens[0].ease);
}

#[test]
fn strict_mode_stays_green_on_a_fully_bound_theme() {
    let dir = tmp("strict-theme");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\ntext value=\"A\" start=0 duration=2 color=\"var(--accent)\"\n}\n",
    );
    let v = check_themed_strict(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.variable_warnings.is_empty(), "{:?}", v.variable_warnings);
}

// --- refusal variants ----------------------------------------------------------

#[test]
fn selecting_two_themes_fails_naming_the_rule() {
    for attr in ["brand bold", "brand,bold"] {
        let dir = tmp("two-themes");
        write_comp(
            &dir,
            &format!("theme name=\"brand\" {{\ntoken id=\"accent\" value=\"#ff3366\"\n}}\ntheme name=\"bold\" {{\ntoken id=\"accent\" value=\"#00ff00\"\n}}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"{attr}\" {{\ntext value=\"A\" start=0 duration=1\n}}\n"),
        );
        let v = check_themed(&dir, &empty_overrides(), None);
        assert_eq!(v.status, Status::Invalid, "attr {attr:?}");
        assert_refusal(
            &v.diagnostics,
            "E_BAD_VARIABLE",
            &["at most one theme", "\"brand\"", "\"bold\""],
        );
    }
}

#[test]
fn unknown_theme_in_the_attribute_fails_naming_the_available() {
    let dir = tmp("unknown-attr-theme");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"nope\" {\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["unknown theme", "\"nope\"", "available themes", "\"brand\""],
    );
}

#[test]
fn unknown_cli_theme_fails_naming_the_available() {
    let dir = tmp("unknown-cli-theme");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), Some("ghost"));
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["--theme", "\"ghost\"", "available themes", "\"brand\""],
    );
}

#[test]
fn unknown_cli_theme_with_no_theme_blocks_names_the_gap() {
    let dir = tmp("unknown-cli-no-blocks");
    write_comp(
        &dir,
        "composition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 {\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), Some("ghost"));
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["--theme", "\"ghost\"", "available themes", "none"],
    );
}

#[test]
fn var_resolving_through_nothing_fails_naming_the_token_and_sources() {
    // Theme selected, but it holds no token for `--x`.
    let dir = tmp("nothing-selected");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"Hi var(--x)\" start=0 duration=2\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["--x", "token", "variable", "fallback"],
    );
    // No theme at all: the same call names the same sources.
    let dir = tmp("nothing-plain");
    write_comp(
        &dir,
        "composition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 {\ntext value=\"Hi var(--x)\" start=0 duration=2\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["--x", "token", "variable", "fallback"],
    );
}

#[test]
fn default_less_variable_with_a_silent_theme_still_needs_a_value() {
    let dir = tmp("noval-silent-theme");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\nvariable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late)\" start=0 duration=2\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["--late", "`default`", "token", "fallback"],
    );
}

#[test]
fn var_in_the_theme_attribute_is_not_a_theme_name() {
    let dir = tmp("var-in-theme-attr");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"var(--t)\" {\nvariable id=\"t\" type=\"string\" default=\"brand\"\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["unknown theme", "var(--t)", "available themes"],
    );
}

#[test]
fn duplicate_theme_names_refuse_naming_the_first() {
    let dir = tmp("dup-theme");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ntheme name=\"brand\" {\ntoken id=\"ink\" value=\"#111111\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"A\" start=0 duration=1 color=\"var(--ink, #222222)\"\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["duplicate theme", "\"brand\""],
    );
}

#[test]
fn duplicate_token_ids_refuse_naming_the_theme() {
    let dir = tmp("dup-token");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\ntoken id=\"accent\" value=\"#00ff00\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["duplicate token", "--accent", "\"brand\""],
    );
}

#[test]
fn token_without_a_value_is_refused() {
    let dir = tmp("token-no-value");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"accent\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(&v.diagnostics, "E_MISSING_FIELD", &["--accent", "`value`"]);
}

#[test]
fn non_token_child_inside_a_theme_is_refused() {
    let dir = tmp("theme-bad-child");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntext value=\"A\" start=0 duration=1\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["`text`", "\"brand\"", "token"],
    );
}

#[test]
fn theme_without_a_name_is_refused() {
    let dir = tmp("theme-no-name");
    write_comp(
        &dir,
        "theme {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 {\ntext value=\"A\" start=0 duration=1\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_refusal(&v.diagnostics, "E_MISSING_FIELD", &["`name`", "theme"]);
}

#[test]
fn bare_scalar_token_values_substitute_verbatim() {
    let dir = tmp("bare-token");
    write_comp(
        &dir,
        "theme name=\"brand\" {\ntoken id=\"n\" value=2\ntoken id=\"flag\" value=true\n}\ncomposition name=\"main\" width=640 height=480 duration=2 background=\"#000000\" version=1 theme=\"brand\" {\ntext value=\"n=var(--n)\" start=0 duration=2 hidden=\"var(--flag)\" order=\"var(--n)\"\n}\n",
    );
    let v = check_themed(&dir, &empty_overrides(), None);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.value, "n=2");
            assert!(t.hidden);
            assert_eq!(t.order, 2);
        }
        other => panic!("expected text, got {other:?}"),
    }
}
