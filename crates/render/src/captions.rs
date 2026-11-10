//! Caption words through the normal layer stack (Sprint 22 captions,
//! T001/T002/T003 / CAP-001..CAP-005).
//!
//! Preparation pre-shapes every cue word with its fully resolved
//! style (track style plus the live per-word overrides), so drawing
//! reads the prepared blocks and pure film-time preset curves —
//! never the wall clock, never randomness, never the network. The
//! same inputs render the same pixels on every run, on every worker
//! count, on both backends.
//!
//! Cue and word times are clip-local seconds; the frame stage maps
//! film time through the clip start before this painter runs.

use crate::frame::ResolvedCaption;
use crate::render::{DecodedImage, RasterCache};
use notation::model::{
    CaptionClip, CaptionPosition, CaptionStyle, CaptionWordOverride, Composition,
};
use std::collections::HashMap;
use std::path::Path;

/// Pre-shaped caption word blocks (Sprint 22 captions, T001): one
/// shaped block per (clip, cue, word) slot, with an outline-colored
/// twin wherever the track draws an outline. Read-only during
/// drawing, so workers share them and reruns shape them identically.
#[derive(Debug, Default)]
pub(crate) struct CaptionLayouts {
    index: HashMap<(usize, usize, usize, bool), usize>,
    blocks: Vec<Option<layout::Block>>,
}

impl CaptionLayouts {
    /// Empty table with nothing shaped.
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(
        &mut self,
        doc: usize,
        cue: usize,
        word: usize,
        halo: bool,
        block: Option<layout::Block>,
    ) {
        let at = self.blocks.len();
        self.blocks.push(block);
        self.index.insert((doc, cue, word, halo), at);
    }

    /// The shaped block for one word slot: `None` when preparation
    /// never shaped this slot (never on valid derivations), inner
    /// `None` when the bitmap fallback owns the word (no resolvable
    /// family and no project family, exactly like `text`).
    pub fn get(
        &self,
        doc: usize,
        cue: usize,
        word: usize,
        halo: bool,
    ) -> Option<&Option<layout::Block>> {
        self.index
            .get(&(doc, cue, word, halo))
            .and_then(|&i| self.blocks.get(i))
    }

    /// True while no word was shaped at all.
    /// True while no word was shaped at all.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// One word's fully resolved paint (Sprint 22 captions, T002):
/// families in try order, pixel size, numeric weight, ink, opacity,
/// offset, scale, and rotation — track style with the live override
/// applied field by field.
#[derive(Debug, Clone)]
pub(crate) struct WordPaint {
    pub families: Option<Vec<String>>,
    pub size: f64,
    pub weight: f64,
    pub color: notation::model::Rgb,
    pub opacity: f64,
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
    pub rotation: f64,
}

impl WordPaint {
    /// Resolve one word's paint: every `Some` override field wins,
    /// everything else inherits the track style.
    pub fn resolve(style: &CaptionStyle, ov: Option<&CaptionWordOverride>) -> Self {
        Self {
            families: ov
                .and_then(|o| o.font_family.clone())
                .or_else(|| style.font_family.clone()),
            size: ov.and_then(|o| o.font_size).unwrap_or(style.font_size),
            weight: ov.and_then(|o| o.font_weight).unwrap_or(style.font_weight),
            color: ov.and_then(|o| o.color).unwrap_or(style.color),
            opacity: ov.and_then(|o| o.opacity).unwrap_or(style.opacity),
            dx: ov.and_then(|o| o.dx).unwrap_or(0.0),
            dy: ov.and_then(|o| o.dy).unwrap_or(0.0),
            scale: ov.and_then(|o| o.scale).unwrap_or(1.0),
            rotation: ov.and_then(|o| o.rotation).unwrap_or(0.0),
        }
        .sane()
    }

    /// Clamp garbage to drawable values without changing valid
    /// paints (validation already refused the garbage; the painter
    /// never invents a refusal mid-frame).
    fn sane(mut self) -> Self {
        if !self.size.is_finite() || self.size <= 0.0 {
            self.size = 48.0;
        }
        if !self.weight.is_finite() {
            self.weight = 400.0;
        }
        self.opacity = self.opacity.clamp(0.0, 1.0);
        if !self.scale.is_finite() || self.scale <= 0.0 {
            self.scale = 1.0;
        }
        if !self.dx.is_finite() {
            self.dx = 0.0;
        }
        if !self.dy.is_finite() {
            self.dy = 0.0;
        }
        if !self.rotation.is_finite() {
            self.rotation = 0.0;
        }
        self
    }
}

/// Families for one caption word (Sprint 22 captions, T002): the
/// explicit try-order list when written (fail-closed: a list nothing
/// resolves refuses at preparation naming the family), else the
/// project family, else the machine style fallback — exactly like
/// `text`, so open lists try in order and unknown lists never guess.
fn word_families(
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    paint: &WordPaint,
    word: &str,
) -> Result<Vec<String>, String> {
    match &paint.families {
        Some(names) => {
            if !fonts.resolves_any(names) {
                return Err(format!(
                    "render refused: captions word {word:?} names unknown font-family {:?}; no MP4 was written",
                    names.join(", ")
                ));
            }
            Ok(names.clone())
        }
        None => Ok(match project_family {
            Some(family) => vec![family.clone()],
            None => fonts
                .resolve_system_style(paint.weight >= 650.0, false)
                .into_iter()
                .collect(),
        }),
    }
}

/// Shape every word of every captions clip in one composition
/// (Sprint 22 captions, T001/T002): ink blocks in the resolved word
/// color plus outline-colored twins wherever the track draws an
/// outline. Returns true when the bitmap fallback owns at least one
/// word (no resolvable family and no project family, exactly like
/// `text`).
pub(crate) fn shape_caption_layouts(
    comp: &Composition,
    base: usize,
    fonts: &mut layout::Fonts,
    project_family: &Option<String>,
    ffmpeg_bin: &Path,
    layouts: &mut CaptionLayouts,
) -> Result<bool, String> {
    let mut fallback = false;
    for (local, clip) in comp.clips.iter().enumerate() {
        let notation::model::Clip::Caption(cc) = clip else {
            continue;
        };
        if cc.hidden {
            continue;
        }
        let doc = base + local;
        for (cue_idx, cue) in cc.cues.iter().enumerate() {
            for (word_idx, word) in cue.words.iter().enumerate() {
                let ov = cc
                    .overrides
                    .iter()
                    .find(|o| o.cue == cue_idx && o.word == word_idx);
                let paint = WordPaint::resolve(&cc.style, ov);
                let families = word_families(fonts, project_family, &paint, &word.text)?;
                let style = layout::Style {
                    families,
                    size: paint.size as f32,
                    weight: paint.weight as f32,
                    italic: false,
                    align: layout::Align::Left,
                    colour: [paint.color.r, paint.color.g, paint.color.b, 255],
                    line_height: None,
                    letter_spacing: None,
                };
                match layout::layout(fonts, &word.text, &style, None, Some(ffmpeg_bin)) {
                    Ok(block) => layouts.insert(doc, cue_idx, word_idx, false, Some(block)),
                    Err(layout::TextError::NoFamily) if project_family.is_none() => {
                        layouts.insert(doc, cue_idx, word_idx, false, None);
                        fallback = true;
                    }
                    Err(e) => {
                        return Err(format!(
                            "render refused: captions word {:?} cannot be laid out ({e}); no MP4 was written",
                            word.text
                        ));
                    }
                }
                // The outline twin paints the same glyphs in the
                // outline ink; halo passes draw it at neighbor
                // offsets under the ink.
                if cc.style.outline_width >= 0.5 {
                    let halo = layout::Style {
                        colour: [
                            cc.style.outline_color.r,
                            cc.style.outline_color.g,
                            cc.style.outline_color.b,
                            255,
                        ],
                        ..style.clone()
                    };
                    match layout::layout(fonts, &word.text, &halo, None, Some(ffmpeg_bin)) {
                        Ok(block) => layouts.insert(doc, cue_idx, word_idx, true, Some(block)),
                        Err(layout::TextError::NoFamily) if project_family.is_none() => {
                            layouts.insert(doc, cue_idx, word_idx, true, None);
                            fallback = true;
                        }
                        Err(e) => {
                            return Err(format!(
                                "render refused: captions word {:?} cannot be laid out ({e}); no MP4 was written",
                                word.text
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(fallback)
}

/// Decode one subject matte PNG into the shared image table
/// (Sprint 22 captions, T003): project-relative still PNG, decoded
/// with the `png` crate (no FFmpeg, no network), keyed by its
/// absolute path like every prepared still. Validation already
/// refused missing and undecodable mattes; a file lost after
/// validation refuses here naming the file, never a partial frame.
pub(crate) fn prepare_caption_mattes(
    comp: &Composition,
    folder: &Path,
    images: &mut Vec<DecodedImage>,
) -> Result<(), String> {
    for clip in &comp.clips {
        let notation::model::Clip::Caption(cc) = clip else {
            continue;
        };
        let Some(matte) = &cc.matte else { continue };
        let abs = folder.join(matte);
        if images.iter().any(|d| d.path == abs) {
            continue;
        }
        images.push(decode_matte_png(&abs, matte)?);
    }
    Ok(())
}

/// Decode one still PNG matte to straight RGBA bytes plus its size.
pub(crate) fn decode_matte_png(abs: &Path, rel: &str) -> Result<DecodedImage, String> {
    let file = std::fs::File::open(abs).map_err(|e| {
        format!("render refused: captions matte {rel:?} is no longer readable ({e}); no MP4 was written")
    })?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| {
        format!("render refused: captions matte {rel:?} is not a readable PNG picture ({e}); no MP4 was written")
    })?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w == 0 || h == 0 {
        return Err(format!(
            "render refused: captions matte {rel:?} has no pixels; no MP4 was written"
        ));
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| {
        format!(
            "render refused: captions matte {rel:?} cannot be decoded ({e}); no MP4 was written"
        )
    })?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(w as usize * h as usize * 4);
            for px in buf.chunks(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(w as usize * h as usize * 4);
            for px in buf.chunks(2) {
                out.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(w as usize * h as usize * 4);
            for px in buf.iter() {
                out.extend_from_slice(&[*px, *px, *px, 255]);
            }
            out
        }
        png::ColorType::Indexed => {
            return Err(format!(
                "render refused: captions matte {rel:?} decoded to an unsupported palette form; no MP4 was written"
            ));
        }
    };
    Ok(DecodedImage::from_rgba(abs.to_path_buf(), w, h, rgba))
}

/// Fill one clipped rectangle with alpha (Sprint 22 captions, T002):
/// cue backings, highlight pills, and underline bars share this
/// deterministic source-over fill.
fn fill_rect_alpha(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    color: (u8, u8, u8),
    alpha: f64,
) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    if a == 0 {
        return;
    }
    let i0 = (x0.floor() as i64).max(0).min(cw as i64);
    let i1 = (x1.ceil() as i64).max(0).min(cw as i64);
    let j0 = (y0.floor() as i64).max(0).min(chh as i64);
    let j1 = (y1.ceil() as i64).max(0).min(chh as i64);
    if i1 <= i0 || j1 <= j0 {
        return;
    }
    for j in j0..j1 {
        let row = (j as u32 * cw) as usize * 3;
        for i in i0..i1 {
            let o = row + (i as usize) * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            let inv = 255 - a;
            frame[o] = ((u32::from(color.0) * a + u32::from(frame[o]) * inv + 127) / 255) as u8;
            frame[o + 1] =
                ((u32::from(color.1) * a + u32::from(frame[o + 1]) * inv + 127) / 255) as u8;
            frame[o + 2] =
                ((u32::from(color.2) * a + u32::from(frame[o + 2]) * inv + 127) / 255) as u8;
        }
    }
}

/// Composite one straight-RGBA matte over the frame with cover fit
/// (Sprint 22 captions, T003): the matte scales to cover the canvas
/// (nearest-neighbor, center-cropped) and blends source-over, so
/// opaque matte pixels cover caption pixels and transparent ones
/// show through. Pure function of the matte bytes and the canvas.
pub(crate) fn composite_matte_cover(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    rgba: &[u8],
    mw: u32,
    mh: u32,
) {
    if mw == 0 || mh == 0 || cw == 0 || chh == 0 {
        return;
    }
    if rgba.len() < mw as usize * mh as usize * 4 {
        return;
    }
    // Cover scale: the matte fills the canvas, cropping the excess.
    let sx = cw as f64 / mw as f64;
    let sy = chh as f64 / mh as f64;
    let s = sx.max(sy);
    let dw = (mw as f64 * s).ceil().max(1.0);
    let dh = (mh as f64 * s).ceil().max(1.0);
    let ox = (dw - cw as f64) / 2.0;
    let oy = (dh - chh as f64) / 2.0;
    for j in 0..chh {
        let row = (j * cw) as usize * 3;
        let my = (((j as f64 + oy) / s).floor() as i64).clamp(0, mh as i64 - 1) as u32;
        for i in 0..cw {
            let mx = (((i as f64 + ox) / s).floor() as i64).clamp(0, mw as i64 - 1) as u32;
            let ms = ((my * mw + mx) as usize) * 4;
            let pa = u32::from(rgba[ms + 3]);
            if pa == 0 {
                continue;
            }
            let o = row + (i as usize) * 3;
            if o + 2 >= frame.len() {
                continue;
            }
            if pa == 255 {
                frame[o] = rgba[ms];
                frame[o + 1] = rgba[ms + 1];
                frame[o + 2] = rgba[ms + 2];
                continue;
            }
            let inv = 255 - pa;
            frame[o] = ((u32::from(rgba[ms]) * pa + u32::from(frame[o]) * inv + 127) / 255) as u8;
            frame[o + 1] =
                ((u32::from(rgba[ms + 1]) * pa + u32::from(frame[o + 1]) * inv + 127) / 255) as u8;
            frame[o + 2] =
                ((u32::from(rgba[ms + 2]) * pa + u32::from(frame[o + 2]) * inv + 127) / 255) as u8;
        }
    }
}

/// One laid-out cue line: word slots with their left edges and the
/// line box, in canvas pixels.
struct CueLine {
    words: Vec<(usize, f64)>,
    width: f64,
    height: f64,
}

/// Lay out the visible words into wrapped lines (Sprint 22 captions,
/// T002): greedy word wrap at the canvas width with `margin` side
/// gutters, capped at `max_lines` (overflow words stay on the last
/// line rather than vanishing). Word widths come from the shaped
/// blocks (fallback words measure through the bitmap face), so
/// painted pixels and line boxes agree.
fn wrap_cue(
    visible: &[usize],
    widths: &[f64],
    heights: &[f64],
    avail: f64,
    max_lines: u32,
    size: f64,
) -> Vec<CueLine> {
    let mut lines: Vec<CueLine> = Vec::new();
    let mut cur_words: Vec<(usize, f64)> = Vec::new();
    let mut cur_w = 0.0f64;
    let mut cur_h = 0.0f64;
    let gap = size * 0.3;
    for &wi in visible {
        let w = widths[wi];
        let h = heights[wi];
        let need = if cur_words.is_empty() {
            w
        } else {
            cur_w + gap + w
        };
        if !cur_words.is_empty() && need > avail {
            lines.push(CueLine {
                words: std::mem::take(&mut cur_words),
                width: cur_w,
                height: cur_h,
            });
            cur_w = 0.0;
            cur_h = 0.0;
        }
        let x = if cur_words.is_empty() {
            0.0
        } else {
            cur_w + gap
        };
        cur_words.push((wi, x));
        cur_w = x + w;
        cur_h = cur_h.max(h);
    }
    if !cur_words.is_empty() {
        lines.push(CueLine {
            words: cur_words,
            width: cur_w,
            height: cur_h,
        });
    }
    // The line cap never drops words: overflow lines merge into the
    // last kept line (re-xed in order), so every visible word paints.
    let cap = max_lines.max(1) as usize;
    while lines.len() > cap {
        let extra = lines.pop().unwrap();
        let last = lines.last_mut().unwrap();
        let mut x = last.width + gap;
        for (wi, _) in extra.words {
            last.words.push((wi, x));
            x += widths[wi] + gap;
        }
        last.width = x - gap;
        last.height = last.height.max(extra.height);
    }
    lines
}

/// Draw one resolved caption cue (Sprint 22 captions, T001/T002):
/// the active cue's spoken words through the shaped blocks with
/// track style, per-word overrides, and pure film-time presets; the
/// subject matte covers the words when the behind-subject order
/// rides the item (T003). Nothing draws outside the clip window —
/// the frame stage gates that — and empty cues draw nothing.
pub(crate) fn draw_caption(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    layouts: &CaptionLayouts,
    images: &[DecodedImage],
    rc: &ResolvedCaption,
) -> Result<(), String> {
    use notation::captions::{
        active_word, blur_passes, entrance_state, exit_state, highlight_state, HighlightMark,
        ENTRANCE_PRESETS, EXIT_PRESETS, HIGHLIGHT_PRESETS,
    };
    use notation::model::CaptionCue;
    if cw == 0 || chh == 0 {
        return Ok(());
    }
    let cue = CaptionCue {
        start: rc.cue_start,
        end: rc.cue_end,
        words: rc.words.clone(),
        line: 0,
        column: 1,
    };
    let t = rc.local_t;
    // Validation refused unknown presets before frame one; the
    // painter treats any other spelling as `none` (resting words),
    // so words never silently vanish off the validated path.
    let entrance = rc
        .entrance
        .as_deref()
        .filter(|n| ENTRANCE_PRESETS.contains(n))
        .unwrap_or("none");
    let highlight = rc
        .highlight
        .as_deref()
        .filter(|n| HIGHLIGHT_PRESETS.contains(n))
        .unwrap_or("none");
    let exit = rc
        .exit
        .as_deref()
        .filter(|n| EXIT_PRESETS.contains(n))
        .unwrap_or("none");
    let (ea, edx, edy, es) = entrance_state(entrance, &cue, t);
    let (xa, xdx, xdy, xs) = exit_state(exit, &cue, t);
    let cue_alpha = (ea * xa * rc.opacity * rc.style.opacity).clamp(0.0, 1.0);
    let cue_dx = edx + xdx;
    let cue_dy = edy + xdy;
    let cue_scale = es * xs;
    // Spoken-word addressing (T002 highlight): the word holding the
    // film time, plus which words it already passed.
    let spoken = active_word(&cue, t);
    // Size animation rides as a draw-time scale over the authored
    // shaping (shaping runs once at preparation).
    let size_ratio = if rc.style.font_size > 0.0 {
        rc.size / rc.style.font_size
    } else {
        1.0
    };
    // Visible words: the cue's words at or past their start
    // (progressive reveal by word timing). Unstarted words hide.
    let mut visible: Vec<usize> = Vec::new();
    for (i, w) in rc.words.iter().enumerate() {
        if t >= w.start {
            visible.push(i);
        }
    }
    // A fully transparent layer (motion or track opacity zero)
    // hides the words and the matte alike; an active cue with no
    // spoken word yet still carries the matte cover.
    if cue_alpha <= 0.0 {
        return Ok(());
    }
    // Per-word geometry from the shaped blocks (bitmap fallback
    // words measure through the fallback face, exactly like text).
    let fallback_face = glyph::font::Face::fallback();
    let mut widths = vec![0.0f64; rc.words.len()];
    let mut heights = vec![0.0f64; rc.words.len()];
    for &wi in &visible {
        let ov = rc.overrides.iter().find(|o| o.word == wi);
        let paint = WordPaint::resolve(&rc.style, ov);
        let scale = (paint.scale * size_ratio * cue_scale).max(1e-9);
        match layouts.get(rc.doc, rc.cue, wi, false) {
            Some(Some(block)) => {
                widths[wi] = block.width as f64 * scale;
                heights[wi] = block.height as f64 * scale;
            }
            _ => {
                let laid = fallback_face.layout(&rc.words[wi].text, paint.size, None);
                widths[wi] = laid.width as f64 * scale;
                heights[wi] = laid.line_height as f64 * scale;
            }
        }
    }
    let margin_x = (cw as f64 * 0.06).max(4.0);
    let margin_bottom = (chh as f64 * 0.05).max(4.0);
    let avail = (cw as f64 - 2.0 * margin_x).max(1.0);
    // The rail wraps to the track `max_lines`; embed paints one
    // line (overflow words stay on it rather than vanishing).
    let line_cap = match rc.style.position {
        CaptionPosition::Rail => rc.style.max_lines.max(1),
        CaptionPosition::Embed => 1,
    };
    let lines = wrap_cue(&visible, &widths, &heights, avail, line_cap, rc.size);
    let leading = rc.size * 0.22;
    let total_h: f64 = lines.iter().map(|l| l.height).sum::<f64>()
        + leading * (lines.len().saturating_sub(1) as f64);
    // Cue origin: bottom rail centers every line; embed paints from
    // the evaluated `x`/`y` downward.
    let rail_top = chh as f64 - margin_bottom - total_h;
    // Cue backing first (under every word, over the plate).
    if let Some(bg) = rc.style.background {
        let (bx0, bx1) = match rc.style.position {
            CaptionPosition::Rail => (margin_x * 0.5, cw as f64 - margin_x * 0.5),
            CaptionPosition::Embed => {
                let w = lines.iter().map(|l| l.width).fold(0.0f64, f64::max);
                (rc.x, rc.x + w)
            }
        };
        let pad = rc.size * 0.2;
        let (by0, by1) = match rc.style.position {
            CaptionPosition::Rail => (rail_top - pad, rail_top + total_h + pad),
            CaptionPosition::Embed => (rc.y - pad, rc.y + total_h + pad),
        };
        fill_rect_alpha(
            frame,
            cw,
            chh,
            bx0 - pad,
            by0,
            bx1 + pad,
            by1,
            (bg.r, bg.g, bg.b),
            cue_alpha,
        );
    }
    let mut ly = match rc.style.position {
        CaptionPosition::Rail => rail_top,
        CaptionPosition::Embed => rc.y,
    };
    for line in &lines {
        let lx0 = match rc.style.position {
            CaptionPosition::Rail => (cw as f64 - line.width) / 2.0,
            CaptionPosition::Embed => rc.x,
        };
        for (wi, wx) in &line.words {
            let w = &rc.words[*wi];
            let ov = rc.overrides.iter().find(|o| o.word == *wi);
            let paint = WordPaint::resolve(&rc.style, ov);
            let is_spoken = spoken == Some(*wi);
            let is_past = spoken.is_some_and(|s| *wi < s);
            let (hscale, hop, mark) = highlight_state(highlight, is_spoken, is_past);
            let alpha = (cue_alpha * paint.opacity * hop).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let scale = (paint.scale * size_ratio * cue_scale * hscale).max(1e-9);
            let x = lx0 + wx + paint.dx + cue_dx;
            let y = ly + paint.dy + cue_dy;
            // Highlight marks paint under the word (pill, bar, halo).
            match mark {
                HighlightMark::Underline => {
                    let bh = heights[*wi];
                    fill_rect_alpha(
                        frame,
                        cw,
                        chh,
                        x,
                        y + bh * 0.92,
                        x + widths[*wi],
                        y + bh * 0.92 + (rc.size * 0.06).max(2.0),
                        (paint.color.r, paint.color.g, paint.color.b),
                        alpha,
                    );
                }
                HighlightMark::Background => {
                    let pad = rc.size * 0.12;
                    fill_rect_alpha(
                        frame,
                        cw,
                        chh,
                        x - pad,
                        y - pad * 0.5,
                        x + widths[*wi] + pad,
                        y + heights[*wi] + pad * 0.5,
                        (paint.color.r, paint.color.g, paint.color.b),
                        0.35 * alpha,
                    );
                }
                HighlightMark::Glow => {
                    if let Some(Some(block)) = layouts.get(rc.doc, rc.cue, *wi, false) {
                        draw_word_passes(
                            frame,
                            cw,
                            chh,
                            raster,
                            block,
                            x,
                            y,
                            scale,
                            &[(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)],
                            0.45 * alpha,
                        )?;
                    }
                }
                HighlightMark::None => {}
            }
            // Entrance/exit blur smear (extra low-alpha offset
            // passes converging to the sharp word).
            let entering = !entrance_state_done(entrance, &cue, t);
            let exiting = !exit_state_done(exit, &cue, t);
            let smear = if entering {
                blur_passes(entrance, &cue, t, false)
            } else if exiting {
                blur_passes(exit, &cue, t, true)
            } else {
                Vec::new()
            };
            if !smear.is_empty() {
                if let Some(Some(block)) = layouts.get(rc.doc, rc.cue, *wi, false) {
                    let offsets: Vec<(f64, f64)> = smear.iter().map(|(dy, _)| (0.0, *dy)).collect();
                    let a = smear.first().map(|(_, a)| *a * alpha).unwrap_or(0.0);
                    draw_word_passes(frame, cw, chh, raster, block, x, y, scale, &offsets, a)?;
                }
            }
            // Outline halo under the ink (8 neighbors, outline ink).
            if rc.style.outline_width >= 0.5 {
                let ow = rc.style.outline_width;
                let halo = layouts.get(rc.doc, rc.cue, *wi, true);
                let block = match halo {
                    Some(Some(b)) => Some(b),
                    _ => match layouts.get(rc.doc, rc.cue, *wi, false) {
                        Some(Some(b)) => Some(b),
                        _ => None,
                    },
                };
                if let Some(block) = block {
                    let n = [
                        (ow, 0.0),
                        (-ow, 0.0),
                        (0.0, ow),
                        (0.0, -ow),
                        (ow, ow),
                        (-ow, ow),
                        (ow, -ow),
                        (-ow, -ow),
                    ];
                    draw_word_passes(frame, cw, chh, raster, block, x, y, scale, &n, alpha)?;
                }
            }
            // The ink itself (rotated words ride the layer path).
            draw_word_ink(
                frame,
                cw,
                chh,
                raster,
                layouts,
                &fallback_face,
                rc,
                *wi,
                w,
                &paint,
                x,
                y,
                scale,
                alpha,
            )?;
        }
        ly += line.height + leading;
    }
    // Behind-subject cover (T003): the matte paints right after the
    // words at the caption order, so subject pixels cover caption
    // pixels where the matte is opaque and captions show through
    // where it is transparent. Later items paint over the matte.
    if rc.behind_subject {
        draw_matte(images, frame, cw, chh, rc)?;
    }
    Ok(())
}

/// True while the entrance preset holds the resting words (past its
/// window, or `none`): no smear passes apply.
fn entrance_state_done(name: &str, cue: &notation::model::CaptionCue, t: f64) -> bool {
    use notation::captions::entrance_window;
    if name == "none" {
        return true;
    }
    let win = entrance_window(cue);
    win <= 0.0 || t >= cue.start + win
}

/// True while the exit preset holds the resting words (before its
/// window, or `none`): no smear passes apply.
fn exit_state_done(name: &str, cue: &notation::model::CaptionCue, t: f64) -> bool {
    use notation::captions::exit_window;
    if name == "none" {
        return true;
    }
    let win = exit_window(cue);
    win <= 0.0 || t < cue.end - win
}

/// Draw one word's block at neighbor offsets into the frame
/// (halos, smears, glows): each pass is one shaped draw at
/// `(x + dx, y + dy)` scaled by `s` at `alpha`.
fn draw_word_passes(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    block: &layout::Block,
    x: f64,
    y: f64,
    s: f64,
    offsets: &[(f64, f64)],
    alpha: f64,
) -> Result<(), String> {
    if alpha <= 0.0 || !s.is_finite() || s <= 0.0 {
        return Ok(());
    }
    for (dx, dy) in offsets {
        // Faded and multi-glyph blocks ride the layer path (exactly
        // like `draw_text_block`); sharp opaque single runs blit
        // directly.
        if alpha < 1.0 || !block.pictures.is_empty() {
            let mut layer = vec![0u8; cw as usize * chh as usize * 4];
            crate::render::draw_block_layer(
                &mut layer,
                cw,
                chh,
                raster,
                block,
                x + dx,
                y + dy,
                s,
                alpha.clamp(0.0, 1.0),
            )?;
            if let Some(bbox) = crate::render::layer_ink_bbox(&layer, cw, chh) {
                crate::render::composite_layer_bbox(
                    frame,
                    cw,
                    chh,
                    &layer,
                    bbox,
                    x + dx,
                    y + dy,
                    0.0,
                );
            }
        } else {
            crate::render::draw_block_scaled(frame, cw, chh, raster, block, x + dx, y + dy, s)?;
        }
    }
    Ok(())
}

/// Draw one word's ink: the shaped block (or the bitmap fallback
/// face when preparation shaped nothing), honoring per-word rotation
/// through the layer path exactly like rotated text.
#[allow(clippy::too_many_arguments)]
fn draw_word_ink(
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    raster: &mut RasterCache,
    layouts: &CaptionLayouts,
    fallback_face: &glyph::font::Face,
    rc: &ResolvedCaption,
    wi: usize,
    w: &notation::model::CaptionWord,
    paint: &WordPaint,
    x: f64,
    y: f64,
    s: f64,
    alpha: f64,
) -> Result<(), String> {
    if alpha <= 0.0 {
        return Ok(());
    }
    match layouts.get(rc.doc, rc.cue, wi, false) {
        Some(Some(block)) => {
            if paint.rotation == 0.0 || !paint.rotation.is_finite() {
                if alpha >= 1.0 && block.pictures.is_empty() {
                    crate::render::draw_block_scaled(frame, cw, chh, raster, block, x, y, s)?;
                } else {
                    let mut layer = vec![0u8; cw as usize * chh as usize * 4];
                    crate::render::draw_block_layer(
                        &mut layer,
                        cw,
                        chh,
                        raster,
                        block,
                        x,
                        y,
                        s,
                        alpha.clamp(0.0, 1.0),
                    )?;
                    if let Some(bbox) = crate::render::layer_ink_bbox(&layer, cw, chh) {
                        crate::render::composite_layer_bbox(
                            frame, cw, chh, &layer, bbox, x, y, 0.0,
                        );
                    }
                }
            } else {
                let mut layer = vec![0u8; cw as usize * chh as usize * 4];
                crate::render::draw_block_layer(
                    &mut layer,
                    cw,
                    chh,
                    raster,
                    block,
                    x,
                    y,
                    s,
                    alpha.clamp(0.0, 1.0),
                )?;
                if let Some(bbox) = crate::render::layer_ink_bbox(&layer, cw, chh) {
                    crate::render::composite_layer_bbox(
                        frame,
                        cw,
                        chh,
                        &layer,
                        bbox,
                        x,
                        y,
                        paint.rotation,
                    );
                }
            }
            let _ = fallback_face;
            Ok(())
        }
        _ => {
            // Bitmap fallback (no resolvable family and no project
            // family): the built-in face draws the word, exactly like
            // fallback text, with rotation and fading through the
            // layer path.
            let laid = fallback_face.layout(&w.text, paint.size, None);
            let color = (paint.color.r, paint.color.g, paint.color.b);
            if (paint.rotation == 0.0 || !paint.rotation.is_finite()) && alpha >= 1.0 {
                for g in laid.lines.iter().flat_map(|l| l.glyphs.iter()) {
                    fallback_face.blit(
                        frame,
                        cw,
                        chh,
                        g.ch,
                        paint.size * s,
                        x + g.x as f64 * s,
                        y + laid.ascent as f64 * s,
                        color,
                        notation::model::FontStyle::Regular,
                    );
                }
                return Ok(());
            }
            // Faded or rotated fallback words: coverage lands in a
            // scratch layer through the same layer blit the shaped
            // path uses (coverage times the word alpha), then the
            // ink box composites with the word rotation.
            let mut layer = vec![0u8; cw as usize * chh as usize * 4];
            for g in laid.lines.iter().flat_map(|l| l.glyphs.iter()) {
                fallback_face.blit_gid_layer(
                    &mut layer,
                    cw,
                    chh,
                    fallback_face.gid_of(g.ch),
                    g.ch,
                    paint.size * s,
                    x + g.x * s,
                    y + laid.ascent as f64 * s,
                    color,
                    notation::model::FontStyle::Regular,
                    alpha.clamp(0.0, 1.0),
                );
            }
            if let Some(bbox) = crate::render::layer_ink_bbox(&layer, cw, chh) {
                crate::render::composite_layer_bbox(
                    frame,
                    cw,
                    chh,
                    &layer,
                    bbox,
                    x,
                    y,
                    paint.rotation,
                );
            }
            Ok(())
        }
    }
}

/// Paint the behind-subject matte over the frame (Sprint 22
/// captions, T003): the prepared still PNG covers the words where
/// opaque. A matte lost after preparation refuses naming the file —
/// never a silent show-through.
fn draw_matte(
    images: &[DecodedImage],
    frame: &mut [u8],
    cw: u32,
    chh: u32,
    rc: &ResolvedCaption,
) -> Result<(), String> {
    let Some(matte) = &rc.matte else {
        return Ok(());
    };
    let Some(decoded) = images
        .iter()
        .find(|d| Path::new(&d.path).ends_with(matte.as_str()) || d.path.ends_with(matte.as_str()))
    else {
        return Err(format!(
            "render refused: captions matte {matte:?} was not prepared; no MP4 was written"
        ));
    };
    composite_matte_cover(frame, cw, chh, &decoded.rgba, decoded.width, decoded.height);
    Ok(())
}

/// Word colors of one cue for the Studio inspector (Sprint 22
/// captions, INT-003): the resolved ink of every visible word, so
/// the inspector readout and the painter agree.
pub fn inspector_words(cc: &CaptionClip, cue_idx: usize) -> Vec<(String, (u8, u8, u8))> {
    let Some(cue) = cc.cues.get(cue_idx) else {
        return Vec::new();
    };
    cue.words
        .iter()
        .enumerate()
        .map(|(wi, w)| {
            let ov = cc
                .overrides
                .iter()
                .find(|o| o.cue == cue_idx && o.word == wi);
            let paint = WordPaint::resolve(&cc.style, ov);
            (
                w.text.clone(),
                (paint.color.r, paint.color.g, paint.color.b),
            )
        })
        .collect::<Vec<_>>()
}

/// Active cue readout for the Studio inspector (Sprint 22 captions,
/// INT-003): the 1-based cue number, its window, and its words at
/// one film time through the same seek the painter uses. `None`
/// while no cue holds the playhead, so the inspector never invents
/// words.
pub fn inspector_cue(cc: &CaptionClip, film_t: f64) -> Option<(usize, f64, f64, Vec<String>)> {
    let local = (film_t - cc.start).max(0.0);
    if !film_t.is_finite() {
        return None;
    }
    let idx = notation::captions::active_cue(&cc.cues, local)?;
    let cue = &cc.cues[idx];
    Some((
        idx + 1,
        cue.start,
        cue.end,
        cue.words.iter().map(|w| w.text.clone()).collect(),
    ))
}
