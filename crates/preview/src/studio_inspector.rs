//! Inspector rows for the Studio shell (Sprint 17, T007 / CAP-007,
//! J-04, INT-002, REC-001).
//!
//! The inspector owns property rows and their reset actions; each
//! row edit is a patch proposal. Row edits travel the identical patch
//! path as canvas gestures ([`apply_inspector_edit`] builds SDK ops
//! and applies them through [`crate::studio::apply_studio_edit`]), so
//! row edits and canvas edits share one undo system: the row shows
//! the applied value and the turn appears in history. Reset is itself
//! an undoable turn — it writes the row default through the same op.
//!
//! Rows cover the render-supported 2D property set only. Render-
//! deferred rows (`filter`, `transform-3d` and its `rotate` /
//! `perspective` / `origin` keys) list with `editable: false` and the
//! milestone that unlocks them, never faked. A row value the
//! composition cannot support refuses with its rule and location
//! while the row keeps its prior value; nothing stages, previews, or
//! records.
//!
//! Style boundary: row keys stay CSS-like hyphenated end to end
//! (`background-color`, `border-radius`, `object-fit`, `z-index`),
//! and `font-family` try-order lists pass through untouched with
//! fail-closed resolution at render time, exactly like the SDK rows
//! module. This layer writes no files directly: edits apply through
//! validated turns, and state besides those turns lives only in the
//! hidden project files the SDK owns.

use std::path::Path;

/// Inspector groups in display order: the ten row families the
/// specification names (Text, Style, Layout, Motion, Grade, Effects,
/// Overlays, Media, Audio FX, Timing).
pub const INSPECTOR_GROUPS: [&str; 10] = [
    "Text", "Style", "Layout", "Motion", "Grade", "Effects", "Overlays", "Media", "Audio FX",
    "Timing",
];

/// Value kind of one row, driving the control the shell paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorKind {
    /// Free authored text (`value`, `font-family`, `source`).
    Text,
    /// Authored number (`font-size`, `x`, `opacity`, ...).
    Number,
    /// Hex color (`color`, `background-color`, `border-color`).
    Color,
    /// Closed vocabulary (`text-align`, `object-fit`, ...).
    Select,
    /// Exactly `true` or `false` (`visible`, `loop`, `waveform`).
    Boolean,
}

impl InspectorKind {
    /// Served spelling of the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            InspectorKind::Text => "text",
            InspectorKind::Number => "number",
            InspectorKind::Color => "color",
            InspectorKind::Select => "select",
            InspectorKind::Boolean => "boolean",
        }
    }
}

/// One row definition: its group, CSS-like property key, value
/// kind, and the closed vocabulary for selects (empty otherwise).
pub struct InspectorRowDef {
    /// Display group, one of [`INSPECTOR_GROUPS`].
    pub group: &'static str,
    /// CSS-like hyphenated row key (Timing rows ride `Retiming`).
    pub property: &'static str,
    /// Value kind driving the shell control.
    pub kind: InspectorKind,
    /// Accepted spellings for [`InspectorKind::Select`], else empty.
    pub options: &'static [&'static str],
}

/// Row catalog in display order: every editable row plus the
/// render-deferred rows the milestone marks unavailable. Timing rows
/// (`start`, `duration`, `trim-start`) edit through the existing
/// `Retiming` op; `visible` edits through `SetHidden`; every other
/// editable row edits through `SetRow`.
pub const INSPECTOR_ROWS: &[InspectorRowDef] = &[
    InspectorRowDef { group: "Text", property: "value", kind: InspectorKind::Text, options: &[] },
    InspectorRowDef { group: "Text", property: "font-size", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Text", property: "color", kind: InspectorKind::Color, options: &[] },
    InspectorRowDef { group: "Text", property: "text-align", kind: InspectorKind::Select, options: &["left", "center", "right"] },
    InspectorRowDef { group: "Text", property: "font-style", kind: InspectorKind::Select, options: &["regular", "bold", "italic", "bold-italic"] },
    InspectorRowDef { group: "Text", property: "font-family", kind: InspectorKind::Text, options: &[] },
    InspectorRowDef { group: "Text", property: "line-height", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Text", property: "letter-spacing", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Style", property: "background-color", kind: InspectorKind::Color, options: &[] },
    InspectorRowDef { group: "Style", property: "border-width", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Style", property: "border-color", kind: InspectorKind::Color, options: &[] },
    InspectorRowDef { group: "Style", property: "border-radius", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Style", property: "opacity", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Style", property: "mix-blend-mode", kind: InspectorKind::Select, options: &["normal", "multiply", "screen", "overlay"] },
    InspectorRowDef { group: "Layout", property: "x", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Layout", property: "y", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Layout", property: "width", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Layout", property: "height", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Layout", property: "z-index", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Layout", property: "object-fit", kind: InspectorKind::Select, options: &["contain", "cover", "fill"] },
    InspectorRowDef { group: "Motion", property: "filter-blur", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Motion", property: "transform-3d", kind: InspectorKind::Text, options: &[] },
    InspectorRowDef { group: "Motion", property: "rotate-x", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Motion", property: "rotate-y", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Motion", property: "perspective", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Motion", property: "origin", kind: InspectorKind::Text, options: &[] },
    InspectorRowDef { group: "Grade", property: "grade-exposure", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Grade", property: "grade-contrast", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Grade", property: "grade-saturation", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Grade", property: "grade-temperature", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Grade", property: "grade-tint", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Effects", property: "effect-amount", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Effects", property: "filter", kind: InspectorKind::Text, options: &[] },
    InspectorRowDef { group: "Overlays", property: "waveform", kind: InspectorKind::Boolean, options: &[] },
    InspectorRowDef { group: "Overlays", property: "visible", kind: InspectorKind::Boolean, options: &[] },
    InspectorRowDef { group: "Media", property: "loop", kind: InspectorKind::Boolean, options: &[] },
    InspectorRowDef { group: "Media", property: "rate", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Media", property: "source", kind: InspectorKind::Text, options: &[] },
    InspectorRowDef { group: "Audio FX", property: "volume", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Audio FX", property: "fade-in", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Audio FX", property: "fade-out", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Audio FX", property: "delay", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Audio FX", property: "delay-feedback", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Audio FX", property: "delay-mix", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Audio FX", property: "reverb", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Timing", property: "start", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Timing", property: "duration", kind: InspectorKind::Number, options: &[] },
    InspectorRowDef { group: "Timing", property: "trim-start", kind: InspectorKind::Number, options: &[] },
];

/// Why an inspector request refused.
fn refused_inspector(message: String) -> String {
    format!("studio refused: {message}")
}

/// One parsed inspector request: the named actor, the clip the row
/// addresses, the row key, and either the authored value or a reset
/// to the row default. `selection` echoes the shared canvas and
/// timeline selection the shell scoped the inspector to.
#[derive(Debug, Clone)]
pub struct InspectorEdit {
    /// Actor the validated turn is recorded under.
    pub actor: String,
    /// Clip the row addresses.
    pub target: sdk::ClipRef,
    /// CSS-like row key (`font-size`, ...).
    pub property: String,
    /// Authored value (`None` while resetting).
    pub value: Option<String>,
    /// True while resetting the row to its default.
    pub reset: bool,
    /// Shared selection the inspector scopes to, echoed back.
    pub selection: Vec<String>,
}

/// Parse one inspector request body (`{"actor":"maker",
/// "target":{"id":"title"},"property":"font-size","value":"32"}` or
/// the same with `"reset":true` instead of a value). The actor must
/// be non-empty, the target must carry an `id` or an `index`, and
/// the property must be a known row key — deferred rows refuse here
/// naming their milestone, so the shell marks them unavailable
/// instead of sending them. A refused body names its cause and
/// stages nothing.
pub fn parse_inspector_body(body: &str) -> Result<InspectorEdit, String> {
    if body.trim().is_empty() {
        return Err(refused_inspector(
            "the inspector edit needs a JSON body (`{\"actor\":\"maker\",\"target\":{\"id\":\"title\"},\"property\":\"font-size\",\"value\":\"32\"}`); pass the row change. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_inspector(format!(
            "the inspector edit body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"target\":{{\"id\":\"title\"}},\"property\":\"font-size\",\"value\":\"32\"}}`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_inspector(
            "the inspector edit names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let target_value = value.get("target").ok_or_else(|| {
        refused_inspector(
            "the inspector edit carries no `target`; rows address one clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged.".to_string(),
        )
    })?;
    let target: sdk::ClipRef = serde_json::from_value(target_value.clone()).map_err(|e| {
        refused_inspector(format!(
            "the inspector edit `target` is not a clip reference ({e}); pass `target:{{\"id\":\"...\"}}` or `target:{{\"index\":N}}`. Nothing was staged."
        ))
    })?;
    if target.id.is_none() && target.index.is_none() {
        return Err(refused_inspector(
            "the inspector edit `target` names neither `id` nor `index`; rows address one clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged.".to_string(),
        ));
    }
    let property = value
        .get("property")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let property = sdk::canonical_row_property(&property);
    if property.is_empty() {
        return Err(refused_inspector(
            "the inspector edit names no `property`; rows address one row key. Pass a `property` like `font-size`. Nothing was staged.".to_string(),
        ));
    }
    if !INSPECTOR_ROWS.iter().any(|row| row.property == property) {
        return Err(refused_inspector(format!(
            "unknown inspector row {property:?}; rows cover {} — see the inspector for the row keys. Nothing was staged.",
            INSPECTOR_ROWS
                .iter()
                .map(|row| row.property)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if let sdk::RowAvailability::Unavailable { milestone } = sdk::row_availability(&property) {
        return Err(refused_inspector(format!(
            "inspector row `{property}` waits for the {milestone}; rows cover the render-supported 2D property set today. Nothing was staged."
        )));
    }
    let reset = value
        .get("reset")
        .and_then(|r| r.as_bool())
        .unwrap_or(false);
    let authored = value
        .get("value")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if !reset && authored.is_none() {
        return Err(refused_inspector(format!(
            "inspector row `{property}` carries no `value`; pass `\"value\":\"...\"` to edit or `\"reset\":true` to restore the default. Nothing was staged."
        )));
    }
    let selection = value
        .get("selection")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    Ok(InspectorEdit {
        actor,
        target,
        property,
        value: authored,
        reset,
        selection,
    })
}

/// Render one number the way rows show it.
fn num_text(x: f64) -> String {
    if x.is_finite() && x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// Live Timing value of one clip, or `None` while the row does not
/// apply (`trim-start` reads footage clips only).
fn timing_live_value(clip: &notation::model::Clip, property: &str) -> Option<String> {
    match property {
        "start" => Some(num_text(clip.start())),
        "duration" => Some(num_text(clip.duration())),
        "trim-start" => match clip {
            notation::model::Clip::Video(c) => Some(num_text(c.trim_start)),
            notation::model::Clip::Audio(c) => Some(num_text(c.trim_start)),
            notation::model::Clip::Nest(c) => Some(num_text(c.trim_start)),
            _ => None,
        },
        _ => None,
    }
}

/// True while one catalog row is a Timing row.
fn is_timing_row(property: &str) -> bool {
    matches!(property, "start" | "duration" | "trim-start")
}

/// Parse one authored Timing number: finite, else a refusal naming
/// the row and the clip.
fn parse_timing_number(
    raw: &str,
    property: &str,
    clip: &notation::model::Clip,
    target: &sdk::ClipRef,
) -> Result<f64, String> {
    match raw.trim().parse::<f64>() {
        Ok(x) if x.is_finite() => Ok(x),
        _ => Err(refused_inspector(format!(
            "inspector row `{property}` value {raw:?} on {target} ({} clip) is not a finite number; write a plain number. Nothing was applied or recorded.",
            clip.kind()
        ))),
    }
}

/// Build the SDK patch operations for one inspector request against
/// the effective composition: Timing rows ride `Retiming`, `visible`
/// rides `SetHidden`, every other row rides `SetRow`. Reset writes
/// the row default (`duration` computes the composition end minus
/// the clip start, refusing while the end is unknown). A refused
/// value names its rule and location and builds no ops, so the row
/// keeps its prior value.
pub fn inspector_edit_ops(
    comp: &notation::model::Composition,
    edit: &InspectorEdit,
) -> Result<Vec<sdk::EditOp>, String> {
    let idx = resolve_target(comp, &edit.target)?;
    let clip = &comp.clips[idx];
    let loc = format!("{} clip at {}", clip.kind(), edit.target);
    if is_timing_row(&edit.property) {
        let start = clip.start();
        let duration = clip.duration();
        let old_trim = match clip {
            notation::model::Clip::Video(c) => Some(c.trim_start),
            notation::model::Clip::Audio(c) => Some(c.trim_start),
            notation::model::Clip::Nest(c) => Some(c.trim_start),
            _ => None,
        };
        if edit.reset {
            let (new_start, new_duration, new_trim) = match edit.property.as_str() {
                "start" => (Some(0.0), None, None),
                "duration" => {
                    let end = comp.duration;
                    if !end.is_finite() || end <= start {
                        return Err(refused_inspector(format!(
                            "inspector row `duration` on {loc} cannot reset: the composition end ({end}) sits at or before the clip start ({start}); write an explicit `duration`. Nothing was applied or recorded."
                        )));
                    }
                    (None, Some(end - start), None)
                }
                _ => (None, None, Some(0.0)),
            };
            return Ok(vec![sdk::EditOp::Retiming {
                target: edit.target.clone(),
                start: new_start,
                duration: new_duration,
                trim_start: new_trim,
            }]);
        }
        let raw = edit.value.clone().unwrap_or_default();
        return match edit.property.as_str() {
            "start" => {
                let next = parse_timing_number(&raw, "start", clip, &edit.target)?;
                if next < 0.0 {
                    return Err(refused_inspector(format!(
                        "inspector row `start` {next} on {loc} runs before the film; write film seconds from 0. Nothing was applied or recorded."
                    )));
                }
                Ok(vec![sdk::EditOp::Retiming {
                    target: edit.target.clone(),
                    start: Some(next),
                    duration: None,
                    trim_start: None,
                }])
            }
            "duration" => {
                let next = parse_timing_number(&raw, "duration", clip, &edit.target)?;
                if next <= 0.0 {
                    return Err(refused_inspector(format!(
                        "inspector row `duration` {next} on {loc} is not positive; write a positive film window. Nothing was applied or recorded."
                    )));
                }
                Ok(vec![sdk::EditOp::Retiming {
                    target: edit.target.clone(),
                    start: None,
                    duration: Some(next),
                    trim_start: None,
                }])
            }
            _ => {
                if old_trim.is_none() {
                    return Err(refused_inspector(format!(
                        "inspector row `trim-start` on {loc} opens a file window on `video`, `audio`, and `nest` clips only; this {} keeps its prior value. Nothing was applied or recorded.",
                        clip.kind()
                    )));
                }
                let next = parse_timing_number(&raw, "trim-start", clip, &edit.target)?;
                if next < 0.0 {
                    return Err(refused_inspector(format!(
                        "inspector row `trim-start` {next} on {loc} runs before the source; write source seconds from 0. Nothing was applied or recorded."
                    )));
                }
                let _ = duration;
                Ok(vec![sdk::EditOp::Retiming {
                    target: edit.target.clone(),
                    start: None,
                    duration: None,
                    trim_start: Some(next),
                }])
            }
        };
    }
    if edit.property == "visible" {
        let hidden_now = clip.hidden();
        if edit.reset {
            return Ok(vec![sdk::EditOp::SetHidden {
                target: edit.target.clone(),
                hidden: false,
            }]);
        }
        let raw = edit.value.clone().unwrap_or_default();
        match raw.trim() {
            "true" => Ok(vec![sdk::EditOp::SetHidden {
                target: edit.target.clone(),
                hidden: false,
            }]),
            "false" => Ok(vec![sdk::EditOp::SetHidden {
                target: edit.target.clone(),
                hidden: true,
            }]),
            _ => Err(refused_inspector(format!(
                "inspector row `visible` value {raw:?} on {loc} is not `true` or `false`; write a boolean. Nothing was applied or recorded. (Prior `visible` is {}.)", !hidden_now
            ))),
        }
    } else {
        if edit.reset {
            match sdk::row_default_value(&edit.property) {
                Some(sdk::RowDefault::Value(v)) => Ok(vec![sdk::EditOp::SetRow {
                    target: edit.target.clone(),
                    property: edit.property.clone(),
                    value: v.to_string(),
                }]),
                // Inherited and default-less rows reset by clearing:
                // optional rows inherit, required rows refuse with
                // rule and location keeping the prior value.
                Some(sdk::RowDefault::Inherit) | Some(sdk::RowDefault::None) => {
                    Ok(vec![sdk::EditOp::SetRow {
                        target: edit.target.clone(),
                        property: edit.property.clone(),
                        value: String::new(),
                    }])
                }
                Option::None => Err(refused_inspector(format!(
                    "inspector row {:?} on {loc} keeps no reset default; write an explicit value. Nothing was applied or recorded.",
                    edit.property
                ))),
            }
        } else {
            // A `source` row must keep pointing at a readable file:
            // refuse a missing file here with rule and location
            // before the SDK stores the path.
            if edit.property == "source" {
                let raw = edit.value.clone().unwrap_or_default();
                if raw.trim().is_empty() {
                    return Err(refused_inspector(format!(
                        "inspector row `source` on {loc} is required and keeps no default; write a media path. Nothing was applied or recorded."
                    )));
                }
            }
            Ok(vec![sdk::EditOp::SetRow {
                target: edit.target.clone(),
                property: edit.property.clone(),
                value: edit.value.clone().unwrap_or_default(),
            }])
        }
    }
}

/// Resolve one inspector target against the effective clips: `id`
/// first, then document index. An `id` that names nothing refuses
/// even with an index present; an index past the end refuses;
/// neither set refuses.
fn resolve_target(
    comp: &notation::model::Composition,
    target: &sdk::ClipRef,
) -> Result<usize, String> {
    if let Some(id) = &target.id {
        if let Some(idx) = comp.clips.iter().position(|c| c.id() == Some(id.as_str())) {
            return Ok(idx);
        }
        return Err(refused_inspector(format!(
            "inspector target clip id {id:?} names nothing; the open composition holds {} clip(s). Nothing was applied or recorded.",
            comp.clips.len()
        )));
    }
    if let Some(index) = target.index {
        if index < comp.clips.len() {
            return Ok(index);
        }
        return Err(refused_inspector(format!(
            "inspector target clip index {index} is past the open composition holding {} clip(s). Nothing was applied or recorded.",
            comp.clips.len()
        )));
    }
    Err(refused_inspector(
        "the inspector edit `target` names neither `id` nor `index`; rows address one clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged.".to_string(),
    ))
}

/// One applied inspector turn.
#[derive(Debug, Clone)]
pub struct InspectorReceipt {
    /// Patch operations the turn recorded.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// Clip the row addressed, echoed for selection sync.
    pub target: String,
    /// Shared selection the inspector scoped to, echoed back so
    /// canvas and timeline show the same selection at once.
    pub selection: Vec<String>,
}

/// Apply one inspector request through the Sprint 15 patch and
/// history machinery: build the row ops against the effective
/// composition, then record one validated turn under the actor —
/// the same undo system as canvas gestures and headless edits. A
/// refused value names its rule and location while the composition,
/// the delta, and history stay exactly as they were.
pub fn apply_inspector_edit(
    folder: &Path,
    composition: Option<&str>,
    edit: &InspectorEdit,
) -> Result<InspectorReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_inspector(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    // A `source` row must keep pointing at a readable file: check
    // existence against the project folder before the SDK stores
    // the path, so a typo refuses instead of staging a dead link.
    if edit.property == "source" && !edit.reset {
        let raw = edit.value.clone().unwrap_or_default();
        let candidate = Path::new(raw.trim());
        let resolved = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            folder.join(candidate)
        };
        if !resolved.is_file() {
            return Err(refused_inspector(format!(
                "inspector row `source` {:?} on {} names no readable file; point the row at a media file inside the project. Nothing was applied or recorded.",
                raw.trim(),
                edit.target
            )));
        }
    }
    let shown = crate::studio::effective_composition_for_session(folder, composition)?;
    let ops = inspector_edit_ops(&shown, edit)?;
    let studio_edit = crate::studio::StudioEdit {
        actor: edit.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &studio_edit) {
        Ok(receipt) => Ok(InspectorReceipt {
            applied: receipt.applied,
            turn: receipt.turn,
            actor: receipt.actor,
            summaries: receipt.summaries,
            target: edit.target.to_string(),
            selection: edit.selection.clone(),
        }),
        Err(message) => Err(message),
    }
}

/// Render one applied inspector receipt as JSON for the row API:
/// the turn, its summaries, and the echoed target and selection so
/// canvas and timeline stay in sync.
pub fn inspector_receipt_json(receipt: &InspectorReceipt) -> String {
    format!(
        "{{\"saved\":true,\"applied\":{},\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}],\"target\":\"{}\",\"selection\":[{}]}}",
        receipt.applied,
        receipt.turn,
        crate::studio::json_escape(&receipt.actor),
        receipt
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
            .collect::<Vec<_>>()
            .join(","),
        crate::studio::json_escape(&receipt.target),
        receipt
            .selection
            .iter()
            .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Live rows of one clip: every catalog row in group order. Rows
/// that apply show their live value with the reset default; rows
/// the kind cannot support are skipped; render-deferred rows show
/// unavailable with the milestone named. `comp_end` is the film
/// length in seconds backing the `duration` reset default.
pub fn inspector_json(
    comp: &notation::model::Composition,
    target: &sdk::ClipRef,
    selection: &[String],
) -> Result<String, String> {
    let idx = resolve_target(comp, target)?;
    let clip = &comp.clips[idx];
    let mut groups = String::new();
    for group in INSPECTOR_GROUPS {
        let mut rows = String::new();
        for def in INSPECTOR_ROWS.iter().filter(|row| row.group == group) {
            let availability = if is_timing_row(def.property) || def.property == "visible" {
                sdk::RowAvailability::Available
            } else {
                sdk::row_availability(def.property)
            };
            match availability {
                sdk::RowAvailability::Unavailable { milestone } => {
                    rows.push_str(&format!(
                        "{{\"property\":\"{}\",\"kind\":\"{}\",\"value\":\"\",\"default\":\"\",\"editable\":false,\"milestone\":\"{}\",\"options\":[]}},",
                        def.property,
                        def.kind.as_str(),
                        milestone
                    ));
                }
                sdk::RowAvailability::Unknown => {
                    return Err(refused_inspector(format!(
                        "inspector catalog row {:?} names nothing settable; this is a shell bug, not a row value. Nothing was staged.",
                        def.property
                    )));
                }
                sdk::RowAvailability::Available => {
                    let live = if is_timing_row(def.property) {
                        timing_live_value(clip, def.property)
                    } else if def.property == "visible" {
                        Some((!clip.hidden()).to_string())
                    } else {
                        sdk::row_live_value(clip, def.property)
                    };
                    let Some(live) = live else { continue };
                    let default = if is_timing_row(def.property) {
                        match def.property {
                            "start" => "0".to_string(),
                            "duration" => {
                                let rest = comp.duration - clip.start();
                                if rest.is_finite() && rest > 0.0 {
                                    num_text(rest)
                                } else {
                                    String::new()
                                }
                            }
                            _ => "0".to_string(),
                        }
                    } else if def.property == "visible" {
                        "true".to_string()
                    } else {
                        match sdk::row_default_value(def.property) {
                            Some(sdk::RowDefault::Value(v)) => v.to_string(),
                            Some(sdk::RowDefault::Inherit) => String::new(),
                            Some(sdk::RowDefault::None) | Option::None => String::new(),
                        }
                    };
                    let options = def
                        .options
                        .iter()
                        .map(|o| format!("\"{o}\""))
                        .collect::<Vec<_>>()
                        .join(",");
                    rows.push_str(&format!(
                        "{{\"property\":\"{}\",\"kind\":\"{}\",\"value\":\"{}\",\"default\":\"{}\",\"editable\":true,\"milestone\":null,\"options\":[{}]}},",
                        def.property,
                        def.kind.as_str(),
                        crate::studio::json_escape(&live),
                        crate::studio::json_escape(&default),
                        options
                    ));
                }
            }
        }
        if rows.ends_with(',') {
            rows.pop();
        }
        groups.push_str(&format!("{{\"group\":\"{group}\",\"rows\":[{rows}]}},",));
    }
    if groups.ends_with(',') {
        groups.pop();
    }
    let selection_json = selection
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        "{{\"composition\":\"{}\",\"target\":\"{}\",\"selection\":[{}],\"groups\":[{}]}}",
        crate::studio::json_escape(&comp.name),
        crate::studio::json_escape(&target.to_string()),
        selection_json,
        groups
    ))
}

/// Parse one inspector target query (`?target=<id-or-index>`): a
/// whole number reads as a document index, anything else as a
/// stable `id`.
pub fn parse_inspector_target(raw: &str) -> sdk::ClipRef {
    match raw.trim().parse::<usize>() {
        Ok(index) => sdk::ClipRef::by_index(index),
        Err(_) => sdk::ClipRef::by_id(raw.trim()),
    }
}
