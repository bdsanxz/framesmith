//! Composition cards with live hover preview, drag-or-add placement,
//! and render-one (Sprint 18, T001 / CAP-001, J-01, J-07, STATE-01,
//! REC-001).
//!
//! The maker browses every composition in the project as a card: the
//! name with a root badge on the render root, the lint count for that
//! composition, and a live hover preview (frame 0 drawn through the
//! same pipeline as a full render, so the thumbnail shows exactly what
//! the render opens with). Dragging a card onto the timeline — or its
//! add button — places the composition there as a `nest` clip through
//! the Sprint 15 patch and history machinery, so placing is one named,
//! undoable turn like every other Studio edit. Render-one queues
//! exactly that composition and nothing else.
//!
//! Read boundary: cards read project truth on every call — one
//! validation plus the persisted SDK delta the session would replay —
//! and every placing or queueing call carries the generation the cards
//! were read with. A call arriving with an older generation refuses as
//! stale and answers with fresh cards instead of acting on old data,
//! so the panels, canvas, and history stay exactly as they were.
//!
//! Write boundary: the only project writes are one validated history
//! turn per placement (the SDK delta plus its history record) and the
//! hidden studio state files (the render-queue file and, at execution
//! time by the Renders panel, the queued outputs beside it) — never
//! the composition sources, which stay byte-identical, and never
//! outside the project except the outside-project preview caches the
//! preview sessions already own. Default renders replay no delta and
//! read no hidden state, so they stay byte-identical.
//!
//! Refusal boundary: an invalid project lists empty cards with the
//! reason; placing an unknown composition, nesting a composition into
//! itself, closing a nest cycle, nesting past the depth limit, or
//! dropping onto a full timeline refuses with the reason naming the
//! cause; render-one on a composition carrying lint findings queues
//! nothing and renders nothing, naming the failing count. Every
//! refusal stages nothing, queues nothing, and records nothing.
//!
//! Style boundary: panel CSS uses hyphenated CSS vocabulary
//! (`background-color`, `border-radius`, `object-fit`,
//! `font-variant-numeric`) and carries no `font-family`, so the
//! fail-closed font try-order passes through untouched.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Version of the persisted render-queue file.
pub const RENDER_QUEUE_VERSION: u32 = 1;

/// Render-queue file name inside the hidden project directory (next
/// to the SDK delta, layout, and history — never beside the sources
/// and never outside the project).
pub const STUDIO_RENDER_QUEUE_FILE: &str = "studio-render-queue.json";

/// Queued-render output directory inside the hidden project
/// directory: queued outputs land here, outside the project sources
/// exactly as CLI renders land outside them.
pub const STUDIO_RENDERS_DIR: &str = "studio-renders";

/// How many clips a timeline holds before Studio calls it full
/// (Sprint 18, T001): a drag onto a timeline at this count refuses
/// naming the cap and the measured count, staging nothing. The cap
/// sits far above reviewable timelines so ordinary projects never
/// meet it; tests pass a smaller `max_clips` to prove the refusal.
pub const MAX_TIMELINE_CLIPS: usize = 512;

/// Hidden render-queue path for one project folder.
pub fn render_queue_path_for(folder: &Path) -> PathBuf {
    folder
        .join(sdk::HISTORY_DIR_NAME)
        .join(STUDIO_RENDER_QUEUE_FILE)
}

/// Hidden queued-output directory for one project folder.
pub fn renders_dir_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(STUDIO_RENDERS_DIR)
}

/// Why a compositions action refused.
fn refused_compositions(message: String) -> String {
    format!("studio refused: {message}")
}

/// One composition card (Sprint 18, T001): the name the timeline
/// answers to, the root badge, the lint count for exactly this
/// composition, and the shape the live hover preview draws at.
#[derive(Debug, Clone)]
pub struct CompositionCard {
    /// Composition `name` as written.
    pub name: String,
    /// Stable reference `id`, when written.
    pub id: Option<String>,
    /// True for the render root (the lone composition, or the one
    /// marked `root=true`).
    pub is_root: bool,
    /// Lint findings located in this composition's source span.
    pub lint_count: usize,
    /// Clips on this composition's timeline.
    pub clip_count: usize,
    /// Output frame count of this composition.
    pub frame_count: u64,
    /// Composition length in seconds.
    pub duration_secs: f64,
    /// Fps value behind the clock.
    pub fps_value: f64,
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
}

/// What listing the cards reported (Sprint 18, T001): the cards in
/// document order, the generation placing and queueing calls must
/// echo, and the fallback notice when the project cannot list.
#[derive(Debug, Clone)]
pub struct CardsLoad {
    /// One card per validated composition, in document order; empty
    /// while the project cannot list, with `notice` naming why.
    pub cards: Vec<CompositionCard>,
    /// Generation of the project truth the cards were read with
    /// (sources plus the persisted edit delta): placing and
    /// queueing calls echo it, and an older echo refuses as stale.
    pub generation: u64,
    /// Fallback reason, set exactly when no card lists.
    pub notice: Option<String>,
    /// Lint findings no card owns (project-level blocks outside any
    /// composition span): reported here, never counted on a card and
    /// never blocking render-one.
    pub unattributed_lint: usize,
}

/// Generation of the project truth behind the cards (Sprint 18,
/// T001): a hash over the composition sources plus the persisted SDK
/// edit delta, so an external edit or a landed turn stales every card
/// read before it. Pure read: opens nothing, writes nothing.
pub fn generation_of(folder: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for name in [
        validate::validate::COMPOSITION_FILE,
        &format!(
            "{}/{}",
            sdk::HISTORY_DIR_NAME,
            sdk::edits::SDK_OVERRIDES_FILE
        ),
    ] {
        let path = folder.join(name);
        match std::fs::read(&path) {
            Ok(bytes) => {
                path.as_os_str().hash(&mut hasher);
                bytes.hash(&mut hasher);
            }
            Err(_) => {
                path.as_os_str().hash(&mut hasher);
                0u8.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

/// Source line span of every top-level `composition` node in document
/// order: `(first line, last line)` from the node's own line through
/// the deepest descendant line (values included), so sibling blocks
/// between compositions never leak into a span. `None` while the
/// sources do not parse — the caller then lists empty cards with the
/// validation reason instead of guessing spans.
fn composition_spans(text: &str) -> Option<Vec<(usize, usize)>> {
    let roots = notation::parse::parse_document(text).ok()?;
    let mut spans = Vec::new();
    for node in &roots {
        if node.name != "composition" {
            continue;
        }
        spans.push((node.name_line, subtree_max_line(node)));
    }
    if spans.is_empty() {
        return None;
    }
    Some(spans)
}

/// Deepest source line in one parsed subtree: the node's own line,
/// every prop value line, and every descendant line.
fn subtree_max_line(node: &notation::parse::RawNode) -> usize {
    let mut max = node.name_line;
    for (_, value) in &node.props {
        max = max.max(value.line);
    }
    for child in &node.children {
        max = max.max(subtree_max_line(child));
    }
    max
}

/// Attribute every lint finding to its composition index in document
/// order (Sprint 18, T001): a finding naming `in composition "NAME"`
/// in its message joins that composition; otherwise a finding filed
/// on `composition.kdl` inside a composition's source span joins the
/// span owner. Anything else (project-level blocks, other files) is
/// unattributed: reported at project level, counted on no card. The
/// second return is that unattributed count.
fn attribute_lint(
    verdict: &validate::validate::Verdict,
    spans: Option<&[(usize, usize)]>,
) -> (BTreeMap<usize, usize>, usize) {
    let mut per: BTreeMap<usize, usize> = BTreeMap::new();
    let mut unattributed = 0usize;
    let use_spans = spans.map(|s| s.len() == verdict.compositions.len());
    for finding in &verdict.lint_findings {
        let mut owner: Option<usize> = None;
        for (index, comp) in verdict.compositions.iter().enumerate() {
            if finding
                .message
                .contains(&format!("in composition {:?}", comp.name))
            {
                owner = Some(index);
                break;
            }
        }
        if owner.is_none() {
            if let Some(file_name) = Path::new(&finding.file)
                .file_name()
                .and_then(|n| n.to_str())
            {
                if file_name == validate::validate::COMPOSITION_FILE {
                    if let (Some(spans), Some(true)) = (spans, use_spans) {
                        for (index, (first, last)) in spans.iter().enumerate() {
                            if finding.line >= *first && finding.line <= *last {
                                owner = Some(index);
                                break;
                            }
                        }
                    }
                }
            }
        }
        match owner {
            Some(index) => {
                *per.entry(index).or_insert(0) += 1;
            }
            None => unattributed += 1,
        }
    }
    (per, unattributed)
}

/// List every composition card (Sprint 18, T001): one validation read
/// of project truth, lint attributed per composition, root badged from
/// the verdict. An invalid or unreadable project lists empty cards
/// with the reason — never blank cards, never guessed cards. Pure
/// read: opens no session, writes nothing anywhere.
pub fn list_composition_cards(folder: &Path) -> CardsLoad {
    let generation = generation_of(folder);
    let verdict = validate::validate::validate_folder(folder);
    if verdict.status != validate::validate::Status::Valid {
        let mut reason = format!(
            "the project {} cannot list compositions; fix the problems `check` reports and reopen the panel.",
            folder.display()
        );
        for d in verdict.diagnostics.iter().take(3) {
            reason.push_str(&format!(
                " {}:{}:{} {} {}",
                d.file, d.line, d.column, d.code, d.message
            ));
        }
        if verdict.diagnostics.len() > 3 {
            reason.push_str(&format!(" (and {} more)", verdict.diagnostics.len() - 3));
        }
        return CardsLoad {
            cards: Vec::new(),
            generation,
            notice: Some(reason),
            unattributed_lint: verdict.lint_findings.len(),
        };
    }
    let text = std::fs::read_to_string(folder.join(validate::validate::COMPOSITION_FILE))
        .unwrap_or_default();
    let spans = composition_spans(&text);
    let (per, unattributed) = attribute_lint(&verdict, spans.as_deref());
    let cards = verdict
        .compositions
        .iter()
        .enumerate()
        .map(|(index, comp)| CompositionCard {
            name: comp.name.clone(),
            id: comp.id.clone(),
            is_root: verdict.root_name.as_deref() == Some(comp.name.as_str()),
            lint_count: per.get(&index).copied().unwrap_or(0),
            clip_count: comp.clips.len(),
            frame_count: comp.fps.frame_count(comp.duration),
            duration_secs: comp.duration,
            fps_value: comp.fps.value(),
            width: comp.width,
            height: comp.height,
        })
        .collect();
    CardsLoad {
        cards,
        generation,
        notice: None,
        unattributed_lint: unattributed,
    }
}

/// Find one composition by `name` or `id`, like `nest` resolution.
fn find_composition<'c>(
    compositions: &'c [notation::model::Composition],
    want: &str,
) -> Option<(usize, &'c notation::model::Composition)> {
    compositions
        .iter()
        .enumerate()
        .find(|(_, c)| c.name == want || c.id.as_deref() == Some(want))
}

/// Escape one string for JSON.
fn json_escape(value: &str) -> String {
    crate::studio::json_escape(value)
}

/// Render one cards load as JSON for the compositions API.
pub fn cards_json(load: &CardsLoad) -> String {
    let mut cards = String::new();
    for (i, card) in load.cards.iter().enumerate() {
        if i > 0 {
            cards.push(',');
        }
        let id = match &card.id {
            Some(id) => format!("\"{}\"", json_escape(id)),
            None => "null".to_string(),
        };
        cards.push_str(&format!(
            "{{\"name\":\"{}\",\"id\":{},\"root\":{},\"lint\":{},\"clips\":{},\"frames\":{},\"duration\":{},\"fps\":{},\"canvas\":[{},{}]}}",
            json_escape(&card.name),
            id,
            card.is_root,
            card.lint_count,
            card.clip_count,
            card.frame_count,
            card.duration_secs,
            card.fps_value,
            card.width,
            card.height
        ));
    }
    let notice = match &load.notice {
        Some(n) => format!("\"{}\"", json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"generation\":{},\"notice\":{},\"unattributed_lint\":{},\"cards\":[{}]}}",
        load.generation, notice, load.unattributed_lint, cards
    )
}

/// One drag-or-add placement request (Sprint 18, T001): which
/// composition lands on which timeline, at what film instant, under
/// whose name, and which card generation the maker acted on.
#[derive(Debug, Clone)]
pub struct PlaceRequest {
    /// Composition to place, by `name` or `id`.
    pub source: String,
    /// Timeline to place onto, by `name` or `id`; the render root
    /// when omitted.
    pub target: Option<String>,
    /// Film instant the nest opens at; the target timeline end when
    /// omitted (add appends, drag passes the drop point).
    pub at: Option<f64>,
    /// Actor the validated turn records.
    pub actor: String,
    /// Card generation the maker acted on; `None` skips the staleness
    /// guard (headless callers that just listed).
    pub expected_generation: Option<u64>,
    /// Timeline seating cap for this call; [`MAX_TIMELINE_CLIPS`]
    /// when omitted.
    pub max_clips: Option<usize>,
}

/// Receipt of one applied placement (Sprint 18, T001): the recorded
/// history turn behind the landed `nest` clip.
#[derive(Debug, Clone)]
pub struct PlaceReceipt {
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Timeline the nest landed on.
    pub target: String,
    /// Composition the nest plays.
    pub source: String,
    /// Film instant the nest opens at.
    pub at: f64,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Why a placement refused (Sprint 18, T001 / REC-001): either the
/// cards went stale — carrying fresh cards to repaint — or the
/// placement cannot land, naming the cause. Both stage nothing.
#[derive(Debug, Clone)]
pub enum PlaceError {
    /// The generation moved under the maker: repaint `fresh` and let
    /// the maker retry — nothing staged.
    Stale { message: String, fresh: CardsLoad },
    /// The placement cannot land: unknown composition, self-nest,
    /// cycle, depth, full timeline, or a failed patch — nothing
    /// staged, queued, or recorded.
    Refused(String),
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaceError::Stale { message, .. } => write!(f, "{message}"),
            PlaceError::Refused(message) => write!(f, "{message}"),
        }
    }
}

/// Nest edges of validated compositions: `edges[user]` holds every
/// composition index the user nests, resolved like `nest` (by `name`
/// or `id`), in document order.
fn nest_edges(compositions: &[notation::model::Composition]) -> Vec<Vec<usize>> {
    let mut edges = vec![Vec::new(); compositions.len()];
    for (user, comp) in compositions.iter().enumerate() {
        for clip in &comp.clips {
            let notation::model::Clip::Nest(nest) = clip else {
                continue;
            };
            if let Some((target, _)) = find_composition(compositions, &nest.target) {
                if !edges[user].contains(&target) {
                    edges[user].push(target);
                }
            }
        }
    }
    edges
}

/// One reachability path from `from` to `to` through `edges`, naming
/// composition names in order — `None` while `to` is unreachable.
fn nest_path(
    compositions: &[notation::model::Composition],
    edges: &[Vec<usize>],
    from: usize,
    to: usize,
) -> Option<Vec<String>> {
    let mut prev: Vec<Option<usize>> = vec![None; compositions.len()];
    let mut seen = vec![false; compositions.len()];
    let mut queue = vec![from];
    seen[from] = true;
    while let Some(node) = queue.pop() {
        if node == to {
            let mut chain = vec![to];
            while let Some(p) = prev[chain[0]] {
                chain.insert(0, p);
            }
            return Some(
                chain
                    .iter()
                    .map(|&i| format!("{:?}", compositions[i].name))
                    .collect(),
            );
        }
        for next in edges[node].iter().rev() {
            if !seen[*next] {
                seen[*next] = true;
                prev[*next] = Some(node);
                queue.push(*next);
            }
        }
    }
    None
}

/// Longest nest edge-chain starting at `node` through `edges`
/// (Sprint 18, T001): edge count, mirroring the `MAX_NEST_DEPTH`
/// count validation refuses past. The graph is acyclic whenever this
/// runs (cycles refuse first), so the memo walk terminates.
fn nest_depth_from(edges: &[Vec<usize>], node: usize, memo: &mut [Option<usize>]) -> usize {
    if let Some(depth) = memo[node] {
        return depth;
    }
    // The graph is acyclic whenever this runs (the base project
    // validated and cycles refuse first), so plain recursion
    // terminates.
    let mut best = 0usize;
    for next in &edges[node] {
        best = best.max(nest_depth_from(edges, *next, memo) + 1);
    }
    memo[node] = Some(best);
    best
}

/// Guard the new `target -> source` nest edge against the validated
/// project (Sprint 18, T001): self-nests, cycles closing through the
/// new edge, and chains past [`MAX_NEST_DEPTH`] refuse naming the
/// chain and the limit — before anything stages, so a refused
/// placement can never break later validation.
fn guard_nest_edge(
    compositions: &[notation::model::Composition],
    target: usize,
    source: usize,
) -> Result<(), String> {
    let source_name = &compositions[source].name;
    let target_name = &compositions[target].name;
    if target == source {
        return Err(refused_compositions(format!(
            "composition {source_name:?} cannot nest into itself; pick another timeline for the drop. Nothing was staged or recorded."
        )));
    }
    let edges = nest_edges(compositions);
    if let Some(chain) = nest_path(compositions, &edges, source, target) {
        let mut cycle = chain;
        cycle.push(format!("{source_name:?}"));
        return Err(refused_compositions(format!(
            "placing {source_name:?} onto {target_name:?} would nest in a cycle ({}); a composition cannot nest itself through other compositions. Break the cycle first. Nothing was staged or recorded.",
            cycle.join(" -> ")
        )));
    }
    let mut with = edges.clone();
    with[target].push(source);
    let mut memo = vec![None; compositions.len()];
    let mut deepest = 0usize;
    for node in 0..compositions.len() {
        deepest = deepest.max(nest_depth_from(&with, node, &mut memo));
    }
    if deepest > notation::model::MAX_NEST_DEPTH {
        return Err(refused_compositions(format!(
            "placing {source_name:?} onto {target_name:?} would nest {} deep, past the {}-level limit; nesting deeper refuses. Flatten the chain first. Nothing was staged or recorded.",
            deepest,
            notation::model::MAX_NEST_DEPTH
        )));
    }
    Ok(())
}

/// Film end of one composition's clips (0 for an empty timeline):
/// where add lands when the request names no instant.
fn timeline_end(comp: &notation::model::Composition) -> f64 {
    let mut end = 0.0f64;
    for clip in &comp.clips {
        end = end.max(clip.start() + clip.duration());
    }
    if end.is_finite() && end > 0.0 {
        end
    } else {
        0.0
    }
}

/// Place one composition on a timeline (Sprint 18, T001): dragging a
/// card passes the drop instant, add omits it and appends. Every guard
/// runs before anything stages — stale generation (with fresh cards),
/// invalid project, unknown composition, self-nest, cycle, depth, and
/// the full-timeline seating cap — and the landing `nest` applies as
/// one [`sdk::EditOp::AddElement`] through the Sprint 15 patch and
/// history machinery, recording one named turn. A lint-failing source
/// still places (the card shows its count so the maker decides);
/// only render-one refuses on lint.
pub fn place_composition(folder: &Path, req: &PlaceRequest) -> Result<PlaceReceipt, PlaceError> {
    let load = list_composition_cards(folder);
    if let Some(expected) = req.expected_generation {
        if expected != load.generation {
            return Err(PlaceError::Stale {
                message: "the compositions panel changed under this drop (the project moved on); fresh cards are attached — repaint and retry the drop. Nothing was staged or recorded."
                    .to_string(),
                fresh: load,
            });
        }
    }
    if let Some(notice) = load.notice.clone() {
        return Err(PlaceError::Refused(refused_compositions(format!(
            "{notice} Nothing was staged or recorded."
        ))));
    }
    let verdict = validate::validate::validate_folder(folder);
    if verdict.status != validate::validate::Status::Valid {
        return Err(PlaceError::Refused(refused_compositions(format!(
            "the project {} cannot place compositions; fix the problems `check` reports and retry. Nothing was staged or recorded.",
            folder.display()
        ))));
    }
    let (source_idx, source) = find_composition(&verdict.compositions, req.source.trim())
        .ok_or_else(|| {
            let available: Vec<String> = verdict
                .compositions
                .iter()
                .map(|c| format!("{:?}", c.name))
                .collect();
            PlaceError::Refused(refused_compositions(format!(
                "unknown composition {:?}: the project holds {}; pass one of those names. Nothing was staged or recorded.",
                req.source.trim(),
                available.join(", ")
            )))
        })?;
    let source_name = source.name.clone();
    let source_duration = source.duration;
    let target_want = req
        .target
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| verdict.root_name.clone())
        .ok_or_else(|| {
            PlaceError::Refused(refused_compositions(
                "the project names no render root; mark one composition `root=true` and retry. Nothing was staged or recorded."
                    .to_string(),
            ))
        })?;
    let (target_idx, target) = find_composition(&verdict.compositions, &target_want).ok_or_else(|| {
        let available: Vec<String> = verdict
            .compositions
            .iter()
            .map(|c| format!("{:?}", c.name))
            .collect();
        PlaceError::Refused(refused_compositions(format!(
            "unknown timeline {:?}: the project holds {}; drop onto one of those. Nothing was staged or recorded.",
            target_want,
            available.join(", ")
        )))
    })?;
    let target_name = target.name.clone();
    guard_nest_edge(&verdict.compositions, target_idx, source_idx).map_err(PlaceError::Refused)?;
    let cap = req.max_clips.unwrap_or(MAX_TIMELINE_CLIPS);
    // The seating check reads the effective timeline (validated base
    // with the persisted delta replayed), so landed turns count toward
    // full before anything new stages. The probe opens read-only with
    // no history, so every refusal path below writes nothing anywhere.
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
        PlaceError::Refused(refused_compositions(format!(
            "the timeline {target_name:?} cannot open for the drop: {e} Nothing was staged or recorded."
        )))
    })?;
    let seated = probe.effective_composition().clips.len();
    if seated >= cap {
        return Err(PlaceError::Refused(refused_compositions(format!(
            "the {target_name:?} timeline is full: it holds {seated} clip(s) at the {cap}-clip seating cap; remove a clip and retry the drop. Nothing was staged or recorded."
        ))));
    }
    let at = req
        .at
        .unwrap_or_else(|| timeline_end(probe.effective_composition()));
    drop(probe);
    let actor = req.actor.trim().to_string();
    if actor.is_empty() {
        return Err(PlaceError::Refused(refused_compositions(
            "the drop names no actor; placements record named turns. Pass a non-empty `actor`. Nothing was staged or recorded."
                .to_string(),
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
        PlaceError::Refused(refused_compositions(format!(
            "the timeline {target_name:?} cannot open for the drop: {e} Nothing was staged or recorded."
        )))
    })?;
    let op = sdk::EditOp::AddElement {
        element: sdk::NewElement::Nest {
            id: None,
            label: None,
            target: source_name.clone(),
            start: at,
            duration: source_duration,
        },
        position: None,
    };
    let applied = session.apply(op).map_err(|e| {
        PlaceError::Refused(refused_compositions(format!(
            "{e} Nothing was persisted or recorded."
        )))
    })?;
    let summaries = vec![applied.op.summary()];
    match session.persist_turn(&actor) {
        Ok(turn) => Ok(PlaceReceipt {
            turn: turn.turn.unwrap_or(0),
            target: target_name,
            source: source_name,
            at,
            summaries,
        }),
        Err(e) => Err(PlaceError::Refused(refused_compositions(format!(
            "{e} Staged work stays preserved unapplied — nothing was persisted or recorded."
        )))),
    }
}

/// Render one placement receipt as JSON for the compositions API.
pub fn place_receipt_json(receipt: &PlaceReceipt) -> String {
    format!(
        "{{\"placed\":true,\"turn\":{},\"target\":\"{}\",\"source\":\"{}\",\"at\":{},\"summaries\":[{}]}}",
        receipt.turn,
        json_escape(&receipt.target),
        json_escape(&receipt.source),
        receipt.at,
        receipt
            .summaries
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Render one stale refusal as JSON: the notice plus fresh cards to
/// repaint, so the maker retries on current truth.
pub fn stale_json(message: &str, fresh: &CardsLoad) -> String {
    format!(
        "{{\"stale\":true,\"notice\":\"{}\",\"cards\":{}}}",
        json_escape(message),
        cards_json(fresh)
    )
}

/// Parse one placement body
/// (`{"actor":"maker","source":"b-roll","target":"main","at":1.5,"generation":42}`):
/// `actor` and `source` must be non-empty; `target` defaults to the
/// render root, `at` to the timeline end, `generation` guards
/// staleness when present. A refused body names its cause and stages
/// nothing.
pub fn parse_place_body(body: &str) -> Result<PlaceRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_compositions(
            "the compositions drop needs a JSON body (`{\"actor\":\"maker\",\"source\":\"...\"}`); pass the dragged card from the panel. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_compositions(format!(
            "the compositions drop body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"source\":\"...\"}}`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_compositions(
            "the compositions drop names no actor; placements record named turns. Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let source = value
        .get("source")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if source.is_empty() {
        return Err(refused_compositions(
            "the compositions drop names no `source`; pass the dragged card's composition name. Nothing was staged.".to_string(),
        ));
    }
    let target = value
        .get("target")
        .and_then(|t| t.as_str())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let at = match value.get("at") {
        None => None,
        Some(v) => Some(v.as_f64().ok_or_else(|| {
            refused_compositions(
                "the compositions drop `at` is not seconds; pass a number (the drop instant) or omit it to append. Nothing was staged.".to_string(),
            )
        })?),
    };
    let expected_generation = match value.get("generation") {
        None => None,
        Some(v) => Some(v.as_u64().ok_or_else(|| {
            refused_compositions(
                "the compositions drop `generation` is not an integer; pass the card generation from the panel, or omit it. Nothing was staged.".to_string(),
            )
        })?),
    };
    Ok(PlaceRequest {
        source,
        target,
        at,
        actor,
        expected_generation,
        max_clips: None,
    })
}

/// One queued render-one job (Sprint 18, T001): exactly one
/// composition, queued by name. The Renders panel (T006) owns
/// execution, progress, and download; this slice owns truthful
/// queueing — one composition in, one job out, outputs planned
/// outside the project sources.
///
/// Execution settings and progress (Sprint 18, T006) extend the same
/// record with `#[serde(default)]` fields, so jobs T001 wrote still
/// read (render-one defaults: the composition's own fps at `source`
/// size, `standard` quality, software encode, forever-looping when a
/// GIF), and T001 readers ignore the newer fields outright.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenderJob {
    /// Job sequence in this queue (1-based, append-only).
    pub seq: u64,
    /// Composition this job renders, by `name`.
    pub composition: String,
    /// Job state (`queued` from this slice; execution states belong
    /// to the Renders panel: `queued`, `running`, `succeeded`,
    /// `failed`, `cancelled`).
    pub status: String,
    /// Planned output path, under the hidden renders directory.
    pub output: String,
    /// Actor that queued the job.
    pub actor: String,
    /// Attributed lint count at queue time (always 0: lint-failing
    /// compositions never queue).
    pub lint: usize,
    /// Queue format (`mp4`, `mov`, `webm`, or `gif`); `mp4` when an
    /// older writer planned no format.
    #[serde(default = "default_job_format")]
    pub format: String,
    /// Quality preset (`draft`, `standard`, or `high`).
    #[serde(default = "default_job_quality")]
    pub quality: String,
    /// Frame rate override (24, 30, or 60); `None` renders the
    /// composition's own rate.
    #[serde(default)]
    pub fps: Option<u32>,
    /// Resolution preset (`auto`, `1080p`, or `4k`); `auto` keeps the
    /// canvas size.
    #[serde(default = "default_job_resolution")]
    pub resolution: String,
    /// True while the GPU toggle was on (hardware encode when the
    /// host provides it, software with a note otherwise).
    #[serde(default)]
    pub gpu: bool,
    /// Job kind (`video`, or `gif` for a GIF export).
    #[serde(default = "default_job_kind")]
    pub kind: String,
    /// GIF loop count (`0` means forever); refused on video jobs.
    #[serde(default)]
    pub loop_count: u32,
    /// Execution stage (`queued`, `ffmpeg`, `validate`, `prepare`,
    /// `audio`, `frames`, `encode`, `output`, `done`).
    #[serde(default = "default_job_stage")]
    pub stage: String,
    /// Measured frames piped to the encoder so far.
    #[serde(default)]
    pub done_frames: u64,
    /// Total frames this job encodes.
    #[serde(default)]
    pub total_frames: u64,
    /// Epoch milliseconds the run started (ETA math); 0 while queued.
    #[serde(default)]
    pub started_ms: u64,
    /// Measured run milliseconds once terminal; 0 while active.
    #[serde(default)]
    pub elapsed_ms: u64,
    /// Failure or cancel report naming the stage; `None` on success.
    #[serde(default)]
    pub error: Option<String>,
    /// Notes the run recorded (fps clamp, software fallback, GIF
    /// stamp limit), oldest first.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Delivered output bytes once succeeded; `None` before that.
    #[serde(default)]
    pub output_bytes: Option<u64>,
}

/// Default queue format for jobs older writers planned.
fn default_job_format() -> String {
    "mp4".to_string()
}

/// Default quality preset for jobs older writers planned.
fn default_job_quality() -> String {
    "standard".to_string()
}

/// Default resolution preset for jobs older writers planned.
fn default_job_resolution() -> String {
    "auto".to_string()
}

/// Default job kind for jobs older writers planned.
fn default_job_kind() -> String {
    "video".to_string()
}

/// Default execution stage for jobs older writers planned.
fn default_job_stage() -> String {
    "queued".to_string()
}

/// The persisted render queue (Sprint 18, T001): versioned, one entry
/// per queued render-one in queue order. Jobs never rewrite: queueing
/// appends, execution updates states (T006), nothing else writes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RenderQueueFile {
    version: u32,
    #[serde(default)]
    jobs: Vec<RenderJob>,
}

/// Atomically persist `raw` at `path`: write a sibling temp file and
/// rename it over the target, so a concurrent reader (for example a
/// cancel handler racing a progress tick) never observes a truncated
/// queue.
pub(crate) fn write_queue_file(path: &Path, raw: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(raw.as_bytes())?;
        file.sync_all()?;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Read the persisted render queue: missing file means an empty queue
/// (no banner); a corrupt or version-wrong file refuses naming the
/// file, never guessed or clobbered.
fn read_render_queue(folder: &Path) -> Result<Vec<RenderJob>, String> {
    let path = render_queue_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(refused_compositions(format!(
                "the render queue {} cannot be read ({e}); fix or remove it and retry. Nothing was queued.",
                path.display()
            )));
        }
        Ok(raw) => raw,
    };
    let file: RenderQueueFile = serde_json::from_str(&raw).map_err(|e| {
        refused_compositions(format!(
            "the render queue {} is not a render queue ({e}); fix or remove it and retry. Nothing was queued.",
            path.display()
        ))
    })?;
    if file.version != RENDER_QUEUE_VERSION {
        return Err(refused_compositions(format!(
            "the render queue {} carries version {}, this Studio reads version {}; fix or remove it and retry. Nothing was queued.",
            path.display(),
            file.version,
            RENDER_QUEUE_VERSION
        )));
    }
    Ok(file.jobs)
}

/// One render-one request (Sprint 18, T001): which composition queues,
/// under whose name, and which card generation the maker acted on.
#[derive(Debug, Clone)]
pub struct RenderOneRequest {
    /// Composition to queue, by `name` or `id`.
    pub composition: String,
    /// Actor that queues the job.
    pub actor: String,
    /// Card generation the maker acted on; `None` skips the staleness
    /// guard (headless callers that just listed).
    pub expected_generation: Option<u64>,
}

/// Receipt of one queued render-one (Sprint 18, T001): the appended
/// job behind exactly the requested composition.
#[derive(Debug, Clone)]
pub struct RenderOneReceipt {
    /// Job sequence in the queue.
    pub seq: u64,
    /// Composition the job renders.
    pub composition: String,
    /// Planned output path, under the hidden renders directory.
    pub output: String,
}

/// Why render-one refused (Sprint 18, T001 / REC-001): either the
/// cards went stale — carrying fresh cards to repaint — or the job
/// cannot queue, naming the cause. Both queue nothing and render
/// nothing.
#[derive(Debug, Clone)]
pub enum RenderOneError {
    /// The generation moved under the maker: repaint `fresh` and let
    /// the maker retry — nothing queued.
    Stale { message: String, fresh: CardsLoad },
    /// The job cannot queue: unknown composition, lint failure, or an
    /// unreadable queue — nothing queued and nothing rendered.
    Refused(String),
}

impl std::fmt::Display for RenderOneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderOneError::Stale { message, .. } => write!(f, "{message}"),
            RenderOneError::Refused(message) => write!(f, "{message}"),
        }
    }
}

/// Queue exactly one composition for render (Sprint 18, T001):
/// render-one from its card. Guards run before anything queues —
/// stale generation (with fresh cards), invalid project, unknown
/// composition — and a composition carrying lint findings queues
/// nothing and renders nothing, naming the failing count on its card.
/// The appended job plans its output under the hidden renders
/// directory, outside the project sources like CLI renders. Panel
/// browsing and queueing never write pixels: no output exists until
/// the queue runs.
pub fn queue_render_one(
    folder: &Path,
    req: &RenderOneRequest,
) -> Result<RenderOneReceipt, RenderOneError> {
    let load = list_composition_cards(folder);
    if let Some(expected) = req.expected_generation {
        if expected != load.generation {
            return Err(RenderOneError::Stale {
                message: "the compositions panel changed under this render-one (the project moved on); fresh cards are attached — repaint and retry. Nothing was queued or rendered."
                    .to_string(),
                fresh: load,
            });
        }
    }
    if let Some(notice) = load.notice.clone() {
        return Err(RenderOneError::Refused(refused_compositions(format!(
            "{notice} Nothing was queued or rendered."
        ))));
    }
    let card = load
        .cards
        .iter()
        .find(|c| c.name == req.composition.trim() || c.id.as_deref() == Some(req.composition.trim()))
        .cloned()
        .ok_or_else(|| {
            let available: Vec<String> = load
                .cards
                .iter()
                .map(|c| format!("{:?}", c.name))
                .collect();
            RenderOneError::Refused(refused_compositions(format!(
                "unknown composition {:?}: the project holds {}; queue one of those. Nothing was queued or rendered.",
                req.composition.trim(),
                available.join(", ")
            )))
        })?;
    if card.lint_count > 0 {
        return Err(RenderOneError::Refused(refused_compositions(format!(
            "composition {:?} carries {} lint finding(s) and cannot render-one; fix the findings the card names and retry. Nothing was queued or rendered.",
            card.name, card.lint_count
        ))));
    }
    let actor = req.actor.trim().to_string();
    if actor.is_empty() {
        return Err(RenderOneError::Refused(refused_compositions(
            "render-one names no actor; queued jobs record who queued them. Pass a non-empty `actor`. Nothing was queued or rendered."
                .to_string(),
        )));
    }
    let mut jobs = read_render_queue(folder).map_err(RenderOneError::Refused)?;
    let seq = jobs.iter().map(|j| j.seq).max().unwrap_or(0) + 1;
    let output = renders_dir_for(folder)
        .join(format!("{}-{}.mp4", card.name, seq))
        .display()
        .to_string();
    jobs.push(RenderJob {
        seq,
        composition: card.name.clone(),
        status: "queued".to_string(),
        output: output.clone(),
        actor,
        lint: 0,
        format: default_job_format(),
        quality: default_job_quality(),
        fps: None,
        resolution: default_job_resolution(),
        gpu: false,
        kind: default_job_kind(),
        loop_count: 0,
        stage: default_job_stage(),
        done_frames: 0,
        total_frames: 0,
        started_ms: 0,
        elapsed_ms: 0,
        error: None,
        notes: Vec::new(),
        output_bytes: None,
    });
    let path = render_queue_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            RenderOneError::Refused(refused_compositions(format!(
                "the hidden queue directory {} cannot be prepared: {e}. Nothing was queued or rendered.",
                parent.display()
            )))
        })?;
    }
    let raw = serde_json::to_string_pretty(&RenderQueueFile {
        version: RENDER_QUEUE_VERSION,
        jobs,
    })
    .map_err(|e| {
        RenderOneError::Refused(refused_compositions(format!(
            "the render queue cannot be encoded: {e}. Nothing was queued or rendered."
        )))
    })?;
    write_queue_file(&path, &raw).map_err(|e| {
        RenderOneError::Refused(refused_compositions(format!(
            "the render queue {} cannot be written: {e}. Nothing was queued or rendered.",
            path.display()
        )))
    })?;
    Ok(RenderOneReceipt {
        seq,
        composition: card.name,
        output,
    })
}

/// List the queued render-one jobs in queue order (Sprint 18, T001):
/// the same file [`queue_render_one`] appends to, so the panel and
/// the Renders panel read one truth.
pub fn list_render_queue(folder: &Path) -> Result<Vec<RenderJob>, String> {
    read_render_queue(folder)
}

/// Render one render-one receipt as JSON for the compositions API.
pub fn render_one_receipt_json(receipt: &RenderOneReceipt) -> String {
    format!(
        "{{\"queued\":true,\"seq\":{},\"composition\":\"{}\",\"output\":\"{}\",\"lint\":0}}",
        receipt.seq,
        json_escape(&receipt.composition),
        json_escape(&receipt.output)
    )
}

/// Render one queued job as JSON (Sprint 18, T001/T006): the six
/// queueing fields plus every execution setting and progress field,
/// so the compositions readout and the Renders panel read one truth.
pub fn render_job_json(job: &RenderJob) -> String {
    let fps = match job.fps {
        Some(f) => f.to_string(),
        None => "null".to_string(),
    };
    let error = match &job.error {
        Some(e) => format!("\"{}\"", json_escape(e)),
        None => "null".to_string(),
    };
    let bytes = match job.output_bytes {
        Some(b) => b.to_string(),
        None => "null".to_string(),
    };
    let notes = job
        .notes
        .iter()
        .map(|n| format!("\"{}\"", json_escape(n)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"seq\":{},\"composition\":\"{}\",\"status\":\"{}\",\"output\":\"{}\",\"actor\":\"{}\",\"lint\":{},\"format\":\"{}\",\"quality\":\"{}\",\"fps\":{},\"resolution\":\"{}\",\"gpu\":{},\"kind\":\"{}\",\"loop_count\":{},\"stage\":\"{}\",\"done_frames\":{},\"total_frames\":{},\"started_ms\":{},\"elapsed_ms\":{},\"error\":{},\"notes\":[{}],\"output_bytes\":{}}}",
        job.seq,
        json_escape(&job.composition),
        json_escape(&job.status),
        json_escape(&job.output),
        json_escape(&job.actor),
        job.lint,
        json_escape(&job.format),
        json_escape(&job.quality),
        fps,
        json_escape(&job.resolution),
        job.gpu,
        json_escape(&job.kind),
        job.loop_count,
        json_escape(&job.stage),
        job.done_frames,
        job.total_frames,
        job.started_ms,
        job.elapsed_ms,
        error,
        notes,
        bytes
    )
}

/// Render the queued jobs as JSON for the compositions API.
pub fn render_queue_json(jobs: &[RenderJob]) -> String {
    let mut out = String::new();
    for (i, job) in jobs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&render_job_json(job));
    }
    format!("{{\"jobs\":[{out}]}}")
}

/// Parse one render-one body
/// (`{"actor":"maker","composition":"main","generation":42}`):
/// `actor` and `composition` must be non-empty; `generation` guards
/// staleness when present. A refused body names its cause and queues
/// nothing.
pub fn parse_render_one_body(body: &str) -> Result<RenderOneRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_compositions(
            "render-one needs a JSON body (`{\"actor\":\"maker\",\"composition\":\"...\"}`); pass the card from the panel. Nothing was queued.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_compositions(format!(
            "the render-one body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"composition\":\"...\"}}`. Nothing was queued."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_compositions(
            "render-one names no actor; queued jobs record who queued them. Pass a non-empty `actor`. Nothing was queued.".to_string(),
        ));
    }
    let composition = value
        .get("composition")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if composition.is_empty() {
        return Err(refused_compositions(
            "render-one names no `composition`; pass the card's composition name. Nothing was queued.".to_string(),
        ));
    }
    let expected_generation = match value.get("generation") {
        None => None,
        Some(v) => Some(v.as_u64().ok_or_else(|| {
            refused_compositions(
                "the render-one `generation` is not an integer; pass the card generation from the panel, or omit it. Nothing was queued.".to_string(),
            )
        })?),
    };
    Ok(RenderOneRequest {
        composition,
        actor,
        expected_generation,
    })
}

/// Draw the live hover preview of one composition (Sprint 18, T001):
/// frame 0 through the same pipeline as a full render on the CPU
/// backend, so the thumbnail shows exactly what the render opens
/// with. Opens a throwaway session per call — always current, never
/// cached — with the disk cache outside the project, so panel
/// browsing writes nothing into the project. An unrunnable preview
/// names its cause and draws nothing.
pub fn composition_preview_png(folder: &Path, name: &str) -> Result<Vec<u8>, String> {
    let want = name.trim();
    if want.is_empty() {
        return Err(refused_compositions(
            "the hover preview names no composition; pass the card's composition name. Nothing was drawn.".to_string(),
        ));
    }
    let mut session = crate::PreviewSession::open(
        folder,
        crate::PreviewOptions {
            composition: Some(want.to_string()),
            backend: render::render::Backend::Cpu,
            ..crate::PreviewOptions::default()
        },
    )
    .map_err(|e| {
        refused_compositions(format!(
            "the hover preview cannot open {want:?}: {e} Nothing was drawn."
        ))
    })?;
    session.frame_png(0).map(|frame| frame.png).map_err(|e| {
        refused_compositions(format!(
            "the hover preview cannot draw {want:?} frame 0: {e} Nothing was drawn."
        ))
    })
}

/// Extra CSS the compositions panel needs (Sprint 18, T001): the
/// card list, the root badge, the lint count, and the hover preview.
/// Style attributes stay CSS-like hyphenated end to end.
pub fn compositions_css() -> String {
    r#"        #panel-compositions{margin-top:8px}
        #composition-list{display:flex;gap:8px;flex-wrap:wrap;margin-top:8px}
        .composition-card{position:relative;border:1px solid var(--line-color);border-radius:8px;padding:8px 12px;min-width:180px;max-width:240px;background-color:var(--panel-color);color:var(--ink-color)}
        .composition-card h4{margin:4px 0;font-variant-numeric:tabular-nums}
        .composition-card .root-badge{display:inline-block;background-color:var(--accent-color);border:1px solid var(--line-color);border-radius:4px;padding:0 6px;margin:2px}
        .composition-card .lint-count{font-variant-numeric:tabular-nums}
        .composition-card .lint-count[data-failing="1"]{background-color:var(--banner-background);color:var(--banner-color);border:1px solid var(--line-color);border-radius:4px;padding:0 6px}
        .composition-card img.hover-preview{display:none;position:absolute;left:100%;top:0;width:192px;background-color:#000;object-fit:contain;border:1px solid var(--line-color);border-radius:4px;z-index:5}
        .composition-card:hover img.hover-preview{display:block}
        .composition-card button{margin:2px 4px 2px 0}
        #composition-status{font-variant-numeric:tabular-nums}"#.to_string()
}

/// The compositions control block (Sprint 18, T001): the card list
/// the panel script paints from the compositions API, with a status
/// line reporting placements, queued jobs, and refusals.
pub fn compositions_html() -> String {
    r#"<section class="panel" id="panel-compositions" aria-label="compositions panel">
        <h2>compositions</h2>
        <div id="composition-list"><p>loading compositions…</p></div>
        <p id="composition-status" role="status">compositions ready</p>
        </section>"#
        .to_string()
}

/// The compositions panel script (Sprint 18, T001): paints cards with
/// root badges and lint counts, shows the live hover preview, drags
/// cards onto the timeline at the drop instant, adds at the timeline
/// end, queues render-one per card, and repaints on stale refusals —
/// every placement landing as a named history turn.
pub fn compositions_js() -> String {
    r#"        let compositionGeneration = null;
        function compositionStatus(t) { const s = document.getElementById('composition-status'); if (s) s.textContent = t; }
        async function paintCompositions() {
          const box = document.getElementById('composition-list'); if (!box) return;
          let j = null;
          try {
            const r = await fetch('/api/studio/compositions');
            if (!r.ok) { box.textContent = await r.text(); return; }
            j = await r.json();
          } catch (e) { box.textContent = 'compositions unavailable: ' + e; return; }
          compositionGeneration = j.generation;
          box.innerHTML = '';
          if (j.notice) { const p = document.createElement('p'); p.setAttribute('role', 'alert'); p.textContent = j.notice; box.appendChild(p); }
          for (const card of (j.cards || [])) {
            const el = document.createElement('div'); el.className = 'composition-card'; el.draggable = true;
            el.title = 'drag onto the timeline to place, or use add';
            const h = document.createElement('h4'); h.textContent = card.name; el.appendChild(h);
            if (card.root) { const b = document.createElement('span'); b.className = 'root-badge'; b.textContent = 'root'; el.appendChild(b); }
            const lint = document.createElement('span'); lint.className = 'lint-count';
            lint.dataset.failing = card.lint > 0 ? '1' : '0';
            lint.textContent = 'lint ' + card.lint;
            lint.title = card.lint > 0 ? (card.lint + ' lint finding(s): render-one refuses until fixed') : 'no lint findings';
            el.appendChild(lint);
            const meta = document.createElement('p'); meta.textContent = card.clips + ' clip(s) · ' + card.frames + ' frames';
            el.appendChild(meta);
            const img = document.createElement('img'); img.className = 'hover-preview';
            img.alt = 'hover preview of ' + card.name;
            img.loading = 'lazy';
            img.dataset.preview = card.name;
            el.appendChild(img);
            el.addEventListener('mouseenter', () => {
              if (!img.src) img.src = '/api/studio/compositions/preview?composition=' + encodeURIComponent(card.name) + '&generation=' + String(compositionGeneration);
            });
            el.addEventListener('dragstart', (e) => { e.dataTransfer.setData('text/studio-composition', card.name); });
            const add = document.createElement('button'); add.type = 'button'; add.textContent = 'add';
            add.title = 'place at the timeline end';
            add.addEventListener('click', () => postCompositionPlace({source: card.name}));
            const render = document.createElement('button'); render.type = 'button'; render.textContent = 'render one';
            render.title = 'queue exactly this composition';
            render.addEventListener('click', () => postCompositionRenderOne(card.name));
            el.append(add, render);
            box.appendChild(el);
          }
          if (j.unattributed_lint > 0) compositionStatus(j.cards.length + ' composition(s), +' + j.unattributed_lint + ' project-level lint note(s)');
          else compositionStatus(j.cards.length + ' composition(s)');
        }
        async function postCompositionPlace(payload) {
          try {
            const body = Object.assign({actor: 'maker', generation: compositionGeneration}, payload);
            const r = await fetch('/api/studio/compositions/place', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
            const t = await r.text();
            if (!r.ok) {
              try {
                const j = JSON.parse(t);
                if (j.stale && j.cards) { compositionGeneration = j.cards.generation; paintCompositions(); }
              } catch (e) {}
              compositionStatus(t); banner(t); toast(t); return;
            }
            const j = JSON.parse(t);
            compositionStatus('placed ' + j.source + ' onto ' + j.target + ' at ' + Number(j.at).toFixed(3) + 's (turn ' + j.turn + ')');
            toast('placed ' + j.source + ' (turn ' + j.turn + '): ' + (j.summaries || []).join('; '));
            paintCompositions(); paintRows();
          } catch (e) { compositionStatus('place failed: ' + e); }
        }
        async function postCompositionRenderOne(name) {
          try {
            const r = await fetch('/api/studio/compositions/render-one', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({actor: 'maker', composition: name, generation: compositionGeneration})});
            const t = await r.text();
            if (!r.ok) {
              try {
                const j = JSON.parse(t);
                if (j.stale && j.cards) { compositionGeneration = j.cards.generation; paintCompositions(); }
              } catch (e) {}
              compositionStatus(t); banner(t); toast(t); return;
            }
            const j = JSON.parse(t);
            compositionStatus('queued ' + j.composition + ' as job ' + j.seq);
            toast('queued ' + j.composition + ' (job ' + j.seq + ')');
            paintCompositions();
          } catch (e) { compositionStatus('render-one failed: ' + e); }
        }
        function wireCompositionDrop() {
          const panel = document.getElementById('panel-timeline'); if (!panel || panel.dataset.compositionsWired) return;
          panel.dataset.compositionsWired = '1';
          panel.addEventListener('dragover', (e) => e.preventDefault());
          panel.addEventListener('drop', (e) => {
            const name = e.dataTransfer.getData('text/studio-composition');
            if (!name) return;
            e.preventDefault();
            const rect = panel.getBoundingClientRect();
            const frac = rect.width > 0 ? (e.clientX - rect.left) / rect.width : 0;
            const at = Math.max(0, frac * (timelineDuration() || 0));
            postCompositionPlace({source: name, at: at});
          });
        }
        wireCompositionDrop(); paintCompositions();"#
        .to_string()
}
