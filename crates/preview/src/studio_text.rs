//! Inline text editing sessions for the Studio shell (Sprint 17, T006
//! / CAP-006, J-03, STATE-03, REC-001, REC-002).
//!
//! Double-clicking a text element opens a live session with bold,
//! italic, underline, and colour; typing previews against the open
//! composition without staging anything; Enter commits as exactly one
//! patch turn and Escape cancels with nothing staged. An invalid
//! value (for example an unknown font family) refuses with its reason
//! while the prior text stays exactly as it was.
//!
//! The canvas proposes, the SDK disposes: every committed change
//! applies through the Sprint 15 patch and history machinery
//! ([`crate::studio::apply_studio_edit`]), so text edits share one
//! undo system with gestures, rows, timeline, and headless edits. A
//! committed session lands as a named history turn with a receipt; a
//! refused, cancelled, or empty session stages nothing.
//!
//! Session boundary: `open` reads the live text (value, `font-style`
//! as bold/italic toggles, `color`, `font-family`); `preview`
//! validates the staged keystrokes and echoes the would-be text with
//! notes, staging nothing; `commit` writes only the rows the render
//! pipeline supports (`value`, `font-style`, `color`, `font-family`)
//! as one validated turn; `cancel` discards the staged keystrokes
//! with a note. Nothing here writes beside validated turns: the
//! session itself is request-scoped, so even the hidden project files
//! stay untouched until a commit records its turn.
//!
//! Style boundary: bold and italic map onto the `font-style` row
//! (`regular`, `bold`, `italic`, `bold-italic`), colour maps onto the
//! `color` row (`#rgb` or `#rrggbb`), and `font-family` try-order
//! lists pass through untouched with fail-closed resolution at
//! commit time, exactly like the SDK rows module. Underline is a
//! live-session toggle: it paints in the editing overlay and the
//! preview echo, and a commit with underline on says so in its notes
//! while the turn carries the render-supported rows — no render row
//! carries an underline rule today, so the turn never fakes one.
//!
//! Font boundary: the session offers only resolvable families — the
//! project-declared families plus the authored families of the open
//! composition that resolve through the same collection the commit
//! checks. A `font-family` list whose every entry fails to resolve
//! refuses naming the unknown families and the offered ones, keeping
//! the prior text; a mixed try-order list passes through in written
//! comma order (fail-closed: the first resolvable entry wins at
//! render time). Request keys and style attributes stay CSS-like
//! hyphenated end to end (`font-style`, `font-family`,
//! `offered-fonts`, `background-color`, `border-radius`).

use std::path::Path;

/// Why an inline text request refused: names the cause, stages
/// nothing, and keeps the prior text exactly as it was.
fn refused_text(message: String) -> String {
    format!("studio refused: {message}")
}

/// Render one color the way rows show it: lowercase `#rrggbb`.
fn rgb_text(c: &notation::model::Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Bold/italic toggles of one authored font style.
fn style_flags(style: notation::model::FontStyle) -> (bool, bool) {
    match style {
        notation::model::FontStyle::Regular => (false, false),
        notation::model::FontStyle::Bold => (true, false),
        notation::model::FontStyle::Italic => (false, true),
        notation::model::FontStyle::BoldItalic => (true, true),
    }
}

/// Authored `font-style` row of one bold/italic toggle pair, with its
/// served spelling.
fn font_style_of(bold: bool, italic: bool) -> (notation::model::FontStyle, &'static str) {
    match (bold, italic) {
        (false, false) => (notation::model::FontStyle::Regular, "regular"),
        (true, false) => (notation::model::FontStyle::Bold, "bold"),
        (false, true) => (notation::model::FontStyle::Italic, "italic"),
        (true, true) => (notation::model::FontStyle::BoldItalic, "bold-italic"),
    }
}

/// Families the project itself declares: the name table family of
/// each readable `font` file, falling back to the file stem when the
/// bytes carry no family name. Unreadable files contribute nothing —
/// the render pass refuses those with its own reason.
fn project_families(folder: &Path, comp: &notation::model::Composition) -> Vec<String> {
    let mut out = Vec::new();
    for decl in &comp.fonts {
        let entry = match std::fs::read(folder.join(&decl.path)) {
            Ok(bytes) => glyph::tables::family_name(&bytes, 0).or_else(|| {
                Path::new(&decl.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
            }),
            Err(_) => None,
        };
        if let Some(name) = entry {
            let name = name.trim().to_string();
            if !name.is_empty() && !out.iter().any(|s| s == &name) {
                out.push(name);
            }
        }
    }
    out
}

/// Every authored `font-family` entry of the open composition in
/// first-seen order: each text clip list, then every span list depth
/// first. Written order survives; empties never ride along.
fn authored_families(comp: &notation::model::Composition) -> Vec<String> {
    fn push(out: &mut Vec<String>, list: &Option<Vec<String>>) {
        if let Some(names) = list {
            for name in names {
                let name = name.trim().to_string();
                if !name.is_empty() && !out.iter().any(|s| s == &name) {
                    out.push(name);
                }
            }
        }
    }
    fn spans(out: &mut Vec<String>, runs: &[notation::model::TextSpan]) {
        for span in runs {
            push(out, &span.font_family);
            spans(out, &span.children);
        }
    }
    let mut out = Vec::new();
    for clip in &comp.clips {
        if let notation::model::Clip::Text(c) = clip {
            push(&mut out, &c.font_family);
            spans(&mut out, &c.spans);
        }
    }
    out
}

/// The font collection one session resolves against: the project
/// fonts registered under their own families (unreadable or
/// unsupported files stay out — the render pass refuses those), so
/// machine families join on demand exactly like the render pass.
fn session_fonts(folder: &Path, comp: &notation::model::Composition) -> layout::Fonts {
    let mut fonts = layout::Fonts::new();
    for decl in &comp.fonts {
        if let Ok(bytes) = std::fs::read(folder.join(&decl.path)) {
            let _ = fonts.register(bytes, None);
        }
    }
    fonts
}

/// Families the session offers: the project families first, then the
/// authored families that resolve through the session collection, in
/// first-seen order. Every offered family resolves by construction —
/// the commit check below runs against this same collection.
pub fn offered_fonts(folder: &Path, comp: &notation::model::Composition) -> Vec<String> {
    let mut fonts = session_fonts(folder, comp);
    let mut out = project_families(folder, comp);
    for name in authored_families(comp) {
        if !out.iter().any(|s| s == &name) && fonts.resolves_any(std::slice::from_ref(&name)) {
            out.push(name);
        }
    }
    out
}

/// Check one authored `font-family` try-order list: comma-separated
/// with every entry non-empty (like the SDK row), and at least one
/// entry resolving through the session collection. An empty entry
/// refuses naming the list; a list whose every entry fails refuses
/// naming the unknown families and the offered ones — the prior text
/// stays. A mixed list passes through in written comma order.
fn check_font_family(
    folder: &Path,
    comp: &notation::model::Composition,
    raw: &str,
) -> Result<Vec<String>, String> {
    let names: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).collect();
    if names.iter().any(|s| s.is_empty()) {
        return Err(refused_text(format!(
            "inline text `font-family` list {raw:?} carries an empty entry; write comma-separated family names like `\"Alpha, Beta\"`. Nothing was staged and the prior text stays."
        )));
    }
    let mut fonts = session_fonts(folder, comp);
    if !fonts.resolves_any(&names) {
        let offered = offered_fonts(folder, comp);
        let offered = if offered.is_empty() {
            "the session offers no resolvable family yet".to_string()
        } else {
            format!(
                "the session offers {}",
                offered
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        return Err(refused_text(format!(
            "inline text `font-family` list {raw:?} resolves to no known family; {offered}. Nothing was staged and the prior text stays."
        )));
    }
    Ok(names)
}

/// Parse one authored color exactly like the `color` row: `#rgb` or
/// `#rrggbb`, else a refusal naming the row that keeps the prior
/// text.
fn check_color(raw: &str) -> Result<notation::model::Rgb, String> {
    notation::model::parse_color(raw.trim()).ok_or_else(|| {
        refused_text(format!(
            "inline text `color` {raw:?} is not `#rgb` or `#rrggbb`; write a hex color. Nothing was staged and the prior text stays."
        ))
    })
}

/// Resolve one inline text target against the effective clips: `id`
/// first, then document index. Unknown targets refuse naming the
/// holdings; non-text clips refuse naming their kind (inline editing
/// is text-only); hidden text refuses naming the `visible` row that
/// hands the session back.
fn resolve_text_target(
    comp: &notation::model::Composition,
    target: &sdk::ClipRef,
) -> Result<usize, String> {
    let idx = if let Some(id) = &target.id {
        comp.clips
            .iter()
            .position(|c| c.id() == Some(id.as_str()))
            .ok_or_else(|| {
                refused_text(format!(
                    "inline text target clip id {id:?} names nothing; the open composition holds {} clip(s). Nothing was staged.",
                    comp.clips.len()
                ))
            })?
    } else if let Some(index) = target.index {
        if index >= comp.clips.len() {
            return Err(refused_text(format!(
                "inline text target clip index {index} is past the open composition holding {} clip(s). Nothing was staged.",
                comp.clips.len()
            )));
        }
        index
    } else {
        return Err(refused_text(
            "the inline text target names neither `id` nor `index`; sessions open on one text clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged."
                .to_string(),
        ));
    };
    let clip = &comp.clips[idx];
    if !matches!(clip, notation::model::Clip::Text(_)) {
        return Err(refused_text(format!(
            "inline text opens on text clips only; this {} keeps its prior value. Nothing was staged.",
            clip.kind()
        )));
    }
    if clip.hidden() {
        let key = clip
            .id()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("#{idx}"));
        return Err(refused_text(format!(
            "inline text on hidden {key} refuses: hidden elements show no session. Show the clip first (the `visible` row), then double-click it. Nothing was staged."
        )));
    }
    Ok(idx)
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

/// Read one selection list from a request value: a comma-separated
/// string or a JSON array of strings, trimming empties — the same
/// lenient echo canvas and timeline sync on.
fn parse_selection_value(value: &serde_json::Value) -> Result<Vec<String>, String> {
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
                    refused_text(format!(
                        "the inline text `selection` entry {item} is not a clip reference; pass declared `id` values or `#index` entries. Nothing was staged."
                    ))
                })?;
                if !name.trim().is_empty() {
                    out.push(name.trim().to_string());
                }
            }
            Ok(out)
        }
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err(refused_text(format!(
            "the inline text `selection` value {value} is not a clip list; pass declared `id` values or `#index` entries as a comma string or an array. Nothing was staged."
        ))),
    }
}

/// One parsed inline text request (Sprint 17, T006): `open` reads the
/// live session, `preview` validates staged keystrokes and echoes the
/// would-be text, `commit` records one validated turn, and `cancel`
/// discards the staged keystrokes. Style fields ride along only when
/// the maker set them — absent fields keep the live value.
#[derive(Debug, Clone)]
pub struct TextRequest {
    /// Session action (`open`, `preview`, `commit`, `cancel`).
    pub action: String,
    /// Clip the session addresses.
    pub target: sdk::ClipRef,
    /// Staged text (`None` while the keystrokes keep it).
    pub value: Option<String>,
    /// Staged bold toggle (`None` while untouched).
    pub bold: Option<bool>,
    /// Staged italic toggle (`None` while untouched).
    pub italic: Option<bool>,
    /// Staged underline toggle (`None` while untouched).
    pub underline: Option<bool>,
    /// Staged colour (`None` while untouched).
    pub color: Option<String>,
    /// Staged `font-family` list (`None` while untouched).
    pub font_family: Option<String>,
    /// Actor the committed turn is recorded under (commit only).
    pub actor: String,
    /// Shared selection the shell scoped the session to, echoed back
    /// so canvas and timeline show the same selection at once.
    pub selection: Vec<String>,
}

/// Read one style toggle: present booleans pass, anything else
/// refuses naming the key — a typo never silently restyles the text.
fn text_bool(value: &serde_json::Value, key: &str) -> Result<bool, String> {
    value.as_bool().ok_or_else(|| {
        refused_text(format!(
            "the inline text `{key}` value {value} is not a switch; pass `true` or omit it. Nothing was staged and the prior text stays."
        ))
    })
}

/// Read one staged text field: present strings pass, anything else
/// refuses naming the key.
fn text_string(value: &serde_json::Value, key: &str) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            refused_text(format!(
                "the inline text `{key}` value {value} is not text; pass a quoted string. Nothing was staged and the prior text stays."
            ))
        })
}

/// Parse one inline text body (`{"action":"open",
/// "target":{"id":"title"}}`, or `preview`/`commit` with staged
/// `value`, `bold`, `italic`, `underline`, `color`, and
/// `font-family`, or `cancel`). Unknown actions, missing targets,
/// and mistyped fields refuse naming the cause; nothing stages
/// either way. Committing needs a non-empty actor.
pub fn parse_text_body(body: &str) -> Result<TextRequest, String> {
    if body.trim().is_empty() {
        return Err(refused_text(
            "inline text needs a JSON session body (`{\"action\":\"open\",\"target\":{\"id\":\"title\"}}`); pass the session from the canvas panel. Nothing was staged."
                .to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_text(format!(
            "the inline text body is not JSON ({e}). Pass `{{\"action\":\"open\",\"target\":{{\"id\":\"title\"}}}}`. Nothing was staged."
        ))
    })?;
    if !value.is_object() {
        return Err(refused_text(
            "the inline text body is not a session; pass a JSON object with an `action`. Nothing was staged."
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
        "open" | "preview" | "commit" | "cancel" => {}
        _ => {
            return Err(refused_text(format!(
                "unknown inline text action {action:?}: open a session with `open`, stage keystrokes with `preview`, record them with `commit`, or drop them with `cancel`. Nothing was staged."
            )));
        }
    }
    let target_value = value.get("target").ok_or_else(|| {
        refused_text(
            "inline text carries no `target`; sessions open on one text clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged."
                .to_string(),
        )
    })?;
    let target: sdk::ClipRef = serde_json::from_value(target_value.clone()).map_err(|e| {
        refused_text(format!(
            "the inline text `target` is not a clip reference ({e}); pass `target:{{\"id\":\"...\"}}` or `target:{{\"index\":N}}`. Nothing was staged."
        ))
    })?;
    if target.id.is_none() && target.index.is_none() {
        return Err(refused_text(
            "the inline text `target` names neither `id` nor `index`; sessions open on one text clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged."
                .to_string(),
        ));
    }
    let opt_bool = |key: &str| -> Result<Option<bool>, String> {
        match value.get(key) {
            None => Ok(None),
            Some(v) => Ok(Some(text_bool(v, key)?)),
        }
    };
    let opt_string = |key: &str| -> Result<Option<String>, String> {
        match value.get(key) {
            None => Ok(None),
            Some(v) => Ok(Some(text_string(v, key)?)),
        }
    };
    let staged = TextRequest {
        action,
        target,
        value: opt_string("value")?,
        bold: opt_bool("bold")?,
        italic: opt_bool("italic")?,
        underline: opt_bool("underline")?,
        color: opt_string("color")?,
        font_family: opt_string("font-family")?,
        actor: value
            .get("actor")
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        selection: match value.get("selection") {
            None => Vec::new(),
            Some(v) => parse_selection_value(v)?,
        },
    };
    if staged.action == "commit" && staged.actor.is_empty() {
        return Err(refused_text(
            "the inline text commit names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged and the prior text stays."
                .to_string(),
        ));
    }
    Ok(staged)
}

/// The live text one session echoes: the value with its bold,
/// italic, underline, colour, `font-style`, and `font-family`
/// readings, exactly as the commit would carry them.
#[derive(Debug, Clone)]
pub struct TextEcho {
    /// Session key of the clip.
    pub key: String,
    /// Live or would-be text.
    pub value: String,
    /// Bold toggle.
    pub bold: bool,
    /// Italic toggle.
    pub italic: bool,
    /// Underline toggle (live-session preview).
    pub underline: bool,
    /// Colour as lowercase `#rrggbb`.
    pub color: String,
    /// `font-style` row spelling the toggles map onto.
    pub font_style: String,
    /// `font-family` try-order list in written comma order.
    pub font_family: Vec<String>,
    /// Families the session offers (every one resolvable).
    pub offered: Vec<String>,
}

/// Echo the live text of one resolved clip: the value with the
/// `font-style` row read back as bold/italic toggles, the colour,
/// and the `font-family` list in written order.
fn live_echo(
    folder: &Path,
    comp: &notation::model::Composition,
    idx: usize,
    key: &str,
) -> TextEcho {
    let (value, (bold, italic), color, family) = match &comp.clips[idx] {
        notation::model::Clip::Text(c) => (
            c.value.clone(),
            style_flags(c.style),
            rgb_text(&c.color),
            c.font_family.clone().unwrap_or_default(),
        ),
        _ => unreachable!("target resolves to text"),
    };
    let (_, style) = font_style_of(bold, italic);
    TextEcho {
        key: key.to_string(),
        value,
        bold,
        italic,
        underline: false,
        color,
        font_style: style.to_string(),
        font_family: family,
        offered: offered_fonts(folder, comp),
    }
}

/// Validate one staged session against the live clip and build the
/// patch operations a commit records: `value`, `font-style`,
/// `color`, and `font-family` rows for the fields that differ, in
/// that order, so one turn reads in session order. Invalid staged
/// values refuse with their reason keeping the prior text.
/// Underline previews only and rides the notes, never the ops — no
/// render row carries it. An unchanged stage carries no ops; the
/// commit path refuses those naming the clean state instead of
/// recording an empty turn, while previews echo them freely.
fn text_edit_ops(
    folder: &Path,
    comp: &notation::model::Composition,
    req: &TextRequest,
    idx: usize,
) -> Result<(Vec<sdk::EditOp>, Vec<String>, TextEcho), String> {
    let key = clip_key(comp, idx);
    let live = live_echo(folder, comp, idx, &key);
    let spans_empty = match &comp.clips[idx] {
        notation::model::Clip::Text(c) => c.spans.is_empty(),
        _ => unreachable!("target resolves to text"),
    };
    let next_value = req.value.clone().unwrap_or_else(|| live.value.clone());
    if next_value.is_empty() && spans_empty {
        return Err(refused_text(format!(
            "inline text `value` on {key} is empty and the clip carries no `span` runs; a text clip needs a `value` or spans. Write quoted text. Nothing was staged and the prior text stays."
        )));
    }
    let next_bold = req.bold.unwrap_or(live.bold);
    let next_italic = req.italic.unwrap_or(live.italic);
    let (_, next_style) = font_style_of(next_bold, next_italic);
    let next_underline = req.underline.unwrap_or(false);
    let next_color = match &req.color {
        None => live.color.clone(),
        Some(raw) => rgb_text(&check_color(raw)?),
    };
    let next_family = match &req.font_family {
        None => live.font_family.clone(),
        Some(raw) if raw.trim().is_empty() => Vec::new(),
        Some(raw) => check_font_family(folder, comp, raw)?,
    };
    let mut ops = Vec::new();
    if next_value != live.value {
        ops.push(sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "value".to_string(),
            value: next_value.clone(),
        });
    }
    if next_style != live.font_style {
        ops.push(sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "font-style".to_string(),
            value: next_style.to_string(),
        });
    }
    if next_color != live.color {
        ops.push(sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "color".to_string(),
            value: next_color.clone(),
        });
    }
    if next_family != live.font_family {
        ops.push(sdk::EditOp::SetRow {
            target: req.target.clone(),
            property: "font-family".to_string(),
            value: next_family.join(", "),
        });
    }
    let mut notes = Vec::new();
    if next_underline {
        notes.push(
            "underline previews in the live session; no render row carries it, so the turn records the render-supported rows".to_string(),
        );
    }
    let echo = TextEcho {
        key,
        value: next_value,
        bold: next_bold,
        italic: next_italic,
        underline: next_underline,
        color: next_color,
        font_style: next_style.to_string(),
        font_family: next_family,
        offered: live.offered,
    };
    Ok((ops, notes, echo))
}

/// Render one text echo as JSON: the value with its bold, italic,
/// underline, and colour toggles, the `font-style` row they map
/// onto, the `font-family` try-order list, and the offered families.
/// Every key is hyphenated.
fn echo_json(echo: &TextEcho, notes: &[String]) -> String {
    let family = echo
        .font_family
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let offered = echo
        .offered
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    let notes_json = notes
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "\"target\":\"{}\",\"value\":\"{}\",\"bold\":{},\"italic\":{},\"underline\":{},\"color\":\"{}\",\"font-style\":\"{}\",\"font-family\":[{}],\"offered-fonts\":[{}],\"notes\":[{}]",
        crate::studio::json_escape(&echo.key),
        crate::studio::json_escape(&echo.value),
        echo.bold,
        echo.italic,
        echo.underline,
        crate::studio::json_escape(&echo.color),
        crate::studio::json_escape(&echo.font_style),
        family,
        offered,
        notes_json,
    )
}

/// Shared selection echo so canvas and timeline show the same
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

/// Open one inline session on the effective composition: the live
/// text with its style toggles, the offered families, and the synced
/// selection echo so canvas and timeline show the same selection at
/// once. Opening stages nothing and records no turn.
pub fn open_session(
    folder: &Path,
    comp: &notation::model::Composition,
    req: &TextRequest,
    kept: &[String],
    dropped: &[String],
) -> Result<String, String> {
    let idx = resolve_text_target(comp, &req.target)?;
    let key = clip_key(comp, idx);
    let echo = live_echo(folder, comp, idx, &key);
    Ok(format!(
        "{{\"staged\":false,\"committed\":false,\"cancelled\":false,\"action\":\"open\",{},\"detail\":\"inline text session on {key} is open; type with bold, italic, underline, and colour, then commit or cancel.\",\"selection\":{}}}",
        echo_json(&echo, &[]),
        selection_echo_json(kept, dropped),
        key = crate::studio::json_escape(&key),
    ))
}

/// Preview one staged session on the effective composition: validate
/// the keystrokes and echo the would-be text with notes. Previews
/// stage nothing and record no turn; an invalid staged value refuses
/// with its reason keeping the prior text.
pub fn preview_session(
    folder: &Path,
    comp: &notation::model::Composition,
    req: &TextRequest,
    kept: &[String],
    dropped: &[String],
) -> Result<String, String> {
    let idx = resolve_text_target(comp, &req.target)?;
    let (ops, notes, echo) = text_edit_ops(folder, comp, req, idx)?;
    Ok(format!(
        "{{\"staged\":false,\"committed\":false,\"cancelled\":false,\"action\":\"preview\",{},\"applied\":{},\"detail\":\"inline text preview on {}; {} change(s) staged in the echo only, nothing was applied or recorded.\",\"selection\":{}}}",
        echo_json(&echo, &notes),
        ops.len(),
        crate::studio::json_escape(&echo.key),
        ops.len(),
        selection_echo_json(kept, dropped),
    ))
}

/// One applied inline text turn.
#[derive(Debug, Clone)]
pub struct TextReceipt {
    /// Patch operations the turn recorded.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// The would-be text the turn recorded.
    pub echo: TextEcho,
    /// Session notes (underline previews only, ...).
    pub notes: Vec<String>,
}

/// Commit one staged session through the Sprint 15 patch and history
/// machinery: validate the keystrokes, then record one named turn
/// under the actor — the same undo system as gestures, rows,
/// timeline, and headless edits. A refused value names its rule and
/// location while the composition, the delta, and history stay
/// exactly as they were; an unchanged session records no turn.
pub fn apply_text_edit(
    folder: &Path,
    composition: Option<&str>,
    req: &TextRequest,
) -> Result<TextReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_text(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    let shown = crate::studio::effective_composition_for_session(folder, composition)?;
    let idx = resolve_text_target(&shown, &req.target)?;
    let (ops, notes, echo) = text_edit_ops(folder, &shown, req, idx)?;
    if ops.is_empty() {
        return Err(refused_text(format!(
            "inline text on {} changed nothing; the session holds no edits. Type, restyle, or recolor first — an empty session records no turn. Nothing was staged.",
            echo.key
        )));
    }
    let studio_edit = crate::studio::StudioEdit {
        actor: req.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops,
    };
    match crate::studio::apply_studio_edit(folder, composition, &studio_edit) {
        Ok(receipt) => Ok(TextReceipt {
            applied: receipt.applied,
            turn: receipt.turn,
            actor: receipt.actor,
            summaries: receipt.summaries,
            echo,
            notes,
        }),
        Err(message) => Err(message),
    }
}

/// Render one committed text receipt as JSON: the named turn with
/// its summaries, the recorded text, and the synced selection echo.
/// Every key is hyphenated.
pub fn text_receipt_json(receipt: &TextReceipt, kept: &[String], dropped: &[String]) -> String {
    let summaries = receipt
        .summaries
        .iter()
        .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"staged\":false,\"committed\":true,\"cancelled\":false,\"saved\":true,\"action\":\"commit\",\"actor\":\"{}\",\"applied\":{},\"turn\":{},\"summaries\":[{summaries}],{},\"detail\":\"inline text on {} committed (turn {})\", \"selection\":{}}}",
        crate::studio::json_escape(&receipt.actor),
        receipt.applied,
        receipt.turn,
        echo_json(&receipt.echo, &receipt.notes),
        crate::studio::json_escape(&receipt.echo.key),
        receipt.turn,
        selection_echo_json(kept, dropped),
    )
}

/// Render one cancelled session as JSON: the staged keystrokes
/// discard, nothing commits, and the answer says so.
pub fn text_cancelled_json(req: &TextRequest, kept: &[String], dropped: &[String]) -> String {
    format!(
        "{{\"staged\":false,\"committed\":false,\"cancelled\":true,\"action\":\"cancel\",\"detail\":\"inline text session on {} cancelled; the staged keystrokes discarded and nothing was recorded.\",\"selection\":{}}}",
        crate::studio::json_escape(&req.target.to_string()),
        selection_echo_json(kept, dropped),
    )
}

/// Extra CSS the inline text slice needs (Sprint 17, T006): the
/// session panel with its style toggles and the editing overlay that
/// paints the live preview. Style attributes stay CSS-like
/// hyphenated end to end (`background-color`, `border-radius`,
/// `font-weight`, `text-decoration`).
pub fn text_css() -> String {
    r#"        #canvas-text-bar{margin-top:8px}
        #canvas-text-session{list-style:none;margin:4px 0;padding:8px;background-color:var(--panel-color);border:1px solid var(--line-color);border-radius:4px}
        #canvas-text-edit{width:100%;box-sizing:border-box;background-color:var(--panel-color);color:var(--ink-color);border:1px solid var(--line-color);border-radius:4px;font-weight:400}
        #canvas-text-toggles{margin-top:8px;display:flex;gap:4px;align-items:center;flex-wrap:wrap}
        #canvas-text-toggles button{margin-right:4px}
        #canvas-text-toggles button[data-on="1"]{background-color:var(--accent-color)}
        #canvas-text-underline[data-on="1"]{text-decoration:underline}
        #canvas-text-status{font-variant-numeric:tabular-nums;z-index:1}"#.to_string()
}

/// The inline text control block (Sprint 17, T006): the session
/// panel with the value field, the bold/italic/underline/colour
/// toggles, the `font-family` picker over the offered families, and
/// the commit/cancel buttons with the session status line.
pub fn text_html() -> String {
    r##"<div class="dock-bar" id="canvas-text-bar">
        <span>text</span>
        <div id="canvas-text-session" hidden>
        <input id="canvas-text-edit" type="text" value="" aria-label="inline text value">
        <div id="canvas-text-toggles">
        <button id="canvas-text-bold" type="button" title="bold">B</button>
        <button id="canvas-text-italic" type="button" title="italic">I</button>
        <button id="canvas-text-underline" type="button" title="underline">U</button>
        <label>colour <input id="canvas-text-color" type="color" value="#ffffff"></label>
        <label>font <select id="canvas-text-font"></select></label>
        <button id="canvas-text-commit" type="button" title="commit (Enter)">commit</button>
        <button id="canvas-text-cancel" type="button" title="cancel (Escape)">cancel</button>
        </div>
        </div>
        </div>
        <p id="canvas-text-status" role="status">no text session open</p>"##
        .to_string()
}

/// The inline text panel script (Sprint 17, T006): double-clicking
/// the canvas with exactly one text element selected opens its
/// session; typing and the style toggles preview live through the
/// session API; Enter commits as one patch turn and Escape cancels
/// with nothing staged. Canvas and timeline selections stay in sync
/// through the shared `selection` array.
pub fn text_js() -> String {
    r#"        let textSession = null;
        function textStatus(t) { const s = document.getElementById('canvas-text-status'); if (s) s.textContent = t; }
        function textKeyOf(target) { return target.id ? {id: target.id} : {index: target.index}; }
        function textTargetOfKey(key) { const m = /^#(\d+)$/.exec(key); return m ? {index: Number(m[1])} : {id: key}; }
        function paintTextToggle(id, on) { const el = document.getElementById(id); if (el) el.dataset.on = on ? '1' : '0'; }
        function paintTextSession(j) {
          textSession = j;
          const box = document.getElementById('canvas-text-session');
          if (box) box.hidden = false;
          const edit = document.getElementById('canvas-text-edit');
          if (edit && document.activeElement !== edit) edit.value = j.value || '';
          paintTextToggle('canvas-text-bold', !!j.bold);
          paintTextToggle('canvas-text-italic', !!j.italic);
          paintTextToggle('canvas-text-underline', !!j.underline);
          const color = document.getElementById('canvas-text-color');
          if (color && j.color) color.value = j.color;
          const font = document.getElementById('canvas-text-font');
          if (font) {
            font.innerHTML = '';
            for (const fam of (j['offered-fonts'] || [])) {
              const opt = document.createElement('option'); opt.value = fam; opt.textContent = fam; font.appendChild(opt);
            }
            const cur = (j['font-family'] || []).join(', ');
            if (cur && !(j['offered-fonts'] || []).includes(cur)) {
              const opt = document.createElement('option'); opt.value = cur; opt.textContent = cur; font.appendChild(opt);
            }
            font.value = cur;
          }
          const overlay = document.getElementById('canvas-text-edit');
          if (overlay) {
            overlay.style.fontWeight = j.bold ? '700' : '400';
            overlay.style.fontStyle = j.italic ? 'italic' : 'normal';
            overlay.style.textDecoration = j.underline ? 'underline' : 'none';
            if (j.color) overlay.style.color = j.color;
          }
        }
        async function postText(payload) {
          try {
            const body = Object.assign({selection: selection.join(',')}, payload);
            const r = await fetch('/api/studio/text', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
            const t = await r.text();
            if (!r.ok) { textStatus(t); banner(t); return null; }
            return JSON.parse(t);
          } catch (e) { textStatus('text session failed: ' + e); return null; }
        }
        async function openTextSession() {
          if (selection.length !== 1) { textStatus('double-click text with exactly one element selected; selection holds ' + selection.length + '.'); return; }
          const j = await postText({action: 'open', target: textTargetOfKey(selection[0])});
          if (!j) return;
          if (j.selection && j.selection.kept) selection = j.selection.kept.slice();
          paintTextSession(j);
          textStatus(j.detail || 'text session open');
          const edit = document.getElementById('canvas-text-edit');
          if (edit) { edit.focus(); edit.select(); }
        }
        async function previewTextSession() {
          if (!textSession) return;
          const edit = document.getElementById('canvas-text-edit');
          const color = document.getElementById('canvas-text-color');
          const font = document.getElementById('canvas-text-font');
          const j = await postText({action: 'preview', target: textTargetOfKey(textSession.target), value: edit ? edit.value : textSession.value,
            bold: (document.getElementById('canvas-text-bold') || {}).dataset
              ? document.getElementById('canvas-text-bold').dataset.on === '1' : !!textSession.bold,
            italic: document.getElementById('canvas-text-italic').dataset.on === '1',
            underline: document.getElementById('canvas-text-underline').dataset.on === '1',
            color: color ? color.value : textSession.color, 'font-family': font ? font.value : (textSession['font-family'] || []).join(', ')});
          if (!j) return;
          if (j.selection && j.selection.kept) selection = j.selection.kept.slice();
          paintTextSession(j);
          textStatus(j.detail || 'text preview ready');
        }
        async function commitTextSession() {
          if (!textSession) { textStatus('no text session open; double-click a text element first.'); return; }
          const edit = document.getElementById('canvas-text-edit');
          const color = document.getElementById('canvas-text-color');
          const font = document.getElementById('canvas-text-font');
          const j = await postText({action: 'commit', actor: 'maker', target: textTargetOfKey(textSession.target), value: edit ? edit.value : textSession.value,
            bold: document.getElementById('canvas-text-bold').dataset.on === '1',
            italic: document.getElementById('canvas-text-italic').dataset.on === '1',
            underline: document.getElementById('canvas-text-underline').dataset.on === '1',
            color: color ? color.value : textSession.color, 'font-family': font ? font.value : (textSession['font-family'] || []).join(', ')});
          if (!j) return;
          if (j.selection && j.selection.kept) selection = j.selection.kept.slice();
          paintTextSession(j);
          textStatus((j.detail || 'text committed') + (j.turn !== undefined && j.turn !== null ? ' (turn ' + j.turn + ')' : ''));
          textSession = null;
          const box = document.getElementById('canvas-text-session');
          if (box) box.hidden = true;
          paintRows(); paintCanvasSelection(); syncLink(); paintLayers(); show(cur);
        }
        async function cancelTextSession() {
          if (!textSession) return;
          const j = await postText({action: 'cancel', target: textTargetOfKey(textSession.target)});
          if (j && j.selection && j.selection.kept) selection = j.selection.kept.slice();
          textSession = null;
          const box = document.getElementById('canvas-text-session');
          if (box) box.hidden = true;
          textStatus((j && j.detail) || 'text session cancelled');
        }
        function wireTextToggle(id, key) {
          const el = document.getElementById(id);
          if (el && !el.dataset.wired) {
            el.dataset.wired = '1';
            el.addEventListener('click', () => { el.dataset.on = el.dataset.on === '1' ? '0' : '1'; previewTextSession(); });
          }
        }
        wireTextToggle('canvas-text-bold'); wireTextToggle('canvas-text-italic'); wireTextToggle('canvas-text-underline');
        { const el = document.getElementById('canvas-text-edit');
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('input', () => previewTextSession()); } }
        { const el = document.getElementById('canvas-text-color');
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('input', () => previewTextSession()); } }
        { const el = document.getElementById('canvas-text-commit');
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('click', () => commitTextSession()); } }
        { const el = document.getElementById('canvas-text-cancel');
          if (el && !el.dataset.wired) { el.dataset.wired = '1'; el.addEventListener('click', () => cancelTextSession()); } }
        document.addEventListener('keydown', (e) => {
          if (!textSession) return;
          if (e.key === 'Enter' && !(e.shiftKey || e.ctrlKey || e.metaKey)) { e.preventDefault(); commitTextSession(); }
          else if (e.key === 'Escape') { e.preventDefault(); cancelTextSession(); }
        });
        { const view = document.getElementById('canvas-view');
          if (view && !view.dataset.textWired) { view.dataset.textWired = '1'; view.addEventListener('dblclick', () => openTextSession()); } }"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_comp() -> notation::model::Composition {
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
                Clip::Text(TextClip {
                    asserts: MotionAsserts::default(),
                    value: "Hi".to_string(),
                    start: 0.0,
                    duration: 4.0,
                    id: Some("title".to_string()),
                    track: TrackKind::Graphics,
                    label: None,
                    start_after: None,
                    x: 10.0,
                    y: 150.0,
                    size: 24.0,
                    color: Rgb {
                        r: 255,
                        g: 255,
                        b: 255,
                    },
                    align: Align::Left,
                    wrap: Wrap::Off,
                    style: FontStyle::Regular,
                    order: 0,
                    line_height: None,
                    letter_spacing: None,
                    font_family: None,
                    direction: TextDirection::Ltr,
                    spans: Vec::new(),
                    split: None,
                    typewriter: false,
                    count_from: None,
                    count_to: None,
                    decimals: 0,
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
                    line: 2,
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

    #[test]
    fn text_style_toggles_map_onto_the_font_style_row() {
        assert_eq!(
            font_style_of(false, false),
            (notation::model::FontStyle::Regular, "regular")
        );
        assert_eq!(
            font_style_of(true, false),
            (notation::model::FontStyle::Bold, "bold")
        );
        assert_eq!(
            font_style_of(false, true),
            (notation::model::FontStyle::Italic, "italic")
        );
        assert_eq!(
            font_style_of(true, true),
            (notation::model::FontStyle::BoldItalic, "bold-italic")
        );
        assert_eq!(
            style_flags(notation::model::FontStyle::BoldItalic),
            (true, true)
        );
        assert_eq!(
            style_flags(notation::model::FontStyle::Regular),
            (false, false)
        );
    }

    #[test]
    fn text_rejects_unknown_actions_and_mistyped_fields() {
        let bad =
            parse_text_body(r#"{"action":"type","target":{"id":"title"}}"#).expect_err("unknown");
        assert!(
            bad.contains("unknown inline text action"),
            "names the action: {bad}"
        );
        let bad = parse_text_body(r#"{"action":"open"}"#).expect_err("no target");
        assert!(bad.contains("no `target`"), "names the target: {bad}");
        let bad = parse_text_body(r#"{"action":"preview","target":{"id":"title"},"bold":"yes"}"#)
            .expect_err("bool");
        assert!(bad.contains("`bold`"), "names the toggle: {bad}");
        assert!(bad.contains("Nothing was staged"), "stages nothing: {bad}");
        let bad =
            parse_text_body(r#"{"action":"commit","target":{"id":"title"}}"#).expect_err("actor");
        assert!(bad.contains("no actor"), "names the actor: {bad}");
    }

    #[test]
    fn text_resolves_text_targets_only() {
        let comp = text_comp();
        assert_eq!(
            resolve_text_target(&comp, &sdk::ClipRef::by_id("title")).unwrap(),
            0
        );
        assert_eq!(
            resolve_text_target(&comp, &sdk::ClipRef::by_index(0)).unwrap(),
            0
        );
        let missing =
            resolve_text_target(&comp, &sdk::ClipRef::by_id("nobody")).expect_err("unknown");
        assert!(missing.contains("\"nobody\""), "names the key: {missing}");
        let boxy = resolve_text_target(&comp, &sdk::ClipRef::by_id("card")).expect_err("box");
        assert!(
            boxy.contains("text clips only"),
            "names the kind rule: {boxy}"
        );
    }

    #[test]
    fn text_hidden_clips_refuse_with_the_visible_row() {
        let mut comp = text_comp();
        if let notation::model::Clip::Text(c) = &mut comp.clips[0] {
            c.hidden = true;
        }
        let why = resolve_text_target(&comp, &sdk::ClipRef::by_id("title")).expect_err("hidden");
        assert!(why.contains("`visible`"), "names the row: {why}");
        assert!(why.contains("Nothing was staged"), "stages nothing: {why}");
    }

    #[test]
    fn text_colors_accept_hex_and_nothing_else() {
        assert_eq!(
            check_color("#fff").unwrap(),
            notation::model::Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(rgb_text(&check_color("#ABCDEF").unwrap()), "#abcdef");
        let bad = check_color("red").expect_err("named");
        assert!(bad.contains("`color`"), "names the row: {bad}");
        assert!(bad.contains("prior text stays"), "keeps prior text: {bad}");
    }

    #[test]
    fn text_font_lists_need_nonempty_entries() {
        let bad = check_font_family(Path::new("/nonexistent"), &text_comp(), "Alpha, , Beta")
            .expect_err("empty entry");
        assert!(bad.contains("empty entry"), "names the list fault: {bad}");
    }

    #[test]
    fn text_stage_diffs_only_changed_rows_and_previews_freely() {
        use std::env;
        let dir = env::temp_dir().join("framesmith-studio-text-stage");
        let _ = std::fs::create_dir_all(&dir);
        let comp = text_comp();
        // An untouched stage carries no ops but previews echo the live text.
        let req = parse_text_body(r#"{"action":"preview","target":{"id":"title"}}"#).unwrap();
        let idx = resolve_text_target(&comp, &req.target).unwrap();
        let (ops, _, echo) = text_edit_ops(&dir, &comp, &req, idx).unwrap();
        assert!(ops.is_empty(), "untouched stage carries no ops");
        assert_eq!(echo.value, "Hi");
        assert_eq!(echo.font_style, "regular");
        assert_eq!(echo.color, "#ffffff");
        assert!(!echo.underline, "underline starts off");
        // Bold plus a new value stage two ops in session order.
        let req = parse_text_body(
            r#"{"action":"preview","target":{"id":"title"},"value":"Yo","bold":true,"underline":true}"#,
        )
        .unwrap();
        let (ops, notes, echo) = text_edit_ops(&dir, &comp, &req, idx).unwrap();
        assert_eq!(ops.len(), 2, "value and font-style rows: {ops:?}");
        assert!(echo.bold && echo.underline);
        assert_eq!(echo.font_style, "bold");
        assert_eq!(notes.len(), 1, "underline previews with a note");
        assert!(
            notes[0].contains("live session"),
            "note names the boundary: {}",
            notes[0]
        );
    }

    #[test]
    fn text_empty_values_refuse_without_spans() {
        use std::env;
        let dir = env::temp_dir().join("framesmith-studio-text-empty");
        let _ = std::fs::create_dir_all(&dir);
        let comp = text_comp();
        let req =
            parse_text_body(r#"{"action":"preview","target":{"id":"title"},"value":""}"#).unwrap();
        let idx = resolve_text_target(&comp, &req.target).unwrap();
        let why = text_edit_ops(&dir, &comp, &req, idx).expect_err("empty");
        assert!(why.contains("`value`"), "names the row: {why}");
        assert!(why.contains("prior text stays"), "keeps prior text: {why}");
    }

    #[test]
    fn text_offered_fonts_resolve_through_the_session_collection() {
        use std::env;
        let dir = env::temp_dir().join("framesmith-studio-text-offered");
        let _ = std::fs::create_dir_all(&dir);
        let comp = text_comp();
        let offered = offered_fonts(&dir, &comp);
        let mut fonts = session_fonts(&dir, &comp);
        for family in &offered {
            assert!(
                fonts.resolves_any(std::slice::from_ref(family)),
                "offered family {family:?} resolves"
            );
        }
    }
}
