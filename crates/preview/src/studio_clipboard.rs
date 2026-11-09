//! Clipboard and drop panel for the Studio shell (Sprint 18, T008 /
//! CAP-008, J-06, STATE-02, REC-001, REC-004).
//!
//! The maker copies, cuts, pastes, and duplicates clips across
//! compositions and projects, and drops files, assets, and
//! compositions onto the timeline. Copy reads the effective
//! composition (validated base with the persisted delta replayed) and
//! returns a self-contained JSON payload — full clips plus the
//! composition-level tweens, keys, and sets targeting them — so the
//! payload crosses projects and Studio servers on the system
//! clipboard with styles and motion attached. Paste lands the
//! earliest clip exactly at the playhead while every other clip
//! keeps its offset, tracks stay as copied, and every `id` renames
//! (`<id>-paste`, then `<id>-paste-2`, ...) on every paste,
//! including within one composition. Relative `start-after` links
//! clear on landing (the paste holds absolute instants) while
//! `font-family` try-order lists pass through untouched with
//! fail-closed resolution at render time, never resolved here.
//!
//! The panel proposes, the SDK disposes: paste and duplicate land
//! through one [`sdk::EditOp::PasteClips`] patch and cut removes
//! through [`sdk::EditOp::RemoveElement`] patches, every one recorded
//! by [`sdk::SdkSession::persist_turn`], so one undo system keeps
//! serving clipboard turns like every other panel edit. Copy stages
//! nothing and writes nothing; cut removes only after its copy
//! confirms, so a refused copy leaves the source exactly as it was.
//! Drops reuse their owners: asset and file drops place through the
//! Assets panel insert, composition drops place through the
//! Compositions panel placement, each at the drop point. A paste
//! that cannot fit (past the seating cap), an incompatible payload
//! (naming what is missing), or an unsupported drop (naming the
//! accepted types) refuses while source and target stay exactly as
//! they were.
//!
//! Write boundary: only validated turns write the project; copy and
//! every refusal write nothing anywhere, and no hidden clipboard
//! state file exists — the payload lives on the maker side. Queue
//! outputs still land outside the project sources exactly like CLI
//! renders; default renders stay byte-identical.
//!
//! Style boundary: request keys, response keys, and payload keys stay
//! CSS-like hyphenated end to end (`source-composition`,
//! `drop-at`, `base-start`, `used-by`); clip ids rename with a
//! hyphenated `-paste` suffix for the same reason.

use std::path::Path;

use crate::studio::json_escape;

/// Version of the clipboard payload envelope.
pub const CLIPBOARD_VERSION: u32 = 1;

/// Payload kind carried in the envelope (clipboard copies hold
/// clips; anything else refuses as incompatible).
pub const CLIPBOARD_KIND: &str = "clips";

fn refused_clipboard(message: String) -> String {
    format!("studio refused: {message}")
}

/// One clip the maker names: by stable `id` when set, else by
/// document index. Resolution prefers `id`; an `id` naming nothing
/// refuses even when an index rides along, never falling back
/// silently. At least one of the two must name a clip.
#[derive(Debug, Clone)]
pub struct ClipTarget {
    pub id: Option<String>,
    pub index: Option<usize>,
}

/// Parse one `{"id": ..., "index": ...}` target value. Both keys are
/// optional; an empty `id` reads as absent.
fn parse_target(value: &serde_json::Value) -> Result<ClipTarget, String> {
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let index = match value.get("index") {
        None => None,
        Some(v) if v.is_null() => None,
        Some(v) => Some(v.as_u64().ok_or_else(|| {
            refused_clipboard(format!(
                "the clipboard target index {v} is not a clip index; pass a non-negative integer. Nothing was staged."
            ))
        })? as usize),
    };
    if id.is_none() && index.is_none() {
        return Err(refused_clipboard(
            "the clipboard target names neither `id` nor `index`; pass the clip `id` or its document index. Nothing was staged.".to_string(),
        ));
    }
    Ok(ClipTarget { id, index })
}

/// Parse the `clips` target list of a copy, cut, or duplicate body.
/// The list must be present and non-empty.
fn parse_targets(value: &serde_json::Value) -> Result<Vec<ClipTarget>, String> {
    let list = value.get("clips").ok_or_else(|| {
        refused_clipboard(
            "the clipboard action names no `clips`; pass the clips to move (`{\"clips\":[{\"id\":\"hero\"}]}`). Nothing was staged.".to_string(),
        )
    })?;
    let items = list.as_array().ok_or_else(|| {
        refused_clipboard(
            "the clipboard `clips` is not a list; pass the clips to move (`{\"clips\":[{\"id\":\"hero\"}]}`). Nothing was staged.".to_string(),
        )
    })?;
    if items.is_empty() {
        return Err(refused_clipboard(
            "the clipboard `clips` names no clips; pass at least one clip `id` or index. Nothing was staged.".to_string(),
        ));
    }
    items.iter().map(parse_target).collect()
}

/// Resolve one target against staged clips, preferring `id`: an `id`
/// naming nothing refuses even when an index rides along.
fn resolve_index(clips: &[notation::model::Clip], target: &ClipTarget) -> Result<usize, String> {
    if let Some(id) = target.id.as_deref() {
        return clips
            .iter()
            .position(|c| c.id() == Some(id))
            .ok_or_else(|| {
                refused_clipboard(format!(
                    "unknown clip id {id:?}: the composition holds {} clip(s); pass one of their ids or indices. Nothing was staged.",
                    clips.len()
                ))
            });
    }
    if let Some(index) = target.index {
        if index < clips.len() {
            return Ok(index);
        }
        return Err(refused_clipboard(format!(
            "clip index {index} names nothing; the composition holds {} clip(s) at indices 0..{}. Nothing was staged.",
            clips.len(),
            clips.len()
        )));
    }
    Err(refused_clipboard(
        "the clipboard target names neither `id` nor `index`; pass the clip `id` or its document index. Nothing was staged.".to_string(),
    ))
}

/// Resolve the acting composition by `name` or `id`, like `nest`
/// resolution; the render root answers when none is named.
fn resolve_target<'c>(
    comps: &'c [notation::model::Composition],
    root: Option<&str>,
    want: Option<&str>,
) -> Result<(usize, &'c notation::model::Composition), String> {
    if let Some(name) = want.map(str::trim).filter(|s| !s.is_empty()) {
        return comps
            .iter()
            .enumerate()
            .find(|(_, c)| c.name == name || c.id.as_deref() == Some(name))
            .ok_or_else(|| {
                refused_clipboard(format!(
                    "unknown composition {name:?}: the project holds {}; pass one of those. Nothing was staged.",
                    comps
                        .iter()
                        .map(|c| format!("{:?}", c.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            });
    }
    let fallback = root.unwrap_or("");
    comps
        .iter()
        .enumerate()
        .find(|(_, c)| c.name == fallback)
        .ok_or_else(|| {
            refused_clipboard(
                "the project names no render root; mark one composition `root=true` or name the composition. Nothing was staged.".to_string(),
            )
        })
}

/// One self-contained clipboard payload: the copied clips with their
/// composition-level motion, ready to paste into any composition of
/// any project. Key spellings are CSS-like hyphenated.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClipboardPayload {
    /// Envelope version ([`CLIPBOARD_VERSION`]).
    pub clipboard: u32,
    /// Payload kind ([`CLIPBOARD_KIND`]).
    pub kind: String,
    /// Composition the clips were copied from.
    pub source_composition: String,
    /// Earliest copied start; paste anchors this instant at the
    /// playhead (`base-start`).
    pub base_start: f64,
    /// Copied clips in timeline order.
    pub clips: Vec<notation::model::Clip>,
    /// Tweens targeting the copied clips, by position in `clips`.
    #[serde(default)]
    pub tweens: Vec<sdk::PastedTween>,
    /// Keys targeting the copied clips, by position in `clips`.
    #[serde(default)]
    pub keys: Vec<sdk::PastedKey>,
    /// Sets targeting the copied clips, by position in `clips`.
    #[serde(default)]
    pub sets: Vec<sdk::PastedSet>,
}

/// Render one payload as JSON for the clipboard API and the system
/// clipboard. Copied clips come from validated compositions, so every
/// number stays finite and encoding cannot fail; a fallback envelope
/// stands in only if it ever could.
pub fn clipboard_json(payload: &ClipboardPayload) -> String {
    serde_json::to_string_pretty(payload)
        .unwrap_or_else(|_| "{\"clipboard\":1,\"kind\":\"clips\"}".to_string())
}

/// Parse one pasted clipboard body value: the envelope must carry
/// the version, the clips kind, and at least one clip. Anything else
/// is an incompatible payload refusing by name while staging
/// nothing.
pub fn parse_clipboard_value(value: &serde_json::Value) -> Result<ClipboardPayload, String> {
    let version = value.get("clipboard").and_then(|v| v.as_u64());
    if version != Some(CLIPBOARD_VERSION as u64) {
        return Err(refused_clipboard(format!(
            "the clipboard payload carries no clips envelope version {found:?}; pass a version-{} clips payload from the copy action. Nothing was staged.",
            CLIPBOARD_VERSION,
            found = version
                .map(|v| v.to_string())
                .unwrap_or_else(|| "missing".to_string())
        )));
    }
    let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if kind != CLIPBOARD_KIND {
        return Err(refused_clipboard(format!(
            "the clipboard payload is a {kind:?} payload, not clips; copy clips before pasting. Nothing was staged."
        )));
    }
    let raw_clips = value.get("clips").ok_or_else(|| {
        refused_clipboard(
            "the clipboard payload carries no `clips`; copy clips before pasting. Nothing was staged.".to_string(),
        )
    })?;
    if raw_clips.as_array().is_none_or(|items| items.is_empty()) {
        return Err(refused_clipboard(
            "the clipboard payload carries no clips; copy at least one clip before pasting. Nothing was staged.".to_string(),
        ));
    }
    let clips: Vec<notation::model::Clip> =
        serde_json::from_value(raw_clips.clone()).map_err(|e| {
            refused_clipboard(format!(
                "the clipboard payload carries unreadable clips ({e}); copy the clips again. Nothing was staged."
            ))
        })?;
    let tweens = match value.get("tweens") {
        None => Vec::new(),
        Some(v) if v.is_null() => Vec::new(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
            refused_clipboard(format!(
                "the clipboard payload carries unreadable tweens ({e}); copy the clips again. Nothing was staged."
            ))
        })?,
    };
    let keys = match value.get("keys") {
        None => Vec::new(),
        Some(v) if v.is_null() => Vec::new(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
            refused_clipboard(format!(
                "the clipboard payload carries unreadable keys ({e}); copy the clips again. Nothing was staged."
            ))
        })?,
    };
    let sets = match value.get("sets") {
        None => Vec::new(),
        Some(v) if v.is_null() => Vec::new(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| {
            refused_clipboard(format!(
                "the clipboard payload carries unreadable sets ({e}); copy the clips again. Nothing was staged."
            ))
        })?,
    };
    let source_composition = value
        .get("source-composition")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("composition")
        .to_string();
    let base_start = value
        .get("base-start")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    Ok(ClipboardPayload {
        clipboard: CLIPBOARD_VERSION,
        kind: CLIPBOARD_KIND.to_string(),
        source_composition,
        base_start,
        clips,
        tweens,
        keys,
        sets,
    })
}

/// Parse one pasted clipboard request body.
pub fn parse_clipboard_body(body: &str) -> Result<ClipboardPayload, String> {
    if body.trim().is_empty() {
        return Err(refused_clipboard(
            "the paste needs a JSON clipboard body (the payload the copy action returned); pass that payload. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_clipboard(format!(
            "the clipboard payload is not JSON ({e}); pass the payload the copy action returned. Nothing was staged."
        ))
    })?;
    parse_clipboard_value(&value)
}

/// Copy clips out of one composition: resolves every target against
/// the effective composition (validated base with the persisted
/// delta replayed), clones the clips in timeline order, and carries
/// the composition-level tweens, keys, and sets targeting them. Copy
/// stages nothing and writes nothing anywhere.
pub fn copy_clips(
    folder: &Path,
    composition: Option<&str>,
    targets: &[ClipTarget],
) -> Result<ClipboardPayload, String> {
    if targets.is_empty() {
        return Err(refused_clipboard(
            "the copy names no clips; pass at least one clip `id` or index. Nothing was staged.".to_string(),
        ));
    }
    let verdict = validate::validate::validate_folder(folder);
    if verdict.status != validate::validate::Status::Valid {
        return Err(refused_clipboard(format!(
            "the project {} cannot copy clips; fix the problems `check` reports and retry. Nothing was staged.",
            folder.display()
        )));
    }
    let (_, source) = resolve_target(
        &verdict.compositions,
        verdict.root_name.as_deref(),
        composition,
    )?;
    let source_name = source.name.clone();
    // The probe opens read-only with no history, so copying never
    // writes anywhere; the effective composition carries replayed
    // turns, so copied clips match what the maker sees.
    let probe = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            composition: Some(source_name.clone()),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused_clipboard(format!(
            "the composition {source_name:?} cannot open for the copy: {e} Nothing was staged."
        ))
    })?;
    let effective = probe.effective_composition().clone();
    drop(probe);
    let mut indices: Vec<usize> = Vec::with_capacity(targets.len());
    for target in targets {
        let idx = resolve_index(&effective.clips, target)?;
        if !indices.contains(&idx) {
            indices.push(idx);
        }
    }
    indices.sort_unstable();
    let clips: Vec<notation::model::Clip> =
        indices.iter().map(|&i| effective.clips[i].clone()).collect();
    let base_start = clips
        .iter()
        .map(notation::model::Clip::start)
        .fold(f64::INFINITY, f64::min);
    let position_of = |doc: usize| indices.iter().position(|&i| i == doc);
    let tweens: Vec<sdk::PastedTween> = effective
        .tweens
        .iter()
        .filter_map(|tween| {
            position_of(tween.target).map(|clip| sdk::PastedTween {
                clip,
                tween: tween.clone(),
            })
        })
        .collect();
    let keys: Vec<sdk::PastedKey> = effective
        .keys
        .iter()
        .filter_map(|key| {
            position_of(key.target).map(|clip| sdk::PastedKey {
                clip,
                key: key.clone(),
            })
        })
        .collect();
    let sets: Vec<sdk::PastedSet> = effective
        .sets
        .iter()
        .filter_map(|set| {
            position_of(set.target).map(|clip| sdk::PastedSet {
                clip,
                set: set.clone(),
            })
        })
        .collect();
    Ok(ClipboardPayload {
        clipboard: CLIPBOARD_VERSION,
        kind: CLIPBOARD_KIND.to_string(),
        source_composition: source_name,
        base_start,
        clips,
        tweens,
        keys,
        sets,
    })
}

/// Every project-relative file one pasted clip needs in the target
/// project: media `path` sources, grade LUT cubes, and VFX
/// displacement and matte maps. Nest targets resolve against
/// composition names instead and are checked separately.
fn pasted_clip_files(clip: &notation::model::Clip) -> Vec<String> {
    let mut files = Vec::new();
    match clip {
        notation::model::Clip::Video(c) => files.push(c.path.clone()),
        notation::model::Clip::Audio(c) => files.push(c.path.clone()),
        notation::model::Clip::Image(c) => files.push(c.path.clone()),
        notation::model::Clip::Svg(c) => files.push(c.path.clone()),
        _ => {}
    }
    if let Some(src) = clip.grade().lut.src.as_deref() {
        files.push(src.to_string());
    }
    let vfx = clip.vfx();
    if let Some(map) = vfx.displacement.as_ref().and_then(|d| d.map.as_deref()) {
        files.push(map.to_string());
    }
    if let Some(src) = vfx.luma_matte.as_ref().map(|m| m.src.as_str()) {
        files.push(src.to_string());
    }
    files
}

/// Receipt of one applied paste: the recorded history turn behind
/// the landed clips.
#[derive(Debug, Clone)]
pub struct PasteReceipt {
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Composition the clips landed in.
    pub composition: String,
    /// Clips the paste landed.
    pub pasted: usize,
    /// Film instant the earliest clip opens at.
    pub at: f64,
    /// Renamed ids of the landed clips, in landing order (id-less
    /// clips land id-less and report nothing here).
    pub renamed: Vec<String>,
    /// Carried motion counts (tweens, keys, sets).
    pub motion: [usize; 3],
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Render one paste receipt as JSON for the clipboard API.
pub fn paste_receipt_json(receipt: &PasteReceipt) -> String {
    format!(
        "{{\"pasted\":true,\"composition\":\"{}\",\"pasted-clips\":{},\"playhead\":{},\"renamed\":[{}],\"carried-motion\":{{\"tweens\":{},\"keys\":{},\"sets\":{}}},\"turn\":{},\"summaries\":[{}]}}",
        json_escape(&receipt.composition),
        receipt.pasted,
        receipt.at,
        receipt
            .renamed
            .iter()
            .map(|id| format!("\"{}\"", json_escape(id)))
            .collect::<Vec<_>>()
            .join(","),
        receipt.motion[0],
        receipt.motion[1],
        receipt.motion[2],
        receipt.turn,
        receipt
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Paste one clipboard payload at the playhead: the earliest clip
/// opens exactly at `at` while every other clip keeps its offset,
/// tracks stay as copied, and every `id` renames on every paste. The
/// seating check reads the effective timeline (validated base with
/// the persisted delta replayed), so landed turns count toward full
/// before anything new stages. An incompatible payload (naming what
/// is missing) or a paste that cannot fit refuses while source and
/// target stay exactly as they were.
pub fn paste_clips(
    folder: &Path,
    composition: Option<&str>,
    payload: &ClipboardPayload,
    at: f64,
    actor: &str,
    max_clips: Option<usize>,
) -> Result<PasteReceipt, String> {
    let actor = actor.trim().to_string();
    if actor.is_empty() {
        return Err(refused_clipboard(
            "the paste names no actor; pastes record named turns. Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    if !at.is_finite() || at < 0.0 {
        return Err(refused_clipboard(format!(
            "the paste playhead {at:?} is not a film instant at or after 0; pass the playhead in seconds. Nothing was staged."
        )));
    }
    if payload.clips.is_empty() {
        return Err(refused_clipboard(
            "the clipboard payload carries no clips; copy at least one clip before pasting. Nothing was staged.".to_string(),
        ));
    }
    let verdict = validate::validate::validate_folder(folder);
    if verdict.status != validate::validate::Status::Valid {
        return Err(refused_clipboard(format!(
            "the project {} cannot take the paste; fix the problems `check` reports and retry. Nothing was staged.",
            folder.display()
        )));
    }
    let (_, target) = resolve_target(
        &verdict.compositions,
        verdict.root_name.as_deref(),
        composition,
    )?;
    let target_name = target.name.clone();
    // Nest clips name compositions, not files: every pasted nest
    // must resolve in the target project, else the paste would stage
    // a turn `check` rejects.
    for clip in &payload.clips {
        if let notation::model::Clip::Nest(nest) = clip {
            let known = verdict
                .compositions
                .iter()
                .any(|c| c.name == nest.target || c.id.as_deref() == Some(nest.target.as_str()));
            if !known {
                return Err(refused_clipboard(format!(
                    "the paste needs composition {:?}, which the target project does not hold (it holds {}); copy that composition first or paste without the nest clip. Nothing was staged.",
                    nest.target,
                    verdict
                        .compositions
                        .iter()
                        .map(|c| format!("{:?}", c.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
        for file in pasted_clip_files(clip) {
            let rel = file.trim();
            if rel.is_empty() {
                continue;
            }
            if !folder.join(rel).is_file() || !folder.join(rel).starts_with(folder) {
                return Err(refused_clipboard(format!(
                    "the paste needs file {rel:?}, which is missing from the target project; copy the media first, then paste. Nothing was staged."
                )));
            }
        }
    }
    let probe = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            composition: Some(target_name.clone()),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused_clipboard(format!(
            "the timeline {target_name:?} cannot open for the paste: {e} Nothing was staged."
        ))
    })?;
    let seated = probe.effective_composition().clips.len();
    drop(probe);
    let cap = max_clips.unwrap_or(crate::studio_compositions::MAX_TIMELINE_CLIPS);
    if seated + payload.clips.len() > cap {
        return Err(refused_clipboard(format!(
            "the {target_name:?} timeline cannot fit the paste: it holds {seated} clip(s) and the paste carries {} at the {cap}-clip seating cap; remove a clip and retry. Nothing was staged.",
            payload.clips.len()
        )));
    }
    let mut session = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            composition: Some(target_name.clone()),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused_clipboard(format!(
            "the timeline {target_name:?} cannot open for the paste: {e} Nothing was staged."
        ))
    })?;
    let before = session.effective_composition().clips.len();
    let op = sdk::EditOp::PasteClips {
        composition: target_name.clone(),
        clips: payload.clips.clone(),
        tweens: payload.tweens.clone(),
        keys: payload.keys.clone(),
        sets: payload.sets.clone(),
        at,
    };
    let applied = session.apply(op).map_err(|e| {
        refused_clipboard(format!("{e} Nothing was persisted or recorded."))
    })?;
    let renamed: Vec<String> = session.effective_composition().clips[before..]
        .iter()
        .filter_map(|c| c.id().map(str::to_string))
        .collect();
    let summaries = vec![applied.op.summary()];
    let motion = [
        payload.tweens.len(),
        payload.keys.len(),
        payload.sets.len(),
    ];
    let pasted = payload.clips.len();
    match session.persist_turn(&actor) {
        Ok(turn) => Ok(PasteReceipt {
            turn: turn.turn.unwrap_or(0),
            composition: target_name,
            pasted,
            at,
            renamed,
            motion,
            summaries,
        }),
        Err(e) => Err(refused_clipboard(format!(
            "{e} Staged work stays preserved unapplied — nothing was persisted or recorded."
        ))),
    }
}

/// Receipt of one applied cut: the confirmed copy payload plus the
/// recorded removal turn.
#[derive(Debug, Clone)]
pub struct CutReceipt {
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Composition the clips were cut from.
    pub composition: String,
    /// Clips removed.
    pub removed: usize,
    /// The confirmed copy payload (what the cut carried away).
    pub payload: ClipboardPayload,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Render one cut receipt as JSON for the clipboard API: the removal
/// turn with the confirmed payload attached, so the maker can paste
/// it elsewhere next.
pub fn cut_receipt_json(receipt: &CutReceipt) -> String {
    format!(
        "{{\"cut\":true,\"composition\":\"{}\",\"removed\":{},\"turn\":{},\"summaries\":[{}],\"clipboard\":{}}}",
        json_escape(&receipt.composition),
        receipt.removed,
        receipt.turn,
        receipt
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(","),
        clipboard_json(&receipt.payload)
    )
}

/// Cut clips out of one composition: the copy must confirm first —
/// a refused copy removes nothing — then every copied clip removes
/// in one named history turn. The receipt carries the confirmed
/// payload, so the cut both moves and stages the paste.
pub fn cut_clips(
    folder: &Path,
    composition: Option<&str>,
    targets: &[ClipTarget],
    actor: &str,
) -> Result<CutReceipt, String> {
    let actor = actor.trim().to_string();
    if actor.is_empty() {
        return Err(refused_clipboard(
            "the cut names no actor; cuts record named turns. Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    // The confirmed copy lands first: only its resolved clips
    // remove, so a copy the project cannot take leaves the source
    // exactly as it was.
    let payload = copy_clips(folder, composition, targets)?;
    let composition_name = payload.source_composition.clone();
    let mut session = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            composition: Some(composition_name.clone()),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused_clipboard(format!(
            "the composition {composition_name:?} cannot open for the cut: {e} Nothing was removed or recorded."
        ))
    })?;
    // Re-resolve against the writable session: the copy confirmed
    // these targets, and anything that moved under the cut refuses
    // with the timeline untouched.
    let mut refs = Vec::with_capacity(targets.len());
    for target in targets {
        let staged = session.effective_composition().clips.clone();
        let idx = resolve_index(&staged, target).map_err(|_| {
            refused_clipboard(
                "the cut copy no longer resolves; the timeline moved under this cut. Nothing was removed or recorded.".to_string(),
            )
        })?;
        let id = staged[idx].id().map(str::to_string);
        let one = sdk::ClipRef { id, index: Some(idx) };
        if !refs.iter().any(|r: &sdk::ClipRef| r == &one) {
            refs.push(one);
        }
    }
    let removed = refs.len();
    let op = sdk::EditOp::RemoveElements {
        composition: composition_name.clone(),
        targets: refs,
    };
    let applied = session.apply(op).map_err(|e| {
        refused_clipboard(format!("{e} Nothing was persisted or recorded."))
    })?;
    let summaries = vec![applied.op.summary()];
    match session.persist_turn(&actor) {
        Ok(turn) => Ok(CutReceipt {
            turn: turn.turn.unwrap_or(0),
            composition: composition_name,
            removed,
            payload,
            summaries,
        }),
        Err(e) => Err(refused_clipboard(format!(
            "{e} Staged work stays preserved unapplied — nothing was persisted or recorded."
        ))),
    }
}

/// Receipt of one applied duplicate: the recorded history turn
/// behind the in-place copies.
#[derive(Debug, Clone)]
pub struct DuplicateReceipt {
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Composition the clips duplicated in.
    pub composition: String,
    /// Clips duplicated.
    pub duplicated: usize,
    /// Renamed ids of the duplicates, in landing order.
    pub renamed: Vec<String>,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Render one duplicate receipt as JSON for the clipboard API.
pub fn duplicate_receipt_json(receipt: &DuplicateReceipt) -> String {
    format!(
        "{{\"duplicated\":true,\"composition\":\"{}\",\"duplicated-clips\":{},\"renamed\":[{}],\"turn\":{},\"summaries\":[{}]}}",
        json_escape(&receipt.composition),
        receipt.duplicated,
        receipt
            .renamed
            .iter()
            .map(|id| format!("\"{}\"", json_escape(id)))
            .collect::<Vec<_>>()
            .join(","),
        receipt.turn,
        receipt
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Duplicate clips in place: copies the targets, then pastes them
/// back at their own earliest start, so offsets, tracks, styles, and
/// motion stay while every `id` renames. One paste is one undo unit.
pub fn duplicate_clips(
    folder: &Path,
    composition: Option<&str>,
    targets: &[ClipTarget],
    actor: &str,
) -> Result<DuplicateReceipt, String> {
    let payload = copy_clips(folder, composition, targets)?;
    let at = payload.base_start;
    let receipt = paste_clips(
        folder,
        Some(&payload.source_composition),
        &payload,
        at,
        actor,
        None,
    )
    .map_err(|e| {
        e.replace(
            "the paste ",
            "the duplicate ",
        )
    })?;
    Ok(DuplicateReceipt {
        turn: receipt.turn,
        composition: receipt.composition,
        duplicated: receipt.pasted,
        renamed: receipt.renamed,
        summaries: receipt.summaries,
    })
}

/// One parsed clipboard selection body (copy, cut, duplicate):
/// the acting composition with the clips to move and the actor
/// recording mutating turns. Key spellings are CSS-like hyphenated.
#[derive(Debug, Clone)]
pub struct SelectionRequest {
    /// Acting composition, by `name` or `id`; the render root when
    /// omitted.
    pub composition: Option<String>,
    /// Actor the turn records (cut and duplicate only; copy ignores
    /// it and records nothing).
    pub actor: String,
    /// Clips to move.
    pub targets: Vec<ClipTarget>,
}

/// Parse one selection body. `clips` is required; `actor` is required
/// unless `actor_optional` (the copy action records no turn).
pub fn parse_selection_body(body: &str, actor_optional: bool) -> Result<SelectionRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_clipboard(
            "the clipboard action needs a JSON body (`{\"clips\":[{\"id\":\"hero\"}]}`); pass the clips to move. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_clipboard(format!(
            "the clipboard body is not JSON ({e}). Pass the clips to move (`{{\"clips\":[{{\"id\":\"hero\"}}]}}`). Nothing was staged."
        ))
    })?;
    let composition = value
        .get("composition")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let actor = value
        .get("actor")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if !actor_optional && actor.is_empty() {
        return Err(refused_clipboard(
            "the clipboard action names no actor; mutating turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let targets = parse_targets(&value)?;
    Ok(SelectionRequest {
        composition,
        actor,
        targets,
    })
}

/// One parsed paste body: the acting composition, the playhead the
/// earliest clip opens at, the recording actor, and the clipboard
/// payload the copy action returned.
#[derive(Debug, Clone)]
pub struct PasteRequest {
    /// Acting composition, by `name` or `id`; the render root when
    /// omitted.
    pub composition: Option<String>,
    /// Film instant the earliest clip opens at.
    pub playhead: f64,
    /// Actor the validated turn records.
    pub actor: String,
    /// Clipboard payload to land.
    pub payload: ClipboardPayload,
}

/// Parse one paste body. Every key is required except the acting
/// composition, which falls back to the render root.
pub fn parse_paste_body(body: &str) -> Result<PasteRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_clipboard(
            "the paste needs a JSON body (`{\"playhead\":2,\"actor\":\"maker\",\"clipboard\":{...}}`); pass the playhead with the copied payload. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_clipboard(format!(
            "the paste body is not JSON ({e}). Pass the playhead with the copied payload. Nothing was staged."
        ))
    })?;
    let composition = value
        .get("composition")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let playhead = value.get("playhead").and_then(|v| v.as_f64()).ok_or_else(|| {
        refused_clipboard(
            "the paste names no `playhead`; pass the film instant the earliest clip opens at in seconds. Nothing was staged.".to_string(),
        )
    })?;
    let actor = value
        .get("actor")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if actor.is_empty() {
        return Err(refused_clipboard(
            "the paste names no actor; pastes record named turns. Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let raw = value.get("clipboard").ok_or_else(|| {
        refused_clipboard(
            "the paste carries no `clipboard`; pass the payload the copy action returned. Nothing was staged.".to_string(),
        )
    })?;
    let payload = parse_clipboard_value(raw)?;
    Ok(PasteRequest {
        composition,
        playhead,
        actor,
        payload,
    })
}

/// What a drop carries onto the timeline: a project file, a project
/// asset, or a project composition.
#[derive(Debug, Clone)]
pub enum DropSource {
    /// Project-relative file path (`media/card.png`).
    File(String),
    /// Project-relative asset name (`media/card.png`).
    Asset(String),
    /// Composition to nest, by `name` or `id`.
    Composition(String),
}

/// One parsed drop body: the acting composition, the drop point, the
/// recording actor, and what the drop carries.
#[derive(Debug, Clone)]
pub struct DropRequest {
    /// Acting composition, by `name` or `id`; the render root when
    /// omitted.
    pub composition: Option<String>,
    /// Film instant the drop lands at; the timeline end when omitted.
    pub drop_at: Option<f64>,
    /// Actor the validated turn records.
    pub actor: String,
    /// What the drop carries.
    pub source: DropSource,
}

/// Parse one drop body. `kind` names `file`, `asset`, or
/// `composition`; each kind reads its own key (`path`, `asset`, or
/// `source`). Anything else is an unsupported drop refusing by name.
pub fn parse_drop_body(body: &str) -> Result<DropRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_clipboard(
            "the drop needs a JSON body (`{\"kind\":\"asset\",\"asset\":\"media/card.png\"}`); pass what the drop carries. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_clipboard(format!(
            "the drop body is not JSON ({e}). Pass what the drop carries (`{{\"kind\":\"asset\",\"asset\":\"media/card.png\"}}`). Nothing was staged."
        ))
    })?;
    let kind = value
        .get("kind")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    let named = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let source = match kind {
        "file" => DropSource::File(named("path").ok_or_else(|| {
            refused_clipboard(
                "the file drop names no `path`; pass the project-relative file (`{\"kind\":\"file\",\"path\":\"media/card.png\"}`). Nothing was staged.".to_string(),
            )
        })?),
        "asset" => DropSource::Asset(named("asset").ok_or_else(|| {
            refused_clipboard(
                "the asset drop names no `asset`; pass the project-relative asset (`{\"kind\":\"asset\",\"asset\":\"media/card.png\"}`). Nothing was staged.".to_string(),
            )
        })?),
        "composition" => DropSource::Composition(named("source").ok_or_else(|| {
            refused_clipboard(
                "the composition drop names no `source`; pass the composition name or id (`{\"kind\":\"composition\",\"source\":\"b\"}`). Nothing was staged.".to_string(),
            )
        })?),
        _ => {
            return Err(refused_clipboard(format!(
                "unsupported drop {kind:?}; drop a file, an asset, or a composition (`{{\"kind\":\"file|asset|composition\"}}`). Nothing was staged."
            )));
        }
    };
    let composition = named("composition");
    let drop_at = match value.get("drop-at") {
        None => None,
        Some(v) if v.is_null() => None,
        Some(v) => Some(v.as_f64().ok_or_else(|| {
            refused_clipboard(format!(
                "the drop point {v} is not a number of seconds; pass the drop instant (`\"drop-at\":2.5`). Nothing was staged."
            ))
        })?),
    };
    if let Some(at) = drop_at {
        if !at.is_finite() || at < 0.0 {
            return Err(refused_clipboard(format!(
                "the drop point {at:?} is not a film instant at or after 0; pass the drop instant in seconds. Nothing was staged."
            )));
        }
    }
    let actor = named("actor").unwrap_or_default();
    if actor.is_empty() {
        return Err(refused_clipboard(
            "the drop names no actor; drops record named turns. Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    Ok(DropRequest {
        composition,
        drop_at,
        actor,
        source,
    })
}

/// Receipt of one applied drop: the recorded history turn behind the
/// placed media.
#[derive(Debug, Clone)]
pub struct DropReceipt {
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Timeline the drop landed on.
    pub composition: String,
    /// Drop kind (`file`, `asset`, or `composition`).
    pub kind: String,
    /// What the drop carried (stored name or composition name).
    pub landed: String,
    /// Film instant the drop opens at.
    pub at: f64,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Render one drop receipt as JSON for the clipboard API.
pub fn drop_receipt_json(receipt: &DropReceipt) -> String {
    format!(
        "{{\"dropped\":true,\"kind\":\"{}\",\"composition\":\"{}\",\"landed\":\"{}\",\"drop-at\":{},\"turn\":{},\"summaries\":[{}]}}",
        json_escape(&receipt.kind),
        json_escape(&receipt.composition),
        json_escape(&receipt.landed),
        receipt.at,
        receipt.turn,
        receipt
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Drop a file, asset, or composition onto the timeline at the drop
/// point. Asset and file drops place through the Assets panel
/// insert; composition drops place through the Compositions panel
/// placement — so drops share their seating caps, cycle guards, and
/// history turns instead of inventing new ones. A file outside the
/// project refuses naming the Assets import; an unsupported type
/// refuses naming the accepted types; every refusal stages nothing.
pub fn drop_onto_timeline(folder: &Path, req: &DropRequest) -> Result<DropReceipt, String> {
    let verdict = validate::validate::validate_folder(folder);
    if verdict.status != validate::validate::Status::Valid {
        return Err(refused_clipboard(format!(
            "the project {} cannot take the drop; fix the problems `check` reports and retry. Nothing was staged.",
            folder.display()
        )));
    }
    let (_, target) = resolve_target(
        &verdict.compositions,
        verdict.root_name.as_deref(),
        req.composition.as_deref(),
    )?;
    let target_name = target.name.clone();
    let at = match req.drop_at {
        Some(at) => at,
        None => {
            let probe = sdk::SdkSession::open(
                folder,
                sdk::OpenOptions {
                    persist: false,
                    history: false,
                    composition: Some(target_name.clone()),
                    ..sdk::OpenOptions::default()
                },
            )
            .map_err(|e| {
                refused_clipboard(format!(
                    "the timeline {target_name:?} cannot open for the drop: {e} Nothing was staged."
                ))
            })?;
            let end = probe
                .effective_composition()
                .clips
                .iter()
                .map(|c| c.start() + c.duration())
                .fold(0.0f64, f64::max);
            drop(probe);
            end
        }
    };
    match &req.source {
        DropSource::File(path) | DropSource::Asset(path) => {
            let rel = path.trim();
            // A file outside the project is an import, not a drop:
            // the Assets panel owns that copy with its stored-name
            // receipt, so the drop refuses and names it.
            if std::path::Path::new(rel).is_absolute()
                || !folder.join(rel).starts_with(folder)
            {
                return Err(refused_clipboard(format!(
                    "the drop file {rel:?} sits outside the project; import it through the Assets panel first, then drop the stored asset. Nothing was staged."
                )));
            }
            let action = crate::studio_assets::AssetActionRequest {
                action: "insert".to_string(),
                asset: rel.to_string(),
                name: None,
                start: Some(at),
                duration: None,
                actor: req.actor.clone(),
                composition: Some(target_name.clone()),
            };
            let comps: Vec<notation::model::Composition> = verdict.compositions.clone();
            let receipt = crate::studio_assets::insert_asset(folder, &comps, &action).map_err(|e| {
                refused_clipboard(format!("{e} Nothing else was staged."))
            })?;
            // The insert receipt is JSON (`{"inserted":true,...}`);
            // the drop receipt re-reports it under drop keys.
            let turn = receipt_turn(&receipt);
            let summaries = receipt_summaries(&receipt);
            Ok(DropReceipt {
                turn,
                composition: target_name,
                kind: if matches!(req.source, DropSource::File(_)) {
                    "file".to_string()
                } else {
                    "asset".to_string()
                },
                landed: rel.to_string(),
                at,
                summaries,
            })
        }
        DropSource::Composition(source) => {
            let place = crate::studio_compositions::PlaceRequest {
                source: source.clone(),
                target: Some(target_name.clone()),
                at: Some(at),
                actor: req.actor.clone(),
                expected_generation: None,
                max_clips: None,
            };
            match crate::studio_compositions::place_composition(folder, &place) {
                Ok(placed) => Ok(DropReceipt {
                    turn: placed.turn,
                    composition: placed.target,
                    kind: "composition".to_string(),
                    landed: placed.source,
                    at: placed.at,
                    summaries: placed.summaries,
                }),
                Err(e) => Err(refused_clipboard(format!("{e} Nothing else was staged."))),
            }
        }
    }
}

/// Read the `turn` number out of an asset insert receipt (`0` when
/// the receipt carries none; inserts always carry one).
fn receipt_turn(receipt: &str) -> u64 {
    serde_json::from_str::<serde_json::Value>(receipt)
        .ok()
        .and_then(|v| v.get("turn").and_then(|t| t.as_u64()))
        .unwrap_or(0)
}

/// Read the `summaries` out of an asset insert receipt.
fn receipt_summaries(receipt: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(receipt)
        .ok()
        .and_then(|v| v.get("summaries").cloned())
        .and_then(|v| {
            v.as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}
