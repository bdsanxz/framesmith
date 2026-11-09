//! Canvas view for the Studio page (Sprint 17, T001 / CAP-001, J-01,
//! STATE-01, REC-003).
//!
//! The canvas panel shows the film with zoom from 25 to 400 percent
//! anchored at the cursor or the centre, free pan, fit framing the
//! film, fullscreen, rulers, a minimap, and 90 and 70 percent safe
//! margins on demand. A missing film shows the empty canvas with the
//! reason and transport disabled.
//!
//! The canvas view is view state only: it persists per project in the
//! hidden history directory next to the layout file — never beside
//! the sources and never outside the project — and it stages no
//! composition patch and records no history turn, so default renders
//! (no SDK delta) replay to themselves byte-identically. Canvas and
//! timeline selections stay in sync: both views resolve the same
//! stable `id` and `#index` keys, and the canvas echoes the synced
//! selection with every state answer.
//!
//! Request bodies use hyphenated keys (`zoom-percent`, `pan-x`,
//! `show-safe-90`, `anchor-at`); the canvas carries no style
//! attributes of its own, so the CSS-like hyphenated vocabulary and
//! the fail-closed `font-family` comma try-order pass through
//! untouched — authored family lists ride along in written order and
//! unknown families stay fail-closed at render time, never resolved
//! or substituted here.

use std::path::{Path, PathBuf};

/// File name of the persisted canvas view inside the hidden project
/// directory (next to the layout file, never beside the sources).
pub const STUDIO_CANVAS_FILE: &str = "studio-canvas.json";

/// Version of the persisted canvas view.
pub const STUDIO_CANVAS_VERSION: u32 = 1;

/// The canvas zoom floor as a percent of fit: zoom never shows below
/// 25 percent, it clamps and shows the clamped value.
pub const MIN_CANVAS_ZOOM_PERCENT: f64 = 25.0;

/// The canvas zoom ceiling as a percent of fit: zoom never shows above
/// 400 percent, it clamps and shows the clamped value.
pub const MAX_CANVAS_ZOOM_PERCENT: f64 = 400.0;

/// Safe-margin fractions the canvas overlays on demand: the centred 90
/// and 70 percent action-safe rectangles of the film.
pub const SAFE_MARGIN_90: f64 = 0.90;
pub const SAFE_MARGIN_70: f64 = 0.70;

/// Hidden canvas-view path for one project folder.
pub fn canvas_path_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(STUDIO_CANVAS_FILE)
}

/// Prefix every canvas refusal carries; nothing saves or applies on a
/// refusal.
fn refused_canvas(message: String) -> String {
    format!("studio refused: {message}")
}

/// One canvas zoom anchor (Sprint 17, T001): the film point that stays
/// put while the scale changes — under the cursor, or the film
/// centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasAnchor {
    /// The cursor point stays put.
    Cursor,
    /// The film centre stays put.
    Center,
}

impl CanvasAnchor {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CanvasAnchor::Cursor => "cursor",
            CanvasAnchor::Center => "center",
        }
    }

    /// Parse one anchor spelling; anything else refuses naming the
    /// two. `centre` reads as `center` so the spec spelling works.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "cursor" => Some(CanvasAnchor::Cursor),
            "center" | "centre" => Some(CanvasAnchor::Center),
            _ => None,
        }
    }
}

/// The persisted canvas view (Sprint 17, T001): zoom, anchor, pan, and
/// the on-demand overlays. Pure view state: applying it never touches
/// composition sources.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasView {
    /// Zoom as a percent of fit (25 to 400; out-of-bounds requests
    /// clamp and show the clamped value).
    pub zoom_percent: f64,
    /// The anchor the scale holds.
    pub anchor: CanvasAnchor,
    /// Free pan offset in CSS pixels (positive moves the film right).
    pub pan_x: f64,
    /// Free pan offset in CSS pixels (positive moves the film down).
    pub pan_y: f64,
    /// True while the canvas panel fills the screen.
    pub fullscreen: bool,
    /// True while rulers overlay the film edges.
    pub show_rulers: bool,
    /// True while the minimap shows the whole film.
    pub show_minimap: bool,
    /// True while the centred 90 percent safe margin overlays.
    pub show_safe_90: bool,
    /// True while the centred 70 percent safe margin overlays.
    pub show_safe_70: bool,
}

/// The default canvas view: the film at fit, centred, unpanned, with
/// every overlay off.
pub fn default_canvas_view() -> CanvasView {
    CanvasView {
        zoom_percent: 100.0,
        anchor: CanvasAnchor::Center,
        pan_x: 0.0,
        pan_y: 0.0,
        fullscreen: false,
        show_rulers: false,
        show_minimap: false,
        show_safe_90: false,
        show_safe_70: false,
    }
}

/// What loading the canvas view reported: the view to paint, whether
/// it came from the project file, and the fallback banner when it did
/// not.
#[derive(Debug, Clone)]
pub struct CanvasLoad {
    /// The view to paint (the default with no file, or on an
    /// unrestorable file).
    pub view: CanvasView,
    /// True when the view came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file cannot restore.
    pub notice: Option<String>,
}

/// Status line the panel shows for one canvas view: the zoom with its
/// anchor, the pan, the overlays on demand, and fullscreen.
pub fn canvas_status_line(view: &CanvasView) -> String {
    let mut overlays = Vec::new();
    if view.show_rulers {
        overlays.push("rulers");
    }
    if view.show_minimap {
        overlays.push("minimap");
    }
    if view.show_safe_90 {
        overlays.push("safe-90");
    }
    if view.show_safe_70 {
        overlays.push("safe-70");
    }
    let overlays = if overlays.is_empty() {
        "overlays off".to_string()
    } else {
        format!("overlays: {}", overlays.join(", "))
    };
    format!(
        "canvas view: zoom {:.0}% anchored at {}, pan ({:.0}, {:.0}) px; {}; fullscreen {}.",
        view.zoom_percent,
        view.anchor.as_str(),
        view.pan_x,
        view.pan_y,
        overlays,
        if view.fullscreen { "on" } else { "off" }
    )
}

/// Clamp one requested zoom percent into the 25 to 400 band: returns
/// the shown percent with whether a bound caught it.
pub fn clamp_canvas_zoom(want: f64) -> (f64, bool) {
    if want < MIN_CANVAS_ZOOM_PERCENT {
        (MIN_CANVAS_ZOOM_PERCENT, true)
    } else if want > MAX_CANVAS_ZOOM_PERCENT {
        (MAX_CANVAS_ZOOM_PERCENT, true)
    } else {
        (want, false)
    }
}

/// One-line summary of one applied canvas update: the clamped value
/// and bound while a bound caught the request, else the plain view
/// line.
pub fn canvas_detail(view: &CanvasView, clamped: bool, want: f64) -> String {
    if clamped {
        let bound = if want > MAX_CANVAS_ZOOM_PERCENT {
            "400% ceiling"
        } else {
            "25% floor"
        };
        format!(
            "canvas zoom {want:.0}% ran past the {bound}, clamped to {:.0}%: {}.",
            view.zoom_percent,
            canvas_status_line(view)
        )
    } else {
        canvas_status_line(view)
    }
}

/// The outcome of one applied canvas update: the saved view, whether
/// a zoom bound clamped it, and the line the panel shows.
#[derive(Debug, Clone)]
pub struct CanvasOutcome {
    /// The saved view (the clamped zoom when clamped).
    pub view: CanvasView,
    /// True while a zoom bound clamped the request.
    pub clamped: bool,
    /// The zoom percent the request named (the clamped value when
    /// clamped).
    pub zoom_percent: f64,
    /// One-line summary naming the scale and any clamp.
    pub detail: String,
}

/// One parsed canvas update (Sprint 17, T001): every field rides
/// along only when the body names it, so a view-only move (pan
/// without zoom, one overlay toggle) never rewrites the rest.
/// Structural parsing only; the zoom clamp runs at apply time so the
/// saved view always shows what the panel paints.
#[derive(Debug, Clone, Default)]
pub struct CanvasUpdate {
    /// Requested zoom percent of fit, when the body names
    /// `zoom-percent`.
    pub zoom_percent: Option<f64>,
    /// Requested anchor, when the body names `anchor`.
    pub anchor: Option<CanvasAnchor>,
    /// Requested pan-x in CSS pixels, when the body names `pan-x`.
    pub pan_x: Option<f64>,
    /// Requested pan-y in CSS pixels, when the body names `pan-y`.
    pub pan_y: Option<f64>,
    /// True while the body asks for fit (`fit: true` frames the film
    /// at 100 percent centred with no pan).
    pub fit: bool,
    /// Requested fullscreen switch, when the body names `fullscreen`.
    pub fullscreen: Option<bool>,
    /// Requested rulers switch, when the body names `show-rulers`.
    pub show_rulers: Option<bool>,
    /// Requested minimap switch, when the body names `show-minimap`.
    pub show_minimap: Option<bool>,
    /// Requested 90 percent safe-margin switch.
    pub show_safe_90: Option<bool>,
    /// Requested 70 percent safe-margin switch.
    pub show_safe_70: Option<bool>,
    /// Selection entries the caller echoes, validated against the
    /// timeline keys at the route (unknown entries drop with a note,
    /// never refuse).
    pub selection: Option<Vec<String>>,
    /// Composition the view belongs to, when named.
    pub composition: Option<String>,
}

/// Read one request number: present, numeric, and finite. Missing
/// returns `None` (the caller keeps the current value); anything else
/// refuses naming the key.
fn canvas_number(value: &serde_json::Value, key: &str) -> Result<Option<f64>, String> {
    let raw = match value.get(key) {
        None => return Ok(None),
        Some(v) => v,
    };
    let n = raw.as_f64().ok_or_else(|| {
        refused_canvas(format!(
            "the studio canvas `{key}` value {raw} is not a number; pass a finite number. Nothing was saved."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_canvas(format!(
            "the studio canvas `{key}` value {n:?} is not finite; pass a finite number. Nothing was saved."
        )));
    }
    Ok(Some(n))
}

/// Read one request switch: present and boolean. Missing returns
/// `None` (the caller keeps the current value); anything else refuses
/// naming the key.
fn canvas_switch(value: &serde_json::Value, key: &str) -> Result<Option<bool>, String> {
    match value.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| {
                refused_canvas(format!(
                    "the studio canvas `{key}` value {v} is not a switch; pass `true` or `false`. Nothing was saved."
                ))
            }),
    }
}

/// Parse one canvas body (`{"zoom-percent":150,"anchor":"cursor",
/// "pan-x":10,"show-rulers":true}`): at least one known key must ride
/// along, so a stray body never rewrites the visible view by
/// accident. Unknown keys are ignored so a host typo never enables a
/// behavior. A refused body names its cause and saves nothing.
pub fn parse_canvas_body(body: &str, _current: &CanvasView) -> Result<CanvasUpdate, String> {
    if body.trim().is_empty() {
        return Err(refused_canvas(
            "the studio canvas needs a JSON canvas body (`{\"zoom-percent\":150,\"anchor\":\"cursor\"}`); pass the view from the canvas panel. Nothing was saved."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_canvas(format!(
            "the studio canvas body is not JSON ({e}). Pass `{{\"zoom-percent\":150}}` or another canvas view. Nothing was saved."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_canvas(
            "the studio canvas body is not a canvas view; pass a JSON object with canvas keys. Nothing was saved."
                .to_string(),
        ));
    }
    let zoom_percent = canvas_number(&value, "zoom-percent")?;
    let anchor = match value.get("anchor") {
        None => None,
        Some(v) => {
            let raw = v.as_str().ok_or_else(|| {
                refused_canvas(format!(
                    "the studio canvas `anchor` value {v} is not an anchor; pass `cursor` or `center`. Nothing was saved."
                ))
            })?;
            Some(CanvasAnchor::parse(raw).ok_or_else(|| {
                refused_canvas(format!(
                    "unknown studio canvas anchor {raw:?}: the canvas anchors at `cursor` or `center`. Pass one of those anchors. Nothing was saved."
                ))
            })?)
        }
    };
    let pan_x = canvas_number(&value, "pan-x")?;
    let pan_y = canvas_number(&value, "pan-y")?;
    let fit = match value.get("fit") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| {
            refused_canvas(format!(
                "the studio canvas `fit` value {v} is not a switch; pass `true` to frame the film. Nothing was saved."
            ))
        })?,
    };
    let fullscreen = canvas_switch(&value, "fullscreen")?;
    let show_rulers = canvas_switch(&value, "show-rulers")?;
    let show_minimap = canvas_switch(&value, "show-minimap")?;
    let show_safe_90 = canvas_switch(&value, "show-safe-90")?;
    let show_safe_70 = canvas_switch(&value, "show-safe-70")?;
    let selection = match value.get("selection") {
        None => None,
        Some(v) => {
            let list = v.as_array().ok_or_else(|| {
                refused_canvas(format!(
                    "the studio canvas `selection` value {v} is not a clip list; pass declared `id` values or `#index` entries. Nothing was saved."
                ))
            })?;
            let mut out = Vec::new();
            for entry in list {
                let name = entry.as_str().ok_or_else(|| {
                    refused_canvas(format!(
                        "the studio canvas `selection` entry {entry} is not a clip reference; pass a declared `id` or a `#index` entry. Nothing was saved."
                    ))
                })?;
                if name.trim().is_empty() {
                    return Err(refused_canvas(
                        "the studio canvas `selection` names an empty clip; pass a declared `id` or a `#index` entry. Nothing was saved."
                            .to_string(),
                    ));
                }
                out.push(name.to_string());
            }
            Some(out)
        }
    };
    let composition = match value.get("composition") {
        None => None,
        Some(v) => {
            let name = v.as_str().ok_or_else(|| {
                refused_canvas(format!(
                    "the studio canvas `composition` value {v} is not a composition name; pass the composition the view belongs to. Nothing was saved."
                ))
            })?;
            if name.trim().is_empty() {
                return Err(refused_canvas(
                    "the studio canvas `composition` names an empty composition; pass the composition the view belongs to. Nothing was saved."
                        .to_string(),
                ));
            }
            Some(name.to_string())
        }
    };
    let known = zoom_percent.is_some()
        || anchor.is_some()
        || pan_x.is_some()
        || pan_y.is_some()
        || fit
        || fullscreen.is_some()
        || show_rulers.is_some()
        || show_minimap.is_some()
        || show_safe_90.is_some()
        || show_safe_70.is_some()
        || selection.is_some();
    if !known {
        return Err(refused_canvas(
            "the studio canvas names no view key (`zoom-percent`, `anchor`, `pan-x`, `pan-y`, `fit`, `fullscreen`, `show-rulers`, `show-minimap`, `show-safe-90`, `show-safe-70`, `selection`); pass at least one. Nothing was saved."
                .to_string(),
        ));
    }
    Ok(CanvasUpdate {
        zoom_percent,
        anchor,
        pan_x,
        pan_y,
        fit,
        fullscreen,
        show_rulers,
        show_minimap,
        show_safe_90,
        show_safe_70,
        selection,
        composition,
    })
}

/// Apply one parsed canvas update to the current view (Sprint 17,
/// T001): fit frames the film at 100 percent centred with no pan and
/// wins over a same-body zoom; an out-of-bounds zoom clamps and shows
/// the clamped value; pan moves freely with no bounds. Pure view
/// math: no patch stages, no history records, sources stay untouched.
pub fn apply_canvas_update(
    current: &CanvasView,
    update: &CanvasUpdate,
) -> (CanvasView, CanvasOutcome) {
    if update.fit {
        let view = CanvasView {
            zoom_percent: 100.0,
            anchor: CanvasAnchor::Center,
            pan_x: 0.0,
            pan_y: 0.0,
            fullscreen: update.fullscreen.unwrap_or(current.fullscreen),
            show_rulers: update.show_rulers.unwrap_or(current.show_rulers),
            show_minimap: update.show_minimap.unwrap_or(current.show_minimap),
            show_safe_90: update.show_safe_90.unwrap_or(current.show_safe_90),
            show_safe_70: update.show_safe_70.unwrap_or(current.show_safe_70),
        };
        let detail = format!(
            "canvas fit: the film fills the view at 100% centred with no pan: {}.",
            canvas_status_line(&view)
        );
        let outcome = CanvasOutcome {
            view: view.clone(),
            clamped: false,
            zoom_percent: 100.0,
            detail,
        };
        return (view, outcome);
    }
    let want = update.zoom_percent.unwrap_or(current.zoom_percent);
    let (zoom_percent, clamped) = clamp_canvas_zoom(want);
    let view = CanvasView {
        zoom_percent,
        anchor: update.anchor.unwrap_or(current.anchor),
        pan_x: update.pan_x.unwrap_or(current.pan_x),
        pan_y: update.pan_y.unwrap_or(current.pan_y),
        fullscreen: update.fullscreen.unwrap_or(current.fullscreen),
        show_rulers: update.show_rulers.unwrap_or(current.show_rulers),
        show_minimap: update.show_minimap.unwrap_or(current.show_minimap),
        show_safe_90: update.show_safe_90.unwrap_or(current.show_safe_90),
        show_safe_70: update.show_safe_70.unwrap_or(current.show_safe_70),
    };
    let detail = canvas_detail(&view, clamped, want);
    let outcome = CanvasOutcome {
        view: view.clone(),
        clamped,
        zoom_percent,
        detail,
    };
    (view, outcome)
}

/// Persist one canvas view (Sprint 17, T001): writes only the hidden
/// canvas file — creating the hidden directory when missing — so a
/// refused body leaves the previous view intact and sources never
/// move.
fn save_canvas_view(folder: &Path, view: &CanvasView) -> Result<(), String> {
    let path = canvas_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_canvas(format!(
                "the hidden canvas directory {} cannot be prepared: {e}. Nothing was saved.",
                parent.display()
            ))
        })?;
    }
    let raw = serde_json::to_string_pretty(&serde_json::json!({
        "version": STUDIO_CANVAS_VERSION,
        "zoom-percent": view.zoom_percent,
        "anchor": view.anchor.as_str(),
        "pan-x": view.pan_x,
        "pan-y": view.pan_y,
        "fullscreen": view.fullscreen,
        "show-rulers": view.show_rulers,
        "show-minimap": view.show_minimap,
        "show-safe-90": view.show_safe_90,
        "show-safe-70": view.show_safe_70,
    }))
    .map_err(|e| {
        refused_canvas(format!(
            "the canvas view cannot be encoded: {e}. Nothing was saved."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_canvas(format!(
            "the canvas file {} cannot be written: {e}. Nothing was saved.",
            path.display()
        ))
    })?;
    Ok(())
}

/// Save one canvas update for the project (Sprint 17, T001): parses
/// and validates the body against the current view first, applies the
/// clamp, then writes the hidden canvas file — so a refused body
/// leaves the previous view intact and sources never move.
pub fn save_canvas(folder: &Path, body: &str) -> Result<(CanvasView, CanvasOutcome), String> {
    let current = load_canvas(folder).view;
    let update = parse_canvas_body(body, &current)?;
    let (view, outcome) = apply_canvas_update(&current, &update);
    save_canvas_view(folder, &view)?;
    Ok((view, outcome))
}

/// Load the per-project canvas view: a missing file paints the
/// default with no banner; a corrupt, version-wrong, or invalid file
/// paints the default and says so.
pub fn load_canvas(folder: &Path) -> CanvasLoad {
    let fallback = CanvasLoad {
        view: default_canvas_view(),
        restored: false,
        notice: None,
    };
    let path = canvas_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return fallback,
        Err(e) => {
            return CanvasLoad {
                view: default_canvas_view(),
                restored: false,
                notice: Some(format!(
                    "studio: the canvas view {} cannot be read ({e}); showing the film at fit instead.",
                    path.display()
                )),
            };
        }
        Ok(raw) => raw,
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Err(e) => {
            return CanvasLoad {
                view: default_canvas_view(),
                restored: false,
                notice: Some(format!(
                    "studio: the canvas view {} is not a canvas view ({e}); showing the film at fit instead.",
                    path.display()
                )),
            };
        }
        Ok(v) => v,
    };
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version as u32 != STUDIO_CANVAS_VERSION {
        return CanvasLoad {
            view: default_canvas_view(),
            restored: false,
            notice: Some(format!(
                "studio: the canvas view {} carries version {version}, this Studio reads version {STUDIO_CANVAS_VERSION}; showing the film at fit instead.",
                path.display()
            )),
        };
    }
    let zoom_percent = value
        .get("zoom-percent")
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite());
    let anchor = value
        .get("anchor")
        .and_then(|v| v.as_str())
        .and_then(CanvasAnchor::parse);
    let pan_x = value
        .get("pan-x")
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite());
    let pan_y = value
        .get("pan-y")
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite());
    let fullscreen = value.get("fullscreen").and_then(|v| v.as_bool());
    let show_rulers = value.get("show-rulers").and_then(|v| v.as_bool());
    let show_minimap = value.get("show-minimap").and_then(|v| v.as_bool());
    let show_safe_90 = value.get("show-safe-90").and_then(|v| v.as_bool());
    let show_safe_70 = value.get("show-safe-70").and_then(|v| v.as_bool());
    match (
        zoom_percent,
        anchor,
        pan_x,
        pan_y,
        fullscreen,
        show_rulers,
        show_minimap,
        show_safe_90,
        show_safe_70,
    ) {
        (
            Some(zoom_percent),
            Some(anchor),
            Some(pan_x),
            Some(pan_y),
            Some(fullscreen),
            Some(show_rulers),
            Some(show_minimap),
            Some(show_safe_90),
            Some(show_safe_70),
        ) => {
            let (zoom_percent, _) = clamp_canvas_zoom(zoom_percent);
            CanvasLoad {
                view: CanvasView {
                    zoom_percent,
                    anchor,
                    pan_x,
                    pan_y,
                    fullscreen,
                    show_rulers,
                    show_minimap,
                    show_safe_90,
                    show_safe_70,
                },
                restored: true,
                notice: None,
            }
        }
        _ => CanvasLoad {
            view: default_canvas_view(),
            restored: false,
            notice: Some(format!(
                "studio: the canvas view {} names no valid `zoom-percent`, `anchor`, `pan-x`, `pan-y`, and overlay switches; showing the film at fit instead.",
                path.display()
            )),
        },
    }
}

/// The film behind the canvas view (Sprint 17, T001): canvas size and
/// frame count from the serving session, plus the authored
/// `font-family` try-order lists in written comma order (pass-through
/// only — unknown families stay fail-closed at render time).
#[derive(Debug, Clone)]
pub struct CanvasFilm {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Output frame count.
    pub frame_count: u64,
    /// Authored `font-family` lists, each in written comma order.
    pub font_families: Vec<String>,
}

/// Why the canvas shows the empty state instead of the film
/// (Sprint 17, T001 / REC-005): a missing or zero-size film names its
/// cause with the fix; transport stays disabled while empty. Returns
/// `None` while the film shows.
pub fn empty_canvas_reason(film: &CanvasFilm) -> Option<String> {
    if film.frame_count == 0 {
        return Some(
            "the canvas is empty: the timeline holds 0 frames; add composition time before arranging. Transport is disabled while the canvas is empty."
                .to_string(),
        );
    }
    if film.width == 0 || film.height == 0 {
        return Some(
            "the canvas is empty: the film has no canvas size; set a canvas width and height above 0. Transport is disabled while the canvas is empty."
                .to_string(),
        );
    }
    None
}

/// One centred safe-margin rectangle `[x, y, w, h]` in film pixels
/// for a margin fraction (0.90 or 0.70) of a `width` by `height` film.
pub fn safe_rect(width: u32, height: u32, fraction: f64) -> [f64; 4] {
    let w = width as f64;
    let h = height as f64;
    let sw = w * fraction;
    let sh = h * fraction;
    [(w - sw) / 2.0, (h - sh) / 2.0, sw, sh]
}

/// One nice ruler step in film pixels for a film dimension: the
/// 1/2/5 power of ten near a tenth of the dimension, so ruler ticks
/// stay readable at any film size.
pub fn ruler_step(dimension: u32) -> f64 {
    let target = (dimension as f64 / 10.0).max(1.0);
    let power = 10f64.powf(target.log10().floor());
    for multiple in [1.0, 2.0, 5.0, 10.0] {
        if power * multiple >= target {
            return power * multiple;
        }
    }
    power * 10.0
}

/// Sync one canvas selection against the timeline keys (Sprint 17,
/// T001 / INT-003): canvas and timeline share the same stable `id`
/// and `#index` spellings, so both views show the same selection at
/// once. Known entries stay in echo order; unknown entries drop with
/// a note, never refuse.
pub fn sync_canvas_selection(
    selection: &[String],
    rows: &[crate::studio::StudioRow],
) -> (Vec<String>, Vec<String>) {
    let known = crate::studio::selection_keys_of_rows(rows);
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for entry in selection {
        if known.iter().any(|k| k == entry) {
            if !kept.iter().any(|k| k == entry) {
                kept.push(entry.clone());
            }
        } else {
            dropped.push(entry.clone());
        }
    }
    (kept, dropped)
}

/// Authored `font-family` lists of one composition in document order
/// (Sprint 17, T001): every text clip list plus every span list, each
/// joined in written comma order with first-seen deduplication. The
/// canvas passes them through untouched — unknown families stay
/// fail-closed at render time, never resolved or substituted here.
pub fn canvas_font_lists_of(comp: &notation::model::Composition) -> Vec<String> {
    fn push(list: &mut Vec<String>, families: &Option<Vec<String>>) {
        if let Some(names) = families {
            let joined = names.join(", ");
            if !joined.trim().is_empty() && !list.iter().any(|seen| seen == &joined) {
                list.push(joined);
            }
        }
    }
    fn walk_span(list: &mut Vec<String>, span: &notation::model::TextSpan) {
        push(list, &span.font_family);
        for child in &span.children {
            walk_span(list, child);
        }
    }
    let mut out = Vec::new();
    for clip in &comp.clips {
        if let notation::model::Clip::Text(text) = clip {
            push(&mut out, &text.font_family);
            for span in &text.spans {
                walk_span(&mut out, span);
            }
        }
    }
    out
}

/// Render one canvas state as JSON for the canvas API: the persisted
/// view, the apply detail, the film with its empty state, the safe
/// margins, the synced selection, and the `font-family` pass-through.
/// Every key is hyphenated; every style the page paints from this
/// state uses CSS-like hyphenated vocabulary.
#[allow(clippy::too_many_arguments)]
pub fn canvas_state_json(
    load: &CanvasLoad,
    detail: &str,
    film: &CanvasFilm,
    minimap_frame: u64,
    kept: &[String],
    dropped: &[String],
) -> String {
    let view = &load.view;
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    let reason = match empty_canvas_reason(film) {
        Some(r) => format!("\"{}\"", crate::studio::json_escape(&r)),
        None => "null".to_string(),
    };
    let empty = empty_canvas_reason(film).is_some();
    let safe_90 = safe_rect(film.width, film.height, SAFE_MARGIN_90);
    let safe_70 = safe_rect(film.width, film.height, SAFE_MARGIN_70);
    let kept_json = kept
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let dropped_json = dropped
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let fonts_json = film
        .font_families
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"zoom-percent\":{:.0},\"anchor\":\"{}\",\"pan-x\":{:.0},\"pan-y\":{:.0},\"fullscreen\":{},\"show-rulers\":{},\"show-minimap\":{},\"show-safe-90\":{},\"show-safe-70\":{},\"restored\":{},\"notice\":{notice},\"status-line\":\"{}\",\"detail\":\"{}\",\"film\":{{\"width\":{},\"height\":{},\"frame-count\":{}}},\"safe-90\":[{:.1},{:.1},{:.1},{:.1}],\"safe-70\":[{:.1},{:.1},{:.1},{:.1}],\"ruler-step\":{:.0},\"minimap-frame\":{},\"empty\":{},\"reason\":{reason},\"transport-enabled\":{},\"selection\":{{\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]}},\"font-families\":[{fonts_json}]}}",
        view.zoom_percent,
        view.anchor.as_str(),
        view.pan_x,
        view.pan_y,
        view.fullscreen,
        view.show_rulers,
        view.show_minimap,
        view.show_safe_90,
        view.show_safe_70,
        load.restored,
        crate::studio::json_escape(&canvas_status_line(view)),
        crate::studio::json_escape(detail),
        film.width,
        film.height,
        film.frame_count,
        safe_90[0],
        safe_90[1],
        safe_90[2],
        safe_90[3],
        safe_70[0],
        safe_70[1],
        safe_70[2],
        safe_70[3],
        ruler_step(film.width.max(film.height)),
        minimap_frame,
        empty,
        !empty,
    )
}

/// Extra CSS the canvas view needs (Sprint 17, T001): the view bar,
/// the stage with its film frame, the ruler edges, the minimap, the
/// safe-margin frames, and the empty state. Style attributes stay
/// CSS-like hyphenated end to end (`background-color`,
/// `border-radius`, `object-fit`, `z-index`).
pub fn canvas_css() -> String {
    r#"        #canvas-view-bar{margin-top:8px}
        #canvas-view-bar label{margin-right:8px;font-variant-numeric:tabular-nums}
        #canvas-view-bar button{margin-right:4px}
        #canvas-status{font-variant-numeric:tabular-nums}
        #canvas-stage{position:relative;overflow:auto;max-height:60vh;background-color:#000;border-radius:4px}
        #canvas-frame-wrap{position:relative;display:inline-block;transform-origin:0 0}
        #canvas-frame-wrap img{display:block;max-width:none;background-color:#000;object-fit:contain}
        #canvas-ruler-x,#canvas-ruler-y{position:sticky;background-color:var(--panel-color);color:var(--ink-color);font-variant-numeric:tabular-nums;z-index:2}
        #canvas-safe-90-box,#canvas-safe-70-box{position:absolute;border:1px dashed var(--accent-color);border-radius:2px;pointer-events:none;z-index:3}
        #canvas-safe-70-box{border-style:solid}
        #canvas-minimap{margin-top:8px;display:flex;gap:8px;align-items:flex-start}
        #canvas-minimap img{width:160px;background-color:#000;object-fit:contain;border:1px solid var(--line-color);border-radius:4px}
        #canvas-empty{border:1px dashed var(--line-color);border-radius:4px;padding:8px 12px;margin-top:8px}"#.to_string()
}

/// The canvas view control block (Sprint 17, T001): zoom with its
/// cursor or centre anchor, free pan, fit, fullscreen, the rulers,
/// minimap, and safe-margin toggles, the status line, the stage with
/// its safe-margin frames, the minimap, and the empty state.
pub fn canvas_html() -> String {
    r#"<div class="dock-bar" id="canvas-view-bar">
        <label>zoom <input id="canvas-zoom" type="number" value="100" step="5" min="25" max="400"></label>
        <label>anchor <select id="canvas-anchor"><option value="center">center</option><option value="cursor">cursor</option></select></label>
        <button id="canvas-zoom-apply" type="button">apply zoom</button>
        <button id="canvas-fit" type="button">fit</button>
        <button id="canvas-fullscreen" type="button">fullscreen</button>
        <label>pan-x <input id="canvas-pan-x" type="number" value="0" step="10"></label>
        <label>pan-y <input id="canvas-pan-y" type="number" value="0" step="10"></label>
        <button id="canvas-pan-apply" type="button">apply pan</button>
        <label>rulers <input id="canvas-rulers" type="checkbox"></label>
        <label>minimap <input id="canvas-minimap-toggle" type="checkbox"></label>
        <label>safe-90 <input id="canvas-safe-90" type="checkbox"></label>
        <label>safe-70 <input id="canvas-safe-70" type="checkbox"></label>
        </div>
        <p id="canvas-status" role="status">canvas view ready</p>
        <div id="canvas-stage"><div id="canvas-frame-wrap"><div id="canvas-ruler-x" hidden></div><div id="canvas-ruler-y" hidden></div><div id="canvas-safe-90-box" hidden></div><div id="canvas-safe-70-box" hidden></div></div></div>
        <div id="canvas-minimap" hidden><img id="canvas-minimap-img" alt="canvas minimap"><p id="canvas-selection-note">selection: none</p></div>
        <p id="canvas-empty" role="alert" hidden></p>"#.to_string()
}

/// The canvas view panel script (Sprint 17, T001): boots the
/// persisted view, paints zoom, pan, rulers, the minimap, and the
/// safe margins over the true frame, shows the empty state with its
/// reason while the film is missing, and keeps the canvas selection
/// note on the same shared selection the timeline paints.
pub fn canvas_js() -> String {
    r#"        let canvasView = null;
        function canvasStatusText(t) { const s = document.getElementById('canvas-status'); if (s) s.textContent = t; }
        function paintCanvasEmpty(reason, transportOn) {
          const box = document.getElementById('canvas-empty');
          if (!box) return;
          if (!reason) { box.hidden = true; box.textContent = ''; return; }
          box.hidden = false;
          box.textContent = reason + (transportOn ? '' : ' Transport is disabled while the canvas is empty.');
        }
        function paintCanvasOverlays(film) {
          const wrap = document.getElementById('canvas-frame-wrap');
          if (!wrap || !film || !canvasView) return;
          const z = (canvasView['zoom-percent'] || 100) / 100;
          wrap.style.transform = 'translate(' + (canvasView['pan-x'] || 0) + 'px,' + (canvasView['pan-y'] || 0) + 'px) scale(' + z + ')';
          const rx = document.getElementById('canvas-ruler-x');
          const ry = document.getElementById('canvas-ruler-y');
          if (rx) {
            rx.hidden = !canvasView['show-rulers'];
            if (canvasView['show-rulers'] && film.width > 0) {
              const step = canvasView['ruler-step'] || Math.max(1, Math.round(film.width / 10));
              let ticks = '';
              for (let x = 0; x <= film.width; x += step) ticks += x + ' ';
              rx.textContent = 'x: ' + ticks;
            }
          }
          if (ry) {
            ry.hidden = !canvasView['show-rulers'];
            if (canvasView['show-rulers'] && film.height > 0) {
              const step = canvasView['ruler-step'] || Math.max(1, Math.round(film.height / 10));
              let ticks = '';
              for (let y = 0; y <= film.height; y += step) ticks += y + ' ';
              ry.textContent = 'y: ' + ticks;
            }
          }
          const b90 = document.getElementById('canvas-safe-90-box');
          if (b90) {
            b90.hidden = !canvasView['show-safe-90'];
            const r = canvasView['safe-90'];
            if (r && canvasView['show-safe-90']) {
              b90.style.left = r[0] + 'px'; b90.style.top = r[1] + 'px';
              b90.style.width = r[2] + 'px'; b90.style.height = r[3] + 'px';
              b90.title = 'safe-90: ' + r.map((n) => Number(n).toFixed(1)).join(', ');
            }
          }
          const b70 = document.getElementById('canvas-safe-70-box');
          if (b70) {
            b70.hidden = !canvasView['show-safe-70'];
            const r = canvasView['safe-70'];
            if (r && canvasView['show-safe-70']) {
              b70.style.left = r[0] + 'px'; b70.style.top = r[1] + 'px';
              b70.style.width = r[2] + 'px'; b70.style.height = r[3] + 'px';
              b70.title = 'safe-70: ' + r.map((n) => Number(n).toFixed(1)).join(', ');
            }
          }
          const mini = document.getElementById('canvas-minimap');
          if (mini) {
            mini.hidden = !canvasView['show-minimap'];
            const img = document.getElementById('canvas-minimap-img');
            if (img && canvasView['show-minimap']) img.src = '/api/frame?i=' + (canvasView['minimap-frame'] || 0) + '&action=step';
          }
        }
        function paintCanvasSelection() {
          const note = document.getElementById('canvas-selection-note');
          if (note) note.textContent = 'selection: ' + (selection.join(', ') || 'none');
        }
        async function paintCanvas() {
          try {
            const q = new URLSearchParams();
            q.set('composition', (rows && rows.composition) || boot.composition);
            if (selection.length) q.set('selection', selection.join(','));
            const j = await (await fetch('/api/studio/canvas?' + q.toString())).json();
            canvasView = j;
            const pct = document.getElementById('canvas-zoom');
            if (pct) pct.value = Math.round(j['zoom-percent']);
            const an = document.getElementById('canvas-anchor');
            if (an) an.value = j.anchor;
            const px = document.getElementById('canvas-pan-x');
            if (px) px.value = Math.round(j['pan-x']);
            const py = document.getElementById('canvas-pan-y');
            if (py) py.value = Math.round(j['pan-y']);
            const set = (id, v) => { const el = document.getElementById(id); if (el) el.checked = !!v; };
            set('canvas-rulers', j['show-rulers']);
            set('canvas-minimap-toggle', j['show-minimap']);
            set('canvas-safe-90', j['show-safe-90']);
            set('canvas-safe-70', j['show-safe-70']);
            canvasStatusText(j.detail || j['status-line']);
            paintCanvasEmpty(j.empty ? j.reason : null, !!j['transport-enabled']);
            paintCanvasOverlays(j.film);
            if (j.selection && j.selection.kept) {
              const dropped = j.selection.dropped || [];
              if (dropped.length) canvasStatusText((j.detail || j['status-line']) + ' dropped unknown selection: ' + dropped.join(', '));
            }
            paintCanvasSelection();
          } catch (e) { canvasStatusText('canvas view unavailable: ' + e); }
        }
        async function postCanvas(payload) {
          try {
            const r = await fetch('/api/studio/canvas', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(payload)});
            const t = await r.text();
            if (!r.ok) { canvasStatusText(t); banner(t); return; }
            canvasView = JSON.parse(t);
            canvasStatusText(canvasView.detail || canvasView['status-line']);
            paintCanvasOverlays(canvasView.film);
          } catch (e) { canvasStatusText('canvas save failed: ' + e); }
        }
        function wireCanvasButton(id, fn) {
          const el = document.getElementById(id);
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('click', fn); }
        }
        function canvasToggle(id, key) {
          const el = document.getElementById(id);
          if (el && !el.dataset.wiredToggle) {
            el.dataset.wiredToggle = '1';
            el.addEventListener('change', () => postCanvas({[key]: el.checked}));
          }
        }
        wireCanvasButton('canvas-zoom-apply', () => {
          const pct = document.getElementById('canvas-zoom');
          const an = document.getElementById('canvas-anchor');
          postCanvas({'zoom-percent': pct ? Number(pct.value) : 100, anchor: an ? an.value : 'center'});
        });
        wireCanvasButton('canvas-fit', () => postCanvas({fit: true}));
        wireCanvasButton('canvas-fullscreen', () => {
          const panel = document.getElementById('panel-preview');
          const on = !(document.fullscreenElement);
          if (on && panel && panel.requestFullscreen) { panel.requestFullscreen().catch(() => {}); }
          else if (!on && document.exitFullscreen) { document.exitFullscreen().catch(() => {}); }
          postCanvas({fullscreen: on});
        });
        wireCanvasButton('canvas-pan-apply', () => {
          const px = document.getElementById('canvas-pan-x');
          const py = document.getElementById('canvas-pan-y');
          postCanvas({'pan-x': px ? Number(px.value) : 0, 'pan-y': py ? Number(py.value) : 0});
        });
        canvasToggle('canvas-rulers', 'show-rulers');
        canvasToggle('canvas-minimap-toggle', 'show-minimap');
        canvasToggle('canvas-safe-90', 'show-safe-90');
        canvasToggle('canvas-safe-70', 'show-safe-70');
        const basePaintRows = paintRows;
        paintRows = function () { basePaintRows(); paintCanvasSelection(); syncLink(); if (typeof paintLayers === 'function') paintLayers(); };"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-studio-canvas-unit-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn canvas_zoom_clamps_at_25_and_400_showing_the_value() {
        assert_eq!(clamp_canvas_zoom(25.0), (25.0, false));
        assert_eq!(clamp_canvas_zoom(100.0), (100.0, false));
        assert_eq!(clamp_canvas_zoom(400.0), (400.0, false));
        assert_eq!(clamp_canvas_zoom(10.0), (25.0, true));
        assert_eq!(clamp_canvas_zoom(500.0), (400.0, true));
        let view = CanvasView {
            zoom_percent: 400.0,
            ..default_canvas_view()
        };
        let detail = canvas_detail(&view, true, 500.0);
        assert!(
            detail.contains("400"),
            "detail shows the clamped value: {detail}"
        );
        assert!(
            detail.contains("400% ceiling"),
            "detail names the bound: {detail}"
        );
        let floor = canvas_detail(
            &CanvasView {
                zoom_percent: 25.0,
                ..default_canvas_view()
            },
            true,
            10.0,
        );
        assert!(
            floor.contains("25% floor"),
            "floor names its bound: {floor}"
        );
    }

    #[test]
    fn canvas_fit_frames_the_film_centred_with_no_pan() {
        let current = CanvasView {
            zoom_percent: 250.0,
            anchor: CanvasAnchor::Cursor,
            pan_x: 40.0,
            pan_y: -30.0,
            ..default_canvas_view()
        };
        let (view, outcome) = apply_canvas_update(
            &current,
            &CanvasUpdate {
                fit: true,
                ..Default::default()
            },
        );
        assert_eq!(view.zoom_percent, 100.0);
        assert_eq!(view.anchor, CanvasAnchor::Center);
        assert_eq!((view.pan_x, view.pan_y), (0.0, 0.0));
        assert!(!outcome.clamped);
        assert!(
            outcome.detail.contains("fit"),
            "fit says so: {}",
            outcome.detail
        );
    }

    #[test]
    fn canvas_safe_margins_centre_90_and_70_percent() {
        fn close(got: [f64; 4], want: [f64; 4]) {
            for (g, w) in got.iter().zip(want.iter()) {
                assert!((g - w).abs() < 1e-9, "safe rect {got:?} near {want:?}");
            }
        }
        close(
            safe_rect(320, 180, SAFE_MARGIN_90),
            [16.0, 9.0, 288.0, 162.0],
        );
        close(
            safe_rect(320, 180, SAFE_MARGIN_70),
            [48.0, 27.0, 224.0, 126.0],
        );
    }

    #[test]
    fn canvas_empty_states_name_the_reason_and_disable_transport() {
        let no_frames = CanvasFilm {
            width: 320,
            height: 180,
            frame_count: 0,
            font_families: Vec::new(),
        };
        let reason = empty_canvas_reason(&no_frames).expect("0 frames is empty");
        assert!(
            reason.contains("0 frames"),
            "reason names the cause: {reason}"
        );
        let no_size = CanvasFilm {
            width: 0,
            height: 0,
            frame_count: 10,
            font_families: Vec::new(),
        };
        assert!(empty_canvas_reason(&no_size).is_some());
        let film = CanvasFilm {
            width: 320,
            height: 180,
            frame_count: 20,
            font_families: Vec::new(),
        };
        assert_eq!(empty_canvas_reason(&film), None);
        let load = CanvasLoad {
            view: default_canvas_view(),
            restored: false,
            notice: None,
        };
        let state = canvas_state_json(&load, "ready", &no_frames, 0, &[], &[]);
        assert!(state.contains("\"empty\":true"), "empty shows: {state}");
        assert!(
            state.contains("\"transport-enabled\":false"),
            "transport disabled: {state}"
        );
        assert!(state.contains("0 frames"), "reason rides along: {state}");
        let live = canvas_state_json(&load, "ready", &film, 3, &[], &[]);
        assert!(live.contains("\"empty\":false"), "film shows: {live}");
        assert!(
            live.contains("\"transport-enabled\":true"),
            "transport on: {live}"
        );
        assert!(
            live.contains("\"minimap-frame\":3"),
            "minimap frames the playhead: {live}"
        );
    }

    #[test]
    fn canvas_state_keys_stay_hyphenated() {
        let load = CanvasLoad {
            view: default_canvas_view(),
            restored: false,
            notice: None,
        };
        let film = CanvasFilm {
            width: 320,
            height: 180,
            frame_count: 20,
            font_families: vec!["Nope, Fallback".to_string()],
        };
        let state = canvas_state_json(&load, "ready", &film, 0, &[], &[]);
        for key in [
            "\"zoom-percent\"",
            "\"pan-x\"",
            "\"pan-y\"",
            "\"show-rulers\"",
            "\"show-minimap\"",
            "\"show-safe-90\"",
            "\"show-safe-70\"",
            "\"status-line\"",
            "\"frame-count\"",
            "\"safe-90\"",
            "\"safe-70\"",
            "\"ruler-step\"",
            "\"minimap-frame\"",
            "\"transport-enabled\"",
            "\"font-families\"",
        ] {
            assert!(
                state.contains(key),
                "hyphenated key {key} rides along: {state}"
            );
        }
        assert!(
            !state.contains("zoom_percent") && !state.contains("show_rulers"),
            "no snake_case keys leak: {state}"
        );
        assert!(
            state.contains("Nope, Fallback"),
            "font try-order passes through in comma order: {state}"
        );
    }

    #[test]
    fn canvas_view_round_trips_through_the_hidden_file_only() {
        let dir = folder("round-trip");
        let (view, outcome) = save_canvas(
            &dir,
            "{\"zoom-percent\":150,\"anchor\":\"cursor\",\"pan-x\":12,\"pan-y\":-8,\"show-rulers\":true,\"show-safe-90\":true}",
        )
        .expect("valid view saves");
        assert_eq!(view.zoom_percent, 150.0);
        assert_eq!(view.anchor, CanvasAnchor::Cursor);
        assert!(!outcome.clamped);
        let load = load_canvas(&dir);
        assert!(load.restored);
        assert_eq!(load.view, view);
        let top: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            top,
            vec![sdk::HISTORY_DIR_NAME.to_string()],
            "only the hidden history directory lands: {top:?}"
        );
        assert!(
            std::fs::read_to_string(canvas_path_for(&dir))
                .unwrap()
                .contains("\"zoom-percent\": 150"),
            "the hidden file carries the hyphenated view"
        );
    }

    #[test]
    fn canvas_refusals_name_the_cause_and_save_nothing() {
        let dir = folder("refusals");
        for body in [
            "",
            "not json",
            "[1,2]",
            "{}",
            "{\"zoom\":150}",
            "{\"zoom-percent\":\"far\"}",
            "{\"anchor\":\"corner\"}",
            "{\"pan-x\":null}",
            "{\"fullscreen\":\"yes\"}",
            "{\"selection\":[42]}",
            "{\"selection\":[\"\"]}",
        ] {
            let err = parse_canvas_body(body, &default_canvas_view()).expect_err("refuses");
            assert!(err.starts_with("studio refused: "), "refusal prefix: {err}");
            assert!(err.contains("Nothing was saved"), "nothing saved: {err}");
        }
        assert!(
            !canvas_path_for(&dir).exists(),
            "no refusal writes the hidden file"
        );
    }
}
