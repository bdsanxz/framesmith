//! Canvas snap engine (Sprint 17, T003 / CAP-003, J-01, INT-003).
//!
//! Drags snap to element edges and centres, frame edges and centre,
//! grid, and equidistance guides with guide lines; rulers reflect the
//! snapped position; conflicting targets resolve by the documented
//! priority showing which one won; with snapping off, drags move
//! freely and the status line says so.
//!
//! Documented priority, strongest first: `element` edges and centres
//! (priority 0), `equidistance` guides (priority 1), `frame` edges and
//! centre (priority 2), `grid` lines (priority 3). Within the visible
//! threshold the strongest family wins even when a weaker target sits
//! nearer; ties inside one family break by distance. The route reports
//! the winning target with its family and how many rivals it beat, so
//! the maker sees exactly which rule caught the drag.
//!
//! The engine is view state only: resolving stages no patch and records
//! no turn — the snapped drag itself still lands as a named history
//! turn through the existing transform route ([`crate::studio_select`])
//! when the maker commits, so one undo system keeps serving canvas,
//! row, timeline, SDK, and CLI edits. The shared snap on/off switch and
//! threshold ride the Sprint 16 snap setting (`POST /api/studio/snap`);
//! this slice persists nothing of its own. Default renders (no SDK
//! delta) replay to themselves, so they stay byte-identical.
//!
//! Target boundary: element targets read authored extents only (the
//! same boxes selection hit-tests — `box`, `svg`, `flex`, image rects
//! defaulting per axis to the film canvas, `path` token bounds), so
//! text, video, audio, and nest clips offer no element targets and
//! hidden clips show no handles and offer none either. Dragged clips
//! ride `exclude` so a drag never snaps to itself; frame, grid, and
//! equidistance still apply. Equidistance completes equal spacings
//! between element edges and frame edges: the midpoint between two
//! edges, or the equal-gap point past either end, inside the film rect.
//!
//! Request bodies use hyphenated keys (`want`, `scale`, `grid`,
//! `exclude`, `selection`, `composition`); the engine carries no style
//! attributes and no font lists, so the CSS-like hyphenated vocabulary
//! and the fail-closed `font-family` comma try-order pass through
//! untouched.
//!
//! [`crate::studio_select`]: ../studio_select/index.html

/// Default canvas grid spacing in film pixels: snap lines repeat every
/// 10 px on each axis unless the request names another `grid`.
pub const DEFAULT_CANVAS_GRID_PX: f64 = 10.0;

/// Widest grid spacing the resolve accepts in film pixels: a wider
/// grid stops meaning anything, so the request refuses instead of
/// silently snapping to film-scale lines.
pub const MAX_CANVAS_GRID_PX: f64 = 512.0;

/// Default screen scale in screen px per film px: the canvas shows one
/// screen pixel per film pixel, so the threshold converts one to one.
pub const DEFAULT_CANVAS_SNAP_SCALE: f64 = 1.0;

/// Documented snap priority, strongest first: element edges and
/// centres beat equidistance guides, which beat frame edges and
/// centre, which beat grid lines. The route echoes this order so the
/// panel can print the rule beside every snap.
pub const CANVAS_SNAP_PRIORITY_ORDER: [&str; 4] = ["element", "equidistance", "frame", "grid"];

/// Prefix every canvas-snap refusal carries; nothing resolves on a
/// refusal.
fn refused_canvas_snap(message: String) -> String {
    format!("studio refused: {message}")
}

/// Render one film-pixel value the way rows show it: whole values read
/// as integers, anything else as written.
fn snap_num(x: f64) -> String {
    if x.is_finite() && x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// One canvas snap target: a film position on one axis with its label,
/// family, and tie-break priority (lower wins).
#[derive(Debug, Clone)]
pub struct CanvasTarget {
    /// Which axis the target sits on (`x` or `y`).
    pub axis: char,
    /// Film pixels of the target.
    pub at: f64,
    /// Human label (`card left`, `frame center-x`, `grid x 200`,
    /// `equidistance x 30 between card left and panel left`).
    pub target: String,
    /// Target family: `element`, `equidistance`, `frame`, or `grid`.
    pub family: &'static str,
    /// Tie-break priority: lower wins inside the threshold.
    pub priority: u8,
}

/// One guide line the panel paints for a snapped axis: the axis it
/// runs across, the film position, and the winning target.
#[derive(Debug, Clone)]
pub struct GuideLine {
    /// Which axis the guide marks (`x` paints a vertical line,
    /// `y` a horizontal one).
    pub axis: char,
    /// Film pixels of the guide.
    pub at: f64,
    /// Family of the winning target.
    pub family: &'static str,
    /// Label of the winning target.
    pub label: String,
}

/// Outcome of one drag axis: the landed position, whether a target
/// caught it, and the winner with its rivals.
#[derive(Debug, Clone)]
pub struct AxisOutcome {
    /// Film pixels the axis lands on (the target when snapped, the
    /// want otherwise).
    pub snapped: f64,
    /// True while a target caught the axis.
    pub did_snap: bool,
    /// Label of the catching target, when one caught the axis.
    pub target: Option<String>,
    /// Family of the catching target, when one caught the axis.
    pub family: Option<&'static str>,
    /// Priority of the catching target, when one caught the axis.
    pub priority: Option<u8>,
    /// Film pixels of the guide line, set exactly when snapped.
    pub guide_at: Option<f64>,
    /// Targets inside the threshold this winner beat on this axis.
    pub rivals: usize,
}

/// Outcome of one canvas snap: the per-axis landings, the guide lines
/// the panel paints, and the winning target text.
#[derive(Debug, Clone)]
pub struct CanvasSnapOutcome {
    /// Landing of the horizontal axis.
    pub x: AxisOutcome,
    /// Landing of the vertical axis.
    pub y: AxisOutcome,
    /// Guide lines for the snapped axes, in axis order.
    pub guides: Vec<GuideLine>,
    /// Landed film point (the targets when snapped, the want
    /// otherwise).
    pub snapped: (f64, f64),
    /// True while a target caught at least one axis.
    pub did_snap: bool,
    /// Winning target text (`card left (element, x=10)`), joined with
    /// `; ` while both axes snap, `None` while nothing caught.
    pub winner: Option<String>,
}

/// Selection key of one clip: the stable `id` when declared, else the
/// `#index` entry — the same spelling selection syncs.
fn snap_key(index: usize, clip: &notation::model::Clip) -> String {
    clip.id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("#{index}"))
}

/// Authored edge positions of one element box on one axis: the two
/// edges with the centre between them, each with its label.
fn element_edges(key: &str, axis: char, lo: f64, hi: f64) -> Vec<(f64, String)> {
    let (first, centre, last) = match axis {
        'x' => ("left", "center-x", "right"),
        _ => ("top", "center-y", "bottom"),
    };
    vec![
        (lo, format!("{key} {first}")),
        (lo + (hi - lo) / 2.0, format!("{key} {centre}")),
        (hi, format!("{key} {last}")),
    ]
}

/// Push one target unless the same axis already holds the position:
/// the first writer wins, so callers add families strongest first.
fn push_target(
    out: &mut Vec<CanvasTarget>,
    axis: char,
    at: f64,
    target: String,
    family: &'static str,
    priority: u8,
) {
    if !at.is_finite() {
        return;
    }
    if out
        .iter()
        .any(|seen| seen.axis == axis && (seen.at - at).abs() < 1e-9)
    {
        return;
    }
    out.push(CanvasTarget {
        axis,
        at,
        target,
        family,
        priority,
    });
}

/// Build every canvas snap target of one composition (Sprint 17,
/// T003): element edges and centres of the visible authored boxes
/// outside `exclude`, frame edges and centre, grid lines every `grid`
/// film pixels, and equidistance completions between element and frame
/// edges. Pure view data: no patch stages, no history records.
pub fn build_canvas_snap_candidates(
    comp: &notation::model::Composition,
    film_w: f64,
    film_h: f64,
    grid: f64,
    exclude: &[String],
) -> Vec<CanvasTarget> {
    let mut out = Vec::new();
    let mut edge_labels: Vec<(char, f64, String)> = Vec::new();
    for (index, clip) in comp.clips.iter().enumerate() {
        if clip.hidden() {
            continue;
        }
        let key = snap_key(index, clip);
        if exclude.iter().any(|e| e == &key) {
            continue;
        }
        let Some(b) = crate::studio_select::element_box_of(clip, film_w, film_h) else {
            continue;
        };
        for (at, label) in element_edges(&key, 'x', b.x, b.x + b.w) {
            edge_labels.push(('x', at, label.clone()));
            push_target(&mut out, 'x', at, label, "element", 0);
        }
        for (at, label) in element_edges(&key, 'y', b.y, b.y + b.h) {
            edge_labels.push(('y', at, label.clone()));
            push_target(&mut out, 'y', at, label, "element", 0);
        }
    }
    // Frame edges and centre come next: weaker than elements, stronger
    // than grid, so they dedupe against element positions first.
    if film_w.is_finite() && film_w > 0.0 {
        for (at, label) in [
            (0.0, "frame left".to_string()),
            (film_w / 2.0, "frame center-x".to_string()),
            (film_w, "frame right".to_string()),
        ] {
            edge_labels.push(('x', at, label.clone()));
            push_target(&mut out, 'x', at, label, "frame", 2);
        }
    }
    if film_h.is_finite() && film_h > 0.0 {
        for (at, label) in [
            (0.0, "frame top".to_string()),
            (film_h / 2.0, "frame center-y".to_string()),
            (film_h, "frame bottom".to_string()),
        ] {
            edge_labels.push(('y', at, label.clone()));
            push_target(&mut out, 'y', at, label, "frame", 2);
        }
    }
    // Equidistance completes equal spacings between the collected
    // edges: the midpoint between two edges, and the equal-gap point
    // past either end, inside the film rect.
    for (axis, dim) in [('x', film_w), ('y', film_h)] {
        if !dim.is_finite() || dim <= 0.0 {
            continue;
        }
        let mut edges: Vec<(f64, String)> = edge_labels
            .iter()
            .filter(|(a, _, _)| *a == axis)
            .map(|(_, at, label)| (*at, label.clone()))
            .collect();
        edges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        edges.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
        let mut pairs: Vec<((f64, String), (f64, String))> = Vec::new();
        for i in 0..edges.len() {
            for j in (i + 1)..edges.len() {
                pairs.push((edges[i].clone(), edges[j].clone()));
            }
        }
        for ((a, la), (b, lb)) in pairs {
            let mid = (a + b) / 2.0;
            if mid >= 0.0 && mid <= dim {
                push_target(
                    &mut out,
                    axis,
                    mid,
                    format!(
                        "equidistance {axis} {} between {la} and {lb}",
                        snap_num(mid)
                    ),
                    "equidistance",
                    1,
                );
            }
            let gap = b - a;
            if gap > 1e-9 {
                let past_b = b + gap;
                if past_b >= 0.0 && past_b <= dim {
                    push_target(
                        &mut out,
                        axis,
                        past_b,
                        format!("equidistance {axis} {} past {lb}", snap_num(past_b)),
                        "equidistance",
                        1,
                    );
                }
                let past_a = a - gap;
                if past_a >= 0.0 && past_a <= dim {
                    push_target(
                        &mut out,
                        axis,
                        past_a,
                        format!("equidistance {axis} {} past {la}", snap_num(past_a)),
                        "equidistance",
                        1,
                    );
                }
            }
        }
    }
    // Grid lines run last: weakest, so every element, equidistance,
    // and frame target already holding the position keeps it.
    if grid.is_finite() && grid > 0.0 && grid <= MAX_CANVAS_GRID_PX {
        for (axis, dim) in [('x', film_w), ('y', film_h)] {
            if !dim.is_finite() || dim <= 0.0 {
                continue;
            }
            let mut at = 0.0;
            let mut steps = 0usize;
            while at <= dim + 1e-9 && steps < 4096 {
                push_target(
                    &mut out,
                    axis,
                    at.min(dim),
                    format!("grid {axis} {}", snap_num(at.min(dim))),
                    "grid",
                    3,
                );
                at += grid;
                steps += 1;
            }
        }
    }
    out
}

/// Resolve one drag point against the targets (Sprint 17, T003): per
/// axis, the strongest family inside `threshold_px / scale` film pixels
/// wins (element, then equidistance, then frame, then grid); ties
/// inside one family break by distance. With snapping off, or a
/// non-positive scale, every drag moves freely. Pure arithmetic: no
/// patch stages, no history records.
pub fn resolve_canvas_snap(
    want: (f64, f64),
    settings: &crate::studio_snap_zoom::SnapSettings,
    scale: f64,
    candidates: &[CanvasTarget],
) -> CanvasSnapOutcome {
    let free_axis = |want: f64| AxisOutcome {
        snapped: want,
        did_snap: false,
        target: None,
        family: None,
        priority: None,
        guide_at: None,
        rivals: 0,
    };
    let free = || CanvasSnapOutcome {
        x: free_axis(want.0),
        y: free_axis(want.1),
        guides: Vec::new(),
        snapped: want,
        did_snap: false,
        winner: None,
    };
    if !settings.enabled {
        return free();
    }
    if !scale.is_finite() || scale <= 0.0 {
        return free();
    }
    let threshold = settings.threshold_px / scale;
    let mut axes = Vec::new();
    for (axis, want_at) in [('x', want.0), ('y', want.1)] {
        let mut hits: Vec<&CanvasTarget> = candidates
            .iter()
            .filter(|c| {
                c.axis == axis && c.at.is_finite() && (want_at - c.at).abs() <= threshold + 1e-9
            })
            .collect();
        hits.sort_by(|a, b| {
            a.priority.cmp(&b.priority).then_with(|| {
                (want_at - a.at)
                    .abs()
                    .partial_cmp(&(want_at - b.at).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        match hits.first() {
            Some(hit) => axes.push(AxisOutcome {
                snapped: hit.at,
                did_snap: true,
                target: Some(hit.target.clone()),
                family: Some(hit.family),
                priority: Some(hit.priority),
                guide_at: Some(hit.at),
                rivals: hits.len() - 1,
            }),
            None => axes.push(free_axis(want_at)),
        }
    }
    let mut outcome = CanvasSnapOutcome {
        x: axes[0].clone(),
        y: axes[1].clone(),
        guides: Vec::new(),
        snapped: want,
        did_snap: false,
        winner: None,
    };
    let mut winners = Vec::new();
    for (axis, axis_outcome) in [('x', &outcome.x), ('y', &outcome.y)] {
        if let (Some(target), Some(family), Some(at)) = (
            &axis_outcome.target,
            axis_outcome.family,
            axis_outcome.guide_at,
        ) {
            outcome.guides.push(GuideLine {
                axis,
                at,
                family,
                label: target.clone(),
            });
            winners.push(format!("{target} ({family}, {axis}={})", snap_num(at)));
        }
    }
    outcome.snapped = (outcome.x.snapped, outcome.y.snapped);
    outcome.did_snap = outcome.x.did_snap || outcome.y.did_snap;
    if !winners.is_empty() {
        outcome.winner = Some(winners.join("; "));
    }
    outcome
}

/// Status line one canvas snap reports: the winning targets with
/// their families and beaten rivals while snapped, the free-move note
/// while off or out of reach.
pub fn canvas_snap_status_line(
    settings: &crate::studio_snap_zoom::SnapSettings,
    outcome: &CanvasSnapOutcome,
    want: (f64, f64),
) -> String {
    if !settings.enabled {
        return "snapping is off: drags move freely.".to_string();
    }
    if !outcome.did_snap {
        return format!(
            "no canvas snap within {:.0} px of ({}, {}): the drag moves freely.",
            settings.threshold_px,
            snap_num(want.0),
            snap_num(want.1)
        );
    }
    let mut parts = Vec::new();
    for (axis, axis_outcome) in [('x', &outcome.x), ('y', &outcome.y)] {
        if let (Some(target), Some(family), Some(at)) = (
            &axis_outcome.target,
            axis_outcome.family,
            axis_outcome.guide_at,
        ) {
            let mut part = format!("{axis} to {target} ({family}) at {}px", snap_num(at));
            if axis_outcome.rivals > 0 {
                part.push_str(&format!(
                    ", beating {} rival{}",
                    axis_outcome.rivals,
                    if axis_outcome.rivals == 1 { "" } else { "s" }
                ));
            }
            parts.push(part);
        }
    }
    format!(
        "snapped {} (guide lines shown; threshold {:.0} px).",
        parts.join(" and "),
        settings.threshold_px
    )
}

/// One parsed canvas-snap resolve request: the drag point with the
/// screen scale and grid behind it. Structural parsing only; the route
/// fills the targets from the effective composition.
#[derive(Debug, Clone)]
pub struct CanvasSnapRequest {
    /// Drag point in film pixels.
    pub want: (f64, f64),
    /// Screen pixels per film pixel (the canvas zoom over 100).
    pub scale: f64,
    /// Grid spacing in film pixels.
    pub grid: f64,
    /// Selection keys of the dragged clips, excluded as snap anchors so
    /// a drag never snaps to itself.
    pub exclude: Vec<String>,
    /// Shared selection the shell scoped the resolve to, echoed back
    /// so canvas and timeline show the same selection at once.
    pub selection: Vec<String>,
    /// Composition the drag belongs to, when named.
    pub composition: Option<String>,
}

/// Read one request number: present, numeric, and finite, or a refusal
/// naming the key.
fn snap_number(value: &serde_json::Value, key: &str) -> Result<f64, String> {
    let n = value.as_f64().ok_or_else(|| {
        refused_canvas_snap(format!(
            "the canvas snap `{key}` value {value} is not a number; pass finite film pixels. Nothing was resolved."
        ))
    })?;
    if !n.is_finite() {
        return Err(refused_canvas_snap(format!(
            "the canvas snap `{key}` value {n:?} is not finite; pass finite film pixels. Nothing was resolved."
        )));
    }
    Ok(n)
}

/// Read one clip list: a comma-separated string or a JSON array of
/// strings, trimming empties. Anything else refuses naming the key.
fn snap_key_list(value: &serde_json::Value, key: &str) -> Result<Vec<String>, String> {
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
                    refused_canvas_snap(format!(
                        "the canvas snap `{key}` entry {item} is not a clip reference; pass declared `id` values or `#index` entries. Nothing was resolved."
                    ))
                })?;
                if !name.trim().is_empty() {
                    out.push(name.trim().to_string());
                }
            }
            Ok(out)
        }
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err(refused_canvas_snap(format!(
            "the canvas snap `{key}` value {value} is not a clip list; pass declared `id` values or `#index` entries as a comma string or an array. Nothing was resolved."
        ))),
    }
}

/// Parse one canvas-snap body
/// (`{"want":[11,44],"scale":1,"grid":10,"exclude":["card"]}`): the
/// drag point must ride along; the scale defaults to one screen pixel
/// per film pixel and the grid to every 10 film pixels. A refused body
/// names its cause and resolves nothing.
pub fn parse_canvas_snap_body(body: &str) -> Result<CanvasSnapRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_canvas_snap(
            "the canvas snap needs a JSON body (`{\"want\":[11,44]}`); pass the drag point in film pixels. Nothing was resolved."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_canvas_snap(format!(
            "the canvas snap body is not JSON ({e}). Pass `{{\"want\":[11,44]}}`. Nothing was resolved."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_canvas_snap(
            "the canvas snap body is not a snap request; pass a JSON object with a `want` point. Nothing was resolved."
                .to_string(),
        ));
    }
    let want_value = value.get("want").ok_or_else(|| {
        refused_canvas_snap(
            "the canvas snap names no `want` point; pass the drag `[x, y]` film pixels. Nothing was resolved."
                .to_string(),
        )
    })?;
    let pair = want_value.as_array().ok_or_else(|| {
        refused_canvas_snap(format!(
            "the canvas snap `want` value {want_value} is not a point; pass the drag `[x, y]` film pixels. Nothing was resolved."
        ))
    })?;
    if pair.len() != 2 {
        return Err(refused_canvas_snap(format!(
            "the canvas snap `want` value {want_value} is not a point; pass the drag `[x, y]` film pixels. Nothing was resolved."
        )));
    }
    let want = (
        snap_number(&pair[0], "want")?,
        snap_number(&pair[1], "want")?,
    );
    let scale = match value.get("scale") {
        None => DEFAULT_CANVAS_SNAP_SCALE,
        Some(v) => {
            let n = snap_number(v, "scale")?;
            if n <= 0.0 {
                return Err(refused_canvas_snap(format!(
                    "the canvas snap `scale` value {n:?} never shows a canvas; pass screen pixels per film pixel above 0. Nothing was resolved."
                )));
            }
            n
        }
    };
    let grid = match value.get("grid") {
        None => DEFAULT_CANVAS_GRID_PX,
        Some(v) => {
            let n = snap_number(v, "grid")?;
            if n <= 0.0 || n > MAX_CANVAS_GRID_PX {
                return Err(refused_canvas_snap(format!(
                    "the canvas snap `grid` value {n:?} is outside 0..={MAX_CANVAS_GRID_PX} film pixels; pass a finite grid spacing in range. Nothing was resolved."
                )));
            }
            n
        }
    };
    let exclude = match value.get("exclude") {
        None => Vec::new(),
        Some(v) => snap_key_list(v, "exclude")?,
    };
    let selection = match value.get("selection") {
        None => Vec::new(),
        Some(v) => snap_key_list(v, "selection")?,
    };
    let composition = value
        .get("composition")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string());
    Ok(CanvasSnapRequest {
        want,
        scale,
        grid,
        exclude,
        selection,
        composition,
    })
}

/// Render one snap axis as JSON: the landing, whether a target caught
/// it, the winner with its family and priority, and the guide.
fn axis_json(axis: &AxisOutcome) -> String {
    let target = match &axis.target {
        Some(t) => format!("\"{}\"", crate::studio::json_escape(t)),
        None => "null".to_string(),
    };
    let family = match axis.family {
        Some(f) => format!("\"{f}\""),
        None => "null".to_string(),
    };
    let priority = match axis.priority {
        Some(p) => format!("{p}"),
        None => "null".to_string(),
    };
    let guide_at = match axis.guide_at {
        Some(at) => snap_num(at),
        None => "null".to_string(),
    };
    format!(
        "{{\"snapped\":{},\"did-snap\":{},\"target\":{target},\"family\":{family},\"priority\":{priority},\"guide-at\":{guide_at},\"rivals\":{}}}",
        snap_num(axis.snapped),
        axis.did_snap,
        axis.rivals,
    )
}

/// Render one canvas snap resolution as JSON for the canvas-snap API:
/// the want with its snapped landing, the per-axis winners, the guide
/// lines and ruler positions the panel paints, the documented priority
/// order, and the synced selection echo. Every key is hyphenated.
#[allow(clippy::too_many_arguments)]
pub fn canvas_snap_json(
    settings: &crate::studio_snap_zoom::SnapSettings,
    outcome: &CanvasSnapOutcome,
    want: (f64, f64),
    grid: f64,
    scale: f64,
    kept: &[String],
    dropped: &[String],
) -> String {
    let winner = match &outcome.winner {
        Some(w) => format!("\"{}\"", crate::studio::json_escape(w)),
        None => "null".to_string(),
    };
    let mut guides = String::new();
    for guide in &outcome.guides {
        if !guides.is_empty() {
            guides.push(',');
        }
        guides.push_str(&format!(
            "{{\"axis\":\"{}\",\"at\":{},\"family\":\"{}\",\"label\":\"{}\"}}",
            guide.axis,
            snap_num(guide.at),
            guide.family,
            crate::studio::json_escape(&guide.label),
        ));
    }
    let order = CANVAS_SNAP_PRIORITY_ORDER
        .iter()
        .map(|f| format!("\"{f}\""))
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
        "{{\"want\":[{},{}],\"snapped\":[{},{}],\"did-snap\":{},\"x\":{},\"y\":{},\"winner\":{winner},\"guide-lines\":[{guides}],\"ruler-x\":{},\"ruler-y\":{},\"scale\":{},\"grid\":{},\"threshold-px\":{},\"snapping-enabled\":{},\"priority-order\":[{order}],\"status-line\":\"{}\",\"selection\":{{\"kept\":[{kept_json}],\"dropped\":[{dropped_json}]}}}}",
        snap_num(want.0),
        snap_num(want.1),
        snap_num(outcome.snapped.0),
        snap_num(outcome.snapped.1),
        outcome.did_snap,
        axis_json(&outcome.x),
        axis_json(&outcome.y),
        snap_num(outcome.snapped.0),
        snap_num(outcome.snapped.1),
        snap_num(scale),
        snap_num(grid),
        snap_num(settings.threshold_px),
        settings.enabled,
        crate::studio::json_escape(&canvas_snap_status_line(settings, outcome, want)),
    )
}

/// Extra CSS the snap slice needs (Sprint 17, T003): the snap resolve
/// panel with its hyphenated inputs, the snap status line, and the
/// guide lines the panel paints over the film. Style attributes stay
/// CSS-like hyphenated end to end (`background-color`,
/// `border-radius`, `border-left`, `z-index`).
pub fn canvas_snap_css() -> String {
    r#"        #canvas-snap-panel{margin-top:8px}
        #canvas-snap-panel label{margin-right:8px;font-variant-numeric:tabular-nums}
        #canvas-snap-panel button{margin-right:4px}
        #canvas-snap-status{font-variant-numeric:tabular-nums}
        #canvas-snap-priority{font-variant-numeric:tabular-nums}
        .canvas-snap-guide{position:absolute;background-color:var(--accent-color);z-index:4;pointer-events:none}
        .canvas-snap-guide[data-axis="x"]{top:0;bottom:0;width:2px}
        .canvas-snap-guide[data-axis="y"]{left:0;right:0;height:2px}"#.to_string()
}

/// The snap control block (Sprint 17, T003): a resolve form posting the
/// drag point with its scale and grid, the status line naming the
/// winner, and the documented priority legend.
pub fn canvas_snap_html() -> String {
    r#"<div class="dock-bar" id="canvas-snap-panel">
        <span>snap</span>
        <label>want-x <input id="canvas-snap-x" type="number" value="0" step="1"></label>
        <label>want-y <input id="canvas-snap-y" type="number" value="0" step="1"></label>
        <label>scale <input id="canvas-snap-scale" type="number" value="1" step="0.25" min="0.25"></label>
        <label>grid <input id="canvas-snap-grid" type="number" value="10" step="1" min="1"></label>
        <button id="canvas-snap-resolve" type="button">resolve snap</button>
        </div>
        <p id="canvas-snap-status" role="status">canvas snap ready</p>
        <p id="canvas-snap-priority">snap priority: element, then equidistance, then frame, then grid; the winner shows with its family.</p>"#.to_string()
}

/// The snap panel script (Sprint 17, T003): reports the shared snap
/// switch, resolves drag points through the canvas-snap API, paints
/// the guide lines over the film, and reflects the snapped position
/// on the rulers.
pub fn canvas_snap_js() -> String {
    r#"        function snapStatus(t) { const s = document.getElementById('canvas-snap-status'); if (s) s.textContent = t; }
        function paintSnapGuides(guides) {
          document.querySelectorAll('.canvas-snap-guide').forEach((g) => g.remove());
          const wrap = document.getElementById('canvas-frame-wrap');
          if (!wrap) return;
          for (const guide of (guides || [])) {
            const line = document.createElement('div');
            line.className = 'canvas-snap-guide';
            line.dataset.axis = guide.axis;
            line.title = guide.label + ' (' + guide.family + ')';
            if (guide.axis === 'x') line.style.left = guide.at + 'px';
            else line.style.top = guide.at + 'px';
            wrap.appendChild(line);
          }
        }
        function paintSnapRulers(x, y) {
          const rx = document.getElementById('canvas-ruler-x');
          if (rx && !rx.hidden) rx.textContent += ' [snap ' + x + ']';
          const ry = document.getElementById('canvas-ruler-y');
          if (ry && !ry.hidden) ry.textContent += ' [snap ' + y + ']';
        }
        async function paintCanvasSnap() {
          try {
            const r = await fetch('/api/studio/snap');
            if (!r.ok) { snapStatus('canvas snap unavailable: ' + await r.text()); return; }
            const j = await r.json();
            snapStatus(j['snapping-enabled'] === false || j.enabled === false
              ? 'canvas snap: snapping is off, drags move freely.'
              : 'canvas snap: drags snap to element edges and centres, equidistance, frame edges and centre, and grid within ' + (j['threshold-px'] || 8) + ' px.');
          } catch (e) { snapStatus('canvas snap unavailable: ' + e); }
        }
        async function resolveCanvasSnap() {
          try {
            const x = Number((document.getElementById('canvas-snap-x') || {}).value || 0);
            const y = Number((document.getElementById('canvas-snap-y') || {}).value || 0);
            const scale = Number((document.getElementById('canvas-snap-scale') || {}).value || 1);
            const grid = Number((document.getElementById('canvas-snap-grid') || {}).value || 10);
            const r = await fetch('/api/studio/canvas-snap', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({want: [x, y], scale: scale, grid: grid, selection: selection.join(',')})});
            const t = await r.text();
            if (!r.ok) { snapStatus(t); banner(t); return; }
            const j = JSON.parse(t);
            if (j.selection && j.selection.kept) selection = j.selection.kept.slice();
            paintSnapGuides(j['guide-lines']);
            paintSnapRulers(j['ruler-x'], j['ruler-y']);
            snapStatus(j['status-line'] || 'snap resolved');
            syncLink();
          } catch (e) { snapStatus('snap resolve failed: ' + e); }
        }
        (function () {
          const el = document.getElementById('canvas-snap-resolve');
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('click', resolveCanvasSnap); }
        })();"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio_snap_zoom::SnapSettings;

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
            clips: vec![
                Clip::Box(BoxClip {
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
                }),
                Clip::Box(BoxClip {
                    asserts: MotionAsserts::default(),
                    start: 0.0,
                    duration: 4.0,
                    id: Some("panel".to_string()),
                    track: TrackKind::Graphics,
                    label: None,
                    start_after: None,
                    x: 50.0,
                    y: 30.0,
                    width: 80.0,
                    height: 40.0,
                    fill: Rgb { r: 255, g: 0, b: 0 },
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
                    order: 1,
                    turn: Turn3D::flat(),
                    filter_blur: 0.0,
                    grade: Grade::default(),
                    effects: Vec::new(),
                    vfx: VfxChain::default(),
                    hidden: false,
                    line: 1,
                    column: 1,
                }),
            ],
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

    fn on() -> SnapSettings {
        SnapSettings {
            enabled: true,
            threshold_px: 8.0,
        }
    }

    fn targets(grid: f64, exclude: &[String]) -> Vec<CanvasTarget> {
        build_canvas_snap_candidates(&test_comp(), 320.0, 180.0, grid, exclude)
    }

    #[test]
    fn element_edge_beats_grid_at_the_same_distance() {
        // `card left` and the grid line both sit at x = 10: the drag at
        // 11 is 1 px from each, and the documented priority crowns the
        // element with one beaten rival.
        let outcome = resolve_canvas_snap((11.0, 200.0), &on(), 1.0, &targets(10.0, &[]));
        assert!(outcome.x.did_snap);
        assert_eq!(outcome.x.snapped, 10.0);
        assert_eq!(outcome.x.family, Some("element"));
        assert_eq!(outcome.x.target.as_deref(), Some("card left"));
        assert_eq!(outcome.x.rivals, 1);
        assert!(!outcome.y.did_snap);
        assert_eq!(outcome.snapped, (10.0, 200.0));
        assert!(outcome.did_snap);
    }

    #[test]
    fn equidistance_midpoint_beats_grid() {
        // `card left` (10) and `panel left` (50) complete the midpoint
        // 30, which the grid line at 30 also holds: equidistance wins.
        let outcome = resolve_canvas_snap((31.0, 200.0), &on(), 1.0, &targets(10.0, &[]));
        assert!(outcome.x.did_snap);
        assert_eq!(outcome.x.snapped, 30.0);
        assert_eq!(outcome.x.family, Some("equidistance"));
        assert!(outcome.x.target.as_deref().unwrap().contains("between"));
    }

    #[test]
    fn frame_centre_beats_grid_on_the_vertical_axis() {
        // The drag at y = 89 sits 1 px from `frame center-y` and from
        // the grid line at 90: the documented priority crowns the
        // frame. Both dragged boxes ride `exclude` so their equal-gap
        // guides stay out of the resolve. (On x the drag sits exactly
        // on grid 300 with no stronger rival, so x reads grid.)
        let excluded = vec!["card".to_string(), "panel".to_string()];
        let outcome = resolve_canvas_snap((300.0, 89.0), &on(), 1.0, &targets(10.0, &excluded));
        assert!(outcome.y.did_snap);
        assert_eq!(outcome.y.snapped, 90.0);
        assert_eq!(outcome.y.family, Some("frame"));
        assert_eq!(outcome.y.target.as_deref(), Some("frame center-y"));
        assert_eq!(outcome.x.family, Some("grid"));
    }

    #[test]
    fn excluded_drag_never_snaps_to_itself() {
        let excluded = vec!["card".to_string()];
        let outcome = resolve_canvas_snap((11.0, 200.0), &on(), 1.0, &targets(10.0, &excluded));
        // `card left` is out as an anchor, so the equal-gap guide past
        // `panel left` holds x = 10 as the equidistance family instead.
        assert!(outcome.x.did_snap);
        assert_eq!(outcome.x.snapped, 10.0);
        assert_eq!(outcome.x.family, Some("equidistance"));
        assert_eq!(
            outcome.x.target.as_deref(),
            Some("equidistance x 10 past panel left")
        );
    }

    #[test]
    fn snap_off_moves_freely() {
        let off = SnapSettings {
            enabled: false,
            threshold_px: 8.0,
        };
        let outcome = resolve_canvas_snap((11.0, 21.0), &off, 1.0, &targets(10.0, &[]));
        assert!(!outcome.did_snap);
        assert_eq!(outcome.snapped, (11.0, 21.0));
        assert!(outcome.guides.is_empty());
        assert_eq!(
            canvas_snap_status_line(&off, &outcome, (11.0, 21.0)),
            "snapping is off: drags move freely."
        );
    }

    #[test]
    fn parse_refuses_bad_bodies() {
        assert!(parse_canvas_snap_body("").is_err());
        assert!(parse_canvas_snap_body("{}").is_err());
        assert!(parse_canvas_snap_body(r#"{"want":[1]}"#).is_err());
        assert!(parse_canvas_snap_body(r#"{"want":[1,2],"scale":0}"#).is_err());
        assert!(parse_canvas_snap_body(r#"{"want":[1,2],"grid":-5}"#).is_err());
        let req = parse_canvas_snap_body(r#"{"want":[11,44]}"#).expect("minimal parses");
        assert_eq!(req.scale, DEFAULT_CANVAS_SNAP_SCALE);
        assert_eq!(req.grid, DEFAULT_CANVAS_GRID_PX);
    }
}
