//! Tween cards, bezier editor, motion paths, and gesture capture
//! for the Studio shell (Sprint 17, T008 / CAP-008, CAP-009, J-05,
//! INT-001, INT-003, REC-001, REC-004).
//!
//! The animation tools write real motion through the same patch and
//! history machinery as every other Studio edit
//! ([`apply_card_edit`], [`apply_bezier_edit`], [`apply_path_edit`],
//! and [`apply_capture`] build SDK ops and apply them through
//! [`crate::studio::apply_studio_edit`]), so cards, curves, paths,
//! and captures share one undo system with canvas, inspector, and
//! timeline edits. Previews, refusals, and discarded captures stage
//! nothing; receipts echo the shared canvas and timeline selection
//! so all three views stay in sync.
//!
//! Cards carry `start`, `length`, and `speed`: the tween or set runs
//! `length / speed` seconds from `start` (`speed` 1 runs the card
//! length exactly). `set` cards hold one number with no easing;
//! colors hold no single number, so a set on `fill` or
//! `border-color` refuses naming the `tween` or `key` that
//! interpolates them instead. An empty target refuses naming the
//! missing clip.
//!
//! The bezier editor offers exactly the engine-supported ease
//! families ([`EASE_FAMILIES`]) with per-segment ease on keys and an
//! apply-to-all that writes every stop plus the key fallback in one
//! turn. Families outside the engine ([`UNSUPPORTED_EASE_FAMILIES`])
//! list as unavailable with the supported spelling, never faked; an
//! ease the engine cannot run refuses with its reason. Motion paths
//! preview live from the same arc-length tables the render uses, and
//! a path the engine cannot follow refuses with its cause.
//!
//! Gesture capture records pointer or wheel samples into keyframes
//! with smoothing and simplification as one turn; a capture with no
//! movement discards itself with a note and stages nothing.
//!
//! Style boundary: card and curve keys stay CSS-like hyphenated end
//! to end (`from-to`, `border-width`, `cubic-bezier`, `in-out-quad`),
//! and `font-family` try-order lists pass through untouched with
//! fail-closed resolution at render time. This layer writes no files
//! directly: edits apply through validated turns, and state besides
//! those turns lives only in the hidden project files the SDK owns.

use std::path::Path;

/// The nineteen named easing curves, in engine order.
pub const EASE_NAMED: [&str; 19] = [
    "linear",
    "in-quad",
    "out-quad",
    "in-out-quad",
    "in-cubic",
    "out-cubic",
    "in-out-cubic",
    "in-quart",
    "out-quart",
    "in-out-quart",
    "in-quint",
    "out-quint",
    "in-out-quint",
    "in-sine",
    "out-sine",
    "in-out-sine",
    "in-expo",
    "out-expo",
    "in-out-expo",
];

/// Every ease family the engine runs: the nineteen named curves plus
/// the parameterized set (`out-back(n)`, `cubic-bezier`, `steps`,
/// `spring`, `bounce`, `elastic`, `wiggle`, `hold`).
pub const EASE_FAMILIES: [&str; 27] = [
    "linear",
    "in-quad",
    "out-quad",
    "in-out-quad",
    "in-cubic",
    "out-cubic",
    "in-out-cubic",
    "in-quart",
    "out-quart",
    "in-out-quart",
    "in-quint",
    "out-quint",
    "in-out-quint",
    "in-sine",
    "out-sine",
    "in-out-sine",
    "in-expo",
    "out-expo",
    "in-out-expo",
    "out-back",
    "cubic-bezier",
    "steps",
    "spring",
    "bounce",
    "elastic",
    "wiggle",
    "hold",
];

/// Bezier editor presets: a display name with the exact engine
/// spelling it writes. The CSS-timed presets are `cubic-bezier`
/// curves the engine runs, never bare keywords.
pub const EASE_PRESETS: &[(&str, &str)] = &[
    ("linear", "linear"),
    ("ease", "cubic-bezier(0.25,0.1,0.25,1)"),
    ("ease-in", "cubic-bezier(0.42,0,1,1)"),
    ("ease-out", "cubic-bezier(0,0,0.58,1)"),
    ("ease-in-out", "cubic-bezier(0.42,0,0.58,1)"),
    ("in-quad", "in-quad"),
    ("out-quad", "out-quad"),
    ("in-out-quad", "in-out-quad"),
    ("in-cubic", "in-cubic"),
    ("out-cubic", "out-cubic"),
    ("in-out-cubic", "in-out-cubic"),
    ("in-quart", "in-quart"),
    ("out-quart", "out-quart"),
    ("in-out-quart", "in-out-quart"),
    ("in-quint", "in-quint"),
    ("out-quint", "out-quint"),
    ("in-out-quint", "in-out-quint"),
    ("in-sine", "in-sine"),
    ("out-sine", "out-sine"),
    ("in-out-sine", "in-out-sine"),
    ("in-expo", "in-expo"),
    ("out-expo", "out-expo"),
    ("in-out-expo", "in-out-expo"),
    ("out-back-soft", "out-back(1.2)"),
    ("out-back-strong", "out-back(2.2)"),
    ("spring", "spring"),
    ("spring-stiff", "spring(200,8)"),
    ("bounce", "bounce"),
    ("elastic", "elastic"),
    ("steps-4", "steps(4)"),
    ("wiggle-3", "wiggle(3)"),
    ("hold", "hold"),
];

/// Familiar names the engine has no family for: each lists as
/// unavailable with the supported spelling, never faked into a
/// nearby curve.
pub const UNSUPPORTED_EASE_FAMILIES: &[(&str, &str)] = &[
    (
        "ease",
        "CSS shorthand; write the `ease` preset `cubic-bezier(0.25,0.1,0.25,1)` instead",
    ),
    (
        "ease-in",
        "CSS shorthand; write the `ease-in` preset `cubic-bezier(0.42,0,1,1)` instead",
    ),
    (
        "ease-out",
        "CSS shorthand; write the `ease-out` preset `cubic-bezier(0,0,0.58,1)` instead",
    ),
    (
        "ease-in-out",
        "CSS shorthand; write the `ease-in-out` preset `cubic-bezier(0.42,0,0.58,1)` instead",
    ),
    (
        "step-start",
        "no engine curve jumps at the segment start; write `steps(n)` or `hold` instead",
    ),
    ("step-end", "CSS keyword; write `steps(1)` instead"),
    (
        "in-back",
        "no such family; write `out-back(n)` for overshoot with `n` from 0 up",
    ),
    (
        "in-out-back",
        "no such family; write `out-back(n)` for overshoot with `n` from 0 up",
    ),
];

/// How one authored ease name resolves against the engine.
#[derive(Debug, Clone)]
pub enum EaseAvailability {
    /// The engine runs the family.
    Available,
    /// A familiar name with no engine family: the supported spelling.
    Unsupported { hint: String },
    /// Nothing the engine knows: the supported families.
    Unknown { supported: String },
}

/// Family of one authored ease (`cubic-bezier(0.25,0.1,0.25,1)` reads
/// `cubic-bezier`; underscores read as hyphens, like the SDK).
fn ease_family(raw: &str) -> String {
    let t = raw.trim().replace('_', "-");
    match t.find('(') {
        None => t,
        Some(i) => t[..i].trim().to_string(),
    }
}

/// Why an animation request refused.
fn refused_animate(message: String) -> String {
    format!("studio refused: {message}")
}

/// Split one comma `selection` value the way the inspector does:
/// trimmed keys, empties dropped, order kept.
fn split_selection(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse one clip target value (`{"id":"..."}` or `{"index":N}`)
/// shared by every animation body.
fn parse_clip_target(
    value: Option<&serde_json::Value>,
    what: &str,
) -> Result<sdk::ClipRef, String> {
    let target_value = value.ok_or_else(|| {
        refused_animate(format!(
            "the {what} carries no `target`; animation addresses one clip. Pass `target:{{\"id\":\"...\"}}` or `target:{{\"index\":N}}`. Nothing was staged."
        ))
    })?;
    let target: sdk::ClipRef = serde_json::from_value(target_value.clone()).map_err(|e| {
        refused_animate(format!(
            "the {what} `target` is not a clip reference ({e}); pass `target:{{\"id\":\"...\"}}` or `target:{{\"index\":N}}`. Nothing was staged."
        ))
    })?;
    if target.id.is_none() && target.index.is_none() {
        return Err(refused_animate(format!(
            "the {what} `target` names neither `id` nor `index`; animation addresses one clip. Pass `target:{{\"id\":\"...\"}}` or `target:{{\"index\":N}}`. Nothing was staged."
        )));
    }
    Ok(target)
}

/// Kept and dropped selection keys against the shown composition, so
/// canvas, timeline, and animation receipts echo the same sync.
fn sync_selection(
    shown: &notation::model::Composition,
    selection: &[String],
) -> (Vec<String>, Vec<String>) {
    let rows = crate::studio::studio_rows_of(shown);
    crate::studio_canvas::sync_canvas_selection(selection, &rows)
}

/// Render the shared selection echo behind every animation receipt:
/// the kept keys with the dropped ones named, so canvas and timeline
/// stay in sync with the animation tools.
fn selection_echo(kept: &[String], dropped: &[String]) -> String {
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
    format!("\"selection\":[{kept_json}],\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]")
}

/// Resolve one authored ease against the engine catalog: supported
/// families run, familiar non-families list as unavailable with the
/// supported spelling, and anything else names the supported set.
pub fn ease_availability(raw: &str) -> EaseAvailability {
    let family = ease_family(raw);
    if EASE_FAMILIES.contains(&family.as_str()) {
        return EaseAvailability::Available;
    }
    if let Some((_, hint)) = UNSUPPORTED_EASE_FAMILIES
        .iter()
        .find(|(name, _)| *name == family)
    {
        return EaseAvailability::Unsupported {
            hint: hint.to_string(),
        };
    }
    EaseAvailability::Unknown {
        supported: EASE_FAMILIES.join(", "),
    }
}

/// One tween card: a `to`, `from`, `from-to`, or `set` card with its
/// window (`start`, `length`, `speed`) and — except `set` — its
/// easing. `speed` 1 runs the card length exactly; `duration` is
/// always `length / speed`.
#[derive(Debug, Clone)]
pub struct CardRequest {
    /// Actor the validated turn is recorded under.
    pub actor: String,
    /// Card kind (`to`, `from`, `from-to`, `set`).
    pub kind: String,
    /// Clip the card animates.
    pub target: sdk::ClipRef,
    /// Canonical property name (`x`, `opacity`, `border-width`, ...).
    pub property: String,
    /// Value motion starts at (`from`, `from-to`).
    pub from: Option<String>,
    /// Value motion ends at (`to`, `from-to`).
    pub to: Option<String>,
    /// Value the window holds (`set` only).
    pub value: Option<String>,
    /// Window open in composition seconds.
    pub start: f64,
    /// Card length in seconds before speed.
    pub length: f64,
    /// Speed multiplier (1 runs the card length exactly).
    pub speed: f64,
    /// Easing curve (`linear` when omitted; never on `set`).
    pub ease: String,
    /// Shared canvas and timeline selection the card scopes to.
    pub selection: Vec<String>,
}

/// Parse one tween card body (`{"actor":"maker","kind":"to",
/// "target":{"id":"title"},"property":"x","to":"120","start":1,
/// "length":2,"speed":1,"ease":"in-out-quad"}`): the actor must be
/// non-empty, the kind one of the four cards, the target one clip,
/// and the endpoints must fit the kind. A refused body names its
/// cause and stages nothing.
pub fn parse_card_body(body: &str) -> Result<CardRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_animate(
            "the tween card needs a JSON body (`{\"actor\":\"maker\",\"kind\":\"to\",\"target\":{\"id\":\"title\"},\"property\":\"x\",\"to\":\"120\",\"start\":0,\"length\":2}`); pass the card change. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_animate(format!(
            "the tween card body is not JSON ({e}). Pass a card with `actor`, `kind`, `target`, and `property`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_animate(
            "the tween card names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let kind = value
        .get("kind")
        .and_then(|k| k.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if !["to", "from", "from-to", "set"].contains(&kind.as_str()) {
        return Err(refused_animate(format!(
            "tween card kind {kind:?} names no card; write `to`, `from`, `from-to`, or `set`. Nothing was staged."
        )));
    }
    let target = parse_clip_target(value.get("target"), "tween card")?;
    let property = value
        .get("property")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if property.is_empty() {
        return Err(refused_animate(format!(
            "tween card `{kind}` names no `property`; cards animate one property. Pass a `property` like `x` or `opacity`. Nothing was staged."
        )));
    }
    let authored = |key: &str| value.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let (from, to, card_value) = (authored("from"), authored("to"), authored("value"));
    let ease_raw = authored("ease");
    if kind == "set" {
        if from.is_some() || to.is_some() {
            return Err(refused_animate(
                "a `set` card holds one number with no easing; remove `from`/`to` or write kind `from-to`. Nothing was staged.".to_string(),
            ));
        }
        if ease_raw.is_some() {
            return Err(refused_animate(
                "a `set` card holds one value with no easing; remove `ease` or write a `to`, `from`, or `from-to` card. Nothing was staged.".to_string(),
            ));
        }
        if card_value.is_none() {
            return Err(refused_animate(
                "a `set` card needs `value`; write the value the window holds. Nothing was staged."
                    .to_string(),
            ));
        }
    } else {
        if card_value.is_some() {
            return Err(refused_animate(format!(
                "a `{kind}` card moves between endpoints; remove `value` or write kind `set`. Nothing was staged."
            )));
        }
        let need_from = kind == "from" || kind == "from-to";
        let need_to = kind == "to" || kind == "from-to";
        if need_from && from.is_none() {
            return Err(refused_animate(format!(
                "a `{kind}` card needs `from`; write the value motion starts at. Nothing was staged."
            )));
        }
        if need_to && to.is_none() {
            return Err(refused_animate(format!(
                "a `{kind}` card needs `to`; write the value motion ends at. Nothing was staged."
            )));
        }
        if !need_from && from.is_some() {
            return Err(refused_animate(format!(
                "a `{kind}` card takes no `from`; remove it or write kind `from-to`. Nothing was staged."
            )));
        }
        if !need_to && to.is_some() {
            return Err(refused_animate(format!(
                "a `{kind}` card takes no `to`; remove it or write kind `from-to`. Nothing was staged."
            )));
        }
    }
    let number = |key: &str, default: Option<f64>| -> Result<f64, String> {
        match value.get(key) {
            None => default.ok_or_else(|| {
                refused_animate(format!(
                    "tween card `{kind}` needs `{key}`; write the card window (`start`, `length`, `speed` 1 when omitted). Nothing was staged."
                ))
            }),
            Some(v) => v.as_f64().ok_or_else(|| {
                refused_animate(format!(
                    "tween card `{key}` is not a number; write a finite number of seconds (a speed multiplier for `speed`). Nothing was staged."
                ))
            }),
        }
    };
    let start = number("start", Some(0.0))?;
    let length = number("length", None)?;
    let speed = number("speed", Some(1.0))?;
    if !start.is_finite() || start < 0.0 {
        return Err(refused_animate(format!(
            "tween card `{kind}` starts at {start:?}; write a finite start at or after 0. Nothing was staged."
        )));
    }
    if !length.is_finite() || length <= 0.0 {
        return Err(refused_animate(format!(
            "tween card `{kind}` lasts {length:?}; write a finite positive `length`. Nothing was staged."
        )));
    }
    if !speed.is_finite() || speed <= 0.0 {
        return Err(refused_animate(format!(
            "tween card `{kind}` runs at {speed:?}; write a finite positive `speed`. Nothing was staged."
        )));
    }
    let duration = length / speed;
    if !duration.is_finite() || duration <= 0.0 {
        return Err(refused_animate(format!(
            "tween card `{kind}` with `length` {length} at `speed` {speed} runs {duration:?}; write a length and speed with a finite positive quotient. Nothing was staged."
        )));
    }
    let ease = ease_raw.unwrap_or_else(|| "linear".to_string());
    if kind != "set" {
        match ease_availability(&ease) {
            EaseAvailability::Available => {}
            EaseAvailability::Unsupported { hint } => {
                return Err(refused_animate(format!(
                    "tween card easing {ease:?} lists as unavailable: {hint}. Nothing was staged."
                )));
            }
            EaseAvailability::Unknown { .. } => {}
        }
    }
    let selection = value
        .get("selection")
        .and_then(|s| s.as_str())
        .map(split_selection)
        .unwrap_or_default();
    Ok(CardRequest {
        actor,
        kind,
        target,
        property,
        from,
        to,
        value: card_value,
        start,
        length,
        speed,
        ease,
        selection,
    })
}

/// Window a card runs: `length / speed` seconds from `start`.
pub fn card_duration(req: &CardRequest) -> f64 {
    req.length / req.speed
}

/// One applied tween card: the recorded turn with its summaries and
/// the echoed selection sync.
#[derive(Debug, Clone)]
pub struct CardReceipt {
    /// Patch operations applied by this card.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// Clip the card animated.
    pub target: String,
    /// Selection keys kept by the sync.
    pub kept: Vec<String>,
    /// Selection keys dropped by the sync.
    pub dropped: Vec<String>,
}

/// Apply one tween card through the Sprint 15 patch and history
/// machinery (Sprint 17, T008): `to`, `from`, and `from-to` cards
/// append a tween, `set` cards hold one value, and the whole card
/// lands as one named turn. A refused card names its rule and
/// location while the composition, the delta, and history stay
/// exactly as they were.
pub fn apply_card_edit(
    folder: &Path,
    composition: Option<&str>,
    req: &CardRequest,
) -> Result<CardReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_animate(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let shown = crate::studio::effective_composition_for_session(folder, composition)?;
    let target = req.target.clone();
    let duration = card_duration(req);
    let ops = if req.kind == "set" {
        vec![sdk::EditOp::AddSet {
            target,
            property: req.property.clone(),
            value: req.value.clone().expect("set card carries a value"),
            start: req.start,
            duration,
        }]
    } else {
        vec![sdk::EditOp::AddTween {
            target,
            property: req.property.clone(),
            kind: req.kind.clone(),
            from: req.from.clone(),
            to: req.to.clone(),
            start: req.start,
            duration,
            ease: req.ease.clone(),
        }]
    };
    let edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &edit) {
        Ok(receipt) => {
            let (kept, dropped) = sync_selection(&shown, &req.selection);
            Ok(CardReceipt {
                applied: receipt.applied,
                turn: receipt.turn,
                actor: receipt.actor,
                summaries: receipt.summaries,
                target: req.target.to_string(),
                kept,
                dropped,
            })
        }
        Err(message) => Err(message),
    }
}

/// Convert one validated easing into the engine curve for unroll
/// sampling: every family maps one to one, exactly like the render
/// plan, so sampled values match what the film shows.
fn engine_ease(decl: &notation::model::EaseDecl) -> motion::Ease {
    match *decl {
        notation::model::EaseDecl::Linear => motion::Ease::Linear,
        notation::model::EaseDecl::InQuad => motion::Ease::InQuad,
        notation::model::EaseDecl::OutQuad => motion::Ease::OutQuad,
        notation::model::EaseDecl::InOutQuad => motion::Ease::InOutQuad,
        notation::model::EaseDecl::InCubic => motion::Ease::InCubic,
        notation::model::EaseDecl::OutCubic => motion::Ease::OutCubic,
        notation::model::EaseDecl::InOutCubic => motion::Ease::InOutCubic,
        notation::model::EaseDecl::InQuart => motion::Ease::InQuart,
        notation::model::EaseDecl::OutQuart => motion::Ease::OutQuart,
        notation::model::EaseDecl::InOutQuart => motion::Ease::InOutQuart,
        notation::model::EaseDecl::InQuint => motion::Ease::InQuint,
        notation::model::EaseDecl::OutQuint => motion::Ease::OutQuint,
        notation::model::EaseDecl::InOutQuint => motion::Ease::InOutQuint,
        notation::model::EaseDecl::InSine => motion::Ease::InSine,
        notation::model::EaseDecl::OutSine => motion::Ease::OutSine,
        notation::model::EaseDecl::InOutSine => motion::Ease::InOutSine,
        notation::model::EaseDecl::InExpo => motion::Ease::InExpo,
        notation::model::EaseDecl::OutExpo => motion::Ease::OutExpo,
        notation::model::EaseDecl::InOutExpo => motion::Ease::InOutExpo,
        notation::model::EaseDecl::OutBack(n) => motion::Ease::OutBack(n),
        notation::model::EaseDecl::CubicBezier(x1, y1, x2, y2) => {
            motion::Ease::CubicBezier(x1, y1, x2, y2)
        }
        notation::model::EaseDecl::Steps(n) => motion::Ease::Steps(n),
        notation::model::EaseDecl::Spring(s, d) => motion::Ease::Spring(s, d),
        notation::model::EaseDecl::Bounce => motion::Ease::Bounce,
        notation::model::EaseDecl::Elastic => motion::Ease::Elastic,
        notation::model::EaseDecl::Wiggle(n) => motion::Ease::Wiggle(n),
        notation::model::EaseDecl::Hold => motion::Ease::Hold,
    }
}

/// One bezier edit: an engine easing written to a tween, to one key,
/// to one key segment, or to every segment of a key at once.
#[derive(Debug, Clone)]
pub struct BezierRequest {
    /// Actor the validated turn is recorded under.
    pub actor: String,
    /// Authored easing curve.
    pub ease: String,
    /// Tween index the ease runs on (exactly one of `tween`, `key`).
    pub tween: Option<usize>,
    /// Key index the ease runs on.
    pub key: Option<usize>,
    /// Stop index leaving the eased segment (keys only).
    pub stop: Option<usize>,
    /// True writes the key fallback plus every stop (keys only).
    pub all: bool,
    /// Shared canvas and timeline selection the editor scopes to.
    pub selection: Vec<String>,
}

/// Parse one bezier body (`{"actor":"maker","ease":"in-out-quad",
/// "key":0,"stop":1}` or the same with `"tween":2` or `"all":true`):
/// the actor and ease must be present with exactly one of `tween`
/// and `key`; `stop` and `all` ride keys only. An unsupported or
/// unknown family refuses before anything stages — curves are never
/// faked.
pub fn parse_bezier_body(body: &str) -> Result<BezierRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_animate(
            "the bezier editor needs a JSON body (`{\"actor\":\"maker\",\"ease\":\"in-out-quad\",\"tween\":0}`); pass the curve change. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_animate(format!(
            "the bezier editor body is not JSON ({e}). Pass a curve with `actor`, `ease`, and `tween` or `key`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_animate(
            "the bezier editor names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let ease = value
        .get("ease")
        .and_then(|e| e.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if ease.is_empty() {
        return Err(refused_animate(
            "the bezier editor names no `ease`; pick a preset or write a curve like `in-out-quad`. Nothing was staged.".to_string(),
        ));
    }
    match ease_availability(&ease) {
        EaseAvailability::Available => {}
        EaseAvailability::Unsupported { hint } => {
            return Err(refused_animate(format!(
                "bezier easing {ease:?} lists as unavailable: {hint}. Nothing was staged."
            )));
        }
        EaseAvailability::Unknown { supported } => {
            return Err(refused_animate(format!(
                "bezier easing {ease:?} names no engine curve; write one of {supported}. Nothing was staged."
            )));
        }
    }
    let opt_usize = |key: &str| -> Result<Option<usize>, String> {
        match value.get(key) {
            None => Ok(None),
            Some(v) => v.as_u64().and_then(|n| usize::try_from(n).ok()).map(Some).ok_or_else(|| {
                refused_animate(format!(
                    "bezier `{key}` is not a whole directive index; pass a `{key}` from 0 up. Nothing was staged."
                ))
            }),
        }
    };
    let (tween, key, stop) = (opt_usize("tween")?, opt_usize("key")?, opt_usize("stop")?);
    let all = value.get("all").and_then(|a| a.as_bool()).unwrap_or(false);
    match (tween, key) {
        (Some(_), Some(_)) => {
            return Err(refused_animate(
                "the bezier editor takes one target; pass `tween` or `key`, never both. Nothing was staged.".to_string(),
            ));
        }
        (None, None) => {
            return Err(refused_animate(
                "the bezier editor names no target; pass the `tween` index or the `key` index the curve runs on. Nothing was staged.".to_string(),
            ));
        }
        _ => {}
    }
    if tween.is_some() && (stop.is_some() || all) {
        return Err(refused_animate(
            "tween easings run the whole window; per-segment ease and apply-to-all ride keys — pass `key` with `stop` or `all`. Nothing was staged.".to_string(),
        ));
    }
    if key.is_none() && all {
        return Err(refused_animate(
            "apply-to-all fans one curve across a key; pass `key` with `\"all\":true`. Nothing was staged.".to_string(),
        ));
    }
    let selection = value
        .get("selection")
        .and_then(|s| s.as_str())
        .map(split_selection)
        .unwrap_or_default();
    Ok(BezierRequest {
        actor,
        ease,
        tween,
        key,
        stop,
        all,
        selection,
    })
}

/// One applied bezier edit: the recorded turn with its summaries and
/// the echoed selection sync.
#[derive(Debug, Clone)]
pub struct BezierReceipt {
    /// Patch operations applied by this edit.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// What the curve runs on (`tween 2`, `key 1`, ...).
    pub target: String,
    /// Selection keys kept by the sync.
    pub kept: Vec<String>,
    /// Selection keys dropped by the sync.
    pub dropped: Vec<String>,
}

/// Apply one bezier edit through the Sprint 15 patch and history
/// machinery (Sprint 17, T008): a tween ease writes one patch,
/// per-segment ease writes one stop, and apply-to-all writes the key
/// fallback plus every stop — always as one named turn. A refused
/// curve names its reason while the composition, the delta, and
/// history stay exactly as they were.
pub fn apply_bezier_edit(
    folder: &Path,
    composition: Option<&str>,
    req: &BezierRequest,
) -> Result<BezierReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_animate(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let shown = crate::studio::effective_composition_for_session(folder, composition)?;
    let mut ops = Vec::new();
    let target = if let Some(tween) = req.tween {
        ops.push(sdk::EditOp::SetTweenEase {
            index: tween,
            ease: req.ease.clone(),
        });
        format!("tween {tween}")
    } else {
        let key = req.key.expect("bezier targets a tween or a key");
        if key >= shown.keys.len() {
            return Err(refused_animate(format!(
                "bezier key {key} names nothing; the open composition holds {} key(s). Nothing was staged.",
                shown.keys.len()
            )));
        }
        if req.all {
            ops.push(sdk::EditOp::SetKeyEase {
                key,
                stop: None,
                ease: req.ease.clone(),
            });
            for stop in 0..shown.keys[key].stops.len() {
                ops.push(sdk::EditOp::SetKeyEase {
                    key,
                    stop: Some(stop),
                    ease: req.ease.clone(),
                });
            }
            format!("key {key} all segments")
        } else if let Some(stop) = req.stop {
            if stop >= shown.keys[key].stops.len() {
                return Err(refused_animate(format!(
                    "bezier key {key} stop {stop} names nothing; key {key} holds {} stop(s). Nothing was staged.",
                    shown.keys[key].stops.len()
                )));
            }
            ops.push(sdk::EditOp::SetKeyEase {
                key,
                stop: Some(stop),
                ease: req.ease.clone(),
            });
            format!("key {key} stop {stop}")
        } else {
            ops.push(sdk::EditOp::SetKeyEase {
                key,
                stop: None,
                ease: req.ease.clone(),
            });
            format!("key {key}")
        }
    };
    let edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &edit) {
        Ok(receipt) => {
            let (kept, dropped) = sync_selection(&shown, &req.selection);
            Ok(BezierReceipt {
                applied: receipt.applied,
                turn: receipt.turn,
                actor: receipt.actor,
                summaries: receipt.summaries,
                target,
                kept,
                dropped,
            })
        }
        Err(message) => Err(message),
    }
}

/// Render one applied bezier receipt as JSON for the curve API.
pub fn bezier_receipt_json(receipt: &BezierReceipt) -> String {
    format!(
        "{{\"saved\":true,\"applied\":{},\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}],\"target\":\"{}\",{}}}",
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
        selection_echo(&receipt.kept, &receipt.dropped)
    )
}

/// Render the animation catalog as JSON for the shell: the card
/// kinds with the fields each needs, every engine-supported ease
/// family with its presets, and the unavailable families with the
/// supported spelling — plus the echoed selection sync.
pub fn animation_catalog_json(
    shown: &notation::model::Composition,
    selection: &[String],
) -> String {
    let (kept, dropped) = sync_selection(shown, selection);
    let kinds = "\"to\":{\"needs\":[\"to\"],\"forbids\":[\"from\",\"value\"]},\"from\":{\"needs\":[\"from\"],\"forbids\":[\"to\",\"value\"]},\"from-to\":{\"needs\":[\"from\",\"to\"],\"forbids\":[\"value\"]},\"set\":{\"needs\":[\"value\"],\"forbids\":[\"from\",\"to\",\"ease\"]}";
    let families = EASE_FAMILIES
        .iter()
        .map(|f| format!("\"{f}\""))
        .collect::<Vec<_>>()
        .join(",");
    let presets = EASE_PRESETS
        .iter()
        .map(|(name, ease)| {
            format!(
                "{{\"name\":\"{name}\",\"ease\":\"{}\"}}",
                crate::studio::json_escape(ease)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let unavailable = UNSUPPORTED_EASE_FAMILIES
        .iter()
        .map(|(family, hint)| {
            format!(
                "{{\"family\":\"{family}\",\"hint\":\"{}\"}}",
                crate::studio::json_escape(hint)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"composition\":\"{}\",\"card-kinds\":{{{kinds}}},\"ease-families\":[{families}],\"presets\":[{presets}],\"unavailable\":[{unavailable}],{}}}",
        crate::studio::json_escape(&shown.name),
        selection_echo(&kept, &dropped)
    )
}

/// One unroll request: computed values of tween `tween` or key
/// `key` at `samples` evenly spaced instants, for editing.
#[derive(Debug, Clone)]
pub struct UnrollRequest {
    /// Tween index to unroll (exactly one of `tween`, `key`).
    pub tween: Option<usize>,
    /// Key index to unroll.
    pub key: Option<usize>,
    /// Sample count after clamping into 2..=64.
    pub samples: usize,
    /// True while the asked count clamped into range.
    pub clamped: bool,
}

/// Parse one unroll query (`?tween=2&samples=9` or `?key=0`):
/// exactly one of `tween` and `key` must ride along; the sample
/// count defaults to 9 and clamps into 2..=64 with the clamp
/// reported. A refused query names its cause and computes nothing.
pub fn parse_unroll_params(
    tween: Option<&str>,
    key: Option<&str>,
    samples: Option<&str>,
) -> Result<UnrollRequest, String> {
    let opt_index = |raw: Option<&str>, name: &str| -> Result<Option<usize>, String> {
        match raw {
            None => Ok(None),
            Some(text) => text.trim().parse::<usize>().map(Some).map_err(|_| {
                refused_animate(format!(
                    "unroll `{name}` {text:?} is not a whole directive index; pass a `{name}` from 0 up. Nothing was computed."
                ))
            }),
        }
    };
    let (tween, key) = (opt_index(tween, "tween")?, opt_index(key, "key")?);
    match (tween, key) {
        (Some(_), Some(_)) => {
            return Err(refused_animate(
                "unroll takes one directive; pass `tween` or `key`, never both. Nothing was computed.".to_string(),
            ));
        }
        (None, None) => {
            return Err(refused_animate(
                "unroll names no directive; pass the `tween` index or the `key` index to expose. Nothing was computed.".to_string(),
            ));
        }
        _ => {}
    }
    let (samples, clamped) = match samples {
        None => (9, false),
        Some(text) => {
            let asked: usize = text.trim().parse().map_err(|_| {
                refused_animate(format!(
                    "unroll `samples` {text:?} is not a whole count; pass a count from 2 to 64. Nothing was computed."
                ))
            })?;
            if asked < 2 {
                (2, true)
            } else if asked > 64 {
                (64, true)
            } else {
                (asked, false)
            }
        }
    };
    Ok(UnrollRequest {
        tween,
        key,
        samples,
        clamped,
    })
}

/// One JSON number: finite values print plainly, anything else reads
/// `null` so the response stays valid JSON.
fn num_json(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "null".to_string()
    }
}

/// Unroll one numeric tween into its computed values: `samples`
/// evenly spaced instants across the window evaluated with the
/// tween easing. The rest value is the target clip's own value —
/// the same base the engine runs `to` from and `from` back to — so
/// sampled values match what the film shows. A property with no
/// finite rest value (like `typed`, which runs unbounded) refuses
/// naming the bound; colors interpolate channel by channel.
fn unroll_numeric_tween(
    shown: &notation::model::Composition,
    tween: &notation::model::TweenDecl,
    samples: usize,
) -> Result<String, String> {
    let clip = shown.clips.get(tween.target).ok_or_else(|| {
        refused_animate(format!(
            "unroll tween targets clip index {}; the open composition holds {} clip(s). Nothing was computed.",
            tween.target,
            shown.clips.len()
        ))
    })?;
    let canvas = (f64::from(shown.width), f64::from(shown.height));
    let own = render::motion_plan::own_values(clip, canvas);
    let prop = motion::Prop::from_name(&tween.property).ok_or_else(|| {
        refused_animate(format!(
            "unroll of tween on {:?} names no animatable property. Nothing was computed.",
            tween.property
        ))
    })?;
    if tween.property == "fill" || tween.property == "border-color" {
        let channels = if tween.property == "fill" {
            [
                motion::Prop::FillR,
                motion::Prop::FillG,
                motion::Prop::FillB,
            ]
        } else {
            [
                motion::Prop::BorderR,
                motion::Prop::BorderG,
                motion::Prop::BorderB,
            ]
        };
        let own_ch = [
            own[channels[0].index()],
            own[channels[1].index()],
            own[channels[2].index()],
        ];
        let channel = |rgb: Option<notation::model::Rgb>, i: usize| {
            rgb.map(|c| [f64::from(c.r), f64::from(c.g), f64::from(c.b)][i])
                .unwrap_or(own_ch[i])
        };
        let (from_ch, to_ch) = match tween.kind {
            notation::model::TweenKind::To => (
                own_ch,
                [
                    channel(tween.to_color, 0),
                    channel(tween.to_color, 1),
                    channel(tween.to_color, 2),
                ],
            ),
            notation::model::TweenKind::From => (
                [
                    channel(tween.from_color, 0),
                    channel(tween.from_color, 1),
                    channel(tween.from_color, 2),
                ],
                own_ch,
            ),
            notation::model::TweenKind::FromTo => (
                [
                    channel(tween.from_color, 0),
                    channel(tween.from_color, 1),
                    channel(tween.from_color, 2),
                ],
                [
                    channel(tween.to_color, 0),
                    channel(tween.to_color, 1),
                    channel(tween.to_color, 2),
                ],
            ),
        };
        let ease = engine_ease(&tween.ease);
        let mut out = String::new();
        for i in 0..samples {
            let p = if samples == 1 {
                0.0
            } else {
                i as f64 / (samples - 1) as f64
            };
            let t = tween.start + p * tween.duration;
            let e = ease.apply(p);
            let mix = |a: f64, b: f64| (a + (b - a) * e).round().clamp(0.0, 255.0) as u8;
            if !out.is_empty() {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"t\":{},\"color\":\"#{:02x}{:02x}{:02x}\"}}",
                num_json(t),
                mix(from_ch[0], to_ch[0]),
                mix(from_ch[1], to_ch[1]),
                mix(from_ch[2], to_ch[2])
            ));
        }
        return Ok(out);
    }
    let base = own[prop.index()];
    if !base.is_finite() {
        return Err(refused_animate(format!(
            "unroll of tween on {:?} names no finite rest value; `typed` runs unbounded, so unroll a bounded property instead. Nothing was computed.",
            tween.property
        )));
    }
    let (from_v, to_v) = match tween.kind {
        notation::model::TweenKind::To => (base, tween.to.unwrap_or(base)),
        notation::model::TweenKind::From => (tween.from.unwrap_or(base), base),
        notation::model::TweenKind::FromTo => {
            (tween.from.unwrap_or(base), tween.to.unwrap_or(base))
        }
    };
    let ease = engine_ease(&tween.ease);
    let mut out = String::new();
    for i in 0..samples {
        let p = if samples == 1 {
            0.0
        } else {
            i as f64 / (samples - 1) as f64
        };
        let t = tween.start + p * tween.duration;
        let v = from_v + (to_v - from_v) * ease.apply(p);
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"t\":{},\"value\":{}}}",
            num_json(t),
            num_json(v)
        ));
    }
    Ok(out)
}

/// Unroll one expression tween: the authored `expr` evaluated at
/// `samples` evenly spaced instants with `t` in composition seconds
/// and the declared input values — the same inputs the render
/// resolves. An unrunnable expression refuses with its reason.
fn unroll_expr_tween(
    shown: &notation::model::Composition,
    tween: &notation::model::TweenDecl,
    samples: usize,
) -> Result<String, String> {
    let expr_src = tween.expr.clone().unwrap_or_default();
    let names: Vec<&str> = shown.inputs.iter().map(|d| d.name.as_str()).collect();
    let expr = motion::expr::parse(&expr_src, &names).map_err(|e| {
        refused_animate(format!(
            "unroll cannot run the expression {expr_src:?}: {e}. Nothing was computed."
        ))
    })?;
    let values: Vec<f64> = shown.inputs.iter().map(|d| d.value).collect();
    let mut out = String::new();
    for i in 0..samples {
        let p = if samples == 1 {
            0.0
        } else {
            i as f64 / (samples - 1) as f64
        };
        let t = tween.start + p * tween.duration;
        let v = expr.eval(t, 0.0, &values);
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"t\":{},\"value\":{}}}",
            num_json(t),
            num_json(v)
        ));
    }
    Ok(out)
}

/// Unroll one path tween into live positions: `samples` evenly spaced
/// progress values mapped through the tween easing onto arc-length
/// distances along the same flattened path the render travels, with
/// the tangent heading at each stop. An unfollowable path refuses
/// with its cause.
fn unroll_path_tween(
    tween: &notation::model::TweenDecl,
    samples: usize,
) -> Result<(String, f64), String> {
    let data = tween.path.clone().unwrap_or_default();
    let path = motion::path::parse(&data).map_err(|e| {
        refused_animate(format!(
            "unroll cannot follow path {data:?}: {e}. Nothing was computed."
        ))
    })?;
    let table = motion::path::flatten(&path);
    let ease = engine_ease(&tween.ease);
    let mut out = String::new();
    for i in 0..samples {
        let p = if samples == 1 {
            0.0
        } else {
            i as f64 / (samples - 1) as f64
        };
        let t = tween.start + p * tween.duration;
        let (pos, heading) = motion::path::sample(&table, ease.apply(p) * table.total);
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"t\":{},\"x\":{},\"y\":{},\"heading\":{}}}",
            num_json(t),
            num_json(pos[0]),
            num_json(pos[1]),
            num_json(heading)
        ));
    }
    Ok((out, table.total))
}

/// Render one key stop for editing: the instant with the authored
/// value (numeric) or color plus the easing of the segment leaving
/// it (`null` means the key easing, then linear).
fn key_stop_json(stop: &notation::model::KeyStop) -> String {
    let at = num_json(stop.at);
    let ease = stop
        .ease
        .as_ref()
        .map(|e| format!("\"{}\"", crate::studio::json_escape(&e.name())))
        .unwrap_or_else(|| "null".to_string());
    if let Some(color) = &stop.value_color {
        format!(
            "{{\"at\":{at},\"color\":\"#{}\",\"ease\":{ease}}}",
            color.to_hex()
        )
    } else {
        format!(
            "{{\"at\":{at},\"value\":{},\"ease\":{ease}}}",
            num_json(stop.value)
        )
    }
}

/// Unroll one key into its computed values: every stop echoed for
/// editing plus `samples` evenly spaced instants across the stop
/// chain evaluated stop-to-stop with each segment's own easing (the
/// stop easing, then the key easing, then linear). Color keys
/// interpolate channel by channel; numeric and color samples never
/// mix.
fn unroll_key(key: &notation::model::KeyDecl, samples: usize) -> Result<(String, String), String> {
    let stops_json = key
        .stops
        .iter()
        .map(key_stop_json)
        .collect::<Vec<_>>()
        .join(",");
    let first = key.stops.first().map(|s| s.at).unwrap_or(0.0);
    let last = key.stops.last().map(|s| s.at).unwrap_or(0.0);
    let span = (last - first).max(0.0);
    let mut out = String::new();
    for i in 0..samples {
        let t = if samples <= 1 || span <= 0.0 {
            first
        } else {
            first + span * i as f64 / (samples - 1) as f64
        };
        let seg = key
            .stops
            .windows(2)
            .find(|w| t >= w[0].at && (t <= w[1].at || w[1].at >= last))
            .or_else(|| key.stops.windows(2).last());
        let Some(seg) = seg else {
            break;
        };
        let (a, b) = (&seg[0], &seg[1]);
        let p = if b.at > a.at {
            ((t - a.at) / (b.at - a.at)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let ease = a
            .ease
            .as_ref()
            .or(key.ease.as_ref())
            .map(engine_ease)
            .unwrap_or(motion::Ease::Linear);
        let e = ease.apply(p);
        if !out.is_empty() {
            out.push(',');
        }
        match (&a.value_color, &b.value_color) {
            (Some(ca), Some(cb)) => {
                let chan = |x: u8, y: u8| {
                    ((x as f64) + (y as f64 - x as f64) * e)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                out.push_str(&format!(
                    "{{\"t\":{},\"color\":\"#{:02x}{:02x}{:02x}\"}}",
                    num_json(t),
                    chan(ca.r, cb.r),
                    chan(ca.g, cb.g),
                    chan(ca.b, cb.b)
                ));
            }
            _ => {
                let v = a.value + (b.value - a.value) * e;
                out.push_str(&format!(
                    "{{\"t\":{},\"value\":{}}}",
                    num_json(t),
                    num_json(v)
                ));
            }
        }
    }
    Ok((stops_json, out))
}

/// Unroll one tween or key into the computed values the maker edits
/// against (Sprint 17, T008): numeric tweens sample the eased window,
/// expression tweens evaluate the authored `expr` with the declared
/// inputs, path tweens sample live positions with headings, and keys
/// echo every stop plus the per-segment computed values. Anything
/// unrunnable refuses with its reason and computes nothing.
pub fn unroll_json(
    shown: &notation::model::Composition,
    req: &UnrollRequest,
) -> Result<String, String> {
    if let Some(index) = req.tween {
        let tween = shown.tweens.get(index).ok_or_else(|| {
            refused_animate(format!(
                "unroll tween {index} names nothing; the open composition holds {} tween(s). Nothing was computed.",
                shown.tweens.len()
            ))
        })?;
        let window = format!(
            "[{},{}]",
            num_json(tween.start),
            num_json(tween.start + tween.duration)
        );
        if tween.path.is_some() {
            let (samples_json, total) = unroll_path_tween(tween, req.samples)?;
            return Ok(format!(
                "{{\"kind\":\"path-tween\",\"index\":{index},\"window\":{window},\"ease\":\"{}\",\"total\":{},\"samples\":[{samples_json}],\"samples-shown\":{},\"clamped\":{}}}",
                crate::studio::json_escape(&tween.ease.name()),
                num_json(total),
                req.samples,
                req.clamped
            ));
        }
        if tween.expr.is_some() {
            let samples_json = unroll_expr_tween(shown, tween, req.samples)?;
            return Ok(format!(
                "{{\"kind\":\"expr-tween\",\"index\":{index},\"window\":{window},\"expr\":\"{}\",\"samples\":[{samples_json}],\"samples-shown\":{},\"clamped\":{}}}",
                crate::studio::json_escape(&tween.expr.clone().unwrap_or_default()),
                req.samples,
                req.clamped
            ));
        }
        let samples_json = unroll_numeric_tween(shown, tween, req.samples)?;
        return Ok(format!(
            "{{\"kind\":\"tween\",\"index\":{index},\"tween-kind\":\"{}\",\"property\":\"{}\",\"window\":{window},\"ease\":\"{}\",\"samples\":[{samples_json}],\"samples-shown\":{},\"clamped\":{}}}",
            tween.kind.name(),
            crate::studio::json_escape(&tween.property),
            crate::studio::json_escape(&tween.ease.name()),
            req.samples,
            req.clamped
        ));
    }
    let key_index = req.key.expect("unroll targets a tween or a key");
    let key = shown.keys.get(key_index).ok_or_else(|| {
        refused_animate(format!(
            "unroll key {key_index} names nothing; the open composition holds {} key(s). Nothing was computed.",
            shown.keys.len()
        ))
    })?;
    if key.stops.len() < 2 {
        return Err(refused_animate(format!(
            "unroll key {key_index} holds {} stop(s); keys unroll through at least two stops in rising time order. Nothing was computed.",
            key.stops.len()
        )));
    }
    let (stops_json, samples_json) = unroll_key(key, req.samples)?;
    let fallback = key
        .ease
        .as_ref()
        .map(|e| format!("\"{}\"", crate::studio::json_escape(&e.name())))
        .unwrap_or_else(|| "null".to_string());
    Ok(format!(
        "{{\"kind\":\"key\",\"index\":{key_index},\"property\":\"{}\",\"ease\":{fallback},\"stops\":[{stops_json}],\"samples\":[{samples_json}],\"samples-shown\":{},\"clamped\":{}}}",
        crate::studio::json_escape(&key.property),
        req.samples,
        req.clamped
    ))
}

/// Render one applied card receipt as JSON for the card API: the
/// turn, its summaries, and the echoed selection sync so canvas,
/// timeline, and cards stay on the same selection.
pub fn card_receipt_json(receipt: &CardReceipt) -> String {
    format!(
        "{{\"saved\":true,\"applied\":{},\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}],\"target\":\"{}\",{}}}",
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
        selection_echo(&receipt.kept, &receipt.dropped)
    )
}

/// Editable anchor of one path: the command holding the point.
#[derive(Debug, Clone, Copy)]
struct AnchorRef {
    /// Command index in the parsed path.
    cmd: usize,
    /// True while the anchor closes a subpath (moves the matching
    /// `M` with it, so the path stays closed).
    close: bool,
}

/// Editable anchors of one parsed path: every command `to` point in
/// order. A `Z` anchor names the subpath start it closes.
fn anchor_refs(path: &motion::path::Path) -> Vec<AnchorRef> {
    path.cmds
        .iter()
        .enumerate()
        .map(|(cmd, c)| AnchorRef {
            cmd,
            close: matches!(c, motion::path::Cmd::Close { .. }),
        })
        .collect()
}

/// Anchor point of one command: the `to` every variant carries.
fn anchor_of(cmd: &motion::path::Cmd) -> [f64; 2] {
    match *cmd {
        motion::path::Cmd::Move { to } => to,
        motion::path::Cmd::Line { to } => to,
        motion::path::Cmd::Cubic { to, .. } => to,
        motion::path::Cmd::Quad { to, .. } => to,
        motion::path::Cmd::Arc { to, .. } => to,
        motion::path::Cmd::Close { to } => to,
    }
}

/// Move the anchor of one command by `delta`, shifting the control
/// points that sit exactly on the old anchor with it so handles the
/// maker placed stay attached. Other controls stay fixed, so the
/// curve reshapes around the dragged point.
fn shift_anchor(cmd: &mut motion::path::Cmd, old: [f64; 2], delta: [f64; 2]) {
    let shift = |p: &mut [f64; 2]| {
        if *p == old {
            p[0] += delta[0];
            p[1] += delta[1];
        }
    };
    match cmd {
        motion::path::Cmd::Move { to } => {
            to[0] += delta[0];
            to[1] += delta[1];
        }
        motion::path::Cmd::Line { to } => {
            to[0] += delta[0];
            to[1] += delta[1];
        }
        motion::path::Cmd::Cubic { c1, c2, to } => {
            shift(c1);
            shift(c2);
            to[0] += delta[0];
            to[1] += delta[1];
        }
        motion::path::Cmd::Quad { c, to } => {
            shift(c);
            to[0] += delta[0];
            to[1] += delta[1];
        }
        motion::path::Cmd::Arc { to, .. } => {
            to[0] += delta[0];
            to[1] += delta[1];
        }
        motion::path::Cmd::Close { to } => {
            to[0] += delta[0];
            to[1] += delta[1];
        }
    }
}

/// Write one coordinate the way the notation reads it: integers
/// print bare, fractions print fully, so edited paths stay readable
/// and re-parse exactly.
fn path_num(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Serialize one parsed path back to the SVG-subset data the engine
/// parses: absolute commands only, in the accepted vocabulary.
fn serialize_path(path: &motion::path::Path) -> String {
    let mut out = String::new();
    for cmd in &path.cmds {
        if !out.is_empty() {
            out.push(' ');
        }
        match *cmd {
            motion::path::Cmd::Move { to } => {
                out.push_str(&format!("M {},{}", path_num(to[0]), path_num(to[1])));
            }
            motion::path::Cmd::Line { to } => {
                out.push_str(&format!("L {},{}", path_num(to[0]), path_num(to[1])));
            }
            motion::path::Cmd::Cubic { c1, c2, to } => {
                out.push_str(&format!(
                    "C {},{} {},{} {},{}",
                    path_num(c1[0]),
                    path_num(c1[1]),
                    path_num(c2[0]),
                    path_num(c2[1]),
                    path_num(to[0]),
                    path_num(to[1])
                ));
            }
            motion::path::Cmd::Quad { c, to } => {
                out.push_str(&format!(
                    "Q {},{} {},{}",
                    path_num(c[0]),
                    path_num(c[1]),
                    path_num(to[0]),
                    path_num(to[1])
                ));
            }
            motion::path::Cmd::Arc {
                rx,
                ry,
                rot,
                large,
                sweep,
                to,
            } => {
                out.push_str(&format!(
                    "A {} {} {} {} {} {},{}",
                    path_num(rx),
                    path_num(ry),
                    path_num(rot),
                    if large { 1 } else { 0 },
                    if sweep { 1 } else { 0 },
                    path_num(to[0]),
                    path_num(to[1])
                ));
            }
            motion::path::Cmd::Close { .. } => {
                out.push('Z');
            }
        }
    }
    out
}

/// Editable anchor points of one path string: every command `to` in
/// order. A path the engine cannot follow refuses with its cause, so
/// previews never guess past a broken path.
pub fn path_anchor_points(data: &str) -> Result<Vec<[f64; 2]>, String> {
    let path = motion::path::parse(data.trim()).map_err(|e| {
        refused_animate(format!(
            "the engine cannot follow path {data:?}: {e}. Nothing was staged."
        ))
    })?;
    Ok(path.cmds.iter().map(anchor_of).collect())
}

/// Drag one path anchor to (`x`, `y`): the edited path string with
/// the anchor moved and its coincident controls shifted, so the
/// curve reshapes around the dragged point. A `Z` anchor moves its
/// subpath `M` with it, keeping the path closed. Anything
/// out-of-range refuses before anything stages.
pub fn move_path_point(data: &str, index: usize, x: f64, y: f64) -> Result<String, String> {
    if !x.is_finite() || !y.is_finite() {
        return Err(refused_animate(format!(
            "path point {index} moves to ({x:?},{y:?}); write finite canvas pixels. Nothing was staged."
        )));
    }
    let mut path = motion::path::parse(data.trim()).map_err(|e| {
        refused_animate(format!(
            "the engine cannot follow path {data:?}: {e}. Nothing was staged."
        ))
    })?;
    let refs = anchor_refs(&path);
    let anchor = refs.get(index).ok_or_else(|| {
        refused_animate(format!(
            "path point {index} names nothing; the path holds {} editable point(s). Nothing was staged.",
            refs.len()
        ))
    })?;
    let old = anchor_of(&path.cmds[anchor.cmd]);
    let delta = [x - old[0], y - old[1]];
    shift_anchor(&mut path.cmds[anchor.cmd], old, delta);
    if anchor.close {
        let mut start = None;
        for cmd in path.cmds[..anchor.cmd].iter().rev() {
            if let motion::path::Cmd::Move { .. } = cmd {
                start = Some(());
                break;
            }
        }
        if start.is_some() {
            for cmd in path.cmds[..anchor.cmd].iter_mut().rev() {
                if let motion::path::Cmd::Move { to } = cmd {
                    to[0] += delta[0];
                    to[1] += delta[1];
                    break;
                }
            }
        }
    }
    Ok(serialize_path(&path))
}

/// Live motion-path preview: `samples` positions along the same
/// arc-length table the render travels, clamped into 2..=256. A path
/// the engine cannot follow refuses with its cause.
pub fn preview_path(data: &str, samples: usize) -> Result<String, String> {
    let path = motion::path::parse(data.trim()).map_err(|e| {
        refused_animate(format!(
            "the engine cannot follow path {data:?}: {e}. Nothing was previewed."
        ))
    })?;
    let table = motion::path::flatten(&path);
    let n = samples.clamp(2, 256);
    let mut points = String::new();
    for i in 0..n {
        let d = if n == 1 {
            0.0
        } else {
            table.total * i as f64 / (n - 1) as f64
        };
        let (pos, heading) = motion::path::sample(&table, d);
        if !points.is_empty() {
            points.push(',');
        }
        points.push_str(&format!(
            "{{\"x\":{},\"y\":{},\"heading\":{}}}",
            num_json(pos[0]),
            num_json(pos[1]),
            num_json(heading)
        ));
    }
    Ok(format!(
        "{{\"d\":\"{}\",\"total\":{},\"points\":[{points}],\"samples-shown\":{n},\"clamped\":{}}}",
        crate::studio::json_escape(data.trim()),
        num_json(table.total),
        n != samples
    ))
}

/// One motion-path edit: a new path tween over a card window, or a
/// retarget of an existing path tween's travel path.
#[derive(Debug, Clone)]
pub struct PathRequest {
    /// Actor the validated turn is recorded under.
    pub actor: String,
    /// `add` appends a path tween, `retarget` replaces the travel
    /// path of tween `index`.
    pub action: String,
    /// Clip the new path tween animates (`add` only).
    pub target: Option<sdk::ClipRef>,
    /// Tween index to retarget (`retarget` only).
    pub index: Option<usize>,
    /// Travel path data.
    pub path: String,
    /// Morph destination (`None` clears it on retarget).
    pub path_to: Option<String>,
    /// True rotates the clip to the path tangent (`add` only).
    pub orient: bool,
    /// Window open in composition seconds (`add` only).
    pub start: f64,
    /// Card length in seconds before speed (`add` only).
    pub length: f64,
    /// Speed multiplier (`add` only).
    pub speed: f64,
    /// Easing curve (`add` only; `linear` when omitted).
    pub ease: String,
    /// Shared canvas and timeline selection the path scopes to.
    pub selection: Vec<String>,
}

/// Parse one motion-path body (`{"actor":"maker","action":"add",
/// "target":{"id":"title"},"path":"M 0,0 L 120,0","start":0,
/// "length":2}` or the same with `"action":"retarget","index":0`):
/// `add` carries a target and a card window, `retarget` the tween
/// index. An unfollowable path refuses here with its cause, before
/// anything stages.
pub fn parse_path_body(body: &str) -> Result<PathRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_animate(
            "the motion path needs a JSON body (`{\"actor\":\"maker\",\"action\":\"add\",\"target\":{\"id\":\"title\"},\"path\":\"M 0,0 L 120,0\",\"start\":0,\"length\":2}`); pass the path change. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_animate(format!(
            "the motion path body is not JSON ({e}). Pass a path with `actor`, `action`, and `path`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_animate(
            "the motion path names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let action = value
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if action != "add" && action != "retarget" {
        return Err(refused_animate(format!(
            "motion path action {action:?} names nothing; write `\"action\":\"add\"` or `\"action\":\"retarget\"`. Nothing was staged."
        )));
    }
    let path = value
        .get("path")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    if path.trim().is_empty() {
        return Err(refused_animate(
            "the motion path names no `path`; write SVG-subset data like `\"M 0,0 L 120,0\"`. Nothing was staged.".to_string(),
        ));
    }
    if motion::path::parse(path.trim()).is_err() {
        let why = motion::path::parse(path.trim()).expect_err("path fails after the check");
        return Err(refused_animate(format!(
            "the engine cannot follow path {path:?}: {why}. Nothing was staged."
        )));
    }
    let path_to = value
        .get("path-to")
        .or_else(|| value.get("path_to"))
        .and_then(|p| p.as_str())
        .map(str::to_string);
    if let Some(dest) = &path_to {
        if motion::path::parse(dest.trim()).is_err() {
            let why =
                motion::path::parse(dest.trim()).expect_err("destination fails after the check");
            return Err(refused_animate(format!(
                "the engine cannot follow `path-to` {dest:?}: {why}. Nothing was staged."
            )));
        }
        let from = motion::path::parse(path.trim()).expect("travel path parses after the check");
        let to = motion::path::parse(dest.trim()).expect("destination parses after the check");
        if motion::path::morph(&from, &to, 0.0).is_err() {
            let why =
                motion::path::morph(&from, &to, 0.0).expect_err("morph fails after the check");
            return Err(refused_animate(format!(
                "the engine cannot morph toward `path-to` {dest:?}: {why}. Nothing was staged."
            )));
        }
    }
    let selection = value
        .get("selection")
        .and_then(|s| s.as_str())
        .map(split_selection)
        .unwrap_or_default();
    if action == "retarget" {
        let index = value
            .get("index")
            .and_then(|i| i.as_u64())
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| {
                refused_animate(
                    "motion path `retarget` names no `index`; pass the tween index the edited points commit to. Nothing was staged.".to_string(),
                )
            })?;
        return Ok(PathRequest {
            actor,
            action,
            target: None,
            index: Some(index),
            path,
            path_to,
            orient: false,
            start: 0.0,
            length: 0.0,
            speed: 1.0,
            ease: String::new(),
            selection,
        });
    }
    let target = Some(parse_clip_target(value.get("target"), "motion path")?);
    let number = |key: &str, default: Option<f64>| -> Result<f64, String> {
        match value.get(key) {
            None => default.ok_or_else(|| {
                refused_animate(format!(
                    "motion path `add` needs `{key}`; write the path window (`start`, `length`, `speed` 1 when omitted). Nothing was staged."
                ))
            }),
            Some(v) => v.as_f64().ok_or_else(|| {
                refused_animate(format!(
                    "motion path `{key}` is not a number; write a finite number of seconds (a speed multiplier for `speed`). Nothing was staged."
                ))
            }),
        }
    };
    let start = number("start", Some(0.0))?;
    let length = number("length", None)?;
    let speed = number("speed", Some(1.0))?;
    if !start.is_finite() || start < 0.0 {
        return Err(refused_animate(format!(
            "motion path `add` starts at {start:?}; write a finite start at or after 0. Nothing was staged."
        )));
    }
    if !length.is_finite() || length <= 0.0 {
        return Err(refused_animate(format!(
            "motion path `add` lasts {length:?}; write a finite positive `length`. Nothing was staged."
        )));
    }
    if !speed.is_finite() || speed <= 0.0 {
        return Err(refused_animate(format!(
            "motion path `add` runs at {speed:?}; write a finite positive `speed`. Nothing was staged."
        )));
    }
    let duration = length / speed;
    if !duration.is_finite() || duration <= 0.0 {
        return Err(refused_animate(format!(
            "motion path `add` with `length` {length} at `speed` {speed} runs {duration:?}; write a length and speed with a finite positive quotient. Nothing was staged."
        )));
    }
    let ease = value
        .get("ease")
        .and_then(|e| e.as_str())
        .unwrap_or("linear")
        .to_string();
    if ease.trim().is_empty() {
        return Err(refused_animate(
            "motion path `add` names no `ease`; write a curve like `linear` or omit it. Nothing was staged.".to_string(),
        ));
    }
    let orient = value
        .get("orient")
        .and_then(|o| o.as_bool())
        .unwrap_or(false);
    Ok(PathRequest {
        actor,
        action,
        target,
        index: None,
        path,
        path_to,
        orient,
        start,
        length,
        speed,
        ease,
        selection,
    })
}

/// One applied motion-path edit: the recorded turn with its
/// summaries and the echoed selection sync.
#[derive(Debug, Clone)]
pub struct PathReceipt {
    /// Patch operations applied by this edit.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// What the path runs on (`clip ...` or `tween N`).
    pub target: String,
    /// Selection keys kept by the sync.
    pub kept: Vec<String>,
    /// Selection keys dropped by the sync.
    pub dropped: Vec<String>,
}

/// Apply one motion-path edit through the Sprint 15 patch and
/// history machinery (Sprint 17, T008): `add` appends a path tween
/// over the card window, `retarget` replaces the travel path of the
/// named tween — always as one named turn. An unfollowable path or
/// a mismatched morph refuses with its reason while the
/// composition, the delta, and history stay exactly as they were.
pub fn apply_path_edit(
    folder: &Path,
    composition: Option<&str>,
    req: &PathRequest,
) -> Result<PathReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_animate(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let shown = crate::studio::effective_composition_for_session(folder, composition)?;
    let (ops, target) = if req.action == "add" {
        let target = req.target.clone().expect("path add carries a target");
        let label = target.to_string();
        (
            vec![sdk::EditOp::AddPathTween {
                target,
                path: req.path.clone(),
                path_to: req.path_to.clone(),
                orient: req.orient,
                start: req.start,
                duration: req.length / req.speed,
                ease: req.ease.clone(),
            }],
            label,
        )
    } else {
        let index = req.index.expect("path retarget carries an index");
        (
            vec![sdk::EditOp::SetTweenPath {
                index,
                path: req.path.clone(),
                path_to: req.path_to.clone(),
            }],
            format!("tween {index}"),
        )
    };
    let edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &edit) {
        Ok(receipt) => {
            let (kept, dropped) = sync_selection(&shown, &req.selection);
            Ok(PathReceipt {
                applied: receipt.applied,
                turn: receipt.turn,
                actor: receipt.actor,
                summaries: receipt.summaries,
                target,
                kept,
                dropped,
            })
        }
        Err(message) => Err(message),
    }
}

/// One recorded gesture sample: the composition instant with the
/// pointer or wheel position in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureSample {
    /// Composition instant of the sample.
    pub at: f64,
    /// Canvas x in pixels.
    pub x: f64,
    /// Canvas y in pixels.
    pub y: f64,
}

/// One gesture capture: pointer or wheel samples for one clip with
/// the smoothing and simplification the shell applies before the
/// samples land as keyframes.
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// Actor the validated turn is recorded under.
    pub actor: String,
    /// Clip the captured motion animates.
    pub target: sdk::ClipRef,
    /// Recorded samples in time order.
    pub samples: Vec<CaptureSample>,
    /// True smooths positions with a 3-point moving average.
    pub smooth: bool,
    /// Ramer-Douglas-Peucker tolerance in canvas pixels (0 keeps
    /// every varying sample).
    pub tolerance: f64,
    /// Shared canvas and timeline selection the capture scopes to.
    pub selection: Vec<String>,
}

/// Parse one gesture capture body (`{"actor":"maker",
/// "target":{"id":"title"},"samples":[{"t":0,"x":10,"y":20},...]}`):
/// at least the actor, the target, and the sample list must ride
/// along. `smooth` defaults to true, `tolerance` to 4 canvas pixels.
/// A refused body names its cause and stages nothing.
pub fn parse_capture_body(body: &str) -> Result<CaptureRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_animate(
            "the gesture capture needs a JSON body (`{\"actor\":\"maker\",\"target\":{\"id\":\"title\"},\"samples\":[{\"t\":0,\"x\":10,\"y\":20}]}`); pass the recorded samples. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_animate(format!(
            "the gesture capture body is not JSON ({e}). Pass the capture with `actor`, `target`, and `samples`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_animate(
            "the gesture capture names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let target = parse_clip_target(value.get("target"), "gesture capture")?;
    let raw_samples = value.get("samples").and_then(|s| s.as_array()).ok_or_else(|| {
        refused_animate(
            "the gesture capture carries no `samples` array; record pointer or wheel movement first. Nothing was staged.".to_string(),
        )
    })?;
    let mut samples = Vec::with_capacity(raw_samples.len());
    for (i, raw) in raw_samples.iter().enumerate() {
        let num = |key: &str| -> Result<f64, String> {
            raw.get(key).and_then(|v| v.as_f64()).ok_or_else(|| {
                refused_animate(format!(
                    "capture sample {i} `{key}` is not a number; samples carry finite `t` seconds with `x`/`y` canvas pixels. Nothing was staged."
                ))
            })
        };
        let (at, x, y) = (num("t")?, num("x")?, num("y")?);
        if !at.is_finite() || at < 0.0 {
            return Err(refused_animate(format!(
                "capture sample {i} sits at {at:?}; write a finite instant at or after 0. Nothing was staged."
            )));
        }
        if !x.is_finite() || !y.is_finite() {
            return Err(refused_animate(format!(
                "capture sample {i} sits at ({x:?},{y:?}); write finite canvas pixels. Nothing was staged."
            )));
        }
        if let Some(prev) = samples.last() {
            let prev: &CaptureSample = prev;
            if at < prev.at {
                return Err(refused_animate(format!(
                    "capture sample {i} sits at {at} before sample {} at {}; write samples in rising time order. Nothing was staged.",
                    i - 1,
                    prev.at
                )));
            }
        }
        samples.push(CaptureSample { at, x, y });
    }
    let smooth = value
        .get("smooth")
        .and_then(|s| s.as_bool())
        .unwrap_or(true);
    let tolerance = value
        .get("tolerance")
        .and_then(|t| t.as_f64())
        .unwrap_or(4.0);
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(refused_animate(format!(
            "capture `tolerance` runs {tolerance:?}; write a finite pixel tolerance from 0 up. Nothing was staged."
        )));
    }
    let selection = value
        .get("selection")
        .and_then(|s| s.as_str())
        .map(split_selection)
        .unwrap_or_default();
    Ok(CaptureRequest {
        actor,
        target,
        samples,
        smooth,
        tolerance,
        selection,
    })
}

/// Smooth one capture with a 3-point moving average on positions:
/// endpoints stay pinned, interior points average their neighbors,
/// and instants never move.
pub fn smooth_samples(samples: &[CaptureSample]) -> Vec<CaptureSample> {
    if samples.len() <= 2 {
        return samples.to_vec();
    }
    let mut out = Vec::with_capacity(samples.len());
    for (i, s) in samples.iter().enumerate() {
        if i == 0 || i + 1 == samples.len() {
            out.push(*s);
        } else {
            out.push(CaptureSample {
                at: s.at,
                x: (samples[i - 1].x + s.x + samples[i + 1].x) / 3.0,
                y: (samples[i - 1].y + s.y + samples[i + 1].y) / 3.0,
            });
        }
    }
    out
}

/// Perpendicular distance of `p` from the line through `a` and `b`
/// in canvas pixels; a degenerate line reads the point distance.
fn point_line_distance(p: &CaptureSample, a: &CaptureSample, b: &CaptureSample) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 <= 0.0 {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    ((dy * p.x - dx * p.y + b.x * a.y - b.y * a.x).abs()) / len2.sqrt()
}

/// Simplify one capture with Ramer-Douglas-Peucker: endpoints stay
/// pinned, and the farthest interior point splits the run while it
/// strays past `tolerance` canvas pixels. A zero tolerance keeps
/// every varying sample.
pub fn simplify_samples(samples: &[CaptureSample], tolerance: f64) -> Vec<CaptureSample> {
    if samples.len() <= 2 || tolerance <= 0.0 {
        return samples.to_vec();
    }
    let mut keep = vec![false; samples.len()];
    keep[0] = true;
    keep[samples.len() - 1] = true;
    let mut stack = vec![(0usize, samples.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        if last <= first + 1 {
            continue;
        }
        let (mut far, mut dist) = (first, 0.0);
        for i in first + 1..last {
            let d = point_line_distance(&samples[i], &samples[first], &samples[last]);
            if d > dist {
                dist = d;
                far = i;
            }
        }
        if dist > tolerance {
            keep[far] = true;
            stack.push((first, far));
            stack.push((far, last));
        }
    }
    samples
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, s)| *s)
        .collect()
}

/// Whether a simplified capture carries real movement: at least two
/// stops with distinct positions. A held pointer advances time at
/// one spot, which is still no movement, so it discards with a note.
fn capture_moves(samples: &[CaptureSample]) -> bool {
    if samples.len() < 2 {
        return false;
    }
    let first = &samples[0];
    samples[1..]
        .iter()
        .any(|s| s.x != first.x || s.y != first.y)
}

/// One gesture capture outcome: either the keyframes turn, or a
/// discard note with nothing staged.
pub enum CaptureOutcome {
    /// The capture landed as keyframes in one turn.
    Applied(CaptureReceipt),
    /// The capture discarded itself with a note; no turn recorded.
    Discarded {
        /// Why the capture discarded (empty movement named).
        note: String,
        /// Clip the capture addressed.
        target: String,
        /// Selection keys kept by the sync.
        kept: Vec<String>,
        /// Selection keys dropped by the sync.
        dropped: Vec<String>,
    },
}

/// One applied gesture capture: the keyframes turn with its
/// summaries, the stop counts measured through smoothing and
/// simplification, and the echoed selection sync.
#[derive(Debug, Clone)]
pub struct CaptureReceipt {
    /// Patch operations applied by this capture (2: `x` and `y`).
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// Clip the captured motion animates.
    pub target: String,
    /// Recorded sample count before simplification.
    pub recorded: usize,
    /// Stop count landed as keyframes.
    pub stops: usize,
    /// Selection keys kept by the sync.
    pub kept: Vec<String>,
    /// Selection keys dropped by the sync.
    pub dropped: Vec<String>,
}

/// Apply one gesture capture through the Sprint 15 patch and history
/// machinery (Sprint 17, T008): smoothing and simplification shape
/// the samples, then `x` and `y` keyframes land as one named turn.
/// An empty capture discards itself with a note naming that no
/// movement was recorded — no turn, nothing staged.
pub fn apply_capture(
    folder: &Path,
    composition: Option<&str>,
    req: &CaptureRequest,
) -> Result<CaptureOutcome, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_animate(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let shown = crate::studio::effective_composition_for_session(folder, composition)?;
    let (kept, dropped) = sync_selection(&shown, &req.selection);
    let shaped = if req.smooth {
        smooth_samples(&req.samples)
    } else {
        req.samples.clone()
    };
    let simplified = simplify_samples(&shaped, req.tolerance);
    if !capture_moves(&simplified) {
        return Ok(CaptureOutcome::Discarded {
            note: format!(
                "capture discarded: no movement was recorded on {} ({} recorded sample(s) simplify to no motion); move the pointer or wheel while recording. Nothing was staged, no turn recorded.",
                req.target,
                req.samples.len()
            ),
            target: req.target.to_string(),
            kept,
            dropped,
        });
    }
    let stops_of = |pick: fn(&CaptureSample) -> f64| -> Vec<sdk::KeyStopInput> {
        simplified
            .iter()
            .map(|s| sdk::KeyStopInput {
                at: s.at,
                value: pick(s).to_string(),
                ease: None,
            })
            .collect()
    };
    let ops = vec![
        sdk::EditOp::AddKey {
            target: req.target.clone(),
            property: "x".to_string(),
            stops: stops_of(|s| s.x),
            ease: None,
        },
        sdk::EditOp::AddKey {
            target: req.target.clone(),
            property: "y".to_string(),
            stops: stops_of(|s| s.y),
            ease: None,
        },
    ];
    let edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &edit) {
        Ok(receipt) => Ok(CaptureOutcome::Applied(CaptureReceipt {
            applied: receipt.applied,
            turn: receipt.turn,
            actor: receipt.actor,
            summaries: receipt.summaries,
            target: req.target.to_string(),
            recorded: req.samples.len(),
            stops: simplified.len(),
            kept,
            dropped,
        })),
        Err(message) => Err(message),
    }
}

/// Render one gesture capture outcome as JSON for the capture API:
/// the keyframes turn with its measured stop counts, or the discard
/// note with nothing staged.
pub fn capture_outcome_json(outcome: &CaptureOutcome) -> String {
    match outcome {
        CaptureOutcome::Applied(receipt) => format!(
            "{{\"saved\":true,\"applied\":{},\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}],\"target\":\"{}\",\"recorded\":{},\"stops\":{},{}}}",
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
            receipt.recorded,
            receipt.stops,
            selection_echo(&receipt.kept, &receipt.dropped)
        ),
        CaptureOutcome::Discarded {
            note,
            target,
            kept,
            dropped,
        } => format!(
            "{{\"saved\":false,\"note\":\"{}\",\"target\":\"{}\",{}}}",
            crate::studio::json_escape(note),
            crate::studio::json_escape(target),
            selection_echo(kept, dropped)
        ),
    }
}

/// Studio CSS for the animation panel: hyphenated properties only.
pub fn animation_css() -> String {
    r#"        #animation-panel{margin-top:12px;border-top:1px solid var(--line-color);padding-top:8px}
        #animation-panel h3{margin:4px 0;font-size:13px}
        #animation-panel .anim-row{display:flex;gap:8px;flex-wrap:wrap;align-items:baseline;margin:4px 0}
        #animation-panel label{font-variant-numeric:tabular-nums}
        #animation-panel input,#animation-panel select{max-width:160px}
        #animation-panel button{margin:2px}
        #anim-status{font-variant-numeric:tabular-nums;min-height:1.2em}
        #anim-catalog{font-size:12px}
"#.to_string()
}

/// Studio HTML for the animation panel: tween cards, the bezier
/// editor, motion paths, and gesture capture.
pub fn animation_html() -> String {
    r#"<section id="animation-panel" aria-label="animation">
        <h3>animation</h3>
        <div class="anim-row" id="anim-card-row">
          <label>kind <select id="anim-kind"><option value="to">to</option><option value="from">from</option><option value="from-to">from-to</option><option value="set">set</option></select></label>
          <label>target <input id="anim-target" type="text" placeholder="id or #index"></label>
          <label>property <input id="anim-property" type="text" value="x"></label>
          <label>from <input id="anim-from" type="text"></label>
          <label>to <input id="anim-to" type="text"></label>
          <label>value <input id="anim-value" type="text"></label>
          <label>start <input id="anim-start" type="number" value="0" step="0.1" min="0"></label>
          <label>length <input id="anim-length" type="number" value="2" step="0.1" min="0.1"></label>
          <label>speed <input id="anim-speed" type="number" value="1" step="0.25" min="0.1"></label>
          <label>ease <input id="anim-ease" type="text" value="linear"></label>
          <button id="anim-card-apply" type="button">apply card</button>
          <button id="anim-unroll" type="button">unroll</button>
        </div>
        <div class="anim-row" id="anim-bezier-row">
          <label>ease <input id="anim-bezier-ease" type="text" value="in-out-quad"></label>
          <label>tween <input id="anim-bezier-tween" type="number" min="0"></label>
          <label>key <input id="anim-bezier-key" type="number" min="0"></label>
          <label>stop <input id="anim-bezier-stop" type="number" min="0"></label>
          <label>all <input id="anim-bezier-all" type="checkbox"></label>
          <button id="anim-bezier-apply" type="button">shape curve</button>
        </div>
        <div class="anim-row" id="anim-path-row">
          <label>path <input id="anim-path" type="text" size="28" placeholder="M 0,0 L 120,0"></label>
          <label>path-to <input id="anim-path-to" type="text" size="20"></label>
          <button id="anim-path-add" type="button">add path tween</button>
          <button id="anim-path-preview" type="button">preview path</button>
        </div>
        <div class="anim-row" id="anim-capture-row">
          <button id="anim-record" type="button">record (R)</button>
          <span id="anim-record-note">press R, move the pointer over the canvas, press R again</span>
        </div>
        <p id="anim-status" role="status"></p>
        <div id="anim-catalog"></div>
      </section>
"#.to_string()
}

/// Studio JS for the animation panel: cards, curves, paths, and
/// capture post to the animation APIs and paint receipts with the
/// shared selection, so canvas, timeline, and animation stay in
/// sync.
pub fn animation_js() -> String {
    r#"        let animRecording = null;
        function animStatus(t) { const s = document.getElementById('anim-status'); if (s) s.textContent = t; }
        function animTarget() {
          const v = (document.getElementById('anim-target') || {}).value || '';
          const t = String(v).trim();
          if (/^\d+$/.test(t)) return {index: Number(t)};
          return {id: t};
        }
        function animSelection() { return (typeof selection !== 'undefined' ? selection : []).join(','); }
        async function animPost(url, payload) {
          const body = Object.assign({actor: 'maker', selection: animSelection()}, payload);
          const r = await fetch(url, {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
          const t = await r.text();
          if (!r.ok) {
            animStatus(t);
            if (typeof banner === 'function') banner(t);
            return null;
          }
          return JSON.parse(t);
        }
        function animSynced(j) {
          if (!j) return;
          if (j.selection && typeof selection !== 'undefined') { selection = j.selection.slice(); }
          animStatus((j.summaries || []).join('; ') + ' (turn ' + j.turn + ')');
          if (typeof paintRows === 'function') paintRows();
          if (typeof syncLink === 'function') syncLink();
        }
        async function animApplyCard() {
          const kind = (document.getElementById('anim-kind') || {}).value || 'to';
          const j = await animPost('/api/studio/card', {
            kind: kind,
            target: animTarget(),
            property: ((document.getElementById('anim-property') || {}).value || '').trim(),
            from: ((document.getElementById('anim-from') || {}).value || '') || undefined,
            to: ((document.getElementById('anim-to') || {}).value || '') || undefined,
            value: ((document.getElementById('anim-value') || {}).value || '') || undefined,
            start: Number((document.getElementById('anim-start') || {}).value || 0),
            length: Number((document.getElementById('anim-length') || {}).value || 0),
            speed: Number((document.getElementById('anim-speed') || {}).value || 1),
            ease: ((document.getElementById('anim-ease') || {}).value || 'linear').trim() || 'linear'
          });
          animSynced(j);
        }
        async function animUnroll() {
          try {
            const first = ((typeof selection !== 'undefined' && selection[0]) || '');
            void first;
            const q = new URLSearchParams();
            q.set('samples', '9');
            const r = await fetch('/api/studio/unroll?' + q.toString());
            const t = await r.text();
            if (!r.ok) { animStatus(t); return; }
            animStatus('unroll needs a tween or key index: ' + t.slice(0, 160));
          } catch (e) { animStatus('unroll failed: ' + e); }
        }
        async function animApplyBezier() {
          const num = (id) => {
            const el = document.getElementById(id);
            if (!el || String(el.value).trim() === '') return undefined;
            return Number(el.value);
          };
          const j = await animPost('/api/studio/bezier', {
            ease: ((document.getElementById('anim-bezier-ease') || {}).value || '').trim(),
            tween: num('anim-bezier-tween'),
            key: num('anim-bezier-key'),
            stop: num('anim-bezier-stop'),
            all: !!((document.getElementById('anim-bezier-all') || {}).checked)
          });
          animSynced(j);
        }
        async function animAddPath() {
          const j = await animPost('/api/studio/path', {
            action: 'add',
            target: animTarget(),
            path: ((document.getElementById('anim-path') || {}).value || '').trim(),
            'path-to': ((document.getElementById('anim-path-to') || {}).value || '').trim() || undefined,
            start: Number((document.getElementById('anim-start') || {}).value || 0),
            length: Number((document.getElementById('anim-length') || {}).value || 0),
            speed: Number((document.getElementById('anim-speed') || {}).value || 1),
            ease: ((document.getElementById('anim-ease') || {}).value || 'linear').trim() || 'linear'
          });
          animSynced(j);
        }
        async function animPreviewPath() {
          try {
            const d = ((document.getElementById('anim-path') || {}).value || '').trim();
            if (!d) { animStatus('preview needs path data like "M 0,0 L 120,0"'); return; }
            const q = new URLSearchParams();
            q.set('d', d);
            q.set('samples', '25');
            const r = await fetch('/api/studio/path-preview?' + q.toString());
            const t = await r.text();
            if (!r.ok) { animStatus(t); return; }
            const j = JSON.parse(t);
            animStatus('path total ' + Number(j.total).toFixed(1) + 'px over ' + j.points.length + ' preview points');
          } catch (e) { animStatus('path preview failed: ' + e); }
        }
        function animToggleRecord() {
          if (animRecording) {
            animStopRecord();
            return;
          }
          animRecording = {t0: Date.now(), samples: []};
          const note = document.getElementById('anim-record-note');
          if (note) note.textContent = 'recording: move the pointer over the canvas, press R again to stop';
          animStatus('recording gesture');
        }
        async function animStopRecord() {
          const rec = animRecording;
          animRecording = null;
          const note = document.getElementById('anim-record-note');
          if (note) note.textContent = 'press R, move the pointer over the canvas, press R again';
          if (!rec || rec.samples.length === 0) {
            const j = await animPost('/api/studio/capture', {target: animTarget(), samples: []});
            if (j && j.saved === false) animStatus(j.note);
            else animSynced(j);
            return;
          }
          const j = await animPost('/api/studio/capture', {target: animTarget(), samples: rec.samples});
          if (j && j.saved === false) animStatus(j.note);
          else animSynced(j);
        }
        document.addEventListener('pointermove', (e) => {
          if (!animRecording) return;
          const stage = document.getElementById('canvas-stage');
          const host = (stage && e.target && stage.contains(e.target)) ? stage : null;
          if (!host) return;
          const rect = host.getBoundingClientRect();
          animRecording.samples.push({t: (Date.now() - animRecording.t0) / 1000, x: e.clientX - rect.left, y: e.clientY - rect.top});
        });
        document.addEventListener('keydown', (e) => {
          const tag = (e.target && e.target.tagName) || '';
          if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target && e.target.isContentEditable)) return;
          if ((e.key === 'r' || e.key === 'R') && !e.ctrlKey && !e.metaKey && !e.altKey) {
            e.preventDefault();
            animToggleRecord();
          }
        });
        document.getElementById('anim-card-apply').addEventListener('click', animApplyCard);
        document.getElementById('anim-unroll').addEventListener('click', animUnroll);
        document.getElementById('anim-bezier-apply').addEventListener('click', animApplyBezier);
        document.getElementById('anim-path-add').addEventListener('click', animAddPath);
        document.getElementById('anim-path-preview').addEventListener('click', animPreviewPath);
        document.getElementById('anim-record').addEventListener('click', animToggleRecord);
        (async () => {
          try {
            const r = await fetch('/api/studio/animation');
            if (!r.ok) return;
            const j = await r.json();
            const box = document.getElementById('anim-catalog');
            if (box) box.textContent = 'curves: ' + (j['ease-families'] || []).join(', ') + ' — unavailable: ' + (j.unavailable || []).map((u) => u.family).join(', ');
          } catch (e) { /* catalog stays empty offline */ }
        })();
"#.to_string()
}

/// Render one applied motion-path receipt as JSON for the path API.
pub fn path_receipt_json(receipt: &PathReceipt) -> String {
    format!(
        "{{\"saved\":true,\"applied\":{},\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}],\"target\":\"{}\",{}}}",
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
        selection_echo(&receipt.kept, &receipt.dropped)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card_json(extra: &str) -> String {
        format!(
            "{{\"actor\":\"maker\",\"kind\":\"to\",\"target\":{{\"id\":\"title\"}},\"property\":\"x\",\"to\":\"120\",\"start\":0,\"length\":2{extra}}}"
        )
    }

    #[test]
    fn card_speed_scales_duration() {
        let req = parse_card_body(&card_json(",\"speed\":2")).unwrap();
        assert_eq!(card_duration(&req), 1.0);
        let req = parse_card_body(&card_json("")).unwrap();
        assert_eq!(card_duration(&req), 2.0);
    }

    #[test]
    fn card_set_forbids_ease_and_endpoints() {
        let body = "{\"actor\":\"maker\",\"kind\":\"set\",\"target\":{\"id\":\"title\"},\"property\":\"opacity\",\"value\":\"0.5\",\"start\":0,\"length\":1,\"ease\":\"linear\"}";
        assert!(parse_card_body(body).is_err());
        let body = "{\"actor\":\"maker\",\"kind\":\"set\",\"target\":{\"id\":\"title\"},\"property\":\"opacity\",\"start\":0,\"length\":1}";
        assert!(parse_card_body(body).is_err());
        let body = "{\"actor\":\"maker\",\"kind\":\"to\",\"target\":{\"id\":\"title\"},\"property\":\"x\",\"value\":\"3\",\"start\":0,\"length\":1}";
        assert!(parse_card_body(body).is_err());
    }

    #[test]
    fn card_empty_target_refuses() {
        let body = "{\"actor\":\"maker\",\"kind\":\"to\",\"property\":\"x\",\"to\":\"1\",\"start\":0,\"length\":1}";
        let err = parse_card_body(body).expect_err("missing target refuses");
        assert!(err.contains("no `target`"), "{err}");
    }

    #[test]
    fn card_bad_window_refuses() {
        let body = "{\"actor\":\"maker\",\"kind\":\"to\",\"target\":{\"id\":\"t\"},\"property\":\"x\",\"to\":\"1\",\"start\":-1,\"length\":1}";
        assert!(parse_card_body(body).is_err());
        let body = "{\"actor\":\"maker\",\"kind\":\"to\",\"target\":{\"id\":\"t\"},\"property\":\"x\",\"to\":\"1\",\"start\":0,\"length\":0}";
        assert!(parse_card_body(body).is_err());
        let body = "{\"actor\":\"maker\",\"kind\":\"to\",\"target\":{\"id\":\"t\"},\"property\":\"x\",\"to\":\"1\",\"start\":0,\"length\":1,\"speed\":0}";
        assert!(parse_card_body(body).is_err());
    }

    #[test]
    fn ease_catalog_marks_unsupported_without_faking() {
        assert!(matches!(
            ease_availability("in-out-quad"),
            EaseAvailability::Available
        ));
        assert!(matches!(
            ease_availability("cubic-bezier(0.25,0.1,0.25,1)"),
            EaseAvailability::Available
        ));
        match ease_availability("ease-in") {
            EaseAvailability::Unsupported { hint } => assert!(hint.contains("cubic-bezier")),
            other => panic!("ease-in must list unavailable, got {other:?}"),
        }
        match ease_availability("nope") {
            EaseAvailability::Unknown { supported } => assert!(supported.contains("linear")),
            other => panic!("unknown ease must name supported, got {other:?}"),
        }
    }

    #[test]
    fn bezier_rejects_tween_segments() {
        let body = "{\"actor\":\"maker\",\"ease\":\"linear\",\"tween\":0,\"stop\":1}";
        let err = parse_bezier_body(body).expect_err("tween stop refuses");
        assert!(err.contains("ride keys"), "{err}");
        let body = "{\"actor\":\"maker\",\"ease\":\"ease-in\",\"key\":0}";
        let err = parse_bezier_body(body).expect_err("unsupported refuses");
        assert!(err.contains("unavailable"), "{err}");
    }

    #[test]
    fn path_points_move_and_stay_closed() {
        let anchors = path_anchor_points("M 0,0 L 120,0 L 120,60 Z").unwrap();
        assert_eq!(anchors.len(), 4);
        let moved = move_path_point("M 0,0 L 120,0", 1, 60.0, 30.0).unwrap();
        assert!(moved.contains("60") && moved.contains("30"), "{moved}");
        let closed = move_path_point("M 10,10 L 90,10 Z", 2, 20.0, 20.0).unwrap();
        let reparsed = motion::path::parse(&closed).expect("edited path re-parses");
        assert_eq!(reparsed.cmds.len(), 3);
        assert!(preview_path("M 0,0 L 100,0", 5)
            .unwrap()
            .contains("\"x\":100"));
        assert!(preview_path("M 0,0 Q 9,9", 5).is_err());
    }

    #[test]
    fn capture_simplifies_and_detects_emptiness() {
        let line: Vec<CaptureSample> = (0..9)
            .map(|i| CaptureSample {
                at: i as f64 * 0.1,
                x: i as f64 * 10.0,
                y: 5.0,
            })
            .collect();
        let simplified = simplify_samples(&line, 4.0);
        assert_eq!(simplified.len(), 2);
        assert!(capture_moves(&simplified));
        let still: Vec<CaptureSample> = (0..4)
            .map(|i| CaptureSample {
                at: i as f64 * 0.1,
                x: 7.0,
                y: 7.0,
            })
            .collect();
        assert!(!capture_moves(&simplify_samples(&still, 4.0)));
        let smoothed = smooth_samples(&line);
        assert_eq!(smoothed.len(), line.len());
        assert_eq!(smoothed[0], line[0]);
    }

    #[test]
    fn unroll_params_clamp_and_refuse() {
        let req = parse_unroll_params(Some("2"), None, Some("200")).unwrap();
        assert!(req.clamped && req.samples == 64);
        assert!(parse_unroll_params(None, None, None).is_err());
        assert!(parse_unroll_params(Some("0"), Some("0"), None).is_err());
    }
}
