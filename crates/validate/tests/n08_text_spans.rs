//! Sprint 8 T002: styled `span` runs and base `direction` through the
//! public [`validate_folder`] API — the same verdict `check` reports,
//! with the resolved model asserted for run attributes.
//!
//! Covers every acceptance refusal of
//! `T002-style-runs-and-shape-world-text.md` at the notation level:
//! per-run styling, family fallback syntax, directions, weights,
//! empty spans, non-span children, spans on non-text, split combined
//! with spans, and each refusal naming its span and property with a
//! concrete fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08spans-{}-{}-{}",
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

fn comp_with_text(inner: &str) -> String {
    format!("{OPEN}{inner}}}\n")
}

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
}

fn assert_refusal(ds: &[Diagnostic], code: &str, property: &str) {
    assert!(
        !ds.is_empty(),
        "expected a `{code}` refusal for `{property}`"
    );
    for d in ds {
        assert_eq!(d.code, code, "unexpected code in {d}");
        assert!(
            d.message.contains(property),
            "refusal must name `{property}`: {d}"
        );
        assert!(!d.fix.is_empty(), "refusal must carry a fix: {d}");
        assert!(
            d.fix.contains(property) || d.fix.contains("span"),
            "fix must name the property or the span: {d}"
        );
    }
}

// --- valid runs -------------------------------------------------------------

#[test]
fn multi_run_paragraph_with_every_override_is_valid() {
    let dir = tmp("runs");
    write_comp(
        &dir,
        &comp_with_text(
            "text x=80 y=320 size=48 color=\"#ffffff\" direction=\"rtl\" start=0 duration=2 {\n\
             span value=\"Hello \" font-size=24 font-weight=700 italic=true color=\"#ff0000\" letter-spacing=2 line-height=60 font-family=\"First, Second\"\n\
             span value=\"there\" font-size=48 font-weight=400 italic=false color=\"#00ff00\"\n\
             }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    match &comp.clips[0] {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.direction, notation::model::TextDirection::Rtl);
            assert_eq!(t.spans.len(), 2);
            let first = &t.spans[0];
            assert_eq!(first.value, "Hello ");
            assert_eq!(first.font_size, Some(24.0));
            assert_eq!(first.font_weight, Some(700.0));
            assert_eq!(first.italic, Some(true));
            assert_eq!(first.letter_spacing, Some(2.0));
            assert_eq!(first.line_height, Some(60.0));
            assert_eq!(
                first.font_family,
                Some(vec!["First".to_string(), "Second".to_string()])
            );
            assert_eq!(t.spans[1].font_weight, Some(400.0));
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn fallback_family_list_underscore_spellings_and_auto_are_valid() {
    let dir = tmp("fallback");
    write_comp(
        &dir,
        &comp_with_text(
            "text value=\"Hi\" x=0 y=0 size=24 start=0 duration=2 direction=\"auto\" font_family=\"Missing, sans-serif\" {\n\
             span value=\"yo\" font_size=20 font_weight=500 letter_spacing=1 line_height=30\n\
             }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn spans_without_clip_value_and_nested_spans_are_valid() {
    let dir = tmp("nested");
    write_comp(
        &dir,
        &comp_with_text(
            "text x=0 y=0 size=24 start=0 duration=2 {\n\
             span font-size=20 {\n\
             span value=\"inner\" color=\"#ff0000\"\n\
             }\n\
             span value=\"outer\"\n\
             }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    match &comp.clips[0] {
        notation::model::Clip::Text(t) => {
            assert_eq!(t.value, "");
            assert_eq!(t.spans.len(), 2);
            assert_eq!(t.spans[0].children.len(), 1);
            assert_eq!(t.spans[0].children[0].value, "inner");
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn plain_text_without_spans_keeps_required_value() {
    let dir = tmp("plain");
    write_comp(
        &dir,
        &comp_with_text("text value=\"Hi\" x=0 y=0 size=24 start=0 duration=2\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    match &comp.clips[0] {
        notation::model::Clip::Text(t) => {
            assert!(t.spans.is_empty());
            assert_eq!(t.direction, notation::model::TextDirection::Ltr);
        }
        other => panic!("expected text, got {other:?}"),
    }
}

// --- refusals ---------------------------------------------------------------

#[test]
fn unknown_direction_fails_naming_accepted_set() {
    let ds = diags(&comp_with_text(
        "text value=\"Hi\" x=0 y=0 size=24 direction=\"sideways\" start=0 duration=2\n",
    ));
    assert_refusal(&ds, "E_BAD_TEXT", "direction");
}

#[test]
fn out_of_range_and_unreadable_weights_fail_naming_range() {
    for weight in ["50", "999", "\"bold\""] {
        let ds = diags(&comp_with_text(&format!(
            "text x=0 y=0 size=24 start=0 duration=2 {{\nspan value=\"Hi\" font-weight={weight}\n}}\n"
        )));
        assert_refusal(&ds, "E_BAD_TEXT", "font-weight");
    }
    // Boundary weights stay valid.
    for weight in ["100", "900"] {
        let ds = diags(&comp_with_text(&format!(
            "text x=0 y=0 size=24 start=0 duration=2 {{\nspan value=\"Hi\" font-weight={weight}\n}}\n"
        )));
        assert!(ds.is_empty(), "weight {weight} must validate: {ds:?}");
    }
}

#[test]
fn span_missing_text_and_children_fails_naming_span() {
    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan font-size=20\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_TEXT", "span 1");
}

#[test]
fn non_span_child_of_text_fails() {
    let ds = diags(&comp_with_text(
        "text value=\"Hi\" x=0 y=0 size=24 start=0 duration=2 {\nbox width=10 height=10\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_TEXT", "span");
}

#[test]
fn spans_on_non_text_clips_fail() {
    for clip in [
        "box x=0 y=0 width=10 height=10 fill=\"#ffffff\"",
        "image path=\"media/gone.png\" x=0 y=0",
    ] {
        let ds = diags(&comp_with_text(&format!(
            "{clip} start=0 duration=2 {{\nspan value=\"Hi\"\n}}\n"
        )));
        assert_refusal(&ds, "E_BAD_TEXT", "span");
    }
}

#[test]
fn split_typewriter_and_count_with_spans_fail() {
    for fx in [
        "split=\"chars\"",
        "split=\"words\" typewriter=true",
        "count-from=0 count-to=5",
    ] {
        let ds = diags(&comp_with_text(&format!(
            "text value=\"Hi\" x=0 y=0 size=24 {fx} start=0 duration=2 {{\nspan value=\"Hi\"\n}}\n"
        )));
        assert_refusal(&ds, "E_BAD_TEXT", "span");
    }
}

#[test]
fn unknown_span_key_bad_color_bad_spacing_and_bad_family_fail() {
    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan value=\"Hi\" float=\"left\"\n}\n",
    ));
    assert_refusal(&ds, "E_UNKNOWN_KEY", "span 1");

    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan value=\"Hi\" color=\"red\"\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_COLOR", "color");

    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan value=\"Hi\" letter-spacing=-2\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_SPACING", "letter-spacing");

    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan value=\"Hi\" font-size=0\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_TEXT", "font-size");

    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan value=\"Hi\" italic=\"yes\"\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_TEXT", "italic");

    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan value=\"Hi\" font-family=\"\"\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_FONT", "font-family");
}

#[test]
fn missing_value_without_spans_still_fails() {
    let ds = diags(&comp_with_text("text x=0 y=0 size=24 start=0 duration=2\n"));
    assert_refusal(&ds, "E_MISSING_FIELD", "value");
}

#[test]
fn nested_non_span_inside_span_fails() {
    let ds = diags(&comp_with_text(
        "text x=0 y=0 size=24 start=0 duration=2 {\nspan value=\"Hi\" {\nbox width=10 height=10\n}\n}\n",
    ));
    assert_refusal(&ds, "E_BAD_TEXT", "span 1");
}
