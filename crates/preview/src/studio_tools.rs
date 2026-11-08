//! Timeline tools (Sprint 16, T004 / CAP-004, J-02, J-06, STATE-03,
//! INT-001, REC-001).
//!
//! The gestures every later slice operates: select moves and trims,
//! razor splits at the click (the all-tracks modifier splitting every
//! track), select-before and select-after pick clip sets around the
//! playhead, split at playhead divides the clip under the playhead,
//! ruler clicks seek, and an in/out work area bounds playback and
//! export ranges. The timeline proposes, the edit SDK disposes: every
//! mutating tool expands to [`sdk::TimelineVerb`] verbs or
//! [`sdk::EditOp`] operations applied through one ephemeral
//! [`sdk::SdkSession`], then lands as a single named history turn via
//! `persist_turn` under the `studio` actor — the same undo system as
//! headless SDK edits and CLI verbs. A refused tool stages nothing:
//! the session drops without persisting, so the override delta and
//! history stay exactly as they were, and the refusal names its cause
//! (`studio refused: ...`) with the SDK reason carried along.
//!
//! Pure view state (the select-before/after pick, the playhead seek,
//! the work-area file) stages no composition patches: selections and
//! seeks answer with the computed set or frame, and the work area
//! persists as a per-project file in the hidden history directory next
//! to the layout file — never beside the sources and never outside
//! the project. Render scratch and caches stay outside the project, as
//! before; default renders (no SDK delta) replay to themselves, so
//! they stay byte-identical.
//!
//! Request bodies use hyphenated keys (`all-tracks`); tools carry no
//! style attributes, so the CSS-like hyphenated vocabulary and the
//! fail-closed `font-family` comma try-order pass through untouched.

use std::path::{Path, PathBuf};

/// Actor every timeline tool records its history turn under: the
/// Studio surface itself (human or agent driving the same panel).
pub const STUDIO_TOOL_ACTOR: &str = "studio";

/// Work-area file name inside the hidden project directory (next to
/// the layout file, never beside the sources).
pub const STUDIO_WORKAREA_FILE: &str = "studio-workarea.json";

/// Version of the persisted work area.
pub const STUDIO_WORKAREA_VERSION: u32 = 1;

/// Hidden work-area path for one project folder.
pub fn work_area_path_for(folder: &Path) -> PathBuf {
    folder
        .join(sdk::HISTORY_DIR_NAME)
        .join(STUDIO_WORKAREA_FILE)
}

/// Prefix every tool refusal carries; the SDK reason follows it.
fn refused_tool(message: String) -> String {
    format!("studio refused: {message}")
}

/// One timeline tool (Sprint 16, T004 + T006): the gesture vocabulary
/// the panel serves. `select` moves (`to`) or trims (`start`/`end`);
/// `razor` splits at `at` (one clip, or every track with
/// `all-tracks`); `select-before`/`select-after` pick clip sets;
/// `split-at-playhead` divides the clip under the playhead; T006 adds
/// clip gestures with ripple and links — `multi-select` picks a named
/// set, `multi-drag` shifts a named set by `delta` keeping offsets,
/// `marquee` picks the clips a window overlaps, `delete` removes whole
/// clips with ripple closing the gap, `close-gap`/`close-all-gaps` shut
/// highlighted gaps, `link`/`unlink`/`detach-audio`/`merge-back` own the
/// linked sets, and `set-ripple` toggles the ripple switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolName {
    /// Move (`to`) or trim (`start`/`end`) the named clip.
    Select,
    /// Split at `at`: one clip, or every track with `all-tracks`.
    Razor,
    /// Pick clips ending at or before `at`.
    SelectBefore,
    /// Pick clips opening at or after `at`.
    SelectAfter,
    /// Split the clip under the playhead.
    SplitAtPlayhead,
    /// Pick the named clip set.
    MultiSelect,
    /// Shift the named clip set by `delta`, keeping offsets.
    MultiDrag,
    /// Pick the clips overlapping the `start`/`end` window.
    Marquee,
    /// Remove whole clips; ripple closes each same-track gap.
    Delete,
    /// Close the gap on `track` containing `at`.
    CloseGap,
    /// Close every highlighted gap (or one track's, with `track`).
    CloseAllGaps,
    /// Link the named clips into one sync set.
    Link,
    /// Remove the named clips from their sync sets.
    Unlink,
    /// Separate the audio of the named clip's sync set.
    DetachAudio,
    /// Move the second named clip back onto its link offset.
    MergeBack,
    /// Switch ripple on or off (`ripple`).
    SetRipple,
}

impl ToolName {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolName::Select => "select",
            ToolName::Razor => "razor",
            ToolName::SelectBefore => "select-before",
            ToolName::SelectAfter => "select-after",
            ToolName::SplitAtPlayhead => "split-at-playhead",
            ToolName::MultiSelect => "multi-select",
            ToolName::MultiDrag => "multi-drag",
            ToolName::Marquee => "marquee",
            ToolName::Delete => "delete",
            ToolName::CloseGap => "close-gap",
            ToolName::CloseAllGaps => "close-all-gaps",
            ToolName::Link => "link",
            ToolName::Unlink => "unlink",
            ToolName::DetachAudio => "detach-audio",
            ToolName::MergeBack => "merge-back",
            ToolName::SetRipple => "set-ripple",
        }
    }

    /// Parse one tool spelling; anything else refuses naming every tool.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "select" => Some(ToolName::Select),
            "razor" => Some(ToolName::Razor),
            "select-before" => Some(ToolName::SelectBefore),
            "select-after" => Some(ToolName::SelectAfter),
            "split-at-playhead" => Some(ToolName::SplitAtPlayhead),
            "multi-select" => Some(ToolName::MultiSelect),
            "multi-drag" => Some(ToolName::MultiDrag),
            "marquee" => Some(ToolName::Marquee),
            "delete" => Some(ToolName::Delete),
            "close-gap" => Some(ToolName::CloseGap),
            "close-all-gaps" => Some(ToolName::CloseAllGaps),
            "link" => Some(ToolName::Link),
            "unlink" => Some(ToolName::Unlink),
            "detach-audio" => Some(ToolName::DetachAudio),
            "merge-back" => Some(ToolName::MergeBack),
            "set-ripple" => Some(ToolName::SetRipple),
            _ => None,
        }
    }

    /// Every served spelling, for refusal messages.
    pub const ALL: [&'static str; 16] = [
        "select",
        "razor",
        "select-before",
        "select-after",
        "split-at-playhead",
        "multi-select",
        "multi-drag",
        "marquee",
        "delete",
        "close-gap",
        "close-all-gaps",
        "link",
        "unlink",
        "detach-audio",
        "merge-back",
        "set-ripple",
    ];
}

/// One parsed tool request (Sprint 16, T004 + T006): the tool plus
/// its hyphenated parameters. Times ride as film seconds (`to`,
/// `start`, `end`, `at`); `at` defaults to the playhead where it
/// applies; `clip` names an `id` or a `#index` entry; `clips` names a
/// set of them; `delta` shifts a set in film seconds; `ripple` flips
/// the ripple switch; `track` scopes gap actions to one lane;
/// `all-tracks` is the razor modifier.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// The gesture to run.
    pub tool: ToolName,
    /// Actor the history turn records (`studio` when omitted).
    pub actor: String,
    /// Clip reference as authored (`id` or `#index`).
    pub clip: Option<String>,
    /// Clip set as authored (`id` or `#index` entries).
    pub clips: Vec<String>,
    /// Move target in film seconds (`select` only).
    pub to: Option<f64>,
    /// Trim window open in film seconds (`select`/`marquee` windows).
    pub start: Option<f64>,
    /// Trim window close in film seconds (`select`/`marquee` windows).
    pub end: Option<f64>,
    /// Split or pick time in film seconds (playhead when omitted).
    pub at: Option<f64>,
    /// Razor modifier: split every track meeting `at`.
    pub all_tracks: bool,
    /// Multi-drag shift in film seconds (`multi-drag` only).
    pub delta: Option<f64>,
    /// Ripple switch (`set-ripple` only).
    pub ripple: Option<bool>,
    /// Track lane (`close-gap`, optional on `close-all-gaps`).
    pub track: Option<String>,
}

/// Read one request number: present, numeric, and finite. Missing
/// returns `None` (the caller defaults it); anything else refuses
/// naming the key.
fn tool_number(value: &serde_json::Value, key: &str) -> Result<Option<f64>, String> {
    let raw = match value.get(key) {
        None => return Ok(None),
        Some(v) => v,
    };
    let n = raw.as_f64().ok_or_else(|| {
        refused_tool(format!(
            "the studio tool `{key}` value {raw} is not seconds; pass a finite number of film seconds. Nothing was staged or recorded."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_tool(format!(
            "the studio tool `{key}` value {n:?} is not finite; pass a finite number of film seconds. Nothing was staged or recorded."
        )));
    }
    Ok(Some(n))
}

/// Parse one tool request body: `{"tool":"razor","at":1.5,
/// "all-tracks":true}` (actor rides along, defaulting to `studio`).
/// A refused body names its cause and stages nothing.
pub fn parse_tool_body(body: &str) -> Result<ToolCall, String> {
    if body.trim().is_empty() {
        return Err(refused_tool(
            "the studio tool needs a JSON tool body (`{\"tool\":\"razor\",\"at\":1.5}`); pass the gesture from the timeline panel. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_tool(format!(
            "the studio tool body is not JSON ({e}). Pass `{{\"tool\":\"razor\",\"at\":1.5}}`. Nothing was staged or recorded."
        ))
    })?;
    let tool_raw = value.get("tool").and_then(|t| t.as_str()).unwrap_or("");
    let tool = ToolName::parse(tool_raw).ok_or_else(|| {
        refused_tool(format!(
            "unknown studio tool {tool_raw:?}: the timeline serves {}. Pass one of those tools. Nothing was staged or recorded.",
            ToolName::ALL.join(", ")
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or(STUDIO_TOOL_ACTOR)
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_tool(
            "the studio tool names an empty actor; tool turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let clip = match value.get("clip") {
        None => None,
        Some(c) => Some(
            c.as_str()
                .ok_or_else(|| {
                    refused_tool(format!(
                        "the studio tool `clip` value {c} is not a clip reference; pass a declared `id` or a `#index` entry. Nothing was staged or recorded."
                    ))
                })?
                .to_string(),
        ),
    };
    if let Some(c) = &clip {
        if c.trim().is_empty() {
            return Err(refused_tool(
                "the studio tool `clip` names nothing; pass a declared `id` or a `#index` entry. Nothing was staged or recorded."
                    .to_string(),
            ));
        }
    }
    let to = tool_number(&value, "to")?;
    let start = tool_number(&value, "start")?;
    let end = tool_number(&value, "end")?;
    let at = tool_number(&value, "at")?;
    let delta = tool_number(&value, "delta")?;
    let all_tracks = match value.get("all-tracks") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| {
            refused_tool(format!(
                "the studio tool `all-tracks` value {v} is not a switch; pass `true` to split every track or omit it. Nothing was staged or recorded."
            ))
        })?,
    };
    let ripple = match value.get("ripple") {
        None => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| {
            refused_tool(format!(
                "the studio tool `ripple` value {v} is not a switch; pass `true` to close gaps on delete or `false` to leave them open. Nothing was staged or recorded."
            ))
        })?),
    };
    let track = match value.get("track") {
        None => None,
        Some(t) => Some(
            t.as_str()
                .ok_or_else(|| {
                    refused_tool(format!(
                        "the studio tool `track` value {t} is not a lane name; pass one of `video`, `audio`, `caption`, `graphics`. Nothing was staged or recorded."
                    ))
                })?
                .to_string(),
        ),
    };
    if let Some(t) = &track {
        if t.trim().is_empty() {
            return Err(refused_tool(
                "the studio tool `track` names no lane; pass one of `video`, `audio`, `caption`, `graphics`. Nothing was staged or recorded."
                    .to_string(),
            ));
        }
    }
    let clips = match value.get("clips") {
        None => Vec::new(),
        Some(list) => {
            let items = list.as_array().ok_or_else(|| {
                refused_tool(format!(
                    "the studio tool `clips` value {list} is not a clip set; pass an array of declared `id` values or `#index` entries. Nothing was staged or recorded."
                ))
            })?;
            let mut out = Vec::new();
            for item in items {
                let entry = item.as_str().ok_or_else(|| {
                    refused_tool(format!(
                        "the studio tool `clips` entry {item} is not a clip reference; pass a declared `id` or a `#index` entry. Nothing was staged or recorded."
                    ))
                })?;
                if entry.trim().is_empty() {
                    return Err(refused_tool(
                        "the studio tool `clips` names an empty entry; pass declared `id` values or `#index` entries. Nothing was staged or recorded."
                            .to_string(),
                    ));
                }
                out.push(entry.to_string());
            }
            out
        }
    };
    let no_times = to.is_none() && start.is_none() && end.is_none() && at.is_none();
    match tool {
        ToolName::Select => {
            let clip_name = clip.as_deref().unwrap_or("");
            if clip_name.trim().is_empty() {
                return Err(refused_tool(
                    "the studio `select` tool names no clip; pass a declared `id` or a `#index` entry with `to` (move) or `start`/`end` (trim). Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if !clips.is_empty() || delta.is_some() || ripple.is_some() || track.is_some() {
                return Err(refused_tool(
                    "the studio `select` tool moves or trims one clip; pass `clip` with `to` (move) or `start`/`end` (trim) — `multi-drag` moves a set. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            match (to, start, end) {
                (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
                    return Err(refused_tool(
                        "the studio `select` tool carries both `to` and a trim edge; a gesture either moves (`to`) or trims (`start`/`end`). Pass one of the two. Nothing was staged or recorded."
                            .to_string(),
                    ));
                }
                (None, None, None) => {
                    return Err(refused_tool(
                        "the studio `select` tool on {clip} names no gesture; pass `to` to move or `start`/`end` to trim.".replace("{clip}", clip_name) + " Nothing was staged or recorded.",
                    ));
                }
                _ => {}
            }
        }
        ToolName::Razor | ToolName::SelectBefore | ToolName::SelectAfter => {
            if to.is_some() || start.is_some() || end.is_some() {
                return Err(refused_tool(format!(
                    "the studio `{}` tool takes no `to`/`start`/`end`; pass `at` (film seconds, playhead when omitted){}. Nothing was staged or recorded.",
                    tool.as_str(),
                    if tool == ToolName::Razor {
                        " with `all-tracks` to split every track"
                    } else {
                        ""
                    }
                )));
            }
            if !clips.is_empty() || delta.is_some() || ripple.is_some() || track.is_some() {
                return Err(refused_tool(format!(
                    "the studio `{}` tool takes no `clips`/`delta`/`ripple`/`track`; pass `at` (film seconds, playhead when omitted). Nothing was staged or recorded.",
                    tool.as_str()
                )));
            }
        }
        ToolName::SplitAtPlayhead => {
            if to.is_some() || start.is_some() || end.is_some() || at.is_some() {
                return Err(refused_tool(
                    "the studio `split-at-playhead` tool takes no time: it divides the clip under the playhead. Pass no time fields. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some()
                || !clips.is_empty()
                || delta.is_some()
                || ripple.is_some()
                || track.is_some()
            {
                return Err(refused_tool(
                    "the studio `split-at-playhead` tool takes no clip set: it divides the clip under the playhead. Pass no clip fields. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::MultiSelect => {
            if clips.is_empty() {
                return Err(refused_tool(
                    "the studio `multi-select` tool names no clips; pass an array of declared `id` values or `#index` entries in `clips`. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some()
                || !no_times
                || delta.is_some()
                || ripple.is_some()
                || track.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `multi-select` tool takes only `clips`; pass the set to pick. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::MultiDrag => {
            if clips.is_empty() {
                return Err(refused_tool(
                    "the studio `multi-drag` tool names no clips; pass an array of declared `id` values or `#index` entries in `clips` with `delta` film seconds. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if delta.is_none() {
                return Err(refused_tool(
                    "the studio `multi-drag` tool names no shift; pass `delta` film seconds with the `clips` set. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some() || !no_times || ripple.is_some() || track.is_some() || all_tracks {
                return Err(refused_tool(
                    "the studio `multi-drag` tool takes only `clips` and `delta`; pass the set with its shift. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::Marquee => {
            if start.is_none() || end.is_none() {
                return Err(refused_tool(
                    "the studio `marquee` tool names no window; pass `start` and `end` film seconds with the drag rectangle. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some()
                || !clips.is_empty()
                || to.is_some()
                || at.is_some()
                || delta.is_some()
                || ripple.is_some()
                || track.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `marquee` tool takes only `start`/`end`; pass the drag window. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::Delete => {
            let named = clip.is_some() as usize + usize::from(!clips.is_empty());
            if named != 1 {
                return Err(refused_tool(
                    "the studio `delete` tool names no clip set; pass one declared `id` or `#index` entry in `clip`, or a set in `clips`. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if !no_times || delta.is_some() || ripple.is_some() || track.is_some() || all_tracks {
                return Err(refused_tool(
                    "the studio `delete` tool takes only `clip`/`clips`; pass the clips to remove (ripple closes each same-track gap). Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::CloseGap => {
            if track.is_none() || at.is_none() {
                return Err(refused_tool(
                    "the studio `close-gap` tool names no gap; pass the lane in `track` with the gap instant in `at`. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some()
                || !clips.is_empty()
                || to.is_some()
                || start.is_some()
                || end.is_some()
                || delta.is_some()
                || ripple.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `close-gap` tool takes only `track` and `at`; pass the lane with the gap instant. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::CloseAllGaps => {
            if clip.is_some()
                || !clips.is_empty()
                || !no_times
                || delta.is_some()
                || ripple.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `close-all-gaps` tool takes only an optional `track`; pass a lane to close one track, or nothing to close every gap. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::Link => {
            if clips.len() < 2 {
                return Err(refused_tool(
                    "the studio `link` tool needs at least two clips; pass distinct declared `id` values in `clips`. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some()
                || !no_times
                || delta.is_some()
                || ripple.is_some()
                || track.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `link` tool takes only `clips`; pass the declared `id` values to sync. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::Unlink => {
            let named = clip.is_some() as usize + usize::from(!clips.is_empty());
            if named != 1 {
                return Err(refused_tool(
                    "the studio `unlink` tool names no clip; pass a linked `id` in `clip`, or a set in `clips`. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if !no_times || delta.is_some() || ripple.is_some() || track.is_some() || all_tracks {
                return Err(refused_tool(
                    "the studio `unlink` tool takes only `clip`/`clips`; pass the linked clips to separate. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::DetachAudio => {
            if clip.as_deref().map(|c| c.trim().is_empty()).unwrap_or(true) {
                return Err(refused_tool(
                    "the studio `detach-audio` tool names no clip; pass a member `id` in `clip` to separate its group audio. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if !clips.is_empty()
                || !no_times
                || delta.is_some()
                || ripple.is_some()
                || track.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `detach-audio` tool takes only `clip`; pass the member whose group audio separates. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::MergeBack => {
            if clips.len() != 2 {
                return Err(refused_tool(
                    "the studio `merge-back` tool needs exactly two clips; pass the linked pair in `clips` to restore the second onto its link offset. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some()
                || !no_times
                || delta.is_some()
                || ripple.is_some()
                || track.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `merge-back` tool takes only `clips`; pass the linked pair. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
        ToolName::SetRipple => {
            if ripple.is_none() {
                return Err(refused_tool(
                    "the studio `set-ripple` tool names no switch; pass `ripple` `true` to close gaps on delete or `false` to leave them open. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
            if clip.is_some()
                || !clips.is_empty()
                || !no_times
                || delta.is_some()
                || track.is_some()
                || all_tracks
            {
                return Err(refused_tool(
                    "the studio `set-ripple` tool takes only `ripple`; pass the switch. Nothing was staged or recorded."
                        .to_string(),
                ));
            }
        }
    }
    Ok(ToolCall {
        tool,
        actor,
        clip,
        clips,
        to,
        start,
        end,
        at,
        all_tracks,
        delta,
        ripple,
        track,
    })
}

/// Selection key of one timeline row: the stable `id` when declared,
/// else the `#index` entry — the same spelling deep links resolve.
fn row_key(row: &sdk::TimelineRow) -> String {
    row.id.clone().unwrap_or_else(|| format!("#{}", row.index))
}

/// Resolve one authored clip reference against the staged rows: `#N`
/// reads as a document index, any other spelling as a stable `id`. An
/// `id` that names nothing refuses even when numeric, never falling
/// back silently. Neither the SDK nor the project tree is touched.
fn resolve_tool_clip(
    rows: &[sdk::TimelineRow],
    raw: &str,
) -> Result<(sdk::ClipRef, usize), String> {
    if let Some(rest) = raw.strip_prefix('#') {
        let index: usize = rest.parse().map_err(|_| {
            refused_tool(format!(
                "the studio tool clip {raw:?} is not a `#index` entry; pass `#N` (document index) or a declared `id`. Nothing was staged or recorded."
            ))
        })?;
        rows.iter()
            .find(|r| r.index == index)
            .map(|r| {
                (
                    sdk::ClipRef {
                        id: r.id.clone(),
                        index: Some(r.index),
                    },
                    r.index,
                )
            })
            .ok_or_else(|| {
                refused_tool(format!(
                    "the studio tool clip {raw:?} is past the open composition holding {} clip(s); pass an index from 0 to {}. Nothing was staged or recorded.",
                    rows.len(),
                    rows.len().saturating_sub(1)
                ))
            })
    } else if let Some(row) = rows.iter().find(|r| r.id.as_deref() == Some(raw)) {
        Ok((
            sdk::ClipRef {
                id: Some(raw.to_string()),
                index: Some(row.index),
            },
            row.index,
        ))
    } else {
        Err(refused_tool(format!(
            "the studio tool clip {raw:?} names no clip in the open composition holding {} clip(s); pass a declared `id` or a `#index` entry. Nothing was staged or recorded.",
            rows.len()
        )))
    }
}

/// Rows whose window contains `at` (film seconds): `start <= at < end`,
/// in document order.
fn rows_containing(rows: &[sdk::TimelineRow], at: f64) -> Vec<&sdk::TimelineRow> {
    rows.iter()
        .filter(|r| r.start <= at && at < r.end)
        .collect()
}

/// Receipt of one applied tool (Sprint 16, T004 + T006): the gesture
/// summary plus the named history turn that recorded it, so the panel
/// reads back the turn and its receipt. `moved` names the ripple
/// follow-ups (empty when ripple stayed off or nothing trailed);
/// `ripple` reports the switch the gesture honored.
#[derive(Debug, Clone)]
pub struct ToolReceipt {
    /// Served tool spelling (`select`, `razor`, ...).
    pub tool: &'static str,
    /// One-line human summary of the gesture with resolved times.
    pub summary: String,
    /// Summary of the recorded history turn.
    pub turn_summary: String,
    /// Patches this application staged and persisted (0 for picks).
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: Option<u64>,
    /// Actor the turn was recorded under.
    pub actor: Option<String>,
    /// Selection the panel highlights after the tool, in link order.
    pub selection: Vec<String>,
    /// Timeline rows JSON of the effective composition after the tool.
    pub rows_json: String,
    /// Clips the ripple follow-up shifted, in link order.
    pub moved: Vec<String>,
    /// Ripple switch the gesture honored.
    pub ripple: bool,
    /// Fallback banner (link-store fallback), when the file did not restore.
    pub notice: Option<String>,
}

/// Render one applied receipt as JSON for the tool API.
pub fn tool_receipt_json(receipt: &ToolReceipt) -> String {
    let selection = receipt
        .selection
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let moved = receipt
        .moved
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let turn = match receipt.turn {
        Some(t) => t.to_string(),
        None => "null".to_string(),
    };
    let actor = match &receipt.actor {
        Some(a) => format!("\"{}\"", crate::studio::json_escape(a)),
        None => "null".to_string(),
    };
    let notice = match &receipt.notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"tool\":\"{}\",\"summary\":\"{}\",\"turn_summary\":\"{}\",\"applied\":{},\"turn\":{},\"actor\":{},\"selection\":[{}],\"moved\":[{}],\"ripple\":{},\"notice\":{}}}",
        receipt.tool,
        crate::studio::json_escape(&receipt.summary),
        crate::studio::json_escape(&receipt.turn_summary),
        receipt.applied,
        turn,
        actor,
        selection,
        moved,
        receipt.ripple,
        notice
    )
}

/// One link-store mutation a tool applies before its composition
/// patches (Sprint 16, T006): fail-closed, so a refused turn never
/// leaves half-written sync metadata behind.
#[derive(Debug, Clone)]
enum LinksMutation {
    /// Flip the ripple switch.
    SetRipple(bool),
    /// Link one member set into a sync group.
    Link(Vec<String>),
    /// Remove members from their sync groups.
    Unlink(Vec<String>),
    /// Separate the audio of one member's group.
    DetachAudio(String),
    /// Restore the second member onto its link offset (moves timing
    /// through `ops` built by the expansion), then clear both detached
    /// flags.
    MergeBack { first: String, second: String },
}

/// One expanded tool (Sprint 16, T004 + T006): raw operations, timeline
/// verbs (trim keeps the file origin on footage), an optional
/// link-store mutation, or a pure pick staging nothing.
struct ToolExpansion {
    ops: Vec<sdk::EditOp>,
    verbs: Vec<sdk::TimelineVerb>,
    links: Option<LinksMutation>,
    summary: String,
    selection: Vec<String>,
    moved: Vec<String>,
}

impl ToolExpansion {
    fn ops(summary: String, ops: Vec<sdk::EditOp>) -> Self {
        ToolExpansion {
            ops,
            verbs: Vec::new(),
            links: None,
            summary,
            selection: Vec::new(),
            moved: Vec::new(),
        }
    }

    fn verbs(summary: String, verbs: Vec<sdk::TimelineVerb>) -> Self {
        ToolExpansion {
            ops: Vec::new(),
            verbs,
            links: None,
            summary,
            selection: Vec::new(),
            moved: Vec::new(),
        }
    }

    fn pick(summary: String, selection: Vec<String>) -> Self {
        ToolExpansion {
            ops: Vec::new(),
            verbs: Vec::new(),
            links: None,
            summary,
            selection,
            moved: Vec::new(),
        }
    }
}

/// Member `id` values touched by resolved row indexes (Sprint 16,
/// T006): clips without a declared `id` can join no link group, so
/// only declared ids gate sync.
fn touched_ids_of(rows: &[sdk::TimelineRow], indexes: &[usize]) -> Vec<String> {
    let mut ids: Vec<String> = indexes
        .iter()
        .filter_map(|i| rows.get(*i))
        .filter_map(|r| r.id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Expand one parsed tool against the staged rows: the whole gesture
/// validates before anything stages, so a refused tool stages
/// nothing. `playhead_secs` fills an omitted `at`. `links` is the
/// loaded link store (ripple switch plus sync groups) and `comp` the
/// serving composition name scoping the groups.
fn expand_tool(
    session: &sdk::SdkSession,
    call: &ToolCall,
    playhead_secs: f64,
    links: &crate::studio_clips::ClipLinksStore,
    comp: &str,
) -> Result<ToolExpansion, String> {
    let rows = session.timeline_rows();
    match call.tool {
        ToolName::Select => {
            let raw = call.clip.as_deref().expect("select names a clip");
            let (target, at_row) = resolve_tool_clip(&rows, raw)?;
            let row = &rows[at_row];
            if let Some(to) = call.to {
                if to < 0.0 {
                    return Err(refused_tool(format!(
                        "the studio `select` move of {} resolves to {to:.3}s; write a film time at or after 0. Nothing was staged or recorded.",
                        row_key(row)
                    )));
                }
                crate::studio_clips::check_linked_subset(
                    links,
                    comp,
                    &rows,
                    &touched_ids_of(&rows, &[at_row]),
                    "select move",
                )?;
                Ok(ToolExpansion::ops(
                    format!("select moved {} to {to:.3}s", row_key(row)),
                    vec![sdk::EditOp::Retiming {
                        target: target.clone(),
                        start: Some(to),
                        duration: None,
                        trim_start: None,
                    }],
                ))
            } else {
                let s = call.start.unwrap_or(row.start);
                let e = call.end.unwrap_or(row.end);
                crate::studio_clips::check_linked_subset(
                    links,
                    comp,
                    &rows,
                    &touched_ids_of(&rows, &[at_row]),
                    "select trim",
                )?;
                Ok(ToolExpansion::verbs(
                    format!("select trimmed {} to {s:.3}s..{e:.3}s", row_key(row)),
                    vec![sdk::TimelineVerb::Trim {
                        target,
                        start: call.start.map(|_| sdk::TimeExpr::Offset(s - row.start)),
                        end: call.end.map(|_| sdk::TimeExpr::Offset(e - row.start)),
                    }],
                ))
            }
        }
        ToolName::Razor => expand_razor(session, &rows, call, playhead_secs),
        ToolName::SelectBefore | ToolName::SelectAfter => {
            expand_pick(session, &rows, call, playhead_secs)
        }
        ToolName::SplitAtPlayhead => {
            let at = playhead_secs;
            let hits = rows_containing(&rows, at);
            let top = hits.into_iter().max_by(|a, b| {
                a.start
                    .partial_cmp(&b.start)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.index.cmp(&b.index))
            });
            match top {
                None => {
                    let comp = session.effective_composition();
                    Err(refused_tool(format!(
                        "the studio `split-at-playhead` at {at:.3}s meets no clip window in {:?} ({} clip(s)); park the playhead inside a clip window. Nothing was staged or recorded.",
                        comp.name,
                        rows.len()
                    )))
                }
                Some(row) => Ok(ToolExpansion::ops(
                    format!("split {} at playhead {at:.3}s", row_key(row)),
                    vec![sdk::EditOp::SplitElement {
                        target: sdk::ClipRef {
                            id: row.id.clone(),
                            index: Some(row.index),
                        },
                        at,
                    }],
                )),
            }
        }
        ToolName::MultiSelect => expand_multi_select(&rows, call),
        ToolName::MultiDrag => expand_multi_drag(&rows, call, links, comp),
        ToolName::Marquee => expand_marquee(session, &rows, call),
        ToolName::Delete => expand_delete(&rows, call, links, comp),
        ToolName::CloseGap => expand_close_gap(&rows, call, links, comp),
        ToolName::CloseAllGaps => expand_close_all_gaps(&rows, call, links, comp),
        ToolName::Link => expand_link(&rows, call),
        ToolName::Unlink => expand_unlink(call),
        ToolName::DetachAudio => expand_detach_audio(call),
        ToolName::MergeBack => expand_merge_back(&rows, call, links, comp),
        ToolName::SetRipple => expand_set_ripple(call),
    }
}

/// Resolve a `clips` set against the staged rows (Sprint 16, T006):
/// every entry must name a clip, and duplicates collapse to one row
/// index. Returns document indexes in ascending order.
fn resolve_tool_set(
    rows: &[sdk::TimelineRow],
    raws: &[String],
    what: &str,
) -> Result<Vec<usize>, String> {
    let mut indexes = Vec::new();
    for raw in raws {
        let (_, at_row) = resolve_tool_clip(rows, raw)?;
        if !indexes.contains(&at_row) {
            indexes.push(at_row);
        }
    }
    if indexes.is_empty() {
        return Err(refused_tool(format!(
            "the studio `{what}` tool names no resolvable clip; pass declared `id` values or `#index` entries. Nothing was staged or recorded."
        )));
    }
    indexes.sort();
    Ok(indexes)
}

/// Expand `multi-select` (Sprint 16, T006): a pure pick over the named
/// set, recording a turn like every other applied gesture.
fn expand_multi_select(
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
) -> Result<ToolExpansion, String> {
    let indexes = resolve_tool_set(rows, &call.clips, "multi-select")?;
    let picked: Vec<String> = indexes.iter().map(|i| row_key(&rows[*i])).collect();
    Ok(ToolExpansion::pick(
        format!("multi-select picked {}", picked.join(", ")),
        picked,
    ))
}

/// Expand `multi-drag` (Sprint 16, T006): shift every named clip by
/// `delta`, keeping offsets exactly. Touching a strict subset of a
/// link group refuses as sync-breaking before anything stages.
fn expand_multi_drag(
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
    links: &crate::studio_clips::ClipLinksStore,
    comp: &str,
) -> Result<ToolExpansion, String> {
    let delta = call.delta.expect("multi-drag names a delta");
    if !delta.is_finite() {
        return Err(refused_tool(format!(
            "the studio `multi-drag` shift {delta:?} is not finite; pass finite film seconds. Nothing was staged or recorded."
        )));
    }
    let indexes = resolve_tool_set(rows, &call.clips, "multi-drag")?;
    for i in &indexes {
        let want = rows[*i].start + delta;
        if !want.is_finite() || want < 0.0 {
            return Err(refused_tool(format!(
                "the studio `multi-drag` shift of {} to {want:.3}s leaves the timeline; pass a shift keeping every clip at or after 0. Nothing was staged or recorded.",
                row_key(&rows[*i])
            )));
        }
    }
    crate::studio_clips::check_linked_subset(
        links,
        comp,
        rows,
        &touched_ids_of(rows, &indexes),
        "multi-drag",
    )?;
    let names: Vec<String> = indexes.iter().map(|i| row_key(&rows[*i])).collect();
    let ops: Vec<sdk::EditOp> = indexes
        .iter()
        .map(|i| sdk::EditOp::Retiming {
            target: sdk::ClipRef {
                id: rows[*i].id.clone(),
                index: Some(rows[*i].index),
            },
            start: Some(rows[*i].start + delta),
            duration: None,
            trim_start: None,
        })
        .collect();
    let mut expansion = ToolExpansion::ops(
        format!(
            "multi-drag moved {} by {delta:+.3}s keeping offsets",
            names.join(", ")
        ),
        ops,
    );
    expansion.selection = names;
    Ok(expansion)
}

/// Expand `marquee` (Sprint 16, T006): a pure pick over the clips the
/// window overlaps (`start <= row.end && row.start < end`).
fn expand_marquee(
    session: &sdk::SdkSession,
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
) -> Result<ToolExpansion, String> {
    let s = call.start.expect("marquee names a start");
    let e = call.end.expect("marquee names an end");
    if !s.is_finite() || s < 0.0 || !e.is_finite() || e <= s {
        return Err(refused_tool(format!(
            "the studio `marquee` window {s:.3}s..{e:.3}s never opens; pass finite film seconds with the start before the end at or after 0. Nothing was staged or recorded."
        )));
    }
    let picked: Vec<String> = rows
        .iter()
        .filter(|r| s <= r.end && r.start < e)
        .map(row_key)
        .collect();
    if picked.is_empty() {
        let comp = session.effective_composition();
        return Err(refused_tool(format!(
            "the studio `marquee` window {s:.3}s..{e:.3}s overlaps no clip in {:?} ({} clip(s)); drag over a clip window. Nothing was staged or recorded.",
            comp.name,
            rows.len()
        )));
    }
    Ok(ToolExpansion::pick(
        format!("marquee picked {}", picked.join(", ")),
        picked,
    ))
}

/// Expand `delete` (Sprint 16, T006): remove whole clips, then ripple
/// on by default — later clips on each removed clip's track pull
/// earlier across the gap, with the receipt naming what moved. Ripple
/// never crosses a locked sync relationship: a shift that would move a
/// strict subset of a link group refuses the whole delete. With ripple
/// off the gaps stay open and the summary says so.
fn expand_delete(
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
    links: &crate::studio_clips::ClipLinksStore,
    comp: &str,
) -> Result<ToolExpansion, String> {
    let raws: Vec<String> = match &call.clip {
        Some(one) => vec![one.clone()],
        None => call.clips.clone(),
    };
    let indexes = resolve_tool_set(rows, &raws, "delete")?;
    crate::studio_clips::check_linked_subset(
        links,
        comp,
        rows,
        &touched_ids_of(rows, &indexes),
        "delete",
    )?;
    let removed: std::collections::BTreeSet<usize> = indexes.iter().copied().collect();
    // Removals apply in descending document order, so earlier targets
    // stay valid; surviving indexes below shift by the removals above.
    let mut ops: Vec<sdk::EditOp> = indexes
        .iter()
        .rev()
        .map(|i| sdk::EditOp::RemoveElement {
            target: sdk::ClipRef {
                id: rows[*i].id.clone(),
                index: Some(rows[*i].index),
            },
        })
        .collect();
    let names: Vec<String> = indexes.iter().map(|i| row_key(&rows[*i])).collect();
    if !links.ripple {
        return Ok(ToolExpansion::ops(
            format!(
                "delete removed {} (ripple off: gaps stay open)",
                names.join(", ")
            ),
            ops,
        ));
    }
    // Ripple: per removed clip, later clips on its track (at or past
    // the removed end) pull earlier by the removed length. Surviving
    // post-removal indexes count only the removals above each row.
    let mut shifts: std::collections::BTreeMap<usize, f64> = std::collections::BTreeMap::new();
    for i in &indexes {
        let gap_len = rows[*i].end - rows[*i].start;
        let track = rows[*i].track.clone();
        let end = rows[*i].end;
        for (j, row) in rows.iter().enumerate() {
            if removed.contains(&j) || row.track != track {
                continue;
            }
            if row.start >= end - 1e-6 {
                *shifts.entry(j).or_insert(0.0) += gap_len;
            }
        }
    }
    let shifted_ids: Vec<String> = shifts
        .keys()
        .flat_map(|j| touched_ids_of(rows, &[*j]))
        .collect();
    if !shifted_ids.is_empty() {
        crate::studio_clips::check_linked_subset(links, comp, rows, &shifted_ids, "ripple").map_err(|e| {
            refused_tool(format!(
                "{e} Unlink the partners first, or turn ripple off (`set-ripple`) to delete without closing the gap."
            ))
        })?;
    }
    let mut moved: Vec<String> = Vec::new();
    for (j, shift) in &shifts {
        let row = &rows[*j];
        let below = removed.iter().filter(|r| **r < *j).count();
        ops.push(sdk::EditOp::Retiming {
            target: sdk::ClipRef {
                id: row.id.clone(),
                index: Some(row.index.saturating_sub(below)),
            },
            start: Some(row.start - shift),
            duration: None,
            trim_start: None,
        });
        moved.push(row_key(row));
    }
    moved.sort();
    let mut expansion = ToolExpansion::ops(
        if moved.is_empty() {
            format!(
                "delete removed {} (ripple on: no trailing clips moved)",
                names.join(", ")
            )
        } else {
            format!(
                "delete removed {} closing each same-track gap moving {}",
                names.join(", "),
                moved.join(", ")
            )
        },
        ops,
    );
    expansion.moved = moved;
    Ok(expansion)
}

/// One ripple shift of a surviving clip (Sprint 16, T006): the row
/// index with its new film start.
struct RippleShift {
    row: usize,
    start: f64,
}

/// Simulate closing one gap (Sprint 16, T006): every clip on the track
/// at or past the gap end pulls earlier by the gap length. Returns the
/// shifts with trailing-clip keys for the receipt.
fn ripple_close_gap(
    rows: &[sdk::TimelineRow],
    track: &str,
    gap: &crate::studio_clips::ClipGap,
) -> (Vec<RippleShift>, Vec<String>) {
    let len = gap.end - gap.start;
    let mut shifts = Vec::new();
    let mut moved = Vec::new();
    for (j, row) in rows.iter().enumerate() {
        if row.track == track && row.start >= gap.end - 1e-6 {
            shifts.push(RippleShift {
                row: j,
                start: row.start - len,
            });
            moved.push(row_key(row));
        }
    }
    moved.sort();
    (shifts, moved)
}

/// Build retiming ops for ripple shifts (Sprint 16, T006): one op per
/// clip at its final start.
fn ripple_ops(rows: &[sdk::TimelineRow], shifts: &[RippleShift]) -> Vec<sdk::EditOp> {
    shifts
        .iter()
        .map(|s| sdk::EditOp::Retiming {
            target: sdk::ClipRef {
                id: rows[s.row].id.clone(),
                index: Some(rows[s.row].index),
            },
            start: Some(s.start),
            duration: None,
            trim_start: None,
        })
        .collect()
}

/// Expand `close-gap` (Sprint 16, T006): shut the gap on `track`
/// containing `at`, pulling later clips earlier in one turn. Ripple
/// honors the switch: with ripple off the close refuses naming the
/// switch, and a shift crossing a locked sync refuses the whole close.
fn expand_close_gap(
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
    links: &crate::studio_clips::ClipLinksStore,
    comp: &str,
) -> Result<ToolExpansion, String> {
    let track = call.track.as_deref().expect("close-gap names a track");
    let at = call.at.expect("close-gap names an instant");
    if !links.ripple {
        return Err(refused_tool(
            "the studio `close-gap` tool refuses while ripple is off; turn ripple on (`set-ripple`) to close gaps. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let gaps = crate::studio_clips::gaps_of(rows);
    let gap = gaps
        .iter()
        .find(|g| g.track == track && g.start - 1e-6 <= at && at <= g.end + 1e-6);
    let Some(gap) = gap else {
        let listed: Vec<String> = gaps
            .iter()
            .filter(|g| g.track == track)
            .map(|g| format!("{:.3}s..{:.3}s", g.start, g.end))
            .collect();
        return Err(refused_tool(format!(
            "the studio `close-gap` tool finds no gap on track {track:?} containing {at:.3}s (gaps: {}); pick a highlighted gap instant. Nothing was staged or recorded.",
            if listed.is_empty() {
                "none".to_string()
            } else {
                listed.join(", ")
            }
        )));
    };
    let (shifts, mut moved) = ripple_close_gap(rows, track, gap);
    if shifts.is_empty() {
        return Err(refused_tool(format!(
            "the studio `close-gap` tool finds the gap {track:?} {:.3}s..{:.3}s trailing with no later clips; nothing to close. Nothing was staged or recorded.",
            gap.start, gap.end
        )));
    }
    let shifted_ids = touched_ids_of(rows, &shifts.iter().map(|s| s.row).collect::<Vec<_>>());
    crate::studio_clips::check_linked_subset(links, comp, rows, &shifted_ids, "close-gap")
        .map_err(|e| {
            refused_tool(format!(
                "{e} Unlink the partners first, or leave the gap open."
            ))
        })?;
    moved.sort();
    let mut expansion = ToolExpansion::ops(
        format!(
            "close-gap shut {track:?} {:.3}s..{:.3}s moving {}",
            gap.start,
            gap.end,
            moved.join(", ")
        ),
        ripple_ops(rows, &shifts),
    );
    expansion.moved = moved;
    Ok(expansion)
}

/// Expand `close-all-gaps` (Sprint 16, T006): shut every highlighted
/// gap (or one track's with `track`) in film order in one turn, each
/// pulling its later clips earlier. Refuses while ripple is off and
/// when no gap has a later clip to pull.
fn expand_close_all_gaps(
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
    links: &crate::studio_clips::ClipLinksStore,
    comp: &str,
) -> Result<ToolExpansion, String> {
    if !links.ripple {
        return Err(refused_tool(
            "the studio `close-all-gaps` tool refuses while ripple is off; turn ripple on (`set-ripple`) to close gaps. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let mut gaps = crate::studio_clips::gaps_of(rows);
    if let Some(track) = call.track.as_deref() {
        gaps.retain(|g| g.track == track);
    }
    if gaps.is_empty() {
        return Err(refused_tool(format!(
            "the studio `close-all-gaps` tool finds no gaps{}; the timeline already runs edge to edge. Nothing was staged or recorded.",
            match call.track.as_deref() {
                Some(t) => format!(" on track {t:?}"),
                None => String::new(),
            }
        )));
    }
    // Simulate in film order so each close sees the earlier pulls:
    // working starts move with every shut gap, so later gaps on the
    // same track read the pulled positions.
    let mut working: Vec<f64> = rows.iter().map(|r| r.start).collect();
    let mut total: std::collections::BTreeMap<usize, f64> = std::collections::BTreeMap::new();
    for gap in &gaps {
        let len = gap.end - gap.start;
        for (j, row) in rows.iter().enumerate() {
            if row.track == gap.track && working[j] >= gap.end - 1e-6 {
                working[j] -= len;
                *total.entry(j).or_insert(row.start) = working[j];
            }
        }
    }
    let shifts: Vec<RippleShift> = total
        .iter()
        .map(|(j, start)| RippleShift {
            row: *j,
            start: *start,
        })
        .collect();
    if shifts.is_empty() {
        return Err(refused_tool(
            "the studio `close-all-gaps` tool finds only trailing gaps with no later clips; nothing to close. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let shifted_ids = touched_ids_of(rows, &shifts.iter().map(|s| s.row).collect::<Vec<_>>());
    crate::studio_clips::check_linked_subset(links, comp, rows, &shifted_ids, "close-all-gaps")
        .map_err(|e| {
            refused_tool(format!(
                "{e} Unlink the partners first, or leave the gaps open."
            ))
        })?;
    let mut moved: Vec<String> = shifts.iter().map(|s| row_key(&rows[s.row])).collect();
    moved.sort();
    let mut expansion = ToolExpansion::ops(
        format!(
            "close-all-gaps shut {} gap(s) moving {}",
            gaps.len(),
            moved.join(", ")
        ),
        ripple_ops(rows, &shifts),
    );
    expansion.moved = moved;
    Ok(expansion)
}

/// Expand `link` (Sprint 16, T006): validate the member set now
/// (unknown or already-linked members refuse), then record the sync
/// group with its baseline. The `clips` entries must be declared ids;
/// `#index` entries refuse naming the link rule.
fn expand_link(rows: &[sdk::TimelineRow], call: &ToolCall) -> Result<ToolExpansion, String> {
    for raw in &call.clips {
        if raw.starts_with('#') {
            return Err(refused_tool(format!(
                "the studio `link` tool names {raw:?}; link needs declared `id` values (clips without one cannot link). Nothing was staged or recorded."
            )));
        }
    }
    // The real mutation runs in `apply_studio_tool` against the
    // loaded store (already-linked members refuse there), so this pass
    // only checks the member spellings resolve to declared ids on the
    // timeline.
    let starts: std::collections::BTreeSet<String> =
        rows.iter().filter_map(|r| r.id.clone()).collect();
    for raw in &call.clips {
        if !starts.contains(raw) {
            return Err(refused_tool(format!(
                "the studio `link` tool names {raw:?}, which is not a declared clip `id` on the timeline; link needs declared `id` values (clips without one cannot link). Nothing was staged or recorded."
            )));
        }
    }
    let mut members = call.clips.clone();
    members.sort();
    members.dedup();
    if members.len() < 2 {
        return Err(refused_tool(
            "the studio `link` tool needs at least two different clips; pass distinct declared `id` values in `clips`. Nothing was staged or recorded.".to_string(),
        ));
    }
    let mut expansion =
        ToolExpansion::pick(format!("link {}", members.join(", ")), members.clone());
    expansion.links = Some(LinksMutation::Link(members));
    Ok(expansion)
}

/// Expand `unlink` (Sprint 16, T006): the member check runs against the
/// loaded store in `apply_studio_tool`; this pass only shapes the
/// mutation.
fn expand_unlink(call: &ToolCall) -> Result<ToolExpansion, String> {
    let members: Vec<String> = match &call.clip {
        Some(one) => vec![one.clone()],
        None => call.clips.clone(),
    };
    let mut expansion = ToolExpansion::pick(format!("unlink {}", members.join(", ")), Vec::new());
    expansion.links = Some(LinksMutation::Unlink(members));
    Ok(expansion)
}

/// Expand `detach-audio` (Sprint 16, T006): the audio check runs
/// against the loaded store in `apply_studio_tool`.
fn expand_detach_audio(call: &ToolCall) -> Result<ToolExpansion, String> {
    let member = call.clip.clone().expect("detach-audio names a clip");
    let mut expansion = ToolExpansion::pick(format!("detach-audio {member}"), Vec::new());
    expansion.links = Some(LinksMutation::DetachAudio(member));
    Ok(expansion)
}

/// Expand `merge-back` (Sprint 16, T006): plan the restoring move
/// against the loaded store now (unknown or unlinked pairs refuse),
/// then move the second member onto its link offset in one turn and
/// clear both detached flags.
fn expand_merge_back(
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
    links: &crate::studio_clips::ClipLinksStore,
    comp: &str,
) -> Result<ToolExpansion, String> {
    let first = call.clips[0].clone();
    let second = call.clips[1].clone();
    if first.starts_with('#') || second.starts_with('#') {
        return Err(refused_tool(
            "the studio `merge-back` tool names a `#index` entry; merge-back needs declared `id` values (links name ids). Nothing was staged or recorded.".to_string(),
        ));
    }
    let want = crate::studio_clips::merge_back_plan(links, comp, rows, &first, &second)?;
    let mut ops = Vec::new();
    let summary = match want {
        Some(at) => {
            let row = rows
                .iter()
                .find(|r| r.id.as_deref() == Some(second.as_str()))
                .expect("merge-back member resolves");
            ops.push(sdk::EditOp::Retiming {
                target: sdk::ClipRef {
                    id: Some(second.clone()),
                    index: Some(row.index),
                },
                start: Some(at),
                duration: None,
                trim_start: None,
            });
            format!("merge-back moved {second:?} to {at:.3}s onto its link offset with {first:?}")
        }
        None => format!("merge-back {second:?} already in sync with {first:?}"),
    };
    let mut expansion = ToolExpansion::ops(summary, ops);
    expansion.links = Some(LinksMutation::MergeBack { first, second });
    Ok(expansion)
}

/// Expand `set-ripple` (Sprint 16, T006): flip the project ripple
/// switch; deletes and gap closes honor it at once.
fn expand_set_ripple(call: &ToolCall) -> Result<ToolExpansion, String> {
    let on = call.ripple.expect("set-ripple names a switch");
    let mut expansion = ToolExpansion::pick(
        if on {
            "ripple on: deletes and gap closes pull later clips earlier".to_string()
        } else {
            "ripple off: deletes and gap closes leave gaps open".to_string()
        },
        Vec::new(),
    );
    expansion.links = Some(LinksMutation::SetRipple(on));
    Ok(expansion)
}

/// Expand the razor: the modifier wins, splitting every track meeting
/// `at` (descending document order, so earlier targets stay valid);
/// else one named clip at `at`; else the topmost clip under the click.
fn expand_razor(
    session: &sdk::SdkSession,
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
    playhead_secs: f64,
) -> Result<ToolExpansion, String> {
    let at = call.at.unwrap_or(playhead_secs);
    if !at.is_finite() || at < 0.0 {
        return Err(refused_tool(format!(
            "the studio `razor` split at {at:?} is outside the timeline; write film seconds at or after 0 inside a clip window. Nothing was staged or recorded."
        )));
    }
    if call.all_tracks {
        let hits = rows_containing(rows, at);
        if hits.is_empty() {
            let comp = session.effective_composition();
            return Err(refused_tool(format!(
                "the studio `razor` split at {at:.3}s meets no clip window in {:?} ({} clip(s)); click inside a clip window. Nothing was staged or recorded.",
                comp.name,
                rows.len()
            )));
        }
        let mut targets: Vec<&sdk::TimelineRow> = hits;
        targets.sort_by_key(|r| std::cmp::Reverse(r.index));
        let mut keys: Vec<String> = targets.iter().map(|r| row_key(r)).collect();
        keys.sort();
        let n = targets.len();
        let ops: Vec<sdk::EditOp> = targets
            .iter()
            .map(|r| sdk::EditOp::SplitElement {
                target: sdk::ClipRef {
                    id: r.id.clone(),
                    index: Some(r.index),
                },
                at,
            })
            .collect();
        Ok(ToolExpansion::ops(
            format!("razor split {n} clip(s) at {at:.3}s: {}", keys.join(", ")),
            ops,
        ))
    } else if let Some(raw) = &call.clip {
        let (target, at_row) = resolve_tool_clip(rows, raw)?;
        let row = &rows[at_row];
        Ok(ToolExpansion::ops(
            format!("razor split {} at {at:.3}s", row_key(row)),
            vec![sdk::EditOp::SplitElement { target, at }],
        ))
    } else {
        let hits = rows_containing(rows, at);
        let top = hits.into_iter().max_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.index.cmp(&b.index))
        });
        match top {
            None => {
                let comp = session.effective_composition();
                Err(refused_tool(format!(
                    "the studio `razor` split at {at:.3}s meets no clip window in {:?} ({} clip(s)); click inside a clip window. Nothing was staged or recorded.",
                    comp.name,
                    rows.len()
                )))
            }
            Some(row) => Ok(ToolExpansion::ops(
                format!("razor split {} at {at:.3}s", row_key(row)),
                vec![sdk::EditOp::SplitElement {
                    target: sdk::ClipRef {
                        id: row.id.clone(),
                        index: Some(row.index),
                    },
                    at,
                }],
            )),
        }
    }
}

/// Expand select-before/after: clips ending at or before `at`, or
/// opening at or after it. Clips straddling `at` belong to neither
/// set. An empty set refuses naming the time.
fn expand_pick(
    session: &sdk::SdkSession,
    rows: &[sdk::TimelineRow],
    call: &ToolCall,
    playhead_secs: f64,
) -> Result<ToolExpansion, String> {
    let at = call.at.unwrap_or(playhead_secs);
    if !at.is_finite() || at < 0.0 {
        return Err(refused_tool(format!(
            "the studio `{}` pick at {at:?} is outside the timeline; write film seconds at or after 0. Nothing was staged or recorded.",
            call.tool.as_str()
        )));
    }
    let before = call.tool == ToolName::SelectBefore;
    let picked: Vec<String> = rows
        .iter()
        .filter(|r| if before { r.end <= at } else { r.start >= at })
        .map(row_key)
        .collect();
    if picked.is_empty() {
        let comp = session.effective_composition();
        return Err(refused_tool(format!(
            "the studio `{}` pick at {at:.3}s matches no clip in {:?} ({} clip(s)); pick a time {} a clip edge. Nothing was staged or recorded.",
            call.tool.as_str(),
            comp.name,
            rows.len(),
            if before { "at or after" } else { "at or before" }
        )));
    }
    Ok(ToolExpansion::pick(
        format!(
            "{} at {at:.3}s picked {}",
            call.tool.as_str(),
            picked.join(", ")
        ),
        picked,
    ))
}

/// Apply one parsed tool through the Sprint 15 patch and history
/// machinery (Sprint 16, T004 + T006): the link store loads first
/// (fail-closed fallback), the whole gesture validates against it,
/// the link mutation persists before anything stages, then every
/// operation or verb applies to a fresh SDK session over the project
/// and one validated turn records the gesture under the call actor.
/// `composition` selects the session (the serving composition) and
/// scopes the link groups; `playhead_secs` fills an omitted `at`.
/// A refused tool — unknown clip, empty pick, a broken sync guard, an
/// SDK capability or value check — returns its cause with the override
/// delta, the link store, and history exactly as they were.
pub fn apply_studio_tool(
    folder: &Path,
    composition: Option<&str>,
    call: &ToolCall,
    playhead_secs: f64,
) -> Result<ToolReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_tool(format!(
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
        refused_tool(format!(
            "the project cannot open for tools: {e} Nothing was applied or recorded."
        ))
    })?;
    let comp_name = session.effective_composition().name.clone();
    let load = crate::studio_clips::load_clip_links(folder);
    let expansion = expand_tool(&session, call, playhead_secs, &load.store, &comp_name)?;
    // The link mutation persists before anything stages (fail-closed):
    // a refused turn never leaves half-written sync metadata behind.
    let mut store = load.store.clone();
    // The merge-back flag clear runs only after its restoring move lands
    // (below): a refused move must leave the sync metadata untouched.
    let mut deferred_clear: Option<(String, String)> = None;
    if let Some(mutation) = &expansion.links {
        let rows = session.timeline_rows();
        match mutation {
            LinksMutation::SetRipple(on) => {
                store.ripple = *on;
            }
            LinksMutation::Link(members) => {
                crate::studio_clips::link_clips(&mut store, &comp_name, &rows, members)?;
            }
            LinksMutation::Unlink(members) => {
                crate::studio_clips::unlink_clips(&mut store, &comp_name, &rows, members)?;
            }
            LinksMutation::DetachAudio(member) => {
                crate::studio_clips::detach_audio(&mut store, &comp_name, &rows, member)?;
            }
            LinksMutation::MergeBack { first, second, .. } => {
                deferred_clear = Some((first.clone(), second.clone()));
            }
        }
        if deferred_clear.is_none() {
            crate::studio_clips::persist_clip_links(folder, &store)?;
        }
    }
    let summary = expansion.summary.clone();
    let selection = expansion.selection.clone();
    let moved = expansion.moved.clone();
    let mut applied = 0usize;
    for op in &expansion.ops {
        session
            .apply(op.clone())
            .map_err(|e| refused_tool(format!("{e} Nothing was persisted or recorded.")))?;
        applied += 1;
    }
    for verb in expansion.verbs {
        let receipt = session
            .timeline_apply(verb)
            .map_err(|e| refused_tool(format!("{e} Nothing was persisted or recorded.")))?;
        applied += receipt.applied;
    }
    // The merge-back detached flags clear only now that the restoring
    // move landed: the badge clears exactly when the offsets return, and
    // a refused move above never reached this persist.
    if let Some((first, second)) = deferred_clear {
        crate::studio_clips::merge_back_clear(&mut store, comp_name.as_str(), &first, &second);
        crate::studio_clips::persist_clip_links(folder, &store)?;
    }
    let turn = session.persist_turn(&call.actor).map_err(|e| {
        refused_tool(format!(
            "{e} Staged work stays preserved unapplied — nothing was persisted or recorded."
        ))
    })?;
    let turn_summary = match turn.turn {
        Some(seq) => sdk::history::show_turn(folder, seq)
            .map(|detail| detail.record.summary.clone())
            .unwrap_or_else(|_| summary.clone()),
        None => summary.clone(),
    };
    Ok(ToolReceipt {
        tool: call.tool.as_str(),
        summary,
        turn_summary,
        applied,
        turn: turn.turn,
        actor: turn.actor,
        selection,
        rows_json: crate::studio::timeline_rows_json(session.effective_composition()),
        moved,
        ripple: store.ripple,
        notice: load.notice.clone(),
    })
}

/// The in/out work area (Sprint 16, T004): film seconds bounding
/// playback and export ranges. `None` on both sides means no work
/// area: the full timeline plays.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkArea {
    /// Film seconds where the work area opens.
    pub in_point: Option<f64>,
    /// Film seconds where the work area closes.
    pub out_point: Option<f64>,
}

/// What loading the work area reported: the area to honor, whether it
/// came from the project file, and the fallback banner when it did not.
#[derive(Debug, Clone)]
pub struct WorkAreaLoad {
    /// The area to honor (empty when no file or unreadable).
    pub area: WorkArea,
    /// True when the area came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file cannot restore.
    pub notice: Option<String>,
}

/// Parse and validate one work-area body (`{"in":1.0,"out":3.0}`)
/// against the composition length: both sides ride along, finite, with
/// `0 <= in < out <= duration`. A refused body names its cause and
/// leaves the saved area intact.
pub fn set_work_area(folder: &Path, body: &str, duration_secs: f64) -> Result<WorkArea, String> {
    if body.trim().is_empty() {
        return Err(refused_tool(
            "the studio work area needs a JSON body (`{\"in\":1.0,\"out\":3.0}`); pass the in and out film seconds. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_tool(format!(
            "the studio work area body is not JSON ({e}). Pass `{{\"in\":1.0,\"out\":3.0}}`. Nothing was staged or recorded."
        ))
    })?;
    let number = |key: &str| -> Result<f64, String> {
        value
            .get(key)
            .and_then(|v| v.as_f64())
            .filter(|n| n.is_finite())
            .ok_or_else(|| {
                refused_tool(format!(
                    "the studio work area `{key}` is not finite film seconds; pass a finite number at or after 0. Nothing was staged or recorded."
                ))
            })
    };
    let inp = number("in")?;
    let outp = number("out")?;
    if inp < 0.0 || outp < 0.0 {
        return Err(refused_tool(format!(
            "the studio work area {inp:.3}s..{outp:.3}s opens before 0; write film seconds at or after 0. Nothing was staged or recorded."
        )));
    }
    if inp >= outp {
        return Err(refused_tool(format!(
            "the studio work area {inp:.3}s..{outp:.3}s never opens: the in point must sit before the out point. Nothing was staged or recorded."
        )));
    }
    if outp > duration_secs {
        return Err(refused_tool(format!(
            "the studio work area {inp:.3}s..{outp:.3}s runs past the composition end ({duration_secs:.3}s); the work area stays inside the timeline. Nothing was staged or recorded."
        )));
    }
    let area = WorkArea {
        in_point: Some(inp),
        out_point: Some(outp),
    };
    let path = work_area_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_tool(format!(
                "the hidden work-area directory {} cannot be prepared: {e}. Nothing was staged or recorded.",
                parent.display()
            ))
        })?;
    }
    let raw = serde_json::to_string_pretty(&serde_json::json!({
        "version": STUDIO_WORKAREA_VERSION,
        "in": inp,
        "out": outp,
    }))
    .map_err(|e| {
        refused_tool(format!(
            "the work area cannot be encoded: {e}. Nothing was staged or recorded."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_tool(format!(
            "the work area file {} cannot be written: {e}. Nothing was staged or recorded.",
            path.display()
        ))
    })?;
    Ok(area)
}

/// Load the per-project work area: a missing file plays the full
/// timeline with no banner; a corrupt, version-wrong, or invalid file
/// plays the full timeline and says so.
pub fn load_work_area(folder: &Path) -> WorkAreaLoad {
    let empty = WorkArea {
        in_point: None,
        out_point: None,
    };
    let path = work_area_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return WorkAreaLoad {
                area: empty,
                restored: false,
                notice: None,
            };
        }
        Err(e) => {
            return WorkAreaLoad {
                area: empty,
                restored: false,
                notice: Some(format!(
                    "studio: the work area {} cannot be read ({e}); playing the full timeline instead.",
                    path.display()
                )),
            };
        }
        Ok(raw) => raw,
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Err(e) => {
            return WorkAreaLoad {
                area: empty,
                restored: false,
                notice: Some(format!(
                    "studio: the work area {} is not a work area ({e}); playing the full timeline instead.",
                    path.display()
                )),
            };
        }
        Ok(v) => v,
    };
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version as u32 != STUDIO_WORKAREA_VERSION {
        return WorkAreaLoad {
            area: empty,
            restored: false,
            notice: Some(format!(
                "studio: the work area {} carries version {version}, this Studio reads version {}; playing the full timeline instead.",
                path.display(),
                STUDIO_WORKAREA_VERSION
            )),
        };
    }
    let area = WorkArea {
        in_point: value.get("in").and_then(|v| v.as_f64()),
        out_point: value.get("out").and_then(|v| v.as_f64()),
    };
    match (area.in_point, area.out_point) {
        (Some(i), Some(o)) if i.is_finite() && o.is_finite() && i >= 0.0 && i < o => {
            WorkAreaLoad {
                area,
                restored: true,
                notice: None,
            }
        }
        _ => WorkAreaLoad {
            area: empty,
            restored: false,
            notice: Some(format!(
                "studio: the work area {} names no valid in/out range; playing the full timeline instead.",
                path.display()
            )),
        },
    }
}

/// Forget the work area: the full timeline plays next. A missing file
/// still clears cleanly.
pub fn clear_work_area(folder: &Path) {
    let _ = std::fs::remove_file(work_area_path_for(folder));
}

/// Render one work area as JSON for the work-area API.
pub fn work_area_json(load: &WorkAreaLoad) -> String {
    let point = |v: Option<f64>| match v {
        Some(n) => format!("{n:.3}"),
        None => "null".to_string(),
    };
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"in\":{},\"out\":{},\"restored\":{},\"notice\":{}}}",
        point(load.area.in_point),
        point(load.area.out_point),
        load.restored,
        notice
    )
}

/// Clamp one frame into the work area (Sprint 16, T004): the frame
/// plays unchanged inside the area (or with no area); outside it parks
/// on the nearer bound and reports the clamp, so the status line names
/// the bound instead of silently seeking past it.
pub fn clamp_frame_to_work_area(
    frame: u64,
    area: &WorkArea,
    fps_value: f64,
    frame_count: u64,
) -> (u64, bool) {
    let (inp, outp) = match (area.in_point, area.out_point) {
        (Some(i), Some(o)) => (i, o),
        _ => return (frame, false),
    };
    if fps_value <= 0.0 || frame_count == 0 {
        return (frame, false);
    }
    let last = frame_count.saturating_sub(1);
    let in_f = (inp * fps_value).floor().max(0.0) as u64;
    let out_f = ((outp * fps_value).floor().max(0.0) as u64).min(last);
    if in_f > out_f {
        return (in_f.min(last), frame != in_f.min(last));
    }
    let clamped = frame.clamp(in_f, out_f);
    (clamped, clamped != frame)
}
