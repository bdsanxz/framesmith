//! Offline map viewports (Sprint 21 adapters, T004 / CAP-004).
//!
//! A `map` clip composites raster tiles from a local tile directory
//! or offline pack (`{pack}/{z}/{x}/{y}.png`) at its `center` and
//! `zoom` in Web Mercator, then paints markers, labels, polylines,
//! and polygons over them, with a scale bar and attribution text on
//! every frame. Uncovered areas show the named
//! [`MAP_PLACEHOLDER_NAME`](notation::model::MAP_PLACEHOLDER_NAME)
//! placeholder, never fetched tiles.
//!
//! Pure standard library plus the `png` decoder and the built-in
//! bitmap text face: no browser, no JavaScript, no network at render
//! time. Every byte derives from the pack files and the clip fields,
//! so the same inputs render byte-identical frames on every run. The
//! pre-rendered viewport enters the shared image stack (order,
//! opacity, motion, grade, effects, VFX) exactly like a decoded
//! picture.

use std::path::Path;

use notation::model::{MapClip, Rgb, MAP_PLACEHOLDER_NAME, MAP_TILE_PX};

/// Fill of one uncovered tile cell (Sprint 21 adapters, T004).
pub const PLACEHOLDER_BG: (u8, u8, u8) = (232, 228, 220);
/// Ink of the placeholder name drawn on uncovered cells.
pub const PLACEHOLDER_INK: (u8, u8, u8) = (96, 94, 88);
/// Height of the scale bar, in canvas pixels.
pub const SCALE_BAR_HEIGHT: f64 = 6.0;
/// Bitmap text size of the scale label and attribution, in pixels.
pub const MAP_TEXT_SIZE: f64 = 10.0;

/// Pixel dimensions of one map viewport (Sprint 21 adapters, T004):
/// the authored `width`/`height`, else the composition canvas. At
/// least one pixel per side, so a degenerate clip still keys and
/// renders deterministically instead of dividing by zero.
pub fn viewport_px(m: &MapClip, canvas: (u32, u32)) -> (u32, u32) {
    let w = m.width.unwrap_or(f64::from(canvas.0));
    let h = m.height.unwrap_or(f64::from(canvas.1));
    (w.max(1.0).round() as u32, h.max(1.0).round() as u32)
}

/// Canonical key of one pre-rendered viewport (Sprint 21 adapters,
/// T004): a wrapping FNV-1a mix over the pack path, the exact center
/// and zoom, the pixel size, the attribution, and every overlay, so
/// the prepare stage and the frame stage address the same entry and
/// equal viewports share one decode.
pub fn viewport_key(m: &MapClip, w: u32, h: u32) -> String {
    let mut canon = String::new();
    canon.push_str(&m.pack);
    canon.push('|');
    canon.push_str(&format!("{:?},{:?}", m.center_lat, m.center_lon));
    canon.push_str(&format!("|{}|{w}x{h}|{}", m.zoom, m.attribution));
    for mk in &m.markers {
        canon.push_str(&format!(
            "|marker:{:?},{:?},{:?},{:02x}{:02x}{:02x}",
            mk.lat, mk.lon, mk.label, mk.color.r, mk.color.g, mk.color.b
        ));
    }
    for lb in &m.labels {
        canon.push_str(&format!(
            "|label:{:?},{:?},{:?},{:?},{:02x}{:02x}{:02x}",
            lb.lat, lb.lon, lb.value, lb.size, lb.color.r, lb.color.g, lb.color.b
        ));
    }
    for pl in &m.polylines {
        canon.push_str(&format!("|polyline:{:?}", pl.width));
        canon.push_str(&format!(
            "|{:02x}{:02x}{:02x}",
            pl.color.r, pl.color.g, pl.color.b
        ));
        for (lat, lon) in &pl.points {
            canon.push_str(&format!("|{lat:?},{lon:?}"));
        }
    }
    for pg in &m.polygons {
        canon.push_str(&format!(
            "|polygon:{:02x}{:02x}{:02x}:{:?}:{:02x}{:02x}{:02x}",
            pg.fill.r,
            pg.fill.g,
            pg.fill.b,
            pg.border_width,
            pg.border_color.r,
            pg.border_color.g,
            pg.border_color.b
        ));
        for (lat, lon) in &pg.points {
            canon.push_str(&format!("|{lat:?},{lon:?}"));
        }
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in canon.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // A synthetic `map-viewports/` path: it never exists on disk, so
    // no real image file can collide with a pre-rendered viewport in
    // the shared decoded-image list the frame stage looks up by path.
    format!("map-viewports/map-viewport-{hash:016x}")
}

/// Paint one opaque RGB texel into the RGB24 working buffer
/// (clipped, never wrapped): the map stack composites tiles opaquely
/// and draws overlays above, so one helper serves every paint step.
/// The finished buffer converts to RGBA once, at the end.
#[allow(clippy::too_many_arguments)]
fn put_px(buf: &mut [u8], w: u32, h: u32, x: i64, y: i64, r: u8, g: u8, b: u8) {
    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
        return;
    }
    let i = (y as u32 * w + x as u32) as usize * 3;
    buf[i] = r;
    buf[i + 1] = g;
    buf[i + 2] = b;
}

/// Fill one rectangle of the RGB24 buffer with an opaque color.
#[allow(clippy::too_many_arguments)]
fn fill_rect(buf: &mut [u8], w: u32, h: u32, x0: i64, y0: i64, x1: i64, y1: i64, c: (u8, u8, u8)) {
    for y in y0.max(0)..=y1.min(h as i64 - 1) {
        for x in x0.max(0)..=x1.min(w as i64 - 1) {
            let i = (y as u32 * w + x as u32) as usize * 3;
            buf[i] = c.0;
            buf[i + 1] = c.1;
            buf[i + 2] = c.2;
        }
    }
}

/// Fill one disc of radius `r` around `(cx, cy)` (pixel centers).
fn fill_disc(buf: &mut [u8], w: u32, h: u32, cx: f64, cy: f64, r: f64, c: (u8, u8, u8)) {
    if r <= 0.0 {
        return;
    }
    let x0 = (cx - r).floor() as i64;
    let x1 = (cx + r).ceil() as i64;
    let y0 = (cy - r).floor() as i64;
    let y1 = (cy + r).ceil() as i64;
    for y in y0.max(0)..=y1.min(h as i64 - 1) {
        for x in x0.max(0)..=x1.min(w as i64 - 1) {
            let dx = ((x as f64) + 0.5 - cx) / r;
            let dy = ((y as f64) + 0.5 - cy) / r;
            if dx * dx + dy * dy <= 1.0 {
                let i = (y as u32 * w + x as u32) as usize * 3;
                buf[i] = c.0;
                buf[i + 1] = c.1;
                buf[i + 2] = c.2;
            }
        }
    }
}

/// Stroke one segment with round caps by stamping discs every half
/// pixel: deterministic, width-exact, no anti-aliasing surprises.
#[allow(clippy::too_many_arguments)]
fn stroke_segment(
    buf: &mut [u8],
    w: u32,
    h: u32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    width: f64,
    c: (u8, u8, u8),
) {
    let len = ((x1 - x0).hypot(y1 - y0) * 2.0).ceil().max(1.0);
    let n = len as u32;
    for i in 0..=n {
        let t = f64::from(i) / f64::from(n);
        fill_disc(
            buf,
            w,
            h,
            x0 + (x1 - x0) * t,
            y0 + (y1 - y0) * t,
            width / 2.0,
            c,
        );
    }
}

/// Draw one bitmap string with its top at `y_top` (Sprint 21
/// adapters, T004): the built-in 5x7 face, so attribution, marker
/// labels, and the scale text render with no font files and no host
/// faces — byte-identical on every machine.
#[allow(clippy::too_many_arguments)]
fn draw_text_rgb(
    buf: &mut [u8],
    w: u32,
    h: u32,
    text: &str,
    size_px: f64,
    x: f64,
    y_top: f64,
    color: (u8, u8, u8),
) {
    let face = glyph::font::Face::fallback();
    let laid = face.layout(text, size_px, None);
    let mut baseline = y_top + laid.ascent;
    for line in &laid.lines {
        for g in &line.glyphs {
            face.blit(
                buf,
                w,
                h,
                g.ch,
                size_px,
                x + g.x,
                baseline,
                color,
                notation::model::FontStyle::Regular,
            );
        }
        baseline += laid.line_height;
    }
}

/// Width of one bitmap string at `size_px`, for pill layout.
fn text_width_px(text: &str, size_px: f64) -> f64 {
    glyph::font::Face::fallback()
        .layout(text, size_px, None)
        .width
}

/// Decode one tile file to opaque RGBA (Sprint 21 adapters, T004):
/// still PNGs only, normalized from gray/RGB/RGBA to RGBA. Any
/// failure names the file and the cause; validation already
/// decode-checks every required tile, so this is the render-side
/// backstop (a vanished or swapped tile refuses before frame one,
/// never a partial MP4).
fn decode_tile_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let detail = |why: String| format!("map tile {:?} {}", path.display(), why);
    let file = std::fs::File::open(path).map_err(|e| detail(format!("cannot be read: {e}")))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| detail(format!("is not a readable PNG picture ({e})")))?;
    if reader.info().is_animated() {
        return Err(detail(
            "is an animated PNG; map tiles are still pictures".to_string(),
        ));
    }
    let (tw, th) = (reader.info().width, reader.info().height);
    if tw != MAP_TILE_PX || th != MAP_TILE_PX {
        return Err(detail(format!(
            "is {tw}x{th}, but map tiles are {}x{} pictures",
            MAP_TILE_PX, MAP_TILE_PX
        )));
    }
    let (color, _depth) = reader.output_color_type();
    let mut raw = vec![0u8; reader.output_buffer_size()];
    reader
        .next_frame(&mut raw)
        .map_err(|e| detail(format!("cannot be decoded ({e})")))?;
    let px = tw as usize * th as usize;
    let rgba = match color {
        png::ColorType::Rgba => {
            if raw.len() != px * 4 {
                return Err(detail(format!(
                    "decoded to a partial picture ({} bytes, want {})",
                    raw.len(),
                    px * 4
                )));
            }
            raw
        }
        png::ColorType::Rgb => {
            if raw.len() != px * 3 {
                return Err(detail(format!(
                    "decoded to a partial picture ({} bytes, want {})",
                    raw.len(),
                    px * 3
                )));
            }
            let mut out = Vec::with_capacity(px * 4);
            for triple in raw.as_chunks::<3>().0 {
                out.extend_from_slice(&[triple[0], triple[1], triple[2], 255]);
            }
            out
        }
        png::ColorType::Grayscale => {
            if raw.len() != px {
                return Err(detail(format!(
                    "decoded to a partial picture ({} bytes, want {px})",
                    raw.len()
                )));
            }
            let mut out = Vec::with_capacity(px * 4);
            for g in raw {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            if raw.len() != px * 2 {
                return Err(detail(format!(
                    "decoded to a partial picture ({} bytes, want {})",
                    raw.len(),
                    px * 2
                )));
            }
            let mut out = Vec::with_capacity(px * 4);
            for pair in raw.as_chunks::<2>().0 {
                out.extend_from_slice(&[pair[0], pair[0], pair[0], pair[1]]);
            }
            out
        }
        other => {
            return Err(detail(format!(
                "has an unsupported color type ({other:?}); use 8-bit gray/RGB/RGBA tiles"
            )));
        }
    };
    Ok((tw, th, rgba))
}

/// Project one longitude/latitude to viewport pixels (Sprint 21
/// adapters, T004): the shared Web Mercator projection minus the
/// viewport origin, so overlays land on the tiles they describe.
fn project(lat: f64, lon: f64, zoom: u32, left: f64, top: f64) -> (f64, f64) {
    let (wx, wy) = notation::model::map_world_px(lat, lon, zoom);
    (wx - left, wy - top)
}

/// Fill one polygon ring with the even-odd rule (Sprint 21 adapters,
/// T004): per-row intersection pairs over the ring bbox, pixel
/// centers, deterministic top-to-bottom.
fn fill_ring(buf: &mut [u8], w: u32, h: u32, pts: &[(f64, f64)], c: (u8, u8, u8)) {
    if pts.len() < 3 {
        return;
    }
    let min_y = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_y = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let y0 = (min_y.floor() as i64).max(0);
    let y1 = (max_y.ceil() as i64).min(h as i64 - 1);
    for y in y0..=y1 {
        let yc = (y as f64) + 0.5;
        let mut xs = Vec::new();
        for i in 0..pts.len() {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % pts.len()];
            if (y0 <= yc && yc < y1) || (y1 <= yc && yc < y0) {
                let t = (yc - y0) / (y1 - y0);
                xs.push(x0 + t * (x1 - x0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.as_chunks::<2>().0 {
            let xa = (pair[0].ceil() as i64).max(0);
            let xb = (pair[1].floor() as i64).min(w as i64 - 1);
            for x in xa..=xb {
                let i = (y as u32 * w + x as u32) as usize * 3;
                buf[i] = c.0;
                buf[i + 1] = c.1;
                buf[i + 2] = c.2;
            }
        }
    }
}

/// Stroke one overlay path through every point, closed for rings.
fn stroke_path(
    buf: &mut [u8],
    w: u32,
    h: u32,
    pts: &[(f64, f64)],
    close: bool,
    width: f64,
    c: (u8, u8, u8),
) {
    if pts.len() < 2 {
        return;
    }
    for i in 0..pts.len() - 1 {
        stroke_segment(
            buf,
            w,
            h,
            pts[i].0,
            pts[i].1,
            pts[i + 1].0,
            pts[i + 1].1,
            width,
            c,
        );
    }
    if close {
        let last = pts.len() - 1;
        stroke_segment(
            buf,
            w,
            h,
            pts[last].0,
            pts[last].1,
            pts[0].0,
            pts[0].1,
            width,
            c,
        );
    }
}

/// Meters per pixel at one latitude and zoom (Sprint 21 adapters,
/// T004): the Web Mercator ground resolution feeding the scale bar.
fn meters_per_pixel(lat: f64, zoom: u32) -> f64 {
    156543.03392 * lat.to_radians().cos() / 2u32.pow(zoom.min(32)) as f64
}

/// Round a raw bar length down to a 1/2/5 step (Sprint 21 adapters,
/// T004): scale bars read clean numbers, never 113 m.
fn nice_bar_length(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 0.0;
    }
    let exp = raw.log10().floor();
    let base = 10f64.powf(exp);
    for m in [5.0, 2.0, 1.0] {
        if m * base <= raw {
            return m * base;
        }
    }
    base / 10.0
}

/// Human scale label: meters below a kilometer, kilometers above.
fn scale_label(meters: f64) -> String {
    if meters >= 1000.0 {
        let km = meters / 1000.0;
        if (km - km.round()).abs() < 1e-9 {
            format!("{} km", km.round() as i64)
        } else {
            format!("{km} km")
        }
    } else if (meters - meters.round()).abs() < 1e-9 {
        format!("{} m", meters.round() as i64)
    } else {
        format!("{meters} m")
    }
}

fn as_rgb(c: &Rgb) -> (u8, u8, u8) {
    (c.r, c.g, c.b)
}

/// Paint the scale bar and attribution (Sprint 21 adapters, T004):
/// always on when a map is on screen (CAP-004). The bar sits
/// bottom-left with its clean-number label above; the attribution
/// sits bottom-right on an opaque pill so it stays legible over any
/// tile.
fn paint_furniture(buf: &mut [u8], w: u32, h: u32, lat: f64, zoom: u32, attribution: &str) {
    let mpp = meters_per_pixel(lat, zoom);
    let bar_m = nice_bar_length(mpp * 100.0);
    if bar_m > 0.0 {
        let bar_w = (bar_m / mpp).round().max(4.0);
        let x0 = 8.0f64;
        let bar_top = f64::from(h) - 8.0 - SCALE_BAR_HEIGHT;
        let label = scale_label(bar_m);
        draw_text_rgb(
            buf,
            w,
            h,
            &label,
            MAP_TEXT_SIZE,
            x0,
            bar_top - 12.0,
            (0, 0, 0),
        );
        let half = (bar_w / 2.0).round();
        fill_rect(
            buf,
            w,
            h,
            x0 as i64,
            bar_top as i64,
            (x0 + half) as i64,
            (bar_top + SCALE_BAR_HEIGHT) as i64,
            (0, 0, 0),
        );
        fill_rect(
            buf,
            w,
            h,
            (x0 + half + 1.0) as i64,
            bar_top as i64,
            (x0 + bar_w) as i64,
            (bar_top + SCALE_BAR_HEIGHT) as i64,
            (255, 255, 255),
        );
        // A one-pixel dark frame keeps the white half visible over
        // bright tiles.
        for x in x0 as i64..=(x0 + bar_w) as i64 {
            put_px(buf, w, h, x, bar_top as i64 - 1, 0, 0, 0);
            put_px(
                buf,
                w,
                h,
                x,
                (bar_top + SCALE_BAR_HEIGHT) as i64 + 1,
                0,
                0,
                0,
            );
        }
    }
    if !attribution.is_empty() {
        let tw = text_width_px(attribution, MAP_TEXT_SIZE).ceil() as i64;
        let pill_w = tw + 10;
        let pill_h = 18i64;
        let px1 = w as i64 - 6;
        let px0 = (px1 - pill_w).max(0);
        let py1 = h as i64 - 6;
        let py0 = (py1 - pill_h).max(0);
        fill_rect(buf, w, h, px0, py0, px1, py1, (255, 255, 255));
        draw_text_rgb(
            buf,
            w,
            h,
            attribution,
            MAP_TEXT_SIZE,
            (px0 + 5) as f64,
            (py0 + 4) as f64,
            (0, 0, 0),
        );
    }
}

/// Pre-render one map viewport to straight opaque RGBA (Sprint 21
/// adapters, T004): tiles composite at `center`/`zoom`, overlays
/// paint above, uncovered cells show the named placeholder, and the
/// scale bar plus attribution always render. Only local file reads
/// and PNG decodes — never the network. A vanished or replaced tile
/// refuses naming the file before frame one, like image sources.
pub fn render_viewport(
    folder: &Path,
    m: &MapClip,
    w: u32,
    h: u32,
) -> Result<(u32, u32, Vec<u8>), String> {
    let pack_abs = folder.join(&m.pack);
    if !pack_abs.is_dir() {
        return Err(format!(
            "render refused: map pack {:?} does not exist or is not readable; no MP4 was written",
            m.pack
        ));
    }
    let tile = MAP_TILE_PX;
    let (cx, cy) = notation::model::map_world_px(m.center_lat, m.center_lon, m.zoom);
    // One integer origin per viewport: tiles and overlays share it,
    // so a fractional center still composites byte-identically.
    let left = (cx - f64::from(w) / 2.0).round();
    let top = (cy - f64::from(h) / 2.0).round();
    let mut rgb = vec![PLACEHOLDER_BG.0; w as usize * h as usize * 3];
    for i in 0..(w as usize * h as usize) {
        rgb[i * 3] = PLACEHOLDER_BG.0;
        rgb[i * 3 + 1] = PLACEHOLDER_BG.1;
        rgb[i * 3 + 2] = PLACEHOLDER_BG.2;
    }
    let (x0, x1, y0, y1) = notation::model::map_tile_range(
        m.center_lat,
        m.center_lon,
        m.zoom,
        f64::from(w),
        f64::from(h),
    );
    for ty in y0..=y1 {
        for tx in x0..=x1 {
            let rel = format!("{}/{tx}/{ty}.png", m.zoom);
            let tile_path = pack_abs.join(&rel);
            let ox = tx as i64 * tile as i64 - left as i64;
            let oy = ty as i64 * tile as i64 - top as i64;
            if !tile_path.is_file() {
                fill_rect(
                    &mut rgb,
                    w,
                    h,
                    ox,
                    oy,
                    ox + tile as i64 - 1,
                    oy + tile as i64 - 1,
                    PLACEHOLDER_BG,
                );
                draw_text_rgb(
                    &mut rgb,
                    w,
                    h,
                    MAP_PLACEHOLDER_NAME,
                    MAP_TEXT_SIZE,
                    (ox + 6) as f64,
                    (oy + 6) as f64,
                    PLACEHOLDER_INK,
                );
                continue;
            }
            let (tw, th, rgba) = decode_tile_png(&tile_path)
                .map_err(|detail| format!("render refused: {detail}; no MP4 was written"))?;
            debug_assert_eq!((tw, th), (tile, tile));
            for py in 0..th as i64 {
                for px in 0..tw as i64 {
                    let si = (py as u32 * tw + px as u32) as usize * 4;
                    put_px(
                        &mut rgb,
                        w,
                        h,
                        ox + px,
                        oy + py,
                        rgba[si],
                        rgba[si + 1],
                        rgba[si + 2],
                    );
                }
            }
        }
    }
    // Overlays paint above the tiles in document order: areas, then
    // lines, then pins, then bare labels.
    for pg in &m.polygons {
        let pts: Vec<(f64, f64)> = pg
            .points
            .iter()
            .map(|(lat, lon)| project(*lat, *lon, m.zoom, left, top))
            .collect();
        fill_ring(&mut rgb, w, h, &pts, as_rgb(&pg.fill));
        if pg.border_width > 0.0 {
            stroke_path(
                &mut rgb,
                w,
                h,
                &pts,
                true,
                pg.border_width,
                as_rgb(&pg.border_color),
            );
        }
    }
    for pl in &m.polylines {
        let pts: Vec<(f64, f64)> = pl
            .points
            .iter()
            .map(|(lat, lon)| project(*lat, *lon, m.zoom, left, top))
            .collect();
        stroke_path(&mut rgb, w, h, &pts, false, pl.width, as_rgb(&pl.color));
    }
    for mk in &m.markers {
        let (px, py) = project(mk.lat, mk.lon, m.zoom, left, top);
        // A white ring under the pin keeps it visible over any tile.
        fill_disc(&mut rgb, w, h, px, py, 7.0, (255, 255, 255));
        fill_disc(&mut rgb, w, h, px, py, 5.5, as_rgb(&mk.color));
        if let Some(label) = mk.label.as_deref() {
            if !label.is_empty() {
                draw_text_rgb(
                    &mut rgb,
                    w,
                    h,
                    label,
                    12.0,
                    px + 9.0,
                    py - 6.0,
                    as_rgb(&mk.color),
                );
            }
        }
    }
    for lb in &m.labels {
        let (px, py) = project(lb.lat, lb.lon, m.zoom, left, top);
        draw_text_rgb(
            &mut rgb,
            w,
            h,
            &lb.value,
            lb.size,
            px + 2.0,
            py - lb.size / 2.0,
            as_rgb(&lb.color),
        );
    }
    paint_furniture(&mut rgb, w, h, m.center_lat, m.zoom, &m.attribution);
    // Straight opaque RGBA for the shared image stack.
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for triple in rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[triple[0], triple[1], triple[2], 255]);
    }
    Ok((w, h, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{MapLabel, MapMarker, MapPolygon, MapPolyline};

    /// The render path never touches the network: this module reads
    /// local files and decodes PNGs only. The assertion below guards
    /// the file itself, so a future socket import fails loudly here
    /// instead of phoning home at render time.
    #[test]
    fn map_module_stays_offline() {
        // Only the production code is scanned (the needle list below
        // lives in the test module, past the split point).
        let src = include_str!("map.rs").split("#[cfg(test)]").next().unwrap();
        for needle in [
            "TcpStream",
            "UdpSocket",
            "std::net",
            "reqwest",
            "hyper",
            "url::",
            "http://",
            "https://",
            "fetch(",
        ] {
            assert!(
                !src.contains(needle),
                "map renderer must stay offline; found {needle:?}"
            );
        }
        // Latitude clamps to Web Mercator, never NaN from tan/sec.
        let (x, y) = notation::model::map_world_px(notation::model::MAP_MAX_LAT, 180.0, 3);
        assert!(x.is_finite() && y.is_finite());
        let (x0, x1, y0, y1) = notation::model::map_tile_range(0.0, 0.0, 2, 640.0, 480.0);
        assert!(x0 <= x1 && y0 <= y1);
        assert!(x1 - x0 <= 4 && y1 - y0 <= 4);
    }

    #[test]
    fn viewport_keys_agree_on_equal_inputs() {
        let clip = test_clip("tiles/osm", 48.0, 11.0, 12);
        let a = viewport_key(&clip, 640, 480);
        let b = viewport_key(&clip, 640, 480);
        assert_eq!(a, b);
        assert!(a.starts_with("map-viewports/map-viewport-"));
        // Any input bit changes the key: no cross-viewport sharing.
        let mut moved = clip.clone();
        moved.center_lon = 11.5;
        assert_ne!(a, viewport_key(&moved, 640, 480));
        let mut relabeled = clip;
        relabeled.attribution = "© Other".to_string();
        assert_ne!(a, viewport_key(&relabeled, 640, 480));
    }

    #[test]
    fn overlays_change_viewport_pixels() {
        let dir = tmp("overlays");
        write_pack(&dir, "0/0/0.png", (200, 210, 220));
        let mut clip = test_clip("tiles", 0.0, 0.0, 0);
        clip.width = Some(256.0);
        clip.height = Some(256.0);
        std::fs::create_dir_all(dir.join("tiles/0/0")).unwrap();
        std::fs::rename(dir.join("0/0/0.png"), dir.join("tiles/0/0/0.png")).unwrap();
        let bare = render_viewport(&dir, &clip, 256, 256).unwrap().2;
        clip.markers.push(MapMarker {
            lat: 0.0,
            lon: 0.0,
            label: Some("hub".to_string()),
            color: Rgb { r: 255, g: 0, b: 0 },
            line: 1,
            column: 1,
        });
        clip.labels.push(MapLabel {
            lat: 10.0,
            lon: 10.0,
            value: "north".to_string(),
            size: 12.0,
            color: Rgb { r: 0, g: 0, b: 0 },
            line: 1,
            column: 1,
        });
        clip.polylines.push(MapPolyline {
            points: vec![(-20.0, -20.0), (20.0, 20.0)],
            width: 3.0,
            color: Rgb { r: 0, g: 0, b: 255 },
            line: 1,
            column: 1,
        });
        clip.polygons.push(MapPolygon {
            points: vec![(-30.0, 20.0), (-30.0, 40.0), (-10.0, 40.0)],
            fill: Rgb { r: 0, g: 255, b: 0 },
            border_width: 2.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            line: 1,
            column: 1,
        });
        let dressed = render_viewport(&dir, &clip, 256, 256).unwrap().2;
        assert_ne!(bare, dressed, "overlays must paint pixels");
        // Deterministic reruns agree byte for byte.
        let again = render_viewport(&dir, &clip, 256, 256).unwrap().2;
        assert_eq!(dressed, again);
    }

    #[test]
    fn placeholder_covers_missing_tiles_by_name() {
        let dir = tmp("placeholder");
        write_pack(&dir, "1/0/0.png", (200, 210, 220));
        std::fs::create_dir_all(dir.join("tiles/1/0")).unwrap();
        std::fs::rename(dir.join("1/0/0.png"), dir.join("tiles/1/0/0.png")).unwrap();
        // Zoom 1 at this center needs tiles x=0..=1: x=1 is missing,
        // so the right half must carry the named placeholder fill.
        let mut clip = test_clip("tiles", 0.0, 0.0, 1);
        clip.width = Some(512.0);
        clip.height = Some(256.0);
        let (_, _, rgba) = render_viewport(&dir, &clip, 512, 256).unwrap();
        let at = |x: u32, y: u32| {
            let i = (y * 512 + x) as usize * 4;
            (rgba[i], rgba[i + 1], rgba[i + 2])
        };
        // Tile (0,0) sits at rows 0..=127; tile (1,1) is missing, so
        // the viewport's lower-right quadrant is placeholder fill.
        assert_eq!(at(10, 64), (200, 210, 220));
        assert_eq!(
            at(400, 200),
            (PLACEHOLDER_BG.0, PLACEHOLDER_BG.1, PLACEHOLDER_BG.2)
        );
    }

    #[test]
    fn scale_bar_and_attribution_render() {
        let dir = tmp("furniture");
        write_pack(&dir, "2/1/1.png", (210, 220, 200));
        write_pack(&dir, "2/2/1.png", (200, 210, 220));
        write_pack(&dir, "2/1/2.png", (220, 200, 210));
        write_pack(&dir, "2/2/2.png", (190, 200, 210));
        std::fs::create_dir_all(dir.join("tiles/2/1")).unwrap();
        std::fs::create_dir_all(dir.join("tiles/2/2")).unwrap();
        for f in ["2/1/1.png", "2/2/1.png", "2/1/2.png", "2/2/2.png"] {
            let (z, x, y) = (&f[0..1], &f[2..3], &f[4..5]);
            std::fs::rename(dir.join(f), dir.join(format!("tiles/{z}/{x}/{y}.png"))).unwrap();
        }
        let mut clip = test_clip("tiles", 0.0, 0.0, 2);
        clip.width = Some(512.0);
        clip.height = Some(512.0);
        let (_, _, rgba) = render_viewport(&dir, &clip, 512, 512).unwrap();
        // The scale bar corner and the attribution corner differ from
        // the tile fills: both furniture pieces painted.
        let corner = |x: u32, y: u32| {
            let i = (y * 512 + x) as usize * 4;
            (rgba[i], rgba[i + 1], rgba[i + 2])
        };
        let mut bar_ink = false;
        for y in 480..506 {
            for x in 8..140 {
                let p = corner(x, y);
                if p == (0, 0, 0) || p == (255, 255, 255) {
                    bar_ink = true;
                }
            }
        }
        assert!(bar_ink, "scale bar must paint black/white segments");
        let mut pill = false;
        for y in 480..506 {
            for x in 300..506 {
                if corner(x, y) == (255, 255, 255) {
                    pill = true;
                }
            }
        }
        assert!(pill, "attribution pill must paint");
        // Attribution text inks dark pixels onto the pill.
        let mut ink = false;
        for y in 484..502 {
            for x in 300..506 {
                let p = corner(x, y);
                if p != (255, 255, 255) && p.0 < 40 && p.1 < 40 && p.2 < 40 {
                    ink = true;
                }
            }
        }
        assert!(ink, "attribution text must paint");
    }

    // --- test helpers (local PNG tiles, hand-encoded) -----------------

    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-map-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for b in data {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn chunk(kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>) {
        let len = data.len() as u32;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut c = Vec::new();
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    }

    /// Hand-encode one solid 256x256 RGB PNG (stored, no compression):
    /// fixtures stay dependency-light and byte-exact.
    fn write_pack(dir: &Path, rel: &str, color: (u8, u8, u8)) {
        let mut raw = Vec::with_capacity(256 * 257);
        for _ in 0..256 {
            raw.push(0);
            for _ in 0..256 {
                raw.extend_from_slice(&[color.0, color.1, color.2]);
            }
        }
        // Minimal zlib wrapper around stored blocks (max 65535 bytes
        // each): CMF/FLG header, then the blocks, then Adler-32 over
        // the raw scanlines.
        let mut deflate = vec![0x78, 0x01];
        let mut rest = raw.as_slice();
        while !rest.is_empty() {
            let take = rest.len().min(65535);
            let (head, tail) = rest.split_at(take);
            rest = tail;
            deflate.push(u8::from(rest.is_empty()));
            deflate.extend_from_slice(&(take as u16).to_le_bytes());
            deflate.extend_from_slice(&(!(take as u16)).to_le_bytes());
            deflate.extend_from_slice(head);
        }
        // Adler-32 over the raw scanlines.
        let (mut a, mut b) = (1u32, 0u32);
        for byte in &raw {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
        deflate.extend_from_slice(&((b << 16 | a).to_be_bytes()));
        let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&256u32.to_be_bytes());
        ihdr.extend_from_slice(&256u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(b"IHDR", &ihdr, &mut png);
        chunk(b"IDAT", &deflate, &mut png);
        chunk(b"IEND", &[], &mut png);
        let abs = dir.join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(abs, png).unwrap();
    }

    fn test_clip(pack: &str, lat: f64, lon: f64, zoom: u32) -> MapClip {
        MapClip {
            asserts: notation::model::MotionAsserts::default(),
            pack: pack.to_string(),
            center_lat: lat,
            center_lon: lon,
            zoom,
            attribution: "© fixtures".to_string(),
            markers: Vec::new(),
            labels: Vec::new(),
            polylines: Vec::new(),
            polygons: Vec::new(),
            start: 0.0,
            duration: 2.0,
            id: None,
            track: notation::model::TrackKind::Graphics,
            label: None,
            start_after: None,
            x: 0.0,
            y: 0.0,
            width: None,
            height: None,
            order: 0,
            turn: notation::model::Turn3D::flat(),
            filter_blur: 0.0,
            grade: notation::model::Grade::default(),
            effects: Vec::new(),
            vfx: notation::model::VfxChain::default(),
            hidden: false,
            line: 1,
            column: 1,
        }
    }
}
