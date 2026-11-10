//! Sprint 22 T002: track styles, per-word overrides, and preset
//! families through the validation gate.
//!
//! Covers the T002 acceptance criteria at the gate: styles render the
//! documented vocabulary (family, size, weight, color, opacity,
//! outline, background, position, max-lines) with hyphen spellings
//! and underscore aliases; unknown presets and unknown properties
//! refuse naming the property; overrides persist project-locally and
//! survive a save/reload; source files are never rewritten.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n22styles-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn diags_on(dir: &Path) -> Vec<Diagnostic> {
    validate_folder(dir).diagnostics
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code}, got {:?}",
        ds.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    let messages: Vec<String> = ds.iter().map(|d| d.message.clone()).collect();
    for needle in needles {
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {messages:?}"
        );
    }
}

const SRT: &str = "1\n00:00:00,000 --> 00:00:02,000\nhello world\n\n";

fn project_with(dir: &Path, clip: &str) {
    std::fs::write(dir.join("talk.srt"), SRT).unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        format!(
            "composition name=\"main\" width=640 height=480 duration=6 background=\"#000\" version=1 {{\n    {clip}\n}}\n"
        ),
    )
    .unwrap();
}

fn valid_clip(dir: &Path) -> notation::model::CaptionClip {
    let v = validate_folder(dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.expect("valid").clips[0] {
        notation::model::Clip::Caption(cc) => cc.clone(),
        other => panic!("expected a captions clip, got {}", other.kind()),
    }
}

#[test]
fn track_styles_accept_hyphens_and_underscore_aliases() {
    for attrs in [
        "font-size=24 font-weight=700 color=\"#ffd60a\" opacity=0.9 outline=2 outline-color=\"#000\" background=\"#111\" position=\"rail\" max-lines=2",
        "font_size=24 font_weight=700 color=\"#ffd60a\" opacity=0.9 outline_width=2 outline_color=\"#000\" background_color=\"#111\" position=\"rail\" max_lines=2",
        "position=\"embed\" x=10 y=20",
    ] {
        let dir = tmp("style");
        project_with(&dir, &format!("captions src=\"talk.srt\" start=0 duration=6 {attrs}"));
        let cc = valid_clip(&dir);
        if attrs.contains("embed") {
            assert_eq!(cc.style.position, notation::model::CaptionPosition::Embed);
            assert_eq!((cc.x, cc.y), (10.0, 20.0));
            continue;
        }
        assert_eq!(cc.style.font_size, 24.0);
        assert_eq!(cc.style.font_weight, 700.0);
        assert_eq!(
            cc.style.color,
            notation::model::Rgb { r: 255, g: 214, b: 10 }
        );
        assert_eq!(cc.style.opacity, 0.9);
        assert_eq!(cc.style.outline_width, 2.0);
        assert!(cc.style.background.is_some());
        assert_eq!(cc.style.max_lines, 2);
    }
}

#[test]
fn track_style_defaults_match_the_documented_rest() {
    let dir = tmp("defaults");
    project_with(&dir, "captions src=\"talk.srt\" start=0 duration=6");
    let cc = valid_clip(&dir);
    let style = &cc.style;
    assert!(style.font_family.is_none());
    assert_eq!(style.font_size, 48.0);
    assert_eq!(style.font_weight, 400.0);
    assert_eq!(
        style.color,
        notation::model::Rgb {
            r: 255,
            g: 255,
            b: 255
        }
    );
    assert_eq!(style.opacity, 1.0);
    assert_eq!(style.outline_width, 0.0);
    assert!(style.background.is_none());
    assert_eq!(style.position, notation::model::CaptionPosition::Rail);
    assert_eq!(style.max_lines, 2);
}

#[test]
fn open_family_lists_resolve_in_try_order() {
    let dir = tmp("family");
    project_with(
        &dir,
        "captions src=\"talk.srt\" start=0 duration=6 font-family=\"First, Fallback\"",
    );
    let cc = valid_clip(&dir);
    assert_eq!(
        cc.style.font_family,
        Some(vec!["First".to_string(), "Fallback".to_string()])
    );
}

#[test]
fn unknown_presets_and_properties_refuse_naming_the_property() {
    let cases = [
        (
            "entrance=\"whoosh\"",
            "E_BAD_CAPTIONS",
            "entrance",
            "whoosh",
        ),
        (
            "highlight=\"sparkle\"",
            "E_BAD_CAPTIONS",
            "highlight",
            "sparkle",
        ),
        ("exit=\"vanish\"", "E_BAD_CAPTIONS", "exit", "vanish"),
        (
            "position=\"middle\"",
            "E_BAD_CAPTIONS",
            "position",
            "middle",
        ),
        (
            "font-weight=50",
            "E_BAD_CAPTIONS",
            "font-weight",
            "100 to 900",
        ),
        ("font-size=0", "E_BAD_CAPTIONS", "font-size", "font-size"),
        ("opacity=2", "E_BAD_CAPTIONS", "opacity", "opacity"),
        ("color=\"red\"", "E_BAD_CAPTIONS", "color", "not a `#rgb`"),
        ("max-lines=0", "E_BAD_CAPTIONS", "max-lines", "max-lines"),
    ];
    for (attrs, code, prop, detail) in cases {
        let dir = tmp("gate");
        project_with(
            &dir,
            &format!("captions src=\"talk.srt\" start=0 duration=6 {attrs}"),
        );
        assert_refusal(&diags_on(&dir), code, &[prop, detail]);
    }
    // Unknown override properties refuse naming the property.
    let dir = tmp("ovgate");
    std::fs::write(dir.join("talk.srt"), SRT).unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"main\" width=640 height=480 duration=6 background=\"#000\" version=1 {\n    captions src=\"talk.srt\" start=0 duration=6 {\n        override cue=0 word=0 blur=3\n    }\n}\n",
    )
    .unwrap();
    assert_refusal(&diags_on(&dir), "E_BAD_CAPTIONS", &["blur"]);
}

#[test]
fn every_documented_preset_name_is_accepted() {
    for entrance in notation::captions::ENTRANCE_PRESETS {
        for highlight in notation::captions::HIGHLIGHT_PRESETS {
            for exit in notation::captions::EXIT_PRESETS {
                let dir = tmp("preset");
                project_with(
                    &dir,
                    &format!(
                        "captions src=\"talk.srt\" start=0 duration=6 entrance=\"{entrance}\" highlight=\"{highlight}\" exit=\"{exit}\""
                    ),
                );
                let cc = valid_clip(&dir);
                assert_eq!(cc.entrance.as_deref(), Some(entrance));
                assert_eq!(cc.highlight.as_deref(), Some(highlight));
                assert_eq!(cc.exit.as_deref(), Some(exit));
            }
        }
    }
    assert_eq!(notation::captions::ENTRANCE_PRESETS.len(), 13);
    assert_eq!(notation::captions::HIGHLIGHT_PRESETS.len(), 7);
    assert_eq!(notation::captions::EXIT_PRESETS.len(), 11);
}

#[test]
fn overrides_persist_project_locally_and_survive_reload() {
    // Overrides live in the project file (never the source), and a
    // second validation of the same project reads back the same
    // overrides — the save/reload round-trip.
    let dir = tmp("persist");
    project_with(
        &dir,
        "captions src=\"talk.srt\" start=0 duration=6 {\n        override cue=0 word=1 color=\"#ffd60a\" scale=1.2 dx=3 dy=-2 opacity=0.8 rotation=5 font-size=54 font-weight=700 font-family=\"First, Fallback\"\n    }",
    );
    let pristine = std::fs::read(dir.join("talk.srt")).unwrap();
    let first = valid_clip(&dir);
    assert_eq!(first.overrides.len(), 1);
    let ov = &first.overrides[0];
    assert_eq!((ov.cue, ov.word), (0, 1));
    assert_eq!(ov.scale, Some(1.2));
    assert_eq!(ov.rotation, Some(5.0));
    assert_eq!(ov.opacity, Some(0.8));
    assert_eq!(ov.font_size, Some(54.0));
    assert_eq!(ov.font_weight, Some(700.0));
    assert_eq!(
        ov.font_family,
        Some(vec!["First".to_string(), "Fallback".to_string()])
    );
    // Reload: same project validates to the same overrides.
    let second = valid_clip(&dir);
    assert_eq!(first.overrides, second.overrides);
    assert_eq!(first.cues, second.cues);
    assert_eq!(first.style, second.style);
    // The source file was never rewritten by validation.
    let after = std::fs::read(dir.join("talk.srt")).unwrap();
    assert_eq!(after, pristine);
    assert_eq!(String::from_utf8(after).unwrap(), SRT);
}
