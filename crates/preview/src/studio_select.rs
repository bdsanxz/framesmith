//! Canvas selection, handles, and nudge (Sprint 17, T002 / CAP-002,
//! J-01, J-06, INT-001, INT-003, REC-001, REC-002, REC-003).
//!
//! The canvas proposes, the SDK disposes: selection gestures resolve
//! against authored element boxes, transform gestures compute new
//! picture values, and every committed change applies through the
//! Sprint 15 patch and history machinery
//! ([`crate::studio::apply_studio_edit`]), so canvas edits share one
//! undo system with row, timeline, and headless edits. A committed
//! gesture lands as a named history turn with a receipt; a refused,
//! cancelled, or clamped gesture behaves per the specification with
//! its cause shown, and previews stage nothing.
//!
//! Gesture boundary: selection (`click`, `toggle`/`add`, `marquee`,
//! `clear`) is view state — it echoes the synced selection and writes
//! nothing, not even a hidden file. Transforms preview by default
//! (`commit` omitted or false) and stage nothing until the maker
//! commits; `cancelled` discards the staged preview with a note.
//! Handles move picture properties only: `x`/`y` on placed clips,
//! `width`/`height` on sized clips, `1` px nudges (`10` with Shift).
//! Rotation has no render-supported static row — `rotation` rides
//! tween/key cards (T008) — so a rotate drag previews the snapped
//! angle and committing refuses naming the rule and the location.
//!
//! Box boundary: hit-testing reads authored extents only (`box`,
//! `svg`, and `flex` rects; `image` rects defaulting per axis to the
//! film canvas; `path` bounds from the numeric tokens of `d`). Text,
//! video, audio, and nest clips carry no authored extents, so point
//! and marquee gestures skip them — select those from the layers
//! readout, by key, or from the timeline, where the same keys sync.
//! Hidden clips show no handles: geometric gestures skip them and
//! transforms on them refuse, while explicit key selection still
//! finds them so the maker can show them again.
//!
//! Style boundary: every request key and style attribute stays
//! CSS-like hyphenated (`show-handles`, `off-canvas`, `z-index`,
//! `background-color`, `border-radius`); `font-family` try-order
//! lists pass through untouched with fail-closed resolution at
//! render time, exactly like the SDK rows module.

use std::path::Path;

/// Smallest canvas size a resize may leave behind, in pixels: a
/// resize drag past zero clamps here and the receipt names the bound
/// (REC-003), instead of staging a refused non-positive size.
pub const MIN_CANVAS_SIZE_PX: f64 = 1.0;

/// Rotation snap step in degrees: a rotate drag with Shift lands on
/// the nearest multiple, exactly like handle rotation snaps.
pub const SNAP_ROTATION_DEG: f64 = 15.0;

/// Arrow nudge distance in canvas pixels.
pub const NUDGE_PX: f64 = 1.0;

/// Arrow nudge distance with Shift held, in canvas pixels.
pub const NUDGE_SHIFT_PX: f64 = 10.0;

/// Why a selection request refused: names the cause, stages nothing.
fn refused_select(message: String) -> String {
    format!("studio refused: {message}")
}

/// Why a transform request refused: names the cause, stages nothing.
fn refused_transform(message: String) -> String {
    format!("studio refused: {message}")
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

/// One authored element box in film pixels: the rectangle selection
/// gestures test against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementBox {
    /// Film pixels from the left film edge.
    pub x: f64,
    /// Film pixels from the top film edge.
    pub y: f64,
    /// Width in film pixels.
    pub w: f64,
    /// Height in film pixels.
    pub h: f64,
}

impl ElementBox {
    /// True while the film point sits inside the box, edges included.
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    /// True while the boxes overlap, edges included; a point box
    /// (zero size) intersects exactly while its point sits inside.
    pub fn intersects(&self, other: &ElementBox) -> bool {
        self.x <= other.x + other.w
            && self.x + self.w >= other.x
            && self.y <= other.y + other.h
            && self.y + self.h >= other.y
    }
}

/// Bounds of one `path` `d` string from its authored numeric tokens,
/// translated by the clip origin: the min/max over every finite
/// number in `d` (commands ride along as separators). Flags and
/// arc parameters widen the box slightly; the box stays an
/// authored-shape selection bound, never a render-ink claim. `None`
/// while `d` carries no finite number.
fn path_token_bounds(d: &str, ox: f64, oy: f64) -> Option<ElementBox> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut token = String::new();
    let mut numbers: Vec<f64> = Vec::new();
    let flush = |token: &mut String, numbers: &mut Vec<f64>| {
        let raw = token.trim();
        if !raw.is_empty() && raw != "+" && raw != "-" && raw != "." {
            if let Ok(n) = raw.parse::<f64>() {
                if n.is_finite() {
                    numbers.push(n);
                }
            }
        }
        token.clear();
    };
    for c in d.chars() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
            token.push(c);
        } else {
            flush(&mut token, &mut numbers);
        }
    }
    flush(&mut token, &mut numbers);
    // Coordinates arrive in x/y pairs; a trailing lone number cannot
    // place itself, so it stays out of the bound.
    let pairs = numbers.len() / 2;
    for i in 0..pairs {
        let x = numbers[2 * i];
        let y = numbers[2 * i + 1];
        if x < min_x {
            min_x = x;
        }
        if x > max_x {
            max_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }
    if pairs == 0 {
        return None;
    }
    Some(ElementBox {
        x: ox + min_x,
        y: oy + min_y,
        w: (max_x - min_x).max(0.0),
        h: (max_y - min_y).max(0.0),
    })
}

/// Authored selection box of one clip, or `None` while the kind
/// carries no authored extents (text, video, audio, nest) or the
/// shape names no numbers (empty `path` `d`). Image axes without an
/// authored size fall back to the film canvas per axis, exactly like
/// the render target rect.
pub fn element_box_of(
    clip: &notation::model::Clip,
    film_w: f64,
    film_h: f64,
) -> Option<ElementBox> {
    match clip {
        notation::model::Clip::Box(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width,
            h: c.height,
        }),
        notation::model::Clip::Svg(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width,
            h: c.height,
        }),
        notation::model::Clip::Lottie(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width,
            h: c.height,
        }),
        notation::model::Clip::Scene3d(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width,
            h: c.height,
        }),
        notation::model::Clip::Chart(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width.unwrap_or(film_w),
            h: c.height.unwrap_or(film_h),
        }),
        notation::model::Clip::Map(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width.unwrap_or(film_w),
            h: c.height.unwrap_or(film_h),
        }),
        notation::model::Clip::Flex(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width,
            h: c.height,
        }),
        notation::model::Clip::Image(c) => Some(ElementBox {
            x: c.x,
            y: c.y,
            w: c.width.unwrap_or(film_w),
            h: c.height.unwrap_or(film_h),
        }),
        notation::model::Clip::Path(c) => path_token_bounds(&c.d, c.x, c.y),
        notation::model::Clip::Text(_)
        | notation::model::Clip::Video(_)
        | notation::model::Clip::Audio(_)
        | notation::model::Clip::Nest(_) => None,
    }
}

/// Canvas origin of one clip, or `None` while the kind carries no
/// canvas position (video, audio, nest) — mirroring the SDK `x`/`y`
/// row support, so previews refuse exactly where commits would.
pub fn clip_origin(clip: &notation::model::Clip) -> Option<(f64, f64)> {
    match clip {
        notation::model::Clip::Text(c) => Some((c.x, c.y)),
        notation::model::Clip::Box(c) => Some((c.x, c.y)),
        notation::model::Clip::Image(c) => Some((c.x, c.y)),
        notation::model::Clip::Svg(c) => Some((c.x, c.y)),
        notation::model::Clip::Chart(c) => Some((c.x, c.y)),
        notation::model::Clip::Map(c) => Some((c.x, c.y)),
        notation::model::Clip::Scene3d(c) => Some((c.x, c.y)),
        notation::model::Clip::Lottie(c) => Some((c.x, c.y)),
        notation::model::Clip::Path(c) => Some((c.x, c.y)),
        notation::model::Clip::Flex(c) => Some((c.x, c.y)),
        notation::model::Clip::Video(_)
        | notation::model::Clip::Audio(_)
        | notation::model::Clip::Nest(_) => None,
    }
}

/// Canvas size of one clip: `(width, height)` with `None` per axis
/// while the axis inherits (image axes without an authored size).
/// `None` while the kind carries no canvas size (text, path,
/// video, audio, nest) — mirroring the SDK `width`/`height` row
/// support, so previews refuse exactly where commits would.
pub fn clip_size(clip: &notation::model::Clip) -> Option<(Option<f64>, Option<f64>)> {
    match clip {
        notation::model::Clip::Box(c) => Some((Some(c.width), Some(c.height))),
        notation::model::Clip::Svg(c) => Some((Some(c.width), Some(c.height))),
        notation::model::Clip::Lottie(c) => Some((Some(c.width), Some(c.height))),
        notation::model::Clip::Scene3d(c) => Some((Some(c.width), Some(c.height))),
        notation::model::Clip::Chart(c) => Some((c.width, c.height)),
        notation::model::Clip::Map(c) => Some((c.width, c.height)),
        notation::model::Clip::Flex(c) => Some((Some(c.width), Some(c.height))),
        notation::model::Clip::Image(c) => Some((c.width, c.height)),
        notation::model::Clip::Text(_)
        | notation::model::Clip::Path(_)
        | notation::model::Clip::Video(_)
        | notation::model::Clip::Audio(_)
        | notation::model::Clip::Nest(_) => None,
    }
}

/// Sides of the film rect an off-canvas box points past, in CSS
/// edge order (`top`, `right`, `bottom`, `left`): set exactly while
/// the box sits fully outside the film on that side. A box the film
/// still touches shows no indicator.
pub fn off_canvas_sides(b: &ElementBox, film_w: f64, film_h: f64) -> Vec<&'static str> {
    let mut sides = Vec::new();
    if b.x + b.w < 0.0 {
        sides.push("left");
    }
    if b.x > film_w {
        sides.push("right");
    }
    if b.y + b.h < 0.0 {
        sides.push("top");
    }
    if b.y > film_h {
        sides.push("bottom");
    }
    sides
}

/// Kind icon of one Layers tree row (Sprint 18, T004 / CAP-004): a
/// single glyph per element kind so the z-order tree scans by shape.
pub fn layer_icon(kind: &str) -> &'static str {
    match kind {
        "text" => "T",
        "video" => "▶",
        "box" => "■",
        "flex" => "▤",
        "image" => "◈",
        "path" => "✎",
        "svg" => "S",
        "audio" => "♪",
        "nest" => "⧉",
        _ => "?",
    }
}

/// One Layers group (Sprint 18, T004 / CAP-004): the group name with
/// its member keys in paint order, so the tree nests members under
/// their group header.
#[derive(Debug, Clone)]
pub struct LayerGroup {
    /// Group name (the `group:<name>` note minus the prefix).
    pub name: String,
    /// Member selection keys in paint order.
    pub members: Vec<String>,
}

/// Groups of one composition in first-member paint order (Sprint 18,
/// T004 / CAP-004): membership reads the render-neutral `group:<name>`
/// editor notes, so grouping re-renders byte-identically.
pub fn groups_of(comp: &notation::model::Composition) -> Vec<LayerGroup> {
    let mut groups: Vec<LayerGroup> = Vec::new();
    let mut entries: Vec<(usize, String, String)> = Vec::new();
    for (index, clip) in comp.clips.iter().enumerate() {
        let key = clip
            .id()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("#{index}"));
        if let Some(label) = clip.label() {
            if let Some(name) = sdk::group_name_of_label(label) {
                entries.push((index, name.to_string(), key));
            }
        }
    }
    entries.sort_by_key(|(index, _, _)| {
        let clip = &comp.clips[*index];
        (clip.order(), *index)
    });
    for (_, name, key) in entries {
        match groups.iter_mut().find(|g| g.name == name) {
            Some(group) => group.members.push(key),
            None => groups.push(LayerGroup {
                name,
                members: vec![key],
            }),
        }
    }
    groups
}

/// Validate one entered-group scope (Sprint 18, T004 / CAP-004):
/// entering scopes canvas selection to the group, so the scope must
/// name a real group. An empty scope means no scoping. Unknown names
/// refuse naming the holdings; nothing stages either way.
pub fn resolve_group_scope(
    comp: &notation::model::Composition,
    scope: &str,
) -> Result<Option<String>, String> {
    let scope = scope.trim();
    if scope.is_empty() {
        return Ok(None);
    }
    let holdings: Vec<String> = groups_of(comp).into_iter().map(|g| g.name).collect();
    if holdings.iter().any(|n| n == scope) {
        return Ok(Some(scope.to_string()));
    }
    let holdings = if holdings.is_empty() {
        "the open composition holds no layer groups".to_string()
    } else {
        format!(
            "the open composition groups {}",
            holdings
                .iter()
                .map(|n| format!("{n:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Err(refused_select(format!(
        "the entered group {scope:?} names no layer group; {holdings}. Enter a grouped name or exit the group. Nothing was staged."
    )))
}

/// One layers-readout entry: the element with its paint rank, kind
/// icon, group, box, off-canvas indicator, and whether the synced
/// selection holds it.
#[derive(Debug, Clone)]
pub struct LayerEntry {
    /// Document index of the clip.
    pub index: usize,
    /// Selection key: the stable `id` when declared, else `#index`.
    pub key: String,
    /// Stable `id`, when declared.
    pub id: Option<String>,
    /// Element kind (`box`, `text`, ...).
    pub kind: &'static str,
    /// Kind icon glyph for the tree row.
    pub icon: &'static str,
    /// Layers group name, when the clip carries a `group:<name>` note.
    pub group: Option<String>,
    /// Explicit paint order (`z-index`).
    pub order: i64,
    /// True while the clip skips picture and sound.
    pub hidden: bool,
    /// True while the synced selection holds the key.
    pub selected: bool,
    /// True while the clip sits inside the entered-group scope (always
    /// true while no group is entered).
    pub in_scope: bool,
    /// Authored selection box, when the kind carries extents.
    pub bounds: Option<ElementBox>,
    /// Off-canvas sides pointing at the element, when fully outside.
    pub indicator: Vec<String>,
}

/// The full layers readout of one composition (Sprint 17, T002 tree;
/// Sprint 18, T004 groups and scope): every clip in paint order
/// (`order`, ties in document order) with its kind icon, group, box,
/// hidden flag, synced selection mark, entered-group scope mark, and
/// off-canvas indicator. `scope` names the entered group, or `None`
/// while no group is entered (every entry in scope then).
pub fn layers_of(
    comp: &notation::model::Composition,
    film_w: f64,
    film_h: f64,
    selection: &[String],
    scope: Option<&str>,
) -> Vec<LayerEntry> {
    let mut entries: Vec<LayerEntry> = comp
        .clips
        .iter()
        .enumerate()
        .map(|(index, clip)| {
            let id = clip.id().map(|s| s.to_string());
            let key = id.clone().unwrap_or_else(|| format!("#{index}"));
            let kind = clip.kind();
            let group = clip
                .label()
                .and_then(sdk::group_name_of_label)
                .map(|s| s.to_string());
            let bounds = element_box_of(clip, film_w, film_h);
            let indicator = bounds
                .map(|b| {
                    off_canvas_sides(&b, film_w, film_h)
                        .iter()
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            let in_scope = match scope {
                None => true,
                Some(name) => group.as_deref() == Some(name),
            };
            LayerEntry {
                index,
                key: key.clone(),
                id,
                kind,
                icon: layer_icon(kind),
                group,
                order: clip.order(),
                hidden: clip.hidden(),
                selected: selection.iter().any(|s| s == &key),
                in_scope,
                bounds,
                indicator,
            }
        })
        .collect();
    entries.sort_by_key(|entry| (entry.order, entry.index));
    entries
}

/// Render one layers readout as JSON: the composition, the entries
/// in paint order with kind icons, groups, boxes, and indicators, the
/// group list, the entered scope, and the synced selection echo
/// (`kept`/`dropped`) so Layers, canvas, and timeline show the same
/// selection at once. Every key is hyphenated.
pub fn layers_json(
    comp: &notation::model::Composition,
    film_w: f64,
    film_h: f64,
    kept: &[String],
    dropped: &[String],
    scope: Option<&str>,
) -> String {
    let entries = layers_of(comp, film_w, film_h, kept, scope);
    let mut layers = String::new();
    for entry in &entries {
        if !layers.is_empty() {
            layers.push(',');
        }
        let id = match &entry.id {
            Some(id) => format!("\"{}\"", crate::studio::json_escape(id)),
            None => "null".to_string(),
        };
        let group = match &entry.group {
            Some(g) => format!("\"{}\"", crate::studio::json_escape(g)),
            None => "null".to_string(),
        };
        let bounds = match entry.bounds {
            Some(b) => format!("[{:.3},{:.3},{:.3},{:.3}]", b.x, b.y, b.w, b.h),
            None => "null".to_string(),
        };
        let indicator = entry
            .indicator
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",");
        layers.push_str(&format!(
            "{{\"key\":\"{}\",\"id\":{id},\"index\":{},\"kind\":\"{}\",\"icon\":\"{}\",\"group\":{group},\"order\":{},\"hidden\":{},\"selected\":{},\"in-scope\":{},\"box\":{bounds},\"off-canvas\":[{indicator}]}}",
            crate::studio::json_escape(&entry.key),
            entry.index,
            entry.kind,
            entry.icon,
            entry.order,
            entry.hidden,
            entry.selected,
            entry.in_scope,
        ));
    }
    let mut groups = String::new();
    for group in groups_of(comp) {
        if !groups.is_empty() {
            groups.push(',');
        }
        let members = group
            .members
            .iter()
            .map(|m| format!("\"{}\"", crate::studio::json_escape(m)))
            .collect::<Vec<_>>()
            .join(",");
        groups.push_str(&format!(
            "{{\"name\":\"{}\",\"members\":[{members}]}}",
            crate::studio::json_escape(&group.name),
        ));
    }
    let scope_json = match scope {
        Some(name) => format!("\"{}\"", crate::studio::json_escape(name)),
        None => "null".to_string(),
    };
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
    let shown: Vec<String> = entries
        .iter()
        .filter(|e| !e.indicator.is_empty())
        .map(|e| {
            format!(
                "\"{}\": [{}]",
                crate::studio::json_escape(&e.key),
                e.indicator
                    .iter()
                    .map(|s| format!("\"{s}\""))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect();
    let group_count = groups_of(comp).len();
    let scope_note = match scope {
        Some(name) => format!(" entered group {name:?};"),
        None => String::new(),
    };
    let status = format!(
        "layers list {} element(s) by z-order in {} group(s);{} {} off canvas.",
        entries.len(),
        group_count,
        scope_note,
        shown.len(),
    );
    format!(
        "{{\"composition\":\"{}\",\"film\":{{\"width\":{:.0},\"height\":{:.0}}},\"layers\":[{layers}],\"groups\":[{groups}],\"scope\":{scope_json},\"indicators\":{{{}}},\"selection\":{{\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]}},\"status-line\":\"{}\"}}",
        crate::studio::json_escape(&comp.name),
        film_w,
        film_h,
        shown.join(","),
        crate::studio::json_escape(&status),
    )
}

/// Why a Layers panel request refused: names the cause, stages nothing.
fn refused_layers(message: String) -> String {
    format!("studio refused: {message}")
}

/// One parsed Layers panel request (Sprint 18, T004 / CAP-004):
/// `reorder` drag-moves one layer to a document index, `group`
/// collects keys under a name, `ungroup` releases a name, `enter`
/// scopes selection to a group, and `exit` clears the scope.
/// Reorder, group, and ungroup commit through the shared patch and
/// history machinery as one named turn; enter, exit, and the
/// empty-group no-op stage nothing and record nothing.
#[derive(Debug, Clone)]
pub struct LayersRequest {
    /// Actor the committed turn is recorded under (required exactly
    /// for reorder, group, and ungroup — view actions record nothing).
    pub actor: String,
    /// Panel action (`reorder`, `group`, `ungroup`, `enter`, `exit`).
    pub action: String,
    /// Layer key the reorder drags, for `reorder`.
    pub key: Option<String>,
    /// Destination document index, for `reorder`: the dragged clip is
    /// removed, then inserted here, so it lands immediately before the
    /// clip occupying this index.
    pub to: Option<usize>,
    /// Paint order (`z-index`) the reorder assigns first, for
    /// `reorder`: `None` keeps the dragged clip's order, so same-band
    /// drags move by document index alone while cross-band drags
    /// carry the drop row's order and land exactly there.
    pub order: Option<i64>,
    /// Selection keys the group collects, for `group`.
    pub keys: Vec<String>,
    /// Group name, for `group`, `ungroup`, and `enter`.
    pub name: Option<String>,
    /// Shared selection echoed back so Layers, canvas, and timeline
    /// show the same selection at once.
    pub selection: Vec<String>,
}

/// One clip reference from a Layers key: a declared `id`, or a
/// `#index` entry. Anything else refuses naming the key.
fn layers_clip_ref(key: &str) -> Result<sdk::ClipRef, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err(refused_layers(
            "the layers request names an empty clip key; pass a declared `id` or a `#index` entry. Nothing was staged or recorded.".to_string(),
        ));
    }
    if let Some(rest) = key.strip_prefix('#') {
        match rest.trim().parse::<usize>() {
            Ok(index) => Ok(sdk::ClipRef::by_index(index)),
            Err(_) => Err(refused_layers(format!(
                "the layers key {key:?} is not a clip reference; pass a declared `id` or a `#index` entry. Nothing was staged or recorded."
            ))),
        }
    } else {
        Ok(sdk::ClipRef::by_id(key.to_string()))
    }
}

/// Parse one Layers panel body (`{"action":"reorder","key":"card",
/// "to":0}`, `{"action":"reorder","key":"card","to":0,"order":2}`,
/// `{"action":"group","keys":["a","b"],"name":"credits"}`,
/// `{"action":"ungroup","name":"credits"}`,
/// `{"action":"enter","name":"credits"}`, `{"action":"exit"}`).
/// Unknown actions and missing action fields refuse naming the cause;
/// nothing stages either way.
pub fn parse_layers_body(body: &str) -> Result<LayersRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_layers(
            "the layers panel needs a JSON layers body (`{\"action\":\"reorder\",\"key\":\"card\",\"to\":0}`); pass the panel gesture. Nothing was staged or recorded.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_layers(format!(
            "the layers body is not JSON ({e}). Pass `{{\"action\":\"reorder\",\"key\":\"card\",\"to\":0}}` or another panel gesture. Nothing was staged or recorded."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_layers(
            "the layers body is not a panel gesture; pass a JSON object with an `action`. Nothing was staged or recorded.".to_string(),
        ));
    }
    let action = value
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    match action.as_str() {
        "reorder" | "group" | "ungroup" | "enter" | "exit" => {}
        _ => {
            return Err(refused_layers(format!(
                "unknown layers action {action:?}: drag `reorder`, `group`, `ungroup`, `enter`, or `exit`. Nothing was staged or recorded."
            )));
        }
    }
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if matches!(action.as_str(), "reorder" | "group" | "ungroup") && actor.is_empty() {
        return Err(refused_layers(
            "the layers commit names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged or recorded.".to_string(),
        ));
    }
    let key = value
        .get("key")
        .and_then(|k| k.as_str())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    let to = match value.get("to") {
        None => None,
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| {
                refused_layers(format!(
                    "the layers `to` value {v} is not a document index; pass the destination index from 0. Nothing was staged or recorded."
                ))
            })?;
            Some(n as usize)
        }
    };
    let order = match value.get("order") {
        None => None,
        Some(v) => {
            let n = v.as_i64().ok_or_else(|| {
                refused_layers(format!(
                    "the layers `order` value {v} is not a paint order; pass the drop row's `z-index` as a whole number, or omit `order`. Nothing was staged or recorded."
                ))
            })?;
            Some(n)
        }
    };
    let keys = match value.get("keys") {
        None => Vec::new(),
        Some(v) => parse_selection_value(v, "keys").map_err(|message| {
            message.replace("the canvas selection `keys`", "the layers `keys`")
        })?,
    };
    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    let selection = match value.get("selection") {
        None => Vec::new(),
        Some(v) => parse_selection_value(v, "selection").map_err(|message| {
            message.replace("the canvas selection `selection`", "the layers `selection`")
        })?,
    };
    if action == "reorder" && key.is_none() {
        return Err(refused_layers(
            "the layers `reorder` names no `key`; pass the dragged layer key. Nothing was staged or recorded.".to_string(),
        ));
    }
    if action == "reorder" && to.is_none() {
        return Err(refused_layers(
            "the layers `reorder` names no `to`; pass the destination document index from 0. Nothing was staged or recorded.".to_string(),
        ));
    }
    if matches!(action.as_str(), "group" | "ungroup" | "enter") && name.is_none() {
        return Err(refused_layers(format!(
            "the layers `{action}` names no `name`; pass the group name. Nothing was staged or recorded."
        )));
    }
    Ok(LayersRequest {
        actor,
        action,
        key,
        to,
        order,
        keys,
        name,
        selection,
    })
}

/// The SDK patch operations one Layers commit stages, in order
/// (Sprint 18, T004): a drag reorder assigns the drop row's paint
/// order first (draw order follows `order`, ties in document order),
/// then moves the document index, so the drop lands exactly at the
/// tree position; a group collect is one group op, and a release is
/// one ungroup op. `None` while the group names no clips: grouping
/// an empty selection does nothing and the caller reports it with no
/// turn recorded.
fn layers_edit_ops(
    req: &LayersRequest,
    comp: &notation::model::Composition,
) -> Result<Option<Vec<sdk::EditOp>>, String> {
    match req.action.as_str() {
        "reorder" => {
            let raw = req.key.as_deref().expect("reorder carries a key");
            let target = layers_clip_ref(raw)?;
            let idx = resolve_key(comp, raw)?;
            let mut ops = Vec::new();
            // The drop row's paint order lands the dragged clip at the
            // tree position even across `z-index` bands; a matching
            // order stays untouched so same-band drags keep the row.
            if let Some(order) = req.order {
                if comp.clips[idx].order() != order {
                    ops.push(sdk::EditOp::SetRow {
                        target: target.clone(),
                        property: "z-index".to_string(),
                        value: order.to_string(),
                    });
                }
            }
            ops.push(sdk::EditOp::MoveElement {
                target,
                to: req.to.expect("reorder carries a destination"),
            });
            Ok(Some(ops))
        }
        "group" => {
            if req.keys.is_empty() {
                return Ok(None);
            }
            let mut targets = Vec::new();
            for key in &req.keys {
                targets.push(layers_clip_ref(key)?);
            }
            Ok(Some(vec![sdk::EditOp::GroupElements {
                targets,
                name: req.name.clone().expect("group carries a name"),
            }]))
        }
        "ungroup" => Ok(Some(vec![sdk::EditOp::UngroupElements {
            name: req.name.clone().expect("ungroup carries a name"),
        }])),
        _ => Err(refused_layers(format!(
            "the layers `{}` stages no turn; only `reorder`, `group`, and `ungroup` commit. Nothing was staged or recorded.",
            req.action
        ))),
    }
}

/// One applied Layers panel turn.
#[derive(Debug, Clone)]
pub struct LayersReceipt {
    /// Panel action the receipt answers.
    pub action: String,
    /// Sequence of the recorded history turn (`None` while the action
    /// stages nothing: `enter`, `exit`, and the empty-group no-op).
    pub turn: Option<u64>,
    /// Actor the turn was recorded under (empty while nothing records).
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// One-line detail for the panel status line.
    pub detail: String,
    /// Entered-group scope the panel now shows (`None` after `exit`).
    pub scope: Option<String>,
}

/// Apply one Layers panel request (Sprint 18, T004 / CAP-004 / INT-001):
/// commits travel the Sprint 15 patch and history machinery
/// ([`crate::studio::apply_studio_edit`]), so Layers shares one undo
/// system with canvas, timeline, and headless edits. View actions
/// (`enter`, `exit`) validate the scope and echo it without staging
/// anything; grouping an empty selection reports the no-op with no
/// turn. A refused commit names its cause and stages nothing.
pub fn apply_layers(
    folder: &std::path::Path,
    composition: Option<&str>,
    req: &LayersRequest,
    comp: &notation::model::Composition,
) -> Result<LayersReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_layers(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    match req.action.as_str() {
        "enter" => {
            let name = req.name.clone().expect("enter carries a name");
            let scope = resolve_group_scope(comp, &name)?;
            let entered = scope.clone().unwrap_or_default();
            let members = groups_of(comp)
                .into_iter()
                .find(|g| g.name == entered)
                .map(|g| g.members.len())
                .unwrap_or(0);
            return Ok(LayersReceipt {
                action: req.action.clone(),
                turn: None,
                actor: req.actor.clone(),
                summaries: Vec::new(),
                detail: format!(
                    "layers entered group {entered:?} ({members} member(s)); selection scopes to its members. Nothing was staged or recorded."
                ),
                scope,
            });
        }
        "exit" => {
            return Ok(LayersReceipt {
                action: req.action.clone(),
                turn: None,
                actor: req.actor.clone(),
                summaries: Vec::new(),
                detail: "layers exited the group scope; selection spans the composition. Nothing was staged or recorded.".to_string(),
                scope: None,
            });
        }
        _ => {}
    }
    let ops = match layers_edit_ops(req, comp)? {
        Some(ops) => ops,
        None => {
            return Ok(LayersReceipt {
                action: req.action.clone(),
                turn: None,
                actor: req.actor.clone(),
                summaries: Vec::new(),
                detail: format!(
                    "layers group {:?} names no clips; grouping an empty selection does nothing. Nothing was staged or recorded.",
                    req.name.clone().unwrap_or_default()
                ),
                scope: None,
            });
        }
    };
    let edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &edit) {
        Ok(receipt) => {
            let detail = match req.action.as_str() {
                "reorder" => match req.order {
                    Some(order) => format!(
                        "layers reorder {:?} to index {} order {} (turn {})",
                        req.key.clone().expect("reorder carries a key"),
                        req.to.expect("reorder carries a destination"),
                        order,
                        receipt.turn
                    ),
                    None => format!(
                        "layers reorder {:?} to index {} (turn {})",
                        req.key.clone().expect("reorder carries a key"),
                        req.to.expect("reorder carries a destination"),
                        receipt.turn
                    ),
                },
                "group" => format!(
                    "layers group {} as {:?} (turn {})",
                    req.keys.join(", "),
                    req.name.clone().expect("group carries a name"),
                    receipt.turn
                ),
                "ungroup" => format!(
                    "layers ungroup {:?} (turn {})",
                    req.name.clone().expect("ungroup carries a name"),
                    receipt.turn
                ),
                _ => format!("layers {} (turn {})", req.action, receipt.turn),
            };
            Ok(LayersReceipt {
                action: req.action.clone(),
                turn: Some(receipt.turn),
                actor: receipt.actor,
                summaries: receipt.summaries,
                detail,
                scope: None,
            })
        }
        Err(message) => Err(message),
    }
}

/// Render one Layers receipt as JSON: the action, the recorded turn
/// (`null` while the action stages nothing), the summaries, the
/// detail, the scope, and the synced selection echo (`kept`/
/// `dropped`) so Layers, canvas, and timeline show the same
/// selection at once. Every key is hyphenated.
pub fn layers_receipt_json(receipt: &LayersReceipt, kept: &[String], dropped: &[String]) -> String {
    let turn = match receipt.turn {
        Some(t) => t.to_string(),
        None => "null".to_string(),
    };
    let summaries = receipt
        .summaries
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let scope = match &receipt.scope {
        Some(name) => format!("\"{}\"", crate::studio::json_escape(name)),
        None => "null".to_string(),
    };
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
    format!(
        "{{\"action\":\"{}\",\"turn\":{turn},\"actor\":\"{}\",\"summaries\":[{summaries}],\"detail\":\"{}\",\"scope\":{scope},\"selection\":{{\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]}}}}",
        crate::studio::json_escape(&receipt.action),
        crate::studio::json_escape(&receipt.actor),
        crate::studio::json_escape(&receipt.detail),
    )
}

/// Read one selection list from a request value: a comma-separated
/// string or a JSON array of strings, trimming empties. Anything
/// else refuses naming the key.
fn parse_selection_value(value: &serde_json::Value, key: &str) -> Result<Vec<String>, String> {
    match value {
        serde_json::Value::String(raw) => Ok(raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()),
        serde_json::Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                let name = item.as_str().ok_or_else(|| {
                    refused_select(format!(
                        "the canvas selection `{key}` entry {item} is not a clip reference; pass declared `id` values or `#index` entries. Nothing was staged."
                    ))
                })?;
                if !name.trim().is_empty() {
                    out.push(name.trim().to_string());
                }
            }
            Ok(out)
        }
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err(refused_select(format!(
            "the canvas selection `{key}` value {value} is not a clip list; pass declared `id` values or `#index` entries as a comma string or an array. Nothing was staged."
        ))),
    }
}

/// One parsed canvas selection gesture (Sprint 17, T002): click by
/// key or at a film point, Shift-toggle or add of keys over the
/// current selection, marquee over a film rect, or clear. Selection
/// is view state: resolving echoes the synced keys and writes
/// nothing, not even a hidden file.
#[derive(Debug, Clone)]
pub struct SelectRequest {
    /// Gesture kind (`click`, `toggle`, `add`, `marquee`, `clear`).
    pub action: String,
    /// Key the click addresses, for key clicks.
    pub key: Option<String>,
    /// Film point the click addresses, for point clicks.
    pub at: Option<(f64, f64)>,
    /// Keys the toggle/add gestures flip or union.
    pub keys: Vec<String>,
    /// Current selection the toggle/add gestures start from.
    pub current: Vec<String>,
    /// Film rect the marquee covers `[x, y, w, h]`.
    pub rect: Option<ElementBox>,
    /// Shared selection the shell scoped the gesture to, echoed back
    /// so canvas and timeline show the same selection at once.
    pub selection: Vec<String>,
    /// Entered Layers group the gesture scopes to (Sprint 18, T004 /
    /// CAP-004): `None` while no group is entered. Key gestures at
    /// clips outside the group refuse naming the scope; point and
    /// marquee gestures skip out-of-group clips.
    pub scope: Option<String>,
}

/// Read one finite request number: present, numeric, and finite, or
/// a refusal naming the key.
fn select_number(value: &serde_json::Value, key: &str) -> Result<f64, String> {
    let n = value.as_f64().ok_or_else(|| {
        refused_select(format!(
            "the canvas selection `{key}` value {value} is not a number; pass finite film pixels. Nothing was staged."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_select(format!(
            "the canvas selection `{key}` value {n:?} is not finite; pass finite film pixels. Nothing was staged."
        )));
    }
    Ok(n)
}

/// Parse one selection body (`{"action":"click","key":"title"}`,
/// `{"action":"click","at":[30,40]}`, `{"action":"toggle",
/// "keys":["a"],"selection":"a,b"}`, `{"action":"marquee",
/// "rect":[0,0,320,180]}`, `{"action":"clear"}`). Unknown actions,
/// missing gesture fields, and non-finite geometry refuse naming
/// the cause; nothing stages either way.
pub fn parse_select_body(body: &str) -> Result<SelectRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_select(
            "the canvas selection needs a JSON select body (`{\"action\":\"click\",\"key\":\"title\"}`); pass the gesture from the canvas panel. Nothing was staged."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_select(format!(
            "the canvas selection body is not JSON ({e}). Pass `{{\"action\":\"click\",\"key\":\"title\"}}` or another gesture. Nothing was staged."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_select(
            "the canvas selection body is not a gesture; pass a JSON object with an `action`. Nothing was staged."
                .to_string(),
        ));
    }
    let action = value
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    match action.as_str() {
        "click" | "toggle" | "add" | "marquee" | "clear" => {}
        _ => {
            return Err(refused_select(format!(
                "unknown canvas selection action {action:?}: gesture `click`, `toggle`, `add`, `marquee`, or `clear`. Nothing was staged."
            )));
        }
    }
    let key = value
        .get("key")
        .and_then(|k| k.as_str())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    let at = match value.get("at") {
        None => None,
        Some(v) => {
            let pair = v.as_array().ok_or_else(|| {
                refused_select(format!(
                    "the canvas selection `at` value {v} is not a film point; pass `[x, y]` film pixels. Nothing was staged."
                ))
            })?;
            if pair.len() != 2 {
                return Err(refused_select(format!(
                    "the canvas selection `at` value {v} is not a film point; pass `[x, y]` film pixels. Nothing was staged."
                )));
            }
            Some((
                select_number(&pair[0], "at")?,
                select_number(&pair[1], "at")?,
            ))
        }
    };
    let keys = match value.get("keys") {
        None => Vec::new(),
        Some(v) => parse_selection_value(v, "keys")?,
    };
    let current = match value.get("current") {
        None => Vec::new(),
        Some(v) => parse_selection_value(v, "current")?,
    };
    let rect = match value.get("rect") {
        None => None,
        Some(v) => {
            let quad = v.as_array().ok_or_else(|| {
                refused_select(format!(
                    "the canvas selection `rect` value {v} is not a film rect; pass `[x, y, w, h]` film pixels. Nothing was staged."
                ))
            })?;
            if quad.len() != 4 {
                return Err(refused_select(format!(
                    "the canvas selection `rect` value {v} is not a film rect; pass `[x, y, w, h]` film pixels. Nothing was staged."
                )));
            }
            let x = select_number(&quad[0], "rect")?;
            let y = select_number(&quad[1], "rect")?;
            let w = select_number(&quad[2], "rect")?;
            let h = select_number(&quad[3], "rect")?;
            Some(ElementBox {
                x: x.min(x + w),
                y: y.min(y + h),
                w: w.abs(),
                h: h.abs(),
            })
        }
    };
    let selection = match value.get("selection") {
        None => Vec::new(),
        Some(v) => parse_selection_value(v, "selection")?,
    };
    let scope = match value.get("scope") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(raw)) => {
            let name = raw.trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        }
        Some(v) => {
            return Err(refused_select(format!(
                "the canvas selection `scope` value {v} is not a group name; pass the entered Layers group or omit `scope`. Nothing was staged."
            )));
        }
    };
    if action == "click" && key.is_none() && at.is_none() {
        return Err(refused_select(
            "the canvas `click` names neither `key` nor `at`; pass a clip `key` or a film point `at: [x, y]`. Nothing was staged."
                .to_string(),
        ));
    }
    if (action == "toggle" || action == "add") && keys.is_empty() {
        return Err(refused_select(format!(
            "the canvas `{action}` names no `keys`; pass the clip keys to flip or add. Nothing was staged."
        )));
    }
    if action == "marquee" && rect.is_none() {
        return Err(refused_select(
            "the canvas `marquee` names no `rect`; pass the film rect `[x, y, w, h]`. Nothing was staged."
                .to_string(),
        ));
    }
    Ok(SelectRequest {
        action,
        key,
        at,
        keys,
        current,
        rect,
        selection,
        scope,
    })
}

/// Resolve one selection key against the effective clips: stable
/// `id` first, then a `#index` entry. Unknown keys refuse naming
/// the holdings, like the inspector target — an explicit gesture at
/// nothing stages nothing.
fn resolve_key(comp: &notation::model::Composition, key: &str) -> Result<usize, String> {
    if let Some(idx) = comp.clips.iter().position(|c| c.id() == Some(key)) {
        return Ok(idx);
    }
    if let Some(rest) = key.strip_prefix('#') {
        if let Ok(index) = rest.trim().parse::<usize>() {
            if index < comp.clips.len() {
                return Ok(index);
            }
        }
    }
    Err(refused_select(format!(
        "canvas selection key {key:?} names nothing; the open composition holds {} clip(s). Nothing was staged.",
        comp.clips.len()
    )))
}

/// Selection key of one clip: the stable `id` when declared, else
/// the `#index` entry — the same spellings canvas, timeline, and
/// layers sync on.
fn clip_key(comp: &notation::model::Composition, idx: usize) -> String {
    comp.clips[idx]
        .id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("#{idx}"))
}

/// Resolve one selection gesture to the synced selection, in gesture
/// order without duplicates. Point clicks pick the topmost boxed,
/// visible element under the point (paint rank `order`, ties in
/// document order); a miss refuses naming the point. Marquees take
/// every boxed, visible element the rect touches; text, video,
/// audio, and nest clips carry no authored extents, so point and
/// marquee gestures skip them — select those by key, from the
/// readout, or from the timeline. Unknown keys refuse; the echo
/// `selection` stays lenient like the canvas view. Nothing stages.
pub fn resolve_selection(
    comp: &notation::model::Composition,
    req: &SelectRequest,
    film_w: f64,
    film_h: f64,
) -> Result<Vec<String>, String> {
    // An entered group scopes canvas selection to its members (Sprint
    // 18, T004 / CAP-004): unknown scopes refuse naming the holdings;
    // key gestures at out-of-group clips refuse naming the scope,
    // while point and marquee gestures skip them. Nothing stages.
    let scope = match &req.scope {
        None => None,
        Some(name) => resolve_group_scope(comp, name)?,
    };
    let in_scope = |idx: usize| match &scope {
        None => true,
        Some(name) => {
            comp.clips[idx].label().and_then(sdk::group_name_of_label) == Some(name.as_str())
        }
    };
    let check_key_scope = |idx: usize, key: &str| {
        match &scope {
            None => Ok(()),
            Some(name) if in_scope(idx) => Ok(()),
            Some(name) => Err(refused_select(format!(
                "canvas selection key {key:?} sits outside entered group {name:?}; exit the group or pick one of its members. Nothing was staged."
            ))),
        }
    };
    match req.action.as_str() {
        "clear" => Ok(Vec::new()),
        "click" => {
            if let Some(key) = &req.key {
                let idx = resolve_key(comp, key)?;
                check_key_scope(idx, key)?;
                return Ok(vec![clip_key(comp, idx)]);
            }
            let (px, py) = req.at.expect("click carries key or point");
            let mut best: Option<(i64, usize)> = None;
            for (index, clip) in comp.clips.iter().enumerate() {
                if clip.hidden() || !in_scope(index) {
                    continue;
                }
                let Some(b) = element_box_of(clip, film_w, film_h) else {
                    continue;
                };
                if b.contains(px, py)
                    && best.is_none_or(|(order, at)| {
                        (clip.order(), index) > (order, at)
                    })
                {
                    best = Some((clip.order(), index));
                }
            }
            match best {
                Some((_, index)) => Ok(vec![clip_key(comp, index)]),
                None => Err(refused_select(format!(
                    "the canvas click at ({:.0}, {:.0}) names nothing selectable; boxed, visible elements select there, while text, video, audio, nest, and hidden clips select by key, from the readout, or from the timeline. Nothing was staged.",
                    px, py
                ))),
            }
        }
        "toggle" | "add" => {
            let mut out = req.current.clone();
            for key in &req.keys {
                let idx = resolve_key(comp, key)?;
                check_key_scope(idx, key)?;
                let canonical = clip_key(comp, idx);
                if req.action == "add" {
                    if !out.iter().any(|s| s == &canonical) {
                        out.push(canonical);
                    }
                } else if let Some(pos) = out.iter().position(|s| s == &canonical) {
                    out.remove(pos);
                } else {
                    out.push(canonical);
                }
            }
            Ok(out)
        }
        "marquee" => {
            let rect = req.rect.expect("marquee carries a rect");
            // A zero-area marquee is a point click in disguise.
            if rect.w == 0.0 && rect.h == 0.0 {
                let point = SelectRequest {
                    action: "click".to_string(),
                    key: None,
                    at: Some((rect.x, rect.y)),
                    keys: Vec::new(),
                    current: Vec::new(),
                    rect: None,
                    selection: req.selection.clone(),
                    scope: req.scope.clone(),
                };
                return resolve_selection(comp, &point, film_w, film_h);
            }
            let mut hits: Vec<(i64, usize)> = Vec::new();
            for (index, clip) in comp.clips.iter().enumerate() {
                if clip.hidden() || !in_scope(index) {
                    continue;
                }
                let Some(b) = element_box_of(clip, film_w, film_h) else {
                    continue;
                };
                if b.intersects(&rect) {
                    hits.push((clip.order(), index));
                }
            }
            hits.sort();
            Ok(hits.into_iter().map(|(_, i)| clip_key(comp, i)).collect())
        }
        _ => Err(refused_select(format!(
            "unknown canvas selection action {:?}; gesture `click`, `toggle`, `add`, `marquee`, or `clear`. Nothing was staged.",
            req.action
        ))),
    }
}

/// Render one resolved selection as JSON: the action, the synced
/// keys with their boxes, and the lenient `selection` echo
/// (`kept`/`dropped`) so canvas and timeline show the same
/// selection at once. Every key is hyphenated.
pub fn select_json(
    comp: &notation::model::Composition,
    req: &SelectRequest,
    resolved: &[String],
    kept: &[String],
    dropped: &[String],
    film_w: f64,
    film_h: f64,
) -> String {
    let mut boxes = String::new();
    for key in resolved {
        if !boxes.is_empty() {
            boxes.push(',');
        }
        let bounds = comp
            .clips
            .iter()
            .position(|c| c.id() == Some(key.as_str()))
            .or_else(|| {
                key.strip_prefix('#')
                    .and_then(|rest| rest.trim().parse::<usize>().ok())
            })
            .and_then(|idx| {
                comp.clips
                    .get(idx)
                    .and_then(|clip| element_box_of(clip, film_w, film_h))
            });
        let bounds = match bounds {
            Some(b) => format!("[{:.3},{:.3},{:.3},{:.3}]", b.x, b.y, b.w, b.h),
            None => "null".to_string(),
        };
        boxes.push_str(&format!(
            "\"{}\": {bounds}",
            crate::studio::json_escape(key)
        ));
    }
    let resolved_json = resolved
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
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
    format!(
        "{{\"action\":\"{}\",\"keys\":[{resolved_json}],\"boxes\":{{{boxes}}},\"selection\":{{\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]}},\"detail\":\"canvas {} selects {}.\"}}",
        crate::studio::json_escape(&req.action),
        crate::studio::json_escape(&req.action),
        if resolved.is_empty() {
            "nothing".to_string()
        } else {
            resolved.join(", ")
        },
    )
}

/// Axis a gesture locks to: the locked axis moves, the other stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockedAxis {
    /// Horizontal moves only; vertical deltas drop with a note.
    X,
    /// Vertical moves only; horizontal deltas drop with a note.
    Y,
}

impl LockedAxis {
    /// Parse one axis spelling; anything else refuses naming the two.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "x" => Some(LockedAxis::X),
            "y" => Some(LockedAxis::Y),
            _ => None,
        }
    }
}

/// One parsed canvas transform gesture (Sprint 17, T002): a move,
/// resize, nudge, or rotate over explicit targets. Transforms
/// preview by default and stage nothing until the maker commits
/// (`commit: true`); `cancelled` discards the staged preview with a
/// note. Every committed change lands as one named history turn
/// through the shared patch machinery with a receipt.
#[derive(Debug, Clone)]
pub struct TransformRequest {
    /// Actor the committed turn is recorded under (required exactly
    /// while committing — previews record no turn).
    pub actor: String,
    /// Clips the gesture addresses, in gesture order.
    pub targets: Vec<sdk::ClipRef>,
    /// Gesture kind (`move`, `resize`, `nudge`, `rotate`).
    pub action: String,
    /// Move delta in film pixels, when moving.
    pub delta: Option<(f64, f64)>,
    /// Axis the gesture locks to, when locked.
    pub axis: Option<LockedAxis>,
    /// Resize handle, when resizing.
    pub handle: Option<String>,
    /// Resize delta in film pixels, when resizing.
    pub size_delta: Option<(f64, f64)>,
    /// Nudge steps (multiplied by 10 with Shift), when nudging.
    pub nudge: Option<(f64, f64)>,
    /// True while Shift scales the nudge (rotate snaps instead).
    pub shift: bool,
    /// Authored rotation in degrees, when rotating.
    pub degrees: Option<f64>,
    /// True while the staged preview commits as a history turn.
    pub commit: bool,
    /// True while the maker cancelled the staged gesture.
    pub cancelled: bool,
    /// Shared selection the shell scoped the gesture to, echoed back
    /// so canvas and timeline show the same selection at once.
    pub selection: Vec<String>,
}

/// Read one finite transform number, refusing anything else with the
/// key named.
fn transform_number(value: &serde_json::Value, key: &str) -> Result<f64, String> {
    let n = value.as_f64().ok_or_else(|| {
        refused_transform(format!(
            "the canvas transform `{key}` value {value} is not a number; pass finite film pixels or degrees. Nothing was staged."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_transform(format!(
            "the canvas transform `{key}` value {n:?} is not finite; pass finite film pixels or degrees. Nothing was staged."
        )));
    }
    Ok(n)
}

/// Read one film-pixel delta pair (`[dx, dy]`) under a key.
fn transform_delta(value: &serde_json::Value, key: &str) -> Result<(f64, f64), String> {
    let pair = value.as_array().ok_or_else(|| {
        refused_transform(format!(
            "the canvas transform `{key}` value {value} is not a delta; pass `[dx, dy]` film pixels. Nothing was staged."
        ))
    })?;
    if pair.len() != 2 {
        return Err(refused_transform(format!(
            "the canvas transform `{key}` value {value} is not a delta; pass `[dx, dy]` film pixels. Nothing was staged."
        )));
    }
    Ok((
        transform_number(&pair[0], key)?,
        transform_number(&pair[1], key)?,
    ))
}

/// Parse one transform body (`{"actor":"maker","targets":[{"id":
/// "card"}],"action":"move","delta":[10,0],"commit":true}`). The
/// action must name a gesture with its fields; at least one target
/// must ride along; committing needs a non-empty actor. A refused
/// body names its cause and stages nothing.
pub fn parse_transform_body(body: &str) -> Result<TransformRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_transform(
            "the canvas transform needs a JSON transform body (`{\"targets\":[{\"id\":\"card\"}],\"action\":\"move\",\"delta\":[10,0]}`); pass the gesture from the canvas panel. Nothing was staged."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_transform(format!(
            "the canvas transform body is not JSON ({e}). Pass `{{\"targets\":[{{\"id\":\"card\"}}],\"action\":\"move\",\"delta\":[10,0]}}` or another gesture. Nothing was staged."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_transform(
            "the canvas transform body is not a gesture; pass a JSON object with an `action`. Nothing was staged."
                .to_string(),
        ));
    }
    let action = value
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    match action.as_str() {
        "move" | "resize" | "nudge" | "rotate" => {}
        _ => {
            return Err(refused_transform(format!(
                "unknown canvas transform action {action:?}: gesture `move`, `resize`, `nudge`, or `rotate`. Nothing was staged."
            )));
        }
    }
    let targets_value = value.get("targets").ok_or_else(|| {
        refused_transform(
            "the canvas transform carries no `targets`; gestures address clips. Pass `targets:[{\"id\":\"...\"}]` or `targets:[{\"index\":N}]`. Nothing was staged."
                .to_string(),
        )
    })?;
    let targets_list = targets_value.as_array().ok_or_else(|| {
        refused_transform(format!(
            "the canvas transform `targets` value {targets_value} is not a clip list; pass `targets:[{{\"id\":\"...\"}}]` or `targets:[{{\"index\":N}}]`. Nothing was staged."
        ))
    })?;
    if targets_list.is_empty() {
        return Err(refused_transform(
            "the canvas transform `targets` names no clip; gestures address at least one clip. Pass `targets:[{\"id\":\"...\"}]` or `targets:[{\"index\":N}]`. Nothing was staged."
                .to_string(),
        ));
    }
    let mut targets = Vec::new();
    for raw in targets_list {
        let target: sdk::ClipRef = serde_json::from_value(raw.clone()).map_err(|e| {
            refused_transform(format!(
                "the canvas transform target {raw} is not a clip reference ({e}); pass `{{\"id\":\"...\"}}` or `{{\"index\":N}}`. Nothing was staged."
            ))
        })?;
        if target.id.is_none() && target.index.is_none() {
            return Err(refused_transform(format!(
                "the canvas transform target {raw} names neither `id` nor `index`; gestures address clips. Pass `{{\"id\":\"...\"}}` or `{{\"index\":N}}`. Nothing was staged."
            )));
        }
        targets.push(target);
    }
    let axis = match value.get("axis") {
        None => None,
        Some(v) => {
            let raw = v.as_str().ok_or_else(|| {
                refused_transform(format!(
                    "the canvas transform `axis` value {v} is not an axis; lock `x` or `y`. Nothing was staged."
                ))
            })?;
            Some(LockedAxis::parse(raw).ok_or_else(|| {
                refused_transform(format!(
                    "unknown canvas transform axis {raw:?}: lock `x` or `y`. Nothing was staged."
                ))
            })?)
        }
    };
    let handle = match value.get("handle") {
        None => None,
        Some(v) => {
            let raw = v
                .as_str()
                .ok_or_else(|| {
                    refused_transform(format!(
                        "the canvas transform `handle` value {v} is not a handle; grab `left`, `right`, `top`, `bottom`, or a corner. Nothing was staged."
                    ))
                })?
                .trim()
                .to_string();
            match raw.as_str() {
                "left" | "right" | "top" | "bottom" | "top-left" | "top-right" | "bottom-left"
                | "bottom-right" => Some(raw),
                _ => {
                    return Err(refused_transform(format!(
                        "unknown canvas transform handle {raw:?}: grab `left`, `right`, `top`, `bottom`, `top-left`, `top-right`, `bottom-left`, or `bottom-right`. Nothing was staged."
                    )));
                }
            }
        }
    };
    let delta = match value.get("delta") {
        None => None,
        Some(v) => Some(transform_delta(v, "delta")?),
    };
    let size_delta = match value.get("size-delta") {
        None => None,
        Some(v) => Some(transform_delta(v, "size-delta")?),
    };
    let nudge = match value.get("nudge") {
        None => None,
        Some(v) => Some(transform_delta(v, "nudge")?),
    };
    let degrees = match value.get("degrees") {
        None => None,
        Some(v) => Some(transform_number(v, "degrees")?),
    };
    let shift = value
        .get("shift")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    if value.get("shift").is_some() && value.get("shift").and_then(|s| s.as_bool()).is_none() {
        return Err(refused_transform(format!(
            "the canvas transform `shift` value {} is not a switch; pass `true` for the Shift modifier or omit it. Nothing was staged.",
            value.get("shift").expect("shift present")
        )));
    }
    let commit = value
        .get("commit")
        .and_then(|c| c.as_bool())
        .unwrap_or(false);
    if value.get("commit").is_some() && value.get("commit").and_then(|c| c.as_bool()).is_none() {
        return Err(refused_transform(format!(
            "the canvas transform `commit` value {} is not a switch; pass `true` to record the history turn or omit it to preview. Nothing was staged.",
            value.get("commit").expect("commit present")
        )));
    }
    let cancelled = value
        .get("cancelled")
        .and_then(|c| c.as_bool())
        .unwrap_or(false);
    let selection = match value.get("selection") {
        None => Vec::new(),
        Some(v) => parse_selection_value(v, "selection")
            .map_err(|e| e.replace("the canvas selection", "the canvas transform selection"))?,
    };
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    match action.as_str() {
        "move" => {
            if delta.is_none() {
                return Err(refused_transform(
                    "the canvas `move` names no `delta`; pass the drag `[dx, dy]` film pixels. Nothing was staged."
                        .to_string(),
                ));
            }
        }
        "resize" => {
            if targets.len() != 1 {
                return Err(refused_transform(format!(
                    "the canvas `resize` addresses {} clips; resize handles grab one element, while groups move together. Pass one target. Nothing was staged.",
                    targets.len()
                )));
            }
            if handle.is_none() {
                return Err(refused_transform(
                    "the canvas `resize` names no `handle`; grab `left`, `right`, `top`, `bottom`, or a corner. Nothing was staged."
                        .to_string(),
                ));
            }
            if size_delta.is_none() {
                return Err(refused_transform(
                    "the canvas `resize` names no `size-delta`; pass the drag `[dw, dh]` film pixels. Nothing was staged."
                        .to_string(),
                ));
            }
        }
        "nudge" => {
            if nudge.is_none() {
                return Err(refused_transform(
                    "the canvas `nudge` names no `nudge` steps; pass arrow steps like `[1, 0]` (Shift scales them tenfold). Nothing was staged."
                        .to_string(),
                ));
            }
        }
        "rotate" => {
            if degrees.is_none() {
                return Err(refused_transform(
                    "the canvas `rotate` names no `degrees`; pass the handle angle in degrees. Nothing was staged."
                        .to_string(),
                ));
            }
        }
        _ => unreachable!("action checked above"),
    }
    if commit && actor.is_empty() {
        return Err(refused_transform(
            "the canvas transform commit names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged."
                .to_string(),
        ));
    }
    Ok(TransformRequest {
        actor,
        targets,
        action,
        delta,
        axis,
        handle,
        size_delta,
        nudge,
        shift,
        degrees,
        commit,
        cancelled,
        selection,
    })
}

/// Snap one authored rotation to the 15-degree handle grid.
pub fn snap_rotation_deg(degrees: f64) -> f64 {
    (degrees / SNAP_ROTATION_DEG).round() * SNAP_ROTATION_DEG
}

/// Normalize one rotation into (-180, 180] degrees.
pub fn normalize_deg(degrees: f64) -> f64 {
    let mut d = degrees % 360.0;
    if d <= -180.0 {
        d += 360.0;
    } else if d > 180.0 {
        d -= 360.0;
    }
    if d == -0.0 {
        0.0
    } else {
        d
    }
}

/// One proposed picture value: the clip with its new geometry.
#[derive(Debug, Clone)]
pub struct ProposedValue {
    /// Document index of the clip.
    pub index: usize,
    /// Selection key of the clip.
    pub key: String,
    /// New canvas origin in film pixels.
    pub x: f64,
    /// New canvas origin in film pixels.
    pub y: f64,
    /// New width in film pixels, when the gesture sizes.
    pub width: Option<f64>,
    /// New height in film pixels, when the gesture sizes.
    pub height: Option<f64>,
}

/// Resolve one transform target against the effective clips: `id`
/// first, then document index. Unknown targets refuse naming the
/// holdings — a drag with no valid target stages nothing.
fn resolve_target(
    comp: &notation::model::Composition,
    target: &sdk::ClipRef,
) -> Result<usize, String> {
    if let Some(id) = &target.id {
        if let Some(idx) = comp.clips.iter().position(|c| c.id() == Some(id.as_str())) {
            return Ok(idx);
        }
        return Err(refused_transform(format!(
            "canvas transform target clip id {id:?} names nothing; the open composition holds {} clip(s). Nothing was staged.",
            comp.clips.len()
        )));
    }
    if let Some(index) = target.index {
        if index < comp.clips.len() {
            return Ok(index);
        }
        return Err(refused_transform(format!(
            "canvas transform target clip index {index} is past the open composition holding {} clip(s). Nothing was staged.",
            comp.clips.len()
        )));
    }
    Err(refused_transform(
        "the canvas transform target names neither `id` nor `index`; gestures address clips. Nothing was staged."
            .to_string(),
    ))
}

/// Refuse a gesture on a hidden clip: hidden elements show no
/// handles, so the canvas cannot grab them — the locked canvas
/// relationship of this slice. Showing the clip first (the
/// `visible` row) hands the handles back.
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
    refused_transform(format!(
        "canvas transform on hidden {name}{at} refuses: hidden elements show no handles. Show the clip first (the `visible` row), then grab a handle. Nothing was staged."
    ))
}

/// Compute the moved, resized, or nudged values of one gesture
/// against the effective composition without staging anything: the
/// staged preview the panel paints before commit or cancel. Group
/// moves apply the same delta to every target, so offsets survive;
/// an axis lock drops the other delta with a note; a resize past
/// the 1 px floor clamps and names the bound (REC-003). Hidden
/// clips and kinds without the addressed property refuse with the
/// rule and the location before anything stages. Image axes without
/// an authored size resize from the film canvas, like the render
/// target rect.
pub fn propose_transform(
    comp: &notation::model::Composition,
    req: &TransformRequest,
    film_w: f64,
    film_h: f64,
) -> Result<(Vec<ProposedValue>, bool, Vec<String>), String> {
    // Rotation previews the snapped angle without proposing rows:
    // committing refuses (no static rotation row), so callers
    // branch before proposing.
    if req.action == "rotate" {
        return Ok((Vec::new(), false, Vec::new()));
    }
    if req.action == "resize" {
        // Parse guarantees exactly one target with a handle and a
        // size delta for resizes: handles grab one element, while
        // groups move together.
        return propose_resize(comp, req, film_w, film_h);
    }
    let mut notes = Vec::new();
    let mut out = Vec::new();
    for target in &req.targets {
        let idx = resolve_target(comp, target)?;
        let clip = &comp.clips[idx];
        if clip.hidden() {
            return Err(refuse_hidden(clip, idx, target));
        }
        let key = clip_key(comp, idx);
        match req.action.as_str() {
            "move" | "nudge" => {
                let (mut dx, mut dy) = if req.action == "move" {
                    req.delta.expect("move carries a delta")
                } else {
                    let (sx, sy) = req.nudge.expect("nudge carries steps");
                    let scale = if req.shift { NUDGE_SHIFT_PX } else { 1.0 };
                    (sx * scale, sy * scale)
                };
                match req.axis {
                    Some(LockedAxis::X) if dy != 0.0 => {
                        notes.push(format!("axis lock `x` drops the vertical {dy}px on {key}"));
                        dy = 0.0;
                    }
                    Some(LockedAxis::Y) if dx != 0.0 => {
                        notes.push(format!(
                            "axis lock `y` drops the horizontal {dx}px on {key}"
                        ));
                        dx = 0.0;
                    }
                    _ => {}
                }
                let Some((x, y)) = clip_origin(clip) else {
                    return Err(refused_transform(format!(
                        "canvas {} on {key} ({} clip) refuses: `{}` writes placed clips (text, box, image, svg, path, flex) only; this {} carries no canvas position and keeps its prior value. Nothing was staged.",
                        req.action,
                        clip.what(),
                        if req.action == "move" { "move" } else { "nudge" },
                        clip.what(),
                    )));
                };
                out.push(ProposedValue {
                    index: idx,
                    key,
                    x: x + dx,
                    y: y + dy,
                    width: None,
                    height: None,
                });
            }
            _ => {
                return Err(refused_transform(format!(
                    "unknown canvas transform action {:?}; gesture `move`, `resize`, `nudge`, or `rotate`. Nothing was staged.",
                    req.action
                )));
            }
        }
    }
    Ok((out, false, notes))
}

/// Compute one resize from its handle: the grabbed edge follows the
/// drag while the opposite edge stays, corners move both axes. A
/// result past the 1 px floor clamps there and names the bound
/// (REC-003) instead of staging a refused non-positive size.
fn propose_resize(
    comp: &notation::model::Composition,
    req: &TransformRequest,
    film_w: f64,
    film_h: f64,
) -> Result<(Vec<ProposedValue>, bool, Vec<String>), String> {
    let mut notes = Vec::new();
    let mut clamped = false;
    let target = &req.targets[0];
    let idx = resolve_target(comp, target)?;
    let clip = &comp.clips[idx];
    if clip.hidden() {
        return Err(refuse_hidden(clip, idx, target));
    }
    let key = clip_key(comp, idx);
    let handle = req.handle.clone().expect("resize carries a handle");
    let (mut dw, mut dh) = req.size_delta.expect("resize carries size-delta");
    match req.axis {
        Some(LockedAxis::X) if dh != 0.0 => {
            notes.push(format!("axis lock `x` drops the vertical {dh}px on {key}"));
            dh = 0.0;
        }
        Some(LockedAxis::Y) if dw != 0.0 => {
            notes.push(format!(
                "axis lock `y` drops the horizontal {dw}px on {key}"
            ));
            dw = 0.0;
        }
        _ => {}
    }
    let Some((ox, oy)) = clip_origin(clip) else {
        return Err(refused_transform(format!(
            "canvas resize on {key} ({} clip) refuses: `width`/`height` write sized clips (`box`, `image`, `svg`, `flex`) only; this {} keeps its prior size. Nothing was staged.",
            clip.what(),
            clip.what(),
        )));
    };
    let size = clip_size(clip).ok_or_else(|| {
        refused_transform(format!(
            "canvas resize on {key} ({} clip) refuses: `width`/`height` write sized clips (`box`, `image`, `svg`, `flex`) only; this {} keeps its prior size. Nothing was staged.",
            clip.what(),
            clip.what(),
        ))
    })?;
    let start_w = size.0.unwrap_or(film_w);
    let start_h = size.1.unwrap_or(film_h);
    let (mut x, mut y) = (ox, oy);
    let (mut w, mut h) = (start_w, start_h);
    if handle.contains("left") {
        w = start_w - dw;
        x = ox + start_w - w;
    } else if handle.contains("right") {
        w = start_w + dw;
    }
    if handle.contains("top") {
        h = start_h - dh;
        y = oy + start_h - h;
    } else if handle.contains("bottom") {
        h = start_h + dh;
    }
    if w < MIN_CANVAS_SIZE_PX {
        clamped = true;
        w = MIN_CANVAS_SIZE_PX;
        if handle.contains("left") {
            x = ox + start_w - w;
        }
        notes.push(format!(
            "resize width on {key} clamped to the {MIN_CANVAS_SIZE_PX:.0}px floor"
        ));
    }
    if h < MIN_CANVAS_SIZE_PX {
        clamped = true;
        h = MIN_CANVAS_SIZE_PX;
        if handle.contains("top") {
            y = oy + start_h - h;
        }
        notes.push(format!(
            "resize height on {key} clamped to the {MIN_CANVAS_SIZE_PX:.0}px floor"
        ));
    }
    Ok((
        vec![ProposedValue {
            index: idx,
            key,
            x,
            y,
            width: Some(w),
            height: Some(h),
        }],
        clamped,
        notes,
    ))
}

/// Previewed rotation of one rotate gesture: the authored angle, or
/// the 15-degree snap with Shift, normalized into (-180, 180].
/// Returns the painted angle with whether a snap caught it.
pub fn preview_rotate(degrees: f64, shift: bool) -> (f64, bool) {
    if shift {
        (normalize_deg(snap_rotation_deg(degrees)), true)
    } else {
        (normalize_deg(degrees), false)
    }
}

/// Refuse a rotate commit: static rotation has no render-supported
/// row — `rotation` rides tween/key cards (T008) — so the drag
/// previews the snapped angle and committing stages nothing. Target
/// and hidden checks run first, so a rotate at nothing still names
/// the missing target.
pub fn rotate_commit_refusal(
    comp: &notation::model::Composition,
    req: &TransformRequest,
    angle: f64,
    snapped: bool,
) -> String {
    for target in &req.targets {
        match resolve_target(comp, target) {
            Err(why) => return why,
            Ok(idx) => {
                let clip = &comp.clips[idx];
                if clip.hidden() {
                    return refuse_hidden(clip, idx, target);
                }
            }
        }
    }
    let keys = req
        .targets
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let snap_note = if snapped { " (snapped to 15°)" } else { "" };
    refused_transform(format!(
        "canvas rotate of {keys} to {angle}°{snap_note} refuses: static rotation has no render-supported row — `rotation` is motion-driven and rides tween/key cards. The staged angle previews only. Nothing was applied or recorded."
    ))
}

/// Build the SDK patch operations of one proposed gesture: `x`/`y`
/// rows for moves and nudges, plus `width`/`height` rows for
/// resizes — in gesture target order, so one turn reads in drag
/// order.
pub fn transform_edit_ops(req: &TransformRequest, proposed: &[ProposedValue]) -> Vec<sdk::EditOp> {
    let mut ops = Vec::new();
    for (target, value) in req.targets.iter().zip(proposed.iter()) {
        ops.push(sdk::EditOp::SetRow {
            target: target.clone(),
            property: "x".to_string(),
            value: num_text(value.x),
        });
        ops.push(sdk::EditOp::SetRow {
            target: target.clone(),
            property: "y".to_string(),
            value: num_text(value.y),
        });
        if req.action == "resize" {
            ops.push(sdk::EditOp::SetRow {
                target: target.clone(),
                property: "width".to_string(),
                value: num_text(value.width.expect("resize proposes a width")),
            });
            ops.push(sdk::EditOp::SetRow {
                target: target.clone(),
                property: "height".to_string(),
                value: num_text(value.height.expect("resize proposes a height")),
            });
        }
    }
    ops
}

/// One applied canvas transform turn.
#[derive(Debug, Clone)]
pub struct TransformReceipt {
    /// Patch operations the turn recorded.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Apply one proposed gesture through the Sprint 15 patch and
/// history machinery: build the row ops and record one validated
/// turn under the actor — the same undo system as row, timeline,
/// and headless edits. A refused value names its rule and location
/// while the composition, the delta, and history stay exactly as
/// they were.
pub fn apply_transform(
    folder: &Path,
    composition: Option<&str>,
    req: &TransformRequest,
    proposed: &[ProposedValue],
) -> Result<TransformReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_transform(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let ops = transform_edit_ops(req, proposed);
    let studio_edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &studio_edit) {
        Ok(receipt) => Ok(TransformReceipt {
            applied: receipt.applied,
            turn: receipt.turn,
            actor: receipt.actor,
            summaries: receipt.summaries,
        }),
        Err(message) => Err(message),
    }
}

/// One-line gesture detail for receipts and the status line.
fn gesture_detail(
    req: &TransformRequest,
    proposed: &[ProposedValue],
    clamped: bool,
    notes: &[String],
    rotate: Option<(f64, bool)>,
) -> String {
    let keys = proposed
        .iter()
        .map(|v| v.key.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let mut detail = match req.action.as_str() {
        "move" => {
            let (dx, dy) = req.delta.expect("move carries a delta");
            format!("canvas move {keys} by ({dx}, {dy})px")
        }
        "nudge" => {
            let (sx, sy) = req.nudge.expect("nudge carries steps");
            let scale = if req.shift { NUDGE_SHIFT_PX } else { 1.0 };
            let shift_note = if req.shift { " (Shift x10)" } else { "" };
            format!(
                "canvas nudge {keys} by ({}, {})px{shift_note}",
                sx * scale,
                sy * scale
            )
        }
        "resize" => {
            let handle = req.handle.clone().unwrap_or_default();
            let value = &proposed[0];
            format!(
                "canvas resize {} from the {handle} to {}x{}px",
                keys,
                num_text(value.width.expect("resize proposes a width")),
                num_text(value.height.expect("resize proposes a height")),
            )
        }
        "rotate" => {
            let (angle, snapped) = rotate.expect("rotate previews an angle");
            let snap_note = if snapped { ", snapped to 15°" } else { "" };
            format!(
                "canvas rotate {} to {angle}°{snap_note}",
                req.targets
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        _ => format!("canvas {} {keys}", req.action),
    };
    if clamped {
        detail.push_str(" (clamped to the 1px floor)");
    }
    for note in notes {
        detail.push_str("; ");
        detail.push_str(note);
    }
    detail
}

/// Render applied values as JSON: the new geometry per target.
fn applied_values_json(proposed: &[ProposedValue], rotate: Option<(f64, bool)>) -> String {
    if let Some((angle, snapped)) = rotate {
        return format!("{{\"angle\":{angle},\"snapped\":{snapped}}}");
    }
    let mut items = String::new();
    for value in proposed {
        if !items.is_empty() {
            items.push(',');
        }
        let width = match value.width {
            Some(w) => num_text(w),
            None => "null".to_string(),
        };
        let height = match value.height {
            Some(h) => num_text(h),
            None => "null".to_string(),
        };
        items.push_str(&format!(
            "{{\"target\":\"{}\",\"x\":{},\"y\":{},\"width\":{width},\"height\":{height}}}",
            crate::studio::json_escape(&value.key),
            num_text(value.x),
            num_text(value.y),
        ));
    }
    format!("[{items}]")
}

/// Shared selection echo shared by every transform answer, so
/// canvas and timeline show the same selection at once.
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

/// Render one staged transform preview as JSON: the gesture with
/// its proposed values, the clamp flag, and the lenient selection
/// echo. Nothing staged, no turn recorded. Every key is hyphenated.
pub fn transform_preview_json(
    req: &TransformRequest,
    proposed: &[ProposedValue],
    clamped: bool,
    notes: &[String],
    rotate: Option<(f64, bool)>,
    kept: &[String],
    dropped: &[String],
) -> String {
    let detail = gesture_detail(req, proposed, clamped, notes, rotate);
    format!(
        "{{\"staged\":false,\"committed\":false,\"cancelled\":false,\"action\":\"{}\",\"actor\":\"{}\",\"applied-values\":{},\"clamped\":{},\"notes\":[{}],\"detail\":\"{}\",\"turn\":null,\"summaries\":[],\"selection\":{}}}",
        req.action,
        crate::studio::json_escape(&req.actor),
        applied_values_json(proposed, rotate),
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

/// Render one committed transform receipt as JSON: the named turn
/// with its summaries, the applied values, and the synced
/// selection echo. Every key is hyphenated.
pub fn transform_receipt_json(
    req: &TransformRequest,
    receipt: &TransformReceipt,
    proposed: &[ProposedValue],
    clamped: bool,
    notes: &[String],
    kept: &[String],
    dropped: &[String],
) -> String {
    let detail = gesture_detail(req, proposed, clamped, notes, None);
    let summaries = receipt
        .summaries
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"staged\":false,\"committed\":true,\"cancelled\":false,\"saved\":true,\"action\":\"{}\",\"actor\":\"{}\",\"applied\":{},\"turn\":{},\"summaries\":[{summaries}],\"applied-values\":{},\"clamped\":{},\"detail\":\"{} (turn {})\",\"selection\":{}}}",
        req.action,
        crate::studio::json_escape(&receipt.actor),
        receipt.applied,
        receipt.turn,
        applied_values_json(proposed, None),
        clamped,
        crate::studio::json_escape(&detail),
        receipt.turn,
        selection_echo_json(kept, dropped),
    )
}

/// Render one cancelled transform as JSON: the staged preview
/// discards, nothing commits, and the answer says so.
pub fn transform_cancelled_json(
    req: &TransformRequest,
    kept: &[String],
    dropped: &[String],
) -> String {
    format!(
        "{{\"staged\":false,\"committed\":false,\"cancelled\":true,\"action\":\"{}\",\"detail\":\"canvas {} cancelled; the staged preview discarded and nothing was recorded.\",\"selection\":{}}}",
        req.action,
        crate::studio::json_escape(&req.action),
        selection_echo_json(kept, dropped),
    )
}

/// Extra CSS the selection slice needs (Sprint 17, T002): the
/// layers readout with its z-order list and off-canvas badges, the
/// nudge pad, and the transform status line. Style attributes stay
/// CSS-like hyphenated end to end (`background-color`,
/// `border-radius`, `z-index`).
pub fn select_css() -> String {
    r#"        #canvas-layers-bar{margin-top:8px}
        #canvas-layers{list-style:none;margin:4px 0;padding:0;max-height:20vh;overflow:auto;background-color:var(--panel-color);border:1px solid var(--line-color);border-radius:4px}
        #canvas-layers li{padding:2px 8px;font-variant-numeric:tabular-nums;cursor:pointer}
        #canvas-layers li[data-selected="1"]{background-color:var(--accent-color)}
        #canvas-layers li[data-off="1"]{border-left:3px solid var(--accent-color)}
        #canvas-layers li[data-dim="1"]{opacity:0.45}
        #canvas-layers li.group-head{font-weight:bold;cursor:pointer}
        #canvas-layers .off-badge{display:inline-block;margin-left:8px;border:1px solid var(--line-color);border-radius:4px;padding:0 6px}
        #canvas-layers .group-badge{display:inline-block;margin-left:8px;border:1px solid var(--line-color);border-radius:4px;padding:0 6px}
        #canvas-layers-bar input{margin-left:8px}
        #canvas-layers-bar button{margin-left:4px}
        #canvas-scope{margin:4px 0;font-variant-numeric:tabular-nums}
        #canvas-nudge{margin-top:8px;display:flex;gap:4px;align-items:center}
        #canvas-nudge button{margin-right:4px}
        #canvas-transform-status{font-variant-numeric:tabular-nums;z-index:1}"#.to_string()
}

/// The selection control block (Sprint 17, T002 readout; Sprint 18,
/// T004 full panel): the layers tree listing elements by z-order
/// with kind icons, group headers, and drag reorder, the group
/// collect/release/enter/exit controls, the nudge pad (Shift scales
/// tenfold), and the transform status line.
pub fn select_html() -> String {
    r#"<div class="dock-bar" id="canvas-layers-bar">
        <span>layers</span>
        <input id="canvas-group-name" type="text" placeholder="group name" aria-label="group name">
        <button id="canvas-group" type="button" title="collect the selection into a group">group</button>
        <button id="canvas-ungroup" type="button" title="release the named group">ungroup</button>
        <button id="canvas-enter" type="button" title="scope selection to the named group">enter</button>
        <button id="canvas-exit" type="button" title="clear the group scope">exit</button>
        <ul id="canvas-layers"></ul>
        <p id="canvas-scope">scope: all</p>
        </div>
        <div id="canvas-nudge">
        <button id="canvas-nudge-left" type="button" title="nudge left 1px (Shift: 10px)">&#8592;</button>
        <button id="canvas-nudge-right" type="button" title="nudge right 1px (Shift: 10px)">&#8594;</button>
        <button id="canvas-nudge-up" type="button" title="nudge up 1px (Shift: 10px)">&#8593;</button>
        <button id="canvas-nudge-down" type="button" title="nudge down 1px (Shift: 10px)">&#8595;</button>
        </div>
        <p id="canvas-transform-status" role="status">no transform staged</p>"#.to_string()
}

/// The selection panel script (Sprint 17, T002 readout; Sprint 18,
/// T004 full panel): paints the layers tree with kind icons, group
/// headers, and drag reorder from the z-order API, collects and
/// releases groups, enters and exits group scope, selects from the
/// readout, nudges the shared selection (Shift scales tenfold), and
/// keeps the canvas and timeline selections in sync through the
/// shared `selection` array.
pub fn select_js() -> String {
    r#"        function transformStatus(t) { const s = document.getElementById('canvas-transform-status'); if (s) s.textContent = t; }
        let layerScope = null;
        let lastSeen = [];
        async function paintLayers() {
          const box = document.getElementById('canvas-layers');
          if (!box) return;
          try {
            const q = new URLSearchParams();
            q.set('composition', (rows && rows.composition) || boot.composition);
            if (selection.length) q.set('selection', selection.join(','));
            if (layerScope) q.set('scope', layerScope);
            const r = await fetch('/api/studio/layers?' + q.toString());
            if (!r.ok) { box.textContent = await r.text(); return; }
            const j = await r.json();
            const scopeLine = document.getElementById('canvas-scope');
            if (scopeLine) scopeLine.textContent = 'scope: ' + (j.scope || 'all');
            box.innerHTML = '';
            for (const group of (j.groups || [])) {
              const head = document.createElement('li');
              head.className = 'group-head';
              head.textContent = '⧉ ' + group.name + ' (' + group.members.length + ')';
              head.title = 'enter group ' + group.name;
              head.addEventListener('click', async () => {
                await postLayers({action: 'enter', name: group.name});
              });
              box.appendChild(head);
            }
            const seen = j.layers || [];
            lastSeen = seen;
            for (const layer of seen) {
              const li = document.createElement('li');
              li.textContent = (layer.icon || layer.kind) + ' ' + layer.key + ' z:' + layer.order + (layer.hidden ? ' hidden' : '');
              li.dataset.selected = layer.selected ? '1' : '0';
              li.dataset.off = ((layer['off-canvas'] || []).length) ? '1' : '0';
              li.dataset.dim = (layer['in-scope'] === false) ? '1' : '0';
              li.draggable = true;
              if (layer.group) {
                const badge = document.createElement('span');
                badge.className = 'group-badge';
                badge.textContent = 'group: ' + layer.group;
                li.appendChild(badge);
              }
              if ((layer['off-canvas'] || []).length) {
                const badge = document.createElement('span');
                badge.className = 'off-badge';
                badge.textContent = 'off-canvas: ' + layer['off-canvas'].join(',');
                li.appendChild(badge);
              }
              li.addEventListener('click', async (e) => {
                const keys = e.shiftKey ? selection.concat([layer.key]) : [layer.key];
                const body = {action: e.shiftKey ? 'add' : 'click', keys: keys, current: selection};
                if (layerScope) body.scope = layerScope;
                await postSelect(body);
              });
              li.addEventListener('dragstart', (e) => {
                e.dataTransfer.setData('text/studio-layer', layer.key);
              });
              li.addEventListener('dragover', (e) => e.preventDefault());
              li.addEventListener('drop', async (e) => {
                e.preventDefault();
                const dragged = e.dataTransfer.getData('text/studio-layer');
                if (!dragged || dragged === layer.key) return;
                const draggedRow = seen.find((l) => l.key === dragged);
                if (!draggedRow) return;
                const targetDoc = layer.index;
                const to = (draggedRow.index < targetDoc) ? Math.max(0, targetDoc - 1) : targetDoc;
                if (draggedRow.index === to && draggedRow.order === layer.order) return;
                await postLayers({action: 'reorder', key: dragged, to: to, order: layer.order});
              });
              box.appendChild(li);
            }
            if (!box.dataset.dropWired) {
              box.dataset.dropWired = '1';
              box.addEventListener('dragover', (e) => e.preventDefault());
              box.addEventListener('drop', async (e) => {
                e.preventDefault();
                if (e.target !== box || !lastSeen.length) return;
                const dragged = e.dataTransfer.getData('text/studio-layer');
                if (!dragged) return;
                const last = lastSeen[lastSeen.length - 1];
                if (dragged === last.key) return;
                const draggedRow = lastSeen.find((l) => l.key === dragged);
                if (!draggedRow) return;
                if (draggedRow.index === lastSeen.length - 1 && draggedRow.order === last.order) return;
                await postLayers({action: 'reorder', key: dragged, to: lastSeen.length - 1, order: last.order});
              });
            }
            transformStatus(j['status-line'] || 'layers ready');
          } catch (e) { transformStatus('layers unavailable: ' + e); }
        }
        async function postLayers(payload) {
          try {
            const body = Object.assign({actor: 'maker', selection: selection.join(',')}, payload);
            const r = await fetch('/api/studio/layers', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
            const t = await r.text();
            if (!r.ok) { transformStatus(t); banner(t); return; }
            const j = JSON.parse(t);
            if (j.selection && j.selection.kept) selection = j.selection.kept.slice();
            if (j.action === 'enter') layerScope = j.scope || null;
            if (j.action === 'exit') layerScope = null;
            transformStatus((j.detail || 'layers ready') + ((j.turn !== undefined && j.turn !== null) ? ' (turn ' + j.turn + ')' : ''));
            paintRows(); paintCanvasSelection(); syncLink(); paintLayers(); show(cur);
          } catch (e) { transformStatus('layers failed: ' + e); }
        }
        function groupName() { const el = document.getElementById('canvas-group-name'); return el ? el.value.trim() : ''; }
        function wireLayers(id, fn) {
          const el = document.getElementById(id);
          if (el && !el.dataset.wired) {
            el.dataset.wired = '1';
            el.addEventListener('click', fn);
          }
        }
        wireLayers('canvas-group', () => postLayers({action: 'group', keys: selection.slice(), name: groupName()}));
        wireLayers('canvas-ungroup', () => postLayers({action: 'ungroup', name: groupName()}));
        wireLayers('canvas-enter', () => postLayers({action: 'enter', name: groupName()}));
        wireLayers('canvas-exit', () => postLayers({action: 'exit'}));
        async function postSelect(payload) {
          try {
            const r = await fetch('/api/studio/select', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(payload)});
            const t = await r.text();
            if (!r.ok) { transformStatus(t); banner(t); return; }
            const j = JSON.parse(t);
            selection = (j.keys || []).slice();
            transformStatus(j.detail || 'selection ready');
            paintRows(); paintCanvasSelection(); syncLink(); paintLayers();
          } catch (e) { transformStatus('select failed: ' + e); }
        }
        async function postTransform(payload) {
          try {
            const body = Object.assign({actor: 'maker', commit: true, selection: selection.join(',')}, payload);
            const r = await fetch('/api/studio/transform', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
            const t = await r.text();
            if (!r.ok) { transformStatus(t); banner(t); return; }
            const j = JSON.parse(t);
            if (j.selection && j.selection.kept) selection = j.selection.kept.slice();
            transformStatus((j.detail || 'transform ready') + (j.turn ? ' (turn ' + j.turn + ')' : ''));
            paintRows(); paintCanvasSelection(); syncLink(); paintLayers(); show(cur);
          } catch (e) { transformStatus('transform failed: ' + e); }
        }
        function wireNudge(id, dx, dy) {
          const el = document.getElementById(id);
          if (el && !el.dataset.wired) {
            el.dataset.wired = '1';
            el.addEventListener('click', (e) => {
              const targets = selection.map((s) => (/^#(\d+)$/.test(s) ? {index: Number(s.slice(1))} : {id: s}));
              if (!targets.length) { transformStatus('nudge names no target; select an element first.'); return; }
              postTransform({action: 'nudge', targets: targets, nudge: [dx, dy], shift: !!e.shiftKey});
            });
          }
        }
        wireNudge('canvas-nudge-left', -1, 0);
        wireNudge('canvas-nudge-right', 1, 0);
        wireNudge('canvas-nudge-up', 0, -1);
        wireNudge('canvas-nudge-down', 0, 1);"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_comp() -> notation::model::Composition {
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

    #[test]
    fn select_rotation_snaps_to_15_degrees_with_shift() {
        assert_eq!(snap_rotation_deg(22.0), 15.0);
        assert_eq!(snap_rotation_deg(23.0), 30.0);
        assert_eq!(snap_rotation_deg(-7.0), 0.0);
        assert_eq!(snap_rotation_deg(-8.0), -15.0);
        let (angle, snapped) = preview_rotate(22.0, true);
        assert_eq!((angle, snapped), (15.0, true));
        let (exact, plain) = preview_rotate(22.0, false);
        assert_eq!((exact, plain), (22.0, false));
        assert_eq!(normalize_deg(190.0), -170.0);
        assert_eq!(normalize_deg(-190.0), 170.0);
        assert_eq!(normalize_deg(360.0), 0.0);
    }

    #[test]
    fn select_element_boxes_read_authored_extents() {
        let comp = test_comp();
        let card = element_box_of(&comp.clips[0], 320.0, 180.0).expect("box has extents");
        assert_eq!(
            card,
            ElementBox {
                x: 10.0,
                y: 20.0,
                w: 100.0,
                h: 50.0
            }
        );
        assert!(card.contains(10.0, 20.0));
        assert!(card.contains(110.0, 70.0));
        assert!(!card.contains(111.0, 70.0));
        let far = ElementBox {
            x: 400.0,
            y: 20.0,
            w: 100.0,
            h: 50.0,
        };
        assert_eq!(off_canvas_sides(&far, 320.0, 180.0), vec!["right"]);
        let above = ElementBox {
            x: 10.0,
            y: -60.0,
            w: 100.0,
            h: 50.0,
        };
        assert_eq!(off_canvas_sides(&above, 320.0, 180.0), vec!["top"]);
        assert!(off_canvas_sides(&card, 320.0, 180.0).is_empty());
    }

    #[test]
    fn select_axis_lock_drops_the_other_delta_with_a_note() {
        let comp = test_comp();
        let req = TransformRequest {
            actor: String::new(),
            targets: vec![sdk::ClipRef::by_id("card")],
            action: "move".to_string(),
            delta: Some((10.0, 5.0)),
            axis: Some(LockedAxis::X),
            handle: None,
            size_delta: None,
            nudge: None,
            shift: false,
            degrees: None,
            commit: false,
            cancelled: false,
            selection: Vec::new(),
        };
        let (proposed, clamped, notes) = propose_transform(&comp, &req, 320.0, 180.0).unwrap();
        assert!(!clamped);
        assert_eq!((proposed[0].x, proposed[0].y), (20.0, 20.0));
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].contains("axis lock"),
            "note names the lock: {}",
            notes[0]
        );
    }

    #[test]
    fn select_resize_clamps_to_the_1px_floor() {
        let comp = test_comp();
        let req = TransformRequest {
            actor: String::new(),
            targets: vec![sdk::ClipRef::by_id("card")],
            action: "resize".to_string(),
            delta: None,
            axis: None,
            handle: Some("left".to_string()),
            size_delta: Some((200.0, 0.0)),
            nudge: None,
            shift: false,
            degrees: None,
            commit: false,
            cancelled: false,
            selection: Vec::new(),
        };
        let (proposed, clamped, _) = propose_transform(&comp, &req, 320.0, 180.0).unwrap();
        assert!(clamped);
        assert_eq!(proposed[0].width, Some(1.0));
        // The grabbed edge follows the drag; the opposite edge stays.
        assert_eq!(proposed[0].x, 10.0 + 100.0 - 1.0);
    }

    #[test]
    fn select_hidden_clips_refuse_with_the_locked_cause() {
        let mut comp = test_comp();
        if let notation::model::Clip::Box(c) = &mut comp.clips[0] {
            c.hidden = true;
        }
        let req = TransformRequest {
            actor: "maker".to_string(),
            targets: vec![sdk::ClipRef::by_id("card")],
            action: "move".to_string(),
            delta: Some((1.0, 0.0)),
            axis: None,
            handle: None,
            size_delta: None,
            nudge: None,
            shift: false,
            degrees: None,
            commit: true,
            cancelled: false,
            selection: Vec::new(),
        };
        let why = propose_transform(&comp, &req, 320.0, 180.0).expect_err("hidden refuses");
        assert!(why.contains("no handles"), "names the lock: {why}");
    }

    /// Two boxes sharing authored geometry: `card` (`order` 2,
    /// note-free) and `panel` (`order` 0, grouped in `g`).
    fn two_box_comp() -> notation::model::Composition {
        use notation::model::Clip;
        let mut comp = test_comp();
        let mut panel = match comp.clips[0].clone() {
            Clip::Box(b) => b,
            _ => unreachable!("fixture opens with a box"),
        };
        panel.id = Some("panel".to_string());
        panel.order = 0;
        panel.label = Some("group:g".to_string());
        comp.clips.push(Clip::Box(panel));
        comp
    }

    #[test]
    fn layers_kind_icons_cover_every_kind() {
        assert_eq!(layer_icon("text"), "T");
        assert_eq!(layer_icon("video"), "▶");
        assert_eq!(layer_icon("box"), "■");
        assert_eq!(layer_icon("flex"), "▤");
        assert_eq!(layer_icon("image"), "◈");
        assert_eq!(layer_icon("path"), "✎");
        assert_eq!(layer_icon("svg"), "S");
        assert_eq!(layer_icon("audio"), "♪");
        assert_eq!(layer_icon("nest"), "⧉");
        assert_eq!(layer_icon("mystery"), "?");
    }

    #[test]
    fn layers_groups_collect_members_in_paint_order() {
        let comp = two_box_comp();
        let groups = groups_of(&comp);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "g");
        assert_eq!(groups[0].members, vec!["panel".to_string()]);
        // Grouping the topmost box too lists members in paint order.
        let mut both = comp.clone();
        if let notation::model::Clip::Box(c) = &mut both.clips[0] {
            c.label = Some("group:g".to_string());
        }
        let groups = groups_of(&both);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].members,
            vec!["panel".to_string(), "card".to_string()],
            "paint order, not document order"
        );
    }

    #[test]
    fn layers_scope_marks_entries_and_rejects_unknown() {
        let comp = two_box_comp();
        let scoped = layers_of(&comp, 320.0, 180.0, &[], Some("g"));
        let panel = scoped.iter().find(|e| e.key == "panel").unwrap();
        assert!(panel.in_scope);
        assert_eq!(panel.group.as_deref(), Some("g"));
        assert_eq!(panel.icon, "■");
        let card = scoped.iter().find(|e| e.key == "card").unwrap();
        assert!(!card.in_scope);
        assert_eq!(card.group, None);
        // No scope marks everything in scope.
        assert!(layers_of(&comp, 320.0, 180.0, &[], None)
            .iter()
            .all(|e| e.in_scope));
        // Unknown scopes refuse naming the holdings.
        let why = resolve_group_scope(&comp, "nope").expect_err("unknown scope refuses");
        assert!(why.contains("no layer group"), "names the failure: {why}");
        assert!(why.contains("\"g\""), "names the holdings: {why}");
        assert_eq!(
            resolve_group_scope(&comp, "  ").expect("blank clears"),
            None
        );
        let bare = test_comp();
        let why = resolve_group_scope(&bare, "nope").expect_err("groupless refuses");
        assert!(why.contains("holds no layer groups"), "{why}");
    }

    #[test]
    fn layers_parse_accepts_panel_gestures_and_refuses_garbage() {
        let req = parse_layers_body(
            r#"{"actor":"maker","action":"reorder","key":"card","to":0,"order":2}"#,
        )
        .expect("reorder parses");
        assert_eq!(req.action, "reorder");
        assert_eq!(req.key.as_deref(), Some("card"));
        assert_eq!(req.to, Some(0));
        assert_eq!(req.order, Some(2));
        let req =
            parse_layers_body(r#"{"actor":"maker","action":"group","keys":["a","b"],"name":"g"}"#)
                .expect("group parses");
        assert_eq!(req.keys, vec!["a".to_string(), "b".to_string()]);
        for body in [
            r#"{"actor":"maker","action":"enter","name":"g"}"#,
            r#"{"actor":"maker","action":"exit"}"#,
            r#"{"actor":"maker","action":"ungroup","name":"g"}"#,
        ] {
            parse_layers_body(body).expect("view and release actions parse");
        }
        // Unknown actions, missing fields, and bad numbers refuse.
        let why = parse_layers_body(r#"{"actor":"maker","action":"fly"}"#)
            .expect_err("unknown action refuses");
        assert!(why.contains("unknown layers action"), "{why}");
        let why = parse_layers_body(r#"{"actor":"maker","action":"reorder","to":0}"#)
            .expect_err("missing key refuses");
        assert!(why.contains("names no `key`"), "{why}");
        let why = parse_layers_body(r#"{"action":"reorder","key":"card","to":0}"#)
            .expect_err("missing actor refuses");
        assert!(why.contains("names no actor"), "{why}");
        let why = parse_layers_body(r#"{"actor":"maker","action":"reorder","key":"card","to":-1}"#)
            .expect_err("negative index refuses");
        assert!(why.contains("not a document index"), "{why}");
        let why = parse_layers_body(
            r#"{"actor":"maker","action":"reorder","key":"card","to":0,"order":1.5}"#,
        )
        .expect_err("fractional order refuses");
        assert!(why.contains("not a paint order"), "{why}");
        let why = parse_layers_body(r#"{"actor":"maker","action":"group","keys":["a"]}"#)
            .expect_err("missing name refuses");
        assert!(why.contains("names no `name`"), "{why}");
        let why = parse_layers_body(r#"{"actor":"maker","action":"group","keys":[7],"name":"g"}"#)
            .expect_err("non-string key refuses");
        assert!(why.contains("the layers `keys`"), "{why}");
    }

    #[test]
    fn layers_edit_ops_builds_reorder_group_and_ungroup() {
        let comp = two_box_comp();
        // A same-band drag moves the document index alone.
        let req = parse_layers_body(r#"{"actor":"maker","action":"reorder","key":"card","to":1}"#)
            .expect("reorder parses");
        let ops = layers_edit_ops(&req, &comp)
            .expect("ops build")
            .expect("commits");
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            sdk::EditOp::MoveElement { target, to } => {
                assert_eq!(target, &sdk::ClipRef::by_id("card"));
                assert_eq!(*to, 1);
            }
            other => panic!("same-band drag moves alone: {other:?}"),
        }
        // A cross-band drag carries the drop row's paint order first.
        let req = parse_layers_body(
            r#"{"actor":"maker","action":"reorder","key":"card","to":0,"order":0}"#,
        )
        .expect("reorder parses");
        let ops = layers_edit_ops(&req, &comp)
            .expect("ops build")
            .expect("commits");
        assert_eq!(ops.len(), 2);
        match &ops[0] {
            sdk::EditOp::SetRow {
                target,
                property,
                value,
            } => {
                assert_eq!(target, &sdk::ClipRef::by_id("card"));
                assert_eq!(property, "z-index");
                assert_eq!(value, "0");
            }
            other => panic!("cross-band drag assigns order first: {other:?}"),
        }
        // A matching order stays untouched.
        let req = parse_layers_body(
            r#"{"actor":"maker","action":"reorder","key":"panel","to":0,"order":0}"#,
        )
        .expect("reorder parses");
        let ops = layers_edit_ops(&req, &comp)
            .expect("ops build")
            .expect("commits");
        assert_eq!(ops.len(), 1, "matching order skips the row op");
        // An empty group stages nothing.
        let req = parse_layers_body(r#"{"actor":"maker","action":"group","name":"g"}"#)
            .expect("empty group parses");
        assert!(
            layers_edit_ops(&req, &comp)
                .expect("empty group builds")
                .is_none(),
            "empty grouping stages nothing"
        );
        // Group and ungroup map to the SDK ops.
        let req =
            parse_layers_body(r#"{"actor":"maker","action":"group","keys":["card"],"name":"h"}"#)
                .expect("group parses");
        let ops = layers_edit_ops(&req, &comp)
            .expect("ops build")
            .expect("commits");
        match &ops[0] {
            sdk::EditOp::GroupElements { targets, name } => {
                assert_eq!(targets, &vec![sdk::ClipRef::by_id("card")]);
                assert_eq!(name, "h");
            }
            other => panic!("group collects: {other:?}"),
        }
        let req = parse_layers_body(r#"{"actor":"maker","action":"ungroup","name":"g"}"#)
            .expect("ungroup parses");
        let ops = layers_edit_ops(&req, &comp)
            .expect("ops build")
            .expect("commits");
        match &ops[0] {
            sdk::EditOp::UngroupElements { name } => assert_eq!(name, "g"),
            other => panic!("ungroup releases: {other:?}"),
        }
        // A reorder of nothing known refuses.
        let req = parse_layers_body(r#"{"actor":"maker","action":"reorder","key":"ghost","to":0}"#)
            .expect("reorder parses");
        let why = layers_edit_ops(&req, &comp).expect_err("unknown key refuses");
        assert!(why.contains("names nothing"), "{why}");
    }

    #[test]
    fn layers_scope_restricts_selection_to_members() {
        let comp = two_box_comp();
        // Key gestures outside the entered group refuse naming it.
        let req = SelectRequest {
            action: "click".to_string(),
            key: Some("card".to_string()),
            at: None,
            keys: Vec::new(),
            current: Vec::new(),
            rect: None,
            selection: Vec::new(),
            scope: Some("g".to_string()),
        };
        let why = resolve_selection(&comp, &req, 320.0, 180.0).expect_err("outside refuses");
        assert!(why.contains("sits outside entered group"), "{why}");
        assert!(why.contains("\"g\""), "{why}");
        // Members still select by key.
        let inside = SelectRequest {
            scope: Some("g".to_string()),
            key: Some("panel".to_string()),
            ..req.clone()
        };
        assert_eq!(
            resolve_selection(&comp, &inside, 320.0, 180.0).expect("member selects"),
            vec!["panel".to_string()]
        );
        // Point and marquee gestures skip out-of-group clips instead.
        let point = SelectRequest {
            action: "click".to_string(),
            key: None,
            at: Some((15.0, 25.0)),
            keys: Vec::new(),
            current: Vec::new(),
            rect: None,
            selection: Vec::new(),
            scope: Some("g".to_string()),
        };
        assert_eq!(
            resolve_selection(&comp, &point, 320.0, 180.0).expect("point selects"),
            vec!["panel".to_string()],
            "the grouped twin wins the shared box"
        );
        let marquee = SelectRequest {
            action: "marquee".to_string(),
            key: None,
            at: None,
            keys: Vec::new(),
            current: Vec::new(),
            rect: Some(ElementBox {
                x: 0.0,
                y: 0.0,
                w: 320.0,
                h: 180.0,
            }),
            selection: Vec::new(),
            scope: Some("g".to_string()),
        };
        assert_eq!(
            resolve_selection(&comp, &marquee, 320.0, 180.0).expect("marquee selects"),
            vec!["panel".to_string()]
        );
        // An unknown scope refuses naming the holdings.
        let scoped = SelectRequest {
            scope: Some("nope".to_string()),
            ..point.clone()
        };
        let why = resolve_selection(&comp, &scoped, 320.0, 180.0).expect_err("bad scope refuses");
        assert!(why.contains("names no layer group"), "{why}");
    }
}
