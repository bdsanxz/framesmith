//! Per-source adapter readiness (Sprint 21 adapters, T007 / CAP-007,
//! STATE-01/02/03, INT-002/INT-003).
//!
//! Every adapter source reports one readiness, per source and never
//! per frame: `ready` once its bytes decoded, `loading` while its
//! bytes are still resolving, or `failed` with the cause when they
//! cannot. Validation records the entry at each adapter dispatch
//! arm, so `check` (and `lint`, which runs the same validation)
//! lists the same readiness the render path gates on: the prepare
//! stage refuses a failed source naming its file and cause before
//! the first frame, and a still-loading source blocks the render by
//! name. File-backed kinds resolve synchronously at validation, so
//! they report `ready` or `failed`; `loading` is the live state of
//! registry-backed custom sources, represented here so every report
//! spells the three states the same way.

/// Readiness of one adapter source (STATE-01/02/03): `Ready` once
/// the source decodes, `Loading` while it still resolves, or
/// `Failed` with the cause when it cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterState {
    Ready,
    Loading,
    Failed,
}

impl AdapterState {
    /// Inspector spelling (`ready`, `loading`, `failed`): the same
    /// word the render path gates on, so reports never disagree.
    pub fn as_str(&self) -> &'static str {
        match self {
            AdapterState::Ready => "ready",
            AdapterState::Loading => "loading",
            AdapterState::Failed => "failed",
        }
    }

    /// True only for [`AdapterState::Ready`].
    pub fn is_ready(&self) -> bool {
        matches!(self, AdapterState::Ready)
    }
}

impl std::fmt::Display for AdapterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One readiness entry: the adapter kind, the clip name, the source
/// it resolves, and its state with the cause when failed. Entries
/// are per source, never per frame: one entry per adapter clip no
/// matter how many frames the render draws from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterReadiness {
    /// Document index the clip would hold, for reviewable ordering.
    pub clip: usize,
    /// Clip `id` when written.
    pub id: Option<String>,
    /// Adapter kind (`lottie`, `chart`, `map`, `scene3d`).
    pub kind: &'static str,
    /// Source as written (file path, data file, pack, or a note for
    /// authored-only sources), for the human line.
    pub source: String,
    /// Resolved state of this source.
    pub state: AdapterState,
    /// Why a failed source cannot decode; `None` unless failed.
    pub cause: Option<String>,
    /// Clip location the entry sorts by.
    pub line: usize,
    pub column: usize,
}

impl AdapterReadiness {
    /// One `ready` entry for a decoded source.
    pub fn ready(
        clip: usize,
        id: Option<String>,
        kind: &'static str,
        source: impl Into<String>,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            clip,
            id,
            kind,
            source: source.into(),
            state: AdapterState::Ready,
            cause: None,
            line,
            column,
        }
    }

    /// One `loading` entry for a source still resolving.
    pub fn loading(
        clip: usize,
        id: Option<String>,
        kind: &'static str,
        source: impl Into<String>,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            clip,
            id,
            kind,
            source: source.into(),
            state: AdapterState::Loading,
            cause: None,
            line,
            column,
        }
    }

    /// One `failed` entry for a source that cannot decode, naming
    /// the cause validation refused with.
    pub fn failed(
        clip: usize,
        id: Option<String>,
        kind: &'static str,
        source: impl Into<String>,
        cause: impl Into<String>,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            clip,
            id,
            kind,
            source: source.into(),
            state: AdapterState::Failed,
            cause: Some(cause.into()),
            line,
            column,
        }
    }

    /// Display name of the source: the clip `id` when written, else
    /// the `<kind clip N>` reviewable label.
    pub fn display_name(&self) -> String {
        match &self.id {
            Some(id) => format!("{} {id:?}", self.kind),
            None => format!("<{} clip {}>", self.kind, self.clip + 1),
        }
    }
}

impl std::fmt::Display for AdapterReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.cause {
            Some(cause) => write!(
                f,
                "readiness[{}] {}: {} ({})",
                self.kind,
                self.display_name(),
                self.state,
                cause
            ),
            None => write!(
                f,
                "readiness[{}] {}: {} ({})",
                self.kind,
                self.display_name(),
                self.state,
                self.source
            ),
        }
    }
}
