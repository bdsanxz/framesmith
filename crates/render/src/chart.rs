//! Native chart rasterizer (Sprint 21 adapters, CAP-002/T002): eight
//! chart kinds drawn in pure Rust with no browser, no JavaScript, and
//! no network. The painter consumes a [`ResolvedChart`](crate::frame::ResolvedChart)
//! — the validated kind, data, and style with per-frame geometry — and
//! returns straight RGBA pixels the shared image blitter composites,
//! so scale, rotation, opacity, turns, blur, grades, effects, and VFX
//! ride the exact image path. Same inputs rasterize byte-identically:
//! fixed float operation order, integer pixel coverage, and the
//! deterministic bitmap fallback face for every label.

use glyph::font::Face;
use notation::model::{ChartKind, ChartPoint, FontStyle, Rgb};

/// Rasterized chart picture: straight RGBA, row-major.
pub struct ChartPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Clip-level chart inputs the painter reads (Sprint 21 adapters,
/// CAP-002/T002): everything that affects pixels, so equal specs
/// rasterize byte-identically. Geometry motion never reaches here —
/// the prepare stage bakes the authored size and the shared blitter
/// moves the picture, exactly like pre-rendered map viewports.
pub struct ChartSpec<'a> {
    pub kind: ChartKind,
    pub title: String,
    pub data: &'a [ChartPoint],
    pub background: Rgb,
    pub border_width: f64,
    pub border_color: Rgb,
    pub palette: &'a [Rgb],
    pub show_legend: bool,
    pub show_gridlines: bool,
    pub show_value_labels: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// Pixel size of one pre-rendered chart (Sprint 21 adapters,
/// CAP-002/T002): the authored extents, falling back to the canvas
/// like images, rounded to whole pixels and clamped to [`MAX_SIDE`].
pub fn viewport_px(ch: &notation::model::ChartClip, canvas: (u32, u32)) -> (u32, u32) {
    let w = ch.width.unwrap_or(f64::from(canvas.0)).clamp(1.0, MAX_SIDE);
    let h = ch
        .height
        .unwrap_or(f64::from(canvas.1))
        .clamp(1.0, MAX_SIDE);
    (w.round().max(1.0) as u32, h.round().max(1.0) as u32)
}

/// Canonical key of one pre-rendered chart (Sprint 21 adapters,
/// CAP-002/T002): a wrapping FNV-1a mix over the kind, the pixel
/// size, the title, every row, and every style input, so the prepare
/// stage and the frame stage address the same entry and equal charts
/// share one raster. The `chart-pictures/` prefix is synthetic — no
/// real file collides with it.
pub fn viewport_key(ch: &notation::model::ChartClip, w: u32, h: u32) -> String {
    let mut canon = String::new();
    canon.push_str(ch.kind.name());
    canon.push_str(&format!("|{w}x{h}|"));
    canon.push_str(ch.title.as_deref().unwrap_or(""));
    for p in &ch.data {
        canon.push_str(&format!(
            "|{:?},{:?},{:?},{:?}",
            p.series.as_deref().unwrap_or(""),
            p.label,
            p.value,
            p.x
        ));
    }
    // Position and paint order never enter the key: the baked raster
    // is position-independent, so equal pictures share one entry
    // wherever they sit.
    canon.push_str(&format!(
        "|{:02x}{:02x}{:02x}|{}|{:02x}{:02x}{:02x}|{}|{}|{}|{:?}|{:?}",
        ch.background.r,
        ch.background.g,
        ch.background.b,
        ch.border_width,
        ch.border_color.r,
        ch.border_color.g,
        ch.border_color.b,
        ch.show_legend,
        ch.show_gridlines,
        ch.show_value_labels,
        ch.min,
        ch.max,
    ));
    for c in &ch.palette {
        canon.push_str(&format!("|{:02x}{:02x}{:02x}", c.r, c.g, c.b));
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in canon.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("chart-pictures/{hash:016x}")
}

/// Pre-render one chart clip (Sprint 21 adapters, CAP-002/T002): the
/// authored data and style baked to straight RGBA at (`w`, `h`).
/// Infallible — validation guarantees finite geometry and data — and
/// deterministic: equal inputs return equal bytes, never invented
/// points (empty data renders the named panel with empty axes).
pub fn render_viewport(ch: &notation::model::ChartClip, w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let spec = ChartSpec {
        kind: ch.kind,
        title: ch
            .title
            .clone()
            .unwrap_or_else(|| ch.kind.name().to_string()),
        data: &ch.data,
        background: ch.background,
        border_width: ch.border_width,
        border_color: ch.border_color,
        palette: &ch.palette,
        show_legend: ch.show_legend,
        show_gridlines: ch.show_gridlines,
        show_value_labels: ch.show_value_labels,
        min: ch.min,
        max: ch.max,
    };
    let pixels = rasterize(&spec, w, h);
    (pixels.width, pixels.height, pixels.rgba)
}

/// Hard cap per raster side in pixels: bounds a pathological extent
/// (validation only requires positive) while clearing every real
/// canvas and supersample multiple. Deterministic — the same inputs
/// clamp the same way.
const MAX_SIDE: f64 = 8192.0;

/// Panel ink colors (deterministic constants, not style inputs).
const INK: (u8, u8, u8) = (26, 26, 26);
const AXIS: (u8, u8, u8) = (70, 70, 70);
const GRID: (u8, u8, u8) = (214, 214, 214);
const TRACK: (u8, u8, u8) = (225, 225, 225);

/// Text sizes in canvas pixels: panel heading, then every label.
const TITLE_SIZE: f64 = 15.0;
const LABEL_SIZE: f64 = 11.0;

/// Rasterize one chart spec at (`w`, `h`) whole pixels: the panel,
/// the kind, and the legend. Never fails and never invents points:
/// empty data draws the named panel with empty axes.
fn rasterize(spec: &ChartSpec, w: u32, h: u32) -> ChartPixels {
    let ch = spec;
    let pw = w.max(1);
    let ph = h.max(1);
    let bg = (ch.background.r, ch.background.g, ch.background.b);
    let mut buf = vec![0u8; pw as usize * ph as usize * 3];
    for px in buf.chunks_exact_mut(3) {
        px[0] = bg.0;
        px[1] = bg.1;
        px[2] = bg.2;
    }
    let face = Face::fallback();
    // Panel border first (under every later pixel).
    let bw = ch.border_width.max(0.0);
    if bw > 0.0 {
        let bc = (ch.border_color.r, ch.border_color.g, ch.border_color.b);
        let b = bw.ceil() as i64;
        fill_rect(&mut buf, pw, ph, 0.0, 0.0, pw as f64, b as f64, bc);
        fill_rect(
            &mut buf,
            pw,
            ph,
            0.0,
            (ph as i64 - b).max(0) as f64,
            pw as f64,
            ph as f64,
            bc,
        );
        fill_rect(&mut buf, pw, ph, 0.0, 0.0, b as f64, ph as f64, bc);
        fill_rect(
            &mut buf,
            pw,
            ph,
            (pw as i64 - b).max(0) as f64,
            0.0,
            pw as f64,
            ph as f64,
            bc,
        );
    }
    // Panel heading: the authored title, else the kind name — so even
    // empty axes carry a name.
    draw_text(
        &mut buf,
        pw,
        ph,
        &face,
        &ch.title,
        TITLE_SIZE,
        10.0,
        8.0 + TITLE_SIZE,
        INK,
    );
    // Daylight between the heading and the top tick labels.
    let body_y = 8.0 + TITLE_SIZE + 14.0;
    match ch.kind {
        ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::Scatter => {
            paint_cartesian(&mut buf, pw, ph, &face, ch, body_y);
        }
        ChartKind::Pie => paint_pie(&mut buf, pw, ph, &face, ch, body_y, false),
        ChartKind::Donut => paint_pie(&mut buf, pw, ph, &face, ch, body_y, true),
        ChartKind::Radar => paint_radar(&mut buf, pw, ph, &face, ch, body_y),
        ChartKind::Gauge => paint_gauge(&mut buf, pw, ph, &face, ch, body_y),
    }
    paint_legend(&mut buf, pw, ph, &face, ch);
    // Straight RGB24 to straight RGBA (opaque: opacity rides the
    // shared compositor, never baked pixels).
    let mut rgba = Vec::with_capacity(pw as usize * ph as usize * 4);
    for px in buf.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    ChartPixels {
        width: pw,
        height: ph,
        rgba,
    }
}

/// Fill an axis-aligned rect (float bounds, rounded outward to whole
/// pixels, clipped to the buffer).
fn fill_rect(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    color: (u8, u8, u8),
) {
    let xa = (x0.floor() as i64).max(0).min(bw as i64);
    let xb = (x1.ceil() as i64).max(0).min(bw as i64);
    let ya = (y0.floor() as i64).max(0).min(bh as i64);
    let yb = (y1.ceil() as i64).max(0).min(bh as i64);
    for y in ya..yb {
        for x in xa..xb {
            let o = (y as u32 * bw + x as u32) as usize * 3;
            buf[o] = color.0;
            buf[o + 1] = color.1;
            buf[o + 2] = color.2;
        }
    }
}

/// One laid-out text width in pixels (fallback face, deterministic).
fn text_width(face: &Face, text: &str, size: f64) -> f64 {
    face.layout(text, size, None).width
}

/// Draw one left-anchored text run with its pen at (`x`, baseline `y`).
fn draw_text(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    face: &Face,
    text: &str,
    size: f64,
    x: f64,
    baseline_y: f64,
    color: (u8, u8, u8),
) {
    if text.is_empty() || size <= 0.0 {
        return;
    }
    let laid = face.layout(text, size, None);
    for line in &laid.lines {
        for g in &line.glyphs {
            face.blit(
                buf,
                bw,
                bh,
                g.ch,
                size,
                x + g.x,
                baseline_y,
                color,
                FontStyle::Regular,
            );
        }
    }
}

/// Draw one text run centered on `cx` (top edge `top`), clamped
/// sideways so edge labels (gauge bounds, pie shares) never clip.
fn draw_text_centered_clamped(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    face: &Face,
    text: &str,
    size: f64,
    cx: f64,
    top: f64,
) {
    let half = text_width(face, text, size) / 2.0;
    let margin = 2.0;
    let cx = if half * 2.0 + margin * 2.0 >= bw as f64 {
        bw as f64 / 2.0
    } else {
        cx.clamp(half + margin, bw as f64 - half - margin)
    };
    draw_text_centered(buf, bw, bh, face, text, size, cx, top);
}

/// Draw one text run centered on `cx` (top edge `top`).
fn draw_text_centered(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    face: &Face,
    text: &str,
    size: f64,
    cx: f64,
    top: f64,
) {
    let (ascent, _) = face.line_metrics_px(size);
    draw_text(
        buf,
        bw,
        bh,
        face,
        text,
        size,
        cx - text_width(face, text, size) / 2.0,
        top + ascent,
        INK,
    );
}

/// Draw one text run right-anchored at `x` (top edge `top`).
fn draw_text_right(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    face: &Face,
    text: &str,
    size: f64,
    x: f64,
    top: f64,
    color: (u8, u8, u8),
) {
    let (ascent, _) = face.line_metrics_px(size);
    draw_text(
        buf,
        bw,
        bh,
        face,
        text,
        size,
        x - text_width(face, text, size),
        top + ascent,
        color,
    );
}

/// Filled disc (pixel centers within radius `r`).
fn disc(buf: &mut [u8], bw: u32, bh: u32, cx: f64, cy: f64, r: f64, color: (u8, u8, u8)) {
    if r <= 0.0 {
        return;
    }
    let xa = (cx - r).floor() as i64;
    let xb = (cx + r).ceil() as i64;
    let ya = (cy - r).floor() as i64;
    let yb = (cy + r).ceil() as i64;
    for y in ya..=yb {
        if y < 0 || y >= bh as i64 {
            continue;
        }
        for x in xa..=xb {
            if x < 0 || x >= bw as i64 {
                continue;
            }
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            if dx * dx + dy * dy <= r * r {
                let o = (y as u32 * bw + x as u32) as usize * 3;
                buf[o] = color.0;
                buf[o + 1] = color.1;
                buf[o + 2] = color.2;
            }
        }
    }
}

/// One stroked segment: DDA steps with disc stamps (fixed order, so
/// reruns land the same pixels).
fn seg(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    w: f64,
    color: (u8, u8, u8),
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i64;
    let r = (w / 2.0).max(0.5);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        disc(buf, bw, bh, x0 + dx * t, y0 + dy * t, r, color);
    }
}

/// Even-odd scanline fill of a polygon (fixed vertex order).
fn poly_fill(buf: &mut [u8], bw: u32, bh: u32, pts: &[(f64, f64)], color: (u8, u8, u8)) {
    if pts.len() < 3 {
        return;
    }
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for p in pts {
        ymin = ymin.min(p.1);
        ymax = ymax.max(p.1);
    }
    let ya = (ymin.floor() as i64).max(0).min(bh as i64);
    let yb = (ymax.ceil() as i64).max(0).min(bh as i64);
    let n = pts.len();
    for y in ya..yb {
        let yc = y as f64 + 0.5;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            if (a.1 <= yc && b.1 > yc) || (b.1 <= yc && a.1 > yc) {
                let t = (yc - a.1) / (b.1 - a.1);
                xs.push(a.0 + t * (b.0 - a.0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut k = 0;
        while k + 1 < xs.len() {
            fill_rect(
                buf,
                bw,
                bh,
                xs[k],
                y as f64,
                xs[k + 1],
                y as f64 + 1.0,
                color,
            );
            k += 2;
        }
    }
}

/// Mix two colors by `t` (0 = `a`, 1 = `b`), channel-rounded.
fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let m = |x: u8, y: u8| {
        (x as f64 * (1.0 - t) + y as f64 * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// One series in first-appearance order with its point indexes.
struct Series<'a> {
    name: String,
    points: Vec<(usize, &'a ChartPoint)>,
}

/// Group rows into series in first-appearance order (deterministic:
/// no hash iteration anywhere in the painter).
fn group_series(data: &[ChartPoint]) -> Vec<Series<'_>> {
    let mut out: Vec<Series<'_>> = Vec::new();
    for (i, p) in data.iter().enumerate() {
        let key = p.series.as_deref().unwrap_or("");
        match out.iter_mut().find(|s| s.name == key) {
            Some(s) => s.points.push((i, p)),
            None => out.push(Series {
                name: key.to_string(),
                points: vec![(i, p)],
            }),
        }
    }
    out
}

/// Format one datum for labels: integers bare, else up to 4 decimals
/// trimmed (deterministic, no locale).
fn fmt_val(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let mut s = format!("{v:.4}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// Nice axis ticks covering `[min, max]` (at most `target` + 1 ticks):
/// 1/2/2.5/5/10 steps on a decade. A degenerate range widens by one
/// unit each side so empty and single-valued data still draw an axis.
fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    let (mut lo, mut hi) = (min, max);
    if !(lo < hi) {
        if !lo.is_finite() {
            return vec![0.0];
        }
        lo -= 1.0;
        hi += 1.0;
    }
    let target = target.max(1) as f64;
    let raw = (hi - lo) / target;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        mag
    } else if norm <= 2.0 {
        2.0 * mag
    } else if norm <= 2.5 {
        2.5 * mag
    } else if norm <= 5.0 {
        5.0 * mag
    } else {
        10.0 * mag
    };
    let mut ticks = Vec::new();
    let mut t = (lo / step).floor() * step;
    // Guard the walk: a fixed cap keeps hostile ranges finite.
    for _ in 0..64 {
        if t > hi + step * 1e-9 {
            break;
        }
        // Snap float dust to the step grid so labels print cleanly.
        let snapped = (t / step).round() * step;
        ticks.push(if snapped == 0.0 { 0.0 } else { snapped });
        t += step;
    }
    // Cover the top end: the last gridline must reach past the data
    // max, or the topmost datum would draw above the axis.
    if ticks.last().copied().unwrap_or(hi) < hi - step * 1e-9 {
        let next = ticks.last().copied().unwrap_or(hi) + step;
        let snapped = (next / step).round() * step;
        ticks.push(if snapped == 0.0 { 0.0 } else { snapped });
    }
    ticks
}

/// Cartesian plot geometry: left axis gutter, bottom category gutter,
/// both in whole pixels.
struct Plot {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn cartesian_plot(pw: u32, ph: u32, body_y: f64, legend_h: f64) -> Plot {
    let left = 46.0;
    let right = 10.0;
    let bottom = if legend_h > 0.0 {
        legend_h + 22.0
    } else {
        24.0
    };
    Plot {
        x: left,
        y: body_y,
        w: (pw as f64 - left - right).max(8.0),
        h: (ph as f64 - body_y - bottom).max(8.0),
    }
}

/// Whether this panel reserves a bottom legend strip.
fn legend_height(ch: &ChartSpec) -> f64 {
    if ch.show_legend && !legend_entries(ch).is_empty() {
        20.0
    } else {
        0.0
    }
}

/// Paint every cartesian kind (bar, line, area, scatter): axes,
/// gridlines, ticks, data, and value labels.
fn paint_cartesian(buf: &mut [u8], pw: u32, ph: u32, face: &Face, ch: &ChartSpec, body_y: f64) {
    let legend_h = legend_height(ch);
    let plot = cartesian_plot(pw, ph, body_y, legend_h);
    let bg = (ch.background.r, ch.background.g, ch.background.b);
    if ch.data.is_empty() {
        // Named empty axes: the frame lines with no ticks or data.
        seg(
            buf,
            pw,
            ph,
            plot.x,
            plot.y,
            plot.x,
            plot.y + plot.h,
            1.0,
            AXIS,
        );
        seg(
            buf,
            pw,
            ph,
            plot.x,
            plot.y + plot.h,
            plot.x + plot.w,
            plot.y + plot.h,
            1.0,
            AXIS,
        );
        return;
    }
    match ch.kind {
        ChartKind::Scatter => paint_scatter(buf, pw, ph, face, ch, &plot, bg),
        _ => paint_category(buf, pw, ph, face, ch, &plot, bg),
    }
}

/// Distinct category labels in first-appearance order.
fn categories(data: &[ChartPoint]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for p in data {
        if !out.contains(&p.label.as_str()) {
            out.push(p.label.as_str());
        }
    }
    out
}

/// Series colors in series order, cycling the validated palette.
/// Validation guarantees a non-empty palette; the single-color
/// fallback keeps the painter total on hand-built inputs.
fn palette_of(ch: &ChartSpec) -> Vec<(u8, u8, u8)> {
    let pal = ch
        .palette
        .iter()
        .map(|c| (c.r, c.g, c.b))
        .collect::<Vec<_>>();
    if pal.is_empty() {
        vec![(78, 121, 167)]
    } else {
        pal
    }
}

/// Paint bar, line, and area charts over category positions.
#[allow(clippy::too_many_arguments)]
fn paint_category(
    buf: &mut [u8],
    pw: u32,
    ph: u32,
    face: &Face,
    ch: &ChartSpec,
    plot: &Plot,
    bg: (u8, u8, u8),
) {
    let cats = categories(&ch.data);
    let series = group_series(&ch.data);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in ch.data {
        lo = lo.min(p.value);
        hi = hi.max(p.value);
    }
    if ch.kind == ChartKind::Bar {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    let ticks = nice_ticks(lo, hi, 4);
    let tlo = ticks.first().copied().unwrap_or(0.0);
    let thi = ticks.last().copied().unwrap_or(1.0);
    let span = (thi - tlo).max(f64::EPSILON);
    let y_of = |v: f64| plot.y + plot.h - (v - tlo) / span * plot.h;
    // Gridlines plus tick labels.
    for t in &ticks {
        let y = y_of(*t);
        if ch.show_gridlines {
            fill_rect(buf, pw, ph, plot.x, y, plot.x + plot.w, y + 1.0, GRID);
        }
        draw_text_right(
            buf,
            pw,
            ph,
            face,
            &fmt_val(*t),
            LABEL_SIZE,
            plot.x - 4.0,
            y - 6.0,
            AXIS,
        );
    }
    // Axes.
    seg(
        buf,
        pw,
        ph,
        plot.x,
        plot.y,
        plot.x,
        plot.y + plot.h,
        1.0,
        AXIS,
    );
    seg(
        buf,
        pw,
        ph,
        plot.x,
        plot.y + plot.h,
        plot.x + plot.w,
        plot.y + plot.h,
        1.0,
        AXIS,
    );
    let n = cats.len().max(1) as f64;
    let x_of = |i: usize| plot.x + (i as f64 + 0.5) * plot.w / n;
    // Category labels with deterministic thinning to fit.
    let mut stride = 1usize;
    loop {
        let widest = (0..cats.len())
            .step_by(stride)
            .map(|i| text_width(face, cats[i], LABEL_SIZE))
            .fold(0.0f64, f64::max);
        let shown = cats.len().div_ceil(stride) as f64;
        if widest + 4.0 <= plot.w / shown.max(1.0) || stride >= cats.len().max(1) {
            break;
        }
        stride += 1;
    }
    for (i, c) in cats.iter().enumerate() {
        if i % stride != 0 {
            continue;
        }
        draw_text_centered(
            buf,
            pw,
            ph,
            face,
            c,
            LABEL_SIZE,
            x_of(i),
            plot.y + plot.h + 4.0,
        );
    }
    let pal = palette_of(ch);
    let ns = series.len().max(1) as f64;
    for (si, s) in series.iter().enumerate() {
        let color = pal[si % pal.len()];
        match ch.kind {
            ChartKind::Bar => {
                let slot = plot.w / n;
                let bw = (slot * 0.7 / ns).max(1.0);
                for p in &s.points {
                    let ci = cats
                        .iter()
                        .position(|c| *c == p.1.label.as_str())
                        .unwrap_or(0);
                    let cx =
                        plot.x + (ci as f64 + 0.5) * slot + (si as f64 - (ns - 1.0) / 2.0) * bw;
                    let yv = y_of(p.1.value);
                    let yz = y_of(0.0);
                    let (top, bot) = if yv <= yz { (yv, yz) } else { (yz, yv) };
                    fill_rect(
                        buf,
                        pw,
                        ph,
                        cx - bw / 2.0,
                        top,
                        cx + bw / 2.0,
                        bot.max(top + 1.0),
                        color,
                    );
                    if ch.show_value_labels {
                        let label = fmt_val(p.1.value);
                        draw_text_centered(buf, pw, ph, face, &label, LABEL_SIZE, cx, top - 13.0);
                    }
                }
            }
            ChartKind::Line | ChartKind::Area => {
                let mut pts: Vec<(f64, f64)> = Vec::new();
                for p in &s.points {
                    let ci = cats
                        .iter()
                        .position(|c| *c == p.1.label.as_str())
                        .unwrap_or(0);
                    pts.push((x_of(ci), y_of(p.1.value)));
                }
                if ch.kind == ChartKind::Area && pts.len() >= 2 {
                    let base = if tlo <= 0.0 && 0.0 <= thi {
                        y_of(0.0)
                    } else {
                        y_of(tlo)
                    };
                    let mut poly: Vec<(f64, f64)> = vec![(pts[0].0, base)];
                    poly.extend_from_slice(&pts);
                    poly.push((pts[pts.len() - 1].0, base));
                    poly_fill(buf, pw, ph, &poly, mix(color, bg, 0.6));
                }
                for w in pts.windows(2) {
                    seg(buf, pw, ph, w[0].0, w[0].1, w[1].0, w[1].1, 2.0, color);
                }
                for (k, pt) in pts.iter().enumerate() {
                    disc(buf, pw, ph, pt.0, pt.1, 3.0, color);
                    if ch.show_value_labels {
                        let label = fmt_val(s.points[k].1.value);
                        draw_text_centered(
                            buf,
                            pw,
                            ph,
                            face,
                            &label,
                            LABEL_SIZE,
                            pt.0,
                            pt.1 - 16.0,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// Paint scatter charts: numeric x (row `x`, else the index within
/// the series) against the value, with nice ticks on both axes.
fn paint_scatter(
    buf: &mut [u8],
    pw: u32,
    ph: u32,
    face: &Face,
    ch: &ChartSpec,
    plot: &Plot,
    _bg: (u8, u8, u8),
) {
    let series = group_series(&ch.data);
    let mut xlo = f64::INFINITY;
    let mut xhi = f64::NEG_INFINITY;
    let mut ylo = f64::INFINITY;
    let mut yhi = f64::NEG_INFINITY;
    for s in series.iter() {
        for (k, p) in &s.points {
            let x = p.x.unwrap_or(*k as f64);
            xlo = xlo.min(x);
            xhi = xhi.max(x);
            ylo = ylo.min(p.value);
            yhi = yhi.max(p.value);
        }
    }
    let xticks = nice_ticks(xlo, xhi, 4);
    let yticks = nice_ticks(ylo, yhi, 4);
    let x0 = xticks.first().copied().unwrap_or(0.0);
    let x1 = xticks.last().copied().unwrap_or(1.0);
    let y0 = yticks.first().copied().unwrap_or(0.0);
    let y1 = yticks.last().copied().unwrap_or(1.0);
    let xspan = (x1 - x0).max(f64::EPSILON);
    let yspan = (y1 - y0).max(f64::EPSILON);
    let x_of = |v: f64| plot.x + (v - x0) / xspan * plot.w;
    let y_of = |v: f64| plot.y + plot.h - (v - y0) / yspan * plot.h;
    for t in &yticks {
        let y = y_of(*t);
        if ch.show_gridlines {
            fill_rect(buf, pw, ph, plot.x, y, plot.x + plot.w, y + 1.0, GRID);
        }
        draw_text_right(
            buf,
            pw,
            ph,
            face,
            &fmt_val(*t),
            LABEL_SIZE,
            plot.x - 4.0,
            y - 6.0,
            AXIS,
        );
    }
    for t in &xticks {
        let x = x_of(*t);
        if ch.show_gridlines {
            fill_rect(buf, pw, ph, x, plot.y, x + 1.0, plot.y + plot.h, GRID);
        }
        draw_text_centered(
            buf,
            pw,
            ph,
            face,
            &fmt_val(*t),
            LABEL_SIZE,
            x,
            plot.y + plot.h + 4.0,
        );
    }
    seg(
        buf,
        pw,
        ph,
        plot.x,
        plot.y,
        plot.x,
        plot.y + plot.h,
        1.0,
        AXIS,
    );
    seg(
        buf,
        pw,
        ph,
        plot.x,
        plot.y + plot.h,
        plot.x + plot.w,
        plot.y + plot.h,
        1.0,
        AXIS,
    );
    let pal = palette_of(ch);
    for (si, s) in series.iter().enumerate() {
        let color = pal[si % pal.len()];
        for (k, p) in &s.points {
            let x = x_of(p.x.unwrap_or(*k as f64));
            let y = y_of(p.value);
            disc(buf, pw, ph, x, y, 3.0, color);
            if ch.show_value_labels {
                draw_text(
                    buf,
                    pw,
                    ph,
                    face,
                    &fmt_val(p.value),
                    LABEL_SIZE,
                    x + 5.0,
                    y - 4.0,
                    INK,
                );
            }
        }
    }
}

/// Math angle of pixel (`x`, `y`) about (`cx`, `cy`) in degrees
/// [0, 360): y grows downward, so increasing angles run visually
/// clockwise from east.
fn pixel_angle(cx: f64, cy: f64, x: f64, y: f64) -> f64 {
    let mut a = (y - cy).atan2(x - cx).to_degrees();
    if a < 0.0 {
        a += 360.0;
    }
    a
}

/// Compass point of math degrees about (`cx`, `cy`) at radius `r`
/// (y grows downward).
fn polar(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let a = deg.to_radians();
    (cx + r * a.cos(), cy + r * a.sin())
}

/// Paint pie and donut charts: one series shares one whole from the
/// top, clockwise. Empty or all-zero data draws the named ring with
/// no slices, never invented shares.
#[allow(clippy::too_many_arguments)]
fn paint_pie(
    buf: &mut [u8],
    pw: u32,
    ph: u32,
    face: &Face,
    ch: &ChartSpec,
    body_y: f64,
    donut: bool,
) {
    let legend_h = legend_height(ch);
    let avail_h = (ph as f64 - body_y - legend_h - 16.0).max(8.0);
    let avail_w = (pw as f64 - 20.0).max(8.0);
    let r = (avail_w.min(avail_h) / 2.0 - 4.0).max(4.0);
    let cx = pw as f64 / 2.0;
    let cy = body_y + avail_h / 2.0;
    let total: f64 = ch.data.iter().map(|p| p.value.max(0.0)).sum();
    let pal = palette_of(ch);
    if total > 0.0 {
        let mut acc = 0.0;
        for (i, p) in ch.data.iter().enumerate() {
            let frac = p.value.max(0.0) / total;
            let a0 = 270.0 + acc * 360.0;
            acc += frac;
            let a1 = 270.0 + acc * 360.0;
            paint_wedge(buf, pw, ph, cx, cy, r, a0, a1, donut, pal[i % pal.len()]);
            if ch.show_value_labels && frac > 0.0 {
                let mid = (a0 + a1) / 2.0;
                let lr = if donut { r * 0.775 } else { r * 0.65 };
                let (tx, ty) = polar(cx, cy, lr, mid);
                draw_text_centered_clamped(
                    buf,
                    pw,
                    ph,
                    face,
                    &fmt_val(p.value),
                    LABEL_SIZE,
                    tx,
                    ty - 6.0,
                );
            }
        }
    }
    // Ring outline (also the whole of an empty chart).
    paint_ring(buf, pw, ph, cx, cy, r, donut, AXIS);
    // Slice separators.
    if total > 0.0 {
        let bg = (ch.background.r, ch.background.g, ch.background.b);
        let mut acc = 0.0;
        for p in ch.data {
            let a0 = 270.0 + acc * 360.0;
            acc += p.value.max(0.0) / total;
            let (ex, ey) = polar(cx, cy, r, a0);
            seg(buf, pw, ph, cx, cy, ex, ey, 1.0, bg);
        }
    }
}

/// Fill one angular wedge (degrees, increasing math angles run
/// visually clockwise from east; pies start at 270, the top).
fn paint_wedge(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    cx: f64,
    cy: f64,
    r: f64,
    a0: f64,
    a1: f64,
    donut: bool,
    color: (u8, u8, u8),
) {
    if a1 <= a0 || r <= 0.0 {
        return;
    }
    let r_in = if donut { r * 0.55 } else { 0.0 };
    let xa = (cx - r - 1.0).floor() as i64;
    let xb = (cx + r + 1.0).ceil() as i64;
    let ya = (cy - r - 1.0).floor() as i64;
    let yb = (cy + r + 1.0).ceil() as i64;
    // Normalize the sweep once: rotate the window into [0, 360).
    let span = a1 - a0;
    let full = span >= 360.0;
    let start = ((a0 % 360.0) + 360.0) % 360.0;
    for y in ya..=yb {
        if y < 0 || y >= bh as i64 {
            continue;
        }
        for x in xa..=xb {
            if x < 0 || x >= bw as i64 {
                continue;
            }
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let d2 = dx * dx + dy * dy;
            if d2 > r * r || d2 < r_in * r_in {
                continue;
            }
            if !full {
                let mut a = pixel_angle(cx, cy, x as f64 + 0.5, y as f64 + 0.5);
                a = ((a - start) % 360.0 + 360.0) % 360.0;
                if a >= span {
                    continue;
                }
            }
            let o = (y as u32 * bw + x as u32) as usize * 3;
            buf[o] = color.0;
            buf[o + 1] = color.1;
            buf[o + 2] = color.2;
        }
    }
}

/// Outline the pie/donut ring.
fn paint_ring(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    cx: f64,
    cy: f64,
    r: f64,
    donut: bool,
    color: (u8, u8, u8),
) {
    let xa = (cx - r - 2.0).floor() as i64;
    let xb = (cx + r + 2.0).ceil() as i64;
    let ya = (cy - r - 2.0).floor() as i64;
    let yb = (cy + r + 2.0).ceil() as i64;
    let r_in = if donut { r * 0.55 } else { 0.0 };
    for y in ya..=yb {
        if y < 0 || y >= bh as i64 {
            continue;
        }
        for x in xa..=xb {
            if x < 0 || x >= bw as i64 {
                continue;
            }
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let on_outer = (d - r).abs() <= 0.75;
            let on_inner = donut && (d - r_in).abs() <= 0.75;
            if on_outer || on_inner {
                let o = (y as u32 * bw + x as u32) as usize * 3;
                buf[o] = color.0;
                buf[o + 1] = color.1;
                buf[o + 2] = color.2;
            }
        }
    }
}

/// Paint radar charts: one polygon per series over the category
/// spokes, with ring gridlines and spoke labels.
fn paint_radar(buf: &mut [u8], pw: u32, ph: u32, face: &Face, ch: &ChartSpec, body_y: f64) {
    let legend_h = legend_height(ch);
    let avail_h = (ph as f64 - body_y - legend_h - 20.0).max(8.0);
    let avail_w = (pw as f64 - 20.0).max(8.0);
    let r = (avail_w.min(avail_h) / 2.0 - 14.0).max(4.0);
    let cx = pw as f64 / 2.0;
    let cy = body_y + avail_h / 2.0;
    let cats = categories(&ch.data);
    let series = group_series(&ch.data);
    let n = cats.len().max(1) as f64;
    let angle_of = |i: usize| 270.0 + i as f64 * 360.0 / n;
    let vmax = ch
        .data
        .iter()
        .map(|p| p.value)
        .fold(0.0f64, f64::max)
        .max(f64::EPSILON);
    // Ring gridlines at quarters with the max label on the outer ring.
    for q in 1..=4 {
        let rr = r * q as f64 / 4.0;
        let mut ring: Vec<(f64, f64)> = Vec::new();
        for i in 0..cats.len().max(1) {
            ring.push(polar(cx, cy, rr, angle_of(i)));
        }
        if ring.len() >= 2 {
            for w in 0..ring.len() {
                let b = ring[(w + 1) % ring.len()];
                seg(buf, pw, ph, ring[w].0, ring[w].1, b.0, b.1, 1.0, GRID);
            }
        } else if ring.len() == 1 {
            disc(buf, pw, ph, ring[0].0, ring[0].1, 1.0, GRID);
        }
        if ch.show_gridlines && q == 4 {
            let label = fmt_val(vmax);
            draw_text(
                buf,
                pw,
                ph,
                face,
                &label,
                LABEL_SIZE,
                cx + 3.0,
                cy - r - 2.0,
                AXIS,
            );
        }
    }
    // Spokes plus spoke labels.
    for (i, c) in cats.iter().enumerate() {
        let (ex, ey) = polar(cx, cy, r, angle_of(i));
        seg(buf, pw, ph, cx, cy, ex, ey, 1.0, AXIS);
        let (lx, ly) = polar(cx, cy, r + 6.0, angle_of(i));
        let cos = (angle_of(i).to_radians()).cos();
        if cos > 0.3 {
            draw_text(buf, pw, ph, face, c, LABEL_SIZE, lx, ly - 5.0, INK);
        } else if cos < -0.3 {
            draw_text_right(buf, pw, ph, face, c, LABEL_SIZE, lx, ly - 11.0, INK);
        } else {
            draw_text_centered(buf, pw, ph, face, c, LABEL_SIZE, lx, ly - 11.0);
        }
    }
    if ch.data.is_empty() {
        return;
    }
    let pal = palette_of(ch);
    let bg = (ch.background.r, ch.background.g, ch.background.b);
    for (si, s) in series.iter().enumerate() {
        let color = pal[si % pal.len()];
        let mut poly: Vec<(f64, f64)> = Vec::new();
        for p in &s.points {
            let ci = cats
                .iter()
                .position(|c| *c == p.1.label.as_str())
                .unwrap_or(0);
            let rr = (p.1.value / vmax).clamp(0.0, 1.0) * r;
            poly.push(polar(cx, cy, rr, angle_of(ci)));
        }
        if poly.len() >= 3 {
            poly_fill(buf, pw, ph, &poly, mix(color, bg, 0.65));
        }
        if poly.len() >= 2 {
            for w in 0..poly.len() {
                let b = poly[(w + 1) % poly.len()];
                seg(buf, pw, ph, poly[w].0, poly[w].1, b.0, b.1, 2.0, color);
            }
        } else {
            for pt in &poly {
                disc(buf, pw, ph, pt.0, pt.1, 3.0, color);
            }
        }
        for (k, pt) in poly.iter().enumerate() {
            disc(buf, pw, ph, pt.0, pt.1, 3.0, color);
            if ch.show_value_labels {
                let label = fmt_val(s.points[k].1.value);
                draw_text_centered(buf, pw, ph, face, &label, LABEL_SIZE, pt.0, pt.1 - 16.0);
            }
        }
    }
}

/// Paint gauge charts: one value on a semicircular track with
/// min/mid/max ticks, a needle, and a centered readout.
fn paint_gauge(buf: &mut [u8], pw: u32, ph: u32, face: &Face, ch: &ChartSpec, body_y: f64) {
    let legend_h = legend_height(ch);
    let avail_h = (ph as f64 - body_y - legend_h - 34.0).max(8.0);
    let avail_w = (pw as f64 - 20.0).max(8.0);
    let r = (avail_w / 2.0 - 8.0).min(avail_h - 8.0).max(4.0);
    let cx = pw as f64 / 2.0;
    let cy = body_y + avail_h;
    let pal = palette_of(ch);
    let color = pal[0 % pal.len()];
    if ch.data.is_empty() {
        paint_arc(buf, pw, ph, cx, cy, r, 180.0, 360.0, 10.0, TRACK);
        return;
    }
    let v = ch.data[0].value;
    let lo = ch.min.unwrap_or(0.0);
    let hi = ch.max.unwrap_or(if v > 0.0 { v } else { 1.0 });
    let span = (hi - lo).max(f64::EPSILON);
    let t = ((v - lo) / span).clamp(0.0, 1.0);
    let vang = 180.0 + t * 180.0;
    paint_arc(buf, pw, ph, cx, cy, r, 180.0, 360.0, 10.0, TRACK);
    paint_arc(buf, pw, ph, cx, cy, r, 180.0, vang, 10.0, color);
    if ch.show_gridlines {
        for tv in [lo, (lo + hi) / 2.0, hi] {
            let tt = ((tv - lo) / span).clamp(0.0, 1.0);
            let a = 180.0 + tt * 180.0;
            let (x1, y1) = polar(cx, cy, r - 8.0, a);
            let (x2, y2) = polar(cx, cy, r + 8.0, a);
            seg(buf, pw, ph, x1, y1, x2, y2, 1.0, AXIS);
            let (lx, ly) = polar(cx, cy, r + 12.0, a);
            draw_text_centered_clamped(buf, pw, ph, face, &fmt_val(tv), LABEL_SIZE, lx, ly - 6.0);
        }
    }
    let (nx, ny) = polar(cx, cy, r - 14.0, vang);
    seg(buf, pw, ph, cx, cy, nx, ny, 2.0, INK);
    disc(buf, pw, ph, cx, cy, 4.0, INK);
    if ch.show_value_labels {
        draw_text_centered(buf, pw, ph, face, &fmt_val(v), TITLE_SIZE, cx, cy + 8.0);
    }
}

/// Stroked arc: pixels whose radius sits within `w` of `r` and whose
/// angle sits in `[a0, a1]` (math degrees, y down).
fn paint_arc(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    cx: f64,
    cy: f64,
    r: f64,
    a0: f64,
    a1: f64,
    w: f64,
    color: (u8, u8, u8),
) {
    if a1 <= a0 || r <= 0.0 || w <= 0.0 {
        return;
    }
    let xa = (cx - r - w).floor() as i64;
    let xb = (cx + r + w).ceil() as i64;
    let ya = (cy - r - w).floor() as i64;
    let yb = (cy + r + w).ceil() as i64;
    let span = a1 - a0;
    let start = ((a0 % 360.0) + 360.0) % 360.0;
    for y in ya..=yb {
        if y < 0 || y >= bh as i64 {
            continue;
        }
        for x in xa..=xb {
            if x < 0 || x >= bw as i64 {
                continue;
            }
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if (d - r).abs() > w / 2.0 {
                continue;
            }
            let mut a = pixel_angle(cx, cy, x as f64 + 0.5, y as f64 + 0.5);
            a = ((a - start) % 360.0 + 360.0) % 360.0;
            if a > span {
                continue;
            }
            let o = (y as u32 * bw + x as u32) as usize * 3;
            buf[o] = color.0;
            buf[o + 1] = color.1;
            buf[o + 2] = color.2;
        }
    }
}

/// Legend rows in draw order: `(color index, text)`. Cartesian and
/// radar charts name their series (a lone unnamed series needs no
/// legend); pies and donuts name their slices with shares; gauges
/// and empty charts name nothing.
fn legend_entries(ch: &ChartSpec) -> Vec<(usize, String)> {
    match ch.kind {
        ChartKind::Pie | ChartKind::Donut => {
            let total: f64 = ch.data.iter().map(|p| p.value.max(0.0)).sum();
            ch.data
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let share = if total > 0.0 {
                        p.value.max(0.0) / total * 100.0
                    } else {
                        0.0
                    };
                    (i, format!("{} ({})", p.label, fmt_share(share)))
                })
                .collect()
        }
        ChartKind::Gauge => Vec::new(),
        _ => {
            let series = group_series(&ch.data);
            if series.len() <= 1 && series.first().is_none_or(|s| s.name.is_empty()) {
                return Vec::new();
            }
            series
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let name = if s.name.is_empty() {
                        format!("series {}", i + 1)
                    } else {
                        s.name.clone()
                    };
                    (i, name)
                })
                .collect()
        }
    }
}

/// Format one pie share: one decimal, trimmed (`42.5%`, `50%`).
fn fmt_share(v: f64) -> String {
    let mut s = format!("{v:.1}");
    if s.ends_with('0') {
        s.pop();
        s.pop();
    }
    format!("{s}%")
}

/// Paint the bottom legend strip: a color chip plus the entry name
/// per row, centered as one row of chips.
fn paint_legend(buf: &mut [u8], pw: u32, ph: u32, face: &Face, ch: &ChartSpec) {
    if !ch.show_legend {
        return;
    }
    let entries = legend_entries(ch);
    if entries.is_empty() {
        return;
    }
    let pal = palette_of(ch);
    let widths: Vec<f64> = entries
        .iter()
        .map(|(_, name)| 14.0 + text_width(face, name, LABEL_SIZE))
        .collect();
    let gap = 12.0;
    let total: f64 = widths.iter().sum::<f64>() + gap * (entries.len().saturating_sub(1) as f64);
    let mut x = (pw as f64 - total) / 2.0;
    let y = ph as f64 - 16.0;
    for (k, (ci, name)) in entries.iter().enumerate() {
        let color = pal[ci % pal.len()];
        fill_rect(buf, pw, ph, x, y, x + 10.0, y + 8.0, color);
        draw_text(buf, pw, ph, face, name, LABEL_SIZE, x + 14.0, y + 8.0, INK);
        x += widths[k] + gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{ChartClip, Rgb, TrackKind, Turn3D};

    fn point(series: Option<&str>, label: &str, value: f64) -> ChartPoint {
        ChartPoint {
            series: series.map(str::to_string),
            label: label.to_string(),
            value,
            x: None,
        }
    }

    fn clip(kind: ChartKind, data: Vec<ChartPoint>) -> ChartClip {
        ChartClip {
            asserts: Default::default(),
            kind,
            title: Some("test".to_string()),
            data,
            data_file: None,
            start: 0.0,
            duration: 1.0,
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            x: 0.0,
            y: 0.0,
            width: Some(320.0),
            height: Some(240.0),
            background: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            border_width: 0.0,
            border_color: Rgb { r: 0, g: 0, b: 0 },
            palette: vec![
                Rgb {
                    r: 78,
                    g: 121,
                    b: 167,
                },
                Rgb {
                    r: 242,
                    g: 142,
                    b: 30,
                },
            ],
            show_legend: true,
            show_gridlines: true,
            show_value_labels: true,
            min: None,
            max: None,
            order: 0,
            turn: Turn3D::flat(),
            filter_blur: 0.0,
            grade: Default::default(),
            effects: Vec::new(),
            vfx: Default::default(),
            hidden: false,
            line: 1,
            column: 1,
        }
    }

    fn sample_data(kind: ChartKind) -> Vec<ChartPoint> {
        match kind {
            ChartKind::Pie | ChartKind::Donut => {
                vec![point(None, "a", 30.0), point(None, "b", 70.0)]
            }
            ChartKind::Gauge => vec![point(None, "load", 62.0)],
            _ => vec![
                point(None, "Q1", 12.0),
                point(Some("s"), "Q2", 19.0),
                point(None, "Q3", 7.0),
            ],
        }
    }

    /// Non-background pixel count of one raster.
    fn inked(rgba: &[u8], bg: (u8, u8, u8)) -> usize {
        rgba.chunks_exact(4)
            .filter(|px| px[0] != bg.0 || px[1] != bg.1 || px[2] != bg.2)
            .count()
    }

    /// Every kind paints its axes and labels: the raster carries ink
    /// beyond the panel, and reruns over the same inputs are
    /// byte-identical.
    #[test]
    fn every_kind_renders_deterministically_with_ink() {
        for kind in [
            ChartKind::Bar,
            ChartKind::Line,
            ChartKind::Area,
            ChartKind::Pie,
            ChartKind::Donut,
            ChartKind::Scatter,
            ChartKind::Radar,
            ChartKind::Gauge,
        ] {
            let c = clip(kind, sample_data(kind));
            let (w, h, first) = render_viewport(&c, 320, 240);
            assert_eq!((w, h), (320, 240));
            let (_, _, second) = render_viewport(&c, 320, 240);
            assert_eq!(first, second, "{kind:?} reruns byte-identically");
            let n = inked(&first, (255, 255, 255));
            assert!(
                n > 200,
                "{kind:?} paints its axes and labels (inked {n} pixels)"
            );
        }
    }

    /// Empty series render named empty axes: the titled panel with
    /// frame lines but no data ink beyond the title — deterministic.
    #[test]
    fn empty_series_renders_named_empty_axes() {
        for kind in [
            ChartKind::Bar,
            ChartKind::Line,
            ChartKind::Area,
            ChartKind::Pie,
            ChartKind::Donut,
            ChartKind::Scatter,
            ChartKind::Radar,
            ChartKind::Gauge,
        ] {
            let c = clip(kind, Vec::new());
            let (_, _, first) = render_viewport(&c, 320, 240);
            let (_, _, second) = render_viewport(&c, 320, 240);
            assert_eq!(first, second, "empty {kind:?} reruns byte-identically");
            // The title still names the panel.
            let n = inked(&first, (255, 255, 255));
            assert!(n > 20, "empty {kind:?} names its axes (inked {n} pixels)");
            // Empty and filled rasters differ: no invented curves.
            let full = clip(kind, sample_data(kind));
            let (_, _, filled) = render_viewport(&full, 320, 240);
            assert_ne!(first, filled, "empty {kind:?} paints no data");
        }
    }

    /// The canonical key is stable per inputs and sensitive to every
    /// pixel-affecting input.
    #[test]
    fn viewport_key_is_stable_and_sensitive() {
        let c = clip(ChartKind::Bar, sample_data(ChartKind::Bar));
        assert_eq!(viewport_key(&c, 320, 240), viewport_key(&c, 320, 240));
        // Geometry and size feed the key.
        assert_ne!(viewport_key(&c, 320, 240), viewport_key(&c, 160, 240));
        // Data feeds the key.
        let mut other = clip(ChartKind::Bar, sample_data(ChartKind::Bar));
        other.data[0].value = 13.0;
        assert_ne!(viewport_key(&c, 320, 240), viewport_key(&other, 320, 240));
        // Kind feeds the key.
        let line = clip(ChartKind::Line, sample_data(ChartKind::Line));
        assert_ne!(viewport_key(&c, 320, 240), viewport_key(&line, 320, 240));
        // Position and order do not: the baked raster is shared.
        let mut moved = clip(ChartKind::Bar, sample_data(ChartKind::Bar));
        moved.x = 50.0;
        moved.order = 3;
        assert_eq!(viewport_key(&c, 320, 240), viewport_key(&moved, 320, 240));
    }

    /// Authored extents win; omitted extents fall back to the canvas.
    #[test]
    fn viewport_px_falls_back_to_canvas() {
        let c = clip(ChartKind::Bar, sample_data(ChartKind::Bar));
        assert_eq!(viewport_px(&c, (640, 480)), (320, 240));
        let mut bare = clip(ChartKind::Bar, sample_data(ChartKind::Bar));
        bare.width = None;
        bare.height = None;
        assert_eq!(viewport_px(&bare, (640, 480)), (640, 480));
    }

    /// Ticks cover the range on a 1/2/2.5/5/10 step; degenerate
    /// ranges widen instead of vanishing.
    #[test]
    fn nice_ticks_cover_the_range() {
        let ticks = nice_ticks(12.0, 19.0, 4);
        assert!(ticks.first().copied().unwrap_or(99.0) <= 12.0);
        assert!(ticks.last().copied().unwrap_or(-99.0) >= 19.0);
        assert!(ticks.len() <= 7, "at most a step past each end: {ticks:?}");
        // Degenerate input still draws an axis.
        let flat = nice_ticks(5.0, 5.0, 4);
        assert!(flat.first().copied().unwrap_or(99.0) < 5.0);
        assert!(flat.last().copied().unwrap_or(-99.0) > 5.0);
    }

    /// Value labels print compactly and deterministically.
    #[test]
    fn values_format_compactly() {
        assert_eq!(fmt_val(12.0), "12");
        assert_eq!(fmt_val(-3.0), "-3");
        assert_eq!(fmt_val(0.5), "0.5");
        assert_eq!(fmt_val(2.25), "2.25");
        assert_eq!(fmt_val(0.0), "0");
    }

    /// Series group in first-appearance order for legend and paint.
    #[test]
    fn series_group_in_first_appearance_order() {
        let data = vec![
            point(Some("b"), "Q1", 1.0),
            point(Some("a"), "Q1", 2.0),
            point(Some("b"), "Q2", 3.0),
            point(None, "Q3", 4.0),
        ];
        let groups = group_series(&data);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].name, "b");
        assert_eq!(groups[0].points.len(), 2);
        assert_eq!(groups[1].name, "a");
        assert_eq!(groups[2].name, "");
    }

    /// Legends name series (cartesian/radar), slices with shares
    /// (pies), and nothing for gauges or lone unnamed series.
    #[test]
    fn legend_entries_name_what_they_draw() {
        let bar = clip(ChartKind::Bar, sample_data(ChartKind::Bar));
        let entries = legend_entries(&spec_of(&bar));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1, "series 1");
        assert_eq!(entries[1].1, "s");
        let pie = clip(ChartKind::Pie, sample_data(ChartKind::Pie));
        let entries = legend_entries(&spec_of(&pie));
        assert_eq!(entries.len(), 2);
        assert!(entries[0].1.contains("30%"), "{}", entries[0].1);
        let gauge = clip(ChartKind::Gauge, sample_data(ChartKind::Gauge));
        assert!(legend_entries(&spec_of(&gauge)).is_empty());
        let lone = clip(ChartKind::Line, vec![point(None, "Q1", 1.0)]);
        assert!(legend_entries(&spec_of(&lone)).is_empty());
    }

    fn spec_of(c: &ChartClip) -> ChartSpec<'_> {
        ChartSpec {
            kind: c.kind,
            title: c.title.clone().unwrap_or_default(),
            data: &c.data,
            background: c.background,
            border_width: c.border_width,
            border_color: c.border_color,
            palette: &c.palette,
            show_legend: c.show_legend,
            show_gridlines: c.show_gridlines,
            show_value_labels: c.show_value_labels,
            min: c.min,
            max: c.max,
        }
    }
}
