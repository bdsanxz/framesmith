//! Headless edit-SDK open (Sprint 15, T001 / CAP-003).
//!
//! A maker or agent opens a composition through [`SdkSession::open`]
//! with persist, preview, overrides, history, and coalesce options. A
//! valid composition opens ready ([`State::Ready`]) with its first
//! frame addressable and controls enabled; a broken composition is
//! refused before anything loads, naming the cause with rule and
//! location exactly like `check`.
//!
//! Option contract (each option takes observable effect; later slices
//! build on the stored choice):
//!
//! * `persist` (default true): the session may write. When false the
//!   session is read-only — [`SdkSession::can_persist`] is false, no
//!   directory is created, and later slices refuse writes with a
//!   reason code.
//! * `preview` (default false): dry-run open. Validation and override
//!   resolution still run and the effective composition is exposed,
//!   but nothing is written to disk — not even the history directory —
//!   so the effect of overrides previews before anything persists.
//!   Preview implies read-only ([`SdkSession::can_persist`] is false).
//! * `overrides`: variable override deltas, applied in memory only and
//!   never written to disk. With `coalesce` they merge into the open
//!   (see below); unknown keys are reported through
//!   [`SdkSession::override_warnings`], never silently created.
//! * `history` (default true): the session records into the project's
//!   hidden history directory ([`HISTORY_DIR_NAME`]/[`HISTORY_SUBDIR`]).
//!   Opening ensures that directory exists — the only write an open
//!   ever performs inside the project. When false (or when the session
//!   is read-only) nothing is created and
//!   [`SdkSession::history_available`] is false.
//! * `coalesce` (default true): how `overrides` join the open. When
//!   true they merge into the resolved composition in one pass, so the
//!   effective composition carries the override values and validation
//!   sees them. When false they stay staged: validation sees the
//!   on-disk values, [`SdkSession::pending_overrides`] holds the map,
//!   and a later slice applies it. The same overrides can therefore
//!   validate (coalesced) or refuse (staged), which is the observable
//!   difference.
//!
//! Refusal parity: opening validates exactly like `check`
//! ([`validate::validate::validate_folder_with_variables_strict_themed`]
//! in default variable mode), so every refusal carries the same rule
//! (`E_*` code) and location (file, line, column) the maker fixes. A
//! refused open creates nothing and changes nothing: no history
//! directory, no composition change, no history change.
//!
//! Rendering boundary: opening never renders. This crate does not
//! depend on the render pipeline, never looks for FFmpeg, and never
//! writes render output (no MP4, no PNG) anywhere. Frame
//! addressability ([`SdkSession::first_frame`]) is pure metadata —
//! frame count from the validated duration and rate — not pixels.
//!
//! Font boundary: `font-family` stays a CSS-like hyphenated,
//! comma-separated try-order list end to end. The SDK preserves the
//! authored order in the effective composition and never resolves,
//! substitutes, or reorders families; unknown families behave exactly
//! as `check` treats them (syntax-checked here, fail-closed at render
//! time), so the open never guesses a face silently.
//!
//! Later slices layer on this core: patches with inverses (T002) apply
//! to the open composition, undo and redo (T003) walk them, variables
//! (T004) extend overrides, the player (T005) loads the same validated
//! composition, and history turns (T008) record into the same hidden
//! directory this open ensures.

pub mod edits;
pub mod history;
pub mod rows;
pub mod timeline;
pub mod variables;

pub use edits::{
    AppliedEdit, ClipRef, EditOp, EditPreview, EditRefusal, EditRefusalKind, KeyStopInput,
    LaneEdit, NewElement, PastedKey, PastedSet, PastedTween, Patch, PatchOffender, PersistReport,
    RedoReceipt, RevertOp, UndoEntry,
    group_name_of_label,
    UndoListing, UndoReceipt, E_SDK_BAD_VALUE, E_SDK_BROKEN_TURN, E_SDK_GUARD_HELD,
    E_SDK_NOTHING_TO_REDO, E_SDK_NOTHING_TO_UNDO, E_SDK_PREVIEW, E_SDK_READ_ONLY,
    E_SDK_UNKNOWN_TARGET, E_SDK_UNSUPPORTED, E_SDK_UNWRITABLE, MAX_UNDO_ENTRIES,
    SDK_OVERRIDES_FILE, SDK_OVERRIDES_VERSION,
};
pub use rows::{
    canonical_row_property, row_availability, row_default_value, row_live_value, RowAvailability,
    RowDefault, ACCEPTED_ROWS, DEFERRED_ROWS, ROW_MILESTONE_3D, ROW_MILESTONE_FILTER,
};
pub use history::{
    BeginOptions, HistoryListing, HistoryRefusal, HistoryRefusalKind, RestoreReceipt, TurnDetail,
    TurnHandle, TurnReceipt, TurnRecord, TurnSummary, UndoTurnReceipt, E_SDK_HISTORY_UNAVAILABLE,
    E_SDK_HISTORY_UNWRITABLE, E_SDK_MISSING_BACKUP, E_SDK_MISSING_TARGET, E_SDK_NO_TURN,
    E_SDK_NO_TURN_IN_EFFECT, record_file_turn,
};
pub use timeline::{
    format_rows_human, parse_set_property, parse_time_expr, rows_of, SetProperty, TimeExpr,
    TimelinePlan, TimelineRow, TimelineVerb, VerbReceipt, VerbUndoReceipt,
};
pub use variables::{
    DeclaredVariable, OverrideDeltaReport, StagedVariable, VariableInput, VariableOffender,
    VariablePersistOutcome, VariablePersistReport, VariableRecord, VariableRefusal,
    VariableRefusalKind, VariableUse, E_SDK_BAD_VARIABLE, E_SDK_INVALID_VARIABLES,
    E_SDK_UNKNOWN_VARIABLE, SDK_VARIABLES_FILE, SDK_VARIABLES_VERSION,
    SDK_VARIABLE_OVERRIDES_FILE, STAGED_DECLARATIONS_FILE, STAGED_OVERRIDES_FILE,
    STRICT_DEFAULTLESS_MARKER, STRICT_UNUSED_MARKER,
};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Hidden directory inside the project owning SDK history (Sprint 15):
/// history lives here, never beside the sources and never outside the
/// project. Render, check, and validation never read it.
pub const HISTORY_DIR_NAME: &str = ".framesmith";

/// History turns live under this subdirectory of [`HISTORY_DIR_NAME`].
pub const HISTORY_SUBDIR: &str = "history";

/// Hidden history directory for one project folder.
pub fn history_dir_for(folder: &Path) -> PathBuf {
    folder.join(HISTORY_DIR_NAME).join(HISTORY_SUBDIR)
}

/// One validation diagnostic carried on an open refusal: file, line,
/// column, code, message, and fix — the same shape `check` prints, so
/// the maker fixes the same problem the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenDiagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub code: String,
    pub message: String,
    pub fix: String,
}

impl From<&validate::validate::Diagnostic> for OpenDiagnostic {
    fn from(d: &validate::validate::Diagnostic) -> Self {
        Self {
            file: d.file.clone(),
            line: d.line,
            column: d.column,
            code: d.code.to_string(),
            message: d.message.clone(),
            fix: d.fix.clone(),
        }
    }
}

/// Why a headless open refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalKind {
    /// The project is invalid: the refusal carries the diagnostics.
    Invalid,
    /// The project uses later-sprint elements: names them.
    Unsupported,
    /// The named composition is not in the project.
    UnknownComposition,
    /// The hidden history directory cannot be prepared.
    HistoryUnavailable,
}

/// A refused headless open: the kind, a human message naming the cause
/// with its fix, and the diagnostics when the project itself is at
/// fault. Nothing starts on refusal: no history directory, no session.
#[derive(Debug, Clone)]
pub struct OpenRefusal {
    pub kind: RefusalKind,
    pub message: String,
    pub diagnostics: Vec<OpenDiagnostic>,
}

impl std::fmt::Display for OpenRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Options for opening a composition headlessly (Sprint 15, T001 /
/// CAP-003). Every field takes observable effect on the session; see
/// the module docs for the contract.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// The session may write (persist turns and history). When false
    /// the session is read-only and nothing is created on disk.
    pub persist: bool,
    /// Dry-run open: validate and resolve without writing anything,
    /// even the history directory. Implies read-only.
    pub preview: bool,
    /// Variable override deltas, applied in memory only. Merged into
    /// the open when `coalesce` is true, staged when false.
    pub overrides: validate::validate::VariableOverrides,
    /// The session uses the project's hidden history directory,
    /// ensuring it exists on open (the only write an open performs).
    pub history: bool,
    /// Merge `overrides` into the resolved composition (true) or stage
    /// them untouched for a later slice to apply (false).
    pub coalesce: bool,
    /// Composition to open: the render root by default, looked up by
    /// name or id when set.
    pub composition: Option<String>,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            persist: true,
            preview: false,
            overrides: BTreeMap::new(),
            history: true,
            coalesce: true,
            composition: None,
        }
    }
}

/// Session state after [`SdkSession::open`]: only `Ready` exists in
/// this slice (closed never materializes a session; editing and
/// blocked arrive with later slices).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The composition is open and loadable.
    Ready,
}

/// Address of the first frame (Sprint 15, T001): metadata only — the
/// frame index with the canvas it draws at — never pixels. Rendering
/// stays with the render pipeline and the preview service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameAddress {
    /// Output frame index (always 0 for the first frame).
    pub index: u64,
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
}

/// An open headless composition (Sprint 15, T001): the validated
/// composition plus the resolved open options. Owns no renderer, no
/// cache, and no server; the only directory it ever creates is the
/// project's hidden history directory.
#[derive(Debug, Clone)]
pub struct SdkSession {
    folder: PathBuf,
    composition_name: String,
    composition: notation::model::Composition,
    all_compositions: Vec<notation::model::Composition>,
    frames: u64,
    canvas: (u32, u32),
    fps: notation::model::Fps,
    duration_secs: f64,
    persist: bool,
    preview: bool,
    coalesce: bool,
    history: bool,
    applied_overrides: validate::validate::VariableOverrides,
    pending_overrides: validate::validate::VariableOverrides,
    override_warnings: Vec<validate::validate::VariableWarning>,
    edits: Vec<edits::Patch>,
    redo: Vec<edits::Patch>,
    dropped: u64,
    dirty: bool,
    next_seq: u64,
    /// Delta operations replay-skipped at open because they scope to
    /// another composition (Sprint 18, T008): persisting rewrites
    /// these ahead of this session's own patches, so sibling
    /// timelines keep their turns while this one records its own.
    skipped: Vec<edits::EditOp>,
    pub(crate) variable_decls: Vec<variables::StagedVariable>,
    pub(crate) persisted_var_overrides: validate::validate::VariableOverrides,
    pub(crate) staged_var_overrides: validate::validate::VariableOverrides,
    pub(crate) last_verb_patches: usize,
    /// True while a staged declaration was removed since the last
    /// variable persist: the declaration delta rewrites even with
    /// nothing newly staged, so removals land on disk.
    pub(crate) variables_pruned: bool,
}

impl SdkSession {
    /// Open a composition headlessly (Sprint 15, T001): validate
    /// exactly like `check`, refuse broken projects before anything
    /// loads (naming rule and location), resolve overrides per
    /// `coalesce` in memory only, then — for a writable,
    /// history-enabled, non-preview open — ensure the hidden history
    /// directory. The project directory is otherwise never written and
    /// nothing is ever rendered.
    pub fn open(folder: &Path, options: OpenOptions) -> Result<Self, OpenRefusal> {
        if !folder.is_dir() {
            return Err(OpenRefusal {
                kind: RefusalKind::Invalid,
                message: format!(
                    "sdk refused: {} has no project directory; pass an existing project folder. No composition was opened.",
                    folder.display()
                ),
                diagnostics: Vec::new(),
            });
        }
        // Staged overrides stay out of validation: the open sees the
        // on-disk values and the map waits for a later slice to apply.
        // Coalesced overrides merge here, so validation (including the
        // variable binding pass with its fail-closed font lists) sees
        // the resolved values exactly like `check --variables` does.
        let effective = if options.coalesce {
            options.overrides.clone()
        } else {
            BTreeMap::new()
        };
        let verdict = validate::validate::validate_folder_with_variables_strict_themed(
            folder, &effective, false, None,
        );
        match verdict.status {
            validate::validate::Status::Invalid => {
                let mut message = format!(
                    "sdk refused: {} is invalid; fix the problems `check` reports and re-open. No composition was opened.",
                    folder.display()
                );
                let diagnostics: Vec<OpenDiagnostic> = verdict
                    .diagnostics
                    .iter()
                    .map(OpenDiagnostic::from)
                    .collect();
                for d in &diagnostics {
                    message.push_str(&format!(
                        "\n{}:{}:{} {} {}",
                        d.file, d.line, d.column, d.code, d.message
                    ));
                }
                return Err(OpenRefusal {
                    kind: RefusalKind::Invalid,
                    message,
                    diagnostics,
                });
            }
            validate::validate::Status::Unsupported => {
                let names: Vec<String> = verdict
                    .diagnostics
                    .iter()
                    .map(|d| d.message.clone())
                    .collect();
                return Err(OpenRefusal {
                    kind: RefusalKind::Unsupported,
                    message: format!(
                        "sdk refused: {} uses later-sprint element(s): {}. No composition was opened.",
                        folder.display(),
                        names.join("; ")
                    ),
                    diagnostics: verdict.diagnostics.iter().map(OpenDiagnostic::from).collect(),
                });
            }
            validate::validate::Status::Valid => {}
        }
        let comp = match &options.composition {
            None => verdict
                .composition
                .clone()
                .expect("valid verdict carries a composition"),
            Some(want) => verdict
                .compositions
                .iter()
                .find(|c| c.name == *want || c.id.as_deref() == Some(want))
                .cloned()
                .ok_or_else(|| {
                    let mut available: Vec<String> = verdict
                        .compositions
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    available.sort();
                    OpenRefusal {
                        kind: RefusalKind::UnknownComposition,
                        message: format!(
                            "sdk refused: unknown composition {want:?}: the project holds {}; pass one of those names. No composition was opened.",
                            available
                                .iter()
                                .map(|n| format!("{n:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        diagnostics: Vec::new(),
                    }
                })?,
        };
        // Replay the persisted SDK edit delta (T002): the file parses
        // before anything is created and the operations apply to the
        // resolved composition before the session exists, so a broken
        // delta refuses exactly like a broken project — creating
        // nothing, changing nothing.
        let delta_path = edits::sdk_overrides_path_for(folder);
        let delta_ops = edits::read_overrides_file(folder)?;
        let mut comp = comp;
        let mut replayed: Vec<edits::Patch> = Vec::new();
        let mut skipped: Vec<edits::EditOp> = Vec::new();
        for (i, op) in delta_ops.iter().enumerate() {
            // Composition-scoped operations (Sprint 18, T008
            // clipboard pastes and cuts) replay only while their own
            // timeline is open: sibling timelines skip them, keeping
            // cross-composition moves from leaking clips anywhere.
            // Skipped operations persist back ahead of this session's
            // own patches, so no timeline loses its turns.
            if let Some(scope) = edits::op_composition_scope(op) {
                if comp.name != scope && comp.id.as_deref() != Some(scope) {
                    skipped.push(op.clone());
                    continue;
                }
            }
            let inverse =
                edits::apply_forward(&mut comp, op).map_err(|e| OpenRefusal {
                    kind: RefusalKind::Invalid,
                    message: format!(
                        "sdk refused: the SDK override delta {} replays operation {i} ({}) but it no longer applies: {e} Remove or fix the file and re-open. No composition was opened.",
                        delta_path.display(),
                        op.summary()
                    ),
                    diagnostics: Vec::new(),
                })?;
            replayed.push(edits::Patch {
                seq: i as u64,
                op: op.clone(),
                inverse,
                persisted: true,
            });
        }
        let next_seq = replayed.len() as u64;
        // Undo walks at most MAX_UNDO_ENTRIES: a hand-grown delta
        // replaying more drops the oldest, counted like any overflow.
        let mut dropped = 0u64;
        if replayed.len() > edits::MAX_UNDO_ENTRIES {
            let excess = replayed.len() - edits::MAX_UNDO_ENTRIES;
            replayed.drain(..excess);
            dropped = excess as u64;
        }
        // Replay the persisted SDK variable deltas (T004): both files
        // parse before anything is created and every record validates
        // before the session exists, so a broken delta refuses exactly
        // like a broken project — creating nothing, changing nothing.
        let var_delta_path = variables::sdk_variables_path_for(folder);
        let var_records = variables::read_variables_file(folder)?;
        let over_delta_path = variables::sdk_variable_overrides_path_for(folder);
        let persisted_var_overrides = variables::read_variable_overrides_file(folder)?;
        let mut variable_decls: Vec<variables::StagedVariable> = Vec::new();
        {
            let mut disk_ids = std::collections::BTreeSet::new();
            for c in &verdict.compositions {
                for v in &c.variables {
                    disk_ids.insert(v.id.clone());
                }
            }
            for raw in &var_records {
                if !disk_ids.insert(raw.id.clone()) {
                    return Err(OpenRefusal {
                        kind: RefusalKind::Invalid,
                        message: format!(
                            "sdk refused: the SDK variable delta {} replays `--{}` but the project already declares it; `id` values stay unique. Remove or fix the file and re-open. No composition was opened.",
                            var_delta_path.display(),
                            raw.id
                        ),
                        diagnostics: Vec::new(),
                    });
                }
                let decl =
                    variables::record_to_decl(folder, raw).map_err(|(rule, message, _)| {
                        OpenRefusal {
                            kind: RefusalKind::Invalid,
                            message: format!(
                                "sdk refused: the SDK variable delta {} replays `--{}` but it no longer validates ({rule}): {message} Remove or fix the file and re-open. No composition was opened.",
                                var_delta_path.display(),
                                raw.id
                            ),
                            diagnostics: Vec::new(),
                        }
                    })?;
                comp.variables.push(decl);
                variable_decls.push(variables::StagedVariable {
                    raw: raw.clone(),
                    persisted: true,
                });
            }
            // Replayed override keys must name a declared variable
            // (disk or replayed), else the delta is stale.
            let declared = disk_ids;
            let mut stale: Vec<String> = persisted_var_overrides
                .keys()
                .filter(|k| !declared.contains(*k))
                .cloned()
                .collect();
            stale.sort();
            if !stale.is_empty() {
                let list = stale
                    .iter()
                    .map(|k| format!("`--{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(OpenRefusal {
                    kind: RefusalKind::Invalid,
                    message: format!(
                        "sdk refused: the SDK variable override delta {} names undeclared variable(s) {list}; no `variable` in the project declares them. Remove or fix the file and re-open. No composition was opened.",
                        over_delta_path.display()
                    ),
                    diagnostics: Vec::new(),
                });
            }
        }
        let read_only = options.preview || !options.persist;
        let history_available = options.history && !read_only;
        if history_available {
            // The only write an open ever performs inside the project:
            // the hidden history directory itself (turn files arrive
            // with the history slice). Created only after validation
            // passed, so a broken project leaves nothing behind.
            if let Err(e) = std::fs::create_dir_all(history_dir_for(folder)) {
                return Err(OpenRefusal {
                    kind: RefusalKind::HistoryUnavailable,
                    message: format!(
                        "sdk refused: the hidden history directory {} cannot be prepared: {e}; history is unavailable. No composition was opened.",
                        history_dir_for(folder).display()
                    ),
                    diagnostics: Vec::new(),
                });
            }
        }
        // Staged (non-coalesced) overrides never reach validation, so
        // their unknown keys surface here instead: reported against
        // the validated declarations, never silently created.
        let mut override_warnings = verdict.variable_warnings.clone();
        let (applied_overrides, pending_overrides) = if options.coalesce {
            (options.overrides.clone(), BTreeMap::new())
        } else {
            if !options.overrides.is_empty() {
                let mut declared = std::collections::BTreeSet::new();
                for c in &verdict.compositions {
                    for v in &c.variables {
                        declared.insert(v.id.clone());
                    }
                }
                let mut staged: Vec<String> = options
                    .overrides
                    .keys()
                    .filter(|k| !declared.contains(*k))
                    .cloned()
                    .collect();
                staged.sort();
                for key in staged {
                    override_warnings.push(validate::validate::VariableWarning {
                        key: key.clone(),
                        message: format!(
                            "overrides stage unknown variable `--{key}`; no `variable` in the project declares `--{key}`, so the staged value applies to nothing"
                        ),
                        fix: format!(
                            "remove the key from the overrides, or declare `variable id=\"{key}\" type=\"...\"` in the composition"
                        ),
                    });
                }
            }
            (BTreeMap::new(), options.overrides.clone())
        };
        let frames = comp.fps.frame_count(comp.duration);
        let canvas = (comp.width, comp.height);
        let fps = comp.fps;
        let duration_secs = comp.duration;
        Ok(Self {
            folder: folder.to_path_buf(),
            composition_name: comp.name.clone(),
            composition: comp,
            all_compositions: verdict.compositions.clone(),
            frames,
            canvas,
            fps,
            duration_secs,
            persist: options.persist,
            preview: options.preview,
            coalesce: options.coalesce,
            history: options.history,
            applied_overrides,
            pending_overrides,
            override_warnings,
            edits: replayed,
            redo: Vec::new(),
            dropped,
            dirty: false,
            next_seq,
            skipped,
            variable_decls,
            persisted_var_overrides,
            staged_var_overrides: BTreeMap::new(),
            last_verb_patches: 0,
            variables_pruned: false,
        })
    }

    /// Project folder this session opened.
    pub fn project_folder(&self) -> &Path {
        &self.folder
    }

    /// Name of the opened composition.
    pub fn composition_name(&self) -> &str {
        &self.composition_name
    }

    /// Session state: `Ready` for every session this slice opens.
    pub fn state(&self) -> State {
        State::Ready
    }

    /// The session opened successfully and the composition is loadable.
    pub fn is_ready(&self) -> bool {
        true
    }

    /// Controls are enabled on a ready session (Sprint 15, STATE-02):
    /// the player and verbs this session feeds stay interactive.
    pub fn controls_enabled(&self) -> bool {
        true
    }

    /// Output frame count of the opened composition, from the validated
    /// duration and rate (rounds up like the render pipeline, so the
    /// last partial frame still counts).
    pub fn frame_count(&self) -> u64 {
        self.frames
    }

    /// Canvas dimensions `(width, height)` of the opened composition.
    pub fn canvas(&self) -> (u32, u32) {
        self.canvas
    }

    /// Frames per second behind the opened composition.
    pub fn fps(&self) -> notation::model::Fps {
        self.fps
    }

    /// Composition duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.duration_secs
    }

    /// Address of the first frame (index 0 with the canvas): metadata
    /// only, never pixels. `Some` exactly when the composition holds
    /// at least one frame, which every valid composition does.
    pub fn first_frame(&self) -> Option<FrameAddress> {
        if self.frames == 0 {
            return None;
        }
        Some(FrameAddress {
            index: 0,
            width: self.canvas.0,
            height: self.canvas.1,
        })
    }

    /// The effective composition: validated on-disk sources with
    /// coalesced overrides resolved in (staged overrides excluded, so
    /// this reads exactly what validation accepted).
    pub fn effective_composition(&self) -> &notation::model::Composition {
        &self.composition
    }

    /// Every validated composition in document order.
    pub fn all_compositions(&self) -> &[notation::model::Composition] {
        &self.all_compositions
    }

    /// Overrides merged into this open (empty unless `coalesce`).
    pub fn applied_overrides(&self) -> &validate::validate::VariableOverrides {
        &self.applied_overrides
    }

    /// Overrides staged for a later slice to apply (empty when
    /// `coalesce`, otherwise the untouched map from the options).
    pub fn pending_overrides(&self) -> &validate::validate::VariableOverrides {
        &self.pending_overrides
    }

    /// Unknown override keys plus declared-but-never-bound variables:
    /// reported here, never silently created or dropped.
    pub fn override_warnings(&self) -> &[validate::validate::VariableWarning] {
        &self.override_warnings
    }

    /// The session may write (persist turns and history). False for a
    /// `persist: false` open and for every `preview` open.
    pub fn can_persist(&self) -> bool {
        self.persist && !self.preview
    }

    /// This open ran as a dry-run preview: resolved but wrote nothing.
    pub fn is_preview(&self) -> bool {
        self.preview
    }

    /// Overrides merged into the open (`coalesce: true`) or staged
    /// untouched (`coalesce: false`).
    pub fn coalescing(&self) -> bool {
        self.coalesce
    }

    /// History recording is available in this session: requested via
    /// `history`, writable via `persist`, and not a `preview` dry-run.
    pub fn history_available(&self) -> bool {
        self.history && self.can_persist()
    }

    /// Hidden history directory for this project (ensured on open when
    /// [`SdkSession::history_available`], otherwise only computed).
    pub fn history_dir(&self) -> PathBuf {
        history_dir_for(&self.folder)
    }

    /// SDK override delta path replayed at open and written by
    /// [`SdkSession::persist`]: `<project>/.framesmith/sdk-overrides.json`.
    pub fn overrides_delta_path(&self) -> PathBuf {
        edits::sdk_overrides_path_for(&self.folder)
    }

    /// Apply one edit operation to the open composition (Sprint 15,
    /// T002): capability and value checks run first, then the staged
    /// composition mutates and the patch records with its inverse.
    /// The effect is immediately visible through
    /// [`SdkSession::effective_composition`] (preview before
    /// persistence); nothing is written to disk. A refused edit names
    /// its reason code and changes nothing.
    ///
    /// Read-only sessions (`persist: false`) refuse every apply with
    /// [`E_SDK_READ_ONLY`]; preview sessions stage normally and refuse
    /// only at [`SdkSession::persist`].
    ///
    /// A new apply after undo discards the redo stack (redo re-applies
    /// only the undone tail); beyond [`MAX_UNDO_ENTRIES`](edits::MAX_UNDO_ENTRIES)
    /// the oldest entry drops and [`SdkSession::undo_listing`] reports it.
    pub fn apply(&mut self, op: edits::EditOp) -> Result<edits::AppliedEdit, edits::EditRefusal> {
        if !self.persist {
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::ReadOnly,
                code: edits::E_SDK_READ_ONLY,
                message: "sdk refused: the session opened read-only (`persist: false`); edits never apply. Re-open with `persist: true` to edit. Nothing was applied, previewed, or recorded.".to_string(),
            });
        }
        let inverse = edits::apply_forward(&mut self.composition, &op)?;
        let seq = self.next_seq;
        self.next_seq += 1;
        // A fresh edit branches: the undone tail no longer re-applies.
        self.redo.clear();
        self.push_undo(edits::Patch {
            seq,
            op: op.clone(),
            inverse: inverse.clone(),
            persisted: false,
        });
        self.dirty = true;
        Ok(edits::AppliedEdit { seq, op, inverse })
    }

    /// Push one patch onto the undo stack, dropping the oldest beyond
    /// [`MAX_UNDO_ENTRIES`](edits::MAX_UNDO_ENTRIES) and counting the drop.
    fn push_undo(&mut self, patch: edits::Patch) {
        if self.edits.len() >= edits::MAX_UNDO_ENTRIES {
            self.edits.remove(0);
            self.dropped += 1;
        }
        self.edits.push(patch);
    }

    /// Undo the newest applied patch (Sprint 15, T003 / CAP-004):
    /// applies its inverse, restoring the exact prior sources, and
    /// parks the patch on the redo stack. Returns a receipt naming the
    /// reversed patch. Capability checks run before anything changes:
    /// read-only sessions refuse with [`E_SDK_READ_ONLY`], an empty
    /// undo stack refuses with [`E_SDK_NOTHING_TO_UNDO`], and an
    /// inverse whose target is gone refuses naming the target while
    /// one whose backup cannot restore refuses naming the backup —
    /// either way the composition and both stacks stay exactly as
    /// they were.
    pub fn undo(&mut self) -> Result<edits::UndoReceipt, edits::EditRefusal> {
        if !self.persist {
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::ReadOnly,
                code: edits::E_SDK_READ_ONLY,
                message: "sdk refused: the session opened read-only (`persist: false`); undo never applies. Re-open with `persist: true` to edit. Nothing was reversed.".to_string(),
            });
        }
        let patch = match self.edits.pop() {
            Some(patch) => patch,
            None => {
                return Err(edits::EditRefusal {
                    kind: edits::EditRefusalKind::NothingToUndo,
                    code: edits::E_SDK_NOTHING_TO_UNDO,
                    message: format!(
                        "sdk refused: undo names no patch target: the undo stack is empty (depth 0, {} dropped beyond the bound). Nothing was reversed.",
                        self.dropped
                    ),
                });
            }
        };
        let seq = patch.seq;
        let summary = patch.op.summary();
        let inverse_summary = patch.inverse.summary();
        if let Err(e) = edits::apply_revert(&mut self.composition, &patch.inverse) {
            self.edits.push(patch);
            let why = match e.kind {
                edits::EditRefusalKind::UnknownTarget | edits::EditRefusalKind::Unsupported => {
                    "its target is missing"
                }
                _ => "its backup cannot restore",
            };
            return Err(edits::EditRefusal {
                kind: e.kind,
                code: e.code,
                message: format!(
                    "sdk refused: undo of patch {seq} ({summary}) refuses: {why} ({e}); the staged composition and the undo stack stay exactly as they were. Nothing was reversed."
                ),
            });
        }
        self.redo.push(patch);
        self.dirty = true;
        Ok(edits::UndoReceipt {
            seq,
            summary,
            inverse_summary,
            redo_available: true,
        })
    }

    /// Redo the newest undone patch (Sprint 15, T003 / CAP-004),
    /// symmetric to [`SdkSession::undo`]: re-applies the forward
    /// operation through the same capability and value checks as
    /// [`SdkSession::apply`], parks the patch back on the undo stack
    /// with its sequence number, and returns a receipt. A new apply
    /// after undo clears the redo stack, so an empty redo refuses with
    /// [`E_SDK_NOTHING_TO_REDO`]; read-only sessions refuse with
    /// [`E_SDK_READ_ONLY`]. A failed re-apply refuses naming its
    /// reason while the composition and both stacks stay unchanged.
    pub fn redo(&mut self) -> Result<edits::RedoReceipt, edits::EditRefusal> {
        if !self.persist {
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::ReadOnly,
                code: edits::E_SDK_READ_ONLY,
                message: "sdk refused: the session opened read-only (`persist: false`); redo never applies. Re-open with `persist: true` to edit. Nothing was re-applied.".to_string(),
            });
        }
        let patch = match self.redo.pop() {
            Some(patch) => patch,
            None => {
                return Err(edits::EditRefusal {
                    kind: edits::EditRefusalKind::NothingToRedo,
                    code: edits::E_SDK_NOTHING_TO_REDO,
                    message:
                        "sdk refused: redo names no patch target: the redo stack is empty. Nothing was re-applied."
                            .to_string(),
                });
            }
        };
        let seq = patch.seq;
        let summary = patch.op.summary();
        match edits::apply_forward(&mut self.composition, &patch.op) {
            Ok(inverse) => {
                let persisted = patch.persisted;
                self.push_undo(edits::Patch {
                    seq,
                    op: patch.op,
                    inverse,
                    persisted,
                });
                self.dirty = true;
                Ok(edits::RedoReceipt {
                    seq,
                    summary,
                    undo_available: true,
                })
            }
            Err(e) => {
                self.redo.push(patch);
                Err(edits::EditRefusal {
                    kind: e.kind,
                    code: e.code,
                    message: format!(
                        "sdk refused: redo of patch {seq} ({summary}) refuses: {e} The staged composition and both stacks stay exactly as they were. Nothing was re-applied."
                    ),
                })
            }
        }
    }

    /// Undo listing (Sprint 15, T003 / CAP-004): the undoable depth
    /// with the redo depth, the 100-entry bound, how many oldest
    /// entries dropped beyond it, and the report naming the drop.
    pub fn undo_listing(&self) -> edits::UndoListing {
        edits::UndoListing {
            depth: self.edits.len(),
            redo_depth: self.redo.len(),
            capacity: edits::MAX_UNDO_ENTRIES,
            dropped: self.dropped,
            drop_report: drop_report(self.dropped),
            entries: self
                .edits
                .iter()
                .map(|p| edits::UndoEntry {
                    seq: p.seq,
                    summary: p.op.summary(),
                    persisted: p.persisted,
                })
                .collect(),
        }
    }

    /// Every applied patch in apply order, each carrying its inverse.
    /// Undo and redo walking (T003) consumes these.
    pub fn patches(&self) -> &[edits::Patch] {
        &self.edits
    }

    /// Preview the staged edits (Sprint 15, T002): what would persist,
    /// with the effect already visible in
    /// [`SdkSession::effective_composition`]. Writes nothing — not
    /// even the history directory — so the maker sees the effect
    /// before anything persists.
    pub fn preview_edits(&self) -> edits::EditPreview {
        let staged: Vec<&edits::Patch> = self.edits.iter().filter(|p| !p.persisted).collect();
        edits::EditPreview {
            staged: staged.len(),
            persisted: self.edits.len() - staged.len(),
            would_write: if self.can_persist() && !staged.is_empty() {
                Some(self.overrides_delta_path())
            } else {
                None
            },
            summary: staged.iter().map(|p| p.op.summary()).collect(),
        }
    }

    /// Persist the staged edits (Sprint 15, T002): the forward log
    /// writes as the SDK override delta
    /// (`<project>/.framesmith/sdk-overrides.json`), the only write a
    /// persist performs — composition sources stay byte-identical, no
    /// render output is produced, and history turns stay with T008.
    /// The delta is replay state, not a history turn: it persists
    /// whenever the session may persist, independent of the `history`
    /// option. Persisting a clean stage writes nothing and reports
    /// zero.
    ///
    /// Broken-turn blocking (Sprint 15, T003 / REC-002): persist
    /// revalidates the project exactly like open and replays the
    /// staged patches onto the fresh base. A project that no longer
    /// validates, or a patch that no longer applies, blocks
    /// persistence with [`E_SDK_BROKEN_TURN`] naming every offender
    /// (rule with location, or patch sequence with reason) while the
    /// staged composition and the undo stack stay exactly as they
    /// were — nothing partial is saved.
    ///
    /// Preview sessions refuse with [`E_SDK_PREVIEW`], read-only
    /// sessions with [`E_SDK_READ_ONLY`]; an unwritable hidden
    /// directory refuses with [`E_SDK_UNWRITABLE`].
    pub fn persist(&mut self) -> Result<edits::PersistReport, edits::EditRefusal> {
        if self.preview {
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::Preview,
                code: edits::E_SDK_PREVIEW,
                message: "sdk refused: the session opened as a dry-run preview; previews never persist. Re-open without `preview` to persist. Nothing was applied, previewed, or recorded.".to_string(),
            });
        }
        if !self.persist {
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::ReadOnly,
                code: edits::E_SDK_READ_ONLY,
                message: "sdk refused: the session opened read-only (`persist: false`); nothing persists. Re-open with `persist: true` to persist. Nothing was applied, previewed, or recorded.".to_string(),
            });
        }
        if !self.dirty {
            return Ok(edits::PersistReport {
                applied: 0,
                path: None,
            });
        }
        // Revalidate the project exactly like open: a composition that
        // broke under the staged edits blocks with its offender list.
        let effective = if self.coalesce {
            self.applied_overrides.clone()
        } else {
            BTreeMap::new()
        };
        let verdict = validate::validate::validate_folder_with_variables_strict_themed(
            &self.folder,
            &effective,
            false,
            None,
        );
        match verdict.status {
            validate::validate::Status::Invalid | validate::validate::Status::Unsupported => {
                let mut offenders = String::new();
                for d in &verdict.diagnostics {
                    offenders.push_str(&format!(
                        "\n{}:{}:{} {} {}",
                        d.file, d.line, d.column, d.code, d.message
                    ));
                }
                return Err(edits::EditRefusal {
                    kind: edits::EditRefusalKind::BrokenTurn,
                    code: edits::E_SDK_BROKEN_TURN,
                    message: format!(
                        "sdk refused: persistence blocked: the project {} no longer validates, so nothing persists. Offenders:{offenders} The staged composition and the undo stack stay exactly as they were; fix the cause and persist again. Nothing was persisted.",
                        self.folder.display()
                    ),
                });
            }
            validate::validate::Status::Valid => {}
        }
        let mut base = match verdict
            .compositions
            .iter()
            .find(|c| c.name == self.composition_name)
            .cloned()
        {
            Some(base) => base,
            None => {
                return Err(edits::EditRefusal {
                    kind: edits::EditRefusalKind::BrokenTurn,
                    code: edits::E_SDK_BROKEN_TURN,
                    message: format!(
                        "sdk refused: persistence blocked: the selected composition {:?} is missing from the revalidated project, so nothing persists. Offender: composition {:?} names no composition in the project. The staged composition and the undo stack stay exactly as they were; fix the cause and persist again. Nothing was persisted.",
                        self.composition_name, self.composition_name
                    ),
                });
            }
        };
        // Replay every staged patch onto the fresh base: a patch that
        // no longer applies is an offender, and every offender blocks
        // while staged state is kept.
        let mut offenders: Vec<edits::PatchOffender> = Vec::new();
        for patch in &self.edits {
            if let Err(e) = edits::apply_forward(&mut base, &patch.op) {
                offenders.push(edits::PatchOffender {
                    seq: patch.seq,
                    summary: patch.op.summary(),
                    reason: e.to_string(),
                });
            }
        }
        if !offenders.is_empty() {
            let mut list = String::new();
            for o in &offenders {
                list.push_str(&format!("\npatch {} ({}): {}", o.seq, o.summary, o.reason));
            }
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::BrokenTurn,
                code: edits::E_SDK_BROKEN_TURN,
                message: format!(
                    "sdk refused: persistence blocked: {} staged patch(es) fail validation, so nothing persists. Offenders:{list} The staged composition and the undo stack stay exactly as they were; fix the cause and persist again. Nothing was persisted.",
                    offenders.len()
                ),
            });
        }
        let staged = self.edits.iter().filter(|p| !p.persisted).count();
        // Sibling-timeline operations skipped at open persist back
        // ahead of this session's own patches (Sprint 18, T008), so
        // recording one timeline never drops another's turns.
        let mut ops: Vec<edits::EditOp> = self.skipped.clone();
        ops.extend(self.edits.iter().map(|p| p.op.clone()));
        edits::write_overrides_file(&self.folder, &ops).map_err(|why| edits::EditRefusal {
            kind: edits::EditRefusalKind::Unwritable,
            code: edits::E_SDK_UNWRITABLE,
            message: format!("sdk refused: {why}; history is unavailable. Nothing was persisted."),
        })?;
        for patch in &mut self.edits {
            patch.persisted = true;
        }
        self.dirty = false;
        Ok(edits::PersistReport {
            applied: staged,
            path: Some(self.overrides_delta_path()),
        })
    }

    /// Persist the staged edits and record the turn in shared history
    /// (Sprint 15, T008 / CAP-007): the actor name, edit targets, and
    /// artifact cleanup land in the project's hidden history directory
    /// with per-turn backups, while composition sources stay
    /// byte-identical and render, check, and validation never read the
    /// record. Guard checks run before anything writes: an exclusive
    /// turn held by another writer refuses with
    /// [`E_SDK_GUARD_HELD`](edits::E_SDK_GUARD_HELD) naming the holder
    /// and the turn's age while staged work stays preserved unapplied.
    /// Preview and read-only sessions refuse exactly like
    /// [`SdkSession::persist`]; sessions without history persist the
    /// delta but record no turn (`turn` is `None`).
    pub fn persist_turn(
        &mut self,
        actor: &str,
    ) -> Result<history::TurnReceipt, edits::EditRefusal> {
        let name = actor.trim().to_string();
        if name.is_empty() {
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::BadValue,
                code: edits::E_SDK_BAD_VALUE,
                message: "sdk refused: the actor name is empty; persisted turns are named (human or agent). Pass a non-empty actor. Nothing was applied, previewed, or recorded.".to_string(),
            });
        }
        let record = self.history_available();
        if record {
            if let Err(h) = history::check_guard_for_persist(&self.folder, &name) {
                let kind = match h.kind {
                    history::HistoryRefusalKind::GuardHeld => edits::EditRefusalKind::GuardHeld,
                    _ => edits::EditRefusalKind::Unwritable,
                };
                let code = match h.kind {
                    history::HistoryRefusalKind::GuardHeld => edits::E_SDK_GUARD_HELD,
                    _ => edits::E_SDK_UNWRITABLE,
                };
                return Err(edits::EditRefusal {
                    kind,
                    code,
                    message: h.to_string(),
                });
            }
        }
        let targets: Vec<String> = self
            .edits
            .iter()
            .filter(|p| !p.persisted)
            .map(|p| p.op.summary())
            .collect();
        let before = if record {
            history::snapshot(&self.folder)
        } else {
            std::collections::BTreeMap::new()
        };
        let report = self.persist()?;
        if !record {
            return Ok(history::TurnReceipt {
                applied: report.applied,
                path: report.path,
                turn: None,
                actor: None,
            });
        }
        let after = history::snapshot(&self.folder);
        let summary = if targets.is_empty() {
            format!("persist by `{name}`: nothing staged")
        } else {
            format!("persist by `{name}`: {}", targets.join("; "))
        };
        match history::record_turn(&self.folder, &name, summary, targets, &before, &after) {
            Ok(turn) => Ok(history::TurnReceipt {
                applied: report.applied,
                path: report.path,
                turn: Some(turn.seq),
                actor: Some(turn.actor),
            }),
            Err(h) => Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::Unwritable,
                code: edits::E_SDK_UNWRITABLE,
                message: format!(
                    "{} The override delta already persisted; re-open to continue. Nothing else was recorded.",
                    h
                ),
            }),
        }
    }

    /// Apply one inverse directly to the staged composition, restoring
    /// the exact prior state without recording a new patch. Tests prove
    /// every inverse this way; undo and redo walking (T003) owns the
    /// recorded traversal on top.
    pub fn apply_revert(&mut self, revert: &edits::RevertOp) -> Result<(), edits::EditRefusal> {
        if !self.persist {
            return Err(edits::EditRefusal {
                kind: edits::EditRefusalKind::ReadOnly,
                code: edits::E_SDK_READ_ONLY,
                message: "sdk refused: the session opened read-only (`persist: false`); nothing mutates. Nothing was applied, previewed, or recorded.".to_string(),
            });
        }
        edits::apply_revert(&mut self.composition, revert)
    }
}

/// Undo drop report (Sprint 15, T003): names the drop when entries
/// fell beyond the 100-entry bound; `None` while nothing dropped.
fn drop_report(dropped: u64) -> Option<String> {
    match dropped {
        0 => None,
        1 => Some(
            "the oldest 1 patch entry dropped beyond the 100-entry bound; undo walks the newest 100 only"
                .to_string(),
        ),
        n => Some(format!(
            "the oldest {n} patch entries dropped beyond the 100-entry bound; undo walks the newest 100 only"
        )),
    }
}
