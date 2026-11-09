//! Variables panel for the Studio shell (Sprint 18, T005 / CAP-005,
//! J-02, STATE-02, INT-001, REC-001).
//!
//! The panel is a view over Sprint 9 variables and Sprint 15 SDK
//! semantics, never a second store: overrides preview in memory
//! without touching declared defaults; declare, edit, and remove
//! validate exactly like the SDK and persist through the SDK delta
//! files; binding writes a `var(--id)` reference onto the selected
//! element through the patch and history machinery, so one undo
//! system serves panel and headless edits alike; copies place exact
//! values text and a runnable render command; unused and undeclared
//! warnings list on the panel. Removing a variable still in use
//! refuses listing its references; an undeclared bind target is
//! reported, never created silently.
//!
//! Write boundary: panel reads never write. Declaration changes
//! persist through the SDK delta files plus one guarded history
//! turn each ([`sdk::record_file_turn`]); binds persist through
//! [`crate::studio::apply_studio_edit`]. Composition sources stay
//! byte-identical and no render output is produced anywhere.
//!
//! Validation boundary: declare, edit, and remove run the exact
//! SDK rules, and staged problems (bad values, duplicates,
//! unknown or mistyped staged overrides) block writing nothing.
//! Pre-existing disk variable health (strict unused and
//! default-less diagnostics, whose declarations live in the KDL
//! sources no panel edit may rewrite) persists as listed
//! warnings instead of deadlocking the panel.
//!
//! Style boundary: row keys stay CSS-like hyphenated end to end
//! (`value`, `font-family`, `source`), and `font-family` try-order
//! lists pass through untouched — unknown families stay fail-closed
//! at render time, never resolved or substituted here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Rows that carry authored text, so a `var(--id)` reference rides
/// through [`sdk::EditOp::SetRow`] verbatim: `value` (text clips),
/// `font-family` (text clips, comma try-order), and `source` (media
/// clips). Typed rows (numbers, colors, selects, booleans) hold
/// resolved values in the model and refuse reference text naming
/// their rule — binding onto one refuses here instead, before
/// anything stages.
pub const BINDABLE_ROWS: [&str; 3] = ["value", "font-family", "source"];

/// Why a variables-panel request refused.
fn refused_variables(message: String) -> String {
    format!("studio refused: {message}")
}

/// Open a writable SDK session over the project for one panel
/// mutation: the open replays the persisted edit and variable
/// deltas, so the panel reads the same truth the SDK and CLI agree
/// on. A refused open names its cause and stages nothing.
fn open_panel_session(folder: &Path, composition: Option<&str>) -> Result<sdk::SdkSession, String> {
    sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused_variables(format!(
            "the project cannot open for variables: {e} Nothing was staged, applied, or recorded."
        ))
    })
}

/// Open a read-only SDK session: it validates and resolves but
/// creates nothing — not the history directory, not a delta — so
/// previews, copies, and warnings never write.
fn open_read_session(
    folder: &Path,
    composition: Option<&str>,
    overrides: validate::validate::VariableOverrides,
    coalesce: bool,
) -> Result<sdk::SdkSession, String> {
    sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            preview: false,
            history: false,
            overrides,
            coalesce,
            composition: composition.map(str::to_string),
        },
    )
    .map_err(|e| {
        refused_variables(format!(
            "the project cannot open for variables: {e} Nothing was staged, applied, or recorded."
        ))
    })
}

/// Actor check shared by every mutating panel call: persisted turns
/// are named (human or agent).
fn check_actor(actor: &str) -> Result<String, String> {
    let name = actor.trim().to_string();
    if name.is_empty() {
        return Err(refused_variables(
            "the variables edit names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    Ok(name)
}

/// Reachability check shared by every panel call: a moved or
/// deleted project stays open read-only with staged work kept.
fn check_reachable(folder: &Path) -> Result<(), String> {
    if !crate::studio::project_reachable(folder) {
        return Err(refused_variables(format!(
            "the project {} is unreachable (moved or deleted); the shell stays open read-only with every staged edit kept. Restore the folder and retry. Nothing was applied or recorded.",
            folder.display()
        )));
    }
    Ok(())
}

/// Every variable id the session declares: on-disk declarations
/// across every composition plus staged records on the open
/// composition. Override deltas never add ids — undeclared keys
/// are reported, never silently created.
fn declared_ids(session: &sdk::SdkSession) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for comp in session.all_compositions() {
        for decl in &comp.variables {
            ids.insert(decl.id.clone());
        }
    }
    for decl in session.variable_declarations() {
        ids.insert(decl.id.clone());
    }
    ids
}

/// Ids declared on disk (every composition), without staged
/// records: the source baseline the panel listing marks `disk`.
fn disk_declared_ids(session: &sdk::SdkSession) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for comp in session.all_compositions() {
        for decl in &comp.variables {
            ids.insert(decl.id.clone());
        }
    }
    ids
}

/// Canonical text of one typed default, exactly like the SDK
/// read-back: strings verbatim, numbers plain, colors lowercase
/// `#rrggbb`, booleans bare, enums verbatim, fonts joined in
/// comma try-order, images verbatim.
fn default_text(decl: &notation::model::VariableDecl) -> Option<String> {
    match &decl.default {
        None => None,
        Some(notation::model::VariableDefault::Str(s)) => Some(s.clone()),
        Some(notation::model::VariableDefault::Num(x)) => Some(x.to_string()),
        Some(notation::model::VariableDefault::Color(c)) => Some(format!("#{}", c.to_hex())),
        Some(notation::model::VariableDefault::Bool(b)) => Some(b.to_string()),
        Some(notation::model::VariableDefault::Enum(s)) => Some(s.clone()),
        Some(notation::model::VariableDefault::Font(names)) => Some(names.join(", ")),
        Some(notation::model::VariableDefault::Image(path)) => Some(path.clone()),
    }
}

/// One caller override value as exact JSON: the same scalar shape
/// the CLI `--variables` payload parses, so copied values apply
/// back byte-neutral.
fn override_json_value(value: &validate::validate::OverrideValue) -> serde_json::Value {
    match value {
        validate::validate::OverrideValue::Str(s) => serde_json::Value::String(s.clone()),
        validate::validate::OverrideValue::Num(x) => serde_json::Number::from_f64(*x)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        validate::validate::OverrideValue::Bool(b) => serde_json::Value::Bool(*b),
    }
}

/// Exact values text of one override map: compact JSON with sorted
/// keys, so repeated copies agree byte for byte.
fn overrides_json(map: &validate::validate::VariableOverrides) -> String {
    let mut out = BTreeMap::new();
    for (key, value) in map {
        out.insert(key.clone(), override_json_value(value));
    }
    serde_json::to_string(&out).unwrap_or_else(|_| "{}".to_string())
}

/// Whether `text` is a plausible variable id, exactly like the SDK
/// and `check`: starts with an ASCII letter, then ASCII letters,
/// digits, dashes, or underscores.
fn valid_variable_id(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Whether `var(` at byte offset `abs` really opens a call: the
/// preceding byte is no name character, so `myvar(` never counts.
/// Mirrors the SDK usage scan.
fn is_call_start(bytes: &[u8], abs: usize) -> bool {
    if abs == 0 {
        return true;
    }
    !matches!(
        bytes[abs - 1],
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'
    )
}

/// Skip ASCII whitespace from byte offset `j`.
fn skip_ws(text: &str, mut j: usize) -> usize {
    while j < text.len() {
        match text.as_bytes()[j] {
            b' ' | b'\t' | b'\n' | b'\r' => j += 1,
            _ => break,
        }
    }
    j
}

/// Parse the single `var()` call opening at `abs` (the `v` of
/// `var(`): the referenced id, or `None` when the text is no
/// well-formed call. Malformed calls belong to validation, never
/// to the usage listing — mirrors the SDK scan.
fn parse_var_id(text: &str, abs: usize) -> Option<String> {
    let mut j = skip_ws(text, abs + 4);
    if !text[j..].starts_with("--") {
        return None;
    }
    j += 2;
    let start = j;
    while j < text.len() {
        match text.as_bytes()[j] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => j += 1,
            _ => break,
        }
    }
    let name = text.get(start..j)?.to_string();
    if !valid_variable_id(&name) {
        return None;
    }
    j = skip_ws(text, j);
    match text.as_bytes().get(j) {
        Some(b')') | Some(b',') => Some(name),
        _ => None,
    }
}

/// Every variable id referenced by `var(--id)` calls in `text`,
/// deduplicated: nested fallbacks surface each inner call on its
/// own, malformed calls are skipped.
fn extract_var_ids(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut i = 0usize;
    while i + 4 <= text.len() {
        match text[i..].find("var(") {
            None => break,
            Some(rel) => {
                let abs = i + rel;
                if !is_call_start(text.as_bytes(), abs) {
                    i = abs + 4;
                    continue;
                }
                if let Some(id) = parse_var_id(text, abs) {
                    seen.insert(id);
                }
                i = abs + 1;
            }
        }
    }
    seen.into_iter().collect()
}

/// One `var(--id)` reference the panel knows: the variable id, a
/// human location, and the hyphenated attribute holding the call,
/// as authored. Source-layer references locate by file, line, and
/// column; references the panel bound through the SDK delta locate
/// by composition, clip, and row — the delta overlays the sources,
/// so both layers list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableReference {
    /// Referenced variable id (without the `--` prefix).
    pub variable: String,
    /// Human location: `path:line:column` on the source layer, or
    /// `composition "<name>" clip "<id>" row `<attribute>`` on the
    /// staged layer.
    pub location: String,
    /// Hyphenated attribute holding the call (`value`,
    /// `font-family`, `source`, ...), as authored.
    pub attribute: String,
}

/// Clip display name for staged-layer locations: the stable `id`
/// when written, else the document index.
fn clip_name(clip: &notation::model::Clip, index: usize) -> String {
    clip.id()
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{index}"))
}

/// Clip identity owning one source line: the `id="..."` written
/// inline on that line, else none. A `grid="..."` or `mid="..."`
/// never counts: the `id` must open the name.
fn clip_id_on_line(folder: &Path, file: &str, line: usize) -> Option<String> {
    let text = std::fs::read_to_string(folder.join(file)).ok()?;
    let raw = text.lines().nth(line.saturating_sub(1))?;
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i + 4 <= raw.len() {
        match raw[i..].find("id=\"") {
            None => break,
            Some(rel) => {
                let abs = i + rel;
                let opens = abs == 0
                    || !matches!(
                        bytes[abs - 1],
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'
                    );
                if !opens {
                    i = abs + 4;
                    continue;
                }
                let start = abs + 4;
                let mut j = start;
                while j < raw.len() && bytes[j] != b'"' {
                    j += 1;
                }
                if j > start {
                    if let Some(id) = raw.get(start..j) {
                        if !id.is_empty() {
                            return Some(id.to_string());
                        }
                    }
                }
                i = abs + 4;
            }
        }
    }
    None
}

/// References bound through the SDK delta: every `var(--id)` call
/// in the effective composition's text-carrying rows (`value`,
/// `font-family`, `source`) — the only rows binding writes — each
/// with the clip identity for source-layer dedupe. Hand-written
/// source references replay here too; the caller dedupes those
/// against the source scan, keeping the file location.
fn effective_references(
    comp: &notation::model::Composition,
) -> Vec<(VariableReference, Option<String>)> {
    let mut out = Vec::new();
    for (index, clip) in comp.clips.iter().enumerate() {
        let name = clip_name(clip, index);
        let identity = clip.id().map(str::to_string);
        let mut rows: Vec<(&str, String)> = Vec::new();
        match clip {
            notation::model::Clip::Text(c) => {
                rows.push(("value", c.value.clone()));
                for entry in c.font_family.clone().unwrap_or_default() {
                    rows.push(("font-family", entry));
                }
            }
            notation::model::Clip::Video(c) => rows.push(("source", c.path.clone())),
            notation::model::Clip::Image(c) => rows.push(("source", c.path.clone())),
            notation::model::Clip::Audio(c) => rows.push(("source", c.path.clone())),
            notation::model::Clip::Svg(c) => rows.push(("source", c.path.clone())),
            _ => {}
        }
        for (attribute, text) in rows {
            for id in extract_var_ids(&text) {
                out.push((
                    VariableReference {
                        variable: id,
                        location: format!(
                            "composition \"{}\" clip \"{}\" row `{}`",
                            comp.name, name, attribute
                        ),
                        attribute: attribute.to_string(),
                    },
                    identity.clone(),
                ));
            }
        }
    }
    out.sort_by(|a, b| {
        (&a.0.variable, &a.0.location, &a.0.attribute).cmp(&(
            &b.0.variable,
            &b.0.location,
            &b.0.attribute,
        ))
    });
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Every `var(--id)` reference the panel knows, optionally
/// filtered to one variable: the source-layer scan (file, line,
/// column) plus staged-layer references the panel bound through
/// the SDK delta. A staged reference on the same clip, attribute,
/// and variable as a source-layer hit is the replayed source
/// text, so the source location wins and the duplicate drops; a
/// staged reference on another clip lists on its own. Sorted for
/// display; read-only, never writes.
pub fn panel_references(
    folder: &Path,
    composition: Option<&str>,
    filter: Option<&str>,
) -> Result<Vec<VariableReference>, String> {
    check_reachable(folder)?;
    let session = open_read_session(folder, composition, BTreeMap::new(), true)?;
    Ok(all_references(&session, filter))
}

/// Reference listing over one open session (see
/// [`panel_references`]).
fn all_references(session: &sdk::SdkSession, filter: Option<&str>) -> Vec<VariableReference> {
    let folder = session.project_folder().to_path_buf();
    let mut out: Vec<VariableReference> = Vec::new();
    let mut disk_clips = BTreeSet::new();
    for found in session.scan_variable_usage(filter) {
        let clip = clip_id_on_line(&folder, &found.file, found.line);
        disk_clips.insert((found.variable.clone(), found.attribute.clone(), clip));
        out.push(VariableReference {
            variable: found.variable,
            location: format!("{}:{}:{}", found.file, found.line, found.column),
            attribute: found.attribute,
        });
    }
    for (staged, identity) in effective_references(session.effective_composition()) {
        if !filter.map(|id| staged.variable == id).unwrap_or(true) {
            continue;
        }
        // Replayed source text drops only on an exact
        // (variable, attribute, clip) match; id-less clips never
        // suppress, so a bound reference always lists somewhere.
        let replayed = match &identity {
            Some(clip) => disk_clips.contains(&(
                staged.variable.clone(),
                staged.attribute.clone(),
                Some(clip.clone()),
            )),
            None => false,
        };
        if !replayed {
            out.push(staged);
        }
    }
    out.sort_by(|a, b| {
        (&a.variable, &a.location, &a.attribute).cmp(&(&b.variable, &b.location, &b.attribute))
    });
    out
}

/// Kind of one panel warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelWarningKind {
    /// Declared but never bound by `var(--id)`.
    Unused,
    /// Referenced or keyed but never declared.
    Undeclared,
    /// A failed SDK rule blocking persistence.
    Invalid,
}

impl PanelWarningKind {
    /// Served spelling of the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            PanelWarningKind::Unused => "unused",
            PanelWarningKind::Undeclared => "undeclared",
            PanelWarningKind::Invalid => "invalid",
        }
    }

    fn order(self) -> u8 {
        match self {
            PanelWarningKind::Unused => 0,
            PanelWarningKind::Undeclared => 1,
            PanelWarningKind::Invalid => 2,
        }
    }
}

/// One panel warning: its kind, the variable it concerns when the
/// warning names one, and the message with the fix — the same
/// shape `check` prints, so the maker fixes the same problem the
/// same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelWarning {
    /// Warning kind.
    pub kind: PanelWarningKind,
    /// Variable the warning concerns, without the `--` prefix,
    /// when the warning names one.
    pub variable: Option<String>,
    /// Human message naming the cause.
    pub message: String,
    /// How to fix it.
    pub fix: String,
}

/// Unused and undeclared warnings over one open session (see
/// [`panel_warnings`]): declared-but-never-bound ids warn as
/// unused; references and override keys naming no declared id
/// warn as undeclared; failed SDK rules (disk diagnostics only —
/// staged advisories already list as unused) warn as invalid.
fn warnings_for(session: &sdk::SdkSession) -> Vec<PanelWarning> {
    let mut out = Vec::new();
    let declared = {
        let mut ids = BTreeSet::new();
        for comp in session.all_compositions() {
            for decl in &comp.variables {
                ids.insert(decl.id.clone());
            }
        }
        for decl in session.variable_declarations() {
            ids.insert(decl.id.clone());
        }
        ids
    };
    let refs = all_references(session, None);
    let mut bound = BTreeSet::new();
    let mut by_variable: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for reference in &refs {
        bound.insert(reference.variable.clone());
        by_variable
            .entry(reference.variable.clone())
            .or_default()
            .push(reference.location.clone());
    }
    for id in &declared {
        if !bound.contains(id) {
            out.push(PanelWarning {
                kind: PanelWarningKind::Unused,
                variable: Some(id.clone()),
                message: format!(
                    "variable `--{id}` is declared but never bound by `var(--{id})`; the declaration has no effect"
                ),
                fix: format!(
                    "bind it with `var(--{id})` in text, media, or an ordinary attribute, or remove the declaration"
                ),
            });
        }
    }
    for (id, locations) in &by_variable {
        if !declared.contains(id) {
            let mut sites = locations.clone();
            sites.sort();
            sites.dedup();
            out.push(PanelWarning {
                kind: PanelWarningKind::Undeclared,
                variable: Some(id.clone()),
                message: format!(
                    "`var(--{id})` at {} names no declared variable; the reference matches nothing",
                    sites.join(", ")
                ),
                fix: format!(
                    "declare `variable id=\"{id}\" type=\"...\"`, or remove the reference"
                ),
            });
        }
    }
    for warning in session.override_warnings() {
        // Only keys naming no declared variable: unused
        // declarations already list above as unused.
        if declared.contains(&warning.key) {
            continue;
        }
        out.push(PanelWarning {
            kind: PanelWarningKind::Undeclared,
            variable: Some(warning.key.clone()),
            message: warning.message.clone(),
            fix: warning.fix.clone(),
        });
    }
    for offender in session.validate_variables() {
        // Staged advisories already list above as unused, and
        // strict disk-unused already lists as unused; only other
        // disk-layer rules warn as invalid from here.
        if offender.file == sdk::STAGED_DECLARATIONS_FILE
            || offender.file == sdk::STAGED_OVERRIDES_FILE
            || offender.message.starts_with(sdk::STRICT_UNUSED_MARKER)
        {
            continue;
        }
        out.push(PanelWarning {
            kind: PanelWarningKind::Invalid,
            variable: None,
            message: format!(
                "{} {}:{}:{} {}",
                offender.rule, offender.file, offender.line, offender.column, offender.message
            ),
            fix: offender.fix.clone(),
        });
    }
    out.sort_by(|a, b| {
        (a.kind.order(), &a.variable, &a.message).cmp(&(b.kind.order(), &b.variable, &b.message))
    });
    out
}

/// Unused and undeclared warnings listed on the panel, sorted:
/// every declared-but-never-bound id, every reference and
/// override key naming no declared id, and every failed SDK rule.
/// Read-only, never writes.
pub fn panel_warnings(
    folder: &Path,
    composition: Option<&str>,
) -> Result<Vec<PanelWarning>, String> {
    check_reachable(folder)?;
    let session = open_read_session(folder, composition, BTreeMap::new(), true)?;
    Ok(warnings_for(&session))
}

/// One declared default the preview read back untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewVariable {
    /// Declared id, without the `--` prefix.
    pub id: String,
    /// Canonical type name (`string`, `number`, ...).
    pub var_type: String,
    /// Canonical default text when the declaration carries one.
    pub default: Option<String>,
}

/// What one override preview staged in memory and dropped: the
/// declared defaults read back untouched, the applied ids in
/// sorted order, the exact values text, and the warnings. Nothing
/// was written anywhere — the staging session dropped without
/// persisting, so declared defaults and project files stay
/// exactly as they were.
#[derive(Debug, Clone)]
pub struct VariablePreview {
    /// Open composition name.
    pub composition: String,
    /// Declared defaults, in session order, read back untouched.
    pub defaults: Vec<PreviewVariable>,
    /// Applied override ids, in sorted order.
    pub applied: Vec<String>,
    /// Exact values text of the applied overrides (see
    /// [`overrides_json`]).
    pub values: String,
    /// Unknown-key warnings: reported, never silently created.
    pub warnings: Vec<PanelWarning>,
}

/// Preview one override delta against the declared defaults
/// (Sprint 18, T005): the delta stages in memory through the SDK
/// exactly like [`sdk::SdkSession::apply_variable_overrides`],
/// then the session drops without persisting — declared defaults
/// stay untouched and nothing is written anywhere. Unknown keys
/// warn (reported, never created); mistyped values refuse the
/// whole preview with [`sdk::E_SDK_BAD_VARIABLE`], staging
/// nothing.
pub fn preview_variable_overrides(
    folder: &Path,
    composition: Option<&str>,
    delta: validate::validate::VariableOverrides,
) -> Result<VariablePreview, String> {
    check_reachable(folder)?;
    // The clean read open reports the declared defaults untouched
    // and the declared id set; it creates nothing.
    let reader = open_read_session(folder, composition, BTreeMap::new(), true)?;
    let composition_name = reader.effective_composition().name.clone();
    let defaults: Vec<PreviewVariable> = reader
        .variable_declarations()
        .iter()
        .map(|decl| PreviewVariable {
            id: decl.id.clone(),
            var_type: decl.var_type.name().to_string(),
            default: default_text(decl),
        })
        .collect();
    let declared = declared_ids(&reader);
    drop(reader);
    // The staging open type-checks every known key through the
    // SDK coercion first (font try-order kept, images verified)
    // and then drops without persisting: mistyped values refuse
    // the whole preview with `E_SDK_BAD_VARIABLE`, staging
    // nothing — deterministically, before any merge runs.
    let mut known = validate::validate::VariableOverrides::new();
    for (key, value) in &delta {
        if declared.contains(key) {
            known.insert(key.clone(), value.clone());
        }
    }
    let mut stager = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            history: false,
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused_variables(format!(
            "the project cannot open for variables: {e} Nothing was staged, applied, or recorded."
        ))
    })?;
    let applied = stager
        .apply_variable_overrides(known)
        .map_err(|e| e.to_string())?
        .applied;
    drop(stager);
    // The coalesced read open merges the delta into the resolved
    // composition in memory only (read-only: creates nothing) and
    // surfaces unknown keys through the exact `check` warnings:
    // reported, never silently created.
    let merger = open_read_session(folder, composition, delta.clone(), true)?;
    let mut warnings = Vec::new();
    for warning in merger.override_warnings() {
        // Only the delta's own unknown keys: project-level unused
        // declarations already list on the panel warnings.
        if !delta.contains_key(&warning.key) {
            continue;
        }
        warnings.push(PanelWarning {
            kind: PanelWarningKind::Undeclared,
            variable: Some(warning.key.clone()),
            message: warning.message.clone(),
            fix: warning.fix.clone(),
        });
    }
    drop(merger);
    let values = overrides_json(&delta_filtered(&delta, &applied));
    warnings.sort_by(|a, b| {
        (a.kind.order(), &a.variable, &a.message).cmp(&(b.kind.order(), &b.variable, &b.message))
    });
    Ok(VariablePreview {
        composition: composition_name,
        defaults,
        applied,
        values,
        warnings,
    })
}

/// The applied subset of one preview delta, in map order.
fn delta_filtered(
    delta: &validate::validate::VariableOverrides,
    applied: &[String],
) -> validate::validate::VariableOverrides {
    let mut out = validate::validate::VariableOverrides::new();
    for id in applied {
        if let Some(value) = delta.get(id) {
            out.insert(id.clone(), value.clone());
        }
    }
    out
}

/// Render one override preview as JSON for the panel API: the
/// untouched defaults, the applied ids, the exact values text as
/// an object, and the warnings.
pub fn preview_json(preview: &VariablePreview) -> String {
    let defaults = preview
        .defaults
        .iter()
        .map(|d| {
            let default = match &d.default {
                Some(text) => format!("\"{}\"", crate::studio::json_escape(text)),
                None => "null".to_string(),
            };
            format!(
                "{{\"id\":\"{}\",\"type\":\"{}\",\"default\":{}}}",
                crate::studio::json_escape(&d.id),
                crate::studio::json_escape(&d.var_type),
                default
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let applied = preview
        .applied
        .iter()
        .map(|id| format!("\"{}\"", crate::studio::json_escape(id)))
        .collect::<Vec<_>>()
        .join(",");
    let warnings = preview
        .warnings
        .iter()
        .map(warning_json_inner)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"composition\":\"{}\",\"defaults\":[{}],\"applied\":[{}],\"values\":{},\"warnings\":[{}]}}",
        crate::studio::json_escape(&preview.composition),
        defaults,
        applied,
        preview.values,
        warnings
    )
}

/// One applied declaration turn: the SDK read-back, the recorded
/// history turn, and the advisory warnings the persist returned
/// (unused declarations persist as warnings, never refusals).
#[derive(Debug, Clone)]
pub struct DeclaredReceipt {
    /// Declared variable read back (canonical type and default).
    pub declared: sdk::DeclaredVariable,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary of the turn.
    pub summary: String,
    /// Advisory warnings from the persist, in display order.
    pub warnings: Vec<sdk::VariableOffender>,
}

/// Declare one variable through the panel (Sprint 18, T005): the
/// declaration validates exactly like
/// [`sdk::SdkSession::declare_variable`] (id syntax, known type,
/// options shape, type-checked default with font try-order kept
/// and image presence verified), persists through the SDK delta
/// files, and records one guarded history turn — so `undo_turn`
/// reverses the declare whole and one undo system keeps serving
/// everything. Failed rules refuse with `E_SDK_BAD_VARIABLE`
/// staging nothing; unused declarations persist with an advisory
/// warning listed on the panel, never a refusal.
pub fn declare_panel_variable(
    folder: &Path,
    composition: Option<&str>,
    actor: &str,
    input: sdk::VariableInput,
) -> Result<DeclaredReceipt, String> {
    check_reachable(folder)?;
    let actor = check_actor(actor)?;
    let mut session = open_panel_session(folder, composition)?;
    let declared = session.declare_variable(input).map_err(|e| e.to_string())?;
    persist_declaration_turn(folder, &actor, &mut session, declared, "declare")
}

/// Edit one staged declaration through the panel: the rewrite
/// validates exactly like a declare, persists through the SDK
/// delta files, and records one guarded history turn. A refused
/// rewrite names its rule and changes nothing; an id with no
/// staged record refuses (on-disk declarations live in their KDL
/// source, unknown ids name `E_SDK_UNKNOWN_VARIABLE`).
pub fn edit_panel_variable(
    folder: &Path,
    composition: Option<&str>,
    actor: &str,
    id: &str,
    input: sdk::VariableInput,
) -> Result<DeclaredReceipt, String> {
    check_reachable(folder)?;
    let actor = check_actor(actor)?;
    let mut session = open_panel_session(folder, composition)?;
    let declared = session
        .edit_variable(id, input)
        .map_err(|e| e.to_string())?;
    persist_declaration_turn(folder, &actor, &mut session, declared, "edit")
}

/// The shared blocked-persistence refusal, mirroring the SDK
/// invalid-variables text: validation reports `offenders`, so
/// nothing was saved — no partial save.
fn invalid_block_message(offenders: &[sdk::VariableOffender]) -> String {
    let mut message = format!(
        "[{}] sdk refused: variable validation reports {} offender(s); persistence is blocked with nothing saved.",
        sdk::E_SDK_INVALID_VARIABLES,
        offenders.len()
    );
    for o in offenders {
        message.push_str(&format!(
            "\n{} {}:{}:{} {}",
            o.rule, o.file, o.line, o.column, o.message
        ));
    }
    message
}

/// Persist one staged declaration change plus its guarded history
/// turn: blocking offenders refuse first with the SDK text
/// writing nothing; the lenient SDK persist then writes and
/// returns soft plus disk-health advisories as panel warnings;
/// the turn wraps the write whole so undo reverses it. History
/// failures report verbatim.
fn persist_declaration_turn(
    folder: &Path,
    actor: &str,
    session: &mut sdk::SdkSession,
    declared: sdk::DeclaredVariable,
    verb: &str,
) -> Result<DeclaredReceipt, String> {
    let blocking = session.variable_blocking_offenders();
    if !blocking.is_empty() {
        return Err(invalid_block_message(&blocking));
    }
    let slot: std::cell::RefCell<Option<sdk::VariablePersistOutcome>> =
        std::cell::RefCell::new(None);
    let record = sdk::record_file_turn(folder, actor, || {
        let outcome = session
            .persist_variables_with_warnings()
            .map_err(|e| e.to_string())?;
        let summary = format!(
            "{} variable `--{}` (type `{}`)",
            verb, declared.id, declared.var_type
        );
        let targets = vec![format!("variable `--{}`", declared.id)];
        *slot.borrow_mut() = Some(outcome);
        Ok((summary, targets))
    })
    .map_err(|h| h.to_string())?;
    let outcome = slot.borrow().clone().expect("work stores the outcome");
    let summary = format!(
        "{} variable `--{}` (type `{}`)",
        verb, declared.id, declared.var_type
    );
    Ok(DeclaredReceipt {
        declared,
        turn: record.seq,
        actor: record.actor,
        summary,
        warnings: outcome.warnings,
    })
}

/// One applied removal turn.
#[derive(Debug, Clone)]
pub struct RemovedReceipt {
    /// Removed variable id, without the `--` prefix.
    pub id: String,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary of the turn.
    pub summary: String,
}

/// Remove one staged declaration through the panel: a variable
/// still referenced by `var(--id)` — in the project sources or
/// bound through the SDK delta — refuses with `E_SDK_BAD_VARIABLE`
/// listing every reference, removing nothing. An unreferenced
/// staged declaration drops, persists through the SDK delta
/// files, and records one guarded history turn. An id with no
/// staged record refuses (on-disk declarations live in their KDL
/// source, unknown ids name `E_SDK_UNKNOWN_VARIABLE`).
pub fn remove_panel_variable(
    folder: &Path,
    composition: Option<&str>,
    actor: &str,
    id: &str,
) -> Result<RemovedReceipt, String> {
    check_reachable(folder)?;
    let actor = check_actor(actor)?;
    let mut session = open_panel_session(folder, composition)?;
    // The full usage listing first: source-layer references plus
    // references the panel bound through the SDK delta. Any hit
    // refuses by name with every site listed — for staged and
    // on-disk declarations alike, before anything stages.
    let uses = all_references(&session, Some(id));
    if !uses.is_empty() {
        let list = uses
            .iter()
            .map(|u| format!("{} (`{}`)", u.location, u.attribute))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(refused_variables(format!(
            "variable `--{id}` is still in use by {list}; unbind every reference first, then remove it. Nothing was staged, applied, or recorded."
        )));
    }
    let removed = session.remove_variable(id).map_err(|e| e.to_string())?;
    let blocking = session.variable_blocking_offenders();
    if !blocking.is_empty() {
        return Err(invalid_block_message(&blocking));
    }
    let removed_id = removed.id.clone();
    let record = sdk::record_file_turn(folder, &actor, || {
        session
            .persist_variables_with_warnings()
            .map_err(|e| e.to_string())?;
        let summary = format!("remove variable `--{removed_id}`");
        let targets = vec![format!("variable `--{removed_id}`")];
        Ok((summary, targets))
    })
    .map_err(|h| h.to_string())?;
    let summary = format!("remove variable `--{}`", removed.id);
    Ok(RemovedReceipt {
        id: removed.id,
        turn: record.seq,
        actor: record.actor,
        summary,
    })
}

/// Render one applied declaration receipt as JSON for the panel
/// API: the read-back, its turn, and the advisory warnings.
pub fn declared_receipt_json(receipt: &DeclaredReceipt) -> String {
    let default = match &receipt.declared.default {
        Some(text) => format!("\"{}\"", crate::studio::json_escape(text)),
        None => "null".to_string(),
    };
    let warnings = receipt
        .warnings
        .iter()
        .map(|o| {
            format!(
                "{{\"rule\":\"{}\",\"file\":\"{}\",\"message\":\"{}\",\"fix\":\"{}\"}}",
                crate::studio::json_escape(&o.rule),
                crate::studio::json_escape(&o.file),
                crate::studio::json_escape(&o.message),
                crate::studio::json_escape(&o.fix)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"saved\":true,\"id\":\"{}\",\"type\":\"{}\",\"default\":{},\"turn\":{},\"actor\":\"{}\",\"summary\":\"{}\",\"warnings\":[{}]}}",
        crate::studio::json_escape(&receipt.declared.id),
        crate::studio::json_escape(&receipt.declared.var_type),
        default,
        receipt.turn,
        crate::studio::json_escape(&receipt.actor),
        crate::studio::json_escape(&receipt.summary),
        warnings
    )
}

/// Render one applied removal receipt as JSON for the panel API.
pub fn removed_receipt_json(receipt: &RemovedReceipt) -> String {
    format!(
        "{{\"saved\":true,\"removed\":\"{}\",\"turn\":{},\"actor\":\"{}\",\"summary\":\"{}\"}}",
        crate::studio::json_escape(&receipt.id),
        receipt.turn,
        crate::studio::json_escape(&receipt.actor),
        crate::studio::json_escape(&receipt.summary)
    )
}

/// Render one panel warning as JSON for the panel APIs.
fn warning_json_inner(warning: &PanelWarning) -> String {
    let variable = match &warning.variable {
        Some(id) => format!("\"{}\"", crate::studio::json_escape(id)),
        None => "null".to_string(),
    };
    format!(
        "{{\"kind\":\"{}\",\"variable\":{},\"message\":\"{}\",\"fix\":\"{}\"}}",
        warning.kind.as_str(),
        variable,
        crate::studio::json_escape(&warning.message),
        crate::studio::json_escape(&warning.fix)
    )
}

/// One parsed bind request: the named actor, the clip the
/// reference addresses, the text-carrying row, and the declared
/// variable to reference. `selection` echoes the shared canvas
/// and timeline selection the shell scoped the panel to.
#[derive(Debug, Clone)]
pub struct VariableBind {
    /// Actor the validated turn is recorded under.
    pub actor: String,
    /// Clip the reference addresses.
    pub target: sdk::ClipRef,
    /// CSS-like row key, one of [`BINDABLE_ROWS`].
    pub property: String,
    /// Declared variable id to reference, without the `--` prefix.
    pub variable: String,
    /// Shared selection the panel scopes to, echoed back.
    pub selection: Vec<String>,
}

/// Parse one bind request body (`{"actor":"maker",
/// "target":{"id":"title"},"property":"value","variable":"title"}`
/// with an optional `"selection":"a,b"`): the actor must be
/// non-empty, the target must carry an `id` or an `index`, the
/// property must name a text-carrying row ([`BINDABLE_ROWS`] —
/// typed rows hold resolved values and refuse references), and
/// the variable must be a plausible id. A refused body names its
/// cause and stages nothing.
pub fn parse_bind_body(body: &str) -> Result<VariableBind, String> {
    if body.trim().is_empty() {
        return Err(refused_variables(
            "the variables bind needs a JSON body (`{\"actor\":\"maker\",\"target\":{\"id\":\"title\"},\"property\":\"value\",\"variable\":\"title\"}`); pass the binding. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_variables(format!(
            "the variables bind body is not JSON ({e}). Pass `{{\"actor\":\"maker\",\"target\":{{\"id\":\"title\"}},\"property\":\"value\",\"variable\":\"title\"}}`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Err(refused_variables(
            "the variables bind names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
        ));
    }
    let target_value = value.get("target").ok_or_else(|| {
        refused_variables(
            "the variables bind carries no `target`; bindings address one clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged.".to_string(),
        )
    })?;
    let target: sdk::ClipRef = serde_json::from_value(target_value.clone()).map_err(|e| {
        refused_variables(format!(
            "the variables bind `target` is not a clip reference ({e}); pass `target:{{\"id\":\"...\"}}` or `target:{{\"index\":N}}`. Nothing was staged."
        ))
    })?;
    if target.id.is_none() && target.index.is_none() {
        return Err(refused_variables(
            "the variables bind `target` names neither `id` nor `index`; bindings address one clip. Pass `target:{\"id\":\"...\"}` or `target:{\"index\":N}`. Nothing was staged.".to_string(),
        ));
    }
    let property = value
        .get("property")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let property = sdk::canonical_row_property(&property);
    if property.is_empty() {
        return Err(refused_variables(
            "the variables bind names no `property`; bindings address one text-carrying row. Pass a `property` like `value`. Nothing was staged.".to_string(),
        ));
    }
    if !BINDABLE_ROWS.contains(&property.as_str()) {
        return Err(refused_variables(format!(
            "the variables bind property `{property}` carries no references; bindings write `var(--id)` onto text-carrying rows only ({}). Nothing was staged.",
            BINDABLE_ROWS.join(", ")
        )));
    }
    let variable = value
        .get("variable")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let variable = variable.strip_prefix("--").unwrap_or(&variable).to_string();
    if variable.is_empty() {
        return Err(refused_variables(
            "the variables bind names no `variable`; bindings reference one declared variable. Pass `\"variable\":\"title\"`. Nothing was staged.".to_string(),
        ));
    }
    if !valid_variable_id(&variable) {
        return Err(refused_variables(format!(
            "the variables bind variable {variable:?} is not a variable id; ids start with a letter, then hold letters, digits, `-` or `_`. Nothing was staged."
        )));
    }
    let selection = value
        .get("selection")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    Ok(VariableBind {
        actor,
        target,
        property,
        variable,
        selection,
    })
}

/// One applied bind turn.
#[derive(Debug, Clone)]
pub struct BindReceipt {
    /// Patch operations the turn recorded.
    pub applied: usize,
    /// Sequence of the recorded history turn.
    pub turn: u64,
    /// Actor the turn was recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
    /// Clip the reference addressed, echoed for selection sync.
    pub target: String,
    /// Shared selection the panel scoped to, echoed back so canvas
    /// and timeline show the same selection at once.
    pub selection: Vec<String>,
}

/// Bind one declared variable onto the selected element (Sprint
/// 18, T005): the reference `var(--id)` writes onto the row
/// through [`sdk::EditOp::SetRow`] and applies as one validated
/// turn through [`crate::studio::apply_studio_edit`] — the same
/// undo system as every other panel edit. An undeclared bind
/// target refuses naming `--id`: the reference is reported, never
/// created silently, and nothing stages, previews, or records.
pub fn apply_variable_bind(
    folder: &Path,
    composition: Option<&str>,
    bind: &VariableBind,
) -> Result<BindReceipt, String> {
    check_reachable(folder)?;
    let reader = open_read_session(folder, composition, BTreeMap::new(), true)?;
    if !declared_ids(&reader).contains(&bind.variable) {
        return Err(refused_variables(format!(
            "bind target `--{}` names no declared variable; no `variable` in the project declares it, so the reference was never created. Declare it first, then bind. Nothing was staged, applied, or recorded.",
            bind.variable
        )));
    }
    drop(reader);
    let reference = format!("var(--{})", bind.variable);
    let studio_edit = crate::studio::StudioEdit {
        actor: bind.actor.clone(),
        source: crate::studio::SaveSource::Visual,
        ops: vec![sdk::EditOp::SetRow {
            target: bind.target.clone(),
            property: bind.property.clone(),
            value: reference,
        }],
    };
    match crate::studio::apply_studio_edit(folder, composition, &studio_edit) {
        Ok(receipt) => Ok(BindReceipt {
            applied: receipt.applied,
            turn: receipt.turn,
            actor: receipt.actor,
            summaries: receipt.summaries,
            target: bind.target.to_string(),
            selection: bind.selection.clone(),
        }),
        Err(message) => Err(message),
    }
}

/// Render one applied bind receipt as JSON for the panel API:
/// the turn, its summaries, and the echoed target and selection
/// so canvas and timeline stay in sync.
pub fn bind_receipt_json(receipt: &BindReceipt) -> String {
    format!(
        "{{\"saved\":true,\"applied\":{},\"turn\":{},\"actor\":\"{}\",\"summaries\":[{}],\"target\":\"{}\",\"selection\":[{}]}}",
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
        receipt
            .selection
            .iter()
            .map(|s| format!("\"{}\"", crate::studio::json_escape(s)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Copy the exact values text of one override delta: the compact
/// sorted-key JSON the CLI `--variables` payload parses, so the
/// text pastes back byte-neutral. Pure text — never writes.
pub fn copy_variable_values(delta: &validate::validate::VariableOverrides) -> String {
    overrides_json(delta)
}

/// Quote one shell word with single quotes, escaping embedded
/// quotes, so the copied command pastes runnable.
fn shell_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// Copy a runnable render command for one override delta:
/// `framesmith render <folder> -o <output> --variables <values>`
/// with the exact values text, so the maker replays the
/// previewed overrides from a shell. The output lands beside the
/// project, never inside the project sources — like CLI renders.
/// Pure text — never writes, never renders.
pub fn copy_render_command(
    folder: &Path,
    composition: Option<&str>,
    delta: &validate::validate::VariableOverrides,
    output: Option<&str>,
) -> String {
    let root = folder.to_path_buf();
    let values = copy_variable_values(delta);
    let out: PathBuf = match output {
        Some(explicit) => PathBuf::from(explicit),
        None => {
            let stem = root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("composition");
            match root.parent() {
                Some(parent) => parent.join(format!("{stem}.mp4")),
                None => PathBuf::from(format!("{stem}.mp4")),
            }
        }
    };
    let mut command = format!(
        "framesmith render {} -o {} --variables {}",
        shell_quote(&root.to_string_lossy()),
        shell_quote(&out.to_string_lossy()),
        shell_quote(&values)
    );
    if let Some(name) = composition {
        command.push_str(&format!(" --composition {}", shell_quote(name)));
    }
    command
}

/// The full panel listing: every declared variable with its
/// canonical type, default, source layer (`disk` or `staged`),
/// bound state, and staged override value, plus the warnings.
/// Read-only, never writes.
pub fn variables_json(folder: &Path, composition: Option<&str>) -> Result<String, String> {
    check_reachable(folder)?;
    let session = open_read_session(folder, composition, BTreeMap::new(), true)?;
    let composition_name = session.effective_composition().name.clone();
    let disk = disk_declared_ids(&session);
    let merged = session.variable_overrides();
    let refs = all_references(&session, None);
    let mut bound = BTreeSet::new();
    for reference in &refs {
        bound.insert(reference.variable.clone());
    }
    let mut seen = BTreeSet::new();
    let mut entries = String::new();
    let mut push = |id: &str, decl: &notation::model::VariableDecl| {
        if !seen.insert(id.to_string()) {
            return;
        }
        let default = match default_text(decl) {
            Some(text) => format!("\"{}\"", crate::studio::json_escape(&text)),
            None => "null".to_string(),
        };
        let label = match &decl.label {
            Some(text) => format!("\"{}\"", crate::studio::json_escape(text)),
            None => "null".to_string(),
        };
        let source = if disk.contains(id) { "disk" } else { "staged" };
        let is_bound = bound.contains(id);
        let override_value = match merged.get(id) {
            Some(value) => serde_json::to_string(&override_json_value(value))
                .unwrap_or_else(|_| "null".to_string()),
            None => "null".to_string(),
        };
        entries.push_str(&format!(
            "{{\"id\":\"{}\",\"type\":\"{}\",\"default\":{},\"label\":{},\"source\":\"{}\",\"bound\":{},\"override\":{}}},",
            crate::studio::json_escape(id),
            crate::studio::json_escape(decl.var_type.name()),
            default,
            label,
            source,
            is_bound,
            override_value
        ));
    };
    for comp in session.all_compositions() {
        for decl in &comp.variables {
            push(&decl.id.clone(), decl);
        }
    }
    for decl in session.variable_declarations() {
        let id = decl.id.clone();
        push(&id, decl);
    }
    if entries.ends_with(',') {
        entries.pop();
    }
    let warnings = warnings_for(&session)
        .iter()
        .map(warning_json_inner)
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        "{{\"composition\":\"{}\",\"variables\":[{}],\"warnings\":[{}]}}",
        crate::studio::json_escape(&composition_name),
        entries,
        warnings
    ))
}

/// One parsed panel action body for `POST /api/studio/variables`:
/// `declare` and `edit` carry a variable object, `remove` and
/// `bind` carry their target, and `preview` carries the override
/// map. Reads (`values`, `command`) ride `GET` instead.
#[derive(Debug, Clone)]
pub enum VariablesAction {
    /// Declare a variable: the actor plus the declaration fields.
    Declare {
        /// Actor the turn records under.
        actor: String,
        /// Declaration fields.
        input: sdk::VariableInput,
    },
    /// Edit a staged declaration: the actor, the id, the fields.
    Edit {
        /// Actor the turn records under.
        actor: String,
        /// Staged id to rewrite.
        id: String,
        /// New declaration fields.
        input: sdk::VariableInput,
    },
    /// Remove a staged declaration: the actor and the id.
    Remove {
        /// Actor the turn records under.
        actor: String,
        /// Staged id to drop.
        id: String,
    },
    /// Bind a variable onto the selected element.
    Bind(VariableBind),
    /// Preview an override delta against the defaults.
    Preview {
        /// Override values, parsed exactly like the CLI payload.
        delta: validate::validate::VariableOverrides,
    },
}

/// Read one variable object (`{"id":..,"type":..}` with optional
/// `default`, `label`, `options`) into SDK input. A missing `id`
/// or `type` refuses naming the field; validation itself runs in
/// the SDK, never here.
fn parse_variable_input(value: &serde_json::Value) -> Result<sdk::VariableInput, String> {
    let object = match value.get("variable") {
        Some(nested) => nested,
        None => value,
    };
    let id = object
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() {
        return Err(refused_variables(
            "the variables action names no variable `id`; declare `{\"id\":\"title\",\"type\":\"string\"}`. Nothing was staged.".to_string(),
        ));
    }
    let var_type = object
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if var_type.is_empty() {
        return Err(refused_variables(format!(
            "the variables action for `--{id}` names no `type`; declare one of `string`, `number`, `color`, `boolean`, `enum`, `font`, or `image`. Nothing was staged."
        )));
    }
    let field = |name: &str| {
        object
            .get(name)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    Ok(sdk::VariableInput {
        id,
        var_type,
        default: field("default"),
        label: field("label"),
        options: field("options"),
    })
}

/// Parse one panel action body (`{"action":"bind", ...}`): the
/// action names `declare`, `edit`, `remove`, `bind`, or
/// `preview`. Anything else refuses naming the accepted actions;
/// the `preview` override map parses exactly like the CLI
/// `--variables` payload. A refused body names its cause and
/// stages nothing.
pub fn parse_variables_action(body: &str) -> Result<VariablesAction, String> {
    if body.trim().is_empty() {
        return Err(refused_variables(
            "the variables action needs a JSON body (`{\"action\":\"bind\",...}`); pass the panel action. Nothing was staged.".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_variables(format!(
            "the variables action body is not JSON ({e}). Pass `{{\"action\":\"bind\",...}}`. Nothing was staged."
        ))
    })?;
    let actor = value
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let action = value
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    match action.as_str() {
        "declare" => {
            if actor.is_empty() {
                return Err(refused_variables(
                    "the variables declare names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
                ));
            }
            Ok(VariablesAction::Declare {
                actor,
                input: parse_variable_input(&value)?,
            })
        }
        "edit" => {
            if actor.is_empty() {
                return Err(refused_variables(
                    "the variables edit names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
                ));
            }
            let id = value
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id.is_empty() {
                return Err(refused_variables(
                    "the variables edit names no `id`; pass the staged `\"id\"` to rewrite. Nothing was staged.".to_string(),
                ));
            }
            Ok(VariablesAction::Edit {
                actor,
                id,
                input: parse_variable_input(&value)?,
            })
        }
        "remove" => {
            if actor.is_empty() {
                return Err(refused_variables(
                    "the variables remove names no actor; persisted turns are named (human or agent). Pass a non-empty `actor`. Nothing was staged.".to_string(),
                ));
            }
            let id = value
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id.is_empty() {
                return Err(refused_variables(
                    "the variables remove names no `id`; pass the staged `\"id\"` to drop. Nothing was staged.".to_string(),
                ));
            }
            Ok(VariablesAction::Remove { actor, id })
        }
        "bind" => Ok(VariablesAction::Bind(parse_bind_body(body)?)),
        "preview" => {
            let raw = match value.get("overrides") {
                Some(map) => serde_json::to_string(map).map_err(|e| {
                    refused_variables(format!(
                        "the variables preview `overrides` do not encode ({e}). Pass a JSON object of string keys to scalar values. Nothing was staged."
                    ))
                })?,
                None => "{}".to_string(),
            };
            let delta =
                validate::validate::parse_variable_overrides(&raw).map_err(|e| {
                    refused_variables(format!(
                        "the variables preview `overrides` are not a variables map ({e}). Pass a JSON object of string keys to scalar values. Nothing was staged."
                    ))
                })?;
            Ok(VariablesAction::Preview { delta })
        }
        _ => Err(refused_variables(format!(
            "unknown variables action {action:?}; pass one of `declare`, `edit`, `remove`, `bind`, or `preview`. Nothing was staged."
        ))),
    }
}
