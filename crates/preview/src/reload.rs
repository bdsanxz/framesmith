//! Hot reload (Sprint 14, T005 / CAP-005, STATE-04, REC-002).
//!
//! Saving a project file while serving applies the new look within
//! 500 ms with the playhead kept; an edit that breaks validity lists
//! the diagnostics while last good frames keep showing and the
//! playhead never resets on the failed edit.
//!
//! The work itself lives on [`PreviewSession`](crate::PreviewSession):
//! [`PreviewSession::reload`](crate::PreviewSession::reload)
//! re-validates the project folder exactly like `check`, re-prepares
//! the render pipeline on success, and drops every cached frame (both
//! layers), the cached mix, and the stale last-good frame, so the next
//! request computes the new look fresh instead of serving stale.
//! A broken edit keeps everything — preparation, caches, last good,
//! mix — and reports its diagnostics. Either way the session never
//! owns the playhead, so a plain `reload` cannot move it; the page
//! calls
//! [`PreviewSession::reload_keep_playhead`](crate::PreviewSession::reload_keep_playhead)
//! with its [`Transport`](crate::Transport) instead, which rebases the
//! transport onto the new timeline (clamping a playhead that now sits
//! past the end) on success and leaves it untouched on failure.
//!
//! [`ReloadReport`] is what the page shows: whether the edit applied,
//! how long the reload took against the 500 ms bar, the kept playhead,
//! and — for a broken edit — the diagnostics list.

use std::time::Duration;

/// Hot-reload bar (Sprint 14, T005 / CAP-005): a saved edit applies
/// within this long, measured from save to the re-prepared session.
pub const RELOAD_BUDGET: Duration = Duration::from_millis(500);
/// [`RELOAD_BUDGET`] in whole milliseconds, for messages and tests.
pub const RELOAD_BUDGET_MS: u64 = 500;

/// What one hot reload did (Sprint 14, T005): applied or broken, how
/// long it took, and what the page must show next. The page renders
/// [`ReloadReport::to_string`] as-is: on success the new look plus the
/// kept playhead, on failure the diagnostics list while last good
/// frames keep showing.
#[derive(Debug, Clone)]
pub struct ReloadReport {
    /// Whether the edit validated and re-prepared: the new look is
    /// live and the next frame request draws it fresh.
    pub applied: bool,
    /// Wall time from re-validation to the re-prepared (or refused)
    /// session. Must stay within [`RELOAD_BUDGET`].
    pub elapsed: Duration,
    /// Human message naming what happened: the new look and timing on
    /// success, the cause with file, line, and fix per diagnostic on
    /// failure.
    pub message: String,
    /// Diagnostics behind a broken edit, in `check` shape (file, line,
    /// column, code, message, fix); empty on success.
    pub diagnostics: Vec<crate::PreviewDiagnostic>,
    /// Output frame count live after the reload: the new count on
    /// success, the untouched count on failure.
    pub frame_count: u64,
    /// Canvas dimensions live after the reload.
    pub canvas: (u32, u32),
    /// Playhead after the reload when the caller passed its transport
    /// to `reload_keep_playhead`: the kept (clamped into the new
    /// timeline) position on success, the untouched position on
    /// failure. `None` for a plain `reload`, which never moves any
    /// playhead by itself.
    pub playhead: Option<u64>,
}

impl ReloadReport {
    /// Whether the reload met the 500 ms bar.
    pub fn within_budget(&self) -> bool {
        self.elapsed <= RELOAD_BUDGET
    }

    /// Whole milliseconds the reload took, for messages and tests.
    pub fn elapsed_ms(&self) -> u128 {
        self.elapsed.as_millis()
    }
}

impl std::fmt::Display for ReloadReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
