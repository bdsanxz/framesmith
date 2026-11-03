//! Project-font text support (T011 / CAP-008).
//!
//! Pure-std font handling with no external crates:
//!
//! * `sniff_kind` / `check_supported` classify raw font bytes so validation
//!   can refuse unsupported formats with a diagnostic instead of silently
//!   substituting another font.
//! * `Face` rasterizes glyphs on the CPU: TrueType / OpenType-`glyf`
//!   outlines (including `ttcf` collections) via a small `glyf` parser and
//!   a 4x4-supersampled even-odd filler, or a built-in 5x7 bitmap face
//!   used only when the composition declares no `font` at all (announced
//!   by render, never silent).
//!
//! Sprint 1 text attributes: the project font file (first declared `font`
//! wins; weight is embodied by the chosen file), `size`, `color`,
//! position (`x`/`y`, or `position="X,Y"`), wrapping (`wrap`), and line
//! alignment (`align`). `line-height` / `letter-spacing` stay refused
//! under `E_UNSUPPORTED` by validation.

use std::cell::RefCell;
use std::collections::HashMap;

/// Raw magic classification of font bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    /// `0x00010000`, `true`, `typ1`: outlines may still be CFF (checked later).
    TrueType,
    /// `OTTO`: outlines may be CFF (checked later).
    OpenType,
    /// `ttcf`: collection; the first `glyf` face is used.
    Collection,
    Woff,
    Woff2,
    Unknown,
}

/// Classify font bytes by magic number.
pub fn sniff_kind(bytes: &[u8]) -> FontKind {
    if bytes.len() < 4 {
        return FontKind::Unknown;
    }
    match &bytes[0..4] {
        [0x00, 0x01, 0x00, 0x00] | [b't', b'r', b'u', b'e'] | [b't', b'y', b'p', b'1'] => {
            FontKind::TrueType
        }
        [b'O', b'T', b'T', b'O'] => FontKind::OpenType,
        [b't', b't', b'c', b'f'] => FontKind::Collection,
        [b'w', b'O', b'F', b'F'] => FontKind::Woff,
        [b'w', b'O', b'F', b'2'] => FontKind::Woff2,
        _ => FontKind::Unknown,
    }
}

/// Refuse anything the CPU renderer cannot draw: not a font at all,
/// WOFF/WOFF2 (compressed wrappers), CFF outlines (`CFF ` table instead
/// of `glyf`), or a `glyf` font missing a required table.
pub fn check_supported(bytes: &[u8]) -> Result<(), String> {
    match sniff_kind(bytes) {
        FontKind::Woff | FontKind::Woff2 => {
            return Err("WOFF/WOFF2 web fonts are not supported; use a TrueType (.ttf) or OpenType (.otf) font with TrueType outlines".to_string());
        }
        FontKind::Unknown => {
            return Err("the file is not a recognized font (expected TrueType/OpenType with `glyf` outlines or a `ttcf` collection)".to_string());
        }
        FontKind::TrueType | FontKind::OpenType | FontKind::Collection => {}
    }
    match TtFace::parse(bytes) {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

pub(crate) fn be16(b: &[u8], off: usize) -> Option<u16> {
    b.get(off..off + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
}

pub(crate) fn be32(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn bei16(b: &[u8], off: usize) -> Option<i16> {
    be16(b, off).map(|v| v as i16)
}

/// One sfnt table directory entry.
pub(crate) struct TableEntry {
    pub(crate) tag: u32,
    pub(crate) off: usize,
    pub(crate) len: usize,
}

pub(crate) fn table_dir(data: &[u8], base: usize) -> Option<Vec<TableEntry>> {
    let num = be16(data, base + 4)? as usize;
    let mut out = Vec::with_capacity(num);
    for i in 0..num {
        let o = base + 12 + i * 16;
        let tag = be32(data, o)?;
        let off = be32(data, o + 8)? as usize;
        let len = be32(data, o + 12)? as usize;
        out.push(TableEntry { tag, off, len });
    }
    Some(out)
}

pub(crate) fn find_table(dir: &[TableEntry], tag: &[u8; 4]) -> Option<(usize, usize)> {
    let want = u32::from_be_bytes(*tag);
    dir.iter()
        .find(|e| e.tag == want)
        .map(|e| (e.off, e.len))
}

pub(crate) fn tag(b0: u8, b1: u8, b2: u8, b3: u8) -> [u8; 4] {
    [b0, b1, b2, b3]
}

/// A parsed TrueType-outline face borrowing nothing: it owns its bytes.
pub struct TtFace {
    data: Vec<u8>,
    upm: f64,
    asc_em: f64,
    desc_em: f64,
    gap_em: f64,
    /// Sorted `(char, gid)` pairs from cmap format 4 or 12.
    cmap: Vec<(u32, u16)>,
    /// Advance width per glyph id, in EM units.
    advances: Vec<f64>,
    loca_off: usize,
    loca_long: bool,
    glyf_off: usize,
    glyf_len: usize,
    num_glyphs: u16,
    cache: RefCell<HashMap<(u32, u32), CachedGlyph>>,
}

/// One rasterized glyph: coverage mask plus pen-space metrics.
#[derive(Debug, Clone)]
pub(crate) struct CachedGlyph {
    w: u32,
    h: u32,
    /// Pen x to bitmap-left-edge offset, in px.
    dx: f64,
    /// Baseline to bitmap-top offset, in px (positive = above baseline).
    up: f64,
    /// Advance width in px, decoded for future pen stepping.
    #[allow(dead_code)]
    adv_px: f64,
    cov: Vec<u8>,
}

impl TtFace {
    /// Parse the first `glyf`-outline face in `bytes` (sfnt or `ttcf`).
    pub fn parse(src: &[u8]) -> Result<Self, String> {
        let data = src.to_vec();
        // `ttcf`: pick the first face that carries a `glyf` table.
        if sniff_kind(src) == FontKind::Collection {
            let n = be32(src, 8).ok_or("truncated `ttcf` header")? as usize;
            if n == 0 {
                return Err("empty `ttcf` collection".to_string());
            }
            let mut last_err = "no `glyf` face in the collection".to_string();
            for i in 0..n {
                let fo = be32(src, 12 + i * 4).ok_or("truncated `ttcf` offsets")? as usize;
                match Self::parse_sfnt(&data, fo) {
                    Ok(f) => return Ok(f),
                    Err(e) => last_err = e,
                }
            }
            return Err(last_err);
        }
        Self::parse_sfnt(&data, 0)
    }

    fn parse_sfnt(data: &[u8], base: usize) -> Result<Self, String> {
        let dir = table_dir(data, base).ok_or("truncated sfnt header")?;
        // CFF outlines are out of scope: refuse instead of misdrawing.
        if find_table(&dir, &tag(b'C', b'F', b'F', b' ')).is_some() {
            return Err(
                "CFF/PostScript outlines are not supported; use a TrueType (`glyf`) font"
                    .to_string(),
            );
        }
        let req = |t: [u8; 4], name: &str| {
            find_table(&dir, &t)
                .ok_or_else(|| format!("font is missing the required `{name}` table"))
        };
        let (head_off, _) = req(tag(b'h', b'e', b'a', b'd'), "head")?;
        let (hhea_off, _) = req(tag(b'h', b'h', b'e', b'a'), "hhea")?;
        let (maxp_off, _) = req(tag(b'm', b'a', b'x', b'p'), "maxp")?;
        let (hmtx_off, _) = req(tag(b'h', b'm', b't', b'x'), "hmtx")?;
        let (cmap_off, cmap_len) = req(tag(b'c', b'm', b'a', b'p'), "cmap")?;
        let (loca_off, _) = req(tag(b'l', b'o', b'c', b'a'), "loca")?;
        let (glyf_off, glyf_len) = req(tag(b'g', b'l', b'y', b'f'), "glyf")?;

        let upm = be16(data, head_off + 18).ok_or("truncated `head`")? as f64;
        if upm <= 0.0 {
            return Err("font has a non-positive units-per-em".to_string());
        }
        let loca_long = bei16(data, head_off + 50).ok_or("truncated `head`")? != 0;
        let num_glyphs = be16(data, maxp_off + 4).ok_or("truncated `maxp`")?;
        let asc_em = bei16(data, hhea_off + 4).ok_or("truncated `hhea`")? as f64;
        let desc_em = bei16(data, hhea_off + 6).ok_or("truncated `hhea`")? as f64;
        let gap_em = bei16(data, hhea_off + 8).ok_or("truncated `hhea`")? as f64;
        let num_hmetrics = be16(data, hhea_off + 34).ok_or("truncated `hhea`")? as usize;

        // Advances: metrics array plus trailing left-side bearings.
        let mut advances = vec![0.0f64; num_glyphs as usize];
        let metrics = num_hmetrics.min(num_glyphs as usize);
        for i in 0..metrics {
            let a = be16(data, hmtx_off + i * 4).ok_or("truncated `hmtx`")? as f64;
            advances[i] = a;
        }
        if metrics > 0 {
            let last = advances[metrics - 1];
            for a in advances.iter_mut().skip(metrics) {
                *a = last;
            }
        }

        let cmap = parse_cmap(data, cmap_off, cmap_len)?;
        if cmap.is_empty() {
            return Err("font has no usable `cmap` format 4 or 12 subtable".to_string());
        }

        Ok(TtFace {
            data: data.to_vec(),
            upm,
            asc_em,
            desc_em,
            gap_em,
            cmap,
            advances,
            loca_off,
            loca_long,
            glyf_off,
            glyf_len,
            num_glyphs,
            cache: RefCell::new(HashMap::new()),
        })
    }

    pub(crate) fn gid(&self, ch: u32) -> u16 {
        match self.cmap.binary_search_by_key(&ch, |(c, _)| *c) {
            Ok(i) => self.cmap[i].1,
            Err(_) => 0,
        }
    }

    /// Glyph outline as EM-space contours (y up), plus advance in EM.
    fn outline(&self, gid: u16) -> Option<(Vec<Vec<(f64, f64)>>, f64)> {
        self.glyph_contours(gid, 0)
    }

    pub(crate) fn raster_gid(&self, gid: u16, size_px: f64) -> CachedGlyph {
        let key = (gid as u32, (size_px * 64.0).round().max(1.0) as u32);
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let scale = size_px / self.upm;
        let (contours, adv_em) = self.outline(gid).unwrap_or((Vec::new(), 0.0));
        let built = if contours.is_empty() {
            CachedGlyph {
                w: 0,
                h: 0,
                dx: 0.0,
                up: 0.0,
                adv_px: adv_em * scale,
                cov: Vec::new(),
            }
        } else {
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for c in &contours {
                for &(x, y) in c {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
            let w = ((max_x - min_x) * scale).ceil().max(1.0) as u32;
            let h = ((max_y - min_y) * scale).ceil().max(1.0) as u32;
            let mut cov = vec![0u8; (w * h) as usize];
            // 4x4 supersampled even-odd fill; edges shared for all subpixels.
            let mut edges: Vec<(f64, f64, f64, f64)> = Vec::new();
            for c in &contours {
                for i in 0..c.len() {
                    let (x0, y0) = c[i];
                    let (x1, y1) = c[(i + 1) % c.len()];
                    if (y0 - y1).abs() > 1e-9 {
                        edges.push((x0, y0, x1, y1));
                    }
                }
            }
            for j in 0..h {
                for i in 0..w {
                    let mut inside = 0u32;
                    for a in 0..4 {
                        for b in 0..4 {
                            let px = min_x + (i as f64 + (a as f64 + 0.5) / 4.0) / scale;
                            let py = max_y - (j as f64 + (b as f64 + 0.5) / 4.0) / scale;
                            if even_odd(&edges, px, py) {
                                inside += 1;
                            }
                        }
                    }
                    cov[(j * w + i) as usize] = ((inside * 255 + 8) / 16) as u8;
                }
            }
            CachedGlyph {
                w,
                h,
                dx: min_x * scale,
                up: max_y * scale,
                adv_px: adv_em * scale,
                cov,
            }
        };
        self.cache.borrow_mut().insert(key, built.clone());
        built
    }

    fn advance_px(&self, ch: char, size_px: f64) -> f64 {
        let gid = self.gid(ch as u32);
        self.advances.get(gid as usize).copied().unwrap_or(0.0) * size_px / self.upm
    }

    fn line_metrics_px(&self, size_px: f64) -> (f64, f64) {
        let s = size_px / self.upm;
        (self.asc_em * s, (self.asc_em - self.desc_em + self.gap_em) * s)
    }
}

/// Parse the `cmap` table: prefer format 12, fall back to format 4.
/// Returns sorted `(char, gid)` pairs.
pub(crate) fn parse_cmap(data: &[u8], off: usize, len: usize) -> Result<Vec<(u32, u16)>, String> {
    let end = off.checked_add(len).ok_or("bad `cmap` length")?;
    if data.len() < end {
        return Err("truncated `cmap` table".to_string());
    }
    let n = be16(data, off + 2).ok_or("truncated `cmap` header")? as usize;
    let mut f4: Option<usize> = None;
    let mut f12: Option<usize> = None;
    for i in 0..n {
        let o = off + 4 + i * 8;
        let platform = be16(data, o).ok_or("truncated `cmap` record")?;
        let encoding = be16(data, o + 2).ok_or("truncated `cmap` record")?;
        let sub = be32(data, o + 4).ok_or("truncated `cmap` record")? as usize;
        let fmt = be16(data, off + sub).ok_or("truncated `cmap` subtable")?;
        // Prefer Unicode encodings; record positions of both formats.
        if (platform == 3 && encoding == 1) || (platform == 0) || (platform == 3 && encoding == 10) {
            match fmt {
                12 => f12 = Some(sub),
                4 => {
                    if f4.is_none() {
                        f4 = Some(sub);
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(sub) = f12 {
        return cmap12(data, off + sub, end);
    }
    if let Some(sub) = f4 {
        return cmap4(data, off + sub, end);
    }
    Err("font has no usable `cmap` format 4 or 12 subtable".to_string())
}

fn cmap12(data: &[u8], sub: usize, end: usize) -> Result<Vec<(u32, u16)>, String> {
    let n = be32(data, sub + 12).ok_or("truncated `cmap` format 12")? as usize;
    let mut out = Vec::new();
    for i in 0..n {
        let o = sub + 16 + i * 12;
        if o + 12 > end || o + 12 > data.len() {
            return Err("truncated `cmap` format 12 group".to_string());
        }
        let start = be32(data, o).ok_or("truncated group")?;
        let stop = be32(data, o + 4).ok_or("truncated group")?;
        let gid = be32(data, o + 8).ok_or("truncated group")?;
        for (k, c) in (start..=stop).enumerate() {
            let g = gid + k as u32;
            if g <= 0xffff {
                out.push((c, g as u16));
            }
        }
        if out.len() > 200_000 {
            return Err("unreasonable `cmap` format 12 size".to_string());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn cmap4(data: &[u8], sub: usize, end: usize) -> Result<Vec<(u32, u16)>, String> {
    let seg_x2 = be16(data, sub + 6).ok_or("truncated `cmap` format 4")? as usize;
    if seg_x2 % 2 != 0 {
        return Err("bad `cmap` format 4 segCount".to_string());
    }
    let segs = seg_x2 / 2;
    let ends = sub + 14;
    let starts = ends + 2 + seg_x2;
    let deltas = starts + seg_x2;
    let offsets = deltas + seg_x2;
    let mut out = Vec::new();
    for i in 0..segs {
        let end_c = be16(data, ends + i * 2).ok_or("truncated seg")? as u32;
        let start_c = be16(data, starts + i * 2).ok_or("truncated seg")? as u32;
        let delta = be16(data, deltas + i * 2).ok_or("truncated seg")? as i32;
        let range_off = be16(data, offsets + i * 2).ok_or("truncated seg")? as usize;
        if start_c == 0xffff && end_c == 0xffff {
            break;
        }
        for c in start_c..=end_c {
            let gid = if range_off == 0 {
                ((c as i32 + delta) & 0xffff) as u16
            } else {
                let addr = offsets + i * 2 + range_off + ((c - start_c) as usize) * 2;
                if addr + 2 > end || addr + 2 > data.len() {
                    return Err("truncated `cmap` format 4 glyph array".to_string());
                }
                let g = be16(data, addr).ok_or("truncated glyph")?;
                if g == 0 {
                    0
                } else {
                    ((g as i32 + delta) & 0xffff) as u16
                }
            };
            if gid != 0 {
                out.push((c, gid));
            }
            if out.len() > 200_000 {
                return Err("unreasonable `cmap` format 4 size".to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Byte range of glyph `gid` inside `glyf`.
fn glyph_byte_range(
    data: &[u8],
    loca_off: usize,
    long: bool,
    glyf_off: usize,
    glyf_len: usize,
    gid: u16,
    num_glyphs: u16,
) -> Option<(usize, usize)> {
    if gid >= num_glyphs {
        return None;
    }
    let at = |i: u16| -> Option<usize> {
        if long {
            be32(data, loca_off + i as usize * 4).map(|v| v as usize)
        } else {
            be16(data, loca_off + i as usize * 2).map(|v| v as usize * 2)
        }
    };
    let s = at(gid)?;
    let e = at(gid + 1)?;
    if s > e || glyf_off + e > data.len() || glyf_off + e > glyf_off + glyf_len {
        return None;
    }
    Some((glyf_off + s, glyf_off + e))
}

impl TtFace {
    /// Glyph outline as EM-space contours (y up), plus advance in EM.
    /// Composite glyphs are flattened recursively (depth-capped).
    fn glyph_contours(&self, gid: u16, depth: u8) -> Option<(Vec<Vec<(f64, f64)>>, f64)> {
        let adv = *self.advances.get(gid as usize).unwrap_or(&0.0);
        let (start, end) = glyph_byte_range(
            &self.data,
            self.loca_off,
            self.loca_long,
            self.glyf_off,
            self.glyf_len,
            gid,
            self.num_glyphs,
        )?;
        if start == end {
            return Some((Vec::new(), adv));
        }
        if depth > 8 || end - start < 10 {
            return None;
        }
        let n_contours = bei16(&self.data, start)? as i32;
        if n_contours > 0 {
            Some((self.parse_simple(start, end, n_contours as usize)?, adv))
        } else if n_contours < 0 {
            Some((self.parse_composite(start, end, depth)?, adv))
        } else {
            Some((Vec::new(), adv))
        }
    }

    fn parse_simple(
        &self,
        start: usize,
        end: usize,
        n_contours: usize,
    ) -> Option<Vec<Vec<(f64, f64)>>> {
        let data = &self.data;
        let mut o = start + 10;
        if o + n_contours * 2 > end {
            return None;
        }
        let mut end_pts = Vec::with_capacity(n_contours);
        for i in 0..n_contours {
            end_pts.push(be16(data, o + i * 2)? as usize);
        }
        o += n_contours * 2;
        let instr_len = be16(data, o)? as usize;
        o += 2 + instr_len;
        // Flags (with repeats).
        let n_pts = end_pts.last().copied().unwrap_or(0) + 1;
        let mut flags: Vec<u8> = Vec::with_capacity(n_pts);
        while flags.len() < n_pts {
            if o >= end {
                return None;
            }
            let f = data[o];
            o += 1;
            flags.push(f);
            if f & 0x08 != 0 {
                if o >= end {
                    return None;
                }
                let rep = data[o] as usize;
                o += 1;
                for _ in 0..rep {
                    flags.push(f);
                }
            }
        }
        if flags.len() != n_pts {
            return None;
        }
        // Coordinates: x then y, short/long forms per flags.
        let mut xs = vec![0i32; n_pts];
        let mut x = 0i32;
        for (i, f) in flags.iter().enumerate() {
            if f & 0x02 != 0 {
                if o >= end {
                    return None;
                }
                let v = data[o] as i32;
                o += 1;
                x += if f & 0x10 != 0 { v } else { -v };
            } else if f & 0x10 == 0 {
                if o + 2 > end {
                    return None;
                }
                x += bei16(data, o)? as i32;
                o += 2;
            }
            xs[i] = x;
        }
        let mut ys = vec![0i32; n_pts];
        let mut y = 0i32;
        for (i, f) in flags.iter().enumerate() {
            if f & 0x04 != 0 {
                if o >= end {
                    return None;
                }
                let v = data[o] as i32;
                o += 1;
                y += if f & 0x20 != 0 { v } else { -v };
            } else if f & 0x20 == 0 {
                if o + 2 > end {
                    return None;
                }
                y += bei16(data, o)? as i32;
                o += 2;
            }
            ys[i] = y;
        }
        let mut contours = Vec::with_capacity(n_contours);
        let mut first = 0usize;
        for &last in &end_pts {
            if last >= n_pts || first > last {
                return None;
            }
            let mut pts: Vec<(f64, f64, bool)> = Vec::new();
            for i in first..=last {
                pts.push((xs[i] as f64, ys[i] as f64, flags[i] & 0x01 != 0));
            }
            contours.push(flatten_closed(&pts));
            first = last + 1;
        }
        Some(contours)
    }

    fn parse_composite(
        &self,
        start: usize,
        end: usize,
        depth: u8,
    ) -> Option<Vec<Vec<(f64, f64)>>> {
        let data = &self.data;
        let mut o = start + 10;
        let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
        loop {
            if o + 4 > end {
                return None;
            }
            let flags = be16(data, o)?;
            let gid = be16(data, o + 2)?;
            o += 4;
            let (dx, dy) = if flags & 0x0001 != 0 {
                // ARG_1_AND_2_WORDS
                if o + 4 > end {
                    return None;
                }
                let a = bei16(data, o)? as f64;
                let b = bei16(data, o + 2)? as f64;
                o += 4;
                if flags & 0x0002 != 0 {
                    (a, b) // ARGS_ARE_XY_VALUES
                } else {
                    (0.0, 0.0) // matched-point args: pairing ignored
                }
            } else {
                if o + 2 > end {
                    return None;
                }
                let a = data[o] as i8 as f64;
                let b = data[o + 1] as i8 as f64;
                o += 2;
                if flags & 0x0002 != 0 {
                    (a, b)
                } else {
                    (0.0, 0.0)
                }
            };
            let (sx, sy) = if flags & 0x0008 != 0 {
                // WE_HAVE_A_SCALE
                if o + 2 > end {
                    return None;
                }
                let s = bei16(data, o)? as f64 / 16384.0;
                o += 2;
                (s, s)
            } else if flags & 0x0040 != 0 {
                // WE_HAVE_AN_X_AND_Y_SCALE
                if o + 4 > end {
                    return None;
                }
                let sx = bei16(data, o)? as f64 / 16384.0;
                let sy = bei16(data, o + 2)? as f64 / 16384.0;
                o += 4;
                (sx, sy)
            } else if flags & 0x0080 != 0 {
                // WE_HAVE_A_TWO_BY_TWO: keep the diagonal, drop shear.
                if o + 8 > end {
                    return None;
                }
                let sx = bei16(data, o)? as f64 / 16384.0;
                let sy = bei16(data, o + 6)? as f64 / 16384.0;
                o += 8;
                (sx, sy)
            } else {
                (1.0, 1.0)
            };
            let (sub, _) = self.glyph_contours(gid, depth + 1)?;
            for contour in sub {
                out.push(
                    contour
                        .into_iter()
                        .map(|(x, y)| (x * sx + dx, y * sy + dy))
                        .collect(),
                );
            }
            if flags & 0x0020 == 0 {
                break;
            }
        }
        Some(out)
    }
}

/// Flatten a closed TrueType point loop into line segments.
/// `pts` carries the on/off-curve flag per point; off-curve points are
/// quadratic control points subdivided to a flatness of 0.5 EM.
fn flatten_closed(pts: &[(f64, f64, bool)]) -> Vec<(f64, f64)> {
    let n = pts.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(pts[0].0, pts[0].1)];
    }
    // Order the loop starting from an on-curve point. An all-off-curve
    // loop (rare) starts at the midpoint of last-to-first.
    let mut seq: Vec<(f64, f64, bool)> = Vec::with_capacity(n + 1);
    match pts.iter().position(|p| p.2) {
        Some(k) => {
            for i in 0..n {
                seq.push(pts[(k + i) % n]);
            }
        }
        None => {
            let (ax, ay, _) = pts[n - 1];
            let (bx, by, _) = pts[0];
            seq.push(((ax + bx) / 2.0, (ay + by) / 2.0, true));
            for p in pts {
                seq.push((p.0, p.1, false));
            }
        }
    }
    let mut out = Vec::new();
    out.push((seq[0].0, seq[0].1));
    let mut i = 1;
    while i < seq.len() {
        if seq[i].2 {
            out.push((seq[i].0, seq[i].1));
            i += 1;
        } else {
            let ctrl = (seq[i].0, seq[i].1);
            let (end, skip) = if i + 1 < seq.len() {
                let q = seq[i + 1];
                if q.2 {
                    ((q.0, q.1), 2usize)
                } else {
                    (((ctrl.0 + q.0) / 2.0, (ctrl.1 + q.1) / 2.0), 1usize)
                }
            } else {
                ((seq[0].0, seq[0].1), 1usize)
            };
            let start = *out.last().unwrap();
            let steps = quad_steps(start, ctrl, end);
            for s in 1..=steps {
                out.push(quad_point(start, ctrl, end, s as f64 / steps as f64));
            }
            i += skip;
        }
    }
    out
}

fn quad_point(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
    )
}

fn quad_steps(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64)) -> usize {
    // Flatness estimate: distance of control point from the chord.
    let dx = p2.0 - p0.0;
    let dy = p2.1 - p0.1;
    let len = (dx * dx + dy * dy).sqrt();
    let dist = if len < 1e-9 {
        ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt()
    } else {
        ((p1.0 - p0.0) * dy - (p1.1 - p0.1) * dx).abs() / len
    };
    let steps = (dist / 0.5).sqrt().ceil() as usize;
    steps.clamp(1, 32)
}

/// Even-odd point-in-polygon test against the flattened edge list.
fn even_odd(edges: &[(f64, f64, f64, f64)], px: f64, py: f64) -> bool {
    let mut inside = false;
    for &(x0, y0, x1, y1) in edges {
        if (y0 > py) != (y1 > py) {
            let x = x0 + (py - y0) * (x1 - x0) / (y1 - y0);
            if px < x {
                inside = !inside;
            }
        }
    }
    inside
}

// ---------------------------------------------------------------------------
// Built-in bitmap face: 5x7 glyphs for printable ASCII, used only when the
// composition declares no `font`. Every entry is 7 rows, low 5 bits set.
// ---------------------------------------------------------------------------

/// 95 glyphs for U+0020..U+007E in order, each 7 rows of 5 pixels.
const BITMAP: [[u8; 7]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // space
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04], // !
    [0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00], // "
    [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A], // #
    [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04], // $
    [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03], // %
    [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D], // &
    [0x04, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00], // '
    [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02], // (
    [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08], // )
    [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00], // *
    [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00], // +
    [0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08], // ,
    [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00], // -
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x04], // .
    [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10], // /
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E], // 0
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E], // 1
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F], // 2
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E], // 3
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02], // 4
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E], // 5
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E], // 6
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E], // 8
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C], // 9
    [0x00, 0x04, 0x04, 0x00, 0x00, 0x04, 0x04], // :
    [0x00, 0x04, 0x04, 0x00, 0x00, 0x04, 0x08], // ;
    [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02], // <
    [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00], // =
    [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08], // >
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04], // ?
    [0x0E, 0x11, 0x1D, 0x15, 0x15, 0x10, 0x0E], // @
    [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11], // A
    [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E], // B
    [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E], // C
    [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E], // D
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F], // E
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10], // F
    [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F], // G
    [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11], // H
    [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // I
    [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C], // J
    [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11], // K
    [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F], // L
    [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11], // M
    [0x11, 0x19, 0x19, 0x15, 0x13, 0x13, 0x11], // N
    [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E], // O
    [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10], // P
    [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D], // Q
    [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11], // R
    [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E], // S
    [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // T
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E], // U
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04], // V
    [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11], // W
    [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11], // X
    [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04], // Y
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F], // Z
    [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E], // [
    [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01], // backslash
    [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E], // ]
    [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00], // ^
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F], // _
    [0x08, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00], // `
    [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F], // a
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1E], // b
    [0x00, 0x00, 0x0E, 0x10, 0x10, 0x11, 0x0E], // c
    [0x01, 0x01, 0x0D, 0x13, 0x11, 0x11, 0x0F], // d
    [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E], // e
    [0x06, 0x09, 0x08, 0x1C, 0x08, 0x08, 0x08], // f
    [0x00, 0x0F, 0x11, 0x11, 0x0F, 0x01, 0x0E], // g
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11], // h
    [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E], // i
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C], // j
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12], // k
    [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // l
    [0x00, 0x00, 0x1A, 0x15, 0x15, 0x11, 0x11], // m
    [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11], // n
    [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E], // o
    [0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10], // p
    [0x00, 0x00, 0x0D, 0x13, 0x0F, 0x01, 0x01], // q
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10], // r
    [0x00, 0x00, 0x0F, 0x10, 0x0E, 0x01, 0x1E], // s
    [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06], // t
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D], // u
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04], // v
    [0x00, 0x00, 0x11, 0x15, 0x15, 0x1B, 0x11], // w
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11], // x
    [0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E], // y
    [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F], // z
    [0x02, 0x04, 0x04, 0x08, 0x04, 0x04, 0x02], // {
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // |
    [0x08, 0x04, 0x04, 0x02, 0x04, 0x04, 0x08], // }
    [0x00, 0x00, 0x00, 0x09, 0x16, 0x00, 0x00], // ~
];

/// Bitmap metrics: an 8-unit em (5px glyph + 1px tracking + line gap).
const BITMAP_UNITS: f64 = 8.0;

fn bitmap_glyph(ch: char) -> Option<&'static [u8; 7]> {
    let c = ch as u32;
    if (0x20..=0x7e).contains(&c) {
        Some(&BITMAP[(c - 0x20) as usize])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Public face: project TrueType font or the built-in bitmap fallback.
// ---------------------------------------------------------------------------

/// The resolved text face for a composition.
pub enum Face {
    /// First declared project `font`, parsed and ready.
    Tt(TtFace),
    /// No `font` declared: built-in 5x7 bitmap face (announced, not silent).
    Bitmap,
}

impl Face {
    /// Load the project font. `bytes` must already have passed
    /// [`check_supported`]; a parse failure here is a render-time refusal.
    pub fn load_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(Face::Tt(
            TtFace::parse(bytes).map_err(|e| format!("cannot use font: {e}"))?,
        ))
    }

    pub fn fallback() -> Self {
        Face::Bitmap
    }

    /// Host system face: first candidate that exists and parses.
    /// Returns the face plus the file it came from, or `None` when
    /// the machine carries no usable system font (the caller keeps
    /// the built-in bitmap fallback and says so).
    pub fn system() -> Option<(Self, std::path::PathBuf)> {
        Self::system_with_style(notation::model::FontStyle::Regular).map(|(f, p, _)| (f, p))
    }

    /// Host system face for a style: the first variant file that
    /// exists and parses, so `bold`/`italic` use real variant
    /// outlines instead of synthesized ones. Returns the face, the
    /// file it came from, and whether the file is a true variant
    /// (`false` means a regular face the caller must synthesize the
    /// style on, or the `FRAMESMITH_SYSTEM_FONT` override which is
    /// taken as-is). `None` when the machine carries no usable
    /// system font for the style.
    pub fn system_with_style(
        style: notation::model::FontStyle,
    ) -> Option<(Self, std::path::PathBuf, bool)> {
        use notation::model::FontStyle as S;
        let supplemental = "/System/Library/Fonts/Supplemental";
        let shared = "/System/Library/Fonts";
        let dejavu = "/usr/share/fonts/truetype/dejavu";
        let names: &[&str] = match style {
            S::Regular => &["Arial.ttf", "Verdana.ttf", "Helvetica.ttc", "DejaVuSans.ttf"],
            S::Bold => &["Arial Bold.ttf", "Verdana Bold.ttf", "DejaVuSans-Bold.ttf"],
            S::Italic => &["Arial Italic.ttf", "Verdana Italic.ttf", "DejaVuSans-Oblique.ttf"],
            S::BoldItalic => &[
                "Arial Bold Italic.ttf",
                "Verdana Bold Italic.ttf",
                "DejaVuSans-BoldOblique.ttf",
            ],
        };
        // Style variants first; the regular faces are only a last
        // resort for style synthesis, never a silent substitution.
        let mut cands: Vec<(std::path::PathBuf, bool)> = Vec::new();
        if let Some(o) = std::env::var_os("FRAMESMITH_SYSTEM_FONT") {
            let p = std::path::PathBuf::from(o);
            if !p.as_os_str().is_empty() {
                cands.push((p, true));
            }
        }
        let dir_of = |n: &str| {
            if n.ends_with(".ttc") {
                shared
            } else if n.starts_with("DejaVu") {
                dejavu
            } else {
                supplemental
            }
        };
        for n in names {
            cands.push((
                std::path::PathBuf::from(format!("{}/{n}", dir_of(n))),
                true,
            ));
        }
        if style != S::Regular {
            for n in ["Arial.ttf", "Verdana.ttf", "Helvetica.ttc", "DejaVuSans.ttf"] {
                cands.push((
                    std::path::PathBuf::from(format!("{}/{n}", dir_of(n))),
                    false,
                ));
            }
        }
        for (p, exact) in cands {
            let bytes = match std::fs::read(&p) {
                Ok(b) => b,
                Err(_) => continue,
            };
            match TtFace::parse(&bytes) {
                Ok(t) => return Some((Face::Tt(t), p, exact)),
                Err(_) => continue,
            }
        }
        None
    }

    pub fn is_fallback(&self) -> bool {
        matches!(self, Face::Bitmap)
    }

    /// Advance width of one char in px at `size_px`.
    pub fn advance_px(&self, ch: char, size_px: f64) -> f64 {
        match self {
            Face::Tt(t) => t.advance_px(ch, size_px),
            Face::Bitmap => {
                if ch == ' ' {
                    6.0 * size_px / BITMAP_UNITS
                } else {
                    6.0 * size_px / BITMAP_UNITS
                }
            }
        }
    }

    /// `(ascent_px, line_height_px)` for `size_px`.
    pub fn line_metrics_px(&self, size_px: f64) -> (f64, f64) {
        match self {
            Face::Tt(t) => t.line_metrics_px(size_px),
            Face::Bitmap => (7.0 * size_px / BITMAP_UNITS, size_px),
        }
    }
}

/// One laid-out glyph: char plus pen position relative to line start.
#[derive(Debug, Clone)]
pub struct Placed {
    pub ch: char,
    pub x: f64,
}

/// One laid-out line with its own width.
#[derive(Debug, Clone)]
pub struct LaidLine {
    pub glyphs: Vec<Placed>,
    pub width: f64,
}

/// Laid-out paragraph: lines plus block metrics.
#[derive(Debug, Clone)]
pub struct LaidText {
    pub lines: Vec<LaidLine>,
    pub width: f64,
    pub height: f64,
    pub ascent: f64,
    pub line_height: f64,
}

impl Face {
    /// Lay out `text` at `size_px`, word-wrapping at `wrap_px` when set.
    /// Lines break at spaces; an overlong word overflows its line.
    pub fn layout(&self, text: &str, size_px: f64, wrap_px: Option<f64>) -> LaidText {
        let (ascent, line_height) = self.line_metrics_px(size_px);
        let space_w = self.advance_px(' ', size_px);
        let mut lines: Vec<LaidLine> = Vec::new();
        let mut cur: Vec<Placed> = Vec::new();
        let mut cur_w = 0.0f64;
        // Split on newlines defensively (the notation cannot carry them
        // today, but a future escape must not produce one long line).
        for (pi, para) in text.split('\n').enumerate() {
            if pi > 0 {
                let width = cur_w;
                lines.push(LaidLine { glyphs: cur, width });
                cur = Vec::new();
                cur_w = 0.0;
            }
            for word in para.split(' ') {
                if word.is_empty() {
                    // Consecutive spaces: one extra space advance.
                    if !cur.is_empty() {
                        cur_w += space_w;
                    }
                    continue;
                }
                let mut ww: f64 = 0.0;
                for ch in word.chars() {
                    ww += self.advance_px(ch, size_px);
                }
                let need = if cur.is_empty() { ww } else { space_w + ww };
                if let Some(limit) = wrap_px {
                    if !cur.is_empty() && cur_w + need > limit {
                        lines.push(LaidLine { glyphs: cur, width: cur_w });
                        cur = Vec::new();
                        cur_w = 0.0;
                    }
                }
                let mut x = if cur.is_empty() { 0.0 } else { cur_w + space_w };
                if cur.is_empty() {
                    cur_w = 0.0;
                } else {
                    cur_w += space_w;
                }
                for ch in word.chars() {
                    cur.push(Placed { ch, x });
                    x += self.advance_px(ch, size_px);
                    cur_w += self.advance_px(ch, size_px);
                }
            }
        }
        lines.push(LaidLine { glyphs: cur, width: cur_w });
        let width = lines.iter().map(|l| l.width).fold(0.0f64, f64::max);
        let height = ascent + line_height * (lines.len().saturating_sub(1) as f64)
            + (line_height - ascent).max(0.0);
        LaidText { lines, width, height, ascent, line_height }
    }

    /// Design units per em of a TrueType face; `None` for the bitmap face.
    pub fn units_per_em(&self) -> Option<f64> {
        match self {
            Face::Tt(t) => Some(t.upm),
            Face::Bitmap => None,
        }
    }

    /// Blend one glyph over `canvas` (RGB24, row-major) at integer
    /// top-left `(dst_x, dst_top)` with the glyph's pen at `baseline_y`.
    /// Pixels outside the canvas are clipped, never wrapped.
    #[allow(clippy::too_many_arguments)]
    pub fn blit(
        &self,
        canvas: &mut [u8],
        cw: u32,
        chh: u32,
        ch: char,
        size_px: f64,
        pen_x: f64,
        baseline_y: f64,
        color: (u8, u8, u8),
        style: notation::model::FontStyle,
    ) {
        self.blit_gid(canvas, cw, chh, self.gid_of(ch), ch, size_px, pen_x, baseline_y, color, style)
    }

    /// Blend glyph `gid` (from this face's `cmap`) over a straight-RGBA
    /// layer (row-major RGBA, source-over): the glyph color lands in RGB
    /// and `coverage * global_alpha` in A. Rotation and fade composite the
    /// layer afterward, so text turns and fades exactly like any other
    /// element. `ch` selects the bitmap cell for the fallback face and is
    /// ignored for TrueType faces.
    #[allow(clippy::too_many_arguments)]
    pub fn blit_gid_layer(
        &self,
        layer: &mut [u8],
        lw: u32,
        lh: u32,
        gid: u16,
        ch: char,
        size_px: f64,
        pen_x: f64,
        baseline_y: f64,
        color: (u8, u8, u8),
        style: notation::model::FontStyle,
        global_alpha: f64,
    ) {
        use notation::model::FontStyle as S;
        let bold = matches!(style, S::Bold | S::BoldItalic);
        let italic = matches!(style, S::Italic | S::BoldItalic);
        let shear = if italic { 0.212 } else { 0.0 };
        let alpha = global_alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        match self {
            Face::Tt(t) => {
                let g = t.raster_gid(gid, size_px);
                if g.w == 0 || g.h == 0 {
                    return;
                }
                let x0 = (pen_x + g.dx).round() as i64;
                let y0 = (baseline_y - g.up).round() as i64;
                let emb = if bold {
                    ((size_px / 32.0).round() as i64).max(1)
                } else {
                    0
                };
                blend_mask_layer(layer, lw, lh, &g.cov, g.w, g.h, x0, y0, color, shear, alpha);
                if emb > 0 {
                    blend_mask_layer(layer, lw, lh, &g.cov, g.w, g.h, x0 + emb, y0, color, shear, alpha);
                }
            }
            Face::Bitmap => {
                let unit = size_px / BITMAP_UNITS;
                if unit <= 0.0 {
                    return;
                }
                let rows: [u8; 7] = match bitmap_glyph(ch) {
                    Some(r) => *r,
                    _ => [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F],
                };
                let x0 = pen_x.round() as i64;
                let y0 = (baseline_y - 7.0 * unit).round() as i64;
                let h = (7.0 * unit).ceil() as u32;
                let w = (5.0 * unit).ceil() as u32;
                let emb = if bold { unit.max(1.0).round() as i64 } else { 0 };
                for j in 0..h {
                    let src_row = ((j as f64) / unit).floor() as usize;
                    if src_row >= 7 {
                        continue;
                    }
                    let slant =
                        ((6usize.saturating_sub(src_row) as f64) * shear * unit).round() as i64;
                    for i in 0..w {
                        let src_col = ((i as f64) / unit).floor() as usize;
                        if src_col >= 5 {
                            continue;
                        }
                        if rows[src_row] & (0x10 >> src_col) != 0 {
                            let x = x0 + i as i64 + slant;
                            blend_pixel_layer(layer, lw, lh, x, y0 + j as i64, color, alpha);
                            if emb > 0 {
                                blend_pixel_layer(layer, lw, lh, x + emb, y0 + j as i64, color, alpha);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Blend glyph `gid` (from this face's `cmap`) over the canvas.
    /// `ch` selects the bitmap cell for the fallback face and is ignored
    /// for TrueType faces. Lets parley-shaped runs draw without a second
    /// `cmap` lookup: the shaper already resolved the glyph.
    #[allow(clippy::too_many_arguments)]
    pub fn blit_gid(
        &self,
        canvas: &mut [u8],
        cw: u32,
        chh: u32,
        gid: u16,
        ch: char,
        size_px: f64,
        pen_x: f64,
        baseline_y: f64,
        color: (u8, u8, u8),
        style: notation::model::FontStyle,
    ) {
        use notation::model::FontStyle as S;
        let bold = matches!(style, S::Bold | S::BoldItalic);
        let italic = matches!(style, S::Italic | S::BoldItalic);
        // Faux-oblique slant: rows above the baseline shift right.
        let shear = if italic { 0.212 } else { 0.0 };
        match self {
            Face::Tt(t) => {
                let g = t.raster_gid(gid, size_px);
                if g.w == 0 || g.h == 0 {
                    return;
                }
                let x0 = (pen_x + g.dx).round() as i64;
                let y0 = (baseline_y - g.up).round() as i64;
                // Faux bold: a second strike one device pixel over.
                let emb = if bold {
                    ((size_px / 32.0).round() as i64).max(1)
                } else {
                    0
                };
                blend_mask(canvas, cw, chh, &g.cov, g.w, g.h, x0, y0, color, shear);
                if emb > 0 {
                    blend_mask(canvas, cw, chh, &g.cov, g.w, g.h, x0 + emb, y0, color, shear);
                }
            }
            Face::Bitmap => {
                let unit = size_px / BITMAP_UNITS;
                if unit <= 0.0 {
                    return;
                }
                // Unknown chars draw as a hollow box, never silently blank.
                let rows: [u8; 7] = match bitmap_glyph(ch) {
                    Some(r) => *r,
                    None => [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F],
                };
                let x0 = pen_x.round() as i64;
                let y0 = (baseline_y - 7.0 * unit).round() as i64;
                let h = (7.0 * unit).ceil() as u32;
                let w = (5.0 * unit).ceil() as u32;
                let emb = if bold { unit.max(1.0).round() as i64 } else { 0 };
                for j in 0..h {
                    let src_row = ((j as f64) / unit).floor() as usize;
                    if src_row >= 7 {
                        continue;
                    }
                    // Rows above the bitmap baseline shift right.
                    let slant =
                        ((6usize.saturating_sub(src_row) as f64) * shear * unit).round() as i64;
                    for i in 0..w {
                        let src_col = ((i as f64) / unit).floor() as usize;
                        if src_col >= 5 {
                            continue;
                        }
                        if rows[src_row] & (0x10 >> src_col) != 0 {
                            let x = x0 + i as i64 + slant;
                            blend_pixel(canvas, cw, chh, x, y0 + j as i64, color, 255);
                            if emb > 0 {
                                blend_pixel(canvas, cw, chh, x + emb, y0 + j as i64, color, 255);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Blend glyph `gid` over the canvas carrying `global_alpha` (T004):
    /// like [`blit_gid`](Self::blit_gid) but every texel fades, so an
    /// unrotated faded run draws straight onto the canvas with no scratch
    /// layer. Fully covered texels land exactly as the opaque path; edge
    /// texels may differ from the layer route by a rounding step, never
    /// more. `ch` selects the bitmap cell for the fallback face and is
    /// ignored for TrueType faces.
    #[allow(clippy::too_many_arguments)]
    pub fn blit_gid_faded(
        &self,
        canvas: &mut [u8],
        cw: u32,
        chh: u32,
        gid: u16,
        ch: char,
        size_px: f64,
        pen_x: f64,
        baseline_y: f64,
        color: (u8, u8, u8),
        style: notation::model::FontStyle,
        global_alpha: f64,
    ) {
        use notation::model::FontStyle as S;
        let bold = matches!(style, S::Bold | S::BoldItalic);
        let italic = matches!(style, S::Italic | S::BoldItalic);
        let shear = if italic { 0.212 } else { 0.0 };
        let g = global_alpha.clamp(0.0, 1.0);
        if g <= 0.0 {
            return;
        }
        match self {
            Face::Tt(t) => {
                let gface = t.raster_gid(gid, size_px);
                if gface.w == 0 || gface.h == 0 {
                    return;
                }
                let x0 = (pen_x + gface.dx).round() as i64;
                let y0 = (baseline_y - gface.up).round() as i64;
                let emb = if bold {
                    ((size_px / 32.0).round() as i64).max(1)
                } else {
                    0
                };
                blend_mask_faded(canvas, cw, chh, &gface.cov, gface.w, gface.h, x0, y0, color, shear, g);
                if emb > 0 {
                    blend_mask_faded(canvas, cw, chh, &gface.cov, gface.w, gface.h, x0 + emb, y0, color, shear, g);
                }
            }
            Face::Bitmap => {
                let unit = size_px / BITMAP_UNITS;
                if unit <= 0.0 {
                    return;
                }
                let rows: [u8; 7] = match bitmap_glyph(ch) {
                    Some(r) => *r,
                    _ => [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F],
                };
                let a = (255.0 * g).round() as u8;
                if a == 0 {
                    return;
                }
                let x0 = pen_x.round() as i64;
                let y0 = (baseline_y - 7.0 * unit).round() as i64;
                let h = (7.0 * unit).ceil() as u32;
                let w = (5.0 * unit).ceil() as u32;
                let emb = if bold { unit.max(1.0).round() as i64 } else { 0 };
                for j in 0..h {
                    let src_row = ((j as f64) / unit).floor() as usize;
                    if src_row >= 7 {
                        continue;
                    }
                    let slant =
                        ((6usize.saturating_sub(src_row) as f64) * shear * unit).round() as i64;
                    for i in 0..w {
                        let src_col = ((i as f64) / unit).floor() as usize;
                        if src_col >= 5 {
                            continue;
                        }
                        if rows[src_row] & (0x10 >> src_col) != 0 {
                            let x = x0 + i as i64 + slant;
                            blend_pixel(canvas, cw, chh, x, y0 + j as i64, color, a);
                            if emb > 0 {
                                blend_pixel(canvas, cw, chh, x + emb, y0 + j as i64, color, a);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Blend a coverage mask over the canvas with `color`, scaling every
/// texel by `global_alpha` first (T004: unrotated faded text draws
/// straight onto the canvas, skipping the scratch layer entirely).
/// `shear` shifts rows above the mask bottom right by
/// `(rows above bottom) * shear` device pixels (faux oblique);
/// `0.0` draws upright.
fn blend_mask_faded(
    canvas: &mut [u8],
    cw: u32,
    chh: u32,
    mask: &[u8],
    mw: u32,
    mh: u32,
    x0: i64,
    y0: i64,
    color: (u8, u8, u8),
    shear: f64,
    global_alpha: f64,
) {
    let g = global_alpha.clamp(0.0, 1.0);
    if g <= 0.0 {
        return;
    }
    for j in 0..mh {
        let y = y0 + j as i64;
        if y < 0 || y >= chh as i64 {
            continue;
        }
        let slant = (((mh - 1 - j) as f64) * shear).round() as i64;
        for i in 0..mw {
            let a = (mask[(j * mw + i) as usize] as f64 * g).round() as u8;
            if a == 0 {
                continue;
            }
            blend_pixel(canvas, cw, chh, x0 + i as i64 + slant, y, color, a);
        }
    }
}

/// Blend a coverage mask over the canvas with `color`.
/// `shear` shifts rows above the mask bottom right by
/// `(rows above bottom) * shear` device pixels (faux oblique);
/// `0.0` draws upright.
fn blend_mask(
    canvas: &mut [u8],
    cw: u32,
    chh: u32,
    mask: &[u8],
    mw: u32,
    mh: u32,
    x0: i64,
    y0: i64,
    color: (u8, u8, u8),
    shear: f64,
) {
    for j in 0..mh {
        let y = y0 + j as i64;
        if y < 0 || y >= chh as i64 {
            continue;
        }
        let slant = (((mh - 1 - j) as f64) * shear).round() as i64;
        for i in 0..mw {
            let a = mask[(j * mw + i) as usize];
            if a == 0 {
                continue;
            }
            blend_pixel(canvas, cw, chh, x0 + i as i64 + slant, y, color, a);
        }
    }
}

/// Blend a coverage mask over a straight-RGBA layer with `color`,
/// carrying `coverage/255 * global_alpha` in A (source-over, straight
/// alpha). `shear` is the faux-oblique slant, as in [`blend_mask`].
#[allow(clippy::too_many_arguments)]
fn blend_mask_layer(
    layer: &mut [u8],
    lw: u32,
    lh: u32,
    mask: &[u8],
    mw: u32,
    mh: u32,
    x0: i64,
    y0: i64,
    color: (u8, u8, u8),
    shear: f64,
    global_alpha: f64,
) {
    for j in 0..mh {
        let y = y0 + j as i64;
        if y < 0 || y >= lh as i64 {
            continue;
        }
        let slant = (((mh - 1 - j) as f64) * shear).round() as i64;
        for i in 0..mw {
            let a = mask[(j * mw + i) as usize];
            if a == 0 {
                continue;
            }
            blend_pixel_layer(
                layer,
                lw,
                lh,
                x0 + i as i64 + slant,
                y,
                color,
                a as f64 / 255.0 * global_alpha,
            );
        }
    }
}

/// Blend one layer pixel toward `color` with straight-alpha `a`
/// (0 transparent to 1 opaque), source-over.
fn blend_pixel_layer(
    layer: &mut [u8],
    lw: u32,
    lh: u32,
    x: i64,
    y: i64,
    color: (u8, u8, u8),
    a: f64,
) {
    if a <= 0.0 || x < 0 || y < 0 || x >= lw as i64 || y >= lh as i64 {
        return;
    }
    let o = (y as u32 * lw + x as u32) as usize * 4;
    if o + 3 >= layer.len() {
        return;
    }
    let a = a.clamp(0.0, 1.0);
    let dst_a = layer[o + 3] as f64 / 255.0;
    let out_a = a + dst_a * (1.0 - a);
    if out_a <= 0.0 {
        layer[o] = 0;
        layer[o + 1] = 0;
        layer[o + 2] = 0;
        layer[o + 3] = 0;
        return;
    }
    // Straight-alpha source-over; un-premultiply back into the layer.
    let mix = |src: u8, dst: u8| ((src as f64 * a + dst as f64 * dst_a * (1.0 - a)) / out_a + 0.5) as u8;
    layer[o] = mix(color.0, layer[o]);
    layer[o + 1] = mix(color.1, layer[o + 1]);
    layer[o + 2] = mix(color.2, layer[o + 2]);
    layer[o + 3] = (out_a * 255.0 + 0.5) as u8;
}

/// Blend one canvas pixel toward `color` with alpha `a` (0-255).
fn blend_pixel(
    canvas: &mut [u8],
    cw: u32,
    chh: u32,
    x: i64,
    y: i64,
    color: (u8, u8, u8),
    a: u8,
) {
    if x < 0 || y < 0 || x >= cw as i64 || y >= chh as i64 {
        return;
    }
    let o = (y as u32 * cw + x as u32) as usize * 3;
    if o + 2 >= canvas.len() {
        return;
    }
    let a = a as u32;
    let inv = 255 - a;
    canvas[o] = ((color.0 as u32 * a + canvas[o] as u32 * inv + 127) / 255) as u8;
    canvas[o + 1] = ((color.1 as u32 * a + canvas[o + 1] as u32 * inv + 127) / 255) as u8;
    canvas[o + 2] = ((color.2 as u32 * a + canvas[o + 2] as u32 * inv + 127) / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal sfnt builder: glyphs 0 (empty), 1 (`A` triangle), 2
    // (composite of 1 shifted +100x). cmap maps `A`->1, `B`->2.
    fn u16b(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    fn i16b(v: &mut Vec<u8>, x: i16) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    fn u32b(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_be_bytes());
    }

    fn simple_a() -> Vec<u8> {
        let mut g = Vec::new();
        i16b(&mut g, 1); // 1 contour
        i16b(&mut g, 0);
        i16b(&mut g, 0);
        i16b(&mut g, 500);
        i16b(&mut g, 700); // bbox
        u16b(&mut g, 2); // endPts[0]
        u16b(&mut g, 0); // no instructions
        g.extend_from_slice(&[0x01, 0x01, 0x01]); // all on-curve
        // x deltas (long): 0, +500, -250
        i16b(&mut g, 0);
        i16b(&mut g, 500);
        i16b(&mut g, -250);
        // y deltas (long): 0, 0, +700
        i16b(&mut g, 0);
        i16b(&mut g, 0);
        i16b(&mut g, 700);
        g
    }

    fn composite_of_a() -> Vec<u8> {
        let mut g = Vec::new();
        i16b(&mut g, -1); // composite
        i16b(&mut g, 0);
        i16b(&mut g, 0);
        i16b(&mut g, 600);
        i16b(&mut g, 700); // bbox
        u16b(&mut g, 0x0002); // ARGS_ARE_XY_VALUES, byte args
        u16b(&mut g, 1); // component gid 1
        g.push(100);
        g.push(0); // dx=100, dy=0
        g
    }

    fn table_cmap() -> Vec<u8> {
        // format 4: `A`->1, `B`->2, then the 0xFFFF terminator segment.
        let mut c = Vec::new();
        u16b(&mut c, 0); // version
        u16b(&mut c, 1); // 1 subtable
        u16b(&mut c, 3); // platform
        u16b(&mut c, 1); // encoding
        u32b(&mut c, 12); // offset
        // subtable at +12:
        u16b(&mut c, 4); // format
        u16b(&mut c, 32); // length
        u16b(&mut c, 0); // language
        u16b(&mut c, 4); // segCountX2
        u16b(&mut c, 4); // searchRange
        u16b(&mut c, 1); // entrySelector
        u16b(&mut c, 0); // rangeShift
        u16b(&mut c, 0x42); // endCode[0]
        u16b(&mut c, 0xffff); // endCode[1]
        u16b(&mut c, 0); // reservedPad
        u16b(&mut c, 0x41); // startCode[0]
        u16b(&mut c, 0xffff); // startCode[1]
        i16b(&mut c, (1i32 - 0x41i32) as i16); // idDelta[0]
        i16b(&mut c, 1); // idDelta[1]
        u16b(&mut c, 0); // idRangeOffset[0] (direct)
        u16b(&mut c, 0); // idRangeOffset[1]
        c
    }

    fn mini_ttf() -> Vec<u8> {
        let head = {
            let mut h = vec![0u8; 54];
            h[18..20].copy_from_slice(&1000u16.to_be_bytes()); // upm
            h[50..52].copy_from_slice(&0i16.to_be_bytes()); // short loca
            h
        };
        let hhea = {
            let mut h = vec![0u8; 36];
            h[4..6].copy_from_slice(&800i16.to_be_bytes());
            h[6..8].copy_from_slice(&(-200i16).to_be_bytes());
            h[34..36].copy_from_slice(&3u16.to_be_bytes()); // 3 metrics
            h
        };
        let maxp = {
            let mut m = Vec::new();
            u32b(&mut m, 0x00005000);
            u16b(&mut m, 3);
            m
        };
        let hmtx = {
            let mut h = Vec::new();
            for adv in [500u16, 600, 600] {
                u16b(&mut h, adv);
                i16b(&mut h, 0);
            }
            h
        };
        let cmap = table_cmap();
        let ga = simple_a();
        let gc = composite_of_a();
        // loca (short): g0 empty at 0, g1 at 0..len, g2 after, end.
        let off1 = 0usize;
        let pad = |n: usize| (4 - n % 4) % 4;
        let mut glyf = Vec::new();
        glyf.extend_from_slice(&ga);
        // Short `loca` entries are divided by two: every glyph starts even.
        if glyf.len() % 2 != 0 {
            glyf.push(0);
        }
        let off2 = glyf.len();
        glyf.extend_from_slice(&gc);
        let off3 = glyf.len();
        glyf.extend(std::iter::repeat(0).take(pad(off3)));
        let loca = {
            let mut l = Vec::new();
            for o in [off1, off1, off2, off3] {
                u16b(&mut l, (o / 2) as u16);
            }
            l
        };
        let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"cmap", cmap),
            (b"glyf", glyf),
            (b"head", head),
            (b"hhea", hhea),
            (b"hmtx", hmtx),
            (b"loca", loca),
            (b"maxp", maxp),
        ];
        let mut out = Vec::new();
        u32b(&mut out, 0x00010000);
        u16b(&mut out, tables.len() as u16);
        u16b(&mut out, 64);
        u16b(&mut out, 3);
        u16b(&mut out, 32);
        let mut base = 12 + 16 * tables.len();
        let mut blobs: Vec<(u32, usize, usize)> = Vec::new();
        for (tag, data) in &tables {
            let off = base;
            let len = data.len();
            blobs.push((u32::from_be_bytes(**tag), off, len));
            base += len + pad(len);
        }
        for (tag, off, len) in &blobs {
            u32b(&mut out, *tag);
            u32b(&mut out, 0);
            u32b(&mut out, *off as u32);
            u32b(&mut out, *len as u32);
        }
        for ((_, data), (_, off, _)) in tables.iter().zip(blobs.iter()) {
            while out.len() < *off {
                out.push(0);
            }
            out.extend_from_slice(data);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        }
        out
    }

    #[test]
    fn sniff_kinds() {
        assert_eq!(sniff_kind(&[0, 1, 0, 0, 0]), FontKind::TrueType);
        assert_eq!(sniff_kind(b"OTTOxxxx"), FontKind::OpenType);
        assert_eq!(sniff_kind(b"ttcfxxxx"), FontKind::Collection);
        assert_eq!(sniff_kind(b"wOFFxxxx"), FontKind::Woff);
        assert_eq!(sniff_kind(b"junk"), FontKind::Unknown);
        assert_eq!(sniff_kind(&[]), FontKind::Unknown);
    }

    #[test]
    fn mini_font_parses_and_draws() {
        let bytes = mini_ttf();
        assert!(check_supported(&bytes).is_ok());
        let face = Face::load_bytes(&bytes).unwrap();
        assert!(!face.is_fallback());
        // Advances scale with size: 600 EM at upm 1000, size 100 -> 60px.
        assert!((face.advance_px('A', 100.0) - 60.0).abs() < 1e-9);
        assert!((face.advance_px('B', 50.0) - 30.0).abs() < 1e-9);
        // Unknown char maps to gid 0 (.notdef): repeatable advance,
        // empty outline, no crash.
        assert_eq!(face.advance_px('Z', 100.0), 50.0);
        // Rasterized `A` covers pixels; composite `B` covers the same
        // area (same triangle shifted +100 EM, same size).
        if let Face::Tt(t) = &face {
            let a = t.raster_gid(t.gid('A' as u32), 100.0);
            let b = t.raster_gid(t.gid('B' as u32), 100.0);
            assert!(a.w > 0 && a.h > 0);
            let suma: u32 = a.cov.iter().map(|c| *c as u32).sum();
            let sumb: u32 = b.cov.iter().map(|c| *c as u32).sum();
            assert!(suma > 1000, "triangle A must cover pixels");
            assert_eq!(suma, sumb, "shifted composite must cover the same area");
            assert!((b.dx - a.dx - 10.0).abs() < 1.0, "dx shifts by 100EM@100px");
        } else {
            panic!("expected a TrueType face");
        }
    }

    #[test]
    fn unsupported_formats_refuse() {
        assert!(check_supported(b"wOFFjunkjunk").is_err());
        assert!(check_supported(b"junkjunkjunk").is_err());
        // CFF-based OpenType (no `glyf` table) refuses.
        let mut otf = vec![0u8; 12];
        otf[0..4].copy_from_slice(b"OTTO");
        assert!(check_supported(&otf).is_err());
        // Truncated sfnt refuses instead of panicking.
        assert!(check_supported(&[0, 1, 0, 0, 0, 6]).is_err());
    }

    #[test]
    fn fallback_bitmap_draws_and_wraps() {
        let face = Face::fallback();
        assert!(face.is_fallback());
        let one = face.layout("a b c", 16.0, None);
        assert_eq!(one.lines.len(), 1);
        let wrapped = face.layout("a b c", 16.0, Some(20.0));
        assert_eq!(wrapped.lines.len(), 3, "{wrapped:?}");
        // Blit leaves the mark on the canvas.
        let mut canvas = vec![0u8; 32 * 32 * 3];
        use notation::model::FontStyle as S;
        face.blit(&mut canvas, 32, 32, 'A', 16.0, 0.0, 14.0, (255, 255, 255), S::Regular);
        assert!(canvas.iter().any(|b| *b > 0));
        // Unknown chars draw the hollow box, never blank.
        let mut canvas2 = vec![0u8; 32 * 32 * 3];
        face.blit(&mut canvas2, 32, 32, 'é', 16.0, 0.0, 14.0, (255, 255, 255), S::Regular);
        assert!(canvas2.iter().any(|b| *b > 0));
        // Faux bold puts down at least as much ink; faux italic moves it.
        let mut canvas3 = vec![0u8; 32 * 32 * 3];
        face.blit(&mut canvas3, 32, 32, 'A', 16.0, 0.0, 14.0, (255, 255, 255), S::Bold);
        let ink = |c: &[u8]| c.iter().map(|b| *b as u32).sum::<u32>();
        assert!(ink(&canvas3) >= ink(&canvas));
        let mut canvas4 = vec![0u8; 32 * 32 * 3];
        face.blit(&mut canvas4, 32, 32, 'A', 16.0, 0.0, 14.0, (255, 255, 255), S::Italic);
        assert!(canvas4 != canvas);
    }

    #[test]
    fn faded_blit_matches_opaque_at_full_alpha_and_fades() {
        // T004: the direct faded blit is the opaque blit at alpha 1,
        // draws nothing at 0, and lands between canvas and opaque ink
        // at 0.5 — deterministically, on the built-in face.
        use notation::model::FontStyle as S;
        let face = Face::fallback();
        let gid = face.gid_of('A');
        let mut opaque = vec![9u8; 32 * 32 * 3];
        face.blit_gid(&mut opaque, 32, 32, gid, 'A', 16.0, 0.0, 14.0, (255, 255, 255), S::Regular);
        let mut full = vec![9u8; 32 * 32 * 3];
        face.blit_gid_faded(&mut full, 32, 32, gid, 'A', 16.0, 0.0, 14.0, (255, 255, 255), S::Regular, 1.0);
        assert_eq!(full, opaque, "alpha 1.0 is the opaque blit exactly");
        let mut gone = vec![9u8; 32 * 32 * 3];
        face.blit_gid_faded(&mut gone, 32, 32, gid, 'A', 16.0, 0.0, 14.0, (255, 255, 255), S::Regular, 0.0);
        assert!(gone.iter().all(|&v| v == 9), "alpha 0.0 draws nothing");
        let mut half = vec![9u8; 32 * 32 * 3];
        face.blit_gid_faded(&mut half, 32, 32, gid, 'A', 16.0, 0.0, 14.0, (255, 255, 255), S::Regular, 0.5);
        let peak = |c: &[u8]| c.chunks_exact(3).map(|p| p[0]).max().unwrap_or(0);
        assert!(peak(&half) > 9 && peak(&half) < peak(&opaque), "alpha 0.5 blends");
    }

    #[test]
    fn flatten_handles_curves_and_lines() {
        // Pure line loop stays a triangle.
        let tri = vec![(0.0, 0.0, true), (10.0, 0.0, true), (5.0, 8.0, true)];
        let flat = flatten_closed(&tri);
        assert!(flat.len() >= 3);
        // Curve loop gains subdivided points.
        let curve = vec![(0.0, 0.0, true), (10.0, 20.0, false), (20.0, 0.0, true)];
        assert!(flatten_closed(&curve).len() > 2);
        // Empty and single-point loops do not panic.
        assert!(flatten_closed(&[]).is_empty());
        assert_eq!(flatten_closed(&[(1.0, 2.0, true)]), vec![(1.0, 2.0)]);
    }
}

