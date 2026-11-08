//! Remappable Studio shortcuts (Sprint 16, T008 / CAP-009, J-04,
//! INT-003, REC-004).
//!
//! The full default map drives playback, the work area, tools, edit,
//! and panels; a project-level remap overrides single keys and chords
//! alike, takes effect immediately, and leaves the default map visible
//! underneath; a describe-a-change action copies a ready agent request
//! from the current selection. Every visual timeline edit a shortcut
//! triggers applies through the Sprint 15 patch and history machinery
//! (one undo system); the remap file lives in the hidden history
//! directory next to the layout file — never beside the sources and
//! never outside the project — so render scratch and caches stay
//! outside the project, as before, and default renders stay
//! byte-identical.
//!
//! Request bodies and the remap file use hyphenated keys (`all-tracks`
//! style); chords are plain key spellings (`space`, `ctrl+z`,
//! `shift+arrow-left`), never style attributes, so the CSS-like
//! hyphenated vocabulary and the fail-closed `font-family` comma
//! try-order pass through untouched.

use std::path::{Path, PathBuf};

/// Shortcut remap file name inside the hidden project directory (next
/// to the layout, theme, work-area, and beat files — never beside the
/// sources).
pub const STUDIO_SHORTCUTS_FILE: &str = "studio-shortcuts.json";

/// Version of the persisted shortcut remap.
pub const STUDIO_SHORTCUTS_VERSION: u32 = 1;

/// Hidden remap path for one project folder.
pub fn shortcuts_path_for(folder: &Path) -> PathBuf {
    folder
        .join(sdk::HISTORY_DIR_NAME)
        .join(STUDIO_SHORTCUTS_FILE)
}

/// Prefix every shortcut refusal carries; the cause follows it.
fn refused_shortcut(message: String) -> String {
    format!("studio refused: {message}")
}

/// One shortcut area (Sprint 16, T008): the five areas the default map
/// covers, so every default shortcut acts in its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutArea {
    /// Transport drive: play, pause, step, seek ends, loop, speed.
    Playback,
    /// In/out work area bounding playback and export ranges.
    WorkArea,
    /// Timeline tools: select, razor, split, picks.
    Tools,
    /// Edits through the shared undo system plus the agent handoff.
    Edit,
    /// Shell panels: theme, layout reset, panel focus.
    Panels,
}

impl ShortcutArea {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ShortcutArea::Playback => "playback",
            ShortcutArea::WorkArea => "work-area",
            ShortcutArea::Tools => "tools",
            ShortcutArea::Edit => "edit",
            ShortcutArea::Panels => "panels",
        }
    }

    /// Every served spelling, for refusal messages.
    pub const ALL: [&'static str; 5] = ["playback", "work-area", "tools", "edit", "panels"];
}

/// One shortcut action (Sprint 16, T008): the full default map across
/// the five areas. Tool-mode and focus actions resolve on the server
/// and apply as panel state through the page; every other action
/// executes against the serving session (transport, work area, SDK
/// patch and history machinery, theme, layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    /// Toggle play/pause over the session playhead.
    PlayToggle,
    /// Step one frame forward.
    StepForward,
    /// Step one frame back.
    StepBack,
    /// Park the playhead on frame 0.
    GoToStart,
    /// Park the playhead on the last frame.
    GoToEnd,
    /// Flip the loop switch.
    ToggleLoop,
    /// Raise the playback rate one notch.
    SpeedUp,
    /// Lower the playback rate one notch.
    SpeedDown,
    /// Set the work-area in point at the playhead.
    SetWorkIn,
    /// Set the work-area out point at the playhead.
    SetWorkOut,
    /// Clear the work area (full timeline plays).
    ClearWorkArea,
    /// Switch the panel to the select tool (move/trim).
    ToolSelect,
    /// Switch the panel to the razor tool (split).
    ToolRazor,
    /// Split the clip under the playhead (one named history turn).
    SplitAtPlayhead,
    /// Pick clips ending at or before the playhead.
    SelectBefore,
    /// Pick clips opening at or after the playhead.
    SelectAfter,
    /// Undo the newest history turn still in effect.
    Undo,
    /// Copy a ready agent request from the current selection.
    DescribeChange,
    /// Forget the saved arrangement (paint the default next).
    ResetLayout,
    /// Flip the light/dark theme.
    ToggleTheme,
    /// Scroll the preview panel into view.
    FocusPreview,
    /// Scroll the timeline panel into view.
    FocusTimeline,
}

impl ShortcutAction {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ShortcutAction::PlayToggle => "play-toggle",
            ShortcutAction::StepForward => "step-forward",
            ShortcutAction::StepBack => "step-back",
            ShortcutAction::GoToStart => "go-to-start",
            ShortcutAction::GoToEnd => "go-to-end",
            ShortcutAction::ToggleLoop => "toggle-loop",
            ShortcutAction::SpeedUp => "speed-up",
            ShortcutAction::SpeedDown => "speed-down",
            ShortcutAction::SetWorkIn => "set-work-in",
            ShortcutAction::SetWorkOut => "set-work-out",
            ShortcutAction::ClearWorkArea => "clear-work-area",
            ShortcutAction::ToolSelect => "tool-select",
            ShortcutAction::ToolRazor => "tool-razor",
            ShortcutAction::SplitAtPlayhead => "split-at-playhead",
            ShortcutAction::SelectBefore => "select-before",
            ShortcutAction::SelectAfter => "select-after",
            ShortcutAction::Undo => "undo",
            ShortcutAction::DescribeChange => "describe-change",
            ShortcutAction::ResetLayout => "reset-layout",
            ShortcutAction::ToggleTheme => "toggle-theme",
            ShortcutAction::FocusPreview => "focus-preview",
            ShortcutAction::FocusTimeline => "focus-timeline",
        }
    }

    /// Parse one action spelling; anything else refuses naming the map.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "play-toggle" => Some(ShortcutAction::PlayToggle),
            "step-forward" => Some(ShortcutAction::StepForward),
            "step-back" => Some(ShortcutAction::StepBack),
            "go-to-start" => Some(ShortcutAction::GoToStart),
            "go-to-end" => Some(ShortcutAction::GoToEnd),
            "toggle-loop" => Some(ShortcutAction::ToggleLoop),
            "speed-up" => Some(ShortcutAction::SpeedUp),
            "speed-down" => Some(ShortcutAction::SpeedDown),
            "set-work-in" => Some(ShortcutAction::SetWorkIn),
            "set-work-out" => Some(ShortcutAction::SetWorkOut),
            "clear-work-area" => Some(ShortcutAction::ClearWorkArea),
            "tool-select" => Some(ShortcutAction::ToolSelect),
            "tool-razor" => Some(ShortcutAction::ToolRazor),
            "split-at-playhead" => Some(ShortcutAction::SplitAtPlayhead),
            "select-before" => Some(ShortcutAction::SelectBefore),
            "select-after" => Some(ShortcutAction::SelectAfter),
            "undo" => Some(ShortcutAction::Undo),
            "describe-change" => Some(ShortcutAction::DescribeChange),
            "reset-layout" => Some(ShortcutAction::ResetLayout),
            "toggle-theme" => Some(ShortcutAction::ToggleTheme),
            "focus-preview" => Some(ShortcutAction::FocusPreview),
            "focus-timeline" => Some(ShortcutAction::FocusTimeline),
            _ => None,
        }
    }

    /// The area the action drives.
    pub fn area(self) -> ShortcutArea {
        match self {
            ShortcutAction::PlayToggle
            | ShortcutAction::StepForward
            | ShortcutAction::StepBack
            | ShortcutAction::GoToStart
            | ShortcutAction::GoToEnd
            | ShortcutAction::ToggleLoop
            | ShortcutAction::SpeedUp
            | ShortcutAction::SpeedDown => ShortcutArea::Playback,
            ShortcutAction::SetWorkIn
            | ShortcutAction::SetWorkOut
            | ShortcutAction::ClearWorkArea => ShortcutArea::WorkArea,
            ShortcutAction::ToolSelect
            | ShortcutAction::SplitAtPlayhead
            | ShortcutAction::ToolRazor
            | ShortcutAction::SelectBefore
            | ShortcutAction::SelectAfter => ShortcutArea::Tools,
            ShortcutAction::Undo | ShortcutAction::DescribeChange => ShortcutArea::Edit,
            ShortcutAction::ResetLayout
            | ShortcutAction::ToggleTheme
            | ShortcutAction::FocusPreview
            | ShortcutAction::FocusTimeline => ShortcutArea::Panels,
        }
    }

    /// Every served spelling, for refusal messages.
    pub const ALL_ACTIONS: [&'static str; 22] = [
        "play-toggle",
        "step-forward",
        "step-back",
        "go-to-start",
        "go-to-end",
        "toggle-loop",
        "speed-up",
        "speed-down",
        "set-work-in",
        "set-work-out",
        "clear-work-area",
        "tool-select",
        "tool-razor",
        "split-at-playhead",
        "select-before",
        "select-after",
        "undo",
        "describe-change",
        "reset-layout",
        "toggle-theme",
        "focus-preview",
        "focus-timeline",
    ];
}

/// One bound shortcut: the normalized chord with the action and area
/// it drives, so the page paints what each key does at a glance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBinding {
    /// Normalized chord (`space`, `ctrl+z`, `shift+arrow-left`).
    pub chord: String,
    /// The action the chord drives.
    pub action: ShortcutAction,
}

impl ShortcutBinding {
    /// The area the binding drives.
    pub fn area(&self) -> ShortcutArea {
        self.action.area()
    }
}

/// The full default map (Sprint 16, T008): playback, work area, tools,
/// edit, and panels — every chord already normalized, every action
/// covered, no two chords alike.
pub fn default_shortcuts() -> Vec<ShortcutBinding> {
    let bind = |chord: &str, action: ShortcutAction| ShortcutBinding {
        chord: chord.to_string(),
        action,
    };
    vec![
        // Playback.
        bind("space", ShortcutAction::PlayToggle),
        bind("arrow-right", ShortcutAction::StepForward),
        bind("arrow-left", ShortcutAction::StepBack),
        bind("home", ShortcutAction::GoToStart),
        bind("end", ShortcutAction::GoToEnd),
        bind("l", ShortcutAction::ToggleLoop),
        bind("=", ShortcutAction::SpeedUp),
        bind("-", ShortcutAction::SpeedDown),
        // Work area.
        bind("i", ShortcutAction::SetWorkIn),
        bind("o", ShortcutAction::SetWorkOut),
        bind("shift+o", ShortcutAction::ClearWorkArea),
        // Tools.
        bind("v", ShortcutAction::ToolSelect),
        bind("b", ShortcutAction::ToolRazor),
        bind("s", ShortcutAction::SplitAtPlayhead),
        bind("shift+arrow-left", ShortcutAction::SelectBefore),
        bind("shift+arrow-right", ShortcutAction::SelectAfter),
        // Edit.
        bind("ctrl+z", ShortcutAction::Undo),
        bind("d", ShortcutAction::DescribeChange),
        // Panels.
        bind("t", ShortcutAction::ToggleTheme),
        bind("ctrl+0", ShortcutAction::ResetLayout),
        bind("1", ShortcutAction::FocusPreview),
        bind("2", ShortcutAction::FocusTimeline),
    ]
}

/// Normalize one chord to its canonical spelling (Sprint 16, T008):
/// strokes split on whitespace (multi-key chords), modifiers split on
/// `+` and sorted `ctrl+alt+shift+meta`, everything lowercased, so a
/// remap spelling and a key press meet on the same string. Single keys
/// and chords alike pass through here.
pub fn normalize_chord(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(refused_shortcut(
            "the shortcut chord names no keys; pass a key (`space`) or a chord (`ctrl+z`, `shift+arrow-left`). Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let strokes: Vec<&str> = trimmed.split_whitespace().collect();
    if strokes.len() > 8 {
        return Err(refused_shortcut(format!(
            "the shortcut chord {raw:?} names {} keystrokes; chords hold at most 8. Nothing was staged or recorded.",
            strokes.len()
        )));
    }
    let mut out = Vec::with_capacity(strokes.len());
    for stroke in strokes {
        out.push(normalize_stroke(stroke)?);
    }
    Ok(out.join(" "))
}

/// Normalize one keystroke: modifiers sorted canonically ahead of the
/// key (`Shift+CTRL+x` reads `ctrl+shift+x`).
fn normalize_stroke(stroke: &str) -> Result<String, String> {
    let parts: Vec<&str> = stroke.split('+').collect();
    if parts.iter().any(|p| p.trim().is_empty()) {
        return Err(refused_shortcut(format!(
            "the shortcut chord {stroke:?} is not keys and `+` modifiers; write `ctrl+z` or `shift+arrow-left` with no empty part. Nothing was staged or recorded."
        )));
    }
    let key = parts.last().expect("stroke holds a key").to_lowercase();
    let bare_modifier = matches!(
        key.as_str(),
        "ctrl" | "control" | "alt" | "option" | "shift" | "meta" | "cmd" | "command" | "super"
    );
    if parts.len() == 1 && bare_modifier {
        return Err(refused_shortcut(format!(
            "the shortcut chord {stroke:?} names only a modifier; add the key it modifies (`ctrl+z`). Nothing was staged or recorded."
        )));
    }
    let mut mods: Vec<&str> = Vec::new();
    for raw in &parts[..parts.len() - 1] {
        let canon = match raw.to_lowercase().as_str() {
            "ctrl" | "control" => "ctrl",
            "alt" | "option" => "alt",
            "shift" => "shift",
            "meta" | "cmd" | "command" | "super" | "windows" => "meta",
            _ => {
                return Err(refused_shortcut(format!(
                    "the shortcut chord {stroke:?} names unknown modifier {raw:?}: modifiers read `ctrl`, `alt`, `shift`, or `meta`. Nothing was staged or recorded."
                )));
            }
        };
        if !mods.contains(&canon) {
            mods.push(canon);
        }
    }
    mods.sort();
    if mods.is_empty() {
        Ok(key)
    } else {
        Ok(format!("{}+{key}", mods.join("+")))
    }
}

/// One parsed remap entry (Sprint 16, T008): the action with its new
/// chord, in authored order, so first-wins collision reporting follows
/// the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemapEntry {
    /// Action the entry rebinds.
    pub action: ShortcutAction,
    /// Action spelling as authored (for receipts).
    pub action_raw: String,
    /// Normalized new chord.
    pub chord: String,
}

/// Parse one remap request body
/// (`{"version":1,"bindings":{"tool-razor":"x"}}`): the version must
/// match, every entry names a known action with a well-formed chord,
/// and a refused body names the bad entry while the saved remap stays
/// intact. Single keys and chords alike pass through
/// [`normalize_chord`].
pub fn parse_remap_body(body: &str) -> Result<Vec<RemapEntry>, String> {
    if body.trim().is_empty() {
        return Err(refused_shortcut(
            "the shortcut remap needs a JSON remap body (`{\"version\":1,\"bindings\":{\"tool-razor\":\"x\"}}`); pass the project remap from the shell. Nothing was saved.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_shortcut(format!(
            "the shortcut remap body is not JSON ({e}). Pass `{{\"version\":1,\"bindings\":{{\"tool-razor\":\"x\"}}}}`. Nothing was saved."
        ))
    })?;
    parse_remap_value(&value)
}

/// Parse one remap value shared by request bodies and the saved file:
/// the bad entry is named either way, so an unparseable remap falls
/// back to defaults naming that entry.
fn parse_remap_value(value: &serde_json::Value) -> Result<Vec<RemapEntry>, String> {
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version as u32 != STUDIO_SHORTCUTS_VERSION {
        return Err(refused_shortcut(format!(
            "the shortcut remap carries version {version}, this Studio reads version {}; reset the remap and re-bind. Nothing was saved.",
            STUDIO_SHORTCUTS_VERSION
        )));
    }
    let bindings = value.get("bindings").ok_or_else(|| {
        refused_shortcut(
            "the shortcut remap carries no `bindings` object; pass `{\"version\":1,\"bindings\":{\"tool-razor\":\"x\"}}`. Nothing was saved.".to_string(),
        )
    })?;
    let map = bindings.as_object().ok_or_else(|| {
        refused_shortcut(format!(
            "the shortcut remap `bindings` is not an object ({bindings}); pass `{{\"<action>\":\"<chord>\"}}`. Nothing was saved."
        ))
    })?;
    let mut entries = Vec::with_capacity(map.len());
    for (action_raw, chord_value) in map {
        let action = ShortcutAction::parse(action_raw).ok_or_else(|| {
            refused_shortcut(format!(
                "the shortcut remap entry \"bindings.{action_raw}\" names unknown action {action_raw:?}: the shell drives {}. Pass one of those actions. Nothing was saved.",
                ShortcutAction::ALL_ACTIONS.join(", ")
            ))
        })?;
        let chord_raw = chord_value.as_str().ok_or_else(|| {
            refused_shortcut(format!(
                "the shortcut remap entry \"bindings.{action_raw}\" carries chord {chord_value}; pass the new keys (`space`, `ctrl+z`). Nothing was saved."
            ))
        })?;
        let chord = normalize_chord(chord_raw).map_err(|why| {
            refused_shortcut(format!(
                "the shortcut remap entry \"bindings.{action_raw}\" carries a bad chord: {why} Nothing was saved."
            ))
        })?;
        entries.push(RemapEntry {
            action,
            action_raw: action_raw.clone(),
            chord,
        });
    }
    Ok(entries)
}

/// What loading the remap reported (Sprint 16, T008): the bindings to
/// honor (the defaults when the file is missing or unparseable), the
/// saved entries underneath, and the fallback or collision banner.
#[derive(Debug, Clone)]
pub struct ShortcutLoad {
    /// The bindings to honor (effective map).
    pub bindings: Vec<ShortcutBinding>,
    /// True when the map came from the project file.
    pub restored: bool,
    /// Fallback or collision banner, set exactly when the saved file
    /// exists but cannot restore cleanly.
    pub notice: Option<String>,
    /// One message per colliding entry, in file order.
    pub collisions: Vec<String>,
}

/// Resolve parsed remap entries over the defaults (Sprint 16, T008):
/// each entry rebinds its action while its chord stays free; an entry
/// whose chord already drives another action collides — the old
/// binding is kept and the collision reported. Entries apply in map
/// order, so the first entry processed wins a shared chord.
pub fn resolve_effective(entries: &[RemapEntry]) -> (Vec<ShortcutBinding>, Vec<String>) {
    let mut bindings = default_shortcuts();
    let mut collisions = Vec::new();
    for entry in entries {
        let taken = bindings
            .iter()
            .any(|b| b.chord == entry.chord && b.action != entry.action);
        if taken {
            let owner = bindings
                .iter()
                .find(|b| b.chord == entry.chord)
                .map(|b| b.action.as_str())
                .unwrap_or("?");
            collisions.push(format!(
                "studio: the shortcut remap gives chord {:?} to {:?}, but {:?} already drives it; keeping {:?} on {:?}. Fix the remap and reload it.",
                entry.chord,
                entry.action.as_str(),
                owner,
                owner,
                entry.chord
            ));
            continue;
        }
        if let Some(slot) = bindings.iter_mut().find(|b| b.action == entry.action) {
            slot.chord = entry.chord.clone();
        }
    }
    (bindings, collisions)
}

/// Load the project remap (Sprint 16, T008): a missing file honors the
/// defaults with no banner; a corrupt, version-wrong, or invalid file
/// honors the defaults and names the bad entry; a parseable file with
/// colliding chords honors the old bindings and reports each collision.
pub fn load_shortcuts(folder: &Path) -> ShortcutLoad {
    let defaults = default_shortcuts();
    let path = shortcuts_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ShortcutLoad {
                bindings: defaults,
                restored: false,
                notice: None,
                collisions: Vec::new(),
            };
        }
        Err(e) => {
            return ShortcutLoad {
                bindings: defaults,
                restored: false,
                notice: Some(format!(
                    "studio: the shortcut remap {} cannot be read ({e}); honoring the default map instead.",
                    path.display()
                )),
                collisions: Vec::new(),
            };
        }
        Ok(raw) => raw,
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Err(e) => {
            return ShortcutLoad {
                bindings: defaults,
                restored: false,
                notice: Some(format!(
                    "studio: the shortcut remap {} is not JSON ({e}); honoring the default map instead.",
                    path.display()
                )),
                collisions: Vec::new(),
            };
        }
        Ok(value) => value,
    };
    match parse_remap_value(&value) {
        Err(why) => ShortcutLoad {
            bindings: defaults,
            restored: false,
            notice: Some(format!(
                "studio: the shortcut remap {} cannot restore ({why}); honoring the default map instead.",
                path.display()
            )),
            collisions: Vec::new(),
        },
        Ok(entries) => {
            let (bindings, collisions) = resolve_effective(&entries);
            let notice = if collisions.is_empty() {
                None
            } else {
                Some(collisions.join(" "))
            };
            ShortcutLoad {
                bindings,
                restored: true,
                notice,
                collisions,
            }
        }
    }
}

/// Save one project remap (Sprint 16, T008): parses and validates the
/// body first — including collisions against the current effective map
/// — then writes the hidden remap file, so a refused body leaves the
/// previous remap (and every binding) intact and takes effect
/// immediately on the next lookup.
pub fn save_remap(folder: &Path, body: &str) -> Result<ShortcutLoad, String> {
    let entries = parse_remap_body(body)?;
    let (bindings, collisions) = resolve_effective(&entries);
    if let Some(first) = collisions.first() {
        return Err(refused_shortcut(format!("{first} Nothing was saved.")));
    }
    let path = shortcuts_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_shortcut(format!(
                "the hidden shortcut directory {} cannot be prepared: {e}. Nothing was saved.",
                parent.display()
            ))
        })?;
    }
    let mut pairs = String::new();
    for entry in &entries {
        if !pairs.is_empty() {
            pairs.push(',');
        }
        pairs.push_str(&format!(
            "\"{}\":\"{}\"",
            crate::studio::json_escape(&entry.action_raw),
            crate::studio::json_escape(&entry.chord)
        ));
    }
    let raw = format!(
        "{{\"version\":{},\"bindings\":{{{pairs}}}}}",
        STUDIO_SHORTCUTS_VERSION
    );
    std::fs::write(&path, raw).map_err(|e| {
        refused_shortcut(format!(
            "the shortcut remap file {} cannot be written: {e}. Nothing was saved.",
            path.display()
        ))
    })?;
    Ok(ShortcutLoad {
        bindings,
        restored: true,
        notice: None,
        collisions: Vec::new(),
    })
}

/// Render one binding as JSON for the shortcuts API.
fn binding_json(binding: &ShortcutBinding) -> String {
    format!(
        "{{\"chord\":\"{}\",\"action\":\"{}\",\"area\":\"{}\"}}",
        crate::studio::json_escape(&binding.chord),
        binding.action.as_str(),
        binding.area().as_str()
    )
}

/// Render the shortcuts readout as JSON for the shortcuts API: the
/// effective bindings with the default map visible underneath, plus
/// the fallback or collision report.
pub fn shortcuts_json(load: &ShortcutLoad) -> String {
    let bindings = load
        .bindings
        .iter()
        .map(binding_json)
        .collect::<Vec<_>>()
        .join(",");
    let defaults = default_shortcuts()
        .iter()
        .map(binding_json)
        .collect::<Vec<_>>()
        .join(",");
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    let collisions = load
        .collisions
        .iter()
        .map(|c| format!("\"{}\"", crate::studio::json_escape(c)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"restored\":{},\"notice\":{},\"collisions\":[{}],\"bindings\":[{}],\"defaults\":[{}]}}",
        load.restored, notice, collisions, bindings, defaults
    )
}

/// One parsed shortcut dispatch request (Sprint 16, T008): the action
/// to drive with the actor the turn records, plus the selection the
/// describe-a-change handoff copies from. Structural parsing only; the
/// chord resolves against the effective map at the route.
#[derive(Debug, Clone)]
pub struct DispatchRequest {
    /// The action to drive.
    pub action: ShortcutAction,
    /// Chord that resolved to the action (for receipts).
    pub chord: String,
    /// Actor the history turn records (`studio` when omitted).
    pub actor: String,
    /// Selection entries the handoff copies from, in order.
    pub selection: Vec<String>,
}

/// Parse one dispatch request body (`{"chord":"space"}` or
/// `{"action":"play-toggle"}` with an optional `actor` and
/// `selection`): a chord normalizes then resolves against the
/// effective map; an unknown chord refuses naming the map. A refused
/// body stages nothing.
pub fn parse_dispatch_body(
    body: &str,
    effective: &[ShortcutBinding],
) -> Result<DispatchRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_shortcut(
            "the shortcut dispatch needs a JSON dispatch body (`{\"chord\":\"space\"}` or `{\"action\":\"play-toggle\"}`); pass the pressed keys from the shell. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_shortcut(format!(
            "the shortcut dispatch body is not JSON ({e}). Pass `{{\"chord\":\"space\"}}`. Nothing was staged or recorded."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("studio")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_shortcut(
            "the shortcut dispatch names an empty actor; dispatched turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let selection = match value.get("selection") {
        None => Vec::new(),
        Some(list) => list
            .as_array()
            .ok_or_else(|| {
                refused_shortcut(format!(
                    "the shortcut dispatch `selection` is not an array ({list}); pass clip ids or `#index` entries. Nothing was staged or recorded."
                ))
            })?
            .iter()
            .map(|entry| {
                entry.as_str().ok_or_else(|| {
                    refused_shortcut(format!(
                        "the shortcut dispatch `selection` entry {entry} is not a clip reference; pass a declared `id` or a `#index` entry. Nothing was staged or recorded."
                    ))
                })
                .map(|s| s.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    if let Some(chord_raw) = value.get("chord").and_then(|c| c.as_str()) {
        let chord = normalize_chord(chord_raw).map_err(|why| {
            refused_shortcut(format!(
                "the dispatched chord is bad: {why} Nothing was staged or recorded."
            ))
        })?;
        let binding = effective.iter().find(|b| b.chord == chord).ok_or_else(|| {
            let known: Vec<String> = effective.iter().map(|b| b.chord.clone()).collect();
            refused_shortcut(format!(
                "the shortcut chord {chord:?} drives nothing: the effective map holds {}. Remap it or press a bound chord. Nothing was staged or recorded.",
                known.join(", ")
            ))
        })?;
        return Ok(DispatchRequest {
            action: binding.action,
            chord,
            actor,
            selection,
        });
    }
    if let Some(action_raw) = value.get("action").and_then(|a| a.as_str()) {
        let action = ShortcutAction::parse(action_raw).ok_or_else(|| {
            refused_shortcut(format!(
                "unknown shortcut action {action_raw:?}: the shell drives {}. Pass one of those actions. Nothing was staged or recorded.",
                ShortcutAction::ALL_ACTIONS.join(", ")
            ))
        })?;
        let chord = effective
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.chord.clone())
            .unwrap_or_default();
        return Ok(DispatchRequest {
            action,
            chord,
            actor,
            selection,
        });
    }
    Err(refused_shortcut(
        "the shortcut dispatch names neither `chord` nor `action`; pass the pressed keys (`{\"chord\":\"space\"}`). Nothing was staged or recorded."
            .to_string(),
    ))
}

/// One resolved selection clip for the agent handoff: the key the
/// maker picked with the window it spans.
#[derive(Debug, Clone)]
pub struct HandoffClip {
    /// Selection key as picked (`id` or `#index`).
    pub key: String,
    /// Stable `id` of the clip, when declared.
    pub id: Option<String>,
    /// Element name (`text`, `video`, ...).
    pub kind: String,
    /// Film seconds where the window opens.
    pub start: f64,
    /// Film seconds where the window closes.
    pub end: f64,
}

/// Display one handoff clip for the agent request: the picked key with
/// its window, naming the resolved `id` when the pick used `#index`.
fn handoff_clip_text(clip: &HandoffClip) -> String {
    match &clip.id {
        Some(id) if id == &clip.key => {
            format!(
                "{} \"{}\" ({:.3}s..{:.3}s)",
                clip.kind, clip.key, clip.start, clip.end
            )
        }
        Some(id) => {
            format!(
                "{} \"{}\" (id {:?}, {:.3}s..{:.3}s)",
                clip.kind, clip.key, id, clip.start, clip.end
            )
        }
        None => {
            format!(
                "{} \"{}\" ({:.3}s..{:.3}s)",
                clip.kind, clip.key, clip.start, clip.end
            )
        }
    }
}

/// Resolve handoff selection entries against the timeline rows
/// (Sprint 16, T008 / INT-003): `#N` reads as a document index, any
/// other spelling as a stable `id`. An empty selection refuses naming
/// the need to pick; unknown entries refuse naming them with the known
/// keys, so the copied request never points at nothing.
pub fn resolve_handoff_selection(
    rows: &crate::studio::StudioRowList,
    entries: &[String],
) -> Result<Vec<HandoffClip>, String> {
    if entries.is_empty() {
        return Err(refused_shortcut(
            "the describe-a-change handoff names no selection; pick one or more timeline clips first. Nothing was staged or recorded.".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let found = if let Some(rest) = entry.strip_prefix('#') {
            match rest.parse::<usize>() {
                Ok(index) => rows.iter().find(|r| r.index == index),
                Err(_) => None,
            }
        } else {
            rows.iter()
                .find(|r| r.id.as_deref() == Some(entry.as_str()))
        };
        match found {
            Some(row) => out.push(HandoffClip {
                key: entry.clone(),
                id: row.id.clone(),
                kind: row.kind.to_string(),
                start: row.start,
                end: row.end,
            }),
            None => {
                let known: Vec<String> = crate::studio::selection_keys_of_rows(rows);
                return Err(refused_shortcut(format!(
                    "the describe-a-change handoff selection {entry:?} names no clip; the timeline holds {}. Pass a declared `id` or a `#index` entry. Nothing was staged or recorded.",
                    known.join(", ")
                )));
            }
        }
    }
    Ok(out)
}

/// Render the ready agent request the describe action copies (Sprint
/// 16, T008 / INT-003): the composition, playhead, and selected clips
/// with their windows, so an agent can apply the change through the
/// edit SDK as a named history turn.
pub fn describe_request_text(
    composition: &str,
    frame: u64,
    fps_value: f64,
    duration_secs: f64,
    clips: &[HandoffClip],
) -> String {
    let time_secs = if fps_value > 0.0 {
        frame as f64 / fps_value
    } else {
        0.0
    };
    let list = clips
        .iter()
        .map(handoff_clip_text)
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Studio agent handoff: composition {composition:?} at frame {frame} ({time_secs:.3}s of {duration_secs:.3}s). Selected clips ({}): {list}. Requested change: describe the edit to apply to this selection. Apply it through the edit SDK as a named history turn and report the turn receipt.",
        clips.len()
    )
}
