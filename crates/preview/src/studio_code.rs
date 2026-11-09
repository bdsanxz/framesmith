//! Code panel: file tree, notation editor, and source jump (Sprint 18,
//! T003 / CAP-003, J-03, REC-003).
//!
//! The maker browses the project file tree, edits notation, JSON, and
//! Markdown sources, and jumps from a canvas or timeline selection to
//! the exact source line — all inside the Studio shell, so the
//! notation stays a first-class editing surface instead of an escape
//! hatch. Tree actions (new, rename, duplicate, delete, drag into
//! folders) apply on disk; notation saves travel validated turns with
//! last-good preservation; jump lands on the selected element's
//! source line; binary files open as named read-only placeholders; a
//! save that breaks validation refuses with diagnostics keeping the
//! last good file; deleting a referenced file refuses naming its
//! referrers.
//!
//! Write boundary: only project writes are validated turns (through
//! [`sdk::record_file_turn`], the same guarded history machinery the
//! Sprint 15 patch persist records, so one undo system keeps serving
//! panel and headless edits alike) plus hidden studio state files —
//! the history index, guard, backups, and deltas under
//! `<project>/.framesmith`. Queue-style outputs never land beside
//! sources, and default renders (no SDK delta) replay to themselves,
//! so they stay byte-identical. Panel browsing never writes.
//!
//! Style boundary: style attributes stay CSS-like hyphenated end to
//! end, and `font-family` try-order lists pass through untouched —
//! unknown families stay fail-closed at render time, never resolved
//! or substituted here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The project root notation file every jump lands in.
pub const CODE_COMPOSITION_FILE: &str = "composition.kdl";

/// Starter body for a recreated root composition: braces ride on
/// their own lines, which the notation parser requires.
pub const CODE_COMPOSITION_STARTER: &str =
    "composition name=\"main\" width=320 height=180 duration=1 fps=30 background=\"#000000\" version=1 {\n}\n";

/// Starter body for a new JSON sidecar.
pub const CODE_JSON_STARTER: &str = "{}\n";

/// Starter body for a new Markdown note.
pub const CODE_MARKDOWN_STARTER: &str = "# Untitled\n";

/// Starter body for a recreated project config (the only config key
/// the validator accepts).
pub const CODE_CONFIG_STARTER: &str = "ffmpeg_auto_download = true\n";

/// Why a code-panel request refused.
fn refused_code(message: String) -> String {
    format!("studio refused: {message}")
}

/// Resolve a panel-supplied relative path inside the project
/// (Sprint 18, T003): the path must stay under the project folder —
/// absolute paths, `.`/`..` segments, and the hidden studio state
/// directory (`.framesmith`, owning layout, history, and deltas)
/// refuse fail-closed, so tree and editor writes can never escape
/// the project or touch hidden state directly.
pub fn resolve_code_path(folder: &Path, rel: &str) -> Result<PathBuf, String> {
    let trimmed = rel.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err(refused_code(
            "the code panel needs a project-relative file path (`composition.kdl`, `notes.md`); pass the file to open. Nothing changed."
                .to_string(),
        ));
    }
    if trimmed.starts_with('/') || Path::new(trimmed.as_str()).is_absolute() {
        return Err(refused_code(format!(
            "the code panel path {rel:?} is absolute; pass a project-relative path under {}. Nothing changed.",
            folder.display()
        )));
    }
    let stripped = trimmed.trim_matches('/');
    if stripped.is_empty() {
        return Err(refused_code(
            "the code panel needs a project-relative file path (`composition.kdl`, `notes.md`); pass the file to open. Nothing changed."
                .to_string(),
        ));
    }
    for segment in stripped.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(refused_code(format!(
                "the code panel path {rel:?} escapes the project; pass a path under {} with no `.` or `..` segments. Nothing changed.",
                folder.display()
            )));
        }
        if segment == sdk::HISTORY_DIR_NAME {
            return Err(refused_code(format!(
                "the code panel path {rel:?} names hidden studio state (`{}` holds layout, history, and deltas); the tree never edits it directly. Nothing changed.",
                sdk::HISTORY_DIR_NAME
            )));
        }
    }
    Ok(folder.join(stripped))
}

/// Project-relative display name with `/` separators.
fn rel_name(folder: &Path, path: &Path) -> String {
    path.strip_prefix(folder)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// Editor language hint by file name (Sprint 18, T003): the editor
/// carries syntax for notation, JSON, Markdown, and the project
/// config; every other file edits as plain text (binary content
/// still opens as a read-only placeholder at read time, whatever
/// the extension says).
pub fn language_of(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    if !lower.contains('.') {
        return "text";
    }
    match ext {
        "kdl" => "notation",
        "json" => "json",
        "md" | "markdown" | "mdown" => "markdown",
        "toml" => "toml",
        _ => "text",
    }
}

/// True while the file name addresses the project root composition.
fn is_composition_file(name: &str) -> bool {
    name == validate::validate::COMPOSITION_FILE
}

/// True while the file name addresses the project config.
fn is_config_file(name: &str) -> bool {
    name == notation::model::CONFIG_FILE
}

/// True while a save or tree write to this file must revalidate the
/// whole project: the root composition and the project config both
/// feed validation, so either can break it.
fn needs_project_validation(name: &str) -> bool {
    is_composition_file(name) || is_config_file(name)
}

/// True while history tracks this file: every `*.kdl` source (the
/// backups behind undo) plus the hidden SDK deltas. Only turns
/// touching tracked files record; other project files save without
/// a turn, exactly like hidden panel state.
fn is_tracked_file(name: &str) -> bool {
    name.ends_with(".kdl")
}

/// True while bytes read as binary: a NUL byte or invalid UTF-8, so
/// media and font files open as named read-only placeholders even
/// when their extension looks textual.
fn is_binary_bytes(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

/// One file-tree row (Sprint 18, T003): project files and folders in
/// path order, skipping the hidden studio state directory. `kind`
/// is `file` or `dir`; `language` is the editor hint (`dir` rows
/// carry `dir`); `size` is bytes (0 for folders).
#[derive(Debug, Clone)]
pub struct CodeEntry {
    /// Project-relative path with `/` separators.
    pub path: String,
    /// `file` or `dir`.
    pub kind: &'static str,
    /// Editor language hint (`notation`, `json`, `markdown`,
    /// `toml`, `text`; `dir` for folders).
    pub language: &'static str,
    /// Byte length (0 for folders).
    pub size: u64,
}

/// List the project file tree (Sprint 18, T003): every file and
/// folder under the project in path order, skipping hidden studio
/// state. Panel browsing never writes; an unreachable project
/// refuses naming the folder.
pub fn list_tree(folder: &Path) -> Result<Vec<CodeEntry>, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(crate::studio::unreachable_notice(folder));
    }
    let mut entries = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut children: Vec<PathBuf> = match std::fs::read_dir(&dir) {
            Ok(it) => it.flatten().map(|e| e.path()).collect(),
            Err(e) => {
                return Err(refused_code(format!(
                    "the code tree under {} cannot be read: {e}. Nothing changed.",
                    dir.display()
                )));
            }
        };
        children.sort();
        for path in children {
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                // Hidden studio state never lists, at any depth —
                // the tree owns project files only.
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == sdk::HISTORY_DIR_NAME)
                    .unwrap_or(false)
                {
                    continue;
                }
                let name = rel_name(folder, &path);
                entries.push(CodeEntry {
                    path: name,
                    kind: "dir",
                    language: "dir",
                    size: 0,
                });
                stack.push(path);
                continue;
            }
            let name = rel_name(folder, &path);
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let language = language_of(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default(),
            );
            entries.push(CodeEntry {
                path: name,
                kind: "file",
                language,
                size,
            });
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Render the file tree as JSON for the tree API.
pub fn tree_json(entries: &[CodeEntry]) -> String {
    let mut rows = String::new();
    for entry in entries {
        if !rows.is_empty() {
            rows.push(',');
        }
        rows.push_str(&format!(
            "{{\"path\":\"{}\",\"kind\":\"{}\",\"language\":\"{}\",\"size\":{}}}",
            crate::studio::json_escape(&entry.path),
            entry.kind,
            entry.language,
            entry.size
        ));
    }
    format!("{{\"files\":[{rows}]}}")
}

/// One opened file (Sprint 18, T003): text files carry their content
/// with the editor language; binary files open as named read-only
/// placeholders (`placeholder` with no content), so media and fonts
/// show their name and size but never stage an edit.
#[derive(Debug, Clone)]
pub struct CodeFile {
    /// Project-relative path with `/` separators.
    pub path: String,
    /// Editor language hint.
    pub language: String,
    /// Byte length.
    pub size: u64,
    /// True while the file is a named read-only placeholder.
    pub placeholder: bool,
    /// Text content (`None` for placeholders).
    pub content: Option<String>,
}

/// Open one project file for the editor (Sprint 18, T003): text
/// files read fully with their language; binary files answer as
/// named read-only placeholders carrying name and size. Opening a
/// folder, a missing file, or hidden state refuses; opening never
/// writes.
pub fn read_code_file(folder: &Path, rel: &str) -> Result<CodeFile, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(crate::studio::unreachable_notice(folder));
    }
    let path = resolve_code_path(folder, rel)?;
    let name = rel_name(folder, &path);
    let bytes = std::fs::read(&path).map_err(|e| {
        if path.is_dir() {
            refused_code(format!(
                "the code panel path {name:?} is a folder; open a file to edit it. Nothing changed."
            ))
        } else {
            refused_code(format!(
                "the code panel file {name:?} cannot be read: {e}. Nothing changed."
            ))
        }
    })?;
    if path.is_dir() {
        return Err(refused_code(format!(
            "the code panel path {name:?} is a folder; open a file to edit it. Nothing changed."
        )));
    }
    let language = language_of(
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default(),
    )
    .to_string();
    if is_binary_bytes(&bytes) {
        return Ok(CodeFile {
            path: name,
            language,
            size: bytes.len() as u64,
            placeholder: true,
            content: None,
        });
    }
    let content = String::from_utf8(bytes).map_err(|e| {
        refused_code(format!(
            "the code panel file {name:?} is not text ({e}); it opens as a read-only placeholder. Nothing changed."
        ))
    })?;
    Ok(CodeFile {
        path: name.clone(),
        language,
        size: content.len() as u64,
        placeholder: false,
        content: Some(content),
    })
}

/// Revalidate the project exactly like the SDK open (Sprint 18,
/// T003): the same verdict `check` and headless opens see, so a
/// code-panel write validates like its headless counterpart before
/// anything records. Read-only — validation never writes.
fn revalidate_project(folder: &Path) -> validate::validate::Verdict {
    let overrides: validate::validate::VariableOverrides = BTreeMap::new();
    validate::validate::validate_folder_with_variables_strict_themed(
        folder, &overrides, false, None,
    )
}

/// Render one validation verdict's diagnostics as `file:line:col
/// code message` lines, so a refused save names every offender the
/// maker fixes.
fn diagnostic_lines(folder: &Path, verdict: &validate::validate::Verdict) -> String {
    let mut lines = String::new();
    for d in &verdict.diagnostics {
        lines.push_str(&format!(
            "\n{}:{}:{} {} {}",
            d.file,
            d.line,
            d.column,
            d.code.to_string(),
            d.message
        ));
    }
    let _ = folder;
    lines
}

/// Normalize one project-relative reference for comparison: `/`
/// separators, no leading `./`, no trailing `/`.
fn normalize_ref(value: &str) -> String {
    let slash = value.trim().replace('\\', "/");
    let stripped = slash.trim_matches('/');
    stripped.strip_prefix("./").unwrap_or(stripped).to_string()
}

/// One file reference with its referrer label, for blocked deletes.
fn push_clip_path_refs(
    out: &mut Vec<(String, String)>,
    comp: &notation::model::Composition,
    file: &str,
) {
    let _ = file;
    for (index, clip) in comp.clips.iter().enumerate() {
        let path = match clip {
            notation::model::Clip::Video(c) => Some(c.path.as_str()),
            notation::model::Clip::Audio(c) => Some(c.path.as_str()),
            notation::model::Clip::Image(c) => Some(c.path.as_str()),
            notation::model::Clip::Svg(c) => Some(c.path.as_str()),
            _ => None,
        };
        if let Some(path) = path {
            let label = match clip.id() {
                Some(id) => format!(
                    "{} clip {:?} ({}:{}:{})",
                    clip.kind(),
                    id,
                    file,
                    clip.line_col().0,
                    clip.line_col().1
                ),
                None => format!(
                    "{} clip #{} ({}:{}:{})",
                    clip.kind(),
                    index,
                    file,
                    clip.line_col().0,
                    clip.line_col().1
                ),
            };
            out.push((normalize_ref(path), label));
        }
        let grade = clip.grade();
        if let Some(src) = grade.lut.src.as_deref() {
            if !normalize_ref(src).is_empty() {
                let label = match clip.id() {
                    Some(id) => format!(
                        "lut of {} clip {:?} ({}:{}:{})",
                        clip.kind(),
                        id,
                        file,
                        clip.line_col().0,
                        clip.line_col().1
                    ),
                    None => format!(
                        "lut of {} clip #{} ({}:{}:{})",
                        clip.kind(),
                        index,
                        file,
                        clip.line_col().0,
                        clip.line_col().1
                    ),
                };
                out.push((normalize_ref(src), label));
            }
        }
        if let Some(luma) = &clip.vfx().luma_matte {
            if !normalize_ref(&luma.src).is_empty() {
                let label = match clip.id() {
                    Some(id) => format!(
                        "luma-matte of {} clip {:?} ({}:{}:{})",
                        clip.kind(),
                        id,
                        file,
                        clip.line_col().0,
                        clip.line_col().1
                    ),
                    None => format!(
                        "luma-matte of {} clip #{} ({}:{}:{})",
                        clip.kind(),
                        index,
                        file,
                        clip.line_col().0,
                        clip.line_col().1
                    ),
                };
                out.push((normalize_ref(&luma.src), label));
            }
        }
    }
}

/// Collect every composition-level file reference: `font` paths,
/// the composition and lane grade `lut` sources.
fn push_composition_refs(
    out: &mut Vec<(String, String)>,
    comp: &notation::model::Composition,
    file: &str,
) {
    for font in &comp.fonts {
        out.push((
            normalize_ref(&font.path),
            format!("font ({}:{}:{})", file, font.line, font.column),
        ));
    }
    if let Some(src) = comp.grade.lut.src.as_deref() {
        if !normalize_ref(src).is_empty() {
            out.push((
                normalize_ref(src),
                format!(
                    "lut of composition {:?} ({}:{}:{})",
                    comp.name, file, comp.line, comp.column
                ),
            ));
        }
    }
    for lane in &comp.lane_grades {
        if let Some(src) = lane.grade.lut.src.as_deref() {
            if !normalize_ref(src).is_empty() {
                out.push((
                    normalize_ref(src),
                    format!(
                        "lut of {:?} lane grade ({}:{}:{})",
                        lane.track.name(),
                        file,
                        comp.line,
                        comp.column
                    ),
                ));
            }
        }
    }
}

/// Every referrer naming one project file (Sprint 18, T003 / REC-003):
/// footage `path` clips, `font` declarations, grade `lut` sources,
/// and VFX `luma-matte` sources across every composition — the
/// SDK-effective root composition (delta replayed) unioned with the
/// on-disk verdict, so staged SDK edits count too. While the project
/// cannot validate, a textual scan of the root composition names
/// matching lines instead of guessing, so the tree stays usable on
/// broken projects; an unreadable root means no referrers.
pub fn referrers_of(folder: &Path, rel: &str) -> Vec<String> {
    let want = normalize_ref(rel);
    if want.is_empty() {
        return Vec::new();
    }
    let verdict = revalidate_project(folder);
    let valid = matches!(verdict.status, validate::validate::Status::Valid);
    if valid {
        let mut refs: Vec<(String, String)> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut comps: Vec<notation::model::Composition> = Vec::new();
        for comp in &verdict.compositions {
            let name = comp.name.clone();
            match sdk::SdkSession::open(
                folder,
                sdk::OpenOptions {
                    persist: false,
                    history: false,
                    composition: Some(name),
                    ..sdk::OpenOptions::default()
                },
            ) {
                Ok(session) => comps.push(session.effective_composition().clone()),
                Err(_) => comps.push(comp.clone()),
            }
        }
        for comp in &comps {
            if !seen.iter().any(|n| n == &comp.name) {
                seen.push(comp.name.clone());
                push_clip_path_refs(&mut refs, comp, CODE_COMPOSITION_FILE);
                push_composition_refs(&mut refs, comp, CODE_COMPOSITION_FILE);
            }
        }
        let mut out: Vec<String> = refs
            .into_iter()
            .filter(|(path, _)| *path == want)
            .map(|(_, label)| label)
            .collect();
        out.sort();
        out.dedup();
        return out;
    }
    // Unvalidated fallback: name the root-composition lines
    // mentioning the file's base name instead of guessing.
    let base = want.rsplit('/').next().unwrap_or(want.as_str());
    let text = match std::fs::read_to_string(folder.join(validate::validate::COMPOSITION_FILE)) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.contains(base) {
            out.push(format!(
                "{}:{} (unvalidated project: text match)",
                CODE_COMPOSITION_FILE,
                idx + 1
            ));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Refuse deleting, renaming, or moving a referenced file (Sprint 18,
/// T003 / REC-003): the refusal names every referrer, and nothing
/// stages, moves, or records.
fn refuse_referenced(verb: &str, name: &str, referrers: &[String]) -> String {
    refused_code(format!(
        "the code panel {verb} of {name:?} is blocked: the file is still referenced by {}; unreference it first (edit the notation, then retry). Nothing was staged, moved, or recorded.",
        referrers.join(", ")
    ))
}

/// One file-tree mutation from the panel (Sprint 18, T003): `new`
/// (with optional starter `content`), `rename` (with bare `name`),
/// `duplicate`, `delete` (with `confirm`), and `move` (drag into the
/// `target` folder). The actor names the validated turn when the
/// mutation touches tracked `*.kdl` sources.
#[derive(Debug, Clone)]
pub struct TreeAction {
    /// `new`, `rename`, `duplicate`, `delete`, or `move`.
    pub action: String,
    /// Actor the validated turn records under (human or agent).
    pub actor: String,
    /// Project-relative subject path (`new` with a trailing slash
    /// creates a folder, so drags have folders to land in).
    pub path: String,
    /// Bare new file name (`rename` only).
    pub name: Option<String>,
    /// Project-relative destination folder, `.` for root (`move` only).
    pub target: Option<String>,
    /// Starter body (`new` only; per-kind defaults when absent).
    pub content: Option<String>,
    /// Delete confirmation (`delete` only; must be true).
    pub confirm: bool,
}

/// Parse one tree-mutation request body
/// (`{"actor":"maker","action":"delete","path":"old.md","confirm":true}`):
/// the actor must be non-empty, the action names one of the five
/// tree verbs, and each verb carries its own fields. A refused body
/// names its cause and changes nothing.
pub fn parse_tree_action_body(body: &str) -> Result<TreeAction, String> {
    if body.trim().is_empty() {
        return Err(refused_code(
            "the code tree needs a JSON action body (`{\"actor\":\"maker\",\"action\":\"delete\",\"path\":\"old.md\",\"confirm\":true}`); pass the tree change from the panel. Nothing changed.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_code(format!(
            "the code tree body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"action\":\"delete\",\"path\":\"old.md\",\"confirm\":true}}`. Nothing changed."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_code(
            "the code tree names no actor; tree turns are named (human or agent). Pass a non-empty `actor`. Nothing changed.".to_string(),
        ));
    }
    let action = value
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if !["new", "rename", "duplicate", "delete", "move"].contains(&action.as_str()) {
        return Err(refused_code(format!(
            "unknown tree action {action:?}: the code tree applies `new`, `rename`, `duplicate`, `delete`, or `move` (drag into folders). Pass one of those verbs. Nothing changed."
        )));
    }
    let path = value
        .get("path")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if path.is_empty() {
        return Err(refused_code(format!(
            "the code tree `{action}` names no `path`; pass the project-relative file to change. Nothing changed."
        )));
    }
    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.trim().to_string());
    if action == "rename" && name.as_deref().map(|n| n.is_empty()).unwrap_or(true) {
        return Err(refused_code(
            "the code tree `rename` names no `name`; pass the bare new file name (no folders). Nothing changed.".to_string(),
        ));
    }
    let target = value
        .get("target")
        .and_then(|t| t.as_str())
        .map(|t| t.trim().to_string());
    if action == "move" && target.is_none() {
        return Err(refused_code(
            "the code tree `move` names no `target`; pass the destination folder (`.` for the project root). Nothing changed.".to_string(),
        ));
    }
    let content = value
        .get("content")
        .and_then(|c| c.as_str())
        .map(|c| c.to_string());
    let confirm = value
        .get("confirm")
        .and_then(|c| c.as_bool())
        .unwrap_or(false);
    if action == "delete" && !confirm {
        return Err(refused_code(format!(
            "deleting {path:?} asks first: pass `\"confirm\":true` to delete it. Nothing was deleted."
        )));
    }
    Ok(TreeAction {
        action,
        actor,
        path,
        name,
        target,
        content,
        confirm,
    })
}

/// Receipt of one applied tree mutation (Sprint 18, T003): what
/// changed on disk and the validated turn behind it (`None` while
/// the mutation touched no tracked `*.kdl` source).
#[derive(Debug, Clone)]
pub struct TreeReceipt {
    /// Tree verb applied.
    pub action: String,
    /// Project-relative subject path.
    pub path: String,
    /// One-line change description.
    pub detail: String,
    /// Sequence of the recorded history turn, when one recorded.
    pub turn: Option<u64>,
}

/// Render one tree receipt as JSON for the tree API.
pub fn tree_receipt_json(receipt: &TreeReceipt) -> String {
    let turn = match receipt.turn {
        Some(seq) => seq.to_string(),
        None => "null".to_string(),
    };
    format!(
        "{{\"applied\":true,\"action\":\"{}\",\"path\":\"{}\",\"detail\":\"{}\",\"turn\":{}}}",
        crate::studio::json_escape(&receipt.action),
        crate::studio::json_escape(&receipt.path),
        crate::studio::json_escape(&receipt.detail),
        turn
    )
}

/// Render one opened file as JSON for the file API.
pub fn code_file_json(file: &CodeFile) -> String {
    let content = match &file.content {
        Some(text) => format!("\"{}\"", crate::studio::json_escape(text)),
        None => "null".to_string(),
    };
    let note = if file.placeholder {
        format!(
            ",\"note\":\"{} is a binary file; it opens as a named read-only placeholder.\"",
            crate::studio::json_escape(&file.path)
        )
    } else {
        String::new()
    };
    format!(
        "{{\"path\":\"{}\",\"language\":\"{}\",\"size\":{},\"binary\":{},\"read_only\":{},\"content\":{}{}}}",
        crate::studio::json_escape(&file.path),
        crate::studio::json_escape(&file.language),
        file.size,
        file.placeholder,
        file.placeholder,
        content,
        note
    )
}

/// Starter body for a new file by kind: a recreated root
/// composition starts valid, JSON and config start parseable,
/// Markdown starts titled, and anything else starts empty.
fn starter_content(file_name: &str) -> String {
    if is_composition_file(file_name) {
        return CODE_COMPOSITION_STARTER.to_string();
    }
    match language_of(file_name) {
        "json" => CODE_JSON_STARTER.to_string(),
        "markdown" => CODE_MARKDOWN_STARTER.to_string(),
        "toml" => CODE_CONFIG_STARTER.to_string(),
        _ => String::new(),
    }
}

/// Check new-file or new-save bytes before they touch disk: JSON
/// must parse, non-root notation must scan, and binary content
/// (NUL or non-UTF-8) refuses — the editor saves text only. Root
/// composition and config content validate against the whole
/// project after the write instead, where the verdict sees them.
fn check_new_text(file_name: &str, content: &str) -> Result<(), String> {
    if content.contains('\0') {
        return Err(refused_code(format!(
            "the code panel content for {file_name:?} is binary; the editor saves text only. Nothing changed."
        )));
    }
    match language_of(file_name) {
        _ if is_composition_file(file_name) || is_config_file(file_name) => Ok(()),
        "json" => serde_json::from_str::<serde_json::Value>(content).map(|_| ()).map_err(|e| {
            refused_code(format!(
                "the code panel JSON for {file_name:?} does not parse ({e}); fix it and retry. The last good file is untouched."
            ))
        }),
        "notation" => notation::parse::parse_document(content).map(|_| ()).map_err(|e| {
            refused_code(format!(
                "the code panel notation for {file_name:?} does not scan ({}:{}: {}); fix it and retry. The last good file is untouched.",
                e.line, e.column, e.message
            ))
        }),
        _ => Ok(()),
    }
}

/// Next free duplicate name beside the original (`notes-copy.md`,
/// then `notes-copy2.md`, ...), so duplicating never overwrites.
fn duplicate_name(file_name: &str) -> String {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            format!("{stem}-copy.{ext}")
        }
        _ => format!("{file_name}-copy"),
    }
}

/// Apply one file-tree mutation on disk (Sprint 18, T003): new,
/// rename, duplicate, delete (confirmed), and move into folders.
/// Mutations touching tracked `*.kdl` sources record one guarded
/// history turn through the Sprint 15 machinery, so the same undo
/// walks them back; other files apply directly. The root
/// composition never renames, moves, or deletes (the project would
/// stop validating); referenced files never delete, rename, or move
/// (the refusal names every referrer); writes never clobber.
pub fn apply_tree_action(folder: &Path, action: &TreeAction) -> Result<TreeReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(crate::studio::unreachable_notice(folder));
    }
    let src = resolve_code_path(folder, &action.path)?;
    let src_name = rel_name(folder, &src);
    let src_base = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let tracked_src = is_tracked_file(&src_base);
    match action.action.as_str() {
        "new" => {
            if action.path.trim().ends_with('/') {
                if src.exists() {
                    return Err(refused_code(format!(
                        "the code panel cannot create the folder {src_name:?}: something already lives there. Nothing changed."
                    )));
                }
                std::fs::create_dir_all(&src).map_err(|e| {
                    refused_code(format!(
                        "the code panel cannot create the folder {src_name:?}: {e}. Nothing changed."
                    ))
                })?;
                return Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: src_name.clone(),
                    detail: format!("new folder {src_name:?}"),
                    turn: None,
                });
            }
            if src.exists() {
                return Err(refused_code(format!(
                    "the code panel cannot create {src_name:?}: a file already lives there. Open it instead. Nothing changed."
                )));
            }
            let body = match &action.content {
                Some(text) => text.clone(),
                None => starter_content(&src_base),
            };
            check_new_text(&src_base, &body)?;
            let composition_new = is_composition_file(&src_base);
            let validates = needs_project_validation(&src_base);
            let write = |folder: &Path| -> Result<(String, Vec<String>), String> {
                if let Some(parent) = src.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        refused_code(format!(
                            "the code panel cannot prepare the folder for {src_name:?}: {e}. Nothing changed."
                        ))
                    })?;
                }
                std::fs::write(&src, &body).map_err(|e| {
                    refused_code(format!(
                        "the code panel cannot create {src_name:?}: {e}. Nothing changed."
                    ))
                })?;
                if validates {
                    let verdict = revalidate_project(folder);
                    if !matches!(verdict.status, validate::validate::Status::Valid) {
                        let _ = std::fs::remove_file(&src);
                        return Err(refused_code(format!(
                            "the code panel cannot create {src_name:?}: the project no longer validates, so the file was removed again. Offenders:{} The last good tree is intact.",
                            diagnostic_lines(folder, &verdict)
                        )));
                    }
                }
                let detail = if composition_new {
                    format!("new project root {src_name:?}")
                } else {
                    format!("new {src_name:?} ({} byte(s))", body.len())
                };
                Ok((detail.clone(), vec![format!("code new {src_name:?}")]))
            };
            if tracked_src {
                let record = sdk::record_file_turn(folder, &action.actor, || write(folder))
                    .map_err(|h| h.message.clone())?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: src_name,
                    detail: record.summary,
                    turn: Some(record.seq),
                })
            } else {
                let (detail, _) = write(folder)?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: src_name,
                    detail,
                    turn: None,
                })
            }
        }
        "rename" => {
            if is_composition_file(&src_base) {
                return Err(refused_code(format!(
                    "the code panel cannot rename {src_name:?}: the project root composition stays at that name, else the project stops validating. Duplicate it instead. Nothing changed."
                )));
            }
            if !src.is_file() {
                return Err(refused_code(format!(
                    "the code panel cannot rename {src_name:?}: no file lives there. Nothing changed."
                )));
            }
            let new_base = action.name.clone().unwrap_or_default();
            if new_base.contains('/') || new_base.contains('\\') {
                return Err(refused_code(format!(
                    "the code panel cannot rename to {new_base:?}: pass a bare file name (no folders); drag the file to move it between folders. Nothing changed."
                )));
            }
            let dst = resolve_code_path(
                folder,
                &match src.parent().map(|p| rel_name(folder, p)) {
                    Some(dir) if !dir.is_empty() => format!("{dir}/{new_base}"),
                    _ => new_base.clone(),
                },
            )?;
            let dst_name = rel_name(folder, &dst);
            if dst.exists() {
                return Err(refused_code(format!(
                    "the code panel cannot rename {src_name:?} to {dst_name:?}: a file already lives there. Nothing changed."
                )));
            }
            let referrers = referrers_of(folder, &src_name);
            if !referrers.is_empty() {
                return Err(refuse_referenced("rename", &src_name, &referrers));
            }
            let dst_base = dst
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let tracked = tracked_src || is_tracked_file(&dst_base);
            let write = || -> Result<(String, Vec<String>), String> {
                std::fs::rename(&src, &dst).map_err(|e| {
                    refused_code(format!(
                        "the code panel cannot rename {src_name:?} to {dst_name:?}: {e}. Nothing changed."
                    ))
                })?;
                Ok((
                    format!("rename {src_name:?} to {dst_name:?}"),
                    vec![format!("code rename {src_name:?} to {dst_name:?}")],
                ))
            };
            if tracked {
                let record = sdk::record_file_turn(folder, &action.actor, write)
                    .map_err(|h| h.message.clone())?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: dst_name,
                    detail: record.summary,
                    turn: Some(record.seq),
                })
            } else {
                let (detail, _) = write()?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: dst_name,
                    detail,
                    turn: None,
                })
            }
        }
        "duplicate" => {
            if !src.is_file() {
                return Err(refused_code(format!(
                    "the code panel cannot duplicate {src_name:?}: no file lives there. Nothing changed."
                )));
            }
            let mut candidate = duplicate_name(&src_base);
            for _ in 0..100 {
                let probe = match src.parent().map(|p| rel_name(folder, p)) {
                    Some(dir) if !dir.is_empty() => format!("{dir}/{candidate}"),
                    _ => candidate.clone(),
                };
                let probe_path = resolve_code_path(folder, &probe)?;
                if !probe_path.exists() {
                    let dst_name = rel_name(folder, &probe_path);
                    let bytes = std::fs::read(&src).map_err(|e| {
                        refused_code(format!(
                            "the code panel cannot duplicate {src_name:?}: {e}. Nothing changed."
                        ))
                    })?;
                    let tracked = tracked_src;
                    let write = || -> Result<(String, Vec<String>), String> {
                        std::fs::write(&probe_path, &bytes).map_err(|e| {
                            refused_code(format!(
                                "the code panel cannot duplicate {src_name:?} as {dst_name:?}: {e}. Nothing changed."
                            ))
                        })?;
                        Ok((
                            format!("duplicate {src_name:?} as {dst_name:?}"),
                            vec![format!("code duplicate {src_name:?} as {dst_name:?}")],
                        ))
                    };
                    if tracked {
                        let record = sdk::record_file_turn(folder, &action.actor, write)
                            .map_err(|h| h.message.clone())?;
                        return Ok(TreeReceipt {
                            action: action.action.clone(),
                            path: dst_name,
                            detail: record.summary,
                            turn: Some(record.seq),
                        });
                    }
                    let (detail, _) = write()?;
                    return Ok(TreeReceipt {
                        action: action.action.clone(),
                        path: dst_name,
                        detail,
                        turn: None,
                    });
                }
                candidate = duplicate_name(&candidate);
            }
            Err(refused_code(format!(
                "the code panel cannot duplicate {src_name:?}: no free name beside it. Nothing changed."
            )))
        }
        "delete" => {
            if !action.confirm {
                return Err(refused_code(format!(
                    "deleting {:?} asks first: pass `\"confirm\":true` to delete it. Nothing was deleted.",
                    action.path
                )));
            }
            if is_composition_file(&src_base) {
                return Err(refused_code(format!(
                    "the code panel cannot delete {src_name:?}: the project root composition is required; remove its clips instead. Nothing was deleted."
                )));
            }
            if src.is_dir() {
                return Err(refused_code(format!(
                    "the code panel cannot delete {src_name:?}: folders delete through their files — empty the folder first. Nothing was deleted."
                )));
            }
            if !src.is_file() {
                return Err(refused_code(format!(
                    "the code panel cannot delete {src_name:?}: no file lives there. Nothing was deleted."
                )));
            }
            let referrers = referrers_of(folder, &src_name);
            if !referrers.is_empty() {
                return Err(refuse_referenced("delete", &src_name, &referrers));
            }
            let tracked = tracked_src;
            let write = || -> Result<(String, Vec<String>), String> {
                std::fs::remove_file(&src).map_err(|e| {
                    refused_code(format!(
                        "the code panel cannot delete {src_name:?}: {e}. Nothing was deleted."
                    ))
                })?;
                Ok((
                    format!("delete {src_name:?}"),
                    vec![format!("code delete {src_name:?}")],
                ))
            };
            if tracked {
                let record = sdk::record_file_turn(folder, &action.actor, write)
                    .map_err(|h| h.message.clone())?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: src_name,
                    detail: record.summary,
                    turn: Some(record.seq),
                })
            } else {
                let (detail, _) = write()?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: src_name,
                    detail,
                    turn: None,
                })
            }
        }
        "move" => {
            if is_composition_file(&src_base) {
                return Err(refused_code(format!(
                    "the code panel cannot move {src_name:?}: the project root composition stays at the project root, else the project stops validating. Nothing changed."
                )));
            }
            if !src.is_file() {
                return Err(refused_code(format!(
                    "the code panel cannot move {src_name:?}: no file lives there. Nothing changed."
                )));
            }
            let raw_target = action.target.clone().unwrap_or_default();
            let target_rel = raw_target.trim();
            let dst = if target_rel.is_empty() || target_rel == "." {
                match src.parent() {
                    Some(parent) if rel_name(folder, parent).is_empty() => {
                        return Err(refused_code(format!(
                            "the code panel cannot move {src_name:?} to the project root: it already lives there. Nothing changed."
                        )));
                    }
                    _ => folder.join(&src_base),
                }
            } else {
                let dir = resolve_code_path(folder, target_rel)?;
                if !dir.is_dir() {
                    return Err(refused_code(format!(
                        "the code panel cannot move {src_name:?} into {target_rel:?}: no folder lives there. Create it with tree `new` first. Nothing changed."
                    )));
                }
                dir.join(&src_base)
            };
            let dst_name = rel_name(folder, &dst);
            if dst == src {
                return Err(refused_code(format!(
                    "the code panel cannot move {src_name:?} onto itself. Nothing changed."
                )));
            }
            if dst.exists() {
                return Err(refused_code(format!(
                    "the code panel cannot move {src_name:?} to {dst_name:?}: a file already lives there. Nothing changed."
                )));
            }
            let referrers = referrers_of(folder, &src_name);
            if !referrers.is_empty() {
                return Err(refuse_referenced("move", &src_name, &referrers));
            }
            let tracked = tracked_src;
            let write = || -> Result<(String, Vec<String>), String> {
                std::fs::rename(&src, &dst).map_err(|e| {
                    refused_code(format!(
                        "the code panel cannot move {src_name:?} to {dst_name:?}: {e}. Nothing changed."
                    ))
                })?;
                Ok((
                    format!("move {src_name:?} to {dst_name:?}"),
                    vec![format!("code move {src_name:?} to {dst_name:?}")],
                ))
            };
            if tracked {
                let record = sdk::record_file_turn(folder, &action.actor, write)
                    .map_err(|h| h.message.clone())?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: dst_name,
                    detail: record.summary,
                    turn: Some(record.seq),
                })
            } else {
                let (detail, _) = write()?;
                Ok(TreeReceipt {
                    action: action.action.clone(),
                    path: dst_name,
                    detail,
                    turn: None,
                })
            }
        }
        _ => Err(refused_code(format!(
            "unknown tree action {:?}. Nothing changed.",
            action.action
        ))),
    }
}

/// One notation-editor save from the panel (Sprint 18, T003): the
/// actor, the project-relative file, and the full new text. Saves
/// never create files — the tree `new` verb does that — and never
/// touch binary files, which stay named read-only placeholders.
#[derive(Debug, Clone)]
pub struct CodeSave {
    /// Actor the validated turn records under (human or agent).
    pub actor: String,
    /// Project-relative file to overwrite.
    pub path: String,
    /// Full new text of the file.
    pub content: String,
}

/// Parse one editor-save request body
/// (`{"actor":"maker","path":"composition.kdl","content":"..."}`):
/// the actor must be non-empty, the path names the file, and the
/// content rides along (empty text saves empty). A refused body
/// names its cause and writes nothing.
pub fn parse_save_body(body: &str) -> Result<CodeSave, String> {
    if body.trim().is_empty() {
        return Err(refused_code(
            "the code editor needs a JSON save body (`{\"actor\":\"maker\",\"path\":\"composition.kdl\",\"content\":\"...\"}`); pass the edited text from the panel. Nothing was saved.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_code(format!(
            "the code editor save body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"path\":\"composition.kdl\",\"content\":\"...\"}}`. Nothing was saved."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_code(
            "the code editor save names no actor; saved turns are named (human or agent). Pass a non-empty `actor`. Nothing was saved.".to_string(),
        ));
    }
    let path = value
        .get("path")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if path.is_empty() {
        return Err(refused_code(
            "the code editor save names no `path`; pass the project-relative file to overwrite. Nothing was saved.".to_string(),
        ));
    }
    let content = match value.get("content").and_then(|c| c.as_str()) {
        Some(text) => text.to_string(),
        None => {
            return Err(refused_code(
                "the code editor save carries no `content`; pass the full new file text (empty text saves empty). Nothing was saved.".to_string(),
            ));
        }
    };
    Ok(CodeSave {
        actor,
        path,
        content,
    })
}

/// Receipt of one applied editor save (Sprint 18, T003): the saved
/// path with its byte length and the validated turn behind it
/// (`None` while the file is untracked — JSON, Markdown, config,
/// and other text save directly, since history backs `*.kdl`
/// sources plus SDK deltas only).
#[derive(Debug, Clone)]
pub struct SaveReceipt {
    /// Project-relative file saved.
    pub path: String,
    /// Byte length of the saved text.
    pub bytes: usize,
    /// Sequence of the recorded history turn, when one recorded.
    pub turn: Option<u64>,
    /// One-line save description.
    pub detail: String,
}

/// Render one save receipt as JSON for the save API.
pub fn save_receipt_json(receipt: &SaveReceipt) -> String {
    let turn = match receipt.turn {
        Some(seq) => seq.to_string(),
        None => "null".to_string(),
    };
    format!(
        "{{\"saved\":true,\"path\":\"{}\",\"bytes\":{},\"detail\":\"{}\",\"turn\":{}}}",
        crate::studio::json_escape(&receipt.path),
        receipt.bytes,
        crate::studio::json_escape(&receipt.detail),
        turn
    )
}

/// Save one editor file through validated turns (Sprint 18, T003 /
/// REC-003): the new text checks per kind before anything stages —
/// notation scans, JSON parses, and the root composition plus the
/// project config revalidate the whole project after the write — so
/// a save that breaks validation refuses with diagnostics keeping
/// the last good file. Saves to tracked `*.kdl` sources record one
/// guarded history turn through the Sprint 15 machinery (the write
/// runs inside the turn, so the `before` backup holds the last good
/// bytes and the same undo walks the save back); untracked files
/// write directly with restore-on-failure. Binary files refuse as
/// read-only placeholders, and saving never creates files.
pub fn save_code_file(folder: &Path, save: &CodeSave) -> Result<SaveReceipt, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(crate::studio::unreachable_notice(folder));
    }
    let path = resolve_code_path(folder, &save.path)?;
    let name = rel_name(folder, &path);
    let base = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    if path.is_dir() {
        return Err(refused_code(format!(
            "the code editor cannot save {name:?}: it is a folder. Nothing was saved."
        )));
    }
    let old = std::fs::read(&path).map_err(|e| {
        refused_code(format!(
            "the code editor cannot save {name:?}: {e}. Create it with tree `new` first. Nothing was saved."
        ))
    })?;
    if is_binary_bytes(&old) {
        return Err(refused_code(format!(
            "the code editor cannot save {name:?}: it is a binary file and opens as a named read-only placeholder. Nothing was saved."
        )));
    }
    check_new_text(&base, &save.content)?;
    let validates = needs_project_validation(&base);
    let bytes = save.content.len();
    if is_tracked_file(&base) {
        let content = save.content.clone();
        let record = sdk::record_file_turn(folder, &save.actor, || {
            std::fs::write(&path, &content).map_err(|e| {
                refused_code(format!(
                    "the code editor cannot save {name:?}: {e}. The last good file is intact. Nothing was recorded."
                ))
            })?;
            if validates {
                let verdict = revalidate_project(folder);
                if !matches!(verdict.status, validate::validate::Status::Valid) {
                    let _ = std::fs::write(&path, &old);
                    return Err(refused_code(format!(
                        "the code editor cannot save {name:?}: the project no longer validates, so the last good file stays. Offenders:{} Fix the problems and retry. Nothing was recorded.",
                        diagnostic_lines(folder, &verdict)
                    )));
                }
            }
            Ok((
                format!("save {name:?} ({bytes} byte(s))"),
                vec![format!("code save {name:?}")],
            ))
        })
        .map_err(|h| h.message.clone())?;
        Ok(SaveReceipt {
            path: name,
            bytes,
            turn: Some(record.seq),
            detail: record.summary,
        })
    } else {
        std::fs::write(&path, &save.content).map_err(|e| {
            refused_code(format!(
                "the code editor cannot save {name:?}: {e}. The last good file is intact. Nothing was saved."
            ))
        })?;
        if validates {
            let verdict = revalidate_project(folder);
            if !matches!(verdict.status, validate::validate::Status::Valid) {
                let _ = std::fs::write(&path, &old);
                return Err(refused_code(format!(
                    "the code editor cannot save {name:?}: the project no longer validates, so the last good file stays. Offenders:{} Fix the problems and retry. Nothing was saved.",
                    diagnostic_lines(folder, &verdict)
                )));
            }
        }
        Ok(SaveReceipt {
            path: name.clone(),
            bytes,
            turn: None,
            detail: format!("save {name:?} ({bytes} byte(s))"),
        })
    }
}

/// Where jump-to-source lands (Sprint 18, T003 / J-03): the source
/// file with the 1-based line and column of the selected element,
/// so the editor opens exactly on it.
#[derive(Debug, Clone)]
pub struct CodeJump {
    /// Source file (`composition.kdl` — the single parsed root).
    pub file: String,
    /// 1-based source line of the element node.
    pub line: usize,
    /// 1-based source column of the element node.
    pub column: usize,
    /// Element kind (`text`, `image`, ...).
    pub kind: String,
    /// Stable `id`, when declared.
    pub id: Option<String>,
}

/// Render one jump as JSON for the jump API.
pub fn jump_json(jump: &CodeJump) -> String {
    let id = match &jump.id {
        Some(id) => format!("\"{}\"", crate::studio::json_escape(id)),
        None => "null".to_string(),
    };
    format!(
        "{{\"file\":\"{}\",\"line\":{},\"column\":{},\"kind\":\"{}\",\"id\":{}}}",
        crate::studio::json_escape(&jump.file),
        jump.line,
        jump.column,
        crate::studio::json_escape(&jump.kind),
        id
    )
}

/// Jump from a canvas or timeline selection to its source line
/// (Sprint 18, T003 / J-03): the selection key names a clip by
/// declared `id` (`title`), by `#index` (`#0`), or by bare index
/// (`0`, like the inspector) in the named composition (the project
/// root when unnamed). Reads the SDK-effective composition with
/// the persisted delta replayed through a read-only probe, so
/// saved turns show at once; the probe never writes. An unknown
/// key or composition refuses naming what the project holds.
pub fn jump_to_selection(
    folder: &Path,
    composition: Option<&str>,
    key: &str,
) -> Result<CodeJump, String> {
    if !crate::studio::project_reachable(folder) {
        return Err(crate::studio::unreachable_notice(folder));
    }
    let trimmed = key.trim().trim_start_matches('#').trim().to_string();
    if trimmed.is_empty() {
        return Err(refused_code(
            "the code panel jump names no selection; pass a clip `id` or `#index` from the canvas or timeline. Nothing changed.".to_string(),
        ));
    }
    let session = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| refused_code(format!("{e} Nothing changed.")))?;
    let comp = session.effective_composition().clone();
    let found = match trimmed.parse::<usize>() {
        Ok(index) => comp.clips.get(index).map(|clip| (index, clip)),
        Err(_) => comp
            .clips
            .iter()
            .enumerate()
            .find(|(_, clip)| clip.id() == Some(trimmed.as_str()))
            .map(|(index, clip)| (index, clip)),
    };
    match found {
        Some((_, clip)) => {
            let (line, column) = clip.line_col();
            Ok(CodeJump {
                file: CODE_COMPOSITION_FILE.to_string(),
                line,
                column,
                kind: clip.kind().to_string(),
                id: clip.id().map(|s| s.to_string()),
            })
        }
        None => Err(refused_code(format!(
            "the code panel jump names {trimmed:?}, which selects no clip in {:?}; the composition holds {}. Nothing changed.",
            comp.name,
            crate::studio::selection_keys_of(&comp)
                .iter()
                .map(|k| format!("{k:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Extra CSS the code panel needs (Sprint 18, T003): the file-tree
/// list, the editor textarea with its language badge, and the jump
/// and status lines. Style attributes stay CSS-like hyphenated end
/// to end (`background-color`, `border-radius`, `font-family`,
/// `tabular-nums` figures).
pub fn code_css() -> String {
    r#"        #code-panel{margin-top:12px}
        #code-panel h3{margin:6px 0;font-size:13px}
        #code-tree{list-style:none;margin:4px 0;padding:0;max-height:160px;overflow-y:auto;border:1px solid var(--line-color);border-radius:4px;padding:4px 8px}
        #code-tree li{display:flex;gap:8px;align-items:baseline;font-variant-numeric:tabular-nums}
        #code-tree button{margin:1px 4px 1px 0}
        #code-editor{width:100%;min-height:160px;font-family:monospace;background-color:var(--panel-color);color:var(--ink-color);border:1px solid var(--line-color);border-radius:4px;padding:6px 8px;white-space:pre}
        #code-status{font-variant-numeric:tabular-nums}
        #code-lang{font-variant-numeric:tabular-nums}"#.to_string()
}

/// The code panel block (Sprint 18, T003): the file tree (new,
/// rename, duplicate, delete with confirmation, drag into folders),
/// the notation editor with its language badge and save, and the
/// jump-to-selected-element button landing on the source line.
pub fn code_html() -> String {
    r#"<div id="code-panel">
        <h3>code</h3>
        <ul id="code-tree" aria-label="project files"></ul>
        <div>
        <label>new file <input id="code-new-name" type="text" placeholder="notes.md"></label>
        <button id="code-new" type="button">new</button>
        </div>
        <p>editing <strong id="code-open-name">composition.kdl</strong> <span id="code-lang">notation</span></p>
        <textarea id="code-editor" spellcheck="false" aria-label="notation editor"></textarea>
        <div>
        <button id="code-save" type="button">save</button>
        <button id="code-jump" type="button">jump to selection</button>
        <button id="code-refresh" type="button">refresh tree</button>
        </div>
        <p id="code-status" role="status">code ready</p>
        </div>"#.to_string()
}

/// The code panel script (Sprint 18, T003): paints the file tree,
/// opens text files into the editor (binaries report their
/// placeholder), saves through validated turns, and jumps from the
/// shared canvas/timeline selection to the source line.
pub fn code_js() -> String {
    r#"        let codeOpen = 'composition.kdl';
        function codeStatusText(t) { const s = document.getElementById('code-status'); if (s) s.textContent = t; }
        async function paintTree() {
          const box = document.getElementById('code-tree'); if (!box) return;
          try {
            const r = await fetch('/api/studio/code/tree');
            const t = await r.text();
            if (!r.ok) { codeStatusText(t); banner(t); return; }
            const j = JSON.parse(t);
            box.innerHTML = '';
            for (const f of (j.files || [])) {
              const li = document.createElement('li');
              const open = document.createElement('button'); open.type = 'button'; open.textContent = 'open';
              open.addEventListener('click', () => openCodeFile(f.path));
              const name = document.createElement('span'); name.textContent = f.path + ' (' + f.language + ')';
              const del = document.createElement('button'); del.type = 'button'; del.textContent = 'delete';
              del.addEventListener('click', async () => {
                if (!confirm('delete ' + f.path + '?')) return;
                await postTree({actor: 'maker', action: 'delete', path: f.path, confirm: true});
              });
              li.append(open, name, del);
              box.appendChild(li);
            }
          } catch (e) { codeStatusText('code tree unavailable: ' + e); }
        }
        async function openCodeFile(path) {
          try {
            const r = await fetch('/api/studio/code/file?path=' + encodeURIComponent(path));
            const t = await r.text();
            if (!r.ok) { codeStatusText(t); banner(t); return; }
            const j = JSON.parse(t);
            codeOpen = j.path;
            const label = document.getElementById('code-open-name'); if (label) label.textContent = j.path;
            const lang = document.getElementById('code-lang'); if (lang) lang.textContent = j.language + (j.binary ? ' (read-only placeholder)' : '');
            const ed = document.getElementById('code-editor');
            if (ed) { ed.value = j.binary ? ('binary file ' + j.path + ' (' + j.size + ' byte(s)): read-only placeholder.') : (j.content || ''); ed.readOnly = !!j.binary; }
            codeStatusText(j.binary ? ('placeholder: ' + j.path) : ('open: ' + j.path));
          } catch (e) { codeStatusText('open failed: ' + e); }
        }
        async function postTree(payload) {
          try {
            const r = await fetch('/api/studio/code/tree', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(payload)});
            const t = await r.text();
            if (!r.ok) { codeStatusText(t); banner(t); return; }
            const j = JSON.parse(t);
            codeStatusText(j.detail + (j.turn === null || j.turn === undefined ? '' : ' (turn ' + j.turn + ')'));
            toast(j.detail);
            paintTree();
          } catch (e) { codeStatusText('tree change failed: ' + e); }
        }
        async function saveCodeFile() {
          const ed = document.getElementById('code-editor');
          const text = ed ? ed.value : '';
          try {
            const r = await fetch('/api/studio/code/save', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({actor: 'maker', path: codeOpen, content: text})});
            const t = await r.text();
            if (!r.ok) { codeStatusText(t); banner(t); return; }
            const j = JSON.parse(t);
            codeStatusText(j.detail + (j.turn === null || j.turn === undefined ? '' : ' (turn ' + j.turn + ')'));
            toast(j.detail);
          } catch (e) { codeStatusText('save failed: ' + e); }
        }
        async function jumpToSelection() {
          const key = (selection && selection.length) ? selection[0] : '';
          if (!key) { codeStatusText('jump needs a canvas or timeline selection first'); return; }
          try {
            const r = await fetch('/api/studio/code/jump?selection=' + encodeURIComponent(key) + '&composition=' + encodeURIComponent((rows && rows.composition) || boot.composition));
            const t = await r.text();
            if (!r.ok) { codeStatusText(t); banner(t); return; }
            const j = JSON.parse(t);
            codeStatusText('selection ' + key + ' lives at ' + j.file + ':' + j.line);
            openCodeFile(j.file);
          } catch (e) { codeStatusText('jump failed: ' + e); }
        }
        document.getElementById('code-new').addEventListener('click', () => {
          const el = document.getElementById('code-new-name');
          const name = el ? el.value.trim() : '';
          if (!name) { codeStatusText('new needs a file name'); return; }
          postTree({actor: 'maker', action: 'new', path: name});
        });
        document.getElementById('code-save').addEventListener('click', saveCodeFile);
        document.getElementById('code-jump').addEventListener('click', jumpToSelection);
        document.getElementById('code-refresh').addEventListener('click', paintTree);
        paintTree(); openCodeFile(codeOpen);"#.to_string()
}
