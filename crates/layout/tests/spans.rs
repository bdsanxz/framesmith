//! Sprint 8 T002: styled runs and world-script shaping through the
//! public [`layout_runs`] API — per-run paint, family fallback,
//! bidirectional visual order, mandatory ligatures and positional
//! forms, word breaking, refusals, and linear shaping cost.

use layout::{layout_runs, Align, Direction, Fonts, RunStyle, TextError};
use std::time::Instant;

fn run(text: &str) -> RunStyle {
    RunStyle {
        text: text.to_string(),
        families: vec!["sans-serif".to_string()],
        size: 48.0,
        weight: 400.0,
        italic: false,
        colour: [255, 255, 255, 255],
        letter_spacing: 0.0,
        line_height: None,
    }
}

fn visual_ltr(block: &layout::Block) -> String {
    let mut gs: Vec<(f32, char)> = block
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| (g.x, g.ch)))
        .collect();
    gs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    gs.into_iter().map(|g| g.1).collect()
}

fn glyph_count(block: &layout::Block) -> usize {
    block.runs.iter().map(|r| r.glyphs.len()).sum()
}

// --- per-run paint ----------------------------------------------------------

#[test]
fn multi_run_paragraph_paints_each_run_with_own_style() {
    let mut fonts = Fonts::new();
    let block = layout_runs(
        &mut fonts,
        &[
            RunStyle {
                size: 24.0,
                weight: 700.0,
                colour: [255, 0, 0, 255],
                letter_spacing: 2.0,
                line_height: Some(60.0),
                ..run("Hi ")
            },
            RunStyle {
                size: 48.0,
                weight: 400.0,
                italic: true,
                colour: [0, 255, 0, 255],
                ..run("there")
            },
        ],
        Align::Left,
        Direction::Ltr,
        None,
        None,
    )
    .expect("multi-run must lay out");
    assert_eq!(visual_ltr(&block), "Hi there");
    let sizes: Vec<f32> = block.runs.iter().map(|r| r.size).collect();
    assert!(sizes.contains(&24.0), "first run keeps its size: {sizes:?}");
    assert!(
        sizes.contains(&48.0),
        "second run keeps its size: {sizes:?}"
    );
    let colours: Vec<[u8; 4]> = block.runs.iter().map(|r| r.colour).collect();
    assert!(
        colours.contains(&[255, 0, 0, 255]),
        "first run keeps its color"
    );
    assert!(
        colours.contains(&[0, 255, 0, 255]),
        "second run keeps its color"
    );
}

#[test]
fn missing_first_family_falls_through_to_next_available() {
    let mut fonts = Fonts::new();
    let block = layout_runs(
        &mut fonts,
        &[RunStyle {
            families: vec!["no-such-family-xyz".to_string(), "sans-serif".to_string()],
            ..run("Hello")
        }],
        Align::Left,
        Direction::Ltr,
        None,
        None,
    )
    .expect("a missing first family must fall through");
    assert_eq!(visual_ltr(&block), "Hello");
    assert!(glyph_count(&block) > 0);
}

// --- bidirectional order ----------------------------------------------------

#[test]
fn rtl_mixed_script_shapes_correct_visual_order() {
    let mut fonts = Fonts::new();
    let runs = [run("Hello "), run("مرحبا "), run("123")];
    let rtl = layout_runs(&mut fonts, &runs, Align::Left, Direction::Rtl, None, None)
        .expect("rtl must lay out");
    let visual = visual_ltr(&rtl);
    let chars: Vec<char> = visual.chars().collect();
    // Digits keep their native order at the visual left, latin at the
    // visual right, arabic between them.
    assert_eq!(
        &chars[..3],
        &['1', '2', '3'],
        "digits stay ordered leftmost: {visual:?}"
    );
    assert_eq!(
        &chars[chars.len() - 5..],
        &['H', 'e', 'l', 'l', 'o'],
        "latin stays ordered rightmost: {visual:?}"
    );
    assert!(
        chars[3..chars.len() - 5]
            .iter()
            .all(|c| *c != 'H' && !c.is_ascii_digit()),
        "arabic sits between the runs: {visual:?}"
    );
    // The ltr base orders the same runs differently: latin leftmost.
    let ltr = layout_runs(&mut fonts, &runs, Align::Left, Direction::Ltr, None, None)
        .expect("ltr must lay out");
    assert!(
        visual_ltr(&ltr).starts_with("Hello"),
        "ltr keeps latin leftmost"
    );
    assert_ne!(
        visual_ltr(&ltr),
        visual,
        "direction changes the visual order"
    );
}

#[test]
fn auto_direction_follows_first_strong_character() {
    let mut fonts = Fonts::new();
    let rtl_first = layout_runs(
        &mut fonts,
        &[run("مرحبا Hello")],
        Align::Left,
        Direction::Auto,
        None,
        None,
    )
    .expect("auto must lay out");
    let rtl = layout_runs(
        &mut fonts,
        &[run("مرحبا Hello")],
        Align::Left,
        Direction::Rtl,
        None,
        None,
    )
    .expect("rtl must lay out");
    assert_eq!(visual_ltr(&rtl_first), visual_ltr(&rtl));
}

// --- ligatures and positional forms -----------------------------------------

#[test]
fn arabic_lam_alef_shapes_mandatory_ligature() {
    let mut fonts = Fonts::new();
    let block = layout_runs(
        &mut fonts,
        &[run("لا")],
        Align::Left,
        Direction::Rtl,
        None,
        None,
    )
    .expect("arabic must lay out");
    assert_eq!(
        glyph_count(&block),
        1,
        "lam plus alef must shape into one mandatory ligature glyph"
    );
}

#[test]
fn arabic_word_shapes_fewer_glyphs_than_characters() {
    let mut fonts = Fonts::new();
    let word = "السلام";
    let block = layout_runs(
        &mut fonts,
        &[run(word)],
        Align::Left,
        Direction::Rtl,
        None,
        None,
    )
    .expect("arabic must lay out");
    assert!(
        glyph_count(&block) < word.chars().count(),
        "ligatures must merge glyphs below the character count"
    );
}

#[test]
fn hebrew_final_forms_shape_positionally() {
    let mut fonts = Fonts::new();
    let kaf = layout_runs(
        &mut fonts,
        &[run("כ")],
        Align::Left,
        Direction::Rtl,
        None,
        None,
    )
    .expect("hebrew must lay out");
    let final_kaf = layout_runs(
        &mut fonts,
        &[run("ך")],
        Align::Left,
        Direction::Rtl,
        None,
        None,
    )
    .expect("hebrew must lay out");
    let id = |b: &layout::Block| {
        b.runs
            .iter()
            .flat_map(|r| r.glyphs.iter().map(|g| g.id))
            .collect::<Vec<_>>()
    };
    assert_eq!(id(&kaf).len(), 1);
    assert_eq!(id(&final_kaf).len(), 1);
    assert_ne!(
        id(&kaf),
        id(&final_kaf),
        "final kaf must shape positionally"
    );
    let word = layout_runs(
        &mut fonts,
        &[run("מלך")],
        Align::Left,
        Direction::Rtl,
        None,
        None,
    )
    .expect("hebrew must lay out");
    assert_eq!(glyph_count(&word), 3);
}

// --- breaking ---------------------------------------------------------------

#[test]
fn long_unbreakable_word_overflows_with_glyphs_unchanged() {
    let mut fonts = Fonts::new();
    let word = "supercalifragilisticexpialidocious";
    let narrow = layout_runs(
        &mut fonts,
        &[run(word)],
        Align::Left,
        Direction::Ltr,
        Some(120.0),
        None,
    )
    .expect("long word must lay out");
    let wide = layout_runs(
        &mut fonts,
        &[run(word)],
        Align::Left,
        Direction::Ltr,
        None,
        None,
    )
    .expect("long word must lay out");
    assert_eq!(narrow.lines.len(), 1, "no silent mid-word split");
    assert!(
        narrow.lines[0].width > 120.0,
        "the unbreakable word overflows the wrap width"
    );
    assert_eq!(
        glyph_count(&narrow),
        glyph_count(&wide),
        "overflow keeps every glyph"
    );
}

#[test]
fn explicit_soft_break_splits_long_word() {
    let mut fonts = Fonts::new();
    let block = layout_runs(
        &mut fonts,
        &[run("supercali\u{200b}fragilistic")], // "supercali\u{200b}fragilistic": explicit soft break
        Align::Left,
        Direction::Ltr,
        Some(220.0),
        None,
    )
    .expect("soft break must lay out");
    assert_eq!(block.lines.len(), 2, "the soft break must split the word");
}

// --- per-run spacing and line height ----------------------------------------

#[test]
fn per_run_letter_spacing_advances_only_its_run() {
    let mut fonts = Fonts::new();
    let block = layout_runs(
        &mut fonts,
        &[
            run("aaa"),
            RunStyle {
                letter_spacing: 10.0,
                ..run("aaa")
            },
        ],
        Align::Left,
        Direction::Ltr,
        None,
        None,
    )
    .expect("spacing must lay out");
    let mut xs: Vec<f32> = block
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| g.x))
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(&b).unwrap());
    assert_eq!(xs.len(), 6);
    let first_gap = xs[1] - xs[0];
    for w in xs[3..].windows(2) {
        assert!(
            ((w[1] - w[0]) - (first_gap + 10.0)).abs() < 1e-3,
            "second-run gaps carry only the second spacing: {xs:?}"
        );
    }
}

#[test]
fn run_line_height_separates_baselines() {
    let mut fonts = Fonts::new();
    let text = "aaa bbb ccc ddd eee fff";
    let plain = layout_runs(
        &mut fonts,
        &[run(text)],
        Align::Left,
        Direction::Ltr,
        Some(100.0),
        None,
    )
    .expect("wrap must lay out");
    assert!(plain.lines.len() > 1, "narrow wrap must break lines");
    let declared = 120.0f32;
    let spaced = layout_runs(
        &mut fonts,
        &[RunStyle {
            line_height: Some(declared),
            ..run(text)
        }],
        Align::Left,
        Direction::Ltr,
        Some(100.0),
        None,
    )
    .expect("line height must lay out");
    assert_eq!(spaced.lines.len(), plain.lines.len());
    let gap = spaced.lines[1].baseline - spaced.lines[0].baseline;
    assert!(
        (gap - declared).abs() < 1e-3,
        "gap {gap} must equal declared {declared}"
    );
}

// --- refusals ---------------------------------------------------------------

#[test]
fn newline_only_run_refuses_naming_span() {
    let mut fonts = Fonts::new();
    let err = layout_runs(
        &mut fonts,
        &[run("Hi"), run("\n")],
        Align::Left,
        Direction::Ltr,
        None,
        None,
    )
    .expect_err("a run shaping to no glyphs must refuse");
    assert_eq!(err, TextError::NoGlyphs { run: 1 });
    assert!(
        err.to_string().contains("span 2"),
        "refusal names the span: {err}"
    );
}

#[test]
fn empty_paragraph_refuses() {
    let mut fonts = Fonts::new();
    let err = layout_runs(&mut fonts, &[], Align::Left, Direction::Ltr, None, None)
        .expect_err("no runs must refuse");
    assert_eq!(err, TextError::NoGlyphs { run: 0 });
}

// --- linear cost ------------------------------------------------------------

#[test]
fn two_hundred_runs_shape_within_two_percent_of_twice_one_hundred() {
    let mut fonts = Fonts::new();
    let latin = "the quick brown fox jumps over the lazy dog ";
    let mk = |n: usize| {
        (0..n)
            .map(|i| RunStyle {
                text: format!("{latin}{i} "),
                ..run("")
            })
            .collect::<Vec<_>>()
    };
    // Warm the fallback and machine caches so the clock reads shaping.
    let _ = layout_runs(&mut fonts, &mk(4), Align::Left, Direction::Ltr, None, None);
    // Round medians stay robust when sibling tests share the CPU: each
    // round times both sizes back to back, and the median round sets
    // the verdict.
    let mut ratios = Vec::new();
    for _ in 0..5 {
        let small = mk(100);
        let big = mk(200);
        let start = Instant::now();
        layout_runs(&mut fonts, &small, Align::Left, Direction::Ltr, None, None)
            .expect("perf paragraph must lay out");
        let hundred = start.elapsed();
        let start = Instant::now();
        layout_runs(&mut fonts, &big, Align::Left, Direction::Ltr, None, None)
            .expect("perf paragraph must lay out");
        let two_hundred = start.elapsed();
        ratios.push(two_hundred.as_secs_f64() / hundred.as_secs_f64());
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let ratio = ratios[2];
    assert!(
        (1.96..=2.04).contains(&ratio),
        "200 runs must cost twice 100 runs ±2%, got median {ratio:.3} from {ratios:.3?}"
    );
}
