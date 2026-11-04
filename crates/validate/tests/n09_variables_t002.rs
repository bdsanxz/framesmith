//! Sprint 9 T002: `var(--id)` bindings, through the public
//! [`validate_folder`] API — the same verdict `check` reports, with
//! the resolved model asserted on the composition.
//!
//! Covers every acceptance criterion of
//! `T002-bind-values-with-var-references.md`: text substitution,
//! media `src` substitution, numeric coercion, motion value bindings,
//! fallbacks, and every binding-level refusal variant naming the
//! attribute and the variable.

use notation::model::{Align, Clip, Rgb};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n09t002-{}-{}-{}",
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

/// The refusal names both the attribute and the variable, whatever the
/// code (shape failures keep their natural attribute code with the
/// variable amended in).
fn assert_names_both(ds: &[Diagnostic], attr: &str, var: &str) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    let ms = messages(ds);
    assert!(
        ms.iter().any(|m| m.contains(attr) && m.contains(var)),
        "expected one message naming attribute {attr:?} and variable {var:?} in {ms:?}"
    );
}

fn valid_comp(body: &str) -> notation::model::Composition {
    let v = check(body);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.diagnostics.is_empty(), "{:?}", v.diagnostics);
    v.composition.unwrap()
}

fn first_text(comp: &notation::model::Composition) -> notation::model::TextClip {
    match &comp.clips[0] {
        Clip::Text(t) => t.clone(),
        other => panic!("expected first clip to be text, got {other:?}"),
    }
}

// --- happy paths -------------------------------------------------------------

#[test]
fn text_binding_renders_the_substituted_string_verbatim() {
    let comp = valid_comp(&body_with(
        "variable id=\"title\" type=\"string\" default=\"Héllo — 100% (x)\"\ntext value=\"Big var(--title)! day\" start=0 duration=2\n",
    ));
    assert_eq!(first_text(&comp).value, "Big Héllo — 100% (x)! day");
}

#[test]
fn number_variable_into_text_stays_verbatim_text() {
    let comp = valid_comp(&body_with(
        "variable id=\"n\" type=\"number\" default=1.5\ntext value=\"count var(--n)\" start=0 duration=2\n",
    ));
    assert_eq!(first_text(&comp).value, "count 1.5");
}

#[test]
fn several_bindings_in_one_value_all_substitute() {
    let comp = valid_comp(&body_with(
        "variable id=\"a\" type=\"string\" default=\"red\"\nvariable id=\"b\" type=\"string\" default=\"roses\"\ntext value=\"var(--a) var(--b)\" start=0 duration=2\n",
    ));
    assert_eq!(first_text(&comp).value, "red roses");
}

#[test]
fn media_src_binding_plays_the_substituted_file() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\n";
    let dir = tmp("media-src");
    write_comp(
        &dir,
        &format!(
            "{OPEN}variable id=\"hero\" type=\"image\" default=\"photo.png\"\nvariable id=\"clip\" type=\"string\" default=\"clip.mp4\"\nimage path=\"var(--hero)\" start=0 duration=1\nvideo path=\"var(--clip)\" start=0 duration=1\n}}\n"
        ),
    );
    std::fs::write(dir.join("photo.png"), png).unwrap();
    std::fs::write(dir.join("clip.mp4"), b"").unwrap();
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.unwrap();
    match &comp.clips[0] {
        Clip::Image(img) => assert_eq!(img.path, "photo.png"),
        other => panic!("expected image clip, got {other:?}"),
    }
    match &comp.clips[1] {
        Clip::Video(vid) => assert_eq!(vid.path, "clip.mp4"),
        other => panic!("expected video clip, got {other:?}"),
    }
}

#[test]
fn numeric_attribute_binding_coerces_and_affects_layout() {
    let comp = valid_comp(&body_with(
        "variable id=\"n\" type=\"number\" default=1.5\nvariable id=\"k\" type=\"number\" default=2\ntext value=\"Hi\" start=0 duration=2 x=\"var(--n)\" order=\"var(--k)\"\n",
    ));
    let t = first_text(&comp);
    assert_eq!(t.x, 1.5);
    assert_eq!(t.order, 2);
}

#[test]
fn motion_value_binding_animates_from_the_substituted_number() {
    let comp = valid_comp(&body_with(
        "variable id=\"n\" type=\"number\" default=1.5\nvariable id=\"k\" type=\"number\" default=0\ntext value=\"Hi\" start=0 duration=2\ntween target=\"var(--k)\" property=\"x\" kind=\"to\" start=0 duration=1 to=\"var(--n)\" ease=\"linear\"\nset target=0 property=\"y\" start=0 duration=1 value=\"var(--n)\"\nkey target=0 property=\"x\" {\nframe at=0 value=\"var(--k)\"\nframe at=1 value=\"var(--n)\"\n}\n",
    ));
    assert_eq!(comp.tweens.len(), 1);
    assert_eq!(comp.tweens[0].to, Some(1.5));
    assert_eq!(comp.tweens[0].target, 0);
    assert_eq!(comp.sets.len(), 1);
    assert_eq!(comp.sets[0].value, 1.5);
    assert_eq!(comp.keys.len(), 1);
    assert_eq!(comp.keys[0].stops.len(), 2);
    assert_eq!(comp.keys[0].stops[0].value, 0.0);
    assert_eq!(comp.keys[0].stops[1].value, 1.5);
}

#[test]
fn color_motion_binding_paints_the_substituted_color() {
    let dir = tmp("color-motion");
    write_comp(
        &dir,
        &format!(
            "{OPEN}variable id=\"accent\" type=\"color\" default=\"#ff3366\"\nbox x=0 y=0 width=8 height=8 fill=\"#000000\" start=0 duration=2\ntween target=0 property=\"fill\" kind=\"to\" start=0 duration=1 to=\"var(--accent)\" ease=\"linear\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.unwrap();
    assert_eq!(
        comp.tweens[0].to_color,
        Some(Rgb {
            r: 0xff,
            g: 0x33,
            b: 0x66
        })
    );
}

#[test]
fn fallback_renders_when_the_variable_is_unknown() {
    let comp = valid_comp(&body_with(
        "text value=\"var(--ghost, Hello)\" start=0 duration=2 x=\"var(--ghost, 3)\"\n",
    ));
    let t = first_text(&comp);
    assert_eq!(t.value, "Hello");
    assert_eq!(t.x, 3.0);
}

#[test]
fn nested_fallback_resolves_the_inner_binding() {
    let comp = valid_comp(&body_with(
        "variable id=\"b\" type=\"string\" default=\"B\"\ntext value=\"var(--a, var(--b))\" start=0 duration=2\n",
    ));
    assert_eq!(first_text(&comp).value, "B");
}

#[test]
fn boolean_binding_accepts_true_and_false_spellings() {
    let comp = valid_comp(&body_with(
        "variable id=\"show\" type=\"boolean\" default=true\nvariable id=\"hide\" type=\"boolean\" default=false\ntext value=\"A\" start=0 duration=2 hidden=\"var(--show)\"\ntext value=\"B\" start=0 duration=2 hidden=\"var(--hide)\"\ntext value=\"C\" start=0 duration=2 hidden=\"var(--ghost, true)\"\n",
    ));
    let hidden: Vec<bool> = comp
        .clips
        .iter()
        .map(|c| match c {
            Clip::Text(t) => t.hidden,
            _ => panic!("expected text"),
        })
        .collect();
    // The shared trailing clip (`body_with`) carries no binding and
    // stays visible.
    assert_eq!(hidden, vec![true, false, true, false]);
}

#[test]
fn enum_binding_matches_options_exactly() {
    let comp = valid_comp(&body_with(
        "variable id=\"sz\" type=\"enum\" options=\"left, right\" default=\"right\"\nvariable id=\"brand\" type=\"font\" default=\"Family, Fallback\"\ntext value=\"Hi\" start=0 duration=2 align=\"var(--sz)\" font-family=\"var(--brand)\"\n",
    ));
    let t = first_text(&comp);
    assert_eq!(t.align, Align::Right);
    assert_eq!(
        t.font_family,
        Some(vec!["Family".to_string(), "Fallback".to_string()])
    );
}

#[test]
fn default_less_variable_with_a_fallback_renders_the_fallback() {
    let comp = valid_comp(&body_with(
        "variable id=\"late\" type=\"string\"\ntext value=\"var(--late, FB)\" start=0 duration=2\n",
    ));
    assert_eq!(first_text(&comp).value, "FB");
}

#[test]
fn forward_reference_resolves_when_the_declaration_follows_the_use() {
    let comp = valid_comp(&body_with(
        "text value=\"Hi var(--title)\" start=0 duration=2\nvariable id=\"title\" type=\"string\" default=\"there\"\n",
    ));
    assert_eq!(first_text(&comp).value, "Hi there");
}

#[test]
fn composition_background_binding_paints_the_substituted_color() {
    let dir = tmp("background");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=1 background=\"var(--accent)\" version=1 {\nvariable id=\"accent\" type=\"color\" default=\"#ff3366\"\ntext value=\"Hi\" start=0 duration=1\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(
        v.composition.unwrap().background,
        Rgb {
            r: 0xff,
            g: 0x33,
            b: 0x66
        }
    );
}

#[test]
fn variable_defaults_stay_literal_and_glued_var_text_is_untouched() {
    // A default containing `var(` is the resolution source, never a
    // binding; `covar(` (glued to an identifier) is literal text.
    let comp = valid_comp(&body_with(
        "variable id=\"a\" type=\"string\" default=\"say var(--b)\"\ntext value=\"covar(--b)\" start=0 duration=2\n",
    ));
    assert_eq!(
        comp.variables[0].default,
        Some(notation::model::VariableDefault::Str(
            "say var(--b)".to_string()
        ))
    );
    assert_eq!(first_text(&comp).value, "covar(--b)");
}

// --- refusal variants ----------------------------------------------------------

#[test]
fn unknown_variable_without_fallback_fails_naming_both() {
    let v = check(&body_with(
        "text value=\"Hi var(--ghost)\" start=0 duration=2\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["value", "--ghost"]);
}

#[test]
fn default_less_variable_without_fallback_is_refused() {
    let v = check(&body_with(
        "variable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late)\" start=0 duration=2\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_VARIABLE",
        &["value", "--late", "`default`", "fallback"],
    );
}

#[test]
fn substitution_that_does_not_coerce_fails_naming_both() {
    // Number attribute bound to a non-numeric string.
    let v = check(&body_with(
        "variable id=\"title\" type=\"string\" default=\"abc\"\ntext value=\"Hi\" start=0 duration=2 x=\"var(--title)\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_names_both(&v.diagnostics, "x", "--title");

    // Color attribute bound to a number.
    let v = check(&body_with(
        "variable id=\"n\" type=\"number\" default=1.5\ntext value=\"Hi\" start=0 duration=2 color=\"var(--n)\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_names_both(&v.diagnostics, "color", "--n");
}

#[test]
fn boolean_binding_refuses_anything_outside_true_false() {
    let v = check(&body_with(
        "text value=\"Hi\" start=0 duration=2 hidden=\"var(--show, yes)\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(
        &v.diagnostics,
        "E_BAD_HIDDEN",
        &["hidden", "`true`", "`false`"],
    );
    assert_names_both(&v.diagnostics, "hidden", "--show");
}

#[test]
fn enum_binding_is_exact_and_case_sensitive() {
    // `Left` is not `left`: the resolved option must match exactly.
    let v = check(&body_with(
        "variable id=\"e\" type=\"enum\" options=\"Left, left\" default=\"Left\"\ntext value=\"Hi\" start=0 duration=2 align=\"var(--e)\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_names_both(&v.diagnostics, "align", "--e");

    // A fallback outside the attribute's accepted set refuses too.
    let v = check(&body_with(
        "text value=\"Hi\" start=0 duration=2 align=\"var(--ghost, LEFT)\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_names_both(&v.diagnostics, "align", "--ghost");
}

#[test]
fn bad_var_syntax_fails_naming_the_attribute() {
    for bad in [
        "var(--title",
        "var()",
        "var(title)",
        "var(--9x)",
        "var(--x,)",
        "var(--title, var(--oops)",
    ] {
        let v = check(&body_with(&format!(
            "variable id=\"title\" type=\"string\" default=\"T\"\ntext value=\"Hi {bad}\" start=0 duration=2\n"
        )));
        assert_eq!(v.status, Status::Invalid, "value {bad:?}");
        assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["value", "var()"]);
    }
}

#[test]
fn var_inside_id_track_or_start_after_is_refused() {
    // `id` carries one clean diagnostic through the placeholder.
    let v = check(&body_with(
        "text value=\"Hi\" start=0 duration=2 id=\"var(--t)\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["`id`", "--t"]);

    // `track` likewise.
    let v = check(&body_with(
        "text value=\"Hi\" start=0 duration=2 track=\"var(--t)\"\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["track", "--t"]);

    // `start-after` targets stay literal; the composition itself stands
    // in as the benign placeholder so only the binding refuses.
    let dir = tmp("start-after-var");
    write_comp(
        &dir,
        &format!(
            "{OPEN}variable id=\"t\" type=\"string\" default=\"a\"\ntext value=\"A\" start=0 duration=1 id=\"a\"\ntext value=\"B\" start-after=\"var(--t)\" duration=1\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["start-after", "--t"]);
}

#[test]
fn var_inside_composition_timing_foundation_is_refused() {
    // One composition per key, each carrying exactly one diagnostic.
    // The clip stays inside even the shortened placeholder duration.
    let heads = [
        (
            "name",
            "composition name=\"var(--n)\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n",
        ),
        (
            "width",
            "composition name=\"main\" width=\"var(--n)\" height=480 duration=2 background=\"#000\" version=1 {\n",
        ),
        (
            "height",
            "composition name=\"main\" width=640 height=\"var(--n)\" duration=2 background=\"#000\" version=1 {\n",
        ),
        (
            "duration",
            "composition name=\"main\" width=640 height=480 duration=\"var(--n)\" background=\"#000\" version=1 {\n",
        ),
        (
            "fps",
            "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 fps=\"var(--n)\" {\n",
        ),
    ];
    for (key, head) in heads {
        let dir = tmp("comp-key");
        write_comp(
            &dir,
            &format!(
                "{head}variable id=\"n\" type=\"number\" default=1\ntext value=\"Hi\" start=0 duration=1\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "key {key}");
        assert_eq!(v.diagnostics.len(), 1, "key {key}: {:?}", v.diagnostics);
        assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &[key, "--n"]);
    }
}

#[test]
fn substituted_missing_file_is_still_a_missing_file_naming_the_variable() {
    let v = check(&body_with(
        "variable id=\"hero\" type=\"string\" default=\"gone.png\"\nimage path=\"var(--hero)\" start=0 duration=1\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FILE", &["gone.png"]);
    assert_names_both(&v.diagnostics, "path", "--hero");
}

#[test]
fn unknown_variable_inside_a_fallback_names_the_inner_variable() {
    let v = check(&body_with(
        "text value=\"Hi var(--a, var(--b))\" start=0 duration=2\n",
    ));
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_VARIABLE", &["value", "--b"]);
}
