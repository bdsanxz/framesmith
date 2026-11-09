//! Studio agent tools (Sprint 20, T006 / CAP-007, J-03, STATE-05,
//! INT-002, REC-004).
//!
//! The twelve tools agents drive Studio with: `look`, `inspect`, and
//! `frame` answer reads with the current composition truth; `select`
//! and `seek` stage view state (the picked clips, the playhead) with a
//! receipt and no history turn, exactly like the timeline view tools
//! they mirror; `set-text`, `set-style`, `transform`, `add-animation`,
//! `add-keyframe`, `update-animation`, and `delete-animation` each stage
//! one edit (a transform row-set rides one row op per given field, in
//! field order) and apply it through the shared Sprint 15 patch and
//! history machinery
//! ([`crate::studio::apply_studio_edit`]), so every agent turn lands in
//! the same undo system as human edits and reads back with the same
//! receipt shape.
//!
//! The contract, per tool:
//!
//! * Reads return current truth and stage nothing: no patch, no delta
//!   write, no history turn.
//! * Every write returns a staged receipt naming what will apply (the
//!   one-line patch summaries) plus the recorded history turn. No tool
//!   renders, and no tool writes without its receipt step: a refused
//!   tool stages nothing and names its cause (`agent refused: ...`),
//!   with the SDK reason carried along.
//! * A missing composition refuses naming the available names; a
//!   missing clip, tween, key, or set refuses naming the target; an
//!   unchanged `set-text` refuses naming the clean state instead of
//!   recording an empty turn.
//!
//! Request bodies use hyphenated keys (`target-id`, `target-index`,
//! `path-to`); targets ride as a declared `id` or a `#index` entry,
//! matching the timeline tool spelling. Style attributes stay
//! CSS-like hyphenated end to end and `font-family` try-order lists
//! pass through untouched, exactly like every other write path.

use std::path::Path;

/// Actor every agent tool turn records under when the body names none:
/// the agent driving Studio (human turns keep their own names).
pub const AGENT_TOOL_ACTOR: &str = "agent";

/// Prefix every agent refusal carries; the SDK reason follows it.
fn refused_agent(message: String) -> String {
    format!("agent refused: {message}")
}

/// One Studio agent tool (Sprint 20, T006): the vocabulary agents
/// serve. Reads (`look`, `inspect`, `frame`) answer current truth;
/// `select` and `seek` set view state with a receipt and no turn;
/// the remaining seven stage composition edits with a staged receipt
/// and a shared history turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolName {
    /// Composition overview: names, clips, window, canvas.
    Look,
    /// One clip's current truth by `target-id` or `target-index`.
    Inspect,
    /// Clips active at `at` seconds.
    Frame,
    /// Pick clips by `targets` (`id` or `#index` entries).
    Select,
    /// Move the playhead to `at` seconds (or `frame` frames).
    Seek,
    /// Set a text clip's `value` (one row op).
    SetText,
    /// Set one CSS-like style row (one row op).
    SetStyle,
    /// Move/resize/retime one clip (one row op per given field).
    Transform,
    /// Append one tween on one clip (one tween op).
    AddAnimation,
    /// Retarget one tween's ease or path (one op per given field).
    UpdateAnimation,
    /// Append one key on one clip (one key op).
    AddKeyframe,
    /// Remove one tween, key, or set by `kind` and `index` (one op).
    DeleteAnimation,
}

impl AgentToolName {
    /// The served spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentToolName::Look => "look",
            AgentToolName::Inspect => "inspect",
            AgentToolName::Frame => "frame",
            AgentToolName::Select => "select",
            AgentToolName::Seek => "seek",
            AgentToolName::SetText => "set-text",
            AgentToolName::SetStyle => "set-style",
            AgentToolName::Transform => "transform",
            AgentToolName::AddAnimation => "add-animation",
            AgentToolName::UpdateAnimation => "update-animation",
            AgentToolName::AddKeyframe => "add-keyframe",
            AgentToolName::DeleteAnimation => "delete-animation",
        }
    }

    /// Parse one tool spelling; anything else refuses naming every tool.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "look" => Some(AgentToolName::Look),
            "inspect" => Some(AgentToolName::Inspect),
            "frame" => Some(AgentToolName::Frame),
            "select" => Some(AgentToolName::Select),
            "seek" => Some(AgentToolName::Seek),
            "set-text" => Some(AgentToolName::SetText),
            "set-style" => Some(AgentToolName::SetStyle),
            "transform" => Some(AgentToolName::Transform),
            "add-animation" => Some(AgentToolName::AddAnimation),
            "update-animation" => Some(AgentToolName::UpdateAnimation),
            "add-keyframe" => Some(AgentToolName::AddKeyframe),
            "delete-animation" => Some(AgentToolName::DeleteAnimation),
            _ => None,
        }
    }

    /// Every served spelling, for refusal messages.
    pub const ALL: &'static [&'static str] = &[
        "look",
        "inspect",
        "frame",
        "select",
        "seek",
        "set-text",
        "set-style",
        "transform",
        "add-animation",
        "update-animation",
        "add-keyframe",
        "delete-animation",
    ];

    /// True for the pure reads: they stage nothing and record no turn.
    pub fn is_read(self) -> bool {
        matches!(
            self,
            AgentToolName::Look | AgentToolName::Inspect | AgentToolName::Frame
        )
    }

    /// True for the composition writers: they stage ops and record one
    /// shared history turn. `select` and `seek` write view state with
    /// a receipt but no turn.
    pub fn is_composition_write(self) -> bool {
        matches!(
            self,
            AgentToolName::SetText
                | AgentToolName::SetStyle
                | AgentToolName::Transform
                | AgentToolName::AddAnimation
                | AgentToolName::UpdateAnimation
                | AgentToolName::AddKeyframe
                | AgentToolName::DeleteAnimation
        )
    }
}

/// One parsed agent tool call: the tool, the actor the turn records
/// under, the optional composition scope, and the raw argument map the
/// tool reads its fields from.
#[derive(Debug, Clone)]
pub struct AgentCall {
    /// The tool to run.
    pub tool: AgentToolName,
    /// Actor the recorded turn carries (human or agent name).
    pub actor: String,
    /// Composition scope: the render root by default, looked up by
    /// name or id when set.
    pub composition: Option<String>,
    /// Raw argument map (hyphenated keys).
    pub args: serde_json::Map<String, serde_json::Value>,
}

/// Read one optional string field; a present-but-not-string value
/// refuses naming the field.
fn get_str(
    args: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<String>, String> {
    match args.get(field) {
        None => Ok(None),
        Some(v) => match v.as_str() {
            Some(s) => Ok(Some(s.to_string())),
            None => Err(refused_agent(format!(
                "the agent `{field}` value {v} is not text; pass a quoted string. Nothing was staged or recorded."
            ))),
        },
    }
}

/// Read one optional number field; a present-but-not-number value
/// refuses naming the field.
fn get_num(
    args: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<f64>, String> {
    match args.get(field) {
        None => Ok(None),
        Some(v) => match v.as_f64() {
            Some(n) => Ok(Some(n)),
            None => Err(refused_agent(format!(
                "the agent `{field}` value {v} is not a number; pass a JSON number. Nothing was staged or recorded."
            ))),
        },
    }
}

/// Require one number field: missing refuses naming the field.
fn need_num(
    args: &serde_json::Map<String, serde_json::Value>,
    tool: &str,
    field: &str,
) -> Result<f64, String> {
    match get_num(args, field)? {
        Some(n) => Ok(n),
        None => Err(refused_agent(format!(
            "the agent `{tool}` names no `{field}`; pass the {field} to stage. Nothing was staged or recorded."
        ))),
    }
}

/// Require one text field: missing or blank refuses naming the field.
fn need_str(
    args: &serde_json::Map<String, serde_json::Value>,
    tool: &str,
    field: &str,
) -> Result<String, String> {
    match get_str(args, field)? {
        Some(s) if !s.trim().is_empty() => Ok(s),
        _ => Err(refused_agent(format!(
            "the agent `{tool}` names no `{field}`; pass the {field} to stage. Nothing was staged or recorded."
        ))),
    }
}

/// Refuse a non-finite number naming the field and the bound.
fn need_finite(tool: &str, field: &str, value: f64) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(refused_agent(format!(
            "the agent `{tool}` `{field}` value {value} is not finite; pass a finite number. Nothing was staged or recorded."
        )))
    }
}

/// Parse one agent tool body
/// (`{"tool":"seek","actor":"agent","at":1.5}`): the tool must name one
/// of the twelve, the actor must be non-empty, and per-tool fields
/// parse here while per-tool values validate at dispatch. A refused
/// body names its cause and stages nothing.
pub fn parse_agent_body(body: &str) -> Result<AgentCall, String> {
    if body.trim().is_empty() {
        return Err(refused_agent(
            "the agent tool needs a JSON tool body (`{\"tool\":\"look\",\"actor\":\"agent\"}`); pass the tool with its fields. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_agent(format!(
            "the agent tool body is not JSON ({e}). Pass `{{\"tool\":\"look\",\"actor\":\"agent\"}}`. Nothing was staged or recorded."
        ))
    })?;
    let map = value.as_object().ok_or_else(|| {
        refused_agent(
            "the agent tool body is not a JSON object; pass the tool with its fields. Nothing was staged or recorded."
                .to_string(),
        )
    })?;
    let tool_raw = map.get("tool").and_then(|t| t.as_str()).unwrap_or("");
    let tool = AgentToolName::parse(tool_raw).ok_or_else(|| {
        refused_agent(format!(
            "unknown agent tool {tool_raw:?}: Studio serves {}. Pass one of those tools. Nothing was staged or recorded.",
            AgentToolName::ALL.join(", ")
        ))
    })?;
    let actor = map
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or(AGENT_TOOL_ACTOR)
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_agent(
            "the agent tool names an empty actor; tool turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let composition = match map.get("composition").and_then(|c| c.as_str()) {
        None => None,
        Some(raw) if raw.trim().is_empty() => None,
        Some(raw) => Some(raw.to_string()),
    };
    Ok(AgentCall {
        tool,
        actor,
        composition,
        args: map.clone(),
    })
}

/// Parse one clip reference from `target-id` or `target-index`: the
/// `id` wins when both ride along; neither refuses naming the tool.
fn parse_target(
    args: &serde_json::Map<String, serde_json::Value>,
    tool: &str,
) -> Result<sdk::ClipRef, String> {
    let id = get_str(args, "target-id")?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let index = match args.get("target-index") {
        None => None,
        Some(v) => Some(v.as_u64().ok_or_else(|| {
            refused_agent(format!(
                "the agent `{tool}` `target-index` value {v} is not a whole clip index; pass a non-negative integer. Nothing was staged or recorded."
            ))
        })? as usize),
    };
    match (id, index) {
        (Some(id), _) => Ok(sdk::ClipRef::by_id(id)),
        (None, Some(index)) => Ok(sdk::ClipRef::by_index(index)),
        (None, None) => Err(refused_agent(format!(
            "the agent `{tool}` names no target; pass `target-id` (a declared clip id) or `target-index` (a document index). Nothing was staged or recorded."
        ))),
    }
}

/// Parse one `#index`-or-`id` selection entry: `#3` addresses document
/// index 3, anything else a declared clip id.
fn parse_selection_entry(raw: &str) -> sdk::ClipRef {
    match raw.strip_prefix('#') {
        Some(rest) => match rest.parse::<usize>() {
            Ok(index) => sdk::ClipRef::by_index(index),
            Err(_) => sdk::ClipRef::by_id(raw),
        },
        None => sdk::ClipRef::by_id(raw),
    }
}

/// Display key of one clip: its declared `id`, else `#index`.
fn clip_key(comp: &notation::model::Composition, idx: usize) -> String {
    match comp.clips[idx].id() {
        Some(id) => id.to_string(),
        None => format!("#{idx}"),
    }
}

/// Resolve one clip reference against the open composition: an `id`
/// that names nothing refuses naming the id, an out-of-range index
/// refuses naming the bound, and a bare reference refuses naming the
/// tool. Nothing stages on refusal.
fn resolve_target(
    comp: &notation::model::Composition,
    tool: &str,
    target: &sdk::ClipRef,
) -> Result<usize, String> {
    if let Some(id) = &target.id {
        if let Some(idx) = comp.clips.iter().position(|c| c.id() == Some(id.as_str())) {
            return Ok(idx);
        }
        return Err(refused_agent(format!(
            "the agent `{tool}` target clip id {id:?} names nothing; the open composition holds {} clip(s). Nothing was staged or recorded.",
            comp.clips.len()
        )));
    }
    if let Some(index) = target.index {
        if index < comp.clips.len() {
            return Ok(index);
        }
        return Err(refused_agent(format!(
            "the agent `{tool}` target clip index {index} is past the open composition holding {} clip(s). Nothing was staged or recorded.",
            comp.clips.len()
        )));
    }
    Err(refused_agent(format!(
        "the agent `{tool}` target names neither `id` nor `index`; tools address clips. Nothing was staged or recorded."
    )))
}

/// Open one composition for reading: a dry-run session that writes
/// nothing, not even the history directory. A missing project or
/// composition refuses naming the cause; reads stage nothing by
/// construction.
fn open_for_read(folder: &Path, composition: Option<&str>) -> Result<sdk::SdkSession, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_agent(format!(
            "the project {} is unreachable (moved or deleted); the agent tools stay read-only with nothing staged. Restore the folder and retry. Nothing was staged or recorded.",
            folder.display()
        )));
    }
    sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            preview: true,
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| refused_agent(format!("{e} Nothing was staged or recorded.")))
}

/// Render the `look` truth: every composition name, the open root with
/// its clips, window, and canvas.
fn look_json(session: &sdk::SdkSession) -> String {
    let comp = session.effective_composition();
    let names: Vec<String> = session
        .all_compositions()
        .iter()
        .map(|c| format!("\"{}\"", crate::studio::json_escape(&c.name)))
        .collect();
    format!(
        "{{\"tool\":\"look\",\"composition\":\"{}\",\"compositions\":[{}],\"clips\":{},\"duration\":{},\"fps\":{},\"canvas\":[{},{}]}}",
        crate::studio::json_escape(session.composition_name()),
        names.join(","),
        comp.clips.len(),
        comp.duration,
        session.fps().value(),
        comp.width,
        comp.height,
    )
}

/// Render one clip's `inspect` truth: the key, kind, window, paint
/// order, hidden switch, and source location.
fn inspect_json(comp: &notation::model::Composition, idx: usize) -> String {
    let clip = &comp.clips[idx];
    let (line, column) = clip.line_col();
    let id = match clip.id() {
        Some(id) => format!("\"{}\"", crate::studio::json_escape(id)),
        None => "null".to_string(),
    };
    format!(
        "{{\"tool\":\"inspect\",\"key\":\"{}\",\"id\":{},\"index\":{},\"kind\":\"{}\",\"start\":{},\"duration\":{},\"order\":{},\"hidden\":{},\"line\":{},\"column\":{}}}",
        crate::studio::json_escape(&clip_key(comp, idx)),
        id,
        idx,
        clip.what(),
        clip.start(),
        clip.duration(),
        clip.order(),
        clip.hidden(),
        line,
        column,
    )
}

/// Render the `frame` truth at `at`: the clips whose film window
/// covers the instant, in document order.
fn frame_json(comp: &notation::model::Composition, at: f64) -> String {
    let mut active = Vec::new();
    for (idx, clip) in comp.clips.iter().enumerate() {
        let start = clip.start();
        if at >= start && at < start + clip.duration() {
            active.push(format!(
                "\"{}\"",
                crate::studio::json_escape(&clip_key(comp, idx))
            ));
        }
    }
    format!(
        "{{\"tool\":\"frame\",\"at\":{},\"active\":[{}],\"count\":{}}}",
        at,
        active.join(","),
        comp.clips.len(),
    )
}

/// Run one `select`: every entry must resolve, so a missing selection
/// refuses naming the target. Selecting stages no composition patch
/// and records no history turn — the receipt names the picked clips,
/// and nothing applies silently.
fn run_select(comp: &notation::model::Composition, call: &AgentCall) -> Result<String, String> {
    let entries = call.args.get("targets").and_then(|t| t.as_array());
    let entries = entries.ok_or_else(|| {
        refused_agent(
            "the agent `select` names no `targets`; pass the clips to pick as `id` or `#index` entries (`{\"targets\":[\"card\",\"#1\"]}`). Nothing was staged or recorded."
                .to_string(),
        )
    })?;
    if entries.is_empty() {
        return Err(refused_agent(
            "the agent `select` names an empty `targets` list; pass at least one clip to pick. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    let mut picked = Vec::new();
    for entry in entries {
        let raw = entry.as_str().ok_or_else(|| {
            refused_agent(format!(
                "the agent `select` target {entry} is not text; pass clip ids or `#index` entries. Nothing was staged or recorded."
            ))
        })?;
        let target = parse_selection_entry(raw);
        let idx = resolve_target(comp, "select", &target)?;
        picked.push(format!(
            "\"{}\"",
            crate::studio::json_escape(&clip_key(comp, idx))
        ));
    }
    Ok(format!(
        "{{\"selected\":true,\"tool\":\"select\",\"actor\":\"{}\",\"picked\":[{}],\"count\":{}}}",
        crate::studio::json_escape(&call.actor),
        picked.join(","),
        picked.len(),
    ))
}

/// Run one `seek`: validate the instant, then echo the staged playhead
/// with its frame. Seeking stages no composition patch and records no
/// history turn — the receipt names the position, and nothing applies
/// silently.
fn run_seek(session: &sdk::SdkSession, call: &AgentCall) -> Result<String, String> {
    let at = match get_num(&call.args, "at")? {
        Some(n) => need_finite("seek", "at", n)?,
        None => match get_num(&call.args, "frame")? {
            Some(f) => {
                let f = need_finite("seek", "frame", f)?;
                if f < 0.0 {
                    return Err(refused_agent(format!(
                        "the agent `seek` `frame` value {f} is negative; pass the frame to show from 0. Nothing was staged or recorded."
                    )));
                }
                let fps = session.fps().value();
                if fps <= 0.0 {
                    f
                } else {
                    f / fps
                }
            }
            None => {
                return Err(refused_agent(
                    "the agent `seek` names no `at` seconds or `frame` index; pass the position to show. Nothing was staged or recorded."
                        .to_string(),
                ))
            }
        },
    };
    if at < 0.0 {
        return Err(refused_agent(format!(
            "the agent `seek` instant {at} is negative; pass the seconds to show from 0. Nothing was staged or recorded."
        )));
    }
    let fps = session.fps().value();
    let frame = if fps > 0.0 {
        (at * fps).round() as i64
    } else {
        0
    };
    Ok(format!(
        "{{\"staged\":true,\"tool\":\"seek\",\"actor\":\"{}\",\"at\":{},\"frame\":{},\"duration\":{}}}",
        crate::studio::json_escape(&call.actor),
        at,
        frame,
        session.effective_composition().duration,
    ))
}

/// One staged write receipt: every write tool returns this naming what
/// will apply (the one-line patch summaries in stage order) plus the
/// shared history turn the apply recorded.
#[derive(Debug, Clone)]
pub struct StagedReceipt {
    /// Tool the receipt answers.
    pub tool: String,
    /// One-line summary per staged patch, in stage order.
    pub staged: Vec<String>,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
}

/// Render one staged receipt as JSON: the tool, the staged summaries,
/// and the recorded turn.
pub fn staged_receipt_json(receipt: &StagedReceipt) -> String {
    format!(
        "{{\"staged\":true,\"tool\":\"{}\",\"turn\":{},\"actor\":\"{}\",\"edits\":[{}]}}",
        crate::studio::json_escape(&receipt.tool),
        receipt.turn,
        crate::studio::json_escape(&receipt.actor),
        receipt
            .staged
            .iter()
            .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Stage one composition write through the shared patch and history
/// machinery: every op applies to a fresh SDK session over the project
/// (validation first, so a refused op stages nothing), then one
/// validated turn records the save under the agent actor — the same
/// undo system as headless edits and every Studio panel. The receipt
/// names the staged summaries plus the recorded turn.
fn stage_write(
    folder: &Path,
    call: &AgentCall,
    ops: Vec<sdk::EditOp>,
) -> Result<StagedReceipt, String> {
    // Storyboard approval gate (Sprint 20, T004 / CAP-004, REC-002):
    // while a presented board awaits maker approval, every agent
    // composition write refuses pointing at the pending gate. Reads
    // stay open; the film stays byte-identical until approval.
    if let Some(refusal) = crate::studio_stills::gate_refusal(folder, "agent") {
        return Err(refusal);
    }
    if ops.is_empty() {
        return Err(refused_agent(format!(
            "the agent `{}` stages no edit; pass values that change the composition. Nothing was staged or recorded.",
            call.tool.as_str()
        )));
    }
    let edit = crate::studio::StudioEdit {
        actor: call.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, call.composition.as_deref(), &edit) {
        Ok(receipt) => Ok(StagedReceipt {
            tool: call.tool.as_str().to_string(),
            staged: receipt.summaries,
            turn: receipt.turn,
            actor: receipt.actor,
        }),
        Err(message) => Err(refused_agent(format!(
            "{message} Nothing was staged or recorded."
        ))),
    }
}

/// Build the `set-text` op: the target must resolve to a text clip
/// carrying a different value, so a missing clip refuses naming the
/// target, a non-text clip refuses naming its kind, and an unchanged
/// value refuses naming the clean state instead of recording an empty
/// turn.
fn set_text_ops(
    comp: &notation::model::Composition,
    call: &AgentCall,
) -> Result<Vec<sdk::EditOp>, String> {
    let target = parse_target(&call.args, "set-text")?;
    let idx = resolve_target(comp, "set-text", &target)?;
    let live = match &comp.clips[idx] {
        notation::model::Clip::Text(c) => c.value.clone(),
        other => {
            return Err(refused_agent(format!(
                "the agent `set-text` target {} is {}; `set-text` addresses text clips. Pass a text clip id or index. Nothing was staged or recorded.",
                clip_key(comp, idx),
                other.what(),
            )))
        }
    };
    let value = need_str(&call.args, "set-text", "value")?;
    if value == live {
        return Err(refused_agent(format!(
            "the agent `set-text` value on {} is already {value:?}; the clip carries no change. Nothing was staged or recorded.",
            clip_key(comp, idx),
        )));
    }
    Ok(vec![sdk::EditOp::SetRow {
        target,
        property: "value".to_string(),
        value,
    }])
}

/// Build the `set-style` op: one CSS-like hyphenated row on the
/// resolved target. Row and value rules validate at apply time, and
/// the SDK refusal names the rule with the location.
fn set_style_ops(
    comp: &notation::model::Composition,
    call: &AgentCall,
) -> Result<Vec<sdk::EditOp>, String> {
    let target = parse_target(&call.args, "set-style")?;
    resolve_target(comp, "set-style", &target)?;
    let property = need_str(&call.args, "set-style", "property")?;
    let value = match get_str(&call.args, "value")? {
        Some(v) => v,
        None => {
            return Err(refused_agent(
                "the agent `set-style` names no `value`; pass the row value to stage. Nothing was staged or recorded."
                    .to_string(),
            ))
        }
    };
    Ok(vec![sdk::EditOp::SetRow {
        target,
        property,
        value,
    }])
}

/// Build the `transform` ops: one row op per given geometric or window
/// field (`x`, `y`, `width`, `height` rows, `start`/`duration` retime),
/// in field order, so one turn reads in author order. No field refuses
/// naming the six it serves.
fn transform_ops(
    comp: &notation::model::Composition,
    call: &AgentCall,
) -> Result<Vec<sdk::EditOp>, String> {
    let target = parse_target(&call.args, "transform")?;
    resolve_target(comp, "transform", &target)?;
    let mut ops = Vec::new();
    for field in ["x", "y", "width", "height"] {
        if let Some(n) = get_num(&call.args, field)? {
            need_finite("transform", field, n)?;
            ops.push(sdk::EditOp::SetRow {
                target: target.clone(),
                property: field.to_string(),
                value: n.to_string(),
            });
        }
    }
    let start = match get_num(&call.args, "start")? {
        Some(n) => Some(need_finite("transform", "start", n)?),
        None => None,
    };
    let duration = match get_num(&call.args, "duration")? {
        Some(n) => Some(need_finite("transform", "duration", n)?),
        None => None,
    };
    if start.is_some() || duration.is_some() {
        ops.push(sdk::EditOp::Retiming {
            target: target.clone(),
            start,
            duration,
            trim_start: None,
        });
    }
    if ops.is_empty() {
        return Err(refused_agent(
            "the agent `transform` names no field; pass at least one of `x`, `y`, `width`, `height`, `start`, or `duration`. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    Ok(ops)
}

/// Build the `add-animation` op: one tween of `property` on the
/// resolved target. `kind` names `to`, `from`, or `from-to` (`to`
/// when omitted); the side a kind needs must ride along, and `ease`
/// defaults to `linear`.
fn add_animation_ops(
    comp: &notation::model::Composition,
    call: &AgentCall,
) -> Result<Vec<sdk::EditOp>, String> {
    let target = parse_target(&call.args, "add-animation")?;
    resolve_target(comp, "add-animation", &target)?;
    let property = need_str(&call.args, "add-animation", "property")?;
    let kind = get_str(&call.args, "kind")?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "to".to_string());
    if !["to", "from", "from-to"].contains(&kind.as_str()) {
        return Err(refused_agent(format!(
            "the agent `add-animation` kind {kind:?} is unknown; pass `to`, `from`, or `from-to`. Nothing was staged or recorded."
        )));
    }
    let from = get_str(&call.args, "from")?;
    let to = get_str(&call.args, "to")?;
    if kind != "to" && from.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(refused_agent(format!(
            "the agent `add-animation` kind {kind:?} names no `from`; pass the authored start value. Nothing was staged or recorded."
        )));
    }
    if kind != "from" && to.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(refused_agent(format!(
            "the agent `add-animation` kind {kind:?} names no `to`; pass the authored end value. Nothing was staged or recorded."
        )));
    }
    let start = need_finite(
        "add-animation",
        "start",
        need_num(&call.args, "add-animation", "start")?,
    )?;
    let duration = need_finite(
        "add-animation",
        "duration",
        need_num(&call.args, "add-animation", "duration")?,
    )?;
    let ease = get_str(&call.args, "ease")?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "linear".to_string());
    Ok(vec![sdk::EditOp::AddTween {
        target,
        property,
        kind,
        from,
        to,
        start,
        duration,
        ease,
    }])
}

/// Build the `update-animation` ops: the ease and/or the travel path of
/// the tween at `index` — one op per given field. Neither refuses
/// naming the two the tool serves; a missing tween refuses at apply
/// time naming the index.
fn update_animation_ops(call: &AgentCall) -> Result<Vec<sdk::EditOp>, String> {
    let index = match call.args.get("index") {
        Some(v) => v.as_u64().ok_or_else(|| {
            refused_agent(format!(
                "the agent `update-animation` `index` value {v} is not a whole tween index; pass a non-negative integer. Nothing was staged or recorded."
            ))
        })? as usize,
        None => {
            return Err(refused_agent(
                "the agent `update-animation` names no `index`; pass the tween index to update. Nothing was staged or recorded."
                    .to_string(),
            ))
        }
    };
    let mut ops = Vec::new();
    if let Some(ease) = get_str(&call.args, "ease")? {
        if ease.trim().is_empty() {
            return Err(refused_agent(
                "the agent `update-animation` `ease` is blank; pass the curve name or drop the field. Nothing was staged or recorded."
                    .to_string(),
            ));
        }
        ops.push(sdk::EditOp::SetTweenEase { index, ease });
    }
    if let Some(path) = get_str(&call.args, "path")? {
        let path_to = get_str(&call.args, "path-to")?;
        ops.push(sdk::EditOp::SetTweenPath {
            index,
            path,
            path_to,
        });
    }
    if ops.is_empty() {
        return Err(refused_agent(
            "the agent `update-animation` names neither `ease` nor `path`; pass at least one field to update. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    Ok(ops)
}

/// Build the `add-keyframe` op: one key of `property` on the resolved
/// target through at least two stops in rising time order, with the
/// key-level `ease` when given.
fn add_keyframe_ops(
    comp: &notation::model::Composition,
    call: &AgentCall,
) -> Result<Vec<sdk::EditOp>, String> {
    let target = parse_target(&call.args, "add-keyframe")?;
    resolve_target(comp, "add-keyframe", &target)?;
    let property = need_str(&call.args, "add-keyframe", "property")?;
    let raw_stops = call.args.get("stops").and_then(|s| s.as_array());
    let raw_stops = raw_stops.ok_or_else(|| {
        refused_agent(
            "the agent `add-keyframe` names no `stops`; pass at least two stops (`{\"at\":0,\"value\":\"0\"}`). Nothing was staged or recorded."
                .to_string(),
        )
    })?;
    if raw_stops.len() < 2 {
        return Err(refused_agent(format!(
            "the agent `add-keyframe` carries {} stop(s); a key needs at least two stops in rising time order. Nothing was staged or recorded.",
            raw_stops.len()
        )));
    }
    let mut stops = Vec::new();
    for (i, raw) in raw_stops.iter().enumerate() {
        let stop = raw.as_object().ok_or_else(|| {
            refused_agent(format!(
                "the agent `add-keyframe` stop {i} is not an object; pass `{{\"at\":0,\"value\":\"0\"}}`. Nothing was staged or recorded."
            ))
        })?;
        let at = stop.get("at").and_then(|a| a.as_f64()).ok_or_else(|| {
            refused_agent(format!(
                "the agent `add-keyframe` stop {i} names no numeric `at`; pass the stop instant. Nothing was staged or recorded."
            ))
        })?;
        need_finite("add-keyframe", "at", at)?;
        let value = stop
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                refused_agent(format!(
                    "the agent `add-keyframe` stop {i} names no text `value`; pass the authored value. Nothing was staged or recorded."
                ))
            })?
            .to_string();
        let ease = match stop.get("ease") {
            None => None,
            Some(e) => Some(
                e.as_str()
                    .ok_or_else(|| {
                        refused_agent(format!(
                            "the agent `add-keyframe` stop {i} `ease` value {e} is not text; pass the curve name. Nothing was staged or recorded."
                        ))
                    })?
                    .to_string(),
            ),
        };
        stops.push(sdk::KeyStopInput { at, value, ease });
    }
    let ease = get_str(&call.args, "ease")?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(vec![sdk::EditOp::AddKey {
        target,
        property,
        stops,
        ease,
    }])
}

/// Build the `delete-animation` op: remove the tween, key, or set at
/// `index`, per `kind`. A missing entry refuses at apply time naming
/// the index.
fn delete_animation_ops(call: &AgentCall) -> Result<Vec<sdk::EditOp>, String> {
    let kind = need_str(&call.args, "delete-animation", "kind")?;
    if !["tween", "key", "set"].contains(&kind.as_str()) {
        return Err(refused_agent(format!(
            "the agent `delete-animation` kind {kind:?} is unknown; pass `tween`, `key`, or `set`. Nothing was staged or recorded."
        )));
    }
    let index = match call.args.get("index") {
        Some(v) => v.as_u64().ok_or_else(|| {
            refused_agent(format!(
                "the agent `delete-animation` `index` value {v} is not a whole directive index; pass a non-negative integer. Nothing was staged or recorded."
            ))
        })? as usize,
        None => {
            return Err(refused_agent(
                "the agent `delete-animation` names no `index`; pass the tween, key, or set index to remove. Nothing was staged or recorded."
                    .to_string(),
            ))
        }
    };
    Ok(vec![match kind.as_str() {
        "tween" => sdk::EditOp::RemoveTween { index },
        "key" => sdk::EditOp::RemoveKey { index },
        _ => sdk::EditOp::RemoveSet { index },
    }])
}

/// Dispatch one parsed agent tool call against the project folder:
/// reads answer with the current truth, view writes answer with a
/// view receipt, and composition writes stage one edit each with a
/// staged receipt through the shared history. The answer renders as
/// JSON; a refusal names its cause and stages nothing.
pub fn dispatch_agent_tool(folder: &Path, call: &AgentCall) -> Result<String, String> {
    let session = open_for_read(folder, call.composition.as_deref())?;
    let comp = session.effective_composition().clone();
    match call.tool {
        AgentToolName::Look => Ok(look_json(&session)),
        AgentToolName::Inspect => {
            let target = parse_target(&call.args, "inspect")?;
            let idx = resolve_target(&comp, "inspect", &target)?;
            Ok(inspect_json(&comp, idx))
        }
        AgentToolName::Frame => {
            let at = match get_num(&call.args, "at")? {
                Some(n) => need_finite("frame", "at", n)?,
                None => 0.0,
            };
            if at < 0.0 {
                return Err(refused_agent(format!(
                    "the agent `frame` instant {at} is negative; pass the seconds to read from 0. Nothing was staged or recorded."
                )));
            }
            Ok(frame_json(&comp, at))
        }
        AgentToolName::Select => run_select(&comp, call),
        AgentToolName::Seek => run_seek(&session, call),
        AgentToolName::SetText => {
            let ops = set_text_ops(&comp, call)?;
            stage_write(folder, call, ops).map(|r| staged_receipt_json(&r))
        }
        AgentToolName::SetStyle => {
            let ops = set_style_ops(&comp, call)?;
            stage_write(folder, call, ops).map(|r| staged_receipt_json(&r))
        }
        AgentToolName::Transform => {
            let ops = transform_ops(&comp, call)?;
            stage_write(folder, call, ops).map(|r| staged_receipt_json(&r))
        }
        AgentToolName::AddAnimation => {
            let ops = add_animation_ops(&comp, call)?;
            stage_write(folder, call, ops).map(|r| staged_receipt_json(&r))
        }
        AgentToolName::UpdateAnimation => {
            let ops = update_animation_ops(call)?;
            stage_write(folder, call, ops).map(|r| staged_receipt_json(&r))
        }
        AgentToolName::AddKeyframe => {
            let ops = add_keyframe_ops(&comp, call)?;
            stage_write(folder, call, ops).map(|r| staged_receipt_json(&r))
        }
        AgentToolName::DeleteAnimation => {
            let ops = delete_animation_ops(call)?;
            stage_write(folder, call, ops).map(|r| staged_receipt_json(&r))
        }
    }
}
