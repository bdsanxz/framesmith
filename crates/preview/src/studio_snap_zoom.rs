//! Snapping and zoom for the Studio timeline (Sprint 16, T005 /
//! CAP-005, J-02, STATE-03, INT-001).
//!
//! Drags snap to the playhead, clip edges, beats, and the frame grid
//! within a visible threshold (8 px by default) with a guide line; the
//! threshold shows in the panel and changes only through the visible
//! project setting (`POST /api/studio/snap`). With snapping off, drags
//! move freely and the status line says so. Zoom fits the timeline or
//! goes manual from 10 percent to frame level, anchored at the cursor
//! or the playhead, with zoom to selection; zoom beyond either bound
//! clamps and shows the clamped value.
//!
//! Both surfaces are view state: snap resolution stages no composition
//! patch (the drag edit itself still lands as a named history turn
//! through the existing timeline tool route, so one undo system keeps
//! serving clicks, keys, SDK calls, and CLI verbs). Snap and zoom
//! preferences persist as per-project files in the hidden history
//! directory next to the layout file — never beside the sources and
//! never outside the project. Render scratch and caches stay outside
//! the project, as before; default renders (no SDK delta) replay to
//! themselves, so they stay byte-identical.
//!
//! Request bodies use hyphenated keys (`threshold-px`, `px-per-sec`,
//! `anchor-at`); snap and zoom carry no style attributes, so the
//! CSS-like hyphenated vocabulary and the fail-closed `font-family`
//! comma try-order pass through untouched.

use std::path::{Path, PathBuf};

/// File name of the persisted snap settings inside the hidden project
/// directory (next to the layout file, never beside the sources).
pub const STUDIO_SNAP_FILE: &str = "studio-snap.json";

/// Version of the persisted snap settings.
pub const STUDIO_SNAP_VERSION: u32 = 1;

/// The default snap threshold in CSS pixels (Sprint 16, T005 /
/// CAP-005): drags within this distance of a target snap to it.
pub const DEFAULT_SNAP_THRESHOLD_PX: f64 = 8.0;

/// The widest threshold the visible setting accepts: wider magnets
/// stop meaning anything, so the setting refuses instead of silently
/// widening.
pub const MAX_SNAP_THRESHOLD_PX: f64 = 64.0;

/// File name of the persisted zoom preference inside the hidden
/// project directory.
pub const STUDIO_ZOOM_FILE: &str = "studio-zoom.json";

/// Version of the persisted zoom preference.
pub const STUDIO_ZOOM_VERSION: u32 = 1;

/// The manual zoom floor as a percent of fit: zoom never goes below
/// 10 percent, it clamps and shows the clamped value.
pub const MIN_ZOOM_PERCENT: f64 = 10.0;

/// Pixels one frame spans at frame level (the manual zoom ceiling):
/// the ceiling in pixels-per-second is the frame rate times this.
pub const FRAME_LEVEL_PX_PER_FRAME: f64 = 64.0;

/// Hidden snap-settings path for one project folder.
pub fn snap_path_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(STUDIO_SNAP_FILE)
}

/// Hidden zoom-preference path for one project folder.
pub fn zoom_path_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(STUDIO_ZOOM_FILE)
}

/// Prefix every snap/zoom refusal carries; nothing stages or persists
/// on a refusal.
fn refused_snap(message: String) -> String {
    format!("studio refused: {message}")
}

/// The persisted snap settings (Sprint 16, T005): the on/off switch
/// plus the threshold in CSS pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapSettings {
    /// True while drags snap to targets.
    pub enabled: bool,
    /// Snap distance in CSS pixels (the 8 px default).
    pub threshold_px: f64,
}

/// The default snap settings: on, within 8 px.
pub fn default_snap() -> SnapSettings {
    SnapSettings {
        enabled: true,
        threshold_px: DEFAULT_SNAP_THRESHOLD_PX,
    }
}

/// What loading the snap settings reported: the settings to honor,
/// whether they came from the project file, and the fallback banner
/// when they did not.
#[derive(Debug, Clone)]
pub struct SnapLoad {
    /// The settings to honor (the default with no file, or on an
    /// unrestorable file).
    pub settings: SnapSettings,
    /// True when the settings came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file cannot restore.
    pub notice: Option<String>,
}

/// Status line the panel shows for these snap settings: the threshold
/// while on, the free-move note while off.
pub fn snap_status_line(settings: &SnapSettings) -> String {
    if settings.enabled {
        format!(
            "snapping on: drags snap to playhead, edges, beats, and grid within {:.0} px.",
            settings.threshold_px
        )
    } else {
        "snapping is off: drags move freely.".to_string()
    }
}

/// Parse one snap threshold value: finite and inside
/// `0 < threshold <= 64` px, else refused (never clamped silently).
fn parse_threshold_px(value: &serde_json::Value) -> Result<f64, String> {
    let n = value.as_f64().ok_or_else(|| {
        refused_snap(format!(
            "the studio snap `threshold-px` value {value} is not a number; pass finite CSS pixels (0..={MAX_SNAP_THRESHOLD_PX}). Nothing was saved."
        ))
    })?;
    if !n.is_finite() || n <= 0.0 || n > MAX_SNAP_THRESHOLD_PX {
        return Err(refused_snap(format!(
            "the studio snap `threshold-px` value {n:?} is outside 0..={MAX_SNAP_THRESHOLD_PX} px; pass finite CSS pixels in range. Nothing was saved."
        )));
    }
    Ok(n)
}

/// Parse one snap setting body (`{"enabled":false}` and/or
/// `{"threshold-px":4}`): at least one of the two keys must ride
/// along, so a stray body never rewrites the visible setting by
/// accident. A refused body names its cause and saves nothing.
pub fn parse_snap_body(body: &str, current: &SnapSettings) -> Result<SnapSettings, String> {
    if body.trim().is_empty() {
        return Err(refused_snap(
            "the studio snap setting needs a JSON body (`{\"enabled\":false}` or `{\"threshold-px\":4}`); pass the visible setting from the timeline panel. Nothing was saved."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_snap(format!(
            "the studio snap body is not JSON ({e}). Pass `{{\"enabled\":false}}` or `{{\"threshold-px\":4}}`. Nothing was saved."
        ))
    })?;
    let enabled = match value.get("enabled") {
        None => current.enabled,
        Some(v) => v.as_bool().ok_or_else(|| {
            refused_snap(format!(
                "the studio snap `enabled` value {v} is not a switch; pass `true` to snap or `false` for free drags. Nothing was saved."
            ))
        })?,
    };
    let threshold_px = match value.get("threshold-px") {
        None => current.threshold_px,
        Some(v) => parse_threshold_px(v)?,
    };
    if value.get("enabled").is_none() && value.get("threshold-px").is_none() {
        return Err(refused_snap(
            "the studio snap setting names neither `enabled` nor `threshold-px`; pass at least one of the two. Nothing was saved."
                .to_string(),
        ));
    }
    Ok(SnapSettings {
        enabled,
        threshold_px,
    })
}

/// Save one snap setting for the project (Sprint 16, T005): parses and
/// validates the body against the current settings first, then writes
/// the hidden snap file — so a refused body leaves the previous
/// setting intact.
pub fn save_snap(folder: &Path, body: &str) -> Result<SnapSettings, String> {
    let current = load_snap(folder).settings;
    let settings = parse_snap_body(body, &current)?;
    let path = snap_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_snap(format!(
                "the hidden snap directory {} cannot be prepared: {e}. Nothing was saved.",
                parent.display()
            ))
        })?;
    }
    let raw = serde_json::to_string_pretty(&serde_json::json!({
        "version": STUDIO_SNAP_VERSION,
        "enabled": settings.enabled,
        "threshold-px": settings.threshold_px,
    }))
    .map_err(|e| {
        refused_snap(format!(
            "the snap setting cannot be encoded: {e}. Nothing was saved."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_snap(format!(
            "the snap file {} cannot be written: {e}. Nothing was saved.",
            path.display()
        ))
    })?;
    Ok(settings)
}

/// Load the per-project snap settings: a missing file snaps within
/// 8 px with no banner; a corrupt, version-wrong, or invalid file
/// snaps within 8 px and says so.
pub fn load_snap(folder: &Path) -> SnapLoad {
    let fallback = SnapSettings {
        enabled: true,
        threshold_px: DEFAULT_SNAP_THRESHOLD_PX,
    };
    let path = snap_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return SnapLoad {
                settings: fallback,
                restored: false,
                notice: None,
            };
        }
        Err(e) => {
            return SnapLoad {
                settings: fallback,
                restored: false,
                notice: Some(format!(
                    "studio: the snap setting {} cannot be read ({e}); snapping within {:.0} px instead.",
                    path.display(),
                    DEFAULT_SNAP_THRESHOLD_PX
                )),
            };
        }
        Ok(raw) => raw,
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Err(e) => {
            return SnapLoad {
                settings: fallback,
                restored: false,
                notice: Some(format!(
                    "studio: the snap setting {} is not a snap setting ({e}); snapping within {:.0} px instead.",
                    path.display(),
                    DEFAULT_SNAP_THRESHOLD_PX
                )),
            };
        }
        Ok(v) => v,
    };
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version as u32 != STUDIO_SNAP_VERSION {
        return SnapLoad {
            settings: fallback,
            restored: false,
            notice: Some(format!(
                "studio: the snap setting {} carries version {version}, this Studio reads version {}; snapping within {:.0} px instead.",
                path.display(),
                STUDIO_SNAP_VERSION,
                DEFAULT_SNAP_THRESHOLD_PX
            )),
        };
    }
    let enabled = value.get("enabled").and_then(|v| v.as_bool());
    let threshold_px = value
        .get("threshold-px")
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite() && *n > 0.0 && *n <= MAX_SNAP_THRESHOLD_PX);
    match (enabled, threshold_px) {
        (Some(enabled), Some(threshold_px)) => SnapLoad {
            settings: SnapSettings {
                enabled,
                threshold_px,
            },
            restored: true,
            notice: None,
        },
        _ => SnapLoad {
            settings: fallback,
            restored: false,
            notice: Some(format!(
                "studio: the snap setting {} names no valid `enabled` switch and `threshold-px`; snapping within {:.0} px instead.",
                path.display(),
                DEFAULT_SNAP_THRESHOLD_PX
            )),
        },
    }
}

/// Render one snap load as JSON for the snap API: the visible setting
/// (switch plus threshold), whether it came from the project file, the
/// fallback banner, and the status line the panel shows.
pub fn snap_json(load: &SnapLoad) -> String {
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"enabled\":{},\"threshold-px\":{},\"restored\":{},\"notice\":{},\"status-line\":\"{}\"}}",
        load.settings.enabled,
        load.settings.threshold_px,
        load.restored,
        notice,
        crate::studio::json_escape(&snap_status_line(&load.settings))
    )
}

/// One snap target (Sprint 16, T005): a film instant with its label
/// and its tie-break priority (playhead, then edges, then beats,
/// then grid, in spec order).
#[derive(Debug, Clone)]
pub struct SnapCandidate {
    /// Film seconds of the target.
    pub at: f64,
    /// Human label (`playhead`, `clip edge bye start`, `beat down`,
    /// `grid frame 15`).
    pub target: String,
    /// Tie-break priority: lower wins on equal distance.
    pub priority: u8,
}

/// Selection key of one row: the stable `id` when declared, else the
/// `#index` entry — the same spelling deep links resolve.
fn row_key(index: usize, id: Option<&str>) -> String {
    id.map(|s| s.to_string())
        .unwrap_or_else(|| format!("#{index}"))
}

/// Build every snap target of one composition (Sprint 16, T005):
/// the playhead, every clip edge, every beat, and every frame-grid
/// line. Pure view data: no patch stages, no history records.
pub fn build_snap_candidates(
    comp: &notation::model::Composition,
    beats: &[crate::studio::Beat],
    playhead_secs: Option<f64>,
    fps_value: f64,
    frame_count: u64,
) -> Vec<SnapCandidate> {
    let mut out = Vec::new();
    if let Some(playhead) = playhead_secs {
        if playhead.is_finite() && playhead >= 0.0 {
            out.push(SnapCandidate {
                at: playhead,
                target: "playhead".to_string(),
                priority: 0,
            });
        }
    }
    for (index, clip) in comp.clips.iter().enumerate() {
        let key = row_key(index, clip.id());
        let (start, end) = (clip.start(), clip.start() + clip.duration());
        out.push(SnapCandidate {
            at: start,
            target: format!("clip edge {key} start"),
            priority: 1,
        });
        out.push(SnapCandidate {
            at: end,
            target: format!("clip edge {key} end"),
            priority: 1,
        });
    }
    for beat in beats {
        if beat.at.is_finite() && beat.at >= 0.0 {
            out.push(SnapCandidate {
                at: beat.at,
                target: format!("beat {}", beat.name),
                priority: 2,
            });
        }
    }
    if fps_value > 0.0 && frame_count > 0 {
        for i in 0..=frame_count {
            out.push(SnapCandidate {
                at: i as f64 / fps_value,
                target: format!("grid frame {i}"),
                priority: 3,
            });
        }
    }
    out
}

/// The outcome of one drag snap (Sprint 16, T005): the instant the
/// drag lands on, whether a target caught it, and the guide line the
/// panel paints.
#[derive(Debug, Clone)]
pub struct SnapOutcome {
    /// Film seconds the drag lands on (the target when snapped, the
    /// want otherwise).
    pub snapped: f64,
    /// True while a target caught the drag.
    pub did_snap: bool,
    /// Label of the catching target, when one caught the drag.
    pub target: Option<String>,
    /// Film seconds of the guide line, set exactly when snapped.
    pub guide_at: Option<f64>,
}

/// Resolve one drag position against the targets (Sprint 16, T005):
/// the nearest target within `threshold_px / px_per_sec` seconds wins
/// (ties break playhead, then edges, then beats, then grid). With
/// snapping off, every drag moves freely. Pure arithmetic: no patch
/// stages, no history records.
pub fn resolve_snap(
    want: f64,
    settings: &SnapSettings,
    px_per_sec: f64,
    candidates: &[SnapCandidate],
) -> SnapOutcome {
    let free = SnapOutcome {
        snapped: want,
        did_snap: false,
        target: None,
        guide_at: None,
    };
    if !settings.enabled {
        return free;
    }
    if px_per_sec <= 0.0 || !px_per_sec.is_finite() {
        return free;
    }
    let threshold_secs = settings.threshold_px / px_per_sec;
    let mut best: Option<&SnapCandidate> = None;
    let mut best_dist = f64::INFINITY;
    for candidate in candidates {
        if !candidate.at.is_finite() {
            continue;
        }
        let dist = (want - candidate.at).abs();
        let better = dist < best_dist
            || (dist == best_dist && best.is_some_and(|b| candidate.priority < b.priority));
        if better {
            best_dist = dist;
            best = Some(candidate);
        }
    }
    match best {
        Some(hit) if best_dist <= threshold_secs => SnapOutcome {
            snapped: hit.at,
            did_snap: true,
            target: Some(hit.target.clone()),
            guide_at: Some(hit.at),
        },
        _ => free,
    }
}

/// Status line one snap resolution reports: the guide target while
/// snapped, the free-move note while off or out of reach.
pub fn snap_resolve_status_line(
    settings: &SnapSettings,
    outcome: &SnapOutcome,
    want: f64,
) -> String {
    if !settings.enabled {
        return "snapping is off: drags move freely.".to_string();
    }
    match (&outcome.did_snap, &outcome.target, outcome.guide_at) {
        (true, Some(target), Some(at)) => format!(
            "snapped to {target} at {at:.3}s (guide shown; threshold {:.0} px).",
            settings.threshold_px
        ),
        _ => format!(
            "no snap within {:.0} px of {want:.3}s: the drag moves freely.",
            settings.threshold_px
        ),
    }
}

/// Render one snap resolution as JSON for the snap-resolve API.
pub fn snap_resolve_json(settings: &SnapSettings, outcome: &SnapOutcome, want: f64) -> String {
    let target = match &outcome.target {
        Some(t) => format!("\"{}\"", crate::studio::json_escape(t)),
        None => "null".to_string(),
    };
    let guide_at = match outcome.guide_at {
        Some(at) => format!("{at:.3}"),
        None => "null".to_string(),
    };
    format!(
        "{{\"want\":{want:.3},\"snapped\":{:.3},\"did_snap\":{},\"target\":{target},\"guide_at\":{guide_at},\"status-line\":\"{}\"}}",
        outcome.snapped,
        outcome.did_snap,
        crate::studio::json_escape(&snap_resolve_status_line(settings, outcome, want))
    )
}

/// One parsed snap-resolve request: the drag position plus the pixels
/// behind it. Structural parsing only; the route fills the targets
/// from the effective composition.
#[derive(Debug, Clone)]
pub struct SnapResolveRequest {
    /// Drag position in film seconds.
    pub want: f64,
    /// Playhead in film seconds (the transport playhead when omitted).
    pub playhead: Option<f64>,
    /// Timeline scale in pixels per film second.
    pub px_per_sec: f64,
    /// Composition the drag belongs to, when named.
    pub composition: Option<String>,
}

/// Read one request number: present, numeric, and finite. Missing
/// returns `None` (the caller defaults it); anything else refuses
/// naming the key.
fn snap_number(value: &serde_json::Value, key: &str) -> Result<Option<f64>, String> {
    let raw = match value.get(key) {
        None => return Ok(None),
        Some(v) => v,
    };
    let n = raw.as_f64().ok_or_else(|| {
        refused_snap(format!(
            "the studio snap `{key}` value {raw} is not seconds; pass a finite number of film seconds. Nothing was resolved."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_snap(format!(
            "the studio snap `{key}` value {n:?} is not finite; pass a finite number of film seconds. Nothing was resolved."
        )));
    }
    Ok(Some(n))
}

/// Parse one snap-resolve body
/// (`{"want":2.06,"playhead":0.5,"px-per-sec":100}`): the drag
/// position and the timeline scale must ride along; the playhead
/// defaults to the transport playhead at the route. A refused body
/// names its cause and resolves nothing.
pub fn parse_snap_resolve_body(body: &str) -> Result<SnapResolveRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_snap(
            "the studio snap resolve needs a JSON body (`{\"want\":2.06,\"px-per-sec\":100}`); pass the drag position with the timeline scale. Nothing was resolved."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_snap(format!(
            "the studio snap body is not JSON ({e}). Pass `{{\"want\":2.06,\"px-per-sec\":100}}`. Nothing was resolved."
        ))
    })?;
    let want = snap_number(&value, "want")?.ok_or_else(|| {
        refused_snap(
            "the studio snap resolve names no `want` position; pass the drag position in film seconds. Nothing was resolved."
                .to_string(),
        )
    })?;
    let playhead = snap_number(&value, "playhead")?;
    let px_per_sec = snap_number(&value, "px-per-sec")?.ok_or_else(|| {
        refused_snap(
            "the studio snap resolve names no `px-per-sec` scale; pass the timeline pixels per film second. Nothing was resolved."
                .to_string(),
        )
    })?;
    if px_per_sec <= 0.0 {
        return Err(refused_snap(format!(
            "the studio snap `px-per-sec` value {px_per_sec:?} never shows a timeline; pass pixels per film second above 0. Nothing was resolved."
        )));
    }
    let composition = value
        .get("composition")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string());
    Ok(SnapResolveRequest {
        want,
        playhead,
        px_per_sec,
        composition,
    })
}

/// One zoom mode (Sprint 16, T005): fit fills the viewport, manual
/// holds a percent of fit, selection frames the selected clips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomMode {
    /// Fit the whole timeline into the viewport.
    Fit,
    /// A manual percent of fit (10 percent to frame level).
    Manual,
    /// Frame the selected clips.
    Selection,
}

impl ZoomMode {
    /// The persisted and served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ZoomMode::Fit => "fit",
            ZoomMode::Manual => "manual",
            ZoomMode::Selection => "selection",
        }
    }

    /// Parse one mode spelling; anything else refuses naming the three.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "fit" => Some(ZoomMode::Fit),
            "manual" => Some(ZoomMode::Manual),
            "selection" => Some(ZoomMode::Selection),
            _ => None,
        }
    }
}

/// One zoom anchor (Sprint 16, T005): the instant that stays put while
/// the scale changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomAnchor {
    /// The cursor instant stays put.
    Cursor,
    /// The playhead instant stays put.
    Playhead,
}

impl ZoomAnchor {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ZoomAnchor::Cursor => "cursor",
            ZoomAnchor::Playhead => "playhead",
        }
    }

    /// Parse one anchor spelling; anything else refuses naming the two.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "cursor" => Some(ZoomAnchor::Cursor),
            "playhead" => Some(ZoomAnchor::Playhead),
            _ => None,
        }
    }
}

/// The persisted zoom preference (Sprint 16, T005): the mode, the
/// percent of fit it shows, and the anchor it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoomPreference {
    /// The zoom mode (`fit`, `manual`, or `selection`).
    pub mode: ZoomMode,
    /// Percent of fit the manual and selection modes show.
    pub percent: f64,
    /// The anchor the scale holds.
    pub anchor: ZoomAnchor,
}

/// The default zoom preference: fit, anchored at the playhead.
pub fn default_zoom() -> ZoomPreference {
    ZoomPreference {
        mode: ZoomMode::Fit,
        percent: 100.0,
        anchor: ZoomAnchor::Playhead,
    }
}

/// What loading the zoom preference reported: the preference to
/// honor, whether it came from the project file, and the fallback
/// banner when it did not.
#[derive(Debug, Clone)]
pub struct ZoomLoad {
    /// The preference to honor (fit on a missing or unrestorable file).
    pub pref: ZoomPreference,
    /// True when the preference came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file cannot restore.
    pub notice: Option<String>,
}

/// Status line the panel shows for one zoom preference.
pub fn zoom_status_line(pref: &ZoomPreference) -> String {
    match pref.mode {
        ZoomMode::Fit => "zoom fit: the timeline fills the viewport.".to_string(),
        ZoomMode::Manual => format!(
            "zoom {:.0}% anchored at {}.",
            pref.percent,
            pref.anchor.as_str()
        ),
        ZoomMode::Selection => format!(
            "zoom to selection at {:.0}% anchored at {}.",
            pref.percent,
            pref.anchor.as_str()
        ),
    }
}

/// Load the per-project zoom preference: a missing file fits with no
/// banner; a corrupt, version-wrong, or invalid file fits and says so.
pub fn load_zoom(folder: &Path) -> ZoomLoad {
    let path = zoom_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ZoomLoad {
                pref: default_zoom(),
                restored: false,
                notice: None,
            };
        }
        Err(e) => {
            return ZoomLoad {
                pref: default_zoom(),
                restored: false,
                notice: Some(format!(
                    "studio: the zoom setting {} cannot be read ({e}); zooming fit instead.",
                    path.display()
                )),
            };
        }
        Ok(raw) => raw,
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Err(e) => {
            return ZoomLoad {
                pref: default_zoom(),
                restored: false,
                notice: Some(format!(
                    "studio: the zoom setting {} is not a zoom setting ({e}); zooming fit instead.",
                    path.display()
                )),
            };
        }
        Ok(v) => v,
    };
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version as u32 != STUDIO_ZOOM_VERSION {
        return ZoomLoad {
            pref: default_zoom(),
            restored: false,
            notice: Some(format!(
                "studio: the zoom setting {} carries version {version}, this Studio reads version {}; zooming fit instead.",
                path.display(),
                STUDIO_ZOOM_VERSION
            )),
        };
    }
    let mode = value
        .get("mode")
        .and_then(|v| v.as_str())
        .and_then(ZoomMode::parse);
    let percent = value
        .get("percent")
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite() && *n > 0.0);
    let anchor = value
        .get("anchor")
        .and_then(|v| v.as_str())
        .and_then(ZoomAnchor::parse);
    match (mode, percent, anchor) {
        (Some(mode), Some(percent), Some(anchor)) => ZoomLoad {
            pref: ZoomPreference {
                mode,
                percent,
                anchor,
            },
            restored: true,
            notice: None,
        },
        _ => ZoomLoad {
            pref: default_zoom(),
            restored: false,
            notice: Some(format!(
                "studio: the zoom setting {} names no valid `mode`, `percent`, and `anchor`; zooming fit instead.",
                path.display()
            )),
        },
    }
}

/// Persist one zoom preference (Sprint 16, T005): refusals never reach
/// this write, so the saved preference always shows what the panel
/// last applied.
fn save_zoom_pref(folder: &Path, pref: &ZoomPreference) -> Result<(), String> {
    let path = zoom_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_snap(format!(
                "the hidden zoom directory {} cannot be prepared: {e}. Nothing was saved.",
                parent.display()
            ))
        })?;
    }
    let raw = serde_json::to_string_pretty(&serde_json::json!({
        "version": STUDIO_ZOOM_VERSION,
        "mode": pref.mode.as_str(),
        "percent": pref.percent,
        "anchor": pref.anchor.as_str(),
    }))
    .map_err(|e| {
        refused_snap(format!(
            "the zoom setting cannot be encoded: {e}. Nothing was saved."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_snap(format!(
            "the zoom file {} cannot be written: {e}. Nothing was saved.",
            path.display()
        ))
    })?;
    Ok(())
}

/// Render one zoom load as JSON for the zoom API.
pub fn zoom_json(load: &ZoomLoad) -> String {
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"mode\":\"{}\",\"percent\":{},\"anchor\":\"{}\",\"restored\":{},\"notice\":{},\"status-line\":\"{}\"}}",
        load.pref.mode.as_str(),
        load.pref.percent,
        load.pref.anchor.as_str(),
        load.restored,
        notice,
        crate::studio::json_escape(&zoom_status_line(&load.pref))
    )
}

/// One parsed zoom request (Sprint 16, T005): the mode plus its
/// hyphenated parameters. Structural parsing only; the scale math runs
/// against the composition at the route.
#[derive(Debug, Clone)]
pub struct ZoomRequest {
    /// The zoom mode to apply.
    pub mode: ZoomMode,
    /// Manual percent of fit (`manual` needs it).
    pub percent: Option<f64>,
    /// The anchor the scale holds (the playhead when omitted).
    pub anchor: ZoomAnchor,
    /// Cursor instant in film seconds (`cursor` anchors read it).
    pub anchor_at: Option<f64>,
    /// Viewport width in CSS pixels (every mode needs it).
    pub viewport_px: Option<f64>,
    /// Clip references to frame (`selection` needs them).
    pub selection: Vec<String>,
    /// Composition the zoom belongs to, when named.
    pub composition: Option<String>,
}

/// Parse one zoom body (`{"mode":"manual","percent":150,
/// "anchor":"cursor","anchor-at":2.5,"viewport-px":800}`): the mode
/// names the three entries; the viewport width must ride along. A
/// refused body names its cause and zooms nothing.
pub fn parse_zoom_body(body: &str) -> Result<ZoomRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_snap(
            "the studio zoom needs a JSON zoom body (`{\"mode\":\"fit\",\"viewport-px\":800}`); pass the zoom from the timeline panel. Nothing was zoomed."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_snap(format!(
            "the studio zoom body is not JSON ({e}). Pass `{{\"mode\":\"fit\",\"viewport-px\":800}}`. Nothing was zoomed."
        ))
    })?;
    let mode_raw = value.get("mode").and_then(|m| m.as_str()).unwrap_or("");
    let mode = ZoomMode::parse(mode_raw).ok_or_else(|| {
        refused_snap(format!(
            "unknown studio zoom mode {mode_raw:?}: the timeline zooms `fit`, `manual`, and `selection`. Pass one of those modes. Nothing was zoomed."
        ))
    })?;
    let number = |key: &str| -> Result<Option<f64>, String> {
        match value.get(key) {
            None => Ok(None),
            Some(v) => {
                let n = v.as_f64().ok_or_else(|| {
                    refused_snap(format!(
                        "the studio zoom `{key}` value {v} is not a number; pass a finite number. Nothing was zoomed."
                    ))
                })?;
                if !n.is_finite() {
                    return Err(refused_snap(format!(
                        "the studio zoom `{key}` value {n:?} is not finite; pass a finite number. Nothing was zoomed."
                    )));
                }
                Ok(Some(n))
            }
        }
    };
    let percent = number("percent")?;
    let anchor_at = number("anchor-at")?;
    let viewport_px = number("viewport-px")?;
    let anchor = match value.get("anchor").and_then(|a| a.as_str()) {
        None => ZoomAnchor::Playhead,
        Some(raw) => ZoomAnchor::parse(raw).ok_or_else(|| {
            refused_snap(format!(
                "unknown studio zoom anchor {raw:?}: the timeline anchors at `cursor` or `playhead`. Pass one of those anchors. Nothing was zoomed."
            ))
        })?,
    };
    if mode == ZoomMode::Manual && percent.is_none() {
        return Err(refused_snap(
            "the studio `manual` zoom names no `percent`; pass the percent of fit (10 percent to frame level). Nothing was zoomed."
                .to_string(),
        ));
    }
    let selection = match value.get("selection") {
        None => Vec::new(),
        Some(v) => {
            let list = v.as_array().ok_or_else(|| {
                refused_snap(format!(
                    "the studio zoom `selection` value {v} is not a clip list; pass declared `id` values or `#index` entries. Nothing was zoomed."
                ))
            })?;
            let mut out = Vec::new();
            for entry in list {
                let name = entry.as_str().ok_or_else(|| {
                    refused_snap(format!(
                        "the studio zoom `selection` entry {entry} is not a clip reference; pass a declared `id` or a `#index` entry. Nothing was zoomed."
                    ))
                })?;
                if name.trim().is_empty() {
                    return Err(refused_snap(
                        "the studio zoom `selection` names an empty clip; pass a declared `id` or a `#index` entry. Nothing was zoomed."
                            .to_string(),
                    ));
                }
                out.push(name.to_string());
            }
            out
        }
    };
    if mode == ZoomMode::Selection && selection.is_empty() {
        return Err(refused_snap(
            "the studio `selection` zoom names no clips; pass at least one declared `id` or `#index` entry. Nothing was zoomed."
                .to_string(),
        ));
    }
    let composition = value
        .get("composition")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string());
    Ok(ZoomRequest {
        mode,
        percent,
        anchor,
        anchor_at,
        viewport_px,
        selection,
        composition,
    })
}

/// The outcome of one zoom (Sprint 16, T005): the scale the panel
/// paints, whether either bound clamped it, and the anchor it holds.
#[derive(Debug, Clone)]
pub struct ZoomOutcome {
    /// Timeline scale in pixels per film second.
    pub px_per_sec: f64,
    /// Percent of fit the scale shows (the clamped value when clamped).
    pub percent: f64,
    /// True while a bound clamped the request.
    pub clamped: bool,
    /// The anchor the scale holds.
    pub anchor: ZoomAnchor,
    /// Film seconds the anchor sits at.
    pub anchor_at: f64,
    /// Content pixels left of the anchor (`anchor_at * px_per_sec`).
    pub anchor_px: f64,
    /// Span the zoom frames (`selection` sets it).
    pub span: Option<(f64, f64)>,
    /// One-line summary naming the scale and any clamp.
    pub detail: String,
}

/// Resolve one clip reference against the rows: `#N` reads as a
/// document index, any other spelling as a stable `id`. An `id` that
/// names nothing refuses even when numeric, never falling back
/// silently.
fn resolve_zoom_clip(rows: &[crate::studio::StudioRow], raw: &str) -> Result<usize, String> {
    if let Some(rest) = raw.strip_prefix('#') {
        let index: usize = rest.parse().map_err(|_| {
            refused_snap(format!(
                "the studio zoom clip {raw:?} is not a `#index` entry; pass `#N` (document index) or a declared `id`. Nothing was zoomed."
            ))
        })?;
        rows.iter()
            .position(|r| r.index == index)
            .ok_or_else(|| {
                refused_snap(format!(
                    "the studio zoom clip {raw:?} is past the open composition holding {} clip(s); pass an index from 0 to {}. Nothing was zoomed.",
                    rows.len(),
                    rows.len().saturating_sub(1)
                ))
            })
    } else if let Some(pos) = rows.iter().position(|r| r.id.as_deref() == Some(raw)) {
        Ok(pos)
    } else {
        Err(refused_snap(format!(
            "the studio zoom clip {raw:?} names no clip in the open composition holding {} clip(s); pass a declared `id` or a `#index` entry. Nothing was zoomed.",
            rows.len()
        )))
    }
}

/// Apply one parsed zoom against the composition (Sprint 16, T005):
/// fit fills the viewport, manual holds a percent of fit clamped from
/// 10 percent to frame level, selection frames the selected clips.
/// Out-of-bounds scales clamp and report the clamped value; the anchor
/// (cursor or playhead) stays put. An empty timeline refuses; the
/// saved preference updates only on success, so a refusal keeps the
/// previous zoom.
#[allow(clippy::too_many_arguments)]
pub fn apply_zoom(
    folder: &Path,
    req: &ZoomRequest,
    rows: &[crate::studio::StudioRow],
    duration_secs: f64,
    fps_value: f64,
    playhead_secs: f64,
) -> Result<(ZoomOutcome, ZoomPreference), String> {
    if duration_secs <= 0.0 || !duration_secs.is_finite() {
        return Err(refused_snap(
            "the studio zoom meets an empty timeline; open a composition with duration first. Nothing was zoomed."
                .to_string(),
        ));
    }
    if fps_value <= 0.0 || !fps_value.is_finite() {
        return Err(refused_snap(
            "the studio zoom meets no frame rate; open a composition with a frame rate first. Nothing was zoomed."
                .to_string(),
        ));
    }
    let viewport_px = req.viewport_px.ok_or_else(|| {
        refused_snap(
            "the studio zoom names no `viewport-px` width; pass the timeline viewport in CSS pixels. Nothing was zoomed."
                .to_string(),
        )
    })?;
    if viewport_px <= 0.0 {
        return Err(refused_snap(format!(
            "the studio zoom `viewport-px` value {viewport_px:?} never shows a timeline; pass a viewport width above 0. Nothing was zoomed."
        )));
    }
    let fit_px = viewport_px / duration_secs;
    let mut ceiling_px = fps_value * FRAME_LEVEL_PX_PER_FRAME;
    let floor_px = fit_px * MIN_ZOOM_PERCENT / 100.0;
    if ceiling_px < floor_px {
        ceiling_px = floor_px;
    }
    let anchor_at = match req.anchor {
        ZoomAnchor::Cursor => req.anchor_at.unwrap_or(duration_secs / 2.0),
        ZoomAnchor::Playhead => req.anchor_at.or(Some(playhead_secs)).unwrap_or(0.0),
    };
    if !anchor_at.is_finite() || anchor_at < 0.0 {
        return Err(refused_snap(format!(
            "the studio zoom anchor at {anchor_at:?} sits outside the timeline; pass film seconds at or after 0. Nothing was zoomed."
        )));
    }
    let (mut px_per_sec, span, mode, anchor) = match req.mode {
        ZoomMode::Fit => (fit_px, None, ZoomMode::Fit, req.anchor),
        ZoomMode::Manual => {
            let want_percent = req.percent.expect("manual names percent");
            (
                fit_px * want_percent / 100.0,
                None,
                ZoomMode::Manual,
                req.anchor,
            )
        }
        ZoomMode::Selection => {
            if rows.is_empty() {
                return Err(refused_snap(
                    "the studio `selection` zoom meets no clips; add a clip before framing a selection. Nothing was zoomed."
                        .to_string(),
                ));
            }
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for raw in &req.selection {
                let pos = resolve_zoom_clip(rows, raw)?;
                let row = &rows[pos];
                lo = lo.min(row.start);
                hi = hi.max(row.end);
            }
            if lo >= hi || !lo.is_finite() || !hi.is_finite() {
                return Err(refused_snap(format!(
                    "the studio `selection` zoom spans no time ({}); select clips whose windows open time. Nothing was zoomed.",
                    req.selection.join(", ")
                )));
            }
            let span = hi - lo;
            (
                viewport_px / span,
                Some((lo, hi)),
                ZoomMode::Selection,
                ZoomAnchor::Cursor,
            )
        }
    };
    let mut clamped = false;
    if px_per_sec < floor_px {
        px_per_sec = floor_px;
        clamped = true;
    }
    if px_per_sec > ceiling_px {
        px_per_sec = ceiling_px;
        clamped = true;
    }
    let percent = px_per_sec / fit_px * 100.0;
    let shown_anchor_at = if mode == ZoomMode::Selection {
        let (lo, hi) = span.expect("selection sets span");
        (lo + hi) / 2.0
    } else {
        anchor_at
    };
    let anchor_px = shown_anchor_at * px_per_sec;
    let detail = if clamped {
        format!(
            "zoom {} at {percent:.0}% anchored at {} ({shown_anchor_at:.3}s): the request ran past the {} bound, clamped to {percent:.0}%.",
            mode.as_str(),
            anchor.as_str(),
            if px_per_sec >= ceiling_px {
                "frame level"
            } else {
                "10 percent floor"
            }
        )
    } else {
        match mode {
            ZoomMode::Fit => "zoom fit: the timeline fills the viewport.".to_string(),
            ZoomMode::Manual => format!(
                "zoom manual at {percent:.0}% anchored at {} ({shown_anchor_at:.3}s).",
                anchor.as_str()
            ),
            ZoomMode::Selection => {
                format!("zoom to selection at {percent:.0}%: the selected clips fill the viewport.",)
            }
        }
    };
    let pref = ZoomPreference {
        mode,
        percent,
        anchor,
    };
    save_zoom_pref(folder, &pref)?;
    Ok((
        ZoomOutcome {
            px_per_sec,
            percent,
            clamped,
            anchor,
            anchor_at: shown_anchor_at,
            anchor_px,
            span,
            detail,
        },
        pref,
    ))
}

/// Render one zoom outcome as JSON for the zoom API: the scale the
/// panel paints, the clamp report, the anchor it holds, and the saved
/// preference behind it.
pub fn zoom_result_json(outcome: &ZoomOutcome, pref: &ZoomPreference) -> String {
    let span = match outcome.span {
        Some((lo, hi)) => format!("[{lo:.3},{hi:.3}]"),
        None => "null".to_string(),
    };
    format!(
        "{{\"px-per-sec\":{:.3},\"percent\":{:.3},\"clamped\":{},\"anchor\":\"{}\",\"anchor-at\":{:.3},\"anchor-px\":{:.3},\"span\":{span},\"detail\":\"{}\",\"saved\":{{\"mode\":\"{}\",\"percent\":{:.3},\"anchor\":\"{}\"}}}}",
        outcome.px_per_sec,
        outcome.percent,
        outcome.clamped,
        outcome.anchor.as_str(),
        outcome.anchor_at,
        outcome.anchor_px,
        crate::studio::json_escape(&outcome.detail),
        pref.mode.as_str(),
        pref.percent,
        pref.anchor.as_str()
    )
}
