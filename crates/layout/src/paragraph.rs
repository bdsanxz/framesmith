//! One paragraph in one style: parley shapes it, breaks its lines and
//! positions its glyphs; this module checks every character came from
//! its own families and collects the runs and emoji pictures the
//! renderer draws.
//!
//! Only the Sprint surface is exposed (one style per paragraph, one
//! color, wrap width, three alignments). Emoji characters whose face
//! holds an `sbix` picture draw as pictures decoded by FFmpeg, never as
//! outlines.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use parley::{
    BaseDirection, FontFamily, FontFamilyName, FontStyle, FontWeight, PositionedLayoutItem,
    StyleProperty,
};

use crate::fonts::{EmojiPic, Fonts, FontsRef};

/// Line alignment within the block width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// The text attributes of a paragraph, already read from the notation.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Families in try order (`font-family`).
    pub families: Vec<String>,
    /// Size in pixels, above 0.
    pub size: f32,
    /// Weight (400 regular, 700 bold).
    pub weight: f32,
    /// Whether the italic style applies (sheared when faceless).
    pub italic: bool,
    pub align: Align,
    /// Drawn color (RGBA).
    pub colour: [u8; 4],
    /// Declared line separation in canvas pixels (T007); `None` means
    /// the font default. Only non-negative values ever arrive here.
    pub line_height: Option<f32>,
    /// Declared extra advance per glyph in canvas pixels (T008); `None`
    /// or zero means default advance. Only non-negative values arrive.
    pub letter_spacing: Option<f32>,
}

/// One positioned glyph: its id in its run's face, its position (`x`
/// from the block's left edge, `y` the baseline from its top), and the
/// character it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
    pub ch: char,
}

/// Glyphs sharing a face and a color: the font file bytes, the face
/// index in them, the size, the faux-italic skew in degrees, whether
/// parley asked for faux bold, the color, and the glyphs in line order.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub font: Arc<Vec<u8>>,
    pub index: u32,
    pub size: f32,
    pub skew_degrees: f32,
    pub embolden: bool,
    pub colour: [u8; 4],
    pub glyphs: Vec<Glyph>,
}

/// A color-emoji picture on the block: straight RGBA pixels, its size,
/// and its top-left corner (`x` from the block's left edge, `y` from its
/// top). The renderer composites it over the canvas with source-over.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    pub x: f32,
    pub y: f32,
}

/// One line: its left edge from the block's left, its width (without
/// trailing space), and its baseline from the block's top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub left: f32,
    pub width: f32,
    pub baseline: f32,
}

/// A laid-out paragraph: block size, outline runs, emoji pictures, lines.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub width: f32,
    pub height: f32,
    pub runs: Vec<Run>,
    pub pictures: Vec<Picture>,
    pub lines: Vec<Line>,
}

/// Base direction of a multi-run paragraph (Sprint 8 look,
/// CAP-002/T002): `ltr` orders runs left to right, `rtl` right to
/// left, `auto` infers the direction from the first strong character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
    Auto,
}

/// One styled run of a multi-run paragraph (Sprint 8 look,
/// CAP-002/T002): the run text with its fully resolved style
/// (inheritance from the clip and parent spans already applied by the
/// caller). `families` lists try-order names with missing names
/// already left out per run; `weight` runs 100 to 900;
/// `letter_spacing` is resolved pixels (0 is default advance);
/// `line_height` is the declared separation (none is the font
/// default).
#[derive(Clone, Debug, PartialEq)]
pub struct RunStyle {
    pub text: String,
    pub families: Vec<String>,
    pub size: f32,
    pub weight: f32,
    pub italic: bool,
    pub colour: [u8; 4],
    pub letter_spacing: f32,
    pub line_height: Option<f32>,
}

/// Why a paragraph cannot be laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// No name of the family list names a composition or machine family.
    NoFamily,
    /// No name of one run's family list names a composition or
    /// machine family; the run is 0-based in document order.
    NoFamilyRun { run: usize },
    /// A character drawn from none of its families: the engine's own
    /// choice, which is never drawn.
    MissingCharacter(char),
    /// A character of one run drawn from none of that run's
    /// families; the run is 0-based in document order.
    MissingCharacterRun { run: usize, character: char },
    /// One run shapes to no glyphs and no pictures; the run is
    /// 0-based in document order.
    NoGlyphs { run: usize },
    /// An emoji character whose picture is missing or refused, with the reason.
    EmojiRefused { character: char, reason: String },
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::NoFamily => write!(
                f,
                "no family of the text is a font of the composition or of the machine"
            ),
            TextError::NoFamilyRun { run } => write!(
                f,
                "no family of span {} is a font of the composition or of the machine",
                run + 1
            ),
            TextError::MissingCharacter(ch) => {
                write!(f, "the character {ch:?} is in none of the fonts named")
            }
            TextError::MissingCharacterRun { run, character } => {
                write!(
                    f,
                    "the character {character:?} of span {} is in none of the fonts named",
                    run + 1
                )
            }
            TextError::NoGlyphs { run } => write!(
                f,
                "span {} shapes to no glyphs; give the span text the fonts can shape",
                run + 1
            ),
            TextError::EmojiRefused { character, reason } => {
                write!(
                    f,
                    "the emoji picture of the character U+{:04X} is refused: {reason}",
                    *character as u32
                )
            }
        }
    }
}

impl std::error::Error for TextError {}

/// Lays out one paragraph in `style`, broken within `width` where
/// given (else one line). `ffmpeg_bin` decodes emoji pictures; `None`
/// refuses emoji with [`TextError::EmojiRefused`].
pub fn layout(
    fonts: &mut Fonts,
    text: &str,
    style: &Style,
    width: Option<f32>,
    ffmpeg_bin: Option<&std::path::Path>,
) -> Result<Block, TextError> {
    // Each name of the list as the family the engine knows.
    let mut families = fonts.families_of(&style.families)?;
    // A character no family maps is drawn by the fallback's family,
    // which joins the end of the list; one no machine family maps is
    // reported below, never drawn as a wrong glyph.
    let mut unmapped: Vec<char> = Vec::new();
    for ch in text.chars() {
        if ch.is_control() || ch.is_whitespace() {
            continue;
        }
        if fonts.families_map(&families, ch) || unmapped.contains(&ch) {
            continue;
        }
        match fonts.fall_back(ch) {
            Some(family) => {
                if !families.contains(&family) {
                    families.push(family);
                }
            }
            None => unmapped.push(ch),
        }
    }
    let names: Vec<FontFamilyName> = families
        .iter()
        .map(|name| FontFamilyName::named(name))
        .collect();
    let stack = FontFamily::List(names.into());
    let weight = if style.weight >= 650.0 {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    };
    let italic = if style.italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    let FontsRef { context, layouts } = fonts.parts();
    let mut builder = layouts.ranged_builder(context, text, 1.0, true);
    builder.set_base_direction(BaseDirection::Ltr);
    builder.push(StyleProperty::FontFamily(stack), 0..text.len());
    builder.push(StyleProperty::FontSize(style.size), 0..text.len());
    builder.push(StyleProperty::FontWeight(weight), 0..text.len());
    builder.push(StyleProperty::FontStyle(italic), 0..text.len());
    builder.push(StyleProperty::Brush(style.colour), 0..text.len());
    let mut layout = builder.build(text);
    layout.break_all_lines(width);
    // First pass: every shaped cluster's character must come from the
    // listed families; line widths leave out trailing space.
    let mut widths: Vec<f32> = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            for cluster in run.clusters() {
                let ch = text
                    .get(cluster.text_range())
                    .and_then(|s| s.chars().next())
                    .unwrap_or('\u{fffd}');
                if ch.is_control() || ch.is_whitespace() {
                    continue;
                }
                if !fonts.families_map(&families, ch) && !unmapped.contains(&ch) {
                    return Err(TextError::MissingCharacter(ch));
                }
            }
        }
        let metrics = line.metrics();
        widths.push(metrics.advance - metrics.hanging_advance);
    }
    // Second pass: runs and emoji pictures with block positions.
    // Letter spacing (T008): every positioned glyph — outlines and
    // emoji pictures alike — advances one extra slot per preceding
    // glyph on its line. Counts are pre-walked so alignment offsets use
    // the spaced widths, exactly as the renderer draws them.
    let letter_spacing = style.letter_spacing.unwrap_or(0.0).max(0.0);
    let mut counts: Vec<usize> = Vec::new();
    for line in layout.lines() {
        let mut n = 0usize;
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                n += glyph_run.positioned_glyphs().count();
            }
        }
        counts.push(n);
    }
    let spaced_widths: Vec<f32> = widths
        .iter()
        .zip(counts.iter())
        .map(|(w, n)| w + *n as f32 * letter_spacing)
        .collect();
    let spaced_widest = spaced_widths.iter().copied().fold(0.0, f32::max);
    let block_width = match width {
        Some(w) => w,
        None => spaced_widest,
    };
    let mut runs: Vec<Run> = Vec::new();
    let mut pictures: Vec<Picture> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();
    let mut height = 0.0f32;
    // Clusters already given to an earlier glyph run of the same run,
    // by (run start, style): shaping cuts one run into glyph runs.
    let mut taken: BTreeMap<(usize, u16), usize> = BTreeMap::new();
    // Flattened glyph/picture totals after each line, for the
    // line-height shift below: each line's items form one contiguous
    // span in push order.
    let mut glyph_marks: Vec<usize> = Vec::new();
    let mut picture_marks: Vec<usize> = Vec::new();
    let mut glyph_total = 0usize;
    for (line, line_width) in layout.lines().zip(spaced_widths.iter().copied()) {
        let offset = match style.align {
            Align::Left => 0.0,
            Align::Center => (block_width - line_width) / 2.0,
            Align::Right => block_width - line_width,
        };
        let mut baseline: Option<f32> = None;
        let mut advance_index = 0usize;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let key = (run.font().data.id(), run.font().index);
            let Some(face) = fonts.face_for_blob(key.0, key.1) else {
                continue;
            };
            if !families.contains(&face.family) {
                let ch = text
                    .get(run.text_range())
                    .and_then(|s| s.chars().next())
                    .unwrap_or('\u{fffd}');
                return Err(TextError::MissingCharacter(ch));
            }
            let placed: Vec<_> = glyph_run.positioned_glyphs().collect();
            let chars = glyph_chars(
                text,
                &run,
                glyph_run.style_index(),
                &mut taken,
                placed.len(),
            );
            for (glyph, ch) in placed.iter().zip(chars) {
                let gx = glyph.x + offset + advance_index as f32 * letter_spacing;
                advance_index += 1;
                if baseline.is_none() {
                    baseline = Some(glyph.y);
                }
                // Emoji of a color face draws as its picture, never outlines.
                if face.colour && !ch.is_whitespace() {
                    let pic =
                        emoji_of(fonts, ffmpeg_bin, ch, run.font_size()).map_err(|reason| {
                            TextError::EmojiRefused {
                                character: ch,
                                reason,
                            }
                        })?;
                    let scale = f64::from(run.font_size()) / f64::from(pic.units_per_em);
                    let px = f64::from(gx) + f64::from(pic.origin_x) * scale;
                    let bottom = f64::from(glyph.y) - f64::from(pic.origin_y) * scale;
                    pictures.push(Picture {
                        width: pic.width,
                        height: pic.height,
                        rgba: pic.rgba.clone(),
                        x: px as f32,
                        y: (bottom - f64::from(pic.height)) as f32,
                    });
                    continue;
                }
                let id = u16::try_from(glyph.id).map_err(|_| TextError::MissingCharacter(ch))?;
                let skew = run.synthesis().skew().unwrap_or(0.0);
                let embolden = run.synthesis().embolden();
                match runs.last_mut() {
                    Some(last)
                        if Arc::ptr_eq(&last.font, &face.bytes)
                            && last.index == face.index
                            && last.size == run.font_size()
                            && last.skew_degrees == skew
                            && last.embolden == embolden
                            && last.colour == glyph_run.style().brush =>
                    {
                        last.glyphs.push(Glyph {
                            id,
                            x: gx,
                            y: glyph.y,
                            ch,
                        });
                    }
                    _ => runs.push(Run {
                        font: face.bytes.clone(),
                        index: face.index,
                        size: run.font_size(),
                        skew_degrees: skew,
                        embolden,
                        colour: glyph_run.style().brush,
                        glyphs: vec![Glyph {
                            id,
                            x: gx,
                            y: glyph.y,
                            ch,
                        }],
                    }),
                }
                glyph_total += 1;
            }
        }
        lines.push(Line {
            left: offset,
            width: line_width,
            baseline: baseline.unwrap_or(height),
        });
        glyph_marks.push(glyph_total);
        picture_marks.push(pictures.len());
        height += line.metrics().line_height;
    }
    // Line height (T007): the first line stays where parley put it;
    // line `i` moves so its baseline sits exactly at
    // `first_baseline + i * declared`. The system never substitutes a
    // different separation silently.
    if let Some(declared) = style.line_height {
        if lines.len() > 1 {
            let first = lines[0].baseline;
            let old_last = lines.last().map(|l| l.baseline).unwrap_or(first);
            let mut start_glyph = 0usize;
            let mut start_pic = 0usize;
            for (i, line) in lines.iter_mut().enumerate() {
                let end_glyph = glyph_marks[i];
                let end_pic = picture_marks[i];
                let dy = first + i as f32 * declared - line.baseline;
                if dy != 0.0 {
                    shift_glyphs(&mut runs, start_glyph, end_glyph, dy);
                    for pic in &mut pictures[start_pic..end_pic] {
                        pic.y += dy;
                    }
                    line.baseline += dy;
                }
                start_glyph = end_glyph;
                start_pic = end_pic;
            }
            // The block grows with its last baseline: descent below it
            // is unchanged.
            height += lines.last().map(|l| l.baseline).unwrap_or(old_last) - old_last;
        }
    }
    Ok(Block {
        width: block_width,
        height,
        runs,
        pictures,
        lines,
    })
}

/// Shift flattened glyphs `[start, end)` vertically: runs merge across
/// lines, but each line's glyphs form one contiguous span in push order.
fn shift_glyphs(runs: &mut [Run], start: usize, end: usize, dy: f32) {
    let mut skipped = 0usize;
    for run in runs.iter_mut() {
        let n = run.glyphs.len();
        for (j, g) in run.glyphs.iter_mut().enumerate() {
            let flat = skipped + j;
            if flat >= start && flat < end {
                g.y += dy;
            }
        }
        skipped += n;
        if skipped >= end {
            break;
        }
    }
}

/// The characters of the `wanted` positioned glyphs of a glyph run: the
/// first character of each of its clusters after the `taken` ones
/// already given to an earlier glyph run, one entry per glyph (a
/// cluster's advance sits on its first glyph). Reversed for RTL runs.
fn glyph_chars(
    text: &str,
    run: &parley::Run<'_, [u8; 4]>,
    style: u16,
    taken: &mut BTreeMap<(usize, u16), usize>,
    wanted: usize,
) -> Vec<char> {
    let slot = taken.entry((run.text_range().start, style)).or_insert(0);
    let mut out: Vec<Vec<char>> = Vec::new();
    let mut count = 0;
    for cluster in run
        .clusters()
        .filter(|cluster| cluster.style_index() == style)
        .skip(*slot)
    {
        if count >= wanted {
            break;
        }
        *slot += 1;
        let ch = text
            .get(cluster.text_range())
            .and_then(|s| s.chars().next())
            .unwrap_or('\u{fffd}');
        let n = cluster.glyphs().count().max(1);
        count += n;
        out.push(vec![ch; n]);
    }
    if run.is_rtl() {
        out.reverse();
    }
    let flat: Vec<char> = out.into_iter().flatten().collect();
    if flat.len() >= wanted {
        flat[..wanted].to_vec()
    } else {
        let mut padded = flat;
        padded.resize(wanted, '\u{fffd}');
        padded
    }
}

/// The decoded emoji picture of `ch` via FFmpeg.
fn emoji_of(
    fonts: &mut Fonts,
    ffmpeg_bin: Option<&std::path::Path>,
    ch: char,
    size: f32,
) -> Result<Arc<EmojiPic>, String> {
    let Some(bin) = ffmpeg_bin else {
        return Err("no FFmpeg binary is available to decode it".to_string());
    };
    fonts.emoji_picture(bin, ch, size)
}

/// The run holding byte `offset`: the last run starting at or before
/// it. Offsets on a boundary belong to the run opening there. Binary
/// search keeps per-cluster lookups logarithmic, so shaping stays
/// linear in the run count.
fn run_of(bounds: &[(usize, usize)], offset: usize) -> usize {
    bounds
        .partition_point(|&(start, _)| start <= offset)
        .saturating_sub(1)
}

/// The characters of the `wanted` positioned glyphs of a glyph run,
/// each with its document run index: like [`glyph_chars`] but the
/// caller also learns which styled run each glyph sets from (for
/// per-run letter spacing, line heights, and refusal labels).
fn glyph_chars_runs(
    text: &str,
    run: &parley::Run<'_, [u8; 4]>,
    style: u16,
    taken: &mut BTreeMap<(usize, u16), usize>,
    wanted: usize,
    bounds: &[(usize, usize)],
) -> Vec<(char, usize)> {
    let slot = taken.entry((run.text_range().start, style)).or_insert(0);
    let mut out: Vec<Vec<(char, usize)>> = Vec::new();
    let mut count = 0;
    for cluster in run
        .clusters()
        .filter(|cluster| cluster.style_index() == style)
        .skip(*slot)
    {
        if count >= wanted {
            break;
        }
        *slot += 1;
        let range = cluster.text_range();
        let ch = text
            .get(range.clone())
            .and_then(|s| s.chars().next())
            .unwrap_or('\u{fffd}');
        let run_idx = run_of(bounds, range.start);
        let n = cluster.glyphs().count().max(1);
        count += n;
        out.push(vec![(ch, run_idx); n]);
    }
    if run.is_rtl() {
        out.reverse();
    }
    let flat: Vec<(char, usize)> = out.into_iter().flatten().collect();
    if flat.len() >= wanted {
        flat[..wanted].to_vec()
    } else {
        let mut padded = flat;
        padded.resize(wanted, ('\u{fffd}', run_of(bounds, run.text_range().start)));
        padded
    }
}

/// Lays out one paragraph of styled runs (Sprint 8 look, CAP-002/T002):
/// the run texts concatenate in order into one shaped paragraph, each
/// range painted with its own family try-order, size, weight, style,
/// color, and spacing; `direction` orders the runs bidirectionally.
/// Shaping, ligatures, positional forms, and line breaking follow the
/// same script rules as [`layout`]: long words never split mid-word
/// silently — they break at explicit soft breaks or overflow the wrap
/// width with glyphs unchanged. A run whose families all miss refuses
/// with [`TextError::NoFamilyRun`], a character none of its run's
/// families maps refuses with [`TextError::MissingCharacterRun`], and
/// a run shaping to no glyphs refuses with [`TextError::NoGlyphs`],
/// each naming the span; a missing first family falls through to the
/// next available family of that run.
pub fn layout_runs(
    fonts: &mut Fonts,
    runs: &[RunStyle],
    align: Align,
    direction: Direction,
    width: Option<f32>,
    ffmpeg_bin: Option<&std::path::Path>,
) -> Result<Block, TextError> {
    // A paragraph of no runs shapes nothing: refuse naming the first
    // span instead of painting silence.
    if runs.is_empty() {
        return Err(TextError::NoGlyphs { run: 0 });
    }
    // The shaped paragraph plus each run's byte range in it.
    let mut text = String::new();
    let mut bounds: Vec<(usize, usize)> = Vec::with_capacity(runs.len());
    for run in runs {
        let start = text.len();
        text.push_str(&run.text);
        bounds.push((start, text.len()));
    }
    // Each run's engine families with per-character fallback, exactly
    // like `layout`: a missing first family falls through to the next
    // name the machine or composition serves, and a run none serves
    // refuses naming the span.
    let mut lists: Vec<Vec<String>> = Vec::with_capacity(runs.len());
    let mut unmapped: Vec<char> = Vec::new();
    for (i, run) in runs.iter().enumerate() {
        let mut families = fonts
            .families_of(&run.families)
            .map_err(|_| TextError::NoFamilyRun { run: i })?;
        for ch in run.text.chars() {
            if ch.is_control() || ch.is_whitespace() {
                continue;
            }
            if fonts.families_map(&families, ch) || unmapped.contains(&ch) {
                continue;
            }
            match fonts.fall_back(ch) {
                Some(family) => {
                    if !families.contains(&family) {
                        families.push(family);
                    }
                }
                None => unmapped.push(ch),
            }
        }
        lists.push(families);
    }
    let FontsRef { context, layouts } = fonts.parts();
    let mut builder = layouts.ranged_builder(context, &text, 1.0, true);
    builder.set_base_direction(match direction {
        Direction::Ltr => BaseDirection::Ltr,
        Direction::Rtl => BaseDirection::Rtl,
        Direction::Auto => BaseDirection::Auto,
    });
    for ((run, families), (start, end)) in runs.iter().zip(lists.iter()).zip(bounds.iter().copied())
    {
        let range = start..end;
        let names: Vec<FontFamilyName> = families
            .iter()
            .map(|name| FontFamilyName::named(name))
            .collect();
        let weight = if run.weight.is_finite() {
            FontWeight::new(run.weight)
        } else {
            FontWeight::NORMAL
        };
        let italic = if run.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        builder.push(
            StyleProperty::FontFamily(FontFamily::List(names.into())),
            range.clone(),
        );
        builder.push(StyleProperty::FontSize(run.size), range.clone());
        builder.push(StyleProperty::FontWeight(weight), range.clone());
        builder.push(StyleProperty::FontStyle(italic), range.clone());
        builder.push(StyleProperty::Brush(run.colour), range);
    }
    let mut shaped = builder.build(&text);
    shaped.break_all_lines(width);
    // First pass: coverage per run plus unspaced line widths.
    let mut widths: Vec<f32> = Vec::new();
    for line in shaped.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            for cluster in run.clusters() {
                let ch = text
                    .get(cluster.text_range())
                    .and_then(|s| s.chars().next())
                    .unwrap_or('\u{fffd}');
                if ch.is_control() || ch.is_whitespace() {
                    continue;
                }
                let idx = run_of(&bounds, cluster.text_range().start);
                if !fonts.families_map(&lists[idx], ch) && !unmapped.contains(&ch) {
                    return Err(TextError::MissingCharacterRun {
                        run: idx,
                        character: ch,
                    });
                }
            }
        }
        let metrics = line.metrics();
        widths.push(metrics.advance - metrics.hanging_advance);
    }
    // Per-line spacing totals with a throwaway taken map (the main
    // walk below repeats the same walk with a fresh one, so the
    // mapping stays identical): each glyph advances its own run's
    // extra spacing, and the spaced width feeds alignment exactly as
    // the renderer draws it.
    let spacings: Vec<f32> = runs.iter().map(|r| r.letter_spacing.max(0.0)).collect();
    let mut spaced_widths: Vec<f32> = Vec::with_capacity(widths.len());
    {
        let mut taken: BTreeMap<(usize, u16), usize> = BTreeMap::new();
        for line in shaped.lines() {
            let mut extra = 0.0f32;
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let placed = glyph_run.positioned_glyphs().count();
                for (_, idx) in glyph_chars_runs(
                    &text,
                    &run,
                    glyph_run.style_index(),
                    &mut taken,
                    placed,
                    &bounds,
                ) {
                    extra += spacings[idx];
                }
            }
            spaced_widths.push(widths[spaced_widths.len()] + extra);
        }
    }
    let spaced_widest = spaced_widths.iter().copied().fold(0.0, f32::max);
    let block_width = match width {
        Some(w) => w,
        None => spaced_widest,
    };
    // Second pass: runs and emoji pictures with block positions, one
    // contiguous span per line for the line-height shift below.
    let mut out_runs: Vec<Run> = Vec::new();
    let mut pictures: Vec<Picture> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();
    let mut height = 0.0f32;
    let mut taken: BTreeMap<(usize, u16), usize> = BTreeMap::new();
    let mut glyph_marks: Vec<usize> = Vec::new();
    let mut picture_marks: Vec<usize> = Vec::new();
    let mut glyph_total = 0usize;
    // Runs contributing to each line, for per-line separation.
    let mut line_runs: Vec<Vec<usize>> = Vec::new();
    // Glyph and picture counts per document run, for the no-glyphs refusal.
    let mut run_counts: Vec<usize> = vec![0; runs.len()];
    for (line, line_width) in shaped.lines().zip(spaced_widths.iter().copied()) {
        let offset = match align {
            Align::Left => 0.0,
            Align::Center => (block_width - line_width) / 2.0,
            Align::Right => block_width - line_width,
        };
        let mut baseline: Option<f32> = None;
        let mut spacing_sum = 0.0f32;
        let mut seen: Vec<usize> = Vec::new();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let key = (run.font().data.id(), run.font().index);
            let Some(face) = fonts.face_for_blob(key.0, key.1) else {
                continue;
            };
            let placed: Vec<_> = glyph_run.positioned_glyphs().collect();
            let chars = glyph_chars_runs(
                &text,
                &run,
                glyph_run.style_index(),
                &mut taken,
                placed.len(),
                &bounds,
            );
            for (glyph, (ch, idx)) in placed.iter().zip(chars) {
                if !seen.contains(&idx) {
                    seen.push(idx);
                }
                let gx = glyph.x + offset + spacing_sum;
                spacing_sum += spacings[idx];
                if baseline.is_none() {
                    baseline = Some(glyph.y);
                }
                if face.colour && !ch.is_whitespace() {
                    let pic =
                        emoji_of(fonts, ffmpeg_bin, ch, run.font_size()).map_err(|reason| {
                            TextError::EmojiRefused {
                                character: ch,
                                reason,
                            }
                        })?;
                    let scale = f64::from(run.font_size()) / f64::from(pic.units_per_em);
                    let px = f64::from(gx) + f64::from(pic.origin_x) * scale;
                    let bottom = f64::from(glyph.y) - f64::from(pic.origin_y) * scale;
                    pictures.push(Picture {
                        width: pic.width,
                        height: pic.height,
                        rgba: pic.rgba.clone(),
                        x: px as f32,
                        y: (bottom - f64::from(pic.height)) as f32,
                    });
                    run_counts[idx] += 1;
                    continue;
                }
                // A shaped outline still belongs to its run's
                // families; anything else is the engine's own choice,
                // never drawn.
                if !lists[idx].contains(&face.family) {
                    return Err(TextError::MissingCharacterRun {
                        run: idx,
                        character: ch,
                    });
                }
                let id = u16::try_from(glyph.id).map_err(|_| TextError::MissingCharacterRun {
                    run: idx,
                    character: ch,
                })?;
                let skew = run.synthesis().skew().unwrap_or(0.0);
                let embolden = run.synthesis().embolden();
                match out_runs.last_mut() {
                    Some(last)
                        if Arc::ptr_eq(&last.font, &face.bytes)
                            && last.index == face.index
                            && last.size == run.font_size()
                            && last.skew_degrees == skew
                            && last.embolden == embolden
                            && last.colour == glyph_run.style().brush =>
                    {
                        last.glyphs.push(Glyph {
                            id,
                            x: gx,
                            y: glyph.y,
                            ch,
                        });
                    }
                    _ => out_runs.push(Run {
                        font: face.bytes.clone(),
                        index: face.index,
                        size: run.font_size(),
                        skew_degrees: skew,
                        embolden,
                        colour: glyph_run.style().brush,
                        glyphs: vec![Glyph {
                            id,
                            x: gx,
                            y: glyph.y,
                            ch,
                        }],
                    }),
                }
                glyph_total += 1;
                run_counts[idx] += 1;
            }
        }
        lines.push(Line {
            left: offset,
            width: line_width,
            baseline: baseline.unwrap_or(height),
        });
        line_runs.push(seen);
        glyph_marks.push(glyph_total);
        picture_marks.push(pictures.len());
        height += line.metrics().line_height;
    }
    // A run shaping to no glyphs and no pictures refuses naming the
    // span: an empty shaping pass never paints silently. Newlines
    // break lines without glyphs of their own, so a newline-only run
    // is the classic trigger; ordinary spaces shape into advances and
    // pass.
    for (i, run) in runs.iter().enumerate() {
        if run_counts[i] == 0 && !run.text.is_empty() {
            return Err(TextError::NoGlyphs { run: i });
        }
    }
    // Line separation (T002): the first line stays where parley put
    // it; each later line sits exactly its declared separation below
    // the previous one, where the declared separation is the tallest
    // run on either line (runs inherit the clip value, so a uniform
    // clip separation behaves like one declared height). Lines with no
    // declared run keep parley's natural separation.
    if lines.len() > 1 {
        let declared_gap = |line_idx: usize| -> Option<f32> {
            let mut tallest: Option<f32> = None;
            for idx in line_runs[line_idx]
                .iter()
                .chain(line_runs[line_idx - 1].iter())
            {
                if let Some(h) = runs[*idx].line_height {
                    tallest = Some(tallest.map_or(h, |t: f32| t.max(h)));
                }
            }
            tallest
        };
        let old_last = lines.last().map(|l| l.baseline).unwrap_or(0.0);
        let first = lines[0].baseline;
        let natural: Vec<f32> = lines
            .windows(2)
            .map(|w| w[1].baseline - w[0].baseline)
            .collect();
        let mut start_glyph = 0usize;
        let mut start_pic = 0usize;
        // Baselines accumulate: line i targets first + the running
        // sum of declared-or-natural gaps above it.
        let mut target = first;
        for i in 1..lines.len() {
            target += declared_gap(i).unwrap_or(natural[i - 1]);
            let dy = target - lines[i].baseline;
            let end_glyph = glyph_marks[i];
            let end_pic = picture_marks[i];
            if dy != 0.0 {
                shift_glyphs(&mut out_runs, start_glyph, end_glyph, dy);
                for pic in &mut pictures[start_pic..end_pic] {
                    pic.y += dy;
                }
                lines[i].baseline += dy;
            }
            start_glyph = end_glyph;
            start_pic = end_pic;
        }
        height += lines.last().map(|l| l.baseline).unwrap_or(old_last) - old_last;
    }
    Ok(Block {
        width: block_width,
        height,
        runs: out_runs,
        pictures,
        lines,
    })
}
