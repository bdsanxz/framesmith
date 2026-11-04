//! Sprint 6 T001/T004/T005: ids, tracks/labels, and canvas presets.
//!
//! Validates through the public [`validate_folder`] API only: every
//! acceptance criterion is checked as `check` would report it, with
//! the resolved model asserted for defaults and identity storage.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n06-{}-{}-{}",
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

fn codes(dir: &Path) -> Vec<String> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

// --- T001: ids -----------------------------------------------------------

const OPEN: &str =
    "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n";

#[test]
fn valid_ids_pass_and_resolve() {
    let dir = tmp("ids-ok");
    write_comp(
        &dir,
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\"\n\
             nest nest=\"bumper\" start=0 duration=1 id=\"n1\"\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.clips.len(), 3);
}

#[test]
fn composition_id_registers_and_answers_refs() {
    let dir = tmp("ids-comp");
    write_comp(
        &dir,
        "composition name=\"main\" root=true id=\"comp-1\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         text value=\"A\" start=0 duration=1 id=\"a\"\n\
         text value=\"B\" duration=1 start-after=\"comp-1\"\n\
         nest nest=\"bumper\" start=0 duration=1\n\
         }\n\
         composition name=\"bumper\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.id.as_deref(), Some("comp-1"));
}

#[test]
fn duplicate_clip_ids_fail_naming_both_lines() {
    let dir = tmp("ids-dup");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"dup\"\n\
             text value=\"B\" start=1 duration=1 id=\"dup\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_DUP_ID");
    assert!(
        d.message.contains('2') && d.message.contains('3'),
        "{}",
        d.message
    );
    assert!(!d.fix.is_empty());
    assert!(d.file.ends_with("composition.kdl"));
}

#[test]
fn clip_id_colliding_with_composition_name_fails() {
    let dir = tmp("ids-comp-collide");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"A\" start=0 duration=1 id=\"main\"\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_DUP_ID"]);
}

#[test]
fn unknown_start_after_fails_naming_line_and_id() {
    let dir = tmp("ids-unknown");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start-after=\"ghost\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_UNKNOWN_REF");
    assert_eq!((d.line, d.column), (3, d.column));
    assert!(d.message.contains("ghost"), "{}", d.message);
    assert!(!d.fix.is_empty());
}

#[test]
fn underscore_start_after_resolves() {
    let dir = tmp("ids-underscore");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start_after=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn relative_start_without_absolute_start_passes() {
    let dir = tmp("ids-relaxed");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 start-after=\"a\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn self_reference_fails() {
    let dir = tmp("ids-self");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"A\" duration=1 id=\"me\" start-after=\"me\"\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_BAD_ID"]);
}

#[test]
fn bad_id_syntax_fails() {
    let dir = tmp("ids-syntax");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"A\" start=0 duration=1 id=\"has space\"\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_BAD_ID"]);
}

#[test]
fn several_id_problems_report_together() {
    let dir = tmp("ids-together");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"dup\"\n\
             text value=\"B\" start=1 duration=1 id=\"dup\"\n\
             text value=\"C\" start=0 duration=1 id=\"bad id\"\n\
             text value=\"D\" duration=1 start-after=\"ghost\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    let mut got = codes(&dir);
    got.sort();
    assert_eq!(
        got,
        vec!["E_BAD_ID", "E_DUP_ID", "E_UNKNOWN_REF"],
        "{:?}",
        v.diagnostics
    );
}

#[test]
fn nest_problems_report() {
    // Unknown nest target.
    let dir = tmp("ids-nest-unknown");
    write_comp(
        &dir,
        &format!("{OPEN}nest nest=\"ghost\" start=0 duration=1\n}}\n"),
    );
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_REF"]);

    // Nest naming a clip id instead of a composition.
    let dir = tmp("ids-nest-clip");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"clip-a\"\n\
             nest nest=\"clip-a\" start=0 duration=1\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_REF"]);
    assert!(
        v.diagnostics[0].message.contains("clip id"),
        "{}",
        v.diagnostics[0].message
    );

    // Nest without a target.
    let dir = tmp("ids-nest-missing");
    write_comp(&dir, &format!("{OPEN}nest start=0 duration=1\n}}\n"));
    assert_eq!(codes(&dir), vec!["E_MISSING_FIELD"]);
}

#[test]
fn projects_without_ids_validate_as_before() {
    let dir = tmp("ids-plain");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"Hi\" start=0 duration=2\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.id, None);
    assert_eq!(comp.canvas, None);
}

// --- T004: tracks and labels ---------------------------------------------

#[test]
fn declared_tracks_validate_and_store() {
    let dir = tmp("tracks-declared");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 track=\"video\"\n\
             text value=\"B\" start=0 duration=1 track=\"audio\"\n\
             text value=\"C\" start=0 duration=1 track=\"caption\"\n\
             text value=\"D\" start=0 duration=1 track=\"graphics\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::TrackKind;
    let got: Vec<TrackKind> = comp
        .clips
        .iter()
        .map(|c| match c {
            notation::model::Clip::Text(t) => t.track,
            _ => panic!("expected text"),
        })
        .collect();
    assert_eq!(
        got,
        vec![
            TrackKind::Video,
            TrackKind::Audio,
            TrackKind::Caption,
            TrackKind::Graphics
        ]
    );
}

#[test]
fn tracks_default_by_kind() {
    let dir = tmp("tracks-default");
    std::fs::write(dir.join("clip.mp4"), b"").unwrap();
    std::fs::write(dir.join("s.wav"), b"").unwrap();
    // An 8-byte PNG signature passes the still-image content sniff.
    std::fs::write(dir.join("p.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    write_comp(
        &dir,
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             text value=\"A\" start=0 duration=1\n\
             video path=\"clip.mp4\" start=0 duration=1\n\
             audio path=\"s.wav\" start=0 duration=1\n\
             box start=0 duration=1 width=4 height=4\n\
             image path=\"p.png\" start=0 duration=1\n\
             nest nest=\"bumper\" start=0 duration=1\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::{Clip, TrackKind};
    let got: Vec<TrackKind> = comp
        .clips
        .iter()
        .map(|c| match c {
            Clip::Text(t) => t.track,
            Clip::Video(w) => w.track,
            Clip::Audio(a) => a.track,
            Clip::Box(b) => b.track,
            Clip::Flex(f) => f.track,
            Clip::Image(i) => i.track,
            Clip::Path(p) => p.track,
            Clip::Svg(s) => s.track,
            Clip::Nest(n) => n.track,
        })
        .collect();
    assert_eq!(
        got,
        vec![
            TrackKind::Graphics,
            TrackKind::Video,
            TrackKind::Audio,
            TrackKind::Graphics,
            TrackKind::Graphics,
            TrackKind::Graphics,
        ]
    );
}

#[test]
fn labels_validate_with_no_effect() {
    let dir = tmp("tracks-label");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"A\" start=0 duration=1 label=\"Editor note\"\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    match &comp.clips[0] {
        notation::model::Clip::Text(t) => assert_eq!(t.label.as_deref(), Some("Editor note")),
        _ => panic!("expected text"),
    }
}

#[test]
fn unknown_track_fails_naming_accepted_set() {
    let dir = tmp("tracks-unknown");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"A\" start=0 duration=1 track=\"titles\"\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_BAD_TRACK");
    assert_eq!((d.line, d.column), (2, d.column));
    for name in ["video", "audio", "caption", "graphics"] {
        assert!(d.message.contains(name), "{}", d.message);
    }
    assert!(!d.fix.is_empty());
}

#[test]
fn non_string_label_fails_singly() {
    let dir = tmp("tracks-label-bad");
    write_comp(
        &dir,
        &format!("{OPEN}text value=\"A\" start=0 duration=1 label=5\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    assert_eq!(v.diagnostics[0].code, "E_BAD_LABEL");
}

#[test]
fn one_bad_value_yields_one_diagnostic_alongside_others() {
    let dir = tmp("tracks-single");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 track=\"titles\"\n\
             text value=\"B\" duration=1 start-after=\"ghost\"\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    let mut got = codes(&dir);
    got.sort();
    assert_eq!(
        got,
        vec!["E_BAD_TRACK", "E_UNKNOWN_REF"],
        "{:?}",
        v.diagnostics
    );
}

// --- T005: canvas presets ------------------------------------------------

fn canvas_comp(canvas: &str, extra: &str) -> String {
    format!(
        "composition name=\"c\" {canvas} {extra} duration=1 fps=30 background=\"#000\" version=1 {{\n\
         text value=\"Hi\" start=0 duration=1\n\
         }}\n"
    )
}

#[test]
fn each_preset_sizes_the_canvas() {
    let cases = [
        ("canvas=\"1080p\"", 1920, 1080),
        ("canvas=\"fullhd\"", 1920, 1080),
        ("canvas=\"720p\"", 1280, 720),
        ("canvas=\"square\"", 1080, 1080),
        ("canvas=\"portrait\"", 1080, 1920),
        ("canvas=\"4k\"", 3840, 2160),
        ("canvas=\"uhd\"", 3840, 2160),
    ];
    for (canvas, w, h) in cases {
        let dir = tmp("canvas-preset");
        write_comp(&dir, &canvas_comp(canvas, ""));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "{canvas}: {:?}", v.diagnostics);
        let comp = v.composition.expect("valid composition");
        assert_eq!((comp.width, comp.height), (w, h), "{canvas}");
    }
}

#[test]
fn canvas_aliases_canonicalize() {
    let dir = tmp("canvas-canon");
    write_comp(&dir, &canvas_comp("canvas=\"fullhd\"", ""));
    let comp = validate_folder(&dir).composition.expect("valid");
    assert_eq!(comp.canvas.as_deref(), Some("1080p"));
    let dir = tmp("canvas-canon2");
    write_comp(&dir, &canvas_comp("canvas=\"uhd\"", ""));
    let comp = validate_folder(&dir).composition.expect("valid");
    assert_eq!(comp.canvas.as_deref(), Some("4k"));
}

#[test]
fn explicit_dims_override_the_preset() {
    let dir = tmp("canvas-override");
    write_comp(&dir, &canvas_comp("canvas=\"square\"", "width=640"));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!((comp.width, comp.height), (640, 1080));

    let dir = tmp("canvas-override2");
    write_comp(
        &dir,
        &canvas_comp("canvas=\"1080p\"", "width=320 height=240"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!((comp.width, comp.height), (320, 240));
}

#[test]
fn unknown_preset_fails_listing_accepted_names() {
    let dir = tmp("canvas-unknown");
    write_comp(&dir, &canvas_comp("canvas=\"wide\"", "width=64 height=64"));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_eq!(v.diagnostics.len(), 1, "{:?}", v.diagnostics);
    let d = &v.diagnostics[0];
    assert_eq!(d.code, "E_BAD_CANVAS");
    for name in ["1080p", "fullhd", "720p", "square", "portrait", "4k", "uhd"] {
        assert!(d.message.contains(name), "{}", d.message);
    }
    assert!(!d.fix.is_empty());
}

#[test]
fn projects_without_canvas_render_as_before() {
    // Explicit dimensions stay required and unchanged.
    let dir = tmp("canvas-plain");
    write_comp(
        &dir,
        "composition name=\"c\" width=320 height=240 duration=1 background=\"#000\" version=1 {\n\
         text value=\"Hi\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!((comp.width, comp.height), (320, 240));
    assert_eq!(comp.canvas, None);

    // A missing width without canvas still fails exactly as before.
    let dir = tmp("canvas-missing");
    write_comp(
        &dir,
        "composition name=\"c\" height=240 duration=1 background=\"#000\" version=1 {\n\
         text value=\"Hi\" start=0 duration=1\n\
         }\n",
    );
    assert_eq!(codes(&dir), vec!["E_MISSING_FIELD"]);
}
