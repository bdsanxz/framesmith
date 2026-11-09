//! Shared guarded history (Sprint 15, T008 / CAP-007, CAP-008, CAP-009).
//!
//! Every persisted turn is recorded with its actor name, edit targets,
//! and artifact cleanup in the project's hidden history directory
//! (`<project>/.framesmith/history/`), while render, check, and
//! validation never read it. Turns interleave by actor name so a human
//! and an agent share one record; `list`, `show`, `peek`, `pin`,
//! `begin`, and `end` expose turns with `show --diff` naming changed
//! files; whole-turn undo reverses a turn from its backup; `undo --who`
//! reverses one actor's last turn still in effect; exclusive, `who`,
//! and `if-open` guards order writers with refusals naming the holder.
//!
//! Storage layout (all inside the hidden history directory, never
//! beside the sources and never outside the project):
//!
//! * `turns.json` — the append-only turn index (versioned).
//! * `backups/<seq>/before/` and `backups/<seq>/after/` — per-turn
//!   snapshots of every tracked file (composition `*.kdl` sources plus
//!   the three hidden SDK delta files), enabling whole-turn undo
//!   (restore `before`) and restore (restore `after`).
//! * `guard.json` — the currently open turns (actor, exclusivity,
//!   start time) coordinating concurrent writers.
//!
//! Read boundary: listing works even when the current run cannot write
//! history — [`list_turns`] then reports `writes_available: false`
//! with the cause instead of refusing. Write operations refuse with a
//! reason code before anything changes; a refused writer's staged work
//! stays preserved unapplied, never discarded.
//!
//! Naming boundary: style attributes stay CSS-like hyphenated end to
//! end; history records summaries as authored and never resolves,
//! substitutes, or reorders `font-family` try-order lists.
//!
//! CLI mapping (for the `history` verbs in T009): `list` is
//! [`list_turns`], `show` is [`show_turn`] (`--diff` is
//! [`TurnDetail::diff`]), `peek` is [`peek_turn`], `pin` is
//! [`pin_turn`], `begin`/`end` are [`begin_turn`]/[`end_turn`], `undo`
//! is [`undo_turn`], `undo --who` is [`undo_who`], `restore` is
//! [`restore_turn`], and closing a turn after [`crate::SdkSession::persist`]
//! is [`crate::SdkSession::persist_turn`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Version of the persisted turn index.
pub const HISTORY_TURNS_VERSION: u32 = 1;

/// Version of the persisted guard file.
pub const HISTORY_GUARD_VERSION: u32 = 1;

/// Turn index file name inside the hidden history directory.
pub const TURNS_FILE: &str = "turns.json";

/// Guard file name inside the hidden history directory.
pub const GUARD_FILE: &str = "guard.json";

/// Per-turn backup directory name inside the hidden history directory.
pub const BACKUPS_DIR: &str = "backups";

/// Reason code: the history directory cannot be read or prepared.
pub const E_SDK_HISTORY_UNAVAILABLE: &str = "E_SDK_HISTORY_UNAVAILABLE";
/// Reason code: the history directory cannot be written.
pub const E_SDK_HISTORY_UNWRITABLE: &str = "E_SDK_HISTORY_UNWRITABLE";
/// Reason code: an exclusive turn held by another writer blocks this one.
pub use crate::edits::E_SDK_GUARD_HELD;
/// Reason code: no recorded turn answers the request.
pub const E_SDK_NO_TURN: &str = "E_SDK_NO_TURN";
/// Reason code: the named actor has no turn still in effect.
pub const E_SDK_NO_TURN_IN_EFFECT: &str = "E_SDK_NO_TURN_IN_EFFECT";
/// Reason code: the named turn or open turn is missing.
pub const E_SDK_MISSING_TARGET: &str = "E_SDK_MISSING_TARGET";
/// Reason code: the named turn's backup is missing.
pub const E_SDK_MISSING_BACKUP: &str = "E_SDK_MISSING_BACKUP";

/// Why a history operation refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRefusalKind {
    /// The history directory cannot be read or prepared.
    Unavailable,
    /// The history directory cannot be written.
    Unwritable,
    /// An exclusive turn held by another writer blocks this one.
    GuardHeld,
    /// No recorded turn answers the request.
    NoTurn,
    /// The named actor has no turn still in effect.
    NoTurnInEffect,
    /// The named turn or open turn is missing.
    MissingTarget,
    /// The named turn's backup is missing.
    MissingBackup,
    /// The actor name is empty.
    BadActor,
}

/// A refused history operation: the kind, the reason code, and a
/// message naming the cause with the fix. Nothing starts on refusal:
/// no turn recorded, no backup touched, no guard changed, and the
/// refused writer's staged work stays preserved unapplied.
#[derive(Debug, Clone)]
pub struct HistoryRefusal {
    pub kind: HistoryRefusalKind,
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for HistoryRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

fn refused(kind: HistoryRefusalKind, code: &'static str, message: String) -> HistoryRefusal {
    HistoryRefusal {
        kind,
        code,
        message,
    }
}

/// One recorded turn: who did it, what it touched, and what it
/// cleaned, with the snapshots that reverse or restore it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TurnRecord {
    /// Turn sequence number (index in the turn file, never reused).
    pub seq: u64,
    /// Actor name attached to the turn (human or agent name).
    pub actor: String,
    /// One-line human summary of the turn.
    pub summary: String,
    /// Edit targets the turn touched (patch summaries, in order).
    pub targets: Vec<String>,
    /// Files changed by the turn, relative to the project, sorted —
    /// what `show --diff` names.
    pub changed_files: Vec<String>,
    /// Stray artifacts removed from the hidden directory by this
    /// turn, relative to the project, sorted.
    pub artifacts_cleaned: Vec<String>,
    /// True while the turn is pinned against cleanup.
    pub pinned: bool,
    /// True once the turn was undone (no longer in effect).
    pub undone: bool,
    /// Unix seconds when the turn was recorded.
    pub created_unix: u64,
}

/// One turn as listed: the sequence, actor, summary, and flags.
#[derive(Debug, Clone)]
pub struct TurnSummary {
    pub seq: u64,
    pub actor: String,
    pub summary: String,
    pub pinned: bool,
    pub undone: bool,
    pub created_unix: u64,
}

impl From<&TurnRecord> for TurnSummary {
    fn from(r: &TurnRecord) -> Self {
        Self {
            seq: r.seq,
            actor: r.actor.clone(),
            summary: r.summary.clone(),
            pinned: r.pinned,
            undone: r.undone,
            created_unix: r.created_unix,
        }
    }
}

/// What [`list_turns`] reports: every turn oldest first, plus whether
/// the current run may write history. Listing never refuses over
/// writability: an unwritable history directory still lists, saying so
/// with the cause.
#[derive(Debug, Clone)]
pub struct HistoryListing {
    pub turns: Vec<TurnSummary>,
    pub writes_available: bool,
    pub write_cause: Option<String>,
}

/// What [`show_turn`] reports: the full turn with its diff.
#[derive(Debug, Clone)]
pub struct TurnDetail {
    pub record: TurnRecord,
    /// Changed files of the turn, relative to the project, sorted —
    /// the `show --diff` names.
    pub diff: Vec<String>,
}

/// One currently open turn coordinating writers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpenTurn {
    pub actor: String,
    pub exclusive: bool,
    pub started_unix: u64,
}

/// Handle returned by [`begin_turn`]: the open turn just started, or
/// the already-open turn an `--if-open` attach joined.
#[derive(Debug, Clone)]
pub struct TurnHandle {
    pub actor: String,
    pub exclusive: bool,
    pub started_unix: u64,
    /// True when the call attached to an already-open turn instead of
    /// starting a new one.
    pub attached: bool,
}

/// Receipt of one [`undo_turn`]/[`undo_who`]: the reversed turn with
/// the restored state, so the caller reads back what changed on disk.
#[derive(Debug, Clone)]
pub struct UndoTurnReceipt {
    pub seq: u64,
    pub actor: String,
    pub summary: String,
    /// Tracked files restored from the turn's backup, sorted.
    pub restored_files: Vec<String>,
    /// Names the restored state (which backup the tree now matches).
    pub restored_state: String,
}

/// Receipt of one [`restore_turn`]: the restored turn with the state
/// the tree now matches.
#[derive(Debug, Clone)]
pub struct RestoreReceipt {
    pub seq: u64,
    pub actor: String,
    pub summary: String,
    pub restored_files: Vec<String>,
    pub restored_state: String,
}

/// Receipt of one [`crate::SdkSession::persist_turn`]: the persisted
/// delta plus the recorded turn.
#[derive(Debug, Clone)]
pub struct TurnReceipt {
    /// Newly staged edits written by this persist (0 when clean).
    pub applied: usize,
    /// Delta path written, when the session rewrote the delta.
    pub path: Option<PathBuf>,
    /// Sequence of the recorded turn; `None` when the session runs
    /// without history (history disabled, read-only, or preview —
    /// the delta still persists, only the record is skipped).
    pub turn: Option<u64>,
    /// Actor the turn was recorded under; `None` when no turn was
    /// recorded.
    pub actor: Option<String>,
}

/// Options for [`begin_turn`]: exclusivity and `--if-open` attach.
#[derive(Debug, Clone, Copy)]
pub struct BeginOptions {
    /// The turn blocks other writers until ended.
    pub exclusive: bool,
    /// Attach to the already-open turn instead of starting a second.
    pub if_open: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TurnsFile {
    version: u32,
    turns: Vec<TurnRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GuardFile {
    version: u32,
    open: Vec<OpenTurn>,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Human age of an open turn (`12s`, `3m04s`, `2h05m`), so guard
/// refusals name the turn's age.
pub fn format_age(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn turns_path_for(folder: &Path) -> PathBuf {
    crate::history_dir_for(folder).join(TURNS_FILE)
}

fn guard_path_for(folder: &Path) -> PathBuf {
    crate::history_dir_for(folder).join(GUARD_FILE)
}

fn backup_dir_for(folder: &Path, seq: u64) -> PathBuf {
    crate::history_dir_for(folder)
        .join(BACKUPS_DIR)
        .join(seq.to_string())
}

/// Files a turn snapshot tracks: every composition `*.kdl` source
/// plus the three hidden SDK delta files when present. Render, check,
/// and validation never read the snapshots; only history undo and
/// restore do.
fn tracked_files(folder: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries: Vec<_> = match std::fs::read_dir(&dir) {
            Ok(it) => it.flatten().collect(),
            Err(_) => continue,
        };
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == crate::HISTORY_DIR_NAME {
                        continue;
                    }
                }
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("kdl") {
                out.push(path);
            }
        }
    }
    for name in [
        crate::edits::SDK_OVERRIDES_FILE,
        crate::variables::SDK_VARIABLES_FILE,
        crate::variables::SDK_VARIABLE_OVERRIDES_FILE,
    ] {
        let path = folder.join(crate::HISTORY_DIR_NAME).join(name);
        if path.is_file() && !out.contains(&path) {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn rel_name(folder: &Path, path: &Path) -> String {
    path.strip_prefix(folder)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// Snapshot every tracked file's bytes, keyed by project-relative name.
pub(crate) fn snapshot(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for path in tracked_files(folder) {
        if let Ok(bytes) = std::fs::read(&path) {
            out.insert(rel_name(folder, &path), bytes);
        }
    }
    out
}

/// Files whose bytes differ between two snapshots, sorted — the
/// `show --diff` names (changed video or audio files appear here
/// whenever a turn touches them; SDK turns change the hidden deltas).
fn diff_snapshots(
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) -> Vec<String> {
    let mut names: Vec<String> = before.keys().chain(after.keys()).cloned().collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter(|n| before.get(n) != after.get(n))
        .collect()
}

fn write_snapshot(dir: &Path, snap: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    for (name, bytes) in snap {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "the history backup {} cannot be prepared: {e}",
                    dir.display()
                )
            })?;
        }
        std::fs::write(&path, bytes).map_err(|e| {
            format!(
                "the history backup {} cannot be written: {e}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn read_snapshot(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, HistoryRefusal> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(top) = stack.pop() {
        let entries: Vec<_> = match std::fs::read_dir(&top) {
            Ok(it) => it.flatten().collect(),
            Err(_) => continue,
        };
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .strip_prefix(dir)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            match std::fs::read(&path) {
                Ok(bytes) => {
                    out.insert(name, bytes);
                }
                Err(e) => {
                    return Err(refused(
                        HistoryRefusalKind::MissingBackup,
                        E_SDK_MISSING_BACKUP,
                        format!(
                            "history refused: the turn backup {} cannot be read: {e}; the backup is missing. Nothing was reversed.",
                            path.display()
                        ),
                    ));
                }
            }
        }
    }
    Ok(out)
}

fn read_turns(folder: &Path) -> Result<Vec<TurnRecord>, HistoryRefusal> {
    let path = turns_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(refused(
                HistoryRefusalKind::Unavailable,
                E_SDK_HISTORY_UNAVAILABLE,
                format!(
                    "history refused: the turn index {} cannot be read: {e}; history is unavailable. Nothing changed.",
                    path.display()
                ),
            ));
        }
    };
    let file: TurnsFile = serde_json::from_str(&raw).map_err(|e| {
        refused(
            HistoryRefusalKind::Unavailable,
            E_SDK_HISTORY_UNAVAILABLE,
            format!(
                "history refused: the turn index {} is corrupt: {e}; history is unavailable. Remove or fix the file and retry. Nothing changed.",
                path.display()
            ),
        )
    })?;
    if file.version != HISTORY_TURNS_VERSION {
        return Err(refused(
            HistoryRefusalKind::Unavailable,
            E_SDK_HISTORY_UNAVAILABLE,
            format!(
                "history refused: the turn index {} carries version {}, this SDK reads version {}; history is unavailable. Remove or migrate the file and retry. Nothing changed.",
                path.display(),
                file.version,
                HISTORY_TURNS_VERSION
            ),
        ));
    }
    Ok(file.turns)
}

fn write_turns(folder: &Path, turns: &[TurnRecord]) -> Result<(), HistoryRefusal> {
    let path = turns_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused(
                HistoryRefusalKind::Unwritable,
                E_SDK_HISTORY_UNWRITABLE,
                format!(
                    "history refused: the hidden history directory {} cannot be prepared: {e}; writes are unavailable. Nothing was recorded.",
                    parent.display()
                ),
            )
        })?;
    }
    let file = TurnsFile {
        version: HISTORY_TURNS_VERSION,
        turns: turns.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|e| {
        refused(
            HistoryRefusalKind::Unwritable,
            E_SDK_HISTORY_UNWRITABLE,
            format!(
                "history refused: the turn index cannot be encoded: {e}. Nothing was recorded."
            ),
        )
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused(
            HistoryRefusalKind::Unwritable,
            E_SDK_HISTORY_UNWRITABLE,
            format!(
                "history refused: the turn index {} cannot be written: {e}; writes are unavailable. Nothing was recorded.",
                path.display()
            ),
        )
    })?;
    Ok(())
}

fn read_guard(folder: &Path) -> Result<Vec<OpenTurn>, HistoryRefusal> {
    let path = guard_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(refused(
                HistoryRefusalKind::Unavailable,
                E_SDK_HISTORY_UNAVAILABLE,
                format!(
                    "history refused: the guard file {} cannot be read: {e}; history is unavailable. Nothing changed.",
                    path.display()
                ),
            ));
        }
    };
    let file: GuardFile = serde_json::from_str(&raw).map_err(|e| {
        refused(
            HistoryRefusalKind::Unavailable,
            E_SDK_HISTORY_UNAVAILABLE,
            format!(
                "history refused: the guard file {} is corrupt: {e}; history is unavailable. Remove or fix the file and retry. Nothing changed.",
                path.display()
            ),
        )
    })?;
    if file.version != HISTORY_GUARD_VERSION {
        return Err(refused(
            HistoryRefusalKind::Unavailable,
            E_SDK_HISTORY_UNAVAILABLE,
            format!(
                "history refused: the guard file {} carries version {}, this SDK reads version {}; history is unavailable. Remove or migrate the file and retry. Nothing changed.",
                path.display(),
                file.version,
                HISTORY_GUARD_VERSION
            ),
        ));
    }
    Ok(file.open)
}

fn write_guard(folder: &Path, open: &[OpenTurn]) -> Result<(), HistoryRefusal> {
    let path = guard_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused(
                HistoryRefusalKind::Unwritable,
                E_SDK_HISTORY_UNWRITABLE,
                format!(
                    "history refused: the hidden history directory {} cannot be prepared: {e}; writes are unavailable. Nothing changed.",
                    parent.display()
                ),
            )
        })?;
    }
    let file = GuardFile {
        version: HISTORY_GUARD_VERSION,
        open: open.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|e| {
        refused(
            HistoryRefusalKind::Unwritable,
            E_SDK_HISTORY_UNWRITABLE,
            format!("history refused: the guard file cannot be encoded: {e}. Nothing changed."),
        )
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused(
            HistoryRefusalKind::Unwritable,
            E_SDK_HISTORY_UNWRITABLE,
            format!(
                "history refused: the guard file {} cannot be written: {e}; writes are unavailable. Nothing changed.",
                path.display()
            ),
        )
    })?;
    Ok(())
}

fn check_actor(actor: &str) -> Result<String, HistoryRefusal> {
    let name = actor.trim().to_string();
    if name.is_empty() {
        return Err(refused(
            HistoryRefusalKind::BadActor,
            crate::edits::E_SDK_BAD_VALUE,
            "history refused: the actor name is empty; named turns need a name (human or agent). Pass a non-empty actor. Nothing changed."
                .to_string(),
        ));
    }
    Ok(name)
}

/// Whether the current run may write history: the history directory
/// (or its nearest existing ancestor) carries a write bit. Read-only
/// on purpose — probing never creates or writes anything.
fn writes_probe(folder: &Path) -> (bool, Option<String>) {
    let mut probe = crate::history_dir_for(folder);
    loop {
        if probe.exists() {
            break;
        }
        match probe.parent() {
            Some(parent) if parent != probe => probe = parent.to_path_buf(),
            _ => {
                probe = folder.to_path_buf();
                break;
            }
        }
        if probe == *folder {
            break;
        }
    }
    if !folder.is_dir() {
        return (
            false,
            Some(format!(
                "writes are unavailable: {} has no project directory",
                folder.display()
            )),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(md) = std::fs::metadata(&probe) {
            if md.permissions().mode() & 0o222 == 0 {
                return (
                    false,
                    Some(format!(
                        "writes are unavailable: {} is not writable (mode {:o}); history lists and shows, but turns, pins, and guards refuse until it is writable again",
                        probe.display(),
                        md.permissions().mode() & 0o777
                    )),
                );
            }
        }
    }
    (true, None)
}

/// List every recorded turn oldest first (Sprint 15, T008 / CAP-007).
/// Listing never refuses over writability: an unwritable history
/// directory still lists, saying so with the cause in
/// [`HistoryListing::write_cause`].
pub fn list_turns(folder: &Path) -> Result<HistoryListing, HistoryRefusal> {
    if !folder.is_dir() {
        return Err(refused(
            HistoryRefusalKind::Unavailable,
            E_SDK_HISTORY_UNAVAILABLE,
            format!(
                "history refused: {} has no project directory; history is unavailable. Nothing changed.",
                folder.display()
            ),
        ));
    }
    let turns = read_turns(folder)?;
    let (writes_available, write_cause) = writes_probe(folder);
    Ok(HistoryListing {
        turns: turns.iter().map(TurnSummary::from).collect(),
        writes_available,
        write_cause,
    })
}

/// Show one recorded turn with its diff (Sprint 15, T008 / CAP-007):
/// the full record plus the changed files `show --diff` names.
pub fn show_turn(folder: &Path, seq: u64) -> Result<TurnDetail, HistoryRefusal> {
    let turns = read_turns(folder)?;
    match turns.iter().find(|t| t.seq == seq) {
        Some(record) => Ok(TurnDetail {
            diff: record.changed_files.clone(),
            record: record.clone(),
        }),
        None => Err(refused(
            HistoryRefusalKind::MissingTarget,
            E_SDK_MISSING_TARGET,
            format!(
                "history refused: turn {seq} names no recorded turn: the history holds {} turn(s). Pick another entry; all other turns stay intact and listed.",
                turns.len()
            ),
        )),
    }
}

/// Peek at the newest recorded turn without consuming anything
/// (Sprint 15, T008 / CAP-007): `None` while no turn is recorded.
pub fn peek_turn(folder: &Path) -> Result<Option<TurnSummary>, HistoryRefusal> {
    let turns = read_turns(folder)?;
    Ok(turns.iter().max_by_key(|t| t.seq).map(TurnSummary::from))
}

/// Pin or unpin one recorded turn (Sprint 15, T008 / CAP-007): pinned
/// turns stay named for [`restore_turn`].
pub fn pin_turn(folder: &Path, seq: u64, pinned: bool) -> Result<TurnRecord, HistoryRefusal> {
    let mut turns = read_turns(folder)?;
    match turns.iter_mut().find(|t| t.seq == seq) {
        Some(record) => {
            record.pinned = pinned;
            let out = record.clone();
            write_turns(folder, &turns)?;
            Ok(out)
        }
        None => Err(refused(
            HistoryRefusalKind::MissingTarget,
            E_SDK_MISSING_TARGET,
            format!(
                "history refused: turn {seq} names no recorded turn: the history holds {} turn(s). Nothing was pinned.",
                turns.len()
            ),
        )),
    }
}

/// Begin an open turn for `actor` (Sprint 15, T008 / CAP-009).
/// `exclusive` blocks other writers until [`end_turn`]; `if_open`
/// attaches to the already-open turn instead of starting a second.
/// A conflicting turn refuses with [`E_SDK_GUARD_HELD`] naming the
/// holder and the turn's age.
pub fn begin_turn(
    folder: &Path,
    actor: &str,
    opts: &BeginOptions,
) -> Result<TurnHandle, HistoryRefusal> {
    let name = check_actor(actor)?;
    let open = read_guard(folder)?;
    if let Some(current) = open.first() {
        if opts.if_open {
            return Ok(TurnHandle {
                actor: current.actor.clone(),
                exclusive: current.exclusive,
                started_unix: current.started_unix,
                attached: true,
            });
        }
        let age = format_age(now_unix().saturating_sub(current.started_unix));
        if current.exclusive || opts.exclusive {
            let holder = current.actor.clone();
            return Err(refused(
                HistoryRefusalKind::GuardHeld,
                E_SDK_GUARD_HELD,
                format!(
                    "history refused: an exclusive turn is held by `{holder}` (open for {age}); writer `{name}` is refused. The refused work is preserved unapplied — nothing was written or discarded. End the open turn or attach with `if-open`, then retry."
                ),
            ));
        }
    }
    let handle = TurnHandle {
        actor: name.clone(),
        exclusive: opts.exclusive,
        started_unix: now_unix(),
        attached: false,
    };
    let mut next = open;
    next.push(OpenTurn {
        actor: name,
        exclusive: opts.exclusive,
        started_unix: handle.started_unix,
    });
    write_guard(folder, &next)?;
    Ok(handle)
}

/// End `actor`'s open turn (Sprint 15, T008 / CAP-009). Ending a name
/// with no open turn refuses with [`E_SDK_NO_TURN`] and changes
/// nothing.
pub fn end_turn(folder: &Path, actor: &str) -> Result<TurnHandle, HistoryRefusal> {
    let name = check_actor(actor)?;
    let open = read_guard(folder)?;
    let mut next: Vec<OpenTurn> = open.iter().filter(|t| t.actor != name).cloned().collect();
    let ended = open.iter().find(|t| t.actor == name).cloned();
    match ended {
        Some(turn) => {
            next.retain(|t| t.actor != name);
            let _ = &mut next;
            write_guard(folder, &next)?;
            Ok(TurnHandle {
                actor: turn.actor,
                exclusive: turn.exclusive,
                started_unix: turn.started_unix,
                attached: false,
            })
        }
        None => Err(refused(
            HistoryRefusalKind::MissingTarget,
            E_SDK_NO_TURN,
            format!(
                "history refused: `{name}` holds no open turn: nothing to end. All recorded turns stay intact and listed."
            ),
        )),
    }
}

/// Whether `actor` may persist right now: an exclusive turn held by
/// another writer blocks with [`E_SDK_GUARD_HELD`] naming the holder
/// and the turn's age. Shared opens never block. Used by
/// [`crate::SdkSession::persist_turn`] before anything writes, so the
/// refused writer's staged work stays preserved unapplied.
pub fn check_guard_for_persist(folder: &Path, actor: &str) -> Result<(), HistoryRefusal> {
    let open = read_guard(folder)?;
    if let Some(current) = open.iter().find(|t| t.exclusive) {
        if current.actor != actor {
            let age = format_age(now_unix().saturating_sub(current.started_unix));
            return Err(refused(
                HistoryRefusalKind::GuardHeld,
                E_SDK_GUARD_HELD,
                format!(
                    "history refused: an exclusive turn is held by `{}` (open for {age}); writer `{actor}` is refused. The refused work is preserved unapplied — nothing was written or discarded. End the open turn or attach with `if-open`, then retry.",
                    current.actor
                ),
            ));
        }
    }
    Ok(())
}

/// Remove stray artifacts from the hidden project directory
/// (`*.tmp`, `*.bak`, `*.part` at its top level, never the turn
/// index, guard, backups, or deltas) and return their
/// project-relative names, sorted — the artifact cleanup the recorded
/// turn carries.
fn sweep_artifacts(folder: &Path) -> Vec<String> {
    let hidden = folder.join(crate::HISTORY_DIR_NAME);
    let entries: Vec<_> = match std::fs::read_dir(&hidden) {
        Ok(it) => it.flatten().collect(),
        Err(_) => return Vec::new(),
    };
    let mut cleaned = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let stray = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "tmp" | "bak" | "part"))
            .unwrap_or(false);
        if !stray {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            cleaned.push(rel_name(folder, &path));
        }
    }
    cleaned.sort();
    cleaned
}

/// Record one persisted turn: `before` must be the snapshot taken
/// before the persist and `after` the snapshot after it. Writes the
/// per-turn backups, sweeps stray artifacts, and appends the turn
/// index entry. Failures refuse with a reason code before the index
/// changes.
pub(crate) fn record_turn(
    folder: &Path,
    actor: &str,
    summary: String,
    targets: Vec<String>,
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) -> Result<TurnRecord, HistoryRefusal> {
    let name = check_actor(actor)?;
    let mut turns = read_turns(folder)?;
    let seq = turns.len() as u64;
    let changed_files = diff_snapshots(before, after);
    let backup = backup_dir_for(folder, seq);
    write_snapshot(&backup.join("before"), before).map_err(|why| {
        refused(
            HistoryRefusalKind::Unwritable,
            E_SDK_HISTORY_UNWRITABLE,
            format!("history refused: {why}; writes are unavailable. Nothing was recorded."),
        )
    })?;
    write_snapshot(&backup.join("after"), after).map_err(|why| {
        refused(
            HistoryRefusalKind::Unwritable,
            E_SDK_HISTORY_UNWRITABLE,
            format!("history refused: {why}; writes are unavailable. Nothing was recorded."),
        )
    })?;
    let artifacts_cleaned = sweep_artifacts(folder);
    let record = TurnRecord {
        seq,
        actor: name,
        summary,
        targets,
        changed_files,
        artifacts_cleaned,
        pinned: false,
        undone: false,
        created_unix: now_unix(),
    };
    turns.push(record.clone());
    write_turns(folder, &turns)?;
    Ok(record)
}

/// Record one guarded file turn around non-patch SDK state
/// (Sprints 18, code and variables panels): file writes that ride
/// no patch operation (code text saves, variable declaration
/// deltas) still record the same guarded history turn a patch
/// persist records. The guard check runs first, the tracked files
/// snapshot before and after `work`, and the turn records the
/// summary with the targets `work` returns — so `undo_turn`
/// reverses the change whole and one undo system keeps serving
/// every writer alike. `work` runs the mutation plus its
/// persistence and returns the turn summary with its targets; a
/// failed `work` records nothing, and the failure reports the
/// history reason with the wrapped cause.
pub fn record_file_turn<F>(
    folder: &Path,
    actor: &str,
    work: F,
) -> Result<TurnRecord, HistoryRefusal>
where
    F: FnOnce() -> Result<(String, Vec<String>), String>,
{
    let name = check_actor(actor)?;
    check_guard_for_persist(folder, &name)?;
    let before = snapshot(folder);
    let (summary, targets) = work().map_err(|why| {
        refused(
            HistoryRefusalKind::Unwritable,
            E_SDK_HISTORY_UNWRITABLE,
            format!("history refused: {why} Nothing was recorded."),
        )
    })?;
    let after = snapshot(folder);
    record_turn(folder, &name, summary, targets, &before, &after)
}

/// Apply one backup snapshot to the project tree: files in the
/// snapshot are written back byte-identical, tracked files absent
/// from the snapshot are removed. Every read happens before the
/// first write, so a missing backup refuses before anything changes.
fn apply_snapshot(folder: &Path, snap: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut restored: Vec<String> = snap.keys().cloned().collect();
    restored.sort();
    for (name, bytes) in snap {
        let path = folder.join(name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, bytes);
    }
    let current: BTreeMap<String, Vec<u8>> = snapshot(folder);
    for name in current.keys() {
        if !snap.contains_key(name) && is_tracked_name(folder, name) {
            let _ = std::fs::remove_file(folder.join(name));
            if !restored.contains(name) {
                restored.push(name.clone());
            }
        }
    }
    restored.sort();
    restored
}

fn is_tracked_name(folder: &Path, name: &str) -> bool {
    if name.ends_with(".kdl") && !name.starts_with(&format!("{}/", crate::HISTORY_DIR_NAME)) {
        return true;
    }
    for delta in [
        crate::edits::SDK_OVERRIDES_FILE,
        crate::variables::SDK_VARIABLES_FILE,
        crate::variables::SDK_VARIABLE_OVERRIDES_FILE,
    ] {
        if name == format!("{}/{}", crate::HISTORY_DIR_NAME, delta) {
            let _ = folder;
            return true;
        }
    }
    false
}

/// Undo one recorded turn from its backup (Sprint 15, T008 / CAP-008
/// / REC-003): the whole turn reverses in reverse order of nothing —
/// turns reverse whole, never merged — and the tree returns to the
/// turn's `before` state. Reports the restored state. Refuses when the
/// turn is missing ([`E_SDK_MISSING_TARGET`]) or its backup is
/// missing ([`E_SDK_MISSING_BACKUP`]), naming which one; either way
/// nothing is reversed and all other turns stay intact.
pub fn undo_turn(folder: &Path, seq: u64) -> Result<UndoTurnReceipt, HistoryRefusal> {
    let mut turns = read_turns(folder)?;
    let index = turns.iter().position(|t| t.seq == seq).ok_or_else(|| {
        refused(
            HistoryRefusalKind::MissingTarget,
            E_SDK_MISSING_TARGET,
            format!(
                "history refused: turn {seq} names no recorded turn: the target is missing (the history holds {} turn(s)). Pick another entry; all other turns stay intact and listed. Nothing was reversed.",
                turns.len()
            ),
        )
    })?;
    let backup = backup_dir_for(folder, seq).join("before");
    if !backup.is_dir() {
        return Err(refused(
            HistoryRefusalKind::MissingBackup,
            E_SDK_MISSING_BACKUP,
            format!(
                "history refused: turn {seq} (`{}`) has no backup: {} is missing. Nothing was reversed; all other turns stay intact and listed.",
                turns[index].summary,
                backup.display()
            ),
        ));
    }
    let snap = read_snapshot(&backup)?;
    let restored_files = apply_snapshot(folder, &snap);
    let actor = turns[index].actor.clone();
    let summary = turns[index].summary.clone();
    turns[index].undone = true;
    write_turns(folder, &turns)?;
    Ok(UndoTurnReceipt {
        seq,
        actor,
        summary,
        restored_files,
        restored_state: format!("the project tree now matches turn {seq}'s backup (before)"),
    })
}

/// Undo the named actor's last turn still in effect (Sprint 15, T008 /
/// `undo --who`): the newest turn by that name with `undone: false`
/// reverses from its backup. With no turn still in effect the refusal
/// reports that and changes nothing.
pub fn undo_who(folder: &Path, actor: &str) -> Result<UndoTurnReceipt, HistoryRefusal> {
    let name = check_actor(actor)?;
    let turns = read_turns(folder)?;
    let total = turns.len();
    let named: Vec<&TurnRecord> = turns.iter().filter(|t| t.actor == name).collect();
    match named.iter().filter(|t| !t.undone).max_by_key(|t| t.seq) {
        Some(target) => undo_turn(folder, target.seq),
        None => Err(refused(
            HistoryRefusalKind::NoTurnInEffect,
            E_SDK_NO_TURN_IN_EFFECT,
            format!(
                "history refused: `{name}` has no turn still in effect ({} turn(s) recorded, {} by `{name}`, all undone or none). Nothing was reversed; all other turns stay intact and listed.",
                total,
                named.len()
            ),
        )),
    }
}

/// Restore a pinned or named turn (Sprint 15, T008 / CAP-008): the
/// tree returns to the turn's `after` state and the turn counts as in
/// effect again. Refuses when the turn ([`E_SDK_MISSING_TARGET`]) or
/// its backup ([`E_SDK_MISSING_BACKUP`]) is missing, naming which one.
pub fn restore_turn(folder: &Path, seq: u64) -> Result<RestoreReceipt, HistoryRefusal> {
    let mut turns = read_turns(folder)?;
    let index = turns.iter().position(|t| t.seq == seq).ok_or_else(|| {
        refused(
            HistoryRefusalKind::MissingTarget,
            E_SDK_MISSING_TARGET,
            format!(
                "history refused: turn {seq} names no recorded turn: the target is missing (the history holds {} turn(s)). Pick another entry; all other turns stay intact and listed. Nothing was restored.",
                turns.len()
            ),
        )
    })?;
    let backup = backup_dir_for(folder, seq).join("after");
    if !backup.is_dir() {
        return Err(refused(
            HistoryRefusalKind::MissingBackup,
            E_SDK_MISSING_BACKUP,
            format!(
                "history refused: turn {seq} (`{}`) has no backup: {} is missing. Nothing was restored; all other turns stay intact and listed.",
                turns[index].summary,
                backup.display()
            ),
        ));
    }
    let snap = read_snapshot(&backup)?;
    let restored_files = apply_snapshot(folder, &snap);
    let actor = turns[index].actor.clone();
    let summary = turns[index].summary.clone();
    turns[index].undone = false;
    write_turns(folder, &turns)?;
    Ok(RestoreReceipt {
        seq,
        actor,
        summary,
        restored_files,
        restored_state: format!("the project tree now matches turn {seq} again (after)"),
    })
}
