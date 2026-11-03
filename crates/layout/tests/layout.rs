//! Parley text engine: shaping, fallback, wrap, and emoji routing.

use layout::{Align, Fonts, Style, layout};

fn style(families: &[&str]) -> Style {
    Style {
        families: families.iter().map(|s| s.to_string()).collect(),
        size: 48.0,
        weight: 400.0,
        italic: false,
        align: Align::Left,
        colour: [255, 255, 255, 255],
        line_height: None,
        letter_spacing: None,
    }
}

/// A declared line height separates baselines exactly per the value.
#[test]
fn line_height_separates_baselines() {
    let mut fonts = Fonts::new();
    let base = style(&["sans-serif"]);
    let plain = layout(&mut fonts, "Hello brave new world", &base, Some(200.0), None)
        .expect("wrap must lay out");
    assert!(plain.lines.len() > 1, "narrow wrap must break lines");
    let natural = plain.lines[1].baseline - plain.lines[0].baseline;
    assert!(natural > 0.0);
    for declared in [80.0f32, 120.0] {
        let spaced = layout(
            &mut fonts,
            "Hello brave new world",
            &Style { line_height: Some(declared), ..base.clone() },
            Some(200.0),
            None,
        )
        .expect("line height must lay out");
        assert_eq!(spaced.lines.len(), plain.lines.len());
        let gap = spaced.lines[1].baseline - spaced.lines[0].baseline;
        assert!((gap - declared).abs() < 1e-3, "gap {gap} must equal declared {declared}");
    }
}

/// Letter spacing advances glyphs exactly per the declared value.
#[test]
fn letter_spacing_advances_glyphs() {
    let mut fonts = Fonts::new();
    let base = style(&["sans-serif"]);
    let plain = layout(&mut fonts, "Hello", &base, None, None).expect("must lay out");
    let spaced = layout(
        &mut fonts,
        "Hello",
        &Style { letter_spacing: Some(6.0), ..base.clone() },
        None,
        None,
    )
    .expect("letter spacing must lay out");
    let px: Vec<f32> = plain.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.x)).collect();
    let qx: Vec<f32> = spaced.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.x)).collect();
    assert_eq!(px.len(), qx.len());
    assert!(px.len() >= 2);
    for (i, (a, b)) in px.iter().zip(qx.iter()).enumerate() {
        assert!((b - a - i as f32 * 6.0).abs() < 1e-3, "glyph {i}: {a} -> {b}");
    }
}

/// Zero letter spacing renders identically to no declared spacing.
#[test]
fn zero_letter_spacing_matches_default() {
    let mut fonts = Fonts::new();
    let base = style(&["sans-serif"]);
    let plain = layout(&mut fonts, "Hello", &base, None, None).expect("must lay out");
    let zero = layout(
        &mut fonts,
        "Hello",
        &Style { letter_spacing: Some(0.0), ..base.clone() },
        None,
        None,
    )
    .expect("zero spacing must lay out");
    assert_eq!(plain, zero, "zero spacing must equal the default advance");
}

/// The machine on this host must supply a sans-serif face; the
/// paragraph shapes into positioned runs.
#[test]
fn hello_shapes_into_runs() {
    let mut fonts = Fonts::new();
    if fonts.machine_faces().is_empty() && fonts.is_empty() {
        // No machine fonts registered yet: families_of would fail.
        // Layout still proves the fallback chain resolves sans-serif.
    }
    let block = layout(&mut fonts, "Hello", &style(&["sans-serif"]), None, None).expect("sans-serif must resolve");
    assert!(!block.runs.is_empty(), "shaped text must produce runs");
    assert!(block.runs.iter().all(|run| !run.glyphs.is_empty()));
    assert!(block.width > 0.0 && block.height > 0.0);
    assert_eq!(block.lines.len(), 1);
}

/// Wrap breaks lines and centers each within the wrap width.
#[test]
fn wrap_breaks_and_centers() {
    let mut fonts = Fonts::new();
    let wrapped = style(&["sans-serif"]);
    let block = layout(&mut fonts, "Hello brave new world", &Style { align: Align::Center, ..wrapped }, Some(200.0), None)
        .expect("wrap must lay out");
    assert!(block.lines.len() > 1, "narrow wrap must break lines");
    assert!((block.width - 200.0).abs() < 1e-3);
}

/// An unknown family with no machine fallback refuses, never blanks.
#[test]
fn unknown_family_refuses() {
    let mut fonts = Fonts::new();
    // A name no composition or machine family can match: with the
    // missing-mark fallback present this still resolves (documented);
    // without any machine font it must refuse.
    let result = layout(&mut fonts, "Hi", &style(&["no-such-family-xyz"]), None, None);
    if fonts.machine_faces().is_empty() {
        assert!(result.is_err(), "without fonts the family must refuse");
    } else {
        assert!(result.is_ok(), "missing-mark fallback must resolve");
    }
}

/// Emoji routes to an FFmpeg-decoded picture when a color face is
/// registered; otherwise the character refuses explicitly.
#[test]
fn emoji_picture_or_explicit_refusal() {
    let mut fonts = Fonts::new();
    let ffmpeg = ["ffmpeg", "/opt/homebrew/bin/ffmpeg", "/usr/bin/ffmpeg"]
        .into_iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.is_file());
    let result = layout(&mut fonts, "A\u{1F600}", &style(&["sans-serif"]), None, ffmpeg.as_deref());
    match result {
        Ok(block) => {
            assert!(!block.pictures.is_empty(), "emoji must draw as a picture, never outlines");
            let pic = &block.pictures[0];
            assert!(pic.width > 0 && pic.height > 0);
            assert_eq!(pic.rgba.len(), pic.width as usize * pic.height as usize * 4);
            assert!(!block.runs.iter().flat_map(|r| &r.glyphs).any(|g| g.ch == '\u{1F600}'), "emoji must not draw as outlines");
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("U+1F600") || msg.contains("none of the fonts"), "{msg}");
        }
    }
}
