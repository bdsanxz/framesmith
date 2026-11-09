//! Selection context and agent handoffs (Sprint 20, T007 / CAP-008, J-04,
//! INT-003).
//!
//! The shared readout agents and makers read before acting: preview and
//! Studio context in one place — the composition, the preview time, and
//! the selected elements — plus two maker handoffs (`ask-agent` and
//! `copy-to-agent`) that hand the maker a ready agent request from the
//! current selection. Agent edits themselves keep travelling the Sprint
//! 15 patch and history machinery through the T006 Studio tools, so
//! every agent turn lands bracketed by its agent name in the same undo
//! system humans use, and the maker undoes an agent turn like any turn.
//!
//! The contract, per readout:
//!
//! * Context is read-only: it opens nothing writable, stages no patch,
//!   and records no history turn. Reading with and without a selection
//!   leaves the sources, the delta, and history exactly as they were.
//! * No selection reads as an explicit empty selection (`"selection":[]`
//!   with a note naming the empty state), never as the whole timeline.
//! * The handoffs refuse an empty selection (naming the need to pick)
//!   and unknown entries (naming the timeline holdings); a refused
//!   handoff stages nothing.
//! * Unknown compositions refuse naming the project holdings.
//!
//! Request keys stay hyphenated (`selection-note`, `time-secs`); style
//! attributes stay CSS-like hyphenated end to end and `font-family`
//! try-order lists pass through untouched, exactly like every other
//! read path.

use crate::studio::StudioRow;
use crate::studio_shortcuts::HandoffClip;

/// Prefix every context and handoff refusal carries; the cause follows
/// it, and nothing stages.
fn refused_context(message: String) -> String {
    format!("studio refused: {message}")
}

/// Resolve one agent selection against the timeline rows (Sprint 20,
/// T007): `#3` addresses document index 3, anything else a declared
/// clip id. An empty selection resolves to no clips — the readout
/// names the empty state explicitly and never expands to the whole
/// timeline. Unknown entries refuse naming the timeline holdings, so
/// a copied request never points at nothing.
pub fn resolve_agent_selection(
    rows: &[StudioRow],
    entries: &[String],
) -> Result<Vec<HandoffClip>, String> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    crate::studio_shortcuts::resolve_handoff_selection(rows, entries)
        .map_err(|message| refused_context(format!("{message} Nothing was staged or recorded.")))
}

/// Render one selected clip for the context readout: the picked key,
/// the resolved id, the kind, and the film window it spans.
fn selected_clip_json(clip: &HandoffClip) -> String {
    let id = match &clip.id {
        Some(id) => format!("\"{}\"", crate::studio::json_escape(id)),
        None => "null".to_string(),
    };
    format!(
        "{{\"key\":\"{}\",\"id\":{id},\"kind\":\"{}\",\"window\":[{:.3},{:.3}]}}",
        crate::studio::json_escape(&clip.key),
        crate::studio::json_escape(&clip.kind),
        clip.start,
        clip.end,
    )
}

/// Render the read-only selection context (Sprint 20, T007 / CAP-008):
/// the composition, the preview time (frame, seconds, duration, fps),
/// and the selected elements. An empty selection reads explicitly
/// empty with no clips, never the whole timeline.
pub fn agent_context_json(
    composition: &str,
    frame: u64,
    fps_value: f64,
    duration_secs: f64,
    selection: &[HandoffClip],
) -> String {
    let time_secs = if fps_value > 0.0 {
        frame as f64 / fps_value
    } else {
        0.0
    };
    let keys = selection
        .iter()
        .map(|c| format!("\"{}\"", crate::studio::json_escape(&c.key)))
        .collect::<Vec<_>>()
        .join(",");
    let clips = selection
        .iter()
        .map(selected_clip_json)
        .collect::<Vec<_>>()
        .join(",");
    let note = if selection.is_empty() {
        "empty selection (no clips picked)".to_string()
    } else {
        format!(
            "{} clip(s) picked: {}",
            selection.len(),
            selection
                .iter()
                .map(|c| c.key.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "{{\"context\":true,\"composition\":\"{}\",\"frame\":{},\"time-secs\":{:.3},\"duration-secs\":{:.3},\"fps\":{},\"selection\":[{}],\"selection-note\":\"{}\",\"clips\":[{}]}}",
        crate::studio::json_escape(composition),
        frame,
        time_secs,
        duration_secs,
        fps_value,
        keys,
        crate::studio::json_escape(&note),
        clips,
    )
}

/// Display one selected clip for the ask-agent request: the picked key
/// with its kind and window, naming the resolved `id` when the pick
/// used a `#index` entry.
fn ask_clip_text(clip: &HandoffClip) -> String {
    match &clip.id {
        Some(id) if id != &clip.key => format!(
            "{} {:?} (id {:?}, {:.3}s..{:.3}s)",
            clip.kind, clip.key, id, clip.start, clip.end
        ),
        _ => format!(
            "{} {:?} ({:.3}s..{:.3}s)",
            clip.kind, clip.key, clip.start, clip.end
        ),
    }
}

/// Render the ask-agent request (Sprint 20, T007 / CAP-008, J-04): a
/// ready question the maker pastes to the agent, naming the
/// composition, the preview time, and exactly the selected clips, with
/// a slot for the requested change. The agent applies it through the
/// edit SDK as a named history turn and reports the turn receipt.
pub fn ask_agent_text(
    composition: &str,
    frame: u64,
    fps_value: f64,
    duration_secs: f64,
    selection: &[HandoffClip],
) -> String {
    let time_secs = if fps_value > 0.0 {
        frame as f64 / fps_value
    } else {
        0.0
    };
    let list = selection
        .iter()
        .map(ask_clip_text)
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Ask the Studio agent: in composition {composition:?} at frame {frame} ({time_secs:.3}s of {duration_secs:.3}s), change only the selected clips ({}): {list}. Requested change: describe the edit to apply to this selection. Apply it through the edit SDK as a named history turn and report the turn receipt.",
        selection.len(),
    )
}

/// Render one handoff envelope (Sprint 20, T007): the handoff kind
/// (`ask-agent` or `copy-to-agent`), the composition, the picked keys
/// in order, and the ready request text.
pub fn handoff_json(
    kind: &str,
    composition: &str,
    selection: &[HandoffClip],
    request: &str,
) -> String {
    let keys = selection
        .iter()
        .map(|c| format!("\"{}\"", crate::studio::json_escape(&c.key)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"handoff\":true,\"kind\":\"{}\",\"composition\":\"{}\",\"selection\":[{}],\"request\":\"{}\"}}",
        crate::studio::json_escape(kind),
        crate::studio::json_escape(composition),
        keys,
        crate::studio::json_escape(request),
    )
}
