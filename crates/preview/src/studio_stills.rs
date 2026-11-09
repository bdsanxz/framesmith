//! Storyboard approval gate (Sprint 20, T004 / CAP-004, J-02,
//! STATE-04, REC-002).
//!
//! One board of stills in one page format is presented before
//! animation; the film stays byte-identical until the maker approves;
//! the Studio stills toggle approves stills before motion starts;
//! locked frames persist for resume; previews never play posed stills
//! as a slideshow; a missing approval state always reads as not
//! approved; building early is refused pointing at the pending gate.
//!
//! The contract, per entry point:
//!
//! * Presenting a board only reads the film (stills render from the
//!   current compositions) and writes presentation sidecars plus the
//!   hidden gate state. Composition sources, the SDK delta, and render
//!   outputs stay untouched, so the film is byte-identical before
//!   approval. Every new board re-arms the gate (`approved: false`).
//! * The gate arms only once a board exists. Projects without a board
//!   behave exactly as before: renders and agent edits run, so default
//!   renders stay byte-identical and existing projects are unaffected.
//! * While a board awaits approval, agent composition writes and renders
//!   refuse naming the pending gate (the Studio stills toggle or
//!   `storyboard approve`). Reads, stills, and maker inspection stay
//!   open: nothing may animate or render, but everything may be seen.
//! * A missing (or corrupt) approval state always reads as not
//!   approved, never as approved. A corrupt state file arms the gate
//!   and names the file with its fix (present a fresh board); the file
//!   itself is kept for inspection.
//! * Approval and board writes travel the shared Sprint 15 patch and
//!   history machinery through [`sdk::record_file_turn`]: every call
//!   returns a staged receipt naming what applied plus the recorded
//!   history turn, so the maker undoes gate turns like any turn. Gate
//!   turns change no tracked film file, so undoing one leaves film
//!   bytes exactly as they were.
//! * The board page is stills only: one HTML page with still images and
//!   captions. No video element, no playback script, no slideshow —
//!   posed stills never play as motion.
//!
//! State lives in `<project>/.framesmith/storyboard.json` (beside the
//! layout file and the work area, never beside the sources). Render,
//! check, and validation never read it.
//!
//! Request keys stay hyphenated (`approved`, `locked-frames`); style
//! attributes stay CSS-like hyphenated end to end and `font-family`
//! try-order lists pass through untouched, exactly like every other
//! read and write path.

use std::path::{Path, PathBuf};

/// Gate state file name inside the hidden project directory (next to
/// the layout file and the work area, never beside the sources).
pub const STORYBOARD_FILE: &str = "storyboard.json";

/// Version of the persisted gate state.
pub const STORYBOARD_VERSION: u32 = 1;

/// Hidden gate state path for one project folder.
pub fn storyboard_path_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(STORYBOARD_FILE)
}

/// Prefix every gate refusal carries; the cause follows it, and nothing
/// stages, renders, or records.
fn refused_gate(message: String) -> String {
    format!("storyboard refused: {message}")
}

/// One locked still: the film instant, its frame index, a caption, and
/// the still file name inside the board output directory.
#[derive(Debug, Clone, PartialEq)]
pub struct LockedFrame {
    /// Film seconds the still was drawn from.
    pub at: f64,
    /// Frame index the still was drawn from.
    pub frame: u64,
    /// Caption shown under the still on the board page.
    pub label: String,
    /// Still file name (relative to the board output directory).
    pub still: String,
}

/// The persisted gate state: one board of locked stills plus whether
/// the maker has approved them.
#[derive(Debug, Clone, PartialEq)]
pub struct StoryboardState {
    /// State version this build reads and writes.
    pub version: u32,
    /// Composition the board was presented from.
    pub composition: String,
    /// Board title shown on the one-page presentation.
    pub title: String,
    /// Locked stills, in presentation order.
    pub board: Vec<LockedFrame>,
    /// True once the maker approves the stills (Studio stills toggle
    /// or `storyboard approve`).
    pub approved: bool,
}

/// What loading found: the parsed state when a board was presented
/// (plus whether the file existed), or a notice naming why the gate
/// reads as not approved.
#[derive(Debug, Clone)]
pub struct StoryboardLoad {
    /// Parsed state when the file exists and parses.
    pub state: Option<StoryboardState>,
    /// True when the state file exists (even when it fails to parse).
    pub restored: bool,
    /// Why the gate reads as not approved (missing or corrupt file).
    pub notice: Option<String>,
}

/// Escape one string for HTML text and attribute contexts.
fn html_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Render one locked still as JSON: the film instant, the frame, the
/// caption, and the still file.
fn locked_frame_json(frame: &LockedFrame) -> String {
    format!(
        "{{\"at\":{},\"frame\":{},\"label\":\"{}\",\"still\":\"{}\"}}",
        frame.at,
        frame.frame,
        crate::studio::json_escape(&frame.label),
        crate::studio::json_escape(&frame.still),
    )
}

/// Encode the gate state as pretty JSON for the hidden state file.
fn encode_state(state: &StoryboardState) -> String {
    let board = state
        .board
        .iter()
        .map(locked_frame_json)
        .collect::<Vec<_>>()
        .join(",\n    ");
    format!(
        "{{\n  \"version\": {},\n  \"composition\": \"{}\",\n  \"title\": \"{}\",\n  \"approved\": {},\n  \"board\": [\n    {}\n  ]\n}}\n",
        state.version,
        crate::studio::json_escape(&state.composition),
        crate::studio::json_escape(&state.title),
        state.approved,
        board,
    )
}

/// Read one required text field from a JSON object; missing or
/// non-text values refuse naming the field.
fn need_text(value: &serde_json::Value, field: &str) -> Result<String, String> {
    match value.get(field).and_then(|v| v.as_str()) {
        Some(s) => Ok(s.to_string()),
        None => Err(format!("field {field:?} is missing or not text")),
    }
}

/// Parse the hidden gate state; a missing file reads as no board (the
/// gate stays idle), while a present-but-unusable file reads as a
/// notice with the file kept for inspection.
pub fn load_storyboard(folder: &Path) -> StoryboardLoad {
    let path = storyboard_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return StoryboardLoad {
                state: None,
                restored: false,
                notice: None,
            };
        }
        Err(e) => {
            return StoryboardLoad {
                state: None,
                restored: true,
                notice: Some(format!(
                    "the storyboard gate {} cannot be read ({e}); it reads as not approved",
                    path.display()
                )),
            };
        }
        Ok(raw) => raw,
    };
    match parse_state(&raw) {
        Ok(state) => StoryboardLoad {
            state: Some(state),
            restored: true,
            notice: None,
        },
        Err(why) => StoryboardLoad {
            state: None,
            restored: true,
            notice: Some(format!(
                "the storyboard gate {} is not a gate state ({why}); it reads as not approved — present a fresh board to replace it",
                path.display()
            )),
        },
    }
}

/// Parse gate state JSON: the version must match, the board must carry
/// at least one locked still, and `approved` must be a boolean (a
/// missing `approved` reads as not approved, never as approved).
fn parse_state(raw: &str) -> Result<StoryboardState, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("not JSON ({e})"))?;
    let version = value
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "its `version` is missing or not a number".to_string())?;
    if version as u32 != STORYBOARD_VERSION {
        return Err(format!(
            "it carries version {version}, this Studio reads version {STORYBOARD_VERSION}"
        ));
    }
    let composition = need_text(&value, "composition")?;
    let title = need_text(&value, "title")?;
    let approved = value
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let board_raw = value
        .get("board")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "its `board` is missing or not a list".to_string())?;
    if board_raw.is_empty() {
        return Err("its `board` names no locked stills".to_string());
    }
    let mut board = Vec::with_capacity(board_raw.len());
    for entry in board_raw {
        let at = entry
            .get("at")
            .and_then(|v| v.as_f64())
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or_else(|| {
                "a locked still `at` is missing or not film seconds at or after 0".to_string()
            })?;
        let frame = entry
            .get("frame")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "a locked still `frame` is missing or not a frame index".to_string())?;
        let label = need_text(entry, "label")?;
        let still = need_text(entry, "still")?;
        if still.trim().is_empty() || still.contains('/') || still.contains('\\') {
            return Err("a locked still `still` must name a still file, not a path".to_string());
        }
        board.push(LockedFrame {
            at,
            frame,
            label,
            still,
        });
    }
    Ok(StoryboardState {
        version: STORYBOARD_VERSION,
        composition,
        title,
        board,
        approved,
    })
}

/// Whether the maker has approved the presented stills. A missing (or
/// corrupt) approval state always reads as not approved, never as
/// approved — and with no board presented the gate stays idle.
pub fn approval_state(folder: &Path) -> bool {
    load_storyboard(folder)
        .state
        .is_some_and(|state| state.approved)
}

/// The pending gate, when a presented board still awaits maker
/// approval: the composition, the board title, and the locked stills.
/// `None` when no board is presented (the gate stays idle) or the
/// stills are approved (the gate has landed).
pub fn pending_gate(folder: &Path) -> Option<StoryboardState> {
    match load_storyboard(folder).state {
        Some(state) if !state.approved => Some(state),
        _ => None,
    }
}

/// Refuse early builds while a presented board awaits maker approval:
/// agent composition writes and renders name the pending gate and its
/// fix, and stage, queue, or render nothing. `origin` selects the
/// refusal voice (`agent`, `studio`, or `render`). `None` when the
/// gate is idle or has landed.
pub fn gate_refusal(folder: &Path, origin: &str) -> Option<String> {
    let load = load_storyboard(folder);
    let state = match load.state {
        Some(state) if !state.approved => state,
        Some(_) => return None,
        None if !load.restored => return None,
        None => {
            let notice = load
                .notice
                .unwrap_or_else(|| "it reads as not approved".to_string());
            let core = format!(
                "the storyboard gate holds: {notice}; present a fresh board before motion or render"
            );
            return Some(match origin {
                "render" => format!("render refused: {core}. No MP4 was written."),
                "studio" => format!("studio refused: {core}. Nothing was staged or recorded."),
                _ => format!("agent refused: {core}. Nothing was staged or recorded."),
            });
        }
    };
    let core = format!(
        "the storyboard gate holds: one board of {} locked still(s) ({}) awaits maker approval; approve the stills first (`storyboard approve`, or the Studio stills toggle) before motion or render",
        state.board.len(),
        state.title,
    );
    Some(match origin {
        "render" => format!("render refused: {core}. No MP4 was written."),
        "studio" => format!("studio refused: {core}. Nothing was staged or recorded."),
        _ => format!("agent refused: {core}. Nothing was staged or recorded."),
    })
}

/// One presented board: the locked stills plus the shared history turn
/// the write recorded.
#[derive(Debug, Clone)]
pub struct BoardReceipt {
    /// Composition the stills were drawn from.
    pub composition: String,
    /// Board title shown on the one-page presentation.
    pub title: String,
    /// Locked stills, in presentation order.
    pub frames: Vec<LockedFrame>,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// Sequence of the recorded history turn.
    pub turn: u64,
}

/// Present one board of locked stills (Sprint 20, T004): persist the
/// gate state (re-arming the gate: every new board starts unapproved)
/// and record the presentation as a shared history turn through
/// [`sdk::record_file_turn`]. The film is only read — composition
/// sources, the SDK delta, and history snapshots of film files stay
/// untouched, so the film is byte-identical before approval.
pub fn present_board(
    folder: &Path,
    actor: &str,
    composition: &str,
    title: &str,
    frames: Vec<LockedFrame>,
) -> Result<BoardReceipt, String> {
    if frames.is_empty() {
        return Err(refused_gate(
            "the board names no locked stills; pass at least one frame time to present. Nothing was staged or recorded."
                .to_string(),
        ));
    }
    for frame in &frames {
        if !frame.at.is_finite() || frame.at < 0.0 {
            return Err(refused_gate(format!(
                "the locked still {:?} `at` is not film seconds at or after 0; pass the instant it was drawn from. Nothing was staged or recorded.",
                frame.still
            )));
        }
        if frame.still.trim().is_empty() || frame.still.contains('/') || frame.still.contains('\\')
        {
            return Err(refused_gate(format!(
                "the locked still {:?} must name a still file, not a path; stills stay inside the board directory. Nothing was staged or recorded.",
                frame.still
            )));
        }
    }
    let title = title.trim();
    let title = if title.is_empty() {
        "Storyboard".to_string()
    } else {
        title.to_string()
    };
    let state = StoryboardState {
        version: STORYBOARD_VERSION,
        composition: composition.to_string(),
        title: title.clone(),
        board: frames.clone(),
        approved: false,
    };
    let raw = encode_state(&state);
    let path = storyboard_path_for(folder);
    let record = sdk::record_file_turn(folder, actor, || {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "the hidden storyboard directory {} cannot be prepared: {e}. Nothing was staged or recorded.",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&path, &raw).map_err(|e| {
            format!(
                "the storyboard gate {} cannot be written: {e}. Nothing was staged or recorded.",
                path.display()
            )
        })?;
        Ok((
            format!(
                "storyboard board presented: {} locked still(s) ({composition})",
                frames.len()
            ),
            frames.iter().map(|f| f.still.clone()).collect(),
        ))
    })
    .map_err(|h| refused_gate(format!("{h} Nothing was staged or recorded.")))?;
    Ok(BoardReceipt {
        composition: composition.to_string(),
        title,
        frames,
        actor: record.actor,
        turn: record.seq,
    })
}

/// One stills approval: the new gate position plus the shared history
/// turn the write recorded.
#[derive(Debug, Clone)]
pub struct ApprovalReceipt {
    /// Whether the stills now read as approved.
    pub approved: bool,
    /// Locked stills the decision covers.
    pub frames: usize,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// Sequence of the recorded history turn.
    pub turn: u64,
}

/// Approve (or re-open) the presented stills through the Studio stills
/// toggle or `storyboard approve`: flip the gate, persist it, and
/// record the decision as a shared history turn through
/// [`sdk::record_file_turn`]. Every call records a turn — even a
/// re-affirm — so the gate history stays auditable. Refuses when no
/// board is presented (present one first) or the state is corrupt
/// (kept for inspection; present a fresh board).
pub fn set_approval(folder: &Path, actor: &str, approved: bool) -> Result<ApprovalReceipt, String> {
    let load = load_storyboard(folder);
    let mut state = match load.state {
        Some(state) => state,
        None => {
            let why = load
                .notice
                .unwrap_or_else(|| "no storyboard board is presented".to_string());
            return Err(refused_gate(format!(
                "{why}; present a board first (`storyboard board`). Nothing was staged or recorded."
            )));
        }
    };
    state.approved = approved;
    let raw = encode_state(&state);
    let path = storyboard_path_for(folder);
    let frames = state.board.len();
    let record = sdk::record_file_turn(folder, actor, || {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "the hidden storyboard directory {} cannot be prepared: {e}. Nothing was staged or recorded.",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&path, &raw).map_err(|e| {
            format!(
                "the storyboard gate {} cannot be written: {e}. Nothing was staged or recorded.",
                path.display()
            )
        })?;
        let summary = if approved {
            format!("storyboard stills approved: {frames} locked still(s)")
        } else {
            format!("storyboard stills re-opened: {frames} locked still(s) await approval")
        };
        Ok((summary, vec![STORYBOARD_FILE.to_string()]))
    })
    .map_err(|h| refused_gate(format!("{h} Nothing was staged or recorded.")))?;
    Ok(ApprovalReceipt {
        approved,
        frames,
        actor: record.actor,
        turn: record.seq,
    })
}

/// Render the gate status as JSON: whether the stills read as
/// approved, the board title and composition, and the locked stills.
/// A missing (or corrupt) state reads as not approved with a notice
/// naming why — never as approved.
pub fn stills_status_json(folder: &Path) -> String {
    let load = load_storyboard(folder);
    match load.state {
        Some(state) => {
            let frames = state
                .board
                .iter()
                .map(locked_frame_json)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"stills\":true,\"presented\":true,\"approved\":{},\"composition\":\"{}\",\"title\":\"{}\",\"frames\":{},\"locked-frames\":[{}]}}",
                state.approved,
                crate::studio::json_escape(&state.composition),
                crate::studio::json_escape(&state.title),
                state.board.len(),
                frames,
            )
        }
        None => {
            let notice = load.notice.unwrap_or_else(|| {
                "no storyboard board is presented; the gate reads as not approved".to_string()
            });
            format!(
                "{{\"stills\":true,\"presented\":false,\"approved\":false,\"frames\":0,\"locked-frames\":[],\"notice\":\"{}\"}}",
                crate::studio::json_escape(&notice),
            )
        }
    }
}

/// Render one board presentation receipt as JSON: the tool, the locked
/// stills, and the recorded history turn.
pub fn board_receipt_json(receipt: &BoardReceipt) -> String {
    let frames = receipt
        .frames
        .iter()
        .map(locked_frame_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"staged\":true,\"tool\":\"present-board\",\"actor\":\"{}\",\"composition\":\"{}\",\"title\":\"{}\",\"frames\":{},\"locked-frames\":[{}],\"turn\":{}}}",
        crate::studio::json_escape(&receipt.actor),
        crate::studio::json_escape(&receipt.composition),
        crate::studio::json_escape(&receipt.title),
        receipt.frames.len(),
        frames,
        receipt.turn,
    )
}

/// Render one stills approval receipt as JSON: the toggle position,
/// the locked stills it covers, and the recorded history turn.
pub fn approval_receipt_json(receipt: &ApprovalReceipt) -> String {
    format!(
        "{{\"staged\":true,\"tool\":\"approve-stills\",\"actor\":\"{}\",\"approved\":{},\"frames\":{},\"turn\":{}}}",
        crate::studio::json_escape(&receipt.actor),
        receipt.approved,
        receipt.frames,
        receipt.turn,
    )
}

/// Render the one-page board presentation (Sprint 20, T004): one HTML
/// page holding every locked still with its film time and frame. The
/// page is stills only — no video element, no playback script, no
/// slideshow: posed stills never play as motion.
pub fn render_board_page(
    title: &str,
    composition: &str,
    frames: &[LockedFrame],
    approved: bool,
) -> String {
    let mut cards = String::new();
    for frame in frames {
        cards.push_str(&format!(
            "<figure><img src=\"{}\" alt=\"{}\"><figcaption>{} — {:.3}s (frame {})</figcaption></figure>\n",
            html_escape(&frame.still),
            html_escape(&frame.label),
            html_escape(&frame.label),
            frame.at,
            frame.frame,
        ));
    }
    let gate = if approved {
        "Approved — motion and render may proceed."
    } else {
        "Awaiting maker approval — approve the stills (the Studio stills toggle, or `storyboard approve`) before motion or render."
    };
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\">\n<title>{}</title>\n<style>body{{font-family:sans-serif;margin:2em}}main{{display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:1.5em}}figure{{margin:0}}img{{width:100%;height:auto;display:block;border:1px solid #ccc}}figcaption{{font-size:.85em;margin-top:.4em}}</style>\n</head>\n<body>\n<h1>{}</h1>\n<p>Composition {} — {} locked still(s).</p>\n<p>{}</p>\n<main>\n{}</main>\n</body>\n</html>\n",
        html_escape(title),
        html_escape(title),
        html_escape(composition),
        frames.len(),
        html_escape(gate),
        cards,
    )
}
