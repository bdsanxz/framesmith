//! Dockable Studio shell (Sprint 16, T001 / CAP-001, J-01, STATE-02,
//! REC-003).
//!
//! The Studio shell grows the preview and player pages: one browser page
//! hosting `preview` and `timeline` panels that dock, drag, group, float,
//! maximise, close, and reset. The arrangement saves per project and
//! restores on reopen; the URL carries project, composition, time, and
//! selection so a deep link reopens the same view. One project serves per
//! Studio server.
//!
//! Layout lives in the project's hidden history directory
//! (`<project>/.framesmith/studio-layout.json`), next to the hidden SDK
//! deltas — never beside the sources and never outside the project. A
//! layout that cannot restore falls back to the default arrangement and
//! says so; a deep link to a missing composition lands on the project
//! root with a banner.
//!
//! Read boundary: visual edits never bypass the edit SDK — every
//! autosaved change applies through the Sprint 15 patch and history
//! machinery ([`apply_studio_edit`]), so one undo system keeps
//! serving clicks, keys, SDK calls, and CLI verbs alike. Readouts
//! (timeline rows, deep-link keys) serve the SDK effective
//! composition with the persisted delta replayed, so saved turns show
//! at once; served frame pixels still come from the preview pipeline
//! over the on-disk sources, which SDK deltas never touch.
//!
//! Style boundary: style attributes stay CSS-like hyphenated end to end
//! (`background-color`, `border-radius`, `font-variant-numeric`,
//! `object-fit`), and `font-family` try-order lists pass through
//! untouched — unknown families stay fail-closed at render time, never
//! resolved or substituted here.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Studio layout file name inside the hidden project directory.
pub const STUDIO_LAYOUT_FILE: &str = "studio-layout.json";

/// Version of the persisted Studio layout.
pub const STUDIO_LAYOUT_VERSION: u32 = 1;

/// Panels the shell hosts in this slice.
pub const STUDIO_PANELS: [&str; 2] = ["preview", "timeline"];

/// Dock areas a panel may occupy (`float` parks the panel freely;
/// hiding is `visible: false`, never a dock).
pub const STUDIO_DOCKS: [&str; 5] = ["main", "left", "right", "bottom", "float"];

/// Hidden history directory name owning the layout file (mirrors the
/// SDK hidden directory; render, check, and validation never read it).
const HIDDEN_DIR: &str = ".framesmith";

/// Hidden layout path for one project folder.
pub fn studio_layout_path_for(folder: &Path) -> PathBuf {
    folder.join(HIDDEN_DIR).join(STUDIO_LAYOUT_FILE)
}

/// Where one panel sits (Sprint 16, T001): a dock area, an optional
/// group partner, a maximise flag, a visibility switch, and an
/// optional floating rectangle in CSS pixels.
fn default_dock() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PanelPlacement {
    /// Dock area, one of [`STUDIO_DOCKS`] (`main` when omitted).
    #[serde(default = "default_dock")]
    pub dock: String,
    /// Partner panel id this panel groups with, when grouped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// True while the panel fills the shell.
    #[serde(default)]
    pub maximized: bool,
    /// False while the panel is closed (reset reopens it).
    #[serde(default = "visible_true")]
    pub visible: bool,
    /// Floating rectangle `[x, y, w, h]` in CSS pixels, used while
    /// `dock` is `float`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub float_rect: Option<[f64; 4]>,
}

fn visible_true() -> bool {
    true
}

impl PanelPlacement {
    fn docked(dock: &str, order: usize) -> (String, Self) {
        let _ = order;
        (
            dock.to_string(),
            Self {
                dock: dock.to_string(),
                group: None,
                maximized: false,
                visible: true,
                float_rect: None,
            },
        )
    }
}

/// The saved per-project arrangement (Sprint 16, T001): one placement
/// per panel plus the paint order. `order` holds panel ids front to
/// back; panels missing from `order` paint last in id order.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StudioLayout {
    /// Layout version ([`STUDIO_LAYOUT_VERSION`]).
    pub version: u32,
    /// Placement per panel id.
    pub panels: BTreeMap<String, PanelPlacement>,
    /// Panel ids in paint order (missing ids paint last in id order).
    #[serde(default)]
    pub order: Vec<String>,
}

/// Fill paint-order gaps: panels missing from `order` paint last in id
/// order, so a body naming only placements still restores fully.
fn normalize_layout(layout: &mut StudioLayout) {
    for id in layout.panels.keys() {
        if !layout.order.iter().any(|o| o == id) {
            layout.order.push(id.clone());
        }
    }
}

/// Normalize one parsed layout, then validate it.
fn normalize_then_validate(layout: &mut StudioLayout) -> Result<(), String> {
    normalize_layout(layout);
    validate_layout(layout)
}

/// The default arrangement (Sprint 16, T001): preview fills the main
/// area, the timeline docks along the bottom.
pub fn default_layout() -> StudioLayout {
    let mut panels = BTreeMap::new();
    let (_, preview) = PanelPlacement::docked("main", 0);
    let (_, timeline) = PanelPlacement::docked("bottom", 1);
    panels.insert("preview".to_string(), preview);
    panels.insert("timeline".to_string(), timeline);
    StudioLayout {
        version: STUDIO_LAYOUT_VERSION,
        panels,
        order: vec!["preview".to_string(), "timeline".to_string()],
    }
}

/// Why a layout body refused.
fn refused_layout(message: String) -> String {
    format!("studio refused: {message}")
}

/// Check one parsed layout: the version must match, every hosted panel
/// must be placed, no unknown panel may appear, docks must name a
/// known area, and groups must name another known panel.
pub fn validate_layout(layout: &StudioLayout) -> Result<(), String> {
    if layout.version != STUDIO_LAYOUT_VERSION {
        return Err(refused_layout(format!(
            "the saved layout carries version {}, this Studio reads version {}; reset the layout and re-arrange. Nothing was applied.",
            layout.version, STUDIO_LAYOUT_VERSION
        )));
    }
    for id in STUDIO_PANELS {
        if !layout.panels.contains_key(id) {
            return Err(refused_layout(format!(
                "the saved layout places no panel {id:?}; the shell hosts {STUDIO_PANELS:?}. Reset the layout and re-arrange. Nothing was applied."
            )));
        }
    }
    for id in layout.panels.keys() {
        if !STUDIO_PANELS.contains(&id.as_str()) {
            return Err(refused_layout(format!(
                "unknown panel {id:?}: the shell hosts {STUDIO_PANELS:?}. Remove it and retry. Nothing was applied."
            )));
        }
    }
    for (id, panel) in &layout.panels {
        if !STUDIO_DOCKS.contains(&panel.dock.as_str()) {
            return Err(refused_layout(format!(
                "unknown dock {:?} on panel {id:?}: dock one of {STUDIO_DOCKS:?}. Fix it and retry. Nothing was applied.",
                panel.dock
            )));
        }
        if let Some(group) = &panel.group {
            if group == id {
                return Err(refused_layout(format!(
                    "panel {id:?} groups with itself; group it with another panel or ungroup it. Nothing was applied."
                )));
            }
            if !STUDIO_PANELS.contains(&group.as_str()) {
                return Err(refused_layout(format!(
                    "panel {id:?} groups with unknown panel {group:?}: the shell hosts {STUDIO_PANELS:?}. Fix it and retry. Nothing was applied."
                )));
            }
        }
        if let Some(rect) = panel.float_rect {
            if rect.iter().any(|v| !v.is_finite() || *v < 0.0) || rect[2] <= 0.0 || rect[3] <= 0.0 {
                return Err(refused_layout(format!(
                    "panel {id:?} carries a floating rectangle {rect:?}; pass finite x/y at or after 0 with a positive size. Fix it and retry. Nothing was applied."
                )));
            }
        }
    }
    for id in &layout.order {
        if !STUDIO_PANELS.contains(&id.as_str()) {
            return Err(refused_layout(format!(
                "unknown panel {id:?} in the paint order: the shell hosts {STUDIO_PANELS:?}. Remove it and retry. Nothing was applied."
            )));
        }
    }
    Ok(())
}

/// Parse and validate one layout request body.
pub fn parse_layout_body(body: &str) -> Result<StudioLayout, String> {
    if body.trim().is_empty() {
        return Err(refused_layout(
            "the layout save needs a JSON layout body; pass the arrangement from the shell. Nothing was saved.".to_string(),
        ));
    }
    let mut layout: StudioLayout = serde_json::from_str(body).map_err(|e| {
        refused_layout(format!(
            "the layout body is not a Studio layout: {e}. Pass the arrangement from the shell. Nothing was saved."
        ))
    })?;
    normalize_then_validate(&mut layout)?;
    Ok(layout)
}

/// What loading the saved arrangement reported (Sprint 16, T001): the
/// layout to paint, whether it came from the project file, and the
/// fallback banner when it did not.
#[derive(Debug, Clone)]
pub struct LayoutLoad {
    /// The arrangement to paint (the saved one, or the default).
    pub layout: StudioLayout,
    /// True when the arrangement came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file exists but
    /// cannot restore.
    pub notice: Option<String>,
}

/// Load the per-project arrangement (Sprint 16, T001): a missing file
/// paints the default with no banner; a corrupt, versioned-wrong, or
/// invalid file paints the default and says so.
pub fn load_layout(folder: &Path) -> LayoutLoad {
    let path = studio_layout_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return LayoutLoad {
                layout: default_layout(),
                restored: false,
                notice: None,
            };
        }
        Err(e) => {
            return LayoutLoad {
                layout: default_layout(),
                restored: false,
                notice: Some(format!(
                    "studio: the saved layout {} cannot be read ({e}); showing the default arrangement instead.",
                    path.display()
                )),
            };
        }
        Ok(raw) => raw,
    };
    match serde_json::from_str::<StudioLayout>(&raw) {
        Err(e) => LayoutLoad {
            layout: default_layout(),
            restored: false,
            notice: Some(format!(
                "studio: the saved layout {} is not a Studio layout ({e}); showing the default arrangement instead.",
                path.display()
            )),
        },
        Ok(mut layout) => match normalize_then_validate(&mut layout) {
            Ok(()) => LayoutLoad {
                layout,
                restored: true,
                notice: None,
            },
            Err(why) => LayoutLoad {
                layout: default_layout(),
                restored: false,
                notice: Some(format!(
                    "studio: the saved layout {} cannot restore ({why}); showing the default arrangement instead.",
                    path.display()
                )),
            },
        },
    }
}

/// Save one arrangement for the project (Sprint 16, T001): parses and
/// validates the body first, then writes the hidden layout file —
/// creating the hidden directory when missing — so a refused body
/// leaves the previous arrangement intact.
pub fn save_layout(folder: &Path, body: &str) -> Result<StudioLayout, String> {
    let layout = parse_layout_body(body)?;
    let path = studio_layout_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_layout(format!(
                "the hidden layout directory {} cannot be prepared: {e}. Nothing was saved.",
                parent.display()
            ))
        })?;
    }
    let raw = serde_json::to_string_pretty(&layout).map_err(|e| {
        refused_layout(format!(
            "the arrangement cannot be encoded: {e}. Nothing was saved."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_layout(format!(
            "the layout file {} cannot be written: {e}. Nothing was saved.",
            path.display()
        ))
    })?;
    Ok(layout)
}

/// Forget the saved arrangement (Sprint 16, T001): the shell paints the
/// default next. A missing file still resets cleanly.
pub fn reset_layout(folder: &Path) {
    let _ = std::fs::remove_file(studio_layout_path_for(folder));
}

/// Render one layout as JSON for the layout API.
pub fn layout_json(load: &LayoutLoad) -> String {
    let raw = serde_json::to_string(&load.layout).unwrap_or_else(|_| "{}".to_string());
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"restored\":{},\"notice\":{},\"layout\":{}}}",
        load.restored, notice, raw
    )
}

/// Escape one string for JSON.
pub fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Escape one string for HTML text.
fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One deep-link request from the Studio URL (Sprint 16, T001): the
/// project, composition, time, and selection the link carries. Times
/// ride as `time` seconds or `frame` frames; selection rides as a
/// comma-separated `selection` list of clip ids or `#index` entries.
#[derive(Debug, Clone, Default)]
pub struct DeepLinkRequest {
    /// Project key the link names, when present.
    pub project: Option<String>,
    /// Composition the link names, when present.
    pub composition: Option<String>,
    /// Time in seconds the link names, when it parses.
    pub time_secs: Option<f64>,
    /// Time parse failure, reported as a banner instead of refusing.
    pub bad_time: Option<String>,
    /// Frame the link names, when it parses.
    pub frame: Option<u64>,
    /// Frame parse failure, reported as a banner instead of refusing.
    pub bad_frame: Option<String>,
    /// Selection entries the link carries, in order.
    pub selection: Vec<String>,
}

/// Parse the Studio query string into a deep-link request. Unknown keys
/// are ignored so host typos never break the shell; unparsable times
/// and frames ride along as banner notes instead of refusing.
pub fn parse_deep_link(query: &str) -> DeepLinkRequest {
    let mut req = DeepLinkRequest::default();
    for (key, value) in super::server::query_pairs_pub(query) {
        match key.as_str() {
            "project" if !value.is_empty() => req.project = Some(value),
            "composition" if !value.is_empty() => req.composition = Some(value),
            "time" if !value.is_empty() => match value.parse::<f64>() {
                Ok(t) => req.time_secs = Some(t),
                Err(_) => req.bad_time = Some(value),
            },
            "frame" if !value.is_empty() => match value.parse::<u64>() {
                Ok(f) => req.frame = Some(f),
                Err(_) => req.bad_frame = Some(value),
            },
            "selection" if !value.is_empty() => {
                req.selection = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    req
}

/// What one deep link resolved to (Sprint 16, T001): the composition,
/// frame, time, and selection the shell opens, plus every fallback
/// banner the resolution raised.
#[derive(Debug, Clone)]
pub struct DeepLinkResolution {
    /// Composition the shell shows (the project root on fallback).
    pub composition: String,
    /// Frame the shell parks on.
    pub frame: u64,
    /// Time in seconds behind the frame.
    pub time_secs: f64,
    /// Selection entries the shell highlights, in link order.
    pub selection: Vec<String>,
    /// Fallback banners to show, in resolution order.
    pub notices: Vec<String>,
}

/// Resolve one deep link against the serving session (Sprint 16, T001 /
/// REC-003): a missing composition lands on the project root with a
/// banner; a link naming another project stays on the served project
/// with a banner; out-of-range times and frames clamp with a banner;
/// unknown selection entries drop with a banner. Nothing refuses: the
/// shell always opens on something real.
#[allow(clippy::too_many_arguments)]
pub fn resolve_deep_link(
    req: &DeepLinkRequest,
    served_project: &str,
    session_composition: &str,
    available: &[String],
    frame_count: u64,
    fps_value: f64,
    duration_secs: f64,
    known_selection: &[String],
) -> DeepLinkResolution {
    let mut notices = Vec::new();
    if let Some(project) = &req.project {
        if project != served_project {
            notices.push(format!(
                "studio: the deep link names project {project:?} but this server hosts {served_project:?}; staying on {served_project:?}."
            ));
        }
    }
    let composition = session_composition.to_string();
    if let Some(want) = &req.composition {
        if available.iter().any(|c| c == want) {
            if want != session_composition {
                notices.push(format!(
                    "studio: this server previews {session_composition:?}; the deep link names {want:?}, so the shell stays on {session_composition:?}."
                ));
            }
        } else {
            notices.push(format!(
                "studio: the deep link names unknown composition {want:?}: the project holds {}; showing the project root {session_composition:?} instead.",
                available
                    .iter()
                    .map(|n| format!("{n:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let last = frame_count.saturating_sub(1);
    let mut frame = 0u64;
    if let Some(bad) = &req.bad_time {
        notices.push(format!(
            "studio: the deep link time {bad:?} is not seconds; parking on frame 0."
        ));
    }
    if let Some(bad) = &req.bad_frame {
        notices.push(format!(
            "studio: the deep link frame {bad:?} is not a frame index; parking on frame 0."
        ));
    }
    if let Some(t) = req.time_secs {
        if !t.is_finite() || t < 0.0 {
            notices.push(format!(
                "studio: the deep link time {t} is outside the timeline (0..={duration_secs:.3}s); parking on frame 0."
            ));
            frame = 0;
        } else if t >= duration_secs && frame_count > 0 {
            frame = last;
            notices.push(format!(
                "studio: the deep link time {t}s runs past the composition end ({duration_secs:.3}s); parking on frame {last}."
            ));
        } else if frame_count > 0 && fps_value > 0.0 {
            frame = ((t * fps_value).floor() as u64).min(last);
        }
    } else if let Some(f) = req.frame {
        if frame_count == 0 || f >= frame_count {
            frame = last;
            notices.push(format!(
                "studio: the deep link frame {f} is outside the timeline ({frame_count} frame(s)); parking on frame {last}."
            ));
        } else {
            frame = f;
        }
    }
    let time_secs = if frame_count > 0 && fps_value > 0.0 {
        frame as f64 / fps_value
    } else {
        0.0
    };
    let mut selection = Vec::new();
    for entry in &req.selection {
        if known_selection.iter().any(|k| k == entry) {
            selection.push(entry.clone());
        } else {
            notices.push(format!(
                "studio: the deep link selection {entry:?} names no clip in {composition:?}; dropping it."
            ));
        }
    }
    DeepLinkResolution {
        composition,
        frame,
        time_secs,
        selection,
        notices,
    }
}

/// One timeline row for the Studio panel (Sprint 16, T001): identity,
/// lane, window, and the file seconds behind the window open —
/// `trim_start` on footage-style clips (`video`, `audio`, `nest`), 0 on
/// every other kind. A readout of the validated composition; it never
/// carries separate state.
#[derive(Debug, Clone)]
pub struct StudioRow {
    /// Document index of the clip.
    pub index: usize,
    /// Element name (`text`, `video`, `box`, ...).
    pub kind: &'static str,
    /// Stable `id`, when declared.
    pub id: Option<String>,
    /// Editor note, when declared.
    pub label: Option<String>,
    /// Display lane name.
    pub track: String,
    /// True while the clip skips picture and sound.
    pub hidden: bool,
    /// Film seconds where the window opens.
    pub start: f64,
    /// Film seconds where the window closes.
    pub end: f64,
    /// File seconds shown at the window open.
    pub media_start: f64,
}

/// Read every timeline row of one composition, in document order.
pub fn studio_rows_of(comp: &notation::model::Composition) -> Vec<StudioRow> {
    comp.clips
        .iter()
        .enumerate()
        .map(|(index, clip)| {
            let (start, end) = (clip.start(), clip.start() + clip.duration());
            StudioRow {
                index,
                kind: clip.kind(),
                id: clip.id().map(|s| s.to_string()),
                label: clip.label().map(|s| s.to_string()),
                track: clip.track().name().to_string(),
                hidden: clip.hidden(),
                start,
                end,
                media_start: media_start_of(clip),
            }
        })
        .collect()
}

/// Selection keys of one composition: stable ids plus `#index`
/// entries, so deep links resolve both spellings.
pub fn selection_keys_of(comp: &notation::model::Composition) -> Vec<String> {
    let mut keys = Vec::new();
    for (index, clip) in comp.clips.iter().enumerate() {
        keys.push(format!("#{index}"));
        if let Some(id) = clip.id() {
            keys.push(id.to_string());
        }
    }
    keys
}

/// File seconds shown at the clip's window open: `trim_start` on
/// footage-style clips, 0 on every other kind.
fn media_start_of(clip: &notation::model::Clip) -> f64 {
    match clip {
        notation::model::Clip::Video(c) => c.trim_start,
        notation::model::Clip::Audio(c) => c.trim_start,
        notation::model::Clip::Nest(c) => c.trim_start,
        _ => 0.0,
    }
}

/// Render timeline rows as JSON for the Studio timeline API.
pub fn timeline_rows_json(comp: &notation::model::Composition) -> String {
    let mut clips = String::new();
    for row in studio_rows_of(comp) {
        if !clips.is_empty() {
            clips.push(',');
        }
        let id = match &row.id {
            Some(id) => format!("\"{}\"", json_escape(id)),
            None => "null".to_string(),
        };
        let label = match &row.label {
            Some(label) => format!("\"{}\"", json_escape(label)),
            None => "null".to_string(),
        };
        clips.push_str(&format!(
            "{{\"index\":{},\"kind\":\"{}\",\"id\":{},\"label\":{},\"track\":\"{}\",\"hidden\":{},\"window\":[{:.3},{:.3}],\"media_start\":{:.3}}}",
            row.index,
            json_escape(row.kind),
            id,
            label,
            json_escape(&row.track),
            row.hidden,
            row.start,
            row.end,
            row.media_start
        ));
    }
    format!(
        "{{\"composition\":\"{}\",\"clips\":[{}]}}",
        json_escape(&comp.name),
        clips
    )
}

/// Light or dark Studio theme (Sprint 16, T002 / CAP-002): one
/// value paints shell and panels together through hyphenated CSS
/// custom properties (`--background-color`, `--panel-color`), so
/// switching the theme restyles every panel at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioTheme {
    /// Default light theme.
    Light,
    /// Dark theme.
    Dark,
}

impl StudioTheme {
    /// The persisted and served spelling (`light` or `dark`).
    pub fn as_str(self) -> &'static str {
        match self {
            StudioTheme::Light => "light",
            StudioTheme::Dark => "dark",
        }
    }

    /// Parse one theme spelling; anything else refuses so a typo never
    /// silently paints the wrong theme.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "light" => Some(StudioTheme::Light),
            "dark" => Some(StudioTheme::Dark),
            _ => None,
        }
    }

    /// The opposite theme, for the shell toggle.
    pub fn other(self) -> Self {
        match self {
            StudioTheme::Light => StudioTheme::Dark,
            StudioTheme::Dark => StudioTheme::Light,
        }
    }
}

/// Theme file name inside the hidden project directory (Sprint 16,
/// T002): next to the layout file, never beside the sources.
pub const STUDIO_THEME_FILE: &str = "studio-theme.json";

/// Version of the persisted Studio theme.
pub const STUDIO_THEME_VERSION: u32 = 1;

/// Hidden theme path for one project folder.
pub fn studio_theme_path_for(folder: &Path) -> PathBuf {
    folder.join(HIDDEN_DIR).join(STUDIO_THEME_FILE)
}

/// Why a theme save refused.
fn refused_theme(message: String) -> String {
    format!("studio refused: {message}")
}

/// What loading the theme reported (Sprint 16, T002): the theme to
/// paint, whether it came from the project file, and the fallback
/// banner when it did not.
#[derive(Debug, Clone)]
pub struct ThemeLoad {
    /// The theme to paint (the saved one, or light).
    pub theme: StudioTheme,
    /// True when the theme came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file exists but
    /// cannot restore.
    pub notice: Option<String>,
}

/// Load the per-project theme (Sprint 16, T002): a missing file paints
/// light with no banner; a corrupt, versioned-wrong, or invalid file
/// paints light and says so.
pub fn load_theme(folder: &Path) -> ThemeLoad {
    let path = studio_theme_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ThemeLoad {
                theme: StudioTheme::Light,
                restored: false,
                notice: None,
            };
        }
        Err(e) => {
            return ThemeLoad {
                theme: StudioTheme::Light,
                restored: false,
                notice: Some(format!(
                    "studio: the saved theme {} cannot be read ({e}); painting the light theme instead.",
                    path.display()
                )),
            };
        }
        Ok(raw) => raw,
    };
    match parse_theme_body(&raw) {
        Ok(theme) => ThemeLoad {
            theme,
            restored: true,
            notice: None,
        },
        Err(why) => ThemeLoad {
            theme: StudioTheme::Light,
            restored: false,
            notice: Some(format!(
                "studio: the saved theme {} cannot restore ({why}); painting the light theme instead.",
                path.display()
            )),
        },
    }
}

/// Parse one theme request body (`{"theme":"light"}` or
/// `{"theme":"dark"}`); anything else refuses naming the two values.
pub fn parse_theme_body(body: &str) -> Result<StudioTheme, String> {
    if body.trim().is_empty() {
        return Err(refused_theme(
            "the theme save needs a JSON theme body (`{\"theme\":\"light\"}` or `{\"theme\":\"dark\"}`); pass the theme from the shell. Nothing was saved.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_theme(format!(
            "the theme body is not JSON ({e}). Pass `{{\"theme\":\"light\"}}` or `{{\"theme\":\"dark\"}}`. Nothing was saved."
        ))
    })?;
    let raw = value.get("theme").and_then(|t| t.as_str()).unwrap_or("");
    StudioTheme::parse(raw).ok_or_else(|| {
        refused_theme(format!(
            "unknown theme {raw:?}: the shell paints `light` or `dark`. Pass one of those values. Nothing was saved."
        ))
    })
}

/// Save one theme for the project (Sprint 16, T002): parses and
/// validates the body first, then writes the hidden theme file — so a
/// refused body leaves the previous theme intact.
pub fn save_theme(folder: &Path, body: &str) -> Result<StudioTheme, String> {
    let theme = parse_theme_body(body)?;
    let path = studio_theme_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_theme(format!(
                "the hidden theme directory {} cannot be prepared: {e}. Nothing was saved.",
                parent.display()
            ))
        })?;
    }
    let raw = format!(
        "{{\"version\":{},\"theme\":\"{}\"}}",
        STUDIO_THEME_VERSION,
        theme.as_str()
    );
    std::fs::write(&path, raw).map_err(|e| {
        refused_theme(format!(
            "the theme file {} cannot be written: {e}. Nothing was saved.",
            path.display()
        ))
    })?;
    Ok(theme)
}

/// Render one theme load as JSON for the theme API.
pub fn theme_json(load: &ThemeLoad) -> String {
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"theme\":\"{}\",\"restored\":{},\"notice\":{}}}",
        load.theme.as_str(),
        load.restored,
        notice
    )
}

/// Whether Studio reads the project from disk (Sprint 16, T002 /
/// REC-003): a moved or deleted folder opens the shell read-only with
/// every staged edit kept, never refusing the page.
pub fn project_reachable(folder: &Path) -> bool {
    folder.is_dir()
}

/// Banner naming an unreachable project: the shell stays open and
/// read-only with every staged edit kept.
pub fn unreachable_notice(folder: &Path) -> String {
    format!(
        "studio: the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder or reopen. Nothing was applied or recorded.",
        folder.display()
    )
}

/// Where an autosaved change comes from (Sprint 16, T002): visual
/// gestures save immediately, code saves debounced — both land
/// through the same validated turn, and the page carries no save
/// button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveSource {
    /// A visual gesture: saved immediately.
    Visual,
    /// A code change: saved debounced.
    Code,
}

impl SaveSource {
    /// The served spelling (`visual` or `code`).
    pub fn as_str(self) -> &'static str {
        match self {
            SaveSource::Visual => "visual",
            SaveSource::Code => "code",
        }
    }

    /// Parse one source spelling; anything else refuses so a typo never
    /// silently changes the save speed.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "visual" => Some(SaveSource::Visual),
            "code" => Some(SaveSource::Code),
            _ => None,
        }
    }
}

/// One autosave request (Sprint 16, T002): the named actor plus the
/// patch operations to apply through the SDK machinery.
#[derive(Debug, Clone)]
pub struct StudioEdit {
    /// Actor the validated turn is recorded under (human or agent).
    pub actor: String,
    /// Whether the change saves immediately (`visual`) or debounced (`code`).
    pub source: SaveSource,
    /// Patch operations to apply, in order.
    pub ops: Vec<sdk::EditOp>,
}

/// Parse one autosave request body
/// (`{"actor":"maker","source":"visual","edits":[...]}`): the actor
/// must be non-empty, the source names `visual` or `code`
/// (`visual` when omitted), and at least one patch operation must ride
/// along. A refused body names its cause and stages nothing.
pub fn parse_edit_body(body: &str) -> Result<StudioEdit, String> {
    if body.trim().is_empty() {
        return Err(refused_theme(
            "the studio save needs a JSON edit body (`{\"actor\":\"maker\",\"edits\":[...]}`); pass the staged change from the shell. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_theme(format!(
            "the studio save body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"edits\":[...]}}`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_theme(
            "the studio save names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let source_raw = value
        .get("source")
        .and_then(|s| s.as_str())
        .unwrap_or("visual");
    let source = SaveSource::parse(source_raw).ok_or_else(|| {
        refused_theme(format!(
            "unknown save source {source_raw:?}: visual gestures save immediately, code saves debounced. Pass `visual` or `code`. Nothing was staged."
        ))
    })?;
    let edits = value.get("edits").ok_or_else(|| {
        refused_theme(
            "the studio save carries no `edits` array; pass the staged patch operations. Nothing was staged.".to_string(),
        )
    })?;
    let ops: Vec<sdk::EditOp> = serde_json::from_value(edits.clone()).map_err(|e| {
        refused_theme(format!(
            "the studio save `edits` are not patch operations ({e}); pass SDK patch operations. Nothing was staged."
        ))
    })?;
    if ops.is_empty() {
        return Err(refused_theme(
            "the studio save carries an empty `edits` array; pass at least one patch operation. Nothing was staged.".to_string(),
        ));
    }
    Ok(StudioEdit { actor, source, ops })
}

/// Receipt of one applied studio save (Sprint 16, T002): what the
/// validated turn recorded, so the shell reads back the save state.
#[derive(Debug, Clone)]
pub struct StudioEditReceipt {
    /// Patch operations applied by this save.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Render one applied receipt as JSON for the edit API.
pub fn edit_receipt_json(receipt: &StudioEditReceipt) -> String {
    format!(
        "{{\"saved\":true,\"applied\":{},\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}]}}",
        receipt.applied,
        receipt.turn,
        json_escape(&receipt.actor),
        receipt
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Apply one studio save through the Sprint 15 patch and history
/// machinery (Sprint 16, T002 / CAP-002): every operation applies to a
/// fresh SDK session over the project, then one validated turn records
/// the save under the actor name — the same undo system as headless
/// edits. A refused operation or an unwritable turn refuses with its
/// reason code while the composition, the delta, and history stay
/// exactly as they were.
pub fn apply_studio_edit(
    folder: &Path,
    composition: Option<&str>,
    edit: &StudioEdit,
) -> Result<StudioEditReceipt, String> {
    if !project_reachable(folder) {
        return Err(refused_theme(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let mut session = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused_theme(format!(
            "the project cannot open for saving: {e} Nothing was applied or recorded."
        ))
    })?;
    let mut summaries = Vec::new();
    for op in &edit.ops {
        match session.apply(op.clone()) {
            Ok(applied) => summaries.push(applied.op.summary()),
            Err(e) => {
                return Err(refused_theme(format!(
                    "{e} Nothing was persisted or recorded."
                )));
            }
        }
    }
    match session.persist_turn(&edit.actor) {
        Ok(receipt) => Ok(StudioEditReceipt {
            applied: receipt.applied,
            turn: receipt.turn.unwrap_or(0),
            actor: receipt.actor.unwrap_or_else(|| edit.actor.clone()),
            summaries,
        }),
        Err(e) => Err(refused_theme(format!(
            "{e} Staged work stays preserved unapplied — nothing was persisted or recorded."
        ))),
    }
}

/// One edit waiting in the save queue (Sprint 16, T002): the actor and
/// source the staged patch arrived with, so resuming replays every
/// staged edit in arrival order.
#[derive(Debug, Clone)]
pub struct QueuedEdit {
    /// Actor the drained turn replays under (first staged actor wins).
    pub actor: String,
    /// Whether the change arrived immediate (`visual`) or debounced (`code`).
    pub source: SaveSource,
    /// The staged patch operation.
    pub op: sdk::EditOp,
}

/// The studio save queue (Sprint 16, T002): while paused, saves stage
/// here with nothing applied; resuming drains every staged edit
/// through [`apply_studio_edit`] in arrival order. A failed drain
/// keeps every staged edit intact.
#[derive(Debug, Default)]
pub struct SaveQueue {
    /// True while saves stage instead of applying.
    pub paused: bool,
    /// Staged patch operations, in arrival order.
    pub staged: Vec<QueuedEdit>,
}

impl SaveQueue {
    /// Pause or resume the queue.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Stage one parsed save without applying it; returns the staged
    /// depth after staging.
    pub fn stage(&mut self, edit: StudioEdit) -> usize {
        for op in edit.ops {
            self.staged.push(QueuedEdit {
                actor: edit.actor.clone(),
                source: edit.source,
                op,
            });
        }
        self.staged.len()
    }

    /// One-line summary per staged patch, in arrival order.
    pub fn summaries(&self) -> Vec<String> {
        self.staged.iter().map(|q| q.op.summary()).collect()
    }

    /// Drain every staged edit through the SDK patch and history
    /// machinery, oldest first, recording one validated turn under the
    /// first staged actor. `composition` selects the session the drain
    /// replays on. `None` while the queue holds nothing; a failed drain
    /// keeps every staged edit intact.
    pub fn drain(
        &mut self,
        folder: &Path,
        composition: Option<&str>,
    ) -> Result<Option<StudioEditReceipt>, String> {
        if self.staged.is_empty() {
            return Ok(None);
        }
        let actor = self.staged[0].actor.clone();
        let ops: Vec<sdk::EditOp> = self.staged.iter().map(|q| q.op.clone()).collect();
        let edit = StudioEdit {
            actor,
            source: self.staged[0].source,
            ops,
        };
        match apply_studio_edit(folder, composition, &edit) {
            Ok(receipt) => {
                self.staged.clear();
                Ok(Some(receipt))
            }
            Err(why) => Err(why),
        }
    }
}

/// Render one save-state snapshot as JSON for the save-state API:
/// the pause switch, the staged depth, the staged summaries, and
/// whether the project reads read-only.
pub fn save_state_json(queue: &SaveQueue, read_only: bool, extra_notice: Option<&str>) -> String {
    let edits = queue
        .summaries()
        .iter()
        .map(|s| format!("\"{}\"", json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let notice = match extra_notice {
        Some(n) => format!("\"{}\"", json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"paused\":{},\"staged\":{},\"edits\":[{}],\"read_only\":{},\"notice\":{}}}",
        queue.paused,
        queue.staged.len(),
        edits,
        read_only,
        notice
    )
}

/// The composition Studio readouts show (Sprint 16, T002): the SDK
/// effective composition with the persisted edit delta replayed, so an
/// autosaved turn updates rows, keys, and labels at once. Opens
/// read-only (`persist: false`, `history: false`), so this probe never
/// writes — not the history directory, not the delta. Falls back to
/// the serving session copy while the delta cannot replay.
pub fn effective_composition_for(
    folder: &Path,
    session_comp: &notation::model::Composition,
    want: Option<&str>,
) -> notation::model::Composition {
    if !project_reachable(folder) {
        return session_comp.clone();
    }
    match sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            composition: want.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    ) {
        Ok(session) => session.effective_composition().clone(),
        Err(_) => session_comp.clone(),
    }
}

/// Model behind the served Studio page: everything the shell needs
/// without touching server internals, so tests build it directly.
#[derive(Debug, Clone)]
pub struct StudioPageModel {
    /// Project directory key shown in the title.
    pub project: String,
    /// Composition the shell shows (the project root on fallback).
    pub composition: String,
    /// Every composition name in the project, for deep links.
    pub all_compositions: Vec<String>,
    /// Frame the shell parks on.
    pub frame: u64,
    /// Output frame count of the shown composition.
    pub frame_count: u64,
    /// Composition length in seconds.
    pub duration_secs: f64,
    /// Fps value behind the clock.
    pub fps_value: f64,
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Selection entries the shell highlights, in link order.
    pub selection: Vec<String>,
    /// Arrangement the shell paints.
    pub layout: StudioLayout,
    /// True when the arrangement came from the project file.
    pub layout_restored: bool,
    /// Banners to show (layout fallback plus deep-link fallbacks).
    pub notices: Vec<String>,
    /// Timeline rows JSON of the shown composition.
    pub rows_json: String,
    /// Theme the shell paints (`light` or `dark`).
    pub theme: String,
    /// True while the project is unreachable: the shell reads read-only.
    pub read_only: bool,
    /// True while the save queue stages instead of applying.
    pub save_paused: bool,
    /// Staged patch depth in the save queue.
    pub staged: usize,
    /// One-line summary per staged patch, in arrival order.
    pub staged_summaries: Vec<String>,
    /// Conflict banner naming the disk version against the unsaved
    /// Studio version (`None` while clean, Sprint 16 T003).
    pub conflict_notice: Option<String>,
}

/// The Studio shell page (Sprint 16, T002): preview and timeline panels
/// in one dockable shell grown from the preview and player pages. The
/// preview panel shows true frames with transport and the time readout;
/// the timeline panel shows the validated rows with ruler seek and
/// multi-select; the toolbar docks, drags (via paint order), groups,
/// floats, maximises, closes, and resets panels; the banner names every
/// fallback; the URL carries project, composition, time, and selection.
/// Light and dark themes paint shell and panels together through
/// hyphenated CSS custom properties; visual edits autosave immediately
/// and code saves debounced with no save button, every save landing as
/// a validated history turn; banners and toasts report the save queue,
/// unreachable projects, missing compositions, and errors; a paused
/// queue banners and keeps every edit staged; an unreachable project
/// opens the shell read-only. Style attributes use hyphenated CSS
/// vocabulary (`background-color`, `border-radius`,
/// `font-variant-numeric`, `object-fit`, `--background-color`);
/// `font-family` stays a generic stack, never resolved.
pub fn studio_page_html(model: &StudioPageModel) -> String {
    let banner = if model.notices.is_empty() {
        "<p id=\"banner\" hidden></p>".to_string()
    } else {
        format!(
            "<p id=\"banner\" role=\"alert\">{}</p>",
            html_escape(&model.notices.join(" "))
        )
    };
    // The conflict banner (Sprint 16, T003): disk versus unsaved with
    // the maker's two picks, server-rendered so the shell names both
    // versions even before the page script polls.
    let conflict = match &model.conflict_notice {
        None => "<p id=\"conflict-banner\" hidden></p><div id=\"conflict-picks\" hidden></div>"
            .to_string(),
        Some(notice) => format!(
            "<p id=\"conflict-banner\" role=\"alert\">{}</p><div id=\"conflict-picks\"><button id=\"pick-disk\" type=\"button\">keep disk version</button><button id=\"pick-unsaved\" type=\"button\">keep unsaved version</button></div>",
            html_escape(notice)
        ),
    };
    let boot_layout = serde_json::to_string(&model.layout).unwrap_or_else(|_| "{}".to_string());
    let boot_staged = model
        .staged_summaries
        .iter()
        .map(|s| format!("\"{}\"", json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let boot = format!(
        "{{\"project\":\"{}\",\"composition\":\"{}\",\"frame\":{},\"timeSecs\":{:.3},\"selection\":[{}],\"layoutRestored\":{},\"layout\":{},\"theme\":\"{}\",\"readOnly\":{},\"saveState\":{{\"paused\":{},\"stagedEdits\":[{}]}}}}",
        json_escape(&model.project),
        json_escape(&model.composition),
        model.frame,
        if model.fps_value > 0.0 {
            model.frame as f64 / model.fps_value
        } else {
            0.0
        },
        model
            .selection
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(","),
        model.layout_restored,
        boot_layout,
        json_escape(&model.theme),
        model.read_only,
        model.save_paused,
        boot_staged
    );
    let max = model.frame_count.saturating_sub(1);
    let selection_text = if model.selection.is_empty() {
        "none".to_string()
    } else {
        model.selection.join(", ")
    };
    // Sprint 16, T005 owns the snap/zoom page slots wired below.
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Framesmith studio — {project} — {composition}</title>\n\
        <style>body{{margin:0;background-color:#000;font-family:sans-serif}}\
        #studio{{display:flex;flex-direction:column;min-height:100vh;background-color:var(--background-color);color:var(--ink-color)}}\
        #studio[data-theme=\"light\"]{{--background-color:#f4f4f6;--panel-color:#ffffff;--ink-color:#1a1a20;--line-color:#d5d5dc;--banner-background:#fff3c4;--banner-color:#5a4a10;--accent-color:#dce4f2;--toast-background:#1a1a20;--toast-color:#f4f4f6}}\
        #studio[data-theme=\"dark\"]{{--background-color:#14141a;--panel-color:#1d1d26;--ink-color:#eee;--line-color:#333;--banner-background:#3a2f10;--banner-color:#ffd970;--accent-color:#2c3a55;--toast-background:#eee;--toast-color:#14141a}}\
        header{{padding:8px 16px;background-color:var(--panel-color);border-bottom:1px solid var(--line-color);border-radius:0}}\
        #banner,#queue-banner,#conflict-banner{{background-color:var(--banner-background);color:var(--banner-color);border:1px solid var(--line-color);border-radius:4px;padding:8px 12px;margin:8px 16px}}\
        #conflict-picks{{margin:0 16px 8px 16px}}\
        #conflict-picks button{{margin-right:8px}}\
        #toasts{{position:fixed;right:16px;bottom:16px;display:flex;flex-direction:column;gap:8px}}\
        #toasts .toast{{background-color:var(--toast-background);color:var(--toast-color);border-radius:4px;padding:8px 12px;max-width:320px}}\
        #panels{{display:flex;flex-direction:row;flex:1;gap:8px;padding:8px 16px}}\
        section.panel{{background-color:var(--panel-color);border:1px solid var(--line-color);border-radius:8px;padding:12px;min-width:0;color:var(--ink-color)}}\
        section.panel[data-dock=\"main\"]{{flex:3}}\
        section.panel[data-dock=\"bottom\"],section.panel[data-dock=\"left\"],section.panel[data-dock=\"right\"]{{flex:1}}\
        section.panel[data-hidden=\"1\"]{{display:none}}\
        section.panel[data-max=\"1\"]{{flex:1 1 100%}}\
        #frame{{max-width:100%;background-color:#000;object-fit:contain;display:block}}\
        #readout,#time-readout,#save-state{{font-variant-numeric:tabular-nums}}\
        .dock-bar button{{margin:2px}}\
        #timeline-rows{{width:100%;border-collapse:collapse}}\
        #timeline-rows td,#timeline-rows th{{border:1px solid var(--line-color);padding:2px 6px;font-variant-numeric:tabular-nums}}\
        tr[aria-selected=\"true\"]{{background-color:var(--accent-color)}}\n        #lanes,#beat-strip,#clip-visuals{{margin-top:12px}}\n        .lane{{margin-bottom:8px}}\n        .lane h4{{margin:4px 0;font-size:12px}}\n        .lane-track{{position:relative;height:28px;background-color:var(--panel-color);border:1px solid var(--line-color);border-radius:4px}}\n        button.diamond{{position:absolute;top:2px;width:20px;height:20px;padding:0;background-color:var(--panel-color);color:var(--ink-color);border:1px solid var(--line-color);border-radius:4px;transform:rotate(45deg);cursor:grab}}\n        button.diamond[data-selected=\"1\"]{{background-color:var(--accent-color)}}\n        .lane-menu button,.beat-row button{{margin:2px}}\n        .beat-row{{display:flex;gap:8px;align-items:baseline;font-variant-numeric:tabular-nums}}\n        .filmstrip{{display:flex;gap:2px;overflow-x:auto}}\n        .filmstrip img{{height:36px;background-color:#000;object-fit:contain;display:block}}\n        .wave{{display:flex;gap:1px;align-items:flex-end;height:48px}}\n        .wave span{{width:4px;background-color:var(--line-color)}}\n        .wave span[data-peak=\"1\"]{{background-color:var(--accent-color)}}\n        .placeholder-box{{border:1px dashed var(--line-color);border-radius:4px;padding:8px 12px}}\n        .badge{{display:inline-block;border:1px solid var(--line-color);border-radius:4px;padding:0 6px;margin:2px;font-variant-numeric:tabular-nums}}\n        .visual-card{{border-top:1px solid var(--line-color);padding:4px 0}}\n        .visual-card h5{{margin:4px 0;font-variant-numeric:tabular-nums}}{snap_zoom_css}{clips_css}\n        #shortcuts{{margin-top:12px}}\n        table.shortcut-table{{width:100%;border-collapse:collapse}}\n        table.shortcut-table td,table.shortcut-table th{{border:1px solid var(--line-color);padding:2px 6px;font-variant-numeric:tabular-nums}}\
        </style>\n</head>\n<body>\n<div id=\"studio\" data-theme=\"{theme}\" data-read-only=\"{read_only}\">\n<header><h1>studio: {project} — {composition}</h1>\n\
        <p>compositions: {comps}</p><p>theme: {theme}</p></header>\n{banner}\n{queue_banner}\n{conflict}\n<div id=\"toasts\" aria-live=\"polite\"></div>\n\
        <div id=\"toolbar\"><button id=\"reset-layout\" type=\"button\">reset layout</button>\n\
        <button id=\"theme-toggle\" type=\"button\">theme: {theme}</button>\n\
        <button id=\"queue-toggle\" type=\"button\">pause saves</button>\n\
        <span id=\"save-state\">layout {saved} · theme {theme} · {save_wording}</span></div>\n\
        <div id=\"panels\">\n\
        <section class=\"panel\" id=\"panel-preview\" data-panel=\"preview\" aria-label=\"preview panel\">\n\
        <h2>preview</h2>\n<div class=\"dock-bar\" data-panel=\"preview\">\n\
        <button type=\"button\" data-act=\"dock-main\">dock main</button>\n\
        <button type=\"button\" data-act=\"dock-left\">dock left</button>\n\
        <button type=\"button\" data-act=\"dock-right\">dock right</button>\n\
        <button type=\"button\" data-act=\"dock-bottom\">dock bottom</button>\n\
        <button type=\"button\" data-act=\"float\">float</button>\n\
        <button type=\"button\" data-act=\"group\">group</button>\n\
        <button type=\"button\" data-act=\"maximise\">maximise</button>\n\
        <button type=\"button\" data-act=\"close\">close</button>\n</div>\n\
        <p id=\"readout\">frame {frame}/{frame_count}</p>\n\
        <img id=\"frame\" src=\"/api/frame?i={frame}&action=step\" alt=\"studio preview frame\">\n\
        <div><button id=\"play\" type=\"button\">play</button><button id=\"pause\" type=\"button\">pause</button>\n\
        <label>scrub <input id=\"scrub\" type=\"range\" min=\"0\" max=\"{max}\" value=\"{frame}\"></label>\n\
        <label>loop <input id=\"loop\" type=\"checkbox\"></label>\n\
        <label>speed <input id=\"speed\" type=\"number\" value=\"1\" step=\"0.25\" min=\"0.1\"></label></div>\n\
        </section>\n\
        <section class=\"panel\" id=\"panel-timeline\" data-panel=\"timeline\" aria-label=\"timeline panel\">\n\
        <h2>timeline</h2>\n<div class=\"dock-bar\" data-panel=\"timeline\">\n\
        <button type=\"button\" data-act=\"dock-main\">dock main</button>\n\
        <button type=\"button\" data-act=\"dock-left\">dock left</button>\n\
        <button type=\"button\" data-act=\"dock-right\">dock right</button>\n\
        <button type=\"button\" data-act=\"dock-bottom\">dock bottom</button>\n\
        <button type=\"button\" data-act=\"float\">float</button>\n\
        <button type=\"button\" data-act=\"group\">group</button>\n\
        <button type=\"button\" data-act=\"maximise\">maximise</button>\n\
        <button type=\"button\" data-act=\"close\">close</button>\n</div>\n\
        <label>ruler <input id=\"ruler\" type=\"range\" min=\"0\" max=\"{max}\" value=\"{frame}\"></label>\n\
        <p id=\"time-readout\">{time:.3}s/{duration:.3}s</p>\n\
        <p id=\"selection-note\">selection: {selection}</p>\n\
        <div class=\"dock-bar\" id=\"tool-bar\">\n\
        <button type=\"button\" data-tool=\"select\">select</button>\n\
        <button type=\"button\" data-tool=\"razor\">razor</button>\n\
        <label>all tracks <input id=\"tool-all-tracks\" type=\"checkbox\"></label>\n\
        <button id=\"tool-split-playhead\" type=\"button\">split at playhead</button>\n\
        <button id=\"tool-select-before\" type=\"button\">select before</button>\n\
        <button id=\"tool-select-after\" type=\"button\">select after</button>\n\
        <label>in <input id=\"work-in\" type=\"number\" value=\"\" step=\"0.1\" min=\"0\"></label>\n\
        <label>out <input id=\"work-out\" type=\"number\" value=\"\" step=\"0.1\" min=\"0\"></label>\n\
        <button id=\"work-set\" type=\"button\">set work area</button>\n\
        <button id=\"work-clear\" type=\"button\">clear work area</button>\n\
        </div>\n\
        <p id=\"tool-status\" role=\"status\">tools ready</p>\n        <div id=\"shortcuts\"><h3>shortcuts</h3><div id=\"shortcut-tables\"><p>loading shortcuts…</p></div><div><button id=\"describe-copy\" type=\"button\">copy agent request</button></div><p id=\"shortcut-status\" role=\"status\">shortcuts ready</p></div>\n\
        <table id=\"timeline-rows\"><thead><tr><th>clip</th><th>track</th><th>window</th><th>media-start</th></tr></thead>\n<tbody id=\"timeline-body\"></tbody></table>{snap_zoom_html}{clips_html}\n        <div id=\"lanes\"><h3>keyframes</h3><div id=\"lane-list\"></div><div class=\"lane-menu\" id=\"lane-menu\" hidden><span id=\"lane-menu-label\"></span><button type=\"button\" data-kact=\"retime\">move to playhead</button><button type=\"button\" data-kact=\"set-ease\">edit ease</button><button type=\"button\" data-kact=\"copy\">copy</button><button type=\"button\" data-kact=\"delete\">delete</button></div></div>\n        <div id=\"beat-strip\"><h3>beats</h3><div id=\"beat-list\"></div><div><label>name <input id=\"beat-name\" type=\"text\"></label><label>at <input id=\"beat-at\" type=\"number\" step=\"0.1\" min=\"0\"></label><button id=\"beat-add\" type=\"button\">add beat</button><button id=\"beat-undo\" type=\"button\">undo beat</button></div></div>\n        <div id=\"clip-visuals\"><h3>clip visuals</h3><div id=\"visual-list\"></div></div>\n\
        </section>\n</div>\n</div>\n\
        <script id=\"studio-boot\" type=\"application/json\">{boot}</script>\n\
        <script id=\"studio-rows\" type=\"application/json\">{rows}</script>\n\
        <script>\nlet boot = JSON.parse(document.getElementById('studio-boot').textContent);\n\
        let rows = JSON.parse(document.getElementById('studio-rows').textContent);\n\
        let layout = boot.layout;\nlet selection = boot.selection.slice();\nlet cur = boot.frame;\n\
        function banner(text) {{ const b = document.getElementById('banner'); b.hidden = false; b.textContent = text; }}\n\
        function toast(text) {{ const box = document.getElementById('toasts'); if (!box) return; const p = document.createElement('p'); p.className = 'toast'; p.textContent = text; box.appendChild(p); while (box.children.length > 4) box.removeChild(box.firstChild); }}\n\
        function paintSaveState(text) {{ const s = document.getElementById('save-state'); if (s) s.textContent = text; }}\n\
        function paintQueue() {{\n          const q = document.getElementById('queue-banner');\n          if (!q) return;\n          const paused = !!(boot.saveState && boot.saveState.paused);\n          const staged = (boot.saveState && boot.saveState.stagedEdits) || [];\n          if (!paused && staged.length === 0) {{ q.hidden = true; q.textContent = ''; return; }}\n          q.hidden = false;\n          q.textContent = paused\n            ? ('save queue paused with ' + staged.length + ' staged edit(s): ' + staged.join('; ') + ' — resume to apply.')\n            : (staged.length + ' staged edit(s) waiting: ' + staged.join('; '));\n          const qt = document.getElementById('queue-toggle');\n          if (qt) qt.textContent = paused ? 'resume saves' : 'pause saves';\n        }}\n\
        async function refreshSaveState() {{\n          try {{\n            const s = await (await fetch('/api/studio/save-state')).json();\n            boot.saveState = {{paused: s.paused, stagedEdits: s.edits || []}};\n            paintQueue();\n          }} catch (e) {{}}\n        }}\n\
        async function postEdit(source, actor, edits) {{\n          paintSaveState('saving: ' + edits.length + ' edit(s) (' + source + ')');\n          try {{\n            const r = await fetch('/api/studio/edit', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify({{actor: actor, source: source, edits: edits}})}});\n            const t = await r.text();\n            if (!r.ok) {{ paintSaveState('save refused'); banner(t); toast(t); return; }}\n            const receipt = JSON.parse(t);\n            if (!receipt.saved) {{\n              paintSaveState('staged ' + receipt.staged + ' edit(s)');\n              toast('staged ' + receipt.staged + ' edit(s) while ' + (receipt.read_only ? 'read-only' : 'paused'));\n            }} else {{\n              paintSaveState('saved turn ' + receipt.turn);\n              toast('saved turn ' + receipt.turn + ': ' + (receipt.summaries || []).join('; '));\n            }}\n            refreshSaveState();\n          }} catch (e) {{ paintSaveState('save failed: ' + e); toast('save failed: ' + e); }}\n        }}\n\
        function autosaveVisual(actor, edits) {{ postEdit('visual', actor, edits); }}\n\
        let codeTimer = null;\n\
        function autosaveCode(actor, edits) {{ if (codeTimer) clearTimeout(codeTimer); codeTimer = setTimeout(() => postEdit('code', actor, edits), 800); }}\n\
        function paintLayout() {{\n          const panels = document.getElementById('panels');\n          const order = (layout.order || ['preview', 'timeline']).slice();\n          for (const id of order) {{\n            const el = document.getElementById('panel-' + id);\n            if (el) panels.appendChild(el);\n          }}\n          for (const id of ['preview', 'timeline']) {{\n            const el = document.getElementById('panel-' + id);\n            const p = (layout.panels || {{}})[id] || {{dock: 'main'}};\n            if (!el) continue;\n            el.dataset.dock = p.dock || 'main';\n            el.dataset.hidden = p.visible === false ? '1' : '0';\n            el.dataset.max = p.maximized ? '1' : '0';\n            el.dataset.group = (p.group || '');\n            if (p.dock === 'float' && p.float_rect) {{\n              el.style.position = 'absolute';\n              el.style.left = p.float_rect[0] + 'px';\n              el.style.top = p.float_rect[1] + 'px';\n              el.style.width = p.float_rect[2] + 'px';\n            }} else {{ el.style.position = ''; el.style.left = ''; el.style.top = ''; el.style.width = ''; }}\n          }}\n        }}\n\
        function paintRows() {{\n          const body = document.getElementById('timeline-body');\n          body.innerHTML = '';\n          for (const c of (rows.clips || [])) {{\n            const tr = document.createElement('tr');\n            const key = c.id || ('#' + c.index);\n            tr.setAttribute('aria-selected', selection.includes(key) || selection.includes('#' + c.index) ? 'true' : 'false');\n            tr.dataset.clip = key;\n            const name = document.createElement('td'); name.textContent = c.kind + ' ' + key;\n            const track = document.createElement('td'); track.textContent = c.track + (c.hidden ? ' hidden' : '');\n            const win = document.createElement('td'); win.textContent = c.window[0].toFixed(3) + 's..' + c.window[1].toFixed(3) + 's';\n            const media = document.createElement('td'); media.textContent = c.media_start.toFixed(3) + 's';\n            tr.append(name, track, win, media);\n            tr.addEventListener('click', (e) => {{\n              const k = tr.dataset.clip;\n              if (e.shiftKey || e.metaKey || e.ctrlKey) {{\n                const i = selection.indexOf(k);\n                if (i >= 0) selection.splice(i, 1); else selection.push(k);\n              }} else if (typeof tool !== 'undefined' && tool === 'razor') {{ callTool({{tool: 'razor', clip: k, 'all-tracks': (typeof allTracks !== 'undefined' && allTracks)}}); return; }}\n              else {{ selection = [k]; }}\n              paintRows(); syncLink();\n            }});\n            body.appendChild(tr);\n          }}\n          const note = document.getElementById('selection-note');\n          if (note) note.textContent = 'selection: ' + (selection.join(', ') || 'none');\n        }}\n\
        function syncLink() {{\n          const q = new URLSearchParams();\n          q.set('composition', rows.composition || boot.composition);\n          q.set('frame', String(cur));\n          if (selection.length) q.set('selection', selection.join(','));\n          history.replaceState(null, '', '/studio?' + q.toString());\n        }}\n\
        async function persistLayout() {{\n          const state = document.getElementById('save-state');\n          try {{\n            const r = await fetch('/api/studio/layout', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify({{version: 1, panels: layout.panels, order: layout.order}})}});\n            if (!r.ok) {{ const t = await r.text(); if (state) state.textContent = 'layout save refused: ' + t; banner(t); return; }}\n            if (state) state.textContent = 'layout saved';\n          }} catch (e) {{ if (state) state.textContent = 'layout save failed: ' + e; }}\n        }}\n\
        function dragPanel(el, id) {{\n          el.draggable = true;\n          el.addEventListener('dragstart', (e) => {{ e.dataTransfer.setData('text/studio-panel', id); }});\n        }}\n\
        document.getElementById('panels').addEventListener('dragover', (e) => e.preventDefault());\n\
        document.getElementById('panels').addEventListener('drop', (e) => {{\n          e.preventDefault();\n          const id = e.dataTransfer.getData('text/studio-panel');\n          if (!id) return;\n          const order = (layout.order || ['preview', 'timeline']).filter((x) => x !== id);\n          const panels = Array.from(document.querySelectorAll('#panels .panel'));\n          let at = panels.length;\n          for (let i = 0; i < panels.length; i++) {{\n            const r = panels[i].getBoundingClientRect();\n            if (e.clientX < r.left + r.width / 2) {{ at = i; break; }}\n          }}\n          const ids = panels.map((p) => p.dataset.panel).filter((x) => x !== id);\n          ids.splice(Math.min(at, ids.length), 0, id);\n          layout.order = ids;\n          paintLayout(); persistLayout();\n          void order;\n        }});\n\
        document.querySelectorAll('.dock-bar').forEach((bar) => {{\n          const id = bar.dataset.panel;\n          dragPanel(bar.closest('.panel'), id);\n          bar.addEventListener('click', (e) => {{\n            const act = e.target && e.target.dataset ? e.target.dataset.act : null;\n            if (!act) return;\n            const p = layout.panels[id] || (layout.panels[id] = {{dock: 'main'}});\n            if (act.startsWith('dock-')) {{ p.dock = act.slice(5); p.maximized = false; p.visible = true; p.float_rect = null; p.group = null; }}\n            else if (act === 'float') {{ p.dock = 'float'; p.visible = true; p.maximized = false; p.group = null; p.float_rect = p.float_rect || [40, 40, 480, 320]; }}\n            else if (act === 'group') {{ const other = id === 'preview' ? 'timeline' : 'preview'; p.group = other; p.visible = true; p.maximized = false; }}\n            else if (act === 'maximise') {{ p.maximized = !p.maximized; p.visible = true; }}\n            else if (act === 'close') {{ p.visible = false; p.maximized = false; }}\n            paintLayout(); persistLayout();\n          }});\n        }});\n\
        document.getElementById('reset-layout').addEventListener('click', async () => {{\n          await fetch('/api/studio/reset', {{method: 'POST'}});\n          const r = await (await fetch('/api/studio/layout')).json();\n          layout = r.layout; selection = boot.selection.slice();\n          paintLayout();\n          const state = document.getElementById('save-state');\n          if (state) state.textContent = 'layout reset to default';\n        }});\n\
        async function show(i) {{\n          const r = await fetch('/api/frame?i=' + i + '&action=scrub');\n          if (!r.ok) return;\n          const b = await r.blob();\n          document.getElementById('frame').src = URL.createObjectURL(b);\n          cur = i;\n          const ro = document.getElementById('readout');\n          if (ro) ro.textContent = 'frame ' + i + '/' + rows.frame_count;\n          const ruler = document.getElementById('ruler');\n          if (ruler) ruler.value = i;\n          const scrub = document.getElementById('scrub');\n          if (scrub) scrub.value = i;\n          syncLink();\n        }}\n\
        document.getElementById('scrub').addEventListener('input', (e) => show(Number(e.target.value)));\n\
        let tool = 'select';\n        function toolStatus(t) {{ const s = document.getElementById('tool-status'); if (s) s.textContent = t; }}\n        async function callTool(payload) {{\n          try {{\n            const r = await fetch('/api/studio/tool', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify(payload)}});\n            const t = await r.text();\n            if (!r.ok) {{ toolStatus(t); banner(t); return; }}\n            const j = JSON.parse(t);\n            const rc = j.receipt || j;\n            if (j.rows) rows = j.rows;\n            if (rc.selection && rc.selection.length) selection = rc.selection.slice();\n            const moved = (rc.moved && rc.moved.length) ? (' moving ' + rc.moved.join(', ')) : '';\n            toolStatus(rc.summary + moved + ' (turn ' + rc.turn + ')');\n            paintRows(); if (typeof paintClips === 'function') paintClips(); syncLink(); show(cur);\n          }} catch (e) {{ toolStatus('tool failed: ' + e); }}\n        }}\n        async function seekFrame(i) {{\n          try {{\n            const r = await fetch('/api/studio/seek?frame=' + i, {{method: 'POST'}});\n            const t = await r.text();\n            if (!r.ok) {{ toolStatus(t); banner(t); return; }}\n            const j = JSON.parse(t);\n            if (j.clamped) toolStatus('seek clamped into the work area: frame ' + j.frame);\n            show(j.frame);\n          }} catch (e) {{ toolStatus('seek failed: ' + e); }}\n        }}\n        document.querySelectorAll('#tool-bar [data-tool]').forEach((b) => b.addEventListener('click', () => {{ tool = b.dataset.tool; toolStatus('tool: ' + tool); }}));\n        let allTracks = false;\n        document.getElementById('tool-all-tracks').addEventListener('change', (e) => {{ allTracks = e.target.checked; toolStatus('razor ' + (allTracks ? 'splits every track' : 'splits one clip')); }});\n        document.getElementById('tool-split-playhead').addEventListener('click', () => callTool({{tool: 'split-at-playhead'}}));\n        document.getElementById('tool-select-before').addEventListener('click', () => callTool({{tool: 'select-before'}}));\n        document.getElementById('tool-select-after').addEventListener('click', () => callTool({{tool: 'select-after'}}));\n        document.getElementById('work-set').addEventListener('click', async () => {{\n          const inp = document.getElementById('work-in').value; const outp = document.getElementById('work-out').value;\n          try {{\n            const r = await fetch('/api/studio/workarea', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify({{in: Number(inp), out: Number(outp)}})}});\n            const t = await r.text();\n            if (!r.ok) {{ toolStatus(t); banner(t); return; }}\n            toolStatus('work area: ' + t);\n          }} catch (e) {{ toolStatus('work area failed: ' + e); }}\n        }});\n        document.getElementById('work-clear').addEventListener('click', async () => {{\n          await fetch('/api/studio/workarea', {{method: 'DELETE'}});\n          toolStatus('work area cleared');\n        }});\n        document.getElementById('scrub').addEventListener('input', (e) => seekFrame(Number(e.target.value)));\n        document.getElementById('ruler').addEventListener('input', (e) => seekFrame(Number(e.target.value)));\n\
        document.getElementById('theme-toggle').addEventListener('click', async () => {{\n          const studio = document.getElementById('studio');\n          const next = studio.dataset.theme === 'dark' ? 'light' : 'dark';\n          try {{\n            const r = await fetch('/api/studio/theme', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify({{theme: next}})}});\n            if (!r.ok) {{ const t = await r.text(); banner(t); toast(t); return; }}\n            studio.dataset.theme = next;\n            const tg = document.getElementById('theme-toggle');\n            if (tg) tg.textContent = 'theme: ' + next;\n            paintSaveState('theme ' + next);\n            toast('theme ' + next);\n          }} catch (e) {{ toast('theme switch failed: ' + e); }}\n        }});\n\
        document.getElementById('queue-toggle').addEventListener('click', async () => {{\n          const paused = !(boot.saveState && boot.saveState.paused);\n          try {{\n            const r = await fetch('/api/studio/queue', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify({{paused: paused}})}});\n            const t = await r.text();\n            if (!r.ok) {{ banner(t); toast(t); return; }}\n            const s = JSON.parse(t);\n            boot.saveState = {{paused: s.paused, stagedEdits: s.edits || (boot.saveState && boot.saveState.stagedEdits) || []}};\n            paintQueue();\n            toast(s.paused ? 'save queue paused' : 'save queue resumed');\n            if (s.turn !== undefined && s.turn !== null) paintSaveState('saved turn ' + s.turn);\n          }} catch (e) {{ toast('queue switch failed: ' + e); }}\n        }});\n\
        async function refreshConflict() {{\n          try {{\n            const c = await (await fetch('/api/studio/conflict')).json();\n            const b = document.getElementById('conflict-banner');\n            const picks = document.getElementById('conflict-picks');\n            if (!b || !picks) return;\n            if (!c.conflict) {{ b.hidden = true; picks.hidden = true; return; }}\n            b.hidden = false; picks.hidden = false;\n            b.textContent = c.notice;\n          }} catch (e) {{}}\n        }}\n\
        async function postPick(pick) {{\n          try {{\n            const r = await fetch('/api/studio/conflict', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify({{pick: pick, actor: 'maker'}})}});\n            const t = await r.text();\n            if (!r.ok) {{ banner(t); toast(t); return; }}\n            const j = JSON.parse(t);\n            toast('conflict resolved (' + j.pick + ', turn ' + j.turn + '): ' + j.kept);\n            paintSaveState('resolved ' + j.pick + ' (turn ' + j.turn + ')');\n            refreshConflict(); refreshSaveState();\n          }} catch (e) {{ toast('conflict pick failed: ' + e); }}\n        }}\n\
        ['pick-disk', 'pick-unsaved'].forEach((id) => {{\n          const el = document.getElementById(id);\n          if (el && !el.dataset.wired) {{ el.dataset.wired = '1'; el.addEventListener('click', () => postPick(id === 'pick-disk' ? 'disk' : 'unsaved')); }}\n        }});\n\
        setInterval(refreshConflict, 3000);\n\
        if (boot.readOnly) {{ document.querySelectorAll('#toolbar button, .dock-bar button').forEach((b) => {{ b.disabled = true; }}); }}\n\
        let lanesData = null; let beatsData = null; let visualsData = null; let filmFps = 30;\n        let selectedDiamond = null;\n        function studioQuery() {{ const q = new URLSearchParams(); q.set('composition', (rows && rows.composition) || boot.composition); return q.toString(); }}\n        function timelineDuration() {{ let d = 0; for (const c of ((rows && rows.clips) || [])) {{ if (c.window && c.window[1] > d) d = c.window[1]; }} return d; }}\n        function playheadSecs() {{ return filmFps > 0 ? cur / filmFps : 0; }}\n        async function postKeyframe(payload) {{\n          try {{\n            const body = Object.assign({{actor: 'maker'}}, payload);\n            const r = await fetch('/api/studio/keyframe', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify(body)}});\n            const t = await r.text();\n            if (!r.ok) {{ banner(t); toast(t); return; }}\n            const j = JSON.parse(t);\n            toast('keyframe turn ' + j.turn + (j.clamped ? ' (clamped to ' + Number(j.applied_at).toFixed(3) + 's)' : '') + ': ' + (j.summaries || []).join('; '));\n            paintLanes();\n          }} catch (e) {{ toast('keyframe failed: ' + e); }}\n        }}\n        function selectDiamond(key, stop) {{\n          selectedDiamond = {{key: key, stop: stop}};\n          document.querySelectorAll('button.diamond').forEach((d) => {{ d.dataset.selected = (d.dataset.key == String(key) && d.dataset.stop == String(stop)) ? '1' : '0'; }});\n          const menu = document.getElementById('lane-menu');\n          if (menu) {{ menu.hidden = false; const label = document.getElementById('lane-menu-label'); if (label) label.textContent = 'key ' + key + ' stop ' + stop + ': '; }}\n        }}\n        async function paintLanes() {{\n          const box = document.getElementById('lane-list'); if (!box) return;\n          try {{\n            const r = await fetch('/api/studio/lanes?' + studioQuery());\n            if (!r.ok) {{ box.textContent = await r.text(); return; }}\n            lanesData = await r.json();\n          }} catch (e) {{ box.textContent = 'lanes unavailable: ' + e; return; }}\n          box.innerHTML = '';\n          const dur = timelineDuration() || 1;\n          for (const lane of (lanesData.lanes || [])) {{\n            const g = document.createElement('div'); g.className = 'lane';\n            const h = document.createElement('h4'); h.textContent = lane.property; g.appendChild(h);\n            for (const key of (lane.keys || [])) {{\n              const track = document.createElement('div'); track.className = 'lane-track';\n              track.title = 'key ' + key.index + ' segment ' + key.segment[0].toFixed(2) + 's..' + key.segment[1].toFixed(2) + 's';\n              for (let s = 0; s < key.stops.length; s++) {{\n                const st = key.stops[s];\n                const d = document.createElement('button'); d.type = 'button'; d.className = 'diamond'; d.draggable = true;\n                d.textContent = '◆';\n                d.title = 'key ' + key.index + ' stop ' + s + ' at ' + st.at.toFixed(3) + 's (drag to retime, click for menu)';\n                d.style.left = 'calc(' + (st.at / dur * 100).toFixed(2) + '% - 10px)';\n                d.dataset.key = key.index; d.dataset.stop = s;\n                d.addEventListener('click', () => selectDiamond(key.index, s));\n                d.addEventListener('dragstart', (e) => {{ e.dataTransfer.setData('text/studio-diamond', key.index + ':' + s); }});\n                track.appendChild(d);\n              }}\n              track.addEventListener('dragover', (e) => e.preventDefault());\n              track.addEventListener('drop', (e) => {{\n                e.preventDefault();\n                const parts = (e.dataTransfer.getData('text/studio-diamond') || '').split(':');\n                if (parts.length !== 2) return;\n                const rect = track.getBoundingClientRect();\n                const at = (e.clientX - rect.left) / rect.width * (timelineDuration() || 1);\n                postKeyframe({{action: 'retime', key: Number(parts[0]), stop: Number(parts[1]), at: at}});\n              }});\n              g.appendChild(track);\n            }}\n            box.appendChild(g);\n          }}\n        }}\n        document.querySelectorAll('#lane-menu [data-kact]').forEach((b) => b.addEventListener('click', () => {{\n          if (!selectedDiamond) {{ toast('pick a diamond first'); return; }}\n          const act = b.dataset.kact;\n          if (act === 'retime') postKeyframe({{action: 'retime', key: selectedDiamond.key, stop: selectedDiamond.stop, at: playheadSecs()}});\n          else if (act === 'set-ease') {{ const v = prompt('easing for key ' + selectedDiamond.key + ' stop ' + selectedDiamond.stop, 'linear'); if (v === null) return; postKeyframe({{action: 'set-ease', key: selectedDiamond.key, stop: selectedDiamond.stop, ease: v}}); }}\n          else if (act === 'copy') postKeyframe({{action: 'copy', key: selectedDiamond.key}});\n          else if (act === 'delete') postKeyframe({{action: 'delete', key: selectedDiamond.key, stop: selectedDiamond.stop}});\n        }}));\n        async function postBeats(payload) {{\n          try {{\n            const r = await fetch('/api/studio/beats', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify(payload)}});\n            const t = await r.text();\n            if (!r.ok) {{ banner(t); toast(t); return; }}\n            const j = JSON.parse(t);\n            toast(j.detail + ' (beats: ' + (j.beats || []).length + ', undo: ' + j.undo_depth + ')');\n            paintBeats();\n          }} catch (e) {{ toast('beats failed: ' + e); }}\n        }}\n        async function paintBeats() {{\n          const box = document.getElementById('beat-list'); if (!box) return;\n          try {{\n            const r = await fetch('/api/studio/beats?' + studioQuery());\n            if (!r.ok) {{ box.textContent = await r.text(); return; }}\n            beatsData = await r.json();\n          }} catch (e) {{ box.textContent = 'beats unavailable: ' + e; return; }}\n          box.innerHTML = '';\n          for (const b of (beatsData.beats || [])) {{\n            const row = document.createElement('div'); row.className = 'beat-row';\n            const name = document.createElement('strong'); name.textContent = b.name;\n            const at = document.createElement('span'); at.textContent = b.at.toFixed(3) + 's';\n            const move = document.createElement('button'); move.type = 'button'; move.textContent = 'move';\n            move.addEventListener('click', () => {{ const v = prompt('move beat ' + b.name + ' to (seconds)', String(b.at)); if (v === null) return; postBeats({{action: 'move', name: b.name, at: Number(v)}}); }});\n            const del = document.createElement('button'); del.type = 'button'; del.textContent = 'delete';\n            del.addEventListener('click', () => postBeats({{action: 'delete', name: b.name}}));\n            row.append(name, at, move, del);\n            box.appendChild(row);\n          }}\n          const undo = document.getElementById('beat-undo');\n          if (undo) {{ undo.textContent = 'undo beat (' + (beatsData.undo_depth || 0) + ')'; undo.onclick = () => postBeats({{action: 'undo'}}); }}\n          const add = document.getElementById('beat-add');\n          if (add && !add.dataset.wired) {{ add.dataset.wired = '1'; add.addEventListener('click', () => {{\n            const nameEl = document.getElementById('beat-name'); const atEl = document.getElementById('beat-at');\n            postBeats({{action: 'add', name: nameEl ? nameEl.value : '', at: Number(atEl ? atEl.value : 0)}});\n          }}); }}\n        }}\n        async function paintVisuals() {{\n          const box = document.getElementById('visual-list'); if (!box) return;\n          try {{\n            const r = await fetch('/api/studio/visuals?' + studioQuery());\n            if (!r.ok) {{ box.textContent = await r.text(); return; }}\n            visualsData = await r.json();\n          }} catch (e) {{ box.textContent = 'visuals unavailable: ' + e; return; }}\n          if (visualsData.fps > 0) filmFps = visualsData.fps;\n          box.innerHTML = '';\n          for (const c of (visualsData.clips || [])) {{\n            const card = document.createElement('div'); card.className = 'visual-card';\n            const h = document.createElement('h5');\n            h.textContent = c.kind + ' ' + (c.id || ('#' + c.index)) + ' ' + c.window[0].toFixed(2) + 's..' + c.window[1].toFixed(2) + 's';\n            card.appendChild(h);\n            const strip = c.strip || {{kind: 'none'}};\n            if (strip.kind === 'filmstrip') {{\n              const f = document.createElement('div'); f.className = 'filmstrip';\n              for (const i of (strip.frames || [])) {{ const img = document.createElement('img'); img.src = '/api/frame?i=' + i + '&action=step'; img.alt = 'thumbnail frame ' + i; f.appendChild(img); }}\n              card.appendChild(f);\n            }} else if (strip.kind === 'waveform') {{\n              const w = document.createElement('div'); w.className = 'wave'; w.title = 'peak ' + strip.peak_dbfs + ' dBFS';\n              const marks = strip.peak_buckets || [];\n              (strip.peaks || []).forEach((p, i) => {{ const s = document.createElement('span'); s.style.height = Math.round(p * 48) + 'px'; if (marks.indexOf(i) >= 0) s.dataset.peak = '1'; s.title = p.toFixed(3); w.appendChild(s); }});\n              card.appendChild(w);\n            }} else if (strip.kind === 'text') {{\n              const p = document.createElement('p'); p.textContent = strip.content; card.appendChild(p);\n            }} else if (strip.kind === 'placeholder') {{\n              const p = document.createElement('p'); p.className = 'placeholder-box'; p.textContent = strip.name + ': ' + strip.reason; card.appendChild(p);\n            }}\n            const bl = document.createElement('div');\n            const badges = c.badges || {{}};\n            if (badges.volume_db) {{ const s = document.createElement('span'); s.className = 'badge'; s.textContent = badges.volume_db.muted ? 'volume muted' : ('volume ' + badges.volume_db.db + ' dB'); bl.appendChild(s); }}\n            for (const fx of (badges.effects || [])) {{ const s = document.createElement('span'); s.className = 'badge'; s.textContent = 'fx ' + fx; bl.appendChild(s); }}\n            if (badges.vfx) {{ const s = document.createElement('span'); s.className = 'badge'; s.textContent = 'vfx'; bl.appendChild(s); }}\n            for (const t of (badges.transitions || [])) {{ const s = document.createElement('span'); s.className = 'badge'; s.textContent = t.kind + ' ' + t.side + ' ' + t.with; bl.appendChild(s); }}\n            if (bl.children.length) card.appendChild(bl);\n            const fades = c.fades || {{}};\n            if ((fades.in || 0) > 0 || (fades.out || 0) > 0) {{\n              const p = document.createElement('p'); p.className = 'badge';\n              p.textContent = 'fade handles in ' + Number(fades.in).toFixed(2) + 's (frame ' + fades.in_frame + '), out ' + Number(fades.out).toFixed(2) + 's (frame ' + fades.out_frame + ')';\n              card.appendChild(p);\n            }}\n            box.appendChild(card);\n          }}\n        }}\n        function shortcutStatus(t) {{ const s = document.getElementById('shortcut-status'); if (s) s.textContent = t; }}\n        function eventChord(e) {{\n          const k = String(e.key || '');\n          if (k === 'Control' || k === 'Alt' || k === 'Shift' || k === 'Meta') return null;\n          const mods = [];\n          if (e.ctrlKey) mods.push('ctrl');\n          if (e.altKey) mods.push('alt');\n          if (e.shiftKey) mods.push('shift');\n          if (e.metaKey) mods.push('meta');\n          let key = k.toLowerCase();\n          if (key === ' ') key = 'space';\n          else if (key === 'arrowleft') key = 'arrow-left';\n          else if (key === 'arrowright') key = 'arrow-right';\n          else if (key === 'arrowup') key = 'arrow-up';\n          else if (key === 'arrowdown') key = 'arrow-down';\n          return (mods.length ? mods.join('+') + '+' : '') + key;\n        }}\n        async function runShortcut(payload) {{\n          try {{\n            const r = await fetch('/api/studio/shortcut', {{method: 'POST', headers: {{'Content-Type': 'application/json'}}, body: JSON.stringify(payload)}});\n            const t = await r.text();\n            if (!r.ok) {{ shortcutStatus(t); banner(t); return; }}\n            applyShortcutEffect(JSON.parse(t));\n          }} catch (e) {{ shortcutStatus('shortcut failed: ' + e); }}\n        }}\n        function applyShortcutEffect(j) {{\n          shortcutStatus(j.action + ' (' + j.area + ')');\n          const fx = j.effect || {{}};\n          if (fx.panel_tool) {{ tool = fx.panel_tool; toolStatus('tool: ' + tool); }}\n          if (fx.focus) {{ const el = document.getElementById('panel-' + fx.focus); if (el && el.scrollIntoView) el.scrollIntoView(); toast('focus ' + fx.focus + ' panel'); }}\n          if (typeof fx.frame === 'number') show(fx.frame);\n          else if (j.readout && typeof j.readout.frame === 'number') show(j.readout.frame);\n          if (fx.rows) {{ rows = fx.rows; paintRows(); }}\n          if (fx.selection && fx.selection.length) {{ selection = fx.selection.slice(); paintRows(); syncLink(); }}\n          if (fx.theme) {{ const studio = document.getElementById('studio'); studio.dataset.theme = fx.theme; const tg = document.getElementById('theme-toggle'); if (tg) tg.textContent = 'theme: ' + fx.theme; paintSaveState('theme ' + fx.theme); }}\n          if (fx.layout_state && fx.layout_state.layout) {{ layout = fx.layout_state.layout; paintLayout(); }}\n          if (fx.request) toast(fx.request);\n          if (fx.turn !== undefined && fx.turn !== null) paintSaveState('saved turn ' + fx.turn);\n          if (fx.undone_seq !== undefined) paintSaveState('undid turn ' + fx.undone_seq);\n          refreshSaveState();\n          paintShortcuts();\n        }}\n        document.addEventListener('keydown', (e) => {{\n          if (boot.readOnly) return;\n          const tag = (e.target && e.target.tagName) || '';\n          if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target && e.target.isContentEditable)) return;\n          if (e.key === ' ' && e.target && e.target.closest && e.target.closest('button')) return;\n          const chord = eventChord(e);\n          if (!chord) return;\n          e.preventDefault();\n          runShortcut({{chord: chord}});\n        }});\n        async function paintShortcuts() {{\n          const box = document.getElementById('shortcut-tables'); if (!box) return;\n          let j = null;\n          try {{\n            const r = await fetch('/api/studio/shortcuts');\n            if (!r.ok) {{ box.textContent = await r.text(); return; }}\n            j = await r.json();\n          }} catch (e) {{ box.textContent = 'shortcuts unavailable: ' + e; return; }}\n          box.innerHTML = '';\n          const mk = (title, list) => {{\n            const h = document.createElement('h4'); h.textContent = title; box.appendChild(h);\n            const tb = document.createElement('table'); tb.className = 'shortcut-table';\n            const head = document.createElement('tr');\n            for (const c of ['keys', 'action', 'area']) {{ const th = document.createElement('th'); th.textContent = c; head.appendChild(th); }}\n            tb.appendChild(head);\n            for (const b of list) {{\n              const tr = document.createElement('tr');\n              for (const c of [b.chord, b.action, b.area]) {{ const td = document.createElement('td'); td.textContent = c; tr.appendChild(td); }}\n              tb.appendChild(tr);\n            }}\n            box.appendChild(tb);\n          }};\n          mk('effective bindings', j.bindings || []);\n          mk('default map (underneath)', j.defaults || []);\n          if (j.notice) shortcutStatus(j.notice);\n          if ((j.collisions || []).length) shortcutStatus((j.collisions || []).join('; '));\n          const copy = document.getElementById('describe-copy');\n          if (copy && !copy.dataset.wired) {{ copy.dataset.wired = '1'; copy.addEventListener('click', async () => {{\n            try {{\n              const q = new URLSearchParams(); q.set('selection', selection.join(','));\n              const r = await fetch('/api/studio/describe?' + q.toString());\n              const t = await r.text();\n              if (!r.ok) {{ shortcutStatus(t); banner(t); return; }}\n              const dj = JSON.parse(t);\n              if (navigator.clipboard && navigator.clipboard.writeText) await navigator.clipboard.writeText(dj.request);\n              shortcutStatus('agent request copied (' + selection.join(', ') + ')');\n              toast(dj.request);\n            }} catch (e) {{ shortcutStatus('describe failed: ' + e); }}\n          }}); }}\n        }}\n        paintLayout(); paintRows(); paintQueue(); paintLanes(); paintBeats(); paintVisuals(); paintSnapZoom();{snap_zoom_js} paintClips();{clips_js} refreshConflict(); paintShortcuts(); syncLink();\n</script>\n</div>\n</body>\n</html>\n",
        project = html_escape(&model.project),
        composition = html_escape(&model.composition),
        comps = html_escape(&model.all_compositions.join(", ")),
        banner = banner,
        conflict = conflict,
        queue_banner = if model.save_paused || model.staged > 0 {
            format!(
                "<p id=\"queue-banner\" role=\"alert\">save queue paused with {} staged edit(s): {}</p>",
                model.staged,
                html_escape(&model.staged_summaries.join("; "))
            )
        } else {
            "<p id=\"queue-banner\" hidden></p>".to_string()
        },
        theme = html_escape(&model.theme),
        read_only = if model.read_only { "1" } else { "0" },
        save_wording = if model.read_only {
            "read-only".to_string()
        } else if model.save_paused {
            format!("paused · {} staged", model.staged)
        } else if model.staged > 0 {
            format!("{} staged", model.staged)
        } else {
            "saved".to_string()
        },
        saved = if model.layout_restored {
            "restored"
        } else {
            "default"
        },
        frame = model.frame,
        frame_count = model.frame_count,
        max = max,
        time = if model.fps_value > 0.0 {
            model.frame as f64 / model.fps_value
        } else {
            0.0
        },
        duration = model.duration_secs,
        selection = html_escape(&selection_text),
        boot = boot,
        rows = model.rows_json,
        snap_zoom_css = snap_zoom_css(),
        snap_zoom_html = snap_zoom_html(),
        snap_zoom_js = snap_zoom_js(),
        clips_css = clips_css(),
        clips_html = clips_html(),
        clips_js = clips_js(),
    )
}

/// Keyframe lanes, the beat strip, and clip visuals (Sprint 16, T007 /
/// CAP-007, CAP-008, J-03, REC-005): diamonds retime one property each
/// through the SDK patch and history machinery, beats move and delete
/// with beat-only undo, and every clip shows truthful visuals read
/// from the same composition data the render uses — never separate
/// state. Style attributes stay CSS-like hyphenated end to end, and
/// `font-family` try-order lists pass through untouched (fail-closed
/// at render time, never resolved here).
///
/// Why a keyframe or beat edit refused: the reason with nothing
/// staged, so the timeline, canvas, and history stay exactly as they
/// were.
fn refused_visual(message: String) -> String {
    format!("studio refused: {message}")
}

/// One keyframe stop on a lane diamond (Sprint 16, T007): the instant
/// with the value and the easing of the segment leaving it. Numeric
/// properties carry `value`; `fill` and `border-color` carry `color`
/// as `#rrggbb`; easing names the curve (`None` means the key
/// easing, then linear).
#[derive(Debug, Clone)]
pub struct LaneStop {
    /// Composition instant of the stop.
    pub at: f64,
    /// Numeric value, for numeric properties.
    pub value: Option<f64>,
    /// Color endpoint as `#rrggbb`, for color properties.
    pub color: Option<String>,
    /// Easing of the segment leaving this stop, when written.
    pub ease: Option<String>,
}

/// One key on a lane (Sprint 16, T007): the directive index with its
/// target clip, property, span, and stops.
#[derive(Debug, Clone)]
pub struct LaneKey {
    /// Document index of the key directive.
    pub index: usize,
    /// Document index of the target clip.
    pub target: usize,
    /// Stable `id` of the target clip, when declared.
    pub target_id: Option<String>,
    /// Canonical property name.
    pub property: String,
    /// Key-level easing fallback, when written.
    pub ease: Option<String>,
    /// Target clip film window `[start, start + duration)`: the
    /// segment drags clamp into.
    pub segment: (f64, f64),
    /// Stop span `[first.at, last.at]`.
    pub span: (f64, f64),
    /// Stops in document order.
    pub stops: Vec<LaneStop>,
}

/// One keyframe lane (Sprint 16, T007): every key of one property
/// group, so lanes cover one property each.
#[derive(Debug, Clone)]
pub struct KeyLane {
    /// Canonical property name of the lane.
    pub property: String,
    /// Keys of the property, in directive order.
    pub keys: Vec<LaneKey>,
}

/// Group every key directive of one composition into property lanes
/// (Sprint 16, T007): a readout of the validated composition in
/// directive order within each property, in property name order
/// across lanes. Keys whose target clip is gone (a stale document
/// index after clip edits) report an empty segment and span at zero
/// rather than guessing a window.
pub fn keyframe_lanes_of(comp: &notation::model::Composition) -> Vec<KeyLane> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<LaneKey>> = BTreeMap::new();
    for (index, key) in comp.keys.iter().enumerate() {
        let (segment, target_id) = match comp.clips.get(key.target) {
            Some(clip) => (
                (clip.start(), clip.start() + clip.duration()),
                clip.id().map(|s| s.to_string()),
            ),
            None => ((0.0, 0.0), None),
        };
        let span = match (key.stops.first(), key.stops.last()) {
            (Some(first), Some(last)) => (first.at, last.at),
            _ => (0.0, 0.0),
        };
        let stops = key
            .stops
            .iter()
            .map(|stop| LaneStop {
                at: stop.at,
                value: if stop.value_color.is_some() {
                    None
                } else {
                    Some(stop.value)
                },
                color: stop.value_color.map(|rgb| format!("#{}", rgb.to_hex())),
                ease: stop.ease.as_ref().map(|e| e.name()),
            })
            .collect();
        groups
            .entry(key.property.clone())
            .or_default()
            .push(LaneKey {
                index,
                target: key.target,
                target_id,
                property: key.property.clone(),
                ease: key.ease.as_ref().map(|e| e.name()),
                segment,
                span,
                stops,
            });
    }
    groups
        .into_iter()
        .map(|(property, keys)| KeyLane { property, keys })
        .collect()
}

/// Render one stop as JSON for the lanes API.
fn lane_stop_json(stop: &LaneStop) -> String {
    let value = match stop.value {
        Some(v) => format!("{v}"),
        None => "null".to_string(),
    };
    let color = match &stop.color {
        Some(c) => format!("\"{}\"", json_escape(c)),
        None => "null".to_string(),
    };
    let ease = match &stop.ease {
        Some(e) => format!("\"{}\"", json_escape(e)),
        None => "null".to_string(),
    };
    format!(
        "{{\"at\":{:.3},\"value\":{value},\"color\":{color},\"ease\":{ease}}}",
        stop.at
    )
}

/// Render keyframe lanes as JSON for the Studio lanes API (Sprint 16,
/// T007): one property group each with draggable-diamond data (stop
/// instants, values, easings) plus the target clip segment every drag
/// clamps into.
pub fn keyframe_lanes_json(comp: &notation::model::Composition) -> String {
    let mut lanes = String::new();
    for lane in keyframe_lanes_of(comp) {
        if !lanes.is_empty() {
            lanes.push(',');
        }
        let mut keys = String::new();
        for key in &lane.keys {
            if !keys.is_empty() {
                keys.push(',');
            }
            let target_id = match &key.target_id {
                Some(id) => format!("\"{}\"", json_escape(id)),
                None => "null".to_string(),
            };
            let ease = match &key.ease {
                Some(e) => format!("\"{}\"", json_escape(e)),
                None => "null".to_string(),
            };
            let stops = key
                .stops
                .iter()
                .map(lane_stop_json)
                .collect::<Vec<_>>()
                .join(",");
            keys.push_str(&format!(
                "{{\"index\":{},\"target\":{},\"target_id\":{},\"property\":\"{}\",\"ease\":{},\"segment\":[{:.3},{:.3}],\"span\":[{:.3},{:.3}],\"stops\":[{}]}}",
                key.index,
                key.target,
                target_id,
                json_escape(&key.property),
                ease,
                key.segment.0,
                key.segment.1,
                key.span.0,
                key.span.1,
                stops
            ));
        }
        lanes.push_str(&format!(
            "{{\"property\":\"{}\",\"keys\":[{}]}}",
            json_escape(&lane.property),
            keys
        ));
    }
    format!(
        "{{\"composition\":\"{}\",\"lanes\":[{}]}}",
        json_escape(&comp.name),
        lanes
    )
}

/// Clamp one keyframe drag into its segment (Sprint 16, T007 /
/// CAP-007): the target clip's film window, tightened to keep the
/// stops in strictly rising time order. Out-of-segment drags clamp to
/// the segment edge and report the clamped value; an unknown key or
/// stop, a stale clip target, a segment the neighbor order cannot
/// enter, or a drag colliding with a neighbor refuses naming the
/// cause; a non-finite instant refuses as a bad value.
pub fn clamp_key_stop(
    comp: &notation::model::Composition,
    key: usize,
    stop: usize,
    want: f64,
) -> Result<(f64, bool), String> {
    let entry = comp.keys.get(key).ok_or_else(|| {
        refused_visual(format!(
            "key index {key} names nothing; the composition holds {} key(s). Nothing was staged.",
            comp.keys.len()
        ))
    })?;
    if stop >= entry.stops.len() {
        return Err(refused_visual(format!(
            "key {key} stop {stop} names nothing; key {key} holds {} stop(s). Nothing was staged.",
            entry.stops.len()
        )));
    }
    if !want.is_finite() {
        return Err(refused_visual(format!(
            "key {key} stop {stop} drags to {want:?}; write a finite instant at or after 0. Nothing was staged."
        )));
    }
    let clip = comp.clips.get(entry.target).ok_or_else(|| {
        refused_visual(format!(
            "key {key} targets clip {} which names nothing; its target is gone. Nothing was staged.",
            entry.target
        ))
    })?;
    let (seg_lo, seg_hi) = (clip.start(), clip.start() + clip.duration());
    let mut lo = seg_lo;
    let mut hi = seg_hi;
    if stop > 0 {
        lo = lo.max(entry.stops[stop - 1].at);
    }
    if stop + 1 < entry.stops.len() {
        hi = hi.min(entry.stops[stop + 1].at);
    }
    if lo > hi {
        return Err(refused_visual(format!(
            "key {key} stop {stop} cannot land inside its segment ({seg_lo:.3}s..{seg_hi:.3}s): the neighbor order leaves no room. Nothing was staged."
        )));
    }
    let clamped = want.clamp(lo, hi);
    if stop > 0 && clamped <= entry.stops[stop - 1].at {
        return Err(refused_visual(format!(
            "key {key} stop {stop} drags to {want} which never rises past stop {} at {}; write an instant past the previous stop. Nothing was staged.",
            stop - 1,
            entry.stops[stop - 1].at
        )));
    }
    if stop + 1 < entry.stops.len() && clamped >= entry.stops[stop + 1].at {
        return Err(refused_visual(format!(
            "key {key} stop {stop} drags to {want} which never stays under stop {} at {}; write an instant before the next stop. Nothing was staged.",
            stop + 1,
            entry.stops[stop + 1].at
        )));
    }
    Ok((clamped, clamped != want))
}

/// One keyframe menu action (Sprint 16, T007): the four diamond menu
/// entries — retime to the playhead (or any instant, clamped into the
/// segment), edit the easing, copy the key, or delete the stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyframeAction {
    /// Move the stop to an instant (clamped into the segment).
    Retime,
    /// Edit the key-level or stop easing.
    SetEase,
    /// Duplicate the whole key.
    Copy,
    /// Delete the stop (or the whole two-stop key).
    Delete,
}

impl KeyframeAction {
    /// Parse one action spelling; anything else refuses.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "retime" => Some(KeyframeAction::Retime),
            "set-ease" => Some(KeyframeAction::SetEase),
            "copy" => Some(KeyframeAction::Copy),
            "delete" => Some(KeyframeAction::Delete),
            _ => None,
        }
    }
}

/// One parsed keyframe request (Sprint 16, T007): the actor the
/// validated turn records plus the menu action with its targets.
/// Structural parsing only; the segment clamp and the SDK apply run
/// against the effective composition at the route.
#[derive(Debug, Clone)]
pub struct KeyframeRequest {
    /// Actor the validated turn is recorded under (human or agent).
    pub actor: String,
    /// The diamond menu action.
    pub action: KeyframeAction,
    /// Composition the edit applies to, when named.
    pub composition: Option<String>,
    /// Key directive index.
    pub key: usize,
    /// Stop index (`retime` and `delete` need it; `set-ease` takes it
    /// for stop easing and omits it for the key fallback).
    pub stop: Option<usize>,
    /// Destination instant (`retime` needs it).
    pub at: Option<f64>,
    /// Easing name (`set-ease` needs it).
    pub ease: Option<String>,
}

/// Parse one keyframe request body
/// (`{"actor":"maker","action":"retime","key":0,"stop":1,"at":2.5}`):
/// the actor must be non-empty, the action names the four menu
/// entries, and the key index must ride along. A refused body names
/// its cause and stages nothing.
pub fn parse_keyframe_body(body: &str) -> Result<KeyframeRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_visual(
            "the studio keyframe edit needs a JSON body (`{\"actor\":\"maker\",\"action\":\"retime\",\"key\":0, ...}`); pass the diamond menu action. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_visual(format!(
            "the studio keyframe body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"action\":\"retime\",\"key\":0, ...}}`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_visual(
            "the studio keyframe edit names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let action_raw = value.get("action").and_then(|a| a.as_str()).unwrap_or("");
    let action = KeyframeAction::parse(action_raw).ok_or_else(|| {
        refused_visual(format!(
            "unknown keyframe action {action_raw:?}: the diamond menu offers `retime`, `set-ease`, `copy`, and `delete`. Pass one of those actions. Nothing was staged."
        ))
    })?;
    let key = value
        .get("key")
        .and_then(|k| k.as_u64())
        .map(|k| k as usize)
        .ok_or_else(|| {
            refused_visual(
                "the studio keyframe edit names no `key` index; pass the key directive index the diamond belongs to. Nothing was staged.".to_string(),
            )
        })?;
    let composition = value
        .get("composition")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string());
    let stop = value
        .get("stop")
        .and_then(|s| s.as_u64())
        .map(|s| s as usize);
    let at = value.get("at").and_then(|a| a.as_f64());
    let ease = value
        .get("ease")
        .and_then(|e| e.as_str())
        .map(|e| e.to_string());
    Ok(KeyframeRequest {
        actor,
        action,
        composition,
        key,
        stop,
        at,
        ease,
    })
}

/// Build the SDK patch for one parsed keyframe request against the
/// effective composition (Sprint 16, T007): retimes clamp into the
/// target clip segment first (reporting the clamped value), ease edits
/// parse like `check`, copies duplicate the key, and deletes drop the
/// stop (or the whole two-stop key). Every refusal names its cause
/// and stages nothing, so the composition, delta, and history stay
/// exactly as they were.
pub fn keyframe_op_for(
    comp: &notation::model::Composition,
    req: &KeyframeRequest,
) -> Result<(sdk::EditOp, bool, Option<f64>), String> {
    match req.action {
        KeyframeAction::Retime => {
            let stop = req.stop.ok_or_else(|| {
                refused_visual(
                    "the keyframe `retime` names no `stop`; pass the stop index the diamond sits at. Nothing was staged.".to_string(),
                )
            })?;
            let want = req.at.ok_or_else(|| {
                refused_visual(
                    "the keyframe `retime` names no `at` instant; pass the destination in composition seconds (the playhead for move-to-playhead). Nothing was staged.".to_string(),
                )
            })?;
            let (applied, clamped) = clamp_key_stop(comp, req.key, stop, want)?;
            Ok((
                sdk::EditOp::RetimeKeyStop {
                    key: req.key,
                    stop,
                    at: applied,
                },
                clamped,
                Some(applied),
            ))
        }
        KeyframeAction::SetEase => {
            let ease = req.ease.clone().filter(|e| !e.is_empty()).ok_or_else(|| {
                refused_visual(
                    "the keyframe `set-ease` names no `ease`; pass the curve (`linear`, `in-quad`, `cubic-bezier(...)`, ...). Nothing was staged.".to_string(),
                )
            })?;
            if comp.keys.get(req.key).is_none() {
                return Err(refused_visual(format!(
                    "key index {} names nothing; the composition holds {} key(s). Nothing was staged.",
                    req.key,
                    comp.keys.len()
                )));
            }
            if let Some(stop) = req.stop {
                if stop >= comp.keys[req.key].stops.len() {
                    return Err(refused_visual(format!(
                        "key {} stop {stop} names nothing; key {} holds {} stop(s). Nothing was staged.",
                        req.key,
                        req.key,
                        comp.keys[req.key].stops.len()
                    )));
                }
            }
            Ok((
                sdk::EditOp::SetKeyEase {
                    key: req.key,
                    stop: req.stop,
                    ease,
                },
                false,
                None,
            ))
        }
        KeyframeAction::Copy => Ok((sdk::EditOp::DuplicateKey { key: req.key }, false, None)),
        KeyframeAction::Delete => {
            let stop = req.stop.ok_or_else(|| {
                refused_visual(
                    "the keyframe `delete` names no `stop`; pass the stop index the diamond sits at (deleting from a two-stop key removes the whole key). Nothing was staged.".to_string(),
                )
            })?;
            if comp.keys.get(req.key).is_none() {
                return Err(refused_visual(format!(
                    "key index {} names nothing; the composition holds {} key(s). Nothing was staged.",
                    req.key,
                    comp.keys.len()
                )));
            }
            Ok((
                sdk::EditOp::RemoveKeyStop { key: req.key, stop },
                false,
                None,
            ))
        }
    }
}

/// Render one keyframe receipt as JSON for the keyframe API: the
/// validated turn with the summaries, plus the clamp report so the
/// lane shows the value the diamond actually landed on.
pub fn keyframe_receipt_json(
    receipt: &StudioEditReceipt,
    clamped: bool,
    applied_at: Option<f64>,
) -> String {
    let at = match applied_at {
        Some(t) => format!("{t:.3}"),
        None => "null".to_string(),
    };
    let base = edit_receipt_json(receipt);
    let base = base.trim_end_matches('}');
    format!("{base},\"clamped\":{clamped},\"applied_at\":{at}}}")
}

/// The Studio beat strip (Sprint 16, T007 / CAP-007, J-03): the
/// rhythm guides the maker places edits against. Beats are
/// Studio-owned rhythm data — one named instant list per composition,
/// kept in the project's hidden history directory next to the layout
/// file — so moving or deleting a beat never retimes a clip and the
/// beat undo restores beats alone without touching clip history.
/// Every mutation validates first and writes the file last, so a
/// refused action leaves the stored beats and their undo exactly as
/// they were.
///
/// Beat strip file name inside the hidden project directory.
pub const STUDIO_BEATS_FILE: &str = "studio-beats.json";

/// Version of the persisted beat strip.
pub const STUDIO_BEATS_VERSION: u32 = 1;

/// Beat undo depth: older snapshots drop, counted like any bounded
/// undo stack.
pub const BEATS_UNDO_CAP: usize = 100;

/// Hidden beat strip path for one project folder.
pub fn studio_beats_path_for(folder: &Path) -> PathBuf {
    folder.join(HIDDEN_DIR).join(STUDIO_BEATS_FILE)
}

/// One beat: a named composition-time instant in seconds.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Beat {
    /// Beat name, unique within its composition.
    pub name: String,
    /// Composition instant in seconds.
    pub at: f64,
}

/// The persisted beat strip: beats plus their beat-only undo snapshots
/// (whole prior lists, newest last), each keyed by composition name so
/// one file serves every composition of the project.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct BeatsFile {
    /// File version ([`STUDIO_BEATS_VERSION`]).
    #[serde(default)]
    pub version: u32,
    /// Beats per composition name, in add order.
    #[serde(default)]
    pub beats: std::collections::BTreeMap<String, Vec<Beat>>,
    /// Undo snapshots per composition name, newest last.
    #[serde(default)]
    pub undo: std::collections::BTreeMap<String, Vec<Vec<Beat>>>,
}

/// What loading the beat strip reported (Sprint 16, T007): the file
/// contents plus the fallback banner when the saved file exists but
/// cannot restore (a missing file loads empty with no banner).
#[derive(Debug, Clone)]
pub struct BeatsLoad {
    /// The loaded file (empty when missing or unrestorable).
    pub store: BeatsStore,
    /// Fallback banner, set exactly when the saved file exists but
    /// cannot restore.
    pub notice: Option<String>,
}

/// The working beat strip (Sprint 16, T007): beats plus beat-only
/// undo snapshots per composition, in memory for the route to mutate
/// and persist.
#[derive(Debug, Clone, Default)]
pub struct BeatsStore {
    beats: std::collections::BTreeMap<String, Vec<Beat>>,
    undo: std::collections::BTreeMap<String, Vec<Vec<Beat>>>,
}

impl BeatsStore {
    /// Beats of one composition, in add order.
    pub fn beats_for(&self, composition: &str) -> Vec<Beat> {
        self.beats.get(composition).cloned().unwrap_or_default()
    }

    /// Beat-only undo depth of one composition.
    pub fn undo_depth(&self, composition: &str) -> usize {
        self.undo.get(composition).map(|u| u.len()).unwrap_or(0)
    }
}

/// Load the beat strip (Sprint 16, T007): a missing file loads empty
/// with no banner; a corrupt, version-wrong, or invalid file loads
/// empty and says so.
pub fn load_beats(folder: &Path) -> BeatsLoad {
    let path = studio_beats_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return BeatsLoad {
                store: BeatsStore::default(),
                notice: None,
            }
        }
        Err(e) => {
            return BeatsLoad {
                store: BeatsStore::default(),
                notice: Some(format!(
                    "studio: the saved beat strip {} cannot be read ({e}); showing an empty strip instead.",
                    path.display()
                )),
            }
        }
        Ok(raw) => raw,
    };
    let file: BeatsFile = match serde_json::from_str(&raw) {
        Err(e) => {
            return BeatsLoad {
                store: BeatsStore::default(),
                notice: Some(format!(
                    "studio: the saved beat strip {} is not a beat strip ({e}); showing an empty strip instead.",
                    path.display()
                )),
            }
        }
        Ok(file) => file,
    };
    if file.version != STUDIO_BEATS_VERSION {
        return BeatsLoad {
            store: BeatsStore::default(),
            notice: Some(format!(
                "studio: the saved beat strip {} carries version {}, this Studio reads version {}; showing an empty strip instead.",
                path.display(),
                file.version,
                STUDIO_BEATS_VERSION
            )),
        };
    }
    BeatsLoad {
        store: BeatsStore {
            beats: file.beats,
            undo: file.undo,
        },
        notice: None,
    }
}

/// Persist one beat strip (Sprint 16, T007): the whole store writes
/// back to the hidden file, creating the hidden directory when
/// missing.
fn save_beats_file(folder: &Path, store: &BeatsStore) -> Result<(), String> {
    let path = studio_beats_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_visual(format!(
                "the hidden beat directory {} cannot be prepared: {e}. Nothing was staged.",
                parent.display()
            ))
        })?;
    }
    let file = BeatsFile {
        version: STUDIO_BEATS_VERSION,
        beats: store.beats.clone(),
        undo: store.undo.clone(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|e| {
        refused_visual(format!(
            "the beat strip cannot be encoded: {e}. Nothing was staged."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_visual(format!(
            "the beat file {} cannot be written: {e}. Nothing was staged.",
            path.display()
        ))
    })
}

/// Snapshot the current beats of one composition onto its beat-only
/// undo stack (Sprint 16, T007): the mutating actions call this before
/// changing anything, so undo restores beats alone.
fn push_beats_undo(store: &mut BeatsStore, composition: &str) {
    let current = store.beats_for(composition);
    let stack = store.undo.entry(composition.to_string()).or_default();
    stack.push(current);
    while stack.len() > BEATS_UNDO_CAP {
        stack.remove(0);
    }
}

/// Clamp one beat instant into the composition window (Sprint 16,
/// T007): `[0, duration]`. Out-of-window moves clamp to the edge and
/// report the clamped value; a non-finite or negative instant refuses.
fn clamp_beat_at(at: f64, duration: f64) -> Result<(f64, bool), String> {
    if !at.is_finite() || at < 0.0 {
        return Err(refused_visual(format!(
            "the beat moves to {at:?}; write a finite instant at or after 0. Nothing was staged."
        )));
    }
    let hi = if duration.is_finite() && duration > 0.0 {
        duration
    } else {
        at
    };
    let clamped = at.min(hi);
    Ok((clamped, clamped != at))
}

/// Add one beat (Sprint 16, T007): the name must be non-empty and
/// fresh within the composition; the instant clamps into the
/// composition window.
pub fn add_beat(
    store: &mut BeatsStore,
    composition: &str,
    name: &str,
    at: f64,
    duration: f64,
) -> Result<(f64, bool), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(refused_visual(
            "the beat names no name; beats are named (one name per composition). Pass a non-empty `name`. Nothing was staged.".to_string(),
        ));
    }
    if store.beats_for(composition).iter().any(|b| b.name == name) {
        return Err(refused_visual(format!(
            "beat {name:?} already marks composition {composition:?}; beat names stay unique. Pass a fresh name. Nothing was staged."
        )));
    }
    let (applied, clamped) = clamp_beat_at(at, duration)?;
    push_beats_undo(store, composition);
    store
        .beats
        .entry(composition.to_string())
        .or_default()
        .push(Beat { name, at: applied });
    Ok((applied, clamped))
}

/// Move one beat (Sprint 16, T007): an unknown name refuses; the
/// destination clamps into the composition window with the clamped
/// value reported. Clips never move: beats are not composition data.
pub fn move_beat(
    store: &mut BeatsStore,
    composition: &str,
    name: &str,
    at: f64,
    duration: f64,
) -> Result<(f64, bool), String> {
    let (applied, clamped) = clamp_beat_at(at, duration)?;
    let known = store.beats_for(composition).iter().any(|b| b.name == name);
    if !known {
        return Err(refused_visual(format!(
            "beat {name:?} names nothing in composition {composition:?}; the strip holds {}. Nothing was staged.",
            store
                .beats_for(composition)
                .iter()
                .map(|b| format!("{:?}", b.name))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    push_beats_undo(store, composition);
    if let Some(slot) = store
        .beats
        .get_mut(composition)
        .and_then(|list| list.iter_mut().find(|b| b.name == name))
    {
        slot.at = applied;
    }
    Ok((applied, clamped))
}

/// Delete one beat (Sprint 16, T007): an unknown name refuses naming
/// the strip contents. Clips never move: beats are not composition
/// data.
pub fn delete_beat(store: &mut BeatsStore, composition: &str, name: &str) -> Result<(), String> {
    let list = store.beats.get(composition).cloned().unwrap_or_default();
    if !list.iter().any(|b| b.name == name) {
        return Err(refused_visual(format!(
            "beat {name:?} names nothing in composition {composition:?}; the strip holds {}. Nothing was staged.",
            list.iter()
                .map(|b| format!("{:?}", b.name))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    push_beats_undo(store, composition);
    store
        .beats
        .entry(composition.to_string())
        .or_default()
        .retain(|b| b.name != name);
    Ok(())
}

/// Undo the newest beat edit of one composition (Sprint 16, T007):
/// restores the prior beats alone — clips, the SDK delta, and shared
/// history stay exactly as they were. An empty beat undo refuses.
pub fn undo_beats(store: &mut BeatsStore, composition: &str) -> Result<usize, String> {
    let restored = store
        .undo
        .get_mut(composition)
        .and_then(|stack| stack.pop())
        .ok_or_else(|| {
            refused_visual(format!(
                "beat undo names no beat edit: the beat-only undo of composition {composition:?} is empty. Nothing was restored."
            ))
        })?;
    let count = restored.len();
    store.beats.insert(composition.to_string(), restored);
    Ok(count)
}

/// Persist one mutated beat strip after a successful action (Sprint
/// 16, T007): the whole store writes back; a write failure refuses
/// after the in-memory change, naming the file.
pub fn persist_beats(folder: &Path, store: &BeatsStore) -> Result<(), String> {
    save_beats_file(folder, store)
}

/// Render one beat strip as JSON for the beats API (Sprint 16, T007):
/// the beats with the beat-only undo depth.
pub fn beats_json(store: &BeatsStore, composition: &str, notice: Option<&str>) -> String {
    let beats = store
        .beats_for(composition)
        .iter()
        .map(|b| {
            format!(
                "{{\"name\":\"{}\",\"at\":{:.3}}}",
                json_escape(&b.name),
                b.at
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let note = match notice {
        Some(n) => format!("\"{}\"", json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"composition\":\"{}\",\"beats\":[{}],\"undo_depth\":{},\"notice\":{}}}",
        json_escape(composition),
        beats,
        store.undo_depth(composition),
        note
    )
}

/// One beat strip action (Sprint 16, T007): add a beat, move a beat,
/// delete a beat, or undo the newest beat edit of the composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatsAction {
    /// Add a named beat.
    Add,
    /// Move a named beat.
    Move,
    /// Delete a named beat.
    Delete,
    /// Undo the newest beat edit (beats alone).
    Undo,
}

impl BeatsAction {
    /// Parse one action spelling; anything else refuses.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "add" => Some(BeatsAction::Add),
            "move" => Some(BeatsAction::Move),
            "delete" => Some(BeatsAction::Delete),
            "undo" => Some(BeatsAction::Undo),
            _ => None,
        }
    }
}

/// One parsed beat strip request (Sprint 16, T007): the action with
/// its targets. Structural parsing only; the mutation runs against
/// the loaded strip at the route.
#[derive(Debug, Clone)]
pub struct BeatsRequest {
    /// The strip action.
    pub action: BeatsAction,
    /// Composition the beats belong to, when named.
    pub composition: Option<String>,
    /// Beat name (`add`, `move`, and `delete` need it).
    pub name: Option<String>,
    /// Destination instant (`add` and `move` need it).
    pub at: Option<f64>,
}

/// Parse one beats request body
/// (`{"action":"add","name":"downbeat","at":1.5}`): the action names
/// the four strip entries. A refused body names its cause and stages
/// nothing.
pub fn parse_beats_body(body: &str) -> Result<BeatsRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_visual(
            "the studio beats edit needs a JSON body (`{\"action\":\"add\",\"name\":\"downbeat\",\"at\":1.5}`); pass the strip action. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_visual(format!(
            "the studio beats body is not JSON ({e}). Pass `{{\"action\":\"add\",\"name\":\"downbeat\",\"at\":1.5}}`. Nothing was staged."
        ))
    })?;
    let action_raw = value.get("action").and_then(|a| a.as_str()).unwrap_or("");
    let action = BeatsAction::parse(action_raw).ok_or_else(|| {
        refused_visual(format!(
            "unknown beats action {action_raw:?}: the strip offers `add`, `move`, `delete`, and `undo`. Pass one of those actions. Nothing was staged."
        ))
    })?;
    let composition = value
        .get("composition")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string());
    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.to_string());
    let at = value.get("at").and_then(|a| a.as_f64());
    Ok(BeatsRequest {
        action,
        composition,
        name,
        at,
    })
}

/// Render one beats receipt as JSON: the action with the strip after
/// it, so the strip paints without a second fetch.
pub fn beats_receipt_json(
    store: &BeatsStore,
    composition: &str,
    action: &str,
    detail: &str,
) -> String {
    let strip = beats_json(store, composition, None);
    let strip = strip.trim_start_matches('{').trim_end_matches('}');
    format!(
        "{{\"saved\":true,\"action\":\"{action}\",\"detail\":\"{}\",{strip}}}",
        json_escape(detail)
    )
}

/// Clip visuals (Sprint 16, T007 / CAP-008, J-03, REC-005): what each
/// clip holds, read from the same composition data the render uses —
/// never separate state. Video clips show filmstrip thumbnails (true
/// frame indexes across the window, served by `/api/frame`), audio
/// clips show waveforms with peak marks measured from the session's
/// render mix over the clip window, text clips show their content,
/// and badges mark volume, effects, and transitions with fade handles
/// on edges. A clip whose media cannot show (a missing source file, a
/// missing nest target, no audible mix) shows a named placeholder box
/// naming its cause — never a guessed picture and never a silent gap.
///
/// Waveform buckets per audio strip: enough columns to read rhythm at
/// a glance without bloating the timeline payload.
pub const WAVEFORM_BUCKETS: usize = 32;

/// Mix sample rate the waveform math assumes (interleaved stereo s16,
/// matching the render mix): one frame is two samples.
pub const WAVEFORM_RATE: f64 = 48_000.0;

/// Filmstrip frames per video strip: true frame indexes evenly spaced
/// across the clip window, capped so long clips stay light.
pub const FILMSTRIP_FRAMES: usize = 8;

/// True frame indexes evenly spaced across one clip window (Sprint 16,
/// T007): the filmstrip thumbnails the page fetches from `/api/frame`.
/// Stills (image clips) show the single frame at the window open.
fn filmstrip_frames(start: f64, end: f64, fps_value: f64, frame_count: u64) -> Vec<u64> {
    if !fps_value.is_finite() || fps_value <= 0.0 || frame_count == 0 {
        return Vec::new();
    }
    let last = frame_count.saturating_sub(1);
    let dur = (end - start).max(0.0);
    if dur <= 0.0 {
        return vec![((start * fps_value).floor() as u64).min(last)];
    }
    let n = ((dur * fps_value).ceil() as usize).clamp(1, FILMSTRIP_FRAMES);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let t = start + (k as f64 + 0.5) * dur / n as f64;
        out.push((t * fps_value).floor().max(0.0) as u64);
    }
    let mut deduped = Vec::with_capacity(out.len());
    for f in out {
        let f = f.min(last);
        if deduped.last() != Some(&f) {
            deduped.push(f);
        }
    }
    deduped
}

/// Per-bucket waveform peaks over one clip window (Sprint 16, T007):
/// the largest absolute sample per bucket as linear 0..1 (three
/// decimals), measured from the session's render mix — the same bytes
/// the full render mixes — so the strip shows what the window plays.
fn waveform_peaks(mix: &[i16], start: f64, end: f64) -> Vec<f64> {
    const CHANNELS: usize = 2;
    let frames = mix.len() / CHANNELS;
    let mut out = Vec::with_capacity(WAVEFORM_BUCKETS);
    for b in 0..WAVEFORM_BUCKETS {
        let from = ((start + (end - start) * b as f64 / WAVEFORM_BUCKETS as f64) * WAVEFORM_RATE)
            .round()
            .max(0.0) as usize;
        let to = ((start + (end - start) * (b + 1) as f64 / WAVEFORM_BUCKETS as f64)
            * WAVEFORM_RATE)
            .round()
            .max(0.0) as usize;
        let from = from.min(frames);
        let to = to.min(frames);
        let mut peak: u32 = 0;
        if to > from {
            for f in from..to {
                let base = f * CHANNELS;
                if base + 1 < mix.len() {
                    peak = peak
                        .max(mix[base].unsigned_abs() as u32)
                        .max(mix[base + 1].unsigned_abs() as u32);
                }
            }
        }
        let linear = peak as f64 / 32768.0;
        out.push((linear * 1000.0).round() / 1000.0);
    }
    out
}

/// dBFS of one linear peak for the waveform peak mark, rounded to one
/// decimal; digital silence (`0`) has no finite level and reports
/// `None`.
fn peak_dbfs(peak: u32) -> Option<f64> {
    if peak == 0 {
        return None;
    }
    let db = 20.0 * (peak as f64 / 32768.0).log10();
    Some((db * 10.0).round() / 10.0)
}

/// One media path resolved against the project folder (Sprint 16,
/// T007 / REC-005): absolute paths read as-is, relative paths from
/// the folder. A missing file makes the clip unrenderable.
fn media_present(folder: &Path, path: &str) -> bool {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.exists()
    } else {
        folder.join(candidate).exists()
    }
}

/// Display name of one clip for a placeholder box: the stable `id`
/// when declared, else `kind#index`.
fn clip_display_name(kind: &str, id: Option<&str>, index: usize) -> String {
    match id {
        Some(id) => id.to_string(),
        None => format!("{kind}#{index}"),
    }
}

/// Render one clip strip as JSON (Sprint 16, T007): the filmstrip,
/// waveform, text, placeholder, or `none` readout of what the clip
/// holds. `mix` carries the session's render mix PCM (interleaved
/// stereo s16 at 48 kHz); `mix_broken` names the mix failure when the
/// mix cannot decode.
#[allow(clippy::too_many_arguments)]
fn clip_strip_json(
    all_names: &[String],
    folder: &Path,
    clip: &notation::model::Clip,
    index: usize,
    mix: Option<&[i16]>,
    mix_broken: Option<&str>,
    fps_value: f64,
    frame_count: u64,
) -> String {
    use notation::model::Clip;
    let kind = clip.kind();
    let name = clip_display_name(kind, clip.id(), index);
    let (start, end) = (clip.start(), clip.start() + clip.duration());
    match clip {
        Clip::Video(c) => {
            if !media_present(folder, &c.path) {
                return placeholder_json(
                    &name,
                    &format!(
                        "source {:?} is missing; restore the file to show thumbnails.",
                        c.path
                    ),
                );
            }
            let frames = filmstrip_frames(start, end, fps_value, frame_count);
            let list = frames
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("{{\"kind\":\"filmstrip\",\"frames\":[{list}]}}")
        }
        Clip::Image(c) => {
            if !media_present(folder, &c.path) {
                return placeholder_json(
                    &name,
                    &format!(
                        "source {:?} is missing; restore the file to show the thumbnail.",
                        c.path
                    ),
                );
            }
            let frames = filmstrip_frames(start, start, fps_value, frame_count);
            let list = frames
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("{{\"kind\":\"filmstrip\",\"frames\":[{list}]}}")
        }
        Clip::Nest(c) => {
            if !all_names.iter().any(|n| n == &c.target) {
                return placeholder_json(
                    &name,
                    &format!(
                        "nest target {:?} is missing from the project; point the nest at a composition to show thumbnails.",
                        c.target
                    ),
                );
            }
            let frames = filmstrip_frames(start, end, fps_value, frame_count);
            let list = frames
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("{{\"kind\":\"filmstrip\",\"frames\":[{list}]}}")
        }
        Clip::Audio(c) => {
            if !media_present(folder, &c.path) {
                return placeholder_json(
                    &name,
                    &format!(
                        "source {:?} is missing; restore the file to show the waveform.",
                        c.path
                    ),
                );
            }
            match mix {
                Some(pcm) => {
                    let peaks = waveform_peaks(pcm, start, end);
                    let max = peaks
                        .iter()
                        .fold(0.0f64, |a, b| a.max(*b));
                    let peak_u32 = (max * 32768.0).round() as u32;
                    let dbfs = match peak_dbfs(peak_u32) {
                        Some(db) => format!("{db:.1}"),
                        None => "null".to_string(),
                    };
                    let marks = peaks
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| **p >= max && max > 0.0)
                        .map(|(i, _)| i.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    let list = peaks
                        .iter()
                        .map(|p| format!("{p:.3}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(
                        "{{\"kind\":\"waveform\",\"buckets\":{WAVEFORM_BUCKETS},\"peaks\":[{list}],\"peak\":{peak_u32},\"peak_dbfs\":{dbfs},\"peak_buckets\":[{marks}]}}"
                    )
                }
                None => placeholder_json(
                    &name,
                    &match mix_broken {
                        Some(why) => format!("the render mix cannot decode ({why}); fix the source to show the waveform."),
                        None => "the project carries no audible sound in this window; silent clips show no waveform.".to_string(),
                    },
                ),
            }
        }
        Clip::Text(c) => {
            let mut content: String = c.value.chars().take(160).collect();
            if c.value.chars().count() > 160 {
                content.push('\u{2026}');
            }
            format!(
                "{{\"kind\":\"text\",\"content\":\"{}\"}}",
                json_escape(&content)
            )
        }
        _ => "{\"kind\":\"none\"}".to_string(),
    }
}

/// Render one named placeholder box as JSON (Sprint 16, T007 /
/// REC-005): the clip name with the cause — never a guessed picture.
fn placeholder_json(name: &str, reason: &str) -> String {
    format!(
        "{{\"kind\":\"placeholder\",\"name\":\"{}\",\"reason\":\"{}\"}}",
        json_escape(name),
        json_escape(reason)
    )
}

/// Render the badge set of one clip as JSON (Sprint 16, T007): volume
/// (audio-bearing clips with a non-default gain), named effects,
/// VFX presence, and the transitions bridging through the clip.
fn clip_badges_json(comp: &notation::model::Composition, clip: &notation::model::Clip) -> String {
    use notation::model::Clip;
    let (volume_db, muted) = match clip {
        Clip::Video(c) => (Some(c.volume_db), c.volume_db == f64::NEG_INFINITY),
        Clip::Audio(c) => (Some(c.volume_db), c.volume_db == f64::NEG_INFINITY),
        _ => (None, false),
    };
    let volume = match volume_db {
        Some(_) if muted => "{\"db\":null,\"muted\":true}".to_string(),
        Some(db) if db != 0.0 => format!("{{\"db\":{db},\"muted\":false}}"),
        _ => "null".to_string(),
    };
    let effects = clip
        .effects()
        .iter()
        .map(|e| format!("\"{}\"", json_escape(e.preset.name())))
        .collect::<Vec<_>>()
        .join(",");
    let vfx = !clip.vfx().is_empty();
    let mut bridges = Vec::new();
    if let Some(id) = clip.id() {
        for t in &comp.transitions {
            if t.from == id {
                bridges.push(format!(
                    "{{\"with\":\"{}\",\"side\":\"out\",\"kind\":\"{}\",\"duration\":{:.3}}}",
                    json_escape(&t.to),
                    t.kind.name(),
                    t.duration
                ));
            }
            if t.to == id {
                bridges.push(format!(
                    "{{\"with\":\"{}\",\"side\":\"in\",\"kind\":\"{}\",\"duration\":{:.3}}}",
                    json_escape(&t.from),
                    t.kind.name(),
                    t.duration
                ));
            }
        }
    }
    format!(
        "{{\"volume_db\":{volume},\"effects\":[{effects}],\"vfx\":{vfx},\"transitions\":[{}]}}",
        bridges.join(",")
    )
}

/// Render the fade handles of one clip as JSON (Sprint 16, T007):
/// the fade seconds with the handle frames, so edge drags land
/// exactly. Non-audio-bearing clips carry no fades.
fn clip_fades_json(clip: &notation::model::Clip, fps_value: f64, frame_count: u64) -> String {
    use notation::model::Clip;
    let (fade_in, fade_out, start, end) = match clip {
        Clip::Video(c) => (c.fade_in, c.fade_out, c.start, c.start + c.duration),
        Clip::Audio(c) => (c.fade_in, c.fade_out, c.start, c.start + c.duration),
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    let frame_at = |t: f64| -> Option<u64> {
        if fps_value > 0.0 && frame_count > 0 {
            Some(((t * fps_value).floor().max(0.0) as u64).min(frame_count.saturating_sub(1)))
        } else {
            None
        }
    };
    let opt = |v: Option<u64>| match v {
        Some(f) => f.to_string(),
        None => "null".to_string(),
    };
    let (in_frame, out_frame) = if fade_in > 0.0 || fade_out > 0.0 {
        (frame_at(start + fade_in), frame_at(end - fade_out))
    } else {
        (None, None)
    };
    format!(
        "{{\"in\":{fade_in:.3},\"in_frame\":{},\"out\":{fade_out:.3},\"out_frame\":{}}}",
        opt(in_frame),
        opt(out_frame)
    )
}

/// Render clip visuals as JSON for the Studio visuals API (Sprint 16,
/// T007): one strip, badge set, and fade handle set per clip, in
/// document order, with the window the strip spans. Every visual reads
/// composition data; a clip whose media cannot show carries a named
/// placeholder box instead.
#[allow(clippy::too_many_arguments)]
pub fn clip_visuals_json(
    comp: &notation::model::Composition,
    all_names: &[String],
    folder: &Path,
    mix: Option<&[i16]>,
    mix_broken: Option<&str>,
    fps_value: f64,
    frame_count: u64,
) -> String {
    let mut clips = String::new();
    for (index, clip) in comp.clips.iter().enumerate() {
        if !clips.is_empty() {
            clips.push(',');
        }
        let id = match clip.id() {
            Some(id) => format!("\"{}\"", json_escape(id)),
            None => "null".to_string(),
        };
        let label = match clip.label() {
            Some(label) => format!("\"{}\"", json_escape(label)),
            None => "null".to_string(),
        };
        let strip = clip_strip_json(
            all_names,
            folder,
            clip,
            index,
            mix,
            mix_broken,
            fps_value,
            frame_count,
        );
        let badges = clip_badges_json(comp, clip);
        let fades = clip_fades_json(clip, fps_value, frame_count);
        clips.push_str(&format!(
            "{{\"index\":{},\"kind\":\"{}\",\"id\":{},\"label\":{},\"track\":\"{}\",\"hidden\":{},\"window\":[{:.3},{:.3}],\"strip\":{strip},\"badges\":{badges},\"fades\":{fades}}}",
            index,
            json_escape(clip.kind()),
            id,
            label,
            json_escape(clip.track().name()),
            clip.hidden(),
            clip.start(),
            clip.start() + clip.duration()
        ));
    }
    format!(
        "{{\"composition\":\"{}\",\"fps\":{fps_value},\"frame_count\":{frame_count},\"clips\":[{clips}]}}",
        json_escape(&comp.name),
    )
}

/// External-change conflict handling (Sprint 16, T003 / CAP-003, J-05,
/// STATE-04, REC-002): a project file changing on disk while Studio
/// holds unsaved work banners the disk version against the unsaved
/// Studio version; the maker's pick applies and the losing version
/// stays in history; hot reload applies automatically only while no
/// unsaved work is held. Every pick lands as a validated history turn
/// through the same SDK patch and history machinery as every other
/// visual edit, so one undo system keeps serving both versions.
/// Style attributes stay CSS-like hyphenated end to end, and
/// `font-family` try-order lists pass through untouched (fail-closed
/// at render time, never resolved here).
///
/// Why a conflict check refused: the reason with both versions kept,
/// so nothing ever wins silently.
fn refused_conflict(message: String) -> String {
    format!("studio refused: {message}")
}

/// Composition file name watched for external changes (Sprint 16,
/// T003): the same single file the preview hot-reload watch follows.
pub fn studio_composition_file(folder: &Path) -> PathBuf {
    folder.join(validate::validate::COMPOSITION_FILE)
}

/// Fingerprint of the composition sources on disk (Sprint 16, T003):
/// presence, the modification stamp, the byte length, and a content
/// hash. The stamp and length detect change cheaply; the hash confirms
/// it, so a same-tick rewrite still banners instead of reloading
/// silent, and a mere touch (same bytes, new stamp) never banners.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskVersion {
    /// False when the composition file is missing or unreadable.
    pub present: bool,
    /// Seconds since the Unix epoch behind the last modification, when known.
    pub modified_secs: Option<u64>,
    /// Sub-second nanos behind the last modification (with the stamp,
    /// full precision so a same-length rewrite still trips the watch).
    pub modified_nanos: u32,
    /// Byte length of the composition file (0 when gone).
    pub len: u64,
    /// Content hash of the composition file (0 when gone).
    pub hash: u64,
}

impl DiskVersion {
    /// Cheap stat-only signature (Sprint 16, T003): presence, full
    /// stamp, and length without reading file bytes, for the hot-path
    /// check.
    pub fn stat_sig(&self) -> (bool, Option<(u64, u32)>, u64) {
        (
            self.present,
            self.modified_secs.map(|s| (s, self.modified_nanos)),
            self.len,
        )
    }
}

/// Fingerprint the composition sources on disk (Sprint 16, T003): a
/// missing or unreadable file fingerprints as gone, never refusing —
/// the pick flow reports it instead.
pub fn disk_version_of(folder: &Path) -> DiskVersion {
    let path = studio_composition_file(folder);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return DiskVersion {
                present: false,
                modified_secs: None,
                modified_nanos: 0,
                len: 0,
                hash: 0,
            };
        }
    };
    let stamp = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_secs(), d.subsec_nanos()));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    DiskVersion {
        present: true,
        modified_secs: stamp.map(|(s, _)| s),
        modified_nanos: stamp.map(|(_, n)| n).unwrap_or(0),
        len: bytes.len() as u64,
        hash: hasher.finish(),
    }
}

/// Cheap stat-only signature of the disk sources (Sprint 16, T003):
/// presence, full modification stamp, and length without reading bytes.
pub fn disk_stat_sig(folder: &Path) -> (bool, Option<(u64, u32)>, u64) {
    let path = studio_composition_file(folder);
    match std::fs::metadata(&path) {
        Ok(m) => {
            let stamp = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| (d.as_secs(), d.subsec_nanos()));
            (true, stamp, m.len())
        }
        Err(_) => (false, None, 0),
    }
}

/// One pending external change (Sprint 16, T003): the disk fingerprint
/// behind the banner. The staged (unsaved) side stays live in the
/// save queue; the notice renders both sides together.
#[derive(Debug, Clone)]
pub struct StudioConflict {
    /// Fingerprint of the changed disk sources.
    pub disk: DiskVersion,
}

/// Describe one side pair for the banner (Sprint 16, T003): the disk
/// version against the unsaved Studio version, naming both.
pub fn conflict_notice(folder: &Path, disk: &DiskVersion, staged_summaries: &[String]) -> String {
    let disk_side = if disk.present {
        format!(
            "the disk version of composition.kdl ({} byte(s), modified {}s after the epoch, change {:016x})",
            disk.len,
            disk.modified_secs
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            disk.hash
        )
    } else {
        "the disk version is gone: composition.kdl is missing or unreadable".to_string()
    };
    let staged_side = if staged_summaries.is_empty() {
        "no staged edit".to_string()
    } else {
        staged_summaries.join("; ")
    };
    format!(
        "studio: composition.kdl changed on disk in {} while {} staged edit(s) wait ({}) — {} against the unsaved Studio version ({}). Pick one at POST /api/studio/conflict (`{{\"pick\":\"disk\"}}` or `{{\"pick\":\"unsaved\"}}`); neither applies until you pick.",
        folder.display(),
        staged_summaries.len(),
        staged_side,
        disk_side,
        staged_side
    )
}

/// Render one disk fingerprint as JSON for the conflict API.
pub fn disk_version_json(disk: &DiskVersion) -> String {
    let modified = match disk.modified_secs {
        Some(s) => s.to_string(),
        None => "null".to_string(),
    };
    format!(
        "{{\"present\":{},\"file\":\"composition.kdl\",\"bytes\":{},\"modified_secs\":{},\"hash\":\"{:016x}\"}}",
        disk.present, disk.len, modified, disk.hash
    )
}

/// Render the conflict state as JSON for the conflict API (Sprint 16,
/// T003): `{"conflict":false}` while clean, both versions plus the
/// banner notice while pending.
pub fn conflict_json(
    folder: &Path,
    conflict: Option<&StudioConflict>,
    staged_summaries: &[String],
) -> String {
    match conflict {
        None => "{\"conflict\":false}".to_string(),
        Some(pending) => {
            let edits = staged_summaries
                .iter()
                .map(|s| format!("\"{}\"", json_escape(s)))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"conflict\":true,\"disk\":{},\"unsaved\":{{\"staged\":{},\"edits\":[{}]}},\"notice\":\"{}\"}}",
                disk_version_json(&pending.disk),
                staged_summaries.len(),
                edits,
                json_escape(&conflict_notice(folder, &pending.disk, staged_summaries))
            )
        }
    }
}

/// Which version the maker's pick keeps (Sprint 16, T003): the disk
/// file wins or the unsaved Studio version wins — the loser always
/// stays in history either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPick {
    /// Reload the disk file; the unsaved version stays in history.
    Disk,
    /// Apply the staged edits; the disk version stays in history.
    Unsaved,
}

impl ConflictPick {
    /// The served spelling (`disk` or `unsaved`).
    pub fn as_str(self) -> &'static str {
        match self {
            ConflictPick::Disk => "disk",
            ConflictPick::Unsaved => "unsaved",
        }
    }

    /// Parse one pick spelling; anything else refuses so a typo never
    /// silently resolves the conflict.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "disk" => Some(ConflictPick::Disk),
            "unsaved" => Some(ConflictPick::Unsaved),
            _ => None,
        }
    }
}

/// One parsed conflict pick (Sprint 16, T003): the winning side plus
/// the actor the validated turn records.
#[derive(Debug, Clone)]
pub struct ConflictPickRequest {
    /// The winning side.
    pub pick: ConflictPick,
    /// Actor the validated turn is recorded under (human or agent).
    pub actor: String,
}

/// Parse one conflict pick body
/// (`{"pick":"disk","actor":"maker"}`): the pick names `disk` or
/// `unsaved` and the actor is non-empty, since resolving records a
/// named history turn. A refused body names its cause and changes
/// nothing.
pub fn parse_conflict_pick_body(body: &str) -> Result<ConflictPickRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_conflict(
            "the conflict pick needs a JSON pick body (`{\"pick\":\"disk\",\"actor\":\"maker\"}` or `{\"pick\":\"unsaved\",\"actor\":\"maker\"}`); pass the winning version. Nothing changed.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_conflict(format!(
            "the conflict pick body is not JSON ({e}). Pass `{{\"pick\":\"disk\",\"actor\":\"maker\"}}` or `{{\"pick\":\"unsaved\",\"actor\":\"maker\"}}`. Nothing changed."
        ))
    })?;
    let raw = value.get("pick").and_then(|p| p.as_str()).unwrap_or("");
    let pick = ConflictPick::parse(raw).ok_or_else(|| {
        refused_conflict(format!(
            "unknown conflict pick {raw:?}: the shell offers the disk version against the unsaved Studio version. Pass `disk` or `unsaved`. Nothing changed."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_conflict(
            "the conflict pick names no actor; resolving records a named history turn (human or agent). Pass a non-empty `actor`. Nothing changed.".to_string(),
        ));
    }
    Ok(ConflictPickRequest { pick, actor })
}

/// Receipt of one applied conflict pick (Sprint 16, T003): the winning
/// side as a validated turn plus where the losing version now lives.
#[derive(Debug, Clone)]
pub struct ConflictResolution {
    /// The winning side.
    pub pick: ConflictPick,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// Where the losing version now lives and how to bring it back.
    pub kept: String,
}

/// Render one applied pick as JSON for the conflict API.
pub fn conflict_resolved_json(resolution: &ConflictResolution) -> String {
    format!(
        "{{\"resolved\":true,\"pick\":\"{}\",\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}],\"kept\":\"{}\"}}",
        resolution.pick.as_str(),
        resolution.turn,
        json_escape(&resolution.actor),
        resolution
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(","),
        json_escape(&resolution.kept)
    )
}

/// Apply one conflict pick through the Sprint 15 patch and history
/// machinery (Sprint 16, T003 / CAP-003, REC-002): the staged edits
/// apply onto the fresh disk sources as one validated turn, so both
/// sides survive whatever the maker chooses. Picking `unsaved` leaves
/// the turn in effect — the disk version rests in the turn's backup
/// and history undo brings it back. Picking `disk` steps the turn back
/// at once — the unsaved version rests in the turn's backup and
/// history restore brings it back. A pick that cannot apply (the disk
/// moved out from under the staged edits, the guard is held, history
/// is unwritable) reports why while the disk file, the staged edits,
/// and the pending conflict all stay exactly as they were.
pub fn apply_conflict_pick(
    folder: &Path,
    composition: Option<&str>,
    pick: ConflictPick,
    actor: &str,
    staged: &[QueuedEdit],
) -> Result<ConflictResolution, String> {
    if staged.is_empty() {
        return Err(refused_conflict(
            "no unsaved work is held; there is nothing to set against the disk version. Nothing changed.".to_string(),
        ));
    }
    if !project_reachable(folder) {
        return Err(refused_conflict(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and pick again. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let edit = StudioEdit {
        actor: actor.to_string(),
        source: staged[0].source,
        ops: staged.iter().map(|q| q.op.clone()).collect(),
    };
    // Turn sequences start at zero, so a zero receipt is a real turn:
    // count turns before and after to learn whether the pick recorded
    // one. The pick's turn is the newest sequence afterwards.
    let turns_before = sdk::history::list_turns(folder)
        .map(|listing| listing.turns.len())
        .unwrap_or(0);
    let receipt = match apply_studio_edit(folder, composition, &edit) {
        Ok(receipt) => receipt,
        Err(why) => {
            let why = why.strip_prefix("studio refused: ").unwrap_or(&why);
            return Err(refused_conflict(format!(
                "the {} pick cannot apply: {why} Both versions are kept: the disk file is untouched and the {} staged edit(s) still wait. Fix the cause and pick again.",
                pick.as_str(),
                staged.len()
            )));
        }
    };
    let seq = match sdk::history::list_turns(folder) {
        Ok(listing) if listing.turns.len() == turns_before + 1 => {
            listing.turns.iter().map(|t| t.seq).max().unwrap_or(0)
        }
        _ => {
            return Err(refused_conflict(
                "the pick applied but history recorded no turn, so the losing version has nowhere to rest. Both versions are kept: the disk file holds its bytes and the staged edits persist in the delta. Nothing further was recorded.".to_string(),
            ));
        }
    };
    match pick {
        ConflictPick::Unsaved => Ok(ConflictResolution {
            pick,
            turn: seq,
            actor: receipt.actor.clone(),
            summaries: receipt.summaries.clone(),
            kept: format!(
                "the disk version is kept in turn {seq}'s backup; history undo brings it back"
            ),
        }),
        ConflictPick::Disk => match sdk::history::undo_turn(folder, seq) {
            Ok(_) => Ok(ConflictResolution {
                pick,
                turn: seq,
                actor: receipt.actor.clone(),
                summaries: receipt.summaries.clone(),
                kept: format!(
                    "the unsaved Studio version is kept in turn {seq}'s backup; history restore brings it back"
                ),
            }),
            Err(h) => Err(refused_conflict(format!(
                "the disk pick recorded turn {seq} but cannot step back to it: {h} Both versions are kept: the disk file is untouched and the unsaved version rests in turn {seq}'s backup. Nothing further was recorded."
            ))),
        },
    }
}

/// Slice of timeline rows behind a shortcut handoff selection (Sprint
/// 16, T008): the same rows the timeline panel paints, so the copied
/// agent request points at real clips.
pub type StudioRowList = [StudioRow];

/// Selection keys of one row slice: stable ids plus `#index` entries,
/// in row order — the same spellings deep links resolve.
pub fn selection_keys_of_rows(rows: &[StudioRow]) -> Vec<String> {
    let mut keys = Vec::new();
    for row in rows {
        keys.push(format!("#{}", row.index));
        if let Some(id) = &row.id {
            keys.push(id.clone());
        }
    }
    keys
}

/// Extra CSS the snap/zoom panel needs (Sprint 16, T005): the snap
/// bar, the status lines, and the vertical guide line the panel
/// paints at the snapped instant. Style attributes stay CSS-like
/// hyphenated end to end.
pub fn snap_zoom_css() -> String {
    r#"        #snap-zoom-bar{margin-top:8px}
        #snap-zoom-bar label{margin-right:8px;font-variant-numeric:tabular-nums}
        #snap-zoom-bar button{margin-right:4px}
        #snap-status,#zoom-status{font-variant-numeric:tabular-nums}
        #timeline{position:relative}
        #snap-guide{position:absolute;top:0;bottom:0;width:2px;background-color:var(--accent-color);border-radius:1px}"#.to_string()
}

/// The snap/zoom control block (Sprint 16, T005): the visible project
/// setting behind drag magnets (the on/off switch plus the threshold
/// in CSS pixels), the zoom controls (fit, manual percent with cursor
/// or playhead anchor, zoom to selection), and the status lines plus
/// the guide line element the panel paints.
pub fn snap_zoom_html() -> String {
    r#"<div class="dock-bar" id="snap-zoom-bar">
        <label>snap <input id="snap-enabled" type="checkbox" checked></label>
        <label>threshold <input id="snap-threshold" type="number" value="8" step="1" min="1"></label>
        <button id="snap-save" type="button">set snap</button>
        <button id="zoom-fit" type="button">zoom fit</button>
        <label>zoom <input id="zoom-percent" type="number" value="100" step="5" min="10"></label>
        <label>anchor <select id="zoom-anchor"><option value="playhead">playhead</option><option value="cursor">cursor</option></select></label>
        <button id="zoom-apply" type="button">apply zoom</button>
        <button id="zoom-selection" type="button">zoom to selection</button>
        </div>
        <p id="snap-status" role="status">snapping on</p>
        <p id="zoom-status" role="status">zoom fit</p>
        <div id="snap-guide" hidden></div>"#.to_string()
}

/// The snap/zoom panel script (Sprint 16, T005): boots the visible
/// setting and the zoom preference, paints the guide line at the
/// snapped instant, and routes select drags through the snap setting
/// before they apply as named history turns.
pub fn snap_zoom_js() -> String {
    r#"        let snapState = null; let zoomState = null; let pxPerSec = 100;
        function snapStatusText(t) { const s = document.getElementById('snap-status'); if (s) s.textContent = t; }
        function zoomStatusText(t) { const s = document.getElementById('zoom-status'); if (s) s.textContent = t; }
        function timelineWidthPx() {
          const el = document.getElementById('timeline-body');
          if (el && el.getBoundingClientRect) {
            const w = el.getBoundingClientRect().width;
            if (w > 0) return w;
          }
          return 800;
        }
        function paintGuide(at) {
          const g = document.getElementById('snap-guide');
          if (!g) return;
          if (at === null || at === undefined) { g.hidden = true; return; }
          const dur = timelineDuration() || 1;
          g.hidden = false;
          g.style.left = 'calc(' + (at / dur * 100).toFixed(2) + '%)';
          g.title = 'snap guide at ' + Number(at).toFixed(3) + 's';
        }
        async function paintSnapZoom() {
          try {
            const s = await (await fetch('/api/studio/snap')).json();
            snapState = s;
            const box = document.getElementById('snap-enabled');
            if (box) box.checked = !!s.enabled;
            const th = document.getElementById('snap-threshold');
            if (th) th.value = s['threshold-px'];
            snapStatusText(s['status-line']);
            paintGuide(null);
          } catch (e) { snapStatusText('snap settings unavailable: ' + e); }
          try {
            const z = await (await fetch('/api/studio/zoom')).json();
            zoomState = z;
            const pct = document.getElementById('zoom-percent');
            if (pct) pct.value = Math.round(z.percent);
            const an = document.getElementById('zoom-anchor');
            if (an) an.value = z.anchor;
            zoomStatusText(z['status-line']);
          } catch (e) { zoomStatusText('zoom setting unavailable: ' + e); }
          pxPerSec = timelineWidthPx();
        }
        async function postSnap(enabled, thresholdPx) {
          const payload = {};
          if (enabled !== null && enabled !== undefined) payload.enabled = enabled;
          if (thresholdPx !== null && thresholdPx !== undefined) payload['threshold-px'] = thresholdPx;
          try {
            const r = await fetch('/api/studio/snap', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(payload)});
            const t = await r.text();
            if (!r.ok) { snapStatusText(t); banner(t); return; }
            snapState = JSON.parse(t);
            snapStatusText(snapState['status-line']);
          } catch (e) { snapStatusText('snap save failed: ' + e); }
        }
        async function resolveDrag(want) {
          try {
            const r = await fetch('/api/studio/snap-resolve', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({'want': want, 'playhead': playheadSecs(), 'px-per-sec': pxPerSec})});
            const t = await r.text();
            if (!r.ok) { snapStatusText(t); return {snapped: want, did_snap: false}; }
            const j = JSON.parse(t);
            snapStatusText(j['status-line']);
            paintGuide(j.did_snap ? j.guide_at : null);
            return j;
          } catch (e) { return {snapped: want, did_snap: false}; }
        }
        async function postZoom(payload) {
          try {
            const r = await fetch('/api/studio/zoom', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(payload)});
            const t = await r.text();
            if (!r.ok) { zoomStatusText(t); banner(t); return; }
            const j = JSON.parse(t);
            zoomState = {mode: j.saved.mode, percent: j.saved.percent, anchor: j.saved.anchor};
            pxPerSec = j['px-per-sec'];
            zoomStatusText(j.detail);
            const pct = document.getElementById('zoom-percent');
            if (pct) pct.value = Math.round(j.percent);
          } catch (e) { zoomStatusText('zoom failed: ' + e); }
        }
        const baseCallTool = callTool;
        callTool = async (payload) => {
          try {
            if (payload && payload.tool === 'select') {
              pxPerSec = timelineWidthPx();
              for (const key of ['to', 'start', 'end']) {
                if (payload[key] !== undefined && payload[key] !== null) {
                  const j = await resolveDrag(Number(payload[key]));
                  payload[key] = j.snapped;
                }
              }
            }
          } catch (e) {}
          return baseCallTool(payload);
        };
        document.getElementById('snap-save').addEventListener('click', () => {
          const box = document.getElementById('snap-enabled');
          const th = document.getElementById('snap-threshold');
          postSnap(box ? box.checked : null, th ? Number(th.value) : null);
        });
        document.getElementById('snap-enabled').addEventListener('change', (e) => postSnap(e.target.checked, null));
        document.getElementById('zoom-fit').addEventListener('click', () => postZoom({mode: 'fit', 'viewport-px': timelineWidthPx()}));
        document.getElementById('zoom-apply').addEventListener('click', () => {
          const pct = document.getElementById('zoom-percent');
          const an = document.getElementById('zoom-anchor');
          postZoom({mode: 'manual', percent: pct ? Number(pct.value) : 100, anchor: an ? an.value : 'playhead', 'anchor-at': playheadSecs(), 'viewport-px': timelineWidthPx()});
        });
        document.getElementById('zoom-selection').addEventListener('click', () => postZoom({mode: 'selection', selection: selection.slice(), 'viewport-px': timelineWidthPx()}));"#.to_string()
}

/// Extra CSS the clip-editing panel needs (Sprint 16, T006): the clip
/// gesture bar, the gap highlight rows, the link group cards with the
/// `out-of-sync` badge, and the clip status line. Style attributes
/// stay CSS-like hyphenated end to end.
pub fn clips_css() -> String {
    r#"        #clip-tools{margin-top:8px}
        #clip-tools h3{margin:6px 0;font-size:13px}
        #clip-tools h4{margin:6px 0 2px 0;font-size:12px}
        #clip-tools label{margin-right:8px;font-variant-numeric:tabular-nums}
        #clip-tools button{margin:2px 4px 2px 0}
        #clip-tools input{margin-right:4px;max-width:88px}
        #gap-list,#link-list{margin-top:8px}
        .gap-row{display:flex;gap:8px;align-items:baseline;font-variant-numeric:tabular-nums}
        .gap-row button{margin:2px}
        .link-group{border:1px solid var(--line-color);border-radius:4px;padding:4px 8px;margin:4px 0}
        .link-group[data-in-sync="0"]{border-style:dashed}
        .out-of-sync{display:inline-block;background-color:var(--banner-background);color:var(--banner-color);border:1px solid var(--line-color);border-radius:4px;padding:0 6px;margin:2px}
        #clip-status{font-variant-numeric:tabular-nums}"#.to_string()
}

/// The clip-editing control block (Sprint 16, T006): move and trim
/// inputs for the selection, multi-select, multi-drag with its shift,
/// the marquee window, delete, the ripple switch, gap highlights with
/// per-gap and close-all actions, and the link, unlink, detach-audio,
/// and merge-back buttons plus the linked-set list.
pub fn clips_html() -> String {
    r#"<div id="clip-tools">
        <h3>clip edits</h3>
        <div>
        <label>move to <input id="clip-to" type="number" step="0.1" min="0"></label>
        <button id="clip-move" type="button">move selection</button>
        <label>trim start <input id="clip-start" type="number" step="0.1" min="0"></label>
        <label>trim end <input id="clip-end" type="number" step="0.1" min="0"></label>
        <button id="clip-trim" type="button">trim selection</button>
        </div>
        <div>
        <label>shift <input id="clip-delta" type="number" step="0.1" value="0.5"></label>
        <button id="clip-multi-select" type="button">multi-select</button>
        <button id="clip-multi-drag" type="button">multi-drag</button>
        <button id="clip-delete" type="button">delete</button>
        <label><input id="clip-ripple" type="checkbox" checked> ripple</label>
        </div>
        <div>
        <label>marquee start <input id="clip-marquee-start" type="number" step="0.1" min="0"></label>
        <label>marquee end <input id="clip-marquee-end" type="number" step="0.1" min="0"></label>
        <button id="clip-marquee" type="button">marquee</button>
        </div>
        <div>
        <button id="clip-link" type="button">link</button>
        <button id="clip-unlink" type="button">unlink</button>
        <button id="clip-detach" type="button">detach-audio</button>
        <button id="clip-merge-back" type="button">merge-back</button>
        <button id="clip-close-all-gaps" type="button">close all gaps</button>
        </div>
        <p id="clip-status" role="status">clips ready</p>
        <div id="gap-list"></div>
        <div id="link-list"></div>
        </div>"#.to_string()
}

/// The clip-editing panel script (Sprint 16, T006): paints the gap
/// highlights with per-gap close actions and the link groups with the
/// `out-of-sync` badge exactly while offsets drifted, keeps the ripple
/// switch in view, and routes every clip gesture through the tool API
/// so each applied gesture lands as a named history turn.
pub fn clips_js() -> String {
    r#"        function clipStatus(t) { const s = document.getElementById('clip-status'); if (s) s.textContent = t; }
        function selectedClip() { return (selection && selection.length) ? selection[0] : null; }
        function clipNumber(id) { const el = document.getElementById(id); if (!el || el.value === '') return null; return Number(el.value); }
        async function paintClips() {
          const gaps = document.getElementById('gap-list');
          const links = document.getElementById('link-list');
          const rippleBox = document.getElementById('clip-ripple');
          try {
            const g = await (await fetch('/api/studio/gaps?' + studioQuery())).json();
            if (rippleBox) rippleBox.checked = !!g.ripple;
            clipStatus(g.ripple ? 'ripple on: deletes close each same-track gap' : 'ripple off: deletes leave gaps open');
            if (gaps) {
              gaps.innerHTML = '';
              const h = document.createElement('h4'); h.textContent = 'gaps (' + g.gaps.length + ')'; gaps.appendChild(h);
              for (const gap of g.gaps) {
                const row = document.createElement('div'); row.className = 'gap-row';
                const label = document.createElement('span');
                label.textContent = gap.track + ' ' + Number(gap.window[0]).toFixed(3) + 's..' + Number(gap.window[1]).toFixed(3) + 's';
                const shut = document.createElement('button'); shut.type = 'button'; shut.textContent = 'close gap';
                shut.addEventListener('click', () => callTool({tool: 'close-gap', track: gap.track, at: (gap.window[0] + gap.window[1]) / 2}));
                row.append(label, shut); gaps.appendChild(row);
              }
            }
          } catch (e) { if (gaps) gaps.textContent = 'gaps unavailable: ' + e; }
          try {
            const l = await (await fetch('/api/studio/links?' + studioQuery())).json();
            if (links) {
              links.innerHTML = '';
              const h = document.createElement('h4'); h.textContent = 'links'; links.appendChild(h);
              for (const group of l.groups) {
                const card = document.createElement('div'); card.className = 'link-group';
                card.dataset.inSync = group.in_sync ? '1' : '0';
                const names = document.createElement('strong');
                names.textContent = group.members.map((m) => m.id + (m.detached ? ' (detached)' : '')).join(' + ');
                card.appendChild(names);
                if (!group.in_sync && group.badge) {
                  const badge = document.createElement('span'); badge.className = 'out-of-sync'; badge.textContent = group.badge; card.appendChild(badge);
                }
                links.appendChild(card);
              }
              if (!l.groups.length) { const p = document.createElement('p'); p.textContent = 'no linked clips'; links.appendChild(p); }
            }
            if (l.notice) clipStatus(l.notice);
          } catch (e) { if (links) links.textContent = 'links unavailable: ' + e; }
        }
        function wireClipButton(id, fn) {
          const el = document.getElementById(id);
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('click', fn); }
        }
        wireClipButton('clip-move', () => {
          const k = selectedClip(); if (!k) { clipStatus('pick a clip first'); return; }
          const v = clipNumber('clip-to'); if (v === null || !isFinite(v)) { clipStatus('pass a move target in film seconds'); return; }
          callTool({tool: 'select', clip: k, to: v});
        });
        wireClipButton('clip-trim', () => {
          const k = selectedClip(); if (!k) { clipStatus('pick a clip first'); return; }
          const payload = {tool: 'select', clip: k};
          const s = clipNumber('clip-start'); const e = clipNumber('clip-end');
          if (s !== null) payload.start = s;
          if (e !== null) payload.end = e;
          if (s === null && e === null) { clipStatus('pass a trim start, end, or both'); return; }
          callTool(payload);
        });
        wireClipButton('clip-multi-select', () => {
          if (!selection.length) { clipStatus('pick clips first'); return; }
          callTool({tool: 'multi-select', clips: selection.slice()});
        });
        wireClipButton('clip-multi-drag', () => {
          if (!selection.length) { clipStatus('pick clips first'); return; }
          const d = clipNumber('clip-delta'); if (d === null || !isFinite(d)) { clipStatus('pass a shift in film seconds'); return; }
          callTool({tool: 'multi-drag', clips: selection.slice(), delta: d});
        });
        wireClipButton('clip-marquee', () => {
          const s = clipNumber('clip-marquee-start'); const e = clipNumber('clip-marquee-end');
          if (s === null || e === null) { clipStatus('pass the marquee window start and end'); return; }
          callTool({tool: 'marquee', start: s, end: e});
        });
        wireClipButton('clip-delete', () => {
          if (!selection.length) { clipStatus('pick clips first'); return; }
          if (selection.length === 1) callTool({tool: 'delete', clip: selection[0]});
          else callTool({tool: 'delete', clips: selection.slice()});
        });
        wireClipButton('clip-link', () => {
          if (selection.length < 2) { clipStatus('pick at least two clips to link'); return; }
          callTool({tool: 'link', clips: selection.slice()});
        });
        wireClipButton('clip-unlink', () => {
          if (!selection.length) { clipStatus('pick linked clips first'); return; }
          if (selection.length === 1) callTool({tool: 'unlink', clip: selection[0]});
          else callTool({tool: 'unlink', clips: selection.slice()});
        });
        wireClipButton('clip-detach', () => {
          const k = selectedClip(); if (!k) { clipStatus('pick a linked clip first'); return; }
          callTool({tool: 'detach-audio', clip: k});
        });
        wireClipButton('clip-merge-back', () => {
          if (selection.length !== 2) { clipStatus('pick exactly two linked clips to merge back'); return; }
          callTool({tool: 'merge-back', clips: selection.slice(0, 2)});
        });
        wireClipButton('clip-close-all-gaps', () => callTool({tool: 'close-all-gaps'}));
        (function () {
          const box = document.getElementById('clip-ripple');
          if (box && !box.dataset.wired) {
            box.dataset.wired = '1';
            box.addEventListener('change', () => callTool({tool: 'set-ripple', ripple: box.checked}));
          }
        })();"#.to_string()
}
