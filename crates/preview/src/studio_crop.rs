//! Crop mode for the Studio canvas (Sprint 17, T004 / CAP-004, J-02,
//! STATE-02, INT-001, INT-003, REC-001..003).
//!
//! The maker picks one preset — Free, 16:9, 9:16, 1:1, or 4:5 — and the
//! crop frame constrains to that aspect with live preview; committing
//! writes one patch turn and cancelling leaves nothing staged. A
//! preset that cannot fit the source clamps and shows the clamped
//! frame. Crop never resamples media: it only rewrites the visible
//! window (`x`, `y`, `width`, `height` rows) on sized clips (`box`,
//! `image`, `svg`, `flex`), leaving media files and every other row
//! untouched.
//!
//! Every crop edit applies through the Sprint 15 patch and history
//! machinery ([`crate::studio::apply_studio_edit`]), so crop shares
//! the one undo system with canvas gestures, row edits, timeline
//! verbs, and headless edits. Previews and cancels stage nothing: no
//! composition write, no delta write, no history turn. Style
//! attributes stay CSS-like hyphenated end to end, and the
//! `font-family` try-order lists pass through untouched — unknown
//! families stay fail-closed at render time, never resolved or
//! substituted here.

use std::path::Path;

/// Crop presets in served order: free plus the four fixed aspects.
pub const CROP_PRESETS: [&str; 5] = ["free", "16:9", "9:16", "1:1", "4:5"];

/// The crop frame floor in film pixels: a frame past it clamps and
/// names the bound (REC-003), mirroring the canvas resize floor.
pub const MIN_CROP_PX: f64 = 1.0;

/// Prefix every crop refusal carries; nothing stages or applies on a
/// refusal.
fn refused_crop(message: String) -> String {
    format!("crop refused: {message}")
}

/// Render one number the way rows show it: whole values read as
/// integers, anything else as written.
fn num_text(x: f64) -> String {
    if x.is_finite() && x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// One crop preset: free, or a fixed width-over-height aspect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CropPreset {
    /// No aspect constraint: the frame follows the drag exactly.
    Free,
    /// Fixed aspect (width divided by height).
    Fixed(f64),
}

impl CropPreset {
    /// Parse one preset spelling; anything else refuses naming the
    /// five served presets.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "free" => Some(CropPreset::Free),
            "16:9" => Some(CropPreset::Fixed(16.0 / 9.0)),
            "9:16" => Some(CropPreset::Fixed(9.0 / 16.0)),
            "1:1" => Some(CropPreset::Fixed(1.0)),
            "4:5" => Some(CropPreset::Fixed(4.0 / 5.0)),
            _ => None,
        }
    }

    /// The served spelling of one preset.
    pub fn as_str(self) -> &'static str {
        match self {
            CropPreset::Free => "free",
            CropPreset::Fixed(a) if (a - 16.0 / 9.0).abs() < 1e-9 => "16:9",
            CropPreset::Fixed(a) if (a - 9.0 / 16.0).abs() < 1e-9 => "9:16",
            CropPreset::Fixed(a) if (a - 1.0).abs() < 1e-9 => "1:1",
            _ => "4:5",
        }
    }

    /// The width-over-height aspect, when fixed.
    pub fn aspect(self) -> Option<f64> {
        match self {
            CropPreset::Free => None,
            CropPreset::Fixed(a) => Some(a),
        }
    }
}

/// One requested crop frame in film pixels, before the preset
/// constrains and the source clamps it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropFrame {
    /// Left edge in film pixels.
    pub x: f64,
    /// Top edge in film pixels.
    pub y: f64,
    /// Frame width in film pixels.
    pub width: f64,
    /// Frame height in film pixels.
    pub height: f64,
}

/// One crop request from the canvas crop bar.
#[derive(Debug, Clone)]
pub struct CropRequest {
    /// Clip the crop frame acts on: crop addresses one clip.
    pub target: sdk::ClipRef,
    /// Preset spelling (`free`, `16:9`, `9:16`, `1:1`, `4:5`).
    pub preset: String,
    /// Requested frame, when the maker dragged one; `None` crops to
    /// the whole source rect (the largest centered preset frame).
    pub frame: Option<CropFrame>,
    /// True while committing the staged frame as one patch turn.
    pub commit: bool,
    /// True while discarding the staged frame with nothing recorded.
    pub cancelled: bool,
    /// Actor the committed turn is recorded under.
    pub actor: String,
    /// Canvas selection echoing through the answer (INT-003).
    pub selection: Vec<String>,
    /// Composition override, when the body names one.
    pub composition: Option<String>,
}

/// Read one finite film-pixel number under a key.
fn crop_number(value: &serde_json::Value, key: &str) -> Result<f64, String> {
    let n = value.as_f64().ok_or_else(|| {
        refused_crop(format!(
            "the crop `{key}` value {value} is not a number; pass finite film pixels. Nothing was staged."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_crop(format!(
            "the crop `{key}` value {n:?} is not finite; pass finite film pixels. Nothing was staged."
        )));
    }
    Ok(n)
}

/// Parse one crop body (`{"target":{"id":"card"},"preset":"16:9",
/// "frame":{"x":0,"y":0,"width":160,"height":90},"commit":true}`).
/// The target addresses exactly one clip and the preset names a
/// served preset; committing needs a non-empty actor. A refused body
/// names its cause and stages nothing.
pub fn parse_crop_body(body: &str) -> Result<CropRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_crop(
            "the crop needs a JSON crop body (`{\"target\":{\"id\":\"card\"},\"preset\":\"16:9\"}`); pass the crop from the canvas panel. Nothing was staged."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_crop(format!(
            "the crop body is not JSON ({e}). Pass `{{\"target\":{{\"id\":\"card\"}},\"preset\":\"16:9\"}}`. Nothing was staged."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_crop(
            "the crop body is not a crop; pass a JSON object with a `target` and a `preset`. Nothing was staged."
                .to_string(),
        ));
    }
    let raw_target = value.get("target").ok_or_else(|| {
        refused_crop(
            "the crop carries no `target`; crops act on one clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged."
                .to_string(),
        )
    })?;
    let target: sdk::ClipRef = serde_json::from_value(raw_target.clone()).map_err(|e| {
        refused_crop(format!(
            "the crop target {raw_target} is not a clip reference ({e}); pass `{{\"id\":\"...\"}}` or `{{\"index\":N}}`. Nothing was staged."
        ))
    })?;
    if target.id.is_none() && target.index.is_none() {
        return Err(refused_crop(format!(
            "the crop target {raw_target} names neither `id` nor `index`; crops act on one clip. Pass `{{\"id\":\"...\"}}` or `{{\"index\":N}}`. Nothing was staged."
        )));
    }
    let preset = value
        .get("preset")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if CropPreset::parse(&preset).is_none() {
        return Err(refused_crop(format!(
            "unknown crop preset {preset:?}: crop `free`, `16:9`, `9:16`, `1:1`, or `4:5`. Nothing was staged."
        )));
    }
    let frame = match value.get("frame") {
        None => None,
        Some(f) => {
            if !f.is_object() {
                return Err(refused_crop(format!(
                    "the crop `frame` value {f} is not a frame; pass `{{\"x\":N,\"y\":N,\"width\":N,\"height\":N}}` film pixels or omit it. Nothing was staged."
                )));
            }
            let x = f
                .get("x")
                .map(|v| crop_number(v, "x"))
                .transpose()?
                .unwrap_or(0.0);
            let y = f
                .get("y")
                .map(|v| crop_number(v, "y"))
                .transpose()?
                .unwrap_or(0.0);
            let width = f
                .get("width")
                .map(|v| crop_number(v, "width"))
                .transpose()?
                .ok_or_else(|| {
                    refused_crop(
                        "the crop `frame` carries no `width`; pass `{\"x\":N,\"y\":N,\"width\":N,\"height\":N}` film pixels. Nothing was staged."
                            .to_string(),
                    )
                })?;
            let height = f
                .get("height")
                .map(|v| crop_number(v, "height"))
                .transpose()?
                .ok_or_else(|| {
                    refused_crop(
                        "the crop `frame` carries no `height`; pass `{\"x\":N,\"y\":N,\"width\":N,\"height\":N}` film pixels. Nothing was staged."
                            .to_string(),
                    )
                })?;
            if width <= 0.0 || height <= 0.0 {
                return Err(refused_crop(format!(
                    "the crop `frame` size {width}x{height}px is not positive; pass positive film pixels. Nothing was staged."
                )));
            }
            Some(CropFrame {
                x,
                y,
                width,
                height,
            })
        }
    };
    let commit = value
        .get("commit")
        .and_then(|c| c.as_bool())
        .unwrap_or(false);
    let cancelled = value
        .get("cancelled")
        .and_then(|c| c.as_bool())
        .unwrap_or(false);
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if commit && actor.is_empty() {
        return Err(refused_crop(
            "the crop commit names no actor; committed turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged."
                .to_string(),
        ));
    }
    let selection = match value.get("selection") {
        None => Vec::new(),
        Some(s) => {
            if let Some(text) = s.as_str() {
                text.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            } else if let Some(list) = s.as_array() {
                let mut out = Vec::new();
                for entry in list {
                    let key = entry.as_str().ok_or_else(|| {
                        refused_crop(format!(
                            "the crop `selection` entry {entry} is not a selection key; pass `id` or `#index` strings. Nothing was staged."
                        ))
                    })?;
                    if !key.trim().is_empty() {
                        out.push(key.trim().to_string());
                    }
                }
                out
            } else {
                return Err(refused_crop(format!(
                    "the crop `selection` value {s} is not a selection; pass `\"id,#index\"` or a key list. Nothing was staged."
                )));
            }
        }
    };
    let composition = value
        .get("composition")
        .and_then(|c| c.as_str())
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());
    Ok(CropRequest {
        target,
        preset,
        frame,
        commit,
        cancelled,
        actor,
        selection,
        composition,
    })
}

/// Resolve one crop target to its document index: an `id` must name a
/// declared clip, an `index` must land inside the open composition.
fn resolve_target(
    comp: &notation::model::Composition,
    target: &sdk::ClipRef,
) -> Result<usize, String> {
    if let Some(id) = &target.id {
        if let Some(idx) = comp.clips.iter().position(|c| c.id() == Some(id.as_str())) {
            return Ok(idx);
        }
        return Err(refused_crop(format!(
            "crop target clip id {id:?} names nothing; the open composition holds {} clip(s). Nothing was staged.",
            comp.clips.len()
        )));
    }
    if let Some(index) = target.index {
        if index < comp.clips.len() {
            return Ok(index);
        }
        return Err(refused_crop(format!(
            "crop target clip index {index} is past the open composition holding {} clip(s). Nothing was staged.",
            comp.clips.len()
        )));
    }
    Err(refused_crop(
        "the crop target names neither `id` nor `index`; crops act on one clip. Nothing was staged."
            .to_string(),
    ))
}

/// Refuse a crop on a hidden clip: hidden elements show no crop
/// frame, so the canvas cannot grab them — the locked canvas
/// relationship of this slice. Showing the clip first (the `visible`
/// row) hands the frame back.
fn refuse_hidden(clip: &notation::model::Clip, idx: usize, target: &sdk::ClipRef) -> String {
    let (line, column) = clip.line_col();
    let name = match clip.id() {
        Some(id) => format!("{} {id:?}", clip.what()),
        None => format!("{} (clip index {idx})", clip.what()),
    };
    let at = if line > 0 {
        format!(" at {target} (composition.kdl:{line}:{column})")
    } else {
        format!(" at {target}")
    };
    refused_crop(format!(
        "crop on hidden {name}{at} refuses: hidden elements show no crop frame. Show the clip first (the `visible` row), then crop. Nothing was staged."
    ))
}

/// The stable selection key of one document index: the declared `id`,
/// else `#index` — the same keys the canvas and timeline echo.
fn crop_key(comp: &notation::model::Composition, idx: usize) -> String {
    match comp.clips[idx].id() {
        Some(id) => id.to_string(),
        None => format!("#{idx}"),
    }
}

/// One proposed crop frame: the staged visible window the panel
/// paints before commit or cancel.
#[derive(Debug, Clone)]
pub struct ProposedCrop {
    /// Document index of the cropped clip.
    pub index: usize,
    /// Stable selection key of the cropped clip.
    pub key: String,
    /// Left edge in film pixels.
    pub x: f64,
    /// Top edge in film pixels.
    pub y: f64,
    /// Frame width in film pixels.
    pub width: f64,
    /// Frame height in film pixels.
    pub height: f64,
}

/// Fit one aspect inside one requested size, keeping the requested
/// centre: the largest frame of `aspect` the request holds.
fn fit_aspect(req_w: f64, req_h: f64, aspect: f64) -> (f64, f64) {
    let h_from_w = req_w / aspect;
    if h_from_w <= req_h {
        (req_w, h_from_w)
    } else {
        (req_h * aspect, req_h)
    }
}

/// Compute the staged crop frame against the effective composition
/// without staging anything: the preset constrains the requested
/// frame (or the whole source rect when the maker dragged none), the
/// source rect clamps it, and the 1 px floor clamps last. An
/// un-fittable frame clamps and shows the clamped frame with the
/// cause named (REC-003). Sized clips (`box`, `image`, `svg`,
/// `flex`) crop; every other kind refuses with its rule and location
/// before anything stages, and hidden clips refuse with the locked
/// relationship.
pub fn propose_crop(
    comp: &notation::model::Composition,
    req: &CropRequest,
    film_w: f64,
    film_h: f64,
) -> Result<(ProposedCrop, bool, Vec<String>), String> {
    let idx = resolve_target(comp, &req.target)?;
    let clip = &comp.clips[idx];
    if clip.hidden() {
        return Err(refuse_hidden(clip, idx, &req.target));
    }
    let key = crop_key(comp, idx);
    let sized = matches!(
        clip,
        notation::model::Clip::Box(_)
            | notation::model::Clip::Image(_)
            | notation::model::Clip::Svg(_)
            | notation::model::Clip::Flex(_)
    );
    if !sized {
        let (line, column) = clip.line_col();
        let at = if line > 0 {
            format!(" (composition.kdl:{line}:{column})")
        } else {
            String::new()
        };
        return Err(refused_crop(format!(
            "crop on {key} ({} clip) refuses: `width`/`height` crop sized clips (`box`, `image`, `svg`, `flex`) only; this {} carries no crop window and keeps its prior frame. Nothing was staged{at}.",
            clip.what(),
            clip.what(),
        )));
    }
    let (ox, oy) = crate::studio_select::clip_origin(clip).ok_or_else(|| {
        refused_crop(format!(
            "crop on {key} ({} clip) refuses: this clip carries no canvas position and keeps its prior frame. Nothing was staged.",
            clip.what(),
        ))
    })?;
    let (opt_w, opt_h) = crate::studio_select::clip_size(clip).ok_or_else(|| {
        refused_crop(format!(
            "crop on {key} ({} clip) refuses: this clip carries no crop window and keeps its prior frame. Nothing was staged.",
            clip.what(),
        ))
    })?;
    let (sw, sh) = (opt_w.unwrap_or(film_w), opt_h.unwrap_or(film_h));
    let preset = CropPreset::parse(&req.preset).expect("preset parsed at the body");
    let mut notes = Vec::new();
    let mut clamped = false;
    let requested = req.frame.unwrap_or(CropFrame {
        x: ox,
        y: oy,
        width: sw,
        height: sh,
    });
    // Constrain the frame to the preset aspect, keeping the
    // requested centre: free follows the drag, fixed presets take
    // the largest fitting frame.
    let (mut w, mut h) = match preset.aspect() {
        None => (requested.width, requested.height),
        Some(aspect) => {
            let (fw, fh) = fit_aspect(requested.width, requested.height, aspect);
            if (fw - requested.width).abs() > 1e-9 || (fh - requested.height).abs() > 1e-9 {
                clamped = true;
                notes.push(format!(
                    "preset {} constrains the frame to {}x{}px",
                    req.preset,
                    num_text(fw),
                    num_text(fh)
                ));
            }
            (fw, fh)
        }
    };
    let mut x = requested.x + (requested.width - w) / 2.0;
    let mut y = requested.y + (requested.height - h) / 2.0;
    // Clamp the frame into the source rect: shrink to fit first
    // (keeping the aspect), then shift to fit.
    if w > sw || h > sh {
        let scale = (sw / w).min(sh / h);
        w *= scale;
        h *= scale;
        clamped = true;
        notes.push(format!(
            "preset {} cannot fit the source; the crop frame clamped to {}x{}px",
            req.preset,
            num_text(w),
            num_text(h)
        ));
    }
    if sw > w {
        let lo = ox.min(ox + sw - w);
        let hi = ox.max(ox + sw - w);
        let nx = x.clamp(lo, hi);
        if (nx - x).abs() > 1e-9 {
            clamped = true;
        }
        x = nx;
    } else {
        if (x - ox).abs() > 1e-9 {
            clamped = true;
        }
        x = ox;
    }
    if sh > h {
        let lo = oy.min(oy + sh - h);
        let hi = oy.max(oy + sh - h);
        let ny = y.clamp(lo, hi);
        if (ny - y).abs() > 1e-9 {
            clamped = true;
        }
        y = ny;
    } else {
        if (y - oy).abs() > 1e-9 {
            clamped = true;
        }
        y = oy;
    }
    // The 1 px floor clamps last, naming the bound.
    if w < MIN_CROP_PX {
        clamped = true;
        w = MIN_CROP_PX;
        notes.push(format!(
            "crop width on {key} clamped to the {MIN_CROP_PX:.0}px floor"
        ));
    }
    if h < MIN_CROP_PX {
        clamped = true;
        h = MIN_CROP_PX;
        notes.push(format!(
            "crop height on {key} clamped to the {MIN_CROP_PX:.0}px floor"
        ));
    }
    if w > sw || h > sh {
        let scale = (sw / w).min(sh / h);
        w *= scale;
        h *= scale;
        x = ox;
        y = oy;
        clamped = true;
        notes.push(format!(
            "the {MIN_CROP_PX:.0}px floor passes the source; the crop frame clamped to the source window {}x{}px",
            num_text(w),
            num_text(h)
        ));
    }
    Ok((
        ProposedCrop {
            index: idx,
            key,
            x,
            y,
            width: w,
            height: h,
        },
        clamped,
        notes,
    ))
}

/// Build the SDK patch operations of one proposed crop: the `x`,
/// `y`, `width`, and `height` rows in frame order — one patch turn
/// that rewrites only the visible window, never media.
pub fn crop_edit_ops(req: &CropRequest, proposed: &ProposedCrop) -> Vec<sdk::EditOp> {
    vec![
        sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "x".to_string(),
            value: num_text(proposed.x),
        },
        sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "y".to_string(),
            value: num_text(proposed.y),
        },
        sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "width".to_string(),
            value: num_text(proposed.width),
        },
        sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "height".to_string(),
            value: num_text(proposed.height),
        },
    ]
}

/// One applied crop turn.
#[derive(Debug, Clone)]
pub struct CropReceipt {
    /// Patch operations the turn recorded.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Apply one proposed crop through the Sprint 15 patch and history
/// machinery: build the frame row ops and record one validated turn
/// under the actor — the same undo system as row, timeline, and
/// headless edits. A refused value names its rule and location while
/// the composition, the delta, and history stay exactly as they were.
pub fn apply_crop(
    folder: &Path,
    composition: Option<&str>,
    req: &CropRequest,
    proposed: &ProposedCrop,
) -> Result<CropReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_crop(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let ops = crop_edit_ops(req, proposed);
    let studio_edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &studio_edit) {
        Ok(receipt) => Ok(CropReceipt {
            applied: receipt.applied,
            turn: receipt.turn,
            actor: receipt.actor,
            summaries: receipt.summaries,
        }),
        Err(message) => Err(message),
    }
}

/// One-line crop detail for receipts and the status line.
fn crop_detail(
    req: &CropRequest,
    proposed: &ProposedCrop,
    clamped: bool,
    notes: &[String],
) -> String {
    let mut detail = format!(
        "crop {} to {} at ({}, {}) {}x{}px",
        proposed.key,
        req.preset,
        num_text(proposed.x),
        num_text(proposed.y),
        num_text(proposed.width),
        num_text(proposed.height),
    );
    if clamped {
        detail.push_str(" (clamped)");
    }
    for note in notes {
        detail.push_str("; ");
        detail.push_str(note);
    }
    detail
}

/// Render one staged frame as JSON.
fn applied_frame_json(proposed: &ProposedCrop) -> String {
    format!(
        "{{\"target\":\"{}\",\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
        crate::studio::json_escape(&proposed.key),
        num_text(proposed.x),
        num_text(proposed.y),
        num_text(proposed.width),
        num_text(proposed.height),
    )
}

/// Shared selection echo, so canvas and timeline show the same
/// selection at once.
fn selection_echo_json(kept: &[String], dropped: &[String]) -> String {
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
    format!("{{\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]}}")
}

/// Render one staged crop preview as JSON: the frame with its clamp
/// flag and the lenient selection echo. Nothing staged, no turn
/// recorded. Every key is hyphenated.
pub fn crop_preview_json(
    req: &CropRequest,
    proposed: &ProposedCrop,
    clamped: bool,
    notes: &[String],
    kept: &[String],
    dropped: &[String],
) -> String {
    let detail = crop_detail(req, proposed, clamped, notes);
    format!(
        "{{\"staged\":false,\"committed\":false,\"cancelled\":false,\"preset\":\"{}\",\"actor\":\"{}\",\"applied-frame\":{},\"clamped\":{},\"notes\":[{}],\"detail\":\"{}\",\"turn\":null,\"summaries\":[],\"selection\":{}}}",
        crate::studio::json_escape(&req.preset),
        crate::studio::json_escape(&req.actor),
        applied_frame_json(proposed),
        clamped,
        notes
            .iter()
            .map(|n| format!("\"{}\"", crate::studio::json_escape(n)))
            .collect::<Vec<_>>()
            .join(","),
        crate::studio::json_escape(&format!("{detail}; preview only, nothing was staged.")),
        selection_echo_json(kept, dropped),
    )
}

/// Render one committed crop receipt as JSON: the named turn with its
/// summaries, the applied frame, and the synced selection echo.
/// Every key is hyphenated.
pub fn crop_receipt_json(
    req: &CropRequest,
    receipt: &CropReceipt,
    proposed: &ProposedCrop,
    clamped: bool,
    notes: &[String],
    kept: &[String],
    dropped: &[String],
) -> String {
    let detail = crop_detail(req, proposed, clamped, notes);
    let summaries = receipt
        .summaries
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"staged\":false,\"committed\":true,\"cancelled\":false,\"saved\":true,\"preset\":\"{}\",\"actor\":\"{}\",\"applied\":{},\"turn\":{},\"summaries\":[{summaries}],\"applied-frame\":{},\"clamped\":{},\"detail\":\"{} (turn {})\",\"selection\":{}}}",
        crate::studio::json_escape(&req.preset),
        crate::studio::json_escape(&receipt.actor),
        receipt.applied,
        receipt.turn,
        applied_frame_json(proposed),
        clamped,
        crate::studio::json_escape(&detail),
        receipt.turn,
        selection_echo_json(kept, dropped),
    )
}

/// Render one cancelled crop as JSON: the staged frame discards,
/// nothing commits, and the answer says so.
pub fn crop_cancelled_json(req: &CropRequest, kept: &[String], dropped: &[String]) -> String {
    format!(
        "{{\"staged\":false,\"committed\":false,\"cancelled\":true,\"preset\":\"{}\",\"detail\":\"crop {} cancelled; the staged frame discarded and nothing was recorded.\",\"selection\":{}}}",
        crate::studio::json_escape(&req.preset),
        crate::studio::json_escape(&req.preset),
        selection_echo_json(kept, dropped),
    )
}

/// Extra CSS the crop slice needs (Sprint 17, T004): the crop bar with
/// its preset buttons and status line. Style attributes stay
/// CSS-like hyphenated end to end (`background-color`,
/// `border-radius`, `font-variant-numeric`).
pub fn crop_css() -> String {
    r#"        #crop-bar{margin-top:8px}
        #crop-bar .preset-row{display:flex;gap:4px;align-items:center;flex-wrap:wrap}
        #crop-bar button[data-preset]{background-color:var(--panel-color);color:var(--ink-color);border:1px solid var(--line-color);border-radius:4px;padding:2px 8px}
        #crop-bar button[data-preset="1"]{background-color:var(--accent-color)}
        #crop-status{margin-top:4px;font-variant-numeric:tabular-nums}"#.to_string()
}

/// The crop control block (Sprint 17, T004): preset buttons for
/// Free, 16:9, 9:16, 1:1, and 4:5, commit and cancel, and the crop
/// status line.
pub fn crop_html() -> String {
    r#"<div class="dock-bar" id="crop-bar">
        <span>crop</span>
        <div class="preset-row" id="crop-presets">
        <button type="button" data-preset="free">free</button>
        <button type="button" data-preset="16:9">16:9</button>
        <button type="button" data-preset="9:16">9:16</button>
        <button type="button" data-preset="1:1">1:1</button>
        <button type="button" data-preset="4:5">4:5</button>
        <button type="button" id="crop-commit">commit crop</button>
        <button type="button" id="crop-cancel">cancel</button>
        </div>
        <div id="crop-status" aria-live="polite"></div>
        </div>"#
        .to_string()
}

/// The crop wiring (Sprint 17, T004): preset buttons preview the
/// constrained frame, commit lands one patch turn, cancel discards
/// with nothing staged. Selection rides along so canvas and timeline
/// stay in sync.
pub fn crop_js() -> String {
    r#"        let cropPreset = 'free';
        function cropStatus(t) { const s = document.getElementById('crop-status'); if (s) s.textContent = t; }
        async function paintCrop() {
          document.querySelectorAll('#crop-presets [data-preset]').forEach((b) => {
            b.dataset.preset = (b.getAttribute('data-preset') === cropPreset) ? '1' : '0';
          });
          cropStatus('crop ' + cropPreset + ' ready');
        }
        async function postCrop(payload) {
          try {
            const targets = selection.map((s) => (/^#(\d+)$/.test(s) ? {index: Number(s.slice(1))} : {id: s}));
            if (!targets.length) { cropStatus('crop names no target; select an element first.'); return; }
            if (targets.length > 1) { cropStatus('crop acts on one clip; select a single element.'); return; }
            const body = Object.assign({actor: 'maker', preset: cropPreset, target: targets[0], selection: selection.join(',')}, payload);
            const r = await fetch('/api/studio/crop', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
            const t = await r.text();
            if (!r.ok) { cropStatus(t); banner(t); return; }
            const j = JSON.parse(t);
            if (j.selection && j.selection.kept) selection = j.selection.kept.slice();
            cropStatus((j.detail || 'crop ready') + (j.turn !== null && j.turn !== undefined ? ' (turn ' + j.turn + ')' : ''));
            paintRows(); paintCanvasSelection(); syncLink(); paintLayers(); show(cur);
          } catch (e) { cropStatus('crop failed: ' + e); }
        }
        document.querySelectorAll('#crop-presets [data-preset]').forEach((b) => b.addEventListener('click', () => {
          cropPreset = b.getAttribute('data-preset');
          postCrop({commit: false});
        }));
        document.getElementById('crop-commit').addEventListener('click', () => postCrop({commit: true}));
        document.getElementById('crop-cancel').addEventListener('click', () => postCrop({cancelled: true}));"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_comp() -> notation::model::Composition {
        use notation::model::*;
        Composition {
            name: "t".to_string(),
            id: None,
            canvas: None,
            width: 320,
            height: 180,
            duration: 4.0,
            fps: Fps::from_int(10),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: vec![Clip::Box(BoxClip {
                asserts: MotionAsserts::default(),
                start: 0.0,
                duration: 4.0,
                id: Some("card".to_string()),
                track: TrackKind::Graphics,
                label: None,
                start_after: None,
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
                fill: Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                fill_paint: FillPaint::Solid,
                radius: 0.0,
                border_width: 0.0,
                border_color: Rgb { r: 0, g: 0, b: 0 },
                stroke_position: StrokePosition::Inside,
                box_shadow: None,
                blend_mode: BlendMode::Normal,
                backdrop_blur: 0.0,
                backdrop_brightness: 1.0,
                overflow: Overflow::Visible,
                mask: None,
                clip_path: None,
                opacity: 1.0,
                order: 2,
                turn: Turn3D::flat(),
                filter_blur: 0.0,
                grade: Grade::default(),
                effects: Vec::new(),
                vfx: VfxChain::default(),
                hidden: false,
                line: 1,
                column: 1,
            })],
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
            keys: Vec::new(),
            marks: Vec::new(),
            inputs: Vec::new(),
            variables: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Normalize::None,
            normalize_loc: None,
            grade: Grade::default(),
            lane_grades: Vec::new(),
            transitions: Vec::new(),
            motion_blur: None,
            line: 1,
            column: 1,
        }
    }

    fn crop_req(preset: &str, frame: Option<CropFrame>) -> CropRequest {
        CropRequest {
            target: sdk::ClipRef::by_id("card"),
            preset: preset.to_string(),
            frame,
            commit: false,
            cancelled: false,
            actor: "maker".to_string(),
            selection: Vec::new(),
            composition: None,
        }
    }

    #[test]
    fn crop_presets_parse_to_the_served_five() {
        assert_eq!(CropPreset::parse("free"), Some(CropPreset::Free));
        for (raw, aspect) in [
            ("16:9", 16.0 / 9.0),
            ("9:16", 9.0 / 16.0),
            ("1:1", 1.0),
            ("4:5", 4.0 / 5.0),
        ] {
            let preset = CropPreset::parse(raw).expect("preset parses");
            let got = preset.aspect().expect("fixed aspect");
            assert!((got - aspect).abs() < 1e-12, "{raw} aspect");
            assert_eq!(preset.as_str(), raw);
        }
        assert_eq!(CropPreset::parse("21:9"), None);
        assert_eq!(CropPreset::parse(""), None);
    }

    #[test]
    fn crop_free_without_a_frame_proposes_the_source_rect() {
        let comp = box_comp();
        let (proposed, clamped, notes) =
            propose_crop(&comp, &crop_req("free", None), 320.0, 180.0).expect("free proposes");
        assert!(!clamped);
        assert!(notes.is_empty());
        assert_eq!(
            (proposed.x, proposed.y, proposed.width, proposed.height),
            (10.0, 20.0, 100.0, 50.0)
        );
    }

    #[test]
    fn crop_preset_constrains_to_the_aspect_keeping_the_centre() {
        let comp = box_comp();
        // Card is 100x50: 16:9 keeps the 50px height, takes 88.88 of
        // width, centred on the 100px span.
        let (proposed, clamped, _) =
            propose_crop(&comp, &crop_req("16:9", None), 320.0, 180.0).expect("16:9 proposes");
        assert!(clamped, "constraining names the clamp");
        assert!((proposed.width - 50.0 * 16.0 / 9.0).abs() < 1e-9);
        assert!((proposed.height - 50.0).abs() < 1e-9);
        assert!((proposed.x - (10.0 + (100.0 - 50.0 * 16.0 / 9.0) / 2.0)).abs() < 1e-9);
        assert!((proposed.y - 20.0).abs() < 1e-9);
        // 1:1 takes the 50px height, centred on the 100px width.
        let (proposed, _, _) =
            propose_crop(&comp, &crop_req("1:1", None), 320.0, 180.0).expect("1:1 proposes");
        assert!((proposed.width - 50.0).abs() < 1e-9);
        assert!((proposed.height - 50.0).abs() < 1e-9);
        assert!((proposed.x - 35.0).abs() < 1e-9);
    }

    #[test]
    fn crop_oversized_frame_clamps_into_the_source() {
        let comp = box_comp();
        let (proposed, clamped, notes) = propose_crop(
            &comp,
            &crop_req(
                "9:16",
                Some(CropFrame {
                    x: 0.0,
                    y: 0.0,
                    width: 1000.0,
                    height: 1000.0,
                }),
            ),
            320.0,
            180.0,
        )
        .expect("oversize proposes");
        assert!(clamped, "un-fittable clamps");
        assert!(
            notes.iter().any(|n| n.contains("clamped")),
            "the clamp shows: {notes:?}"
        );
        assert!(proposed.width <= 100.0 + 1e-9);
        assert!(proposed.height <= 50.0 + 1e-9);
        assert!(proposed.x >= 10.0 - 1e-9);
        assert!(proposed.y >= 20.0 - 1e-9);
        assert!((proposed.width / proposed.height - 9.0 / 16.0).abs() < 1e-9);
    }

    #[test]
    fn crop_tiny_frame_clamps_to_the_1px_floor() {
        let comp = box_comp();
        let (proposed, clamped, notes) = propose_crop(
            &comp,
            &crop_req(
                "free",
                Some(CropFrame {
                    x: 10.0,
                    y: 20.0,
                    width: 0.2,
                    height: 0.2,
                }),
            ),
            320.0,
            180.0,
        )
        .expect("tiny proposes");
        assert!(clamped);
        assert!(notes.iter().any(|n| n.contains("1px floor")));
        assert!(proposed.width >= MIN_CROP_PX - 1e-9);
        assert!(proposed.height >= MIN_CROP_PX - 1e-9);
    }

    #[test]
    fn crop_bodies_refuse_with_causes() {
        assert!(parse_crop_body("").is_err());
        assert!(parse_crop_body("not json").is_err());
        assert!(parse_crop_body(r#"{"target":{"id":"card"}}"#).is_err());
        assert!(
            parse_crop_body(r#"{"target":{"id":"card"},"preset":"21:9"}"#)
                .unwrap_err()
                .contains("unknown crop preset")
        );
        assert!(parse_crop_body(
            r#"{"target":{"id":"card"},"preset":"free","frame":{"x":0,"y":0,"width":0,"height":10}}"#
        )
        .unwrap_err()
        .contains("not positive"));
        assert!(
            parse_crop_body(r#"{"target":{"id":"card"},"preset":"free","commit":true}"#)
                .unwrap_err()
                .contains("names no actor")
        );
    }

    #[test]
    fn crop_edit_ops_rewrite_only_the_visible_window() {
        let req = crop_req("16:9", None);
        let proposed = ProposedCrop {
            index: 0,
            key: "card".to_string(),
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 56.25,
        };
        let ops = crop_edit_ops(&req, &proposed);
        assert_eq!(ops.len(), 4);
        let props: Vec<String> = ops
            .iter()
            .map(|op| match op {
                sdk::EditOp::SetRow { property, .. } => property.clone(),
                _ => "other".to_string(),
            })
            .collect();
        assert_eq!(props, vec!["x", "y", "width", "height"]);
    }
}
